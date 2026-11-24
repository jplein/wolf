// The tab's last editor pane beside the drawer: :bd keeps it, and a picked file lands in it.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// The tab's only editor pane while the terminal drawer is open, over a real core and Neovim:
/// `:bd` in it keeps the pane, which can take focus again, and a file opened through
/// `openPickedFile` while the drawer is the active pane lands in it rather than in the drawer.
///
/// The drawer is a Neovim window that the pane tree does not hold, so Neovim's own `:bd` and
/// `:edit` treat it as another window, where the tree has one pane.
@MainActor
final class TerminalDrawerLastPaneTests: XCTestCase {

  /// The windows Neovim reports, as handles.
  private func windows(_ fixture: TerminalViewFixture) -> [Int64] {
    fixture.handle.windows().map { $0.win() }
  }

  /// The file window `win` shows, or empty.
  private func shown(_ fixture: TerminalViewFixture, in win: Int64) -> String {
    fixture.handle.win_buffer(win).abs_path().toString()
  }

  /// Writes `lines` lines to `name` in a fresh scratch directory and returns its path; `nil`,
  /// with the test failed, when it cannot.
  private func fixtureFile(_ name: String, lines: Int) -> String? {
    guard let dir = LiveCore.scratchDirectory() else { return nil }
    let path = (dir as NSString).appendingPathComponent(name)
    let text = (1...lines).map { "line \($0) of \(name)" }.joined(separator: "\n") + "\n"
    do {
      try text.write(toFile: path, atomically: true, encoding: .utf8)
    } catch {
      XCTFail("premise: could not write the fixture \(path): \(error)")
      return nil
    }
    return path
  }

  /// Opens `path` in the active editor pane and waits until it shows there.
  private func edit(_ fixture: TerminalViewFixture, _ path: String) -> Bool {
    fixture.handle.open(path)
    let opened = fixture.wake {
      let win = fixture.container.activeWin
      return win != 0 && shown(fixture, in: win) == path
    }
    XCTAssertTrue(
      opened,
      "premise: \(path) must show in the active editor pane; it shows"
        + " \(shown(fixture, in: fixture.container.activeWin))")
    return opened
  }

  /// Opens the drawer and waits until its session shows in the strip as the active pane;
  /// the session, or 0 with the test failed.
  private func openDrawer(_ fixture: TerminalViewFixture) -> UInt64 {
    let handle = fixture.handle
    let container = fixture.container
    handle.toggle_terminal()
    var session: UInt64 = 0
    let opened = fixture.wake {
      session = Array(handle.live_terminals()).first ?? 0
      guard session != 0, let view = fixture.view(showing: session) else { return false }
      return fixture.isInDrawerStrip(view) && container.activeTerminal == session
        && handle.active_win() == fixture.window(showing: session)
    }
    XCTAssertTrue(
      opened,
      "premise: toggle_terminal() must show the drawer's session in the strip, active, with"
        + " Neovim's current window its own: live \(Array(handle.live_terminals())), active"
        + " terminal \(String(describing: container.activeTerminal)), active window"
        + " \(handle.active_win())")
    return opened ? session : 0
  }

