"""Module extension that downloads the pinned libghostty-vt 1.0.0 release tarballs.

Two `http_file` repos fetch the sha256-pinned source tarball (`ghostty_vt_src`) and its
companion tarball of Zig packages (`ghostty_vt_deps`) from release.files.ghostty.org.
They stay unextracted: Highway's package in the deps tarball carries its own `BUILD`,
`WORKSPACE` and `MODULE.bazel` files, which would split an extracted repo into
subpackages that a glob silently drops. `//bazel/ghostty-vt:libghostty_vt` extracts
both inside its build action instead.

The same release is pinned for Linux as `wolfGhosttyVtPin` in `flake.nix`; bump the two
together. To bump: change `_VERSION`, take both sha256 sums from the release directory's
`SHA256SUMS`, then regenerate `MODULE.bazel.lock`. A bump whose `build.zig.zon` names
another Zig minor moves `//bazel/zig` too.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_file")

_VERSION = "1.0.0"

_SRC_SHA256 = "4a6ef758be721923e1969e7b81b443dcc92d8dd3fb5a39600c03a8817a753676"

_DEPS_SHA256 = "4d5ea5e448e4c7ea8fb55ccbaa8668f2a97d44f438cee831e2b7ecd162d85c29"

_BASE_URL = "https://release.files.ghostty.org/libghostty-vt/{v}/".format(v = _VERSION)

def _ghostty_vt_impl(_module_ctx):
    src = "libghostty-vt-{v}.tar.gz".format(v = _VERSION)
    deps = "libghostty-vt-{v}-deps.tar.gz".format(v = _VERSION)
    http_file(
        name = "ghostty_vt_src",
        urls = [_BASE_URL + src],
        sha256 = _SRC_SHA256,
        downloaded_file_path = src,
    )
    http_file(
        name = "ghostty_vt_deps",
        urls = [_BASE_URL + deps],
        sha256 = _DEPS_SHA256,
        downloaded_file_path = deps,
    )

ghostty_vt = module_extension(
    implementation = _ghostty_vt_impl,
    doc = "Downloads the pinned libghostty-vt 1.0.0 source and dependency tarballs.",
)
