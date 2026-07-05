//
//  PaletteSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.PaletteSeam` namespace.
//
//  The command-palette analog of `PickerSeamTests.swift`: it exercises the pure,
//  display-free seams of the macOS command palette with scalar inputs only — NO
//  NSEvent, NO window, NO AppKit, NO NSView. Two contracts are under test:
//
//    * PaletteSeam.isPaletteTrigger(charactersIgnoringModifiers:command:) -> Bool
//      — true iff `command && firstScalar.lowercased == "p"`. This is the macOS
//      Command+P chord, the palette sibling of PickerSeam's Command+O ("o" -> "p").
//      Command does not mangle `charactersIgnoringModifiers`, so with Command held
//      the field carries "p" (or "P" if Shift is also down); the predicate matches
//      case-insensitively. An empty string (no first scalar) is false.
//
//      NOTE (plan §10): this seam is DORMANT/UNWIRED — ⌘P is a menu key-equivalent
//      that preempts keyDown (like ⌘N), so the predicate is kept for consistency
//      with PickerSeam and tested, but NOT called from keyDown. The test pins the
//      contract regardless.
//
//    * PaletteSeam.PaletteRowKind(ffi:) — decodes the FFI row-kind discriminant
//      (`0` -> .staticCommand, `1` -> .switchWindow, `2` -> .openRecent, else nil),
//      mirroring `wolf-ffi`'s `mod palette_row_kind` (STATIC=0, SWITCH_WINDOW=1,
//      OPEN_RECENT=2). Structurally the sibling of `CommandKind(ffi:)`.
//
//  NOTE to coder: this references the pure `WolfKeyMap.PaletteSeam` enum (with a
//  nested `PaletteRowKind: UInt8` + guarded `init?(ffi:)`) the coder will add in
//  `Sources/WolfKeyMap/PaletteSeam.swift`. The signatures asserted below are the
//  contract the implementation must satisfy.
//

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
