# Plan: Fix the file-picker / Command-T hang — COMPLETE ✅

## Problem statement

The AppKit main thread freezes for ~23s when opening the "Go to File" picker.
Diagnosed by symbolicating `~/Dustbin/hang-report.md` against the built binary
(UUID matches).

Frozen main-thread stack:

```
AppDelegate.handleCommandMenuItem
  -> CommandDispatcher.dispatch
    -> WolfWindowController.openFilePicker()
      -> PickerView.open()
        -> WolfCoreHandle.start_file_search()
          -> WolfCore::resolve_search_root()          (crates/wolf-core/src/api.rs:703)
            -> tokio Runtime::block_on(nvim.current_dir())   <-- blocks main thread, no timeout
              -> parked in condvar 22+s (Neovim not answering the getcwd RPC)
```

All tokio workers are parked; the IO driver is parked in `mio::poll` waiting for
bytes from Neovim — so the Rust side correctly sent the `getcwd()` request and is
waiting. Neovim was momentarily unresponsive, and because the RPC runs as a
synchronous `block_on` on the main thread with **no timeout**, the whole UI
freezes until Neovim recovers.

## Root cause

`WolfCore::resolve_search_root` is the only hot-path call that does a blocking
`getcwd()` RPC on the AppKit main thread. It runs on every "Go to File" picker
open. The core already tracks Neovim's live cwd out-of-band (the
`wolf_cwd`/`DirChanged` notification -> `tracked_cwd`, already canonicalized),
exposed synchronously as `WolfCore::live_cwd()`. Sibling methods
(`mru_recent`, `pane_status`, `pane_tabs`) all read `tracked_cwd` precisely to
avoid a main-thread `getcwd` block. `resolve_search_root` is the odd one out.

## Fixes

### Primary — remove the blocking RPC from resolve_search_root
Change `resolve_search_root` to read the tracked live cwd instead of doing a
blocking `getcwd()` RPC:

```rust
self.live_cwd()
    .unwrap_or_else(|| std::fs::canonicalize(&self.cwd).unwrap_or_else(|_| self.cwd.clone()))
```

Same fallback (configured cwd, canonicalized) it uses today on RPC error.
Keep `NvimProcess::current_dir()` as-is (tests may use it); just stop calling it
here.

### Secondary — timeout hardening (defense-in-depth)
`WolfCore::copy_visual_selection` and `WolfCore::list_buffers` use the same
`block_on`-on-main-thread pattern but genuinely need live nvim data. Wrap each
`block_on` future in a bounded `tokio::time::timeout` (mirror the precedent in
`crates/wolf-core/src/nvim.rs::shutdown`, named `Duration` consts). On timeout,
`tracing::warn` and return the same graceful empty/None each already returns on
error. Short timeout (~1-2s) with a named, documented const.

## Constraints

- Docs are source of truth (CLAUDE.md). Update `crates/wolf-core/src/README.md`
  and the doc comments on `resolve_search_root`, `copy_visual_selection`,
  `list_buffers`, `start_file_search` so they no longer claim a blocking getcwd
  RPC. Reformat edited Markdown with `prettier --write`.
- Prefer real Neovim integration tests (no mocks). Existing tests
  `file_search_root_follows_nvim_cd` and `file_search_root_defaults_to_cfg_cwd_without_cd`
  must still pass.
- macOS build/test via Bazel: `bazel test //crates/wolf-core:file_search`.

## Status

- [x] Architect plan
- [x] Coder pass 1 — surfaced a test-synchronization issue; orchestrator decision below
- [x] Test-writer: update sync in file_search.rs (wait_for_live_cwd helper)
- [x] Coder pass 2: pure live_cwd() + all tests green
- [x] Review — APPROVED with 2 minor SHOULD-FIX stale field-doc comments
- [x] Docs — fixed both stale field docs; grep sweep confirmed nothing else stale

