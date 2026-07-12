//! Pure, GUI-free, IO-free Markdown → themed-HTML rendering for the preview pane.
//!
//! `wolf-core` owns the render (requirement 6): the front-end web view just loads
//! the HTML string this module produces. Like [`crate::rank`] this module touches
//! NO GUI toolkit and NO filesystem — it is a pure `&str` → `String` transform,
//! headlessly unit-testable. The Markdown engine is [`pulldown_cmark`], chosen for
//! its [`pulldown_cmark::Parser::into_offset_iter`], which yields each event paired
//! with its SOURCE byte range — the basis of the forward-compatible marker scheme.
//!
//! ## The marker/anchor scheme (built now; JS + cursor signal deferred)
//!
//! The emitted HTML carries invisible per-line and per-100-column position markers
//! whose ids encode a position in the ORIGINAL Markdown source, so a later
//! cursor-follow scroll can find and reveal the line the user is editing. Each
//! marker is an empty inline `<span class="wolf-src" id="wl-{L}-{C}"
//! data-line="{L}" data-col="{C}"></span>` (see [`marker_span`]): `L` is the
//! 1-based hard buffer line (matches nvim `line('.')`), `C` is the 0-based
//! CHARACTER column (OD4 — a character offset, NOT a byte offset, so multibyte
//! lines resolve correctly). The `id` gives O(1) `getElementById`; the CSS
//! `.wolf-src` rule (in `markdown/base.css`) makes it a zero-size, invisible inline
//! anchor that never affects the rendered body.
//!
//! Marker set ([`desired_markers`]): for EVERY hard line `L` in `1..=line_count`, a
//! marker `(L, 0)`; ADDITIONALLY, for any line whose character length is
//! `>= MARKER_COLUMN_STRIDE`, markers `(L, 100)`, `(L, 200)`, … for each multiple
//! of [`MARKER_COLUMN_STRIDE`] strictly less than the line's character length.
//!
//! ## DOM placement (block-level injection only)
//!
//! Markers are injected ONLY immediately before block-level `Start` tags, by
//! [`MarkerInjector`] over the offset iterator: before emitting a block-level
//! `Start(tag)` whose source range is `s..e`, every pending marker with byte offset
//! `< e` is flushed (as an `Event::Html` raw span) in source order, so a block's
//! interior line markers COLLAPSE to immediately before that block's opening tag.
//! Any markers left after the stream ends are flushed as trailing spans. Inline
//! tags are never injection points, so a marker never splits inline content and
//! never lands inside `<pre><code>` (which would corrupt rendered code).
//!
//! **Collapse consequence (v1, documented):** lines inside one block (a wrapped
//! paragraph, a fenced code block, a table) collapse to that block's start `y` —
//! every line's `wl-{L}-0` marker still EXISTS (a unique id) but resolves to the
//! block top. That is the correct scroll granularity for v1. Because a marker is
//! flushed before the block whose range CONTAINS it, a container block (e.g. a
//! `List`) collapses all of its items' interior line markers to the top of the
//! container; finer per-item anchoring is a later refinement. The per-100-column
//! markers are mostly redundant in v1 (they collapse to the same block top) but are
//! REQUIRED by the scheme and become meaningful once inline-level anchoring is
//! added.
//!
//! ## Forward-compat contract (designed, NOT built here)
//!
//! The follow-up (out of scope for this change) adds (1) a Neovim → web-view
//! cursor-position `(line, col)` signal — which must convert nvim's BYTE column to
//! a CHARACTER column before looking up `#wl-{line}-{col}` (OD4) — and (2) JS that
//! on cursor move finds `#wl-{line}-{col}` (falling back to `#wl-{line}-0`) and
//! scrolls it into view only when it nears / crosses a viewport-edge threshold.
//! None of that is implemented here; this module only guarantees the markers are
//! present in the emitted HTML from day one.

use std::ops::Range;

use pulldown_cmark::{html, Event, Options, Parser, Tag};

/// The column stride for the per-100-column markers: for any hard line whose
/// character length is `>= MARKER_COLUMN_STRIDE`, an extra marker is emitted at
/// each multiple of this value strictly less than the line's character length
/// (`100`, `200`, …). A named const so the "every 100 columns" rule is explicit
/// (no magic number).
pub const MARKER_COLUMN_STRIDE: usize = 100;

/// The structural + marker CSS shared by every preview theme (the `.wolf-src`
/// marker rule plus baseline layout). Embedded BEFORE the selected theme's CSS in
/// the rendered document. See `markdown/base.css`.
const BASE_CSS: &str = include_str!("markdown/base.css");

/// The available preview HTML/CSS themes.
///
/// One variant ships this cut ([`PreviewTheme::Default`], a fixed dark theme —
/// OD5). The enum + [`theme_css`] + `include_str!`'d CSS files are structured so
/// more themes and a config-driven selector can drop in later; selection is a
/// hardcoded `PreviewTheme::Default` at the call site for now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PreviewTheme {
    /// The default (dark) preview theme.
    #[default]
    Default,
}

