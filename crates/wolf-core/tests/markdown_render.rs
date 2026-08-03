//! Integration test for `wolf-core`'s PUBLIC Markdown-preview render surface
//! (plan §"Core: Markdown→HTML rendering module" + the "Pure Rust unit" test
//! bullet). It exercises ONLY the re-exported public API — `wolf_core::render_html`,
//! `wolf_core::PreviewTheme`, and `wolf_core::theme_css` — NOT the module's internals
//! (`desired_markers`, `MarkerInjector`, …), which remain private.
//!
//! Note that `MARKER_COLUMN_STRIDE` IS public (re-exported from `lib.rs`), but this
//! file deliberately does not import it: the stride is re-declared locally as a
//! pinned literal (see [`COLUMN_STRIDE`] below) so the assertions check the rendered
//! behavior against an independently stated expectation rather than tautologically
//! comparing the implementation to itself. Changing the source const must therefore
//! FAIL this test, not silently move the goalposts with it. `const BASELINE: i32 = 24`
//! further down follows the same principle.
//!
//! Behavior, not implementation, is asserted: a `>= 100`-character line yields a
//! per-100-column marker `id="wl-{L}-100"`, a shorter line does not, every hard
//! line yields a `wl-{L}-0` anchor, and none of these markers land inside a fenced
//! code block.
//!
//! The two anchor tiers are both pinned here. A `wl-{L}-0` anchor is flushed
//! immediately BEFORE its block's opening tag (so a multi-line paragraph collapses
//! every one of its column-zero anchors to the block top, and their `data-line`
//! values stay in ascending document order — the front-end's fallback sweep depends
//! on that). A per-100-column anchor is placed INSIDE the block's rendered inline
//! content, at its real position, EXCEPT inside a code block, an HTML block, a
//! table or an image's alt text, where it keeps the block-top placement. See the
//! section at the end of this file.
//!
//! ## Why a separate integration test file (not `markdown.rs`'s own `#[cfg(test)]`)
//!
//! `crates/wolf-core/src/markdown.rs` carries its own in-module
//! `#[cfg(test)] mod tests` for the renderer's internals. Testing through the
//! PUBLIC surface here keeps the two layers distinct and pins the re-exported
//! contract from `lib.rs`: an internal refactor may move the in-module tests
//! freely, but anything asserted here is a promise to callers outside the crate.
//!
//! ## Build wiring
//!
//! Bazel does not auto-discover test files, so this one runs only because
//! `crates/wolf-core/BUILD.bazel` declares the `//crates/wolf-core:markdown_render`
//! `rust_test` target (`srcs = ["tests/markdown_render.rs"]`,
//! `crate_root = "tests/markdown_render.rs"` — a pure, nvim-free unit test modeled
//! on the `pane_status_unit` stanza). The in-module tests run under
//! `//crates/wolf-core:wolf_core_lib_test`. A new `tests/*.rs` file added here
//! without its own `rust_test` target is silently skipped on macOS even though
//! `cargo test` would pick it up.
//!
//! This is a PURE render test: no Neovim, no filesystem, no wake — it needs no
//! `WOLF_NVIM_BIN` and never skips.

use std::collections::{HashMap, HashSet};

use wolf_core::{
    code_blocks, render_html, render_html_with_highlights, theme_css, BlockHighlight,
    HighlightSpan, HighlightStyle, PreviewTheme, WidthMode,
};

/// The per-line/per-100-column marker id substring for hard line `line` (1-based)
/// and 0-based character column `col`, per the plan's `id="wl-{L}-{C}"` anchor
/// scheme. Behavior is asserted via this id substring so the test never depends on
/// the source `MARKER_COLUMN_STRIDE`, the private marker-injection internals, or
/// attribute order.
// per plan: the anchor id format is `wl-{L}-{C}` with double-quoted `id="…"`.
fn marker_id(line: u32, col: u32) -> String {
    format!("id=\"wl-{line}-{col}\"")
}

/// The per-100-column stride the plan mandates (`MARKER_COLUMN_STRIDE = 100`),
/// re-declared LOCALLY as a behavioral constant. The source const is public and
/// could be imported, but pinning the literal here keeps the assertion independent
/// of the implementation: if the source stride changes, this test must fail rather
/// than quietly track it. A line of exactly this many characters is the first that
/// earns a `-100` marker.
const COLUMN_STRIDE: u32 = 100;

/// A paragraph line long enough (>= [`COLUMN_STRIDE`], < `2 * COLUMN_STRIDE`) to
/// earn exactly one per-100-column marker (`-100`) and no `-200`.
const LONG_LINE_LEN: usize = 150;

/// Number of 2-byte multibyte characters ("é") on the OD4 line: between one and two
/// strides of CHARACTERS (so `-100` fires, `-200` does not), while the BYTE length
/// (240) exceeds two strides — proving columns count characters, not bytes.
const MULTIBYTE_CHAR_COUNT: usize = 120;

