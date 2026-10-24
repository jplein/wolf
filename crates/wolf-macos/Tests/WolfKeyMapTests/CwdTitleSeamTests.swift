// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#cwdtitleseamtestsswift

import XCTest

@testable import WolfKeyMap

final class CwdTitleSeamTests: XCTestCase {

  /// A representative home directory used across cases.
  private let home = "/Users/jane"

  // MARK: - cwd under home => abbreviated

  func testCwdUnderHomeIsAbbreviated() {
    XCTAssertEqual(
      CwdTitleSeam.displayTitle(cwd: "/Users/jane/Git/wolf", home: home), "~/Git/wolf",
      "a cwd under home must abbreviate the home prefix to \"~/\"")
  }

  // MARK: - cwd exactly equals home => "~"

  func testCwdEqualsHomeIsTilde() {
    XCTAssertEqual(
      CwdTitleSeam.displayTitle(cwd: home, home: home), "~",
      "a cwd equal to home must abbreviate to a bare \"~\"")
  }

  // MARK: - cwd NOT under home => verbatim

  func testCwdNotUnderHomeIsVerbatim() {
    XCTAssertEqual(
      CwdTitleSeam.displayTitle(cwd: "/etc", home: home), "/etc",
      "a cwd outside home must be returned verbatim (full absolute path)")
  }

  // MARK: - prefix-collision guard (segment-boundary match only)

  func testPrefixCollisionIsNotAbbreviated() {
    // "/Users/janet/x" shares the text prefix "/Users/jane" but is not under it (next char is "t"),
    // so it is returned verbatim, not "~t/x".
    XCTAssertEqual(
      CwdTitleSeam.displayTitle(cwd: "/Users/janet/x", home: home), "/Users/janet/x",
      "a path that only shares a textual prefix (not a segment boundary) must not abbreviate")
  }

  // MARK: - home WITH a trailing slash normalizes identically

  func testTrailingSlashHomeNormalizesIdentically() {
    XCTAssertEqual(
      CwdTitleSeam.displayTitle(cwd: "/Users/jane/Git/wolf", home: "/Users/jane/"), "~/Git/wolf",
      "one trailing slash on home must be stripped, yielding the same abbreviation")
  }

  // MARK: - empty cwd => ""

  func testEmptyCwdIsEmptyString() {
    XCTAssertEqual(
      CwdTitleSeam.displayTitle(cwd: "", home: home), "",
      "an empty cwd must yield an empty string")
  }

  // MARK: - all-whitespace cwd => ""

  func testWhitespaceCwdIsEmptyString() {
    XCTAssertEqual(
      CwdTitleSeam.displayTitle(cwd: "   ", home: home), "",
      "an all-whitespace cwd must yield an empty string")
  }

  // MARK: - empty home disables abbreviation => verbatim

  func testEmptyHomeReturnsCwdVerbatim() {
    XCTAssertEqual(
      CwdTitleSeam.displayTitle(cwd: "/Users/jane/Git/wolf", home: ""), "/Users/jane/Git/wolf",
      "an empty home must disable abbreviation and return the cwd unchanged")
  }

  // MARK: - deep nested path preserves all trailing segments

  func testDeepNestedPathPreservesAllSegments() {
    XCTAssertEqual(
      CwdTitleSeam.displayTitle(cwd: "/Users/jane/a/b/c/d", home: home), "~/a/b/c/d",
      "a deeply nested cwd must keep every trailing segment after the home prefix")
  }
}
