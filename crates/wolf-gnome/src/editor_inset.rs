//! The chrome around a pane's text grid, as arithmetic: the pane-local rectangle
//! the grid paints into, the map from a pane-local point into the grid-local space
//! [`crate::geometry::cell_at`] speaks, and the grid size that survives the chrome.
//!
//! A port of `WolfKeyMap/EditorInset.swift`, named for it the way
//! [`crate::cell_metrics_math`] is named for `CellMetricsMath.swift`. GTK-, Pango-
//! and Cairo-free, so the whole rulebook runs in the fast unit suite.
//!
//! ## The subtraction and the division stay apart
//!
//! macOS keeps `EditorInset` (subtraction) and `PaneGeometry` (division) in
//! separate files and never merges them. So does this: [`crate::geometry`]'s module
//! doc states its composition guarantee over the DIVISION alone, and growing
//! `grid_size_for` into something that also subtracts would force that guarantee to
//! be restated — and would leave `pane_view::resize_global`, which genuinely wants
//! the un-inset division for the global grid, with no seam to call.
//! [`grid_size_for_pane`] is therefore a composition ON TOP of the unchanged
//! `grid_size_for`, and `cell_at_over_grid_local_never_exceeds_grid_size_for_pane`
//! in this module's test block is the inset pipeline's own companion guarantee.
//!
//! ## There is no `StatusbarTextSpan` counterpart, and no `text_left_inset`
//!
//! macOS's `StatusbarTextSpan.swift` exists solely to add `PaneGeometry`'s
//! `paintOffsetX`, the centring inset its 90-column cap produces. This crate has no
//! column cap and no centring, so that offset is identically zero and the whole
//! struct degenerates to `grid_rect(..).x` and `grid_rect(..).w` — two fields the
//! resizer's one call already produced. The painter reads them directly.
//!
//! There is no `text_left_inset` function either: a function returning its own
//! argument names a rule without guaranteeing one, whereas reading `grid_rect(..).x`
//! makes the strip's text and the grid's text share an origin BY CONSTRUCTION. A
//! reader porting `StatusbarTextSpan.swift` would find it has no content here.
//!
//! ## The cmdline strip is deliberately not indented
//!
//! macOS's `EditorInset.textLeftInset` serves the statusbar AND the cmdline,
//! because both are per-pane there. This crate's cmdline is one full-width strip at
//! the WINDOW bottom, outside every pane, so the inset does not reach it.

use crate::geometry;

/// The uniform inset applied around every pane's text grid, in device pixels.
///
/// `EditorInset.swift`'s `padding` verbatim.
pub const EDITOR_PADDING_PX: f64 = 20.0;

/// A pane-local rectangle in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridRect {
    /// The rectangle's left edge, measured from the pane's own left edge.
    pub x: f64,
    /// The rectangle's top edge, measured from the pane's own top edge.
    pub y: f64,
    /// The rectangle's width.
    pub w: f64,
    /// The rectangle's height.
    pub h: f64,
}

/// The all-zero rectangle a non-finite input collapses to.
const COLLAPSED: GridRect = GridRect {
    x: 0.0,
    y: 0.0,
    w: 0.0,
    h: 0.0,
};

/// The pane-local rectangle a pane's text grid paints into, once the statusbar
/// strip along the top and the uniform padding on all four sides are removed.
///
/// `EditorInset.swift`'s rule verbatim: `x = padding`,
/// `y = statusbar_h + padding`, `w = max(0, pane_w - 2 * padding)`,
/// `h = max(0, pane_h - statusbar_h - 2 * padding)`. **Only `w` and `h` clamp;
/// `x` and `y` are fixed offsets**, so a pane too small for its own chrome still
/// reports where the grid WOULD start.
///
/// **Divergence from Swift, deliberate and strictly more defensive:** any
/// non-finite input collapses the WHOLE result to zeros, following
/// [`crate::geometry`]'s whole-result-collapse convention. A NaN `x` origin would
/// otherwise poison [`grid_local`] for every pointer event in the pane, and
/// `cell_at` silently treats a NaN axis as the origin rather than rejecting it.
#[must_use]
pub fn grid_rect(pane_w: f64, pane_h: f64, statusbar_h: f64, padding: f64) -> GridRect {
    if !pane_w.is_finite()
        || !pane_h.is_finite()
        || !statusbar_h.is_finite()
        || !padding.is_finite()
    {
        return COLLAPSED;
    }
    GridRect {
        x: padding,
        y: statusbar_h + padding,
        w: (pane_w - 2.0 * padding).max(0.0),
        h: (pane_h - statusbar_h - 2.0 * padding).max(0.0),
    }
}

/// Maps a PANE-LOCAL point into the GRID-LOCAL space
/// [`crate::geometry::cell_at`] speaks: `(x - rect.x, y - rect.y)`, and nothing
/// else.
///
/// A point in the statusbar strip or in one of the four gutters comes back
/// NEGATIVE, which `cell_at` clamps to the nearest edge cell — exactly macOS's
/// `PaneGeometry.cellAt`, and exactly what makes a click on the chrome name a real
/// cell instead of being rejected.
///
/// No clamping and no non-finite handling happen here on purpose: a NaN stays NaN
/// and an infinity keeps its sign, so `cell_at`'s two DIFFERENT non-finite rules —
/// a NaN has no direction and becomes the origin, an infinity has one and clamps to
/// the far edge — survive the translation. Collapsing either here would break
/// drag-past-the-edge selection.
#[must_use]
pub fn grid_local(x: f64, y: f64, rect: GridRect) -> (f64, f64) {
    (x - rect.x, y - rect.y)
}

