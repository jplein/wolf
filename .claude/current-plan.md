# Plan: Make `//crates/wolf-macos:WolfRelease` runnable

## Problem statement

`bazel run //crates/wolf-macos:WolfRelease` fails:

```
ERROR: Cannot run target //crates/wolf-macos:WolfRelease: Not executable
```

The target was deliberately made build-only in the previous pass (commit `8ba033c`). The
user has asked for it to be runnable. That decision is reversed — implement it.

## Background: why it was build-only, and what still holds

The original rationale (recorded in `release.bzl`'s module docstring, `build.md`, and
`crates/wolf-macos/README.md`): `build.md` already warned that
`bazel run //crates/wolf-macos:Wolf` has less predictable stderr plumbing through the
rules_apple runner, and the optimized bundle is the binary people run with
`WOLF_FRAME_STATS=1` while reading stderr. A run target with unreliable stderr defeats that
purpose.

**That concern is still valid but it is not a reason to block `bazel run` — it is a reason
to keep BOTH paths documented:**

- `bazel run //crates/wolf-macos:WolfRelease` — the convenient way to launch the app.
- direct exec of `"$APP/Contents/MacOS/Wolf"` via the `cquery` recipe — the reliable way to
  take a frame-stats measurement, because it inherits the terminal's stdio and environment.

The docs must NOT present `bazel run` as the way to capture frame stats.

## Scope

### 1. Make the rule executable — `crates/wolf-macos/release.bzl`

Current state: `_opt_bundle_impl` returns a single
`DefaultInfo(files = ..., runfiles = dep[DefaultInfo].default_runfiles)`, no `executable`.
`ctx.attr.bundle` is a LIST even for this 1:1 transition on Bazel 8.7.0 (observed; the
existing code uses `[0]` with a comment recording the error text).

The architect must determine and specify:

- **How to forward an executable that another rule produced.** Bazel requires
  `DefaultInfo.executable` to be an artifact this rule generates, so the dep's runner
  cannot simply be handed through. The standard mechanism is
  `ctx.actions.symlink(output = ctx.actions.declare_file(ctx.label.name), target_file =
  <dep executable>, is_executable = True)`. Confirm this is right for a
  transitioned dep and specify it exactly.
- **Runfiles — the part most likely to break.** rules_apple's `macos_application` runner is
  a generated script that locates the bundle through its runfiles. A symlinked copy must
  still resolve them. Specify exactly what to merge (the dep's `default_runfiles`, plus
  whatever the symlink itself needs) and in what order. **This is the failure mode to guard
  against: a runner that launches but cannot find its `.app`, or launches a stale/other
  bundle.**
- Whether `executable = True` on the `rule()` is required in addition to setting
  `DefaultInfo.executable`, and whether the declared file name may collide with anything.
- Whether the transitioned dep's executable is the OPT one. **This is load-bearing: a
  runner symlinked from the wrong configuration would silently launch the `-Onone` app,
  which is precisely the class of bug this whole target exists to prevent.** Specify how to
  verify it empirically, not by reasoning.

### 2. Documentation corrections — every place the old rationale is recorded

All of these currently state the target is deliberately non-executable and must be corrected
or they become lies (CLAUDE.md: docs are source of truth):

- `crates/wolf-macos/release.bzl` — module docstring. It also contains a "do not
  re-litigate" note about non-executability; rewrite it to record the new decision AND
  preserve the stderr caveat as a reason to prefer direct exec for measurement.
- `build.md` — the release-build section and the frame-time instrumentation section. Add
  `bazel run` as the launch path; keep the `cquery` + direct-exec recipe as the measurement
  path, and say plainly why they differ.
- `crates/wolf-macos/README.md` — the `release.bzl` / `opt_bundle` paragraph.
- `crates/wolf-macos/BUILD.bazel` — the `WolfRelease` target comment, if it mentions
  non-executability.

## Constraints

- Do NOT change any application code (Swift/Rust). Build rule + docs only.
- Do NOT alter `:Wolf`, `bundle_layout_test`, or `xcodeproj`.
- Keep `tags = ["manual"]` — the rebuild-the-whole-graph-twice cost is unchanged and the
  reason for `manual` is unrelated to executability.
- Do not touch `MODULE.bazel` or either lockfile.
- **Verify by actually running `bazel run //crates/wolf-macos:WolfRelease`**, not by graph
  inspection. It must launch the OPTIMIZED app.

