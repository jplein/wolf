# Current Plan: Introduce a "project" architectural element

## Status: COMPLETE ✅

## Finalization
Technical-writer synced all docs (architecture.md + wolf-core/wolf-ffi/wolf-macos/runtime READMEs +
src/tests READMEs): Project/View/tabpage model, per-project wake token→`Project`, per-view `:tcd`
cwd (`Chdir`→`:tcd`), `Snapshot.current_tabpage`, win_hide→window-removed-grid-kept (OD8 `hidden`
now dead), refocus rebind, two-phase group-by-cwd restore, OD4 lockout, the six new FFI methods, the
five `WolfCore` tabpage methods, `wolf_setup_tabs` + helpers, `RestoreSeam`/`Project.swift`, and the
two `tabpages.rs` test binaries (7 core + 1 ffi) + `RestoreSeamTests`. Fixed the two actively-false
spots (`architecture.md` per-window→per-project; `crates/wolf-macos/README.md` controller→Project).
Orchestrator ran `prettier --write` on all 11 touched Markdown files → all already conformant
(`proseWrap: never`). Technical-writer agent id: a114cb30410b1e89b.

## Problem statement

Wolf's macOS front-end currently maps each Wolf "view" (one OS window — a tab in a native
Tahoe tab group or standalone) 1:1 to its own captive Neovim instance (its own
`WolfCoreHandle`/core, buffer list, cwd). We are introducing a **project**: one captive Neovim
instance shared by multiple views.

### Confirmed design decisions (from the user)

1. **Rendering: TABPAGE-PER-VIEW.** Each view binds to one Neovim TABPAGE within a shared nvim
   instance. Only the focused view live-updates; other views of the same project hold their last
   painted frame until refocused (invisible for tabs in a group; visible-but-stale for two
   side-by-side windows of the same project until clicked into). This is the only model that
   gives views independent content, because nvim renders only its current tabpage per UI and
   current-tabpage/cursor is global across all UIs of one instance.
2. **Lifecycle:** closing a view runs `:tabclose` on its tabpage but keeps the project's nvim
   alive while other views remain; the nvim shuts down only when its FINAL view closes. Reopening
   the same dir later is a FRESH project (no resurrection/merge).
3. **Working directory is PER-VIEW**, realized as each tabpage's `:tcd`. Each view's
   sidebar/picker roots at its tabpage's `getcwd()`.
4. **Restore: GROUP-BY-CWD** (confirmed — do NOT persist a project id). Project identity is NOT
   persisted; it is rebuilt from directories on load.

### Definitions

- **Project** = one captive Neovim instance (one `WolfCoreHandle`/core, one process, ONE shared
  buffer list). Identity = the nvim instance, not a directory.
- **View** = what `WolfWindowController` becomes: one OS window bound to (project, nvim tabpage).
  Owns its own panes, geometry, sidebar, picker, buffer picker, font state.

### Behavioral rules

- New window => new project (fresh nvim), one view/tabpage.
- New tab => inherits the originating view's project; new tabpage (`tabnew`) in that same nvim,
  added to the OS tab group.
- Tear-off => the view KEEPS its project (nvim + tabpage); no-op at the project level.
- Same directory in a different window => still a DISTINCT project; no merging.
- Restore: gather all (window, tab, cwd) across ALL windows; group tabs by cwd => one project
  (one nvim) per distinct cwd; one tabpage per tab; distribute tabpages/views back into the saved
  windows/tab-groups with their geometry. Restored projects are uniform-cwd.

### Accepted consequence (intended)

Identity is rebuilt from cwd: a runtime project whose views were `:tcd`'d to different dirs will
be SPLIT on restore; same-dir tabs from different runtime nvims will be MERGED on restore; every
restored project is uniform-cwd.

## Key technical facts about the current implementation (from READMEs)

- `WolfWindowController` owns ONE tab's stack: `handle` (own core), window, `paneContainer`,
  `tree`, `picker`, `palette`, `bufferPicker`, `statePersistence`, monotonic `tabId`, per-window
  `wakeToken`. `start()` calls `wolf_ffi_start(init_lua, launchCwd, cols, rows)`, reads
  `handle.wake_token()`, registers itself in `WindowRegistry` under that token. `windowWillClose`
  → `teardown()` → shutdown the core (1:1 window==nvim today).
- `AppDelegate` = coordinator over `controllers: [WolfWindowController]` (one per tab); mints
  monotonic `tabId`; owns coalesced whole-tree state snapshot (`scheduleStateSnapshot` →
  `snapshotNow` walks controllers, groups windows by `NSWindow.tabGroup`, drives
  `wolf_ffi_state_snapshot_begin/add_window/add_tab/commit`); owns tab titles via `TabTitleSeam`.
  `makeNewWindow(originatingCwd:)` opens a standalone empty-state window. `makeNewTab(from:)`
  currently starts a FRESH controller/core and `addTabbedWindow`s it into the key window's group.
- `WindowRegistry` (shared singleton, main-thread-only): maps a core's per-window wake `token`
  (u64) → owning `WolfWindowController`.
- Wake: `wolf_ffi_set_wake` registers a process-global `extern "C" fn(uint64_t token)`; each core
  fires with its per-window `wake_token` (minted in `wolf_ffi_start` from `NEXT_WAKE_TOKEN`).
  Two parallel wakes: `wolf_ffi_set_filesearch_wake`, `wolf_ffi_set_tree_wake`.
