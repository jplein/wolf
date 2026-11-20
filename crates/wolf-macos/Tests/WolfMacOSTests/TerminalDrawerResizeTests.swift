// Drags the terminal drawer's top edge over a real core and its shell session.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// Drives the container's mouse handlers with synthesized events at the drawer's top edge,
/// and checks that the drawer, its session and the editor grid follow, and that the share
/// is the view's: kept across a close and reopen, and absent from a new view.
@MainActor
final class TerminalDrawerResizeTests: XCTestCase {

  /// How far each drag moves the drawer's top edge up, in points. At `windowSize` it keeps
  /// both sides well above one pane's minimum, so no clamp plays a part.
  private let distance: CGFloat = 160
  /// The x of every press and drag: the middle of the `windowSize` width.
  private let x: CGFloat = 480

  /// A drag on the drawer edge moves the drawer view up and grows it at once, then the
  /// session's rows follow, the shell's `stty size` reports them, and the editor grid
  /// above loses rows.
  func testDraggingTheDrawerEdgeResizesTheDrawerItsSessionAndTheEditor() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    guard let opened = openDrawer(fixture) else { return }
    let session = opened.session
    let before = opened.view.frame
    let sessionRows = handle.terminal_frame(session).rows()
    guard let editorBefore = editorRows(fixture) else { return }

    guard press(fixture), drag(fixture) else { return }
    let dragged = fixture.view(showing: session)?.frame
    XCTAssertEqual(
      dragged?.minY ?? .nan, before.minY - distance, accuracy: 1,
      "C1: a drag of \(distance) pt up from the drawer edge must move the drawer view's top by"
        + " as much before any wake: from \(before) to \(String(describing: dragged)). An"
        + " unmoved top means the press missed the edge, the share was not used by the layout,"
        + " or the drag laid nothing out; nil that the drag dropped the drawer view")
    XCTAssertEqual(
      dragged?.height ?? .nan, before.height + distance, accuracy: 1,
      "C1: the same drag must grow the drawer view by \(distance) pt: from \(before) to"
        + " \(String(describing: dragged)); a moved top with an unchanged height means the"
        + " editor and drawer no longer tile the pane area")
    release(fixture)

    let grew = fixture.wake { handle.terminal_frame(session).rows() > sessionRows }
    XCTAssertTrue(
      grew,
      "C1: the drawer session must be resized to more than its \(sessionRows) rows after the"
        + " drag; it has \(handle.terminal_frame(session).rows()). Unchanged means the drawer"
        + " pane's new rectangle never reached resize_terminal")
    guard grew else { return }

    let frame = handle.terminal_frame(session)
    let size = "\(frame.rows()) \(frame.cols())"
    fixture.runLine("stty size", in: session)
    let reported = fixture.wake { fixture.text(of: session).contains(size) }
    XCTAssertTrue(
      reported,
      "C1: the shell's stty size must print the frame's \"\(size)\", so the pty's size and"
        + " SIGWINCH followed the frame; the session reads:\n\(fixture.text(of: session))")

