# Current Plan: bound synchronous nvim RPCs + make swap prompts impossible

Source of truth for this effort is `rpc-timeout-plan.md` at the repo root. This
file is the orchestrator's working copy and will be replaced with the
architect's detailed plan.

## Scope (confirmed with user)

Implement TWO of the three separable efforts from `rpc-timeout-plan.md`:

- **(a) RPC timeout backstop** — a central `block_on_rpc` chokepoint on
  `WolfCore` that wraps every interactive synchronous nvim RPC in a
  `tokio::time::timeout`, converting an unbounded main-thread hang (macOS
  beach-ball) into a bounded stall that degrades to each method's existing
  fallback, plus a `tracing::warn!` naming the wedged RPC.
- **(b) SwapExists autocmd** — add a `SwapExists` autocmd to
  `runtime/nvim/init.lua` that sets `vim.v.swapchoice = "e"` so a stale swap
  file never blocks the captive nvim.

OUT OF SCOPE for this run (deferred to a separate PR, per the plan):

- (c) the inline cmdline `prompt` render/answer UX in `CmdlineView.swift`.
- The off-main-thread follow-up for the hot picker/preview paths.

## Confirmed open decisions

1. `RPC_TIMEOUT_DEFAULT` = **5 seconds** (matches `SHUTDOWN_TIMEOUT`).
2. Env-override escape hatch **`WOLF_RPC_TIMEOUT_MS`**: **yes** — resolved once
   at `WolfCore::start`, stored as a `Duration` field on `WolfCore`. Makes the
   integration test deterministic and gives operators an escape hatch.
4. `SwapExists` policy: **`"e"`** (edit anyway) — Wolf is hermetic and does not
   manage recovery.

## Design summary (from rpc-timeout-plan.md)

### (a) RPC timeout

1. Add a `Timeout(&'static str, Duration)` variant to `NvimError` in
   `crates/wolf-core/src/nvim.rs`. Existing call sites already match
   `Ok(_) => …, Err(_) => <fallback>`, so the new `Err` variant needs no
   per-site branch change.
2. Add `const RPC_TIMEOUT_DEFAULT: Duration = Duration::from_secs(5);` and a
   `block_on_rpc<F, T>(&self, what: &'static str, fut: F)` helper on `WolfCore`
   in `crates/wolf-core/src/api.rs` that runs
   `self.runtime.block_on(tokio::time::timeout(self.rpc_timeout, fut))`, logging
   a `warn!` and returning `NvimError::Timeout` on elapse.
3. Store `rpc_timeout: Duration` on `WolfCore`, resolved at `WolfCore::start`
   from `WOLF_RPC_TIMEOUT_MS` (falling back to `RPC_TIMEOUT_DEFAULT`).
4. Route the interactive RPC sites through the helper (see the table in
   `rpc-timeout-plan.md`, sites at api.rs 377, 414, 437, 471, 484, 486, 539,
   565, 587, 630, 654, 672, 947). `start_file_search` is covered transitively
   via `resolve_search_root`. Explicitly NOT rerouted: startup `block_on`
   (api.rs:251) and shutdown (api.rs:933).

### (b) SwapExists autocmd

Add a `SwapExists` autocmd to `runtime/nvim/init.lua` setting
`vim.v.swapchoice = "e"`.

## Tests (from rpc-timeout-plan.md)

- Unit test of `block_on_rpc` with a never-resolving future under a short
  `rpc_timeout`; assert `Err(NvimError::Timeout(..))` within a small bound.
- Integration test `crates/wolf-core/tests/rpc_timeout.rs`: real nvim, small
  `WOLF_RPC_TIMEOUT_MS` (~200ms), wedge nvim with a FINITE
  `:lua vim.uv.sleep(<3x timeout>)<CR>`, call interactive methods
  (`start_file_search`/`resolve_search_root`, plus one of `open_preview` /
  `current_tabpage`), assert fallback returned within ~`RPC_TIMEOUT`, record
  elapsed to prove the bound.
- SwapExists test: create a stale swap file for a fixture, open it through the
  captive nvim, assert it opens without a pending `"confirm"` toast and without
  blocking.

## Docs to update

- `crates/wolf-ffi/README.md`: bridge methods reaching nvim are bounded by
  `RPC_TIMEOUT` and return their documented fallback on timeout; correct the
  impression that `start_file_search` is entirely non-blocking.