- Rendering: `ext_linegrid` + `ext_multigrid`; `Snapshot` is the multigrid render model with
  `grids`, `windows` (per-window WinInfo keyed by grid id), `active_win`, `current_file`,
  `buffers`, `cwd` (single global). `PaneContainerView.onRenderWake()` reconciles: `ensureInitialPane`,
  `applyIntents` (drain_intents), `detectCloses`, `resolvePendingGrids`, `updateActivePane`,
  `syncGridSizes`, then `needsDisplay`.
- State: `WolfState { windows: Vec<WindowState { x,y,width,height, tabs: Vec<TabState { cwd,
  sidebar_visible }> }> }`; `StateStore::set_snapshot` (whole-tree, coalesced writer thread,
  atomic rename). `load_state` infallible/read-only; version must be 1; missing `tabs` fails
  deser (no migration). FFI: `wolf_ffi_load_state`/`FfiState`/`FfiWindowState`/`FfiTabState` load
  side; `wolf_ffi_state_snapshot_begin/add_window/add_tab/commit` + `wolf_ffi_state_flush` write side.
- init.lua setup helpers (installed after ui_attach): current-file tracking, buffer tracking
  (`wolf_buffers`), layout overrides (`wolf_layout`/`wolf_active_win`), directory tracking
  (`wolf_cwd`), theme tracking, command registration, view tracking (`wolf_switch_buffer`).
- `Command` enum + `COMMAND_SPECS` (18 rows, append-only discriminant contract shared with
  `wolf-ffi` `command_kind` and Swift `CommandSeam`). `NewWindow=0 … NewTab=17`.

## Implementation areas (to be detailed by architect)

1. Core (wolf-core + runtime/nvim/init.lua): tabpage lifecycle (create/close/switch) +
   per-tabpage cwd (`cwd_for_tabpage`).
2. FFI (wolf-ffi): tabpage create/close/switch handle methods + per-tabpage cwd + per-project
   wake routing; regenerate bindings.
3. Swift front-end (wolf-macos): Project type; WolfWindowController -> View; AppDelegate projects
   registry; focus -> set current tabpage; reconciliation gated on "is my tabpage current".
4. State/restore: schema unchanged; group-by-cwd reconstruction.
5. Docs & tests: sync every touched README + architecture.md; real-nvim integration tests.

## Constraints

- Documentation is source of truth; keep every touched dir's README + architecture.md in sync.
- Markdown: no hard line breaks within a paragraph; run `prettier --write` after editing.
- wolf-core one hard rule: no GUI toolkit in its dependency tree.
- macOS builds wolf-core/wolf-ffi via Bazel; regenerate committed swift-bridge bindings after any
  bridge change (`bazel run //crates/wolf-ffi:gen_bindings`).
- Prefer e2e/integration tests that spawn a REAL pinned nvim; requires the flake devShell so
  `WOLF_NVIM_BIN` is set.
- Command / command_kind discriminant contract is APPEND-ONLY.

## Progress log

- (init) Problem statement written; invoking architect.
- Architect plan complete (below). Orchestrator decisions OD1–OD6 recorded. Invoking test-writer.
- Test-writer wrote §5 tests (core `tabpages.rs`, ffi `tabpages.rs`, `RestoreSeamTests.swift`).
  Pinned `RestoreSeam` shape passed to coder. Invoked coder.
- Coder complete. OD2 gate PASSED (win-handle stability confirmed, 6/6). All new + regression
  suites green; bindings regenerated (`wolf_ffi_wake.h` byte-identical); wolf-core GUI-free.
  Key finding + coder recommendations ruled on below (OD7–OD9). Invoking test-writer for OD7.

## Reviewer verdict: CODE APPROVED (no code blockers); docs pending → technical-writer

All 14 code invariants verified correct (Snapshot.current_tabpage fold; win_hide removes-window-
keeps-grid + no bad `hidden` dependents; Chdir→:tcd; SetTabpage/CloseTabpage; nvim/api tabpage
methods; init.lua OD4 lockout CONFIRMED not to swallow `:tcd`/`:lcd`/`:cd`; six additive FFI methods
+ regenerated bindings + wake.h clean; Project sole wolf_ffi_start caller + strong-view/weak-project
ownership + last-view-only shutdown; token→Project wakes; PaneContainerView refocus gate; controller
view refactor; AppDelegate two-phase restore + terminate-order no-double-shutdown; RestoreSeam
matches pinned shape + OD5; CommandDispatcher/TabGroupObserver tear-off keeps project). wolf-gnome
behavior-preserving; wolf-core GUI-free. The reviewer's ONLY "needs revision" item is the docs
deliverable (Architect Plan §6), which the coder deliberately left to the technical-writer. Specific
stale/false spots the technical-writer MUST fix: `architecture.md:216` (per-window core/token/
controller → now per-project), `crates/wolf-macos/README.md:72` (controller owns handle/wakeToken/
start/shutdown → now Project owns them), `render.rs:423` + `crates/wolf-core/src/README.md:57`
(`WinInfo.hidden` doc → OD8: now effectively always-false), plus the additive surfaces in items 4–7
of the review. Reviewer agent id: a0a41813c07cf2807.

## Coder results (complete — all tests green)

- **OD2 gate PASS:** `//crates/wolf-core:tabpages` 6/6 incl. `window_handles_survive_a_tabpage_switch`.
  nvim WINDOW handles ARE stable across a tabpage switch. Refocus-rebind design proceeds.
