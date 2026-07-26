//
//  PaneTreeTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.PaneTree`.
//
//  These are a FAITHFUL 1:1 port of the `#[cfg(test)] mod tests` in
//  `crates/wolf-gnome/src/layout.rs`: the same pane ids, the same 100x40 bounds,
//  the same half/third/quarter constants, the same `approx` / `assert_rect` /
//  `rects_overlap` / `rect_for` helpers, and the same case names (rendered as
//  XCTest `test...` methods). They exercise the PURE pane-layout tree the coder
//  will add in `Sources/WolfKeyMap/PaneTree.swift` (Foundation only, no AppKit /
//  CoreGraphics) — NO NSView, NO NSSplitView, NO live Neovim — exactly the way
//  the GNOME layout tests stay GTK-free.
//
//  CONTRACT UNDER TEST (must match wolf-gnome/src/layout.rs exactly):
//    * `PaneTree(root:)` is a single leaf filling the bounds.
//    * `split(_:_:_:)` — a same-orientation split inserts a FLAT sibling AFTER
//      the target then re-equalizes (the `[P1, P3, P2]` regression); a
//      PERPENDICULAR split NESTS `[target, newPane]`.
//    * `remove(_:)` — drops the slot, then normalizes (flatten same-orientation,
//      collapse a 1-child split, re-equalize); removing the last pane is a no-op.
//    * `leaves()` / `paneRects(width:height:)` are a stable DFS order; Vertical
//      divides WIDTH, Horizontal divides HEIGHT.
//    * `neighbor(_:_:width:height:)` uses STRICTLY positive cross-axis overlap.
//
//  NOTE to coder: this references the public `WolfKeyMap.PaneTree` API the coder
//  will add (`PaneId`, `Orientation`, `FocusDir`, `Rect`, `PaneLayout`). The
//  signatures asserted below are the contract:
//    * `PaneTree(root: PaneId)`
//    * `.split(_ target: PaneId, _ newPane: PaneId, _ orientation: Orientation)`
//    * `.remove(_ pane: PaneId)`
//    * `.leaves() -> [PaneId]`
//    * `.paneRects(width: Double, height: Double) -> [(PaneId, Rect)]`
//    * `.neighbor(_ from: PaneId, _ dir: FocusDir, width: Double, height: Double)
//      -> PaneId?`
//    * `.layout() -> PaneLayout`
//    * `struct Rect { let x, y, w, h: Double }`
//    * `enum Orientation { case horizontal, vertical }`
//    * `enum FocusDir { case left, right, up, down }`
//  ASSUMPTION (the discriminant test below): the coder adds
//  `Orientation.init?(ffi: UInt8)` (0 -> .horizontal, 1 -> .vertical, else nil)
//  and `FocusDir.init?(ffi: UInt8)` (0 -> .left, 1 -> .right, 2 -> .up,
//  3 -> .down, else nil), per plan §2A / §3.1 (the FFI discriminant contract
//  shared with `wolf-ffi`).
//

import XCTest

@testable import WolfKeyMap

final class PaneTreeTests: XCTestCase {

  // MARK: - Pane ids (small, distinct; < 5 so no magic-number concerns)

  private let p1: PaneId = 1
  private let p2: PaneId = 2
  private let p3: PaneId = 3
  private let p4: PaneId = 4

  // MARK: - Bounds used across the geometry tests
  //
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
    // Vertical split (P1 left, P2 right), then split the LEFT child (P1)
    // horizontally into P1 (top) / P3 (bottom). The three rects must tile the
    // bounds with no overlap and a union equal to the full area.
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
  //
  // These pin the n-ary frame model: a split whose orientation MATCHES the
  // containing frame FLATTENS into a flat, equalized sibling row/column (no
  // nesting); a PERPENDICULAR split still NESTS. The flat cases are THE 3-pane
  // regression: the old binary always-nest model produced nested 25/25/50
  // columns, blanking the pre-existing right pane.

  func testVsplitFlattensIntoRow() {
    // Start with a row [P1, P2] (Vertical = side by side), then Vertical-split
    // P1 AGAIN. Because the new split's orientation MATCHES the row, P3 is
    // inserted as a FLAT sibling AFTER the target P1 — NOT a nested sub-split.
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

    // POLICY CHANGE (macOS weight preservation, feature plan §3.1): the new pane's
    // share is CARVED FROM THE TARGET P1 only (P1's 0.5 halves to 0.25/0.25); the
    // pre-existing P2 keeps its 0.5. The old gnome 1:1 port re-equalized to thirds.
    // The TOPOLOGY (flat [P1, P3, P2]) is unchanged — only the weights differ.
    assertRect(r1, 0.0, 0.0, quarterW, h, "P1 halves to a left quarter (0.25 weight)")
    assertRect(r3, quarterW, 0.0, quarterW, h, "P3 takes the other quarter (0.25 weight)")
    assertRect(r2, halfW, 0.0, halfW, h, "P2 keeps its pre-existing 0.5 (right half)")

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

  func testVsplitTwiceStaysFlatWeightsPreserved() {
    // Repeated same-orientation vsplits of P1 keep the row FLAT (no growing
    // nesting). POLICY CHANGE (macOS weight preservation, feature plan §3.1):
    // each vsplit halves ONLY the target P1, leaving the other siblings' weights
    // intact — so the columns are NOT four equal quarters. The TOPOLOGY (a single
    // flat row of four leaves) is unchanged; only the widths differ.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)  // [P1:0.5, P2:0.5]
    tree.split(p1, p3, .vertical)  // carve P1 -> [P1:0.25, P3:0.25, P2:0.5]
    tree.split(p1, p4, .vertical)  // carve P1 -> [P1:0.125, P4:0.125, P3:0.25, P2:0.5]

    let leaves = tree.leaves()
    XCTAssertEqual(
      leaves.count, 4, "four flat leaves after three same-orientation vsplits")
    XCTAssertEqual(
      leaves, [p1, p4, p3, p2], "each new pane is inserted AFTER the target (flat row)")

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 4, "four flat leaves -> four rects")

    // Only the repeatedly-split P1 shrinks; the earlier siblings keep their weights.
    assertRect(rectFor(rects, p1), 0.0, 0.0, 0.125 * w, h, "P1 halved twice -> 0.125")
    assertRect(rectFor(rects, p4), 0.125 * w, 0.0, 0.125 * w, h, "P4 takes the other 0.125")
    assertRect(rectFor(rects, p3), 0.25 * w, 0.0, quarterW, h, "P3 keeps its 0.25")
    assertRect(rectFor(rects, p2), halfW, 0.0, halfW, h, "P2 keeps its original 0.5")

    for (id, r) in rects {
      XCTAssertTrue(r.w > eps, "pane \(id) must not be zero-width: \(r)")
    }

    // Areas tile the full bounds.
    XCTAssertTrue(
      approx(totalArea(rects), w * h),
      "leaf areas must sum to the full bounds")
  }

