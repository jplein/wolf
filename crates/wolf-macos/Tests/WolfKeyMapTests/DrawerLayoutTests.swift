// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#drawerlayouttestsswift

import XCTest

@testable import WolfKeyMap

final class DrawerLayoutTests: XCTestCase {

  /// Tolerance for comparing the `Double` rect / scalar components (ported from
  /// `EditorInsetTests.swift`).
  private let eps: Double = 1e-9

  /// A representative pane-area height (px) evenly divisible by 3, so the
  /// one-third drawer weight lands on an exact integer with no rounding ambiguity.
  private let evenPaneAreaHeight: Double = 600.0

  /// A pane-area height (px) NOT divisible by 3, to exercise the `.rounded()` in
  /// `drawerHeight`: 601/3 == 200.333… -> 200.
  private let oddPaneAreaHeight: Double = 601.0

  /// A full canvas width (px) for the drawer strip.
  private let width: Double = 1000.0

  /// A deliberately tiny pane-area height (px) to pin the non-negative floor.
  private let tinyPaneAreaHeight: Double = 2.0

  /// The drawer's minimum height (px), one pane's minimum. It sits under a third of 600 and
  /// 601, so the one-third cases above it keep their unfloored expectations.
  private let minDrawer: Double = 63.0

  // MARK: - Helpers (ported from EditorInsetTests' `approx` / `assertRectEqual`)

  /// Returns whether two doubles are within `eps`.
  private func approx(_ a: Double, _ b: Double) -> Bool {
    abs(a - b) < eps
  }

  /// Builds a layout and asserts its two heights exactly. Every expectation is a whole number,
  /// and `==` is false for NaN, so a poisoned height fails as surely as a wrong one.
  private func assertSplit(
    paneAreaHeight: Double,
    hasDrawer: Bool,
    weight: Double,
    minDrawerHeight: Double,
    editor: Double,
    drawer: Double,
    _ desc: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    let dl = DrawerLayout(
      paneAreaHeight: paneAreaHeight, hasDrawer: hasDrawer,
      weight: weight, minDrawerHeight: minDrawerHeight)
    XCTAssertEqual(
      dl.editorHeight, editor,
      "\(desc): editorHeight must be \(editor), got \(dl.editorHeight)", file: file, line: line)
    XCTAssertEqual(
      dl.drawerHeight, drawer,
      "\(desc): drawerHeight must be \(drawer), got \(dl.drawerHeight)", file: file, line: line)
  }

  // MARK: - defaultWeight constant pinned

  func testDefaultWeightIsOneThird() {
    XCTAssertEqual(
      DrawerLayout.defaultWeight, 1.0 / 3.0, accuracy: eps,
      "DrawerLayout.defaultWeight is pinned at 1/3")
  }

  // MARK: - No drawer: editor fills the whole pane area, no drawer rect

  func testNoDrawerFillsEditorHeightAndNilRect() {
    let dl = DrawerLayout(
      paneAreaHeight: evenPaneAreaHeight, hasDrawer: false,
      weight: DrawerLayout.defaultWeight, minDrawerHeight: minDrawer)

    XCTAssertEqual(
      dl.editorHeight, evenPaneAreaHeight, accuracy: eps,
      "without a drawer the editor fills the full pane-area height")
    XCTAssertEqual(
      dl.drawerHeight, 0.0, accuracy: eps,
      "without a drawer the drawer height is 0")
    XCTAssertNil(
      dl.drawerRect(width: width),
      "without a drawer there is no drawer rect")
  }

  // MARK: - With drawer: one-third strip at the bottom, editor above

  func testWithDrawerOneThirdBottomStrip() {
    let dl = DrawerLayout(
      paneAreaHeight: evenPaneAreaHeight, hasDrawer: true,
      weight: DrawerLayout.defaultWeight, minDrawerHeight: minDrawer)

    // drawerHeight == (600 * 1/3).rounded() == 200; editor takes the rest.
    let expectedDrawerHeight = (evenPaneAreaHeight * DrawerLayout.defaultWeight).rounded()
    XCTAssertEqual(
      dl.drawerHeight, expectedDrawerHeight, accuracy: eps,
      "drawerHeight == round(paneAreaHeight * weight)")
    XCTAssertEqual(
      dl.editorHeight, evenPaneAreaHeight - dl.drawerHeight, accuracy: eps,
      "editorHeight == paneAreaHeight - drawerHeight")

    // The drawer rect is a full-width strip sitting directly below the editor.
    guard let rect = dl.drawerRect(width: width) else {
      XCTFail("a drawer layout must produce a drawer rect")
      return
    }
    XCTAssertEqual(rect.x, 0.0, accuracy: eps, "drawer rect is left-aligned (x == 0)")
    XCTAssertEqual(rect.w, width, accuracy: eps, "drawer rect is full-width (w == width)")
    XCTAssertEqual(
      rect.y, dl.editorHeight, accuracy: eps,
      "drawer rect sits at y == editorHeight (below the editor)")
    XCTAssertEqual(
      rect.h, dl.drawerHeight, accuracy: eps,
      "drawer rect height == drawerHeight")

    // The editor + drawer heights tile the whole pane area exactly.
    XCTAssertEqual(
      dl.editorHeight + dl.drawerHeight, evenPaneAreaHeight, accuracy: eps,
      "editor + drawer heights tile the full pane area")
  }

  // MARK: - Rounding: an odd height still rounds the one-third strip