- **Important finding (accepted):** nvim emits `win_hide` (NOT `win_close`/`grid_destroy`) for the
  leaving tabpage's windows on a tabpage switch, and REUSES the same grid id + window handle on the
  way back (re-`win_pos`, possibly without re-`grid_resize`). The coder changed `redraw.rs`'s
  `win_hide` handler to REMOVE the window from `Snapshot.windows` (so `windows`/`grid_for_win`
  reflect only the current tabpage — the contract the tests + refocus rebind need) but KEEP the grid
  in `Snapshot.grids` (nvim reuses it; a rebound window always resolves to a live grid). Side effect:
  `WinInfo.hidden` is now effectively always-false / dead (no test asserts `hidden==true`; all
  `!w.hidden()` filters still behave correctly).
- Tests: `//crates/wolf-core:tabpages` 6/6, `//crates/wolf-ffi:tabpages` 1/1,
  `//crates/wolf-macos:WolfKeyMapTests` pass (RestoreSeam a–e), `//crates/wolf-core:all` 17/17,
  `//crates/wolf-ffi:all` 25/25 (incl. `file_actions` under `Chdir`→`:tcd`, `state`, `multi_window`,
  `panes`, `close_pane`, `pane_status`), `bazel build //crates/wolf-macos:Wolf` OK.
- Bindings regenerated; `wolf_ffi_wake.h` byte-identical. wolf-core dep tree GUI-free (denylist clean).
- New files: `Sources/WolfKeyMap/RestoreSeam.swift`, `Sources/WolfMacOS/Project.swift`.
  `CommandDispatcher`/`TabGroupObserver` audited, no change needed (tear-off keeps project+tabpage;
  no core shutdown/recreate). Coder agent id: aa6cb62e33d1102c4.

## Orchestrator rulings on the coder's recommendations (BINDING)

- **OD7 — ACCEPT rec #1: add a focused test pinning the new `win_hide` semantics.** The
  tabpage-switch window-churn contract (win_hide REMOVES the window from `Snapshot.windows` but
  KEEPS its grid in `Snapshot.grids`) is currently covered only indirectly by the tabpages
  integration test. This nvim-behavior-dependent change is load-bearing for the whole refocus
  rebind → pin it with a focused test that directly asserts window-removed / grid-kept. Test-writer
  to add it (headless synthetic-redraw unit test if a `RedrawHandler` harness exists, else a
  minimal real-nvim assertion — test-writer picks the form matching existing conventions).
- **OD8 — DECLINE rec #2: keep `WinInfo.hidden`.** Do NOT remove the now-dead `hidden` field /
  `FfiWinInfo::hidden()` accessor / `!w.hidden()` filters — that is an FFI+Swift surface churn for
  no functional gain. The technical-writer must DOCUMENT that `hidden` is now effectively
  always-false (a window leaves `Snapshot.windows` entirely rather than being marked hidden).
- **OD9 — ACCEPT rec #3 as-is: no automated AppKit-restore coverage.** The two-phase project
  materialization / refocus-rebind-on-restored-view / tear-off-keeps-project paths touch the window
  server → compile + manual-smoke only (per §5.4). The pure `RestoreSeam` grouping is already
  headlessly tested. No new harness this pass; surfaced to the user as manual-smoke items.

## Orchestrator decisions on the architect's open questions (BINDING — all agents follow these)

- **OD1 — Include BOTH the front-end-authoritative current-view AND `Snapshot.current_tabpage`
  as the refocus barrier.** The front-end drives every tabpage switch; the snapshot field lets
  the refocus rebind consume against the correct frame. Ratified.
- **OD2 — Refocus rebind by re-resolving `grid_for_win` on the PRESERVED `PaneTree`** (not
  geometry reconstruction). This is CONTINGENT on the nvim win-handle-stability integration test
  (§5.1 `tabpages.rs`). **If that test fails, the coder must STOP and report back** — the fallback
  (geometry-based tree reconstruction) is a larger, deferred design that needs re-planning.
- **OD3 — `InputEvent::Chdir` changes from `:cd` to `:tcd`.** Required for per-view cwd. Ratified.
  Caveat for coder/reviewer: verify `crates/wolf-core/tests/file_actions.rs` still holds (`:tcd`
  also updates `getcwd`/`DirChanged`); wolf-gnome is single-tabpage so `:tcd` ≡ `:cd` there and it
  has no macOS Bazel target (cannot be built/tested here) — note the semantic equivalence.
- **OD4 — Lock out ALL nvim-internal tabpage navigation AND creation/mutation, best-effort.**
  Not just the `gt`/`gT`/`<C-PageUp>`/`<C-PageDown>` chords (→ `<Nop>`) but also the typed Ex-
  commands `:tabnew`/`:tabnext`/`:tabprevious`/`:tabonly`/`:tabclose`/`:tabmove`, using the same
  abbrev/override mechanism the split overrides use. Rationale: a user-typed `:tabnew` would create
  an ORPHAN tabpage no Wolf view is bound to; `:tabnext`/`:tabonly`/`:tabclose` would switch/close
  a tabpage out from under a view and desync the front-end-authoritative current view. Wolf owns
  ALL tabpage lifecycle. Residual gaps (obscure typed variants) are an accepted v1 limitation and
  must be documented.
- **OD5 — Empty-state / no-folder tabs (persisted `cwd == ""`) are NOT grouped on restore.**
  Each restored empty-state tab becomes its OWN single-tabpage project (a fresh nvim), matching
  "new window => new project". Only REAL directories group by canonical cwd. `RestoreSeam.plan`
  must give every `""`-cwd tab a UNIQUE project key (do not collapse them under one `""` key).
  (Surfaced to the user in the final report; sensible reading of group-by-cwd for the degenerate
  no-directory case.)
