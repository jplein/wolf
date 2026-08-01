# Plan: Move recent-files MRU cache to the state directory

## Status: COMPLETE ✓

## Progress log
- **test-writer**: rewrote `tests/mru.rs` (8 cases: create/record/filter/order+upsert/prune/state-dir-location/cap/relative-display), added `common::hermetic_state_dir()`, updated `mru_open_records.rs` + `file_search.rs` (3 tests) + stale comments. No BUILD changes (targets already wired).
- **coder**: implemented `mru.rs` rewrite (global self-healing store, `directory`+`file` schema, filter/prune/cap), `lib.rs` re-export, `api.rs` (dropped `MruCache`/`mru_for`/`CoreError::Mru`/pre-warm; `mru: MruStore` field). Docs updated + prettier. Tests GREEN under `bazelisk test`: `:mru` 8/8, `:mru_open_records` 2/2, `:file_search` 11/11 (confirmed not cached). Clippy + rustfmt_core/ffi PASS; wolf-core + wolf-ffi build.
- **reviewer**: APPROVED. All 5 correctness questions verified (write/read directory normalization agree in tracked + fallback paths; prune is best-effort `let _=`; `MRU_LIMIT` per-directory; no panic/unwrap; nothing creates `<cwd>/.wolf` for MRU). One nit: `mru.rs` clock-before-epoch logged at `warn!` vs documented `debug!`.
- **coder (nit fix)**: downgraded `mru.rs:112` `warn!`→`debug!` to match documented contract. `:mru` 8/8, build, rustfmt_core, clippy all PASS.
- **technical-writer**: verified core READMEs accurate; corrected remaining stale MRU-location references — `wolf-gnome/tests/common/mod.rs:67`, `wolf-ffi/tests/common/mod.rs`, `wolf-core/tests/README.md`, `wolf-ffi/tests/README.md`, `src/README.md` (per-cwd phrasings), `redraw.rs`, `init.lua`, `BUILD.bazel`, and ~22 wolf-ffi test-file comments ("per-cwd/`.wolf` MRU" → "global store scoped by live cwd"). Left legitimate `.wolf` traversal-exclusion docs intact.
- **orchestrator**: ran `prettier --write` on the 3 edited `.md` files (all clean/unchanged). Technical-writer's other edits were comment-only (`.rs`/`BUILD.bazel`/`init.lua`) — no impact on build/rustfmt.

## COMPLETE ✓
MRU cache moved to `state_dir()/file-mru.sqlite` with `directory`+`file` schema, directory-filtered recents, no migration. Tested (bazelisk: `:mru` 8/8, `:mru_open_records` 2/2, `:file_search` 11/11), reviewed (APPROVED), documented, formatters green.

## Problem statement

The recent-files MRU cache currently lives in the working directory at
`<cwd>/.wolf/mru.sqlite`. It must move to the platform state directory
(`~/Library/Application Support/Wolf` on macOS, the XDG state dir on Linux),
using the existing `crate::state::state_dir()` helper.

### Requirements

1. Store the recent-files MRU in the state directory (single global DB),
   following the pattern of the sibling stores `command-mru.sqlite` and
   `folder-mru.sqlite`.
2. The MRU table must have a `file` column and a `directory` column,
   replacing the current single `path TEXT PRIMARY KEY` column.
3. When Wolf populates the recent-file list, it must filter to only the rows
   whose `directory` matches the current working directory.
4. Retain `opened_at` ordering (most-recent first).
5. **No migration path** — the old `<cwd>/.wolf/mru.sqlite` scheme is simply
   dropped. Do not attempt to read or migrate the old DB.

### Key files
- `crates/wolf-core/src/mru.rs` — `MruStore`, schema, `open`/`record`/`recent`/`recent_inner`.
- `crates/wolf-core/src/api.rs` — `mru_for` HashMap cache, record on `InputEvent::Open`
  (~L406), `mru_recent()` (~L980), `start_file_search` seed (~L1104-1111), pre-warm (~L340).
- `crates/wolf-core/src/state.rs:194` — `state_dir()` helper.
- `crates/wolf-core/src/command_mru.rs`, `recent_dirs.rs` — sibling-store patterns.
- `crates/wolf-core/README.md` — MRU documentation (source of truth; must be updated).

### Testing rules (CLAUDE.md)
- Prefer real-nvim integration / end-to-end tests over mocks.
- Any new Rust test file must be wired into a `rust_test` target in `BUILD.bazel`
  and confirmed to run under `bazelisk test`.

## Architect plan

### Key decisions
- **DB filename**: `file-mru.sqlite` under `state_dir()` (matches `command-mru.sqlite`/`folder-mru.sqlite`). Const `MRU_DB_NAME = "file-mru.sqlite"`.
- **Schema**:
  ```
  CREATE TABLE IF NOT EXISTS mru (
      directory  TEXT    NOT NULL,
      file       TEXT    NOT NULL,
      opened_at  INTEGER NOT NULL,
      PRIMARY KEY (directory, file)
  )
  ```
  `file` stores the **absolute** path (canonical if exists, else absolutized against `directory`). `FileEntry.path` = `PathBuf::from(file)`; `FileEntry.display` = `strip_prefix(directory)`.
