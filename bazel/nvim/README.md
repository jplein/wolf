# bazel/nvim

Hermetic, pinned Neovim **0.12.3** for bundling into `Wolf.app`.

This package downloads the official Neovim macOS release tarballs (sha256-pinned, fetched by Bazel — no system or `PATH` dependency), assembles each into a structure-preserving directory artifact, and selects the right one for the host CPU. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/nvim/`, so the packaged app launches its own Neovim with no `WOLF_NVIM_BIN` required.

## Targets

- **`//bazel/nvim:nvim_tree`** — an `alias` that `select()`s on `@platforms//cpu` between `@nvim_macos_arm64//:nvim_tree` and `@nvim_macos_x86_64//:nvim_tree`. The actual target is a `copy_to_directory` TreeArtifact whose contents are `nvim/bin/nvim`, `nvim/lib/nvim/parser/*.so`, `nvim/share/nvim/runtime/**` (exec bits preserved). This is the value bundled into the app and exercised by the version test.
- **`//bazel/nvim:nvim_version_test`** — an `sh_test` over `:nvim_tree`: asserts the binary exists and is executable, that `nvim --version` reports `NVIM v0.12.3`, that `VIMRUNTIME` self-locates inside the bundled tree rather than at a system path, and that the `markdown` treesitter parser ships.

## How it works

`extensions.bzl` is a bzlmod module extension that declares two `http_archive` repos (`nvim_macos_arm64`, `nvim_macos_x86_64`) for the pinned release tarballs. Each repo gets an overlay `BUILD` file (`build_file_content`) that:

1. globs every extracted file (all under the archive's top dir, `nvim-macos-<arch>/`), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, remapping the archive's top dir to `nvim/` via `replace_prefixes` rather than `http_archive`'s `strip_prefix`.

The `nvim/` inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting everything under `nvim/`, the app reliably gets `Contents/Resources/nvim/...` even if a future release adds a new top-level directory. `copy_to_directory` preserves file permissions, so the executable bit on `bin/nvim` and the parser `.so`s survives into the bundle (required for the nested ad-hoc code signature to remain valid).

`MODULE.bazel` wires the extension up:

```starlark
bazel_dep(name = "aspect_bazel_lib", version = "2.9.4")

nvim = use_extension("//bazel/nvim:extensions.bzl", "nvim")
use_repo(nvim, "nvim_macos_arm64", "nvim_macos_x86_64")
```

`aspect_bazel_lib` is a build-time-only ruleset (it never enters `wolf-core`'s dependency graph); 2.9.4 declares `bazel_compatibility = [">=6.0.0"]` and has a lean transitive graph compared with later 2.x releases.

## Bumping the pinned Neovim version

1. Pick the new release tag (e.g. `v0.12.4`) and download both tarballs: `nvim-macos-arm64.tar.gz` and `nvim-macos-x86_64.tar.gz`.
2. Recompute the checksums: `shasum -a 256 nvim-macos-arm64.tar.gz` (and the x86_64 one). Update `_VERSION`, `_ARM64_SHA256`, and `_X86_64_SHA256` in `extensions.bzl`.
3. If the tarball's top-level directory name changes (it has been `nvim-macos-<arch>` for these releases), update the `_build_file(...)` archive root arguments in `_nvim_impl` accordingly.
4. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
5. Re-run `bazel test //bazel/nvim:nvim_version_test` and update its expected version (that lives in the test script, owned by the test author).