  /// C22: `:bd` in the tab's only editor pane beside the drawer keeps the pane, on the file
  /// shown before, active and current in Neovim; the pane then takes focus again after the
  /// drawer has had it.
  func testBdBesideTheDrawerKeepsTheLastEditorPane() {
    guard let first = fixtureFile("lpearlier.md", lines: 3),
      let second = fixtureFile("lplater.md", lines: 3)
    else { return }
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard edit(fixture, first), edit(fixture, second) else { return }
    let editorWin = handle.active_win()
    let editorPane = container.activePaneId

    let session = openDrawer(fixture)
    guard session != 0 else { return }
    let drawerWin = fixture.window(showing: session)
    fixture.terminalChord("k", to: session)
    let up = fixture.wake {
      container.activePaneId == editorPane && handle.active_win() == editorWin
    }
    XCTAssertTrue(
      up,
      "premise: <C-w>k in the drawer must make the editor pane active: active window"
        + " \(handle.active_win()), active terminal"
        + " \(String(describing: container.activeTerminal))")
    guard up else { return }

    fixture.typeEx("bd")
    let kept = fixture.wake {
      container.paneCount == 1 && windows(fixture).contains(editorWin)
        && container.activePaneId == editorPane && handle.active_win() == editorWin
        && shown(fixture, in: editorWin) == first
    }
    XCTAssertTrue(
      kept,
      ":bd beside the drawer must keep the tab's only editor pane on \(first):"
        + " \(container.paneCount) pane(s), Neovim's windows \(windows(fixture)) (the editor's is \(editorWin), the"
        + " drawer's \(drawerWin)), active pane \(String(describing: container.activePaneId))"
        + " (the editor's is \(String(describing: editorPane))), Neovim's current window"
        + " \(handle.active_win()), the editor showing \(shown(fixture, in: editorWin))."
        + " No pane and the drawer current means Neovim closed the editor window, because the"
        + " drawer kept it from being the tabpage's last")
    guard kept else { return }

    handle.toggle_terminal()
    let toDrawer = fixture.wake {
      container.activeTerminal == session && handle.active_win() == drawerWin
    }
    XCTAssertTrue(
      toDrawer,
      "premise: toggle_terminal() from the editor must focus the open drawer: active terminal"
        + " \(String(describing: container.activeTerminal)), active window \(handle.active_win())")
    guard toDrawer else { return }
    fixture.terminalChord("k", to: session)
    let back = fixture.wake {
      container.activePaneId == editorPane && container.activeTerminal == nil
        && handle.active_win() == editorWin
    }
    XCTAssertTrue(
      back,
      "<C-w>k from the drawer must make the kept editor pane active again: active pane"
        + " \(String(describing: container.activePaneId)), Neovim's current window"
        + " \(handle.active_win()) (the editor's is \(editorWin)). Staying on the drawer means the"
        + " kept pane cannot take focus")
  }

  /// Opens the drawer over one editor pane, then hands a picked `path` to `openPickedFile` at
  /// `cursor`, and asserts it lands in the editor pane, active, with the drawer kept. Returns
  /// the fixture and the editor's window once it has, or `nil` with the test failed.
  private func pickWithTheDrawerActive(
    _ path: String, at cursor: (line: UInt64, col: UInt64)?
  ) -> (TerminalViewFixture, Int64)? {
    guard let fixture = TerminalViewFixture() else { return nil }
    let handle = fixture.handle
    let container = fixture.container
    let editorWin = handle.active_win()
    let editorPane = container.activePaneId
    let session = openDrawer(fixture)
    guard session != 0 else {
      fixture.shutdown()
      return nil
    }
    let drawerWin = fixture.window(showing: session)

    container.openPickedFile(path, at: cursor)
    let landed = fixture.wake {
      shown(fixture, in: editorWin) == path && container.activePaneId == editorPane
        && handle.active_win() == editorWin
    }
    XCTAssertTrue(
      landed,
      "a file picked while the drawer is active must open in the editor pane, active:"
        + " the editor \(editorWin) shows \(shown(fixture, in: editorWin)), the drawer"
        + " \(drawerWin) shows \(shown(fixture, in: drawerWin)), active pane"
        + " \(String(describing: container.activePaneId)) (the editor's is"
        + " \(String(describing: editorPane))), Neovim's current window \(handle.active_win())."
        + " The file in the drawer's window means it was edited in Neovim's current window")
    XCTAssertEqual(
      fixture.window(showing: session), drawerWin,
      "the drawer's window \(drawerWin) must still show its session \(session) after the pick")
    guard landed else {
      fixture.shutdown()
      return nil
    }
    return (fixture, editorWin)
  }

  /// C23: a file picked while the drawer is the active pane opens in the editor pane.
  func testAFilePickedWithTheDrawerActiveOpensInTheEditorPane() {
    guard let path = fixtureFile("lppicked.md", lines: 3) else { return }
    guard let (fixture, _) = pickWithTheDrawerActive(path, at: nil) else { return }
    fixture.shutdown()
  }

  /// C24: a search result picked while the drawer is the active pane opens in the editor pane
  /// with the cursor on its line.
  func testAResultPickedWithTheDrawerActiveOpensAtItsLine() {
    guard let path = fixtureFile("lpresult.md", lines: 8) else { return }
    guard let (fixture, editorWin) = pickWithTheDrawerActive(path, at: (line: 3, col: 0))
    else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let onLine = fixture.wake { handle.window_cursor(editorWin).line() == 3 }
    XCTAssertTrue(
      onLine,
      "the picked result must open with the cursor on line 3; it is on line"
        + " \(handle.window_cursor(editorWin).line()). Line 1 means the cursor was dropped and"
        + " the file opened with a plain open")
  }
}
