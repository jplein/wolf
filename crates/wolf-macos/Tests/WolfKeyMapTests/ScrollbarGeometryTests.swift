// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import XCTest

@testable import WolfKeyMap

final class ScrollbarGeometryTests: XCTestCase {

  /// Tolerance for the exact-valued `Double` assertions below.
  private let eps: Double = 1e-9

  /// The worked base: an 840 x 680 pane at the origin, its grid inset by the 20 pt padding
  /// and a 32 pt statusbar. Literals, so a bug in `EditorInset` cannot move the inputs.
  private let basePane = Rect(x: 0, y: 0, w: 840, h: 680)
  private let baseGrid = Rect(x: 20, y: 52, w: 800, h: 608)

  /// The statusbar height the sweep builds its grids with, matching `baseGrid.y - padding`.
  private let statusbarHeight: Double = 32

  // MARK: - Helpers

  private func assertRect(
    _ got: Rect, _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ desc: String, file: StaticString = #filePath, line: UInt = #line
  ) {
    XCTAssertTrue(
      abs(got.x - x) < eps && abs(got.y - y) < eps && abs(got.w - width) < eps
        && abs(got.h - height) < eps,
      "\(desc): got \(got), want (x:\(x), y:\(y), w:\(width), h:\(height))",
      file: file, line: line)
  }

  /// The geometry, or a failure naming `desc` when the initializer returned `nil`.
  private func geometry(
    pane: Rect, grid: Rect, _ topline: UInt64, _ botline: UInt64, _ linecount: UInt64,
    _ desc: String, file: StaticString = #filePath, line: UInt = #line
  ) -> ScrollbarGeometry? {
    let g = ScrollbarGeometry(
      paneRect: pane, gridRect: grid, topline: topline, botline: botline, linecount: linecount)
    if g == nil {
      XCTFail(
        "\(desc): got nil for a scrollable viewport \(topline)/\(botline)/\(linecount) in pane "
          + "\(pane), grid \(grid); a guard rejects an ordinary window",
        file: file, line: line)
    }
    return g
  }

  /// Asserts the track lies right of the grid's text and inside the pane, so it covers no glyph.
  private func assertTrackInGutter(
    _ g: ScrollbarGeometry, pane: Rect, grid: Rect, _ desc: String,
    file: StaticString = #filePath, line: UInt = #line
  ) {
    XCTAssertGreaterThanOrEqual(
      g.track.x, grid.x + grid.w,
      "\(desc): the track's left edge \(g.track.x) is left of the grid's right edge "
        + "\(grid.x + grid.w), so the bar covers the last text column. The track is measured "
        + "from the grid rect instead of the pane's right edge.",
      file: file, line: line)
    XCTAssertLessThanOrEqual(
      g.track.x + g.track.w, pane.x + pane.w,
      "\(desc): the track's right edge \(g.track.x + g.track.w) is past the pane's right edge "
        + "\(pane.x + pane.w), so the bar paints into the neighbouring pane",
      file: file, line: line)
  }

  // MARK: - Projection

  func testTopOfBufferPutsThumbAtTopOfTrack() {
    guard let g = geometry(pane: basePane, grid: baseGrid, 0, 50, 200, "0/50/200") else { return }
    assertRect(
      g.track, 830, 54, 8, 604,
      "the track is 8 wide, 2 in from the pane's right edge, and spans the grid's height "
        + "less 2 at each end")
    assertRect(
      g.thumb, 830, 54, 8, 151,
      "50 of 200 lines from the top is a quarter of the 604 track, at the track's top")
  }

  func testMidBufferProjectsTopline() {
    guard let g = geometry(pane: basePane, grid: baseGrid, 100, 150, 200, "100/150/200") else {
      return
    }
    assertRect(
      g.thumb, 830, 356, 8, 151,
      "topline 100 of 200 must put the thumb's top halfway down the track (54 + 302); "
        + "another y means topline is not projected as topline / linecount")
  }

  func testRightHandSplitMeasuresFromItsOwnPane() {
    let pane = Rect(x: 500, y: 0, w: 400, h: 680)
    let grid = Rect(x: 520, y: 52, w: 360, h: 608)
    guard let g = geometry(pane: pane, grid: grid, 0, 50, 200, "right-hand split") else { return }
    assertRect(
      g.track, 890, 54, 8, 604,
      "a pane at x 500 puts its track at 900 - 2 - 8; 390 would mean the pane's origin "
        + "is dropped")
    assertTrackInGutter(g, pane: pane, grid: grid, "right-hand split")
  }

  // MARK: - Floor and end clamp

  func testTinyViewportAtEndFloorsAndClampsThumb() {
    guard
      let g = geometry(
        pane: basePane, grid: baseGrid, 99_989, 100_000, 100_000, "99989/100000/100000")
    else { return }
    assertRect(
      g.thumb, 830, 634, 8, 24,
      "11 of 100000 lines must lift the thumb to the 24 floor, and the end clamp must hold "
        + "its bottom at the track's bottom (658). A y near 657.9 means the end clamp is gone; "
        + "a height under 24 means the floor is gone.")
  }

  // MARK: - Short pane: the thumb never exceeds its track

