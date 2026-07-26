# Current Plan: bound the file-search traversal (kill the $HOME-walk I/O storm)

## Confirmed root cause (from a full WOLF_LOG=debug repro)

The earlier `tab_cwd` out-of-band fix WORKED — zero `tab_cwd` lines in the whole
log. The residual "never recovers" is a self-reinforcing disk-I/O storm:

```
current_dir timed out 5s → fallback root=/Users/jplein → spawn_traversal walking /Users/jplein   (x4, concurrent)
… ~3.5 min of grinding …
current_tabpage completed 24µs   ← nvim finally responsive
```

Chain:
1. A window's nvim is wedged (30s sleep). ⌘O → getcwd times out at 5s → picker
   falls back to `self.cwd`. For a HOME-launched empty window that's `/Users/jplein`.
2. It spawns a traversal of the ENTIRE home dir — UNBOUNDED: no depth cap, no
   entry cap, no time budget; `hidden(false)` descends into `~/Library`,
   `~/.cargo`, caches, every worktree (`files.rs:164-172`). Millions of files /
   minutes / heavy disk on a dev Mac.
3. Spamming ⌘O spawns SEVERAL concurrent full-$HOME walks (threads 31/45/46…),
   none cancelled.
4. Those walks saturate disk I/O → starve nvim itself → getcwd keeps timing out
   LONG after the 30s sleep → picker falls back to $HOME AGAIN → MORE home walks.
   Only "recovers" when the walks finally drain (~5 min).

Latent even without a wedge: every empty/new window roots the picker at $HOME
(`startEmptyStateWindow` → `Project.start(launchCwd: homeDir())`), so ⌘O there
always walks all of $HOME.

## Goal (user chose: BOUND THE WALK — safety net, universal, low-risk)

Make a file search INCAPABLE of running away, regardless of root:
1. **Entry cap** — stop streaming after N entries (log truncation, no silent cap).
2. **Wall-clock budget** — stop the walk after T seconds (log truncation).
3. **Max depth** — a sane depth cap via `WalkBuilder::max_depth`.
4. **Skip pathological dirs** — `~/Library`, caches, and similar huge non-project
   trees (architect to choose the mechanism: name-based `filter_entry` extension
   vs absolute-path skip; must not hide legit project files).
5. **No concurrent pileup** — a superseded/closed search must cancel its walk
   promptly, and/or cap the number of concurrent traversals per core, so repeated
   ⌘O can't stack N full walks. Investigate why the log shows multiple concurrent
   home walks from one window (picker close→reopen each starting a new search).

Caps should be generous enough not to hurt real projects (pick sensible defaults;
consider env overrides for tuning/testing, mirroring `WOLF_RPC_TIMEOUT_MS`).

## OUT OF SCOPE (this effort)
- Changing that empty/home windows root the picker at $HOME (a product/UX change —
  the user chose the pure safety-net scope). The bounding makes a home root merely
  "large but bounded," which is sufficient to kill the storm.
- The RPC timeout, SwapExists, observability, tab_cwd work (all landed).

## Grounding pointers
- `crates/wolf-core/src/files.rs`: `spawn_traversal` (~150), `TRAVERSAL_BATCH`,
  the `ignore::WalkBuilder` config (~164), `filter_entry` (~173),
  `has_excluded_component` (~236), `is_listable_file`, `append_batch`,
  `FileSearchState` (top of file), the `cancel: Arc<AtomicBool>` + `FileSearch`
  Drop-cancels path.
- `crates/wolf-core/src/api.rs`: `start_file_search` (~951, spawns the traversal),
  `resolve_search_root` (~1006, the getcwd→`self.cwd` fallback that roots at home).
- `WalkBuilder` supports `.max_depth(Some(n))`; the walk already checks `cancel`
  each iteration and streams in `TRAVERSAL_BATCH` chunks.

## Test policy (real nvim / real FS, no mocks)
- A test that builds a temp dir tree EXCEEDING the entry cap (or depth), starts a
  search, and asserts the results are bounded at the cap, the search terminates
  (marks complete/truncated), and truncation is logged — within the time budget.
- A test that `max_depth` is respected (a file deeper than the cap is not listed).
- A test that excluded dirs (e.g. a `Library`-named subtree) are skipped.
- A cancellation/no-pileup test if feasible (superseded search stops walking).
- See `crates/wolf-core/tests/file_search.rs` + `common/mod.rs` for harness.

## Architect's detailed plan (authoritative)

### files.rs
- New consts near `TRAVERSAL_BATCH` (~42): `SEARCH_MAX_ENTRIES_DEFAULT: usize =
  50_000` (accepted/listable files), `SEARCH_BUDGET_DEFAULT: Duration =
  from_secs(5)`, `SEARCH_MAX_DEPTH_DEFAULT: usize = 64`. Add `use
  std::time::{Duration, Instant};`.
