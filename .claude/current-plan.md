# Bug fix: sidebar file tree does not reflect writes, renames, or deletes

## Problem statement

On macOS, the sidebar file tree never updates after the initial traversal:

- `:e new-file.md`, insert text, `:w` — `new-file.md` never appears in the sidebar.
- Renames (`:saveas`, anything firing `BufFilePost`) never appear.
- Deletes never disappear.

## Root cause (from initial investigation)

- The tree sidebar is driven by a long-lived `FfiFileSearch` session:
  `FileTreeView.swift` → `wolf_ffi` `start_tree_search` (`crates/wolf-ffi/src/lib.rs:2323`) → `WolfCore::start_file_search` (`crates/wolf-core/src/api.rs:1307`).
- That session's traversal is one-shot: a synchronous MRU seed, then one background `ignore`-crate walk (`crates/wolf-core/src/files.rs`) that streams batches and ends `complete`. Nothing ever re-walks.
- Neovim already notifies core when files are written/renamed/deleted: `runtime/nvim/init.lua` registers `BufWritePost` / `BufFilePost` / `BufDelete` autocmds feeding the `wolf_open_files` and `wolf_buffers` notifications, folded in `crates/wolf-core/src/redraw.rs` (`apply_open_files` / `apply_buffers`). But nothing routes those events into a re-traversal of any live file-search session.
- The macOS picker (`PickerView.swift`) opens a fresh session per open, so it self-heals; the tree session lives as long as the sidebar and goes permanently stale.

## Requirements

- After a write of a new file, a rename, or a delete performed through the captive Neovim, the sidebar tree must update to reflect the filesystem.
- The tree view's expansion state must be preserved across the update (`FileTreeView` keys expansion on cached `TreeItem` identity per relPath).
- Coalesced-refresh behavior must be preserved (streaming batches collapse to ~one reload per frame; refresh work deferred while the sidebar is collapsed).
- Tests: prefer end-to-end/integration tests that start a real Neovim process (per CLAUDE.md). New Rust test files must be wired into `BUILD.bazel` `rust_test` targets (Bazel does not auto-discover), mirroring a sibling real-nvim test's runfiles/env/tags.

## Plan (architect)

### Chosen design

Core-driven rescan (option 1). A new dedicated Neovim notification `wolf_fs_changed` (autocmds `BufWritePost` / `BufFilePost` / `BufDelete` in `runtime/nvim/init.lua`) is routed by `RedrawHandler` into a new session registry inside `wolf-core`, which debounces and re-walks every live `FileSearch` session, atomically swapping the entry list and bumping `version` so the existing wake → `scheduleRefresh()` → `TreeIndexCache` pipeline redraws the tree. The fix lives entirely below `start_file_search`, so both front-ends benefit with zero Swift and zero GTK changes.

Rejected: `notify` fs-watcher (new dep, Bazel lockfile churn, watcher-thread complexity — external-change support is out of scope, and the registry is the seam a watcher could later plug into); front-end-driven restart (implemented twice, no reliable front-end signal, discards streaming/MRU continuity).

A dedicated event (not piggybacking `wolf_open_files`) is required because `wolf_open_files` also fires on `BufEnter`/`WinEnter`/`TabEnter`, which would trigger walks on every focus change.

### Key mechanics

