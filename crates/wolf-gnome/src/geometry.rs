//! Pure pixel-to-grid geometry for the `wolf-gnome` window.
//!
//! This module is deliberately GTK/pango-free: it converts a widget's pixel
//! dimensions and a measured cell size into a Neovim grid size (cols, rows). That
//! purity is what lets the conversion be unit-tested headlessly, with no display
//! server, no widget, and no nvim — exactly mirroring how the `gdk_key`/`paint`
//! seams stay testable in Tier 1.

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
}
