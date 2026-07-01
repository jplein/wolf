//
//  PickerSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.PickerSeam` namespace.
//
//  These are the AppKit analog of wolf-gnome's headless picker-seam tests in
//  `crates/wolf-gnome/src/picker.rs` (`is_picker_trigger_table`,
//  `next_selection_table`, `restored_index_table`). They exercise the THREE
//  PURE, display-free seams of the macOS picker with scalar inputs only — NO
//  NSEvent, NO window, NO AppKit, NO NSView. They are the authoritative parity
//  reference: every case here mirrors a case from picker.rs.
//
//  CONTRACT UNDER TEST (must match wolf-gnome/src/picker.rs exactly):
//    * PickerSeam.isPickerTrigger(charactersIgnoringModifiers:ctrl:shift:) ->
//      Bool — true iff `ctrl && shift && firstScalar.lowercased == "p"`. Other
//      modifiers (alt/meta) are NOT part of the Swift signature, so they are
//      neither required nor disqualifying; they are simply ignored. An empty
//      string (no first scalar) is false.
//    * PickerSeam.nextSelection(current:delta:nRows:) -> Int? — exact port of
//      Rust `next_selection`: `nRows <= 0 -> nil`, else
//      `clamp(current + delta, 0, nRows - 1)` (NO wrap; out-of-range/negative
//      `current` handled by clamping the sum). Swift `Int?` is the analog of
//      Rust `Option<usize>`.
//    * PickerSeam.restoredIndex(resetToTop:prev:newLen:) -> Int? — exact port of
//      Rust `restored_index`: `newLen == 0 -> nil` regardless of reset/prev;
//      else `resetToTop -> 0`; else `min(prev ?? 0, newLen - 1)`.
//
//  NOTE to coder: this test references the pure `WolfKeyMap.PickerSeam` enum the
//  coder will add in `Sources/WolfKeyMap/PickerSeam.swift`. The three function
//  signatures asserted below are the contract the implementation must satisfy.
//

import XCTest

@testable import WolfKeyMap

/// Row count reused across the navigation/restore tables (named to avoid a
/// magic number, mirroring `let n: i32 = 5` / `const LEN: usize = 5` in
/// picker.rs).
private let kNRows = 5

final class PickerSeamTests: XCTestCase {

    // MARK: - isPickerTrigger

    /// One `isPickerTrigger` table row: (description, charactersIgnoringModifiers,
    /// ctrl, shift, expected).
    private struct TriggerCase {
        let desc: String
        let chars: String
        let ctrl: Bool
        let shift: Bool
        let expected: Bool
    }

    func testIsPickerTriggerTable() {
        let cases: [TriggerCase] = [
            // --- The trigger: Ctrl+Shift held, letter p ----------------------
            TriggerCase(
                desc: "Ctrl+Shift+'p' (lowercase) => true",
                chars: "p", ctrl: true, shift: true, expected: true),
            TriggerCase(
                // With Shift held, charactersIgnoringModifiers carries the
                // uppercase `P`; the predicate must match case-insensitively.
                desc: "Ctrl+Shift+'P' (uppercase) => true",
                chars: "P", ctrl: true, shift: true, expected: true),
            TriggerCase(
                // The Swift signature does NOT take alt/meta, so the
                // "extra modifier allowed" case from picker.rs is implicitly
                // covered: ctrl+shift+'p' is true regardless of alt/meta, which
                // the signature ignores entirely.
                desc: "Ctrl+Shift+'p' is true regardless (alt/meta ignored by signature)",
                chars: "p", ctrl: true, shift: true, expected: true),
            // --- Missing a required modifier ---------------------------------
            TriggerCase(
                desc: "Ctrl+'p' (no shift) => false",
                chars: "p", ctrl: true, shift: false, expected: false),
            TriggerCase(
                desc: "Shift+'P' (no ctrl) => false",
                chars: "P", ctrl: false, shift: true, expected: false),
            TriggerCase(
                desc: "plain 'p' (no modifiers) => false",
                chars: "p", ctrl: false, shift: false, expected: false),
            // --- Right modifiers, wrong letter -------------------------------
            TriggerCase(
                desc: "Ctrl+Shift+'A' (wrong letter) => false",
                chars: "A", ctrl: true, shift: true, expected: false),
            TriggerCase(
                desc: "Ctrl+Shift+'a' (wrong letter, lowercase) => false",
                chars: "a", ctrl: true, shift: true, expected: false),
            // --- No first scalar ---------------------------------------------
            TriggerCase(
                desc: "empty string (no first scalar) => false",
                chars: "", ctrl: true, shift: true, expected: false),
        ]

        for c in cases {
            let got = PickerSeam.isPickerTrigger(
                charactersIgnoringModifiers: c.chars, ctrl: c.ctrl, shift: c.shift)
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }
    }

