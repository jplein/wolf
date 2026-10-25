# bazel/treesitter-parsers

Hermetic, pinned, host-compiled treesitter parsers for bundling into `Wolf.app`.

This package downloads sha256-pinned GitHub source tarballs for a curated set of treesitter grammars, compiles each grammar's `src/parser.c` (plus an optional `src/scanner.c`) into a loadable shared object with the host (apple_support / Xcode) cc toolchain, and assembles them into a structure-preserving directory artifact. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/parsers/`, so each parser lands at `Contents/Resources/parsers/parser/<lang>.so`.

These parsers are vendored, shipped, and active on macOS: the captive `runtime/nvim/init.lua` prepends the sibling `parsers/` and `nvim-treesitter/` trees to `runtimepath`, so fenced-code injection is live in the `.app`. This package is the macOS half of a two-platform arrangement — Linux fires the same gate over a nix-built grammar set the flake devShell exports, and [`runtime/nvim`](../../runtime/nvim/README.md) owns the resolution order that serves both.

## Why host-compile, rather than nix-built or the parsers inside nvim

The pinned Neovim ships seven parsers carrying `highlights` queries and no more — `c`, `lua`, `markdown`, `markdown_inline`, `query`, `vim`, `vimdoc` — measured with `--clean` against the Neovim 0.12.6 macOS tarball. To highlight the languages Wolf cares about, the parsers must be present in the bundle, be ABI-compatible with pinned nvim's treesitter runtime, and be signable, relocatable, host-compiled Mach-O.

The two platforms run the same Neovim at the current lock, each pinned independently: `pkgs.neovim` at the revision `flake.lock` pins for Linux, and [`bazel/nvim`](../nvim/README.md) for macOS, which owns how the two are kept matched.

A nix-built `.so` embeds `/nix/store` install names and is not relocatable inside a signed `.app`, so the parsers are compiled from source with the same apple_support host-Xcode cc toolchain that builds `wolf-core`'s bundled SQLite. The resulting `.so`s are `MH_DYLIB` Mach-O shared libraries with `NOUNDEFS` — self-contained, needing no runtime symbol resolution — which `dlopen` and `vim.treesitter.language.add` accept.

`//crates/wolf-ffi:treesitter_parser_abi` proves the ABI end to end: on the macOS Bazel path it drives the live pinned Neovim, `add`s the compiled `parser/go.so` by absolute path, and parses Go source with it.

## Provenance of the pins

Grammar repos and revisions come from `lua/nvim-treesitter/parsers.lua` at the commit [`bazel/nvim-treesitter`](../nvim-treesitter/README.md) pins, so each parser is the one its vendored queries were written against. Swift is the single exception — see the swift note. `:queries_compile_test` is what proves the pairing.

## The `ts_grammar` overlay template

`extensions.bzl` declares one `http_archive` per grammar repo (`ts_<lang>`; typescript and tsx share one repo). Each repo gets a generated overlay `BUILD` file through `build_file_content` rather than a loaded macro, which keeps `-Isrc` resolving in-repo and matches the orlando/lighthouse pattern. A reusable template emits, per grammar:

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

The main `BUILD.bazel` merges every `@ts_<lang>//:<lang>_parser` into one `parsers_tree` TreeArtifact via `copy_to_directory`. `include_external_repositories = ["*ts_*"]` makes each `parser/<lang>.so` enter as its in-repo path; the leading `*` is required because the pattern has to match the bzlmod canonical repo names, such as `+treesitter_parsers+ts_go`. `replace_prefixes = {"parser": "parsers/parser"}` then remaps it to `parsers/parser/<lang>.so`.

The inner `parsers/` dir is deliberate: `rules_apple` strips the bundled TreeArtifact's own top-level dir name, so nesting under `parsers/` lands it at `Contents/Resources/parsers/` robustly.

Two grammars need special handling:

- typescript and tsx live in one repo (subdirs `typescript/` and `tsx/`) and their scanners `#include "../../common/scanner.h"`, so that repo's overlay also stages `common/*.h` as inputs and emits two `cc_binary`s.
- yaml's `scanner.c` textually `#include`s a `schema.<x>.c` file, selected by the `YAML_SCHEMA` macro and defaulting to `core`. Those `.c` files must be staged but must not be compiled as their own translation units, so yaml gets a custom overlay that supplies them through a `cc_library` `textual_hdrs` dep.