- `crates/wolf-core/src/README.md`: document `block_on_rpc`,
  `RPC_TIMEOUT_DEFAULT`, `WOLF_RPC_TIMEOUT_MS`, `NvimError::Timeout`.
- `runtime/nvim/` README (if present): note the `SwapExists` autocmd.
- Reformat edited Markdown with `prettier --write`.

## Architect's detailed plan (authoritative for implementation)

### (a) RPC timeout — source changes

**nvim.rs** (`crates/wolf-core/src/nvim.rs`): add to `NvimError` enum after the
`Attach(String)` variant (~line 53):
```
#[error("nvim RPC '{0}' timed out after {1:?}")]
Timeout(&'static str, Duration),
```
`Duration` already imported. Verified: all `NvimError::*` uses are constructors,
never exhaustive matches — new variant compiles with no match-arm changes.

**api.rs** (`crates/wolf-core/src/api.rs`):
1. Module-level const near `INPUT_COALESCE_DELAY` (~line 77):
   `const RPC_TIMEOUT_DEFAULT: Duration = Duration::from_secs(5);`
2. Field on `WolfCore` struct: `rpc_timeout: Duration`.
3. In `WolfCore::start` (~line 214, near `cwd`), resolve once:
   ```
   let rpc_timeout = std::env::var("WOLF_RPC_TIMEOUT_MS")
       .ok().and_then(|v| v.trim().parse::<u64>().ok())
       .filter(|&ms| ms > 0)
       .map(Duration::from_millis)
       .unwrap_or(RPC_TIMEOUT_DEFAULT);
   ```
   The `.filter(|&ms| ms > 0)` guard (refinement over the source plan) prevents
   `0`/invalid → instant-timeout `Duration::ZERO`. Add `rpc_timeout,` to the
   `Ok(Self { … })` initializer. Runtime already built `.enable_all()` (~209),
   so the timer works.
