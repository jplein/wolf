// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#dividerhittesttestsswift

import XCTest

@testable import WolfKeyMap

final class DividerHitTestTests: XCTestCase {

  /// Hover tolerance used across the cases (points).
  private let tolerance: Double = 4.0

  /// Builds a vertical divider: a line at `x` spanning `y0..y0+height`.
  private func verticalDivider(
    x: Double, y0: Double, height: Double, path: [Int], boundary: Int
  ) -> PaneTree.Divider {
    PaneTree.Divider(
      orientation: .vertical, path: path, boundary: boundary,
      lineRect: Rect(x: x, y: y0, w: 0, h: height),
      pairStart: 0, pairSpan: height, before: 0, after: 0, fraction: 0.5)
  }

  /// Builds a horizontal divider: a line at `y` spanning `x0..x0+width`.
  private func horizontalDivider(
    y: Double, x0: Double, width: Double, path: [Int], boundary: Int
  ) -> PaneTree.Divider {
    PaneTree.Divider(
      orientation: .horizontal, path: path, boundary: boundary,
      lineRect: Rect(x: x0, y: y, w: width, h: 0),
      pairStart: 0, pairSpan: width, before: 0, after: 0, fraction: 0.5)
  }

  // MARK: - within / beyond tolerance

  func testPointWithinToleranceHitsTheDivider() {
    let v = verticalDivider(x: 50, y0: 0, height: 40, path: [], boundary: 0)
    // 2 points off in x, comfortably within the y-span.
    let hit = DividerHitTest.hit([v], point: (x: 52, y: 20), tolerance: tolerance)
    XCTAssertEqual(hit, v, "a point 2pt from a divider line (tolerance 4) hits it")
  }

  func testPointBeyondToleranceMisses() {
    let v = verticalDivider(x: 50, y0: 0, height: 40, path: [], boundary: 0)
    // 10pt off in x, well beyond the 4pt tolerance.
    XCTAssertNil(
      DividerHitTest.hit([v], point: (x: 60, y: 20), tolerance: tolerance),
      "a point 10pt from the line (tolerance 4) does not hit")
  }

  func testPointBeyondSpanMisses() {
    // On the line in x, but ABOVE the top end of the segment (y-span 0..40).
    let v = verticalDivider(x: 50, y0: 0, height: 40, path: [], boundary: 0)
    XCTAssertNil(
      DividerHitTest.hit([v], point: (x: 50, y: -10), tolerance: tolerance),
      "a point off the end of the line segment does not hit even when on the line's axis")
  }

  func testHorizontalDividerHit() {
    let hz = horizontalDivider(y: 20, x0: 0, width: 100, path: [0], boundary: 0)
    XCTAssertEqual(
      DividerHitTest.hit([hz], point: (x: 40, y: 22), tolerance: tolerance), hz,
      "a horizontal divider hits on a small y offset within its x-span")
    XCTAssertNil(
      DividerHitTest.hit([hz], point: (x: 40, y: 30), tolerance: tolerance),
      "10pt off in y misses the horizontal divider")
  }

  // MARK: - nearest wins near an intersection

  func testNearIntersectionPicksTheNearerDivider() {
    // V at x=50 (y 0..40) crosses H at y=20 (x 0..100); a point 1pt from V and 3pt from H
    // must pick the nearer, vertical one.
    let v = verticalDivider(x: 50, y0: 0, height: 40, path: [], boundary: 0)
    let hz = horizontalDivider(y: 20, x0: 0, width: 100, path: [0], boundary: 0)

    let hit = DividerHitTest.hit([v, hz], point: (x: 51, y: 23), tolerance: tolerance)
    XCTAssertEqual(hit, v, "1pt from the vertical vs 3pt from the horizontal -> the vertical wins")

    // Symmetric: closer to the horizontal line picks it.
    let hit2 = DividerHitTest.hit([v, hz], point: (x: 53, y: 21), tolerance: tolerance)
    XCTAssertEqual(hit2, hz, "1pt from the horizontal vs 3pt from the vertical -> the horizontal wins")
  }

  // MARK: - empty list

  func testEmptyListReturnsNil() {
    XCTAssertNil(
      DividerHitTest.hit([], point: (x: 10, y: 10), tolerance: tolerance),
      "no dividers -> no hit")
  }
}
