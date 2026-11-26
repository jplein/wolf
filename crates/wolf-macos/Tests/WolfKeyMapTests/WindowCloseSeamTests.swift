// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import Foundation
import XCTest

@testable import WolfKeyMap

/// `WindowCloseSeam`, the decisions Close Window and Quit make over each tab's answer,
/// `UnsavedAnswer`'s raw values, which mirror wolf-ffi's `mod unsaved_answer`, and
/// `CloseScopeSeam`, which tells the red close button of a tab group from a tab's own close.
final class WindowCloseSeamTests: XCTestCase {

  /// One row per case: the inputs, then the tab's answer and how many times the tab's Neovim
  /// must be asked.
  private struct UnsavedRow {
    let name: String
    let liveness: NvimLiveness
    let answer: UnsavedAnswer
    let want: TabUnsaved
    let asks: Int
  }

  /// CW11.
  func testTabUnsavedTable() {
    let rows: [UnsavedRow] = [
      UnsavedRow(
        name: "a dead or restarting Neovim, which is never asked", liveness: .dead,
        answer: .unsaved, want: .unreachable, asks: 0),
      UnsavedRow(
        name: "a stalled Neovim, whose ask would stall the UI for another timeout",
        liveness: .notResponding, answer: .unsaved, want: .unreachable, asks: 0),
      UnsavedRow(
        name: "a live Neovim that gives no answer within the RPC timeout", liveness: .alive,
        answer: .noAnswer, want: .unreachable, asks: 1),
      UnsavedRow(
        name: "a live Neovim holding a modified buffer", liveness: .alive, answer: .unsaved,
        want: .unsaved, asks: 1),
      UnsavedRow(
        name: "a live Neovim with nothing modified", liveness: .alive, answer: .clean,
        want: .clean, asks: 1),
    ]
    for row in rows {
      var asks = 0
      let got = WindowCloseSeam.tabUnsaved(
        liveness: row.liveness,
        ask: {
          asks += 1
          return row.answer
        })
      XCTAssertEqual(
        got, row.want,
        "\(row.name) (\(row.liveness), answer \(row.answer)) must give \(row.want): .unsaved for"
          + " .noAnswer or a dead or stalled Neovim keeps a wedged tab's window and the app open;"
          + " .clean or .unreachable for .unsaved closes over unsaved edits")
      XCTAssertEqual(
        asks, row.asks,
        "\(row.name) must ask Neovim \(row.asks) time(s): an ask of a dead or stalled Neovim"
          + " means a guard is missing, and more than one that it asks twice")
    }
  }

  /// A window's answers in tab order, the decision, and the indices that must be pulled.
  private typealias WindowRow = (
    name: String, tabs: [TabUnsaved], want: WindowCloseSeam.WindowClose, pulled: [Int]
  )

  /// CW12.
  func testCloseWindowTable() {
    let rows: [WindowRow] = [
      ("a window with no tabs", [], .closeAll, []),
      ("two clean tabs", [.clean, .clean], .closeAll, [0, 1]),
      ("an unreachable tab beside a clean one", [.unreachable, .clean], .closeAll, [0, 1]),
      ("one unsaved tab, in the background", [.clean, .unsaved], .show(tab: 1), [0, 1]),
      (
        "the first of two unsaved tabs", [.clean, .unsaved, .unsaved], .show(tab: 1),
        [0, 1]
      ),
    ]
    for row in rows {
      var pulled: [Int] = []
      let lazy = row.tabs.enumerated().lazy.map { (index, tab) -> TabUnsaved in
        pulled.append(index)
        return tab
      }
      let got = WindowCloseSeam.closeWindow(lazy)
      XCTAssertEqual(
        got, row.want,
        "\(row.name) \(row.tabs) must give \(row.want): .closeAll with an .unsaved tab closes"
          + " over its edits; .show for an .unreachable tab keeps a wedged tab's window open;"
          + " .show of a later index shows the last unsaved tab rather than the first in tab"
          + " order")
      XCTAssertEqual(
        pulled, row.pulled,
        "\(row.name) must ask the tabs \(row.pulled) in tab order and stop at the first"
          + " .unsaved: a later index means the answers were collected before deciding, so a"
          + " wedged tab after the shown one costs another RPC timeout")
    }
  }

  /// Each window's answers, frontmost first, the decision, and the windows that must be pulled.
  private typealias QuitRow = (
    name: String, windows: [[TabUnsaved]], want: WindowCloseSeam.Quit, pulled: [Int]
  )