/// Returns the CSS for `theme` (embedded AFTER [`BASE_CSS`] in the rendered
/// document). Each theme's CSS is `include_str!`'d from
/// `markdown/themes/<theme>.css`.
#[must_use]
pub fn theme_css(theme: PreviewTheme) -> &'static str {
    match theme {
        PreviewTheme::Default => include_str!("markdown/themes/default.css"),
    }
}

/// Renders `source` Markdown to a complete, themed HTML document with the
/// forward-compatible per-line / per-100-column position markers embedded.
///
/// The rendered Markdown body is wrapped as
/// `<!doctype html><html><head><meta charset="utf-8"><style>{BASE_CSS}{theme_css}</style></head><body class="wolf-markdown">{body}</body></html>`
/// so the front-end web view can load the string directly. See the module docs for
/// the marker scheme and its DOM placement.
#[must_use]
pub fn render_html(source: &str, theme: PreviewTheme) -> String {
    let markers = desired_markers(source);

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let offset_iter = Parser::new_ext(source, options).into_offset_iter();
    let injector = MarkerInjector::new(offset_iter, markers);

    let mut body = String::new();
    html::push_html(&mut body, injector);

    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>{BASE_CSS}{theme}</style>\
         </head><body class=\"wolf-markdown\">{body}</body></html>",
        theme = theme_css(theme),
    )
}

/// The invisible marker span for hard line `line` (1-based) and 0-based CHARACTER
/// column `col`, exactly as the scheme (and the deferred JS) expect:
/// `<span class="wolf-src" id="wl-{line}-{col}" data-line="{line}" data-col="{col}"></span>`.
/// Double-quoted `id`, with a real `</span>` close (NOT self-closing).
fn marker_span(line: u32, col: u32) -> String {
    format!(
        "<span class=\"wolf-src\" id=\"wl-{line}-{col}\" data-line=\"{line}\" data-col=\"{col}\"></span>"
    )
}

/// Computes the sorted (ascending byte offset) desired markers for `source`.
///
/// Each entry is `(byte_offset, line, col)`: `line` is the 1-based hard line,
/// `col` is the 0-based CHARACTER column, and `byte_offset` is that position's byte
/// offset into `source` (the line's start byte plus the byte length of the first
/// `col` characters of that line — OD4). Emits `(L, 0)` for every hard line, plus
/// `(L, k*MARKER_COLUMN_STRIDE)` for each `k*MARKER_COLUMN_STRIDE` strictly less
/// than the line's character length.
///
/// Hard lines are `source.split('\n')` (so a trailing newline yields a final empty
/// line), matching nvim's buffer-line count.
fn desired_markers(source: &str) -> Vec<(usize, u32, u32)> {
    let mut markers = Vec::new();
    let mut line_start = 0usize;
    for (idx, line) in source.split('\n').enumerate() {
        let line_no = (idx as u32) + 1;
        // Column-zero marker for every hard line.
        markers.push((line_start, line_no, 0));

        // Per-100-column markers for a long enough line. `char_indices().nth(col)`
        // gives the byte offset of the `col`-th CHARACTER within the line; a line
        // exactly `char_len` chars long has none at or past `char_len`, so the loop
        // stops before reaching the line end.
        let char_len = line.chars().count();
        let mut col = MARKER_COLUMN_STRIDE;
        while col < char_len {
            let byte_in_line = line
                .char_indices()
                .nth(col)
                .map_or(line.len(), |(b, _)| b);
            markers.push((line_start + byte_in_line, line_no, col as u32));
            col += MARKER_COLUMN_STRIDE;
        }

        // Advance past this line and its trailing '\n'. The extra +1 for the final
        // line (which has no '\n') is harmless: no later line references it.
        line_start += line.len() + 1;
    }
    markers
}

/// Returns whether `event` is a block-level `Start` tag — the only injection
/// point for markers. Block-level tags: `Paragraph`, `Heading`, `BlockQuote`,
/// `CodeBlock`, `HtmlBlock`, `List`, `Item`, `FootnoteDefinition`, `Table`,
/// `TableHead`, `TableRow`. Inline tags (`Emphasis`/`Strong`/`Link`/…) and
/// `TableCell` are intentionally NOT injection points, so a marker never splits
/// inline content or lands inside `<pre><code>`.
fn is_block_start(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::Start(
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::BlockQuote(..)
                | Tag::CodeBlock(..)
                | Tag::HtmlBlock
                | Tag::List(..)
                | Tag::Item
                | Tag::FootnoteDefinition(..)
                | Tag::Table(..)
                | Tag::TableHead
                | Tag::TableRow
        )
    )
}

