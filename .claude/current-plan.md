# Plan: Bundle pinned Neovim 0.12.3 + init.lua into Wolf.app/Contents/Resources

## STATUS: COMPLETE ✅

The macOS app now bundles pinned Neovim 0.12.3 (full tree at
`Contents/Resources/nvim/`, hermetic sha256-pinned Bazel download via the
`//bazel/nvim` module extension + aspect_bazel_lib `copy_to_directory`) and the
captive `init.lua` at `Contents/Resources/init.lua`; `AppDelegate` sets
`WOLF_NVIM_BIN` to the bundled binary (overwrite=0, dev override wins) before
starting the core — so double-clicking Wolf.app works with no shell env.
Verification all green (structure + exec bits + codesign --verify --deep --strict
+ nvim_version_test + bundle_layout_test + regression + denylist; wolf-core
untouched). Reviewer APPROVED; docs reconciled + prettier. Nothing committed
(standing rule).

RESOLVED FOLLOW-UP — bridge_smoke hang (FIXED ✅):
- Root cause (pinpointed via stack-sampling the hung process): the test resolved
  `runtime/nvim/init.lua` from `env!("CARGO_MANIFEST_DIR")`, an EPHEMERAL Bazel
  sandbox path absent at runtime → nvim launched `-u <missing init.lua>` →
  blocked forever on the "Press ENTER" (`wait_return`) prompt → `WolfCore::start`
  RPC never returned. NOT a sandbox artifact, NOT an FFI/product bug (it
  reproduced unsandboxed; the app bundles a readable init.lua so was never
  affected). A secondary off-by-one in the resize assertion (active window grid =
  UI height − cmdline row under ext_multigrid) also surfaced once it ran.
- Fix (test+build only; wolf-core/app UNCHANGED): the `bridge_smoke` rust_test
  now STAGES `//runtime/nvim:init.lua` + `//bazel/nvim:nvim_tree` as runfiles
  (`data` + `$(rootpath)` env `WOLF_TEST_INIT_LUA`/`WOLF_TEST_NVIM_DIR`), resolved
  by `common::{init_lua_path,nvim_bin_or_skip}` (explicit `WOLF_NVIM_BIN` still
  wins for cargo/devShell); the test exports the resolved bin into `WOLF_NVIM_BIN`
  for wolf-core; runs `tags=["no-sandbox"]`, `size="small"`; resize assertion
  corrected to width-exact + height∈[ROWS-2,ROWS]. Result: `bazel test
  //crates/wolf-ffi:bridge_smoke` PASSES (~0.2s), exercising the FULL live
  round-trip (start→type i,x→grid shows x→resize→shutdown) against the pinned
  nvim 0.12.3 — hermetically, on every run. The live nvim→FFI round-trip is now
  COVERED by a passing automated test on the macOS Bazel host. Reviewer APPROVED;
  stale "hang/unverified" docs corrected + prettier'd. Full regression green;
  wolf-core `git status` empty.
- NITs (test-writer, optional): nvim_version_test.sh has the version in two
  places (EXPECTED_VERSION msg + the grep pattern) — unify to one source;
  trim the stale `# NOTE to coder:` scaffolding comments from both .sh tests.
- Forward-looking: the bundled nvim's adhoc/linker signatures will need
  re-signing under hardened-runtime + Developer ID notarization (deferred).
- Scratch: `.claude/verify-results.md` holds the verbatim verification evidence.

## Problem statement

Bundle a pinned Neovim 0.12.3 (full extracted tree) and the captive runtime
`init.lua` into `Wolf.app/Contents/Resources` at BUILD TIME, downloaded
hermetically by Bazel, so the macOS app launches the bundled Neovim and no longer
requires `WOLF_NVIM_BIN` to run — it becomes genuinely double-clickable. Realizes
the macOS half of architecture.md Decision 8.

Context: the wolf-macos AppKit app exists (first increment done). wolf-core
locates nvim ONLY via `WOLF_NVIM_BIN` (`NvimConfig::from_env`); never PATH.
AppDelegate already resolves init.lua via `WOLF_INIT_LUA` →
`Bundle.main.path(forResource:"init",ofType:"lua")` → cwd-relative
`runtime/nvim/init.lua`. All-Bazel build (Bazel 8.7.0; rules_apple 4.5.3,
rules_swift 3.6.1, rules_xcodeproj 4.1.0, apple_support 2.6.1). App target
`//crates/wolf-macos:Wolf`.

