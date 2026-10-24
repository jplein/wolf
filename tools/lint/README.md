# tools/lint

Lint and format targets for the macOS Bazel host: clippy and rustfmt for Rust, swift-format for Swift, a markdown checker for the documentation rules in [CLAUDE.md](../../CLAUDE.md), and the comment checks of [doc-lint](../doc-lint/README.md).

Each tool has a check target (`bazel test`, reports and fails on violations) and, where a fixer exists, a fix target (`bazel run`, rewrites files in place under `$BUILD_WORKSPACE_DIRECTORY`).

These targets need Bazel and Xcode, neither of which the Linux devShell has. Linux runs the same checks directly in its verification gate — see [`build.md`](../../build.md#the-verification-gate).

## Policy: checks gate

Every check target runs under `bazel test //...`, and any finding fails it. clippy runs with `-Dwarnings` by default, set in `.bazelrc`, and runs as a build action of the `rust_clippy` targets, so a clippy warning fails `bazel build //...` as well. The fix targets are tagged `manual`, because they rewrite the developer's checkout and must run only when named by label.

## Toolchains and binaries

- clippy and rustfmt come from the toolchain `rules_rust` downloads (Rust 1.85.0); the Darwin devShell ships no standalone cargo/rustc/clippy/rustfmt.
- swift-format is the host Xcode 26 binary (v6.3.0), resolved at runtime via `xcrun --find swift-format`, with `xcode-select -p` and the canonical Xcode path as fallbacks, because Bazel test actions run with a scrubbed environment.
- The markdown checker is bash and awk only, so it needs no toolchain and runs sandboxed.
- doc-lint is built from `//tools/doc-lint` with plain `rules_rust` rules and has no dependencies.

## Targets

| Purpose               | Target                 | Command                                       |
| --------------------- | ---------------------- | --------------------------------------------- |
| clippy check          | `:clippy`              | `bazel test //tools/lint:clippy`              |
| clippy fix            | `:clippy.fix`          | `bazel run //tools/lint:clippy.fix`           |
| rustfmt check         | `:rustfmt`             | `bazel test //tools/lint:rustfmt`             |
| rustfmt fix           | `:rustfmt.fix`         | `bazel run //tools/lint:rustfmt.fix`          |
| swift-format check    | `:swift_format`        | `bazel test //tools/lint:swift_format`        |
| swift-format fix      | `:swift_format.fix`    | `bazel run //tools/lint:swift_format.fix`     |
| markdown check        | `:markdown`            | `bazel test //tools/lint:markdown`            |
| markdown checker test | `:markdown_check_test` | `bazel test //tools/lint:markdown_check_test` |
| comment check         | `:comments`            | `bazel test //tools/lint:comments`            |
| all checks            | `:check`               | `bazel test //tools/lint:check`               |
| all fixes             | `:fix`                 | `bazel run //tools/lint:fix`                  |

Internal targets: `:clippy_core`, `:clippy_ffi` and `:clippy_doc_lint` (`rust_clippy`, built as `data` of `:clippy` so a clippy failure fails that test), `:rustfmt_core`, `:rustfmt_ffi` and `:rustfmt_doc_lint` (`rustfmt_test`, aggregated by the `:rustfmt` and `:check` suites), `:all_markdown_srcs` and `:all_comment_srcs` (every package's filegroups), and this package's own `:markdown_srcs` and `:comment_srcs`.

## How each target works

- `:clippy` is a trivial `sh_test` (`clippy_gate.sh`, `exit 0`) whose `data` are the `rust_clippy` targets. Bazel must build them, which runs clippy, to stage runfiles, so a clippy failure fails the test.
- `:clippy.fix` is the custom `clippy_fix` rule (`clippy_fix.bzl`): it resolves `@rules_rust//rust:toolchain_type`, stages the toolchain sysroot (cargo/cargo-clippy/clippy-driver/rustc/rust_std) as runfiles, and runs `cargo clippy --fix --allow-dirty --allow-staged -p wolf-core -p wolf-ffi`.
- `:rustfmt` is a `test_suite` over the three `rustfmt_test` targets (`rustfmt --check` on each crate). `:rustfmt.fix` is an `alias` to the rules_rust writer `@rules_rust//:rustfmt`.
- `:swift_format` is an `sh_test` (`swift_format_check.sh`, tagged `no-sandbox`) that runs `swift-format lint --strict` over the `//crates/wolf-macos:swift_format_srcs` filegroup. `:swift_format.fix` is an `sh_binary` (`swift_format_fix.sh`) running `swift-format format -i`.
- `:markdown` is an `sh_test` (`markdown_check.sh`) over `:all_markdown_srcs`. It has no fixer, by design — see below.
- `:markdown_check_test` runs `markdown_check_test.sh`, which runs the checker over `testdata/markdown/` and asserts each case's exit status and per-rule finding counts.
- `:comments` is an `sh_test` (`doc_lint_check.sh`) that runs `doc-lint --files` over `:all_comment_srcs`. `:all_markdown_srcs` is staged as data too, so the doc paths written in comments resolve in the runfiles tree. The rules are in [doc-lint's README](../doc-lint/README.md#rules).
- `:fix` is the custom `fix_all` rule: it stages both the rust toolchain and the separate rules_rust rustfmt toolchain, then runs rustfmt, then `cargo clippy --fix`, then swift-format, all in place.

## Adding a package

Bazel forbids cross-package globbing, so every package collects its own files. A package with a `BUILD.bazel` loads `comment_srcs` from `//tools/lint:defs.bzl`, calls `comment_srcs()`, declares a `markdown_srcs` filegroup visible to this package, and is listed in both `:all_comment_srcs` and `:all_markdown_srcs`. doc-lint's `bazel-coverage` rule fails the Linux gate when a package is missing from either list.

`COMMENT_LINT_PATTERNS` in `defs.bzl` must name the same files as doc-lint's [language table](../doc-lint/README.md#languages). A file type missing from it is checked on Linux and silently skipped here.

## What `:markdown` checks

The documentation rules live in [CLAUDE.md](../../CLAUDE.md); this target enforces the half a machine can see.

| Rule           | Fails on                                                                                                                                                                 |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `long-line`    | A line over `WOLF_MD_MAX_LINE` (800) characters, excluding table rows.                                                                                                   |
| `changelog`    | Prose describing a delta from a past state rather than the present system. Files under a `notes/` directory are exempt, since a note may record how the code came to be. |
| `allcaps`      | An all-caps word that is not an allowlisted acronym and contains no digit.                                                                                               |
| `entry-budget` | A `####` per-file entry over `WOLF_MD_MAX_ENTRY_WORDS` (250) words.                                                                                                      |
| `notes-header` | A file under a `notes/` directory whose first line that is neither blank nor a heading is not ``Last verified at commit `<40-hex sha>`.``, or which has no such line.    |
| `big-file`     | A doc over `WOLF_MD_MAX_BYTES` (50000) bytes. Warning only, since a directory with many source files needs a longer README.                                              |

Text inside backticks and fenced code blocks is exempt from the `changelog` and `allcaps` rules, so a doc can name a banned word by writing it as code.

`long-line` exists because `.prettierrc` sets `proseWrap: never` for Markdown: a paragraph is stored as exactly one line, so line length is paragraph length, and prettier will never split it. That is also why there is no `:markdown.fix` — shortening a paragraph is an authorial act, not a formatting one.

The scope is every `.md` file except `CLAUDE.md`, which quotes the changelog words it bans, the agent definitions under `.claude/`, and the test fixtures. Here that is the union of every package's `markdown_srcs`; on Linux, `markdown_all.sh` runs the checker over every such file git knows of, tracked or untracked, skipping any deleted from the worktree.

### Checker tests

`markdown_check_test.sh` runs the checker over `testdata/markdown/`, passing paths as `./notes/…` so a `notes/` match anchored at the start of the path would fail it.

| Fixture                   | Expected                                                   |
| ------------------------- | ---------------------------------------------------------- |
| `notes/good.md`           | No findings: a valid header, and `now` in prose.           |
| `notes/missing.md`        | One `notes-header`, at line 3.                             |
| `notes/short-sha.md`      | One `notes-header`: a 7-digit SHA is not enough.           |
| `notes/no-h1.md`          | No findings: the header line is line 1.                    |
| `notes/nested/deep.md`    | No findings: a nested `notes/` file follows the same rule. |
| `notes/empty-after-h1.md` | One `notes-header` at line 1, reported when the file ends. |
| `other/changelog.md`      | One `changelog`: outside `notes/` the rule applies.        |
| `other/no-sha.md`         | No findings: outside `notes/` no header is needed.         |

## Caveats

- The fixers are non-hermetic. `cargo clippy --fix` reaches crates.io (network) and the host cc toolchain. Rust fixers are scoped to `-p wolf-core -p wolf-ffi`, because wolf-gnome's GTK stack does not build on macOS.
- swift-format scope. Only hand-written `wolf-macos` Swift is covered (Sources/WolfKeyMap, Sources/WolfMacOS, Tests/WolfKeyMapTests). The machine-generated swift-bridge bindings under `crates/wolf-ffi/generated/` are excluded. The file list lives in `//crates/wolf-macos:swift_format_srcs` because Bazel forbids cross-package globbing.
- swift-format check and fix enforce different things. `:swift_format` (lint) runs `swift-format lint --strict` and checks only swift-format's lint-category rules; `:swift_format.fix` (format) runs `swift-format format -i` and additionally restyles whitespace and indentation. A passing check does not mean the fixer would leave the files unchanged: the lint check can report zero findings on files that `format` would still restyle.
- The vendoring notes under `bazel/*` are in scope for style but carry no per-file entries, so only the line-length, changelog and all-caps rules bite there. `entry-budget` fires on `####` headings, which those files do not use.
- `.bazelignore`. The rustfmt writer (`:rustfmt.fix`) internally evaluates `//...`; the repo's `.bazelignore` excludes `.direnv` so the nix and direnv symlink tree does not break that query.

## Files

- `BUILD.bazel` — all targets.
- `defs.bzl` — `COMMENT_LINT_PATTERNS` and the `comment_srcs` macro every package calls.
- `clippy_fix.bzl` — the `clippy_fix` and `fix_all` toolchain-staging rules.
- `clippy_fix_launcher.sh.tpl` — launcher template for `clippy_fix`.
- `fix_all.sh` — launcher template for `fix_all` (the `:fix` aggregate).
- `clippy_gate.sh` — the `exit 0` body for the `:clippy` gate test.
- `swift_format_check.sh` — `swift-format lint --strict` over the passed files.
- `swift_format_fix.sh` — `swift-format format -i` over the wolf-macos sources.
- `markdown_check.sh` — the documentation rule checks over the passed files.
- `markdown_check_test.sh` — the `:markdown_check_test` body, also a Linux gate step.
- `markdown_all.sh` — Linux only: the checker over the Linux scope above. It calls the host `git`, which the devShell inherits from the host `PATH` because it is not a pure shell.
- `doc_lint_check.sh` — the `:comments` body: `doc-lint --files` over the files it is passed.
- `testdata/markdown/` — the fixtures for `markdown_check_test.sh`.
