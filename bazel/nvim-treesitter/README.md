# bazel/nvim-treesitter

Hermetic, pinned **nvim-treesitter** plugin for bundling into `Wolf.app`.

This package downloads the pinned GitHub source tarball for the pure-Lua nvim-treesitter plugin (sha256-pinned, fetched by Bazel — no system or PATH dependency), assembles it into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/nvim-treesitter/`.

The bundled `queries/` are used **as an on-`runtimepath` query provider** by the captive `runtime/nvim/init.lua`: on the macOS `.app` (where the vendored `nvim-treesitter/` + `parsers/` siblings are present next to `init.lua`) treesitter markdown highlighting and fenced-code-block injection are now **ENABLED** (`vim.treesitter.start` on markdown buffers). Nothing here sources the plugin's Lua or calls its `.setup{}`; the plugin is used purely as a rtp query provider. On Linux/cargo — where the siblings are absent — the enablement block in `init.lua` is a strict no-op. The Linux/Nix vendoring path is still deferred.

## Why the `main` branch (not `master`)

The pin is on nvim-treesitter's **`main`** branch. This is a **decision reversal** from the original `master` pin, forced by a query-loader incompatibility with the pinned Neovim 0.12.3:

- **`master` is broken on nvim 0.12.3.** master's `queries/markdown/injections.scm` routes fenced blocks with the custom `#set-lang-from-info-string!` predicate. That predicate's frozen-master handler targets a pre-0.10 query-match API that nvim 0.12.3 removed, so every fenced-block redraw raises `E5108` and injection is broken.
- **`main` fixes it with core-native injections.** main's `queries/markdown/injections.scm` uses pure core-supported, capture-based injection (an `@injection.language` / `@injection.content` capture pair, no custom predicate). It works from the queries-on-`runtimepath` alone — no `:TSInstall`, no plugin Lua sourced, no `.setup{}` — and both `//crates/wolf-ffi:treesitter_highlight` (markdown highlighter active + injected `go` langtree present) and a headless repro pass on it.

On `main` the queries moved under `runtime/queries/<lang>/…` (the overlay remaps that back to `nvim-treesitter/queries/…` — see "How it works"); the classic `lua/nvim-treesitter/` modules and the `plugin/` loader are still present in the tree (bundled but never sourced). The parser revisions Wolf compiles in [`bazel/treesitter-parsers`](../treesitter-parsers/README.md) are taken from this same pinned commit's `lockfile.json`, so the queries and parsers are version-matched.

## The pin

- Repo: `nvim-treesitter/nvim-treesitter`, branch `main`
- Commit: `4916d6592ede8c07973490d9322f187e07dfefac`
- Tarball: https://github.com/nvim-treesitter/nvim-treesitter/archive/4916d6592ede8c07973490d9322f187e07dfefac.tar.gz
- sha256: `4e720e38f1a74c07fc8bc9d6264e25ab620df93c6576200f43f2ca0ce5fbdec1`
- Archive root dir: `nvim-treesitter-4916d6592ede8c07973490d9322f187e07dfefac`

## Targets

- **`//bazel/nvim-treesitter:nvim_treesitter_tree`** — an `alias` to `@nvim_treesitter//:nvim_treesitter_tree`. Pure Lua and therefore arch-independent, so there is **no** `select()` (unlike `//bazel/nvim`): a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `nvim-treesitter/queries/**` (remapped from the archive's `runtime/queries/**`), `nvim-treesitter/lua/nvim-treesitter/**`, `nvim-treesitter/plugin/**`, and `nvim-treesitter/LICENSE`. This is the value bundled into the app and used by `init.lua` as a query provider.
- **`//bazel/nvim-treesitter:nvim_treesitter_tree_test`** — an `sh_test` over `:nvim_treesitter_tree`: asserts a markdown query file (`queries/markdown/injections.scm`, or `highlights.scm` as a fallback) and the `lua/nvim-treesitter/` module directory landed at the expected paths inside the tree's inner `nvim-treesitter/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`nvim_treesitter`) for the pinned source tarball. It gets an overlay `BUILD` file (`build_file_content`) that:

1. globs the vendored subtrees (`runtime/queries/`, `lua/`, `plugin/`, `LICENSE*`) under the archive's top dir, `nvim-treesitter-<commit>/` (deliberately excluding dev-only `tests/`, `scripts/`, `doc/`, `assets/`, the committed `parser/` — Wolf compiles its own curated parsers — `lockfile.json`, `README.md`, `Makefile`, and dotfiles), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, applying **two** `replace_prefixes` remaps: `<root>/runtime/queries` → `nvim-treesitter/queries` (main relocated the queries under `runtime/`, so they are lifted back up), and `<root>` → `nvim-treesitter/` for everything else (NOT `http_archive`'s `strip_prefix`).

The `nvim-treesitter/` inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting everything under `nvim-treesitter/`, the app reliably gets `Contents/Resources/nvim-treesitter/...`, and the `queries/` lift keeps `init.lua`'s single rtp prepend (`<this_dir>/nvim-treesitter`) resolving `queries/<lang>/{highlights,injections}.scm` exactly as before.

`aspect_bazel_lib` is a build-time-only ruleset (already used by `//bazel/nvim`); it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

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
