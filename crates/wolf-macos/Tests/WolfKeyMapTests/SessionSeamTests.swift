// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import Foundation
import XCTest

@testable import WolfKeyMap

final class SessionSeamTests: XCTestCase {

  private typealias Content = [PaneId: SessionSeam.LeafContent]

  private let p1: PaneId = 1
  private let p2: PaneId = 2
  private let p3: PaneId = 3

  /// A folder view's reported working directory.
  private let folder = "/Users/alice/projects/notes"

  /// What Neovim reports as the cwd of a view with no folder: its launch directory, home.
  /// It must never be persisted as the tab's folder.
  private let home = "/Users/alice"

  /// Three distinct files, none named after anything the assertions look for but its own leaf.
  private let fileA = "/Users/alice/projects/notes/alpha.md"
  private let fileB = "/Users/alice/projects/notes/beta.md"
  private let fileC = "/Users/alice/projects/notes/gamma.md"

  /// The tab the restore seeded a view with: the saved state that must survive a refusal.
  private var seed: RestoreSeam.SavedTab {
    RestoreSeam.SavedTab(
      cwd: folder, sidebarVisible: false,
      layout: .split(
        orientation: .horizontal, weights: [0.7, 0.3],
        children: [
          .leaf(path: fileA, preview: false), .leaf(path: fileB, preview: false),
        ]))
  }

  /// A live tab that differs from `seed` in every field.
  private var live: RestoreSeam.SavedTab {
    RestoreSeam.SavedTab(
      cwd: "/Users/alice/projects/other", sidebarVisible: true,
      layout: .leaf(path: fileC, preview: false))
  }

  /// `[p1, [p2 / p3]]`: a vertical split whose second child is a horizontal split.
  private var nested: PaneLayout {
    .split(
      orientation: .vertical,
      children: [
        .leaf(p1),
        .split(orientation: .horizontal, children: [.leaf(p2), .leaf(p3)], weights: [0.25, 0.75]),
      ],
      weights: [0.6, 0.4])
  }

  private func gather(
    hasFolder: Bool = true, cwd: String? = nil, sidebarVisible: Bool = true,
    layout: PaneLayout, content: Content
  ) -> SessionSeam.TabGather {
    SessionSeam.gatherTab(
      hasFolder: hasFolder, cwd: cwd ?? folder, sidebarVisible: sidebarVisible, layout: layout,
      content: content)
  }

  // MARK: - Closing a view (C2)

  func testClosingTheLastViewKeepsItsSubtree() {
    XCTAssertEqual(
      SessionSeam.closeCommit(terminating: false, otherLiveViews: 0), .keepSubtree,
      "closing the last window quits the app, so its files must be committed as they are: "
        + ".dropSubtree commits `windows: []` and the next launch opens one empty window, "
        + ".skip leaves the session at whatever the last unrelated trigger wrote")
  }

  func testClosingWhileTerminatingCommitsNothing() {
    for others in [0, 3] {
      XCTAssertEqual(
        SessionSeam.closeCommit(terminating: true, otherLiveViews: others), .skip,
        "a view closing during termination (\(others) other live views) must commit nothing: "
          + "applicationWillTerminate has already committed the whole tree, and a drop here "
          + "would overwrite it with one window fewer")
    }
    for others in [1, 2] {
      XCTAssertEqual(
        SessionSeam.closeCommit(terminating: false, otherLiveViews: others), .dropSubtree,
        "closing one of several windows (\(others) others live) must commit without it: "
          + ".keepSubtree resurrects a window the user closed, .skip leaves it in state.json "
          + "until some unrelated commit")
    }
  }

  /// What a `CloseBurst` under test did: its reads, commits and scheduled ends.
  private final class BurstRecord {
    /// The trees `read` returns, one per call, in order.
    let reads: [String]
    var readCount = 0
    var committed: [String] = []
    /// The end closures `scheduleEnd` received, held and never run unless a test runs them.
    var ends: [() -> Void] = []

    init(reads: [String]) { self.reads = reads }

    /// Runs and drops every scheduled end, as the next main-queue turn would.
    func runEnds() {
      let pending = ends
      ends = []
      for end in pending { end() }
    }
  }

  /// The tree read while both tabs of a two-tab window are still live and grouped.
  private let bothTabs = "a1+a2"
  /// The tree read once the first tab is torn down: the second tab alone.
  private let secondTab = "a2"

  private func makeBurst(_ record: BurstRecord) -> SessionSeam.CloseBurst<String> {
    SessionSeam.CloseBurst<String>(
      scheduleEnd: { record.ends.append($0) },
      read: {
        defer { record.readCount += 1 }
        guard record.readCount < record.reads.count else { return "read past the list" }
        return record.reads[record.readCount]
      },
      commit: { record.committed.append($0) })
  }

