# tools/lint

Advisory (non-gating) lint/format targets for the macOS Bazel host: clippy and rustfmt for Rust, swift-format for Swift, and a markdown checker for the documentation rules in [CLAUDE.md](../../CLAUDE.md).

Each tool has a check target (`bazel test`, reports and fails on violations) and, where a fixer exists, a fix target (`bazel run`, rewrites files in place under `$BUILD_WORKSPACE_DIRECTORY`).

These targets need Bazel and Xcode, neither of which the Linux devShell has. Linux runs its own equivalents directly, and gates on them rather than treating them as advisory — see [`build.md`](../../build.md).

## Policy: advisory, not gating

Every target here is tagged `manual`, so no wildcard build or test (`bazel test //crates/...`) sweeps it in and the default suite is unaffected. Only running these targets deliberately, by label, exercises them.

Existing lint violations across the codebase are expected and out of scope. A check target "passing verification" means the linter ran on the host without a Bazel or toolchain error; it may still legitimately report findings, which fail the target.

## Toolchains and binaries

- clippy and rustfmt come from the toolchain `rules_rust` downloads (Rust 1.85.0); the Darwin devShell ships no standalone cargo/rustc/clippy/rustfmt.
- swift-format is the host Xcode 26 binary (v6.3.0), resolved at runtime via `xcrun --find swift-format`, with `xcode-select -p` and the canonical Xcode path as fallbacks, because Bazel test actions run with a scrubbed environment.
- The markdown checker is bash and awk only, so it needs no toolchain and runs sandboxed.

## Targets

| Purpose            | Target              | Command                                   |
| ------------------ | ------------------- | ----------------------------------------- |
| clippy check       | `:clippy`           | `bazel test //tools/lint:clippy`          |
| clippy fix         | `:clippy.fix`       | `bazel run //tools/lint:clippy.fix`       |
| rustfmt check      | `:rustfmt`          | `bazel test //tools/lint:rustfmt`         |
| rustfmt fix        | `:rustfmt.fix`      | `bazel run //tools/lint:rustfmt.fix`      |
| swift-format check | `:swift_format`     | `bazel test //tools/lint:swift_format`    |
| swift-format fix   | `:swift_format.fix` | `bazel run //tools/lint:swift_format.fix` |
| markdown check     | `:markdown`         | `bazel test //tools/lint:markdown`        |
| all checks         | `:check`            | `bazel test //tools/lint:check`           |
| all fixes          | `:fix`              | `bazel run //tools/lint:fix`              |

Internal targets: `:clippy_core` / `:clippy_ffi` (`rust_clippy`, built as `data` of `:clippy` so a clippy failure fails that test), `:rustfmt_core` / `:rustfmt_ffi` (`rustfmt_test`, aggregated by the `:rustfmt` and `:check` suites), and `:markdown_srcs` (the doc file list).

## How each target works

- `:clippy` is a trivial `sh_test` (`clippy_gate.sh`, `exit 0`) whose `data` are the `rust_clippy` targets `:clippy_core` / `:clippy_ffi`. Bazel must build them, which runs clippy, to stage runfiles, so a clippy failure fails the test.
- `:clippy.fix` is the custom `clippy_fix` rule (`clippy_fix.bzl`): it resolves `@rules_rust//rust:toolchain_type`, stages the toolchain sysroot (cargo/cargo-clippy/clippy-driver/rustc/rust_std) as runfiles, and runs `cargo clippy --fix --allow-dirty --allow-staged -p wolf-core -p wolf-ffi`.
- `:rustfmt` is a `test_suite` over two `rustfmt_test` targets (`rustfmt --check` on each crate). `:rustfmt.fix` is an `alias` to the rules_rust writer `@rules_rust//:rustfmt`.
- `:swift_format` is an `sh_test` (`swift_format_check.sh`, tagged `manual` and `no-sandbox`) that runs `swift-format lint --strict` over the `//crates/wolf-macos:swift_format_srcs` filegroup. `:swift_format.fix` is an `sh_binary` (`swift_format_fix.sh`) running `swift-format format -i`.
- `:markdown` is an `sh_test` (`markdown_check.sh`) over the `:markdown_srcs` filegroup. It has no fixer, by design — see below.
- `:fix` is the custom `fix_all` rule: it stages both the rust toolchain and the separate rules_rust rustfmt toolchain, then runs rustfmt, then `cargo clippy --fix`, then swift-format, all in place.

