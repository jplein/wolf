# Current Plan: Per-window tabs (native Tahoe tab bar)

## Status: COMPLETE ✅

## Technical-writer + finalization
Docs verified/synced against the real implementation: `wolf-ffi/README.md` (command_kind table
→ NewTab=17, `:commands` 0..=17), `wolf-macos/Sources/README.md` (WolfKeyMap count 19→18),
`wolf-macos/Tests/README.md` (CommandSeamTests 0..17 / boundary 18; `:state` headless),
`architecture.md` (group-by-group restore, coordinator-owned whole-tree snapshot, StatePersistence
trigger-only, cwd read live, terminate `wolf_ffi_state_flush`, palette count 17),
`wolf-core/tests/README.md` (state_unit tree API, state_save retired, command count 18), `build.md`
(dropped the deleted `state_save` target line). Reviewer nit fixed: the two stale
`crates/wolf-core/BUILD.bazel` comments no longer reference the retired `state_save` target.
Orchestrator ran `prettier --write` on all touched Markdown → clean (only `wolf-ffi/README.md`
reflowed). No README still describes a removed symbol (WindowTitleSeam/WindowIndexSeam/state_set_*/
window_index/attach_state_store/old flat state.json). Technical-writer agent id: ab07cd5189f5cec07.

## Outstanding (not blockers, surfaced to user)
- Manual GUI smoke (needs a window server — cannot be automated): ⌘T opens a tab inheriting cwd;
  native "+" opens a tab; tear-off → standalone window; drag-merge re-groups; reorder persists;
  each tab label = active-buffer basename with disambiguation; restore reopens groups with correct
  geometry + per-tab sidebar; terminate writes the tree; old flat state.json is clobbered.
- Pre-existing `//crates/wolf-ffi:buffer_picker` 1s-`lastused` timing flake (unrelated to tabs;
  passes on rerun) — recommend a separate de-flake pass.

## Reviewer verdict: APPROVED
All 15 invariants verified (state tree / no-serde-default / set_snapshot no-churn / api-redraw /
append-only lockstep / ffi surface / TabTitleSeam untitled-excluded + terminating growth loop /
snapshot walker dedupe + solo-group / makeNewTab / tabId replaces windowIndex / coalesced
snapshot triggers + terminate flush / titles + WindowTitleSeam removed / weak-self KVO no cycles).
wolf-gnome UNAFFECTED (no StateStore/WolfState use; no exhaustive Command match). No blockers.
**Nit (→ technical-writer):** `crates/wolf-core/BUILD.bazel` lines ~309 and ~404 have stale
comments calling `theme`/`recent_dirs_records` "an EXACT mirror of the `state_save` target",
which was retired — re-point them at a surviving sibling. Reviewer agent id: af84c107daf61a13b.

## Coder results
Implemented all 17 steps. `bazel test //crates/wolf-core/... //crates/wolf-ffi/...` = 39/40
PASS; the single FAIL (`//crates/wolf-ffi:buffer_picker`) is a PRE-EXISTING 1s-`lastused`
timing flake unrelated to this feature (passed 3/3 on `--runs_per_test=3`; our changes don't
touch it). `bazel build //crates/wolf-macos:Wolf` PASS (all AppKit tab wiring compiles).
`WolfKeyMapTests` (incl. new `TabTitleSeamTests` + updated `CommandSeamTests`) PASS. Bindings
regenerated via `bazel run //crates/wolf-ffi:gen_bindings`; `wolf_ffi_wake.h` byte-identical.
wolf-core GUI-free denylist clean. `handle.current_file()` confirmed present (no rename).
swift-bridge handled `Vec<FfiTabState>` + `String` free-fn args with no limitation. Coder also
pre-updated the docs (technical-writer will verify). NEW file `Sources/WolfMacOS/TabGroupObserver.swift`.

**Orchestrator decision:** the `buffer_picker` flake is pre-existing + out of scope for this
feature → NOT routing a test-writer pass for it; surface to user as separate cleanup. The
post-coder SourceKit diagnostics (missing WolfFFI/WolfKeyMap modules, unresolved AppDelegate/
TabGroupObserver) are false positives (Bazel-only resolution); `bazel build //crates/wolf-macos:Wolf`
passing is authoritative. Coder agent id: a6a9b329bfb29df37.

## Test-writer results + orchestrator reconciliation of its flags

