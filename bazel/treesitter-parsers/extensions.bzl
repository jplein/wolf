"""Module extension that downloads + host-compiles a curated set of treesitter parsers.

Each curated grammar gets a sha256-pinned `http_archive` (repo `ts_<lang>`) for its
GitHub source tarball, overlaid with a generated BUILD file (`build_file_content`)
that HOST-COMPILES the grammar's `src/parser.c` (+ optional `src/scanner.c`) into a
loadable shared object `parser/<lang>.so` via `rules_cc`'s `cc_binary` (linkshared),
then exposes it as a `filegroup` (`<lang>_parser`). The main
`//bazel/treesitter-parsers:BUILD.bazel` merges all of those into ONE
structure-preserving `parsers_tree` TreeArtifact bundled into Wolf.app at
`Contents/Resources/parsers/`.

## Why host-compile (not the parsers bundled in nvim, not nix-built)

The pinned Neovim ships seven parsers carrying `highlights` queries and no more
(c, lua, markdown, markdown_inline, query, vim, vimdoc) — measured with --clean against
Neovim 0.12.6 from the Linux flake, not this package's pin; the directory README records
why. To highlight the languages Wolf cares about, the parsers must be present in the
bundle, be ABI-compatible with pinned nvim's treesitter runtime, and — crucially — be
signable, RELOCATABLE host-compiled Mach-O. A nix-built `.so` embeds `/nix/store`
install names and is not relocatable inside a signed `.app`, so we compile from
source with the apple_support host-Xcode cc toolchain (the same toolchain that
builds wolf-core's bundled SQLite). These parsers are VENDORED + SHIPPED and
ENABLED on macOS: the captive init.lua prepends them (with the vendored
nvim-treesitter queries) to runtimepath, so fenced-code injection is live in the
.app. Base markdown highlighting is not what this buys — the pinned nvim starts
treesitter on a markdown buffer on its own.

## The `ts_grammar` overlay template

Grammar sources use `#include "tree_sitter/parser.h"` relative to their own `src/`,
so each `cc_binary` sets `includes = ["<root>/[<subdir>/]src"]` (`-Isrc`). Parser
sources are pure C; a few grammars carry a `src/scanner.c` (all curated grammars'
scanners are C at their pinned revisions — RE-VERIFY on bump). typescript + tsx live
in ONE repo (subdirs `typescript/` and `tsx/`) and their scanners `#include
"../../common/scanner.h"`, so that repo's overlay also stages `common/**` and emits
TWO cc_binaries.

## Drop+LOG policy

A grammar that cannot compile hermetically is removed from this extension, from
`use_repo(...)` in MODULE.bazel, from the main BUILD's assembly, and RECORDED in
`bazel/treesitter-parsers/README.md`'s status table. One grammar never blocks the
rest; the tree/bundle tests HARD-assert only the low-risk core (go, rust,
typescript, tsx, json).

BUILD-only extension: never enters the `wolf-core` dependency graph. To bump a pin:
update the grammar's `rev`/`sha256` below (revisions come from the nvim-treesitter
`lockfile.json` as pinned during the original vendoring — master-era; see the
directory README — repos from its `lua/nvim-treesitter/parsers.lua`), RE-VERIFY the
scanner extension at the new revision, then regenerate `MODULE.bazel.lock`.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

# Emits one grammar's cc rules: a linkshared `<lang>.so`, its copy at `parser/<lang>.so`, and a
# `<lang>_parser` filegroup. `subdir` is the in-repo grammar dir; `common` stages `common/**`.
def _cc_block(archive_root, lang, subdir, scanner, common):
    src_dir = archive_root + (("/" + subdir) if subdir else "") + "/src"
    # Glob parser.c, the tree_sitter/ ABI headers, AND any grammar-private headers
    # that live directly under src/ (e.g. html's `tag.h`, which scanner.c includes).
    srcs = [
        '"{d}/parser.c"'.format(d = src_dir),
        '"{d}/*.h"'.format(d = src_dir),
        '"{d}/tree_sitter/*.h"'.format(d = src_dir),
    ]
    if scanner:
        srcs.append('"{d}/scanner.c"'.format(d = src_dir))
    if common:
        srcs.append('"{r}/common/*.h"'.format(r = archive_root))
    srcs_str = ", ".join(srcs)
    return """
