# Plan: File tree sidebar (wolf-core current-file + wolf-gnome tree UI) — STAGED

## Status: COMPLETE ✅ (file tree sidebar S1-S3 shipped)

### Final state
- S1 core current-file tracking (Snapshot.current_file, canonicalized; init.lua autocmd
  BufEnter/BufWinEnter/BufFilePost + initial emit, set up over RPC after attach; redraw routes
  wolf_current_file, no seq bump, wake after unlock). Harness improved to seq-based FlushWaiter.
- S2 pure tree helpers (tree.rs: children_of, ancestors_to_reveal, node_rel_path, rel_path_for).
- S3 GNOME sidebar (tree_view.rs: gtk::Paned left; TreeListModel+ListView+TreeExpander via
  BoxedAnyObject; single-click open + grab_focus; top-level expanded; current-file highlight +
  reveal=expand ancestors+select+scroll_to (gtk v4_12); populated by same FileSearch as picker;
  on_render_wake for current_file; close drops FileSearch). All 7 spec bullets met.
- Tests: wolf-core 44 (incl current_file.rs 4 real-nvim: Open + :e keystrokes + unnamed=None +
  switch); wolf-gnome lib 30 (tree + rel_path_for pure) + 4 cage smokes (3 existing under Paned +
  tree population). clippy clean both crates; denylist CLEAN (only PathBuf added to core; v4_12 is
  wolf-gnome-only). Reviewer APPROVED (no Must-fix). Docs (core+gnome+runtime READMEs, tests
  READMEs, architecture.md) updated.
- New files: tests/current_file.rs, src/tree.rs, src/tree_view.rs.
- v1 limitation (documented): a current_file change rebuilds the tree, which may collapse manual
  nested expansion. Future: incremental/version-gated rebuild + file-watching for live updates.

### scroll_to + autoselect landed: gtk v4_12 feature; reveal now scroll_to(pos) after select;
set_autoselect(false); load_from_data->load_from_string (deprec fix). All 4 cage smokes✓, wolf-core
44✓, wolf-gnome tests✓, clippy clean, denylist CLEAN (v4_12 is wolf-gnome-only).

## (history) FEATURE REVIEWED ✅ (all 7 bullets met) — scroll_to fix, then technical-writer

### Reviewer verdict: APPROVED, no Must-fix; all 7 spec bullets satisfied; concurrency/borrow/
canonical-path invariants hold. ORCHESTRATOR DECISIONS:
- D-SCROLL (coder, Should-fix accepted): enable the gtk `v4_12` feature and add
  `list_view.scroll_to(pos, gtk::ListScrollFlags::NONE, None)` after selection in reveal() so a
  deeply-nested current file is actually scrolled into view (completes "reveal/show"). System GTK is
  4.22 so v4_12 is safe. Re-store the list_view in TreeInner (needed for scroll_to). Cargo.toml:
  `gtk = { version="0.11", package="gtk4", features=["v4_12"] }`.
- D-AUTOSEL (coder, nit accepted): `selection.set_autoselect(false)` so no stray row-0 highlight
  before a reveal.
- CSS-provider-stacking nit: ACCEPT as-is (harmless single-window).
- Docs deferred to technical-writer (NEXT): gnome src/README (tree.rs/tree_view.rs, Paned, window
  1200, consts, highlight/reveal), core src/README (current_file + wolf_current_file event + setup_
  current_file_tracking), runtime/nvim/README (current-file tracking fn), gnome tests/README (tree
  smoke), core tests/README:35 wait_for_flush(&core,..) signature, architecture.md (current-file in
  contract + tree sidebar in wolf-gnome).

## (history) S1✅ S2✅ S3 IMPLEMENTED ✅ — full-feature reviewer, then docs

