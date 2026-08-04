//! The pure configuration seam: every rule about CONSUMING a
//! [`wolf_core::WolfConfig`], with no GTK and no environment read.
//!
//! The Linux counterpart of [`crate::state_seam`], and the same shape: `app.rs`
//! does the environment read and the file load, this module decides what the
//! values mean. Injecting the override rather than reading `WOLF_FONT_PT` here is
//! what makes the precedence rule testable without touching the process
//! environment, which a parallel test suite cannot safely do.
//!
//! Reading `wolf-config.yaml` itself belongs to `wolf_core::config`; nothing here
//! touches the filesystem.

use wolf_core::{Appearance, WolfConfig};

/// The font size used when neither `WOLF_FONT_PT` nor the configured `font-size`
/// is usable, in points.
///
/// The value the front-end ran at before it read a config at all, so a developer
/// with no config file and no override sees what they saw then.
pub const FALLBACK_FONT_PT: f64 = 14.0;

/// One configuration setting this front-end reads and does not honour.
///
/// Reported once at launch rather than dropped silently: a user who wrote a
/// setting deserves to learn it does nothing here.
pub struct UnsupportedSetting {
    /// The key's spelling in `wolf-config.yaml`, so the message names the line to
    /// edit.
    pub key: &'static str,
    /// The value the user actually wrote, echoed back.
    pub value: &'static str,
    /// Why this front-end does not honour it.
    pub reason: &'static str,
}

/// The `appearance` key, spelled as `wolf-config.yaml` spells it.
const APPEARANCE_KEY: &str = "appearance";

/// The point size to measure the cell at: the environment override when it is
/// usable, else the configured `font-size`, else [`FALLBACK_FONT_PT`].
///
/// "Usable" is finite and strictly positive. Both inputs come from a human — one
/// from a shell, one from a hand-edited YAML file — so `0`, a negative and a
/// non-finite value are all reachable, and any of them reaching Pango produces a
/// window that draws nothing with no error anywhere.
///
/// **The override wins on purpose.** That is parity, not a Linux quirk:
/// `CellMetrics.defaultPointSize` gives `WOLF_FONT_PT` the same precedence on
/// macOS, `crates/wolf-gnome/README.md` documents the override, and the compositor
/// smokes drive it.
#[must_use]
pub fn resolve_font_pt(env_override: Option<f64>, config_font_size: f64) -> f64 {
    if let Some(pt) = env_override {
        if pt.is_finite() && pt > 0.0 {
            return pt;
        }
    }
    if config_font_size.is_finite() && config_font_size > 0.0 {
        return config_font_size;
    }
    FALLBACK_FONT_PT
}

