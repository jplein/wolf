//! Pure pixel-to-grid geometry for the `wolf-gnome` window.
//!
//! This module is deliberately GTK/pango-free: it converts a widget's pixel
//! dimensions and a measured cell size into a Neovim grid size (cols, rows)
//! ([`grid_size_for`]), and a pointer position in pane-local pixels into the grid
//! cell it lands on ([`cell_at`]). That purity is what lets both conversions be
//! unit-tested headlessly, with no display server, no widget, and no nvim —
//! exactly mirroring how the `gdk_key`/`paint` seams stay testable in Tier 1.
//!
//! Those two live in ONE file on purpose: the composition guarantee — that a cell
//! produced by [`cell_at`] is always inside the grid [`grid_size_for`] asked for —
//! must be provable (and is mechanically asserted) in a single place. That
//! guarantee is stated over the DIVISION alone, which is why the per-pane chrome
//! subtracts in [`crate::editor_inset`] rather than growing this file's arguments.
//! The inset pipeline carries its own companion guarantee over the pair —
//! `editor_inset`'s `cell_at_over_grid_local_never_exceeds_grid_size_for_pane`,
//! which probes the statusbar band and all four gutters — so the two are
//! discoverable from either side.
//!
//! [`divider_position`] and [`divider_span_fraction`] are here for the same
//! reason: they are the pixel half of a divider drag, the point where a
//! `gtk::Paned` position and a pane weight meet. They must be inverses — which
//! constrains them to share a denominator, and, on its own, says nothing about
//! which one. That second half is `equal_weights_give_equal_widths`'s, and the two
//! properties are only meaningful together.

// Contract the tests below assert (from the ARCHITECT PLAN):
//   cols = floor(width_px / cell_w), rows = floor(height_px / cell_h),
//   each clamped to [1, u16::MAX].
//   Non-positive dims (<= 0) OR zero/negative/NaN/infinite cell sizes => (1, 1).

/// The minimum grid extent in each axis: the window must always present at least
/// one cell, so a degenerate or undersized input can never collapse a dimension
/// below 1.
const MIN_GRID_DIM: u16 = 1;

/// Converts a widget's pixel size and a measured monospace cell size into a
/// Neovim grid size `(cols, rows)`.
///
/// `cols = floor(width_px / cell_w)` and `rows = floor(height_px / cell_h)`, each
/// clamped to `[1, u16::MAX]`. As a resize guard, the WHOLE result collapses to
/// `(1, 1)` if either pixel dimension is non-positive, or either cell size is
/// degenerate (zero, negative, NaN, or infinite) — in those cases no meaningful
/// grid can be derived, and `(1, 1)` is a safe floor.
#[must_use]
pub fn grid_size_for(width_px: i32, height_px: i32, cell_w: f64, cell_h: f64) -> (u16, u16) {
    // Whole-result collapse: any degenerate cell size or non-positive dimension
    // yields the minimum 1x1 grid rather than a per-axis partial result.
    if !cell_w.is_finite()
        || !cell_h.is_finite()
        || cell_w <= 0.0
        || cell_h <= 0.0
        || width_px <= 0
        || height_px <= 0
    {
        return (MIN_GRID_DIM, MIN_GRID_DIM);
    }

    (
        floor_div_clamped(f64::from(width_px), cell_w),
        floor_div_clamped(f64::from(height_px), cell_h),
    )
}

/// Computes `floor(numer / denom)` clamped into `[1, u16::MAX]` and returns it as
/// a `u16`. Callers guarantee `denom` is finite and positive and `numer` is
/// positive, so the quotient is a finite non-negative number.
fn floor_div_clamped(numer: f64, denom: f64) -> u16 {
    // Clamp in f64 space FIRST so the value is provably within u16 range before
    // the cast; this makes the truncating cast saturating-safe by construction.
    let n = (numer / denom)
        .floor()
        .clamp(f64::from(MIN_GRID_DIM), f64::from(u16::MAX));
    // n is finite and in [1.0, 65535.0], so the cast neither wraps nor truncates
    // out of range; `floor` already removed any fractional part.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "n is clamped to [1.0, u16::MAX] and floored, so the cast is exact and in range"
    )]
    {
        n as u16
    }
}

/// Converts a PANE-LOCAL pointer position in pixels into the grid cell it lands
/// on, as `(row, col)`.
///
/// The tuple order is `(row, col)` — matching [`wolf_core::MouseEvent`]'s
/// `row`/`col` field order and macOS's `PaneGeometry.cellAt` — NOT `(x, y)`.
///
/// Contract the tests below assert:
///
/// - `col = clamp(floor(x / cell_w), 0, cols - 1)` and
///   `row = clamp(floor(y / cell_h), 0, rows - 1)`.
/// - A point OUTSIDE the grid maps to the NEAREST EDGE CELL — never rejected,
///   never out of range. That clamping is what makes drag-past-the-edge selection
///   work: a drag whose pointer has left the pane keeps extending the selection to
///   the last row/column instead of silently stopping at the edge.
/// - `(x, y)` are ALREADY GRID-LOCAL: measured from the grid's own top-left corner,
///   not from the pane's. This function subtracts no origin. A pane's
///   `DrawingArea` is bigger than its grid — it also carries the per-pane statusbar
///   strip and the editor padding — so the translation is
///   [`crate::editor_inset::grid_local`]'s job, applied by `pane_view` to every GTK
///   gesture/motion coordinate before it arrives here, and
///   [`crate::editor_inset::grid_size_for_pane`] is what hands [`grid_size_for`] the
///   correspondingly reduced rectangle. Keeping the subtraction there is what lets
///   the composition guarantee below stay a statement about the division alone.
///   `wolf-gnome` has no 90-column cap and no centering — two still-unticked
///   `feat-list.md` rows — so macOS's `paintOffset` machinery has no counterpart
///   here. **Do not import it.** TRIPWIRE: if either row is ever implemented, the
///   centring offset belongs in `editor_inset`'s rectangle rather than in a new
///   parameter on this function, and this paragraph is where that is meant to be
///   noticed.
/// - Degenerate guards, mirroring [`grid_size_for`]'s whole-result collapse: if
///   `cell_w` or `cell_h` is non-finite or `<= 0.0`, or `cols == 0`, or
///   `rows == 0`, the WHOLE result is `(0, 0)`.
/// - A NaN coordinate has no direction, so that axis is treated as `0.0`. An
///   INFINITE coordinate DOES have a direction, so it clamps to the nearest edge
///   like any other out-of-range point (`+inf` -> last cell, `-inf` -> first
///   cell). See `cell_at_table`, which pins both.
/// - The clamp happens in `f64` space BEFORE the cast, exactly as
///   `floor_div_clamped` does, so the truncating cast to `u32` is saturating-safe
///   by construction.
#[must_use]
pub fn cell_at(x: f64, y: f64, cell_w: f64, cell_h: f64, cols: u16, rows: u16) -> (u32, u32) {
    // Whole-result collapse, mirroring `grid_size_for`: a degenerate cell size or a
    // zero-extent grid leaves no cell that could meaningfully be named, so the
    // whole result is the origin rather than a per-axis partial answer.
    if !cell_w.is_finite() || !cell_h.is_finite() || cell_w <= 0.0 || cell_h <= 0.0 {
        return (0, 0);
    }
    // `cols - 1` / `rows - 1` are the last valid indices; both would underflow at
    // zero, which is exactly the degenerate case guarded here.
    let (Some(last_col), Some(last_row)) = (cols.checked_sub(1), rows.checked_sub(1)) else {
        return (0, 0);
    };

    // (row, col) — NOT (x, y): the y axis feeds the ROW, the x axis the COLUMN.
    (
        floor_clamped(y, cell_h, last_row),
        floor_clamped(x, cell_w, last_col),
    )
}

