# tools/lint

Lint and format targets for the macOS Bazel host: clippy and rustfmt for Rust, swift-format for Swift, a markdown checker for the documentation rules in [CLAUDE.md](../../CLAUDE.md), a check that wolf-gnome's sources name macOS only at an accepted divergence, and the comment checks of [doc-lint](../doc-lint/README.md).

Each tool has a check target (`bazel test`, reports and fails on violations) and, where a fixer exists, a fix target (`bazel run`, rewrites files in place under `$BUILD_WORKSPACE_DIRECTORY`).

These targets need Bazel and Xcode, neither of which the Linux devShell has. Linux runs the same checks directly in its verification gate — see [`build.md`](../../build.md#the-verification-gate). The wolf-core dependency checker has no target of its own: the `wolf-core-deps` gate of [`gates.sh`](../../gates.sh) runs it on both platforms, and only its test is a target.

## Policy: checks gate

Every check target runs under `bazel test //...`, and any finding fails it. clippy runs with `-Dwarnings` by default, set in `.bazelrc`, and runs as a build action of the `rust_clippy` targets, so a clippy warning fails `bazel build //...` as well. The fix targets are tagged `manual`, because they rewrite the developer's checkout and must run only when named by label.

## Toolchains and binaries

- clippy comes from the toolchain `rules_rust` downloads (Rust 1.85.0), and rustfmt from its separate rustfmt toolchain, a nightly that reports `rustfmt 1.9.0-nightly (57d06900fd 2026-05-27)`. The Darwin devShell ships no standalone cargo/rustc/clippy/rustfmt.
- `:fix` and `:rustfmt_files.fix` run that rustfmt toolchain's binary, which `:rustfmt_files.fix` stages through `@rules_rust//rust/toolchain:current_rustfmt_toolchain`. The `rust_toolchain`'s own `rustfmt` aborts at load time, unable to find its `librustc_driver`, and `@rules_rust//tools/upstream_wrapper:rustfmt` passes no `--edition`, so rustfmt's default of 2015 rejects `async fn`.
- swift-format is the host Xcode binary, resolved at runtime by `find_swift_format` in `swift_format_lib.sh`: `xcrun --find swift-format`, then `xcode-select -p`, then the canonical Xcode path, because Bazel test actions run with a scrubbed environment. `swift_format_check.sh` and `swift_format_fix.sh` source it, and `:fix` reaches it through `:swift_format.fix`.
- The host swift-format is not a Bazel input, so `:swift_format`, `:swift_format_check_test` and `:swift_format_fix_test` are tagged `external`, and Bazel reruns them on every `bazel test`. Without the tag an Xcode upgrade would leave a cached pass standing. The cost is about two seconds a run.
- The markdown checker, the wolf-core dependency checker and the wolf-gnome wording checker are bash, awk and grep only, so they need no toolchain and run sandboxed.
- doc-lint is built from `//tools/doc-lint` with plain `rules_rust` rules and has no dependencies.

## Targets

| Purpose                         | Target                           | Command                                                            |
| ------------------------------- | -------------------------------- | ------------------------------------------------------------------ |
| clippy check                    | `:clippy`                        | `bazel test //tools/lint:clippy`                                   |
| clippy fix                      | `:clippy.fix`                    | `bazel run //tools/lint:clippy.fix`                                |
| rustfmt check                   | `:rustfmt`                       | `bazel test //tools/lint:rustfmt`                                  |
| rustfmt fix                     | `:rustfmt.fix`                   | `bazel run //tools/lint:rustfmt.fix [-- <target pattern>…]`        |
| rustfmt_files fix               | `:rustfmt_files.fix`             | `bazel run //tools/lint:rustfmt_files.fix -- [--check] <file.rs>…` |
| rustfmt fixer test              | `:rustfmt_files_test`            | `bazel test //tools/lint:rustfmt_files_test`                       |
| swift-format check              | `:swift_format`                  | `bazel test //tools/lint:swift_format`                             |
| swift-format fix                | `:swift_format.fix`              | `bazel run //tools/lint:swift_format.fix [-- <file.swift>…]`       |
| markdown check                  | `:markdown`                      | `bazel test //tools/lint:markdown`                                 |
| markdown checker test           | `:markdown_check_test`           | `bazel test //tools/lint:markdown_check_test`                      |
| swift-format checker test       | `:swift_format_check_test`       | `bazel test //tools/lint:swift_format_check_test`                  |
| swift-format fixer test         | `:swift_format_fix_test`         | `bazel test //tools/lint:swift_format_fix_test`                    |
| wolf-core deps checker test     | `:wolf_core_deps_check_test`     | `bazel test //tools/lint:wolf_core_deps_check_test`                |
| wolf-gnome wording check        | `:wolf_gnome_wording`            | `bazel test //tools/lint:wolf_gnome_wording`                       |
| wolf-gnome wording checker test | `:wolf_gnome_wording_check_test` | `bazel test //tools/lint:wolf_gnome_wording_check_test`            |
| comment check                   | `:comments`                      | `bazel test //tools/lint:comments`                                 |
| all checks                      | `:check`                         | `bazel test //tools/lint:check`                                    |
| all fixes                       | `:fix`                           | `bazel run //tools/lint:fix`                                       |

Internal targets: `:clippy_core`, `:clippy_ffi`, `:clippy_test_support`, `:clippy_vt` and `:clippy_doc_lint` (`rust_clippy`, built as `data` of `:clippy` so a clippy failure fails that test), `:rustfmt_core`, `:rustfmt_ffi`, `:rustfmt_test_support`, `:rustfmt_vt` and `:rustfmt_doc_lint` (`rustfmt_test`, aggregated by the `:rustfmt` and `:check` suites), `:all_markdown_srcs` and `:all_comment_srcs` (every package's filegroups), and this package's own `:markdown_srcs` and `:comment_srcs`.

## How each target works

- `:clippy` is a trivial `sh_test` (`clippy_gate.sh`, `exit 0`) whose `data` are the `rust_clippy` targets. Bazel must build them, which runs clippy, to stage runfiles, so a clippy failure fails the test.
- `:clippy.fix` is the custom `clippy_fix` rule (`clippy_fix.bzl`): it resolves `@rules_rust//rust:toolchain_type`, stages the toolchain sysroot (cargo/cargo-clippy/clippy-driver/rustc/rust_std) as runfiles, and runs `cargo clippy --fix --allow-dirty --allow-staged -p wolf-core -p wolf-ffi`. It also stages `//bazel/ghostty-vt:libghostty_vt`, for the reason under [Caveats](#caveats).
- `:rustfmt` is a `test_suite` over the `rustfmt_test` targets (`rustfmt --check` on each crate).
- `:rustfmt.fix` is an `alias` to the rules_rust writer `@rules_rust//:rustfmt`, which formats every `.rs` source file that a Rust target in `//...` lists directly, `data` included, and skips targets tagged `no-format`, `no-rustfmt` or `norustfmt`. Target patterns given as arguments narrow that scope. It looks one dependency level deep, so a file reached through a filegroup is out of its reach: a package whose Rust tests read `.rs` fixtures stages them that way, as [doc-lint does](../doc-lint/tests/README.md#fixtures).

  No gate sees the writer reach those fixtures, so after a `rules_rust` bump, rerun this query and confirm that it prints `0`, meaning no `tools/doc-lint/tests/fixtures/` path appears:

  ```sh
  bazel query 'filter("^//.*\.rs$", kind("source file", deps(attr(edition, ".*", //...), 1)))' \
    | grep -c 'tools/doc-lint/tests/fixtures/'
  ```

  It is a superset of the writer's own query at the same `deps(…, 1)` depth: the writer, `tools/rustfmt/src/main.rs` in rules_rust, queries each edition and the unset one, and drops the tagged targets above and those with a `crate` attribute. If a bump changes that depth or target set, run the changed form instead.

- `:rustfmt_files.fix` is an `sh_binary` (`rustfmt_files.sh`) that runs rustfmt over the `.rs` files given, outside any Bazel target, so it reaches `crates/wolf-gnome/`, which no target builds. Each path is absolute or relative to the checkout root, never to the shell's cwd. Each must be a regular `.rs` file under the checkout, and one refusal exits 1 before any file is formatted; no file at all, or no `BUILD_WORKSPACE_DIRECTORY`, exits 2. `--check` reports without writing, with rustfmt's own exit status.

  It passes `--edition 2021`, as `fix_all.sh` does and as the workspace's `Cargo.toml` sets. rustfmt also formats the out-of-line modules a given file declares, so naming `lib.rs` reaches every `mod foo;` below it.

- `:rustfmt_files_test` runs `rustfmt_files_test.sh` over a scratch checkout of `testdata/rustfmt_files/` copies: a relative and an absolute argument each format only that file into `formatted.rs`, `--check` leaves a file unchanged and exits 1 only on a misformatted one, each bad argument after a good one exits 1 and formats nothing, and a missing file list, rustfmt or `BUILD_WORKSPACE_DIRECTORY` exits 2. It is sandboxed and also the Linux gate step `rustfmt-files-test`, where it runs the devShell's `rustfmt` from `PATH`.
- `:swift_format` is an `sh_test` (`swift_format_check.sh`, tagged `external` and `no-sandbox`) that runs `swift-format lint --strict --follow-symlinks` over the `//crates/wolf-macos:swift_format_srcs` filegroup. The inputs arrive as runfiles symlinks, and swift-format silently skips a symlink argument without that flag, so the check would lint nothing and pass.
- `:swift_format.fix` is an `sh_binary` (`swift_format_fix.sh`) running `swift-format format -i` over the paths given as arguments, each relative to the checkout root rather than the shell's cwd, or absolute. Each must be a `.swift` file under the checkout, and one refusal exits 1 before any file is formatted. With no arguments it formats every wolf-macos source and test.
- `:markdown` is an `sh_test` (`markdown_check.sh`) over `:all_markdown_srcs`. It has no fixer, by design — see below.
- `:markdown_check_test` runs `markdown_check_test.sh`, which runs the checker over `testdata/markdown/` and asserts each case's exit status and per-rule finding counts.
- `:swift_format_check_test` runs `swift_format_check_test.sh`, which runs the checker over `testdata/swift_format/` staged as runfiles symlinks: `misindented.swift` must fail with `Indentation` findings, `formatted.swift` must pass, and both together must fail on the misindented file alone. It is `external`, `no-sandbox` and macOS only, like `:swift_format`, and has no Linux `run` line in [`gates.sh`](../../gates.sh), because Linux has neither Bazel nor swift-format.
- `:swift_format_fix_test` runs `swift_format_fix_test.sh`, which runs the fixer over a scratch checkout of `testdata/swift_format/` copies: a relative and an absolute argument each format only that file, no argument formats only the wolf-macos files, and each bad argument (not Swift, a directory, missing, or outside the checkout) placed after a good one exits non-zero, names the bad argument and formats nothing. It is `external`, `no-sandbox` and macOS only, with no Linux `run` line, for the same reason as `:swift_format_check_test`.
- `:wolf_core_deps_check_test` runs `wolf_core_deps_check_test.sh`, which runs the checker with the real forbidden list over `testdata/wolf_core_deps/`, each fixture as an argument and on stdin. `clean.txt` must pass with every line counted, `forbidden.txt` must fail naming `gtk4` and notify 8.2.1's `objc2-core-foundation` and none of the clean names, and an empty stdin or a missing file must exit 2. It is sandboxed and also a Linux gate step.
- `:wolf_gnome_wording` is an `sh_test` (`wolf_gnome_wording_check.sh`) over the root package's `//:wolf_gnome_srcs` filegroup, since wolf-gnome has no Bazel package, and `wolf_gnome_wording_allowlist.txt`. The rule is [below](#what-wolf_gnome_wording-checks). On Linux the same script runs as the `wolf-gnome-wording` gate, over the checkout.
- `:wolf_gnome_wording_check_test` runs `wolf_gnome_wording_check_test.sh` over the fake checkouts in `testdata/wolf_gnome_wording/`, with the cases [below](#checker-cases). It is sandboxed and also the Linux gate step `wolf-gnome-wording-check-test`.
- `:comments` is an `sh_test` (`doc_lint_check.sh`) that runs `doc-lint --files` over `:all_comment_srcs`. `:all_markdown_srcs` is staged as data too, so the doc paths written in comments resolve in the runfiles tree. The rules are in [doc-lint's README](../doc-lint/README.md#rules).
- `:fix` is the custom `fix_all` rule: it stages the rust toolchain, the separate rules_rust rustfmt toolchain, `:swift_format.fix` and the libghostty-vt archive, then runs rustfmt over `wolf-core`, `wolf-ffi`, `wolf-test-support` and `wolf-vt`, then `cargo clippy --fix`, then that fixer with no arguments, which formats every wolf-macos source and test, all in place.

## Adding a package

Bazel forbids cross-package globbing, so every package collects its own files. A package with a `BUILD.bazel` loads `comment_srcs` from `//tools/lint:defs.bzl`, calls `comment_srcs()`, declares a `markdown_srcs` filegroup visible to this package, and is listed in both `:all_comment_srcs` and `:all_markdown_srcs`. doc-lint's `bazel-coverage` rule fails the `doc-lint` gate of [`gates.sh`](../../gates.sh) when a package is missing from either list.

`COMMENT_LINT_PATTERNS` in `defs.bzl` must name the same files as doc-lint's [language table](../doc-lint/README.md#languages). A file type missing from it is silently skipped by `:comments`, so `bazel test //...` passes and only the `--root` run of `gates.sh`'s `doc-lint` gate catches it.

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

`markdown_check.sh --entry-words <files>` prints one `<file>:<line>: <words>/<budget> words: <heading>` line per `####` entry, in file order, and nothing else. It counts as `entry-budget` does: the whitespace-separated words of every line after the heading, inline code and link targets included and fenced code blocks excluded, up to the next heading of level 1 to 4 or the end of the file. It runs no rule and exits 0 even for an entry over budget, so it is how to see how close an entry is before the check fails it.

`long-line` exists because `.prettierrc` sets `proseWrap: never` for Markdown: a paragraph is stored as exactly one line, so line length is paragraph length, and prettier will never split it. That is also why there is no `:markdown.fix` — shortening a paragraph is an authorial act, not a formatting one.

The scope is every `.md` file except `CLAUDE.md`, which quotes the changelog words it bans, the agent definitions under `.claude/`, and the test fixtures. Here that is the union of every package's `markdown_srcs`; on Linux, `markdown_all.sh` runs the checker over every such file git knows of, tracked or untracked, skipping any deleted from the worktree.

### Checker tests

`markdown_check_test.sh` runs the checker over `testdata/markdown/`, passing paths as `./notes/…` so a `notes/` match anchored at the start of the path would fail it.

| Fixture                   | Expected                                                                                                                                                                                           |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `notes/good.md`           | No findings: a valid header, and `now` in prose.                                                                                                                                                   |
| `notes/missing.md`        | One `notes-header`, at line 3.                                                                                                                                                                     |
| `notes/short-sha.md`      | One `notes-header`: a 7-digit SHA is not enough.                                                                                                                                                   |
| `notes/no-h1.md`          | No findings: the header line is line 1.                                                                                                                                                            |
| `notes/nested/deep.md`    | No findings: a nested `notes/` file follows the same rule.                                                                                                                                         |
| `notes/empty-after-h1.md` | One `notes-header` at line 1, reported when the file ends.                                                                                                                                         |
| `other/changelog.md`      | One `changelog`: outside `notes/` the rule applies.                                                                                                                                                |
| `other/no-sha.md`         | No findings: outside `notes/` no header is needed.                                                                                                                                                 |
| `other/entries.md`        | The `--entry-words` cases: each count printed, a fenced `####` line not an entry, a `###` heading ending one, and the count matching the `entry-budget` finding at a budget one below the largest. |

## What `:wolf_gnome_wording` checks

wolf-gnome's sources name the macOS front-end only where a seam diverges from its counterpart and links that counterpart's README entry, the rule [The pure seams](../../notes/wolf-gnome-decisions.md#the-pure-seams) states. Anywhere else such a sentence describes code the crate does not own, and [retro-fixes-5](../../notes/retrospectives/retro-fixes-5.md) records a sweep of about 270 of them in which hand-run greps missed two sites. This target makes that sweep a gate.

A hit is a line of a `*.rs` file under `crates/wolf-gnome/src/` that matches `macos`, `swift` or `appkit` case-insensitively, or `(^|[^[:alnum:]_])NS[A-Z]`. The second pattern is `\bNS[A-Z]` written so BSD and GNU grep agree: `NSOutlineView` is a hit and `DNSServer` is not. Write a pattern to a file with `printf '%s\n'`, never zsh's `echo`, which turns `\b` into a backspace.

`src/README.md` is out of scope because a divergence is stated in its file's entry there, so its mentions are the rule at work. `crates/wolf-gnome/tests/` is out too: it holds about thirty mentions no sweep has reviewed.

An allowlist entry is `<path>:<line text>`, the text trimmed of surrounding whitespace, one entry per matching line of a site. A line number would break on any edit above the site, and the bare match would excuse every later `macOS` in the same file, while an edit to the listed line itself fails, which is the point. An entry that matches no line fails as stale, so the list stays closed. An entry is admitted only for a divergence that links its counterpart, as the numbered `A` entries and the `scroll_direction_is_inverted_from_macos` test's name and its citing doc are.

The checker exits 1 on a finding and 2, with an `ERROR:` line, when there is no `src/` directory, no `*.rs` file under it, or no readable allowlist. Under Bazel the sources arrive only through `data`, so a broken data list would otherwise pass an empty tree. It lists files with `find -L`, because runfiles are symlinks.

### Checker cases

| Case                              | Root           | Expected                                                           |
| --------------------------------- | -------------- | ------------------------------------------------------------------ |
| clean tree                        | `clean/`       | Exit 0: `DNSServer`, `_NSFoo` and a `src/README.md` are no hit.    |
| unlisted mentions                 | `unlisted/`    | Exit 1 with four exact `FAIL:` lines, one per pattern alternative. |
| NS at the start of a line         | `line-start/`  | Exit 1: a line-initial `NSView` is a hit.                          |
| allowlisted site                  | `allowlisted/` | Exit 0: an indented line matches its trimmed entry.                |
| allowlisted text in another file  | `wrong-file/`  | Exit 1 on the other file only: the key includes the path.          |
| stale allowlist entry             | `clean/`       | Exit 1 with a `FAIL: stale allowlist entry:` line.                 |
| symlinked source                  | scratch        | Exit 1: a symlinked `.rs` is scanned, as runfiles are.             |
| no Rust source, no src directory  | scratch        | Exit 2 and an `ERROR:` line each.                                  |
| missing allowlist, extra argument | `clean/`       | Exit 2 and an `ERROR:` line each.                                  |

## Caveats

- The fixers are non-hermetic. `cargo clippy --fix` reaches crates.io (network) and the host cc toolchain. `cargo clippy --fix` is scoped to `-p wolf-core -p wolf-ffi`, because wolf-gnome's GTK stack does not build on macOS. Cargo still builds `wolf-vt` as `wolf-core`'s dependency, and its `build.rs` panics with no `WOLF_GHOSTTY_VT_DIR`, which macOS does not export. So `:clippy.fix` and `:fix` stage the Bazel-built archive, link it into a scratch prefix's `lib/` and export that prefix before the cargo step.
- swift-format scope. Only hand-written `wolf-macos` Swift is covered: every file under its `Sources/` and `Tests/`. The machine-generated swift-bridge bindings under `crates/wolf-ffi/generated/` are excluded. The file list lives in `//crates/wolf-macos:swift_format_srcs` because Bazel forbids cross-package globbing.
- swift-format check and fix. `:swift_format` runs `swift-format lint --strict`, which reports the pretty-printer's findings (`Indentation`, `AddLines`, `LineLength`) as well as the lint-category rules, so a file that `format` would re-indent or re-wrap fails it. To check one file as the target does, run `xcrun swift-format lint --strict --follow-symlinks <file>`; to format one, `bazel run //tools/lint:swift_format.fix -- <file>`.
- The vendoring notes under `bazel/*` are in scope for style but carry no per-file entries, so only the line-length, changelog and all-caps rules bite there. `entry-budget` fires on `####` headings, which those files do not use.
- Two rustfmts. `:rustfmt_files.fix` runs the rules_rust nightly above, while Linux's `fmt` gate runs the devShell's `rustfmt 1.10.0-stable`, and the Linux gate is the judge of wolf-gnome's formatting. Run with `--check` over every tracked `crates/wolf-gnome/*.rs` file, the fixer exits 0 with no diff, so the two agree on the tree as it stands.
- `.bazelignore`. The rustfmt writer (`:rustfmt.fix`) internally evaluates `//...`; the repo's `.bazelignore` excludes `.direnv` so the nix and direnv symlink tree does not break that query.

## Files

- `BUILD.bazel` — all targets.
- `defs.bzl` — `COMMENT_LINT_PATTERNS` and the `comment_srcs` macro every package calls.
- `clippy_fix.bzl` — the `clippy_fix` and `fix_all` toolchain-staging rules.
- `clippy_fix_launcher.sh.tpl` — launcher template for `clippy_fix`.
- `fix_all.sh` — launcher template for `fix_all` (the `:fix` aggregate).
- `clippy_gate.sh` — the `exit 0` body for the `:clippy` gate test.
- `swift_format_check.sh` — `swift-format lint --strict --follow-symlinks` over the passed files.
- `swift_format_check_test.sh` — the `:swift_format_check_test` body.
- `swift_format_fix.sh` — `swift-format format -i` over the given files, or over the wolf-macos sources and tests. It must run under macOS `/bin/bash` 3.2, which its test runs it with through Bazel's test `PATH`, so it reads lists with a `while read` loop, never `mapfile`.
- `swift_format_fix_test.sh` — the `:swift_format_fix_test` body.
- `swift_format_lib.sh` — `find_swift_format`, sourced and never run by `swift_format_check.sh` and `swift_format_fix.sh`, so it has no shebang and no executable bit. Each script sources it from beside its own resolved path, so a target that runs either script lists it in `data`. It must run under macOS `/bin/bash` 3.2.
- `markdown_check.sh` — the documentation rule checks over the passed files.
- `markdown_check_test.sh` — the `:markdown_check_test` body, also a Linux gate step.
- `markdown_all.sh` — Linux only: the checker over the Linux scope above. It calls the host `git`, which the devShell inherits from the host `PATH` because it is not a pure shell.
- `doc_lint_check.sh` — the `:comments` body: `doc-lint --files` over the files it is passed.
- `wolf_core_forbidden_deps.txt` — the patterns no entry of the dependency listing of `wolf-core` and `wolf-test-support` may contain, one per line, with `#` comment lines and blank lines skipped. Each is a case-insensitive substring, so a stem covers its variants: `gtk` covers `gtk4`, and `objc` covers the `objc2-core-*` crates an exact name would miss. It is the one copy both platforms' gates read.
- `wolf_core_deps_check.sh` — `wolf_core_deps_check.sh [LISTING]` matches a listing, the file or else stdin, against `wolf_core_forbidden_deps.txt` beside it. It prints a `FAIL: <line> matches forbidden pattern <pattern>` line per match and exits 1, or prints a count of the clean lines and exits 0. It exits 2 with an `ERROR:` line on a usage error, an unreadable listing or list, a list with no pattern, or a listing with no non-blank line, so a producer that printed nothing never passes. It must run under macOS `/bin/bash` 3.2.
- `wolf_core_deps_check_test.sh` — the `:wolf_core_deps_check_test` body, also a Linux gate step.
- `wolf_gnome_wording_check.sh` — `wolf_gnome_wording_check.sh [ROOT [ALLOWLIST]]` checks the root's `crates/wolf-gnome/src/` against the allowlist [above](#what-wolf_gnome_wording-checks). The root defaults to two directories above the script, not resolved through symlinks, so under Bazel it is the runfiles tree, and the allowlist to the one beside the script. It prints `FAIL: <path>:<line>: <text>` per unexcused hit and `FAIL: stale allowlist entry: <entry>` per stale entry, then a count. It must run under macOS `/bin/bash` 3.2.
- `wolf_gnome_wording_allowlist.txt` — the accepted sites, each entry under a `#` line naming it. Generate an entry from the checker's `FAIL:` line by dropping the line number, never by retyping it.
- `wolf_gnome_wording_check_test.sh` — the `:wolf_gnome_wording_check_test` body, also a Linux gate step.
- `rustfmt_files.sh` — `rustfmt_files.sh RUSTFMT [--check] FILE...`, the `:rustfmt_files.fix` body. It must run under macOS `/bin/bash` 3.2.
- `rustfmt_files_test.sh` — the `:rustfmt_files_test` body, also a Linux gate step. It takes the rustfmt binary as its argument, or else the one on `PATH`.
- `testdata/markdown/` — the fixtures for `markdown_check_test.sh`.
- `testdata/rustfmt_files/` — `misformatted.rs`, holding an `async fn`, and `formatted.rs`, its rustfmt output under both toolchains. `//tools/lint:comments` lexes them, so they carry no comments.
- `testdata/swift_format/` — the fixtures for both swift-format tests.
- `testdata/wolf_core_deps/` — the clean and forbidden listings for `wolf_core_deps_check_test.sh`, crate names plus one Bazel label.
- `testdata/wolf_gnome_wording/` — fake checkouts for `wolf_gnome_wording_check_test.sh`, each a `crates/wolf-gnome/src/` and its own `allowlist.txt`. `//tools/lint:comments` lexes their `.rs` files, so a plain `//` comment there keeps to the two-line limit; nothing compiles them.
