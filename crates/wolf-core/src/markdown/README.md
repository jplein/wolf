# wolf-core/src/markdown

The stylesheets and font payloads that [`markdown.rs`](../markdown.rs) compiles into every rendered preview document. Read this before changing how the preview looks; the module's own entry is in [`../README.md`](../README.md) and the crate-wide constraints are in [`wolf-core-decisions.md`](../../../../notes/wolf-core-decisions.md). Nothing in this directory is Markdown, so prettier formats only this README.

`render_html` emits a single `<style>` block assembled in a fixed order: the Inter `@font-face`, the math `@font-face`, `base.css`, `math.css`, the selected theme's CSS, and — for `WidthMode::Capped` — the horizontal width cap. Every part is `include_str!`'d at compile time, so a rendered document is self-contained and requests nothing from disk or network. The order is load-bearing: a theme is emitted after `base.css` and wins the cascade wherever both declare the same property.

## Files

#### `base.css`

The structural and marker CSS shared by every theme. It owns the `.wolf-src` marker rule, the structural layout (block margins, the heading scale, code, table and blockquote framing, and the Inter-fronted body font stack), and a 24px baseline grid whose arithmetic is specified in [`markdown-stylesheet.md`](../../../../specs/markdown-stylesheet.md) — the single source of truth for those numbers, deliberately not restated here or per declaration.

**Decisions**

- `.wolf-src` is an inline box clamped to zero width and height, with `font-size: 0` and `line-height: 0`. The zero leading is load-bearing twice over: the body sets `line-height` as a length, so a marker that inherited it would get a hard 24px line box, and a marker also sits between two characters of a paragraph, heading, list item or link, where an inherited strut would grow that line.
- The rule is `visibility: hidden`, never `display: none`. A hidden element still generates a box and reports a real `getBoundingClientRect()`, which is exactly what the macOS cursor-follow resolver measures; a `display: none` marker reports zeros and silently collapses every anchor to the top of the document.
- Headings set `scroll-margin-top` because the body carries no padding of its own; without it a heading reached through an anchor link sits flush against the top edge of the pane.
- The body font stack names `"Inter"` first and keeps the platform stack behind it, so the document still renders if the embedded face fails to decode.

#### `math.css`

MathML styling for the `<math>` output of pulldown-latex, emitted after `base.css`. It is adapted from upstream's `styles.css` (MIT, carloskiki/pulldown-latex) with three changes.

- Upstream's `@font-face` rules are dropped: their relative `url('font/…')` paths cannot resolve inside an inlined document, and `markdown.rs` supplies the face as a data URI instead.
- The `@namespace m` prefix is removed and every `m|…` selector rewritten to plain element form, because the math DOM pulldown-latex emits is un-namespaced as WebKit sees it.
- A `math, math *` family pin is appended, so math resolves to the bundled face rather than a host-installed one.

**Gotchas**

- The file must not contain the literal math element open-tag sequence anywhere, comments included. Tests locate the rendered math by scanning the document for the first occurrence, and a mention here would match ahead of the body.

#### `themes/default.css`

The single shipped theme (`PreviewTheme::Default`, returned by `theme_css`), emitted after `base.css`. It sets `color`, `background-color` and `border-color` only — structure stays in `base.css`. It carries a dark palette plus a `prefers-color-scheme: light` block and declares `color-scheme: light dark`, so the document follows whatever light/dark preference its host web view reports, with no per-appearance render and no work across the FFI boundary.

