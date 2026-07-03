# Feature: WolfCursor / WolfTitlebar theme integration

## Status

Phase: reviewer (implementation complete, all tests green)

## Coder report (summary)

All targets green: `//bazel/lighthouse:lighthouse_tree_test`, `//crates/wolf-core:theme` (new Bazel target, pre-authorized — cargo unavailable on this machine, modeled on `state_save`), `//crates/wolf-ffi:{theme,frame_pack,lighthouse_load,bridge_smoke}`, full `//crates/wolf-core:all` + `//crates/wolf-ffi:all`, `bazel build //crates/wolf-macos:Wolf`, `bundle_layout_test`, `WolfKeyMapTests`, clippy clean. Lighthouse pinned to 9da2bba, `_SHA256 = 7e3d0bda062b74fa85261107beadeec515cc6e661adc5d1f6457c5f03997967c`, MODULE.bazel.lock regenerated. Bindings regenerated (`generated/wolf-ffi.{h,swift}`). Docs updated + prettier run.

Deviations (accepted by orchestrator): new `//crates/wolf-core:theme` Bazel target; wolf-gnome cursor tests verified via a throwaway in-workspace harness (15/15 pass, harness deleted) — full `cargo test -p wolf-gnome` still needs a Linux run (follow-up for user); `setup_theme_tracking` is `pub` mirroring `setup_dir_tracking`; the lighthouse_load README note landed in build.md (wolf-ffi/README.md doesn't document per-test targets).

Orchestrator decisions: no second test-writer pass (Linux gnome run impossible here; out-of-range-payload test not worth the direct-notification plumbing per coder's own assessment).

## Test-writer report

Tests written (NOT yet run — no implementation exists yet):

- NEW `crates/wolf-core/tests/theme.rs` — 4 tests: `startup_theme_is_all_none_without_wolf_groups`, `runtime_hi_change_updates_theme_on_colorscheme_autocmd`, `linked_wolf_cursor_resolves_to_effective_attributes`, `cleared_groups_fall_back_to_all_none`.
- NEW `crates/wolf-ffi/tests/theme.rs` — 1 test: `theme_titlebar_and_cursor_follow_wolf_groups` (baseline → define → assert → clear → fallback).
- EXTENDED `crates/wolf-ffi/tests/frame_pack.rs` — 3 deterministic tests: `cursor_fill_glyph_honor_theme_cursor_fg_and_bg`, `cursor_glyph_falls_back_to_cell_bg_when_theme_fg_unset`, `theme_cursor_wins_over_mode_cursor_attr`.
- EXTENDED `crates/wolf-ffi/tests/lighthouse_load.rs` — polls `theme_titlebar_bg()`, asserts pinned dark-variant values (bg 0x1b1b1b, fg 0xf1f1f1, cursor fill 0x009fce, glyph 0x000000) tied to lighthouse commit 9da2bba.
- EXTENDED `crates/wolf-gnome/src/cursor.rs` `mod tests` — 3 new tests, existing cases untouched.
- EXTENDED `crates/wolf-ffi/BUILD.bazel` — new `rust_test` target `theme`.

**Orchestrator decision on the flagged ambiguity**: the `theme` Bazel target must use `tags = ["no-sandbox"]` (the sibling convention for every real-nvim target in that BUILD file), NOT `["local"]` as the plan guessed. Coder: change the tag and drop the divergence comment.

API assumptions the coder must satisfy: `wolf_core` root re-exports `ThemeColors`/`ThemeGroup` (Copy + Debug + Default + PartialEq); `Snapshot.theme` public and covered by `..Snapshot::default()`; handle methods `theme_titlebar_bg()`/`theme_titlebar_fg()` returning `FfiColor { rgb, set }`; theme-honor branch precedes the mode-attr branch in BOTH `cursor_colors` implementations.

## Problem statement

Wolf should let a colorscheme control two app-level colors via optional highlight groups that have no effect on the terminal Neovim canvas:

- **WolfCursor** — drives the app cursor color.
- **WolfTitlebar** — drives the macOS titlebar color.

Wolf must read these groups at startup **and** react when the theme changes at runtime (`:set background=...`, `:colorscheme ...`).

Fallback behavior when a group is unspecified or cleared:

- Cursor: existing light-or-dark-depending-on-background logic.
- Titlebar: system titlebar color / current behavior.

## Requirements

1. Detect the two named highlight groups from the live Neovim runtime. Note: the redraw stream's `hl_attr_define` / `default_colors_set` only cover attrs actually rendered on the grid; named-group lookup likely needs either the `hl_group_set` UI event or a `ColorScheme` autocommand notification from `runtime/nvim/init.lua` (the established `wolf_*` notification pattern, e.g. `wolf_cwd`).
2. React to runtime theme changes, not just startup.
3. Bump the vendored lighthouse colorscheme to commit `9da2bba190305b2c8d5d300c254a48c487dd4a1e` (defines these groups), per the documented procedure in `build.md` ("To bump the vendored lighthouse colorscheme"): update `_COMMIT` and `_SHA256` in `bazel/lighthouse/extensions.bzl`, re-run `bazel test //bazel/lighthouse:lighthouse_tree_test`.
4. Wire through the full stack: wolf-core snapshot state → wolf-ffi exposure → wolf-macos (cursor + titlebar) and wolf-gnome (cursor at least; titlebar not applicable → current behavior).
5. Tests: prefer end-to-end/integration tests that start a real Neovim process (model: `crates/wolf-ffi/tests/lighthouse_load.rs`). Cover startup pickup, runtime theme change, and fallback when groups are absent/cleared.
6. Documentation: update affected README.md files (docs are source of truth) and build.md if touched.

## Architect plan

### Summary

Wolf will let a colorscheme drive two app-level colors through the optional highlight groups **WolfCursor** (app cursor fill/glyph) and **WolfTitlebar** (macOS titlebar background, fg reserved for title text later). Because these groups are never painted on the grid, the redraw stream (`hl_attr_define`) never carries them and nvim's `hl_group_set` UI event only reports *builtin* groups — so detection uses the established `wolf_*` rpcnotify pattern: a new `_G.wolf_setup_theme_tracking(chan)` helper in `runtime/nvim/init.lua` (a `ColorScheme` autocmd + emit-once) sends a `wolf_theme` notification that `RedrawHandler` folds into a new `Snapshot.theme`. The cursor override is applied where the existing reverse-video cursor logic already lives in Rust (`wolf-ffi::cursor_colors` and `wolf-gnome::cursor::cursor_colors`), so the Swift cursor renderer needs zero changes; the titlebar color crosses the bridge via two new `WolfCoreHandle` accessors and is applied/reverted on `NSWindow.backgroundColor` in the macOS shell. The vendored lighthouse pin is bumped to `9da2bba190305b2c8d5d300c254a48c487dd4a1e` (verified upstream: it defines both groups with plain `fg`/`bg`, no links). Real-nvim integration tests cover startup pickup, runtime `:hi`/ColorScheme change, and cleared-group fallback; on-screen titlebar color is manual smoke only.

### Design decisions (resolved)

**D1 — Detection: a `ColorScheme` autocmd + emit-once (`wolf_theme` rpcnotify).**

- `hl_group_set` is not viable: it reports **builtin** highlight groups only (the internal `HLF_*` set), never custom groups like `WolfCursor`. Polling is contrary to the repo's event-driven design.
- Matches the five existing `wolf_*` notifications (`crates/wolf-core/src/README.md` "Directory tracking" is the closest template, including the emit-once).
- **Startup ordering is safe**: `init.lua` activates lighthouse while nvim boots; core installs the helper over RPC *after* `ui_attach` (in `WolfCore::start`), and the helper **emits once immediately**, so the startup colorscheme is always reported — same reasoning as `wolf_cwd`.
- **Runtime changes**: `:colorscheme X` fires `ColorScheme`; `:set background=...` re-sources the active colorscheme (`:h 'background'`), which also fires `ColorScheme`. No `OptionSet` autocmd needed.
- **Links**: read groups with `vim.api.nvim_get_hl(0, { name = ..., link = false })` so a linked group resolves to effective attributes. Wrap in `pcall` (defensive; a never-defined name yields `{}` on 0.12 but the pcall costs nothing).
- **Payload shape**: FOUR flat scalars, `vim.rpcnotify(chan, "wolf_theme", cursor_fg, cursor_bg, titlebar_fg, titlebar_bg)`, each `h.fg or -1` / `h.bg or -1`. Flat scalars deliberately sidestep the empty-Lua-table map-vs-array msgpack ambiguity a table payload would have for a cleared group (`hi clear WolfCursor` → `nvim_get_hl` → `{}`). `-1` = unset (nvim colors are 24-bit, so `-1` is unambiguous).

**D2 — State representation and seams.**

- `wolf-core::render`: new `ThemeGroup { fg: Option<Rgb>, bg: Option<Rgb> }` and `ThemeColors { cursor: ThemeGroup, titlebar: ThemeGroup }` (both `Clone, Copy, Debug, Default, PartialEq, Eq`, all-`None` default); new field `Snapshot.theme: ThemeColors`. Theme colors ARE render state (they change what is painted), so they live in the snapshot — unlike `TrackedCwd`.
- `redraw.rs`: `THEME_EVENT = "wolf_theme"` routed in `handle_notify`; `apply_theme` takes the write lock via `write_async`, stores the parsed `ThemeColors`, does **not** bump `seq` (not a redraw frame — same convention as `current_file`/`active_win`), and the caller fires the wake after the lock drops so front-ends repaint/apply.
- FFI "unset" convention: reuse the existing transparent `FfiColor { rgb: u32, set: bool }` (`crates/wolf-ffi/src/lib.rs:229`, `FfiColor::from_opt`) — exactly how `default_fg`/`default_bg` already express absence.
- Bridge surface: two new `WolfCoreHandle` methods, `theme_titlebar_bg() -> FfiColor` and `theme_titlebar_fg() -> FfiColor` (unset when the core is gone). The titlebar is window-level, not per-grid, so it does NOT ride `FfiFrame`. The cursor colors need **no new FFI surface**: the WolfCursor override happens inside the existing `cursor_colors` at frame-pack time, so `FfiFrame::cursor_fill()/cursor_glyph()` carry it automatically.

**D3 — macOS titlebar application: `NSWindow.backgroundColor`, cached-default revert.**

- `MainWindow` uses `.fullSizeContentView` + `titlebarAppearsTransparent = true` (`crates/wolf-macos/Sources/WolfMacOS/MainWindow.swift:91`), so the window background shows through the titlebar over the editor pane while the sidebar keeps its own vibrancy material. Setting `backgroundColor` is the minimal, reversible lever: capture the pristine `backgroundColor` once in `MainWindow`, set the theme color when `set`, restore the captured default when unset.
- Attribute mapping: **`WolfTitlebar` bg = titlebar background color**. **`WolfTitlebar` fg is carried through the whole stack (core → FFI) but not applied on macOS this pass** — AppKit exposes no public titlebar-text-color API with a system-drawn title; applying it is documented as deferred. GNOME titlebar: not applicable (Adw header bar keeps current behavior).
- Application point: `PaneContainerView.onRenderWake()` (it already owns the handle and runs on every wake — `PaneContainerView.swift:391`) reads `theme_titlebar_bg()`, compares against a cached `(rgb, set)` pair, and only on change calls a new `MainWindow.applyTitlebarBackground(...)` — no per-wake AppKit churn.

**D4 — Cursor attribute semantics: `WolfCursor` bg = cursor fill, fg = glyph color** (the conventional `Cursor` `guibg`/`guifg` semantics; matches what lighthouse@9da2bba defines: dark = fill `#009fce`, glyph `#000000`). Honor rule, applied as a new **first** branch in both `cursor_colors` implementations:

- If `theme.cursor.bg` is `Some(bg)` → `fill = bg`, `glyph = theme.cursor.fg.unwrap_or(cell_bg)` (the reverse-video glyph when fg is unset). Honored unconditionally — WolfCursor is an explicit app-level opt-in, unlike the mode-`attr_id` path which keeps its visibility guard.
- If `theme.cursor.bg` is `None` (group absent, cleared, or fg-only) → fall through to the **unchanged** existing logic (mode-attr honor with visibility guard, else invert the cell) — the required fallback.

**D5 — Lighthouse bump** follows build.md "To bump the vendored lighthouse colorscheme" verbatim (see Files below).

### Files to modify (no new source files except tests)

1. **`bazel/lighthouse/extensions.bzl`** — `_COMMIT` → `"9da2bba190305b2c8d5d300c254a48c487dd4a1e"`; `_SHA256` → recompute via `curl -L https://github.com/jplein/lighthouse.nvim/archive/9da2bba190305b2c8d5d300c254a48c487dd4a1e.tar.gz -o lighthouse.tar.gz && shasum -a 256 lighthouse.tar.gz`. Regenerate + commit `MODULE.bazel.lock`; re-run `bazel test //bazel/lighthouse:lighthouse_tree_test`.
2. **`runtime/nvim/init.lua`** — new global helper `_G.wolf_setup_theme_tracking(chan)` (alongside the other `_G.wolf_setup_*` helpers): defines `emit()` reading both groups via `pcall(vim.api.nvim_get_hl, 0, { name = name, link = false })` (empty table on failure), sending `vim.rpcnotify(chan, "wolf_theme", cursor_fg or -1, cursor_bg or -1, titlebar_fg or -1, titlebar_bg or -1)`; creates augroup `WolfTheme` (`clear = true`), registers a `ColorScheme` autocmd (`pattern = "*"`) calling `emit`, then calls `emit()` once immediately.
3. **`crates/wolf-core/src/render.rs`** — add `ThemeGroup` and `ThemeColors` (D2), doc-commented; add `pub theme: ThemeColors` to `Snapshot` (near `active_win`/`current_file`). `Snapshot::new` needs no change (`..Self::default()` covers it).
4. **`crates/wolf-core/src/lib.rs`** — re-export `ThemeColors`, `ThemeGroup` with the other render-state types.
5. **`crates/wolf-core/src/redraw.rs`** — `const THEME_EVENT: &str = "wolf_theme";` (doc-comment style of `CWD_EVENT`); `handle_notify` new arm `THEME_EVENT => { self.apply_theme(args).await; (self.wake)(); }` (wake after lock drop, like `CURRENT_FILE_EVENT`); new `async fn apply_theme(&self, args: Vec<Value>)`: parse `[cursor_fg, cursor_bg, titlebar_fg, titlebar_bg]` — each via `Value::as_i64`, mapped to `Some(Rgb::from_packed(v as u32))` only when `(0..=0xFF_FFFF).contains(&v)`, else `None` (missing args → `None`); then `write_async`, assign `snap.theme`, no `seq` bump.
6. **`crates/wolf-core/src/nvim.rs`** — new `pub(crate) async fn setup_theme_tracking(&self) -> Result<(), NvimError>` on `NvimProcess`, one-for-one mirror of `setup_dir_tracking` (`nvim_get_api_info` for the channel id, `nvim_exec_lua` calling `_G.wolf_setup_theme_tracking(...)`).
7. **`crates/wolf-core/src/api.rs`** — `WolfCore::start`: call `setup_theme_tracking` after `setup_dir_tracking`. No new public API — front-ends read `Snapshot.theme` via `read_snapshot()`.
8. **`crates/wolf-gnome/src/cursor.rs`** — `cursor_colors(snap)`: insert the D4 theme-honor branch first (after computing `cell_fg`/`cell_bg`, which the bg-only glyph fallback needs): `if let Some(bg) = snap.theme.cursor.bg { return (bg, snap.theme.cursor.fg.unwrap_or(cell_bg)); }`. Update doc comments.
9. **`crates/wolf-ffi/src/lib.rs`** — `cursor_colors(snap, grid)` (~line 1909): the identical theme-honor branch (keep the two implementations in lockstep; note it in both doc comments). `WolfCoreHandle`: new methods `theme_titlebar_bg(&self) -> FfiColor` / `theme_titlebar_fg(&self) -> FfiColor`, following the `current_file` pattern (`FfiColor::from_opt(core.read_snapshot().theme.titlebar.bg)`, `FfiColor::unset()` when the core is gone). Bridge module (`mod ffi`, `extern "Rust"` block ~line 288): declare the two methods, house comment style.
10. **`crates/wolf-ffi/generated/*`** — regenerate: `bazel run //crates/wolf-ffi:gen_bindings`, commit.
11. **`crates/wolf-macos/Sources/WolfMacOS/MainWindow.swift`** — new stored `private var defaultBackgroundColor: NSColor?` and `func applyTitlebarBackground(rgb: UInt32, set: Bool)`: on first themed application capture `backgroundColor`; when `set`, set from packed rgb (sRGB); when `!set`, restore captured default. Idempotent and reversible.
12. **`crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`** — in `onRenderWake()` (~line 391): read `handle.theme_titlebar_bg()`, compare to cached `lastTitlebarTheme: (rgb: UInt32, set: Bool)?`, on change call `(window as? MainWindow)?.applyTitlebarBackground(...)`. Cursor drawing untouched (colors arrive pre-resolved via `cursor_fill()`/`cursor_glyph()`).
13. **`crates/wolf-ffi/BUILD.bazel`** — new `rust_test` target `theme` modeled on `panes`/`mouse_scroll` (`srcs = ["tests/common/mod.rs", "tests/theme.rs"]`, `crate_root = "tests/theme.rs"`, same `data`/`env` nvim staging, `tags = ["local"]`).

### Test plan (real nvim; visual application is manual smoke per project policy)

1. **`crates/wolf-core/tests/theme.rs`** (new; modeled on `current_file.rs`). Against a live `WolfCore` reading `read_snapshot().theme`:
   - Startup with plain repo `init.lua` (no lighthouse staged): after attach, `theme` all-`None` (emit-once arrived, parsed as unset).
   - `:hi WolfCursor guifg=#112233 guibg=#445566`, `:hi WolfTitlebar guibg=#0a0b0c`, then `:doautocmd ColorScheme`; poll until `theme.cursor == { fg: Some(0x112233), bg: Some(0x445566) }` and `theme.titlebar == { fg: None, bg: Some(0x0a0b0c) }`.
   - Link resolution: `:hi link WolfCursor Visual` + `:doautocmd ColorScheme` → `theme.cursor.bg` equals Visual's effective bg (assert `is_some()`).
   - Cleared fallback: `:hi clear WolfCursor` + `:hi clear WolfTitlebar` + `:doautocmd ColorScheme` → back to all-`None`.
2. **`crates/wolf-ffi/tests/theme.rs`** (new; modeled on `panes.rs`):
   - Baseline after attach: `theme_titlebar_bg().set == false`, `theme_titlebar_fg().set == false`.
   - Define both groups (`guifg=#000000 guibg=#009fce` for WolfCursor, `guifg=#f1f1f1 guibg=#101010` for WolfTitlebar) + `:doautocmd ColorScheme`; poll until `theme_titlebar_bg()` is `set` with `rgb == 0x101010`; assert `theme_titlebar_fg().rgb == 0xf1f1f1`; assert `read_frame().cursor_fill().rgb == 0x009fce` and `cursor_glyph().rgb == 0x000000`.
   - Clear both + `:doautocmd ColorScheme`; poll until `theme_titlebar_bg().set == false`; assert `cursor_fill()` returned to the pre-theme reverse-video baseline.
3. **`crates/wolf-ffi/tests/frame_pack.rs`** (extend; deterministic, no nvim): hand-built `Snapshot` with `theme.cursor = ThemeGroup { fg: Some(A), bg: Some(B) }` → `cursor_fill == B`, `cursor_glyph == A`; bg-only theme → glyph falls back to cell bg; theme set **and** mode `attr_id` set → theme wins.
4. **`crates/wolf-ffi/tests/lighthouse_load.rs`** (extend; startup pickup from the real vendored colorscheme): after existing assertions, poll `theme_titlebar_bg()` until set; assert pinned dark-variant values (`titlebar bg 0x1b1b1b`, `titlebar fg 0xf1f1f1`, `cursor_fill() == 0x009fce`, `cursor_glyph() == 0x000000`) with a comment tying constants to commit `9da2bba…`. Proves the emit-once picks up a colorscheme activated *before* the RPC setup ran.
5. **`crates/wolf-gnome/src/cursor.rs` `mod tests`** (extend, headless): theme fg+bg honored as `(bg, fg)`; theme bg-only → `(bg, cell_bg)`; theme present beats an otherwise-honorable mode attr; theme absent → every existing case unchanged (do not modify existing cases).
6. **Manual smoke (macOS)**: build the `.app`; titlebar shows lighthouse's `#1b1b1b`; `:hi WolfTitlebar guibg=#803030` + `:doautocmd ColorScheme` recolors live; `:hi clear WolfTitlebar` + `:doautocmd ColorScheme` restores system titlebar; cursor is `#009fce`.

### Documentation updates (same change)

- `architecture.md`: "Theming" section gains the reverse direction (colorscheme → app via `wolf_theme` / `WolfCursor` / `WolfTitlebar`); snapshot contract list gains `Snapshot.theme`; out-of-band notification paragraph counts `wolf_theme` (wakes, no `seq` bump).
- `crates/wolf-core/src/README.md`: `render.rs` (ThemeGroup/ThemeColors, `Snapshot.theme`), `redraw.rs` (THEME_EVENT + `apply_theme`, flat 4-scalar `-1`-sentinel payload), `nvim.rs` (`setup_theme_tracking`), `api.rs` (start order), `lib.rs` re-exports.
- `crates/wolf-core/README.md`: snapshot facet enumeration.
- `runtime/nvim/README.md`: new "Theme tracking — `_G.wolf_setup_theme_tracking(chan)`" section (mirroring dir-tracking: augroup, ColorScheme autocmd, emit-once, `link = false`, `-1` sentinel); note the pinned lighthouse commit defines the two Wolf groups.
- `crates/wolf-ffi/README.md`: two new handle methods, WolfCursor note on `cursor_fill`/`cursor_glyph`, new `:theme` target, extended `lighthouse_load`.
- `crates/wolf-gnome/README.md` and `crates/wolf-macos/README.md`: cursor honor / titlebar application + revert (and that `WolfTitlebar.fg` is carried but unapplied on macOS this pass).
- `build.md`: add `//crates/wolf-ffi:theme` to the test-target list; update `lighthouse_load` description.
- `bazel/lighthouse/README.md`: update if it names the pinned commit; note the scheme now carries the Wolf groups.
- Run `prettier --write` on every touched `.md`; no hard line breaks within paragraphs.

### Implementation order

1. Lighthouse pin bump (`extensions.bzl`, `MODULE.bazel.lock`, `lighthouse_tree_test`) — independent.
2. `wolf-core`: `render.rs` types + `Snapshot.theme` → `lib.rs` re-exports → `redraw.rs` event → `init.lua` helper → `nvim.rs` setup fn → `api.rs` start call.
3. `wolf-core/tests/theme.rs`.
4. `wolf-gnome/src/cursor.rs` change + tests.
5. `wolf-ffi`: `cursor_colors` change, handle methods, bridge declarations → `bazel run //crates/wolf-ffi:gen_bindings` (commit `generated/`) → `frame_pack.rs` additions → `tests/theme.rs` + BUILD target → `lighthouse_load.rs` extension.
6. `wolf-macos`: `MainWindow.applyTitlebarBackground` → `PaneContainerView.onRenderWake` hookup.
7. Documentation + prettier.

### Dependencies / lint considerations

- **No new Cargo or Bazel dependencies** — no repin needed; `MODULE.bazel.lock` changes only from the lighthouse sha.
- Rust: doc comments on every public item, `#[must_use]` on pure accessors, no lock held across `.await` in `apply_theme` (parse args before `write_async`), bounds-checked `u32` cast from the `i64` payload values.
- Swift: no new pure seam needed; AppKit titlebar glue is compile-verified + manual smoke.

### Verification commands

```sh
# Linux / cargo devShell
cargo test -p wolf-core -p wolf-gnome
cargo test -p wolf-ffi --test frame_pack

# macOS / Bazel
bazel test //bazel/lighthouse:lighthouse_tree_test
bazel run  //crates/wolf-ffi:gen_bindings          # then commit generated/
bazel test //crates/wolf-ffi:frame_pack //crates/wolf-ffi:theme \
           //crates/wolf-ffi:lighthouse_load //crates/wolf-ffi:bridge_smoke
bazel build //crates/wolf-macos:Wolf
bazel test //crates/wolf-macos:bundle_layout_test //crates/wolf-macos:WolfKeyMapTests
```

Key file references: `runtime/nvim/init.lua`, `crates/wolf-core/src/{render.rs,redraw.rs,nvim.rs,api.rs,lib.rs}`, `crates/wolf-gnome/src/cursor.rs`, `crates/wolf-ffi/src/lib.rs` (bridge decls ~lines 288–357 and 464–499; `cursor_colors` ~line 1909; `FfiFrame::from_snapshot_grid` ~line 1759), `crates/wolf-macos/Sources/WolfMacOS/{MainWindow.swift,PaneContainerView.swift,Color.swift,WakeBridge.swift}`, `bazel/lighthouse/extensions.bzl`, `crates/wolf-ffi/tests/{lighthouse_load.rs,frame_pack.rs,panes.rs}`.
