// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import Foundation
import XCTest

@testable import WolfKeyMap

private typealias LiveTab = OpenFolderSeam.LiveTab
private typealias LiveWindow = OpenFolderSeam.LiveWindow

/// `FileOpenSeam` routes a request from outside Wolf, `specs/cli-and-file-open.md` §4 and §5.1:
/// a live `WOLF_TAB_ID` names the current tab, or the tab a file opens in, and otherwise the
/// current tab is the frontmost window's active tab and a file goes to its project. It also
/// decodes core's §5.2 pane choice for a tab's layout.
///
/// Every live window list is passed out of stacking order, so only the stacking can put the
/// current tab right, and tab ids never equal a tab index. Routes, targets and placements are
/// compared through `describe`, so no conformance is assumed.
///
/// The project lookup and the pane choice run through wolf-ffi's builder scratch, never the
/// store, so this file needs no `StateDirFence`.
final class FileOpenSeamTests: XCTestCase {

  private let dirA = "/wolf-file-open-test/alpha"
  private let dirB = "/wolf-file-open-test/beta"
  private let dirC = "/wolf-file-open-test/gamma"

  /// The `WOLF_TAB_ID` of tab 11, which `tabIds` maps.
  private let idOf11 = "3f1c6a52-0b7e-4d2a-9c41-5e8f0a7b6c11"
  /// The `WOLF_TAB_ID` of tab 22.
  private let idOf22 = "8a2d4e61-7c3b-4f9e-a052-1d6b9c8e7f22"
  /// A well-formed ID `tabIds` does not map: a closed tab, or a relaunched Wolf.
  private let unmappedId = "c7e1b0d4-2f5a-4b68-9e3c-a4d7f2e1b033"
  /// An ID `tabIds` maps to tab 99, which no live window holds.
  private let closedTabId = "5b9e3a71-d2c4-4e86-8f10-b6a3c5d9e044"

  /// Tab 11 on A, alone in the back window; tab 21 on C and tab 22 on B, 22 selected, in the
  /// front window.
  private var windows: [LiveWindow] {
    [
      LiveWindow(tabs: [LiveTab(id: 11, directory: dirA)], selected: 11),
      LiveWindow(
        tabs: [LiveTab(id: 21, directory: dirC), LiveTab(id: 22, directory: dirB)], selected: 22),
    ]
  }

  private var tabIds: [String: Int] { [idOf11: 11, idOf22: 22, closedTabId: 99] }

  /// The stacking that focuses `tabs` in order, so the last is frontmost.
  private func stacking(focusing tabs: [Int]) -> StackingOrder {
    var stacking = StackingOrder()
    for tab in tabs { stacking.focus(tab) }
    return stacking
  }

  /// The front window, holding 21 and 22, ahead of the back one, which the list puts first.
  private var frontSecond: StackingOrder { stacking(focusing: [11, 21, 22]) }

  // MARK: - F1: activate

  func testAnActivateRequestCarriesTheFrontmostWindowsActiveTab() {
    let route = FileOpenSeam.route(
      FileOpenSeam.Request(target: .activate, tabId: ""), windows: windows, tabIds: tabIds,
      stacking: frontSecond)
    XCTAssertEqual(
      describe(route), "activate(22)",
      "a bare `wolf` must activate the frontmost window's active tab, 22, the front window's "
        + "selected tab; activate(nil) means the current tab is dropped, activate(11) that the "
        + "first window in list order was taken as frontmost")
  }

  func testAWindowWithNoSelectedTabFallsBackToItsMostRecentTab() {
    let front = LiveWindow(
      tabs: [LiveTab(id: 21, directory: dirC), LiveTab(id: 22, directory: dirB)], selected: nil)
    XCTAssertEqual(
      FileOpenSeam.currentTab(windows: [front], stacking: stacking(focusing: [21, 22])), 22,
      "with no selected tab, the frontmost window's active tab is its most recently focused, 22; "
        + "21 means the first tab in tab order was taken")
  }

  // MARK: - F2, F3: a directory

  func testADirectoryWithALiveTabIdUsesThatTabAsCurrent() {
    let route = FileOpenSeam.route(
      FileOpenSeam.Request(target: .directory(dirC), tabId: idOf11), windows: windows,
      tabIds: tabIds, stacking: frontSecond)
    XCTAssertEqual(
      describe(route), "openFolder(\(dirC), 11)",
      "a directory sent from tab 11's terminal is Open Folder with 11 as the current tab, though "
        + "tab 22 is frontmost; openFolder(…, 22) means the tab ID is ignored for directories")
  }

