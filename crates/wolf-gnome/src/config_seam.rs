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

/// The font size used when neither `WOLF_FONT_PT` nor the configured `font-size`
/// is usable, in points.
///
/// The value the front-end ran at before it read a config at all, so a developer
/// with no config file and no override sees what they saw then.
pub const FALLBACK_FONT_PT: f64 = 14.0;

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

#[cfg(test)]
mod tests {
    use super::*;

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
             means the precedence is reversed, which re-sizes the cell in all forty-three \
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
}
