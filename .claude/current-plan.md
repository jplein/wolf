# Plan: File picker search root follows Neovim's cwd

## STATUS: COMPLETE ✅

All layers implemented, 10/10 real-nvim tests green on the macOS Bazel host,
reviewer APPROVED, docs reconciled (incl. the stale BUILD.bazel comments + tests
README) and prettier-clean. Nothing committed (standing rule). Net effect: the
picker now lists files from Neovim's live `getcwd()` — `:cd <dir>` inside the editor
re-roots the picker — and the captive nvim now starts in `cfg.cwd`. Foundational for
per-window/per-project cwd.


## Problem statement

The file picker's search root is currently the static `cwd` captured at startup
(`CoreConfig.cwd` → `WolfCore.cwd`), set by the front-end from the process working
directory. Under `bazel run //crates/wolf-macos:Wolf` that is the runfiles/bundle
dir, so the picker lists bundle contents and is hard to evaluate.

**Change:** `WolfCore::start_file_search` must resolve the traversal root by
querying the captive Neovim's **current working directory** over RPC (live
`getcwd()`), falling back to the configured `self.cwd` if the query fails or
returns an unusable path. The picker then follows wherever Neovim is (`:cd`/`:lcd`/
`:tcd`), which is foundational for future multi-window/multi-project support where
each window's embedded Neovim owns its own cwd.

## Scope / constraints

- Change is in **wolf-core** only (the search-root resolution inside
  `start_file_search`). The FFI `start_file_search(&self)` signature is UNCHANGED,
  so swift-bridge bindings need NO regeneration. Both front-ends (wolf-gnome,
  wolf-macos) call `start_file_search` and benefit automatically.
- **wolf-core stays GUI-free** (no new GUI dep).
- Tests use a **real Neovim** (project policy): start core, `:cd` to a temp dir
  seeded with known files, `start_file_search`, assert results come from the nvim
  cwd, NOT the process cwd.
- **MRU root question (keep tight):** `MruStore::open(&cfg.cwd)` still opens
  `<cfg.cwd>/.wolf/mru.sqlite`, and the search seeds from `self.mru.recent()`. The
  headline change is the SEARCH root following nvim's cwd. Architect: note the
  resulting MRU/seed-vs-root mismatch and recommend the minimal correct handling
  (e.g. keep MRU as-is for now and document, OR a small adjustment) — but do NOT
  scope-creep into per-project MRU.

## Relevant code (confirmed by orchestrator)

- `crates/wolf-core/src/api.rs`:
  - `WolfCore.cwd: PathBuf` (field), set from `cfg.cwd` in `start`.
  - `start_file_search(&self, wake) -> FileSearch` (api.rs:227): roots the
    traversal at `std::fs::canonicalize(&self.cwd)`, seeds `FileSearchState` from
    `self.mru.recent()`, spawns `files::spawn_traversal(root, ...)` on
    `self.runtime`.
  - `WolfCore` holds `runtime: Runtime` and `nvim: NvimProcess` (`nvim.handle()`
    gives a cloneable `Neovim` RPC handle). `start`/`shutdown` already use
    `runtime.block_on`, so a brief blocking RPC from the main thread is an
    established pattern.
- `crates/wolf-core/src/nvim.rs`: generic untyped RPC via `nvim.call(method,
  args)`. Neovim is spawned with NO `.current_dir()` (nvim.rs:302), so it inherits
  the process cwd unless changed by `:cd`. `NvimProcess::handle()` → `Neovim`.
  Existing helpers (`setup_current_file_tracking`, `setup_layout`) show the
  `nvim_exec_lua` / `nvim_call`/`nvim_eval` patterns and error mapping
  (`NvimError::Rpc`).
- `crates/wolf-core/src/files.rs`: `spawn_traversal(root, state, cancel, wake,
  handle)`; the root is the only directory input.

## Orchestrator approval of architect's flagged item

