# Current Plan: Tabs per pane (macOS-first)

## Status: COMPLETE ✅

Docs synced (technical-writer a2a7a0f0b26c5d4f0) across 11 Markdown files
(architecture.md + wolf-core/wolf-ffi/wolf-macos READMEs & sub-READMEs +
runtime/nvim/README) with the tab model, `wolf_tabs`/`wolf_close_tab`, CloseTab=17,
FfiTab/pane_tabs_for_grid/close_tab/switch_tab, and the macOS tab strip. Orchestrator
ran `prettier --write` on all 11 -> all unchanged/clean. Pre-existing unrelated
PaneGeometry maxCols nil-vs-90 discrepancy left untouched (out of scope).

Manual GUI smoke remaining (not automatable): launch Wolf, open several buffers in a
pane and confirm tabs accrue left->right with the active one highlighted; click a tab
to switch; hover to reveal the X and click it to close (buffer survives if tabbed in
another pane, deleted if last); ⌘W closes the active tab and closes the pane on the
last tab; ⌘W disabled on the last pane's last tab; the strip replaces the old
statusbar and dims with an inactive pane.

## Status history: technical-writer (docs)

### Fix pass COMPLETE (test-writer a70d52eef75b5d7e2, coder ae978edb74bcc7cad)
BLOCKER #1 fixed (build_ladder single-segment guard `basename_idx <= dir_start ->
vec![display]`), pinned by new tests `home_exact_path_is_tilde` +
`home_exact_path_coexists_with_home_child`. NIT #3 fixed (sub-closeSize tabs get a
zero-area close rect, unhittable; TabStripLayoutTests unchanged/passing). NIT #4 fixed
(`vim.bo[buf].modified`). Decision #5 buffer-preservation pinned in tabs.rs 5b. ALL
GREEN: wolf_core_lib_test (tab_label 10 + command 18), tabs (3x stable), commands,
WolfKeyMapTests, regressions (buffer_picker/panes/pane_status/bridge_smoke), full Wolf
app build. Denylist clean. No FFI surface change in the fix pass -> no rebinding.
Declined coder's optional NIT#3 hit-routing assertion (low value). Review satisfied.

## Status history: fix pass

### Reviewer verdict (agent aa45ddaeb1e55033a): CHANGES-REQUIRED

All 15 invariants + discriminant lockstep verified conformant. Findings:
- **BLOCKER #1** `crates/wolf-core/src/tab_label.rs:142` — `build_ladder` panics on a
  home-exact path (`pane_status` display == bare `"~"` -> `segs[1..0]` slice panic).
  Reachable via `:e ~`; panics inside `pane_tabs_for_grid` on the synchronous Swift
  paint path -> unwind across FFI -> app abort. Invariant 11 gap. Fix: guard
  single-segment case in `build_ladder` (when `basename_idx <= dir_start` /
  `segs.len()==1`, return `vec![display]`).
- **SHOULD-FIX #2** `crates/wolf-ffi/tests/tabs.rs:486-499` — Decision #5 case 5b
  asserts only window-count; the X affordance bypasses the menu disable, so the
  top-guard's buffer/tab preservation is user-reachable and must be pinned. Add:
  surviving tab count==1, tab still `life_z.md`, buffer still listed. (= PIN IT.)
- **NIT #3** `TabStripLayout.swift:63-64,76-79` — zero-width overflow tab's close rect
  placed before its origin can capture clicks (close-before-tab hit order). Skip close
  rects for tabs narrower than `closeSize`.
- **NIT #4** `init.lua:545` — deprecated `nvim_buf_get_option(buf,"modified")`; use
  `vim.bo[buf].modified`.
Open questions: (A) two [No Name] tabs identical -> reviewer concurs DECLINE.
(B) pin buffer-preservation -> reviewer concurs PIN IT (X bypasses menu gate).

### Fix routing
- test-writer: (a) tab_label.rs home-exact-path test `labels(&["/home/user"],
  Some(home), Some(home)) -> ["~"]`; (b) tabs.rs 5b buffer-preservation assertions.
- coder: BLOCKER #1 guard + NIT #3 + NIT #4; re-run full suite.

## Status history: reviewer

### Coder result (agent a0b5cb2210cc055b6): ALL TESTS GREEN

wolf_core_lib_test (tab_label 9 + command 18), //crates/wolf-ffi:tabs (real-nvim, 3x
stable), commands (guard 0..=17 + :WolfCloseTab->17), WolfKeyMapTests (TabStripLayout
+ CommandSeam thru 17), regressions (buffer_picker/panes/pane_status/bridge_smoke/
frame_pack/close_pane/copy_paste), wolf_ffi + wolf_macos_lib + full Wolf app build.
Denylist clean. Bindings regenerated; wolf_ffi_wake.h byte-identical. Also wired
TabStripLayout.swift/Tests into crates/wolf-macos/BUILD.bazel. Env fix: refreshed
stale Xcode-locator Bazel repos (26.5->26.6), not a code change.

### Coder test recommendations — orchestrator decisions
1. Docs (plan §6) NOT done -> route to technical-writer (step 8). ACCEPTED (planned).
2. Two unnamed [No Name] tabs render identically (matches pane_status) -> DECLINE
   pinning; acceptable v1.
3. Decision #5 buffer-preservation not asserted (Lua top-guard implements it) -> PIN
   IT. Route a targeted test-writer addition (batched with any reviewer findings).

### Diagnostics after coder: IDE/SourceKit noise only (no Bazel context) — vim global,
Rect/AppDelegate/WolfKeyMap module resolution. Bazel builds + WolfKeyMapTests + app
build all pass, so not real. Ignored.

## Status history: coder

User confirmed decision #3: no close-time prompt, keep the buffer (autosave coming
later). All four resolved open questions stand.

### Decision #5 (last-tab-of-last-pane) — RESOLVED

`wolf_close_tab(win, bufnr)` must be a COMPLETE NO-OP when it is the pane's last tab
AND the last window: guard at the TOP — if `#order == 1` (last tab in pane) and
`#nvim_tabpage_list_wins(0) == 1` (last window), return immediately WITHOUT removing
the tab, deleting the buffer, or closing the window. This keeps the tab, the buffer,
and the window intact (there is nowhere to go — you cannot close the last pane, so
closing its last tab is meaningless). The macOS menu additionally disables ⌘W in
this state (`paneCount<=1 && activeTabCount<=1`). The existing `tabs.rs` 5b test
asserts window-count-unchanged (passes under this semantics); buffer-preservation in
this exact case is a defensible-but-unpinned invariant (menu-disabled anyway) — may
strengthen later if the reviewer wants it.

