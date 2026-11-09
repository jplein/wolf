//! Sizing a terminal session from its pane, and mapping a pointer into the session's cell
//! pixels. Pure.

use crate::editor_inset::GridRect;
use crate::layout::PaneId;

/// What a pane sends a session through `resize_terminal`: cells, and the cell in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_w_px: u32,
    pub cell_h_px: u32,
}

/// The session size for `cols` by `rows` cells of `cell_w` by `cell_h` pixels. Counts are at
/// least 1, because core ignores a zero size, and each pixel size is rounded and at least 1,
/// because core keeps the previous cell size for a zero one.
#[must_use]
pub fn size(cols: u16, rows: u16, cell_w: f64, cell_h: f64) -> TerminalSize {
    TerminalSize {
        cols: cols.max(1),
        rows: rows.max(1),
        cell_w_px: whole_pixels(cell_w),
        cell_h_px: whole_pixels(cell_h),
    }
}

/// `value` rounded to whole pixels, at least 1, saturating at `u32::MAX`; 1 when not finite.
fn whole_pixels(value: f64) -> u32 {
    let rounded = value.round();
    if !rounded.is_finite() || rounded < 1.0 {
        return 1;
    }
    if rounded >= f64::from(u32::MAX) {
        return u32::MAX;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "rounded is finite and in [1, u32::MAX), so the cast is exact"
    )]
    {
        rounded as u32
    }
}

/// The pane-local point `x`, `y` as pixels from `rect`'s origin in `size`'s cell pixels,
/// clamped at 0, a non-finite result read as 0. Core takes `floor(px / cell_px)` as the cell,
/// so this names the cell under the pointer whatever the painted cell size.
#[must_use]
pub fn pointer_pixels(
    x: f64,
    y: f64,
    rect: GridRect,
    cell_w: f64,
    cell_h: f64,
    size: TerminalSize,
) -> (f32, f32) {
    let px = (x - rect.x) / cell_w * f64::from(size.cell_w_px);
    let py = (y - rect.y) / cell_h * f64::from(size.cell_h_px);
    (clamped(px), clamped(py))
}

/// `value` as `f32`, at least 0, with a non-finite value read as 0.
fn clamped(value: f64) -> f32 {
    if !value.is_finite() || value <= 0.0 {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a pixel offset loses only precision it never had, and saturates past f32::MAX"
    )]
    {
        value as f32
    }
}

/// The pane that sizes a session among the panes `showing` it: the active pane, else the pane
/// that sized it last, else the lowest id; `None` when none shows it.
#[must_use]
pub fn sizer(
    showing: &[PaneId],
    active: Option<PaneId>,
    previous: Option<PaneId>,
) -> Option<PaneId> {
    if let Some(active) = active.filter(|pane| showing.contains(pane)) {
        return Some(active);
    }
    if let Some(previous) = previous.filter(|pane| showing.contains(pane)) {
        return Some(previous);
    }
    showing.iter().copied().min()
}
