# tools/lint

**macOS-only.** Advisory (non-gating) lint/format targets for the macOS Bazel host: **clippy** and **rustfmt** for Rust and **swift-format** for Swift. This is not the repo's whole lint story — Linux has its own, gating, path; see [Linux](#linux) below. Each tool has a CHECK target (`bazel test`, reports/fails on violations) and a FIX target (`bazel run`, rewrites files in place under `$BUILD_WORKSPACE_DIRECTORY`).

## Policy: advisory, not gating

**Every** target here is tagged `manual`, so no wildcard build/test (e.g. `bazel test //crates/...`) ever sweeps it in — the pre-existing default suite is unchanged by these targets. (A specific pass/fail count was asserted here historically; it is not verifiable from a Linux host and the repo has grown test targets since, so the load-bearing claim is the `manual` tagging, not a number. See [`macos-followups.md`](../../macos-followups.md).) Existing lint violations across the codebase are EXPECTED and out of scope: a CHECK target "passing verification" means "the linter RAN on the host without a Bazel/toolchain error" — it may still legitimately REPORT findings (which fail the target). Only running these targets deliberately, by label, exercises them.

## Linux

These Bazel targets do not and cannot run on Linux: the Linux devShell has no Bazel and no Xcode. On Linux the equivalents are `cargo clippy`, `cargo fmt` and `prettier` invoked directly inside the Nix devShell, and — unlike the advisory targets here — they are **gating**. See [`build.md`](../../build.md), "The Linux verification gate", for the canonical command set.

The two hosts are deliberately asymmetric:

- **`-Dwarnings`** is opt-in on macOS (via `--config=lint`) and unconditional on Linux.
- **`swift-format`** has no Linux counterpart — there is no Swift in the Linux devShell.
- **`prettier`** (the Markdown format check) has no macOS/Bazel counterpart today; it exists only in the Linux gate.
- **`wolf-gnome`** is linted only on Linux; its GTK stack does not build on macOS, which is why the Rust fixers here are scoped to `-p wolf-core -p wolf-ffi`.

## Toolchains and binaries

- **clippy / rustfmt** come from the toolchain `rules_rust` downloads (Rust 1.85.0); the Darwin devShell ships no standalone cargo/rustc/clippy/rustfmt.
- **swift-format** is the host Xcode 26 binary (v6.3.0), resolved at runtime via `xcrun --find swift-format` (with `xcode-select -p` and the canonical Xcode path as fallbacks, because Bazel test actions run with a scrubbed environment).

## Targets

| Purpose            | Target              | Command                                   |
| ------------------ | ------------------- | ----------------------------------------- |
| clippy check       | `:clippy`           | `bazel test //tools/lint:clippy`          |
| clippy fix         | `:clippy.fix`       | `bazel run //tools/lint:clippy.fix`       |
| rustfmt check      | `:rustfmt`          | `bazel test //tools/lint:rustfmt`         |
| rustfmt fix        | `:rustfmt.fix`      | `bazel run //tools/lint:rustfmt.fix`      |
| swift-format check | `:swift_format`     | `bazel test //tools/lint:swift_format`    |
| swift-format fix   | `:swift_format.fix` | `bazel run //tools/lint:swift_format.fix` |
| all checks         | `:check`            | `bazel test //tools/lint:check`           |
| all fixes          | `:fix`              | `bazel run //tools/lint:fix`              |

Internal targets: `:clippy_core` / `:clippy_ffi` (`rust_clippy`, built as `data` of `:clippy` so a clippy failure fails that test) and `:rustfmt_core` / `:rustfmt_ffi` (`rustfmt_test`, aggregated by the `:rustfmt` and `:check` suites).

## How each target works

- **`:clippy`** is a trivial `sh_test` (`clippy_gate.sh`, `exit 0`) whose `data` are the `rust_clippy` targets `:clippy_core` / `:clippy_ffi`. Bazel must BUILD them (running clippy) to stage runfiles, so a clippy failure fails the test.
- **`:clippy.fix`** is the custom `clippy_fix` rule (`clippy_fix.bzl`): it resolves `@rules_rust//rust:toolchain_type`, stages the toolchain sysroot (cargo/cargo-clippy/clippy-driver/rustc/rust_std) as runfiles, and runs `cargo clippy --fix --allow-dirty --allow-staged -p wolf-core -p wolf-ffi`.
- **`:rustfmt`** is a `test_suite` over two `rustfmt_test` targets (`rustfmt --check` on each crate). **`:rustfmt.fix`** is an `alias` to the rules_rust writer `@rules_rust//:rustfmt`.
- **`:swift_format`** is an `sh_test` (`swift_format_check.sh`, tags `manual` + `no-sandbox`) that runs `swift-format lint --strict` over the `//crates/wolf-macos:swift_format_srcs` filegroup. **`:swift_format.fix`** is an `sh_binary` (`swift_format_fix.sh`) running `swift-format format -i`.
- **`:fix`** is the custom `fix_all` rule: it stages BOTH the rust toolchain and the (separate) rules_rust rustfmt toolchain, then runs rustfmt, then `cargo clippy --fix`, then swift-format, all in place.

## Caveats

- **clippy fail-on-lint is opt-in.** By default the clippy CHECK reports findings; `--config=lint` (in `.bazelrc`) adds `-Dwarnings` so it hard-fails: `bazel test --config=lint //tools/lint:clippy`. It is not default so the standard build/test path is byte-for-byte unchanged.
- **The fixers are non-hermetic.** `cargo clippy --fix` reaches crates.io (network) and the host cc toolchain. Rust fixers are scoped to `-p wolf-core -p wolf-ffi`: wolf-gnome's GTK stack does not build on macOS.
- **swift-format scope.** Only hand-written `wolf-macos` Swift is covered (Sources/WolfKeyMap, Sources/WolfMacOS, Tests/WolfKeyMapTests). The machine-generated swift-bridge bindings under `crates/wolf-ffi/generated/` are EXCLUDED. The file list lives in `//crates/wolf-macos:swift_format_srcs` because Bazel forbids cross-package globbing.
- **swift-format check and fix enforce different things.** `:swift_format` (lint) runs `swift-format lint --strict` and checks only swift-format's lint-category rules; `:swift_format.fix` (format) runs `swift-format format -i` and additionally restyles whitespace and indentation. A passing check therefore does NOT mean the fixer would leave the files unchanged: the lint check can report 0 findings on files that `format` would still restyle. (The tree is currently formatted, so `format -i` is a no-op; a hand-edit that only violates whitespace/indentation rules would pass the lint check but be rewritten by the fixer.)
- **`.bazelignore`.** The rustfmt writer (`:rustfmt.fix`) internally evaluates `//...`; the repo's `.bazelignore` excludes `.direnv` so the nix+direnv symlink tree does not break that query.

## Files

- `BUILD.bazel` — all targets.
- `clippy_fix.bzl` — the `clippy_fix` and `fix_all` toolchain-staging rules.
- `clippy_fix_launcher.sh.tpl` — launcher template for `clippy_fix`.
- `fix_all.sh` — launcher template for `fix_all` (the `:fix` aggregate).
- `clippy_gate.sh` — the `exit 0` body for the `:clippy` gate test.
- `swift_format_check.sh` — `swift-format lint --strict` over the passed files.
- `swift_format_fix.sh` — `swift-format format -i` over the wolf-macos sources.
