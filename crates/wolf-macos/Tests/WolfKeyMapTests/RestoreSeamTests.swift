//
//  RestoreSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.RestoreSeam` namespace.
//
//  A NEW pure, AppKit-free seam (plan §4.2 + OD5) that groups the saved
//  (window, tab, cwd) tree into PROJECTS for restore ("group-by-cwd"): one
//  project per distinct real directory, MERGED across windows; each empty-state
//  (or since-deleted) tab is its OWN project. It mirrors the existing pure seams
//  (`StateSeam`, `TabTitleSeam`) and is exercised with scalar inputs only — NO
//  AppKit, NO NSWindow, NO live Neovim — so it stays in the headless
//  `WolfKeyMapTests` bundle.
//
//  CONTRACT UNDER TEST (the SIGNATURE + shape the coder implements to satisfy):
//    * RestoreSeam.plan(windows:home:isDirectory:) -> RestorePlan
//      - `windows: [RestoreSeam.SavedWindow]` — the saved OS windows in order;
//        each carries `tabs: [RestoreSeam.SavedTab]` (a per-tab `cwd: String` and
//        `sidebarVisible: Bool`), in tab order.
//      - `home: String` — the launch cwd for an empty-state project (mirrors
//        `StateSeam.launchDecision`'s `home`: nvim always needs a working dir).
//      - `isDirectory: (String) -> Bool` — injected purity boundary (tests pass a
//        set lookup). A REAL directory (non-empty cwd AND `isDirectory(cwd)`)
//        groups by cwd; anything else (empty "", or a since-deleted/plain-file
//        path) is an empty-state tab and gets a UNIQUE project of its own.
//      - Returns `RestorePlan { projects: [ProjectPlan] }`.
//      - `ProjectPlan { launchCwd: String, tabs: [PlannedTab] }` — `launchCwd` is
//        the real directory for a folder project, or `home` for an empty-state
//        project (so Phase A can `Project.start(launchCwd:)`).
//      - `PlannedTab { groupIndex: Int, positionInGroup: Int, cwd: String,
//        sidebarVisible: Bool }` — every tab retains its (window, position)
//        placement + its ORIGINAL saved cwd + sidebar flag, so Phase B can drop it
//        back into the right OS window/tab-group. Must be `Hashable` (+ `Equatable`)
//        so the (e) test can compare an order-independent `Set` of tabs;
//        `ProjectPlan` / `RestorePlan` are `Equatable`.
//
//  ASSUMPTIONS (stated in the test-writer report; the coder implements to match):
//    * Grouping is by the cwd STRING as-saved (the AppKit caller persists an
//      already-canonical cwd); the seam does NOT itself canonicalize (that would
//      need FileManager and break purity).
//    * A since-deleted / non-directory non-empty cwd is an empty-state tab (its
//      own project, `launchCwd == home`) but the `PlannedTab.cwd` still retains the
//      ORIGINAL saved string (grouping KEY differs from the retained field).
//    * These tests assert grouping + per-tab metadata ORDER-INDEPENDENTLY (they do
//      not pin the order of `projects` in the plan).
//

import Foundation
import XCTest

@testable import WolfKeyMap

final class RestoreSeamTests: XCTestCase {

  /// The empty-state launch cwd (home).
  private let home = "/Users/alice"

  /// Builds a plan with an injected directory set for `isDirectory`.
  private func makePlan(
    _ windows: [RestoreSeam.SavedWindow], directories: Set<String>
  ) -> RestoreSeam.RestorePlan {
    RestoreSeam.plan(
      windows: windows, home: home, isDirectory: { directories.contains($0) })
  }

  /// A `SavedWindow` from `(cwd, sidebarVisible)` tuples, in tab order. The
  /// per-tab pane `layout` (feature plan §3.3) is irrelevant to grouping, so it is
  /// `nil` here; the dedicated passthrough test below sets it explicitly.
  private func window(_ tabs: [(String, Bool)]) -> RestoreSeam.SavedWindow {
    RestoreSeam.SavedWindow(
      tabs: tabs.map { RestoreSeam.SavedTab(cwd: $0.0, sidebarVisible: $0.1, layout: nil) })
  }

  /// The index of the project containing the tab at `(groupIndex, position)`, or
  /// `nil` if none does. Used for order-independent "same/different project"
  /// assertions.
  private func projectIndex(
    _ plan: RestoreSeam.RestorePlan, groupIndex: Int, position: Int
  ) -> Int? {
    plan.projects.firstIndex(where: { proj in
      proj.tabs.contains(where: {
        $0.groupIndex == groupIndex && $0.positionInGroup == position
      })
    })
  }

  /// Every planned tab across all projects (flattened), for count / membership
  /// assertions that do not depend on project order.
  private func allTabs(_ plan: RestoreSeam.RestorePlan) -> [RestoreSeam.PlannedTab] {
    plan.projects.flatMap { $0.tabs }
  }

