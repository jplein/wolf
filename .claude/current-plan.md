# Plan: `:qa` and variants close the active window

## Feature request

In the captive Neovim runtime, `:qa` and its variants must **close the active
(front-end) window** — behaving like `:q` EXCEPT that when there are multiple
panes, the whole window is closed rather than just the active pane. In the
single-pane case `:qa` and `:q` are identical (both close the window).

This BUILDS ON the just-shipped `:q` family (`do_quit`, `WolfQuit` /
`WolfWriteQuit`, the bare-word abbrevs) in `runtime/nvim/init.lua`'s
`_G.wolf_setup_layout(chan)`. The `:q` change previously declared quit-ALL "out
of scope"; this change brings it IN scope — those "out of scope" notes in code
comments and docs must be updated.

## Authoritative investigation already done by the orchestrator (build on this)

### Semantics

- `:qa` (and variants) → ALWAYS close the front-end window, i.e. always take the
  "close window" branch that fires `vim.rpcnotify(chan, "wolf_command",
  "close_window")` — regardless of how many panes are open. (This is exactly the
  `:q` last-pane branch, but unconditional on pane count.)
- Force / write / unsaved-guard semantics mirror `:q` EXACTLY (keep it "basically
  like :q"):
  - `qa!` (bang) → force (discards; overrides the modified guard).
  - `wqa`/`xa` (write variants) → `:write`(`!`) first; abort the quit if the
    write fails.
  - a plain (non-force) `:qa` on a MODIFIED buffer is REFUSED with the same E37
    echo the `:q` last-pane branch uses. KEEP the guard scoped to the current
    buffer (`vim.bo.modified`), same as `do_quit` today — do NOT expand it to
    scan all panes. Rationale: the user framed this as "basically like `:q`";
    consistency with `:q` is the goal, and closing a front-end window does not
    unload the shared nvim's buffers except when it is the LAST view (buffers are
    global to the captive nvim), so the active-buffer guard matches `:q`'s UX
    without new complexity.

### Interception mechanism (PROVEN empirically against pinned nvim v0.12.3)

Same mechanism as the `:q` family. Verified this session for the qa family:
`:qa`, `:qa!`, `:qall`, `:quitall`, `:wqa`, `:wqa!`, `:xa`, `:xall` all route
correctly with the bang preserved, and there is NO collision with the existing
`q`/`wq`/`x` abbrevs (because `a` is a keyword char, `:qa` forms a distinct word
and never triggers the bare `q` abbrev). Concretely:

- Extend the existing `do_quit(opts)` helper with an `all` flag: `opts = { write,
  force, all }`. When `all` is true, SKIP the pane-count check and always take
  the close-window branch (subject to the same modified guard + force). Minimal
  diff — one added condition, e.g. change `if #vim.api.nvim_tabpage_list_wins(0)
  > 1 then` to `if not opts.all and #vim.api.nvim_tabpage_list_wins(0) > 1
  then`. (The existing `:q` callers pass no `all`, so `opts.all` is nil/false —
  behavior unchanged.)
- Add two internal user commands (NOT registry commands; do NOT touch
  command.rs/FFI/Swift):
  - `WolfQuitAll` → `do_quit({ write = false, force = o.bang, all = true })`
  - `WolfWriteQuitAll` → `do_quit({ write = true, force = o.bang, all = true })`
  - both `{ bang = true, nargs = 0 }`.
- Add bare-word abbrevs (reusing the existing `abbrev()` helper): `qa`, `qall`,
  `quitall` → `WolfQuitAll`; `wqa`, `wqall`, `xa`, `xall` → `WolfWriteQuitAll`.
  Do NOT add `qa!`/`wqa!` abbrevs — the trailing `!` rides through to
  `:WolfQuitAll!` / `:WolfWriteQuitAll!` (`o.bang` → force), same proven reason
  as the `:q` family.

### Reuses the existing `close_window` command — NO Rust/Swift/FFI changes

`close_window` is `Command::CloseWindow` (id `"close_window"`, discriminant 1),
already wired end to end. `:qa` firing `wolf_command "close_window"` routes
through `Command::from_id` onto the drainable command queue; the macOS front-end
drains it and calls `window?.performClose(nil)`. The KEY behavioral difference
from `:q` (and the thing tests must prove): with MULTIPLE panes open, `:qa`
fires `close_window` (whole window closes) whereas `:q` closes just the active
pane (pane count drops, NO command fired).

### Test patterns (mirror the `:q` tests added this session)

- `crates/wolf-core/tests/commands.rs` — has `ex_command`, `drain_until_command`,
  `poll_until`, and the `:q` tests (`ex_q_on_last_pane_emits_close_window`,
  `ex_q_bang_forces_close_window_when_modified`,
  `ex_wq_writes_to_disk_and_closes_window`, `ex_q_refuses_modified_last_pane`).
  The `:qa` command-channel tests belong here.