/// The Neovim grid size `(cols, rows)` for a pane of `pane_w` x `pane_h` device
/// pixels, once its chrome is removed.
///
/// The one place [`grid_rect`]'s subtraction and
/// [`crate::geometry::grid_size_for`]'s division compose; see the module docs for
/// why they are not one function.
#[must_use]
pub fn grid_size_for_pane(
    pane_w: i32,
    pane_h: i32,
    statusbar_h: f64,
    padding: f64,
    cell_w: f64,
    cell_h: f64,
) -> (u16, u16) {
    let rect = grid_rect(f64::from(pane_w), f64::from(pane_h), statusbar_h, padding);
    geometry::grid_size_for(floor_to_px(rect.w), floor_to_px(rect.h), cell_w, cell_h)
}

/// Floors a rectangle dimension to whole pixels, clamped into `[0, i32::MAX]`.
///
/// FLOOR, never round: half a pixel of leftover strip must not buy a whole row of
/// buffer text. `grid_rect` has already rejected every non-finite input, so the
/// value here is finite.
fn floor_to_px(v: f64) -> i32 {
    // Clamp in f64 space FIRST so the value is provably within i32 range before the
    // cast, exactly as `geometry::floor_div_clamped` does.
    let n = v.floor().clamp(0.0, f64::from(i32::MAX));
    #[allow(
        clippy::cast_possible_truncation,
        reason = "n is clamped to [0.0, i32::MAX] and floored, so the cast is exact and in range"
    )]
    {
        n as i32
    }
}

