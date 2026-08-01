//
//  SidebarMenuSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.SidebarMenuSeam` namespace.
//
//  These exercise the display-free seam that decides whether a View-menu
//  sidebar entry ("Show Project Tree" / "Show Search") shows a checkmark —
//  scalar inputs only, NO NSMenuItem, NO window, NO AppKit.
//
//  CONTRACT UNDER TEST (sidebar-menu plan, `SidebarMenuSeam.swift`):
//    * SidebarPanel: Equatable { case tree, search }
//    * SidebarMenuSeam.isChecked(sidebarVisible:current:item:) -> Bool
//      returns `sidebarVisible && current == item`, i.e. a panel's entry is
//      checked iff the sidebar is visible AND that panel is the active one.
//      Hidden sidebar (sidebarVisible == false) ⇒ never checked, regardless of
//      `current`. No active panel (current == nil) ⇒ never checked.
//
//  NOTE to coder: this test references the pure `WolfKeyMap.SidebarMenuSeam`
//  enum + `SidebarPanel` the coder adds in
//  `Sources/WolfKeyMap/SidebarMenuSeam.swift`. Until that source file is added
//  to the `wolf_key_map` `srcs` list, this test target will not compile.
//

import XCTest

@testable import WolfKeyMap

final class SidebarMenuSeamTests: XCTestCase {

  // MARK: - Bullet 1: the visible panel's own entry is checked

  func testVisiblePanelEntryIsChecked() {
    XCTAssertTrue(
      SidebarMenuSeam.isChecked(sidebarVisible: true, current: .tree, item: .tree),
      "tree visible => tree entry checked")
    XCTAssertTrue(
      SidebarMenuSeam.isChecked(sidebarVisible: true, current: .search, item: .search),
      "search visible => search entry checked")
  }

  // MARK: - Bullet 2: the non-active panel is NOT checked while visible

  func testNonActivePanelIsNotChecked() {
    XCTAssertFalse(
      SidebarMenuSeam.isChecked(sidebarVisible: true, current: .tree, item: .search),
      "tree visible => search entry NOT checked")
    XCTAssertFalse(
      SidebarMenuSeam.isChecked(sidebarVisible: true, current: .search, item: .tree),
      "search visible => tree entry NOT checked")
  }

  // MARK: - Bullet 3: hidden sidebar => NEITHER entry checked

  func testHiddenSidebarNeverChecked() {
    XCTAssertFalse(
      SidebarMenuSeam.isChecked(sidebarVisible: false, current: .tree, item: .tree),
      "hidden sidebar => tree entry NOT checked even if current == tree")
    XCTAssertFalse(
      SidebarMenuSeam.isChecked(sidebarVisible: false, current: .search, item: .search),
      "hidden sidebar => search entry NOT checked even if current == search")
    XCTAssertFalse(
      SidebarMenuSeam.isChecked(sidebarVisible: false, current: .tree, item: .search),
      "hidden sidebar => cross panel NOT checked")
    XCTAssertFalse(
      SidebarMenuSeam.isChecked(sidebarVisible: false, current: .search, item: .tree),
      "hidden sidebar => cross panel NOT checked")
    XCTAssertFalse(
      SidebarMenuSeam.isChecked(sidebarVisible: false, current: nil, item: .tree),
      "hidden sidebar + no active panel => tree entry NOT checked")
    XCTAssertFalse(
      SidebarMenuSeam.isChecked(sidebarVisible: false, current: nil, item: .search),
      "hidden sidebar + no active panel => search entry NOT checked")
  }

  // MARK: - Bullet 4 (defensive): no active panel while visible => nothing checked

  func testNoActivePanelWhileVisibleNeverChecked() {
    XCTAssertFalse(
      SidebarMenuSeam.isChecked(sidebarVisible: true, current: nil, item: .tree),
      "visible but no active panel => tree entry NOT checked")
    XCTAssertFalse(
      SidebarMenuSeam.isChecked(sidebarVisible: true, current: nil, item: .search),
      "visible but no active panel => search entry NOT checked")
  }
}
