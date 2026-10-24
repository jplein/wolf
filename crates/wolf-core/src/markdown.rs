//! Pure, GUI-free, IO-free Markdown → themed-HTML rendering for the preview pane.
//!
//! `wolf-core` owns the render: the front-end web view just loads
//! the HTML string this module produces. Like [`crate::rank`] this module touches
//! NO GUI toolkit and NO filesystem — it is a pure `&str` → `String` transform,
//! headlessly unit-testable. The Markdown engine is [`pulldown_cmark`], chosen for
//! its [`pulldown_cmark::Parser::into_offset_iter`], which yields each event paired
//! with its SOURCE byte range — the basis of the forward-compatible marker scheme.
//!
//! ## The marker/anchor scheme (and the cursor-follow scroll that consumes it)
//!
//! The emitted HTML carries invisible per-line and per-100-column position markers
//! whose ids encode a position in the ORIGINAL Markdown source, so the front-end's
//! cursor-follow scroll can find and reveal the line the user is editing. Each
//! marker is an empty inline `<span class="wolf-src" id="wl-{L}-{C}"
//! data-line="{L}" data-col="{C}"></span>` (see [`marker_span`]): `L` is the
//! 1-based hard buffer line (matches nvim `line('.')`), `C` is the 0-based
//! CHARACTER column (a character offset, NOT a byte offset, so multibyte
//! lines resolve correctly). The `id` gives O(1) `getElementById`; the CSS
//! `.wolf-src` rule (in `markdown/base.css`) makes it a zero-size, invisible inline
//! anchor that never affects the rendered body.
//!
//! Marker set ([`desired_markers`]): for EVERY hard line `L` in `1..=line_count`, a
//! marker `(L, 0)`; ADDITIONALLY, for any line whose character length is
//! `>= MARKER_COLUMN_STRIDE`, markers `(L, 100)`, `(L, 200)`, … for each multiple
//! of [`MARKER_COLUMN_STRIDE`] strictly less than the line's character length.
//!
//! ## DOM placement (two tiers: block-top flush, and inline splitting)
//!
//! [`MarkerInjector`] walks the offset iterator and places each marker by one of
//! two rules. Marker ids, the id scheme, the CHARACTER-column semantics,
//! source ordering and [`MARKER_COLUMN_STRIDE`] are identical under both — only
//! DOM PLACEMENT differs.
//!
//! **Tier 1 — block-top flush (unchanged).** At a block-level `Start(tag)` whose
//! source range is `s..e`, the injector CLAIMS every marker with byte offset `< e`
//! and emits it (as an `Event::Html` raw span, in source order) immediately before
//! the opening tag, UNLESS the marker qualifies for tier 2. A marker is always
//! flushed here when its column is `0`, when its offset is `<= s`, when the block
//! is a `CodeBlock` / `HtmlBlock` / `Table` / `TableHead` / `TableRow`, or when
//! injection is suppressed (see the exclusions below). Any markers left after the
//! stream ends are flushed as trailing spans.
//!
//! **Tier 2 — inline splitting of a block's interior `col != 0` markers.** A
//! marker with `col != 0` whose offset is strictly inside a block that may hold it
//! (`Paragraph`, `Heading`, `BlockQuote`, `List`, `Item`, `FootnoteDefinition`) is
//! DEFERRED rather than flushed, and lands at the first of these, in stream order:
//!
//! 1. an incoming `Event::Text` whose range STARTS at or after the marker's offset
//!    — the marker is emitted immediately before that `Text` (this is how a marker
//!    that fell in a source region covered by no `Text` at all — `**`, `_`,
//!    `[`/`](url)`, a backslash byte, a link destination, an entity span, raw
//!    inline HTML — still lands inside its block);
//! 2. an incoming `Event::Text` that strictly CONTAINS the offset and passes the
//!    identity predicate below — that `Text` is SPLIT into `Text(prefix)`,
//!    `Html(marker)`, …, `Text(remainder)`, so the anchor sits at its true position
//!    in the rendered inline flow. Several markers inside one `Text` all split it,
//!    in order, at strictly increasing points, and empty slices are never emitted;
//! 3. a block-level `End` — the drain is BOUNDED by that `End`'s own `range.end`,
//!    so a deferred marker never escapes the INNERMOST block containing it and a
//!    sibling's marker is never dragged into the block that closes first (a
//!    container claims all of its items' markers at its own `Start`, so item two's
//!    `wl-L-100` must survive item one's `</li>`);
//! 4. the next block-level `Start` (markers with offset `<= s` are drained before
//!    that block's own flush), or the trailing flush at stream end.
//!
//! A marker is therefore never dropped and never emitted twice.
//!
//! **Identity predicate (why splitting is safe).** A `Text` is split only when
//! `source[range] == payload`, byte for byte. For a real text item the payload IS
//! the source slice of its reported range (pulldown-cmark 0.12.2,
//! `parse.rs:2113`), but `SynthesizeText`/`SynthesizeChar` items —
//! entity references (`&amp;` over 5 source bytes → the 1-byte `"&"`), code/HTML
//! indent padding — carry a payload UNRELATED to the range (`parse.rs:2115-2116`).
//! A LENGTH check is not sufficient: some HTML5 named entities expand to a
//! same-length string (`&nvap;`, 6 source bytes → `≍⃒`, 6 bytes). When the
//! predicate fails the `Text` is emitted verbatim and the marker simply waits for
//! rule 1/3/4. Splitting is escaping-neutral because pulldown-cmark-escape escapes
//! byte by byte with no cross-character state.
//!
//! The identity holds in the non-obvious cases too, which is what makes the
//! predicate usable rather than merely safe:
//!
//! - trailing intra-line whitespace is trimmed out of the text item's RANGE, not
//!   just its payload (`firstpass.rs:1218-1227` closes the item at
//!   `final_ix - trailing_whitespace`), so a paragraph ending in a space yields
//!   `Text@0..239` for a 240-byte line rather than `0..240`. Were the range to
//!   cover the trimmed byte, the predicate would reject EVERY such paragraph and
//!   inline anchoring would silently never happen;
//! - a backslash escape closes the current item and restarts the next one AFTER
//!   the backslash (`firstpass.rs:890-926`), so the backslash byte is covered by no
//!   event at all — a gap, not a mismatch;
//! - soft/hard breaks are their own items, so a `Text` never spans a `\n` outside a
//!   code block;
//! - adjacent text items `a..b` + `b..c` are merged into `a..c`
//!   (`parse.rs:1361-1376`), which preserves the identity.
//!
//! **Exclusions.** `CodeBlock`, `HtmlBlock`, `Table` and `Image`, tracked as a
//! depth counter. No marker is emitted, deferred or split INSIDE any of the four.
//!
//! Where the excluded marker goes instead differs between the three BLOCK-level
//! exclusions and the inline one:
//!
//! - `CodeBlock` / `HtmlBlock` / `Table` are block `Start`s, so every marker whose
//!   offset falls inside one is flushed immediately BEFORE its opening tag — the
//!   historical placement. That flush happens at the block's own `Start` for its
//!   still-unclaimed markers, AND, because a container
//!   (`List` / `Item` / `BlockQuote` / `FootnoteDefinition`) defers `col != 0`
//!   markers across its whole range at its own `Start`, that same `Start` also
//!   drains any of its interior markers the container already claimed. Both halves
//!   are required: without the second, a fence nested in a list item would have its
//!   deferred markers drained at the block's `End`, where the depth is back to 0 —
//!   landing them inside `<pre><code>`.
//! - `Image` is **not** a block start (it is inline, and `End(Image)` deliberately
//!   does not drain — see below), so a marker whose offset falls in an image's alt
//!   text is NOT flushed before the `<img …/>`. It simply stays deferred across the
//!   image and lands at the containing block's next placement point, i.e. AFTER the
//!   `<img …/>` — at the next splittable `Text`, or at that block's `End`. What the
//!   exclusion guarantees for an image is only that the marker never enters the
//!   element; it does not promise a before-the-tag position.
//!
//! - a PLAIN code block still yields `Event::Text`, and inside a fence one `Text`
//!   can span a whole line INCLUDING its `\n`, so an interior marker would
//!   otherwise be "strictly inside" it and would corrupt rendered code (a
//!   highlighted block's interior is already `InlineHtml`, and `CodeHighlighter`
//!   passes `Start`/`End(CodeBlock)` through, so the bracket holds either way);
//! - an HTML block yields only `Event::Html`, but a marker must not be emitted
//!   between two raw HTML lines either;
//! - a raw span between `<tbody>` and `<tr>` is FOSTER-PARENTED out of the table by
//!   the HTML parser;
//! - **images are the load-bearing case**: pulldown-cmark builds `alt="…"` with
//!   `raw_text()`, which DROPS `Event::Html` outright and escapes
//!   `Text`/`InlineHtml` INTO the attribute (`html.rs:471-509`). A marker leaking
//!   into an image would vanish from the document and break its id's existence
//!   guarantee — which is also why a marker span is emitted as `Event::Html` and
//!   must NEVER become `Event::InlineHtml` (`Html` fails safe).
//!
//! `Tag::MetadataBlock` is unreachable: neither metadata option is enabled in
//! `markdown_options`.
//!
//! **Snap (layout safety).** An inline split puts an inline-box boundary between
//! two characters. That introduces no soft-wrap opportunity (line breaking is
//! defined on text content, not inline-box edges) and `.wolf-src`'s
//! `font-size:0`/`line-height:0` keep it from contributing a line box — but it does
//! break glyph SHAPING across it, so a ligature or kern pair straddling the point
//! would be lost. The split point is therefore snapped to the nearest whitespace
//! word boundary within [`MARKER_SNAP_MAX_BYTES`] (ties pick the smaller offset);
//! with no boundary in the window it splits at the exact offset. The marker's `id`
//! / `data-line` / `data-col` always name the EXACT `(L, C)`.
//!
//! One accepted behavioral consequence: an inline anchor's
//! `getBoundingClientRect().top` is measured at its zero-height inline box, near
//! the text baseline of the line it sits on — a few pixels lower than the rect a
//! block-top anchor reported for the same `(L, C)`. The cursor-follow decision's
//! viewport-edge threshold absorbs that offset, so no front-end change is needed.
//!
//! **Ordering invariant, and its documented exception.** The col-0 markers are
//! still emitted in strictly ascending document order — the front-end's tier-4
//! fallback sweep walks `.wolf-src[data-col="0"]` backwards and depends on exactly
//! that, so it must never be widened to all `.wolf-src`. GLOBAL document order is
//! no longer ascending: within one block a col-0 marker belonging to a LATER hard
//! line is flushed at the block top before an earlier-offset `col != 0` marker
//! placed inline (a paragraph spanning lines 3–4 whose line 3 is 250 characters
//! emits `wl-3-0`, `wl-4-0`, then `wl-3-100`, `wl-3-200` inside the `<p>`). This is
//! harmless because tiers 1–3 of the front-end's `resolve()` are `getElementById`
//! lookups.
//!
//! **Remaining accepted limitation.** Only lines longer than
//! [`MARKER_COLUMN_STRIDE`] gain intra-block anchoring. A paragraph HARD-WRAPPED
//! into several short source lines still collapses all of its `wl-{L}-0` markers to
//! the block top, so cursor-follow is still paragraph-granular there; likewise a
//! fenced code block, a table and an image's alt text anchor to their block top by
//! construction. Every marker still EXISTS with a unique id in all of those cases.
//!
//! ## Who consumes the markers (all of it built, all of it OUTSIDE this module)
//!
//! The cursor-follow scroll is live. The chain, end to end:
//!
//! 1. `runtime/nvim/init.lua` emits the `wolf_markdown_cursor` notification
//!    (`[source_bufnr, line, char_col]`) from markdown-guarded `CursorMoved` /
//!    `CursorMovedI` / `BufEnter` / `BufWinEnter` autocmds. `line` is a 1-based
//!    `line('.')`; `char_col` is `charcol('.') - 1`, which is ALREADY a 0-based
//!    CHARACTER column — `charcol()` is `col()`'s character-index twin, so
//!    there is **no** byte→character conversion anywhere in the chain, and none is
//!    needed. Do NOT add one, and do not switch the Lua side to `col('.')`: that
//!    would silently mis-anchor every line containing a multibyte character.
//! 2. [`crate::redraw`]'s `MARKDOWN_CURSOR_EVENT` / `apply_markdown_cursor` upserts
//!    it into `Snapshot::markdown_cursors`, keyed by the SOURCE buffer number (the
//!    same key as `markdown_ticks`). No `seq` bump.
//! 3. `wolf-ffi` joins that cursor per preview window as
//!    `FfiPreview::source_cursor_line()` / `source_cursor_col()`.
//! 4. The macOS `WolfKeyMap.PreviewScrollSeam` builds the JS that resolves
//!    `#wl-{line}-{col}` (column rounded DOWN to a multiple of
//!    [`MARKER_COLUMN_STRIDE`]) → `#wl-{line}-0` → the nearest PRECEDING line's
//!    marker → raw `scrollY`, and scrolls it into view only when it crosses a
//!    viewport-edge threshold. On the paths that still navigate (first bind, a
//!    source switch, a pending cross-file `#fragment`, a load already in flight) it
//!    captures the anchor's viewport offset before the reload and restores it
//!    afterward. On the TYPING path it does not navigate at all: it splits this
//!    module's rendered document at the `<body …>` tag and swaps the body's inner
//!    HTML into the live page (`__wolfUpdate` — `DOMParser` + `replaceChildren`),
//!    re-anchoring in the SAME JavaScript task, so typing neither jumps the preview
//!    to the top nor flickers.
//!
//!    That body-only swap is sound ONLY because the in-app preview's `<head>` is
//!    INVARIANT: [`crate::nvim::Nvim::render_preview`] pins
//!    [`PreviewTheme::Default`] + [`WidthMode::Full`], and everything
//!    [`render_html`] puts in the head is a compile-time constant — the CSS, the
//!    two inlined faces, and the HTML comment carrying
//!    [`crate::licenses::FONT_LICENSE_NOTICES`] — so the head is
//!    byte-identical on every render, INCLUDING across a dark/light flip (that flip
//!    is a `prefers-color-scheme` media query inside the constant CSS; only the
//!    body's inline `style="color:#…"` highlight spans change). It also matters
//!    because that head inlines two woff2 faces as ~965 KiB of base64, which a
//!    navigation would re-ship and re-parse on every keystroke. A future themes or
//!    width feature MUST revisit this: the front-end self-enforces the premise by
//!    comparing head LENGTHS and falling back to a full load when they differ, so a
//!    change here degrades performance rather than correctness — but do not assume
//!    the swap stays free.
//!
//! This module's own guarantee is unchanged and is the whole contract it owes that
//! chain: the markers are present in the emitted HTML, their ids are stable, and `C`
//! is a CHARACTER column.
//!
//! ## Code-block syntax highlighting
//!
//! Fenced/indented code blocks render plain (`<pre><code>`) by default. When a
//! caller supplies per-block highlight data (via [`render_html_with_highlights`]),
//! this module rewrites each highlighted block's interior text into inline-styled
//! `<span style="color:#RRGGBB;font-weight:bold;…">` runs, leaving the plain
//! `<pre><code class="language-xxx">` wrapper untouched.
//!
//! The highlight data itself is produced OUTSIDE this pure module (in
//! [`crate::nvim`], by asking the captive Neovim's treesitter to highlight each
//! block). This module stays IO-free: it only enumerates the blocks
//! ([`code_blocks`]) so the caller can align highlight results back to them by
//! DOCUMENT ORDER, and applies whatever spans it is given ([`BlockHighlight`]).
//!
//! CRITICAL alignment contract: a pulldown-cmark fenced block is
//! `Start(CodeBlock) → 1+ Text → End(CodeBlock)`. Both [`code_blocks`] (which the
//! caller uses to build the highlight request) and the internal `CodeHighlighter`
//! (which applies the results) concatenate the SAME `Event::Text` bodies —
//! INCLUDING the trailing `\n` pulldown-cmark yields — so Neovim's `(row,
//! byte-col)` indices align with `text.split('\n')` here. When a block has no
//! highlight data — the fence carries no language, or that language has no parser
//! and `highlights` query on Neovim's runtimepath — it falls back to the plain
//! output with zero change. That decision is per LANGUAGE, not per platform: the
//! producing Lua helper is module-level and runs everywhere.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::ops::Range;