/// An iterator adapter over [`pulldown_cmark::OffsetIter`] that injects the
/// marker spans (as raw `Event::Html`) immediately before block-level `Start`
/// tags, in source order, and flushes any remaining markers as trailing spans.
///
/// See the module docs for the placement rule and its collapse consequence.
struct MarkerInjector<'a, I> {
    inner: I,
    /// Desired markers, ascending byte offset (from [`desired_markers`]).
    markers: Vec<(usize, u32, u32)>,
    /// Index of the next un-emitted marker in `markers`.
    next_marker: usize,
    /// Buffered events to yield before pulling the next inner event (a block
    /// `Start` expands to its flushed markers followed by the `Start` itself).
    pending: std::collections::VecDeque<Event<'a>>,
    /// Set once the inner iterator is exhausted, so the trailing-marker flush runs
    /// exactly once.
    inner_done: bool,
}

impl<'a, I> MarkerInjector<'a, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    fn new(inner: I, markers: Vec<(usize, u32, u32)>) -> Self {
        Self {
            inner,
            markers,
            next_marker: 0,
            pending: std::collections::VecDeque::new(),
            inner_done: false,
        }
    }

    /// Buffers every pending marker with byte offset `< end` as a raw-HTML span,
    /// in source order.
    fn flush_before(&mut self, end: usize) {
        while self.next_marker < self.markers.len() && self.markers[self.next_marker].0 < end {
            let (_, line, col) = self.markers[self.next_marker];
            self.pending
                .push_back(Event::Html(marker_span(line, col).into()));
            self.next_marker += 1;
        }
    }
}

impl<'a, I> Iterator for MarkerInjector<'a, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Event<'a>> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some(event);
            }
            match self.inner.next() {
                Some((event, range)) => {
                    if is_block_start(&event) {
                        // Flush this block's interior (and any preceding) markers
                        // before its opening tag, then emit the tag.
                        self.flush_before(range.end);
                        self.pending.push_back(event);
                        // Loop back to drain `pending` (the flushed markers first).
                    } else {
                        return Some(event);
                    }
                }
                None => {
                    if self.inner_done {
                        return None;
                    }
                    self.inner_done = true;
                    // Flush every remaining marker as a trailing span.
                    self.flush_before(usize::MAX);
                    // Loop back to drain `pending`; if empty, the next pass returns
                    // None (inner_done is set).
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `desired_markers` emits one column-zero marker per hard line, in ascending
    /// byte order, for a simple three-line source.
    #[test]
    fn desired_markers_one_per_line() {
        let markers = desired_markers("# Title\n\nHello");
        let zero_cols: Vec<_> = markers
            .iter()
            .filter(|(_, _, c)| *c == 0)
            .map(|(_, l, _)| *l)
            .collect();
        assert_eq!(zero_cols, vec![1, 2, 3], "one wl-L-0 marker per hard line");
        // Ascending byte offset.
        let offsets: Vec<_> = markers.iter().map(|(b, _, _)| *b).collect();
        let mut sorted = offsets.clone();
        sorted.sort_unstable();
        assert_eq!(offsets, sorted, "markers must be in ascending byte order");
    }

    /// A line of exactly `MARKER_COLUMN_STRIDE` characters earns no `-100` marker
    /// (100 is not strictly `< 100`); a longer line does.
    #[test]
    fn per_hundred_marker_only_past_the_stride() {
        let at_stride = "a".repeat(MARKER_COLUMN_STRIDE);
        let markers = desired_markers(&at_stride);
        assert!(
            !markers.iter().any(|(_, _, c)| *c as usize == MARKER_COLUMN_STRIDE),
            "a line of exactly {MARKER_COLUMN_STRIDE} chars earns no -100 marker"
        );

        let past_stride = "a".repeat(MARKER_COLUMN_STRIDE + 1);
        let markers = desired_markers(&past_stride);
        assert!(
            markers.iter().any(|(_, _, c)| *c as usize == MARKER_COLUMN_STRIDE),
            "a line longer than {MARKER_COLUMN_STRIDE} chars earns a -100 marker"
        );
    }

    /// The per-100-column byte offset is a CHARACTER offset (OD4): for a line of
    /// 2-byte characters the `-100` marker's byte offset is `200`, not `100`.
    #[test]
    fn per_hundred_marker_uses_character_columns() {
        // 120 two-byte characters (U+00E9 é = 0xC3 0xA9): char_len 120 > stride.
        let line = "é".repeat(120);
        let markers = desired_markers(&line);
        let hundred = markers
            .iter()
            .find(|(_, _, c)| *c as usize == MARKER_COLUMN_STRIDE)
            .expect("a 120-char line earns a -100 marker");
        assert_eq!(
            hundred.0,
            MARKER_COLUMN_STRIDE * 2,
            "the -100 char column sits at byte 200 on a 2-byte-per-char line"
        );
    }

    /// `render_html` wraps the body and renders the Markdown.
    #[test]
    fn render_html_smoke() {
        let html = render_html("# Hi\n\ntext", PreviewTheme::Default);
        assert!(html.to_ascii_lowercase().contains("<!doctype html"));
        assert!(html.contains(r#"class="wolf-markdown""#));
        assert!(html.contains("<h1"));
        assert!(html.contains(r#"id="wl-1-0""#));
    }
}
