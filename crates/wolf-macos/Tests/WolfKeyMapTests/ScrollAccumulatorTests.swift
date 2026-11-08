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

  // MARK: - rejected calls leave the carry untouched

  /// A non-finite `deltaY` returns 0 without touching the carry. The 0.6h carry set first
  /// must survive the three rejected calls, so the closing 0.6h nudge still crosses one cell.
  func testNonFiniteDeltaLeavesCarryUntouched() {
    var acc = ScrollAccumulator()
    XCTAssertEqual(
      acc.consume(deltaY: 0.6 * h, cellHeight: h), 0, "0.6 of a cell sets a carry, 0 steps")
    XCTAssertEqual(acc.consume(deltaY: .nan, cellHeight: h), 0, "a NaN deltaY yields 0 steps")
    XCTAssertEqual(
      acc.consume(deltaY: .infinity, cellHeight: h), 0, "a +inf deltaY yields 0 steps")
    XCTAssertEqual(
      acc.consume(deltaY: -.infinity, cellHeight: h), 0, "a -inf deltaY yields 0 steps")
    XCTAssertEqual(
      acc.consume(deltaY: 0.6 * h, cellHeight: h), 1,
      "the surviving 0.6 carry plus 0.6 crosses one cell; 0 means a rejected delta reached the "
        + "carry and zeroed or poisoned it")
  }

  /// The bug as a user sees it: one NaN `deltaY` must not stop all later scrolling. This case
  /// fails against an unguarded accumulator, but removing either the delta guard or the
  /// remainder guard alone leaves it green, because each covers for the other.
  func testNaNDeltaDoesNotPoisonLaterScrolling() {
    var acc = ScrollAccumulator()
    XCTAssertEqual(acc.consume(deltaY: .nan, cellHeight: h), 0, "a NaN deltaY yields 0 steps")
    XCTAssertEqual(
      acc.consume(deltaY: h, cellHeight: h), 1,
      "a whole cell after a NaN still scrolls one step; 0 means the NaN poisoned the carry")
  }

  /// A non-finite or non-positive `cellHeight` returns 0 without touching the carry, which
  /// must survive the three rejected calls.
  func testInvalidCellHeightLeavesCarryUntouched() {
    var acc = ScrollAccumulator()
    XCTAssertEqual(
      acc.consume(deltaY: 0.6 * h, cellHeight: h), 0, "0.6 of a cell sets a carry, 0 steps")
    XCTAssertEqual(
      acc.consume(deltaY: h, cellHeight: -h), 0, "a negative cellHeight yields 0 steps")
    XCTAssertEqual(
      acc.consume(deltaY: h, cellHeight: .nan), 0, "a NaN cellHeight yields 0 steps")
    XCTAssertEqual(
      acc.consume(deltaY: h, cellHeight: .infinity), 0, "a +inf cellHeight yields 0 steps")
    XCTAssertEqual(
      acc.consume(deltaY: 0.6 * h, cellHeight: h), 1,
      "the surviving 0.6 carry plus 0.6 crosses one cell; 0 means a rejected call added its "
        + "delta to the carry or poisoned it")
  }

  // MARK: - an overflowing carry is zeroed

  /// Two finite deltas, neither crossing a cell on its own, sum to +inf in the carry. That is
  /// the only way to reach the remainder guard with finite input.
  func testRemainderOverflowIsZeroedNotPoisoned() {
    var acc = ScrollAccumulator()
    let big = Double.greatestFiniteMagnitude
    XCTAssertEqual(
      acc.consume(deltaY: 1e308, cellHeight: big), 0, "1e308 is about 0.56 of the cell, 0 steps")
    XCTAssertEqual(
      acc.consume(deltaY: 1e308, cellHeight: big), 0,
      "a carry that overflows to inf is zeroed and yields 0; 256 means the inf carry was divided "
        + "and clamped instead")
    XCTAssertEqual(
      acc.consume(deltaY: big, cellHeight: big), 1,
      "after the zeroed carry one whole cell is one step; 0 means the carry stayed poisoned")
  }

  // MARK: - saturation at 256 steps per call

  /// One call yields at most 256 steps either way, in O(1). A `while`-loop form would iterate
  /// about 1e300 times here, so a hang in this test means the loops are back.
  func testEnormousDeltaSaturatesAt256() {
    var acc = ScrollAccumulator()
    XCTAssertEqual(
      acc.consume(deltaY: 1e300, cellHeight: 1), 256,
      "a 1e300 delta saturates at 256 steps; a crash means the unclamped Int conversion trapped")

    var negAcc = ScrollAccumulator()
    XCTAssertEqual(
      negAcc.consume(deltaY: -1e300, cellHeight: 1), -256,
      "a -1e300 delta saturates at -256 steps; a crash means the unclamped Int conversion trapped")
  }

  /// A call the clamp cut short drops its carry, so the leftover cannot keep firing.
  func testClampedDeltaDropsCarry() {
    var acc = ScrollAccumulator()
    XCTAssertEqual(
      acc.consume(deltaY: 1e300, cellHeight: 1), 256, "a 1e300 delta saturates at 256 steps")
    XCTAssertEqual(
      acc.consume(deltaY: 0, cellHeight: 1), 0,
      "a zero delta after a clamped call yields 0; 256 means the clamped call kept its carry")
  }
}
