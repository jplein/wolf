# Plan: Multiple-window support in Wolf (macOS front-end) + Command-N new window

## Status: COMPLETE

- Implemented per plan. All tests green: 16 wolf-ffi (incl. new `multi_window` driving TWO real nvim instances), 6 wolf-core, `WolfKeyMapTests` (incl. new `ShortcutSeamTests`/`WindowIndexSeamTests`), `bundle_layout_test`; `bazel build //crates/wolf-macos:Wolf` succeeds. swift-bridge `generated/` regenerated.
- Reviewer: APPROVED with minor notes — fixed 3 stale "window 0" doc comments (lib.rs:357/359/361). Accepted as documented v1 limitation: terminate-time cleanup truncates to `highestLiveIndex+1` (not dense re-index), so a closed MIDDLE window's slot survives a clean quit and restores as a stale empty window next launch (bounded by session concurrent-window count; no data loss). Documented in wolf-macos/README.md + wolf-ffi/README.md.
- Docs verified/updated (architecture.md, wolf-core/README.md + src, wolf-ffi/README.md + src + tests, wolf-macos/README.md + Sources + Tests); prettier clean.

## Post-ship bugfix: crash when closing a non-last window

Crash report (`~/Dustbin/crash-report.txt`): `EXC_BAD_ACCESS` on main thread in
`-[_NSWindowTransformAnimation dealloc]` → `objc_release` during a deferred CA transaction
commit — an over-release of an ARC-owned `NSWindow`. Root cause: `NSWindow.isReleasedWhenClosed`
defaults to `true`, so AppKit adds an unbalanced release on close while `WolfWindowController`
also holds a strong ARC ref. Single-window build never hit it (closing the only window quit the
process first). Rust/wake path NOT involved (tokio threads were just parked cores).

Fix (two parts):
1. `MainWindow.swift` — set `isReleasedWhenClosed = false` in the designated init so ARC (the
   owning controller) is the sole owner.
2. `WolfWindowController.teardown()` — defer `coordinator?.removeController(self)` to the next
   main-runloop turn via `DispatchQueue.main.async`. Synchronously dropping the controller (last
   strong ref) during `windowWillClose` would deallocate the window mid-close (its close
   animation captures the window) → UAF. Deferring keeps the window alive until AppKit's close
   transaction finishes.

`bazel build //crates/wolf-macos:Wolf` succeeds. Docs updated (wolf-macos/README.md MainWindow +
WolfWindowController bullets); prettier clean. Not automatically testable (no headless window
server; AppKit glue is manual-smoke per the documented coverage boundary) — user to verify by
closing a non-last window.

## Problem statement

The macOS shell is currently single-window. We need to support multiple top-level OS
windows, each backed by its own embedded Neovim (its own `WolfCore` / `WolfCoreHandle`),
plus a Command-N keyboard shortcut that opens a new window.

### Known constraints (from prior investigation; documentation is source of truth)

The core (`wolf-core`) is already per-instance and multi-window-friendly: no process-global
editor state; snapshot is a per-core `RwLock`; `WolfCoreHandle` is 1:1 with a `WolfCore` and
not a singleton; `wolf_ffi_start` already returns a fresh handle per call.

**The one genuine architectural blocker is the wake-routing layer.** `wolf-ffi` stores the
`WakeFn` in three PROCESS-GLOBAL single-slot pointers:
- `crates/wolf-ffi/src/lib.rs:564` `WAKE_FN` (editor redraw)
- `crates/wolf-ffi/src/lib.rs:603` `FILESEARCH_WAKE_FN` (picker)
- `crates/wolf-ffi/src/lib.rs:644` `TREE_WAKE_FN` (sidebar)

Swift side resolves these to single statics (`WakeBridge.swift:22,28`;
`PickerWakeBridge.swift:19`). Two windows/cores would clobber each other. Must become
per-core/per-window routed.

**Mechanical plumbing:**
- State indexing: schema is already `WolfState { windows: Vec<WindowState> }`
  (`crates/wolf-core/src/state.rs:125`) but writers hardcode index 0
  (`wolf-ffi/src/lib.rs:1168/1176/1192/1204`, `wolf-core/src/redraw.rs:346`, `api.rs:269`).
