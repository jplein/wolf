//! Pure, GTK-free cursor geometry and color logic for the `wolf-gnome` window.
//!
//! This module mirrors the `geometry.rs` pattern: it holds the cursor decisions
//! that can be reasoned about without a display server, a widget, or a live
//! Neovim — namely (1) the pixel rectangle to fill for a given [`CursorShape`]
//! and cell box, and (2) the (fill, glyph) color pair that keeps the cursor
//! reliably visible by honoring a well-defined cursor highlight or, failing that,
//! inverting the underlying cell (reverse video). Keeping these pure is what lets
//! them be unit-tested headlessly, exactly like `grid_size_for` and `paint`.
//!
//! `app.rs` is the only GTK glue: it computes the cell box from `CellMetrics`,
//! calls [`cursor_rect`]/[`cursor_colors`] here, and issues the opaque Cairo fill.

// =============================================================================
// NOTE to coder: this file was bootstrapped by the test-writer with ONLY the
// tests below. It WILL NOT COMPILE until you add the items the tests reference,
// exactly as `geometry.rs` was bootstrapped. Implement the following ABOVE the
// `#[cfg(test)] mod tests` block (do not change the test module):
//
//   Named constants (the tests pin these exact values):
//     - MIN_BAR_PX: f64 = 2.0      // minimum bar thickness in pixels
//     - DEFAULT_CELL_PCT: f64 = 0.25 // Neovim convention when cell_percentage is None
//     - PCT_DENOM: f64 = 100.0     // cell_percentage is a PERCENT (1–100)
//
//   pub fn cursor_rect(
//       shape: wolf_core::CursorShape,
//       cell_x: f64, cell_y: f64, cell_w: f64, cell_h: f64,
//       cell_percentage: Option<u32>,
//   ) -> (f64, f64, f64, f64)   // returns (x, y, w, h)
//     - Block      => (cell_x, cell_y, cell_w, cell_h), IGNORING cell_percentage.
//     - Vertical   => left bar: w = (cell_w * pct).max(MIN_BAR_PX).min(cell_w),
//                     full height; x = cell_x, y = cell_y, h = cell_h.
//     - Horizontal => bottom bar: h = (cell_h * pct).max(MIN_BAR_PX).min(cell_h),
//                     full width; x = cell_x, w = cell_w, y = cell_y + cell_h - h.
//     - pct = cell_percentage.map(|p| f64::from(p) / PCT_DENOM)
//                            .unwrap_or(DEFAULT_CELL_PCT)
//                            .clamp(0.0, 1.0);
//
//   pub fn cursor_colors(snap: &wolf_core::Snapshot)
//       -> (wolf_core::Rgb, wolf_core::Rgb)   // returns (fill, glyph)
//     - If mode_infos[cursor.mode_idx].attr_id resolves (via highlights.resolve)
//       to BOTH fg=Some AND bg=Some => HONOR it: (fill, glyph) = (bg, fg).
//     - Else INVERT the underlying cell: resolve grid.cell(row, col).hl_id;
//         fill  = style.fg or defaults.fg or FALLBACK_FG (white 255,255,255)
//         glyph = style.bg or defaults.bg or FALLBACK_BG (black 0,0,0)
//       Reuse render.rs's pub(crate) FALLBACK_FG / FALLBACK_BG (no drift).
//     - mode_idx out of range OR empty mode_infos => inversion path, no panic
//       (use `mode_infos.get(mode_idx)`).
// =============================================================================

use wolf_core::{CursorShape, Rgb, Snapshot};

use crate::render::{FALLBACK_BG, FALLBACK_FG};

/// Minimum bar thickness in pixels, so a tiny `cell_percentage` still renders a
/// visible bar cursor rather than a hairline.
pub const MIN_BAR_PX: f64 = 2.0;

/// Bar thickness as a fraction of the cell when `cell_percentage` is `None`
/// (Neovim's conventional default).
pub const DEFAULT_CELL_PCT: f64 = 0.25;

/// Divisor turning a `cell_percentage` PERCENT (1–100) into a 0.0–1.0 fraction.
pub const PCT_DENOM: f64 = 100.0;

