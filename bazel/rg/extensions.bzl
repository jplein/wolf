"""Module extension that downloads the pinned ripgrep 15.2.0 release tarballs.

Two `http_archive` repos (`rg_macos_arm64`, `rg_macos_x86_64`) fetch the official
sha256-pinned release tarballs and overlay a BUILD file that assembles the
extracted tree into a single structure-preserving TreeArtifact via
`@aspect_bazel_lib//lib:copy_to_directory` (which preserves the executable bit on
`rg`).

This mirrors `//bazel/nvim` exactly. The archive's own top directory
(`ripgrep-15.2.0-<triple>/`, which contains `rg` at its root) is remapped to `rg/`
(via `copy_to_directory`'s `replace_prefixes`, NOT `strip_prefix`), so the
assembled TreeArtifact's contents are `rg/rg` (plus the shipped docs/completions).
This matters because `rules_apple` strips the bundled TreeArtifact's OWN top-level
directory name when copying it into `Contents/Resources/` — keeping the binary
under an inner `rg/` ensures the app gets `Contents/Resources/rg/rg` (what
`Project.rgBinPath()` and the bundle-layout test expect).

Arch selection is done by `//bazel/rg:rg_tree` (`select()` on `@platforms//cpu`).
To bump the pin: change the version in the URLs, recompute both sha256 sums
(`shasum -a 256 <tarball>`), update the per-arch archive root names below if the
release directory name changes, then regenerate `MODULE.bazel.lock`.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

_VERSION = "15.2.0"

_ARM64_SHA256 = "3750b2e93f37e0c692657da574d7019a101c0084da05a790c83fd335bad973e4"

_X86_64_SHA256 = "af7825fcc69a2afc7a7aea55fc9af90e26421d8f20fe59df32e233c0b8a231c1"

# BUILD overlay for each extracted tarball; `copy_to_directory` remaps `{archive_root}` to `rg/`,
# preserving the exec bit.
_BUILD_FILE_TEMPLATE = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# Every extracted file lives under the archive's top directory; the generated
# overlay markers (BUILD/REPO/WORKSPACE) sit at the repo root and are not matched.
filegroup(
    name = "all",
    srcs = glob(["{archive_root}/**"]),
)

# Assemble the whole arch tree under a single inner `rg/` directory (remapping the
# archive root), preserving the executable bit on `rg`. `rg_tree` is the
# TreeArtifact bundled into Wolf.app at Contents/Resources/rg/.
copy_to_directory(
    name = "rg_tree",
    srcs = [":all"],
    out = "rg_tree",
    replace_prefixes = {{"{archive_root}": "rg"}},
)
"""

def _build_file(archive_root):
    return _BUILD_FILE_TEMPLATE.format(archive_root = archive_root)

def _rg_impl(_module_ctx):
    http_archive(
        name = "rg_macos_arm64",
        urls = [
            "https://github.com/BurntSushi/ripgrep/releases/download/{v}/ripgrep-{v}-aarch64-apple-darwin.tar.gz".format(v = _VERSION),
        ],
        sha256 = _ARM64_SHA256,
        build_file_content = _build_file("ripgrep-{v}-aarch64-apple-darwin".format(v = _VERSION)),
    )
    http_archive(
        name = "rg_macos_x86_64",
        urls = [
            "https://github.com/BurntSushi/ripgrep/releases/download/{v}/ripgrep-{v}-x86_64-apple-darwin.tar.gz".format(v = _VERSION),
        ],
        sha256 = _X86_64_SHA256,
        build_file_content = _build_file("ripgrep-{v}-x86_64-apple-darwin".format(v = _VERSION)),
    )

rg = module_extension(
    implementation = _rg_impl,
    doc = "Downloads the pinned ripgrep 15.2.0 macOS release tarballs (arm64 + x86_64).",
)
