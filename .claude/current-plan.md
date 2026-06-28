# Plan: File picker (wolf-core logic + wolf-gnome UI) — STAGED

## Status: COMPLETE ✅ (file picker S1-S4 shipped)

### Final state
- S1 MRU (sqlite in .wolf/, sync read, record-on-open, prune-missing), S2 async streaming
  traversal (ignore crate: gitignore+ancestors, exclude .git/.wolf, untracked, symlink-to-file,
  dotfiles; non-repo FS walk), S3 pure ranking (MRU-first, ci subsequence, filename strict-prefix),
  S4 GNOME picker (Ctrl+Shift+P + header-bar button, Overlay+SearchEntry+ListBox, streaming via
  wake, keyboard nav, Enter=>Open via nvim :e + MRU record, Esc close).
- Deps: rusqlite(bundled) + ignore in wolf-core (no flake change, denylist CLEAN). 
- Tests: wolf-core 40 (mru 6, mru_open_records 1, file_search 8 real fs/git/nvim, rank 10 pure,
  + existing 15); wolf-gnome lib 17 (is_picker_trigger/next_selection/restored_index pure) + 3
  cage smokes (window/picker/picker-nav, CAGE_LOCK serialized, --ignored). clippy clean both crates.
- Reviewed (core S1-S3 + S4) APPROVED. Docs (core+gnome READMEs, tests READMEs, architecture.md)
  updated; module dep order corrected. .gitignore has .wolf/.
- New files: src/{mru,files,rank}.rs, tests/{mru,mru_open_records,file_search}.rs, gnome src/picker.rs.

### Nav seams extracted + unit-tested: next_selection + restored_index (pure, in picker.rs),
move_selection/refresh refactored to use them (behavior unchanged). wolf-gnome lib 17✓ (incl
next_selection_table, restored_index_table, is_picker_trigger_table), 3 cage smokes✓, wolf-core
40✓, clippy clean, denylist CLEAN.

## (history) S4 + keyboard nav DONE ✅ — extract pure nav seams + unit-test, then technical-writer

### Keyboard nav landed: Up/Down move ListBox selection (Stop, focus stays in entry, clamp no-wrap);
selection PRESERVED across refresh unless query changed (last_query: RefCell<Option<String>>);
Enter on empty = no-op. All 3 cage smokes pass twice (window/picker/picker-arrow-nav), non-ignored
wolf-gnome 15 lib + tests✓, wolf-core 40✓, clippy clean, denylist CLEAN, .wolf/ gitignored.

### ORCHESTRATOR DECISION: extract the two PURE nav helpers + unit-test them (the clamp/preserve
logic currently has coverage ONLY in the opt-in cage smoke, never in default `cargo test`):
- `fn next_selection(current: i32, delta: i32, n_rows: i32) -> Option<usize>` (clamp [0,n-1], no-wrap,
  None when n_rows==0). move_selection uses it.
- `fn restored_index(reset_to_top: bool, prev: Option<usize>, new_len: usize) -> Option<usize>`
  (reset=>Some(0) if len>0 else None; else clamp prev to [0,len-1]; None when len==0). refresh uses it.
Keep them pure (no GTK), in picker.rs above the test module; add table-driven #[cfg(test)] tests
(clamp at top/bottom, empty list, preserve vs reset, prev beyond new_len). Coder refactors to call
them (behavior unchanged; cage smokes must still pass).

## (history) S4 REVIEWED ✅ (feature sound) — adding keyboard nav, then technical-writer (docs)

