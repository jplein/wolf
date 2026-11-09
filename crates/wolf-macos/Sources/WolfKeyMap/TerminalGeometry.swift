// Pure sizing of a terminal session from its pane, and pointer mapping into it.

import Foundation

/// The size a pane sends a session through `resize_terminal`: cells, and the cell in
/// backing pixels.
public struct TerminalSize: Equatable {
  public let cols: UInt16
  public let rows: UInt16
  public let cellWidthPx: UInt32
  public let cellHeightPx: UInt32
}

/// Sizes terminal sessions from their panes and maps pointer positions into the cell
/// pixels core expects.
public enum TerminalGeometry {
  /// The session size for a content area of `cols` by `rows` cells of `cellWidth` by
  /// `cellHeight` points at backing `scale`. Counts clamp to `1...UInt16.max`, because
  /// core ignores a zero size, and each pixel size is at least 1, because core keeps the
  /// previous cell size for a zero one.
  public static func size(
    cols: Int, rows: Int, cellWidth: Double, cellHeight: Double, scale: Double
  ) -> TerminalSize {
    let range = 1...Int(UInt16.max)
    return TerminalSize(
      cols: UInt16(min(max(cols, range.lowerBound), range.upperBound)),
      rows: UInt16(min(max(rows, range.lowerBound), range.upperBound)),
      cellWidthPx: pixels(cellWidth * scale),
      cellHeightPx: pixels(cellHeight * scale))
  }

  /// A cell dimension in whole pixels, at least 1.
  private static func pixels(_ value: Double) -> UInt32 {
    let rounded = value.rounded()
    guard rounded.isFinite, rounded >= 1 else { return 1 }
    return rounded >= Double(UInt32.max) ? UInt32.max : UInt32(rounded)
  }

  /// A pointer at `x`, `y` in view points, as pixels from the content origin in `size`'s
  /// cell pixels, clamped at 0. Core takes `floor(px / cell_px)` as the cell, so this
  /// names the cell under the pointer at any backing scale.
  public static func pointerPixels(
    x: Double, y: Double, originX: Double, originY: Double, cellWidth: Double,
    cellHeight: Double, size: TerminalSize
  ) -> (x: Double, y: Double) {
    let px = (x - originX) / cellWidth * Double(size.cellWidthPx)
    let py = (y - originY) / cellHeight * Double(size.cellHeightPx)
    return (max(0, px), max(0, py))
  }

  /// The pane that sizes a session among the panes of one view `showing` it: the active
  /// pane, else the pane that sized it last, else the lowest id; `nil` when none shows it.
  /// This is the spec's "most recently focused of its panes" within one view.
  public static func sizer(showing: [PaneId], active: PaneId?, previous: PaneId?) -> PaneId? {
    if let active, showing.contains(active) { return active }
    if let previous, showing.contains(previous) { return previous }
    return showing.min()
  }
}
