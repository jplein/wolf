// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#statusbarlayouttestsswift

import XCTest

@testable import WolfKeyMap

final class StatusbarLayoutTests: XCTestCase {

  /// Tolerance for the exact-valued Double assertions below (ported from the
  /// sibling headless tests' `eps`).
  private let eps: Double = 1e-9

  /// The pinned line factor (`ceil(fontSize * lineFactor)`).
  private let lineFactor: Double = 1.25

  // MARK: - Helpers

  /// Builds a layout from the four inputs, in the initializer's argument order.
  private func layout(
    primary: Double, secondary: Double, padding: Double, gap: Double
  ) -> StatusbarLayout {
    StatusbarLayout(
      primaryFontSize: primary,
      secondaryFontSize: secondary,
      verticalPadding: padding,
      lineGap: gap)
  }

  /// The expected line height for a font size under the pinned line factor.
  private func expectedLineHeight(_ fontSize: Double) -> Double {
    (fontSize * lineFactor).rounded(.up)
  }

  /// Asserts every structural invariant of a `StatusbarLayout`,
  /// independent of the exact line factor.
  private func assertInvariants(
    _ l: StatusbarLayout,
    padding: Double,
    gap: Double,
    _ desc: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    // All heights strictly positive.
    XCTAssertGreaterThan(
      l.primaryLineHeight, 0.0, "\(desc): primaryLineHeight > 0", file: file, line: line)
    XCTAssertGreaterThan(
      l.secondaryLineHeight, 0.0, "\(desc): secondaryLineHeight > 0", file: file, line: line)
    XCTAssertGreaterThan(l.height, 0.0, "\(desc): height > 0", file: file, line: line)

    // The primary line sits at the top padding.
    XCTAssertEqual(
      l.primaryLineTop, padding, accuracy: eps,
      "\(desc): primaryLineTop == verticalPadding", file: file, line: line)

    // The secondary line is strictly below the primary line.
    XCTAssertGreaterThan(
      l.secondaryLineTop, l.primaryLineTop,
      "\(desc): secondaryLineTop > primaryLineTop", file: file, line: line)

    // The secondary top is exactly the primary top + primary height + the gap.
    XCTAssertEqual(
      l.primaryLineTop + l.primaryLineHeight + gap, l.secondaryLineTop, accuracy: eps,
      "\(desc): primaryLineTop + primaryLineHeight + lineGap == secondaryLineTop",
      file: file, line: line)

    // The strip is tall enough to hold both lines.
    XCTAssertGreaterThanOrEqual(
      l.height, l.secondaryLineTop + l.secondaryLineHeight,
      "\(desc): height >= secondaryLineTop + secondaryLineHeight", file: file, line: line)
  }

  /// Asserts every field of a collapsed layout is exactly zero. `==` is false for NaN,
  /// so a poisoned field fails here as surely as an infinite or non-zero one.
  private func assertCollapsed(
    _ l: StatusbarLayout,
    _ desc: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    let fields: [(String, Double)] = [
      ("height", l.height),
      ("primaryLineTop", l.primaryLineTop),
      ("primaryLineHeight", l.primaryLineHeight),
      ("secondaryLineTop", l.secondaryLineTop),
      ("secondaryLineHeight", l.secondaryLineHeight),
    ]
    for (name, value) in fields {
      XCTAssertEqual(
        value, 0.0,
        "\(desc): \(name) must collapse to 0; NaN or inf means the input reached the "
          + "arithmetic unguarded, a finite non-zero value that the guard let this input through",
        file: file, line: line)
    }
  }

  // MARK: - Exact height pinned to lineFactor == 1.25

  func testExactHeightForRepresentativeInputs() {
    // primary 13, secondary 10, padding 3, gap 1: line heights ceil(16.25) = 17, ceil(12.5) = 13,
    // tops 3 and 3 + 17 + 1 = 21, height 21 + 13 + 3 = 37.
    let l = layout(primary: 13, secondary: 10, padding: 3, gap: 1)

    XCTAssertEqual(l.primaryLineHeight, 17.0, accuracy: eps, "ceil(13 * 1.25) == 17")
    XCTAssertEqual(l.secondaryLineHeight, 13.0, accuracy: eps, "ceil(10 * 1.25) == 13")
    XCTAssertEqual(l.primaryLineTop, 3.0, accuracy: eps, "primaryLineTop == verticalPadding")
    XCTAssertEqual(l.secondaryLineTop, 21.0, accuracy: eps, "3 + 17 + 1 == 21")
    XCTAssertEqual(l.height, 37.0, accuracy: eps, "21 + 13 + 3 == 37")
  }

