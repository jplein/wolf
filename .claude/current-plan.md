# Plan: Terminal Drawer + Command-Lifecycle README

## Status: FIX COMPLETE ✅ (pending user manual smoke-test of the rendered drawer)

## Fix finalization
Technical-writer updated all 8 docs (wolf-core/wolf-ffi src READMEs, runtime/nvim README, wolf-macos
Sources/Tests READMEs, wolf-ffi tests README, command-lifecycle.md, architecture.md); orchestrator ran
`prettier --write` → all 8 already conformant. Fix is code-complete, reviewer-APPROVED (FD6 nit
declined as out-of-scope), 43/43 core+ffi tests green + WolfKeyMapTests + real-nvim terminal.rs intent
assertion + app builds. Manual smoke-test of the RENDERED drawer + `<C-w>j/k` navigation is the only
remaining verification (AppKit layer, no headless coverage) — handed to the user.

## Fix progress log
- Architect base plan + navigation addendum complete; FD1–FD5 recorded. Test-writer wrote layout.rs
  parse tests, terminal.rs DrawerOpen intent assertion (confirmed `drain_intents`/`kind()`/`win()`
  exist), DrawerLayoutTests + DrawerFocusSeamTests, BUILD.bazel srcs. Two contact points pinned:
  `drawerUpTarget(lastEditorPane:leaves:rects:)` labeled; `drawerRect` height == drawerHeight.
- Coder complete + FULLY GREEN. Implemented base (DrawerOpen intent end-to-end, DrawerLayout seam,
  PaneContainerView drawer render) + navigation (DrawerFocusSeam, applyFocus bridge, lastEditorPane,
  init.lua t-mode maps + WolfTerminalInsert autocmd). Tests: `wolf_core_lib_test` PASS (layout parse
  tests live here — no separate layout_intents target), `//crates/wolf-ffi:terminal` PASS (real nvim,
  intent assertion exercised), `WolfKeyMapTests` PASS (both new seam tests), `//crates/wolf-ffi:all` +
  `//crates/wolf-core:all` 43/43 PASS, `//crates/wolf-macos:Wolf` builds. `wolf_ffi_wake.h`
  byte-identical AND entire `generated/` tree unchanged (only a discriminant const + from_intent arm →
  no bridge surface change; nothing to commit there). No `PaneTree.swift` change. Coder micro-note:
  drawer-origin `.up` computes leaf rects with `editorHeight` not `paneAreaHeight` — identical target
  (bottomMostLeaf is scale-invariant) and consistent with paneRects. Coder agent id: abc79d587dbe6ce1e.
- Coder recs: (1) applyFocus/applyDrawerOpen/detectCloses drawer branch are AppKit-only → manual-smoke
  is the only coverage for the rendered layer (matches plan). (2) Docs are technical-writer's pass.
- Invoking reviewer.
- Reviewer verdict: APPROVED WITH MINOR NOTES. Base fix + navigation bridge correct and conformant;
  invisible-drawer root cause resolved; lifecycle/focus-bridge edge cases handled; `PaneTree.swift`
  untouched; intent_kind append-only; `generated/` confirmed unchanged. One nit (drawer whose
  boundGrid never resolves before its window closes leaves `drawerPaneId` set) is a pre-existing race
  shared with editor panes → **FD6: DECLINE, out of scope** (consistent, negligible). Only outstanding:
  technical-writer docs. Reviewer agent id: aa5d0b7099c1751f5.
- Invoking technical-writer (fix-cycle docs).

## FIX CYCLE: terminal drawer opens but is not visible

### Bug report (from the user)
Running Terminal opens a terminal buffer (reachable from the buffer list) but it is NOT visible.
It IS the active buffer — typing goes into it; switching the visible pane's buffer to the terminal
via the buffer picker reveals the previously-typed text filling the canvas.

### Root cause (orchestrator-confirmed against the code)
In Wolf the SHELL (front-end) owns the pane layout, not Neovim. The macOS `PaneContainerView` creates
a visible pane ONLY when it drains a `wolf_layout` **Split intent** (emitted by `wolf_setup_layout`'s
`do_split`: it performs the split AND `rpcnotify`s `{action='split', orientation, from, win}`; the
front-end then adds a pane to its `PaneTree` and binds it to the new window's grid — layout.rs
`LayoutIntent::Split`; PaneContainerView.swift:595 `tree.split(...)`).

Our `wolf_toggle_terminal()` calls `vim.cmd("botright split")` DIRECTLY. That creates the real nvim
window + terminal buffer (so it becomes current, captures keystrokes, and `windows().len()` grows —
why the headless ffi test passed), but emits NO layout intent. So the front-end's `PaneTree` never
learns the pane exists → its grid is never painted → the prior editor pane keeps filling the canvas.
The headless test could not catch this (§5.4 territory: real rendering needs the AppKit front-end).

### User decision (chosen this cycle): BOTTOM SPLIT PANE VIA INTENTS
Make the drawer a real front-end pane by going through the layout-intent system, rendered as a
FULL-WIDTH bottom pane at ~1/3 height with DRAWER semantics (excluded from focus-dir navigation and
from close-pane; toggled by the Terminal command; buffer persists on hide). NOT a plain 50/50 peer
split of the focused pane. (Rejected alternative: a dedicated sidebar-like bottom region outside the
PaneTree — truer VS Code drawer but a much larger front-end build.)

### Design constraints for the architect
- The normal Split intent splits the `from` (focused) pane; a drawer must instead be a FULL-WIDTH
  child at the ROOT bottom regardless of which editor pane is focused, at a fixed ~1/3 weight. So a
  plain reuse of `do_split("h")` is insufficient — likely need a new intent flavor/flag (e.g.
  `action='drawer_open'` or a `drawer: true` field) plus PaneTree/PaneContainerView support for a
  drawer pane (root-bottom placement, fixed weight, non-navigable, removable on `win_close` without
  disturbing the editor subtree).
- The toggle helper needs the RPC channel to emit the intent (currently `wolf_toggle_terminal()` is
  argless). Resolve how it obtains `chan` (a `wolf_setup_terminal(chan)` storing a module global, or
  folding into `wolf_setup_layout`, or reusing the layout channel) — mirror existing setup patterns.
- Keep: per-project terminal BUFFER (tag `b:wolf_terminal` + `buftype=='terminal'`), per-tabpage
  drawer WINDOW, persist-on-hide (buffer/job survive `win_close`), recreate-if-gone.
- Hide path: closing the drawer window emits `win_close`; the front-end must remove the drawer pane
  (its existing close-observation path) and NOT leave a dangling PaneTree node.
- This is real front-end work → cannot be fully covered by the headless ffi test. Plan compile +
  targeted unit tests (PaneTree drawer placement/weight/removal is headlessly testable in
  `WolfKeyMapTests`) + a manual-smoke checklist for the actual rendered drawer.