use pulldown_cmark::{html, CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};
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

/// How far (in BYTES, each way) an interior marker's inline split point may move
/// to land on a word boundary instead of mid-word.
///
/// A marker placed inside a paragraph's text splits one `Event::Text` into a
/// prefix and a remainder, which puts an inline box boundary between two
/// characters. Such a boundary introduces no soft-wrap opportunity (line breaking
/// is defined on text content, not inline-box edges) and `.wolf-src`'s
/// `font-size:0`/`line-height:0` keep it from contributing a line box — but it
/// DOES break glyph shaping across it, so a ligature (Inter has `ffi`/`fi`) or a
/// kern pair straddling the split point would be lost. Snapping the split to the
/// nearest whitespace boundary within this window avoids that; the marker's `id` /
/// `data-line` / `data-col` still name the EXACT `(L, C)` position.
///
/// The value must stay comfortably below HALF of [`MARKER_COLUMN_STRIDE`]: two
/// consecutive stride markers are at least `MARKER_COLUMN_STRIDE` CHARACTERS (so
/// at least that many bytes) apart, and `2 * 32 < 100` guarantees their snap
/// windows cannot overlap — snapped split points therefore stay strictly
/// ascending, and no two markers can ever snap onto the same point.
const MARKER_SNAP_MAX_BYTES: usize = 32;

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
/// One variant exists ([`PreviewTheme::Default`], the default theme whose CSS
/// carries a dark palette plus a `prefers-color-scheme: light` palette so it
/// follows the app's resolved dark/light appearance — but is NOT wired to the live
/// nvim colorscheme). The enum + [`theme_css`] + `include_str!`'d CSS files
/// leave room for more themes and a config-driven selector; the call site
/// hardcodes `PreviewTheme::Default`.
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
/// full-bleed behavior (the in-app webview FRAME does any capping);
/// [`WidthMode::Capped`] appends a `max-width` cap + horizontal centering to the
/// CSS so the standalone document (the always-capped browser preview)
/// caps its own content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WidthMode {
    /// Cap the content to [`CAPPED_WIDTH_CSS`] (`max-width: 45rem`, ≈ 90 proportional
    /// Inter characters) and center it horizontally. Used by the always-capped
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
/// characters" request for a proportional font, the same target the 90-column
/// editor cap uses for its monospaced grid.
///
/// ONLY horizontal properties are set (`max-width`, the two side margins, the two
/// side paddings). The vertical margin/padding is deliberately left untouched: the
/// preview's 24px baseline grid (in `markdown/base.css`) depends on the body's
/// vertical box, so overriding it here would break the baseline rhythm.
const CAPPED_WIDTH_CSS: &str =
    "body.wolf-markdown{max-width:45rem;margin-left:auto;margin-right:auto;\
     padding-left:24px;padding-right:24px}";

/// One fenced or indented code block enumerated from a Markdown source, in
/// document order, by [`code_blocks`].
///
/// The caller (see [`crate::nvim`]) hands each block's `lang` + `text` to Neovim's
/// treesitter highlighter and threads the per-block results back into
/// [`render_html_with_highlights`], aligning them by document-order index.
#[derive(Clone, Debug)]
pub struct CodeBlockInfo {
    /// The block's language: the first whitespace-delimited token of a fenced
    /// block's info string, lowercased. `None` for an indented block or a bare
    /// fence (empty info string).
    pub lang: Option<String>,
    /// The block's body: the in-order concatenation of the block's
    /// [`Event::Text`] events, byte-identical to what the render pipeline
    /// concatenates (INCLUDING the trailing newline pulldown-cmark yields).
    pub text: String,
}

/// The visual style resolved for one highlight span: a foreground color plus the
/// three carried emphases (bold / italic / underline). No background color and no
/// other attributes are carried (locked decision 4).
#[derive(Clone, Copy, Debug)]
pub struct HighlightStyle {
    /// The foreground color as a packed `0xRRGGBB` integer (mirrors
    /// [`crate::render::Rgb::from_packed`]), or `None` for no explicit foreground.
    pub color: Option<u32>,
    /// Whether the span is bold.
    pub bold: bool,
    /// Whether the span is italic.
    pub italic: bool,
    /// Whether the span is underlined.
    pub underline: bool,
}

/// One highlighted run within a code block: a half-open `[start_col, end_col)`
/// BYTE-column range on a single 0-based `row` (relative to the block's
/// `text.split('\n')`), plus the [`HighlightStyle`] to apply.
///
/// Spans are single-line, non-overlapping, and sorted by `(row, start_col)` (the
/// Neovim-side helper guarantees this; [`render_html_with_highlights`] is
/// nonetheless defensive against malformed/overlapping input).
#[derive(Clone, Copy, Debug)]
pub struct HighlightSpan {
    /// The 0-based row within the block text (`text.split('\n')`).
    pub row: u32,
    /// The 0-based inclusive start BYTE column of the run.
    pub start_col: u32,
    /// The exclusive end BYTE column of the run (half-open `[start_col, end_col)`).
    pub end_col: u32,
    /// The style applied to the run.
    pub style: HighlightStyle,
}

/// All highlight spans for ONE code block (see [`HighlightSpan`]).
#[derive(Clone, Debug)]
pub struct BlockHighlight {
    /// The block's spans, single-line and (expected) non-overlapping.
    pub spans: Vec<HighlightSpan>,
}

/// The [`Options`] set used by every parse in this module (the Markdown feature
/// flags): tables, footnotes, strikethrough, task lists, explicit heading
/// attributes, and `$…$` math. Factored so [`render_html_with_highlights`], the
/// [`heading_slugs`] pre-pass, and [`code_blocks`] all parse with identical flags.
fn markdown_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    // Honor explicit `{#custom-id}` heading attributes; the auto-slug pipeline
    // only assigns an id when the heading declares none.
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    // Yield `Event::InlineMath` / `Event::DisplayMath` for `$…$` / `$$…$$`
    // LaTeX so `MathRenderer` can convert them to inline MathML.
    options.insert(Options::ENABLE_MATH);
    options
}

/// Enumerates every fenced and indented code block in `source`, in DOCUMENT ORDER.
///
/// Runs a FRESH [`Parser::new_ext`] pass with [`markdown_options`]. Each block's
/// `lang` is the lowercased first whitespace-token of a fenced info string
/// (`None` for an indented block or a bare fence), and its `text` is the in-order
/// concatenation of the block's [`Event::Text`] bodies — byte-identical to what
/// the render pipeline concatenates, INCLUDING pulldown-cmark's trailing newline.
///
/// The i-th returned block ALIGNS with the i-th `Start(CodeBlock)` the internal
/// `CodeHighlighter` counts in [`render_html_with_highlights`], so a caller can key
/// per-block highlight data by index.
#[must_use]
pub fn code_blocks(source: &str) -> Vec<CodeBlockInfo> {
    let mut blocks = Vec::new();
    let mut current: Option<CodeBlockInfo> = None;

    for event in Parser::new_ext(source, markdown_options()) {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .map(str::to_lowercase)
                        .filter(|s| !s.is_empty()),
                    CodeBlockKind::Indented => None,
                };
                current = Some(CodeBlockInfo {
                    lang,
                    text: String::new(),
                });
            }
            Event::Text(t) => {
                if let Some(block) = current.as_mut() {
                    block.text.push_str(&t);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
            }
            _ => {}
        }
    }

    blocks
}

/// Renders `source` Markdown to a complete, themed HTML document with the
/// forward-compatible per-line / per-100-column position markers embedded.
///
/// The rendered Markdown body is wrapped as
/// `<!doctype html><html><head><meta charset="utf-8"><!--\n{FONT_LICENSE_NOTICES}\n--><style>{INTER_FONT_FACE}{MATH_FONT_FACE}{BASE_CSS}{MATH_CSS}{theme_css}{width_css}</style></head><body class="wolf-markdown">{body}</body></html>`
/// so the front-end web view can load the string directly. The comment carries
/// [`crate::licenses::FONT_LICENSE_NOTICES`], which is how the four bundled fonts'
/// attribution reaches a reader of the preview. See the module docs for the marker
/// scheme and its DOM placement.
///
/// `width_mode` selects the content-area sizing: [`WidthMode::Full`] appends no
/// width CSS (full-bleed; the in-app webview frame caps it), while
/// [`WidthMode::Capped`] appends [`CAPPED_WIDTH_CSS`] AFTER the theme CSS so the
/// standalone document (the always-capped browser preview) caps + centers its
/// own content.
#[must_use]
pub fn render_html(source: &str, theme: PreviewTheme, width_mode: WidthMode) -> String {
    // An empty highlight slice makes every code block plain — the current
    // behavior — so existing call sites see zero change.
    render_html_with_highlights(source, theme, width_mode, &[])
}

