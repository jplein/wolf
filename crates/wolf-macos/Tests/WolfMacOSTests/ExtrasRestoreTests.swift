// Restores a tab's focus, cursor and drawer through WindowStateCodec, over a live core.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// The extras of `specs/application-state.md` §5 restored the way the launch restores them: a
/// tab submitted through `WindowStateCodec` and flushed, read back with `load`, and handed to
/// `PaneContainerView.requestLayoutRebuild(_:tab:focused:drawer:)` with its `rebuild`, over a
/// real core, Neovim and `/bin/sh`.
///
/// Panes are told apart by the file their window shows, read with `win_buffer`, and the drawer
/// by the terminal session its window shows. A focus or a zoom is sampled at every wake of a
/// settle, because Neovim reporting another pane a round trip after the rebuild moves the
/// view's active pane, and unzooms it, after a first check would have passed.
///
/// The window state lives in `LiveCore`'s `WOLF_STATE_DIR`, which `LiveCore.scratchDirectory`
/// prepares before the first `wolf_ffi_app_state_*` call opens the store. A failure prefixed
/// `premise:` is a setup step that did not happen.
@MainActor
final class ExtrasRestoreTests: XCTestCase {

  /// How long a restored focus or zoom is watched for a later change.
  private let settleSeconds: TimeInterval = 1

  // MARK: - M3: a focused pane

  /// A tab saved focused on its middle leaf restores with that leaf active in Neovim and in the
  /// view, at every wake.
  func testAFocusedPaneRestoresFocused() {
    guard let dir = LiveCore.scratchDirectory(),
      let paths = write(["first.md", "second.md", "third.md"], lines: 3, in: dir)
    else { return }
    let tab = RestoreSeam.SavedTab(
      cwd: dir, sidebarVisible: true,
      layout: .split(
        orientation: .vertical, weights: [1.0 / 3, 1.0 / 3, 1.0 / 3],
        children: paths.map { .leaf(path: $0, preview: false) }),
      focused: .pane(1))
    guard save(tab), let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    guard restoreSaved(fixture) else { return }
    let handle = fixture.handle
    let container = fixture.container

    var wB: Int64 = 0
    let rebuilt = fixture.wake {
      wB = window(fixture, showing: "second.md")
      return container.paneCount == 3 && wB != 0
    }
    XCTAssertTrue(
      rebuilt,
      "premise: the restore must rebuild three panes, one showing second.md:"
        + " \(container.paneCount) pane(s)")
    guard rebuilt else { return }

    var elsewhere: [String] = []
    settle(fixture) {
      if handle.active_win() != wB || container.activeWin != wB {
        elsewhere.append("Neovim \(handle.active_win()), view \(container.activeWin)")
      }
    }
    XCTAssertEqual(
      elsewhere, [],
      "the focused leaf, second.md's window \(wB), must be active in Neovim and the view at"
        + " every wake. third.md's window \(window(fixture, showing: "third.md")) means the"
        + " rebuild ends on its last leaf, so init.lua ignores the focused mark or submit wrote"
        + " no focus; first.md's \(window(fixture, showing: "first.md")) that the view chose its"
        + " first pane")
  }

  // MARK: - M4: a cursor beyond a shortened file