  func testADirectoryWithAStaleTabIdUsesTheFrontmostActiveTab() {
    for stale in [unmappedId, closedTabId] {
      let route = FileOpenSeam.route(
        FileOpenSeam.Request(target: .directory(dirC), tabId: stale), windows: windows,
        tabIds: tabIds, stacking: frontSecond)
      XCTAssertEqual(
        describe(route), "openFolder(\(dirC), 22)",
        "a tab ID naming no live tab (\(stale)) is handled as unset, so the current tab is the "
          + "frontmost window's active tab, 22; 11 means the first window in list order was "
          + "taken as frontmost, 99 that a tab ID whose tab is in no window counted as live")
    }
  }

  // MARK: - F4, F5, F6: a file

  func testAFileWithALiveTabIdOpensInThatTabWhateverItsProject() {
    let file = (dirB as NSString).appendingPathComponent("notes.md")
    let route = FileOpenSeam.route(
      FileOpenSeam.Request(target: .file(file), tabId: idOf11), windows: windows,
      tabIds: tabIds, stacking: frontSecond)
    XCTAssertEqual(
      describe(route), "openFile(\(file), 11)",
      "§5.1 rule 1: a file sent from tab 11 opens in tab 11, though it lies in B, open in tab "
        + "22; openFileInProject means the project was resolved ahead of the tab ID")
  }

  func testAFileWithAStaleTabIdGoesToItsProject() {
    let file = (dirB as NSString).appendingPathComponent("notes.md")
    for stale in [unmappedId, closedTabId] {
      let route = FileOpenSeam.route(
        FileOpenSeam.Request(target: .file(file), tabId: stale), windows: windows,
        tabIds: tabIds, stacking: frontSecond)
      XCTAssertEqual(
        describe(route), "openFileInProject(\(file), \(dirB), 22)",
        "a file with a tab ID naming no live tab (\(stale)) goes to its project, B, open in "
          + "tab 22, with the frontmost active tab as current; openFile(…, 22) means a stale ID "
          + "fell back to the current tab rather than to resolution")
    }
  }