### Test files written (test-writer, agent a50905bbbc937f30f)

- `crates/wolf-core/src/tab_label.rs` — NEW file, currently ONLY the `#[cfg(test)]
  mod tests` block + header banner (inert until coder adds the production body +
  lib.rs wiring). Coder MUST add `TabLabel`/`tab_labels` ABOVE the test module.
- `crates/wolf-core/src/command.rs` — count 17->18, relaxed buffer_picker index test,
  added close_tab tests.
- `crates/wolf-ffi/tests/tabs.rs` — NEW real-nvim: accumulation/isolation, preserve/
  delete/pane-lifecycle, left-neighbor+delete, right-neighbor view round-trip
  (flaky-flagged).
- `crates/wolf-ffi/tests/commands.rs` — guard array -> 0..=17 + :WolfCloseTab drain.
- `crates/wolf-macos/Tests/WolfKeyMapTests/TabStripLayoutTests.swift` — NEW pure seam.
- `crates/wolf-macos/Tests/WolfKeyMapTests/CommandSeamTests.swift` — through 17.

### Contracts the coder MUST honor (pinned by the tests)

- `TabLabel { bufnr: i64, display: String, active: bool, modified: bool }`.
- `tab_labels(&[String], Option<&Path>, Option<&Path>) -> Vec<String>`; disambiguation
  = MINIMAL parent-suffix of the path's pane_status display, expanded one parent at a
  time until unique, else full pane_status precedence (cwd-relative / `~` / absolute).
  Empty path -> `pane_status::NO_NAME`.
