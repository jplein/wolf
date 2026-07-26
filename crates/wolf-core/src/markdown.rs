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

use std::collections::HashMap;
use std::ops::Range;

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};
use pulldown_latex::{
    config::{DisplayMode, RenderConfig},
    mathml::push_mathml,
    Parser as LatexParser, Storage,
};

/// The column stride for the per-100-column markers: for any hard line whose
/// character length is `>= MARKER_COLUMN_STRIDE`, an extra marker is emitted at
/// each multiple of this value strictly less than the line's character length
/// (`100`, `200`, …). A named const so the "every 100 columns" rule is explicit
/// (no magic number).
pub const MARKER_COLUMN_STRIDE: usize = 100;

/// The structural + marker CSS shared by every preview theme (the `.wolf-src`
/// marker rule plus the 24px-baseline-grid structural layout). Embedded BEFORE
/// the selected theme's CSS in the rendered document. See `markdown/base.css`
/// and `markdown/README.md`.
const BASE_CSS: &str = include_str!("markdown/base.css");

/// The Inter variable-weight `@font-face`, inlined as a base64 `data:font/woff2`
/// URI so the preview body font never depends on a host-installed font. ONE face
/// spans the full `100 900` weight range (the variable roman woff2 from
/// rsms/inter, SIL OFL 1.1). The single-line base64 lives in the committed
/// `assets/inter-variable.woff2.b64` sidecar (see `markdown/assets`); `concat!`
/// wraps it in the rule at compile time with zero runtime encoding.
const INTER_FONT_FACE: &str = concat!(
    "@font-face{font-family:'Inter';font-weight:100 900;font-style:normal;\
     font-display:swap;src:url('data:font/woff2;base64,",
    include_str!("markdown/assets/inter-variable.woff2.b64"),
    "') format('woff2')}"
);

/// The Latin Modern Math `@font-face`, inlined as a base64 `data:font/woff2` URI so
/// the MathML output is pinned to a bundled math font (carrying an OpenType `MATH`
/// table) rather than a host-installed one. The woff2 (GUST GFL) ships with
/// pulldown-latex and matches its `styles.css` family name; the single-line base64
/// lives in the committed `assets/latinmodern-math.woff2.b64` sidecar. `MATH_CSS`
/// pins all `<math>` content to this family.
const MATH_FONT_FACE: &str = concat!(
    "@font-face{font-family:'Latin Modern Math';src:url('data:font/woff2;base64,",
    include_str!("markdown/assets/latinmodern-math.woff2.b64"),
    "') format('woff2')}"
);

/// The MathML styling for pulldown-latex's `<math>` output, adapted from its
/// `styles.css` (@font-face rules dropped, `m|…` namespaced selectors rewritten to
/// plain elements, and a final `Latin Modern Math` font pin appended). Embedded
/// AFTER [`BASE_CSS`] in the rendered document. See `markdown/math.css`.
const MATH_CSS: &str = include_str!("markdown/math.css");

/// The available preview HTML/CSS themes.
///
/// One variant ships this cut ([`PreviewTheme::Default`], the default theme whose
/// CSS carries a dark palette plus a `prefers-color-scheme: light` palette so it
/// follows the app's resolved dark/light appearance — but is NOT wired to the live
/// nvim colorscheme (OD5)). The enum + [`theme_css`] + `include_str!`'d CSS files
/// are structured so more themes and a config-driven selector can drop in later;
/// selection is a hardcoded `PreviewTheme::Default` at the call site for now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PreviewTheme {
    /// The default preview theme. Its CSS ships a dark palette plus a
    /// `prefers-color-scheme: light` variant, so it follows the app's resolved
    /// dark/light appearance.
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

/// How the rendered Markdown body's content area is sized horizontally.
///
/// A per-render choice threaded into [`render_html`]. [`WidthMode::Full`] is the
/// full-bleed / historical behavior (the in-app webview FRAME does any capping);
/// [`WidthMode::Capped`] appends a `max-width` cap + horizontal centering to the
/// CSS so the standalone document (the browser preview, always-capped per OQ-3)
/// caps its own content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WidthMode {
    /// Cap the content to [`CAPPED_WIDTH_CSS`] (`max-width: 45rem`, ≈ 90 proportional
    /// Inter characters — OQ-3) and center it horizontally. Used by the always-capped
    /// browser preview and static `.md` disk renders.
    Capped,
    /// Full-bleed: no content `max-width` cap and no centering, so the caller's
    /// frame (the in-app webview) does any capping. The default and historical
    /// behavior.
    #[default]
    Full,
}

