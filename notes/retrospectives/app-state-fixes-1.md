# Retrospective: raise Wolf.app's bundle version and register it with Launch Services on install

Last verified at commit `d6bfe849c8c6e5212bee93f585e9d65554e615e3`.

The task set `Info.plist` to `0.2.0` (build `2`), made `crates/wolf-macos/install_wolf.sh` run `lsregister -f` on the installed bundle, and added `install_wolf_test` beside two new checks in `bundle_layout_test`. It ran in one pass: `architect-1`, `test-writer-1`, `coder-1`, `technical-writer-1` and `reviewer-1`, about $5.15 in all. No rejection, discrepancy or resume occurred, and both gates exited 0 on both workers at their first run.

## What went well

- **The installer was testable through one environment override.** Plan 1 had the coder add `WOLF_LSREGISTER`, so `install_wolf_test` runs the real script on a `ditto` archive with a temporary `HOME` and a fake `lsregister`. Every falsifier of plan 1, `V1` to `L3`, gave its own red in `coder-1` at the first try, and no test touches the real Launch Services database.
- **`bundle_layout_test`'s `plist_get` helper made the version checks two short additions.** `test-writer-1` added checks (ad) and (ae) after (ac) without new plumbing, and `coder-1`'s red run named the found value.
- **An earlier report's finding on mirror headings was applied.** `architect-1` searched `notes/retrospectives` for the `md-anchor` ordering before planning, and plan 1 §3 had the coder add `## Launch Services registration` with the comment that links it. Neither gate failed on it. This is the case of recommendation 1 of [followups-1](followups-1.md).
- **The `sh_test` needed no sandbox exception.** Writing everything under `TEST_TMPDIR` let `ditto` run under the darwin sandbox, as plan 1 predicted, and `comment_srcs` already globbed `*.sh`, so no lint wiring was needed.

## What did not go well

- **The architect learned the mirror-heading rule from a report, not from its definition.** `architect-1`'s system prompt says nothing about who writes a new `##` in a `notes/<path>.md` mirror or when. It found the rule only by grepping `notes/retrospectives` for "mirror". Recommendation 1 of [followups-1](followups-1.md) comes up again.
- **`notes/wolf-macos-decisions.md` is now 836 bytes under the `big-file` ceiling.** It grew from 48,124 to 49,164 bytes against the 50,000 of `tools/lint/markdown_check.sh`. `technical-writer-1` measured it before editing and review 1 flagged it. The next decision added to the crate will trip the warning, as happened to the READMEs in [application-state-1](application-state-1.md) and [cli-open-2-1](cli-open-2-1.md).
- **The `:InstallWolf` behaviour is described in three documents.** The instructions named both `crates/wolf-macos/README.md` and `build.md` for the registration sentence, so both now say that a missing or failing `lsregister` only warns, and the mirror says it again with the reason. Review 1 also noted that `build.md`'s "Installing the app" restates the mirror's `## Archive and staging` rationale, which predates this task.

## Recommendations

1. **Say in the architect's definition that a new mirror heading is the coder's, in the same change as its comment.** This is recommendation 1 of [followups-1](followups-1.md), come up again. Here it cost `architect-1` a search of the reports, and an architect that skipped it would have planned the heading for the technical writer and failed `md-anchor` at the implementation gate.
2. **Split or trim `notes/wolf-macos-decisions.md` before the next macOS decision.** Its largest sections, `## State is a tree, and the front-end owns every write` at about 5,700 bytes and `## Zoom Pane` at about 5,100, could each move to a free-form note linked from it. Doing it now spares a later writer from trimming under a `big-file` warning in the middle of a task.
3. **Let `build.md`'s "Installing the app" link the `:InstallWolf` behaviour instead of restating it.** Keep the installer's behaviour in `crates/wolf-macos/README.md` and its reasons in `notes/crates/wolf-macos/install_wolf.sh.md`, and give `build.md` the command and a link. This task edited all three for one fact, and the next change to the installer would edit all three again.