- `TabStripLayout.init(stripWidth:tabCount:tabWidth:tabHeight:closeSize:secondaryLineHeight:)`,
  `hit(x:y:) -> Hit?` with nested `enum Hit: Equatable { case tab(Int); case close(Int) }`;
  close rect flush to each tab's trailing edge, no extra padding param.
- FFI: `pane_tabs_for_grid(grid) -> Vec<FfiTab>` with `bufnr()/display()/active()/
  modified()`; `close_tab(win, bufnr)`; `switch_tab(win, bufnr)`.
- Build wiring: `pub mod tab_label;` + re-export in lib.rs; `//crates/wolf-ffi:tabs`
  rust_test in BUILD.bazel (copy buffer_picker stanza).

## Problem statement

Add per-pane tabs. Each pane (Neovim window) shows an ordered, accumulating row of
tabs, one per buffer shown in that pane. The active tab tracks the pane's current
buffer.

### Requirements

1. Switching buffers by ANY means (link, `:e`, buffer picker, file picker) updates
   the active tab for the pane.
2. If a pane has no tab for the current buffer, append a tab to the RIGHT.
3. Closing a tab does NOT delete the buffer unless it's the last tab for that buffer
   ACROSS ALL PANES.

### Locked decisions

- macOS front-end only this pass; core/init.lua/wolf-ffi model built
  platform-agnostically (wolf-gnome gets the model, no strip yet).
- Tab strip REPLACES the per-pane statusbar (tab row + active tab's path as a
  secondary line).
- ⌘W → new append-only `Command::CloseTab` (17). Closes active tab; last tab in pane
  falls through to closing the pane. Keep `ClosePane` (8) + `<C-w>c`. Menu validation
  disables ⌘W only at last-pane-last-tab. X-on-hover per tab.
- Click a tab to switch (win-explicit); on closing the active tab, activate RIGHT
  else LEFT neighbor.

### Resolved open questions (orchestrator decisions)

1. **Payload shape:** wholesale all-windows array `[[win, [[bufnr,name,modified],...],
   active_bufnr], ...]` (mirrors `wolf_buffers`, gets pruning for free). ADOPTED.
2. **Close Pane's ⌘W:** Close Pane keeps its `<C-w>c` chord, loses its menu
   key-equivalent (blank). ACCEPTED (consistent with locked decision).
3. **Modified buffer on last-tab close:** `nvim_buf_delete{force=false}` silently
   keeps a modified buffer (tab reappears on next emit). ACCEPTED for v1 (never lose
   unsaved work).
4. **Switch naming:** add a distinct win-explicit `switch_tab(win, bufnr)`; leave the
   buffer picker's no-arg `switch_buffer(bufnr)` untouched. ADOPTED.

---

## Architect's plan (adopted)

### 1. runtime/nvim/init.lua

- New `_G.wolf_setup_tabs(chan)` (after `wolf_setup_view_tracking`, ~line 491),
  modeled on `wolf_setup_buffer_tracking` (205-224) + `wolf_setup_view_tracking`
  (447-491).
  - Upvalue `win_tabs = {}` (`win` -> ordered array of `bufnr`).
  - `note(win)`: append the window's current buffer to its list if absent.
  - `emit()`: wholesale all-windows notify `wolf_tabs` = per-window
    `[win, [[bufnr, name, modified], ...], active_bufnr]`; skip invalid bufnrs;
    unnamed buffers still get a tab (empty name).
  - Augroup `WolfTabs`: `{BufEnter,BufWinEnter,BufFilePost,WinEnter,WinNew}` -> emit;
    `WinClosed` -> drop `win_tabs[match]` then emit; `BufDelete` -> remove bufnr from
    every list then emit. Initial emit.
