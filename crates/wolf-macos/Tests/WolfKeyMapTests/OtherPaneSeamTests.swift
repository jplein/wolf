// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#otherpaneseamtestsswift

import XCTest

@testable import WolfKeyMap

final class OtherPaneSeamTests: XCTestCase {

  // MARK: - Pane ids (small, distinct)

  private let p1: PaneId = 1
  private let p2: PaneId = 2
  private let p3: PaneId = 3
  private let p4: PaneId = 4

  /// A pane id never inserted into any tree — the "active pane is unknown"
  /// (drawer-focused) probe.
  private let unknownPane: PaneId = 99

  // MARK: - Bounds (non-square so width math and height math are distinguishable)

  /// Test bounds width (px). Mirrors `PaneTreeTests`.
  private let w: Double = 100.0
  /// Test bounds height (px). Mirrors `PaneTreeTests`.
  private let h: Double = 40.0

  // MARK: - 1. single leaf -> .split for BOTH sides

  func testSingleLeafSplitsBothSides() {
    let tree = PaneTree(root: p1)
    XCTAssertEqual(
      OtherPaneSeam.target(active: p1, prefer: .right, tree: tree, width: w, height: h),
      .split,
      "a lone pane has no other pane: prefer-right must ask for a split")
    XCTAssertEqual(
      OtherPaneSeam.target(active: p1, prefer: .left, tree: tree, width: w, height: h),
      .split,
      "a lone pane has no other pane: prefer-left must ask for a split")
  }

  // MARK: - 2. two side-by-side, active left, prefer right -> the geometric neighbour

  func testTwoSideBySidePreferRightUsesRightNeighbor() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)  // [p1][p2]
    XCTAssertEqual(
      OtherPaneSeam.target(active: p1, prefer: .right, tree: tree, width: w, height: h),
      .existing(p2),
      "with a real pane to the right, prefer-right must reuse it (rule 3)")
  }

  // MARK: - 3. two side-by-side, active left, prefer left -> DFS fallback wraps

  func testTwoSideBySidePreferLeftWrapsToOtherPane() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)  // [p1][p2]
    XCTAssertEqual(
      OtherPaneSeam.target(active: p1, prefer: .left, tree: tree, width: w, height: h),
      .existing(p2),
      "no pane sits left of p1, so the DFS previous-with-wrap fallback must land on the only other pane (p2), not a split"
    )
  }

  // MARK: - 4. three in a row, active the MIDDLE pane -> both geometric neighbours

  func testThreeInARowMiddleUsesGeometricNeighbors() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p2, p3, .vertical)  // [p1][p2][p3]
    XCTAssertEqual(
      OtherPaneSeam.target(active: p2, prefer: .right, tree: tree, width: w, height: h),
      .existing(p3),
      "from the middle pane, prefer-right must reuse the pane on its right")
    XCTAssertEqual(
      OtherPaneSeam.target(active: p2, prefer: .left, tree: tree, width: w, height: h),
      .existing(p1),
      "from the middle pane, prefer-left must reuse the pane on its left")
  }

  // MARK: - 5. edge panes wrap in leaves() order

  func testEdgePanesWrapInDfsOrder() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p2, p3, .vertical)  // [p1][p2][p3], leaves() == [1, 2, 3]
    XCTAssertEqual(
      OtherPaneSeam.target(active: p3, prefer: .right, tree: tree, width: w, height: h),
      .existing(p1),
      "the rightmost pane has no right neighbour: next-with-wrap must land on the FIRST leaf (p1)"
    )
    XCTAssertEqual(
      OtherPaneSeam.target(active: p1, prefer: .left, tree: tree, width: w, height: h),
      .existing(p3),
      "the leftmost pane has no left neighbour: previous-with-wrap must land on the LAST leaf (p3)"
    )
  }

  // MARK: - 6. vertical stack -> no lateral neighbour, fallback to the other pane

  func testVerticalStackFallsBackToOtherPane() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .horizontal)  // p1 stacked above p2
    XCTAssertEqual(
      OtherPaneSeam.target(active: p1, prefer: .right, tree: tree, width: w, height: h),
      .existing(p2),
      "a stacked layout has no lateral neighbour: prefer-right must fall back to the other existing pane (p2), never a split"
    )
    XCTAssertEqual(
      OtherPaneSeam.target(active: p2, prefer: .left, tree: tree, width: w, height: h),
      .existing(p1),
      "prefer-left from the bottom pane must fall back to the other existing pane")
  }

  // MARK: - 7. 2x2 grid -> the cross-axis-overlap neighbour, never the diagonal

  func testGridPrefersCrossAxisOverlapNeighborNotDiagonal() {
    // [p1][p2]
    // [p3][p4]
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .horizontal)
    tree.split(p2, p4, .horizontal)
    XCTAssertEqual(
      OtherPaneSeam.target(active: p1, prefer: .right, tree: tree, width: w, height: h),
      .existing(p2),
      "right from top-left must be TOP-right (strict cross-axis overlap), not the diagonal bottom-right pane"
    )
  }

  // MARK: - 8. active is not a leaf (the drawer case) -> nil

  func testNonLeafActiveReturnsNil() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    XCTAssertNil(
      OtherPaneSeam.target(
        active: unknownPane, prefer: .right, tree: tree, width: w, height: h),
      "an active id that is no leaf of the tree (unknown pane / drawer focus) must resolve to nil — the command is a no-op, not a split"
    )
  }
}