4. Add a private module-level free fn `timeout_block_on` (extracted so the
   mechanism is unit-testable WITHOUT a live nvim — the source plan's "unit test
   with a never-resolving future, no live nvim" is only achievable this way):
   ```
   fn timeout_block_on<F, T>(runtime: &Runtime, timeout: Duration,
       what: &'static str, fut: F) -> Result<T, NvimError>
   where F: std::future::Future<Output = Result<T, NvimError>> {
       match runtime.block_on(tokio::time::timeout(timeout, fut)) {
           Ok(inner) => inner,
           Err(_elapsed) => {
               tracing::warn!("nvim RPC '{what}' timed out after {timeout:?}");
               Err(NvimError::Timeout(what, timeout))
           }
       }
   }
   ```
   And the delegating method on `WolfCore`:
   ```
   fn block_on_rpc<F, T>(&self, what: &'static str, fut: F) -> Result<T, NvimError>
   where F: std::future::Future<Output = Result<T, NvimError>> {
       timeout_block_on(&self.runtime, self.rpc_timeout, what, fut)
   }
   ```
5. Reroute the 13 interactive sites: replace `self.runtime.block_on(self.nvim.X())`
   with `self.block_on_rpc("X", self.nvim.X())`, leaving the surrounding
   `.unwrap_or_default()` / `.ok()` / `match` wrapper INTACT. Verified current
   lines (no drift from source table):

   | Line | Method | `what` | Fallback on Err |
   | --- | --- | --- | --- |
   | 377 | copy_visual_selection | `yank_visual_selection` | None |
   | 414 | list_buffers | `list_buffers` | `Vec::new()` |
   | 435-437 | render_preview | `render_preview` | `String::new()` |
   | 471 | open_in_browser | `current_markdown_bufnr` | early `return String::new()` |
   | 483-484 | open_in_browser | `buffer_path` | `.unwrap_or_default()`→`""`, cmd continues (URL root `/`) |
   | 486 | open_in_browser | `current_dir` | `.ok()`→`None` root |
   | 539 | open_preview | `open_preview` | 0 |
   | 565 | edit_source | `edit_source` | 0 |
   | 587 | preview_file | `preview_file` | 0 |
   | 630 | create_tabpage | `create_tabpage` | 0 |
   | 654 | cwd_for_tabpage | `tab_cwd` | None |
   | 672 | current_tabpage | `current_tabpage` | 0 |
   | 947 | resolve_search_root | `current_dir` | `self.cwd` (reproduced crash path) |

   **Do NOT touch**: startup `block_on` (~api.rs:251), shutdown (~api.rs:933,
   already `SHUTDOWN_TIMEOUT`-bounded), `browser_preview.rs` `TcpListener::bind`.
   Steps 1-5 land as ONE unit to keep the crate compiling (avoid transient
   `dead_code` on the helpers — do not add `#[allow(dead_code)]`).

   Note: sites 483-484 and 486 use combinators, not `match` — flagged
   discrepancy with source plan line 50, but both still degrade on `Err` with no
   per-site branch change, so the reroute is mechanical. Sites that already
   `warn!` in their Err arm will log twice on timeout — harmless, leave as-is.

### (b) SwapExists autocmd

**runtime/nvim/init.lua**: immediately after line 25 (`opt.swapfile = false`),
before `opt.shortmess`, add (tab-indented to match the file):
```
local swap_group = vim.api.nvim_create_augroup("WolfSwap", { clear = true })
vim.api.nvim_create_autocmd("SwapExists", {
	group = swap_group,
	pattern = "*",
	callback = function()
		vim.v.swapchoice = "e"
	end,
})
```
With a short comment explaining hermetic "edit anyway" policy.

## Tests (architect's plan — test-writer follows this)

1. **Unit tests** in `#[cfg(test)] mod tests` in `api.rs` (test `timeout_block_on`,
   in-crate, no nvim):
   - `timeout_block_on_times_out_on_never_resolving_future`: current-thread
     runtime `.enable_all()`, `std::future::pending::<Result<(),NvimError>>()`
     under 50ms, assert `Err(NvimError::Timeout("wedged", 50ms))`, returned in
     well under 2s.
   - `timeout_block_on_passes_through_ready_ok`: `future::ready(Ok(7))` under
     generous timeout → `Ok(7)`.
   - `timeout_block_on_passes_through_inner_err`: future → `Err(Rpc("boom"))`
     returned verbatim (not converted to Timeout).
2. **Integration** `crates/wolf-core/tests/rpc_timeout.rs` (real nvim; copy
   harness from `tests/file_search.rs` / `tests/save_open.rs`: `mod common;`,
   `require_nvim_bin`, `config`/`config_in`, `wake_channel`, `start_core_*`,
   `wait_for_initial_flush`, `ch`/`type_str`/`key(Key::Enter)`):
   - Set `std::env::set_var("WOLF_RPC_TIMEOUT_MS", "200")` BEFORE `WolfCore::start`
     (resolved once at start).
   - Wedge FINITE: feed `:` then `type_str("lua vim.uv.sleep(700)")` then Enter.
     Use `vim.uv.sleep` NOT `while true do end` so nvim recovers for teardown.
     Do NOT `wait_for_flush` after wedging.
   - `start_file_search_returns_within_timeout_while_wedged`: call
     `start_file_search` (→ resolve_search_root, site 947) while wedged, assert
     returns `< 2s` (>> 200ms timeout, << ~24s real hang), record elapsed.
   - At least one more: `current_tabpage()`→0 or `list_buffers()`→empty within
     the same bound.
   - Teardown: `core.shutdown().expect(..)` after nvim recovers.
   - Document the coalesce/async race; widen sleep/bound if flaky, don't tighten.
3. **SwapExists** (real nvim, in `save_open.rs` or new `swap_exists.rs`): place a
   stale swap file in the isolated `XDG_STATE_HOME/nvim/swap` (under the core's
   temp `state_root`, see `NvimConfig::from_env` in nvim.rs), open the fixture
   through the core, assert it opens (grid reflects content) without blocking and
   `!core.read_snapshot().toast.is_prompt()`. If a genuine E325 can't be provoked
   deterministically in hermetic `--clean`, the load-bearing assertion is:
   opening with a stale swap present completes and shows no confirm prompt.
   Coordinate with coder on where the isolated swap dir resolves.

No existing test should need changes (FFI discriminant/snapshot/signatures
unchanged; new NvimError variant breaks no match).

## Docs to update (technical-writer)

- `crates/wolf-core/src/README.md`: `block_on_rpc`, `RPC_TIMEOUT_DEFAULT`,
  `WOLF_RPC_TIMEOUT_MS`, `rpc_timeout` field, the 13 guarded methods + the 2 not
  guarded (startup/shutdown), and `NvimError::Timeout` variant.
- `crates/wolf-ffi/README.md`: bridge methods reaching nvim bounded by
  `RPC_TIMEOUT`; correct `start_file_search` (~line 34) — not entirely
  non-blocking, resolves search root via a bounded `getcwd()` RPC.
- `runtime/nvim/README.md`: `SwapExists`/`WolfSwap` autocmd in the hermetic
  bullet (~line 17).
- `prettier --write` each edited Markdown.

## Status

- [x] architect — detailed plan
- [x] test-writer — tests (unit in api.rs mod tests; `tests/rpc_timeout.rs`;
      `tests/swap_exists.rs`). Assumed sigs: `timeout_block_on(runtime:
      &tokio::runtime::Runtime, timeout: Duration, what: &'static str, fut: F) ->
      Result<T, NvimError>` where `F: Future<Output=Result<T,NvimError>>`, called
      as `super::timeout_block_on`; `NvimError::Timeout(&'static str, Duration)`
      with `#[error("nvim RPC '{0}' timed out after {1:?}")]`. Tuple order is
      load-bearing (test asserts `"wedged"`, `Duration::from_millis(50)`). Swap
      test is best-effort provocation (E325 may not fire in hermetic --clean);
      load-bearing assertion = opens + no confirm prompt.
- [x] coder — implement + run tests. All green under **bazel** (real nvim):
      `wolf_core_lib_test` 141 pass (incl. 3 `timeout_block_on_*`),
      `rpc_timeout` pass, `swap_exists` pass, `//crates/wolf-core:all` +
      `//crates/wolf-ffi:all` 54/54, rustfmt + clippy (fail-on-warnings) clean.
      DEVIATION (correctness): `timeout_block_on` builds `tokio::time::timeout`
      INSIDE the `block_on` async block (`async move { timeout(..).await }`) — the
      plan's literal form constructs the timer future outside a runtime context
      and panics "no reactor running"; unit tests caught it. Signature/behavior
      otherwise identical. Added `rpc_timeout`/`swap_exists` rust_test targets to
      `crates/wolf-core/BUILD.bazel`. ORCHESTRATOR DECISION: declined the
      optional `WOLF_RPC_TIMEOUT_MS=0/invalid` fallback test (trivial guard, would
      need a live-nvim start-level test for marginal value; env path already
      exercised with "200"). Note: `tests/swap_exists.rs` (untracked) was
      rustfmt-formatted by the workspace fix run — cosmetic, leaves it gate-clean.
- [x] reviewer — APPROVED WITH NITS. Full plan conformance: all 13 sites
      rerouted with exact labels (api.rs 402,439,460,494,506,509,563,589,611,654,
      678,696,971), no interactive site missed (only startup:275, shutdown:957,
      the helper's own:1019, and browser_preview bind:130 remain untouched — all
      intentional), deviation validated correct, fallback wrappers intact,
      SwapExists idempotent. NITS: (1) stale inline rustdoc in api.rs at ~396,
      432, 451, 475, 550, 576, 602, 644, 666, 690, 964 still says "Blocks briefly
      on the captive nvim (`runtime.block_on`)" — now bounded by rpc_timeout →
      technical-writer. (2) swap test provocation probabilistic (empty swap file
      may not fire E325) — ACCEPTED as-is, no change (guard still catches
      regression when check fires; honestly documented).
- [x] technical-writer — docs. Updated `crates/wolf-core/src/README.md`
      (block_on_rpc/timeout_block_on, rpc_timeout, RPC_TIMEOUT_DEFAULT,
      WOLF_RPC_TIMEOUT_MS, 13 guarded sites + 2 unguarded, NvimError::Timeout),
      `crates/wolf-ffi/README.md` (RPC_TIMEOUT bound + corrected start_file_search),
      `runtime/nvim/README.md` (SwapExists/WolfSwap bullet), and the 11 inline
      api.rs `///` doc-comments (reviewer nit 1 resolved). Orchestrator ran
      `prettier --write` on the 3 READMEs (all unchanged/already clean) and
      verified `//tools/lint:rustfmt_core` PASSES after the doc-comment edits.
      Note: actual startup block_on is api.rs:275, shutdown :957 (early-draft
      plan said 251/933) — both correctly remain unguarded.

## COMPLETE ✅

Both efforts landed and verified. NOT committed (consistent with prior effort's
convention — awaiting user). Effort (c) cmdline-prompt render and the
off-main-thread follow-up remain deferred as separate PRs per the plan.
