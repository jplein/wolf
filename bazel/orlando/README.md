# bazel/orlando

Hermetic, pinned **orlando** Neovim plugin for bundling into `Wolf.app`.

This package downloads the pinned GitHub source tarball for the pure-Lua orlando plugin (sha256-pinned, fetched by Bazel — no system or PATH dependency), assembles it into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/orlando/`, and the captive `runtime/nvim/init.lua` conditionally prepends it to `runtimepath` and sources `plugin/orlando.lua`. The load is a strict no-op when the tree is absent (Linux/cargo).

## The pin

- Commit: `c960cf3e1c24fbdd2e6fb30ab9c8022cdc7b2bf9`
- Tarball: https://github.com/jplein/orlando/archive/c960cf3e1c24fbdd2e6fb30ab9c8022cdc7b2bf9.tar.gz
- sha256: `794911361108eb509ab00ec64ed03c229418be32f45ad7a385b6d2d6b057f3c5`
- Archive root dir: `orlando-c960cf3e1c24fbdd2e6fb30ab9c8022cdc7b2bf9`

## Targets

- **`//bazel/orlando:orlando_tree`** — an `alias` to `@orlando//:orlando_tree`. Unlike `//bazel/nvim` there is **no** `select()`: orlando is pure Lua and therefore arch-independent, so there is a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `orlando/plugin/orlando.lua`, `orlando/lua/orlando/**`, `orlando/doc/**`, and `orlando/LICENSE*`. This is the value bundled into the app and exercised by the smoke test.
- **`//bazel/orlando:orlando_tree_test`** — an `sh_test` over `:orlando_tree`: asserts `plugin/orlando.lua` and `lua/orlando/init.lua` landed at the expected paths inside the tree's inner `orlando/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`orlando`) for the pinned source tarball. It gets an overlay `BUILD` file (`build_file_content`) that:

1. globs the vendored subtrees (`plugin/`, `lua/`, `doc/`, `LICENSE*`) under the archive's top dir, `orlando-<commit>/` (deliberately excluding dev-only `test/`, `flake.*`, `.luarc.json`, `README.md`), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, **remapping** the archive's top dir to `orlando/` via `replace_prefixes` (NOT `http_archive`'s `strip_prefix`).

The `orlando/` inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting everything under `orlando/`, the app reliably gets `Contents/Resources/orlando/...`.

`aspect_bazel_lib` is a build-time-only ruleset (already used by `//bazel/nvim`); it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

```starlark
orlando = use_extension("//bazel/orlando:extensions.bzl", "orlando")
use_repo(orlando, "orlando")
```

## Bumping the pinned orlando commit

1. Pick the new commit SHA and update `_COMMIT` in `extensions.bzl`. The archive root name (`orlando-<commit>`) auto-follows.
2. Recompute the checksum: `curl -L https://github.com/jplein/orlando/archive/<commit>.tar.gz -o orlando.tar.gz && shasum -a 256 orlando.tar.gz`. Update `_SHA256`.
3. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/orlando:orlando_tree_test`.
