// Split Horizontal and Split Vertical through PaneContainerView.splitActivePane, on a live core.

import AppKit
import WebKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// The palette's Split Horizontal and Split Vertical rows reach
/// `PaneContainerView.splitActivePane(_:)` through `WolfWindowController.splitPane(_:)`. No
/// palette or key window renders here, so each test calls `splitActivePane(_:)` over a real core
/// and Neovim and reads the result from `paneLayout`, `leafContents()` and the bridge.
///
/// Split Horizontal is Vim's `:split`, panes stacked, so the new leaf and its origin must share a
/// `.horizontal` split; Split Vertical is `:vsplit`, side by side, under a `.vertical` one.
///
/// A failure prefixed `premise:` is a setup step that did not happen, before any split ran.
@MainActor
final class SplitCommandTests: XCTestCase {

  /// The orientation of the split that holds `leaf` as a direct child, with that split's
  /// children; `nil` when `leaf` is the root or absent.
  private func parent(of leaf: PaneId, in layout: PaneLayout) -> (
    orientation: Orientation, children: [PaneLayout]
  )? {
    guard case .split(let orientation, let children, _) = layout else { return nil }
    if children.contains(.leaf(leaf)) { return (orientation, children) }
    for child in children {
      if let found = parent(of: leaf, in: child) { return found }
    }
    return nil
  }

  /// Every leaf of `layout`, in order.
  private func leaves(of layout: PaneLayout) -> [PaneId] {
    switch layout {
    case .leaf(let id): return [id]
    case .split(_, let children, _): return children.flatMap { leaves(of: $0) }
    }
  }

  /// Asserts the tree's one new leaf, absent from `before`, sits beside `origin` under a split
  /// of `orientation`, and returns it; `nil` with the test failed otherwise.
  private func newLeafBeside(
    _ origin: PaneId, before: [PaneId], in container: PaneContainerView,
    orientation: Orientation, file: StaticString = #filePath, line: UInt = #line
  ) -> PaneId? {
    let layout = container.paneLayout
    let added = leaves(of: layout).filter { !before.contains($0) }
    guard added.count == 1, let new = added.first else {
      XCTFail(
        "the split must add exactly one leaf to the tree, not \(added.count): layout \(layout)",
        file: file, line: line)
      return nil
    }
    let holder = parent(of: new, in: layout)
    XCTAssertEqual(
      holder?.orientation, orientation,
      "the new leaf must sit under a \(orientation) split: layout \(layout). The other"
        + " orientation means Split Horizontal and Split Vertical are swapped on the way to core",
      file: file, line: line)
    XCTAssertTrue(
      holder?.children.contains(.leaf(origin)) ?? false,
      "the new leaf must be a sibling of the split pane's leaf \(origin): layout \(layout). Any"
        + " other parent means the command split a pane other than the view's active one",
      file: file, line: line)
    return new
  }

  /// Splits the fixture's one editor pane and checks the new pane is a window on its buffer.
  private func checkEditorSplit(_ orientation: Orientation) {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let originWin = container.activeWin
    guard case .leaf(let origin) = container.paneLayout else {
      XCTFail("premise: the fixture must start with one leaf: \(container.paneLayout)")
      return
    }
    let originPath = handle.win_buffer(originWin).abs_path().toString()
    XCTAssertFalse(
      originPath.isEmpty,
      "premise: the editor window \(originWin) must show a named buffer, so a new window's"
        + " buffer can be told from it")

    container.splitActivePane(orientation)
    let split = fixture.wake {
      container.paneCount == 2 && container.activeWin != originWin && container.activeWin != 0
        && handle.win_buffer(container.activeWin).reported()
    }
    XCTAssertTrue(
      split,
      "splitActivePane(\(orientation)) must add a pane and make it active: \(container.paneCount)"
        + " pane(s), active window \(container.activeWin) (the origin is \(originWin)). One pane"
        + " means the split never reached Neovim")
    guard split else { return }
    guard
      newLeafBeside(origin, before: [origin], in: container, orientation: orientation) != nil
    else { return }

    let newWin = container.activeWin
    let shown = handle.win_buffer(newWin)
    XCTAssertEqual(
      shown.abs_path().toString(), originPath,
      "the new window \(newWin) must show the origin's buffer \(originPath), as <C-w>s and <C-w>v"
        + " do. Another buffer, or an unnamed one, means the split pane is not a window on the"
        + " same buffer")
    XCTAssertEqual(
      shown.terminal(), 0, "an editor pane's split must not be a terminal pane")
    XCTAssertEqual(
      handle.active_win(), newWin,
      "Neovim's current window must be the new pane's \(newWin), not \(handle.active_win())")
  }