### S4 reviewer verdict: APPROVED; feature S1-S4 complete & sound. No Must-fix.
### ORCHESTRATOR DECISION — implement the Should-fix (keyboard list navigation):
A file picker whose list can't be navigated by keyboard is half-built. ADD to picker.rs:
- Up/Down arrow keys (handled in the picker's key controller / on the SearchEntry) move the
  ListBox SELECTION (not the text cursor); wrap or clamp at ends; Enter opens the SELECTED row.
- refresh() must PRESERVE the current selection index across list rebuilds (clamp to new len)
  so streaming traversal wakes don't yank selection back to row 0 while the user is navigating.
  (Only reset to row 0 when the QUERY changes / on open.)
- Enter with ZERO rows = no-op (do NOT close the picker). (nit fix)
- Leave the harmless redundant open()->set_text("")->refresh double-call as-is (nit, skip).
Testing: extend the opt-in picker cage smoke to exercise arrow-nav (two matching files w/
distinct OCR-friendly headings; type a shared query, Down to the 2nd, Enter, assert the 2nd
file's heading). Keep pure where possible; nav is GTK so the cage smoke is the e2e gate.
### Then: technical-writer for ALL deferred docs (S1-S4): core src/README files.rs/rank.rs/mru.rs +
dep order; core README rusqlite+ignore deps + denylist note + CoreConfig.cwd + start_file_search/
mru_recent API; gnome src/README + README picker.rs + is_picker_trigger + trigger(Ctrl+Shift+P+
button) + deferred-list update; gnome tests/README (file_search/mru/rank? those are core; picker
trigger test + picker cage smoke); architecture.md recent-files/traversal + picker/trigger note.

## (history) ALL STAGES IMPLEMENTED ✅ — S4 reviewer, then technical-writer (docs)

### S4 fully green: window smoke (cursor moved off row 0, asserts "hello") + picker smoke
(asserts "pickertarget") BOTH pass in one --ignored invocation (CAGE_LOCK serialized). Non-ignored
wolf-gnome 20✓, wolf-core 40✓, clippy clean, denylist CLEAN, .wolf/ gitignored.

## (history) S4 — picker✅ + D-A/D-C done; D-B needs v2 (move cursor off text), then review

### Progress: D-A (CAGE_LOCK serialize) WORKS — both cage tests run in one --ignored invocation,
no socket contention. D-C done (.gitignore .wolf/, stray dir removed, confirmed ignored). Picker
cage smoke PASSES. wolf-core 40✓, wolf-gnome lib 15✓, clippy clean, denylist CLEAN.

### D-B v2 DECISION: asserting "hell" was insufficient — OCR garbles "hello"->"hbehe" (block cursor
+ mediocre OCR distorts interior letters, not just trailing char). Real fix: MOVE THE CURSOR OFF
ROW 0 before screenshot so the asserted text is fully unobscured. In window_renders smoke: after
`i`+hello+Esc, send `o` (normal-mode open-line-below -> insert on a NEW row 1) then Esc; cursor now
on row 1, "hello" on row 0 cursor-free. Restore assertion to full `hello`. (Pre-cursor-feature this
test passed only because the cursor was INVISIBLE; now it renders correctly, so move it aside.)
Keep CAGE_LOCK + picker test unchanged.

## (history) S4 IMPL DONE (picker works under cage 4/4) — fixing test harness, then review

### S4 RESULT
picker.rs: is_picker_trigger (pure, Ctrl+Shift+P ci) + PickerHandle/build (Overlay child Box{
SearchEntry, ScrolledWindow(ListBox)}; start_file_search+drop-safe picker_wake -> async_channel
-> spawn_future_local drain -> refresh: rank_matches top 200; Enter/row-activated -> Open + close;
Esc close; Weak to avoid cycle). app.rs: Overlay wraps DrawingArea; header-bar Open button;
wire_keys: picker open => Proceed no-forward; else is_picker_trigger => open Stop; else gdk_key.
wire_close closes picker. lib.rs pub mod picker. Tests: wolf-gnome lib 15✓ (incl is_picker_trigger),
gdk_key/paint_golden/roundtrip ✓, clippy clean, builds; wolf-core 40✓; denylist CLEAN.
PICKER cage smoke PASSES 4/4 (real e2e: Ctrl+Shift+P -> type -> Enter -> nvim opens file -> OCR).

### ORCHESTRATOR DECISIONS (test-harness fixes; picker code is correct)
- D-A: the two #[ignore] cage tests CANNOT run in parallel (Wayland socket contention -> both
  fail). FIX: serialize via a shared `static CAGE_LOCK: std::sync::Mutex<()>` in smoke_headless.rs;
  each cage test locks it at start (recover poison via into_inner). Test-writer.
