//
//  HistorySeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.HistorySeam` namespace.
//
//  A NEW pure, AppKit-free seam (plan §B8) that (a) zips the three parallel
//  FFI-derived arrays behind `WolfCoreHandle::history_entries` into ordered menu
//  rows, and (b) owns the toolbar identifier / SF-Symbol constants plus the LOCKED
//  toolbar ordering. It mirrors the existing pure seams (`PickerSeam`,
//  `RestoreSeam`, `CwdTitleSeam`) and is exercised with scalar inputs only — NO
//  AppKit, NO NSToolbar, NO NSMenu, NO live Neovim — so it stays in the headless
//  `WolfKeyMapTests` bundle.
//
//  CONTRACT UNDER TEST (the SIGNATURE + shape the coder implements to satisfy):
//    * HistorySeam.Row — `Equatable`, with `title: String`, `index: Int`,
//      `isCurrent: Bool` (the synthesized memberwise init is reachable here via
//      `@testable import`).
//    * HistorySeam.rows(displays:indices:currentFlags:) -> [Row]
//      - zips the three arrays IN ORDER (the FFI hands them over newest-first;
//        the seam preserves that order verbatim — it never re-sorts).
//      - mismatched lengths TRUNCATE to the shortest array.
//      - a NEGATIVE index is DROPPED (it can never address a stack slot).
//      - at most ONE row keeps `isCurrent`; if several arrive, the FIRST wins.
//      - empty input yields [].
//    * HistorySeam.itemIdentifiers(toggleSidebar:trackingSeparator:) -> [String]
//      - exactly [toggleSidebar, trackingSeparator, back, forward], in that order
//        (the locked toolbar ordering after the traffic lights).
//    * The constants `emptyPlaceholderTitle`, `backIdentifier`,
//      `forwardIdentifier`, `backSymbolName`, `forwardSymbolName`.
//
//  ASSUMPTIONS (stated in the test-writer report; the coder implements to match):
//    * The "drop a negative index" rule and the "keep only the first isCurrent"
//      rule are asserted INDEPENDENTLY here — the tests deliberately do not pin
//      how they interact (i.e. whether a dropped row still consumes the
//      one-checkmark budget), because the plan does not specify it.
//    * `rows` does NOT clamp or renumber indices; a non-negative index rides
//      through verbatim (it is a STACK index handed straight back to
//      `history_jump`).
//
//  NOTE to coder: this test references the pure `WolfKeyMap.HistorySeam` enum the
//  coder adds in `Sources/WolfKeyMap/HistorySeam.swift` (and lists in the
//  `wolf_key_map` srcs in BUILD.bazel). This file must ALSO be listed in the
//  hand-maintained `srcs` of `//crates/wolf-macos:WolfKeyMapTests` — that list is
//  NOT a glob, so an unlisted file never compiles into the test binary and never
//  runs.
//

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
