//! Which rows of a terminal frame a pane's backing surface must repaint. Pure.

use wolf_core::Rgb;

/// Everything a painted row depends on besides its cells. When it changes, every row of the
/// backing surface is stale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintKey {
    pub cols: u16,
    pub rows: u16,
    pub default_bg: Rgb,
    pub cell_w: f64,
    pub cell_h: f64,
    /// The target's device scale, which the backing surface is created at.
    pub scale: f64,
}

/// The frame version the backing surface holds for `current`: `painted` when the surface was
/// painted under the same key, else `None`.
#[must_use]
pub fn effective_painted_version(
    previous: Option<PaintKey>,
    current: PaintKey,
    painted: Option<u64>,
) -> Option<u64> {
    if previous == Some(current) {
        painted
    } else {
        None
    }
}

/// The rows to repaint, ascending: every row when nothing is painted, else those whose version
/// is newer than `painted_version`.
#[must_use]
pub fn rows_to_repaint(row_versions: &[u64], painted_version: Option<u64>) -> Vec<u16> {
    (0..row_versions.len())
        .filter(|&row| painted_version.is_none_or(|painted| row_versions[row] > painted))
        .filter_map(|row| u16::try_from(row).ok())
        .collect()
}
