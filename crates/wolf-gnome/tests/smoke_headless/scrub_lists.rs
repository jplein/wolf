//! The packaged smokes' scrub lists: the devShell variables removed from a packaged launch.
//!
//! `flake_exports.rs` compiles this file through `#[path]`, so its `flake.nix` guard reads
//! the same lists the packaged smokes use.

/// Every variable `flake.nix`'s `linuxShell.shellHook` exports that the PACKAGED
/// artifact must supply for itself, removed from every packaged smoke's `cage`
/// invocation.
///
/// `WOLF_INIT_LUA` is in the list although the `shellHook` does not export it, and
/// that asymmetry is deliberate: it is the development override, so a developer who
/// happens to have it exported would silently redirect the packaged binary's
/// `init.lua` away from the store tree — and the executable-relative branch, which
/// is the whole claim, would never run.
///
/// `WOLF_TEST_ORLANDO_DIR` and `WOLF_TEST_LIGHTHOUSE_DIR` are here rather than on the
/// compositor keep-list because NO `wolf-gnome` code reads either: they stage the two
/// vendored trees for `crates/wolf-ffi/tests/orlando_load.rs` and `lighthouse_load.rs`,
/// which reconstruct their own sibling layout in a temp directory. Scrubbing them
/// costs nothing and keeps `KEPT_FOR_THE_COMPOSITOR` at exactly the two names
/// `cage` itself needs.
///
/// `WOLF_DEV_SHELL` is here for the same reason: it is the marker `gates.sh` reads
/// to know it is already inside the dev shell, so it means nothing to `cage` and no
/// `wolf-gnome` code reads it. An installed artifact never runs inside that shell.
///
/// `WOLF_GHOSTTY_VT_DIR` is here for the same reason: only `crates/wolf-vt/build.rs`
/// reads it, at Cargo build time, to find the static `libghostty-vt.a` it links, so
/// no `wolf-gnome` code reads it at run time. An installed artifact carries the
/// archive inside its binary and never needs the variable.
///
/// `WOLF_TEST_TERMINFO_DIR` is here for the same reason: it stages the compiled
/// terminfo entry for the `terminal_terminfo` tests and smoke, and no `wolf-gnome`
/// code reads it.
///
/// `WOLF_TERMINFO_DIR` is here for `WOLF_INIT_LUA`'s reason: the wrapper defaults it to
/// the package's own `share/wolf/terminfo`, so a developer's export would mask that
/// default and the packaged terminal pane would never be shown the package's entry.
pub(crate) const PACKAGED_SCRUB_ALWAYS: [&str; 12] = [
    "WOLF_NVIM_BIN",
    "WOLF_RG_BIN",
    "WOLF_NVIM_TREESITTER_DIR",
    "WOLF_PARSERS_DIR",
    "WOLF_INIT_LUA",
    "WOLF_TERMINFO_DIR",
    "WOLF_TEST_ORLANDO_DIR",
    "WOLF_TEST_LIGHTHOUSE_DIR",
    "WOLF_TEST_TERMINFO_DIR",
    "WOLF_GHOSTTY_VT_DIR",
    "RUST_SRC_PATH",
    "WOLF_DEV_SHELL",
];

/// The two GTK-runtime variables the `shellHook` exports and `wrapGAppsHook4` is
/// supposed to replace.
///
/// Scrubbed only where every launch in the scenario is the WRAPPED binary. The A/B
/// smoke keeps them, because its control leg launches the UNWRAPPED checkout binary,
/// which cannot reach `activate` without them — see
/// `the_packaged_binary_loads_the_vendored_lighthouse_and_orlando_under_cage`.
/// `packaged_binary_renders_typed_text_under_cage` is where the
/// wrapped-GTK-runtime claim is made.
pub(crate) const PACKAGED_SCRUB_GTK_RUNTIME: [&str; 2] = ["XDG_DATA_DIRS", "GSETTINGS_SCHEMA_DIR"];

/// The font stack the devShell pins, scrubbed under the SAME condition as
/// [`PACKAGED_SCRUB_GTK_RUNTIME`] and for an entirely different reason — which is why
/// it is a list of its own rather than a name appended to either neighbour. Each of
/// the three lists carries exactly one reason, and a reader classifying a new export
/// has to pick which reason applies.
///
/// `packaged_binary_renders_typed_text_under_cage` is where the packaged, additive
/// `FONTCONFIG_FILE` is the subject: it runs the store binary with this removed and
/// OCRs the result, so a package whose own font configuration resolves no usable face
/// draws no readable glyph and that smoke fires.
///
/// `the_packaged_binary_loads_the_vendored_lighthouse_and_orlando_under_cage` KEEPS
/// it, deliberately. That smoke's subject is whether the two vendored Lua trees load,
/// and the font stack is an independent variable underneath every rectangle it
/// samples: `cell_metrics_for_smoke` measures the cell through the devShell's pinned
/// configuration, and `PANE_ORIGIN_X_EXPECTED` plus that cell is what
/// `packaged_code_band` and `packaged_prose_band` are derived from. Letting the
/// packaged launch resolve a different face would move the grid under launch B alone,
/// so the A/B would differ in TWO things and neither band could name which. Making a
/// second claim there would not strengthen the first; it would confound it.
pub(crate) const PACKAGED_SCRUB_FONT_STACK: [&str; 1] = ["FONTCONFIG_FILE"];
