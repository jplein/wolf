// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import XCTest

@testable import WolfKeyMap

/// `CloseTabSeam.shouldClose(confirmed:liveness:ask:)`, the decision `windowShouldClose(_:)`
/// makes, and `CloseTabAnswer`'s raw values, which mirror wolf-ffi's `mod close_tab_answer`.
final class CloseTabSeamTests: XCTestCase {

  /// One row per case: the inputs, then the answer the seam must give and how many times it
  /// must ask the tab's Neovim.
  private struct Row {
    let name: String
    let confirmed: Bool
    let liveness: NvimLiveness
    let answer: CloseTabAnswer
    let closes: Bool
    let asks: Int
  }

  func testShouldCloseTable() {
    let rows: [Row] = [
      // S1: Neovim refused, and has echoed E37 or E162 in the tab.
      Row(
        name: "a modified tab", confirmed: false, liveness: .alive, answer: .refused,
        closes: false, asks: 1),
      // S2: Neovim notified close_tab, whose drain closes the window confirmed.
      Row(
        name: "an unmodified tab", confirmed: false, liveness: .alive, answer: .closing,
        closes: false, asks: 1),
      // S3: the ask failed or timed out, so a wedged tab stays closable.
      Row(
        name: "a tab whose Neovim gives no answer", confirmed: false, liveness: .alive,
        answer: .noAnswer, closes: true, asks: 1),
      // S4: a crashed or restarting Neovim is never asked.
      Row(
        name: "a dead Neovim", confirmed: false, liveness: .dead, answer: .refused,
        closes: true, asks: 0),
      // S5: a stalled Neovim is not asked, which would stall the UI for another timeout.
      Row(
        name: "a stalled Neovim", confirmed: false, liveness: .notResponding, answer: .refused,
        closes: true, asks: 0),
      // S6: a drained close_tab, or termination the user has confirmed.
      Row(
        name: "a confirmed close", confirmed: true, liveness: .alive, answer: .refused,
        closes: true, asks: 0),
    ]
    for row in rows {
      var asks = 0
      let closes = CloseTabSeam.shouldClose(
        confirmed: row.confirmed, liveness: row.liveness,
        ask: {
          asks += 1
          return row.answer
        })
      let inputs: String =
        "\(row.name) (confirmed \(row.confirmed), \(row.liveness), answer \(row.answer))"
      let outcomes: String =
        "true for .refused closes over unsaved edits, true for .closing closes before Neovim's"
        + " close_tab drains, and false for .noAnswer, a dead or stalled Neovim or a confirmed"
        + " close leaves the tab unclosable"
      XCTAssertEqual(
        closes, row.closes, "\(inputs) must answer \(row.closes): \(outcomes)")
      XCTAssertEqual(
        asks, row.asks,
        "\(row.name) must ask Neovim \(row.asks) time(s): an ask of a dead or stalled Neovim, or"
          + " of a confirmed close, means a guard is missing; more than one means it asks twice")
    }
  }

  func testAnswerRawValuesMatchTheBridge() {
    let rows: [(raw: UInt8, answer: CloseTabAnswer?)] = [
      (0, .noAnswer), (1, .refused), (2, .closing), (3, nil),
    ]
    for row in rows {
      XCTAssertEqual(
        CloseTabAnswer(rawValue: row.raw), row.answer,
        "raw \(row.raw) must decode to \(String(describing: row.answer)), as wolf-ffi's"
          + " close_tab_answer numbers it: another case means a raw value moved or two traded,"
          + " so a refusal could read as closing; a value for 3 means a case the bridge never"
          + " sends")
    }
  }
}
