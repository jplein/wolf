//! Deterministic, headless integration coverage for the `wolf-ffi` config surface
//! (plan §4): the free `wolf_ffi_load_config()` constructor and the opaque
//! `FfiConfig` accessors `font_family()` / `font_size()` / `line_height()`.
//!
//! Unlike the real-nvim tests in this crate (`bridge_smoke`, `panes`, …), this one
//! spawns NO Neovim and needs no network: it drives the config bridge end-to-end
//! purely through `WOLF_CONFIG_DIR` (the test/determinism hook `preferences_dir()`
//! honors first) pointed at a `tempfile::tempdir()`. It is modeled structurally on
//! `tests/frame_pack.rs` (self-contained, no `tests/common/` nvim staging).
//!
//! ## NOTE to coder — Bazel wiring required for this to run under `bazel test`
//!
//! Add a `rust_test` named `config` mirroring `frame_pack` (self-contained, no
//! `common/`, NO `no-sandbox` tag — this test needs no nvim child):
//!
//! ```python
//! rust_test(
//!     name = "config",
//!     srcs = ["tests/config.rs"],
//!     crate_root = "tests/config.rs",
//!     edition = "2021",
//!     deps = [":wolf_ffi"] + all_crate_deps(normal_dev = True),
//! )
//! ```
//!
//! `tempfile` is already a `[dev-dependencies]` of `wolf-ffi`, so `all_crate_deps(
//! normal_dev = True)` supplies it (as it does for `panes`). The bridge accessors
//! and `wolf_ffi_load_config()` are the only `wolf_ffi` symbols named here, so no
//! `//crates/wolf-core` dep is required.
//!
//! Both tests below MUTATE the process-global `WOLF_CONFIG_DIR`, so they are
//! serialized through `ENV_LOCK` (cargo runs them concurrently in one process).

use std::path::Path;
use std::sync::Mutex;

use tempfile::tempdir;
use wolf_ffi::wolf_ffi_load_config;

/// The on-disk config file name the core reads/creates.
const CONFIG_FILE_NAME: &str = "wolf-config.yaml";

/// Platform-independent default font family (see `wolf_core::config`).
// NOTE: must match wolf_core::config::DEFAULT_FONT_FAMILY (the verified iA Writer Mono S CoreText family name).
const EXPECT_DEFAULT_FONT_FAMILY: &str = "iA Writer Mono S";
/// Platform-independent default line-height (percent).
const EXPECT_DEFAULT_LINE_HEIGHT: f64 = 130.0;
/// Default font size on macOS (points).
const EXPECT_DEFAULT_FONT_SIZE_MACOS: f64 = 14.0;
/// Default font size on Linux (points).
const EXPECT_DEFAULT_FONT_SIZE_LINUX: f64 = 10.0;

/// A distinct, non-default font family for the round-trip case.
const CUSTOM_FONT_FAMILY: &str = "Fira Code";
/// A distinct, non-default font size for the round-trip case.
const CUSTOM_FONT_SIZE: i64 = 16;
/// A distinct, non-default line-height for the round-trip case.
const CUSTOM_LINE_HEIGHT: i64 = 150;

// -- Appearance discriminants (mirror `wolf-ffi/src/lib.rs` `mod appearance`) ---
/// `appearance: auto` -> `FfiConfig::appearance()` discriminant (the default).
const APPEARANCE_AUTO: u8 = 0;
/// `appearance: dark` -> `FfiConfig::appearance()` discriminant.
const APPEARANCE_DARK: u8 = 1;
/// `appearance: light` -> `FfiConfig::appearance()` discriminant.
const APPEARANCE_LIGHT: u8 = 2;

/// Serializes the `WOLF_CONFIG_DIR`-mutating tests. `wolf_ffi_load_config()` reads
/// this process-global var via `preferences_dir()`; since cargo runs the tests in
/// this file concurrently within a single process, both cases hold this lock for
/// their whole set/read/remove window so they cannot race each other.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// The platform-specific expected default font size.
fn expected_default_font_size() -> f64 {
    if cfg!(target_os = "macos") {
        EXPECT_DEFAULT_FONT_SIZE_MACOS
    } else {
        EXPECT_DEFAULT_FONT_SIZE_LINUX
    }
}

/// Writes `contents` to `path`, panicking with context on failure (errcheck).
fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
}

#[test]
fn load_config_round_trips() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

    let dir = tempdir().expect("create temp dir");
    let yaml = format!(
        "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {CUSTOM_FONT_SIZE}\n  line-height: {CUSTOM_LINE_HEIGHT}\n",
    );
    write(&dir.path().join(CONFIG_FILE_NAME), &yaml);

    std::env::set_var("WOLF_CONFIG_DIR", dir.path());
    let config = wolf_ffi_load_config();
    std::env::remove_var("WOLF_CONFIG_DIR");

    assert_eq!(
        config.font_family(),
        CUSTOM_FONT_FAMILY,
        "bridge must surface the file's font-family"
    );
    assert_eq!(
        config.font_size(),
        CUSTOM_FONT_SIZE as f64,
        "bridge must surface the file's font-size"
    );
    assert_eq!(
        config.line_height(),
        CUSTOM_LINE_HEIGHT as f64,
        "bridge must surface the file's line-height"
    );
}

