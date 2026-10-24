"""An optimized (`-c opt`) build of an Apple bundle, as a NAMED TARGET.

`//crates/wolf-macos:Wolf` builds in whatever configuration the command line
asks for, which by default is `fastbuild`: Swift at `-Onone` and Rust
unoptimized. That is the right fast-iteration development build, but it is
SUBSTANTIALLY slower at runtime than the optimized one, and it is not what
should be shipped or measured. See build.md for the measured penalty.

WHY A TRANSITION AND NOT AN ALIAS. Compilation mode is a CONFIGURATION, not an
attribute. No amount of `alias`/`macos_application` attribute wiring can change
it, because it is not something the target owns — it is part of the context the
target is analyzed in. The only mechanism that changes configuration from
within the build graph is a Starlark transition. `opt_bundle` therefore applies
an OUTGOING-EDGE transition to its `bundle` dep, setting
`//command_line_option:compilation_mode` to `opt`, so the ENTIRE subgraph below
it (both Swift modules, all Rust crates, the treesitter parsers) is rebuilt
optimized without `-c opt` ever appearing on the command line.

WHAT IT FORWARDS: only `DefaultInfo` — the transitioned dep's `files` (the opt
`Wolf.zip`), an EXECUTABLE that is a symlink to the transitioned
`macos_application`'s own runner, and the dep's RUNFILES (which is how that
runner finds the zip; see below). Deliberately NOT `AppleBundleInfo` /
`MacosApplicationBundleInfo` / `AppleExecutableBinaryInfo` — nothing consumes
them (`bundle_layout_test` and `xcodeproj` depend on `:Wolf` directly), and
forwarding Apple providers out of a non-Apple rule invites rules_apple to treat
this target as an embeddable bundle: added risk, zero benefit.

THE `.app` IS AN UNDECLARED SIBLING OF THE ZIP. `macos_application`'s declared
output here is `Wolf.zip`. Without
`--define=apple.experimental.tree_artifact_outputs` (this repo does not set it)
rules_apple does NOT `declare_directory` the bundle; `Wolf_archive-root/Wolf.app`
is a side-effect directory the bundling/signing action leaves on disk next to
the zip. There is consequently nothing bundle-shaped to put in `DefaultInfo` —
we forward the zip, and the `.app` is found at `${ZIP%.zip}_archive-root/Wolf.app`.
Because Bazel does not track that directory, an action-cache hit restores only
the zip; if the directory is ever missing, `ditto -x -k <zip> <dir>` recreates it.

`bazel-bin` IS NOT THE RIGHT PLACE TO LOOK. `bazel-bin` symlinks the bin dir of
the TOP-LEVEL target configuration. The top-level target here is analyzed in the
DEFAULT (fastbuild) config — the transition applies BELOW it, on the dep edge.
So `bazel-bin/crates/wolf-macos/Wolf_archive-root/Wolf.app` is either absent or,
far worse, a STALE fastbuild `-Onone` bundle left by an earlier `:Wolf` build.
Resolve the path from the configuration-aware
`bazel cquery --output=files //crates/wolf-macos:WolfRelease` instead, NEVER from
`bazel-bin`.

RUNNABLE (this reverses an earlier decision, and the reversal is the correct
one). This target was previously build-only on the theory that the rules_apple
runner's stderr plumbing was unreliable, which would defeat the purpose of the
one bundle people launch with `WOLF_FRAME_STATS=1`. That objection was WEAKER
THAN ASSUMED and did not survive contact with the evidence: it was inherited
from a general warning in build.md, never verified against the template.
`macos.template.sh` invokes `"${APP_DIR}/Contents/MacOS/${BUNDLE_EXECUTABLE}"`
as a DIRECT CHILD — it does not `open` the bundle — so environment and stdio are
inherited. Verified empirically: `WOLF_FRAME_STATS=1 bazel run` prints
`wolf-frame-stats enabled interval=...` and its subsequent reports to the
terminal.

WHAT REMAINS TRUE, and why direct exec is still the MEASUREMENT path: the runner
`unzip`s the bundle into a fresh `mktemp -d "${TMPDIR:-/tmp}/bazel_temp.XXXXXX"`
and deletes it on exit, so (a) the app that runs is a TEMPORARY COPY, not the
signed `.app` the `cquery` recipe resolves, (b) its path CHANGES EVERY LAUNCH,
which macOS treats as a different app for TCC prompts and saved window state,
(c) every launch re-unzips the WHOLE bundle (bundled Neovim, 16 treesitter
parsers, two font families, `Assets.car`), making it noticeably slower than a
direct exec, and (d) the app's output interleaves with Bazel's own client
output. For frame-stats capture, run `"$APP/Contents/MacOS/Wolf"` directly.

THE RUNFILES MERGE IS LOAD-BEARING — DO NOT DROP IT. `run_support.bzl`
substitutes `%app_path%` with `archive.short_path` (`crates/wolf-macos/Wolf.zip`),
a bare CWD-RELATIVE runfiles path with NO `rlocation` / `RUNFILES_DIR` logic. It
resolves only because `bazel run` sets CWD to the runfiles root of the target
being run, so OUR rule's runfiles must carry the zip at exactly that short_path.
Consequently `--nobuild_runfile_links` and `--enable_runfiles=false` BREAK this
target (symptom: an `unzip` error naming `crates/wolf-macos/Wolf.zip`, or a
PlistBuddy failure on a missing `Info.plist`, and no window).

VERIFYING THE TRANSITION STILL APPLIES. Nothing automated guards this, and the
failure mode is SILENT: a misconfigured symlink launches a perfectly working
`-Onone` app, so it cannot be caught by eyeballing. Two checks, both verified
empirically:

1. STATIC.
   `bazel aquery --output=text 'mnemonic("ExecutableSymlink", //crates/wolf-macos:WolfRelease)'`
   — the action's INPUT path must contain `-opt-`. A `-fastbuild-` input is the
   bug. NOTE the action's OWN configuration reads `darwin_arm64-fastbuild`: that
   is the TOP-LEVEL target's config, and only its input crosses into opt. This is
   expected, not a bug, and is easy to misdiagnose. NOTE also the mnemonic is
   `ExecutableSymlink`, not `Symlink` — Bazel uses the former when
   `is_executable = True`, and `Symlink` matches zero actions.

2. DYNAMIC (the conclusive one). While the app is running under `bazel run`, find
   the process under `$TMPDIR/bazel_temp.*/Wolf.app/Contents/MacOS/Wolf` and check
   its size: ~8.26 MB (8,255,984 bytes observed) optimized, vs ~25.4 MB
   (25,384,256 bytes) fastbuild. The ~3x difference is unambiguous.

Frame-stats values are NOT usable corroboration for an idle app: reports are
activity-gated (`FrameMetrics.swift` guards on `recordedAnything`), so with no
typing the buckets the transition would affect never appear. The byte size is the
proof.
"""