## Targets

- **`//bazel/treesitter-parsers:parsers_tree`** — the merged TreeArtifact; its contents are `parsers/parser/<lang>.so` for each curated grammar. This is the value bundled into the app and staged by the ABI test.
- **`//bazel/treesitter-parsers:parsers_tree_test`** — an `sh_test` over `:parsers_tree`. It fails unless `parser/{go,rust,typescript,tsx,json}.so` all exist and are non-empty; for `swift`, `kotlin`, and `java`, and for whether each `.so` reports as Mach-O, it only prints a note.
- **`//bazel/treesitter-parsers:queries_compile_test`** — an `sh_test` over `:parsers_tree`, `//bazel/nvim:nvim_tree` and `//bazel/nvim-treesitter:nvim_treesitter_tree`; see its two files below. It runs sandboxed.

### Files

#### `queries_compile_test.sh`

Purpose: the `:queries_compile_test` driver. It finds the pinned `nvim`, the vendored nvim-treesitter tree and the `parser/` dir in runfiles, and runs `queries_compile_probe.lua` once over every `parser/<lang>.so`, with the XDG dirs under `$TEST_TMPDIR`.

**Decisions**

- The language list is the `.so` files present, not a fixed list, so a grammar added to `:parsers_tree` is covered with no edit here.
- Exit 0 is not enough: every language must print its own `PASS <lang>` line, because exit 0 alone cannot show that the probe received every language.

**Gotchas** — this is the check that a grammar pin and the nvim-treesitter pin agree. A query naming a node type the pinned grammar lacks fails here, such as swift's `nil_literal` against the `0.7.3-with-generated-files` grammar.

#### `queries_compile_probe.lua`

Purpose: the `nvim -l` probe. Arguments are `<ts_dir> <parser_dir> <lang>...`; per language it loads `<parser_dir>/<lang>.so` with `vim.treesitter.language.add`, then compiles `highlights`, and `injections` when any injections file resolves, through `vim.treesitter.query.get`. It prints `PASS <lang>` or `FAIL <lang>: <reason>`, then a `SUMMARY` line, and exits 1 on any failure.

**Decisions**

- A pass requires a `highlights` file from the vendored tree. Neovim's runtime ships its own queries for the parsers it bundles, `c` among them, so a vendored tree missing a language would otherwise pass on Neovim's copy.
- That check compares normalized, unresolved paths. In the sandbox the tree is per-file symlinks whose realpaths leave it, so a realpath prefix never matches.

## Curated grammar status table

All 16 languages in the curated set compile hermetically at their pinned revisions and ship in the bundle, with grammar ABIs 14–15. Every grammar that has a scanner has a C one at these revisions, so no C++ scanner support is needed.

| lang       | repo                                  | subdir        | scanner   | status | notes                                                     |
| ---------- | ------------------------------------- | ------------- | --------- | ------ | --------------------------------------------------------- |
| json       | tree-sitter/tree-sitter-json          | —             | none      | ships  |                                                           |
| go         | tree-sitter/tree-sitter-go            | —             | none      | ships  |                                                           |
| c          | tree-sitter/tree-sitter-c             | —             | none      | ships  |                                                           |
| java       | tree-sitter/tree-sitter-java          | —             | none      | ships  |                                                           |
| css        | tree-sitter/tree-sitter-css           | —             | scanner.c | ships  |                                                           |
| javascript | tree-sitter/tree-sitter-javascript    | —             | scanner.c | ships  |                                                           |
| rust       | tree-sitter/tree-sitter-rust          | —             | scanner.c | ships  |                                                           |
| bash       | tree-sitter/tree-sitter-bash          | —             | scanner.c | ships  |                                                           |
| html       | tree-sitter/tree-sitter-html          | —             | scanner.c | ships  | scanner also includes `src/tag.h` (globbed via `src/*.h`) |
| toml       | tree-sitter-grammars/tree-sitter-toml | —             | scanner.c | ships  |                                                           |
| cpp        | tree-sitter/tree-sitter-cpp           | —             | scanner.c | ships  |                                                           |
| typescript | tree-sitter/tree-sitter-typescript    | `typescript/` | scanner.c | ships  | shares repo + `common/scanner.h` with tsx                 |
| tsx        | tree-sitter/tree-sitter-typescript    | `tsx/`        | scanner.c | ships  | shares repo + `common/scanner.h` with typescript          |
| yaml       | tree-sitter-grammars/tree-sitter-yaml | —             | scanner.c | ships  | custom overlay: `schema.*.c` staged as `textual_hdrs`     |
| kotlin     | fwcd/tree-sitter-kotlin               | —             | scanner.c | ships  |                                                           |
| swift      | alex-pinkus/tree-sitter-swift         | —             | scanner.c | ships  | tag `0.7.4-with-generated-files`; see the swift note      |