  func testShortPaneThumbNeverExceedsItsTrack() {
    let pane = Rect(x: 0, y: 0, w: 840, h: 92)
    let grid = Rect(x: 20, y: 52, w: 800, h: 20)
    guard let g = geometry(pane: pane, grid: grid, 0, 1, 100, "short pane 0/1/100") else {
      return
    }
    assertRect(g.track, 830, 54, 8, 16, "a 20 tall grid leaves a 16 tall track")
    XCTAssertLessThanOrEqual(
      g.thumb.h, g.track.h,
      "the thumb (\(g.thumb.h)) is taller than its track (\(g.track.h)): the 24 floor is not "
        + "clamped to the track, so a short pane draws its thumb over the pane below")
    XCTAssertGreaterThanOrEqual(
      g.thumb.y, g.track.y, "the thumb's top \(g.thumb.y) is above the track's top \(g.track.y)")
    XCTAssertLessThanOrEqual(
      g.thumb.y + g.thumb.h, g.track.y + g.track.h,
      "the thumb's bottom \(g.thumb.y + g.thumb.h) is below the track's bottom "
        + "\(g.track.y + g.track.h), so it spills out of this pane")
    assertRect(g.thumb, 830, 54, 8, 16, "the clamped thumb fills exactly the 16 tall track")
  }

  // MARK: - No overlap: the track lies in the right padding gutter

  func testTrackCoversNoGlyph() {
    guard let base = geometry(pane: basePane, grid: baseGrid, 0, 50, 200, "base pane") else {
      return
    }
    assertTrackInGutter(base, pane: basePane, grid: baseGrid, "base pane")

    let narrowPane = Rect(x: 0, y: 0, w: 300, h: 680)
    let narrowGrid = Rect(x: 20, y: 52, w: 260, h: 608)
    guard let narrow = geometry(pane: narrowPane, grid: narrowGrid, 0, 50, 200, "narrow pane")
    else { return }
    assertTrackInGutter(narrow, pane: narrowPane, grid: narrowGrid, "narrow pane")
  }

  func testTrackStaysInGutterAcrossPaneWidths() {
    XCTAssertLessThanOrEqual(
      ScrollbarGeometry.inset + ScrollbarGeometry.thickness, EditorInset.padding,
      "the bar needs \(ScrollbarGeometry.inset + ScrollbarGeometry.thickness) pt of gutter but "
        + "the padding is \(EditorInset.padding) pt, so at some width it covers the last "
        + "text column")

    for paneW in [100.0, 320.0, 840.0, 2500.0] {
      let pane = Rect(x: 0, y: 0, w: paneW, h: 680)
      let inset = EditorInset(
        paneWidth: paneW, paneHeight: 680, statusbarHeight: statusbarHeight,
        padding: EditorInset.padding)
      guard let g = geometry(pane: pane, grid: inset.gridRect, 0, 11, 200, "pane width \(paneW)")
      else { continue }
      assertTrackInGutter(g, pane: pane, grid: inset.gridRect, "pane width \(paneW)")
    }
  }

  // MARK: - nil rows

  /// One input that must yield no scrollbar.
  private struct NilRow {
    let desc: String
    let pane: Rect
    let grid: Rect
    let topline: UInt64
    let botline: UInt64
    let linecount: UInt64
  }

  func testDegenerateInputsReturnNil() {
    let shortGrid = Rect(x: 20, y: 52, w: 800, h: 4)
    let narrowPane = Rect(x: 0, y: 0, w: 9, h: 680)
    let narrowGrid = Rect(x: 20, y: 52, w: 0, h: 608)
    let infinitePane = Rect(x: 0, y: 0, w: .infinity, h: 680)
    let nanGrid = Rect(x: 20, y: .nan, w: 800, h: 608)
    let rows: [NilRow] = [
      NilRow(
        desc: "linecount 0: an empty buffer has nothing to scroll",
        pane: basePane, grid: baseGrid, topline: 0, botline: 0, linecount: 0),
      NilRow(
        desc: "botline == topline: nothing visible",
        pane: basePane, grid: baseGrid, topline: 10, botline: 10, linecount: 200),
      NilRow(
        desc: "botline < topline: a malformed viewport (a UInt64 subtraction traps here)",
        pane: basePane, grid: baseGrid, topline: 50, botline: 10, linecount: 200),
      NilRow(
        desc: "visible == linecount: the whole buffer fits",
        pane: basePane, grid: baseGrid, topline: 0, botline: 200, linecount: 200),
      NilRow(
        desc: "grid 4 tall: the track is 0 tall",
        pane: basePane, grid: shortGrid, topline: 0, botline: 50, linecount: 200),
      NilRow(
        desc: "pane 9 wide: too narrow to hold the bar",
        pane: narrowPane, grid: narrowGrid, topline: 0, botline: 50, linecount: 200),
      NilRow(
        desc: "paneRect.w infinite",
        pane: infinitePane, grid: baseGrid, topline: 0, botline: 50, linecount: 200),
      NilRow(
        desc: "gridRect.y NaN",
        pane: basePane, grid: nanGrid, topline: 0, botline: 50, linecount: 200),
    ]
    for row in rows {
      let g = ScrollbarGeometry(
        paneRect: row.pane, gridRect: row.grid, topline: row.topline, botline: row.botline,
        linecount: row.linecount)
      XCTAssertNil(
        g, "\(row.desc): want no scrollbar, got \(String(describing: g)); a guard is missing")
    }
  }
}
