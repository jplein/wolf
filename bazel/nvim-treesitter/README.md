# bazel/nvim-treesitter

Hermetic, pinned **nvim-treesitter** plugin for bundling into `Wolf.app`.

This package downloads the pinned GitHub source tarball for the pure-Lua nvim-treesitter plugin (sha256-pinned, fetched by Bazel — no system or `PATH` dependency), assembles it into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/nvim-treesitter/`.

The captive `runtime/nvim/init.lua` uses the tree as a `runtimepath` query provider and, when the user parser dir resolves, sources its `plugin/` loader and calls `setup` to give `:TSInstall` a durable `install_dir`. On Linux the devShell's `WOLF_NVIM_TREESITTER_DIR` supplies the same tree shape. [`runtime/nvim`](../../runtime/nvim/README.md) owns the resolution order and the activation.

## Why the `main` branch and not `master`

The pin tracks nvim-treesitter's `main` branch because `master`'s markdown injections fail on the Neovim query loader:

- **`master` is broken, measured on Neovim 0.12.3.** Its `queries/markdown/injections.scm` routes fenced blocks through the custom `#set-lang-from-info-string!` predicate. That predicate's frozen-master handler targets a pre-0.10 query-match API that Neovim removed, so every fenced-block redraw raises `E5108` and injection is broken.
- **`main` uses core-native injections.** Its `queries/markdown/injections.scm` is pure core-supported, capture-based injection (an `@injection.language` / `@injection.content` capture pair, no custom predicate). It needs only the queries on `runtimepath` — no plugin Lua and no `setup` — and `//crates/wolf-ffi:treesitter_highlight` (injected `go` langtree present) passes on it.

## The pin

- Repo: `nvim-treesitter/nvim-treesitter`, branch `main`
- Commit: `e289100ff98969e118c702199d88b764ce9e7fdf`
- Tarball: https://github.com/nvim-treesitter/nvim-treesitter/archive/e289100ff98969e118c702199d88b764ce9e7fdf.tar.gz
- sha256: `f4b626bb41f5408c02dcbd7f40c0236d83248a0822fa872d7c16c0752672cb16`
- Archive root dir: `nvim-treesitter-e289100ff98969e118c702199d88b764ce9e7fdf`

The commit matches the nvim-treesitter the Linux flake lock builds, so both platforms ship the same queries; the match is kept by hand, and a `nix flake update` that moves it owes the bump below.

## Targets

- **`//bazel/nvim-treesitter:nvim_treesitter_tree`** — an `alias` to `@nvim_treesitter//:nvim_treesitter_tree`. Pure Lua and therefore arch-independent, so there is no `select()` (unlike `//bazel/nvim`): a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `nvim-treesitter/queries/**` (remapped from the archive's `runtime/queries/**`), `nvim-treesitter/lua/nvim-treesitter/**`, `nvim-treesitter/plugin/**`, and `nvim-treesitter/LICENSE`. This is the value bundled into the app and used by `init.lua` as a query provider.
- **`//bazel/nvim-treesitter:nvim_treesitter_tree_test`** — an `sh_test` over `:nvim_treesitter_tree`: asserts a markdown query file (`queries/markdown/injections.scm`, or `highlights.scm` as a fallback) and the `lua/nvim-treesitter/` module directory landed at the expected paths inside the tree's inner `nvim-treesitter/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`nvim_treesitter`) for the pinned source tarball. It gets an overlay `BUILD` file (`build_file_content`) that:

1. globs the vendored subtrees (`runtime/queries/`, `lua/`, `plugin/`, `LICENSE*`) under the archive's top dir, `nvim-treesitter-<commit>/` (deliberately excluding dev-only `tests/`, `scripts/`, `doc/`, `assets/`, the committed `parser/` — Wolf compiles its own curated parsers — `README.md`, `Makefile`, and dotfiles), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, applying two `replace_prefixes` remaps rather than `http_archive`'s `strip_prefix`: `<root>/runtime/queries` → `nvim-treesitter/queries`, which lifts `main`'s queries back out of `runtime/`, and `<root>` → `nvim-treesitter/` for everything else.

The `nvim-treesitter/` inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting everything under `nvim-treesitter/`, the app reliably gets `Contents/Resources/nvim-treesitter/...`, and the `queries/` lift lets `init.lua`'s single `runtimepath` prepend (`<this_dir>/nvim-treesitter`) resolve `queries/<lang>/{highlights,injections}.scm`.

`aspect_bazel_lib` is a build-time-only ruleset shared by the other vendoring packages; it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

```starlark
nvim_treesitter = use_extension("//bazel/nvim-treesitter:extensions.bzl", "nvim_treesitter")
use_repo(nvim_treesitter, "nvim_treesitter")
```

## Bumping the pinned nvim-treesitter commit

Bump this **together with** the parser revisions in [`bazel/treesitter-parsers`](../treesitter-parsers/README.md), which come from this commit's `lua/nvim-treesitter/parsers.lua`, so queries and parsers stay version-matched.

1. Take the commit the Linux lock's nvim-treesitter is built from and update `_COMMIT` in `extensions.bzl`. The archive root name (`nvim-treesitter-<commit>`) auto-follows.
2. Recompute the checksum: `curl -L https://github.com/nvim-treesitter/nvim-treesitter/archive/<commit>.tar.gz -o nvts.tar.gz && shasum -a 256 nvts.tar.gz`. Update `_SHA256`.
3. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/nvim-treesitter:nvim_treesitter_tree_test`, `//bazel/treesitter-parsers:queries_compile_test` (every vendored query compiles against its pinned grammar) and `//crates/wolf-ffi:treesitter_highlight` (the proof that fenced-code injection is live).
