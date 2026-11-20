"""Module extension that downloads the pinned Nerd Fonts symbols-only release archive.

A single `http_archive` repo (`nerd_fonts_symbols`) fetches the sha256-pinned
`NerdFontsSymbolsOnly.tar.xz` release asset of `ryanoasis/nerd-fonts`, a flat
archive with no top directory, and overlays a BUILD file that assembles the Mono
face plus the release's two notices into a `fonts/` TreeArtifact via
`@aspect_bazel_lib//lib:copy_to_directory`. The pin, why the Mono face, and how to
bump it are in this package's README.md.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

# Bump with flake.nix's `wolfVendoredPins`; each side checks only its own sha256.
# crates/wolf-gnome/tests/vendored_pins.rs compares the two, reading each pin from its own line.
_VERSION = "3.5.1"

_SHA256 = "01172f37db8543edb102e5cb5c64101c9f4686630804d49b419aa07b23a69996"

_URL = "https://github.com/ryanoasis/nerd-fonts/releases/download/v{v}/NerdFontsSymbolsOnly.tar.xz".format(v = _VERSION)

# BUILD overlay for the flat release archive; `copy_to_directory` renames the face's two
# notices and nests all three under an inner `fonts/`.
_BUILD_FILE_CONTENT = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# The Mono face, the MIT licence and the README whose icon-set table names each set's
# licence. The proportional SymbolsNerdFont-Regular.ttf maps the same code points at
# wider advances, and 10-nerd-font-symbols.conf's aliases prefer that proportional
# family for every monospace family, so both stay out.
filegroup(
    name = "runtime_files",
    srcs = [
        "SymbolsNerdFontMono-Regular.ttf",
        "LICENSE",
        "README.md",
    ],
)

# The inner `fonts/` is shared with the other font trees: rules_apple strips the
# TreeArtifact's own directory name, so the files land in Contents/Resources/fonts/.
copy_to_directory(
    name = "nerd_fonts_symbols_tree",
    srcs = [":runtime_files"],
    out = "nerd_fonts_symbols_tree",
    replace_prefixes = {
        "SymbolsNerdFontMono-Regular.ttf": "fonts/SymbolsNerdFontMono-Regular.ttf",
        "LICENSE": "fonts/nerd-fonts-symbols-LICENSE",
        "README.md": "fonts/nerd-fonts-symbols-README.md",
    },
)
"""

def _nerd_fonts_symbols_impl(_module_ctx):
    http_archive(
        name = "nerd_fonts_symbols",
        urls = [_URL],
        sha256 = _SHA256,
        build_file_content = _BUILD_FILE_CONTENT,
    )

nerd_fonts_symbols = module_extension(
    implementation = _nerd_fonts_symbols_impl,
    doc = "Downloads the pinned Nerd Fonts symbols-only release archive (data, arch-independent).",
)
