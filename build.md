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

The Linux shell supplies the pinned Rust toolchain (`rust-src`, `rust-analyzer`, `clippy`, `rustfmt`), a pinned hermetic Neovim exported as `WOLF_NVIM_BIN`, a pinned ripgrep exported as `WOLF_RG_BIN`, the GTK4, libadwaita and WebKitGTK stack `wolf-gnome` needs, and a generated `FONTCONFIG_FILE` that pins the font stack to three vendored families — DejaVu plus the two the shared `wolf-config.yaml` can name — with no host font directory, so the offscreen golden render is reproducible.

The `shellHook` generates that file by substituting store paths into `$PWD/crates/wolf-gnome/testdata/fonts.conf`, a repo-relative template, so every `nix develop` command in this document is run from the repository root. Started anywhere else the substitution has no template to read, and the font pinning every golden render rests on is gone. Do not reach for a system `cargo`, `rustc`, `nvim` or `rg` — `flake.nix` pins all four, and builds and tests must run inside the shell.

`WOLF_NVIM_BIN` is the only way core and its tests locate Neovim: never `PATH`, never a scan of `/nix/store`. Unset, `WolfCore::start` fails with `NvimError::BinNotSet` and the wolf-core integration tests panic loudly rather than skip. `WOLF_RG_BIN` locates ripgrep the same way; unset, content search degrades to an empty session carrying an error.

Two more exports name vendored Neovim assets rather than binaries. `WOLF_NVIM_TREESITTER_DIR` points at a runtimepath-shaped nvim-treesitter tree carrying `queries/`, `lua/` and `plugin/` as siblings, assembled because upstream splits the queries under `runtime/` from the modules and the loader at its root, and `WOLF_PARSERS_DIR` at a `symlinkJoin` of sixteen compiled grammars laid out as `parser/<lang>.so`. The captive `init.lua` reads both as the fallback half of its treesitter gate — see [`runtime/nvim`](runtime/nvim/README.md), which owns the resolution order and what the gate delivers.

Both trees are fetched, not compiled. Measured with `nix build --dry-run` against an empty chroot store: 66 paths fetched, 22.2 MiB of download, and the only local builds are the `symlinkJoin` and the tree assembly, neither of which runs a compiler. `nix path-info` against `cache.nixos.org` agrees, every grammar reporting a substitute. No cold wall clock was ever taken — the measuring host's store was already warm — so this document quotes none, and neither should anything derived from it.

After editing `flake.nix`, run through `nix develop --command`. An already-loaded direnv environment is stale until the shell is re-entered, and a stale one has no GTK.

### Running the packaged release build

To run the packaged Nix release build without adding it as an input to your NixOS configuration, run these commands:

```sh
nix build .#wolf-gnome
env -u FONTCONFIG_FILE ./result/bin/wolf-gnome
```

`nix build` resolves `.` from the repository root, which is where an active direnv has already exported the devShell's pinned `FONTCONFIG_FILE`. The wrapper sets that variable as a default, so an exported one wins, and the launch draws its GTK chrome — sidebar, menu, pickers — from the pinned DejaVu stack rather than the desktop's own UI font, which that configuration cannot see. Scrubbing the variable is what lets the wrapper's additive fragment apply and makes the run represent an installed one.

### Building and testing

```sh
cargo build                                   # the whole workspace
cargo test                                    # every crate
cargo test -p wolf-core                       # one crate
cargo run -p wolf-core --example grid_dump    # headless dump of the rendered grid
nix develop --command cargo run -p wolf-gnome # the GTK application, unoptimized
```

These select cargo's default profiles — `dev` for a build or a run, `test` for a test target — and write into `target/debug`; [the optimized build](#the-optimized-build) is a separate command and a separate output directory.

Most tests spawn the pinned Neovim as a real child process rather than a mock, and they stay headless: `nvim --embed` speaks msgpack-RPC over stdio and needs no TTY, so the suite is safe to run unattended. Cargo discovers every `tests/*.rs`, so nothing needs wiring on this path.