## Success criteria

`bazel run //crates/wolf-macos:WolfRelease` launches the optimized Wolf. The launched binary
is demonstrably the opt one (~8.26 MB, vs ~25.4 MB for fastbuild). `bazel build` on the
target still works, `:Wolf` is unaffected, and the docs describe both the run path and the
measurement path without implying `bazel run` is suitable for frame-stats capture.

## Architect's plan (APPROVED — implement exactly this)

### RESEARCH: what rules_apple 4.5.3 actually does (this corrects the original rationale)

`macos_application` DOES return a real executable: `apple/internal/macos_rules.bzl` declares
`outputs.executable(...)`, calls `run_support.register_macos_executable(...)`, and returns
`DefaultInfo(executable = executable, files = ..., runfiles = ctx.runfiles(files = [archive], ...))`.

The runner is `apple/internal/templates/macos.template.sh` with exactly two substitutions
(`run_support.bzl`): `%app_name%` -> `Wolf`, `%app_path%` -> `archive.short_path` =
`crates/wolf-macos/Wolf.zip`. What it does:

1. if `%app_path%` is a dir use it; else `mktemp -d "${TMPDIR:-/tmp}/bazel_temp.XXXXXX"`,
   `trap 'rm -rf "${TEMP_DIR}"' EXIT`, `unzip -qq '%app_path%' -d "${TEMP_DIR}"`
2. read `CFBundleExecutable` from `Info.plist` via `/usr/libexec/PlistBuddy`
3. invoke `"${APP_DIR}/Contents/MacOS/${BUNDLE_EXECUTABLE}" "$@"` — **NOT `open`**, and
   deliberately not `exec` (so the cleanup trap still fires)

**Three consequences the plan hinges on:**

- **`%app_path%` is a bare CWD-relative runfiles path with NO `rlocation`/`RUNFILES_DIR`
  logic.** It resolves only because `bazel run` sets CWD to the runfiles root of the target
  being run. So **OUR** rule's runfiles must contain the zip at short_path
  `crates/wolf-macos/Wolf.zip`. The runfiles merge is load-bearing.
- **stdio and environment ARE inherited** — direct child, inherited fds.
  `WOLF_FRAME_STATS=1 bazel run //crates/wolf-macos:WolfRelease` works and stderr reaches
  the terminal. **The original "unpredictable stderr plumbing" rationale for making this
  build-only was weaker than assumed — it was inherited from build.md's pre-existing
  warning, not verified against the template.**
- **The app that runs is a fresh unzip into `$TMPDIR/bazel_temp.XXXXXX`, deleted on exit** —
  NOT the signed `.app` the `cquery` recipe resolves. This is the real, concrete reason
  direct exec remains the measurement path.

### `release.bzl` — the only build-logic change

`_opt_bundle_impl`: keep `bundle = ctx.attr.bundle[0]` and its comment verbatim (the
list-indexing observation is still true and load-bearing). Then:

1. **Use `ctx.outputs.executable` as the symlink output, NOT
   `ctx.actions.declare_file(ctx.label.name)`.** With `executable = True` Bazel PREDECLARES
   an output named `ctx.label.name` and exposes it as `ctx.outputs.executable`; a
   `declare_file("WolfRelease")` on top of that is a duplicate declaration and fails analysis.
   `ctx.actions.symlink(output = ctx.outputs.executable,
   target_file = bundle[DefaultInfo].files_to_run.executable, is_executable = True)`.
   `files_to_run.executable` is the canonical way to read another target's executable
   artifact; a transition changes only WHICH configured target `bundle[0]` is, not the
   provider surface. Add a `progress_message`.
2. Runfiles = union of `bundle[DefaultInfo].default_runfiles` (**the essential one — carries
   the OPT `Wolf.zip` at exactly the short_path baked into the runner as `%app_path%`**),
   `bundle[DefaultInfo].data_runfiles` (merged defensively; identical today since
   `macos_application` sets only `runfiles=`, but survives an upstream change), and
   `ctx.runfiles(files = [ctx.outputs.executable])` (belt-and-braces — Bazel adds it
   automatically for an executable rule; comment saying so). Order is irrelevant
   (`Runfiles.merge` is a union keyed by runfiles path) and there is no collision.
