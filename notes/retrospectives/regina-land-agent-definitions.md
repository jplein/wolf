# Retrospective: add the facts recent retrospectives paid for to the regina definitions and `build.md`

Last verified at commit `3c40dfc3eae2625506a5c965c4d34f889181abed`.

The task carried out recommendation 6 of [round-3-10](round-3-10.md), along with facts from the other retrospectives the instructions list. It changed `architect.md`, `coder.md`, `test-writer.md`, `technical-writer.md` and `retrospective.md` under `.claude/skills/regina/agents/` (commit `b3f30d1`) and `build.md` (commit `3c40dfc`). No code or test changed.

There was no round trip. Every gate run passed on both workers on its first try, and review 1 approved with minor notes. The four invocations cost $8.83 over about 37 minutes: `architect-1` $3.88 (64 turns), `coder-1` $2.42 (67), `reviewer-1` $1.85 (36) and `technical-writer-1` $0.68 (17).

## What went well

- **A check table let one agent measure and another copy.** Plan 1 §4 gave every quoted command a check ID, C1 to C18, and D6 kept the coder out of `build.md`. `coder-1`'s report marked the forms "for build.md", so `technical-writer-1` wrote `build.md` in 17 turns. `reviewer-1` matched each quoted form to its check and to the `regina on linux` command logs without running anything new.
- **The README records which definition sections are identical.** The `agents/` entry of `.claude/skills/regina/README.md` says the `## In a regina worker` sections are word for word the same across the definitions. That settled where plan 1's D1 and D2 put the new text, and the `awk`/`cmp` extraction in C17 checked it, run by `architect-1`, `coder-1` and `reviewer-1`. Keep that entry when the definitions next change.
- **The size and word budgets were checked before writing.** `./tools/lint/markdown_check.sh --entry-words build.md` printed nothing at the base, and plan 1 §5 gave a 1,500-byte allowance against the 50,000-byte `big-file` warning. `technical-writer-1` added 1,111 bytes and the documentation gate passed.
- **The plan read the rule's source, not a guess.** D4 explained the comma list's exit 3 from rules_swift 3.6.1's `ShardingFilteringTestCollector.swift` and predicted that an alternation selects several classes. C7 confirmed it, so `build.md` gives a working form for several classes instead of only the failure.

## What did not go well

- **Three invocations looked for regina's source.** The instructions said to read regina's documentation at `$REGINA_SOURCE`, default `~/dev/jplein/regina`, which is what `.claude/skills/regina/SKILL.md` names. In a worker the variable is unset, `~/dev/jplein/regina` resolves under the task's home, and `/Users/jplein/dev/jplein/regina` gives `Operation not permitted`. `architect-1` spent about 11 of its 63 tool calls finding a copy among the nix-store sources of `regina-0.1.0.drv`, and its D3 recorded the path, so `coder-1` needed two calls.
- **The architect read rules_swift over the web.** `architect-1` made 13 `WebFetch` and `WebSearch` calls on rules_swift 3.6.1 to learn how `--test_filter` reaches XCTest. Its result is in `build.md` at `3c40dfc`, so this cost should not recur for `--test_filter`. The general gap, a pinned dependency's source with no documented local path, is recommendation 5 of [round-3-10](round-3-10.md), and it came up again here.
- **The technical writer ran the coder's checks again.** `technical-writer-1` re-ran C1, C4, C6, C7, C8 and C9 on macOS before writing, although plan 1's D6 had it copy them from `coder-1`'s report. The instructions' "Verify every command you write by running it" invited this, and it cost little, about $0.68 for the whole invocation. Nothing to change.
- **The definition still contradicts its new exception.** `architect.md` line 7 says the architect runs no build, with no qualifier, and so does its frontmatter description. The new probe in "Before you plan" and the "Write nothing" rule both make the Neovim build an exception. Review 1 §8 noted it.
- **Three claims in the new text are unmeasured.** Review 1 §7 and `coder-1`'s "Not exercised" name them: the probe form was measured in a coder's worker, never in an architect's sandbox; no poll was stopped by the tool at 10 minutes; and nobody saw that ending a turn kills background jobs. The next retrospective of a task whose architect probes Neovim should read that transcript for the first claim.
- **`build.md` is close to the size warning.** It is 49,343 bytes against the checker's 50,000-byte `big-file` warning, so the next passage of any size trips it. Six READMEs are past it already, as the Linux documentation gate's log shows.

## Recommendations

1. **Say how a worker reads regina's documentation.** `.claude/skills/regina/SKILL.md` and `README.md` name `~/dev/jplein/regina`, which no worker can read. Either have the worker configuration export `REGINA_SOURCE` as the store path of the running build, or add plan 1's D3 command to the `## In a regina worker` section of the definitions. In this task it would have saved about 11 of `architect-1`'s calls.
2. **Qualify the architect's "runs no build".** In `.claude/skills/regina/agents/architect.md`, change line 7 and the frontmatter description to say "but for the Neovim probe", so that the opening line agrees with "Before you plan" and "Rules". Review 1 §8 found it, and an architect that reads line 7 first may skip the probe.
3. **Make room in `build.md` before its next addition.** It has 657 bytes left under the `big-file` warning, and this task's +1,111 bytes would not fit again. Either run a net-negative pass over it, or have a person name a second build document in `CLAUDE.md`'s ownership list, which names `build.md` alone and which no agent may write. Without this, the next task that adds to `build.md` has to plan a cut first.