## Verified pins (use exactly these — release confirmed, checksums computed)

- arm64: https://github.com/neovim/neovim/releases/download/v0.12.3/nvim-macos-arm64.tar.gz
  sha256 `532da1d00e465a660fa01c3d4991333d09c52107dce7df937368545daca0a14e`
  strip_prefix `nvim-macos-arm64`
- x86_64: https://github.com/neovim/neovim/releases/download/v0.12.3/nvim-macos-x86_64.tar.gz
  sha256 `4b40e318eb7073321fa5fc06d7f60c3c0de1d7ea50ffbaa8b04286f5484d294f`
  strip_prefix `nvim-macos-x86_64`
- Layout (verified): `nvim-macos-<arch>/{bin/nvim, lib/nvim/parser/*.so (incl.
  markdown + markdown_inline), share/nvim/runtime/..., share/man/...}`. nvim
  self-locates runtime+lib relative to the binary path → bundle the WHOLE tree
  preserving structure + exec bits; launch `…/nvim/bin/nvim`.

## Required behavior

- Bundle full nvim tree at `Contents/Resources/nvim/` (binary at
  `Contents/Resources/nvim/bin/nvim`) AND `runtime/nvim/init.lua` at
  `Contents/Resources/init.lua` (so the existing `Bundle.main.path` resolves it).
- App sets `WOLF_NVIM_BIN` to the bundled binary BEFORE `wolf_ffi_start`, ONLY IF
  not already set — `setenv(name, value, /*overwrite=*/0)` so a dev's exported
  `WOLF_NVIM_BIN` still wins. Double-clicking Wolf.app (no shell env) must start
  the bundled nvim and show the grid, NOT the failure alert.

## Known risks (architect addresses; coder VERIFIES by building + launching)

1. rules_apple resource STRUCTURE PRESERVATION (plain `resources` flattens) —
   the `nvim/bin/nvim`, `lib/nvim/parser/*.so`, `share/nvim/runtime/**` hierarchy
   MUST survive into the bundle. Find the exact rules_apple 4.5.3 mechanism;
   verify by unzipping the .app.
2. NESTED CODE SIGNING — Mach-O `bin/nvim` + many `.so` inside Resources is
   nested code; may fail `codesign --verify --deep --strict` or fail to launch
   under hardened runtime. Verify with codesign; escalate (sign nested code /
   dev entitlements / relocate) rather than ship an unlaunchable app. Preserve
   exec bit on bin/nvim + .so.
3. ARCH SELECTION — wire both arm64 + x86_64 via `select()`; dev host is
   aarch64-darwin. If Tahoe is Apple-Silicon-only, x86_64 wired-but-untested.
4. nvim LAUNCH SELF-LOCATION — confirm the bundled binary finds its
   runtime/parsers from inside the bundle (classic bundling failure).

## Verification (coder MUST run + report verbatim)

- `bazel build //crates/wolf-macos:Wolf`
- Bundle inspect: `Contents/Resources/nvim/bin/nvim` exists + executable;
  `lib/nvim/parser/markdown.so` + `share/nvim/runtime` present;
  `Contents/Resources/init.lua` present.
- Exec bundled binary: `…/nvim/bin/nvim --version` → v0.12.3, locates runtime.
- `codesign --verify --deep --strict --verbose=2` on the .app (report; if it
  fails, report exact error + chosen remedy).
- End-to-end WITHOUT `WOLF_NVIM_BIN`: confirm the app starts the bundled nvim (at
  minimum a non-GUI smoke resolving the bundled path the way AppDelegate does and
  starting wolf-core/nvim against it; GUI-window assertion not headlessly
  testable — be honest).
- Prior tests stay green: `//crates/wolf-ffi:frame_pack`,
  `//crates/wolf-ffi:bridge_smoke` (loud skip ok), `//crates/wolf-macos:WolfKeyMapTests`.
  wolf-core UNCHANGED (no new dep; GUI denylist clean).

## Testing policy

