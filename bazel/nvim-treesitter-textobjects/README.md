# bazel/nvim-treesitter-textobjects

Hermetic, pinned **nvim-treesitter-textobjects** plugin for bundling into `Wolf.app`.

This package downloads the pinned GitHub source tarball for the pure-Lua nvim-treesitter-textobjects plugin (sha256-pinned, fetched by Bazel — no system or `PATH` dependency), assembles it into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/nvim-treesitter-textobjects/`.

The plugin is bundled but inert. The captive `runtime/nvim/init.lua` prepends the tree to `runtimepath` when the sibling directory exists, so the `lua/` modules and the per-language `queries/<lang>/textobjects.scm` are resolvable, but nothing sources the plugin, calls its setup, or binds a textobject keymap. Enablement, and the Linux/Nix vendoring path, are deferred.

## Why the `main` branch and not `master`

The pin tracks the textobjects `main` branch for branch-consistency with the sibling [`bazel/nvim-treesitter`](../nvim-treesitter/README.md), which is pinned to nvim-treesitter's `main` (the rewrite and its new module API). Pairing textobjects with the nvim-treesitter branch it targets avoids a latent incompatibility, so both track `main`.

Three layout facts follow from that branch, and the overlay depends on all three:

- The Lua modules live under the self-named `lua/nvim-treesitter-textobjects/` layout, not the `lua/nvim-treesitter/textobjects/` layout `master` uses.
- The per-language `queries/<lang>/textobjects.scm` sit at the archive root. Unlike the sibling nvim-treesitter `main`, textobjects `main` does not relocate its queries under `runtime/`.
- There is no top-level `plugin/` loader, so the overlay globs none.

## The pin

- Repo: `nvim-treesitter/nvim-treesitter-textobjects`, branch `main`
- Commit: `851e865342e5a4cb1ae23d31caf6e991e1c99f1e`
- Tarball: https://github.com/nvim-treesitter/nvim-treesitter-textobjects/archive/851e865342e5a4cb1ae23d31caf6e991e1c99f1e.tar.gz
- sha256: `8d5fd6d8f48021f941420053389c0b23ccbe136fb2deacc3872e42492fb8b9fa`
- Archive root dir: `nvim-treesitter-textobjects-851e865342e5a4cb1ae23d31caf6e991e1c99f1e`

## Targets

- **`//bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree`** — an `alias` to `@nvim_treesitter_textobjects//:nvim_treesitter_textobjects_tree`. Pure Lua (arch-independent), so there is no `select()`: a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `nvim-treesitter-textobjects/lua/nvim-treesitter-textobjects/**`, `nvim-treesitter-textobjects/queries/<lang>/textobjects.scm`, and `nvim-treesitter-textobjects/LICENSE`.
- **`//bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree_test`** — an `sh_test` over `:nvim_treesitter_textobjects_tree`: asserts the `lua/nvim-treesitter-textobjects/` module directory and at least one `queries/**/textobjects.scm` landed inside the tree's inner `nvim-treesitter-textobjects/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`nvim_treesitter_textobjects`) for the pinned source tarball. Its overlay `BUILD` file globs the vendored subtrees (`lua/`, `queries/`, `LICENSE*`) under the archive's top dir, `nvim-treesitter-textobjects-<commit>/`, deliberately excluding dev-only `tests/`, `scripts/`, `doc/`, `README.md`, and dotfiles.

It then runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into one TreeArtifact, remapping the archive's top dir to `nvim-treesitter-textobjects/` via `replace_prefixes`. Because the queries stay at the archive root, that inner-dir remap is the only prefix rewrite needed.

The inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. Nesting everything under `nvim-treesitter-textobjects/` reliably yields `Contents/Resources/nvim-treesitter-textobjects/...`.

`aspect_bazel_lib` is a build-time-only ruleset; it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

```starlark
nvim_treesitter_textobjects = use_extension("//bazel/nvim-treesitter-textobjects:extensions.bzl", "nvim_treesitter_textobjects")
use_repo(nvim_treesitter_textobjects, "nvim_treesitter_textobjects")
```

## Bumping the pinned commit

1. Pick the new commit SHA on the `main` branch and update `_COMMIT` in `extensions.bzl`. The archive root name auto-follows.
2. Recompute the checksum: `curl -L https://github.com/nvim-treesitter/nvim-treesitter-textobjects/archive/<commit>.tar.gz -o txo.tar.gz && shasum -a 256 txo.tar.gz`. Update `_SHA256`.
3. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree_test`.
