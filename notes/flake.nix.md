# `flake.nix`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Single nixpkgs input

One input supplies the toolchain, the GTK stack and the runtime assets the package ships (Neovim, ripgrep and nvim-treesitter), so `nix flake update` moves all of them together. The single input is deliberate: do not split the runtime assets into an input of their own. An update that moves Neovim owes the full verification gate in `build.md`, because [`runtime/nvim/README.md`](../runtime/nvim/README.md), [the init.lua notes](runtime/nvim/init.lua.md) and the test comments state measured claims per Neovim version. macOS pins its own Neovim and nvim-treesitter under `bazel/`, and [the divergence is tracked](macos-followups.md).

The input must not lag the host. The GTK process dlopens the host's Mesa from `/run/opengl-driver`, and that Mesa needs a glibc at least as new as the system's. A `nixpkgs` lock older than the NixOS host leaves the process on an older glibc, the driver fails to load, and WebKit's web process aborts with `Could not create default EGL display: EGL_BAD_PARAMETER`, which shows as a blank Markdown preview.

## Darwin eval laziness

`rustToolchain`, `wolfPointerInject`, `iaWriterMono`, the three treesitter bindings, `wolfNeovim`, `wolfRipgrep`, `wolfOrlando`, `wolfLighthouse`, `wolfGnomePackage` and the GTK and Neovim packages are forced only when `linuxShell` or the package output is evaluated. On Darwin `devShells.default` resolves to `darwinShell`, which references none of them, so laziness keeps them off the Darwin eval path. `wolfPointerInject` is a Wayland program that cannot build on Darwin at all; `iaWriterMono` is arch-independent data and the grammars are ordinary C, so for those the argument is the weaker "safer there than here", but the rule is the same.

Do not add a top-level reference to any of them, and do not wrap any of them in a `lib.optional` guard, which would imply the laziness argument is unsound.

The `optionalAttrs` on the `packages` output is the one deliberate guard, and it is a different thing. The prohibition is about guarding a derivation inside `linuxShell.packages`, where laziness already does the work. The output attribute set is not lazy in the same way: without the guard, `packages.aarch64-darwin.default` would exist as an attribute naming a Wayland GTK application, and `nix flake show` and `nix flake check` would try to evaluate it. The guard is on the attribute's existence, not on the derivation's value.

## Vendored Lua trees

`wolfOrlando` installs exactly what `//bazel/orlando`'s `runtime_files` filegroup carries (`plugin/`, `lua/`, `doc/` and `LICENSE*`), and the exclusions are the point: `test/`, `flake.nix`, `flake.lock`, `.luarc.json` and `README.md` are dev-only, macOS does not bundle them, and vendoring on both platforms exists so both ship the same files. An inclusive `cp -r` would be a divergence nothing else reports; `crates/wolf-gnome/tests/packaged_layout.rs` asserts the exclusions took.

The output root is the tree itself (`$out/plugin/orlando.lua`), with no inner `orlando/` directory. That differs from the Bazel TreeArtifact on purpose: `rules_apple` strips a TreeArtifact's own top-level name when copying it into `Contents/Resources/`, so Bazel has to nest, while `wolfGnomePackage`'s `postInstall` names the destination directory explicitly.

Neither `wolfOrlando` nor `wolfLighthouse` is in `linuxShell.packages`. The devShell must keep running the compositor smokes with neither tree beside the checkout's `runtime/nvim/init.lua`, because that binary is the control leg of the packaged-artifact comparison in `smoke_headless.rs::the_packaged_binary_loads_the_vendored_lighthouse_and_orlando_under_cage`. Adding either tree to the shell looks like a convenience and silently destroys that control. Both are Linux-only bindings under the [Darwin eval laziness](#darwin-eval-laziness) rule.

## Treesitter grammar set

`wolfTreesitterLanguages` is defined by the `srcs` of `bazel/treesitter-parsers/BUILD.bazel`, and the point of vendoring on Linux is cross-platform parity, so a divergence is the bug. It keeps that file's alphabetical order so a side-by-side diff shows a drift. Adding a language means editing this list, that `srcs`, the grammar's pin in that package's `extensions.bzl`, and `MODULE.bazel` in one change.

`c` stays although the pinned Neovim ships its own `c` parser, because taking the parser and the highlights query from the same nvim-treesitter revision is more consistent, and macOS does the same. `markdown` and `markdown_inline` are absent: Neovim ships both, macOS excludes both, and a second copy would shadow Neovim's. `python` is absent because adding it on one side would make macOS the laggard; it goes to both sides in one change.

Do not use `pkgs.tree-sitter-grammars.tree-sitter-<lang>`. Its shared object is named `parser`, not `<lang>.so`, so Neovim never resolves it; its queries are flat and in the upstream tree-sitter dialect rather than nvim-treesitter's; and its grammar revisions do not match the queries Wolf ships.

## Treesitter parsers