  func testWithDrawerRoundsHeight() {
    let dl = DrawerLayout(
      paneAreaHeight: oddPaneAreaHeight, hasDrawer: true,
      weight: DrawerLayout.defaultWeight, minDrawerHeight: minDrawer)

    XCTAssertEqual(
      dl.drawerHeight, (oddPaneAreaHeight * DrawerLayout.defaultWeight).rounded(),
      accuracy: eps, "drawerHeight rounds paneAreaHeight * weight to a whole pixel")
    XCTAssertEqual(
      dl.editorHeight + dl.drawerHeight, oddPaneAreaHeight, accuracy: eps,
      "editor + drawer still tile the full (odd) pane area")
  }

  // MARK: - Edge: a tiny pane area keeps both heights non-negative

  func testTinyPaneAreaStaysNonNegative() {
    let dl = DrawerLayout(
      paneAreaHeight: tinyPaneAreaHeight, hasDrawer: true,
      weight: DrawerLayout.defaultWeight, minDrawerHeight: minDrawer)

    XCTAssertGreaterThanOrEqual(
      dl.editorHeight, 0.0, "editorHeight is never negative even for a tiny pane area")
    XCTAssertGreaterThanOrEqual(
      dl.drawerHeight, 0.0, "drawerHeight is never negative even for a tiny pane area")
  }

  // MARK: - The minimum-height floor

  /// A third of a short pane area is under one pane's minimum, which would give a strip
  /// that is allocated and focusable but shows no text; the floor lifts it to 63.
  func testShortPaneAreaFloorsDrawerAtMinimumHeight() {
    assertSplit(
      paneAreaHeight: 90, hasDrawer: true, weight: 1.0 / 3.0, minDrawerHeight: 63,
      editor: 27, drawer: 63,
      "a third of 90 (30) floors at 63; drawer 30 means the floor is missing")
    assertSplit(
      paneAreaHeight: 600, hasDrawer: true, weight: -1, minDrawerHeight: 63,
      editor: 537, drawer: 63,
      "a negative weight (-600) lifts to the floor; drawer 0 or -600 means the floor is missing")
  }

  /// A floor above the pane area takes the whole area. The cap must apply after the floor:
  /// the other order gives drawer 63 and editor -23, and a `ClosedRange` of 63...40 traps.
  func testFloorAbovePaneAreaTakesWholeAreaWithoutTrapping() {
    assertSplit(
      paneAreaHeight: 40, hasDrawer: true, weight: 1.0 / 3.0, minDrawerHeight: 63,
      editor: 0, drawer: 40,
      "a floor of 63 over a 40 pane area caps at 40; drawer 63 / editor -23 means the cap ran "
        + "before the floor")
  }

  func testWeightPastOneCapsAtPaneArea() {
    assertSplit(
      paneAreaHeight: 600, hasDrawer: true, weight: 2, minDrawerHeight: 63,
      editor: 0, drawer: 600,
      "round(600 * 2) == 1200 caps at 600; drawer 1200 / editor -600 means the cap is missing")
  }

  func testNegativeFloorAndWeightStayAtZero() {
    assertSplit(
      paneAreaHeight: 600, hasDrawer: true, weight: -1, minDrawerHeight: -50,
      editor: 600, drawer: 0,
      "a negative floor and weight keep the drawer at 0; drawer -50 or -600 means the lower "
        + "bound is not clamped at zero")
  }

  // MARK: - Degenerate input collapses both heights

  /// A non-finite weight, floor or pane-area height collapses both heights to 0. This is a
  /// separate test from the floor's: a dropped guard shows NaN or inf, or a silently lost floor.
  func testNonFiniteInputCollapsesBothHeights() {
    // Each row: (name, paneAreaHeight, hasDrawer, weight, minDrawerHeight).
    let rows: [(String, Double, Bool, Double, Double)] = [
      ("NaN weight", 600, true, .nan, 63),
      ("+inf weight", 600, true, .infinity, 63),
      ("NaN floor", 600, true, 1.0 / 3.0, .nan),
      ("+inf floor", 600, true, 1.0 / 3.0, .infinity),
      ("NaN pane area", .nan, true, 1.0 / 3.0, 63),
      ("+inf pane area", .infinity, true, 1.0 / 3.0, 63),
      // The guard runs before the hasDrawer branch, so it collapses a term that is never read.
      ("NaN weight with no drawer", 600, false, .nan, 63),
    ]
    for (name, h, hasDrawer, weight, floor) in rows {
      assertSplit(
        paneAreaHeight: h, hasDrawer: hasDrawer, weight: weight, minDrawerHeight: floor,
        editor: 0, drawer: 0,
        "\(name) must collapse both heights to 0; NaN or inf means the guard is missing, 200 or "
          + "537 that a NaN term was dropped by min/max, 600 that it ran after the hasDrawer branch"
      )
    }
  }

  /// A pane area of zero or below collapses both heights, with or without a drawer, so a
  /// negative area never yields a negative drawer or editor.
  func testNonPositivePaneAreaCollapses() {
    assertSplit(
      paneAreaHeight: -10, hasDrawer: true, weight: 1.0 / 3.0, minDrawerHeight: 63,
      editor: 0, drawer: 0,
      "a -10 pane area with a drawer collapses; drawer -10 means the non-positive guard is missing")
    assertSplit(
      paneAreaHeight: -10, hasDrawer: false, weight: 1.0 / 3.0, minDrawerHeight: 63,
      editor: 0, drawer: 0,
      "a -10 pane area without a drawer collapses; editor -10 means the guard is missing or runs "
        + "after the hasDrawer branch")
    assertSplit(
      paneAreaHeight: 0, hasDrawer: true, weight: 1.0 / 3.0, minDrawerHeight: 63,
      editor: 0, drawer: 0,
      "a zero pane area with a drawer gives (0, 0)")
  }
}
