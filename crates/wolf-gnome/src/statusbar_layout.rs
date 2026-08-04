// PRODUCTION CODE GOES ABOVE THIS LINE — written by the coder per plan §3.2

#[cfg(test)]
mod tests {
    use super::*;
    use wolf_core::Rgb;

    /// The five fields of a [`StatusbarLayout`] in declaration order — `(height,
    /// primary_line_top, primary_line_height, secondary_line_top,
    /// secondary_line_height)` — so a table row states its expectation as one tuple
    /// and a failure prints all five at once instead of stopping at the first field
    /// that differs. Which field moved is the whole diagnosis here; see the table's
    /// doc comment for the two edits that share a height and differ only in
    /// `primary_line_height`.
    fn fields(layout: StatusbarLayout) -> (f64, f64, f64, f64, f64) {
        (
            layout.height,
            layout.primary_line_top,
            layout.primary_line_height,
            layout.secondary_line_top,
            layout.secondary_line_height,
        )
    }

    /// One [`statusbar_layout`] table row.
    ///
    /// Every expected value is a LITERAL and every row carries its arithmetic beside
    /// it. Nothing is computed from [`STATUSBAR_LINE_FACTOR`] or from any other
    /// production constant: an expectation derived from the constant the production
    /// code reads holds for whatever value that constant takes.
    struct LayoutCase {
        desc: &'static str,
        primary: f64,
        secondary: f64,
        pad: f64,
        gap: f64,
        expected: (f64, f64, f64, f64, f64),
    }

