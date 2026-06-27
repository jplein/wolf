# Plan: Tier 1 testability for the wolf-gnome GTK/Adwaita front-end

## Status: COMPLETE ✅

### Final state
- Tests: wolf-gnome gdk_key 1✓, paint_golden 3✓ (+1 ignored), roundtrip_nvim 1✓ (real nvim);
  wolf-core regression 15✓; clippy --all-targets clean; GUI denylist clean.
- Docs: crate/src/tests READMEs added; architecture.md Tier-1 status line; S1 doc-comment fixed.
- Tree clean (no stray scratch file; the diagnostic was stale). New crate fully populated.

## Status (was): REVIEWER PASS (seam code approved) — technical-writer closing docs

### Reviewer verdict: seam code correct & in-scope. Outstanding items routed to technical-writer:
- M1: create crates/wolf-gnome/{README.md, src/README.md, tests/README.md} (CLAUDE.md mandate + plan §6).
- M2: add wolf-gnome Tier-1 status line to architecture.md (§6).
- S1: fix src/input.rs module doc comment that references the nonexistent
  `gdk::Key::from_unicode` (doc/impl discrepancy) — code uses gdk::Key constants +
  keyval.to_unicode(); correct the comment to match.
- N1 (too_many_arguments allow on draw_glyph) + N2: ACCEPTED as-is for Tier 1 (no refactor pass).
Note: M1/M2 were always the technical-writer step (workflow step 8/plan order step 9), not a regression.

### Final test run (coder, via nix develop)
- wolf-gnome: gdk_key 1✓, paint_golden 3✓ (1 ignored), roundtrip_nvim 1✓ (real nvim). clippy --all-targets clean.
- wolf-core regression: 15✓. GUI denylist: clean. No implementation fixes were needed after the test fix.

### Coder result
- Implemented flake.nix (GTK toolchain + pinned fontconfig/DejaVu), testdata/fonts.conf,
  workspace member, Cargo.toml, src/lib.rs, src/input.rs (gdk_key), src/render.rs (paint+CellMetrics).
- gtk-rs: gdk4 0.11.2 + cairo-rs/pango/pangocairo 0.22.x against flake's GTK 4.22.4/cairo 1.18.4/pango 1.57.1.
- paint_golden.rs: 3 passed, 1 ignored. wolf-core: still green. clippy clean. GUI denylist: clean.

### ORCHESTRATOR DECISIONS
- D1 (gtk-rs versions): ACCEPT coder deviation. gdk4 0.11 pairs with core crates 0.22
  ("one coherent release set", not identical minors) — that's the real ecosystem invariant.
- D2 (flake $PWD fontconfig path): ACCEPT as-is for now (documented test cmd runs from repo root).
- D3 (TEST BUG): `gdk::Key::from_unicode` does NOT exist in gdk4-rs (0.9 or 0.11).
  Used in gdk_key.rs + roundtrip_nvim.rs → both fail to compile. APPROVED FIX: add the
  coder-verified helper to each test file and replace from_unicode calls:
      use gdk::glib::translate::from_glib;
      fn key_from_char(c: char) -> gdk::Key { unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) } }
  This is test-side only; it does NOT change gdk_key's contract (printables still carry the
  exact glyph; gdk_key reads keyval.to_unicode() and sets shift:false for Char). Route to test-writer.

