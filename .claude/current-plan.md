# Plan: `:q` and variants close the active pane / window

## Feature request

In the captive Neovim runtime, `:q` and its variants (`:q!`, `:wq`, and the
other single-window quit forms) must:

- **Close the active pane**, and
- **If the active pane is the only one open, close the (front-end) window.**

## Why this matters / context

The captive Neovim is SHARED across front-end views (one nvim per project, one
tabpage per view). A raw `:q` on the last window would quit nvim entirely (or
close a tabpage out from under a view) — a disaster. Wolf already reroutes
`:close` / `<C-w>c` / `<C-w>q` to a guarded pane-close (`wolf_do_close`) that
refuses to close the last window. `:q` and its variants are currently NOT
intercepted, so they fall through to native Neovim behavior. This change makes
the whole `:q` family behave like Wolf's pane/window model.

## Investigation already done by the orchestrator (authoritative — build on this)

### Interception mechanism (PROVEN empirically against the pinned nvim v0.12.3)

- A plain `cnoreabbrev` on the bare word (`q`, `wq`) does NOT handle the `!`
  variants: when the user types `q` then `!`, the bare-`q` abbrev fires while
  `getcmdline() == 'q'` is still true, expanding `q` and leaving a stray `!`
  appended → e.g. `lua wolf_quit('q')!` → Lua syntax error. Confirmed by
  experiment.
- **The robust approach that WORKS for all variants:** expand the abbrev to a
  real `nvim_create_user_command` that accepts a bang (`{ bang = true }`). Then
  `:q!` expands `q` → `WolfQuit`, and the trailing `!` becomes `WolfQuit!` — a
  valid banged user-command invocation whose callback sees `opts.bang == true`.
  Empirically verified: `:q`, `:q!`, `:quit`, `:quit!`, `:wq`, `:wq!`, `:x`,
  `:xit` all route correctly with the bang preserved.

  Two internal user commands are needed:
  - `WolfQuit` (bang → force): for `q`, `quit`.
  - `WolfWriteQuit` (bang → force): for `wq`, `x`, `xit`.

  (These are INTERNAL plumbing — they are NOT registry `COMMAND_SPECS` commands
  and must NOT be added to the `Command` enum / FFI. They exist only as
  abbreviation targets. Naming is at the architect's discretion, but keep them
  distinct from the registry-generated `:WolfClosePane` / `:WolfCloseWindow`.)

- The abbrevs are installed exactly like the existing ones in
  `runtime/nvim/init.lua`'s `_G.wolf_setup_layout(chan)` (see the `abbrev()`
  helper and the `abbrev("close", "lua wolf_do_close()")` line, ~L760–779).

### The close logic

A new helper (e.g. `do_quit({ write, force })`), living inside
`wolf_setup_layout(chan)` alongside `do_close` (so it closes over `chan`), and
exposed as a global (e.g. `_G.wolf_do_quit`) like `_G.wolf_do_close`:

```
local function do_quit(opts)          -- opts = { write = bool, force = bool }
  if opts.write then
    pcall(vim.cmd, opts.force and "write!" or "write")
  end
  if #vim.api.nvim_tabpage_list_wins(0) > 1 then
    pcall(vim.api.nvim_win_close, 0, opts.force or false)   -- close the pane
  else
    vim.rpcnotify(chan, "wolf_command", "close_window")      -- last pane: close the window
  end
end
```

Key facts backing this:
- `#vim.api.nvim_tabpage_list_wins(0) > 1` is the SAME multi-pane guard
  `wolf_do_close` uses (init.lua ~L681–685).
- `nvim_win_close(0, force)` with `force = false` naturally refuses to discard
  unsaved changes (matching `:q` semantics); `force = true` (from `:q!`)
  discards (matching `:q!`).
- `close_window` is a real registry command (`command.rs`: `Command::CloseWindow`,
  id `"close_window"`, discriminant 1). Firing `vim.rpcnotify(chan,
  "wolf_command", "close_window")` routes through `Command::from_id` onto the
  drainable command queue; the macOS front-end drains it and calls
  `window?.performClose(nil)` (`CommandDispatcher` `.closeWindow`). This is the
  established way to close the front-end window from inside nvim.