- New crate-visible `SearchBounds { max_entries: usize, budget: Duration,
  max_depth: usize, skip_dirs: HashSet<PathBuf> }` + `pub(crate) fn from_env()`
  reading `WOLF_SEARCH_MAX_ENTRIES` / `WOLF_SEARCH_BUDGET_MS` /
  `WOLF_SEARCH_MAX_DEPTH` via the api.rs:233 guard pattern
  (`.ok().and_then(parse).filter(|&n| n>0).map(..).unwrap_or(DEFAULT)`);
  `skip_dirs` empty (caller populates).
- `FileSearchState`: add `pub truncated: bool` (doc: complete-but-not-exhaustive);
  `seeded()` sets false.
- `spawn_traversal` (~150): add ONE `bounds: SearchBounds` param (struct, not 3
  scalars → avoids `too_many_arguments`). In the closure: `let started =
  Instant::now(); let mut accepted = 0usize; let mut truncated = false;`.
  `WalkBuilder`: add `.max_depth(Some(bounds.max_depth))`. Extend `filter_entry`
  (keep `.git`/`.wolf`) to also reject `bounds.skip_dirs.contains(entry.path())`
  (move a clone of skip_dirs in). In the loop, alongside the `cancel` check: if
  `started.elapsed() >= bounds.budget` → set truncated, `warn!` (root, budget_ms,
  accepted), break. After `batch.push`: `accepted += 1; if accepted >=
  bounds.max_entries` → set truncated, `warn!` (root, cap), break. Post-loop
  `complete = !cancel.load()` (cap-truncation ⇒ cancel==false ⇒ complete==true;
  real cancel ⇒ complete==false, truncated==false).
- `append_batch` (~226): add `truncated: bool` param → `if truncated {
  guard.truncated = true; }`. Mid-loop call passes false; final passes computed.
- NOTE: max_depth pruning is invisible in the `ignore` API, so depth-limited
  results do NOT set `truncated` (silent, like gitignore). `truncated` reflects
  entry-cap + time-budget only. Depth test asserts via ABSENCE of a deep file.

### api.rs `start_file_search` (~943)
After resolving `root`+`seed`: `let mut bounds = files::SearchBounds::from_env();`
then populate `bounds.skip_dirs` from `canonical_home()` (~212): if `Some(home)`,
insert `home.join("Library")`, `home.join(".Trash")`, `home.join(".cache")` (ORCH
DECISION: KEEP this dir-skip — zero false positives, kills the ~/Library offender
before the 5s budget). Pass `bounds` as the new final arg to `spawn_traversal`.
Env read here (calling thread, once per search) → deterministic for tests. Extend
the doc comment to mention caps + the 3 env vars.

### complete vs truncated
`complete` is NOT bridged to Swift (FFI `FfiFileSearch` lib.rs:3149-3234 exposes
only ranked/children_of/version/rel_path_for; Swift gates on `version()`).
`complete` consumed only by core tests' `poll_until_complete`. So truncation sets
`complete=true` (free via `!cancel`); `truncated` is added for TEST DETERMINISM
(assert truncation w/o log capture) + future UX. No FFI change.

### Concurrency: NO new machinery (assessed). The caps make a never-cancelled or
stacked walk terminate in ≤ budget (~5s), so pileup is harmless. Cancel-on-drop
still supersedes in the common case. Distinct windows = distinct cores (fine).
Per-core "cancel previous search"/semaphore = deliberately out of scope.

### Docs: `crates/wolf-core/src/README.md` (files.rs section ~225-238: the 4
bounds, `truncated`, `SearchBounds`/from_env, 3 env vars+defaults, depth pruning
is silent), `crates/wolf-core/README.md` (~91 FileSearch bullet). prettier.

### Impl order: 1) files.rs data (consts, SearchBounds, truncated field). 2)
files.rs behavior (append_batch+spawn_traversal) + 3) api.rs caller — atomically.
4) docs. 5) tests.

### Lint: bundle caps into `SearchBounds` (avoid too_many_arguments); Instant in
spawn_blocking is normal; no new #[allow].

