# Plan: Bootstrap a Bazel build for `wolf-core` on macOS

## STATUS: COMPLETE ✅
`bazel build //crates/wolf-core:wolf_core` builds GREEN on aarch64-darwin under
Bazel 8.7.0 using the host Xcode SDK; GUI denylist clean; Linux devShell unchanged;
docs reconciled (build.md/architecture.md/READMEs/flake comments) + prettier-formatted.
Reviewer APPROVED wiring; bazelShim verified necessary; nothing committed (per rule).
New files: .bazelversion .bazelrc MODULE.bazel MODULE.bazel.lock BUILD.bazel
Cargo.Bazel.lock crates/wolf-core/BUILD.bazel. Modified: flake.nix .envrc .gitignore
+ docs. Follow-ups deferred: tests-under-Bazel (need WOLF_NVIM_BIN on macOS),
wolf-macos front-end, CI lockfile_mode=error.

## Problem statement

Wolf is a cross-platform Markdown editor (Neovim core + native front-ends). Linux
builds with Cargo under Nix (`nix build`). We are now starting the macOS port. On
macOS the canonical build tool will be **Bazel** (driven by **bazelisk**), and it
**must build with the host system Xcode SDK**, not the older SDK that ships inside
Nix.

This first step is scoped narrowly to getting the Rust core compiling under Bazel:

1. Add **bazelisk** to the root `flake.nix` so `direnv` brings it into the dev
   shell on macOS (the flake is currently Linux-only and emits no darwin outputs;
   `.envrc` only runs `use flake` on Linux).
2. Stand up a working **Bazel toolchain that builds the `wolf-core` crate** using
   the system SDK for the C compiler/linker (rusqlite's `bundled` SQLite compiles
   via `cc`, so the C toolchain choice matters).

### Environment (verified)
- Host: `aarch64-darwin` (Apple Silicon), macOS (Darwin 25.5).
- Present: Nix 2.32, direnv 2.37. **Absent**: bazel, bazelisk, cargo, rustc.
- Xcode at `/Applications/Xcode.app/...`; SDK at `.../MacOSX.sdk`.
- Repo is a Cargo workspace: `crates/wolf-core` (lib `wolf_core`) + `crates/wolf-gnome`.
  `wolf-core` deps include `nvim-rs`, `tokio`, `rmpv`, `parking_lot`,
  `rusqlite` (bundled), `ignore`, `thiserror`, `tracing`, `async-trait`, `tempfile`.
  `Cargo.lock` exists.

### Out of scope (later phases)
- Building `wolf-gnome` (Linux/GTK) under Bazel.
- Running `wolf-core` tests under Bazel (they need a pinned `nvim` via
  `WOLF_NVIM_BIN`, not yet provisioned on macOS). Verification for THIS step is a
  successful `bazel build` of `wolf-core`.
- swift-bridge bindings, `wolf-macos` front-end, bundling Neovim into a `.app`.

### Constraints / source-of-truth note
- Per `CLAUDE.md`, docs are the source of truth. `architecture.md` §Build and
  `build.md` currently describe macOS as "Xcode/SwiftPM"; introducing Bazel as the
  macOS build tool means those docs must be reconciled (technical-writer pass).
- `wolf-core`'s "one hard rule": no GUI toolkit in its dependency tree. The Bazel
  build must not introduce one.
- Each source directory must have a README.md describing its contents.

### Intended design (to be validated/refined by architect)
- bzlmod (`MODULE.bazel`) + `rules_rust` with **crate_universe** consuming the
  existing `Cargo.toml`/`Cargo.lock`.
- **`apple_support`** to drive the C/link toolchain from the host Xcode SDK.
- `rules_rust` downloads a pinned rustc toolchain.
- `flake.nix`: add `aarch64-darwin` (+ `x86_64-darwin`) devShell providing
  bazelisk; teach `.envrc` to `use flake` on Darwin.
- Verification = `bazel build //crates/wolf-core` (or `//crates/wolf-core:wolf_core`)
  succeeds on this machine using the system SDK.

## Architect's plan (approved by orchestrator)

### Version pins
- Bazel: **target 8 if rules_rust 0.71.2 + apple_support 2.6.1 declare Bazel 8
  support; else fall back to 7.4.1** (user decision 2026-06-29; NOT 9 — 9 would
  re-pin the whole dep set, out of scope for bootstrap).
- (orig plan pin: Bazel `7.4.1`); rules_rust `0.71.2`; apple_support `2.6.1`
  (repo_name `build_bazel_apple_support`); rules_cc `0.1.1`; platforms `0.0.11`;
  rustc `1.85.0`; edition `2021`.

### Files to CREATE
1. `.bazelversion` → `7.4.1`.
2. `.bazelrc` → `common --enable_bzlmod`; `common --enable_platform_specific_config`;
   `build --incompatible_enable_cc_toolchain_resolution`;
   `build:macos --apple_platform_type=macos`;
   `build:macos --repo_env=DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`;
   `build:macos --apple_generate_dsym=false`; `common --lockfile_mode=update`.
