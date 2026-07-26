# Current Plan: resolve the file-search root out-of-band (drop the blocking getcwd from the picker)

## Why (confirmed by repro logs)

After the prior four fixes, the ORIGINAL never-recovers hang is gone. Residual:
the ⌘O picker still fetches the cwd with a BLOCKING getcwd RPC
(`resolve_search_root` → `block_on_rpc("current_dir", self.nvim.current_dir())`,
api.rs:1019). When a window's nvim is wedged, every ⌘O issues a getcwd that
blocks 5s (timeout) and piles requests into nvim's stdin (undrained during the
wedge), so recovery has a tail proportional to the wedge length (5s sleep →
recovers in a few seconds; 30s sleep → ~50s).

Wolf ALREADY tracks nvim's cwd OUT-OF-BAND: `live_cwd()` (api.rs:874) is a cheap
synchronous read of the `tracked_cwd` cell, already CANONICALIZED (in
`apply_cwd`), fed by the `wolf_cwd`/`DirChanged` autocmd (redraw.rs CWD_EVENT).
The picker asks nvim for something Wolf already knows.

## Goal

Make `resolve_search_root` use the out-of-band `live_cwd()` instead of the
blocking getcwd RPC, falling back to the configured `self.cwd` only when
`live_cwd()` is `None` (before nvim first reports). Result: the ⌘O picker becomes
fully independent of a wedged nvim — opens instantly from the last-known cwd, no
5s stall ever, and no getcwd pile-up during a wedge. Keeps the 5s RPC timeout as
the backstop for OTHER synchronous RPCs. Mirrors the tab_cwd out-of-band fix.

## Design (confirmed against code)

`resolve_search_root` (api.rs:1019) becomes:
```
fn resolve_search_root(&self) -> PathBuf {
    if let Some(cwd) = self.live_cwd() {
        tracing::debug!(root = %cwd.display(), source = "live_cwd", "resolved file-search root");
        return cwd;            // live_cwd is already canonicalized (apply_cwd)
    }
    let fallback = std::fs::canonicalize(&self.cwd).unwrap_or_else(|_| self.cwd.clone());
    tracing::debug!(root = %fallback.display(), source = "configured_cwd", "resolved file-search root");
    fallback
}
```
- No `block_on_rpc`, no `nvim.current_dir()` call here → no main-thread RPC on the
  ⌘O path.
- `live_cwd()` = `self.tracked_cwd.read().clone()` (already canonical, None until
  nvim first reports); per-core (per-window) so it's the right window's cwd.
- Keep the doc comment accurate: now out-of-band, no RPC; the tracked cwd follows
  `:cd/:lcd/:tcd` via DirChanged; last-known value used if a change is mid-flight.

## Scope / non-goals
- Do NOT remove `NvimProcess::current_dir` — it's still used by `open_in_browser`
  (verify: api.rs ~509). Leave that site as-is (out of scope; not the repro path).
  Architect may NOTE whether open_in_browser should get the same treatment as a
  future follow-up, but do not implement it here.
- Do NOT change the 5s timeout, SwapExists, tab_cwd, observability, or traversal
  bounding work (all landed).

## Tests (real nvim)
- A real-nvim test: wedge nvim with a finite `:lua vim.uv.sleep(...)` (copy
  rpc_timeout.rs / tabpage_cwd.rs pattern), then call `start_file_search` (→
  resolve_search_root) and assert it returns PROMPTLY (<< the 5s timeout, e.g.
  <500ms) with the correct root from the tracked cwd — proving no getcwd block.
  Ideally first `:cd <tempdir>` and poll `live_cwd()` so the tracked cwd is
  populated, then wedge, then assert the search root matches the tracked cwd
  promptly.
- A test that after `:cd <dir>` (out-of-band), `start_file_search` roots at
  `<dir>` (the tracked cwd is followed).
- Confirm existing `file_search.rs` tests still pass (the healthy-path root is now
  live_cwd instead of getcwd — should be the same value; verify no assertion
  depended on the getcwd RPC specifically). Note `resolve_search_root` no longer
  emits `source="getcwd"`/`"cwd_fallback"` — it emits `source="live_cwd"`/
  `"configured_cwd"`; update any test/doc that keyed on those strings (grep).
- See rpc_timeout.rs, tabpage_cwd.rs, file_search.rs, common/mod.rs for harness.

## Docs
- `crates/wolf-core/src/README.md` + `crates/wolf-core/README.md`: update
  `resolve_search_root`/`start_file_search` to describe the out-of-band cwd
  resolution (no RPC); note the picker is now wedge-independent. prettier.

## Architect's detailed plan (authoritative)

