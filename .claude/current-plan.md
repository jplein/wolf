# Plan: Name anonymous buffers `Untitled.md` and make preview work

## Problem statement

When first opening a folder or the app, or after pressing Command-N, anonymous
files are named `[No Name]`, and preview silently fails.

Three changes are wanted:

1. **Naming.** An anonymous/new buffer must be named `Untitled.md`, `Untitled 2.md`
   (note: "1" is skipped — `Untitled.md` is implicitly 1), `Untitled 3.md`, and so
   on. It is always treated as a *new* file: if a saved file OR an existing buffer
   already uses `Untitled.md`, try `Untitled 2.md`, and so on until a free name is
   found.

2. **Preview works.** Preview must successfully render the contents of the
   (now-named) anonymous buffer, instead of silently failing.

3. **No auto-save.** Do NOT automatically save an empty file to disk. The
   `Untitled N.md` buffer is a new, unsaved document until the user explicitly
   saves it.

## Known context (from initial orchestrator investigation)

- The in-app preview pane is created by `wolf_open_preview(mode)` in
  `runtime/nvim/init.lua` (~line 1241). It returns `0` (no-op) unless
  `vim.bo.filetype == "markdown"`. A `[No Name]` buffer has no filetype, so preview
  silently fails. Naming the buffer `Untitled.md` should make filetype detection
  yield `markdown` (may require an explicit `filetype detect` / setting after
  naming, since `nvim_buf_set_name` alone does not re-run filetype detection).
- New empty buffers come from `InputEvent::NewBuffer` → `:enew`
  (`crates/wolf-core/src/input.rs:433`), reached from FFI `new_buffer()`
  (`crates/wolf-ffi/src/lib.rs`) and the macOS `WolfWindowController.newBuffer()`
  (Cmd-N within a window / new-buffer action). Initial window launch with no folder
  shows an empty-state buffer too (`EmptyStateView.swift`, `WolfWindowController`).
- The browser-preview HTTP server (`crates/wolf-core/src/browser_preview.rs`)
  already renders unnamed buffers live via `render_pinned_live`; the primary
  failing path is the in-app preview pane's filetype gate.
- Tests: prefer end-to-end/integration tests that start a real Neovim (see
  CLAUDE.md and existing `crates/wolf-ffi/tests/*.rs`, `crates/wolf-core/tests`).
  Build/test with bazelisk, not cargo; use the bundled pinned nvim.

## Plan

### Summary

Give every anonymous editable buffer a real, unsaved name — `Untitled.md`, then
`Untitled 2.md`, `Untitled 3.md`, … (number `1` skipped; `Untitled.md` is
implicitly 1) — choosing the first name colliding with neither an existing Neovim
buffer nor a file on disk in cwd. Naming with `.md` + explicitly setting
`filetype = markdown` makes the in-app preview gate pass. Buffer stays a normal,
modifiable, UNSAVED document (no auto-write). Naming logic lives in Neovim-side Lua
helpers wired into the entry points.

### Naming algorithm — `_G.wolf_next_untitled(cwd, exclude_buf) -> (name, abspath)`

Loop `i = 1, 2, 3, …`:
- `name = (i == 1) and "Untitled.md" or ("Untitled " .. i .. ".md")`
  (index 1 → un-numbered `Untitled.md`; literal `Untitled 1.md` is NEVER generated).
- `abspath = cwd .. "/" .. name` (cwd = `vim.fn.getcwd()`).
- **Disk collision:** `vim.uv.fs_stat(abspath) ~= nil` (file OR dir).
- **Buffer collision:** iterate `vim.api.nvim_list_bufs()` (listed + unlisted +
  loaded + unloaded — all can trigger E95); for each `b ~= exclude_buf` valid,
  `nvim_buf_get_name(b) == abspath` ⇒ collision.
- First index with neither collision wins. Defensive upper bound (~10000) with a
  UUID-ish fallback, never expected to hit.
- Deliberately does NOT inspect `.swp` swap files — only the `.md` file itself.

### Files to modify

