# Retrospective: port wolf-gnome's defensive arithmetic to `WolfKeyMap`, and close three ledger rows

Last verified at commit `06567b1a622c8bb4639984acb7da706cae70701a`.

The task gave `CellMetricsMath`, `StatusbarLayout`, `EditorInset`, `DrawerLayout` and `ScrollAccumulator` the guards their wolf-gnome ports already had, with XCTest cases, a drawer floor passed from `PaneContainerView`, and rewrites of the wolf-gnome comments that called those guards divergences from Swift. It also closed three rows of `notes/macos-followups.md`.

It took one round trip. `review-1` rejected the work for two stale sentences in `crates/wolf-gnome/src/lib.rs`, and plan 2, `coder-2`, `technical-writer-2` and `review-2` fixed and approved them. The nine invocations cost about $15.33. The first pass cost $12.72 (`architect-1` $3.66, `test-writer-1` $1.69, `coder-1` $2.50, `technical-writer-1` $2.37, `reviewer-1` $2.50), and the round trip $2.61 (`architect-2` $0.53, `coder-2` $0.31, `technical-writer-2` $0.31, `reviewer-2` $1.47), plus a second run of both gates on both workers.

## What went well

- The Linux references made the port cheap to plan and to check. Each wolf-gnome module carried its guard and a test table, so plan 1 could give the Swift rule and each expected literal, and `review-1` §1 checked `DrawerLayout` against `drawer_split` input by input. No invocation had to work out a rule for itself.
- All five test files were already in the `srcs` of `//crates/wolf-macos:WolfKeyMapTests`, so `crates/wolf-macos/BUILD.bazel` did not change and there was nothing to wire (plan 1 §3, `review-1` §3).
- `WolfKeyMap` holds pure value seams, so each guard had a one-line falsifier and a fast red. `coder-1` ran F1 to F16 through one scratch script in the background, with restore checksums, and both reviews took that evidence as sufficient.
- The implementation and documentation gates passed on the first run, on both workers, in both rounds. `coder-1` ran `cargo fmt`, `clippy` and the doctests on the Linux worker before handing off, so the comment-only Rust edits cost no gate failure.

## What did not go well

- **The one rejection came from a fact kept in five places.** The number of divergences from Swift in `cell_metrics_math.rs` and `drawer.rs` was written in each module's docs, its test docs, its entry in `crates/wolf-gnome/src/README.md`, `notes/wolf-gnome-decisions.md` and the crate-root module list in `lib.rs`. Plan 1 §3 listed the places to rewrite in the modules, the README and the note, and missed the two in `lib.rs`, so `coder-1` and `technical-writer-1` missed them too. `review-1` §5 found them, and the fix cost the $2.61 round trip.
- **The searches that should have found them could not.** `architect-1` grepped for `divergence` only in the five module files and in `*.md`. Its wider grep, over all of `crates/wolf-gnome`, looked for `two divergences` and `three divergences`, which do not match the `two deliberate divergences` and `three deliberate divergences` that `lib.rs` said. `technical-writer-1` grepped only `src/README.md`, and its report said no source comment was stale.
- **The `lib.rs` module list is a second document for every module.** Most of its 39 bullets, in about 370 lines of `//!`, describe one file of `crates/wolf-gnome/src/`, as that directory's `README.md` does. `CLAUDE.md` says that exactly one document describes each file. The two that disagreed in this task are where a later edit to a module will drift again.
- **The swift-format fixer has no per-file mode.** `test-writer-1` ran `bazel run //tools/lint:swift_format.fix`, which formats all 130 Swift files, and it rewrote six files outside the task, `AppearanceSeam.swift` among them. Its first two attempts to reverse exactly those edits failed with `No valid patches in input`, and it needed about seven commands to get back to its five files. `tools/lint/README.md` says the fixer runs over all the `wolf-macos` sources, and there is no way to run it on fewer.
- **The checker could not be reproduced by hand.** `test-writer-1` then ran `swift-format lint --strict` on a base file, `AppearanceSeamTests.swift`, and got `Indentation` errors, while `//tools/lint:swift_format` passed on the same tree. It settled the question by running the Bazel target, but `tools/lint/README.md` does not say how to check one file the way the gate does.
- **The ledger cost another hand-written parser.** `technical-writer-1` matched the three rows with a script, matched every `typography` and `terminal-drawer` row, and rebuilt the file from `HEAD` with a stricter match. This is the third task of this request in which that happened.

## Recommendations

1. **Make `lib.rs`'s module list say less.** In `crates/wolf-gnome/src/lib.rs`, cut each module bullet to its link and a clause, and leave each module's decisions to `crates/wolf-gnome/src/README.md` and the module's own docs. At the least, drop counts such as "one deliberate divergence" and say "the divergences named in that module's docs". Either one would have kept `lib.rs` out of plan 1's edit list and saved the $2.61 round trip.
2. **Let `swift_format.fix` take files.** In `tools/lint/swift_format_fix.sh`, format only the paths given as arguments when there are any, and all files when there are none, and say so in the fixer's row of `tools/lint/README.md`. `test-writer-1` would then have formatted its five files and reverted nothing. The README could also give the command that checks one file as `:swift_format` does.
3. **Change the ledger's format when it is pruned.** This came up again: it is recommendation 2 of [open-items-2](open-items-2.md), repeated in [open-items-3](open-items-3.md). `technical-writer-1` wrote a parser for three rows, and its first match edited the wrong ones.
