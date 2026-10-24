// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#dividerdragseamtestsswift

import XCTest

@testable import WolfKeyMap

final class DividerDragSeamTests: XCTestCase {

  // MARK: - Fixed cell metrics + minima (>= 5 so named for clarity)

  private let cellWidth: Double = 10.0
  private let cellHeight: Double = 20.0
  private let minCols: Int = 10
  private let minRows: Int = 3

  /// Tolerance for `Double` comparisons.
  private let eps: Double = 1e-9

  private func approx(_ a: Double, _ b: Double) -> Bool {
    abs(a - b) < eps
  }

  // MARK: - middle drag passes through un-clamped

  func testMiddleDragPassesThrough() {
    // pairSpan 400, minLeft = 10*10 = 100 -> lower clamp 0.25, upper 0.75. A
    // pointer at the pair midpoint (raw 0.5) is inside the window -> untouched.
    let pairStart = 50.0
    let pairSpan = 400.0
    let f = DividerDragSeam.clampedLeftFraction(
      pairStart: pairStart, pairSpan: pairSpan, pointerPos: pairStart + 200.0,
      orientation: .vertical, cellWidth: cellWidth, cellHeight: cellHeight,
      minCols: minCols, minRows: minRows, leadingOverhead: 0, trailingOverhead: 0)
    XCTAssertTrue(approx(f, 0.5), "a mid-range pointer passes through as the raw fraction")
  }

  // MARK: - clamp past the LEFT min (both orientations)

  func testClampLeftKeepsMinColsVertical() {
    let pairSpan = 400.0
    // Drag far past the left edge; the left pane must clamp to exactly minCols cells.
    let f = DividerDragSeam.clampedLeftFraction(
      pairStart: 50.0, pairSpan: pairSpan, pointerPos: -1000.0,
      orientation: .vertical, cellWidth: cellWidth, cellHeight: cellHeight,
      minCols: minCols, minRows: minRows, leadingOverhead: 0, trailingOverhead: 0)
    let leftPixels = f * pairSpan
    XCTAssertTrue(
      approx(leftPixels, Double(minCols) * cellWidth),
      "clamped-left keeps exactly minCols columns of pixels; got \(leftPixels)")
  }

  func testClampLeftKeepsMinRowsHorizontal() {
    let pairSpan = 240.0
    let f = DividerDragSeam.clampedLeftFraction(
      pairStart: 0.0, pairSpan: pairSpan, pointerPos: -1000.0,
      orientation: .horizontal, cellWidth: cellWidth, cellHeight: cellHeight,
      minCols: minCols, minRows: minRows, leadingOverhead: 0, trailingOverhead: 0)
    let topPixels = f * pairSpan
    XCTAssertTrue(
      approx(topPixels, Double(minRows) * cellHeight),
      "clamped-top keeps exactly minRows rows of pixels; got \(topPixels)")
  }

  // MARK: - clamp past the RIGHT min (symmetric)

  func testClampRightKeepsMinColsVertical() {
    let pairSpan = 400.0
    // Drag far past the right edge; the RIGHT pane must clamp to minCols cells.
    let f = DividerDragSeam.clampedLeftFraction(
      pairStart: 50.0, pairSpan: pairSpan, pointerPos: 50.0 + pairSpan + 1000.0,
      orientation: .vertical, cellWidth: cellWidth, cellHeight: cellHeight,
      minCols: minCols, minRows: minRows, leadingOverhead: 0, trailingOverhead: 0)
    let rightPixels = (1.0 - f) * pairSpan
    XCTAssertTrue(
      approx(rightPixels, Double(minCols) * cellWidth),
      "clamped-right keeps exactly minCols columns of pixels for the RIGHT pane; got \(rightPixels)")
  }

  func testClampRightKeepsMinRowsHorizontal() {
    let pairSpan = 240.0
    let f = DividerDragSeam.clampedLeftFraction(
      pairStart: 0.0, pairSpan: pairSpan, pointerPos: pairSpan + 1000.0,
      orientation: .horizontal, cellWidth: cellWidth, cellHeight: cellHeight,
      minCols: minCols, minRows: minRows, leadingOverhead: 0, trailingOverhead: 0)
    let bottomPixels = (1.0 - f) * pairSpan
    XCTAssertTrue(
      approx(bottomPixels, Double(minRows) * cellHeight),
      "clamped-bottom keeps exactly minRows rows of pixels for the BOTTOM pane; got \(bottomPixels)")
  }

  // MARK: - overhead shrinks the usable span

  func testOverheadWidensTheMinimum() {
    let pairSpan = 400.0
    // With a leading overhead, the left minimum is minCols*cellWidth + overhead,
    // so the far-left clamp keeps MORE pixels than with no overhead.
    let leadingOverhead = 40.0
    let f = DividerDragSeam.clampedLeftFraction(
      pairStart: 0.0, pairSpan: pairSpan, pointerPos: -1000.0,
      orientation: .vertical, cellWidth: cellWidth, cellHeight: cellHeight,
      minCols: minCols, minRows: minRows,
      leadingOverhead: leadingOverhead, trailingOverhead: 0)
    let leftPixels = f * pairSpan
    XCTAssertTrue(
      approx(leftPixels, Double(minCols) * cellWidth + leadingOverhead),
      "the left clamp includes the leading overhead (min cells + overhead); got \(leftPixels)")
    XCTAssertGreaterThan(
      leftPixels, Double(minCols) * cellWidth,
      "a non-zero overhead makes the clamped minimum strictly larger")
  }

  // MARK: - degenerate: a pair too small for both minima -> midpoint

  func testDegeneratePairYieldsMidpoint() {
    // pairSpan 50 cannot hold two 100px minima -> the lower clamp exceeds the upper,
    // so the seam returns the midpoint 0.5 rather than an inverted range.
    let pairSpan = 50.0
    let f = DividerDragSeam.clampedLeftFraction(
      pairStart: 0.0, pairSpan: pairSpan, pointerPos: 10.0,
      orientation: .vertical, cellWidth: cellWidth, cellHeight: cellHeight,
      minCols: minCols, minRows: minRows, leadingOverhead: 0, trailingOverhead: 0)
    XCTAssertTrue(approx(f, 0.5), "a too-small pair collapses to the midpoint 0.5")
  }
}