/// The configured settings this front-end reads and does not honour, one row each.
///
/// A forced `appearance` produces a row. `macos_option_as_alt` produces **none**,
/// however it is set, and the asymmetry is deliberate: one `wolf-config.yaml` is
/// shared across both platforms and the key's own name says which platform it is
/// for, so a Linux warning would fire on every launch for a setting that is
/// correct where its author wrote it. A forced appearance has no such excuse — it
/// names no platform and it does nothing here.
#[must_use]
pub fn unsupported_settings(config: &WolfConfig) -> Vec<UnsupportedSetting> {
    let mut rows = Vec::new();
    let value = match config.appearance {
        Appearance::Auto => None,
        Appearance::Dark => Some("dark"),
        Appearance::Light => Some("light"),
    };
    if let Some(value) = value {
        rows.push(UnsupportedSetting {
            key: APPEARANCE_KEY,
            value,
            reason: "wolf-gnome does no theming in either direction: it neither queries the OS \
                     theme nor pushes one into Neovim, so the appearance follows Neovim's own \
                     colorscheme and the system GTK theme",
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    use wolf_core::{default_config, Appearance, WolfConfig};

    /// The `font-size` a Linux `wolf-config.yaml` carries by default, in points.
    /// Used as the "configured" arm so a pass cannot be faked by the last-resort
    /// fallback, which is a different number.
    const CONFIGURED_PT: f64 = 10.0;

    /// The `WOLF_FONT_PT` value every compositor smoke exports, in points.
    /// Deliberately unequal to both [`CONFIGURED_PT`] and
    /// [`EXPECT_FALLBACK_FONT_PT`], so each of the three outcomes is
    /// distinguishable from the other two.
    const ENV_PT: f64 = 28.0;

    /// [`FALLBACK_FONT_PT`] re-typed here as a LITERAL, per this crate's
    /// re-declare-do-not-import discipline.
    ///
    /// Asserting `resolve_font_pt(None, 0.0) == FALLBACK_FONT_PT` would hold for
    /// whatever value the constant took — including `0.0`, the degenerate input
    /// the guard exists to reject. Stating 14 independently means a change to the
    /// constant fails here rather than moving with it.
    const EXPECT_FALLBACK_FONT_PT: f64 = 14.0;

    /// The config key an unsupported forced appearance must be reported under.
    const APPEARANCE_KEY: &str = "appearance";

    /// The config key that must NEVER be reported, however it is spelled.
    const OPTION_AS_ALT_KEY: &str = "macos-option-as-alt";

    /// A config carrying the requested `appearance` and `macos_option_as_alt`, with
    /// every other field left at the shipped defaults.
    ///
    /// Built by mutating [`default_config`] rather than by a struct literal so a
    /// later field added to `WolfConfig` does not break this file — the seam under
    /// test reads two fields and a new one is not this file's business.
    fn cfg_with(appearance: Appearance, macos_option_as_alt: bool) -> WolfConfig {
        let mut config = default_config();
        config.appearance = appearance;
        config.macos_option_as_alt = macos_option_as_alt;
        config
    }

    /// The reported keys, in order — a `Vec<&str>` so a failure message can name
    /// what was actually produced without `UnsupportedSetting` having to derive
    /// `Debug`.
    fn keys(rows: &[UnsupportedSetting]) -> Vec<&'static str> {
        rows.iter().map(|row| row.key).collect()
    }

    // =====================================================================
    // resolve_font_pt — the WOLF_FONT_PT over `font-size` precedence (D1)
    // =====================================================================

    /// `WOLF_FONT_PT` wins over the configured `font-size`.
    ///
    /// This is macOS parity, not a Linux quirk: `CellMetrics.defaultPointSize`
    /// gives the environment the same precedence. Reversing it silently re-sizes
    /// every compositor smoke, all of which export `WOLF_FONT_PT` and none of
    /// which would report the configured size instead as an error.
    #[test]
    fn env_override_wins_over_the_configured_font_size() {
        assert_eq!(
            resolve_font_pt(Some(ENV_PT), CONFIGURED_PT),
            ENV_PT,
            "a usable WOLF_FONT_PT must beat the configured font-size. Getting {CONFIGURED_PT} \
             means the precedence is reversed, which re-sizes the cell in all thirty-five \
             compositor smokes at once and drops `crates/wolf-gnome/README.md`'s documented \
             override on the floor."
        );
    }

    /// An unusable override falls through to the configured size rather than
    /// poisoning the measurement.
    ///
    /// `WOLF_FONT_PT` comes from a human, so `0`, a negative, and a value that
    /// parses to a non-finite float are all reachable. Each must behave exactly as
    /// an absent variable does; a `0.0` reaching `for_font` yields a zero-width
    /// cell and a window that draws no columns at all.
    #[test]
    fn an_unusable_env_override_falls_back_to_the_configured_size() {
        let cases: [(&str, Option<f64>); 5] = [
            ("absent", None),
            ("zero", Some(0.0)),
            ("negative", Some(-3.0)),
            ("NaN", Some(f64::NAN)),
            ("infinite", Some(f64::INFINITY)),
        ];
        for (what, override_pt) in cases {
            let got = resolve_font_pt(override_pt, CONFIGURED_PT);
            assert_eq!(
                got, CONFIGURED_PT,
                "the {what} override must fall through to the configured {CONFIGURED_PT} pt, but \
                 this returned {got}. Anything else means the `is_finite() && > 0.0` filter on the \
                 override arm was dropped: a 0 or a negative gives a zero-or-inverted cell size, \
                 and a NaN makes every later comparison against the cell size false, so the pane \
                 view reports no rows and the window paints nothing with no error logged."
            );
        }
    }

    /// An unusable CONFIGURED size falls through to the last-resort fallback.
    ///
    /// The config file is user-written too, and `load_or_create` returns whatever
    /// number the YAML carried — `font-size: 0` parses cleanly. The second guard is
    /// what stops that reaching Pango.
    #[test]
    fn an_unusable_configured_size_falls_back_to_the_last_resort() {
        assert_eq!(
            FALLBACK_FONT_PT, EXPECT_FALLBACK_FONT_PT,
            "FALLBACK_FONT_PT must be {EXPECT_FALLBACK_FONT_PT} pt — the value moved verbatim from \
             `app.rs`'s DEFAULT_FONT_PT, so a developer with no config and no WOLF_FONT_PT sees \
             what they saw before the config was read at all."
        );

        let cases: [(&str, f64); 4] = [
            ("zero", 0.0),
            ("negative", -12.0),
            ("NaN", f64::NAN),
            ("infinite", f64::INFINITY),
        ];
        for (what, configured) in cases {
            let got = resolve_font_pt(None, configured);
            assert_eq!(
                got, EXPECT_FALLBACK_FONT_PT,
                "a {what} configured font-size must fall through to {EXPECT_FALLBACK_FONT_PT} pt, \
                 but this returned {got}. Returning the configured value unchecked means a \
                 hand-edited `font-size: 0` in `wolf-config.yaml` reaches `CellMetrics::for_font` \
                 and the window comes up blank — with the config file, not the code, as the cause."
            );
        }
    }

    // =====================================================================
    // unsupported_settings — what is reported once at launch, and what is not
    // =====================================================================

    /// The default `appearance: auto` is fully supported, so nothing is reported.
    ///
    /// Without this case a `unsupported_settings` that returned the appearance row
    /// unconditionally would still satisfy every positive case below, and every
    /// launch would print a warning about a setting the user never wrote.
    #[test]
    fn appearance_auto_reports_no_unsupported_setting() {
        let rows = unsupported_settings(&cfg_with(Appearance::Auto, false));
        assert!(
            rows.is_empty(),
            "`appearance: auto` is what the shipped default writes and it is honoured (the \
             front-end simply follows nothing), so it must produce NO row. Got {:?}. An \
             unconditional row means every launch prints a warning about a setting the user did \
             not choose, which trains the reader to ignore the one that matters.",
            keys(&rows)
        );
    }

    /// A forced `appearance` is reported exactly once, with a key and a reason.
    ///
    /// D2: a user who wrote `appearance: dark` deserves to learn it does nothing on
    /// this front-end, and gets told once at launch rather than never.
    #[test]
    fn a_forced_appearance_reports_one_unsupported_setting() {
        let mut values: Vec<&'static str> = Vec::new();
        for appearance in [Appearance::Dark, Appearance::Light] {
            let rows = unsupported_settings(&cfg_with(appearance, false));
            assert_eq!(
                rows.len(),
                1,
                "a forced {appearance:?} appearance must produce exactly ONE row, got {:?}. Zero \
                 means the appearance arm was deleted and the setting is silently dropped — the \
                 outcome D2 rejects. More than one means the same setting is reported twice and \
                 the launch log stops being readable.",
                keys(&rows)
            );
            let row = &rows[0];
            assert_eq!(
                row.key, APPEARANCE_KEY,
                "the row for a forced {appearance:?} appearance must be keyed {APPEARANCE_KEY:?} — \
                 the exact spelling of the key in `wolf-config.yaml`, so the message tells the user \
                 which line to edit."
            );
            assert!(
                !row.reason.is_empty(),
                "the {appearance:?} row carries an EMPTY reason, so the launch message would name \
                 a setting and say nothing about why it does nothing. The reason is where the \
                 \"no theming in either direction\" limitation reaches the user."
            );
            assert!(
                !row.value.is_empty(),
                "the {appearance:?} row carries an EMPTY value, so the message cannot echo back \
                 what the user actually wrote."
            );
            values.push(row.value);
        }
        assert_ne!(
            values[0], values[1],
            "`Dark` and `Light` reported the same value {:?}. The value must be the setting the \
             user wrote, so a hardcoded string here means the message misquotes one of the two \
             configs back at its author.",
            values[0]
        );
    }

    /// `macos_option_as_alt` is never reported, however the config sets it.
    ///
    /// D3, and the "helpful" addition it rejects: one `wolf-config.yaml` is shared
    /// across both platforms, and the key's own name says which platform it is for,
    /// so a Linux warning would fire on every launch for a setting that is correct
    /// where its author wrote it. Both halves matter — the `Auto` half proves the
    /// flag alone produces nothing, and the `Dark` half proves it does not sneak in
    /// alongside a row that IS produced.
    #[test]
    fn macos_option_as_alt_is_never_reported_as_unsupported() {
        let rows = unsupported_settings(&cfg_with(Appearance::Auto, true));
        assert!(
            rows.is_empty(),
            "`macos-option-as-alt: true` with a supported appearance must produce NO row at all, \
             got {:?}. A row here is the exact change D3 rejects: the setting is correct on the \
             macOS host that shares this file, so the warning would fire on every Linux launch \
             forever and could not be silenced without breaking the other platform.",
            keys(&rows)
        );

        let mixed = unsupported_settings(&cfg_with(Appearance::Dark, true));
        assert_eq!(
            mixed.len(),
            1,
            "with a forced appearance AND `macos-option-as-alt: true`, only the appearance is \
             unsupported, so exactly one row is expected; got {:?}.",
            keys(&mixed)
        );
        let key = mixed[0].key;
        assert_ne!(
            key, OPTION_AS_ALT_KEY,
            "the single reported row is keyed {OPTION_AS_ALT_KEY:?}, so the appearance row was \
             replaced by an option-as-alt row rather than joined by one — the forced appearance is \
             silently dropped as well."
        );
        assert!(
            !key.contains("option"),
            "the reported row is keyed {key:?}, which names the option-as-alt setting under some \
             other spelling. The rule is that this setting is never reported, not that one \
             particular string is never used."
        );
    }
}
