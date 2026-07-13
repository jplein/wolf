# wolf-core/src/markdown

The CSS and font assets that [`crate::markdown`](../markdown.rs) `include_str!`s into the preview HTML document it renders. None of the files in this directory (or its `assets/` subdirectory) are Markdown docs, so they are NOT prettier-formatted. `render_html` assembles the document `<style>` block from them in the fixed order `{INTER_FONT_FACE}{MATH_FONT_FACE}{base.css}{math.css}{theme.css}` (see [`../markdown.rs`](../markdown.rs) and [`../README.md`](../README.md)).

## `base.css`

The structural + marker CSS shared by every preview theme, embedded BEFORE the selected theme's CSS. It owns two things:

- **The `.wolf-src` marker rule** — a zero-size, invisible, non-selectable inline anchor (`width: 0; height: 0; overflow: hidden; visibility: hidden; pointer-events: none`). Each source-position marker span the renderer injects carries this class; the rule guarantees the markers take no layout space and never affect the rendered body. See the marker/anchor scheme in [`../README.md`](../README.md).
- **Baseline layout** — margins, headings, code/table/blockquote framing, list padding, `scroll-margin-top: 2rem` on `h1..h6` (so an anchor-scrolled heading clears the top edge), and the `body.wolf-markdown` font stack. That stack is now **Inter-fronted**: `font-family: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif` — `"Inter"` (supplied by the inlined `INTER_FONT_FACE`) first, with the system fallback retained behind it. A theme layers colors on top; `base.css` carries structure only.

## `themes/default.css`

The one shipped preview theme (`PreviewTheme::Default`), embedded AFTER `base.css`. Its CSS declares `color-scheme: light dark;` and carries a dark default palette plus a `@media (prefers-color-scheme: light)` block (light background `#f4f4f4`), so the preview follows the app's resolved dark/light appearance automatically (the web view inherits `NSApp.effectiveAppearance`) with no FFI change — while still NOT being wired to the live nvim colorscheme. The `PreviewTheme` enum + `theme_css` + this `include_str!`'d file are structured so more themes and a config-driven selector can drop in later; for now selection is a hardcoded `PreviewTheme::Default`.

## `math.css`

The MathML styling for pulldown-latex's `<math>` output, embedded (via the `MATH_CSS` const) AFTER `base.css`, alongside the inlined `@font-face` rules. It is **adapted from pulldown-latex's `styles.css`** (MIT, carloskiki/pulldown-latex) with three changes:

1. **All `@font-face` rules dropped.** Upstream's faces used relative `url('font/…')` paths that do not resolve in the inlined preview document; `markdown.rs` supplies the math font itself as an inlined base64 `data:` URI `@font-face` (`MATH_FONT_FACE`), and the upstream `mtext` font rule was dropped with the faces it referenced.
2. **The MathML `@namespace m` prefix removed** and every `m|…` selector rewritten to its plain element/class form (WebKit / WebKitGTK render the un-namespaced math DOM pulldown-latex emits).
3. **A final font pin appended** — `math, math * { font-family: "Latin Modern Math", math; }` — so all math content resolves to the bundled Latin Modern Math font, never a host-installed one.

(Its top comment also notes it must never contain the literal `<math` open-tag sequence, so a test scanning the document for the first such tag finds the rendered body, not this file's comment.)

## `assets/`

The vendored fonts, each committed three ways: the raw woff2 (provenance), a single-line base64 sidecar (`*.woff2.b64`, the file the code actually `include_str!`s), and the license text.

| File | Bytes | Upstream source | License |
| --- | --- | --- | --- |
| `inter-variable.woff2` | 352240 | rsms/inter v4.1 release asset `web/InterVariable.woff2` | SIL OFL 1.1 (`inter-OFL.txt`) |
| `latinmodern-math.woff2` | 387976 | pulldown-latex repo `font/latinmodern-math.woff2` (`raw.githubusercontent.com/carloskiki/pulldown-latex/main/font/latinmodern-math.woff2`) | GUST GFL v1.0 (`latinmodern-math-GUST-FONT-LICENSE.txt`) |

**How they reach the document.** `markdown.rs` builds the `INTER_FONT_FACE` and `MATH_FONT_FACE` consts at COMPILE time with `concat!` + `include_str!` of the `.b64` sidecars, wrapping each in a `@font-face { … src: url('data:font/woff2;base64,<b64>') format('woff2') }` rule — zero runtime base64 encoding, and no base64 crate dependency. Inter declares the full variable weight axis (`font-weight: 100 900`); Latin Modern Math is pinned to all `<math>` content by `math.css`.

**Single-line invariant.** Each `.woff2.b64` MUST stay a single line (produced with `base64 -i f.woff2 | tr -d '\n' > f.woff2.b64`). A newline inside the base64 would break the `data:` URI. Only the roman Inter variable face is bundled (no separate italic file), so italic body text is faux-italic; the lmroman text faces pulldown-latex ships were deliberately NOT vendored (Wolf bundles ONE math font).

**Bazel note.** Because the `.b64` sidecars are `include_str!`'d at compile time, `crates/wolf-core/BUILD.bazel`'s `compile_data` glob must include `src/markdown/assets/*.b64` (alongside `src/markdown/**/*.css`). The raw woff2 and license texts are provenance-only and are not needed by rustc. See [`../../../../build.md`](../../../../build.md).

See [`preview-math-and-fonts-decisions.md`](../../../../preview-math-and-fonts-decisions.md) for the decisions behind bundling and inlining these assets.
