// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#paletteseamtestsswift

import XCTest

@testable import WolfKeyMap

final class PaletteSeamTests: XCTestCase {

  // MARK: - isPaletteTrigger

  /// One `isPaletteTrigger` table row: (description, charactersIgnoringModifiers,
  /// command, expected).
  private struct TriggerCase {
    let desc: String
    let chars: String
    let command: Bool
    let expected: Bool
  }

  func testIsPaletteTriggerTable() {
    let cases: [TriggerCase] = [
      // --- The trigger: Command held, letter p --------------------------
      TriggerCase(
        desc: "Command+'p' (lowercase) => true",
        chars: "p", command: true, expected: true),
      TriggerCase(
        // With Command+Shift held, charactersIgnoringModifiers carries
        // the uppercase `P`; the predicate must match case-insensitively.
        desc: "Command+'P' (uppercase, i.e. Command+Shift+P) => true",
        chars: "P", command: true, expected: true),
      // --- Missing the required modifier --------------------------------
      TriggerCase(
        desc: "plain 'p' (command not held) => false",
        chars: "p", command: false, expected: false),
      // --- Right modifier, wrong letter ---------------------------------
      TriggerCase(
        // Guards against confusing the palette chord with the picker's
        // Command+O: the palette seam matches "p", not "o".
        desc: "Command+'o' (wrong letter, the picker chord) => false",
        chars: "o", command: true, expected: false),
      // --- No first scalar ----------------------------------------------
      TriggerCase(
        desc: "empty string (no first scalar) => false",
        chars: "", command: true, expected: false),
    ]

    for c in cases {
      let got = PaletteSeam.isPaletteTrigger(
        charactersIgnoringModifiers: c.chars, command: c.command)
      XCTAssertEqual(got, c.expected, "case: \(c.desc)")
    }
  }

  // MARK: - PaletteRowKind(ffi:)

  func testPaletteRowKindFfiDiscriminants() {
    XCTAssertEqual(
      PaletteSeam.PaletteRowKind(ffi: 0), .staticCommand,
      "ffi 0 -> .staticCommand")
    XCTAssertEqual(
      PaletteSeam.PaletteRowKind(ffi: 1), .switchWindow,
      "ffi 1 -> .switchWindow")
    XCTAssertEqual(
      PaletteSeam.PaletteRowKind(ffi: 2), .openRecent,
      "ffi 2 -> .openRecent")
  }

  func testPaletteRowKindOutOfRangeIsNil() {
    XCTAssertNil(
      PaletteSeam.PaletteRowKind(ffi: 3),
      "the first out-of-range discriminant -> nil")
    XCTAssertNil(
      PaletteSeam.PaletteRowKind(ffi: 255),
      "a far out-of-range discriminant -> nil")
  }
}
