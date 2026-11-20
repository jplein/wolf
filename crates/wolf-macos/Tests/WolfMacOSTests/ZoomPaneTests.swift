// Zoom Pane through PaneContainerView.toggleZoom, over a live core, Neovim and /bin/sh.

import AppKit
import WebKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// The palette's Zoom Pane row reaches `PaneContainerView.toggleZoom()` through
/// `WolfWindowController.zoomPane()`. No palette or key window renders here, so each test calls
/// `toggleZoom()` and `toggleTerminal()` over a real core, Neovim and shell, and reads the panes
/// the view shows from `visiblePaneFrames`, keyed by window handle.
///
/// Most tests start from the same layout, `vertical[A, horizontal[B, C]]` with C a `/bin/sh`
/// terminal pane and the drawer D open below, built by `buildLayout(_:)`.
///
/// A hidden pane's sizes are compared only after a settle, because a wrong resize reaches the
/// grid or the session a round trip later. The inert-action tests sample every wake, because the
/// view sends Neovim back to the zoomed pane within a round trip, which would hide a wrong focus
/// move from a check made only at the end.
///
/// A failure prefixed `premise:` is a setup step that did not happen.
@MainActor
final class ZoomPaneTests: XCTestCase {

  /// The panes of `buildLayout(_:)` by window handle, with the sessions of C and D.
  private struct Layout {
    let wA: Int64
    let wB: Int64
    let wC: Int64
    let wD: Int64
    let sC: UInt64
    let sD: UInt64
  }

  /// A grid's or a session's size in cells.
  private struct CellSize: Equatable, CustomStringConvertible {
    let rows: Int
    let cols: Int
    var description: String { "\(rows) rows x \(cols) cols" }
  }

  /// Within half a point: the tree's rects round to the backing pixel.
  private let accuracy: CGFloat = 0.5

  // MARK: - Z1: zooming