- **OD6 — Tab titles via per-view cached `activeBufferPath`; state/Window-menu cwd via the
  blocking `cwd_for_tabpage`.** Ratified (titles need the buffer path per-wake so caching is used;
  cwd reads are low-frequency so the blocking RPC is fine).

None of OD1–OD6 require re-consulting the user; they are correct consequences of the locked
decisions (OD5 is surfaced in the final report as a courtesy).

---

# Architect Plan (authoritative — all agents implement against this + the OD1–OD6 rulings)

## 0. Summary

Convert the macOS front-end from **one captive Neovim per view** to **one Neovim per PROJECT,
shared by N views, each view bound to one nvim TABPAGE**. Only the focused view live-updates;
others hold their last painted frame. Closing a view runs `:tabclose`; the nvim shuts down only
when its last view closes. Each view's cwd is its tabpage's `:tcd`. Restore is group-by-cwd.

Load-bearing facts confirmed from the code:
- `Snapshot.windows` already contains only the CURRENT tabpage's windows; on a tabpage switch nvim
  emits `win_close`/`grid_destroy` for the old grids then fresh `grid_resize`/`win_pos` for the new
  (`redraw.rs`: `win_close`~766, `grid_destroy`~528, `win_pos`~732). A view's pane→grid bindings go
  STALE across a switch and must be re-derived on refocus.
- nvim WINDOW handles are STABLE across a tabpage switch; only GRID ids churn. (Must be VERIFIED by
  the §5.1 integration test — top risk; OD2 gates on it.)
- `getcwd()` honors the current window/tab's cwd (`nvim.rs` `current_dir` ~191), so
  `live_cwd()`/`cwd()`/picker/tree/MRU already return the right value for the FOCUSED view; the only
  gap is a NON-current view's cwd → solved by `cwd_for_tabpage`.
- `wolf_buffers` uses `nvim_list_wins()` = current-tabpage windows only (`init.lua`~209), so
  `Snapshot.buffers`/per-pane statusbar stay correct with no change.
- The wake token is minted per `wolf_ffi_start` (`lib.rs`~1653), mapped token→controller in
  `WindowRegistry`; it becomes per-PROJECT and the fan-out to the current view is front-end.

No FFI discriminant table changes (all new FFI is additive methods). Regenerate bindings.

## 1. Core — wolf-core + runtime/nvim/init.lua

### 1.1 New Snapshot field (render gate + refocus barrier)
- `render.rs`: add `current_tabpage: i64` to `Snapshot` (default 0 = unknown), beside `active_win`
  (bare `pub` field, mirror it). Document in `src/README.md`.
- `redraw.rs`: add `const TABPAGE_EVENT: &str = "wolf_tabpage";` beside `ACTIVE_WIN_EVENT`; add a
  `TABPAGE_EVENT` arm in `handle_notify` → `apply_tabpage(args)` (take+drop write lock, set
  `snap.current_tabpage = ext_handle(arg0)`, NO `seq` bump) then fire `(self.wake)()` — same shape
  as `apply_active_win` (~348).

### 1.2 init.lua: tabpage tracking + nav/creation lockout (OD4) + helpers
Add `_G.wolf_setup_tabs(chan)` (installed after `wolf_setup_commands`, channel-id pattern):
- `TabEnter`/`TabNewEntered` autocmd `rpcnotify`-ing `wolf_tabpage` with
  `nvim_get_current_tabpage()`, plus one initial emit.
- **OD4 lockout:** map `gt`/`gT`/`<C-w>gt`/`<C-w>gT`/`<C-PageUp>`/`<C-PageDown>` → `<Nop>`; and
  best-effort intercept typed `:tabnew`/`:tabnext`/`:tabprevious`/`:tabonly`/`:tabclose`/`:tabmove`
  (same abbrev/override mechanism as the split overrides). Document residual gaps as v1 limitation.
