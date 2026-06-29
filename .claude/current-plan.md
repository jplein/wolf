# Plan: Split panes / multiple files (multigrid + nvim->shell intents + native panes) — STAGED

## Status: COMPLETE ✅ (split panes / multigrid shipped)

### Final state
- S1 multigrid core: Snapshot.grids/windows/Cursor.grid/active_win; ext_multigrid attach; per-grid
  events + resize; active_grid() compat shim; Grid::resize content-preserving (bug fix).
- S2 intent channel: init.lua wolf_setup_layout overrides <C-w>s/v/c/q/hjkl+arrows + cnoreabbrev ->
  wolf_layout/wolf_active_win rpcnotify; LayoutIntent + drain_intents + focus_win; shell-owns-layout.
- S3 native panes: pure layout.rs PaneTree (split/remove/pane_rects/neighbor); pane_view.rs nested
  gtk::Paned of per-pane DrawingAreas bound to win+grid; cmdline strip (grid 1 last row) restores
  :/search visibility; drain intents -> split/focus/close; per-pane + divider resize -> resize_grid;
  click/<C-w>hjkl focus; open-into-active-pane; coexists with tree sidebar + picker.
- All 5 v1 ops work: h/v split, close, focus-move, drag-resize. Comment cleanups done.
- Tests: wolf-core all binaries✓ (redraw_multigrid 6, layout_intents 8, from_payload pure 10, +
  migrated); wolf-gnome lib (incl 13 pure layout)✓ + integration✓ + ALL 8 CAGE SMOKES✓; clippy clean
  both crates; denylist CLEAN. Reviewed (S1 + full) APPROVED. Docs (architecture.md, core+gnome+
  runtime READMEs, tests READMEs, in-code comments) updated; known v1 limitations documented.
- New files: core src/layout.rs, tests/{redraw_multigrid,layout_intents}.rs; gnome src/{layout,pane_view}.rs.
- KNOWN v1 LIMITATIONS (documented): windows from non-overridden paths (:help/quickfix/preview/
  :diffsplit) get a grid but no pane; cmdline strip shows only grid 1's last row (multi-row prompts
  truncated until ext_messages); neighbor uses idealized 0.5 ratios not dragged positions; floats ignored.
- Working branch: feat/split-panes (all accumulated work uncommitted).

### Reviewer verdict: APPROVED, no Must-fix. All locked decisions + 5 ops verified; Grid::resize fix
protocol-correct; cmdline strip restores typed-command visibility; pane<->win<->grid lockstep +
pending-grid resolution + DrawingArea reuse + borrow/concurrency safety all hold.
### DECISIONS on reviewer findings:
- SHOULD-FIX (windows from NON-overridden paths — :help/quickfix/preview/:diffsplit get a grid+WinInfo
  but no pane -> invisible + possible stale focus): DOCUMENT as a known v1 limitation (consistent with
  the user's intent-driven design; out of scope for "split panes for files"). Future: create a pane
  for any unbound window grid (append-split of active). NOT fixing now.
- NITS (document as limitations, no code change): close-detect by grid-vanished (vs win) — negligible
  grid-reuse race; neighbor uses idealized 0.5 ratios not dragged positions; cmdline strip shows only
  grid 1's LAST row (multi-row prompts truncated until ext_messages, deferred per arch decision 7);
  transient pending-pane may briefly paint grid1 via active_grid fallback (resolves next wake);
  CMDLINE_GRID_ID dups core OUTER_GRID_ID.

### technical-writer doc list (final step):
architecture.md (render-state now multigrid; intents channel realized: LayoutIntent/drain_intents +
wolf_layout/wolf_active_win; ext_multigrid; native panes + cmdline strip; floats/ext_cmdline deferred).
core src/README (render multigrid model+accessors+active_win NOW populated [fix stale "always None"],
redraw multigrid+intent events, nvim setup_layout+resize_grid, layout.rs LayoutIntent). core
render.rs:441 + README:68 stale "always None in S1" -> fix. redraw.rs:217 "in S1" label. gnome
src/README + lib.rs:15-21 crate doc (DrawingArea/cmdline "deferred" -> pane_view + cmdline strip +
layout.rs). runtime/nvim/README (wolf_setup_layout overrides + wolf_layout/wolf_active_win). both
tests/README (redraw_multigrid, layout_intents, layout pure tests, 8 cage smokes; fix stale CAGE_LOCK
"two tests"->8). DOCUMENT the known v1 limitations above (in gnome README / architecture deferred).

## (history) ALL STAGES GREEN ✅ (8/8 cage smokes) — full-feature reviewer, then technical-writer

### Final test state: wolf-core all binaries✓ (incl redraw_multigrid 6, layout_intents 8, from_payload
pure, Grid::resize fix), wolf-gnome lib 43✓ (13 layout) + integration✓ + ALL 8 CAGE SMOKES✓ (window/
picker/picker-nav/tree/vsplit/hsplit/focus-move/close-pane). clippy clean both crates, denylist CLEAN,
no stray .wolf.

