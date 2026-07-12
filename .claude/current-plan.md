# Current Plan: Markdown Preview — LaTeX Math + Inter Font (tasks 1 & 2)

## Status: COMPLETE — implemented, tested (97 lib + integration + real-nvim all green), reviewed (APPROVED), docs written + prettier'd. Assets untracked → need `git add`.

## Problem statement

Implement `markdown-tasks.md` tasks 1 and 2 together in the Wolf preview pipeline.
Both depend on outside assets that must be vendored into the repo and inlined into
the generated preview HTML — no network access at preview time.

### Settled design decisions (NOT to be relitigated)

- Rendering stays a pure, self-contained `&str -> String` transform in
  `crates/wolf-core/src/markdown.rs` (`render_html`), inlining everything into the
  single `<style>` block. Both front-ends (wolf-macos WebKit, wolf-gnome WebKitGTK)
  load the same HTML string unchanged.
- **Task 2 (Inter font):** vendor the Inter VARIABLE woff2 (one file, weights
  100–900 + italic via `font-weight: 100 900`) under `crates/wolf-core/src/markdown/`.
  Embed with `include_bytes!`, emit ONE `@font-face` as a base64 `data:` URI, and put
  `Inter` at the FRONT of the `body.wolf-markdown` font stack in `base.css`, keeping
  the existing `-apple-system`/system fallback behind it.
- **Task 1 (LaTeX math):** enable `pulldown_cmark::Options::ENABLE_MATH` in
  `render_html` so the parser yields `Event::InlineMath` / `Event::DisplayMath`
  carrying the LaTeX. Convert that LaTeX to MathML in pure Rust (evaluate
  `pulldown-latex` first; fall back to another pure-Rust LaTeX->MathML crate if
  needed) and embed the MathML inline. WebKit (macOS Tahoe 26+) and WebKitGTK
  (GNOME 50+) render MathML Core natively.
- Bundle ONE math font carrying an OpenType `MATH` table (e.g. STIX Two Math or
  Latin Modern Math) as a woff2, vendored alongside Inter, embedded as a base64
  `data:` URI in a `@font-face`, and PIN the MathML output to it via CSS so
  rendering does not depend on a host-installed math font. Also inline any small CSS
  the chosen math crate requires.
- Asset delivery is inline `data:` URIs only — NO custom URL scheme handler, NO
  front-end glue.

### Constraints (CLAUDE.md)

- Documentation is source of truth. Add a README for the
  `crates/wolf-core/src/markdown/` assets directory (vendored fonts + math-rendering
  path + licenses: Inter = OFL, math font license). Update affected READMEs.
- Prefer real end-to-end/integration tests over mocks. Add a real-Neovim
  integration test (spirit of `crates/wolf-ffi/tests/preview.rs`): buffer with inline
  `$...$` and a `$$...$$` block → pull rendered HTML → assert MathML present + block
  rendered + Inter `@font-face` + bundled math font present. Add wolf-core unit tests
  to `markdown.rs`: math→MathML, `@font-face`/`Inter` default emitted, markers +
  heading-slug behavior intact.
- Keep the marker/anchor scheme and heading-slug pipeline intact.
- Write a short decision note at repo root (mirroring
  `preview-code-block-syntax-highlighting-decisions.md`): MathML-over-KaTeX, the
  bundle-and-pin-a-math-font rationale, inline-data-URI delivery.

## Plan (from architect)

### Dependencies
- Add `pulldown-latex = "0.7"` to `crates/wolf-core/Cargo.toml` (after line 16). Pure-Rust
  LaTeX→MathML (MIT), MathML Core, ships a matching Latin Modern Math woff2 + `styles.css`.
  Chosen over stale `latex2mathml`.
- NO base64 crate: commit woff2 pre-encoded as single-line base64 sidecars, pull in via
  `include_str!` + `concat!` (zero runtime encode).
