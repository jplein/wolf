// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#panetreeweightstestsswift

import XCTest

@testable import WolfKeyMap

final class PaneTreeWeightsTests: XCTestCase {

  // MARK: - Pane ids (small, distinct)

  private let p1: PaneId = 1
  private let p2: PaneId = 2
  private let p3: PaneId = 3
  private let p4: PaneId = 4

  // MARK: - Bounds (non-square so width vs height math is distinguishable)

  private let w: Double = 100.0
  private let h: Double = 40.0
  private let halfW: Double = 50.0
  private let halfH: Double = 20.0

  /// Tolerance for `Double` comparisons.
  private let eps: Double = 1e-9

  // MARK: - Helpers

  private func approx(_ a: Double, _ b: Double) -> Bool {
    abs(a - b) < eps
  }

  private func rectFor(
    _ rects: [(PaneId, Rect)], _ id: PaneId,
    file: StaticString = #filePath, line: UInt = #line
  ) -> Rect {
    guard let found = rects.first(where: { $0.0 == id }) else {
      XCTFail("pane \(id) not found in rects \(rects)", file: file, line: line)
      return Rect(x: 0, y: 0, w: 0, h: 0)
    }
    return found.1
  }

  private func assertRect(
    _ got: Rect, _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ desc: String, file: StaticString = #filePath, line: UInt = #line
  ) {
    XCTAssertTrue(
      approx(got.x, x) && approx(got.y, y) && approx(got.w, width) && approx(got.h, height),
      "case \(desc): got \(got), want (x:\(x), y:\(y), w:\(width), h:\(height))",
      file: file, line: line)
  }

  private func totalArea(_ rects: [(PaneId, Rect)]) -> Double {
    rects.reduce(0.0) { $0 + $1.1.w * $1.1.h }
  }

  /// The single divider matching `path` + `boundary`, or a failure.
  private func divider(
    _ dividers: [PaneTree.Divider], path: [Int], boundary: Int,
    file: StaticString = #filePath, line: UInt = #line
  ) -> PaneTree.Divider {
    let matches = dividers.filter { $0.path == path && $0.boundary == boundary }
    guard matches.count == 1, let d = matches.first else {
      XCTFail(
        "expected exactly one divider at path \(path) boundary \(boundary); got \(matches)",
        file: file, line: line)
      return dividers.first ?? PaneTree.Divider(
        orientation: .vertical, path: [], boundary: 0,
        lineRect: Rect(x: 0, y: 0, w: 0, h: 0),
        pairStart: 0, pairSpan: 0, before: 0, after: 0, fraction: 0)
    }
    return d
  }

  // MARK: - setPairFraction: two-child transfer preserves the global sum

  func testSetPairFractionTwoChildTransfers() {
    // Fresh vertical split -> 0.5/0.5, then move the divider to 0.7 left.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.setPairFraction(path: [], boundary: 0, leftFraction: 0.7)

    let rects = tree.paneRects(width: w, height: h)
    assertRect(rectFor(rects, p1), 0.0, 0.0, 0.7 * w, h, "P1 gets 0.7 of the width")
    assertRect(rectFor(rects, p2), 0.7 * w, 0.0, 0.3 * w, h, "P2 gets the remaining 0.3")
    // Global sum invariant: the two rects still tile the full bounds.
    XCTAssertTrue(approx(totalArea(rects), w * h), "widths still sum to the full bounds")
  }

  // MARK: - setPairFraction: n-ary interior boundary touches ONLY the pair

