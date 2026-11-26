// Close Pane's action on a terminal split, the drawer, and the tab's last pane.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// File > Close Pane reaches `closeActivePane()` through `WolfWindowController.closePane()`.
/// Neither the menu nor a key window renders here, so each test drives that member over a real
/// core and Neovim. The item's validation is `ClosePaneMenuTests`'.
///
/// On the tab's last pane core asks the front-end to close the tab, a `close_tab` command. No
/// fixture drains commands, since `onRenderWake()` drains intents only, so a test sees every
/// command core queued.
///
/// A failure prefixed `focus:` is Neovim's current window not being the pane's placeholder, a
/// focus that never reached Neovim through `focus_win`.
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

  /// Wakes the fixture until a drained command decodes to `.closeTab`; whether one did, and
  /// every command kind drained, decoded or not.
  private func drainCloseTab(_ fixture: TerminalViewFixture) -> (Bool, [String]) {
    var drained: [String] = []
    let asked = fixture.wake {
      let kinds = fixture.handle.drain_commands().map { $0.kind() }
      drained += kinds.map { CommandKind(ffi: $0).map { "\($0)" } ?? "raw \($0)" }
      return kinds.contains { CommandKind(ffi: $0) == .closeTab }
    }
    return (asked, drained)
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

  /// Close Pane on the tree's only editor leaf asks to close the tab, as `:q` does there, and
  /// closes no window itself: the front-end's close of the tab does.
  func testCloseOnTheOnlyEditorPaneAsksToCloseTheTab() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    let editorWin = fixture.handle.active_win()
    XCTAssertEqual(container.paneCount, 1, "premise: the fixture's tab must hold one pane")
    _ = fixture.handle.drain_commands()

    container.closeActivePane()
    let (asked, drained) = drainCloseTab(fixture)
    XCTAssertTrue(
      asked,
      "closeActivePane() on the only editor pane must reach core's close and drain close_tab;"
        + " drained \(drained). Nothing means the view dropped the close of the last pane"
        + " before it reached core, or core refused it as <C-w>c refuses the last window")
    XCTAssertTrue(
      windows(fixture).contains(editorWin),
      "the editor window \(editorWin) must stay until the front-end closes the tab; Neovim"
        + " reports \(windows(fixture))")
    XCTAssertEqual(
      container.paneCount, 1, "asking to close the tab must leave the tree's one editor leaf")
  }

  /// Close Pane on a terminal pane that is the tree's only leaf asks to close the tab, and
  /// leaves its session to the tab's shutdown.
  func testCloseOnALoneTerminalPaneAsksToCloseTheTab() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
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
    _ = handle.drain_commands()

    container.closeActivePane()
    let (asked, drained) = drainCloseTab(fixture)
    XCTAssertTrue(
      asked,
      "closeActivePane() on a lone terminal pane must drain close_tab; drained \(drained)."
        + " Nothing means the view dropped the close of the tree's last leaf, or core refused"
        + " it as wolf_terminal_close refuses the last pane")
    XCTAssertTrue(
      Array(handle.live_terminals()).contains(session),
      "asking to close the tab must leave session \(session) live until the tab shuts down:"
        + " live \(Array(handle.live_terminals())). Gone means the pane closed as a split and"
        + " hung it up")
  }

  /// Close Pane on the tree's last editor leaf, with the drawer open beside it, asks to close
  /// the tab, since the drawer is no pane of the tab, and closes neither window itself.
  func testCloseOnTheLastEditorPaneBesideTheDrawerAsksToCloseTheTab() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let editorWin = handle.active_win()

    let session = openDrawer(fixture)
    guard session != 0 else { return }
    let drawerWin = fixture.window(showing: session)
    fixture.terminalChord("k", to: session)
    let up = fixture.wake { container.activeTerminal == nil && handle.active_win() == editorWin }
    XCTAssertTrue(
      up,
      "premise: <C-w>k in the drawer must focus the editor window \(editorWin): active window"
        + " \(handle.active_win()), active terminal"
        + " \(String(describing: container.activeTerminal))")
    guard up else { return }
    _ = handle.drain_commands()

    container.closeActivePane()
    let (asked, drained) = drainCloseTab(fixture)
    XCTAssertTrue(
      asked,
      "closeActivePane() on the last editor leaf beside the drawer must drain close_tab;"
        + " drained \(drained). Nothing means the view dropped the close, or core counted the"
        + " drawer as a pane and closed the editor as a split")
    XCTAssertTrue(
      windows(fixture).contains(editorWin) && windows(fixture).contains(drawerWin),
      "the editor window \(editorWin) and the drawer's \(drawerWin) must stay until the"
        + " front-end closes the tab; Neovim reports \(windows(fixture)). The editor gone means"
        + " it was closed as a split beside the drawer")
    XCTAssertEqual(
      container.paneCount, 1, "asking to close the tab must leave the tree's one editor leaf")
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
    XCTAssertEqual(
      container.activeTerminal, session,
      "premise: with no wake since focus_win, the view's active pane must still be the drawer")
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
