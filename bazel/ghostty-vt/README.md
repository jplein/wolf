# bazel/ghostty-vt

libghostty-vt **1.0.0**, built on macOS from its sha256-pinned release tarballs with the Zig of [`//bazel/zig`](../zig/README.md), and linked statically into [`wolf-vt`](../../crates/wolf-vt/README.md). The same release supplies Ghostty's `xterm-ghostty` terminfo entry, which this package compiles for the app bundle. Linux builds both from the same release in `wolfGhosttyVt` in `flake.nix`. [Building libghostty-vt](../../specs/terminal-panes.md#building-libghostty-vt) owns the pin, the `zig build` flags, and why both platforms build from source.

## Targets

- **`//bazel/ghostty-vt:libghostty_vt`** — a `ghostty_vt_archive` (`defs.bzl`) whose one output is `libghostty-vt.a`. It is visible to `//tools/lint` as well, whose fix launchers stage it for Cargo ([why](../../tools/lint/README.md#caveats)).
- **`//bazel/ghostty-vt:ghostty_vt`** — a `cc_import` over that archive, visible to `//crates/wolf-vt` only.
- **`//bazel/ghostty-vt:ghostty_terminfo`** — a `ghostty_terminfo` (`defs.bzl`) whose one output is a TreeArtifact holding `terminfo/78/xterm-ghostty` and `terminfo/67/ghostty`. `//crates/wolf-macos:Wolf` bundles it at `Contents/Resources/terminfo/`, and the `terminal_terminfo` tests of `//crates/wolf-core` and `//crates/wolf-ffi` stage it as `WOLF_TEST_TERMINFO_DIR`; those three packages are its visibility.

The package has no test of its own: `//crates/wolf-vt:binding` links the archive and checks the version it reports, and `//crates/wolf-macos:bundle_layout_test` checks the bundled entry.

## How it works

`extensions.bzl` declares two `http_file` repos, `ghostty_vt_src` and `ghostty_vt_deps`, for the source tarball and its companion tarball of Zig packages from `release.files.ghostty.org`. The `ghostty_vt_archive` action runs `build_archive.sh`, which extracts both into a scratch directory, runs `zig build` offline against the deps, and copies out `lib/libghostty-vt.a`.

The `ghostty_terminfo` action runs `build_terminfo.sh`, which extracts only the source tarball's `src/terminfo/` and runs `zig run` on `terminfo.zig`. That program imports the release's `src/terminfo/ghostty.zig` as the named module `ghostty`, through `-Mghostty=…` and `--dep ghostty`, because Zig refuses an import from outside the root module's directory. It writes the entry as terminfo source to stdout, and `/usr/bin/tic -x -o` compiles that into the output's inner `terminfo/` directory. The inner directory exists because `rules_apple` strips a bundled TreeArtifact's own top-level name, as [`//bazel/lighthouse`](../lighthouse/README.md#how-it-works) explains.

## Reading the pinned source

Nothing needs downloading again to read the release's C headers. On macOS the source tarball sits under the output base once a build reaching `//bazel/ghostty-vt:libghostty_vt` or a fetch has run, and only then. No build leaves it extracted, because the action extracts into its own scratch directory and removes it, so read a header straight out of the tarball:

```sh
nix develop --command bazel fetch --repo=@ghostty_vt_src   # only if no build has run
src="$(nix develop --command bazel info output_base)/external/+ghostty_vt+ghostty_vt_src/file/libghostty-vt-1.0.0.tar.gz"
tar -tzf "$src" | grep '/include/ghostty/vt/'
tar -xzOf "$src" libghostty-vt-1.0.0/include/ghostty/vt/render.h
```

On Linux, inside `nix develop`, the built library installs the same headers under `$WOLF_GHOSTTY_VT_DIR/include/ghostty/vt/`, file for file. The tarball itself is in the store, at the path `nix-store --print-fixed-path sha256 <_SRC_SHA256> libghostty-vt-1.0.0.tar.gz` prints.

## Decisions

- `http_file`, not `http_archive`. Highway's package in the deps tarball carries its own `BUILD`, `WORKSPACE` and `MODULE.bazel` files, which would split an extracted repo into subpackages that a `glob` silently drops, so the action extracts both tarballs itself.
- Zig on the host may call `/usr/bin/xcrun`, which needs the selected Xcode, and `.bazelrc`'s strict action env passes no client variables. Both actions therefore take the resolved C toolchain's environment for a C compile (`use_cc_toolchain()`, `cc_common.get_environment_variables`), whose Xcode keys Bazel turns into `DEVELOPER_DIR` and `SDKROOT`.
- Not `apple_support.run_shell`, which reaches the same Xcode: its `_xcode_config` attribute puts `@bazel_tools//tools/objc:host_xcodes` in `wolf-vt`'s graph, and the `objc` pattern in `tools/lint/wolf_core_forbidden_deps.txt` rejects that, because `wolf-core` depends on `wolf-vt`.
- A `mktemp` shim leads the build's `PATH`. The library's `libsystem_override.sh` calls bare `mktemp -d`, which on macOS ignores `TMPDIR` and uses the user temp directory, which a sandboxed build may not write. The shim sends exactly `mktemp -d` into `TMPDIR` and passes every other call through, so the pinned source stays unpatched.
- `build_archive.sh` and `build_terminfo.sh` set their own `PATH=/usr/bin:/bin`, since the action env carries only the Xcode keys, and both actions are `requires-darwin`.
- The entry is compiled with the system `/usr/bin/tic`, not an ncurses from Bazel or Nix. It writes the hex directory layout, `78/xterm-ghostty`, which the host's `/usr/lib/libncurses` reads, and every program a macOS shell runs links that library. Linux compiles the entry with nixpkgs' `tic` instead, which writes the letter layout, `x/xterm-ghostty`, that nixpkgs' ncurses reads; core accepts either.
- `-x` keeps the entry's extended capabilities, such as `setrgbf` for truecolour, which a plain `tic` drops.
- `build_terminfo.sh` shows `tic`'s stderr only when `tic` fails. Every run warns that the entry's last name, `Ghostty`, reads as a description, and Ghostty's own build discards that warning too.
- No compiled or generated copy of the entry is committed, so the entry always follows the pinned release.
- The empty global cache goes through `ZIG_GLOBAL_CACHE_DIR`, because Zig 0.17's `zig build` has no `--global-cache-dir` flag.
- Scratch lives beside the declared output, as `<out>.build`, and is removed on exit, never under a literal `/tmp`, which a sandboxed build may not write.
- Only the static archive leaves the archive action. `wolf-vt` declares the C API by hand and reads no header.

## Bumping the pinned release

1. Change `_VERSION` in `extensions.bzl`, and `_SRC_SHA256` and `_DEPS_SHA256` to the sums in the release directory's `SHA256SUMS`.
2. Change `wolfGhosttyVtPin` in `flake.nix` to the same version and sums.
3. When the new `build.zig.zon`'s `minimum_zig_version` names another minor, bump [`//bazel/zig`](../zig/README.md#bumping-the-pinned-zig-version) and the flake's `zig_0_17` too.
4. Re-derive the `LIBRARY_LICENSES` rows in `crates/wolf-core/src/licenses.rs` from the new tarballs. Each row's provenance names a tarball sha256, and the linked third-party code is whatever the new `deps/libghostty-vt-third-party.json` lists.
5. Re-check `terminfo.zig` against the new `src/terminfo/Source.zig` and `ghostty.zig`: it calls `ghostty.ghostty.encode` and Zig's `std.Io` writer, either of which a release may move. Linux builds the entry from the same tarball in `wolfGhosttyVt`.
6. Regenerate and commit `MODULE.bazel.lock`, change `PINNED_VERSION` in `crates/wolf-vt/tests/binding.rs`, and run `bazel test //crates/wolf-vt:binding //crates/wolf-core:terminal_terminfo` here and the gate on Linux.
