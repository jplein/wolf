//! User-preferences (`wolf-config.yaml`) reading + platform prefs-dir resolution.
//!
//! This is the GUI-toolkit-free config module: it resolves the per-platform
//! preferences directory (macOS `~/Library/Preferences/Wolf`, Linux
//! `~/.config/wolf`), and reads — or creates-with-defaults — a
//! `wolf-config.yaml` holding `config: { font-family, font-size, line-height }`.
//!
//! The production implementation (the `WolfConfig` struct, the private serde
//! `ConfigFile`/`ConfigSection`, the `CONFIG_FILE_NAME`/`DEFAULT_*` consts, and
//! the `preferences_dir()` / `default_config()` / `load_or_create()` functions)
//! is written by the coder ABOVE this test module, per the architect plan §2.
//
// NOTE(test-writer): production code goes here (config module); the coder
// implements it above this test module.

use std::path::{Path, PathBuf};

/// The on-disk config file name read/created in the platform preferences dir.
pub const CONFIG_FILE_NAME: &str = "wolf-config.yaml";

/// The default font family (platform-independent).
const DEFAULT_FONT_FAMILY: &str = "iA Writer Mono S";
/// The default line-height, as a percentage of the font size (all platforms).
const DEFAULT_LINE_HEIGHT: f64 = 130.0;
/// The default font size in points on macOS.
const DEFAULT_FONT_SIZE_MACOS: f64 = 14.0;
/// The default font size in points on Linux.
const DEFAULT_FONT_SIZE_LINUX: f64 = 10.0;

/// The app appearance mode: whether to follow the OS or force dark/light.
///
/// A GUI-toolkit-free value type surfaced to the front-end, and both front-ends
/// resolve it. `Auto` means the app tracks the desktop's own dark/light setting
/// live; `Dark`/`Light` force that appearance regardless of the OS. Serialized as
/// kebab-case (`auto`/`dark`/`light`) in `wolf-config.yaml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Appearance {
    /// Follow the desktop's own light/dark appearance, switching live when the
    /// OS changes.
    Auto,
    /// Force dark mode regardless of the OS setting.
    Dark,
    /// Force light mode regardless of the OS setting.
    Light,
}

/// The built-in default appearance: follow the OS (`auto`).
const DEFAULT_APPEARANCE: Appearance = Appearance::Auto;

/// The default appearance, used as the `#[serde(default)]` for the on-disk
/// `appearance` field so a config that predates the key parses to `Auto`.
fn default_appearance() -> Appearance {
    DEFAULT_APPEARANCE
}

/// The built-in default for the macOS option-as-alt flag: `false`
/// (option-as-option — Option composes accented characters rather than sending
/// Neovim `<M-…>` mappings). Mirrors Ghostty's `macos-option-as-alt` default.
const DEFAULT_MACOS_OPTION_AS_ALT: bool = false;

/// The default macos-option-as-alt flag, used as the `#[serde(default)]` for the
/// on-disk `macos-option-as-alt` field so a config that predates the key parses
/// to `false`.
fn default_macos_option_as_alt() -> bool {
    DEFAULT_MACOS_OPTION_AS_ALT
}

/// The resolved user configuration surfaced to the front-end.
///
/// A GUI-toolkit-free value type: `font_family` is a CoreText/Pango family name,
/// `font_size` is in points, and `line_height` is a percentage of the font size
/// (e.g. `130.0` means 130%). Read/created from `wolf-config.yaml` by
/// [`load_or_create`], or produced by [`default_config`] when no file resolves.
#[derive(Debug, Clone)]
pub struct WolfConfig {
    /// The font family name (e.g. `"iA Writer Mono S"`).
    pub font_family: String,
    /// The font size, in points.
    pub font_size: f64,
    /// The line-height, as a percentage of the font size (e.g. `130.0` == 130%).
    pub line_height: f64,
    /// The app appearance mode (`auto`/`dark`/`light`); defaults to `auto`.
    pub appearance: Appearance,
    /// On macOS, whether the Option key acts as Alt (`<M-…>` mappings) rather
    /// than composing accented characters; defaults to `false` (option-as-option).
    pub macos_option_as_alt: bool,
}