- Shared StateStore: opening same directory twice races writer threads
  (`state.rs:254`, `wolf-core/README.md:120`). Each handle opens its own store today.
  Move to one process-wide store, cloned per window.
- AppKit shell lifecycle: `AppDelegate.swift:20-24` holds single properties;
  `applicationShouldTerminateAfterLastWindowClosed` returns true (:136). Promote into a
  per-window controller held in a collection; decouple teardown from app termination.

**New feature:** Command-N opens a new window (no menubar yet, so wire via key handling).
New window's cwd: pick a reasonable default (e.g. frontmost window's cwd, or home dir).

### Requirements
- Follow CLAUDE.md: documentation is source of truth; each source directory must have a
  README.md describing interfaces/functions; update READMEs to match.
- Prefer end-to-end/integration tests that actually start a Neovim process over mocks.
- Verify the implementation and run tests.

## Architect's plan

### 1. Wake-routing redesign (central change) — mechanism: per-window `u64` context token

The wake fires on a tokio worker thread and must not touch the `!Send` `WolfCoreHandle`, so
identity crossing the boundary is a plain `Send` scalar `u64` token, minted in `wolf_ffi_start`,
captured by the core's `WakeFn` closure, passed as sole arg to the C callback. Swift registers
one thunk per channel that looks the token up in a main-thread `WindowRegistry`.

**Rust `crates/wolf-ffi/src/lib.rs`:**
- Callback ABI `extern "C" fn()` → `extern "C" fn(u64)`. `type WakeCallback = extern "C" fn(u64);` (replaces `fn()` at ~line 560).
- Add `static NEXT_WAKE_TOKEN: AtomicU64 = AtomicU64::new(1);`.
- Three globals `WAKE_FN`/`FILESEARCH_WAKE_FN`/`TREE_WAKE_FN` (564/603/644) stay `AtomicPtr<()>`, now hold `fn(u64)`.
- `wolf_ffi_set_wake`/`_filesearch_wake`/`_tree_wake` (579/618/660) param → `Option<WakeCallback>`; body unchanged.
- Rewrite `registered_wake(token)`/`registered_filesearch_wake(token)`/`registered_tree_wake(token)` to capture the token and pass it to `f(token)`.
- `WolfCoreHandle` gains `wake_token: u64`; add `pub fn wake_token(&self) -> u64` (`#[must_use]`) + `mod ffi` decl.
- `wolf_ffi_start` mints token via `fetch_add`, passes `registered_wake(token)` to `WolfCore::start`, stores token.
- `start_file_search`/`start_tree_search` (1058/1076) pass `registered_filesearch_wake(self.wake_token)` / `_tree_wake(self.wake_token)`.

**C header `crates/wolf-ffi/generated/wolf_ffi_wake.h` (hand-written):** all three decls carry `uint64_t token`; add `#include <stdint.h>`.

**Swift bridges** (`WakeBridge.swift`, `PickerWakeBridge.swift`, `TreeWakeBridge.swift`): remove `registered*` statics; each thunk takes `token`, dispatches to main, resolves `WindowRegistry.shared.controller(for: token)?.{onRenderWake()|picker?.refresh()|tree?.refresh()}`. Install once at launch; closures capture only the global registry (stay `@convention(c)`).

**New `Sources/WolfMacOS/WindowRegistry.swift`:** `final class WindowRegistry` singleton, `[UInt64: WolfWindowController]`, main-thread only (no lock). `register/unregister/controller(for:)`.

### 2. State indexing

Window index = slot in `state.json`'s `windows` list, assigned by front-end. Restore: one
controller per saved entry, `windowIndex = position`. New (Cmd-N) window: first free slot via
`WindowIndexSeam.nextFreeIndex(occupied:)`. Fixed for window's life; passed into `wolf_ffi_start`.