**`runtime/nvim/init.lua`** (primary) — add near preview helpers (~line 1180):
1. `_G.wolf_next_untitled(cwd, exclude_buf)` — pure name generator (above).
2. `_G.wolf_name_untitled(buf)` — compute name for `buf`; `pcall(nvim_buf_set_name,
   buf, abspath)` (retry next index on E95 race, bail gracefully rather than raise);
   `vim.bo[buf].filetype = "markdown"`. Do NOT write; do NOT touch
   `buftype`/`bufhidden`/`swapfile` — leave normal, modifiable, `:w`-able. Fires
   `BufFilePost` + `FileType`, so existing current-file / per-window trackers
   (init.lua:306, :343) re-emit `Untitled.md` automatically (titlebar/tabs/picker
   update, no extra wiring).
3. `_G.wolf_new_untitled()` — `vim.cmd("enew")` then `wolf_name_untitled(current buf)`.
   Replaces raw `:enew` for `NewBuffer`.
4. `_G.wolf_name_untitled_current()` — GUARDED re-home of the current buffer: proceed
   only if `buftype == ""`, `modified == false`, and name is empty OR (basename matches
   `^Untitled( %d+)?%.md$` AND `vim.uv.fs_stat(name) == nil`). Then
   `wolf_name_untitled(current buf)`.
5. **`VimEnter` autocmd** (new augroup `WolfUntitled`): if exactly one listed buffer
   and the current buffer passes the guard, name it. Covers first-launch and
   launch-on-folder (getcwd() is the launch cwd at VimEnter). Guard keeps it off
   crash-recovery-restored buffers.

**`crates/wolf-core/src/input.rs`**:
- `NewBuffer` arm (line 433): `command(nvim, "enew")` → `nvim_exec_lua("wolf_new_untitled()", [])`
  (add `async fn new_untitled(nvim)` mirroring toggle_terminal/toggle_full_width shape
  at input.rs:550-587).
- `Chdir` arm (line 429): after `tcd <fnameescape(path)>`, run
  `nvim_exec_lua("wolf_name_untitled_current()", [])` so opening a folder from empty
  state produces `Untitled.md` IN the opened folder. Guard makes it a no-op for real
  files / general chdirs.
- Update `NewBuffer` variant doc (line 229): `:enew` → `wolf_new_untitled` helper.
- Keep `Ref`/`RefCell` borrows out of `.await` (api.rs:150 convention).

**`crates/wolf-ffi/src/lib.rs`**: doc-comment only (516-518, 2037-2044) — describe
`Untitled N.md` naming via `wolf_new_untitled` instead of `:enew`/`[No Name]`.

**No macOS Swift code change** — open-folder is `:cd` via `Chdir`; Cmd-N via existing
`new_buffer()` FFI. (Fallback if Chdir re-home is undesirable: call `new_buffer()` from
`WolfWindowController.didOpenFolder()` — note but PREFER the Chdir approach.)

### Documentation (source of truth per CLAUDE.md; `prettier --write` after)

- `runtime/nvim/README.md` — document the four helpers + `VimEnter` autocmd, naming
  sequence, collision rules, no-write guarantee, filetype-set step.
- `crates/wolf-core/README.md` (lines 118, 128) & `crates/wolf-core/src/README.md`
  (lines 111, 150) — `NewBuffer` `:enew` → Untitled naming; `Chdir` re-homes untouched
  anonymous buffer.
- `crates/wolf-ffi/README.md` (line 51) — `new_buffer` Untitled description.
- `crates/wolf-macos/README.md` (line 139) — match.

### Edge cases

- Skip "1": idx1 `Untitled.md`, idx2 `Untitled 2.md`; never `Untitled 1.md`.
- Buffer collision: two Cmd-N ⇒ `Untitled.md` then `Untitled 2.md`.
- Disk collision: real `Untitled.md` (file or dir) in cwd forces `Untitled 2.md`.
- E95 race: `pcall` + retry next index; never surface as RPC error.
- No auto-write: naming (name + filetype) never writes; verify no `:w`/`:update`/
  `:saveas` fires. Temp cwd must have no `Untitled*.md` on disk after `new_buffer`.