- Bazel: deps vendored via `crate_universe` (MODULE.bazel:87-103 → Cargo.Bazel.lock). After
  editing Cargo.toml: regenerate `Cargo.lock`, then `CARGO_BAZEL_REPIN=1 bazel build
  //crates/wolf-core:wolf_core`, commit `Cargo.lock`/`Cargo.Bazel.lock`/`MODULE.bazel.lock`.
  CODER MUST VERIFY repin succeeds and denylist check (`cargo tree` / build.md:252-256) passes.

### Vendored assets — new dir `crates/wolf-core/src/markdown/assets/`
Commit raw woff2 (provenance) + single-line `.b64` sidecar (used by code) + license each:
- `inter-variable.woff2` (+ `.woff2.b64` + `inter-OFL.txt`) — Inter roman variable, SIL OFL 1.1,
  from rsms/inter releases `web/InterVariable.woff2`.
- `latinmodern-math.woff2` (+ `.woff2.b64` + `latinmodern-math-GUST-FONT-LICENSE.txt`) — GUST GFL,
  from pulldown-latex repo `font/latinmodern-math.woff2` (matches its `styles.css` family). Do NOT
  vendor the lmroman text faces (design: ONE math font).
- `.b64` MUST be single-line (`base64 -i f.woff2 | tr -d '\n' > f.woff2.b64`), else data URI breaks.
- Coder fetches binaries (architect/agents cannot). Record actual byte sizes in the assets README.
- ~1.0–1.3 MB added to each HTML doc; acceptable (renders debounced, one trailing render/burst).

### `crates/wolf-core/src/markdown.rs` changes
- §4.1 imports: add `pulldown_latex::{Parser as LatexParser, Storage, mathml::push_mathml,
  config::{RenderConfig, DisplayMode}}` — CODER VERIFY exact paths vs docs.rs 0.7.1.
- §4.2 new const CSS near `BASE_CSS` (line 75), all const via `concat!`+`include_str!`:
  - `INTER_FONT_FACE`: `@font-face{font-family:'Inter';font-weight:100 900;font-style:normal;
    font-display:swap;src:url('data:font/woff2;base64,<b64>') format('woff2')}`.
  - `MATH_FONT_FACE`: `@font-face{font-family:'Latin Modern Math';src:url('data:font/woff2;base64,<b64>')
    format('woff2')}`.
  - `MATH_CSS`: NON-font rules from pulldown-latex `styles.css` with: (1) drop lmroman `@font-face` +
    `mtext` font rule; (2) rewrite namespaced `m|math` selectors to plain `math`; (3) append pin
    `math, math * { font-family: "Latin Modern Math", math; }`.
- §4.3 `render_html`: add `options.insert(Options::ENABLE_MATH);` (line 111-119).
- §4.4 new adapter `MathRenderer<I>` (near HeadingIdAssigner ~305), preserves `(Event, Range)`:
  - `Event::InlineMath(cow)` → `(Event::InlineHtml(mathml.into()), range)`, `DisplayMode::Inline`.
  - `Event::DisplayMath(cow)` → `(Event::InlineHtml(mathml.into()), range)`, `DisplayMode::Block`
    (Block gives `display="block"` on `<math>`).
  - USE `Event::InlineHtml` for BOTH (load-bearing: math events are emitted at inline positions; block
    HTML would corrupt paragraph nesting; InlineHtml passes through verbatim, not double-escaped).
  - Helper: `Storage::new()` + `LatexParser::new(latex, &storage)` + `push_mathml(&mut out, p, cfg)`.
    On `Err`, gracefully fall back to `Event::Text(format!("${latex}$"))` — NEVER unwrap.
- §4.5 chain order: `offset_iter → HeadingIdAssigner → MathRenderer → MarkerInjector → push_html`.
  MathRenderer before MarkerInjector; replacements are `InlineHtml` (not block Start), so
  `is_block_start` returns false and no marker is injected inside math. Do NOT add math to
  `is_block_start`.
- §4.6 embed CSS in the `format!` (128-132): `<style>` = `{INTER_FONT_FACE}{MATH_FONT_FACE}{BASE_CSS}
  {MATH_CSS}{theme}`.