- FFI: `wolf_ffi_start(init_lua, cwd, cols, rows, window_index: u16)`; `WolfCoreHandle.window_index: usize`; `state_set_geometry/_sidebar_visible/_toolbar_mode` (1166/1174/1187) use `self.window_index`; `attach_state_store(store.clone(), window_index as usize)`.
- Core cwd persistence (without touching `CoreConfig` — wolf-gnome depends on it and does NOT call attach_state_store):
  - `crates/wolf-core/src/redraw.rs:56` alias → `pub type SharedStateStore = Arc<parking_lot::RwLock<Option<(StateStore, usize)>>>;`
  - `apply_cwd` (~342-348, `store.set_cwd(0,…)` at 346) reads tuple: `if let Some((store, idx)) = self.state.read().clone() { store.set_cwd(idx, &target); }`.
  - `crates/wolf-core/src/api.rs` `attach_state_store(&self, store: StateStore, window_index: usize)` (276): store `Some((store.clone(), window_index))`; immediate push → `store.set_cwd(window_index, &cwd)` (280). Field init at 182 unchanged.
- Save: each window pushes its facets at its slot; `StateStore::mutate` already grows+coalesces. Restore: one window per saved entry.
- Ghost cleanup: add `StateStore::truncate_windows(&self, len: usize)` in `state.rs` (mutate-style, bumps generation); expose as `WolfCoreHandle::state_set_window_count(&self, count: u16)`. At `applicationWillTerminate` re-push live windows at dense indices + call once. v1 limitation to DOCUMENT: closing a non-last window leaves its slot until terminate-time truncation; crash-before-terminate could restore one stale window.

### 3. Shared StateStore

`wolf_ffi_start` currently `StateStore::open` per call → racing writer threads (unsupported).
Fix in `lib.rs`: `static SHARED_STORE: OnceLock<Option<StateStore>> = OnceLock::new();` +
`fn shared_store() -> Option<StateStore>` that `get_or_init(|| state_dir().map(StateStore::open)).clone()`.
`wolf_ffi_start` replaces the `match state_dir()` block (1241-1253) with `let store = shared_store();`,
then `attach_state_store(store.clone(), window_index)` + stash in handle; log-and-disable when `None`.
Realizes the "attach one store and clone it" guidance in `wolf-core/README.md:120`.

### 4. AppKit shell: per-window controller

**New `Sources/WolfMacOS/WolfWindowController.swift`:** `final class WolfWindowController: NSObject,
NSWindowDelegate` owning one window's stack (moved out of AppDelegate): `handle`, `window`,
`paneContainer`, `tree`, `picker`, `statePersistence`, `windowIndex`, `wakeToken`.
- `init(windowIndex:launchCwd:restored:metrics:config:)` — metrics/config measured once by app, shared.
- `start()` — body of old `applicationDidFinishLaunching` (53-133) minus once-only font/config: set `WOLF_NVIM_BIN`, resolve init.lua, `wolf_ffi_start(…, UInt16(windowIndex))`, read `wake_token()`, `WindowRegistry.register(self, token:)`, build pane/tree/picker/window, restore frame/sidebar/toolbar only when `restored != nil`, install StatePersistence, `window.delegate = self`, show + first-responder. Sets `paneContainer.onNewWindow = { [weak self] in self?.requestNewWindow() }`.
- `onRenderWake()` — `paneContainer.onRenderWake()` then `tree.onRenderWake()`.
- `windowWillClose(_:)` → `teardown()`.
- `teardown()` — old `applicationWillTerminate` body (140-155) scoped to this window: push final snapshot, `state_flush`, `tree.close()`, `picker.close()`, `handle.shutdown()`, `WindowRegistry.unregister(token:)`, ask AppDelegate to drop from collection.

**Modify `AppDelegate.swift`** → app coordinator: keep once-only setup (3× `*Bridge.install()`, font
registration, `wolf_ffi_load_config()`, `CellMetrics.measured`). Hold `private var controllers:
[WolfWindowController] = []` (drop single window/handle/picker/tree/statePersistence fields).
`applicationDidFinishLaunching` maps EVERY saved window into `RestoredWindowState`, one controller
per entry (else one default at index 0); startup-failure alert only if FIRST window's core fails.
`makeNewWindow(originatingCwd:)` = Cmd-N entry: `nextFreeIndex(occupied: controllers.map(\.windowIndex))`,
resolve cwd, create+`start()` controller with `restored: nil` (cascade placement), append.
`removeController(_:)` drops from collection. `applicationWillTerminate` dense re-index +
`state_set_window_count(controllers.count)` then teardown remaining (backstop).
`applicationShouldTerminateAfterLastWindowClosed` stays `true`.

