// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import XCTest

@testable import WolfKeyMap

/// The tab focus rule: which tab is focused, and the loss and gain edges each change reports to
/// the tabs' Neovims.
final class TabFocusSeamTests: XCTestCase {

  private typealias Edge = TabFocusSeam.Edge

  /// F1: moving focus from one tab to another reports the old tab's loss, then the new tab's
  /// gain, so the new tab's Neovim runs `:checktime` and the old one is told it is unfocused.
  func testSwitchingTabsLosesTheOldAndGainsTheNew() {
    XCTAssertEqual(
      TabFocusSeam.edges(from: 1, to: 2),
      [Edge(tab: 1, gained: false), Edge(tab: 2, gained: true)],
      "F1: a switch from tab 1 to tab 2 must report tab 1's loss, then tab 2's gain. Only the"
        + " gain leaves tab 1's Neovim believing it is focused; the gain first reports the"
        + " edges out of order; no gain means the new tab never reloads a changed file")
  }

  /// The app becoming active with a main tab gains that tab alone, and the app resigning
  /// loses it alone.
  func testActivatingAndResigningReportOneEdge() {
    XCTAssertEqual(
      TabFocusSeam.edges(from: nil, to: 3), [Edge(tab: 3, gained: true)],
      "activation onto tab 3 must report only tab 3's gain; anything more invents a tab")
    XCTAssertEqual(
      TabFocusSeam.edges(from: 3, to: nil), [Edge(tab: 3, gained: false)],
      "resigning from tab 3 must report only tab 3's loss; anything more invents a tab")
  }

  /// F2: no change in the focused tab reports nothing, so a refresh that finds the same tab
  /// focused fires no `FocusGained`, and no file-tree rescan rides on it.
  func testAnUnchangedTabReportsNothing() {
    XCTAssertEqual(
      TabFocusSeam.edges(from: 2, to: 2), [],
      "F2: tab 2 staying focused must report no edge; a loss and a gain here make every"
        + " refresh, such as a picker closing, rerun :checktime and the file-tree rescan")
    XCTAssertEqual(
      TabFocusSeam.edges(from: nil, to: nil), [],
      "F2: no tab focused before or after must report no edge")
  }

  /// F3: while the app is inactive no tab is focused, whatever window AppKit still calls main.
  func testAnInactiveAppFocusesNoTab() {
    XCTAssertNil(
      TabFocusSeam.focusedTab(appActive: false, mainTab: 3),
      "F3: an inactive app focuses no tab; 3 means a tab's Neovim stays focused while the user"
        + " works in another app, so returning to it reports no gain and no reload")
    XCTAssertEqual(
      TabFocusSeam.focusedTab(appActive: true, mainTab: 3), 3,
      "F3: an active app focuses its main window's tab")
    XCTAssertNil(
      TabFocusSeam.focusedTab(appActive: true, mainTab: nil),
      "an active app with no main tab, such as only a panel open, focuses no tab")
  }
}
