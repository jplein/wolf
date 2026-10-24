// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#scrollaccumulatortestsswift

import XCTest

@testable import WolfKeyMap

final class ScrollAccumulatorTests: XCTestCase {

  /// The cell height used throughout (px). A whole `h` of deltaY is one step.
  private let h: Double = 16.0

  // MARK: - whole cell -> +/- 1 step

  func testWholeCellYieldsOneStep() {
    var acc = ScrollAccumulator()
    XCTAssertEqual(
      acc.consume(deltaY: h, cellHeight: h), 1,
      "a full +cellHeight of deltaY is exactly one positive step")

    var negAcc = ScrollAccumulator()
    XCTAssertEqual(
      negAcc.consume(deltaY: -h, cellHeight: h), -1,
      "a full -cellHeight of deltaY is exactly one negative step")
  }

  // MARK: - sub-cell deltas accumulate

  func testSubCellAccumulates() {
    // Two 0.6h nudges: the first (0.6h < h) yields 0; the second crosses h (1.2h)
    // and yields exactly 1, leaving a 0.2h remainder.
    var acc = ScrollAccumulator()
    XCTAssertEqual(
      acc.consume(deltaY: 0.6 * h, cellHeight: h), 0,
      "0.6 of a cell is below the threshold -> 0 steps")
    XCTAssertEqual(
      acc.consume(deltaY: 0.6 * h, cellHeight: h), 1,
      "a second 0.6 cell (1.2 total) crosses one cell -> 1 step")
  }

  // MARK: - remainder carries across calls

  func testRemainderCarriesAcrossCalls() {
    // 1.5h -> 1 step (0.5h remainder); then a further 0.5h reaches a full cell -> 1.
    var acc = ScrollAccumulator()
    XCTAssertEqual(
      acc.consume(deltaY: 1.5 * h, cellHeight: h), 1,
      "1.5 cells -> 1 step, carrying a 0.5-cell remainder")
    XCTAssertEqual(
      acc.consume(deltaY: 0.5 * h, cellHeight: h), 1,
      "the carried 0.5 cell + another 0.5 cell completes one cell -> 1 step")
  }

  // MARK: - sign change reverses

  func testSignChangeReverses() {
    // +1.5h -> 1 step, leaving a +0.5h remainder. A subsequent -2h takes the
    // remainder to -1.5h, crossing exactly one NEGATIVE cell boundary -> -1.
    var acc = ScrollAccumulator()
    XCTAssertEqual(
      acc.consume(deltaY: 1.5 * h, cellHeight: h), 1,
      "+1.5 cells -> +1 step, +0.5-cell remainder")
    XCTAssertEqual(
      acc.consume(deltaY: -2.0 * h, cellHeight: h), -1,
      "remainder +0.5 - 2.0 = -1.5 cells crosses one negative boundary -> -1 step")
  }

  // MARK: - zero cell height is safe

  func testZeroCellHeightIsSafe() {
    // A zero (or non-positive) cell height must short-circuit to 0 — never divide
    // by zero or loop forever.
    var acc = ScrollAccumulator()
    XCTAssertEqual(
      acc.consume(deltaY: 5.0 * h, cellHeight: 0.0), 0,
      "a zero cell height yields 0 steps (guarded)")
  }
}