## Reviewer verdict: Approved with minor notes
Primary + secondary fixes correct; timeout built inside runtime (no reactor
panic); nested Result matches exhaustive with warn on both error+timeout arms;
test re-sync honest (assertions unchanged, bounded poll can't hang). Two stale
struct-field doc comments still describe the old blocking getcwd query:
- api.rs:105-106 CoreConfig.cwd — "picker's LIVE search root is getcwd()... used
  only when that query fails" → reword to tracked-cwd / used until nvim reports.
- api.rs:196-198 WolfCore.cwd — same stale framing → same reword.

## Pass 2 result (all green)
- api.rs: resolve_search_root is now pure non-blocking live_cwd() read (helper
  deleted); NVIM_QUERY_TIMEOUT + timeout-wrapping of copy_visual_selection /
  list_buffers kept; docs reworded.
- nvim.rs: current_dir() doc-only update (no caller, no dead-code warning).
- READMEs updated + prettier.
- file_search: PASS (all 3 named tests). copy_paste: PASS. wolf_core_lib_test:
  PASS. clippy: clean. Pre-existing palette_rank failure (16 vs 17) out of scope.

## ORCHESTRATOR DECISION (after coder pass 1)

Coder pass 1 found: the plan's pure-`live_cwd()` `resolve_search_root` breaks
`file_search_root_follows_nvim_cd` and `mru_seed_follows_nvim_cd`, because the
old `block_on(getcwd)` doubled as a **synchronization barrier** — `wolf_cwd` is a
FIFO notification delivered before the `getcwd` response, so blocking on `getcwd`
guaranteed `tracked_cwd` was applied by the time it returned. `wait_for_flush`
alone is too weak: it can return on a redraw flush that lands before the io_loop
applies `wolf_cwd`.

DECISION: adopt the TRUE non-blocking fix (pure `live_cwd()`, zero main-thread
RPC) and update the TEST synchronization, rather than the coder's conservative
bounded-2s-RPC mitigation. Rationale: (1) the user's complaint is UI freezing —
a 2s freeze on every picker open in a degraded nvim state is still a freeze;
(2) `tracked_cwd` is the established house pattern (mru_recent/pane_status/
pane_tabs); (3) in production the test race is impossible (a user cannot change
cwd and open the picker within the notification round-trip), so updating the test
sync is honest, not papering over a defect.

Required changes for pass 2:
- **api.rs `resolve_search_root`**: make it the pure non-blocking form
  `self.live_cwd().unwrap_or_else(|| std::fs::canonicalize(&self.cwd).unwrap_or_else(|_| self.cwd.clone()))`.
  Remove the bounded-getcwd-RPC + `search_root_fallback` helper from pass 1
  (inline into the one expression). Update its doc comment + `start_file_search`
  doc + READMEs to say "reads the tracked live cwd, NO blocking RPC".
- **KEEP** pass 1's `NVIM_QUERY_TIMEOUT` const and the timeout-wrapping of
  `copy_visual_selection` and `list_buffers` (those genuinely need live RPC; the
  `block_on(async { timeout(...).await })` form is correct).
- **file_search.rs tests** (`file_search_root_follows_nvim_cd`,
  `mru_seed_follows_nvim_cd`): replace the sole `wait_for_flush` barrier with an
  explicit poll that awaits `core.live_cwd()` reflecting `cwd_b` (bounded by
  FLUSH_TIMEOUT) BEFORE calling `start_file_search`. This synchronizes on the
  actual contract (nvim's cwd change has propagated to the tracked cell). The
  assertions (search lists B's files, not A's sentinel) are unchanged.

Out of scope (pre-existing, flag to user): `//crates/wolf-core:palette_rank`
fails on the clean tree (expects 16 static rows, branch has 17 after adding
CloseTab) — unrelated to this hang fix.

---

## Architect's plan (VERIFIED against code)

Implementation order (each step compiles independently):

1. **api.rs — add const** near `INPUT_COALESCE_DELAY` (~line 76):
   `NVIM_QUERY_TIMEOUT: Duration = Duration::from_secs(2)` with a doc comment
   explaining it bounds the two remaining main-thread `block_on` nvim queries
   (`copy_visual_selection`, `list_buffers`), referencing the `nvim::shutdown`
   timeout precedent. `Duration` already imported (line 16).

2. **api.rs — rewrite `resolve_search_root`** (lines 703-713). New body:
   ```rust
   self.live_cwd()
       .unwrap_or_else(|| std::fs::canonicalize(&self.cwd).unwrap_or_else(|_| self.cwd.clone()))
   ```
   No longer touches `self.runtime`/`self.nvim`. Signature stays
   `fn resolve_search_root(&self) -> PathBuf`. Rewrite its doc comment (694-702):
   resolves from the tracked live cwd (`live_cwd()`, already canonical), NO
   blocking RPC, falls back to canonicalizing `self.cwd`; mirrors
   `mru_recent`/`pane_status`.

3. **api.rs — wrap `copy_visual_selection`** (378-387) in
   `tokio::time::timeout(NVIM_QUERY_TIMEOUT, …)`. Nested match on
   `Result<Result<Option<String>, NvimError>, Elapsed>`:
   - `Ok(Ok(Some(text)))` && `!text.is_empty()` → `Some(text)`
   - `Ok(Ok(_))` → `None`
   - `Ok(Err(e))` → `tracing::warn!("copy_visual_selection failed: {e}")`; `None`
   - `Err(_elapsed)` → `tracing::warn!("copy_visual_selection timed out after {NVIM_QUERY_TIMEOUT:?}")`; `None`
   Update doc comment (369-376): drop "same pattern as resolve_search_root
   blocking"; note bounded by `NVIM_QUERY_TIMEOUT`.

4. **api.rs — wrap `list_buffers`** (404-412). Nested match on
   `Result<Result<Vec<BufInfo>, NvimError>, Elapsed>`:
   - `Ok(Ok(buffers))` → `buffers`
   - `Ok(Err(e))` → `tracing::warn!("list_buffers failed: {e}")`; `Vec::new()`
   - `Err(_elapsed)` → `tracing::warn!("list_buffers timed out after {NVIM_QUERY_TIMEOUT:?}")`; `Vec::new()`
   Update doc comment (395-402): drop the resolve_search_root cross-ref; note
   bounded by `NVIM_QUERY_TIMEOUT`.

5. **api.rs — `start_file_search` doc comment** (628-649): reword so root is
   resolved from tracked live cwd (`live_cwd()`/`tracked_cwd` via
   `wolf_cwd`/`DirChanged`), not a live `getcwd()` RPC. Keep MRU-seed wording.

6. **nvim.rs — `current_dir()` doc comment** (184-190) ONLY (no code change):
   note it is retained on the public `NvimProcess` surface but no longer used by
   `resolve_search_root` (which now reads the tracked cwd). It stays `pub`, no
   dead-code warning, no test calls it.

7. **README updates**: `crates/wolf-core/src/README.md` (lines ~148-150, ~299)
   and `crates/wolf-core/README.md` (lines ~58, ~68, ~69, ~72). Reflect:
   resolve_search_root reads tracked live cwd (no RPC); copy/list bounded by
   NVIM_QUERY_TIMEOUT; behavioral contract (follows `:cd`, falls back to cfg cwd)
   unchanged. Then `prettier --write` both; preserve one-long-line-per-bullet
   style, no hard breaks mid-paragraph.

**Testing:** No test files created/modified. Existing tests stay green because
they `wait_for_flush` after `:cd`, by which point `apply_cwd` has written the
canonical cwd into `tracked_cwd`, so `live_cwd()` returns exactly what
`getcwd()` did. Timeout branches not deterministically testable without a
fault-injection seam — rely on existing happy-path coverage (copy_paste,
buffer_rank).

**Bazel commands:**
- `bazel build //crates/wolf-core:wolf_core`
- `bazel test //crates/wolf-core:file_search`
- `bazel test //crates/wolf-core:copy_paste`
- `bazel test //crates/wolf-core:wolf_core_lib_test`
