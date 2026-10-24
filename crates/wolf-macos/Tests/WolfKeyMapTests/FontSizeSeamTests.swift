// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#fontsizeseamtestsswift

import XCTest

@testable import WolfKeyMap

final class FontSizeSeamTests: XCTestCase {

  /// Tolerance for the exact-valued Double assertions below.
  private let eps: Double = 1e-9

  // MARK: - constants pin the documented bounds + step

  func testConstants() {
    XCTAssertEqual(FontSizeSeam.minPointSize, 6.0, accuracy: eps, "minPointSize == 6")
    XCTAssertEqual(FontSizeSeam.maxPointSize, 72.0, accuracy: eps, "maxPointSize == 72")
    XCTAssertEqual(FontSizeSeam.step, 1.0, accuracy: eps, "step == 1")
  }

  // MARK: - increased steps +1, clamps at the max

  func testIncreasedStepsUpByOneBelowMax() {
    XCTAssertEqual(
      FontSizeSeam.increased(14.0), 15.0, accuracy: eps,
      "increased(14) == 14 + 1 == 15 (well below the max)")
  }

  func testIncreasedClampsAtMax() {
    XCTAssertEqual(
      FontSizeSeam.increased(71.0), 72.0, accuracy: eps,
      "increased(71) == 72 (steps up to exactly the max)")
    XCTAssertEqual(
      FontSizeSeam.increased(72.0), 72.0, accuracy: eps,
      "increased(72) stays at 72 (already at the max)")
  }

  // MARK: - decreased steps -1, clamps at the min

  func testDecreasedStepsDownByOneAboveMin() {
    XCTAssertEqual(
      FontSizeSeam.decreased(14.0), 13.0, accuracy: eps,
      "decreased(14) == 14 - 1 == 13 (well above the min)")
  }

  func testDecreasedClampsAtMin() {
    XCTAssertEqual(
      FontSizeSeam.decreased(7.0), 6.0, accuracy: eps,
      "decreased(7) == 6 (steps down to exactly the min)")
    XCTAssertEqual(
      FontSizeSeam.decreased(6.0), 6.0, accuracy: eps,
      "decreased(6) stays at 6 (already at the min)")
  }

  // MARK: - clamped pins out-of-range inputs and passes through in-range

  func testClampedBelowMinReturnsMin() {
    XCTAssertEqual(
      FontSizeSeam.clamped(3.0), 6.0, accuracy: eps,
      "clamped(3) == 6 (below the min is pinned to the min)")
  }

  func testClampedAboveMaxReturnsMax() {
    XCTAssertEqual(
      FontSizeSeam.clamped(100.0), 72.0, accuracy: eps,
      "clamped(100) == 72 (above the max is pinned to the max)")
  }

  func testClampedInRangeIsUnchanged() {
    XCTAssertEqual(
      FontSizeSeam.clamped(14.0), 14.0, accuracy: eps,
      "clamped(14) == 14 (an in-range value is passed through)")
    XCTAssertEqual(
      FontSizeSeam.clamped(6.0), 6.0, accuracy: eps,
      "clamped(6) == 6 (the min is in range)")
    XCTAssertEqual(
      FontSizeSeam.clamped(72.0), 72.0, accuracy: eps,
      "clamped(72) == 72 (the max is in range)")
  }
}