## (history) S3 + extra coverage; REAL bug found+fixed; 1 test-OCR fix left, then full review

### New coverage caught a REAL bug (why it was worth adding):
- BUG: Grid::resize cleared ALL content (vec![Cell::default()]) — violates ui-linegrid (grid_resize
  is content-preserving; only grid_clear clears; nvim doesn't re-send unchanged rows after resize).
  => panes went BLANK after any split/close-driven nvim_ui_try_resize_grid. vsplit smoke masked it via
  later repaints. FIX (coder, render.rs Grid::resize): copy overlapping min(old,new) region; only new
  cells blank. Existing resize tests assert dims only — still pass. ACCEPTED (protocol-correct).
- from_payload pure tests: 10✓. horizontal_split + close_pane cage smokes: PASS after the fix.
- 7/8 cage smokes pass. wolf-core all✓, wolf-gnome lib 43✓, clippy clean, denylist CLEAN.

### ORCHESTRATOR DECISION (test-OCR fix): focus_move_returns_to_first_pane_under_cage fails ONLY
because the block cursor parks on the marker's last char (ZZLEF[T] -> OCR 'zzlef}'); APP IS CORRECT
(screenshot: left pane refocused w/ marker, both files, split intact — the two split-survival
asserts pass). FIX (test-writer): in FOCUS_SCENARIO, after `i$FOCUS_MARKER<Esc>`, move the cursor off
the marker row (o + Escape, mirroring the working SCENARIO) and keep asserting `zzleft` (now
unobscured) — OR assert the cursor-free prefix `zzlef`. Prefer the move-cursor-off approach + full
`zzleft`. Keep CAGE_LOCK + the two heading asserts. Then coder re-runs to confirm 8/8.

## (history) S1✅ S2✅ S3 IMPLEMENTED ✅ — add e2e smokes for focus/close/hsplit, then review