# --- {lang} -----------------------------------------------------------------
# Host-compile parser.c (+scanner.c) into a loadable shared object. `-Isrc` (via
# `includes`) resolves the grammar's `#include "tree_sitter/parser.h"`. `-w`
# silences the noisy generated-parser warnings; `-fPIC` for a shared object.
cc_binary(
    name = "{lang}.so",
    srcs = glob([{srcs}], allow_empty = True),
    includes = ["{src_dir}"],
    copts = ["-std=c11", "-fPIC", "-w"],
    linkshared = True,
    linkstatic = True,
)

# Place the built `.so` at `parser/{lang}.so` (the layout nvim expects on a
# `parser/` runtimepath dir). copy_to_directory in the main BUILD then remaps the
# `parser/` prefix to `parsers/parser/` for the bundled TreeArtifact.
copy_file(
    name = "{lang}_parser_file",
    src = ":{lang}.so",
    out = "parser/{lang}.so",
    allow_symlink = False,
)

filegroup(
    name = "{lang}_parser",
    srcs = [":{lang}_parser_file"],
)
""".format(lang = lang, srcs = srcs_str, src_dir = src_dir)

_BUILD_HEADER = """\
load("@rules_cc//cc:defs.bzl", "cc_binary")
load("@aspect_bazel_lib//lib:copy_file.bzl", "copy_file")

package(default_visibility = ["//visibility:public"])
"""

def _single_grammar_build_file(archive_root, lang, subdir, scanner, common):
    return _BUILD_HEADER + _cc_block(archive_root, lang, subdir, scanner, common)

def _typescript_build_file(archive_root):
    # ONE repo, TWO grammars (typescript/ and tsx/); both scanners include the
    # shared ../../common/scanner.h, so stage common/ for both.
    return (_BUILD_HEADER +
            _cc_block(archive_root, "typescript", "typescript", True, True) +
            _cc_block(archive_root, "tsx", "tsx", True, True))

def _yaml_build_file(archive_root):
    # yaml's scanner.c `#include`s a `schema.<x>.c`, which must be staged but never compiled alone,
    # so it comes in through a cc_library's `textual_hdrs`, not the cc_binary's `srcs`.
    src_dir = archive_root + "/src"
    return _BUILD_HEADER + """
load("@rules_cc//cc:defs.bzl", "cc_library")

# The `schema.*.c` files scanner.c textually includes — staged, never compiled
# standalone.
cc_library(
    name = "yaml_schema_textual",
    textual_hdrs = glob(["{d}/schema.*.c"]),
    includes = ["{d}"],
)

cc_binary(
    name = "yaml.so",
    srcs = glob([
        "{d}/parser.c",
        "{d}/scanner.c",
        "{d}/*.h",
        "{d}/tree_sitter/*.h",
    ], allow_empty = True),
    includes = ["{d}"],
    copts = ["-std=c11", "-fPIC", "-w"],
    linkshared = True,
    linkstatic = True,
    deps = [":yaml_schema_textual"],
)

copy_file(
    name = "yaml_parser_file",
    src = ":yaml.so",
    out = "parser/yaml.so",
    allow_symlink = False,
)

