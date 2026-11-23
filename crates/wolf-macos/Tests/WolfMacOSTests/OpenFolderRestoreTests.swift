// Open Folder's read of a project state through WindowStateCodec.loadProject, over a live core.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A directory's project state, saved by the ordinary window-state submit and read back the way
/// Open Folder reads it (`specs/application-state.md` §6): `wolf_ffi_app_state_project_state`,
/// then `WindowStateCodec.loadProject`, whose tab's `rebuild` is handed to
/// `PaneContainerView.requestLayoutRebuild` over a real core and Neovim.
///
/// The project state outlives the window state: the second submit holds no tab on `D`, and §4
/// keeps a directory's row when no tab is open on it. So what is read is the stored row, not
/// the window the first submit wrote.
///
/// The store lives in `LiveCore`'s `WOLF_STATE_DIR`, which `LiveCore.scratchDirectory` prepares
/// before the first `wolf_ffi_app_state_*` call opens it. A failure prefixed `premise:` is a
/// setup step that did not happen.
@MainActor
final class OpenFolderRestoreTests: XCTestCase {

  /// The frame the project's window is saved with, every field distinct and nonzero.
  private let savedFrame = CGRect(x: 120, y: 80, width: 1400, height: 900)

  // MARK: - M8: a project state loads with its frame and a rebuild that shows its panes

  func testAProjectStateLoadsItsFrameAndRebuildsItsSplit() {
    guard let dir = LiveCore.scratchDirectory(), let elsewhere = LiveCore.scratchDirectory()
    else { return }
    let first = (dir as NSString).appendingPathComponent("d1.md")
    let second = (dir as NSString).appendingPathComponent("d2.md")
    do {
      try "# first\n".write(toFile: first, atomically: true, encoding: .utf8)
      try "# second\n".write(toFile: second, atomically: true, encoding: .utf8)
    } catch {
      XCTFail("premise: could not write the fixture files in \(dir): \(error)")
      return
    }
    let tab = RestoreSeam.SavedTab(
      cwd: dir, sidebarVisible: true,
      layout: .split(
        orientation: .vertical, weights: [0.5, 0.5],
        children: [.leaf(path: first, preview: false), .leaf(path: second, preview: false)]))
    WindowStateCodec.submit([
      WindowStateCodec.Window(
        frame: savedFrame, activeTab: 0, minimized: false, tabs: [tab], recency: [0])
    ])
    XCTAssertTrue(wolf_ffi_app_state_flush(), "premise: the window holding D must flush")
    WindowStateCodec.submit([
      WindowStateCodec.Window(
        frame: CGRect(x: 10, y: 20, width: 800, height: 600), activeTab: 0, minimized: false,
        tabs: [RestoreSeam.SavedTab(cwd: elsewhere, sidebarVisible: true, layout: nil)],
        recency: [0])
    ])
    XCTAssertTrue(wolf_ffi_app_state_flush(), "premise: the window without D must flush")

    guard let project = WindowStateCodec.loadProject(wolf_ffi_app_state_project_state(dir)) else {
      XCTFail(
        "D's project state was written by the first submit and kept by the second, so "
          + "loadProject must answer it; nil means it was not read or not found")
      return
    }
    XCTAssertEqual(
      project.frame, savedFrame,
      "the project state carries the frame of the window its tab came from; nil means "
        + "loadProject dropped the frame, another frame means it read the wrong window's")
    guard let rebuild = project.tab.rebuild else {
      XCTFail("a project tab with a root must load with a rebuild; nil restores no layout")
      return
    }

    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    container.requestLayoutRebuild(rebuild.layout, tab: rebuild.tab)

    var shown: [String] = []
    let reported = fixture.wake {
      guard container.paneCount == 2 else { return false }
      let contents = container.leafContents()
      shown = leafIds(container.paneLayout).compactMap { id -> String? in
        if case .buffer(let path) = contents[id], !path.isEmpty {
          return (path as NSString).lastPathComponent
        }
        return nil
      }
      return shown == ["d1.md", "d2.md"]
    }
    XCTAssertTrue(
      reported,
      "the rebuilt project tab must show two panes on d1.md and d2.md, in that order: "
        + "\(container.paneCount) pane(s), buffers \(shown). One pane means the rebuild did "
        + "not split; the startup buffer in a pane means it edited no file")
  }

  // MARK: - M9: a directory with no project state loads nothing

  func testAnUnseenDirectoryLoadsNoProject() {
    guard let dir = LiveCore.scratchDirectory() else { return }
    let unseen = (dir as NSString).appendingPathComponent("unseen-\(UUID().uuidString)")
    XCTAssertNil(
      WindowStateCodec.loadProject(wolf_ffi_app_state_project_state(unseen)),
      "no project state was ever written for this directory, so loadProject must answer nil, "
        + "which Open Folder takes for a fresh tab; a project means found() was ignored")
  }

  /// The leaf ids of `layout`, in depth-first order.
  private func leafIds(_ layout: PaneLayout) -> [PaneId] {
    switch layout {
    case .leaf(let id):
      return [id]
    case .split(_, let children, _):
      return children.flatMap(leafIds)
    }
  }
}