### Source (api.rs) — coder
- Rewrite `resolve_search_root` (api.rs:1019-1041) body: if `self.live_cwd()` is
  `Some(cwd)` → `debug!(root=%cwd.display(), source="live_cwd", ...)`, return cwd
  (NO re-canonicalize — live_cwd is already canonical via `apply_cwd`
  redraw.rs:663). Else `fallback = canonicalize(self.cwd).unwrap_or(self.cwd
  .clone())`, `debug!(source="configured_cwd", ...)`, return. Remove the
  `block_on_rpc`/`nvim.current_dir()` call and the `warn!`. Signature UNCHANGED
  (`fn resolve_search_root(&self) -> PathBuf` — NO `#[must_use]`, it doesn't have
  one today; architect corrected the sketch). Rewrite the doc comment (out-of-band,
  no RPC, follows :cd/:lcd/:tcd via wolf_cwd, last-known if mid-flight, self.cwd
  only before first report, wedge-independent).
- Update stale getcwd-mentioning doc comments in api.rs: `start_file_search`
  (~921-927), `WolfCore.cwd` field (~185-186), `CoreConfig.cwd` (~98-101).
- Do NOT delete `NvimProcess::current_dir` — still used by `open_in_browser`
  (api.rs:522). Do NOT touch open_in_browser (out of scope; note as future
  follow-up only).
- Add the `search_root_out_of_band` rust_test Bazel target (mirror `tabpage_cwd`
  BUILD.bazel:646-668).

### Log-string migration
`source="getcwd"`→`"live_cwd"`, `source="cwd_fallback"` (was warn!)→
`"configured_cwd"` (now debug!). No test/assertion keys on these (verified); only
docs/comments reference them.

### Behavior equivalence
Healthy case: live_cwd == the old getcwd value (both nvim cwd, both canonical).
Only diff: a :cd whose wolf_cwd notify hasn't landed yet → picker uses previous
cwd for that one invocation. Acceptable (last-known is fine for a picker; during
a wedge the old getcwd returned nothing anyway).

## Tests (test-writer)
### NEW `crates/wolf-core/tests/search_root_out_of_band.rs` (real nvim; mirror
`tabpage_cwd.rs` wedge/poll harness; `mod common`):
1. `file_search_root_is_out_of_band_and_prompt_when_wedged`: start core (NO
   WOLF_RPC_TIMEOUT_MS); `:cd <tempdir>` via input pump; `poll_until`
   `core.live_cwd()==Some(canon(tempdir))`; put a sentinel file (e.g.
   `wedged_sentinel.md`) in tempdir; WEDGE nvim (`:lua vim.uv.sleep(1500)` +
   ~200ms settle); time `core.start_file_search(wake)` → assert returns <500ms;
   poll the FileSearch to complete and assert entries CONTAIN `wedged_sentinel.md`
   (proves root = tracked cwd, not stale/fallback). Wait out the sleep,
   `shutdown()`. Copy the widen-never-tighten timing note.
2. `file_search_root_follows_cd_out_of_band` (no wedge): core in dir A (sentinel
   `only_in_a.md`); `:cd <B>` (with `alpha.md`); poll `live_cwd()==canon(B)`;
   `start_file_search` → entries include `alpha.md`, NOT `only_in_a.md`.
Copy a local `poll_until` + `poll_until_complete` from tabpage_cwd.rs/file_search.rs.

### CRITICAL — update existing `file_search.rs` (flaky-race fix)
`file_search_root_follows_nvim_cd` (~551) and `mru_seed_follows_nvim_cd` (~641)
`:cd <B>` then `wait_for_flush` then `start_file_search`. After this change the
root+seed come from `tracked_cwd` (fed by the `wolf_cwd` notify, which fires NO
wake), so `wait_for_flush` (keys on seq) does NOT guarantee the cwd landed → add
`poll_until(core.live_cwd().as_deref()==Some(canon_b.as_path()))` between the
flush and `start_file_search` (add a small local `poll_until` — file_search.rs
has none). `file_search_root_defaults_to_cfg_cwd_without_cd` (~605) needs NO poll
(fallback==cfg.cwd==A either way). Reword `getcwd` prose comments (file_search.rs
~575/601) to "tracked live_cwd()".

### Comment-only update `rpc_timeout.rs`
start_file_search/resolve_search_root is now out-of-band (no RPC) — update the
module docstring (~13-14) + comments (~67/68/92) to say `current_tabpage`
(api.rs:719) is the remaining real RPC-timeout demo; the start_file_search
`<RETURN_BOUND` assertion stays valid (now even faster). No assertion change.