### 5. Command-N

- **New seam `Sources/WolfKeyMap/ShortcutSeam.swift`** (Foundation-only): `public enum ShortcutSeam { public static func isNewWindowTrigger(charactersIgnoringModifiers: String, command: Bool) -> Bool }` — same shape as `PickerSeam.isPickerTrigger`, matches `"n"`.
- **New pure seam `Sources/WolfKeyMap/WindowIndexSeam.swift`:** `public enum WindowIndexSeam { public static func nextFreeIndex(occupied: [Int]) -> Int }` — smallest non-negative int not in `occupied`.
- **Call site `Sources/WolfMacOS/PaneContainerView.swift`:** add `var onNewWindow: (() -> Void)?` (mirror `onPickerTrigger` ~124); in `keyDown` (210), before the picker block, if `ShortcutSeam.isNewWindowTrigger(charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "", command: flags.contains(.command)) { onNewWindow?(); return }`.
- **cwd choice (STATED):** new window opens in originating window's live Neovim cwd (`self.handle.cwd()`), falling back to `FileManager.default.homeDirectoryForCurrentUser.path` when empty.

### 6. Files to create / modify

**Create:** `WolfMacOS/WolfWindowController.swift`, `WolfMacOS/WindowRegistry.swift`,
`WolfKeyMap/ShortcutSeam.swift`, `WolfKeyMap/WindowIndexSeam.swift`,
`Tests/WolfKeyMapTests/ShortcutSeamTests.swift`, `Tests/WolfKeyMapTests/WindowIndexSeamTests.swift`,
`crates/wolf-ffi/tests/multi_window.rs`.

**Modify:** `wolf-ffi/src/lib.rs`, `wolf-ffi/generated/wolf_ffi_wake.h`, `wolf-ffi/generated/*` (regen),
`wolf-core/src/redraw.rs`, `wolf-core/src/api.rs`, `wolf-core/src/state.rs`,
`WolfMacOS/AppDelegate.swift`, `WolfMacOS/WakeBridge.swift`, `WolfMacOS/PickerWakeBridge.swift`,
`WolfMacOS/TreeWakeBridge.swift`, `WolfMacOS/PaneContainerView.swift`,
`crates/wolf-macos/BUILD.bazel` (add 2 seams to `wolf_key_map` srcs + 2 tests to `WolfKeyMapTests`),
`crates/wolf-ffi/BUILD.bazel` (add `multi_window` rust_test, modeled on `state`, no-sandbox, stage nvim+init.lua).

### 7. Test plan

**Rust FFI integration (real pinned nvim, no mocks) — `crates/wolf-ffi/tests/multi_window.rs`** — single
`#[test]` (sets process-global `WOLF_STATE_DIR` like `tests/state.rs`; reuses `tests/common/mod.rs`):
- Point `WOLF_STATE_DIR` at tempdir. Start TWO cores: `wolf_ffi_start(initLua, cwdA, 80,24, 0)` and `(…, cwdB, 80,24, 1)` distinct cwds.
- **Distinct identity:** `hA.wake_token() != hB.wake_token()`.
- **Independent snapshots:** send_key into A; `hA.read_frame()` reflects it while `hB.read_frame()` does not.
- **Wake routing not clobbered:** register test `extern "C" fn(u64)` recording counts into `static Mutex<HashMap<u64,u32>>`; drive input per core; assert each core's token increments (A never bumps B).
- **Shared store + indexing:** set geometry/sidebar on each; `:cd` into B; `state_flush`; `load_state().windows()` has TWO entries with correct per-index geometry/sidebar and B's cwd at index 1.
- **Truncate:** `state_set_window_count(1)` → `load_state` yields one window.
- Regression: `//crates/wolf-ffi:state` still passes; bridge_smoke/panes/file_search/tree_search unchanged.

