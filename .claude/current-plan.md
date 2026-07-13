# Plan: Bazel lint targets (clippy, rustfmt, swift-format) for macOS host

## Problem statement

Add Bazel targets that let developers run three linters/formatters on the macOS host:

1. **clippy** (Rust)
2. **rustfmt** (Rust)
3. **swift-format** (Swift — the host Xcode 26 binary, NOT SwiftLint, NOT vendored)

The Darwin devShell ships no standalone Rust toolchain, so clippy/rustfmt must use the toolchain that `rules_rust` downloads. swift-format is the host Xcode binary at `XcodeDefault.xctoolchain/usr/bin/swift-format` (v6.3.0, resolvable via `xcrun --find swift-format`).

## Requirements (decided)

- **Three tools**: clippy, rustfmt, swift-format.
- **Two modes per tool**:
  1. a CHECK target that reports/fails on violations (`bazel test` or `bazel build`);
  2. an AUTOFIX target that rewrites files in place (`bazel run`). clippy `--fix` is limited to machine-applicable suggestions; rustfmt and swift-format fully autoformat.
- **Advisory, not gating**: check targets MUST NOT be swept into the existing default suite. `bazel test //crates/wolf-core/... //crates/wolf-ffi/... //crates/wolf-macos/...` is green (47/47) and must stay exactly that. Tag lint checks `manual` and/or place them in a dedicated package (e.g. `//tools/lint`) so wildcards skip them. Existing lint violations are EXPECTED and out of scope to fix now.
- **Aggregate entry points**: one command to run all checks, one to run all fixes; documented in `build.md`.
- **Docs are source of truth** (CLAUDE.md): update `build.md` and relevant READMEs.

## Environment facts

- MODULE.bazel: rules_rust 0.71.2 (rust toolchain 1.85.0, edition 2021, downloaded), rules_swift 3.6.1, rules_apple 4.5.3, apple_support 2.6.1. crate_universe repo = `@crates`.
- Rust crates: `//crates/wolf-core`, `//crates/wolf-ffi` (rust_library/rust_test).
- Swift: `//crates/wolf-macos` (rules_swift swift_library + swift_test; WolfKeyMap/WolfMacOS Sources).
- swift-format has no Bazel ruleset here — likely a genrule/sh_binary wrapper around the xcrun/toolchain swift-format binary.
- rules_rust supports clippy (rust_clippy rule / clippy aspect) and rustfmt (rustfmt_test / rustfmt aspect / `@rules_rust//tools/rustfmt` writer).

## Verification

The targets actually work on this macOS host:

- each CHECK target runs (`bazel build`/`test`) without Bazel/config errors;
- each FIX runner runs (`bazel run`) without Bazel/config errors;
- the pre-existing `bazel test //crates/...` suite stays green (47/47).

Because these are build-tooling targets (not app code), "tests" = demonstrating the targets execute correctly on the host, not new Rust/Swift unit tests.

## Architect's implementation plan

All targets live in a new `//tools/lint` package, every target `tags = ["manual"]` so wildcard build/test never sweeps them. clippy/rustfmt use the rules_rust downloaded 1.85.0 toolchain; swift-format is the host Xcode binary via `xcrun --find swift-format`.

### Target layout (all in `//tools/lint`, all `manual`)

| Purpose | Target | Rule | Invocation |
| --- | --- | --- | --- |
| clippy check | `:clippy` | `sh_test` (body `exit 0`) whose `data=[:clippy_core,:clippy_ffi]` | `bazel test //tools/lint:clippy` |
| clippy check (core) | `:clippy_core` | `rust_clippy` (deps = wolf-core rust targets) | built as data of `:clippy` |
| clippy check (ffi) | `:clippy_ffi` | `rust_clippy` (deps = wolf-ffi rust targets) | built as data of `:clippy` |
| clippy fix | `:clippy.fix` | `clippy_fix` custom rule (`clippy_fix.bzl`) | `bazel run //tools/lint:clippy.fix` |
| rustfmt check | `:rustfmt` | `test_suite` over `:rustfmt_core` + `:rustfmt_ffi` | `bazel test //tools/lint:rustfmt` |
| rustfmt check (core/ffi) | `:rustfmt_core`, `:rustfmt_ffi` | `rustfmt_test` | via the suite |
| rustfmt fix | `:rustfmt.fix` | `alias` → `@rules_rust//:rustfmt` | `bazel run //tools/lint:rustfmt.fix` |
| swift-format check | `:swift_format` | `sh_test` (`no-sandbox`) | `bazel test //tools/lint:swift_format` |
| swift-format fix | `:swift_format.fix` | `sh_binary` | `bazel run //tools/lint:swift_format.fix` |
| ALL checks | `:check` | `test_suite` (`:clippy`,`:rustfmt_core`,`:rustfmt_ffi`,`:swift_format`) | `bazel test //tools/lint:check` |
| ALL fixes | `:fix` | `sh_binary` running all three fixers inline | `bazel run //tools/lint:fix` |

