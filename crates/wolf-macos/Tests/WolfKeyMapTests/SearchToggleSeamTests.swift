// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#searchtoggleseamtestsswift

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