### User refinement (this cycle): drawer is KEYBOARD-NAVIGABLE
`<C-w>j` from a bottom-edge editor pane focuses the terminal drawer; `<C-w>k` from the drawer focuses
the editor — INCLUDING from terminal-insert mode (a terminal-mode mapping intercepts it). Drawer stays
OUT of the pure `PaneTree` (no wolf-gnome parity break); navigation is done via explicit focus-dir
bridging in the front-end. Drawer is excluded from `<C-w>h/l` (full-width; nothing beside it), included
for `j/k` only. This EXTENDS the architect plan below (see "Navigation addendum").

### Orchestrator rulings on the architect's open questions (BINDING)
- **FD1 — ACCEPT R1**: Close-Pane while the drawer is focused closes (hides) the drawer; buffer
  persists. Hide-equivalent, no data loss. Acceptable v1; document.
- **FD2 — ACCEPT R2**: A split fired from within the drawer is dropped front-end-side (`applySplit`
  guard); the orphan nvim window is low-risk. Document as a limitation.
- **FD3 — ACCEPT R3**: Fixed ~1/3 drawer weight, no divider-drag resize (matches the editor tree's
  equal-weight v1 limitation).
- **FD4 — ACCEPT R4**: `pickInitialWindow` pre-existing-drawer edge left as-is (unreachable in the
  normal flow).
- **FD5 — CONFIRM R5**: `intent_kind::DRAWER_OPEN = 2` is append-only/last; never reorder.

### Fix architecture plan (architect — authoritative for the fix cycle)

