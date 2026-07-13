# Building Wolf

This covers building and testing Wolf. On Linux (NixOS) the `wolf-core` Rust core and the `wolf-gnome` GTK front-end build with Cargo under Nix. On macOS, Bazel (via bazelisk, using the host Xcode SDK) builds the Rust core, the `wolf-ffi` swift-bridge crate, and the `wolf-macos` AppKit `.app` — the macOS app requires macOS Tahoe (26) or newer and Xcode 26+.

## Building on NixOS (Linux, Cargo)

### Prerequisites

- **NixOS (Linux).** The Linux dev environment is the `flake.nix` devShell.
- **Nix with flakes enabled** (`experimental-features = nix-command flakes`).
- **[`direnv`](https://direnv.net/) + [`nix-direnv`](https://github.com/nix-community/nix-direnv).**

### Activating the dev environment

The repo ships an `.envrc` that runs `use flake` (on Linux and on macOS). Activate it once per checkout:

```sh
direnv allow
```

This enters the flake devShell, which provides:

- the **pinned Rust toolchain** (stable, with `rust-src`, `rust-analyzer`, `clippy`, `rustfmt`), and
- a **pinned, hermetic `nvim`** binary, exported as the environment variable **`WOLF_NVIM_BIN`**.

`WOLF_NVIM_BIN` is the **only** way core and its tests locate Neovim. Core never falls back to `PATH` and never scans `/nix/store`. If the variable is unset or empty, `WolfCore::start` fails with `NvimError::BinNotSet` and the integration tests panic loudly rather than skip.

Do not use a system `cargo`/`rustc`/`nvim`. The toolchain and the Neovim binary are pinned by `flake.nix`; all builds and tests must run inside the devShell.

### Building

```sh
cargo build        # builds the whole workspace
```

### Testing

```sh
cargo test         # unit + integration tests for wolf-core
```

The integration tests spawn the **pinned** Neovim (via `WOLF_NVIM_BIN`) as a real child process — there are no mocks, per the project's testing policy. They are fully **headless**: `nvim --embed` speaks msgpack-RPC over stdio and needs no TTY, so the suite is CI-safe. The tests fail loudly (they do not silently pass) if `WOLF_NVIM_BIN` is unset.

You can also run the headless smoke example, which starts the core, types a little text, and dumps the resulting grid:

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

The `wolf-core` `markdown` module's LaTeX-math render adds `pulldown-latex` (`0.7`) — added via the same `CARGO_BAZEL_REPIN` workflow above (regen `Cargo.lock`, repin against `//crates/wolf-core:wolf_core`, commit `Cargo.Bazel.lock` / `MODULE.bazel.lock`). It is pure Rust and pulls only `bumpalo` into core's tree, so the GUI-toolkit denylist re-check (below / [`crates/wolf-core/README.md`](crates/wolf-core/README.md)) still passes. The same module also bundles two woff2 fonts as single-line base64 sidecars that it `include_str!`s at compile time, so `//crates/wolf-core:wolf_core`'s `compile_data` glob is extended to `glob(["src/markdown/**/*.css", "src/markdown/assets/*.b64"])` — the `assets/*.b64` sidecars must be visible to rustc (the raw woff2 / license texts are provenance-only and are not compiled in). See [`crates/wolf-core/src/markdown/README.md`](crates/wolf-core/src/markdown/README.md) and [`preview-math-and-fonts-decisions.md`](preview-math-and-fonts-decisions.md).

## Building the macOS app (Bazel)

The macOS app builds entirely under Bazel — **Bazel is authoritative**; the generated Xcode project (below) is for debugging only. This requires macOS Tahoe (26) or newer and Xcode 26+. The resolved rule pins are `rules_swift` 3.6.1, `rules_apple` 4.5.3, and `rules_xcodeproj` 4.1.0 (with `apple_support` held at 2.6.1 so the host-Xcode toolchain still resolves).

### Building the .app

```sh
bazel build //crates/wolf-macos:Wolf
```

This produces `Wolf.app`, code-signed (dev ad-hoc), with a minimum OS of 26.0. The app bundles its own pinned Neovim 0.12.3 + the captive `init.lua` in `Contents/Resources/` (see "Bundled Neovim (macOS)" below), so double-clicking it works with no shell environment. A developer's exported `WOLF_NVIM_BIN` (and `WOLF_INIT_LUA`) still overrides the bundled binary (and `init.lua`).

The app icon comes from an Icon Composer bundle `Wolf.icon/` that is assembled at build time by `bazel_skylib` `copy_file` targets in `crates/wolf-macos/BUILD.bazel`: the committed `crates/wolf-macos/Wolf.icon.json` becomes `Wolf.icon/icon.json`, and the full-bleed 1024×1024 light and dark PNGs are read directly from `resources/icons/` (no committed copies); the three outputs are wired into the `Wolf` target via the `app_icons` attribute. actool (Xcode 26) compiles it into `Contents/Resources/Assets.car` and merges `CFBundleIconName = Wolf` into the Info.plist; on macOS Tahoe the icon switches automatically between the light and dark artwork with the system's icon-appearance setting. With `minimum_os_version = "26.0"` the `.icon` bundle is the only route rules_apple accepts (a legacy `.appiconset` is a hard build failure). To regenerate the artwork, see [`resources/icons/README.md`](resources/icons/README.md).

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

The `.app` now also bundles a **vendored orlando plugin** at `Contents/Resources/orlando/`. The `//bazel/orlando` module extension downloads the sha256-pinned GitHub source tarball (pure Lua, arch-independent — a single repo, no `select()`) and assembles it into a structure-preserving `orlando/` `TreeArtifact`, reusing `aspect_bazel_lib`'s `copy_to_directory` (build-time only — it never enters `wolf-core`'s dependency graph). The captive `runtime/nvim/init.lua` loads it conditionally: it self-locates a sibling `orlando/` dir next to itself and, only when that dir exists, prepends it to `runtimepath` and sources `plugin/orlando.lua`; when it is absent (Linux/cargo, or any test that does not stage it) the block is a strict no-op. Vendoring orlando is **Bazel-only for now** — the Linux path will vendor it separately later. See [`bazel/orlando/README.md`](bazel/orlando/README.md) for the pin and how it is assembled.

