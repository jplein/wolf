//! Headless coverage for `wolf-config.yaml`'s `gtk-titlebar-style` key, read through the public
//! `wolf_core::load_or_create`: how each value of the key parses, that a file without it keeps
//! its font settings and gets `tabs`, that an invalid value fails the whole section, and that the
//! defaults file written for a new install carries `gtk-titlebar-style: tabs`.
//!
//! Every row writes non-default font fields, so a row that silently fell back to the defaults
//! cannot pass as a parsed one. Needs no nvim: everything runs in `tempfile` tempdirs passed to
//! `load_or_create` explicitly, so no case touches the environment.

use std::path::Path;

use tempfile::tempdir;
use wolf_core::config::CONFIG_FILE_NAME;
use wolf_core::{load_or_create, GtkTitlebarStyle, WolfConfig};

/// A non-default font family, so a row that fell back to the defaults is told apart.
const CUSTOM_FONT_FAMILY: &str = "Fira Code";
/// A non-default font size, for the same reason.
const CUSTOM_FONT_SIZE: f64 = 16.0;
/// A non-default line-height, for the same reason.
const CUSTOM_LINE_HEIGHT: f64 = 150.0;

/// The documented default font family, re-typed so the test does not move with the source.
const EXPECT_DEFAULT_FONT_FAMILY: &str = "iA Writer Mono S";
/// The documented default line-height, all platforms.
const EXPECT_DEFAULT_LINE_HEIGHT: f64 = 180.0;

/// The line a new install's defaults file must carry.
const EXPECT_DEFAULT_LINE: &str = "gtk-titlebar-style: tabs";

/// The documented default font size: 14 pt on macOS, 10 pt elsewhere.
fn expect_default_font_size() -> f64 {
    if cfg!(target_os = "macos") {
        14.0
    } else {
        10.0
    }
}

/// Reads a file to a `String`, panicking with context on failure (errcheck).
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Writes `contents` to `path`, panicking with context on failure (errcheck).
fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
}

/// A full `config:` section with the custom font fields, plus `gtk-titlebar-style: {value}` when
/// `value` is `Some`.
fn config_yaml(value: Option<&str>) -> String {
    let mut yaml = format!(
        "config:\n  font-family: \"{CUSTOM_FONT_FAMILY}\"\n  font-size: {}\n  line-height: {}\n",
        CUSTOM_FONT_SIZE as i64, CUSTOM_LINE_HEIGHT as i64,
    );
    if let Some(value) = value {
        yaml.push_str(&format!("  gtk-titlebar-style: {value}\n"));
    }
    yaml
}

/// Asserts `cfg` kept the custom font fields, which proves the section parsed rather than
/// falling back to the defaults.
fn assert_fonts_kept(cfg: &WolfConfig, row: &str) {
    assert_eq!(
        cfg.font_family, CUSTOM_FONT_FAMILY,
        "{row}: font_family fell back, so the section failed to parse instead of taking this value"
    );
    assert_eq!(
        cfg.font_size, CUSTOM_FONT_SIZE,
        "{row}: font_size fell back, so the section failed to parse instead of taking this value"
    );
    assert_eq!(
        cfg.line_height, CUSTOM_LINE_HEIGHT,
        "{row}: line_height fell back, so the section failed to parse instead of taking this value"
    );
}

/// Each value of the key, the key's absence and an invalid value, in that order. A valid or absent
/// key parses with the fonts kept; `sideways` fails the whole section to the literal defaults and
/// leaves the user's file byte-identical.
#[test]
fn the_key_parses_by_table() {
    let parsed: [(&str, Option<&str>, GtkTitlebarStyle); 3] = [
        ("absent key", None, GtkTitlebarStyle::Tabs),
        ("`tabs`", Some("tabs"), GtkTitlebarStyle::Tabs),
        ("`native`", Some("native"), GtkTitlebarStyle::Native),
    ];
    for (row, value, want) in parsed {
        let dir = tempdir().expect("create temp dir");
        write(&dir.path().join(CONFIG_FILE_NAME), &config_yaml(value));
        let cfg = load_or_create(dir.path());
        // Fonts first: a section that failed to parse also reads `Tabs`, from the defaults.
        assert_fonts_kept(&cfg, row);
        assert_eq!(
            cfg.gtk_titlebar_style, want,
            "{row}: the section parsed but the key resolved to the wrong style; for the absent \
             key that means the serde default is not `tabs`"
        );
    }

    let row = "invalid `sideways`";
    let dir = tempdir().expect("create temp dir");
    let path = dir.path().join(CONFIG_FILE_NAME);
    let invalid = config_yaml(Some("sideways"));
    write(&path, &invalid);
    let cfg = load_or_create(dir.path());
    assert_eq!(
        cfg.font_family, EXPECT_DEFAULT_FONT_FAMILY,
        "{row}: the custom font family survived, so an invalid value parsed (to some variant, or \
         was ignored) instead of failing the whole section"
    );
    assert_eq!(
        cfg.font_size,
        expect_default_font_size(),
        "{row}: font_size is not the platform default, so the section did not fall back"
    );
    assert_eq!(
        cfg.line_height, EXPECT_DEFAULT_LINE_HEIGHT,
        "{row}: line_height is not the default, so the section did not fall back"
    );
    assert_eq!(
        cfg.gtk_titlebar_style,
        GtkTitlebarStyle::Tabs,
        "{row}: the fallback's style is not `tabs`, so the defaults do not carry the default style"
    );
    assert_eq!(
        read(&path),
        invalid,
        "{row}: load_or_create rewrote the user's file; a broken file must be left untouched"
    );
}

/// An absent file is created with the defaults: `load_or_create` answers `tabs`, the written text
/// carries `gtk-titlebar-style: tabs`, and a re-read answers `tabs` from that text.
#[test]
fn a_new_install_writes_tabs() {
    let dir = tempdir().expect("create temp dir");
    let path = dir.path().join(CONFIG_FILE_NAME);
    assert!(
        !path.exists(),
        "precondition: the config file must not exist yet"
    );

    let created = load_or_create(dir.path());
    assert_eq!(
        created.gtk_titlebar_style,
        GtkTitlebarStyle::Tabs,
        "a new install's config must default to `tabs`"
    );
    let text = read(&path);
    assert!(
        text.contains(EXPECT_DEFAULT_LINE),
        "the defaults file must carry `{EXPECT_DEFAULT_LINE}`; it lacks the key (not serialized) \
         or spells the value otherwise. Written:\n{text}"
    );

    let reread = load_or_create(dir.path());
    assert_eq!(
        reread.gtk_titlebar_style,
        GtkTitlebarStyle::Tabs,
        "re-reading the written defaults must give `tabs`"
    );
}