  func testATabGroupClosedInOneTurnKeepsEveryTab() {
    let record = BurstRecord(reads: [bothTabs, secondTab])
    let burst = makeBurst(record)

    XCTAssertEqual(
      burst.close(terminating: false, otherLiveViews: 1), .dropSubtree,
      "the first tab of a two-tab window closing by the red button still has a live sibling, "
        + "so it must drop; .keepSubtree commits before the rest of the group has closed")
    XCTAssertEqual(
      record.committed, [], "a dropping close must commit nothing; a commit here wrote a tree")

    XCTAssertEqual(
      burst.close(terminating: false, otherLiveViews: 0), .keepSubtree,
      "the group's last tab closing in the same turn is quitting and must keep its subtree")
    XCTAssertEqual(
      record.committed, [bothTabs],
      "the keep must commit the tree read at the burst's first close, with both tabs: "
        + "[\"\(secondTab)\"] means it read again after the first tab was torn down and "
        + "lost it, [] means .keepSubtree skipped the commit")
    XCTAssertEqual(
      record.readCount, 1,
      "one burst must read the tree once, at its first close; 2 means every close re-reads")
    XCTAssertEqual(
      record.ends.count, 1,
      "one burst must schedule one end; 0 leaves it open forever, 2 opened a second burst")
  }

  func testATabClosedAloneDoesNotReturnAfterItsTurn() {
    let record = BurstRecord(reads: [bothTabs, secondTab])
    let burst = makeBurst(record)

    XCTAssertEqual(
      burst.close(terminating: false, otherLiveViews: 1), .dropSubtree,
      "a tab closed alone while its sibling lives must drop")
    XCTAssertEqual(
      record.ends.count, 1, "the lone close must schedule its burst's end on the next turn")
    record.runEnds()

    XCTAssertEqual(
      burst.close(terminating: false, otherLiveViews: 0), .keepSubtree,
      "closing the remaining tab on a later turn quits the app and must keep its subtree")
    XCTAssertEqual(
      record.committed, [secondTab],
      "a later close opens a burst of its own and commits a fresh read, the remaining tab: "
        + "[\"\(bothTabs)\"] means the end closure did not close the first burst, so the "
        + "tab the user closed returns at relaunch")
    XCTAssertEqual(
      record.readCount, 2,
      "two bursts must read twice; 1 means the second close reused the first burst's tree")
  }

  func testATerminatingCloseOpensNoBurst() {
    let record = BurstRecord(reads: [bothTabs, secondTab])
    let burst = makeBurst(record)

    XCTAssertEqual(
      burst.close(terminating: true, otherLiveViews: 0), .skip,
      "a close during termination must skip: the terminate commit already wrote the tree")
    XCTAssertEqual(
      record.readCount, 0,
      "a terminating close must not read: the read walks views that are being torn down")
    XCTAssertEqual(record.committed, [], "a terminating close must commit nothing")
    XCTAssertTrue(
      record.ends.isEmpty,
      "a terminating close must open no burst; a scheduled end means it opened one")

    XCTAssertEqual(
      burst.close(terminating: false, otherLiveViews: 0), .keepSubtree,
      "a non-terminating last close after it must keep; anything else voids the checks above")
    XCTAssertEqual(
      record.committed, [bothTabs],
      "the first non-terminating close must commit the first read: [\"\(secondTab)\"] means "
        + "the terminating close consumed a read, [] means the keep skipped its commit")
  }

  func testAWalkWithNoLiveViewOrNoWindowCommitsNothing() {
    XCTAssertNil(
      SessionSeam.committedWindows(liveViews: 0, windows: ["w"]),
      "with no live view the walk read only torn-down controllers; committing it writes a "
        + "nil-handle, cwd \"\" window over the saved session")
    XCTAssertNil(
      SessionSeam.committedWindows(liveViews: 1, windows: [String]()),
      "a walk that produced no window (every core failed) must commit nothing: committing "
        + "`windows: []` erases the session the failed launch could not restore")
    XCTAssertEqual(
      SessionSeam.committedWindows(liveViews: 1, windows: ["w"]), ["w"],
      "a walk with a live view and a window must commit exactly that window; nil here means "
        + "no session is ever written")
  }

  // MARK: - Gathering a tab (C3a, C5)

