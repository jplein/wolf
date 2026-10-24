// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#pickerpanelseamtestsswift

import XCTest

@testable import WolfKeyMap

final class PickerPanelSeamTests: XCTestCase {

  /// Tolerance for comparing the `Double` rect components (ported from
  /// `PaneGeometryTests.swift` / `EditorInsetTests.swift`).
  private let eps: Double = 1e-9

  /// The picker's fixed top margin below the content top, in points.
  private let topMargin: Double = 24.0

  /// The picker's fixed width, in points.
  private let panelWidth: Double = 600.0

  // MARK: - Helpers (ported from PaneGeometryTests' `approx` / `assertRectEqual`)

  /// Returns whether two doubles are within `eps`.
  private func approx(_ a: Double, _ b: Double) -> Bool {
    abs(a - b) < eps
  }

  /// Asserts a `Rect` matches another `Rect` component-by-component within `eps`.
  private func assertRectEqual(
    _ got: Rect,
    _ want: Rect,
    _ desc: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    XCTAssertTrue(
      approx(got.x, want.x) && approx(got.y, want.y)
        && approx(got.w, want.w) && approx(got.h, want.h),
      "case \(desc): got \(got), want \(want)",
      file: file,
      line: line
    )
  }

  // MARK: - Case 1: exact top-center numbers pinned

  func testExactTopCenter() {
    // A 600x300 panel in 1200x800 content, 24pt from the top: w = 600, x = (1200 - 600) / 2 = 300,
    // y = 24, h = min(300, 800 - 24) = 300.
    let r = PickerPanelSeam.frame(
      parentContentWidth: 1200, parentContentHeight: 800,
      panelWidth: panelWidth, panelHeight: 300, topMargin: topMargin)

    assertRectEqual(
      r, Rect(x: 300, y: 24, w: 600, h: 300),
      "frame(1200, 800, 600, 300, 24) == Rect(300, 24, 600, 300)")
  }

  // MARK: - Case 2: horizontal centering at a different content width

  func testHorizontalCenteringDifferentWidth() {
    // 600-wide panel in 1000-wide content -> x = (1000 - 600) / 2 = 200, and the
    // panel's horizontal center coincides with the content's center.
    let parentContentWidth: Double = 1000
    let r = PickerPanelSeam.frame(
      parentContentWidth: parentContentWidth, parentContentHeight: 800,
      panelWidth: panelWidth, panelHeight: 300, topMargin: topMargin)

    XCTAssertEqual(r.x, 200.0, accuracy: eps, "x == (1000 - 600) / 2 == 200")
    XCTAssertEqual(
      r.x + r.w / 2, parentContentWidth / 2, accuracy: eps,
      "panel center == content center (x + w/2 == parentContentWidth/2)")
  }

  // MARK: - Case 3: topMargin echoed unchanged (24 and 0)

  func testTopMarginRespected() {
    // y is a FIXED offset echoed straight through, for a non-zero AND a zero margin.
    let withMargin = PickerPanelSeam.frame(
      parentContentWidth: 1200, parentContentHeight: 800,
      panelWidth: panelWidth, panelHeight: 300, topMargin: topMargin)
    XCTAssertEqual(withMargin.y, topMargin, accuracy: eps, "y == topMargin == 24")

    let zeroMargin = PickerPanelSeam.frame(
      parentContentWidth: 1200, parentContentHeight: 800,
      panelWidth: panelWidth, panelHeight: 300, topMargin: 0)
    XCTAssertEqual(zeroMargin.y, 0.0, accuracy: eps, "y == topMargin == 0")
  }

  // MARK: - Case 4: height clamps when the panel is taller than content-below-margin

  func testHeightClampsToContentBelowMargin() {
    // 900-tall panel, content only 500 tall, 24pt margin:
    //   available = 500 - 24 = 476; h = min(900, 476) = 476. y stays a fixed 24.
    let r = PickerPanelSeam.frame(
      parentContentWidth: 1200, parentContentHeight: 500,
      panelWidth: panelWidth, panelHeight: 900, topMargin: topMargin)

    XCTAssertEqual(r.h, 476.0, accuracy: eps, "h clamps to available == 500 - 24 == 476")
    XCTAssertEqual(r.y, topMargin, accuracy: eps, "y stays the fixed 24pt offset")
  }

  // MARK: - Case 5: width clamps + x floors at 0 when the panel is wider than content

  func testWidthClampsAndXFloorsAtZero() {
    // 600-wide panel in only 400-wide content:
    //   w = min(600, 400) = 400; x = max(0, (400 - 400) / 2) = 0.
    let r = PickerPanelSeam.frame(
      parentContentWidth: 400, parentContentHeight: 800,
      panelWidth: panelWidth, panelHeight: 300, topMargin: topMargin)

    XCTAssertEqual(r.w, 400.0, accuracy: eps, "w clamps to the content width == 400")
    XCTAssertEqual(r.x, 0.0, accuracy: eps, "x floors at 0 (never negative)")
  }

  // MARK: - Case 6: exact-fit boundary (available == panelHeight, no clamp)

  func testExactFitBoundaryHeight() {
    // content 324 tall, margin 24 -> available = 300, exactly the panel height:
    // h = min(300, 300) = 300, no clamp at the inclusive boundary.
    let r = PickerPanelSeam.frame(
      parentContentWidth: 1200, parentContentHeight: 324,
      panelWidth: panelWidth, panelHeight: 300, topMargin: topMargin)

    XCTAssertEqual(r.h, 300.0, accuracy: eps, "h == 300 when available == 300 (no clamp)")
  }

  // MARK: - Case 7: degenerate content shorter than the margin

  func testContentShorterThanMarginClampsHeightToZero() {
    // content 10 tall, margin 24 -> available = max(0, 10 - 24) = 0, so h = 0.
    // y is a FIXED offset (24) and is NOT clamped even though it exceeds the content.
    let r = PickerPanelSeam.frame(
      parentContentWidth: 1200, parentContentHeight: 10,
      panelWidth: panelWidth, panelHeight: 300, topMargin: topMargin)

    XCTAssertEqual(r.h, 0.0, accuracy: eps, "h clamps to 0 when content is shorter than the margin")
    XCTAssertEqual(r.y, topMargin, accuracy: eps, "y stays the fixed 24pt offset (unclamped)")
  }

  // MARK: - Case 8: never-negative floors across all of the above inputs

  func testNeverNegativeFloors() {
    // Every case above must keep x >= 0 and h >= 0, including the degenerate ones.
    let inputs: [(w: Double, hgt: Double, pw: Double, ph: Double, margin: Double)] = [
      (1200, 800, 600, 300, 24),
      (1000, 800, 600, 300, 24),
      (1200, 800, 600, 300, 0),
      (1200, 500, 600, 900, 24),
      (400, 800, 600, 300, 24),
      (1200, 324, 600, 300, 24),
      (1200, 10, 600, 300, 24),
    ]
    for input in inputs {
      let r = PickerPanelSeam.frame(
        parentContentWidth: input.w, parentContentHeight: input.hgt,
        panelWidth: input.pw, panelHeight: input.ph, topMargin: input.margin)
      XCTAssertGreaterThanOrEqual(
        r.x, 0.0, "x is never negative for input \(input) (got \(r.x))")
      XCTAssertGreaterThanOrEqual(
        r.h, 0.0, "h is never negative for input \(input) (got \(r.h))")
    }
  }
}
