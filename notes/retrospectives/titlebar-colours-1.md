# Keep the regina worktrees under `.worktrees/` out of Bazel's `//...`

Last verified at commit `752898ff19a0e3be1961a79cd4099d8980db327e`.

The task added `.worktrees` to `.bazelignore`, added the macOS `bazelignore` gate step that runs the new `tools/lint/bazelignore_check.sh`, and documented each `.bazelignore` entry in `tools/lint/README.md`. The commits are `ce93069` (coder) and `752898f` (technical writer).

It ran with no rework: one plan, one implementation gate run and one documentation gate run, both green on both workers, and one review that approved. The four invocations cost about $4.88 and 18 minutes. `architect-1` was the largest at $1.76, 18 turns and 6.7 minutes, then `reviewer-1` at $1.22, `technical-writer-1` at $1.16 and `coder-1` at $0.74.

## What went well

- **A precedent settled the hardest design question.** `tools/lint/wolf_core_deps_check.sh` is a checker that `gates.sh` runs with no Bazel target of its own, and the `tools/lint/README.md` intro already said so. Plan 1 §1b cited it to make the new probe a gate step rather than an `sh_test`, and review 1 §3 cited it to accept that, so nobody disputed the departure from `CLAUDE.md`'s `*_test.sh` wiring rule.
- **New scripts are covered with no wiring.** The `comment_srcs()` glob in `tools/lint/BUILD.bazel` and doc-lint's `bazel-coverage` rule picked up `bazelignore_check.sh` without a `BUILD.bazel` edit, and the `run` helper in `gates.sh` made the gate step one line (plan 1 §3, review 1 §1).
- **The behaviour could be shown in the worker.** `git worktree add --detach .worktrees/probe HEAD` worked under Seatbelt, so `coder-1` ran the task's `bazel query` evidence directly: 20 packages with the entry, and 41, of which 21 under `.worktrees/`, without it. The plan's fallback was not needed.
- **The plan named every tree walker with its file.** `coder-1` reported that this made its confirmation of plan 1 §1a quick: four tool calls covered it.

## What did not go well

- **One fact is kept in four ignore lists, and one of them missed it for months.** `.worktrees` entered `.gitignore` in `b159793`, `.prettierignore` in `9e43f54` and doc-lint's `WalkConfig::repo_default()` in `0c9ee7f`, but `.bazelignore` only in this task. That gap was the whole task. `1b24259` had added `.workspace` to all four at once, and its commit message lists them, but nothing in the repository says which lists a directory holding a checkout needs.
- **The tree-walker audit was done three times.** The task asked for it, `architect-1` spent seven of its 15 commands reading `.gitignore`, `.prettierignore`, `tools/doc-lint/src/walk.rs` and `lib.rs`, the root `BUILD.bazel` and the `tools/lint` scripts, then `coder-1` re-confirmed the list as plan 1 §1a asked, and `reviewer-1` re-checked each one again (review 1 §1). No document lists the walkers and the ignore list each one reads, so each agent rebuilt the list from the code.
- **The README had already drifted from `.bazelignore`.** At the base, the `.bazelignore` bullet in `tools/lint/README.md` explained only `.direnv`, while the file had listed `.workspace` since `1b24259`. `technical-writer-1` found and fixed this; no check ties an ignore file's entries to their documented reasons.
- **The new gate guards only `.worktrees`.** `bazelignore_check.sh` probes `.worktrees/` alone, so dropping `.workspace` or `.direnv` from `.bazelignore` would still pass it. This is within plan 1 §2, which asked for no more, but the regression class the task fixed is any missing entry.
- **Bumping `notes/gates.sh.md`'s header meant re-checking claims that macOS cannot measure.** `technical-writer-1` spent four commands diffing `8f5cdbfe..ce93069` across `gates.sh`, `flake.nix` and `crates/wolf-macos/BUILD.bazel`, and still could not re-measure the `rustdoc-links` timings or the "over a hundred each" `private_intra_doc_links` count, which need a Linux `cargo doc` run. Review 1 §5 notes that the bump covers them by inspection only.

## Recommendations

1. **Document the ignored directories once, against every list.** In the Caveats of `tools/lint/README.md`, replace the `.bazelignore` bullet's per-entry reasons with one table of the generated and nested directories (`.direnv`, `.workspace`, `.worktrees`, `target`, `bazel-*`, `.claude`) against `.gitignore`, `.prettierignore`, `.bazelignore` and `WalkConfig::repo_default()`, with a reason per row. Point the doc-lint README's exclusion list at it rather than restating it. This task's gap and its three-fold audit would each have been a lookup.
2. **Probe every `.bazelignore` entry, not one.** Have `tools/lint/bazelignore_check.sh` read `.bazelignore` and, for every directory entry, create a probe package under it and fail when `//...` lists any. The cost is one more probe per entry under the same query, which `coder-1` timed at under a second on a warm server.
3. **Two earlier findings came up again.** Hand-kept counts in `notes/gates.sh.md` went stale before, as [round-4-3](round-4-3.md) recorded, and the "over a hundred each" sentence is one more: make it count-free. Settling or narrowing the `rustdoc-links` cost sentence, [round-3-2](round-3-2.md) recommendation 3, still applies to the same section. Both left this task's header bump unverifiable on macOS.
