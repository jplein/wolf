//
//  SearchBudgetSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.SearchBudgetSeam` namespace.
//
//  These exercise the display-free seam that converts a sidebar pixel width + a
//  per-character advance into an integer CHARACTER budget for a matched line
//  wrapped across up to two rows — scalar inputs only, NO AppKit / NSFont.
//
//  CONTRACT UNDER TEST (search plan §2, `SearchBudgetSeam.swift`):
//    * SearchBudgetSeam.charBudget(sidebarWidth:charAdvance:lines:) -> Int
//      == max(1, floor(width / advance)) * lines, with `lines` defaulting to 2
//      (the "wrapped to no more than two lines" rule). A degenerate
//      `charAdvance <= 0` must NOT crash or divide-by-zero and must return a
//      sensible positive floor (>= lines), never 0.
//
//  NOTE to coder: this test references the pure `WolfKeyMap.SearchBudgetSeam` enum
//  the coder adds in `Sources/WolfKeyMap/SearchBudgetSeam.swift`. The default
//  `lines: Int = 2` parameter is part of the asserted contract.
//

import XCTest

@testable import WolfKeyMap

/// The two-row wrap factor, named to avoid a bare magic number and to mirror the
/// seam's `lines` default.
private let kLines = 2

final class SearchBudgetSeamTests: XCTestCase {

  // MARK: - charBudget = floor(width / advance) * 2 (default lines)

  private struct BudgetCase {
    let desc: String
    let width: Double
    let advance: Double
    let expected: Int
  }

  func testCharBudgetDefaultTwoLines() {
    let cases: [BudgetCase] = [
      // Exact division: 100 / 10 = 10 chars per row, x2 rows = 20.
      BudgetCase(desc: "exact division", width: 100, advance: 10, expected: 20),
      // Fractional: floor(95 / 10) = floor(9.5) = 9, x2 = 18.
      BudgetCase(desc: "fractional floors down", width: 95, advance: 10, expected: 18),
      // Fractional: floor(100 / 7) = floor(14.28) = 14, x2 = 28.
      BudgetCase(desc: "non-integer advance floors", width: 100, advance: 7, expected: 28),
    ]
    for c in cases {
      let got = SearchBudgetSeam.charBudget(sidebarWidth: c.width, charAdvance: c.advance)
      XCTAssertEqual(got, c.expected, "case: \(c.desc)")
    }
  }

  // MARK: - the `lines` factor generalizes past the default 2

  func testCharBudgetHonorsLinesFactor() {
    // floor(100 / 10) = 10 chars per row; x1 = 10, x2 = 20, x3 = 30.
    XCTAssertEqual(
      SearchBudgetSeam.charBudget(sidebarWidth: 100, charAdvance: 10, lines: 1), 10,
      "one row => floor(width/advance)")
    XCTAssertEqual(
      SearchBudgetSeam.charBudget(sidebarWidth: 100, charAdvance: 10, lines: 3), 30,
      "three rows => floor(width/advance) * 3")
  }

  // MARK: - degenerate advance guard (no crash, no zero)

  func testDegenerateAdvanceReturnsSensibleFloor() {
    // advance == 0 would divide-by-zero / produce a non-finite quotient; the guard
    // must yield a positive floor of at least `lines` (max(1, ...) * lines), never 0.
    let zero = SearchBudgetSeam.charBudget(sidebarWidth: 100, charAdvance: 0)
    XCTAssertGreaterThanOrEqual(
      zero, kLines,
      "charAdvance == 0 must floor to at least `lines`, not 0/crash (got \(zero))")

    // A negative advance is equally nonsensical and must be guarded the same way.
    let negative = SearchBudgetSeam.charBudget(sidebarWidth: 100, charAdvance: -5)
    XCTAssertGreaterThanOrEqual(
      negative, kLines,
      "negative charAdvance must floor to at least `lines`, not 0/crash (got \(negative))")
  }
}