### Test-writer output (files created)
- crates/wolf-gnome/tests/gdk_key.rs (table-driven, layout-independent)
- crates/wolf-gnome/tests/paint_golden.rs (structural ARGB sampling; #[ignore]d PNG regen)
- crates/wolf-gnome/tests/roundtrip_nvim.rs (real-nvim, types "Hello", shift contract)
- crates/wolf-gnome/tests/common/mod.rs (require_nvim_bin, init_lua_path ../../runtime/nvim/init.lua, config, wake_channel/FlushWaiter, FLUSH_TIMEOUT=10s)

### NOTES the coder MUST honor (from test-writer)
1. Lib named `wolf_gnome` re-exporting gdk_key, paint, CellMetrics at crate root.
2. Cargo.toml MUST rename deps: `gdk4`→`gdk`, `cairo-rs`→`cairo` (tests use `use gdk::` / `use cairo::`).
3. `CellMetrics.cell_w` and `cell_h` must be `pub` (paint_golden reads them).
4. cairo dep MUST enable the `png` feature (else #[ignore]d regen won't compile).
5. Pixel sampling assumes little-endian ARGB32 (B,G,R,A) — fine on x86_64 Linux.
6. gdk::Key associated consts used (Return, Page_Up, ISO_Left_Tab...); if pinned
   gtk-rs minor names differ, gdk_key impl may use from_name internally — keep contract.
7. Shift contract: printables built via gdk::Key::from_unicode carrying exact glyph;
   gdk_key reads keyval.to_unicode() and ALWAYS sets shift:false for Char; named keys
   keep shift (ISO_Left_Tab+SHIFT→Tab{shift:true}).

## Problem statement

Stand up the first platform front-end crate, `wolf-gnome`, but scoped **only** to
its testable "seams" and their tests — **not** the full windowed application. The
goal is to establish the testing foundation described as "Tier 1" so future
front-end work is verifiable without a display server.

### In scope
1. Create `crates/wolf-gnome` and add it to the workspace `members`.
2. Add the GTK4 / libadwaita / Cairo / Pango toolchain to the `flake.nix`
   devShell. The flake currently **deliberately** ships no GUI toolkit; this is
   the build gate. The `cargo tree` denylist in `wolf-core/README.md` stays
   scoped to `wolf-core` only (GTK is allowed in `wolf-gnome`).
3. A pure input-translation function `gdk_key(keyval, modifiers) -> wolf_core::KeyEvent`,
   unit-tested table-driven, honoring the **shift-glyph contract** (printable
   chars carry the already-shifted glyph with `shift = false`; `S-` only for
   named keys).
4. A `paint(cr: &cairo::Context, snap: &Snapshot, metrics)` free function,
   golden-image-tested against an offscreen Cairo `ImageSurface` (no window).
5. A round-trip integration test: real `gdk_key` translation -> `WolfCore::start`
   -> wait for flush -> assert the snapshot, against **real Neovim** (reuse the
   `examples/grid_dump.rs` pattern; no mocks, per project policy).

### Out of scope (defer)
The actual `ApplicationWindow`, `DrawingArea` wiring, wake marshalling, the run
loop, file dialogs, theming, mouse/scroll. Keep `paint()` and `gdk_key()` as
**free functions independent of any widget** so they are testable headlessly.

### Design constraints (from architecture.md / CLAUDE.md)
- Docs are the source of truth; every source dir needs a `README.md`.
- Rendering is Cairo/Pango per architecture decision 2.
- Prefer real-nvim E2E/integration over mocks.

## Environment gate (CONFIRMED by orchestrator probe)
- Current devShell has **none** of gtk4/libadwaita-1/cairo/pango/pangocairo
  (`pkg-config --exists` all fail). Expected — the flake excludes GUI toolkits.
- **Consequence for the coder:** after editing `flake.nix`, the already-loaded
  direnv environment will NOT have GTK. Build/test must run via
  `nix develop --command cargo test ...` (re-evaluates the flake) or after a
  manual devShell reload. The coder should use `nix develop --command ...`.
- `WOLF_NVIM_BIN` is provided by the devShell and is required by the round-trip
  test (it spawns real nvim).

## Key existing API (for reference)
- `WolfCore::start(cfg: CoreConfig, wake: WakeFn) -> Result<WolfCore, CoreError>`
- `CoreConfig { init_lua: PathBuf, cols: u16, rows: u16 }`
- `read_snapshot() -> SnapshotReadGuard` (Derefs to `Snapshot`)
- `send_input(InputEvent)`, `resize(cols, rows)`, `shutdown(self)`
- `Snapshot { grid, highlights, cursor, mode_infos, viewport, seq }`
- `Grid::cell(row, col) -> Option<&Cell>`, `Cell { text, hl_id }`
- `HighlightTable::resolve(hl_id) -> ResolvedStyle`, `Rgb { r, g, b }`
- `KeyEvent { key: Key, ctrl, shift, alt, meta }`, `key_to_notation`
- `examples/grid_dump.rs` is the canonical headless driver template.

## Progress log
- (orchestrator) Probed devShell: GUI libs absent as expected. Plan file reset
  from stale Phase 1 content. Next: invoke architect.
- (orchestrator) Architect plan recorded below. Next: test-writer.

---

# ARCHITECT PLAN

## 0. Summary
Stand up `crates/wolf-gnome` containing only its two pure, widget-free seams —
`gdk_key()` (GDK key → `wolf_core::KeyEvent`) and `paint()` (`Snapshot` → Cairo
`ImageSurface`) — plus toolchain + tests to verify them headlessly. No window,
DrawingArea, run loop, or wake marshalling. `wolf-core` GUI denylist stays scoped
to `wolf-core`; GTK allowed in `wolf-gnome`.

## 1. flake.nix changes
Add to devShell `packages` (compile-time, pkg-config `.pc` probed by gtk-rs `-sys`):
`gtk4` (provides gtk4.pc + GDK for gdk4-sys), `cairo`, `pango` (provides
pangocairo.pc too), `glib`, `gdk-pixbuf`, `graphene`, `harfbuzz`, keep
`pkg-config`. Runtime for deterministic golden render: `fontconfig`, `freetype`,
and a pinned font `dejavu_fonts` (DejaVu Sans Mono). shellHook: export
`FONTCONFIG_FILE` pointing at a minimal repo-controlled `fonts.conf` exposing ONLY
the pinned font (disable host font dirs) so Pango can't pick up host fonts; keep
existing `WOLF_NVIM_BIN`/`RUST_SRC_PATH`. Do NOT add (window-only): `libadwaita`,
`wrapGAppsHook4`, gsettings/icon-theme packages.
BUILD GATE: after editing flake.nix the coder MUST run via
`nix develop --command cargo ...` (re-evaluates flake); direnv env is stale.
Verify first: `nix develop --command pkg-config --exists gtk4 cairo pango pangocairo`.

## 2. Crate scaffolding
- `crates/wolf-gnome/Cargo.toml`: `[lib]` name `wolf_gnome`, path `src/lib.rs`
  (LIBRARY not bin for Tier 1, so tests import the seams; main.rs deferred).
  deps: `wolf-core` (path), and Tier-1 gtk-rs crates ALL pinned to ONE aligned
  gtk-rs minor: `gdk4` (as gdk), `cairo-rs` (as cairo), `pango`, `pangocairo`.
  Deferred (do NOT add): `gtk4` (gtk), `libadwaita` (adw). Comment that all
  gtk-rs crates must share the same minor (mismatched -sys minors fail to link).
  Golden compare uses cairo's own PNG I/O (no image crate).
- Root `Cargo.toml`: `members = ["crates/wolf-core", "crates/wolf-gnome"]`.
- `src/lib.rs` declares `pub mod input;` `pub mod render;` and re-exports
  `gdk_key`, `paint`, `CellMetrics`. No gtk4/adw import anywhere yet.

## 3. gdk_key(keyval, state) -> Option<wolf_core::KeyEvent>
Signature: `pub fn gdk_key(keyval: gdk::Key, state: gdk::ModifierType) ->
Option<wolf_core::KeyEvent>`. Real gtk-rs types (matches future
EventControllerKey callback exactly; gdk::Key constants are compile-time, no
display). `Option` so unmapped/bare-modifier/dead/control keyvals → None.
Mapping: Return & KP_Enter→Enter; Escape→Esc; Tab & ISO_Left_Tab→Tab;
BackSpace→Backspace; Delete→Delete; arrows; Home/End; Page_Up/Down→PageUp/Down;
F1..=F12→Key::F(n) (named const for F-key base); else `keyval.to_unicode()` →
non-control char → Char(c), else None.
Modifiers: ctrl=CONTROL_MASK, alt=ALT_MASK, meta=SUPER_MASK, shift=SHIFT_MASK.
SHIFT CONTRACT: printable branch sets `shift:false` ALWAYS (to_unicode already
gives shifted glyph); named keys pass real `shift` through (so <S-Tab>, <S-Left>,
<S-F5>). ISO_Left_Tab+SHIFT → Key::Tab with shift:true.
None for: bare Shift_L/R, Control_L/R, Alt_L/R, Super_L/R, Caps_Lock; no-unicode
or control-char keyvals; unmapped named keys.

## 4. paint(cr, snap, metrics)
```
pub struct CellMetrics { cell_w: f64, cell_h: f64, baseline: f64,
    font: pango::FontDescription }
pub fn paint(cr: &cairo::Context, snap: &wolf_core::Snapshot, metrics: &CellMetrics)
```
Pure/widget-free (test binds an ImageSurface; prod binds DrawingArea ctx). Helper
`CellMetrics::for_font(cr, family, size_pt)` measuring monospace advance + ascent
via pango Layout/FontMetrics; named consts for default test font family + pt size.
Algorithm per row×col: `cell = grid.cell(r,c)` (skip None); `style =
highlights.resolve(cell.hl_id)` (reverse already applied); fg = style.fg OR
defaults.fg OR FALLBACK_FG, bg likewise; Rgb 0–255 → Cairo 0.0–1.0 (named 255.0);
fill bg rect (col*cell_w,row*cell_h,cell_w,cell_h) — HANDLE cairo Result, don't
drop; if `cell.text.is_empty()` skip fg (wide-trailing/never-painted cell — bg
fill preserves alignment); else pangocairo create_layout(cr), set FontDescription
(toggle bold/italic per style), set text, set_source_rgb(fg), move_to cell
top-left, show_layout. underline via AttrList or manual line (minimal for Tier 1;
undercurl/strikethrough deferred — note in README). One reusable FontDescription;
clone only to toggle bold/italic. No system theme query (deferred).

## 5. Test plan (test-writer)
`tests/gdk_key.rs` — table-driven, no display/no nvim. Named keys incl.
KP_Enter, ISO_Left_Tab+SHIFT→Tab{shift:true}, arrows, Home/End, PageUp/Down,
F1/F5/F12; printable+shift: a→Char('a',shift:false), A→Char('A',shift:false),
'<'→Char('<'); modifiers on named keys (Ctrl+Left, Ctrl+Shift+Left), Super+s
(meta,shift:false), Alt+x; None cases (Shift_L/Control_L/Super_L/Caps_Lock,
no-unicode keyval). Keep assertions LAYOUT-INDEPENDENT (letter case + named keys,
avoid shifted punctuation flakiness). Ctrl+letter must use base unicode + ctrl,
not a control char in Char.
`tests/paint_golden.rs` — hand-built deterministic Snapshot (e.g. 4×2 grid, one
defined hl_id w/ distinct fg/bg + default_colors_set, include a wide-trailing
empty cell) via core public ctors (Grid::new/set_cell, HighlightTable::define/
set_defaults, Snapshot; Cell/HlAttr fields public). ImageSurface ARgb32 + Context
+ pinned CellMetrics; paint; flush. COMPARE STRATEGY (preference order):
(1) PRIMARY always-on STRUCTURAL assertions — sample raw ImageSurface::data() ARGB
at CENTER of known cell rects: bg fill is rasterizer-independent → exact equality;
glyph cell has non-bg pixels in its rect; wide-trailing/empty cell rect is bg-only
(skip path). (2) OPTIONAL exact-PNG golden under tests/testdata/, regen via
WOLF_REGEN_GOLDEN=1 or #[ignore] helper (write_to_png), compared with tolerance
or env-gated (NOT whole-image hash — too brittle across freetype/fontconfig).
Recommendation: structural-only as the gate; PNG regen as dev aid. Font pinned via
FONTCONFIG_FILE.
`tests/roundtrip_nvim.rs` + local `tests/common/mod.rs` (wolf-gnome can't import
core's tests/common): fail-loud WOLF_NVIM_BIN check, init_lua_path resolving
`../../runtime/nvim/init.lua` from wolf-gnome CARGO_MANIFEST_DIR, wake channel +
flush waiter (mirror core's common). Flow: gdk_key(i) → send_input → type word via
gdk_key printables → gdk_key(Escape) → wait+drain flushes → assert row_text(0)
contains the word. Also assert a shifted letter lands uppercase (shift contract
end-to-end). Run via `nix develop --command cargo test -p wolf-gnome`.

## 6. READMEs
Create `crates/wolf-gnome/README.md` (purpose, Tier 1 scope, the two seams +
contracts, build/test via nix develop + pinned font + WOLF_NVIM_BIN, deferred
list, note core denylist N/A here), `crates/wolf-gnome/src/README.md` (per-module:
lib re-exports, input.rs gdk_key mapping/shift/None, render.rs paint algo +
CellMetrics + skip + fallback + attrs applied vs deferred),
`crates/wolf-gnome/tests/README.md` (3 test areas, golden strategy + regen cmd,
font pinning, headless/real-nvim split). Modify `architecture.md`: 1–2 line status
"wolf-gnome Phase: Tier 1 seams exist (gdk_key, paint), verified headlessly;
window/run-loop/wake/theming deferred."

## 7. Risks
Golden nondeterminism (mitigate: pinned font+fonts.conf + structural assertions,
not hashing). Cairo/Pango offscreen needs no display (confirmed CI-safe). gdk::Key
constants compile-time, no display for gdk_key tests. gtk-rs version pinning: all 4
crates same minor; let -sys build scripts report needed lib version vs
nixos-unstable's gtk4/pango/cairo, pick matching gtk-rs minor (0.9 vs 0.11), pin
it. Layout dependence: keep gdk_key asserts layout-independent. Verify
`../../runtime/nvim/init.lua` relative path.

## 8. Implementation order
1) flake.nix (+verify pkg-config). 2) workspace members. 3) crate skeleton +
empty-compiling lib.rs (flush out -sys/pkg-config issues early). 4) input.rs
gdk_key + re-export. 5) tests/gdk_key.rs (fastest loop). 6) render.rs
CellMetrics+paint + re-export. 7) tests/paint_golden.rs (structural first).
8) tests/common + roundtrip_nvim.rs. 9) READMEs + architecture.md line.

## 9/10. Deps + lint
New nixpkgs: gtk4, cairo, pango, glib, gdk-pixbuf, graphene, harfbuzz, fontconfig,
freetype, dejavu_fonts. New cargo (wolf-gnome only): gdk4, cairo-rs, pango,
pangocairo (+ wolf-core path). Must NOT enter wolf-core tree (denylist still
passes since wolf-gnome→wolf-core, not reverse). Rust/clippy (Go/depguard notes
N/A): name magic numbers (255.0 divisor, font size, F-key base, test dims); do NOT
drop cairo `Result`s (handle/expect); keep gdk_key→Option<KeyEvent>, paint→().
