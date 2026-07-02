//
//  CellMetricsMath.swift
//  Pure, AppKit-/CoreText-free line-height + baseline math for the grid cell.
//
//  This is the headless-testable seam extracted from `CellMetrics` (plan §2 /
//  risk 6): given a font size, a line-height percentage, and the raw font metrics
//  (ascent/descent/leading), it computes the cell height and the vertically
//  centered text baseline. It imports ONLY Foundation, so it lives in the
//  Foundation-only `WolfKeyMap` module and is unit-tested without a live font.
//

import Foundation

/// Pure cell-geometry math: line-height-driven cell height and the recentered
/// text baseline. No AppKit / CoreText, no live font — every input is a plain
/// `Double`, so the whole seam is deterministically unit-testable.
public enum CellMetricsMath {
    /// The cell height in points.
    ///
    /// For a POSITIVE `lineHeightPercent`, the height is driven purely by the font
    /// size, INDEPENDENT of the raw font metrics:
    ///
    ///     round(fontSize * lineHeightPercent / 100)
    ///
    /// For a DEGENERATE `lineHeightPercent <= 0` it falls back to the font's
    /// natural line height, `ceil(ascent + descent + leading)`.
    public static func cellHeight(
        fontSize: Double, lineHeightPercent: Double,
        ascent: Double, descent: Double, leading: Double
    ) -> Double {
        guard lineHeightPercent > 0 else {
            return (ascent + descent + leading).rounded(.up)
        }
        return (fontSize * lineHeightPercent / 100).rounded()
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