Rationale: making each check TEST-shaped lets one `bazel test //tools/lint:check` run everything. clippy has no native test, so `:clippy` is a trivial `sh_test` whose `data` deps are `rust_clippy` targets — Bazel must build them (running clippy) to stage runfiles, so a clippy failure fails the test.

### Approach per tool (chosen / rejected)

- **clippy CHECK** = `rust_clippy` rule from `@rules_rust//rust:defs.bzl` (rejected: clippy aspect — too easy to sweep into wildcards). **clippy FIX** = custom `clippy_fix` Starlark rule resolving `@rules_rust//rust:toolchain_type`, staging the toolchain's cargo/clippy_driver/rustc/rust_std as runfiles, launcher runs `cargo clippy --fix --allow-dirty --allow-staged --workspace` in `$BUILD_WORKSPACE_DIRECTORY` (rejected: plain `sh_binary` assuming cargo on PATH — devShell has none). NOTE: clippy --fix uses cargo → crates.io (network) + Xcode cc; non-hermetic dev convenience. **Simpler fallback allowed** if the custom rule fights back: any launcher that reaches the toolchain cargo is acceptable — bar is "runs without Bazel/config error."
- **rustfmt CHECK** = `rustfmt_test` rule (rejected: rustfmt aspect). **rustfmt FIX** = `alias` → `@rules_rust//:rustfmt` writer.
- **swift-format CHECK** = `sh_test` running `swift_format_check.sh`: `SF=$(xcrun --find swift-format); "$SF" lint --strict <files>`; tags `["manual","no-sandbox"]` (host binary, not a sandbox input). **swift-format FIX** = `sh_binary` running `swift_format_fix.sh`: `swift-format format -i --parallel` over `$BUILD_WORKSPACE_DIRECTORY/crates/wolf-macos/{Sources,Tests}/**/*.swift`. EXCLUDE `crates/wolf-ffi/generated/*.swift` (machine-generated bindings).

### Files to CREATE

- `tools/lint/BUILD.bazel` — all targets above.
- `tools/lint/clippy_fix.bzl` — `clippy_fix` executable rule (toolchain-resolving).
- `tools/lint/clippy_gate.sh` — `#!/usr/bin/env bash` + `exit 0` (forces clippy data deps to build).
- `tools/lint/swift_format_check.sh` — xcrun-resolve + `lint --strict`, loud fail if not found.
- `tools/lint/swift_format_fix.sh` — xcrun-resolve + `format -i --parallel` in workspace.
- `tools/lint/fix_all.sh` — rustfmt writer, then clippy --fix, then swift fix; all vs `$BUILD_WORKSPACE_DIRECTORY`.
- `tools/lint/README.md` — required by CLAUDE.md; documents every target, check/fix, advisory policy, caveats.
- (Optional, recommended) `.rustfmt.toml` — pins rustfmt so Bazel + local `cargo fmt` agree. `clippy.toml` optional. `.swift-format` NOT added (defaults). If config files add friction, SHIP WITHOUT them.

### Files to MODIFY

- `crates/wolf-macos/BUILD.bazel` — add `filegroup` `swift_format_srcs` (the Swift files under `Sources/WolfKeyMap`, `Sources/WolfMacOS`, `Tests/WolfKeyMapTests`; NOT the wolf-ffi generated Swift), visible to `//tools/lint`. Cross-package globbing is not allowed, so this filegroup MUST live here and be referenced by label.
- `.bazelrc` — add `--config=lint` carrying `--@rules_rust//rust/settings:clippy_flags=-Dwarnings` (make clippy fail on lints) scoped so the DEFAULT build/test path is byte-for-byte unchanged. If config files committed, also the `clippy_toml`/`rustfmt_toml` setting labels + root `exports_files`.
- `BUILD.bazel` (root) — `exports_files` for any committed config referenced by `//:` label.
- `MODULE.bazel` — EXPECTED NO CHANGE (`rust.toolchain(versions=["1.85.0"])` + `register_toolchains("@rust_toolchains//:all")` already provide rustfmt/clippy). FALLBACK only if `bazel run @rules_rust//:rustfmt` errors "no rustfmt toolchain": add `rustfmt_version` to `rust.toolchain`.
- `build.md` — new "Linting and formatting (macOS)" section (6 cmds + 2 aggregates, advisory note, toolchain/host-binary notes, clippy-fix cargo/network caveat).
- `crates/wolf-macos/README.md` — one-line pointer to swift-format targets + generated exclusion.

### Developer commands (for build.md)

- clippy check `bazel test //tools/lint:clippy` · fix `bazel run //tools/lint:clippy.fix`
- rustfmt check `bazel test //tools/lint:rustfmt` · fix `bazel run //tools/lint:rustfmt.fix`
- swift check `bazel test //tools/lint:swift_format` · fix `bazel run //tools/lint:swift_format.fix`
- ALL checks `bazel test //tools/lint:check` · ALL fixes `bazel run //tools/lint:fix`