/// Renders `source` Markdown like [`render_html`], but with per-code-block
/// treesitter highlight data applied.
///
/// `code_highlights[i]` (indexed by DOCUMENT ORDER, aligned with [`code_blocks`])
/// carries the spans for the i-th code block: `Some(block)` rewrites that block's
/// interior text into inline-styled `<span>`s (see [`build_highlighted_html`]),
/// while `None` — or an index past the end of the slice — leaves it PLAIN
/// (`<pre><code>`). Passing `&[]` makes every block plain, exactly reproducing
/// [`render_html`].
///
/// The render pipeline is
/// `offset_iter → HeadingIdAssigner → MathRenderer → CodeHighlighter → MarkerInjector → html::push_html`:
/// `CodeHighlighter` runs BEFORE `MarkerInjector` so the injector still sees the
/// real block-level `Start(CodeBlock)` (its flush trigger). The highlighter passes
/// `Start`/`End` through untouched and swaps only interior `Text` for an
/// `Event::InlineHtml` of the highlighted markup, so the `<pre><code
/// class="language-xxx">` wrapper is unaffected and no marker ever lands inside
/// `<pre>`.
#[must_use]
pub fn render_html_with_highlights(
    source: &str,
    theme: PreviewTheme,
    width_mode: WidthMode,
    code_highlights: &[Option<BlockHighlight>],
) -> String {
    let markers = desired_markers(source);

    let options = markdown_options();

    let slugs = heading_slugs(source, options);
    let offset_iter = Parser::new_ext(source, options).into_offset_iter();
    let with_ids = HeadingIdAssigner::new(offset_iter, slugs);
    // Before MarkerInjector: math becomes `InlineHtml`, which `is_block_start` never matches, so
    // no marker is injected inside a math element.
    let with_math = MathRenderer::new(with_ids);
    // Before MarkerInjector, which still sees the real `Start(CodeBlock)` it flushes on: the
    // highlighter swaps only interior `Text`.
    let with_code = CodeHighlighter::new(with_math, code_highlights);
    let injector = MarkerInjector::new(with_code, markers, source);

    let mut body = String::new();
    html::push_html(&mut body, injector);

    // `WidthMode::Capped` appends the horizontal-only content cap AFTER the theme
    // CSS (so it wins the cascade); `WidthMode::Full` appends nothing (full-bleed).
    let width_css = match width_mode {
        WidthMode::Capped => CAPPED_WIDTH_CSS,
        WidthMode::Full => "",
    };

    // Notices go in the head, never the body: see crates/wolf-core/src/markdown/README.md#decisions.
    // The leading `\n` stops a notice starting with `>` or `->` from closing the `<!--` at once.
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <!--\n{licenses}\n-->\
         <style>{INTER_FONT_FACE}{MATH_FONT_FACE}{BASE_CSS}{MATH_CSS}{theme}{width_css}</style>\
         </head><body class=\"wolf-markdown\">{body}</body></html>",
        licenses = crate::licenses::FONT_LICENSE_NOTICES,
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
/// `col` characters of that line). Emits `(L, 0)` for every hard line, plus
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

        // `char_indices().nth(col)` is the byte offset of the `col`-th character; no marker sits
        // at or past `char_len`.
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
/// order. This is the source-of-truth implementation of the heading-id slug rule.
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
///    dedup of a second `"foo"`) is documented and accepted.
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

/// Deduplicates `base` against the shared `seen` map (`slugify` step 4): an unseen
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
/// This runs a FRESH parse of `source` (the extra pre-pass is accepted because renders
/// are debounced). The accumulated heading text for auto-slugs is the in-order
/// concatenation of every [`Event::Text`] and [`Event::Code`] between the heading's
/// `Start` and `End`.
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

/// An iterator adapter over the render pipeline that rewrites each HIGHLIGHTED
/// code block's interior [`Event::Text`] into an [`Event::InlineHtml`] of
/// inline-styled `<span>`s, leaving the block-level `Start`/`End` tags (and thus
/// the `<pre><code class="language-xxx">` wrapper) untouched.
///
/// Blocks are matched to `highlights` by DOCUMENT ORDER: `next_index` advances on
/// every `Start(CodeBlock)`, so the i-th code block consults `highlights[i]`. A
/// `None` slot (or an index past the slice) passes the block through verbatim
/// (plain fallback). Because this adapter preserves the real `Start(CodeBlock)`,
/// the downstream [`MarkerInjector`] still flushes markers BEFORE the block, never
/// inside `<pre>`; and because the highlighted markup is emitted as `InlineHtml`
/// it passes through the HTML writer verbatim (like [`MathRenderer`]'s MathML),
/// never double-escaped.
struct CodeHighlighter<'a, I> {
    inner: I,
    /// Per-block highlight data (document order); see [`render_html_with_highlights`].
    highlights: &'a [Option<BlockHighlight>],
    /// Index of the next code-block `Start` to be seen.
    next_index: usize,
    /// Buffered events to yield before pulling the next inner event.
    pending: VecDeque<(Event<'a>, Range<usize>)>,
}

impl<'a, I> CodeHighlighter<'a, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    fn new(inner: I, highlights: &'a [Option<BlockHighlight>]) -> Self {
        Self {
            inner,
            highlights,
            next_index: 0,
            pending: VecDeque::new(),
        }
    }
}

impl<'a, I> Iterator for CodeHighlighter<'a, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        // Drain any buffered (rewritten) events first.
        if let Some(buffered) = self.pending.pop_front() {
            return Some(buffered);
        }

        let (event, range) = self.inner.next()?;

        if matches!(event, Event::Start(Tag::CodeBlock(_))) {
            let idx = self.next_index;
            self.next_index += 1;
            let block = self.highlights.get(idx).and_then(Option::as_ref);
            let Some(block) = block else {
                // Plain passthrough: no highlight data for this block.
                return Some((event, range));
            };

            // Drain to this block's `End(CodeBlock)`, joining interior `Text` byte-identically to
            // `code_blocks`; any other interior event is buffered verbatim.
            let mut text = String::new();
            let mut end_range = range.clone();
            for (inner_event, inner_range) in self.inner.by_ref() {
                match inner_event {
                    Event::Text(t) => text.push_str(&t),
                    Event::End(TagEnd::CodeBlock) => {
                        end_range = inner_range;
                        break;
                    }
                    other => self.pending.push_back((other, inner_range)),
                }
            }

            let inner_html = build_highlighted_html(&text, block);
            self.pending
                .push_back((Event::InlineHtml(inner_html.into()), end_range.clone()));
            self.pending
                .push_back((Event::End(TagEnd::CodeBlock), end_range));

            // Emit the original `Start(CodeBlock)` now; the buffered rewritten
            // interior + `End` follow on subsequent calls.
            return Some((event, range));
        }

        Some((event, range))
    }
}

/// Builds the inline-styled HTML for one code block's interior `text`, applying
/// `block`'s spans.
///
/// The text is split on `'\n'`; each row after the first is preceded by a literal
/// `'\n'` (so line structure is preserved inside `<pre>`). Within a row, the row's
/// spans are collected and sorted by `start_col`, then walked with a byte
/// `cursor`: each span's `[start, end)` byte range is DEFENSIVELY clamped to
/// `[cursor, line.len()]` and SKIPPED if it would be empty (`start >= end`) or
/// overlap an earlier span (`start < cursor`). The gap before a span and its body
/// are HTML-escaped ([`escape_html_into`]); the body is wrapped in
/// `<span style="{style_css}">…</span>`. Any trailing text after the last span is
/// escaped. This never panics: all slicing uses clamped indices on treesitter byte
/// columns (valid UTF-8 boundaries) via [`str::get`].
fn build_highlighted_html(text: &str, block: &BlockHighlight) -> String {
    let mut out = String::new();

    for (row_idx, line) in text.split('\n').enumerate() {
        if row_idx > 0 {
            out.push('\n');
        }

        // Collect this row's spans, sorted by start column.
        let mut row_spans: Vec<&HighlightSpan> = block
            .spans
            .iter()
            .filter(|s| s.row as usize == row_idx)
            .collect();
        row_spans.sort_by_key(|s| s.start_col);

        let line_len = line.len();
        let mut cursor = 0usize;

        for span in row_spans {
            let start = (span.start_col as usize).min(line_len);
            let end = (span.end_col as usize).min(line_len);
            // Skip empty (after clamp) or overlapping spans.
            if start >= end || start < cursor {
                continue;
            }

            // Escaped gap before the span.
            if let Some(gap) = line.get(cursor..start) {
                escape_html_into(&mut out, gap);
            }

            out.push_str("<span style=\"");
            out.push_str(&style_css(&span.style));
            out.push_str("\">");
            if let Some(body) = line.get(start..end) {
                escape_html_into(&mut out, body);
            }
            out.push_str("</span>");

            cursor = end;
        }

        // Escaped trailing text after the last span.
        if let Some(rest) = line.get(cursor..) {
            escape_html_into(&mut out, rest);
        }
    }

    out
}

/// Appends `s` to `out`, escaping the three HTML-significant characters
/// (`&`→`&amp;`, `<`→`&lt;`, `>`→`&gt;`).
///
/// A local helper so the module does not depend on [`pulldown_cmark::escape`]
/// visibility. Order matters: `&` is escaped first so an already-escaped entity is
/// not double-escaped by a later pass — here each char is handled exactly once, so
/// a single left-to-right walk is correct.
fn escape_html_into(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
}

/// Renders a [`HighlightStyle`] to an inline CSS declaration string in a FIXED,
/// deterministic order: `color:#{:06x};` (when a color is set, masked to
/// `0x00FF_FFFF`), then `font-weight:bold;`, `font-style:italic;`,
/// `text-decoration:underline;` for each set flag.
fn style_css(style: &HighlightStyle) -> String {
    let mut css = String::new();
    if let Some(color) = style.color {
        css.push_str(&format!("color:#{:06x};", color & 0x00FF_FFFF));
    }
    if style.bold {
        css.push_str("font-weight:bold;");
    }
    if style.italic {
        css.push_str("font-style:italic;");
    }
    if style.underline {
        css.push_str("text-decoration:underline;");
    }
    css
}

/// Returns whether `event` is a block-level `Start` tag — where [`MarkerInjector`]
/// CLAIMS the markers whose offsets fall inside the block (flushing them before the
/// opening tag, or deferring them for inline placement — see
/// [`defers_interior_markers`]). Block-level tags: `Paragraph`, `Heading`,
/// `BlockQuote`, `CodeBlock`, `HtmlBlock`, `List`, `Item`, `FootnoteDefinition`,
/// `Table`, `TableHead`, `TableRow`. Inline tags (`Emphasis`/`Strong`/`Link`/…) and
/// `TableCell` are intentionally NOT claim points; the only other placement point
/// is an `Event::Text` in an unsuppressed context, so a marker still never lands
/// inside `<pre><code>` or an attribute.
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

/// Returns whether `event` is a block-level `Start` that may HOLD BACK its
/// interior `col != 0` markers for inline placement instead of flushing them
/// before its opening tag.
///
/// True for the blocks whose rendered body is ordinary inline content
/// (`Paragraph`, `Heading`, `BlockQuote`, `List`, `Item`, `FootnoteDefinition`).
/// False for `CodeBlock` / `HtmlBlock` / `Table` / `TableHead` / `TableRow` (see
/// [`opens_no_inject_region`]): those keep the historical flush-before-the-tag
/// placement EXACTLY, so no marker can reach a `<pre>`, an author's raw HTML, or a
/// table interior. Column-zero markers are never deferred regardless of this
/// predicate — [`MarkerInjector::claim_before`] decides that.
fn defers_interior_markers(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::Start(
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::BlockQuote(..)
                | Tag::List(..)
                | Tag::Item
                | Tag::FootnoteDefinition(..)
        )
    )
}

/// Returns whether `event` OPENS a region in which no marker may be emitted,
/// deferred or split (see [`MarkerInjector::no_inject_depth`]).
///
/// - `CodeBlock`: a plain (unhighlighted) block still yields `Event::Text`, and
///   inside a fence one `Text` can span a whole line INCLUDING its `\n`, so an
///   interior marker would otherwise be "strictly inside" it and would corrupt the
///   rendered code. (A highlighted block's interior is already `InlineHtml`, but
///   `CodeHighlighter` passes `Start`/`End(CodeBlock)` through, so this bracket
///   still holds.)
/// - `HtmlBlock`: yields only `Event::Html`, so there is nothing to split — but a
///   marker must not be emitted BETWEEN two raw HTML lines either.
/// - `Table`: a raw span between `<tbody>` and `<tr>` is FOSTER-PARENTED out of the
///   table by the HTML parser, relocating the anchor.
/// - `Image`: pulldown-cmark builds `alt="…"` with `raw_text()`, which DROPS
///   `Event::Html` outright and escapes `Text`/`InlineHtml` INTO the attribute
///   (`html.rs:471-509`) — a marker leaking in would vanish from the document and
///   break its id's existence guarantee.
///
/// `Tag::MetadataBlock` is unreachable here: neither metadata option is enabled in
/// [`markdown_options`].
fn opens_no_inject_region(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::Start(Tag::CodeBlock(..) | Tag::HtmlBlock | Tag::Table(..) | Tag::Image { .. })
    )
}

/// Returns whether `event` CLOSES a region opened by [`opens_no_inject_region`].
fn closes_no_inject_region(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::End(TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::Table | TagEnd::Image)
    )
}

/// Returns whether `event` is a block-level `End` at which still-deferred markers
/// must be drained, so that **a deferred marker never escapes the INNERMOST block
/// that contains it**.
///
/// The drain is BOUNDED by the `End`'s own `range.end`
/// ([`MarkerInjector::drain_deferred_before`]): a container (a `List`) claims every
/// one of its items' markers at its own `Start`, so an unbounded drain at the first
/// `End(Item)` would flush item two's marker into item one's `<li>`. pulldown-cmark
/// reports the FULL tag range on an `End` (it is read from the same tree node that
/// produced the `Start` — `parse.rs:2051-2060` vs `:2066-2075`), which is what
/// makes the bound available.
///
/// Deliberately NOT `TagEnd::Image`: a raw `Event::Html` emitted before
/// `End(Image)` is still consumed and dropped by `html.rs`'s `raw_text`. The inline
/// ends (`Emphasis`/`Strong`/`Strikethrough`/`Link`/`TableCell`) are excluded to
/// keep the rule minimal, not because they are known-unsafe.
fn drains_deferred(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::End(
            TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::BlockQuote(_)
                | TagEnd::List(_)
                | TagEnd::FootnoteDefinition
                | TagEnd::CodeBlock
                | TagEnd::HtmlBlock
                | TagEnd::Table
                | TagEnd::TableHead
                | TagEnd::TableRow
        )
    )
}

