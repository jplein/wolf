//
//  CwdTitleSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.CwdTitleSeam` namespace.
//
//  A pure, display-free seam (plan §A) that computes the window title text from a
//  working-directory path, abbreviating the user's home directory to `~` per the
//  macOS convention. Exercised with scalar string inputs only — NO AppKit, NO
//  window, NO FileManager, NO implicit `$HOME` (the `home` argument is always
//  injected, which is load-bearing for determinism).
//
//  CONTRACT UNDER TEST:
//    * CwdTitleSeam.displayTitle(cwd: String, home: String) -> String
//      - empty or all-whitespace `cwd` => "" (caller substitutes an app-name
//        fallback).
//      - `home` is normalized by stripping ONE trailing "/" before comparison; an
//        empty `home` disables abbreviation and returns `cwd` unchanged.
//      - `cwd == normalizedHome` => "~".
//      - `cwd` begins with `normalizedHome + "/"` => "~/" + <remainder after the
//        "home/">. Abbreviation happens ONLY on a full path-segment boundary, so
//        "/Users/janet/x" must NOT be abbreviated against home "/Users/jane".
//      - otherwise (cwd not under home) => `cwd` returned verbatim (full absolute
//        path).
//
//  NOTE to coder: this test references the pure `WolfKeyMap.CwdTitleSeam` enum the
//  coder adds in `Sources/WolfKeyMap/CwdTitleSeam.swift` (and lists in the
//  `wolf_key_map` srcs in BUILD.bazel). The single function signature asserted
//  below is the contract the implementation must satisfy.
//

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
    // "/Users/janet/x" shares the textual prefix "/Users/jane" with home but is
    // NOT under it (the next character is "t", not "/"), so it must NOT become
    // "~t/x" — it is returned verbatim.
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