### `crates/wolf-core/src/markdown/base.css` (lines 43-44)
- Prepend `"Inter",` to `body.wolf-markdown` font-family, keep the full system fallback behind it.
  (CSS asset, not Markdown — no prettier.)

### `crates/wolf-core/BUILD.bazel` (compile_data ~line 34)
- Extend glob to include the b64 sidecars: `glob(["src/markdown/**/*.css", "src/markdown/assets/*.b64"])`
  (they are `include_str!`'d). Raw woff2/licenses are provenance-only, not needed by rustc.

### Tests (test-writer)
- §7.1 unit tests in `markdown.rs` mod tests (after line 653), via `render_html`:
  `inline_math_renders_mathml` (`<math`/`</math>`, source not visible); `display_math_is_block`
  (`display="block"`); `math_not_double_escaped` (literal `<math`, not `&lt;math`);
  `inter_font_face_present` (`@font-face`, `Inter`, `100 900`, `data:font/woff2;base64,`);
  `math_font_present` (`Latin Modern Math`, 2nd data URI, math font pin); `marker_not_inside_math`
  (no `wolf-src` span between `<math` and `</math>`). Existing 528-652 tests must still pass.
- §7.2 wolf-core integration `crates/wolf-core/tests/markdown_render.rs` (no nvim, primary CI proof):
  render `$e=mc^2$` inline + `$$\int_0^1 x\,dx$$` block → assert `<math`, `</math>`, `display="block"`,
  Inter `@font-face` + data URI, `Latin Modern Math` + data URI. (Has Bazel target already.)
- §7.3 real-Neovim `crates/wolf-ffi/tests/preview.rs` — ONE `#[test]` matching existing harness
  (`nvim_bin_or_skip`, `wolf_ffi_start`, `wait_for_attach`, `open_preview("split_right")`,
  `render_preview`): fixture `math.md` with inline + display math → assert HTML has `<math`,
  `display="block"`, Inter `@font-face`, `Latin Modern Math`.
- Edge cases: malformed LaTeX (`$\frac{1}$`) degrades to visible text, no panic; no-math doc renders
  identically (marker/heading tests hold).

### Decision note + docs
- NEW `preview-math-and-fonts-decisions.md` at repo root (mirror the syntax-highlighting note):
  Decisions 1 MathML-via-pulldown-latex, 2 Inter one-file/one-face at front of stack, 3 Latin Modern
  Math bundled+pinned, 4 inline data URIs only / no scheme handler / no base64 crate; licenses +
  provenance; rationale/consequences. prettier --write after.
- NEW `crates/wolf-core/src/markdown/README.md` — base.css/themes/assets, each woff2 filename +
  upstream URL + license + single-line-b64 note + that MATH_CSS is adapted from pulldown-latex styles.css.
- UPDATE `crates/wolf-core/src/README.md` markdown.rs section (98-106): ENABLE_MATH, MathRenderer,
  inlined fonts + math CSS, pulldown-latex dep.
- UPDATE `crates/wolf-core/README.md` (if it lists pipeline/deps); `build.md` (new .b64 compile_data,
  repin+denylist); mark `markdown-tasks.md` tasks 1-2 done. prettier --write all touched Markdown.

### Implementation order
1. Vendor font/license/b64 files. 2. Add dep + regen Cargo.lock + Bazel repin + denylist. 3. BUILD.bazel
compile_data. 4. CSS consts + base.css. 5. MathRenderer + render helper (graceful fallback). 6. Enable
ENABLE_MATH + wire chain + embed CSS. 7. Tests. 8. Decision note + READMEs + prettier.

### Coder MUST-VERIFY flags
- Exact pulldown-latex 0.7.1 module paths + `push_mathml`/`Parser::new`/`RenderConfig` signatures.
- A lone `$$…$$` wrapping on pulldown-cmark 0.12 (InlineHtml is safe either way).
- Bazel repin success + denylist pass; single-line b64; actual font byte sizes.
- No `#[allow]` in markdown.rs; handle `push_mathml` `Err` explicitly (no unwrap).

## Status: architect plan recorded → invoking test-writer
