// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import Foundation
import XCTest

@testable import WolfKeyMap

/// `StackingOrder` keeps `specs/application-state.md` §3's window stacking order and per-window
/// tab recency over tab ids. A window is a list of tab ids in tab-bar order.
final class StackingOrderTests: XCTestCase {

  // MARK: - SO1: a focused window moves to the front

  func testAFocusedWindowMovesToTheFront() {
    var stacking = StackingOrder()
    stacking.focus(1)
    stacking.focus(2)
    XCTAssertEqual(
      stacking.order([[1], [2]]), [1, 0],
      "tab 2 was focused last, so its window must be frontmost; [0, 1] means focus moves "
        + "nothing and the order falls back to the input order")
    stacking.focus(1)
    XCTAssertEqual(
      stacking.order([[1], [2]]), [0, 1],
      "focusing tab 1 again must bring its window back to the front")
  }

  // MARK: - SO2, SO3: a minimized window moves behind every other

  /// Windows `[1, 2]` and `[3]`, with tab 1 focused last and its window then minimized.
  private func minimizedFront() -> StackingOrder {
    var stacking = StackingOrder()
    stacking.focus(3)
    stacking.focus(1)
    stacking.minimize([1, 2])
    return stacking
  }

  func testAMinimizedWindowMovesBehindTheOthers() {
    let stacking = minimizedFront()
    XCTAssertTrue(
      stacking.isMinimized([1, 2]),
      "every tab of the minimized window was marked, so the window is minimized; false means "
        + "minimize marks nothing")
    XCTAssertFalse(stacking.isMinimized([3]), "the other window was never minimized")
    XCTAssertEqual(
      stacking.order([[1, 2], [3]]), [1, 0],
      "the minimized window must rank behind the one that is not, although its tab was "
        + "focused last; [0, 1] means order ranks by focus alone")
  }

  func testADeminimizedWindowTakesItsRankBack() {
    var stacking = minimizedFront()
    stacking.deminimize([1, 2])
    XCTAssertFalse(
      stacking.isMinimized([1, 2]),
      "deminimize must unmark the window's tabs; true means it changes nothing")
    XCTAssertEqual(
      stacking.order([[1, 2], [3]]), [0, 1],
      "deminimized, the window whose tab was focused last must be frontmost again")
  }

  // MARK: - SO4: the launch seed

  func testTheSeedPutsEachActiveTabFirstInSeedOrder() {
    let stacking = StackingOrder(seed: [
      StackingOrder.SeedWindow(tabs: [10, 11, 12], activeTab: 2, minimized: false),
      StackingOrder.SeedWindow(tabs: [20], activeTab: 0, minimized: false),
      StackingOrder.SeedWindow(tabs: [30], activeTab: 0, minimized: true),
    ])
    XCTAssertEqual(
      stacking.recency([10, 11, 12]), [2, 0, 1],
      "§3 seeds a window's recency with its active tab first, then the rest in tab order; "
        + "[0, 1, 2] means the seed lists the tabs in tab order alone")
    XCTAssertEqual(
      stacking.order([[20], [10, 11, 12]]), [1, 0],
      "the first seed window must rank first whatever the input order; [0, 1] means the seed "
        + "is walked in reverse")
    XCTAssertTrue(
      stacking.isMinimized([30]),
      "a seed window marked minimized must start minimized; false means the seed ignores it")
    XCTAssertFalse(
      stacking.isMinimized([10, 11, 12]), "a seed window not marked minimized must not be")
  }

  // MARK: - SO5: per-window tab recency

  func testRecencyListsTheMostRecentlyFocusedTabFirst() {
    var stacking = StackingOrder()
    stacking.focus(3)
    stacking.focus(1)
    XCTAssertEqual(
      stacking.recency([1, 2, 3]), [0, 2, 1],
      "tab 1 was focused last and tab 3 before it, and the never-focused tab 2 follows both; "
        + "[0, 1, 2] means recency is the tab order")
  }

  // MARK: - SO6: a closed tab leaves the order

  func testARemovedTabLeavesTheOrder() {
    var stacking = StackingOrder()
    stacking.focus(1)
    stacking.focus(2)
    stacking.remove(1)
    XCTAssertEqual(
      stacking.tabs, [2],
      "a removed tab must leave the order; [2, 1] means remove changes nothing, so a closed "
        + "tab keeps ranking a window")
  }
}