    /// Pins [`statusbar_layout`]'s vertical arithmetic, which is
    /// `WolfKeyMap/StatusbarLayout.swift`'s verbatim: each line box is
    /// `ceil(font_size * 1.25)`, the primary sits at `vertical_padding`, the
    /// secondary at `vertical_padding + primary + line_gap`, and the height closes
    /// with a second `vertical_padding`.
    ///
    /// Row 1 is `StatusbarLayoutTests.swift`'s own worked example, case for case, so
    /// the two files can be read side by side — the convention
    /// `cell_metrics_math.rs` already follows for `CellMetricsMathTests.swift`.
    ///
    /// FALSIFYING EDITS. Each carries a PREDICTED outcome and a `Red run: owed.` —
    /// the verbatim failure is the coder's to record from an actual red run, since
    /// nothing here has been executed.
    ///
    /// - `round` instead of `ceil`: ROW 1 is the discriminator, not row 4.
    ///   `13 * 1.25 = 16.25` rounds to 16 and ceils to 17, so row 1 gives
    ///   `(36, 3, 16, 20, 13)`. Row 4's `10 * 1.25 = 12.5` is a TIE, and Rust's
    ///   `f64::round` is half-AWAY-from-zero, so `round` and `ceil` both answer 13
    ///   there and row 4 stays green under this edit. Red run: owed.
    /// - `floor` or `trunc` instead of `ceil`: row 1 gives `(35, 3, 16, 20, 12)` —
    ///   both line boxes shrink, unlike the `round` edit, which shrinks only the
    ///   primary — and row 4 moves too, to `(24, 0, 12, 12, 12)`. Red run: owed.
    /// - `line_gap` omitted from `secondary_line_top`: row 1 gives sTop 20 and
    ///   height 36. That is the same PAIR the `round` edit produces, and the two are
    ///   told apart by `primary_line_height` — 17 here, 16 there — which is why the
    ///   assertion compares all five fields rather than the height alone.
    ///   Red run: owed.
    /// - `vertical_padding` counted once instead of twice: row 1 gives height 34.
    ///   Red run: owed.
    /// - [`STATUSBAR_LINE_FACTOR`] changed at all: row 1's `primary_line_height`
    ///   moves off 17. Red run: owed.
    /// - Delete the non-finite / negative guard: rows 6, 7 and 8 produce NaN,
    ///   infinite or negative fields. Red run: owed.
    ///
    /// The guard's own reason, so it is not tidied away as defensive noise: Swift has
    /// none, and an infinite strip height there makes `editor_inset::grid_rect`
    /// return a zero grid and `grid_size_for` a 1x1 one — an editor that paints one
    /// cell. Collapsing to a zero-height strip instead leaves a pane that is all
    /// padding and still perfectly usable.
    #[test]
    fn statusbar_layout_table() {
        let cases = [
            LayoutCase {
                // pH = ceil(13 * 1.25) = ceil(16.25) = 17;
                // sH = ceil(10 * 1.25) = ceil(12.5)  = 13;
                // pTop = 3; sTop = 3 + 17 + 1 = 21; h = 21 + 13 + 3 = 37.
                desc: "the macOS worked example, case for case",
                primary: 13.0,
                secondary: 10.0,
                pad: 3.0,
                gap: 1.0,
                expected: (37.0, 3.0, 17.0, 21.0, 13.0),
            },
            LayoutCase {
                // pH = ceil(28 * 1.25) = 35; sH = ceil(22.4 * 1.25) = ceil(28) = 28;
                // sTop = 3 + 35 + 1 = 39; h = 39 + 28 + 3 = 70.
                // A realistic Linux scale: the strip is called in DEVICE PIXELS here
                // (see the module doc), so these are the sizes a 22-point-ish font
                // reaches it as.
                desc: "a realistic Linux scale in device pixels",
                primary: 28.0,
                secondary: 22.4,
                pad: 3.0,
                gap: 1.0,
                expected: (70.0, 3.0, 35.0, 39.0, 28.0),
            },
            LayoutCase {
                // pH = ceil(8 * 1.25) = ceil(10) = 10 — an EXACT product, which must
                // not grow; sH = ceil(4 * 1.25) = ceil(5) = 5;
                // sTop = 2 + 10 + 0 = 12; h = 12 + 5 + 2 = 19.
                desc: "an exact product does not grow",
                primary: 8.0,
                secondary: 4.0,
                pad: 2.0,
                gap: 0.0,
                expected: (19.0, 2.0, 10.0, 12.0, 5.0),
            },
            LayoutCase {
                // pH = sH = ceil(10 * 1.25) = ceil(12.5) = 13; sTop = 0 + 13 + 0 = 13;
                // h = 13 + 13 + 0 = 26. NOTE: 12.5 is a rounding TIE, so this row
                // does NOT tell `ceil` from Rust's half-away-from-zero `round`; row 1
                // is what does.
                desc: "a half-pixel product ceils (and would also round) up",
                primary: 10.0,
                secondary: 10.0,
                pad: 0.0,
                gap: 0.0,
                expected: (26.0, 0.0, 13.0, 13.0, 13.0),
            },
            LayoutCase {
                // pH = sH = ceil(16 * 1.25) = 20; sTop = 0 + 20 + 0 = 20; h = 40.
                // With no padding and no gap the two line boxes are flush, which is
                // the arm that proves neither term is added unconditionally.
                desc: "zero padding and gap put the lines flush",
                primary: 16.0,
                secondary: 16.0,
                pad: 0.0,
                gap: 0.0,
                expected: (40.0, 0.0, 20.0, 20.0, 20.0),
            },
            // --- Degenerate inputs collapse the WHOLE result --------------------
            LayoutCase {
                desc: "a NaN primary font size collapses the whole result",
                primary: f64::NAN,
                secondary: 10.0,
                pad: 3.0,
                gap: 1.0,
                expected: (0.0, 0.0, 0.0, 0.0, 0.0),
            },
            LayoutCase {
                desc: "an infinite secondary font size collapses the whole result",
                primary: 13.0,
                secondary: f64::INFINITY,
                pad: 3.0,
                gap: 1.0,
                expected: (0.0, 0.0, 0.0, 0.0, 0.0),
            },
            LayoutCase {
                desc: "a negative vertical padding collapses the whole result",
                primary: 13.0,
                secondary: 10.0,
                pad: -3.0,
                gap: 1.0,
                expected: (0.0, 0.0, 0.0, 0.0, 0.0),
            },
        ];

        for case in &cases {
            let got = statusbar_layout(case.primary, case.secondary, case.pad, case.gap);
            assert_eq!(
                fields(got),
                case.expected,
                "case: {} — statusbar_layout({}, {}, {}, {}) as (height, \
                 primary_line_top, primary_line_height, secondary_line_top, \
                 secondary_line_height). Read `primary_line_height` first: a 16 where \
                 17 is expected means `ceil` became `round`, while a 17 with a short \
                 `secondary_line_top` means the line gap went missing — the two edits \
                 produce the same height and are told apart by that field alone.",
                case.desc,
                case.primary,
                case.secondary,
                case.pad,
                case.gap
            );
        }
    }

