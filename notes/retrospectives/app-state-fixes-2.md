# Retrospective: route normal-mode `ZZ` and `ZQ` through `do_quit`

Last verified at commit `acddb2a703dcbe25ef9c917a1a1b1d44f38a2a92`.

The task mapped normal-mode `ZZ` and `ZQ` in `runtime/nvim/init.lua`'s `wolf_setup_layout` onto `do_quit`, so that on a tab's last pane they close the window through `close_window` instead of quitting Neovim, and `wolf --wait` exits `0`. It ran in one pass: `architect-1`, `test-writer-1`, `coder-1`, `technical-writer-1` and `reviewer-1`, about $7.25 and 28 minutes of agent time in all. No rejection, discrepancy or resume occurred, and both gates exited 0 on both workers at their first run.

## What went well

- **`do_quit` is the single quit path, so the fix was four lines.** Every intercepted quit already calls `do_quit` with `write`, `force` and `all` flags, so plan 1 needed only two `map` calls beside the `WolfWriteQuit` abbreviations, using the function's own `map` and `silent` locals. Each falsifier of plan 1 §4 was a one-line edit of a map, and `coder-1` showed all nine red in under two minutes of scripted runs.
- **The three test targets were already wired and had siblings to copy.** `//crates/wolf-core:commands`, `:layout_intents` and `:wolf_wait` list their sources and carry the real-Neovim runfiles, so the task changed no `BUILD.bazel`. The new tests follow `ex_wq_writes_to_disk_and_closes_window`, `ex_q_closes_pane_when_multiple` and W2 with their helpers `spawn_wait`, `open_waited` and `shut_down_on_close_window`, and `test-writer-1` wrote all seven in about five minutes for $1.40.
- **`Liveness` made "Neovim did not exit" a direct assertion.** The last-pane tests check `Liveness::Alive` after `SETTLE`, and review 1 found that state terminal in a core-only test, so the check cannot pass after a restart.

## What did not go well

- **`runtime/nvim/README.md` took `architect-1` four reads.** At 56,785 bytes, its `cat` and a 90-line `sed` both overflowed the tool's output limit and were saved to files, so the architect re-read the same sections with `cut -c1-1500` and `cut -c1-1800`. The task then grew it to 57,262 bytes and `crates/wolf-core/tests/README.md` to 72,440, both under the `big-file` warning. Recommendation 1 of [cli-open-2-4](cli-open-2-4.md), to split the over-budget READMEs, comes up again.
- **The instructions' "`:x`" could be read two ways.** They asked for `ZZ` to "behave as `:x` (write if modified, then quit)", but Wolf's `:x` abbreviation goes to `WolfWriteQuit`, which always writes. Plan 1 settled it for the parenthesis and warned the reviewer not to align the two, and `runtime/nvim/README.md` gained a Decisions paragraph for the exception. Nothing was reworked, but the documented divergence from native `:x` stays for the next task to trip on.
- **An unmeasured lead on the landed maps.** Review 1 notes that `init.lua` sets no `timeoutlen`, so `Z`, a pause, then `Z` may time out the map and run native `ZZ`, quitting Neovim on the last pane; `<C-w>q` has the same exposure. Nobody measured it, and no test covers it.
- **The Linux gate ran twice.** `coder-1` ran `regina on linux -- ./gates.sh` after its narrow `cargo test` of the seven tests had passed, about seven of its 11.6 minutes, and Regina's gate then ran it again. The cost was wall time only, and earlier reports such as [retro-fixes-4](retro-fixes-4.md) count the coder's own run as a strength.

## Recommendations

1. **Split `runtime/nvim/README.md` and `crates/wolf-core/tests/README.md`.** This is recommendation 1 of [cli-open-2-4](cli-open-2-4.md), come up again. Here it cost `architect-1` three extra reads of the same README, and every task that touches the Lua runtime reads it first.
2. **Decide whether Wolf's `:x` and `:xit` should write only a modified buffer.** Routing them to `do_quit({ write = vim.bo.modified })`, as `ZZ` is, would match native Vim and remove the exception paragraph from `runtime/nvim/README.md`. It changes user-visible behaviour, so it belongs in a request that says so, not a cleanup alone.
3. **Measure the `timeoutlen` lead from review 1.** A real-Neovim test in `crates/wolf-core/tests/commands.rs` that types `Z`, waits past `timeoutlen`, then types `Z` on the last pane would show whether native `ZZ` runs. If it does, the same fix covers `<C-w>q`.