- Swapfile/crash recovery: normal buffer (swap on first modify, desirable). Naming
  does not read disk ⇒ no SwapExists prompt at naming time. Existing swap-exists
  machinery (see MEMORY swap_exists / confirm-dialog-two-surfaces) handles later `:w`.
- Guard protects modified buffers, non-normal buftypes (terminal/preview/help),
  already-saved Untitled names, preview scratch (`b:wolf_preview_source`), and
  crash-recovery-restored buffers.
- `browser_preview.rs` unchanged (already handles named/unnamed buffers).

### Implementation order

1. init.lua helpers + VimEnter autocmd. 2. input.rs rewire NewBuffer + extend Chdir.
3. lib.rs doc updates. 4. READMEs + prettier. 5. Tests.

### Test plan (real Neovim, bazelisk, bundled pinned nvim; loud-skip via
`common::nvim_bin_or_skip`; basename/`ends_with` asserts for `/var`↔`/private/var`)

**`crates/wolf-ffi/tests/file_actions.rs` (modify):** the existing test (164-180)
asserts `current_file()` becomes empty after `new_buffer()` — now inverted. Update to
assert it ends with `Untitled.md`; update header doc (7-9, 24).

**`crates/wolf-ffi/tests/preview.rs` (add) `preview_renders_untitled_buffer`:** fresh
temp cwd (no md files); after attach assert `is_markdown_active()` true and
`current_file()` ends with `Untitled.md`; type a heading; `open_preview("split_right")`
returns bufnr > 0 (was 0); `render_preview(source_bufnr)` contains the heading.

**`crates/wolf-ffi/tests/untitled.rs` (new; add `rust_test` target to
`crates/wolf-ffi/BUILD.bazel` copying the `:file_actions` stanza — srcs
`["tests/common/mod.rs", "tests/untitled.rs"]`, `crate_root = "tests/untitled.rs"`):**
1. Startup naming: after attach `current_file()` ends `Untitled.md`,
   `is_markdown_active()` true, NO `Untitled.md` on disk (no auto-write).
2. Cmd-N + buffer collision: `new_buffer()` ⇒ `Untitled 2.md`; again ⇒ `Untitled 3.md`.
3. On-disk collision: seed `Untitled.md` file before start ⇒ startup buffer is
   `Untitled 2.md`.
4. Skip "1": first name is exactly `Untitled.md`, never `Untitled 1.md`.
5. No file written: after `new_buffer()` neither `Untitled.md`/`Untitled 2.md` exist on
   disk. (Optional: send `:w`, poll until file appears — proves explicit save works.)
6. Open-folder re-home: start in temp dir A, `chdir(B)` (empty); poll until
   `current_file()` ends `Untitled.md` with parent B; no file written in B.
7. Guard: open a real markdown file, `chdir` elsewhere ⇒ `current_file()` still that
   real file (guard prevents re-home).

### Confirmed no-change

`crates/wolf-core/src/browser_preview.rs`, `EmptyStateView.swift`,
`WolfWindowController.swift` (open-folder handled via `Chdir`).

---

## Post-implementation status & follow-up (round 2)

Coder implemented the feature; all 7 new `untitled.rs` tests PASS. `:enew`
prior-buffer-listing HELD (Cmd-N sequence Untitled.md → 2 → 3 works). 8 pre-existing
targets fail — analyzed, all consequences of the feature working as specified. NO test
files were edited by the coder. Fixes below.

### Category A — sync-barrier races (TEST-WRITER fix)

`preview` (4 pre-existing), `edit` (5), `full_width` (1), `code_block_highlight` (1),
and the new `preview_renders_untitled_buffer` fail with "timed out waiting for
previews()". Root cause: they send `:e <file>` on the async key channel then call the
SYNC `open_preview`/`render_preview`, relying on `wait_for_markdown_active(true)` as an
implicit barrier — which only worked because the startup buffer used to be
non-markdown. Now the startup buffer is `Untitled.md` (markdown), so
`wait_for_markdown_active(true)` returns immediately and the RPC races ahead of the
buffered `:e`. FIX: before each `open_preview`/`edit_source`, add a REAL switch barrier
that witnesses the active-buffer change, e.g. `wait_for_current_file(handle,
"<basename>")` (helper already exists in preview.rs). Spots: preview.rs picker/latex/
reuse/repoints tests; edit.rs ~220; full_width.rs ~247; code_block_highlight.rs ~179.
Also `preview_renders_untitled_buffer`: after typing the heading, POLL
`render_preview(source_bufnr)` until it contains the heading (bounded), not a one-shot
assert.

