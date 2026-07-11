//
//  TabTitleSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.TabTitleSeam` namespace.
//
//  A pure, display-free seam (plan Phase 3 + OD5) that computes the per-tab
//  titles for the tabs in ONE window from their active buffer paths. Exercised
//  with scalar string inputs only — NO AppKit, NO window, NO live Neovim.
//
//  CONTRACT UNDER TEST:
//    * TabTitleSeam.labels(activeBufferPaths: [String]) -> [String]
//      - 1:1 and ORDER-PRESERVING: the result has the same count and order as the
//        input (result[i] is the label for activeBufferPaths[i]).
//      - Each label starts as the path's basename (last path component).
//      - Empty string, "[No Name]", and other "unnamed" inputs => "untitled".
//      - When two or more tabs in the SAME window share a basename, each colliding
//        label is disambiguated to "<basename> — <minimal trailing parent
//        segments>" using an EM DASH (U+2014) with a single space on each side.
//        The number of trailing parent segments grows minimally until every label
//        in the colliding set is distinct (segments joined path-order with "/").
//      - A distinct basename (including a bare filename with no slash) is left as
//        the bare basename.
//
//  NOTE to coder: this test references the pure `WolfKeyMap.TabTitleSeam` enum the
//  coder adds in `Sources/WolfKeyMap/TabTitleSeam.swift` (and lists in the
//  `wolf_key_map` srcs in BUILD.bazel). The single function signature + the exact
//  minimal-segment em-dash format (locked by OD5) are the contract the
//  implementation must satisfy.
//

import XCTest

@testable import WolfKeyMap

final class TabTitleSeamTests: XCTestCase {

    /// The unnamed/empty sentinel label.
    private let untitled = "untitled"

    /// Builds a disambiguated label with the LOCKED format: basename, a space, an
    /// EM DASH (U+2014), a space, then the trailing parent segments. Constructing
    /// it from the explicit scalar guarantees the exact code point under test.
    private func labeled(_ base: String, _ parents: String) -> String {
        "\(base) \u{2014} \(parents)"
    }

    // MARK: - distinct basenames pass through unchanged

    func testAllDistinctBasenamesPassThrough() {
        let input = ["/a/b/alpha.md", "/c/d/beta.txt", "/e/gamma.rs"]
        let expected = ["alpha.md", "beta.txt", "gamma.rs"]
        XCTAssertEqual(
            TabTitleSeam.labels(activeBufferPaths: input), expected,
            "distinct basenames must be left as bare basenames, order + count preserved")
    }

    // MARK: - two colliding basenames disambiguate with ONE parent segment

    func testTwoWayCollisionUsesOneParentSegment() {
        let input = ["/Users/jane/wolf/README.md", "/Users/jane/docs/README.md"]
        let expected = [labeled("README.md", "wolf"), labeled("README.md", "docs")]
        XCTAssertEqual(
            TabTitleSeam.labels(activeBufferPaths: input), expected,
            "two same-basename tabs must each extend to their (distinct) immediate parent")
    }

    // MARK: - three-way collision whose immediate parents ALSO collide

    func testThreeWayCollisionExtendsToTwoSegments() {
        // The immediate parent ("shared") is identical for all three, so ONE
        // segment does not disambiguate — the seam must grow to TWO trailing
        // segments, which are then all distinct.
        let input = ["/root/a/shared/x.md", "/root/b/shared/x.md", "/root/c/shared/x.md"]
        let expected = [
            labeled("x.md", "a/shared"),
            labeled("x.md", "b/shared"),
            labeled("x.md", "c/shared"),
        ]
        XCTAssertEqual(
            TabTitleSeam.labels(activeBufferPaths: input), expected,
            "a collision whose immediate parents also collide must extend to two segments")

        // Belt-and-suspenders: every produced label is distinct.
        let produced = TabTitleSeam.labels(activeBufferPaths: input)
        XCTAssertEqual(
            Set(produced).count, produced.count,
            "the disambiguated labels must all be distinct")
    }

    // MARK: - empty / unnamed => "untitled"; a bare distinct filename => basename

    func testEmptyAndUnnamedAreUntitledAndBareFileKeepsBasename() {
        // "" and "[No Name]" are the unnamed sentinels => "untitled". They both map
        // to "untitled" WITHOUT being path-disambiguated (there is no meaningful
        // path). A normal distinct path keeps its basename.
        let input = ["", "[No Name]", "/x/dir/notes.md"]
        let expected = [untitled, untitled, "notes.md"]
        XCTAssertEqual(
            TabTitleSeam.labels(activeBufferPaths: input), expected,
            "empty and [No Name] map to \"untitled\"; a distinct path keeps its basename")

        // A bare filename with NO slash is a NAMED buffer: when distinct it keeps
        // its basename (it is not "unnamed").
        let bare = ["/a/b/other.md", "file.md"]
        XCTAssertEqual(
            TabTitleSeam.labels(activeBufferPaths: bare), ["other.md", "file.md"],
            "a bare distinct filename with no parent keeps its basename")
    }

    // MARK: - 1:1 order preservation on a mixed (some colliding, some not) input

    func testOrderPreservedOnMixedInput() {
        let input = [
            "/x/README.md",  // collides with the other README.md
            "/proj/main.swift",  // distinct
            "/y/README.md",  // collides with the first README.md
            "/q/util.rs",  // distinct
        ]
        let expected = [
            labeled("README.md", "x"),
            "main.swift",
            labeled("README.md", "y"),
            "util.rs",
        ]
        let produced = TabTitleSeam.labels(activeBufferPaths: input)
        XCTAssertEqual(produced.count, input.count, "labels are 1:1 with the input")
        XCTAssertEqual(
            produced, expected,
            "mixed colliding/non-colliding tabs keep input order with per-tab labels")
    }
}
