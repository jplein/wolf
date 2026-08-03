# bazel/rg

Hermetic, pinned ripgrep **15.2.0** for bundling into `Wolf.app`.

This package downloads the official ripgrep macOS release tarballs (sha256-pinned, fetched by Bazel — no system or `PATH` dependency), assembles each into a structure-preserving directory artifact, and selects the right one for the host CPU. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/rg/`, so the packaged app runs its own ripgrep for the sidebar content search with no system `rg` required. It mirrors `//bazel/nvim` end-to-end.

## Targets

- **`//bazel/rg:rg_tree`** — an `alias` that `select()`s on `@platforms//cpu` between `@rg_macos_arm64//:rg_tree` and `@rg_macos_x86_64//:rg_tree`. The actual target is a `copy_to_directory` TreeArtifact whose contents are `rg/rg` (plus the shipped docs/completions), exec bit preserved. This is the value bundled into the app and exercised by the version test.
- **`//bazel/rg:rg_version_test`** — an `sh_test` over `:rg_tree`: asserts the binary exists and is executable, and that `rg --version` reports `ripgrep 15.2.0`.

## How it works

`extensions.bzl` is a bzlmod module extension that declares two `http_archive` repos (`rg_macos_arm64`, `rg_macos_x86_64`) for the pinned release tarballs. Each repo gets an overlay `BUILD` file (`build_file_content`) that:

1. globs every extracted file (all under the archive's top dir, `ripgrep-15.2.0-<triple>/`, which contains `rg` at its root), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, remapping the archive's top dir to `rg/` via `replace_prefixes` rather than `http_archive`'s `strip_prefix`, giving `rg/rg`.

The `rg/` inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting the binary under `rg/`, the app reliably gets `Contents/Resources/rg/rg`. `copy_to_directory` preserves file permissions, so the executable bit on `rg` survives into the bundle (required for the nested ad-hoc code signature to remain valid).

`MODULE.bazel` wires the extension up:

```starlark
rg = use_extension("//bazel/rg:extensions.bzl", "rg")
use_repo(rg, "rg_macos_arm64", "rg_macos_x86_64")
```

(`aspect_bazel_lib` is already declared as a `bazel_dep` for the nvim package; the rg overlay reuses it.)

## Bumping the pinned ripgrep version

1. Pick the new release tag (e.g. `15.2.1`) and download both tarballs: `ripgrep-<v>-aarch64-apple-darwin.tar.gz` and `ripgrep-<v>-x86_64-apple-darwin.tar.gz`.
2. Recompute the checksums: `shasum -a 256 ripgrep-<v>-aarch64-apple-darwin.tar.gz` (and the x86_64 one). Update `_VERSION`, `_ARM64_SHA256`, and `_X86_64_SHA256` in `extensions.bzl`.
3. If the tarball's top-level directory name pattern changes (it has been `ripgrep-<v>-<triple>` for these releases), update the `_build_file(...)` archive root arguments in `_rg_impl` accordingly.
4. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
5. Re-run `bazel test //bazel/rg:rg_version_test` and update its expected version (that lives in the test script, owned by the test author).
