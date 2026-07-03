# bazel/lighthouse

Hermetic, pinned **lighthouse.nvim** colorscheme for bundling into `Wolf.app`.

This package downloads the pinned GitHub source tarball for the pure-Lua lighthouse colorscheme (sha256-pinned, fetched by Bazel — no system or PATH dependency), assembles it into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/lighthouse/`, and the captive `runtime/nvim/init.lua` conditionally prepends it to `runtimepath` and activates it. The load is a strict no-op when the tree is absent (Linux/cargo).

## Activation

lighthouse is a **colorscheme**, not a plugin. Unlike orlando, merely placing the directory on `runtimepath` does **nothing** — a colorscheme is inert until it is explicitly selected. When the sibling `lighthouse/` tree is present, the captive `init.lua` prepends it to `runtimepath`, sets `vim.o.background = "dark"`, and runs `vim.cmd.colorscheme("lighthouse")` (which finds `colors/lighthouse.lua` on `runtimepath` itself — no `runtime()` of the colors file is needed).

Ordering matters: this activation runs **after** `vim.cmd("syntax on")`, because both `syntax on` and `:colorscheme` clear/redefine highlight groups, so the colorscheme must win by running last. `vim.o.background` is set **before** `:colorscheme` so a dual-variant scheme reads it. A future change will switch dark/light automatically from the OS appearance preference; the activation is kept isolated and simple to make that easy.

## The pin

- Commit: `9da2bba190305b2c8d5d300c254a48c487dd4a1e`
- Tarball: https://github.com/jplein/lighthouse.nvim/archive/9da2bba190305b2c8d5d300c254a48c487dd4a1e.tar.gz
- sha256: `7e3d0bda062b74fa85261107beadeec515cc6e661adc5d1f6457c5f03997967c`
- Archive root dir: `lighthouse.nvim-9da2bba190305b2c8d5d300c254a48c487dd4a1e` (the upstream repo name includes `.nvim`)

This pin defines the two optional **Wolf app-theme highlight groups** — `WolfCursor` (app cursor fill/glyph) and `WolfTitlebar` (macOS titlebar background) — that core tracks via the `wolf_theme` notification (see `runtime/nvim/README.md`); `//crates/wolf-ffi:lighthouse_load` pins the dark-variant values.

## Targets

- **`//bazel/lighthouse:lighthouse_tree`** — an `alias` to `@lighthouse//:lighthouse_tree`. Unlike `//bazel/nvim` there is **no** `select()`: lighthouse is pure Lua and therefore arch-independent, so there is a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `lighthouse/colors/lighthouse.lua` and `lighthouse/LICENSE*`. This is the value bundled into the app and exercised by the smoke test.
- **`//bazel/lighthouse:lighthouse_tree_test`** — an `sh_test` over `:lighthouse_tree`: asserts `colors/lighthouse.lua` landed at the expected path inside the tree's inner `lighthouse/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`lighthouse`) for the pinned source tarball. It gets an overlay `BUILD` file (`build_file_content`) that:

1. globs the vendored subtrees (`colors/`, `LICENSE*`) under the archive's top dir, `lighthouse.nvim-<commit>/` (deliberately excluding dev-only `build/`, `flake.*`, `Makefile`, `README.md`, `.gitignore`, `.prettierrc`), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, **remapping** the archive's top dir to `lighthouse/` via `replace_prefixes` (NOT `http_archive`'s `strip_prefix`).

The `lighthouse/` inner directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting everything under `lighthouse/`, the app reliably gets `Contents/Resources/lighthouse/...`.

Note the archive root: the upstream repo is named `lighthouse.nvim`, so the tarball's top directory is `lighthouse.nvim-<commit>`, **not** `lighthouse-<commit>`. Dropping the `.nvim` would make the `copy_to_directory` glob match nothing.

`aspect_bazel_lib` is a build-time-only ruleset (already used by `//bazel/nvim`); it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

```starlark
lighthouse = use_extension("//bazel/lighthouse:extensions.bzl", "lighthouse")
use_repo(lighthouse, "lighthouse")
```

## Bumping the pinned lighthouse commit

1. Pick the new commit SHA and update `_COMMIT` in `extensions.bzl`. The archive root name (`lighthouse.nvim-<commit>`) auto-follows.
2. Recompute the checksum: `curl -L https://github.com/jplein/lighthouse.nvim/archive/<commit>.tar.gz -o lighthouse.tar.gz && shasum -a 256 lighthouse.tar.gz`. Update `_SHA256`.
3. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
4. Re-run `bazel test //bazel/lighthouse:lighthouse_tree_test`.