  /// Splits a terminal pane on `/bin/sh` and checks the new pane is a terminal on a new session.
  private func checkTerminalSplit(_ orientation: Orientation) {
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
    guard opened, case .leaf(let origin) = container.paneLayout else { return }

    container.splitActivePane(orientation)
    var live: [UInt64] = []
    let split = fixture.wake {
      live = Array(handle.live_terminals())
      return container.paneCount == 2 && live.count == 2 && fixture.terminalViews.count == 2
    }
    XCTAssertTrue(
      split,
      "splitActivePane(\(orientation)) in a terminal pane must add a terminal pane on a new"
        + " session: \(container.paneCount) pane(s), live \(live), terminal views"
        + " \(fixture.terminalViews.map { $0.session }). Two panes on one session mean the split"
        + " went to Neovim as an editor split rather than through core's router")
    guard split else { return }
    guard
      let new = newLeafBeside(origin, before: [origin], in: container, orientation: orientation)
    else { return }

    let contents = container.leafContents()
    for leaf in [origin, new] {
      if case .terminal? = contents[leaf] { continue }
      XCTFail(
        "leaf \(leaf) must be a terminal pane, not \(String(describing: contents[leaf])):"
          + " a terminal pane's split must give a terminal pane")
    }
    let shown = Set(fixture.terminalViews.map { $0.session })
    XCTAssertEqual(
      shown, Set(live),
      "the two terminal views must show the two live sessions \(live), not \(shown)")
    XCTAssertTrue(
      live.contains(session),
      "the split pane's session \(session) must stay live: \(live)")
  }

  /// Splits the preview fixture's preview pane and checks the new pane previews the same source.
  private func checkPreviewSplit(_ orientation: Orientation) {
    guard let fixture = PreviewViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    let before = container.leafContents()
    let previewLeaves = before.filter {
      if case .preview = $0.value { return true }
      return false
    }
    guard previewLeaves.count == 1, let origin = previewLeaves.first?.key,
      case .preview(let source)? = previewLeaves.first?.value
    else {
      XCTFail("premise: exactly one leaf must be a preview: \(before)")
      return
    }
    XCTAssertEqual(
      (source as NSString).resolvingSymlinksInPath,
      (fixture.probePath as NSString).resolvingSymlinksInPath,
      "premise: the preview leaf must preview probe.md")
    let beforeLeaves = leaves(of: container.paneLayout)

    container.splitActivePane(orientation)
    var webViews = 0
    let split = fixture.base.wake {
      webViews = container.subviews.compactMap { $0 as? WKWebView }.count
      return container.paneCount == 3 && webViews == 2
    }
    XCTAssertTrue(
      split,
      "splitActivePane(\(orientation)) in a preview pane must add a pane with a web view of its"
        + " own: \(container.paneCount) pane(s), \(webViews) web view(s). Three panes with one"
        + " web view mean the new window is not reported as a preview")
    guard split else { return }
    guard
      let new = newLeafBeside(
        origin, before: beforeLeaves, in: container, orientation: orientation)
    else { return }

    let after = container.leafContents()
    XCTAssertEqual(
      after[new], .preview(source: source),
      "the new pane must preview the same source \(source), not"
        + " \(String(describing: after[new]))")
    XCTAssertEqual(
      after[origin], .preview(source: source), "the split preview pane must keep its preview")
  }

  /// Split Horizontal in an editor pane stacks a new pane on the same buffer and makes it active.
  func testSplitHorizontalSplitsAnEditorPaneOnItsBuffer() {
    checkEditorSplit(.horizontal)
  }

  /// Split Vertical in an editor pane puts a new pane on the same buffer beside it.
  func testSplitVerticalSplitsAnEditorPaneOnItsBuffer() {
    checkEditorSplit(.vertical)
  }

