# Plan: Vendor the "orlando" Neovim plugin into the Wolf macOS Bazel build

## Problem statement

Add a build step that vendors the Neovim plugin **orlando**
(https://github.com/jplein/orlando) into the Wolf macOS **Bazel** build, and
update `runtime/nvim/init.lua` to load it. Bazel-only for now; the Linux/cargo
path will build it separately later, so `init.lua` must gracefully **skip**
loading when the vendored plugin is absent.

## Key facts

- **Orlando pin:** commit `c51efff1a2faf9ff90a5334fb18a72901a561cad`
  - Tarball: https://github.com/jplein/orlando/archive/c51efff1a2faf9ff90a5334fb18a72901a561cad.tar.gz
  - sha256: `971aa8149a2a71c0075cb0ba1abde68fb8ebf1fc1f067ed185975ed1026eb25a`
  - archive root dir: `orlando-c51efff1a2faf9ff90a5334fb18a72901a561cad`
- Orlando is a standard nvim plugin: `plugin/orlando.lua` registers
  `FileType`/`BufWinEnter` autocmds calling `require("orlando").attach(buf)`;
  `lua/orlando/{init,config}.lua` + `lua/orlando/features/*.lua`; `doc/`.
  Activates automatically on markdown buffers; `setup()` optional. Guards on
  `vim.g.loaded_orlando`.
- Wolf spawns captive nvim: `nvim --embed --clean -u <init.lua> -i NONE -n`
  (see `crates/wolf-core/src/nvim.rs`). `--clean` relies on runtimepath for
  plugin sourcing.
- Precedent to MIRROR: pinned Neovim tarball fetched by module extension at
  `bazel/nvim/extensions.bzl` (http_archive + `copy_to_directory` with
  `replace_prefixes` remapping archive root to inner dir), aliased at
  `bazel/nvim/BUILD.bazel`, wired in `MODULE.bazel`, bundled by
  `crates/wolf-macos/BUILD.bazel` `macos_application` resources. rules_apple
  STRIPS the TreeArtifact's own top-level dir name, so the inner dir must be
  `orlando` to land at `Contents/Resources/orlando/`.
- `AppDelegate.swift` resolves init.lua via
  `Bundle.main.path(forResource:"init", ofType:"lua")` ->
  `Contents/Resources/init.lua`. Bundled orlando is sibling at
  `Contents/Resources/orlando/`.

## Proposed implementation (subject to architect refinement)

1. `bazel/orlando/extensions.bzl` — module extension: `http_archive` for the
   pinned tarball; overlay BUILD using `@aspect_bazel_lib//lib:copy_to_directory`
   to assemble an `orlando_tree` TreeArtifact remapping archive root -> inner
   `orlando`. Include `plugin/`, `lua/`, `doc/`.
2. `bazel/orlando/BUILD.bazel` — `alias orlando_tree` (single-arch, no select),
   public visibility; smoke `sh_test` asserting `plugin/orlando.lua` +
   `lua/orlando/init.lua` present.
3. `bazel/orlando/README.md` — pin + bump instructions + inner-dir reasoning.
4. `MODULE.bazel` — `use_extension` + `use_repo(orlando, "orlando")`.
5. `crates/wolf-macos/BUILD.bazel` — add `//bazel/orlando:orlando_tree` to
   `macos_application` resources.
6. `crates/wolf-macos/bundle_layout_test.sh` — assert
   `Contents/Resources/orlando/plugin/orlando.lua` + `.../lua/orlando/init.lua`.
7. `runtime/nvim/init.lua` — compute own dir via `debug.getinfo(1,"S").source`;
   if sibling `orlando` dir exists (`vim.uv.fs_stat`), prepend to runtimepath and
   `vim.cmd.runtime("plugin/orlando.lua")`. No-op when absent.
8. Docs: `build.md`, `runtime/nvim/README.md`, `crates/wolf-macos/README.md`,
   `architecture.md` as relevant.

## Testing (real nvim, no mocks)

- `bazel build //crates/wolf-macos:Wolf` bundles orlando.
- `bazel test //crates/wolf-macos:bundle_layout_test` (updated) passes.
- New `bazel/orlando` smoke test passes.
- init.lua still works when orlando absent (bridge_smoke path unchanged);
  consider a test that stages orlando and asserts `vim.g.loaded_orlando`.

## Orchestrator decisions

- **Integration test (d) IS in scope.** `//crates/wolf-ffi:orlando_load` (real
  nvim, stages orlando as a sibling of a temp init.lua, asserts
  `vim.g.loaded_orlando`/`require("orlando")`). Project policy favors real-nvim
  tests; this is the only automated proof of the new init.lua load branch.
- Keep `LICENSE*` in the vendored tree for compliance.

## Status: COMPLETE ✅

- Implementation done, reviewer APPROVED, docs updated + prettier (all 5 md files
  unchanged = already conformant, proseWrap:never).
- Final verification: bundle_layout_test, orlando_tree_test, orlando_load,
  bridge_smoke all PASS. Full suite green earlier (Wolf builds, nvim_version_test,
  WolfKeyMapTests, frame_pack, core dep-graph guard clean).
- Docs updated: build.md, runtime/nvim/README.md, crates/wolf-macos/README.md,
  architecture.md, bazel/orlando/README.md (new). 2 comment NITs fixed.
- MODULE.bazel.lock regenerated (uncommitted). Nothing committed (standing rule).

## Status: reviewer APPROVED → technical-writer (docs + 3 comment NITs) next

### Reviewer verdict: APPROVED (no BLOCKER/SHOULD-FIX)
No-op-when-absent guarantee airtight; orlando_load test genuinely proves the load
(both vim.g.loaded_orlando + require via sentinel "OK"/"OK"), skips can't false-pass;
bundling lands at Contents/Resources/orlando/. 3 cosmetic NITs for technical-writer:
- crates/wolf-macos/BUILD.bazel:69-70 bundle_layout_test header comment omits orlando (e)/(f)
- crates/wolf-macos/bundle_layout_test.sh:7 top one-line summary omits orlando
- crates/wolf-ffi/BUILD.bazel:3-11 Targets list omits orlando_load (pre-existing pattern; optional)



### Coder result — ALL TARGETS PASS
orlando_tree_test PASS; Wolf builds; bundle_layout_test PASS; orlando_load PASS;
bridge_smoke PASS (absent no-op confirmed); nvim_version_test PASS;
WolfKeyMapTests PASS; frame_pack PASS. Core dep-graph guard clean (only false
hit was `glibc` path substring via libc crate — not a GUI toolkit).
MODULE.bazel.lock regenerated (uncommitted). Coder also created
`bazel/orlando/README.md` (CLAUDE.md per-dir README rule).
DOCS (build.md, runtime/nvim/README.md, crates/wolf-macos/README.md,
architecture.md) NOT yet updated — technical-writer step pending.



### Test-writer notes (IMPORTANT for coder)
- NO exec_lua seam exists on `WolfCoreHandle` (verified against wolf-ffi/src/lib.rs
  + bridge_smoke/panes). `orlando_load.rs` therefore drives a typed
  `:lua vim.fn.writefile({...}, "<path>")` Ex command via the existing send_key
  path (like `panes.rs` `send_ex_command`), writing a 2-line sentinel recording
  `vim.g.loaded_orlando` truthiness + `pcall(require,"orlando")` success, then
  reads the file back in Rust and asserts both `"OK"`. Do NOT add a new FFI method.
- Test files written: `bazel/orlando/orlando_tree_test.sh`,
  `crates/wolf-macos/bundle_layout_test.sh` (checks e/f added, uses `$RES`),
  `crates/wolf-ffi/tests/common/mod.rs` (`orlando_dir_or_none()` +
  `copy_dir_recursive()` added), `crates/wolf-ffi/tests/orlando_load.rs`
  (`orlando_loads_from_sibling_dir`, skips when nvim/tree absent, stages sibling
  temp layout via tempfile — already a dev-dep).
- Coder must wire BUILD targets: `//bazel/orlando:orlando_tree_test` sh_test;
  `//crates/wolf-ffi:orlando_load` rust_test (srcs common/mod.rs + orlando_load.rs,
  crate_root tests/orlando_load.rs, tags no-sandbox, size small, data +=
  orlando_tree, env += WOLF_TEST_ORLANDO_DIR = $(rootpath //bazel/orlando:orlando_tree)).

---

# AUTHORITATIVE ARCHITECT PLAN

## 1. Summary

Vendor the pure-Lua Neovim plugin **orlando** (pinned by commit) into the Wolf
macOS Bazel build and load it from the captive `runtime/nvim/init.lua`. A new
`//bazel/orlando` module extension downloads and sha256-verifies the pinned
tarball, then assembles `plugin/`, `lua/`, `doc/`, `LICENSE*` into a
structure-preserving `orlando/` TreeArtifact (mirroring `//bazel/nvim`, but
arch-independent — no `select()`). `//crates/wolf-macos:Wolf` bundles it at
`Contents/Resources/orlando/`. `init.lua` gains a self-locating, guarded block
that prepends a sibling `orlando/` dir to `runtimepath` and sources
`plugin/orlando.lua` — but only when that dir exists, so it is a total no-op on
Linux/cargo and in any bridge test that does not stage orlando.

## 2. Files to create

### 2a. `bazel/orlando/extensions.bzl`
Mirror `bazel/nvim/extensions.bzl`. Single `http_archive` (pure Lua, arch-independent).
Constants:
```starlark
load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")
_COMMIT = "c51efff1a2faf9ff90a5334fb18a72901a561cad"
_SHA256 = "971aa8149a2a71c0075cb0ba1abde68fb8ebf1fc1f067ed185975ed1026eb25a"
_ARCHIVE_ROOT = "orlando-" + _COMMIT
```
URL in impl: `"https://github.com/jplein/orlando/archive/{c}.tar.gz".format(c = _COMMIT)`.
`_BUILD_FILE_TEMPLATE` overlay (placed at extracted repo root; `{archive_root}` =
`orlando-<commit>`):
```starlark
_BUILD_FILE_TEMPLATE = """\
load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")

package(default_visibility = ["//visibility:public"])

# The vendored subtrees Wolf ships: plugin/, lua/orlando/**, doc/, and LICENSE*
# (kept for compliance). Deliberately EXCLUDE test/, flake.nix, flake.lock,
# .luarc.json, README.md — dev-only, no runtime role.
filegroup(
    name = "runtime_files",
    srcs = glob([
        "{archive_root}/plugin/**",
        "{archive_root}/lua/**",
        "{archive_root}/doc/**",
        "{archive_root}/LICENSE*",
    ]),
)

# Assemble under a single inner `orlando/` dir, remapping the archive root via
# replace_prefixes (NOT http_archive strip_prefix). Output TreeArtifact contains
# orlando/plugin/orlando.lua, orlando/lua/orlando/init.lua, orlando/doc/**, etc.
# The inner `orlando/` is deliberate: rules_apple strips the bundled
# TreeArtifact's OWN top-level dir name when copying into Contents/Resources/,
# so nesting under `orlando/` lands it at Contents/Resources/orlando/ robustly.
copy_to_directory(
    name = "orlando_tree",
    srcs = [":runtime_files"],
    out = "orlando_tree",
    replace_prefixes = {{"{archive_root}": "orlando"}},
)
"""
```
`_build_file(archive_root)` formats it; `_orlando_impl(_module_ctx)` calls
`http_archive(name="orlando", urls=[...], sha256=_SHA256, build_file_content=_build_file(_ARCHIVE_ROOT))`.
`orlando = module_extension(implementation=_orlando_impl, doc="Downloads the pinned orlando Neovim plugin tarball (pure Lua, arch-independent).")`.
Module docstring explains: single repo, replace_prefixes → inner `orlando/`,
rules_apple strip reasoning, NO arch select, build-only (never in wolf-core graph).

### 2b. `bazel/orlando/BUILD.bazel`
```starlark
alias(
    name = "orlando_tree",
    actual = "@orlando//:orlando_tree",
    visibility = ["//visibility:public"],
)

sh_test(
    name = "orlando_tree_test",
    srcs = ["orlando_tree_test.sh"],
    data = [":orlando_tree"],
)
```
Header comment: structure-preserving `orlando/` TreeArtifact bundled at
Contents/Resources/orlando/; pure Lua so NO arch select.

### 2c. `bazel/orlando/orlando_tree_test.sh`
Mirror `nvim_version_test.sh` runfiles-resolution style. `set -eu` + `fail()`.
Build `RUNFILES_ROOTS` (TEST_SRCDIR, RUNFILES_DIR, $0.runfiles). Locate tree via
direct candidates then `find -L "$root" -type d -path '*/orlando_tree/orlando'`
fallback. Assert both regular files exist: `<tree>/plugin/orlando.lua` and
`<tree>/lua/orlando/init.lua`. Final `echo "PASS: ..."`. Add NOTE-to-coder
comment to pin the observed direct runfiles path, keeping find as safety net.

### 2d. `bazel/orlando/README.md`
Sections (mirror bazel/nvim/README.md; prettier after): title+one-liner; the pin
(commit, sha256, archive root); targets (`orlando_tree` alias → `@orlando`, NO
select; `orlando_tree_test`); how it works (http_archive + overlay, selective
glob, replace_prefixes→inner orlando + WHY, reuses aspect_bazel_lib build-only,
never in wolf-core graph); bumping the pin (update `_COMMIT`; recompute sha256 via
`curl -L .../<commit>.tar.gz -o orlando.tar.gz && shasum -a 256 orlando.tar.gz`;
archive root auto-follows; regenerate/commit MODULE.bazel.lock; re-run test).

## 3. Files to modify

### 3a. `MODULE.bazel` — after the nvim block (~line 100), append:
```starlark
# --- Vendored orlando Neovim plugin (macOS bundle only) ---------------------
# Downloaded + sha256-verified by the //bazel/orlando module extension; assembled
# into the `orlando/` TreeArtifact bundled into Wolf.app at
# Contents/Resources/orlando/. Pure Lua (arch-independent), so there is a SINGLE
# repo `orlando` (no per-arch select). See bazel/orlando/README.md.
orlando = use_extension("//bazel/orlando:extensions.bzl", "orlando")
use_repo(orlando, "orlando")
```

### 3b. `crates/wolf-macos/BUILD.bazel` — in `macos_application(name="Wolf")`:
Update resources comment to add orlando at Contents/Resources/orlando/. Add
`"//bazel/orlando:orlando_tree"` to resources (after nvim_tree):
```starlark
    resources = [
        "//bazel/nvim:nvim_tree",
        "//bazel/orlando:orlando_tree",
        "//runtime/nvim:init.lua",
    ],
```

### 3c. `crates/wolf-macos/bundle_layout_test.sh` — add checks after (d), style-matched:
```sh
# (e) Contents/Resources/orlando/plugin/orlando.lua exists.
ORLANDO_PLUGIN="$RES/orlando/plugin/orlando.lua"
[ -f "$ORLANDO_PLUGIN" ] || fail "expected vendored orlando plugin at $ORLANDO_PLUGIN, but it is missing"
echo "PASS check (e): $ORLANDO_PLUGIN exists"

# (f) Contents/Resources/orlando/lua/orlando/init.lua exists.
ORLANDO_INIT="$RES/orlando/lua/orlando/init.lua"
[ -f "$ORLANDO_INIT" ] || fail "expected vendored orlando module at $ORLANDO_INIT, but it is missing"
echo "PASS check (f): $ORLANDO_INIT exists"
```
Update header comment block (items e/f) and final PASS line to mention orlando.
(Verify the exact variable name for the Resources dir in the script — plan says
`$RES`; coder must match whatever the script actually uses.)

### 3d. `runtime/nvim/init.lua` — insert AFTER `vim.cmd("syntax on")` (line 62),
BEFORE `_G.wolf_setup_current_file_tracking` (line 69). Use TABS (match file):
```lua
-- Vendored orlando plugin (Bazel macOS bundle only).
--
-- The macOS .app places a sibling `orlando/` tree next to this init.lua at
-- Contents/Resources/orlando/ (bundled by //bazel/orlando). On Linux/cargo it is
-- ABSENT, so this whole block is a strict NO-OP: no runtimepath change, no error.
-- We self-locate init.lua's own directory (rather than assume a fixed path), look
-- for the sibling `orlando/`, and only if it exists prepend it to runtimepath and
-- deterministically source its plugin file — necessary because `--clean` does not
-- scan user plugin dirs, so a plain rtp prepend alone would not auto-source it.
-- orlando itself guards on vim.g.loaded_orlando, so the explicit source is safe.
-- orlando's wrap_indent also sets wrap/linebreak/breakindent; additive & harmless
-- with the wrap/linebreak/smoothscroll set above.
local this_file = debug.getinfo(1, "S").source:sub(2) -- strip the leading '@'
local this_dir = vim.fn.fnamemodify(this_file, ":p:h")
local orlando_dir = this_dir .. "/orlando"
local uv = vim.uv or vim.loop -- vim.uv on nvim 0.12.3; vim.loop fallback
if uv and uv.fs_stat(orlando_dir) then
	vim.opt.runtimepath:prepend(orlando_dir)
	vim.cmd.runtime("plugin/orlando.lua")
end
```

## 4. Tests (real nvim, no mocks) — four layers

- (a) `//bazel/orlando:orlando_tree_test` (new, §2c) — tree contents.
- (b) `//crates/wolf-macos:bundle_layout_test` (modified, §3c) — bundling.
- (c) `//crates/wolf-ffi:bridge_smoke` (unchanged) — absent-path no-op (stages
  only init.lua + nvim_tree; proves init.lua still starts cleanly with orlando
  absent). Coverage only, no change.
- (d) `//crates/wolf-ffi:orlando_load` (NEW, IN SCOPE) — present-path load.
  - New `rust_test` in `crates/wolf-ffi/BUILD.bazel` modeled on `bridge_smoke`/
    `panes` (`tags=["no-sandbox"]`, `size="small"`, reuse `tests/common/mod.rs`),
    `data` adds `"//bazel/orlando:orlando_tree"`, `env` adds
    `"WOLF_TEST_ORLANDO_DIR": "$(rootpath //bazel/orlando:orlando_tree)"`
    alongside existing WOLF_TEST_INIT_LUA / WOLF_TEST_NVIM_DIR.
  - Add helper to `tests/common/mod.rs`: `orlando_dir_or_none() -> Option<PathBuf>`
    reusing existing `resolve_runfile("WOLF_TEST_ORLANDO_DIR")` (returns staged
    `orlando_tree` dir; inner content is `orlando/`).
  - `tests/orlando_load.rs`: `nvim_bin_or_skip()` guard; because init.lua derives
    orlando from its OWN sibling dir, stage a sibling layout in a temp dir T: copy
    staged init.lua → T/init.lua, recursively copy `<orlando_tree>/orlando` →
    T/orlando. Spawn real captive nvim with init_lua=T/init.lua (WOLF_NVIM_BIN
    from nvim), ui_attach, `nvim_exec_lua` returning
    `{ vim.g.loaded_orlando, pcall(require, "orlando") }`; assert loaded; shutdown.
  - Coder must confirm the exact common/mod.rs helpers available
    (resolve_runfile, nvim_bin_or_skip, init_lua_path, start/ui_attach/shutdown
    seam) and mirror how bridge_smoke constructs+starts core.

Full target list that must pass:
`bazel build //crates/wolf-macos:Wolf`; `bazel test //bazel/orlando:orlando_tree_test`;
`//bazel/nvim:nvim_version_test`; `//crates/wolf-macos:bundle_layout_test`;
`//crates/wolf-ffi:bridge_smoke`; `//crates/wolf-ffi:orlando_load`;
`//crates/wolf-macos:WolfKeyMapTests`; `//crates/wolf-ffi:frame_pack`. Plus core
dep-graph guard clean (`bazel query 'deps(//crates/wolf-core:wolf_core)' | grep -iE 'gtk|...' || echo clean`).

## 5. Docs (prettier --write after; no hard line breaks in paragraphs)

- `build.md` "Bundled Neovim (macOS)": add vendored-orlando paragraph
  (Contents/Resources/orlando/, //bazel/orlando extension, reuses
  aspect_bazel_lib, init.lua conditionally loads it / no-op when absent,
  Bazel-only for now); add "bump the vendored orlando plugin" mini-list; add
  `//bazel/orlando:orlando_tree_test` to Test targets + extend bundle_layout_test
  bullet.
- `runtime/nvim/README.md`: add bullet on the conditional orlando load
  (self-locate sibling, prepend rtp + source plugin/orlando.lua because --clean
  doesn't scan plugin dirs, no-op when absent; wrap_indent additive). Update
  "Deferred": plugin baking no longer fully deferred — orlando is first vendored
  plugin; marksman LSP + formatter remain deferred.
- `crates/wolf-macos/README.md`: bundle now includes Contents/Resources/orlando/
  (loaded by init.lua, not Swift); bundle_layout_test now covers orlando.
- `architecture.md`: §8 macOS bundle bullet + reconcile "plugin baking deferred"
  — orlando is first vendored plugin via //bazel/orlando; full wrapNeovim baking
  (marksman, formatter) remains deferred; Bazel-only for now.

## 6. Implementation order

1. `bazel/orlando/extensions.bzl`
2. `bazel/orlando/BUILD.bazel`
3. `bazel/orlando/orlando_tree_test.sh`
4. `MODULE.bazel` (then `//bazel/orlando:orlando_tree_test` must pass)
5. `runtime/nvim/init.lua` (bridge_smoke still passes)
6. `crates/wolf-macos/BUILD.bazel`
7. `crates/wolf-macos/bundle_layout_test.sh` (build Wolf, run test)
8. `crates/wolf-ffi` test (d): common/mod.rs helper + orlando_load.rs + BUILD target
9. Docs + prettier
10. Regenerate/commit `MODULE.bazel.lock` (any build with `--lockfile_mode=update`)

## 7. Deps/lint
No new external Bazel modules (http.bzl + aspect_bazel_lib already present). No new
Rust crates (reuse all_crate_deps(normal_dev=True) + :wolf_ffi). No Go — Go-lint
notes N/A. Buildifier-clean Starlark; prettier on markdown; init.lua uses TABS;
orlando_tree_test.sh POSIX sh with set -eu + fail().

## 8. Status checklist
- [ ] bazel/orlando/extensions.bzl
- [ ] bazel/orlando/BUILD.bazel
- [ ] bazel/orlando/orlando_tree_test.sh
- [ ] MODULE.bazel wired; orlando_tree_test passes
- [ ] runtime/nvim/init.lua guarded load block
- [ ] crates/wolf-macos/BUILD.bazel resources + comment
- [ ] crates/wolf-macos/bundle_layout_test.sh checks (e)/(f)
- [ ] crates/wolf-ffi orlando_load test (common helper + test + BUILD target)
- [ ] bazel/orlando/README.md
- [ ] build.md, runtime/nvim/README.md, crates/wolf-macos/README.md, architecture.md + prettier
- [ ] MODULE.bazel.lock regenerated/committed
- [ ] all §4 targets pass; core dep-graph guard clean