- Module-level Lua helpers:
  - `_G.wolf_create_tabpage(cwd)`: `vim.cmd('tabnew')`; if `cwd~=''` then `tcd fnameescape(cwd)`;
    `return nvim_get_current_tabpage()`.
  - `_G.wolf_close_tabpage(handle)`: `n = nvim_tabpage_get_number(handle); pcall(vim.cmd,
    n..'tabclose')` (pcall swallows last-tab E784 — the front-end never closes the last view; the
    Project shuts the core down instead).
  - `_G.wolf_tab_cwd(handle)`: `n = nvim_tabpage_get_number(handle); return getcwd(-1, n)` (maps the
    stable HANDLE to the renumbering NUMBER `getcwd(-1,n)` needs; falls back to global cwd for an
    un-`tcd`'d tab — correct).
- Update `wolf_setup_dir_tracking`/`wolf_setup_buffer_tracking` comments to note they report the
  current tabpage's cwd/windows (the focused view).

### 1.3 nvim.rs: tabpage RPCs
Add (mirroring `current_dir`/`list_buffers`/`setup_view_tracking`):
- `create_tabpage(&self, cwd: &str) -> Result<i64, NvimError>` (exec_lua `wolf_create_tabpage`,
  decode via `ext_handle`).
- `tab_cwd(&self, handle: i64) -> Result<PathBuf, NvimError>` (exec_lua `wolf_tab_cwd`, `as_str`).
- `current_tabpage(&self) -> Result<i64, NvimError>` (`nvim_get_current_tabpage`, `ext_handle`).
- `set_current_tabpage(&self, handle) -> Result<(), NvimError>` and
  `close_tabpage(&self, handle) -> Result<(), NvimError>` (latter via `wolf_close_tabpage`).
- `setup_tabs(&self) -> Result<(), NvimError>` (register `wolf_setup_tabs`, channel-id pattern).

### 1.4 input.rs: new variants + Chdir → :tcd (OD3)
- `InputEvent`: add `SetTabpage { handle: i64 }` and `CloseTabpage { handle: i64 }`; `apply_batch`
  arms → `nvim.set_current_tabpage`/`close_tabpage`.
- **Change `InputEvent::Chdir` to `:tcd`** (not `:cd`). Update doc-comment (~93) and
  `crates/wolf-core/README.md` `InputEvent` bullet.

### 1.5 api.rs: WolfCore tabpage methods
Following blocking (`list_buffers`) vs fire-and-forget (`focus_win`) idioms:
- `create_tabpage(&self, cwd: Option<&Path>) -> i64` (blocking; 0 on error).
- `cwd_for_tabpage(&self, handle: i64) -> Option<PathBuf>` (blocking; canonicalized; None on error) —
  authoritative non-current-view cwd.
- `current_tabpage(&self) -> i64` (blocking; 0 on error) — first-view bind.
- `set_current_tabpage(&self, handle)` / `close_tabpage(&self, handle)` — `send_input` (ordered).
Document the two-tier pattern: `Snapshot.current_tabpage` (cheap live gate via `read_snapshot`) vs
`current_tabpage()` (authoritative bind-time RPC) — mirrors `live_cwd()` cell vs `current_dir()` RPC.

### 1.6 Directory/buffer tracking — minimal, documented
- `apply_cwd`/`recent_dirs` recording UNCHANGED (DirChanged fires for `:tcd`; current tab = focused
  view). Only reading a non-current view's cwd was missing → `cwd_for_tabpage`. Document.
- `apply_buffers`/`pane_status` UNCHANGED (current-tabpage-scoped). Document.

### 1.7 Start sequence
`api.rs::WolfCore::start` — add `proc.setup_tabs().await?;` to the setup chain (after
`setup_view_tracking`/alongside `setup_commands`). Update `start` doc bullet.

## 2. FFI — wolf-ffi (all additive; NO discriminant change; wake headers unchanged)

### 2.1 New WolfCoreHandle methods (bridge + glue, `borrow().as_ref()` core-gone convention)
- `create_tabpage(&self, cwd: String) -> i64` → `core.create_tabpage((!cwd.is_empty())
  .then(|| PathBuf::from(cwd)).as_deref())`; 0 when gone.
- `current_tabpage(&self) -> i64` → `core.current_tabpage()`; 0 when gone (first-view bind).
- `active_tabpage(&self) -> i64` → `core.read_snapshot().current_tabpage`; 0 when gone (cheap gate).
- `cwd_for_tabpage(&self, tab: i64) -> String` → map to String (empty when gone/none).
- `set_current_tabpage(&self, tab: i64)` / `close_tabpage(&self, tab: i64)` (no-op when gone).
Place near `cwd()`/`active_win()` (~388–443), mirror doc style.

### 2.2 Wake stays per-project
`wolf_ffi_start` signature unchanged; still mints one `wake_token` per core = per project.
token→Project map + current-view fan-out are entirely front-end. Update crate-doc "Wake" +
`crates/wolf-ffi/README.md`: the token identifies a PROJECT.

### 2.3 Regenerate bindings
`bazel run //crates/wolf-ffi:gen_bindings`; commit refreshed `generated/wolf-ffi.swift` +
`wolf-ffi.h`. `wolf_ffi_wake.h` untouched.

## 3. Swift front-end — wolf-macos

### 3.1 New type `Project` (`Sources/WolfMacOS/Project.swift`, main-thread class)
- Stored: `let handle`, `let wakeToken`, `private(set) weak var currentView`, weak `views` list,
  `weak var coordinator`.
- `static func start(launchCwd:coordinator:) -> Project?` — the ONLY caller of `wolf_ffi_start`;
  reads `handle.wake_token()`, registers in `WindowRegistry` under the token.
- `bindFirstTabpage() -> Int64` (= `handle.current_tabpage()`), `addTabpage(cwd:) -> Int64`
  (= `handle.create_tabpage(cwd)`; 0 → caller drops the view).
- `attach(_:)`/`detach(_:)` (weak list; detach → if empty, `shutdown()`; else reassign currentView).
- `setCurrent(_:)` → guard change; `handle.set_current_tabpage(view.tabpageHandle)`.
- `closeTabpage(for:)` → `handle.close_tabpage(view.tabpageHandle)` (non-last close).
- `onWake()`/`onPickerWake()`/`onTreeWake()` → route to `currentView?.…` (picker/tree refresh is
  session-local → harmless if a stray non-current traversal wake lands; document).
- `shutdown()` → `handle.shutdown()` (flushes SHARED_STORE); `WindowRegistry.unregister(token:)`;
  `coordinator?.removeProject(self)`.
- **Ownership (no cycles):** AppDelegate strong-holds `projects` + `controllers`; each view holds
  `let project` STRONG (core outlives a view teardown + a still-binding sibling); Project holds WEAK
  views/currentView. Non-last close → `closeTabpage`+`detach` (core lives); last close's `detach` →
  `shutdown()`. The view NEVER calls `handle.shutdown()` itself.

### 3.2 WindowRegistry — token→Project
`projects: [UInt64: Project]`; `register(_:token:)`/`unregister(token:)`/`project(for:)`. Remove the
controller-keyed API; update all three wake-bridge call sites.

### 3.3 Wake bridges
`WakeBridge`/`PickerWakeBridge`/`TreeWakeBridge` → `WindowRegistry.shared.project(for: token)?
.onWake()/onPickerWake()/onTreeWake()`.

### 3.4 WolfWindowController → VIEW
- Add `let project`, `private(set) var tabpageHandle: Int64`. `start()` no longer calls
  `wolf_ffi_start`; the core-start portion (`startCore()` ~666) is removed; the controller uses the
  injected `project.handle` and a coordinator-supplied `tabpageHandle` (first view →
  `project.bindFirstTabpage()`; joiner/restore → `project.addTabpage(cwd)`). Rest of `start()` stays;
  rename to `build() -> Bool` (false if handle nil). Remove per-controller `wakeToken`/
  `WindowRegistry.register` (~171–173).
- `windowDidBecomeKey(_:)` (NSWindowDelegate): `guard project.currentView !== self;
  project.setCurrent(self); paneContainer?.markRefocusRebind()`.
- `onRenderWake` (~296): at TOP update cached `activeBufferPath` from `handle.current_file()`, then
  `refreshTabTitles`. The empty-state `:cd` detection `handle.cwd()` read stays correct (current tab
  = this focused view).
- `teardown()` (~622): REMOVE `handle?.shutdown()`; instead `project.closeTabpage(for: self)` +
  `project.detach(self)` (Project decides last-view shutdown). Remove `WindowRegistry.unregister`.
  Keep per-view overlay/session/observer cleanup + coalesced snapshot + deferred `removeController`.

### 3.5 AppDelegate
- Add `projects: [Project]` + `removeProject(_:)`.
- `makeNewWindow` (~169): `Project.start(launchCwd: home, coordinator: self)` → first view via
  `makeController(project:tabpageHandle: project.bindFirstTabpage(), …)`; `build()`; append; attach;
  setCurrent. (Fresh nvim.)
- `makeNewTab(from key:)` (~187): reuse `key.project`; `h = key.project.addTabpage(cwd:
  inheritedCwd)`; guard `h != 0`; build view bound to `(key.project, h)`; `addTabbedWindow` into
  key's group; `key.project.setCurrent(newView)`. (Same nvim, new tabpage.)
- `makeController(...)` (~379): thread `project`/`tabpageHandle`.
- `snapshotNow()` (~286): per-tab cwd via the view's `persistedCwd` (§3.7). Grouping/geometry
  unchanged (a native tab group may hold views from MULTIPLE projects; a project may span MULTIPLE
  groups; project membership NOT persisted).
- `refreshTabTitles` (~321): reads each tab's CACHED `activeBufferPath`.
- `applicationWillTerminate` (~150): after `snapshotNow()` + `wolf_ffi_state_flush()`, iterate
  PROJECTS and `project.shutdown()` (backstop) instead of per-controller `handle.shutdown()`.
- Window-menu/palette switch-window/`controllerForCwd`/`otherWindows` unchanged (per-view `tabId`),
  but any surfaced cwd uses `persistedCwd`.

### 3.6 PaneContainerView — refocus re-derivation (TOP RISK; OD2)
- Add `private var pendingRefocusRebind = false` + `markRefocusRebind()`.
- Gate `onRenderWake` (~478): if `pendingRefocusRebind`, `guard handle.active_tabpage() ==
  controllerTabpageHandle else { return }` (consume only once the snapshot reflects THIS tabpage),
  clear the flag, `rebindStaleGrids()` (null EVERY pane's `boundGrid`, then `resolvePendingGrids()`
  which rebinds via `grid_for_win(binding.win)`), `updateActivePane()`, `syncGridSizes()`,
  `applyTitlebarTheme()`, `needsDisplay = true`, return. SKIP `detectCloses()` on the rebind pass
  (old destroyed grids would wrongly prune live panes; next normal pass reconciles real closes).
- Works because nvim WINDOW handles survive the switch (only grids churn) and the `PaneTree`
  topology is preserved → multi-pane layouts restore by re-binding grids, no geometry reconstruction.
- Pass the view's `tabpageHandle` into `PaneContainerView` (init param or setter → `controllerTabpageHandle`).
- Non-current views hold their last paint (Project.onWake never drives them). Set
  `paneContainer.wantsLayer = true` so a VISIBLE non-key window (two side-by-side same-project
  windows) keeps its layer. Live-resize of such a window can still force a blank `draw` (grids gone)
  → v1 limitation (§10).

### 3.7 Per-view cwd (state/menu) + per-view active buffer (titles) — OD6
- `var persistedCwd: String { hasFolder ? (handle?.cwd_for_tabpage(tabpageHandle).toString() ?? "")
  : "" }` (the `hasFolder` gate preserves the "empty-state persists ''" contract). Used by
  `snapshotNow` + Window-menu cwd (low-frequency → blocking RPC OK).
- `private var cachedActiveBufferPath = ""`, updated at top of `onRenderWake` from
  `handle.current_file()`; `var activeBufferPath { cachedActiveBufferPath }`. A non-current view
  holds its last cached value (frozen-accurate; a non-current tabpage's buffer can't change without
  input). Restored-but-never-focused tab → empty → generic-label fallback until first focus (v1 OK).
- Invariant making this sound: `InputEvent::Chdir` = `:tcd` (OD3), so no view ever issues a global
  `:cd`; `getcwd(-1,n)` returns each tab's own value (global/launch cwd for the first un-`tcd`'d view).

