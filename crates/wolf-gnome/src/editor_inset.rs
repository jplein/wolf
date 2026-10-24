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
//! `paintOffsetX`, the centring inset its column cap produces. This crate has that
//! cap and that centring — [`ColumnCap`], applied inside [`grid_rect`] — and still
//! needs no such seam, because ONE rectangle carries the offset: it rides in
//! `grid_rect(..).x`, and the painter, the strip's text and the pointer translation
//! all read that one field. macOS needs the seam only because `EditorInset` and
//! `PaneGeometry` are two structs, with the strip drawn from the first and the grid
//! from the second, so the offset has to be re-derived for the strip.
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

/// The default content-width cap, in columns: `PaneContainerView.maxGridCols`.
pub const MAX_GRID_COLS: u16 = 90;

/// The column cap a pane's grid is measured against, and the cell width it is
/// measured in.
///
/// The two travel together because the cap is a number of COLUMNS and every
/// expression of it — a width in [`grid_rect`], a count in [`grid_size_for_pane`] —
/// needs both. Keeping them one value removes the possibility of a caller handing
/// one call two different cell widths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnCap {
    /// The greatest number of columns the grid may occupy, or `None` for no cap.
    pub max_cols: Option<u16>,
    /// The cell width the cap is measured in, in device pixels.
    pub cell_w: f64,
}

impl ColumnCap {
    /// The uncapped cap at `cell_w` — the grid fills the whole inset rectangle.
    #[must_use]
    pub const fn uncapped(cell_w: f64) -> Self {
        Self {
            max_cols: None,
            cell_w,
        }
    }

    /// `cols` reduced to this cap.
    ///
    /// A cap of ZERO columns names no grid at all and is treated as NO cap, never
    /// as a zero-width one: a zero-width grid hands Neovim a one-column window and
    /// reflows the buffer, which is the failure [`min_pane_size`] exists to prevent.
    #[must_use]
    pub fn apply(self, cols: u16) -> u16 {
        match self.max_cols {
            Some(max) if max > 0 => cols.min(max),
            _ => cols,
        }
    }
}