  // MARK: - (a) two same-cwd tabs in ONE window collapse into ONE project

  func testSameCwdTabsInOneWindowCollapseToOneProject() {
    let proj = "/Users/alice/projects/notes"
    let plan = makePlan(
      [window([(proj, true), (proj, false)])],
      directories: [proj, home])

    XCTAssertEqual(
      plan.projects.count, 1,
      "two tabs sharing a real cwd in one window must collapse into ONE project")
    XCTAssertEqual(
      plan.projects[0].launchCwd, proj,
      "a folder project's launchCwd is its real directory")
    XCTAssertEqual(
      plan.projects[0].tabs.count, 2,
      "the single project must own both same-cwd tabs")
    // Both tabs are in the same project (trivially, since there is one).
    XCTAssertEqual(
      projectIndex(plan, groupIndex: 0, position: 0),
      projectIndex(plan, groupIndex: 0, position: 1),
      "both same-cwd tabs must belong to the same project")
  }

  // MARK: - (b) same-dir tabs in DIFFERENT windows MERGE into one project

  func testSameDirTabsInDifferentWindowsMerge() {
    let proj = "/Users/alice/projects/notes"
    let plan = makePlan(
      [window([(proj, true)]), window([(proj, false)])],
      directories: [proj, home])

    XCTAssertEqual(
      plan.projects.count, 1,
      "same-dir tabs from DIFFERENT saved windows must MERGE into one project")
    XCTAssertEqual(
      projectIndex(plan, groupIndex: 0, position: 0),
      projectIndex(plan, groupIndex: 1, position: 0),
      "the tab in window 0 and the tab in window 1 (same dir) must share a project")
    // Both original placements are retained.
    let tabs = allTabs(plan).sorted { $0.groupIndex < $1.groupIndex }
    XCTAssertEqual(tabs.map { $0.groupIndex }, [0, 1], "both window placements retained")
  }

  // MARK: - (c) two different-dir tabs in ONE window SPLIT into two projects

  func testDifferentDirTabsInOneWindowSplit() {
    let dirX = "/Users/alice/projects/x"
    let dirY = "/Users/alice/projects/y"
    let plan = makePlan(
      [window([(dirX, true), (dirY, false)])],
      directories: [dirX, dirY, home])

    XCTAssertEqual(
      plan.projects.count, 2,
      "two different-dir tabs in one window must SPLIT into two projects")
    XCTAssertNotEqual(
      projectIndex(plan, groupIndex: 0, position: 0),
      projectIndex(plan, groupIndex: 0, position: 1),
      "the two different-dir tabs must land in DIFFERENT projects")
  }

  // MARK: - (d) OD5: each empty-state tab is its OWN project, never merged

  func testEmptyStateTabsAreEachTheirOwnProject() {
    // Three empty-state ("") tabs in ONE window => THREE distinct projects.
    let plan = makePlan(
      [window([("", true), ("", false), ("", true)])],
      directories: [home])

    XCTAssertEqual(
      plan.projects.count, 3,
      "N empty-state tabs must become N distinct projects (OD5: never merged)")
    for proj in plan.projects {
      XCTAssertEqual(
        proj.tabs.count, 1,
        "each empty-state project holds exactly one tab")
      XCTAssertEqual(
        proj.launchCwd, home,
        "an empty-state project launches nvim at home")
      XCTAssertEqual(
        proj.tabs[0].cwd, "",
        "an empty-state PlannedTab retains its empty cwd")
    }
    // The three empty tabs are pairwise in DIFFERENT projects.
    let p0 = projectIndex(plan, groupIndex: 0, position: 0)
    let p1 = projectIndex(plan, groupIndex: 0, position: 1)
    let p2 = projectIndex(plan, groupIndex: 0, position: 2)
    XCTAssertNotEqual(p0, p1, "empty-state tabs 0 and 1 must not merge")
    XCTAssertNotEqual(p1, p2, "empty-state tabs 1 and 2 must not merge")
    XCTAssertNotEqual(p0, p2, "empty-state tabs 0 and 2 must not merge")
  }

  // MARK: - (d′) empty-state never merges with a real-dir project; deleted dir is empty-state