The `.app` also bundles a **vendored lighthouse colorscheme** at `Contents/Resources/lighthouse/`. The `//bazel/lighthouse` module extension downloads the sha256-pinned GitHub source tarball (pure Lua, arch-independent — a single repo, no `select()`) and assembles it into a structure-preserving `lighthouse/` `TreeArtifact` the same way. Unlike orlando (a plugin auto-sourced from `plugin/`), lighthouse is a **colorscheme** (`colors/lighthouse.lua`), which is inert on `runtimepath` until it is explicitly selected. The captive `runtime/nvim/init.lua` therefore **activates** it conditionally: it self-locates a sibling `lighthouse/` dir next to itself and, only when that dir exists, prepends it to `runtimepath`, sets `vim.o.background = "dark"`, and runs `vim.cmd.colorscheme("lighthouse")` — **after `syntax on`**, because both `:colorscheme` and `syntax on` clear/redefine highlight groups, so the colorscheme must run last to win; when the dir is absent (Linux/cargo, or any test that does not stage it) the block is a strict no-op. A future change will switch dark/light automatically from the OS appearance preference. Vendoring lighthouse is **Bazel-only for now** — the Linux path will vendor it separately later. See [`bazel/lighthouse/README.md`](bazel/lighthouse/README.md) for the pin and how it is assembled.