## Tests (test-writer)
NEW SEPARATE binary `crates/wolf-core/tests/file_search_bounds.rs` (isolates its
process-global env mutations from `file_search.rs`'s 11 tests). Use a module-level
`static BOUNDS_ENV_LOCK: std::sync::Mutex<()>` held across
`{set_var; start_file_search}` then remove the var (caps read synchronously in
start_file_search, so holding across that call suffices — no serial_test dep).
Reuse `mod common;` + copy the needed `file_search.rs`-local helpers (`temp_cwd`,
`write_file`, `search_wake`, `start_core_in`, `poll_until_complete`, `displays`,
`rel`).
1. `entry_cap_truncates_and_completes`: `WOLF_SEARCH_MAX_ENTRIES=10`, ~100 flat
   `.md` files (empty MRU), poll complete, assert `complete==true`,
   `truncated==true`, `entries.len()` bounded near cap (==10 since BATCH 256>10).
2. `max_depth_prunes_deep_files`: `WOLF_SEARCH_MAX_DEPTH=2`; `top.md`(d1),
   `a/mid.md`(d2), `a/b/c/deep.md`(d4); assert top+mid present, deep ABSENT.
3. `budget_truncates_quickly`: `WOLF_SEARCH_BUDGET_MS=1`, ~2000 files; assert
   `complete==true` && `truncated==true`, returns within timeout; assert NO exact
   count (loose). ORCH DECISION: make robust (large tree, loose assert); if it
   can't be made non-flaky, DROP it (entry-cap test covers the same break path).
4. NO home-junk-skip integration test (needs a home override — verify by
   inspection). ORCH: agreed.
Existing `file_search.rs` (≤600 files, shallow, no env vars) unchanged — confirm
no assertion reads `truncated` or exceeds defaults.
Coder must add a `file_search_bounds` rust_test Bazel target (mirror `file_search`
/ real-nvim targets: staged nvim tree, no-sandbox).

## Status
- [x] architect — detailed plan
- [x] test-writer — created `tests/file_search_bounds.rs` (3 tests:
      `entry_cap_truncates_and_completes`, `max_depth_prunes_deep_files`,
      `budget_truncates_quickly`) with a `BOUNDS_ENV_LOCK` mutex + bare `set_var`
      (crate not on 2024 edition; mirrors rpc_timeout.rs). CODER CONTRACT: read
      guard must expose `entries`, `complete`, `version`, NEW `truncated: bool`;
      `truncated` visible true in the SAME guard where `complete` first true;
      `from_env()` read SYNCHRONOUSLY in `start_file_search` (not the spawned
      thread) or the lock-then-remove races; entry cap counts ACCEPTED files;
      `max_depth(Some(n))` uses ignore's root=depth-0 convention (root children =
      depth 1). Copied file_search.rs-local helpers locally. Coder MUST add the
      `file_search_bounds` rust_test Bazel target.
- [x] coder — implemented files.rs (consts, SearchBounds/from_env, truncated
      field, bounded spawn_traversal + append_batch) + api.rs (from_env on calling
      thread, skip_dirs from canonical_home) + `file_search_bounds` Bazel target.
      ALL PASS: file_search_bounds 3/3, file_search unchanged, //crates/wolf-core:
      all, //crates/wolf-ffi:all, //crates/wolf-macos:Wolf builds. rustfmt+clippy
      clean (no too_many_arguments — bundled into SearchBounds), denylist clean.
      Depth convention matched (no off-by-one); budget test no flake this run.
- [x] reviewer — APPROVED. Confirmed: (a) truncation → complete==true &&
      truncated==true, real cancel → neither; (b) from_env on calling thread
      (api.rs:966); (c) $HOME walk bounded on all axes (entries/time/depth +
      ~/Library/.cache/.Trash skip) → storm + feedback loop broken. No off-by-one
      (accepted>=cap after push; 0/invalid→default via >0 filter); skip_dirs
      match canonical paths; 6 args (no too_many_arguments); no await_holding_lock.
      Benign edge (cancel same iter after ≥5s → complete=false,truncated=true):
      ORCH ACCEPTED as-is (vanishingly rare, no consumer depends). NITS → tech-
      writer: FileSearchReadGuard doc (files.rs:165) omits `truncated`; READMEs.
- [x] technical-writer — documented the bounds/env-vars/truncated in
      crates/wolf-core/src/README.md + crates/wolf-core/README.md +
      crates/wolf-core/tests/README.md, and fixed the FileSearchReadGuard doc
      nit (files.rs). Orchestrator ran prettier (all clean) + rustfmt_core PASSED.

## COMPLETE ✅

The unbounded $HOME-walk I/O storm — the confirmed cause of "never recovers" — is
fixed: every file search is now capped (50k entries / 5s / depth 64, env-tunable
via WOLF_SEARCH_MAX_ENTRIES / WOLF_SEARCH_BUDGET_MS / WOLF_SEARCH_MAX_DEPTH) and
prunes ~/Library, ~/.cache, ~/.Trash. No walk can run more than ~5s, so it can't
starve nvim or feed back into repeated home walks. Regression tests pin all three
bounds. Full suite green, app builds, rustfmt/clippy/denylist clean. NOT committed.

### This branch now carries FOUR landed efforts (all uncommitted):
1. RPC timeout backstop (5s, WOLF_RPC_TIMEOUT_MS) + SwapExists autocmd.
2. Observability: init_tracing() (stderr, WOLF_LOG) + hot-path logs.
3. Out-of-band per-tabpage cwd (removed the looped blocking tab_cwd RPC).
4. Bounded file-search traversal (this effort).
Lockfiles from effort #2's repin are part of the uncommitted state.