Prefer real over mocks (actually exec the bundled nvim). Be honest about
GUI-window untestability. If feasible add a Bazel test (swift_test / sh_test)
exec-ing the bundled `nvim --version` asserting "0.12.3", and/or asserting the
bundle contains the expected resource paths.

## Deliverables

- Bazel: module extension + http_archive pins (+ build_file overlay), MODULE.bazel
  additions, resource-bundling wiring in crates/wolf-macos/BUILD.bazel, updated
  MODULE.bazel.lock, any //bazel/ helper dir (README.md per repo convention).
- App: AppDelegate.swift (setenv WOLF_NVIM_BIN bundled path, overwrite=0; keep dev
  overrides) + bundle init.lua. Minimal, surgical.
- Docs: architecture.md Decision 8 (macOS bundling REALIZED, nvim 0.12.3; release
  carries markdown/markdown_inline parsers) + update wolf-macos "no bundled
  Neovim yet" notes; build.md (download pin, how to bump version + recompute
  sha256, app no longer needs WOLF_NVIM_BIN); crates/wolf-macos/README.md
  (bundled nvim + init.lua; double-click works; dev override honored); README.md
  if relevant. prettier --write all touched Markdown.

## Out of scope

Full wrapNeovim plugin baking beyond the release (marksman, formatter) — deferred;
notarization / Developer ID signing — dev ad-hoc fine; Linux/gnome bundling —
unchanged.

## Orchestrator decisions on architect's flagged items

- **APPROVED: add `aspect_bazel_lib` as a new `bazel_dep`** (for
  `copy_to_directory` → a structure-preserving, signable TreeArtifact). It's a
  build-time ruleset, NOT a wolf-core dep (GUI-free hard rule unaffected), and
  consistent with the already-approved all-Bazel toolchain. Coder pins the
  highest BCR release declaring Bazel-8 compat (architect suggested 2.9.4 —
  confirm on BCR).

## Architect's plan (APPROVED)

### Approach (decisive picks)
- **Download:** bzlmod module extension `//bazel/nvim/extensions.bzl` wrapping
  `http_archive` (from `@bazel_tools//tools/build_defs/repo:http.bzl`) with a
  `build_file_content` overlay, two repos `nvim_macos_arm64`/`nvim_macos_x86_64`
  (exact verified pins + strip_prefix). MODULE.bazel.lock regenerated+committed.
- **Structure preservation:** assemble each arch tree into a TreeArtifact dir
  named `nvim/` via `@aspect_bazel_lib//lib:copy_to_directory` INSIDE the overlay
  (repo root already = arch contents, so `out="nvim"` → `nvim/bin/nvim`,
  `nvim/lib/nvim/parser/*.so`, `nvim/share/nvim/runtime/**`). Feed to
  `macos_application(resources=...)` → lands at `Contents/Resources/nvim/`.
  Fallback if `@aspect_bazel_lib` won't resolve in the overlay: move
  `copy_to_directory` into `//bazel/nvim` with `include_external_repositories` +
  `root_paths`/`replace_prefixes`.
- **Arch select:** `//bazel/nvim:nvim_tree` alias `select()`ing arm64/x86_64.
- **Signing (HIGHEST RISK):** rely on rules_apple ad-hoc bundle sealing; coder
  MUST verify `codesign -dvv` on the extracted `bin/nvim` + a `.so` and verify a
  real launch. FALLBACK if invalid/unlaunchable: ad-hoc re-sign the tree
  (`codesign --force --sign - …` on `bin/nvim` + `lib/nvim/parser/*.so`) BEFORE
  the app is sealed (never post-build). Secondary: dev entitlement
  `com.apple.security.cs.disable-library-validation`, or relocate to
  `Contents/Helpers`. Do NOT ship an unlaunchable app — escalate.

### Files to CREATE
- `bazel/nvim/extensions.bzl` — module extension + 2 `http_archive` (verified
  pins) + overlay assembling the `nvim/` TreeArtifact (`copy_to_directory`,
  preserve exec bits); keep a `filegroup all` for the version test.
- `bazel/nvim/BUILD.bazel` — `alias nvim_tree` (`select` on `@platforms//cpu`),
  `sh_test nvim_version_test`.
