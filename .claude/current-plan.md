# Current Plan: Vendor the lighthouse.nvim colorscheme

## Problem statement

Vendor the `lighthouse.nvim` Neovim **colorscheme** into the Wolf runtime, mirroring
how the `orlando` plugin is already vendored, and activate its **dark** variant in the
captive `init.lua`.

- Source repo: https://github.com/jplein/lighthouse.nvim
- Pinned commit: `5862a1a78d9f4f38418ccb3aebef275ab927b779`
- Tarball: https://github.com/jplein/lighthouse.nvim/archive/5862a1a78d9f4f38418ccb3aebef275ab927b779.tar.gz
- sha256: `af378b8c90a8bb205c01a5590dd8b4e9bb0e0bfdcf7eed8338f55427ff9329d5`
- Archive root dir: `lighthouse.nvim-5862a1a78d9f4f38418ccb3aebef275ab927b779`

### KEY DIFFERENCE from orlando

lighthouse is a **COLORSCHEME**, not a plugin. Its runtime file is `colors/lighthouse.lua`
(plus `LICENSE` for compliance). There is **NO** `plugin/` dir. Putting the dir on
`runtimepath` alone does nothing for a colorscheme — `init.lua` must ALSO activate it.

**Requested behavior:** when the vendored lighthouse tree is present (macOS bundle only),
set `vim.o.background = "dark"` and run `vim.cmd.colorscheme("lighthouse")`. Keep the
activation isolated/simple because a future change will make dark/light switch
automatically from the OS appearance preference. When the tree is ABSENT
(Linux/cargo/tests that don't stage it), the whole block must be a strict no-op
(no rtp change, no colorscheme call, no error) — exactly like the orlando load block.

## Deliverables (mirror the orlando pattern)

1. `bazel/lighthouse/extensions.bzl` — module extension, single http_archive repo `lighthouse`,
   pure Lua (NO select), overlay BUILD globs `colors/**` + `LICENSE*`, assembles via
   `copy_to_directory` with `replace_prefixes` remapping archive root -> inner `lighthouse/`.
   Output TreeArtifact `lighthouse_tree`.
2. `bazel/lighthouse/BUILD.bazel` — alias `lighthouse_tree` -> `@lighthouse//:lighthouse_tree`,
   plus `sh_test` `lighthouse_tree_test`.
3. `bazel/lighthouse/lighthouse_tree_test.sh` — asserts `colors/lighthouse.lua` landed at the
   expected inner path (model on `orlando_tree_test.sh`).
4. `bazel/lighthouse/README.md` — pin, targets, how it works, how to bump.
5. `MODULE.bazel` — `use_extension` + `use_repo`; regenerate `MODULE.bazel.lock`.
6. `crates/wolf-macos/BUILD.bazel` — add `//bazel/lighthouse:lighthouse_tree` to Wolf
   `macos_application` resources (lands at `Contents/Resources/lighthouse/`). Extend
   `bundle_layout_test.sh` with a check for `Contents/Resources/lighthouse/colors/lighthouse.lua`.
7. `runtime/nvim/init.lua` — lighthouse load+activate block modeled on orlando block; strict
   no-op when absent.
8. `crates/wolf-ffi` — integration test `lighthouse_load.rs` (real nvim, no mocks) asserting the
   colorscheme is actually active (`vim.g.colors_name == "lighthouse"`, `vim.o.background == "dark"`).
   Add `rust_test` target with data `//bazel/lighthouse:lighthouse_tree` + env
   `WOLF_TEST_LIGHTHOUSE_DIR`. Add `lighthouse_dir_or_none()` helper to `tests/common/mod.rs`.
9. Update READMEs (`runtime/nvim/README.md`, `bazel/lighthouse/README.md`, any architecture/build
   docs mentioning orlando). Reformat edited Markdown with `prettier --write`.

## Testing policy (CLAUDE.md)

Prefer real end-to-end/integration tests over mocks — `lighthouse_load` MUST start a real Neovim
and verify the colorscheme is actually active. Run `lighthouse_tree_test`, `bundle_layout_test`,
the wolf-ffi `lighthouse_load` test, and the existing orlando/nvim tests (no regressions).

## Status

- [x] Architect plan (recorded below)
- [x] Tests written (lighthouse_tree_test.sh, lighthouse_load.rs, bundle_layout_test.sh check (g))
- [x] Implementation + tests passing (all new + regression targets PASSED; exec bit on tree_test.sh set)
- [x] Review — PASS (correctness-critical items confirmed; only remaining item is docs)
- [x] Docs — runtime/nvim/README.md, crates/wolf-macos/README.md, architecture.md, build.md,
      bazel/lighthouse/README.md updated + prettier-clean (`proseWrap: never`)

## COMPLETE — all steps done, all tests passing.

---

## Architect's detailed plan

### Verified constants (use everywhere)
- `_COMMIT = "5862a1a78d9f4f38418ccb3aebef275ab927b779"`
- `_SHA256 = "af378b8c90a8bb205c01a5590dd8b4e9bb0e0bfdcf7eed8338f55427ff9329d5"`
- **Archive root: `lighthouse.nvim-<commit>`** (upstream repo name includes `.nvim`!). The Bazel repo
  name, TreeArtifact, inner dir, and bundle path are all the bare token `lighthouse`. Getting
  `_ARCHIVE_ROOT` wrong (dropping `.nvim`) makes the `copy_to_directory` glob match nothing.
- Tarball: `https://github.com/jplein/lighthouse.nvim/archive/<commit>.tar.gz`
- KEEP: `colors/**`, `LICENSE*`. EXCLUDE dev-only: `build/`, `flake.nix`, `flake.lock`, `Makefile`,
  `README.md`, `.gitignore`, `.prettierrc`. (No `plugin/`, `lua/`, or `doc/` exist.)
- Colorscheme name registered by `colors/lighthouse.lua` is `lighthouse`.

### Files to CREATE
- **(Coder)** `bazel/lighthouse/extensions.bzl` — parallel of orlando; `runtime_files` filegroup globs
  only `colors/**` + `LICENSE*`; `copy_to_directory` named `lighthouse_tree`, `replace_prefixes`
  remaps archive root -> inner `lighthouse/`. Single `http_archive(name="lighthouse")`, no `select()`.
- **(Coder)** `bazel/lighthouse/BUILD.bazel` — `alias lighthouse_tree -> @lighthouse//:lighthouse_tree`
  + `sh_test lighthouse_tree_test`.
- **(Test-writer)** `bazel/lighthouse/lighthouse_tree_test.sh` — assert `<tree>/lighthouse/colors/lighthouse.lua`
  is a regular file (hard). Optional soft LICENSE* echo. Keep runfiles multi-candidate + `find -L` fallback.
- **(Coder)** `bazel/lighthouse/README.md` — pin/targets/how-it-works + an **Activation** note (colorscheme
  is inert on rtp until `background=dark` + `:colorscheme lighthouse`). `prettier --write`.
- **(Test-writer)** `crates/wolf-ffi/tests/lighthouse_load.rs` — real nvim; sentinel via
  `:lua vim.fn.writefile({(vim.g.colors_name=="lighthouse" and "OK" or "NO"),(vim.o.background=="dark" and "OK" or "NO")}, "<path>")`.
  Two lines, both must be `OK`. Skip guard uses `common::lighthouse_dir_or_none()`. Stage temp bundle:
  copy init.lua + `copy_dir_recursive(lighthouse_src, T/lighthouse)`; sanity-assert `T/lighthouse/colors/lighthouse.lua`.

### Files to MODIFY
- **(Coder)** `MODULE.bazel` — append lighthouse `use_extension`/`use_repo` block after orlando (line 108).
  Regenerate `MODULE.bazel.lock`.
- **(Coder)** `crates/wolf-macos/BUILD.bazel` — add `"//bazel/lighthouse:lighthouse_tree"` to Wolf
  `resources`; update comments (54-59, 69-72).
- **(Test-writer)** `crates/wolf-macos/bundle_layout_test.sh` — new check **(g)**:
  `$RES/lighthouse/colors/lighthouse.lua` is a file; update header comment + final echo.
- **(Coder)** `runtime/nvim/init.lua` — lighthouse block **AFTER `syntax on`** (line 62) and after the
  orlando block (~line 84), reusing existing `this_dir`/`uv` locals (do NOT re-declare). Strict no-op when
  absent. When present: `runtimepath:prepend`, `vim.o.background="dark"`, `vim.cmd.colorscheme("lighthouse")`.
  Do NOT `runtime()` the colors file — `:colorscheme` finds it on rtp. Ordering matters: `:colorscheme`
  redefines highlight groups that a later `syntax on` would clear.
- **(Coder)** `crates/wolf-ffi/tests/common/mod.rs` — add `lighthouse_dir_or_none()` after
  `orlando_dir_or_none()` (resolves `WOLF_TEST_LIGHTHOUSE_DIR`).
- **(Coder)** `crates/wolf-ffi/BUILD.bazel` — new `rust_test lighthouse_load` copying `orlando_load`:
  srcs `tests/common/mod.rs` + `tests/lighthouse_load.rs`, crate_root the latter, data adds
  `//bazel/lighthouse:lighthouse_tree`, env adds `WOLF_TEST_LIGHTHOUSE_DIR = $(rootpath ...)`.
- **(Coder)** Docs: `runtime/nvim/README.md` (new bullet + Deferred note), `build.md` (bump subsection +
  test targets), `architecture.md` (bundling + phase-status paras), `crates/wolf-macos/README.md`
  (AppDelegate + Building paras). `prettier --write` all.

### init.lua ordering (correctness)
`:colorscheme` and `syntax on` both clear/redefine highlight groups. Activation MUST run AFTER
`vim.cmd("syntax on")` and set `vim.o.background` BEFORE `:colorscheme` (dual-variant scheme reads it).
`termguicolors` already true.

### Coder verification commands
```sh
bazel build //bazel/lighthouse:lighthouse_tree --lockfile_mode=update   # regenerates MODULE.bazel.lock
bazel test //bazel/lighthouse:lighthouse_tree_test
bazel test //crates/wolf-macos:bundle_layout_test
bazel test //crates/wolf-ffi:lighthouse_load
# regression:
bazel test //bazel/orlando:orlando_tree_test //crates/wolf-ffi:orlando_load \
           //crates/wolf-ffi:bridge_smoke //crates/wolf-ffi:panes \
           //crates/wolf-ffi:file_search //crates/wolf-ffi:tree_search \
           //crates/wolf-ffi:mouse_scroll //crates/wolf-ffi:frame_pack
```

### Note
Architect flagged that the harness "Project conventions" mention a Go project (`tarjeta`) — NOT applicable
to this Rust/Swift/Bazel/Lua repo. Ignore Go-specific lint/depguard guidance.
