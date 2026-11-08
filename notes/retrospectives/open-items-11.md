# Retrospective: draw Neovim's undercurl as a wave on macOS

Last verified at commit `394b0c276e8858b2540653a165870d3efadbc4da`.

The task ran with no rework. Each agent ran once, in order: architect, test writer, coder, technical writer, reviewer. Both gates passed on both workers on their first run, and review 1 approved with minor notes. The five invocations cost about $5.67 in total: `architect-1` $1.50, `test-writer-1` $0.60, `coder-1` $1.11, `technical-writer-1` $1.14 and `reviewer-1` $1.32.

## What went well

- **The behaviour to test sat in a pure resolver.** `GlyphStyleResolver.resolve` lives in `WolfKeyMap`, apart from the AppKit renderer, and `GlyphStyleTests.swift` was already in the `srcs` of `//crates/wolf-macos:WolfKeyMapTests`. The test writer edited one file and no `BUILD.bazel`. `coder-1` ran six falsifiers, each a one-line edit to `GlyphStyle.swift` checked by `shasum`, at about 4 s per run.
- **Linux gave the rule to copy.** `glyph_decoration` in `crates/wolf-gnome/src/glyph_style.rs` already fixed the precedence when both bits are set, and the instructions pointed at it. Plan 1 took the rule from it, and review 1 checked the Swift against its three lines.
- **One distinctive word found every stale claim.** `architect-1` grepped the whole repository for `undercurl` once and listed ten places that said macOS folds undercurl into a straight line, in Swift, Rust, test messages and Markdown. Review 1 repeated the search and found none left but the ledger cell the rule keeps.
- **Linux checks ran before the gate.** `test-writer-1` and `coder-1` ran `cargo fmt --check`, the `glyph_style` tests and the Linux `./gates.sh` through `regina on linux`, so the text edits to `\`-continued Rust strings passed on the first gate run.

## What did not go well

- **The formatter fixer rewrote files outside the task, a sixth time, and twice in one task.** `test-writer-1` and `coder-1` each ran `bazel run //tools/lint:swift_format.fix`, which each time rewrote the same six Swift files, `AppearanceSeam.swift` and five tests, and each restored them from `HEAD`. [open-items-1](open-items-1.md), [open-items-4](open-items-4.md), [open-items-6](open-items-6.md), [open-items-7](open-items-7.md) and [open-items-10](open-items-10.md) report the same thing.
- **The lint gate passes on the files the fixer rewrites.** `coder-1` and review 1 both noted that `//tools/lint:check` passes while the fixer finds six files to restyle, and called it a follow-up. [open-items-5](open-items-5.md) already traced this to `//tools/lint:swift_format` linting nothing, and it is still so.
- **The `PaneContainerView.swift` entry was at its word budget a fourth time.** Plan 1 §5 found the entry in `crates/wolf-macos/Sources/WolfMacOS/README.md` at 250 words, the limit. `technical-writer-1` spent three Python passes and a `sed` to fit one bullet, cutting the Purpose sentence's link and the `rebind(tabpageHandle:)` line. [open-items-5](open-items-5.md), [open-items-9](open-items-9.md) and [open-items-10](open-items-10.md) hit the same entry.
- **wolf-gnome's tests and docs described macOS's code.** Of the ten stale claims, five were in `crates/wolf-gnome`: three assertion messages and a doc comment in `tests/paint_golden.rs`, and the `## Divergence from macOS` section and a test doc in `src/glyph_style.rs`, two of which quoted Swift's `hasUnderline = underline || undercurl`. So a macOS-only task changed a Linux test file and a Linux source file. This is the pattern of [open-items-4](open-items-4.md) and [open-items-5](open-items-5.md).
- **The ledger needed a script once more.** `technical-writer-1` rewrote the one row of `notes/macos-followups.md` in Python, by its index in the list of lines, as in every earlier task that edited the ledger.
- **Running one XCTest class is not documented.** `coder-1` searched `build.md` for a way to run one case, found none, and ran the whole 473-case target for each falsifier. In [open-items-5](open-items-5.md) a coder used `--test_filter=<Class>` on the same target, but neither `build.md` nor a README names it. The cost here was small.

## Recommendations

1. **Make `swift_format.fix` take paths, and make `:swift_format` lint the real files.** This came up again: it is recommendation 2 of [open-items-4](open-items-4.md), recommendation 1 of [open-items-5](open-items-5.md) and recommendation 1 of [open-items-10](open-items-10.md). In this task two agents restored the same six files after the same command.
2. **Split `PaneContainerView.swift` and its entry.** This came up again: it is recommendation 2 of [open-items-10](open-items-10.md) and recommendations 1 and 2 of [open-items-9](open-items-9.md). Plan 1 had to budget words for a one-bullet change.
3. **Stop wolf-gnome's tests and docs from describing macOS's code.** This came up again: it is recommendation 1 of [open-items-4](open-items-4.md) and recommendation 4 of [open-items-5](open-items-5.md). Assertion messages in `crates/wolf-gnome/tests/paint_golden.rs` should state what Linux draws, and let `notes/feat-list.md` hold the one cross-platform comparison.
4. **Change the ledger's format when it is pruned.** This came up again: it is recommendation 2 of [open-items-2](open-items-2.md).
5. **Name `--test_filter` for `WolfKeyMapTests` in `build.md`.** Add one line to the macOS test commands giving `bazel test //crates/wolf-macos:WolfKeyMapTests --test_filter=<Class>`. `coder-1` looked for it and ran the whole target six times instead. Each run was about 4 s, so this saves a search more than time.
