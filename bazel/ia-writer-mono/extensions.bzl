"""Module extension that downloads the pinned iA Writer Mono font source tarball.

A single `http_archive` repo (`ia_writer_mono`) fetches the sha256-pinned GitHub
COMMIT source tarball for iA Writer Mono and overlays a BUILD file that assembles
the four shipped iA Writer Mono S static TTF faces (Regular/Italic/Bold/
BoldItalic) plus the iA SIL Open Font License into a single structure-preserving
TreeArtifact via `@aspect_bazel_lib//lib:copy_to_directory`.

Unlike `//bazel/jetbrains-mono` (which pins a flat RELEASE-asset zip), the
`iaolo/iA-Fonts` repo publishes NO release assets, so — like `//bazel/orlando` —
this pins a COMMIT source tarball (`/archive/<commit>.tar.gz`) whose contents are
wrapped in a top directory `iA-Fonts-<commit>/`.

iA Writer Mono is DATA (arch-independent TTFs + a text license) and therefore
arch-independent: there is a SINGLE repo `ia_writer_mono` and NO `select()`.

The four static faces live under `iA Writer Mono/Static/` (note the SPACES in the
`iA Writer Mono` directory name), and the license is PER-FONT-DIR at `iA Writer
Mono/LICENSE.md` (SIL OFL 1.1, named LICENSE.md — NOT OFL.txt). To de-risk the
spaces in glob patterns, the `runtime_files` filegroup lists the five source
files EXPLICITLY.

`copy_to_directory`'s `replace_prefixes` remaps `iA Writer Mono/Static` -> `fonts`
(flattening the spaced dir away) and `iA Writer Mono/LICENSE.md` ->
`fonts/LICENSE.md`, so the assembled TreeArtifact's contents are
`fonts/iAWriterMonoS-Regular.ttf`, `fonts/iAWriterMonoS-Italic.ttf`,
`fonts/iAWriterMonoS-Bold.ttf`, `fonts/iAWriterMonoS-BoldItalic.ttf`, and
`fonts/LICENSE.md`. The inner `fonts/` dir is deliberate and is the SAME inner
dir JetBrains Mono uses: `rules_apple` strips the bundled TreeArtifact's OWN
top-level directory name when copying it into `Contents/Resources/`, so nesting
everything under an inner `fonts/` lands the app's fonts at
`Contents/Resources/fonts/...` (where `FontRegistration` registers them with
CoreText and the bundle-layout test expects them). iA Writer Mono S is the
DEFAULT UI/grid font.

This is a BUILD-only extension: the vendored fonts are bundled into the macOS
`.app` and registered with CoreText by `FontRegistration.swift`; they never enter
the `wolf-core` dependency graph. To bump the pin: change `_COMMIT`, recompute
`_SHA256` (`curl -L <url> -o f.tgz && shasum -a 256 f.tgz`; confirm the TTF paths
with `tar -tzf f.tgz`; the archive root name auto-follows the commit), regenerate
`MODULE.bazel.lock`, then update `flake.nix`'s `iaWriterMono` to the SAME commit
and sha256.

BUMP BOTH TOGETHER: this file and `flake.nix`. The Linux devShell vendors this
same tarball into its pinned fontconfig from its OWN copy of the commit and the
sha256, so the asset has one pin expressed in two files. A bump made here alone
leaves Linux rendering the OLD faces and nothing catches it — each side fetches
successfully against its own sha256, and neither reads the other's. `flake.nix`
carries the reciprocal note.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

# Bump with flake.nix's `wolfVendoredPins`; each side checks only its own sha256.
# crates/wolf-gnome/tests/vendored_pins.rs compares the two, reading each pin from its own line.
_COMMIT = "f32c04c3058a75d7ce28919ce70fe8800817491b"

_SHA256 = "826767eaea4ce43ccb7ca71327a5c63ac31206b99df400699e6744df6104e1b5"

_ARCHIVE_ROOT = "iA-Fonts-" + _COMMIT

# BUILD overlay for the extracted tarball; `{archive_root}` is its top directory, and
# `copy_to_directory` remaps the faces and licence into an inner `fonts/`.
_BUILD_FILE_TEMPLATE = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# The vendored files Wolf ships: the four static desktop TTF faces of iA Writer
# Mono S (Regular/Italic/Bold/BoldItalic, from `iA Writer Mono/Static/`) plus
# `iA Writer Mono/LICENSE.md` (the iA SIL Open Font License, kept for
# compliance). srcs are listed EXPLICITLY (not glob) to de-risk the SPACES in the
# `iA Writer Mono` directory name. Deliberately EXCLUDE the variable font, the
# Duo/Quattro families, and any README/dev files -- no runtime role in Wolf.
filegroup(
    name = "runtime_files",
    srcs = [
        "{archive_root}/iA Writer Mono/Static/iAWriterMonoS-Regular.ttf",
        "{archive_root}/iA Writer Mono/Static/iAWriterMonoS-Italic.ttf",
        "{archive_root}/iA Writer Mono/Static/iAWriterMonoS-Bold.ttf",
        "{archive_root}/iA Writer Mono/Static/iAWriterMonoS-BoldItalic.ttf",
        "{archive_root}/iA Writer Mono/LICENSE.md",
    ],
)

# Assemble under a single inner `fonts/` dir, remapping the archive layout via
# replace_prefixes (NOT http_archive strip_prefix). Output TreeArtifact contains
# fonts/iAWriterMonoS-{Regular,Italic,Bold,BoldItalic}.ttf and fonts/LICENSE.md.
# The inner `fonts/` is deliberate and shared with JetBrains Mono: rules_apple
# strips the bundled TreeArtifact's OWN top-level dir name when copying into
# Contents/Resources/, so nesting under `fonts/` lands it at
# Contents/Resources/fonts/ robustly.
copy_to_directory(
    name = "ia_writer_mono_tree",
    srcs = [":runtime_files"],
    out = "ia_writer_mono_tree",
    replace_prefixes = {
        "{archive_root}/iA Writer Mono/Static": "fonts",
        "{archive_root}/iA Writer Mono/LICENSE.md": "fonts/LICENSE.md",
    },
)
"""

def _build_file(archive_root):
    return _BUILD_FILE_TEMPLATE.replace("{archive_root}", archive_root)

def _ia_writer_mono_impl(_module_ctx):
    http_archive(
        name = "ia_writer_mono",
        urls = [
            "https://github.com/iaolo/iA-Fonts/archive/{c}.tar.gz".format(c = _COMMIT),
        ],
        sha256 = _SHA256,
        build_file_content = _build_file(_ARCHIVE_ROOT),
    )

ia_writer_mono = module_extension(
    implementation = _ia_writer_mono_impl,
    doc = "Downloads the pinned iA Writer Mono font source tarball (data, arch-independent).",
)