### 3.8 CommandDispatcher / TabGroupObserver / StatePersistence
- CommandDispatcher: `newWindow`→`makeNewWindow` (mints Project); `newTab`→`makeNewTab` (same
  Project, new tabpage). AUDIT `.closeWindow` (`window?.performClose`) routes each tab close through
  `windowWillClose`→teardown→`project.detach` (tab close = `:tabclose`; last = shutdown). No change
  expected; audit only.
- TabGroupObserver: AUDIT tear-off — a tear-off/merge changes `NSWindow.tabGroup` identity but must
  NOT close/reopen the view; the view keeps `project`+`tabpageHandle`; NO core shutdown/recreate.
  Confirm `didBecomeKey` re-attach + coalesced snapshot don't touch handle/project. New interaction:
  `windowDidBecomeKey` also drives `setCurrent` (§3.4); the ~150ms coalescer absorbs the KVO storm —
  verify no re-entrancy between `setCurrent`'s `set_current_tabpage` wake and the snapshot scheduler.
- StatePersistence: unchanged (trigger-only; the walk reads `persistedCwd`).

## 4. State / restore (schema UNCHANGED; no project id)

### 4.1 Write side — `snapshotNow` only changes per-tab cwd to `persistedCwd`. Grouping/geometry
unchanged.

