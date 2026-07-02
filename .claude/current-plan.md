# Feature: Persist and restore application state (window geometry + Neovim cwd)

## STATUS: COMPLETE

All phases done: plan → tests-first → implementation → review (1 blocker found and fixed: writer-thread exit race in state.rs writer_loop) → follow-up drop-persistence test (10/10 runs) → docs synced + prettier → stale test comments cleaned → final fresh-run verification: state_unit, state_save, //crates/wolf-ffi:state, WolfKeyMapTests all PASS; //crates/wolf-macos:Wolf builds. Work is uncommitted on branch feat/remember-working-directory. Remaining for user: manual smoke (launch app, move/resize window, toggle sidebar, change toolbar mode, :cd, relaunch and confirm restore).

> **Format decision (user, 2026-07-02):** the state file is **pretty-printed JSON** (`state.json`), not YAML. It is machine-written only, never hand-edited; JSON avoids YAML parsing footguns while staying readable.

## Status

- [x] Problem statement written
- [x] Architect plan
- [x] Tests written
- [x] Implementation
- [x] Review (blocker fixed; fix is exactly the reviewer's prescription; verified 10/10 runs)
- [x] Documentation

## Problem statement

Implement application state persistence, first realized in the macOS front-end:

- On macOS, save application state to `~/Library/Application Support/Wolf/state.json` (pretty-printed JSON).
- State recorded **per window**:
  - the window geometry,
  - the embedded Neovim's working directory,
  - whether the file-tree sidebar is visible,
  - **macOS only**: the toolbar icon settings (icon + text, icon only, text only). The shared state schema must accommodate this platform-specific field gracefully (optional field ignored by GNOME).
- On app restart, restore the saved state.
- **Corrupt / invalid state file**: silently ignore it (start with defaults). Never crash, never surface an error.
- **Future-proof for multiple windows**: the file format holds a *list* of windows.
- **Save triggers**: any working-directory change (UI-initiated or `:cd` in embedded Neovim), plus geometry / sidebar / toolbar changes.
- **Concurrency**: all reads/writes of the state file are serialized through an orderly queue; no concurrent writes ever.
- **Shared layer**: persistence logic lives in GUI-free `wolf-core`, bridged over `wolf-ffi` for macOS, ready for future GNOME adoption.

## Architect plan (approved)

### Summary

Add a GUI-free `state` module to `wolf-core` that owns a pretty-printed JSON state file (`state.json`, multi-window list: geometry, cwd, sidebar visibility, optional toolbar display mode), loaded infallibly (any failure → `None` → defaults) and written through a single dedicated writer thread (coalescing, latest-wins queue, atomic tmp+rename writes). Core saves the cwd itself: the existing `wolf_cwd`/`DirChanged` fold point (`RedrawHandler::apply_cwd`) additionally pushes the new cwd into an attached `StateStore`, so both UI- and `:cd`-initiated changes persist with no front-end involvement. The macOS front-end restores state at launch (window frame, sidebar collapsed state, toolbar display mode, initial nvim cwd) and pushes geometry/sidebar/toolbar changes into the store over new `wolf-ffi` bridge methods; a future wolf-gnome consumes the identical `wolf_core::state` API directly.

**Key decisions:**

- **State dir resolved in core** via new `state_dir()` parallel to `preferences_dir()`: `WOLF_STATE_DIR` env override first (test hook), else macOS `dirs::data_dir()/"Wolf"` (= `~/Library/Application Support/Wolf`), else Linux `dirs::state_dir().or_else(dirs::data_dir()).map(|d| d.join("wolf"))`. `StateStore::open(dir: &Path)` takes an explicit directory (the injection seam).
- **Trigger set**: cwd change (in core, both routes); macOS: window move, end-of-live-resize, non-live resize (zoom), sidebar collapse KVO, toolbar displayMode KVO, one full snapshot push right after launch, one final push + blocking `flush` at terminate.
- **Queue mechanism**: dedicated `std::thread` writer (not tokio — must serve GNOME later) fed by a generation counter + condvar; mutations merge into an in-memory `WolfState` under a `Mutex`, bump `gen`; writer sleeps `WRITE_COALESCE_DELAY` (~100 ms), clones latest state, writes `state.json.tmp`, renames. `flush(timeout)` waits until `written_gen >= gen`.

### 1. State file schema

`<state-dir>/state.json`, pretty-printed via `serde_json::to_string_pretty`. Top-level `version` (currently `1`); `version != 1` → silently ignored. Unknown fields tolerated (no `deny_unknown_fields`); window entries beyond index 0 preserved on rewrite. All per-window fields required except `toolbar_display_mode` (optional, omitted when unset); a file missing required fields fails deserialization as a whole → silently ignored (all-or-nothing, matching `config` precedent).

```json
{
  "version": 1,
  "windows": [
    {
      "x": 128.0,
      "y": 64.0,
      "width": 1180.0,
      "height": 760.0,
      "cwd": "/Users/alice/projects/notes",
      "sidebar_visible": true,
      "toolbar_display_mode": "icon_and_text"
    }
  ]
}
```

- Geometry is `f64` points, interpreted by the writing front-end (macOS stores `NSWindow.frame`, AppKit bottom-left global coords). `width/height <= 0` = "unset" sentinel; restore treats as absent.
- `cwd` is absolute canonicalized path; `""` = "not yet reported". Restore validates non-empty existing directory before use.
- `toolbar_display_mode` ∈ `"icon_and_text" | "icon_only" | "text_only"` (serde snake_case, `skip_serializing_if = Option::is_none`, `#[serde(default)]`).
- Atomic writes (tmp + rename) → crash never leaves a partial file. Concurrent Wolf processes out of scope (last-writer-wins; note in module docs).

### 2. Files to create

**`crates/wolf-core/src/state.rs`** (new module, modeled on `config.rs`):

- `pub const STATE_FILE_NAME: &str = "state.json";` private tmp name; private `STATE_VERSION: u64 = 1`, `WRITE_COALESCE_DELAY` (~100ms), `FLUSH_TIMEOUT` (~2s).
- `pub enum ToolbarMode { IconAndText, IconOnly, TextOnly }` — Clone, Copy, Debug, PartialEq, serde snake_case.
- `pub struct WindowState { x, y, width, height: f64, cwd: String, sidebar_visible: bool, toolbar_mode: Option<ToolbarMode> }` — Clone, Debug; Default = zeros, empty cwd, `sidebar_visible: true`, `toolbar_mode: None`.
- `pub struct WolfState { pub windows: Vec<WindowState> }` — Clone, Debug, Default. Private serde wrapper `StateFile { version, windows }`.
- `pub fn state_dir() -> Option<PathBuf>` — as above, per-OS cfg like `preferences_dir()`.
- `pub fn load_state(dir: &Path) -> Option<WolfState>` — NotFound → None silently (debug log); any other read/parse/version failure → warn + None. Never writes/creates/errors.
- `pub struct StateStore` — Clone (`inner: Arc<StoreInner>`; `StoreInner { dir, state: Mutex<StateCell>, cond: Condvar }`, `StateCell { state, gen, written_gen, shutdown }`; parking_lot). API (all infallible, log-not-fail):
  - `open(dir: &Path) -> StateStore` — loads via `load_state` (defaults on None), creates dir lazily on first write, spawns named writer thread (`wolf-state-writer`).
  - `state(&self) -> WolfState` — clone of in-memory truth.
  - `set_geometry(idx, x, y, width, height)`, `set_cwd(idx, cwd: &Path)`, `set_sidebar_visible(idx, visible)`, `set_toolbar_mode(idx, mode: Option<ToolbarMode>)` — each: lock, grow `windows` with defaults up to idx, mutate, skip gen-bump when unchanged, bump gen, notify condvar.
  - `flush(timeout) -> bool` — blocks until `written_gen >= gen` or timeout.
  - Writer loop: wait on condvar for `gen > written_gen || shutdown`; sleep coalesce delay; clone state + capture target_gen under lock; drop lock; create_dir_all, to_string_pretty, write tmp, rename; re-lock, `written_gen = target_gen`, notify_all. Errors: warn, still advance written_gen (flush can't hang on read-only disk). Exit when `shutdown && written_gen >= gen`.
- `#[cfg(test)] mod tests` — see test plan.

**`crates/wolf-core/tests/state_save.rs`** — real-nvim integration test, modeled on `tests/mru_open_records.rs` (reuses `tests/common/mod.rs`).

**`crates/wolf-ffi/tests/state.rs`** — real-nvim integration test, modeled on `tests/tree_search.rs`.

**`crates/wolf-macos/Sources/WolfKeyMap/StateSeam.swift`** — pure, AppKit-free seam (Foundation/CoreGraphics only):

- `toolbarModeDiscriminant` mapping ↔ wolf-ffi `toolbar_mode` table (`ICON_AND_TEXT = 0`, `ICON_ONLY = 1`, `TEXT_ONLY = 2`).
- `restoredWindowFrame(x:y:width:height:screenVisibleFrames:) -> CGRect?` — nil when width/height non-positive or rect intersects no screen visible frame (don't restore offscreen).
- `launchCwd(restored:fallback:isDirectory:) -> String` — restored only when non-empty and `isDirectory(restored)`; else fallback. (Injected closure keeps it pure.)

**`crates/wolf-macos/Sources/WolfMacOS/StatePersistence.swift`** — AppKit glue, `final class StatePersistence: NSObject` owned by AppDelegate, weak/unowned refs to WolfCoreHandle, MainWindow, sidebar NSSplitViewItem, NSToolbar:

- `install()` — NotificationCenter observers for `NSWindow.didMoveNotification`, `didEndLiveResizeNotification`, `didResizeNotification` (skip while `inLiveResize`), KVO on sidebar `isCollapsed`, KVO on `toolbar.displayMode` (best-effort; terminate-time snapshot backstops).
- `pushSnapshot()` — geometry + sidebar visibility + toolbar mode in one go; called once after window shown at launch and from `applicationWillTerminate`.
- No file IO in Swift; only the new WolfCoreHandle setters.

**`crates/wolf-macos/Tests/WolfKeyMapTests/StateSeamTests.swift`** — headless tests over StateSeam.

### 3. Files to modify

**wolf-core:**

- `Cargo.toml` — add `serde_json = "1"` (already transitively in Cargo.lock at 1.0.150).
- `src/lib.rs` — `pub mod state;` + re-export `load_state, state_dir, StateStore, ToolbarMode, WindowState, WolfState`.
- `src/redraw.rs` — `pub type SharedStateStore = Arc<parking_lot::RwLock<Option<StateStore>>>;` next to `TrackedCwd`. `RedrawHandler` gains `state: SharedStateStore` field + fifth `new()` param. In `apply_cwd`, after storing into TrackedCwd (guard dropped), clone the `Option<StateStore>` out of a brief read guard and call `store.set_cwd(0, &path)`. No lock held across the store call.
- `src/api.rs` — `WolfCore` gains `state_store: SharedStateStore` (empty in `start()`, passed into `RedrawHandler::new`). New `pub fn attach_state_store(&self, store: StateStore)` — writes the cell, then immediately pushes current `tracked_cwd` (if Some) via `store.set_cwd(0, …)` so the setup-time `wolf_cwd` emission that raced ahead of attach is not lost.
- `BUILD.bazel` — add `state_save` rust_test target mirroring `mru_open_records` (same data/env/no-sandbox/size).

**wolf-ffi:**

- `src/lib.rs`:
  - Discriminant module `toolbar_mode { ICON_AND_TEXT: u8 = 0, ICON_ONLY = 1, TEXT_ONLY = 2 }` (single source of truth, mirrors `wolf_core::ToolbarMode` variant order; same pattern as `orientation`/`focus_dir`).
  - Bridge: opaque `FfiState { windows() -> Vec<FfiWindowState> }`; opaque `FfiWindowState { x/y/width/height() -> f64, cwd() -> String, sidebar_visible() -> bool, toolbar_mode() -> u8, toolbar_mode_set() -> bool }` (`{value,set}` pair follows FfiColor precedent).
  - Free fn `wolf_ffi_load_state() -> FfiState` — `state_dir().and_then(load_state)` defaulting to empty list; parallel to `wolf_ffi_load_config()`. Pre-start restore read; does NOT open the live store.
  - `WolfCoreHandle` methods: `state_set_geometry(x,y,width,height)`, `state_set_sidebar_visible(visible)`, `state_set_toolbar_mode(mode: u8, set: bool)` (unrecognized discriminant → logged no-op; set==false → None), `state_flush()` (blocking up to FLUSH_TIMEOUT). All hardcode window index 0 this pass (documented). No-ops when store absent.
  - Glue: `WolfCoreHandle` gains `store: Option<StateStore>`. `wolf_ffi_start` — after `WolfCore::start` succeeds — resolves `state_dir()`; if Some, `StateStore::open`, `core.attach_state_store(store.clone())`, stash store in handle (None dir just logs). `shutdown()` calls `store.flush(FLUSH_TIMEOUT)` before taking core out.
- `generated/*` — regenerate via `bazel run //crates/wolf-ffi:gen_bindings`, commit. No new wake needed.
- `BUILD.bazel` — add `state` rust_test (modeled on `tree_search`: no-sandbox, nvim + init.lua runfiles, size small).

**wolf-macos:**

- `AppDelegate.swift`:
  - `applicationDidFinishLaunching`: call `wolf_ffi_load_state()` BEFORE `startCore()`; keep first restored window.
  - `startCore()`: `launchCwd(restored:fallback:isDirectory:)` picks nvim's launch cwd.
  - After building MainWindow: restore frame via `restoredWindowFrame(…)` against `NSScreen.screens` visible frames — `setFrame(frame, display: false)` when valid, else existing `center()` path. Restore sidebar (`sidebarItem.isCollapsed = !sidebar_visible`) and toolbar displayMode BEFORE `makeKeyAndOrderFront`.
  - Create + `install()` StatePersistence, then `pushSnapshot()` once after window shown.
  - `applicationWillTerminate`: `pushSnapshot()` then `state_flush()` BEFORE `shutdown()`.
- `MainWindow.swift` — expose sidebar NSSplitViewItem + toolbar; set `toolbar.allowsDisplayModeCustomization = true` (makes the icon+text/icon-only/text-only context menu exist). Restore applied before window shown.
- `EditorSplitViewController.swift` — expose `var sidebarItem: NSSplitViewItem`.
- `BUILD.bazel` — add `StateSeam.swift` to `wolf_key_map` srcs; `StateSeamTests.swift` to `WolfKeyMapTests` srcs. (`StatePersistence.swift` picked up by existing glob in `wolf_macos_lib`.)

### 4. Save-on-cwd-change flow

- Core observes `:cd`/`:lcd`/`:tcd` in `RedrawHandler::handle_notify` → `apply_cwd` (`wolf_cwd` notification from the `WolfDir` DirChanged autocmd, also emitted once at setup). State layer hooks exactly there → `StateStore.set_cwd(0, path)`. UI-initiated cwd changes flow through nvim and hence DirChanged — one hook covers both.
- Geometry/sidebar/toolbar come from the front-end via `state_set_*` scalar setters on each change + full snapshot at launch and terminate. No full-snapshot marshalling across FFI; every mutation funnels into one mutex + single writer thread.

### 5. GNOME readiness

No GNOME code this pass. Future: wolf-gnome calls `wolf_core::state::state_dir()` + `StateStore::open`, `core.attach_state_store(store)`, pushes geometry/sidebar from GTK signals, leaves `toolbar_mode` None. One sentence in architecture.md's wolf-core section.

### 6. Test plan

1. **wolf-core unit tests** (`src/state.rs` `#[cfg(test)]`, run by `//crates/wolf-core:wolf_core_lib_test` + `cargo test -p wolf-core`), modeled on config.rs tests (tempdirs; env cases under ENV_LOCK):
   - Round-trip: open empty dir → mutate all fields (toolbar Some/None) → flush → `load_state` returns same values; output pretty-printed (contains `"\n  "`) with `"version": 1`.
   - Silently-ignore matrix: absent file → None (open yields defaults); junk bytes → None; valid JSON wrong shape → None; missing required field → None; `version: 2` → None; unknown extra fields → still parses. `load_state` never creates/modifies the file (byte-for-byte preservation asserted).
   - Multi-window preservation: hand-write 2-window file, open, `set_geometry(0, …)`, flush → window 1 survives verbatim; `set_cwd(2, …)` on 1-window state grows list with defaults.
   - "Unset" sentinel: `set_cwd` before any geometry push writes zero geometry.
   - Concurrency: N threads hammering setters + final flush → file parses cleanly, no `.tmp` residue.
   - Flush semantics: flush after mutation returns true, file reflects latest; coalescing (rapid mutations → file ends at last value).
   - `state_dir` honors `WOLF_STATE_DIR` (under env lock).
2. **`crates/wolf-core/tests/state_save.rs`** (`//crates/wolf-core:state_save`, real pinned nvim, explicit dirs, no env needed): start WolfCore in tempdir A; `StateStore::open(tempdir B)`; `attach_state_store`; poll `state.json` until window 0 cwd == canonicalized A (robust to `/var`→`/private/var`); type `:cd <tempdir C><CR>` via send_input; poll until cwd == canonicalized C; assert pretty-printing; shutdown.
3. **`crates/wolf-ffi/tests/state.rs`** (`//crates/wolf-ffi:state`, real pinned nvim): one `#[test]` (single binary → env mutation safe) setting `WOLF_STATE_DIR` to tempdir: `wolf_ffi_load_state()` on empty dir → 0 windows; corrupt `state.json` → still 0 windows; delete; `wolf_ffi_start` → `state_set_geometry/sidebar/toolbar_mode` → `:cd` keys → `state_flush()` → parse state.json, assert all five facets; `wolf_ffi_load_state()` round-trips through FfiWindowState accessors incl. toolbar_mode/toolbar_mode_set; guard discriminant contract with local consts (mouse_scroll pattern); shutdown.
4. **StateSeamTests.swift** (`//crates/wolf-macos:WolfKeyMapTests`): discriminants 0/1/2; `restoredWindowFrame` rejects non-positive sizes + fully-offscreen rects, accepts intersecting; `launchCwd` picks restored-when-directory, falls back on empty/non-directory. AppKit wiring stays compile-verified + manual smoke.
5. **Commands**: `bazel test //crates/wolf-core:wolf_core_lib_test //crates/wolf-core:state_save //crates/wolf-ffi:state //crates/wolf-macos:WolfKeyMapTests` + regression suites (`//crates/wolf-ffi:bridge_smoke`, `:config`, `:frame_pack`); `bazel build //crates/wolf-macos:Wolf`.

### 7. Implementation order

1. `wolf-core/Cargo.toml` + serde_json; `cargo check -p wolf-core`; `CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-core:wolf_core`; commit Cargo.Bazel.lock.
2. `state.rs` + lib.rs module/re-exports + unit tests.
3. Wire core: redraw.rs, api.rs.
4. `tests/state_save.rs` + Bazel target.
5. wolf-ffi: toolbar_mode mod, FfiState/FfiWindowState, wolf_ffi_load_state, handle setters/flush, start/shutdown wiring; `bazel run //crates/wolf-ffi:gen_bindings`; commit generated/.
6. `tests/state.rs` + Bazel target.
7. wolf-macos: StateSeam.swift (+tests +BUILD), StatePersistence.swift, EditorSplitViewController accessor, MainWindow toolbar exposure, AppDelegate wiring.
8. Documentation pass, `prettier --write`.

### 8. Conventions

- rustdoc on every item; `#[must_use]` on getters; `tracing::warn!` never panic on degraded paths; named consts; never hold a lock across the store call (clone-out pattern as in apply_cwd); swift-bridge `mod ffi` keeps its narrow allow; env-mutating tests behind ENV_LOCK.

### 9. Documentation updates

- `architecture.md` — wolf-core responsibilities bullet, wolf-ffi surface, wolf-macos phase status, core↔front-end contract paragraph (`wolf_cwd` also feeds the state store), GNOME integration sentence.
- `crates/wolf-core/README.md` — "Application state — `state`" section + dependency note.
- `crates/wolf-core/src/README.md` — state.rs detail; redraw/api deltas.
- `crates/wolf-core/tests/README.md` — state_save.
- `crates/wolf-ffi/README.md`, `src/README.md`, `tests/README.md` — bridge surface, discriminant table, test target.
- `crates/wolf-macos/README.md`, `Sources/README.md`, `Tests/README.md` — restore-at-launch, trigger set, StateSeam/StatePersistence, StateSeamTests.

### Risk notes for the coder

1. swift-bridge accepts `Vec<FfiWindowState>` as opaque-type method return (identical to `FfiFileSearch::children_of → Vec<FfiTreeNode>`); the free fn returns opaque `FfiState` wrapper to stay on that proven pattern.
2. `NSToolbar.displayMode` KVO is not documented-guaranteed — terminate-time `pushSnapshot()` is the correctness backstop, KVO is the eager path.
3. The `wolf_cwd` setup emission can arrive before `attach_state_store`; the attach-time push of `tracked_cwd` closes that race — do not skip it.

## Test-writer output

**Created:**

- `crates/wolf-core/tests/state_unit.rs` — headless unit coverage of the public `state` API: round-trip, unset-geometry sentinel + toolbar omission, silently-ignore matrix (junk/wrong-shape/missing-field/version-2 → None, byte-for-byte preserved), unknown-fields tolerance, multi-window preservation, grow-with-defaults, concurrency (8 threads × 50 iters, no .tmp residue), coalescing, `WOLF_STATE_DIR` under a local ENV_LOCK.
- `crates/wolf-core/tests/state_save.rs` — real-nvim e2e: attach store, poll for setup-cwd push (risk note 3), `:cd`, poll for new cwd, pretty-print assert.
- `crates/wolf-ffi/tests/state.rs` — real-nvim FFI e2e, single #[test]: WOLF_STATE_DIR; empty→0 windows (file never created); corrupt→0 windows (preserved); start; set geometry/sidebar/toolbar; `:cd`; flush; substring assertions on all five facets; load_state round-trip through FfiWindowState accessors incl. toolbar_mode/toolbar_mode_set; discriminant 99 → no-op; (0,false) clears; local-consts discriminant guard.
- `crates/wolf-macos/Tests/WolfKeyMapTests/StateSeamTests.swift` — discriminant table 0/1/2 both directions (3/99→nil); restoredWindowFrame accept/reject table; launchCwd table.

**Modified:** `crates/wolf-core/BUILD.bazel` (+`state_unit`, +`state_save` targets), `crates/wolf-ffi/BUILD.bazel` (+`state` target), `crates/wolf-macos/BUILD.bazel` (+StateSeamTests.swift in WolfKeyMapTests srcs).

**Coder must know:**

- No new dev-deps needed; tempfile reused; no serde_json in tests (load_state + substring assertions).
- Unit cases live in `tests/state_unit.rs` (module didn't exist yet); coder may fold into `src/state.rs` `#[cfg(test)]` — if folded, remove the file + `state_unit` target.
- Coder must add `Sources/WolfKeyMap/StateSeam.swift` to `wolf_key_map` srcs in wolf-macos BUILD.bazel; WolfKeyMapTests won't compile until the seam exists (expected, tests-first).
- FFI behaviors pinned by tests: unrecognized toolbar discriminant = no-op (unchanged, not cleared); `set == false` clears to None (file omits the key).
- Substring needles assume serde_json pretty float formatting (`321.0`, `87.5`, `1234.0`, `567.0`) and snake_case names.
- Assumed signatures: `StateStore::open(&Path) -> StateStore` (Clone), setters idx-first, `set_cwd(idx, &Path)`, `set_toolbar_mode(idx, Option<ToolbarMode>)`, `flush(Duration) -> bool`; pub fields; `ToolbarMode: PartialEq+Debug`; `attach_state_store(&self, StateStore)`. Swift `StateSeam.ToolbarMode` assumed UInt8-raw enum; `restoredWindowFrame(x:y:width:height:screenVisibleFrames:[CGRect]) -> CGRect?`; `launchCwd(restored:fallback:isDirectory:(String)->Bool)`.
- Harness helpers relied on (all exist): wolf-core `common::{require_nvim_bin, config_in, wake_channel, ch, key, type_str, FlushWaiter, DEFAULT_COLS/ROWS, FLUSH_TIMEOUT}`; wolf-ffi `common::{nvim_bin_or_skip, init_lua_path, DEFAULT_COLS/ROWS}`, `handle.cwd()`, `send_key()`, `shutdown()`.

## Coder output

**Implemented** (all tests green, app builds):

- wolf-core: `Cargo.toml` +serde_json (Cargo.lock + Cargo.Bazel.lock refreshed via `CARGO_BAZEL_REPIN=1`); new `src/state.rs` (full module per plan); `src/lib.rs` re-exports; `src/redraw.rs` `SharedStateStore` + `apply_cwd` mirror into store; `src/api.rs` `attach_state_store` with attach-time tracked_cwd push.
- wolf-ffi: `toolbar_mode` discriminant mod, `STATE_FLUSH_TIMEOUT` (2s), opaque `FfiState`/`FfiWindowState`, free `wolf_ffi_load_state()`, handle `store` + `state_set_geometry/state_set_sidebar_visible/state_set_toolbar_mode/state_flush`, start/shutdown wiring; regenerated committed bindings (`generated/wolf-ffi.h`, `generated/wolf-ffi.swift`).
- wolf-macos: new `Sources/WolfKeyMap/StateSeam.swift` (+ wolf_key_map srcs), new `Sources/WolfMacOS/StatePersistence.swift`, `EditorSplitViewController.sidebarItem` accessor, `MainWindow` retains split VC + exposes sidebarItem + `allowsDisplayModeCustomization = true`, `AppDelegate` restore-before-show + launchCwd + snapshot-after-show + terminate push/flush/shutdown. Bridge element refs copied into a plain `RestoredWindowState` struct (must not outlive their RustVec).

**Test results**: feature targets (`state_unit`, `state_save`, `//crates/wolf-ffi:state`, `WolfKeyMapTests`) PASSED; full regression across `//crates/wolf-core/...`, `//crates/wolf-ffi/...`, macOS — all 21 targets PASSED; `bazel build //crates/wolf-macos:Wolf` succeeds.

**Deviations**:

1. Writer-thread lifecycle: plan's `shutdown` flag had no setter; writer instead exits when an idle-poll (500ms) sees `Arc::strong_count == 1` and nothing pending; final pending write still performed.
2. `FLUSH_TIMEOUT` lives in wolf-ffi as `STATE_FLUSH_TIMEOUT` (core callers pass explicit Durations).
3. `state_flush() -> bool` (true when no store attached).
4. `tests/state_unit.rs` left in `tests/` (sanctioned option).
5. Docs untouched; note for technical-writer: `crates/wolf-core/src/README.md` still documents four-arg `RedrawHandler::new`.

**Orchestrator decision on coder's test recommendations**: no second test-writer pass — the "default-equal setter on empty list" behavior is incidental, the writer self-exit path is impractical to test non-flakily, and AppKit wiring stays compile-verified per plan.

## Review

**Verdict: FAIL — one blocker** (everything else approvable):

1. **BLOCKER** `crates/wolf-core/src/state.rs:391` — writer-thread exit race: the `Arc::strong_count == 1` exit check runs after `wait_for` returns but before the `while` condition rechecks pending work; a mutate-then-drop by the last handle colliding with an idle-poll timeout can exit the thread with an unwritten final mutation. Fix: exit only when `timed_out && strong_count == 1 && cell.generation <= cell.written`. No new test required (orchestrator already ruled the path untestable non-flakily).
2. **MINOR** — documentation pass (plan §9) still owed; `crates/wolf-core/src/README.md:86` documents four-arg `RedrawHandler::new`; no README mentions state.rs / StateSeam / StatePersistence / bridge surface / new test targets. → technical-writer.
3. **NOTES** — double-open same dir in one process = two writers racing on the tmp (unsupported config; one-sentence doc mention suffices); NaN defeats set_geometry skip-unchanged optimization (unreachable); terminate-time geometry push skipped if window already nil (harmless, eager pushes cover it).

All hard requirements verified good: silently-ignore read-only on all paths; single-writer serialized IO with atomic sibling tmp+rename, no lock across IO; schema/pretty-print pinned; both cwd routes via the single apply_cwd hook + attach-time push (duplicate absorbed by skip-unchanged); macOS restore ordering, sidebar polarity, toolbar bijection, RustVec lifetime handling, offscreen rejection all correct; wolf-core GUI-free; discriminant table consistent across all five sites; lockfiles contain serde_json.

## Blocker fix

Applied in `crates/wolf-core/src/state.rs` `writer_loop`: exit condition now `timed_out && Arc::strong_count(inner) == 1 && cell.generation <= cell.written` (pending check re-read under the lock), with a comment explaining the mutate-then-drop window. Verified: `state_unit`, `state_save`, `wolf_core_lib_test`, `//crates/wolf-ffi:state` all PASS.

**Orchestrator decision**: accept the coder's follow-up test recommendation — add `mutate_then_immediate_drop_is_persisted` to `crates/wolf-core/tests/state_unit.rs` (mutate, drop store without flush, poll `state.json` with a generous timeout until the mutation appears). Pins the drop-without-flush persistence contract the GNOME front-end will rely on.

**Added + verified**: test added (poll 50ms / 10s deadline, `paths_equal` for /var symlink robustness, tmp-residue check); `state_unit` PASSED including 10/10 under `--runs_per_test=10`; full feature+regression set 8/8 PASSED; `bazel build //crates/wolf-macos:Wolf` succeeds. No implementation changes needed.