- `bazel/nvim/nvim_version_test.sh` — execs assembled `nvim/bin/nvim --version`
  (assert `NVIM v0.12.3`), asserts executable, asserts `$VIMRUNTIME` resolves
  INSIDE the tree (self-location). Sandbox-safe (`nvim_tree` as `data`).
- `bazel/nvim/README.md` — extension/pins/overlay/select/test + how to bump the
  pin (new URLs → recompute both sha256 → regen MODULE.bazel.lock). prettier.
- `runtime/nvim/BUILD.bazel` — `exports_files(["init.lua"])` (flat → bundles as
  `Resources/init.lua`, which `Bundle.main.path(forResource:"init",…)` resolves).
- `crates/wolf-macos/bundle_layout_test.sh` — over the built `.app`: assert
  `Contents/Resources/nvim/bin/nvim` (executable), `…/nvim/lib/nvim/parser/
  markdown.so`, `…/nvim/share/nvim/runtime` (dir), `Contents/Resources/init.lua`.

### Files to MODIFY
- `MODULE.bazel` — `bazel_dep(aspect_bazel_lib, 2.9.4)` (APPROVED) +
  `use_extension("//bazel/nvim:extensions.bzl","nvim")` + `use_repo(nvim,
  "nvim_macos_arm64","nvim_macos_x86_64")`.
- `MODULE.bazel.lock` — regenerate + commit.
- `crates/wolf-macos/BUILD.bazel` — `macos_application` gains `resources =
  ["//bazel/nvim:nvim_tree", "//runtime/nvim:init.lua"]`; add `sh_test
  bundle_layout_test` (data=[":Wolf"]). Verify rules_apple preserves the `nvim/`
  prefix by unzipping; fallback `structured_resources`/`apple_resource_group`.
- `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift` — add `nvimBinPath()`
  (`Bundle.main.resourcePath + "/nvim/bin/nvim"` if `isExecutableFile`); in
  `startCore()` BEFORE `wolf_ffi_start`, `if let bin = nvimBinPath() {
  setenv("WOLF_NVIM_BIN", bin, 0) }` (overwrite=0 → dev export wins). `init.lua`
  resolution UNCHANGED. Optional: soften `presentStartupFailureAndQuit` text.
- Docs: architecture.md Decision 8 (macOS bundling REALIZED, nvim 0.12.3, release
  carries markdown/markdown_inline parsers; remove "no bundled Neovim yet");
  build.md (pin + bump/sha256 steps + app no longer needs WOLF_NVIM_BIN + new
  test targets); crates/wolf-macos/README.md (bundled nvim+init.lua, double-click
  works, dev override honored); README.md only if it references macOS nvim
  location. prettier --write all touched Markdown.

### CONFIRMED unchanged
- `crates/wolf-core/src/nvim.rs` already reads `WOLF_NVIM_BIN` via
  `std::env::var` and owns NVIM_APPNAME/XDG isolation in `nvim::spawn` — the app
  only points WOLF_NVIM_BIN at the binary; no core change, no CARGO_BAZEL_REPIN.

### Tests (real over mocks)
1. `//bazel/nvim:nvim_version_test` — runs the pinned binary (version +
   exec-bit + self-location). 2. `//crates/wolf-macos:bundle_layout_test` —
   built `.app` contains the expected resource paths + exec bit. 3. Regression:
   `//crates/wolf-ffi:frame_pack`, `//crates/wolf-ffi:bridge_smoke` (loud-skip
   ok), `//crates/wolf-macos:WolfKeyMapTests`; GUI denylist clean; wolf-core
   unchanged. NOT headlessly testable (state honestly): GUI window appearing,
   Finder double-click, `codesign --verify` — coder runs/reports these manually.

### Implementation order
1. (approval done) → 2. `runtime/nvim/BUILD.bazel` → 3. `bazel/nvim/*` →
4. MODULE.bazel + regen lock → 5. `bazel test //bazel/nvim:nvim_version_test`
(fix before touching app) → 6. wire `resources`, build Wolf → 7.
`bundle_layout_test` + unzip eyeball → 8. codesign + real launch (apply re-sign
fallback if needed) → 9. AppDelegate.swift, rebuild, Finder-launch w/ empty env
→ 10. docs + prettier → 11. full regression + denylist.

