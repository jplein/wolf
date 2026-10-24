// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#shortcutseamtestsswift

import XCTest

@testable import WolfKeyMap

final class ShortcutSeamTests: XCTestCase {

  /// One `isNewWindowTrigger` table row: (description,
  /// charactersIgnoringModifiers, command, expected).
  private struct TriggerCase {
    let desc: String
    let chars: String
    let command: Bool
    let expected: Bool
  }

  func testIsNewWindowTriggerTable() {
    let cases: [TriggerCase] = [
      // --- The trigger: Command held, letter n -------------------------
      TriggerCase(
        desc: "Command+'n' (lowercase) => true",
        chars: "n", command: true, expected: true),
      TriggerCase(
        // With Command+Shift held, charactersIgnoringModifiers carries
        // the uppercase `N`; the predicate must match case-insensitively.
        desc: "Command+'N' (uppercase, i.e. Command+Shift+N) => true",
        chars: "N", command: true, expected: true),
      // --- Missing the required modifier -------------------------------
      TriggerCase(
        desc: "plain 'n' (command not held) => false",
        chars: "n", command: false, expected: false),
      // --- Right modifier, wrong letter --------------------------------
      TriggerCase(
        // Guards against colliding with the Command+O picker chord: the
        // new-window seam matches "n", not "o".
        desc: "Command+'o' (wrong letter) => false",
        chars: "o", command: true, expected: false),
      // --- No first scalar ---------------------------------------------
      TriggerCase(
        desc: "empty string (no first scalar) => false",
        chars: "", command: true, expected: false),
    ]

    for c in cases {
      let got = ShortcutSeam.isNewWindowTrigger(
        charactersIgnoringModifiers: c.chars, command: c.command)
      XCTAssertEqual(got, c.expected, "case: \(c.desc)")
    }
  }
}
