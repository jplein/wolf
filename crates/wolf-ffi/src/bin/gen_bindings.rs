//! Standalone swift-bridge codegen for `wolf-ffi`.
//!
//! Run via `bazel run //crates/wolf-ffi:gen_bindings` (Bazel sets
//! `BUILD_WORKSPACE_DIRECTORY` and runs the binary with the workspace writable, so
//! it can rewrite the committed `crates/wolf-ffi/generated/` tree in place — a
//! `genrule` cannot, as it may not write into the source tree).
//!
//! It parses the `#[swift_bridge::bridge]` module in `src/lib.rs` and emits the
//! generated Swift + C header for the per-crate bridge plus the shared
//! `SwiftBridgeCore` runtime, then flattens swift-bridge's nested per-crate
//! directory into `generated/` so the file layout matches the README:
//!
//! - `generated/SwiftBridgeCore.swift`, `generated/SwiftBridgeCore.h`
//! - `generated/wolf-ffi.swift`, `generated/wolf-ffi.h`
//!
//! The hand-written `generated/module.modulemap` and `generated/wolf_ffi_wake.h`
//! are committed and NOT overwritten here.

use std::path::PathBuf;

use swift_bridge_build::parse_bridges;

/// The crate name swift-bridge uses for the per-crate generated files.
const CRATE_NAME: &str = "wolf-ffi";

/// The `import` line prepended to each generated Swift file so the bare C symbols
/// from the committed headers resolve. The module name matches
/// `generated/module.modulemap` and the `swift_interop_hint` in `BUILD.bazel`.
const IMPORT_LINE: &str = "import WolfFFI\n";

fn main() {
    let workspace = std::env::var("BUILD_WORKSPACE_DIRECTORY").unwrap_or_else(|_| {
        eprintln!(
            "BUILD_WORKSPACE_DIRECTORY is unset; run via `bazel run //crates/wolf-ffi:gen_bindings`"
        );
        std::process::exit(1);
    });
    let workspace = PathBuf::from(workspace);
    let bridge_src = workspace.join("crates/wolf-ffi/src/lib.rs");
    let out_dir = workspace.join("crates/wolf-ffi/generated");

    let bridges = parse_bridges([bridge_src]);
    bridges.write_all_concatenated(out_dir.clone(), CRATE_NAME);

    // swift-bridge writes the per-crate files into `{out_dir}/{CRATE_NAME}/`;
    // flatten them up into `{out_dir}/` to match the committed layout.
    let nested = out_dir.join(CRATE_NAME);
    for ext in ["swift", "h"] {
        let from = nested.join(format!("{CRATE_NAME}.{ext}"));
        let to = out_dir.join(format!("{CRATE_NAME}.{ext}"));
        if from.exists() {
            std::fs::rename(&from, &to)
                .unwrap_or_else(|e| panic!("failed to move {from:?} -> {to:?}: {e}"));
        }
    }
    let _ = std::fs::remove_dir(&nested);

    // The generated Swift references the bare `__swift_bridge__$…` C symbols from
    // the committed headers; prepend an `import` of the C interop module (named in
    // `generated/module.modulemap`) so those symbols are in scope when the Swift
    // sources compile as their own module.
    for name in ["SwiftBridgeCore.swift", format!("{CRATE_NAME}.swift").as_str()] {
        let path = out_dir.join(name);
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"));
        if !body.starts_with(IMPORT_LINE) {
            std::fs::write(&path, format!("{IMPORT_LINE}{body}"))
                .unwrap_or_else(|e| panic!("failed to rewrite {path:?}: {e}"));
        }
    }

    println!("wrote swift-bridge bindings into {out_dir:?}");
}