    /// Tolerance for a font size this module COMPUTED rather than copied, in the
    /// caller's unit. The arithmetic is one multiply against a fraction that is not
    /// exactly representable, so a tight epsilon is right — the same shape
    /// `geometry.rs`'s `approx_fraction` uses.
    const FONT_EPS: f64 = 1e-9;

    /// Whether two font sizes agree within [`FONT_EPS`].
    fn approx(got: f64, want: f64) -> bool {
        (got - want).abs() < FONT_EPS
    }

    /// One [`secondary_font_size`] row.
    struct SecondaryCase {
        desc: &'static str,
        primary: f64,
        expected: f64,
    }

    /// Pins [`secondary_font_size`]: the primary scaled by 0.8, floored at 9.
    ///
    /// CONTRACT THE PRODUCTION FUNCTION OWES, stated here because rows 6 and 7 are
    /// what enforce it: **any non-finite input returns the floor**, not just a NaN
    /// one. That is one `is_finite()` check on the implementation side. `f64::max`
    /// alone gives it for free on the NaN path — it discards a NaN operand — but
    /// answers `+inf` for an infinite primary, which would then reach the strip's
    /// `pango::FontDescription` and [`statusbar_layout`], where it collapses the
    /// whole strip to a zero-height band. Making the seam total instead is strictly
    /// more defensive, in exactly the shape `cell_metrics_math.rs`'s two documented
    /// divergences from Swift already establish for this crate.
    ///
    /// FALSIFYING EDITS. Each carries a PREDICTED outcome and a `Red run: owed.` —
    /// the verbatim failure is the coder's to record from an actual red run, since
    /// nothing here has been executed.
    ///
    /// - Drop the floor: row 3 gives 4 instead of 9, i.e. a path line small enough
    ///   that no OCR and few humans could read it. Red run: owed.
    /// - Use `min` where `max` belongs: rows 1, 2 and 5 collapse to 9 and row 3 falls
    ///   to 4; the loop reports row 1 first. Red run: owed.
    /// - Change the scale: rows 1 and 2 move (they are two different multiples, so a
    ///   changed factor cannot satisfy both by coincidence). Red run: owed.
    /// - Drop the `is_finite()` check and lean on `f64::max` alone: row 7 gives
    ///   `+inf`. Row 6 stays GREEN under that edit, which is why the two rows are not
    ///   redundant — row 7 is the guard's only discriminator. Red run: owed.
    ///
    /// ONE NON-CLAIM, so nobody reads more into row 6 than it holds: it does not on
    /// its own prove an explicit non-finite guard, because a bare
    /// `MIN.max(primary * SCALE)` satisfies it. What row 6 proves alone is that SOME
    /// floor exists on the NaN path, which a plain `primary * SCALE` would fail; the
    /// guard itself is row 7's to catch.
    #[test]
    fn secondary_font_size_table() {
        let cases = [
            SecondaryCase {
                // 20 * 0.8 = 16, comfortably above the floor.
                desc: "the scale applies above the floor",
                primary: 20.0,
                expected: 16.0,
            },
            SecondaryCase {
                // 30 * 0.8 = 24. A second multiple, so a changed scale cannot pass
                // both this row and the one above it.
                desc: "a bigger font scales too",
                primary: 30.0,
                expected: 24.0,
            },
            SecondaryCase {
                // 5 * 0.8 = 4, which the floor raises to 9.
                desc: "the floor wins for a small font",
                primary: 5.0,
                expected: 9.0,
            },
            SecondaryCase {
                // 11.25 * 0.8 = 9 exactly: the crossover, where the two arms agree
                // and neither can be told from the other.
                desc: "exactly at the crossover",
                primary: 11.25,
                expected: 9.0,
            },
            SecondaryCase {
                // 12.5 * 0.8 = 10, one step above the crossover — the row that
                // proves the scale is still applied just past the floor.
                desc: "just above the crossover",
                primary: 12.5,
                expected: 10.0,
            },
            SecondaryCase {
                desc: "a NaN primary yields the floor",
                primary: f64::NAN,
                expected: 9.0,
            },
            SecondaryCase {
                // `+inf * 0.8` is `+inf`, and `9f64.max(f64::INFINITY)` is `+inf` —
                // so this row is red for any implementation that does not check
                // `is_finite()` explicitly. See the CONTRACT paragraph above: an
                // infinity escaping here reaches the strip's font description and
                // `statusbar_layout`, which collapses the strip entirely.
                desc: "an infinite primary yields the floor too",
                primary: f64::INFINITY,
                expected: 9.0,
            },
        ];

        for case in &cases {
            let got = secondary_font_size(case.primary);
            assert!(
                approx(got, case.expected),
                "case: {} — secondary_font_size({}) = {got}, want {} (within \
                 {FONT_EPS}). A 4 where 9 is expected means the floor is gone; a 9 \
                 where a larger number is expected means `max` became `min`; an `inf` \
                 means the `is_finite()` guard is missing and `f64::max` alone let the \
                 infinity through, which is the one case a NaN row cannot catch.",
                case.desc,
                case.primary,
                case.expected
            );
        }
    }

