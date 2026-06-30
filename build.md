# Building Wolf

This covers building and testing Wolf. On Linux (NixOS) the `wolf-core` Rust core and the `wolf-gnome` GTK front-end build with Cargo under Nix. On macOS, Bazel (via bazelisk, using the host Xcode SDK) builds the Rust core, the `wolf-ffi` swift-bridge crate, and the `wolf-macos` AppKit `.app` — the macOS app requires macOS Tahoe (26) or newer and Xcode 26+.

## Building on NixOS (Linux, Cargo)

### Prerequisites

- **NixOS (Linux).** The Linux dev environment is the `flake.nix` devShell.
- **Nix with flakes enabled** (`experimental-features = nix-command flakes`).
- **[`direnv`](https://direnv.net/) + [`nix-direnv`](https://github.com/nix-community/nix-direnv).**

### Activating the dev environment

The repo ships an `.envrc` that runs `use flake` (on Linux and on macOS).
Activate it once per checkout:

```sh
direnv allow
```

This enters the flake devShell, which provides:

- the **pinned Rust toolchain** (stable, with `rust-src`, `rust-analyzer`,
  `clippy`, `rustfmt`), and
- a **pinned, hermetic `nvim`** binary, exported as the environment variable
  **`WOLF_NVIM_BIN`**.

`WOLF_NVIM_BIN` is the **only** way core and its tests locate Neovim. Core never
falls back to `PATH` and never scans `/nix/store`. If the variable is unset or
empty, `WolfCore::start` fails with `NvimError::BinNotSet` and the integration
tests panic loudly rather than skip.

Do not use a system `cargo`/`rustc`/`nvim`. The toolchain and the Neovim binary
are pinned by `flake.nix`; all builds and tests must run inside the devShell.

### Building

```sh
cargo build        # builds the whole workspace
```

### Testing

```sh
cargo test         # unit + integration tests for wolf-core
```

The integration tests spawn the **pinned** Neovim (via `WOLF_NVIM_BIN`) as a
real child process — there are no mocks, per the project's testing policy. They
are fully **headless**: `nvim --embed` speaks msgpack-RPC over stdio and needs no
TTY, so the suite is CI-safe. The tests fail loudly (they do not silently pass)
if `WOLF_NVIM_BIN` is unset.

You can also run the headless smoke example, which starts the core, types a
little text, and dumps the resulting grid:

```sh
cargo run -p wolf-core --example grid_dump
```

## Building on macOS (Bazel)

On macOS the canonical build tool is **Bazel**, driven by **bazelisk**, and it builds with the **host Xcode SDK** (not the older SDK shipped inside Nix). It builds `wolf-core`, the `wolf-ffi` swift-bridge crate, and the `wolf-macos` AppKit `.app` (see "Building the macOS app" below). The `.app` bundles its own pinned Neovim 0.12.3 (see "Bundled Neovim (macOS)" below), so the packaged app no longer needs `WOLF_NVIM_BIN` to run.

### Prerequisites

- **macOS** with **Xcode installed** at `/Applications/Xcode.app`. The Bazel C/link toolchain (apple_support) drives the host Xcode SDK via `DEVELOPER_DIR`, which the dev shell sets to `/Applications/Xcode.app/Contents/Developer`. This matters because `rusqlite`'s `bundled` SQLite is compiled from C via `cc`.
- **Nix with flakes enabled** and **[`direnv`](https://direnv.net/) + [`nix-direnv`](https://github.com/nix-community/nix-direnv)** (same as Linux).

### Activating the dev environment

The same `.envrc` runs `use flake` on macOS, selecting the Darwin dev shell. Activate it once per checkout:

```sh
direnv allow
```

The Darwin shell provides **bazelisk** (exposed as both `bazelisk` and `bazel`) and exports `DEVELOPER_DIR`. It deliberately carries **no** Rust toolchain — `rules_rust` downloads a pinned `rustc`, and the Rust dependencies are resolved from the existing `Cargo.toml`/`Cargo.lock` via `rules_rust`'s `crate_universe`. There is no system `cargo`/`rustc` requirement on macOS.

bazelisk reads the Bazel version pinned in `.bazelversion` and fetches it; the Bazel build itself is configured by `MODULE.bazel` (bzlmod) with committed lockfiles (`MODULE.bazel.lock`, `Cargo.Bazel.lock`).

### Building

```sh
bazel build //crates/wolf-core:wolf_core
```

### Repinning Rust dependencies

After changing `wolf-core`'s Cargo dependencies (`Cargo.toml`/`Cargo.lock`), regenerate the `crate_universe` lock by repinning:

```sh
CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-core:wolf_core
```

Commit the updated `Cargo.Bazel.lock` (and `MODULE.bazel.lock`). A clean checkout then builds without needing to repin.

The `wolf-ffi` crate adds the `swift-bridge` / `swift-bridge-build` (0.1.59) Cargo dependencies, so the same `CARGO_BAZEL_REPIN` workflow applies after touching its `Cargo.toml`; repin against either target (e.g. `CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-ffi:wolf_ffi`) and commit the lockfiles.

## Building the macOS app (Bazel)

The macOS app builds entirely under Bazel — **Bazel is authoritative**; the generated Xcode project (below) is for debugging only. This requires macOS Tahoe (26) or newer and Xcode 26+. The resolved rule pins are `rules_swift` 3.6.1, `rules_apple` 4.5.3, and `rules_xcodeproj` 4.1.0 (with `apple_support` held at 2.6.1 so the host-Xcode toolchain still resolves).

### Building the .app

```sh
bazel build //crates/wolf-macos:Wolf
```

This produces `Wolf.app`, code-signed (dev ad-hoc), with a minimum OS of 26.0. The app bundles its own pinned Neovim 0.12.3 + the captive `init.lua` in `Contents/Resources/` (see "Bundled Neovim (macOS)" below), so double-clicking it works with no shell environment. A developer's exported `WOLF_NVIM_BIN` (and `WOLF_INIT_LUA`) still overrides the bundled binary (and `init.lua`).

### Regenerating the swift-bridge bindings

The Swift + C bindings under `crates/wolf-ffi/generated/` are **committed**. After changing the `#[swift_bridge::bridge]` module in `crates/wolf-ffi/src/lib.rs`, regenerate and commit them:

```sh
bazel run //crates/wolf-ffi:gen_bindings
```

`gen_bindings` writes in-tree (it reads `BUILD_WORKSPACE_DIRECTORY`, which only `bazel run` provides — a genrule cannot write into the source tree).

### Generating the Xcode project (debugging only)

```sh
bazel run //crates/wolf-macos:xcodeproj
```

The output `.xcodeproj` is **uncommitted** (`.gitignore`d) and exists only for LLDB / IDE debugging. It is never the build of record — Bazel remains authoritative.

### Bundled Neovim (macOS)

The macOS `.app` ships its own pinned Neovim so it runs with no external Neovim install and no `WOLF_NVIM_BIN`. The pinned version (**0.12.3**) and both per-arch sha256 checksums live in [`bazel/nvim/extensions.bzl`](bazel/nvim/extensions.bzl); the `//bazel/nvim` module extension downloads and verifies the official release tarballs (no system or `PATH` dependency). `//bazel/nvim:nvim_tree` is the arch-selected (`select()` on `@platforms//cpu`), structure-preserving `nvim/` tree — `bin/nvim` + `lib/nvim/parser/*.so` (including the `markdown` / `markdown_inline` treesitter parsers) + `share/nvim/runtime` — that `//crates/wolf-macos:Wolf` bundles into `Contents/Resources/nvim/`. The captive `runtime/nvim/init.lua` is bundled flat at `Contents/Resources/init.lua`. `aspect_bazel_lib` was added as a `bazel_dep` (build-time only — it never enters `wolf-core`'s dependency graph) for `copy_to_directory`, which preserves the tree hierarchy and the executable bits. See [`bazel/nvim/README.md`](bazel/nvim/README.md) for the overlay/select details.

At launch `AppDelegate` sets `WOLF_NVIM_BIN` to the bundled `Contents/Resources/nvim/bin/nvim` **only if it is not already set**, so a developer's exported `WOLF_NVIM_BIN` still wins (likewise `WOLF_INIT_LUA` overrides the bundled `init.lua`). Bundling is **arm64-tested** on the dev host; the x86_64 tree is wired via `select()` but untested. The bundled binary is the **official release build** (ad-hoc / linker-signed Mach-O); the app's `codesign --verify --deep --strict` passes for dev — there is no Developer ID signing or notarization (out of scope).

To **bump the pinned Neovim version**:

1. Change the version in the release URLs in [`bazel/nvim/extensions.bzl`](bazel/nvim/extensions.bzl) (`_VERSION`).
2. Recompute **both** sha256 checksums — `shasum -a 256 nvim-macos-arm64.tar.gz` and `shasum -a 256 nvim-macos-x86_64.tar.gz` — and update `_ARM64_SHA256` / `_X86_64_SHA256` (and the per-arch archive-root names if the release directory name changes).
3. Regenerate and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/nvim:nvim_version_test` (and update its expected version, which lives in the test script).

### Test targets

```sh
bazel test //bazel/nvim:nvim_version_test        # execs the bundled pinned nvim
bazel test //crates/wolf-macos:bundle_layout_test # asserts the .app resource layout
bazel test //crates/wolf-ffi:frame_pack          # headless, deterministic
bazel test //crates/wolf-ffi:bridge_smoke        # real nvim; see the note below
bazel test //crates/wolf-macos:WolfKeyMapTests   # pure Swift key translation
```

- `//bazel/nvim:nvim_version_test` execs the pinned, bundled Neovim tree: it asserts the binary exists and is executable, that `nvim --version` reports `NVIM v0.12.3`, that `VIMRUNTIME` self-locates **inside** the bundled tree, and that the `markdown` treesitter parser ships.
- `//crates/wolf-macos:bundle_layout_test` inspects the built `.app` and asserts the resource layout: `Contents/Resources/nvim/bin/nvim` (executable), `…/nvim/lib/nvim/parser/markdown.so`, `…/nvim/share/nvim/runtime` (a directory), and `Contents/Resources/init.lua`.
- `//crates/wolf-ffi:frame_pack` builds a `wolf_core::Snapshot` by hand (no nvim) and asserts `FfiFrame`'s color packing, flag bits, reverse-video cursor, and cell accessors. It is deterministic and runs **regardless** of `WOLF_NVIM_BIN`.
- `//crates/wolf-ffi:bridge_smoke` drives the Rust half of the bridge against a real pinned Neovim (no mocks). It runs for real on Linux/Cargo devShells. On the macOS Bazel host it currently **hangs** when pointed at a real nvim — including against a separately-installed system nvim 0.12.3 — so it is a **pre-existing, host-wide** issue in the bridge's read/wake pump or the test harness, unrelated to the Neovim bundling and under investigation. It is therefore not yet a passing macOS proxy for the live nvim→FFI round-trip.
- `//crates/wolf-macos:WolfKeyMapTests` is a pure Swift `XCTest` over the AppKit-free `WolfKeyMap` key-translation seam — no `NSEvent`, no window — and runs regardless of `WOLF_NVIM_BIN`.

## The one hard rule and how it is enforced

> **`wolf-core` must compile with no GUI toolkit in its dependency tree.**

This is Wolf's analog of a dependency guard. Enforce it by inspecting
`wolf-core`'s (non-dev) dependency tree and confirming none of the denied crates
appear:

```sh
cargo tree -e no-dev -p wolf-core
```

The denylist (no occurrence of any of these is permitted):

```
gtk gtk4 glib gdk4 cairo cairo-rs pango libadwaita
objc objc2 cocoa core-graphics winit wgpu skia-safe softbuffer
```

If any of these appears in core's tree, the platform boundary has leaked and the
build is wrong.

The same rule holds under the Bazel build on macOS: `wolf-gnome` is deliberately given no Bazel target, so GTK never enters `wolf-core`'s Bazel dependency graph. Verify it with a `bazel query` over the resolved deps:

```sh
bazel query 'deps(//crates/wolf-core:wolf_core)' | grep -iE \
  'gtk|gdk|glib|cairo|pango|adwaita|objc|cocoa|core-graphics|winit|wgpu|skia|softbuffer' \
  || echo clean
```

## Notes

- The captive Neovim is pinned and hermetic from day one (see
  [architecture.md](architecture.md) §8): on Linux via the flake devShell, on
  macOS via the Neovim bundled inside the `.app` (above). Either way it runs the
  pinned binary with the repo's `runtime/nvim/init.lua`.
- Full `wrapNeovim`-style plugin baking beyond what the macOS release ships — a
  Markdown LSP such as `marksman` and a formatter — is **deferred to a later
  phase**. The current build relies on Neovim's markdown syntax highlighting and
  the `markdown` / `markdown_inline` treesitter parsers the release carries.
