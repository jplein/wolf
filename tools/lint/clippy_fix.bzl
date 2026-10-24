"""`bazel run`-nable Rust fixers backed by the rules_rust downloaded toolchain.

The Darwin devShell ships no standalone cargo/rustc/clippy/rustfmt. These rules
resolve the `@rules_rust//rust:toolchain_type` (the toolchain rules_rust
downloads: 1.85.0) and stage its whole sysroot (cargo, cargo-clippy,
clippy-driver, rustc, rustfmt, rust_std, rustc_lib) as runfiles, then generate a
launcher that puts the toolchain `bin/` on PATH and runs the fixer(s) in
`$BUILD_WORKSPACE_DIRECTORY` (the developer's real checkout).

Two rules:
  - `clippy_fix`: runs `cargo clippy --fix --allow-dirty --allow-staged --workspace`.
  - `fix_all`:    runs `cargo fmt`, then `cargo clippy --fix`, then the host
                  Xcode swift-format over the wolf-macos Swift sources.

CAVEAT (documented in build.md / README): `cargo fmt` / `cargo clippy --fix`
shell out to cargo, which resolves crates from crates.io (network) and links via
the host Xcode cc toolchain. These are NON-HERMETIC developer conveniences, not
hermetic Bazel build actions.
"""

def _toolchain_launcher(ctx, template):
    toolchain = ctx.toolchains["@rules_rust//rust:toolchain_type"]
    launcher = ctx.actions.declare_file(ctx.label.name + "_launcher.sh")
    ctx.actions.expand_template(
        template = template,
        output = launcher,
        substitutions = {
            "@@CARGO_SHORT_PATH@@": toolchain.cargo.short_path,
        },
        is_executable = True,
    )
    runfiles = ctx.runfiles(transitive_files = toolchain.all_files)
    return [DefaultInfo(executable = launcher, runfiles = runfiles)]

def _clippy_fix_impl(ctx):
    return _toolchain_launcher(ctx, ctx.file._launcher_tpl)

clippy_fix = rule(
    implementation = _clippy_fix_impl,
    doc = "Runs `cargo clippy --fix` using the rules_rust downloaded toolchain.",
    attrs = {
        "_launcher_tpl": attr.label(
            default = Label("//tools/lint:clippy_fix_launcher.sh.tpl"),
            allow_single_file = True,
        ),
    },
    toolchains = ["@rules_rust//rust:toolchain_type"],
    executable = True,
)

def _fix_all_impl(ctx):
    rust_toolchain = ctx.toolchains["@rules_rust//rust:toolchain_type"]
    rustfmt_toolchain = ctx.toolchains["@rules_rust//rust/rustfmt:toolchain_type"]

    launcher = ctx.actions.declare_file(ctx.label.name + "_launcher.sh")
    ctx.actions.expand_template(
        template = ctx.file._launcher_tpl,
        output = launcher,
        substitutions = {
            "@@CARGO_SHORT_PATH@@": rust_toolchain.cargo.short_path,
            "@@RUSTFMT_SHORT_PATH@@": rustfmt_toolchain.rustfmt.short_path,
        },
        is_executable = True,
    )

    # Both toolchains: the rust toolchain's bare `rustfmt` links a different librustc_driver hash
    # and aborts at load time, so rustfmt comes from the separate rustfmt toolchain.
    runfiles = ctx.runfiles(
        transitive_files = depset(
            transitive = [rust_toolchain.all_files, rustfmt_toolchain.all_files],
        ),
    )
    return [DefaultInfo(executable = launcher, runfiles = runfiles)]

fix_all = rule(
    implementation = _fix_all_impl,
    doc = "Runs rustfmt + cargo clippy --fix + swift-format format, all in place.",
    attrs = {
        "_launcher_tpl": attr.label(
            default = Label("//tools/lint:fix_all.sh"),
            allow_single_file = True,
        ),
    },
    toolchains = [
        "@rules_rust//rust:toolchain_type",
        "@rules_rust//rust/rustfmt:toolchain_type",
    ],
    executable = True,
)