### VERIFICATION checklist (coder runs on macOS host)

1. `bazel test //tools/lint:clippy` — runs via downloaded toolchain (pass/fail OK; must not error on config/toolchain).
2. `bazel run //tools/lint:clippy.fix` — resolves toolchain cargo, executes without Bazel/config error.
3. `bazel test //tools/lint:rustfmt` — runs `rustfmt --check` on both crates.
4. `bazel run //tools/lint:rustfmt.fix` — runs the writer.
5. `bazel test //tools/lint:swift_format` — finds swift-format via xcrun, lints (no-sandbox).
6. `bazel run //tools/lint:swift_format.fix` — formats in place under `BUILD_WORKSPACE_DIRECTORY`.
7. `bazel test //tools/lint:check` — expands to all checks.
8. `bazel run //tools/lint:fix` — runs all three fixers.
9. `bazel test //crates/wolf-core/... //crates/wolf-ffi/... //crates/wolf-macos/...` — still 47/47 green.
10. `bazel query 'tests(//crates/wolf-core/... + //crates/wolf-ffi/... + //crates/wolf-macos/...)'` — NO `//tools/lint` target appears.

IMPORTANT for advisory policy: because existing code likely HAS violations, the CHECK targets (2/3/5/7 and the clippy/rustfmt/swift checks) may legitimately REPORT FAILURES. That is ACCEPTED. The pass bar for each check target is "it RUNS on the host without a Bazel/config/toolchain error" — NOT "it reports zero violations." Only checklist item 9 (default suite 47/47) and item 10 (exclusion) are hard green gates. Capture and report the actual violation output for later cleanup.

### Risks (rules_rust 0.71.2)

- clippy `--fix` non-hermetic (cargo/crates.io/cc) — dev convenience only.
- Confirm rustfmt toolchain auto-registers; fallback = add `rustfmt_version`.
- clippy fails on lints only with `-Dwarnings`; keep `capture_clippy_output` FALSE.
- Confirm swift-format subcommands are `lint` / `format -i` on v6.3.0; `xcrun --find` resolves.
- `swift_format_srcs` filegroup must live in wolf-macos pkg (no cross-pkg glob); exclude generated Swift.

## Status

- [x] Architect plan
- [x] Implementation
- [x] Verification (10/10 ran; hard gates 9 & 10 green; checks run w/o config/toolchain error)
- [x] Review (PASS, no blockers; all 5 deviations accepted incl. .bazelignore)
- [x] Docs (build.md + tools/lint/README.md swift lint-vs-format nuance added; prettier clean)

## Coder outcome + deviations (for reviewer)

Implemented `//tools/lint` per plan. Verification: item 9 (default suite 47/47) ✓, item 10 (0 lint targets in default wildcards) ✓, swift check PASSES (0 findings), clippy/rustfmt checks RUN and report existing violations (advisory-accepted). Fix runners all execute; their rewrites were reverted to keep the tree clean.

Deviations needing reviewer judgment:

1. **Added `.bazelignore` (`.direnv`)** — the rustfmt writer internally evaluates `//...` which hits the `.direnv/flake-inputs` symlink gotcha; `.bazelignore` is the canonical nix+direnv fix. Repo-wide effect on `//...`, but default suite stays green and explicit `//crates/...` patterns unaffected. Scope-acceptable?
2. **`:fix` uses custom `fix_all` rule** (not plain sh_binary) — devShell has no cargo/rustfmt on PATH; toolchain's bare `rustfmt` aborts (driver hash mismatch), so it stages `@rules_rust//rust/rustfmt:toolchain_type` too. Allowed by plan.
3. **Rust fixers scoped `-p wolf-core -p wolf-ffi`** (not `--workspace`) — wolf-gnome (GTK) doesn't build on macOS.
4. **swift-format resolution hardened** — Bazel scrubbed-env made `xcrun --find` fail; check script falls back to `xcode-select -p` / canonical Xcode path.
5. **`--config=lint` retained but redundant** — `rust_clippy` already denies warnings by default.

Semantic nuance to confirm acceptable: swift `lint --strict` (check) reports 0, but `format -i` (fix) would restyle 75/76 files — lint mode enforces a subset of format. Documented; no committed `.swift-format` config (defaults).

Existing-violation volume (follow-up cleanup, OUT OF SCOPE now): clippy — 1 real library error `crates/wolf-ffi/src/lib.rs:1252` (`unnecessary_map_or`) + many `doc_lazy_continuation` in tests; rustfmt — 45 files; swift-format — 0 lint but 75 files differ under `format`.

## Orchestrator notes

- **Test-writer step SKIPPED (explicit decision):** the plan creates NO Rust/Swift unit tests — the committed `.sh` scripts ARE the check-test bodies and are implementation artifacts (coder's job), and verification is running the Bazel targets on the host. There is nothing for the test-writer to author. Proceeding architect → coder → reviewer → technical-writer.
