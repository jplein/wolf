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

  /// One pane's minimum height (px), the floor of both the drawer and the editor region. It
  /// sits under a third of 600 and 601, so the one-third cases above it keep their unfloored
  /// expectations.
  private let minPane: Double = 63.0

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
    minPaneHeight: Double,
    editor: Double,
    drawer: Double,
    _ desc: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    let dl = DrawerLayout(
      paneAreaHeight: paneAreaHeight, hasDrawer: hasDrawer,
      weight: weight, minPaneHeight: minPaneHeight)
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
      weight: DrawerLayout.defaultWeight, minPaneHeight: minPane)

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
      weight: DrawerLayout.defaultWeight, minPaneHeight: minPane)

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
      weight: DrawerLayout.defaultWeight, minPaneHeight: minPane)

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
      weight: DrawerLayout.defaultWeight, minPaneHeight: minPane)

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
      paneAreaHeight: 90, hasDrawer: true, weight: 1.0 / 3.0, minPaneHeight: 63,
      editor: 27, drawer: 63,
      "a third of 90 (30) floors at 63; drawer 30 means the floor is missing")
    assertSplit(
      paneAreaHeight: 600, hasDrawer: true, weight: -1, minPaneHeight: 63,
      editor: 537, drawer: 63,
      "a negative weight (-600) lifts to the floor; drawer 0 or -600 means the floor is missing")
  }

  /// A floor above the pane area takes the whole area. The cap must apply after the floor:
  /// the other order gives drawer 63 and editor -23, and a `ClosedRange` of 63...40 traps.
  func testFloorAbovePaneAreaTakesWholeAreaWithoutTrapping() {
    assertSplit(
      paneAreaHeight: 40, hasDrawer: true, weight: 1.0 / 3.0, minPaneHeight: 63,
      editor: 0, drawer: 40,
      "a floor of 63 over a 40 pane area caps at 40; drawer 63 / editor -23 means the cap ran "
        + "before the floor")
  }

  /// When the area holds both minimums, the drawer's cap leaves the editor its floor.
  func testWeightPastOneCapsAtTheEditorFloor() {
    assertSplit(
      paneAreaHeight: 600, hasDrawer: true, weight: 2, minPaneHeight: 63,
      editor: 63, drawer: 537,
      "round(600 * 2) == 1200 caps at 600 - 63; drawer 600 / editor 0 means the cap ignores the"
        + " editor's floor, drawer 1200 / editor -600 that the cap is missing")
  }

  func testNegativeFloorAndWeightStayAtZero() {
    assertSplit(
      paneAreaHeight: 600, hasDrawer: true, weight: -1, minPaneHeight: -50,
      editor: 600, drawer: 0,
      "a negative floor and weight keep the drawer at 0; drawer -50 or -600 means the lower "
        + "bound is not clamped at zero")
  }

  // MARK: - A drag on the drawer's top edge

  /// A pointer 200 into a 600 area leaves 400 below it, so the drawer takes 400/600.
  func testDraggedWeightIsTheShareBelowThePointer() {
    let weight = DrawerLayout.draggedWeight(pointerY: 200, paneAreaHeight: 600, minPaneHeight: 63)
    XCTAssertEqual(
      weight ?? .nan, 400.0 / 600.0, accuracy: eps,
      "a pointer at y 200 of 600 must give the share below it, 400/600; got"
        + " \(String(describing: weight)). 200/600 means the share above the pointer was taken, nil that the guard refused a"
        + " valid drag")
  }

  /// A pointer 20 above the bottom would leave a 20 drawer; the floor lifts it to 63.
  func testDraggedWeightHoldsTheDrawerMinimum() {
    let weight = DrawerLayout.draggedWeight(pointerY: 580, paneAreaHeight: 600, minPaneHeight: 63)
    XCTAssertEqual(
      weight ?? .nan, 63.0 / 600.0, accuracy: eps,
      "a pointer at y 580 of 600 must hold the drawer at its 63 minimum; got"
        + " \(String(describing: weight)). 20/600 means the drawer's floor is missing")
  }

  /// A pointer 10 below the top would leave the editor 10; the cap holds it at 63.
  func testDraggedWeightHoldsTheEditorMinimum() {
    let weight = DrawerLayout.draggedWeight(pointerY: 10, paneAreaHeight: 600, minPaneHeight: 63)
    XCTAssertEqual(
      weight ?? .nan, 537.0 / 600.0, accuracy: eps,
      "a pointer at y 10 of 600 must hold the editor at its 63 minimum, a 537/600 drawer; got"
        + " \(String(describing: weight)). 590/600 means the editor's floor is missing")
  }

  /// A 100 area cannot hold two 63 minimums, so a drag there keeps the remembered share.
  func testDraggedWeightIsNilWhenTooShortForBoth() {
    let weight = DrawerLayout.draggedWeight(pointerY: 50, paneAreaHeight: 100, minPaneHeight: 63)
    XCTAssertNil(
      weight,
      "an area of 100 is under two 63 minimums, so the drag must give nil (keep the share); got"
        + " \(String(describing: weight)). 0.37 means the too-short guard is missing and a"
        + " clamp inverted")
  }

  /// Every non-finite input, and an area of zero or below, gives nil rather than a share.
  func testDraggedWeightIsNilForNonFiniteInput() {
    // Each row: (name, pointerY, paneAreaHeight, minPaneHeight).
    let rows: [(String, Double, Double, Double)] = [
      ("NaN pointer", .nan, 600, 63),
      ("+inf pointer", .infinity, 600, 63),
      ("-inf pointer", -.infinity, 600, 63),
      ("NaN pane area", 200, .nan, 63),
      ("+inf pane area", 200, .infinity, 63),
      ("NaN minimum", 200, 600, .nan),
      ("+inf minimum", 200, 600, .infinity),
      ("zero pane area", 200, 0, 63),
      ("negative pane area", 200, -10, 63),
    ]
    for (name, pointerY, h, minimum) in rows {
      let weight = DrawerLayout.draggedWeight(
        pointerY: pointerY, paneAreaHeight: h, minPaneHeight: minimum)
      XCTAssertNil(
        weight,
        "\(name) must give nil (keep the share); got \(String(describing: weight)). NaN means"
          + " a finiteness check is missing, a clamped share such as 63/600 or 537/600 that a"
          + " non-finite pointer reached the clamps")
    }
  }

  /// A dragged share is a fraction of the area, so a taller area scales the drawer with it.
  func testDraggedShareSurvivesAPaneAreaResize() {
    guard
      let weight = DrawerLayout.draggedWeight(
        pointerY: 200, paneAreaHeight: 600, minPaneHeight: 63)
    else {
      XCTFail("premise: a drag at y 200 of 600 must give a share")
      return
    }
    assertSplit(
      paneAreaHeight: 900, hasDrawer: true, weight: weight, minPaneHeight: 63,
      editor: 300, drawer: 600,
      "the dragged 2/3 share of a 900 area is a 600 drawer; drawer 400 means an absolute height"
        + " was kept, drawer 300 that the weight was dropped for the default third")
    assertSplit(
      paneAreaHeight: 600, hasDrawer: true, weight: weight, minPaneHeight: 63,
      editor: 200, drawer: 400,
      "back at 600 the share gives the dragged 400 drawer again; drawer 200 means the weight was"
        + " dropped for the default third")
  }

  /// A share near the top, resized into a shorter area, still leaves the editor its floor.
  func testAResizeHoldsTheEditorAtItsMinimum() {
    guard
      let weight = DrawerLayout.draggedWeight(
        pointerY: 60, paneAreaHeight: 600, minPaneHeight: 63)
    else {
      XCTFail("premise: a drag at y 60 of 600 must give a share")
      return
    }
    XCTAssertEqual(
      weight, 537.0 / 600.0, accuracy: eps,
      "premise: a drag at y 60 of 600 holds the editor at 63, a 537/600 share; got \(weight)")
    assertSplit(
      paneAreaHeight: 400, hasDrawer: true, weight: weight, minPaneHeight: 63,
      editor: 63, drawer: 337,
      "round(400 * 0.895) == 358 must cap at 400 - 63; drawer 358 / editor 42 means the ceiling"
        + " ignores the editor's floor")
  }

  // MARK: - Degenerate input collapses both heights

  /// A non-finite weight, floor or pane-area height collapses both heights to 0. This is a
  /// separate test from the floor's: a dropped guard shows NaN or inf, or a silently lost floor.
  func testNonFiniteInputCollapsesBothHeights() {
    // Each row: (name, paneAreaHeight, hasDrawer, weight, minPaneHeight).
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
        paneAreaHeight: h, hasDrawer: hasDrawer, weight: weight, minPaneHeight: floor,
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
      paneAreaHeight: -10, hasDrawer: true, weight: 1.0 / 3.0, minPaneHeight: 63,
      editor: 0, drawer: 0,
      "a -10 pane area with a drawer collapses; drawer -10 means the non-positive guard is missing")
    assertSplit(
      paneAreaHeight: -10, hasDrawer: false, weight: 1.0 / 3.0, minPaneHeight: 63,
      editor: 0, drawer: 0,
      "a -10 pane area without a drawer collapses; editor -10 means the guard is missing or runs "
        + "after the hasDrawer branch")
    assertSplit(
      paneAreaHeight: 0, hasDrawer: true, weight: 1.0 / 3.0, minPaneHeight: 63,
      editor: 0, drawer: 0,
      "a zero pane area with a drawer gives (0, 0)")
  }

  /// S9: a saved share restores only when it is a fraction strictly inside the pane area, since a
  /// non-finite weight collapses the layout to zero heights and 0 or 1 leaves one side empty.
  func testARestoredShareIsKeptOnlyWhenItIsAFractionOfTheArea() {
    XCTAssertEqual(
      DrawerLayout.restoredWeight(0.5), 0.5,
      "a saved share of 0.5 must restore as 0.5; \(DrawerLayout.defaultWeight) means every"
        + " saved share is replaced by the default")
    XCTAssertEqual(
      DrawerLayout.restoredWeight(0.05), 0.05,
      "a small saved share inside (0, 1) must be kept; the minimum height floors it on layout")
    let rejected: [Double] = [.nan, .infinity, -.infinity, 0, 1, -0.2, 1.4]
    for share in rejected {
      XCTAssertEqual(
        DrawerLayout.restoredWeight(share), DrawerLayout.defaultWeight,
        "a saved share of \(share) is no fraction strictly between 0 and 1, so it must restore"
          + " as the default \(DrawerLayout.defaultWeight); \(share) itself means the share is"
          + " returned unchecked")
    }
  }
}