/// The on-disk file shape: a single top-level `config:` key wrapping the section.
#[derive(serde::Serialize, serde::Deserialize)]
struct ConfigFile {
    config: ConfigSection,
}

/// The `config:` section's three kebab-case fields (`font-family`, `font-size`,
/// `line-height`). ALL are required: a partial section deserializes to an error,
/// which [`load_or_create`] treats as "fall back to defaults" (never a partial
/// merge).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ConfigSection {
    font_family: String,
    font_size: f64,
    line_height: f64,
    /// Back-compat: a config file predating this key parses to `Auto` rather than
    /// failing the whole section (unlike the required font fields).
    #[serde(default = "default_appearance")]
    appearance: Appearance,
    /// Back-compat: a config file predating this key parses to `false` rather than
    /// failing the whole section (unlike the required font fields).
    #[serde(default = "default_macos_option_as_alt")]
    macos_option_as_alt: bool,
}

/// Resolves the per-platform preferences directory that holds `wolf-config.yaml`.
///
/// The `WOLF_CONFIG_DIR` environment variable overrides everything (a
/// test/determinism hook); otherwise it is macOS `~/Library/Preferences/Wolf`
/// (via [`dirs::preference_dir`]) or Linux `~/.config/wolf` (via
/// [`dirs::config_dir`]). Returns `None` only when the platform dir cannot be
/// resolved (no override and no home directory).
#[must_use]
pub fn preferences_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("WOLF_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    #[cfg(target_os = "macos")]
    {
        dirs::preference_dir().map(|d| d.join("Wolf"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::config_dir().map(|d| d.join("wolf"))
    }
}

/// The built-in defaults: font family `"iA Writer Mono S"`, line-height 130%, and
/// a platform-specific font size (14 pt on macOS, 10 pt elsewhere).
#[must_use]
pub fn default_config() -> WolfConfig {
    let font_size = if cfg!(target_os = "macos") {
        DEFAULT_FONT_SIZE_MACOS
    } else {
        DEFAULT_FONT_SIZE_LINUX
    };
    WolfConfig {
        font_family: DEFAULT_FONT_FAMILY.to_string(),
        font_size,
        line_height: DEFAULT_LINE_HEIGHT,
        appearance: DEFAULT_APPEARANCE,
        macos_option_as_alt: DEFAULT_MACOS_OPTION_AS_ALT,
    }
}

/// Reads `dir/wolf-config.yaml`, or creates it with defaults when absent.
///
/// INFALLIBLE (defaults-on-error), so the front-end always gets a usable config:
///
/// - Absent file -> creates the parent dir, writes the serialized defaults, and
///   returns the defaults.
/// - Present + well-formed -> returns the parsed values (the file is NOT
///   rewritten).
/// - Present + malformed / partial / unreadable -> logs a `tracing::warn!` and
///   returns the defaults WITHOUT overwriting the user's file (graceful
///   degradation: a broken user file is preserved for the user to fix).
#[must_use]
pub fn load_or_create(dir: &Path) -> WolfConfig {
    let path = dir.join(CONFIG_FILE_NAME);
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_yaml::from_str::<ConfigFile>(&text) {
            Ok(file) => WolfConfig {
                font_family: file.config.font_family,
                font_size: file.config.font_size,
                line_height: file.config.line_height,
                appearance: file.config.appearance,
                macos_option_as_alt: file.config.macos_option_as_alt,
            },
            Err(e) => {
                tracing::warn!(
                    "wolf-core: malformed {}: {e}; using defaults (file left untouched)",
                    path.display()
                );
                default_config()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let config = default_config();
            write_defaults(&path, &config);
            config
        }
        Err(e) => {
            tracing::warn!(
                "wolf-core: could not read {}: {e}; using defaults",
                path.display()
            );
            default_config()
        }
    }
}

/// Serializes `config` (wrapped in the `config:` section) to `path`, creating the
/// parent directory first. INFALLIBLE: any error is logged, not returned (config
/// loading must never fail).
fn write_defaults(path: &Path, config: &WolfConfig) {
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                "wolf-core: could not create config dir {}: {e}",
                parent.display()
            );
            return;
        }
    }
    let file = ConfigFile {
        config: ConfigSection {
            font_family: config.font_family.clone(),
            font_size: config.font_size,
            line_height: config.line_height,
            appearance: config.appearance,
            macos_option_as_alt: config.macos_option_as_alt,
        },
    };
    match serde_yaml::to_string(&file) {
        Ok(yaml) => {
            if let Err(e) = std::fs::write(path, yaml) {
                tracing::warn!("wolf-core: could not write {}: {e}", path.display());
            }
        }
        Err(e) => {
            tracing::warn!("wolf-core: could not serialize default config: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;
    use std::sync::Mutex;

    use tempfile::tempdir;

    /// The on-disk file name the module reads/creates. Kept as a local literal so
    /// the tests do not depend on the production `CONFIG_FILE_NAME` const being
    /// public; if the coder makes it `pub`, this can be swapped for `super::…`.
    const CONFIG_FILE_NAME: &str = "wolf-config.yaml";

    /// The documented default font family (platform-independent).
    // NOTE: must match wolf_core::config::DEFAULT_FONT_FAMILY (the verified iA Writer Mono S CoreText family name).
    const EXPECT_DEFAULT_FONT_FAMILY: &str = "iA Writer Mono S";
    /// The documented default line-height (percent of font size), all platforms.
    const EXPECT_DEFAULT_LINE_HEIGHT: f64 = 130.0;
    /// The documented default font size on macOS (points).
    const EXPECT_DEFAULT_FONT_SIZE_MACOS: f64 = 14.0;
    /// The documented default font size on Linux (points).
    const EXPECT_DEFAULT_FONT_SIZE_LINUX: f64 = 10.0;

    /// A distinct, non-default font family used by the "parse an existing file"
    /// cases, so a pass cannot be faked by the defaults.
    const CUSTOM_FONT_FAMILY: &str = "Fira Code";
    /// A distinct, non-default font size used by the same cases.
    const CUSTOM_FONT_SIZE: f64 = 16.0;
    /// A distinct, non-default line-height used by the same cases.
    const CUSTOM_LINE_HEIGHT: f64 = 150.0;

    /// Serializes every test that mutates PROCESS-GLOBAL environment (here, only
    /// `preferences_dir_honors_env_override`, which sets `WOLF_CONFIG_DIR`).
    ///
    /// `cargo test` runs the tests in this module concurrently on multiple
    /// threads within ONE process, so an env var set by one test is visible to
    /// (and would race with) any other test reading the same var. Holding this
    /// lock for the whole set/read/remove window makes such tests mutually
    /// exclusive. The non-env cases below never touch the environment (they pass
    /// an explicit `dir` to `load_or_create`), so they do not need the lock.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Field-wise config equality (avoids requiring `PartialEq`/`Debug` derives on
    /// the production `WolfConfig`). `font_size`/`line_height` are integ(14/130/…)
    /// or half-integer values parsed exactly from the YAML, so exact `f64`
    /// comparison is correct here.
    fn assert_config_eq(actual: &WolfConfig, expected: &WolfConfig, ctx: &str) {
        assert_eq!(
            actual.font_family, expected.font_family,
            "{ctx}: font_family"
        );
        assert_eq!(actual.font_size, expected.font_size, "{ctx}: font_size");
        assert_eq!(
            actual.line_height, expected.line_height,
            "{ctx}: line_height"
        );
        // `Appearance` derives `PartialEq`/`Debug` (plan §3.1), so compare it
        // directly. `default_config()` yields `Appearance::Auto`.
        assert_eq!(actual.appearance, expected.appearance, "{ctx}: appearance");
        // The macOS option-as-alt flag (plan: `WolfConfig.macos_option_as_alt`,
        // kebab key `macos-option-as-alt`); `default_config()` yields `false`.
        assert_eq!(
            actual.macos_option_as_alt, expected.macos_option_as_alt,
            "{ctx}: macos_option_as_alt"
        );
    }

    /// Reads a file to a `String`, panicking with context on failure (errcheck).
    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
    }

    /// Writes `contents` to `path`, panicking with context on failure (errcheck).
    fn write(path: &Path, contents: &str) {
        std::fs::write(path, contents)
            .unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
    }

    // =====================================================================
    // load_or_create — absent file
    // =====================================================================

    #[test]
    fn creates_file_with_defaults_when_absent() {
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join(CONFIG_FILE_NAME);
        assert!(
            !path.exists(),
            "precondition: config file must not exist yet"
        );

        // Absent -> returns the platform defaults AND writes the file.
        let created = load_or_create(dir.path());
        assert_config_eq(&created, &default_config(), "created == defaults");
        assert!(path.exists(), "load_or_create must create the config file");

        // The written YAML has the documented shape: a top-level `config:` key and
        // the three kebab-case fields.
        let text = read(&path);
        assert!(
            text.contains("config:"),
            "written YAML must have a top-level `config:` key, got:\n{text}"
        );
        assert!(
            text.contains("font-family:"),
            "written YAML must use kebab-case `font-family:`, got:\n{text}"
        );
        assert!(
            text.contains("font-size:"),
            "written YAML must use kebab-case `font-size:`, got:\n{text}"
        );
        assert!(
            text.contains("line-height:"),
            "written YAML must use kebab-case `line-height:`, got:\n{text}"
        );
        assert!(
            text.contains("appearance:"),
            "written YAML must serialize the `appearance:` field, got:\n{text}"
        );

        // Re-reading the file the module just wrote parses back to the same values.
        let reread = load_or_create(dir.path());
        assert_config_eq(&reread, &created, "re-read == first create");
    }

    // =====================================================================
    // load_or_create — existing, well-formed file
    // =====================================================================

    #[test]
    fn parses_existing_file() {
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join(CONFIG_FILE_NAME);
        let yaml = format!(
            "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {}\n  line-height: {}\n",
            CUSTOM_FONT_SIZE as i64, CUSTOM_LINE_HEIGHT as i64,
        );
        write(&path, &yaml);
        let before = read(&path);

        let cfg = load_or_create(dir.path());
        assert_eq!(cfg.font_family, CUSTOM_FONT_FAMILY, "parsed font_family");
        assert_eq!(cfg.font_size, CUSTOM_FONT_SIZE, "parsed font_size");
        assert_eq!(cfg.line_height, CUSTOM_LINE_HEIGHT, "parsed line_height");

        // A present, parseable file is read verbatim, never rewritten.
        let after = read(&path);
        assert_eq!(
            before, after,
            "load_or_create must NOT rewrite a valid existing config file"
        );
    }

    #[test]
    fn round_trips() {
        let dir = tempdir().expect("create temp dir");

        // First call creates the file from defaults; the second reads that exact
        // file back. Both must equal the defaults — i.e. the serialized form the
        // module writes round-trips cleanly through its own reader.
        let first = load_or_create(dir.path());
        let second = load_or_create(dir.path());
        assert_config_eq(&first, &default_config(), "first == defaults");
        assert_config_eq(&second, &first, "second (re-read) == first");
    }

    // =====================================================================
    // load_or_create — degraded inputs (defaults-on-error, never clobber)
    // =====================================================================

    #[test]
    fn malformed_file_falls_back_to_defaults() {
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join(CONFIG_FILE_NAME);
        let junk = "this is: not: valid: yaml: {{{\n\t- broken\n";
        write(&path, junk);

        let cfg = load_or_create(dir.path());
        assert_config_eq(&cfg, &default_config(), "malformed -> defaults");

        // A malformed USER file must be preserved untouched (graceful degradation).
        assert_eq!(
            read(&path),
            junk,
            "load_or_create must NOT overwrite a malformed user config file"
        );
    }

    #[test]
    fn missing_field_falls_back_to_defaults() {
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join(CONFIG_FILE_NAME);
        // A partial `config:` (only font-family; font-size/line-height absent) does
        // not deserialize into the full section, so the whole load falls back.
        let partial = format!("config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n");
        write(&path, &partial);

        let cfg = load_or_create(dir.path());
        assert_config_eq(&cfg, &default_config(), "missing fields -> defaults");

        // As with malformed input, the partial user file is left as-is.
        assert_eq!(
            read(&path),
            partial,
            "load_or_create must NOT overwrite a partial user config file"
        );
    }

    #[test]
    fn valid_yaml_wrong_shape_falls_back_to_defaults() {
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join(CONFIG_FILE_NAME);
        // Structurally valid YAML, but the WRONG SHAPE: `config` maps to a scalar
        // string instead of the expected mapping. This parses cleanly as YAML yet
        // fails to deserialize into `ConfigFile`, so the whole load falls back.
        let wrong_shape = "config: \"just a string\"\n";
        write(&path, wrong_shape);

        let cfg = load_or_create(dir.path());
        assert_config_eq(&cfg, &default_config(), "wrong-shape -> defaults");

        // As with malformed/partial input, the wrong-shape user file is preserved
        // byte-for-byte (never clobbered/rewritten).
        assert_eq!(
            read(&path),
            wrong_shape,
            "load_or_create must NOT overwrite a valid-YAML wrong-shape config file"
        );
    }

    // =====================================================================
    // appearance — new config key (auto/dark/light, default auto)
    // =====================================================================

    /// Formats a full, well-formed `config:` section (all three required font
    /// fields, with the CUSTOM non-default values) plus an `appearance:` line
    /// holding `value`. Used by the appearance parse/round-trip cases so a pass
    /// cannot be faked by the defaults.
    fn config_yaml_with_appearance(value: &str) -> String {
        format!(
            "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {}\n  \
             line-height: {}\n  appearance: {value}\n",
            CUSTOM_FONT_SIZE as i64, CUSTOM_LINE_HEIGHT as i64,
        )
    }

    #[test]
    fn default_appearance_is_auto() {
        assert_eq!(
            default_config().appearance,
            Appearance::Auto,
            "the built-in default appearance must be Auto"
        );
    }

    #[test]
    fn parses_appearance_values() {
        // Each valid enum string maps to its DISTINCT variant, and the font fields
        // parse alongside it (so we know the whole section deserialized, not a
        // defaults fallback).
        let cases = [
            ("auto", Appearance::Auto),
            ("dark", Appearance::Dark),
            ("light", Appearance::Light),
        ];
        for (value, want) in cases {
            let dir = tempdir().expect("create temp dir");
            let path = dir.path().join(CONFIG_FILE_NAME);
            write(&path, &config_yaml_with_appearance(value));

            let cfg = load_or_create(dir.path());
            assert_eq!(
                cfg.appearance, want,
                "`appearance: {value}` must parse to {want:?}"
            );
            // The font fields must survive alongside the appearance field.
            assert_eq!(
                cfg.font_family, CUSTOM_FONT_FAMILY,
                "`appearance: {value}` case: font_family preserved"
            );
            assert_eq!(
                cfg.font_size, CUSTOM_FONT_SIZE,
                "`appearance: {value}` case: font_size preserved"
            );
            assert_eq!(
                cfg.line_height, CUSTOM_LINE_HEIGHT,
                "`appearance: {value}` case: line_height preserved"
            );
        }
    }

    #[test]
    fn missing_appearance_defaults_to_auto() {
        // Back-compat: a config with all the required font fields but NO
        // `appearance` key must parse SUCCESSFULLY (via `#[serde(default)]`), NOT
        // fall back to the full defaults. So appearance is Auto AND the custom font
        // fields are preserved (distinguishing this from the whole-file fallback of
        // `missing_field_falls_back_to_defaults`, which drops a REQUIRED font field).
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join(CONFIG_FILE_NAME);
        let yaml = format!(
            "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {}\n  \
             line-height: {}\n",
            CUSTOM_FONT_SIZE as i64, CUSTOM_LINE_HEIGHT as i64,
        );
        write(&path, &yaml);

        let cfg = load_or_create(dir.path());
        assert_eq!(
            cfg.appearance,
            Appearance::Auto,
            "a config with no `appearance` key must default to Auto (back-compat)"
        );
        // The custom font fields must be preserved — proving the section parsed
        // rather than falling back to defaults.
        assert_eq!(
            cfg.font_family, CUSTOM_FONT_FAMILY,
            "back-compat: font_family preserved when appearance is absent"
        );
        assert_eq!(
            cfg.font_size, CUSTOM_FONT_SIZE,
            "back-compat: font_size preserved when appearance is absent"
        );
        assert_eq!(
            cfg.line_height, CUSTOM_LINE_HEIGHT,
            "back-compat: line_height preserved when appearance is absent"
        );
    }

    #[test]
    fn invalid_appearance_falls_back_to_defaults() {
        // An out-of-range appearance value (`purple`) fails the enum, so the whole
        // section fails to deserialize and the load falls back to the defaults
        // (OD-b: whole-file fallback), leaving the user's file untouched — the same
        // contract as the wrong-shape case above.
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join(CONFIG_FILE_NAME);
        let invalid = config_yaml_with_appearance("purple");
        write(&path, &invalid);

        let cfg = load_or_create(dir.path());
        assert_config_eq(&cfg, &default_config(), "invalid appearance -> defaults");

        // The user's (invalid) file must be preserved byte-for-byte, never rewritten.
        assert_eq!(
            read(&path),
            invalid,
            "load_or_create must NOT overwrite a config with an invalid appearance value"
        );
    }

    #[test]
    fn appearance_round_trips() {
        // 1. An absent file is created with the defaults: the written YAML carries
        //    the `appearance:` key and re-reads to Auto.
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join(CONFIG_FILE_NAME);
        let created = load_or_create(dir.path());
        assert_eq!(
            created.appearance,
            Appearance::Auto,
            "defaults-created config must have appearance Auto"
        );
        assert!(
            read(&path).contains("appearance:"),
            "the defaults-written YAML must serialize the `appearance:` key"
        );
        let reread = load_or_create(dir.path());
        assert_eq!(
            reread.appearance,
            Appearance::Auto,
            "re-reading the written defaults must round-trip appearance to Auto"
        );

        // 2. Hand-written non-default values (`dark`, `light`) round-trip through the
        //    loader to their matching variants.
        for (value, want) in [("dark", Appearance::Dark), ("light", Appearance::Light)] {
            let vdir = tempdir().expect("create temp dir");
            write(
                &vdir.path().join(CONFIG_FILE_NAME),
                &config_yaml_with_appearance(value),
            );
            let cfg = load_or_create(vdir.path());
            assert_eq!(
                cfg.appearance, want,
                "hand-written `appearance: {value}` must load as {want:?}"
            );
        }
    }

    // =====================================================================
    // macos-option-as-alt — new config key (bool, default false =
    // option-as-option; mirrors Ghostty's `macos-option-as-alt`)
    // =====================================================================

    /// Formats a full, well-formed `config:` section (all three required font
    /// fields, with the CUSTOM non-default values) plus a `macos-option-as-alt:`
    /// line holding `value`. Used by the option-as-alt parse/round-trip cases so a
    /// pass cannot be faked by the defaults.
    fn config_yaml_with_option_as_alt(value: bool) -> String {
        format!(
            "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {}\n  \
             line-height: {}\n  macos-option-as-alt: {value}\n",
            CUSTOM_FONT_SIZE as i64, CUSTOM_LINE_HEIGHT as i64,
        )
    }

    #[test]
    fn default_macos_option_as_alt_is_false() {
        assert!(
            !default_config().macos_option_as_alt,
            "the built-in default macos_option_as_alt must be false (option-as-option)"
        );
    }

    #[test]
    fn parses_macos_option_as_alt_values() {
        // Each boolean maps to itself, and the font fields parse alongside it (so we
        // know the whole section deserialized, not a defaults fallback).
        for value in [true, false] {
            let dir = tempdir().expect("create temp dir");
            let path = dir.path().join(CONFIG_FILE_NAME);
            write(&path, &config_yaml_with_option_as_alt(value));

            let cfg = load_or_create(dir.path());
            assert_eq!(
                cfg.macos_option_as_alt, value,
                "`macos-option-as-alt: {value}` must parse to {value}"
            );
            // The font fields must survive alongside the option-as-alt field,
            // proving the whole section parsed (not a defaults fallback).
            assert_eq!(
                cfg.font_family, CUSTOM_FONT_FAMILY,
                "`macos-option-as-alt: {value}` case: font_family preserved"
            );
            assert_eq!(
                cfg.font_size, CUSTOM_FONT_SIZE,
                "`macos-option-as-alt: {value}` case: font_size preserved"
            );
            assert_eq!(
                cfg.line_height, CUSTOM_LINE_HEIGHT,
                "`macos-option-as-alt: {value}` case: line_height preserved"
            );
        }
    }

    #[test]
    fn missing_macos_option_as_alt_defaults_to_false() {
        // Back-compat: a config with all the required font fields but NO
        // `macos-option-as-alt` key must parse SUCCESSFULLY (via
        // `#[serde(default)]`), NOT fall back to the full defaults. So the flag is
        // false AND the custom font fields are preserved (distinguishing this from
        // the whole-file fallback of `missing_field_falls_back_to_defaults`, which
        // drops a REQUIRED font field).
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join(CONFIG_FILE_NAME);
        let yaml = format!(
            "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {}\n  \
             line-height: {}\n",
            CUSTOM_FONT_SIZE as i64, CUSTOM_LINE_HEIGHT as i64,
        );
        write(&path, &yaml);

        let cfg = load_or_create(dir.path());
        assert!(
            !cfg.macos_option_as_alt,
            "a config with no `macos-option-as-alt` key must default to false (back-compat)"
        );
        // The custom font fields must be preserved — proving the section parsed
        // rather than falling back to defaults.
        assert_eq!(
            cfg.font_family, CUSTOM_FONT_FAMILY,
            "back-compat: font_family preserved when macos-option-as-alt is absent"
        );
        assert_eq!(
            cfg.font_size, CUSTOM_FONT_SIZE,
            "back-compat: font_size preserved when macos-option-as-alt is absent"
        );
        assert_eq!(
            cfg.line_height, CUSTOM_LINE_HEIGHT,
            "back-compat: line_height preserved when macos-option-as-alt is absent"
        );
    }

    #[test]
    fn macos_option_as_alt_round_trips() {
        // 1. An absent file is created with the defaults: the written YAML carries
        //    the `macos-option-as-alt:` key and re-reads to false.
        let dir = tempdir().expect("create temp dir");
        let path = dir.path().join(CONFIG_FILE_NAME);
        let created = load_or_create(dir.path());
        assert!(
            !created.macos_option_as_alt,
            "defaults-created config must have macos_option_as_alt false"
        );
        assert!(
            read(&path).contains("macos-option-as-alt:"),
            "the defaults-written YAML must serialize the `macos-option-as-alt:` key"
        );
        let reread = load_or_create(dir.path());
        assert!(
            !reread.macos_option_as_alt,
            "re-reading the written defaults must round-trip macos_option_as_alt to false"
        );

        // 2. A hand-written `true` round-trips through the loader to `true`.
        let vdir = tempdir().expect("create temp dir");
        write(
            &vdir.path().join(CONFIG_FILE_NAME),
            &config_yaml_with_option_as_alt(true),
        );
        let cfg = load_or_create(vdir.path());
        assert!(
            cfg.macos_option_as_alt,
            "hand-written `macos-option-as-alt: true` must load as true"
        );
    }

    // =====================================================================
    // default_config — platform-specific font size
    // =====================================================================

    #[test]
    fn defaults_are_platform_specific() {
        let cfg = default_config();
        assert_eq!(
            cfg.font_family, EXPECT_DEFAULT_FONT_FAMILY,
            "default font family is platform-independent"
        );
        assert_eq!(
            cfg.line_height, EXPECT_DEFAULT_LINE_HEIGHT,
            "default line-height is platform-independent (130%)"
        );

        let want_size = if cfg!(target_os = "macos") {
            EXPECT_DEFAULT_FONT_SIZE_MACOS
        } else {
            EXPECT_DEFAULT_FONT_SIZE_LINUX
        };
        assert_eq!(
            cfg.font_size, want_size,
            "default font size is platform-specific (14 macOS / 10 Linux)"
        );
    }

    // =====================================================================
    // preferences_dir — WOLF_CONFIG_DIR override (env-mutating: serialized)
    // =====================================================================

    #[test]
    fn preferences_dir_honors_env_override() {
        // Env-mutating: hold ENV_LOCK across the whole set/read/remove window so
        // this cannot race any other env-touching test in this process.
        let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

        let dir = tempdir().expect("create temp dir");
        std::env::set_var("WOLF_CONFIG_DIR", dir.path());

        let resolved = preferences_dir();
        std::env::remove_var("WOLF_CONFIG_DIR");

        assert_eq!(
            resolved.as_deref(),
            Some(dir.path()),
            "WOLF_CONFIG_DIR must override the platform preferences dir"
        );
    }
}