`cargo test` also runs the rustdoc doctests in `src/**`, which `rust_test` does not. A doctest is therefore Linux-only coverage, the exact mirror of the Bazel wiring gap under [Running the tests](#running-the-tests): a fenced or indented block in a `//!` or `///` comment compiles as Rust unless it is tagged `text` or `ignore`, and a prose list that only looks like code fails there and nowhere else.

What each target covers lives with the tests: [`wolf-core/tests`](crates/wolf-core/tests/README.md), [`wolf-ffi/tests`](crates/wolf-ffi/tests/README.md), [`wolf-gnome/tests`](crates/wolf-gnome/tests/README.md). The GTK end-to-end smokes run in that suite rather than opting in: they launch the real window under `cage`, a headless Wayland compositor, and drive it with `wtype`, `grim`, `tesseract` and [`wolf-pointer-inject`](tools/wolf-pointer-inject/README.md). All four come from the devShell, and the smokes fail loudly rather than skipping when one is missing, since absence means the suite is running outside `nix develop`. They also need a set and writable `XDG_RUNTIME_DIR`, which is where `cage` puts its Wayland socket.

### The optimized build

From the repository root, for the reason [Activating the dev environment](#activating-the-dev-environment) gives, and because a checkout binary resolves the captive `init.lua` out of the checkout it was compiled in — the last of the three branches [`wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md) states, and the one [the Nix package](#the-nix-package) does not take:

```sh
nix develop --command cargo build --release
nix develop --command cargo run --release -p wolf-gnome
nix develop --command ./target/release/wolf-gnome  # no cargo in the process tree; what a profiling session wants
```

The profile is optimized, unwinding, and carries the line tables a profiler and a release panic backtrace need to resolve a `file:line`. The per-key reasons, and the list of keys deliberately left unset, belong to the comment above `[profile.release]` in the workspace [`Cargo.toml`](Cargo.toml), which owns that rationale — read it before retuning anything.

No step of [the verification gate](#the-verification-gate) builds this profile. `cargo test`, `cargo clippy --workspace --all-targets` and `cargo build --workspace --all-targets` all select `dev` and `test`, so an optimized build never sweeps into an ordinary run — the Linux counterpart of `WolfRelease`'s `manual` tag, by a different mechanism to the same end. Measured across a full gate run: every `Finished` line reads `dev` or `test`, and `target/release/wolf-gnome` keeps its timestamp.

`cargo test --release` reuses those same `target/release` artifacts, because the `bench` profile inherits `release`. That reuse is what `panic = "unwind"` buys, for the reason the manifest comment gives.

Two commands are the release evidence:

```sh
nix develop --command cargo test --release -p wolf-gnome --test release_profile -- --nocapture
nix develop --command cargo test --release -p wolf-gnome --test smoke_headless -- \
  --exact window_renders_typed_text_under_cage \
          preview_pane_renders_and_edit_source_returns_under_cage
```

What each of them covers belongs with the tests, in [`wolf-gnome/tests`](crates/wolf-gnome/tests/README.md).

Measured on the 32-core dev host with `cargo 1.96.0` and `rustc 1.96.0`. A cold `cargo build --release` — `target/release` absent, 167 crates — took `real 0m52.610s` and `user 6m19.665s`, and `target/release` came to 1.4 GB.

The binary is 45,656,208 bytes from a whole-workspace build, of which about 31.3 MiB is the debug info `debug = "line-tables-only"` retains — every `.debug*` section it emits rather than the line table alone: the same profile with `debug = false` yields 12,791,936 bytes. A `-p wolf-gnome` build differs by 7,424 bytes through feature unification, so quote a size only together with the build scope it came from.

Runtime measurement belongs to this profile as well: [Frame-time instrumentation](#frame-time-instrumentation) is off by default and measures the keystroke-to-pixel path, and a `dev` build's numbers are comparable to nothing a user experiences.

Outside the devShell the binary still resolves every library it links: `ldd` reports no missing entry, all 188 of them resolving through store RPATHs written by nixpkgs' linker wrapper. What is missing is the environment, and it fails along two different paths.

With no display it prints `Gtk-WARNING: Failed to open display` and exits 1. With a display but none of the devShell's exports it prints host fontconfig warnings, then a `GLib-GIO-CRITICAL` about `g_settings_schema_source_lookup` which is a warning rather than an abort, then `failed to start core: WOLF_NVIM_BIN is not set`, then `no window could be opened…` — and exits 0 having opened no window. That zero belongs to the second path alone.

### The Nix package

`packages.wolf-gnome`, and `packages.default` as the same derivation, is the installed Linux application. It exists on Linux systems only, behind an `optionalAttrs` guard that keeps a Wayland GTK derivation off the Darwin evaluation path.

```sh
nix build .#wolf-gnome --no-link --print-out-paths
```

`--no-link` keeps a `result` symlink out of the working tree and prints the store path on stdout instead. Do not record that path as a fact: the derivation is input-addressed over everything under `crates/`, so editing a comment in a test file gives a different output. A cold build is a full release compile of the workspace with no `target/` reuse, so budget minutes rather than seconds.

| Path                    | Contents                                                                |
| ----------------------- | ----------------------------------------------------------------------- |
| `bin/`                  | the wrapper and the binary it execs                                     |
| `share/wolf/nvim/`      | the captive `init.lua`, with sibling `orlando/` and `lighthouse/` trees |
| `share/wolf/fonts.conf` | the additive fontconfig fragment `FONTCONFIG_FILE` defaults to          |
| `share/applications/`   | `org.wolf.Wolf.desktop`                                                 |
| `share/icons/hicolor/`  | `org.wolf.Wolf` at 48, 64, 128 and 256 px, plus a scalable SVG          |
| `share/fonts/truetype/` | iA Writer Mono S and JetBrains Mono                                     |
| `share/licenses/`       | iA Writer Mono S's licence                                              |

The wrapper sets `WOLF_NVIM_BIN`, `WOLF_RG_BIN`, `WOLF_NVIM_TREESITTER_DIR`, `WOLF_PARSERS_DIR` and `FONTCONFIG_FILE` as defaults, so an exported value of any of them still wins — the rule `Project.swift` follows on macOS with `setenv(…, 0)`, and what keeps the devShell's pinned font stack winning inside `nix develop`. It never sets `WOLF_INIT_LUA`, which would make the installed runtime unreachable and the development override useless on the packaged binary.

A NixOS configuration consumes the flake as an input:

```nix
{
  inputs.wolf.url = "github:jplein/wolf";
  # then, from a module that has the flake in scope:
  environment.systemPackages = [ wolf.packages.${pkgs.system}.wolf-gnome ];
}
```

Listing it in `fonts.packages` as well makes the two editor families available system-wide, because they are installed at the layout fontconfig scans. That is a convenience and not a requirement: the application resolves them through its own `FONTCONFIG_FILE` either way.

Nothing about the artifact is checked inside the build sandbox. `doCheck` is off, because the suite needs a compositor, a writable `XDG_RUNTIME_DIR` and a session bus a Nix builder has none of, so one command builds the package and runs every test that reads or launches it:

```sh
./tools/wolf-package-smoke.sh
```

It prints the store path it resolved before it starts, since that path is the subject of everything the run reports, and it must not overlap another `cargo test`: two compositors contend for one Wayland socket. What those tests assert belongs with them, in [`wolf-gnome/tests`](crates/wolf-gnome/tests/README.md).

### The verification gate

This is the definition of "Linux is green", and unlike the advisory macOS lint targets every step gates: `-D warnings` applies unconditionally and each command must exit 0.

```sh
nix develop --command bash -c '
  set -euo pipefail
  cargo fmt --all -- --check
  prettier --check "**/*.md"
  cargo clippy --workspace --all-targets -- -D warnings
  cargo build --workspace --all-targets
  cargo test --workspace
'
```

The order is deliberate. The two format checks compile nothing, so a stray blank line fails in seconds rather than after a long compile; clippy precedes the build because it compiles the same dependency graph the build then reuses; and the tests run last because they dominate the wall clock. Expect roughly 10-25 minutes cold, most of it compilation and the integration tests that each spawn a real Neovim.

Three details are load-bearing. `--all-targets` on clippy is what makes it lint `tests/*.rs` and every `#[cfg(test)] mod tests` rather than just `--lib` and `--bins`. `cargo test` omits `--all-targets` deliberately, because that flag silently drops doctests. And running through `nix develop --command` is not optional: after a `flake.nix` edit the loaded direnv environment is stale and lacks any newly added tool.

Prettier's scope is `**/*.md` minus [`.prettierignore`](.prettierignore). Quote the glob so the shell does not expand it — prettier expands it internally and only then applies the ignore file. `.direnv/` must stay excluded, because it symlinks into the Nix store and prettier's globber follows symlinks.

## macOS (Bazel)

None of this can be built or verified from the Linux devShell. Work done on Linux-only branches that still needs re-checking here is logged in [macos-followups.md](macos-followups.md).

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

### Installing the app

```sh
bazel run //crates/wolf-macos:InstallWolf
```

Builds `:WolfRelease` and installs it to `~/Applications/Wolf.app`, replacing any bundle already there. Because the installer depends on `:WolfRelease` rather than on `:Wolf`, the installed app is always the optimized one; there is no separate build step to forget and no way to install an `-Onone` bundle by mistake.

The installer extracts the archive, not the `<name>_archive-root/Wolf.app` directory beside it, since that directory is untracked and cannot be carried in runfiles. It extracts with `ditto`, which preserves the symlinks, exec bits and code signature that make the bundle launchable, and it extracts to a staging directory next to the destination before removing the installed app, so an interrupted run leaves the existing installation intact.

Like `:WolfRelease` itself the target is tagged `manual`, so it never sweeps into a wildcard build.

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

The default `warn` level surfaces the RPC-timeout warning naming the wedged call. `wolf_core=debug` adds per-RPC timing and file-search root resolution; `trace` adds the file-traversal batch markers. See [`crates/wolf-core/src/README.md`](crates/wolf-core/src/README.md) for what the subscriber carries. This is unrelated to `WOLF_FRAME_STATS` below, which is each front-end's own instrumentation rather than `tracing`.

## Frame-time instrumentation

Both front-ends carry an off-by-default measurement of the keystroke-to-pixel path, over the same three variables: `WOLF_FRAME_STATS=1` arms it, `WOLF_FRAME_STATS_INTERVAL` sets the report period in seconds, and `WOLF_FRAME_STATS_FILE` appends every report to a file as well as writing it to stderr. The line prefix and the column layout are identical on both, so two reports `diff` as text. Reports are activity-gated, so an idle application prints the enabled header and nothing more.

### macOS

The bucket set, the report format and the per-bucket caveats are documented in [`Sources/WolfKeyMap/README.md`](crates/wolf-macos/Sources/WolfKeyMap/README.md); the caveat that applies to a report as a whole, and the pointer to the rest, are in [`wolf-macos/DECISIONS.md`](crates/wolf-macos/DECISIONS.md).

**Double-clicking `Wolf.app` shows nothing.** A bundle launched from the Finder is started by `launchd`, so it inherits `launchd`'s stderr, which reaches no terminal, and it does not inherit the shell environment either — so the flag would not even be set.

**Measure `WolfRelease`, not `:Wolf`.** Numbers from the development build are `-Onone` Swift and unoptimized Rust; several buckets differ by three to ten times (see the table above), so they are comparable to nothing captured under `-c opt`.

The capture path is to exec the bundle's inner binary directly. It inherits the terminal's stderr and environment while `Bundle.main` still resolves to the `.app`, so the bundled Neovim, fonts and `init.lua` load normally:

```sh
bazel build //crates/wolf-macos:WolfRelease
ZIP=$(bazel cquery --output=files //crates/wolf-macos:WolfRelease 2>/dev/null)
APP="${ZIP%.zip}_archive-root/Wolf.app"
WOLF_FRAME_STATS=1 "$APP/Contents/MacOS/Wolf"
```

`bazel run //crates/wolf-macos:WolfRelease` also launches the app and does pass the environment through, since the rules_apple runner invokes the inner binary as a direct child rather than opening the bundle. It is a convenient launch but a poor measurement: it unzips a temporary copy to a path that changes every launch, which macOS treats as a different app for TCC prompts and saved window state, and its output interleaves with Bazel's client output.

For an app that is genuinely double-clicked, or for a persistent log to `diff` between runs, write to a file and tail it:

```sh
WOLF_FRAME_STATS=1 WOLF_FRAME_STATS_FILE=/tmp/wolf-frame-stats.txt \
  "$APP/Contents/MacOS/Wolf" &
tail -f /tmp/wolf-frame-stats.txt
```

### Linux

The bucket set is [`wolf-gnome/src/README.md`](crates/wolf-gnome/src/README.md#frame_statsrs)'s; the cadence, the `wakes/key` caveat and the measured reading are [`wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md#frame-time-instrumentation)'s.

**Measure the release binary, and leave it where cargo put it.** A `dev` build's numbers mean nothing, and a checkout binary reaches its captive `init.lua` through the `CARGO_MANIFEST_DIR` branch, so a copy moved elsewhere — with no installed `share/wolf/nvim/` beside it and no `WOLF_INIT_LUA` naming one — is launched against a path to nothing.

```sh
nix develop --command cargo build --release
nix develop --command env WOLF_FRAME_STATS=1 ./target/release/wolf-gnome
```

For a log to `diff` between runs, write to a file and tail it:

```sh
nix develop --command env WOLF_FRAME_STATS=1 WOLF_FRAME_STATS_INTERVAL=2 \
  WOLF_FRAME_STATS_FILE=/tmp/wolf-frame-stats.txt \
  ./target/release/wolf-gnome &
tail -f /tmp/wolf-frame-stats.txt
```

**On a host with no display session, drive it under `cage` and `wtype`** — the recipe the measured reading came from. Give `cage` a fixture project directory outside the checkout holding one large prose file, with `HOME` set to that directory's parent and `DBUS_SESSION_BUS_ADDRESS` removed, so the run cannot touch the real `~/.local/state/wolf` or `~/.config/wolf`. The scenario then settles, opens the file, jumps to its end, and types several `wtype -d 30` bursts over roughly thirty seconds.

**Do not set `WOLF_FONT_PT`** in that scenario. The smokes' 28 pt exists for OCR and would understate every per-cell cost. Record the grid size, the font size, the pane count and whether a preview was open beside any number taken this way, and run each scenario twice: only a difference larger than the spread between two runs of the same scenario is a signal.

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

The asymmetry between the platforms is deliberate: these targets are advisory on macOS, whereas [the verification gate](#the-verification-gate) runs the stricter `--all-targets` forms on Linux and gates on them.

The markdown checker also runs standalone, which is how to check a document on a host without Bazel:

```sh
./tools/lint/markdown_check.sh <files>
```

Its Bazel target covers the per-package `README.md` and `DECISIONS.md` files only. The workspace documents — this file, [`architecture.md`](architecture.md), and the decision reports — are outside that filegroup, so check them by hand with the script above.

## The dependency boundary

`wolf-core` must compile with no GUI toolkit in its dependency tree. That rule, the denylist, and the two commands that verify it under Cargo and under Bazel belong to [`crates/wolf-core/DECISIONS.md`](crates/wolf-core/DECISIONS.md). Re-run both after any dependency change, and repin `crate_universe` as described above.

`wolf-gnome` is deliberately given no Bazel target, which is what keeps the GTK stack out of the Bazel graph; see [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md).