/// Computes `floor(coord / cell)` clamped into `[0, max_index]` and returns it as
/// a `u32`. Callers guarantee `cell` is finite and positive; `coord` may be any
/// `f64`, including non-finite (see [`cell_at`]'s contract).
fn floor_clamped(coord: f64, cell: f64, max_index: u16) -> u32 {
    // A NaN coordinate has no direction, so there is no "nearest edge" to clamp it
    // to; treat that axis as the origin. (An INFINITE coordinate is left alone: it
    // does have a direction, and the clamp below maps it to the correct edge.)
    let coord = if coord.is_nan() { 0.0 } else { coord };
    // Clamp in f64 space FIRST so the value is provably within u32 range before the
    // cast; this makes the truncating cast saturating-safe by construction, exactly
    // as `floor_div_clamped` does. `+inf / cell` stays `+inf` and clamps to
    // `max_index`; `-inf` clamps to 0.
    let n = (coord / cell).floor().clamp(0.0, f64::from(max_index));
    // n is finite and in [0.0, 65534.0], so the cast neither wraps nor truncates
    // out of range; `floor` already removed any fractional part.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is clamped in f64 space to [0.0, max_index as f64] and floored, so the cast is exact and in range"
    )]
    {
        n as u32
    }
}

/// Converts a divider fraction into a `gtk::Paned` position, as
/// `round(span * fraction)` clamped into `[max(min, 0), max]`.
///
/// **The fraction is of the paned's own SPAN — its width for a horizontal paned,
/// its height for a vertical one — and `min`/`max` are the CLAMP, never the
/// scale.** A weight is a proportion of width: that is what [`crate::layout`]'s
/// `PaneTree` holds, what `crate::pane_layout::encode` persists, what `state.json`
/// carries, and what a user sees on screen. Scaling by `max_position` instead hands
/// the start child `fraction * (span - handle - end_min)` and the end child
/// everything left, so the two differ by the end child's minimum at EVERY fraction
/// — including `0.5`, where a fresh split has to come out even.
/// `equal_weights_give_equal_widths` is the test that says so.
///
/// `min` and `max` are the paned's own `min_position` / `max_position`. Clamping
/// into that range rather than into `[0, span]` matters because `min_position` is
/// the far side of the start child's minimum size: a position below it is one GTK
/// would clamp itself, and every clamp GTK performs emits a `notify::position` the
/// drag handler then has to tell apart from a user's. A fraction the clamp has to
/// move snaps ONCE and then holds, which
/// `clamped_fraction_snaps_once_and_then_holds` pins — and it is a clamp rather
/// than a rescale into the reachable band, because a rescale would silently make a
/// weight a proportion of that band instead of of the pane.
///
/// That lower bound is itself capped at `max`, and the cap is a PANIC GUARD rather
/// than tidiness: `gtk::Paned` transiently reports a `min_position` ABOVE its
/// `max_position` while a nested allocation settles, and `f64::clamp` on an
/// inverted range panics outright (`min > max, or either was NaN. min = 900.0,
/// max = 500.0`, measured). Capping collapses that instant to `max`, which the next
/// `max-position` notify corrects.
///
/// Degenerate inputs collapse the whole result, following this module's convention.
/// `max <= 0` and `span <= 0` are separate arms and both yield `0`: a paned
/// measured before its first allocation reports neither a travel nor an extent, and
/// with the span as the scale a zero one would otherwise multiply out to `0` and be
/// lifted to `min` by the clamp — a position that looks deliberate. A non-finite
/// `fraction` yields the minimum, since it names no position.
#[must_use]
pub fn divider_position(min: i32, max: i32, span: i32, fraction: f64) -> i32 {
    if max <= 0 || span <= 0 {
        return 0;
    }
    let low = min.max(0).min(max);
    if !fraction.is_finite() {
        return low;
    }
    // Clamp in f64 space FIRST so the value is provably within i32 range before the
    // cast, exactly as `floor_div_clamped` does.
    let v = (f64::from(span) * fraction)
        .round()
        .clamp(f64::from(low), f64::from(max));
    #[allow(
        clippy::cast_possible_truncation,
        reason = "v is clamped to [low, max], both i32, and rounded, so the cast is exact and in range"
    )]
    {
        v as i32
    }
}

