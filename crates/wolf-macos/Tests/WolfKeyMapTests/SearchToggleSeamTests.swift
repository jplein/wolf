//
//  SearchToggleSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.SearchToggleSeam` namespace.
//
//  These exercise the display-free seam that seeds the search panel's three
//  toggle buttons with scalar inputs only — NO NSButton, NO window, NO AppKit.
//
//  CONTRACT UNDER TEST (search plan §2, `SearchToggleSeam.swift`):
//    * SearchToggleSeam.initialCaseSensitive(ignorecase:) -> Bool
//      — the "case sensitive" toggle is the INVERSE of nvim's `:ignorecase`:
//      ignorecase == true  => case-sensitive OFF (false)
//      ignorecase == false => case-sensitive ON  (true)
//    * SearchToggleSeam.defaultUseRegex == true   (use-regex ON by default)
//    * SearchToggleSeam.defaultAllFiles == false  (search-all-files OFF by default;
//      default is Markdown-only)
//
//  NOTE to coder: this test references the pure `WolfKeyMap.SearchToggleSeam` enum
//  the coder adds in `Sources/WolfKeyMap/SearchToggleSeam.swift`.
//

import XCTest

@testable import WolfKeyMap

final class SearchToggleSeamTests: XCTestCase {

  // MARK: - initialCaseSensitive inverts :ignorecase

  func testInitialCaseSensitiveInvertsIgnorecase() {
    XCTAssertFalse(
      SearchToggleSeam.initialCaseSensitive(ignorecase: true),
      "ignorecase ON => case-sensitive toggle starts OFF")
    XCTAssertTrue(
      SearchToggleSeam.initialCaseSensitive(ignorecase: false),
      "ignorecase OFF => case-sensitive toggle starts ON")
  }

  // MARK: - default toggle states

  func testDefaultUseRegexIsOn() {
    XCTAssertTrue(
      SearchToggleSeam.defaultUseRegex,
      "use-regex is ON by default")
  }

  func testDefaultAllFilesIsOff() {
    XCTAssertFalse(
      SearchToggleSeam.defaultAllFiles,
      "search-all-files is OFF by default (Markdown-only)")
  }
}
