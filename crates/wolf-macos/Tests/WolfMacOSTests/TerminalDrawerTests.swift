// Opens, paints, focuses, hides and reopens the terminal drawer over a real core and its shell.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A packed `0x00RRGGBB` colour as an sRGB `NSColor`.
private func srgb(_ packed: UInt32) -> NSColor {
  NSColor(
    srgbRed: CGFloat((packed >> 16) & 0xFF) / 255, green: CGFloat((packed >> 8) & 0xFF) / 255,
    blue: CGFloat(packed & 0xFF) / 255, alpha: 1)
}

@MainActor
final class TerminalDrawerTests: XCTestCase {

  /// The background the drawer's shell prints, far from any default terminal colour.
  private let crimson = srgb(0x00DC_143C)
  /// Eight crimson-background spaces, then a newline. Typed, the line holds no crimson.
  private let crimsonLine = "printf '\\033[48;2;220;20;60m        \\033[0m\\n'"

  /// `toggle_terminal()` shows the drawer as a terminal view in the bottom strip on the
  /// drawer window's session, paints what the shell prints, hides with the session kept,
  /// and shows the same session again on the next toggle.
  func testTheDrawerOpensOnASessionPaintsHidesAndReopens() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle

    handle.toggle_terminal()
    var session: UInt64 = 0
    let opened = fixture.wake {
      session = Array(handle.live_terminals()).first ?? 0
      guard session != 0, let view = fixture.view(showing: session) else { return false }
      return fixture.isInDrawerStrip(view) && fixture.window(showing: session) != 0
    }
    XCTAssertTrue(
      opened,
      "after toggle_terminal() a terminal view must show the drawer's session in the bottom"
        + " strip; live sessions \(Array(handle.live_terminals())), terminal views at"
        + " \(fixture.terminalViews.map { $0.frame }). No session means toggle_terminal starts"
        + " none; a session with no view means applyDrawerOpen bound no drawer pane; a view in"
        + " the upper half means the drawer was put into the pane tree")
    guard opened else { return }
    XCTAssertEqual(
      handle.win_buffer(fixture.window(showing: session)).terminal(), session,
      "the drawer view's session must be the one the drawer window's buffer carries")
    XCTAssertEqual(
      fixture.container.paneCount, 1,
      "the drawer lives outside the pane tree, so the tree must still hold one leaf")

    guard let before = fixture.view(showing: session) else { return }
    let precondition = fixture.render()
    XCTAssertEqual(
      precondition.colors(in: before.bounds, of: before).filter {
        colorDistance($0, crimson) <= 0.05
      }.count, 0,
      "premise: nothing in the drawer is crimson before the line runs")

    fixture.runLine(crimsonLine, in: session)
    let painted = fixture.wake {
      guard let view = fixture.view(showing: session) else { return false }
      let capture = fixture.renderer.render()
      let crimsonPixels = capture.colors(in: view.bounds, of: view).filter {
        colorDistance($0, crimson) <= 0.05
      }.count
      let cellArea = Double(fixture.live.metrics.cellWidth * fixture.live.metrics.cellHeight)
      return Double(crimsonPixels) >= 4 * cellArea
    }
    XCTAssertTrue(
      painted,
      "the drawer view must paint the crimson background its shell printed under eight cells"
        + " (at least four cells' area); the frame reads:\n\(fixture.text(of: session))")

    handle.toggle_terminal()
    let hidden = fixture.wake { fixture.view(showing: session) == nil }
    XCTAssertTrue(
      hidden, "a second toggle must hide the drawer: its terminal view must go")
    XCTAssertTrue(
      Array(handle.live_terminals()).contains(session),
      "hiding the drawer must keep its session live; gone means its placeholder was wiped")

