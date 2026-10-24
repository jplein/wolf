"""Module extension that downloads the pinned nvim-treesitter plugin tarball.

A single `http_archive` repo (`nvim_treesitter`) fetches the sha256-pinned GitHub
source tarball for the vendored nvim-treesitter plugin and overlays a BUILD file
that assembles the extracted tree into a single structure-preserving TreeArtifact
via `@aspect_bazel_lib//lib:copy_to_directory`.

The pin is on nvim-treesitter's **main** branch. This is a decision reversal from
the original `master` pin: master's `queries/markdown/injections.scm` uses the custom
`#set-lang-from-info-string!` predicate, whose frozen-master handler targets a
pre-0.10 query-match API that pinned nvim 0.12.3 removed → `E5108` on every fenced
redraw (injection broken). main's markdown `injections.scm` is pure core-supported
capture-based injection (no custom predicate), which works from the queries on
`runtimepath` alone. On main the queries live under `runtime/queries/` (remapped back
to `queries/` below); the classic `lua/nvim-treesitter/` modules and the `plugin/`
loader are still present (bundled but never sourced — the plugin is used purely as an
on-rtp query provider, no `.setup{}`).

Unlike `//bazel/nvim` (which fetches a per-arch native Neovim release and selects
on `@platforms//cpu`), nvim-treesitter is PURE LUA and therefore arch-independent:
there is a SINGLE repo `nvim_treesitter` and NO `select()`.

Crucially, the archive's own top directory (`nvim-treesitter-<commit>/`) is remapped
to `nvim-treesitter/` (via `copy_to_directory`'s `replace_prefixes`, NOT
`strip_prefix`), and main's `runtime/queries/` is lifted back to `queries/` by a
second remap, so the assembled TreeArtifact's contents are
`nvim-treesitter/queries/**`, `nvim-treesitter/lua/nvim-treesitter/**`,
`nvim-treesitter/plugin/**`, `nvim-treesitter/LICENSE`. This matters because
`rules_apple` strips the bundled TreeArtifact's OWN top-level directory name when
copying it into `Contents/Resources/` — keeping everything under an inner
`nvim-treesitter/` ensures the app gets `Contents/Resources/nvim-treesitter/...`,
robustly even if a future revision adds a new top-level directory.

This is a BUILD-only extension: the vendored plugin is bundled into the macOS
`.app`; it never enters the `wolf-core` dependency graph. To bump the pin: change
`_COMMIT`, recompute `_SHA256` (`shasum -a 256 <tarball>`; the archive root name
auto-follows the commit), then regenerate `MODULE.bazel.lock`.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

# Pinned to `main`: master's `#set-lang-from-info-string!` predicate fails on nvim 0.12.3.
# See bazel/nvim-treesitter/README.md and the module docstring.
_COMMIT = "4916d6592ede8c07973490d9322f187e07dfefac"

_SHA256 = "4e720e38f1a74c07fc8bc9d6264e25ab620df93c6576200f43f2ca0ce5fbdec1"

_ARCHIVE_ROOT = "nvim-treesitter-" + _COMMIT

# BUILD overlay for the extracted tarball; `copy_to_directory` remaps `{archive_root}`, the
# tarball's top directory, to `nvim-treesitter/`.
_BUILD_FILE_TEMPLATE = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# The vendored subtrees Wolf ships: runtime/queries/ (bundled treesitter queries —
# lifted back to queries/ by the remap below; main's core-native markdown injections
# are the reason we pin main), lua/ (the plugin modules), plugin/ (the plugin loader),
# and LICENSE (kept for compliance). Deliberately EXCLUDE dev-only tests/,
# scripts/, doc/, assets/, the committed parser/ (Wolf compiles its own curated
# parsers in //bazel/treesitter-parsers), lockfile.json, README.md, Makefile, and
# dotfiles — no runtime role for a vendored, highlighting-disabled plugin.
filegroup(
    name = "runtime_files",
    srcs = glob([
        "{archive_root}/runtime/queries/**",
        "{archive_root}/lua/**",
        "{archive_root}/plugin/**",
        "{archive_root}/LICENSE*",
    ]),
)

# Assemble under a single inner `nvim-treesitter/` dir, remapping the archive root
# via replace_prefixes (NOT http_archive strip_prefix). Output TreeArtifact
# contains nvim-treesitter/queries/**, nvim-treesitter/lua/nvim-treesitter/**, etc.
# The inner `nvim-treesitter/` is deliberate: rules_apple strips the bundled
# TreeArtifact's OWN top-level dir name when copying into Contents/Resources/, so
# nesting under `nvim-treesitter/` lands it at Contents/Resources/nvim-treesitter/
# robustly.
copy_to_directory(
    name = "nvim_treesitter_tree",
    srcs = [":runtime_files"],
    out = "nvim_treesitter_tree",
    replace_prefixes = {{
        "{archive_root}/runtime/queries": "nvim-treesitter/queries",
        "{archive_root}": "nvim-treesitter",
    }},
)
"""

def _build_file(archive_root):
    return _BUILD_FILE_TEMPLATE.format(archive_root = archive_root)

def _nvim_treesitter_impl(_module_ctx):
    http_archive(
        name = "nvim_treesitter",
        urls = [
            "https://github.com/nvim-treesitter/nvim-treesitter/archive/{c}.tar.gz".format(c = _COMMIT),
        ],
        sha256 = _SHA256,
        build_file_content = _build_file(_ARCHIVE_ROOT),
    )

nvim_treesitter = module_extension(
    implementation = _nvim_treesitter_impl,
    doc = "Downloads the pinned nvim-treesitter plugin tarball (main branch, core-native bundled queries; pure Lua, arch-independent).",
)