- Trigger payload `[event_name, abs_path]`. `BufWritePost` is conditional (rescan only if path not already in the session's `seen` set and under the session root — a `:w` of an existing file costs nothing); `BufFilePost` / `BufDelete` are unconditional (`force`).
- Rescan = walk-then-swap, not streaming: collect into a local `Vec` (re-seeded from `MruStore::recent(&root)`, which prunes rows for deleted files), then under one short write lock replace `entries`/`seen`/`truncated`, bump `version`, keep `complete = true`; fire the session wake once after unlock. If the new list equals the old, skip swap and version bump (no UI churn on `:bdelete` of a still-existing file).
- Debounce: registry-level, default 200 ms (`WOLF_RESCAN_DEBOUNCE_MS`, `> 0` guard). Per-session `rescan_running` + `rescan_dirty` atomics prevent overlapping walks; a request during a walk re-runs once at the end.
- If the initial traversal has not yet completed, the rescan waits in a bounded poll loop (~25 ms cadence, bounded by `bounds.budget`) so a swap never races `append_batch`.
- Sessions register in the registry from `start_file_search`; an entry whose `cancel` flag is set (Drop sets it) is dead and pruned on every notify. Picker sessions register too but prune themselves.
- `needs_rescan(force, paths, root, seen) -> bool` is a pure, headlessly unit-testable helper; canonicalization mismatches fail open to an extra walk.

### Files to modify

1. `crates/wolf-core/src/files.rs` — `SearchBounds: Clone`; extract shared `walk_files` helper from `spawn_traversal` (one set of traversal rules for initial walk and rescan); add `SearchRegistry` + `SessionEntry` + `needs_rescan` + `rescan`; headless unit tests in `#[cfg(test)]`.
2. `crates/wolf-core/src/nvim.rs` — `setup_fs_events()`, structural mirror of `setup_open_files` (line 1009).
3. `crates/wolf-core/src/redraw.rs` — `FS_CHANGED_EVENT` const; `RedrawHandler` gains `search_registry` field + `new` param; new `handle_notify` arm: decode, canonicalize with raw fallback (canonicalize fails for deleted paths), call `notify_fs_changed(path, force)` with `force = event != "BufWritePost"`. No snapshot lock, no `seq` bump, no handler-level wake.
4. `crates/wolf-core/src/api.rs` — `search_registry: Arc<SearchRegistry>` field created in `start()` (handle + debounce from env) before `RedrawHandler::new` (line 277); registration in `start_file_search` (line 1307); `setup_fs_events().await?` in both the start chain (~304–320) and `restart_nvim_inner` (~1512–1527).
5. `runtime/nvim/init.lua` — `_G.wolf_setup_fs_events(chan)` next to `wolf_setup_open_files`: augroup `WolfFsEvents`, one autocmd for the three events, emits `vim.rpcnotify(chan, "wolf_fs_changed", { args.event, vim.fn.fnamemodify(args.file, ":p") })`; pcall-guarded, skips non-file buffers (`buftype ~= ""`, pcall since `args.buf` can be invalid on `BufDelete`).

No changes: wolf-ffi, FileTreeView.swift, TreeWakeBridge.swift, Project.swift, wolf-gnome (verify only).

### Test plan

- New `crates/wolf-core/tests/file_search_rescan.rs` (modeled on `tests/file_search.rs`, real nvim via `tests/common/mod.rs`):
  1. New file appears after `:e fresh.md` / insert / `:w`; version advanced; still `complete`; no dupes.
  2. Deleted file disappears (`remove_file` + `:bdelete!`); MRU seed does not resurrect it.
  3. `:saveas renamed.md` lists new name; old name still listed (old file really remains on disk — pin semantic in comment).
  4. `:w` of an existing listed file does not bump version (sleep past debounce, assert unchanged).
  5. Burst of three new files: all appear, deduped (no strict rescan-count assertion).
- Headless `needs_rescan` unit tests in `src/files.rs` `#[cfg(test)]` (existing lib test target, no new wiring).
- New `crates/wolf-ffi/tests/tree_search_rescan.rs` (modeled on `tests/tree_search.rs`): end-to-end `start_tree_search()` → baseline `children_of("")` → drive `:e`/`:w` via `send_key` → poll until `fresh.md` node appears and `tree.version()` advanced; then delete flow; assert dirs-before-files ordering holds.
- Bazel wiring (mandatory): new `rust_test` targets `//crates/wolf-core:file_search_rescan` (mirror `file_search` target verbatim) and `//crates/wolf-ffi:tree_search_rescan` (mirror `tree_search` target, line 204). Confirm both run under `bazelisk test`.

### Documentation impact

- `crates/wolf-core/src/README.md`: `files.rs`, `redraw.rs`, `nvim.rs` entries (edit in place); `api.rs` only if it enumerates chains.
- `crates/wolf-core/DECISIONS.md`: one decision entry (rescan-on-nvim-events vs watcher; walk-then-swap; cancel flag as registry liveness).
- `runtime/nvim/README.md`: `wolf_setup_fs_events` / `wolf_fs_changed` entry.
- `crates/wolf-core/tests/README.md` + `crates/wolf-ffi/tests/README.md`: one entry each.
- `architecture.md`: only if the `wolf_*` notification list is exhaustive.

### Lint notes

- `RedrawHandler::new` grows to 8 params — targeted `#[allow(clippy::too_many_arguments)]` with one-line reason.
- Build replacement vec before taking the write lock; never hold state lock across the wake; bind guards to locals.

### Risks / edge cases

Rename window shows old name until swap; `:saveas` correctly leaves both; `:bdelete` of existing file → equality check suppresses churn; external changes remain invisible (documented limitation; registry is the future watcher seam); N views ⇒ N debounced bounded walks; sidebar collapsed → `TreeRefreshGate` defers as today; nvim restart re-arms via `setup_fs_events` in restart chain; wedged nvim → no events, no rescans, no RPC on rescan path; truncated flag replaced truthfully on swap.

### Implementation order

files.rs → nvim.rs → redraw.rs → api.rs → init.lua → tests + Bazel → docs (`prettier --write`, `./tools/lint/markdown_check.sh`).

## Stage 1 addendum: non-buffer-event triggers (user feedback 2026-08-09)

User verification found the fix partial: new-file writes update the sidebar, but renames/deletes performed as `git mv` in an external terminal or in the Wolf terminal drawer do not — those flows fire no `BufWritePost`/`BufFilePost`/`BufDelete`. User decision: staged fix. Stage 1 (now): broaden the nvim-side trigger set — `ShellCmdPost` (catches `:!mv`/`:!rm`/`:!git mv`), `FocusGained` (catches external-terminal/Finder changes when Wolf regains focus; the app already pushes both UI focus edges to nvim), `TermLeave` (catches drawer-terminal changes when focus leaves the drawer). All are force triggers; the 200 ms debounce plus the equality-check no-op suppression bound the cost (a rescan walk costs the same as one picker open). Stage 2 (follow-up): a filesystem watcher plugged into the `SearchRegistry` seam for immediate updates without a refocus edge.

### Stage 1 spec (architect)

- Adopt all three as force triggers. `ShellCmdPost` (fires on `:!…` incl. `:silent !…`); `FocusGained` (delivery chain verified end to end: `AppDelegate.pushUiFocusToAllProjects` → `set_ui_focus` → `nvim_ui_set_focus` → autocmd; already load-bearing for `WolfAutoRead :checktime`; GNOME doesn't forward focus — pre-existing asymmetry, document only); `TermLeave` unguarded (drawer auto-`startinsert`s on WinEnter per `init.lua:886–897`, so every drawer-exit path passes a t→n mode change). No `TermClose`, no `WinLeave`. Contingency: if the real-drawer test shows `TermLeave` not firing, replace with `WinLeave` + `buftype == "terminal"` guard — do not stack both.
- `init.lua` only: second `nvim_create_autocmd` in `WolfFsEvents` for the three events with its own callback (the existing `buftype ~= ""` guard would swallow `TermLeave`); emit `pcall(vim.rpcnotify, chan, "wolf_fs_changed", { args.event, "" })` — explicit empty path (not `fnamemodify`, which fabricates the cwd from ""). The empty string is required: omitting the element would hit the pair-decode guard and drop the event.
- Zero Rust logic changes: `force = event != "BufWritePost"` already covers the new events; canonicalize-fallback handles `""`; `needs_rescan` short-circuits on force. Only comment touch-ups in `redraw.rs` (`FS_CHANGED_EVENT` doc + arm).
- Cost: human-paced edges; 200 ms debounce collapses each (incl. autoread-induced writes riding the same FocusGained); per-fire cost = one bounded walk per session, same as a picker open; equality check keeps unchanged trees at zero UI cost. No cooldown gate.
- Tests: extend `crates/wolf-core/tests/file_search_rescan.rs` (no new files, no Bazel changes): `shell_cmd_post_rescans` (`:silent !rm <abs>`), `focus_gained_rescans` (`set_ui_focus(false)` then `(true)` mirroring `tests/autoread.rs` — nvim de-dupes, both edges required), `term_leave_rescans` (real drawer flow via `InputEvent::ToggleTerminal` twice, mirroring `wolf-ffi/tests/terminal.rs`; `doautocmd` only as dev diagnostic, never the committed assertion).
- Docs: `runtime/nvim/README.md` entry extended in place (two trigger classes + GNOME caveat link); `crates/wolf-core/src/README.md` `files.rs`/`redraw.rs` entries compressed under budget in the same edit (link to module docs, don't transcribe); DECISIONS.md rescan entry extended (not a second entry) with the TermLeave-not-WinLeave reason and the Stage 2 watcher note; tests README one sentence; `architecture.md:115` wake-claim scoped.

## Status

- [x] Problem statement written
- [x] Architect plan
- [x] Tests written — `crates/wolf-core/tests/file_search_rescan.rs` (5 tests), `crates/wolf-ffi/tests/tree_search_rescan.rs` (1 e2e test), `rescan_decision_tests` module appended to `crates/wolf-core/src/files.rs` (6 headless tests pinning `needs_rescan(force: bool, paths: &[PathBuf], root: &Path, seen: &HashSet<PathBuf>) -> bool`), Bazel targets `//crates/wolf-core:file_search_rescan` and `//crates/wolf-ffi:tree_search_rescan` added. Note: lib tests will not compile until the coder implements `needs_rescan` with the pinned signature.
- [x] Implementation + tests passing — all targets green on macOS/Bazel: new `//crates/wolf-core:file_search_rescan` (5) + `//crates/wolf-ffi:tree_search_rescan` (1); regressions `file_search`, `file_search_bounds`, `tree_search`, `wolf_core_lib_test` (230 incl. 6 `rescan_decision_tests` + `append_batch_seen_tests`); full `//crates/wolf-core/... //crates/wolf-ffi/...` 85/85; `//tools/lint:check` all 5. Deviation from plan (documented in code): `append_batch` now folds streamed paths into `FileSearchState::seen` so `needs_rescan` knows walk-discovered files. Linux `cargo test` not exercised (no Cargo toolchain on this host); change touches no GTK code.
- [x] Review — verdict: implementation correct and plan-conformant. Three fixes applied: (1) `deleted_file_disappears` now opens via `InputEvent::Open` + MRU-record wait so the MRU-resurrect guard is genuinely exercised; (2) stale "expected to FAIL" comment blocks removed from both new test files; (3) `SearchRegistry::register` prunes dead sessions so browse-only sessions don't accumulate dead picker entries. All targets re-run green (85/85 + lint). Known pre-existing flake noted: `file_search::mru_seed_follows_nvim_cd` (shared `WOLF_STATE_DIR` sqlite contention across concurrent tests; passes 3/3 rerun; out of scope). Reviewer doc note: `architecture.md:115` "out-of-band `wolf_*` notifications fire the same wake" is now over-general — `wolf_fs_changed` fires no handler wake (session wakes fire on swap).
- [x] Documentation — all planned docs updated across both passes; over-budget entries compressed (`files.rs` 376→~214 words, `redraw.rs` 297→~214); long-line finding split; `prettier` clean and `//tools/lint:markdown` passes with zero findings.
- [x] Stage 1 (refocus-edge triggers) — implemented (`init.lua` second autocmd for `ShellCmdPost`/`FocusGained`/`TermLeave`, comment-only `redraw.rs` touch-up), 3 new real-nvim tests green (TermLeave fired on the real drawer flow; WinLeave contingency not needed), reviewer approved with no issues, docs updated.
- [ ] Stage 2 (filesystem watcher on the `SearchRegistry` seam) — agreed with user as follow-up; supersedes the refocus-edge dependency and per-refocus walk cost with targeted invalidation. Requires the `notify` crate (Cargo + Bazel crate-universe repin) and watcher lifecycle per session root.
- Note: Linux `cargo test` path unexercised on this macOS host (no Cargo toolchain); change touches no GTK code. Pre-existing flake noted in `file_search::mru_seed_follows_nvim_cd` (shared `WOLF_STATE_DIR` sqlite contention; out of scope).