/// The byte offset at which to split a `Text` event so an interior marker lands as
/// close as possible to `offset` WITHOUT breaking glyph shaping mid-word.
///
/// Snaps to the nearest WORD BOUNDARY within `MARKER_SNAP_MAX_BYTES` bytes either
/// way: a byte position `p` is a word boundary iff `source` has a char boundary at
/// `p` and either the char ENDING at `p` or the char STARTING at `p` is
/// [`char::is_whitespace`]. Ties (equal distance on both sides) pick the SMALLER
/// `p`, so the result is deterministic. With no boundary in the window — a `>= 32`
/// byte unbroken token — it splits at `offset` exactly; the body's
/// `word-wrap: break-word` already permits intra-word breaks there.
///
/// Preconditions (the caller, [`MarkerInjector::try_split_text`], checks all of
/// them): `min_exclusive < offset < range.end`, `range.start <= min_exclusive`, and
/// `source.is_char_boundary(offset)`. The result then always satisfies
/// `min_exclusive < result < range.end` and `source.is_char_boundary(result)`.
/// `min_exclusive` is the previous split point in the same `Text`, which keeps
/// split points strictly increasing even though [`MARKER_SNAP_MAX_BYTES`] already
/// makes overlapping snap windows impossible.
///
/// Never panics: every index goes through [`str::get`] / [`str::is_char_boundary`].
fn snap_split_point(
    source: &str,
    range: &Range<usize>,
    offset: usize,
    min_exclusive: usize,
) -> usize {
    // The window `[low, high)` is clamped into `(max(min_exclusive, range.start), range.end)`;
    // an empty one, possible only on violated preconditions, falls through to `offset`.
    let lower = min_exclusive.max(range.start).saturating_add(1);
    let low = offset.saturating_sub(MARKER_SNAP_MAX_BYTES).max(lower);
    let high = offset.saturating_add(MARKER_SNAP_MAX_BYTES).min(range.end);

    let boundary = (low..high)
        .filter(|p| is_word_boundary(source, *p))
        .min_by_key(|p| (p.abs_diff(offset), *p));

    boundary.unwrap_or(offset)
}

/// Whether byte position `p` is a word boundary in `source`: a char boundary with
/// whitespace on at least one side. See [`snap_split_point`].
fn is_word_boundary(source: &str, p: usize) -> bool {
    if !source.is_char_boundary(p) {
        return false;
    }
    let before_is_space = source
        .get(..p)
        .and_then(|s| s.chars().next_back())
        .is_some_and(char::is_whitespace);
    let after_is_space = source
        .get(p..)
        .and_then(|s| s.chars().next())
        .is_some_and(char::is_whitespace);
    before_is_space || after_is_space
}

/// An iterator adapter over [`pulldown_cmark::OffsetIter`] that injects the marker
/// spans (as raw `Event::Html`) at their DOM position: column-zero markers (and the
/// interior markers of the non-deferrable blocks) immediately before a block-level
/// `Start` tag, and a block's interior `col != 0` markers inside the rendered
/// inline flow, by splitting the `Event::Text` that contains them. Any markers left
/// after the stream ends are flushed as trailing spans.
///
/// See the module docs for the full placement rule, the identity predicate that
/// gates splitting, the exclusion contexts, and the ordering invariant.
struct MarkerInjector<'a, I> {
    inner: I,
    /// The ORIGINAL Markdown source the inner events' ranges index into. Used for
    /// the `source[range] == payload` identity predicate, for the word-boundary
    /// snap, and as the owner of the borrowed prefix/remainder slices a split
    /// emits.
    source: &'a str,
    /// Desired markers, ascending byte offset (from [`desired_markers`]).
    markers: Vec<(usize, u32, u32)>,
    /// Index of the next un-emitted marker in `markers`. Invariant: every marker
    /// below this index has been either emitted or moved to `deferred`, exactly
    /// once.
    next_marker: usize,
    /// Markers CLAIMED by a block (so `next_marker` has passed them) but not yet
    /// placed: they are waiting for the `Event::Text` that contains them. FIFO,
    /// always ascending by offset.
    deferred: VecDeque<(usize, u32, u32)>,
    /// `> 0` while inside a code block, an HTML block, a table or an image, where
    /// no marker may be emitted, deferred or split. See
    /// [`opens_no_inject_region`].
    no_inject_depth: u32,
    /// Buffered events to yield before pulling the next inner event (a block
    /// `Start` expands to its flushed markers followed by the `Start` itself; a
    /// split `Text` expands to prefix/marker/remainder).
    pending: VecDeque<Event<'a>>,
    /// Set once the inner iterator is exhausted, so the trailing-marker flush runs
    /// exactly once.
    inner_done: bool,
}