3. `MODULE.bazel` (module `wolf` 0.1.0):
   - `bazel_dep` rules_rust 0.71.2, rules_cc 0.1.1, platforms 0.0.11,
     apple_support 2.6.1 (repo_name build_bazel_apple_support).
   - apple cc: `use_extension("@build_bazel_apple_support//crosstool:setup.bzl","apple_cc_configure_extension")`,
     `use_repo(...,"local_config_apple_cc")`,
     `register_toolchains("@local_config_apple_cc_toolchains//:all")`.
   - rust: `use_extension("@rules_rust//rust:extensions.bzl","rust")`,
     `rust.toolchain(edition="2021", versions=["1.85.0"])`,
     `use_repo(rust,"rust_toolchains")`, `register_toolchains("@rust_toolchains//:all")`.
   - crates: `use_extension("@rules_rust//crate_universe:extensions.bzl","crate")`,
     `crate.from_cargo(name="crates", cargo_lockfile="//:Cargo.lock",
     manifests=["//:Cargo.toml","//crates/wolf-core:Cargo.toml"],
     supported_platform_triples=["aarch64-apple-darwin","x86_64-apple-darwin"])`,
     `use_repo(crate,"crates")`. RECOMMENDED: add `lockfile="//:Cargo.Bazel.lock"`
     for reviewability and commit it.
4. root `BUILD.bazel` → `exports_files(["Cargo.toml","Cargo.lock"])`.
5. `crates/wolf-core/BUILD.bazel`:
   - `load("@rules_rust//rust:defs.bzl","rust_library")`,
     `load("@crates//:defs.bzl","all_crate_deps")`, `exports_files(["Cargo.toml"])`.
   - `rust_library(name="wolf_core", srcs=glob(["src/**/*.rs"]),
     crate_name="wolf_core", edition="2021", deps=all_crate_deps(normal=True),
     proc_macro_deps=all_crate_deps(proc_macro=True), visibility=["//visibility:public"])`.
     Follow whatever `@crates//:defs.bzl` documents for normal vs proc_macro split.
     NO rust_test/example target this step.
6. NO `crates/wolf-gnome/BUILD.bazel` — deliberately: gnome stays out of the Bazel
   graph (read by cargo metadata during repin, never given a target → GTK never built).
7. Commit generated lockfile(s): `MODULE.bazel.lock` (and `Cargo.Bazel.lock` if the
   `lockfile` attr is set — recommended).

### Files to MODIFY
- `flake.nix`: systems → add `aarch64-darwin` + `x86_64-darwin`. Add
  `isDarwin = pkgs.stdenv.isDarwin`. `devShells.default = if isDarwin then darwinShell
  else linuxShell`. `linuxShell` = the EXISTING mkShell block moved VERBATIM (zero
  behavior change — reviewer diffs to confirm). `darwinShell` = `pkgs.mkShell {
  packages = [ pkgs.bazelisk pkgs.git ]; shellHook = export
  DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer; }`. darwinShell must NOT
  reference rustToolchain/neovim/GTK. Guard rustToolchain/GTK eval so darwin eval
  doesn't force Linux-only pkgs (Nix laziness helps; wrap if needed).
- `.envrc`: `case "$(uname -s)" in Linux|Darwin) use flake ;; esac`.
- `.gitignore`: append `/bazel-*`.

### C toolchain (system SDK) wiring
apple_support's `apple_cc_configure_extension` → `@local_config_apple_cc_toolchains`,
registered + selected via cc toolchain resolution; `DEVELOPER_DIR` (`.bazelrc` :macos
+ flake shellHook) pins host Xcode. rusqlite/libsqlite3-sys bundled `cargo_build_script`
(cc) + final link resolve through that apple cc toolchain. No per-crate annotation
expected; fallback = `crate.annotation(crate="libsqlite3-sys", build_script_env={...})`.

### Commands
- Repin/generate: `CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-core:wolf_core`
  (uses rules_rust-downloaded cargo, NOT system cargo).
- Verify build: `bazel build //crates/wolf-core:wolf_core`.
- Denylist query: `bazel query 'deps(//crates/wolf-core:wolf_core)' | grep -iE
  'gtk|gdk|glib|cairo|pango|adwaita|objc|cocoa|core-graphics|winit|wgpu|skia|softbuffer'
  || echo clean`.

### Implementation order
1. flake.nix + .envrc → `direnv allow`, confirm bazel/bazelisk on PATH.
2. `.bazelversion`. 3. `.gitignore`. 4. MODULE.bazel + both BUILD.bazel.
5. repin (generate lockfiles). 6. verify build. 7. commit lockfiles.

### Risks + fallbacks (condensed)
- repin needs network/cargo → run once online, commit lockfiles.
- rusqlite cc build fails → crate.annotation build_script_env/SDKROOT; ensure DEVELOPER_DIR.
- gnome GTK leaks → already mitigated; fallback = split Bazel-only manifest under `bazel/`.
- apple cc not selected → register apple_support first; keep cc_toolchain_resolution;
  fallback `--crosstool_top=@local_config_apple_cc//:toolchain`.