### S3 RESULT
tree.rs +rel_path_for (pure). tree_view.rs (NEW): TreeHandle/build/widget/on_render_wake/close;
BoxedAnyObject(TreeNode) model; TreeListModel(autoexpand=false)+create_func(children_of); 
SignalListItemFactory(TreeExpander+Label, CURRENT_FILE_CSS highlight); SingleSelection; ListView
single-click-activate; long-lived FileSearch + drop-safe wake + spawn_future_local drain (Weak);
refresh (rebuild root store from children_of); on_render_wake (reveal+highlight only when
current_file changed); reveal (rel_path_for + expand ancestors top-down + select); activate
(File=>Open+grab_focus, Dir=>toggle); close drops FileSearch. app.rs: gtk::Paned(start=tree.widget,
end=overlay, position 250); WINDOW_WIDTH 960->1200; render drain also calls tree.on_render_wake;
wire_close tree.close before shutdown. lib.rs pub mod tree_view. Tests: lib 30✓ (incl rel_path_for
4), gdk_key/paint_golden/roundtrip✓, ALL 4 CAGE SMOKES✓ (3 existing under new layout + tree pop),
wolf-core 44✓, clippy clean, denylist CLEAN.

### S3 KNOWN GAP (assess in review): reveal does NOT scroll_to (needs gtk `v4_12` feature; coder
avoided dep change). System GTK is 4.22 so v4_12 is safe — likely worth enabling to complete
"reveal/show the file" (expand+select works; off-screen rows just don't auto-scroll). Reviewer opine.

## (history) S1✅ S2✅ — STAGE S3 (gnome tree sidebar UI), test-writer next

### S2 RESULT ✅
tree.rs (pure): TreeNode{Dir|File}, children_of (dir=""=root; immediate children; dirs deduped;
dirs-first then files, case-insensitive name, rel_path tiebreak; File abs_path=entry.path),
ancestors_to_reveal (cumulative ancestor dirs minus last), node_rel_path. lib.rs pub mod tree.
Tests: tree 9✓ (26 lib total), wolf-core 44✓, clippy clean, denylist CLEAN. No deviations.

### S3 ADD: extract one more pure seam `rel_path_for(entries: &[FileEntry], abs: &Path) -> Option<String>`
(match entry.path == abs -> entry.display.clone(); None if absent) in tree.rs — the current_file ->
tree-display mapping used by reveal; gives default-gate coverage. Unit-test it.

## (history) S1 COMPLETE ✅ — STAGE S2 (pure tree helpers), test-writer next

### S1 RESULT ✅
Snapshot.current_file (canonicalized) tracked via init.lua autocmd (BufEnter/BufWinEnter/BufFilePost
+ initial emit) set up over RPC after attach (nvim_get_api_info chan -> exec_lua, chan as vararg);
redraw.rs routes wolf_current_file -> apply_current_file (write_async, NO seq bump, wake after
unlock). Harness improved: FlushWaiter now seq-based (wait_for_flush waits for seq to advance;
wait_for_initial_flush waits seq>=1 for attach) — robust to out-of-band wakes. Tests: wolf-core 44✓
(current_file 4 real-nvim incl :e keystrokes + unnamed=None; all prior green), clippy clean, gnome
builds, denylist CLEAN, stable x2. Doc carryover: tests/README.md:35 wait_for_flush signature now
takes &core (technical-writer).

## (history) STAGE S1 — seq removed (correct), harness needs startup-flush fix, then re-verify

### D-SEQ applied (apply_current_file no longer bumps seq; wake still fires — correct). This
unmasked a latent race in the seq-based waiter: the ATTACH flush (seq 0->1) lands during
WolfCore::start (block_on ui_attach), so the FIRST wait_for_flush baselines at 1 and waits forever
for >1. (Old seq-bump masked it via the initial current_file emit.) Post-input waits are FINE
(input pump's 8ms coalesce delay => their flush is future when baseline captured). Only the
startup/attach wait is broken.

### ORCHESTRATOR DECISION (D-HARNESS2, test-writer): the STARTUP wait must mean "attach flush has
landed" = `read_snapshot().seq >= 1`, not "advance from current". In crates/wolf-core/tests/common/
mod.rs add `wait_for_initial_flush(&self, core, timeout)` that polls until `core.read_snapshot().seq
>= 1` (return immediately if already), draining wakes, within FLUSH_TIMEOUT. Use it for the ATTACH
wait in start_core_sized (~line 234) and the FIRST wait in start_core_with_temp_md_sized (~217).
Leave the post-Open wait (~223) and all test-level post-input waits as the advance-based
wait_for_flush(&core,timeout) (they work — pump delay). Do NOT reintroduce the seq bump. current_
file.rs untouched (polls). After fix, coder re-runs full wolf-core suite.

## (history) STAGE S1 — impl done but wake/flush conflict; fixing harness + seq, then re-verify

### S1 coder result: render.rs current_file field, redraw.rs CURRENT_FILE_EVENT routing +
apply_current_file (canonicalize, write_async, wake-after-unlock), nvim.rs setup_current_file_
tracking (nvim_get_api_info chan -> nvim_exec_lua passing chan as vararg, NOT interpolated),
api.rs calls it after ui_attach, init.lua _G.wolf_setup_current_file_tracking. current_file.rs 4✓,
clippy clean, gnome builds, denylist CLEAN. BUT firing the wake on wolf_current_file breaks 10
OLD tests (input_keys cursor_moves, redraw_grid x7, save_open open_existing_markdown) because the
shared FlushWaiter treats each wake as exactly one redraw flush -> extra current_file wake desyncs.

### ORCHESTRATOR DECISION (correctness-first; keep the wake):
- KEEP firing (self.wake)() on the wolf_current_file path — the front-end (S3 tree) relies on it
  as a "snapshot changed, re-read" signal. Do NOT drop it.
- D-SEQ (coder): apply_current_file must NOT bump `seq`. `seq` is the REDRAW FRAME counter (bumped
  on flush only); a current_file change is not a frame. Revert the seq bump.
- D-HARNESS (test-writer): make the redraw-pipeline test waiting robust to out-of-band (non-flush)
  wakes by keying on a REAL flush = `read_snapshot().seq` ADVANCING, not raw wake count. Fix the
  shared `wake_channel`/`FlushWaiter` in crates/wolf-core/tests/common/mod.rs so wait_for_flush
  blocks until seq increases (draining/ignoring current_file wakes), within FLUSH_TIMEOUT. This must
  fix all 10 tests via the helper WITHOUT weakening what they verify (they still wait for the real
  redraw to land). current_file.rs stays as-is (it polls current_file directly). The waiter needs a
  seq source (SharedSnapshot/owned read guard via the core); choose the cleanest mechanism within
  common/mod.rs — minimal call-site churn preferred, but adjusting call sites in the affected core
  test files is acceptable (they're test files). Assume current_file does NOT bump seq (D-SEQ).

## (history) STAGE S1 (core current-file) — test-writer next

## Orchestration: implement S1->S2->S3, each green (build+clippy+tests+denylist via nix develop)
before next; ONE reviewer pass after S1 (core) optional/light, full reviewer after S3; technical-
writer for all docs at END (architect already specced per-stage READMEs — coder/tech-writer do them).

---

# ARCHITECT PLAN (condensed) — STAGED

## Cross-cutting decisions
- A. CURRENT-FILE: hybrid — autocmd LOGIC in runtime/nvim/init.lua as `_G.wolf_setup_current_file_
  tracking(chan)` (augroup WolfCurrentFile; events BufEnter/BufWinEnter/BufFilePost; callback
  rpcnotify(chan,"wolf_current_file",expand('%:p')); + ONE initial emit), INVOKED over RPC after
  ui_attach with core's channel id from nvim_get_api_info -> nvim_exec_lua. Snapshot gains
  `current_file: Option<PathBuf>` (Default None; no ctor change). RedrawHandler::handle_notify ->
  match "redraw" | "wolf_current_file"(=CURRENT_FILE_EVENT const) | _ ; apply_current_file: parse
  arg string, empty=>None else canonicalize (fallback raw), write_async set current_file + bump seq,
  drop lock, THEN wake (preserve lock-never-across-await + wake-after-unlock). Unnamed buffer => None.
- B. TREE HELPERS: PURE seam in crates/wolf-gnome/src/tree.rs (NOT core — it's a GTK-shaped
  presentation projection; core stays FFI-flat + denylist trivially clean). Types/fns:
  `enum TreeNode { Dir{name,rel_path} | File{name,rel_path,abs_path:PathBuf} }`;
  `children_of(entries:&[FileEntry], dir:&str)->Vec<TreeNode>` (dir=""=root; split display on '/';
  immediate children only; dirs deduped; only dirs containing files appear; sort DIRS-FIRST then
  files, case-insensitive name, tie-break rel_path; O(n)/call recompute — fine for lazy expansion);
  `ancestors_to_reveal(rel_path:&str)->Vec<String>` (root-down ancestor dirs of file's parent);
  `node_rel_path(&TreeNode)->&str`. #[must_use], in-module table tests.
- C. GTK4 WIDGET: wrap TreeNode in glib::BoxedAnyObject (NO custom GObject subclass). Root
  gio::ListStore = children_of(entries,""); gtk::TreeListModel(passthrough=false, autoexpand=false,
  create_func: Dir=>Some(ListStore of children_of(entries,dir)), File=>None); SingleSelection;
  SignalListItemFactory (setup: TreeExpander+Label; bind: set_list_row, label=name, apply current-
  file CSS highlight); ListView set_single_click_activate(true). Top-level expanded "by default" =
  root store shows cwd's immediate children at depth 0 (autoexpand=false, no manual root expand).
  Single-click File -> send_input(Open{abs}) + area.grab_focus(); Dir -> toggle expand. Highlight:
  CSS class "wolf-current-file" iff node.abs_path==snapshot.current_file. REVEAL: match current_file
  to entry.display; ancestors_to_reveal; expand ancestor TreeListRows top-down (re-scan model each
  step), select file row + scroll_to. Streaming race: pending_reveal RefCell retried each FileSearch
  refresh + at complete.
- D. WAKES: tree population = long-lived FileSearch started at build, OWN picker-style wake
  (async-channel bounded(1) + spawn_future_local drain -> refresh; Weak to avoid cycle). current_file
  rides EXISTING render wake: in app.rs render drain loop, after area.queue_draw() call
  tree.on_render_wake() (read snapshot.current_file; if changed -> reveal+highlight). WolfCore stays
  Rc<RefCell<Option>> on main thread; tree owns its FileSearch, dropped on close/shutdown.
- E. LAYOUT: gtk::Paned(Horizontal): start=ScrolledWindow(tree ListView), end=existing overlay
  (picker still overlays editor). content holds the Paned. RESIZE already correct (connect_resize is
  on `area`/DrawingArea -> measures editor pane). Click returns focus via area.grab_focus().
  wire_close: tree.close() before core.shutdown() (picker ordering).

## S1 — core current-file tracking
MODIFY: render.rs (+use PathBuf; Snapshot.current_file: Option<PathBuf>, doc). redraw.rs (const
CURRENT_FILE_EVENT="wolf_current_file"; handle_notify match; async apply_current_file: parse
args.first() string, empty=>None else canonicalize fallback-raw, write_async set + bump seq, drop,
wake after). nvim.rs: NvimProcess::setup_current_file_tracking(&self)->Result<(),NvimError> (nvim_
get_api_info -> chan id -> nvim_exec_lua("wolf_setup_current_file_tracking(chan)",[chan]); errors->
Rpc). api.rs: after ui_attach in start block_on, call proc.setup_current_file_tracking().await?.
init.lua: add _G.wolf_setup_current_file_tracking(chan). READMEs core src + runtime/nvim.
TEST (real nvim, tests/current_file.rs): open via Open -> current_file==Some(canonicalize(temp));
open via `:e <path><CR>` keystrokes -> canonical match; unnamed/no open -> None (initial emit);
open A then B -> follows to B. Canonicalize both sides (symlinked /tmp). DONE=GREEN core tests +
clippy + denylist.

## S2 — pure tree helpers (tree.rs)
CREATE crates/wolf-gnome/src/tree.rs (TreeNode, children_of, ancestors_to_reveal, node_rel_path,
#[must_use], #[cfg(test)] table tests). MODIFY lib.rs pub mod tree; README. TEST (pure table):
children_of("") top-level dirs-first+ci-order; children_of("a")/("a/b") immediate only; only dirs-
with-files appear; deep file surfaces ancestor dirs; dedup; files-vs-dirs order; ci name sort +
rel_path tiebreak; ancestors_to_reveal nested/root/single; '/' handling; empty entries => empty;
node_rel_path both variants. DONE=GREEN gnome lib tests + clippy.

## S3 — gnome tree sidebar UI
CREATE tree_view.rs (GTK glue): TreeHandle{Rc<TreeInner>}; build(paned, area, core)->TreeHandle;
on_render_wake() (read current_file; reveal+highlight if changed); close() (drop FileSearch).
TreeInner: entries RefCell<Vec<FileEntry>> (Rc-shared w/ create_func), search RefCell<Option<
FileSearch>>, root_store, tree_model, selection, list_view, core, area, wake, pending_reveal,
last_current. refresh() (read FileSearch -> copy entries -> rebuild root_store via children_of("")
-> retry pending_reveal). MODIFY app.rs build_ui: build tree; gtk::Paned(start=ScrolledWindow(tree),
end=overlay); content holds Paned; render drain loop also calls tree.on_render_wake() after
queue_draw; wire_close tree.close() before shutdown; keep area.grab_focus(). lib.rs pub mod
tree_view. READMEs + architecture.md. TEST: pure already in S2; glue needs display. Optional cage
smoke (serialized CAGE_LOCK, OCR-robust, LOOSE): file visible in sidebar; AVOID pixel-accurate row
clicks (flaky) — open via picker/:e then OCR assert filename + heading. Rely mainly on S2 unit +
existing picker smoke. DONE=GREEN gnome tests + clippy + app builds.

## Deps/lint/risks
NO new crates (all gtk4 0.11/glib/async-channel present). Denylist clean (only PathBuf added to core).
Lint: named consts (CURRENT_FILE_EVENT, CSS class const); #[must_use] pure fns; try_from for
casts (chan id, u32<->usize) or #[allow(reason=)]; preserve funcorder + lock-never-across-await;
Weak in closures (no cycles). Risks: TreeListModel/BoxedAnyObject glue density (isolate in
tree_view.rs); reveal-before-stream (pending_reveal retry); canonical path match (canonicalize both);
autocmd channel timing (after ui_attach + initial emit); focus (grab_focus after open); Paned+Overlay
+resize (resize on area stays correct); rebuild churn may reset manual expansion mid-stream (v1
documented limitation, optional version-gated rebuild); keep core GUI-free.

## Impl order: S1 -> S2 -> S3, each green via nix develop --command first.
LOAD-BEARING: in redraw.rs wake MUST fire AFTER the write_async guard drops (both redraw + the new
wolf_current_file path), per the render.rs locking invariant.

---

## Problem statement
Add a file tree sidebar on the LEFT of the wolf-gnome window (editor on the right).
Large, STAGED. Each stage green before the next.

### FULL SPEC (faithful)
- File tree on the LEFT; editor on the right.
- SINGLE click on a file opens it for editing.
- Top-level directory expanded by default; all other dirs start collapsed.
- The file CURRENTLY being edited is highlighted in the tree.
- Navigating via the FILE PICKER reveals the new current file in the tree (expand ancestors).
- Navigating via NEOVIM (e.g. :e) ALSO reveals the new current file (expand ancestors).
- Populating the tree uses the SAME mechanism as the file picker (wolf-core FileSearch /
  `ignore` traversal). Tree = hierarchical projection of that flat file list (dirs derived
  from file paths; only dirs containing listed files appear; .git/.wolf/.gitignored excluded
  consistently with the picker).

### KEY DESIGN POINTS (architect decides + justifies)
1. CURRENT-FILE TRACKING (crux; only way to satisfy reveal-after-:e): wolf-core must know the
   absolute path of the active buffer and expose it via snapshot+wake (e.g. Snapshot.current_file:
   Option<PathBuf>, updated + wake on buffer change). Recommended: BufEnter/BufWinEnter autocmd
   rpcnotifying `expand('%:p')`; set up in runtime/nvim/init.lua OR via RPC after attach (get core
   channel from nvim_get_api_info; nvim_create_autocmd/exec_lua; Handler routes a new
   "wolf_current_file" notification beside "redraw"). Canonicalize to match FileEntry paths.
2. TREE MODEL from the flat list: PURE unit-testable helpers — children-of-dir (subdirs+files for
   a given dir from the flat `display` list, for GTK's lazy TreeListModel create-func) + ancestors-
   to-reveal(path). Decide location (wolf-core app logic vs wolf-gnome pure seam like geometry/
   cursor/rank) + justify.
3. GNOME UI: GTK4 gtk::TreeListModel + gtk::ListView + gtk::TreeExpander (NOT deprecated TreeView).
   Layout gtk::Paned (tree left, existing Overlay/DrawingArea right; preserve picker overlay +
   cursor + keys). Populate from a long-lived FileSearch started at startup (reuse start_file_search,
   ignore mru_rank). Single-click -> send_input(Open{path}). Top-level expanded by default. Highlight
   + REVEAL current file (expand ancestors + scroll/select) on current_file change (render/current-
   file wake); population on FileSearch wake (picker pattern). Handle streaming + current_file
   arriving before/after tree population.
4. Coexistence: tree + picker (Ctrl+Shift+P) + block cursor + EventControllerKey->nvim + resize +
   shutdown. Tree click must NOT permanently steal editor focus (return focus to DrawingArea after
   open). Editor keys still go to nvim.

### SUGGESTED STAGING (architect may refine)
- S1 (core): current-file tracking (autocmd/rpcnotify -> Snapshot.current_file + wake). Real-nvim
  tests: open via Open AND via simulated :e keystrokes both update current_file (canonicalized).
- S2 (pure): tree hierarchy helpers (children-of-dir; ancestors-to-reveal). Unit-tested.
- S3 (gnome): sidebar UI (Paned + TreeListModel/ListView/TreeExpander); populate via FileSearch;
  single-click open; default-expand top level; highlight + reveal current file (after picker AND
  after :e). Unit-test pure logic; extend opt-in cage smoke where reasonable (serialized via
  CAGE_LOCK, OCR-robust).

### Constraints
- Build gate: ALL builds/tests via `nix develop --command` (direnv stale after flake edits).
- wolf-core GUI denylist MUST stay clean. Prefer real-nvim/real-fs tests (no mocks). Every src
  dir needs a README. Docs are source of truth.

### Relevant existing surface (reuse)
- wolf-core: Snapshot + SharedSnapshot + WakeFn (wake-on-flush); RedrawHandler::handle_notify
  (currently routes only "redraw"); start_file_search(&self, WakeFn)->FileSearch (MRU seed +
  streamed `ignore` traversal); FileEntry{path abs, display rel, mru_rank}; InputEvent::Open{path}
  -> nvim :e + MRU record (api.rs input pump); nvim.rs spawn/attach; runtime/nvim/init.lua in-repo.
- wolf-gnome: app.rs (Overlay wrapping DrawingArea; async-channel + glib::spawn_future_local wake
  marshalling; header-bar; EventControllerKey -> gdk_key/send_input; wire_close shutdown); picker.rs
  (start_file_search + streaming drain + rank_matches + pure seams pattern); pure seams geometry/
  cursor/rank + their in-module tests; cage smoke harness (CAGE_LOCK serialized, tesseract --psm 11).

## Progress log
- (orchestrator) Plan reset for file-tree (staged). File picker COMPLETE. Next: architect.