  func testSetPairFractionNaryTouchesOnlyThePair() {
    // Flat row [P1, P3, P2], equal thirds after the vsplits. Resizing the first boundary (P1/P3)
    // to 0.75 must leave P2 untouched.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .vertical)  // re-equalize -> [P1, P3, P2] each 1/3

    let before = tree.paneRects(width: w, height: h)
    let p2WidthBefore = rectFor(before, p2).w  // (1/3) * w

    // pairSum for boundary 0 = w[0] + w[1] = 1/3 + 1/3 = 2/3.
    // leftFraction 0.75 -> w[0] = 0.5, w[1] = 1/6; P2 (w[2]) stays 1/3.
    tree.setPairFraction(path: [], boundary: 0, leftFraction: 0.75)

    let rects = tree.paneRects(width: w, height: h)
    assertRect(rectFor(rects, p1), 0.0, 0.0, 0.5 * w, h, "P1 = 0.75 of the 2/3 pair")
    assertRect(rectFor(rects, p3), 0.5 * w, 0.0, (1.0 / 6.0) * w, h, "P3 = 0.25 of the 2/3 pair")
    assertRect(
      rectFor(rects, p2), (2.0 / 3.0) * w, 0.0, p2WidthBefore, h,
      "the third child P2 is untouched by a resize of the P1/P3 boundary")
    XCTAssertTrue(approx(totalArea(rects), w * h), "the global sum stays 1 (rects tile bounds)")
  }

  // MARK: - setPairFraction: clamps to (eps, 1-eps) so neither pane collapses

  func testSetPairFractionClampsBothExtremes() {
    // leftFraction 0.0 -> the left pane must stay strictly positive (clamped), and
    // the right pane keeps almost everything.
    let low = PaneTree(root: p1)
    low.split(p1, p2, .vertical)
    low.setPairFraction(path: [], boundary: 0, leftFraction: 0.0)
    let lowRects = low.paneRects(width: w, height: h)
    let l1 = rectFor(lowRects, p1)
    let l2 = rectFor(lowRects, p2)
    XCTAssertGreaterThan(l1.w, 0.0, "leftFraction 0 must clamp so P1 keeps > 0 width")
    XCTAssertLessThan(l1.w, l2.w, "clamped-low P1 keeps only a sliver; P2 keeps the rest")
    XCTAssertTrue(approx(l1.w + l2.w, w), "the two clamped rects still tile the width")

    // leftFraction 1.0 -> symmetric: the RIGHT pane stays strictly positive.
    let high = PaneTree(root: p1)
    high.split(p1, p2, .vertical)
    high.setPairFraction(path: [], boundary: 0, leftFraction: 1.0)
    let highRects = high.paneRects(width: w, height: h)
    let h1 = rectFor(highRects, p1)
    let h2 = rectFor(highRects, p2)
    XCTAssertGreaterThan(h2.w, 0.0, "leftFraction 1 must clamp so P2 keeps > 0 width")
    XCTAssertLessThan(h2.w, h1.w, "clamped-high P2 keeps only a sliver; P1 keeps the rest")
    XCTAssertTrue(approx(h1.w + h2.w, w), "the two clamped rects still tile the width")
  }

  // MARK: - setPairFraction: a perpendicular nested split is untouched by a parent resize

  func testSetPairFractionLeavesPerpendicularNestingUntouched() {
    // 2x2 grid: root vertical [P1/P3, P2/P4], each column a fresh 0.5/0.5 split. Resizing the root
    // divider changes only the column widths; the pane heights stay put.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .horizontal)
    tree.split(p2, p4, .horizontal)

    tree.setPairFraction(path: [], boundary: 0, leftFraction: 0.7)

    let rects = tree.paneRects(width: w, height: h)
    // Column widths reflect the resize (left 0.7, right 0.3)...
    assertRect(rectFor(rects, p1), 0.0, 0.0, 0.7 * w, halfH, "P1 top-left, 0.7 width")
    assertRect(rectFor(rects, p3), 0.0, halfH, 0.7 * w, halfH, "P3 bottom-left, 0.7 width")
    assertRect(rectFor(rects, p2), 0.7 * w, 0.0, 0.3 * w, halfH, "P2 top-right, 0.3 width")
    assertRect(rectFor(rects, p4), 0.7 * w, halfH, 0.3 * w, halfH, "P4 bottom-right, 0.3 width")
    // ...but every pane keeps its HALF height (the nested splits were untouched).
    for (id, r) in rects {
      XCTAssertTrue(approx(r.h, halfH), "pane \(id) keeps its nested 0.5 height: \(r)")
    }
  }

