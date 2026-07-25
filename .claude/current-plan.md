# Current Plan: eliminate the looped blocking `tab_cwd` RPC (out-of-band per-tabpage cwd)

## The confirmed root cause (from a real repro log)

With `WOLF_LOG=wolf_core=debug`, wedging nvim (`:lua vim.uv.sleep(30000)`) and
spamming ⌘O produced this after nvim wedged:

```
WARN current_dir timed out after 5s → source="cwd_fallback"   ← picker getcwd, now BOUNDED ✓
WARN tab_cwd timed out after 5s → cwd_for_tabpage(1) failed
WARN tab_cwd timed out after 5s → cwd_for_tabpage(1) failed
WARN tab_cwd timed out after 5s → cwd_for_tabpage(1) failed
WARN tab_cwd timed out after 5s → cwd_for_tabpage(1) failed
```

- The RPC timeout WORKS (picker getcwd bounded at 5.001s, falls back correctly).
- The residual "never recovers" is `cwd_for_tabpage(handle)` → the blocking
  `tab_cwd` RPC (`nvim_exec_lua "return wolf_tab_cwd(...)"`), called REPEATEDLY on
  the main thread (ThreadId 13), each blocking the full 5s. It's re-fired by the
  state-snapshot walk (`snapshotNow`, per tab) + Window-menu rebuild
  (`rebuildWindowMenu`, per window) + `otherWindows`/`controllerForCwd`, all via
  `persistedCwd`. The 5s timeout turned an infinite hang into an infinite SEQUENCE
  of 5s hangs — bounded but unusable.
- Tell: it's `cwd_for_tabpage(1)` = the CURRENT tab, whose cwd is ALREADY
  available out-of-band (cheap) via `live_cwd` / the existing `DirChanged`
  autocmd. The code does a blocking RPC to fetch something it already has.

## Goal (user chose: FULL out-of-band)

Make `cwd_for_tabpage` a cheap, lock-free SNAPSHOT READ with NO nvim RPC — the
same out-of-band pattern already used for the current tab's cwd (`live_cwd`,
`TrackedCwd`) and for `current_file`. Track EVERY tabpage's cwd out-of-band via
nvim autocmds, so both current and non-current tabs are covered. The looped
blocking disappears, and so does the wake-storm that re-fires it. Keep the 5s RPC
timeout as the backstop for anything still synchronous.

## Existing out-of-band cwd machinery to mirror/extend (grounding)

- `runtime/nvim/init.lua`: `DirChanged` autocmd (~289) does
  `vim.rpcnotify(chan, "wolf_cwd", vim.fn.getcwd())` (current tab only). The
  `wolf_tab_cwd(handle)` lua helper (~611) returns `getcwd(-1, n)` — the current
  BLOCKING read to be replaced.
- Core: a `TrackedCwd` cell updated from the `wolf_cwd` notify feeds `live_cwd`
  (cheap). Find where the `wolf_cwd` redraw/notify is handled and how the snapshot
  is structured (`crates/wolf-core/src/redraw.rs` / `state.rs` / `api.rs`).
- `crates/wolf-core/src/api.rs`: `cwd_for_tabpage` (~1006 area / the `tab_cwd`
  block_on_rpc site), `live_cwd`, `resolve_search_root`. `crates/wolf-core/src/
  nvim.rs`: `tab_cwd` (~534).
- FFI/Swift consumers: `crates/wolf-ffi/src/lib.rs` `cwd_for_tabpage` (~1634);
  Swift `WolfWindowController.persistedCwd` (~645), `AppDelegate.snapshotNow`
  (~504-537, per-tab loop), `AppMenu.rebuildWindowMenu` (~264-282, per-window),
  `otherWindows`/`controllerForCwd` (~470/478).

## Design sketch (architect to make concrete)

1. **nvim side**: on `DirChanged` AND tab lifecycle (`TabEnter`/`TabNewEntered`/
   `TabClosed`, and the startup pass), notify core of `(tabpage_handle, cwd)` —
   e.g. `vim.rpcnotify(chan, "wolf_tab_cwd", handle, getcwd(-1, tabnr))` — so core
   has each tab's tab-local cwd without a blocking round-trip. Handle tab close
   (remove from the map).