  func testAnEmptyStateLonePaneContributesNoLayout() {
    XCTAssertEqual(
      gather(
        hasFolder: false, cwd: home, sidebarVisible: false, layout: .leaf(p1),
        content: [p1: .buffer(path: "")]),
      .ready(RestoreSeam.SavedTab(cwd: "", sidebarVisible: false, layout: nil)),
      "an empty-state lone unnamed pane is saved with cwd \"\" and no layout: a layout "
        + "persists an empty leaf, and the launch cwd (home) as cwd restores home as a folder")
    XCTAssertEqual(
      gather(layout: .leaf(p1), content: [p1: .buffer(path: fileA)]),
      .ready(
        RestoreSeam.SavedTab(
          cwd: folder, sidebarVisible: true, layout: .leaf(path: fileA, preview: false))),
      "a lone pane showing a file must persist its one leaf; nil drops the file at relaunch")
    XCTAssertEqual(
      gather(layout: .leaf(p1), content: [p1: .preview(source: fileA)]),
      .ready(
        RestoreSeam.SavedTab(
          cwd: folder, sidebarVisible: true, layout: .leaf(path: fileA, preview: true))),
      "a lone preview pane must persist a preview leaf; nil or preview: false loses the preview")
  }

  func testEveryLeafResolvesByItsWindow() {
    let expected = SavedPaneNode.split(
      orientation: .vertical, weights: [0.6, 0.4],
      children: [
        .leaf(path: fileA, preview: false),
        .split(
          orientation: .horizontal, weights: [0.25, 0.75],
          children: [.leaf(path: fileB, preview: true), .leaf(path: "", preview: false)]),
      ])
    XCTAssertEqual(
      gather(
        layout: nested,
        content: [p1: .buffer(path: fileA), p2: .preview(source: fileB), p3: .buffer(path: "")]),
      .ready(RestoreSeam.SavedTab(cwd: folder, sidebarVisible: true, layout: expected)),
      "each leaf must carry its own window's content and the tree its weights: a path of \"\" "
        + "loses a file, preview: false reopens a preview as an editor, other weights resize "
        + "the panes, and .refuse writes nothing for a fully reported tab")
  }

  func testATerminalPanePersistsAsATerminalLeaf() {
    let sub = "/Users/alice/projects/notes/sub"
    XCTAssertEqual(
      gather(
        layout: .split(
          orientation: .vertical, children: [.leaf(p1), .leaf(p2)], weights: [0.5, 0.5]),
        content: [p1: .buffer(path: fileA), p2: .terminal(cwd: sub)]),
      .ready(
        RestoreSeam.SavedTab(
          cwd: folder, sidebarVisible: true,
          layout: .split(
            orientation: .vertical, weights: [0.5, 0.5],
            children: [
              .leaf(path: fileA, preview: false),
              .leaf(path: sub, preview: false, terminal: true),
            ]))),
      "a terminal pane must persist as a terminal leaf carrying its cwd; terminal: false means "
        + ".terminal is mapped like .buffer and the pane restores as an editor of its directory")
    XCTAssertEqual(
      gather(layout: .leaf(p1), content: [p1: .terminal(cwd: "")]),
      .ready(
        RestoreSeam.SavedTab(
          cwd: folder, sidebarVisible: true,
          layout: .leaf(path: "", preview: false, terminal: true))),
      "a lone terminal pane with no known cwd must still persist its leaf; nil means it was "
        + "taken for the empty-state lone pane")
  }

  func testAnUnreportedLeafRefusesTheTab() {
    XCTAssertEqual(
      gather(
        layout: nested,
        content: [p1: .buffer(path: fileA), p2: .unreported, p3: .buffer(path: fileC)]),
      .refuse,
      "a leaf whose window is not reported yet must refuse the tab; .ready writes that leaf "
        + "as an empty path (or the startup Untitled.md) and the file is lost at relaunch")
    XCTAssertEqual(
      gather(layout: nested, content: [p1: .buffer(path: fileA), p3: .buffer(path: fileC)]),
      .refuse,
      "a leaf missing from content (no window bound yet) must refuse the tab like .unreported")
    XCTAssertEqual(
      gather(layout: .leaf(p1), content: [p1: .unreported]), .refuse,
      "an unreported lone pane must refuse the tab; .ready commits it layout-less, which "
        + "restores as an empty pane")
    XCTAssertEqual(
      gather(layout: .leaf(p1), content: [:]), .refuse,
      "a lone pane with no window yet must refuse the tab, as the launch commit sees it")
  }

  func testAFolderTabWithNoReportedCwdRefuses() {
    let content: Content = [p1: .buffer(path: fileA)]
    XCTAssertEqual(
      gather(hasFolder: true, cwd: "", layout: .leaf(p1), content: content), .refuse,
      "a folder view whose cwd is not reported yet must refuse: .ready writes cwd \"\", "
        + "which restores the folder as the empty state")
    XCTAssertEqual(
      gather(hasFolder: true, cwd: folder, layout: .leaf(p1), content: content),
      .ready(
        RestoreSeam.SavedTab(
          cwd: folder, sidebarVisible: true, layout: .leaf(path: fileA, preview: false))),
      "the same tab with its cwd reported must be ready; .refuse here means the refusal "
        + "above holds for every tab, not for a missing cwd")
  }

  // MARK: - Contribution and its cache (C1, C5, C6, C7)

