"""Module extension that downloads the pinned orlando Neovim plugin tarball.

A single `http_archive` repo (`orlando`) fetches the sha256-pinned GitHub source
tarball for the vendored orlando plugin and overlays a BUILD file that assembles
the extracted tree into a single structure-preserving TreeArtifact via
`@aspect_bazel_lib//lib:copy_to_directory`.

Unlike `//bazel/nvim` (which fetches a per-arch native Neovim release and selects
on `@platforms//cpu`), orlando is PURE LUA and therefore arch-independent: there
is a SINGLE repo `orlando` and NO `select()`.

Crucially, the archive's own top directory (`orlando-<commit>/`) is remapped to
`orlando/` (via `copy_to_directory`'s `replace_prefixes`, NOT `strip_prefix`), so
the assembled TreeArtifact's contents are `orlando/plugin/orlando.lua`,
`orlando/lua/orlando/init.lua`, `orlando/doc/**`, `orlando/LICENSE*`. This matters
because `rules_apple` strips the bundled TreeArtifact's OWN top-level directory
name when copying it into `Contents/Resources/` — keeping everything under an
inner `orlando/` ensures the app gets `Contents/Resources/orlando/...` (what
`init.lua` self-locates and the bundle-layout test expect), robustly even if a
future revision adds a new top-level directory.

This is a BUILD-only extension: the vendored plugin is bundled into the macOS
`.app` and loaded by the captive `init.lua`; it never enters the `wolf-core`
dependency graph. To bump the pin: change `_COMMIT`, recompute `_SHA256`
(`shasum -a 256 <tarball>`; the archive root name auto-follows the commit), then
regenerate `MODULE.bazel.lock`.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

_COMMIT = "c51efff1a2faf9ff90a5334fb18a72901a561cad"

_SHA256 = "971aa8149a2a71c0075cb0ba1abde68fb8ebf1fc1f067ed185975ed1026eb25a"

_ARCHIVE_ROOT = "orlando-" + _COMMIT

# BUILD overlay placed at the root of the extracted tarball. `{archive_root}` is
# the tarball's top directory (`orlando-<commit>`); `copy_to_directory` remaps
# that prefix to `orlando/` so the output TreeArtifact contains
# `orlando/plugin/orlando.lua`, `orlando/lua/orlando/init.lua`, `orlando/doc/**`,
# and `orlando/LICENSE*`.
_BUILD_FILE_TEMPLATE = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# The vendored subtrees Wolf ships: plugin/, lua/orlando/**, doc/, and LICENSE*
# (kept for compliance). Deliberately EXCLUDE test/, flake.nix, flake.lock,
# .luarc.json, README.md — dev-only, no runtime role.
filegroup(
    name = "runtime_files",
    srcs = glob([
        "{archive_root}/plugin/**",
        "{archive_root}/lua/**",
        "{archive_root}/doc/**",
        "{archive_root}/LICENSE*",
    ]),
)

# Assemble under a single inner `orlando/` dir, remapping the archive root via
# replace_prefixes (NOT http_archive strip_prefix). Output TreeArtifact contains
# orlando/plugin/orlando.lua, orlando/lua/orlando/init.lua, orlando/doc/**, etc.
# The inner `orlando/` is deliberate: rules_apple strips the bundled
# TreeArtifact's OWN top-level dir name when copying into Contents/Resources/,
# so nesting under `orlando/` lands it at Contents/Resources/orlando/ robustly.
copy_to_directory(
    name = "orlando_tree",
    srcs = [":runtime_files"],
    out = "orlando_tree",
    replace_prefixes = {{"{archive_root}": "orlando"}},
)
"""

def _build_file(archive_root):
    return _BUILD_FILE_TEMPLATE.format(archive_root = archive_root)

def _orlando_impl(_module_ctx):
    http_archive(
        name = "orlando",
        urls = [
            "https://github.com/jplein/orlando/archive/{c}.tar.gz".format(c = _COMMIT),
        ],
        sha256 = _SHA256,
        build_file_content = _build_file(_ARCHIVE_ROOT),
    )

orlando = module_extension(
    implementation = _orlando_impl,
    doc = "Downloads the pinned orlando Neovim plugin tarball (pure Lua, arch-independent).",
)
