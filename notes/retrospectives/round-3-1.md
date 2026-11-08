# Retrospective: shared swift-format lookup, `external` swift-format tests and a per-target macOS test summary

Last verified at commit `613400b2db9f57b8834097538b6256c86c361aab`.

The task carried out recommendation 2 of [retro-fixes-1](retro-fixes-1.md), recommendations 2, 3, the `external` half of 4, and 5 of [retro-fixes-2](retro-fixes-2.md), and recommendation 5 of [open-items-9](open-items-9.md). It moved `find_swift_format` into `tools/lint/swift_format_lib.sh`, made `:fix` run `:swift_format.fix`, tagged the three swift-format targets `external`, switched the macOS gates to `--test_summary=short` and pointed the `rules_rust` pin in `MODULE.bazel` at the fixture query. The change is `60aee22` (coder) and `613400b` (technical writer).

It took one round: no rejection, no failed gate, no discrepancy, nothing resumed. Both gates passed on both workers on their first run, and review 1 approved with minor notes. The four invocations cost about $5.89: `architect-1` $2.25 (27 turns, 7.9 minutes), `coder-1` $1.20 (29 turns, 5.4 minutes), `technical-writer-1` $0.98 (25 turns, 2.0 minutes) and `reviewer-1` $1.46 (23 turns, 3.9 minutes).

## What went well

- **The instructions cited each recommendation by report and number, and named the files to read.** Plan 1 could give line numbers for every edit (`swift_format_check.sh` lines 6–24, `fix_all.sh` lines 60–74, `gates.sh` line 153), and review 1 checked the work item by item against them.
- **`clippy_fix.bzl` already had a template-substitution pattern for staged tools.** The base `fix_all` rule substitutes `@@CARGO_SHORT_PATH@@` and `@@RUSTFMT_SHORT_PATH@@` into the launcher. Staging `:swift_format.fix` was one more attribute and one more `@@SWIFT_FORMAT_FIX_SHORT_PATH@@` in the same shape, and `coder-1` got it right on the first build.
- **The existing swift-format tests covered the new code path without a change.** Case 3 of `swift_format_fix_test.sh` already ran the no-argument mode that `:fix` uses, so plan 1 added no test and the test writer was skipped. Because both scripts print `ERROR: swift-format not found` from the lookup, the falsifier's failure output in `coder-1`'s report proved that each script sourced the shared file.
- **The new summary paid off in the same task.** `reviewer-1` counted the 119 `PASSED` lines in `target/gates/test.log` and saw the three `external` targets uncached there, instead of reading `bazel-testlogs` as `coder-1` and `reviewer-1` of retro-fixes-2 had to.
- **doc-lint checks the anchor in the new `MODULE.bazel` comment.** Plan 1 linked an existing heading, `tools/lint/README.md#how-each-target-works`, so the pointer passed before the technical writer touched the README and stays guarded if the heading moves.

## What did not go well

- **The architect could not read the `rules_rust` source from the task's output root.** The root is cold until the first build, so `architect-1`'s `ls` for `external/rules_rust+` found nothing and it fetched `tools/rustfmt/src/main.rs` from GitHub twice, and plan 1 then said the runner drops "`no-format`-tagged targets". `technical-writer-1` read the real file after `coder-1`'s build and found it also drops `no-rustfmt` and `norustfmt`. The `:rustfmt.fix` bullet of `tools/lint/README.md` now records the query and its filters, so the next agent has it without the source.
- **`.claude/current-plan.md` turned up in every grep for the old flag.** Line 121 still quotes `--test_summary=terse`, and `coder-1`, `technical-writer-1` and `reviewer-1` each found it, judged it out of scope and wrote a sentence about it. This came up again: it is recommendation 5 of [retro-fixes-4](retro-fixes-4.md).
- **A timing landed in `tools/lint/README.md`.** The `external` bullet of "Toolchains and binaries" ends "The cost is about two seconds a run", which `technical-writer-1` derived from `coder-1`'s 0.8 s, 0.2 s and 0.6 s. `CLAUDE.md` lists timings under "Never document", review 1 noted it, and `//tools/lint:markdown` cannot catch it.
- **The `realpath` comment does not match its code.** In both `swift_format_check.sh:6` and `swift_format_fix.sh:6` it says "`$0` is a bazel-bin symlink", while the line resolves `${BASH_SOURCE[0]}`, and the check script never runs under `bazel run`. The wording is plan 1's; review 1 left it.
- **Nothing guards the `external` tags.** Review 1 §3 records that deleting them fails no gate; the evidence that they work is `coder-1`'s two consecutive runs and the gate's `test.log`. A Bazel cache hit is invisible unless someone reads the summary, which the new `short` summary at least makes possible.

## Recommendations

1. **Remove or own `.claude/current-plan.md` and `.claude/verify-results.md`.** This came up again: it is recommendation 5 of [retro-fixes-4](retro-fixes-4.md). Here three of four invocations spent a grep and a report sentence on line 121's stale `--test_summary=terse`.
2. **Cut the timing and fix the `realpath` comment.** In `tools/lint/README.md` "Toolchains and binaries", end the `external` bullet at "a cached pass standing", or say "a few seconds" as plan 1 did. In `tools/lint/swift_format_check.sh:6` and `swift_format_fix.sh:6`, say "the script path" for `$0`, and drop `bazel run` from the check script's comment. Both are one-line edits that review 1 found and no agent of this task could make after approval.