    handle.toggle_terminal()
    let reopened = fixture.wake {
      guard let view = fixture.view(showing: session) else { return false }
      return fixture.isInDrawerStrip(view)
    }
    XCTAssertTrue(
      reopened,
      "a third toggle must show the SAME session \(session) in the drawer strip again; live"
        + " sessions \(Array(handle.live_terminals())). A stale drawer pane id refuses the"
        + " reopen; another session means the drawer started a new shell")
  }

  /// With the drawer open and an editor pane active, `PaneContainerView.toggleTerminal()`, the
  /// call the palette and `:WolfTerminal` reach, makes the drawer the active pane, with its
  /// session taking the keys, and leaves it open at its frame; a second call closes it and keeps
  /// the session.
  func testToggleTerminalFocusesTheOpenDrawerFromAnEditorPaneAndASecondClosesIt() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let editor = container.activeWin
    XCTAssertNotEqual(editor, 0, "premise: the editor pane must be active before the drawer opens")

    container.toggleTerminal()
    var session: UInt64 = 0
    var drawer: Int64 = 0
    let opened = fixture.wake {
      session = Array(handle.live_terminals()).first ?? 0
      guard session != 0, let view = fixture.view(showing: session), fixture.isInDrawerStrip(view)
      else { return false }
      drawer = fixture.window(showing: session)
      return drawer != 0 && drawer != editor && container.activeWin == drawer
    }
    XCTAssertTrue(
      opened,
      "premise: toggleTerminal() with no drawer must open it in the strip, active: session"
        + " \(session), window \(drawer), view \(container.activeWin)")
    guard opened else { return }

    handle.focus_win(editor)
    let onEditor = fixture.wake { container.activeWin == editor && handle.active_win() == editor }
    XCTAssertTrue(
      onEditor,
      "premise: focus_win(\(editor)) must make the editor active in Neovim and the view, with"
        + " the drawer open: Neovim \(handle.active_win()), view \(container.activeWin)")
    guard onEditor, let before = fixture.view(showing: session)?.frame else {
      XCTFail("premise: the drawer's view must still show \(session) once the editor is active")
      return
    }
    XCTAssertNotEqual(
      container.activeTerminal, session,
      "premise: with the editor active, the drawer's session must not take the keys")

    container.toggleTerminal()
    let focused = fixture.wake {
      container.activeWin == drawer && handle.active_win() == drawer
        && container.activeTerminal == session
    }
    XCTAssertTrue(
      focused,
      "toggleTerminal() with the drawer open and the editor active must make the drawer"
        + " (\(drawer)) the active pane, its session \(session) taking the keys: view"
        + " \(container.activeWin), Neovim \(handle.active_win()), active terminal"
        + " \(String(describing: container.activeTerminal)). The editor in both means the toggle"
        + " closed the drawer or did nothing; Neovim on the drawer and the view on the editor,"
        + " that the view does not follow Neovim to the drawer")
    guard focused else { return }
    let view = fixture.view(showing: session)
    XCTAssertTrue(
      view.map { fixture.isInDrawerStrip($0) } ?? false,
      "the focused drawer must stay open in the strip; no view means the toggle closed it")
    XCTAssertEqual(
      view?.frame, before,
      "focusing the drawer must leave its frame as it was")
    XCTAssertTrue(
      handle.windows().contains { $0.win() == drawer },
      "the focused drawer must keep its window \(drawer); another means it was closed and"
        + " reopened: \(handle.windows().map { $0.win() })")

    container.toggleTerminal()
    let closed = fixture.wake {
      fixture.view(showing: session) == nil && !handle.windows().contains { $0.win() == drawer }
    }
    XCTAssertTrue(
      closed,
      "a second toggleTerminal(), made from the focused drawer, must close it: windows"
        + " \(handle.windows().map { $0.win() }). Still open means a toggle from the active"
        + " drawer focuses it again instead of closing it")
    XCTAssertTrue(
      Array(handle.live_terminals()).contains(session),
      "closing the drawer must keep its session live: \(Array(handle.live_terminals()))")
  }
}