**swift note:** `parsers.lua` pins swift at `35245fb`, which commits no `src/parser.c` (`generate = true`), and Wolf compiles grammars without the tree-sitter CLI. So the pin is `82bb3a5`, the `0.7.4-with-generated-files` tag, the nearest generated-files tag after that revision. Its source commit, `171fa3b`, is three commits newer than `35245fb`: `scanner.c` is identical, while `grammar.js` differs (Swift 6 syntax). Linux builds the `35245fb` grammar, so swift is the one parser the two platforms do not share; `:queries_compile_test` is what shows the vendored swift queries still compile against it.

**markdown / markdown_inline** are deliberately excluded: the pinned Neovim already ships them at `Contents/Resources/nvim/lib/nvim/parser/`.

### Dropping a grammar

A grammar that cannot compile hermetically after a genuine attempt is removed from `extensions.bzl`, from `use_repo(...)` in `MODULE.bazel`, and from the main BUILD's `copy_to_directory` srcs — and recorded here, with status `dropped` and the reason. One grammar never blocks the rest.

The tree test and the bundle-layout test fail only on the low-risk core (go, rust, typescript, tsx, json); the higher-risk grammars (swift, kotlin, java) are noted rather than asserted, so a future drop does not break those tests. **Dropping one of the asserted grammars means updating the tree and bundle tests as well.**

## Adding a language

1. Find the grammar's repo URL and revision in `lua/nvim-treesitter/parsers.lua` at the pinned nvim-treesitter commit. An entry with `generate = true` commits no `src/parser.c`, so pin the nearest later commit that does, as swift does.
2. Download the tarball and compute `sha256`: `curl -L https://github.com/<repo>/archive/<rev>.tar.gz -o g.tar.gz && shasum -a 256 g.tar.gz`. Confirm whether it carries a `src/scanner.c`, and that the scanner is C rather than C++, at that revision.
3. Add a `(repo_name, gh_repo, rev, sha256, scanner?)` tuple to `root_grammars` in `extensions.bzl`, or a custom overlay for the odd cases above.
4. Add its `ts_<lang>` to `use_repo(treesitter_parsers, ...)` in `MODULE.bazel` and its `@ts_<lang>//:<lang>_parser` to the `copy_to_directory` srcs in `BUILD.bazel`.
5. `bazel build @ts_<lang>//:<lang>.so` to verify it compiles; then regenerate `MODULE.bazel.lock` and re-run `bazel test //bazel/treesitter-parsers:parsers_tree_test //bazel/treesitter-parsers:queries_compile_test`.
6. Add the language to `wolfTreesitterLanguages` in `flake.nix`, which is the Linux half of the same set. The curated set is defined here and the flake mirrors it, so a one-sided edit is a cross-platform divergence rather than a preference — and nothing on either side can see one, since no host runs both builds.

## Bumping a pinned grammar

Bump the grammar revisions together with the nvim-treesitter query pin, so queries and parsers stay version-matched. Update the grammar's `rev` and `sha256` in `extensions.bzl` (typescript's and yaml's revs appear a second time, in their overlay's archive-root argument), **re-verify the scanner extension at the new revision** (C or C++, and whether one exists at all), regenerate `MODULE.bazel.lock`, and re-run the tree, `:queries_compile_test` and ABI tests.