#[test]
fn render_html_wraps_body_in_a_themed_document() {
    let html = render_html("# Title\n\nHello", PreviewTheme::Default, WidthMode::Full);

    // The doctype + <style> + body-class document wrapper.
    assert!(
        html.to_ascii_lowercase().contains("<!doctype html"),
        "rendered document must open with a doctype: {html}"
    );
    assert!(
        html.contains("<style"),
        "rendered document must embed a <style> block: {html}"
    );
    assert!(
        html.contains(r#"class="wolf-markdown""#),
        "the <body> must carry class=\"wolf-markdown\": {html}"
    );

    // The rendered Markdown body itself.
    assert!(
        html.contains("<h1"),
        "`# Title` must render an <h1>: {html}"
    );
    assert!(html.contains("<p>"), "`Hello` must render a <p>: {html}");
    assert!(
        html.contains("Title") && html.contains("Hello"),
        "the source text must survive into the body: {html}"
    );
}

#[test]
fn code_blocks_enumerates_a_fenced_go_block_via_public_api() {
    // The re-exported `code_blocks` enumerates one `CodeBlockInfo` for a fenced go
    // block: lowercased lang + the body text (incl. the trailing newline). Mirrors
    // the in-module unit test at the PUBLIC-API level (crate-root re-exports).
    let blocks = code_blocks("# H\n\n```go\npackage main\n```\n");
    assert_eq!(blocks.len(), 1, "one fenced code block: {blocks:?}");
    assert_eq!(
        blocks[0].lang.as_deref(),
        Some("go"),
        "the fenced info-string language, lowercased: {blocks:?}"
    );
    assert_eq!(
        blocks[0].text, "package main\n",
        "the block body incl. the trailing newline: {blocks:?}"
    );
}

#[test]
fn render_html_with_highlights_emits_inline_styled_spans_via_public_api() {
    // The re-exported `render_html_with_highlights` + `BlockHighlight` /
    // `HighlightSpan` / `HighlightStyle` types turn one `[0, 7)` byte-column span
    // into a color+bold inline `<span>`, still inside `<pre><code
    // class="language-go">`.
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
    let html =
        render_html_with_highlights(src, PreviewTheme::Default, WidthMode::Full, &[Some(block)]);
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

#[test]
fn render_html_empty_highlights_matches_render_html_via_public_api() {
    // The zero-churn delegation contract at the public-API level: an empty highlight
    // slice reproduces the plain `render_html` byte-for-byte.
    let src = "# H\n\n```go\npackage main\nfunc f() {}\n```\n";
    let empty = render_html_with_highlights(src, PreviewTheme::Default, WidthMode::Full, &[]);
    let plain = render_html(src, PreviewTheme::Default, WidthMode::Full);
    assert_eq!(
        empty, plain,
        "render_html_with_highlights(src, .., &[]) must byte-equal render_html(src, ..)"
    );
}

#[test]
fn every_hard_line_yields_a_column_zero_marker() {
    // Three hard lines: a heading, a BLANK line, and a paragraph. Even the blank
    // line 2 must carry its own `wl-2-0` anchor (a marker per hard line, always).
    let source = "# Title\n\nHello";
    let line_count = source.split('\n').count() as u32;
    assert_eq!(line_count, 3, "sanity: this fixture has three hard lines");

    let html = render_html(source, PreviewTheme::Default, WidthMode::Full);
    for line in 1..=line_count {
        assert!(
            html.contains(&marker_id(line, 0)),
            "every hard line must yield a `wl-{line}-0` anchor: {html}"
        );
    }
}

#[test]
fn markers_appear_in_source_order() {
    // Distinct blocks over six hard lines; every `wl-{L}-0` anchor must appear in
    // ascending source order in the emitted HTML.
    let source = "# A\n\n## B\n\nC\n";
    let line_count = source.split('\n').count() as u32;

    let html = render_html(source, PreviewTheme::Default, WidthMode::Full);

    let mut last = None;
    for line in 1..=line_count {
        let idx = html
            .find(&marker_id(line, 0))
            .unwrap_or_else(|| panic!("missing `wl-{line}-0` anchor in: {html}"));
        if let Some(prev) = last {
            assert!(
                prev < idx,
                "anchors must appear in source order: `wl-{line}-0` at {idx} \
                 should follow the previous anchor at {prev}: {html}"
            );
        }
        last = Some(idx);
    }
}

#[test]
fn heading_marker_sits_immediately_before_its_tag() {
    // A paragraph, a blank line, then a heading on hard line 3. Its `wl-3-0` anchor
    // must be emitted IMMEDIATELY BEFORE the `<h1>` (substring order), so a later
    // scroll-sync can map line 3 to the heading's top.
    let source = "para\n\n# Heading\n";
    let html = render_html(source, PreviewTheme::Default, WidthMode::Full);

    let marker = marker_id(3, 0);
    let marker_idx = html
        .find(&marker)
        .unwrap_or_else(|| panic!("missing `wl-3-0` anchor in: {html}"));
    let heading_idx = html
        .find("<h1")
        .unwrap_or_else(|| panic!("missing <h1> in: {html}"));
    assert!(
        marker_idx < heading_idx,
        "`wl-3-0` (at {marker_idx}) must precede its <h1> (at {heading_idx}): {html}"
    );

    // "Immediately before": nothing but whitespace between the anchor span's close
    // and the opening `<h1`.
    let after_marker = &html[marker_idx..];
    let close = after_marker
        .find("</span>")
        .expect("the anchor must be a self-closed <span…></span>");
    let tail = after_marker[close + "</span>".len()..].trim_start();
    assert!(
        tail.starts_with("<h1"),
        "the `wl-3-0` anchor must sit immediately before its <h1> (found: {:.40}): {html}",
        tail
    );
}

#[test]
fn long_line_earns_a_per_hundred_column_marker() {
    // Hard line 1 is short (< stride, no `-100` anchor); hard line 3 is a paragraph
    // of LONG_LINE_LEN characters (>= stride, so it earns a `wl-3-100` anchor but
    // NOT a `wl-3-200` one, since 200 is not strictly < LONG_LINE_LEN).
    assert!(
        LONG_LINE_LEN >= COLUMN_STRIDE as usize,
        "sanity: the long line clears the stride"
    );
    let long_line = "a".repeat(LONG_LINE_LEN);
    let source = format!("short\n\n{long_line}");

    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    assert!(
        html.contains(&marker_id(3, COLUMN_STRIDE)),
        "a {LONG_LINE_LEN}-char line must earn a `wl-3-100` per-100-column anchor: {html}"
    );
    assert!(
        !html.contains(&marker_id(3, 2 * COLUMN_STRIDE)),
        "a {LONG_LINE_LEN}-char line must NOT earn a `wl-3-200` anchor \
         (200 !< {LONG_LINE_LEN}): {html}"
    );
    assert!(
        !html.contains(&marker_id(1, COLUMN_STRIDE)),
        "the short hard line 1 must NOT earn a `wl-1-100` anchor: {html}"
    );
}

#[test]
fn no_marker_span_inside_a_fenced_code_block() {
    // Hard lines 3..=6 are a fenced code block. Their per-line anchors collapse to
    // BEFORE the `<pre>` (the block start); none may land inside `<pre><code>…`,
    // which would corrupt the rendered code.
    let source = "# Heading\n\n```\nfn main() {}\ncode two\n```\n";
    let html = render_html(source, PreviewTheme::Default, WidthMode::Full);

    let pre_start = html
        .find("<pre")
        .unwrap_or_else(|| panic!("missing <pre> for the fenced code block in: {html}"));
    let pre_end = html
        .find("</pre>")
        .unwrap_or_else(|| panic!("missing </pre> in: {html}"));
    let code_block = &html[pre_start..pre_end];
    assert!(
        !code_block.contains("wolf-src"),
        "no `wolf-src` marker span may appear inside <pre><code>…: {code_block}"
    );
    assert!(
        !code_block.contains("wl-"),
        "no `wl-` anchor id may appear inside <pre><code>…: {code_block}"
    );

    // The code lines' anchors still EXIST (collapsed before the block).
    assert!(
        html.contains(&marker_id(4, 0)),
        "the code line's `wl-4-0` anchor must still exist (collapsed before <pre>): {html}"
    );
}

#[test]
fn multi_line_paragraph_collapses_its_markers_before_the_single_p() {
    // Three consecutive non-blank lines form ONE paragraph. Each line still gets a
    // unique `wl-{L}-0` anchor, and all of them collapse to BEFORE the single <p>.
    let source = "foo\nbar\nbaz";
    let html = render_html(source, PreviewTheme::Default, WidthMode::Full);

    assert_eq!(
        html.matches("<p>").count(),
        1,
        "the three lines must render as exactly one <p>: {html}"
    );
    let p_idx = html.find("<p>").expect("the paragraph must render a <p>");

    for line in 1..=3u32 {
        let marker = marker_id(line, 0);
        let idx = html
            .find(&marker)
            .unwrap_or_else(|| panic!("missing `wl-{line}-0` anchor in: {html}"));
        assert!(
            idx < p_idx,
            "`wl-{line}-0` (at {idx}) must collapse to before the single <p> (at {p_idx}): {html}"
        );
    }
}

#[test]
fn per_hundred_column_marker_counts_characters_not_bytes() {
    // OD4: columns are 0-based CHARACTER offsets, not bytes. Hard line 3 is a
    // paragraph of MULTIBYTE_CHAR_COUNT × "é" (each 2 bytes in UTF-8): > 100
    // CHARACTERS but > 200 BYTES. The per-100-column marker fires on the CHARACTER
    // count, so `wl-3-100` must appear (char length >= 100) while `wl-3-200` must
    // NOT (only ~120 chars) — a byte-based scheme would wrongly emit `wl-3-200`
    // (since the byte length exceeds 200).
    let multibyte_line = "é".repeat(MULTIBYTE_CHAR_COUNT);
    assert!(
        multibyte_line.chars().count() >= COLUMN_STRIDE as usize
            && multibyte_line.chars().count() < 2 * COLUMN_STRIDE as usize,
        "sanity: the line has between one and two full strides of CHARACTERS"
    );
    assert!(
        multibyte_line.len() > 2 * COLUMN_STRIDE as usize,
        "sanity: the line's BYTE length exceeds two strides (so byte/char differ)"
    );
    // Two leading lines put the long multibyte line on hard line 3, rendered as its
    // own single-line paragraph — exactly the block shape
    // `long_line_earns_a_per_hundred_column_marker` already relies on.
    let source = format!("# Title\n\n{multibyte_line}");

    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    assert!(
        html.contains(&marker_id(3, COLUMN_STRIDE)),
        "a >=100-CHARACTER line must earn a `wl-3-100` anchor even when its byte \
         length far exceeds 100: {html}"
    );
    assert!(
        !html.contains(&marker_id(3, 2 * COLUMN_STRIDE)),
        "a ~{MULTIBYTE_CHAR_COUNT}-CHARACTER line must NOT earn a `wl-3-200` anchor \
         (columns count characters, not bytes): {html}"
    );
}

#[test]
fn container_collapses_interior_line_markers_before_its_opening_tag() {
    // OD8 / the ratified `offset < range.end` rule: a container block (here a
    // bulleted list) collapses EVERY interior line's `wl-{L}-0` anchor to the TOP of
    // the container — before the opening `<ul>` — rather than per-item. This is the
    // accepted v1 scroll granularity: all interior anchors still EXIST (unique ids)
    // but resolve to the container top.
    //
    // Two leading lines put the list's three items on hard lines 3, 4, and 5.
    let source = "intro\n\n- item one\n- item two\n- item three\n";
    let html = render_html(source, PreviewTheme::Default, WidthMode::Full);

    let list_idx = html
        .find("<ul>")
        .unwrap_or_else(|| panic!("missing the container's opening <ul> in: {html}"));

    // Each interior line (3, 4, 5) still has a unique anchor, and each collapses to
    // BEFORE the container's opening tag.
    for line in 3..=5u32 {
        let marker = marker_id(line, 0);
        let idx = html
            .find(&marker)
            .unwrap_or_else(|| panic!("missing interior `wl-{line}-0` anchor in: {html}"));
        assert!(
            idx < list_idx,
            "the interior `wl-{line}-0` anchor (at {idx}) must collapse to before the \
             container's <ul> (at {list_idx}): {html}"
        );
    }
}

#[test]
fn latex_math_renders_mathml_with_inlined_fonts() {
    // The no-Neovim primary automated proof for tasks 1 & 2: a doc with BOTH inline
    // `$...$` and display `$$...$$` LaTeX math must render inline MathML (block mode
    // for the display math), and inline BOTH the Inter and the Latin Modern Math
    // fonts as self-contained base64 data URIs. Assertions key off the STABLE
    // contracts (`<math`, `display="block"`, the family names, `data:font/woff2`),
    // NOT exact MathML byte sequences.
    let source = "Inline $e=mc^2$ here.\n\n$$\\int_0^1 x\\,dx$$\n";
    let html = render_html(source, PreviewTheme::Default, WidthMode::Full);

    assert!(
        html.contains("<math"),
        "the LaTeX math must render a <math> element: {html}"
    );
    assert!(
        html.contains("</math>"),
        "the LaTeX math must close its <math> element: {html}"
    );
    assert!(
        html.contains("display=\"block\""),
        "the display `$$...$$` math must render `display=\"block\"`: {html}"
    );
    assert!(
        html.contains("@font-face"),
        "the document must inline an @font-face rule: {html}"
    );
    assert!(
        html.contains("Inter"),
        "the document must inline the `Inter` body font: {html}"
    );
    assert!(
        html.contains("data:font/woff2"),
        "fonts must be inlined as base64 data:font/woff2 URIs: {html}"
    );
    assert!(
        html.contains("Latin Modern Math"),
        "the document must bundle + pin the `Latin Modern Math` font: {html}"
    );
}

#[test]
fn theme_css_default_is_non_empty() {
    assert!(
        !theme_css(PreviewTheme::Default).is_empty(),
        "the shipped Default theme must return non-empty CSS"
    );
}

#[test]
fn default_theme_carries_dark_and_light_palettes() {
    // The Default theme keeps its DARK values as the unconditional default and adds
    // a `@media (prefers-color-scheme: light)` block so the preview follows the app's
    // resolved appearance. The rendered document must embed BOTH palettes plus the
    // `color-scheme: light dark` declaration that makes the web view honor them.
    let html = render_html("# Title\n\ntext", PreviewTheme::Default, WidthMode::Full);

    assert!(
        html.contains("#1b1b1b"),
        "the dark background `#1b1b1b` must remain the unconditional default: {html}"
    );
    assert!(
        html.contains("prefers-color-scheme: light"),
        "the theme must include a `prefers-color-scheme: light` media query: {html}"
    );
    assert!(
        html.contains("#f4f4f4"),
        "the light media query must set the light-mode background `#f4f4f4`: {html}"
    );
    assert!(
        html.contains("color-scheme: light dark"),
        "the theme must declare `color-scheme: light dark` so both schemes are honored: {html}"
    );
    assert!(
        html.contains("#0969da"),
        "the light media query must mirror the light-mode link color `#0969da`: {html}"
    );
    assert!(
        html.contains("#111111"),
        "the light media query must mirror the light-mode heading color `#111111`: {html}"
    );
}

// ---------------------------------------------------------------------------
// Baseline-grid assertions (plan §6). `base.css` is `include_str!`'d verbatim
// into the rendered document, so these tests audit the SHIPPED stylesheet
// through the public `render_html` surface — no private const, no new file.
// ---------------------------------------------------------------------------

/// The baseline unit, re-declared LOCALLY as a behavioral constant: every
/// vertical dimension in `base.css` must be `0` or a whole multiple of this.
const BASELINE: i32 = 24;

/// The width of every border that has to be compensated out of an adjacent
/// padding or absorbed into an explicit height (`th`/`td` `border`, `hr`'s
/// `border-top`). Named so the compensation arithmetic in the failure messages
/// reads as arithmetic.
const BORDER: i32 = 1;

/// `(selector, font-size px, line-height px)` for the six heading levels
/// (plan §2). h3–h6 are the levels that today fall through to UA defaults; the
/// whole point of pinning them here is that the fix cannot silently regress.
const HEADING_SCALE: &[(&str, i32, i32)] = &[
    ("h1", 32, 48),
    ("h2", 24, 48),
    ("h3", 20, 24),
    ("h4", 18, 24),
    ("h5", 16, 24),
    ("h6", 14, 24),
];

/// Every selector in `base.css` that carries a vertical dimension, swept by
/// [`every_vertical_dimension_is_on_the_baseline_grid`].
///
/// # What is deliberately NOT swept, and why
///
/// The omissions below are as much a part of the audit as the inclusions. None
/// of them belongs on this list, and — importantly — none belongs in
/// [`COMPENSATION_ALLOWLIST`] either: allowlisting `code`'s `2px`, for example,
/// would make the sweep report success no matter what later happened to that
/// rule, converting a real audit into a rubber stamp. A comment is the honest
/// record.
///
/// - **`code` and `pre code`** — `code` sets `padding: 2px 4px`, which is
///   off-grid and CORRECT. Per plan §3.4 an inline box's vertical padding
///   paints its background but does NOT grow the line box, so those 2px are
///   layout-neutral: they cannot move a baseline. Sweeping `code` would flag a
///   value the grid does not care about; allowlisting it would then stop the
///   sweep from ever noticing if `code` later grew a genuinely vertical
///   dimension. `pre code` (`padding: 0`) is asserted directly by
///   [`pre_sits_on_the_baseline_grid`] for its `font-size: inherit` instead.
/// - **`img`** — `height: auto` is the accepted intrinsic-height drift (plan
///   §3.4 / the header's DOCUMENTED RESIDUALS). It is unparseable as px by
///   construction, and there is no CSS-only way to snap it. Sweeping it would
///   fail permanently for a drift the design accepts.
/// - **`a` / `a:hover`** — `text-decoration` only. No vertical dimension.
/// - **`.wolf-src`** — deliberately ZERO height. Its `line-height: 0` and
///   `font-size: 0` are load-bearing (the body's LENGTH line-height would
///   otherwise inherit a hard 24px into every marker and inject visible blank
///   lines), so 0 here is a correct value the sweep would trivially pass. The
///   rule is explicitly out of scope for this change; do not touch it.
/// - **`html` and `*` / `*::before` / `*::after`** — `box-sizing` only. No
///   vertical dimensions at all, though the inherited `border-box` is what lets
///   `hr` absorb its rule into an explicit height.
const GRID_SELECTORS: &[&str] = &[
    "body.wolf-markdown",
    "body.wolf-markdown > *:not(.wolf-src):not(*:not(.wolf-src) ~ *)",
    "body.wolf-markdown > *:last-child",
    // The GENERAL last-block rule, the symmetric twin of the general
    // first-block entry above. Trivially on-grid today (`margin-bottom: 0`), so
    // it catches no arithmetic — it is listed because both general edge rules
    // are new, are spelled in a way that invites "simplification" back to
    // `:first-child`/`:last-child`, and because the sweep's non-empty assertion
    // makes SILENT DELETION of the rule fail loudly here.
    "body.wolf-markdown > *:not(.wolf-src):not(:has(~ *:not(.wolf-src)))",
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "pre",
    "blockquote",
    "table",
    "th, td",
    "hr",
    "ul, ol",
    "li > ul, li > ol",
];

/// The vertical longhand properties the sweep checks directly.
const VERTICAL_LONGHANDS: &[&str] = &[
    "margin-top",
    "margin-bottom",
    "padding-top",
    "padding-bottom",
    "line-height",
    "height",
    "scroll-margin-top",
];

/// The ONLY vertical values in `base.css` allowed to be off the 24px grid:
/// `(selector, property, value)`. Each entry names the border it compensates.
/// Adding a value here is the documented way to introduce a compensated
/// dimension — and it forces the author to say which border it pays for.
const COMPENSATION_ALLOWLIST: &[(&str, &str, i32)] = &[
    // Compensate the `th, td { border: 1px }` collapsed row border:
    // 12 + 24 (line-height) + 11 + 1 = 48 = 2 baselines of row pitch. The 12/11
    // asymmetry exists because 47 is odd; it is imperceptible.
    ("th, td", "padding-top", 12),
    ("th, td", "padding-bottom", 11),
    // Compensates the table's OWN bottom outer border, which has no next row to
    // share with: a collapsed-border table is 48N + 1 tall, so 23 + 1 = 24
    // brings the whole table to 48N + 24 = exactly 2N+1 baselines.
    ("table", "margin-bottom", 23),
];

/// Slices the document's embedded stylesheet out of the `<style>` block.
///
/// # Why a crude parser is legitimate here
///
/// [`style_block`], [`rule_decls`] and [`px`] are NOT a CSS parser. They are
/// valid only because `base.css` is hand-written, flat, and non-nested: every
/// rule is `selector-list { prop: value; … }` at the top level, and it is
/// `include_str!`'d into the document byte-for-byte, so what these helpers read
/// is exactly what ships.
///
/// The one construct that defeats them is `@media`, which exists ONLY in
/// `themes/default.css` (the `prefers-color-scheme: light` block, emitted
/// AFTER `base.css`). Inside it the brace scan desynchronizes, so:
///
/// - every `base.css` rule is still read correctly (it all precedes the
///   `@media`), and
/// - the `hr` theme assertions in
///   [`hr_occupies_whole_baselines_and_is_painted_by_its_border`] must use
///   plain substring checks against the whole document, NOT [`rule_decls`].
///
/// Do not grow these helpers into a general CSS parser; if a future stylesheet
/// needs nesting, the tests should assert on something else instead.
fn style_block(html: &str) -> &str {
    let open = html
        .find("<style>")
        .unwrap_or_else(|| panic!("the rendered document must embed a <style> block: {html}"));
    let start = open + "<style>".len();
    let end = html[start..]
        .find("</style>")
        .unwrap_or_else(|| panic!("the <style> block must be closed: {html}"));
    &html[start..start + end]
}

/// Removes `/* … */` comments. `base.css` carries a long header comment that
/// tabulates the scale and the compensation arithmetic — it contains braces,
/// colons and semicolons, so it MUST be stripped before any brace scanning.
fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Collapses all runs of whitespace to a single space and trims.
fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// True when a rule's comma-separated selector `entry` addresses the element
/// named by the query `part`.
///
/// `base.css` writes its rules descendant-scoped (`body.wolf-markdown h1`),
/// while the plan's tables name bare elements (`h1`), so an entry matches when
/// it equals the query OR ends with ` {query}` after a purely-descendant
/// ancestor prefix. The "purely descendant" guard is what keeps the query `ul`
/// from wrongly matching `body.wolf-markdown li > ul` — the discarded prefix
/// would contain a `>` combinator.
fn selector_entry_matches(entry: &str, part: &str) -> bool {
    if entry == part {
        return true;
    }
    let Some(prefix) = entry.strip_suffix(part) else {
        return false;
    };
    prefix.ends_with(' ') && !prefix.contains(['>', '+', '~'])
}

/// The declarations that apply to `selector`, merged across every matching rule
/// in document order (later declarations win, as the cascade resolves them at
/// equal specificity). Merging matters: `h1`'s `scroll-margin-top` lives on the
/// shared `h1, h2, … h6` rule while its `font-size` lives on the per-level rule.
///
/// `selector` may itself be a comma-separated list (`"th, td"`, `"ul, ol"`); a
/// rule matches when ANY of its entries matches ANY query part.
///
/// See [`style_block`] for why this crude scan is valid and where it is not.
fn rule_decls(css: &str, selector: &str) -> HashMap<String, String> {
    let css = strip_comments(css);
    let queries: Vec<String> = selector
        .split(',')
        .map(normalize_ws)
        .filter(|q| !q.is_empty())
        .collect();

    let mut decls = HashMap::new();
    let mut rest = css.as_str();
    while let Some(open) = rest.find('{') {
        let selectors = &rest[..open];
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let body = &after[..close];
        rest = &after[close + 1..];

        let matched = selectors.split(',').map(normalize_ws).any(|entry| {
            queries
                .iter()
                .any(|part| selector_entry_matches(&entry, part))
        });
        if !matched {
            continue;
        }
        for decl in body.split(';') {
            let Some((prop, value)) = decl.split_once(':') else {
                continue;
            };
            let prop = normalize_ws(prop).to_ascii_lowercase();
            let value = normalize_ws(value);
            if !prop.is_empty() && !value.is_empty() {
                decls.insert(prop, value);
            }
        }
    }
    decls
}

/// Parses `"Npx"` (or the unitless `"0"`) into whole pixels.
///
/// Returns `i32`, deliberately NOT `f64`: a FRACTIONAL pixel anywhere in this
/// stylesheet is itself a grid violation (it cannot be a whole multiple of 24),
/// so `"9.6px"` must return `None` and fail loudly rather than round silently.
fn px(value: &str) -> Option<i32> {
    let value = value.trim();
    if value == "0" {
        return Some(0);
    }
    value.strip_suffix("px")?.trim().parse::<i32>().ok()
}

/// The top and bottom components of a `margin`/`padding` shorthand, per the CSS
/// 1/2/3/4-value expansion rules.
fn shorthand_block_edges(value: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    match parts.len() {
        1 => Some((parts[0].to_string(), parts[0].to_string())),
        2 | 3 | 4 => Some((
            parts[0].to_string(),
            if parts.len() == 2 {
                parts[0].to_string()
            } else {
                parts[2].to_string()
            },
        )),
        _ => None,
    }
}

/// Renders a document whose body exercises every styled element, and returns it
/// together with its `<style>` block. The Markdown source is irrelevant to the
/// CSS assertions (the stylesheet is embedded unconditionally) but keeps these
/// tests reading as end-to-end render tests rather than file reads.
fn rendered_stylesheet() -> String {
    let source = "# H1\n\ntext\n\n## H2\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n---\n";
    let html = render_html(source, PreviewTheme::Default, WidthMode::Full);
    style_block(&html).to_string()
}

#[test]
fn baseline_unit_is_declared() {
    // The grid's single documented token. Literal px are used everywhere else
    // (so the sweep test can actually check the numbers), but the unit itself
    // must be declared and discoverable.
    let html = render_html("# Title\n\ntext", PreviewTheme::Default, WidthMode::Full);

    assert!(
        html.contains("--wolf-baseline: 24px"),
        "the stylesheet must declare the grid unit `--wolf-baseline: 24px` on \
         body.wolf-markdown: {html}"
    );
}

#[test]
fn body_line_height_is_a_length_not_a_ratio() {
    let css = rendered_stylesheet();
    let decls = rule_decls(&css, "body.wolf-markdown");

    let line_height = decls
        .get("line-height")
        .unwrap_or_else(|| panic!("body.wolf-markdown must declare line-height: {decls:?}"));

    // THE most load-bearing declaration in the grid, and the one most likely to
    // be "cleaned up" back into a ratio by a future editor.
    assert!(
        line_height.ends_with("px"),
        "body.wolf-markdown must set `line-height` as a LENGTH (`24px`), not a \
         unitless ratio — found `{line_height}`. A unitless ratio re-multiplies \
         against each DESCENDANT's own font-size, so inline <code> at 0.875em, \
         <small>, and superscripts would each produce a different line box and \
         fall off the {BASELINE}px grid. A length inherits as the COMPUTED \
         {BASELINE}px regardless of descendant font-size: {decls:?}"
    );
    assert_eq!(
        line_height, "24px",
        "body.wolf-markdown line-height must be exactly one baseline \
         ({BASELINE}px): {decls:?}"
    );
    assert_eq!(
        decls.get("font-size").map(String::as_str),
        Some("16px"),
        "body.wolf-markdown font-size must be 16px (16 x 1.5 = {BASELINE}px \
         leading): {decls:?}"
    );
    assert_eq!(
        decls.get("padding").map(String::as_str),
        Some("0"),
        "body padding must be exactly `0`: the preview body is deliberately \
         FLUSH to the pane's edges, and any inset belongs to the preview pane \
         chrome rather than to the document. `0` is trivially on-grid, so the \
         sweep can never object to it — which is precisely why this assertion \
         exists: it pins the explicit zero so a \"comfortable\" padding cannot \
         be silently reintroduced. Note that removing the declaration entirely \
         would also read as `0` in the browser (the UA default) but would lose \
         the record that flush edges are a decision, not an omission: {decls:?}"
    );
}

#[test]
fn headings_declare_explicit_font_size_and_line_height() {
    let css = rendered_stylesheet();

    for &(selector, font_size, line_height) in HEADING_SCALE {
        let decls = rule_decls(&css, selector);

        let declared_size = decls.get("font-size").unwrap_or_else(|| {
            panic!(
                "`{selector}` must declare an explicit font-size — h3..h6 must \
                 NOT fall through to UA defaults (18.7/16/13.3/10.7px, where h5 \
                 equals body text and h6 is illegibly smaller): {decls:?}"
            )
        });
        assert_eq!(
            px(declared_size),
            Some(font_size),
            "`{selector}` font-size must be {font_size}px per the type scale, \
             found `{declared_size}`: {decls:?}"
        );

        let declared_leading = decls.get("line-height").unwrap_or_else(|| {
            panic!(
                "`{selector}` must declare an explicit line-height on the grid — \
                 inheriting the body's {BASELINE}px is only correct for the \
                 levels whose scale says {BASELINE}px, and must be stated, not \
                 assumed: {decls:?}"
            )
        });
        let parsed = px(declared_leading).unwrap_or_else(|| {
            panic!(
                "`{selector}` line-height must be whole pixels, found \
                 `{declared_leading}` (a fractional px cannot sit on the grid): \
                 {decls:?}"
            )
        });
        assert_eq!(
            parsed, line_height,
            "`{selector}` line-height must be {line_height}px per the type \
             scale: {decls:?}"
        );
        assert_eq!(
            parsed % BASELINE,
            0,
            "`{selector}` line-height {parsed}px is not a whole multiple of the \
             {BASELINE}px baseline: {decls:?}"
        );
    }
}

#[test]
fn headings_reserve_anchor_scroll_clearance() {
    // The shared `h1, h2, … h6` rule's `scroll-margin-top`. The grid sweep sees
    // this property (it is in VERTICAL_LONGHANDS) and so would catch an off-grid
    // value — but it is blind to the declaration being DELETED outright, which
    // is the regression this test exists to make impossible.
    const ANCHOR_CLEARANCE: i32 = 2 * BASELINE;

    let css = rendered_stylesheet();

    for &(selector, _, _) in HEADING_SCALE {
        // `rule_decls` merges across matching rules, so the shared rule's
        // declaration is visible from every per-level query.
        let decls = rule_decls(&css, selector);

        let declared = decls.get("scroll-margin-top").unwrap_or_else(|| {
            panic!(
                "`{selector}` must declare `scroll-margin-top` (it lives on the \
                 shared `h1, h2, … h6` rule, alongside `font-weight`). It \
                 reserves {ANCHOR_CLEARANCE}px = 2 baselines of clearance above \
                 a heading scrolled into view by an in-document anchor link. The \
                 body has NO padding — it is deliberately flush to the pane \
                 edges — so this is the ONLY thing keeping a jumped-to heading \
                 off the pane's top edge; without it, clicking an anchor jams \
                 the target heading's cap-height flat against the top pixel of \
                 the preview. Deleting the declaration is invisible to every \
                 other assertion here, including the grid sweep, which can only \
                 check a value that exists: {decls:?}"
            )
        });
        let parsed = px(declared).unwrap_or_else(|| {
            panic!(
                "`{selector}` `scroll-margin-top` must be whole integer pixels, \
                 found `{declared}`: {decls:?}"
            )
        });
        assert_eq!(
            parsed, ANCHOR_CLEARANCE,
            "`{selector}` `scroll-margin-top` must be {ANCHOR_CLEARANCE}px \
             (2 baselines of clearance for anchor-link navigation), found \
             {parsed}px. The body has no padding, so this declaration alone \
             prevents an anchor-scrolled heading from sitting flush against the \
             pane's top edge. Any replacement value must ALSO be a whole \
             multiple of the {BASELINE}px baseline to stay on-grid — the sweep \
             enforces that much, but only this test pins the amount of \
             clearance: {decls:?}"
        );
    }
}

#[test]
fn heading_bottom_rule_is_gone_from_base_and_theme() {
    // h1/h2 no longer carry a section rule. Both halves of the deletion matter,
    // and the theme half is the one the `hr` bug taught us to check: a
    // `border-bottom-color` left behind in `themes/default.css` would paint
    // nothing today (there is no border to color) and would therefore sit there
    // silently until someone re-adds a border and gets a surprise.
    let css = rendered_stylesheet();

    for &(selector, font_size, _) in HEADING_SCALE
        .iter()
        .filter(|&&(selector, _, _)| matches!(selector, "h1" | "h2"))
    {
        let decls = rule_decls(&css, selector);

        // Sanity: `rule_decls` merges every matching rule, so a non-empty map
        // proves nothing — the shared `h1, h2, … h6` rule and the theme's
        // `h1…h6 { color }` rule would keep it populated even if the per-level
        // `body.wolf-markdown {selector}` rule vanished entirely. Assert a
        // declaration ONLY that per-level rule supplies (`font-size`; the shared
        // rule carries font-weight/scroll-margin-top, the theme only color), so
        // the two absence assertions below are about a rule that is genuinely
        // still being parsed rather than vacuously true of a lost rule.
        assert_eq!(
            decls.get("font-size").and_then(|v| px(v)),
            Some(font_size),
            "`{selector}` must still declare its own `font-size: {font_size}px` \
             from the per-level rule in base.css. Without this the two \
             `border-bottom`/`padding-bottom` absence checks below would pass \
             trivially if that rule were lost or renamed, since the merged map \
             stays non-empty from the shared heading rule and the theme's color \
             rule alone: {decls:?}"
        );

        assert_eq!(
            decls.get("border-bottom"),
            None,
            "`{selector}` must NOT declare a `border-bottom` — the section rule \
             under h1/h2 was deliberately removed. Reintroducing one is not a \
             one-line change: a bare {BORDER}px border on a 48px content box \
             makes the heading 49px tall and pushes everything below it off the \
             {BASELINE}px grid, so it also requires re-adding the 23px \
             `padding-bottom` compensation (23 + {BORDER} = {BASELINE}) AND a \
             matching COMPENSATION_ALLOWLIST entry naming the border it pays \
             for: {decls:?}"
        );
        assert_eq!(
            decls.get("padding-bottom"),
            None,
            "`{selector}` must NOT declare a `padding-bottom` at all. The 23px \
             that used to live here existed ONLY to compensate the {BORDER}px \
             `border-bottom`; with the border gone the padding has nothing to \
             pay for, and even an explicit `0` is noise (the UA default is \
             already 0, and h3..h6 declare no padding either) that the grid \
             sweep would then have to parse: {decls:?}"
        );
    }

    // The theme half. Substring checks against the WHOLE document, not
    // `rule_decls`: the light palette lives inside the one `@media
    // (prefers-color-scheme: light)` block, which the crude parser deliberately
    // cannot address (see `style_block`). One check covers BOTH palettes.
    //
    // Note for future authors: this is a substring check over the whole emitted
    // document, and base.css's header comment ships inside that document. So
    // merely MENTIONING `border-bottom-color` in CSS prose will fail this test,
    // not just declaring it. That is deliberate — a comment describing a border
    // color is a strong hint someone is about to re-add one — but if you trip it
    // from a comment, the fix is to reword the comment, not to loosen the check.
    let html = render_html(
        "# Title\n\n## Section\n",
        PreviewTheme::Default,
        WidthMode::Full,
    );
    assert!(
        !html.contains("border-bottom-color"),
        "no `border-bottom-color` may survive anywhere in the emitted \
         stylesheets — base.css no longer gives h1/h2 a bottom border, so any \
         remaining color declaration paints nothing. That is exactly what makes \
         it dangerous: it is invisible today and becomes a loaded gun for \
         whoever next adds a border. This single check covers BOTH the dark \
         palette and the `prefers-color-scheme: light` block: {html}"
    );
}

#[test]
fn blockquote_keeps_its_indent_without_a_border() {
    // The border-left is gone; the padding that used to sit BESIDE it is now the
    // only thing marking a quote off from the surrounding text.
    let css = rendered_stylesheet();
    let decls = rule_decls(&css, "blockquote");

    assert_eq!(
        decls.get("padding").map(String::as_str),
        Some("0 24px"),
        "`blockquote` padding must be `0 24px`. The horizontal {BASELINE}px IS \
         the indent — with the `border-left` removed and the body itself flush \
         to the pane edges, this padding is the ONLY thing holding a quote in \
         from the edge, so shrinking it to 0 erases the quote's last structural \
         cue. The VERTICAL `0` is separately load-bearing: it is why blockquote \
         needs no border compensation at all, since border-box height = content \
         height = a sum of on-grid child boxes: {decls:?}"
    );
    assert_eq!(
        decls.get("margin").map(String::as_str),
        Some("0 0 24px"),
        "`blockquote` margin must be `0 0 24px` — one baseline below, zero \
         above: {decls:?}"
    );
    assert_eq!(
        decls.get("border-left"),
        None,
        "`blockquote` must NOT declare a `border-left`; the quote is marked by \
         its {BASELINE}px indent (plus the theme's dimmer text color) alone: \
         {decls:?}"
    );

    // Theme half, same argument as `heading_bottom_rule_is_gone_from_base_and_theme`:
    // substring over the whole document so the one check covers both palettes.
    let html = render_html("> quoted\n", PreviewTheme::Default, WidthMode::Full);
    assert!(
        !html.contains("border-left-color"),
        "no `border-left-color` may survive anywhere in the emitted stylesheets \
         — base.css no longer gives blockquote a left border, so the color \
         declaration paints nothing and merely lies in wait for the next person \
         to add one. Covers both the dark palette and the \
         `prefers-color-scheme: light` block: {html}"
    );
}

#[test]
fn hr_occupies_whole_baselines_and_is_painted_by_its_border() {
    let css = rendered_stylesheet();
    let decls = rule_decls(&css, "hr");

    // base.css structure: the rule is absorbed into an explicit border-box
    // height, so the hr's total footprint is 24 (box) + 24 (margin-top) = 2B.
    assert_eq!(
        decls.get("height").map(String::as_str),
        Some("24px"),
        "`hr` must have an explicit {BASELINE}px height — with the inherited \
         `box-sizing: border-box` this ABSORBS its {BORDER}px rule structurally. \
         A 1px-tall hr plus margins totals 49px and margins alone cannot fix it \
         (an hr before an h2 collapses to 48 and re-exposes the 1px): {decls:?}"
    );
    let border_top = decls
        .get("border-top")
        .unwrap_or_else(|| panic!("`hr` must draw its rule with `border-top`: {decls:?}"));
    assert!(
        border_top.contains("1px"),
        "`hr` must draw a {BORDER}px `border-top` (color from the theme), \
         found `{border_top}`: {decls:?}"
    );
    assert_eq!(
        decls.get("margin").map(String::as_str),
        Some("24px 0 0"),
        "`hr` margin must be `24px 0 0`: one baseline above, ZERO below — the \
         {BASELINE}px box already supplies the clear space beneath the rule, so \
         total = {BASELINE} + {BASELINE} = 48px = 2 baselines: {decls:?}"
    );
    assert_eq!(
        decls.get("background").map(String::as_str),
        Some("transparent"),
        "`hr` background must be `transparent` in base.css — the box is now \
         {BASELINE}px tall, so ANY background paints a solid {BASELINE}px bar \
         instead of a hairline: {decls:?}"
    );

    // The important half: the theme guard. `themes/default.css` is emitted
    // AFTER base.css, so a leftover `hr { background-color: … }` WINS the
    // cascade and paints the 24px box as a solid bar — the highest-consequence
    // regression in this change. Substring checks, not `rule_decls`: the light
    // palette lives inside the one `@media` block, which the crude parser
    // cannot address (see `style_block`).
    let html = render_html("---\n", PreviewTheme::Default, WidthMode::Full);
    assert!(
        html.contains("border-top-color: #3a3a3a"),
        "the DARK theme must color the hr via `border-top-color: #3a3a3a`, not \
         `background-color`. base.css is emitted BEFORE the theme, so a leftover \
         `hr {{ background-color }}` wins the cascade and paints the hr's {BASELINE}px \
         grid box as a SOLID BAR instead of a hairline: {html}"
    );
    assert!(
        html.contains("border-top-color: #d0d0d0"),
        "the LIGHT (`prefers-color-scheme: light`) theme must color the hr via \
         `border-top-color: #d0d0d0`, not `background-color` — otherwise the hr \
         renders as a solid {BASELINE}px bar in light mode only: {html}"
    );
}

#[test]
fn table_cells_land_two_baselines_per_row() {
    const CELL_PADDING_TOP: i32 = 12;
    const CELL_PADDING_BOTTOM: i32 = 11;
    const CELL_LINE_HEIGHT: i32 = 24;
    const ROW_PITCH: i32 = 48;
    const TABLE_MARGIN_BOTTOM: i32 = 23;

    let css = rendered_stylesheet();
    let cells = rule_decls(&css, "th, td");

    assert_eq!(
        cells.get("padding").map(String::as_str),
        Some("12px 16px 11px"),
        "`th, td` padding must be `12px 16px 11px`. The 12/11 asymmetry is NOT a \
         typo: with `border-collapse: collapse` adjacent rows SHARE one {BORDER}px \
         border, so the row pitch is {CELL_PADDING_TOP} + {CELL_LINE_HEIGHT} \
         (line-height) + {CELL_PADDING_BOTTOM} + {BORDER} = {ROW_PITCH}px = 2 \
         baselines. 47 is odd, so the padding cannot be split evenly; the 1px \
         asymmetry is imperceptible. (16px horizontal is not grid-constrained.): \
         {cells:?}"
    );
    let border = cells
        .get("border")
        .unwrap_or_else(|| panic!("`th, td` must declare a border: {cells:?}"));
    assert!(
        border.contains("1px"),
        "`th, td` border must be {BORDER}px — the 12/11 padding compensates \
         exactly one shared collapsed border. Found `{border}`: {cells:?}"
    );
    assert_eq!(
        CELL_PADDING_TOP + CELL_LINE_HEIGHT + CELL_PADDING_BOTTOM + BORDER,
        ROW_PITCH,
        "row-pitch arithmetic: {CELL_PADDING_TOP} + {CELL_LINE_HEIGHT} + \
         {CELL_PADDING_BOTTOM} + {BORDER} must equal {ROW_PITCH} (2 baselines)"
    );

    let table = rule_decls(&css, "table");
    assert_eq!(
        table.get("border-collapse").map(String::as_str),
        Some("collapse"),
        "`table` must set `border-collapse: collapse` — the whole 2-baseline row \
         pitch depends on adjacent rows SHARING one border: {table:?}"
    );
    assert_eq!(
        table.get("margin").map(String::as_str),
        Some("0 0 23px"),
        "`table` margin-bottom must be {TABLE_MARGIN_BOTTOM}px, not {BASELINE}px \
         — this looks like a typo and is not. A collapsed-border table of N rows \
         has N+1 horizontal lines, so its height is 47N + (N+1) = 48N + {BORDER}: \
         the trailing {BORDER}px is the table's OWN bottom outer border, which \
         has no next row to share with. {TABLE_MARGIN_BOTTOM} + {BORDER} = \
         {BASELINE} brings the total to 48N + {BASELINE} = exactly 2N+1 \
         baselines. Documented residual: whenever the table is immediately \
         followed by a block whose margin-top EXCEEDS {TABLE_MARGIN_BOTTOM}px — \
         every heading (h1/h2/h3 at 48px, h4/h5/h6 at {BASELINE}px) and `hr` \
         ({BASELINE}px) — the {TABLE_MARGIN_BOTTOM}px collapses away against the \
         larger value and re-exposes the {BORDER}px; that error is constant, not \
         accumulating: {table:?}"
    );
}

#[test]
fn pre_sits_on_the_baseline_grid() {
    let css = rendered_stylesheet();
    let decls = rule_decls(&css, "pre");

    assert_eq!(
        decls.get("line-height").map(String::as_str),
        Some("24px"),
        "`pre` line-height must be one baseline ({BASELINE}px) so code lines \
         share the body baseline exactly. The only tighter grid-legal value is \
         12px, which is smaller than the 14px code font — lines would overlap: \
         {decls:?}"
    );
    assert_eq!(
        decls.get("padding").map(String::as_str),
        Some("24px"),
        "`pre` padding must be one baseline on every side, so the block totals \
         {BASELINE} + {BASELINE}n + {BASELINE} = {BASELINE}(n+2) — always a whole \
         multiple, with ZERO compensation needed (a `pre` has no border): \
         {decls:?}"
    );
    assert_eq!(
        decls.get("margin").map(String::as_str),
        Some("0 0 24px"),
        "`pre` margin must be `0 0 24px`: {decls:?}"
    );

    // Regression guard for the 0.81em compounding bug: `code` set 0.9em and
    // `pre code` ALSO set 0.9em, so code inside a <pre> rendered at 0.81em.
    // The size is now set once, on `pre`.
    let pre_code = rule_decls(&css, "pre code");
    assert_eq!(
        pre_code.get("font-size").map(String::as_str),
        Some("inherit"),
        "`pre code` font-size must be `inherit`, NOT a second em-relative value \
         — `code` already sets an em-relative size, so any relative value here \
         COMPOUNDS (0.9em x 0.9em = 0.81em). The size is set once, on `pre`: \
         {pre_code:?}"
    );
    assert_eq!(
        decls.get("font-size").map(String::as_str),
        Some("14px"),
        "`pre` must set the code font-size once, in absolute px: {decls:?}"
    );
}

#[test]
fn blockquote_last_child_margin_is_zeroed() {
    // One of the three rules that exist ONLY to prevent a specific layout bug,
    // and the only one the rest of the suite does not reach:
    //   - `pre code { font-size: inherit }` is guarded by
    //     `pre_sits_on_the_baseline_grid`;
    //   - `li > ul, li > ol { margin-bottom: 0 }` is incidentally guarded by the
    //     sweep, which asserts that every GRID_SELECTORS rule is non-empty;
    //   - this one has no guard at all and can currently be deleted silently.
    //
    // What deleting it would cause: a blockquote's last child (typically a <p>,
    // which carries `margin: 0 0 24px`) keeps its bottom margin, and because
    // blockquote has NO vertical padding, that margin has nothing to collapse
    // through — the quote renders 24px TALLER than its content. That is still
    // on-grid, so the sweep would never notice, but it is visually lopsided
    // against the quote's 0 top padding: air below the text, none above.
    let css = rendered_stylesheet();
    let decls = rule_decls(&css, "blockquote > :last-child");

    assert!(
        !decls.is_empty(),
        "base.css must keep the `blockquote > :last-child` rule. Without it a \
         blockquote is one baseline taller than its content (its last <p> keeps \
         `margin-bottom: 24px`, and blockquote's 0 vertical padding gives that \
         margin nothing to collapse through) — on-grid, but lopsided against the \
         quote's 0 top padding"
    );
    assert_eq!(
        decls.get("margin-bottom").and_then(|v| px(v)),
        Some(0),
        "`blockquote > :last-child` must zero `margin-bottom`; any other value \
         re-introduces the trailing gap this rule exists to remove: {decls:?}"
    );
}

#[test]
fn stylesheets_are_emitted_in_base_then_math_then_theme_order() {
    // Emission order is load-bearing, not cosmetic: the three stylesheets share
    // one <style> block at equal specificity, so on any conflicting property the
    // LAST one wins. Plan §10 flags this as a MEDIUM risk for math.css (which is
    // vendored and must not be edited — it wins over base.css on anything it
    // touches), and the `hr` theme guard in
    // `hr_occupies_whole_baselines_and_is_painted_by_its_border` proves the same
    // ordering for the theme: base.css sets `background: transparent` on a 24px
    // hr box, and only because the theme comes AFTER can it repaint that box —
    // which is exactly why the theme must use `border-top-color` and not
    // `background-color`. Reorder these and the grid's compensation rules start
    // losing cascades silently, with no other test noticing.
    //
    // Markers are chosen to be stable and unique to one file: `--wolf-baseline`
    // is base.css's documented grid token (and is separately asserted by
    // `baseline_unit_is_declared`); `menv-cases` is a vendored pulldown-latex
    // class name in math.css that this repo does not author; `color-scheme:
    // light dark` is the theme declaration `default_theme_carries_dark_and_light_palettes`
    // already pins. None can be edited away without failing another test first.
    let source = "# Title\n\nInline $e=mc^2$ and a rule.\n\n---\n";
    let html = render_html(source, PreviewTheme::Default, WidthMode::Full);

    let base = html.find("--wolf-baseline").unwrap_or_else(|| {
        panic!("expected base.css's `--wolf-baseline` token in the rendered document")
    });
    let math = html
        .find("menv-cases")
        .unwrap_or_else(|| panic!("expected math.css's vendored `menv-cases` selector"));
    let theme = html.find("color-scheme: light dark").unwrap_or_else(|| {
        panic!("expected themes/default.css's `color-scheme: light dark` declaration")
    });

    assert!(
        base < math,
        "base.css (at {base}) must be emitted BEFORE math.css (at {math}) — \
         math.css is vendored and DELIBERATELY wins on the properties it sets \
         inside <math>, which is the accepted display-MathML drift. Flipping \
         this would let base.css's grid silently override vendored math layout"
    );
    assert!(
        math < theme,
        "math.css (at {math}) must be emitted BEFORE the theme (at {theme}) so \
         theme colors win over any color math.css sets"
    );
    assert!(
        base < theme,
        "base.css (at {base}) must be emitted BEFORE the theme (at {theme}): the \
         theme layers color onto base.css's structural rules and MUST win. This \
         is why the theme paints `hr` with `border-top-color` rather than \
         `background-color` — after base.css gave it a 24px grid box, a theme \
         `background-color` would win the cascade and paint a solid bar"
    );
}

#[test]
fn every_vertical_dimension_is_on_the_baseline_grid() {
    // The sweep. Over every styled selector, every vertical dimension must be
    // 0 or a whole multiple of 24px — except the three documented compensations.
    // Any NEW off-grid vertical value anywhere in base.css fails here, and any
    // new compensated value forces its author to add an allowlist entry naming
    // the border it pays for.
    let css = rendered_stylesheet();
    let mut violations: Vec<String> = Vec::new();

    for &selector in GRID_SELECTORS {
        let decls = rule_decls(&css, selector);
        assert!(
            !decls.is_empty(),
            "the sweep expects `{selector}` to be a styled rule in base.css; \
             either the rule was removed (update GRID_SELECTORS) or its selector \
             text changed shape in a way the crude parser cannot address"
        );

        // (effective vertical property, declared value) pairs: the longhands as
        // written, plus the block edges expanded out of any shorthand.
        let mut checks: Vec<(String, String)> = Vec::new();
        for &prop in VERTICAL_LONGHANDS {
            if let Some(value) = decls.get(prop) {
                checks.push((prop.to_string(), value.clone()));
            }
        }
        for shorthand in ["margin", "padding"] {
            let Some(value) = decls.get(shorthand) else {
                continue;
            };
            let Some((top, bottom)) = shorthand_block_edges(value) else {
                violations.push(format!(
                    "`{selector}` has an unparseable `{shorthand}: {value}` \
                     shorthand; the sweep cannot verify its block edges"
                ));
                continue;
            };
            checks.push((format!("{shorthand}-top"), top));
            checks.push((format!("{shorthand}-bottom"), bottom));
        }

        for (prop, value) in checks {
            let Some(parsed) = px(&value) else {
                violations.push(format!(
                    "`{selector}` `{prop}: {value}` is not a whole number of \
                     pixels. Every vertical dimension must be written as literal \
                     integer px (not em, rem, %, or calc()): a fractional or \
                     relative length cannot be verified against the {BASELINE}px \
                     grid, and rem in particular resolves against a root \
                     font-size this document never sets"
                ));
                continue;
            };

            let allowed = COMPENSATION_ALLOWLIST
                .iter()
                .any(|&(s, p, v)| s == selector && p == prop && v == parsed);
            if allowed || parsed % BASELINE == 0 {
                continue;
            }

            violations.push(format!(
                "`{selector}` `{prop}: {value}` is {parsed}px — neither 0, nor a \
                 whole multiple of the {BASELINE}px baseline ({parsed} % \
                 {BASELINE} = {}), nor a documented compensation. If this value \
                 exists to compensate a border, add it to \
                 COMPENSATION_ALLOWLIST with a comment naming the border it \
                 compensates and showing that <value> + <border width> is a \
                 whole multiple of {BASELINE}; otherwise put it back on the grid",
                parsed % BASELINE
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "{} vertical dimension(s) are off the {BASELINE}px baseline grid:\n  - {}",
        violations.len(),
        violations.join("\n  - ")
    );
}

// ---------------------------------------------------------------------------
// Edge-reset assertions: the RENDERED structure, not the declaration text.
//
// Every test above audits `base.css` as TEXT. That is structurally unable to
// catch the class of bug where a declaration exists, is spelled correctly, and
// still applies to the wrong element — which is exactly what the NAIVE spelling
// of the two edge rules does:
//
//   body.wolf-markdown > *:first-child { margin-top: 0 }
//   body.wolf-markdown > *:last-child  { margin-bottom: 0 }
//
// The scroll-sync `MarkerInjector` flushes its column-ZERO `<span
// class="wolf-src">` anchors as raw HTML BEFORE each block's opening tag (its
// per-100-column anchors go inside the block's inline content instead, which is
// irrelevant here — they are never direct children of the body), and the
// `wl-1-0` anchor has byte offset 0 — so the body's actual first child is an
// ANCHOR, not the first block:
//
//   <body class="wolf-markdown"><span class="wolf-src" id="wl-1-0" …></span><h1 …>
//
// `:first-child` would therefore match a span that has no margin to zero,
// leaving the <h1> with its full 48px margin-top. The body has NO padding, so
// that margin resolves one of two ways, and it does not matter which: either it
// collapses out through the body's top edge (the body satisfies every condition
// for parent-child collapsing — no border-top, no padding-top, no clearance, no
// BFC), displacing the whole document 48px down; or, if the leading anchor's
// anonymous block box blocks the collapse-through, the 48px sits INSIDE the body
// between the anchor and the <h1>. Both outcomes put the opening heading 48px
// below the top edge and defeat flush-to-edge. (The escaped-margin case is
// positional, NOT a color gap: neither base.css nor the theme sets a background
// on `html`, so the body's background propagates to the canvas and paints the
// displaced region.) `base.css` consequently spells the opening-block rule as
//
//   body.wolf-markdown > *:not(.wolf-src):not(*:not(.wolf-src) ~ *)
//
// — "not an anchor, and no non-anchor sibling precedes it" — which skips a
// leading anchor run of ANY length. Symmetrically, `flush_before(usize::MAX)`
// emits trailing anchors AFTER the last block, so `:last-child` matches an
// anchor too. Note 48 is 2 baselines: the grid invariant SURVIVES, so the sweep
// cannot see this either.
//
// These tests therefore pin the OUTCOME — "the declaration that must reach the
// opening/closing block actually wins on that block" — by parsing the emitted
// body into its child list and resolving the cascade against it. They are
// deliberately agnostic about HOW that is achieved: any selector shape that
// skips the anchors passes, and so would a change that stopped emitting a
// leading anchor at all.
// ---------------------------------------------------------------------------

/// A direct element child of `<body class="wolf-markdown">`.
#[derive(Debug, Clone)]
struct BodyChild {
    tag: String,
    classes: Vec<String>,
}

impl BodyChild {
    /// True for the scroll-sync `<span class="wolf-src">` anchors — the elements
    /// that displace `:first-child` / `:last-child` off the real content.
    fn is_anchor(&self) -> bool {
        self.classes.iter().any(|c| c == "wolf-src")
    }
}

/// Elements that never open a subtree, so the depth scan in [`body_children`]
/// must not push for them. Only `hr`/`img`/`br` can occur in rendered Markdown;
/// the rest are listed for safety.
const VOID_TAGS: &[&str] = &["hr", "img", "br", "meta", "link", "input", "source"];

/// The `class` attribute of a start tag, split on whitespace.
fn tag_classes(tag_text: &str) -> Vec<String> {
    let Some(at) = tag_text.find("class=\"") else {
        return Vec::new();
    };
    let rest = &tag_text[at + "class=\"".len()..];
    let Some(end) = rest.find('"') else {
        return Vec::new();
    };
    rest[..end].split_whitespace().map(str::to_string).collect()
}

/// The direct element children of the rendered `<body>`, in document order.
///
/// # Why a crude scanner is legitimate here
///
/// Same argument as [`style_block`]: this is not an HTML parser. It is a depth
/// counter over `<`…`>` runs, valid because the fixtures below are deliberately
/// plain Markdown (headings and paragraphs) whose rendered attribute values
/// contain no `>`, and because the only thing being extracted is the TOP-LEVEL
/// child sequence — tag name and classes. Do not feed it tables, MathML, or raw
/// HTML blocks; assert on something else instead.
fn body_children(html: &str) -> Vec<BodyChild> {
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

    let mut children = Vec::new();
    let mut depth = 0usize;
    let mut rest = &html[start..start + end];
    while let Some(lt) = rest.find('<') {
        let after = &rest[lt..];
        let Some(gt) = after.find('>') else { break };
        let tag_text = &after[..=gt];
        rest = &after[gt + 1..];

        if tag_text.starts_with("</") {
            depth = depth.saturating_sub(1);
            continue;
        }
        if tag_text.starts_with("<!") {
            continue;
        }
        let tag: String = tag_text[1..]
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        if tag.is_empty() {
            continue;
        }
        if depth == 0 {
            children.push(BodyChild {
                tag: tag.clone(),
                classes: tag_classes(tag_text),
            });
        }
        if !tag_text.ends_with("/>") && !VOID_TAGS.contains(&tag.as_str()) {
            depth += 1;
        }
    }
    children
}

/// A one-line summary of the body's child list, for failure messages.
fn describe(children: &[BodyChild]) -> String {
    children
        .iter()
        .map(|c| {
            if c.is_anchor() {
                format!("<{} class=wolf-src>", c.tag)
            } else {
                format!("<{}>", c.tag)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A stand-in for whitespace INSIDE a functional argument. Tokenization below
/// splits on whitespace, so an argument that contains any (`:not(*:not(.wolf-src)
/// ~ *)`) would be torn into several tokens. Substituting a character that is
/// not `char::is_whitespace` keeps the argument one token; it is restored to a
/// plain space the moment tokenization is done, so every consumer downstream
/// sees the argument's real internal structure.
const ARG_SPACE: char = '\u{1}';

/// Splits a selector into `(combinator, compound)` steps, left to right. The
/// first step's combinator is `'\0'`; a descendant combinator is `' '`.
///
/// Combinator characters inside `:not(…)` / `:has(…)` are left alone, and so is
/// whitespace inside them: it round-trips through [`ARG_SPACE`] so the argument
/// survives as ONE token WITHOUT losing its structure. A functional argument may
/// therefore itself be a complex selector — `:not(*:not(.wolf-src) ~ *)` — and
/// [`complex_matches`] evaluates it. Descendant and `>` combinators inside an
/// argument are still not modelable against the flat body-child list, but they
/// are now DETECTED (the whitespace is still there to see) and reported as
/// unsupported rather than silently misread as a single compound.
fn selector_steps(entry: &str) -> Vec<(char, String)> {
    let mut spaced = String::with_capacity(entry.len() * 2);
    let mut depth = 0usize;
    for ch in entry.chars() {
        match ch {
            '(' => {
                depth += 1;
                spaced.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                spaced.push(ch);
            }
            _ if ch.is_whitespace() && depth > 0 => spaced.push(ARG_SPACE),
            '>' | '+' | '~' if depth == 0 => {
                spaced.push(' ');
                spaced.push(ch);
                spaced.push(' ');
            }
            _ => spaced.push(ch),
        }
    }

    let mut steps: Vec<(char, String)> = Vec::new();
    let mut pending = '\0';
    for token in spaced.split_whitespace() {
        match token {
            ">" => pending = '>',
            "+" => pending = '+',
            "~" => pending = '~',
            compound => {
                let combinator = if steps.is_empty() {
                    '\0'
                } else if pending == '\0' {
                    ' '
                } else {
                    pending
                };
                steps.push((combinator, compound.replace(ARG_SPACE, " ")));
                pending = '\0';
            }
        }
    }
    steps
}

/// Splits a compound selector (`.wolf-src:first-child`) into its simple
/// selectors (`[".wolf-src", ":first-child"]`), respecting `(…)` nesting.
fn simple_selectors(compound: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in compound.chars() {
        if depth == 0 && matches!(ch, '.' | ':' | '#') && !current.is_empty() {
            parts.push(std::mem::take(&mut current));
        }
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        current.push(ch);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// The argument of a functional pseudo-class (`:not(X)` -> `X`).
///
/// Paren-BALANCED, so a nested functional argument yields the whole inner
/// selector: `:not(*:not(.wolf-src) ~ *)` -> `*:not(.wolf-src) ~ *`. (A naive
/// `strip_suffix(')')` on the inner text stops at the wrong paren and fails
/// outright whenever the argument does not END in its own `)`.) As before, the
/// closing paren must be the part's last character; anything trailing it is
/// syntax this helper does not describe, so it returns `None`.
fn functional_arg(part: &str) -> Option<&str> {
    let open = part.find('(')?;
    let mut depth = 0usize;
    for (offset, ch) in part[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    if open + offset + 1 != part.len() {
                        return None;
                    }
                    return Some(part[open + 1..open + offset].trim());
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the compound selector matches `children[idx]`.
///
/// `None` means "this test's matcher does not understand the syntax" — reported
/// loudly in the failure message rather than silently treated as a match, so an
/// unfamiliar-but-correct fix produces an actionable message instead of a false
/// pass or an inscrutable false failure.
///
/// `:not()` takes a COMPLEX selector (Selectors 4), so its argument is handed to
/// [`complex_matches`], not back to this function: `:not(*:not(.wolf-src) ~ *)`
/// means "has no non-anchor sibling before it". `:has()` still accepts only a
/// relative `+`/`~` compound.
fn compound_matches(compound: &str, children: &[BodyChild], idx: usize) -> Option<bool> {
    let child = children.get(idx)?;
    for part in simple_selectors(compound) {
        let matched = match part.as_str() {
            "*" => true,
            _ if part.starts_with('.') => child.classes.iter().any(|c| c == &part[1..]),
            ":first-child" => idx == 0,
            ":last-child" => idx + 1 == children.len(),
            _ if part.starts_with(":not(") => {
                let inner = functional_arg(&part)?;
                !complex_matches(inner, children, idx)?
            }
            _ if part.starts_with(":has(") => {
                let inner = functional_arg(&part)?;
                match inner.chars().next()? {
                    // `:has(+ X)` — the very next sibling.
                    '+' => {
                        let inner = inner[1..].trim();
                        idx + 1 < children.len() && compound_matches(inner, children, idx + 1)?
                    }
                    // `:has(~ X)` — any following sibling.
                    '~' => {
                        let inner = inner[1..].trim();
                        let mut any = false;
                        for j in idx + 1..children.len() {
                            any |= compound_matches(inner, children, j)?;
                        }
                        any
                    }
                    // A descendant / child argument: this matcher models only the
                    // flat body-child list, so it cannot answer.
                    _ => return None,
                }
            }
            _ if part.starts_with(':') || part.starts_with('#') => return None,
            tag => child.tag == tag.to_ascii_lowercase(),
        };
        if !matched {
            return Some(false);
        }
    }
    Some(true)
}

/// Whether a chain of sibling-combinator steps (rightmost last) matches
/// `children[idx]`.
fn sibling_chain_matches(
    steps: &[(char, String)],
    children: &[BodyChild],
    idx: usize,
) -> Option<bool> {
    let (combinator, compound) = steps.last()?;
    if !compound_matches(compound, children, idx)? {
        return Some(false);
    }
    let head = &steps[..steps.len() - 1];
    if head.is_empty() {
        return Some(true);
    }
    match combinator {
        '+' => {
            if idx == 0 {
                return Some(false);
            }
            sibling_chain_matches(head, children, idx - 1)
        }
        '~' => {
            let mut any = false;
            for j in 0..idx {
                any |= sibling_chain_matches(head, children, j)?;
            }
            Some(any)
        }
        _ => Some(false),
    }
}

/// Whether a COMPLEX selector — one or more compounds joined by combinators —
/// matches `children[idx]`, evaluated against the flat body-child list.
///
/// This is what makes a general first-block selector modelable. CSS has no
/// preceding-sibling combinator and `:has()` only looks forward, so "the first
/// block that is not a scroll-sync anchor" can only be spelled as a negation
/// over the elements BEFORE it:
///
/// ```css
/// body.wolf-markdown > *:not(.wolf-src):not(*:not(.wolf-src) ~ *)
/// ```
///
/// The inner `*:not(.wolf-src) ~ *` is a complex selector — a compound, a `~`,
/// and another compound — which [`compound_matches`] cannot represent. Routing
/// it here lets the negation resolve to "no non-anchor sibling precedes me",
/// which holds for the opening block whatever the LENGTH of the leading anchor
/// run (`.wolf-src:first-child + *` only ever covers a run of exactly one).
///
/// `None` — not `Some(false)` — for anything unmodelable, preserving the design
/// property that unevaluated syntax is COLLECTED and named in the failure
/// message rather than quietly counted as a non-match:
///
/// - a `>` or descendant combinator, which leaves the body-child list entirely;
/// - a leading (relative) combinator such as `:not(~ .wolf-src)`, which has no
///   subject inside this list to anchor against.
fn complex_matches(selector: &str, children: &[BodyChild], idx: usize) -> Option<bool> {
    let selector = selector.trim();
    if selector.starts_with(['+', '~', '>']) {
        return None;
    }
    let steps = selector_steps(selector);
    if steps.is_empty() {
        return Some(false);
    }
    if steps.len() == 1 {
        return compound_matches(&steps[0].1, children, idx);
    }
    if steps[1..].iter().any(|(c, _)| !matches!(c, '+' | '~')) {
        return None;
    }
    sibling_chain_matches(&steps, children, idx)
}

/// Whether one comma-separated selector entry matches the body child at `idx`.
///
/// `None` means unsupported syntax (see [`compound_matches`]).
fn selector_matches_body_child(entry: &str, children: &[BodyChild], idx: usize) -> Option<bool> {
    let steps = selector_steps(entry);
    if steps.is_empty() {
        return Some(false);
    }
    if steps.len() == 1 {
        return compound_matches(&steps[0].1, children, idx);
    }
    // Everything in base.css that can reach a body child is body-scoped.
    if steps[0].1 != "body.wolf-markdown" {
        return Some(false);
    }
    // The first hop must land ON a body child (child or descendant combinator);
    // any further `>`/descendant hop lands on a DESCENDANT of a body child, which
    // is never the element under test.
    if !matches!(steps[1].0, '>' | ' ') {
        return Some(false);
    }
    if steps[2..].iter().any(|(c, _)| !matches!(c, '+' | '~')) {
        return Some(false);
    }
    sibling_chain_matches(&steps[1..], children, idx)
}

/// CSS specificity as `(ids, classes, types)`; `:not()`/`:has()` contribute
/// their argument's specificity.
fn specificity(entry: &str) -> (u32, u32, u32) {
    let mut acc = (0, 0, 0);
    for (_, compound) in selector_steps(entry) {
        for part in simple_selectors(&compound) {
            add_specificity(&part, &mut acc);
        }
    }
    acc
}

fn add_specificity(part: &str, acc: &mut (u32, u32, u32)) {
    if part == "*" {
        return;
    }
    if part.starts_with('#') {
        acc.0 += 1;
    } else if part.starts_with('.') {
        acc.1 += 1;
    } else if part.starts_with(':') {
        if let Some(inner) = functional_arg(part) {
            let inner = inner.trim_start_matches(|c| matches!(c, '+' | '~' | '>' | ' '));
            let (a, b, c) = specificity(inner);
            acc.0 += a;
            acc.1 += b;
            acc.2 += c;
        } else if part.starts_with("::") {
            acc.2 += 1;
        } else {
            acc.1 += 1;
        }
    } else {
        acc.2 += 1;
    }
}

/// Every top-level rule as `(selector list, declarations)`, in document order.
///
/// Truncated at the first `@media` for the reason given on [`style_block`]: the
/// brace scan desynchronizes inside it, and everything these tests care about
/// (all of `base.css`) precedes it.
fn stylesheet_rules(css: &str) -> Vec<(String, HashMap<String, String>)> {
    let css = strip_comments(css);
    let scan = match css.find("@media") {
        Some(at) => &css[..at],
        None => css.as_str(),
    };

    let mut rules = Vec::new();
    let mut rest = scan;
    while let Some(open) = rest.find('{') {
        let selectors = normalize_ws(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let body = &after[..close];
        rest = &after[close + 1..];

        let mut decls = HashMap::new();
        for decl in body.split(';') {
            let Some((prop, value)) = decl.split_once(':') else {
                continue;
            };
            let prop = normalize_ws(prop).to_ascii_lowercase();
            let value = normalize_ws(value);
            if !prop.is_empty() && !value.is_empty() {
                decls.insert(prop, value);
            }
        }
        rules.push((selectors, decls));
    }
    rules
}

/// The vertical margin edge a rule declares, whether written as a longhand or
/// as the corresponding edge of a `margin` shorthand. (No rule in `base.css`
/// writes both; if one ever does, the longhand is taken.)
fn declared_margin_edge(decls: &HashMap<String, String>, top: bool) -> Option<String> {
    let longhand = if top { "margin-top" } else { "margin-bottom" };
    if let Some(value) = decls.get(longhand) {
        return Some(value.clone());
    }
    let (t, b) = shorthand_block_edges(decls.get("margin")?)?;
    Some(if top { t } else { b })
}

/// The declaration that WINS `margin-top` (or `margin-bottom`) on
/// `children[idx]`: highest specificity, last rule breaking ties — which is the
/// whole cascade at equal origin/importance, since no `!important` exists in
/// this stylesheet.
///
/// Returns the winner as `(selector list, value)` plus any selector entries the
/// matcher could not evaluate, so a failure can say so out loud.
fn winning_margin(
    css: &str,
    children: &[BodyChild],
    idx: usize,
    top: bool,
) -> (Option<(String, String)>, Vec<String>) {
    let mut unsupported = Vec::new();
    let mut winner: Option<((u32, u32, u32), usize, String, String)> = None;

    for (order, (selectors, decls)) in stylesheet_rules(css).into_iter().enumerate() {
        let Some(value) = declared_margin_edge(&decls, top) else {
            continue;
        };
        let mut best: Option<(u32, u32, u32)> = None;
        for entry in selectors.split(',').map(normalize_ws) {
            if entry.is_empty() {
                continue;
            }
            match selector_matches_body_child(&entry, children, idx) {
                Some(true) => {
                    let spec = specificity(&entry);
                    if best.is_none_or(|b| spec > b) {
                        best = Some(spec);
                    }
                }
                Some(false) => {}
                None => unsupported.push(entry),
            }
        }
        let Some(spec) = best else { continue };
        let candidate = (spec, order, selectors, value);
        if winner
            .as_ref()
            .is_none_or(|(s, o, _, _)| (spec, order) > (*s, *o))
        {
            winner = Some(candidate);
        }
    }

    (
        winner.map(|(_, _, selectors, value)| (selectors, value)),
        unsupported,
    )
}

/// A rendering note for the failure messages below: which anchors displace the
/// edge, spelled out from the actual child list.
fn anchor_note(children: &[BodyChild], idx: usize, leading: bool) -> String {
    let displacing = if leading {
        children[..idx].iter().filter(|c| c.is_anchor()).count()
    } else {
        children[idx + 1..].iter().filter(|c| c.is_anchor()).count()
    };
    let edge = if leading { "before" } else { "after" };
    format!(
        "{displacing} `wolf-src` scroll-sync anchor(s) sit {edge} it in the \
         rendered body [{}]",
        describe(children)
    )
}

#[test]
fn first_child_margin_reset_reaches_the_opening_heading() {
    // A document that OPENS with a heading — the exact case the opening-block
    // rule, `> *:not(.wolf-src):not(*:not(.wolf-src) ~ *) { margin-top: 0 }`,
    // exists for.
    let html = render_html(
        "# Title\n\nBody text.\n",
        PreviewTheme::Default,
        WidthMode::Full,
    );
    let children = body_children(&html);
    assert!(
        !children.is_empty(),
        "the rendered body must have element children: {html}"
    );

    let idx = children
        .iter()
        .position(|c| !c.is_anchor())
        .unwrap_or_else(|| panic!("the body must contain a content block: {html}"));
    assert_eq!(
        children[idx].tag,
        "h1",
        "fixture sanity: the first content block must be the <h1>, found \
         [{}]",
        describe(&children)
    );

    let css = style_block(&html).to_string();
    let (winner, unsupported) = winning_margin(&css, &children, idx, true);

    let (selector, value) = winner.unwrap_or_else(|| {
        panic!(
            "NO rule sets a `margin-top` on the document's opening <h1>, so it \
             cannot even be inspected — {}. Unevaluated selectors: {unsupported:?}",
            anchor_note(&children, idx, true)
        )
    });
    assert_eq!(
        px(&value),
        Some(0),
        "the opening <h1>'s winning `margin-top` is `{value}` (from `{selector}`), \
         but it MUST resolve to 0.\n\n\
         Why: the body has NO padding — it is deliberately flush to the pane's \
         edges — so an unreset 48px `margin-top` on the opening block resolves \
         one of two ways. Either it collapses out through the body's top edge \
         (the body has no border-top, no padding-top, no clearance and \
         establishes no BFC, so parent-child collapsing applies) and displaces \
         the WHOLE document 48px down; or, if the leading anchor's anonymous \
         block box blocks the collapse-through, the 48px sits inside the body \
         between that box and the heading. Which one it is depends on the \
         anonymous container's inherited strut and is engine-dependent — and it \
         does not matter, because BOTH put the opening heading 48px below the \
         top edge and defeat flush-to-edge. (In the collapse-out case there is \
         no visible COLOR gap: neither base.css nor the theme sets a background \
         on `html`, so the body's background propagates to the canvas and paints \
         the escaped margin. The symptom is purely positional.) That is \
         what `body.wolf-markdown > *:not(.wolf-src):not(*:not(.wolf-src) ~ *) \
         {{ margin-top: 0 }}` is for — but {}. That selector reads \"not an \
         anchor, and no non-anchor sibling precedes it\", so it deliberately \
         SKIPS a leading anchor run of any length and lands on the first REAL \
         block. If this fails, the rule has either been narrowed back to \
         something structural — `:first-child` matches the first element child \
         whatever it is, so it lands on an anchor span that has no margin to \
         zero — or it has lost the cascade to a later/more specific \
         `margin-top`. Note 48px is still a multiple of {BASELINE}, so the grid \
         sweep passes and only this test can see the bug.\n\n\
         The fix belongs in the SELECTOR — it must reach the first non-anchor \
         block — and must NOT touch the `.wolf-src` rule itself, whose \
         `font-size: 0` / `line-height: 0` is load-bearing for scroll-sync.\n\n\
         Unevaluated selectors (extend this test's matcher if the fix is \
         spelled with syntax it does not model): {unsupported:?}",
        anchor_note(&children, idx, true)
    );
}

/// The shortest leading anchor run this fixture must produce to be testing
/// anything: a run of ONE is already covered by the test above, and would still
/// be covered by a narrow `.wolf-src:first-child + *` selector — the shape
/// `base.css` deliberately does NOT use.
const MIN_LEADING_ANCHORS: usize = 2;

#[test]
fn first_child_margin_reset_survives_a_multi_anchor_leading_run() {
    // The same outcome as the test above, but with a leading anchor run LONGER
    // THAN ONE. Leading blank lines advance the first block's source line past
    // 0, so the renderer flushes a marker per skipped line before the opening
    // <h1>. Blank lines are now the ONLY way to stack a leading run: a heading
    // past 100 columns no longer works, because its per-100-column anchors are
    // placed INSIDE the <h1> rather than before it. Nothing about this is exotic
    // — a document that opens with a blank line is ordinary — so the reset must
    // hold for a run of ANY length, not just one.
    let html = render_html(
        "\n\n# Title\n\nBody.\n",
        PreviewTheme::Default,
        WidthMode::Full,
    );
    let children = body_children(&html);

    let idx = children
        .iter()
        .position(|c| !c.is_anchor())
        .unwrap_or_else(|| panic!("the body must contain a content block: {html}"));
    assert_eq!(
        children[idx].tag,
        "h1",
        "fixture sanity: the first content block must be the <h1>, found [{}]",
        describe(&children)
    );
    assert!(
        idx >= MIN_LEADING_ANCHORS,
        "fixture sanity: this test only means something if MORE THAN ONE anchor \
         precedes the opening block, and this source produced {idx}: [{}].\n\n\
         If marker emission changed so leading blank lines no longer stack \
         anchors, pick another source that does stack them (NOT a heading \
         longer than 100 columns — those anchors are placed inside the <h1> \
         now) rather than weakening this assertion — the point is to cover \
         leading runs of arbitrary length.",
        describe(&children)
    );

    let css = style_block(&html).to_string();
    let (winner, unsupported) = winning_margin(&css, &children, idx, true);

    let (selector, value) = winner.unwrap_or_else(|| {
        panic!(
            "NO rule sets a `margin-top` on the document's opening <h1> — {}. \
             Unevaluated selectors: {unsupported:?}",
            anchor_note(&children, idx, true)
        )
    });
    assert_eq!(
        px(&value),
        Some(0),
        "the opening <h1>'s winning `margin-top` is `{value}` (from `{selector}`), \
         but it MUST resolve to 0 — {}.\n\n\
         This is the SAME 48px displacement as \
         `first_child_margin_reset_reaches_the_opening_heading`, re-exposed by a \
         LONGER leading anchor run. A NARROW selector of the form\n\n    \
         body.wolf-markdown > .wolf-src:first-child + *\n\n\
         reaches the block after exactly ONE anchor and stops there, which is \
         why `base.css` does not use it. The general form it uses instead has to \
         negate over everything before the block, because CSS has no \
         preceding-sibling combinator and `:has()` only looks forward:\n\n    \
         body.wolf-markdown > *:not(.wolf-src):not(*:not(.wolf-src) ~ *)\n\n\
         — \"not an anchor, and no non-anchor sibling precedes it\". Any shape \
         with that meaning passes; so would no longer emitting leading anchors. \
         Do NOT touch the `.wolf-src` rule.\n\n\
         Unevaluated selectors: {unsupported:?}",
        anchor_note(&children, idx, true)
    );
}

#[test]
fn last_child_margin_reset_reaches_the_closing_block() {
    // A TRAILING NEWLINE is the common case, and it is what puts a trailing
    // `wolf-src` anchor after the final block: the last hard line's marker has
    // nowhere left to attach, so `flush_before(usize::MAX)` emits it after the
    // closing tag.
    let html = render_html(
        "# Title\n\nLast paragraph.\n",
        PreviewTheme::Default,
        WidthMode::Full,
    );
    let children = body_children(&html);
    assert!(
        !children.is_empty(),
        "the rendered body must have element children: {html}"
    );

    let idx = children
        .iter()
        .rposition(|c| !c.is_anchor())
        .unwrap_or_else(|| panic!("the body must contain a content block: {html}"));
    assert_eq!(
        children[idx].tag,
        "p",
        "fixture sanity: the last content block must be the <p>, found [{}]",
        describe(&children)
    );

    let css = style_block(&html).to_string();
    let (winner, unsupported) = winning_margin(&css, &children, idx, false);

    let (selector, value) = winner.unwrap_or_else(|| {
        panic!(
            "NO rule sets a `margin-bottom` on the document's final block — {}. \
             Unevaluated selectors: {unsupported:?}",
            anchor_note(&children, idx, false)
        )
    });
    assert_eq!(
        px(&value),
        Some(0),
        "the final block's winning `margin-bottom` is `{value}` (from \
         `{selector}`), but it MUST resolve to 0 — {}.\n\n\
         This reset exists so the document does not end with a stray baseline \
         of margin below the last line of content, at the pane's bottom edge. \
         The rule that CARRIES \
         that invariant for the common case — a source ending in a newline, as \
         this fixture does — is the general form:\n\n    \
         body.wolf-markdown > *:not(.wolf-src):not(:has(~ *:not(.wolf-src)))\n\n\
         — \"not an anchor, and no non-anchor sibling follows it\". It has to \
         negate over everything after the block because the bare structural \
         spelling is INERT here: `:last-child` matches the last ELEMENT child, \
         which is the zero-size scroll-sync anchor that \
         `flush_before(usize::MAX)` emits after the closing tag, and that \
         anchor has no margin to zero — so the real last block keeps its \
         margin.\n\n\
         `base.css` DOES also ship the naive `body.wolf-markdown > *:last-child` \
         ahead of the general rule, but only as a partial FALLBACK: it is \
         correct just for sources that do not end in a newline, where no \
         trailing anchor exists. It is not the rule under test here.\n\n\
         Any selector shape meaning \"last non-anchor child\" passes. Do NOT \
         change the `.wolf-src` rule (its `font-size: 0` / `line-height: 0` is \
         load-bearing) and do NOT change marker emission.\n\n\
         Unevaluated selectors (extend this test's matcher if the fix is \
         spelled with syntax it does not model): {unsupported:?}",
        anchor_note(&children, idx, false)
    );
}

#[test]
fn scroll_sync_anchors_stay_zero_sized() {
    // The guard that makes the two tests above safe to fix. The cheapest-looking
    // way to stop the anchors from displacing `:first-child` is to change the
    // `.wolf-src` rule — and it does not even work (`:first-child` is structural,
    // so `display: none` would not help), while `font-size: 0` / `line-height: 0`
    // are load-bearing: the body sets `line-height` as a LENGTH, so without the
    // explicit zero every marker would inherit a hard 24px line box and inject a
    // visible blank line before every block in the document.
    let css = rendered_stylesheet();
    let decls = rule_decls(&css, ".wolf-src");

    assert_eq!(
        decls.get("font-size").and_then(|v| px(v)),
        Some(0),
        "`.wolf-src` must keep `font-size: 0`: {decls:?}"
    );
    assert_eq!(
        decls.get("line-height").and_then(|v| px(v)),
        Some(0),
        "`.wolf-src` must keep `line-height: 0`. The body declares its \
         line-height as a LENGTH ({BASELINE}px), which inherits as a COMPUTED \
         length — so a marker span without its own explicit 0 would get a full \
         {BASELINE}px line box and inject a visible blank line before every \
         block: {decls:?}"
    );
    assert_eq!(
        decls.get("height").and_then(|v| px(v)),
        Some(0),
        "`.wolf-src` must keep `height: 0`: {decls:?}"
    );
}

// ---------------------------------------------------------------------------
// Inline-level anchoring for the per-100-column markers.
//
// Reported symptom: "if a single paragraph is very long and a user is typing at
// the end, it is the beginning of the paragraph that is in view, not the end."
// The per-100-column markers used to be flushed before the block's opening tag
// along with the per-line ones, so every one of them measured to the paragraph's
// top and cursor-follow always revealed the paragraph's start.
//
// A block's interior `col != 0` markers are now DEFERRED past the opening tag and
// placed at their real position in the rendered inline flow. Column-zero markers
// keep their block-top placement (the front-end's tier-4 sweep filters on
// `[data-col="0"]` and depends on their document order), and the placement is
// suppressed inside code blocks, HTML blocks, tables and image alt text.
//
// These tests use the PUBLIC surface only, like the rest of this file.
// ---------------------------------------------------------------------------

/// Number of `"lorem ipsum dolor "` (18-character) repeats that make the
/// regression fixture's paragraph: 30 × 18 = 540 characters on ONE hard line, so
/// it earns interior anchors at columns 100, 200, 300, 400 and 500.
const REGRESSION_PARAGRAPH_REPEATS: usize = 30;

/// The `id="wl-…"` values in the document, in document order.
///
/// A crude scanner, legitimate for the same reason [`body_children`]'s is: the
/// anchor markup is a fixed shape emitted by this crate.
///
/// LOAD-BEARING ASSUMPTION: it scans the WHOLE document, so it relies on the
/// `<style>` block containing no `id="`. That is true today but is not enforced
/// anywhere — an ID selector written as an attribute selector
/// (`[id="wl-1-0"] { … }`) in `base.css` or a theme would be picked up as a
/// marker id and this helper would report anchors that do not exist. The
/// `embedded_stylesheets_carry_no_markup_like_text` guard in `markdown.rs` does
/// NOT cover this needle — it only rejects `<`-prefixed sequences. If a
/// stylesheet ever needs such a selector, restrict this scan to the body first.
fn marker_ids_in_document_order(html: &str) -> Vec<String> {
    const NEEDLE: &str = "id=\"wl-";
    let mut ids = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find(NEEDLE) {
        let after = &rest[at + "id=\"".len()..];
        let end = after
            .find('"')
            .unwrap_or_else(|| panic!("an anchor id must be closed by a quote: {after}"));
        ids.push(after[..end].to_string());
        rest = &after[end..];
    }
    ids
}

/// The `data-line` of every `data-col="0"` anchor, in document order.
///
/// LOAD-BEARING ASSUMPTION: like [`marker_ids_in_document_order`], this scans the
/// WHOLE document and relies on the `<style>` block containing no `data-line="`.
/// That is a REAL risk rather than a theoretical one: the front-end already
/// selects on `.wolf-src[data-col="0"]`, so mirroring that selector into
/// `base.css` is a plausible future edit, and a `[data-line="…"]` rule alongside
/// it would be parsed here as an anchor. The
/// `embedded_stylesheets_carry_no_markup_like_text` guard in `markdown.rs` does
/// NOT cover this needle. If such a selector is ever added, restrict this scan to
/// the body first.
fn column_zero_marker_lines(html: &str) -> Vec<u32> {
    const NEEDLE: &str = "data-line=\"";
    let mut lines = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find(NEEDLE) {
        let after = &rest[at + NEEDLE.len()..];
        let end = after
            .find('"')
            .unwrap_or_else(|| panic!("a data-line attribute must be closed: {after}"));
        let line: u32 = after[..end]
            .parse()
            .unwrap_or_else(|_| panic!("a data-line must be numeric: {}", &after[..end]));
        let tail = &after[end + 1..];
        if tail.starts_with(" data-col=\"0\"") {
            lines.push(line);
        }
        rest = tail;
    }
    lines
}

/// The marker ids the SCHEME predicts for `source`, recomputed here from the
/// documented rule rather than read out of the implementation: `wl-{L}-0` for
/// every hard line, plus `wl-{L}-{k*100}` for every multiple of
/// [`COLUMN_STRIDE`] strictly less than the line's CHARACTER length.
fn expected_marker_ids(source: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for (idx, line) in source.split('\n').enumerate() {
        let line_no = idx as u32 + 1;
        ids.push(format!("wl-{line_no}-0"));

        let char_len = line.chars().count() as u32;
        let mut col = COLUMN_STRIDE;
        while col < char_len {
            ids.push(format!("wl-{line_no}-{col}"));
            col += COLUMN_STRIDE;
        }
    }
    ids
}

/// A document exercising every placement path at once: a heading, a very long
/// single-line paragraph, a list with a long item, a table with a long cell, a
/// fenced code block with a long line, an image with long alt text, and a
/// trailing newline (which leaves a final empty hard line).
fn mixed_marker_fixture() -> String {
    let long_para = "lorem ipsum dolor ".repeat(REGRESSION_PARAGRAPH_REPEATS);
    let long_item = "list word ".repeat(15);
    let long_cell = "w".repeat(130);
    let long_code = "z".repeat(150);
    let long_alt = "alt word ".repeat(15);

    let mut source = String::new();
    source.push_str("# Title\n\n");
    source.push_str(&long_para);
    source.push_str("\n\n- item one\n- ");
    source.push_str(&long_item);
    source.push_str("\n\n| a | b |\n| --- | --- |\n| ");
    source.push_str(&long_cell);
    source.push_str(" | y |\n\n```text\n");
    source.push_str(&long_code);
    source.push_str("\n```\n\n![");
    source.push_str(&long_alt);
    source.push_str("](pic.png)\n");
    source
}

#[test]
fn long_single_line_paragraph_anchors_inside_the_paragraph() {
    // THE regression test for the reported bug. Hard line 3 is one 540-character
    // paragraph (`"lorem ipsum dolor "` is 18 characters). Its `wl-3-0` anchor
    // keeps the block-top placement, but `wl-3-100`, `wl-3-200` and `wl-3-300`
    // must now sit INSIDE the <p>, in source order, so a cursor deep in the line
    // resolves to a rect deep in the rendered paragraph.
    let paragraph = "lorem ipsum dolor ".repeat(REGRESSION_PARAGRAPH_REPEATS);
    assert!(
        paragraph.chars().count() > 3 * COLUMN_STRIDE as usize,
        "fixture sanity: the paragraph must clear three strides"
    );
    let source = format!("# Title\n\n{paragraph}");

    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    let p_open = html
        .find("<p>")
        .unwrap_or_else(|| panic!("the long line must render one <p>: {html}"));
    let p_close = html
        .find("</p>")
        .unwrap_or_else(|| panic!("the <p> must be closed: {html}"));

    let zero = html
        .find(&marker_id(3, 0))
        .unwrap_or_else(|| panic!("missing the `wl-3-0` anchor: {html}"));
    assert!(
        zero < p_open,
        "the column-ZERO anchor must keep its block-top placement: `wl-3-0` at \
         {zero} must precede the <p> at {p_open}: {html}"
    );

    let mut previous = p_open;
    for stride in 1..=3u32 {
        let col = stride * COLUMN_STRIDE;
        let idx = html
            .find(&marker_id(3, col))
            .unwrap_or_else(|| panic!("missing the `wl-3-{col}` anchor: {html}"));
        assert!(
            previous < idx && idx < p_close,
            "REGRESSION: the `wl-3-{col}` anchor (at {idx}) must sit INSIDE the \
             paragraph's inline content, after the previous anchor (at {previous}) \
             and before the </p> (at {p_close}).\n\n\
             When the per-100-column anchors all collapse to the paragraph's top \
             instead, the preview reveals the START of the paragraph while the \
             user is typing at its END — the exact reported symptom: `#wl-3-{col}` \
             resolves, but its rect is the paragraph's top, so cursor-follow \
             scrolls the wrong wrapped visual line into view.\n\n{html}"
        );
        previous = idx;
    }
}

#[test]
fn inline_marker_never_lands_inside_a_fenced_code_block() {
    // Hard line 4 is a 200-character code line, so it earns a `wl-4-100` marker
    // whose byte offset is inside the fence. Inside a fenced block a single Text
    // event can span a whole line INCLUDING its newline, so without the exclusion
    // that marker would split the rendered code.
    let code_line = "x".repeat(2 * COLUMN_STRIDE as usize);
    let source = format!("# H\n\n```\n{code_line}\ncode two\n```\n");

    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    let pre_start = html
        .find("<pre")
        .unwrap_or_else(|| panic!("missing <pre> for the fenced code block: {html}"));
    let pre_end = html
        .find("</pre>")
        .unwrap_or_else(|| panic!("missing </pre>: {html}"));
    let inside = &html[pre_start..pre_end];
    assert!(
        !inside.contains("wolf-src"),
        "no `wolf-src` anchor may be split into <pre><code>…: {inside}"
    );
    assert!(
        !inside.contains("wl-"),
        "no `wl-` anchor id may appear inside <pre><code>…: {inside}"
    );
    assert!(
        html.contains(&marker_id(4, COLUMN_STRIDE)),
        "the code line's `wl-4-100` anchor must still EXIST outside the <pre> \
         (excluded from the block, never dropped): {html}"
    );
}

#[test]
fn inline_marker_never_lands_inside_a_highlighted_code_block() {
    // The highlighted path: `CodeHighlighter` rewrites the block interior into one
    // inline-HTML event but keeps the real `Start`/`End(CodeBlock)`, so the
    // exclusion must still bracket the block — and the `<pre><code
    // class="language-…">` wrapper must be untouched.
    const HIGHLIGHT_END_COL: u32 = 10;
    let code_line = "x".repeat(2 * COLUMN_STRIDE as usize);
    let source = format!("# H\n\n```rust\n{code_line}\ncode two\n```\n");
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

    let pre_start = html
        .find("<pre")
        .unwrap_or_else(|| panic!("missing <pre> for the highlighted block: {html}"));
    let pre_end = html
        .find("</pre>")
        .unwrap_or_else(|| panic!("missing </pre>: {html}"));
    let inside = &html[pre_start..pre_end];
    assert!(
        !inside.contains("wolf-src"),
        "no `wolf-src` anchor may land inside a HIGHLIGHTED <pre><code>: {inside}"
    );
    assert!(
        !inside.contains("wl-"),
        "no `wl-` anchor id may appear inside a HIGHLIGHTED <pre><code>: {inside}"
    );
    assert!(
        html.contains(r#"class="language-rust""#),
        "the highlighted block must keep its `class=\"language-rust\"`: {html}"
    );
    assert!(
        html.contains(&marker_id(4, COLUMN_STRIDE)),
        "the code line's `wl-4-100` anchor must still EXIST outside the <pre>: {html}"
    );
}

#[test]
fn image_alt_attribute_never_receives_a_marker() {
    // `"alt word "` is 9 characters, so the alt text spans source bytes 2..182 and
    // character column 100 falls well inside it. pulldown-cmark builds the `alt`
    // attribute by consuming the image's inner events: it ESCAPES text into the
    // attribute and DROPS raw HTML outright — so a marker leaking in would either
    // corrupt `alt="…"` or vanish from the document entirely.
    let source = format!("![{}](i.png)", "alt word ".repeat(20));
    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    let img_start = html
        .find("<img")
        .unwrap_or_else(|| panic!("missing the <img> element: {html}"));
    let img_len = html[img_start..]
        .find("/>")
        .unwrap_or_else(|| panic!("the <img> element must be closed with `/>`: {html}"));
    let img_tag = &html[img_start..img_start + img_len + "/>".len()];
    assert!(
        !img_tag.contains("wolf-src"),
        "no `wolf-src` anchor may be escaped into the image's alt attribute: {img_tag}"
    );
    assert!(
        !img_tag.contains("wl-"),
        "no `wl-` anchor id may appear inside the <img> tag: {img_tag}"
    );
    assert!(
        html.contains(&marker_id(1, COLUMN_STRIDE)),
        "the `wl-1-100` anchor must still exist in the document — if it is missing \
         it leaked into the image and was silently dropped while the alt attribute \
         was built: {html}"
    );
}

#[test]
fn escapes_and_entities_survive_inline_anchoring() {
    // 96 filler characters put `&amp;` at bytes 96..101, so character column 100
    // (the `;`) falls STRICTLY inside the entity's source span — a region whose
    // rendered payload (`&`) is unrelated to its source bytes, so it must never be
    // split. `\*escaped\*` follows, where the backslash bytes are covered by no
    // event at all. Total line length is 174 characters: past one stride, short of
    // two.
    let source = format!("{}&amp; \\*escaped\\* {}", "a".repeat(96), "a".repeat(60));
    assert!(
        source.chars().count() > COLUMN_STRIDE as usize
            && source.chars().count() < 2 * COLUMN_STRIDE as usize,
        "fixture sanity: exactly one interior anchor (`wl-1-100`) is expected"
    );

    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    assert!(
        html.contains("*escaped*"),
        "the escaped `\\*escaped\\*` must render as the literal `*escaped*`, \
         uninterrupted by an anchor span: {html}"
    );
    assert!(
        !html.contains("<em>"),
        "the backslash escapes must still suppress emphasis (no <em>): {html}"
    );
    assert_eq!(
        html.matches("&amp;").count(),
        1,
        "the source's single `&amp;` must render exactly once, un-split: {html}"
    );
    assert!(
        !html.contains("&amp;amp;"),
        "the entity must not be double-escaped: {html}"
    );
    assert!(
        html.contains(&marker_id(1, COLUMN_STRIDE)),
        "a marker that cannot be placed by splitting must still be emitted \
         somewhere (never dropped): {html}"
    );
}

#[test]
fn column_zero_markers_still_collapse_to_the_block_top() {
    // Pins BOTH halves of the placement rule at once, on one paragraph made of
    // three hard lines whose first line is 250 characters:
    //
    //   * `wl-1-0`, `wl-2-0` and `wl-3-0` all still collapse to before the <p>;
    //   * `wl-1-100` and `wl-1-200` sit inside it.
    //
    // That also pins the DOCUMENTED ordering exception: in document order the
    // later lines' column-zero anchors precede line 1's interior anchors, so
    // markers are no longer globally ascending. That is only safe because the
    // front-end's fallback sweep filters on `[data-col="0"]`, and the col-0
    // subsequence IS still ascending (see
    // `column_zero_markers_appear_in_ascending_document_order`).
    let long_first_line = "a".repeat(250);
    let source = format!("{long_first_line}\nsecond line\nthird line\n");

    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    assert_eq!(
        html.matches("<p>").count(),
        1,
        "the three hard lines must render as exactly one <p>: {html}"
    );
    let p_open = html.find("<p>").expect("the paragraph must render a <p>");
    let p_close = html
        .find("</p>")
        .unwrap_or_else(|| panic!("the <p> must be closed: {html}"));

    for line in 1..=3u32 {
        let idx = html
            .find(&marker_id(line, 0))
            .unwrap_or_else(|| panic!("missing the `wl-{line}-0` anchor: {html}"));
        assert!(
            idx < p_open,
            "`wl-{line}-0` (at {idx}) must still collapse to before the <p> (at \
             {p_open}) — the tier-4 fallback sweep depends on it: {html}"
        );
    }

    for stride in 1..=2u32 {
        let col = stride * COLUMN_STRIDE;
        let idx = html
            .find(&marker_id(1, col))
            .unwrap_or_else(|| panic!("missing the `wl-1-{col}` anchor: {html}"));
        assert!(
            p_open < idx && idx < p_close,
            "`wl-1-{col}` (at {idx}) must sit inside the <p> ({p_open}..{p_close}): {html}"
        );
    }
}

#[test]
fn every_marker_id_appears_exactly_once() {
    // The "none emitted twice, none dropped" guard, over a fixture that drives
    // every placement path (inline split, deferral past an un-splittable region,
    // block-top flush for code/table/image, and the trailing flush).
    let source = mixed_marker_fixture();
    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    let found = marker_ids_in_document_order(&html);
    let unique: HashSet<&String> = found.iter().collect();
    assert_eq!(
        found.len(),
        unique.len(),
        "every `wl-…` anchor id must be emitted EXACTLY once; the document order \
         was {found:?}"
    );

    let expected = expected_marker_ids(&source);
    let expected_set: HashSet<&String> = expected.iter().collect();
    let missing: Vec<&&String> = expected_set.difference(&unique).collect();
    assert!(
        missing.is_empty(),
        "every marker the scheme predicts must exist in the document; missing \
         {missing:?} (deferring a marker must never drop it): {html}"
    );
    let extra: Vec<&&String> = unique.difference(&expected_set).collect();
    assert!(
        extra.is_empty(),
        "the document must not carry anchors the scheme does not predict; extra \
         {extra:?}: {html}"
    );
}

#[test]
fn column_zero_markers_appear_in_ascending_document_order() {
    // The exact invariant the front-end's tier-4 fallback sweep depends on: it
    // walks `.wolf-src[data-col="0"]` BACKWARDS from the end of the document to
    // find the nearest preceding line, which is only correct while the col-0
    // anchors' `data-line` values are non-decreasing in document order.
    //
    // Interior (`col != 0`) anchors deliberately break the GLOBAL ordering — see
    // `column_zero_markers_still_collapse_to_the_block_top` — so this subsequence
    // is what has to hold, and the sweep must never be widened to all `.wolf-src`.
    let source = mixed_marker_fixture();
    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    let lines = column_zero_marker_lines(&html);
    assert!(
        !lines.is_empty(),
        "fixture sanity: the document must carry column-zero anchors: {html}"
    );
    for pair in lines.windows(2) {
        assert!(
            pair[0] <= pair[1],
            "the `data-col=\"0\"` anchors must stay in non-decreasing `data-line` \
             order in the document: found {} before {} in {lines:?}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn inline_marker_lands_inside_its_own_list_item() {
    // A container claims ALL of its items' markers at its own `Start`, so a
    // multi-item list is the case where two siblings' deferred markers coexist in
    // the queue. Hard line 1 is the short item; hard line 2 is `- ` + 240
    // characters = 242 characters, so it earns `wl-2-100` and `wl-2-200` (300 is
    // not < 242), whose byte offsets (112 and 212) both fall inside the SECOND
    // item's text.
    //
    // The list is TIGHT (no blank line between the items), so pulldown-cmark
    // splices the item paragraphs away and renders `<ul>\n<li>…</li>\n<li>…</li>\n
    // </ul>` with no `<p>` wrappers — these assertions do not depend on that
    // either way, they only key off the `</li>` positions.
    let long = "lorem ipsum ".repeat(20);
    let source = format!("- short one\n- {long}\n");

    let second_line = source
        .split('\n')
        .nth(1)
        .expect("fixture sanity: the source has a second hard line");
    assert!(
        second_line.chars().count() > 2 * COLUMN_STRIDE as usize
            && second_line.chars().count() < 3 * COLUMN_STRIDE as usize,
        "fixture sanity: the long item must earn exactly `wl-2-100` and `wl-2-200`, \
         but hard line 2 is {} characters",
        second_line.chars().count()
    );

    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    let ul_open = html
        .find("<ul>")
        .unwrap_or_else(|| panic!("the two items must render one <ul>: {html}"));
    let ul_close = html
        .find("</ul>")
        .unwrap_or_else(|| panic!("the <ul> must be closed: {html}"));
    let first_li_close = html
        .find("</li>")
        .unwrap_or_else(|| panic!("missing the first item's </li>: {html}"));
    let last_li_close = html
        .rfind("</li>")
        .unwrap_or_else(|| panic!("missing the second item's </li>: {html}"));
    assert!(
        first_li_close < last_li_close,
        "fixture sanity: the list must render TWO <li> elements: {html}"
    );

    // 1. Column-zero still collapses to the container's top (unchanged).
    let zero = html
        .find(&marker_id(2, 0))
        .unwrap_or_else(|| panic!("missing the `wl-2-0` anchor: {html}"));
    assert!(
        zero < ul_open,
        "the column-ZERO anchor must still collapse to the container top: `wl-2-0` \
         at {zero} must precede the <ul> at {ul_open}: {html}"
    );

    // 2. The interior anchor belongs to the SECOND item, not the first.
    let interior = html
        .find(&marker_id(2, COLUMN_STRIDE))
        .unwrap_or_else(|| panic!("missing the `wl-2-100` anchor: {html}"));
    assert!(
        first_li_close < interior && interior < last_li_close,
        "the `wl-2-100` anchor (at {interior}) must sit inside the SECOND <li> — \
         after the first item's </li> (at {first_li_close}) and before the last \
         </li> (at {last_li_close}).\n\n\
         When it lands earlier, the bug is: a deferred column marker drained at \
         the *first* `End(Item)` lands in the wrong `<li>`; the drain at a block \
         `End` must be bounded by that End's own `range.end`. A container claims \
         every item's markers at its own `Start`, so an UNBOUNDED drain at the \
         first item's `End` flushes item two's marker into item one — and the \
         preview then scrolls to the wrong list item.\n\n{html}"
    );

    // 3. And it never escapes the list.
    assert!(
        interior < ul_close,
        "the `wl-2-100` anchor (at {interior}) must not escape the list (</ul> at \
         {ul_close}): {html}"
    );
}

// ---------------------------------------------------------------------------
// NESTED excluded blocks.
//
// Every other code-block / table test in this file uses a TOP-LEVEL block, where
// nothing is ever deferred — so they cannot see this failure mode at all:
//
//   A CONTAINER (`List` / `Item` / `BlockQuote` / `FootnoteDefinition`) defers
//   every `col != 0` marker across its WHOLE range at its own `Start`, including
//   markers whose byte offsets fall inside a `CodeBlock` / `HtmlBlock` / `Table`
//   NESTED in that container. By the time the nested block's `Start` arrives
//   those markers are already deferred — past the marker cursor — so the nested
//   block's own non-deferrable claim cannot reclaim them. They then survive to
//   the nested block's `End`, where the suppression depth has already dropped
//   back to 0, and get drained INSIDE `<pre><code>` / `<table>`.
//
// The markers of a nested excluded block must instead be flushed ahead of that
// block's opening tag, exactly as a top-level one's are.
// ---------------------------------------------------------------------------

/// The slice of `html` from the first `open` to the first `close`, with `html` in
/// the panic message when either is missing. Crude on purpose — the fixtures
/// below emit exactly one code block / table each.
fn region_between<'a>(html: &'a str, open: &str, close: &str) -> &'a str {
    let start = html
        .find(open)
        .unwrap_or_else(|| panic!("missing `{open}` in: {html}"));
    let end = html
        .find(close)
        .unwrap_or_else(|| panic!("missing `{close}` in: {html}"));
    assert!(
        start < end,
        "`{open}` (at {start}) must precede `{close}` (at {end}): {html}"
    );
    &html[start..end]
}

/// The failure message shared by the three nested-exclusion tests: names the bug
/// and the required placement. `container` / `nested` describe the fixture.
fn nested_exclusion_failure(container: &str, nested: &str, region: &str, html: &str) -> String {
    format!(
        "an anchor span landed inside the {nested} nested in a {container}.\n\n\
         A container defers markers across its WHOLE range, including ones whose \
         offsets fall inside a nested excluded block; those must be flushed AHEAD \
         of the nested block's opening tag, not drained at its `End` (by which \
         point the suppression depth is already back to 0). Every other \
         code-block/table test uses a TOP-LEVEL block, where nothing is ever \
         deferred, so only this test can see the bug.\n\n\
         Region: {region}\n\nDocument: {html}"
    )
}

#[test]
fn nested_code_block_in_a_blockquote_takes_no_inline_marker() {
    // Hard line 2 is `> ` + 200 characters = 202 characters, so it earns `wl-2-100`
    // and `wl-2-200` at byte offsets 106 and 206 — both inside the fenced block's
    // source range, and both deferred by the enclosing blockquote.
    let long = "x".repeat(2 * COLUMN_STRIDE as usize);
    let source = format!("> ```\n> {long}\n> ```\n");

    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    let inside = region_between(&html, "<pre", "</pre>");
    assert!(
        !inside.contains("wolf-src"),
        "{}",
        nested_exclusion_failure("blockquote", "fenced code block", inside, &html)
    );
    assert!(
        !inside.contains("wl-"),
        "{}",
        nested_exclusion_failure("blockquote", "fenced code block", inside, &html)
    );

    for stride in 1..=2u32 {
        let col = stride * COLUMN_STRIDE;
        assert!(
            html.contains(&marker_id(2, col)),
            "the code line's `wl-2-{col}` anchor must still EXIST outside the <pre> \
             — excluded from the block, never dropped: {html}"
        );
    }
}

#[test]
fn nested_code_block_in_a_list_item_takes_no_inline_marker() {
    // The same hole reached through `List`/`Item` rather than `BlockQuote`, and the
    // shape this repo's own Markdown actually uses. Hard line 2 is two spaces of
    // continuation indent plus 200 characters = 202 characters, so `wl-2-100` and
    // `wl-2-200` sit at byte offsets 106 and 206, inside the fence.
    let long = "x".repeat(2 * COLUMN_STRIDE as usize);
    let source = format!("- ```\n  {long}\n  ```\n");

    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    let li_open = html
        .find("<li>")
        .unwrap_or_else(|| panic!("fixture sanity: the source must render a <li>: {html}"));
    let inside = region_between(&html, "<pre", "</pre>");
    let pre_start = html
        .find("<pre")
        .unwrap_or_else(|| panic!("missing <pre>: {html}"));
    assert!(
        li_open < pre_start,
        "fixture sanity: the fenced block must be NESTED inside the list item \
         (<li> at {li_open}, <pre> at {pre_start}) — if it is not, the test is not \
         exercising the deferred path at all: {html}"
    );

    assert!(
        !inside.contains("wolf-src"),
        "{}",
        nested_exclusion_failure("list item", "fenced code block", inside, &html)
    );
    assert!(
        !inside.contains("wl-"),
        "{}",
        nested_exclusion_failure("list item", "fenced code block", inside, &html)
    );

    for stride in 1..=2u32 {
        let col = stride * COLUMN_STRIDE;
        assert!(
            html.contains(&marker_id(2, col)),
            "the code line's `wl-2-{col}` anchor must still EXIST outside the <pre>: {html}"
        );
    }
}

#[test]
fn nested_table_in_a_list_item_takes_no_inline_marker() {
    // The foster-parenting variant: a raw span emitted between `<tbody>` and `<tr>`
    // is relocated OUT of the table by the HTML parser, so it must never get there.
    // Hard line 3 is `  | ` + 130 + ` | y |` = 140 characters, so it earns
    // `wl-3-100` at byte offset 128 — inside the table, deferred by the list.
    let long_cell = "w".repeat(130);
    let source = format!("- | a | b |\n  | --- | --- |\n  | {long_cell} | y |\n");

    let html = render_html(&source, PreviewTheme::Default, WidthMode::Full);

    let li_open = html.find("<li>").unwrap_or_else(|| {
        panic!(
            "fixture sanity: the source must render a <li>. If pulldown-cmark stopped \
             parsing this indented table inside the list item, FIX THE FIXTURE (a \
             blockquote-wrapped table reaches the same code path) rather than \
             deleting the test: {html}"
        )
    });
    let table_start = html.find("<table").unwrap_or_else(|| {
        panic!(
            "fixture sanity: the source must render a <table> nested in the list \
             item. If pulldown-cmark stopped parsing it, FIX THE FIXTURE (a \
             blockquote-wrapped table reaches the same code path) rather than \
             deleting the test: {html}"
        )
    });
    assert!(
        li_open < table_start,
        "fixture sanity: the table must be NESTED inside the list item (<li> at \
         {li_open}, <table> at {table_start}): {html}"
    );

    let inside = region_between(&html, "<table", "</table>");
    assert!(
        !inside.contains("wolf-src"),
        "{}",
        nested_exclusion_failure("list item", "table", inside, &html)
    );
    assert!(
        !inside.contains("wl-"),
        "{}",
        nested_exclusion_failure("list item", "table", inside, &html)
    );

    assert!(
        html.contains(&marker_id(3, COLUMN_STRIDE)),
        "the long cell's `wl-3-100` anchor must still EXIST outside the <table> \
         — excluded from the block, never dropped: {html}"
    );
}
