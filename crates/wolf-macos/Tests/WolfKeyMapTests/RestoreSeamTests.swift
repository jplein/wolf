// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#restoreseamtestsswift

import Foundation
import XCTest

@testable import WolfKeyMap

final class RestoreSeamTests: XCTestCase {

  /// The empty-state launch cwd (home).
  private let home = "/Users/alice"

  /// Builds a plan with an injected directory set for `isDirectory`.
  private func makePlan(
    _ windows: [RestoreSeam.SavedWindow], directories: Set<String>
  ) -> [RestoreSeam.PlannedTab] {
    RestoreSeam.plan(
      windows: windows, home: home, isDirectory: { directories.contains($0) })
  }

  /// A `SavedWindow` from `(cwd, sidebarVisible)` tuples, in tab order. The
  /// per-tab pane `layout` is irrelevant to planning, so it is `nil` here; the
  /// dedicated passthrough test below sets it explicitly.
  private func window(_ tabs: [(String, Bool)]) -> RestoreSeam.SavedWindow {
    RestoreSeam.SavedWindow(
      tabs: tabs.map { RestoreSeam.SavedTab(cwd: $0.0, sidebarVisible: $0.1, layout: nil) })
  }

  /// Each planned tab's `[groupIndex, positionInGroup]`, in plan order.
  private func placements(_ plan: [RestoreSeam.PlannedTab]) -> [[Int]] {
    plan.map { [$0.groupIndex, $0.positionInGroup] }
  }

  // MARK: - R1: tabs on one directory each get their own planned tab

  func testTabsOnOneDirectoryEachGetTheirOwnPlannedTab() {
    let proj = "/Users/alice/projects/notes"
    let plan = makePlan(
      [window([(proj, true), (proj, false)]), window([(proj, true)])],
      directories: [proj, home])

    XCTAssertEqual(
      plan.count, 3,
      "R1: three saved tabs on one directory must plan three tabs, each starting its own"
        + " Neovim; fewer means tabs on one directory were merged onto one, got \(plan)")
    XCTAssertEqual(
      placements(plan), [[0, 0], [0, 1], [1, 0]],
      "R1: the planned tabs must keep their saved (window, position) in window then tab order;"
        + " a missing pair names the tab that was dropped")
    for tab in plan {
      XCTAssertEqual(
        tab.launchCwd, proj,
        "R1: a tab on an existing directory launches its Neovim there, got \(tab)")
      XCTAssertTrue(tab.hasFolder, "R1: a tab on an existing directory has a folder, got \(tab)")
    }
  }

  // MARK: - R2: an empty or since-deleted directory launches at home

  func testEmptyAndMissingDirectoriesLaunchAtHome() {
    let live = "/Users/alice/projects/live"
    let deleted = "/Users/alice/projects/gone"
    let plan = makePlan(
      [window([("", true), (deleted, false), (live, true)])],
      directories: [live, home])  // `deleted` is deliberately NOT a directory

    XCTAssertEqual(plan.count, 3, "every saved tab is planned, got \(plan)")
    guard plan.count == 3 else { return }
    XCTAssertEqual(
      plan[0].launchCwd, home,
      "R2: an empty-state tab launches at home; \"\" means its saved cwd was used as is")
    XCTAssertFalse(plan[0].hasFolder, "R2: an empty-state tab has no folder")
    XCTAssertEqual(plan[0].cwd, "", "R2: an empty-state tab keeps its empty saved cwd")
    XCTAssertEqual(
      plan[1].launchCwd, home,
      "R2: a tab whose directory is gone launches at home; \(deleted) means the planner did"
        + " not check that the directory still exists")
    XCTAssertFalse(plan[1].hasFolder, "R2: a tab whose directory is gone has no folder")
    XCTAssertEqual(
      plan[1].cwd, deleted, "R2: a tab whose directory is gone keeps its saved cwd string")
    XCTAssertEqual(
      plan[2].launchCwd, live,
      "the tab on an existing directory beside them still launches there; home means every"
        + " tab is treated as empty-state")
    XCTAssertTrue(plan[2].hasFolder, "the tab on an existing directory has a folder")
  }

  // MARK: - every tab retains (groupIndex, positionInGroup, cwd, sidebarVisible)

  func testEveryTabRetainsItsPlacementMetadata() {
    let dirX = "/Users/alice/projects/x"
    let dirY = "/Users/alice/projects/y"
    let plan = makePlan(
      [window([(dirX, true), (dirX, false)]), window([(dirY, true)])],
      directories: [dirX, dirY, home])

    XCTAssertEqual(plan.count, 3, "no tab is dropped or duplicated, got \(plan)")
    XCTAssertEqual(placements(plan), [[0, 0], [0, 1], [1, 0]], "every tab keeps its placement")
    XCTAssertEqual(
      plan.map { $0.cwd }, [dirX, dirX, dirY], "every tab keeps its saved cwd, in order")
    XCTAssertEqual(
      plan.map { $0.sidebarVisible }, [true, false, true],
      "every tab keeps its sidebar visibility, in order")
  }

  // MARK: - a saved tab's pane layout is carried through plan() unchanged

  func testLayoutIsCarriedThroughPlanUnchanged() throws {
    let proj = "/Users/alice/projects/notes"
    let splitLayout: SavedPaneNode = .split(
      orientation: .vertical, weights: [0.5, 0.5],
      children: [
        .leaf(path: "\(proj)/a.md", preview: false),
        .leaf(path: "\(proj)/b.md", preview: false),
      ])

    let windows = [
      RestoreSeam.SavedWindow(tabs: [
        RestoreSeam.SavedTab(cwd: proj, sidebarVisible: true, layout: splitLayout),
        RestoreSeam.SavedTab(cwd: proj, sidebarVisible: false, layout: nil),
      ])
    ]
    let plan = makePlan(windows, directories: [proj, home])

    XCTAssertEqual(plan.count, 2, "both tabs are planned, got \(plan)")
    let tab0 = try XCTUnwrap(
      plan.first { $0.positionInGroup == 0 }, "the position-0 tab must exist")
    let tab1 = try XCTUnwrap(
      plan.first { $0.positionInGroup == 1 }, "the position-1 tab must exist")
    XCTAssertEqual(
      tab0.layout, splitLayout,
      "the saved split layout rides into the PlannedTab unchanged")
    XCTAssertNil(tab1.layout, "a nil saved layout stays nil in the PlannedTab")
  }

  // MARK: - the selected tab of a restored window

  func testTheSavedActiveTabIsSelectedWhenItWasRestored() {
    XCTAssertEqual(
      RestoreSeam.selectedPosition(active: 2, materialized: [0, 1, 2]), 2,
      "the saved active tab was restored, so it must be selected; 0 means the selection "
        + "falls back to the first tab although the saved one exists")
    XCTAssertEqual(
      RestoreSeam.selectedPosition(active: 1, materialized: [0, 2]), 0,
      "the saved active tab 1 was not restored, so the first restored tab must be selected; "
        + "1 means a position with no tab is selected")
    XCTAssertNil(
      RestoreSeam.selectedPosition(active: 0, materialized: []),
      "a window with no restored tab has nothing to select")
  }
}