  // MARK: - Full arithmetic cross-checked against the pinned formula

  func testHeightMatchesFormulaForSeveralInputs() {
    // Each tuple: (primary, secondary, padding, gap). Non-integer factor
    // products (e.g. 13 * 1.25) exercise the ceil rounding.
    let cases: [(Double, Double, Double, Double)] = [
      (13, 10, 3, 1),
      (14, 11, 4, 2),
      (20, 16, 5, 0),
      (12, 9, 2, 1),
    ]
    for (primary, secondary, padding, gap) in cases {
      let l = layout(primary: primary, secondary: secondary, padding: padding, gap: gap)
      let desc = "primary=\(primary) secondary=\(secondary) pad=\(padding) gap=\(gap)"

      let ph = expectedLineHeight(primary)
      let sh = expectedLineHeight(secondary)
      let secondaryTop = padding + ph + gap
      let expectedHeight = secondaryTop + sh + padding

      XCTAssertEqual(l.primaryLineHeight, ph, accuracy: eps, "\(desc): primaryLineHeight")
      XCTAssertEqual(l.secondaryLineHeight, sh, accuracy: eps, "\(desc): secondaryLineHeight")
      XCTAssertEqual(l.secondaryLineTop, secondaryTop, accuracy: eps, "\(desc): secondaryLineTop")
      XCTAssertEqual(l.height, expectedHeight, accuracy: eps, "\(desc): height")

      assertInvariants(l, padding: padding, gap: gap, desc)
    }
  }

  // MARK: - Zero-padding edge

  func testZeroPaddingStillPositiveAndOrdered() {
    // padding 0: primaryLineTop collapses to 0 but the lines stay positive and
    // ordered (secondaryLineTop > 0 == primaryLineTop).
    let l = layout(primary: 13, secondary: 10, padding: 0, gap: 1)
    assertInvariants(l, padding: 0, gap: 1, "zero padding")
    XCTAssertEqual(l.primaryLineTop, 0.0, accuracy: eps, "primaryLineTop == 0 with no padding")
    XCTAssertGreaterThan(l.secondaryLineTop, 0.0, "secondaryLineTop stays positive with no padding")
  }

  // MARK: - Small-font edge

  func testSmallFontStillPositiveAndOrdered() {
    // A tiny font (1pt) still yields a positive line height (ceil(1.25) == 2)
    // and correctly ordered lines.
    let l = layout(primary: 1, secondary: 1, padding: 1, gap: 1)
    assertInvariants(l, padding: 1, gap: 1, "small font")
    XCTAssertEqual(l.primaryLineHeight, 2.0, accuracy: eps, "ceil(1 * 1.25) == 2")
    XCTAssertEqual(l.secondaryLineHeight, 2.0, accuracy: eps, "ceil(1 * 1.25) == 2")
  }

  // MARK: - Degenerate input collapses the whole layout

  /// A non-finite input in any of the four positions collapses all five fields, so the
  /// strip is zero-height rather than infinite. The other inputs are the 13/10/3/1 example.
  func testNonFiniteInputCollapsesWholeLayout() {
    assertCollapsed(layout(primary: .nan, secondary: 10, padding: 3, gap: 1), "NaN primary")
    assertCollapsed(
      layout(primary: 13, secondary: .infinity, padding: 3, gap: 1), "+inf secondary")
    assertCollapsed(layout(primary: 13, secondary: 10, padding: .infinity, gap: 1), "+inf padding")
    assertCollapsed(layout(primary: 13, secondary: 10, padding: 3, gap: .nan), "NaN gap")
  }

  /// A negative padding or gap collapses the layout too, as wolf-gnome's `statusbar_layout`
  /// does. Zero stays usable: `testZeroPaddingStillPositiveAndOrdered` pins that.
  func testNegativeInputCollapsesWholeLayout() {
    assertCollapsed(layout(primary: 13, secondary: 10, padding: -3, gap: 1), "negative padding")
    assertCollapsed(layout(primary: 13, secondary: 10, padding: 3, gap: -1), "negative gap")
  }
}
