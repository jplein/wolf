# bazel/lighthouse

Hermetic, pinned **lighthouse.nvim** colorscheme for bundling into `Wolf.app`.

This package downloads the pinned GitHub source tarball for the pure-Lua lighthouse colorscheme (sha256-pinned, fetched by Bazel — no system or `PATH` dependency), assembles it into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/lighthouse/`, and the captive `runtime/nvim/init.lua` conditionally prepends it to `runtimepath` and activates it. The wolf-gnome package installs the same tree under `share/wolf/nvim/lighthouse/`. The load is a strict no-op when the tree is absent, as in a checkout.

## Activation

lighthouse is a colorscheme, not a plugin. Unlike orlando, merely placing the directory on `runtimepath` does nothing — a colorscheme is inert until it is explicitly selected. When the sibling `lighthouse/` tree is present, the captive `init.lua` prepends it to `runtimepath`, sets `vim.o.background = "dark"`, and runs `vim.cmd.colorscheme("lighthouse")` (which finds `colors/lighthouse.lua` on `runtimepath` itself — no `runtime()` of the colors file is needed).

Ordering matters: this activation runs after `vim.cmd("syntax on")`, because both `syntax on` and `:colorscheme` clear/redefine highlight groups, so the colorscheme must win by running last. `vim.o.background` is set before `:colorscheme` so a dual-variant scheme reads it.

The hardcoded `"dark"` is only the pre-push fallback for the initial paint: the front-end pushes the resolved appearance at launch, and live in `auto` mode when the OS toggles, via `:set background=…`.

## The pin

- Commit: `431d3f6c4034af4155a9b2a9a810faa28f92be70`
- Tarball: https://github.com/jplein/lighthouse.nvim/archive/431d3f6c4034af4155a9b2a9a810faa28f92be70.tar.gz
- sha256: `55a4c72e071a2dcab60a4d69b0b8a0051bb6176ac0001581f7866598081ed1d5`
- Archive root dir: `lighthouse.nvim-431d3f6c4034af4155a9b2a9a810faa28f92be70` (the upstream repo name includes `.nvim`)

Of the two optional Wolf app-theme highlight groups that core tracks through the `wolf_theme` notification ([`runtime/nvim`](../../runtime/nvim/README.md#wolf_setup_theme_trackingchan)), this pin defines `WolfCursor` and leaves `WolfTitlebar` unset, so the titlebar keeps the system colour. It sets `g:terminal_color_0` to `g:terminal_color_15` for both backgrounds, which become a terminal pane's 16 ANSI colours.

The tests that check these colours read their expected values from the pinned `colors/lighthouse.lua` rather than from constants, so a bump needs no test edit unless it changes the shapes they parse: the `local dark = {` and `local light = {` palette tables of `key = "#rrggbb",` lines, and the `local ansi = {` list of `p.<key>,` lines, each closed by a line that is exactly `}`. The Swift reader also needs the lines that set `Normal` to the palette's `foreground` on its `background`. A bump that changes a shape fails those tests naming the missing piece.

The readers are `Lighthouse` in [`crates/wolf-ffi/tests/common/mod.rs`](../../crates/wolf-ffi/tests/README.md), its copy in `crates/wolf-gnome/tests/smoke_headless/lighthouse.rs`, and [`LighthouseColours.swift`](../../crates/wolf-macos/Tests/WolfMacOSTests/README.md#lighthousecoloursswift).

## Targets

- **`//bazel/lighthouse:lighthouse_tree`** — an `alias` to `@lighthouse//:lighthouse_tree`. Unlike `//bazel/nvim` there is no `select()`: lighthouse is pure Lua and therefore arch-independent, so there is a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `lighthouse/colors/lighthouse.lua` and `lighthouse/LICENSE*`. This is the value bundled into the app and exercised by the smoke test.
- **`//bazel/lighthouse:lighthouse_tree_test`** — an `sh_test` over `:lighthouse_tree`: asserts `colors/lighthouse.lua` landed at the expected path inside the tree's inner `lighthouse/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`lighthouse`) for the pinned source tarball. It gets an overlay `BUILD` file (`build_file_content`) that:

1. globs the vendored subtrees (`colors/`, `LICENSE*`) under the archive's top dir, `lighthouse.nvim-<commit>/` (deliberately excluding dev-only `build/`, `flake.*`, `Makefile`, `README.md`, `.gitignore`, `.prettierrc`), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, remapping the archive's top dir to `lighthouse/` via `replace_prefixes` rather than `http_archive`'s `strip_prefix`.

The `lighthouse/` inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting everything under `lighthouse/`, the app reliably gets `Contents/Resources/lighthouse/...`.

Note the archive root: the upstream repo is named `lighthouse.nvim`, so the tarball's top directory is `lighthouse.nvim-<commit>`, not `lighthouse-<commit>`. Dropping the `.nvim` would make the `copy_to_directory` glob match nothing.

`aspect_bazel_lib` is a build-time-only ruleset (already used by `//bazel/nvim`); it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

```starlark
lighthouse = use_extension("//bazel/lighthouse:extensions.bzl", "lighthouse")
use_repo(lighthouse, "lighthouse")
```

## Bumping the pinned lighthouse commit

1. Pick the new commit SHA and update `_COMMIT` in `extensions.bzl`. The archive root name (`lighthouse.nvim-<commit>`) auto-follows.
2. Recompute the checksum: `curl -L https://github.com/jplein/lighthouse.nvim/archive/<commit>.tar.gz -o lighthouse.tar.gz && shasum -a 256 lighthouse.tar.gz`. Update `_SHA256`.
3. Set the same `commit` and `sha256` on the `lighthouse` entry of `flake.nix`'s `wolfVendoredPins`, in the same change, or [`crates/wolf-gnome/tests/vendored_pins.rs`](../../crates/wolf-gnome/tests/README.md#vendored_pinsrs) fails on Linux.
4. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
5. Re-run `bazel test //bazel/lighthouse:lighthouse_tree_test`.
