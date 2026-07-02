# bazel/treesitter-parsers

Hermetic, pinned, **host-compiled** treesitter parsers for bundling into `Wolf.app`.

This package downloads sha256-pinned GitHub source tarballs for a curated set of treesitter grammars, **compiles** each grammar's `src/parser.c` (+ optional `src/scanner.c`) into a loadable shared object with the host (apple_support / Xcode) cc toolchain, and assembles them into a structure-preserving directory artifact. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/parsers/`, so each parser lands at `Contents/Resources/parsers/parser/<lang>.so`.

These parsers are **vendored + shipped and ENABLED on macOS**: the captive `runtime/nvim/init.lua` prepends the sibling `parsers/` + `nvim-treesitter/` trees to `runtimepath` and runs a pcall-wrapped `vim.treesitter.start` on markdown buffers, so markdown highlighting + fenced-code injection are live in the `.app`. On Linux/cargo the init.lua block is a strict no-op (`syntax on` remains). The Linux/Nix vendoring path is still deferred.

## Why host-compile (not nix-built, not the parsers inside nvim)

Pinned Neovim 0.12.3 ships only the `markdown` / `markdown_inline` parsers. To highlight the languages Wolf cares about, the parsers must (a) be present in the bundle, (b) be ABI-compatible with pinned nvim's treesitter runtime, and (c) be **signable, relocatable host-compiled Mach-O**. A nix-built `.so` embeds `/nix/store` install names and is not relocatable inside a signed `.app`, so the parsers are compiled from source with the same apple_support host-Xcode cc toolchain that builds `wolf-core`'s bundled SQLite. The resulting `.so`s are `MH_DYLIB` Mach-O shared libraries with `NOUNDEFS` (self-contained; no runtime symbol resolution needed), which `dlopen` / `vim.treesitter.language.add` accept.

The **ABI is proven end-to-end** by `//crates/wolf-ffi:treesitter_parser_abi`, which drives a REAL pinned nvim 0.12.3, `add`s the compiled `parser/go.so` by absolute path, and parses Go source with it.

## Provenance of the pins

Grammar **revisions** come from the nvim-treesitter `lockfile.json` as pinned during the original vendoring — when the plugin was on `master` (master-era) — and grammar **repo URLs** come from that commit's `lua/nvim-treesitter/parsers.lua`. The single exception is **swift** (see the status table). These parser revisions were **not** re-synced when the vendored plugin later moved to `main` (see [`bazel/nvim-treesitter`](../nvim-treesitter/README.md)): re-deriving them from `main`'s `lockfile.json` for an exact queries⇄parsers version-match is a future nicety. The current pairing is proven working — the `//crates/wolf-ffi:treesitter_highlight` injection test and `//crates/wolf-ffi:treesitter_parser_abi` both pass — and tree-sitter grammar ABIs are stable, so queries-from-`main` with parsers-from-master-era is fine.

## The `ts_grammar` overlay template

`extensions.bzl` declares one `http_archive` per grammar repo (`ts_<lang>`; typescript + tsx share one repo). Each repo gets a generated overlay `BUILD` file (`build_file_content`, NOT a loaded macro — this keeps `-Isrc` resolving in-repo, matching the orlando/lighthouse pattern) built from a reusable template that emits, per grammar:

```starlark
cc_binary(
    name = "<lang>.so",
    srcs = glob([".../src/parser.c", ".../src/*.h", ".../src/tree_sitter/*.h", ".../src/scanner.c"]),
    includes = [".../src"],          # -Isrc resolves #include "tree_sitter/parser.h"
    copts = ["-std=c11", "-fPIC", "-w"],
    linkshared = True,               # -> a loadable MH_DYLIB Mach-O
    linkstatic = True,
)
copy_file(name = "<lang>_parser_file", src = ":<lang>.so", out = "parser/<lang>.so")
filegroup(name = "<lang>_parser", srcs = [":<lang>_parser_file"])
```

The main `BUILD.bazel` merges every `@ts_<lang>//:<lang>_parser` into one `parsers_tree` TreeArtifact via `copy_to_directory`, using `include_external_repositories = ["*ts_*"]` (the leading `*` matches the bzlmod **canonical** repo names, e.g. `+treesitter_parsers+ts_go`) so each `parser/<lang>.so` enters as its in-repo path, then `replace_prefixes = {"parser": "parsers/parser"}` remaps it to `parsers/parser/<lang>.so`. The inner `parsers/` dir is deliberate — `rules_apple` strips the bundled TreeArtifact's own top-level dir name, so nesting under `parsers/` lands it at `Contents/Resources/parsers/` robustly.

Two grammars need special handling:

- **typescript + tsx** live in one repo (subdirs `typescript/` and `tsx/`) and their scanners `#include "../../common/scanner.h"`, so that repo's overlay stages `common/**` as inputs and emits two `cc_binary`s.
- **yaml**'s `scanner.c` textually `#include`s a `schema.<x>.c` file (selected by the `YAML_SCHEMA` macro, default `core`). Those `.c` files must be staged but **not** compiled as their own translation units, so yaml gets a custom overlay that provides them via a `cc_library` `textual_hdrs` dep.

## Targets