filegroup(
    name = "yaml_parser",
    srcs = [":yaml_parser_file"],
)
""".format(d = src_dir)

def _archive(name, repo, rev, sha256, build_file_content):
    http_archive(
        name = name,
        urls = ["https://github.com/{repo}/archive/{rev}.tar.gz".format(repo = repo, rev = rev)],
        sha256 = sha256,
        build_file_content = build_file_content,
    )

def _treesitter_parsers_impl(_module_ctx):
    # Root grammars: (repo_name, gh_repo, rev, sha256, scanner?).
    root_grammars = [
        ("ts_html", "tree-sitter/tree-sitter-html", "cbb91a0ff3621245e890d1c50cc811bffb77a26b", "f66eff7a7a29fc26fffc675f71386d2457fe1f6fc414dddea7b2779d32eb0bb5", True),
        ("ts_javascript", "tree-sitter/tree-sitter-javascript", "6fbef40512dcd9f0a61ce03a4c9ae7597b36ab5c", "4a720a2da28221f9468783e27bea4c1ba0983f18847bc72eea3b8c70185f2698", True),
        ("ts_css", "tree-sitter/tree-sitter-css", "6e327db434fec0ee90f006697782e43ec855adf5", "80b2dde17dec1a8f9dcf06beb0ca2e4f818922c02fd31e2fc718ca41b250af1b", True),
        ("ts_json", "tree-sitter/tree-sitter-json", "46aa487b3ade14b7b05ef92507fdaa3915a662a3", "b6276cc85e113e3e752b208bcac6351cf4af69fde261606746ce35893c57cd6e", False),
        ("ts_go", "tree-sitter/tree-sitter-go", "5e73f476efafe5c768eda19bbe877f188ded6144", "c5bf6d3b9c61207c61d889e6bd69783338749c91ffee71cbb92f3835272eff44", False),
        ("ts_rust", "tree-sitter/tree-sitter-rust", "e86119bdb4968b9799f6a014ca2401c178d54b5f", "536562c1ff345bb51e3158903e9afcbf5ea0f4963a37598fe1ecd8ab4718a497", True),
        ("ts_c", "tree-sitter/tree-sitter-c", "2a265d69a4caf57108a73ad2ed1e6922dd2f998c", "b79cec7ba0b0133f7d1f406340b294466af514c3e1a042555ed0cc8d5cec5d29", False),
        ("ts_cpp", "tree-sitter/tree-sitter-cpp", "e5cea0ec884c5c3d2d1e41a741a66ce13da4d945", "8154b5b842b7a41f88ce6831738d6743af62e4c85598cf9d2f80384f0f356be0", True),
        ("ts_toml", "tree-sitter-grammars/tree-sitter-toml", "64b56832c2cffe41758f28e05c756a3a98d16f41", "feeb2e1cf531588cdcfb9c57292620151a08597f18a98dad26cc17fd4b544dcb", True),
        ("ts_bash", "tree-sitter/tree-sitter-bash", "0c46d792d54c536be5ff7eb18eb95c70fccdb232", "698387b4d30a32672dc48d6af421a689dd7b9141d259638072d88c95245599cd", True),
        ("ts_java", "tree-sitter/tree-sitter-java", "a7db5227ec40fcfe94489559d8c9bc7c8181e25a", "7402d7b4f4f081569058d3554dad5db718a7496cc2c526c73f949e855b107487", False),
        ("ts_kotlin", "fwcd/tree-sitter-kotlin", "c4ddea359a7ff4d92360b2efcd6cfce5dc25afe6", "42f032cbc1ac701cfd27f23475673d7fa636bdc4a398887237e5fa67c212521e", True),
        # swift's parser.c is generated, so this pins a `with-generated-files` commit rather than the
        # nvim-treesitter lockfile's; see bazel/treesitter-parsers/README.md.
        ("ts_swift", "alex-pinkus/tree-sitter-swift", "31d17fe7e818a2048c808b5c6fdc2dc792f4f5b5", "6be61819aca68710dce8a83dae147474d6c2cba37c48a84daa9f442de96f0be8", True),
    ]
    for name, repo, rev, sha256, scanner in root_grammars:
        lang = name[len("ts_"):]
        _archive(name, repo, rev, sha256, _single_grammar_build_file("{repo}-{rev}".format(repo = repo.split("/")[-1], rev = rev), lang, "", scanner, False))

    # typescript + tsx share ONE repo (subdirs + common/ scanner header).
    _archive(
        "ts_typescript",
        "tree-sitter/tree-sitter-typescript",
        "75b3874edb2dc714fb1fd77a32013d0f8699989f",
        "96ce4d1b513767d414bcca408efd9b49879162cceecdbed79a88e8ad2184f385",
        _typescript_build_file("tree-sitter-typescript-75b3874edb2dc714fb1fd77a32013d0f8699989f"),
    )

    # yaml needs a custom overlay: its scanner.c textually #includes a schema.*.c
    # file that must be staged (textual_hdrs) but not compiled standalone.
    _archive(
        "ts_yaml",
        "tree-sitter-grammars/tree-sitter-yaml",
        "1805917414a9a8ba2473717fd69447277a175fae",
        "14c4d868ea567b3dd3af7a2316c87d5e313ad85f61fba4ca7b09c619df33bf42",
        _yaml_build_file("tree-sitter-yaml-1805917414a9a8ba2473717fd69447277a175fae"),
    )

treesitter_parsers = module_extension(
    implementation = _treesitter_parsers_impl,
    doc = "Downloads + host-compiles a curated set of treesitter parser shared objects (macOS bundle only).",
)