**Swift pure seams (headless `WolfKeyMapTests`):**
- `ShortcutSeamTests.swift`: `command+"n"`→true, `command+"N"`→true, `"n"` no command→false, `command+"o"`→false, empty→false.
- `WindowIndexSeamTests.swift`: `[]`→0, `[0,1,2]`→3, `[0,2]`→1 (hole reuse), `[1]`→0.

AppKit glue (`WolfWindowController`, `WindowRegistry`, bridge thunks, `AppDelegate` collection,
`onNewWindow`) stays compile-verified + manual-smoke only (no headless window server). Add eyeball
checklist to `wolf-macos/README.md`: Cmd-N opens second window in same cwd; typing/splits/picker/tree
independent per window; each persists/restores own geometry+sidebar; closing one leaves other live;
closing last quits.

### 8. Docs to update (reformat each with `prettier --write`, no hard line breaks in paragraphs)

`README.md` (macOS summary → multi-window); `architecture.md` (macOS phase status, "The wake" token,
state per-window index); `crates/wolf-core/README.md` (state module N windows, `attach_state_store`
window_index param, shared-store guidance realized); `crates/wolf-ffi/README.md` (3 wake sections token,
`wolf_ffi_start` +window_index, `wake_token()`, `state_set_*` uses handle index, `state_set_window_count`,
add `//crates/wolf-ffi:multi_window`); `crates/wolf-macos/README.md` (new sources, coordinator, token
lookup, seams, Cmd-N + cwd, flip "Single window only", eyeball checklist); `crates/wolf-core/src/README.md`
(if it documents `SharedStateStore`/wake type).

### 9. Implementation order (no broken intermediate compiles)

1. **wolf-core** — redraw.rs alias + apply_cwd; api.rs attach_state_store(store, idx); state.rs truncate_windows. (Coder: grep-confirm wolf-gnome doesn't call attach_state_store.)
2. **wolf-ffi lib.rs** — token ABI + NEXT_WAKE_TOKEN + registered_*_wake(token); wake_token(); window_index field + state_set_*; wolf_ffi_start(+window_index) + shared_store(); attach_state_store(store, idx); state_set_window_count. Update mod ffi decls.
3. **wolf_ffi_wake.h** — hand-edit token signatures.
4. **Regenerate** — `bazel run //crates/wolf-ffi:gen_bindings`; commit generated/.
5. **WolfKeyMap** — ShortcutSeam.swift, WindowIndexSeam.swift; update wolf_key_map srcs.
6. **WolfMacOS** — WindowRegistry.swift → WolfWindowController.swift → rewrite 3 *WakeBridge thunks → PaneContainerView (onNewWindow + keyDown) → slim AppDelegate.swift.
7. **BUILD** — multi_window (wolf-ffi) + 2 seam tests (WolfKeyMapTests).
8. **Tests** — multi_window.rs, ShortcutSeamTests.swift, WindowIndexSeamTests.swift.
9. **READMEs.**

### 10. Notes
- No new external packages (std `AtomicU64`/`OnceLock` only; no Swift frameworks). No `CARGO_BAZEL_REPIN`. wolf-core GUI denylist unaffected.
- swift-bridge codegen MUST be re-run (wolf_ffi_start signature changed; wake_token/state_set_window_count added); commit `generated/`. `wolf_ffi_wake.h` is hand-written — edit directly, gen_bindings won't regenerate it.
- Wake `transmute` keeps existing `// SAFETY:`; new closures capture only `u64` (Send+Sync preserved). Mark `wake_token` `#[must_use]`.

### 11. Verification commands (Bazel)
- `bazel test //crates/wolf-ffi:multi_window` (new)
- `bazel test //crates/wolf-ffi:state` (single-window regression)
- `bazel test //crates/wolf-ffi:bridge_smoke //crates/wolf-ffi:panes //crates/wolf-ffi:file_search //crates/wolf-ffi:tree_search`
- `bazel test //crates/wolf-macos:WolfKeyMapTests`
- `bazel build //crates/wolf-macos:Wolf`
- `bazel test //crates/wolf-macos:bundle_layout_test`
- `bazel run //crates/wolf-ffi:gen_bindings` (refresh + commit)
- `cargo test -p wolf-core` (Linux devShell; confirms CoreConfig unchanged / GTK build compiles)
