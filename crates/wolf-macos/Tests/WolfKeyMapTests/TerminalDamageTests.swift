// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import XCTest

@testable import WolfKeyMap

final class TerminalDamageTests: XCTestCase {

  private let key = TerminalDamage.PaintKey(
    cols: 80, rows: 24, defaultBg: 0x0011_1111, cellWidth: 8, cellHeight: 16, scale: 1)

  private func mark(row: Int, col: Int, span: Int = 1, style: UInt8 = 0)
    -> TerminalDamage.CursorMark
  {
    TerminalDamage.CursorMark(row: row, col: col, span: span, style: style)
  }

  // MARK: - rowsToRepaint

  func testNothingPaintedRepaintsEveryRow() {
    XCTAssertEqual(
      TerminalDamage.rowsToRepaint(rowVersions: [3, 1, 7, 2], paintedVersion: nil),
      [0, 1, 2, 3],
      "a view that has painted nothing must paint every row, whatever their versions")
  }

  func testOnlyRowsNewerThanThePaintedVersionRepaint() {
    XCTAssertEqual(
      TerminalDamage.rowsToRepaint(rowVersions: [3, 5, 7, 6, 4], paintedVersion: 5),
      [2, 3],
      "only rows whose version is past the painted version 5 repaint, ascending: row 1 at"
        + " version 5 repainting means the test is `>=`, and was already painted")
  }

  func testAnEqualVersionRepaintsNothing() {
    XCTAssertEqual(
      TerminalDamage.rowsToRepaint(rowVersions: [5, 5, 2], paintedVersion: 5), [],
      "rows at the painted version were painted at it, so none repaint")
  }

  // MARK: - effectivePaintedVersion

  func testAChangedKeyForgetsThePaintedVersion() {
    let variants: [(String, TerminalDamage.PaintKey)] = [
      (
        "cols",
        .init(cols: 81, rows: 24, defaultBg: 0x0011_1111, cellWidth: 8, cellHeight: 16, scale: 1)
      ),
      (
        "rows",
        .init(cols: 80, rows: 25, defaultBg: 0x0011_1111, cellWidth: 8, cellHeight: 16, scale: 1)
      ),
      (
        "default bg",
        .init(cols: 80, rows: 24, defaultBg: 0x0022_2222, cellWidth: 8, cellHeight: 16, scale: 1)
      ),
      (
        "cell width",
        .init(cols: 80, rows: 24, defaultBg: 0x0011_1111, cellWidth: 9, cellHeight: 16, scale: 1)
      ),
      (
        "cell height",
        .init(cols: 80, rows: 24, defaultBg: 0x0011_1111, cellWidth: 8, cellHeight: 17, scale: 1)
      ),
      (
        "scale",
        .init(cols: 80, rows: 24, defaultBg: 0x0011_1111, cellWidth: 8, cellHeight: 16, scale: 2)
      ),
    ]
    for (name, changed) in variants {
      XCTAssertNil(
        TerminalDamage.effectivePaintedVersion(previous: key, current: changed, painted: 9),
        "a change of \(name) must forget the painted version so every row repaints: keeping it"
          + " leaves rows painted at the old geometry or background")
    }
  }

  func testNoPreviousKeyForgetsThePaintedVersion() {
    XCTAssertNil(
      TerminalDamage.effectivePaintedVersion(previous: nil, current: key, painted: 9),
      "with no previous key nothing was painted under this key, so every row repaints")
  }

  func testAnEqualKeyKeepsThePaintedVersion() {
    XCTAssertEqual(
      TerminalDamage.effectivePaintedVersion(previous: key, current: key, painted: 9), 9,
      "an unchanged key must keep the painted version, so only damaged rows repaint")
  }

  // MARK: - cursorRows

  func testAMovedCursorRepaintsBothRows() {
    XCTAssertEqual(
      TerminalDamage.cursorRows(previous: mark(row: 7, col: 2), current: mark(row: 3, col: 0)),
      [3, 7],
      "a cursor that moved from row 7 to row 3 must repaint both, ascending: only [3] leaves"
        + " the old cursor painted on row 7")
  }

  func testACursorMovedWithinARowRepaintsThatRowOnce() {
    XCTAssertEqual(
      TerminalDamage.cursorRows(previous: mark(row: 4, col: 2), current: mark(row: 4, col: 9)),
      [4], "a cursor moved within row 4 repaints row 4 once")
  }

  func testACursorThatChangedOnlyItsShapeRepaintsItsRow() {
    XCTAssertEqual(
      TerminalDamage.cursorRows(
        previous: mark(row: 4, col: 2, style: 0), current: mark(row: 4, col: 2, style: 1)),
      [4], "a cursor that changed its shape in place repaints its row")
  }

  func testAnUnchangedCursorRepaintsNothing() {
    XCTAssertEqual(
      TerminalDamage.cursorRows(previous: mark(row: 4, col: 2), current: mark(row: 4, col: 2)),
      [], "an unchanged cursor repaints no row")
    XCTAssertEqual(
      TerminalDamage.cursorRows(previous: nil, current: nil), [],
      "a cursor absent before and after repaints no row")
  }

  func testAHiddenOrAppearingCursorRepaintsItsRow() {
    XCTAssertEqual(
      TerminalDamage.cursorRows(previous: mark(row: 5, col: 1), current: nil), [5],
      "a cursor that disappeared must repaint the row it was on, or it stays painted there")
    XCTAssertEqual(
      TerminalDamage.cursorRows(previous: nil, current: mark(row: 6, col: 1)), [6],
      "a cursor that appeared must repaint the row it is on")
  }
}