**Approach:** New append-only `LayoutIntent::DrawerOpen { win }` flows Lua → `redraw.rs` (no change) →
IntentQueue → FFI `FfiLayoutIntent` (reuse existing `win`/`kind` fields — NO struct/method/wake-header
change; add `intent_kind::DRAWER_OPEN = 2`) → Swift `PaneContainerView`, which renders the drawer as a
FULL-WIDTH root-bottom pane at fixed ~1/3 weight, held in `panes` but OUTSIDE the pure `PaneTree`
(preserves PaneTree.swift's 1:1 parity with wolf-gnome). Navigation addendum (below) makes it
keyboard-navigable via focus bridging. Buffer/persist/recreate + `b:wolf_terminal` identity unchanged.

**2. Intent (new `LayoutIntent::DrawerOpen { win }`, chosen over a Split flag):**
- `crates/wolf-core/src/layout.rs`: add variant `DrawerOpen { win: i64 }` (after `FocusDir`) + doc; add
  `from_payload` arm `"drawer_open" => DrawerOpen { win: get("win")&handle_i64 }`; update doc blocks
  (module ~10-16 drawer asymmetry: PERFORM+report on open, guarded win_close on hide; from_payload
  ~74-78 third action).
- `redraw.rs` / `api.rs`: NO change (LAYOUT_EVENT arm + drain_intents are generic).
- `crates/wolf-ffi/src/lib.rs`: `mod intent_kind` append `pub const DRAWER_OPEN: u8 = 2;`; update
  `FfiLayoutIntent` doc/bridge comments (~655-671, 1841-1854); `from_intent` (~1892) arm
  `DrawerOpen { win } => Self { kind: DRAWER_OPEN, orientation: 0, dir: 0, from: 0, win }`. NO new
  bridged field/method. Regenerate bindings; verify `wolf_ffi_wake.h` byte-identical.

**3. Lua (`runtime/nvim/init.lua`):** Channel via the EXISTING layout channel — in `wolf_setup_layout`
(alongside `_G.wolf_do_split` etc., ~line 353) expose `_G.wolf_emit_drawer = function(win)
vim.rpcnotify(chan, "wolf_layout", { action = "drawer_open", win = win }) end`. In
`wolf_toggle_terminal` open path (after `botright <height>split` + set/create+tag the terminal buffer):
`local w = vim.api.nvim_get_current_win()`; `if _G.wolf_emit_drawer then _G.wolf_emit_drawer(w) end`;
then `startinsert`. Hide path UNCHANGED (`nvim_win_close(w, false)` → front-end observes `win_close`;
no intent). Buffer-find (tag + `buftype=='terminal'`) and recreate-if-gone UNCHANGED.

**4. New pure seam `crates/wolf-macos/Sources/WolfKeyMap/DrawerLayout.swift`** (Foundation-only, like
`EditorInset`/`PaneGeometry`): `init(paneAreaHeight, hasDrawer, weight)`; `editorHeight = hasDrawer ?
max(0, paneAreaHeight - drawerHeight) : paneAreaHeight`; `drawerHeight = hasDrawer ?
(paneAreaHeight*weight).rounded() : 0`; `drawerRect(width) -> Rect?` full-width strip at `y=editorHeight`;
`static let defaultWeight = 1.0/3.0`.

**5. `PaneContainerView.swift` wiring:** drawer binding kept in `panes` but its id NOT in `tree` (so
excluded from `leaves`/`neighbor`/`split`/`paneCount` for free). New state: `private var drawerPaneId:
PaneId?`; `private static let intentKindDrawerOpen: UInt8 = 2`.
- `applyIntents()` (~573): add `case intentKindDrawerOpen: applyDrawerOpen(win: intent.win())`.
- new `applyDrawerOpen(win:)`: idempotent (`paneForWin(win)!=nil` or `drawerPaneId!=nil` → return);
  else `id = allocPaneId(); drawerPaneId = id; panes[id] = PaneBinding(win, grid_for_win(win)?);
  setActivePane(id)`. Does NOT touch `tree`.
- `paneRects()` (~217): `dl = DrawerLayout(paneAreaHeight, hasDrawer: drawerPaneId != nil,
  weight: defaultWeight)`; tile the tree over `dl.editorHeight`; append `(drawerPaneId, dl.drawerRect)`
  if present. Every consumer (draw/mouse/scroll/syncGridSizes/scrollbars/dividers/paneRect(forGrid:))
  goes through paneRects → drawer paints/hit-tests/sizes for free.
- `detectCloses()` (~615): FIRST, if `drawerPaneId` bound-grid not in liveGrids → remove from `panes`,
  clear `drawerPaneId`, reassign active if needed — WITHOUT the `leaves().count<=1` guard and WITHOUT
  `tree.remove`. THEN the existing editor-close loop, skipping the drawer id.
- `applySplit()` (~591): after resolving `target`, `guard target != drawerPaneId else { return }`.
- `applyFocus()` (~603): editor height must be drawer-aware (see Navigation addendum for the full
  focus bridging). `syncGridSizes`/`draw`/`rebindStaleGrids`/`ensureInitialPane`: no change (rebind
  works via stable win handle; drawer rebinds across tabpage switch like any pane).
- Per-tabpage inherent: `PaneContainerView`/`drawerPaneId` are per-view; `Project` fans wake only to
  `currentView` → the DrawerOpen intent is drained by the right view; toggling tab B is independent.

**6. Focus (base):** open → `startinsert` → `WinEnter`→`wolf_active_win=drawerWin` →
`updateActivePane` marks drawer active/undimmed. Hide → `win_close` returns focus to editor. (Extended
by Navigation addendum.)

**7. Tests:** layout.rs parse unit tests (`from_payload_valid_drawer_open`,
`from_payload_drawer_missing_win_is_none`); extend real-nvim `crates/wolf-ffi/tests/terminal.rs` to
poll `drain_intents()` for `kind()==2 (DRAWER_OPEN)` with `win()!=0` after open (intents accumulate as
no front-end drains them); new `DrawerLayoutTests.swift` (no-drawer → full editor height + nil rect;
with-drawer → drawerHeight≈round(h/3), editorHeight=h-drawerHeight, full-width bottom rect; tiny-height
non-negative) + BUILD.bazel srcs entry. Manual-smoke checklist for the rendered drawer (open shows
bottom ~1/3; type paints; hide removes; buffer persists; reopen same session; recreate after exit;
multi-tab). Swift intent-kind has no WolfKeyMap contract test (AppKit-only) → rely on compile switch +
the real round-trip.

**8. Docs:** `runtime/nvim/README.md`, `crates/wolf-core/src/README.md` (layout.rs/new intent),
`crates/wolf-ffi/src/README.md` (`intent_kind::DRAWER_OPEN`), `crates/wolf-macos/Sources/README.md`
(DrawerLayout + PaneContainerView drawer), `crates/wolf-ffi/tests/README.md`, `command-lifecycle.md`
(Terminal now emits a layout intent), `architecture.md` (drawer = intent-driven non-tree pane).
`prettier --write` all.

**9. Order:** layout.rs → wolf-ffi (const+from_intent+regen bindings) → init.lua → DrawerLayout.swift →
DrawerLayoutTests → PaneContainerView → terminal.rs intent assertion → docs. **10.** No new deps; FFI
additive; doc comments on additions.

### Navigation addendum (architect — makes the drawer keyboard-navigable; extends the fix plan)

Verified facts: `applyFocus(dir:from:)` at `PaneContainerView.swift:603-611` resolves origin
`paneForWin(from) ?? activePane`, calls `tree.neighbor(...)`, and on hit `handle.focus_win(win)` — the
ONLY focus path. `focus_win` → `InputEvent::FocusWin` → `nvim_set_current_win`, which pulls nvim OUT of
the terminal window (auto-leaves terminal-insert). `paneForWin(drawerWin)` returns `drawerPaneId` (drawer
is in `panes`), but `tree.neighbor(drawerPaneId,…)` is always nil (not a tree leaf) → must intercept the
drawer-origin case BEFORE `tree.neighbor`. No existing "last active editor pane" field. `_G.wolf_do_focus`
/ `do_focus` (init.lua:343-355) emit `focus_dir` with `from=nvim_get_current_win()`; normal-mode maps at
367-374.

**1. New pure seam `crates/wolf-macos/Sources/WolfKeyMap/DrawerFocusSeam.swift`** (Foundation-only; keeps
`PaneTree` a faithful wolf-gnome port):
- `static func bottomMostLeaf(_ rects: [(PaneId, Rect)]) -> PaneId?` — leaf with greatest `y+h`
  (top-left origin → larger y = lower), tie-break smallest `x` then `PaneId`; nil on empty.
- `static func drawerUpTarget(lastEditorPane:leaves:rects:) -> PaneId?` — `lastEditorPane` if non-nil AND
  in `leaves`; else `bottomMostLeaf(rects)`.

**2. `PaneContainerView`:**
- Add `private var lastEditorPane: PaneId?` near `activePane` (~130). Update ONLY in `setActivePane(_:)`
  (~1165): set `lastEditorPane = pane` only when `pane != nil && pane != drawerPaneId &&
  tree.leaves().contains(pane!)` (do NOT overwrite when the drawer becomes active). All focus changes
  funnel through `setActivePane`, so one edit covers all.
- Rewrite `applyFocus(dir:from:)` (601-611): resolve origin (unchanged). THEN:
  - Drawer-origin (`origin == drawerPaneId`): `dir==.up` → `target =
    DrawerFocusSeam.drawerUpTarget(lastEditorPane, tree.leaves(), tree.paneRects(width:Double(bounds.width),
    height:Double(paneAreaHeight)))`; `guard let target, let win = panes[target]?.win`; `focus_win(win)`.
    `.down/.left/.right` → no-op. Return.
  - Editor-origin: `target = tree.neighbor(origin, dir, width, height)` (unchanged). If hit →
    `focus_win(panes[target].win)`. ELSE if `dir==.down && drawerPaneId != nil` →
    `focus_win(panes[drawerPaneId!].win)` (bottom-edge into drawer). Else no-op.
  - Only calls `focus_win`; let `updateActivePane` reconcile `activePane` next wake (no `setActivePane`
    here). Rects MUST use the same width/paneAreaHeight as the neighbour call.

**3. `runtime/nvim/init.lua` `wolf_setup_layout`:**
- Normal mode: NO new map (terminal-NORMAL `<C-w>k` already emits `focus_dir` from the drawer window).
- Add GLOBAL terminal-mode maps after line 374 (reuse `map`/`silent`/`do_focus`):
  `map("t","<C-w>h",function() do_focus("left") end, silent)` and likewise `j→down`, `k→up`, `l→right`.
  A `t`-mode function/`<cmd>` RHS fires WITHOUT leaving terminal mode or forwarding to the shell; `from`
  = the drawer window. TRADEOFF (document): this shadows the shell's `<C-w>` word-erase — accepted per
  the user's request.
- §2c re-entry: add augroup `WolfTerminalInsert` (clear) with an autocmd on `{"WinEnter","BufWinEnter"}`:
  `if vim.b.wolf_terminal and vim.bo.buftype=="terminal" then vim.cmd("startinsert") end` — auto-enter
  terminal-insert when focusing INTO the drawer via `<C-w>j` (VS Code parity). Does NOT yank back from a
  deliberate `<C-\><C-n>` (same window, no WinEnter). Droppable with no other change if undesired.

**4. Lifecycle confirmations:** `<C-w>k` moves focus only (no win_close → drawer stays open; editor
un-dims via `updateActivePane` + `paintPane dimmed: id != activePane` at 686). `<C-w>j` back re-enters
insert via §2c. Drawer out-of-tree → leaving it never triggers `detectCloses` editor pruning.

**5. Tests:** new `DrawerFocusSeamTests.swift` (WolfKeyMapTests): `bottomMostLeaf` single/vertical-2/2×2
+ empty; `drawerUpTarget` returns MRU when in leaves, falls back when nil or stale. The `applyFocus` glue
is AppKit → manual-smoke (see 4b in addendum: `<C-w>j` editor→terminal; `<C-w>k` terminal→editor from
BOTH insert & normal; drawer stays open; 2×2 top-pane `<C-w>j` goes to pane below not drawer; `h/l` in
drawer no-op; re-entry lands in insert). Optional `terminal.rs`: send `<C-w>k` and observe a `focus_dir`
intent surfaces — ONLY if an ergonomic layout-intent accessor already exists on `WolfCoreHandle`
(check `panes.rs`); do NOT add FFI surface just for the test.

**6. Docs delta:** `runtime/nvim/README.md` (t-mode maps + shadow tradeoff + WolfTerminalInsert autocmd);
`crates/wolf-macos/Sources/README.md` (applyFocus drawer bridging, `lastEditorPane`, `DrawerFocusSeam`);
`crates/wolf-macos/Tests/README.md` (`DrawerFocusSeamTests`); `architecture.md` (one sentence: editor↔drawer
focus is a front-end bridge over `focus_dir`, drawer excluded from `PaneTree`).

**7. Order (navigation):** DrawerFocusSeam.swift + tests → `lastEditorPane`/`setActivePane` → rewrite
`applyFocus` → init.lua t-maps + autocmd → docs. No new deps/lint directives. Navigation architect
follow-up agent id: a6ac30da3c68cbdd2.

---

## (previous cycle, COMPLETE) Terminal command + command-lifecycle README

## Finalization

Technical-writer wrote the new `command-lifecycle.md` (8-section end-to-end walkthrough + three-flavor
add-a-command checklist) and updated all affected READMEs plus `architecture.md`/root `README.md`
links; it also corrected four docs that had gone stale ("eighteen commands"/`0..17`). Orchestrator ran
`prettier --write` on all 12 touched Markdown files → 11 already conformant, 1
(`crates/wolf-macos/Sources/README.md`) reformatted. Test-writer removed the erroneous "(OD4)" in the
`palette_rank.rs` comment. Final change set verified via `git status`: 25 modified + 2 new
(`command-lifecycle.md`, `crates/wolf-ffi/tests/terminal.rs`); `architecture.md:158` + `README.md:18`
carry the lifecycle-doc link. All tests green (coder + re-run), reviewer APPROVED, `wolf_ffi_wake.h`
byte-identical, bindings regenerated (uncommitted, per commit-only-when-asked). Technical-writer agent
id: abfebe24879411ed2.

## Problem statement

Two deliverables in one coordinated change.

### Feature 1: Terminal drawer command

Add a new command **Terminal** (id `terminal`, ex-command `WolfTerminal`, menu `View`)
that toggles a single `:terminal` drawer pane covering the bottom ~1/3 of the current
tabpage/window.

Behavior requirements (from the user):

- **Toggle**: hidden → open; visible → hide.
- **Persist across toggle**: hiding keeps the terminal job/buffer alive (VS Code
  behavior). Do NOT kill the shell on hide.
- **Scope**: ONE terminal per PROJECT. Wolf runs one captive Neovim per project with
  one tabpage per view. User said "one per project" → a single shared terminal buffer
  for the Neovim instance. Architect to confirm shared-across-tabs vs per-tabpage and
  state the decision explicitly.
- **Recreate if gone**: if the terminal buffer no longer exists (user `:q`'d / exited
  the shell), recreate it on next toggle.
- **Identifiable buffer**: tag with a buffer-local var (e.g. `b:wolf_terminal`) and/or
  track its bufnr so toggle logic reliably finds the right terminal. (This was the
  user's original design question — answer: tag/track the buffer, do not rely on
  `:terminal` opening in the current window.)

Implementation follows the established command lifecycle. Heavy lifting lives in a
`wolf_toggle_terminal()` Lua helper in `runtime/nvim/init.lua`, invoked via a new
`InputEvent::ToggleTerminal` from `input.rs`, reached via a new `WolfCoreHandle` FFI
method + `command_kind` discriminant, dispatched by the macOS `CommandDispatcher`.

Command-configuration checklist (file map from orchestrator's investigation — architect
to verify each before implementation):

1. `crates/wolf-core/src/command.rs` — add `Command::Terminal` (append LAST) +
   `COMMAND_SPECS` row; update count/index tests (EXPECTED_COMMAND_COUNT 18→19, add a
   `terminal_is_the_last_row` guard, move `NewTab` off "last row").
2. `crates/wolf-ffi/src/lib.rs` — add `command_kind::TERMINAL = 18`,
   `FfiCommand::from_command` arm, and `WolfCoreHandle::toggle_terminal()` sending
   `InputEvent::ToggleTerminal`. Regenerate bindings via
   `bazel run //crates/wolf-ffi:gen_bindings`.
3. `crates/wolf-core/src/input.rs` — add `InputEvent::ToggleTerminal` variant + `apply`
   arm invoking the Lua helper over `nvim_exec_lua`.
4. `runtime/nvim/init.lua` — add `wolf_toggle_terminal()` (open/hide/recreate + bottom
   ~1/3 split, tag buffer `b:wolf_terminal`).
5. macOS front-end: `CommandSeam.swift` (`CommandKind.terminal` + ffi decode range 0..18),
   `CommandDispatcher.swift` (dispatch arm), `WolfWindowController.swift`
   (`toggleTerminal()` → `handle?.toggle_terminal()`), View-menu wiring if commands get
   menu items there.
6. Command palette auto-includes it (all `COMMAND_SPECS` except `CommandPalette` become
   rows) — verify no extra work needed.

### Feature 2: Command-lifecycle README

Create a NEW standalone markdown file (e.g. `command-lifecycle.md` at repo root, matching
`architecture.md`/`build.md`) documenting the places a command is configured and how it
interacts with Neovim: the `Command` registry (source of truth) + its two faces (Neovim
Ex-command/Lua via `wolf_setup_commands`; native-menu FFI `command_kind`), the
invocation→drain path (`wolf_command` notify → `redraw.rs` → `Command::from_id` →
`drain_commands` → FFI `FfiCommand` → Swift dispatch), the `CommandDispatcher`
native-action mapping, and the editor-affecting path back into Neovim (`WolfCoreHandle`
method → `InputEvent` → `input.rs::apply` → `nvim_command`/`nvim_exec_lua` helper).
Include the step-by-step checklist for adding a new command. Link from `architecture.md`.
Run `prettier --write` after editing.

### Testing (prefer real Neovim integration tests over mocks)

- Rust registry round-trip/index tests in `command.rs`.
- Integration test starting a real pinned Neovim, invoking the terminal toggle, asserting
  a terminal buffer appears/hides and persists/recreates — model on
  `crates/wolf-ffi/tests/file_actions.rs` and `crates/wolf-core/tests/*.rs`.
- Update every affected directory `README.md` and the wolf-ffi generated-bindings note.

## Constraints (from CLAUDE.md + repo conventions)

- Documentation is the source of truth; keep every touched dir's README + `architecture.md`
  in sync.
- Each source directory must have a README describing its interfaces.
- Markdown: no hard line breaks within a paragraph; run `prettier --write` after editing.
- wolf-core hard rule: no GUI toolkit in its dependency tree.
- macOS builds wolf-core/wolf-ffi via Bazel; regenerate committed swift-bridge bindings
  after any bridge change (`bazel run //crates/wolf-ffi:gen_bindings`).
- Prefer e2e/integration tests that spawn a REAL pinned nvim.
- Command / `command_kind` discriminant contract is APPEND-ONLY.

## Progress log

- (init) Problem statement written; invoking architect.
- Architect plan complete (below). Orchestrator decisions OD1–OD7 recorded. Invoking test-writer.
- Test-writer complete. Wrote: `command.rs` registry tests (count→19, TERMINAL_INDEX, demoted
  NewTab guard → `new_tab_is_at_index_17`, `terminal_id_round_trips`/`terminal_spec_fields`/
  `terminal_is_the_last_row`); extended `wolf-ffi/tests/commands.rs` (CMD_KIND_TERMINAL=18,
  contract-guard array 0..=18, `:WolfTerminal` drain step in the existing test); new
  `wolf-ffi/tests/terminal.rs` (`terminal_toggle_persists_and_recreates`); `wolf-ffi/BUILD.bazel`
  `rust_test(name="terminal")`; Swift `CommandSeamTests.swift` (ffi 18→`.terminal`, out-of-range
  bumped 18→19, doc range 0..18). Verified all assumed FFI/Swift APIs exist:
  `windows().len()`, `buffer_picker().ranked(String::new(), 100)`, `FfiBufferRow::bufnr()/display()`,
  `send_key(FfiKeyEvent{ctrl:true,…})`; `term://` detection confirmed sound (picker `display()`
  retains prefix; terminal buffers are `buflisted`). Tests intentionally reference not-yet-existing
  symbols (`Command::Terminal`, `command_kind::TERMINAL`, `toggle_terminal()`,
  `InputEvent::ToggleTerminal`, `wolf_toggle_terminal()`, Swift `.terminal`) — coder lands those.
  IMPORTANT for coder: keep the plan §4 helper's `startinsert`/terminal-focus-on-open, else
  test step 5's `<C-\><C-n>` + `:bwipeout!` may target the wrong buffer. Test-writer agent id:
  a40d900c161e9a92a.
- Invoking coder.
- Coder complete. Implemented §3–§7 (Rust core + init.lua + FFI + regenerated bindings + Swift).
  `wolf_ffi_wake.h` byte-identical (OD5 ✅). Tests: `command` unit PASS; `//crates/wolf-ffi:commands`
  PASS; `//crates/wolf-ffi:terminal` PASS (real nvim, 0.54s); `//crates/wolf-macos:WolfKeyMapTests`
  PASS; `//crates/wolf-ffi:all` PASS; `//crates/wolf-macos:Wolf` builds. ONE regression FAIL:
  `//crates/wolf-core:palette_rank` — stale hardcoded `STATIC_COMMAND_COUNT = 17` (line 38); adding
  a palette-eligible command correctly grows it to 18. Coder's impl is CORRECT (Terminal SHOULD be a
  palette candidate); the test is stale and was outside the test-writer's original scope. Bindings
  regenerated on disk but NOT committed (correct — commit only when the user asks + after green).
  Pre-existing clippy lint at `wolf-ffi/src/lib.rs:1193` (`map_or`→`is_none_or`) is NOT in the diff
  and NOT a build/test gate — ignore. Coder agent id: a9d4671756cca7f22.
