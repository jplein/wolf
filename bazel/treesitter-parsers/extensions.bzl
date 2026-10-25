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
(c, lua, markdown, markdown_inline, query, vim, vimdoc) — measured with `--clean` against
the pinned Neovim 0.12.6 macOS tarball. To highlight the languages Wolf cares about, the
parsers must be present in the bundle, be ABI-compatible with pinned nvim's treesitter
runtime, and — crucially — be signable, RELOCATABLE host-compiled Mach-O. A nix-built
`.so` embeds `/nix/store` install names and is not relocatable inside a signed `.app`,
so we compile from source with the apple_support host-Xcode cc toolchain (the same
toolchain that builds wolf-core's bundled SQLite). These parsers are VENDORED + SHIPPED and
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
update the grammar's `rev`/`sha256` below (revisions and repos come from
`lua/nvim-treesitter/parsers.lua` at the `//bazel/nvim-treesitter` commit), RE-VERIFY the
scanner extension at the new revision, run `:queries_compile_test`, then regenerate
`MODULE.bazel.lock`.
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
        ("ts_html", "tree-sitter/tree-sitter-html", "73a3947324f6efddf9e17c0ea58d454843590cc0", "892f6b732e08bcb90918a985fdee58d6a0fd7a90326af601a2536a5d477583fa", True),
        ("ts_javascript", "tree-sitter/tree-sitter-javascript", "58404d8cf191d69f2674a8fd507bd5776f46cb11", "f3e51e9f7b129f62a817551ae22a878dac5c18d71c456d5ad73e9c82d687f33d", True),
        ("ts_css", "tree-sitter/tree-sitter-css", "dda5cfc5722c429eaba1c910ca32c2c0c5bb1a3f", "c47392e483feb9137d8f0acf9ca6e916b820122c260324454a9980c75c98c3c5", True),
        ("ts_json", "tree-sitter/tree-sitter-json", "254c42a6476413b776221e03982ac8ae159eeb72", "5735ec109268bdc25eb67875b96bc891de1f405253a1d3f9528a504249b0f832", False),
        ("ts_go", "tree-sitter/tree-sitter-go", "2346a3ab1bb3857b48b29d779a1ef9799a248cd7", "94d08fc0f727a8dbe03203e2aaf1c5dc33a57e496b583049324eddc79769797b", False),
        ("ts_rust", "tree-sitter/tree-sitter-rust", "77a3747266f4d621d0757825e6b11edcbf991ca5", "dee82ddfd01bfc3a8ed201cc03b56448107d3217a4b3a7a8fc7fa6bc32b2405b", True),
        ("ts_c", "tree-sitter/tree-sitter-c", "b780e47fc780ddc8da13afa35a3f4ed5c157823d", "dc3979e40fce678c27273ba8ab93b48b5acd6b91d1dfde6a4451222d366d689f", False),
        ("ts_cpp", "tree-sitter/tree-sitter-cpp", "c009222808634c1014f82438d4883753516a2c24", "6c6efaf024f5c02fee0403e4e2373346eeadc40828587bcfad9351df2f0a7986", True),
        ("ts_toml", "tree-sitter-grammars/tree-sitter-toml", "64b56832c2cffe41758f28e05c756a3a98d16f41", "feeb2e1cf531588cdcfb9c57292620151a08597f18a98dad26cc17fd4b544dcb", True),
        ("ts_bash", "tree-sitter/tree-sitter-bash", "a06c2e4415e9bc0346c6b86d401879ffb44058f7", "879e8951ea2cc82455407e3eda0293319657ec53e83191e0fb5a67430d10d804", True),
        ("ts_java", "tree-sitter/tree-sitter-java", "e10607b45ff745f5f876bfa3e94fbcc6b44bdc11", "95a46b6b7b545b48cd6c3a0d5b1014fed271a63a71164cdc4382d9d441e3609f", False),
        ("ts_kotlin", "fwcd/tree-sitter-kotlin", "1852ea17b7f60fb3f9d84e0b1555d56b46b39fb1", "61282c8d3d231fab7d6086d4d0c9ff284824f78d4a7a8d3993d49023fbb739c5", True),
        # parsers.lua's swift rev has no parser.c (`generate = true`), so this pins the nearest later
        # `with-generated-files` tag (0.7.4); see bazel/treesitter-parsers/README.md.
        ("ts_swift", "alex-pinkus/tree-sitter-swift", "82bb3a533e0801fd2bbaa11dc49676e10bf41948", "1cec20e92ddee958bbc1b19ff4251bff2b4618e9e55c33a45a0c56ceaa889a5c", True),
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
        "a1c4812a73ec5e089de8e441fdea3a921e8d5079",
        "6548cd059983e8073dfa39f102641f71d045b37f3a05b7ebd799ad17df8471d1",
        _yaml_build_file("tree-sitter-yaml-a1c4812a73ec5e089de8e441fdea3a921e8d5079"),
    )

treesitter_parsers = module_extension(
    implementation = _treesitter_parsers_impl,
    doc = "Downloads + host-compiles a curated set of treesitter parser shared objects (macOS bundle only).",
)