2. **core side**: maintain a `HashMap<i64 handle, PathBuf>` (per-tabpage cwd) in
   the shared snapshot/tracked state, updated on the notify. `cwd_for_tabpage`
   reads it (no RPC). Decide the neutral fallback when a handle isn't yet known
   (None, matching today's failure fallback). Keep it consistent with how
   `live_cwd`/`TrackedCwd` are done (same locking discipline; watch
   `await_holding_lock` clippy).
3. **Remove** the blocking `tab_cwd` block_on_rpc from `cwd_for_tabpage` (and,
   if now unused, the `nvim.tab_cwd` method + `wolf_tab_cwd` helper — verify no
   other caller first).
4. FFI/Swift `cwd_for_tabpage`/`persistedCwd` signatures should stay the same
   (still return the cwd string) — only the core impl changes from RPC to
   snapshot read, so no Swift change is needed beyond confirming correctness.

## Non-goals
- Don't change the 5s timeout or the SwapExists autocmd.
- Don't refactor other main-thread RPCs unless the log/architect shows they also
  loop (the log shows ONLY current_dir (bounded) and tab_cwd (this fix)).

## Test policy
Real nvim, no mocks for nvim behavior. Must include a real-nvim test that:
- Drives `:tcd`/`:cd` in one or more tabpages and asserts `cwd_for_tabpage`
  reflects the change out-of-band (no RPC), including for a NON-current tab.
- Wedges nvim (finite `vim.uv.sleep`) and asserts `cwd_for_tabpage` returns
  PROMPTLY (does NOT block ~5s) — the regression guard for this exact bug.
See `crates/wolf-core/tests/` (`common/mod.rs`, `file_search.rs`, `rpc_timeout.rs`)
for harness patterns.

## Architect's detailed plan (authoritative)

**Approach: wholesale-replace map** (mirror `wolf_buffers`/`apply_buffers`), NOT
per-handle upsert — auto-prunes closed tabs (TabClosed can't report the closed
handle). Store in `Snapshot`, read on the main thread where `cwd_for_tabpage`
already runs.

### nvim (`runtime/nvim/init.lua`)
- Extend existing `_G.wolf_setup_tabs(chan)` (~638): add an `emit()` closure that
  iterates `nvim_list_tabpages()`, maps each handle→number via
  `nvim_tabpage_get_number(tp)` (pcall), reads `getcwd(-1, n)` (pcall), builds a
  list of `[tabpage_handle, cwd]` pairs, and `vim.rpcnotify(chan, "wolf_tab_cwds",
  list)` (payload shape like `wolf_buffers` ~260).
- Register a 2nd autocmd in the existing `WolfTabs` augroup on `{DirChanged,
  TabNewEntered, TabClosed}` calling `emit()`. (TabEnter not needed.) Add an
  initial `emit()` right after the initial `wolf_tabpage` emit (~647).
- DELETE `_G.wolf_tab_cwd(handle)` (~606-621) — dead after core switch.
- Update the `wolf_setup_tabs` block comment. (Lua, not Markdown — no prettier.)

### core
- `crates/wolf-core/src/render.rs`: add `pub tab_cwds: HashMap<i64, PathBuf>` to
  `Snapshot` (after `markdown_ticks` ~707), doc mirroring `buffers`/`previews`.
  `Snapshot` derives `Default` + uses `..Self::default()`, so no constructor/
  struct-literal change.
- `crates/wolf-core/src/redraw.rs`: add `const TAB_CWDS_EVENT: &str =
  "wolf_tab_cwds";` (~after 131); add a `handle_notify` arm (~173) mirroring the
  `BUFFERS_EVENT` arm → `self.apply_tab_cwds(args).await; (self.wake)();`; add
  `async fn apply_tab_cwds(&self, args)` modeled byte-for-byte on `apply_buffers`
  (453-479): decode each `[tp, dir]` (tp via `ext_handle` ~680, dir via
  `as_str`, skip empty, `canonicalize(dir).unwrap_or(PathBuf::from(dir))`), BUILD
  the map BEFORE taking `write_async().await` (no `await_holding_lock`), assign
  `snap.tab_cwds = map;`, DON'T bump `seq`.
- `crates/wolf-core/src/api.rs`: rewrite `cwd_for_tabpage` (706-714) body to
  `self.read_snapshot().tab_cwds.get(&handle).cloned()`. SIGNATURE UNCHANGED
  (`fn cwd_for_tabpage(&self, handle: i64) -> Option<PathBuf>`). Canonicalization
  now in `apply_tab_cwds` so callers still get canonical paths. Update doc (no
  longer blocks/RPC; cheap snapshot read; `None` until nvim first reports the
  tab). Keep `#[must_use]`. `create_tabpage`/`current_tabpage` block_on_rpc sites
  UNCHANGED (out of scope).
- `crates/wolf-core/src/nvim.rs`: DELETE `NvimProcess::tab_cwd` (526-554) — sole
  caller was api.rs:707 (verified). Fix the `setup_tabs` doc comment (drop
  `wolf_tab_cwd`, mention `wolf_tab_cwds`).

### FFI/Swift — NO code change
FFI `cwd_for_tabpage` (`lib.rs` ~1634) still `None→""`; Swift `persistedCwd`
(~645), `snapshotNow`, `rebuildWindowMenu` all handle `""` gracefully. No
swift-bridge regen. Startup race (handle not yet in map → `None`→`""`) = same
neutral state as the old timeout fallback. (Swift doc-comments calling it
"blocking" at WolfWindowController.swift:641 / AppDelegate.swift:530 →
technical-writer corrects to "snapshot read".)

### Implementation order (avoid compile breaks)
1. render.rs field. 2. redraw.rs const+apply+arm. 3. api.rs rewrite (removes the
nvim.tab_cwd caller). 4. nvim.rs delete tab_cwd + doc. 5. init.lua emit+autocmd,
delete wolf_tab_cwd. 6. tests. 7. docs.

### Lint: no new `#[allow]`; build-before-lock keeps `await_holding_lock` quiet;
deleting `tab_cwd` removes dead code.

## Tests (test-writer)
- **Existing tests already cover correctness + must pass unchanged** (they poll):
  `crates/wolf-core/tests/tabpages.rs::per_tabpage_cwd_including_non_current`
  (~324; asserts current + NON-current tab cwd) and
  `::stable_handle_survives_tabpage_renumber` (~437; asserts closed-tab →
  `None`, which the wholesale-replace pruning delivers). Verify they don't need
  changes.
- **NEW regression guard** `crates/wolf-core/tests/tabpage_cwd.rs` (real nvim,
  one `#[test]`): `attach_and_first_tabpage`→t1; `:tcd <tempdir>`; `poll_until`
  `cwd_for_tabpage(t1)==Some(canon(dir))` (proves out-of-band populate); then
  WEDGE nvim with finite `:lua vim.uv.sleep(1500)` (copy `rpc_timeout.rs:85-90`
  pattern: `ch(':')`, `type_str`, `key(Enter)`, ~200ms settle); time
  `cwd_for_tabpage(t1)` and assert `elapsed < 500ms` (THE regression guard — no
  RPC to block) AND value still correct; `shutdown()`. NO
  `WOLF_RPC_TIMEOUT_MS` needed (no RPC to bound). Optionally also assert a
  NON-current tab's cwd returns promptly while wedged.
- **FFI test robustness**: `crates/wolf-ffi/tests/tabpages.rs:259` reads
  `cwd_for_tabpage(t2)` WITHOUT a poll (works today via on-demand RPC; fragile
  with async arrival) — wrap it in the file's existing `poll(...)` helper.
- Grep `tests/` for other `cwd_for_tabpage`/`tab_cwd` users needing updates.

## Docs (technical-writer)
`runtime/nvim/README.md` (drop `wolf_tab_cwd` ~129, document `wolf_tab_cwds`
autocmd+payload), `crates/wolf-core/src/README.md` (drop `tab_cwd` ~194, add
`Snapshot.tab_cwds`), `crates/wolf-core/README.md` (rewrite `cwd_for_tabpage`
~75: snapshot read not blocking), `crates/wolf-ffi/README.md` (rewrite
`cwd_for_tabpage` ~18), `crates/wolf-core/tests/README.md` (new `tabpage_cwd.rs`),
+ the two Swift doc-comments. `prettier --write` all edited Markdown.

## Status

- [x] architect — detailed plan
- [x] test-writer — created `tests/tabpage_cwd.rs` (regression guard:
      `cwd_for_tabpage_is_out_of_band_and_does_not_block_when_nvim_wedged` —
      populates via out-of-band, wedges nvim, asserts current+non-current tab
      cwd returns <500ms while wedged). Wrapped FFI `tests/tabpages.rs:259`
      read in poll. Coder must match: `cwd_for_tabpage(handle)->Option<PathBuf>`
      returning CANONICAL paths (canonicalize in `apply_tab_cwds`), `None` for
      unknown/closed handle. ORCHESTRATOR follow-up IN PROGRESS: wrapping the
      flagged non-polled t2 read at `tabpages.rs:365` in `poll_until` (async
      arrival makes it race-prone).
- [x] coder — implemented all 5 source sites + added `//crates/wolf-core:
      tabpage_cwd` Bazel target. `NvimProcess::tab_cwd` deletion confirmed
      callerless (remaining `tab_cwd` hits are doc/README only). Handle wire-rep
      matches `ext_handle`/`create_tabpage`. FFI/Swift unchanged. ALL PASS:
      tabpage_cwd, tapages (incl. per_tabpage_cwd_including_non_current +
      stable_handle_survives_tabpage_renumber), wolf_core_lib_test,
      //crates/wolf-core:all 25/25, //crates/wolf-ffi:all 30/30,
      //crates/wolf-macos:Wolf builds. rustfmt+clippy clean, denylist clean.
      Flagged for technical-writer: wolf-core/src/README.md ~182/192/194 +
      wolf-ffi/src/README.md still describe the old blocking tab_cwd.
- [x] reviewer — APPROVED WITH NITS. Confirmed: (a) notify name matches both
      sides char-for-char (`wolf_tab_cwds`), (b) handle reps match (ext_handle ==
      create_tabpage/current_tabpage == nvim_list_tabpages space), (c) main-thread
      blocking RPC GONE (now `read_snapshot().tab_cwds.get().cloned()`).
      apply_tab_cwds build-before-lock clean, wholesale-replace prunes closed
      tabs, no seq bump, no autocmd feedback loop, TabEnter omission sound
      (all cwd mutations fire DirChanged). Regression test pins the bug. NITS: all
      docs → technical-writer (runtime/nvim/README.md:129, wolf-core/src/
      README.md:65/192/194, wolf-core/README.md:75, 2 Swift doc-comments).
- [x] technical-writer — synced runtime/nvim/README.md,
      crates/wolf-core/src/README.md, crates/wolf-core/README.md,
      crates/wolf-ffi/README.md, crates/wolf-core/tests/README.md,
      crates/wolf-macos/{README.md,Sources/README.md}, + the 2 Swift doc-comments
      (WolfWindowController.swift, AppDelegate.swift). All verified against source.
      Orchestrator ran `prettier --write` (all clean) + `//tools/lint:swift_format`
      PASSED (Swift doc-comment edits format-clean).

## COMPLETE ✅

The confirmed "never recovers" hang is fixed at the root: `cwd_for_tabpage` is now
a lock-scoped snapshot read fed out-of-band by the `wolf_tab_cwds` notification —
NO main-thread nvim RPC, so the repeated 5s-blocking `tab_cwd` from the repro log
is eliminated. Regression test (`tabpage_cwd.rs`) wedges nvim and proves prompt
(<500ms) return for current AND non-current tabs. Full suite green
(//crates/wolf-core:all, //crates/wolf-ffi:all), app builds, rustfmt/clippy/swift-
format/denylist clean. NOT committed. Lockfiles from the observability effort's
repin remain part of the uncommitted branch state.

### This branch now carries three landed efforts (all uncommitted):
1. RPC timeout backstop (5s, WOLF_RPC_TIMEOUT_MS) + SwapExists autocmd.
2. Observability: `wolf_core::init_tracing()` (stderr, WOLF_LOG) + hot-path logs.
3. Out-of-band per-tabpage cwd — the actual hang fix.

### User's next step
Rebuild + rerun the reliable repro (`:lua vim.uv.sleep(30000)`, spam ⌘O) — the
serial 5s `tab_cwd` freezes should be gone. `WOLF_LOG=wolf_core=debug` should now
show NO `tab_cwd` timeouts (only the bounded picker `current_dir` if the picker
itself is used while wedged).
