// The launch restore of a saved tab through WindowStateCodec and rebuild_tab, over a live core.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A tab saved to the window state, then restored the way the launch restores it: read back
/// through `WindowStateCodec.load`, whose `rebuild` is the tab after `FfiAppTab.restored()`, and
/// handed to `PaneContainerView.requestLayoutRebuild` over a real core and Neovim.
///
/// The premise under SR1 is the rebuild helper's, measured here rather than assumed: an edit
/// pane with an empty path issues no `:edit`, so its window keeps the buffer it was split from,
/// the core's startup buffer, `Untitled.md`.
///
/// The window state lives in `LiveCore`'s `WOLF_STATE_DIR`, which `LiveCore.scratchDirectory`
/// prepares before the first `wolf_ffi_app_state_*` call opens the store. A failure prefixed
/// `premise:` is a setup step that did not happen.
@MainActor
final class SessionRestoreTests: XCTestCase {

  /// The startup buffer's name, which an empty pane inherits.
  private let startupBuffer = "Untitled.md"

  // MARK: - SR1: a deleted file restores as an empty pane

  func testARestoredTabWhoseFileWasDeletedShowsAnEmptyPane() {
    guard let dir = LiveCore.scratchDirectory() else { return }
    let kept = (dir as NSString).appendingPathComponent("kept.md")
    let gone = (dir as NSString).appendingPathComponent("gone.md")
    do {
      try "# kept\n".write(toFile: kept, atomically: true, encoding: .utf8)
      try "# gone\n".write(toFile: gone, atomically: true, encoding: .utf8)
    } catch {
      XCTFail("premise: could not write the fixture files in \(dir): \(error)")
      return
    }
    let tab = RestoreSeam.SavedTab(
      cwd: dir, sidebarVisible: true,
      layout: .split(
        orientation: .vertical, weights: [0.5, 0.5],
        children: [
          .leaf(path: kept, preview: false), .leaf(path: gone, preview: false, zoomed: true),
        ]))
    WindowStateCodec.submit([
      WindowStateCodec.Window(
        frame: CGRect(origin: .zero, size: TerminalViewFixture.windowSize), activeTab: 0,
        minimized: false, tabs: [tab], recency: [0])
    ])
    XCTAssertTrue(wolf_ffi_app_state_flush(), "premise: the window state must flush")
    do {
      try FileManager.default.removeItem(atPath: gone)
    } catch {
      XCTFail("premise: could not delete \(gone): \(error)")
      return
    }

    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    let loaded = WindowStateCodec.load(wolf_ffi_app_state_window_state())
    guard let saved = loaded.last?.tabs.last else {
      XCTFail("premise: the committed window state must load back with its tab")
      return
    }
    XCTAssertEqual(
      saved.saved, tab,
      "premise: the loaded tab must be the one this test saved, its zoom and the deleted path "
        + "included, since the store checks no file")
    guard let rebuild = saved.rebuild else {
      XCTFail("a saved tab with a root must load with a rebuild; nil restores no layout")
      return
    }
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
      return shown.count == 2
    }
    XCTAssertTrue(
      reported,
      "premise: the restore must rebuild two panes, each showing a named buffer: "
        + "\(container.paneCount) pane(s), buffers \(shown)")
    guard reported else { return }
    XCTAssertEqual(
      shown.first, "kept.md", "the pane on the file that still exists must show it, in place")
    XCTAssertEqual(
      shown.last, startupBuffer,
      "the pane on the deleted file must restore empty, keeping the startup buffer it was "
        + "split from; gone.md means the rebuild edited the missing file, so the restore "
        + "applied none of §5's rules")

    var zoomedAt: [String] = []
    fixture.live.waitFor(timeout: 1) {
      container.onRenderWake()
      if container.isZoomed { zoomedAt.append("frames \(container.visiblePaneFrames)") }
      return false
    }
    XCTAssertEqual(
      zoomedAt, [],
      "the zoom named the pane that restored empty, so the view must stay unzoomed at every "
        + "wake; zoomed means the rebuild layout kept the saved zoom")
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