    /// Builds an [`Rgb`] from three channels, so a table row reads as a colour rather
    /// than as a struct literal repeated four times.
    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    /// One [`statusbar_text`] row.
    struct TextCase {
        desc: &'static str,
        fg: Rgb,
        bg: Rgb,
        active: bool,
        expected: StatusbarText,
    }

    /// Pins [`statusbar_text`]: each channel is `round(bg + (fg - bg) * alpha)` with
    /// `alpha = 1.0` for an active pane's primary line and `0.5` for an inactive
    /// one's, times a further `0.7` for the secondary line — and the primary line is
    /// bold exactly when the pane is active.
    ///
    /// Every expected triple below was derived from that rule and the arithmetic is
    /// written out beside each row, because the blend toward `bg` is the half a
    /// reader is most likely to "simplify" into a composite over black.
    ///
    /// The blend is toward the BACKGROUND rather than toward black, and that is
    /// load-bearing rather than stylistic: the strip's background is opaque and known
    /// (`draw_pane` has just filled it), so blending toward it produces exactly the
    /// pixels alpha compositing would — without `set_source_rgba` and without
    /// touching `render::draw_glyph`. Rows 3 and 4 are the only rows that can tell
    /// the two apart, because rows 1 and 2 sit on a black background where they
    /// coincide.
    ///
    /// FALSIFYING EDITS. Each carries a PREDICTED outcome and a `Red run: owed.` —
    /// the verbatim failure is the coder's to record from an actual red run, since
    /// nothing here has been executed.
    ///
    /// - Delete the `active` term from `primary_alpha`: row 2's primary becomes
    ///   (255, 255, 255). Red run: owed.
    /// - Hardcode `primary_bold`: row 1 or row 2 fires, whichever way it is pinned.
    ///   Red run: owed.
    /// - Apply the secondary alpha to the primary line too: row 1's primary becomes
    ///   (179, 179, 179). Red run: owed.
    /// - Composite over black (`fg * alpha`) instead of blending toward `bg`: row 3
    ///   gives (100, 50, 0) and (70, 35, 0), and row 4's secondary gives
    ///   (140, 70, 0). Red run: owed.
    /// - Scale the secondary alpha by the inactive factor twice
    ///   (`0.7 * 0.5 * 0.5`): row 2's secondary moves off (89, 89, 89).
    ///   Red run: owed.
    ///
    /// TWO NON-CLAIMS:
    ///
    /// 1. The `[0, 255]` clamp is NOT falsifiable here. Both endpoints are `u8` and
    ///    every alpha is in `[0, 1]`, so the blend is bounded by `min(fg, bg)` and
    ///    `max(fg, bg)` by construction and the clamp never engages. It is
    ///    defensive, and deleting it would leave this table green.
    /// 2. Row 1's secondary sits on a rounding TIE: `255 * 0.7` is exactly `178.5`
    ///    in IEEE double (the real product is under a half-ulp below it), and
    ///    `f64::round` takes a tie away from zero, so the answer is 179. A 178 here
    ///    means the multiplication was reassociated — the value, not the rounding
    ///    mode, is what changed.
    #[test]
    fn statusbar_text_table() {
        let cases = [
            TextCase {
                // active => primary alpha 1.0: 0 + (255 - 0) * 1.0 = 255.
                // secondary alpha 0.7 * 1.0 = 0.7: 0 + 255 * 0.7 = 178.5, and
                // round-half-away-from-zero gives 179.
                desc: "an active pane is full strength and bold",
                fg: rgb(255, 255, 255),
                bg: rgb(0, 0, 0),
                active: true,
                expected: StatusbarText {
                    primary: rgb(255, 255, 255),
                    secondary: rgb(179, 179, 179),
                    primary_bold: true,
                },
            },
            TextCase {
                // inactive => primary alpha 0.5: 0 + 255 * 0.5 = 127.5 -> 128.
                // secondary alpha 0.7 * 0.5 = 0.35: 0 + 255 * 0.35 = 89.25 -> 89.
                desc: "an inactive pane dims both lines and is not bold",
                fg: rgb(255, 255, 255),
                bg: rgb(0, 0, 0),
                active: false,
                expected: StatusbarText {
                    primary: rgb(128, 128, 128),
                    secondary: rgb(89, 89, 89),
                    primary_bold: false,
                },
            },
            TextCase {
                // inactive over a GREY background, so the blend's direction shows.
                // primary, alpha 0.5:
                //   r: 40 + (200 - 40) * 0.5 = 40 + 80 = 120
                //   g: 40 + (100 - 40) * 0.5 = 40 + 30 = 70
                //   b: 40 + (  0 - 40) * 0.5 = 40 - 20 = 20
                // secondary, alpha 0.35:
                //   r: 40 + 160 * 0.35 = 40 + 56 = 96
                //   g: 40 +  60 * 0.35 = 40 + 21 = 61
                //   b: 40 -  40 * 0.35 = 40 - 14 = 26
                // A composite over black would give (100, 50, 0) and (70, 35, 0):
                // note the BLUE channel, which rises toward the background here and
                // falls to zero there.
                desc: "the blend is toward the background, not toward black",
                fg: rgb(200, 100, 0),
                bg: rgb(40, 40, 40),
                active: false,
                expected: StatusbarText {
                    primary: rgb(120, 70, 20),
                    secondary: rgb(96, 61, 26),
                    primary_bold: false,
                },
            },
            TextCase {
                // active over the same grey background.
                // primary, alpha 1.0: bg + (fg - bg) * 1 == fg exactly.
                // secondary, alpha 0.7:
                //   r: 40 + 160 * 0.7 = 40 + 112 = 152
                //   g: 40 +  60 * 0.7 = 40 +  42 =  82
                //   b: 40 -  40 * 0.7 = 40 -  28 =  12
                // The blue channel is 12, NOT 28: 28 is what `40 * 0.7` is, and the
                // channel is `40 - 28`. Derived here rather than copied.
                desc: "an active pane over a non-black background is untouched",
                fg: rgb(200, 100, 0),
                bg: rgb(40, 40, 40),
                active: true,
                expected: StatusbarText {
                    primary: rgb(200, 100, 0),
                    secondary: rgb(152, 82, 12),
                    primary_bold: true,
                },
            },
        ];

        for case in &cases {
            let got = statusbar_text(case.fg, case.bg, case.active);
            assert_eq!(
                got, case.expected,
                "case: {} — statusbar_text({:?}, {:?}, active = {}). The primary line \
                 is the editor foreground at full strength when the pane is active \
                 and at half when it is not; the secondary is a further 0.7 of that; \
                 both blend toward the BACKGROUND, so a channel that fell to 0 where \
                 the background is grey means the rule became a composite over black.",
                case.desc, case.fg, case.bg, case.active
            );
        }
    }
}