def _opt_transition_impl(_settings, _attr):
    return {"//command_line_option:compilation_mode": "opt"}

_opt_transition = transition(
    implementation = _opt_transition_impl,
    # Reads nothing from the incoming config: the value is unconditional, a no-op under `-c opt`.
    inputs = [],
    outputs = ["//command_line_option:compilation_mode"],
)

def _opt_bundle_impl(ctx):
    # On Bazel 8.7.0 a transitioned attr is a list of `Target`s even for a 1:1 transition;
    # `[0]` is safe because this transition always returns exactly one dict.
    bundle = ctx.attr.bundle[0]

    # `executable = True` predeclares `ctx.outputs.executable`; declaring it again with
    # `declare_file` fails analysis, so the symlink writes to the predeclared output.
    ctx.actions.symlink(
        output = ctx.outputs.executable,
        target_file = bundle[DefaultInfo].files_to_run.executable,
        is_executable = True,
        progress_message = "Linking optimized runner for %{label}",
    )

    runfiles = bundle[DefaultInfo].default_runfiles.merge_all([
        # Identical to `default_runfiles` today, merged defensively so an upstream change cannot
        # silently drop the zip the runner needs.
        bundle[DefaultInfo].data_runfiles,
        # Belt-and-braces: Bazel adds the executable to an executable rule's
        # runfiles automatically.
        ctx.runfiles(files = [ctx.outputs.executable]),
    ])

    return [DefaultInfo(
        files = bundle[DefaultInfo].files,
        runfiles = runfiles,
        executable = ctx.outputs.executable,
    )]

opt_bundle = rule(
    implementation = _opt_bundle_impl,
    executable = True,
    doc = (
        "Builds an Apple bundle target at `-c opt` without `-c opt` on the " +
        "command line. Runnable: `bazel run` launches the optimized bundle " +
        "via the transitioned dep's runner."
    ),
    attrs = {
        "bundle": attr.label(
            doc = "The bundle target to rebuild in the optimized configuration.",
            mandatory = True,
            providers = [DefaultInfo],
            cfg = _opt_transition,
        ),
    },
)