### S3 RESULT ✅
layout.rs (pure PaneTree: new/split/remove/leaves/pane_rects/neighbor + PaneLayout mirror; 13 tests).
pane_view.rs (NEW glue): PaneHandle/build/widget/cmdline_widget/on_render_wake; PaneInner{tree,
panes:HashMap<PaneId,{win,grid:Cell<Option>,area}>,container[pane_area|cmdline strip],metrics,core,
active_pane}; per-pane paint of bound grid + cursor only in cursor.grid's pane; ★ CMDLINE STRIP =
1-row DrawingArea painting grid 1's last row (panes never paint grid 1) -> :/search/messages visible
again; on_render_wake: ensure first pane, drain_intents (Split->tree.split+bind new pane; FocusDir->
neighbor->focus_win), detect closes (bound pane's grid vanished->tree.remove, never empty), resolve
pending grids via grid_for_win, active pane from active_win (fallback cursor.grid) + focus-ring CSS;
rebuild_pane_area reuses DrawingAreas by PaneId; per-pane connect_resize->resize_grid + resize_global
->resize(grid1); GestureClick->focus_win. app.rs: pane view as overlay main child (tree sidebar left,
picker overlay over panes); wake drain calls panes.on_render_wake()+tree.on_render_wake(); cmdline
strip is picker/tree focus-return target. lib.rs pub mod layout/pane_view.
Tests: wolf-gnome lib 43✓ (incl 13 layout), integration✓, ALL 5 CAGE SMOKES✓ (incl new
vertical_split_shows_two_panes — two files side by side OCR'd at 28pt/1200px, no tweak), clippy clean,
wolf-core 10 binaries✓, denylist CLEAN.

### ORCHESTRATOR DECISION — add coverage for the user-facing ops only covered at pure/intent level:
(test-writer) 3 new opt-in cage smokes (focus-move <C-w>v then <C-w>h returns to left pane; close-
pane <C-w>c surviving pane reclaims full width; horizontal split <C-w>s stacks two files top/bottom)
+ a PURE core unit test for LayoutIntent::from_payload (malformed action/orientation/missing fields
-> None; valid -> Some) in core layout.rs in-module (closes the untested tolerance path in the default
gate). Keep CAGE_LOCK serialization; loose OCR; reuse harness.

## (history) S1✅ S2✅ — STAGE S3 (native GTK panes + cmdline strip), test-writer next

### S2 RESULT ✅
layout.rs (LayoutIntent{Split{orientation,from,win}|FocusDir{dir,from}}, SplitOrientation,
FocusDir, from_payload tolerant). redraw.rs: IntentQueue=Arc<Mutex<VecDeque>>, RedrawHandler::new(
snapshot,intents,wake); routes wolf_active_win (->active_win, no seq bump, wake) + wolf_layout
(parse->push queue, wake). api.rs: drain_intents()->Vec, focus_win(win) via pump; start() calls
setup_layout after current_file. input.rs FocusWin{win}->nvim_set_current_win (plain i64 works).
nvim.rs setup_layout. init.lua wolf_setup_layout: do_split(real split + rpcnotify), do_close(guarded
nvim_win_close, last-window no-op), do_focus(PURE rpcnotify, no nvim focus change); <C-w> chords
s/v/c/q/hjkl+arrows + cnoreabbrev split/sp/vsplit/vs/close/new/vnew; WinEnter->wolf_active_win.
Findings: wolf_layout handles arrive as plain Value::Integer; set_current_win accepts plain i64.
Tests: layout_intents 8/8✓ (incl pure-focus <C-w>l unchanged + cabbrev :vsplit fired reliably),
all wolf-core✓, wolf-gnome unchanged✓, clippy clean, denylist CLEAN. Deviation accepted: focus via
input pump (ordering) not a standalone set_current_win method.
- Coder test recs (DEFER to S3/final): more FocusDir variants (<C-w>h/j/k + arrows + <C-w><C-s/v>);
  pure from_payload malformed-payload unit test; drain_intents-empty-after-drain. Fold cheap pure
  ones into S3 if convenient.

## (history) S1 COMPLETE ✅ (reviewed) — STAGE S2 (intents + init.lua overrides), test-writer next

### S1 reviewer verdict: APPROVED as sound foundation. Model/redraw/ext_handle/active_grid shim/
lock-wake invariants correct; migrated tests preserve semantics. Nits accepted (win_pos overwrites
entry.win - fine; win_viewport may transiently create a windows entry for a grid not yet in grids -
benign). S2 must empirically confirm set_current_win/win_close accept plain i64 (focus_win test
catches it).

### ★ NEW S3 REQUIREMENT (reviewer point 6 — real regression from enabling multigrid):
The cmdline / `/`-search / messages now live on GRID 1, which the front-end does NOT paint, so typed
`:`/`/` input is INVISIBLE (and active_grid_id() returning 1 during cmdline mode would blank-swap the
editor). FIX IN S3 (front-end): (a) render grid 1 as a bottom CMDLINE/MESSAGE STRIP across the editor
area; (b) ensure editor PANES always show their window grid (>=2), never grid 1 — i.e. the per-pane
paint uses the pane's bound grid, and grid 1 is painted only in the strip. This makes multigrid
usable for typed commands. (Native ext_cmdline/ext_messages remains a later option per architecture
decision 7.) Tracked as a required S3 task, not optional.

## (history) S1 IMPLEMENTED ✅ — focused review of multigrid core, then S2

### S1 RESULT ✅
ext_handle finding: win arrives as Value::Ext(1, msgpack-int) (NOT plain Integer); decoded to i64
via rmpv::decode::read_value; stored as WinInfo.win:i64 (same form for set_current_win/win_close in
S2). render.rs: grids:HashMap + windows:HashMap<grid,WinInfo> + Cursor.grid + active_win:Option(None
in S1) + accessors (active_grid_id/active_grid[OnceLock empty fallback]/grid/grid_mut[get-or-create]/
active_viewport/grid_for_win) + OUTER_GRID_ID const. redraw.rs: route by params[0] grid id;
grid_destroy/win_pos/win_hide/win_close/per-win win_viewport; floats+msg_set_pos ignored; unknown
grid dropped. nvim.rs ext_multigrid attach + try_resize_grid; input.rs ResizeGrid; api.rs resize_grid.
gnome render.rs paint(cr,&Grid,&HighlightTable,&CellMetrics); app.rs/cursor.rs use active_grid().
Tests: wolf-core 10 binaries✓ (redraw_multigrid 6, redraw_grid 8 migrated, resize honored exact
50x15 no relax needed), wolf-gnome 30 unit + paint_golden/roundtrip/gdk_key✓, clippy clean, denylist
CLEAN. App unchanged (single active grid painted).
- NOTE: coder mechanically migrated 4 MORE test files the test-writer missed (spawn.rs, save_open.rs,
  paint_golden.rs, roundtrip_nvim.rs) — access-path only (snap.grid -> active_grid()/grid(1)/
  grid_mut(1) + paint() call sig), NO assertion/semantic changes. Reviewer: confirm no weakening.
- Coder test recs (DEFER to S2/S3, noted): per-window viewport test (split+scroll one pane, assert
  active_viewport reflects focused win only); win_hide/hidden-flag coverage.

## (history) STAGE S1 (multigrid core) — test-writer next

## Orchestration: S1(a-d) -> reviewer(core multigrid, foundational/risky) -> S2 -> S3 -> full
reviewer -> technical-writer. Each stage green (build+clippy+tests+denylist via nix develop) first.
VERIFY-FIRST (gates S1b): how nvim-rs delivers the `win` arg in win_pos/win_viewport (Value::Ext
{wintype, encoded int} vs plain Integer) — the ext_handle helper + set_current_win/win_close must
use the SAME id form. Coder confirms empirically early.

---

# ARCHITECT PLAN (condensed) — STAGED

## Cross-cutting decisions
- D1 MULTIGRID MODEL (render.rs): Snapshot { grids: HashMap<u64,Grid>, windows: HashMap<u64 grid,
  WinInfo>, highlights, cursor: Cursor{grid,row,col,mode_idx}, mode_infos, current_file,
  active_win: Option<i64>, seq }. WinInfo{grid,win:i64,start_row,start_col,width,height,viewport,
  hidden}. COMPAT accessors (keep S1 front-end single-grid + tests working): active_grid_id() ->
  cursor.grid if present else highest non-1 else 1; active_grid()->&Grid (empty static fallback);
  grid(id), grid_mut(id) (get-or-create), active_viewport(), grid_for_win(win). Snapshot::new makes
  grid 1 + cursor.grid=1. GRID-1 ROLE: keep in grids (nvim resizes it; cmdline/msgs live there) but
  NEVER paint it; window content on grids >=2; active_grid() returns the cursor's window grid.
  Migrate EVERY single-grid assumption (render.rs grid/Cursor/Viewport; redraw.rs route by params[0]
  grid id + cursor.grid + per-win viewport; gnome render.rs paint(cr,&Grid,&HighlightTable,&Cell
  Metrics); app.rs/cursor.rs use active_grid(); cursor.rs tests add grid:; core tests use
  active_grid()/active_viewport(); tree_view/picker unchanged - read only current_file). Preserve
  lock/wake invariants; flush bumps seq; out-of-band (current_file/active_win/intents) wake w/o seq.
- D2 ATTACH: ui_attach opts {ext_linegrid,ext_multigrid,rgb}. Handle grid_resize/line/scroll/clear/
  cursor_goto (carry grid id), grid_destroy, win_pos, win_viewport(per grid), win_hide, win_close.
  DEFER (ignore, documented): win_float_pos, win_external_pos, msg_set_pos. Grid 1 still gets initial
  cols/rows.
- D3 PER-GRID RESIZE: nvim.rs try_resize_grid(grid,cols,rows)->nvim_ui_try_resize_grid; InputEvent::
  ResizeGrid{grid,cols,rows} (through ordered pump, no race); WolfCore::resize_grid. Keep WolfCore::
  resize -> nvim_ui_try_resize for grid 1.
- D4 INTENT CHANNEL (RECOMMENDED HYBRID): SPLITS -> nvim PERFORMS (Lua runs real :split/:vsplit so
  nvim makes window+grid+emits win_pos) THEN rpcnotify wolf_layout{action:split,orientation,from,
  win}; shell uses intent for orientation+which-pane, resolves new grid via win_pos (retry on wake).
  CLOSE -> nvim PERFORMS guarded (refuse last window) nvim_win_close; shell collapses pane from the
  win_close redraw event (NO intent variant). FOCUS moves -> PURE intent wolf_layout{action:focus_
  dir,dir,from}; shell computes neighbor in ITS layout + nvim_set_current_win. Click focus -> shell
  FocusWin{win}->set_current_win. Rust: LayoutIntent{ Split{orientation:SplitOrientation(Horizontal=
  stacked|Vertical=side),from:i64,win:i64}, FocusDir{dir:FocusDir(L/R/U/D),from:i64} }. STORAGE: a
  separate Mutex<VecDeque<LayoutIntent>> in core (NOT snapshot); RedrawHandler parses wolf_layout ->
  push -> wake (no seq bump); WolfCore::drain_intents()->Vec<LayoutIntent>. InputEvent::FocusWin{win}
  -> nvim_set_current_win; nvim.rs set_current_win.
- D5 init.lua _G.wolf_setup_layout(chan) (called from nvim.rs after attach, chan as Lua vararg).
  PRIMARY robust path: vim.keymap.set('n',...) for <C-w>s/<C-w><C-s>->split h, <C-w>v/<C-w><C-v>->
  split v, <C-w>c/<C-w>q->close, <C-w>h/j/k/l + <C-w><arrows>->focus(dir). Ex-commands via
  cnoreabbrev guarded by getcmdtype()==':'&&getcmdline()==word for split/sp/vsplit/vs/close/new/
  vnew (document cabbrev whole-word limitation; chords are the robust path). WinEnter autocmd ->
  rpcnotify wolf_active_win(nvim_get_current_win()) + emit once. BufEnter already follows focus so
  current_file tracks active pane (no change). Helpers split/focus/close per D4. Hermetic, in-repo.
- D6 FOCUS/ROUTING: nvim_input global (nvim routes to current win). Pane focus=nvim_set_current_win.
  Active pane detect via BOTH cursor.grid (render) + wolf_active_win (focus ring/pane lookup).
  <C-w>hjkl -> FocusDir intent -> shell neighbor -> FocusWin.
- D7 PER-PANE RENDER (gnome S3): NEW pure layout.rs (GTK-free, unit-tested): PaneId, Orientation,
  LayoutNode, PaneTree{split,remove/collapse,leaves,pane_rects(bounds),neighbor(rects,from,dir)}.
  NEW pane_view.rs (glue): build/rebuild nested gtk::Paned from tree; {PaneId->(win,grid:Option,
  DrawingArea)}; per-pane paint(cr,&grid,&highlights,&metrics) + active pane draws cursor; each
  DrawingArea connect_resize->grid_size_for->resize_grid(grid); divider drag same path; active focus
  ring CSS; REUSE DrawingAreas by PaneId on rebuild (don't recreate grids). Outer [tree sidebar |
  pane-tree]; picker Overlay wraps pane-tree, opens into active pane.
- D8 current_file/picker/tree per active pane: current_file already follows via BufEnter on WinEnter;
  tree highlight/reveal + picker/tree Open(:e to current win) UNCHANGED beyond active-win tracking.

## S1 (sub-staged) — core multigrid foundation; front-end shows ACTIVE grid only (app unchanged)
- S1a render.rs: grids/windows/WinInfo, Cursor.grid, per-grid viewport, accessors, Snapshot::new
  grid1+cursor.grid=1; lib.rs re-export WinInfo. Build green.
- S1b redraw.rs: route params[0] grid id; grid_resize->grid_mut(id).resize; clear/line/scroll/
  cursor_goto on that grid (cursor_goto sets cursor.grid); add grid_destroy/win_pos/win_hide/
  win_close; win_viewport->windows[grid].viewport; ext_handle(&Value)->Option<i64> (Value::Ext OR
  Integer — VERIFY against live nvim-rs). Ignore floats/msg_set_pos. Unknown grid -> drop quietly.
- S1c nvim.rs attach +ext_multigrid; try_resize_grid; input.rs ResizeGrid; api.rs resize_grid.
- S1d migrate consumers: gnome render.rs paint(cr,&Grid,&HighlightTable,&CellMetrics); app.rs +
  cursor.rs use active_grid(); cursor.rs test literals (Cursor{grid:1}, grid_mut(1)); core tests
  redraw_grid.rs + current_file.rs -> active_grid()/active_viewport().
- S1 TEST (real nvim, tests/redraw_multigrid.rs): attach-multigrid (grids has 1 + a window grid;
  active_grid_id()!=1; cursor.grid set; windows has active win); raw `:split<CR>` keystrokes -> grids
  /windows +1; per-grid grid_line lands in active grid only; `:close<CR>` -> grids/windows -1 (gone
  grid absent); `:split`+`<C-w>w` -> cursor.grid changes, active_grid follows; resize_grid changes
  that grid only (grid 1 unchanged). No HashMap order assumptions. DONE=GREEN wolf-core + wolf-gnome
  (cursor unit) + clippy + denylist.

## S2 — intent channel + init.lua overrides + active-win
init.lua wolf_setup_layout (D5). nvim.rs setup_layout + set_current_win; api.rs call after attach;
intent queue + drain_intents + focus_win. redraw.rs route wolf_active_win (set active_win, wake) +
wolf_layout (parse map -> LayoutIntent -> push queue, wake). input.rs/layout.rs LayoutIntent/Split
Orientation/FocusDir + InputEvent::FocusWin. lib.rs re-exports. Tolerate malformed payloads. Close
last-window guard in Lua. TEST (real nvim, tests/layout_intents.rs): <C-w>s->Split{Horizontal},
<C-w>v->Vertical, :vsplit<CR>->Vertical (cabbrev), <C-w>l->FocusDir{Right} & active_win UNCHANGED
(pure), focus_win(other)->active_win changes + current_file follows, <C-w>c (2 wins)->count drops &
win_close removes grid; 1 win -> no-op, wolf_active_win updates on WinEnter. Front-end still ignores
intents (green). DONE=GREEN.

## S3 — native GTK panes
NEW pure layout.rs (PaneTree split/remove/pane_rects/neighbor) unit-tested. NEW pane_view.rs glue.
app.rs: replace single DrawingArea+Overlay with pane-tree container wrapped in picker Overlay; on
wake drain_intents -> apply (split->mutate tree+bind new pane; focus_dir->neighbor->focus_win);
detect win_close (known pane's grid gone) -> remove pane; active ring from active_win; create first
pane when first window grid appears. lib.rs pub mod layout/pane_view. Edge: split-before-grid-known
(grid=None, resolve via grid_for_win on wake); close collapses parent; never 0 panes; active pane =
win==active_win (fallback cursor.grid); divider/resize->grid_size_for->resize_grid; picker/tree open
active pane; don't shrink editor below OCR-legible width. TEST: pure layout.rs table tests (split/
remove/pane_rects nested+ratios/neighbor each dir + edges); opt-in cage smoke vertical_split_shows_
two_panes (open A, vsplit + :e B, OCR both headings); keep 3 existing smokes green. DONE=GREEN.

## Risks: win-handle ext decoding (isolate ext_handle, real-nvim test); multigrid order/grid1 (retry
-on-wake binding); intent hybrid asymmetry (documented); active-grid vs active-win 1-frame lag ok;
rebuild Paned reusing DrawingAreas by PaneId (don't recreate grids); per-grid resize via pump;
big migration contained in S1a-d w/ active_grid() shim; floats/cmdline deferred (grid1 unpainted -
verify cmdline still usable, follow-up to paint grid1 bottom rows if needed). NO new deps; denylist
unchanged (layout/pane_view in gnome; core gains only HashMap/VecDeque+rmpv). Docs: architecture.md
(multigrid + intents realized), core src/README, gnome src/README, runtime/nvim/README, both
tests/README.

## Impl order: S1a->S1b->S1c->S1d (green) -> S2 (green) -> S3 (pure layout.rs first, then pane_view/
app wiring, then cage smoke). Each compiles+passes under nix develop before next.

---

## Problem statement
Add split panes / multiple files open at once. LARGEST feature: migrate wolf-core from a
SINGLE grid to MULTIGRID and add a Neovim->shell intent channel. Heavily STAGED (3+ stages,
sub-stage as needed); each green before the next.

### LOCKED USER DECISIONS
1. LAYOUT OWNER = THE SHELL (native panes). GTK Paned (draggable) own geometry. init.lua
   OVERRIDES split/window cmds+shortcuts (:split/:sp, :vsplit/:vs, <C-w>s, <C-w>v, <C-w>c/
   :close, <C-w>h/j/k/l + arrows, maybe <C-w>q/:q-as-pane, :new/:vnew) and REPLACES them with
   rpcnotify messages to the UI shell. Shell creates/destroys/focuses native panes, binds each
   pane to a Neovim window + its grid (ext_multigrid), sizes each grid to its pane.
2. v1 OPS = ALL: horizontal split, vertical split, close pane, move focus between panes, AND
   drag-to-resize dividers (divider drag -> nvim_ui_try_resize_grid for the affected grid(s)).
3. OPEN TARGET = ACTIVE PANE: tree/picker open -> :e in the focused pane (replace its buffer).
   Splitting is a separate explicit gesture.

### KEY DESIGN POINTS (architect decides + justifies)
A. MULTIGRID (core): attach ext_multigrid (+ext_linegrid +rgb). Render model holds MULTIPLE
   grids keyed by grid id. Multigrid events: grid_resize/grid_line/grid_scroll/grid_clear/
   grid_cursor_goto carry a grid id; + win_pos, win_float_pos (floats: decide ignore/defer),
   win_hide, win_close, grid_destroy, win_viewport (per grid), msg_set_pos (defer?). Global
   cursor belongs to a specific (active) grid. Decide grid-1 (outer) role. Per-grid resize:
   nvim_ui_try_resize_grid(grid,cols,rows). STAGE IDEA: land multigrid in core FIRST while the
   front-end still shows only the active grid (app unchanged) — migrate+test the model first.
B. NVIM->SHELL INTENT CHANNEL: reuse the wolf_current_file rpcnotify pattern (init.lua
   rpcnotify(chan,"wolf_<event>",payload); RedrawHandler routes). Realize architecture.md's
   core->front-end "intents". Decide a drainable INTENT QUEUE (in snapshot or sibling) the
   front-end consumes on wake: {split h/v (which win), close (win), focus (dir|win)}. Decide
   EXACTLY what init.lua does per command: perform the real nvim window op + report ids, OR pure
   intent + shell drives nvim_open_win/win_close/set_current_win. Recommend least-race lockstep.
   Expose pane<->window<->grid mapping (win_pos carries (grid,win) -> learn grid<->win).
C. PANE<->WIN<->GRID + FOCUS: each pane maps to a nvim win + grid. Split creates win+grid+pane;
   close removes; focus = nvim_set_current_win (keys go to nvim globally, it routes to current
   win). Keep in sync as nvim/user actions change things.
D. PER-PANE RENDER+RESIZE (gnome): replace the single DrawingArea with nested gtk::Paned, one
   DrawingArea per pane, each painted from its grid (reuse paint()). Cursor + current-file
   highlight scope to the ACTIVE pane. Pane/divider resize -> grid_size_for -> try_resize_grid.
E. COEXISTENCE: file-tree sidebar (left) + picker (Ctrl+Shift+P -> active pane) + per-pane
   cursor + EventControllerKey->nvim + current-file (per active pane) + clean shutdown. Outer
   layout: [tree sidebar | (Paned tree of editor panes)] with the picker overlay over editor area.

### SUGGESTED STAGING (architect refines/sub-stages)
- S1 (core): multigrid foundation — attach ext_multigrid; multi-grid render model + new events;
  per-grid resize; front-end still shows only the active grid (app unchanged). Real-nvim tests.
- S2 (core): intent channel + init.lua command overrides -> intents; expose intents + mapping.
- S3 (gnome): native panes UI — nested Paned + per-pane DrawingArea; consume intents; map
  pane<->win<->grid; per-pane paint + active-pane cursor; per-pane/divider resize; open-into-
  active-pane; coexist with tree+picker. Pure layout logic unit-tested; opt-in cage smoke.

### Constraints
- Build gate nix develop. wolf-core GUI denylist MUST stay clean. Prefer real-nvim/fs tests.
  Every src dir README. Docs are source of truth. Reuse wake/snapshot + rpcnotify-intent +
  FileSearch + picker/tree infra. init.lua in-repo. CALL OUT every single-grid assumption to
  migrate (render.rs/redraw.rs/app.rs/tree_view.rs/picker.rs).

### Hardest unknowns to flag/resolve
multigrid grid-1 semantics; intent-vs-direct nvim ops in init.lua; focus routing; per-pane
picker/cursor/current-file; keeping pane/win/grid in lockstep across user+nvim actions;
drag-resize -> per-grid resize; floats (defer?).

### Relevant existing surface (reuse)
- core: attach ext_linegrid+rgb single grid (nvim.rs); Snapshot{grid:Grid, cursor, ...,
  current_file} + SharedSnapshot + WakeFn (wake-on-flush + on current_file); RedrawHandler::
  handle_notify routes "redraw" + "wolf_current_file" -> apply_*; seq bumped on flush only;
  FlushWaiter seq-based; nvim.rs setup_current_file_tracking (chan via get_api_info + exec_lua);
  start_file_search/FileSearch; InputEvent::Open -> :e + MRU; runtime/nvim/init.lua (autocmd +
  wolf_setup_current_file_tracking).
- gnome: app.rs (single DrawingArea in Overlay, in gtk::Paned w/ tree sidebar; render wake drain
  -> queue_draw + tree.on_render_wake; EventControllerKey -> gdk_key/send_input; wire_close);
  paint()/CellMetrics; cursor.rs; picker.rs; tree_view.rs; geometry.rs.

## Progress log
- (orchestrator) Plan reset for split-panes (staged). File tree COMPLETE. Decisions locked
  (shell-owns-layout, all ops, open-active-pane). Next: architect for staged plan.