- `_G.wolf_close_tab(win, bufnr)` (closes over `win_tabs`, `emit`), pcall-wrapped:
  1. find idx in `win_tabs[win]`; not found -> no-op.
  2. `was_active = nvim_win_get_buf(win) == bufnr`.
  3. `table.remove(order, idx)`.
  4. if `was_active` and `#order>0`: neighbor = `order[idx]` (right) else `order[idx-1]`
     (left); `wolf_switch_buffer(win, neighbor)` (reuse view restore).
  5. if `#order==0`: guarded pane close `if #nvim_tabpage_list_wins(0) > 1 then
     nvim_win_close(win, false) end`; `win_tabs[win] = nil`.
  6. orphan scan: if `bufnr` in NO `win_tabs[*]`, `pcall(nvim_buf_delete, bufnr,
     {force=false})`.
  7. `emit()`.
- Core wiring: `NvimProcess::setup_tabs` in nvim.rs (copy `setup_buffer_tracking`
  388-419); invoke in `WolfCore::start` AFTER `setup_view_tracking` (api.rs:262) so
  `wolf_switch_buffer` exists.

### 2. wolf-core

- New pure module `src/tab_label.rs` (precedent buffer_picker.rs):
  `TabLabel { bufnr, display, active, modified }`; `tab_labels(paths, cwd, home) ->
  Vec<String>` with basename base case + same-basename disambiguation (prepend parent
  dirs until unique; fall back to pane_status-style cwd-relative/`~`/absolute).
  Re-export from lib.rs.
- `render.rs`: `Snapshot.tabs: HashMap<i64, PaneTabs>`; `TabEntry { bufnr, path,
  modified }`; `PaneTabs { order: Vec<TabEntry>, active: Option<i64> }`. Wholesale
  replaced, no seq bump. Re-export.
- `redraw.rs`: `TABS_EVENT = "wolf_tabs"`; `apply_tabs` mirrors `apply_buffers`
  (327-351): build map before write guard, canonicalize non-empty paths, keep
  unnamed entries, wake without seq bump.
- `api.rs`: `WolfCore::pane_tabs(grid) -> Vec<TabLabel>` (join grid->win->tabs, drop
  guard before `live_cwd()`, format via `tab_labels`, mark active). `WolfCore::close_tab(win,
  bufnr) -> send_input(CloseTab)`.
- `input.rs`: `InputEvent::CloseTab { win, bufnr }`; apply arm -> `close_tab(nvim,
  win, bufnr)` invoking `wolf_close_tab` (copy `switch_buffer` 410-421).
- `command.rs`: append `Command::CloseTab` (17); COMMAND_SPECS row (id `close_tab`,
  ex `WolfCloseTab`, title `Close Tab`, menu `File`). Tests: `EXPECTED_COMMAND_COUNT`
  17->18; relax `buffer_picker_is_the_last_row` (now index 16); add
  `close_tab_is_the_last_row` + id/spec round-trip tests.

### 3. wolf-ffi (src/lib.rs)

- `mod command_kind`: `CLOSE_TAB: u8 = 17`; add `Command::CloseTab =>
  command_kind::CLOSE_TAB` to exhaustive `from_command`.
- `FfiTab` opaque (mirror FfiBufferRow/FfiPaneStatus): `bufnr/display/active/modified`;
  `WolfCoreHandle::pane_tabs_for_grid(grid) -> Vec<FfiTab>`.
- Handle methods `close_tab(win, bufnr)` -> `core.close_tab`; `switch_tab(win, bufnr)`
  -> `core.switch_buffer(win, bufnr)` (win-explicit — see risks).
- Regenerate committed bindings via `bazel run //crates/wolf-ffi:gen_bindings`
  (rewrites generated/wolf-ffi.swift + .h).