## Progress log

- (orchestrator) New increment. Verified the v0.12.3 release + both arch
  checksums BEFORE planning. Architect produced a decisive plan; orchestrator
  APPROVED the one flagged item (aspect_bazel_lib new bazel_dep). Architect's
  first run lost its output to a socket error (read-only, no partial state) and
  was re-run cleanly. Next: test-writer (the two sh_test scripts).

## Tests written (test-writer)

`bazel/nvim/nvim_version_test.sh` (exec-bit + `NVIM v0.12.3` + VIMRUNTIME-inside-
tree + markdown.so) and `crates/wolf-macos/bundle_layout_test.sh` (4 bundle
paths). Both POSIX sh, `set -eu`, robust runfiles resolver (TEST_SRCDIR/
RUNFILES_DIR/$0.runfiles + find fallback). Coder MUST finalize: the exact
TreeArtifact + `:Wolf` runfiles paths once targets exist (pin direct path, keep
find as safety net); confirm whether rules_apple 4.5.3 delivers `:Wolf` as a
`Wolf.app` dir vs an archive; verify the nvim-0.12 VIMRUNTIME incantation prints
(fallback `vim.fn.expand('$VIMRUNTIME')`). Coder wires both as `sh_test` with the
right `data` deps and marks them executable.

## Implementation + verification result — BUNDLING GREEN

Prior coder wrote the full implementation (recovered from a socket-dropped run;
all files on disk + wired). Verification coder (writing to `.claude/
verify-results.md`) confirmed:

| Step | Result |
|------|--------|
| nvim_version_test (pinned 0.12.3, exec bit, VIMRUNTIME inside tree) | PASS |
| build //crates/wolf-macos:Wolf (delivered as Wolf.zip) | PASS |
| structure preservation: Contents/Resources/nvim/{bin/nvim(exec),lib/nvim/parser/markdown.so,share/nvim/runtime}, + init.lua | PASS |
| bundle_layout_test | PASS |
| signing: nvim Mach-O adhoc/linker-signed; codesign --verify --deep --strict OK | PASS |
| regression frame_pack + WolfKeyMapTests | PASS |
| boundary: GUI denylist clean; wolf-core untouched (no REPIN) | PASS |

No fixes needed (test runfiles resolvers + VIMRUNTIME incantation worked as
written; no signing fallback required — `replace_prefixes` `nvim/` remap held,
exec bits survived). Implementation matches the plan.

## FINDING: bridge_smoke hangs on the macOS Bazel host (pre-existing, out of scope)

bridge_smoke now RUNS against a real nvim (it skipped before, with no nvim) but
HANGS/times out — AND it also hangs against the system nvim (same 0.12.3), both
under `bazel test` sandboxing. So this is a PRE-EXISTING, host-wide issue in the
bridge's macOS read/wake pump OR the test's blocking poll loop OR the test
sandbox's handling of the nvim subprocess — NOT caused by the bundling. Net: the
LIVE nvim→FFI→read_frame round-trip has never executed green on the macOS Bazel
host (it always skipped). The bundled binary itself works (direct headless
edit→write→quit proxy succeeded; nvim_version_test proves run + self-location).

ORCHESTRATOR DECISIONS:
- The bundling increment is COMPLETE + green; do NOT scope-creep it to fix the
  bridge hang. Proceed to review + docs.
- Track TWO follow-ups for the USER (separate investigation, surfaced in the
  completion report; not done now): (1) investigate the bridge_smoke hang —
  is it the FFI wake/read model on macOS, the test's poll loop, or a Bazel
  sandbox/subprocess artifact? (run outside the sandbox / with a hard timeout +
  tracing). This is the real "does the live editor round-trip work on macOS"
  question. (2) optionally add a Bazel-wired headless `nvim --headless`
  edit→write→quit smoke for the bundled binary (the proxy the coder ran
  manually) — nvim_version_test already covers run+self-location, so this is
  nice-to-have, not required for THIS increment.

## Status

- [x] Architect plan
- [x] Tests written
- [x] Implementation + verification (bundling green; bridge_smoke hang tracked)
- [ ] Tests written
- [ ] Implementation + tests passing
- [ ] Review
- [ ] Docs
