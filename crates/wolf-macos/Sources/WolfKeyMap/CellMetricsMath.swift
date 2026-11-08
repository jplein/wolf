// Pure, CoreText-free line-height and baseline math for the grid cell.

import Foundation

/// Pure cell-geometry math: line-height-driven cell height and the recentered
/// text baseline. No AppKit / CoreText, no live font — every input is a plain
/// `Double`, so the whole seam is deterministically unit-testable.
public enum CellMetricsMath {
  /// The floor, in points, under every `cellHeight` result.
  public static let minCellHeight: Double = 1

  /// The cell height in points.
  ///
  /// For a positive, finite `lineHeightPercent`, the height is driven purely by the
  /// font size, independent of the raw font metrics:
  ///
  ///     round(fontSize * lineHeightPercent / 100)
  ///
  /// For a non-positive or non-finite `lineHeightPercent` it falls back to the font's
  /// natural line height, `ceil(ascent + descent + leading)`.
  ///
  /// A result that is non-positive or non-finite is floored at `minCellHeight`, so a
  /// degenerate font never yields a zero, negative or NaN cell height.
  public static func cellHeight(
    fontSize: Double, lineHeightPercent: Double,
    ascent: Double, descent: Double, leading: Double
  ) -> Double {
    let height =
      lineHeightPercent.isFinite && lineHeightPercent > 0
      ? (fontSize * lineHeightPercent / 100).rounded()
      : (ascent + descent + leading).rounded(.up)
    return height.isFinite && height > 0 ? height : minCellHeight
  }

  /// The text baseline offset in points, measured from the TOP of the cell.
  ///
  /// The extra space (`cellHeight - ascent - descent`) is split evenly above and
  /// below the glyph box so glyphs sit vertically centered within the cell:
  ///
  ///     baseline = ascent + (cellHeight - ascent - descent) / 2
  public static func baseline(
    cellHeight: Double, ascent: Double, descent: Double
  ) -> Double {
    ascent + (cellHeight - ascent - descent) / 2
  }
}