impl<'a, I> MarkerInjector<'a, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    fn new(inner: I, markers: Vec<(usize, u32, u32)>, source: &'a str) -> Self {
        Self {
            inner,
            source,
            markers,
            next_marker: 0,
            deferred: VecDeque::new(),
            no_inject_depth: 0,
            pending: VecDeque::new(),
            inner_done: false,
        }
    }

    /// Buffers ONE marker span for output.
    ///
    /// The event MUST stay `Event::Html` and must never become
    /// `Event::InlineHtml`: `html.rs:471-509`'s `raw_text` (which builds an image's
    /// `alt="…"` attribute) DROPS `Html` but escapes `InlineHtml` INTO the
    /// attribute. Emitting `Html` is therefore fail-safe — a marker that ever leaks
    /// into an image loses its anchor rather than corrupting the rendered markup.
    fn emit_marker(&mut self, line: u32, col: u32) {
        self.pending
            .push_back(Event::Html(marker_span(line, col).into()));
    }

    /// Buffers every UNCLAIMED marker with byte offset `< end` as a raw-HTML span,
    /// in source order. Used only by the trailing flush at stream end.
    fn flush_before(&mut self, end: usize) {
        while self.next_marker < self.markers.len() && self.markers[self.next_marker].0 < end {
            let (_, line, col) = self.markers[self.next_marker];
            self.emit_marker(line, col);
            self.next_marker += 1;
        }
    }

    /// Claims every marker whose offset falls before `block.end` as this block's,
    /// at the block's opening tag.
    ///
    /// A claimed marker is either EMITTED right away (today's behavior — the flush
    /// that collapses it to the block top) or moved to `deferred` for inline
    /// placement. It is deferred only when all of these hold:
    ///
    /// - `deferrable` — the block may hold interior markers back
    ///   ([`defers_interior_markers`]);
    /// - `no_inject_depth == 0` — not inside an excluded region;
    /// - `col != 0` — column-zero markers ALWAYS keep their block-top placement
    ///   (the front-end's tier-4 sweep filters on `[data-col="0"]` and depends on
    ///   their document order);
    /// - `offset > block.start` — a marker at or before the tag has nothing to sit
    ///   inside.
    ///
    /// `next_marker` advances either way, so a marker is claimed exactly once.
    fn claim_before(&mut self, block: &Range<usize>, deferrable: bool) {
        while self.next_marker < self.markers.len() && self.markers[self.next_marker].0 < block.end
        {
            let (offset, line, col) = self.markers[self.next_marker];
            if deferrable && self.no_inject_depth == 0 && col != 0 && offset > block.start {
                self.deferred.push_back((offset, line, col));
            } else {
                self.emit_marker(line, col);
            }
            self.next_marker += 1;
        }
    }

    /// Emits every deferred marker whose offset is `<= limit`. Used at a block
    /// `Start` (with the block's `range.start`) and before a `Text` event (with the
    /// text's `range.start`), so a marker that fell in a gap covered by no `Text`
    /// still lands inside its block.
    fn drain_deferred_at_or_before(&mut self, limit: usize) {
        while self.deferred.front().is_some_and(|(o, _, _)| *o <= limit) {
            if let Some((_, line, col)) = self.deferred.pop_front() {
                self.emit_marker(line, col);
            }
        }
    }

    /// Emits every deferred marker whose offset is STRICTLY `< end`. Two call
    /// sites, both passing the range of the block they are placing markers for:
    ///
    /// - a block `End`, with that `End`'s own `range.end` — so a deferred marker
    ///   never escapes the innermost block containing it, and, equally important, a
    ///   SIBLING block's marker (a container claims all of its items' markers at its
    ///   own `Start`) is not dragged into the block that happens to close first. See
    ///   [`drains_deferred`];
    /// - an EXCLUDED block's `Start` ([`opens_no_inject_region`]), with that
    ///   block's `range.end` — flushing the interior markers a container already
    ///   claimed ahead of the opening tag, since nothing may place them once the
    ///   block is open and its `End` would place them inside it.
    ///
    /// The bound is strict in both cases: `claim_before` only defers markers with
    /// `offset > block.start`, so an offset can never equal the range end of the
    /// block that owns it.
    fn drain_deferred_before(&mut self, end: usize) {
        while self.deferred.front().is_some_and(|(o, _, _)| *o < end) {
            if let Some((_, line, col)) = self.deferred.pop_front() {
                self.emit_marker(line, col);
            }
        }
    }

    /// Emits every remaining deferred marker. Used ONLY at stream end, before the
    /// trailing [`flush_before`](Self::flush_before), so nothing is ever dropped.
    fn drain_deferred_all(&mut self) {
        self.drain_deferred_before(usize::MAX);
    }

    /// Tries to place the deferred markers that fall STRICTLY inside `range` by
    /// splitting this `Text` event into `Text(prefix), Html(marker), …,
    /// Text(remainder)`.
    ///
    /// Returns `false` — the caller then emits the `Text` verbatim and the markers
    /// stay deferred for a later placement point — when:
    ///
    /// - injection is suppressed (`no_inject_depth > 0`), or
    /// - no deferred marker lies in `(range.start, range.end)`, or
    /// - the IDENTITY PREDICATE fails: `source[range] != payload`. A `Text` payload
    ///   is the source slice of its range for a real text item
    ///   (`parse.rs:2113`), but `SynthesizeText`/`SynthesizeChar` items — entity
    ///   references (`&amp;` → `"&"`), code/HTML indent padding — carry a payload
    ///   UNRELATED to the range (`parse.rs:2115-2116`). A length check is not
    ///   enough: some HTML5 named entities expand to a same-LENGTH string, so the
    ///   bytes are compared.
    ///
    /// Splitting is escaping-neutral: pulldown-cmark-escape escapes byte by byte
    /// with no cross-character state, so `escape(prefix) ++ escape(remainder)` is
    /// `escape(prefix ++ remainder)`.
    fn try_split_text(&mut self, text: &CowStr<'a>, range: &Range<usize>) -> bool {
        if self.no_inject_depth > 0 {
            return false;
        }
        // Is there anything to place, and is the range's payload trustworthy?
        if !self
            .deferred
            .front()
            .is_some_and(|(o, _, _)| *o > range.start && *o < range.end)
        {
            return false;
        }
        if self.source.get(range.start..range.end) != Some(text.as_ref()) {
            return false;
        }

        let mut cut = range.start;
        while let Some(&(offset, line, col)) = self.deferred.front() {
            if offset <= cut || offset >= range.end || !self.source.is_char_boundary(offset) {
                break;
            }
            let point = snap_split_point(self.source, range, offset, cut);
            if let Some(prefix) = self.source.get(cut..point) {
                if !prefix.is_empty() {
                    self.pending.push_back(Event::Text(prefix.into()));
                }
            }
            self.emit_marker(line, col);
            self.deferred.pop_front();
            cut = point;
        }

        if let Some(remainder) = self.source.get(cut..range.end) {
            if !remainder.is_empty() {
                self.pending.push_back(Event::Text(remainder.into()));
            }
        }
        true
    }

    /// Places every marker due at `event` and buffers `event` itself, expanding a
    /// `Text` that contains deferred markers into its split form. The whole
    /// placement rule lives here so [`Iterator::next`] stays a dispatcher.
    fn place(&mut self, event: Event<'a>, range: &Range<usize>) {
        // 1. Leave an excluded region BEFORE any drain, so a marker due right after
        //    a `</pre>` is emittable.
        if closes_no_inject_region(&event) {
            self.no_inject_depth = self.no_inject_depth.saturating_sub(1);
        }

        // 2. A block `End` drains the deferred markers that belong to THIS block
        //    (bounded by its own `range.end`), so none escapes it.
        if drains_deferred(&event) && self.no_inject_depth == 0 {
            self.drain_deferred_before(range.end);
        }

        // 3. A block `Start` first lets through anything due at or before its tag,
        //    then claims its own interior markers.
        if is_block_start(&event) {
            let opens_exclusion = opens_no_inject_region(&event);
            self.drain_deferred_at_or_before(range.start);
            if opens_exclusion {
                // Drain markers a container already deferred, ahead of the excluded block's tag; not
                // gated on `no_inject_depth`. See notes/crates/wolf-core/src/markdown.rs.md#nested-excluded-blocks
                self.drain_deferred_before(range.end);
            }
            self.claim_before(range, defers_interior_markers(&event));
            if opens_exclusion {
                self.no_inject_depth += 1;
            }
            self.pending.push_back(event);
            return;
        }

        // 4. An unsuppressed `Text` is the inline placement point: drain the markers due at or
        //    before it, then try to split it.
        if let Event::Text(text) = &event {
            if self.no_inject_depth == 0 && !self.deferred.is_empty() {
                self.drain_deferred_at_or_before(range.start);
                if self.try_split_text(text, range) {
                    return;
                }
            }
        }

        // 5. `Start(Image)` is not a block start, so its exclusion opens here.
        if opens_no_inject_region(&event) {
            self.no_inject_depth += 1;
        }

        self.pending.push_back(event);
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
                    // Every event goes through `pending`; returning one directly can reorder markers.
                    // See notes/crates/wolf-core/src/markdown.rs.md#every-event-goes-through-pending
                    self.place(event, &range);
                }
                None => {
                    if self.inner_done {
                        return None;
                    }
                    self.inner_done = true;
                    // Nothing is ever dropped: place what is still deferred, then
                    // flush every remaining marker as a trailing span.
                    self.drain_deferred_all();
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

    /// The per-100-column byte offset is a CHARACTER offset: for a line of
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

    /// `WidthMode::Capped` (the BROWSER preview's always-on mode) appends a
    /// `body.wolf-markdown` rule that caps the content to `max-width:45rem` (≈ 90
    /// proportional Inter chars) and centers it with `margin-left:auto` /
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

    /// The common case: lowercase and space→hyphen (`slugify` steps 1-2).
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

    /// An existing hyphen in the source text is preserved verbatim (`slugify` step 2: `'-'`
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
    /// `"Café Ω"` → `"café-ω"` (slugs are Unicode-preserving by design).
    #[test]
    fn slugify_unicode() {
        let mut seen = fresh_seen();
        assert_eq!(slugify("Café Ω", &mut seen), "café-ω");
    }

    /// A heading that strips to nothing (all punctuation) falls back to the base
    /// `"section"` (`slugify` step 3).
    #[test]
    fn slugify_empty_after_strip() {
        let mut seen = fresh_seen();
        assert_eq!(slugify("!!!", &mut seen), "section");
    }

    /// Duplicate headings dedup against the SHARED `seen` map in document order:
    /// the first `"Intro"` → `"intro"`, the second → `"intro-1"`, the third →
    /// `"intro-2"` (`slugify` step 4).
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

    /// An explicit `{#custom}` id is honored verbatim; it is also REGISTERED
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

    // Code-block highlighting. `start_col`/`end_col` are half-open byte columns; `row` is
    // 0-based within `text.split('\n')`.

    /// A single fenced ```` ```go ```` block enumerates to one `CodeBlockInfo` whose
    /// `lang` is the lowercased info-string token and whose `text` is the block body
    /// INCLUDING the trailing newline pulldown-cmark yields.
    #[test]
    fn code_blocks_enumerates_lang_and_text() {
        let blocks = code_blocks("# H\n\n```go\npackage main\n```\n");
        assert_eq!(blocks.len(), 1, "exactly one fenced code block: {blocks:?}");
        assert_eq!(
            blocks[0].lang.as_deref(),
            Some("go"),
            "the fenced info-string language, lowercased: {blocks:?}"
        );
        assert_eq!(
            blocks[0].text, "package main\n",
            "the block text is the concatenated body incl. the trailing newline: {blocks:?}"
        );
    }

    /// `code_blocks` enumerates every fenced AND indented block in DOCUMENT ORDER;
    /// an indented block (no info string) carries `lang == None`.
    #[test]
    fn code_blocks_document_order_and_indented() {
        // A fenced go block, a 4-space indented block, then a fenced rust block.
        let source = "```go\na\n```\n\n    indented\n\n```rust\nb\n```\n";
        let blocks = code_blocks(source);
        let langs: Vec<Option<&str>> = blocks.iter().map(|b| b.lang.as_deref()).collect();
        assert_eq!(
            langs,
            vec![Some("go"), None, Some("rust")],
            "blocks enumerate in document order; the indented block has no lang: {blocks:?}"
        );
    }

    /// A bare fence (no info string) has `lang == None`.
    #[test]
    fn code_blocks_bare_fence_is_none() {
        let blocks = code_blocks("```\nplain text\n```\n");
        assert_eq!(blocks.len(), 1, "one bare fenced block: {blocks:?}");
        assert_eq!(
            blocks[0].lang, None,
            "a bare fence carries no language: {blocks:?}"
        );
    }

    /// With highlight data for the block, the interior code is emitted as inline
    /// styled `<span>`s in the EXACT `style_css` order, still wrapped in the plain
    /// `<pre><code class="language-go">`.
    #[test]
    fn render_html_with_highlights_emits_styled_spans() {
        let src = "```go\npackage foo\n```\n";
        let block = BlockHighlight {
            spans: vec![HighlightSpan {
                row: 0,
                start_col: 0,
                end_col: 7,
                style: HighlightStyle {
                    color: Some(0x00FF_8800),
                    bold: true,
                    italic: false,
                    underline: false,
                },
            }],
        };
        let html = render_html_with_highlights(
            src,
            PreviewTheme::Default,
            WidthMode::Full,
            &[Some(block)],
        );
        assert!(
            html.contains(r#"<span style="color:#ff8800;font-weight:bold;">package</span>"#),
            "the `package` token must render as an inline-styled span (color + bold): {html}"
        );
        assert!(
            html.contains("<pre"),
            "the highlighted block must still be wrapped in <pre>: {html}"
        );
        assert!(
            html.contains(r#"class="language-go""#),
            "the highlighted block must keep its `class=\"language-go\"`: {html}"
        );
    }

    /// The highlighted interior text is HTML-ESCAPED — both the span-covered token
    /// and the surrounding gap text — so `<`/`&`/`>` become `&lt;`/`&amp;`/`&gt;`
    /// and never survive raw.
    #[test]
    fn render_html_with_highlights_escapes_text() {
        let src = "```go\nx < y & z > w\n```\n";
        // A span over the leading `x`, so both the span body and the gap text pass
        // through the escaper.
        let block = BlockHighlight {
            spans: vec![HighlightSpan {
                row: 0,
                start_col: 0,
                end_col: 1,
                style: HighlightStyle {
                    color: Some(0x0000_FF00),
                    bold: false,
                    italic: false,
                    underline: false,
                },
            }],
        };
        let html = render_html_with_highlights(
            src,
            PreviewTheme::Default,
            WidthMode::Full,
            &[Some(block)],
        );
        assert!(
            html.contains("&lt;"),
            "`<` must be escaped to `&lt;`: {html}"
        );
        assert!(
            html.contains("&amp;"),
            "`&` must be escaped to `&amp;`: {html}"
        );
        assert!(
            html.contains("&gt;"),
            "`>` must be escaped to `&gt;`: {html}"
        );
        assert!(
            !html.contains("x < y"),
            "the raw `<` must not survive unescaped: {html}"
        );
        assert!(
            !html.contains("z > w"),
            "the raw `>` must not survive unescaped: {html}"
        );
    }

    /// A `None` highlight slot for a block renders it PLAIN — byte-identical body to
    /// the 3-arg `render_html`, with no inline `<span style=` anywhere.
    #[test]
    fn render_html_with_highlights_none_is_plain() {
        let src = "```go\npackage foo\n```\n";
        let plain = render_html(src, PreviewTheme::Default, WidthMode::Full);
        let with =
            render_html_with_highlights(src, PreviewTheme::Default, WidthMode::Full, &[None]);
        assert_eq!(
            with, plain,
            "a `None` highlight slot must reproduce the plain render exactly"
        );
        assert!(
            !with.contains("<span style="),
            "a `None` highlight slot must emit no inline-styled span: {with}"
        );
    }

    /// An EMPTY highlight slice makes every block plain — `render_html_with_highlights`
    /// with `&[]` is byte-for-byte identical to `render_html` for a doc with a code
    /// block (the delegation contract that gives existing call sites zero churn).
    #[test]
    fn render_html_empty_highlights_matches_render_html() {
        let src = "# H\n\n```go\npackage main\nfunc f() {}\n```\n";
        let empty = render_html_with_highlights(src, PreviewTheme::Default, WidthMode::Full, &[]);
        let plain = render_html(src, PreviewTheme::Default, WidthMode::Full);
        assert_eq!(
            empty, plain,
            "render_html_with_highlights(src, .., &[]) must byte-equal render_html(src, ..)"
        );
    }

    /// A highlighted block still carries NO `wolf-src` marker inside its `<pre>`
    /// (markers collapse to before the block), while the line's `wl-1-0` anchor
    /// still exists elsewhere in the document.
    #[test]
    fn highlighted_block_has_no_marker_inside_pre() {
        let src = "# H\n\n```go\npackage main\nfunc f() {}\n```\n";
        let block = BlockHighlight {
            spans: vec![HighlightSpan {
                row: 0,
                start_col: 0,
                end_col: 7,
                style: HighlightStyle {
                    color: Some(0x0000_FF00),
                    bold: false,
                    italic: false,
                    underline: false,
                },
            }],
        };
        let html = render_html_with_highlights(
            src,
            PreviewTheme::Default,
            WidthMode::Full,
            &[Some(block)],
        );
        let pre_start = html
            .find("<pre")
            .unwrap_or_else(|| panic!("missing <pre> for the highlighted block in: {html}"));
        let pre_end = html
            .find("</pre>")
            .unwrap_or_else(|| panic!("missing </pre> in: {html}"));
        let inside = &html[pre_start..pre_end];
        assert!(
            !inside.contains("wolf-src"),
            "no `wolf-src` marker span may appear inside the highlighted <pre>: {inside}"
        );
        assert!(
            html.contains(r#"id="wl-1-0""#),
            "the first hard line's `wl-1-0` anchor must still exist elsewhere: {html}"
        );
    }

    /// Deliberately MALFORMED highlight data — overlapping and out-of-range spans —
    /// must not panic and must still emit well-formed, NON-NESTED spans (the
    /// defensive clamp/overlap guard in `build_highlighted_html`).
    #[test]
    fn render_html_with_highlights_defensive_overlap() {
        let src = "```go\npackage foo\n```\n";
        // Row 0 is "package foo" (len 11). These spans overlap each other and the
        // last runs past the line end — the renderer must cope.
        let block = BlockHighlight {
            spans: vec![
                HighlightSpan {
                    row: 0,
                    start_col: 0,
                    end_col: 7,
                    style: HighlightStyle {
                        color: Some(0x0000_FF00),
                        bold: false,
                        italic: false,
                        underline: false,
                    },
                },
                HighlightSpan {
                    row: 0,
                    start_col: 3,
                    end_col: 10,
                    style: HighlightStyle {
                        color: Some(0x00FF_0000),
                        bold: false,
                        italic: false,
                        underline: false,
                    },
                },
                HighlightSpan {
                    row: 0,
                    start_col: 5,
                    end_col: 999,
                    style: HighlightStyle {
                        color: Some(0x0000_00FF),
                        bold: false,
                        italic: false,
                        underline: false,
                    },
                },
            ],
        };
        // Must not panic.
        let html = render_html_with_highlights(
            src,
            PreviewTheme::Default,
            WidthMode::Full,
            &[Some(block)],
        );
        let pre_start = html
            .find("<pre")
            .unwrap_or_else(|| panic!("missing <pre> in: {html}"));
        let pre_end = html
            .find("</pre>")
            .unwrap_or_else(|| panic!("missing </pre> in: {html}"));
        let inside = &html[pre_start..pre_end];

        // `match_indices("<span")` never matches the `</span>` close (its char after
        // `<` is `/`), so opens counts only span openings.
        let opens = inside.matches("<span").count();
        let closes = inside.matches("</span>").count();
        assert_eq!(
            opens, closes,
            "every highlight <span> must be closed (well-formed): {inside}"
        );

        // Non-nested: walking the interleaved open/close tags in position order, the
        // depth must never exceed 1 and must return to 0.
        let mut events: Vec<(usize, i32)> = Vec::new();
        events.extend(inside.match_indices("<span").map(|(i, _)| (i, 1)));
        events.extend(inside.match_indices("</span>").map(|(i, _)| (i, -1)));
        events.sort_by_key(|(i, _)| *i);
        let mut depth = 0i32;
        for (_, delta) in events {
            depth += delta;
            assert!(
                (0..=1).contains(&depth),
                "highlight spans must not nest (depth {depth}): {inside}"
            );
        }
        assert_eq!(depth, 0, "highlight spans must balance: {inside}");
    }

    // Inline-level marker anchoring.
    // See notes/wolf-core-decisions.md#the-preview-position-marker-contract

    /// The rendered document's `<body …>` INNER html: the slice between the body
    /// start tag's `>` and the closing `</body>`.
    ///
    /// Every assertion in this section goes through it, because the `<style>`
    /// block mentions `.wolf-src`, `p`, `pre`, `table`, … and would otherwise
    /// poison a bare `contains`/`find` on the whole document.
    fn body_of(html: &str) -> &str {
        let open = html
            .find("<body")
            .unwrap_or_else(|| panic!("the rendered document must contain a <body>: {html}"));
        let open_end = html[open..]
            .find('>')
            .unwrap_or_else(|| panic!("the <body> start tag must be closed: {html}"));
        let start = open + open_end + 1;
        let end = html[start..]
            .find("</body>")
            .unwrap_or_else(|| panic!("the <body> must be closed: {html}"));
        &html[start..start + end]
    }

    /// `s` with every `<span class="wolf-src" …></span>` anchor removed, leaving
    /// only the markup the anchors are woven into.
    ///
    /// The whole point of inline anchoring is that it changes DOM PLACEMENT and
    /// nothing else, so stripping the anchors must reproduce a vanilla render.
    fn strip_marker_spans(s: &str) -> String {
        const OPEN: &str = "<span class=\"wolf-src\"";
        const CLOSE: &str = "</span>";

        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(at) = rest.find(OPEN) {
            out.push_str(&rest[..at]);
            let after = &rest[at..];
            let close = after
                .find(CLOSE)
                .unwrap_or_else(|| panic!("an anchor span must be closed by `</span>`: {after}"));
            rest = &after[close + CLOSE.len()..];
        }
        out.push_str(rest);
        out
    }

    /// U1: a single-line paragraph longer than two strides puts its `wl-1-100` and
    /// `wl-1-200` anchors INSIDE the `<p>` (in source order), while `wl-1-0` keeps
    /// its block-top placement before the `<p>`. This is the whole point of the
    /// change: a cursor near the end of the line must resolve to a rect deep in the
    /// paragraph, not to the paragraph's top.
    #[test]
    fn column_marker_lands_inside_the_paragraph() {
        // `"lorem ipsum "` is 12 characters, so this is one 240-character hard line.
        let source = "lorem ipsum ".repeat(20);
        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let p_open = body
            .find("<p>")
            .unwrap_or_else(|| panic!("the long line must render one <p>: {body}"));
        let p_close = body
            .find("</p>")
            .unwrap_or_else(|| panic!("the <p> must be closed: {body}"));
        let zero = body
            .find(r#"id="wl-1-0""#)
            .unwrap_or_else(|| panic!("missing the `wl-1-0` anchor: {body}"));
        let hundred = body
            .find(r#"id="wl-1-100""#)
            .unwrap_or_else(|| panic!("missing the `wl-1-100` anchor: {body}"));
        let two_hundred = body
            .find(r#"id="wl-1-200""#)
            .unwrap_or_else(|| panic!("missing the `wl-1-200` anchor: {body}"));

        assert!(
            zero < p_open,
            "the column-ZERO anchor keeps its block-top placement: `wl-1-0` at \
             {zero} must precede the <p> at {p_open}: {body}"
        );
        assert!(
            p_open < hundred && hundred < p_close,
            "`wl-1-100` (at {hundred}) must sit INSIDE the paragraph's inline \
             content (<p> at {p_open}, </p> at {p_close}): {body}"
        );
        assert!(
            p_open < two_hundred && two_hundred < p_close,
            "`wl-1-200` (at {two_hundred}) must sit INSIDE the paragraph's inline \
             content (<p> at {p_open}, </p> at {p_close}): {body}"
        );
        assert!(
            hundred < two_hundred,
            "the interior anchors must stay in source order: `wl-1-100` at \
             {hundred} must precede `wl-1-200` at {two_hundred}: {body}"
        );
    }

    /// U2: the split point SNAPS to the nearest word boundary (so no glyph-shaping
    /// pair is broken by the inline box), while the anchor's `id` / `data-line` /
    /// `data-col` stay at the EXACT `(L, C)`.
    ///
    /// Fixture arithmetic for `"lorem ipsum ".repeat(20)` (12 ASCII characters per
    /// repeat, so byte offset == character column):
    ///
    /// - Repeat #9 covers bytes `96..108` = `l o r e m ␠ i p s u m ␠`. Column 100 is
    ///   therefore the `m` that ENDS `lorem`. The nearest word boundary is byte 101
    ///   (the space starts there), 1 byte away — the alternatives are 102 (distance
    ///   2), 96 and 95 (distances 4 and 5). So the split snaps FORWARD to 101 and
    ///   the anchor lands between `lorem` and `␠ipsum`.
    /// - Repeat #17 covers bytes `192..204`. Column 200 is the `s` of `ipsum`
    ///   (`i`=198, `p`=199, `s`=200). The nearest boundary is byte 198 (the space at
    ///   197 ends there), 2 bytes away; 197 and 203 are both 3 away. So the split
    ///   snaps BACKWARD to 198 and the anchor lands between `lorem␠` and `ipsum`.
    ///
    /// The two directions are asserted together on purpose: a splitter that only
    /// ever scanned forward (or only backward) would pass one and fail the other.
    #[test]
    fn split_snaps_to_the_nearest_word_boundary() {
        let source = "lorem ipsum ".repeat(20);
        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let snapped_forward = format!("lorem{} ipsum", marker_span(1, 100));
        assert!(
            body.contains(&snapped_forward),
            "the `wl-1-100` anchor must snap FORWARD from column 100 (the `m` \
             ending `lorem`) to the word boundary one byte later, landing between \
             `lorem` and ` ipsum` — expected the substring `{snapped_forward}` \
             in: {body}"
        );

        let snapped_backward = format!("lorem {}ipsum", marker_span(1, 200));
        assert!(
            body.contains(&snapped_backward),
            "the `wl-1-200` anchor must snap BACKWARD from column 200 (the `s` of \
             `ipsum`) to the word boundary two bytes earlier, landing between \
             `lorem ` and `ipsum` — expected the substring `{snapped_backward}` \
             in: {body}"
        );
    }

    /// U3: with no word boundary anywhere in the snap window, the split happens at
    /// the EXACT byte offset. A 300-`a` paragraph therefore renders as three
    /// 100-character runs separated by the two interior anchors — asserted as an
    /// exact body, so nothing else may be inserted, duplicated or dropped.
    ///
    /// The `\n` before `<p>` and the one after `</p>` are pulldown-cmark's, and the
    /// leading one is a PRE-EXISTING artifact of flushing a raw span before a block
    /// tag — it is NOT produced by inline anchoring, and the same `\n` is already
    /// emitted today for the column-zero anchor. The chain: `html.rs:65` seeds
    /// `end_newline = true`; `html.rs:86-88`'s `write` clears it when the anchor
    /// span (which does not end in a newline) is written; `html.rs:163-167` then
    /// emits `"\n<p>"` rather than `"<p>"`. `TagEnd::Paragraph` (`html.rs:394-396`)
    /// always writes `"</p>\n"`, anchors or not.
    #[test]
    fn unbroken_token_splits_at_the_exact_offset() {
        const RUNS: usize = 3;
        let chunk = "a".repeat(MARKER_COLUMN_STRIDE);
        let source = chunk.repeat(RUNS);

        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let expected = format!(
            "{zero}\n<p>{chunk}{hundred}{chunk}{two_hundred}{chunk}</p>\n",
            zero = marker_span(1, 0),
            hundred = marker_span(1, 100),
            two_hundred = marker_span(1, 200),
        );
        assert_eq!(
            body, expected,
            "a 300-character unbroken token has no word boundary within the snap \
             window, so each interior anchor must split the text at its EXACT \
             column offset (100 and 200), leaving three untouched 100-character \
             runs and the column-zero anchor still ahead of the <p>"
        );
    }

    /// U4: several markers inside ONE `Event::Text` all get placed — each exactly
    /// once, at strictly increasing positions, all within the paragraph. This is
    /// the split loop's guard against emitting a marker twice, dropping the tail of
    /// the queue, or letting two markers collapse onto the same point.
    #[test]
    fn markers_in_one_text_event_stay_ordered_and_unique() {
        const RUNS: usize = 4;
        const TAIL: usize = 50;
        let source = "a".repeat(RUNS * MARKER_COLUMN_STRIDE + TAIL);

        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let p_open = body
            .find("<p>")
            .unwrap_or_else(|| panic!("the long line must render one <p>: {body}"));
        let p_close = body
            .find("</p>")
            .unwrap_or_else(|| panic!("the <p> must be closed: {body}"));

        let mut previous = p_open;
        for col in 1..=RUNS {
            let id = format!(r#"id="wl-1-{}""#, col * MARKER_COLUMN_STRIDE);
            assert_eq!(
                body.matches(&id).count(),
                1,
                "`{id}` must be emitted EXACTLY once: {body}"
            );
            let idx = body
                .find(&id)
                .unwrap_or_else(|| panic!("missing the `{id}` anchor: {body}"));
            assert!(
                previous < idx && idx < p_close,
                "`{id}` (at {idx}) must sit inside the paragraph and after the \
                 previous anchor (at {previous}, </p> at {p_close}): {body}"
            );
            previous = idx;
        }
    }

    /// U5: column-ZERO markers are untouched by this change — a three-hard-line
    /// paragraph still collapses all three `wl-{L}-0` anchors to before the single
    /// `<p>`. The front-end's tier-4 sweep filters on `[data-col="0"]` and depends
    /// on exactly this.
    #[test]
    fn column_zero_markers_keep_block_placement() {
        let html = render_html("foo\nbar\nbaz", PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let p_open = body
            .find("<p>")
            .unwrap_or_else(|| panic!("the three lines must render one <p>: {body}"));
        for line in 1..=3u32 {
            let id = format!(r#"id="wl-{line}-0""#);
            let idx = body
                .find(&id)
                .unwrap_or_else(|| panic!("missing the `{id}` anchor: {body}"));
            assert!(
                idx < p_open,
                "`{id}` (at {idx}) must still collapse to before the <p> (at \
                 {p_open}): {body}"
            );
        }
    }

    /// U6: a PLAIN (unhighlighted) fenced code block still yields `Event::Text`, and
    /// inside a fence one `Text` event can span a whole line including its `\n` — so
    /// without the code-block exclusion an interior column marker would be "strictly
    /// inside" it and would corrupt the rendered code. Nothing may land in `<pre>`,
    /// and the marker must still exist elsewhere (never dropped).
    #[test]
    fn plain_code_block_takes_no_inline_marker() {
        let code_line = "x".repeat(2 * MARKER_COLUMN_STRIDE);
        // Hard lines: 1 = "```", 2 = the 200-character body line, 3 = "code two",
        // 4 = "```", 5 = "" (the trailing newline).
        let source = format!("```\n{code_line}\ncode two\n```\n");

        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let pre_start = body
            .find("<pre")
            .unwrap_or_else(|| panic!("missing <pre> for the fenced block: {body}"));
        let pre_end = body
            .find("</pre>")
            .unwrap_or_else(|| panic!("missing </pre>: {body}"));
        let inside = &body[pre_start..pre_end];
        assert!(
            !inside.contains("wolf-src"),
            "no `wolf-src` anchor may be split into a plain <pre><code>: {inside}"
        );
        assert!(
            !inside.contains("wl-"),
            "no `wl-` anchor id may appear inside a plain <pre><code>: {inside}"
        );
        assert!(
            body.contains(r#"id="wl-2-100""#),
            "the code line's `wl-2-100` anchor must still EXIST outside the <pre> \
             (excluded from the block, not dropped): {body}"
        );
    }

    /// U7: the HIGHLIGHTED path is the other half of the code-block exclusion.
    /// `CodeHighlighter` rewrites the block interior to a single `Event::InlineHtml`
    /// (never splittable) but passes `Start`/`End(CodeBlock)` through, so the
    /// exclusion depth still brackets the block.
    #[test]
    fn highlighted_code_block_takes_no_inline_marker() {
        const HIGHLIGHT_END_COL: u32 = 10;
        let code_line = "x".repeat(2 * MARKER_COLUMN_STRIDE);
        let source = format!("```go\n{code_line}\ncode two\n```\n");
        let block = BlockHighlight {
            spans: vec![HighlightSpan {
                row: 0,
                start_col: 0,
                end_col: HIGHLIGHT_END_COL,
                style: HighlightStyle {
                    color: Some(0x0000_FF00),
                    bold: false,
                    italic: false,
                    underline: false,
                },
            }],
        };

        let html = render_html_with_highlights(
            &source,
            PreviewTheme::Default,
            WidthMode::Full,
            &[Some(block)],
        );
        let body = body_of(&html);

        let pre_start = body
            .find("<pre")
            .unwrap_or_else(|| panic!("missing <pre> for the highlighted block: {body}"));
        let pre_end = body
            .find("</pre>")
            .unwrap_or_else(|| panic!("missing </pre>: {body}"));
        let inside = &body[pre_start..pre_end];
        assert!(
            !inside.contains("wolf-src"),
            "no `wolf-src` anchor may land inside a HIGHLIGHTED <pre><code>: {inside}"
        );
        assert!(
            !inside.contains("wl-"),
            "no `wl-` anchor id may appear inside a HIGHLIGHTED <pre><code>: {inside}"
        );
        assert!(
            inside.contains("<span style="),
            "fixture sanity: the block must actually be highlighted (inline-styled \
             spans present): {inside}"
        );
        assert!(
            body.contains(r#"id="wl-2-100""#),
            "the code line's `wl-2-100` anchor must still EXIST outside the <pre>: {body}"
        );
    }

    /// U8: image alt text is a VERIFIED corruption hazard. `html.rs`'s `raw_text()`
    /// (used to build the `alt` attribute) DROPS `Event::Html` outright and escapes
    /// `Event::Text`/`InlineHtml` into the attribute — so a marker leaking into an
    /// image would either vanish (breaking its id's existence guarantee) or be
    /// baked into `alt="…"`. Neither may happen: the anchor must stay out of the
    /// `<img …/>` tag AND still exist in the document.
    #[test]
    fn image_alt_text_keeps_its_marker_out_of_the_attribute() {
        // `"alt word "` is 9 characters, so the alt text spans source bytes
        // 2..182 and character column 100 falls well inside it.
        let source = format!("![{}](i.png)", "alt word ".repeat(20));
        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let img_start = body
            .find("<img")
            .unwrap_or_else(|| panic!("missing the <img> element: {body}"));
        let img_len = body[img_start..]
            .find("/>")
            .unwrap_or_else(|| panic!("the <img> element must be closed with `/>`: {body}"));
        let img_tag = &body[img_start..img_start + img_len + "/>".len()];
        assert!(
            !img_tag.contains("wolf-src"),
            "no `wolf-src` anchor may be escaped into the image's alt attribute: {img_tag}"
        );
        assert!(
            !img_tag.contains("wl-"),
            "no `wl-` anchor id may appear inside the <img> tag: {img_tag}"
        );
        assert!(
            body.contains(r#"id="wl-1-100""#),
            "the anchor must still exist in the document — if it is missing, it \
             leaked into the image and `raw_text()`'s `Html(_) => {{}}` arm \
             silently dropped it: {body}"
        );
    }

    /// U9: a marker must never reach a table interior. A raw span between `<tbody>`
    /// and `<tr>` is foster-parented out of the table by the HTML parser, so tables
    /// keep the pre-change behavior: every interior marker is flushed BEFORE the
    /// opening `<table>`.
    #[test]
    fn table_interior_takes_no_inline_marker() {
        // Hard line 3 is `| {130 w's} | y |` — 138 characters, so it earns a
        // `wl-3-100` marker whose offset is inside the table's source range.
        let long_cell = "w".repeat(130);
        let source = format!("| a | b |\n| --- | --- |\n| {long_cell} | y |\n| c | d |\n");

        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let table_start = body
            .find("<table")
            .unwrap_or_else(|| panic!("missing the <table>: {body}"));
        let table_end = body
            .find("</table>")
            .unwrap_or_else(|| panic!("missing </table>: {body}"));
        let inside = &body[table_start..table_end];
        assert!(
            !inside.contains("wolf-src"),
            "no `wolf-src` anchor may be placed inside a <table> (it would be \
             foster-parented out of the table by the HTML parser): {inside}"
        );

        let marker = body
            .find(r#"id="wl-3-100""#)
            .unwrap_or_else(|| panic!("missing the `wl-3-100` anchor: {body}"));
        assert!(
            marker < table_start,
            "the long cell's `wl-3-100` anchor (at {marker}) must keep its \
             flush-before-the-block placement, ahead of the <table> at \
             {table_start}: {body}"
        );
    }

    /// U10: an HTML block yields only `Event::Html`, so there is nothing to split —
    /// but the exclusion also has to stop a deferred marker from being emitted
    /// BETWEEN two raw HTML lines, which would inject a span into the author's own
    /// markup.
    #[test]
    fn html_block_is_not_split() {
        const RUN_LEN: usize = 150;
        let run = "h".repeat(RUN_LEN);
        let source = format!("<div>\n{run}\n</div>\n");

        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let div_start = body
            .find("<div>")
            .unwrap_or_else(|| panic!("the HTML block must pass through verbatim: {body}"));
        let div_end = body
            .find("</div>")
            .unwrap_or_else(|| panic!("missing the HTML block's </div>: {body}"));
        let inside = &body[div_start..div_end];
        assert!(
            !inside.contains("wolf-src"),
            "no `wolf-src` anchor may be placed inside a raw HTML block: {inside}"
        );
        assert!(
            body.contains(&run),
            "the HTML block's {RUN_LEN}-character run must survive verbatim: {body}"
        );
    }

    /// U11: character column 100 falls inside a LINK DESTINATION, a source region
    /// covered by no `Event::Text` at all. The marker must not be dropped and must
    /// not end up inside the `href` attribute; it lands at the next legal point
    /// inside the containing paragraph.
    #[test]
    fn link_destination_gap_marker_lands_inside_the_paragraph() {
        // `[text](https://example.com/` is 27 bytes, so the 150-`p` path occupies
        // bytes 27..177 and character column 100 sits inside the URL.
        let source = format!("[text](https://example.com/{})", "p".repeat(150));
        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let marker = body
            .find(r#"id="wl-1-100""#)
            .unwrap_or_else(|| panic!("missing the `wl-1-100` anchor: {body}"));
        let a_close = body
            .find("</a>")
            .unwrap_or_else(|| panic!("missing the link's </a>: {body}"));
        let p_close = body
            .find("</p>")
            .unwrap_or_else(|| panic!("missing the paragraph's </p>: {body}"));
        assert!(
            a_close < marker && marker < p_close,
            "a marker whose offset falls in the link DESTINATION (a gap covered by \
             no Text event) must land after the </a> (at {a_close}) and before the \
             </p> (at {p_close}), not be dropped — found at {marker}: {body}"
        );

        let href_start = body
            .find("href=\"")
            .unwrap_or_else(|| panic!("missing the link's href: {body}"));
        let href_rest = &body[href_start + "href=\"".len()..];
        let href_end = href_rest
            .find('"')
            .unwrap_or_else(|| panic!("the href attribute must be closed: {body}"));
        let href = &href_rest[..href_end];
        assert!(
            !href.contains("wolf-src") && !href.contains("wl-"),
            "the href attribute must never receive an anchor span: {href}"
        );
    }

    /// U12: an entity reference is a `SynthesizeText` item whose payload
    /// (`"&"`) is UNRELATED to its 5-byte source range (`"&amp;"`), so the exact
    /// `source[range] == payload` identity predicate must reject it and the marker
    /// must fall through to the next legal point rather than slicing the entity in
    /// half (or double-escaping it).
    #[test]
    fn entity_reference_is_not_split() {
        // 98 filler characters put `&amp;` at bytes 98..103, so character column
        // 100 (the `m`) is STRICTLY inside the entity's source span.
        let source = format!("{}&amp;{}", "a".repeat(98), "a".repeat(60));
        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        assert!(
            body.contains("a&amp;"),
            "the rendered entity must stay intact and un-split, directly following \
             the filler text: {body}"
        );
        assert_eq!(
            body.matches("&amp;").count(),
            1,
            "the source's single `&amp;` must render exactly once: {body}"
        );
        assert!(
            !body.contains("&amp;amp;"),
            "the entity must not be double-escaped: {body}"
        );
        assert!(
            body.contains(r#"id="wl-1-100""#),
            "a marker that cannot be placed by splitting must still be emitted \
             somewhere (never dropped): {body}"
        );
    }

    /// U13: a backslash escape closes the current text item and starts the next one
    /// AFTER the backslash, so the backslash byte is covered by no event at all.
    /// A marker landing on that seam must not resurrect the escape (turning
    /// `\*stars\*` into `<em>stars</em>`) or split the escaped run.
    #[test]
    fn backslash_escape_is_not_corrupted() {
        // 99 filler characters, then `\*stars\*` at bytes 99..108: the backslash is
        // byte 99 and character column 100 is the escaped `*` that opens the run.
        let source = format!("{}\\*stars\\*{}", "a".repeat(99), "a".repeat(60));
        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        assert!(
            body.contains("*stars*"),
            "the escaped `\\*stars\\*` must render as the literal `*stars*`, \
             uninterrupted by an anchor span: {body}"
        );
        assert!(
            !body.contains("<em>"),
            "the backslash escapes must still suppress emphasis (no <em>): {body}"
        );
        assert!(
            body.contains(r#"id="wl-1-100""#),
            "the marker must still be emitted: {body}"
        );
    }

    /// A deliberately hostile single fixture for [`stripping_marker_spans_reproduces_a_vanilla_render`]:
    /// a >300-character paragraph carrying strong/emphasis, a code span, a long
    /// link, an image with long alt text, backslash escapes, an entity reference, a
    /// multibyte run and a hard break, followed by a fenced code block with a
    /// 150-character line, a table with a long cell, and a raw HTML block.
    ///
    /// It deliberately contains NO heading and NO math: `HeadingIdAssigner` adds
    /// slug ids and `MathRenderer` rewrites math to MathML, so either one would make
    /// the render legitimately differ from a vanilla `html::push_html` for reasons
    /// that have nothing to do with markers.
    fn marker_stress_fixture() -> String {
        let filler = "lorem ipsum dolor sit amet ".repeat(6);
        let link_path = "q".repeat(60);
        let alt = "alt word ".repeat(12);
        let multibyte = "é".repeat(40);
        let code_line = "z".repeat(150);
        let long_cell = "w".repeat(130);
        let html_run = "h".repeat(120);

        let mut source = String::new();
        source.push_str(&filler);
        source.push_str("**bold** and _em_ and `code span` and ");
        source.push_str(&format!("[link](https://example.com/{link_path}) and "));
        source.push_str(&format!("![{alt}](pic.png) and "));
        source.push_str("\\*escapes\\* and &amp; and ");
        source.push_str(&multibyte);
        // Two trailing spaces: a HARD break, so this paragraph spans two hard lines.
        source.push_str(" tail  \n");
        source.push_str("second hard line of the same paragraph\n\n");
        source.push_str(&format!("```text\n{code_line}\n```\n\n"));
        source.push_str(&format!(
            "| a | b |\n| --- | --- |\n| {long_cell} | y |\n\n"
        ));
        source.push_str(&format!("<div>\n{html_run}\n</div>\n"));
        source
    }

    /// U14: the strongest guard. Stripping every anchor span out of the rendered
    /// body must reproduce a VANILLA `html::push_html` render byte for byte — i.e.
    /// inline anchoring changes DOM placement and nothing else: no text is
    /// duplicated, dropped, re-escaped, or moved.
    #[test]
    fn stripping_marker_spans_reproduces_a_vanilla_render() {
        let source = marker_stress_fixture();
        let rendered = render_html(&source, PreviewTheme::Default, WidthMode::Full);

        let mut vanilla = String::new();
        html::push_html(&mut vanilla, Parser::new_ext(&source, markdown_options()));

        // Required: after a raw anchor span pulldown-cmark writes a block tag as `\n<p>`, which is
        // inter-tag whitespace; both sides are normalized, so any real difference still fails.
        let stripped = strip_marker_spans(body_of(&rendered)).replace('\n', "");
        let expected = vanilla.replace('\n', "");
        assert_eq!(
            stripped, expected,
            "with the anchor spans stripped, the rendered body must be identical \
             to a vanilla pulldown-cmark render — inline anchoring may change DOM \
             PLACEMENT only, never the rendered content"
        );
    }

    /// The HTML5 named entity `&nvap;`, whose expansion is **the same byte length
    /// as its source**: 6 source bytes (`&`, `n`, `v`, `a`, `p`, `;`) expand to
    /// `U+224D U+20D2`, two 3-byte UTF-8 characters — 6 bytes.
    ///
    /// Verified in the vendored engine: `pulldown-cmark-0.12.2/src/entities.rs:1547`
    /// maps `b"nvap"` to `"\u{224D}\u{20D2}"`, and `scanners.rs:901-928`'s
    /// `scan_entity` returns `(end + 1, Some(value))` — i.e. the item range spans
    /// the full `&nvap;`, while the payload is the expansion.
    const SAME_LENGTH_ENTITY_SOURCE: &str = "&nvap;";

    /// What [`SAME_LENGTH_ENTITY_SOURCE`] renders to: `≍` (U+224D) immediately
    /// followed by its combining overlay (U+20D2). They must stay ADJACENT — an
    /// anchor span wedged between a base character and its combining mark breaks
    /// the grapheme.
    const SAME_LENGTH_ENTITY_RENDERED: &str = "\u{224D}\u{20D2}";

    /// G2: the identity predicate must be an EXACT byte compare of
    /// `source[range]` against the payload, not the `range.len() == text.len()`
    /// heuristic.
    ///
    /// `&amp;` (U12) cannot tell the two apart: 5 source bytes → 1 payload byte
    /// fails a length check just as it fails a byte check. `&nvap;` can — its
    /// source and payload are both 6 bytes, so ONLY the byte compare rejects it.
    ///
    /// The consequence of getting this wrong is concrete, not theoretical.
    /// [`MarkerInjector::try_split_text`] slices **`self.source`**, not the payload,
    /// so a length-gated variant would accept this range and then cut the SOURCE at
    /// the split point — emitting `Text("&nv")`, the anchor, `Text("ap;")`. The HTML
    /// writer escapes both halves, so the document ends up carrying a literal
    /// `&amp;nvap;` and the rendered `≍⃒` grapheme never appears at all.
    #[test]
    fn same_length_entity_is_not_split() {
        assert_eq!(
            SAME_LENGTH_ENTITY_SOURCE.len(),
            SAME_LENGTH_ENTITY_RENDERED.len(),
            "fixture sanity: this test only distinguishes the byte predicate from a \
             length check while the entity's source and expansion are the SAME byte \
             length; pick another same-length entity if this ever changes"
        );

        // 97 filler characters put `&nvap;` at bytes 97..103, so character column
        // 100 (the `a` of `nvap`) is STRICTLY inside the entity's source span.
        let source = format!(
            "{}{SAME_LENGTH_ENTITY_SOURCE}{}",
            "a".repeat(97),
            "a".repeat(60)
        );
        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        assert!(
            body.contains(SAME_LENGTH_ENTITY_RENDERED),
            "`{SAME_LENGTH_ENTITY_SOURCE}` must render as an intact grapheme — its \
             base character and combining mark ADJACENT, with no anchor span \
             between them. A splitter gated on `range.len() == text.len()` instead \
             of the exact `source[range] == payload` compare accepts this range \
             (both sides are 6 bytes) and then slices the SOURCE, emitting `&nv` + \
             the anchor + `ap;` — which the HTML writer escapes into a literal \
             `&amp;nvap;`, so the grapheme is gone entirely: {body}"
        );
        assert!(
            body.contains(&format!("a{SAME_LENGTH_ENTITY_RENDERED}")),
            "nothing may be inserted between the filler text and the rendered \
             entity: {body}"
        );
        assert!(
            !body.contains(SAME_LENGTH_ENTITY_SOURCE),
            "fixture sanity: the entity must actually be EXPANDED by the parser (if \
             the raw `{SAME_LENGTH_ENTITY_SOURCE}` survives, the fixture is not \
             exercising the `SynthesizeText` path at all): {body}"
        );
        assert!(
            body.contains(r#"id="wl-1-100""#),
            "a marker that cannot be placed by splitting must still be emitted \
             somewhere (never dropped): {body}"
        );
    }

    /// Length of the multibyte run in [`snap_split_point_honors_its_contract`]:
    /// 40 × `あ` (3 UTF-8 bytes each) = 120 bytes, comfortably more than two
    /// [`MARKER_SNAP_MAX_BYTES`] windows, so the whitespace at either end of the
    /// fixture is out of reach of a split point in the middle.
    const MULTIBYTE_RUN_CHARS: usize = 40;

    /// G3: [`snap_split_point`]'s contract, pinned directly rather than inferred
    /// from a rendered document.
    ///
    /// Its documented postcondition is
    /// `min_exclusive < result < range.end && source.is_char_boundary(result)`;
    /// every case below asserts that in addition to the exact expected offset. The
    /// cases are the adversarial ones the contract exists for: a `min_exclusive`
    /// one byte below the offset (both when a boundary is reachable and when the
    /// only nearby one is excluded by it), whitespace only at `range.start`,
    /// whitespace only at `range.end - 1`, `range.end` ITSELF as the nearest
    /// boundary, a distance tie, a sub-range that is not the whole source, and a
    /// multibyte run where nothing in the window qualifies.
    #[test]
    fn snap_split_point_honors_its_contract() {
        // A space, 40 × `あ`, a space: 122 bytes. Byte 61 is 61 and 60 bytes from the spaces,
        // outside the ±32-byte window, so the split stays at the offset.
        let multibyte_run = format!(" {} ", "あ".repeat(MULTIBYTE_RUN_CHARS));
        let multibyte_len = multibyte_run.len();
        assert!(
            multibyte_run.is_char_boundary(61),
            "fixture sanity: byte 61 must be a char boundary in the multibyte run"
        );

        // `(case, source, range, offset, min_exclusive, expected)`; the numbers are byte offsets.
        let cases = [
            (
                "snaps to the word boundary at the offset itself",
                "ab cdefgh",
                0..9,
                3,
                2,
                3,
            ),
            (
                "min_exclusive excludes the only nearby boundary, so it falls back \
                 to the exact offset",
                "ab cdefghij",
                0..11,
                4,
                3,
                4,
            ),
            (
                "whitespace only at range.start",
                " abcdefghij",
                0..11,
                5,
                0,
                1,
            ),
            (
                "whitespace only at range.end - 1",
                "abcdefghij ",
                0..11,
                5,
                0,
                10,
            ),
            // Byte 11 == `range.end` is the nearest boundary but leaves an empty remainder, so it is
            // rejected; without the `min(.., range.end)` clamp this returns 11.
            (
                "range.end is never a candidate, even when it is the nearest boundary",
                "abcdefghijk lm",
                0..11,
                9,
                0,
                9,
            ),
            (
                "a distance tie picks the SMALLER offset",
                "ab cd ef",
                0..8,
                4,
                0,
                3,
            ),
            (
                "candidates are clamped to the given sub-range, not the whole source",
                "aaa bbb ccc ddd",
                4..11,
                6,
                4,
                7,
            ),
            (
                "no word boundary anywhere in the window of a multibyte run",
                multibyte_run.as_str(),
                0..multibyte_len,
                61,
                0,
                61,
            ),
        ];

        for (case, source, range, offset, min_exclusive, expected) in &cases {
            let result = snap_split_point(source, range, *offset, *min_exclusive);
            assert_eq!(
                result, *expected,
                "snap_split_point({source:?}, {range:?}, {offset}, {min_exclusive}) \
                 must be {expected} — {case}"
            );
            assert!(
                *min_exclusive < result,
                "the split point must stay STRICTLY above `min_exclusive` \
                 ({min_exclusive}), or two markers could share a point / the prefix \
                 could be empty; got {result} — {case}"
            );
            assert!(
                result < range.end,
                "the split point must stay STRICTLY below `range.end` ({}), or the \
                 remainder slice would be empty; got {result} — {case}",
                range.end
            );
            assert!(
                source.is_char_boundary(result),
                "the split point must be a char boundary, or slicing the `Text` \
                 payload would panic; got {result} in {source:?} — {case}"
            );
        }
    }

    /// G4: a split lands INSIDE an inline element. Both the module docs and
    /// `src/README.md` state that an interior anchor is placed within the rendered
    /// inline flow — including inside `<em>` / `<strong>` / `<a>` text — and
    /// nothing else in the suite pins it.
    ///
    /// Fixture: `*{240 chars}end*` — one emphasized run spanning both stride
    /// columns. The trailing `end` matters: a closing `*` preceded by whitespace is
    /// not right-flanking, so `*lorem ipsum *` would NOT produce emphasis at all.
    #[test]
    fn split_lands_inside_an_inline_element() {
        let inner = format!("{}end", "lorem ipsum ".repeat(20));
        let source = format!("*{inner}*");

        let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);
        let body = body_of(&html);

        let em_open = body
            .find("<em>")
            .unwrap_or_else(|| panic!("fixture sanity: the run must render an <em>: {body}"));
        let em_close = body
            .find("</em>")
            .unwrap_or_else(|| panic!("the <em> must be closed: {body}"));

        let mut previous = em_open;
        for stride in 1..=2u32 {
            let col = stride * (MARKER_COLUMN_STRIDE as u32);
            let id = format!(r#"id="wl-1-{col}""#);
            let idx = body
                .find(&id)
                .unwrap_or_else(|| panic!("missing the `{id}` anchor: {body}"));
            assert!(
                previous < idx && idx < em_close,
                "`{id}` (at {idx}) must sit INSIDE the <em> element (<em> at \
                 {em_open}, </em> at {em_close}), after the previous anchor at \
                 {previous} — an interior marker is placed in the rendered inline \
                 flow, which includes the interior of an inline element: {body}"
            );
            previous = idx;
        }

        // …and splitting the emphasis text must not disturb it.
        let em_inner = &body[em_open + "<em>".len()..em_close];
        assert_eq!(
            strip_marker_spans(em_inner),
            inner,
            "with its anchors stripped, the <em> element's text must be byte-identical \
             to the source's emphasized run"
        );
    }

    /// The maximum number of characters of an offending stylesheet line to quote
    /// back in a failure. The inlined `@font-face` rules are single lines of several
    /// hundred KiB of base64, so an untruncated excerpt would be unusable.
    const CSS_EXCERPT_CHARS: usize = 120;

    /// The first `<` followed by an ASCII letter or `/` in `css`, as a 1-based line
    /// number plus a truncated excerpt of that line. `None` when the stylesheet
    /// carries nothing that an HTML scanner could mistake for a tag.
    fn first_markup_like_sequence(css: &str) -> Option<(usize, String)> {
        for (idx, line) in css.lines().enumerate() {
            let bytes = line.as_bytes();
            for (i, b) in bytes.iter().enumerate() {
                if *b != b'<' {
                    continue;
                }
                if bytes
                    .get(i + 1)
                    .is_some_and(|next| next.is_ascii_alphabetic() || *next == b'/')
                {
                    return Some((idx + 1, line.chars().take(CSS_EXCERPT_CHARS).collect()));
                }
            }
        }
        None
    }

    /// The failure message for [`embedded_stylesheets_carry_no_markup_like_text`].
    /// Spelled out at length on purpose: the hazard is invisible at the edit site
    /// (it is "just a comment"), so the failure has to carry the whole argument.
    fn markup_in_stylesheet_failure(what: &str, line: usize, excerpt: &str) -> String {
        format!(
            "{what} line {line} contains a `<` followed by a letter or `/`:\n\n  \
             {excerpt}\n\n\
             This is NOT a style nit. Every one of these stylesheets is \
             `include_str!`'d VERBATIM into the `<style>` block of each rendered \
             document, so an element name in angle brackets — even inside a CSS \
             COMMENT — becomes literal text in the emitted HTML, ahead of the real \
             `<body>`. Three consumers scan that HTML for the FIRST match of a \
             fixed needle and are silently corrupted by it:\n\n\
             1. `PreviewScrollSeam.splitDocument` (`PreviewScrollSeam.swift`) does \
             `html.range(of: \"<body\")`. A `<body`-like string in the stylesheet \
             precedes the real body tag, so the head/body split moves. The update \
             strategy guards itself by comparing head LENGTHS across renders — but a \
             wrong split is CONSISTENTLY wrong, so the lengths still match and the \
             guard never trips. Instead of degrading to a safe full page load, the \
             preview swaps a wrong body into the live page on every keystroke.\n\n\
             2. `browser_preview::render_page` does \
             `html.replacen(\"</body>\", …, 1)` to inject the live-reload script. A \
             `</body>`-like string in the stylesheet puts that `<script>` INSIDE the \
             `<style>` block, silently killing browser-preview auto-reload.\n\n\
             3. The integration tests in `tests/markdown_render.rs` scan for `<p>`, \
             `<pre`, `<table`, `<ul>`, `<li>`, `<em>` … over the whole document; a \
             match inside the stylesheet makes them assert against CSS instead of \
             rendered content. A previous edit of this kind broke ten of them at \
             once.\n\n\
             Fix: write the element name as prose (`the code element`) or without \
             the angle brackets (`code`). Do not \"fix\" the scanners."
        )
    }

    /// The rendered document's `<style>` block contents. Deliberately does NOT dump
    /// the document on failure: it is around a megabyte of inlined base64 fonts.
    fn style_block_of(html: &str) -> &str {
        const OPEN: &str = "<style>";
        const CLOSE: &str = "</style>";
        let open = html
            .find(OPEN)
            .unwrap_or_else(|| panic!("the rendered document must embed a <style> block"));
        let start = open + OPEN.len();
        let end = html[start..]
            .find(CLOSE)
            .unwrap_or_else(|| panic!("the <style> block must be closed"));
        &html[start..start + end]
    }

    /// No stylesheet that reaches the rendered `<style>` block may contain anything
    /// an HTML scanner could mistake for a tag — see
    /// [`markup_in_stylesheet_failure`] for the full argument and the three
    /// consumers it protects.
    ///
    /// Scope: EVERY constant `render_html_with_highlights` concatenates into the
    /// `<style>` block, not just `base.css` — `MATH_CSS`, the theme CSS, both
    /// inlined `@font-face` rules and `CAPPED_WIDTH_CSS` all ship through the same
    /// path and would cause the same corruption. The named-constant pass names the
    /// exact file to fix; the rendered-document pass then re-scans the real emitted
    /// `<style>` block so anything ADDED to that concatenation later is covered
    /// without touching this test.
    #[test]
    fn embedded_stylesheets_carry_no_markup_like_text() {
        // Exhaustive by construction: adding a `PreviewTheme` variant breaks this
        // `match`, which is the reminder to add its CSS to `sheets` below.
        match PreviewTheme::Default {
            PreviewTheme::Default => {}
        }

        let sheets = [
            ("markdown/base.css (BASE_CSS)", BASE_CSS),
            ("markdown/math.css (MATH_CSS)", MATH_CSS),
            (
                "markdown/themes/default.css (theme_css(PreviewTheme::Default))",
                theme_css(PreviewTheme::Default),
            ),
            ("the INTER_FONT_FACE rule", INTER_FONT_FACE),
            ("the MATH_FONT_FACE rule", MATH_FONT_FACE),
            ("CAPPED_WIDTH_CSS", CAPPED_WIDTH_CSS),
        ];

        for (name, css) in &sheets {
            if let Some((line, excerpt)) = first_markup_like_sequence(css) {
                panic!("{}", markup_in_stylesheet_failure(name, line, &excerpt));
            }
        }

        // The same scan over the REAL emitted `<style>` block, for both width modes
        // (only `WidthMode::Capped` appends `CAPPED_WIDTH_CSS`).
        for width_mode in [WidthMode::Full, WidthMode::Capped] {
            let html = render_html("# H\n\ntext\n", PreviewTheme::Default, width_mode);
            if let Some((line, excerpt)) = first_markup_like_sequence(style_block_of(&html)) {
                let what = format!("the rendered <style> block ({width_mode:?} width)");
                panic!("{}", markup_in_stylesheet_failure(&what, line, &excerpt));
            }
        }
    }
}