    // MARK: - nextSelection

    /// One `nextSelection` table row: (description, current, delta, nRows,
    /// expected). `expected == nil` is the Rust `None` analog.
    private struct NextCase {
        let desc: String
        let current: Int
        let delta: Int
        let nRows: Int
        let expected: Int?
    }

    func testNextSelectionTable() {
        let n = kNRows
        let cases: [NextCase] = [
            // --- move down within range --------------------------------------
            NextCase(
                desc: "down from top stays in range",
                current: 0, delta: 1, nRows: n, expected: 1),
            NextCase(
                desc: "down to last row",
                current: 3, delta: 1, nRows: n, expected: 4),
            // --- move up within range ----------------------------------------
            NextCase(
                desc: "up within range",
                current: 3, delta: -1, nRows: n, expected: 2),
            // --- clamp at the ends (NO wrap) ---------------------------------
            NextCase(
                desc: "down at bottom clamps (no wrap)",
                current: 4, delta: 1, nRows: n, expected: 4),
            NextCase(
                desc: "up at top clamps (no wrap)",
                current: 0, delta: -1, nRows: n, expected: 0),
            // --- empty list --------------------------------------------------
            NextCase(
                desc: "empty list, down => nil",
                current: 0, delta: 1, nRows: 0, expected: nil),
            NextCase(
                desc: "empty list, up => nil",
                current: 0, delta: -1, nRows: 0, expected: nil),
            // --- single row --------------------------------------------------
            NextCase(
                desc: "single row, down clamps to 0",
                current: 0, delta: 1, nRows: 1, expected: 0),
            NextCase(
                desc: "single row, up clamps to 0",
                current: 0, delta: -1, nRows: 1, expected: 0),
            // --- current out of range / negative -----------------------------
            NextCase(
                desc: "current above range clamps to last (sum clamped)",
                current: 99, delta: 1, nRows: n, expected: 4),
            NextCase(
                desc: "current negative clamps to 0 (sum clamped)",
                current: -5, delta: 1, nRows: n, expected: 0),
        ]

        for c in cases {
            let got = PickerSeam.nextSelection(
                current: c.current, delta: c.delta, nRows: c.nRows)
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }
    }

    // MARK: - restoredIndex

    /// One `restoredIndex` table row: (description, resetToTop, prev, newLen,
    /// expected). `prev == nil` is the Rust `None` analog of `Option<usize>`.
    private struct RestoredCase {
        let desc: String
        let resetToTop: Bool
        let prev: Int?
        let newLen: Int
        let expected: Int?
    }

    func testRestoredIndexTable() {
        let len = kNRows
        let cases: [RestoredCase] = [
            // --- empty list always nil, regardless of reset/prev -------------
            RestoredCase(
                desc: "empty list with reset => nil",
                resetToTop: true, prev: 3, newLen: 0, expected: nil),
            RestoredCase(
                desc: "empty list, preserve => nil",
                resetToTop: false, prev: 3, newLen: 0, expected: nil),
            // --- reset to top ------------------------------------------------
            RestoredCase(
                desc: "reset with prev set => 0",
                resetToTop: true, prev: 3, newLen: len, expected: 0),
            RestoredCase(
                desc: "reset with no prev => 0",
                resetToTop: true, prev: nil, newLen: len, expected: 0),
            // --- preserve previous selection ---------------------------------
            RestoredCase(
                desc: "preserve in range",
                resetToTop: false, prev: 3, newLen: len, expected: 3),
            RestoredCase(
                desc: "preserve clamped when prev beyond new len",
                resetToTop: false, prev: 9, newLen: len, expected: 4),
            RestoredCase(
                desc: "preserve with no prev defaults to 0",
                resetToTop: false, prev: nil, newLen: len, expected: 0),
        ]

        for c in cases {
            let got = PickerSeam.restoredIndex(
                resetToTop: c.resetToTop, prev: c.prev, newLen: c.newLen)
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }
    }