/// Computes the pixel rectangle `(x, y, w, h)` to fill for `shape` within the cell
/// box `(cell_x, cell_y, cell_w, cell_h)`.
///
/// - [`CursorShape::Block`] fills the whole cell, ignoring `cell_percentage`.
/// - [`CursorShape::Vertical`] is a left bar of full height; its width is
///   `cell_percentage` of the cell (default [`DEFAULT_CELL_PCT`]), floored to
///   [`MIN_BAR_PX`] and capped at `cell_w`.
/// - [`CursorShape::Horizontal`] is a bottom bar of full width; its height follows
///   the same rule against `cell_h`, anchored to the cell's bottom edge.
#[must_use]
pub fn cursor_rect(
    shape: CursorShape,
    cell_x: f64,
    cell_y: f64,
    cell_w: f64,
    cell_h: f64,
    cell_percentage: Option<u32>,
) -> (f64, f64, f64, f64) {
    match shape {
        CursorShape::Block => (cell_x, cell_y, cell_w, cell_h),
        CursorShape::Vertical => {
            let pct = pct_fraction(cell_percentage);
            let w = (cell_w * pct).max(MIN_BAR_PX).min(cell_w);
            (cell_x, cell_y, w, cell_h)
        }
        CursorShape::Horizontal => {
            let pct = pct_fraction(cell_percentage);
            let h = (cell_h * pct).max(MIN_BAR_PX).min(cell_h);
            (cell_x, cell_y + cell_h - h, cell_w, h)
        }
    }
}

/// Computes the cursor's `(fill, glyph)` colors for `snap`.
///
/// The `WolfCursor` app-theme group ([`Snapshot::theme`]`.cursor`) is honored
/// FIRST and unconditionally (it is an explicit app-level opt-in): a set
/// `theme.cursor.bg` becomes the fill, with the glyph from `theme.cursor.fg` or
/// — when the theme fg is unset — the underlying cell's background (the
/// reverse-video glyph). This logic is kept in lockstep with wolf-ffi's
/// `cursor_colors` (its port of this function).
///
/// Otherwise the cursor inverts the underlying cell (reverse video) by default:
/// the fill is the cell's foreground and the glyph reads in the cell's
/// background, each falling back to the editor defaults (already merged by
/// [`HighlightTable::resolve`](wolf_core::HighlightTable::resolve)) and finally to
/// [`FALLBACK_FG`] / [`FALLBACK_BG`]. The cursor cell being out of grid bounds is
/// handled without panicking (defaults/fallbacks only).
///
/// The current mode's `attr_id` highlight is honored as `(fill, glyph) = (bg, fg)`
/// ONLY when it yields a demonstrably VISIBLE cursor: it must resolve to BOTH a
/// foreground `f` and a background `b`, with `b != cell_bg` (so the block is not
/// drawn dark-on-dark over an identically-colored cell) AND `b != f` (so the glyph
/// is not invisible against its own fill). Any other case falls through to
/// inversion, which is always visible by construction.
#[must_use]
pub fn cursor_colors(snap: &Snapshot) -> (Rgb, Rgb) {
    let defaults = &snap.highlights.defaults;

    // Underlying cell colors (the inversion source), computed once and reused as
    // the inversion result, the visibility reference for the mode-attr honor
    // path, and the glyph fallback for a bg-only WolfCursor theme.
    let (cell_fg, cell_bg) = match snap.active_grid().cell(snap.cursor.row, snap.cursor.col) {
        Some(cell) => {
            let style = snap.highlights.resolve(cell.hl_id);
            (
                style.fg.unwrap_or(FALLBACK_FG),
                style.bg.unwrap_or(FALLBACK_BG),
            )
        }
        None => (
            defaults.fg.unwrap_or(FALLBACK_FG),
            defaults.bg.unwrap_or(FALLBACK_BG),
        ),
    };

    // WolfCursor theme override: honored unconditionally (explicit app-level
    // opt-in, unlike the guarded mode-attr path below). A bg-only group keeps
    // the reverse-video glyph (the cell's bg).
    if let Some(bg) = snap.theme.cursor.bg {
        return (bg, snap.theme.cursor.fg.unwrap_or(cell_bg));
    }

    // Honor the mode's cursor highlight only when it is guaranteed visible.
    if let Some(attr_id) = snap
        .mode_infos
        .get(snap.cursor.mode_idx)
        .and_then(|mode| mode.attr_id)
    {
        let style = snap.highlights.resolve(attr_id);
        if let (Some(f), Some(b)) = (style.fg, style.bg) {
            if b != cell_bg && b != f {
                return (b, f);
            }
        }
    }

    (cell_fg, cell_bg)
}