  /// Split Horizontal in a terminal pane stacks a terminal pane on a new session.
  func testSplitHorizontalInATerminalPaneStartsANewSession() {
    checkTerminalSplit(.horizontal)
  }

  /// Split Vertical in a terminal pane puts a terminal pane on a new session beside it.
  func testSplitVerticalInATerminalPaneStartsANewSession() {
    checkTerminalSplit(.vertical)
  }

  /// Split Horizontal in a preview pane stacks a preview of the same source.
  func testSplitHorizontalInAPreviewPaneShowsTheSameSource() {
    checkPreviewSplit(.horizontal)
  }

  /// Split Vertical in a preview pane puts a preview of the same source beside it.
  func testSplitVerticalInAPreviewPaneShowsTheSameSource() {
    checkPreviewSplit(.vertical)
  }

  /// Both split commands do nothing from the focused drawer: no pane and no session.
  func testSplitCommandsInTheDrawerDoNothing() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let editorWin = handle.active_win()

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
      "premise: toggle_terminal() must show the drawer's session in the strip, focused in the"
        + " view and in Neovim: live \(Array(handle.live_terminals())), active terminal"
        + " \(String(describing: container.activeTerminal)), Neovim's window"
        + " \(handle.active_win())")
    guard opened else { return }

    container.splitActivePane(.horizontal)
    container.splitActivePane(.vertical)
    // Queued behind both splits on core's input channel, so its landing means both were handled.
    handle.focus_win(editorWin)
    let handled = fixture.wake { handle.active_win() == editorWin }
    XCTAssertTrue(
      handled,
      "premise: focus_win(\(editorWin)) queued after the splits must land: Neovim's window"
        + " \(handle.active_win())")
    fixture.live.waitFor(timeout: 1.5) {
      container.onRenderWake()
      return false
    }

    XCTAssertEqual(
      Array(handle.live_terminals()), [session],
      "the split commands in the drawer must start no session: live"
        + " \(Array(handle.live_terminals())). A second session means core's router split the"
        + " drawer, which the front-end then dropped")
    XCTAssertEqual(
      container.paneCount, 1, "the split commands in the drawer must add no pane to the tree")
  }

  /// While Neovim's current window lags the view's active pane, as during a focus move in
  /// flight, the split acts on the pane the view shows active.
  func testSplitActsOnTheViewsActivePaneWhileNeovimLags() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let winA = container.activeWin
    guard case .leaf(let leafA) = container.paneLayout else {
      XCTFail("premise: the fixture must start with one leaf: \(container.paneLayout)")
      return
    }

    fixture.neovimChord("v")
    let split = fixture.wake {
      container.paneCount == 2 && container.activeWin != winA && container.activeWin != 0
    }
    XCTAssertTrue(
      split,
      "premise: <C-w>v must split the editor into two panes with the new one active:"
        + " \(container.paneCount) pane(s), active window \(container.activeWin)")
    guard split else { return }
    let winB = container.activeWin
    guard let leafB = leaves(of: container.paneLayout).first(where: { $0 != leafA }) else {
      XCTFail("premise: the split must leave a second leaf: \(container.paneLayout)")
      return
    }

    handle.focus_win(winA)
    let backOnA = fixture.wake { container.activeWin == winA && handle.active_win() == winA }
    XCTAssertTrue(
      backOnA,
      "premise: focus_win(\(winA)) must make A the active pane: view \(container.activeWin),"
        + " Neovim \(handle.active_win())")
    guard backOnA else { return }

    // No wake between the two: the view's active pane stays A, Neovim's is B.
    handle.focus_win(winB)
    container.splitActivePane(.horizontal)
    let done = fixture.wake { container.paneCount == 3 }
    XCTAssertTrue(done, "the split must add a third pane: \(container.paneCount)")
    guard done else { return }

    let layout = container.paneLayout
    XCTAssertEqual(
      parent(of: leafA, in: layout)?.orientation, .horizontal,
      "the split must stack a pane under A, the view's active pane: layout \(layout). A left"
        + " under the side-by-side split means the split ran on Neovim's current window, B,"
        + " with no focus_win first")
    XCTAssertEqual(
      parent(of: leafB, in: layout)?.orientation, .vertical,
      "B, Neovim's current window when the split ran, must not be split: layout \(layout)")
  }
}