**APPROVED — spawn the captive nvim in `cfg.cwd`** (companion change in `nvim.rs`).
The architect correctly found that nvim is spawned with NO `.current_dir()`, so it
inherits the PROCESS cwd (the crate dir under `cargo test`, the bundle under `bazel
run`). Trusting `getcwd()` without this would (a) break all 8 existing
`file_search.rs` tests and (b) be incoherent. Spawning nvim in `cfg.cwd` makes
`getcwd()` == `cfg.cwd` at startup (tests pass unchanged) and the picker then
follows `:cd`. This is the right, contained fix. Proceed.

## APPROVED PLAN

### Layer A — `crates/wolf-core/src/nvim.rs`
1. Add `pub cwd: PathBuf` to `NvimConfig`; add a `cwd: PathBuf` param to `from_env`
   and store it.
2. In `spawn`, after the `cmd.arg(...).env(...)` chain, add `cmd.current_dir(&cfg.cwd);`
   so the captive nvim starts in `cfg.cwd` instead of inheriting the process cwd.
3. Add `pub async fn current_dir(&self) -> Result<PathBuf, NvimError>` on
   `NvimProcess` (place with the other async RPC helpers): call
   `nvim_call_function("getcwd", [])` (no-arg getcwd → honors window/tab-local cwd
   of the current window). Two-layer error map to `NvimError::Rpc` (reuse the
   variant). Extract via `Value::as_str`; if `None` or empty-after-trim → `Err(Rpc(..))`
   so the caller's fallback engages; else `Ok(PathBuf::from(s))`.

### Layer B — `crates/wolf-core/src/api.rs`
4. At the `NvimConfig::from_env(...)` call (api.rs:118), pass `cfg.cwd.clone()`.
5. Add private `fn resolve_search_root(&self) -> PathBuf` (placed AFTER the pub
   methods): `match self.runtime.block_on(self.nvim.current_dir())`:
   - `Ok(p)` → `std::fs::canonicalize(&p).unwrap_or(p)` (follow nvim; keep nvim path
     if canonicalize fails — getcwd success means a real dir).
   - `Err(_)` → `tracing::warn!` then fall back to today's behavior:
     `std::fs::canonicalize(&self.cwd).unwrap_or_else(|_| self.cwd.clone())`.
   Canonicalization retained so the MRU canonical-path dedup invariant
   (`FileSearchState::seeded`) holds.
6. In `start_file_search`, replace the `let root = canonicalize(&self.cwd)...` line
   with `let root = self.resolve_search_root();`. Keep `self.cwd` (it's the fallback
   + MRU location).
7. Update doc-comments: `CoreConfig.cwd` (api.rs:44) → "FALLBACK search root + MRU
   location; live root is nvim's `getcwd()`"; `WolfCore.cwd` field (api.rs:96);
   `start_file_search` (api.rs:218) → "rooted at nvim's live cwd, falling back to
   the configured cwd."

**Threading:** `start_file_search` is sync on the UI/main thread;
`runtime.block_on(self.nvim.current_dir())` is the same pattern as `start`/`shutdown`
— one sub-ms getcwd round-trip, no snapshot-lock/input-pump contact, no deadlock.

### Decision 4 — MRU/seed mismatch: LEAVE AS-IS + DOCUMENT (option a)
The search still seeds from `self.mru.recent()` (rooted at `cfg.cwd`); when nvim's
cwd differs, seeded MRU entries may be outside the new root and their `display`
strings are relative to `cfg.cwd`. They still appear (MRU-first). ACCEPTED for this
change (per-project MRU is out of scope). State it in the `start_file_search` doc +
README so the reviewer reads it as intentional, NOT a defect. Do NOT filter/re-root
the MRU now.

### NOT changed
`files.rs`, `mru.rs`, `wolf-ffi`, both front-ends — `start_file_search(&self, wake)
-> FileSearch` signature unchanged ⇒ NO swift-bridge regeneration.

