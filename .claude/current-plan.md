# Plan: Per-cell text-attribute painting in the Wolf macOS front-end

## STATUS: COMPLETE

Docs verified by technical-writer: all coder-updated docs accurate; one straggler fixed
(`build.md` test-targets list gained `//crates/wolf-ffi:attribute_highlight` + GlyphStyleTests
note). No stale "attributes not painted" claims remain (GNOME undercurl/strikethrough deferral
is unrelated + correctly still deferred). `prettier --write build.md` → unchanged.

Remaining for user: manual on-screen smoke (needs window server) — open a lighthouse-styled
markdown buffer, confirm bold headings/`**bold**` bold, `_italic_` italic, link underline in the
special (comment-fg) color; optionally eyeball-tune `strikethroughRatio=0.3`.

## DOCS VERIFICATION (review PASSED)

### Reviewer verdict: PASS — approved
All 7 areas correct; flag bits mirror `mod flag` exactly; underline/strikethrough are manual CGContext strokes in the resolved color; `makeVariant` verifies `CTFontGetSymbolicTraits` before accepting a variant; cellWidth from regular face only. Non-blocking: `strikethroughRatio=0.3` may sit slightly high — eyeball during manual smoke.

### Coder output
- wolf-ffi: `special` field + `special()` accessor + bridge decl; regenerated committed bindings (`bazel run //crates/wolf-ffi:gen_bindings`; `cell.special()` in generated Swift).
- wolf-macos: new `Sources/WolfKeyMap/GlyphStyle.swift` (seam + `extension FfiColor: Equatable {}`, no `@retroactive` since same module) wired into `wolf_key_map` srcs; `CellMetrics.swift` 4-face cache + `font(for:)`; `PaneContainerView.swift` resolves style per cell, `drawGlyph(style:)` + manual `CGContext` decoration strokes, cursor uses plain `.regular`.
- Docs updated + prettier: Sources/README.md, wolf-macos/README.md, wolf-ffi/src/README.md, wolf-ffi/README.md, architecture.md.
- **Tests (bazel):** frame_pack, attribute_highlight (RAN real nvim), config, bridge_smoke, treesitter_highlight (RAN real nvim), WolfKeyMapTests, bundle_layout_test — ALL PASSED. wolf_ffi / wolf_macos_lib / Wolf build.
- Only deviation: dropped `@retroactive` (FfiColor same module). No tests flagged wrong.
- Manual on-screen smoke: left for user (needs window server).

### Test-writer output
- Created: `crates/wolf-ffi/tests/attribute_highlight.rs`, `crates/wolf-macos/Tests/WolfKeyMapTests/GlyphStyleTests.swift`.
- Modified: `crates/wolf-ffi/tests/frame_pack.rs` (+`special_color_packs_from_resolved_style`), `crates/wolf-ffi/BUILD.bazel` (+`attribute_highlight` target), `crates/wolf-macos/BUILD.bazel` (+GlyphStyleTests src).
- **Coder note:** `FfiColor` is NOT declared `Equatable` in generated Swift → add `extension FfiColor: Equatable {}` for `GlyphStyle: Equatable` to synthesize. Frame cells read via `frame.cells() -> Vec<FfiCell>`; `FfiCell::text()` returns `String`. `FfiColor` = `{ rgb: UInt32, set: Bool }`.

## Problem statement

The Wolf macOS front-end renders Neovim highlight **colors** (fg/bg) but not text
**attributes** (bold, italic, underline, undercurl, strikethrough) nor the
**special** (underline) color. As a result, Markdown documents styled by the
lighthouse colorscheme — which uses attributes (bold headings, bold `**bold**`,
italic `_italic_`, underlined links in a special color `sp=commentFG`) rather than
foreground colors for markup — render as plain text, while color-styled content
(fenced code via treesitter injection, link URLs) renders correctly.

Treesitter is confirmed loading and highlighting correctly; this is purely a
front-end rendering gap.

