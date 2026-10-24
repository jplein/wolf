"""Module extension that downloads the pinned Neovim 0.12.3 release tarballs.

Two `http_archive` repos (`nvim_macos_arm64`, `nvim_macos_x86_64`) fetch the
official sha256-pinned release tarballs and overlay a BUILD file that assembles
the extracted tree into a single structure-preserving TreeArtifact via
`@aspect_bazel_lib//lib:copy_to_directory` (which preserves the executable bit
on `bin/nvim` and the parser `.so`s).

Crucially, the archive's own top directory (`nvim-macos-<arch>/`) is remapped to
`nvim/` (via `copy_to_directory`'s `replace_prefixes`, NOT `strip_prefix`), so
the assembled TreeArtifact's contents are `nvim/bin/nvim`,
`nvim/lib/nvim/parser/*.so`, `nvim/share/nvim/runtime/**`. This matters because
`rules_apple` strips the bundled TreeArtifact's OWN top-level directory name when
copying it into `Contents/Resources/` — keeping everything under an inner
`nvim/` ensures the app gets `Contents/Resources/nvim/...` (what `AppDelegate`
and the bundle-layout test expect), robustly even if a future release adds a new
top-level directory.

Arch selection is done by `//bazel/nvim:nvim_tree` (`select()` on
`@platforms//cpu`). To bump the pin: change the version in the URLs, recompute
both sha256 sums (`shasum -a 256 <tarball>`), update the per-arch archive root
names below if the release directory name changes, then regenerate
`MODULE.bazel.lock`.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

_VERSION = "0.12.3"

_ARM64_SHA256 = "532da1d00e465a660fa01c3d4991333d09c52107dce7df937368545daca0a14e"

_X86_64_SHA256 = "4b40e318eb7073321fa5fc06d7f60c3c0de1d7ea50ffbaa8b04286f5484d294f"

# BUILD overlay for each extracted tarball; `copy_to_directory` remaps `{archive_root}` to
# `nvim/`, preserving exec bits.
_BUILD_FILE_TEMPLATE = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# Every extracted file lives under the archive's top directory; the generated
# overlay markers (BUILD/REPO/WORKSPACE) sit at the repo root and are not matched.
filegroup(
    name = "all",
    srcs = glob(["{archive_root}/**"]),
)

# Assemble the whole arch tree under a single inner `nvim/` directory (remapping
# the archive root), preserving the hierarchy and executable bits. `nvim_tree`
# is the TreeArtifact bundled into Wolf.app at Contents/Resources/nvim/.
copy_to_directory(
    name = "nvim_tree",
    srcs = [":all"],
    out = "nvim_tree",
    replace_prefixes = {{"{archive_root}": "nvim"}},
)
"""

def _build_file(archive_root):
    return _BUILD_FILE_TEMPLATE.format(archive_root = archive_root)

def _nvim_impl(_module_ctx):
    http_archive(
        name = "nvim_macos_arm64",
        urls = [
            "https://github.com/neovim/neovim/releases/download/v{v}/nvim-macos-arm64.tar.gz".format(v = _VERSION),
        ],
        sha256 = _ARM64_SHA256,
        build_file_content = _build_file("nvim-macos-arm64"),
    )
    http_archive(
        name = "nvim_macos_x86_64",
        urls = [
            "https://github.com/neovim/neovim/releases/download/v{v}/nvim-macos-x86_64.tar.gz".format(v = _VERSION),
        ],
        sha256 = _X86_64_SHA256,
        build_file_content = _build_file("nvim-macos-x86_64"),
    )

nvim = module_extension(
    implementation = _nvim_impl,
    doc = "Downloads the pinned Neovim 0.12.3 macOS release tarballs (arm64 + x86_64).",
)
