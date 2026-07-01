# bazel/nvim-treesitter

Hermetic, pinned **nvim-treesitter** plugin for bundling into `Wolf.app`.

This package downloads the pinned GitHub source tarball for the pure-Lua nvim-treesitter plugin (sha256-pinned, fetched by Bazel — no system or PATH dependency), assembles it into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/nvim-treesitter/`.

This change **vendors the plugin only** — it does **not** enable treesitter highlighting. The bundled `queries/` and `lua/` are shipped for a future enablement change; the captive `runtime/nvim/init.lua` is **not** modified by this change (no `vim.treesitter.start`, no runtimepath wiring). Enablement, and the Linux/Nix vendoring path, are deferred.

## Why the `master` branch (not `main`)

The pin is on nvim-treesitter's **`master`** branch, which is the frozen / legacy branch that still ships the **bundled `queries/`** (`highlights.scm`, `injections.scm`, ...) plus the classic `lua/nvim-treesitter/` modules. Active development moved to the `main` branch, which drops the bundled queries in favor of an external query source — not what we want for a self-contained bundle. The parser revisions Wolf compiles in [`bazel/treesitter-parsers`](../treesitter-parsers/README.md) are taken from this same pinned commit's `lockfile.json`, so the queries and parsers are version-matched.

## The pin

- Repo: `nvim-treesitter/nvim-treesitter`, branch `master`
- Commit: `cf12346a3414fa1b06af75c79faebe7f76df080a`
- Tarball: https://github.com/nvim-treesitter/nvim-treesitter/archive/cf12346a3414fa1b06af75c79faebe7f76df080a.tar.gz
- sha256: `b3061a2ab7e4d50b5b703f8ea56f9dd579e3cbb47a4d7aaa68ef331c14b64b20`
- Archive root dir: `nvim-treesitter-cf12346a3414fa1b06af75c79faebe7f76df080a`

## Targets

- **`//bazel/nvim-treesitter:nvim_treesitter_tree`** — an `alias` to `@nvim_treesitter//:nvim_treesitter_tree`. Pure Lua and therefore arch-independent, so there is **no** `select()` (unlike `//bazel/nvim`): a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `nvim-treesitter/queries/**`, `nvim-treesitter/lua/nvim-treesitter/**`, `nvim-treesitter/plugin/**`, and `nvim-treesitter/LICENSE`. This is the value bundled into the app and exercised by the smoke test.
- **`//bazel/nvim-treesitter:nvim_treesitter_tree_test`** — an `sh_test` over `:nvim_treesitter_tree`: asserts a markdown query file (`queries/markdown/injections.scm`, or `highlights.scm` as a fallback) and the `lua/nvim-treesitter/` module directory landed at the expected paths inside the tree's inner `nvim-treesitter/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`nvim_treesitter`) for the pinned source tarball. It gets an overlay `BUILD` file (`build_file_content`) that:

1. globs the vendored subtrees (`queries/`, `lua/`, `plugin/`, `LICENSE*`) under the archive's top dir, `nvim-treesitter-<commit>/` (deliberately excluding dev-only `tests/`, `scripts/`, `doc/`, `assets/`, the committed `parser/` — Wolf compiles its own curated parsers — `lockfile.json`, `README.md`, `Makefile`, and dotfiles), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, **remapping** the archive's top dir to `nvim-treesitter/` via `replace_prefixes` (NOT `http_archive`'s `strip_prefix`).

The `nvim-treesitter/` inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting everything under `nvim-treesitter/`, the app reliably gets `Contents/Resources/nvim-treesitter/...`.

`aspect_bazel_lib` is a build-time-only ruleset (already used by `//bazel/nvim`); it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

```starlark
nvim_treesitter = use_extension("//bazel/nvim-treesitter:extensions.bzl", "nvim_treesitter")
use_repo(nvim_treesitter, "nvim_treesitter")
```

## Bumping the pinned nvim-treesitter commit

Bump this **together with** the parser revisions in [`bazel/treesitter-parsers`](../treesitter-parsers/README.md), so queries and parsers stay version-matched.

1. Pick the new commit SHA on the `master` branch and update `_COMMIT` in `extensions.bzl`. The archive root name (`nvim-treesitter-<commit>`) auto-follows.
2. Recompute the checksum: `curl -L https://github.com/nvim-treesitter/nvim-treesitter/archive/<commit>.tar.gz -o nvts.tar.gz && shasum -a 256 nvts.tar.gz`. Update `_SHA256`.
3. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/nvim-treesitter:nvim_treesitter_tree_test`.