Test files written/rewritten: `state_unit.rs` (tree), `palette_rank.rs` (16→17 + NewTab row),
`command.rs` inline (count 18, NEW_TAB_INDEX=17, buffer_picker fixed at 16), `state.rs`
(ffi builder round-trip, headless — nvim deps dropped), `commands.rs` (0..=17 + `:WolfNewTab`
drain), `multi_window.rs` (wake-token only), `TabTitleSeamTests.swift` (new), `CommandSeamTests`
(ffi:17→.newTab, out-of-range 18). Blanked-to-stub (coder must `git rm`): `state_save.rs`,
`WindowTitleSeamTests.swift`, `WindowIndexSeamTests.swift`. Trimmed `empty_state_cwd.rs` to
recent-dirs only. **Dropped the `window_index` arg from `wolf_ffi_start` in 21 ffi test files**
(all call sites) — tests now expect the 4-arg `wolf_ffi_start(init_lua, cwd, cols, rows)`.

Reconciliation of the test-writer's flags (BINDING for the coder):
- **TW1/TW2 — `TabTitleSeam` "untitled" handling: CONFIRMED.** Only `""` / `[No Name]` /
  unnamed → `"untitled"`; a real path always keeps its basename (a bare distinct `"file.md"`
  stays `"file.md"`). **`"untitled"` entries are EXCLUDED from the collision/disambiguation
  pass** — two unnamed tabs both stay `"untitled"` (paths can't disambiguate the pathless).
  Coder implements exactly this.
- **TW3 — `empty_state_cwd.rs`: ACCEPT the trimmed version as-is** (valid recent-dirs behavior;
  a slight overlap with `recent_dirs_records.rs` is harmless — do NOT retire it).
- **TW4 — 21-file `wolf_ffi_start` arity update: APPROVED** (necessary; all test files).
- **TW5 — coder wiring the tests depend on:** add `Sources/WolfKeyMap/TabTitleSeam.swift` to the
  `wolf_key_map` library `srcs`; remove `WindowIndexSeam.swift` + `WindowTitleSeam.swift` from it
  and DELETE those two source files; `git rm` the four blanked files
  (`state_save.rs`, `WindowTitleSeamTests.swift`, `WindowIndexSeamTests.swift` + the retired
  `state_save` stanza already removed).
- The `XCTest` SourceKit diagnostics are false positives (Bazel-only test bundle); ignore.

Test-writer agent id: a0412790b8d5d2af1

## Orchestrator decisions on the architect's open questions (BINDING)

- **OD1 — Approve removing core-side cwd persistence.** With the whole-tree snapshot
  model, cwd is read from each controller at snapshot time; `attach_state_store` and the
  `redraw.rs::apply_cwd` StateStore mirror are removed. The writer thread / coalescing /
  atomic-rename stay intact — only the *source* of the tree changes (front-end snapshot
  vs core mirror). Consequently: **retire** `crates/wolf-core/tests/state_save.rs`;
  **trim** `crates/wolf-core/tests/empty_state_cwd.rs` to its recent-dirs assertions only.
- **OD2 — Approve deleting `WindowTitleSeam` AND `WindowIndexSeam`** (both dead) and their
  tests. The OS window/tab title now derives from the active buffer via `TabTitleSeam`
  (`"untitled"` for an empty-state tab). A single-tab window's titlebar therefore shows
  the active buffer basename instead of the home-abbreviated cwd — consistent with the
  spec ("tab title = active buffer basename"). Monotonic `tabId` replaces `WindowIndexSeam`.
- **OD3 — Keep the `//crates/wolf-ffi:state` target name** (no rename to `state_roundtrip`);
  rewrite its sources in place.
- **OD4 — NewTab IS a command-palette row** (palette iterates all specs except
  CommandPalette): `palette_rank.rs` `STATIC_COMMAND_COUNT` 16→17.
- **OD5 — `TabTitleSeam` format = `"<basename> — <minimal trailing parent segments>"`**
  (em dash U+2014), only on colliding labels; empty/unnamed → `"untitled"`. Approved.
- **OD6 — `newWindowForTab(_:)` lives on `AppDelegate`** (NSApp→delegate forwarding for the
  native "+"); the explicit ⌘T menu item routes through `handleCommandMenuItem → .newTab`.
  If smoke shows the "+" doesn't reach the delegate, add a `MainWindow` override as fallback.

None of these require re-consulting the user — they are correct consequences of the
locked decisions.

## Problem statement

Add per-window tabs to the macOS app (`wolf-macos`), using the native macOS Tahoe
titlebar tab bar (`NSWindowTabGroup`), like Finder/Safari.

### Locked decisions (from the user)

- Each tab = its own `WolfWindowController` + its own captive Neovim core + its own
  buffer list, splits, scroll, sidebar, cwd. New tab ≈ new window in the same directory.
- Tear-off into a new window and dragging a tab onto another window are ALLOWED (native
  default; no suppression work). Drag-to-reorder is native/free.
- Tab title = active buffer's basename (not full path), with minimal disambiguation when
  basenames collide across tabs in the same window (e.g. two `README.md`).
- `state.json` becomes a TREE: `windows[]` each with geometry (x,y,width,height) +
  `tabs[]`, each tab `{ cwd, sidebar_visible }`. NO migration path: unparseable in the
  new format → ignore and clobber on exit (version stays 1; missing `tabs` naturally
  fails deserialization).
- New Tab is a FULL Wolf command (`command_kind == 17`, after `bufferPicker == 16`):
  `COMMAND_SPECS` entry (`:WolfNewTab` / `require('wolf').new_tab()`), FFI discriminant,
  `CommandSeam.CommandKind.newTab`, `CommandDispatcher .newTab` branch, `AppMenu` ⌘T item
  routed through `handleCommandMenuItem`. Keep New Window at ⇧⌘N.

### Constraints (CLAUDE.md)

- Documentation is the source of truth: keep every touched directory's README in sync;
  treat any doc/impl divergence as a bug.
- Prefer end-to-end/integration tests with a REAL Neovim process over mocks.
- Reformat edited Markdown with `prettier --write`; no hard line breaks within a paragraph.
- `wolf-core` must compile with NO GUI toolkit in its dependency tree.
- APPEND-ONLY discriminant contract for `Command` / `command_kind` (currently `0..=16`;
  `NewTab` = 17).

## Orchestrator's handoff summary (5 phases — architect to expand/verify)

- **Phase 1 — wolf-core `state.rs`:** tree types `TabState`/`WindowState{geom,tabs}`/`WolfState`;
  `StateStore::set_snapshot` replaces granular setters; old-format rejection; unit tests.
- **Phase 2 — wolf-ffi:** load side `FfiTabState` + `FfiWindowState.tabs()`; write side free-fn
  snapshot builder over `SHARED_STORE` + Mutex scratch; remove per-index setters + `window_index`
  (grep-verified state-only); `NewTab` discriminant (17); round-trip integration test.
- **Phase 3 — Swift seam `TabTitleSeam`:** basename + minimal disambiguation; headless tests.
- **Phase 4 — AppKit:** `NSWindowTabGroup` grouping (explicit `addTabbedWindow`, native "+" via
  `newWindowForTab`); `CommandSeam`/`CommandDispatcher` `.newTab`; coordinator-owned whole-tree
  snapshot walking live tab groups; tab titles from `current_file()` via `TabTitleSeam`.
- **Phase 5 — Docs & tests:** READMEs (wolf-core/src, wolf-ffi/src, wolf-macos + Sources) +
  architecture.md; prettier; real-nvim integration + headless seam tests; NSWindow grouping
  compile + manual-smoke.

## Progress log

- (init) Problem statement written; invoked architect.
- Architect plan complete (below); orchestrator decisions OD1–OD6 recorded above. Invoking test-writer.

---

# Architect Plan (authoritative — all agents implement against this + the OD1–OD6 rulings)

## Grep findings & confirmed decisions
- `window_index` is used ONLY for state in wolf-core (`attach_state_store` → `apply_cwd` `set_cwd`) and wolf-ffi (`wolf_ffi_start` param → `attach_state_store` + `WolfCoreHandle.window_index`, read only by `state_set_geometry`/`state_set_sidebar_visible`). Once state moves to the snapshot builder it is fully dead in both crates → REMOVE (from `wolf_ffi_start`, `WolfCoreHandle`, `attach_state_store`, `SharedStateStore`). In wolf-macos it becomes monotonic `tabId`.
- NewTab IS a palette row (palette iterates all specs except CommandPalette) → `palette_rank.rs` count 16→17.
- TabTitleSeam format: `"<basename> — <minimal trailing parent segments>"` (em dash), colliding labels only; empty/`[No Name]`/unnamed → `"untitled"`.
- Source conflicts flagged: (a) `state_save.rs` + `empty_state_cwd.rs` assert the core-side cwd mirror → retire/trim (see OD1); (b) wolf-ffi `state.rs` + `multi_window.rs` drive the per-index setters → rewrite/trim; (c) `redraw.rs::apply_cwd` keeps `recent_dirs.record` + tracked-cwd, drops only the StateStore mirror.

## Phase 1 — wolf-core `src/state.rs`
- Types: `TabState { cwd: String, sidebar_visible: bool }` (derive Clone/Debug/PartialEq/Serde/Default: cwd "", sidebar true). `WindowState { x,y,width,height: f64, tabs: Vec<TabState> }` (Clone/Debug/PartialEq/Serde/Default; **NO `#[serde(default)]` on `tabs`**). `WolfState { windows: Vec<WindowState> }` add PartialEq. `StateFile { version: u64=1, windows }`.
- `StateStore`: REMOVE `set_geometry`/`set_cwd`/`set_sidebar_visible`/`truncate_windows`/`mutate`. ADD `set_snapshot(&self, next: WolfState)`: compare on `&next` first; if `cell.state == next` return (no gen bump); else swap + `generation += 1` + `notify_all`. KEEP `open`/`state`/`flush`/writer thread/`writer_loop`/`write_state_file`/coalescing UNCHANGED.
- Doc-comments updated to describe the tree. `lib.rs` export `TabState`.

## Phase 1b — wolf-core `api.rs` + `redraw.rs`
- `api.rs`: remove `attach_state_store`, the `state_store: SharedStateStore` field + construction + clone into `RedrawHandler::new`; keep `state_dir` (recent-dirs/command-mru).
- `redraw.rs`: delete `SharedStateStore` alias + `state` field + ctor param; in `apply_cwd` delete the store-mirror block, KEEP `*self.cwd.write()=Some(target)`, the `is_initial` early-return, and `recent_dirs.record(&target)`. Update doc.

## Phase 2 — wolf-core `command.rs`
- `Command`: append `NewTab` (variant 17); update doc list. `COMMAND_SPECS`: append `{ NewTab, id:"new_tab", ex:"WolfNewTab", title:"New Tab", menu: File }`.
- In-module tests: `EXPECTED_COMMAND_COUNT` 17→18; `NEW_TAB_INDEX=17`; add id round-trip / spec-fields / last-row guards; change `buffer_picker_is_the_last_row` → fixed-index guard (`COMMAND_SPECS[16]==BufferPicker`).

## Phase 2 — wolf-ffi `src/lib.rs` (+ regenerate `generated/`)
- command_kind: `NEW_TAB=17`; `from_command` arm `Command::NewTab => NEW_TAB`; `kind()` doc range → `…NEW_TAB`.
- Load side: NEW opaque `FfiTabState { cwd()->String, sidebar_visible()->bool }` (`from_tab(&TabState)`); `FfiWindowState` DROPS `cwd()`/`sidebar_visible()`, ADDS `tabs()->Vec<FfiTabState>` (+ field), `from_window` maps tabs; keep x/y/w/h. `wolf_ffi_load_state` shape unchanged.
- Write side — free-fns over `SHARED_STORE` + `static SCRATCH: OnceLock<Mutex<WolfState>>` (helper `scratch()`; `.lock().expect("state scratch mutex poisoned")`):
  - `wolf_ffi_state_snapshot_begin()` → `windows.clear()`
  - `wolf_ffi_state_snapshot_add_window(x,y,width,height)` → push window w/ empty tabs
  - `wolf_ffi_state_snapshot_add_tab(cwd: String, sidebar_visible: bool)` → push onto `windows.last_mut()` (defensive no-op if none)
  - `wolf_ffi_state_snapshot_commit()` → clone scratch, `shared_store().map(|s| s.set_snapshot(state))`
  - `wolf_ffi_state_flush() -> bool` → `shared_store().map_or(true, |s| s.flush(STATE_FLUSH_TIMEOUT))`
- REMOVE handle `state_set_geometry`/`state_set_sidebar_visible`/`state_set_window_count`/`state_flush` (+ bridge decls); remove `store`/`window_index` fields, `attach_state_store` call, and the `window_index: u16` param of `wolf_ffi_start` → `wolf_ffi_start(init_lua, cwd, cols, rows)`. `shutdown` keeps a best-effort flush via `shared_store()`. `#[must_use]` + docs on the new outside-bridge items.
- Regenerate `generated/SwiftBridgeCore.swift`, `wolf-ffi.swift`, `wolf-ffi.h` (wake.h unchanged).

## Phase 3 — Swift `WolfKeyMap/TabTitleSeam.swift` (Foundation-only, headless)
`public enum TabTitleSeam { public static func labels(activeBufferPaths: [String]) -> [String] }` — 1:1 order-preserving; basename; empty/`[No Name]`/unnamed → `"untitled"`; colliding basenames extended with minimal trailing parent segments as `"<base> — <segs>"` growing until the collision set is distinct.

## Phase 4 — wolf-macos AppKit
- `CommandSeam.swift`: append `.newTab`; `init?(ffi:)` `case 17`; range doc `0..17`.
- `CommandDispatcher.swift`: `.newTab` → `controller.map { coordinator?.makeNewTab(from: $0) }`.
- `MainWindow.swift`: shared `tabbingIdentifier = "com.wolf.editor"`; `tabbingMode` default `.automatic`.
- `AppDelegate.swift` (coordinator): flat `controllers` kept for routing/wake/menu; REMOVE `windowIndex` slot, add monotonic `nextTabId`/`tabId` (used for Window-menu tags, `raiseWindow(tabId:)`, `otherWindows`, `raiseWindowMenuItem`). `makeController` takes `tabId:` not `windowIndex:`. `makeNewWindow` = standalone empty-state (unchanged placement). NEW `makeNewTab(from key:)` inherits `key.currentCwd`+`key.hasFolder`, `start()`, `key.window.addTabbedWindow(new.window, ordered:.above)` + select, append/`rebuildWindowMenu`/`scheduleStateSnapshot`. `@objc newWindowForTab(_:)` → key controller → `makeNewTab`. Restore GROUP-BY-GROUP: per `WindowState` start each `TabState`'s controller, set FIRST tab frame via `StateSeam.restoredWindowFrame`, `addTabbedWindow` the rest, set each tab's sidebar per `TabState.sidebar_visible` (empty-state hides). `restoredWindows`→`restoredGroups(from:) -> [[RestoredTabState]]` + per-group geometry. `applicationWillTerminate` → `snapshotNow()` + `wolf_ffi_state_flush()` then teardown. ADD `scheduleStateSnapshot()` (coalesced ~150ms) + `snapshotNow()` (walker §7.7) + `refreshTabTitles(forGroupOf:)`.
- Snapshot walker `snapshotNow()`: iterate controllers→windows; group by `window.tabGroup` (nil ⇒ solo group); order via `tabGroup.windows`; map window→controller via `owns(_:)`; geometry from `tabGroup.selectedWindow?.frame ?? first.frame`; per tab `currentCwd`+`sidebarVisible`; drive `begin`/`add_window`/`add_tab…`/`commit`.
- `WolfWindowController.swift`: `windowIndex`→`tabId`; `startCore()` calls shortened `wolf_ffi_start`; DELETE `updateWindowTitle`/`WindowTitleSeam` use — `onRenderWake` calls `coordinator?.refreshTabTitles(forGroupOf: window)`. Keep empty→folder transition + `lastKnownCwd`/`windowCwdDidChange`; call `scheduleStateSnapshot()` on cwd change, `didOpenFolder`, sidebar toggle, and `teardown` (before deferred `removeController`). Remove `handle?.state_flush()`. Expose `sidebarVisible`, `activeBufferPath` (`handle?.current_file().toString() ?? ""`), and the NSWindow for grouping.
- `StatePersistence.swift`: repurpose to snapshot triggers — no `WolfCoreHandle`/`state_set_*`; observers (move/end-live-resize/discrete-resize/sidebar KVO) call injected `onChange` (=`coordinator.scheduleStateSnapshot`). Add tab-group KVO on `window.tabGroup?.windows` + `.selectedWindow` (re-attached when membership changes; optional `TabGroupObserver.swift`). Coalesced timer absorbs the KVO storm.
- `AppMenu.swift`: `Tag.newTab=17`; File menu New Tab `⌘T` (after New Window ⇧⌘N); Window-menu item `tag = controller.tabId`.
- DELETE `WolfKeyMap/WindowTitleSeam.swift` + `WolfKeyMap/WindowIndexSeam.swift` (OD2).

## Append-only discriminant lockstep (all together)
1. `Command::NewTab`(17) 2. COMMAND_SPECS row 3. command.rs tests (count 18, NEW_TAB_INDEX=17, last-row) 4. ffi `command_kind::NEW_TAB=17`+arm+doc 5. regenerate generated/ 6. `CommandKind.newTab`+case 17+doc 7. `CommandDispatcher .newTab` 8. `AppMenu.Tag.newTab=17`+⌘T 9. `CommandSeamTests (ffi:17)==.newTab`, out-of-range→18 10. `commands.rs` guard `0..=17`+`CMD_KIND_NEW_TAB=17`+`:WolfNewTab` drain 11. `palette_rank.rs` STATIC_COMMAND_COUNT 16→17.

## Tests & Bazel targets
- `//crates/wolf-core:state_unit` — REWRITE: tree round-trip via StateFile; old-format rejection; unchanged-snapshot skips gen bump (behavioral assertion); snapshot replaces prior; version=1/absent/corrupt→None.
- `//crates/wolf-core:palette_rank` — count 16→17. `command.rs` inline (via `wolf_core_lib_test`) count 18.
- RETIRE `//crates/wolf-core:state_save` (delete stanza+file); TRIM `//crates/wolf-core:empty_state_cwd` to recent-dirs only.
- `//crates/wolf-ffi:state` (keep name — OD3) — REWRITE: drive builder free-fns → `wolf_ffi_load_state` round-trip (2 windows/3 tabs; geometry + per-tab cwd/sidebar/order) + old-format rejection (flat file → `windows()` empty). Builder half needs no nvim.
- `//crates/wolf-ffi:commands` — guard `0..=17` + `:WolfNewTab` drain. `//crates/wolf-ffi:multi_window` — drop state-setter section, keep wake-token identity.
- `WolfKeyMapTests`: ADD `TabTitleSeamTests` (distinct pass-through; two README collide→`README.md — <parent>`; three-way equal-immediate-parent→two segments; empty/`[No Name]`/unnamed→`untitled`; order/1:1; mixed). UPDATE `CommandSeamTests`. DELETE `WindowTitleSeamTests` + `WindowIndexSeamTests` from the source set (update BUILD/SwiftPM srcs).
- NSWindow tab grouping = compile + manual-smoke (window server). Smoke checklist: ⌘T inherits cwd; native "+"; tear-off standalone; drag-merge re-groups; reorder persists; label=active-buffer basename+disambiguation; restore reopens groups w/ geometry+per-tab sidebar; terminate writes tree.

## Implementation order
1. core state.rs (+lib export) → api.rs/redraw.rs → command.rs. 2. core tests. 3. ffi lib.rs → regenerate generated/. 4. ffi tests. 5. Swift WolfKeyMap (CommandSeam, TabTitleSeam; delete WindowTitle/WindowIndex seams; tests). 6. Swift WolfMacOS (MainWindow, WolfWindowController, StatePersistence, AppDelegate, CommandDispatcher, AppMenu). 7. smoke + docs.

## Docs (Phase 5)
`architecture.md` (state tree/`set_snapshot`/`attach_state_store` removed/cwd read per-snapshot; ffi FfiTabState + builder free-fns + removed setters + `window_index` + shortened `wolf_ffi_start` + NEW_TAB=17; macOS NSWindowTabGroup model + ⌘T/`:WolfNewTab` + coordinator snapshot ownership + TabTitleSeam + tear-off/merge native; discriminant table →`BufferPicker=16/NewTab=17`, "eighteen commands 0..17"; window↔tab model + new state contract). `crates/wolf-core/src/README.md` + `README.md` (state tree, set_snapshot, granular setters + attach_state_store removed, Command::NewTab, TabState/WolfState surface). `crates/wolf-ffi/src/README.md` + `README.md` (FfiTabState, FfiWindowState.tabs, builder free-fns, removed handle setters, window_index removal + shortened wolf_ffi_start, NEW_TAB=17). `crates/wolf-macos/README.md` + `Sources/README.md` (TabTitleSeam, tab grouping + shared tabbingIdentifier, coordinator snapshot ownership, :WolfNewTab/⌘T, CommandSeam/CommandDispatcher/AppMenu deltas, WindowTitleSeam/WindowIndexSeam removal, optional TabGroupObserver). `crates/wolf-ffi/tests/README.md` + `crates/wolf-macos/Tests/README.md` (rewritten state/commands/multi_window targets, new TabTitleSeamTests, removed seam tests, retired state_save / trimmed empty_state_cwd). `prettier --write` all edited md; no hard breaks in a paragraph.

## Risks
NSWindowTabGroup KVO reliability (mitigated by coalesced timer + snapshot-on-open/close backstop; fallback `didBecomeKey` + periodic); `newWindowForTab:` delegate routing (fallback MainWindow override); tab-title transient `"untitled"` before first cwd/current_file wake (acceptable); restore anchor when a group's first tab core fails (anchor next started tab; all-fail drops group).

Architect agent id: a0bd6c2fcac5ac00b
