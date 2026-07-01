//
//  PaneGeometryTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.PaneGeometry`.
//
//  These exercise the per-pane GRID-RECT-inside-PANE-RECT geometry seam the coder
//  adds in `Sources/WolfKeyMap/PaneGeometry.swift` (plan §4.1). Like
//  `PaneTreeTests.swift`, they import ONLY Foundation-level test machinery (no
//  AppKit / CoreGraphics, no live Neovim) and stay in the headless `WolfKeyMap`
//  module. They reuse the `eps` / `approx` style from `PaneTreeTests.swift`.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor, plan §4.1)
//
//    public struct PaneGeometry {
//        public let paneRect: Rect      // the whole pane area
//        public let gridRect: Rect      // where the nvim grid is drawn (== paneRect today)
//        public let cellWidth: Double
//        public let cellHeight: Double
//        public let cols: Int           // max(1, floor(gridRect.w / cellWidth))
//        public let rows: Int           // max(1, floor(gridRect.h / cellHeight))
//        public var paintOffsetX: Double // gridRect.x - paneRect.x (0 today)
//        public var paintOffsetY: Double // gridRect.y - paneRect.y (0 today)
//        public init(paneRect: Rect, cellWidth: Double, cellHeight: Double, maxCols: Int? = nil)
//        public func cellAt(x: Double, y: Double) -> (row: Int, col: Int)
//    }
//
//  `Rect` is the EXISTING type from `PaneTree.swift` (`Rect(x:y:w:h:)`, `Double`
//  fields). With `maxCols == nil` (ALWAYS today) `gridRect == paneRect` and both
//  paint offsets are 0. `cellAt` is CLAMPED into `[0, cols-1] x [0, rows-1]`:
//  margins/edges map to the nearest edge cell.
//

import XCTest

@testable import WolfKeyMap

final class PaneGeometryTests: XCTestCase {

    // MARK: - Cell size + bounds constants
    //
    // A non-square cell (W != H) so column math and row math are distinguishable.

    /// Test cell width (px).
    private let cellW: Double = 8.0
    /// Test cell height (px).
    private let cellH: Double = 16.0

    /// A wide pane width (px): 800 / 8 = 100 columns exactly.
    private let wideW: Double = 800.0
    /// A tall pane height (px): 480 / 16 = 30 rows exactly.
    private let tallH: Double = 480.0
    /// Expected columns for an 800px-wide pane at an 8px cell width.
    private let wideCols: Int = 100
    /// Expected rows for a 480px-tall pane at a 16px cell height.
    private let tallRows: Int = 30

    /// A non-zero pane origin x (px), to prove `cellAt` subtracts the origin.
    private let originX: Double = 200.0
    /// A non-zero pane origin y (px).
    private let originY: Double = 120.0

    /// Tolerance for comparing the `Double` rect / offset components (ported from
    /// `PaneTreeTests.swift`).
    private let eps: Double = 1e-9

    // MARK: - Helpers (ported from PaneTreeTests' `approx` / `assertRect`)

    /// Returns whether two doubles are within `eps`.
    private func approx(_ a: Double, _ b: Double) -> Bool {
        abs(a - b) < eps
    }

    /// Asserts a `Rect` matches another `Rect` component-by-component within `eps`.
    private func assertRectEqual(
        _ got: Rect,
        _ want: Rect,
        _ desc: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        XCTAssertTrue(
            approx(got.x, want.x) && approx(got.y, want.y)
                && approx(got.w, want.w) && approx(got.h, want.h),
            "case \(desc): got \(got), want \(want)",
            file: file,
            line: line
        )
    }

    // MARK: - gridRect == paneRect today (cap deferred)

    func testGridRectEqualsPaneRectToday() {
        // With maxCols nil (the only mode today), the grid fills the whole pane and
        // there is no paint offset — the future centered-narrow-grid is deferred.
        let pane = Rect(x: originX, y: originY, w: wideW, h: tallH)
        let geo = PaneGeometry(paneRect: pane, cellWidth: cellW, cellHeight: cellH)

        assertRectEqual(geo.paneRect, pane, "paneRect is stored verbatim")
        assertRectEqual(geo.gridRect, pane, "gridRect == paneRect when maxCols is nil")
        XCTAssertTrue(approx(geo.paintOffsetX, 0.0), "paintOffsetX is 0 today (got \(geo.paintOffsetX))")
        XCTAssertTrue(approx(geo.paintOffsetY, 0.0), "paintOffsetY is 0 today (got \(geo.paintOffsetY))")
    }

    // MARK: - cols / rows from cell size