- `crates/wolf-core/tests/layout_intents.rs` — has `ex_q_closes_pane_when_multiple`
  (proves `:q` drops the pane count). The CRITICAL differentiator test for `:qa`
  belongs here or in commands.rs: with multiple panes, `:qa` must fire
  `close_window` and must NOT merely drop the pane count. Prefer commands.rs
  (drain_until_command) for the close_window assertion; a negative pane-count
  check can accompany it.

### Docs to update

- `runtime/nvim/README.md` — the `_G.wolf_setup_layout(chan)` section: extend the
  QUIT note to describe the `:qa` family (always closes the window; the `all`
  flag), add the two new user commands + abbrevs to the Ex-command overrides
  bullet, and REMOVE/UPDATE the "quit-ALL is out of scope" statement.
- `command-lifecycle.md` — the §5 note already says the `:q` family fires
  `close_window`; extend it to include the `:qa` family.
- Update the `do_quit` code comment in `init.lua` (it currently says quit-ALL is
  out of scope).
- Run `prettier --write` on edited Markdown.

### Build/test toolchain

- Build/test with **bazelisk** (NOT cargo). Run at least
  `//crates/wolf-core:commands` and `//crates/wolf-core:layout_intents`; the
  final verification is `bazelisk test //...` (the documented full-suite command).

## Architect plan (APPROVED by orchestrator)

Key points (full plan executed by the agents):
- `init.lua` `do_quit`: change the guard to `if not opts.all and #vim.api.nvim_tabpage_list_wins(0) > 1 then` — one condition; existing `:q` callers pass no `all`, so unchanged. Update the comment block (drop "quit-ALL out of scope").
- Add user commands `WolfQuitAll` (write=false, all=true) / `WolfWriteQuitAll` (write=true, all=true), both `{ bang=true, nargs=0 }`.
- Add abbrevs: `qa`,`qall`,`quitall`→WolfQuitAll; `wqa`,`wqall`,`xa`,`xall`→WolfWriteQuitAll. NO `qa!`/`wqa!` abbrevs (bang rides through). Extend the collision comment to the qa family.
- Modified guard stays current-buffer-scoped (`vim.bo.modified`). No Rust/Swift/FFI changes.
- Tests (commands.rs): ex_qa_on_last_pane_emits_close_window, ex_qa_bang_forces_close_window_when_modified, ex_wqa_writes_to_disk_and_closes_window, ex_qa_refuses_modified_last_pane, and the MUST-HAVE differentiator ex_qa_emits_close_window_when_multiple_panes (use `ex_command(&core,"vsplit")` + poll windows growth, then `:qa`, assert CloseWindow drained — avoids importing intent machinery/ctrl). Tests (layout_intents.rs): ex_qa_does_not_merely_drop_pane_count (negative: pane count does NOT drop to base after `:qa` with two panes).
- Docs: README QUIT bullet + Ex-command overrides bullet (remove out-of-scope note); command-lifecycle §5 Face A; init.lua comment; prettier.
- Verify: `bazelisk test //crates/wolf-core:commands //crates/wolf-core:layout_intents`, then `bazelisk test //...`.

## Status

- [x] Architect plan
- [x] Tests written (commands.rs: ex_qa_on_last_pane_emits_close_window, ex_qa_bang_forces_close_window_when_modified, ex_wqa_writes_to_disk_and_closes_window, ex_qa_refuses_modified_last_pane, ex_qa_emits_close_window_when_multiple_panes; layout_intents.rs: ex_qa_does_not_merely_drop_pane_count)
- [x] Implementation + tests passing — init.lua: `all` flag on do_quit + WolfQuitAll/WolfWriteQuitAll + 7 abbrevs. `bazelisk test //crates/wolf-core:commands //crates/wolf-core:layout_intents` → pass (all new + existing :q tests). No Rust/Swift/FFI changes.
- [x] Review — APPROVED, no findings. Confirmed all-flag correctness, bang mechanism, no collision, reuse of close_window (no Rust/Swift/FFI changes), both sides of the differentiator tested, existing :q tests untouched.
- [x] Docs — runtime/nvim/README.md (QUIT bullet extended for :qa + all flag; Ex-command overrides bullet + 2 new user commands/7 abbrevs; removed quit-ALL out-of-scope wording) and command-lifecycle.md §5 Face A (:qa shares close_window trigger). prettier: clean.

## Final verification

`bazelisk test //...` → 79/80 pass on the first sweep. The single failure was
`//crates/wolf-core:input_keys` (`cursor_moves`) — a PRE-EXISTING timing flake
(reads the cursor before the `j` motion flushes), unrelated to :qa; it passed in
the prior full-suite run and 5/5 on re-run. Not caused by this change; it is a
latent flake in a file newly wired into Bazel this branch. FOLLOW-UP: harden
`cursor_moves` to wait for a flush/poll the cursor (test-writer).

FEATURE COMPLETE: :qa family. All :qa + :q tests pass reliably.