- **OD8 — ACCEPT coder rec: bump `STATIC_COMMAND_COUNT` 17→18 in `crates/wolf-core/tests/palette_rank.rs`**
  (+ update its doc comment to mention Terminal appended at 18). Registry grew by one palette-eligible
  command. Test change → dispatched to test-writer, then coder re-runs `//crates/wolf-core:palette_rank`.
- Invoking test-writer for the palette_rank constant fix.
- Test-writer bumped `STATIC_COMMAND_COUNT` 17→18 + doc. Coder re-ran: `//crates/wolf-core:palette_rank`
  PASS, `//crates/wolf-core:all` ALL 17 targets PASS. Zero failures. Implementation COMPLETE + green.
  Feature 2 docs (`command-lifecycle.md` + READMEs) still outstanding → technical-writer.
- Invoking reviewer.
- Reviewer verdict: **APPROVED**. Full plan conformance §3–§8; Lua helper correct on every failure
  mode (buffer find requires tag AND `buftype=='terminal'`; hide via `nvim_win_close(w,false)` persists
  buffer; no empty-split/last-window-crash/double-drawer paths; per-tabpage window + per-project buffer
  scope holds; recreate-if-gone works; `math.max(1,…)` guards 0-height). Tests adequate. wolf-core
  GUI-free; append-only contract intact. ONE non-blocking nit: `palette_rank.rs:37-38` doc comment
  mislabels the CommandPalette self-exclusion as "(OD4)" (OD4 is the ⌃` accelerator). Reviewer agent
  id: ac15e94fd99110857.
- **OD9 — route the palette_rank comment nit to test-writer** (fix the "(OD4)" mislabel). Trivial;
  runs in parallel with the technical-writer docs pass (independent files).
- Invoking technical-writer (Feature 2 docs) + test-writer (nit) in parallel.
- Test-writer fixed the palette_rank "(OD4)" comment nit (comment-only, value/assertions unchanged).
  Test-writer agent id: ac9ba9eb23f630bcf. Awaiting technical-writer.

## Orchestrator decisions on the architect's open questions (BINDING — all agents follow these)

- **OD1 — ACCEPT `nvim_win_close(w, false)`.** The drawer always coexists with an editor
  window today; terminal buffers are never `'modified'`, so force=false hides the window
  without deleting the buffer and cannot hit "can't close last window". Correct as designed.
- **OD2 — ACCEPT height `math.floor(vim.o.lines / 3)` via `botright <n>split`** for a
  full-width bottom drawer ~1/3 tall. Easy to tweak later; no further analysis needed.
- **OD3 — ACCEPT the per-tabpage window / per-project buffer model exactly as the architect
  specified.** One shared terminal BUFFER per project (tagged `b:wolf_terminal` AND
  `buftype == 'terminal'`); the drawer WINDOW is per-tabpage. Toggle acts only on the current
  tabpage's windows. Invoking Terminal from a different tabpage opens a second window onto the
  SAME buffer; each tabpage's toggle is independent. The headless ffi test need NOT cover the
  cross-tabpage case — rely on the Lua design + manual QA. Document the semantics.
- **OD4 — ACCEPT the View-menu accelerator `` ⌃` `` (Ctrl-backtick).** The feature is explicitly
  modeled on VS Code, which uses that chord; Cmd-T is taken by New Tab. The menu accelerator
  fires at the AppKit layer, so `` ⌃` `` toggles/hides even when the terminal has focus (desired
  VS Code behavior). `keyEquivalent: "`"`, `modifiers: .control`.
- **OD5 — ACCEPT: regenerate and COMMIT the swift-bridge bindings in the same change**
  (`bazel run //crates/wolf-ffi:gen_bindings`). Confirmed additive: no new `extern "C"` wake
  symbol, so `generated/wolf_ffi_wake.h` must remain byte-identical (coder/reviewer verify).
- **OD6 — ACCEPT: test-writer must locate the exact Swift `CommandKind` decode/contract test**
  in `crates/wolf-macos/Tests/` and extend it for discriminant `18` / `.terminal` (mirror how
  `newTab` was added). If no such test exists, note that and rely on the Rust-side contract
  guard + the compile-time exhaustive match.
- **OD7 — ACCEPT `term://` naming-convention detection in the real-nvim test, corroborated by
  the `b:wolf_terminal` tag / `buftype == 'terminal'`.** If it proves brittle across the pinned
  nvim, the documented fallback is a tiny read-only FFI probe — but do NOT add that unless the
  convention actually fails. Keep the change additive.

None of OD1–OD7 require re-consulting the user; the VS-Code-style `` ⌃` `` accelerator (OD4) is a
sensible default for a feature the user explicitly compared to VS Code.

---

# Architect Plan (authoritative — implement against this + the OD1–OD7 rulings)

## 1. Summary

Add a new registry command **Terminal** (id `terminal`, ex-command `WolfTerminal`,
`MenuGroup::View`, discriminant `18`) that toggles a per-tabpage bottom drawer window showing a
single per-project `:terminal` buffer. Mirrors the `NewBuffer` lifecycle exactly (Command → FFI
`command_kind` + `WolfCoreHandle` method → `InputEvent` → `input.rs::apply`), but the editor-side
work is a `wolf_toggle_terminal()` Lua helper in `init.lua` (the `SwitchBuffer`/`wolf_switch_buffer`
pattern of invoking a helper via `nvim_exec_lua`). Second deliverable: a new standalone
`command-lifecycle.md` documenting where a command is configured and how it reaches Neovim, linked
from `architecture.md`.

## 2. Per-project vs per-tabpage decision (see OD3)

Wolf runs one captive nvim per **Project**; each front-end view is one nvim **tabpage**. In Neovim a
**buffer is global** to the nvim; a **window** showing it belongs to a tabpage. Therefore:
- **One shared terminal BUFFER per project** — identified by `b:wolf_terminal = true` AND
  `buftype == 'terminal'`. Same shell/job reused everywhere in that nvim.
- **The drawer WINDOW is per-tabpage.** Toggle operates only on the current tabpage's windows.

Toggle semantics: in tabpage T, if a window in T shows the tagged buffer → close (hide) it
(buffer/job persist); else open a `botright` split in T showing the shared buffer (create+tag if it
does not exist). Drawer open in tabpage A + Terminal invoked from B → B opens its own window onto the
same buffer; A unaffected. Focus: on open, focus enters the terminal + `startinsert` (VS Code); on
hide (`nvim_win_close`) nvim returns focus to the editor automatically.

## 3. Rust core

### 3.1 `crates/wolf-core/src/command.rs`
- Append variant `Terminal` after `NewTab` (discriminant 18) with doc comment ("Toggle the terminal
  drawer (a bottom `:terminal` split, one per project).").
- Update the enum module doc list (lines 27–31) to append `Terminal`(18).
- Append a `COMMAND_SPECS` row after `NewTab`: `command: Command::Terminal, id: "terminal",
  ex_command: "WolfTerminal", title: "Terminal", menu: MenuGroup::View`.
- Tests (mirror NewTab exactly):
  - `EXPECTED_COMMAND_COUNT`: 18 → **19**; update its doc comment.
  - Add `const TERMINAL_INDEX: usize = 18;`.
  - Demote `new_tab_is_the_last_row` → rename `new_tab_is_at_index_17`, drop the `len()-1`
    assertion (keep the index-17 guard); update `NEW_TAB_INDEX` doc (no longer last).
  - Add `terminal_id_round_trips` (`from_id("terminal") == Some(Command::Terminal)`).
  - Add `terminal_spec_fields` (id `"terminal"`, ex `"WolfTerminal"`, title `"Terminal"`, menu
    `MenuGroup::View`).
  - Add `terminal_is_the_last_row` (row at `TERMINAL_INDEX` is `Command::Terminal` AND
    `TERMINAL_INDEX == COMMAND_SPECS.len() - 1`).
  - Update `command_specs_has_expected_len` message text.

### 3.2 `crates/wolf-core/src/input.rs`
- Add unit variant `ToggleTerminal` to `InputEvent` (after `SwitchBuffer`) with a doc comment
  describing the `wolf_toggle_terminal` delegation.
- Add `apply` arm (near line 385): `InputEvent::ToggleTerminal => toggle_terminal(nvim).await,`.
- Add an async helper (no args), mirroring `switch_buffer`/`close_tabpage`:

```rust
async fn toggle_terminal(nvim: &Nvim) -> Result<(), Box<CallError>> {
    let _ = nvim.call("nvim_exec_lua",
        vec![Value::from("wolf_toggle_terminal()"), Value::Array(vec![])]).await?;
    Ok(())
}
```

### 3.3 `crates/wolf-core/src/api.rs`
- Add a public wrapper (near line 473), mirroring `close_pane`/`switch_buffer`:

```rust
pub fn toggle_terminal(&self) {
    self.send_input(InputEvent::ToggleTerminal);
}
```

## 4. `runtime/nvim/init.lua`

Add a module-level global (like `wolf_create_tabpage`/`wolf_close_tabpage`, near line 468 — no setup
fn / channel needed; no args, no upvalues):

```lua
-- Toggle the per-project terminal drawer. The terminal BUFFER is a single shared
-- buffer for this nvim instance (tagged b:wolf_terminal); the drawer WINDOW is
-- per-tabpage. Hiding closes the window only -- the terminal buffer/job persist
-- (VS Code behavior). Recreated on demand if the user exited the shell.
function _G.wolf_toggle_terminal()
	-- 1. Find the tagged terminal buffer (buffers are global to the nvim).
	local termbuf = nil
	for _, b in ipairs(vim.api.nvim_list_bufs()) do
		if vim.api.nvim_buf_is_valid(b)
			and vim.b[b].wolf_terminal
			and vim.bo[b].buftype == "terminal" then
			termbuf = b
			break
		end
	end

	-- 2. If a window in the CURRENT tabpage shows it, hide (close) that window.
	--    Closing the window keeps the terminal buffer + job alive.
	if termbuf then
		for _, w in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
			if vim.api.nvim_win_get_buf(w) == termbuf then
				vim.api.nvim_win_close(w, false)
				return
			end
		end
	end

	-- 3. Otherwise open a bottom split ~1/3 tall showing the terminal
	--    (creating + tagging a fresh one if it is gone).
	local height = math.max(1, math.floor(vim.o.lines / 3))
	vim.cmd("botright " .. height .. "split")
	if termbuf and vim.api.nvim_buf_is_valid(termbuf) then
		vim.api.nvim_win_set_buf(0, termbuf)
	else
		vim.cmd("terminal")
		vim.b.wolf_terminal = true
	end
	vim.cmd("startinsert")
end
```

Notes to document: height ~1/3 = `math.floor(vim.o.lines / 3)`; `botright` = full-width bottom split;
`nvim_win_close(w, false)` is safe (terminals never `'modified'`; drawer never the sole window); the
`buftype == 'terminal'` guard avoids matching a stale tagged non-terminal buffer. No `wolf_setup_*`
change needed (plain module-level global reached via `nvim_exec_lua`, like `wolf_close_tabpage`).

## 5. `crates/wolf-ffi/src/lib.rs`

- `mod command_kind` (after `NEW_TAB` at line 177): add `pub const TERMINAL: u8 = 18;` (doc comment).
- Update `FfiCommand::kind` doc (line 1925) and the bridge comment (line 671) range text to
  `.. TERMINAL`.
- `FfiCommand::from_command` (line ~1953): add arm `Command::Terminal => command_kind::TERMINAL,`.
- `extern "Rust"` block for `WolfCoreHandle` (near `fn new_buffer(&self);` line 446): add
  `fn toggle_terminal(&self);` (doc comment).
- `impl WolfCoreHandle` (near `new_buffer` line 1666):

```rust
/// Toggles the per-project terminal drawer via [`InputEvent::ToggleTerminal`]
/// (the `wolf_toggle_terminal` Lua helper). A no-op once the core is gone.
pub fn toggle_terminal(&self) {
    if let Some(core) = self.core.borrow().as_ref() {
        core.send_input(InputEvent::ToggleTerminal);
    }
}
```

- **Regenerate bindings:** `bazel run //crates/wolf-ffi:gen_bindings` (target at BUILD.bazel line 81);
  commit refreshed `crates/wolf-ffi/generated/`. `generated/wolf_ffi_wake.h` MUST be byte-identical
  (additive only — OD5).

## 6. Swift front-end

### 6.1 `crates/wolf-macos/Sources/WolfKeyMap/CommandSeam.swift`
- Add `case terminal` after `case newTab` in `CommandKind`.
- Add `case 18: self = .terminal` in `init?(ffi:)` before `default`.
- Update the doc comment (lines 20–23, 62–64): include `terminal == 18` and range `0..18`.

### 6.2 `crates/wolf-macos/Sources/WolfMacOS/CommandDispatcher.swift`
- Add arm after `.newTab`: `case .terminal: controller?.toggleTerminal()`.

### 6.3 `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift`
- Add near `newBuffer()` (line 523):

```swift
/// Toggles this window's per-project terminal drawer (`:WolfTerminal`).
func toggleTerminal() {
    handle?.toggle_terminal()
}
```

### 6.4 `crates/wolf-macos/Sources/WolfMacOS/AppMenu.swift`
Menu is hardcoded (not registry-driven), so add a View-menu item (Terminal.menu == View):
- Add `static let terminal = 18` to the `Tag` enum (after `newTab`).
- In `viewMenuItem` (after the Buffer Picker item, line ~200):

```swift
menu.addItem(
    commandItem(
        title: "Terminal", tag: Tag.terminal, keyEquivalent: "`",
        target: target, modifiers: .control))
