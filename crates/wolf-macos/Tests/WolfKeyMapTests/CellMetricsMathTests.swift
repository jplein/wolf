//
//  CellMetricsMathTests.swift
//  Pure, headless unit tests for the AppKit-/CoreText-free `WolfKeyMap.CellMetricsMath`.
//
//  These exercise the line-height + baseline math seam the coder extracts into
//  `Sources/WolfKeyMap/CellMetricsMath.swift` (plan §2). Like `ScrollAccumulatorTests`
//  and `PaneGeometryTests`, they import ONLY Foundation-level test machinery (no
//  AppKit / CoreText, no live font), and stay in the headless `WolfKeyMap` module.
//  This is the ONLY headless-testable slice of the macOS metrics work — font
//  registration, CTFont family resolution, and on-screen rendering are explicitly
//  manual-smoke only (plan §4).
//
//  ## CONTRACT UNDER TEST (the API the coder must honor, plan §2 / risk 6)
//
//    public enum CellMetricsMath {
//        // Cell height in points. For a POSITIVE line-height percentage this is the
//        // documented formula, INDEPENDENT of the raw font metrics:
//        //     round(fontSize * lineHeightPercent / 100)
//        // For a DEGENERATE lineHeightPercent <= 0 it falls back to the font's
//        // natural line height: ceil(ascent + descent + leading).
//        public static func cellHeight(
//            fontSize: Double, lineHeightPercent: Double,
//            ascent: Double, descent: Double, leading: Double) -> Double
//
//        // Baseline offset (points from the TOP of the cell) that keeps the glyph
//        // box vertically centered: the extra space (cellHeight - ascent - descent)
//        // is split evenly above and below, so
//        //     baseline = ascent + (cellHeight - ascent - descent) / 2
//        public static func baseline(
//            cellHeight: Double, ascent: Double, descent: Double) -> Double
//    }
//
//  All returns are exact-valued in these cases (integer or half-integer), so they
//  are asserted with a tight `eps` accuracy in the `approx` style of the sibling
//  headless tests.
//

import XCTest

@testable import WolfKeyMap

final class CellMetricsMathTests: XCTestCase {

    /// Tolerance for the exact-valued Double assertions below.
    private let eps: Double = 1e-9

    // MARK: - positive line-height -> round(fontSize * pct / 100)

    func testCellHeightAt130PercentRoundsFontScaledHeight() {
        // 14 pt at 130% -> 14 * 1.30 = 18.2 -> round -> 18. The raw ascent/descent/
        // leading are IGNORED on the positive path, so pass arbitrary values.
        let fontSize: Double = 14.0
        let lineHeightPercent: Double = 130.0
        XCTAssertEqual(
            CellMetricsMath.cellHeight(
                fontSize: fontSize, lineHeightPercent: lineHeightPercent,
                ascent: 11.0, descent: 3.0, leading: 0.0),
            18.0, accuracy: eps,
            "round(14 * 130 / 100) == round(18.2) == 18")
    }

    func testCellHeightAt100PercentEqualsRoundedFontSize() {
        // The natural 100% case: cell height is exactly the (rounded) font size.
        let fontSize: Double = 16.0
        XCTAssertEqual(
            CellMetricsMath.cellHeight(
                fontSize: fontSize, lineHeightPercent: 100.0,
                ascent: 12.0, descent: 4.0, leading: 1.0),
            16.0, accuracy: eps,
            "round(16 * 100 / 100) == 16")
    }

    // MARK: - degenerate line-height -> natural font metrics fallback

    func testDegenerateLineHeightFallsBackToFontMetrics() {
        // ascent + descent + leading == 16.0 (already integral, so ceil/round/raw
        // all agree). A zero AND a negative percentage must BOTH take the fallback.
        let ascent: Double = 12.0
        let descent: Double = 3.0
        let leading: Double = 1.0
        let expected: Double = 16.0  // ceil(12 + 3 + 1)

        XCTAssertEqual(
            CellMetricsMath.cellHeight(
                fontSize: 14.0, lineHeightPercent: 0.0,
                ascent: ascent, descent: descent, leading: leading),
            expected, accuracy: eps,
            "lineHeightPercent == 0 falls back to ceil(ascent + descent + leading)")

        XCTAssertEqual(
            CellMetricsMath.cellHeight(
                fontSize: 14.0, lineHeightPercent: -50.0,
                ascent: ascent, descent: descent, leading: leading),
            expected, accuracy: eps,
            "a negative lineHeightPercent also falls back to the font's natural height")
    }

    // MARK: - baseline recenters glyphs within the cell

    func testBaselineSplitsExtraLeadingEvenly() {
        // cellHeight 18, glyph box (ascent 12 + descent 4) == 16, so 2 pt of extra
        // space is split 1 pt above / 1 pt below -> baseline = ascent + 1 == 13.
        XCTAssertEqual(
            CellMetricsMath.baseline(cellHeight: 18.0, ascent: 12.0, descent: 4.0),
            13.0, accuracy: eps,
            "baseline == ascent + (cellHeight - ascent - descent) / 2 == 12 + 1")
    }

    func testBaselineEqualsAscentWhenCellIsExactGlyphBox() {
        // No extra space (cellHeight == ascent + descent) -> baseline sits exactly
        // at the ascent (glyph box fills the cell, nothing to split).
        XCTAssertEqual(
            CellMetricsMath.baseline(cellHeight: 16.0, ascent: 12.0, descent: 4.0),
            12.0, accuracy: eps,
            "with no extra leading the baseline is the ascent itself")
    }

    func testBaselineWithLargerCellCentersGlyphBox() {
        // cellHeight 20, glyph box 16 -> 4 pt extra, 2 above / 2 below -> baseline 14.
        XCTAssertEqual(
            CellMetricsMath.baseline(cellHeight: 20.0, ascent: 12.0, descent: 4.0),
            14.0, accuracy: eps,
            "baseline == 12 + (20 - 16) / 2 == 14")
    }
}
