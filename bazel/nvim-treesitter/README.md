# bazel/nvim-treesitter

Hermetic, pinned **nvim-treesitter** plugin for bundling into `Wolf.app`.

This package downloads the pinned GitHub source tarball for the pure-Lua nvim-treesitter plugin (sha256-pinned, fetched by Bazel — no system or `PATH` dependency), assembles it into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/nvim-treesitter/`.

The bundled `queries/` serve as an on-`runtimepath` query provider for the captive `runtime/nvim/init.lua`. On the macOS `.app`, where the vendored `nvim-treesitter/` and `parsers/` siblings sit next to `init.lua`, treesitter markdown highlighting and fenced-code-block injection are active (`vim.treesitter.start` on markdown buffers).

Nothing here sources the plugin's Lua or calls its `.setup{}`; the plugin is a `runtimepath` query provider and nothing more. On Linux/cargo the siblings are absent, so that block in `init.lua` is a strict no-op. The Linux/Nix vendoring path is deferred.

## Why the `main` branch and not `master`

The pin tracks nvim-treesitter's `main` branch because `master` is incompatible with the pinned Neovim 0.12.3 query loader:

- **`master` is broken on nvim 0.12.3.** Its `queries/markdown/injections.scm` routes fenced blocks through the custom `#set-lang-from-info-string!` predicate. That predicate's frozen-master handler targets a pre-0.10 query-match API that nvim 0.12.3 removed, so every fenced-block redraw raises `E5108` and injection is broken.
- **`main` uses core-native injections.** Its `queries/markdown/injections.scm` is pure core-supported, capture-based injection (an `@injection.language` / `@injection.content` capture pair, no custom predicate). It works from the queries on `runtimepath` alone — no `:TSInstall`, no plugin Lua sourced, no `.setup{}` — and `//crates/wolf-ffi:treesitter_highlight` (markdown highlighter active, injected `go` langtree present) passes on it.

On `main` the queries live under `runtime/queries/<lang>/…`, and the overlay remaps them back to `nvim-treesitter/queries/…` (see "How it works"). The classic `lua/nvim-treesitter/` modules and the `plugin/` loader are present in the tree, bundled but never sourced.

The grammar revisions compiled in [`bazel/treesitter-parsers`](../treesitter-parsers/README.md) come from an earlier nvim-treesitter `lockfile.json`, not this commit's, so queries and parsers are not exactly version-matched. Tree-sitter grammar ABIs are stable and the pairing is proven by `//crates/wolf-ffi:treesitter_highlight`; re-deriving the revisions from this commit's `lockfile.json` is a future nicety.

## The pin

- Repo: `nvim-treesitter/nvim-treesitter`, branch `main`
- Commit: `4916d6592ede8c07973490d9322f187e07dfefac`
- Tarball: https://github.com/nvim-treesitter/nvim-treesitter/archive/4916d6592ede8c07973490d9322f187e07dfefac.tar.gz
- sha256: `4e720e38f1a74c07fc8bc9d6264e25ab620df93c6576200f43f2ca0ce5fbdec1`
- Archive root dir: `nvim-treesitter-4916d6592ede8c07973490d9322f187e07dfefac`

## Targets

- **`//bazel/nvim-treesitter:nvim_treesitter_tree`** — an `alias` to `@nvim_treesitter//:nvim_treesitter_tree`. Pure Lua and therefore arch-independent, so there is no `select()` (unlike `//bazel/nvim`): a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `nvim-treesitter/queries/**` (remapped from the archive's `runtime/queries/**`), `nvim-treesitter/lua/nvim-treesitter/**`, `nvim-treesitter/plugin/**`, and `nvim-treesitter/LICENSE`. This is the value bundled into the app and used by `init.lua` as a query provider.
- **`//bazel/nvim-treesitter:nvim_treesitter_tree_test`** — an `sh_test` over `:nvim_treesitter_tree`: asserts a markdown query file (`queries/markdown/injections.scm`, or `highlights.scm` as a fallback) and the `lua/nvim-treesitter/` module directory landed at the expected paths inside the tree's inner `nvim-treesitter/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`nvim_treesitter`) for the pinned source tarball. It gets an overlay `BUILD` file (`build_file_content`) that:

1. globs the vendored subtrees (`runtime/queries/`, `lua/`, `plugin/`, `LICENSE*`) under the archive's top dir, `nvim-treesitter-<commit>/` (deliberately excluding dev-only `tests/`, `scripts/`, `doc/`, `assets/`, the committed `parser/` — Wolf compiles its own curated parsers — `lockfile.json`, `README.md`, `Makefile`, and dotfiles), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, applying two `replace_prefixes` remaps rather than `http_archive`'s `strip_prefix`: `<root>/runtime/queries` → `nvim-treesitter/queries`, which lifts `main`'s queries back out of `runtime/`, and `<root>` → `nvim-treesitter/` for everything else.

The `nvim-treesitter/` inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting everything under `nvim-treesitter/`, the app reliably gets `Contents/Resources/nvim-treesitter/...`, and the `queries/` lift lets `init.lua`'s single `runtimepath` prepend (`<this_dir>/nvim-treesitter`) resolve `queries/<lang>/{highlights,injections}.scm`.

`aspect_bazel_lib` is a build-time-only ruleset shared by the other vendoring packages; it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

```starlark
nvim_treesitter = use_extension("//bazel/nvim-treesitter:extensions.bzl", "nvim_treesitter")
use_repo(nvim_treesitter, "nvim_treesitter")
```

## Bumping the pinned nvim-treesitter commit

Bump this **together with** the parser revisions in [`bazel/treesitter-parsers`](../treesitter-parsers/README.md), so queries and parsers stay version-matched.

1. Pick the new commit SHA on the `main` branch and update `_COMMIT` in `extensions.bzl`. The archive root name (`nvim-treesitter-<commit>`) auto-follows.
2. Recompute the checksum: `curl -L https://github.com/nvim-treesitter/nvim-treesitter/archive/<commit>.tar.gz -o nvts.tar.gz && shasum -a 256 nvts.tar.gz`. Update `_SHA256`.
3. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/nvim-treesitter:nvim_treesitter_tree_test` and `//crates/wolf-ffi:treesitter_highlight` (the load-bearing proof that markdown highlighting + go injection are active).