- module version conflict → align rules_cc/platforms to rules_rust 0.71.2's deps (`bazel mod deps`).
- bazelisk wrong ver → `.bazelversion`; fallback `USE_BAZEL_VERSION=7.4.1`.
- darwin flake forces GTK eval → fully scope rustToolchain/GTK under Linux branch.
- extension symbol drift → check 0.71.2 crate_universe_bzlmod docs.

### Verification checklist
bazel/bazelisk on PATH in darwin shell; `bazel version`==7.4.1; build of
`//crates/wolf-core:wolf_core` succeeds; build used host Xcode SDK (xcrun/MacOSX.sdk
in log), not Nix; deps query has NONE of the GUI denylist; rusqlite/libsqlite3-sys
compile; Linux devShell diff empty; lockfiles committed (clean re-run needs no repin);
no `bazel-*` symlinks in `git status`.

### Documentation impact (technical-writer, later — NOT now)
`build.md` (add macOS/Bazel section, reconcile "later phase" line); `architecture.md`
§Build (macOS core = Bazel+bazelisk host SDK; SwiftPM front-end still later);
`crates/wolf-core/README.md` (Bazel build note + denylist holds); `README.md`
(macOS Bazel path). No new source dir created → no new dir README required.

## Implementation result (coder)
- BUILD GREEN: `bazel build //crates/wolf-core:wolf_core` succeeds on aarch64-darwin
  (468 actions → libwolf_core-*.rlib). Bazel pinned **8.7.0** (apple_support 2.6.1
  `bazel_compatibility=[">=7.4.0"]`, rules_rust 0.71.2 no restriction → 8 clean).
- Deviations (forced transitive, NOT rule-set chasing): rules_cc 0.1.1→**0.2.15**
  (apple_support 2.6.1 requires it); platforms 0.0.11→**1.1.0** (rules_rust requires).
  Added `local_config_apple_cc_toolchains` to apple_cc `use_repo` (plan's
  register_toolchains referenced it). Added committed `lockfile="//:Cargo.Bazel.lock"`.
- Host Xcode SDK CONFIRMED: SQLite cc action uses apple_support `wrapped_clang`
  `-target arm64-apple-macosx26.5`, XCODE 26.5 — NOT the Nix apple-sdk-14.4 (whose
  SDKROOT/NIX_CFLAGS leak is overridden by wrapped_clang's resolved SDKROOT).
- GUI denylist CLEAN (only false-substring hit: libc's `glibc/` paths).
- Lockfiles reproducible: re-run w/o REPIN = up-to-date, no repin. `bazel-*` gitignored.
- Files: created `.bazelversion`(8.7.0), `MODULE.bazel`, `crates/wolf-core/BUILD.bazel`,
  `MODULE.bazel.lock`, `Cargo.Bazel.lock`; modified `.gitignore`. (flake.nix/.envrc/
  .bazelrc/root BUILD.bazel were already present from a prior session — orchestrator
  verified their final contents match the plan.)
- Nothing committed yet (per standing rule).

## Orchestrator-spotted issues for review
- `flake.nix:12-13` top comment still says "Linux-only … emits no darwin/macOS
  outputs" — now FALSE (darwin shells added). FIX (technical-writer/reviewer).
- `flake.nix:~52` comment says bazelisk "fetches the pinned Bazel (7.4.1)" — actual
  pin is 8.7.0. FIX comment.

## Reviewer verdict: CHANGES-REQUIRED (comments only) — wiring APPROVED
- Bazel wiring, system-SDK routing, Linux-shell-unchanged, denylist, version-pin
  deviations: all confirmed correct/justified. Build ship-ready.
- MUST-FIX (technical-writer, stale flake.nix comments): line 1 "NixOS-only dev
  environment"; lines 12-13 "Linux-only … emits no darwin/macOS outputs"; line 15
  description "NixOS-only"; lines 51-52 "(7.4.1)" → 8.7.0 (better: "version pinned
  in .bazelversion").
- NIT-2 (coder): `bazelShim` (flake.nix:53-61) may be redundant if nixpkgs
  `bazelisk` already ships a `bazel` symlink — verify; simplify if so; re-confirm
  `bazel` still resolves in the darwin shell. Harmless either way.
- NIT-1 (.bazelrc:7,12 redundant-on-8 flags) + NIT-3 (lockfile_mode=update vs CI
  error mode): non-blocking, orchestrator-tracked for a later CI step.

## Decisions on review findings (orchestrator)
- MUST-FIX flake comments → technical-writer (fold into the doc-reconciliation pass).
- NIT-2 bazelShim → coder verifies/simplifies FIRST (structural flake edit + reverify),
  THEN technical-writer touches comments — sequenced to avoid concurrent flake.nix edits.
- NIT-1/NIT-3 → defer to the future CI step; note in docs as known follow-ups, no change now.

## Progress log
- (orchestrator) Reset stale plan. Verified env. Architect plan approved. User chose
  Bazel-8-if-compatible. Coder → build GREEN on Bazel 8.7.0, host SDK confirmed,
  denylist clean. Reviewer APPROVED wiring; only stale-comment MUST-FIX + bazelShim
  NIT. Next: coder (bazelShim), then technical-writer (comments + doc reconciliation).
