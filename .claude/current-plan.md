# Plan: MRU cache follows Neovim's current directory

## STATUS: COMPLETE ✅

Per-cwd MRU implemented; 13/13 real-nvim tests green on the macOS bazel host
(incl. both headline read + write tests); reviewer APPROVED (no nits needing code
changes; race/threading/isolation/relativization all confirmed sound); docs verified
accurate by the reviewer + prettier-clean. Nothing committed (standing rule). Net: the
picker's MRU entries are now relative to Neovim's live cwd and stored at
`<nvim-cwd>/.wolf/mru.sqlite`, switching on `:cd`/`:lcd`/`:tcd` for both seeding and
recording. Outstanding (host limitation): run the Linux `cargo fmt`/`clippy` gate to
confirm formatting/lints (no cargo on the macOS host; code is rustfmt-clean by inspection).


## Problem statement

The file picker shows some MRU entries as ABSOLUTE paths (see user screenshot:
`/Users/jplein/.../runtime/nvim/README.md` etc.) while traversal entries are
relative. Root cause (the documented Decision-4 limitation, now being RESOLVED):
the single `WolfCore.mru: MruStore` is opened ONCE at startup `CoreConfig.cwd`
(the bundle/runfiles dir under `bazel run`), and `MruStore::recent()` relativizes
stored paths against THAT cwd; files actually under Neovim's live cwd can't be
relativized against the bundle dir → shown absolute.

The file-search ROOT already follows Neovim's live `getcwd()` (prior increment).
Now the MRU store + relativization must follow the same cwd.

### Requirements
1. MRU entries' `display` must be **relative to Neovim's current directory**.
2. When Neovim's cwd changes (`:cd`/`:lcd`/`:tcd`), the MRU cache in use must be
   `<nvim-cwd>/.wolf/mru.sqlite` — for BOTH:
   - **seeding** the picker (read, in `start_file_search`), and
   - **recording** opened files (write, currently in the async input pump's
     `InputEvent::Open` handling at api.rs:152-157).

### Approach (to be detailed by architect)
Replace the single `WolfCore.mru: MruStore` (opened at cfg.cwd) with per-cwd
resolution: a per-directory `MruStore` (e.g. a small cache keyed by canonical cwd,
lazily opened at `<cwd>/.wolf/mru.sqlite`), used for both seeding and recording.

### KEY DESIGN TENSION the architect MUST resolve decisively
How the WRITE path (recording opens in the tokio input pump) learns nvim's current
cwd:
- (A) **Continuous tracking** of nvim's cwd into shared state via a `DirChanged`
  autocmd → `wolf_cwd`-style notification (aligned with existing `current_file` /
  `active_win` tracking; avoids an RPC per open; is the multi-window foundation), OR
- (B) **On-demand resolution** (`block_on(getcwd)`) at each operation.
Recommend and justify ONE.

## Constraints
- Change is in **wolf-core** (MRU store mgmt + file-search seed + input-pump record
  + possibly cwd tracking via init.lua autocmd + redraw handler). **wolf-core stays
  GUI-free.** The **FFI surface stays unchanged** (no swift-bridge regeneration).
- Tests use a **real Neovim**: start core, `:cd` to a temp project dir, open a file,
  reopen the picker, assert the MRU entry appears RELATIVE to nvim's cwd AND that the
  store lives at `<nvim-cwd>/.wolf/mru.sqlite`.
- Update existing tests (`crates/wolf-core/tests/{mru.rs, mru_open_records.rs,
  file_search.rs}`) and docs (the Decision-4 limitation note in `architecture.md` +
  the wolf-core READMEs is now RESOLVED, not deferred).

## Relevant code (confirmed by orchestrator)
- `crates/wolf-core/src/api.rs`: `WolfCore.mru: MruStore` (field), opened at
  `MruStore::open(&cfg.cwd)` (api.rs:111); cloned into the input pump
  (`pump_mru = mru.clone()`, api.rs:140) which records opens (api.rs:152-157);
  `mru_recent()` (api.rs:214); `start_file_search` seeds `FileSearchState::seeded(
  self.mru.recent())` (api.rs:233) and roots the traversal at `resolve_search_root()`
  (already follows nvim cwd). `WolfCore` also holds `runtime`, `nvim` (with
  `nvim.current_dir()` getcwd helper added last increment), and `cwd` (fallback).