/// The cap a pane takes for its window's `b:wolf_full_width` flag.
///
/// **THE one place the flag becomes geometry.** A buffer that asked for the full
/// pane width takes no cap; every other one takes [`MAX_GRID_COLS`]. The cell width
/// rides along because the cap is measured in it.
#[must_use]
pub fn column_cap(full_width: bool, cell_w: f64) -> ColumnCap {
    ColumnCap {
        max_cols: if full_width {
            None
        } else {
            Some(MAX_GRID_COLS)
        },
        cell_w,
    }
}

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
/// non-finite input collapses the WHOLE result to zeros — `cap.cell_w` included,
/// since the cap is one of the terms `x` and `w` are built from — following
/// [`crate::geometry`]'s whole-result-collapse convention. A NaN `x` origin would
/// otherwise poison [`grid_local`] for every pointer event in the pane, and
/// `cell_at` silently treats a NaN axis as the origin rather than rejecting it.
///
/// **`cap` widens the rule rather than replacing it.** `w` becomes
/// `min(inset_w, max_cols * cell_w)` and `x` becomes
/// `padding + floor((inset_w - w) / 2)`, so an uncapped pane — and a pane narrower
/// than its own cap — produces exactly the rectangle above. A cap naming no usable
/// width (zero columns, or a non-positive cell) is DISCARDED rather than obeyed:
/// `min(inset_w, 0)` would be a zero-width grid, which hands Neovim a one-column
/// window and reflows the buffer.
///
/// **The `.floor()` on the centring offset is deliberate and diverges from macOS.**
/// Cairo rasterizes glyphs at the translated origin, so a half-pixel `cr.translate`
/// resamples every glyph in the pane, and this crate has neither HiDPI nor
/// subpixel-positioning handling where macOS runs on a 2x backing store. The cost is
/// that the left margin can be one device pixel wider than the right.
#[must_use]
pub fn grid_rect(
    pane_w: f64,
    pane_h: f64,
    statusbar_h: f64,
    padding: f64,
    cap: ColumnCap,
) -> GridRect {
    if !pane_w.is_finite()
        || !pane_h.is_finite()
        || !statusbar_h.is_finite()
        || !padding.is_finite()
        || !cap.cell_w.is_finite()
    {
        return COLLAPSED;
    }
    let inset_w = (pane_w - 2.0 * padding).max(0.0);
    let capped_w = match cap.max_cols {
        Some(max) if max > 0 && cap.cell_w > 0.0 => inset_w.min(f64::from(max) * cap.cell_w),
        _ => inset_w,
    };
    let centre = ((inset_w - capped_w) / 2.0).max(0.0).floor();
    GridRect {
        x: padding + centre,
        y: statusbar_h + padding,
        w: capped_w,
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
///
/// The cell WIDTH arrives inside `cap`, which is the one it is measured in, so a
/// caller cannot hand one call two different widths.
#[must_use]
pub fn grid_size_for_pane(
    pane_w: i32,
    pane_h: i32,
    statusbar_h: f64,
    padding: f64,
    cell_h: f64,
    cap: ColumnCap,
) -> (u16, u16) {
    // The division uses the uncapped rect and caps the column count: flooring a capped width loses
    // the last column at every non-integral cell width. Do not pass `cap` down.
    let rect = grid_rect(
        f64::from(pane_w),
        f64::from(pane_h),
        statusbar_h,
        padding,
        ColumnCap::uncapped(cap.cell_w),
    );
    let (cols, rows) =
        geometry::grid_size_for(floor_to_px(rect.w), floor_to_px(rect.h), cap.cell_w, cell_h);
    (cap.apply(cols), rows)
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
    //! ## The cap is a pure ADDITION, and the uncapped rows are what claim it
    //!
    //! Every table here has two halves. The UNCAPPED rows drive
    //! `ColumnCap::uncapped(CELL_W)` — or, where a row carries its own cell,
    //! `ColumnCap::uncapped` of that width — and each states the value the chrome
    //! arithmetic alone produces, with no term of the cap in it. The CAPPED rows
    //! state the cap.
    //!
    //! Holding the first half fixed is itself a claim, and the strongest one this
    //! module makes: an uncapped grid is exactly the grid the subtraction and the
    //! division produce on their own, in all four fields, at every pane size,
    //! including the degenerate and the non-finite ones. **If an uncapped literal
    //! ever has to move to make a capped row pass, the cap is not an addition and
    //! the arithmetic is wrong — fix the arithmetic, never the literal.** The
    //! practical stake is the compositor smokes, every one of which runs at a
    //! geometry where the cap cannot bind.
    //!
    //! The capped rows spell their cap out as `Some(90)` rather than naming
    //! [`MAX_GRID_COLS`], for the reason [`RectCase`] states: an expectation
    //! derived from the production constant it is testing holds for whatever value
    //! that constant takes.

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

    /// One [`grid_rect`] table row: a description, the five inputs, and the four
    /// expected fields as `(x, y, w, h)`.
    ///
    /// Every expected value is a LITERAL, and every row carries its arithmetic in a
    /// comment beside it. Nothing here is computed from [`EDITOR_PADDING_PX`], from
    /// [`MAX_GRID_COLS`], or from any other production constant: an expectation
    /// derived from the constant the production code reads holds for whatever value
    /// that constant takes, which is the shape of an assertion that cannot fail.
    ///
    /// That is why every capped row below writes `Some(90)` by hand instead of
    /// naming [`MAX_GRID_COLS`]. The constant's own VALUE is pinned where it can be
    /// pinned against something independent — `smoke_headless.rs`'s
    /// `column_cap_and_full_width_toggle_change_the_grid_under_cage`, which compares
    /// it against a column count the captive Neovim reports about itself, and
    /// `the_column_cap_cannot_bind_at_the_ocr_smokes_geometry`, which measures it
    /// against a real cell. Neither belongs here.
    struct RectCase {
        desc: &'static str,
        pane_w: f64,
        pane_h: f64,
        statusbar_h: f64,
        padding: f64,
        cap: ColumnCap,
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
    ///
    /// The five measurements above quote a four-argument call, while a run prints
    /// five arguments and the cap's `Debug`. The rows they name and the left/right
    /// values they report are the ones below: all five drive
    /// `ColumnCap::uncapped(CELL_W)`, whose whole point is that it changes none of
    /// them.
    ///
    /// ## THE COLUMN CAP AND ITS CENTRING (rows 11-18)
    ///
    /// Rows 1-10 are the uncapped rule and are the invariance claim the module doc
    /// makes. Rows 11-18 are the cap: `w` is `min(inset_w, max_cols * cell_w)` and
    /// `x` is `padding + floor((inset_w - w) / 2)`, plus the three arms in which a
    /// cap is present but names no usable width and the whole thing degenerates to
    /// the uncapped rule.
    ///
    /// The `floor` on the centring offset is deliberate and diverges from macOS:
    /// Cairo rasterizes glyphs at the translated origin, so a half-pixel
    /// `cr.translate` resamples every glyph in the pane. Row 13 is the only row that
    /// can see it, because it is the only one whose leftover is odd.
    ///
    /// FALSIFYING EDITS for the cap. Each names the edit, the row it fires on, and
    /// the verbatim failure measured for it.
    ///
    /// - Replace `inset_w.min(max * cell_w)` with `inset_w`: row 11 fires. MEASURED:
    ///   `case: 11: a pane far wider than the cap is capped and centred`, left
    ///   `(20.0, 52.0, 2460.0, 608.0)`, right `(890.0, 52.0, 720.0, 608.0)` — BOTH
    ///   `x` and `w` move, because with no cap the centring leftover is zero and `x`
    ///   falls back to the padding. Row 12, the uncapped twin of the same pane,
    ///   carries no cap for the edit to reach; the fail-fast loop aborts at 11 before
    ///   it runs, so the failure names the CAP rather than the pane arithmetic.
    /// - Drop the `+ centre` from `x`: row 11's `x` becomes 20 instead of 890. This
    ///   is the "click lands in the wrong cell" edit, and
    ///   [`a_capped_grid_maps_the_margins_and_its_own_edges_to_real_columns`] is
    ///   what proves the consequence rather than merely the number. MEASURED:
    ///   `case: 11`, left `(20.0, 52.0, 720.0, 608.0)`, right
    ///   `(890.0, 52.0, 720.0, 608.0)` — `w` stays 720 here and moves under the edit
    ///   above, which is what tells the two apart from their output alone.
    /// - Drop the `.floor()` on `centre`: row 13 fires. It is the ONLY discriminator
    ///   for the floor anywhere — row 11's leftover is even, so the two agree there —
    ///   so do not delete row 13 as a near-duplicate of row 11. MEASURED, and the
    ///   measurement is what establishes that uniqueness rather than the arithmetic:
    ///   the failure names row 13, not row 11, which passed under the same edit.
    ///   `case: 13: an odd leftover floors the centring offset`, left
    ///   `(890.5, 52.0, 720.0, 608.0)`, right `(890.0, 52.0, 720.0, 608.0)`.
    /// - Drop the `cap.cell_w > 0.0` guard: row 16's `w` collapses to 0, because
    ///   `min(2460, 90 * 0.0)` is 0. A zero-width grid hands Neovim a one-column
    ///   window and reflows the buffer, which is the failure [`min_pane_size`]
    ///   exists to prevent. MEASURED: `case: 16: a zero cell width discards the cap`,
    ///   left `(1250.0, 52.0, 0.0, 608.0)`, right `(20.0, 52.0, 2460.0, 608.0)` — the
    ///   `x` of 1250 is the visible symptom, the centring dutifully centring a
    ///   zero-width grid in the middle of the pane.
    /// - Drop the `max > 0` guard: row 17's `w` collapses to 0 the same way.
    ///   MEASURED: `case: 17: a zero-column cap is discarded, never obeyed`, left
    ///   `(1250.0, 52.0, 0.0, 608.0)`, right `(20.0, 52.0, 2460.0, 608.0)` — the same
    ///   pair as row 16 on a different row, so rows 16 and 17 are what tell the two
    ///   guards apart and neither row alone would.
    /// - Drop `cap.cell_w` from the non-finite guard: row 18 yields
    ///   `(20.0, 52.0, 2460.0, 608.0)` instead of the whole-result collapse. Note
    ///   what it does NOT yield — a NaN field: `NaN > 0.0` is false, so the capping
    ///   arm is skipped and the row fails because the collapse did not happen, the
    ///   same shape as row 8. MEASURED, confirming that shape rather than predicting
    ///   it: `case: 18: a NaN cap cell width collapses the whole result`, left
    ///   `(20.0, 52.0, 2460.0, 608.0)`, right `(0.0, 0.0, 0.0, 0.0)`, with no NaN
    ///   field anywhere in the result.
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
                cap: ColumnCap::uncapped(CELL_W),
                expected: (20.0, 50.0, 800.0, 612.0),
            },
            // --- The clamps: only w and h, and only at zero ---------------------
            RectCase {
                // w = 40 - 40 = 0; h = 70 - 30 - 40 = 0: exactly on the floor with no clamp, hence rows 3 and 4.
                desc: "a pane that is exactly its own chrome leaves no grid",
                pane_w: 40.0,
                pane_h: 70.0,
                statusbar_h: 30.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                expected: (20.0, 50.0, 0.0, 0.0),
            },
            RectCase {
                // w = 10 - 40 = -30, CLAMPED to 0; h = 682 - 30 - 40 = 612, untouched.
                desc: "narrower than the chrome clamps w only",
                pane_w: 10.0,
                pane_h: 682.0,
                statusbar_h: 30.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                expected: (20.0, 50.0, 0.0, 612.0),
            },
            RectCase {
                // w = 840 - 40 = 800, untouched; h = 10 - 30 - 40 = -60, CLAMPED to 0.
                desc: "shorter than the chrome clamps h only",
                pane_w: 840.0,
                pane_h: 10.0,
                statusbar_h: 30.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
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
                cap: ColumnCap::uncapped(CELL_W),
                expected: (20.0, 20.0, 800.0, 592.0),
            },
            RectCase {
                // x = 0; y = 30 + 0 = 30; w = 800 - 0 = 800; h = 622 - 30 - 0 = 592.
                desc: "zero padding is pure strip",
                pane_w: 800.0,
                pane_h: 622.0,
                statusbar_h: 30.0,
                padding: 0.0,
                cap: ColumnCap::uncapped(CELL_W),
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
                cap: ColumnCap::uncapped(CELL_W),
                expected: (20.0, 50.0, 0.0, 0.0),
            },
            // Non-finite inputs collapse the whole result: a NaN `x` would hand `cell_at` a NaN column, read
            // silently as column 0. Zeroing all four makes the failure a visible zero-extent grid.
            RectCase {
                desc: "a NaN pane width collapses the whole result",
                pane_w: f64::NAN,
                pane_h: 682.0,
                statusbar_h: 30.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                expected: (0.0, 0.0, 0.0, 0.0),
            },
            RectCase {
                desc: "an infinite strip height collapses the whole result",
                pane_w: 840.0,
                pane_h: 682.0,
                statusbar_h: f64::INFINITY,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                expected: (0.0, 0.0, 0.0, 0.0),
            },
            RectCase {
                desc: "a NaN padding collapses the whole result",
                pane_w: 840.0,
                pane_h: 682.0,
                statusbar_h: 30.0,
                padding: f64::NAN,
                cap: ColumnCap::uncapped(CELL_W),
                expected: (0.0, 0.0, 0.0, 0.0),
            },
            // The cap, the centring and the degenerate arms. Every row shares 680 px height, statusbar_h 32,
            // padding 20, so y = 52 and h = 608 throughout; only x/w is under test.
            RectCase {
                // inset_w = 2500 - 40 = 2460; capped = min(2460, 90 * 8) = 720;
                // centre = floor((2460 - 720) / 2) = 870; x = 20 + 870 = 890.
                desc: "11: a pane far wider than the cap is capped and centred",
                pane_w: 2500.0,
                pane_h: 680.0,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                expected: (890.0, 52.0, 720.0, 608.0),
            },
            RectCase {
                // The same pane uncapped: x at the padding, w the whole inset. Row 11's control, so its failure
                // names the cap rather than the pane arithmetic.
                desc: "12: the same pane uncapped fills the inset from the padding",
                pane_w: 2500.0,
                pane_h: 680.0,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(8.0),
                expected: (20.0, 52.0, 2460.0, 608.0),
            },
            RectCase {
                // inset_w = 2461; capped = 720; the leftover 1741 is odd, so centre = floor(870.5) = 870 and
                // x = 890, not 890.5. The only odd leftover here, so the only row that sees the floor.
                desc: "13: an odd leftover floors the centring offset",
                pane_w: 2501.0,
                pane_h: 680.0,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                expected: (890.0, 52.0, 720.0, 608.0),
            },
            RectCase {
                // inset_w = 660 < 90 * 8 = 720, so the cap does not bind and the rect equals row 15's uncapped
                // twin. Every existing compositor smoke runs in this arm.
                desc: "14: a pane narrower than the cap is untouched by it",
                pane_w: 700.0,
                pane_h: 680.0,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                expected: (20.0, 52.0, 660.0, 608.0),
            },
            RectCase {
                // Row 14's uncapped twin, with the expectation written out so "identical" is checkable by eye.
                desc: "15: row 14's pane uncapped is the same rectangle",
                pane_w: 700.0,
                pane_h: 680.0,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(8.0),
                expected: (20.0, 52.0, 660.0, 608.0),
            },
            RectCase {
                // A zero-width cell names no width, so the whole cap is discarded rather than giving
                // `min(2460, 0) = 0`: a zero-width grid hands Neovim a one-column window.
                desc: "16: a zero cell width discards the cap",
                pane_w: 2500.0,
                pane_h: 680.0,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 0.0,
                },
                expected: (20.0, 52.0, 2460.0, 608.0),
            },
            RectCase {
                // A cap of ZERO columns names no grid at all, and is treated as NO
                // cap for the same reason row 16 is — never as a zero-width one.
                desc: "17: a zero-column cap is discarded, never obeyed",
                pane_w: 2500.0,
                pane_h: 680.0,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(0),
                    cell_w: 8.0,
                },
                expected: (20.0, 52.0, 2460.0, 608.0),
            },
            RectCase {
                // The cap's cell width joins the non-finite guard, as a NaN pane width does in row 8. Without it
                // `NaN > 0.0` skips capping and yields (20, 52, 2460, 608): plausible, which is worse than NaN.
                desc: "18: a NaN cap cell width collapses the whole result",
                pane_w: 2500.0,
                pane_h: 680.0,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: f64::NAN,
                },
                expected: (0.0, 0.0, 0.0, 0.0),
            },
        ];

        for case in &cases {
            let got = grid_rect(
                case.pane_w,
                case.pane_h,
                case.statusbar_h,
                case.padding,
                case.cap,
            );
            assert_eq!(
                fields(got),
                case.expected,
                "case: {} — grid_rect({}, {}, {}, {}, {:?}) as (x, y, w, h). Only w \
                 and h clamp at 0; x and y are FIXED offsets, x carrying the centring \
                 the cap produces. A NaN or infinite field means the non-finite guard \
                 is gone. An `x` of {} where {} was expected means the `+ centre` term \
                 went missing, and a `w` equal to the whole inset means the `min` \
                 against `max_cols * cell_w` did. Read the two together before blaming \
                 either: an `x` that moved ALONE is the centring, while an `x` off by \
                 half a cell WITH a `w` short by a whole one is the `min` against \
                 `max_cols * cell_w`, which leaves more to centre — not the centring.",
                case.desc,
                case.pane_w,
                case.pane_h,
                case.statusbar_h,
                case.padding,
                case.cap,
                got.x,
                case.expected.0
            );
        }
    }

    /// One [`column_cap`] table row: the window's `b:wolf_full_width` flag, the cell
    /// width, and the `max_cols` that flag must produce.
    struct CapCase {
        desc: &'static str,
        full_width: bool,
        cell_w: f64,
        expected_max: Option<u16>,
    }

    /// One [`ColumnCap::apply`] table row: a cap, a column count, and the count that
    /// cap reduces it to.
    struct ApplyCase {
        desc: &'static str,
        cap: ColumnCap,
        cols: u16,
        expected: u16,
    }

    /// [`column_cap`] is THE one place `b:wolf_full_width` becomes geometry, and
    /// [`ColumnCap::apply`] is the one place a cap becomes a column count.
    ///
    /// The flag rows write `Some(MAX_GRID_COLS)` rather than `Some(90)`, and that is
    /// the opposite convention from [`grid_rect_table`]'s deliberately. What this
    /// table claims is which cap the flag SELECTS — the default one, or none at all
    /// — and that claim is true whatever number the default happens to be. It
    /// therefore claims nothing about the number itself: 90's own value is pinned in
    /// `smoke_headless.rs`, against a column count the captive Neovim reports and
    /// against a real measured cell.
    ///
    /// The `cell_w` echo is not decoration. `column_cap` is the only constructor the
    /// widget layer calls, so a cap that dropped the cell width — or substituted a
    /// default one — would silently measure the cap in the wrong unit, and every
    /// capped grid would come out the wrong width with no other symptom.
    ///
    /// FALSIFYING EDITS. Each names the edit and the row it fires on.
    ///
    /// - Invert the flag in [`column_cap`] (`if full_width { Some(MAX_GRID_COLS) }
    ///   else { None }`): both flag rows fire and the `false` row goes first, so the
    ///   failure reads "an ordinary window was handed NO cap" rather than the
    ///   less-alarming inverse. MEASURED, which is what establishes that ordering:
    ///   `case: an ordinary window takes the default cap — column_cap(false, 8)
    ///   produced max_cols None, want Some(90)`, left `None`, right `Some(90)`.
    /// - Return `ColumnCap::uncapped(cell_w)` unconditionally: the `false` row fires
    ///   alone, and the `true` row stays green — which names the direction.
    /// - Substitute any other cell width in the returned cap: both flag rows fire on
    ///   the echo, at whichever `cell_w` the table reaches first.
    /// - Replace `ColumnCap::apply`'s `cols.min(max)` with `max`: the `apply(40)`
    ///   row answers 90 for a pane that only ever had 40 columns, which is the
    ///   widening direction and the one that would put glyphs outside the rect.
    /// - Replace it with `cols`: the `apply(200)` row answers 200 and the cap does
    ///   nothing at all.
    /// - Drop `apply`'s `max > 0` arm: the `Some(0)` row answers 0, and a
    ///   zero-column grid reflows the whole buffer to one column.
    #[test]
    fn column_cap_table() {
        let flag_cases = [
            CapCase {
                desc: "an ordinary window takes the default cap",
                full_width: false,
                cell_w: 8.0,
                expected_max: Some(MAX_GRID_COLS),
            },
            CapCase {
                desc: "a full-width window takes no cap at all",
                full_width: true,
                cell_w: 8.0,
                expected_max: None,
            },
            CapCase {
                desc: "the default cap at the smokes' measured cell",
                full_width: false,
                cell_w: 22.4765625,
                expected_max: Some(MAX_GRID_COLS),
            },
            CapCase {
                desc: "no cap at the smokes' measured cell",
                full_width: true,
                cell_w: 22.4765625,
                expected_max: None,
            },
        ];

        for case in &flag_cases {
            let got = column_cap(case.full_width, case.cell_w);
            assert_eq!(
                got.max_cols, case.expected_max,
                "case: {} — column_cap({}, {}) produced max_cols {:?}, want {:?}. This \
                 is the ONLY place `b:wolf_full_width` becomes geometry: a `false` \
                 answering None means every buffer is full width and the cap ships \
                 dead, and a `true` answering Some means the toggle does nothing a \
                 user can see.",
                case.desc, case.full_width, case.cell_w, got.max_cols, case.expected_max
            );
            assert_eq!(
                got.cell_w, case.cell_w,
                "case: {} — column_cap({}, {}) echoed cell_w {} instead of the {} it \
                 was handed. The cap is measured in this width, so a substituted one \
                 makes every capped grid the wrong number of pixels wide with no \
                 other symptom.",
                case.desc, case.full_width, case.cell_w, got.cell_w, case.cell_w
            );
        }

        let apply_cases = [
            ApplyCase {
                desc: "a count far above the cap is reduced to it",
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                cols: 200,
                expected: 90,
            },
            ApplyCase {
                desc: "a count below the cap is left alone",
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                cols: 40,
                expected: 40,
            },
            ApplyCase {
                desc: "a count exactly at the cap is left alone",
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                cols: 90,
                expected: 90,
            },
            ApplyCase {
                desc: "no cap reduces nothing",
                cap: ColumnCap::uncapped(8.0),
                cols: 200,
                expected: 200,
            },
            ApplyCase {
                desc: "a zero-column cap is no cap, never a zero-width one",
                cap: ColumnCap {
                    max_cols: Some(0),
                    cell_w: 8.0,
                },
                cols: 200,
                expected: 200,
            },
        ];

        for case in &apply_cases {
            let got = case.cap.apply(case.cols);
            assert_eq!(
                got, case.expected,
                "case: {} — {:?}.apply({}) = {got}, want {}. `apply` is a `min` and \
                 nothing else: an answer ABOVE the input widens a grid past the rect \
                 that holds it, and a zero answer hands Neovim a one-column window \
                 and reflows the buffer.",
                case.desc, case.cap, case.cols, case.expected
            );
        }
    }

    /// One [`grid_size_for_pane`] table row. The cell HEIGHT is 16 throughout,
    /// matching `geometry.rs`'s own table; the cell WIDTH rides inside the row's
    /// [`ColumnCap`], which is where `grid_size_for_pane` takes it from.
    struct SizeCase {
        desc: &'static str,
        pane_w: i32,
        pane_h: i32,
        statusbar_h: f64,
        padding: f64,
        cap: ColumnCap,
        expected: (u16, u16),
    }

    /// The grid-size table: literal pixel sizes proving the grid handed to Neovim
    /// accounts for BOTH the statusbar strip and the four padding gutters.
    ///
    /// ATTRIBUTION — each falsifying edit leaves a different pair of rows green, so a
    /// failure names which subtraction went missing rather than merely reporting that
    /// the arithmetic moved. Each names the edit, the row it fires on, and the
    /// verbatim failure measured for it.
    ///
    /// - Delete the padding subtraction entirely: A gives (105, 40) — 840/8 and
    ///   floor(648/16) — while C and D still pass, so the failure names the PADDING.
    ///   MEASURED: `case: A: the chrome costs whole cells in both axes —
    ///   grid_size_for_pane(840, 680, 32, 20, 16, ColumnCap { max_cols: None,
    ///   cell_w: 8.0 })`, left `(105, 40)`, right `(100, 38)`.
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
    ///   680, 32.5, 20, 16, ColumnCap { max_cols: None, cell_w: 8.0 })`, left
    ///   `(100, 38)`, right `(100, 37)` — and the column axis is 100 on both sides,
    ///   as claimed.
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
    ///
    /// ## THE COLUMN CAP'S SECOND EXPRESSION (rows I-M)
    ///
    /// The cap is a `min` on the WIDTH in [`grid_rect`] and a `min` on the COUNT
    /// here, and rows I-M pin the second of the two. Rows A-H all drive
    /// `ColumnCap::uncapped(CELL_W)`, and each states the count the chrome
    /// arithmetic alone produces, which is the module doc's invariance claim.
    ///
    /// - Return `cols` instead of `cap.apply(cols)`: row I fires, so the failure
    ///   names the COUNT's `min` and nothing else. Rows J, K and L carry nothing the
    ///   edit can reach — J is uncapped and already 307, K and L are narrower than
    ///   their own cap — and the fail-fast loop aborts at I before any of them runs.
    ///   MEASURED: `case: I: a wide pane's column count is reduced to the cap —
    ///   grid_size_for_pane(2500, 680, 32, 20, 16, ColumnCap { max_cols: Some(90),
    ///   cell_w: 8.0 })`, left `(307, 38)`, right `(90, 38)`.
    /// - Pass `cap` into the internal [`grid_rect`] call instead of
    ///   `ColumnCap::uncapped(cap.cell_w)`: **row M is the only discriminator
    ///   anywhere, and row I cannot see this edit at all.** At row I's cell the
    ///   capped width is `90 * 8 = 720.0`, which `floor_to_px` truncates losslessly,
    ///   so I still reports 90. At row M's `cell_w = 22.4765625` the capped width is
    ///   `2022.890625`, `floor_to_px` takes it to `2022`, and `2022 / 22.4765625` is
    ///   `89.96…` — so M reports **89**, one column short, and does so for every
    ///   non-integral cell width, which is every cell width the shipped app
    ///   measures. Do not delete row M as artificial: it is the whole reason the
    ///   internal call is written the way it is. MEASURED: `case: M: a non-integral
    ///   cell still reaches the whole cap`, left `(89, 38)`, right
    ///   `(90, 38)`. Row I precedes M and the loop is fail-fast, so the run
    ///   evaluated and PASSED I before reaching M: row I stays GREEN under this
    ///   edit and M fires alone. Where a written prediction says row I reports 89,
    ///   the measurement is the record.
    /// - Rows K and L are a crossed pair: a capped pane narrower than its own cap,
    ///   and the same pane uncapped, with the same expectation written out twice. A
    ///   cap that bound early — `<` where `<=` belongs, or a cap applied to the
    ///   width here as well as in `grid_rect` — separates them.
    #[test]
    fn grid_size_for_pane_table() {
        let cases = [
            SizeCase {
                // w = 800 -> 100 cols; h = 680 - 32 - 40 = 608 -> 38 rows. Without the chrome the pane is
                // 105 x 42, so the chrome costs exactly 5 columns and 4 rows.
                desc: "A: the chrome costs whole cells in both axes",
                pane_w: 840,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                expected: (100, 38),
            },
            SizeCase {
                // h = 680 - 0 - 40 = 640 -> 40 rows. The padding alone.
                desc: "B: a zero strip leaves the padding alone",
                pane_w: 840,
                pane_h: 680,
                statusbar_h: 0.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                expected: (100, 40),
            },
            SizeCase {
                // h = 648 - 32 - 0 = 616 -> floor(616/16) = 38 rows. The strip alone.
                desc: "C: zero padding leaves the strip alone",
                pane_w: 800,
                pane_h: 648,
                statusbar_h: 32.0,
                padding: 0.0,
                cap: ColumnCap::uncapped(CELL_W),
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
                cap: ColumnCap::uncapped(CELL_W),
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
                cap: ColumnCap::uncapped(CELL_W),
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
                cap: ColumnCap::uncapped(CELL_W),
                expected: (100, 38),
            },
            SizeCase {
                // w = 848 - 40 = 808 -> 808/8 = 101 exactly.
                desc: "G: one more pixel buys a column",
                pane_w: 848,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                expected: (101, 38),
            },
            SizeCase {
                // The only fractional rect: h = 607.5 floors to 607 -> 37 rows (rounding gives 38); w is control.
                // See notes/crates/wolf-gnome/src/editor_inset.rs.md#fractional-strip-height
                desc: "H: a fractional strip height is floored, never rounded",
                pane_w: 840,
                pane_h: 680,
                statusbar_h: 32.5,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                expected: (100, 37),
            },
            // --- The cap, applied to the COUNT ---------------------------------
            SizeCase {
                // The uncapped rect is 2460 px = 307 whole columns; the cap reduces the count to 90. Rows are
                // untouched in every row below: h = 608 -> 38.
                desc: "I: a wide pane's column count is reduced to the cap",
                pane_w: 2500,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                expected: (90, 38),
            },
            SizeCase {
                // Row I's control: the same pane with no cap keeps all 307 columns.
                desc: "J: the same pane uncapped keeps every column",
                pane_w: 2500,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(8.0),
                expected: (307, 38),
            },
            SizeCase {
                // w = 660 -> floor(660/8) = 82, below the 90-column cap, so `apply` leaves it alone. Every
                // existing compositor smoke runs in this arm.
                desc: "K: a pane narrower than the cap is untouched by it",
                pane_w: 700,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                expected: (82, 38),
            },
            SizeCase {
                // Row K's uncapped twin, its expectation written out rather than
                // cross-referenced so "identical" is checkable by eye.
                desc: "L: row K's pane uncapped is the same grid",
                pane_w: 700,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(8.0),
                expected: (82, 38),
            },
            SizeCase {
                // Pins the floor-to-px hazard at the smokes' 28 pt cell: 2460 / 22.4765625 -> 109, capped to 90.
                // On the capped rect, 90 * 22.4765625 floors to 2022 px -> 89 columns, one short.
                desc: "M: a non-integral cell still reaches the whole cap",
                pane_w: 2500,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 22.4765625,
                },
                expected: (90, 38),
            },
        ];

        for case in &cases {
            let got = grid_size_for_pane(
                case.pane_w,
                case.pane_h,
                case.statusbar_h,
                case.padding,
                CELL_H,
                case.cap,
            );
            assert_eq!(
                got, case.expected,
                "case: {} — grid_size_for_pane({}, {}, {}, {}, {CELL_H}, {:?}) as \
                 (cols, rows). Read the sibling rows before touching anything: if C \
                 and D are green the padding subtraction is what moved, if B and D \
                 are green the strip subtraction is, if H is red ALONE the `f64 -> \
                 i32` conversion rounds where it must floor, if D itself is red the \
                 division in `geometry::grid_size_for` changed rather than this \
                 module, if I is red alone the cap stopped being applied to the \
                 COUNT, and if M is red alone with I green the division is running \
                 on the CAPPED rect and losing the last column to `floor_to_px`.",
                case.desc, case.pane_w, case.pane_h, case.statusbar_h, case.padding, case.cap
            );
        }
    }

    /// One row of the cap's cross-expression property: a whole pane, its chrome, and
    /// the cap both halves of the arithmetic are handed.
    struct WidthCase {
        desc: &'static str,
        pane_w: i32,
        pane_h: i32,
        statusbar_h: f64,
        padding: f64,
        cap: ColumnCap,
        cell_h: f64,
    }

    /// The cap has TWO expressions — a `min` on the width in [`grid_rect`] and a
    /// `min` on the count in [`grid_size_for_pane`] — and this is what holds them
    /// together.
    ///
    /// The rectangle a pane's grid paints into must be at least as wide as the grid
    /// that pane asked Neovim for. If it is not, Neovim draws columns the painter
    /// clips away and the pointer path maps clicks to cells that are not on screen,
    /// with both individual tables still agreeing with themselves — which is exactly
    /// why this is a property over the pair rather than a third table of literals.
    ///
    /// Every row is run at BOTH an integral cell width and the non-integral one the
    /// smokes measure, and in both the capped and the uncapped arm, because the two
    /// expressions can only disagree where a rounding boundary lets them.
    ///
    /// The `cols >= 1` assertion is the non-vacuity half. `geometry::grid_size_for`
    /// floors at a 1x1 grid, so a zero count is unreachable today — but a zero count
    /// would satisfy the width property for any rectangle at all, including a
    /// collapsed one, so the guard is asserted rather than assumed.
    ///
    /// EXPLICIT NON-CLAIM: this property constrains the two expressions of the cap
    /// **against each other** and says nothing about either being right on its own.
    /// [`grid_rect_table`] and [`grid_size_for_pane_table`] are what pin the values;
    /// a pair that agreed on the wrong quantity — both capping at 80 columns, say —
    /// satisfies this property perfectly at every row. That is the same lesson
    /// `notes/wolf-gnome-decisions.md` records under "Pane weights and dividers" for
    /// `divider_position` and `divider_span_fraction`, where the round trip agreed
    /// for the defect's whole life; it is cross-referenced here rather than
    /// restated.
    ///
    /// FALSIFYING EDIT: in [`grid_rect`], cap the width at
    /// `f64::from(max - 1) * cap.cell_w`. The rect comes out one column narrower
    /// than the count [`grid_size_for_pane`] reports. MEASURED: `case: a capped pane
    /// at an integral cell, where the cap binds — grid_size_for_pane asked Neovim
    /// for 90 columns, which need 720 px at a cell width of 8, but grid_rect
    /// produced a rectangle only 712 px wide`.
    ///
    /// Measured in the same session, under the same edit:
    /// **[`grid_size_for_pane_table`] stayed GREEN**, run alone (`test result: ok.
    /// 1 passed`), which is the half of the claim that matters — the count table
    /// agrees with itself, still answering 90, while the rect has drifted out from
    /// under it, and that drift is the whole reason this test exists.
    /// [`grid_rect_table`] does go red on its own literals under that edit, so the
    /// two per-function tables are not symmetric here; only the count table is blind
    /// to it. MEASURED: `case: 11: a pane far wider than the cap is capped and
    /// centred — grid_rect(2500, 680, 32, 20, ColumnCap { max_cols: Some(90),
    /// cell_w: 8.0 }) as (x, y, w, h)`, left `(894.0, 52.0, 712.0, 608.0)`,
    /// right `(890.0, 52.0, 720.0, 608.0)`. The two fields move by different amounts,
    /// which is not obvious from the edit: `w` drops one whole cell (720 → 712) while
    /// `x` moves RIGHT by half of one (890 → 894), because a narrower capped width
    /// leaves more leftover to centre.
    #[test]
    fn grid_rect_is_wide_enough_for_its_own_grid_size_for_pane() {
        let cases = [
            WidthCase {
                desc: "a capped pane at an integral cell, where the cap binds",
                pane_w: 2500,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                cell_h: CELL_H,
            },
            WidthCase {
                desc: "the same pane uncapped",
                pane_w: 2500,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(8.0),
                cell_h: CELL_H,
            },
            WidthCase {
                desc: "a capped pane at the smokes' non-integral cell",
                pane_w: 2500,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 22.4765625,
                },
                cell_h: 44.0,
            },
            WidthCase {
                desc: "the same non-integral pane uncapped",
                pane_w: 2500,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(22.4765625),
                cell_h: 44.0,
            },
            WidthCase {
                desc: "a capped pane narrower than its own cap",
                pane_w: 700,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                cell_h: CELL_H,
            },
            WidthCase {
                desc: "a capped pane whose rect is not a whole number of pixels",
                pane_w: 847,
                pane_h: 680,
                statusbar_h: 32.5,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 22.4765625,
                },
                cell_h: 44.0,
            },
        ];

        for case in &cases {
            let (cols, _rows) = grid_size_for_pane(
                case.pane_w,
                case.pane_h,
                case.statusbar_h,
                case.padding,
                case.cell_h,
                case.cap,
            );
            let rect = grid_rect(
                f64::from(case.pane_w),
                f64::from(case.pane_h),
                case.statusbar_h,
                case.padding,
                case.cap,
            );
            assert!(
                cols >= 1,
                "case: {} — grid_size_for_pane reported {cols} columns. A zero count \
                 satisfies the width property below for ANY rectangle, including a \
                 collapsed one, so this guard is what keeps that assertion from \
                 passing vacuously.",
                case.desc
            );
            let needed = f64::from(cols) * case.cap.cell_w;
            assert!(
                rect.w >= needed,
                "case: {} — grid_size_for_pane asked Neovim for {cols} columns, which \
                 need {needed} px at a cell width of {}, but grid_rect produced a \
                 rectangle only {} px wide ({rect:?}). The cap has two expressions — \
                 a `min` on this width and a `min` on that count — and they have \
                 drifted apart: Neovim is drawing columns the painter clips away, and \
                 `grid_local` maps clicks to cells that are not on screen. Both \
                 per-function tables can still be green against their own literals \
                 while this is false, which is why the property exists.",
                case.desc,
                case.cap.cell_w,
                rect.w
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
    /// green. It is the rect `grid_rect(840, 680, 32, 20,
    /// ColumnCap::uncapped(CELL_W))` produces — x = 20, y = 32 + 20 = 52,
    /// w = 840 - 40 = 800, h = 680 - 32 - 40 = 608 — so the two tables describe the
    /// same pane; only `x` and `y` are load-bearing here. The cap is spelled out
    /// because it is not idle at this pane width: a 90-column cap at an 8 px cell
    /// binds on an 800 px inset, and the rect would be `x` 60, `w` 720 instead.
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
    ///
    /// ## The second rect: a CAPPED, CENTRED grid
    ///
    /// The rows after the first loop run the same rule over a rectangle whose `x` is
    /// 890 rather than the padding — the rect a 2500 px pane produces under a
    /// 90-column cap at an 8 px cell. It is written as a literal for exactly the
    /// reason the first one is.
    ///
    /// What those rows add is the discriminator no uncapped row can carry:
    /// **`grid_local` must subtract `rect.x`, not [`EDITOR_PADDING_PX`].** In every
    /// uncapped rect the two are the same number, so an implementation reading the
    /// constant instead of the rectangle passes all seven rows above and every
    /// pointer smoke — and then puts every click in a capped pane 870 px to the
    /// left, which is a hundred columns. The capped origin row fires on it, reading
    /// 870 where 0 is expected.
    ///
    /// FALSIFYING EDIT for those rows: subtract `EDITOR_PADDING_PX` instead of
    /// `rect.x`. MEASURED: `case: the capped grid's origin maps to (0, 0) —
    /// grid_local(890, 52, GridRect { x: 890.0, y: 52.0, w: 720.0, h: 608.0 }) =
    /// (870.0, 0.0), want (0.0, 0.0)`. **All seven uncapped rows stayed GREEN**, and
    /// that is measured rather than reasoned: the failure is reported from the
    /// CAPPED loop's assertion site, so the first loop ran to completion before it
    /// was reached. The capped rows are therefore not decoration — they are the only
    /// thing in the file that catches an implementation reading the constant.
    ///
    /// FALSIFYING EDIT, additionally: return `(x, y - rect.y)`. Every row in both
    /// loops moves, and the first loop's row 1 fires first. MEASURED: `case: the
    /// grid origin maps to (0, 0) — grid_local(20, 52, GridRect { x: 20.0, y: 52.0,
    /// w: 800.0, h: 608.0 }) = (20.0, 0.0), want (0.0, 0.0)`.
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

        // 2500 px pane, 90-column cap, 8 px cell: x = 20 + floor(870.0) = 890, y = 52, h = 608.
        // A literal, for the same reason the first rect is one.
        let capped = GridRect {
            x: 890.0,
            y: 52.0,
            w: 720.0,
            h: 608.0,
        };
        let capped_cases = [
            LocalCase {
                // (890 - 890, 52 - 52). Reading EDITOR_PADDING_PX instead of rect.x
                // gives (870, 0) here and (0, 0) in every uncapped row above.
                desc: "the capped grid's origin maps to (0, 0)",
                x: 890.0,
                y: 52.0,
                expected: (0.0, 0.0),
            },
            LocalCase {
                // (898 - 890, 68 - 52) — one 8x16 cell in from the capped origin.
                desc: "one cell in from the capped origin",
                x: 898.0,
                y: 68.0,
                expected: (8.0, 16.0),
            },
            LocalCase {
                // (100 - 890, 60 - 52): x = 100 is in the centring's left margin, inside the pane and outside the
                // grid. It goes negative and `cell_at` clamps it to column 0, as a gutter point does.
                desc: "a point in the centring's left margin is negative in x",
                x: 100.0,
                y: 60.0,
                expected: (-790.0, 8.0),
            },
            LocalCase {
                // (2400 - 890, 60 - 52). x = 2400 is in the RIGHT margin, past the
                // grid's own right edge at 1610, and clamps to the last column.
                desc: "a point in the centring's right margin is past the grid",
                x: 2400.0,
                y: 60.0,
                expected: (1510.0, 8.0),
            },
        ];

        for case in &capped_cases {
            let got = grid_local(case.x, case.y, capped);
            assert!(
                same_coord(got.0, case.expected.0) && same_coord(got.1, case.expected.1),
                "case: {} — grid_local({}, {}, {capped:?}) = {got:?}, want {:?}. This \
                 rect is CENTRED, so its `x` is 890 rather than the padding: an \
                 implementation subtracting `EDITOR_PADDING_PX` instead of `rect.x` \
                 answers 870 for the origin row and passes every uncapped row above, \
                 where the two numbers coincide.",
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
        cap: ColumnCap,
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
    ///
    /// **The same is true of the CENTRING, including in the capped row below.**
    /// Deleting the `+ centre` term from [`grid_rect`]'s `x` leaves this test green,
    /// and that is MEASURED rather than reasoned: the edit was applied and the run
    /// reported `test cell_at_over_grid_local_never_exceeds_grid_size_for_pane ...
    /// ok`. The reason it cannot see the edit is that every probe here is on a pane
    /// edge, in a gutter, or far outside the pane, so it clamps to an edge cell with
    /// the origin at 890 and with it at 20 alike. What that edit breaks is which
    /// cell an ordinary click INSIDE the grid names, and
    /// [`a_capped_grid_maps_the_margins_and_its_own_edges_to_real_columns`] is
    /// measured red under the very same edit, so it is what covers it. The capped
    /// row here adds the escape property under a cap, not the centring.
    #[test]
    fn cell_at_over_grid_local_never_exceeds_grid_size_for_pane() {
        let cases = [
            ComposeCase {
                desc: "a typical pane with a strip and padding",
                pane_w: 840,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                cell_h: CELL_H,
            },
            ComposeCase {
                desc: "a pane that is exactly its own chrome",
                pane_w: 40,
                pane_h: 72,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                cell_h: CELL_H,
            },
            ComposeCase {
                desc: "a pane smaller than its own chrome",
                pane_w: 30,
                pane_h: 30,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(CELL_W),
                cell_h: CELL_H,
            },
            ComposeCase {
                desc: "a non-integer cell, where every division has a remainder",
                pane_w: 840,
                pane_h: 680,
                statusbar_h: 37.0,
                padding: 20.0,
                cap: ColumnCap::uncapped(9.37),
                cell_h: 19.51,
            },
            ComposeCase {
                // A pane wide enough for the cap to bind: a 870 px margin each side, regions no uncapped pane has.
                // A click there must still name a real cell.
                desc: "a capped, centred pane with a margin on each side",
                pane_w: 2500,
                pane_h: 680,
                statusbar_h: 32.0,
                padding: 20.0,
                cap: ColumnCap {
                    max_cols: Some(90),
                    cell_w: 8.0,
                },
                cell_h: CELL_H,
            },
        ];

        for case in &cases {
            let (cols, rows) = grid_size_for_pane(
                case.pane_w,
                case.pane_h,
                case.statusbar_h,
                case.padding,
                case.cell_h,
                case.cap,
            );
            let rect = grid_rect(
                f64::from(case.pane_w),
                f64::from(case.pane_h),
                case.statusbar_h,
                case.padding,
                case.cap,
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
                let (row, col) = cell_at(gx, gy, case.cap.cell_w, case.cell_h, cols, rows);
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

    /// A click inside a CAPPED, CENTRED grid names the cell that is drawn under it,
    /// and a click in either margin names the nearest real column.
    ///
    /// This is the "click lands in the wrong cell" bug class, and it is the one thing
    /// [`cell_at_over_grid_local_never_exceeds_grid_size_for_pane`] structurally
    /// cannot see: that property only asks whether a cell is INSIDE the grid, and
    /// every wrong answer the centring can produce is inside it too. Here the answers
    /// are exact columns.
    ///
    /// The pane is 2500 px wide under a 90-column cap at an 8 px cell, so the grid
    /// runs from x 890 to x 1610 with an 870 px margin on each side.
    ///
    /// **Every probe x below is a LITERAL, and none is derived from `rect`.** That is
    /// the whole discipline of this test: a probe computed as `rect.x + 4` moves with
    /// the revert it is supposed to detect, so the click and the grid stay
    /// self-consistent while both are 870 px from where the painter put them, and the
    /// test passes green against a build that is exactly wrong. `rect` is passed to
    /// `grid_local` because that is the function under test; it never reaches an
    /// input or an expectation.
    ///
    /// The `(cols, rows)` precondition is the non-vacuity half. If the grid ever came
    /// back one column wide, `cell_at` would clamp every probe to column 0 and the
    /// first row would pass while proving nothing.
    ///
    /// FALSIFYING EDIT: delete the `+ centre` term from [`grid_rect`]'s `x`. The
    /// probe at 894 — four pixels inside the real grid's left edge — translates to
    /// 874 instead of 4, `floor(874 / 8)` is 109, `cell_at` clamps that to the last
    /// column, and the col-0 row fires reporting 89. The two right-hand probes stay
    /// green under it, which is what names the direction. MEASURED, arithmetic and
    /// all: `probe "just inside the grid's left edge" at pane-local x = 894 named
    /// column 89, want 0`, left `89`, right `0`, against
    /// `GridRect { x: 20.0, y: 52.0, w: 720.0, h: 608.0 }`. The `(cols, rows)`
    /// precondition passed first in that run, so the grid really was 90 columns wide
    /// and the clamp is what produced 89 — the row is not passing for the
    /// "one-column grid" reason the precondition exists to rule out.
    #[test]
    fn a_capped_grid_maps_the_margins_and_its_own_edges_to_real_columns() {
        let cap = ColumnCap {
            max_cols: Some(90),
            cell_w: 8.0,
        };
        let rect = grid_rect(2500.0, 680.0, 32.0, 20.0, cap);
        let (cols, rows) = grid_size_for_pane(2500, 680, 32.0, 20.0, CELL_H, cap);

        assert_eq!(
            (cols, rows),
            (90, 38),
            "grid_size_for_pane(2500, 680, 32, 20, {CELL_H}, {cap:?}) reported \
             {cols}x{rows} rather than 90x38. Every probe below is checked against a \
             column index, and `cell_at` clamps into whatever grid it is handed — so \
             a one-column grid would satisfy the col-0 rows for the wrong reason. \
             This is a precondition, not the claim: the claim itself is \
             `grid_size_for_pane_table`'s row I."
        );

        // (description, pane-local x, the column that x must name).
        let probes = [
            // 890 + 4: four pixels inside the grid's own left edge, in column 0.
            ("just inside the grid's left edge", 894.0, 0),
            // 890 + 715: inside column 89, the last one, which spans 1602..1610.
            ("just inside the grid's right edge", 1605.0, 89),
            // In the LEFT MARGIN the centring creates — inside the pane, outside the
            // grid — which clamps to the first column.
            ("the left margin", 100.0, 0),
            // In the RIGHT margin, past x 1610, which clamps to the last column.
            ("the right margin", 2400.0, 89),
        ];

        for (probe, x, want_col) in probes {
            // y is a literal too, and is inside the grid's vertical extent (52..660)
            // so the row axis is never what clamps. Only the column is under test.
            let (gx, gy) = grid_local(x, 100.0, rect);
            let (_row, col) = cell_at(gx, gy, cap.cell_w, CELL_H, cols, rows);
            assert_eq!(
                col, want_col,
                "probe {probe:?} at pane-local x = {x} named column {col}, want \
                 {want_col}. The grid is CAPPED and CENTRED: it starts at x 890, not \
                 at the padding, and `grid_local` translated this probe to {gx}. A \
                 column of 89 where 0 was expected is the signature of the centring \
                 offset missing from `grid_rect`'s `x` — the click lands 870 px, a \
                 hundred columns, from the glyph the painter drew under the pointer. \
                 The rect this ran against was {rect:?}."
            );
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
                // The 28 pt DejaVu Sans Mono cell `tests/smoke_headless.rs` records (22.4765625 x 44.0), under
                // a tall strip: (ceil(22.4765625) + 40, 44 + 92 + 40) = (63, 176).
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

            let rect = grid_rect(
                got.0,
                got.1,
                case.statusbar_h,
                case.padding,
                ColumnCap::uncapped(case.cell_w),
            );
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
                case.cell_h,
                ColumnCap::uncapped(case.cell_w),
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
