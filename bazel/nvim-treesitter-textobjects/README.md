# bazel/nvim-treesitter-textobjects

Hermetic, pinned **nvim-treesitter-textobjects** plugin for bundling into `Wolf.app`.

This package downloads the pinned GitHub source tarball for the pure-Lua nvim-treesitter-textobjects plugin (sha256-pinned, fetched by Bazel — no system or PATH dependency), assembles it into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/nvim-treesitter-textobjects/`.

This change **vendors the plugin only** — it does **not** enable treesitter textobjects. The bundled `lua/` and per-language `queries/**/textobjects.scm` are shipped for a future enablement change; the captive `runtime/nvim/init.lua` is **not** modified by this change. Enablement, and the Linux/Nix vendoring path, are deferred.

## Why the `master` branch (not `main`)

The pin is on nvim-treesitter-textobjects' **`master`** branch — the frozen / legacy branch that matches the sibling [`bazel/nvim-treesitter`](../nvim-treesitter/README.md), which is intentionally pinned to `master` (legacy module API + bundled queries). Active development of textobjects moved to the `main` branch, whose self-named `lua/nvim-treesitter-textobjects/` layout targets the new nvim-treesitter `main` module API. Pairing a `main` textobjects with a `master` nvim-treesitter is a latent incompatibility we must not ship, so both are pinned to `master`. On master the modules live under the classic `lua/nvim-treesitter/textobjects/` layout (plus a top-level `lua/nvim-treesitter-textobjects.lua` entry), it still ships per-language `queries/**/textobjects.scm`, and it carries a top-level `plugin/` loader.

## The pin

- Repo: `nvim-treesitter/nvim-treesitter-textobjects`, branch `master`
- Commit: `5ca4aaa6efdcc59be46b95a3e876300cfead05ef`
- Tarball: https://github.com/nvim-treesitter/nvim-treesitter-textobjects/archive/5ca4aaa6efdcc59be46b95a3e876300cfead05ef.tar.gz
- sha256: `c38279bbfedd4a1859d5f3ebdb8641201b3140365c197b68d8a9be092ef7b686`
- Archive root dir: `nvim-treesitter-textobjects-5ca4aaa6efdcc59be46b95a3e876300cfead05ef`

## Targets

- **`//bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree`** — an `alias` to `@nvim_treesitter_textobjects//:nvim_treesitter_textobjects_tree`. Pure Lua (arch-independent), so there is **no** `select()`: a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `nvim-treesitter-textobjects/lua/nvim-treesitter/textobjects/**`, `nvim-treesitter-textobjects/queries/**`, `nvim-treesitter-textobjects/plugin/**`, and `nvim-treesitter-textobjects/LICENSE`.
- **`//bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree_test`** — an `sh_test` over `:nvim_treesitter_textobjects_tree`: asserts the `lua/nvim-treesitter/textobjects/` module directory and at least one `queries/**/textobjects.scm` landed inside the tree's inner `nvim-treesitter-textobjects/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`nvim_treesitter_textobjects`) for the pinned source tarball. Its overlay `BUILD` file globs the vendored subtrees (`lua/`, `queries/`, `plugin/`, `LICENSE*`) under the archive's top dir, `nvim-treesitter-textobjects-<commit>/` (deliberately excluding dev-only `tests/`, `scripts/`, `autoload/`, `doc/`, `README.md`, and dotfiles), then runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into one TreeArtifact, **remapping** the archive's top dir to `nvim-treesitter-textobjects/` via `replace_prefixes`.

The inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. Nesting everything under `nvim-treesitter-textobjects/` reliably yields `Contents/Resources/nvim-treesitter-textobjects/...`.

`aspect_bazel_lib` is a build-time-only ruleset; it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

```starlark
nvim_treesitter_textobjects = use_extension("//bazel/nvim-treesitter-textobjects:extensions.bzl", "nvim_treesitter_textobjects")
use_repo(nvim_treesitter_textobjects, "nvim_treesitter_textobjects")
```

## Bumping the pinned commit

1. Pick the new commit SHA on the `master` branch and update `_COMMIT` in `extensions.bzl`. The archive root name auto-follows.
2. Recompute the checksum: `curl -L https://github.com/nvim-treesitter/nvim-treesitter-textobjects/archive/<commit>.tar.gz -o txo.tar.gz && shasum -a 256 txo.tar.gz`. Update `_SHA256`.
3. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree_test`.