### 4.2 Restore — two-phase materialize-projects-then-build-windows
Extract the pure grouping into a testable seam `RestoreSeam.plan(windows:home:isDirectory:)
-> RestorePlan` in `WolfKeyMap` (Foundation-only):
- `projects: [ProjectPlan]` keyed by resolved cwd. **OD5:** `.folder(cwd)` → key = canonical cwd;
  `.emptyState` (`""`) → a UNIQUE key PER TAB (each empty-state tab = its own project — do NOT
  collapse under one `""` key).
- Each `ProjectPlan` lists its tabs, each carrying `(groupIndex, positionInGroup, cwd,
  sidebarVisible)` for Phase-B placement.
AppDelegate restore:
1. `let plan = RestoreSeam.plan(...)` over `wolf_ffi_load_state()`.
2. **Phase A — materialize projects & views (no windows shown):** per `ProjectPlan`,
   `Project.start(launchCwd: firstTabCwdOrHome, coordinator:)` (fail → drop its tabs); first tab →
   `bindFirstTabpage()` (implicit tabpage 1); tabs 2..N → `addTabpage(cwd)` (0 → drop that tab);
   per surviving tab build a `WolfWindowController` bound to `(project, tabpageHandle)` with saved
   `sidebar_visible` + `(groupIndex, position)`; `build()`; `attach`. Do NOT anchor/`addTabbedWindow`
   yet. Append project + controllers.
3. **Phase B — assemble OS windows/tab-groups:** iterate original saved window groups in order; take
   the materialized views for that group by `(groupIndex, position)`; anchor the first survivor at
   `StateSeam.restoredWindowFrame`; `addTabbedWindow` the rest (existing group-assembly logic).
4. **Robustness (preserved):** failed tab skipped; group with no survivors dropped; ALL projects
   fail → `presentStartupFailureAndQuit`.
5. First-tabpage-implicit: a fresh nvim owns tabpage 1 → first tab binds it (no `tabnew`).
Set each project's `currentView` to whichever view ends up key.

## 5. Tests (prefer real nvim per CLAUDE.md; per-binary Bazel targets; existing conventions)

### 5.1 `crates/wolf-core/tests/tabpages.rs` — real nvim — `//crates/wolf-core:tabpages`
- create_tabpage on a running core → new window grid appears; `Snapshot.current_tabpage` updates.
- set_current_tabpage back → poll `read_snapshot()`; the 2nd tabpage's grids VANISH, the 1st's
  reappear (stale-binding contract).
- **Win-handle-stability keystone:** on T1 `:vsplit` (W1,W2); switch to T2, back to T1; assert the
  SAME W1,W2 reappear with fresh grids and `grid_for_win(W1/W2)` resolve. (OD2 hinges on this.)
- Per-tabpage cwd: `:tcd dirA` on T1, create T2 `cwd=dirB`; assert `cwd_for_tabpage(T1)==dirA` and
  `cwd_for_tabpage(T2)==dirB` INCLUDING while T1 current / T2 not (non-current read).
- Shared buffers: `:e a.md` on T1, `:e b.md` on T2 → `list_buffers()` has both regardless of current.
- Stable-handle-survives-renumber: create T2,T3; close T2; assert T3's HANDLE unchanged and
  `cwd_for_tabpage(T3)` still works (its NUMBER changed, handle did not).

### 5.2 `crates/wolf-ffi/tests/tabpages.rs` — real nvim — `//crates/wolf-ffi:tabpages`
- Drive the bridge: `wolf_ffi_start` → `current_tabpage()` → `create_tabpage(dirB)` →
  `set_current_tabpage` back/forth polling `active_tabpage()` + `windows()`/`read_frame_for_grid`
  for appear/disappear.
- Grid re-bind contract: split on T1, switch away+back, assert `grid_for_win(win)` re-resolves live.
- `cwd_for_tabpage` for a non-current tabpage returns the tab-local `:tcd` value.
- Last-tabpage-close vs shutdown: two tabpages, `close_tabpage(second)` → back to one; then
  `shutdown()` → subsequent handle calls return neutral values (core gone).