    // MARK: - shouldShowResults

    /// One `shouldShowResults` table row: (description, query, expected).
    ///
    /// macOS-only presentation seam (no Rust/wolf-gnome parity): the picker
    /// shows no result rows until the user has typed a non-whitespace query.
    private struct ShouldShowResultsCase {
        let desc: String
        let query: String
        let expected: Bool
    }

    func testShouldShowResultsTable() {
        let cases: [ShouldShowResultsCase] = [
            // --- empty / whitespace-only queries hide results ----------------
            ShouldShowResultsCase(
                desc: "empty string => false",
                query: "", expected: false),
            ShouldShowResultsCase(
                desc: "spaces only => false",
                query: "   ", expected: false),
            ShouldShowResultsCase(
                desc: "newline + tab only => false",
                query: "\n\t", expected: false),
            // --- non-whitespace queries show results -------------------------
            ShouldShowResultsCase(
                desc: "single char => true",
                query: "a", expected: true),
            ShouldShowResultsCase(
                desc: "word => true",
                query: "foo", expected: true),
            ShouldShowResultsCase(
                desc: "word padded with spaces => true",
                query: " foo ", expected: true),
        ]

        for c in cases {
            let got = PickerSeam.shouldShowResults(query: c.query)
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }
    }

    // MARK: - baseName

    /// One `baseName` table row: (description, relativePath, expected).
    ///
    /// macOS-only presentation seam (no Rust/wolf-gnome parity): derives the
    /// primary title (base filename) from a cwd-relative display path via
    /// `NSString.lastPathComponent`.
    private struct BaseNameCase {
        let desc: String
        let relativePath: String
        let expected: String
    }

    func testBaseNameTable() {
        let cases: [BaseNameCase] = [
            BaseNameCase(
                desc: "empty path => empty",
                relativePath: "", expected: ""),
            BaseNameCase(
                desc: "cwd-root file returns itself",
                relativePath: "README.md", expected: "README.md"),
            BaseNameCase(
                desc: "nested path returns last component",
                relativePath: "src/foo/bar.swift", expected: "bar.swift"),
            BaseNameCase(
                desc: "extensionless nested path returns last component",
                relativePath: "a/b/c", expected: "c"),
            BaseNameCase(
                desc: "trailing slash trimmed (lastPathComponent robustness)",
                relativePath: "dir/", expected: "dir"),
        ]

        for c in cases {
            let got = PickerSeam.baseName(fromRelativePath: c.relativePath)
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }
    }

    // MARK: - parentRelativePath

    /// One `parentRelativePath` table row: (description, relativePath,
    /// expected).
    ///
    /// macOS-only presentation seam (no Rust/wolf-gnome parity): derives the
    /// subtitle (relative parent directory) from a cwd-relative display path via
    /// `NSString.deletingLastPathComponent`.
    private struct ParentRelativePathCase {
        let desc: String
        let relativePath: String
        let expected: String
    }

    func testParentRelativePathTable() {
        let cases: [ParentRelativePathCase] = [
            ParentRelativePathCase(
                desc: "empty path => empty",
                relativePath: "", expected: ""),
            ParentRelativePathCase(
                desc: "cwd-root file has no parent",
                relativePath: "README.md", expected: ""),
            ParentRelativePathCase(
                desc: "single-level parent",
                relativePath: "a/b.txt", expected: "a"),
            ParentRelativePathCase(
                desc: "multi-level parent",
                relativePath: "src/foo/bar.swift", expected: "src/foo"),
            ParentRelativePathCase(
                desc: "extensionless multi-level parent",
                relativePath: "a/b/c", expected: "a/b"),
        ]

        for c in cases {
            let got = PickerSeam.parentRelativePath(fromRelativePath: c.relativePath)
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }
    }
}
