// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#treechildrenmemotestsswift

import XCTest

@testable import WolfKeyMap

final class TreeChildrenMemoTests: XCTestCase {

  /// Number of repeated lookups of one directory (a named constant rather
  /// than a bare literal, mirroring the seam tests' convention).
  private let kLookupCount = 5

  /// The projection for a dir: two synthetic child items derived from the dir
  /// key, so results are deterministic and comparable.
  private func projection(for dir: String) -> [String] {
    [dir + ":a", dir + ":b"]
  }

  /// Repeated lookups of ONE dir project exactly once; every returned array is
  /// equal (served from the memo after the first compute).
  func testRepeatedLookupsForOneDirProjectOnce() {
    var projectCount = 0
    let memo = TreeChildrenMemo<String>(project: { dir in
      projectCount += 1
      return [dir + ":a", dir + ":b"]
    })

    var results: [[String]] = []
    for _ in 0..<kLookupCount {
      results.append(memo.children(ofDir: "src"))
    }

    XCTAssertEqual(
      projectCount, 1,
      "\(kLookupCount) lookups of one dir must project exactly once")
    for result in results {
      XCTAssertEqual(
        result, projection(for: "src"),
        "every memoized lookup must return the same projected array")
    }
  }

  /// Distinct dirs are memoized independently: two dirs project twice (once
  /// each), and each returns its own projection.
  func testDistinctDirsProjectPerDir() {
    var projectCount = 0
    let memo = TreeChildrenMemo<String>(project: { dir in
      projectCount += 1
      return [dir + ":a", dir + ":b"]
    })

    let src = memo.children(ofDir: "src")
    let docs = memo.children(ofDir: "docs")

    XCTAssertEqual(
      projectCount, 2,
      "two distinct dirs must each project once (total 2)")
    XCTAssertEqual(src, projection(for: "src"))
    XCTAssertEqual(docs, projection(for: "docs"))
  }

  /// `invalidate()` drops the cache, so the next lookup of the same dir
  /// recomputes (projectCount advances) and still returns the correct result.
  func testInvalidateForcesRecompute() {
    var projectCount = 0
    let memo = TreeChildrenMemo<String>(project: { dir in
      projectCount += 1
      return [dir + ":a", dir + ":b"]
    })

    let first = memo.children(ofDir: "src")
    XCTAssertEqual(projectCount, 1, "first lookup projects once")

    memo.invalidate()

    let second = memo.children(ofDir: "src")
    XCTAssertEqual(
      projectCount, 2,
      "a lookup after invalidate() must recompute (project again)")
    XCTAssertEqual(
      first, second,
      "the recomputed result must equal the original projection")
    XCTAssertEqual(second, projection(for: "src"))
  }

  /// The memo's output for a key equals calling the injected `project`
  /// directly for that same key.
  func testResultsMatchDirectProjection() {
    let project: (String) -> [String] = { dir in [dir + ":a", dir + ":b"] }
    let memo = TreeChildrenMemo<String>(project: project)

    for dir in ["", "src", "a/b", "docs"] {
      XCTAssertEqual(
        memo.children(ofDir: dir), project(dir),
        "memo output must match direct project(\(dir))")
    }
  }
}
