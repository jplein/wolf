//
//  WindowIndexSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.WindowIndexSeam` namespace.
//
//  A pure, display-free seam (plan §5) that picks the window's `state.json` slot:
//  the smallest non-negative integer NOT already occupied. Exercised with scalar
//  array inputs only — NO AppKit, NO window. It reuses the lowest freed hole so
//  slots stay dense as windows open and close.
//
//  CONTRACT UNDER TEST:
//    * WindowIndexSeam.nextFreeIndex(occupied: [Int]) -> Int — the smallest
//      non-negative integer not present in `occupied`. Order and duplicates in
//      `occupied` are irrelevant.
//
//  NOTE to coder: this test references the pure `WolfKeyMap.WindowIndexSeam` enum
//  the coder adds in `Sources/WolfKeyMap/WindowIndexSeam.swift`. The single
//  function signature asserted below is the contract the implementation must
//  satisfy.
//

import XCTest

@testable import WolfKeyMap

final class WindowIndexSeamTests: XCTestCase {

    /// One `nextFreeIndex` table row: (description, occupied, expected).
    private struct FreeIndexCase {
        let desc: String
        let occupied: [Int]
        let expected: Int
    }

    func testNextFreeIndexTable() {
        let cases: [FreeIndexCase] = [
            // --- empty => the first slot -------------------------------------
            FreeIndexCase(
                desc: "no windows => 0",
                occupied: [], expected: 0),
            // --- dense run => append at the end ------------------------------
            FreeIndexCase(
                desc: "dense [0,1,2] => 3 (append)",
                occupied: [0, 1, 2], expected: 3),
            FreeIndexCase(
                desc: "single [1] (0 free) => 0",
                occupied: [1], expected: 0),
            // --- reuse the lowest hole ---------------------------------------
            FreeIndexCase(
                desc: "hole reuse [0,2] => 1",
                occupied: [0, 2], expected: 1),
            FreeIndexCase(
                desc: "hole reuse [0,1,3] => 2",
                occupied: [0, 1, 3], expected: 2),
            // --- order is irrelevant -----------------------------------------
            FreeIndexCase(
                desc: "unordered dense [2,0,1] => 3",
                occupied: [2, 0, 1], expected: 3),
        ]

        for c in cases {
            let got = WindowIndexSeam.nextFreeIndex(occupied: c.occupied)
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }
    }
}
