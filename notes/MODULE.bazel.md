# `MODULE.bazel`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Module scope and pins

The module builds the macOS side: `wolf-core` and the AppKit front-end, with the C and link toolchain from apple_support driven by `DEVELOPER_DIR` to the host Xcode, rustc downloaded by rules_rust, and crates resolved from the Cargo workspace through crate_universe. `wolf-gnome` has no Bazel target: cargo metadata reads it during a repin, but it is never built, so its GTK stack never enters the Bazel graph. `wolf-core`'s rule of no GUI toolkit in its dependency tree is checked by the denylist commands in [the wolf-core decisions](wolf-core-decisions.md).

The direct pins match the resolved dependency graph, since Minimal Version Selection upgrades lower pins anyway. rules_rust 0.71.2 requires platforms 1.1.0, and apple_support 2.6.1 requires rules_cc 0.2.15. apple_support 2.6.1 declares `bazel_compatibility = [">=7.4.0"]` and rules_rust 0.71.2 declares no restriction, so the set builds on the Bazel 8 that `.bazelversion` pins with no rule-set re-pinning.