### Verified facts (from investigation)
- `crates/wolf-core/src/render.rs` — `ResolvedStyle` already resolves
  bold/italic/underline/undercurl/strikethrough and `special` color.
- `crates/wolf-ffi/src/lib.rs` — flag constants (~99-108), `pack_flags` (~1565),
  `FfiCell::flags()` (~974). `FfiCell` struct (~946) carries only `fg`/`bg`/`flags`
  — **NOT** `special`. `FfiCell` built ~1471.
- `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift:761-775` —
  `drawGlyph` paints only `.font: metrics.font` + `.foregroundColor: fg`; never
  reads `cell.flags()`.
- Documented deferral: `crates/wolf-macos/Sources/README.md:31`,
  `crates/wolf-macos/Sources/WolfMacOS/CellMetrics.swift:23-26`.
- **iA Writer Mono S (the default font) ships real static faces**: Regular, Italic,
  Bold, BoldItalic (bundled at `Contents/Resources/fonts/iAWriterMonoS-*.ttf`,
  CoreText family name `iA Writer Mono S`). So bold/italic have true faces available.

## Required changes (from request)
1. Extend `FfiCell` to carry `special` color (from `ResolvedStyle.special`), exposed
   via `special() -> FfiColor` mirroring `fg()/bg()`; populate where FfiCell is built.
2. macOS renderer: thread `cell.flags()` + `cell.special()` into glyph paint:
   - bold/italic → real iA Writer Mono Bold/Italic/BoldItalic faces via
     `NSFontDescriptor` symbolic traits; synthesize as fallback; keep advance width stable.
   - underline + undercurl → underline, colored with special when set else fg.
   - strikethrough → strikethrough.
3. Update docs claiming attributes are unpainted (Sources/README.md, CellMetrics.swift,
   wolf-ffi/src/README.md, others).

## Testing
- wolf-ffi integration test with a REAL embedded Neovim: define a hl group with
  bold/italic/underline + special color, apply to text, assert the FfiCell exposes
  the correct flags AND special color across the FFI boundary. (Primary guard.)
- Swift: extract attribute/font-variant mapping into a testable seam where practical;
  smoke-level otherwise (window server needed for full rendering).
- Run existing treesitter_highlight.rs + related FFI tests for regressions.

## Architecture plan (from architect)

### (A) wolf-ffi changes — all in `crates/wolf-ffi/src/lib.rs`
- **A1** Add `special: Option<Rgb>` to the `FfiCell` struct (~946-953), next to `fg`/`bg`. Update struct doc (~938-944).
- **A2** Populate `special: style.special,` where `FfiCell` is built in `FfiFrame::from_snapshot_grid` (~1469-1474). Only construction site.
- **A3** Add accessor in `impl FfiCell`: `pub fn special(&self) -> FfiColor { FfiColor::from_opt(self.special) }` (mirror `bg()`).
- **A4** Add `fn special(&self) -> FfiColor;` to the `extern "Rust" { type FfiCell; … }` bridge block (~425-437), with doc mirroring `bg()`.
- **A5 — REGEN (manual, required):** generated Swift/C bindings under `crates/wolf-ffi/generated/` are COMMITTED, not build-time-generated. After editing the bridge, run `bazel run //crates/wolf-ffi:gen_bindings`, then commit regenerated `generated/wolf-ffi.swift` + `generated/wolf-ffi.h`. Skipping = Swift can't see `cell.special()` → macOS lib won't compile. (`module.modulemap` + `wolf_ffi_wake.h` are hand-written, not overwritten.)