  func testHsplitOfRowMemberNests() {
    // PERPENDICULAR split still NESTS: row [P1, P2], then HORIZONTAL-split P1.
    // Horizontal is perpendicular to the Vertical row, so P1 is wrapped in a
    // stacked sub-frame [P1, P3] occupying the left half; P2 keeps the full-
    // height right half.
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
    // Nested case: P1 on the left, and a horizontally-split (P2 top / P3
    // bottom) subtree on the right. Removing P1 collapses its sibling SUBTREE
    // up into the root slot, which then fills the whole bounds.
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
    // Flat row [P1, P3, P2] with weights [0.25, 0.25, 0.5] (see
    // testVsplitFlattensIntoRow's new policy). POLICY CHANGE (macOS weight
    // preservation, feature plan §3.1): removing the middle pane P3 leaves a flat
    // [P1, P2] whose survivors keep their RELATIVE sizes (0.25 : 0.5) and
    // renormalize PROPORTIONALLY to [1/3, 2/3] — the old gnome 1:1 port
    // re-equalized to 50/50. The TOPOLOGY (flat [P1, P2]) is unchanged.
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p1, p3, .vertical)
    tree.remove(p3)

    XCTAssertEqual(
      tree.leaves(), [p1, p2], "removing P3 leaves the flat [P1, P2] row")

    let rects = tree.paneRects(width: w, height: h)
    XCTAssertEqual(rects.count, 2, "two leaves -> two rects")
    assertRect(
      rectFor(rects, p1), 0.0, 0.0, (1.0 / 3.0) * w, h,
      "P1 keeps its relative 0.25 -> renormalizes to 1/3 (NOT re-equalized)")
    assertRect(
      rectFor(rects, p2), (1.0 / 3.0) * w, 0.0, (2.0 / 3.0) * w, h,
      "P2 keeps its relative 0.5 -> renormalizes to 2/3")
  }

  func testRemoveCollapsesSingleChild() {
    // Row [P1, P2], then a PERPENDICULAR (Horizontal) split of P1 nests a
    // column [P1, P3] in the left half. Removing P3 leaves the column with a
    // single child P1, which COLLAPSES into the parent slot; P1 then re-
    // equalizes with P2 into 50/50 halves.
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
    // The MERGE case:
    //   PaneTree(root: P1)
    //   vsplit P1            -> row[P1, P2]
    //   hsplit P2 (perp)     -> row[P1, col[P2, P3]]
    //   vsplit P2 (perp to   -> row[P1, col[ row[P2, P4], P3 ]]
    //     the column)
    //   remove P3            -> the column drops to a single child row[P2, P4],
    //                           which COLLAPSES, and because it is the SAME
    //                           orientation as the root row it MERGES upward:
    //                           => flat row[P1, P2, P4].
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

    // POLICY CHANGE (macOS weight preservation, feature plan §3.1): the collapsed
    // + same-orientation-MERGED subtree keeps its PROPORTIONAL share of the parent
    // slot. The column occupied 0.5 of the row; its two survivors (0.5/0.5) scale
    // by that 0.5 to 0.25/0.25, while P1 keeps its 0.5. The old gnome 1:1 port
    // re-equalized to thirds. The TOPOLOGY (flat [P1, P2, P4]) is unchanged.
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
    // Build a clean 2x2 grid:
    //   split V (P1 left, P2 right), then split EACH side horizontally:
    //     split P1 -> P1 (top-left)    / P3 (bottom-left)
    //     split P2 -> P2 (top-right)   / P4 (bottom-right)
    // Resulting rects in 100x40:
    //   P1 (0, 0,50,20)   P2 (50, 0,50,20)
    //   P3 (0,20,50,20)   P4 (50,20,50,20)
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
  //
  // Guards the discriminant table shared with `wolf-ffi` (plan §3.1):
  //   orientation: HORIZONTAL = 0, VERTICAL = 1
  //   focus_dir:   LEFT = 0, RIGHT = 1, UP = 2, DOWN = 3
  // ASSUMPTION: the coder adds `Orientation.init?(ffi:)` / `FocusDir.init?(ffi:)`
  // per plan §2A. An out-of-range discriminant must map to nil.

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
