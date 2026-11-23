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
/// grid or the session a round trip later. A move that must change nothing is sampled at every
/// wake, and so is a restored zoom, because Neovim reporting another pane a round trip later
/// unzooms the view, which a check made only at the start would miss.
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
      "while zoomed, paneLayout must still be the unzoomed tree, which the window state records;"
        + " the zoomed pane alone means a relaunch would lose the other panes")

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

  // MARK: - Z7 to Z9: Toggle Terminal and focus moves while zoomed

  /// Toggle Terminal while a tree pane is zoomed unzooms, then closes the open, hidden drawer
  /// as it would unzoomed.
  func testToggleTerminalWhileAPaneIsZoomedUnzoomsAndClosesTheOpenDrawer() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture), zoom(fixture, on: l.wA) else { return }

    container.toggleTerminal()
    XCTAssertFalse(
      container.isZoomed,
      "toggleTerminal() while A is zoomed must unzoom at once; zoomed means the toggle reaches"
        + " a drawer the user cannot see, or does nothing")
    let closed = fixture.wake {
      !handle.windows().contains { $0.win() == l.wD } && container.visiblePaneFrames[l.wD] == nil
    }
    XCTAssertTrue(
      closed,
      "toggleTerminal() while A is zoomed must close the open drawer, as unzoomed: windows"
        + " \(handle.windows().map { $0.win() }), frames \(container.visiblePaneFrames)")
    guard closed else { return }

    let frames = container.visiblePaneFrames
    XCTAssertEqual(
      Set(frames.keys), [l.wA, l.wB, l.wC],
      "with the drawer closed, the three tree panes must show: \(frames). A alone means the"
        + " view stayed zoomed, as closing a hidden pane keeps a zoom")
    XCTAssertFalse(container.isZoomed, "the view must stay unzoomed once the drawer is closed")
    XCTAssertEqual(
      frames.values.map { $0.maxY }.max() ?? 0, container.bounds.height, accuracy: accuracy,
      "with the drawer closed, the tree must reach the bottom of the area: \(frames)")
    XCTAssertTrue(
      Array(handle.live_terminals()).contains(l.sD),
      "the drawer's session must outlive its toggle: \(Array(handle.live_terminals()))")
  }

  /// A focus move with no target in the unzoomed layout changes nothing, in Neovim or in the
  /// view, at any wake.
  func testAFocusMoveWithNoTargetWhileZoomedChangesNothing() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    // A is the leftmost pane, so <C-w>h from it has no target.
    guard let l = buildLayout(fixture), zoom(fixture, on: l.wA) else { return }

    fixture.neovimChord("h")
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
      "<C-w>h from the leftmost pane A (\(l.wA)) while zoomed has no target, so it must keep A"
        + " zoomed and focused, in Neovim and the view, at every wake. These wakes differ:"
        + " every pane in the frames means the move unzoomed without a target")
    XCTAssertTrue(container.isZoomed, "a focus move with no target must keep the zoom")
  }

  /// Neovim leaving the zoomed pane by a path the view does not intercept unzooms, and the
  /// view follows Neovim to the pane it entered.
  func testNeovimLeavingTheZoomedPaneUnzoomsAndIsFollowed() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture), zoom(fixture, on: l.wA) else { return }

    fixture.typeEx("wincmd w")
    let followed = fixture.wake {
      !container.isZoomed && container.activeWin == handle.active_win()
        && container.activeWin != l.wA && container.activeWin != 0
    }
    XCTAssertTrue(
      followed,
      ":wincmd w while A (\(l.wA)) is zoomed must unzoom and leave the view on Neovim's new"
        + " window: zoomed \(container.isZoomed), view \(container.activeWin), Neovim"
        + " \(handle.active_win()). Still zoomed means the view ignores Neovim's move; the view"
        + " still on A, that it sends Neovim back")
    XCTAssertEqual(
      Set(container.visiblePaneFrames.keys), [l.wA, l.wB, l.wC, l.wD],
      "unzoomed, all four panes must show: \(container.visiblePaneFrames)")
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

  // MARK: - Z11, Z12: a focus chord and Toggle Terminal unzoom

  /// A focus chord while zoomed unzooms, restores every rectangle and weight, and focuses the
  /// pane the same chord reaches unzoomed.
  func testAFocusChordWhileZoomedUnzoomsRestoresTheLayoutAndFocusesTheNeighbour() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture) else { return }

    // A dragged divider, so a restore to the default weights cannot pass.
    guard let a = container.visiblePaneFrames[l.wA],
      drag(
        fixture, from: CGPoint(x: a.maxX, y: a.minY + a.height / 4),
        by: CGVector(dx: -160, dy: 0))
    else { return }
    let movedA = (container.visiblePaneFrames[l.wA]?.width ?? a.width) <= a.width - 100
    XCTAssertTrue(
      movedA,
      "premise: the drag must narrow A by 160 pt: from \(a) to"
        + " \(String(describing: container.visiblePaneFrames[l.wA]))")
    guard movedA, focus(fixture, l.wA) else { return }

    fixture.neovimChord("l")
    var target: Int64 = 0
    let reached = fixture.wake {
      target = handle.active_win()
      return target != l.wA && target != 0 && container.activeWin == target
    }
    XCTAssertTrue(
      reached,
      "premise: <C-w>l from A unzoomed must focus a pane right of A: Neovim"
        + " \(handle.active_win()), view \(container.activeWin)")
    guard reached, focus(fixture, l.wA) else { return }
    settle(fixture, 1)
    let frames0 = container.visiblePaneFrames
    let layout0 = container.paneLayout

    guard zoom(fixture, on: l.wA) else { return }
    fixture.neovimChord("l")
    let unzoomed = fixture.wake {
      !container.isZoomed && container.activeWin == target && handle.active_win() == target
    }
    XCTAssertTrue(
      unzoomed,
      "<C-w>l while A is zoomed must unzoom and focus \(target), the pane it reaches unzoomed:"
        + " zoomed \(container.isZoomed), view \(container.activeWin), Neovim"
        + " \(handle.active_win()). Still zoomed on A means a zoom swallows focus moves; another"
        + " window means the move was resolved from elsewhere than A")
    guard unzoomed else { return }

    let frames = container.visiblePaneFrames
    XCTAssertEqual(
      Set(frames.keys), Set(frames0.keys), "every pane must show again: \(frames)")
    for (win, rect) in frames0 {
      assertRect(
        frames[win], rect,
        "pane \(win) must be where it was before the zoom; A at half the width means the unzoom"
          + " reset the dragged weight")
    }
    XCTAssertEqual(
      container.paneLayout, layout0,
      "the tree and its weights must equal those before the zoom")
  }

  /// Toggle Terminal while zoomed with no drawer unzooms, then opens the drawer and focuses it,
  /// as it would unzoomed.
  func testToggleTerminalWhileZoomedUnzoomsAndOpensAndFocusesTheDrawer() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let wA = container.activeWin
    container.splitActivePane(.vertical)
    let split = fixture.wake {
      container.paneCount == 2 && ![0, wA].contains(container.activeWin)
    }
    XCTAssertTrue(split, "premise: a vertical split must add an active pane B")
    guard split else { return }
    let wB = container.activeWin
    let live0 = Set(handle.live_terminals())
    XCTAssertTrue(live0.isEmpty, "premise: no session may run before the toggle: \(live0)")
    guard zoom(fixture, on: wA) else { return }

    container.toggleTerminal()
    XCTAssertFalse(
      container.isZoomed,
      "toggleTerminal() while A is zoomed must unzoom at once; zoomed means it is refused, or"
        + " leaves the unzoom to the drawer's arrival")
    var sD: UInt64 = 0
    var wD: Int64 = 0
    let opened = fixture.wake {
      sD = Array(handle.live_terminals()).first { !live0.contains($0) } ?? 0
      guard sD != 0, let view = fixture.view(showing: sD), fixture.isInDrawerStrip(view)
      else { return false }
      wD = fixture.window(showing: sD)
      return wD != 0 && container.activeWin == wD && handle.active_win() == wD
    }
    XCTAssertTrue(
      opened,
      "toggleTerminal() while A is zoomed must open the drawer and focus it, in the view and in"
        + " Neovim: session \(sD), window \(wD), view \(container.activeWin), Neovim"
        + " \(handle.active_win()). No session means a tree pane's zoom refuses the toggle")
    guard opened else { return }
    XCTAssertFalse(container.isZoomed, "the view must stay unzoomed with the drawer open")
    XCTAssertEqual(
      Set(container.visiblePaneFrames.keys), [wA, wB, wD],
      "A, B and the drawer must all show: \(container.visiblePaneFrames)")
  }

  // MARK: - Z13: a save and restore of the layout

  /// A zoom survives the production save and restore: the zoomed leaf is gathered, submitted
  /// through `WindowStateCodec` into the window state, read back and restored, rebuilt in a
  /// second core through `rebuild_tab` and re-zoomed, with Neovim's current window on it and the
  /// saved weights behind it. `restored()` replaces at most A, whose startup buffer is no file
  /// on disk, in place, so B keeps index 1 and its zoom.
  func testZoomSurvivesASaveAndRestoreOfTheLayout() {
    let base =
      ProcessInfo.processInfo.environment["TEST_TMPDIR"].flatMap { $0.isEmpty ? nil : $0 }
      ?? NSTemporaryDirectory()
    let dir = URL(fileURLWithPath: base)
      .appendingPathComponent("wolf-zoom-restore-\(UUID().uuidString)")
    let file = dir.appendingPathComponent("second-leaf.md")
    do {
      try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
      try "# Second leaf\n".write(to: file, atomically: true, encoding: .utf8)
    } catch {
      XCTFail("premise: could not write the fixture file \(file.path): \(error)")
      return
    }
    defer { try? FileManager.default.removeItem(at: dir) }
    let filePath = canonical(file.path)

    guard let saved = saveWithBZoomed(editing: file.path, cwd: dir.path) else { return }

    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let loaded = WindowStateCodec.load(wolf_ffi_app_state_window_state())
    guard let rebuild = loaded.last?.tabs.last?.rebuild else {
      XCTFail("the committed window state must load back with a rebuild on its last tab")
      return
    }
    container.requestLayoutRebuild(rebuild.layout, tab: rebuild.tab)
    let rebuilt = fixture.wake { container.paneCount == 3 }
    XCTAssertTrue(
      rebuilt, "premise: the restore must rebuild three tree panes: \(container.paneCount)")
    guard rebuilt else { return }

    var unzoomedAt: [String] = []
    settle(fixture, 1) {
      if !container.isZoomed { unzoomedAt.append("frames \(container.visiblePaneFrames)") }
    }
    XCTAssertEqual(
      unzoomedAt, [],
      "the restored view must be zoomed at every wake. Unzoomed at all means the zoom is lost"
        + " on the way through gatherTab, submit, the load or rebuildLayout; unzoomed after"
        + " a zoomed wake means Neovim reported another pane after the rebuild")
    let frames = container.visiblePaneFrames
    guard frames.count == 1, let zoomedFrame = frames.first else {
      XCTFail("the restored view must show one pane alone, the zoomed B: \(frames)")
      return
    }
    let win = zoomedFrame.key
    assertRect(zoomedFrame.value, container.bounds, "the zoomed pane must cover the whole area")
    let shown = canonical(handle.win_buffer(win).abs_path().toString())
    XCTAssertEqual(
      shown, filePath,
      "the zoomed pane must be B, the leaf showing \(filePath); another buffer means the mark"
        + " landed on another leaf")
    XCTAssertEqual(
      handle.active_win(), win,
      "Neovim's current window must be the zoomed pane's; another means the rebuild ends"
        + " elsewhere and the user types into a hidden pane")
    XCTAssertEqual(container.activeWin, win, "the view's active pane must be the zoomed one")

    container.toggleZoom()
    XCTAssertEqual(
      container.visiblePaneFrames.count, 3,
      "unzoomed, all three restored panes must show: \(container.visiblePaneFrames)")
    XCTAssertEqual(
      PaneLayoutCodec.encode(container.paneLayout, paths: [:], previews: []), saved,
      "unzoomed, the topology and weights must be those saved, the dragged divider included")
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

  /// Builds `vertical[A, horizontal[B, C]]` in a first core, drags A's divider, edits `path`
  /// into B and zooms it, then submits the tab through `WindowStateCodec` and flushes.
  /// Returns the layout's topology and weights, with no paths, after shutting the core down;
  /// `nil`, with the test failed, when a step does not happen.
  private func saveWithBZoomed(
    editing path: String, cwd: String, file: StaticString = #filePath, line: UInt = #line
  ) -> SavedPaneNode? {
    guard let fixture = TerminalViewFixture(file: file, line: line) else { return nil }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let l = buildLayout(fixture, file: file, line: line),
      let a = container.visiblePaneFrames[l.wA],
      drag(
        fixture, from: CGPoint(x: a.maxX, y: a.minY + a.height / 4),
        by: CGVector(dx: -160, dy: 0))
    else { return nil }
    let movedA = (container.visiblePaneFrames[l.wA]?.width ?? a.width) <= a.width - 100
    XCTAssertTrue(
      movedA,
      "premise: the drag must narrow A by 160 pt, so the saved weights are not the defaults",
      file: file, line: line)
    guard movedA, focus(fixture, l.wB, file: file, line: line) else { return nil }

    fixture.typeEx("edit \(path)")
    let edited = fixture.wake {
      canonical(handle.win_buffer(l.wB).abs_path().toString()) == canonical(path)
    }
    XCTAssertTrue(
      edited,
      "premise: :edit must show \(path) in B: \(handle.win_buffer(l.wB).abs_path().toString())",
      file: file, line: line)
    guard edited, zoom(fixture, on: l.wB, file: file, line: line) else { return nil }
    let shape = PaneLayoutCodec.encode(container.paneLayout, paths: [:], previews: [])

    var gathered: RestoreSeam.SavedTab?
    fixture.wake {
      if case .ready(let tab) = SessionSeam.gatherTab(
        hasFolder: true, cwd: cwd, sidebarVisible: true, layout: container.paneLayout,
        content: container.leafContents(), zoomed: container.zoomedPaneId)
      {
        gathered = tab
      }
      return gathered != nil
    }
    guard let tab = gathered, let layout = tab.layout else {
      XCTFail("premise: the tab must gather with a layout", file: file, line: line)
      return nil
    }
    XCTAssertEqual(
      layout.zoomedLeafIndex, 1,
      "the gathered layout must mark B, depth-first leaf 1; nil means the zoom is dropped from"
        + " the saved state", file: file, line: line)

    WindowStateCodec.submit([
      WindowStateCodec.Window(
        frame: CGRect(origin: .zero, size: TerminalViewFixture.windowSize), activeTab: 0,
        minimized: false, tabs: [tab], recency: [0])
    ])
    XCTAssertTrue(
      wolf_ffi_app_state_flush(), "premise: the window state must flush", file: file, line: line)
    return shape
  }

  /// `path` with symlinks resolved, so `/var` and `/private/var` compare equal.
  private func canonical(_ path: String) -> String {
    URL(fileURLWithPath: path).resolvingSymlinksInPath().path
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