```

(`commandItem` supports `modifiers:` defaulting to `.command`; `` ⌃` `` per OD4.) Update the
`viewMenuItem` doc comment. No `validateMenuItem` change (Terminal always enabled → `default: return
true` at AppDelegate line 486).

## 7. Command-lifecycle README (Feature 2)

Create `command-lifecycle.md` at repo root (alongside `architecture.md`/`build.md`). Sections:
1. **Overview** — a Command is a no-parameter, no-return user action; one canonical snake_case id
   derives two faces; variant order is the append-only FFI discriminant contract.
2. **The registry (source of truth)** — `command.rs`: `Command`, `COMMAND_SPECS`
   (id/ex_command/title/menu), `MenuGroup`, `from_id`/`spec`; variant-order == discriminant rule.
3. **Face A — Neovim Ex-command + Lua** — `nvim.rs::setup_commands` → `wolf_setup_commands` in
   `init.lua` creates `:Wolf<Pascal>` and `require('wolf').<id>()`, both `rpcnotify`ing
   `wolf_command <id>`.
4. **Face B — native menu FFI** — `command_kind` in `wolf-ffi/src/lib.rs` → Swift `CommandKind(ffi:)`
   in `CommandSeam.swift`; `AppMenu` `Tag` carries the discriminant in `NSMenuItem.tag`.