3. Return `DefaultInfo(files = bundle[DefaultInfo].files, runfiles = <merged>,
   executable = ctx.outputs.executable)`.

**Do NOT try to forward `bundle[DefaultInfo].files_to_run`** — `DefaultInfo` accepts no such
parameter; Bazel synthesizes `FilesToRunProvider` from `executable`.

`opt_bundle = rule(...)`: add **`executable = True`** — REQUIRED, not optional. Setting
`DefaultInfo.executable` from a non-executable rule is an analysis error, and `bazel run`
dispatches on the rule's executable-ness. Leave `attrs`, the transition, and the absence of
`_allowlist_function_transition` untouched. Update the rule's `doc` to say it is runnable.

**Module docstring:** keep the transition/forwarding/undeclared-sibling/`bazel-bin`
paragraphs; amend "WHAT IT FORWARDS" to include the executable symlink + the dep's runfiles.
**Replace the "DELIBERATELY NOT EXECUTABLE (do not re-litigate)" paragraph entirely** with the
reversed decision AND why the old objection was weaker than assumed (the runner does not
`open`; env/stdio are inherited). Record what remains true: temp-unzip copy at a per-launch
changing path, re-unzip每 launch, output interleaved with Bazel's client output. Record that
`%app_path%` is CWD-relative so the runfiles merge must not be dropped, and that
`--nobuild_runfile_links` / `--enable_runfiles=false` breaks it.

### Docs

- **`build.md:113`** — replace the "deliberately build-only" sentence: `bazel run` launches
  the optimized app and is the convenient way to do so; it inherits environment and stderr
  (the runner invokes the inner binary directly, it does not `open` the bundle). But it runs
  a TEMPORARY unzipped copy under `$TMPDIR`, recreated per launch, deleted on exit, at a
  path that differs every run — so for frame-stats capture, and anything caring about the
  bundle's on-disk identity (TCC prompts, saved state), prefer the direct
  `"$APP/Contents/MacOS/Wolf"` exec. Note `manual` does not prevent `bazel run` on an
  explicit label.
- **`build.md:209`** — keep the `bazel run //crates/wolf-macos:Wolf` stderr caveat framing
  but DROP the "that is also why WolfRelease is deliberately build-only" clause. New text:
  `bazel run //crates/wolf-macos:WolfRelease` also launches the app and DOES pass
  `WOLF_FRAME_STATS` through, but the recommended capture path is preferred because it
  exercises the exact on-disk signed bundle at a stable path, skips a per-launch unzip of
  the whole bundle into `$TMPDIR`, and keeps the app's stderr unmixed with Bazel's client output.
- **`build.md` ~194-198** — "There are two working paths" now promises two but there are
  genuinely three: direct exec (recommended), `bazel run` (works, caveats), and
  `WOLF_FRAME_STATS_FILE` for a double-clicked app. Adjust the framing minimally.
- **`crates/wolf-macos/README.md` ~109** — add
  `bazel run //crates/wolf-macos:WolfRelease   # launch the optimized .app` to the block.
- **`crates/wolf-macos/README.md:118`** — rewrite the "non-executable by design" sentence:
  the rule IS executable, symlinking the transitioned `macos_application` runner into its own
  output and forwarding the dep's runfiles, which is how that runner locates `Wolf.zip`
  (CWD-relative runfiles path -> unzip to temp -> invoke `Contents/MacOS/Wolf` directly,
  inheriting env and stdio). Keep a clause that direct exec remains the measurement path and
  why. Keep the rest (Apple providers, undeclared sibling, generic rule name).
- **`BUILD.bazel`** — header comment line ~15-17: append "Runnable with `bazel run`." Target
  comment: one sentence that `bazel run` launches it and `manual` affects only wildcard
  expansion, not explicit `run`/`build`. The comment does NOT currently claim
  non-executability, so no correction needed beyond these additions.

### Verification (manual protocol — no automated test is viable; a GUI-launching `sh_test` is not)

