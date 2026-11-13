// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import XCTest

@testable import WolfKeyMap

final class StatusbarTextTests: XCTestCase {

  /// One `StatusbarText(fg:bg:active:)` row: packed `0x00RRGGBB` editor defaults in, the
  /// blended packed colours and the weight out.
  private struct Row {
    let desc: String
    let fg: UInt32
    let bg: UInt32
    let active: Bool
    let primary: UInt32
    let secondary: UInt32
    let primaryBold: Bool
  }

  /// The three fields of a `StatusbarText`, compared as one value.
  private struct Treatment: Equatable, CustomStringConvertible {
    let primary: UInt32
    let secondary: UInt32
    let primaryBold: Bool

    var description: String {
      "primary \(StatusbarTextTests.hex(primary)), secondary"
        + " \(StatusbarTextTests.hex(secondary)), bold \(primaryBold)"
    }
  }

  /// The rows of `statusbar_text_table` in `crates/wolf-gnome/src/statusbar_layout.rs`, verbatim
  /// and in its order, so the two front-ends give one theme the same strip text. Change the two
  /// tables together. Each channel is `round(bg + (fg - bg) * alpha)`, half away from zero, with
  /// a primary alpha of 1 in an active pane and 0.5 in an inactive one, and a secondary alpha of
  /// a further 0.7 of the primary's.
  func testMatchesWolfGnomeStatusbarTextTable() {
    let rows = [
      // Primary α 1 -> 255. Secondary α 0.7: 255 * 0.7 = 178.5, a tie that rounds away to 179.
      Row(
        desc: "an active pane is full strength and bold", fg: 0x00FF_FFFF, bg: 0x0000_0000,
        active: true, primary: 0x00FF_FFFF, secondary: 0x00B3_B3B3, primaryBold: true),
      // Primary α 0.5: 255 * 0.5 = 127.5 -> 128. Secondary α 0.35: 255 * 0.35 = 89.25 -> 89.
      Row(
        desc: "an inactive pane dims both lines and is not bold", fg: 0x00FF_FFFF,
        bg: 0x0000_0000, active: false, primary: 0x0080_8080, secondary: 0x0059_5959,
        primaryBold: false),
      // Over grey α 0.5 gives (120, 70, 20) and α 0.35 (96, 61, 26); over black blue would be 0.
      Row(
        desc: "the blend is toward the background, not toward black", fg: 0x00C8_6400,
        bg: 0x0028_2828, active: false, primary: 0x0078_4614, secondary: 0x0060_3D1A,
        primaryBold: false),
      // Primary α 1 is fg exactly. Secondary α 0.7: 40 + (fg - 40) * 0.7 = (152, 82, 12).
      Row(
        desc: "an active pane over a non-black background is untouched", fg: 0x00C8_6400,
        bg: 0x0028_2828, active: true, primary: 0x00C8_6400, secondary: 0x0098_520C,
        primaryBold: true),
    ]
    for row in rows {
      let text = StatusbarText(fg: row.fg, bg: row.bg, active: row.active)
      let got = Treatment(
        primary: text.primary, secondary: text.secondary, primaryBold: text.primaryBold)
      let want = Treatment(
        primary: row.primary, secondary: row.secondary, primaryBold: row.primaryBold)
      let call =
        "StatusbarText(fg: \(Self.hex(row.fg)), bg: \(Self.hex(row.bg)), active: \(row.active))"
      XCTAssertEqual(
        got, want, "\(row.desc): \(call) must give \(want), but gives \(got). \(Self.drift)")
    }
  }

  /// What a mismatch in the table means.
  private static let drift =
    "A changed alpha moves the dimmed lines; a channel at 0 over the grey background means a"
    + " composite over black; 178 for 178.5 means ties round to even; a wrong bold means the"
    + " weight no longer follows `active`. Each is a drift from wolf-gnome's statusbar_text."

  /// `#rrggbb` for a packed colour.
  private static func hex(_ packed: UInt32) -> String {
    String(format: "#%06x", packed)
  }
}