- `chan` here is core's RPC channel — the SAME channel `wolf_command`
  notifications already use (the layout helper's `chan`).

### Scope decision (orchestrator)

- IN scope: the single-window quit family — `q`, `q!`, `quit`, `quit!`, `wq`,
  `wq!`, `x`, `xit`. (`x`/`xit` = write-then-quit; treat like `wq`.)
- OUT of scope: the quit-ALL family (`qa`, `qall`, `wqa`, `xa`, `qa!`, …). These
  are pre-existing un-intercepted behavior; leave them as-is and note the
  deferral. Do not expand scope without asking.

### Test patterns to mirror

- `crates/wolf-core/tests/layout_intents.rs` — drives a REAL hermetic nvim,
  types Ex commands one key at a time (`ex_command` helper), polls the snapshot
  (`poll_until`), and already tests pane close: `ctrl_w_c_closes_pane_when_multiple`
  and `ctrl_w_c_noop_on_last_window`. The multi-pane `:q` close test mirrors the
  former.
- `crates/wolf-core/tests/commands.rs` — drives the command channel and has
  `drain_until_command` for asserting a drained `Command` (e.g.
  `Command::CloseWindow`). The last-pane `:q` → close-window test mirrors this.
- NOTE the `ex_command` helper types the word one char at a time, which is what
  triggers `cnoreabbrev`. For a banged variant the test types e.g. `q!`.

### Build/test toolchain

- Build/test with **bazelisk** (NOT cargo). Integration tests need the pinned
  nvim via `WOLF_NVIM_BIN` (the common helpers panic if unset) — the Bazel test
  targets wire this up.

### Docs to update

- `runtime/nvim/README.md` — the `_G.wolf_setup_layout(chan)` section
  (Ex-command overrides list) and the asymmetry/close notes.
- `command-lifecycle.md` — only if the close_window path description needs it
  (likely a light touch, since `:q` reuses the existing `close_window` command).
- Any affected directory README. Run `prettier --write` on edited Markdown.

## Orchestrator decision on the architect's refinement

ACCEPTED: the architect tightened the last-pane path so a plain `:q` on a
MODIFIED last-pane buffer is REFUSED (native `:q` parity via an unsaved-changes
guard + echo) rather than silently closing the front-end window and abandoning
edits, and a failed write in a `:wq` ABORTS the quit. This prevents data loss
and matches vim semantics; `:q!` (force) still closes. The optional
`ex_q_refuses_modified_last_pane` guard test IS in scope. Everything else per
the architect plan below.

## Architect plan

# Architect Plan: `:q` family closes the active pane / window

## 1. Summary

The captive Neovim currently intercepts `:close` / `<C-w>c` / `<C-w>q` (guarded
pane-close) but lets the `:q` family fall through to native Neovim, which would
quit nvim entirely or tear a tabpage out from under a front-end view. This
change reroutes the single-window quit family (`q`, `q!`, `quit`, `quit!`, `wq`,
`wq!`, `x`, `xit`) into Wolf's pane/window model: with multiple panes in the
tabpage it performs a guarded `nvim_win_close` (respecting bang/unsaved
semantics); on the last pane it fires the existing `close_window` registry
command over RPC so the front-end closes the window. The work is entirely inside
`runtime/nvim/init.lua`'s `_G.wolf_setup_layout(chan)` plus new integration
tests and docs. No Rust, Swift, or FFI changes are required — `close_window`
(`Command::CloseWindow`, id `"close_window"`, discriminant 1) already exists end
to end, and the two new user commands are internal Lua-only plumbing, NOT
registry commands.

## 2. Files to modify

### `runtime/nvim/init.lua` — inside `_G.wolf_setup_layout(chan)` (~L663–791)

All additions live INSIDE `wolf_setup_layout` so they close over `chan` (core's
RPC channel — the same one `do_close`/`do_focus` use). Place them alongside the
existing `do_close` helper (~L681–685), the globals block (~L710–713), and the
`abbrev(...)` calls (~L773–779).

**a. New helper `do_quit(opts)`** — next to `do_close`, before the globals
block. `opts` is `{ write = boolean, force = boolean }`:

- If `opts.write`: run `pcall(vim.cmd, opts.force and "write!" or "write")`. If
  the write fails (pcall false — nofile/scratch buffer, E32/E212), ABORT
  `do_quit` (return early) so a failed `:wq` does not close, matching native.
- Multi-pane branch — guard `#vim.api.nvim_tabpage_list_wins(0) > 1` (same guard
  as `do_close` at ~L682): `pcall(vim.api.nvim_win_close, 0, opts.force or
  false)`. `force = false` refuses to discard unsaved changes (native `:q`);
  `force = true` discards (native `:q!`). The `pcall` swallows a bare E37.
- Last-pane branch (`else`): if `not opts.force and vim.bo.modified`, surface a
  native-style refusal (`vim.api.nvim_echo({{ "E37: No write since last change
  (add ! to override)", "ErrorMsg" }}, true, {})`) and return WITHOUT firing.
  Otherwise `vim.rpcnotify(chan, "wolf_command", "close_window")`.

**b. Global exposure** — in the globals block (~L710–713) add `_G.wolf_do_quit =
do_quit` for symmetry with `_G.wolf_do_close` (the user-command callbacks capture
`do_quit` as a closure directly; the global is for consistency / Lua-face use).

**c. Two internal user commands** — near the `abbrev(...)` calls. PROVEN
bang-handling mechanism; NOT registry commands (do NOT touch
`Command`/`COMMAND_SPECS`/FFI):
- `vim.api.nvim_create_user_command("WolfQuit", function(o) do_quit({ write =
  false, force = o.bang }) end, { bang = true, nargs = 0 })`
- `vim.api.nvim_create_user_command("WolfWriteQuit", function(o) do_quit({ write
  = true, force = o.bang }) end, { bang = true, nargs = 0 })`

**d. Abbrevs (bare word ONLY)** — after `abbrev("close", ...)` (~L777), reusing
the existing `abbrev(word, replacement)` helper. Replacement is the Ex-command
NAME (not a `lua ...` string):
- `abbrev("q", "WolfQuit")`
- `abbrev("quit", "WolfQuit")`
- `abbrev("wq", "WolfWriteQuit")`
- `abbrev("x", "WolfWriteQuit")`
- `abbrev("xit", "WolfWriteQuit")`

**CRITICAL — do NOT add `q!` / `quit!` / `wq!` abbrevs.** `cnoreabbrev` matches a
whole KEYWORD word; `!` is not a keyword char. Typing `:q!` fires the bare-`q`
abbrev (`getcmdline() == "q"` still true), expands `q` → `WolfQuit`, and the
trailing `!` rides through to `:WolfQuit!` — a valid banged invocation whose
callback sees `o.bang == true`. A `q!` abbrev would be dead/broken.

### Correctness notes / edge cases

- `x`/`xit` treated as `wq` (always write). Native `:x` writes only when
  modified; on an unmodified nofile/scratch buffer our always-write path could
  error where native `:x` would not. Accepted, documented minor deviation.
- Multi-pane E37 swallowing: `pcall` on `nvim_win_close(0, false)` swallows the
  E37 native `:q` prints on a modified buffer shown only in that pane, so the
  pane silently stays open. Acceptable (no crash, no data loss); note it.
- NO Rust/Swift/FFI changes — reuses `close_window` verbatim.
- Scope: single-window quit family only. Quit-ALL (`qa`, `qall`, `wqa`, `xa`,
  `qa!`, …) stays un-intercepted, OUT of scope — add no abbrevs for them.

## 3. Files to create

None.

## 4. Test plan

All integration tests drive a REAL hermetic nvim via the shared
`tests/common/mod.rs` helpers and type Ex commands ONE key at a time (so
`cnoreabbrev` fires). Reuse the per-file locals (`ex_command`, `poll_until`,
`wait_for_initial_window`, `ctrl_w_then`, `drain_until_intent` /
`drain_until_command`). Build/test with bazelisk.

### In `crates/wolf-core/tests/layout_intents.rs`

- **`ex_q_closes_pane_when_multiple`** — mirror `ctrl_w_c_closes_pane_when_multiple`
  (L397–423). Split with `ctrl_w_then(&core, 'v')`, `drain_until_intent`, poll
  windows == base+1. Then `ex_command(&core, "q")` and poll windows == base (the
  active pane closed via guarded `nvim_win_close`; buffer is unmodified so
  `force = false` closes cleanly).

### In `crates/wolf-core/tests/commands.rs`

- **`ex_q_on_last_pane_emits_close_window`** — `wait_for_initial_window`, assert
  `core.drain_commands().is_empty()` (no emit-on-attach), then `ex_command(&core,
  "q")` on the sole (unmodified) pane; `drain_until_command` must contain
  `Command::CloseWindow`.
- **`ex_q_bang_forces_close_window_when_modified`** — the fragile BANG path.
  Modify the buffer (`ch('i')`, `ch('x')`, `key(Key::Esc)`), drain/discard, then
  `ex_command(&core, "q!")`; `drain_until_command` must contain
  `Command::CloseWindow`, proving the bang reached the callback and overrode the
  modified-buffer guard.
- **`ex_wq_writes_to_disk_and_closes_window`** — capture `file.path()` from
  `start_core_with_temp_md(DOC)`. Modify with distinctive text (`i`, chars,
  `Esc`). `ex_command(&core, "wq")`. Poll `std::fs::read_to_string(file.path())`
  until it contains the inserted text (write is async over RPC), and
  `drain_until_command` must contain `Command::CloseWindow`.
- **`ex_q_refuses_modified_last_pane`** (guard test — IN scope per orchestrator
  decision) — modify the buffer (`i`,`x`,`Esc`), drain/discard, `ex_command(&core,
  "q")`, let a SETTLE window elapse (draining wakes), assert `drain_commands()`
  does NOT contain `Command::CloseWindow`. Mirrors the negative style of
  `ctrl_w_c_noop_on_last_window`.

Do NOT weaken the existing chord/close tests — the `<C-w>` chord tests remain the
robust primary coverage.

## 5. Docs to update

- **`runtime/nvim/README.md`** — `_G.wolf_setup_layout(chan)` section:
  - Ex-command overrides bullet (L114): append the quit family — `q`/`quit` →
    `WolfQuit`, `wq`/`x`/`xit` → `WolfWriteQuit`. Explain the abbrev targets are
    two INTERNAL `nvim_create_user_command`s (`{ bang = true, nargs = 0 }`, NOT
    registry commands), and that only the bare word is abbreviated because the
    trailing `!` rides through to `:WolfQuit!` / `:WolfWriteQuit!` (`o.bang` →
    `force`).
  - CLOSE asymmetry bullet (L103) / new QUIT note: describe `do_quit({ write,
    force })` — multi-pane → guarded `nvim_win_close(0, force)` (no intent, like
    CLOSE); last pane → `wolf_command "close_window"`; write variants write first
    (aborting on failure); a plain `:q` on a modified last-pane buffer is
    refused. Note quit-ALL is out of scope.
  - Globals sentence (L107): add `_G.wolf_do_quit`.
  - Optionally the intro (L98) that lists `:split`/`:vsplit`/`:close`.
- **`command-lifecycle.md`** — light touch: if it enumerates `close_window`
  triggers, add that the `:q` family (last pane) now also fires it.
- Run `prettier --write` on every edited Markdown file.

## 6. Implementation order

1. `init.lua`: `do_quit(opts)`; `_G.wolf_do_quit`; two `nvim_create_user_command`;
   five bare-word `abbrev(...)`.
2. `commands.rs`: the four command-channel tests.
3. `layout_intents.rs`: `ex_q_closes_pane_when_multiple`.
4. Docs + `prettier --write`.
5. Run the two integration suites + pure `command.rs` tests via bazelisk.

## 7. Dependencies

None.

## Status

- [x] Architect plan
- [x] Tests written (layout_intents.rs: ex_q_closes_pane_when_multiple; commands.rs: ex_q_on_last_pane_emits_close_window, ex_q_bang_forces_close_window_when_modified, ex_wq_writes_to_disk_and_closes_window, ex_q_refuses_modified_last_pane)
- [x] Implementation + tests passing — init.lua do_quit + WolfQuit/WolfWriteQuit + 5 abbrevs. `bazelisk test //crates/wolf-core:commands //crates/wolf-core:layout_intents` → all pass (10/10, 9/9). Coder also added a missing `//crates/wolf-core:layout_intents` rust_test target to crates/wolf-core/BUILD.bazel (the file previously had no target). No Rust/Swift/FFI changes.
- [x] Review — APPROVED, no blocking issues. Confirmed do_quit correctness, bang mechanism (no broken q! abbrevs), scope (no quit-all, no Rust/Swift/FFI changes), tests match §4, new BUILD target correct. Only pending: §5 docs.
- [x] Docs — runtime/nvim/README.md (QUIT bullet, wolf_do_quit global, Ex-command overrides + bang mechanism) and command-lifecycle.md (§5 Face A note that :q family fires close_window). prettier --write run: both files conformant.
