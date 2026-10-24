// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#historyseamtestsswift

import Foundation
import XCTest

@testable import WolfKeyMap

final class HistorySeamTests: XCTestCase {

  // MARK: - rows(): order-preserving zip

  func testRowsZipTheThreeArraysInOrder() {
    let rows = HistorySeam.rows(
      displays: ["c.md", "bar/README.md", "Preview: a.md"],
      indices: [4, 2, 1],
      currentFlags: [true, false, false])

    XCTAssertEqual(rows.count, 3, "one row per zipped triple")
    XCTAssertEqual(
      rows.map { $0.title }, ["c.md", "bar/README.md", "Preview: a.md"],
      "the newest-first order handed over by the FFI is preserved verbatim")
    XCTAssertEqual(
      rows.map { $0.index }, [4, 2, 1],
      "stack indices ride through unchanged (they are handed back to history_jump)")
    XCTAssertEqual(rows.map { $0.isCurrent }, [true, false, false], "flags ride through")
    XCTAssertEqual(
      rows[0], HistorySeam.Row(title: "c.md", index: 4, isCurrent: true),
      "Row is Equatable and carries all three fields")
  }

  // MARK: - rows(): mismatched lengths truncate to the shortest

  func testMismatchedLengthsTruncateToTheShortestArray() {
    let rows = HistorySeam.rows(
      displays: ["a.md", "b.md", "c.md"],
      indices: [0, 1],
      currentFlags: [false])

    XCTAssertEqual(rows.count, 1, "the shortest array (currentFlags) bounds the zip")
    XCTAssertEqual(rows[0].title, "a.md", "the surviving row is the FIRST triple")
    XCTAssertEqual(rows[0].index, 0)
    XCTAssertFalse(rows[0].isCurrent)
  }

  func testAnEmptyIndicesArrayYieldsNoRows() {
    let rows = HistorySeam.rows(
      displays: ["a.md", "b.md"], indices: [], currentFlags: [true, false])
    XCTAssertTrue(rows.isEmpty, "an empty parallel array truncates everything away")
  }

  // MARK: - rows(): a negative index is dropped

  func testNegativeIndexIsDropped() {
    let rows = HistorySeam.rows(
      displays: ["a.md", "bogus.md", "c.md"],
      indices: [0, -1, 2],
      currentFlags: [false, false, false])

    XCTAssertEqual(
      rows.map { $0.title }, ["a.md", "c.md"],
      "a negative index can never address a stack slot, so that row is dropped")
    XCTAssertEqual(
      rows.map { $0.index }, [0, 2],
      "the surviving rows keep their own indices (no renumbering)")
  }

  func testZeroIndexIsKept() {
    let rows = HistorySeam.rows(displays: ["a.md"], indices: [0], currentFlags: [false])
    XCTAssertEqual(rows.count, 1, "index 0 is the OLDEST stack slot, not a sentinel")
    XCTAssertEqual(rows[0].index, 0)
  }

  // MARK: - rows(): at most one isCurrent, the FIRST wins

  func testOnlyTheFirstCurrentFlagIsKept() {
    let rows = HistorySeam.rows(
      displays: ["a.md", "b.md", "c.md"],
      indices: [0, 1, 2],
      currentFlags: [false, true, true])

    XCTAssertEqual(
      rows.filter { $0.isCurrent }.count, 1,
      "the menu draws at most ONE checkmark")
    XCTAssertEqual(
      rows.map { $0.isCurrent }, [false, true, false],
      "when several current flags arrive, the FIRST one wins")
  }

  func testNoCurrentFlagIsAllowed() {
    // Core reports NO checkmark when the cursor's target falls outside the
    // distinct-row cap; the seam must pass that through, not synthesize one.
    let rows = HistorySeam.rows(
      displays: ["a.md", "b.md"], indices: [1, 0], currentFlags: [false, false])
    XCTAssertEqual(rows.count, 2)
    XCTAssertTrue(
      rows.allSatisfy { !$0.isCurrent },
      "no checkmark must be synthesized when core reports none")
  }

  // MARK: - rows(): empty input

  func testEmptyInputYieldsNoRows() {
    XCTAssertTrue(
      HistorySeam.rows(displays: [], indices: [], currentFlags: []).isEmpty,
      "an empty history yields no rows (the caller substitutes the placeholder)")
  }

  // MARK: - itemIdentifiers(): the locked toolbar ordering

  func testItemIdentifiersFollowTheLockedToolbarOrder() {
    let toggle = "com.wolf.editor.toolbar.toggleSidebar"
    let separator = "com.wolf.editor.toolbar.sidebarSeparator"

    let ids = HistorySeam.itemIdentifiers(toggleSidebar: toggle, trackingSeparator: separator)

    XCTAssertEqual(
      ids,
      [toggle, separator, HistorySeam.backIdentifier, HistorySeam.forwardIdentifier],
      "locked order: toggle sidebar -> sidebar tracking separator -> back -> forward")
    XCTAssertEqual(ids.count, 4, "exactly the four toolbar items")
  }

  // MARK: - constants

  func testIdentifierAndSymbolConstants() {
    XCTAssertEqual(HistorySeam.backIdentifier, "com.wolf.editor.toolbar.back")
    XCTAssertEqual(HistorySeam.forwardIdentifier, "com.wolf.editor.toolbar.forward")
    XCTAssertNotEqual(
      HistorySeam.backIdentifier, HistorySeam.forwardIdentifier,
      "the two toolbar items must be addressable separately")
    // RTL-aware chevrons (NOT chevron.left/right), per the architect's call.
    XCTAssertEqual(HistorySeam.backSymbolName, "chevron.backward")
    XCTAssertEqual(HistorySeam.forwardSymbolName, "chevron.forward")
    XCTAssertEqual(
      HistorySeam.emptyPlaceholderTitle, "No History",
      "the disabled placeholder keeps the History menu from ever being empty")
  }
}
