# bazel/zig

Hermetic, pinned Zig **0.17.0** for macOS. It is a build tool only: [`//bazel/ghostty-vt`](../ghostty-vt/README.md) runs it to build libghostty-vt, and nothing ships it. The minor follows libghostty-vt's `build.zig.zon`, as [Building libghostty-vt](../../specs/terminal-panes.md#building-libghostty-vt) says.

## Targets

- **`//bazel/zig:zig`** — an `alias` that `select()`s on `@platforms//cpu` between `@zig_macos_arm64//:zig` and `@zig_macos_x86_64//:zig`, the binary.
- **`//bazel/zig:zig_files`** — the matching release tree. Zig finds its `lib/` beside the binary, so a consumer stages this with it, in the exec configuration as the binary is.
- **`//bazel/zig:zig_version_test`** — an `sh_test` over both: the binary is executable, `zig version` prints exactly `0.17.0`, and `zig env` exits 0, which shows it finds its `lib/`.

## How it works

`extensions.bzl` is a bzlmod module extension that declares two `http_archive` repos, `zig_macos_arm64` and `zig_macos_x86_64`, for the official release tarballs from ziglang.org, with the top directory stripped. Each repo's overlay `BUILD` exports `zig` and a `files` filegroup over the whole tree.

Anything that runs this Zig gives it an empty global cache through `ZIG_GLOBAL_CACHE_DIR`, the test included, for the reason the spec gives.

## Bumping the pinned Zig version

1. Change `_VERSION` in `extensions.bzl`, and `_ARM64_SHA256` and `_X86_64_SHA256` to the sums [ziglang.org's index](https://ziglang.org/download/index.json) lists for `zig-aarch64-macos-<v>.tar.xz` and `zig-x86_64-macos-<v>.tar.xz`.
2. Regenerate and commit `MODULE.bazel.lock`; a plain build does this.
3. Change `EXPECTED_VERSION` in `zig_version_test.sh`, then run `bazel test //bazel/zig:zig_version_test //crates/wolf-vt:binding`.

A new minor comes with a libghostty-vt bump, and moves the flake's `zig_0_17` on Linux in the same change.