1. `bazel build //crates/wolf-macos:WolfRelease` succeeds, still reports `Wolf.zip`.
2. **Symlink points into OPT (static):**
   `bazel aquery --output=text 'mnemonic("ExecutableSymlink", //crates/wolf-macos:WolfRelease)'`
   — the action's INPUT path must contain `-opt-`; output is `.../WolfRelease`. A
   `-fastbuild-` input is the bug.
   **CORRECTED FROM THE ORIGINAL PLAN:** the mnemonic is `ExecutableSymlink`, NOT `Symlink`
   — Bazel uses the former when `is_executable = True`, and `Symlink` matches ZERO actions.
   Also expected, not a bug: the action's OWN configuration reads `darwin_arm64-fastbuild`
   (the top-level target's config); only its INPUT crosses into opt.
3. **Runfiles carry the OPT zip (static — the wrong-bundle guard):** resolve
   `.../WolfRelease.runfiles/_main/crates/wolf-macos/Wolf.zip` with `ls -lL`/`readlink`; the
   resolved path must contain `-opt-`. Confirm exactly ONE `Wolf.zip` in the tree.
4. **The LAUNCHED binary is optimized (dynamic — load-bearing, do NOT accept reasoning):**
   `bazel run`, then while up, `ps -axo pid,comm | grep bazel_temp` to get the running path
   under `$TMPDIR/bazel_temp.*/Wolf.app/Contents/MacOS/Wolf`; `stat -f%z` it. Must be
   ~**8.26 MB**, NOT ~**25.4 MB**. Corroborate with
   `WOLF_FRAME_STATS=1 WOLF_FRAME_STATS_INTERVAL=2 bazel run ...`: `ctrl.wake` ~0.5 ms and
   `prev.render` ~6 ms, not 4.85/25.6. **The byte size is the proof; frame stats corroborate.**
5. **Env/stdio:** confirm `wolf-frame-stats enabled interval=2.0s` and subsequent reports
   actually appear on the terminal under `bazel run`. **This determines the final doc
   wording — if it does NOT appear, adjust the docs rather than suppress the finding.**
6. **No regression:** `bazel build //crates/wolf-macos:Wolf` and
   `bazel test //crates/wolf-macos:bundle_layout_test //crates/wolf-macos:WolfKeyMapTests`
   pass; wildcard still does not sweep in `WolfRelease`.

**Failure symptoms to recognize:**

- *Runner launches but cannot find the `.app`* → aborts almost immediately with an `unzip`
  error naming `crates/wolf-macos/Wolf.zip`, or a PlistBuddy failure on a missing
  `Info.plist`; no window. Cause: runfiles dropped, or `--nobuild_runfile_links` /
  `--enable_runfiles=false`.
- *Runner launches the WRONG bundle* → **the app comes up NORMALLY**, which is exactly why
  this must be caught by measurement not by eyeballing. Tell: running binary ~25.4 MB and
  `ctrl.wake` ~4.85 ms. Cause: symlink from a non-transitioned target, or two `Wolf.zip`s
  colliding in one runfiles tree.

### Risks

- **Wrong-bundle silence** — highest severity: a misconfigured symlink launches a perfectly
  working `-Onone` app. Mitigated ONLY by the empirical checks above actually being run.
- **Runfiles-dependent runner** — no `rlocation` fallback; `--nobuild_runfile_links`,
  `--enable_runfiles=false`, or invoking the symlink from `bazel-out` with a different CWD
  all break it. One sentence in the docstring.
- **No second copy staged at build time** — the runfiles tree holds a SYMLINK to `Wolf.zip`,
  and the zip was already in the forwarded runfiles before this change, so build-time/on-disk
  footprint is unchanged. The new cost is at RUNTIME: every `bazel run` unzips the whole
  bundle (bundled Neovim, 16 treesitter parsers, two font families, `Assets.car`) into
  `$TMPDIR` and deletes it on exit. Noticeably slower launch than direct exec. Document; not
  a blocker.
- **Bundle identity churn** — a new temp path each launch means macOS treats it as a
  different app for TCC prompts, saved window state, signature-identity-sensitive behavior.
  Another reason measurement stays direct-exec.
- **`manual` unchanged** — suppresses wildcard expansion only; explicit `run`/`build`
  unaffected. Executability does not alter caching (one extra, essentially free `Symlink`
  action).
- **Downstream** — nothing depends on `:WolfRelease` (`bundle_layout_test` and `xcodeproj`
  both point at `:Wolf`), so adding `FilesToRunProvider` cannot perturb a consumer.

## Status

- [x] architect plan
- [ ] implementation
- [ ] verify: `bazel run` launches the OPT app
- [ ] docs
- [ ] review