- D-B: window_renders_typed_text_under_cage OCR is flaky/pre-existing (block cursor sits on a
  letter, distorts OCR "hello"->"hbehe"; text renders fine). FIX: make it robust — assert on a
  substring NOT under the cursor (e.g. cursor lands on one char; assert OCR contains the first
  4 letters "hell" or use a fuzzy/contains-most check). Test-writer; keep it loose.
- D-C: gitignore `crates/wolf-gnome/.wolf/` (and any `.wolf/`) — the MRU cache created when run
  from the crate dir must not be committed. Coder adds to .gitignore + removes the stray dir.

## (history) CORE COMPLETE ✅ (37 tests) — STAGE S4 (GNOME picker UI)

### Core final: 37 wolf-core tests pass (rank 10 incl. match_start/span tiebreaks, file_search 8,
mru 6, mru_open_records 1, redraw 8, save_open 3, spawn 1), clippy clean, denylist CLEAN.

### S4 USER DECISION (trigger): "Different shortcut + button" -> LOCKED to **Ctrl+Shift+P**
(non-conflicting; all nvim keys incl <C-p> preserved) PLUS a header-bar button. Picker open =>
all keys go to the picker widget (suppress forwarding to nvim/core); Esc closes; Enter/row-
activate => send_input(Open{path}) (MRU auto-recorded by core pump) + close.
- Trigger predicate must be a PURE fn: `is_picker_trigger(keyval: gdk::Key, state:
  gdk::ModifierType) -> bool` (true iff CONTROL+SHIFT and key == p/P), testable headlessly like gdk_key.
- picker_wake try_send MUST be drop-safe (ignore errors on closed channel) — late cancel-path wake.

## (history) CORE (S1-S3) REVIEWED ✅ — add 1 tiebreak test, then S4

### Core reviewer verdict: APPROVED as sound foundation for S4. No Must-fix.
- SHOULD-FIX: add a decisive unit test for match_start(key5) & span(key6) tiebreaks in rank.rs
  (two entries tied on keys 1-4, separated only by match_start; another pair only by span).
  Impl already has the logic (rank.rs:148-149) so it should pass; coder verifies.
- NITS ACCEPTED AS-IS: is_listable_file double-stat (perf only); core has no total entry cap
  (top-N deferred to S4 display — memory grows until complete/cancel, conscious); seen is
  seed-only after clone; rank_empty fills unused key fields (harmless); to_string_lossy on
  non-UTF8 paths (edge). 
- S4 CARRYOVER: picker_wake try_send must be drop-safe (ignore errors on closed channel) since
  a late cancel-path wake() can fire after FileSearch dropped.
- Docs deferred to technical-writer at END (after S4): core src/README files.rs/rank.rs/mru.rs;
  README rusqlite+ignore deps + denylist note + CoreConfig.cwd; architecture.md recent-files/traversal.

## (history) S1✅ S2✅ S3✅ — CORE REVIEW (S1-S3) before S4 UI

### S3 RESULT ✅
rank.rs: Match{index,key} + MatchKey (8 priority fields, derived Ord) + rank_matches (empty=>
all MRU-first; non-empty=>ci subsequence over full display, drop non-matches, sort by key).
Words from FILENAME only, split on / \ _ - . space. lib.rs re-exports rank_matches/Match/MatchKey.
Tests: rank 8✓ (exact orders), S1+S2+existing all✓, clippy clean (incl file_search.rs:124 fix),
denylist CLEAN. No deps, no flake change, no deviations.
- Coder flagged OPTIONAL gap: no direct test for match_start(key5)/span(key6) tiebreaks
  (keys 1-4,7,8 covered). Reviewer to opine; add if warranted.

## (history) S1✅ S2✅ — STAGE S3 (pure rank), test-writer next