  /// A cursor saved on line 100 of a file shortened to 3 lines restores on line 3.
  func testACursorBeyondAShortenedFileIsClamped() {
    guard let dir = LiveCore.scratchDirectory(),
      let paths = write(["numbered.md"], lines: 200, in: dir), let path = paths.first
    else { return }
    let tab = RestoreSeam.SavedTab(
      cwd: dir, sidebarVisible: true,
      layout: .leaf(
        path: path, preview: false,
        view: SavedPaneView(cursor: .init(line: 100, col: 0), topLine: 90)))
    guard save(tab), write(["numbered.md"], lines: 3, in: dir) != nil,
      let fixture = TerminalViewFixture()
    else { return }
    defer { fixture.shutdown() }
    guard restoreSaved(fixture) else { return }
    let handle = fixture.handle
    let container = fixture.container

    var line: UInt32 = 0
    var shown = ""
    let clamped = fixture.wake {
      let win = container.activeWin
      shown = handle.win_buffer(win).abs_path().toString()
      guard container.paneCount == 1, shown.hasSuffix("/numbered.md") else { return false }
      let cursor = handle.window_cursor(win)
      line = cursor.found() ? cursor.line() : 0
      return line == 3
    }
    XCTAssertTrue(
      clamped,
      "the cursor saved on line 100 of a file shortened to 3 lines must restore on its last line,"
        + " 3; the active window shows \(shown) with its cursor on line \(line). Line 1 means the"
        + " saved cursor was not written or not restored, or Neovim rejected the unclamped line;"
        + " 0 that window_cursor found no window")
  }

  // MARK: - M5 to M8: the drawer

  /// A drawer saved open at half the pane area restores open, with a shell, at that share.
  func testAnOpenDrawerRestoresOpenAtItsShare() {
    guard let fixture = restoreLoneLeaf(drawer: RestoreSeam.SavedDrawer(open: true, share: 0.5))
    else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    guard let drawer = awaitDrawer(fixture) else { return }

    XCTAssertEqual(
      container.drawerShare, 0.5,
      "the restored drawer's share must be the saved 0.5; \(DrawerLayout.defaultWeight) means the"
        + " restore ignores the saved share")
    let mid = container.bounds.height - (container.bounds.height * 0.5).rounded()
    guard let frame = container.visiblePaneFrames[drawer.win] else {
      XCTFail(
        "the drawer's window \(drawer.win) must be shown: frames \(container.visiblePaneFrames)")
      return
    }
    XCTAssertEqual(
      frame.minY, mid, accuracy: 1,
      "the drawer pane's top must sit at the midline \(mid) of the \(container.bounds.height) pt"
        + " area: it is at \(frame.minY). Two thirds down means the drawer opened at the default"
        + " share")
    XCTAssertTrue(
      frame.insetBy(dx: -1, dy: -1).contains(drawer.view.frame),
      "the drawer's terminal view \(drawer.view.frame) must lie in the drawer pane \(frame)")
  }

  /// A drawer saved open and zoomed restores zoomed, and stays zoomed at every wake.
  func testAZoomedDrawerRestoresZoomed() {
    guard
      let fixture = restoreLoneLeaf(
        drawer: RestoreSeam.SavedDrawer(open: true, share: 0.4, zoomed: true))
    else { return }
    defer { fixture.shutdown() }
    let container = fixture.container

    let zoomed = fixture.wake {
      container.drawerPane != nil && container.zoomedPaneId == container.drawerPane
    }
    XCTAssertTrue(
      zoomed,
      "the restored drawer must open and be zoomed: drawer pane"
        + " \(String(describing: container.drawerPane)), zoomed pane"
        + " \(String(describing: container.zoomedPaneId)). A nil zoom with a drawer means the"
        + " drawer's pending zoom is not applied when the drawer binds")
    guard zoomed else { return }

    var unzoomed: [String] = []
    settle(fixture) {
      if container.drawerPane == nil || container.zoomedPaneId != container.drawerPane {
        unzoomed.append("zoomed \(String(describing: container.zoomedPaneId))")
      }
    }
    XCTAssertEqual(
      unzoomed, [],
      "the restored drawer must stay zoomed at every wake; an unzoom means Neovim reported"
        + " another pane after the zoom was applied")
    XCTAssertEqual(
      container.visiblePaneFrames.count, 1,
      "zoomed on the drawer, the view must show it alone: \(container.visiblePaneFrames)")
  }