5. **Invocation → drain** — `:WolfXxx` → `wolf_command` notify → `redraw.rs` (`Command::from_id`) →
   `CommandQueue` → `api.rs::drain_commands` → FFI `FfiCommand::from_command` → Swift
   `WolfWindowController.drainAndDispatchCommands`.
6. **Dispatch** — `CommandDispatcher.dispatch` maps `CommandKind` → native action; same funnel for
   the menu path (`AppDelegate.handleCommandMenuItem`) and the palette (`activatePaletteRow`).
7. **Editor-affecting path back into nvim** — `WolfCoreHandle::<method>` →
   `WolfCore::send_input(InputEvent::…)` → `input.rs::apply` → `nvim_command` (simple) or a `wolf_*`
   `nvim_exec_lua` helper (complex). Worked examples: `NewBuffer` (`:enew`), `SwitchBuffer`
   (`wolf_switch_buffer`), `Terminal` (`wolf_toggle_terminal`).
8. **Checklist: adding a new command** — three flavors:
   - **(a) Native-only** (e.g. `MinimizeWindow`): registry row + `command_kind` + `CommandSeam` case +
     `CommandDispatcher` arm → AppKit action; no core InputEvent.
   - **(b) Editor-affecting** (e.g. `NewBuffer`, `Terminal`): (a) PLUS `WolfCoreHandle` method +
     `InputEvent` variant + `input.rs::apply` arm (+ `init.lua` helper if non-trivial), regenerate
     bindings.
   - **(c) Front-end overlay** (e.g. `CommandPalette`, `BufferPicker`, `GoToFile`): (a); dispatcher
     opens a Swift overlay; no InputEvent. Note `CommandPalette` is excluded from the palette list.
   - Shared steps: append variant (never reorder); add `COMMAND_SPECS` row; bump
     `EXPECTED_COMMAND_COUNT` + add index/round-trip/spec tests; add `command_kind` const; extend
     `CommandSeam` `init?(ffi:)`; extend the `commands.rs` contract-guard array; add an `AppMenu`
     `Tag` + item if it has a menu group; update READMEs; run `prettier --write`.