  // MARK: - setPairFraction: invalid path / boundary is a no-op

  func testSetPairFractionInvalidIsNoop() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    let before = tree.paneRects(width: w, height: h)

    // Path that does not point at a split, and an out-of-range boundary: both no-op.
    tree.setPairFraction(path: [5], boundary: 0, leftFraction: 0.9)
    tree.setPairFraction(path: [], boundary: 9, leftFraction: 0.9)

    let after = tree.paneRects(width: w, height: h)
    for (id, _) in before {
      assertRect(
        rectFor(after, id), rectFor(before, id).x, rectFor(before, id).y,
        rectFor(before, id).w, rectFor(before, id).h, "pane \(id) unchanged by an invalid resize")
    }
  }

  // MARK: - split RE-EQUALIZATION: a same-orientation split redistributes the row

  func testSameOrientationSplitReEqualizesRow() {
    // Resize [P1, P2] to 0.8/0.2, then vsplit P1: the split re-equalizes the whole row, discarding
    // the drag, so all three columns become equal thirds.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.setPairFraction(path: [], boundary: 0, leftFraction: 0.8)
    tree.split(p1, p4, .vertical)  // insert P4 after P1 -> [P1, P4, P2], re-equalized

    XCTAssertEqual(tree.leaves(), [p1, p4, p2], "P4 inserted after the target P1")
    let rects = tree.paneRects(width: w, height: h)
    assertRect(rectFor(rects, p1), 0.0, 0.0, w / 3.0, h, "P1 re-equalizes to a third")
    assertRect(rectFor(rects, p4), w / 3.0, 0.0, w / 3.0, h, "P4 re-equalizes to a third")
    assertRect(
      rectFor(rects, p2), 2.0 * w / 3.0, 0.0, w / 3.0, h,
      "the pre-split 0.8/0.2 drag is discarded — P2 re-equalizes to a third too")
    XCTAssertTrue(approx(totalArea(rects), w * h), "weights still sum to 1")
  }

  func testSplittingSiblingReEqualizesRow() {
    // Resize [P1, P2] to 0.8/0.2, then split the other child P2: the row still re-equalizes, so
    // P1 does not keep 0.8.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.setPairFraction(path: [], boundary: 0, leftFraction: 0.8)
    tree.split(p2, p3, .vertical)  // insert P3 after P2 -> [P1, P2, P3], re-equalized

    XCTAssertEqual(tree.leaves(), [p1, p2, p3], "P3 inserted after the target P2")
    let rects = tree.paneRects(width: w, height: h)
    assertRect(
      rectFor(rects, p1), 0.0, 0.0, w / 3.0, h,
      "the resized P1 re-equalizes to a third (its 0.8 drag is discarded, NOT kept)")
    assertRect(rectFor(rects, p2), w / 3.0, 0.0, w / 3.0, h, "P2 re-equalizes to a third")
    assertRect(rectFor(rects, p3), 2.0 * w / 3.0, 0.0, w / 3.0, h, "P3 re-equalizes to a third")
  }

  // MARK: - remove weight PRESERVATION: survivors keep RELATIVE sizes

  func testRemoveRenormalizesProportionallyNotEqually() {
    // Remove stays proportional: drag thirds [P1, P4, P2] so P1=0.5 and P4=1/6, then remove P4;
    // survivors [0.5, 1/3] renormalize to [0.6, 0.4], not 0.5/0.5.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p4, .vertical)  // re-equalize -> [P1, P4, P2] each 1/3
    tree.setPairFraction(path: [], boundary: 0, leftFraction: 0.75)  // -> [0.5, 1/6, 1/3]
    tree.remove(p4)

    XCTAssertEqual(tree.leaves(), [p1, p2], "removing the middle leaves [P1, P2]")
    let rects = tree.paneRects(width: w, height: h)
    assertRect(
      rectFor(rects, p1), 0.0, 0.0, 0.6 * w, h,
      "P1:P2 relative sizes 0.5:(1/3) are PRESERVED (proportional renormalize to 0.6)")
    assertRect(
      rectFor(rects, p2), 0.6 * w, 0.0, 0.4 * w, h,
      "P2 renormalizes to 0.4 (NOT re-equalized to 0.5)")
    // Explicitly reject the equalization policy for REMOVE.
    XCTAssertFalse(
      approx(rectFor(rects, p1).w, halfW), "survivors must NOT re-equalize to 50/50")
  }

  func testCollapseKeepsParentWeightNotEqual() {
    // Resize to 0.8/0.2, nest [P1, P3] in P1, then remove P3: the column collapses back to P1,
    // which keeps 0.8, so the already-normalized 0.8/0.2 survives.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.setPairFraction(path: [], boundary: 0, leftFraction: 0.8)
    tree.split(p1, p3, .horizontal)
    tree.remove(p3)

    XCTAssertEqual(tree.leaves(), [p1, p2], "the 1-child column collapses back to [P1, P2]")
    let rects = tree.paneRects(width: w, height: h)
    assertRect(
      rectFor(rects, p1), 0.0, 0.0, 0.8 * w, h,
      "the collapsed child keeps the parent slot's 0.8 (not re-equalized)")
    assertRect(rectFor(rects, p2), 0.8 * w, 0.0, 0.2 * w, h, "P2 keeps 0.2")
  }

  // MARK: - topology + neighbor unchanged by a resize

  func testResizeLeavesTopologyAndNeighborsUnchanged() {
    // 2x2 grid; resize the root divider. leaves() order and every neighbor
    // relationship must be identical to the un-resized grid.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .horizontal)
    tree.split(p2, p4, .horizontal)
    let leavesBefore = tree.leaves()

    tree.setPairFraction(path: [], boundary: 0, leftFraction: 0.65)

    XCTAssertEqual(tree.leaves(), leavesBefore, "resize preserves leaves() DFS order")
    XCTAssertEqual(tree.neighbor(p1, .right, width: w, height: h), p2, "TL right -> TR")
    XCTAssertEqual(tree.neighbor(p1, .down, width: w, height: h), p3, "TL down -> BL")
    XCTAssertEqual(tree.neighbor(p4, .left, width: w, height: h), p3, "BR left -> BL")
    XCTAssertEqual(tree.neighbor(p4, .up, width: w, height: h), p2, "BR up -> TR")
    XCTAssertNil(tree.neighbor(p1, .up, width: w, height: h), "TL still has no pane above")
  }

  // MARK: - dividers(): one per interior boundary (2x2 grid)

  func testDividersForTwoByTwoGrid() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .horizontal)
    tree.split(p2, p4, .horizontal)

    let dividers = tree.dividers(width: w, height: h)
    XCTAssertEqual(dividers.count, 3, "a 2x2 grid has 3 interior dividers (1 root + 2 nested)")

    // Root vertical divider: full-height line at x = halfW; the pair spans the
    // whole width.
    let root = divider(dividers, path: [], boundary: 0)
    XCTAssertEqual(root.orientation, .vertical, "the root split is vertical")
    XCTAssertTrue(approx(root.lineRect.x, halfW), "root divider at x = halfW")
    XCTAssertTrue(approx(root.lineRect.y, 0.0), "root divider line starts at y = 0")
    XCTAssertTrue(approx(root.lineRect.w, 0.0), "a vertical divider line is zero-width")
    XCTAssertTrue(approx(root.lineRect.h, h), "root divider spans the full height")
    XCTAssertTrue(approx(root.pairStart, 0.0), "root pair starts at x = 0")
    XCTAssertTrue(approx(root.pairSpan, w), "root pair spans the full width")
    XCTAssertTrue(approx(root.fraction, 0.5), "root divider is at the 0.5 fraction")

    // Left column horizontal divider: line at y = halfH over the LEFT half's x-range.
    let left = divider(dividers, path: [0], boundary: 0)
    XCTAssertEqual(left.orientation, .horizontal, "the left column split is horizontal")
    XCTAssertTrue(approx(left.lineRect.y, halfH), "left divider at y = halfH")
    XCTAssertTrue(approx(left.lineRect.x, 0.0), "left divider line starts at x = 0")
    XCTAssertTrue(approx(left.lineRect.h, 0.0), "a horizontal divider line is zero-height")
    XCTAssertTrue(approx(left.lineRect.w, halfW), "left divider spans the left column width")
    XCTAssertTrue(approx(left.pairStart, 0.0), "left pair starts at y = 0")
    XCTAssertTrue(approx(left.pairSpan, h), "left pair spans the full height")
    XCTAssertTrue(approx(left.fraction, 0.5), "left divider is at the 0.5 fraction")

    // Right column horizontal divider: line at y = halfH over the RIGHT half's x-range.
    let right = divider(dividers, path: [1], boundary: 0)
    XCTAssertEqual(right.orientation, .horizontal, "the right column split is horizontal")
    XCTAssertTrue(approx(right.lineRect.y, halfH), "right divider at y = halfH")
    XCTAssertTrue(approx(right.lineRect.x, halfW), "right divider line starts at x = halfW")
    XCTAssertTrue(approx(right.lineRect.w, halfW), "right divider spans the right column width")
  }

  // MARK: - dividers(): flat n-ary row (per-pair start/span + before/after)

  func testDividersForFlatRow() {
    // Row [P1, P3, P2] with weights [1/3, 1/3, 1/3] (ROUND 2: the same-orientation
    // vsplit RE-EQUALIZES to equal thirds; widths ~33.33 each).
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .vertical)

    let third = w / 3.0  // ~33.333: one equal column width.
    let twoThirds = 2.0 * w / 3.0  // ~66.667: two equal columns.

    let dividers = tree.dividers(width: w, height: h)
    XCTAssertEqual(dividers.count, 2, "a flat 3-way row has 2 interior dividers")

    // Boundary 0: between P1 (0..1/3) and P3 (1/3..2/3). Pair spans x 0..2/3.
    let d0 = divider(dividers, path: [], boundary: 0)
    XCTAssertEqual(d0.orientation, .vertical, "row dividers are vertical")
    XCTAssertTrue(approx(d0.lineRect.x, third), "divider 0 sits at the P1/P3 boundary x = w/3")
    XCTAssertTrue(approx(d0.pairStart, 0.0), "pair 0 starts at x = 0")
    XCTAssertTrue(approx(d0.pairSpan, twoThirds), "pair 0 spans P1+P3 = 2w/3")
    XCTAssertTrue(approx(d0.fraction, 0.5), "(1/3) / (1/3 + 1/3) = 0.5")
    XCTAssertEqual(d0.before, p1, "divider 0 is between leaf P1...")
    XCTAssertEqual(d0.after, p3, "...and leaf P3")

    // Boundary 1: between P3 (1/3..2/3) and P2 (2/3..1). Pair spans x 1/3..1.
    let d1 = divider(dividers, path: [], boundary: 1)
    XCTAssertTrue(approx(d1.lineRect.x, twoThirds), "divider 1 sits at the P3/P2 boundary x = 2w/3")
    XCTAssertTrue(approx(d1.pairStart, third), "pair 1 starts at x = w/3 (left edge of P3)")
    XCTAssertTrue(approx(d1.pairSpan, twoThirds), "pair 1 spans P3+P2 = 2w/3")
    XCTAssertTrue(approx(d1.fraction, 0.5), "(1/3) / (1/3 + 1/3) = 0.5")
    XCTAssertEqual(d1.before, p3, "divider 1 is between leaf P3...")
    XCTAssertEqual(d1.after, p2, "...and leaf P2")
  }

  // MARK: - dividers(): fraction tracks a resize

  func testDividerFractionReflectsResize() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.setPairFraction(path: [], boundary: 0, leftFraction: 0.7)

    let d = divider(tree.dividers(width: w, height: h), path: [], boundary: 0)
    XCTAssertTrue(approx(d.fraction, 0.7), "the divider fraction reflects the new 0.7 weight")
    XCTAssertTrue(approx(d.lineRect.x, 0.7 * w), "the divider line moved to x = 0.7 * width")
  }
}
