# Retrospective: one home for the GUI-free rule's check, and two untracked `/orchestrate` files

Last verified at commit `c406106c6194f2fcc00f9ce0dcdad9a6231b462f`.

The task carried out recommendations 1 to 5 of [retro-fixes-4](retro-fixes-4.md). It cut every restatement of where `wolf-core`'s GUI-free rule is checked to a link to `notes/wolf-core-decisions.md#the-one-hard-rule`, dropped the pipeline copies from that note, recorded the macOS query's toolchain labels in `notes/gates.sh.md`, and stopped tracking `.claude/current-plan.md` and `.claude/verify-results.md`. This task's commits are `21d3fda` (coder) and `c406106` (technical writer); the earlier commits in the range belong to `round-3-1` to `round-3-3`.

It took one round: no rejection, no failed gate, no discrepancy, nothing resumed. Both gates passed on both workers on their first run, and review 1 approved with two minor notes. The four invocations cost about $4.16: `architect-1` $1.78 (29 turns, 5.6 minutes), `reviewer-1` $1.10, `technical-writer-1` $0.71 and `coder-1` $0.56.

## What went well

- **The retrospective that asked for the work had already done the sweep.** [retro-fixes-4](retro-fixes-4.md) named all six restating files and the exact headings, so the task instructions could list them and plan 1 could give the coder replacement text and link targets. `coder-1` ran 18 turns and reported that nothing made the task harder.
- **doc-lint checked every new link, including the one in a doc comment.** The `md-link` and `md-anchor` checks covered the Markdown links in `specs/`, `build.md` and `notes/`, and `doc-ref` and `doc-anchor` resolved the code-span path `notes/wolf-core-decisions.md#the-one-hard-rule` in `crates/wolf-core/src/lib.rs`'s `//!` block. Review 1 §2 relied on the gates for all of them instead of checking by hand.
- **The regina wrapper already ignored untracked files.** `regina-run.sh` checks `git status --porcelain --untracked-files=no`, so plan 1 could untrack the `/orchestrate` working files with only a one-sentence edit to `.claude/skills/regina/SKILL.md`.

## What did not go well

No step was redone. Three parts of the task instructions could not be carried out as written, and `architect-1` resolved each in plan 1's "Decisions this plan makes" at no extra round.

- **The instructions gave `specs/architecture.md` to the technical writer.** `flow.yaml`'s `docs` set is `README.md`, `/build.md` and `/notes/**`, so the technical writer cannot write `specs/`; plan 1 moved the edit to the coder. `.claude/skills/regina/SKILL.md:61` says when a task may change a spec, but not whose edit it is.
- **The instructions asked for `git rm --cached`, which no agent may run.** Agents use git only to read, so plan 1 had `coder-1` delete both files from the worktree and let Regina's commit drop them. The effect on the branch is the same, but a developer checkout that pulls `21d3fda` loses its local copies, as review 1 §8 notes.
- **The done grep could not pass literally.** `git grep -n 'cargo tree -e no-dev'` also finds `notes/retrospectives/open-items-13.md:36`, which quotes a past recommendation. Plan 1 excluded `notes/retrospectives/` from the check, and review 1 §8 agreed, because a retrospective is a record nobody rewrites.
- **The two reviewer definitions were edited identically again.** `.claude/agents/reviewer.md:57` and `.claude/skills/regina/agents/reviewer.md:86` carry the same bullet, and `coder-1` changed both in `21d3fda`, as `coder-1` of `retro-fixes-4` did in `d843298`. doc-lint skips `.claude/`, so `coder-1` checked their relative links with `ls` and review 1 §1 checked them again by hand.
- **The macOS query was measured three times.** `architect-1` ran `bazel query 'kind(rule, deps(//crates/wolf-core:wolf_core))'` with and without `--notool_deps` to count the `rrc__` labels for plan 1 item 8, and `technical-writer-1` ran both queries again to verify the counts before writing them down. The counts are not in the note, which plan 1 left optional, so the next agent who needs them measures again.
- **An unverified version claim was left as a lead.** `technical-writer-1` reported that `notes/MODULE.bazel.md` says apple_support 2.6.1 requires rules_cc 0.2.15 while `MODULE.bazel` pins 0.2.17. That is consistent with Minimal Version Selection, but nobody re-verified the file, and its `Last verified` SHA was correctly left alone.

## Recommendations

1. **Say in the task-writing guidance whose a spec edit is, and what git an agent may run.** Extend `.claude/skills/regina/SKILL.md:61` to say that a spec edit a task names is the coder's, and that a task to untrack a file asks for its deletion and a `.gitignore` entry, never `git rm --cached`. Plan 1 had to correct both, and the next task that opens a spec or untracks a file would otherwise send the architect the same puzzle.
2. **Scope a done grep away from `notes/retrospectives/`.** Add to the same guidance that a done criterion phrased as "`git grep` finds X only in Y" passes `':!notes/retrospectives'`, since retrospectives quote what they asked to remove. Here plan 1 and review 1 each had to argue the exclusion.
3. **Make one reviewer definition refer to the other.** This came up again from [retro-fixes-6](retro-fixes-6.md) recommendation 4 and [retro-fixes-4](retro-fixes-4.md): both reviewer files carry the same rule bullet and were edited in step a second time.
4. **Check Markdown links under `.claude/` too.** `tools/doc-lint/src/lib.rs:152` skips `.claude`, so the links this task added to two agent definitions were checked only by hand, twice. Letting doc-lint's `md-link` and `md-anchor` checks walk `.claude/`, or giving them a narrower exclusion than the whole directory, would make the gate prove them; it extends [retro-fixes-6](retro-fixes-6.md) recommendation 1, which asks the same of prettier.
