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

WHAT IT FORWARDS: only `DefaultInfo`. Deliberately NOT `AppleBundleInfo` /
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

DELIBERATELY NOT EXECUTABLE (do not re-litigate). Making this `bazel run`-nable
is technically possible, but build.md already documents that
`bazel run //crates/wolf-macos:Wolf` has less predictable stderr plumbing through
the rules_apple runner. That is decisive here: the entire point of this bundle is
that it is the one people launch with `WOLF_FRAME_STATS=1` and read STDERR from.
A run target whose stderr is unreliable defeats its own purpose. It would also
stage a second copy of a large bundle into runfiles — one more place for "which
.app am I actually running" to go wrong. Launch the bundle directly instead.
"""

def _opt_transition_impl(_settings, _attr):
    return {"//command_line_option:compilation_mode": "opt"}

_opt_transition = transition(
    implementation = _opt_transition_impl,
    # Nothing is read from the incoming configuration: the new value is
    # unconditional, so `opt_bundle` is opt regardless of `-c` on the command
    # line (with `-c opt` the transition is simply a no-op).
    inputs = [],
    outputs = ["//command_line_option:compilation_mode"],
)

def _opt_bundle_impl(ctx):
    # OBSERVED on Bazel 8.7.0: an attr carrying a Starlark transition is a LIST
    # of configured `Target`s even for a 1:1 (non-split) transition, so the
    # single dep must be indexed. (The plain-`Target` form fails analysis with
    # "got Provider for sequence index, want int".) `[0]` is safe precisely
    # because the transition is 1:1 — it always returns exactly one dict.
    bundle = ctx.attr.bundle[0]
    return [DefaultInfo(
        files = bundle[DefaultInfo].files,
        runfiles = bundle[DefaultInfo].default_runfiles,
    )]

opt_bundle = rule(
    implementation = _opt_bundle_impl,
    doc = "Builds an Apple bundle target at `-c opt` without `-c opt` on the command line.",
    attrs = {
        "bundle": attr.label(
            doc = "The bundle target to rebuild in the optimized configuration.",
            mandatory = True,
            providers = [DefaultInfo],
            cfg = _opt_transition,
        ),
    },
)