/// The HORIZONTAL-ONLY content cap appended AFTER the theme CSS for
/// [`WidthMode::Capped`]. Caps the body to `max-width: 45rem` and centers it with
/// `margin-left:auto` / `margin-right:auto`, plus symmetric horizontal padding.
///
/// `45rem` ≈ 90 proportional Inter characters: at the default 16px root font the
/// average Inter glyph advance is roughly half the em (~8px), so `45rem = 720px`
/// holds on the order of 90 characters — honoring the user's explicit "~90
/// characters" request (OQ-3) for a proportional font, the same target the 90-column
/// editor cap uses for its monospaced grid.
///
/// ONLY horizontal properties are set (`max-width`, the two side margins, the two
/// side paddings). The vertical margin/padding is deliberately left untouched: the
/// preview's 24px baseline grid (in `markdown/base.css`) depends on the body's
/// vertical box, so overriding it here would break the baseline rhythm.
const CAPPED_WIDTH_CSS: &str =
    "body.wolf-markdown{max-width:45rem;margin-left:auto;margin-right:auto;\
     padding-left:24px;padding-right:24px}";

/// Renders `source` Markdown to a complete, themed HTML document with the
/// forward-compatible per-line / per-100-column position markers embedded.
///
/// The rendered Markdown body is wrapped as
/// `<!doctype html><html><head><meta charset="utf-8"><style>{INTER_FONT_FACE}{MATH_FONT_FACE}{BASE_CSS}{MATH_CSS}{theme_css}{width_css}</style></head><body class="wolf-markdown">{body}</body></html>`
/// so the front-end web view can load the string directly. See the module docs for
/// the marker scheme and its DOM placement.
///
/// `width_mode` selects the content-area sizing: [`WidthMode::Full`] appends no
/// width CSS (full-bleed; the in-app webview frame caps it), while
/// [`WidthMode::Capped`] appends [`CAPPED_WIDTH_CSS`] AFTER the theme CSS so the
/// standalone document (the always-capped browser preview, OQ-3) caps + centers its
/// own content.
#[must_use]
pub fn render_html(source: &str, theme: PreviewTheme, width_mode: WidthMode) -> String {
    let markers = desired_markers(source);

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    // O5: honor explicit `{#custom-id}` heading attributes; the auto-slug pipeline
    // only assigns an id when the heading declares none.
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    // Task 1: yield `Event::InlineMath` / `Event::DisplayMath` for `$…$` / `$$…$$`
    // LaTeX so `MathRenderer` can convert them to inline MathML.
    options.insert(Options::ENABLE_MATH);

    let slugs = heading_slugs(source, options);
    let offset_iter = Parser::new_ext(source, options).into_offset_iter();
    let with_ids = HeadingIdAssigner::new(offset_iter, slugs);
    // MathRenderer sits BEFORE MarkerInjector: it replaces each math event with an
    // `InlineHtml` MathML span, which `is_block_start` never matches, so no marker
    // is ever injected inside a `<math>` element.
    let with_math = MathRenderer::new(with_ids);
    let injector = MarkerInjector::new(with_math, markers);

    let mut body = String::new();
    html::push_html(&mut body, injector);

    // `WidthMode::Capped` appends the horizontal-only content cap AFTER the theme
    // CSS (so it wins the cascade); `WidthMode::Full` appends nothing (full-bleed).
    let width_css = match width_mode {
        WidthMode::Capped => CAPPED_WIDTH_CSS,
        WidthMode::Full => "",
    };

    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <style>{INTER_FONT_FACE}{MATH_FONT_FACE}{BASE_CSS}{MATH_CSS}{theme}{width_css}</style>\
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
            let byte_in_line = line.char_indices().nth(col).map_or(line.len(), |(b, _)| b);
            markers.push((line_start + byte_in_line, line_no, col as u32));
            col += MARKER_COLUMN_STRIDE;
        }

        // Advance past this line and its trailing '\n'. The extra +1 for the final
        // line (which has no '\n') is harmless: no later line references it.
        line_start += line.len() + 1;
    }
    markers
}

