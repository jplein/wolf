"""Module extension that downloads the pinned nvim-treesitter-textobjects tarball.

A single `http_archive` repo (`nvim_treesitter_textobjects`) fetches the
sha256-pinned GitHub source tarball for the vendored nvim-treesitter-textobjects
plugin and overlays a BUILD file that assembles the extracted tree into a single
structure-preserving TreeArtifact via `@aspect_bazel_lib//lib:copy_to_directory`.

The pin is on the textobjects `main` branch — for branch-consistency with the
sibling `//bazel/nvim-treesitter`, which was re-pinned to nvim-treesitter's `main`
branch (the rewrite / new module API). On the textobjects `main` branch the Lua
modules live under the self-named `lua/nvim-treesitter-textobjects/` layout (NOT
the classic `lua/nvim-treesitter/textobjects/` of the old `master` branch), it
still ships per-language `queries/<lang>/textobjects.scm` at the archive ROOT
(unlike the sibling nvim-treesitter `main`, which moved its queries under
`runtime/queries/` — textobjects `main` did NOT move its queries), and it carries
NO top-level `plugin/` loader (the `master` branch had one; `main` dropped it).
This change VENDORS the plugin only and does NOT enable treesitter textobjects
(not sourced, no keymaps); the queries/modules are shipped for a future
enablement change. The re-pin is purely for branch-consistency and future use.

Like the other pure-Lua vendored plugins (orlando, lighthouse, nvim-treesitter),
this is arch-independent: a SINGLE repo, NO `select()`. The archive's own top
directory (`nvim-treesitter-textobjects-<commit>/`) is remapped to
`nvim-treesitter-textobjects/` via `copy_to_directory`'s `replace_prefixes` (NOT
`strip_prefix`), so the assembled TreeArtifact's contents are
`nvim-treesitter-textobjects/lua/nvim-treesitter-textobjects/**`,
`nvim-treesitter-textobjects/queries/<lang>/textobjects.scm`, and
`nvim-treesitter-textobjects/LICENSE`. The inner directory ensures the app gets
`Contents/Resources/nvim-treesitter-textobjects/...` after rules_apple strips the
TreeArtifact's own top-level dir name.

BUILD-only extension: never enters the `wolf-core` dependency graph. To bump the
pin: change `_COMMIT`, recompute `_SHA256` (`shasum -a 256 <tarball>`), then
regenerate `MODULE.bazel.lock`.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

# Pinned to the textobjects `main` branch (self-named `lua/nvim-treesitter-textobjects/`
# module layout, bundled per-language `queries/<lang>/textobjects.scm` at the archive
# root, NO top-level `plugin/`). Re-pinned to `main` for branch-consistency with the
# sibling //bazel/nvim-treesitter (also on `main`).
_COMMIT = "851e865342e5a4cb1ae23d31caf6e991e1c99f1e"

_SHA256 = "8d5fd6d8f48021f941420053389c0b23ccbe136fb2deacc3872e42492fb8b9fa"

_ARCHIVE_ROOT = "nvim-treesitter-textobjects-" + _COMMIT

_BUILD_FILE_TEMPLATE = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# The vendored subtrees Wolf ships: lua/ (the plugin modules, self-named
# `lua/nvim-treesitter-textobjects/` layout on main), queries/ (the per-language
# textobjects.scm queries, kept at the archive root on main — NOT moved under
# runtime/ like the sibling nvim-treesitter main), plus LICENSE (kept for
# compliance). NOTE: main has NO top-level `plugin/` loader (master did). Deliberately
# EXCLUDE dev-only tests/, scripts/, doc/, README.md, and dotfiles — no runtime role
# for a vendored, inert plugin.
filegroup(
    name = "runtime_files",
    srcs = glob([
        "{archive_root}/lua/**",
        "{archive_root}/queries/**",
        "{archive_root}/LICENSE*",
    ]),
)

# Assemble under a single inner `nvim-treesitter-textobjects/` dir, remapping the
# archive root via replace_prefixes (NOT http_archive strip_prefix). Queries stay at
# `queries/` (main did not relocate them under runtime/), so the ONLY remap is the
# inner-dir one. Output TreeArtifact contains
# nvim-treesitter-textobjects/lua/nvim-treesitter-textobjects/**,
# nvim-treesitter-textobjects/queries/<lang>/textobjects.scm, and LICENSE. The inner
# dir is deliberate: rules_apple strips the bundled TreeArtifact's OWN top-level dir
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