  /// CW13.
  func testQuitTable() {
    let rows: [QuitRow] = [
      ("no windows", [], .quit, []),
      ("every tab clean", [[.clean], [.clean, .clean]], .quit, [0, 1]),
      ("an unreachable tab beside a clean one", [[.unreachable], [.clean]], .quit, [0, 1]),
      (
        "one unsaved tab, in the background of the second window",
        [[.clean], [.clean, .unsaved]], .show(window: 1, tab: 1), [0, 1]
      ),
      (
        "the first window's second tab and the second window's only tab unsaved",
        [[.clean, .unsaved], [.unsaved]], .show(window: 0, tab: 1), [0]
      ),
      (
        "an unsaved first tab of the third window",
        [[.clean], [.clean, .clean], [.unsaved, .clean]], .show(window: 2, tab: 0), [0, 1, 2]
      ),
    ]
    for row in rows {
      var pulled: [Int] = []
      let lazy = row.windows.enumerated().lazy.map { (index, tabs) -> [TabUnsaved] in
        pulled.append(index)
        return tabs
      }
      let got = WindowCloseSeam.quit(lazy)
      XCTAssertEqual(
        got, row.want,
        "\(row.name) \(row.windows) must give \(row.want): .quit with an .unsaved tab quits over"
          + " its edits; .show for an .unreachable tab keeps the app open for a wedged tab; a"
          + " .show of another window means the windows are walked out of the order given, and"
          + " window and tab traded means the fields are swapped")
      XCTAssertEqual(
        pulled, row.pulled,
        "\(row.name) must ask the windows \(row.pulled) in the order given and stop at the"
          + " first with an .unsaved tab: a later window means it asked every window first")
    }
  }

  /// CW14.
  func testUnsavedAnswerRawValuesMatchTheBridge() {
    let rows: [(raw: UInt8, answer: UnsavedAnswer?)] = [
      (0, .noAnswer), (1, .unsaved), (2, .clean), (3, nil),
    ]
    for row in rows {
      XCTAssertEqual(
        UnsavedAnswer(rawValue: row.raw), row.answer,
        "raw \(row.raw) must decode to \(String(describing: row.answer)), as wolf-ffi's"
          + " unsaved_answer numbers it: another case means a raw value moved or two traded, so"
          + " an unsaved tab could read as clean; a value for 3 means a case the bridge never"
          + " sends")
    }
  }

  // MARK: - CloseScopeSeam

  /// A `CloseScopeSeam` over window names whose scheduled ends run only when the test says.
  private final class Scope {
    /// Every batch end `schedule` was given, run or not.
    private(set) var ends: [() -> Void] = []
    /// Every decision made, in order.
    private(set) var decisions: [CloseScopeSeam<String>.Decision] = []
    private(set) var seam: CloseScopeSeam<String>!

    init() {
      seam = CloseScopeSeam<String>(
        schedule: { [unowned self] end in self.ends.append(end) },
        decide: { [unowned self] decision in self.decisions.append(decision) })
    }

    /// Runs the latest scheduled end, as the settle elapsing would.
    func settle() {
      ends.last?()
    }
  }

  /// CW15.
  func testOneWindowOfAGroupClosesItsTab() {
    let scope = Scope()
    let group = ["a", "b"]

    scope.seam.request("a", group: group)
    scope.seam.request("a", group: group)
    XCTAssertEqual(
      scope.decisions, [],
      "nothing may be decided before the settle ends: a decision here means a tab's close"
        + " button is answered before the red button's other windows have had a chance to ask")
    XCTAssertEqual(
      scope.ends.count, 1,
      "the batch must schedule its end once: 0 leaves it undecided forever, 2 means a repeated"
        + " request opened a second batch")

    scope.settle()
    XCTAssertEqual(
      scope.decisions, [.tab("a")],
      "one window of a two-window group asking, twice, is that tab's own close and must decide"
        + " [.tab(a)] once: .window closes the whole group for one tab's close button, and"
        + " [.tab(a), .tab(a)] means a repeated request counted twice")
  }

  /// CW16.
  func testEveryWindowOfAGroupClosesTheWindow() {
    for order in [["a", "b"], ["b", "a"]] {
      let scope = Scope()
      for window in order {
        scope.seam.request(window, group: ["a", "b"])
      }
      XCTAssertEqual(
        scope.decisions, [], "nothing may be decided before the settle ends (order \(order))")
      scope.settle()
      XCTAssertEqual(
        scope.decisions, [.window(["a", "b"])],
        "every window of the group asking, in the order \(order), is the red close button and"
          + " must decide .window once, in the group's order: [.tab(_), .tab(_)] closes each"
          + " tab on its own check, and a window list in request order means it lists the"
          + " requests rather than the group")
    }

    let scope = Scope()
    scope.seam.request("c", group: ["c", "d"])
    scope.seam.request("a", group: ["a", "b"])
    scope.seam.request("b", group: ["a", "b"])
    scope.settle()
    XCTAssertEqual(
      scope.decisions, [.tab("c"), .window(["a", "b"])],
      "one batch over two groups must decide each group apart, in first-request order: group"
        + " [c, d] had one window ask and [a, b] both, so a .window([c, d]) means one group's"
        + " requests counted toward the other")
  }

  /// CW17.
  func testARequestAfterTheSettleStartsANewBatch() {
    let scope = Scope()
    let group = ["a", "b"]

    scope.seam.request("a", group: group)
    scope.settle()
    XCTAssertEqual(scope.decisions, [.tab("a")], "the first batch must decide [.tab(a)]")

    scope.seam.request("b", group: group)
    XCTAssertEqual(
      scope.ends.count, 2,
      "a request after the settle must schedule a new end: 1 means it joined a batch that has"
        + " already ended, so it is never decided")
    scope.settle()
    XCTAssertEqual(
      scope.decisions, [.tab("a"), .tab("b")],
      "the second batch must decide [.tab(b)] alone: .window([a, b]) means the ended batch was"
        + " not emptied, so a's earlier request counted again")
  }
}