  func testARefusedGatherContributesTheCache() {
    XCTAssertEqual(
      SessionSeam.contribution(mayGather: true, gather: .refuse, cache: seed), seed,
      "a refused gather must contribute the cached tab: nil drops the window's files from "
        + "state.json, which is how a launch commit erased every restored tab")
    XCTAssertNil(
      SessionSeam.contribution(mayGather: true, gather: .refuse, cache: nil),
      "a refused gather with no cache contributes nothing rather than an invented tab")
  }

  func testAReadyGatherReplacesTheCache() {
    XCTAssertEqual(
      SessionSeam.contribution(mayGather: true, gather: .ready(live), cache: seed), live,
      "a ready gather must replace the cache; returning the cache freezes the session at "
        + "launch, so files opened since are never saved")
  }

  func testAShutGateNeverGathers() {
    var gathered = false
    func recordingGather() -> SessionSeam.TabGather {
      gathered = true
      return .ready(live)
    }
    XCTAssertEqual(
      SessionSeam.contribution(mayGather: false, gather: recordingGather(), cache: seed), seed,
      "a shut gate must contribute the cache: the live tab of a failed or missing-folder "
        + "restore overwrites the saved folder and files")
    XCTAssertFalse(gathered, "a shut gate must not evaluate the gather, which reads the FFI")

    gathered = false
    XCTAssertEqual(
      SessionSeam.contribution(mayGather: true, gather: recordingGather(), cache: seed), live,
      "an open gate must evaluate the gather and contribute its tab")
    XCTAssertTrue(
      gathered, "the recording gather never ran with the gate open, so the check above is void")
  }

  func testAFailedRestoreKeepsTheSeed() {
    XCTAssertFalse(
      SessionSeam.mayGather(hasFolder: true, restoredCwd: folder, restoreFailed: true),
      "a view whose rebuild failed shows one fallback pane; gathering it replaces the saved "
        + "split and files with that pane")
    XCTAssertFalse(
      SessionSeam.mayGather(hasFolder: true, restoredCwd: nil, restoreFailed: true),
      "restoreFailed alone must shut the gate, whatever the restored cwd")
    XCTAssertTrue(
      SessionSeam.mayGather(hasFolder: true, restoredCwd: folder, restoreFailed: false),
      "a restored folder view whose rebuild succeeded must gather, or its later edits are lost")
  }

  func testAMissingRestoredFolderKeepsTheSeedUntilAFolderOpens() {
    XCTAssertFalse(
      SessionSeam.mayGather(hasFolder: false, restoredCwd: "/Volumes/gone", restoreFailed: false),
      "a view restored to the empty state because its saved folder was missing must keep "
        + "its seed: gathering writes the empty state over the folder and its files")
    XCTAssertTrue(
      SessionSeam.mayGather(hasFolder: true, restoredCwd: "/Volumes/gone", restoreFailed: false),
      "once that view opens a folder it must gather again, or the new folder is never saved")
    XCTAssertTrue(
      SessionSeam.mayGather(hasFolder: false, restoredCwd: "", restoreFailed: false),
      "a view restored from a saved empty state has nothing to keep and must gather")
    XCTAssertTrue(
      SessionSeam.mayGather(hasFolder: false, restoredCwd: nil, restoreFailed: false),
      "a view that was never restored must gather")
  }

  // MARK: - Orphaned slots (C6)

  func testUnmaterializedSlotsAreCarriedInSavedOrder() {
    let slot: (Int, Int) -> SessionSeam.TabSlot = {
      SessionSeam.TabSlot(groupIndex: $0, positionInGroup: $1)
    }
    XCTAssertEqual(
      SessionSeam.orphanedSlots(savedTabCounts: [3, 2], materialized: [slot(0, 0), slot(0, 2)]),
      [
        SessionSeam.OrphanGroup(groupIndex: 0, positions: [1]),
        SessionSeam.OrphanGroup(groupIndex: 1, positions: [0, 1]),
      ],
      "every saved slot without a live view must be carried, by window and in saved order: "
        + "a missing slot drops that tab from state.json, an extra one duplicates a live tab")
    XCTAssertEqual(
      SessionSeam.orphanedSlots(
        savedTabCounts: [1, 1, 1], materialized: [slot(1, 0)]),
      [
        SessionSeam.OrphanGroup(groupIndex: 0, positions: [0]),
        SessionSeam.OrphanGroup(groupIndex: 2, positions: [0]),
      ],
      "a fully materialized window must yield no group, and the others keep ascending order")
    XCTAssertEqual(
      SessionSeam.orphanedSlots(
        savedTabCounts: [2, 1], materialized: [slot(0, 0), slot(0, 1), slot(1, 0)]),
      [],
      "with every slot materialized nothing is orphaned; any group here duplicates a tab")
  }
}