### (B) wolf-macos renderer changes
- **B1 — testable seam (pure, Foundation-only, in `WolfKeyMap`):** new
  `crates/wolf-macos/Sources/WolfKeyMap/GlyphStyle.swift` (imports only Foundation; `FfiColor` is in scope there via generated_swift):
  - `public enum GlyphFace: Equatable { case regular, bold, italic, boldItalic }`
  - `public struct GlyphStyle: Equatable { face; underline: Bool; underlineColor: FfiColor; strikethrough: Bool; strikethroughColor: FfiColor }`
  - `public enum GlyphStyleResolver { public static func resolve(flags: UInt8, fg: FfiColor, special: FfiColor) -> GlyphStyle }`
  - Private flag consts MIRRORING `mod flag` in lib.rs (`bold=1<<0, italic=1<<1, underline=1<<2, undercurl=1<<3, strikethrough=1<<4`) — keep in lock-step (single-source-of-truth discipline).
  - Logic: face = boldItalic if both bits, else bold, else italic, else regular. underline = underline-bit OR undercurl-bit (undercurl → straight underline this pass). underlineColor = special if special.set else fg. strikethrough = strikethrough-bit. strikethroughColor = fg.
- **B2 — cache 4 faces in `CellMetrics.swift`:** replace single `let font` with cached variants + `func font(for: GlyphFace) -> CTFont`. In `measured(config:)` derive bold/italic/boldItalic via `CTFontCreateCopyWithSymbolicTraits` (`.traitBold`/`.traitItalic`); private `makeVariant(base, addingTraits:)` returns trait-matched font when CoreText resolves a DISTINCT face, else falls back to base. **Advance-width stability:** compute `cellWidth` from the REGULAR face ONLY; all faces paint into the same slot (grid is fixed-cell). iA Writer Mono S bold/italic are real monospaced faces sharing the regular advance.
- **B3 — thread into paint path (`PaneContainerView.swift`):**
  - In `paintCells` (~606-632) where `fg` is pulled (~627) and `drawGlyph` called (~628): `let style = GlyphStyleResolver.resolve(flags: cell.flags(), fg: cell.fg(), special: cell.special())`, pass `style` to `drawGlyph`.
  - `drawGlyph` (~761-775): add `style: GlyphStyle` param; use `metrics.font(for: style.face)` instead of `metrics.font`.
  - **Underline/strikethrough = MANUAL `CGContext` strokes** (NOT `.underlineStyle`): `CTLineDraw` draws underline only in the FOREGROUND color and ignores `.underlineColor`/`.strikethroughColor` (AppKit layout attrs). After `CTLineDraw`, stroke a horizontal line in the resolved color: underline at `y + baseline + underlineOffset` in `style.underlineColor`; strikethrough near mid-x-height in `style.strikethroughColor`. Use named `private static let` constants for offsets/thickness. Undercurl = straight underline this pass.
  - `drawCursor` (~804) also calls `drawGlyph`: pass a plain `.regular` style for the cursor glyph this pass (avoid bold overflow under block cursor).

### (C) Tests
- **C1** Extend `crates/wolf-ffi/tests/frame_pack.rs` (existing rust_test, no BUILD change): new `special_color_packs_from_resolved_style` mirroring `explicit_highlight_colors_pack_to_set_rgb` (~114-155) — `HlAttr { special: Some(Rgb::from_packed(0x0000FF00)), underline: true, .. }`, set a cell, build frame, assert `cell.special().set == true`, `cell.special().rgb == 0x0000FF00`, `flags() & UNDERLINE != 0`; and UNSET special → `set == false`. Deterministic, no nvim.
- **C2** New `crates/wolf-ffi/tests/attribute_highlight.rs` + new `rust_test` target in `crates/wolf-ffi/BUILD.bazel` modeled on `bridge_smoke` (~88-119): data/env = `//bazel/nvim:nvim_tree` + `//runtime/nvim:init.lua`, `WOLF_TEST_NVIM_DIR`/`WOLF_TEST_INIT_LUA`, `tags=["no-sandbox"]`, `size="small"`, deps `:wolf_ffi` + dev crates. Reuse `tests/common/mod.rs`. Follow `treesitter_highlight.rs` scaffolding (SKIP-when-unset via `nvim_bin_or_skip()`, `char_key`/`enter_key`/`send_ex_command`, poll `read_frame()` until `cols()>0`). Paint a known hl group via a single `:lua` ex command: `nvim_buf_set_lines` to put `X`, `nvim_set_hl(0,'WolfAttr',{bold=true,italic=true,underline=true,special='#00ff00',fg='#ffffff'})`, `vim.fn.matchadd('WolfAttr','X')`. Poll for repaint, scan `read_frame().cells()` for `text()=="X"`, assert `flags()` has BOLD|ITALIC|UNDERLINE and `special().set && special().rgb==0x0000FF00`. Local flag consts mirroring `mod flag`. Loud SKIP banner when nvim unset.
- **C3** New `crates/wolf-macos/Tests/WolfKeyMapTests/GlyphStyleTests.swift`, add to `WolfKeyMapTests` swift_test `srcs` (BUILD ~108-119). `@testable import WolfKeyMap`, `import XCTest` (mirror `CellMetricsMathTests.swift`). Assert pure `GlyphStyle`: flags 0 → regular/no decorations; bold→.bold; italic→.italic; both→.boldItalic; underline-bit OR undercurl-bit → underline true; special.set → underlineColor==special else ==fg; strikethrough-bit → strikethrough true, strikethroughColor==fg. Construct `FfiColor` values directly.
- **C4** Regression run: `frame_pack`, `config`, `bridge_smoke`, `treesitter_highlight`, `attribute_highlight`, `WolfKeyMapTests`.