- **`//bazel/treesitter-parsers:parsers_tree`** — the merged TreeArtifact; contents are `parsers/parser/<lang>.so` for each curated grammar. This is the value bundled into the app and staged by the ABI test.
- **`//bazel/treesitter-parsers:parsers_tree_test`** — an `sh_test` over `:parsers_tree`: HARD-asserts `parser/{go,rust,typescript,tsx,json}.so` exist and are non-empty; SOFT-notes `swift`, `kotlin`, `java` (drop+LOG policy) and whether each `.so` reports as Mach-O.

## Curated grammar status table

All 16 languages in the curated set compile hermetically at their pinned revisions and ship in the bundle. **No grammar was dropped.** All scanners are C at these revisions (no C++ scanner needed).

| lang | repo | subdir | scanner | status | notes |
| --- | --- | --- | --- | --- | --- |
| json | tree-sitter/tree-sitter-json | — | none | SHIPPED |  |
| go | tree-sitter/tree-sitter-go | — | none | SHIPPED |  |
| c | tree-sitter/tree-sitter-c | — | none | SHIPPED |  |
| java | tree-sitter/tree-sitter-java | — | none | SHIPPED |  |
| css | tree-sitter/tree-sitter-css | — | scanner.c | SHIPPED |  |
| javascript | tree-sitter/tree-sitter-javascript | — | scanner.c | SHIPPED |  |
| rust | tree-sitter/tree-sitter-rust | — | scanner.c | SHIPPED |  |
| bash | tree-sitter/tree-sitter-bash | — | scanner.c | SHIPPED |  |
| html | tree-sitter/tree-sitter-html | — | scanner.c | SHIPPED | scanner also includes `src/tag.h` (globbed via `src/*.h`) |
| toml | tree-sitter-grammars/tree-sitter-toml | — | scanner.c | SHIPPED |  |
| cpp | tree-sitter/tree-sitter-cpp | — | scanner.c | SHIPPED |  |
| typescript | tree-sitter/tree-sitter-typescript | `typescript/` | scanner.c | SHIPPED | shares repo + `common/scanner.h` with tsx |
| tsx | tree-sitter/tree-sitter-typescript | `tsx/` | scanner.c | SHIPPED | shares repo + `common/scanner.h` with typescript |
| yaml | tree-sitter-grammars/tree-sitter-yaml | — | scanner.c | SHIPPED | custom overlay: `schema.*.c` staged as `textual_hdrs` |
| kotlin | fwcd/tree-sitter-kotlin | — | scanner.c | SHIPPED |  |
| swift | alex-pinkus/tree-sitter-swift | — | scanner.c | SHIPPED | pinned to the `with-generated-files` branch (committed parser.c) |

**swift note:** swift's `src/parser.c` is generated and **not** committed on the default branch, so it is pinned to the `with-generated-files` branch HEAD (an immutable commit sha) which carries a committed `src/parser.c` + a C `src/scanner.c`. This is a different revision than the nvim-treesitter lockfile (which points at the default branch); acceptable because tree-sitter grammar ABIs are stable and we only need a compilable, ABI-compatible parser to ship (no test asserts swift highlighting).

**markdown / markdown_inline** are deliberately **excluded**: pinned nvim 0.12.3 already ships them at `Contents/Resources/nvim/lib/nvim/parser/`.

### Drop+LOG policy

A grammar that cannot compile hermetically after a genuine attempt is **removed** from `extensions.bzl`, from `use_repo(...)` in `MODULE.bazel`, and from the main BUILD's `copy_to_directory` srcs — and **recorded here** (status `DROPPED` with the reason). One grammar never blocks the rest. The tree test and the bundle-layout test HARD-assert only the low-risk core (go, rust, typescript, tsx, json); the higher-risk grammars (swift, kotlin, java) are SOFT-noted so a future drop does not break those tests. If a HARD-asserted grammar is ever dropped, the tree/bundle tests must be updated too.

## Adding a language

1. Find the grammar's repo URL in the pinned nvim-treesitter `master` `lua/nvim-treesitter/parsers.lua` and its revision in that commit's `lockfile.json`.
2. Download the tarball and compute `sha256`: `curl -L https://github.com/<repo>/archive/<rev>.tar.gz -o g.tar.gz && shasum -a 256 g.tar.gz`. Confirm whether it carries a `src/scanner.c` (and that it is C, not C++) at that revision.
3. Add a `(repo_name, gh_repo, rev, sha256, scanner?)` tuple to `root_grammars` in `extensions.bzl` (or a custom overlay for the odd cases above).
4. Add its `ts_<lang>` to `use_repo(treesitter_parsers, ...)` in `MODULE.bazel` and its `@ts_<lang>//:<lang>_parser` to the `copy_to_directory` srcs in `BUILD.bazel`.
5. `bazel build @ts_<lang>//:<lang>.so` to verify it compiles; then regenerate `MODULE.bazel.lock` and re-run `bazel test //bazel/treesitter-parsers:parsers_tree_test`.

## Bumping a pinned grammar

Bump the grammar revisions **together with** the nvim-treesitter query pin so queries and parsers stay version-matched. Update the grammar's `rev` + `sha256` in `extensions.bzl`, **RE-VERIFY the scanner extension** at the new revision (C vs C++, whether one exists), regenerate `MODULE.bazel.lock`, and re-run the tree + ABI tests.