/// Turns an optional `cell_percentage` PERCENT into a clamped 0.0–1.0 fraction,
/// defaulting to [`DEFAULT_CELL_PCT`] when absent.
fn pct_fraction(cell_percentage: Option<u32>) -> f64 {
    cell_percentage
        .map(|p| f64::from(p) / PCT_DENOM)
        .unwrap_or(DEFAULT_CELL_PCT)
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    use wolf_core::{
        Cell, Cursor, CursorShape, DefaultColors, HlAttr, ModeInfo, Rgb, Snapshot, ThemeGroup,
    };

    // --- Float comparison --------------------------------------------------

    /// Tolerance for comparing the `f64` rect components. The arithmetic here is
    /// a couple of multiplies/clamps, so a tight epsilon is appropriate.
    const EPS: f64 = 1e-9;

    /// Returns whether two floats are within [`EPS`].
    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    /// Asserts two `(x, y, w, h)` rects are component-wise approximately equal.
    fn assert_rect_eq(got: (f64, f64, f64, f64), want: (f64, f64, f64, f64), desc: &str) {
        assert!(
            approx(got.0, want.0)
                && approx(got.1, want.1)
                && approx(got.2, want.2)
                && approx(got.3, want.3),
            "case: {desc}: got {got:?}, want {want:?}"
        );
    }

    // --- cursor_rect fixture dims ------------------------------------------
    //
    // Deliberately NON-SQUARE (cell_w != cell_h) so width math and height math
    // are distinguishable, and a NON-ZERO origin so offsets are not assumed 0.

    /// Fixture cell width (px) — distinct from the height on purpose.
    const CELL_W: f64 = 20.0;
    /// Fixture cell height (px).
    const CELL_H: f64 = 16.0;
    /// Non-zero cell origin x, to prove offsets are propagated.
    const CELL_X: f64 = 40.0;
    /// Non-zero cell origin y, to prove offsets are propagated.
    const CELL_Y: f64 = 32.0;

    /// One `cursor_rect` table row.
    struct RectCase {
        desc: &'static str,
        shape: CursorShape,
        cell_x: f64,
        cell_y: f64,
        cell_w: f64,
        cell_h: f64,
        pct: Option<u32>,
        expected: (f64, f64, f64, f64),
    }

    #[test]
    fn cursor_rect_table() {
        let cases = [
            // --- Block: always the whole cell, cell_percentage IGNORED -------
            RectCase {
                desc: "block Some(25) ignores pct -> full cell",
                shape: CursorShape::Block,
                cell_x: CELL_X,
                cell_y: CELL_Y,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: Some(25),
                expected: (CELL_X, CELL_Y, CELL_W, CELL_H),
            },
            RectCase {
                desc: "block None -> full cell",
                shape: CursorShape::Block,
                cell_x: CELL_X,
                cell_y: CELL_Y,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: None,
                expected: (CELL_X, CELL_Y, CELL_W, CELL_H),
            },
            RectCase {
                desc: "block at origin (0,0) -> full cell",
                shape: CursorShape::Block,
                cell_x: 0.0,
                cell_y: 0.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: Some(25),
                expected: (0.0, 0.0, CELL_W, CELL_H),
            },
            // --- Vertical: left bar, full height -----------------------------
            RectCase {
                desc: "vertical Some(25) -> w = 0.25*20 = 5.0, x=cell_x, full height",
                shape: CursorShape::Vertical,
                cell_x: CELL_X,
                cell_y: CELL_Y,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: Some(25),
                expected: (CELL_X, CELL_Y, 5.0, CELL_H),
            },
            RectCase {
                desc: "vertical None -> default 0.25 -> w = 5.0",
                shape: CursorShape::Vertical,
                cell_x: CELL_X,
                cell_y: CELL_Y,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: None,
                expected: (CELL_X, CELL_Y, 5.0, CELL_H),
            },
            RectCase {
                desc: "vertical Some(1) -> 0.2px floored to MIN_BAR_PX",
                shape: CursorShape::Vertical,
                cell_x: CELL_X,
                cell_y: CELL_Y,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: Some(1),
                expected: (CELL_X, CELL_Y, MIN_BAR_PX, CELL_H),
            },
            RectCase {
                desc: "vertical Some(150) -> clamp 1.0 -> w capped at cell_w (not 30)",
                shape: CursorShape::Vertical,
                cell_x: CELL_X,
                cell_y: CELL_Y,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: Some(150),
                expected: (CELL_X, CELL_Y, CELL_W, CELL_H),
            },
            RectCase {
                desc: "vertical Some(25) at origin (0,0)",
                shape: CursorShape::Vertical,
                cell_x: 0.0,
                cell_y: 0.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: Some(25),
                expected: (0.0, 0.0, 5.0, CELL_H),
            },
            // --- Horizontal: bottom bar, full width --------------------------
            RectCase {
                desc: "horizontal Some(25) -> h = 0.25*16 = 4.0, y = cell_y+12, full width",
                shape: CursorShape::Horizontal,
                cell_x: CELL_X,
                cell_y: CELL_Y,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: Some(25),
                expected: (CELL_X, CELL_Y + CELL_H - 4.0, CELL_W, 4.0),
            },
            RectCase {
                desc: "horizontal None -> default 0.25 -> h = 4.0, y = cell_y+12",
                shape: CursorShape::Horizontal,
                cell_x: CELL_X,
                cell_y: CELL_Y,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: None,
                expected: (CELL_X, CELL_Y + CELL_H - 4.0, CELL_W, 4.0),
            },
            RectCase {
                desc: "horizontal Some(1) -> 0.16px floored to MIN_BAR_PX, y = cell_y+14",
                shape: CursorShape::Horizontal,
                cell_x: CELL_X,
                cell_y: CELL_Y,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: Some(1),
                expected: (CELL_X, CELL_Y + CELL_H - MIN_BAR_PX, CELL_W, MIN_BAR_PX),
            },
            RectCase {
                desc: "horizontal Some(150) -> clamp 1.0 -> h = cell_h, y = cell_y",
                shape: CursorShape::Horizontal,
                cell_x: CELL_X,
                cell_y: CELL_Y,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: Some(150),
                expected: (CELL_X, CELL_Y, CELL_W, CELL_H),
            },
            RectCase {
                desc: "horizontal Some(25) at origin (0,0) -> y = 12.0",
                shape: CursorShape::Horizontal,
                cell_x: 0.0,
                cell_y: 0.0,
                cell_w: CELL_W,
                cell_h: CELL_H,
                pct: Some(25),
                expected: (0.0, CELL_H - 4.0, CELL_W, 4.0),
            },
        ];

        for case in &cases {
            let got = cursor_rect(
                case.shape,
                case.cell_x,
                case.cell_y,
                case.cell_w,
                case.cell_h,
                case.pct,
            );
            assert_rect_eq(got, case.expected, case.desc);
        }
    }

    /// Pins the documented named constants so a drift in their values is caught
    /// here rather than silently changing the cursor geometry contract.
    #[test]
    fn named_constants_match_contract() {
        assert!(approx(MIN_BAR_PX, 2.0), "MIN_BAR_PX must be 2.0");
        assert!(
            approx(DEFAULT_CELL_PCT, 0.25),
            "DEFAULT_CELL_PCT must be 0.25"
        );
        assert!(approx(PCT_DENOM, 100.0), "PCT_DENOM must be 100.0");
    }

    // =====================================================================
    // cursor_colors
    // =====================================================================

    /// Grid columns in the color fixtures.
    const COLS: usize = 4;
    /// Grid rows in the color fixtures.
    const ROWS: usize = 2;
    /// Grid id used by the fixtures. `Snapshot::new` creates grid 1 and points
    /// `cursor.grid` at it, so making the fixture cell and cursor both live on
    /// grid 1 keeps grid 1 the ACTIVE grid (what `cursor_colors` reads via
    /// `active_grid()` after the S1d migration).
    const ACTIVE_GRID: u64 = 1;
    /// The cursor cell row used by the fixtures.
    const CURSOR_ROW: usize = 0;
    /// The cursor cell column used by the fixtures.
    const CURSOR_COL: usize = 1;

    /// Highlight id for the mode's cursor attr (the "honor" highlight).
    const MODE_HL: u64 = 10;
    /// Highlight id for the underlying cell (the "invert" source).
    const CELL_HL: u64 = 7;

    // Four maximally-distinct colors so honor-vs-invert and fill-vs-glyph are
    // unambiguous, plus the two fallbacks the contract pins.
    /// Mode cursor highlight foreground.
    const MODE_FG: Rgb = Rgb {
        r: 10,
        g: 20,
        b: 30,
    };
    /// Mode cursor highlight background.
    const MODE_BG: Rgb = Rgb {
        r: 40,
        g: 50,
        b: 60,
    };
    /// Underlying cell foreground.
    const CELL_FG: Rgb = Rgb {
        r: 70,
        g: 80,
        b: 90,
    };
    /// Underlying cell background.
    const CELL_BG: Rgb = Rgb {
        r: 100,
        g: 110,
        b: 120,
    };
    /// Contract fallback fill when nothing resolves: white.
    const FALLBACK_FG: Rgb = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    /// Contract fallback glyph when nothing resolves: black.
    const FALLBACK_BG: Rgb = Rgb { r: 0, g: 0, b: 0 };

    // Defaults-merged fixtures for the visibility-guard cases. These exercise the
    // production path where a Cursor highlight leaves a channel unset and the
    // editor defaults fill it in (the exact situation that re-introduced the
    // invisible cursor the guard must prevent).
    /// Editor default foreground (set via `set_defaults` in the guard cases).
    const DEFAULT_FG: Rgb = Rgb { r: 1, g: 2, b: 3 };
    /// Editor default background (set via `set_defaults` in the guard cases);
    /// deliberately distinct from `CELL_BG` so it is never mistaken for the
    /// underlying cell background.
    const DEFAULT_BG: Rgb = Rgb {
        r: 200,
        g: 210,
        b: 220,
    };
    /// A single color used for BOTH fg and bg to make a degenerate, invisible
    /// honored pair. Distinct from `CELL_BG` so the "equals background" guard is
    /// NOT what trips here — only the "fill == glyph" guard.
    const DEGENERATE: Rgb = Rgb {
        r: 77,
        g: 88,
        b: 99,
    };

    /// Builds a base snapshot whose cursor sits at `(CURSOR_ROW, CURSOR_COL)` on a
    /// cell carrying `cell_hl`. `CELL_HL` is always defined with distinct fg/bg so
    /// the inversion path produces `(CELL_FG, CELL_BG)`. No defaults are set, so an
    /// `hl_id` of 0 resolves to all-`None` (exercising the fallback path).
    fn base_snapshot(cell_hl: u64) -> Snapshot {
        let mut snap = Snapshot::new(COLS, ROWS);
        snap.highlights.define(
            CELL_HL,
            HlAttr {
                fg: Some(CELL_FG),
                bg: Some(CELL_BG),
                ..HlAttr::default()
            },
        );
        snap.grid_mut(ACTIVE_GRID).set_cell(
            CURSOR_ROW,
            CURSOR_COL,
            Cell {
                text: "x".to_string(),
                hl_id: cell_hl,
            },
        );
        snap.cursor = Cursor {
            grid: ACTIVE_GRID,
            row: CURSOR_ROW,
            col: CURSOR_COL,
            mode_idx: 0,
        };
        snap
    }

    /// A single-mode `mode_infos` whose only entry carries `attr_id`.
    fn one_mode(attr_id: Option<u64>) -> Vec<ModeInfo> {
        vec![ModeInfo {
            name: "test".to_string(),
            shape: CursorShape::Block,
            cell_percentage: None,
            attr_id,
        }]
    }

    #[test]
    fn cursor_colors_honors_mode_attr_with_both_fg_and_bg() {
        let mut snap = base_snapshot(CELL_HL);
        snap.highlights.define(
            MODE_HL,
            HlAttr {
                fg: Some(MODE_FG),
                bg: Some(MODE_BG),
                ..HlAttr::default()
            },
        );
        snap.mode_infos = one_mode(Some(MODE_HL));

        let (fill, glyph) = cursor_colors(&snap);
        // Honor path: fill = mode bg, glyph = mode fg.
        assert_eq!(fill, MODE_BG, "honor: fill should be the mode bg");
        assert_eq!(glyph, MODE_FG, "honor: glyph should be the mode fg");
        // And NOT the inversion of the underlying cell.
        assert_ne!(fill, CELL_FG, "honor path must not invert the cell");
    }

    #[test]
    fn cursor_colors_inverts_when_mode_attr_has_bg_only() {
        let mut snap = base_snapshot(CELL_HL);
        // bg-only: with no defaults, the resolved fg stays None, so the "both set"
        // honor condition fails and we fall through to inversion.
        snap.highlights.define(
            MODE_HL,
            HlAttr {
                bg: Some(MODE_BG),
                ..HlAttr::default()
            },
        );
        snap.mode_infos = one_mode(Some(MODE_HL));

        let (fill, glyph) = cursor_colors(&snap);
        assert_eq!(fill, CELL_FG, "inversion: fill = cell fg");
        assert_eq!(glyph, CELL_BG, "inversion: glyph = cell bg");
    }

    #[test]
    fn cursor_colors_inverts_when_mode_attr_has_fg_only() {
        let mut snap = base_snapshot(CELL_HL);
        // fg-only: resolved bg stays None (no defaults), honor condition fails.
        snap.highlights.define(
            MODE_HL,
            HlAttr {
                fg: Some(MODE_FG),
                ..HlAttr::default()
            },
        );
        snap.mode_infos = one_mode(Some(MODE_HL));

        let (fill, glyph) = cursor_colors(&snap);
        assert_eq!(fill, CELL_FG, "inversion: fill = cell fg");
        assert_eq!(glyph, CELL_BG, "inversion: glyph = cell bg");
    }

    #[test]
    fn cursor_colors_inverts_when_mode_attr_id_is_none() {
        let mut snap = base_snapshot(CELL_HL);
        snap.mode_infos = one_mode(None);

        let (fill, glyph) = cursor_colors(&snap);
        assert_eq!(fill, CELL_FG, "inversion: fill = cell fg");
        assert_eq!(glyph, CELL_BG, "inversion: glyph = cell bg");
    }

    #[test]
    fn cursor_colors_inverts_when_mode_idx_out_of_range() {
        let mut snap = base_snapshot(CELL_HL);
        // One mode entry, but the cursor points at index 5 -> .get() == None.
        snap.mode_infos = one_mode(Some(MODE_HL));
        snap.highlights.define(
            MODE_HL,
            HlAttr {
                fg: Some(MODE_FG),
                bg: Some(MODE_BG),
                ..HlAttr::default()
            },
        );
        snap.cursor.mode_idx = 5;

        // Must not panic, and must fall through to inversion (no honored mode).
        let (fill, glyph) = cursor_colors(&snap);
        assert_eq!(fill, CELL_FG, "OOR mode_idx: fill = cell fg");
        assert_eq!(glyph, CELL_BG, "OOR mode_idx: glyph = cell bg");
    }

    #[test]
    fn cursor_colors_inverts_when_mode_infos_empty() {
        let mut snap = base_snapshot(CELL_HL);
        snap.mode_infos = Vec::new();

        // Must not panic; empty mode_infos => inversion.
        let (fill, glyph) = cursor_colors(&snap);
        assert_eq!(fill, CELL_FG, "empty mode_infos: fill = cell fg");
        assert_eq!(glyph, CELL_BG, "empty mode_infos: glyph = cell bg");
    }

    #[test]
    fn cursor_colors_falls_back_when_cell_colors_unset() {
        // Cursor cell uses hl 0 with no defaults set => resolve yields all-None,
        // so the inversion path must reach the white/black fallbacks.
        let mut snap = base_snapshot(0);
        snap.mode_infos = one_mode(None);

        let (fill, glyph) = cursor_colors(&snap);
        assert_eq!(fill, FALLBACK_FG, "fallback fill must be white");
        assert_eq!(glyph, FALLBACK_BG, "fallback glyph must be black");
    }

    #[test]
    fn cursor_colors_falls_through_when_honored_fill_equals_background() {
        // Regression guard (core case): a Cursor highlight that sets ONLY a bg
        // equal to the underlying cell's background, with fg filled in from the
        // editor DEFAULTS, must NOT be honored. Honoring it would paint a dark
        // block over an identically-dark cell — an invisible cursor. The function
        // must fall through to inversion instead.
        //
        // "The background" here = resolve(CELL_HL).bg = CELL_BG (the cursor cell's
        // own highlight wins over defaults.bg).
        let mut snap = base_snapshot(CELL_HL);
        snap.highlights.set_defaults(DefaultColors {
            fg: Some(DEFAULT_FG),
            bg: Some(DEFAULT_BG),
            special: None,
        });
        // bg-ONLY mode hl, equal to the cell background. After resolve:
        //   fg = DEFAULT_FG (Some), bg = CELL_BG (Some) -> passes the old
        //   "both Some" gate, but the honored fill (CELL_BG) == cell background.
        snap.highlights.define(
            MODE_HL,
            HlAttr {
                bg: Some(CELL_BG),
                ..HlAttr::default()
            },
        );
        snap.mode_infos = one_mode(Some(MODE_HL));

        let (fill, glyph) = cursor_colors(&snap);
        // Must invert the cell, NOT return the invisible (CELL_BG, DEFAULT_FG).
        assert_eq!(fill, CELL_FG, "guard: fill must be the cell fg (inversion)");
        assert_eq!(
            glyph, CELL_BG,
            "guard: glyph must be the cell bg (inversion)"
        );
        assert_ne!(
            fill, CELL_BG,
            "guard: must not honor a fill equal to the cell background"
        );
    }

    #[test]
    fn cursor_colors_falls_through_when_honored_fill_equals_glyph() {
        // Degenerate Cursor hl: fg == bg makes an invisible glyph. Even though both
        // channels are Some (and bg != the cell background, so the "equals
        // background" guard does NOT apply), fill == glyph makes honoring pointless;
        // the function must fall through to inversion.
        let mut snap = base_snapshot(CELL_HL);
        snap.highlights.define(
            MODE_HL,
            HlAttr {
                fg: Some(DEGENERATE),
                bg: Some(DEGENERATE),
                ..HlAttr::default()
            },
        );
        snap.mode_infos = one_mode(Some(MODE_HL));

        let (fill, glyph) = cursor_colors(&snap);
        assert_eq!(
            fill, CELL_FG,
            "degenerate honor: fill = cell fg (inversion)"
        );
        assert_eq!(
            glyph, CELL_BG,
            "degenerate honor: glyph = cell bg (inversion)"
        );
        assert_ne!(
            fill, glyph,
            "inversion must yield a visible (fill != glyph) pair"
        );
    }

    #[test]
    fn cursor_colors_honors_distinct_mode_attr_with_defaults_set() {
        // Positive case proving the guard does NOT over-trigger in the production,
        // defaults-merged path: with editor defaults SET, a Cursor hl whose bg and
        // fg are both set, bg != fg, and bg != the underlying cell background
        // (CELL_BG), is still honored as (bg, fg).
        let mut snap = base_snapshot(CELL_HL);
        snap.highlights.set_defaults(DefaultColors {
            fg: Some(DEFAULT_FG),
            bg: Some(DEFAULT_BG),
            special: None,
        });
        snap.highlights.define(
            MODE_HL,
            HlAttr {
                // MODE_BG != CELL_BG and MODE_BG != MODE_FG, so both guards pass.
                fg: Some(MODE_FG),
                bg: Some(MODE_BG),
                ..HlAttr::default()
            },
        );
        snap.mode_infos = one_mode(Some(MODE_HL));

        let (fill, glyph) = cursor_colors(&snap);
        assert_eq!(
            fill, MODE_BG,
            "honor: fill = mode bg even with defaults set"
        );
        assert_eq!(
            glyph, MODE_FG,
            "honor: glyph = mode fg even with defaults set"
        );
        assert_ne!(
            fill, CELL_BG,
            "honored fill is distinct from the cell background"
        );
    }

    // =====================================================================
    // cursor_colors — WolfCursor theme override (Snapshot.theme.cursor)
    // =====================================================================
    //
    // The theme branch is the FIRST branch: a `theme.cursor.bg` of `Some(bg)` is
    // honored UNCONDITIONALLY (WolfCursor is an explicit app-level opt-in,
    // unlike the mode-attr path with its visibility guard), with
    // `glyph = theme.cursor.fg.unwrap_or(cell_bg)` (the reverse-video glyph when
    // the theme fg is unset). A `theme.cursor.bg` of `None` falls through to the
    // UNCHANGED existing logic, which every test above pins (all of them run
    // with the default all-`None` theme).

    /// WolfCursor theme foreground (the themed GLYPH color); distinct from every
    /// other fixture color so theme-vs-mode-vs-invert is unambiguous.
    const THEME_FG: Rgb = Rgb { r: 5, g: 15, b: 25 };
    /// WolfCursor theme background (the themed FILL color); distinct from every
    /// other fixture color.
    const THEME_BG: Rgb = Rgb {
        r: 130,
        g: 140,
        b: 150,
    };

    #[test]
    fn cursor_colors_honors_theme_cursor_fg_and_bg() {
        let mut snap = base_snapshot(CELL_HL);
        snap.mode_infos = one_mode(None);
        snap.theme.cursor = ThemeGroup {
            fg: Some(THEME_FG),
            bg: Some(THEME_BG),
        };

        let (fill, glyph) = cursor_colors(&snap);
        // Theme honor: fill = WolfCursor bg, glyph = WolfCursor fg.
        assert_eq!(fill, THEME_BG, "theme: fill should be the WolfCursor bg");
        assert_eq!(glyph, THEME_FG, "theme: glyph should be the WolfCursor fg");
        // And NOT the inversion of the underlying cell.
        assert_ne!(fill, CELL_FG, "the theme override must not invert the cell");
    }

    #[test]
    fn cursor_colors_theme_bg_only_uses_cell_bg_as_glyph() {
        let mut snap = base_snapshot(CELL_HL);
        snap.mode_infos = one_mode(None);
        // bg-only WolfCursor: the fill is still honored, and the glyph falls
        // back to the underlying cell's bg (the reverse-video glyph).
        snap.theme.cursor = ThemeGroup {
            fg: None,
            bg: Some(THEME_BG),
        };

        let (fill, glyph) = cursor_colors(&snap);
        assert_eq!(fill, THEME_BG, "bg-only theme: fill = WolfCursor bg");
        assert_eq!(
            glyph, CELL_BG,
            "bg-only theme: glyph falls back to the cell bg (reverse-video glyph)"
        );
    }

    #[test]
    fn cursor_colors_theme_beats_honorable_mode_attr() {
        // The mode attr here is exactly the honorable setup of
        // `cursor_colors_honors_mode_attr_with_both_fg_and_bg` (both channels
        // set, MODE_BG != CELL_BG, MODE_BG != MODE_FG). With the theme ALSO set,
        // the theme branch runs FIRST and must win over the mode attr.
        let mut snap = base_snapshot(CELL_HL);
        snap.highlights.define(
            MODE_HL,
            HlAttr {
                fg: Some(MODE_FG),
                bg: Some(MODE_BG),
                ..HlAttr::default()
            },
        );
        snap.mode_infos = one_mode(Some(MODE_HL));
        snap.theme.cursor = ThemeGroup {
            fg: Some(THEME_FG),
            bg: Some(THEME_BG),
        };

        let (fill, glyph) = cursor_colors(&snap);
        assert_eq!(
            fill, THEME_BG,
            "theme beats mode attr: fill = WolfCursor bg, not the mode bg"
        );
        assert_eq!(
            glyph, THEME_FG,
            "theme beats mode attr: glyph = WolfCursor fg, not the mode fg"
        );
        assert_ne!(
            fill, MODE_BG,
            "the mode-cursor bg must not leak through when the theme is set"
        );
    }
}
