// Pure choice of the rows a terminal pane repaints for a new frame.

import Foundation

/// Decides which rows of a terminal pane a new frame damages, from the frame's per-row
/// versions and the version the pane last painted.
public enum TerminalDamage {
  /// Everything besides the cells that changes every row's pixels. A pane whose key
  /// changed repaints every row.
  public struct PaintKey: Equatable {
    public let cols: Int
    public let rows: Int
    public let defaultBg: UInt32
    public let cellWidth: Double
    public let cellHeight: Double
    public let scale: Double

    public init(
      cols: Int, rows: Int, defaultBg: UInt32, cellWidth: Double, cellHeight: Double,
      scale: Double
    ) {
      self.cols = cols
      self.rows = rows
      self.defaultBg = defaultBg
      self.cellWidth = cellWidth
      self.cellHeight = cellHeight
      self.scale = scale
    }
  }

  /// Where the cursor paints. An absent or invisible cursor has no mark.
  public struct CursorMark: Equatable {
    public let row: Int
    public let col: Int
    public let span: Int
    public let style: UInt8

    public init(row: Int, col: Int, span: Int, style: UInt8) {
      self.row = row
      self.col = col
      self.span = span
      self.style = style
    }
  }

  /// The painted version to damage against: `nil`, meaning nothing painted, when there
  /// was no previous key or it differs from `current`, else `painted`.
  public static func effectivePaintedVersion(
    previous: PaintKey?, current: PaintKey, painted: UInt64?
  ) -> UInt64? {
    previous == current ? painted : nil
  }

  /// The rows to repaint, ascending: every row for a `nil` painted version, else the rows
  /// whose version is past it.
  public static func rowsToRepaint(rowVersions: [UInt64], paintedVersion: UInt64?) -> [Int] {
    guard let paintedVersion else { return Array(rowVersions.indices) }
    return rowVersions.indices.filter { rowVersions[$0] > paintedVersion }
  }

  /// The rows a cursor change repaints, ascending and distinct: none when the mark is
  /// unchanged, else the rows of both marks.
  public static func cursorRows(previous: CursorMark?, current: CursorMark?) -> [Int] {
    if previous == current { return [] }
    return Array(Set([previous?.row, current?.row].compactMap { $0 })).sorted()
  }
}