### Tests (real nvim) — extend `crates/wolf-core/tests/file_search.rs`
- Regression gates: the 8 existing `file_search.rs` tests (proof the spawn-cwd
  change works: nvim starts in cfg.cwd ⇒ getcwd()==cfg.cwd ⇒ assertions hold).
  Also keep `current_file.rs`, `mru.rs`, `mru_open_records.rs`, `spawn.rs` green.
- NEW test 1 (headline): two temp dirs A,B (canonicalized like `temp_cwd`). Seed B
  with `alpha.md`/`beta.md`, A with sentinel `only_in_a.md`. Start core with
  cfg.cwd=A. Drive `:cd <B>` via the SAME keystroke pattern `current_file.rs` uses
  for `:e` (`send_input(':')`, `type_str("cd ")`, `type_str(<B>)`, `send_input(Enter)`)
  then `wait_for_flush`. Use ASCII/space-free temp names so the path types literally.
  Build the core inline (via `common::config_in`/`wake_channel`) to retain the
  `FlushWaiter`. Then `start_file_search` + `poll_until_complete`; assert displays
  CONTAIN alpha.md+beta.md and do NOT contain only_in_a.md (rooted at B = nvim cwd).
- NEW test 2 (no-`:cd` ⇒ root is cfg.cwd): core in A seeded `gamma.md`, no `:cd`,
  start_file_search, assert gamma.md listed, root is A. Documents the fallback.
- Do NOT attempt a forced getcwd-RPC-failure test (would need a mock; violates the
  no-mock policy). The empty/`Err` guard is covered by code review + test 2.

### Docs to reconcile afterward
`architecture.md` "Output: file-search session"; `crates/wolf-core/README.md`
(start_file_search bullet, CoreConfig.cwd paragraph, MRU-seed limitation note);
`crates/wolf-core/src/README.md` (nvim.rs `current_dir` + spawn-cwd; api.rs/files.rs
root wording). prettier --write all touched Markdown.

### Implementation order (no breakage at any step)
1. nvim.rs: NvimConfig.cwd + from_env param + `cmd.current_dir` + `current_dir()`.
2. api.rs: pass cfg.cwd.clone() to from_env (now existing behavior preserved AND
   nvim starts in cfg.cwd — regression gate satisfied before the picker trusts
   getcwd).
3. api.rs: resolve_search_root + use in start_file_search + doc-comments.
4. Docs + prettier.
5. (test-writer) extend file_search.rs with the two new tests.

### Dependencies / lint
No new crates; GUI-free rule unaffected; no Cargo/Bazel/flake change. Reuse
`NvimError::Rpc` (no new variant); private helper after pub methods; `tracing::warn!`
on the fallback path; no `#[allow]` expected.

## Progress log
- (orchestrator) Wrote problem statement; architect produced the plan; orchestrator
  APPROVED the one flagged item (spawn nvim in cfg.cwd — mandatory + correct).
  Next: test-writer (extend file_search.rs).

## Tests written (test-writer) — extended `crates/wolf-core/tests/file_search.rs`
- `start_core_in_with_waiter(cwd) -> (WolfCore, FlushWaiter)` — inline builder
  mirroring `start_core_in` but retaining the waiter.
- `file_search_root_follows_nvim_cd` (test 9): core in A, `:cd` to B via keystrokes
  (`ch(':')`, `type_str("cd ")`, `type_str(path_b)`, `key(Enter)`) + `wait_for_flush`;
  asserts displays contain alpha.md+beta.md, not only_in_a.md.
- `file_search_root_defaults_to_cfg_cwd_without_cd` (test 10): core in A seeded
  gamma.md, no `:cd`; asserts gamma.md listed (depends on nvim spawning in cfg.cwd).
- Both drive only the PUBLIC surface; existing tests/helpers untouched (only appended;
  added `Key` to the `use wolf_core::{...}` line).