### 5.3 State group-by-cwd — headless
Schema round-trip stays in Rust (`//crates/wolf-core:state_unit` + `//crates/wolf-ffi:state`
unchanged). Add a PURE `RestoreSeam.plan(...)` unit test in `WolfKeyMapTests` (Foundation-only):
(a) same-cwd tabs collapse into one ProjectPlan; (b) same-dir tabs from different saved windows
MERGE; (c) different-dir tabs in one window SPLIT; **(d) OD5: each empty-state (`""`) tab is its OWN
project (NOT merged);** (e) each tab retains `(groupIndex, position, sidebarVisible)`.

### 5.4 macOS AppKit wiring — compile + manual smoke
Project lifecycle, wake fan-out, `windowDidBecomeKey`→setCurrent, refocus rebind, restore assembly
touch the window server → compile-verified + manual smoke only. Pure `RestoreSeam` is headless.

### 5.5 Discriminant guards — none needed (no FFI discriminant changed).

## 6. Docs (source of truth)
- `architecture.md`: wolf-ffi tabpage methods + per-project token; wolf-macos projects/views/tabpages
  model, per-view `:tcd` cwd, per-project wake routing, refocus re-derivation, group-by-cwd restore;
  wake section (token = project, fan-out to current view); core↔front-end contract
  (`Snapshot.current_tabpage`, current-tabpage-scoped windows/buffers/cwd, `Chdir`=`:tcd`); a short
  Project/View/tabpage definition + last-view-closes-shutdown; v1 limitations (OD4 lockout gaps,
  resize-of-non-current-visible-window blank).
- `crates/wolf-core/README.md` + `src/README.md`: new WolfCore methods, `InputEvent::SetTabpage/
  CloseTabpage`, `Chdir`→`:tcd`, `Snapshot.current_tabpage`, `wolf_tabpage`/`wolf_setup_tabs`,
  tabpage-awareness of cwd/buffers.
- `crates/wolf-ffi/README.md` + `src/README.md`: six new handle methods, per-project token,
  regenerate-bindings note, wake headers unchanged.
- `crates/wolf-macos/README.md` + `Sources/README.md` + `Tests/README.md`: `Project` type + ownership,
  controller→view refactor, wake fan-out, refocus rebind, group-by-cwd restore + `RestoreSeam`.
- `runtime/nvim/README.md`: `wolf_setup_tabs`, tab nav/creation lockout, `wolf_create_tabpage`/
  `wolf_close_tabpage`/`wolf_tab_cwd`, `:tcd` per-view cwd.
- `crates/wolf-core/tests/README.md` + `crates/wolf-ffi/tests/README.md`: the two `tabpages.rs`
  binaries + `RestoreSeam` note.
- `prettier --write` all touched md; no hard breaks within paragraphs.

## 7. Append-only steps — none needed (all new FFI is additive methods). If a discriminant is later
added: append at END of the relevant `mod <kind>` in `wolf-ffi/src/lib.rs`, mirror at END of the
`wolf_core` enum, add the Swift `WolfKeyMap` case, extend the contract-guard test — never renumber.

## 8. Implementation order (avoids compile breakage)
1. Core bottom-up: `render.rs` → `redraw.rs` → `input.rs` → `nvim.rs` → `init.lua` → `api.rs`
   (+ `setup_tabs()` in `start`). Land `//crates/wolf-core:tabpages`.
2. FFI: six `WolfCoreHandle` methods → `bazel run //crates/wolf-ffi:gen_bindings` (commit) → land
   `//crates/wolf-ffi:tabpages`.
3. Swift: `RestoreSeam` (+ WolfKeyMapTests) → `Project.swift` → `WindowRegistry` → wake bridges →
   `PaneContainerView` (refocus rebind, `wantsLayer`) → `WolfWindowController` (view refactor) →
   `AppDelegate` (projects registry, makeNewWindow/makeNewTab, two-phase restore, snapshot/titles/
   terminate) → CommandDispatcher/TabGroupObserver audit.
4. Docs + prettier.
5. Verification (coder runs): `//crates/wolf-core:tabpages`, `//crates/wolf-ffi:tabpages`, existing
   `state`/`multi_window`/`panes`/`file_actions` (regression: `Chdir`→`:tcd`), `WolfKeyMapTests`,
   `bazel build //crates/wolf-macos:Wolf`.

## 9/10. Risks
1. Win-handle stability across tabpage switch (§5.1) — the whole refocus-rebind rests on it; the
   test lands in Phase 1 BEFORE any Swift work (OD2 gate: fail → STOP + report).
2. Refocus timing — mitigated by the `active_tabpage()==tabpageHandle` gate before consuming the
   rebind.
3. Teardown/retain cycles — view→project strong, project→views weak, registry cleared at last-view
   shutdown; last-view path is the only core shutdown; audit `performClose`/`windowWillClose`/tear-off.
4. NSWindow tabGroup KVO — `windowDidBecomeKey` re-attaches the observer AND drives `setCurrent`;
   coalescer absorbs the storm; verify no re-entrancy with the snapshot scheduler.
5. Non-current visible window blanking on live-resize — mitigated by `wantsLayer`; residual = v1 limit.
6. Picker/tree wake routed to current view while a non-current view has an in-flight traversal —
   harmless (session-local), documented, manual-smoke.

Architect agent id: a0f6f6d2366cf1e01
