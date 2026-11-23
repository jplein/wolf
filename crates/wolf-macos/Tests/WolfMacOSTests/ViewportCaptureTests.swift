// The cursor capture of PaneContainerView.paneViews over a live core and Neovim.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// The cursor and top-line capture of `specs/application-state.md` §4 ("Cursor and scroll
/// capture") in a `PaneContainerView` over a real core and Neovim. A pane focus change captures
/// the pane it leaves, and `captureViewports()`, which the quit runs for every view, captures
/// every edit pane at once. Both read each window through `window_cursor`, so the expected top
/// line is the one Neovim reports, never a constant.
///
/// Each test edits a 200-line file in pane A, splits it vertically into pane B, which shows the
/// same buffer, and moves B's cursor with `:150`. That scrolls B, so its top line is neither 1
/// nor 150, which a premise checks: a capture that saves no top line, or the cursor line as the
/// top line, then shows.
///
/// A failure prefixed `premise:` is a setup step that did not happen.
@MainActor
final class ViewportCaptureTests: XCTestCase {

  /// The panes of `splitOnFarLine(_:)`, and B's top line as Neovim reports it.
  private struct Split {
    let idA: PaneId
    let wA: Int64
    let idB: PaneId
    let wB: Int64
    let topB: UInt32
  }

  /// The line `:150` moves pane B's cursor to.
  private let farLine: UInt32 = 150

  // MARK: - M1: a pane focus change

  /// Moving the focus from B to A captures B's cursor and top line, and nothing captures them
  /// before that.
  func testAPaneFocusChangeCapturesTheCursorAndTopLine() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let s = splitOnFarLine(fixture) else { return }

    var early: [String] = []
    settle(fixture, 1) {
      if let view = container.paneViews[s.idB], view.cursor?.line == farLine {
        early.append("\(view)")
      }
    }
    XCTAssertEqual(
      early, [],
      "with no focus change, B's cursor on line \(farLine) must not be captured yet; a capture"
        + " at a plain wake means the view captures on every wake rather than at §4's moments")

    handle.focus_win(s.wA)
    let moved = fixture.wake { container.activeWin == s.wA && handle.active_win() == s.wA }
    XCTAssertTrue(
      moved,
      "premise: focus_win(\(s.wA)) must make A active in Neovim and the view: Neovim"
        + " \(handle.active_win()), view \(container.activeWin)")
    guard moved else { return }

    let want = SavedPaneView(cursor: .init(line: farLine, col: 0), topLine: s.topB)
    XCTAssertEqual(
      container.paneViews[s.idB], want,
      "leaving B for A must capture B's cursor (\(farLine), 0) and its top line \(s.topB), as"
        + " Neovim reports them. nil or an older line means setActivePane does not capture the"
        + " pane it leaves; another top line that it is not read from B's own window")
  }

  // MARK: - M2: the quit's capture

  /// `captureViewports()` captures every edit pane, the active one included, with no focus
  /// change.
  func testCaptureViewportsReadsEveryEditPaneAsTheQuitDoes() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    guard let s = splitOnFarLine(fixture) else { return }
    XCTAssertNotEqual(
      container.paneViews[s.idB]?.cursor?.line, farLine,
      "premise: B's cursor on line \(farLine) must not be captured before captureViewports(),"
        + " or the capture below is not what records it")

    container.captureViewports()
    XCTAssertEqual(
      container.paneViews[s.idB],
      SavedPaneView(cursor: .init(line: farLine, col: 0), topLine: s.topB),
      "captureViewports() must capture B, the active pane, at (\(farLine), 0) with Neovim's top"
        + " line \(s.topB). nil or no cursor means it skips the active pane; a top line of"
        + " \(farLine) that the top line is taken from the cursor line")
    XCTAssertEqual(
      container.paneViews[s.idA],
      SavedPaneView(cursor: .init(line: 1, col: 0), topLine: 1),
      "captureViewports() must capture A, which never moved, at (1, 0) with top line 1; B's"
        + " values here mean every pane is read from the current window")
  }

  // MARK: - Helpers

  /// Edits a 200-line file in the first pane A, splits it vertically into an active pane B and
  /// runs `:150` in B, waiting until `window_cursor` reports it; `nil`, with the test failed,
  /// when a step does not happen.
  private func splitOnFarLine(
    _ fixture: TerminalViewFixture, file: StaticString = #filePath, line: UInt = #line
  ) -> Split? {
    let handle = fixture.handle
    let container = fixture.container
    guard let dir = LiveCore.scratchDirectory(file: file, line: line) else { return nil }
    let path = (dir as NSString).appendingPathComponent("numbered.md")
    do {
      try (1...200).map { "row \($0)\n" }.joined().write(
        toFile: path, atomically: true, encoding: .utf8)
    } catch {
      XCTFail("premise: could not write \(path): \(error)", file: file, line: line)
      return nil
    }

    let wA = container.activeWin
    fixture.typeEx("edit \(path)")
    let edited = fixture.wake {
      handle.win_buffer(wA).abs_path().toString().hasSuffix("/numbered.md")
    }
    guard edited, let idA = container.activePaneId else {
      XCTFail(
        "premise: :edit must show numbered.md in the first pane: window \(wA) shows"
          + " \(handle.win_buffer(wA).abs_path().toString()), active pane"
          + " \(String(describing: container.activePaneId))", file: file, line: line)
      return nil
    }

    container.splitActivePane(.vertical)
    let split = fixture.wake {
      container.paneCount == 2 && ![0, wA].contains(container.activeWin)
    }
    guard split, let idB = container.activePaneId, idB != idA else {
      XCTFail(
        "premise: a vertical split must add an active pane B: \(container.paneCount) pane(s),"
          + " active window \(container.activeWin)", file: file, line: line)
      return nil
    }
    let wB = container.activeWin

    fixture.typeEx("\(farLine)")
    var cursor = handle.window_cursor(wB)
    let moved = fixture.wake {
      cursor = handle.window_cursor(wB)
      return cursor.found() && cursor.line() == farLine
    }
    let topB = cursor.top_line()
    guard moved, cursor.col() == 0, topB != 1, topB != farLine else {
      XCTFail(
        "premise: :\(farLine) must move B's cursor to (\(farLine), 0) and scroll B to a top line"
          + " other than 1 and \(farLine): found \(cursor.found()), line \(cursor.line()), col"
          + " \(cursor.col()), top line \(topB)", file: file, line: line)
      return nil
    }
    return Split(idA: idA, wA: wA, idB: idB, wB: wB, topB: topB)
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
}
