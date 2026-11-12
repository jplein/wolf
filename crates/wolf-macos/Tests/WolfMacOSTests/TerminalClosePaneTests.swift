// Close Pane's gate and action on a terminal split, the drawer and the last editor pane.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// File > Close reaches `PaneContainerView.canClosePane` through `validateMenuItem` and
/// `closeActivePane()` through `WolfWindowController.closePane()`. Neither the menu nor a key
/// window renders here, so each test drives those two members over a real core and Neovim.
///
/// A failure prefixed `focus:` is Neovim's current window not being the pane's placeholder, a
/// focus that never reached Neovim through `focus_win`; one prefixed `gate:` is Close disabled
/// by `validateMenuItem`.
@MainActor
final class TerminalClosePaneTests: XCTestCase {

  /// The windows Neovim reports, as handles.
  private func windows(_ fixture: TerminalViewFixture) -> [Int64] {
    fixture.handle.windows().map { $0.win() }
  }

  /// Opens the drawer and waits until its session `s` shows in the strip and the container's
  /// active pane is the drawer; the session, or 0 with the test failed.
  private func openDrawer(_ fixture: TerminalViewFixture) -> UInt64 {
    let handle = fixture.handle
    let container = fixture.container
    handle.toggle_terminal()
    var session: UInt64 = 0
    let opened = fixture.wake {
      session = Array(handle.live_terminals()).first ?? 0
      guard session != 0, let view = fixture.view(showing: session) else { return false }
      return fixture.isInDrawerStrip(view) && container.activeTerminal == session
    }
    XCTAssertTrue(
      opened,
      "premise: toggle_terminal() must show the drawer's session in the strip, focused: live"
        + " \(Array(handle.live_terminals())), terminal views"
        + " \(fixture.terminalViews.map { ($0.session, $0.frame) }), active terminal"
        + " \(String(describing: container.activeTerminal))")
    return opened ? session : 0
  }

  /// Close Pane on a focused terminal split closes its placeholder window, removes the pane
  /// from the tree, keeps the editor, and hangs up the split's session.
  func testCloseClosesAFocusedTerminalSplitAndHangsUpItsSession() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let editorWin = handle.active_win()

    fixture.neovimChord("v")
    let split = fixture.wake { container.paneCount == 2 }
    XCTAssertTrue(
      split, "premise: <C-w>v must split the editor into two panes: \(container.paneCount)")
    guard split else { return }
    fixture.typeEx("terminal")
    var session: UInt64 = 0
    let opened = fixture.wake {
      session = fixture.terminalViews.first?.session ?? 0
      return session != 0 && container.paneCount == 2 && fixture.window(showing: session) != 0
    }
    XCTAssertTrue(
      opened,
      "premise: a typed :terminal must turn the new pane into a terminal view:"
        + " \(container.paneCount) pane(s), terminal views"
        + " \(fixture.terminalViews.map { ($0.session, $0.frame) })")
    guard opened else { return }

    fixture.terminalChord("h", to: session)
    let left = fixture.wake { handle.active_win() == editorWin && container.activeTerminal == nil }
    XCTAssertTrue(
      left,
      "premise: <C-w>h in the terminal pane must focus the editor window \(editorWin): active"
        + " window \(handle.active_win())")
    guard left else { return }
    fixture.neovimChord("l")
    let back = fixture.wake { container.activeTerminal == session }
    XCTAssertTrue(
      back,
      "premise: <C-w>l in the editor must focus the terminal pane on session \(session): active"
        + " terminal \(String(describing: container.activeTerminal))")
    guard back else { return }

    let terminalWin = fixture.window(showing: session)
    let neovimFocused = fixture.wake { handle.active_win() == terminalWin }
    XCTAssertTrue(
      neovimFocused,
      "focus: with the terminal pane active in the view, Neovim's current window must be"
        + " its placeholder \(terminalWin), but it is \(handle.active_win()). The focus move"
        + " never sent focus_win, so :close would close another window")
    XCTAssertTrue(
      container.canClosePane,
      "gate: with two leaves and the terminal pane active, Close must be enabled;"
        + " canClosePane is false, so validateMenuItem disables File > Close and ⌘W falls"
        + " through to the pty")