- **Directory normalization**: helper `normalize_dir(dir) = canonicalize(dir).unwrap_or(dir)`, applied identically on write and read so `WHERE directory = ?` matches exactly.
- **Collapse the per-cwd `MruCache` HashMap** into a single global always-on `MruStore` built once in `start` from `state_dir()` (or `disabled()`). Scoping directory passed into `record`/`recent`. Removes `mru_for`, `MruCache`, pre-warm.
- **Infallible + self-heal pattern** mirroring `recent_dirs.rs`/`command_mru.rs`: drop `MruError`; `open` infallible; `record`/`recent` swallow errors. Drop `CoreError::Mru`.
- **Retain**: `opened_at DESC, rowid DESC` ordering, `MRU_LIMIT = 50` per-directory, WAL, busy_timeout=2000, inline missing-file prune (`!path.exists()` → best-effort `DELETE`, `let _ =`, NOT `?`), `mru_rank` 0-based.

### Files to modify
1. `crates/wolf-core/src/mru.rs` — rewrite: new struct `{ dir: Option<PathBuf>, conn: Arc<Mutex<Option<Connection>>> }`; `open(dir)->MruStore`, `disabled()`, `record(&self, directory, file)`, `recent(&self, directory)->Vec<FileEntry>`; self-heal helpers `with_conn`/`db_path`/`init`/`open_or_heal`/`remove_db_files`; `normalize_dir`, `absolute`, `display_for`, `now_millis`. Remove `MruError`.
2. `crates/wolf-core/src/lib.rs` — L84 `pub use mru::{MruStore, MRU_LIMIT};` (drop `MruError`); L31 doc.
3. `crates/wolf-core/src/api.rs` — drop `MruCache` (L38-45) + `mru_for` (L47-74) + `CoreError::Mru` (L106-122); import `use crate::mru::{FileEntry, MruStore};`; struct field `mru: MruStore` (was `mru_cache`); construct in `start` near recent_dirs/command_mru (`state_dir().map(|d| MruStore::open(&d)).unwrap_or_else(MruStore::disabled)`); remove pre-warm (L340-343); pump clone `pump_mru`; record path (L399-409) → `pump_mru.record(&record_cwd, path)`; `mru_recent` (L980-989) → `self.mru.recent(&cwd)`; `start_file_search` seed (L1088-1109) → `self.mru.recent(&root)`; update `CoreConfig.cwd` doc (L99-104).
4. `crates/wolf-core/src/README.md` — rewrite `## mru.rs` (L280-292): new location/struct/schema/signatures; remove `MruError` + per-cwd `MruCache`/`mru_for`. Adjust `## recent_dirs.rs`/`## command_mru.rs` cross-refs.
5. `crates/wolf-core/README.md` — L70 `mru_recent` reword; L24/L102/L267 drop `<cwd>/.wolf`/`MruError` (L102 lists `MruStore`/`MRU_LIMIT` only).

### Test plan (all targets already wired — no new BUILD wiring)
- `crates/wolf-core/tests/mru.rs` (headless, `//crates/wolf-core:mru`) — rewrite to new API; cases: (a) record writes correct dir/file; (b) `recent` filters by directory (A vs B isolation); (c) opened_at ordering + ranks + upsert re-record; (d) missing-file prune (re-open confirms DELETE); (e) DB at `<state_dir>/file-mru.sqlite`, no `.wolf` created; cap at MRU_LIMIT; nested display relative to directory.
- `crates/wolf-core/tests/mru_open_records.rs` (real-nvim, `//crates/wolf-core:mru_open_records`) — rewrite physical-DB assertions to `MruStore::open(&common::hermetic_state_dir())` + `recent(&cwd_b)`/`recent(&cwd_a)`; update doc comments.
- `crates/wolf-core/tests/file_search.rs` (real-nvim, `//crates/wolf-core:file_search`) — `wolf_cache_dir_is_excluded`: drop `<cwd>/.wolf` existence assert, manually create `.wolf/junk.md` to still exercise exclusion; `mru_seed_is_synchronous_and_not_duplicated` + `mru_seed_follows_nvim_cd`: pre-seed via `MruStore::open(&hermetic_state_dir()).record(&cwd, file)`; final assert `<hermetic>/file-mru.sqlite` exists.
- `crates/wolf-core/tests/common/mod.rs` — add `pub fn hermetic_state_dir() -> PathBuf`; fix stale `<cwd>/.wolf/mru.sqlite` comments (L194-197, L254-255).

### Notable correctness points
- Exact filter match hinges on identical `normalize_dir` on write and read.
- Inline-prune `DELETE` must NOT propagate through `with_conn` (would heal/recreate whole DB on transient failure).
- Old `<cwd>/.wolf/mru.sqlite` never read/deleted/migrated; `.wolf` exclusion in `files.rs` left as harmless backstop.
- Lint: lock mutex guard into a named local (avoid `clippy::significant_drop_in_scrutinee`), mirror sibling `with_conn`.

### Optional (comment-only, accuracy): stale `<cwd>/.wolf` MRU comments in wolf-ffi/wolf-gnome test files — not required to pass.
