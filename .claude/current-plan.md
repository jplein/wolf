# Plan: Markdown preview theme responds to app dark/light appearance

## Problem statement

The Markdown preview's `themes/default.css` is currently a FIXED dark theme. Make
it respond to the app's resolved dark/light appearance, and add light color
variants. The light-mode background must be `#f4f4f4` (matching the lighthouse
light background).

## Decided approach (no FFI plumbing)

The front-end WKWebView (macOS) / WebKitGTK (GNOME) inherits the app's resolved
`NSApp.effectiveAppearance` (set by `AppDelegate.applyResolvedAppearance()` from
the config `auto`/`dark`/`light` mode). Therefore a CSS
`@media (prefers-color-scheme: light)` block makes the preview respond to the app
appearance automatically, live, with NO changes to `render_preview` signatures or
the FFI. This works for both front-ends.

## Scope

1. Rewrite `crates/wolf-core/src/markdown/themes/default.css`:
   - Keep the existing DARK values as the default (unconditional) rules.
   - Declare `color-scheme: light dark;` so the web view honors both schemes.
   - Add a `@media (prefers-color-scheme: light)` block: body background `#f4f4f4`,
     harmonious light palette (dark text ~#1f1f1f, near-black headings, link blue
     ~#0969da, code bg ~#e8e8e8, pre bg ~#eaeaea, blockquote text ~#656565,
     borders/hr ~#d0d0d0, th bg ~#eaeaea). Only color/background/border-color.
   - Update the file header comment (no longer fixed dark).

2. Docs:
   - `crates/wolf-core/src/markdown/README.md` (`themes/default.css` section).
   - `crates/wolf-core/src/markdown.rs` doc comments (default.css header, the
     `PreviewTheme` enum + `Default` variant docs). Keep the enum single-variant.

3. Tests: extend `crates/wolf-core/tests/markdown_render.rs` (or a unit test in
   `markdown.rs`) to assert the rendered document contains BOTH the dark bg
   (`#1b1b1b`) and a `prefers-color-scheme: light` block with the light bg
   (`#f4f4f4`), and that `color-scheme` is declared. Keep
   `theme_css_default_is_non_empty`.

## Constraints

- Documentation is source of truth; every source dir README must stay in sync.
- CSS files under `markdown/` are NOT prettier-formatted.
- Edited Markdown must be `prettier --write`'d, no hard line breaks within a paragraph.
- Prefer real integration tests exercising the public `render_html` API.
- Do NOT add FFI parameters or change `render_preview` signatures.

## Architect's detailed plan

### default.css rewrite
Keep all existing DARK rules unconditional; add `color-scheme: light dark;` to the
`body.wolf-markdown` block; rewrite header comment (no longer "fixed dark" — still
NOT wired to nvim colorscheme, but follows app/OS appearance via prefers-color-scheme).
Append `@media (prefers-color-scheme: light) {` (exact spacing) with 1:1 mirror of
the dark selector set:
- body: bg #f4f4f4, color #1f1f1f
- h1-h6: #111111
- h1,h2 border-bottom-color: #d0d0d0
- a: #0969da
- code: bg #e8e8e8, color #1f1f1f
- pre: bg #eaeaea
- blockquote: color #656565, border-left-color #d0d0d0
- th,td border-color: #d0d0d0
- th bg: #eaeaea
- hr bg: #d0d0d0
All light selectors keep the `body.wolf-markdown` prefix (specificity tie → media rule wins).

### markdown.rs doc edits (doc comments only, no code)
- PreviewTheme enum doc (~113-118): drop "a fixed dark theme"; describe dark + prefers-color-scheme:light palette following app appearance; KEEP "not wired to live nvim colorscheme (OD5)" and the "more themes later / hardcoded call site" clauses.
- PreviewTheme::Default variant doc (line 121): remove bare "(dark)"; note it ships dark + light variant.
- theme_css doc (126-128) generic — no change. Grep "fixed dark"/"dark theme" to confirm nothing else.

### markdown/README.md (themes/default.css section, ~12-14)
Rewrite: one shipped theme, embedded after base.css, declares `color-scheme: light dark;`,
dark default + `@media (prefers-color-scheme: light)` (light bg #f4f4f4), follows app resolved
appearance (web view inherits NSApp.effectiveAppearance) with no FFI change, still not wired to
nvim colorscheme. Keep "structured for more themes / config selector later, hardcoded Default now".
`prettier --write` after; no hard line breaks within a paragraph.

### Tests (markdown_render.rs)
Add `default_theme_carries_dark_and_light_palettes` near `theme_css_default_is_non_empty`.
Exercise `render_html("...", PreviewTheme::Default)`; assert html contains:
`#1b1b1b`, `prefers-color-scheme: light`, `#f4f4f4`, `color-scheme` (optionally `color-scheme: light dark`).
Include `{html}` diagnostic tail per file style. Keep `theme_css_default_is_non_empty`.

### Risk
No existing test pins style/color content — `render_html_wraps_body_in_a_themed_document` and
markdown.rs unit tests only check structural substrings. Load-bearing: exact authoring of
`@media (prefers-color-scheme: light)` and `color-scheme: light dark;` so new test substrings match.

## Implementation result

- `default.css` rewritten: dark rules kept unconditional + `color-scheme: light dark;`
  on body + `@media (prefers-color-scheme: light)` mirror block (bg #f4f4f4). Header
  comment rewritten (no longer "fixed dark").
- `markdown.rs` enum + variant doc comments updated (no "fixed dark"; keeps OD5 nvim
  distinction). No code changes.
- `markdown/README.md` themes/default.css section rewritten + prettier.
- Tests: `default_theme_carries_dark_and_light_palettes` added + strengthened
  (asserts #1b1b1b, prefers-color-scheme: light, #f4f4f4, color-scheme: light dark,
  #0969da, #111111). markdown_render 12/12 pass; wolf_core_lib_test 109/109 pass.

## Status: COMPLETE

- Reviewer: PASS, no issues (verified light block mirrors dark 1:1, base.css supplies
  structure for every colored selector, no code/signature changes).
- Technical writer: also corrected stale "fixed dark theme" phrasing in
  crates/wolf-core/README.md, crates/wolf-core/src/README.md, and architecture.md.
- prettier --write run on all edited Markdown (all unchanged/conformant).
- Tests: markdown_render 12/12, wolf_core_lib_test 109/109 pass.