### Category B — fixtures/expectations changed by the feature (TEST-WRITER fix)

- `crates/wolf-core/tests/pane_buffers.rs::unnamed_buffer_reports_no_name` — asserted
  the startup buffer is `[No Name]`; it is now `Untitled.md`. Drive a genuinely UNNAMED
  buffer explicitly instead (e.g. create a raw unnamed buffer / `:enew` without naming
  — NOT via `wolf_new_untitled`), preserving coverage of the `[No Name]` path.
- `crates/wolf-core/tests/browser_preview.rs::non_markdown_buffer_returns_empty_url` —
  asserted the bare scratch buffer is not markdown; it is now. Drive a genuinely
  NON-markdown buffer (open a real `.txt`, or `:enew`+`setf text`) to keep coverage.
- `crates/wolf-ffi/tests/file_actions.rs::file_actions_chdir_and_new_buffer` — after
  `chdir` re-homes the startup buffer to `<target>/Untitled.md` and `open(new_me.md)`,
  the `new_buffer()` collision walk correctly yields `Untitled 2.md`. Update the
  expectation from `Untitled.md` to `Untitled 2.md`; best made robust by asserting the
  basename matches `^Untitled( \d+)?\.md$`.

### Design ruling — crash-recovery interaction (architect ruled OPTION (a))

`crates/wolf-core/tests/nvim_restart.rs::restart_respawns_and_reopens_open_files`: the
named-but-unsaved `Untitled.md` now legitimately joins the Tier-1 crash-reopen set.
PRODUCTION CODE UNCHANGED — `listed_files()` stays as-is. The `wolf_open_files` mirror
feeds ONLY the crash-restart reopen (`open_files_for_tabpage`), NOT the buffer picker
(picker uses the separate `list_buffers`/`getbufinfo` path). Reopening `Untitled.md`
via `:e` is exactly what triggers Neovim swap recovery of unsaved edits — Wolf's
headline feature — so including it is correct; excluding it would permanently lose
unsaved Untitled edits. TEST-WRITER fix to `nvim_restart.rs`: change
`assert_eq!(reopen.len(), 2, …)` → expect 3 (two fixtures + startup `Untitled.md`);
add a positive assertion that `mirror.files` contains a basename `Untitled.md` entry;
update the file header doc (lines 1-4) to note Tier-1 now includes named-but-unsaved
Untitled buffers. Reopen loop and post-restart `listed_has` need no change.

### NEW RISK to verify — "restart shadow" (TEST-WRITER adds test; CODER fixes only if it fails)

On restart the fresh nvim's `VimEnter` naming names the fresh startup buffer
`<cwd>/Untitled.md` WITHOUT loading it (`nvim_buf_set_name` skips the swap check). If
the subsequent Tier-1 `:e <cwd>/Untitled.md` then merely switches to that already-named
empty buffer, `SwapExists` (init.lua:45-79) may NOT fire and swap recovery is shadowed
→ unsaved edits lost. TEST-WRITER: add/extend a test (home:
`crates/wolf-core/tests/swap_recovery.rs`) — type distinctive text into the startup
`Untitled.md`, `self_sigkill`, `restart_nvim`, drive the reopen via
`open_files_for_tabpage`, assert the recovered buffer's contents match the typed text.
CODER: run it. If it PASSES, done. If it FAILS (shadow confirmed), minimal fix — make
the `VimEnter` STARTUP naming treat "a swap file already exists for `<cwd>/Untitled.md`"
as a collision (name the fresh startup buffer `Untitled 2.md` so the reopen `:e
<cwd>/Untitled.md` recovers cleanly into its own buffer). Do NOT change the Cmd-N
(`wolf_new_untitled`) collision rules.

### Do NOT edit tests to assert wrong behavior

All above are legitimate test updates reflecting the intended feature. The coder must
still not weaken assertions to force a pass.