- BUILD.bazel: add `//crates/wolf-ffi:tabs` rust_test (copy buffer_picker stanza
  745-766).

### 4. wolf-macos

- New pure seam `Sources/WolfKeyMap/TabStripLayout.swift` (Foundation-only, precedent
  StatusbarLayout.swift): stripHeight, tabRects, closeRects (X hit rects),
  secondaryLineRect, `hit(point) -> .tab(i)|.close(i)|nil`. Tests
  `TabStripLayoutTests.swift`.
- `PaneContainerView.swift`: statusbar strip -> tab strip backed by TabStripLayout;
  replace `paintStatusbar` with `paintTabStrip` reading `pane_tabs_for_grid` (active
  highlight, inactive dim, hover fill, X-on-hover, secondary path via
  `pane_status_for_grid`); `mouseDown` hit-tests X (`close_tab(win, bufnr)`) vs tab
  (`focus_win(win)` + `switch_tab(win, bufnr)`); hovered-tab tracking via
  NSTrackingArea; keep pane-dim scrim.
- `AppMenu.swift`: `Tag.closeTab = 17`; add Close Tab item ⌘W; MOVE ⌘W off Close Pane
  (blank equivalent).
- `CommandSeam.swift`: `CommandKind.closeTab` + `case 17`. Update CommandSeamTests.
- `CommandDispatcher.swift`: `.closeTab -> controller?.closeTab()`.
- `WolfWindowController.swift`: `closeTab()` (resolve active_win -> grid_for_win ->
  pane_tabs_for_grid -> active tab -> `close_tab(win, bufnr)`); `activeTabCount`
  computed.
- `AppDelegate.swift` validateMenuItem: `.closeTab -> !(paneCount<=1 &&
  activeTabCount<=1)`.

### 5. Tests (real nvim preferred)

- Pure: `tab_label.rs` tests (unique, disambiguation, fallbacks, [No Name]);
  `command.rs` count/order tests.
- `crates/wolf-ffi/tests/tabs.rs` (model buffer_picker.rs + panes.rs for the split):
  (1) accumulation order across `:e`; (2) per-pane isolation with `:vsplit`;
  (3) CloseTab preserve-when-tabbed-elsewhere + right-else-left neighbor;
  (4) CloseTab delete-when-last; (5) last-tab-closes-pane + E444 no-op; (6) neighbor
  view round-trip.
- `TabStripLayoutTests.swift`; `CommandSeamTests.swift` -> 17;
  `crates/wolf-ffi/tests/commands.rs` guard array -> 0..=17 + `:WolfCloseTab` drain.

### 6. Docs

architecture.md (lines ~34, 43, 65, 156, 173, 180), wolf-core README + src/README,
wolf-ffi README, wolf-macos README + Sources/README, runtime/nvim/README.
prettier --write after each.

### 7. Order / risks

- Order: core (render->tab_label->redraw->input->api->command->nvim) -> init.lua ->
  wolf-ffi (+ regenerate) -> wolf-macos -> docs.
- Discriminant touchpoints move together 16->17: core Command + SPECS + count tests;
  ffi command_kind + exhaustive from_command; ffi commands.rs guard 0..=17; macOS
  CommandKind + AppMenu.Tag + CommandSeamTests.
- CloseTab is win+bufnr EXPLICIT (X can target an inactive tab in a non-active pane);
  front-end resolves active tab for the ⌘W path.
- Tab-click needs win-explicit `switch_tab` — the FFI `switch_buffer(bufnr)` resolves
  active_win live, which would switch the wrong pane; `focus_win` first doesn't help
  (active_win updates async).
- Last-pane-last-tab guarded 3 ways: Lua win-count guard, pcall'd buf_delete, menu
  validation.
- `wolf_tabs` is additive/independent of `wolf_buffers`/`current_file`; active tab
  always == pane current buffer.

Architect agent id: a7fd9bf4f6f1ba469.
