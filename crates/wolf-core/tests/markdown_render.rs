//! Integration test for `wolf-core`'s PUBLIC Markdown-preview render surface
//! (plan §"Core: Markdown→HTML rendering module" + the "Pure Rust unit" test
//! bullet). It exercises ONLY the re-exported public API — `wolf_core::render_html`,
//! `wolf_core::PreviewTheme`, and `wolf_core::theme_css` — NOT any private const
//! (`MARKER_COLUMN_STRIDE`, `desired_markers`, `MarkerInjector`, …). Behavior, not
//! implementation, is asserted: a `>= 100`-character line yields a per-100-column
//! marker `id="wl-{L}-100"`, a shorter line does not, every hard line yields a
//! `wl-{L}-0` anchor, and none of these markers land inside a fenced code block.
//!
//! ## Why a separate integration test file (not `markdown.rs`'s own `#[cfg(test)]`)
//!
//! `crates/wolf-core/src/markdown.rs` does not exist yet — the coder creates it,
//! together with its own in-module `#[cfg(test)] mod tests`. Testing through the
//! public surface here keeps file ownership clean (this file is the test-writer's
//! alone; the coder never edits it) and pins the re-exported contract from
//! `lib.rs`.
//!
//! ## Dependency on coder work
//!
//! None of `render_html`, `PreviewTheme`, or `theme_css` exists yet: the coder adds
//! the `markdown` module + re-exports in `crates/wolf-core/src/lib.rs`, AND (since
//! every `crates/wolf-core/tests/*.rs` file has a matching `rust_test` target) adds
//! a `//crates/wolf-core:markdown_render` `rust_test` target to
//! `crates/wolf-core/BUILD.bazel` (a copy of the `pane_status_unit` stanza — a pure,
//! nvim-free unit test, `srcs = ["tests/markdown_render.rs"]`,
//! `crate_root = "tests/markdown_render.rs"`). This test file does NOT touch source
//! or BUILD files and will not compile until the module lands — expected.
//!
//! This is a PURE render test: no Neovim, no filesystem, no wake — it needs no
//! `WOLF_NVIM_BIN` and never skips.

use wolf_core::{render_html, theme_css, PreviewTheme};

/// The per-line/per-100-column marker id substring for hard line `line` (1-based)
/// and 0-based character column `col`, per the plan's `id="wl-{L}-{C}"` anchor
/// scheme. Behavior is asserted via this id substring so the test never depends on
/// the private `MARKER_COLUMN_STRIDE`, `marker_span`, or attribute order.
// per plan: the anchor id format is `wl-{L}-{C}` with double-quoted `id="…"`.
fn marker_id(line: u32, col: u32) -> String {
    format!("id=\"wl-{line}-{col}\"")
}

/// The per-100-column stride the plan mandates (`MARKER_COLUMN_STRIDE = 100`),
/// re-declared LOCALLY as a behavioral constant (the source const is private). A
/// line of exactly this many characters is the first that earns a `-100` marker.
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
    let html = render_html("# Title\n\nHello", PreviewTheme::Default);

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
fn every_hard_line_yields_a_column_zero_marker() {
    // Three hard lines: a heading, a BLANK line, and a paragraph. Even the blank
    // line 2 must carry its own `wl-2-0` anchor (a marker per hard line, always).
    let source = "# Title\n\nHello";
    let line_count = source.split('\n').count() as u32;
    assert_eq!(line_count, 3, "sanity: this fixture has three hard lines");

    let html = render_html(source, PreviewTheme::Default);
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

    let html = render_html(source, PreviewTheme::Default);

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
    let html = render_html(source, PreviewTheme::Default);

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

    let html = render_html(&source, PreviewTheme::Default);

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
    let html = render_html(source, PreviewTheme::Default);

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
    let html = render_html(source, PreviewTheme::Default);

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

    let html = render_html(&source, PreviewTheme::Default);

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
    let html = render_html(source, PreviewTheme::Default);

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
    let html = render_html(source, PreviewTheme::Default);

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
    let html = render_html("# Title\n\ntext", PreviewTheme::Default);

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