  func testEmptyAndDeletedTabsNeverMergeWithRealProjects() {
    let dir = "/Users/alice/projects/live"
    let deleted = "/Users/alice/projects/gone"
    // One window: two tabs of the same LIVE dir + one empty + one DELETED dir
    // (non-empty cwd but NOT a directory). Expected: 1 folder project (2 tabs) +
    // 1 empty-state project + 1 deleted-dir (empty-state) project = 3 projects.
    let plan = makePlan(
      [window([(dir, true), (dir, false), ("", true), (deleted, false)])],
      directories: [dir, home])  // `deleted` is deliberately NOT a directory

    XCTAssertEqual(
      plan.projects.count, 3,
      "same-dir tabs collapse; the empty and the deleted-dir tabs each get their own project")

    let liveProject = projectIndex(plan, groupIndex: 0, position: 0)
    XCTAssertEqual(
      liveProject, projectIndex(plan, groupIndex: 0, position: 1),
      "the two same-live-dir tabs must share one project")
    XCTAssertNotEqual(
      liveProject, projectIndex(plan, groupIndex: 0, position: 2),
      "the empty-state tab must NOT merge into the real-dir project")
    XCTAssertNotEqual(
      liveProject, projectIndex(plan, groupIndex: 0, position: 3),
      "the deleted-dir tab must NOT merge into the real-dir project")
    XCTAssertNotEqual(
      projectIndex(plan, groupIndex: 0, position: 2),
      projectIndex(plan, groupIndex: 0, position: 3),
      "the empty-state and the deleted-dir tab must be DISTINCT projects")

    // The deleted-dir tab is treated as empty-state (launchCwd == home) but its
    // PlannedTab still retains the ORIGINAL saved cwd string.
    let deletedIdx = projectIndex(plan, groupIndex: 0, position: 3)!
    XCTAssertEqual(
      plan.projects[deletedIdx].launchCwd, home,
      "a since-deleted dir is an empty-state project (launchCwd == home)")
    XCTAssertEqual(
      plan.projects[deletedIdx].tabs[0].cwd, deleted,
      "the deleted-dir PlannedTab retains its ORIGINAL saved cwd string")
  }

  // MARK: - (e) every tab retains (groupIndex, positionInGroup, cwd, sidebarVisible)

  func testEveryTabRetainsItsPlacementMetadata() {
    let dirX = "/Users/alice/projects/x"
    let dirY = "/Users/alice/projects/y"
    // window0: two same-dir tabs (one project, positions 0/1);
    // window1: one different-dir tab (its own project).
    let plan = makePlan(
      [window([(dirX, true), (dirX, false)]), window([(dirY, true)])],
      directories: [dirX, dirY, home])

    XCTAssertEqual(plan.projects.count, 2, "one dirX project + one dirY project")

    // No tab is lost, and every input tab appears exactly once with its exact
    // placement metadata (asserted as an order-independent set).
    let produced = Set(allTabs(plan))
    let expected: Set<RestoreSeam.PlannedTab> = [
      RestoreSeam.PlannedTab(
        groupIndex: 0, positionInGroup: 0, cwd: dirX, sidebarVisible: true, layout: nil),
      RestoreSeam.PlannedTab(
        groupIndex: 0, positionInGroup: 1, cwd: dirX, sidebarVisible: false, layout: nil),
      RestoreSeam.PlannedTab(
        groupIndex: 1, positionInGroup: 0, cwd: dirY, sidebarVisible: true, layout: nil),
    ]
    XCTAssertEqual(allTabs(plan).count, 3, "no tab is dropped or duplicated")
    XCTAssertEqual(
      produced, expected,
      "every tab retains its (groupIndex, positionInGroup, cwd, sidebarVisible)")
  }

  // MARK: - (f) a saved tab's pane layout is carried through plan() unchanged

  func testLayoutIsCarriedThroughPlanUnchanged() throws {
    // Feature plan §3.3: `SavedTab.layout: SavedPaneNode?` is a PASSTHROUGH — it
    // rides into the matching `PlannedTab.layout` verbatim, and it does NOT affect
    // project grouping (still cwd-only). Two tabs sharing a real cwd but carrying
    // DIFFERENT layouts must still merge into ONE project, each keeping its own
    // layout.
    let proj = "/Users/alice/projects/notes"
    let splitLayout: SavedPaneNode = .split(
      orientation: .vertical, weights: [0.5, 0.5],
      children: [.leaf(path: "\(proj)/a.md"), .leaf(path: "\(proj)/b.md")])

    let windows = [
      RestoreSeam.SavedWindow(tabs: [
        RestoreSeam.SavedTab(cwd: proj, sidebarVisible: true, layout: splitLayout),
        RestoreSeam.SavedTab(cwd: proj, sidebarVisible: false, layout: nil),
      ])
    ]
    let plan = makePlan(windows, directories: [proj, home])

    // Grouping is UNAFFECTED by layout: both tabs still collapse into one project.
    XCTAssertEqual(
      plan.projects.count, 1,
      "differing layouts must not split a shared-cwd project (grouping stays cwd-only)")

    // Position 0 carries the split layout verbatim; position 1 carries nil.
    let tab0 = try XCTUnwrap(
      allTabs(plan).first { $0.positionInGroup == 0 }, "the position-0 tab must exist")
    let tab1 = try XCTUnwrap(
      allTabs(plan).first { $0.positionInGroup == 1 }, "the position-1 tab must exist")
    XCTAssertEqual(
      tab0.layout, splitLayout,
      "the saved split layout rides into the PlannedTab unchanged")
    XCTAssertNil(tab1.layout, "a nil saved layout stays nil in the PlannedTab")
  }
}