Link from `architecture.md` §10 (line ~150) with "See [Command lifecycle](command-lifecycle.md)."
Optionally add to root `README.md` doc list. Run `prettier --write` on both edited md files.

## 8. Test plan (prefer real pinned nvim)

### 8.1 Registry unit tests — `command.rs` (see §3.1).

### 8.2 Extend `crates/wolf-ffi/tests/commands.rs`
- Add `const CMD_KIND_TERMINAL: u8 = 18;` (doc).
- Extend the contract-guard array (lines 174–197) to include `CMD_KIND_TERMINAL` and `[0..=18]`;
  update the message to `0..=18`.
- Add a step after the NewTab step: `send_ex_command(&handle, ":WolfTerminal");
  poll_for_command_kind(&handle, CMD_KIND_TERMINAL, ":WolfTerminal");` (pure FFI drain — proves the
  Ex-command face registers + drains discriminant 18). No BUILD change (target already lists src).

### 8.3 New real-nvim behavior test — `crates/wolf-ffi/tests/terminal.rs`
Model on `tests/file_actions.rs` (same `common` module, SKIP-when-unset, poll-until-attached). Drive
via `handle.toggle_terminal()` (the FFI method actually runs the helper; the Ex-command would only
enqueue an FfiCommand with no dispatcher). Detect terminal buffers via `buffer_picker()` (terminal
buffers are `buflisted`; display name prefixed `term://`) and window open/hide via `windows()`.
Single `#[test] fn terminal_toggle_persists_and_recreates()`:
1. Attach; `let base = handle.windows().len();`.
2. **Open**: `toggle_terminal()`; poll `windows().len() == base + 1`. Assert exactly one
   `buffer_picker().ranked("",100)` row whose `display()` starts `"term://"`; record `bufnr()` as
   `first` (≠ 0).
