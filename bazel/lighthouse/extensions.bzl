"""Module extension that downloads the pinned lighthouse.nvim colorscheme tarball.

A single `http_archive` repo (`lighthouse`) fetches the sha256-pinned GitHub
source tarball for the vendored lighthouse COLORSCHEME and overlays a BUILD file
that assembles the extracted tree into a single structure-preserving TreeArtifact
via `@aspect_bazel_lib//lib:copy_to_directory`.

Unlike `//bazel/nvim` (which fetches a per-arch native Neovim release and selects
on `@platforms//cpu`), lighthouse is PURE LUA and therefore arch-independent:
there is a SINGLE repo `lighthouse` and NO `select()`.

lighthouse is a COLORSCHEME, not a plugin: its only runtime file is
`colors/lighthouse.lua` (plus `LICENSE*` for compliance). There is NO `plugin/`,
`lua/`, or `doc/` directory. Merely placing the dir on `runtimepath` does nothing
for a colorscheme; the captive `init.lua` must ALSO activate it (`background=dark`
+ `:colorscheme lighthouse`).

Crucially, the archive's own top directory (`lighthouse.nvim-<commit>/` — the
upstream repo name INCLUDES `.nvim`) is remapped to `lighthouse/` (via
`copy_to_directory`'s `replace_prefixes`, NOT `strip_prefix`), so the assembled
TreeArtifact's contents are `lighthouse/colors/lighthouse.lua` and
`lighthouse/LICENSE*`. This matters because `rules_apple` strips the bundled
TreeArtifact's OWN top-level directory name when copying it into
`Contents/Resources/` — keeping everything under an inner `lighthouse/` ensures
the app gets `Contents/Resources/lighthouse/...` (what `init.lua` self-locates and
the bundle-layout test expect), robustly even if a future revision adds a new
top-level directory.

This is a BUILD-only extension: the vendored colorscheme is bundled into the macOS
`.app` and activated by the captive `init.lua`; it never enters the `wolf-core`
dependency graph. To bump the pin: change `_COMMIT`, recompute `_SHA256`
(`shasum -a 256 <tarball>`; the archive root name auto-follows the commit), then
regenerate `MODULE.bazel.lock`.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

_COMMIT = "9da2bba190305b2c8d5d300c254a48c487dd4a1e"

_SHA256 = "7e3d0bda062b74fa85261107beadeec515cc6e661adc5d1f6457c5f03997967c"

# The upstream repo name is `lighthouse.nvim`, so the archive's top directory is
# `lighthouse.nvim-<commit>` (NOT `lighthouse-<commit>`). Getting this wrong makes
# the copy_to_directory glob match nothing.
_ARCHIVE_ROOT = "lighthouse.nvim-" + _COMMIT

# BUILD overlay placed at the root of the extracted tarball. `{archive_root}` is
# the tarball's top directory (`lighthouse.nvim-<commit>`); `copy_to_directory`
# remaps that prefix to `lighthouse/` so the output TreeArtifact contains
# `lighthouse/colors/lighthouse.lua` and `lighthouse/LICENSE*`.
_BUILD_FILE_TEMPLATE = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# The vendored subtrees Wolf ships: colors/ (the colorscheme itself) and LICENSE*
# (kept for compliance). lighthouse is a COLORSCHEME, so there is NO plugin/, lua/,
# or doc/ dir. Deliberately EXCLUDE dev-only build/, flake.nix, flake.lock,
# Makefile, README.md, .gitignore, .prettierrc — no runtime role.
filegroup(
    name = "runtime_files",
    srcs = glob([
        "{archive_root}/colors/**",
        "{archive_root}/LICENSE*",
    ]),
)

# Assemble under a single inner `lighthouse/` dir, remapping the archive root via
# replace_prefixes (NOT http_archive strip_prefix). Output TreeArtifact contains
# lighthouse/colors/lighthouse.lua and lighthouse/LICENSE*. The inner `lighthouse/`
# is deliberate: rules_apple strips the bundled TreeArtifact's OWN top-level dir
# name when copying into Contents/Resources/, so nesting under `lighthouse/` lands
# it at Contents/Resources/lighthouse/ robustly.
copy_to_directory(
    name = "lighthouse_tree",
    srcs = [":runtime_files"],
    out = "lighthouse_tree",
    replace_prefixes = {{"{archive_root}": "lighthouse"}},
)
"""

def _build_file(archive_root):
    return _BUILD_FILE_TEMPLATE.format(archive_root = archive_root)

def _lighthouse_impl(_module_ctx):
    http_archive(
        name = "lighthouse",
        urls = [
            "https://github.com/jplein/lighthouse.nvim/archive/{c}.tar.gz".format(c = _COMMIT),
        ],
        sha256 = _SHA256,
        build_file_content = _build_file(_ARCHIVE_ROOT),
    )

lighthouse = module_extension(
    implementation = _lighthouse_impl,
    doc = "Downloads the pinned lighthouse.nvim colorscheme tarball (pure Lua, arch-independent).",
)
