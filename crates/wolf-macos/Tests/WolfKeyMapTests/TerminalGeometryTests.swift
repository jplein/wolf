// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import XCTest

@testable import WolfKeyMap

final class TerminalGeometryTests: XCTestCase {

  // A cell whose point size is fractional, so rounding, not truncation, shows.
  private let cellWidth = 7.8
  private let cellHeight = 16.4

  // MARK: - size

  func testSizeAtScaleOne() {
    XCTAssertEqual(
      TerminalGeometry.size(
        cols: 100, rows: 30, cellWidth: cellWidth, cellHeight: cellHeight, scale: 1),
      TerminalSize(cols: 100, rows: 30, cellWidthPx: 8, cellHeightPx: 16),
      "at scale 1 a 7.8 x 16.4 point cell is 8 x 16 pixels, rounded")
  }

  func testSizeAtScaleTwo() {
    XCTAssertEqual(
      TerminalGeometry.size(
        cols: 100, rows: 30, cellWidth: cellWidth, cellHeight: cellHeight, scale: 2),
      TerminalSize(cols: 100, rows: 30, cellWidthPx: 16, cellHeightPx: 33),
      "at scale 2 the cell is 15.6 x 32.8 backing pixels, rounded to 16 x 33: 8 x 16 means the"
        + " backing scale is ignored, so a program sizing images by pixel draws them half size")
  }

  func testCellPixelsAreAtLeastOne() {
    let size = TerminalGeometry.size(cols: 10, rows: 10, cellWidth: 0.2, cellHeight: 0.3, scale: 1)
    XCTAssertEqual(
      [size.cellWidthPx, size.cellHeightPx], [1, 1],
      "a cell under half a pixel must still report 1 pixel: core keeps the previous cell size"
        + " for a 0, so the pointer would map against a stale cell")
  }

  func testCellCountsClampToTheRange() {
    let small = TerminalGeometry.size(
      cols: 0, rows: -3, cellWidth: cellWidth, cellHeight: cellHeight, scale: 1)
    XCTAssertEqual(
      [small.cols, small.rows], [1, 1],
      "a pane narrower or shorter than a cell must still size the session to at least 1 x 1:"
        + " core ignores a zero size, leaving the session at its old size")
    let huge = TerminalGeometry.size(
      cols: 70_000, rows: 70_000, cellWidth: cellWidth, cellHeight: cellHeight, scale: 1)
    XCTAssertEqual(
      [huge.cols, huge.rows], [UInt16.max, UInt16.max],
      "a count past UInt16.max must clamp rather than wrap or trap")
  }

  // MARK: - pointerPixels

  func testAPointInAColumnMapsIntoThatColumnsPixels() {
    let size = TerminalGeometry.size(
      cols: 80, rows: 24, cellWidth: cellWidth, cellHeight: cellHeight, scale: 2)
    let originX = 20.0
    let originY = 52.0
    // The middle of column 3, row 2, in view points.
    let p = TerminalGeometry.pointerPixels(
      x: originX + 3.5 * cellWidth, y: originY + 2.5 * cellHeight, originX: originX,
      originY: originY, cellWidth: cellWidth, cellHeight: cellHeight, size: size)
    let w = Double(size.cellWidthPx)
    let h = Double(size.cellHeightPx)
    XCTAssertTrue(
      p.x >= 3 * w && p.x < 4 * w,
      "the middle of column 3 must map into [3, 4) cell widths of \(w) pixels, not \(p.x):"
        + " core takes floor(x / cell_px) as the column, so a point past 4 cells means the"
        + " origin was not subtracted")
    XCTAssertTrue(
      p.y >= 2 * h && p.y < 3 * h,
      "the middle of row 2 must map into [2, 3) cell heights of \(h) pixels, not \(p.y)")
  }

  func testPointsBeforeTheOriginClampToZero() {
    let size = TerminalGeometry.size(
      cols: 80, rows: 24, cellWidth: cellWidth, cellHeight: cellHeight, scale: 1)
    let p = TerminalGeometry.pointerPixels(
      x: 5, y: 3, originX: 20, originY: 52, cellWidth: cellWidth, cellHeight: cellHeight,
      size: size)
    XCTAssertEqual(
      [p.x, p.y], [0, 0],
      "a pointer above or left of the content area must clamp to pixel 0, not go negative")
  }

  // MARK: - sizer

  func testTheActivePaneSizesWhenItShowsTheSession() {
    XCTAssertEqual(
      TerminalGeometry.sizer(showing: [4, 2, 9], active: 9, previous: 2), 9,
      "the active pane showing the session is the most recently focused, so it sizes it")
  }

  func testThePreviousSizerKeepsSizingWhenTheActivePaneShowsAnother() {
    XCTAssertEqual(
      TerminalGeometry.sizer(showing: [4, 2, 9], active: 7, previous: 9), 9,
      "with the active pane elsewhere, the pane that last sized the session keeps sizing it:"
        + " the lowest id (2) means `previous` is ignored, so the size jumps on every focus")
  }

  func testTheLowestPaneSizesWithNoActiveOrPrevious() {
    XCTAssertEqual(
      TerminalGeometry.sizer(showing: [4, 2, 9], active: nil, previous: 5), 2,
      "with neither the active nor the previous pane showing the session, the lowest id sizes")
  }

  func testNoPaneShowingTheSessionSizesNothing() {
    XCTAssertNil(
      TerminalGeometry.sizer(showing: [], active: 1, previous: 1),
      "a session no pane shows has no sizer")
  }
}
