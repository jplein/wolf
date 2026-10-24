// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#editorinsettestsswift

import XCTest

@testable import WolfKeyMap

final class EditorInsetTests: XCTestCase {

  /// Tolerance for comparing the `Double` rect / scalar components (ported from
  /// `PaneGeometryTests.swift`).
  private let eps: Double = 1e-9

  /// The padding the plan pins as the single source of truth (points).
  private let pinnedPadding: Double = 20.0

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

  // MARK: - Exact case pinned to the representative numbers (plan §6)

  func testExactGridRectForRepresentativePane() {
    // pane 840x600, statusbar 40, padding 20: gridRect = (20, 40 + 20, 840 - 40, 600 - 40 - 40)
    // = (20, 60, 800, 520), and textLeftInset = 20.
    let inset = EditorInset(
      paneWidth: 840, paneHeight: 600, statusbarHeight: 40, padding: 20)

    assertRectEqual(
      inset.gridRect,
      Rect(x: 20, y: 60, w: 800, h: 520),
      "gridRect == Rect(20, 60, 800, 520)")
    XCTAssertEqual(
      inset.textLeftInset, 20.0, accuracy: eps, "textLeftInset == padding == 20")
  }

  // MARK: - Padding constant pinned

  func testPaddingConstantIsTwenty() {
    XCTAssertEqual(
      EditorInset.padding, pinnedPadding, accuracy: eps,
      "EditorInset.padding is pinned at 20pt")
  }

  // MARK: - Structural invariants (independent of the exact numbers)

  func testStructuralInvariants() {
    // A second, different pane (1000 x 700, statusbar 37, padding 20): assert the
    // additive relationships hold rather than re-deriving each field.
    let padding: Double = 20
    let statusbarHeight: Double = 37
    let paneWidth: Double = 1000
    let paneHeight: Double = 700
    let inset = EditorInset(
      paneWidth: paneWidth, paneHeight: paneHeight,
      statusbarHeight: statusbarHeight, padding: padding)

    XCTAssertEqual(
      inset.gridRect.x, padding, accuracy: eps,
      "gridRect.x == padding")
    XCTAssertEqual(
      inset.gridRect.y, statusbarHeight + padding, accuracy: eps,
      "gridRect.y == statusbarHeight + padding")
    XCTAssertEqual(
      inset.gridRect.w, paneWidth - 2 * padding, accuracy: eps,
      "gridRect.w == paneWidth - 2*padding")
    XCTAssertEqual(
      inset.gridRect.h, paneHeight - statusbarHeight - 2 * padding, accuracy: eps,
      "gridRect.h == paneHeight - statusbarHeight - 2*padding")
    XCTAssertEqual(
      inset.gridRect.w + 2 * padding, paneWidth, accuracy: eps,
      "gridRect.w + 2*padding == paneWidth")
    XCTAssertEqual(
      inset.gridRect.y + inset.gridRect.h + padding, paneHeight, accuracy: eps,
      "gridRect.y + gridRect.h + padding == paneHeight")
    XCTAssertEqual(
      inset.textLeftInset, padding, accuracy: eps,
      "textLeftInset == padding")
    XCTAssertEqual(
      inset.textLeftInset, inset.gridRect.x, accuracy: eps,
      "textLeftInset == gridRect.x")
  }

  // MARK: - Edge: a pane narrower than 2*padding clamps grid width to zero

  func testNarrowPaneClampsGridWidthToZero() {
    // pane 30 x 600, statusbar 40, padding 20: 30 - 2*20 == -10, clamped to 0.
    let inset = EditorInset(
      paneWidth: 30, paneHeight: 600, statusbarHeight: 40, padding: 20)

    XCTAssertEqual(
      inset.gridRect.w, 0.0, accuracy: eps,
      "gridRect.w clamps to 0 (never negative)")
    XCTAssertGreaterThanOrEqual(
      inset.gridRect.w, 0.0, "gridRect.w is never negative")
  }

  // MARK: - Edge: a pane shorter than statusbar + 2*padding clamps grid height to zero

  func testShortPaneClampsGridHeightToZero() {
    // pane 840 x 60, statusbar 40, padding 20: 60 - 40 - 2*20 == -20, clamped to 0.
    let inset = EditorInset(
      paneWidth: 840, paneHeight: 60, statusbarHeight: 40, padding: 20)

    XCTAssertEqual(
      inset.gridRect.h, 0.0, accuracy: eps,
      "gridRect.h clamps to 0 (never negative)")
    XCTAssertGreaterThanOrEqual(
      inset.gridRect.h, 0.0, "gridRect.h is never negative")
  }

  // MARK: - Edge: statusbar taller than the pane clamps height, y stays unclamped

  func testStatusbarTallerThanPaneClampsGridHeightToZero() {
    // pane 840x50, statusbar 80, padding 20: height 50 - 80 - 40 clamps to 0, but the fixed
    // y origin (80 + 20 = 100) stays unclamped even though it exceeds the pane height.
    let inset = EditorInset(
      paneWidth: 840, paneHeight: 50, statusbarHeight: 80, padding: 20)

    XCTAssertEqual(
      inset.gridRect.h, 0.0, accuracy: eps,
      "gridRect.h clamps to 0 when the statusbar is taller than the pane")
    XCTAssertEqual(
      inset.gridRect.y, 100.0, accuracy: eps,
      "gridRect.y == statusbarHeight + padding == 100 (unclamped offset)")
  }
}
