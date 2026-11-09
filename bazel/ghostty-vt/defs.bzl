"""`ghostty_vt_archive`: builds libghostty-vt's static archive with the pinned Zig.

The action runs `build_archive.sh`, which extracts the release and deps tarballs into a
scratch directory beside the output and runs `zig build` offline (`--system`) with an
empty Zig cache.

The build calls `/usr/bin/xcrun`, which needs the selected Xcode, and the strict action
env passes no client variables. The action therefore takes the environment the resolved
C toolchain gives its compile actions, whose Xcode version and SDK keys Bazel turns into
`DEVELOPER_DIR` and `SDKROOT`. It does not use `apple_support.run_shell`: that needs an
`_xcode_config` attribute, which puts `@bazel_tools//tools/objc:host_xcodes` in the
dependency graph, where the wolf-core-deps gate's `objc` pattern matches it once
wolf-core depends on wolf-vt. The cc toolchain type is already in that graph.
"""

load("@rules_cc//cc:action_names.bzl", "ACTION_NAMES")
load("@rules_cc//cc:find_cc_toolchain.bzl", "find_cc_toolchain", "use_cc_toolchain")
load("@rules_cc//cc/common:cc_common.bzl", "cc_common")

def _xcode_env(ctx):
    """The environment the C toolchain sets on a C compile, which carries the Xcode keys."""
    cc_toolchain = find_cc_toolchain(ctx)
    feature_configuration = cc_common.configure_features(
        ctx = ctx,
        cc_toolchain = cc_toolchain,
        requested_features = ctx.features,
        unsupported_features = ctx.disabled_features,
    )
    return cc_common.get_environment_variables(
        feature_configuration = feature_configuration,
        action_name = ACTION_NAMES.c_compile,
        variables = cc_common.create_compile_variables(
            feature_configuration = feature_configuration,
            cc_toolchain = cc_toolchain,
        ),
    )

def _ghostty_vt_archive_impl(ctx):
    out = ctx.actions.declare_file("libghostty-vt.a")
    zig = ctx.file._zig
    ctx.actions.run_shell(
        inputs = depset(
            [ctx.file.src, ctx.file.deps_tarball, ctx.file._script, zig],
            transitive = [ctx.attr._zig_files[DefaultInfo].files],
        ),
        outputs = [out],
        command = "/bin/sh \"$1\" \"$2\" \"$3\" \"$4\" \"$5\"",
        arguments = [
            ctx.file._script.path,
            ctx.file.src.path,
            ctx.file.deps_tarball.path,
            zig.path,
            out.path,
        ],
        env = _xcode_env(ctx),
        execution_requirements = {"requires-darwin": ""},
        mnemonic = "GhosttyVtBuild",
        progress_message = "Building libghostty-vt with Zig %{output}",
    )
    return [DefaultInfo(files = depset([out]))]

ghostty_vt_archive = rule(
    implementation = _ghostty_vt_archive_impl,
    doc = "Builds `libghostty-vt.a` from the pinned release and deps tarballs.",
    attrs = {
        "src": attr.label(
            allow_single_file = [".tar.gz"],
            mandatory = True,
            doc = "The libghostty-vt release source tarball.",
        ),
        "deps_tarball": attr.label(
            allow_single_file = [".tar.gz"],
            mandatory = True,
            doc = "The release's companion tarball of Zig packages, passed to `--system`.",
        ),
        "_script": attr.label(
            default = Label("//bazel/ghostty-vt:build_archive.sh"),
            allow_single_file = True,
        ),
        "_zig": attr.label(
            default = Label("//bazel/zig:zig"),
            allow_single_file = True,
            cfg = "exec",
        ),
        "_zig_files": attr.label(
            default = Label("//bazel/zig:zig_files"),
            cfg = "exec",
        ),
    },
    fragments = ["cpp"],
    toolchains = use_cc_toolchain(),
)