  /// A leaf saved zoomed beside an open drawer restores zoomed once the drawer is bound, and
  /// stays zoomed with Neovim on it.
  func testAZoomedPaneBesideAnOpenDrawerRestoresZoomed() {
    guard let dir = LiveCore.scratchDirectory(),
      let paths = write(["first.md", "second.md"], lines: 3, in: dir)
    else { return }
    let tab = RestoreSeam.SavedTab(
      cwd: dir, sidebarVisible: true,
      layout: .split(
        orientation: .vertical, weights: [0.5, 0.5],
        children: [
          .leaf(path: paths[0], preview: false),
          .leaf(path: paths[1], preview: false, zoomed: true),
        ]),
      drawer: RestoreSeam.SavedDrawer(open: true, share: 0.33))
    guard save(tab), let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    guard restoreSaved(fixture) else { return }
    let handle = fixture.handle
    let container = fixture.container

    var wB: Int64 = 0
    let zoomed = fixture.wake {
      wB = window(fixture, showing: "second.md")
      return wB != 0 && container.drawerPane != nil && container.isZoomed
        && Set(container.visiblePaneFrames.keys) == [wB]
    }
    XCTAssertTrue(
      zoomed,
      "the restore must bind the drawer and zoom second.md's window \(wB): drawer pane"
        + " \(String(describing: container.drawerPane)), zoomed \(container.isZoomed), frames"
        + " \(container.visiblePaneFrames). Every pane shown with the drawer means the zoom was"
        + " applied before the drawer opened, whose open unzooms")
    guard zoomed else { return }

    var unzoomed: [String] = []
    settle(fixture) {
      if !container.isZoomed || Set(container.visiblePaneFrames.keys) != [wB] {
        unzoomed.append("frames \(container.visiblePaneFrames)")
      }
    }
    XCTAssertEqual(
      unzoomed, [],
      "second.md's pane must stay zoomed at every wake; an unzoom means Neovim reported another"
        + " pane, the drawer, after the zoom was applied")
    XCTAssertEqual(
      handle.active_win(), wB,
      "Neovim's current window must be the zoomed pane's \(wB), or the user types into a hidden"
        + " pane")
    XCTAssertEqual(container.activeWin, wB, "the view's active pane must be the zoomed one")
  }

  /// A tab saved focused on its open drawer restores with the drawer active in Neovim and the
  /// view, at every wake.
  func testAFocusedDrawerRestoresFocused() {
    guard
      let fixture = restoreLoneLeaf(
        drawer: RestoreSeam.SavedDrawer(open: true, share: 0.33), focused: .drawer)
    else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let drawer = awaitDrawer(fixture) else { return }

    var elsewhere: [String] = []
    settle(fixture) {
      if container.activePaneId != container.drawerPane || handle.active_win() != drawer.win {
        elsewhere.append(
          "view pane \(String(describing: container.activePaneId)), Neovim"
            + " \(handle.active_win())")
      }
    }
    XCTAssertEqual(
      elsewhere, [],
      "the focused drawer, pane \(String(describing: container.drawerPane)) on window"
        + " \(drawer.win), must be active in Neovim and the view at every wake. The leaf's pane"
        + " and window mean the restore moved Neovim to a leaf after opening the drawer")
  }

  // MARK: - Helpers

  /// Writes `lines` numbered lines to each of `names` in `dir` and returns their paths; `nil`,
  /// with the test failed, when a write fails.
  private func write(
    _ names: [String], lines: Int, in dir: String, file: StaticString = #filePath,
    line: UInt = #line
  ) -> [String]? {
    let text = (1...lines).map { "row \($0)\n" }.joined()
    var paths: [String] = []
    for name in names {
      let path = (dir as NSString).appendingPathComponent(name)
      do {
        try text.write(toFile: path, atomically: true, encoding: .utf8)
      } catch {
        XCTFail("premise: could not write \(path): \(error)", file: file, line: line)
        return nil
      }
      paths.append(path)
    }
    return paths
  }

