// Checks that Neovim panes stay painted beside an open terminal view, and after it closes.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// Each test marks the editor's rows with `TerminalViewFixture.mark` before any terminal
/// opens, then samples the container where only the editor paints: beside the terminal
/// view while it is open, and inside the view's last frame once it has closed.
@MainActor
final class TerminalNeighbourPaneTests: XCTestCase {

  /// One poll of a marked-pixel check: whether it held, and the last count and rect read.
  private struct Sample {
    var held = false
    var count = 0
    var rect = NSRect.zero
  }

  /// Wakes and renders `host` until the rect `rect` answers holds at least the fixture's
  /// `markThreshold` marked pixels, or the timeout passes. A `nil` rect counts as unmarked.
  private func sampleMarked(
    _ fixture: TerminalViewFixture, in host: PaneContainerView, renderer: OffscreenRenderer,
    rect: () -> NSRect?
  ) -> Sample {
    var sample = Sample()
    sample.held = fixture.wake(host) {
      guard let sampled = rect() else {
        sample.count = 0
        sample.rect = .zero
        return false
      }
      let capture = renderer.render()
      sample.rect = sampled
      sample.count = fixture.markedPixels(in: sampled, of: host, capture)
      return sample.count >= fixture.markThreshold
    }
    return sample
  }

  /// The frames of `host`'s terminal views, for failure messages.
  private func frames(_ fixture: TerminalViewFixture, in host: PaneContainerView) -> String {
    "\(fixture.terminalViews(in: host).map { ($0.session, $0.frame) })"
  }

  /// Marks Neovim's current buffer and waits until `host` paints the mark anywhere: the
  /// premise every check here rests on. False, with the test failed, when it never does.
  private func markEditor(
    _ fixture: TerminalViewFixture, in host: PaneContainerView, renderer: OffscreenRenderer
  ) -> Bool {
    fixture.markEditor()
    let marked = sampleMarked(fixture, in: host, renderer: renderer) { host.bounds }
    XCTAssertTrue(
      marked.held,
      "premise: after markEditor() the container must paint crimson rows; it painted"
        + " \(marked.count) crimson pixels of \(fixture.markThreshold) needed in \(marked.rect)."
        + " None means the :lua line never ran (typed into the buffer, or refused) or"
        + " line_hl_group paints no cell background; nothing below would be about terminals")
    return marked.held
  }

  /// T1: the editor above an open drawer stays painted (T1a), and the strip the drawer
  /// covered paints the editor once it hides (T1b).
  func testTheEditorStaysPaintedWhileTheDrawerIsOpenAndAfterItHides() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard markEditor(fixture, in: container, renderer: fixture.renderer) else { return }

    handle.toggle_terminal()
    var session: UInt64 = 0
    let opened = fixture.wake {
      session = Array(handle.live_terminals()).first ?? 0
      guard session != 0, let view = fixture.view(showing: session) else { return false }
      return fixture.isInDrawerStrip(view)
    }
    XCTAssertTrue(
      opened,
      "premise: toggle_terminal() must show the drawer's session in the bottom strip; live"
        + " \(Array(handle.live_terminals())), terminal views \(frames(fixture, in: container))")
    guard opened else { return }

    let above = sampleMarked(fixture, in: container, renderer: fixture.renderer) {
      guard let view = fixture.view(showing: session) else { return nil }
      return NSRect(x: 0, y: 0, width: container.bounds.width, height: view.frame.minY)
    }
    XCTAssert(
      above.held,
      "T1a: with the drawer open, the editor above it must stay painted: \(above.count) crimson"
        + " pixels of \(fixture.markThreshold) needed in \(above.rect), terminal views"
        + " \(frames(fixture, in: container)). None means something painted over the editor"
        + " pane, such as a terminal view filling a dirty rect past its own bounds, or the"
        + " container skipped the editor's grid; a few means it covered part of the pane")

