// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#panetreetestsswift

import XCTest

@testable import WolfKeyMap

final class PaneTreeTests: XCTestCase {

  // MARK: - Pane ids (small, distinct; < 5 so no magic-number concerns)

  private let p1: PaneId = 1
  private let p2: PaneId = 2
  private let p3: PaneId = 3
  private let p4: PaneId = 4

  // MARK: - Bounds used across the geometry tests
  // Non-square (W != H) so width math and height math are distinguishable.

  /// Test bounds width (px).
  private let w: Double = 100.0
  /// Test bounds height (px).
  private let h: Double = 40.0
  /// Half the width — a 0.5 Vertical split boundary.
  private let halfW: Double = 50.0
  /// Half the height — a 0.5 Horizontal split boundary.
  private let halfH: Double = 20.0
  /// One quarter of the test width — a 0.25-weight Vertical split column width
  /// under the macOS weight-preservation policy (feature plan §3.1).
  private let quarterW: Double = 100.0 / 4.0

  /// Tolerance for comparing the `Double` rect components. The arithmetic is a
  /// few multiplies, so a tight epsilon is appropriate.
  private let eps: Double = 1e-9

  // MARK: - Float comparison helpers (ports of layout.rs `approx` / `assert_rect` / ...)

  /// Returns whether two doubles are within `eps`.
  private func approx(_ a: Double, _ b: Double) -> Bool {
    abs(a - b) < eps
  }

  /// Looks up the `Rect` for `id` in a `paneRects` result, failing with a
  /// readable message if the pane is missing.
  private func rectFor(
    _ rects: [(PaneId, Rect)],
    _ id: PaneId,
    file: StaticString = #filePath,
    line: UInt = #line
  ) -> Rect {
    guard let found = rects.first(where: { $0.0 == id }) else {
      XCTFail("pane \(id) not found in rects \(rects)", file: file, line: line)
      return Rect(x: 0, y: 0, w: 0, h: 0)
    }
    return found.1
  }

  /// Asserts a `Rect` matches the expected `(x, y, w, h)` within `eps`.
  private func assertRect(
    _ got: Rect,
    _ x: Double,
    _ y: Double,
    _ width: Double,
    _ height: Double,
    _ desc: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    XCTAssertTrue(
      approx(got.x, x) && approx(got.y, y) && approx(got.w, width)
        && approx(got.h, height),
      "case \(desc): got \(got), want (x:\(x), y:\(y), w:\(width), h:\(height))",
      file: file,
      line: line
    )
  }

  /// Whether two rects overlap with strictly positive area (a shared edge does
  /// NOT count as overlap).
  private func rectsOverlap(_ a: Rect, _ b: Rect) -> Bool {
    let xOverlap = max(a.x, b.x) < min(a.x + a.w, b.x + b.w) - eps
    let yOverlap = max(a.y, b.y) < min(a.y + a.h, b.y + b.h) - eps
    return xOverlap && yOverlap
  }

  /// Sum of every leaf rect's area (used to assert the rects tile the bounds).
  private func totalArea(_ rects: [(PaneId, Rect)]) -> Double {
    rects.reduce(0.0) { $0 + $1.1.w * $1.1.h }
  }

  // MARK: - construction