  /// Submits `tab` as the only tab of the window state and flushes it; `false`, with the test
  /// failed, when the write does not flush.
  private func save(
    _ tab: RestoreSeam.SavedTab, file: StaticString = #filePath, line: UInt = #line
  ) -> Bool {
    WindowStateCodec.submit([
      WindowStateCodec.Window(
        frame: CGRect(origin: .zero, size: TerminalViewFixture.windowSize), activeTab: 0,
        minimized: false, tabs: [tab], recency: [0])
    ])
    let flushed = wolf_ffi_app_state_flush()
    XCTAssertTrue(flushed, "premise: the window state must flush", file: file, line: line)
    return flushed
  }

  /// Loads the saved window state and requests its last tab's rebuild in `fixture`, as the
  /// launch does; `false`, with the test failed, when the tab loads with no rebuild.
  private func restoreSaved(
    _ fixture: TerminalViewFixture, file: StaticString = #filePath, line: UInt = #line
  ) -> Bool {
    let loaded = WindowStateCodec.load(wolf_ffi_app_state_window_state())
    guard let rebuild = loaded.last?.tabs.last?.rebuild else {
      XCTFail(
        "premise: the saved tab must load back with a rebuild: \(loaded.count) window(s)",
        file: file, line: line)
      return false
    }
    fixture.container.requestLayoutRebuild(
      rebuild.layout, tab: rebuild.tab, focused: rebuild.focused, drawer: rebuild.drawer)
    return true
  }

  /// Saves a tab of one edit leaf with `drawer` and `focused` and restores it into a new
  /// fixture; `nil`, with the test failed, when a step does not happen.
  private func restoreLoneLeaf(
    drawer: RestoreSeam.SavedDrawer, focused: RestoreSeam.SavedPaneRef? = nil,
    file: StaticString = #filePath, line: UInt = #line
  ) -> TerminalViewFixture? {
    guard let dir = LiveCore.scratchDirectory(file: file, line: line),
      let paths = write(["lone.md"], lines: 3, in: dir, file: file, line: line)
    else { return nil }
    let tab = RestoreSeam.SavedTab(
      cwd: dir, sidebarVisible: true, layout: .leaf(path: paths[0], preview: false),
      focused: focused, drawer: drawer)
    guard save(tab, file: file, line: line),
      let fixture = TerminalViewFixture(file: file, line: line)
    else { return nil }
    guard restoreSaved(fixture, file: file, line: line) else {
      fixture.shutdown()
      return nil
    }
    return fixture
  }

  /// Wakes until the restored drawer is bound and its window shows a session in a hosted
  /// terminal view, and returns that window and view; `nil`, with the test failed, when it
  /// does not.
  private func awaitDrawer(
    _ fixture: TerminalViewFixture, file: StaticString = #filePath, line: UInt = #line
  ) -> (win: Int64, view: TerminalPaneView)? {
    let container = fixture.container
    var found: (win: Int64, view: TerminalPaneView)?
    let opened = fixture.wake {
      guard container.drawerPane != nil else { return false }
      for view in fixture.terminalViews {
        let win = fixture.window(showing: view.session)
        if win != 0, container.visiblePaneFrames[win] != nil {
          found = (win, view)
          return true
        }
      }
      return false
    }
    XCTAssertTrue(
      opened,
      "the saved open drawer must restore open, with a shell in a terminal view: drawer pane"
        + " \(String(describing: container.drawerPane)), terminal views"
        + " \(fixture.terminalViews.map { $0.frame }). A nil drawer pane means the restore never"
        + " opened the drawer", file: file, line: line)
    return found
  }

  /// The window showing the file named `name`, or 0.
  private func window(_ fixture: TerminalViewFixture, showing name: String) -> Int64 {
    let handle = fixture.handle
    return handle.windows().map { $0.win() }.first {
      handle.win_buffer($0).abs_path().toString().hasSuffix("/" + name)
    } ?? 0
  }

  /// Runs render wakes for `settleSeconds`, calling `sample` after each.
  private func settle(_ fixture: TerminalViewFixture, _ sample: () -> Void) {
    fixture.live.waitFor(timeout: settleSeconds) {
      fixture.container.onRenderWake()
      sample()
      return false
    }
  }
}