  func testAFileInAnOpenSubdirectoryOfARepositoryGoesToThatSubdirectory() throws {
    let root = try repository()
    let sub = (root as NSString).appendingPathComponent("packages/app")
    let file = (sub as NSString).appendingPathComponent("src/main.md")
    try FileManager.default.createDirectory(
      atPath: (file as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
    try "# main\n".write(toFile: file, atomically: true, encoding: .utf8)
    let live = [
      LiveWindow(tabs: [LiveTab(id: 31, directory: sub)], selected: 31),
      LiveWindow(tabs: [LiveTab(id: 41, directory: dirA)], selected: 41),
    ]
    let route = FileOpenSeam.route(
      FileOpenSeam.Request(target: .file(file), tabId: ""), windows: live, tabIds: [:],
      stacking: stacking(focusing: [31, 41]))
    XCTAssertEqual(
      describe(route), "openFileInProject(\(file), \(sub), 41)",
      "§5.1 step 2.1: inside the repository \(root), the nearest directory open in a tab that "
        + "contains the file is \(sub), so it is the project; the repository root means the "
        + "lookup was given no open directories")
  }

  // MARK: - C1 to C5: the pane choice

  private let fileF = "/wolf-file-open-test/alpha/f.md"
  private let fileX = "/wolf-file-open-test/alpha/x.md"
  private let fileY = "/wolf-file-open-test/alpha/y.md"

  private func edit(_ path: String, zoomed: Bool = false) -> SavedPaneNode {
    .leaf(path: path, preview: false, zoomed: zoomed)
  }

  private func vertical(_ children: SavedPaneNode...) -> SavedPaneNode {
    .split(
      orientation: .vertical,
      weights: Array(
        repeating: 1.0 / Double(children.count),
        count: children.count), children: children)
  }

  func testATerminalOnlyLayoutSplitsLeft() {
    let terminals = SavedPaneNode.split(
      orientation: .horizontal, weights: [0.5, 0.5],
      children: [
        .leaf(path: dirA, preview: false, terminal: true),
        .leaf(path: dirA, preview: false, terminal: true),
      ])
    let choice = FileOpenSeam.choosePane(
      fileF, layout: terminals, focused: 0, drawerZoomed: false)
    XCTAssertEqual(
      describe(choice.placement), "splitLeft",
      "§5.2 rule 4: with only terminal panes, the file gets a new pane split off the whole tab; "
        + "open(0) means terminal leaves reached core as edit panes")
  }

  func testTheFocusedEditorPaneTakesTheFile() {
    let choice = FileOpenSeam.choosePane(
      fileF, layout: vertical(edit(fileX), edit(fileY)), focused: 1, drawerZoomed: false)
    XCTAssertEqual(
      describe(choice.placement), "open(1)",
      "§5.2 rule 1: the focused editor pane, 1, takes the file; open(0) means the focus never "
        + "reached core, so the leftmost editor pane was chosen")
    XCTAssertFalse(choice.unzoom, "an unzoomed layout must not ask to unzoom")
  }

  func testAZoomedPaneOrDrawerIsUnzoomed() {
    let zoomedPane = FileOpenSeam.choosePane(
      fileF, layout: vertical(edit(fileX), edit(fileY, zoomed: true)), focused: 1,
      drawerZoomed: false)
    XCTAssertTrue(
      zoomedPane.unzoom,
      "§5.2: a zoomed pane is unzoomed first; false means the zoomed leaf's mark never reached "
        + "core")
    let zoomedDrawer = FileOpenSeam.choosePane(
      fileF, layout: vertical(edit(fileX), edit(fileY)), focused: nil, drawerZoomed: true)
    XCTAssertTrue(
      zoomedDrawer.unzoom,
      "a zoomed drawer is unzoomed first too; false means the drawer's zoom never reached core")
    XCTAssertEqual(
      describe(zoomedDrawer.placement), "open(0)",
      "with the drawer focused, the leftmost editor pane, 0, takes the file")
  }

  func testAPreviewBesideATerminalBecomesTheEditor() {
    let layout = vertical(
      .leaf(path: fileX, preview: true), .leaf(path: dirA, preview: false, terminal: true))
    let choice = FileOpenSeam.choosePane(fileF, layout: layout, focused: 1, drawerZoomed: false)
    XCTAssertEqual(
      describe(choice.placement), "convert(0)",
      "§5.2 rule 3: with no editor pane, the leftmost preview, 0, becomes the editor; open(0) "
        + "means CONVERT decodes as OPEN, so the preview is never replaced, and splitLeft that "
        + "the preview reached core as a terminal")
  }

  func testAnEditorPaneAlreadyOnTheFileIsFocused() {
    let choice = FileOpenSeam.choosePane(
      fileF, layout: vertical(edit(fileX), edit(fileF)), focused: 0, drawerZoomed: false)
    XCTAssertEqual(
      describe(choice.placement), "focus(1)",
      "§5.2: the editor pane already on the file, 1, is focused; open(0) means the leaves' paths "
        + "never reached core")
  }

  // MARK: - Helpers

  /// A fresh repository under `TEST_TMPDIR`, canonical as core reports paths: a directory with a
  /// `.git` directory.
  private func repository() throws -> String {
    let env = ProcessInfo.processInfo.environment
    let tmp = env["TEST_TMPDIR"].flatMap { $0.isEmpty ? nil : $0 } ?? NSTemporaryDirectory()
    let made = (tmp as NSString).appendingPathComponent("file-open-repo-\(UUID().uuidString)")
    try FileManager.default.createDirectory(
      atPath: (made as NSString).appendingPathComponent(".git"),
      withIntermediateDirectories: true)
    return OpenFolderSeam.canonicalDirectory(made)
  }

  /// A current tab's id, or `nil`.
  private func label(_ tab: Int?) -> String {
    tab.map { String($0) } ?? "nil"
  }

  private func describe(_ route: FileOpenSeam.Route) -> String {
    switch route {
    case .activate(let current):
      return "activate(\(label(current)))"
    case .openFolder(let directory, let current):
      return "openFolder(\(directory), \(label(current)))"
    case .openFile(let file, let tab):
      return "openFile(\(file), \(tab))"
    case .openFileInProject(let file, let project, let current):
      return "openFileInProject(\(file), \(project), \(label(current)))"
    }
  }

  private func describe(_ placement: FileOpenSeam.Placement) -> String {
    switch placement {
    case .focus(let index): return "focus(\(index))"
    case .open(let index): return "open(\(index))"
    case .convert(let index): return "convert(\(index))"
    case .splitLeft: return "splitLeft"
    }
  }
}
