# Building Wolf

Wolf builds one way per platform. On Linux (NixOS) Cargo builds the [`wolf-core`](crates/wolf-core/README.md) Rust core and the [`wolf-gnome`](crates/wolf-gnome/README.md) GTK front-end inside the flake devShell. On macOS, Bazel — driven by bazelisk against the host Xcode SDK — builds `wolf-core`, the [`wolf-ffi`](crates/wolf-ffi/README.md) swift-bridge crate, and the [`wolf-macos`](crates/wolf-macos/README.md) AppKit `.app`, which requires macOS Tahoe (26) or newer and Xcode 26+.

Bazel is authoritative for everything it builds. The generated Xcode project is a debugging aid and never the build of record.

## Linux and NixOS (Cargo)

### Prerequisites

- NixOS. The Linux dev environment is the `flake.nix` devShell.
- Nix with flakes enabled (`experimental-features = nix-command flakes`).
- [`direnv`](https://direnv.net/) with [`nix-direnv`](https://github.com/nix-community/nix-direnv).

### Activating the dev environment

The repo ships an `.envrc` that runs `use flake` on both Linux and macOS. Activate it once per checkout:

```sh
direnv allow
```

The Linux shell supplies the pinned Rust toolchain (`rust-src`, `rust-analyzer`, `clippy`, `rustfmt`), a pinned hermetic Neovim exported as `WOLF_NVIM_BIN`, a pinned ripgrep exported as `WOLF_RG_BIN`, the GTK4 and libadwaita stack `wolf-gnome` needs, and a generated `FONTCONFIG_FILE` that pins the font stack so the offscreen golden render is reproducible. Do not reach for a system `cargo`, `rustc`, `nvim` or `rg` — `flake.nix` pins all four, and builds and tests must run inside the shell.

`WOLF_NVIM_BIN` is the only way core and its tests locate Neovim: never `PATH`, never a scan of `/nix/store`. Unset, `WolfCore::start` fails with `NvimError::BinNotSet` and the wolf-core integration tests panic loudly rather than skip. `WOLF_RG_BIN` locates ripgrep the same way; unset, content search degrades to an empty session carrying an error.

After editing `flake.nix`, run through `nix develop --command`. An already-loaded direnv environment is stale until the shell is re-entered, and a stale one has no GTK.

### Building and testing

```sh
cargo build                                   # the whole workspace
cargo test                                    # every crate
cargo test -p wolf-core                       # one crate
cargo run -p wolf-core --example grid_dump    # headless dump of the rendered grid
nix develop --command cargo run -p wolf-gnome # the GTK application
```

Most tests spawn the pinned Neovim as a real child process rather than a mock, and they stay headless: `nvim --embed` speaks msgpack-RPC over stdio and needs no TTY, so the suite is safe to run unattended. Cargo discovers every `tests/*.rs`, so nothing needs wiring on this path.

`cargo test` also runs the rustdoc doctests in `src/**`, which `rust_test` does not. A doctest is therefore Linux-only coverage, the exact mirror of the Bazel wiring gap under [Running the tests](#running-the-tests): a fenced or indented block in a `//!` or `///` comment compiles as Rust unless it is tagged `text` or `ignore`, and a prose list that only looks like code fails there and nowhere else.

What each target covers lives with the tests: [`wolf-core/tests`](crates/wolf-core/tests/README.md), [`wolf-ffi/tests`](crates/wolf-ffi/tests/README.md), [`wolf-gnome/tests`](crates/wolf-gnome/tests/README.md). The GTK end-to-end smokes are `#[ignore]`d, because they launch the real window under a headless Wayland compositor; the wolf-gnome tests README has the invocation.

## macOS (Bazel)

### Prerequisites

- macOS Tahoe (26) or newer with Xcode 26+ installed at `/Applications/Xcode.app`. The Bazel C and link toolchain (apple_support) drives the host Xcode SDK through `DEVELOPER_DIR`, which both `.bazelrc` and the dev shell set. This matters because `rusqlite`'s bundled SQLite is compiled from C through `cc`.
- Nix with flakes, plus `direnv` and `nix-direnv`, as on Linux.

### Activating the dev environment

The same `.envrc` selects the Darwin shell:

```sh
direnv allow
```

The Darwin shell supplies bazelisk (exposed as both `bazelisk` and a `bazel` shim), git, starpls, and exports `DEVELOPER_DIR`. It deliberately carries no Rust toolchain and no GUI packages: `rules_rust` downloads a pinned rustc (1.85.0), and the Rust dependencies resolve from the workspace `Cargo.toml` and `Cargo.lock` through `crate_universe`. Every command below is written `bazel`, which is the bazelisk shim.

bazelisk reads the Bazel version from `.bazelversion` (8.7.0) and fetches it. `MODULE.bazel` (bzlmod) pins the rule sets and is authoritative for their versions; `MODULE.bazel.lock` and `Cargo.Bazel.lock` are committed, so a clean checkout builds without resolving anything.

### Building

```sh
bazel build //crates/wolf-core:wolf_core
bazel build //crates/wolf-ffi:wolf_ffi
```

### Repinning Rust dependencies

After changing a crate's Cargo dependencies, regenerate the `crate_universe` lock by repinning against any Rust target:

```sh
CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-core:wolf_core
```

Commit the updated `Cargo.Bazel.lock` and `MODULE.bazel.lock`. Repin after a feature change too, not only after adding a crate: enabling a feature moves the Bazel lockfile even when no node joins the graph.

Bazel discovers no files on its own, which costs two things a Cargo-only change would never notice. A new `tests/*.rs` needs a `rust_test` target (see [Running the tests](#running-the-tests)), and a new non-Rust asset that a module `include_str!`s needs the owning `rust_library`'s `compile_data` glob widened — an asset outside that glob compiles under Cargo and fails under Bazel. See [`crates/wolf-core/src/markdown/README.md`](crates/wolf-core/src/markdown/README.md) for the live example.

## Building the macOS app

Two targets produce the same `Wolf.app`, code-signed ad-hoc for development, with a minimum OS of 26.0. They differ only in configuration.

### The development build

```sh
bazel build //crates/wolf-macos:Wolf
bazel run   //crates/wolf-macos:Wolf
```

This is the fast-iteration build: fastbuild, Swift at `-Onone`, Rust unoptimized. Every test target and the generated Xcode project build against it. It is substantially slower at runtime than the optimized build, so its timings are not what a user experiences.

### The optimized build

```sh
bazel build //crates/wolf-macos:WolfRelease
ZIP=$(bazel cquery --output=files //crates/wolf-macos:WolfRelease 2>/dev/null)
APP="${ZIP%.zip}_archive-root/Wolf.app"
```

`:WolfRelease` produces the same bundle as `:Wolf` with the whole graph — Swift, Rust, and the treesitter parsers — built at `-c opt`, without `-c opt` on the command line. Build this for real use, for installing, and for any performance measurement. The rule, and the reason it is a transition rather than an alias, are documented in [`crates/wolf-macos/release.bzl`](crates/wolf-macos/release.bzl) and [`wolf-macos/DECISIONS.md`](crates/wolf-macos/DECISIONS.md).

**Resolve the path from `cquery` every time; `bazel-bin` is wrong for `WolfRelease`.** `bazel-bin` symlinks the bin directory of the top-level target's configuration, and the transition applies below that edge, so the optimized bundle is never under it. The trap is a build of `:WolfRelease`, a later build of `:Wolf`, and then the habitual `bazel-bin` path, which resolves again and silently hands over the `-Onone` bundle. The `cquery` output resolves under an `-opt-` output directory whose name embeds a platform hash that is not a stable contract, so use the command rather than a literal path.

For both targets `macos_application` declares `Wolf.zip` as its output; the unzipped bundle under `<name>_archive-root/` is an undeclared sibling that the bundling and signing action leaves in the same directory. Nothing tracks it, so if it is ever missing, `ditto -x -k "$ZIP" "${ZIP%.zip}_archive-root"` recreates it.

`WolfRelease` is tagged `manual`, so no wildcard build or test sweeps it in and the standard path never rebuilds the graph a second time at `-c opt`. An explicit `build` or `run` on the label works normally. The consequence is that the first `WolfRelease` build is a full rebuild in a second configuration — every Rust crate, both Swift modules, the treesitter parsers, actool, code signing — which roughly doubles the on-disk output footprint. Incremental rebuilds within either configuration are unaffected.

Nothing automated guards the transition, and the failure mode is silent: a broken one launches a working `-Onone` app. Two checks:

```sh
bazel aquery --output=text 'mnemonic("ExecutableSymlink", //crates/wolf-macos:WolfRelease)'
ls -l "$APP/Contents/MacOS/Wolf"
```

The aquery action's input path must contain `-opt-`; a `-fastbuild-` input is the bug. The action's own configuration reads `darwin_arm64-fastbuild`, which is the top-level target's config and is expected. The size check is the conclusive one: the main binary is about 8.26 MB optimized against about 25.4 MB for fastbuild, roughly three times smaller.

### The measured penalty

Measured on the dev host with the same capture protocol (steady-state window, typing only), fastbuild against `-c opt`:

| bucket        | default (`-Onone`) | `-c opt` (`-O`) | change |
| ------------- | ------------------ | --------------- | ------ |
| `ctrl.wake`   | 4.85 ms            | 0.51 ms         | -89%   |
| `prev.render` | 25.63 ms           | 6.17 ms         | -76%   |
| `draw`        | 21.87 ms           | 14.64 ms        | -33%   |
| `pane.glyphs` | 18.96 ms           | 14.00 ms        | -26%   |
| `pane.read`   | 0.67 ms            | 0.07 ms         | -90%   |
| `prev.html`   | 2.23 ms            | 2.31 ms         | ~0%    |

Main-thread busy falls from 73% to 34%; per keystroke, roughly 70 ms to roughly 36 ms. Two buckets act as controls and are what make this credible: `prev.html` is flat because it is a `loadHTMLString` call into WebKit, a system framework that is already optimized, and `pane.glyphs` gains least because its cost is dominated by CoreText. `prev.render` at -76% is the Rust side, so the transition is not a Swift-only story.

### Regenerating the swift-bridge bindings

The Swift and C bindings under `crates/wolf-ffi/generated/` are committed. After changing the `#[swift_bridge::bridge]` module, regenerate and commit them:

```sh
bazel run //crates/wolf-ffi:gen_bindings
```

`gen_bindings` writes in-tree because it reads `BUILD_WORKSPACE_DIRECTORY`, which only `bazel run` provides — a genrule cannot write into the source tree.

### Generating the Xcode project

```sh
bazel run //crates/wolf-macos:xcodeproj
```

The output `.xcodeproj` is uncommitted and `.gitignore`d, and exists for LLDB and IDE debugging only.

### What the bundle ships

The `.app` is self-contained, so a double-clicked bundle runs with no shell environment: it carries its own pinned Neovim and `init.lua`, ripgrep, the vendored Neovim plugins and colorscheme, the compiled treesitter parsers, both font families, and the app icon. [`crates/wolf-macos/README.md`](crates/wolf-macos/README.md) has the inventory, with a link to the `//bazel` package behind each entry, and `bundle_layout_test` asserts it. Exported `WOLF_NVIM_BIN`, `WOLF_RG_BIN` and `WOLF_INIT_LUA` values override their bundled counterparts, so a development override still wins.

## Running the tests

This section is the macOS Bazel path. On Linux the whole suite is `cargo test`, under [Building and testing](#building-and-testing) above.

Let Bazel expand the pattern rather than naming targets one by one:

```sh
bazel test //...              # everything
bazel test //crates/...       # the Rust and Swift crate tests
bazel test //bazel/...        # the vendored-asset tree tests
```

Targets tagged `manual` are excluded from wildcard expansion — the optimized `//crates/wolf-macos:WolfRelease` build and every `//tools/lint` target — so a plain `bazel test //...` never pulls in an opt rebuild or the advisory linters. Run those explicitly.

Tests run in the default configuration, not `-c opt`, and that is deliberate rather than an oversight. `bazel test -c opt //crates/wolf-macos:WolfKeyMapTests` fails to build, because `@testable import` requires `-enable-testing`, which rules_swift passes only in non-opt compilation modes. Test in the default configuration and ship the optimized build. `//crates/wolf-macos:bundle_layout_test` runs against `:Wolf` for a related reason: bundle layout is configuration-independent, so a release variant would add a full opt build to every sweep for no added coverage.

Bazel does not discover test files. A new `tests/*.rs` or `*Tests.swift` runs only once it is wired into a target in the owning `BUILD.bazel`, and until then it is a silent macOS-only gap that `cargo test` hides. The rule is in [`CLAUDE.md`](CLAUDE.md); the runfiles and tags each kind of Rust test needs are in the "Adding a test file" sections of [`wolf-core/tests`](crates/wolf-core/tests/README.md) and [`wolf-ffi/tests`](crates/wolf-ffi/tests/README.md), and the Swift `srcs` list is described in [`Tests/WolfKeyMapTests`](crates/wolf-macos/Tests/WolfKeyMapTests/README.md).

What each target covers belongs with the code it tests:

| Suite                                                                | Covers                                                             |
| -------------------------------------------------------------------- | ------------------------------------------------------------------ |
| [`//crates/wolf-core:*`](crates/wolf-core/tests/README.md)           | Core against a real pinned Neovim, plus the headless pure modules. |
| [`//crates/wolf-ffi:*`](crates/wolf-ffi/tests/README.md)             | The bridge surface, most of it live across the FFI boundary.       |
| [`//crates/wolf-macos:WolfKeyMapTests`](crates/wolf-macos/README.md) | The AppKit-free Swift seams, through XCTest.                       |
| `//crates/wolf-macos:bundle_layout_test`                             | The built `.app`'s resource layout.                                |
| [`//bazel/...`](#repinning-a-vendored-asset)                         | One test per vendored asset, asserting the assembled tree.         |

## Repinning a vendored asset

Every bundled third-party asset is downloaded by a bzlmod module extension under `bazel/`, sha256-pinned, with no system or `PATH` dependency. Each package owns its pin, its assembly, and its bump procedure:

| Package                                                                              | Pinned                            | Test                                     |
| ------------------------------------------------------------------------------------ | --------------------------------- | ---------------------------------------- |
| [`//bazel/nvim`](bazel/nvim/README.md)                                               | Neovim, per-arch release tarball  | `:nvim_version_test`                     |
| [`//bazel/rg`](bazel/rg/README.md)                                                   | ripgrep, per-arch release tarball | `:rg_version_test`                       |
| [`//bazel/orlando`](bazel/orlando/README.md)                                         | The orlando plugin                | `:orlando_tree_test`                     |
| [`//bazel/lighthouse`](bazel/lighthouse/README.md)                                   | The lighthouse colorscheme        | `:lighthouse_tree_test`                  |
| [`//bazel/nvim-treesitter`](bazel/nvim-treesitter/README.md)                         | The markdown queries provider     | `:nvim_treesitter_tree_test`             |
| [`//bazel/nvim-treesitter-textobjects`](bazel/nvim-treesitter-textobjects/README.md) | The textobjects plugin            | `:nvim_treesitter_textobjects_tree_test` |
| [`//bazel/treesitter-parsers`](bazel/treesitter-parsers/README.md)                   | The host-compiled grammars        | `:parsers_tree_test`                     |
| [`//bazel/jetbrains-mono`](bazel/jetbrains-mono/README.md)                           | JetBrains Mono                    | `:jetbrains_mono_tree_test`              |
| [`//bazel/ia-writer-mono`](bazel/ia-writer-mono/README.md)                           | iA Writer Mono S                  | `:ia_writer_mono_tree_test`              |

Every bump has the same four steps, and each package README gives the exact commands for its own archive layout: change the version or commit in that package's `extensions.bzl`, recompute and update the sha256, regenerate and commit `MODULE.bazel.lock` (a plain build does this, since `.bazelrc` sets `--lockfile_mode=update`), then re-run that package's test.

Two of them move together. `//bazel/nvim-treesitter` and `//bazel/treesitter-parsers` are bumped in one change so the bundled queries and the compiled parsers stay version-matched, and that bump also re-runs `//crates/wolf-ffi:treesitter_parser_abi` and `//crates/wolf-ffi:treesitter_highlight` — the proofs that a shipped parser is ABI-compatible with the pinned Neovim and that markdown highlighting with fenced-code injection is live.

## Logging (`WOLF_LOG`)

`wolf-core` emits `tracing` diagnostics gated by `WOLF_LOG`, using standard [`EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) directive syntax and defaulting to `warn`. The process-global subscriber writes to stderr and is installed on the first `wolf_ffi_start` call. Because `bazel run` launches the inner binary with stderr inherited, the lines land in the terminal:

```sh
WOLF_LOG=wolf_core=debug bazel run //crates/wolf-macos:Wolf
```

The default `warn` level surfaces the RPC-timeout warning naming the wedged call. `wolf_core=debug` adds per-RPC timing and file-search root resolution; `trace` adds the file-traversal batch markers. See [`crates/wolf-core/src/README.md`](crates/wolf-core/src/README.md) for what the subscriber carries. This is unrelated to `WOLF_FRAME_STATS` below, which is Swift instrumentation rather than `tracing`.

## Frame-time instrumentation (macOS)

The macOS front-end carries an off-by-default frame-time measurement surface, enabled with `WOLF_FRAME_STATS=1`, which reports periodic aggregates to stderr. `WOLF_FRAME_STATS_INTERVAL` sets the report period in seconds and `WOLF_FRAME_STATS_FILE` also appends each report to a file. The bucket set and the report format are documented in [`Sources/WolfKeyMap/README.md`](crates/wolf-macos/Sources/WolfKeyMap/README.md), and the two caveats worth knowing before drawing a conclusion from a report are in [`wolf-macos/DECISIONS.md`](crates/wolf-macos/DECISIONS.md).

**Double-clicking `Wolf.app` shows nothing.** A bundle launched from the Finder is started by `launchd`, so it inherits `launchd`'s stderr, which reaches no terminal, and it does not inherit the shell environment either — so the flag would not even be set.

**Measure `WolfRelease`, not `:Wolf`.** Numbers from the development build are `-Onone` Swift and unoptimized Rust; several buckets differ by three to ten times (see the table above), so they are comparable to nothing captured under `-c opt`.

The capture path is to exec the bundle's inner binary directly. It inherits the terminal's stderr and environment while `Bundle.main` still resolves to the `.app`, so the bundled Neovim, fonts and `init.lua` load normally:

```sh
bazel build //crates/wolf-macos:WolfRelease
ZIP=$(bazel cquery --output=files //crates/wolf-macos:WolfRelease 2>/dev/null)
APP="${ZIP%.zip}_archive-root/Wolf.app"
WOLF_FRAME_STATS=1 "$APP/Contents/MacOS/Wolf"
```

`bazel run //crates/wolf-macos:WolfRelease` also launches the app and does pass the environment through, since the rules_apple runner invokes the inner binary as a direct child rather than opening the bundle. It is a convenient launch but a poor measurement: it unzips a temporary copy to a path that changes every launch, which macOS treats as a different app for TCC prompts and saved window state, and its output interleaves with Bazel's client output. Reports are activity-gated, so an idle app prints the enabled header and nothing more.

For an app that is genuinely double-clicked, or for a persistent log to `diff` between runs, write to a file and tail it:

```sh
WOLF_FRAME_STATS=1 WOLF_FRAME_STATS_FILE=/tmp/wolf-frame-stats.txt \
  "$APP/Contents/MacOS/Wolf" &
tail -f /tmp/wolf-frame-stats.txt
```

## Linting and formatting

The lint and format targets live in [`//tools/lint`](tools/lint/README.md): clippy and rustfmt for Rust, swift-format for Swift, and a markdown checker for the documentation rules in [`CLAUDE.md`](CLAUDE.md). Each has a check target run with `bazel test` and, where a fixer exists, a fix target run with `bazel run`.

```sh
bazel test //tools/lint:check   # every check
bazel run  //tools/lint:fix     # every fixer, in place
```

Every one of them is tagged `manual`, which is why `bazel test //...` leaves them alone and why the standard test path is unaffected by them. They are advisory rather than gating, and existing violations across the tree are expected: a check target passing means the linter ran, not that the tree is clean. The per-tool table, the toolchain each one resolves, and the caveats are in [`tools/lint/README.md`](tools/lint/README.md).

Linux has no Bazel, so the Rust half runs through the devShell's own clippy and rustfmt instead:

```sh
cargo clippy --workspace   # --fix to apply the suggestions
cargo fmt --check          # drop --check to rewrite in place
```

These cover `wolf-gnome` too, which the Bazel targets deliberately skip. swift-format has no Cargo equivalent and needs a macOS host.

The markdown checker also runs standalone, which is how to check a document on a host without Bazel:

```sh
./tools/lint/markdown_check.sh <files>
```

Its Bazel target covers the per-package `README.md` and `DECISIONS.md` files only. The workspace documents — this file, [`architecture.md`](architecture.md), and the decision reports — are outside that filegroup, so check them by hand with the script above.

## The dependency boundary

`wolf-core` must compile with no GUI toolkit in its dependency tree. That rule, the denylist, and the two commands that verify it under Cargo and under Bazel belong to [`crates/wolf-core/DECISIONS.md`](crates/wolf-core/DECISIONS.md). Re-run both after any dependency change, and repin `crate_universe` as described above.

`wolf-gnome` is deliberately given no Bazel target, which is what keeps the GTK stack out of the Bazel graph; see [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md).