/// The smallest pane, in device pixels, that still leaves one whole cell of grid:
/// `(ceil(cell_w) + 2 * padding, ceil(cell_h) + statusbar_h + 2 * padding)`.
///
/// This is `pane_view::make_pane_area`'s content floor. The alternative — leaving
/// the floor at one cell and letting a starved pane report a 1x1 grid — reflows
/// that window's whole buffer to a single column, silently.
#[must_use]
pub fn min_pane_size(cell_w: f64, cell_h: f64, statusbar_h: f64, padding: f64) -> (f64, f64) {
    (
        cell_w.ceil() + 2.0 * padding,
        cell_h.ceil() + statusbar_h + 2.0 * padding,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::cell_at;

    /// The "typical" monospace cell every table below runs against, in device
    /// pixels. It is `geometry.rs`'s own 8x16 cell, deliberately, so the two files'
    /// tables can be read side by side and a shared failure is obviously shared.
    const CELL_W: f64 = 8.0;
    /// Height of the typical cell (see [`CELL_W`]).
    const CELL_H: f64 = 16.0;

    /// The four fields of a [`GridRect`] in declaration order, so a table row can
    /// state its expectation as one tuple and a failure prints all four at once
    /// instead of stopping at the first field that differs.
    fn fields(rect: GridRect) -> (f64, f64, f64, f64) {
        (rect.x, rect.y, rect.w, rect.h)
    }

    /// One [`grid_rect`] table row: a description, the four inputs, and the four
    /// expected fields as `(x, y, w, h)`.
    ///
    /// Every expected value is a LITERAL, and every row carries its arithmetic in a
    /// comment beside it. Nothing here is computed from [`EDITOR_PADDING_PX`] or from
    /// any other production constant: an expectation derived from the constant the
    /// production code reads holds for whatever value that constant takes, which is
    /// the shape of an assertion that cannot fail.
    struct RectCase {
        desc: &'static str,
        pane_w: f64,
        pane_h: f64,
        statusbar_h: f64,
        padding: f64,
        expected: (f64, f64, f64, f64),
    }

    /// Pins [`grid_rect`]: `x = padding`, `y = statusbar_h + padding`,
    /// `w = max(0, pane_w - 2 * padding)`,
    /// `h = max(0, pane_h - statusbar_h - 2 * padding)` — with ONLY `w` and `h`
    /// clamping, and any non-finite input collapsing the WHOLE result.
    ///
    /// The rule is `WolfKeyMap/EditorInset.swift`'s verbatim, so the two files can be
    /// compared as values. The whole-result collapse on a non-finite input is this
    /// crate's one deliberate divergence from it, following `geometry.rs`'s
    /// convention: a NaN `x` origin would otherwise poison [`grid_local`] for every
    /// pointer event in the pane.
    ///
    /// Rows 1 and 3 leave an `h` of 612 while rows 5 and 6 leave 592, and that is not
    /// an inconsistency to be tidied: the four rows drive four DIFFERENT pane heights
    /// (682, 682, 632, 622). The height term is `pane_h - statusbar_h - 2 * padding` —
    /// one padding above the grid, under the strip, and one below it — so every row
    /// states its arithmetic inline and a reader checks a row against the RULE rather
    /// than against a neighbouring row.
    ///
    /// FALSIFYING EDITS. Each names the edit, the row it fires on, and the verbatim
    /// failure measured for it.
    ///
    /// - Delete the `max(0, …)` on `w`: row 3 yields `w = -30` instead of `0`
    ///   (`10 - 2 * 20`, as that row's own inline arithmetic states). That row is the
    ///   clamp's ONLY discriminator anywhere — see [`grid_size_for_pane_table`]'s
    ///   non-claim, and do not delete it as redundant. MEASURED: `case: narrower than
    ///   the chrome clamps w only — grid_rect(10, 682, 30, 20)`, left
    ///   `(20.0, 50.0, -30.0, 612.0)`, right `(20.0, 50.0, 0.0, 612.0)`.
    /// - Delete the `max(0, …)` on `h`: row 4 yields `h = -60` instead of `0`.
    ///   MEASURED: `case: shorter than the chrome clamps h only — grid_rect(840, 10,
    ///   30, 20)`, left `(20.0, 50.0, 800.0, -60.0)`, right
    ///   `(20.0, 50.0, 800.0, 0.0)`.
    /// - Swap `y = statusbar_h + padding` for `y = padding`: row 1 yields `y = 20`.
    ///   This is what covers the origins; a `max(0, …)` added to `x`/`y` is
    ///   unobservable, since no row drives either origin negative. MEASURED:
    ///   `case: typical pane — grid_rect(840, 682, 30, 20)`, left
    ///   `(20.0, 20.0, 800.0, 612.0)`, right `(20.0, 50.0, 800.0, 612.0)`.
    /// - Subtract `padding` once instead of twice on `w`: row 1 yields `w = 820`.
    ///   MEASURED: `case: typical pane`, left `(20.0, 50.0, 820.0, 612.0)`, right
    ///   `(20.0, 50.0, 800.0, 612.0)`.
    /// - Delete the non-finite guard: rows 8, 9 and 10 all move, and row 8 fires
    ///   first. What it produces there is NOT a NaN field: `f64::max` discards a NaN
    ///   operand, so a NaN `pane_w` yields a `w` of `0.0` and the row fails because
    ///   the WHOLE-result collapse did not happen, not because a NaN propagated.
    ///   (Rows 9 and 10 do carry non-finite fields — an infinite `y` and a NaN `x`
    ///   respectively — but the loop never reaches them.) MEASURED: `case: a NaN pane
    ///   width collapses the whole result — grid_rect(NaN, 682, 30, 20)`, left
    ///   `(20.0, 50.0, 0.0, 612.0)`, right `(0.0, 0.0, 0.0, 0.0)`.
    #[test]
    fn grid_rect_table() {
        let cases = [
            // --- The ordinary case, and the two origins ------------------------
            RectCase {
                // x = 20; y = 30 + 20 = 50; w = 840 - 40 = 800;
                // h = 682 - 30 - 40 = 612.
                desc: "typical pane",
                pane_w: 840.0,
                pane_h: 682.0,
                statusbar_h: 30.0,
                padding: 20.0,
                expected: (20.0, 50.0, 800.0, 612.0),
            },
            // --- The clamps: only w and h, and only at zero ---------------------
            RectCase {
                // w = 40 - 40 = 0; h = 70 - 30 - 40 = 0. Both land exactly on the
                // floor without the clamp doing anything, which is why rows 3 and 4
                // exist beside this one.
                desc: "a pane that is exactly its own chrome leaves no grid",
                pane_w: 40.0,
                pane_h: 70.0,
                statusbar_h: 30.0,
                padding: 20.0,
                expected: (20.0, 50.0, 0.0, 0.0),
            },
            RectCase {
                // w = 10 - 40 = -30, CLAMPED to 0; h = 682 - 30 - 40 = 612, untouched.
                desc: "narrower than the chrome clamps w only",
                pane_w: 10.0,
                pane_h: 682.0,
                statusbar_h: 30.0,
                padding: 20.0,
                expected: (20.0, 50.0, 0.0, 612.0),
            },
            RectCase {
                // w = 840 - 40 = 800, untouched; h = 10 - 30 - 40 = -60, CLAMPED to 0.
                desc: "shorter than the chrome clamps h only",
                pane_w: 840.0,
                pane_h: 10.0,
                statusbar_h: 30.0,
                padding: 20.0,
                expected: (20.0, 50.0, 800.0, 0.0),
            },
            // --- The two halves of the chrome, isolated -------------------------
            RectCase {
                // y = 0 + 20 = 20; h = 632 - 0 - 40 = 592. With no strip the rect is
                // the padding alone, which is the arm `resize_global` still wants.
                desc: "a zero strip is pure padding",
                pane_w: 840.0,
                pane_h: 632.0,
                statusbar_h: 0.0,
                padding: 20.0,
                expected: (20.0, 20.0, 800.0, 592.0),
            },
            RectCase {
                // x = 0; y = 30 + 0 = 30; w = 800 - 0 = 800; h = 622 - 30 - 0 = 592.
                desc: "zero padding is pure strip",
                pane_w: 800.0,
                pane_h: 622.0,
                statusbar_h: 30.0,
                padding: 0.0,
                expected: (0.0, 30.0, 800.0, 592.0),
            },
            RectCase {
                // w = -100 - 40 = -140 and h = -100 - 30 - 40 = -170, both clamped.
                // The origins are NOT clamped and stay at their fixed offsets.
                desc: "negative pane dimensions clamp w and h but not the origins",
                pane_w: -100.0,
                pane_h: -100.0,
                statusbar_h: 30.0,
                padding: 20.0,
                expected: (20.0, 50.0, 0.0, 0.0),
            },
            // --- Non-finite inputs collapse the WHOLE result --------------------
            // Not a per-field answer: a rect with a good `y` and a NaN `x` would let
            // `grid_local` hand `cell_at` a NaN column for every pointer event in the
            // pane, which `cell_at` silently treats as column 0 rather than
            // rejecting. All four fields go to zero so the failure is visible as a
            // zero-extent grid instead.
            RectCase {
                desc: "a NaN pane width collapses the whole result",
                pane_w: f64::NAN,
                pane_h: 682.0,
                statusbar_h: 30.0,
                padding: 20.0,
                expected: (0.0, 0.0, 0.0, 0.0),
            },
            RectCase {
                desc: "an infinite strip height collapses the whole result",
                pane_w: 840.0,
                pane_h: 682.0,
                statusbar_h: f64::INFINITY,
                padding: 20.0,
                expected: (0.0, 0.0, 0.0, 0.0),
            },
            RectCase {
                desc: "a NaN padding collapses the whole result",
                pane_w: 840.0,
                pane_h: 682.0,
                statusbar_h: 30.0,
                padding: f64::NAN,
                expected: (0.0, 0.0, 0.0, 0.0),
            },
        ];

        for case in &cases {
            let got = grid_rect(case.pane_w, case.pane_h, case.statusbar_h, case.padding);
            assert_eq!(
                fields(got),
                case.expected,
                "case: {} — grid_rect({}, {}, {}, {}) as (x, y, w, h). Only w and h \
                 clamp at 0; x and y are FIXED offsets. A NaN or infinite field means \
                 the non-finite guard is gone.",
                case.desc,
                case.pane_w,
                case.pane_h,
                case.statusbar_h,
                case.padding
            );
        }
    }

    /// One [`grid_size_for_pane`] table row. The cell is 8x16 throughout, matching
    /// `geometry.rs`'s own table.
    struct SizeCase {
        desc: &'static str,
        pane_w: i32,
        pane_h: i32,
        statusbar_h: f64,
        padding: f64,
        expected: (u16, u16),
    }

    /// The table this unit is judged on: literal pixel sizes proving the grid handed
    /// to Neovim accounts for BOTH the statusbar strip and the four padding gutters.
    ///
    /// ATTRIBUTION — each falsifying edit leaves a different pair of rows green, so a
    /// failure names which subtraction went missing rather than merely reporting that
    /// the arithmetic moved. Each names the edit, the row it fires on, and the
    /// verbatim failure measured for it.
    ///
    /// - Delete the padding subtraction entirely: A gives (105, 40) — 840/8 and
    ///   floor(648/16) — while C and D still pass, so the failure names the PADDING.
    ///   MEASURED: `case: A: the chrome costs whole cells in both axes —
    ///   grid_size_for_pane(840, 680, 32, 20, 8, 16)`, left `(105, 40)`, right
    ///   `(100, 38)`.
    /// - Delete the strip subtraction: A gives (100, 40) — floor(640/16) — while B
    ///   and D still pass, so the failure names the STRIP. MEASURED: `case: A`, left
    ///   `(100, 40)`, right `(100, 38)`.
    /// - Subtract `padding` once instead of twice: A gives (102, 39) and B gives
    ///   (102, 41); C and D still pass. MEASURED: `case: A`, left `(102, 39)`, right
    ///   `(100, 38)`. The loop is fail-fast, so B's `(102, 41)` is derived rather
    ///   than observed — A aborts the run before B is reached.
    /// - Round instead of floor when converting the rect's `f64` dimensions to `i32`:
    ///   H gives (100, 38) instead of (100, 37). It is the ONLY row that can catch
    ///   that edit — see non-claim 2 — and its column axis stays green, so the
    ///   failure names the axis the fractional input reached. MEASURED: `case: H: a
    ///   fractional strip height is floored, never rounded — grid_size_for_pane(840,
    ///   680, 32.5, 20, 8, 16)`, left `(100, 38)`, right `(100, 37)` — and the
    ///   column axis is 100 on both sides, as claimed.
    /// - Row D is the ANCHOR: it is `geometry.rs`'s own first row with no chrome at
    ///   all, so if D ever fails the division changed and this module did not.
    ///
    /// EXPLICIT NON-CLAIMS, both of which exist to stop a later reader deleting a row
    /// as redundant:
    ///
    /// 1. The `max(0, …)` clamps in [`grid_rect`] are NOT falsifiable through this
    ///    table. `geometry::grid_size_for` collapses to (1, 1) on any non-positive
    ///    dimension, so a negative `w` and a clamped `0` produce the same answer.
    ///    Row E therefore proves the one-cell FLOOR, not the clamp;
    ///    [`grid_rect_table`]'s row 3 is the clamp's only discriminator.
    /// 2. Rows A–G cannot tell rounding from flooring in the `f64 -> i32`
    ///    conversion: every rect dimension they produce is a whole number, so the two
    ///    agree on all seven. What F and G pin is that a partial COLUMN is dropped —
    ///    847 px of pane leaves 807 px of grid and 100 whole columns, 848 leaves 808
    ///    and 101 — and that flooring happens inside `geometry::grid_size_for`'s
    ///    division, which this module hands its reduced rect to unchanged. **Row H is
    ///    the one that reaches this module's own conversion**, by way of a fractional
    ///    `statusbar_h`; do not delete it as artificial without reading its comment.
    #[test]
    fn grid_size_for_pane_table() {
        let cases = [
            SizeCase {
                // w = 840 - 40 = 800 -> 800/8 = 100 cols.
                // h = 680 - 32 - 40 = 608 -> 608/16 = 38 rows.
                // Without the chrome the same pane would be 105 x 42, so the chrome
                // costs exactly 5 columns and 4 rows here.
                desc: "A: the chrome costs whole cells in both axes",
                pane_w: 840,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                expected: (100, 38),
            },
            SizeCase {
                // h = 680 - 0 - 40 = 640 -> 40 rows. The padding alone.
                desc: "B: a zero strip leaves the padding alone",
                pane_w: 840,
                pane_h: 680,
                statusbar_h: 0.0,
                padding: 20.0,
                expected: (100, 40),
            },
            SizeCase {
                // h = 648 - 32 - 0 = 616 -> floor(616/16) = 38 rows. The strip alone.
                desc: "C: zero padding leaves the strip alone",
                pane_w: 800,
                pane_h: 648,
                statusbar_h: 32.0,
                padding: 0.0,
                expected: (100, 38),
            },
            SizeCase {
                // 800/8 = 100, 592/16 = 37 — `geometry::grid_size_for_table`'s own
                // first row, reached through this function with no chrome at all.
                desc: "D: no chrome at all is the bare division (the anchor row)",
                pane_w: 800,
                pane_h: 592,
                statusbar_h: 0.0,
                padding: 0.0,
                expected: (100, 37),
            },
            SizeCase {
                // w = max(0, 30 - 40) = 0 and h = max(0, 30 - 32 - 40) = 0, so
                // `grid_size_for`'s non-positive collapse yields its 1x1 floor.
                desc: "E: a pane smaller than its own chrome floors at one cell",
                pane_w: 30,
                pane_h: 30,
                statusbar_h: 32.0,
                padding: 20.0,
                expected: (1, 1),
            },
            SizeCase {
                // w = 847 - 40 = 807 -> floor(807/8) = 100: the 7 leftover pixels
                // buy nothing.
                desc: "F: one pixel short of a whole column is dropped",
                pane_w: 847,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                expected: (100, 38),
            },
            SizeCase {
                // w = 848 - 40 = 808 -> 808/8 = 101 exactly.
                desc: "G: one more pixel buys a column",
                pane_w: 848,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                expected: (101, 38),
            },
            SizeCase {
                // The only row whose rect is NOT a whole number of pixels, and the
                // only one that reaches this module's own `f64 -> i32` conversion.
                //
                // w = 840 - 40 = 800 -> 800/8 = 100 cols. Integral, so the column
                //   axis is the CONTROL: it must not move.
                // h = 680 - 32.5 - 40 = 607.5, FLOORED to 607 -> floor(607/16)
                //   = 37.9375 -> 37 rows.
                // Rounding instead would carry 607.5 to 608, and 608/16 is exactly
                // 38 — a whole extra row of buffer text, from half a pixel of strip.
                //
                // The strip height this seam is CALLED with is integral by
                // construction: `statusbar_layout`'s `height` is two `ceil`ed line
                // boxes plus the vertical padding twice and the line gap once, all of
                // which ship as whole numbers. So this row pins the seam's stated
                // CONTRACT — the conversion floors, whatever it is handed — rather
                // than a state production can reach today. That is deliberate: the
                // seam takes `f64`, and the day someone feeds it a scaled or
                // dpi-converted strip height is the day the contract has to already
                // be true.
                desc: "H: a fractional strip height is floored, never rounded",
                pane_w: 840,
                pane_h: 680,
                statusbar_h: 32.5,
                padding: 20.0,
                expected: (100, 37),
            },
        ];

        for case in &cases {
            let got = grid_size_for_pane(
                case.pane_w,
                case.pane_h,
                case.statusbar_h,
                case.padding,
                CELL_W,
                CELL_H,
            );
            assert_eq!(
                got, case.expected,
                "case: {} — grid_size_for_pane({}, {}, {}, {}, {CELL_W}, {CELL_H}) as \
                 (cols, rows). Read the sibling rows before touching anything: if C \
                 and D are green the padding subtraction is what moved, if B and D \
                 are green the strip subtraction is, if H is red ALONE the `f64 -> \
                 i32` conversion rounds where it must floor, and if D itself is red \
                 the division in `geometry::grid_size_for` changed rather than this \
                 module.",
                case.desc, case.pane_w, case.pane_h, case.statusbar_h, case.padding
            );
        }
    }

    /// Whether two grid-local coordinates agree, counting NaN as agreeing with NaN.
    ///
    /// A plain `==` would fail the NaN rows against themselves, and an `is_nan()`
    /// branch written into each row would let a NaN slip past a row expecting a
    /// number. Infinities compare with `==` like any other value, which is what pins
    /// that they keep their SIGN rather than collapsing.
    fn same_coord(got: f64, want: f64) -> bool {
        (got.is_nan() && want.is_nan()) || got == want
    }

    /// One [`grid_local`] table row.
    struct LocalCase {
        desc: &'static str,
        x: f64,
        y: f64,
        expected: (f64, f64),
    }

    /// Pins [`grid_local`]: `(x - rect.x, y - rect.y)`, and nothing else.
    ///
    /// The rect is written as a LITERAL rather than as a call to [`grid_rect`], so a
    /// bug in `grid_rect` cannot move this table's inputs underneath it and leave it
    /// green. It is the rect `grid_rect(840, 680, 32, 20)` produces — x = 20,
    /// y = 32 + 20 = 52, w = 840 - 40 = 800, h = 680 - 32 - 40 = 608 — so the two
    /// tables describe the same pane; only `x` and `y` are load-bearing here.
    ///
    /// A point in the strip or in a gutter maps to a NEGATIVE coordinate, which
    /// `geometry::cell_at` clamps to the nearest edge cell. That is deliberate and is
    /// exactly macOS's `PaneGeometry.cellAt`; it is also what
    /// [`cell_at_over_grid_local_never_exceeds_grid_size_for_pane`] proves can never
    /// escape the grid.
    ///
    /// FALSIFYING EDITS. Each names the edit, the row it fires on, and the verbatim
    /// failure measured for it.
    ///
    /// - Return the pair in the other order, `(y - rect.y, x - rect.x)` — the mistake
    ///   `cell_at`'s `(row, col)` return order invites, since this function's is
    ///   `(x, y)`. Rows 2, 3 and 4 move; row 1 stays GREEN, which is why the origin
    ///   row alone is not enough. MEASURED, firing on row 2 and so leaving row 1
    ///   green exactly as claimed: `case: one cell in from the origin — grid_local(28,
    ///   68, GridRect { x: 20.0, y: 52.0, w: 800.0, h: 608.0 }) = (16.0, 8.0), want
    ///   (8.0, 16.0)`.
    /// - Cross the two subtractions instead, `(x - rect.y, y - rect.x)`: every row
    ///   including row 1 moves. MEASURED, firing on row 1: `case: the grid origin maps
    ///   to (0, 0) — grid_local(20, 52, …) = (-32.0, 32.0), want (0.0, 0.0)`.
    /// - Collapse a non-finite coordinate to `0.0` here: rows 5, 6 and 7 all move, and
    ///   the loop reports row 5 first. That collapse would undo `cell_at`'s
    ///   drag-past-the-edge rule, which distinguishes a NaN (no direction, treated as
    ///   the origin) from an infinity (a direction, clamped to the far edge) — see
    ///   `geometry::cell_at_table`, which this table must not contradict. MEASURED:
    ///   `case: a NaN x stays NaN — grid_local(NaN, 52, …) = (0.0, 0.0), want
    ///   (NaN, 0.0)`.
    ///
    /// NON-CLAIM, stated because it is the discriminator a reader most expects this
    /// table to carry: **forgetting the strip in [`grid_rect`] — `y = padding`
    /// instead of `statusbar_h + padding` — cannot redden this test, and structurally
    /// cannot.** The rect above is a LITERAL, on purpose, so no edit to `grid_rect`
    /// reaches this table's inputs at all; that is the same property the paragraph
    /// above claims as a feature, and it cuts both ways. The edit is caught by
    /// [`grid_rect_table`]'s third falsifying edit, where row 1's `y` moves from 50 to
    /// 20. Do not "fix" this by building the rect from a `grid_rect` call: that would
    /// let a bug in `grid_rect` move these inputs underneath the table and leave every
    /// row here green.
    #[test]
    fn grid_local_table() {
        let rect = GridRect {
            x: 20.0,
            y: 52.0,
            w: 800.0,
            h: 608.0,
        };
        let cases = [
            LocalCase {
                // (20 - 20, 52 - 52).
                desc: "the grid origin maps to (0, 0)",
                x: 20.0,
                y: 52.0,
                expected: (0.0, 0.0),
            },
            LocalCase {
                // (28 - 20, 68 - 52) — one 8x16 cell in from the origin.
                desc: "one cell in from the origin",
                x: 28.0,
                y: 68.0,
                expected: (8.0, 16.0),
            },
            LocalCase {
                // (30 - 20, 10 - 52). y = 10 is inside the statusbar band, which is
                // the whole point: it goes NEGATIVE rather than being rejected.
                desc: "a point in the statusbar band is negative in y",
                x: 30.0,
                y: 10.0,
                expected: (10.0, -42.0),
            },
            LocalCase {
                // (5 - 20, 60 - 52) — x = 5 is inside the left gutter.
                desc: "a point in the left gutter is negative in x",
                x: 5.0,
                y: 60.0,
                expected: (-15.0, 8.0),
            },
            LocalCase {
                // NaN - 20 is NaN. It must STAY NaN: `cell_at` treats a NaN axis as
                // the origin, and collapsing it here would hide the difference.
                desc: "a NaN x stays NaN",
                x: f64::NAN,
                y: 52.0,
                expected: (f64::NAN, 0.0),
            },
            LocalCase {
                // +inf - 20 is +inf, which `cell_at` clamps to the LAST column.
                desc: "a +inf x stays +inf, keeping its direction",
                x: f64::INFINITY,
                y: 52.0,
                expected: (f64::INFINITY, 0.0),
            },
            LocalCase {
                // -inf - 52 is -inf, which `cell_at` clamps to the FIRST row.
                desc: "a -inf y stays -inf, keeping its direction",
                x: 20.0,
                y: f64::NEG_INFINITY,
                expected: (0.0, f64::NEG_INFINITY),
            },
        ];

        for case in &cases {
            let got = grid_local(case.x, case.y, rect);
            assert!(
                same_coord(got.0, case.expected.0) && same_coord(got.1, case.expected.1),
                "case: {} — grid_local({}, {}, {rect:?}) = {got:?}, want {:?}. The \
                 result is GRID-LOCAL: a point in the strip or a gutter is negative \
                 here and is clamped by `geometry::cell_at`, never clamped here. A \
                 zero where a NaN or an infinity was expected means a non-finite \
                 coordinate was collapsed, which breaks drag-past-the-edge selection.",
                case.desc,
                case.x,
                case.y,
                case.expected
            );
        }
    }

    /// One composition-guarantee row: a whole pane plus its chrome and its cell size,
    /// whose [`grid_size_for_pane`] result must bound EVERY cell
    /// `grid_local` + `geometry::cell_at` can produce for it.
    struct ComposeCase {
        desc: &'static str,
        pane_w: i32,
        pane_h: i32,
        statusbar_h: f64,
        padding: f64,
        cell_w: f64,
        cell_h: f64,
    }

    /// How far outside the pane the "far" probes reach, in pixels.
    const FAR: f64 = 1e9;

    /// The composition guarantee for the INSET pipeline, mirroring
    /// `geometry::cell_at_never_exceeds_grid_size_for` in shape and in naming: a cell
    /// produced by `cell_at` over a `grid_local`-translated point is ALWAYS inside the
    /// grid `grid_size_for_pane` asked Neovim for.
    ///
    /// This is the mechanical answer to the objection the painted design invites —
    /// that a click on the status bar or in one of the four gutters could name a cell
    /// outside the grid. Every probe below is in one of those regions or far outside
    /// the pane entirely, and none of them can escape.
    ///
    /// What it does NOT cover, stated so nobody reads it as more than it is: it calls
    /// the two seams directly, so it cannot say that `pane_view::send_pane_mouse`
    /// actually performs the translation. Deleting `grid_local` from that call site
    /// leaves this test green. The call site is covered by the pointer smokes, whose
    /// derived pane origin moves by exactly the chrome.
    ///
    /// FALSIFYING EDIT: none is aimed at this test in particular, and none is invented
    /// for it. It is a property over the pair — any edit that lets a cell escape the
    /// grid fires it, and no single-line edit to either seam is known to. The edit a
    /// reader most expects it to catch, removing the `max(0, …)` clamps from
    /// [`grid_rect`], leaves it GREEN: the negative dimension reaches
    /// `geometry::grid_size_for`, which collapses to (1, 1) exactly as a clamped zero
    /// does. That is recorded here rather than dressed up as coverage.
    #[test]
    fn cell_at_over_grid_local_never_exceeds_grid_size_for_pane() {
        let cases = [
            ComposeCase {
                desc: "a typical pane with a strip and padding",
                pane_w: 840,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
            },
            ComposeCase {
                desc: "a pane that is exactly its own chrome",
                pane_w: 40,
                pane_h: 72,
                statusbar_h: 32.0,
                padding: 20.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
            },
            ComposeCase {
                desc: "a pane smaller than its own chrome",
                pane_w: 30,
                pane_h: 30,
                statusbar_h: 32.0,
                padding: 20.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
            },
            ComposeCase {
                desc: "a non-integer cell, where every division has a remainder",
                pane_w: 840,
                pane_h: 680,
                statusbar_h: 37.0,
                padding: 20.0,
                cell_w: 9.37,
                cell_h: 19.51,
            },
        ];

        for case in &cases {
            let (cols, rows) = grid_size_for_pane(
                case.pane_w,
                case.pane_h,
                case.statusbar_h,
                case.padding,
                case.cell_w,
                case.cell_h,
            );
            let rect = grid_rect(
                f64::from(case.pane_w),
                f64::from(case.pane_h),
                case.statusbar_h,
                case.padding,
            );
            let w = f64::from(case.pane_w);
            let h = f64::from(case.pane_h);
            let probes = [
                ("the pane's top-left corner", 0.0, 0.0),
                ("the pane's top-right corner", w, 0.0),
                ("the pane's bottom-left corner", 0.0, h),
                ("the pane's bottom-right corner", w, h),
                ("the pane centre", w / 2.0, h / 2.0),
                ("inside the statusbar band", w / 2.0, case.statusbar_h / 2.0),
                ("the left gutter", case.padding / 2.0, h / 2.0),
                ("the right gutter", w - case.padding / 2.0, h / 2.0),
                (
                    "the top gutter, between the strip and the grid",
                    w / 2.0,
                    case.statusbar_h + case.padding / 2.0,
                ),
                ("the bottom gutter", w / 2.0, h - case.padding / 2.0),
                ("far outside, negative in both axes", -FAR, -FAR),
                ("far outside, positive in both axes", FAR, FAR),
                ("NaN in both axes", f64::NAN, f64::NAN),
            ];

            for (probe, x, y) in probes {
                let (gx, gy) = grid_local(x, y, rect);
                let (row, col) = cell_at(gx, gy, case.cell_w, case.cell_h, cols, rows);
                assert!(
                    col < u32::from(cols),
                    "case {:?}, probe {probe:?} at pane-local ({x}, {y}): col {col} \
                     must be < cols {cols}. grid_size_for_pane said {cols}x{rows} for \
                     the reduced rect {rect:?}, and the probe translated to \
                     ({gx}, {gy}). A pointer event naming a column outside the grid \
                     is one `wolf_core::MouseEvent` Neovim has no cell for.",
                    case.desc
                );
                assert!(
                    row < u32::from(rows),
                    "case {:?}, probe {probe:?} at pane-local ({x}, {y}): row {row} \
                     must be < rows {rows}. grid_size_for_pane said {cols}x{rows} for \
                     the reduced rect {rect:?}, and the probe translated to \
                     ({gx}, {gy}).",
                    case.desc
                );
            }
        }
    }

    /// One [`min_pane_size`] row: the inputs, the expected size as literals, and the
    /// same size as the `i32` pair a pane allocation would carry.
    ///
    /// The `i32` pair is stated rather than cast so the test needs no `as` conversion
    /// of its own, and it is checked against the `f64` answer inside the loop, so the
    /// two literals cannot drift apart.
    struct MinPaneCase {
        desc: &'static str,
        cell_w: f64,
        cell_h: f64,
        statusbar_h: f64,
        padding: f64,
        expected: (f64, f64),
        expected_i32: (i32, i32),
    }

    /// [`min_pane_size`] is exactly the pane size at which one whole cell of grid
    /// survives the chrome — no more and no less.
    ///
    /// A property rather than a table: for each row the answer is fed straight back
    /// into [`grid_rect`] and [`grid_size_for_pane`], so the three functions are
    /// asserted to agree rather than each being asserted against a number somebody
    /// wrote down. That is what makes it the guard on
    /// `pane_view::make_pane_area`'s content floor: a floor that does not leave a
    /// whole cell hands Neovim a 1x1 grid and reflows that window's whole buffer to
    /// one column.
    ///
    /// The `> 0.0` assertion is the load-bearing half of the (1, 1) check.
    /// `geometry::grid_size_for` returns (1, 1) for a genuine one-cell grid AND for
    /// every degenerate input, so without it a rect of zero extent — which is what a
    /// floor that forgot the strip produces — would satisfy the size assertion.
    ///
    /// FALSIFYING EDIT: drop the `statusbar_h` term from [`min_pane_size`]. Row 1's
    /// height becomes `16 + 40 = 56`, and the row's EQUALITY assertion fires on that
    /// literal before either property assertion is reached — the `h >= cell_h` and
    /// `> 0.0` checks behind it are what would catch the same edit in a table whose
    /// literals had been updated to match it. MEASURED: `case: an integer cell with a
    /// strip and padding — min_pane_size(8, 16, 32, 20) = (48.0, 56.0), want
    /// (48.0, 88.0)`.
    ///
    /// FALSIFYING EDIT: use `floor` or a bare cast instead of `ceil` on the cell.
    /// Row 2's non-integer cell then yields `(49, 91)` and its equality assertion
    /// fires first, on the literal. The property assertions behind it are what make
    /// that literal more than a transcription: `grid_rect(49, 91, 32, 20)` is
    /// 9 x 19, too small for the 9.37 x 19.51 cell it is supposed to hold, while
    /// `grid_size_for_pane` still answers (1, 1) from `grid_size_for`'s own clamp —
    /// so a table whose literals were updated to match the broken floor would still
    /// go red on `rect.w >= cell_w`. MEASURED: `case: a non-integer cell is ceiled
    /// before the chrome is added — min_pane_size(9.37, 19.51, 32, 20) = (49.0, 91.0),
    /// want (50.0, 92.0)`.
    #[test]
    fn min_pane_size_leaves_exactly_one_cell() {
        let cases = [
            MinPaneCase {
                // (8 + 40, 16 + 32 + 40).
                desc: "an integer cell with a strip and padding",
                cell_w: CELL_W,
                cell_h: CELL_H,
                statusbar_h: 32.0,
                padding: 20.0,
                expected: (48.0, 88.0),
                expected_i32: (48, 88),
            },
            MinPaneCase {
                // ceil(9.37) = 10 and ceil(19.51) = 20, so (10 + 40, 20 + 32 + 40).
                // The ceil is what leaves a WHOLE cell rather than 0.37 px short.
                desc: "a non-integer cell is ceiled before the chrome is added",
                cell_w: 9.37,
                cell_h: 19.51,
                statusbar_h: 32.0,
                padding: 20.0,
                expected: (50.0, 92.0),
                expected_i32: (50, 92),
            },
            MinPaneCase {
                // (8 + 0, 16 + 0 + 0) — the un-inset floor `geometry` alone implies.
                desc: "no chrome at all is the bare cell",
                cell_w: CELL_W,
                cell_h: CELL_H,
                statusbar_h: 0.0,
                padding: 0.0,
                expected: (8.0, 16.0),
                expected_i32: (8, 16),
            },
            MinPaneCase {
                // The 28 pt cell `tests/smoke_headless.rs` records for the pinned
                // DejaVu Sans Mono (22.4765625 x 44.0), under a tall strip:
                // (ceil(22.4765625) + 40, 44 + 92 + 40) = (63, 176).
                desc: "the smoke's measured cell under a tall strip",
                cell_w: 22.4765625,
                cell_h: 44.0,
                statusbar_h: 92.0,
                padding: 20.0,
                expected: (63.0, 176.0),
                expected_i32: (63, 176),
            },
        ];

        for case in &cases {
            let got = min_pane_size(case.cell_w, case.cell_h, case.statusbar_h, case.padding);
            assert_eq!(
                got, case.expected,
                "case: {} — min_pane_size({}, {}, {}, {}) = {got:?}, want {:?}. It is \
                 `(ceil(cell_w) + 2 * padding, ceil(cell_h) + statusbar_h + 2 * \
                 padding)`; a missing term here becomes a pane GTK will allocate that \
                 has no room for a single cell.",
                case.desc, case.cell_w, case.cell_h, case.statusbar_h, case.padding, case.expected
            );
            assert_eq!(
                (
                    f64::from(case.expected_i32.0),
                    f64::from(case.expected_i32.1)
                ),
                case.expected,
                "case: {} — the row's `expected_i32` {:?} and `expected` {:?} \
                 disagree. They are two spellings of one number, and the assertions \
                 below would otherwise be probing a different pane from the one \
                 `min_pane_size` returned.",
                case.desc,
                case.expected_i32,
                case.expected
            );

            let rect = grid_rect(got.0, got.1, case.statusbar_h, case.padding);
            assert!(
                rect.w >= case.cell_w && rect.h >= case.cell_h,
                "case: {} — the minimum pane {got:?} leaves a grid rect of \
                 {rect:?}, which does not hold one whole {} x {} cell. A content \
                 floor that does not leave a cell is what makes GTK allocate a pane \
                 whose grid rounds down to nothing.",
                case.desc,
                case.cell_w,
                case.cell_h
            );
            assert!(
                rect.w > 0.0 && rect.h > 0.0,
                "case: {} — the minimum pane {got:?} leaves a ZERO-extent grid rect \
                 {rect:?}. `geometry::grid_size_for` answers (1, 1) for a real \
                 one-cell grid AND for a degenerate one, so without this the (1, 1) \
                 assertion below would be satisfied by a pane with no grid at all.",
                case.desc
            );

            let size = grid_size_for_pane(
                case.expected_i32.0,
                case.expected_i32.1,
                case.statusbar_h,
                case.padding,
                case.cell_w,
                case.cell_h,
            );
            assert!(
                size.0 >= 1 && size.1 >= 1,
                "case: {} — the minimum pane {got:?} produced a grid of {size:?}. \
                 The rect {rect:?} is big enough for a cell (asserted above), so a \
                 zero here would mean the division disagrees with the subtraction.",
                case.desc
            );
        }
    }
}