    let lastFrame = fixture.view(showing: session)?.frame ?? .zero
    handle.toggle_terminal()
    let hidden = fixture.wake {
      fixture.window(showing: session) == 0 && container.paneCount == 1
    }
    XCTAssertTrue(
      hidden,
      "premise: a second toggle_terminal() must close the drawer's window; window"
        + " \(fixture.window(showing: session)) still shows session \(session)")
    guard hidden else { return }

    let inside = sampleMarked(fixture, in: container, renderer: fixture.renderer) { lastFrame }
    XCTAssert(
      inside.held,
      "T1b: once the drawer hides, the editor must paint where the drawer view was:"
        + " \(inside.count) crimson pixels of \(fixture.markThreshold) needed in \(inside.rect),"
        + " terminal views \(frames(fixture, in: container)). A view still listed there means"
        + " syncTerminalViews kept the hidden drawer's view; none listed means the editor grid"
        + " was not repainted or resized over the strip")
  }

  /// T2: the editor beside a terminal split stays painted (T2a), and paints over the
  /// terminal's half once `<C-w>c` closes it (T2b).
  func testTheEditorStaysPaintedBesideATerminalSplitAndAfterItCloses() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    guard markEditor(fixture, in: container, renderer: fixture.renderer) else { return }

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
        + " \(container.paneCount) pane(s), terminal views \(frames(fixture, in: container))")
    guard opened, let opening = fixture.view(showing: session) else { return }
    XCTAssertGreaterThan(
      opening.frame.minX, container.bounds.width / 3,
      "premise: the terminal must be the right-hand pane, so the editor lies left of it;"
        + " terminal views \(frames(fixture, in: container))")

    let beside = sampleMarked(fixture, in: container, renderer: fixture.renderer) {
      guard let view = fixture.view(showing: session) else { return nil }
      return NSRect(x: 0, y: 0, width: view.frame.minX, height: container.bounds.height)
    }
    XCTAssert(
      beside.held,
      "T2a: with a terminal split open, the editor left of it must stay painted:"
        + " \(beside.count) crimson pixels of \(fixture.markThreshold) needed in \(beside.rect),"
        + " terminal views \(frames(fixture, in: container)). None means something painted over"
        + " the editor pane, such as the terminal view filling a dirty rect past its own bounds;"
        + " a few means it covered part of the pane")

    let lastFrame = fixture.view(showing: session)?.frame ?? .zero
    fixture.terminalChord("c", to: session)
    let closed = fixture.wake {
      fixture.window(showing: session) == 0 && container.paneCount == 1
    }
    XCTAssertTrue(
      closed,
      "premise: <C-w>c in the terminal pane must close its window: \(container.paneCount)"
        + " pane(s), window \(fixture.window(showing: session)) shows session \(session)")
    guard closed else { return }

    let inside = sampleMarked(fixture, in: container, renderer: fixture.renderer) { lastFrame }
    XCTAssert(
      inside.held,
      "T2b: once the terminal pane closes, the editor must paint where its view was:"
        + " \(inside.count) crimson pixels of \(fixture.markThreshold) needed in \(inside.rect),"
        + " terminal views \(frames(fixture, in: container)). A view still listed there means"
        + " syncTerminalViews kept the closed pane's view; none listed means the editor grid"
        + " was not repainted across the freed half")
  }

  /// T3: a terminal split in a new tab leaves that tab's editor painted (T3a), the first
  /// tab's editor paints when it is current again (T3b), and the new tab's editor paints
  /// over the terminal's half once `<C-w>c` closes it (T3c).
  func testTheEditorStaysPaintedBesideATerminalInANewTab() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard markEditor(fixture, in: container, renderer: fixture.renderer) else { return }

    let t1 = handle.current_tabpage()
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
    let bound = fixture.wake(second) {
      second.paneCount == 1 && second.activeWin == handle.active_win()
    }
    XCTAssertTrue(
      bound,
      "premise: the second tab's container must bind its first pane to window"
        + " \(handle.active_win()); it is on \(second.activeWin)")
    guard bound, markEditor(fixture, in: second, renderer: secondRenderer) else { return }

    fixture.neovimChord("v")
    let split = fixture.wake(second) { second.paneCount == 2 }
    XCTAssertTrue(
      split, "premise: <C-w>v must split the second tab's editor: \(second.paneCount) pane(s)")
    guard split else { return }
    fixture.typeEx("terminal")
    var session: UInt64 = 0
    let opened = fixture.wake(second) {
      session = fixture.terminalViews(in: second).first?.session ?? 0
      return session != 0 && second.paneCount == 2 && fixture.window(showing: session) != 0
    }
    XCTAssertTrue(
      opened,
      "premise: a typed :terminal must turn the second tab's new pane into a terminal view:"
        + " \(second.paneCount) pane(s), terminal views \(frames(fixture, in: second))")
    guard opened else { return }

    let beside = sampleMarked(fixture, in: second, renderer: secondRenderer) {
      guard let view = fixture.view(showing: session, in: second) else { return nil }
      return NSRect(x: 0, y: 0, width: view.frame.minX, height: second.bounds.height)
    }
    XCTAssert(
      beside.held,
      "T3a: in the new tab, the editor left of its terminal split must stay painted:"
        + " \(beside.count) crimson pixels of \(fixture.markThreshold) needed in \(beside.rect),"
        + " terminal views \(frames(fixture, in: second)). None means something painted over"
        + " the editor pane, such as the terminal view filling a dirty rect past its bounds")

    container.markRefocusRebind()
    handle.set_current_tabpage(t1)
    let backInFirst = fixture.wake { handle.active_tabpage() == t1 }
    XCTAssertTrue(
      backInFirst,
      "premise: set_current_tabpage(\(t1)) must make the first tab current: tabpage"
        + " \(handle.active_tabpage())")
    if backInFirst {
      let first = sampleMarked(fixture, in: container, renderer: fixture.renderer) {
        container.bounds
      }
      XCTAssert(
        first.held,
        "T3b: back in the first tab, which shows no terminal, its editor must paint:"
          + " \(first.count) crimson pixels of \(fixture.markThreshold) needed in \(first.rect),"
          + " terminal views \(frames(fixture, in: container)). None means the container skips"
          + " its editor grid while another tab's session is live, or the refocus rebind left"
          + " the pane on a stale grid")
    }

    second.markRefocusRebind()
    handle.set_current_tabpage(t2)
    let backInSecond = fixture.wake(second) {
      handle.active_tabpage() == t2 && fixture.view(showing: session, in: second) != nil
    }
    XCTAssertTrue(
      backInSecond,
      "premise: set_current_tabpage(\(t2)) must make the second tab current with its terminal"
        + " view: tabpage \(handle.active_tabpage()), terminal views"
        + " \(frames(fixture, in: second))")
    guard backInSecond else { return }

    let lastFrame = fixture.view(showing: session, in: second)?.frame ?? .zero
    fixture.terminalChord("c", to: session)
    let closed = fixture.wake(second) {
      fixture.window(showing: session) == 0 && second.paneCount == 1
    }
    XCTAssertTrue(
      closed,
      "premise: <C-w>c in the second tab's terminal pane must close its window:"
        + " \(second.paneCount) pane(s), window \(fixture.window(showing: session))")
    guard closed else { return }

    let inside = sampleMarked(fixture, in: second, renderer: secondRenderer) { lastFrame }
    XCTAssert(
      inside.held,
      "T3c: once the second tab's terminal pane closes, its editor must paint where the view"
        + " was: \(inside.count) crimson pixels of \(fixture.markThreshold) needed in"
        + " \(inside.rect), terminal views \(frames(fixture, in: second)). A view still listed"
        + " means syncTerminalViews kept the closed pane's view; none listed means the editor"
        + " grid was not repainted across the freed half")
  }
}
