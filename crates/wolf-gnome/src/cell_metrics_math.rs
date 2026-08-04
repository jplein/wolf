//! The pure cell-metrics arithmetic the configured `line-height` drives — a Rust
//! port of `WolfKeyMap/CellMetricsMath.swift`, with no GTK, Pango or Cairo.
//!
//! Both functions take plain `f64`s, which is what lets the whole rule run in the
//! fast unit suite: [`crate::render::CellMetrics::for_font`] needs a live Cairo
//! context to obtain the metrics it feeds in, and a rule left inside `for_font`
//! would be a rule only the golden-render target could execute.
//!
//! ## Units: the caller picks, and the two platforms pick differently
//!
//! The arithmetic is unit-agnostic — it rounds in whatever unit its caller uses.
//! macOS calls it in POINTS, where 1 pt is 1 px at 1x. Linux calls it in DEVICE
//! PIXELS, because that is the unit Cairo paints in, so `for_font` converts the
//! configured point size to device pixels before calling. Both platforms therefore
//! snap the cell to a whole device pixel and a shared `line-height: 130` gives the
//! same visual ratio; only the sub-pixel snap can differ, and only above 1x.
//!
//! ## Two deliberate divergences from Swift
//!
//! Both are strictly more defensive, and both are confined to inputs macOS also
//! mishandles, so no in-range result changes:
//!
//! - [`cell_height`] additionally requires the percentage to be FINITE. Swift's
//!   `guard lineHeightPercent > 0` accepts `+inf` and yields an infinite cell.
//! - [`cell_height`] floors a degenerate result at [`MIN_CELL_PX`]. Swift can
//!   return `0` or `NaN`, either of which makes `geometry::grid_size_for`'s
//!   division silently produce an unusable row count and the window paint nothing.

/// The floor a degenerate [`cell_height`] result collapses to: one whole device
/// unit.
///
/// Not zero. A zero cell height satisfies every `> 0.0` check downstream while
/// still dividing a pane's height into an unbounded number of rows.
pub const MIN_CELL_PX: f64 = 1.0;

/// The cell height, in the caller's unit (device pixels on this front-end).
///
/// For a positive, finite `line_height_percent` the height is
/// `round(font_size * line_height_percent / 100)` — driven by the font size alone
/// and INDEPENDENT of the raw font metrics, so the cell does not change size when
/// fontconfig substitutes a different face. `round` is half-away-from-zero,
/// matching Swift's bare `.rounded()`.
///
/// Otherwise it falls back to the font's natural line height,
/// `ceil(ascent + descent + leading)`.
///
/// A result that is not finite and `> 0.0` becomes [`MIN_CELL_PX`].
#[must_use]
pub fn cell_height(
    font_size: f64,
    line_height_percent: f64,
    ascent: f64,
    descent: f64,
    leading: f64,
) -> f64 {
    let height = if line_height_percent.is_finite() && line_height_percent > 0.0 {
        (font_size * line_height_percent / 100.0).round()
    } else {
        (ascent + descent + leading).ceil()
    };
    if height.is_finite() && height > 0.0 {
        height
    } else {
        MIN_CELL_PX
    }
}