- RISK TO WATCH (coder verify): the tests assume `:cd <B><CR>` produces ≥1 redraw
  flush after the baseline so `wait_for_flush` returns. If `:cd` doesn't reliably
  flush, the coder must surface it (the flush-wait is the plan's prescribed sync
  point). Coder must satisfy: `cmd.current_dir(&cfg.cwd)`, `NvimProcess::current_dir()`
  via getcwd, `resolve_search_root` wired into `start_file_search`.

## Implementation result (coder, round 1) — CODE DONE + COMPILES; TESTS NOT YET RUN

Implemented all of Layer A (nvim.rs: `NvimConfig.cwd`, `from_env` param,
`cmd.current_dir(&cfg.cwd)`, `current_dir()` getcwd helper) and Layer B (api.rs:
from_env call, `resolve_search_root`, `start_file_search` uses it, doc-comments;
Decision-4 MRU note). Docs updated + prettier'd. `start_file_search` signature
byte-for-byte unchanged; FFI rebuilds; wolf-core GUI-free (no new dep).

Compile gate (what ran here): `bazel build //crates/wolf-core:wolf_core` PASS (recompiled,
10 files), `bazel build //crates/wolf-ffi:wolf_ffi` PASS (no-op rebuild → signature
unchanged confirmed).

VERIFICATION GAP: the cargo integration-test gate could NOT run on this aarch64-darwin
host — no cargo/rustc toolchain, macOS devShell doesn't export WOLF_NVIM_BIN, and there
is NO bazel rust_test target for wolf-core's integration tests. So the 8 existing + 2
new `file_search` tests are UNEXECUTED. Coder's code-review belief: test 9's
`:cd <B><CR>` flush wait should return (cmdline on grid 1 ⇒ ≥1 flush past baseline) and
assertions should hold — but UNVERIFIED.

## Orchestrator decision: wire a bazel rust_test for wolf-core so the tests RUN here

APPROVED scope addition: mirror `//crates/wolf-ffi:bridge_smoke` to add a
`//crates/wolf-core:file_search` (and reuse for the other wolf-core integration tests
if cheap) `rust_test` that stages the pinned nvim (`//bazel/nvim:nvim_tree`) +
`//runtime/nvim:init.lua` as runfiles (`WOLF_TEST_*` env, `tags=["no-sandbox"]`,
`all_crate_deps(normal_dev=True)`), so the real-nvim file_search tests EXECUTE on the
macOS bazel host. This is reusable verification infra (the BUILD.bazel comment that
"tests need a pinned nvim not yet provisioned on macOS" is now resolvable). If
crate_universe dev-deps for wolf-core aren't wired and this balloons, the coder reports
back rather than rabbit-holing, and we fall back to Linux-deferred execution.

## Coder round 2 — bazel rust_test wired; harness gap blocks execution

Added `//crates/wolf-core:file_search` `rust_test` (stages `//bazel/nvim:nvim_tree` +
`//runtime/nvim:init.lua`; `deps = [":wolf_core"] + all_crate_deps(normal=True,
normal_dev=True)` — the `normal=True` is REQUIRED because crate_universe drops a dep
from the dev map when it's also a normal dep, so `tempfile` was missing with
`normal_dev=True` alone). NO repin / lockfile / MODULE change needed. All 10 tests are
discovered + RUN, but all fail at `WolfCore::start` with `Nvim(Spawn(NotFound))`:

ROOT CAUSE (definitive): wolf-core's `tests/common/mod.rs` reads `WOLF_NVIM_BIN`
LITERALLY (no runfiles resolution) and resolves init.lua from compile-time
`CARGO_MANIFEST_DIR`. Under Bazel, `$(rootpath)` is runfiles-RELATIVE, and now that nvim
is spawned with `current_dir(cfg.cwd)` (a temp dir), `Command::new(<relative>)` →
NotFound. wolf-ffi's `bridge_smoke` works ONLY because ITS harness joins absolute
`$TEST_SRCDIR` and reads `WOLF_TEST_INIT_LUA`/`WOLF_TEST_NVIM_DIR`. wolf-core's harness
lacks both. Secondary: even with an absolute nvim bin, `CARGO_MANIFEST_DIR` init.lua is
absent at runtime under Bazel → "Press ENTER" hang. There is NO BUILD-only fix.

