"""Module extension that downloads the pinned nvim-treesitter-textobjects tarball.

A single `http_archive` repo (`nvim_treesitter_textobjects`) fetches the
sha256-pinned GitHub source tarball for the vendored nvim-treesitter-textobjects
plugin and overlays a BUILD file that assembles the extracted tree into a single
structure-preserving TreeArtifact via `@aspect_bazel_lib//lib:copy_to_directory`.

The pin is on the textobjects `master` branch — the FROZEN / legacy branch that
matches the sibling `//bazel/nvim-treesitter` (also pinned to `master`, legacy
module API + bundled queries). Mixing a `main` textobjects with a `master`
nvim-treesitter is a latent incompatibility, so both are pinned to `master`. The
master branch's Lua modules live under `lua/nvim-treesitter/textobjects/` (the
classic layout, plus a top-level `lua/nvim-treesitter-textobjects.lua` entry) and
it still ships per-language `queries/**/textobjects.scm` and a top-level `plugin/`
loader. This change VENDORS the plugin only and does NOT enable treesitter
textobjects; the queries/modules are shipped for a future enablement change.

Like the other pure-Lua vendored plugins (orlando, lighthouse, nvim-treesitter),
this is arch-independent: a SINGLE repo, NO `select()`. The archive's own top
directory (`nvim-treesitter-textobjects-<commit>/`) is remapped to
`nvim-treesitter-textobjects/` via `copy_to_directory`'s `replace_prefixes` (NOT
`strip_prefix`), so the assembled TreeArtifact's contents are
`nvim-treesitter-textobjects/lua/nvim-treesitter/textobjects/**`,
`nvim-treesitter-textobjects/queries/**`, `nvim-treesitter-textobjects/plugin/**`,
`nvim-treesitter-textobjects/LICENSE`. The inner directory ensures the app gets
`Contents/Resources/nvim-treesitter-textobjects/...` after rules_apple strips the
TreeArtifact's own top-level dir name.

BUILD-only extension: never enters the `wolf-core` dependency graph. To bump the
pin: change `_COMMIT`, recompute `_SHA256` (`shasum -a 256 <tarball>`), then
regenerate `MODULE.bazel.lock`.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

# Pinned to the textobjects frozen `master` branch (classic `lua/nvim-treesitter/textobjects/`
# module layout, bundled per-language `queries/**/textobjects.scm`, top-level `plugin/`).
_COMMIT = "5ca4aaa6efdcc59be46b95a3e876300cfead05ef"

_SHA256 = "c38279bbfedd4a1859d5f3ebdb8641201b3140365c197b68d8a9be092ef7b686"

_ARCHIVE_ROOT = "nvim-treesitter-textobjects-" + _COMMIT

_BUILD_FILE_TEMPLATE = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# The vendored subtrees Wolf ships: lua/ (the plugin modules, classic
# `lua/nvim-treesitter/textobjects/` layout on master), queries/ (the per-language
# textobjects.scm queries), plugin/ (the plugin loader — master ships one, unlike
# the textobjects `main` branch), plus LICENSE (kept for compliance). Deliberately
# EXCLUDE dev-only tests/, scripts/, autoload/, doc/, README.md, and dotfiles —
# no runtime role for a vendored plugin.
filegroup(
    name = "runtime_files",
    srcs = glob([
        "{archive_root}/lua/**",
        "{archive_root}/queries/**",
        "{archive_root}/plugin/**",
        "{archive_root}/LICENSE*",
    ]),
)

# Assemble under a single inner `nvim-treesitter-textobjects/` dir, remapping the
# archive root via replace_prefixes (NOT http_archive strip_prefix). The inner dir
# is deliberate: rules_apple strips the bundled TreeArtifact's OWN top-level dir
# name when copying into Contents/Resources/, so nesting under
# `nvim-treesitter-textobjects/` lands it at
# Contents/Resources/nvim-treesitter-textobjects/ robustly.
copy_to_directory(
    name = "nvim_treesitter_textobjects_tree",
    srcs = [":runtime_files"],
    out = "nvim_treesitter_textobjects_tree",
    replace_prefixes = {{"{archive_root}": "nvim-treesitter-textobjects"}},
)
"""

def _build_file(archive_root):
    return _BUILD_FILE_TEMPLATE.format(archive_root = archive_root)

def _nvim_treesitter_textobjects_impl(_module_ctx):
    http_archive(
        name = "nvim_treesitter_textobjects",
        urls = [
            "https://github.com/nvim-treesitter/nvim-treesitter-textobjects/archive/{c}.tar.gz".format(c = _COMMIT),
        ],
        sha256 = _SHA256,
        build_file_content = _build_file(_ARCHIVE_ROOT),
    )

nvim_treesitter_textobjects = module_extension(
    implementation = _nvim_treesitter_textobjects_impl,
    doc = "Downloads the pinned nvim-treesitter-textobjects plugin tarball (pure Lua, arch-independent).",
)