/// Converts accumulated heading text into a GitHub-style, Unicode-preserving URL
/// slug, deduplicating against the shared `seen` map so ids are unique in document
/// order. This is the source-of-truth implementation of §SLUG.
///
/// Algorithm (EXACT):
/// 1. Lowercase the whole string via [`str::to_lowercase`] (full Unicode — so
///    `"Café Ω"` lowercases to `"café ω"`, not just ASCII).
/// 2. Walk char-by-char with NO hyphen collapsing:
///    - [`char::is_alphanumeric`] (Unicode-aware) → push the char,
///    - `'-'` → push `'-'` (an existing hyphen is preserved verbatim),
///    - [`char::is_whitespace`] → push `'-'`,
///    - anything else (punctuation/symbols) → drop it.
///
///    Because whitespace maps 1:1 to a hyphen and nothing is collapsed,
///    `"C++ & Rust"` → `"c--rust"` and `"foo  bar"` → `"foo--bar"`.
/// 3. If the result is empty after stripping, fall back to the base `"section"`.
/// 4. Deduplicate against `seen` (a shared `base -> count` map, threaded across
///    every heading in document order): if `base` is unseen, use `base` and record
///    count `1`; otherwise the id is `"{base}-{n}"` where `n` is the current count,
///    and the count is incremented. Explicit `{#custom}` ids (see [`heading_slugs`])
///    are also registered in the same map, so a later auto-slug that would collide
///    with an explicit id suffixes around it. The rare literal `"{base}-{n}"`
///    heading-text collision (e.g. an actual `"foo-1"` heading clashing with the
///    dedup of a second `"foo"`) is documented and accepted (O2).
fn slugify(text: &str, seen: &mut HashMap<String, u32>) -> String {
    let lowered = text.to_lowercase();
    let mut base = String::with_capacity(lowered.len());
    for c in lowered.chars() {
        if c.is_alphanumeric() {
            base.push(c);
        } else if c == '-' || c.is_whitespace() {
            // An existing hyphen is preserved verbatim; whitespace maps to '-'.
            base.push('-');
        }
        // else: drop punctuation/symbols.
    }
    if base.is_empty() {
        base = "section".to_string();
    }

    dedup(base, seen)
}

/// Deduplicates `base` against the shared `seen` map (§SLUG step 4): an unseen
/// `base` is used as-is (count → 1); a seen `base` becomes `"{base}-{n}"` for the
/// current count `n` (count → n+1). Shared by [`slugify`] and the explicit-id
/// registration in [`heading_slugs`].
fn dedup(base: String, seen: &mut HashMap<String, u32>) -> String {
    match seen.get(&base).copied() {
        None => {
            seen.insert(base.clone(), 1);
            base
        }
        Some(n) => {
            seen.insert(base.clone(), n + 1);
            format!("{base}-{n}")
        }
    }
}

/// Computes, in document order, the auto-slug id for every heading in `source`.
///
/// One entry per heading: `Some(slug)` when the heading declares no explicit id
/// (an auto-slug must be assigned), or `None` when the heading already carries an
/// explicit `{#custom}` id (nothing to assign — but the explicit id is still
/// REGISTERED in the shared `seen` map so later auto-slugs suffix around it).
///
/// This runs a FRESH parse of `source` (O6: the extra pre-pass is accepted — renders
/// are debounced). The accumulated heading text for auto-slugs is the in-order
/// concatenation of every [`Event::Text`] and [`Event::Code`] between the heading's
/// `Start` and `End`, matching §SLUG.
fn heading_slugs(source: &str, options: Options) -> Vec<Option<String>> {
    let mut seen: HashMap<String, u32> = HashMap::new();
    let mut slugs: Vec<Option<String>> = Vec::new();
    let mut in_heading = false;
    let mut explicit = false;
    let mut acc = String::new();

    for event in Parser::new_ext(source, options) {
        match event {
            Event::Start(Tag::Heading { id, .. }) => {
                in_heading = true;
                acc.clear();
                if let Some(id) = id {
                    // Explicit id: register it (verbatim) so later auto-slugs
                    // dedup around it, and mark this heading as needing no slug.
                    explicit = true;
                    let _ = dedup(id.to_string(), &mut seen);
                } else {
                    explicit = false;
                }
            }
            Event::End(TagEnd::Heading(_)) if in_heading => {
                if explicit {
                    slugs.push(None);
                } else {
                    slugs.push(Some(slugify(&acc, &mut seen)));
                }
                in_heading = false;
            }
            Event::Text(t) | Event::Code(t) if in_heading && !explicit => {
                acc.push_str(&t);
            }
            _ => {}
        }
    }

    slugs
}