## What `:markdown` checks

The documentation rules live in [CLAUDE.md](../../CLAUDE.md); this target enforces the half a machine can see.

| Rule           | Fails on                                                                                                                    |
| -------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `long-line`    | A line over `WOLF_MD_MAX_LINE` (800) characters, excluding table rows.                                                      |
| `changelog`    | Prose describing a delta from a past state rather than the present system.                                                  |
| `allcaps`      | An all-caps word that is not an allowlisted acronym and contains no digit.                                                  |
| `entry-budget` | A `####` per-file entry over `WOLF_MD_MAX_ENTRY_WORDS` (250) words.                                                         |
| `big-file`     | A doc over `WOLF_MD_MAX_BYTES` (50000) bytes. Warning only, since a directory with many source files needs a longer README. |

Text inside backticks and fenced code blocks is exempt from the `changelog` and `allcaps` rules, so a doc can name a banned word by writing it as code.

`long-line` exists because `.prettierrc` sets `proseWrap: never` for Markdown: a paragraph is stored as exactly one line, so line length is paragraph length, and prettier will never split it. That is also why there is no `:markdown.fix` — shortening a paragraph is an authorial act, not a formatting one.

## Caveats

- clippy fail-on-lint is opt-in. By default the clippy check reports findings; `--config=lint` (in `.bazelrc`) adds `-Dwarnings` so it hard-fails: `bazel test --config=lint //tools/lint:clippy`. It is not the default so the standard build and test path is byte-for-byte unchanged.
- The fixers are non-hermetic. `cargo clippy --fix` reaches crates.io (network) and the host cc toolchain. Rust fixers are scoped to `-p wolf-core -p wolf-ffi`, because wolf-gnome's GTK stack does not build on macOS.
- swift-format scope. Only hand-written `wolf-macos` Swift is covered (Sources/WolfKeyMap, Sources/WolfMacOS, Tests/WolfKeyMapTests). The machine-generated swift-bridge bindings under `crates/wolf-ffi/generated/` are excluded. The file list lives in `//crates/wolf-macos:swift_format_srcs` because Bazel forbids cross-package globbing.
- swift-format check and fix enforce different things. `:swift_format` (lint) runs `swift-format lint --strict` and checks only swift-format's lint-category rules; `:swift_format.fix` (format) runs `swift-format format -i` and additionally restyles whitespace and indentation. A passing check does not mean the fixer would leave the files unchanged: the lint check can report zero findings on files that `format` would still restyle.
- markdown scope. `:markdown_srcs` collects the `README.md` and `DECISIONS.md` docs from each package's own `markdown_srcs` filegroup, again because Bazel forbids cross-package globbing. Adding docs to a new package means adding a filegroup there and listing it in `:markdown_srcs`.
- The workspace docs `architecture.md`, `build.md` and `command-lifecycle.md` are covered, listed one by one in the root filegroup. Most other root-level Markdown is deliberately excluded: `CLAUDE.md` quotes the banned changelog words as rules and would flag itself, and the plan and decision reports are historical artifacts that legitimately describe past states.
- The vendoring notes under `bazel/*` are in scope for style but carry no per-file entries, so only the line-length, changelog and all-caps rules bite there. `entry-budget` fires on `####` headings, which those files do not use.
- `.bazelignore`. The rustfmt writer (`:rustfmt.fix`) internally evaluates `//...`; the repo's `.bazelignore` excludes `.direnv` so the nix and direnv symlink tree does not break that query.

## Files

- `BUILD.bazel` — all targets.
- `clippy_fix.bzl` — the `clippy_fix` and `fix_all` toolchain-staging rules.
- `clippy_fix_launcher.sh.tpl` — launcher template for `clippy_fix`.
- `fix_all.sh` — launcher template for `fix_all` (the `:fix` aggregate).
- `clippy_gate.sh` — the `exit 0` body for the `:clippy` gate test.
- `swift_format_check.sh` — `swift-format lint --strict` over the passed files.
- `swift_format_fix.sh` — `swift-format format -i` over the wolf-macos sources.
- `markdown_check.sh` — the documentation rule checks over the passed files.