Each front-end's route from its resolved appearance to that preference is its own: [`architecture.md`](../../../../specs/architecture.md#theming) names both mechanisms, and the GTK chain's version gate is in [`wolf-gnome-decisions.md`](../../../../notes/wolf-gnome-decisions.md#the-previews-light-and-dark). It does not track the live nvim colorscheme.

**Decisions**

- `hr` is painted through `border-top-color`, never `background-color`. `base.css` gives the rule an explicit 24px border-box height so its hairline stays on the baseline grid, and this file wins the cascade over it — a background declaration here would fill that 24px box with a solid bar.

#### `assets/inter-variable.woff2`

The preview body font: the roman variable face from rsms/inter v4.1 (`web/InterVariable.woff2`), a single file spanning the full 100–900 weight axis, which `markdown.rs` inlines from the `inter-variable.woff2.b64` sidecar. No italic face is vendored, so italic body text is synthesised by the engine.

**Decisions**

- `inter-OFL.txt` is the SIL Open Font License 1.1 under which this font is licensed, and it binds anyone shipping Wolf: every copy of the font — including a binary with the base64 embedded — must be accompanied by that copyright notice and license text, the font may not be sold on its own, and it may not be redistributed under any other license. How it ships: [`licenses.rs`](../README.md#licensesrs) `include_str!`s this file into the binary and `render_html` emits it in every document's head, so the text travels with the base64 instead of depending on a separate file staying beside it.

#### `assets/latinmodern-math.woff2`

The math font: the Latin Modern Math face that ships with pulldown-latex (`font/latinmodern-math.woff2`), matching the family name its stylesheet expects and carrying the OpenType `MATH` table that MathML layout needs. `markdown.rs` inlines it from the `latinmodern-math.woff2.b64` sidecar, and `math.css` pins all math content to it. The lmroman text faces pulldown-latex also ships are not vendored — one math font is enough.

**Decisions**

- `latinmodern-math-GUST-FONT-LICENSE.txt` is the `GUST` Font License 1.0 (the Polish TeX users group), which distributes and modifies the font under the LaTeX Project Public License 1.3c or later. Anyone redistributing must carry the license text, and a modified font must be marked as changed and given a new name (`GUST` further requests a manifest listing the renamed files). It ships the same way Inter's does, through [`licenses.rs`](../README.md#licensesrs); the file is CRLF and ends `\r\n\r\n`, which is a third party's bytes and is not this repo's to normalize.

## Decisions

- **Both faces are embedded as base64 `data:` URIs rather than linked.** The rendered document then depends on no host-installed font and no external request, and looks identical in the app's web view and in the browser preview. The base64 lives in a committed single-line `.woff2.b64` sidecar that `concat!` and `include_str!` fold into the `@font-face` rule at compile time, so there is no runtime encoding and no base64 crate in the dependency tree.
- **A sidecar is exactly one line, with no trailing newline.** `include_str!` copies it verbatim into a CSS `url('…')` string, where any newline breaks the data URI. Regenerate with `base64 -i f.woff2 | tr -d '\n' > f.woff2.b64`; the committed raw woff2 is what makes that reproducible and lets the bytes be checked against upstream.
- **rustc sees the `.css`, `.b64` and `.txt` files.** [`../../BUILD.bazel`](../../BUILD.bazel) supplies them through a `compile_data` glob over `src/markdown/**/*.css`, `src/markdown/assets/*.b64`, `src/markdown/assets/*.txt` and `src/licenses/*.txt`, so an asset added outside those patterns compiles under cargo and fails under Bazel. The two license texts here are in that set because [`licenses.rs`](../README.md#licensesrs) compiles them in; the raw woff2 files are provenance and are not. See [`build.md`](../../../../build.md).
- **The 24px baseline grid holds over declared values.** The grid and its border compensation are specified in the [stylesheet spec](../../../../specs/markdown-stylesheet.md#baseline-grid). The sweep in [`markdown_render.rs`](../../tests/markdown_render.rs) fails on any off-grid declared value and is the whole of that guarantee; the laid-out document is a separate claim, which the next bullet covers.
- **`line-height` is a length, not a ratio.** A unitless 1.5 re-multiplies against each descendant's own font size, so inline `code`, `small` and superscripts would each declare a different line box; a length inherits as the computed 24px. The cost is that an element wanting different leading must declare it, as `h1`, `h2` and inline `code` do. A length equalizes only the declared value: inline `code` still needs its zero-leading declaration, for the reason the [stylesheet spec](../../../../specs/markdown-stylesheet.md#inline-code-leading) gives.
- **Values are literal px — not `calc()` over the `--wolf-baseline` token, and not `rem`.** The grid sweep can check that `48px` is a multiple of 24 but could only string-match a `calc()` expression, and the compensated 23/12/11 values are not clean multiples anyway, so wrapping them would hide the compensation. `rem` would resolve against a root font size this document never sets, so a minimum-font-size preference or OS text scaling could silently detune the grid.
- **The first-block and last-block margin resets skip the marker anchors.** Why the bare `:first-child` and `:last-child` forms are inert, and the selectors that replace them, are in the [stylesheet spec](../../../../specs/markdown-stylesheet.md#edge-rules). Do not attempt a fix from the `.wolf-src` side: `:first-child` is structural, so nothing done to the anchor helps.
- **Do not simplify the edge selectors.** Each is unbounded in the length of the anchor run it skips, and each lives in its own rule so an engine lacking Selectors 4 drops that rule alone. Each is a `body.wolf-markdown > *` child selector, so an anchor placed inside a block is a descendant and can never match one; leading and trailing anchor runs come only from column-zero markers and from markers flushed at a block boundary.
- **The font license notices belong in the document head, never in the body.** [`markdown.rs`](../README.md#markdownrs) emits [`licenses.rs`](../README.md#licensesrs)'s block as an HTML comment before `<style>`. An element child of `body.wolf-markdown` would take `:last-child` and satisfy `:not(.wolf-src)`, so the real last block would start matching `:has(~ *:not(.wolf-src))` and lose its bottom-margin reset; prepending one breaks the first-block rule the same way. Measured rather than reasoned: moving the comment into the body reddens `first_child_margin_reset_reaches_the_opening_heading`, `first_child_margin_reset_survives_a_multi_anchor_leading_run` and `last_child_margin_reset_reaches_the_closing_block`.
- **The grid is vertical only.** The list indent, cell padding and blockquote inset are chosen for looks — do not round them to multiples of 24. The body is flush to the pane edges by design; any inset belongs to the preview pane chrome or to the horizontal-only width cap covered in [`../README.md`](../README.md).
- **Code-block syntax highlighting needs no CSS here.** `markdown.rs` writes treesitter colors as inline `style` attributes on spans inside `<pre><code>`, which beat the inherited `pre code` color in the cascade; there is no highlight class and no generated stylesheet. A span declares colour and the three emphases only, never `line-height` or `font-family`, so a highlighted block is exactly as tall as a plain one; why neither the spans nor `pre code` may take a `line-height` is in the [stylesheet spec](../../../../specs/markdown-stylesheet.md#inline-code-leading).
- **Accepted residuals.** A table followed by a block whose `margin-top` exceeds 23px collapses against it and re-exposes that 1px — a constant error that does not accumulate. Images and display MathML have intrinsic heights that no CSS-only rule can snap to the grid, and no JavaScript re-snapping runs in the document.