    var shrunk = editorBefore
    let reflowed = fixture.wake {
      shrunk = editorRows(fixture, failing: false) ?? editorBefore
      return shrunk < editorBefore
    }
    XCTAssertTrue(
      reflowed,
      "C1: the editor grid above the drawer must lose rows after the drag; it had"
        + " \(editorBefore) and has \(shrunk). Unchanged means the editor pane's ResizeGrid did"
        + " not follow the editor region's new height")
  }

  /// A dragged share outlives the drawer window: toggled closed and open again, the drawer
  /// view comes back at the dragged top, not at the opening third.
  func testTheDraggedShareSurvivesClosingAndReopeningTheDrawer() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    guard let opened = openDrawer(fixture) else { return }
    let session = opened.session
    let before = opened.view.frame
    guard press(fixture), drag(fixture) else { return }
    release(fixture)
    guard let dragged = fixture.view(showing: session)?.frame.minY,
      premiseMoved(from: before.minY, to: dragged)
    else { return }

    handle.toggle_terminal()
    let hidden = fixture.wake { fixture.view(showing: session) == nil }
    XCTAssertTrue(hidden, "premise: a second toggle must hide the drawer: its view must go")
    guard hidden else { return }

    handle.toggle_terminal()
    var reopened: CGFloat?
    let shown = fixture.wake {
      reopened = fixture.view(showing: session)?.frame.minY
      return reopened != nil
    }
    XCTAssertTrue(
      shown,
      "premise: a third toggle must show the drawer's session \(session) again; terminal views"
        + " \(fixture.terminalViews.map { $0.frame })")
    guard shown else { return }
    XCTAssertEqual(
      reopened ?? .nan, dragged, accuracy: 1,
      "C2: the reopened drawer must keep the dragged top at \(dragged), not the opening third's"
        + " \(before.minY); got \(String(describing: reopened)). The opening third means closing"
        + " the drawer reset the view's share")
  }

  /// A share dragged in one view does not reach another: a second container over a new
  /// tabpage opens its drawer at the opening third.
  func testANewViewOpensItsDrawerAtTheDefaultShare() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    guard let opened = openDrawer(fixture) else { return }
    let session = opened.session
    let before = opened.view.frame
    guard press(fixture), drag(fixture) else { return }
    release(fixture)
    guard let dragged = fixture.view(showing: session)?.frame.minY,
      premiseMoved(from: before.minY, to: dragged)
    else { return }

    handle.toggle_terminal()
    let hidden = fixture.wake { fixture.view(showing: session) == nil }
    XCTAssertTrue(hidden, "premise: a second toggle must hide the first view's drawer")
    guard hidden else { return }

    let editorWin1 = handle.active_win()
    let t2 = handle.create_tabpage("")
    XCTAssertNotEqual(t2, 0, "premise: create_tabpage must make a second tabpage")
    guard t2 != 0 else { return }
    // Only the current tab's container is woken, as the app does, so this waits unwoken.
    let entered = fixture.live.waitFor(timeout: TerminalViewFixture.timeout) {
      let win = handle.active_win()
      return handle.active_tabpage() == t2 && win != 0 && win != editorWin1
        && handle.windows().contains { $0.win() == win }
    }
    XCTAssertTrue(
      entered,
      "premise: the snapshot must report the new tabpage \(t2) current with its own window:"
        + " tabpage \(handle.active_tabpage()), window \(handle.active_win())")
    guard entered else { return }

    let (second, secondRenderer) = fixture.hostContainer(tabpage: t2)
    defer { withExtendedLifetime(secondRenderer) {} }
    let bound = fixture.wake(second) {
      second.paneCount == 1 && second.activeWin == handle.active_win()
    }
    XCTAssertTrue(
      bound,
      "premise: the second container must bind its first pane to window"
        + " \(handle.active_win()); it is on \(second.activeWin)")
    guard bound else { return }

    handle.toggle_terminal()
    var minY: CGFloat?
    let shown = fixture.wake(second) {
      minY = fixture.terminalViews(in: second).first?.frame.minY
      return minY != nil
    }
    XCTAssertTrue(
      shown,
      "premise: toggle_terminal() in the new tabpage must give the second container a drawer"
        + " view; it hosts \(fixture.terminalViews(in: second).map { $0.frame })")
    guard shown else { return }
    XCTAssertEqual(
      minY ?? .nan, before.minY, accuracy: 1,
      "C3: a new view's drawer must open at the opening third's top \(before.minY), not the"
        + " first view's dragged \(dragged); got \(String(describing: minY)). The dragged top"
        + " means the share is shared between views rather than held by each")
  }

  // MARK: - Helpers

  /// Toggles the drawer open and wakes until its view shows the session in the drawer strip,
  /// the session's rows match that view's height, and the editor window has a grid; `nil`,
  /// with the test failed, when that never happens.
  private func openDrawer(
    _ fixture: TerminalViewFixture, file: StaticString = #filePath, line: UInt = #line
  ) -> (session: UInt64, view: TerminalPaneView)? {
    let handle = fixture.handle
    let cellHeight = fixture.live.metrics.cellHeight
    handle.toggle_terminal()
    var session: UInt64 = 0
    var view: TerminalPaneView?
    let opened = fixture.wake {
      session = Array(handle.live_terminals()).first ?? 0
      view = session == 0 ? nil : fixture.view(showing: session)
      guard let view, fixture.isInDrawerStrip(view), fixture.window(showing: session) != 0
      else { return false }
      // Within a row: the frame's height snaps to the backing pixel, the rows to the cell.
      let rows = Int(handle.terminal_frame(session).rows())
      return abs(rows - Int(view.frame.height / cellHeight)) <= 1
        && editorRows(fixture, failing: false) != nil
    }
    XCTAssertTrue(
      opened,
      "premise: after toggle_terminal() a terminal view must show the drawer's session in the"
        + " bottom strip, sized to it, beside a gridded editor window; live sessions"
        + " \(Array(handle.live_terminals())), terminal views"
        + " \(fixture.terminalViews.map { $0.frame }), session rows"
        + " \(handle.terminal_frame(session).rows())", file: file, line: line)
    guard opened, let view else { return nil }
    return (session, view)
  }

  /// The rows of the editor window's grid: the window whose buffer is no terminal's. `nil`
  /// when it has no grid yet, failing the test when `failing`.
  private func editorRows(
    _ fixture: TerminalViewFixture, failing: Bool = true, file: StaticString = #filePath,
    line: UInt = #line
  ) -> UInt16? {
    let handle = fixture.handle
    let win =
      handle.windows().map { $0.win() }.first { handle.win_buffer($0).terminal() == 0 } ?? 0
    let grid = win == 0 ? 0 : handle.grid_for_win(win)
    guard grid != 0 else {
      if failing {
        XCTFail("premise: the editor window \(win) must have a grid", file: file, line: line)
      }
      return nil
    }
    return handle.read_frame_for_grid(grid).rows()
  }

  /// The drawer's top edge, in the container's flipped coordinates, at the opening share. It
  /// uses only `defaultWeight`, so this file compiles whatever the container holds.
  private func dividerY(_ fixture: TerminalViewFixture) -> CGFloat {
    let h = Double(fixture.container.bounds.height)
    return CGFloat(h - (h * DrawerLayout.defaultWeight).rounded())
  }

  /// A left-button mouse event of `type` at `point` in the container's coordinates, as the
  /// window delivers one; `nil`, with the test failed, when AppKit cannot make it.
  private func mouseEvent(
    _ type: NSEvent.EventType, at point: CGPoint, in fixture: TerminalViewFixture,
    file: StaticString = #filePath, line: UInt = #line
  ) -> NSEvent? {
    let event = NSEvent.mouseEvent(
      with: type, location: fixture.container.convert(point, to: nil), modifierFlags: [],
      timestamp: ProcessInfo.processInfo.systemUptime,
      windowNumber: fixture.renderer.window.windowNumber, context: nil, eventNumber: 0,
      clickCount: 1, pressure: type == .leftMouseUp ? 0 : 1)
    if event == nil {
      XCTFail("premise: could not synthesize a \(type) event", file: file, line: line)
    }
    return event
  }

  /// Presses on the drawer's top edge at the opening share.
  private func press(_ fixture: TerminalViewFixture) -> Bool {
    let point = CGPoint(x: x, y: dividerY(fixture))
    guard let event = mouseEvent(.leftMouseDown, at: point, in: fixture) else { return false }
    fixture.container.mouseDown(with: event)
    return true
  }

  /// Drags the pointer `distance` above the edge `press` pressed.
  private func drag(_ fixture: TerminalViewFixture) -> Bool {
    let point = CGPoint(x: x, y: dividerY(fixture) - distance)
    guard let event = mouseEvent(.leftMouseDragged, at: point, in: fixture) else { return false }
    fixture.container.mouseDragged(with: event)
    return true
  }

  /// Releases the pointer where `drag` left it.
  private func release(_ fixture: TerminalViewFixture) {
    let point = CGPoint(x: x, y: dividerY(fixture) - distance)
    guard let event = mouseEvent(.leftMouseUp, at: point, in: fixture) else { return }
    fixture.container.mouseUp(with: event)
  }

  /// Fails the test unless the drag moved the drawer's top up by at least 100 pt, which a
  /// later comparison against the opening third needs to tell the two apart.
  private func premiseMoved(
    from before: CGFloat, to after: CGFloat, file: StaticString = #filePath, line: UInt = #line
  ) -> Bool {
    let moved = before - after >= 100
    XCTAssertTrue(
      moved,
      "premise: the drag must move the drawer's top up by at least 100 pt, from \(before) to"
        + " \(after); T1 tells why it did not", file: file, line: line)
    return moved
  }
}
