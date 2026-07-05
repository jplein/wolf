//
//  TreeChildrenMemoTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.TreeChildrenMemo` seam.
//
//  This is the per-directory memo behind the file-tree sidebar (plan Layer 1):
//  AppKit's `NSOutlineView` asks `numberOfChildrenOfItem` then `child:index:`
//  per child, each of which previously re-invoked the FFI `children_of(dir)`
//  from scratch — an O(K^2 * N) callback storm. `TreeChildrenMemo` collapses
//  that so `project(dir)` (the FFI hop) runs AT MOST ONCE per directory per
//  reload, serving every subsequent lookup from memory. `invalidate()` clears
//  the memo (called on refresh-reload and reroot).
//
//  These tests inject the seam's `project` closure so NO real FFI / outline view
//  is needed — exactly the headless style of the other WolfKeyMap seam tests
//  (PickerRefreshCoalescerTests etc.). The injected closure bumps a counter so
//  the tests assert the exact number of projections.
//
//  NOTE to coder: this references the pure generic `WolfKeyMap.TreeChildrenMemo<Item>`
//  the coder adds in `Sources/WolfKeyMap/TreeChildrenMemo.swift` (plan Layer 1).
//  The contract asserted below is:
//    * `init(project: @escaping (String) -> [Item])`
//    * `func children(ofDir dir: String) -> [Item]` — compute-once-per-dir:
//      first lookup calls `project(dir)` and caches; later lookups return the
//      cached array without calling `project`.
//    * `func invalidate()` — drop all cached per-dir results.
//

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