Each `nvim-treesitter.parsers.<lang>` derivation contains exactly `parser/<lang>.so`, so the join is `$out/parser/<lang>.so`, the runtimepath layout Neovim resolves a parser from, and no two inputs can collide.

`nvim-treesitter.withPlugins` and `.withAllGrammars` build to the same store path as the bare plugin and only set `passthru.dependencies`, which nothing here materializes, so they would deliver no parsers at all. `withAllGrammars` also costs 249.9 MiB and 313 local builds for a set far wider than the curated one. The binding is Linux-only under the [Darwin eval laziness](#darwin-eval-laziness) rule.

## Treesitter plugin tree

Upstream nvim-treesitter keeps its queries at `runtime/queries/` while `lua/` and `plugin/` sit at the archive root, and a runtimepath entry must expose all three at its top level, so the tree is assembled. `${pkgs.vimPlugins.nvim-treesitter}/runtime` satisfies the query lookup and silently drops the other two: `vim.cmd.runtime("plugin/nvim-treesitter.lua")` finds nothing, `:TSInstall` never registers, and the pcall'd `require("nvim-treesitter").setup` becomes a no-op, so the durable `parsers_user/` install dir is reachable on macOS and not on Linux. `crates/wolf-core/tests/parsers_user.rs` catches it.

The remap mirrors `bazel/nvim-treesitter/extensions.bzl`, which lifts `runtime/queries` back to `queries` and globs `lua/**` and `plugin/**`, so both platforms present the same tree shape.

Queries stay unfiltered, unlike the curated parser list, matching the macOS package, which globs them with no per-language filter: each parser is a compile, while queries are data that cost nothing to carry. The revision is nixpkgs', independent of the one `bazel/nvim-treesitter/extensions.bzl` pins, so the two platforms' queries can differ ([tracked in the macOS follow-ups](macos-followups.md)).

## WebKitGTK and libsoup

webkitgtk does two jobs. At build time it supplies `webkitgtk-6.0.pc`, which `webkit6-sys`'s `system-deps` probe resolves; at run time it supplies the out-of-process `WebKitNetworkProcess` and `WebKitWebProcess` helpers a `WebKitWebView` spawns, and a web view that cannot spawn them renders nothing.

The helpers are found through an absolute path compiled into `libwebkitgtk-6.0.so.4`, not through `PATH` and not through `WEBKIT_EXEC_PATH`, a name that does not appear in the library at all. So the packaged wrapper needs no `PATH` entry and no execution variable of its own. That is measured: `smoke_headless.rs`'s packaged preview smoke launches the store binary with the devShell's exports scrubbed and gets a rendered document.

`libsoup_3` is required and not transitive here. `webkitgtk-6.0.pc` declares `Requires: glib-2.0 gtk4 libsoup-3.0 javascriptcoregtk-6.0`, and `libsoup-3.0.pc` is otherwise absent from the devShell, so without it `soup3-sys` fails to configure and the build breaks.

## No check phase

The suite `cargo test -p wolf-gnome` runs is not a headless unit suite, and running it in the Nix build is impossible, not merely slow. Most tests launch `cage`, a wlroots compositor needing a writable `XDG_RUNTIME_DIR` at mode 0700, alongside `wtype`, `grim`, `tesseract` and `wolf-pointer-inject`; every real-nvim test needs `WOLF_NVIM_BIN`; and `wolf-core`'s swap-recovery test needs a writable `$HOME`. The sandbox has no `/run/user/<uid>`, no `/dev/dri` for GTK's GL renderer, and no session bus.

A curated subset would run only the pure seams the checkout's `cargo test` already covers and would say nothing about the artifact. Every piece of evidence about the artifact therefore comes from outside the sandbox, through `./tools/wolf-package-smoke.sh`: `crates/wolf-gnome/tests/packaged_layout.rs` and the ignored packaged smokes in `crates/wolf-gnome/tests/smoke_headless.rs`.

## Wrapper environment

`wrapGAppsHook4` wraps during `fixupPhase` and consumes the `gappsWrapperArgs` array, so these flags are appended to it. A hand-rolled `wrapProgram` would double-wrap and shadow the hook's own `--prefix XDG_DATA_DIRS`, `GSETTINGS_SCHEMA_DIR`, `GIO_EXTRA_MODULES` and `GDK_PIXBUF_MODULE_FILE`.

Every flag is `--set-default`, for reasons that differ by group. `WOLF_NVIM_BIN` and `WOLF_RG_BIN` are macOS parity: `Project.swift` sets both with `setenv(name, value, 0)`, so a developer's own export wins there too. The treesitter directories follow for consistency, and so a developer testing a work-in-progress query tree can point the packaged binary at one. `FONTCONFIG_FILE` follows so the devShell's exclusionary pin keeps winning inside `nix develop`, which keeps `paint_golden.rs` and every OCR smoke reproducible; outside the shell the packaged additive config applies.

`WOLF_INIT_LUA` is never set here. Setting it would make the executable-relative branch of `init_lua::resolve_init_lua` dead code, since branch 1 always wins, and would make the development override unusable on the packaged binary.
