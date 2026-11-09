# bazel/ghostty-vt

libghostty-vt **1.0.0**, built on macOS from its sha256-pinned release tarballs with the Zig of [`//bazel/zig`](../zig/README.md), and linked statically into [`wolf-vt`](../../crates/wolf-vt/README.md). Linux builds the same release as `wolfGhosttyVt` in `flake.nix`. [Building libghostty-vt](../../specs/terminal-panes.md#building-libghostty-vt) owns the pin, the `zig build` flags, and why both platforms build from source.

## Targets

- **`//bazel/ghostty-vt:libghostty_vt`** — a `ghostty_vt_archive` (`defs.bzl`) whose one output is `libghostty-vt.a`.
- **`//bazel/ghostty-vt:ghostty_vt`** — a `cc_import` over that archive, visible to `//crates/wolf-vt` only.

The package has no test of its own: `//crates/wolf-vt:binding` links the archive and checks the version it reports.

## How it works

`extensions.bzl` declares two `http_file` repos, `ghostty_vt_src` and `ghostty_vt_deps`, for the source tarball and its companion tarball of Zig packages from `release.files.ghostty.org`. The `ghostty_vt_archive` action runs `build_archive.sh`, which extracts both into a scratch directory, runs `zig build` offline against the deps, and copies out `lib/libghostty-vt.a`.

## Decisions

- `http_file`, not `http_archive`. Highway's package in the deps tarball carries its own `BUILD`, `WORKSPACE` and `MODULE.bazel` files, which would split an extracted repo into subpackages that a `glob` silently drops, so the action extracts both tarballs itself.
- The build calls `/usr/bin/xcrun`, which needs the selected Xcode, and `.bazelrc`'s strict action env passes no client variables. The action therefore takes the resolved C toolchain's environment for a C compile (`use_cc_toolchain()`, `cc_common.get_environment_variables`), whose Xcode keys Bazel turns into `DEVELOPER_DIR` and `SDKROOT`.
- Not `apple_support.run_shell`, which reaches the same Xcode: its `_xcode_config` attribute puts `@bazel_tools//tools/objc:host_xcodes` in `wolf-vt`'s graph, and the `objc` pattern in `tools/lint/wolf_core_forbidden_deps.txt` rejects that once `wolf-core` depends on `wolf-vt`.
- A `mktemp` shim leads the build's `PATH`. The library's `libsystem_override.sh` calls bare `mktemp -d`, which on macOS ignores `TMPDIR` and uses the user temp directory, which a sandboxed build may not write. The shim sends exactly `mktemp -d` into `TMPDIR` and passes every other call through, so the pinned source stays unpatched.
- `build_archive.sh` sets its own `PATH=/usr/bin:/bin`, since the action env carries only the Xcode keys, and the action is `requires-darwin`.
- The empty global cache goes through `ZIG_GLOBAL_CACHE_DIR`, because Zig 0.17's `zig build` has no `--global-cache-dir` flag.
- Scratch lives beside the declared output, as `<out>.build`, and is removed on exit, never under a literal `/tmp`, which a sandboxed build may not write.
- Only the static archive leaves the action. `wolf-vt` declares the C API by hand and reads no header.

## Bumping the pinned release

1. Change `_VERSION` in `extensions.bzl`, and `_SRC_SHA256` and `_DEPS_SHA256` to the sums in the release directory's `SHA256SUMS`.
2. Change `wolfGhosttyVtPin` in `flake.nix` to the same version and sums.
3. When the new `build.zig.zon`'s `minimum_zig_version` names another minor, bump [`//bazel/zig`](../zig/README.md#bumping-the-pinned-zig-version) and the flake's `zig_0_17` too.
4. Re-derive the `LIBRARY_LICENSES` rows in `crates/wolf-core/src/licenses.rs` from the new tarballs. Each row's provenance names a tarball sha256, and the linked third-party code is whatever the new `deps/libghostty-vt-third-party.json` lists.
5. Regenerate and commit `MODULE.bazel.lock`, change `PINNED_VERSION` in `crates/wolf-vt/tests/binding.rs`, and run `bazel test //crates/wolf-vt:binding` here and the gate on Linux.
