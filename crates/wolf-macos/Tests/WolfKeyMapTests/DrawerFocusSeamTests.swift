//
//  DrawerFocusSeamTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.DrawerFocusSeam`.
//
//  These exercise the terminal-drawer FOCUS-SELECTION seam the coder adds in
//  `Sources/WolfKeyMap/DrawerFocusSeam.swift` (terminal-drawer navigation addendum
//  §1/§5). The drawer stays OUT of the pure `PaneTree` (preserving its 1:1 parity
//  with wolf-gnome), so editor<->drawer `<C-w>j`/`<C-w>k` navigation is a front-end
//  bridge computed from leaf rects rather than a tree neighbour. This seam is that
//  pure computation; the `applyFocus` AppKit glue that calls it is compile +
//  manual-smoke only.
//
//  Like `PaneTreeTests` / `DrawerLayoutTests`, they import ONLY Foundation-level
//  test machinery (no AppKit / CoreGraphics, no live Neovim) and stay in the
//  headless `WolfKeyMap` module.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor, addendum §1)
//
//    public enum DrawerFocusSeam {
//        // The leaf with the greatest `y + h` (top-left origin -> larger y is
//        // lower); tie-break: smallest `x`, then smallest `PaneId`. nil on empty.
//        public static func bottomMostLeaf(_ rects: [(PaneId, Rect)]) -> PaneId?
//        // `lastEditorPane` if non-nil AND present in `leaves`; otherwise the
//        // `bottomMostLeaf(rects)` fallback.
//        public static func drawerUpTarget(
//            lastEditorPane: PaneId?, leaves: [PaneId], rects: [(PaneId, Rect)]
//        ) -> PaneId?
//    }
//
//  `Rect` / `PaneId` are the EXISTING types from `PaneTree.swift` (`Rect(x:y:w:h:)`,
//  `Double` fields; `PaneId == UInt64`).
//
//  NOTE (coder): `DrawerFocusSeam` does NOT exist yet — this file will not compile
//  until the coder adds it (by design). Test-writer references it as the contract.
//

import XCTest

@testable import WolfKeyMap

final class DrawerFocusSeamTests: XCTestCase {

  // MARK: - Pane ids (small, distinct; < 5 so no magic-number concerns)

  private let p1: PaneId = 1
  private let p2: PaneId = 2
  private let p3: PaneId = 3
  private let p4: PaneId = 4

  // MARK: - Bounds used across the geometry fixtures (mirrors PaneTreeTests)

  /// Test bounds width (px).
  private let w: Double = 100.0
  /// Test bounds height (px).
  private let h: Double = 40.0
  /// Half the width — a 0.5 Vertical split boundary.
  private let halfW: Double = 50.0
  /// Half the height — a 0.5 Horizontal split boundary.
  private let halfH: Double = 20.0

  // MARK: - Geometry fixtures

  /// A single leaf filling the whole 100x40 bounds.
  private func singleLeaf() -> [(PaneId, Rect)] {
    [(p1, Rect(x: 0, y: 0, w: w, h: h))]
  }

  /// A vertical 2-stack: p1 on top, p2 below (p2 has the greater `y + h`).
  private func verticalStack() -> [(PaneId, Rect)] {
    [
      (p1, Rect(x: 0, y: 0, w: w, h: halfH)),
      (p2, Rect(x: 0, y: halfH, w: w, h: halfH)),
    ]
  }

  /// A clean 2x2 grid:
  ///   p1 top-left   p2 top-right
  ///   p3 bottom-left p4 bottom-right
  /// The bottom row (p3, p4) shares the greatest `y + h`; the smallest-`x`
  /// tie-break selects the bottom-LEFT p3.
  private func grid2x2() -> [(PaneId, Rect)] {
    [
      (p1, Rect(x: 0, y: 0, w: halfW, h: halfH)),
      (p2, Rect(x: halfW, y: 0, w: halfW, h: halfH)),
      (p3, Rect(x: 0, y: halfH, w: halfW, h: halfH)),
      (p4, Rect(x: halfW, y: halfH, w: halfW, h: halfH)),
    ]
  }

  // MARK: - bottomMostLeaf

  func testBottomMostLeafSingle() {
    XCTAssertEqual(
      DrawerFocusSeam.bottomMostLeaf(singleLeaf()), p1,
      "a single leaf is its own bottom-most leaf")
  }

  func testBottomMostLeafVerticalStack() {
    XCTAssertEqual(
      DrawerFocusSeam.bottomMostLeaf(verticalStack()), p2,
      "in a vertical stack the lower leaf (greater y+h) is bottom-most")
  }

  func testBottomMostLeafGridTieBreaksLeftmost() {
    // p3 and p4 tie on y+h (both bottom row); smallest-x picks the bottom-LEFT.
    XCTAssertEqual(
      DrawerFocusSeam.bottomMostLeaf(grid2x2()), p3,
      "a bottom-row tie breaks to the leftmost (smallest x) leaf")
  }

  func testBottomMostLeafPaneIdTieBreak() {
    // Two co-located leaves (same y+h AND same x): the PaneId tie-break selects
    // the smaller id.
    let coLocated: [(PaneId, Rect)] = [
      (p2, Rect(x: 0, y: halfH, w: halfW, h: halfH)),
      (p1, Rect(x: 0, y: halfH, w: halfW, h: halfH)),
    ]
    XCTAssertEqual(
      DrawerFocusSeam.bottomMostLeaf(coLocated), p1,
      "an x tie breaks to the smaller PaneId")
  }

  func testBottomMostLeafEmptyIsNil() {
    XCTAssertNil(
      DrawerFocusSeam.bottomMostLeaf([]),
      "no leaves -> no bottom-most leaf")
  }

  // MARK: - drawerUpTarget

  func testDrawerUpTargetReturnsLastEditorPaneWhenInLeaves() {
    let leaves = [p1, p2, p3, p4]
    XCTAssertEqual(
      DrawerFocusSeam.drawerUpTarget(
        lastEditorPane: p2, leaves: leaves, rects: grid2x2()),
      p2,
      "the MRU editor pane is used when it is still a live leaf")
  }

  func testDrawerUpTargetFallsBackWhenNil() {
    let leaves = [p1, p2, p3, p4]
    XCTAssertEqual(
      DrawerFocusSeam.drawerUpTarget(
        lastEditorPane: nil, leaves: leaves, rects: grid2x2()),
      p3,
      "with no MRU pane it falls back to the bottom-most leaf (p3)")
  }

  func testDrawerUpTargetFallsBackWhenStale() {
    let leaves = [p1, p2, p3, p4]
    let stale: PaneId = 99
    XCTAssertEqual(
      DrawerFocusSeam.drawerUpTarget(
        lastEditorPane: stale, leaves: leaves, rects: grid2x2()),
      p3,
      "a stale MRU pane (not in leaves) falls back to the bottom-most leaf (p3)")
  }
}