/// The text baseline offset from the cell's TOP, in the caller's unit.
///
/// The leftover space (`cell_height - ascent - descent`) is split evenly above and
/// below the glyph box, so a cell taller than the glyphs centres them rather than
/// hanging them from the top.
///
/// When the cell is exactly the glyph box the leftover is zero and the baseline IS
/// the ascent — which is why `render::draw_glyph`'s `baseline - ascent` offset can
/// be exactly `0.0` and move no glyph at all.
#[must_use]
pub fn baseline(cell_height: f64, ascent: f64, descent: f64) -> f64 {
    ascent + (cell_height - ascent - descent) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The degenerate floor, re-typed here as a LITERAL rather than read from
    /// [`MIN_CELL_PX`].
    ///
    /// A test that asserted `cell_height(…) == MIN_CELL_PX` would hold for any
    /// value the constant took, including `0.0` — which is precisely the outcome
    /// the floor exists to prevent. Stating `1.0` independently means a change to
    /// the constant fails here instead of moving with it.
    const EXPECT_MIN_CELL_PX: f64 = 1.0;

    /// A positive line-height makes the cell a function of the FONT SIZE ALONE.
    ///
    /// The raw ascent/descent/leading are ignored on this path, so the same call
    /// with wildly different metrics must return the same number. Without that
    /// second half the case would also pass against an implementation that
    /// returned `ascent + descent`, since the fixture's glyph box happens to be
    /// 10 units tall.
    ///
    /// The 14 pt / 130 % row is `CellMetricsMathTests.swift`'s
    /// `testCellHeightAt130PercentRoundsFontScaledHeight`, and the 16 pt / 100 %
    /// row is `testCellHeightAt100PercentEqualsRoundedFontSize`, so the two files
    /// can be read side by side.
    #[test]
    fn a_positive_line_height_drives_cell_height_from_the_font_size_alone() {
        assert_eq!(
            cell_height(10.0, 130.0, 8.0, 2.0, 0.0),
            13.0,
            "round(10 * 130 / 100) == round(13.0) == 13. A different answer means the positive \
             path is not `round(font_size * percent / 100)`, so a shared `line-height: 130` stops \
             producing the same ratio it produces on macOS."
        );

        // Same font size, same percentage, absurd metrics: the answer must not move.
        assert_eq!(
            cell_height(10.0, 130.0, 40.0, 17.0, 9.0),
            13.0,
            "the positive path must IGNORE ascent/descent/leading entirely. A result of 66 here \
             (`ceil(40 + 17 + 9)`) means the guard is inverted; anything else that tracks the \
             metrics means the font's natural line height leaked into the configured one, and the \
             cell would then change size when the substituted face changes."
        );

        assert_eq!(
            cell_height(14.0, 130.0, 11.0, 3.0, 0.0),
            18.0,
            "round(14 * 130 / 100) == round(18.2) == 18 — the macOS case-for-case counterpart \
             (`testCellHeightAt130PercentRoundsFontScaledHeight`). A divergence here means the two \
             front-ends do not lay out a shared config identically."
        );

        assert_eq!(
            cell_height(16.0, 100.0, 12.0, 4.0, 1.0),
            16.0,
            "round(16 * 100 / 100) == 16 — at 100 % the cell is exactly the rounded font size, \
             even though the glyph box plus leading is 17."
        );
    }

    /// The cell height is rounded to a whole unit, HALF AWAY FROM ZERO.
    ///
    /// `10 * 125 / 100` is exactly 12.5. Rust's `f64::round` and Swift's bare
    /// `.rounded()` both take 13; `trunc`, `floor` and a banker's rounding all take
    /// 12. A cell that is one unit short of macOS's is a whole row of divergence
    /// over a tall window, which is why the tie case is pinned rather than left to
    /// whichever rounding mode a later edit reaches for.
    #[test]
    fn cell_height_rounds_to_a_whole_unit() {
        assert_eq!(
            cell_height(10.0, 125.0, 8.0, 2.0, 0.0),
            13.0,
            "10 * 125 / 100 == 12.5, which must round AWAY FROM ZERO to 13. A 12 here means \
             `round()` was replaced by `trunc()`/`floor()` or by a to-even rounding, and the Linux \
             cell would then be one device pixel shorter than the macOS one for every tie."
        );
        assert_eq!(
            cell_height(10.0, 124.0, 8.0, 2.0, 0.0),
            12.0,
            "10 * 124 / 100 == 12.4 rounds DOWN to 12. This is the control for the tie case above: \
             without it an implementation that simply took `ceil` would satisfy the 12.5 row."
        );
    }

    /// A non-positive line-height falls back to the font's NATURAL line height,
    /// `ceil(ascent + descent + leading)`.
    ///
    /// The fixture uses a synthetic `leading` of 1.5 so the `+ leading` term and
    /// the `ceil` are both load-bearing: `ceil(8 + 2 + 1.5) == 12`, while dropping
    /// the leading gives 10 and truncating gives 11. Its sibling
    /// [`with_zero_leading_the_natural_line_height_is_the_glyph_box`] pins the
    /// other arm of that term, which is the one this repo's pinned font actually
    /// takes.
    #[test]
    fn a_non_positive_line_height_falls_back_to_the_natural_line_height() {
        for percent in [0.0, -50.0] {
            assert_eq!(
                cell_height(10.0, percent, 8.0, 2.0, 1.5),
                12.0,
                "`line-height: {percent}` must take the natural-metrics branch, \
                 ceil(8 + 2 + 1.5) == 12. A 13 means the positive formula ran anyway \
                 (round(10 * {percent} / 100) is 0 or negative, so the floor would then have to \
                 rescue it); a 10 means the `+ leading` term was dropped; an 11 means `ceil` \
                 became `round` or `trunc`."
            );
        }
    }

    /// With ZERO leading the natural line height is the glyph box itself.
    ///
    /// A zero `leading` is what `render::for_font` hands in for the pinned DejaVu
    /// Sans Mono at both sizes this repo measures — but for two DIFFERENT reasons,
    /// and only one of them is equality. Measured on this host under the devShell's
    /// `FONTCONFIG_FILE`:
    ///
    /// | size | ascent | descent | `metrics.height()/SCALE` | `height - ascent - descent` |
    /// | --- | --- | --- | --- | --- |
    /// | 28 pt (the smokes) | 35 | 9 | 44 | `0` |
    /// | 16 pt (the golden fixture) | 20 | 6 | 25 | `-1` |
    ///
    /// At 28 pt the recommended height IS the glyph box, so `for_font`'s `.max(0.0)`
    /// is a no-op. At 16 pt the recommended height is one unit UNDER the glyph box,
    /// and the clamp alone is what makes `leading` `0.0`: without it this branch
    /// would give the 16 pt fixture a natural cell of 25 rather than 26.
    /// `render.rs`'s own comment on the clamp — "a face whose recommended height is
    /// under its own glyph box would otherwise make the natural line height shrink"
    /// — therefore describes a live case on this repo's pinned font, not a
    /// hypothetical one. The clamp is load-bearing; do not delete it.
    ///
    /// The 35/9/44 numbers below are the measured 28 pt ascent, descent and cell
    /// height, so the case reads as the thing it stands in for.
    ///
    /// NOTHING SHIPPED REACHES THIS BRANCH TODAY. Every `line_height_percent` that
    /// arrives at `for_font` is positive: `wolf_core::default_config()`'s 130, the
    /// smokes' 118, the golden fixture's 130/100/160. `load_or_create` does not
    /// validate the key, so a hand-written `line-height: 0` in a user's
    /// `wolf-config.yaml` would land here — that is what the branch is for, and this
    /// test and its sibling are the only callers exercising it.
    ///
    /// It also discriminates the guard: with the `line_height_percent <= 0` arm
    /// deleted, `round(37 * 0 / 100)` is `0`, which the final floor turns into
    /// [`EXPECT_MIN_CELL_PX`] — nothing like 44.
    #[test]
    fn with_zero_leading_the_natural_line_height_is_the_glyph_box() {
        for percent in [0.0, -50.0] {
            assert_eq!(
                cell_height(37.0, percent, 35.0, 9.0, 0.0),
                44.0,
                "`line-height: {percent}` with zero leading must give ceil(35 + 9 + 0) == 44, the \
                 measured DejaVu Sans Mono cell at the smoke's font size. A 1 means the guard is \
                 gone and the degenerate floor caught the result; a 46 (round(37 * 125/100)) means \
                 the positive formula ran with some other percentage."
            );
        }
    }

    /// A non-finite line-height takes the natural branch too.
    ///
    /// This is the first of the two deliberate divergences from Swift, whose
    /// `guard lineHeightPercent > 0` accepts `+inf` and yields an infinite cell
    /// height. `NaN > 0.0` is already false in both languages, so only the infinity
    /// row is new — and it is the one an `is_finite()` deletion reddens.
    #[test]
    fn a_non_finite_line_height_falls_back_to_the_natural_line_height() {
        for percent in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let got = cell_height(10.0, percent, 8.0, 2.0, 1.5);
            assert_eq!(
                got, 12.0,
                "a non-finite `line-height` ({percent}) must take the natural-metrics branch, \
                 ceil(8 + 2 + 1.5) == 12, but this returned {got}. An infinite or NaN result means \
                 the `is_finite()` half of the guard was dropped, and `for_font` would then hand \
                 the pane view a non-finite cell height — every grid-size division becomes NaN and \
                 the window paints nothing."
            );
        }
    }

    /// A degenerate result is floored at one whole device unit, never zero and
    /// never non-finite.
    ///
    /// The second deliberate divergence from Swift, which can return `0` or `NaN`
    /// here. A zero cell height makes `grid_size_for`'s division produce an
    /// infinite row count; a NaN one makes it produce no rows at all. Both are
    /// silent — the window simply stops drawing — so the floor is asserted from
    /// both the zero side and the NaN side.
    #[test]
    fn cell_height_is_never_zero_or_non_finite() {
        assert_eq!(
            MIN_CELL_PX, EXPECT_MIN_CELL_PX,
            "MIN_CELL_PX must be {EXPECT_MIN_CELL_PX} — one whole device unit. A smaller floor \
             (0.0 especially) satisfies every `> 0.0` check downstream while still dividing the \
             pane height into an unbounded number of rows."
        );

        let cases: [(&str, f64); 4] = [
            ("all zeros", cell_height(0.0, 0.0, 0.0, 0.0, 0.0)),
            ("NaN metrics", cell_height(10.0, 0.0, f64::NAN, 0.0, 0.0)),
            ("NaN font size", cell_height(f64::NAN, 130.0, 8.0, 2.0, 0.0)),
            ("negative metrics", cell_height(10.0, 0.0, -8.0, -2.0, 0.0)),
        ];
        for (what, got) in cases {
            assert_eq!(
                got, EXPECT_MIN_CELL_PX,
                "the {what} case returned {got}, not the {EXPECT_MIN_CELL_PX} floor. A 0 means the \
                 final floor was deleted and `grid_size_for` will divide by zero; a NaN means it \
                 was deleted and every later comparison against the cell height is false, so the \
                 pane view reports zero rows and paints an empty window with no error anywhere."
            );
        }
    }

    /// The baseline splits the leftover space evenly above and below the glyph box.
    ///
    /// Case-for-case with `CellMetricsMathTests.swift`'s
    /// `testBaselineSplitsExtraLeadingEvenly` and
    /// `testBaselineWithLargerCellCentersGlyphBox`.
    #[test]
    fn the_baseline_splits_the_leftover_space_evenly() {
        assert_eq!(
            baseline(18.0, 12.0, 4.0),
            13.0,
            "cell 18, glyph box 12 + 4 == 16, so 2 units of leftover split 1 above and 1 below: \
             baseline == 12 + 1 == 13. A 12 means the leftover term was dropped and the glyph sits \
             flush against the cell top; a 14 means the whole leftover went above it."
        );
        assert_eq!(
            baseline(20.0, 12.0, 4.0),
            14.0,
            "cell 20, glyph box 16, so 4 units of leftover split 2 and 2: baseline == 14. This \
             second ratio is what stops a divisor change (`/ 4`, say) passing the 18/16 case by \
             coincidence."
        );
    }

    /// PRECONDITION: a cell exactly as tall as the glyph box leaves the baseline
    /// at the ascent.
    ///
    /// This pins a premise, not a rule. Both rows below give the cell the glyph
    /// box's own height (`16 == 12 + 4`, `44 == 35 + 9`), so the leftover space is
    /// `0.0` and the offset `render::draw_glyph` applies, `baseline - ascent`, is
    /// exactly `0.0` — it moves no glyph by a single device pixel.
    ///
    /// That zero is what the compositor smokes rest on. `smoke_headless.rs` pins
    /// `SMOKE_LINE_HEIGHT = 118`, and its doc comment carries the measurement
    /// behind the number: at the smoke's font size the app's cell height is
    /// `round(37.3333 * 1.18) == 44`, which is exactly `ascent + descent` for the
    /// pinned DejaVu Sans Mono (35 + 9). Every glyph therefore paints at the
    /// coordinates a top-aligned layout puts it at, and every OCR and pointer
    /// target calibrated against those coordinates holds unchanged. A cell height
    /// that stopped equalling the glyph box would move all of them at once, so the
    /// premise is asserted here rather than assumed.
    ///
    /// It gives NO coverage of the centring rule itself, and cannot: with a
    /// leftover of `0.0` the centring term contributes nothing under any divisor,
    /// and a `baseline` that returned the bare `ascent` satisfies both rows.
    /// [`the_baseline_splits_the_leftover_space_evenly`] is the test that covers
    /// the rule, with two ratios so a changed divisor cannot pass by coincidence.
    #[test]
    fn precondition_a_cell_that_matches_the_glyph_box_leaves_the_baseline_at_the_ascent() {
        assert_eq!(
            baseline(16.0, 12.0, 4.0),
            12.0,
            "with no leftover space the baseline must be the ascent itself. A non-zero \
             `baseline - ascent` here would shift every glyph in every pane by that amount, which \
             is exactly what the compositor smokes' pixel targets are calibrated against."
        );
        assert_eq!(
            baseline(44.0, 35.0, 9.0),
            35.0,
            "the measured 28 pt DejaVu Sans Mono case (ascent 35, descent 9, cell 44): \
             `baseline - ascent` must be 0.0 so `draw_glyph` paints at byte-identical coordinates \
             to a top-aligned layout."
        );
    }
}