### S2 RESULT ✅
ignore 0.4.26 (pure Rust, no flake change, denylist CLEAN). files.rs: FileEntry moved here
(re-exported via mru + lib); FileSearchState{entries,complete,version,seen}; FileSearch
(read()->owned guard deref to state, cancel(), Drop=cancel); spawn_traversal (spawn_blocking
ignore walk, D3 config, prune .git/.wolf, classify plain/symlink-to-file, dedup, batch 256 +
version bump + wake after unlock, complete on finish). api.rs start_file_search (canonical root,
sync MRU seed, spawn on runtime handle). Tests: file_search.rs 8✓ (real fs/git/nvim), S1 7✓,
existing 15✓, clippy src clean, gnome builds, denylist CLEAN. Ran twice, no flakiness.
- Coder note (accepted): traversal uses entry.path() NOT fs::canonicalize (preserves symlink
  display + dedup vs MRU canonical paths for regular files).
- CARRYOVER (fold into S3 test-writer): clippy warning tests/file_search.rs:124
  iter_cloned_collect -> use `view.entries.to_vec()`.

## (history) S1 COMPLETE ✅ — STAGE S2 tests written, coder next

### S2 test-writer output: tests/file_search.rs (real fs+git+nvim, 8 tests, local poll-until-
complete helper). No common/mod.rs change. Tests: non-repo plain+dotfiles+no-dirs; symlinks
(file in, dir/broken out, #[cfg(unix)]); symlink loop no-hang; git gitignore + .git excluded +
untracked included; nested/ancestor gitignore; .wolf excluded; MRU sync seed + dedup keeps rank;
cancel stops streaming. Asserts on SET of display strings.

### S2 ORCHESTRATOR DECISIONS / coder guidance
- GUARD SHAPE: FileSearch::read() returns an OWNED guard that Derefs to a state struct OWNING
  `entries: Vec<FileEntry>, complete: bool, version: u64` (mirror SnapshotReadGuard -> owned
  Snapshot; use parking_lot read_arc). Field names entries/complete/version are LOAD-BEARING.
- CANONICAL PATHS: start_file_search should canonicalize the traversal root, and MRU record()
  already canonicalizes — both must yield canonical absolute paths so the dedup HashSet<PathBuf>
  matches (tests pass already-canonical cwd). FileEntry.path absolute, display rel to root.
- Git tests only `git init` (+ maybe add), NO commit (no user.name/email needed); ignore crate
  reads .gitignore directly. cancel test is tolerant (version stable, not mid-flight).

### Orchestration note: implement core stages S1✓→S2→S3 (green gate each), then ONE reviewer
pass over core picker logic, then S4 UI (ask user the Ctrl+P trigger question first), then
reviewer on S4 + technical-writer for ALL docs at the end.

### S1 RESULT ✅
rusqlite 0.31 (bundled, no flake change). mru.rs (MruStore + FileEntry + MruError + MRU_LIMIT,
WAL+busy_timeout, opened_at millis, ORDER BY opened_at DESC,rowid DESC, prune-missing inline).
api.rs: CoreConfig.cwd, CoreError::Mru, WolfCore.mru + mru_recent(), pump records Open. lib.rs
re-exports. grid_dump.rs + app.rs cwd added. Tests: mru.rs 6✓, mru_open_records.rs 1✓ (real
nvim), existing 15✓, clippy clean, gnome builds, denylist CLEAN. No deviations.

## (history) STAGE S1 — tests written, coder next

### S1 test-writer output
- tests/mru.rs (real sqlite+fs): open creates .wolf/mru.sqlite + empty recent; 3-record order
  0/1/2; re-record moves front no dup; delete -> omitted + row pruned; MRU_LIMIT cap 50; display
  relative + path absolute.
- tests/mru_open_records.rs (real-nvim): start_core_with_temp_md -> mru_recent() contains opened path.
- Modified BOTH tests/common/mod.rs (core + gnome): added fresh_cwd() (persistent tempdir, NOT a
  dropped guard) + config_in(cwd,cols,rows); config() unchanged signature, delegates w/ fresh cwd.

### S1 ORCHESTRATOR DECISIONS / coder guidance (from test-writer notes)
- DET-ORDER: opened_at must be millisecond-or-finer; recent() ORDER BY opened_at DESC, rowid DESC
  so same-ms inserts still order by recency (tests insert ~5ms apart + rely on re-record ordering).
- MRU_LIMIT: re-export the impl const (single source of truth) so tests can import it if desired.
- display: relative to cwd using platform separator; record() of a file OUTSIDE cwd needs a sane
  display fallback (e2e doesn't assert display). canonicalize cwd so record() canonical path lands under it.
- MUST also add `cwd` to the NON-test CoreConfig constructors or it won't compile:
  examples/grid_dump.rs:~56 and gnome src/app.rs:~126 (cwd = current_dir()).

## ORCHESTRATOR DECISIONS on architect's flags
- MRU-FIRST (S3): LOCKED to literal spec — MRU is the PRIMARY sort key (recent file
  outranks a better non-MRU prefix match). Spec: "files in the MRU cache appear first."
- TRIGGER (S4): DEFER to a user question when we reach S4 (Ctrl+P shadows nvim's <C-p>).
  Lean: intercept Ctrl+P before nvim + header-bar button. Will confirm before S4 UI work.

---

# ARCHITECT PLAN (condensed) — STAGED

## Cross-cutting decisions
- D1: picker logic in wolf-core MODULES (mru.rs/files.rs/rank.rs), not a new crate (one
  FFI-shaped surface; sqlite/git deps don't trip the GUI denylist).
- D2: SQLite = `rusqlite` with `bundled` feature (vendors sqlite via cc; NO flake change).
- D3: file listing = the `ignore` crate (ripgrep walker; pure Rust; handles BOTH repo &
  non-repo; NO flake change). Config WalkBuilder at root=cwd: parents(true)+git_ignore(true)
  +git_global(true)+git_exclude(true) (= git ls-files --others --exclude-standard set);
  hidden(false) (show dotfiles); ignore(false)+require_git(false) (don't apply .ignore;
  walk non-repo); follow_links(false) (no symlink loops); defensively skip any `.git` and
  `.wolf` component. Classify each entry: symlink_metadata; if symlink, fs::metadata
  (follow) keep only if target is_file; keep regular files; drop dirs/broken/dir-symlinks.
  FALLBACK (documented, not impl): `git ls-files --cached --others --exclude-standard -z`.
  Rejected git2 (heavy vendored libgit2 build, repo-only) & gix (low-level, repo-only).
- D4: front-end API = snapshot+wake "FileSearch" session (mirrors SharedSnapshot/WakeFn).
  `WolfCore::start_file_search(&self, wake: WakeFn) -> FileSearch`: SYNCHRONOUSLY seeds MRU
  into the shared buffer BEFORE returning, then spawns traversal on the existing tokio
  runtime (runtime.handle().spawn_blocking — ignore walk is blocking IO). `FileSearch::read()
  -> owned guard Deref to FileSearchView { entries: &[FileEntry], complete: bool, version:u64 }`.
  `FileSearch::cancel()` sets an AtomicBool; `Drop` also cancels (close picker / new search).
  FileEntry { path: PathBuf (abs, for Open), display: String (rel to root, what UI shows +
  ranks), mru_rank: Option<u32> }. Dedup: seed MRU first into a HashSet<PathBuf>; traversal
  skips already-seen paths (MRU file discovered again listed once, keeps mru_rank).
- D5: selection => `core.send_input(InputEvent::Open{path})` (no separate select call). MRU
  recorded in the api.rs INPUT PUMP: when a drained batch contains Open{path}, call
  mru.record(path) (synchronous, never across .await). input.rs stays storage-free.
- D6: CoreConfig gains `cwd: PathBuf` (explicit+testable; gnome defaults to current_dir()).
  Cache at <cwd>/.wolf/mru.sqlite (literal cwd, NO repo-root discovery for cache). Traversal
  EXCLUDES .git/ AND .wolf/ (documented exception to "dotfiles shown" — .wolf is our cache).

## S1 — MRU cache (SQLite), record-on-open, prune-missing
CREATE crates/wolf-core/src/mru.rs:
- MruStore { conn: Arc<parking_lot::Mutex<rusqlite::Connection>>, cwd: PathBuf }
- MruError (thiserror): Open/Io/Query.
- open(cwd: &Path) -> Result<MruStore, MruError>: create_dir_all(cwd/.wolf); open
  cwd/.wolf/mru.sqlite; PRAGMA journal_mode=WAL + busy_timeout=MRU_BUSY_TIMEOUT_MS(2000);
  CREATE TABLE IF NOT EXISTS mru(path TEXT PRIMARY KEY, opened_at INTEGER NOT NULL).
- record(&self, path:&Path): canonicalize if exists (else make absolute vs cwd); upsert
  (path, opened_at=now); errors logged via tracing, NOT propagated (fire-and-forget).
- recent(&self) -> Vec<FileEntry>: SELECT path ORDER BY opened_at DESC LIMIT MRU_LIMIT(50);
  for each, if path missing on disk DELETE + skip (prune inline); else FileEntry{path,
  display=rel-to-cwd, mru_rank=Some(i)}.
- consts MRU_LIMIT=50, MRU_BUSY_TIMEOUT_MS=2000. FileEntry defined here in S1 (moves to
  files.rs in S2, re-export from both).
MODIFY: Cargo.toml +rusqlite{bundled}. api.rs: CoreConfig +cwd; WolfCore +mru store +cwd;
start() opens MruStore (new CoreError::Mru variant), clones store into input pump; pump
records each Open in a drained batch. lib.rs: pub mod mru + re-export FileEntry. ADD test
accessor `pub fn mru_recent(&self) -> Vec<FileEntry>` on WolfCore. Update BOTH crates'
tests/common config() to set cwd (default fresh tempdir kept alive); add config_in(cwd,..).
gnome app.rs CoreConfig +cwd=current_dir(). Docs: core src/README mru.rs + dep order,
README rusqlite dep + denylist note + CoreConfig.cwd, architecture.md recent-files realized.
TEST (real sqlite+fs, tests/mru.rs): empty recent + .wolf created; record 3 -> MRU order
0,1,2; re-record older -> moves front; delete file -> omitted AND row gone; e2e via
start_core_with_temp_md -> mru_recent() contains opened path after flush; MRU_LIMIT cap.
DONE=GREEN: nix develop cargo test -p wolf-core (15+new); denylist clean; clippy clean.

## S2 — async streaming traversal + FileSearch (see D3/D4)
CREATE files.rs: move FileEntry here; FileSearchState{entries,seen:HashSet<PathBuf>,
complete,version} behind Arc<RwLock>; cancel:Arc<AtomicBool>; FileSearch{read()->guard,
cancel(), Drop=cancel}. spawn_traversal(root,state,cancel,wake,handle): handle.spawn_blocking
ignore-walk; classify (D3); skip .git/.wolf; dedup vs seen; append in TRAVERSAL_BATCH(256)
chunks, bump version, wake AFTER dropping write lock; check cancel each iter; complete=true +
final wake. Plain write() ok (blocking task), short critical sections, never log in lock.
Extract pure classification helper for unit test. MODIFY Cargo.toml +ignore="0.4"; api.rs
start_file_search (sync MRU seed + spawn); lib.rs pub mod files + re-export FileEntry,
FileSearch. Docs updated. TEST (real fs+real git via shelled git init, tests/file_search.rs,
poll until complete): non-repo all plain+dotfiles, dirs excluded; symlink-to-file in,
symlink-to-dir/broken out, loop no-hang; git repo gitignore excluded (+ancestor), untracked
included, .git/.wolf never listed; MRU sync seed present pre-complete + not duplicated; cancel
stops task. DONE=GREEN core tests + denylist + clippy.

## S3 — pure fuzzy match + ranking (rank.rs)
CREATE rank.rs: #[must_use] pub fn rank_matches(query:&str, entries:&[FileEntry]) ->
Vec<Match{index,key}>. Empty query => ALL entries in MRU-first order (mru group, mru_rank,
stable). Non-empty: case-insensitive SUBSEQUENCE match on display (ascii-lowercased); drop
non-matches. "word" = run in FILENAME (last path component) split on / \ _ - . space.
strict-prefix = some word starts_with(query) ci (full-word equality ranks highest in prefix
group). MatchKey ascending: (1) mru_group 0 if mru_rank.is_some else 1 [MRU FIRST — LOCKED];
(2) mru_rank.unwrap_or(MAX); (3) prefix_group 0 if prefix-of-a-word else 1; (4) word_index
of earliest anchored word; (5) match_start; (6) span=last-first; (7) display.len; (8) display
lexicographic. In-module table tests. MODIFY lib.rs pub mod rank + re-export. Docs. TEST:
empty=>all MRU-first; ci (README~readme); subseq (rdm~readme); non-match excluded; MRU
outranks better non-MRU prefix; strict-prefix beats scattered; word splitting per separator +
full-word top; deterministic tie-breaks. DONE=GREEN core tests + clippy.

## S4 — GNOME picker UI (TRIGGER decision deferred to user)
CREATE picker.rs: gtk::Overlay wrapping DrawingArea; overlay child = Box(SearchEntry +
ScrolledWindow(ListBox)); show top PICKER_MAX_ROWS(200) ranked. build(overlay,area,core) ->
PickerHandle{open(),close()}. open(): start_file_search(picker_wake) store FileSearch, show,
focus entry, initial refresh (MRU immediately). close(): drop FileSearch (cancel), hide,
area.grab_focus(). Streaming: picker_wake -> async_channel::bounded(1) -> spawn_future_local
drain -> refresh(): read query, search.read(), rank_matches, top N, rebuild ListBox. search-
changed -> refresh. Esc=close. Enter/row-activated -> send_input(Open{path}) (MRU auto), close.
MODIFY app.rs: Overlay between content Box and area; picker::build; wire_keys: if picker open
=> Proceed (don't forward to core); detect trigger -> picker.open() Stop. wire_close drops
picker search. lib.rs pub mod picker. Docs. TEST: pure helpers in-module; real-nvim
(roundtrip or picker_open_records_mru.rs): start_file_search seed+stream, send Open{selected},
flush, assert grid shows content + mru_recent lists it; opt-in cage smoke: temp cwd w/ known
.md, Ctrl+P, type, Enter, grim+tesseract assert heading. DONE=GREEN gnome tests + clippy.

## Deps / lint / risks
Deps: wolf-core +rusqlite(bundled)+ignore (both GUI-free; denylist unchanged, doc-note only).
gnome: none. flake: NO change (rusqlite uses cc; ignore pure rust). Lint: named consts
(MRU_LIMIT/MRU_BUSY_TIMEOUT_MS/TRAVERSAL_BATCH/PICKER_MAX_ROWS); concrete return types
(FileSearch/guard/PickerHandle — FFI shape); #[must_use]; let _ for dropped sqlite/try_send;
#[allow] needs reason=; NEVER hold parking_lot lock across .await. Risks: sqlite concurrency
(Mutex+WAL+busy_timeout, short non-await sections); cancel races (atomic+Drop, harmless late
batch); large dirs (stream+batch+cap); symlink loops (follow_links false); CoreConfig.cwd is
a breaking change -> update all constructors in S1 same change; <C-p> shadow (S4 user q).

## Impl order: S1 -> S2 -> S3 -> S4, each green (build+clippy+tests+denylist) before next,
all via nix develop --command.

---

## Problem statement
Add a file picker to Wolf. Large feature, STAGED into multiple
architect→test-writer→coder→reviewer→tech-writer passes; implement stage by stage,
each landing green before the next.

### FULL SPEC (faithful)
CORE — MRU cache:
- In the CWD, store an MRU cache as a SQLite file inside a `.wolf/` directory.
- If the cache exists, reading is FAST — results included SYNCHRONOUSLY (before async
  traversal completes).
- On file open, record it in the MRU. If an MRU entry no longer exists on disk, remove
  it from the list AND prune from the cache.

CORE — file list traversal (ASYNC, streaming):
- Traverse CWD + subdirs; async; files ADDED incrementally as found (streamed to UI).
- Plain files ONLY, not directories. Symlinks included ONLY if they target plain files.
- Show dotfiles — .gitignore is the ONLY filter.
- If in a Git repo: respect .gitignore (cwd or any ancestor up to repo root); exclude
  .git contents; use a Git LIBRARY (fallback: shell `git ls-files` with untracked flags);
  INCLUDE untracked-but-not-ignored files.
- If NOT a Git repo: FS traversal (all plain files incl. dotfiles).

GNOME — UI:
- Textbox + list below. Fuzzy-match typed text; empty query => all files.
- Case-INSENSITIVE. MRU files rank FIRST. If a typed word is a STRICT PREFIX of a word in
  a filename, prefer over non-strict-prefix matches.

### SUGGESTED STAGING (architect may refine)
- S1 (core): SQLite MRU cache — fast sync read, record-on-open, prune-missing. Decide
  rusqlite (bundled vs system sqlite via flake); schema; `.wolf/` in cwd.
- S2 (core): async streaming traversal — git path (git2 vs gix vs `ignore` crate; respect
  .gitignore + exclude .git + include untracked; fallback git ls-files) + non-repo FS path
  (plain files + symlinks-to-plain-files, dotfiles). Integrate MRU (sync-first, prune missing).
- S3 (core/shared, PURE unit-testable): fuzzy match + ranking fn (case-insensitive;
  MRU-first; strict-prefix-of-word preference). Pure free fn like gdk_key/geometry/cursor.
- S4 (gnome): picker UI — textbox + list wired to streaming source + ranking fn; TRIGGER
  (Ctrl+P intercepted before nvim, and/or header-bar button — flag as UX decision);
  selection => open via existing InputEvent::Open (nvim :e) AND record MRU. Verify via cage
  smoke where reasonable; pure logic unit-tested.

### KEY DECISIONS for architect (decide + justify)
- Where picker logic lives: wolf-core modules vs new crate.
- FFI-shaped core API the UI calls: start search session; SYNCHRONOUS MRU results
  immediately; stream later results (reuse wake/snapshot mechanism or a new channel).
- Concurrency: reuse WolfCore's tokio runtime.
- Git library choice; sqlite crate + flake.nix additions (system sqlite/libgit2 if not
  vendored).
- How selection feeds Open + MRU recording.

### Constraints
- Build gate: ALL builds/tests via `nix develop --command` (direnv stale after flake edits).
- wolf-core GUI denylist MUST stay clean (rusqlite/git libs OK in core; GTK only in
  wolf-gnome). New deps must NOT pull GUI into wolf-core's tree.
- Prefer real-nvim/real-FS integration tests (no mocks). Every source dir needs a README.
  Docs are source of truth.

### Relevant existing surface
- wolf-core: `WolfCore::start/read_snapshot/send_input/resize/shutdown`, `CoreConfig`,
  `WakeFn = Arc<dyn Fn()+Send+Sync>`, ordered input channel; `InputEvent::Open { path }`
  already maps to nvim `edit <fnameescape path>` (input.rs). tokio multi-thread runtime
  owned by WolfCore.
- wolf-gnome: app.rs (window/DrawingArea/wake marshalling via async-channel +
  glib::spawn_future_local), pure seams pattern (gdk_key, geometry, cursor) +
  unit/golden/roundtrip/cage-smoke tests.

## Progress log
- (orchestrator) Plan reset for file-picker (staged). Next: architect for the staged plan.