/// An iterator adapter over [`pulldown_cmark::OffsetIter`] that rewrites each
/// heading `Start` tag with no explicit id to carry its auto-slug id from
/// [`heading_slugs`] (computed in the same document order).
///
/// `next_heading` advances on EVERY heading `Start` (both explicit-id and
/// auto-id headings), so it stays aligned with the one-entry-per-heading `slugs`
/// vector. A heading that already carries an explicit id (its slot is `None`) is
/// passed through untouched. All non-heading events pass through with their source
/// range intact, so [`MarkerInjector`] downstream still sees `(Event, Range)`.
struct HeadingIdAssigner<I> {
    inner: I,
    /// Per-heading auto-slug (document order); see [`heading_slugs`].
    slugs: Vec<Option<String>>,
    /// Index of the next heading `Start` to be seen.
    next_heading: usize,
}

impl<'a, I> HeadingIdAssigner<I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    fn new(inner: I, slugs: Vec<Option<String>>) -> Self {
        Self {
            inner,
            slugs,
            next_heading: 0,
        }
    }
}

impl<'a, I> Iterator for HeadingIdAssigner<I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        let (event, range) = self.inner.next()?;
        if let Event::Start(Tag::Heading {
            level,
            id: None,
            classes,
            attrs,
        }) = event
        {
            let slug = self.slugs.get(self.next_heading).cloned().flatten();
            self.next_heading += 1;
            let rebuilt = Event::Start(Tag::Heading {
                level,
                id: slug.map(Into::into),
                classes,
                attrs,
            });
            return Some((rebuilt, range));
        }
        if matches!(event, Event::Start(Tag::Heading { .. })) {
            // Explicit-id heading: still advance to keep alignment with `slugs`.
            self.next_heading += 1;
        }
        Some((event, range))
    }
}

/// Renders one LaTeX fragment to a MathML string via [`pulldown_latex`], in the
/// given [`DisplayMode`] (`Block` yields `<math display="block">`, `Inline` yields
/// `<math display="inline">`). Returns `Some(mathml)` on success, or `None` if the
/// renderer reports an I/O error (unreachable for the in-memory `String` writer,
/// but handled explicitly — never unwrapped — so a caller can degrade gracefully).
///
/// Note: malformed LaTeX does NOT surface here as `None`; pulldown-latex renders it
/// into an inline `<merror>` element and still returns `Ok`, so it degrades to a
/// visible error inside the MathML rather than a panic.
fn render_math(latex: &str, mode: DisplayMode) -> Option<String> {
    let storage = Storage::new();
    let parser = LatexParser::new(latex, &storage);
    let config = RenderConfig {
        display_mode: mode,
        ..RenderConfig::default()
    };
    let mut out = String::new();
    match push_mathml(&mut out, parser, config) {
        Ok(()) => Some(out),
        Err(_) => None,
    }
}

/// An iterator adapter over [`pulldown_cmark::OffsetIter`] that converts each
/// `$…$` inline-math and `$$…$$` display-math event (yielded once
/// [`Options::ENABLE_MATH`] is on) into an inline MathML span, preserving the
/// `(Event, Range)` shape [`MarkerInjector`] downstream expects.
///
/// Both inline and display math become an [`Event::InlineHtml`] carrying the
/// rendered `<math>` markup: math events are emitted at INLINE positions, so a
/// block-level replacement would corrupt paragraph nesting, and `InlineHtml`
/// passes through the HTML writer verbatim (never double-escaped). Display math
/// uses [`DisplayMode::Block`] (so its `<math>` carries `display="block"`), inline
/// math [`DisplayMode::Inline`]. If [`render_math`] reports failure the fragment
/// degrades to a plain [`Event::Text`] of its original `$…$` source — never a
/// panic. All non-math events pass through with their source range intact.
struct MathRenderer<I> {
    inner: I,
}

impl<'a, I> MathRenderer<I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    fn new(inner: I) -> Self {
        Self { inner }
    }
}