/// Converts a `gtk::Paned` position back into a divider fraction, clamped into
/// `[0, 1]`. The inverse of [`divider_position`], to within the half pixel `round`
/// can move a position.
///
/// **The hazard is a denominator the two functions do not share, not which
/// denominator they share.** `round(d * (p / d)) == p` holds for any `d` and for no
/// mismatched pair, so the inverse property constrains the two functions to agree
/// and says nothing whatever about what they agree ON. Passing `max_position` here
/// while [`divider_position`] scales by the span — or the reverse — makes the
/// re-apply walk the weight a little further on every allocation pass, which is the
/// failure `divider_position_and_span_fraction_round_trip` exists to catch.
///
/// **Agreement is necessary and nowhere near sufficient.** This pair agreed
/// perfectly with each other for the entire life of the divider defect, while a
/// fresh split came out visibly lopsided, because both scaled by `max_position` and
/// a weight is a proportion of width. `equal_weights_give_equal_widths` is the other
/// half of the contract and the two must be read together; a change that keeps the
/// round trip green has established only half of what it needs to.
///
/// `None` when `span <= 0`: a paned notifies before it has been allocated, and there
/// is no extent for a position to be a fraction OF. Answering `Some(0.0)` there
/// would write a collapsed pane into the layout on the first notify.
#[must_use]
pub fn divider_span_fraction(position: i32, span: i32) -> Option<f64> {
    if span <= 0 {
        return None;
    }
    Some((f64::from(position) / f64::from(span)).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimum grid extent: the function never returns a zero dimension, so the
    /// smallest possible grid is exactly one cell.
    const MIN_DIM: u16 = 1;

    /// A "typical" monospace cell measured at a normal font size, in pixels.
    const CELL_W: f64 = 8.0;
    /// A "typical" monospace cell height, in pixels.
    const CELL_H: f64 = 16.0;

    /// One table row: a human-readable description, the inputs, and the expected
    /// `(cols, rows)` result. Keeping the description inline makes a failure point
    /// straight at the offending case.
    struct Case {
        desc: &'static str,
        width_px: i32,
        height_px: i32,
        cell_w: f64,
        cell_h: f64,
        expected: (u16, u16),
    }

    #[test]
    fn grid_size_for_table() {
        let cases = [
            // --- Typical: exact integer division both ways ---
            Case {
                desc: "typical 800x592 with 8x16 cells -> 100x37",
                width_px: 800,
                height_px: 592,
                cell_w: CELL_W,
                cell_h: CELL_H,
                expected: (100, 37),
            },
            // --- Flooring / truncation: partial cells are dropped ---
            Case {
                desc: "17x17 with 8x16 floors to 2x1",
                width_px: 17,
                height_px: 17,
                cell_w: CELL_W,
                cell_h: CELL_H,
                expected: (2, 1),
            },
            // --- Min-1 clamp: dims that would floor to 0, plus zero/negative dims ---
            Case {
                desc: "tiny 3x3 clamps up to 1x1",
                width_px: 3,
                height_px: 3,
                cell_w: CELL_W,
                cell_h: CELL_H,
                expected: (MIN_DIM, MIN_DIM),
            },
            Case {
                desc: "zero dims clamp to 1x1",
                width_px: 0,
                height_px: 0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                expected: (MIN_DIM, MIN_DIM),
            },
            Case {
                desc: "negative dims clamp to 1x1",
                width_px: -50,
                height_px: -50,
                cell_w: CELL_W,
                cell_h: CELL_H,
                expected: (MIN_DIM, MIN_DIM),
            },
            // --- Degenerate cell sizes always collapse to 1x1 ---
            Case {
                desc: "zero cell sizes -> 1x1",
                width_px: 800,
                height_px: 592,
                cell_w: 0.0,
                cell_h: 0.0,
                expected: (MIN_DIM, MIN_DIM),
            },
            Case {
                desc: "negative cell sizes -> 1x1",
                width_px: 800,
                height_px: 592,
                cell_w: -8.0,
                cell_h: -16.0,
                expected: (MIN_DIM, MIN_DIM),
            },
            Case {
                desc: "NaN cell sizes -> 1x1",
                width_px: 800,
                height_px: 592,
                cell_w: f64::NAN,
                cell_h: f64::NAN,
                expected: (MIN_DIM, MIN_DIM),
            },
            Case {
                desc: "infinite cell sizes -> 1x1",
                width_px: 800,
                height_px: 592,
                cell_w: f64::INFINITY,
                cell_h: f64::INFINITY,
                expected: (MIN_DIM, MIN_DIM),
            },
            // A degenerate cell in only ONE axis still collapses that axis (and,
            // per the contract, the whole result) to the minimum.
            Case {
                desc: "NaN width-cell only -> 1x1",
                width_px: 800,
                height_px: 592,
                cell_w: f64::NAN,
                cell_h: CELL_H,
                expected: (MIN_DIM, MIN_DIM),
            },
            // --- Upper clamp: huge dims saturate at u16::MAX without panic/wrap ---
            Case {
                desc: "i32::MAX dims with 1px cells clamp to u16::MAX",
                width_px: i32::MAX,
                height_px: i32::MAX,
                cell_w: 1.0,
                cell_h: 1.0,
                expected: (u16::MAX, u16::MAX),
            },
        ];

        for case in &cases {
            let got = grid_size_for(case.width_px, case.height_px, case.cell_w, case.cell_h);
            assert_eq!(got, case.expected, "case: {}", case.desc);
        }
    }

    /// The two axes are independent: a degenerate or undersized height must not
    /// drag down a perfectly valid width beyond its own clamp, except where the
    /// contract collapses to 1 (covered above). Here both axes are valid but
    /// asymmetric, proving cols and rows are computed separately.
    #[test]
    fn grid_size_for_independent_axes() {
        let (cols, rows) = grid_size_for(800, 16, CELL_W, CELL_H);
        assert_eq!((cols, rows), (100, MIN_DIM), "wide-but-short grid");

        let (cols, rows) = grid_size_for(8, 592, CELL_W, CELL_H);
        assert_eq!((cols, rows), (MIN_DIM, 37), "narrow-but-tall grid");
    }

    // -- cell_at ------------------------------------------------------------

    /// The "typical" grid these cases run against: 800x592 px at 8x16 cells, i.e.
    /// exactly what `grid_size_for_table`'s first row produces.
    const COLS: u16 = 100;
    /// Rows of the typical grid (see [`COLS`]).
    const ROWS: u16 = 37;
    /// Last valid column index of the typical grid (`COLS - 1`).
    const LAST_COL: u32 = 99;
    /// Last valid row index of the typical grid (`ROWS - 1`).
    const LAST_ROW: u32 = 36;

    /// Last valid index of a `u16::MAX`-wide/tall grid (`u16::MAX - 1`), used by
    /// the saturation case.
    const LAST_INDEX_MAX_GRID: u32 = 65_534;

    /// One `cell_at` table row. `expected` is `(row, col)` — the SAME order
    /// `cell_at` returns, and the same order `wolf_core::MouseEvent` carries.
    struct CellCase {
        desc: &'static str,
        x: f64,
        y: f64,
        cell_w: f64,
        cell_h: f64,
        cols: u16,
        rows: u16,
        expected: (u32, u32),
    }

    #[test]
    fn cell_at_table() {
        let cases = [
            // --- Typical: a point inside the grid picks the cell containing it --
            CellCase {
                desc: "exact centre of cell (row 0, col 0)",
                x: 4.0,
                y: 8.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (0, 0),
            },
            CellCase {
                desc: "exact centre of cell (row 3, col 7)",
                x: 60.0,
                y: 56.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (3, 7),
            },
            // --- Boundaries: floor(), so a boundary belongs to the HIGHER cell --
            CellCase {
                desc: "an exact cell boundary belongs to the HIGHER cell",
                x: 8.0,
                y: 16.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (1, 1),
            },
            CellCase {
                desc: "a hair below the boundary stays in cell (0, 0)",
                x: 7.999,
                y: 15.999,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (0, 0),
            },
            // --- Outside the grid clamps to the NEAREST EDGE cell --------------
            // This is the drag-past-the-edge behaviour: never rejected, never out
            // of range.
            CellCase {
                desc: "far negative x and y clamp to the first cell",
                x: -500.0,
                y: -500.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (0, 0),
            },
            CellCase {
                desc: "far beyond the right/bottom edge clamps to the last cell",
                x: 1e6,
                y: 1e6,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (LAST_ROW, LAST_COL),
            },
            CellCase {
                desc: "negative x with a valid y clamps ONLY x",
                x: -5.0,
                y: 56.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (3, 0),
            },
            // --- A 1x1 grid: every point in the plane collapses to (0, 0) ------
            CellCase {
                desc: "1x1 grid: the origin is cell (0, 0)",
                x: 0.0,
                y: 0.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: MIN_DIM,
                rows: MIN_DIM,
                expected: (0, 0),
            },
            CellCase {
                desc: "1x1 grid: a far positive point still clamps to (0, 0)",
                x: 1e6,
                y: 1e6,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: MIN_DIM,
                rows: MIN_DIM,
                expected: (0, 0),
            },
            CellCase {
                desc: "1x1 grid: a far negative point still clamps to (0, 0)",
                x: -1e6,
                y: -1e6,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: MIN_DIM,
                rows: MIN_DIM,
                expected: (0, 0),
            },
            // --- Degenerate cell sizes collapse the WHOLE result to (0, 0) -----
            CellCase {
                desc: "cell_w == 0.0 -> (0, 0)",
                x: 60.0,
                y: 56.0,
                cell_w: 0.0,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (0, 0),
            },
            CellCase {
                desc: "negative cell_h -> (0, 0)",
                x: 60.0,
                y: 56.0,
                cell_w: CELL_W,
                cell_h: -16.0,
                cols: COLS,
                rows: ROWS,
                expected: (0, 0),
            },
            CellCase {
                desc: "NaN cell_w -> (0, 0)",
                x: 60.0,
                y: 56.0,
                cell_w: f64::NAN,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (0, 0),
            },
            CellCase {
                desc: "infinite cell_h -> (0, 0)",
                x: 60.0,
                y: 56.0,
                cell_w: CELL_W,
                cell_h: f64::INFINITY,
                cols: COLS,
                rows: ROWS,
                expected: (0, 0),
            },
            // --- A zero-extent grid has no cell to name ------------------------
            CellCase {
                desc: "cols == 0 -> (0, 0)",
                x: 60.0,
                y: 56.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: 0,
                rows: ROWS,
                expected: (0, 0),
            },
            CellCase {
                desc: "rows == 0 -> (0, 0)",
                x: 60.0,
                y: 56.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: 0,
                expected: (0, 0),
            },
            // --- Non-finite COORDINATES (as opposed to cell sizes) -------------
            // This is TWO rules, not one, and the difference is the whole point of
            // these three rows:
            //   * NaN has no direction, so there is no edge to clamp toward: that
            //     axis is treated as `0.0`.
            //   * An infinity DOES have a direction, so it is just an
            //     out-of-range point and clamps to the nearest edge like any other
            //     one — `+inf` to the LAST row/col, `-inf` to the FIRST.
            // Do not "simplify" these into a single treat-every-non-finite-as-0.0
            // guard: that would silently break drag-past-the-edge selection for a
            // coordinate that overflowed, which is exactly the case the clamp
            // exists for.
            CellCase {
                desc: "NaN x with a valid y treats x as 0.0",
                x: f64::NAN,
                y: 56.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (3, 0),
            },
            CellCase {
                desc: "+infinite y with a valid x clamps to the LAST row",
                x: 60.0,
                y: f64::INFINITY,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (LAST_ROW, 7),
            },
            CellCase {
                desc: "-infinite y with a valid x clamps to the FIRST row",
                x: 60.0,
                y: f64::NEG_INFINITY,
                cell_w: CELL_W,
                cell_h: CELL_H,
                cols: COLS,
                rows: ROWS,
                expected: (0, 7),
            },
            // --- Upper saturation: the cast can never wrap ---------------------
            CellCase {
                desc: "u16::MAX grid with 1px cells and a huge point saturates at the last index",
                x: 1e12,
                y: 1e12,
                cell_w: 1.0,
                cell_h: 1.0,
                cols: u16::MAX,
                rows: u16::MAX,
                expected: (LAST_INDEX_MAX_GRID, LAST_INDEX_MAX_GRID),
            },
        ];

        for case in &cases {
            let got = cell_at(
                case.x,
                case.y,
                case.cell_w,
                case.cell_h,
                case.cols,
                case.rows,
            );
            assert_eq!(
                got, case.expected,
                "case: {} — cell_at returns (row, col), NOT (x, y)",
                case.desc
            );
        }
    }

    /// One composition-guarantee row: a widget size plus a cell size, whose
    /// [`grid_size_for`] result must bound EVERY [`cell_at`] result.
    struct ComposeCase {
        desc: &'static str,
        width_px: i32,
        height_px: i32,
        cell_w: f64,
        cell_h: f64,
    }

    /// How far outside the widget the "far" probe points reach, in pixels. Large
    /// enough that no plausible widget/cell combination contains them.
    const FAR: f64 = 1e9;
    /// A hair, in pixels — used to probe just inside the bottom-right corner.
    const HAIR: f64 = 0.001;

    /// The composition guarantee, made executable: a cell produced by [`cell_at`]
    /// is ALWAYS inside the grid [`grid_size_for`] asked for, for every probe
    /// point — including far outside the widget in both directions, and NaN.
    ///
    /// This is what lets `pane_view` pass a pointer position straight through
    /// without a bounds check of its own: the seam cannot produce an out-of-range
    /// cell, so `wolf_core::MouseEvent`'s grid-local `row`/`col` are always valid
    /// for the grid they target.
    #[test]
    fn cell_at_never_exceeds_grid_size_for() {
        let cases = [
            ComposeCase {
                desc: "typical 800x592 at 8x16",
                width_px: 800,
                height_px: 592,
                cell_w: CELL_W,
                cell_h: CELL_H,
            },
            ComposeCase {
                desc: "tiny 3x3 at 8x16 (grid_size_for clamps up to 1x1)",
                width_px: 3,
                height_px: 3,
                cell_w: CELL_W,
                cell_h: CELL_H,
            },
            ComposeCase {
                desc: "huge i32::MAX x i32::MAX at 1x1 (grid_size_for saturates at u16::MAX)",
                width_px: i32::MAX,
                height_px: i32::MAX,
                cell_w: 1.0,
                cell_h: 1.0,
            },
            ComposeCase {
                desc: "non-integer cell 9.37x19.51 in an 800x592 widget",
                width_px: 800,
                height_px: 592,
                cell_w: 9.37,
                cell_h: 19.51,
            },
        ];

        for case in &cases {
            let (cols, rows) =
                grid_size_for(case.width_px, case.height_px, case.cell_w, case.cell_h);
            let w = f64::from(case.width_px);
            let h = f64::from(case.height_px);
            let probes = [
                ("far outside, negative in both axes", -FAR, -FAR),
                ("the widget origin", 0.0, 0.0),
                ("the widget centre", w / 2.0, h / 2.0),
                ("a hair inside the bottom-right corner", w - HAIR, h - HAIR),
                ("exactly the bottom-right corner", w, h),
                ("far outside, positive in both axes", FAR, FAR),
                ("NaN in both axes", f64::NAN, f64::NAN),
            ];

            for (probe, x, y) in probes {
                let (row, col) = cell_at(x, y, case.cell_w, case.cell_h, cols, rows);
                assert!(
                    col < u32::from(cols),
                    "case {:?}, probe {probe:?}: col {col} must be < cols {cols} \
                     (grid_size_for said {cols}x{rows})",
                    case.desc
                );
                assert!(
                    row < u32::from(rows),
                    "case {:?}, probe {probe:?}: row {row} must be < rows {rows} \
                     (grid_size_for said {cols}x{rows})",
                    case.desc
                );
            }
        }
    }

    // -- divider position / span fraction -----------------------------------
    //
    // The integer half of the divider conversion. `pane_view` owns the widget and
    // the tree owns the weight; these two are the only place the pixel and the
    // fraction meet, and they must be exact inverses or the `max-position` re-apply
    // walks the weight a little further on every allocation pass.
    //
    // A weight is a proportion of WIDTH — that is what `PaneTree` holds, what
    // `pane_layout::encode` persists, what `state.json` carries, and what a user
    // sees — so the quantity these two scale by is the paned's own SPAN, not its
    // `max_position`. `min`/`max` are the CLAMP. Scaling by `max_position` instead
    // hands the start child `f * (span - handle - e_min)` and the end child the
    // rest, so at `f = 0.5` the two differ by the end child's minimum whatever the
    // fraction was; `equal_weights_give_equal_widths` is the test that says so, and
    // its old-floor row is there to show that the defect never depended on how big
    // that minimum happened to be.

    /// One [`divider_position`] table row.
    struct DividerCase {
        desc: &'static str,
        min: i32,
        max: i32,
        span: i32,
        fraction: f64,
        expected: i32,
    }

    /// A `gtk::Paned` SPAN typical of the smokes' 1280x720 cage output: the pane
    /// area's own width, which is what a weight is a proportion of.
    const SPAN: i32 = 1064;

    /// The `max_position` that same paned reports: its span less the handle and the
    /// end child's minimum. Kept as its own constant, and deliberately NOT equal to
    /// [`SPAN`], because a table that fed one number to both could not tell the two
    /// quantities apart — which is exactly how the defect survived.
    const TRAVEL: i32 = 1000;

    /// What separates [`SPAN`] from [`TRAVEL`]: a 1 px Adwaita separator plus the
    /// 63 px per-pane floor `editor_inset::min_pane_size` produces at the smokes'
    /// cell size. Stated so the two constants above are visibly one paned rather
    /// than two unrelated numbers.
    const END_MIN_PLUS_HANDLE: i32 = 64;

    /// A far child's minimum position, as `gtk::Paned::min_position` reports it for
    /// a non-shrinking start child.
    const MIN_POS: i32 = 100;

    /// Pins [`divider_position`]: `round(span * fraction)` clamped into the paned's
    /// own `[min, max]`, with a whole-result collapse for a degenerate span OR a
    /// degenerate travel.
    ///
    /// The scale is the SPAN and the clamp is `[min, max]`; the two are different
    /// numbers here on purpose (see [`SPAN`] and [`TRAVEL`]), so a row cannot pass
    /// by scaling with the wrong one.
    ///
    /// FALSIFYING EDITS.
    ///
    /// - Scale by `max` instead of `span` — the defect this signature exists to fix.
    ///   Every row whose fraction is neither 0 nor clamped moves: the loop reports
    ///   "half the SPAN" first, at 500 instead of 532, and the two band-edge rows
    ///   (0.93 -> 930 rather than 990, 0.95 -> 950 rather than the clamp) go with it.
    ///   Red run: owed.
    /// - Change the clamp's lower bound from `min` back to `0`. The `0.0 clamps UP`
    ///   row gives 0 instead of 100. MEASURED against the PRE-`span` signature, whose
    ///   message had no `span` field: `case: 0.0 clamps UP to the paned's own minimum
    ///   position (min 100, max 1000, fraction 0)`, left `0` and right `100`. The
    ///   outcome is unchanged by the new argument; only the quoted text is stale.
    /// - Drop the `.min(max)` cap on that lower bound. MEASURED, likewise
    ///   pre-`span`: not an assertion failure but a PANIC, from inside the standard
    ///   library — `core/src/num/f64.rs:1503:9: min > max, or either was NaN.
    ///   min = 900.0, max = 500.0` — which is the whole reason the cap is there.
    /// - Drop the `span <= 0` collapse: the two `span == 0` rows give 100, because
    ///   `round(0 * f)` is 0 and the clamp lifts it to the minimum. Those rows carry
    ///   a NON-ZERO `min` for exactly that reason. Red run: owed.
    ///
    /// A further one, aimed at the rounding rows rather than at either clamp:
    /// replacing `.round()` with `.floor()` fires this table too (MEASURED
    /// pre-`span`), so `divider_position_and_span_fraction_round_trip` is not the
    /// only guard on the rounding.
    #[test]
    fn divider_position_table() {
        // Guard on the FIXTURE, not on the function: [`SPAN`] and [`TRAVEL`] have to
        // describe one paned, or "half the span is not half the travel" is a
        // coincidence between two unrelated numbers rather than a property. The gap
        // between them is the handle plus the end child's floor, and nothing else.
        assert_eq!(
            SPAN - TRAVEL,
            END_MIN_PLUS_HANDLE,
            "SPAN ({SPAN}) minus TRAVEL ({TRAVEL}) is {}, not END_MIN_PLUS_HANDLE \
             ({END_MIN_PLUS_HANDLE}). `gtk::Paned` derives `max_position` from its span by \
             subtracting exactly the handle and the end child's minimum, so these three \
             constants are one measurement in three parts; changing one without the others \
             leaves this table asserting about a paned that cannot exist.",
            SPAN - TRAVEL
        );

        let cases = [
            // --- A degenerate TRAVEL collapses the whole result, as this module's
            // --- other functions do.
            DividerCase {
                desc: "max == 0 (a paned measured before its first allocation) -> 0",
                min: 0,
                max: 0,
                span: SPAN,
                fraction: 0.5,
                expected: 0,
            },
            DividerCase {
                desc: "a negative max -> 0, even with a positive minimum",
                min: MIN_POS,
                max: -5,
                span: SPAN,
                fraction: 0.5,
                expected: 0,
            },
            // --- A degenerate SPAN collapses it too, and that is a SECOND guard
            // --- rather than a restatement: the span is now the scale, so a paned
            // --- measured before its first allocation reports a zero one.
            // ---
            // --- Both rows carry min = 100, and that is load-bearing. With min = 0
            // --- the collapse and the clamp agree on 0 and neither row could tell
            // --- them apart; with a non-zero min, a missing span guard shows up as
            // --- the MINIMUM (100) rather than as the collapse (0).
            DividerCase {
                desc: "span == 0 with a real travel and a real minimum -> 0",
                min: MIN_POS,
                max: TRAVEL,
                span: 0,
                fraction: 0.5,
                expected: 0,
            },
            DividerCase {
                desc: "a negative span likewise -> 0",
                min: MIN_POS,
                max: TRAVEL,
                span: -5,
                fraction: 0.5,
                expected: 0,
            },
            // --- The ordinary case, and the whole point of the signature: the
            // --- fraction is of the SPAN. 1064 * 0.5 = 532, which is NOT 500 —
            // --- half of `max_position` — and the gap between those two numbers is
            // --- the end child's minimum plus the handle.
            DividerCase {
                desc: "half the SPAN, not half the travel",
                min: 0,
                max: TRAVEL,
                span: SPAN,
                fraction: 0.5,
                expected: 532,
            },
            DividerCase {
                // 1065 * 0.5 = 532.5 exactly, which must round AWAY from zero.
                desc: "a .5 pixel rounds AWAY from zero, never truncates",
                min: 0,
                max: TRAVEL,
                span: 1065,
                fraction: 0.5,
                expected: 533,
            },
            DividerCase {
                // 1064 * 0.3998 = 425.3872 -> 425.
                desc: "below the .5 boundary rounds down",
                min: 0,
                max: TRAVEL,
                span: SPAN,
                fraction: 0.3998,
                expected: 425,
            },
            DividerCase {
                // 1064 * 0.4 = 425.6 -> 426.
                desc: "above the .5 boundary rounds up",
                min: 0,
                max: TRAVEL,
                span: SPAN,
                fraction: 0.4,
                expected: 426,
            },
            // --- The clamp is into [min, max], NOT [0, span]: `min_position` is the
            // --- far side of the start child's own minimum, so a position below it
            // --- is one GTK would clamp itself and notify about.
            DividerCase {
                desc: "0.0 clamps UP to the paned's own minimum position",
                min: MIN_POS,
                max: TRAVEL,
                span: SPAN,
                fraction: 0.0,
                expected: MIN_POS,
            },
            DividerCase {
                desc: "a negative fraction clamps up to the minimum too",
                min: MIN_POS,
                max: TRAVEL,
                span: SPAN,
                fraction: -0.5,
                expected: MIN_POS,
            },
            DividerCase {
                desc: "a NEGATIVE minimum is floored at 0",
                min: -50,
                max: TRAVEL,
                span: SPAN,
                fraction: 0.0,
                expected: 0,
            },
            // --- The TOP of the range, where the two quantities part company. A
            // --- fraction of 1 asks for the whole span (1064 px); `max_position`
            // --- caps it at 1000. The number is what it always was, but it now
            // --- proves the CLAMP binds rather than that the scale reached `max`
            // --- exactly — those two are indistinguishable while span == max, which
            // --- is why the rows below bracket the crossing at 1000/1064.
            DividerCase {
                desc: "1.0 asks for the WHOLE span, and max_position caps it",
                min: MIN_POS,
                max: TRAVEL,
                span: SPAN,
                fraction: 1.0,
                expected: TRAVEL,
            },
            DividerCase {
                desc: "a fraction above 1 clamps to the maximum too",
                min: MIN_POS,
                max: TRAVEL,
                span: SPAN,
                fraction: 1.5,
                expected: TRAVEL,
            },
            DividerCase {
                // 1064 * 0.93 = 989.52 -> 990, which is under max_position (1000),
                // so nothing clamps. The crossing sits at 1000/1064 = 0.93985...
                desc: "just below the top of the reachable band is NOT clamped",
                min: MIN_POS,
                max: TRAVEL,
                span: SPAN,
                fraction: 0.93,
                expected: 990,
            },
            DividerCase {
                // 1064 * 0.95 = 1010.8 -> 1011, which the clamp cuts to 1000. The
                // pair above and below the crossing is what stops a "fix" that
                // rescales the fraction into the reachable band from passing: such a
                // fix moves BOTH of these, not just the clamped one.
                desc: "just above the top of the reachable band IS clamped",
                min: MIN_POS,
                max: TRAVEL,
                span: SPAN,
                fraction: 0.95,
                expected: TRAVEL,
            },
            // --- An INVERTED range, which is a PANIC GUARD rather than a
            // --- convention: `f64::clamp` panics outright when its lower bound
            // --- exceeds its upper one, and `gtk::Paned` can report
            // --- `min_position > max_position` transiently while an allocation is
            // --- still settling — the same multi-pass allocation the whole
            // --- `max-position` re-apply exists for. Capping the lower bound at
            // --- `max` is what makes the call total, and this row is the cap's
            // --- only test.
            DividerCase {
                desc: "min > max caps at max instead of panicking",
                min: 900,
                max: 500,
                span: SPAN,
                fraction: 0.5,
                expected: 500,
            },
            // --- A non-finite fraction has no position at all.
            DividerCase {
                desc: "NaN collapses to the minimum",
                min: MIN_POS,
                max: TRAVEL,
                span: SPAN,
                fraction: f64::NAN,
                expected: MIN_POS,
            },
            DividerCase {
                desc: "+inf collapses to the minimum",
                min: MIN_POS,
                max: TRAVEL,
                span: SPAN,
                fraction: f64::INFINITY,
                expected: MIN_POS,
            },
            DividerCase {
                desc: "-inf collapses to the minimum",
                min: MIN_POS,
                max: TRAVEL,
                span: SPAN,
                fraction: f64::NEG_INFINITY,
                expected: MIN_POS,
            },
        ];

        for case in &cases {
            let got = divider_position(case.min, case.max, case.span, case.fraction);
            assert_eq!(
                got, case.expected,
                "case: {} (min {}, max {}, span {}, fraction {}). The scale is the SPAN \
                 and the clamp is [min, max]; a result that looks like `round(max * \
                 fraction)` means the two have been swapped back.",
                case.desc, case.min, case.max, case.span, case.fraction
            );
        }
    }

    /// One [`equal_weights_give_equal_widths`] row: a paned's own geometry, plus the
    /// `max_position` GTK derives from it and the position this module must answer.
    ///
    /// `max` and `expected_position` are LITERALS in the table and are also derived
    /// inside the test from the other fields, so a row cannot drift into describing a
    /// paned GTK would never produce.
    struct EqualCase {
        desc: &'static str,
        span: i32,
        handle: i32,
        s_min: i32,
        e_min: i32,
        max: i32,
        expected_position: i32,
        clamped: bool,
    }

    /// Equal weights give equal widths — the property the whole signature exists for,
    /// and the test that would have caught the defect the day it landed.
    ///
    /// A fresh `<C-w>v` splits with two equal weights and no drag, so the two panes
    /// must come out the same width to within the handle. Scaling by `max_position`
    /// makes that impossible by construction: `max_position` is the span less the
    /// handle less the END child's minimum, so at `f = 0.5` the start child gets
    /// `f * (span - handle - e_min)` and the end child gets everything else — a
    /// difference of exactly `e_min`, at every fraction, forever.
    ///
    /// **The old-floor row is the load-bearing one and must not be deleted as
    /// redundant.** It uses the one-cell `e_min` that shipped before
    /// `editor_inset::min_pane_size` raised the per-pane floor, and it shows from a
    /// unit test that the defect never depended on the raise: it was always there,
    /// 22 px wide, sitting just inside a compositor tolerance that had been
    /// calibrated on it. What the raised floor changed was the size of the symptom,
    /// not its existence.
    ///
    /// The bound is `handle + 1`, and both terms are real: the handle is space
    /// neither child gets, and the `+ 1` is the half pixel `round` can move an odd
    /// span's midpoint. It is not slack — widening it past the handle would readmit
    /// the whole defect, whose smallest recorded spread was 22 px.
    ///
    /// FALSIFYING EDIT: restore `round(max * fraction)`. Row 1 then answers 483,
    /// giving widths 483 and 545, and row 2 answers 503, giving 503 and 525 — spreads
    /// of 62 and 22 px against a bound of 2. Both rows go red, and the second is the
    /// one that says the old floor was never safe either. Red run: owed.
    #[test]
    fn equal_weights_give_equal_widths() {
        let cases = [
            EqualCase {
                // 1029 - 1 = 1028 of child span; max(63, max(1, 1028 - 63)) = 965.
                // round(1029 * 0.5) = round(514.5) = 515, inside [63, 965].
                // widths 515 and 1029 - 1 - 515 = 513; spread 2, bound 2.
                desc: "the raised floor: a 63 px per-pane minimum",
                span: 1029,
                handle: 1,
                s_min: 63,
                e_min: 63,
                max: 965,
                expected_position: 515,
                clamped: false,
            },
            EqualCase {
                // The floor that shipped BEFORE the pane chrome: one 22.47 px cell,
                // ceiled to 23. max(23, max(1, 1028 - 23)) = 1005 — the very
                // `max_position` the divider smoke has recorded all along.
                // The answer is the same 515: the position does not depend on
                // `max_position` at all any more, which is the fix in one line.
                desc: "the OLD one-cell floor, which HID the defect",
                span: 1029,
                handle: 1,
                s_min: 23,
                e_min: 23,
                max: 1005,
                expected_position: 515,
                clamped: false,
            },
            EqualCase {
                // An even span: round(1030 * 0.5) = 515 with no .5 to resolve, so the
                // residual is exactly the handle and the `+ 1` term is unused.
                desc: "an even span leaves no rounding at all",
                span: 1030,
                handle: 1,
                s_min: 63,
                e_min: 63,
                max: 966,
                expected_position: 515,
                clamped: false,
            },
            EqualCase {
                // A 5 px handle: widths 515 and 1030 - 5 - 515 = 510. The spread is
                // the handle's own width and nothing else, which is what says the
                // handle is the only asymmetry left.
                desc: "a wide handle costs exactly its own width",
                span: 1030,
                handle: 5,
                s_min: 63,
                e_min: 63,
                max: 962,
                expected_position: 515,
                clamped: false,
            },
            EqualCase {
                // 119 of child span against two 63 px floors: max(63, max(1, 56)) =
                // 63, so min == max and the midpoint (60) is clamped up to 63.
                //
                // Equal widths are IMPOSSIBLE here and the clamp is the right answer.
                // 63 is exactly `editor_inset::min_pane_size(22.4765625, _, _, 20.0).0`
                // — one whole cell plus two paddings — so averaging through it would
                // hand BOTH panes less than one column and reflow both buffers to a
                // single character per line. This row therefore asserts the POSITION
                // and deliberately does not assert equal widths.
                desc: "a span too small for two floors keeps the floor, not the average",
                span: 120,
                handle: 1,
                s_min: 63,
                e_min: 63,
                max: 63,
                expected_position: 63,
                clamped: true,
            },
        ];

        for case in &cases {
            // GTK's own arithmetic, restated here rather than assumed: a paned's
            // `max_position` is its span less the handle less the end child's
            // minimum, floored so it can never fall under the start child's.
            let child_span = case.span - case.handle;
            let derived_max = case.s_min.max((child_span - case.e_min).max(1));
            assert_eq!(
                derived_max, case.max,
                "case: {} — the row says max_position is {}, but a paned of span {} with a \
                 {} px handle and {}/{} px child minimums reports {derived_max}. The row \
                 describes a paned GTK would never produce, so whatever it asserts below \
                 is about nothing.",
                case.desc, case.max, case.span, case.handle, case.s_min, case.e_min
            );

            let position = divider_position(case.s_min, derived_max, case.span, 0.5);
            assert_eq!(
                position, case.expected_position,
                "case: {} — divider_position({}, {derived_max}, {}, 0.5) = {position}, want \
                 {}. Equal weights must put the divider at half the SPAN; a position near \
                 half of {derived_max} means the scale is `max_position` again.",
                case.desc, case.s_min, case.span, case.expected_position
            );

            let start = position;
            let end = case.span - case.handle - position;
            if case.clamped {
                assert!(
                    position == derived_max || position == case.s_min.max(0),
                    "case: {} — this row is marked CLAMPED, so the position must be one of \
                     the two bounds ({} or {derived_max}), but it is {position}. If the \
                     clamp stopped binding here the row silently became an ordinary one \
                     and stopped covering the too-small-paned arm.",
                    case.desc,
                    case.s_min.max(0)
                );
            } else {
                assert!(
                    (start - end).abs() <= case.handle + 1,
                    "case: {} — a fresh split with EQUAL weights gave widths {start} and \
                     {end}, which differ by {}. The bound is the handle ({}) plus the one \
                     pixel `round` can move an odd span's midpoint. A spread near the end \
                     child's minimum ({}) is the signature of scaling by `max_position` \
                     instead of by the span: the start child gets `f * (span - handle - \
                     e_min)` and the end child keeps the rest, so the two differ by that \
                     minimum at EVERY fraction. Do not widen the bound.",
                    case.desc,
                    (start - end).abs(),
                    case.handle,
                    case.e_min
                );
            }
        }
    }

    /// Tolerance for a fraction this module COMPUTED (as opposed to copied), in
    /// fraction units. The arithmetic is one divide, so a tight epsilon is right.
    const FRACTION_EPS: f64 = 1e-9;

    /// Whether two optional fractions agree within [`FRACTION_EPS`], treating
    /// `None` as its own answer rather than as a number.
    fn approx_fraction(got: Option<f64>, want: Option<f64>) -> bool {
        match (got, want) {
            (None, None) => true,
            (Some(a), Some(b)) => (a - b).abs() < FRACTION_EPS,
            _ => false,
        }
    }

    /// One [`divider_span_fraction`] table row.
    struct SpanCase {
        desc: &'static str,
        position: i32,
        span: i32,
        expected: Option<f64>,
    }

    /// Pins [`divider_span_fraction`]: `None` for a degenerate span, and otherwise
    /// `position / span` clamped into `[0, 1]`.
    ///
    /// The `None` is the load-bearing half. This is the direction a USER DRAG
    /// travels — `notify::position` reads the pixel and the tree stores the
    /// fraction — so whatever this function answers for a paned that has not been
    /// allocated yet is what gets written into the layout's weights and persisted.
    /// `Some(0.0)` there would collapse the head child of a split the user never
    /// touched, and nothing downstream rejects a zero weight (see
    /// `layout::MIN_DIVIDER_FRACTION`).
    ///
    /// **NON-CLAIM, and it is the honest half of this table: no edit to this function
    /// alone can express the divider defect.** Its body is `position / denominator`
    /// whichever quantity the denominator is, so every row below passes with the
    /// argument renamed and nothing else changed — which is the concrete demonstration
    /// that the defect lived in what the CALLER passed, not here. The rows that can
    /// see it are [`equal_weights_give_equal_widths`] and `divider_position_table`'s
    /// "half the SPAN" row. Two rows below use [`TRAVEL`]'s 1000 as a span for exactly
    /// that reason: the function cannot tell, and pretending otherwise by renaming the
    /// literal would suggest coverage that is not here.
    ///
    /// FALSIFYING EDIT: return `Some(0.0)` instead of `None` when `span <= 0`.
    /// MEASURED against the PRE-rename function, whose message named
    /// `divider_travel_fraction` and whose first row said "travel": `case: max == 0
    /// has no travel to be a fraction OF — divider_travel_fraction(0, 0) = Some(0.0),
    /// want None`. The outcome is unchanged by the rename; only the quoted text is
    /// stale, so the current spelling is `Red run: owed.`
    #[test]
    fn divider_span_fraction_table() {
        let cases = [
            // --- No span means there is no fraction to report. `None` rather
            // --- than `Some(0.0)` is the whole point: a paned notifies `position`
            // --- before it has been allocated, and writing a 0.0 back into the
            // --- tree there would persist a collapsed pane the user never asked
            // --- for.
            SpanCase {
                desc: "span == 0 has nothing to be a fraction OF",
                position: 0,
                span: 0,
                expected: None,
            },
            SpanCase {
                desc: "a negative span likewise",
                position: 10,
                span: -10,
                expected: None,
            },
            // --- The ordinary case, and the two ends.
            SpanCase {
                desc: "half the span",
                position: 500,
                span: TRAVEL,
                expected: Some(0.5),
            },
            SpanCase {
                desc: "the near end",
                position: 0,
                span: TRAVEL,
                expected: Some(0.0),
            },
            SpanCase {
                desc: "the far end",
                position: TRAVEL,
                span: TRAVEL,
                expected: Some(1.0),
            },
            SpanCase {
                desc: "a non-round fraction",
                position: 1,
                span: 3,
                expected: Some(1.0 / 3.0),
            },
            // --- A REAL paned's numbers: a position of 1030 in a span of 1064 is
            // --- the weight a drag near the far end stores, and it is deliberately
            // --- ABOVE that paned's `max_position` of 1000 — this function knows
            // --- nothing of `max_position` and must not start clamping to it.
            SpanCase {
                // 1030 / 1064 = 0.968045...
                desc: "a position past max_position is still a fraction of the SPAN",
                position: 1030,
                span: SPAN,
                expected: Some(1030.0 / 1064.0),
            },
            // --- Out of range clamps rather than escaping [0, 1]: GTK can report a
            // --- position outside the span during an allocation pass, and a
            // --- weight outside [0, 1] would be rejected by `from_layout` at the
            // --- next launch.
            SpanCase {
                desc: "a position past the SPAN itself clamps to 1.0",
                position: 1100,
                span: SPAN,
                expected: Some(1.0),
            },
            SpanCase {
                desc: "a position past the far end clamps to 1.0",
                position: 1500,
                span: TRAVEL,
                expected: Some(1.0),
            },
            SpanCase {
                desc: "a negative position clamps to 0.0",
                position: -20,
                span: TRAVEL,
                expected: Some(0.0),
            },
        ];

        for case in &cases {
            let got = divider_span_fraction(case.position, case.span);
            assert!(
                approx_fraction(got, case.expected),
                "case: {} — divider_span_fraction({}, {}) = {got:?}, want {:?}",
                case.desc,
                case.position,
                case.span,
                case.expected
            );
        }
    }

    /// Slack added to the half-pixel round-trip bound, to absorb the rounding of
    /// `span * fraction` itself rather than of its result.
    const ROUND_TRIP_SLACK: f64 = 1e-12;

    /// [`divider_position`] and [`divider_span_fraction`] are inverses, to within
    /// the half pixel `round` can move a position.
    ///
    /// This is the test that stops the `max-position` re-apply drifting a weight.
    /// Every notify reads the tree's fraction, turns it into a pixel, and — on the
    /// next user drag — turns a pixel back into a fraction; a pair that lost half a
    /// pixel per pass would walk every divider across the window over a session.
    ///
    /// The bound is HALF a pixel of SPAN, which is exactly what `round` can cost
    /// and is what makes the test discriminating: `floor` can lose up to a WHOLE
    /// pixel, so a fraction whose scaled value sits past the .5 boundary catches it.
    ///
    /// **What this test does NOT establish, stated because it stayed green through
    /// the entire life of the defect:** the proof is scale-free. `round(d * (p / d))
    /// == p` holds for ANY shared denominator `d`, so agreement between the two
    /// directions is necessary and nowhere near sufficient — it says the pair are
    /// inverses, never that they are inverses about the right quantity. That second
    /// half is [`equal_weights_give_equal_widths`]'s, and the two must be read
    /// together. The backward loop's positions and expected results are unchanged
    /// across the span change for exactly this reason, which is also the concrete
    /// evidence that the inverse survived it.
    ///
    /// FALSIFYING EDIT: use `floor` instead of `round` in [`divider_position`]. The
    /// "just past a .5 pixel boundary" row then floors 1064 * 0.5009 = 532.9576 to
    /// 532 and reads back 0.5, which is 9e-4 out against a bound of ~4.7e-4. (A bound
    /// of one whole pixel could NOT catch it: `floor`'s error is always strictly less
    /// than one pixel, which is why the bound is half of one.) The recorded failure
    /// from the pre-`span` signature quoted a 1000-pixel travel and no longer matches
    /// anything this test prints, so it is struck rather than left to mislead.
    /// Red run: owed.
    #[test]
    fn divider_position_and_span_fraction_round_trip() {
        // Every forward case must land strictly INSIDE the clamp, and that is a
        // property of the rows rather than of the function: a clamped position is
        // deliberately not a round trip (the fraction it reads back is the clamp's,
        // not the caller's), and `clamped_fraction_snaps_once_and_then_holds` is
        // where that behaviour is pinned instead.
        let cases = [
            ("the midpoint", 0, TRAVEL, SPAN, 0.5),
            ("just past a .5 pixel boundary", 0, TRAVEL, SPAN, 0.5009),
            ("just below a .5 pixel boundary", 0, TRAVEL, SPAN, 0.4991),
            ("a dragged head fraction", 0, 998, SPAN, 0.7),
            ("the midpoint above a real minimum", MIN_POS, 998, SPAN, 0.5),
            (
                "a wide paned and an awkward fraction",
                0,
                65_471,
                65_535,
                0.123_456,
            ),
        ];

        for (desc, min, max, span, fraction) in cases {
            let position = divider_position(min, max, span, fraction);
            assert!(
                position >= min.max(0) && position <= max,
                "case {desc}: divider_position({min}, {max}, {span}, {fraction}) = {position}, \
                 outside [{}, {max}] — the table row itself is wrong, not the function. A \
                 forward row that clamps proves nothing about the inverse.",
                min.max(0)
            );

            let Some(back) = divider_span_fraction(position, span) else {
                panic!("case {desc}: divider_span_fraction({position}, {span}) reported no span");
            };
            let tolerance = 0.5 / f64::from(span) + ROUND_TRIP_SLACK;
            assert!(
                (back - fraction).abs() <= tolerance,
                "case {desc}: {fraction} -> position {position} -> {back}, which is {} out; at most \
                 {tolerance} (half a pixel of a {span}-pixel span) is allowed",
                (back - fraction).abs()
            );
        }

        // The other direction, which is the one the re-apply actually walks: a
        // position that came OUT of a paned must go back IN unchanged, so a
        // notify that changes nothing writes nothing back.
        //
        // These positions and their expected results are UNCHANGED from the
        // pre-`span` signature. That is the point of leaving them alone: the inverse
        // property is about a shared denominator, so changing which quantity the
        // denominator is cannot move it, and a row that had to be re-tuned here would
        // have meant the opposite.
        for (desc, min, max, span, position) in [
            ("the near end", 0, 998, SPAN, 0),
            ("one pixel in", 0, 998, SPAN, 1),
            ("exactly the minimum", MIN_POS, 998, SPAN, MIN_POS),
            ("a dragged position", MIN_POS, 998, SPAN, 700),
            ("the far end", 0, 998, SPAN, 998),
            ("a wide paned", 0, 65_471, 65_535, 12_345),
        ] {
            let Some(fraction) = divider_span_fraction(position, span) else {
                panic!("case {desc}: divider_span_fraction({position}, {span}) reported no span");
            };
            assert_eq!(
                divider_position(min, max, span, fraction),
                position,
                "case {desc}: position {position} -> fraction {fraction} -> a DIFFERENT position. \
                 The re-apply runs on every `max-position` notify, so a pixel lost here is a pixel \
                 lost per allocation pass."
            );
        }
    }

    /// A fraction the clamp has to move snaps ONCE and then holds.
    ///
    /// The round trip above only covers positions inside `[min, max]`, and it has to:
    /// a clamped position reads back as the CLAMP's fraction rather than the caller's,
    /// so the pair are not inverses there and no bound could make them so. What must
    /// hold instead is idempotence — the re-apply runs on every `max-position` notify,
    /// so a clamped divider that moved a little further on each pass would walk across
    /// the window over a session, which is the same failure the round trip exists to
    /// prevent, reached through the other door.
    ///
    /// The two clamped rows also assert that the fraction really DID change. Without
    /// that they would be satisfied by a function that never clamps at all, which is
    /// the vacuity this file's conventions exist to catch.
    ///
    /// **The wrong fix this test exists to stop**, because it is the one a later
    /// reader reaches for on seeing that a fraction near 1 cannot be expressed:
    /// rescaling into the reachable band, `low + fraction * (max - low)`. It looks
    /// like it removes the snap, and it reddens this test twice over — the unclamped
    /// row's position becomes 550 rather than 532, and idempotence breaks outright
    /// (550 reads back as 0.5169, which re-applies to 565). It also silently
    /// redefines what a weight means, since a fraction would then be of the reachable
    /// band rather than of the pane's width, which is the whole defect again in a new
    /// spelling. Red run: owed.
    #[test]
    fn clamped_fraction_snaps_once_and_then_holds() {
        for (desc, min, max, span, fraction, expected_p1, clamped) in [
            (
                "an unclamped fraction is already its own fixed point",
                MIN_POS,
                TRAVEL,
                SPAN,
                0.5,
                532,
                false,
            ),
            (
                "a fraction past max_position snaps DOWN once",
                MIN_POS,
                TRAVEL,
                SPAN,
                0.98,
                TRAVEL,
                true,
            ),
            (
                "a fraction under min_position snaps UP once",
                MIN_POS,
                TRAVEL,
                SPAN,
                0.0,
                MIN_POS,
                true,
            ),
        ] {
            let p1 = divider_position(min, max, span, fraction);
            assert_eq!(
                p1, expected_p1,
                "case {desc}: divider_position({min}, {max}, {span}, {fraction}) = {p1}, want \
                 {expected_p1}. The scale is the span and the clamp is [min, max]; a value \
                 between the two means the fraction was rescaled into the reachable band \
                 instead of clamped, which redefines what a weight is."
            );

            let Some(f1) = divider_span_fraction(p1, span) else {
                panic!("case {desc}: divider_span_fraction({p1}, {span}) reported no span");
            };
            let p2 = divider_position(min, max, span, f1);
            assert_eq!(
                p2, p1,
                "case {desc}: the clamped position {p1} read back as {f1} and re-applied to \
                 {p2}. The re-apply runs on every `max-position` notify, so a position that \
                 moves on each pass walks the divider across the window over a session."
            );

            if clamped {
                assert!(
                    (f1 - fraction).abs() > f64::EPSILON,
                    "case {desc}: this row is marked CLAMPED, so the fraction read back \
                     ({f1}) must differ from the one asked for ({fraction}) — the snap is \
                     the thing being tested. An equal pair means nothing clamped, and the \
                     idempotence assertion above then holds for the trivial reason."
                );
            }
        }
    }
}
