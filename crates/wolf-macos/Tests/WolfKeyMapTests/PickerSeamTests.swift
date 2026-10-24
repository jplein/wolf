// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#pickerseamtestsswift

import XCTest

@testable import WolfKeyMap

/// Row count reused across the navigation/restore tables (named to avoid a
/// magic number, mirroring `let n: i32 = 5` / `const LEN: usize = 5` in
/// picker.rs).
private let kNRows = 5

final class PickerSeamTests: XCTestCase {

  // MARK: - isPickerTrigger

  /// One `isPickerTrigger` table row: (description, charactersIgnoringModifiers,
  /// command, expected).
  private struct TriggerCase {
    let desc: String
    let chars: String
    let command: Bool
    let expected: Bool
  }

  func testIsPickerTriggerTable() {
    let cases: [TriggerCase] = [
      // --- The trigger: Command held, letter o -------------------------
      TriggerCase(
        desc: "Command+'o' (lowercase) => true",
        chars: "o", command: true, expected: true),
      TriggerCase(
        // With Command+Shift held, charactersIgnoringModifiers carries
        // the uppercase `O`; the predicate must match case-insensitively.
        desc: "Command+'O' (uppercase, i.e. Command+Shift+O) => true",
        chars: "O", command: true, expected: true),
      // --- Missing the required modifier -------------------------------
      TriggerCase(
        desc: "plain 'o' (command not held) => false",
        chars: "o", command: false, expected: false),
      // --- Right modifier, wrong letter --------------------------------
      TriggerCase(
        // Guards against a stale "p" target from the old Ctrl+Shift+P
        // chord: the macOS seam matches "o", not "p".
        desc: "Command+'p' (wrong letter) => false",
        chars: "p", command: true, expected: false),
      TriggerCase(
        desc: "Command+'a' (wrong letter, lowercase) => false",
        chars: "a", command: true, expected: false),
      // --- No first scalar ---------------------------------------------
      TriggerCase(
        desc: "empty string (no first scalar) => false",
        chars: "", command: true, expected: false),
    ]

    for c in cases {
      let got = PickerSeam.isPickerTrigger(
        charactersIgnoringModifiers: c.chars, command: c.command)
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
}