impl<'a, I> Iterator for MathRenderer<I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        let (event, range) = self.inner.next()?;
        let rebuilt = match event {
            Event::InlineMath(latex) => match render_math(&latex, DisplayMode::Inline) {
                Some(mathml) => Event::InlineHtml(mathml.into()),
                None => Event::Text(format!("${latex}$").into()),
            },
            Event::DisplayMath(latex) => match render_math(&latex, DisplayMode::Block) {
                Some(mathml) => Event::InlineHtml(mathml.into()),
                None => Event::Text(format!("$${latex}$$").into()),
            },
            other => other,
        };
        Some((rebuilt, range))
    }
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
    use std::collections::HashMap;

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
            !markers
                .iter()
                .any(|(_, _, c)| *c as usize == MARKER_COLUMN_STRIDE),
            "a line of exactly {MARKER_COLUMN_STRIDE} chars earns no -100 marker"
        );

        let past_stride = "a".repeat(MARKER_COLUMN_STRIDE + 1);
        let markers = desired_markers(&past_stride);
        assert!(
            markers
                .iter()
                .any(|(_, _, c)| *c as usize == MARKER_COLUMN_STRIDE),
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
        let html = render_html("# Hi\n\ntext", PreviewTheme::Default, WidthMode::Full);
        assert!(html.to_ascii_lowercase().contains("<!doctype html"));
        assert!(html.contains(r#"class="wolf-markdown""#));
        assert!(html.contains("<h1"));
        assert!(html.contains(r#"id="wl-1-0""#));
    }

    /// `WidthMode::Full` is the FULL-BLEED / current behavior: no content
    /// `max-width` cap and no centering, so the in-app webview FRAME (not CSS) does
    /// any capping. The body stays flush.
    #[test]
    fn render_html_full_is_flush() {
        let html = render_html("# Hi\n\ntext", PreviewTheme::Default, WidthMode::Full);
        assert!(
            !html.contains("max-width:45rem"),
            "full-width render must NOT cap the content width (no `max-width:45rem` \
             content-centering cap; note `base.css`'s `img {{ max-width: 100% }}` means \
             the bare substring `max-width` is always present): {html}"
        );
        assert!(
            !html.contains("margin-left:auto"),
            "full-width render must NOT center the content (no `margin-left:auto`): {html}"
        );
    }

    /// `WidthMode::Capped` (the BROWSER preview's always-on mode, OD3) appends a
    /// `body.wolf-markdown` rule that caps the content to `max-width:45rem` (≈ 90
    /// proportional Inter chars, OQ-3) and centers it with `margin-left:auto` /
    /// `margin-right:auto`. Only HORIZONTAL props are set, so the vertical baseline
    /// grid is preserved (checked here by the presence of the horizontal-only cap
    /// rule; a substring match for `max-width:45rem` + the auto side margins is
    /// sufficient).
    #[test]
    fn render_html_capped_centers() {
        let html = render_html("# Hi\n\ntext", PreviewTheme::Default, WidthMode::Capped);
        assert!(
            html.contains("max-width:45rem"),
            "capped render must cap the content to `max-width:45rem`: {html}"
        );
        assert!(
            html.contains("margin-left:auto"),
            "capped render must center via `margin-left:auto`: {html}"
        );
        assert!(
            html.contains("margin-right:auto"),
            "capped render must center via `margin-right:auto`: {html}"
        );
    }

    /// A fresh, empty dedup map, so each `slugify` case starts from a clean
    /// `seen` count (except the dedup test, which threads one map across calls).
    fn fresh_seen() -> HashMap<String, u32> {
        HashMap::new()
    }

    /// The common case: lowercase and space→hyphen (§SLUG steps 1-2).
    #[test]
    fn slugify_basic() {
        let mut seen = fresh_seen();
        assert_eq!(slugify("Hello World", &mut seen), "hello-world");
    }

    /// Punctuation is DROPPED (not hyphenated); whitespace becomes a hyphen and
    /// is NOT collapsed, so `"Foo, Bar!"` → `"foo-bar"` and `"C++ & Rust"` →
    /// `"c--rust"` (the comma/`+`/`&`/`!` drop, the surrounding spaces stay).
    #[test]
    fn slugify_drops_punctuation() {
        let mut seen = fresh_seen();
        assert_eq!(slugify("Foo, Bar!", &mut seen), "foo-bar");

        let mut seen = fresh_seen();
        assert_eq!(slugify("C++ & Rust", &mut seen), "c--rust");
    }

    /// An existing hyphen in the source text is preserved verbatim (§SLUG: `'-'`
    /// → push `'-'`), never doubled or collapsed.
    #[test]
    fn slugify_keeps_existing_hyphens() {
        let mut seen = fresh_seen();
        assert_eq!(slugify("multi-word title", &mut seen), "multi-word-title");
    }

    /// Code-span text participates in the slug (the accumulator includes
    /// `Event::Code`), so the accumulated `"foo"` slugs to `"foo"`.
    #[test]
    fn slugify_code_span() {
        let mut seen = fresh_seen();
        assert_eq!(slugify("foo", &mut seen), "foo");
    }

    /// Unicode is RETAINED and lowercased (full Unicode `to_lowercase`), so
    /// `"Café Ω"` → `"café-ω"` (O2 accepts Unicode-preserving slugs).
    #[test]
    fn slugify_unicode() {
        let mut seen = fresh_seen();
        assert_eq!(slugify("Café Ω", &mut seen), "café-ω");
    }

    /// A heading that strips to nothing (all punctuation) falls back to the base
    /// `"section"` (§SLUG step 3).
    #[test]
    fn slugify_empty_after_strip() {
        let mut seen = fresh_seen();
        assert_eq!(slugify("!!!", &mut seen), "section");
    }

    /// Duplicate headings dedup against the SHARED `seen` map in document order:
    /// the first `"Intro"` → `"intro"`, the second → `"intro-1"`, the third →
    /// `"intro-2"` (§SLUG step 4).
    #[test]
    fn slugify_dedup() {
        let mut seen = fresh_seen();
        assert_eq!(slugify("Intro", &mut seen), "intro");
        assert_eq!(slugify("Intro", &mut seen), "intro-1");
        assert_eq!(slugify("Intro", &mut seen), "intro-2");
    }

    /// `render_html` emits a computed slug `id` on the heading (with
    /// `ENABLE_HEADING_ATTRIBUTES` on and the auto-slug pipeline wired in).
    #[test]
    fn render_html_heading_has_id() {
        let html = render_html("# Alpha Heading\n", PreviewTheme::Default, WidthMode::Full);
        assert!(
            html.contains(r#"id="alpha-heading""#),
            "a heading must carry its computed slug id: {html}"
        );
    }

    /// An explicit `{#custom}` id is honored verbatim (O5); it is also REGISTERED
    /// in `seen`, so a later auto-slug that would collide suffixes around it
    /// (`"# Custom"` → `id="custom-1"`).
    #[test]
    fn render_html_explicit_heading_id() {
        let html = render_html(
            "# Title {#custom}\n",
            PreviewTheme::Default,
            WidthMode::Full,
        );
        assert!(
            html.contains(r#"id="custom""#),
            "an explicit {{#custom}} id must be honored verbatim: {html}"
        );

        let html = render_html(
            "# Title {#custom}\n\n# Custom\n",
            PreviewTheme::Default,
            WidthMode::Full,
        );
        assert!(
            html.contains(r#"id="custom""#),
            "the explicit id stays verbatim: {html}"
        );
        assert!(
            html.contains(r#"id="custom-1""#),
            "the later auto-slug colliding with the explicit id must suffix to custom-1: {html}"
        );
    }

    /// The heading-slug id and the invisible `wl-{L}-{C}` source marker coexist:
    /// adding heading ids must not displace the marker injector's spans.
    #[test]
    fn render_html_marker_still_present() {
        let html = render_html("# Alpha Heading\n", PreviewTheme::Default, WidthMode::Full);
        assert!(
            html.contains(r#"id="alpha-heading""#),
            "the heading slug id must be present: {html}"
        );
        assert!(
            html.contains(r#"id="wl-1-0""#),
            "the source marker must still be present alongside the heading id: {html}"
        );
    }

    /// Inline `$...$` LaTeX math is rendered to inline MathML (`<math`…`</math>`),
    /// and the raw `$e=mc^2$` source literal no longer appears in the output.
    /// Assertions key off the STABLE MathML contract (`<math`/`</math>`), not the
    /// exact markup pulldown-latex emits.
    #[test]
    fn inline_math_renders_mathml() {
        let html = render_html("$e=mc^2$\n", PreviewTheme::Default, WidthMode::Full);
        assert!(
            html.contains("<math"),
            "inline math must render a <math> element: {html}"
        );
        assert!(
            html.contains("</math>"),
            "inline math must close its <math> element: {html}"
        );
        assert!(
            !html.contains("$e=mc^2$"),
            "the raw `$e=mc^2$` LaTeX literal must not survive into the output: {html}"
        );
    }

    /// Display `$$...$$` math renders a BLOCK-mode `<math display="block">` element.
    #[test]
    fn display_math_is_block() {
        let html = render_html(
            "$$\\int_0^1 x\\,dx$$\n",
            PreviewTheme::Default,
            WidthMode::Full,
        );
        assert!(
            html.contains("display=\"block\""),
            "display math must render `display=\"block\"` on its <math> element: {html}"
        );
    }

    /// The generated MathML passes through as RAW HTML — a literal `<math`, not the
    /// escaped `&lt;math` — proving the math events are emitted verbatim and not
    /// double-escaped by the HTML writer.
    #[test]
    fn math_not_double_escaped() {
        let html = render_html("$e=mc^2$\n", PreviewTheme::Default, WidthMode::Full);
        assert!(
            html.contains("<math"),
            "the MathML must appear as raw HTML (`<math`), not escaped: {html}"
        );
        assert!(
            !html.contains("&lt;math"),
            "the MathML must NOT be double-escaped into `&lt;math`: {html}"
        );
    }

    /// The Inter variable `@font-face` is inlined as a base64 `data:` woff2 URI, with
    /// the `100 900` variable weight range, so preview rendering never depends on a
    /// host-installed font.
    #[test]
    fn inter_font_face_present() {
        let html = render_html("# Title\n", PreviewTheme::Default, WidthMode::Full);
        assert!(
            html.contains("@font-face"),
            "the document must inline an @font-face rule: {html}"
        );
        assert!(
            html.contains("Inter"),
            "the inlined font stack must include the `Inter` family: {html}"
        );
        assert!(
            html.contains("100 900"),
            "the Inter @font-face must declare the `100 900` variable weight range: {html}"
        );
        assert!(
            html.contains("data:font/woff2;base64,"),
            "the Inter font must be inlined as a base64 data:font/woff2 URI: {html}"
        );
    }

    /// The bundled Latin Modern Math font is inlined as a base64 `data:` woff2 URI so
    /// MathML rendering is pinned to a bundled math font (no host-installed font).
    #[test]
    fn math_font_present() {
        let html = render_html("$e=mc^2$\n", PreviewTheme::Default, WidthMode::Full);
        assert!(
            html.contains("Latin Modern Math"),
            "the document must pin MathML to the bundled `Latin Modern Math` font: {html}"
        );
        assert!(
            html.contains("data:font/woff2;base64,"),
            "the math font must be inlined as a base64 data:font/woff2 URI: {html}"
        );
    }

    /// Malformed LaTeX (here, a `\frac` missing its second argument) degrades
    /// gracefully: `pulldown-latex` does not error but renders a `<merror>` MathML
    /// node, so `render_html` must NOT panic and must still emit a valid full
    /// document containing a `<math` element.
    #[test]
    fn malformed_latex_degrades_without_panic() {
        let html = render_html("$\\frac{1}$\n", PreviewTheme::Default, WidthMode::Full);
        assert!(
            html.to_ascii_lowercase().contains("<!doctype html"),
            "malformed math must still yield a valid full document: {html}"
        );
        assert!(
            html.contains(r#"class="wolf-markdown""#),
            "malformed math must still yield the wolf-markdown body wrapper: {html}"
        );
        assert!(
            html.contains("<math"),
            "malformed math must still render a MathML `<math` element: {html}"
        );
    }

    /// A source marker (`wolf-src`) is never injected INSIDE a `<math>` element,
    /// which would corrupt the MathML. Checked robustly by scanning the substring
    /// between the first `<math` and the following `</math>` for any `wolf-src` span.
    #[test]
    fn marker_not_inside_math() {
        let html = render_html("$x=1$ and text\n", PreviewTheme::Default, WidthMode::Full);
        let math_start = html
            .find("<math")
            .unwrap_or_else(|| panic!("missing <math> element in: {html}"));
        let after_start = &html[math_start..];
        let math_end = after_start
            .find("</math>")
            .unwrap_or_else(|| panic!("missing </math> close in: {html}"));
        let math_span = &after_start[..math_end];
        assert!(
            !math_span.contains("wolf-src"),
            "no `wolf-src` marker span may appear inside the <math> element: {math_span}"
        );
    }
}
