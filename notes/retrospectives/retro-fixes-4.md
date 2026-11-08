# Retrospective: gate wolf-core's GUI-free dependency rule on both platforms

Last verified at commit `2c54fb3e55d1654a9947795f0ac7791d38b3f2cb`.

The task carried out recommendation 1 of [open-items-13](open-items-13.md). It added `tools/lint/wolf_core_forbidden_deps.txt`, the checker `tools/lint/wolf_core_deps_check.sh` with its test and fixtures, and a `wolf-core-deps` gate in `gates.sh` that feeds the checker `cargo tree` on Linux and `bazel query` on macOS. This task's commits are `d843298` and `2c54fb3`; the earlier commits in the range belong to `retro-fixes-1` to `retro-fixes-3`.

It ran once through the flow with no rework: no rejection, no failed gate, no discrepancy. Both gates passed on both workers on their first run, and review 1 approved with two minor notes. The five invocations cost about $7.42: `architect-1` $2.48 (24 turns, 9.4 minutes), `reviewer-1` $1.58, `technical-writer-1` $1.42, `coder-1` $1.28 and `test-writer-1` $0.66.

## What went well

- **The decisions note held the exact commands and their reasons.** `notes/wolf-core-decisions.md` gave both commands and why the `kind(rule, …)` wrapper is load-bearing (`libc`'s `glibc` source paths match `glib`). Plan 1 took the macOS query verbatim and seeded the list from the note's `grep -iE` stems, so no agent had to rediscover either.
- **`gates.sh`'s `run` helper took a new gate cheaply.** Under `set -o pipefail`, a shell function holding one pipeline gets the same log, `<gate> <rc>` line and failure excerpt as every other gate, and the checker's `FAIL:` prefix matched the existing excerpt pattern. `coder-1` added two functions and three `run` lines, 15 lines of `gates.sh` in all.
- **A sibling script test was a template.** `tools/lint/markdown_check_test.sh` gave the shape of `wolf_core_deps_check_test.sh`, and `CLAUDE.md`'s rule that a lint script needs both an `sh_test` and a Linux `run` line made plan 1 §3 name both. `test-writer-1` finished in 11 turns, and neither wiring was missed.
- **`regina on linux` let one invocation prove both platforms.** `coder-1` ran the six falsifiers, the `serde` falsifier on both workers and both full `./gates.sh` runs itself. Review 1 checked them against the command logs instead of running anything.
- **The doc-lint ordering was foreseen.** Plan 1 §1 told the coder to link only existing `notes/gates.sh.md` headings, because `md-anchor` runs at the implementation gate before the technical writer adds any. [open-items-8](open-items-8.md) recorded the same foresight, so the constraint is learnable from the docs.

## What did not go well

No step was redone. The cost was in finding every restatement of the rule's check, and two facts did not land where a later agent will look.

- **Where the rule is checked was restated in seven places.** `architect-1` grepped the repository to find the sentences the gate would make false: `crates/wolf-core/src/lib.rs`, `build.md`, `notes/wolf-gnome-decisions.md`, `notes/MODULE.bazel.md` and the two reviewer definitions, beside the decisions note itself. Plan 1 split those edits between the coder and the technical writer. The `lib.rs` sentence, "see the crate README", was already wrong at the base. All seven still say where the check lives, so the next change to the gate repeats the sweep.
- **The same reviewer bullet is kept in two definitions.** `.claude/agents/reviewer.md:57` and `.claude/skills/regina/agents/reviewer.md:86` carry the same sentence about the rule, and `coder-1` edited both identically in `d843298`.
- **A spec sentence pins the commands to a note, so they are now in two places.** `specs/architecture.md:15` says "Both verification commands … are in `wolf-core-decisions.md`". The task did not open the spec, so plan 1 §1 forbade the technical writer to drop the command strings from the note. The pipelines live in `gates.sh`, and their copies in the note's "The one hard rule" are checked by nothing.
- **A false-positive risk is recorded nowhere in the repository.** Review 1 found that `deps(//crates/wolf-core:wolf_core)` also lists rules_rust's own toolchain crates, such as `@@rules_rust++i2+rrc__serde_spanned-…`, which `coder-1`'s `serde` falsifier run showed. A toolchain bump that brings a label containing a stem such as `objc` would fail the gate on something that is not core's. The fact is only in the task's run evidence; `notes/gates.sh.md`'s `## macOS gates` does not mention it.
- **One link covers half its sentence.** `notes/wolf-core-decisions.md` line 11 says how each platform lists the graph is in the gates notes, but links only `gates.sh.md#linux-gates`, not `#macos-gates` (review 1 §8).
- **A tracked working file adds noise to searches.** `.claude/verify-results.md` still records a "GUI denylist" step from earlier work. `architect-1` read it while sweeping for restatements, and `technical-writer-1` had to decide that it is not documentation. Nobody owns it under `CLAUDE.md`'s rules.

## Recommendations

1. **Say where the rule is checked in one place, and link to it.** Let `notes/wolf-core-decisions.md` "The one hard rule" alone name the gate and the list. Cut the matching sentences in `crates/wolf-core/src/lib.rs`, `build.md` "The dependency boundary", `notes/wolf-gnome-decisions.md` and `notes/MODULE.bazel.md` to a link to that heading. `architect-1` spent much of its 9.4 minutes finding six such sentences, one of them already stale.
2. **Let `specs/architecture.md` point at the gate, and drop the command copies.** A task that opens the spec could have its line 15 say the rule is checked by `gates.sh`'s `wolf-core-deps` gate, linking the decisions note. The note could then drop the two command strings, leaving `gates.sh` and `notes/gates.sh.md` the only home of the pipelines. Plan 1 kept the copies only to keep that sentence true.
3. **Record the toolchain labels in the macOS query.** Add to `## macOS gates` of `notes/gates.sh.md` that the query follows rules_rust's exec and toolchain edges, so a match on a `rrc__` or similar toolchain label calls for narrowing the query, not editing the list. Review 1 is the evidence, and the next agent to see that failure would otherwise start from a red gate with no lead.
4. **Link both halves of the platform rationale.** In `notes/wolf-core-decisions.md` line 11, link `gates.sh.md#macos-gates` beside `#linux-gates`.
5. **Remove `.claude/current-plan.md` and `.claude/verify-results.md` from the tree, or give them an owner.** They are working files of an earlier `/orchestrate` run, last touched in `1b24259`. Every repository-wide grep for a rule's wording finds them, as `architect-1`'s did here.
