"""Module extension that downloads the pinned JetBrains Mono font release zip.

A single `http_archive` repo (`jetbrains_mono`) fetches the sha256-pinned GitHub
RELEASE-asset zip for JetBrains Mono and overlays a BUILD file that assembles the
four shipped TTF faces (Regular/Italic/Bold/BoldItalic) plus the OFL license into
a single structure-preserving TreeArtifact via
`@aspect_bazel_lib//lib:copy_to_directory`.

Unlike `//bazel/nvim` (which fetches a per-arch native Neovim release and selects
on `@platforms//cpu`), JetBrains Mono is DATA (arch-independent TTFs + a text
license) and therefore arch-independent: there is a SINGLE repo `jetbrains_mono`
and NO `select()`.

The release-asset zip is FLAT (it has NO wrapping top directory): its contents
are `fonts/ttf/JetBrainsMono-<Face>.ttf`, `fonts/webfonts/...`, `fonts/variable/
...`, `OFL.txt`, `AUTHORS.txt`. Wolf ships ONLY the four static desktop TTF faces
(Regular/Italic/Bold/BoldItalic, from `fonts/ttf/`) plus `OFL.txt` (compliance).

`copy_to_directory`'s `replace_prefixes` remaps `fonts/ttf` -> `fonts` (flattening
the `ttf/` subdir away) and `OFL.txt` -> `fonts/OFL.txt`, so the assembled
TreeArtifact's contents are `fonts/JetBrainsMono-Regular.ttf`,
`fonts/JetBrainsMono-Italic.ttf`, `fonts/JetBrainsMono-Bold.ttf`,
`fonts/JetBrainsMono-BoldItalic.ttf`, and `fonts/OFL.txt`. The inner `fonts/` dir
is deliberate: `rules_apple` strips the bundled TreeArtifact's OWN top-level
directory name when copying it into `Contents/Resources/`, so nesting everything
under an inner `fonts/` lands the app's fonts at `Contents/Resources/fonts/...`
(where `FontRegistration` registers them with CoreText and the bundle-layout test
expects them), robustly even if a future release adds a new top-level entry.

This is a BUILD-only extension: the vendored fonts are bundled into the macOS
`.app` and registered with CoreText by `FontRegistration.swift`; they never enter
the `wolf-core` dependency graph. To bump the pin: change `_VERSION`, recompute
`_SHA256` (`curl -L <url> -o f.zip && shasum -a 256 f.zip`; confirm the TTF paths
with `unzip -l f.zip`), then regenerate `MODULE.bazel.lock`.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

_VERSION = "2.304"

_URL = "https://github.com/JetBrains/JetBrainsMono/releases/download/v{v}/JetBrainsMono-{v}.zip".format(v = _VERSION)

_SHA256 = "6f6376c6ed2960ea8a963cd7387ec9d76e3f629125bc33d1fdcd7eb7012f7bbf"

# BUILD overlay for the flat release zip; `copy_to_directory` remaps `fonts/ttf` and `OFL.txt`
# into an inner `fonts/`.
_BUILD_FILE_CONTENT = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# The vendored files Wolf ships: the four static desktop TTF faces
# (Regular/Italic/Bold/BoldItalic, from `fonts/ttf/`) plus `OFL.txt` (the SIL Open
# Font License, kept for compliance). Deliberately EXCLUDE the webfonts, the
# variable fonts, the "NL" (no-ligature) faces, the other weights, and
# AUTHORS.txt / README-style files — no runtime role in Wolf.
filegroup(
    name = "runtime_files",
    srcs = [
        "fonts/ttf/JetBrainsMono-Regular.ttf",
        "fonts/ttf/JetBrainsMono-Italic.ttf",
        "fonts/ttf/JetBrainsMono-Bold.ttf",
        "fonts/ttf/JetBrainsMono-BoldItalic.ttf",
        "OFL.txt",
    ],
)

# Assemble under a single inner `fonts/` dir, remapping the archive layout via
# replace_prefixes (NOT http_archive strip_prefix). Output TreeArtifact contains
# fonts/JetBrainsMono-{Regular,Italic,Bold,BoldItalic}.ttf and fonts/OFL.txt. The
# inner `fonts/` is deliberate: rules_apple strips the bundled TreeArtifact's OWN
# top-level dir name when copying into Contents/Resources/, so nesting under
# `fonts/` lands it at Contents/Resources/fonts/ robustly.
copy_to_directory(
    name = "jetbrains_mono_tree",
    srcs = [":runtime_files"],
    out = "jetbrains_mono_tree",
    replace_prefixes = {
        "fonts/ttf": "fonts",
        "OFL.txt": "fonts/OFL.txt",
    },
)
"""

def _jetbrains_mono_impl(_module_ctx):
    http_archive(
        name = "jetbrains_mono",
        urls = [_URL],
        sha256 = _SHA256,
        build_file_content = _BUILD_FILE_CONTENT,
    )

jetbrains_mono = module_extension(
    implementation = _jetbrains_mono_impl,
    doc = "Downloads the pinned JetBrains Mono font release zip (data, arch-independent).",
)
