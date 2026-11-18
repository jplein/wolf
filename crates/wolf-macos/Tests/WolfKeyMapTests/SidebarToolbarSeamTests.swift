// Checks the toolbar sidebar control's derived selection, its click rule and its segment mapping.

import XCTest

@testable import WolfKeyMap

final class SidebarToolbarSeamTests: XCTestCase {

  // MARK: - selectedSegment(): derived from visibility and mode

  func testSelectedSegmentFollowsVisibilityAndMode() {
    XCTAssertEqual(
      SidebarToolbarSeam.selectedSegment(sidebarVisible: true, current: .tree), 0,
      "visible on tree must select the tree segment 0: nil means the visible panel is not"
        + " derived from the mode, 1 means the segments are mapped in the wrong order")
    XCTAssertEqual(
      SidebarToolbarSeam.selectedSegment(sidebarVisible: true, current: .search), 1,
      "visible on search must select the search segment 1: nil means the visible panel is not"
        + " derived from the mode, 0 means the segments are mapped in the wrong order")
    XCTAssertNil(
      SidebarToolbarSeam.selectedSegment(sidebarVisible: true, current: nil),
      "visible with no known panel must select nothing, not invent a panel")

    for current in [SidebarPanel.tree, .search, nil] {
      XCTAssertNil(
        SidebarToolbarSeam.selectedSegment(sidebarVisible: false, current: current),
        "a hidden sidebar must select no segment whatever the mode (\(String(describing: current))):"
          + " a segment here means the selection ignores visibility and stays lit after a hide")
    }
  }

  func testSelectedPanelIsTheVisiblePanelOrNothing() {
    XCTAssertEqual(
      SidebarToolbarSeam.selectedPanel(sidebarVisible: true, current: .search), .search,
      "visible on search must report search as the selected panel")
    XCTAssertNil(
      SidebarToolbarSeam.selectedPanel(sidebarVisible: false, current: .search),
      "a hidden sidebar must report no selected panel, which the View menu's checkmarks share")
  }

  // MARK: - click(): the toolbar's click rule

  func testClickingTheActivePanelHidesTheSidebar() {
    XCTAssertEqual(
      SidebarToolbarSeam.click(.tree, sidebarVisible: true, current: .tree), .hide,
      "clicking tree while tree is showing must hide the sidebar; .show(.tree) means a click on"
        + " the active segment can never clear it")
    XCTAssertEqual(
      SidebarToolbarSeam.click(.search, sidebarVisible: true, current: .search), .hide,
      "clicking search while search is showing must hide the sidebar; .show(.search) means a"
        + " click on the active segment can never clear it")
  }

  func testClickingTheOtherPanelSwitchesToIt() {
    XCTAssertEqual(
      SidebarToolbarSeam.click(.search, sidebarVisible: true, current: .tree), .show(.search),
      "clicking search while tree is showing must switch to search; .hide means any click on a"
        + " visible sidebar hides it")
    XCTAssertEqual(
      SidebarToolbarSeam.click(.tree, sidebarVisible: true, current: .search), .show(.tree),
      "clicking tree while search is showing must switch to tree; .hide means any click on a"
        + " visible sidebar hides it")
  }

  func testClickingEitherWhileHiddenShowsThatPanel() {
    for current in [SidebarPanel.tree, .search, nil] {
      for clicked in [SidebarPanel.tree, .search] {
        XCTAssertEqual(
          SidebarToolbarSeam.click(clicked, sidebarVisible: false, current: current),
          .show(clicked),
          "clicking \(clicked) while the sidebar is hidden on \(String(describing: current))"
            + " must show \(clicked); .hide means the rule compares the mode but ignores that"
            + " the sidebar is already hidden")
      }
    }
  }

  // MARK: - Segments, symbols, tooltips and identifier

  func testSegmentsMapToPanelsInToolbarOrder() {
    XCTAssertEqual(
      SidebarToolbarSeam.panels, [.tree, .search],
      "the segments run tree then search, left to right")
    XCTAssertEqual(SidebarToolbarSeam.segment(of: .tree), 0, "tree is the first segment")
    XCTAssertEqual(SidebarToolbarSeam.segment(of: .search), 1, "search is the second segment")
    for panel in [SidebarPanel.tree, .search] {
      XCTAssertEqual(
        SidebarToolbarSeam.panel(atSegment: SidebarToolbarSeam.segment(of: panel)), panel,
        "segment(of:) and panel(atSegment:) must round-trip \(panel), or a click on its segment"
          + " acts on the other panel")
    }
    XCTAssertNil(
      SidebarToolbarSeam.panel(atSegment: -1),
      "segment -1, a control with nothing selected, must map to no panel; a panel here means"
        + " an out-of-range index is clamped and a cleared control acts as a click")
    XCTAssertNil(
      SidebarToolbarSeam.panel(atSegment: 2),
      "segment 2 does not exist and must map to no panel; a panel here means the index is"
        + " clamped into range")
  }

  func testSymbolsToolTipsAndIdentifier() {
    XCTAssertEqual(
      SidebarToolbarSeam.symbolName(for: .tree), "folder",
      "the tree segment uses the folder symbol the sidebar header used")
    XCTAssertEqual(
      SidebarToolbarSeam.symbolName(for: .search), "magnifyingglass",
      "the search segment uses the magnifier symbol the sidebar header used")
    XCTAssertEqual(
      SidebarToolbarSeam.label(for: .tree), "Project", "the tree label is the View menu's title")
    XCTAssertEqual(
      SidebarToolbarSeam.label(for: .search), "Search",
      "the search label is the View menu's title")
    XCTAssertEqual(
      SidebarToolbarSeam.toolTip(for: .tree), "Project (⇧⌘E)",
      "the tree tooltip names the panel and its shortcut")
    XCTAssertEqual(
      SidebarToolbarSeam.toolTip(for: .search), "Search (⇧⌘F)",
      "the search tooltip names the panel and its shortcut")
    XCTAssertNotEqual(
      SidebarToolbarSeam.identifier, HistorySeam.backIdentifier,
      "the control must be addressable apart from Back")
    XCTAssertNotEqual(
      SidebarToolbarSeam.identifier, HistorySeam.forwardIdentifier,
      "the control must be addressable apart from Forward")
  }
}