    func testColsRowsFromCellSize() {
        // 800 x 480 at an 8 x 16 cell -> 100 columns, 30 rows.
        let pane = Rect(x: 0, y: 0, w: wideW, h: tallH)
        let geo = PaneGeometry(paneRect: pane, cellWidth: cellW, cellHeight: cellH)
        XCTAssertEqual(geo.cols, wideCols, "800px / 8px = 100 columns")
        XCTAssertEqual(geo.rows, tallRows, "480px / 16px = 30 rows")

        // A pane smaller than one cell still yields a 1 x 1 grid (floor clamped to 1).
        let tiny = Rect(x: 0, y: 0, w: 5.0, h: 5.0)
        let tinyGeo = PaneGeometry(paneRect: tiny, cellWidth: cellW, cellHeight: cellH)
        XCTAssertEqual(tinyGeo.cols, 1, "a sub-cell-wide pane clamps to 1 column")
        XCTAssertEqual(tinyGeo.rows, 1, "a sub-cell-tall pane clamps to 1 row")
    }

    // MARK: - cellAt interior

    func testCellAtInterior() {
        // A point 3.5 cells right and 2.5 cells down from the origin lands in the
        // cell at (row 2, col 3): floor(3.5) = 3, floor(2.5) = 2.
        let pane = Rect(x: 0, y: 0, w: wideW, h: tallH)
        let geo = PaneGeometry(paneRect: pane, cellWidth: cellW, cellHeight: cellH)

        let cell = geo.cellAt(x: 3.5 * cellW, y: 2.5 * cellH)
        XCTAssertEqual(cell.col, 3, "x = 3.5 cells -> col 3 (got \(cell.col))")
        XCTAssertEqual(cell.row, 2, "y = 2.5 cells -> row 2 (got \(cell.row))")
    }

    // MARK: - cellAt clamps the left / top margin to (0, 0)

    func testCellAtClampsLeftTopMargin() {
        // A point above and to the left of the grid (negative local coords) clamps to
        // the nearest edge cell (0, 0).
        let pane = Rect(x: 0, y: 0, w: wideW, h: tallH)
        let geo = PaneGeometry(paneRect: pane, cellWidth: cellW, cellHeight: cellH)

        let cell = geo.cellAt(x: -50.0, y: -50.0)
        XCTAssertEqual(cell.col, 0, "a point left of the grid clamps to col 0 (got \(cell.col))")
        XCTAssertEqual(cell.row, 0, "a point above the grid clamps to row 0 (got \(cell.row))")
    }

    // MARK: - cellAt clamps the right / bottom edge to (rows-1, cols-1)

    func testCellAtClampsRightBottomEdge() {
        // A point just inside the far corner clamps to the last cell, never cols/rows
        // (off-by-one guard at the maxX / maxY edge).
        let pane = Rect(x: 0, y: 0, w: wideW, h: tallH)
        let geo = PaneGeometry(paneRect: pane, cellWidth: cellW, cellHeight: cellH)

        let cell = geo.cellAt(x: pane.x + pane.w - 0.1, y: pane.y + pane.h - 0.1)
        XCTAssertEqual(cell.col, geo.cols - 1, "the far-right edge clamps to col cols-1 (got \(cell.col))")
        XCTAssertEqual(cell.row, geo.rows - 1, "the bottom edge clamps to row rows-1 (got \(cell.row))")
    }

    // MARK: - cellAt subtracts a non-zero pane origin

    func testCellAtClampsWithPaneOrigin() {
        // A pane at a non-zero origin must map by SUBTRACTING the origin: a point
        // 3.5 cells right / 2.5 cells down from the pane's own origin -> (row 2, col 3).
        let pane = Rect(x: originX, y: originY, w: wideW, h: tallH)
        let geo = PaneGeometry(paneRect: pane, cellWidth: cellW, cellHeight: cellH)

        let cell = geo.cellAt(x: originX + 3.5 * cellW, y: originY + 2.5 * cellH)
        XCTAssertEqual(cell.col, 3, "origin-relative x = 3.5 cells -> col 3 (got \(cell.col))")
        XCTAssertEqual(cell.row, 2, "origin-relative y = 2.5 cells -> row 2 (got \(cell.row))")

        // A point AT the pane origin maps to the first cell (0, 0).
        let corner = geo.cellAt(x: originX, y: originY)
        XCTAssertEqual(corner.col, 0, "the pane origin maps to col 0 (got \(corner.col))")
        XCTAssertEqual(corner.row, 0, "the pane origin maps to row 0 (got \(corner.row))")
    }
}