The `.app` also bundles a **vendored nvim-treesitter plugin** at `Contents/Resources/nvim-treesitter/`. The `//bazel/nvim-treesitter` module extension downloads the sha256-pinned GitHub source tarball (pure Lua, arch-independent — a single repo, no `select()`) and assembles it into a structure-preserving `nvim-treesitter/` `TreeArtifact` (bundled `queries/**` — lifted from main's `runtime/queries/**` — + `lua/nvim-treesitter/**` + `plugin/**`) the same way. The pin is on nvim-treesitter's **`main`** branch: master's markdown `injections.scm` uses the `#set-lang-from-info-string!` predicate, which is incompatible with pinned nvim 0.12.3's query-match API (`E5108` on every fenced redraw), whereas main's markdown injections are pure core-native capture-based and route fenced blocks correctly from the queries on `runtimepath` alone. See [`bazel/nvim-treesitter/README.md`](bazel/nvim-treesitter/README.md) for the pin and details.

The `.app` also bundles a **vendored nvim-treesitter-textobjects plugin** at `Contents/Resources/nvim-treesitter-textobjects/` (bundled `lua/nvim-treesitter-textobjects/**` — the `main`-branch self-named layout — + `queries/**/textobjects.scm`), assembled the same way from a sha256-pinned source tarball, pinned to `main` for branch-consistency with nvim-treesitter. It is bundled-but-inert (no keymaps wired). See [`bazel/nvim-treesitter-textobjects/README.md`](bazel/nvim-treesitter-textobjects/README.md).

The `.app` also bundles a curated set of **host-compiled treesitter parsers** at `Contents/Resources/parsers/parser/<lang>.so`. The `//bazel/treesitter-parsers` module extension downloads sha256-pinned grammar source tarballs and **compiles** each grammar's `src/parser.c` (+ optional `src/scanner.c`) into a signable, relocatable `MH_DYLIB` Mach-O shared object with the host apple_support / Xcode cc toolchain (a nix-built `.so` would embed non-relocatable `/nix/store` install names), then merges them into a structure-preserving `parsers/` `TreeArtifact`. The curated set is: html, javascript, typescript, tsx, css, json, go, rust, c, cpp, yaml, toml, bash, swift, kotlin, java (markdown / markdown_inline are excluded — pinned nvim 0.12.3 already ships them). Grammar revisions come from the nvim-treesitter `lockfile.json` as pinned during the original vendoring — when the plugin was on `master` (master-era) — and repos from that commit's `parsers.lua`; they were **not** re-synced when the plugin later moved to `main`. Re-deriving the parser revisions from `main`'s `lockfile.json` for an exact queries⇄parsers version-match is a future nicety: the current pairing is proven working (the `treesitter_highlight` injection test and `treesitter_parser_abi` both pass) and tree-sitter grammar ABIs are stable, so queries-from-`main` with parsers-from-master-era is fine. See [`bazel/treesitter-parsers/README.md`](bazel/treesitter-parsers/README.md) for the `ts_grammar` template, the per-grammar status table, and the drop+LOG policy.

On macOS these three treesitter artifacts (the two plugins and the compiled parsers) are now **ACTIVE in the `.app`**: the captive `runtime/nvim/init.lua` self-locates the sibling `nvim-treesitter/` + `parsers/` trees and, when present, prepends them to `runtimepath` and runs a pcall-wrapped `vim.treesitter.start` on every markdown buffer (a `FileType=markdown` autocmd + emit-once), so markdown highlighting **and** fenced-code-block **injection** (core-native, capture-based, from main's markdown `injections.scm` on `runtimepath`) are enabled. The nvim-treesitter-textobjects tree is prepended but stays **inert** (no keymaps). On Linux/cargo — and any test that does not stage the sibling trees — the block is a strict no-op (`syntax on` remains). Still **deferred**: the Linux/Nix vendoring path for all three (mirroring how orlando/lighthouse were vendored Bazel-only first), textobjects keymaps, and a Markdown LSP/formatter. The ABI compatibility of the compiled parsers with pinned nvim 0.12.3 is proven by `//crates/wolf-ffi:treesitter_parser_abi`, and the live markdown highlighter + injected `go` langtree by `//crates/wolf-ffi:treesitter_highlight`.

### Bundled fonts (macOS)

The macOS `.app` bundles **TWO vendored fonts** into the single `Contents/Resources/fonts/` directory. The **JetBrains Mono font** ships its four static desktop TTF faces (`JetBrainsMono-{Regular,Italic,Bold,BoldItalic}.ttf`) plus `OFL.txt`: the `//bazel/jetbrains-mono` module extension downloads the sha256-pinned GitHub release-asset zip (JetBrains Mono **v2.304**; DATA, arch-independent — a single repo, no `select()`, unlike `//bazel/nvim`) and assembles the four faces + the license into a structure-preserving `fonts/` `TreeArtifact` via `aspect_bazel_lib`'s `copy_to_directory` (build-time only — it never enters `wolf-core`'s dependency graph). Its release zip is **FLAT** (no wrapping top dir, unlike orlando's source tarball), so the overlay remaps `fonts/ttf` → `fonts` and `OFL.txt` → `fonts/OFL.txt` via `replace_prefixes` to land the inner `fonts/` dir that `rules_apple` needs. The **iA Writer Mono S font** ships its four static faces (`iAWriterMonoS-{Regular,Italic,Bold,BoldItalic}.ttf`) plus `LICENSE.md`: the `//bazel/ia-writer-mono` module extension downloads a sha256-pinned GitHub **commit source tarball** (the `iaolo/iA-Fonts` repo has no release assets, so — like orlando — it pins `/archive/<commit>.tar.gz` with a wrapping `iA-Fonts-<commit>/` top dir) and assembles the four faces + the license into its own `fonts/` `TreeArtifact`; because the four faces sit under a spaced source dir (`iA Writer Mono/Static/`), its overlay lists the sources EXPLICITLY and remaps `iA Writer Mono/Static` → `fonts` and `iA Writer Mono/LICENSE.md` → `fonts/LICENSE.md`. Both TreeArtifacts use an inner `fonts/` dir, so `rules_apple` **merges** them into the one `Contents/Resources/fonts/` (their file sets are disjoint, no collision). Unlike the nvim-side assets, the fonts are consumed by **Swift**, not `init.lua`: `FontRegistration.swift` CoreText-registers `Contents/Resources/fonts/*.ttf` at launch (`CTFontManagerRegisterFontURLs`) so `CellMetrics` resolves both families — **iA Writer Mono S is the config-driven DEFAULT face**, JetBrains Mono is bundled + registered but non-default (Menlo is the fallback). Vendoring the fonts is **Bazel-only for now** (macOS-only this increment). See [`bazel/jetbrains-mono/README.md`](bazel/jetbrains-mono/README.md) and [`bazel/ia-writer-mono/README.md`](bazel/ia-writer-mono/README.md) for the pins and how they are assembled.

The macOS front-end reads a user-level **`wolf-config.yaml`** preferences file (font-family / font-size / line-height) at the platform preferences dir (`~/Library/Preferences/Wolf`), parsed in the GUI-free `wolf-core` `config` module and bridged to Swift over `wolf-ffi` (`wolf_ffi_load_config` / `FfiConfig`); `AppDelegate` registers the bundled fonts, loads the config, then measures the cell metrics from it. See [`crates/wolf-core/src/README.md`](crates/wolf-core/src/README.md) and [`config-decisions-report.md`](config-decisions-report.md).

To **bump the pinned Neovim version**:

1. Change the version in the release URLs in [`bazel/nvim/extensions.bzl`](bazel/nvim/extensions.bzl) (`_VERSION`).
2. Recompute **both** sha256 checksums — `shasum -a 256 nvim-macos-arm64.tar.gz` and `shasum -a 256 nvim-macos-x86_64.tar.gz` — and update `_ARM64_SHA256` / `_X86_64_SHA256` (and the per-arch archive-root names if the release directory name changes).
3. Regenerate and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/nvim:nvim_version_test` (and update its expected version, which lives in the test script).

To **bump the vendored orlando plugin**:

1. Update `_COMMIT` in [`bazel/orlando/extensions.bzl`](bazel/orlando/extensions.bzl) (the archive root name auto-follows the commit).
2. Recompute `_SHA256`: `curl -L https://github.com/jplein/orlando/archive/<commit>.tar.gz -o orlando.tar.gz && shasum -a 256 orlando.tar.gz`.
3. Regenerate and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/orlando:orlando_tree_test`.

To **bump the vendored lighthouse colorscheme**:

1. Update `_COMMIT` in [`bazel/lighthouse/extensions.bzl`](bazel/lighthouse/extensions.bzl) (the archive root name — `lighthouse.nvim-<commit>`, the upstream repo name includes `.nvim` — auto-follows the commit).
2. Recompute `_SHA256`: `curl -L https://github.com/jplein/lighthouse.nvim/archive/<commit>.tar.gz -o lighthouse.tar.gz && shasum -a 256 lighthouse.tar.gz`.
3. Regenerate and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/lighthouse:lighthouse_tree_test`.

To **bump the vendored JetBrains Mono font**:

1. Update `_VERSION` in [`bazel/jetbrains-mono/extensions.bzl`](bazel/jetbrains-mono/extensions.bzl) (the release URL + inner filenames auto-follow).
2. Recompute `_SHA256`: `curl -L https://github.com/JetBrains/JetBrainsMono/releases/download/v<version>/JetBrainsMono-<version>.zip -o f.zip && shasum -a 256 f.zip`; confirm the flat layout + the exact `fonts/ttf/JetBrainsMono-<Face>.ttf` paths with `unzip -l f.zip` (and update the `runtime_files` filegroup / `replace_prefixes` if a face is renamed or relocated).
3. Regenerate and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/jetbrains-mono:jetbrains_mono_tree_test`.

To **bump the vendored iA Writer Mono S font**:

1. Update `_COMMIT` in [`bazel/ia-writer-mono/extensions.bzl`](bazel/ia-writer-mono/extensions.bzl) (the archive root name — `iA-Fonts-<commit>` — auto-follows the commit).
2. Recompute `_SHA256`: `curl -L https://github.com/iaolo/iA-Fonts/archive/<commit>.tar.gz -o f.tgz && shasum -a 256 f.tgz`; confirm the (spaced) source paths with `tar -tzf f.tgz | grep 'iA Writer Mono/Static'` — verify the exact `iA Writer Mono/Static/iAWriterMonoS-<Face>.ttf` filenames + `iA Writer Mono/LICENSE.md` (and update the explicit `runtime_files` filegroup / `replace_prefixes` if a face is renamed or relocated).
3. Regenerate and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/ia-writer-mono:ia_writer_mono_tree_test`.

To **bump the vendored nvim-treesitter plugin + compiled parsers** (bump these together so the bundled queries and the compiled parsers stay version-matched):

1. Update `_COMMIT`/`_SHA256` in [`bazel/nvim-treesitter/extensions.bzl`](bazel/nvim-treesitter/extensions.bzl) to a new `main`-branch commit (`curl -L https://github.com/nvim-treesitter/nvim-treesitter/archive/<commit>.tar.gz -o nvts.tar.gz && shasum -a 256 nvts.tar.gz`). Keep the `runtime/queries` → `queries` overlay remap (main relocates the queries under `runtime/`).
2. From that commit's `lockfile.json` (per-lang `revision`) and `lua/nvim-treesitter/parsers.lua` (per-lang repo url), update each grammar's `rev` + `sha256` in [`bazel/treesitter-parsers/extensions.bzl`](bazel/treesitter-parsers/extensions.bzl), **RE-VERIFYING the scanner extension** (C vs C++, whether one exists) at each new revision.
3. Optionally bump [`bazel/nvim-treesitter-textobjects/extensions.bzl`](bazel/nvim-treesitter-textobjects/extensions.bzl) similarly (`main` branch, for branch-consistency).
4. Regenerate and commit `MODULE.bazel.lock`.
5. Re-run `bazel test //bazel/nvim-treesitter:nvim_treesitter_tree_test //bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree_test //bazel/treesitter-parsers:parsers_tree_test //crates/wolf-ffi:treesitter_parser_abi //crates/wolf-ffi:treesitter_highlight`.

If any grammar stops compiling hermetically at the new revision, apply the **drop+LOG** policy (remove it from the extension, `use_repo`, and the assembly, and record it in [`bazel/treesitter-parsers/README.md`](bazel/treesitter-parsers/README.md)'s status table) rather than blocking the bump.

### Test targets

```sh
bazel test //bazel/nvim:nvim_version_test        # execs the bundled pinned nvim
bazel test //bazel/orlando:orlando_tree_test     # asserts the vendored orlando tree contents
bazel test //bazel/lighthouse:lighthouse_tree_test # asserts the vendored lighthouse tree contents
bazel test //bazel/nvim-treesitter:nvim_treesitter_tree_test # asserts the vendored nvim-treesitter tree
bazel test //bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree_test # asserts the textobjects tree
bazel test //bazel/treesitter-parsers:parsers_tree_test # asserts the compiled-parser tree
bazel test //bazel/jetbrains-mono:jetbrains_mono_tree_test # asserts the vendored JetBrains Mono tree
bazel test //bazel/ia-writer-mono:ia_writer_mono_tree_test # asserts the vendored iA Writer Mono S tree
bazel test //crates/wolf-macos:bundle_layout_test # asserts the .app resource layout
bazel test //crates/wolf-core:state_unit         # state tree store/load/state_dir, no nvim, deterministic
bazel test //crates/wolf-core:empty_state_cwd    # real nvim, launch cwd NOT recorded into folder-MRU
bazel test //crates/wolf-core:theme              # real nvim, WolfCursor/WolfTitlebar theme tracking
bazel test //crates/wolf-ffi:config              # config load/create, no nvim, deterministic
bazel test //crates/wolf-ffi:state               # headless, no nvim: snapshot-builder tree round-trip
bazel test //crates/wolf-ffi:theme               # real nvim, WolfCursor/WolfTitlebar theme bridge surface
bazel test //crates/wolf-ffi:frame_pack          # headless, deterministic
bazel test //crates/wolf-ffi:bridge_smoke        # real nvim, live round-trip; see the note below
bazel test //crates/wolf-ffi:orlando_load        # real nvim, loads the vendored orlando plugin
bazel test //crates/wolf-ffi:lighthouse_load     # real nvim, activates the vendored lighthouse colorscheme
bazel test //crates/wolf-ffi:treesitter_parser_abi # real nvim, proves a compiled parser's ABI compat
bazel test //crates/wolf-ffi:treesitter_highlight # real nvim, markdown highlighter active + go injection
bazel test //crates/wolf-ffi:attribute_highlight # real nvim, per-cell attribute flags + special color across the FFI
bazel test //crates/wolf-macos:WolfKeyMapTests   # pure Swift key translation + glyph-style seam
```

- `//bazel/nvim:nvim_version_test` execs the pinned, bundled Neovim tree: it asserts the binary exists and is executable, that `nvim --version` reports `NVIM v0.12.3`, that `VIMRUNTIME` self-locates **inside** the bundled tree, and that the `markdown` treesitter parser ships.
- `//bazel/orlando:orlando_tree_test` asserts the assembled orlando tree contents: `plugin/orlando.lua` and `lua/orlando/init.lua` land at the expected paths inside the tree's inner `orlando/` dir.
- `//bazel/lighthouse:lighthouse_tree_test` asserts the assembled lighthouse tree contents: `colors/lighthouse.lua` lands at the expected path inside the tree's inner `lighthouse/` dir.
- `//bazel/nvim-treesitter:nvim_treesitter_tree_test` asserts the assembled nvim-treesitter tree contents: a markdown query file (`queries/markdown/injections.scm`, or `highlights.scm`) and the `lua/nvim-treesitter/` module dir land inside the tree's inner `nvim-treesitter/` dir.
- `//bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree_test` asserts the assembled textobjects tree contents: the `lua/nvim-treesitter-textobjects/` module dir (the `main`-branch self-named layout) and at least one `queries/**/textobjects.scm` land inside the tree's inner `nvim-treesitter-textobjects/` dir.
- `//bazel/treesitter-parsers:parsers_tree_test` asserts the assembled compiled-parser tree: HARD-asserts `parser/{go,rust,typescript,tsx,json}.so` are non-empty inside the tree's inner `parsers/` dir; SOFT-notes swift/kotlin/java (drop+LOG policy).
- `//bazel/jetbrains-mono:jetbrains_mono_tree_test` asserts the assembled JetBrains Mono tree: the four TTF faces (`JetBrainsMono-{Regular,Italic,Bold,BoldItalic}.ttf`) land non-empty and `OFL.txt` is present inside the tree's inner `fonts/` dir.
- `//bazel/ia-writer-mono:ia_writer_mono_tree_test` asserts the assembled iA Writer Mono S tree: the four TTF faces (`iAWriterMonoS-{Regular,Italic,Bold,BoldItalic}.ttf`) land non-empty and `LICENSE.md` (the iA SIL OFL, named `LICENSE.md` not `OFL.txt`) is present inside the tree's inner `fonts/` dir.
- `//crates/wolf-macos:bundle_layout_test` inspects the built `.app` and asserts the resource layout: `Contents/Resources/nvim/bin/nvim` (executable), `…/nvim/lib/nvim/parser/markdown.so`, `…/nvim/share/nvim/runtime` (a directory), `Contents/Resources/init.lua`, the vendored orlando resource files (`Contents/Resources/orlando/plugin/orlando.lua` and `…/orlando/lua/orlando/init.lua`), the vendored lighthouse colorscheme (`Contents/Resources/lighthouse/colors/lighthouse.lua`), the vendored nvim-treesitter markdown query (check h) + textobjects Lua dir (check i), the curated compiled treesitter parsers (check j: `Contents/Resources/parsers/parser/<lang>.so`, HARD for go/rust/typescript/tsx/json, SOFT for swift/kotlin/java), the bundled JetBrains Mono fonts (checks k–o: the four `Contents/Resources/fonts/JetBrainsMono-*.ttf` faces + `Contents/Resources/fonts/OFL.txt`), and the bundled iA Writer Mono S fonts (checks p–t: the four `Contents/Resources/fonts/iAWriterMonoS-*.ttf` faces + `Contents/Resources/fonts/LICENSE.md`) — the two TreeArtifacts merging into the one `fonts/` dir.
- `//crates/wolf-core:state_unit` covers the `wolf_core::state` module **without** any Neovim (explicit tempdirs, deterministic): the whole-tree `StateStore::set_snapshot` → `flush` → `load_state` round-trip (a 2-window / 3-tab tree read back verbatim), OLD-FLAT-FORMAT rejection (a pre-tabs window with no `tabs` → `None`, bytes preserved, no migration path), the identical-snapshot no-rewrite (no generation bump) contract, a later snapshot replacing the prior tree, the silently-ignore matrix (absent/corrupt/wrong-version `state.json` → defaults, on-disk bytes preserved — `load_state` never creates or modifies the file), that a written file carries `version: 1`, and the `WOLF_STATE_DIR` override.
- `//crates/wolf-core:empty_state_cwd` drives a real pinned Neovim, TRIMMED (plan OD1) to its folder-MRU hygiene assertions: core no longer mirrors cwd into a `StateStore` (`attach_state_store` + the `redraw.rs::apply_cwd` store-mirror are removed — the front-end reads each tab's cwd at snapshot time), so what remains observable core-side is that the one-shot startup `wolf_cwd` emit updates the tracked-cwd cell ONLY and does NOT record the launch dir into `folder-mru.sqlite`, while a genuine later typed `:cd` (a real `DirChanged`) still records into the recent-dirs MRU.
- `//crates/wolf-ffi:state` is HEADLESS (no nvim; a single `#[test]`, since it mutates the process-global `WOLF_STATE_DIR` and opens the process-wide `SHARED_STORE`): `wolf_ffi_load_state()` on empty/corrupt dirs yields zero windows without touching the files; the write-side snapshot builder (`wolf_ffi_state_snapshot_begin` / `add_window` / `add_tab` / `commit` / `wolf_ffi_state_flush`) lands a 2-window / 3-tab tree (geometry + per-tab cwd/sidebar/order) in the pretty-printed version-1 `state.json`, round-tripping through the `FfiWindowState` / `FfiTabState` accessors, plus old-flat-format rejection.
- `//crates/wolf-ffi:config` covers the config surface **without** any Neovim: it points `WOLF_CONFIG_DIR` at a tempdir and asserts `wolf_ffi_load_config()` round-trips a written `wolf-config.yaml` and falls back to the built-in defaults when the file is absent. Self-contained and deterministic — it stages no nvim, so it runs regardless of `WOLF_NVIM_BIN`.
- `//crates/wolf-core:theme` drives a real pinned Neovim (no mocks, same runfile staging): `Snapshot.theme` follows the `wolf_theme` notification — an all-`None` startup with no Wolf groups, a runtime `:hi WolfCursor`/`:hi WolfTitlebar` + `:doautocmd ColorScheme` update, a linked group resolving to effective attributes, and the cleared-group fallback back to all-`None`.
- `//crates/wolf-ffi:theme` drives a real pinned Neovim through the theme bridge surface: `theme_titlebar_bg()` / `theme_titlebar_fg()` start unset, carry the exact packed `:hi WolfTitlebar` colors after `:doautocmd ColorScheme`, and revert to unset on `:hi clear`; the themed `WolfCursor` colors ride the existing `cursor_fill()` / `cursor_glyph()` frame accessors and return to the reverse-video baseline when cleared.
- `//crates/wolf-ffi:frame_pack` builds a `wolf_core::Snapshot` by hand (no nvim) and asserts `FfiFrame`'s color packing, flag bits, reverse-video cursor (including the `WolfCursor` theme override: theme bg wins as the fill, a bg-only theme keeps the reverse-video glyph, and the theme beats an honorable mode cursor attr), and cell accessors. It is deterministic and runs **regardless** of `WOLF_NVIM_BIN`.
- `//crates/wolf-ffi:bridge_smoke` drives the Rust half of the bridge against a real pinned Neovim (no mocks) and **passes** under `bazel test` on the macOS Bazel host as well as on Linux/Cargo devShells. It exercises the full live round-trip — start, type `i` then `x` and confirm the grid reflects the `x`, resize, shut down — against the bundled/pinned Neovim 0.12.3, so it is the automated proxy for the live nvim→FFI→`read_frame` round-trip on macOS. To make this hermetic, the `rust_test` stages the captive `init.lua` and the pinned Neovim tree (`//bazel/nvim:nvim_tree`) as runfiles (via `data` + `$(rootpath)` env vars `WOLF_TEST_INIT_LUA` / `WOLF_TEST_NVIM_DIR`, resolved by `common::{init_lua_path, nvim_bin_or_skip}`); an explicit `WOLF_NVIM_BIN` still wins for the cargo/devShell path. It runs `tags = ["no-sandbox"]` (unsandboxed, but still test-result-cached) because spawning a real nvim child does not work under the macOS build sandbox.
- `//crates/wolf-ffi:orlando_load` drives a real pinned Neovim: it stages the vendored orlando tree as a sibling `orlando/` next to a temp `init.lua`, starts the captive editor, and asserts `vim.g.loaded_orlando` is set and `require("orlando")` loads — the automated proof of `init.lua`'s conditional orlando-load branch. It skips loudly when Neovim or the orlando tree is not staged (the cargo / Linux path).
- `//crates/wolf-ffi:lighthouse_load` drives a real pinned Neovim: it stages the vendored lighthouse tree as a sibling `lighthouse/` next to a temp `init.lua`, starts the captive editor, and asserts the colorscheme is actually active (`vim.g.colors_name == "lighthouse"` and `vim.o.background == "dark"`) — the automated proof of `init.lua`'s conditional lighthouse-activation branch. It ALSO proves **startup theme pickup**: the pinned scheme defines `WolfCursor` / `WolfTitlebar` before core installs the `wolf_theme` tracker over RPC, so the tracker's emit-once must still surface the pinned dark-variant values (titlebar bg `0x1b1b1b`, fg `0xf1f1f1`; cursor fill `0x009fce`, glyph `0x000000` — tied to lighthouse commit `9da2bba…`). It skips loudly when Neovim or the lighthouse tree is not staged (the cargo / Linux path).
- `//crates/wolf-ffi:treesitter_parser_abi` drives a real pinned Neovim: it stages **only** the compiled `parsers_tree` (not the `nvim-treesitter/` queries sibling), so `init.lua`'s treesitter enablement block is a strict no-op for this test, then `add`s `parser/go.so` by absolute path via `vim.treesitter.language.add` and parses Go source with it, asserting a `source_file` root with no errors — the automated proof that a shipped, host-compiled parser `.so` is ABI-compatible with pinned nvim 0.12.3. It skips loudly when Neovim or the parsers tree is not staged (the cargo / Linux path).
- `//crates/wolf-ffi:treesitter_highlight` drives a real pinned Neovim with treesitter **enabled**: it stages BOTH the `nvim-treesitter/` queries provider and the `parsers_tree` as siblings of a temp `init.lua`, opens a markdown buffer containing a ` ```go ` fenced block, and asserts (1) the markdown treesitter highlighter is active on the buffer (`vim.treesitter.highlighter.active[buf]` set) AND (2) fenced-code **injection** works — an injected `go` langtree is present inside the block (via `p:parse(true)` then a `go` child langtree). This is the load-bearing proof that main's core-native markdown injections light up embedded code. It skips loudly when Neovim or the trees are not staged (the cargo / Linux path).
- `//crates/wolf-ffi:attribute_highlight` drives a real pinned Neovim: it defines a highlight group carrying `bold` / `italic` / `underline` + a `special` color, applies it to a known cell (`nvim_set_hl` + `matchadd`), polls for the repaint, and asserts the packed `FfiCell` exposes the matching `flags()` (BOLD | ITALIC | UNDERLINE) AND `special().set` with the expected `rgb` across the FFI boundary — the automated guard that per-cell text attributes and the special color survive resolution and packing. It skips loudly when Neovim is not staged (the cargo / Linux path).
- `//crates/wolf-macos:WolfKeyMapTests` is a pure Swift `XCTest` over the AppKit-free `WolfKeyMap` seams — the key-translation seam, the `GlyphStyleResolver` glyph-style seam (`GlyphStyleTests`), and the `StateSeam` state-restore seam (`StateSeamTests`: offscreen/unset window-frame rejection, and the launch-cwd fallback) — with no `NSEvent` and no window, and runs regardless of `WOLF_NVIM_BIN`.

## Linting and formatting (macOS)

Advisory (non-gating) lint/format targets live in `//tools/lint` and run on the macOS Bazel host: **clippy** and **rustfmt** for Rust (via the rules_rust downloaded 1.85.0 toolchain — the devShell has no standalone cargo/rustc) and **swift-format** for Swift (the host Xcode 26 binary, resolved via `xcrun --find swift-format`).

These targets are **advisory**: every one is tagged `manual`, so `bazel test //crates/...` never sweeps them in and the default suite stays exactly 47/47. They are NOT run in CI gating here; existing violations are expected. A CHECK target "passing" means "the linter ran"; it may still report findings (which fail the target). See [`tools/lint/README.md`](tools/lint/README.md).

Each tool has a CHECK (report/fail on violations via `bazel test`) and a FIX (rewrite in place via `bazel run`):

| Tool | Check | Fix |
| --- | --- | --- |
| clippy | `bazel test //tools/lint:clippy` | `bazel run //tools/lint:clippy.fix` |
| rustfmt | `bazel test //tools/lint:rustfmt` | `bazel run //tools/lint:rustfmt.fix` |
| swift-format | `bazel test //tools/lint:swift_format` | `bazel run //tools/lint:swift_format.fix` |
| **all** | `bazel test //tools/lint:check` | `bazel run //tools/lint:fix` |

Notes and caveats:

- **clippy fail-on-lint is opt-in.** The clippy CHECK reports findings by default; to make `-Dwarnings` the hard rule add `--config=lint`: `bazel test --config=lint //tools/lint:clippy`. `--config=lint` (in `.bazelrc`) is DELIBERATELY not default so the standard build/test path is byte-for-byte unchanged.
- **clippy `--fix` is non-hermetic.** `bazel run //tools/lint:clippy.fix` shells out to `cargo clippy --fix`, which resolves crates from crates.io (network) and links via the host Xcode cc toolchain. It is a dev convenience, scoped to `-p wolf-core -p wolf-ffi` (wolf-gnome's GTK stack does not build on macOS).
- **rustfmt** uses the separate rules_rust rustfmt toolchain. `//tools/lint:rustfmt.fix` is the rules_rust writer; it internally evaluates `//...`, which is why the repo carries a `.bazelignore` entry for `.direnv` (the nix+direnv symlink tree would otherwise break the `//...` query).
- **swift-format** covers only the hand-written `wolf-macos` Swift (Sources/WolfKeyMap, Sources/WolfMacOS, Tests/WolfKeyMapTests, enumerated via the `//crates/wolf-macos:swift_format_srcs` filegroup). It EXCLUDES the machine-generated swift-bridge bindings under `crates/wolf-ffi/generated/`. Its check `sh_test` is tagged `no-sandbox` because the binary is host Xcode, not a hermetic input. Note that check and fix enforce different things: `:swift_format` (lint) checks only swift-format's lint-category rules, while `:swift_format.fix` (format) additionally restyles whitespace and indentation, so a passing check does not mean the fixer would leave the files unchanged.

## The one hard rule and how it is enforced

> **`wolf-core` must compile with no GUI toolkit in its dependency tree.**

This is Wolf's analog of a dependency guard. Enforce it by inspecting `wolf-core`'s (non-dev) dependency tree and confirming none of the denied crates appear:

```sh
cargo tree -e no-dev -p wolf-core
```

The denylist (no occurrence of any of these is permitted):

```
gtk gtk4 glib gdk4 cairo cairo-rs pango libadwaita
objc objc2 cocoa core-graphics winit wgpu skia-safe softbuffer
```

If any of these appears in core's tree, the platform boundary has leaked and the build is wrong.

The same rule holds under the Bazel build on macOS: `wolf-gnome` is deliberately given no Bazel target, so GTK never enters `wolf-core`'s Bazel dependency graph. Verify it with a `bazel query` over the resolved deps:

```sh
bazel query 'deps(//crates/wolf-core:wolf_core)' | grep -iE \
  'gtk|gdk|glib|cairo|pango|adwaita|objc|cocoa|core-graphics|winit|wgpu|skia|softbuffer' \
  || echo clean
```

## Notes

- The captive Neovim is pinned and hermetic from day one (see [architecture.md](architecture.md) §8): on Linux via the flake devShell, on macOS via the Neovim bundled inside the `.app` (above). Either way it runs the pinned binary with the repo's `runtime/nvim/init.lua`.
- The **orlando** plugin is the first vendored plugin, and the **lighthouse** colorscheme is the second vendored asset — activated (not just sourced) by the captive `init.lua` (both macOS Bazel bundle only; see "Bundled Neovim (macOS)" above). Full `wrapNeovim`-style plugin baking beyond that — a Markdown LSP such as `marksman` and a formatter — is **deferred to a later phase**. The current build relies on Neovim's markdown syntax highlighting and the `markdown` / `markdown_inline` treesitter parsers the release carries.
