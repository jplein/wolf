# Plan: Empty-state launch (no process-cwd open) + MRU hygiene

## Status: COMPLETE

Empty-state launch + MRU/persistence hygiene implemented, reviewed (APPROVED, code+tests),
documented, prettier-clean (8 Markdown files no-op under prettier). Tests: `//crates/wolf-core/...`
10/10, `//crates/wolf-ffi/...` 20/20, `//crates/wolf-macos:WolfKeyMapTests` PASS,
`//crates/wolf-macos:Wolf` + `//crates/...` build OK, wolf-core clippy clean; no FFI/ABI change
(`wolf_ffi_wake.h` unchanged). Docs synced across 9 locations (init.lua/redraw/api semantics,
architecture.md launch model, both wolf-macos READMEs + Tests README + Sources README,
runtime/nvim README, build.md, BUILD.bazel state_save comment) incl. the new EmptyStateView and
StateSeam.launchDecision. Not committed (per workflow).



### Reviewer verdict: APPROVED (code + tests)
All plan points conform: init.lua initial emit flagged `true` (fires once, still tracks cwd);
apply_cwd marker decode correct (tracked-only + early return, skips store+record; genuine
change unchanged; canonicalize-before-lock intact); attach_state_store push removed (restored
folder cwd preserved via StateStore::open disk load); no FFI/ABI change (wake.h untouched);
StateSeam.launchDecision correct + process-cwd fallback gone + no stale callers; AppDelegate
threading (empty always-empty-state, restore per-decision, Cmd-N always empty); controller
sidebar rule + overlay + tree gating + idempotent didOpenFolder fired from all 3 paths + empty→
folder tree-start works via reroot→start_tree_search (no gap); EmptyStateView correct, no retain
cycle; GTK unaffected. Tests encode new behavior.

