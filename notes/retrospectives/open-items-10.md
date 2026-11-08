# Retrospective: make Rust the only renderer of the macOS layout-rebuild spec

Last verified at commit `721afd75b9554242c5862d1f6022d43f17806159`.

The task ran with no rework. Each agent ran once, in order: architect, test writer, coder, technical writer, reviewer. Both gates passed on both workers on their first run, and review 1 approved with minor notes. The five invocations cost about $9.89 in total: `architect-1` $2.92, `test-writer-1` $1.49, `coder-1` $1.35, `technical-writer-1` $1.57 and `reviewer-1` $2.56.

## What went well

- **The restore path hands its tree on unchanged, and says so.** `PlannedTab.layout` is documented as a verbatim passthrough, and nothing between `AppDelegate.restoredGroups` and `rebuildLayout(from:)` edits the tree. Plan 1 could therefore settle the task's `failed` exit (does Swift change the tree?) by reading, and review 1 confirmed the same chain without finding a gap.
- **The wolf-ffi test kit covered every step of the new test.** `crates/wolf-ffi/tests/state.rs` already had the push-builder and the `wolf_ffi_load_state()` poll, `common::nvim_bin_or_skip` set up the state dir, and the `unicode_key` target in `crates/wolf-ffi/BUILD.bazel` was a real-nvim sibling to copy. `test-writer-1` wrote `crates/wolf-ffi/tests/rebuild_layout.rs` in one pass, and its only build error was the expected one, the old `String` signature.
- **One grep token defined "done" for the documents.** The instructions made `git grep specJSON` the check. `coder-1` used it to hand the technical writer a list of nine stale documents with line numbers, and `technical-writer-1` and review 1 each closed with the same grep returning nothing.
- **The bindings regenerate with one command.** `build.md`'s `bazel run //crates/wolf-ffi:gen_bindings` rewrote only `wolf-ffi.h` and `wolf-ffi.swift`, and `coder-1` hand-edited neither.
- **Linux checks ran before the gate.** `coder-1` ran `cargo clippy`, `cargo fmt --check` and the new test on the Linux worker through `regina on linux`, so `./gates.sh` passed on both workers on its first run.

## What did not go well

- **The formatter fix targets rewrote files outside the task, a fifth time.** `test-writer-1` ran `bazel run //tools/lint:rustfmt.fix` and `//tools/lint:swift_format.fix`, which rewrote the three doc-lint fixtures under `tools/doc-lint/tests/fixtures/lang/rust/`, `AppearanceSeam.swift` and four Swift test files. It restored them with a `git show HEAD:<path>` loop, which also reset its own edit to `PaneLayoutCodecTests.swift`. [open-items-1](open-items-1.md), [open-items-4](open-items-4.md), [open-items-6](open-items-6.md) and [open-items-7](open-items-7.md) report the same thing.
- **The `PaneContainerView.swift` entry ran over its word budget a third time.** One renamed signature, `requestLayoutRebuild(_:node:)`, took the entry in `crates/wolf-macos/Sources/WolfMacOS/README.md` to 262 of 250 words. `technical-writer-1` cut the `activePreviewSourcePath` and `activeWin` bullets to fit. Review 1 flagged the new clause as unclear, because it names `saved`, an unlabeled parameter, to save words.
- **The defects that [open-items-9](open-items-9.md) found in that entry are still there.** At `721afd7` the entry still names `geometry(_:fullWidth:)` for `geometry(for:fullWidth:)`, and the `activeWin` bullet still lacks "Never". This task rewrote both neighbouring bullets and fixed neither, because plan 1 did not name them.
- **The bridge node travels beside the restore plan.** `RestoreSeam` in `WolfKeyMap` cannot hold an `FfiPaneNode`, so the restore keeps two trees: the `SavedPaneNode` projection, which goes through the plan, and the bridge node, which `materializeProjects(_:groups:)` finds again by `(groupIndex, positionInGroup)`. Plan 1 traced nine steps to show the two stay paired, and review 1 rated that lookup "compile-verified only", with no automated test.
- **The ledger needed a script once more.** `technical-writer-1` rewrote one row of `notes/macos-followups.md` with Python, finding it by line index, as in every earlier task that edited the ledger.
- **The macOS gate log did not show that the new target ran.** `coder-1` ran three greps on `target/gates/test.log` for `//crates/wolf-ffi:rebuild_layout`, found only totals, and showed that the test ran from `bazel-testlogs` instead.

## Recommendations

1. **Keep the fix targets to the files given.** This came up again: it is recommendation 4 of [open-items-1](open-items-1.md) (exclude `tools/doc-lint/tests/fixtures/` from `:rustfmt.fix`) and recommendation 2 of [open-items-4](open-items-4.md) (let `swift_format.fix` take paths). This task hit both targets in one run, and it is the fifth task to restore files after them.
2. **Split `PaneContainerView.swift`, and fix its entry's two known defects.** This came up again: these are recommendations 1 and 2 of [open-items-9](open-items-9.md), and [open-items-5](open-items-5.md) hit the same budget. In this task a one-clause signature change forced cuts to two unrelated bullets.
3. **Change the ledger's format when it is pruned.** This came up again: it is recommendation 2 of [open-items-2](open-items-2.md).
4. **List each test target's result in the macOS gate log.** This came up again: it is recommendation 5 of [open-items-9](open-items-9.md). `coder-1` spent three commands looking for a target in a log that holds only totals.
5. **Let `RestoreSeam` carry an opaque per-tab payload.** Make `RestoreSeam.SavedTab` and `PlannedTab` generic over a payload, or give them an `AnyObject?` field, so that `AppDelegate` passes the `FfiPaneNode` through the plan. Then `materializeProjects` needs neither its `groups` parameter nor the index lookup, and a `WolfKeyMapTests` case could check that each tab's payload reaches the planned tab. Plan 1's nine-step trace and review 1's untested lookup are the evidence.
