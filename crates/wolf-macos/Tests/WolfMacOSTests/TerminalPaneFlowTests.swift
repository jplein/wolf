// Moves focus in and out of a terminal pane, splits it, and restores terminals after a restart.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

@MainActor
final class TerminalPaneFlowTests: XCTestCase {

  /// `terminal_target::SPLIT_VERTICAL`.
  private let splitVertical: UInt8 = 2
  /// `NvimLiveness.dead`'s raw value, as `liveness()` reports it.
  private let dead = NvimLiveness.dead.rawValue

  /// `<C-w>h` in a terminal pane moves focus to the editor beside it, `<C-w>l` in the editor
  /// moves it back, and `<C-w>v` in the terminal pane splits it into a new pane on a new
  /// session, which takes focus.
  func testFocusMovesInAndOutOfATerminalPaneAndASplitMakesANewSession() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let editorWin = handle.active_win()

    let session = handle.open_terminal(editorWin, splitVertical, "exec /bin/sh")
    XCTAssertNotEqual(session, 0, "open_terminal must start a session in a split")
    guard session != 0 else { return }
    let opened = fixture.wake {
      container.paneCount == 2 && fixture.view(showing: session) != nil
        && container.activeTerminal == session
    }
    XCTAssertTrue(
      opened,
      "the split must become a second, focused pane showing the session: \(container.paneCount)"
        + " pane(s), active terminal \(String(describing: container.activeTerminal))")
    guard opened else { return }
    let terminalWin = fixture.window(showing: session)

    fixture.terminalChord("h", to: session)
    let left = fixture.wake {
      handle.active_win() == editorWin && container.activeTerminal == nil
    }
    XCTAssertTrue(
      left,
      "<C-w>h in the terminal pane must focus the editor window \(editorWin): active window"
        + " \(handle.active_win()), active terminal \(String(describing: container.activeTerminal))."
        + " Unchanged means applyFocus ignores a FocusDir whose origin is a terminal pane, or"
        + " the chord went to the shell")

    fixture.neovimChord("l")
    let back = fixture.wake {
      handle.active_win() == terminalWin && container.activeTerminal == session
    }
    XCTAssertTrue(
      back,
      "<C-w>l in the editor must focus the terminal pane's window \(terminalWin) and its"
        + " session \(session): active window \(handle.active_win()), active terminal"
        + " \(String(describing: container.activeTerminal))")
    guard back else { return }

    fixture.terminalChord("v", to: session)
    var newSession: UInt64 = 0
    let split = fixture.wake {
      newSession = container.activeTerminal ?? 0
      return container.paneCount == 3 && newSession != 0 && newSession != session
        && fixture.view(showing: session) != nil && fixture.view(showing: newSession) != nil
    }
    XCTAssertTrue(
      split,
      "<C-w>v in the terminal pane must add a third pane on a NEW session and focus it:"
        + " \(container.paneCount) pane(s), active terminal"
        + " \(String(describing: container.activeTerminal)), first session \(session), live"
        + " \(Array(handle.live_terminals())). Two panes means the router's split bound no"
        + " window; the first session focused means the new pane was never focused")
  }

  /// After Neovim is killed and restarted, the restore a view requests puts its split
  /// session into a new pane of the tree and reopens the drawer on the drawer's session,
  /// each still showing what its shell printed before the kill.
  func testARestartRebindsTheSplitSessionAndReopensTheDrawer() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container

    // A session in the current window, which becomes a terminal leaf of the tree.
    let pane = fixture.live.openTerminal("echo pane-$((4+4)); exec sleep 600")
    XCTAssertNotEqual(pane, 0, "open_terminal must start the pane's session")
    guard pane != 0 else { return }
    handle.toggle_terminal()
    var drawer: UInt64 = 0
    let ready = fixture.wake {
      drawer = Array(handle.live_terminals()).first { $0 != pane } ?? 0
      return drawer != 0 && fixture.view(showing: pane) != nil
        && fixture.view(showing: drawer) != nil
    }
    XCTAssertTrue(
      ready,
      "premise: the pane's session and the drawer's must both be shown before the kill; live"
        + " \(Array(handle.live_terminals()))")
    guard ready else { return }
    fixture.runLine("echo drawer-$((5+5))", in: drawer)
    XCTAssertTrue(
      fixture.wake {
        fixture.text(of: pane).contains("pane-8") && fixture.text(of: drawer).contains("drawer-10")
      },
      "premise: both shells must print their markers before the kill")

    let captured = container.capturedTerminals()
    XCTAssertEqual(
      captured.splits, [pane],
      "capturedTerminals() must name the tree's terminal leaf's session, and not the drawer's")
    XCTAssertTrue(captured.drawerOpen, "capturedTerminals() must report the drawer open")

    // The drawer's placeholder is nofile and not in terminal mode, so Neovim takes the command.
    fixture.typeEx("lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)")
    let died = fixture.live.waitFor(timeout: TerminalViewFixture.timeout) {
      handle.liveness() == self.dead
    }
    XCTAssertTrue(died, "premise: the SIGKILLed Neovim must be reported dead")
    guard died else { return }
    XCTAssertTrue(handle.restart(), "restart() on a dead Neovim must respawn it")

    container.rebind(tabpageHandle: handle.current_tabpage())
    container.requestTerminalRestore(
      LivenessSeam.terminalRestore(
        splits: captured.splits, drawerOpen: captured.drawerOpen,
        live: Set(Array(handle.live_terminals()))))

    let restored = fixture.wake {
      guard container.paneCount == 2, let paneView = fixture.view(showing: pane),
        let drawerView = fixture.view(showing: drawer)
      else { return false }
      return !fixture.isInDrawerStrip(paneView) && fixture.isInDrawerStrip(drawerView)
    }
    XCTAssertTrue(
      restored,
      "after the restart the tree must hold two leaves, the editor and a terminal pane on the"
        + " SAME session \(pane), and the drawer strip must show the drawer's session \(drawer):"
        + " \(container.paneCount) pane(s), terminal views"
        + " \(fixture.terminalViews.map { ($0.session, $0.frame) }). One pane means the split"
        + " was never rebound (queue_bind_terminal queued nothing); no drawer view means the"
        + " .drawer step was skipped")
    XCTAssertTrue(
      fixture.text(of: pane).contains("pane-8") && fixture.text(of: drawer).contains("drawer-10"),
      "the restored panes must show the surviving sessions' output, not new shells")
  }
}
