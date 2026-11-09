"""Module extension that downloads the pinned Zig 0.17.0 macOS release tarballs.

Two `http_archive` repos (`zig_macos_arm64`, `zig_macos_x86_64`) fetch the official
sha256-pinned release tarballs from ziglang.org. Each exposes the `zig` binary and a
`files` filegroup holding the whole tree, since Zig finds its `lib/` beside the binary.

Zig is a build tool only: `//bazel/ghostty-vt` runs it to build libghostty-vt, whose
`build.zig.zon` requires Zig 0.17.x. Arch selection is done by `//bazel/zig:zig` and
`//bazel/zig:zig_files` (`select()` on `@platforms//cpu`). To bump the pin: change
`_VERSION`, recompute both sha256 sums from https://ziglang.org/download/index.json,
then regenerate `MODULE.bazel.lock`.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

_VERSION = "0.17.0"

_ARM64_SHA256 = "b607e9b9234790a008116ae5bdb71c6243b84b9fb42a53a9e70fde41c06c536a"

_X86_64_SHA256 = "4f9a1c5269aa17ebda5e6d3c2b89d6cbf36f7d2b22a0306e9ab98f25f95529c6"

_BUILD_FILE = """\
package(default_visibility = ["//visibility:public"])

exports_files(["zig"])

filegroup(
    name = "files",
    srcs = glob(["**"]),
)
"""

def _zig_archive(name, arch):
    root = "zig-{arch}-macos-{v}".format(arch = arch, v = _VERSION)
    http_archive(
        name = name,
        urls = ["https://ziglang.org/download/{v}/{root}.tar.xz".format(v = _VERSION, root = root)],
        sha256 = _ARM64_SHA256 if arch == "aarch64" else _X86_64_SHA256,
        strip_prefix = root,
        build_file_content = _BUILD_FILE,
    )

def _zig_impl(_module_ctx):
    _zig_archive("zig_macos_arm64", "aarch64")
    _zig_archive("zig_macos_x86_64", "x86_64")

zig = module_extension(
    implementation = _zig_impl,
    doc = "Downloads the pinned Zig 0.17.0 macOS release tarballs (arm64 + x86_64).",
)