#[test]
fn load_config_creates_defaults_when_absent() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

    let dir = tempdir().expect("create temp dir");
    let path = dir.path().join(CONFIG_FILE_NAME);
    assert!(!path.exists(), "precondition: no config file yet");

    std::env::set_var("WOLF_CONFIG_DIR", dir.path());
    let config = wolf_ffi_load_config();
    std::env::remove_var("WOLF_CONFIG_DIR");

    assert_eq!(
        config.font_family(),
        EXPECT_DEFAULT_FONT_FAMILY,
        "absent file -> default font family over the bridge"
    );
    assert_eq!(
        config.font_size(),
        expected_default_font_size(),
        "absent file -> platform default font size over the bridge"
    );
    assert_eq!(
        config.line_height(),
        EXPECT_DEFAULT_LINE_HEIGHT,
        "absent file -> default line-height over the bridge"
    );

    assert!(
        path.exists(),
        "loading with an absent file must create {CONFIG_FILE_NAME} with defaults"
    );
}

#[test]
fn appearance_discriminant_round_trips() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

    // Each present-and-valid `appearance` value must surface as its `mod appearance`
    // discriminant over the bridge (OD-a: `appearance() -> u8`).
    let present_cases = [
        ("dark", APPEARANCE_DARK),
        ("light", APPEARANCE_LIGHT),
        ("auto", APPEARANCE_AUTO),
    ];
    for (value, want) in present_cases {
        let dir = tempdir().expect("create temp dir");
        let yaml = format!(
            "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {CUSTOM_FONT_SIZE}\n  \
             line-height: {CUSTOM_LINE_HEIGHT}\n  appearance: {value}\n",
        );
        write(&dir.path().join(CONFIG_FILE_NAME), &yaml);

        std::env::set_var("WOLF_CONFIG_DIR", dir.path());
        let config = wolf_ffi_load_config();
        std::env::remove_var("WOLF_CONFIG_DIR");

        assert_eq!(
            config.appearance(),
            want,
            "`appearance: {value}` must surface as discriminant {want} over the bridge"
        );
    }

    // Back-compat: a config with NO `appearance` key (only the required font fields)
    // must parse successfully and default to `auto` (0), NOT fall back to defaults.
    let dir = tempdir().expect("create temp dir");
    let yaml = format!(
        "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {CUSTOM_FONT_SIZE}\n  \
         line-height: {CUSTOM_LINE_HEIGHT}\n",
    );
    write(&dir.path().join(CONFIG_FILE_NAME), &yaml);

    std::env::set_var("WOLF_CONFIG_DIR", dir.path());
    let config = wolf_ffi_load_config();
    std::env::remove_var("WOLF_CONFIG_DIR");

    assert_eq!(
        config.appearance(),
        APPEARANCE_AUTO,
        "an absent `appearance` key must surface as auto (0) over the bridge (back-compat)"
    );
    // The custom font fields must survive too — proving the section parsed rather
    // than falling back to the full defaults.
    assert_eq!(
        config.font_family(),
        CUSTOM_FONT_FAMILY,
        "back-compat: font-family preserved when appearance is absent"
    );
}

#[test]
fn macos_option_as_alt_round_trips() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

    // A present `macos-option-as-alt: true` must surface as `true` over the bridge.
    let dir = tempdir().expect("create temp dir");
    let yaml = format!(
        "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {CUSTOM_FONT_SIZE}\n  \
         line-height: {CUSTOM_LINE_HEIGHT}\n  macos-option-as-alt: true\n",
    );
    write(&dir.path().join(CONFIG_FILE_NAME), &yaml);

    std::env::set_var("WOLF_CONFIG_DIR", dir.path());
    let config = wolf_ffi_load_config();
    std::env::remove_var("WOLF_CONFIG_DIR");

    assert!(
        config.macos_option_as_alt(),
        "`macos-option-as-alt: true` must surface as true over the bridge"
    );

    // Back-compat: a config with NO `macos-option-as-alt` key (only the required
    // font fields) must parse successfully and surface `false`, NOT fall back to
    // defaults — the custom font fields must survive.
    let dir = tempdir().expect("create temp dir");
    let yaml = format!(
        "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {CUSTOM_FONT_SIZE}\n  \
         line-height: {CUSTOM_LINE_HEIGHT}\n",
    );
    write(&dir.path().join(CONFIG_FILE_NAME), &yaml);

    std::env::set_var("WOLF_CONFIG_DIR", dir.path());
    let config = wolf_ffi_load_config();
    std::env::remove_var("WOLF_CONFIG_DIR");

    assert!(
        !config.macos_option_as_alt(),
        "an absent `macos-option-as-alt` key must surface as false over the bridge (back-compat)"
    );
    assert_eq!(
        config.font_family(),
        CUSTOM_FONT_FAMILY,
        "back-compat: font-family preserved when macos-option-as-alt is absent"
    );
}