3. **Hide (persist)**: `toggle_terminal()`; poll `windows().len() == base`. Assert a `term://` row
   with `bufnr() == first` STILL present (window closed, buffer/job persist).
4. **Reopen (reuse)**: `toggle_terminal()`; poll `windows().len() == base + 1`. Assert the `term://`
   row's `bufnr()` is still `first` (reused, not recreated).
5. **Recreate-if-gone**: send `<C-\><C-n>` (two `FfiKeyEvent`s: `Char('\\')`+ctrl, `Char('n')`+ctrl)
   to leave terminal-insert, then `send_ex_command(&handle, ":bwipeout!")` to wipe the focused
   terminal buffer; poll until no `term://` row remains. Then `toggle_terminal()`; poll until a
   `term://` row reappears; assert its `bufnr() != first` (fresh tagged terminal).
6. `handle.shutdown(); drop(tmp);`.
Reuse `FLUSH_TIMEOUT`/`POLL_INTERVAL`; copy `char_key`/`enter_key`/`send_ex_command` + a
`ctrl_char_key(c)` helper from `commands.rs`. Add a Bazel `rust_test(name = "terminal", …)` target to
`crates/wolf-ffi/BUILD.bazel` copied verbatim from the `file_actions` target (lines 356–377), with
`srcs = ["tests/common/mod.rs", "tests/terminal.rs"]` and `crate_root = "tests/terminal.rs"`.

### 8.4 Swift tests (OD6)
Locate the `CommandKind` decode/contract test in `crates/wolf-macos/Tests/` and add `.terminal == 18`
/ `init?(ffi: 18)` (mirror how `newTab` was added). If none exists, note it and rely on the Rust
contract guard + compile-time exhaustive match.

## 9. Docs to update

- `crates/wolf-core/src/README.md` — `command.rs` (`Terminal`), `input.rs` (`ToggleTerminal`).
- `crates/wolf-ffi/src/README.md` — `command_kind::TERMINAL`, `FfiCommand`,
  `WolfCoreHandle::toggle_terminal`.
- `crates/wolf-ffi/tests/README.md` — new `terminal` test.
- `crates/wolf-macos/Sources/README.md` — `CommandSeam` case, `CommandDispatcher` arm,
  `WolfWindowController.toggleTerminal`, `AppMenu` View item.
- `runtime/nvim/README.md` — `wolf_toggle_terminal()`.
- `architecture.md` — link to `command-lifecycle.md`; optionally note the Terminal drawer.
- New `command-lifecycle.md`.
- Root `README.md` — optionally add `command-lifecycle.md` to the docs list.
- `prettier --write` every edited md.

## 10. Implementation order (avoid compile breakage)

1. `command.rs` — variant + spec + tests (self-contained).
2. `input.rs` — `ToggleTerminal` + `apply` arm + helper.
3. `api.rs` — `WolfCore::toggle_terminal` wrapper.
4. `init.lua` — `wolf_toggle_terminal()`.
5. `wolf-ffi/src/lib.rs` — `command_kind::TERMINAL`, `from_command` arm, bridge `fn toggle_terminal`,
   impl method.
6. Regenerate bindings; commit `generated/`.
7. Swift: `CommandSeam` → `WolfWindowController` → `CommandDispatcher` → `AppMenu`.
8. Tests: `command.rs` tests, extend `commands.rs`, add `terminal.rs` + BUILD target, Swift test.
9. READMEs + `command-lifecycle.md` + `architecture.md` link; `prettier --write`.

## 11. Dependencies / Lint

No new crates/Bazel deps (`tempfile` dev-dep already used). Follow the existing named-constant
pattern in `command.rs` tests (no new `#[allow]`); extend the `commands.rs` contract-guard array to
reference `CMD_KIND_TERMINAL`; the bridge module already has `#[allow(clippy::all, missing_docs,
unused)]` but hand-written additions must carry doc comments to match siblings.

Architect agent id: a376256a1ef114d7c