### (D) Docs to correct (attributes now painted; prettier --write, no hard breaks)
- `crates/wolf-macos/Sources/WolfMacOS/CellMetrics.swift` doc comment (~22-27).
- `crates/wolf-macos/Sources/README.md` — CellMetrics bullet (~31) + PaneContainerView bullet (~25).
- `crates/wolf-ffi/src/README.md` — FfiCell accessors (~13, add `special()`), mod ffi (~9), pack_flags (~21) if they enumerate cell contents.
- `architecture.md` — "Deferred for macOS" clause (~71) naming text attributes; remove/rewrite. (~148 core hl_id desc stays.)
- Grep `crates/wolf-macos/README.md` + `build.md` for "not yet"/"deferred"/attribute claims; fix stragglers.

### (E) Build/verify order (bazel; see build.md)
1. wolf-ffi Rust A1-A4 → `bazel build //crates/wolf-ffi:wolf_ffi`.
2. Regen A5 → `bazel run //crates/wolf-ffi:gen_bindings`; commit generated.
3. Rust tests C1,C2 → `bazel test //crates/wolf-ffi:frame_pack //crates/wolf-ffi:attribute_highlight`.
4. Swift seam B1 + test C3 → `bazel test //crates/wolf-macos:WolfKeyMapTests`.
5. CellMetrics B2 → `bazel build //crates/wolf-macos:wolf_macos_lib`.
6. PaneContainerView B3 → build lib, then `bazel build //crates/wolf-macos:Wolf`.
7. Docs D + prettier.
8. Full regression (E-list) + manual on-screen smoke of a bold/italic/underlined markdown buffer.

### (F) Ranked risks
1. **CTLineDraw underline color** — draws in fg, ignores `.underlineColor`. MUST draw decorations as manual CGContext strokes. Verify by smoke.
2. **swift-bridge regen is manual + forgettable** — generated Swift is committed; run gen_bindings before Swift compiles.
3. **Advance-width stability** — cellWidth from regular face only; all faces into same slot.
4. **Painting a known hl group in the FFI test** — `matchadd`+`nvim_set_hl` chosen; fallbacks: extmark/`nvim_buf_add_highlight` or `:syntax match`. Poll for repaint before scanning (FLUSH_TIMEOUT/POLL_INTERVAL pattern).
5. **Trait-match fallback** — `CTFontCreateCopyWithSymbolicTraits` may return non-distinct face; `makeVariant` must detect + fall back or bold renders as regular. Smoke-only.
6. **Constants discipline** — named consts for stroke offsets/thickness; keep Swift flag bits in lock-step with `mod flag`. No new external deps.