  /// Zooming a terminal pane gives it the whole pane area and grows its session, while every
  /// other pane keeps its window, its session and its last size.
  func testZoomingATerminalPaneFillsTheAreaAndKeepsTheOthers() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture), focus(fixture, l.wC) else { return }

    let gA = gridSize(fixture, l.wA)
    let gB = gridSize(fixture, l.wB)
    let sizeC = sessionSize(fixture, l.sC)
    let sizeD = sessionSize(fixture, l.sD)
    let heightC = fixture.view(showing: l.sC)?.frame.height ?? 0

    container.toggleZoom()
    XCTAssertTrue(container.isZoomed, "toggleZoom() over four panes must zoom the view")
    XCTAssertEqual(
      container.visiblePaneFrames, [l.wC: container.bounds],
      "the zoomed pane C (\(l.wC)) must be the only pane shown, over the whole area"
        + " \(container.bounds). Four frames mean the other panes still show; C at its own rect"
        + " means the zoomed pane keeps its layout rect")

    var grownC = sizeC
    let grew = fixture.wake {
      grownC = sessionSize(fixture, l.sC)
      return grownC.rows > sizeC.rows && grownC.cols > sizeC.cols
        && (fixture.view(showing: l.sC)?.frame.height ?? 0) > heightC
    }
    XCTAssertTrue(
      grew,
      "the zoomed terminal's session must grow in rows and columns from \(sizeC), and its view"
        + " from \(heightC) pt tall: the session is \(grownC), the view"
        + " \(String(describing: fixture.view(showing: l.sC)?.frame)). An unchanged session"
        + " means the session sizes still follow the unzoomed layout")

    settle(fixture, 2)
    let windows = handle.windows().map { $0.win() }
    for (name, win) in [("A", l.wA), ("B", l.wB), ("D", l.wD)] {
      XCTAssertTrue(
        windows.contains(win),
        "the hidden pane \(name)'s window \(win) must live on: windows \(windows). Gone means"
          + " zooming closed the other windows, as :only would")
    }
    let live = Array(handle.live_terminals())
    XCTAssertTrue(
      live.contains(l.sC) && live.contains(l.sD),
      "both sessions, \(l.sC) zoomed and \(l.sD) hidden, must stay live: \(live)")
    XCTAssertEqual(
      gridSize(fixture, l.wA), gA,
      "the hidden pane A's grid must keep its last size; another size means a hidden pane was"
        + " sized to the zoomed rect or re-divided")
    XCTAssertEqual(
      gridSize(fixture, l.wB), gB,
      "the hidden pane B's grid must keep its last size; another size means a hidden pane was"
        + " sized to the zoomed rect or re-divided")
    XCTAssertEqual(
      sessionSize(fixture, l.sD), sizeD,
      "the hidden drawer's session must keep its last size, so its shell gets no SIGWINCH;"
        + " another size means hidden sessions are still sized")
    let viewD = fixture.view(showing: l.sD)
    XCTAssertTrue(
      viewD?.isHidden ?? true,
      "the hidden drawer's terminal view must be hidden or gone, not shown at"
        + " \(String(describing: viewD?.frame)) over the zoomed pane")
    XCTAssertEqual(
      fixture.view(showing: l.sC)?.isHidden, false,
      "the zoomed pane's terminal view must be hosted and shown")
    XCTAssertEqual(
      container.otherPaneTarget(prefer: .right), 0,
      "with tree pane C zoomed, an other-pane command must take its split path (0). A window"
        + " handle means it binds into a hidden pane; nil that it does nothing")
  }

  // MARK: - Z2: unzooming

  /// Unzooming restores every rectangle and weight, the dragged divider and drawer share
  /// included, though the hidden divider and drawer edge were dragged and the window resized
  /// while zoomed.
  func testUnzoomRestoresEveryRectangleAndWeight() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    guard let l = buildLayout(fixture) else { return }

    let start = container.visiblePaneFrames
    guard let a = start[l.wA], let d = start[l.wD] else {
      XCTFail("premise: the layout must show A and D: \(start)")
      return
    }
    // Clear of B and C's divider, which lies at half the editor height.
    let dividerY = a.minY + a.height / 4
    guard drag(fixture, from: CGPoint(x: a.maxX, y: dividerY), by: CGVector(dx: -160, dy: 0)),
      drag(
        fixture, from: CGPoint(x: container.bounds.midX, y: d.minY), by: CGVector(dx: 0, dy: -80))
    else { return }
    let dragged = container.visiblePaneFrames
    let movedA = (dragged[l.wA]?.width ?? a.width) <= a.width - 100
    let movedD = (dragged[l.wD]?.minY ?? d.minY) <= d.minY - 50
    XCTAssertTrue(
      movedA && movedD,
      "premise: the drags must narrow A by 160 pt and raise the drawer's top by 80 pt, so the"
        + " weights differ from the defaults: from \(start) to \(dragged)")
    guard movedA, movedD else { return }

    settle(fixture, 1)
    let frames0 = container.visiblePaneFrames
    let layout0 = container.paneLayout
    let gA0 = gridSize(fixture, l.wA)
    guard let a0 = frames0[l.wA], let d0 = frames0[l.wD], focus(fixture, l.wA) else { return }

    container.toggleZoom()
    var gA = gA0
    let grew = fixture.wake {
      gA = gridSize(fixture, l.wA)
      return gA.rows > gA0.rows && gA.cols > gA0.cols
    }
    XCTAssertTrue(
      grew,
      "the zoomed pane A's grid must grow in rows and columns from \(gA0); it is \(gA)."
        + " Unchanged means the grid sizes still follow the unzoomed layout")
    XCTAssertEqual(
      container.paneLayout, layout0,
      "while zoomed, paneLayout must still be the unzoomed tree, which state.json records; the"
        + " zoomed pane alone means a relaunch would lose the other panes")

    // Both presses land in the zoomed pane: a hidden divider or edge must not take them.
    guard
      drag(fixture, from: CGPoint(x: a0.maxX, y: dividerY), by: CGVector(dx: 120, dy: 0)),
      drag(
        fixture, from: CGPoint(x: container.bounds.midX, y: d0.minY), by: CGVector(dx: 0, dy: -60))
    else { return }

    fixture.renderer.window.setContentSize(NSSize(width: 1200, height: 760))
    container.onRenderWake()
    XCTAssertEqual(
      container.bounds.size, NSSize(width: 1200, height: 760),
      "premise: the container must follow the window to 1200 x 760")
    XCTAssertEqual(
      container.visiblePaneFrames, [l.wA: container.bounds],
      "after a resize while zoomed, the zoomed pane A must follow the area to"
        + " \(container.bounds)")

    container.toggleZoom()
    let resized = container.visiblePaneFrames
    XCTAssertFalse(container.isZoomed, "a second toggleZoom() must unzoom")
    XCTAssertEqual(
      Set(resized.keys), [l.wA, l.wB, l.wC, l.wD],
      "unzoomed, all four panes must show again: \(resized)")
    let area = resized.values.reduce(CGFloat(0)) { $0 + $1.width * $1.height }
    let fitted =
      resized.values.allSatisfy {
        container.bounds.insetBy(dx: -accuracy, dy: -accuracy).contains($0)
      }
    XCTAssertTrue(fitted, "every restored pane must lie inside \(container.bounds): \(resized)")
    XCTAssertEqual(
      resized.values.map { $0.maxX }.max() ?? 0, 1200, accuracy: accuracy,
      "the restored layout must reach the resized width: \(resized)")
    XCTAssertEqual(
      resized.values.map { $0.maxY }.max() ?? 0, 760, accuracy: accuracy,
      "the restored layout must reach the resized height: \(resized)")
    XCTAssertEqual(
      area, 1200 * 760, accuracy: 2,
      "the restored panes must tile the resized area with no gap or overlap: \(resized)")

    fixture.renderer.window.setContentSize(TerminalViewFixture.windowSize)
    container.onRenderWake()
    XCTAssertEqual(
      container.visiblePaneFrames, frames0,
      "back at the first size, every pane must be where it was before the zoom. A drawer at"
        + " the opening third, or A at half the width, means unzooming reset a dragged weight; a"
        + " wider A or a taller drawer means a drag while zoomed reached a hidden divider or edge")
    XCTAssertEqual(
      container.paneLayout, layout0,
      "back at the first size, the tree and its weights must equal those before the zoom")
    var gAEnd = gridSize(fixture, l.wA)
    let restored = fixture.wake {
      gAEnd = gridSize(fixture, l.wA)
      return gAEnd == gA0
    }
    XCTAssertTrue(
      restored,
      "after unzooming, A's grid must return to its unzoomed \(gA0); it is \(gAEnd)")
  }

  // MARK: - Z3 to Z6: what unzooms

  /// A split while zoomed unzooms, then splits the zoomed pane in place.
  func testASplitWhileZoomedUnzoomsThenSplits() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    guard let l = buildLayout(fixture) else { return }
    let frames0 = container.visiblePaneFrames
    guard let a0 = frames0[l.wA], zoom(fixture, on: l.wA) else { return }

    container.splitActivePane(.horizontal)
    let split = fixture.wake { container.paneCount == 4 }
    XCTAssertTrue(
      split, "premise: splitActivePane(.horizontal) must add a fourth tree pane")
    guard split else { return }

    XCTAssertFalse(
      container.isZoomed,
      "a split while zoomed must unzoom; zoomed means the new pane is hidden behind A")
    let frames = container.visiblePaneFrames
    XCTAssertEqual(frames.count, 5, "the four tree panes and the drawer must show: \(frames)")
    let known: Set<Int64> = [l.wA, l.wB, l.wC, l.wD]
    guard let new = frames.keys.first(where: { !known.contains($0) }), let n = frames[new],
      let a = frames[l.wA]
    else {
      XCTFail("the split's new pane and A must both show: \(frames)")
      return
    }
    let top = a.minY <= n.minY ? a : n
    let bottom = a.minY <= n.minY ? n : a
    for (name, rect) in [("A", a), ("the new pane", n)] {
      XCTAssertEqual(
        rect.minX, a0.minX, accuracy: accuracy,
        "\(name) must keep A's unzoomed left edge: \(rect) against \(a0)")
      XCTAssertEqual(
        rect.width, a0.width, accuracy: accuracy,
        "\(name) must keep A's unzoomed width: \(rect) against \(a0). The whole width means"
          + " the split was laid out over the zoomed rect")
    }
    XCTAssertEqual(top.minY, a0.minY, accuracy: accuracy, "the stack must start at A's top")
    XCTAssertEqual(
      top.maxY, bottom.minY, accuracy: accuracy, "A and the new pane must stack with no gap")
    XCTAssertEqual(
      bottom.maxY, a0.maxY, accuracy: accuracy,
      "A and the new pane must span A's unzoomed height \(a0): \(top), \(bottom)")
    for (name, win) in [("B", l.wB), ("C", l.wC), ("D", l.wD)] {
      assertRect(frames[win], frames0[win], "\(name) must be where it was before the zoom")
    }
  }

  /// Closing the zoomed pane with Close Pane unzooms, and its sibling takes its place.
  func testClosingTheZoomedPaneUnzooms() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture) else { return }
    let frames0 = container.visiblePaneFrames
    guard let b0 = frames0[l.wB], let c0 = frames0[l.wC], zoom(fixture, on: l.wB) else { return }

    container.closeActivePane()
    let closed = fixture.wake {
      container.paneCount == 2 && !handle.windows().contains { $0.win() == l.wB }
        && container.visiblePaneFrames[l.wB] == nil
    }
    XCTAssertTrue(
      closed,
      "premise: closeActivePane() must close B: \(container.paneCount) tree pane(s), frames"
        + " \(container.visiblePaneFrames)")
    guard closed else { return }

    XCTAssertFalse(
      container.isZoomed,
      "closing the zoomed pane must unzoom; zoomed means the view holds a pane that is gone")
    let frames = container.visiblePaneFrames
    XCTAssertEqual(
      Set(frames.keys), [l.wA, l.wC, l.wD], "A, C and the drawer must show: \(frames)")
    assertRect(frames[l.wA], frames0[l.wA], "A must be where it was before the zoom")
    assertRect(frames[l.wD], frames0[l.wD], "the drawer must be where it was before the zoom")
    assertRect(frames[l.wC], b0.union(c0), "C must take the closed B's place above it")
  }

  /// The zoomed terminal's shell exiting closes its pane and unzooms.
  func testTheZoomedTerminalsShellExitingUnzooms() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture) else { return }
    let frames0 = container.visiblePaneFrames
    guard let b0 = frames0[l.wB], let c0 = frames0[l.wC], zoom(fixture, on: l.wC) else { return }

    fixture.runLine("exit", in: l.sC)
    let closed = fixture.wake {
      container.paneCount == 2 && !handle.windows().contains { $0.win() == l.wC }
        && container.visiblePaneFrames[l.wC] == nil
    }
    XCTAssertTrue(
      closed,
      "premise: the shell's exit must close C's window: \(container.paneCount) tree pane(s),"
        + " frames \(container.visiblePaneFrames)")
    guard closed else { return }

    XCTAssertFalse(
      container.isZoomed,
      "the zoomed pane closing by its shell's exit must unzoom; zoomed means the view holds a"
        + " pane that is gone")
    let frames = container.visiblePaneFrames
    XCTAssertEqual(
      Set(frames.keys), [l.wA, l.wB, l.wD], "A, B and the drawer must show: \(frames)")
    assertRect(frames[l.wB], b0.union(c0), "B must take the closed C's place below it")
  }

  /// Toggle Terminal on a zoomed drawer closes it, which unzooms, and keeps its session.
  func testToggleTerminalOnAZoomedDrawerClosesItAndUnzooms() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture) else { return }

    container.toggleZoom()
    XCTAssertTrue(
      container.isZoomed,
      "toggleZoom() with the drawer active must zoom the drawer; unzoomed means the drawer is"
        + " not among the panes a zoom considers")
    XCTAssertEqual(
      container.visiblePaneFrames, [l.wD: container.bounds],
      "the zoomed drawer must be the only pane shown, over the whole area")
    guard container.isZoomed else { return }

    container.toggleTerminal()
    let closed = fixture.wake {
      !handle.windows().contains { $0.win() == l.wD } && container.visiblePaneFrames[l.wD] == nil
    }
    XCTAssertTrue(
      closed,
      "toggleTerminal() on the zoomed drawer must close it: windows"
        + " \(handle.windows().map { $0.win() }). Still open means a zoomed drawer refuses its"
        + " own toggle")
    guard closed else { return }

    XCTAssertFalse(container.isZoomed, "closing the zoomed drawer must unzoom")
    let frames = container.visiblePaneFrames
    XCTAssertEqual(
      Set(frames.keys), [l.wA, l.wB, l.wC], "the three tree panes must show: \(frames)")
    XCTAssertEqual(
      frames.values.map { $0.maxY }.max() ?? 0, container.bounds.height, accuracy: accuracy,
      "with the drawer closed, the tree must reach the bottom of the area: \(frames)")
    XCTAssertTrue(
      Array(handle.live_terminals()).contains(l.sD),
      "the drawer's session must outlive its toggle: \(Array(handle.live_terminals()))")
  }

  // MARK: - Z7 to Z9: what is inert while zoomed

  /// Toggle Terminal while a tree pane is zoomed does nothing.
  func testToggleTerminalWhileAnotherPaneIsZoomedDoesNothing() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture), zoom(fixture, on: l.wA) else { return }
    let live = Array(handle.live_terminals())

    container.toggleTerminal()
    var lost = 0
    settle(fixture, 1.5) {
      if !handle.windows().contains(where: { $0.win() == l.wD }) { lost += 1 }
    }
    XCTAssertEqual(
      lost, 0,
      "Toggle Terminal while A is zoomed must leave the hidden drawer open; its window \(l.wD)"
        + " was gone at \(lost) wake(s), so the toggle reached the drawer")
    XCTAssertTrue(container.isZoomed, "Toggle Terminal while A is zoomed must keep the zoom")
    XCTAssertEqual(
      container.visiblePaneFrames, [l.wA: container.bounds], "A must still be the only pane shown")
    XCTAssertEqual(
      Array(handle.live_terminals()), live,
      "Toggle Terminal while A is zoomed must start or end no session")
  }

  /// A focus move while zoomed changes nothing, in Neovim or in the view.
  func testAFocusMoveWhileZoomedChangesNothing() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture), zoom(fixture, on: l.wA) else { return }

    fixture.neovimChord("l")
    var moved: [String] = []
    settle(fixture, 1.5) {
      let frames = container.visiblePaneFrames
      if handle.active_win() != l.wA || container.activeWin != l.wA
        || frames != [l.wA: container.bounds]
      {
        moved.append(
          "Neovim \(handle.active_win()), view \(container.activeWin), frames \(frames)")
      }
    }
    XCTAssertEqual(
      moved, [],
      "<C-w>l while A (\(l.wA)) is zoomed must move no focus, in Neovim or the view, at any"
        + " wake; it moved to a hidden pane at these wakes")
    XCTAssertTrue(container.isZoomed, "a focus move while zoomed must keep the zoom")
  }

  /// Neovim leaving the zoomed pane by a path the view does not intercept is sent back.
  func testNeovimLeavingTheZoomedPaneIsSentBack() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture), zoom(fixture, on: l.wA) else { return }

    fixture.typeEx("wincmd w")
    var left: [String] = []
    settle(fixture, 2) {
      if container.activeWin != l.wA || !container.isZoomed {
        left.append("view \(container.activeWin), zoomed \(container.isZoomed)")
      }
    }
    XCTAssertEqual(
      left, [],
      ":wincmd w while A (\(l.wA)) is zoomed must leave A the view's active pane and the view"
        + " zoomed at every wake; a hidden pane became active at these wakes")
    XCTAssertEqual(
      handle.active_win(), l.wA,
      "Neovim must be sent back to the zoomed pane A; another window means the user types into"
        + " a pane they cannot see")
  }

  // MARK: - Z10: a hidden preview

  /// A preview hidden by a zoom keeps its web view, hidden, and shows it again on unzoom.
  func testAHiddenPreviewKeepsItsWebViewHidden() {
    guard let fixture = PreviewViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    let web = fixture.web
    guard
      let source = container.visiblePaneFrames.keys.first(where: { $0 != fixture.previewWin })
    else {
      XCTFail("premise: a source pane must show beside the preview: \(container.visiblePaneFrames)")
      return
    }
    guard focus(fixture.base, source) else { return }
    XCTAssertFalse(web.isHidden, "premise: the unzoomed preview's web view must show")

    container.toggleZoom()
    container.onRenderWake()
    XCTAssertTrue(
      container.isZoomed && container.visiblePaneFrames == [source: container.bounds],
      "premise: toggleZoom() on the source must show it alone: \(container.visiblePaneFrames)")
    XCTAssertTrue(
      web.superview === container,
      "the hidden preview's web view must stay hosted; removed means it reloads, losing its"
        + " scroll, on unzoom")
    XCTAssertTrue(
      web.isHidden,
      "the hidden preview's web view must be hidden; shown, it paints over the zoomed pane")

    container.toggleZoom()
    container.onRenderWake()
    XCTAssertTrue(
      web.superview === container,
      "after unzooming, the preview must show in the same web view")
    XCTAssertFalse(web.isHidden, "after unzooming, the preview's web view must show again")
  }

  // MARK: - Helpers

  /// Builds `vertical[A, horizontal[B, C]]` with C on `exec /bin/sh` and the drawer D open and
  /// active, then settles for a second; `nil`, with the test failed, when a step does not happen.
  private func buildLayout(
    _ fixture: TerminalViewFixture, file: StaticString = #filePath, line: UInt = #line
  ) -> Layout? {
    let handle = fixture.handle
    let container = fixture.container
    let wA = container.activeWin

    container.splitActivePane(.vertical)
    guard
      fixture.wake(until: { container.paneCount == 2 && ![0, wA].contains(container.activeWin) })
    else {
      XCTFail(
        "premise: a vertical split must add an active pane B: \(container.paneCount) pane(s)",
        file: file, line: line)
      return nil
    }
    let wB = container.activeWin

    container.splitActivePane(.horizontal)
    guard
      fixture.wake(until: {
        container.paneCount == 3 && ![0, wA, wB].contains(container.activeWin)
      })
    else {
      XCTFail(
        "premise: a horizontal split of B must add an active pane C: \(container.paneCount)"
          + " pane(s)", file: file, line: line)
      return nil
    }
    let wC = container.activeWin

    let sC = fixture.live.openTerminal("exec /bin/sh")
    guard sC != 0,
      fixture.wake(until: {
        container.activeTerminal == sC && fixture.view(showing: sC) != nil
          && fixture.window(showing: sC) == wC
      })
    else {
      XCTFail(
        "premise: C (\(wC)) must show an active /bin/sh session: session \(sC), active terminal"
          + " \(String(describing: container.activeTerminal))", file: file, line: line)
      return nil
    }

    handle.toggle_terminal()
    var sD: UInt64 = 0
    var wD: Int64 = 0
    let opened = fixture.wake {
      sD = Array(handle.live_terminals()).first { $0 != sC } ?? 0
      guard sD != 0, let view = fixture.view(showing: sD), fixture.isInDrawerStrip(view)
      else { return false }
      wD = fixture.window(showing: sD)
      return wD != 0 && container.activeWin == wD && container.visiblePaneFrames.count == 4
    }
    guard opened else {
      XCTFail(
        "premise: toggle_terminal() must open the drawer D, active, as a fourth pane: live"
          + " \(Array(handle.live_terminals())), active window \(container.activeWin), frames"
          + " \(container.visiblePaneFrames)", file: file, line: line)
      return nil
    }
    settle(fixture, 1)
    return Layout(wA: wA, wB: wB, wC: wC, wD: wD, sC: sC, sD: sD)
  }

  /// Focuses `win` in Neovim and wakes until the view follows; `false`, with the test failed,
  /// when it does not.
  private func focus(
    _ fixture: TerminalViewFixture, _ win: Int64, file: StaticString = #filePath,
    line: UInt = #line
  ) -> Bool {
    let handle = fixture.handle
    let container = fixture.container
    handle.focus_win(win)
    let focused = fixture.wake { container.activeWin == win && handle.active_win() == win }
    XCTAssertTrue(
      focused,
      "premise: focus_win(\(win)) must make it active in Neovim and the view: Neovim"
        + " \(handle.active_win()), view \(container.activeWin)", file: file, line: line)
    return focused
  }

  /// Focuses `win` and zooms it, failing the test unless it then shows alone over the area.
  private func zoom(
    _ fixture: TerminalViewFixture, on win: Int64, file: StaticString = #filePath,
    line: UInt = #line
  ) -> Bool {
    let container = fixture.container
    guard focus(fixture, win, file: file, line: line) else { return false }
    container.toggleZoom()
    let zoomed = container.isZoomed && container.visiblePaneFrames == [win: container.bounds]
    XCTAssertTrue(
      zoomed,
      "premise: toggleZoom() must show \(win) alone over \(container.bounds): zoomed"
        + " \(container.isZoomed), frames \(container.visiblePaneFrames)", file: file, line: line)
    return zoomed
  }

  /// The size of `win`'s grid, or zero cells when it has none.
  private func gridSize(_ fixture: TerminalViewFixture, _ win: Int64) -> CellSize {
    let grid = fixture.handle.grid_for_win(win)
    guard grid != 0 else { return CellSize(rows: 0, cols: 0) }
    let frame = fixture.handle.read_frame_for_grid(grid)
    return CellSize(rows: Int(frame.rows()), cols: Int(frame.cols()))
  }

  /// The size of `session`'s frame.
  private func sessionSize(_ fixture: TerminalViewFixture, _ session: UInt64) -> CellSize {
    let frame = fixture.handle.terminal_frame(session)
    return CellSize(rows: Int(frame.rows()), cols: Int(frame.cols()))
  }

  /// Runs render wakes for `seconds`, calling `sample` after each.
  private func settle(
    _ fixture: TerminalViewFixture, _ seconds: TimeInterval, _ sample: () -> Void = {}
  ) {
    fixture.live.waitFor(timeout: seconds) {
      fixture.container.onRenderWake()
      sample()
      return false
    }
  }

  /// Asserts `got` is `want` within `accuracy` on every edge.
  private func assertRect(
    _ got: CGRect?, _ want: CGRect?, _ message: String, file: StaticString = #filePath,
    line: UInt = #line
  ) {
    guard let got, let want else {
      XCTFail(
        "\(message): got \(String(describing: got)), want \(String(describing: want))",
        file: file, line: line)
      return
    }
    let close =
      abs(got.minX - want.minX) <= accuracy && abs(got.minY - want.minY) <= accuracy
      && abs(got.width - want.width) <= accuracy && abs(got.height - want.height) <= accuracy
    XCTAssertTrue(close, "\(message): got \(got), want \(want)", file: file, line: line)
  }

  /// A left-button mouse event of `type` at `point` in the container's coordinates, as the
  /// window delivers one; `nil`, with the test failed, when AppKit cannot make it.
  private func mouseEvent(
    _ type: NSEvent.EventType, at point: CGPoint, in fixture: TerminalViewFixture
  ) -> NSEvent? {
    let event = NSEvent.mouseEvent(
      with: type, location: fixture.container.convert(point, to: nil), modifierFlags: [],
      timestamp: ProcessInfo.processInfo.systemUptime,
      windowNumber: fixture.renderer.window.windowNumber, context: nil, eventNumber: 0,
      clickCount: 1, pressure: type == .leftMouseUp ? 0 : 1)
    if event == nil {
      XCTFail("premise: could not synthesize a \(type) event")
    }
    return event
  }

  /// Presses at `start`, drags by `offset` and releases there, through the container's
  /// mouse handlers.
  private func drag(_ fixture: TerminalViewFixture, from start: CGPoint, by offset: CGVector)
    -> Bool
  {
    let end = CGPoint(x: start.x + offset.dx, y: start.y + offset.dy)
    guard let down = mouseEvent(.leftMouseDown, at: start, in: fixture),
      let dragged = mouseEvent(.leftMouseDragged, at: end, in: fixture),
      let up = mouseEvent(.leftMouseUp, at: end, in: fixture)
    else { return false }
    fixture.container.mouseDown(with: down)
    fixture.container.mouseDragged(with: dragged)
    fixture.container.mouseUp(with: up)
    return true
  }
}