## Docs (technical-writer)
- `crates/wolf-core/src/README.md`: observability para ~364/366/368 — drop
  `resolve_search_root` from the guarded-call-site list, count 13→12, remove the
  "start_file_search covered transitively" claim, rewrite the start_file_search
  root sentence (live_cwd, no RPC), migrate the source= strings.
- `crates/wolf-core/README.md`: ~71 (start_file_search bullet), ~87 (CoreConfig
  cwd) — out-of-band live_cwd, fallback only before first report.
- `crates/wolf-core/tests/README.md`: ~46 (search-root test) + mention the new
  wedge-independence test.
- prettier --write all edited Markdown.

## Status
- [x] architect — detailed plan
- [x] test-writer — NEW `tests/search_root_out_of_band.rs` (2 tests:
      `file_search_root_is_out_of_band_and_prompt_when_wedged`,
      `file_search_root_follows_cd_out_of_band`). FLAKY-RACE FIX applied to BOTH
      `file_search.rs` cd/seed tests (added local `poll_until` +
      `poll_until(live_cwd()==canon_b)` before start_file_search). rpc_timeout.rs
      comments updated (no assertion change). Coder must: rewrite
      resolve_search_root to read live_cwd() (read guard exposes entries/complete;
      live_cwd() -> Option<PathBuf> already canonical), add the
      `search_root_out_of_band` Bazel target (mirror tabpage_cwd:646-668).
- [x] coder — rewrote resolve_search_root (live_cwd, no RPC) + stale doc
      comments + `search_root_out_of_band` Bazel target. ALL PASS:
      search_root_out_of_band 2/2, file_search (with poll guards), rpc_timeout,
      tabpage_cwd, //crates/wolf-ffi:all, //crates/wolf-macos:Wolf builds.
      rustfmt+clippy+denylist clean. (copy_paste flaked under parallelism, passed
      in isolation — pre-existing, unrelated.) NvimProcess::current_dir kept
      (open_in_browser still uses it). FOLLOW-UP IN PROGRESS: test-writer wiring
      `canon()` into the live_cwd equality polls (unused-canon warning; macOS
      symlinked tempdirs need canonical comparison) → coder re-verify after.
      (test-writer follow-up DONE: removed unused `canon`; temp_cwd() already
      canonicalizes so live_cwd polls are canonical-vs-canonical, macOS symlink-
      safe; warning cleared, dead-code removal can't affect the green run.)
- [x] reviewer — APPROVED. Confirmed: (a) no RPC in resolve_search_root; (b)
      Some branch doesn't re-canonicalize (live_cwd already canonical); (c) both
      file_search.rs cd/seed tests got the poll_until(live_cwd()==canon_b) guard
      (canonical-vs-canonical, macOS-safe); (d) start_file_search issues ZERO
      nvim RPCs on the resolve path. current_dir kept (open_in_browser). Normal-
      search equivalent; behavioral delta (mid-flight :cd) benign. NITS → tech-
      writer: README.md:71, src/README.md:364/368 still describe getcwd (keep
      src/README.md:186 current_dir entry — still valid for open_in_browser).
- [x] technical-writer — updated wolf-core/README.md, wolf-core/src/README.md
      (guarded-site count 13→12, source= strings, kept current_dir entry),
      wolf-core/tests/README.md, wolf-ffi/README.md, wolf-macos/README.md,
      architecture.md, build.md. Left rpc-timeout-plan.md +
      completions-decision-report.md as historical artifacts (correct). Orch ran
      prettier (all clean); no api.rs `///` touched so rustfmt unaffected.

## COMPLETE ✅

The ⌘O picker no longer issues ANY blocking nvim RPC on its resolve path:
`resolve_search_root` reads the out-of-band `live_cwd()` (tracked cwd), so the
picker opens instantly and is fully wedge-independent — removing the getcwd
pile-up that caused the recovery tail. Regression test wedges nvim and proves
`start_file_search` returns <500ms with the correct tracked root. Full suite
green, app builds, rustfmt/clippy/denylist clean. NOT committed.

### This branch now carries FIVE landed efforts (all uncommitted):
1. RPC timeout backstop (5s, WOLF_RPC_TIMEOUT_MS) + SwapExists autocmd.
2. Observability: init_tracing() (stderr, WOLF_LOG) + hot-path logs.
3. Out-of-band per-tabpage cwd (removed the looped blocking tab_cwd RPC).
4. Bounded file-search traversal (killed the $HOME-walk I/O storm).
5. Out-of-band search-root resolution (removed the last blocking getcwd from ⌘O).
Lockfiles from effort #2's repin are part of the uncommitted state.
