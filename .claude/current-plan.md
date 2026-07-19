# Plan: `//crates/wolf-macos:WolfRelease` + document the optimized build

## Problem statement

**`build.md` documents only `bazel build //crates/wolf-macos:Wolf`, which compiles Swift at
`-Onone` and Rust unoptimized.** There is no optimized build documented anywhere in the
repo. This is the single largest performance defect found in the whole typing-latency
investigation — larger than any code fix we made — and per `CLAUDE.md` ("if the
documentation and the implementation differ, that's a bug") a doc that presents a debug
build as *the* build is itself the bug.

## Evidence (measured, this machine)

Same capture protocol, steady-state window #5. `test-5-with-preview` (default build) vs
`test-7-with-opt-param` (`-c opt`), both in the repo root:

| bucket        | default (`-Onone`) | `-c opt` (`-O`) | change |
| ------------- | ------------------ | --------------- | ------ |
| `ctrl.wake`   | 4.85 ms            | **0.51 ms**     | -89%   |
| `prev.render` | 25.63 ms           | **6.17 ms**     | -76%   |
| `draw`        | 21.87 ms           | 14.64 ms        | -33%   |
| `pane.glyphs` | 18.96 ms           | 14.00 ms        | -26%   |
| `pane.read`   | 0.67 ms            | 0.07 ms         | -90%   |
| `prev.html`   | 2.23 ms            | 2.31 ms         | ~0%    |

- **Main-thread busy 73% -> 34%. Per keystroke ~70 ms -> ~36 ms.** User reports typing
  "feels very fluid" on the optimized build.
- `prev.render` -76% is the **Rust** side (`pulldown-cmark`) — Rust was unoptimized too,
  not just Swift.
- `prev.html` unchanged is a control: WebKit is a system framework, always optimized, so it
  had nothing to gain. CoreText is likewise why `pane.glyphs` improved least.

Verified with `bazel aquery`: default (fastbuild) passes `-Onone`; `-c opt` passes `-O`/`-Os`.

## Scope

### 1. New target `//crates/wolf-macos:WolfRelease`

A named target producing the optimized app WITHOUT requiring `-c opt` on the command line,
so the shippable build is a first-class thing you can ask for by name.

`macos_application` does not change compilation mode on its own, and an `alias` cannot —
compilation mode is a configuration, not an attribute. This needs a **Starlark
outgoing-edge transition** setting `//command_line_option:compilation_mode` to `"opt"`,
applied to a thin rule wrapping the existing `:Wolf` target.

The architect must determine and specify:

- the exact transition + wrapper rule (a `transition(implementation, inputs, outputs)` with
  `outputs = ["//command_line_option:compilation_mode"]`, plus a rule with `cfg = <transition>`
  on its dep, and any allowlist handling the Bazel version requires)
- whether it belongs inline in `crates/wolf-macos/BUILD.bazel` or in a new `.bzl` (prefer a
  `.bzl` — BUILD files should not carry rule definitions; check repo convention first)
- how the output is surfaced. `macos_application` yields `Wolf.zip` plus the unzipped bundle
  under `Wolf_archive-root/`. **The wrapper must forward a usable `DefaultInfo` so the
  bundle is actually reachable at a predictable path** — the part most likely to be fiddly,
  and it must be verified by building and running the result, not assumed.
- the resulting on-disk path, so `build.md` can document it exactly. NOTE the `bazel-bin`
  symlink retargets to whichever config was built last, so a `bazel-bin/...` path is
  FRAGILE when both configs exist. Prefer `bazel cquery --output=files` or an explicit
  `bazel-out/darwin_arm64-opt-.../` path, and say which is robust.

### 2. `build.md` corrections

- Document `//crates/wolf-macos:WolfRelease` as the way to build the app you actually run
  and install, with the exact on-disk path to the bundle.
- State plainly that the plain `:Wolf` target is a **development** build at `-Onone` and is
  substantially slower, with the measured numbers above as justification.
- Document the test caveat: **`bazel test -c opt //crates/wolf-macos:WolfKeyMapTests` FAILS
  to build** — `@testable import` requires `-enable-testing`, which rules_swift only sets in
  non-opt compilation modes (error: `module 'WolfKeyMap' was not compiled for testing`).
  This is expected and standard: **test in the default config, ship the optimized build.**
  Do not present it as a defect to fix.
- Update the frame-time instrumentation section: its invocation should use the release
  bundle path, and it should note that measuring the default build measures `-Onone`.

### 3. Other docs

Check `crates/wolf-macos/README.md` and `architecture.md` for build-command claims that are
now incomplete. Use judgment on `architecture.md` — a build-configuration fact may or may
not rise to architecture.

## Constraints

- Do NOT change any application code. Build target + documentation only.
- Do not alter the existing `:Wolf` target's behavior — other targets/tests depend on it,
  and the fastbuild path must stay the fast-iteration build.
- The new target must actually build AND the resulting app must launch. Verify by running
  it, not by inspecting the graph.
- `prettier --write` every Markdown file touched; no hard line breaks within a paragraph.

## Explicitly NOT in this pass

- The glyph work (`pane.glyphs`, now 97% of `draw.pane` at ~4.4 us/cell under `-O`). The
  ablation probe for it is preserved on branch `probe/glyph-cost-diagnostic` (commit
  `0295ab6`, THROWAWAY, must never merge).
- Moving `prev.render` off the main thread — now only 6.17 ms under `-O`, much less
  valuable than when it was 25.6 ms.
- Wake amplification — `ctrl.wake` is now 0.51 ms, so ~2.9 wakes/key is ~1.5 ms. Noise.
- Dirty-rect tracking, `INPUT_COALESCE_DELAY`, `isOpaque`.
- Any Rust source change.

## Success criteria

`bazel build //crates/wolf-macos:WolfRelease` produces an optimized bundle that launches and
behaves identically to the `-c opt` build already measured in `test-7-with-opt-param`.
`build.md` tells a reader unambiguously which target to build for real use and which for
development, and why it matters.

## Architect's plan (APPROVED — implement exactly this)

### Versions / allowlist — SETTLED

`.bazelversion` = Bazel **8.7.0**; rules_swift 3.6.1, rules_apple 4.5.3, apple_support
2.6.1, rules_rust 0.71.2. **`_allowlist_function_transition` is NOT required on Bazel 8**
(requirement removed upstream, bazelbuild/bazel#19233). **Do not add it** — it pins a
dependency on a legacy allowlist a future Bazel drops. Only if analysis errors with an
allowlist message (it should not) add it, commented as a version-specific workaround.

### Where the Starlark lives — `crates/wolf-macos/release.bzl`

Repo convention: every `.bzl` is co-located with the package that consumes it
(`bazel/<topic>/extensions.bzl`; `tools/lint/clippy_fix.bzl` is the direct precedent for a
custom RULE — a `.bzl` in the consuming package, module docstring, `load()`ed from that
package's BUILD). Follow it exactly. No rule definitions inline in BUILD.bazel.

### Output surfacing — THE FIDDLY PART

rules_apple 4.5.3 `apple/internal/outputs.bzl`: `_root_path_from_archive` derives the
archive-root path by STRING MANIPULATION (`paths.replace_extension(archive.path,
"_archive-root")`), and `_archive()` only calls `declare_directory` when experimental
tree-artifact outputs are enabled. This repo does NOT enable that
(`--define=apple.experimental.tree_artifact_outputs` appears nowhere in `.bazelrc`).

**Therefore `Wolf_archive-root/Wolf.app` is NOT a Bazel artifact.** It is an UNDECLARED
side-effect directory the bundling/signing action leaves behind. Consequences:

- The wrapper **cannot** forward the `.app` in `DefaultInfo`. There is nothing to forward.
  Any plan saying "forward the bundle TreeArtifact" is wrong for this configuration.
- Forward the dep's `DefaultInfo.files` (the opt-configured `Wolf.zip`). The `.app` sits on
  disk as its sibling at `<zip path minus .zip>_archive-root/Wolf.app`, in the OPT output dir.
- Return `DefaultInfo(files = dep[DefaultInfo].files, runfiles = dep[DefaultInfo].default_runfiles)`.
- **Do NOT** set `executable =`. **Do NOT** forward `AppleBundleInfo` /
  `MacosApplicationBundleInfo` / `AppleExecutableBinaryInfo` — nothing consumes them
  (`bundle_layout_test` and `xcodeproj` depend on `:Wolf` directly), and forwarding Apple
  providers from a non-Apple rule invites rules_apple to treat `WolfRelease` as an
  embeddable bundle: added risk, zero benefit.

Residual fragility to DOCUMENT not fix: because the dir is undeclared, Bazel does not track
it; on an action-cache hit only `Wolf.zip` is restored. No remote cache is configured so in
practice it is always materialized. Recovery is one line: unzip / `ditto -x -k` the zip.

FALLBACK, only if verification shows the dir is not reliably present under opt: a second
action expanding the zip into a DECLARED `declare_directory("Wolf.app")` via `ditto -x -k`
(preserves exec bits/symlinks/xattrs where plain `unzip` may not). Costs a full extra copy
of a large bundle every rebuild. **Do not implement speculatively.**

### THE PATH PROBLEM — `bazel-bin` is WRONG for this target

`bazel-bin` symlinks the bin dir of the **top-level target configuration**. Building
`//crates/wolf-macos:WolfRelease` analyzes the top-level target in the **default
(fastbuild)** config — the transition applies to its dependency edge, i.e. BELOW it. So
`bazel-bin/crates/wolf-macos/Wolf_archive-root/Wolf.app` is either ABSENT or — far worse —
**a stale fastbuild `-Onone` bundle from an earlier `:Wolf` build.** That is silently
measuring the wrong binary: exactly the failure mode that already bit this project.
**`build.md` must NEVER print a `bazel-bin/...` path for `WolfRelease`.**

**Do not hardcode the opt output-dir name.** Under Bazel 7+'s default
`diff_against_baseline` naming, a Starlark transition may append an `-ST-<hash>` fragment;
whether it is `darwin_arm64-opt` or `darwin_arm64-opt-ST-<hash>` must be OBSERVED, not
predicted. Record the observed value during verification but document the COMMAND, since
the hash is not a stable contract.

Documented recipe for `build.md`:

```sh
bazel build //crates/wolf-macos:WolfRelease
ZIP=$(bazel cquery --output=files //crates/wolf-macos:WolfRelease 2>/dev/null)
APP="${ZIP%.zip}_archive-root/Wolf.app"
```

`cquery --output=files` is reliable **because the wrapper forwards the dep's
`DefaultInfo.files`** — configuration-aware, unlike the `bazel-bin` symlink. `cquery` prints
progress to stderr (hence `2>/dev/null`); verify the captured value is a single clean path.
The `bazel build` "Target up-to-date:" line prints the same path as an eyeball cross-check.

### Runnable? — NO. Build-only, deliberately.

Technically doable (symlink a launcher, `executable = True`, forward runfiles), but it lands
on the path `build.md:158` already warns about: `bazel run //crates/wolf-macos:Wolf` has
less predictable stderr plumbing through the rules_apple runner. **Decisive here, because
the entire point of this bundle is that it is the binary people run with `WOLF_FRAME_STATS=1`
and read stderr from.** A run target whose stderr is unreliable defeats its own purpose. It
would also stage a second copy of a large bundle in runfiles — another place "which .app am
I running" can go wrong. Record this decision + rationale as a comment in `release.bzl` so
it is not re-litigated.

### `release.bzl` contents

Module docstring (style of `tools/lint/clippy_fix.bzl`): why a transition is needed at all
(compilation mode is a CONFIGURATION, not an attribute — `alias` cannot do this); that the
wrapper forwards only `DefaultInfo`; that the `.app` is an UNDECLARED sibling of the zip;
that `bazel-bin` is NOT the right place to look; that the rule is deliberately non-executable.

1. `_opt_transition_impl(settings, attr)` → `{"//command_line_option:compilation_mode": "opt"}`
2. `_opt_transition = transition(implementation = ..., inputs = [], outputs = ["//command_line_option:compilation_mode"])`
   — `inputs = []` because the new value is unconditional. NO allowlist attr.
3. `_opt_bundle_impl(ctx)` — single `DefaultInfo` forwarding `files` + `default_runfiles`.
   **CODER NOTE:** for a non-split (1:1) transition `ctx.attr.<dep>` is a plain `Target`;
   Bazel makes it a LIST only for 1:2+ split transitions. Write the plain-`Target` form; if
   analysis fails with a list error, index `[0]` and comment which applies. Do not guess —
   the error is immediate and unambiguous.
4. `opt_bundle = rule(...)`: one attr `bundle` =
   `attr.label(mandatory = True, cfg = _opt_transition, providers = [DefaultInfo])`;
   NO `executable = True`; NO `toolchains`. Keep the rule name GENERIC (`opt_bundle`), not
   Wolf-specific, so it could move to `bazel/` later without a rename.

### `crates/wolf-macos/BUILD.bazel`

`load("//crates/wolf-macos:release.bzl", "opt_bundle")` with the existing loads (17-20).
Extend the file-header target-list comment (1-16) with a `WolfRelease` entry. Add the target
right AFTER the `macos_application(name = "Wolf", ...)` block (after ~176), before
`bundle_layout_test`: `name = "WolfRelease"`, `bundle = ":Wolf"`,
`visibility = ["//visibility:public"]` (mirrors `:Wolf`), and **`tags = ["manual"]`**.

**Why `manual` (deliberate, not boilerplate):** without it, `bazel build //...` sweeps
`WolfRelease` in and rebuilds the ENTIRE Rust + Swift + treesitter graph a second time at
`-c opt` on every wildcard build — a large, silent, recurring cost on a command people run
casually. The repo already uses `manual` for exactly this on every `//tools/lint` target
(`build.md:295`), so it is idiomatic here. `WolfRelease` is not a test, so the default
test-suite count is unaffected. Explicit `bazel build //crates/wolf-macos:WolfRelease` still
works normally. Add a comment covering: what it is, that `:Wolf` is unchanged and remains
the fast-iteration dev build, why `manual`, and that the path must come from `cquery` and
NEVER from `bazel-bin`.

### `build.md` restructure

**§"Building the .app" (~95-103)** → two labeled subsections, RELEASE FIRST (it is the
answer to "how do I build Wolf"):

- **"Building the .app for real use (optimized)"** — leads with the `WolfRelease` build.
  Same bundle as `:Wolf` but the whole graph (Swift AND Rust) at `-c opt`, without `-c opt`
  on the command line, via an outgoing-edge transition in `crates/wolf-macos/release.bzl`.
  Give the `cquery` recipe verbatim. **State in its own sentence that `bazel-bin/...` is not
  a valid path for this target**, and why (top-level config is fastbuild → missing or stale
  `-Onone`). Note the `manual` tag and that the first build is a full second-config rebuild.
- **"Building the .app for development"** — keep `:Wolf` and everything at 100-103 (icon,
  bundled nvim, signing, min OS), but state up front this is a DEVELOPMENT build at
  `-Onone` with Rust unoptimized, that it is the fast-iteration build and what every test
  target builds against, and that it is SUBSTANTIALLY slower at runtime — reproduce the
  measured table. **Include the two controls, they are what make it credible rather than
  anecdotal:** `prev.html` flat because WebKit is a system framework and always optimized;
  `pane.glyphs` improves least because CoreText likewise. Note `prev.render` -76% is the
  RUST side (`pulldown-cmark`) — Rust was unoptimized too, not just Swift.

**§"Test targets" (~239-265)** — add, worded as expected-and-standard, NOT as a defect and
NOT as a TODO:

> Tests run in the **default** configuration, not `-c opt`.
> `bazel test -c opt //crates/wolf-macos:WolfKeyMapTests` fails to build: `@testable import`
> requires `-enable-testing`, which rules_swift only passes in non-opt compilation modes
> (`module 'WolfKeyMap' was not compiled for testing`). This is the standard, intended
> arrangement across the Swift ecosystem — **test in the default config, ship the optimized
> build** — not something to work around.

**§"Frame-time instrumentation (macOS)" (147-168)** — (1) replace the recommended
invocation (151-154) and the log-to-file variant (162-166) with the release-bundle form via
the `cquery`-derived `$APP`. (2) Add a prominent note: **measuring the default `:Wolf` build
measures `-Onone`** — those numbers are not what users experience and are not comparable to
anything captured under `-c opt`; any capture meant to inform a decision must come from
`WolfRelease`. (3) Rewrite the `Wolf_archive-root/` explanation at 156 to apply to BOTH
targets: an unzipped sibling of `Wolf.zip` in the same output dir as THAT target's zip, not
a declared Bazel output; no `Wolf.app` at either config's package root; unzip if missing.
Keep the 158 parenthetical about `bazel run` stderr and extend it: this is also why
`WolfRelease` is deliberately build-only.

### `crates/wolf-macos/README.md`

§"Building" (106-113): add the `WolfRelease` line FIRST in the code block
(`# the OPTIMIZED (-c opt) .app — build this for real use`), annotate `:Wolf` as the
development / `-Onone` build, and point at `build.md` for the measured penalty and the
`cquery` recipe (do NOT duplicate the numbers — one source of truth). **CLAUDE.md
compliance:** this directory's README must describe its components, and `release.bzl` is a
new component — add a short paragraph on the `opt_bundle` rule and its transition (what it
does, forwards only `DefaultInfo`, non-executable by design, `.app` is an undeclared sibling).

### `architecture.md`

ONE sentence, minimal. §"Build" (~244) currently names only
`bazel build //crates/wolf-macos:Wolf`; name both targets (`:Wolf` development,
`:WolfRelease` optimized build-for-real-use) pointing at `build.md`. Judgment stated plainly:
which compilation mode a target uses is a build-configuration detail that does not belong in
architecture — BUT architecture.md currently names a specific build command, and leaving it
naming only the debug target reproduces the original documentation bug in a second file. Fix
the naming; do NOT import the performance table or the transition mechanics.

### NOT modified

`bundle_layout_test.sh` / its `sh_test` — unchanged, still `data = [":Wolf"]`. A release
variant would force a full opt build into the default test sweep for zero added coverage:
bundle LAYOUT is configuration-independent (same `resources` attr, same rules_apple partials).
Verify the layout once manually against the opt bundle instead. `//tools/lint/**` — no change;
`swift_format_srcs` is `.swift` only; `rustfmt.fix` evaluates `//...` internally so it will
LOAD `release.bzl` (must be load-clean) but will not BUILD `WolfRelease`, and `manual` keeps
it out regardless. `.bazelrc` — **do NOT add a `--config=release`**; the whole point is a
named target rather than a flag to remember. Nothing under `crates/wolf-core`,
`crates/wolf-ffi`, or `crates/wolf-macos/Sources` — no application code changes.

### Verification protocol (verify by BUILDING AND LAUNCHING, not graph inspection)

**No new Bazel test target.** Any `sh_test` with `data = [":WolfRelease"]` forces a full opt
build; tagged `manual` it never runs, untagged it doubles every sweep. Neither is worth it.

1. `bazel build //crates/wolf-macos:WolfRelease` completes. Resolve the plain-`Target`-vs-list
   question here if the first form errors.
2. **The load-bearing check:** `bazel aquery --output=text 'mnemonic("SwiftCompile",
   //crates/wolf-macos:WolfRelease)'` shows `-O` (NOT `-Onone`); Rust actions show an
   optimizing `-C opt-level`. Cross-check the same aquery on `//crates/wolf-macos:Wolf` still
   shows `-Onone`. Necessary precondition, NOT a substitute for 4-6.
3. Path recipe resolves: single clean path, `$APP` exists and is a directory, config component
   is an `-opt-` one. **Record the literal observed directory name** (including any
   `-ST-<hash>`). Independently confirm the NEGATIVE:
   `bazel-bin/crates/wolf-macos/Wolf_archive-root/Wolf.app` is absent or demonstrably the
   fastbuild bundle — `build.md` will make this claim, so it must be observed.
4. `codesign --verify --deep --strict` passes on the opt `.app`. Spot-check the layout
   `bundle_layout_test` asserts: `Contents/Resources/nvim/bin/nvim` executable, `init.lua`,
   `fonts/`, `parsers/parser/go.so`.
5. Launches and works: run `"$APP/Contents/MacOS/Wolf"` directly — window appears, typing
   reaches nvim, sidebar and preview work, quits cleanly. Record honestly as a manual smoke.
6. **Genuinely optimized (the success criterion):** `WOLF_FRAME_STATS=1` steady-state capture
   under the `test-7-with-opt-param` protocol; buckets land near the `-c opt` column
   (`ctrl.wake` ~0.5 ms, `prev.render` ~6 ms, `draw` ~15 ms), NOT the `-Onone` column. This
   closes the loop — proves `WolfRelease` reproduces the measured `-c opt` build.
7. No regression: `bazel build //crates/wolf-macos:Wolf` still succeeds, fastbuild outputs
   untouched, `WolfKeyMapTests` + `bundle_layout_test` still pass.
8. Wildcard hygiene: `bazel build //crates/wolf-macos:all` does NOT pull in `WolfRelease`.

Edge cases: building `WolfRelease` and `Wolf` in the SAME invocation (both configs coexist —
no output collision, no action conflict); building `WolfRelease` WITH `-c opt` on the command
line (transition is a no-op — still works, no third output dir); after `bazel clean` + rebuild
the archive-root dir is present (if ever not, that triggers the fallback — escalate, do not
paper over).

### Risks

**HIGH — first-build cost (certain).** The transition rebuilds the ENTIRE graph in a second
config: all crate_universe Rust deps (tokio, rusqlite with bundled SQLite through `cc`,
pulldown-cmark, pulldown-latex), `wolf-core`, `wolf-ffi`, both Swift modules, 16 host-compiled
treesitter parsers, actool, code signing. External repo-rule outputs (downloaded nvim/font/
plugin tarballs) are NOT reconfigured (repo rules are config-independent) but the
`copy_to_directory` targets assembling them ARE build targets and re-run at opt, re-copying
the ~100 MB nvim tree. Expect roughly one additional clean app build (several minutes) and
roughly double on-disk output footprint. Incremental rebuilds within either config unaffected.
`manual` is what keeps this opt-in.

**HIGH — the stale-`bazel-bin` trap** (already bit this project once). Mitigation is
documentation discipline: give the `cquery` recipe AND state the negative claim explicitly.
If the docs print a `bazel-bin` path, someone WILL profile a stale `-Onone` bundle and reach
a wrong conclusion — precisely the class of bug this change exists to eliminate.

**MEDIUM — undeclared archive-root dir.** Pre-existing (already applies to `:Wolf`), not
introduced here; mitigated by the one-line unzip note.

**LOW —** `:Wolf` untouched (separate output dir, fastbuild cache preserved;
`bundle_layout_test`/`xcodeproj` depend on it directly). rules_apple applies its own
platform/cpu transitions BELOW `macos_application` while ours applies ABOVE — they compose
normally; step 2 catches it immediately if wrong. `rules_xcodeproj` lists only
`top_level_target(":Wolf")` and has historically been sensitive to custom transitions — a
further reason NOT to add `WolfRelease` to `top_level_targets`. `//tools/lint` `rustfmt.fix`
loads `release.bzl` so it must be parse-clean (caught at step 1). Opt-only compile failures
are essentially retired — this exact configuration already built successfully via `-c opt`,
which is where the measurements came from.

### Dependencies: NONE

No new `bazel_dep`, no module extension, no `MODULE.bazel` change, therefore **no
`MODULE.bazel.lock` regeneration and no `CARGO_BAZEL_REPIN`**. `transition()` and
`rule(cfg = ...)` are core Bazel Starlark. **If the implementation touches `MODULE.bazel` or
either lockfile, something has gone wrong — stop and reconsider.**

### Lint

buildifier is not wired into this repo (`//tools/lint` = clippy, rustfmt, swift-format only),
so no Starlark-lint suppressions needed — but write `release.bzl` buildifier-clean anyway
(module docstring first, `def`s before `rule()`, underscore-prefixed privates, canonical
`rule()` arg order), matching `clippy_fix.bzl`. **No `# buildifier: disable=` directives
anticipated** — the one plausible candidate, unused params on `_opt_transition_impl(settings,
attr)`, is a Bazel-mandated signature and buildifier does not flag unused function params.
Do not add one preemptively. `.bzl` is not prettier-covered; only the Markdown is.

## Status

- [x] architect plan
- [ ] implementation
- [ ] verify: build + launch + confirm optimized
- [ ] docs
- [ ] review