    container.closeActivePane()
    let closed = fixture.wake {
      fixture.window(showing: session) == 0 && fixture.view(showing: session) == nil
        && container.paneCount == 1
    }
    XCTAssertTrue(
      closed,
      "closeActivePane() must close the terminal pane and drop it from the tree: placeholder"
        + " window \(fixture.window(showing: session)), view"
        + " \(fixture.view(showing: session) == nil ? "gone" : "still hosted"),"
        + " \(container.paneCount) pane(s). A window still showing the session means no"
        + " :close reached Neovim; a closed window with the pane kept means the container never"
        + " noticed the close")
    guard closed else { return }
    XCTAssertTrue(
      windows(fixture).contains(editorWin),
      "the editor window \(editorWin) must survive; Neovim reports \(windows(fixture)), so"
        + " :close closed the editor rather than the terminal pane")

    let hungUp = fixture.wake { !Array(handle.live_terminals()).contains(session) }
    XCTAssertTrue(
      hungUp,
      "closing a split terminal pane must hang up its session \(session): live"
        + " \(Array(handle.live_terminals())). Still live means its placeholder was hidden, not"
        + " wiped, so the session outlived its pane")
  }

  /// Close Pane on the focused drawer closes the drawer window and keeps its session, so the
  /// next toggle shows the same session again.
  func testCloseClosesTheFocusedDrawerAndKeepsItsSession() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let editorWin = handle.active_win()

    let session = openDrawer(fixture)
    guard session != 0 else { return }
    let drawerWin = fixture.window(showing: session)
    let neovimFocused = fixture.wake { handle.active_win() == drawerWin }
    XCTAssertTrue(
      neovimFocused,
      "focus: with the drawer active in the view, Neovim's current window must be the"
        + " drawer's \(drawerWin), but it is \(handle.active_win()) (the editor is \(editorWin))."
        + " The drawer's focus never sent focus_win, so :close would close another window")
    XCTAssertTrue(
      container.canClosePane,
      "gate: with the drawer focused, Close must be enabled although the tree holds"
        + " \(container.paneCount) leaf; canClosePane is false, so validateMenuItem disables"
        + " File > Close and ⌘W falls through to the drawer's pty")

    container.closeActivePane()
    let closed = fixture.wake {
      fixture.view(showing: session) == nil && fixture.window(showing: session) == 0
    }
    XCTAssertTrue(
      closed,
      "closeActivePane() must close the drawer: window \(fixture.window(showing: session))"
        + " still shows session \(session), view"
        + " \(fixture.view(showing: session) == nil ? "gone" : "still hosted"). A window still"
        + " open means no :close reached Neovim, or it closed the editor instead")
    guard closed else { return }
    XCTAssertEqual(
      container.paneCount, 1, "closing the drawer must leave the tree's one editor leaf")
    XCTAssertNil(
      container.activeTerminal,
      "after the drawer closes, the active pane must be the editor, not a terminal")
    XCTAssertTrue(
      Array(handle.live_terminals()).contains(session),
      "closing the drawer must keep its session \(session) live, as <C-w>c does: live"
        + " \(Array(handle.live_terminals())). Gone means its placeholder was wiped")

    handle.toggle_terminal()
    let reopened = fixture.wake {
      guard let view = fixture.view(showing: session) else { return false }
      return fixture.isInDrawerStrip(view)
    }
    XCTAssertTrue(
      reopened,
      "the next toggle must show the SAME session \(session) in the drawer strip: live"
        + " \(Array(handle.live_terminals())), terminal views"
        + " \(fixture.terminalViews.map { ($0.session, $0.frame) }). Another session means the"
        + " close ended the drawer's shell and the toggle started a new one")
  }

  /// Close Pane is refused on a terminal pane that is the tree's only leaf.
  func testCloseIsRefusedOnALoneTerminalPane() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container

    let session = fixture.live.openTerminal("exec /bin/sh")
    XCTAssertNotEqual(session, 0, "premise: open_terminal must start a session in the window")
    guard session != 0 else { return }
    let opened = fixture.wake {
      fixture.view(showing: session) != nil && container.activeTerminal == session
        && container.paneCount == 1
    }
    XCTAssertTrue(
      opened,
      "premise: the only pane must show session \(session), active: \(container.paneCount)"
        + " pane(s), active terminal \(String(describing: container.activeTerminal))")
    guard opened else { return }

    XCTAssertFalse(
      container.canClosePane,
      "a terminal pane that is the tree's last leaf must refuse Close, as wolf_do_close refuses"
        + " the last window; canClosePane is true, so File > Close would be enabled on it")
  }

  /// Close Pane is refused on the tree's last editor leaf while the drawer is open beside
  /// it, and closing anyway would leave the drawer as the tabpage's only window.
  func testCloseIsRefusedOnTheLastEditorPaneBesideTheDrawer() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let editorWin = handle.active_win()

    let session = openDrawer(fixture)
    guard session != 0 else { return }
    fixture.terminalChord("k", to: session)
    let up = fixture.wake { container.activeTerminal == nil && handle.active_win() == editorWin }
    XCTAssertTrue(
      up,
      "premise: <C-w>k in the drawer must focus the editor window \(editorWin): active window"
        + " \(handle.active_win()), active terminal"
        + " \(String(describing: container.activeTerminal))")
    guard up else { return }

    XCTAssertFalse(
      container.canClosePane,
      "the tree's last editor leaf must refuse Close while the drawer is open: Neovim counts"
        + " the drawer's window, so :close would take the editor and leave only the drawer")

    // The toggle is queued behind the close on core's input channel.
    container.closeActivePane()
    handle.toggle_terminal()
    let hidden = fixture.wake {
      fixture.view(showing: session) == nil && fixture.window(showing: session) == 0
    }
    XCTAssertTrue(
      hidden,
      "the toggle after closeActivePane() must hide the drawer: window"
        + " \(fixture.window(showing: session)) still shows session \(session), Neovim reports"
        + " \(windows(fixture)). A drawer that stays means closeActivePane() closed the editor"
        + " despite the refused gate, leaving the drawer as the last window")
    guard hidden else { return }
    XCTAssertTrue(
      windows(fixture).contains(editorWin),
      "the editor window \(editorWin) must survive a refused Close; Neovim reports"
        + " \(windows(fixture))")
    XCTAssertEqual(
      container.paneCount, 1, "a refused Close must leave the tree's one editor leaf")
  }

  /// While Neovim's current window lags the view's active pane, as during a focus move in
  /// flight, Close Pane still closes the pane the view shows active.
  func testCloseActsOnTheActivePaneWhileNeovimFocusLags() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let editorWin = handle.active_win()

    let session = openDrawer(fixture)
    guard session != 0 else { return }

    // No wake between the two: the view's active pane stays the drawer, Neovim's is the editor.
    handle.focus_win(editorWin)
    XCTAssertTrue(
      container.canClosePane,
      "premise: with the drawer still the view's active pane, Close must be enabled")
    container.closeActivePane()

    let closed = fixture.wake {
      fixture.view(showing: session) == nil && fixture.window(showing: session) == 0
    }
    XCTAssertTrue(
      closed,
      "closeActivePane() must close the drawer the view shows active even while Neovim's"
        + " current window is the editor: window \(fixture.window(showing: session)) still"
        + " shows session \(session), Neovim reports \(windows(fixture)). A drawer that stays"
        + " means :close ran on Neovim's current window, the editor, with no focus_win first")
    guard closed else { return }
    XCTAssertTrue(
      windows(fixture).contains(editorWin),
      "the editor window \(editorWin) must survive; Neovim reports \(windows(fixture))")
  }
}
