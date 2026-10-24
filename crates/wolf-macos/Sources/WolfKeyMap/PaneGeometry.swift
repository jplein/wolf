// Pure grid-rect-inside-pane-rect coordinate seam, including the column cap.

import Foundation

/// The per-pane geometry: a Neovim GRID RECT positioned inside a PANE RECT, plus
/// the derived cell grid dimensions and the pixel<->cell mapping.
///
/// With `maxCols == nil`, or a pane no wider than `maxCols` cells, the grid fills
/// the whole pane: `gridRect == paneRect` and both paint offsets are 0. A wider
/// pane with a non-nil `maxCols` caps the grid width and centers it horizontally.
public struct PaneGeometry {
  /// The whole pane area, in device pixels.
  public let paneRect: Rect
  /// Where the Neovim grid is drawn (== `paneRect` today).
  public let gridRect: Rect
  /// Cell width, in device pixels.
  public let cellWidth: Double
  /// Cell height, in device pixels.
  public let cellHeight: Double
  /// Grid columns: `max(1, floor(gridRect.w / cellWidth))`.
  public let cols: Int
  /// Grid rows: `max(1, floor(gridRect.h / cellHeight))`.
  public let rows: Int
  /// Horizontal paint offset of the grid within the pane (`gridRect.x -
  /// paneRect.x`, 0 today).
  public let paintOffsetX: Double
  /// Vertical paint offset of the grid within the pane (`gridRect.y -
  /// paneRect.y`, 0 today).
  public let paintOffsetY: Double

  /// Builds the geometry for `paneRect` at the given cell size.
  ///
  /// `maxCols` is the max-column cap: `nil` disables it (grid fills the pane). When
  /// set, a pane wider than `maxCols` cells caps the grid to that width and centers
  /// it; a narrower pane is unaffected.
  public init(paneRect: Rect, cellWidth: Double, cellHeight: Double, maxCols: Int? = nil) {
    self.paneRect = paneRect
    self.cellWidth = cellWidth
    self.cellHeight = cellHeight

    // Cap the grid at `maxCols` cells, centered; with no cap or a narrower pane it fills the pane.
    if let maxCols, cellWidth > 0 {
      let cappedWidth = min(paneRect.w, Double(maxCols) * cellWidth)
      let inset = max(0, (paneRect.w - cappedWidth) / 2)
      gridRect = Rect(
        x: paneRect.x + inset, y: paneRect.y, w: cappedWidth, h: paneRect.h)
    } else {
      gridRect = paneRect
    }

    cols = cellWidth > 0 ? max(1, Int((gridRect.w / cellWidth).rounded(.down))) : 1
    rows = cellHeight > 0 ? max(1, Int((gridRect.h / cellHeight).rounded(.down))) : 1
    paintOffsetX = gridRect.x - paneRect.x
    paintOffsetY = gridRect.y - paneRect.y
  }

  /// Maps a pane-area point `(x, y)` (in device pixels, same space as
  /// `paneRect`) to a grid cell, CLAMPED into `[0, rows-1] x [0, cols-1]`.
  ///
  /// Points in the side/edge margins (or outside the grid) map to the nearest
  /// edge cell, so a click or scroll anywhere in the pane resolves to a valid
  /// cell. The grid origin is subtracted first, so a non-zero pane/grid origin is
  /// handled correctly.
  public func cellAt(x: Double, y: Double) -> (row: Int, col: Int) {
    let localX = x - gridRect.x
    let localY = y - gridRect.y
    let rawCol = cellWidth > 0 ? Int((localX / cellWidth).rounded(.down)) : 0
    let rawRow = cellHeight > 0 ? Int((localY / cellHeight).rounded(.down)) : 0
    let col = min(max(rawCol, 0), cols - 1)
    let row = min(max(rawRow, 0), rows - 1)
    return (row: row, col: col)
  }
}