- `crates/wolf-core/src/mru.rs`: `MruStore::open(&cwd)` → `<cwd>/.wolf/mru.sqlite`;
  `recent()` relativizes `display` against the store's cwd (mru.rs:160-167);
  `record(path)`. `MruStore` is Arc-backed/cheap to clone.
- `crates/wolf-core/src/nvim.rs`: `setup_current_file_tracking` / `setup_layout`
  show the init.lua autocmd → `rpcnotify` → redraw-handler pattern (for option A);
  `NvimProcess::current_dir()` does no-arg `getcwd` (for option B). Neovim spawns in
  `cfg.cwd` (set last increment).
- `runtime/nvim/init.lua`: hosts `wolf_setup_current_file_tracking` /
  `wolf_setup_layout` helpers (for an option-A `DirChanged` autocmd).
- `crates/wolf-core/src/redraw.rs`: the `RedrawHandler` routes `wolf_*` notifications
  into snapshot/state (for option A).

## Orchestrator approval of architect's flagged items (ALL APPROVED)

1. **Option A (continuous cwd tracking) for the WRITE path** — APPROVED. Decisive
   justification: the input pump runs ON a tokio runtime worker thread, so
   `block_on(getcwd)` there would PANIC ("cannot start a runtime from within a
   runtime") — Option B is technically impossible for the write path. So a
   `DirChanged` autocmd → `wolf_cwd` notification → shared `TrackedCwd` cell, read
   synchronously by the pump.
2. **READ path keeps `block_on(getcwd)` via `resolve_search_root`** — APPROVED. It
   already resolves the canonical traversal root; reuse it for the MRU seed store so
   the seed's `display` relativizes against the SAME dir the traversal lists
   (relativization parity; avoids cell-vs-getcwd skew).
3. **init.lua `wolf_setup_dir_tracking` + `wolf_cwd` notification** — APPROVED.
4. **cwd tracked in a sibling `TrackedCwd` cell, NOT the `Snapshot`** — APPROVED (no
   front-end consumes nvim cwd yet; promoting to Snapshot later is additive).

## APPROVED PLAN

### Decision summary
- Write path: Option A (tracked cell). Read path: keep `resolve_search_root()`
  (block_on getcwd), reuse its result for the seed store. Both target nvim's live cwd.
- Per-cwd MruStore cache replaces the single `WolfCore.mru`.

### `runtime/nvim/init.lua`
- Add `_G.wolf_setup_dir_tracking(chan)`: augroup `WolfDir` (clear=true); a
  `DirChanged` autocmd (pattern `*`, covers `:cd`/`:lcd`/`:tcd`) that
  `rpcnotify(chan, "wolf_cwd", vim.fn.getcwd())`; PLUS one immediate emit at setup
  (setup runs after VimEnter, like the other two helpers).

### `crates/wolf-core/src/nvim.rs`
- Add `pub async fn setup_dir_tracking(&self) -> Result<(), NvimError>` mirroring
  `setup_current_file_tracking` (get channel id via `nvim_get_api_info`, then
  `nvim_exec_lua("wolf_setup_dir_tracking(...)", [chan])`).

### `crates/wolf-core/src/redraw.rs`
- `pub type TrackedCwd = Arc<parking_lot::RwLock<Option<PathBuf>>>;` (sibling to
  `IntentQueue`). `const CWD_EVENT: &str = "wolf_cwd";`. Add `cwd: TrackedCwd` field +
  `RedrawHandler::new(...)` param. `handle_notify` arm `CWD_EVENT => self.apply_cwd(args)`
  — NO wake (cwd isn't render state; `:cd` already flushes). `fn apply_cwd(&self, args)`
  (sync): parse `args.first().and_then(Value::as_str)`; non-empty → store
  `Some(canonicalize(p).unwrap_or(PathBuf::from(p)))`; empty/missing → leave unchanged.
  Reads/writes a plain RwLock, never the snapshot lock (clone-out pattern to avoid
  `significant_drop_in_scrutinee`).

### `crates/wolf-core/src/api.rs`
- `type MruCache = Arc<Mutex<HashMap<PathBuf, MruStore>>>;` (parking_lot Mutex, keyed
  by CANONICAL cwd). Free `fn mru_for(cache: &MruCache, cwd: &Path) -> Option<MruStore>`:
  canonicalize key (same expr as `resolve_search_root`); return cached clone or
  `MruStore::open(&key)` (insert + clone); on open failure `tracing::warn!` + return
  `None` (GRACEFUL — never breaks picker/input). Hold the brief lock across the
  synchronous open (never across `.await`).
- `WolfCore`: REMOVE `mru: MruStore`; ADD `mru_cache: MruCache` + `tracked_cwd: TrackedCwd`;
  KEEP `cwd` (fallback). 
- `start`: remove eager `MruStore::open(&cfg.cwd)?`; create cache + cell; pass
  `tracked_cwd.clone()` into `RedrawHandler::new`; after `setup_layout().await?` call
  `proc.setup_dir_tracking().await?`; PRE-WARM `let _ = mru_for(&mru_cache, &cfg.cwd);`
  (creates `<cfg.cwd>/.wolf` eagerly — preserves the `.wolf`-exists-after-start
  invariant the tests rely on; graceful so `start` no longer fails with
  `CoreError::Mru`). Pump: replace `pump_mru` with `pump_mru_cache` + `pump_tracked_cwd`
  + `pump_fallback_cwd`; record into `mru_for(&cache, &(tracked_cwd.read().clone()
  .unwrap_or(fallback)))` if `Some`.
- `mru_recent`: read tracked cwd (fallback `self.cwd`) → `mru_for(...).map(recent)
  .unwrap_or_default()`.
- `start_file_search`: keep `let root = self.resolve_search_root();`; seed =
  `mru_for(&self.mru_cache, &root).map(|s| s.recent()).unwrap_or_default()`; pass `root`
  to `spawn_traversal`. Replace the Decision-4 limitation doc paragraph with: seed now
  follows nvim's live cwd, from `<resolved-root>/.wolf`, root-relative displays.
- Keep the `CoreError::Mru` variant (public API; just no longer produced by `start`).

### Decision: NO new exclusion / NO Snapshot field
`.wolf/` is already excluded by the traversal (confirmed; tests #1/#4/#6 assert it);
`MruStore::open` already `create_dir_all`s `<cwd>/.wolf`. cwd lives in `TrackedCwd`, not
`Snapshot`.

### FFI / front-ends UNCHANGED
`start_file_search` / `mru_recent` / `CoreConfig` signatures unchanged ⇒ NO swift-bridge
regeneration. wolf-core stays GUI-free (only std + parking_lot + tracing; all present).

### Tests (real nvim)
- UNCHANGED (must pass): `mru.rs` (MruStore API unchanged); `mru_open_records.rs::
  open_records_file_in_mru` (no `:cd` ⇒ tracked cwd == cfg.cwd); `file_search.rs` #1–#10
  (.wolf preserved by pre-warm; cfg.cwd seed store = same sqlite via lazy mru_for).
- NEW `file_search.rs::mru_seed_follows_nvim_cd` (read/seed, HEADLINE): dirs A,B; in B
  record nested `B/sub/recent_b.md` into B's store directly; in A record `A/recent_a.md`.
  Start core in A, `:cd <B>` (keystrokes + wait_for_flush), start_file_search,
  poll_until_complete. Assert: an entry with `display == rel(["sub","recent_b.md"])`
  (RELATIVE to B — guards the screenshot bug) with `mru_rank.is_some()`; displays do NOT
  contain `recent_a.md` (per-project isolation); `B/.wolf/mru.sqlite` exists.
- NEW `mru_open_records.rs::open_after_cd_records_in_nvim_cwd_mru` (write): build core
  inline retaining FlushWaiter (pull `common::{ch,key,type_str,wake_channel,config_in}`);
  start in A, `:cd <B>`, wait; `send_input(InputEvent::Open { B/written.md })`; poll.
  Assert `mru_recent()` (now B's store) contains `written.md` B-relative; `MruStore::open(
  &B).recent()` contains it; `MruStore::open(&A).recent()` does NOT (wrote to B's store).
- Graceful-open-failure left unasserted (hard to force on a real fs; the `None` +
  `unwrap_or_default` is the guarantee).

### Docs to reconcile
`architecture.md` (resolve the Decision-4 limitation note; add `wolf_cwd` to the
out-of-band tracking enumeration + MRU bullet); `crates/wolf-core/README.md`
(`mru_recent`/`start_file_search`/`CoreConfig.cwd`; `start` no longer fails on initial
MRU open); `crates/wolf-core/src/README.md` (api.rs MruCache/mru_for/tracked_cwd; nvim.rs
setup_dir_tracking; redraw.rs wolf_cwd/CWD_EVENT/apply_cwd/TrackedCwd; mru.rs per-cwd
note); `runtime/nvim/README.md` (wolf_setup_dir_tracking subsection);
`crates/wolf-core/tests/README.md` (new tests). prettier --write touched Markdown.

### Implementation order (compiles at each step)
1. redraw.rs (TrackedCwd, CWD_EVENT, field+new param, handle_notify arm, apply_cwd).
2. nvim.rs (setup_dir_tracking).
3. api.rs (imports, MruCache, mru_for, field swap, start incl. setup_dir_tracking call +
   pre-warm + pump, mru_recent, start_file_search).
4. init.lua (wolf_setup_dir_tracking) — needed before tests run.
5. Docs.
6. (test-writer) two new tests; then run full wolf-core integration suite (real nvim).

### Approval / dependencies
No new crate, no flake change, no FFI regen. Run `cargo fmt`/`cargo clippy` gate (Linux)
and the bazel `//crates/wolf-core:file_search` rust_test (macOS host) — note: the write
test lives in `mru_open_records.rs`, which has NO bazel rust_test target yet (only
`file_search` does). Coder must wire a bazel rust_test for `mru_open_records` too (mirror
`file_search`) OR run it via cargo on Linux — orchestrator will decide at coder step.

## Tests written (test-writer)
- `file_search.rs::mru_seed_follows_nvim_cd` (read/seed headline): A,B dirs; B records
  nested `sub/recent_b.md`, A records `recent_a.md`; `:cd <B>`; assert one entry
  `display==rel(["sub","recent_b.md"])` with `mru_rank.is_some()`, no `recent_a.md`,
  `<B>/.wolf/mru.sqlite` exists. Reuses existing helpers (`start_core_in_with_waiter`,
  `temp_cwd`, `rel`, `displays`, `poll_until_complete`). `mru_rank` via public field
  `e.mru_rank: Option<u32>` (matches existing seed test).
- `mru_open_records.rs::open_after_cd_records_in_nvim_cwd_mru` (write): broadened imports
  (`InputEvent, Key, MruStore`), replicated local `temp_cwd` + `start_core_in_with_waiter`
  + `has_display`; `:cd <B>`; `send_input(InputEvent::Open { B/written.md })`; assert
  `mru_recent()` has `written.md` B-relative, `MruStore::open(&B).recent()` has it,
  `MruStore::open(&A).recent()` does NOT.
- Append-only; existing tests untouched; public API only. Both PASS only after the coder
  lands per-cwd MRU + `wolf_cwd` tracking.

## Implementation result (coder) — ALL TESTS GREEN on the macOS bazel host

Implemented per plan: redraw.rs (`TrackedCwd`/`CWD_EVENT`/`apply_cwd` + field/param,
no-wake), nvim.rs (`setup_dir_tracking`), api.rs (`MruCache`/`mru_for`/`tracked_cwd`,
field swap, start incl. setup_dir_tracking call + pre-warm + pump record via tracked
cwd, mru_recent, start_file_search seed from resolved root), init.lua
(`wolf_setup_dir_tracking` DirChanged + immediate emit). Wired a NEW bazel
`mru_open_records` rust_test (copy of `file_search`) so the write test runs here. Docs
updated + prettier'd.

Verbatim:
- `bazel build //crates/wolf-core:wolf_core` PASS.
- `bazel test //crates/wolf-core:file_search` → 11 passed, 0 failed (incl. headline
  `mru_seed_follows_nvim_cd` + the prior 10).
- `bazel test //crates/wolf-core:mru_open_records` → 2 passed, 0 failed
  (`open_after_cd_records_in_nvim_cwd_mru` NEW + `open_records_file_in_mru` existing).
- `bazel query kind(rust_test,...)` → only those two targets exist; `mru.rs`/
  `current_file.rs` have no bazel target (run via cargo on Linux; unaffected — MruStore
  API unchanged).
- `bazel build //crates/wolf-ffi:wolf_ffi` PASS (FFI signatures unchanged, no regen).

`wolf_cwd`/`DirChanged` worked first try (write test passing proves the pump's tracked
cwd reached B; no wiring fix needed). wolf-core GUI-free (no new dep); `start_file_search`
/`mru_recent`/`CoreConfig` signatures unchanged; `TrackedCwd`/`MruCache`/`mru_for` internal.

CAVEAT (host limitation): cargo fmt/clippy NOT runnable on macOS (no cargo). Coder
proactively rustfmt-formatted; the LINUX fmt/clippy gate must still confirm. The Lua LSP
"undefined global vim" diagnostics are a pre-existing whole-file false positive (vim is a
Neovim runtime global), NOT from this change.

## Status: AWAITING REVIEW