### FULL stale-docs list for technical-writer (9 locations — 3 known + 6 found by reviewer)
Known: `build.md:203,228`; `crates/wolf-macos/Tests/README.md:43` (old testLaunchCwdTable/
launchCwd); `crates/wolf-core/BUILD.bazel:151-157` (state_save header comment).
Found by reviewer: `architecture.md:70` (references REMOVED StateSeam.launchCwd; "persists on
every wolf_cwd/DirChanged" omits initial-emit skip; no empty-state/launchDecision);
`crates/wolf-core/src/README.md:114` (mirrors cwd on each wolf_cwd — omit initial-emit skip +
removed attach push); `crates/wolf-core/src/README.md:115` (records each cwd change — now
excludes flagged initial emit); `crates/wolf-macos/Sources/README.md:29` (makeNewWindow roots at
originatingCwd/home — now always empty-state; add launchDecision/empty-state/hasFolder);
`crates/wolf-macos/Sources/README.md:30` (onRenderWake unconditional tree.onRenderWake;
presentOpenFolderDialog w/o didOpenFolder; no EmptyStateView/hasFolder/sidebar-reveal);
`runtime/nvim/README.md:43,48` (DirChanged callback; add initial-emit `true` flag +
tracked-but-not-recorded/persisted semantics).

### Round 2: state_save.rs fixed, re-run GREEN
Test-writer updated `state_save.rs` (removed setup-time-persist assertion + stale
"attach-time push/risk note 3" docs; kept typed-`:cd` persistence assertion; no BUILD change).
Re-run: `//crates/wolf-core:state_save` PASS; `//crates/wolf-core/...` 10/10; `//crates/wolf-ffi/...`
20/20; `WolfKeyMapTests` PASS; `//crates/wolf-macos:Wolf` + `//crates/...` build OK.
Stale prose for doc pass: `build.md:203,228`, `crates/wolf-macos/Tests/README.md:43`,
`crates/wolf-core/BUILD.bazel:151-157` (state_save target header comment).

### Coder report (round 1)
Implemented all core (init.lua marker, redraw.rs apply_cwd is_initial skip, api.rs
attach_state_store push removal) + macOS (StateSeam.launchDecision, EmptyStateView,
WolfWindowController hasFolder/overlay/tree-gating/didOpenFolder, AppDelegate launchDecision
+ Cmd-N always-empty). NO FFI/ABI change; `wolf_ffi_wake.h` unchanged (the `recent_dirs`
delta in generated/ + lib.rs is pre-existing branch work from the earlier feature, untouched).
Results: `empty_state_cwd` PASS, `recent_dirs_records` PASS, `state_unit` PASS,
`//crates/wolf-ffi/...` 20/20 PASS, `WolfKeyMapTests` PASS, `//crates/wolf-macos:Wolf` build
PASS, `//crates/...` build PASS, wolf-core clippy clean. **ONE FAILURE: `state_save.rs::
cwd_changes_persist_to_state_json`** — it asserts the OLD setup-time-cwd-persists behavior
(line 48-49 `wait_for_state_cwd(..., &cwd_a)`) that the plan §3 deletes. Correct new behavior:
window-0 cwd stays "" until a real `:cd` (now covered by empty_state_cwd.rs). The `:cd C`
half (lines 53-63) is still valid.

### Orchestrator decision
state_save.rs encodes behavior we intentionally changed → route to test-writer to update it
(remove the setup-time-persist assertion + its stale doc referencing "attach-time push of
tracked_cwd / risk note 3"; keep the typed-`:cd` persistence assertion). Then re-run coder to
confirm green. Stale prose flagged for the doc pass: `build.md:203,228` (state_save "setup-time
cwd report") and `crates/wolf-macos/Tests/README.md:43` (old testLaunchCwdTable/launchCwd).

### Test-writer report
CREATED `crates/wolf-core/tests/empty_state_cwd.rs` (real nvim,
`launch_cwd_is_not_persisted_but_real_cd_is`: attach store + push geometry, poll live_cwd==X,
flush, assert persisted window-0 cwd=="" NOT X, then `:cd Y` → persisted cwd==Y). MODIFIED
`recent_dirs_records.rs` (pre-`:cd` MRU empty + race-free "A never present after B lands";
fixed comment), `state_unit.rs` (headless `empty_cwd_round_trips_through_store_and_load` +
asserts on-disk `"cwd": ""`), `crates/wolf-ffi/tests/state.rs` (step 11 in the single #[test]:
seed raw state.json with cwd="", `wolf_ffi_load_state()` → `w.cwd()==""`),
`StateSeamTests.swift` (replaced testLaunchCwdTable → `testLaunchDecisionTable` for
`launchDecision(restored:home:isDirectory:)`; ""/deleted/file/missing → `.emptyState(home)`,
valid dir → `.folder(restored)`, never process cwd), `crates/wolf-core/BUILD.bazel` (new
`empty_state_cwd` target mirroring `state_save`). Skipped optional
`launch_cwd_not_recorded.rs` (would duplicate). Did NOT touch `tests/common/mod.rs`.
FLAG for coder/tech-writer: `crates/wolf-macos/Tests/README.md:43` still documents old
`testLaunchCwdTable`/`launchCwd` — update in doc pass. API assumptions unchanged
(`recent_dirs()->Vec<PathBuf>`, `live_cwd()->Option<PathBuf>`, `attach_state_store`);
`WindowState.cwd` default `""` is a REQUIRED serde field (no skip_serializing_if); Swift
`enum LaunchDecision{ case emptyState(nvimCwd:String); case folder(cwd:String) }` + static
`launchDecision`, `launchCwd` removed.

## Problem statement

Wolf currently opens the process working directory at launch. When launched via `bazel run`
that's a temp/sandbox dir, which (a) is the wrong UX and (b) pollutes the global
recent-directories MRU (Open Recent fills with temp dirs). Fix both:

1. **Do not open the process working directory at launch.** When the state file is empty and
   there is no prior session to restore, show an **empty state**:
   - Sidebar hidden.
   - A button centered in the window: **"Open folder…"**.
   - Clicking it runs the normal Open Folder command (browse-for-folder dialog → user picks →
     that directory opens).
2. **Stop the launch/initial cwd from polluting the recent-dirs MRU** (the motivation): the
   MRU should reflect directories the user actually opened (via the Open Folder command or a
   typed `:cd`), not the passive launch/initial cwd.

## User decisions (locked)

- **Cmd-N (New Window):** ALWAYS opens a new empty-state window (no folder), even from a
  window that has a folder open. (Changes today's inherit-cwd behavior.)
- **Opening a folder into an empty-state window:** REVEAL the sidebar (transition empty state
  → normal open project). Empty state = sidebar hidden; once a folder opens, show the sidebar.

## Assumptions (stated; architect confirm/adjust)

- **macOS-only for the empty-state UI.** wolf-gnome is a runnable app but has NO state.json /
  sidebar / restore machinery (roots core at `std::env::current_dir()` in
  `crates/wolf-gnome/src/app.rs:143`). The empty-state overlay/sidebar behavior is macOS-only;
  the CORE MRU-hygiene fix benefits GTK too. A GTK empty-state UI is OUT OF SCOPE.
- **"No folder open" is represented by an empty cwd (`""`).** Neovim always needs a cwd (core
  requires `CoreConfig.cwd` and launches nvim there — `crates/wolf-core/src/nvim.rs:65-67`), so
  in the no-folder state the front-end launches nvim at a benign real dir (home, matching the
  existing Cmd-N fallback) while the UI tracks "no folder" separately (empty cwd in
  RestoredWindowState + a controller flag). Persist `cwd = ""` for empty-state windows; restore
  restores to empty state.

## Pre-scouted seams (verify; cite file:line in the plan)

### The launch-cwd → MRU pollution chain (root cause)
- Fresh-window cwd = process cwd: `AppDelegate.applicationDidFinishLaunching` empty branch
  (`crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift:68-78`) → `launchCwd(restoredCwd: "")`
  (AppDelegate.swift:242-252) → `StateSeam.launchCwd(restored:fallback:isDirectory:)`
  (`crates/wolf-macos/Sources/WolfKeyMap/StateSeam.swift:66-71`) returns `fallback =
  FileManager.default.currentDirectoryPath`. **This fallback is THE change point for a fresh
  window.**
- cwd flows: `WolfWindowController.startCore()` (WolfWindowController.swift:417-430) →
  `wolf_ffi_start(init_lua, cwd, cols, rows, window_index)` (lib.rs:1416-1452) → `CoreConfig`
  → `WolfCore::start` (api.rs:183-296) → nvim child cwd (NvimConfig, nvim.rs:65-67, 82, 102).
  No explicit `:cd` at startup.
- MRU record of launch cwd: `runtime/nvim/init.lua:208` initial `wolf_cwd` emit (fires once at
  setup) → `RedrawHandler::apply_cwd` (`crates/wolf-core/src/redraw.rs:378-394`), which at
  redraw.rs:392 calls `self.recent_dirs.record(&target)`. Also updates tracked cwd
  (redraw.rs) + `store.set_cwd`. **The MRU-hygiene fix must skip the RECORD for the initial
  startup emit while keeping the tracked-cwd + state mirror update.** Architect designs the
  exact mechanism (e.g. init.lua flags the initial emit distinctly, or apply_cwd skips the
  first cwd it sees per session). Benefits both front-ends.

### State / restore
- `WindowState { x,y,width,height, cwd: String, sidebar_visible: bool (default true),
  toolbar_mode }` (`crates/wolf-core/src/state.rs:84-106`); `StateFile{version,windows}`,
  `STATE_VERSION=1`, `state.json` under `state_dir()`. `load_state` infallible, absent/corrupt
  → None (state.rs:172-209). No "no folder" field yet.
- FFI: `wolf_ffi_load_state() -> FfiState` (lib.rs:961-968); `FfiWindowState` accessors incl.
  `cwd()`/`sidebar_visible()` (lib.rs:906-915). Restore: `AppDelegate.restoredWindows(from:)`
  (AppDelegate.swift:257-274) → `RestoredWindowState{...}` (WolfWindowController.swift:18-26).
  Empty branch opens ONE window at process cwd (AppDelegate.swift:68-78); restore branch one
  per saved window (AppDelegate.swift:80-95).
- An empty `cwd == ""` can be the "no folder" sentinel end-to-end (WindowState.cwd →
  FfiWindowState.cwd → RestoredWindowState.cwd). Persist via `set_cwd(idx, "")`; the writer
  already persists cwd.

### Sidebar + editor presentation (macOS)
- window `MainWindow` → `EditorSplitViewController: NSSplitViewController` → [ sidebar split
  item hosting `FileTreeView` | content split item hosting `PaneContainerView` ]
  (MainWindow.swift, EditorSplitViewController.swift:24-68). Sidebar hidden ==
  `sidebarItem.isCollapsed`; toolbar `.toggleSidebar` routes automatically. Restore applies
  `window.sidebarItem.isCollapsed = !restored.sidebarVisible` (WolfWindowController.swift:165).
- Editor = single custom `PaneContainerView` NSView (PaneContainerView.swift:36). **Overlay
  precedent:** the file `PickerView` is added as a subview of the pane container
  (WolfWindowController.swift:148-150); `PaneContainerView.onPickerTrigger`
  (PaneContainerView.swift:126) shows how an overlay's action wires back to the controller. A
  centered "Open folder…" button is a parallel subview overlay.
- Open Folder end-to-end: menu (tag OPEN_FOLDER=7) → `handleCommandMenuItem` → `CommandDispatcher`
  `.openFolder` (CommandDispatcher.swift:49-50) → `WolfWindowController.presentOpenFolderDialog()`
  (WolfWindowController.swift:255-270): NSOpenPanel → `handle?.chdir(url.path)` + `refocusEditor()`.
  The button must invoke this SAME path.

### No welcome/empty view exists today; no config for start-dir / restore-session
(`crates/wolf-core/src/config.rs` = font only). Cmd-N `makeNewWindow(originatingCwd:)`
(AppDelegate.swift:135-147) currently uses home fallback — will change to empty-state.

## Testing (real, no mocks)
- Core: a real-nvim test that the recent-dirs MRU does NOT record the initial launch cwd
  (start core at dir X, assert `recent_dirs()` empty / does not contain X until an explicit
  `:cd`/chdir; after chdir into Y, Y is recorded). Model on existing recent_dirs / state tests.
- FFI / state round-trip: `cwd == ""` persists and reloads as empty (no-folder) — model on
  the state tests. Confirm empty cwd doesn't crash the start path (nvim launched at home).
- macOS empty-state overlay / sidebar transition = compile + manual-smoke (AppKit glue), per
  conventions; extract any pure logic into a Foundation seam (e.g. StateSeam launch decision)
  and unit-test it in WolfKeyMapTests.

## Docs
architecture.md (launch/session/contract), crates/wolf-core/README.md + src/README.md (MRU
record-on-change semantics; CoreConfig.cwd), runtime/nvim/README.md (initial-emit behavior),
crates/wolf-macos/README.md + Sources/README.md (empty-state launch, button overlay, sidebar
reveal), StateSeam. Reformat edited Markdown with `prettier --write`.

## Architect plan (authoritative)

### Load-bearing refinement (deviation from draft, CONFIRMED CORRECT)
The initial `wolf_cwd` emit must skip BOTH the MRU record AND the `StateStore` write (update
the tracked-cwd cell ONLY), and `attach_state_store`'s immediate tracked-cwd push must be
REMOVED. Otherwise empty-state windows persist `home` instead of `""`. Restored folder windows
keep their cwd because `StateStore::open` already loads it from `state.json`; a later real `:cd`
updates it. This is the clean way to persist `cwd=""` without teaching core about empty state.

### CORE — MRU + persistence hygiene (benefits both front-ends)
Mechanism chosen: (a) make the initial emit DISTINGUISHABLE (stateless, provably the only
passive cwd signal — nvim's child-workdir launch fires no DirChanged; every real
`:cd`/`:lcd`/`:tcd`, incl. Open-Folder `handle.chdir`→`InputEvent::Chdir`→`:cd`, flows through
the DirChanged autocmd with NO marker). Rejected (b) "skip first per session" (a latch can't
distinguish an early real `:cd` from the initial emit).

1. **`runtime/nvim/init.lua`** `wolf_setup_dir_tracking` (~:198-209): the one-shot initial
   emit (~:208) gains a `true` arg: `vim.rpcnotify(chan, "wolf_cwd", vim.fn.getcwd(), true)`.
   The DirChanged callback (~:203-205) stays `rpcnotify(chan, "wolf_cwd", getcwd())` (no 2nd
   arg → treated false). Update doc comment.
2. **`crates/wolf-core/src/redraw.rs`** `apply_cwd` (~:378-394): decode
   `is_initial = matches!(args.get(1).and_then(Value::as_bool), Some(true))`. Always
   `*self.cwd.write() = Some(target.clone())`; if `is_initial` → `return` (skip both
   `store.set_cwd` and `recent_dirs.record`); else do the store mirror + record as today.
   Update doc.
3. **`crates/wolf-core/src/api.rs`** `attach_state_store` (~:308-313): REMOVE the immediate
   `tracked_cwd.read() → store.set_cwd(window_index, …)` push; keep
   `*self.state_store.write() = Some((store, window_index));`. Clean up any now-unused
   import/binding (no `#[allow]`). Update doc.
- NO FFI change: `cwd == ""` is pure data already round-tripping WindowState.cwd →
  FfiWindowState.cwd → RestoredWindowState.cwd. `""` never reaches `wolf_ffi_start` (front-end
  passes home). `WindowState::default().cwd == ""`.

### macOS — empty-state representation & threading
4. **`crates/wolf-macos/Sources/WolfKeyMap/StateSeam.swift`**: REMOVE
   `launchCwd(restored:fallback:isDirectory:)` and its process-cwd fallback. ADD a pure,
   testable decision:
   ```
   public enum LaunchDecision: Equatable { case emptyState(nvimCwd: String); case folder(cwd: String) }
   public static func launchDecision(restored: String, home: String, isDirectory: (String)->Bool) -> LaunchDecision
   ```
   `restored==""` OR not a dir → `.emptyState(home)`; valid dir → `.folder(restored)`. No BUILD
   change (file already in target). This replaces the `FileManager.default.currentDirectoryPath`
   fallback (the pollution source) with `home`.
5. **`AppDelegate.swift`**: remove `launchCwd(restoredCwd:)` (~:242-252); use
   `StateSeam.launchDecision(restored:home:isDirectory:)` with
   `home = FileManager.default.homeDirectoryForCurrentUser.path`. Empty branch (~:68-78): always
   `.emptyState` → controller `launchCwd=home, restored=nil, hasFolder=false`. Restore loop
   (~:80-95): decide per `restored.cwd`; `launchCwd=decision dir`, `hasFolder=(.folder)`; keep
   passing `restored` for frame/toolbar. `makeController(...)` (~:224-238): add `hasFolder: Bool`
   param. `makeNewWindow(originatingCwd:)` (~:135-147): Cmd-N ALWAYS empty — ignore originatingCwd,
   build `launchCwd=home, restored=nil, hasFolder=false`.
6. **`WolfWindowController.swift`**: add init param + stored `private var hasFolder: Bool`.
   - Sidebar in `start()` (replace ~:165): `hasFolder==false` → `sidebarItem.isCollapsed=true`
     (ignore stored sidebar_visible); `hasFolder==true` w/ restored →
     `isCollapsed = !restored.sidebarVisible` (unchanged); fresh folder → default visible.
   - Overlay: after building `paneContainer`, add `EmptyStateView` subview (mirror PickerView add
     ~:148-150), `overlay.onOpenFolder = { [weak self] in self?.presentOpenFolderDialog() }`,
     `overlay.isHidden = hasFolder`.
   - File-tree gating: only `tree.start()` (~:136) when `hasFolder`; in `onRenderWake` (~:196-207)
     only `tree?.onRenderWake()` when `hasFolder` (prevents home scan / auto-reroot). TreeWakeBridge
     already no-ops while search==nil.
   - `didOpenFolder()` (new, idempotent `guard !hasFolder`): `hasFolder=true`; hide overlay;
     uncollapse `sidebarItem`; next `onRenderWake` starts/reroots tree at the opened folder via
     existing `FileTreeView.reroot`.
   - Transition fires from TWO paths: (1) `presentOpenFolderDialog()` `.OK` (after `handle?.chdir`)
     and `changeDirectory(_:)` (Open Recent) → call `didOpenFolder()` optimistically; (2)
     nvim-initiated `:cd` in `onRenderWake`: baseline the first non-empty observed `handle.cwd()`
     as `initialCwd`; when `!hasFolder` and a later non-empty `handle.cwd() != initialCwd` → 
     `didOpenFolder()`. Use raw `handle.cwd()` strings (no canonicalization mismatch).
   - Documented accepted edge: typed `:cd` to the SAME home dir won't auto-transition (no cwd
     change); the button is the intended path and always transitions.
   - Persistence falls out: StatePersistence persists sidebar_visible (empty→false); per-window
     cwd persisted only by a real `:cd` → empty-state windows persist `cwd==""`.

### macOS — new overlay `crates/wolf-macos/Sources/WolfMacOS/EmptyStateView.swift`
Auto-included via the BUILD glob (no BUILD change). Model on `PickerView.swift`. `final class
EmptyStateView: NSView`, fills the `PaneContainerView` (pinned leading/trailing/top/bottom),
holds one centered `NSButton` "Open folder…" (center X/Y in `viewDidMoveToSuperview`), `var
onOpenFolder: (()->Void)?` invoked by an `@objc` button action, `isHidden` toggled by controller,
`init?(coder:)` → fatalError per convention. Non-first-responder so launch keystrokes reach the
editor. Same single Open Folder path (→ NSOpenPanel → chdir → refocusEditor).

### Tests (real nvim, no mocks)
- **5a MRU hygiene** — strengthen `crates/wolf-core/tests/recent_dirs_records.rs`: after startup
  (before any `:cd`) assert `recent_dirs()` does NOT contain launch cwd A (ideally empty); existing
  `:cd B`/`:cd C` still prove changes record + move-to-top. Update the now-wrong comment (lines
  57-61). Optionally new `tests/launch_cwd_not_recorded.rs` (+ BUILD) isolated guard.
- **5b persistence (load-bearing)** — new `crates/wolf-core/tests/empty_state_cwd.rs` (+ BUILD),
  model on `state_save.rs`: inject StateStore in tempdir, attach idx0, start core at X, flush,
  assert persisted window-0 `cwd == ""` (NOT X) — proves §2+§3 store suppressions; then `:cd Y`,
  flush, assert `cwd == Y`. Plus a cheap `state_unit.rs`-style serde round-trip of `cwd:""`.
- **5c FFI restore** — extend `crates/wolf-ffi/tests/state.rs` (single #[test]): a window with
  `cwd==""` loads back via `wolf_ffi_load_state()` with `w.cwd()==""`.
- **5d Swift seam** — update `crates/wolf-macos/Tests/WolfKeyMapTests/StateSeamTests.swift`:
  replace `testLaunchCwdTable` with a `launchDecision` table: `""`→`.emptyState(home)`; valid dir
  →`.folder(dir)`; deleted/plain-file→`.emptyState(home)`; assert NEVER returns process cwd.
- **5e** AppKit glue (EmptyStateView overlay, sidebar reveal, didOpenFolder, tree gating) =
  compile + manual-smoke only; all pure logic extracted into StateSeam and unit-tested (5d).

### Docs (then `prettier --write`)
README/architecture.md (empty-state launch, Open folder button, sidebar reveal on open, Cmd-N
always-empty, MRU/persisted-cwd hygiene = only user-opened dirs); `crates/wolf-core/src/README.md`
(initial emit tracks-cell-only, never mirrored/recorded; attach_state_store no longer pushes cwd);
`runtime/nvim/README.md` (flagged initial emit); `crates/wolf-macos/README.md` + `Sources/README.md`
(EmptyStateView, hasFolder flag, empty-vs-folder sidebar, StateSeam.launchDecision replacing launchCwd).

### Implementation order
1 init.lua → 2 redraw.rs → 3 api.rs → 4 core tests 5a/5b → 5 StateSeam → 6 StateSeamTests →
7 EmptyStateView → 8 WolfWindowController → 9 AppDelegate → 10 FFI test 5c → 11 docs+prettier.
Steps 5–9 (macOS) must land together to compile (launchCwd removal + new makeController/init sigs
are interdependent).

### Key risks / decisions
- Empty cwd does NOT break `WolfCore::start` (front-end passes home; `""` is only UI/persist data).
- Initial-emit suppression does NOT suppress Open-Folder chdir record (that's a DirChanged, unmarked).
- Restore of `cwd==""` → `.emptyState(home)` → hasFolder=false. Folder windows 100% unchanged
  (sidebar uses stored sidebar_visible, tree starts at X, `:cd` records/persists as today; only the
  redundant passive double-write is gone).
- FileSearch-on-home avoided by gating tree.start()/onRenderWake on hasFolder; downgrade to
  follow-up only if the wake lifecycle proves more entangled (TreeWakeBridge no-op-when-closed
  makes the two gates sufficient).
- Transition can't rely on runtime `cwd==""` (nvim reports home) → explicit `hasFolder` flag +
  the two transition paths.

### Dependencies: NONE. No new packages, no FFI/ABI change.
