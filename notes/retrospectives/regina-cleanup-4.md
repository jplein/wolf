# Retrospective: gate the Markdown of the regina skill and `.claude/agents/`

Last verified at commit `ca199573e6e7a3d8f5878e118de4d60346c75f0b`.

The task brought `.claude/skills/regina/` and `.claude/agents/` under the markdown checker, a new `prettier-claude` step in `gates.sh`, and doc-lint's `md-link` and `md-anchor`. It also fixed the five checker findings already there. `coder-1` landed the code with the test writer's `markdown_all_test.sh` and two doc-lint cases in `762b4b5`, and `technical-writer-1` landed the documents in `ca19957`.

There was no round trip. Every gate exited 0 on both workers on its first run, and review 1 approved with minor notes. The five invocations cost $12.78 over about 49 minutes: `architect-1` $5.45 (60 turns, 18.4 minutes), `reviewer-1` $2.60 (52), `coder-1` $2.07 (49, 16.9 minutes), `technical-writer-1` $1.63 (36) and `test-writer-1` $1.04 (19).

## What went well

- **The standalone checker counted the work before the gate existed.** `architect-1` ran `./tools/lint/markdown_check.sh` over the 13 in-scope files and planned the five fixes from its output. `coder-1` ran it again after the fixes and got `No findings.` before touching any gate.
- **Sibling tests gave the new test its shape.** `test-writer-1` modelled the git isolation of `markdown_all_test.sh` on `regina_run_clean_test.sh`, and the T2 and T3 cases on the `.workspace/` and `node_modules/` trees `fixtures.rs` already writes at run time. It finished in 19 turns for $1.04, and `notes/test-discovery.md` told it where to wire each one.
- **`regina on linux` ran the Linux falsifiers.** `coder-1` ran C4b, C5b, C6b and C7b and the Linux `./gates.sh` on the Linux worker from the macOS sandbox, so the task's Linux red was measured, not argued.
- **This closes three earlier recommendations.** [regina-cleanup-1](regina-cleanup-1.md) recommendation 2, [regina-cleanup-2](regina-cleanup-2.md) recommendation 2 and [regina-cleanup-3](regina-cleanup-3.md) recommendation 1 asked for this gate, and the long lines and changelog words they named are fixed.

## What did not go well

- **The gated scope is written in five places.** The two paths sit in the root `BUILD.bazel` `markdown_srcs` glob, `tools/lint/markdown_all.sh` line 11, the `claude_md` array in `gates.sh`, both `gate-documentation` lines of `regina.conf.template`, and `repo_default()` in `tools/doc-lint/src/lib.rs`. Plan 1 §3 had to name each one, and widening the scope later, as to `.claude/commands/orchestrate.md`, which D3 leaves out, means editing all five.
- **No gate ran `prettier-claude` over the final commit.** Review 1 §7 found that the implementation gate ran before `technical-writer-1` edited `.claude/skills/regina/README.md`. The documentation gate ran from a `regina.conf` written at `regina-run.sh start` from the base template, which lacks the new command. Only the writer's hand run of the same `prettier --check` covered that edit.
- **The coder edited a writer's document.** Bringing `.claude/skills/regina/README.md` into scope made the gate fail on its 344-word `regina-run.sh` entry, and the implementation gate runs before the writer. So plan 1 D5 gave the cut to `coder-1`, which took four edits to reach 235 words. [application-state-2](application-state-2.md) records the same ordering problem for a broken link, and it came up again.
- **Provenance of the agent definitions had to be checked by hand.** Before planning the rewording, `architect-1` grepped 282 copies of regina's definitions under `/nix/store` to show the four flagged sentences were wolf's own and not copied protocol. Review 1 §8 adds that a re-copy at a regina upgrade can bring in a banned word or an 800-character line, which these gates will now fail.
- **Two review notes landed unfixed.** In `tools/lint/markdown_all_test.sh`, the `*.yaml)` arm at line 121 can never match, because `.claude/*)` at line 118 comes first, so a listed `flow.yaml` fails with the wrong reason. `.claude/skills/regina/README.md` line 5 says `.prettierignore` keeps "the rest of `.claude/`" out when it keeps all of it out of the tree-wide step. The base flow has no notes pass, as in the three earlier cleanup tasks.
- **The plan's prediction for T3 was wrong, harmlessly.** Plan 1 §4 said T3 would pass before the code, but its root-A positive control needs the prefix walk, and `coder-1` found it red. Nothing was reworked.

## Recommendations

1. **Keep the gated `.claude/` scope in one place.** Add `tools/lint/prettier_claude.sh` holding the prettier command and its two globs, and call it from `gates.sh` and both `gate-documentation` lines of `regina.conf.template` instead of the inline command. Then a scope change edits the script, `markdown_all.sh`, the root glob and `repo_default()`, not five places, and a running regina run reads the current command from the tree, which would have closed review 1 §7's gap.
2. **Say in the README's `agents/` entry which sections are copied.** In `.claude/skills/regina/README.md`, mark which sections of each definition are regina's protocol and which are wolf's own, and say the gate requires a re-copy to pass the checker. It would have replaced `architect-1`'s grep of 282 store copies, and it warns the next upgrade before its gate fails.
3. **Mend review 1's two notes.** Move the `*.yaml)` arm above `.claude/*)` in `tools/lint/markdown_all_test.sh`, and write "keeps `.claude/` out of the tree-wide step" on line 5 of `.claude/skills/regina/README.md`.
4. **Check the shared worker sections.** [regina-cleanup-3](regina-cleanup-3.md) recommendation 2 is still open, and the step added here is where such a check would run.
