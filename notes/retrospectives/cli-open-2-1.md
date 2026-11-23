# Retrospective: the `wolf` command, the open channel, tab IDs and the file-open rules

Last verified at commit `3d90ffdc4d452fbe9d8439facb545e3a6e0de566`.

The task built the platform-independent half of `specs/cli-and-file-open.md` in `wolf-core`: `src/bin/wolf.rs`, `open_channel.rs`, `tab_id.rs`, `file_open.rs`, `WOLF_TAB_ID` in terminal sessions, and six new test targets. It took two rounds and ten invocations for about $28.69: round 1 cost $23.83 and round 2, caused by one review rejection, cost $4.86.

## What went well

- **Pure functions made 67 reds cheap.** `project_directory` and `choose_pane` take plain paths and a hand-built layout, and the channel runs over a real socket without Neovim. `coder-1` drove every plan ID through one falsifier script, and its transcript shows the whole pass, 67 entries each red, restored by checksum and green again, running from 16:48 to 16:55. Review 1 §4 found every plan ID with its own red.
- **The wiring rule held for six new targets.** `CLAUDE.md`'s "Every new test must actually be run" and the "Adding a test file" section of `crates/wolf-core/tests/README.md` gave `test-writer-1` a sibling to mirror for each kind: headless, real `git`, real socket, real binary, real Neovim. Review 1 §3 found all six targets passing in the macOS gate, with no wiring round trip.
- **The socket-path length was measured before it could fail a gate.** `architect-1` printed the worker's `TMPDIR` (102 bytes) in its 32nd command and planned relative state dirs from one kept base directory per test binary. No gate run failed on a socket path. The gotcha is now in the `open_channel.rs` entry of `crates/wolf-core/tests/README.md`; keep it there, because the next socket test meets the same limit.
- **`libc` was already a dependency.** `getentropy` and `poll` came from it, so the task needed no `Cargo.toml` or `Cargo.lock` change and the `wolf-core-deps` step passed on both workers (review 1 §5).
- **The instructions quoted the spec sentence to change.** The task named the `specs/terminal-panes.md` bullet and what it must come to say, which an earlier attempt had stopped on. No agent disputed a spec this time.

## What did not go well

- **An unplanned test fell between the test writer and the coder again.** `test-writer-1` added `parse_accepts_any_well_formed_uuid` to `crates/wolf-core/tests/tab_id.rs` and listed it, with no ID, among new rows of planned tests. `coder-1` picked three new functions out of that list for IDs of its own, missed this one, and wrote that every other addition "sits inside a test that already has a plan ID". Review 1 rejected on that alone, and round 2 cost $4.86 and about 22 minutes.

- **The test writer's definition still has no rule for it.** `test-writer-1`'s system prompt says nothing about a test the plan does not ask for, while `coder-1`'s says to make up an ID for one. So the coder sorts the test writer's prose by hand, which is where this test was lost.
- **The markdown checker failed `coder-1`'s first gate run on wording the instructions dictated.** Both workers' `./gates.sh` runs failed only on `//tools/lint:markdown`: the instructions' `terminating NUL` tripped the all-caps rule, and the dictated `specs/terminal-panes.md` sentence made a 1009-character line. `tools/lint/markdown_check.sh` allowlists `DEL`, `ESC` and `EOF` but not `NUL`. The fix and re-run cost about seven minutes and no invocation, but the spec now says "null byte" and the terminal-panes bullet is a loose list item split into two paragraphs (review 1 §5).
- **The stale-socket probe drifted from the plan with no row to catch it.** Plan 1 §2 said only a refused connect means stale, but its falsifiers C10 and C11 covered only a stale and a live socket, so `coder-1`'s remove-on-any-error `bind` passed. Review 1 §2 noted it as optional, and `architect-2` folded it into the round the rejection had already opened, adding C19, C20 and fresh reds for C10 and C11. Nothing in the repository invited it.
- **Two wolf-core READMEs grew past the size warning.** `crates/wolf-core/src/README.md` crossed it, from 47,337 to 51,865 bytes, and `crates/wolf-core/tests/README.md` went from 64,541 to 69,890. The Linux documentation gate printed both as `big-file` warnings.

## Recommendations

1. **Give an unplanned test its own ID at the test writer.** This is recommendation 1 of [sidebar-toggles-1](sidebar-toggles-1.md), come up again after [application-state-2](application-state-2.md) and [application-state-9](application-state-9.md); this is its fourth task. In `.claude/skills/regina/agents/test-writer.md`, require the report to list every new test function in one table, each with its plan ID or a proposed ID and disabling edit, separate from new rows inside planned tests. `coder-1` would have had a T6 to run, and round 2's $4.86 would not have been spent.
2. **Add `NUL` to the all-caps allowlist.** Put it in `ACRONYMS` in `tools/lint/markdown_check.sh`, beside `DEL` and `ESC`. A control character's name is not emphasis, and `coder-1` would have kept the instructions' wording and saved one of its two gate failures.
3. **Split the wolf-core READMEs.** This is recommendation 6 of [regina-land-toast](regina-land-toast.md) and recommendation 7 of [round-3-10](round-3-10.md), come up again. `crates/wolf-core/src/README.md` is now over the warning too; `src/terminal/` already has a README of its own, the shape a split could follow.