Test 9 flush outcome: NOT REACHED (dies before spawn). 8 existing tests: NOT VALIDATED.
Also: wolf-core's harness PANICS (vs wolf-ffi's loud-skip) when nvim unresolved, so the
new target FAILS under `bazel test //...` on macOS until the harness resolves nvim.

## Orchestrator decision: test-writer ports wolf-ffi's runfiles resolution into wolf-core's common

Route to TEST-WRITER (it's test code: `crates/wolf-core/tests/common/mod.rs`). Make the
harness ADDITIVELY prefer the Bazel-staged absolute paths, mirroring
`crates/wolf-ffi/tests/common/mod.rs` EXACTLY, WITHOUT breaking the cargo/Linux path
(an explicit `WOLF_NVIM_BIN` must still win for cargo/devShell). This is the same,
already-proven pattern — clean port, leaves wolf-core tests runnable on macOS.

## Tests GREEN on the macOS Bazel host (coder round 3)

`bazel test //crates/wolf-core:file_search --test_output=all` → `test result: ok. 10
passed; 0 failed`. All 8 existing tests pass (proves nvim now spawns in cfg.cwd ⇒
getcwd()==cfg.cwd at startup). Both new tests pass:
- `file_search_root_follows_nvim_cd` (TEST 9, headline) — PASS; the `:cd <B><CR>`
  flush-wait did NOT hang (flagged risk did not materialize). Picker follows nvim's cwd.
- `file_search_root_defaults_to_cfg_cwd_without_cd` (test 10) — PASS.
`bazel build //crates/wolf-ffi:wolf_ffi` still succeeds (signature unchanged). No
implementation bugs surfaced; no test changes needed.

## Review result (reviewer) — APPROVED WITH NITS (none blocking)

Confirmed: fallback chain robust (always returns a valid path, never panics, logs
the degraded path); threading safe (`block_on(current_dir())` same as start/shutdown,
no snapshot-lock/input-pump contact, no deadlock); no-arg getcwd semantics correct
(window/tab-local, always absolute) and right for the multi-window goal; Decision-4
MRU limitation left as-is + documented (intentional); GUI-free preserved; FFI
signature byte-for-byte unchanged (no regeneration); change confined to wolf-core +
the test harness. New tests non-tautological. spawn-in-cfg.cwd present + correctly
placed (nvim.rs:356).

NITS (docs-only → route to technical-writer / Docs step):
1. `crates/wolf-core/BUILD.bazel` has 3 STALE comment blocks contradicting reality:
   lines ~5-7 ("no rust_test... tests need pinned nvim not yet provisioned on macOS"),
   lines ~29-38 ("KNOWN BLOCKER... tests fail at WolfCore::start"), lines ~56-59
   ("harness reads WOLF_NVIM_BIN directly / CARGO_MANIFEST_DIR"). All now FALSE — the
   target exists, the harness was ported, tests are 10/10 GREEN. Remove/rewrite.
2. `crates/wolf-core/tests/README.md` doesn't mention the new
   `bazel test //crates/wolf-core:file_search` runfile-staged target nor the two
   cwd-following tests (9/10). (crate README + src/README already updated by coder.)

## Status
- [x] Architect plan
- [x] Tests written
- [x] Implementation + tests passing (10/10 on macOS bazel host)
- [x] Review (APPROVED WITH NITS — docs-only)
- [x] Docs (BUILD.bazel stale comments fixed; tests/README.md updated; coder's
      architecture.md/README.md/src/README.md edits verified accurate; prettier clean)