  func testNewIsSingleLeaf() {
    let tree = PaneTree(root: p1)
    XCTAssertEqual(tree.leaves(), [p1], "a fresh tree has exactly its root leaf")

    // The single pane fills the whole bounds.
    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 1, "one leaf -> one rect")
    assertRect(rectFor(rects, p1), 0.0, 0.0, w, h, "root fills bounds")
  }

  // MARK: - split

  func testSplitVerticalTwoLeaves() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)

    let leaves = tree.leaves()
    XCTAssertEqual(leaves.count, 2, "a single split yields two leaves")
    XCTAssertTrue(leaves.contains(p1), "the split target survives as a leaf")
    XCTAssertTrue(leaves.contains(p2), "the new pane is a leaf")
    // CONVENTION: new pane is the SECOND child, so DFS order is [target, new].
    XCTAssertEqual(leaves, [p1, p2], "new pane goes second (DFS order)")
  }

  func testSplitThenPaneRectsSideBySide() {
    // Vertical split divides WIDTH: target (P1) stays LEFT, new (P2) on RIGHT.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 2, "two leaves -> two rects")
    assertRect(rectFor(rects, p1), 0.0, 0.0, halfW, h, "P1 left half")
    assertRect(rectFor(rects, p2), halfW, 0.0, halfW, h, "P2 right half")
  }

  func testSplitHorizontalStacked() {
    // Horizontal split divides HEIGHT: target (P1) stays TOP, new (P2) BOTTOM.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .horizontal)

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 2, "two leaves -> two rects")
    assertRect(rectFor(rects, p1), 0.0, 0.0, w, halfH, "P1 top half")
    assertRect(rectFor(rects, p2), 0.0, halfH, w, halfH, "P2 bottom half")
  }

  func testNestedSplitRects() {
    // Vertical split P1 | P2, then split P1 horizontally into P1 over P3: the three rects tile
    // the bounds with no overlap.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .horizontal)

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 3, "three leaves -> three rects")

    let r1 = rectFor(rects, p1)
    let r2 = rectFor(rects, p2)
    let r3 = rectFor(rects, p3)

    // Exact tiling of the 100x40 area.
    assertRect(r1, 0.0, 0.0, halfW, halfH, "P1 top-left")
    assertRect(r3, 0.0, halfH, halfW, halfH, "P3 bottom-left")
    assertRect(r2, halfW, 0.0, halfW, h, "P2 full-height right")

    // No pairwise overlap.
    XCTAssertFalse(rectsOverlap(r1, r2), "P1/P2 must not overlap")
    XCTAssertFalse(rectsOverlap(r1, r3), "P1/P3 must not overlap")
    XCTAssertFalse(rectsOverlap(r2, r3), "P2/P3 must not overlap")

    // Areas sum to the full bounds.
    XCTAssertTrue(
      approx(totalArea(rects), w * h),
      "leaf areas must sum to the full bounds")
  }

  // MARK: - n-ary FLAT (same-orientation) split behavior
  // A split matching its frame's orientation joins a flat, equalized row; a perpendicular nests.

  func testVsplitFlattensIntoRow() {
    // Row [P1, P2] (Vertical = side by side), then Vertical-split P1 again: the same orientation
    // inserts P3 as a flat sibling after P1, not a nested sub-split.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .vertical)

    // Insert-after-target pins the DFS order to [P1, P3, P2].
    XCTAssertEqual(
      tree.leaves(),
      [p1, p3, p2],
      "same-orientation vsplit inserts the new pane AFTER the target (flat row)")

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 3, "three flat leaves -> three rects")

    let r1 = rectFor(rects, p1)
    let r3 = rectFor(rects, p3)
    let r2 = rectFor(rects, p2)

    // A same-orientation vsplit re-equalizes the whole row to thirds, as `:vsplit` does; the
    // pre-split 0.5/0.5 is discarded.
    assertRect(r1, 0.0, 0.0, w / 3.0, h, "P1 re-equalizes to a left third")
    assertRect(r3, w / 3.0, 0.0, w / 3.0, h, "P3 takes the middle third")
    assertRect(r2, 2.0 * w / 3.0, 0.0, w / 3.0, h, "P2 re-equalizes to the right third")

    // No rect is zero-width (the pre-existing right pane is NOT starved).
    for (id, r) in rects {
      XCTAssertTrue(r.w > eps, "pane \(id) must not be zero-width: \(r)")
    }

    // No pairwise overlap.
    XCTAssertFalse(rectsOverlap(r1, r3), "P1/P3 must not overlap")
    XCTAssertFalse(rectsOverlap(r3, r2), "P3/P2 must not overlap")
    XCTAssertFalse(rectsOverlap(r1, r2), "P1/P2 must not overlap")

    // Areas tile the full bounds.
    XCTAssertTrue(
      approx(totalArea(rects), w * h),
      "leaf areas must sum to the full bounds")
  }

  func testVsplitTwiceStaysFlatEqualWeights() {
    // Repeated vsplits of P1 keep one flat row, each re-equalizing it, so three vsplits leave
    // four equal quarters.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)  // [P1:0.5, P2:0.5]
    tree.split(p1, p3, .vertical)  // re-equalize -> [P1, P3, P2] each 1/3
    tree.split(p1, p4, .vertical)  // re-equalize -> [P1, P4, P3, P2] each 0.25

    let leaves = tree.leaves()
    XCTAssertEqual(
      leaves.count, 4, "four flat leaves after three same-orientation vsplits")
    XCTAssertEqual(
      leaves, [p1, p4, p3, p2], "each new pane is inserted AFTER the target (flat row)")

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 4, "four flat leaves -> four rects")

    // The row re-equalizes: every column is an equal quarter (x at 0/0.25w/0.5w/0.75w).
    assertRect(rectFor(rects, p1), 0.0, 0.0, quarterW, h, "P1 re-equalizes to a quarter")
    assertRect(rectFor(rects, p4), quarterW, 0.0, quarterW, h, "P4 re-equalizes to a quarter")
    assertRect(rectFor(rects, p3), 2.0 * quarterW, 0.0, quarterW, h, "P3 re-equalizes to a quarter")
    assertRect(rectFor(rects, p2), 3.0 * quarterW, 0.0, quarterW, h, "P2 re-equalizes to a quarter")

    for (id, r) in rects {
      XCTAssertTrue(r.w > eps, "pane \(id) must not be zero-width: \(r)")
    }

    // Areas tile the full bounds.
    XCTAssertTrue(
      approx(totalArea(rects), w * h),
      "leaf areas must sum to the full bounds")
  }

  func testHsplitOfRowMemberNests() {
    // A perpendicular split still nests: Horizontal-splitting P1 in row [P1, P2] wraps it in a
    // stacked [P1, P3] in the left half; P2 keeps the full-height right half.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .horizontal)

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 3, "three leaves -> three rects")

    let r1 = rectFor(rects, p1)
    let r3 = rectFor(rects, p3)
    let r2 = rectFor(rects, p2)

    assertRect(r1, 0.0, 0.0, halfW, halfH, "P1 top-left (nested column)")
    assertRect(r3, 0.0, halfH, halfW, halfH, "P3 bottom-left (nested column)")
    assertRect(r2, halfW, 0.0, halfW, h, "P2 full-height right")

    for (id, r) in rects {
      XCTAssertTrue(
        r.w > eps && r.h > eps, "pane \(id) must not be zero-area: \(r)")
    }
    XCTAssertFalse(rectsOverlap(r1, r2), "P1/P2 must not overlap")
    XCTAssertFalse(rectsOverlap(r1, r3), "P1/P3 must not overlap")
    XCTAssertFalse(rectsOverlap(r2, r3), "P2/P3 must not overlap")

    XCTAssertTrue(
      approx(totalArea(rects), w * h),
      "leaf areas must sum to the full bounds")
  }

  // MARK: - remove / collapse

  func testRemoveCollapsesSibling() {
    // Simple case: split then remove the new pane -> back to a single leaf
    // covering the whole bounds.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.remove(p2)

    XCTAssertEqual(tree.leaves(), [p1], "removing the sibling collapses to P1")
    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 1, "one leaf remains")
    assertRect(rectFor(rects, p1), 0.0, 0.0, w, h, "P1 reclaims the full bounds")
  }

  func testRemoveCollapsesSiblingSubtree() {
    // P1 left, a horizontally split P2/P3 subtree right: removing P1 collapses the subtree into
    // the root slot, which then fills the bounds.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p2, p3, .horizontal)
    tree.remove(p1)

    let leaves = tree.leaves()
    XCTAssertEqual(leaves.count, 2, "P1 gone, the right subtree survives")
    XCTAssertTrue(
      leaves.contains(p2) && leaves.contains(p3), "P2 and P3 remain")

    // The surviving subtree now occupies the FULL bounds (its parent's slot):
    // P2 top half, P3 bottom half across the entire width.
    let rects = tree.paneRects(width: w, height: h)
    assertRect(rectFor(rects, p2), 0.0, 0.0, w, halfH, "P2 spans full width, top")
    assertRect(
      rectFor(rects, p3), 0.0, halfH, w, halfH, "P3 spans full width, bottom")
  }

  func testRemoveLastIsNoop() {
    // The tree is never empty: removing the only pane is a no-op.
    let tree = PaneTree(root: p1)
    tree.remove(p1)
    XCTAssertEqual(tree.leaves(), [p1], "removing the last pane is a no-op")
  }

  func testRemoveProportionallyRenormalizesRow() {
    // Remove renormalizes proportionally; since splits re-equalize, a drag makes the unequal
    // precondition. Survivors 1/3 : 1/2 become [0.4, 0.6], not 50/50.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .vertical)  // re-equalize -> [P1, P3, P2] each 1/3
    // Drag the second boundary (between P3 and P2) to leftFraction 0.25 of the
    // (1/3 + 1/3) = 2/3 pair: P3 -> 1/6, P2 -> 1/2; P1 (untouched) stays 1/3.
    tree.setPairFraction(path: [], boundary: 1, leftFraction: 0.25)
    tree.remove(p3)

    XCTAssertEqual(
      tree.leaves(), [p1, p2], "removing P3 leaves the flat [P1, P2] row")

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 2, "two leaves -> two rects")
    // Survivors [1/3, 1/2] renormalize proportionally to [0.4, 0.6].
    assertRect(
      rectFor(rects, p1), 0.0, 0.0, 0.4 * w, h,
      "P1 keeps its relative 1/3 -> renormalizes to 0.4 (NOT re-equalized)")
    assertRect(
      rectFor(rects, p2), 0.4 * w, 0.0, 0.6 * w, h,
      "P2 keeps its relative 1/2 -> renormalizes to 0.6")
  }

  func testRemoveCollapsesSingleChild() {
    // Row [P1, P2], then a perpendicular split of P1 nests column [P1, P3]; removing P3 collapses
    // the one-child column, and P1 re-equalizes with P2 to 50/50.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .horizontal)
    tree.remove(p3)

    XCTAssertEqual(
      tree.leaves(),
      [p1, p2],
      "the 1-child column collapses, leaving the flat [P1, P2] row")

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 2, "two leaves -> two rects")
    assertRect(
      rectFor(rects, p1), 0.0, 0.0, halfW, h, "P1 reclaims+equalizes left half")
    assertRect(rectFor(rects, p2), halfW, 0.0, halfW, h, "P2 right half")
  }

  func testRemoveCollapsesAndMergesSameOrientation() {
    // vsplit P1, hsplit P2, vsplit P2 -> row[P1, col[row[P2, P4], P3]]. Removing P3 collapses
    // the column to row[P2, P4], which merges into the root row as flat row[P1, P2, P4].
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p2, p3, .horizontal)
    tree.split(p2, p4, .vertical)
    tree.remove(p3)

    XCTAssertEqual(
      tree.leaves(),
      [p1, p2, p4],
      "collapse + same-orientation merge yields a flat [P1, P2, P4] row")

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 3, "three flat leaves -> three rects")

    // The merged subtree keeps its proportional slot: P2 and P4 scale 0.5 -> 0.25, P1 keeps 0.5.
    // This deliberately diverges from wolf-gnome's layout.rs, which re-equalizes to thirds.
    let r1 = rectFor(rects, p1)
    let r2 = rectFor(rects, p2)
    let r4 = rectFor(rects, p4)
    assertRect(r1, 0.0, 0.0, halfW, h, "P1 keeps its 0.5 (left half)")
    assertRect(r2, halfW, 0.0, quarterW, h, "P2 scales to 0.25")
    assertRect(r4, 0.75 * w, 0.0, quarterW, h, "P4 scales to 0.25")

    for (id, r) in rects {
      XCTAssertTrue(r.w > eps, "pane \(id) must not be zero-width: \(r)")
    }

    XCTAssertTrue(
      approx(totalArea(rects), w * h),
      "leaf areas must sum to the full bounds")
  }

  // MARK: - neighbor

  func testNeighborBasicVertical() {
    // Side-by-side: P1 left, P2 right.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)

    XCTAssertEqual(
      tree.neighbor(p1, .right, width: w, height: h), p2, "left->right is P2")
    XCTAssertEqual(
      tree.neighbor(p2, .left, width: w, height: h), p1, "right->left is P1")
    XCTAssertNil(
      tree.neighbor(p1, .left, width: w, height: h), "P1 is at the left edge")
    XCTAssertNil(
      tree.neighbor(p2, .right, width: w, height: h), "P2 is at the right edge")
    // No vertical neighbours in a purely side-by-side layout.
    XCTAssertNil(tree.neighbor(p1, .up, width: w, height: h), "no pane above P1")
    XCTAssertNil(tree.neighbor(p1, .down, width: w, height: h), "no pane below P1")
  }

  func testNeighborBasicHorizontal() {
    // Stacked: P1 top, P2 bottom.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .horizontal)

    XCTAssertEqual(
      tree.neighbor(p1, .down, width: w, height: h), p2, "top->down is P2")
    XCTAssertEqual(
      tree.neighbor(p2, .up, width: w, height: h), p1, "bottom->up is P1")
    XCTAssertNil(
      tree.neighbor(p1, .up, width: w, height: h), "P1 is at the top edge")
    XCTAssertNil(
      tree.neighbor(p2, .down, width: w, height: h), "P2 is at the bottom edge")
    // No horizontal neighbours in a purely stacked layout.
    XCTAssertNil(tree.neighbor(p1, .left, width: w, height: h), "no pane left of P1")
    XCTAssertNil(
      tree.neighbor(p1, .right, width: w, height: h), "no pane right of P1")
  }

  func testNeighborNested() {
    // 2x2 grid in 100x40: P1 (0,0,50,20), P2 (50,0,50,20), P3 (0,20,50,20), P4 (50,20,50,20).
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .horizontal)
    tree.split(p2, p4, .horizontal)

    // Sanity-check the geometry the neighbour cases rely on.
    let rects = tree.paneRects(width: w, height: h)
    assertRect(rectFor(rects, p1), 0.0, 0.0, halfW, halfH, "P1 top-left")
    assertRect(rectFor(rects, p2), halfW, 0.0, halfW, halfH, "P2 top-right")
    assertRect(rectFor(rects, p3), 0.0, halfH, halfW, halfH, "P3 bottom-left")
    assertRect(rectFor(rects, p4), halfW, halfH, halfW, halfH, "P4 bottom-right")

    // Unambiguous neighbours (each shares a full edge with exactly one pane
    // per direction).
    XCTAssertEqual(
      tree.neighbor(p1, .right, width: w, height: h), p2, "TL right -> TR")
    XCTAssertEqual(
      tree.neighbor(p1, .down, width: w, height: h), p3, "TL down -> BL")
    XCTAssertEqual(
      tree.neighbor(p4, .left, width: w, height: h), p3, "BR left -> BL")
    XCTAssertEqual(tree.neighbor(p4, .up, width: w, height: h), p2, "BR up -> TR")
    XCTAssertEqual(
      tree.neighbor(p2, .left, width: w, height: h), p1, "TR left -> TL")
    XCTAssertEqual(
      tree.neighbor(p3, .right, width: w, height: h), p4, "BL right -> BR")

    // Edges return nil.
    XCTAssertNil(
      tree.neighbor(p1, .up, width: w, height: h), "TL has no pane above")
    XCTAssertNil(
      tree.neighbor(p1, .left, width: w, height: h), "TL has no pane left")
    XCTAssertNil(
      tree.neighbor(p4, .right, width: w, height: h), "BR has no pane right")
    XCTAssertNil(
      tree.neighbor(p4, .down, width: w, height: h), "BR has no pane below")
  }

  func testNeighborUnknownPaneIsNone() {
    // A pane id not in the tree has no neighbour in any direction.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    let absent: PaneId = 99
    XCTAssertNil(
      tree.neighbor(absent, .left, width: w, height: h), "unknown pane -> nil")
    XCTAssertNil(
      tree.neighbor(absent, .right, width: w, height: h), "unknown pane -> nil")
  }

  func testNeighborFlatRowOfThree() {
    // Flat row [P1, P3, P2] (see testVsplitFlattensIntoRow): left-to-right
    // neighbours chain across the three columns, with nil at each edge.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .vertical)

    XCTAssertEqual(
      tree.neighbor(p1, .right, width: w, height: h), p3, "P1 right -> P3")
    XCTAssertEqual(
      tree.neighbor(p3, .right, width: w, height: h), p2, "P3 right -> P2")
    XCTAssertNil(
      tree.neighbor(p2, .right, width: w, height: h), "P2 is at the right edge")
    XCTAssertEqual(
      tree.neighbor(p3, .left, width: w, height: h), p1, "P3 left -> P1")
    XCTAssertNil(
      tree.neighbor(p1, .left, width: w, height: h), "P1 is at the left edge")
  }

  // MARK: - determinism

  func testLeavesAndRectsOrderIsStable() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .horizontal)
    tree.split(p2, p4, .horizontal)

    // Repeated calls must yield identical ordering.
    XCTAssertEqual(tree.leaves(), tree.leaves(), "leaves() order is stable")

    let first = tree.paneRects(width: w, height: h).map { $0.0 }
    let second = tree.paneRects(width: w, height: h).map { $0.0 }
    XCTAssertEqual(first, second, "paneRects() order is stable")

    // paneRects order matches leaves order (same DFS).
    XCTAssertEqual(first, tree.leaves(), "paneRects order matches leaves order")
  }

  // MARK: - FFI discriminant contract (NOT in layout.rs)
  // Shared with wolf-ffi: orientation H=0 V=1, focus_dir L=0 R=1 U=2 D=3; out of range is nil.

  func testOrientationFfiDiscriminants() {
    XCTAssertEqual(Orientation(ffi: 0), .horizontal, "ffi 0 -> .horizontal")
    XCTAssertEqual(Orientation(ffi: 1), .vertical, "ffi 1 -> .vertical")
    XCTAssertNil(Orientation(ffi: 9), "an out-of-range orientation -> nil")
  }

  func testFocusDirFfiDiscriminants() {
    XCTAssertEqual(FocusDir(ffi: 0), .left, "ffi 0 -> .left")
    XCTAssertEqual(FocusDir(ffi: 1), .right, "ffi 1 -> .right")
    XCTAssertEqual(FocusDir(ffi: 2), .up, "ffi 2 -> .up")
    XCTAssertEqual(FocusDir(ffi: 3), .down, "ffi 3 -> .down")
    XCTAssertNil(FocusDir(ffi: 9), "an out-of-range focus dir -> nil")
  }
}
