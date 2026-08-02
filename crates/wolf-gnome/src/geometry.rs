//! Pure pixel-to-grid geometry for the `wolf-gnome` window.
//!
//! This module is deliberately GTK/pango-free: it converts a widget's pixel
//! dimensions and a measured cell size into a Neovim grid size (cols, rows)
//! ([`grid_size_for`]), and a pointer position in pane-local pixels into the grid
//! cell it lands on ([`cell_at`]). That purity is what lets both conversions be
//! unit-tested headlessly, with no display server, no widget, and no nvim —
//! exactly mirroring how the `gdk_key`/`paint` seams stay testable in Tier 1.
//!
//! The two live in ONE file on purpose: the composition guarantee — that a cell
//! produced by [`cell_at`] is always inside the grid [`grid_size_for`] asked for —
//! must be provable (and is mechanically asserted) in a single place.

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
/// - `(x, y)` are ALREADY pane-local. GTK gesture/motion callbacks deliver
///   widget-local coordinates, and `pane_view::draw_pane` paints cell `(row, col)`
///   at `(col * cell_w, row * cell_h)` from the `DrawingArea`'s own origin, so
///   there is NO grid-origin subtraction here. `wolf-gnome` has no editor insets,
///   no 90-column cap, no centering and no per-pane status bar — those are
///   separate, still-unticked `feat-list.md` rows. **Do not import the macOS
///   `gridRect`/`paintOffset` machinery.** TRIPWIRE: if any of those rows is ever
///   implemented, this function grows a grid-origin parameter, and this paragraph
///   is where that is meant to be noticed.
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
}
