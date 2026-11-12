// Paints a terminal pane's cursor hollow while the pane is not focused, over a real core and shell.

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

/// Only the focused terminal pane paints the frame's cursor style; every other terminal pane,
/// the drawer included, paints a hollow block. Focus needs the window to be key, which the
/// offscreen window reports through `OffscreenWindow.reportsKey`.
///
/// A hollow cursor is told from a block by its interior, which keeps the default background,
/// and from no cursor by its top band, which is the cursor colour. Every checked cursor sits in
/// row 0 of a backing-aligned view, so that 1-point band covers whole pixels.
///
/// `render()` redraws the whole view whatever was invalidated, so no pixel check shows that a
/// focus change repaints the cursor; `testAFocusChangeAloneMarksTheViewForDisplay` checks that
/// invalidation through its backing layer's `needsDisplay()`, with no wake.
@MainActor
final class TerminalCursorFocusTests: XCTestCase {

  /// `terminal_target::SPLIT_VERTICAL`.
  private let splitVertical: UInt8 = 2
  /// `terminal_cursor_style::BLOCK`.
  private let blockStyle: UInt8 = 0
  /// Sets a steady block cursor (DECSCUSR 2), prints `AB` and leaves the cursor at (2, 0).
  private let blockCommand = "printf '\\033[2 qAB'; exec sleep 600"

  /// Whether `frame`'s cursor is visible in row 0 at `col` as a block.
  private func blockCursor(_ frame: FfiTerminalFrame, at col: Int) -> Bool {
    frame.has_cursor() && frame.cursor_visible() && Int(frame.cursor_col()) == col
      && frame.cursor_row() == 0 && frame.cursor_style() == blockStyle
  }

  /// Where `frame`'s cursor stands, for a failure message.
  private func describeCursor(_ frame: FfiTerminalFrame) -> String {
    guard frame.has_cursor() else { return "no cursor" }
    return "cursor at (\(frame.cursor_col()), \(frame.cursor_row())), visible"
      + " \(frame.cursor_visible()), style \(frame.cursor_style())"
  }

  /// The colour `frame`'s cursor fills in and the frame's default background, failing the
  /// test when the two are near, since a cursor never drawn would then pass for one drawn.
  private func colours(
    of frame: FfiTerminalFrame, file: StaticString = #filePath, line: UInt = #line
  ) -> (cursor: NSColor, bg: NSColor) {
    let set = frame.cursor_color()
    let cursor = srgb(set.set ? set.rgb : frame.default_fg().rgb)
    let bg = srgb(frame.default_bg().rgb)
    XCTAssertGreaterThan(
      colorDistance(cursor, bg), 0.5,
      "premise: the cursor colour \(cursor) must be far from the default background \(bg), or"
        + " a cell with no cursor passes for a cursor's", file: file, line: line)
    return (cursor, bg)
  }

  /// Cell `(col, row)` in a terminal view's flipped coordinates.
  private func cellRect(_ metrics: CellMetrics, col: Int, row: Int) -> NSRect {
    NSRect(
      x: CGFloat(col) * metrics.cellWidth, y: CGFloat(row) * metrics.cellHeight,
      width: metrics.cellWidth, height: metrics.cellHeight)
  }

  /// The fraction of `rect`'s pixels within 0.1 of `color`.
  private func fraction(
    of color: NSColor, in rect: NSRect, of view: NSView, capture: RenderedView
  ) -> Double {
    let pixels = capture.colors(in: rect, of: view)
    guard !pixels.isEmpty else { return 0 }
    return Double(pixels.filter { colorDistance($0, color) <= 0.1 }.count) / Double(pixels.count)
  }

  /// Asserts the cursor at `cell` paints as a block: the cell's centre is the cursor colour.
  private func assertBlock(
    _ capture: RenderedView, view: TerminalPaneView, cell: NSRect, cursor: NSColor, bg: NSColor,
    _ when: String, file: StaticString = #filePath, line: UInt = #line
  ) {
    let centre = capture.color(at: NSPoint(x: cell.midX, y: cell.midY), in: view)
    XCTAssertLessThan(
      colorDistance(centre, cursor), 0.1,
      "\(when), the cursor must paint the frame's block, so the centre of its cell \(cell) must"
        + " be the cursor colour \(cursor), not \(centre). The default background \(bg) means the"
        + " cursor is drawn hollow, or not at all, although its pane is focused",
      file: file, line: line)
  }

  /// Asserts the cursor at `cell`, in row 0, paints as a hollow block: the cell's interior
  /// keeps the default background and its top band is the cursor colour.
  private func assertHollow(
    _ capture: RenderedView, view: TerminalPaneView, cell: NSRect, cursor: NSColor, bg: NSColor,
    _ when: String, file: StaticString = #filePath, line: UInt = #line
  ) {
    let backing = capture.root.convertToBacking(capture.root.convert(view.bounds, from: view))
    XCTAssertTrue(
      [backing.minY, backing.maxY].allSatisfy { abs($0 - $0.rounded()) < 0.01 },
      "premise: the terminal view must lie on whole backing pixels, or the 1-point top band of"
        + " a hollow cursor is antialiased; its edges are \(backing) in backing pixels",
      file: file, line: line)
    let interior = cell.insetBy(dx: 2, dy: 2)
    let inside = fraction(of: bg, in: interior, of: view, capture: capture)
    XCTAssertGreaterThanOrEqual(
      inside, 0.9,
      "\(when), the cursor must paint hollow, so at least 90% of its cell's interior \(interior)"
        + " must keep the default background \(bg), not \(inside). The cursor colour \(cursor)"
        + " there means the frame's block is painted although the pane is not focused",
      file: file, line: line)
    let band = NSRect(x: cell.minX + 2, y: cell.minY, width: cell.width - 4, height: 1)
    let edge = fraction(of: cursor, in: band, of: view, capture: capture)
    XCTAssertGreaterThanOrEqual(
      edge, 0.9,
      "\(when), a hollow cursor strokes its cell's outline, so at least 90% of the cell's top"
        + " band \(band) must be the cursor colour \(cursor), not \(edge). The default background"
        + " there means no cursor is drawn at all, where a hollow block belongs",
      file: file, line: line)
  }

  /// In a split beside the editor, the terminal pane's cursor paints as the frame's block
  /// while the pane is focused, hollow once `<C-w>h` focuses the editor, and as the block again
  /// once `<C-w>l` focuses the terminal pane.
  func testCursorIsHollowWhileAnotherPaneIsFocused() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    fixture.renderer.window.reportsKey = true
    let handle = fixture.handle
    let container = fixture.container
    let metrics = fixture.live.metrics

    let session = handle.open_terminal(handle.active_win(), splitVertical, blockCommand)
    XCTAssertNotEqual(session, 0, "premise: open_terminal must start a session in a split")
    guard session != 0 else { return }
    let opened = fixture.wake {
      container.paneCount == 2 && fixture.view(showing: session) != nil
        && container.activeTerminal == session
        && blockCursor(handle.terminal_frame(session), at: 2)
    }
    XCTAssertTrue(
      opened,
      "premise: the split must be a second, focused pane showing the session, whose cursor"
        + " stands after `AB` at (2, 0) as a block: \(container.paneCount) pane(s), active"
        + " terminal \(String(describing: container.activeTerminal)),"
        + " \(describeCursor(handle.terminal_frame(session)))")
    guard opened else { return }
    let (cursor, bg) = colours(of: handle.terminal_frame(session))
    let cell = cellRect(metrics, col: 2, row: 0)

    var capture = fixture.render()
    guard let view = fixture.view(showing: session) else {
      XCTFail("no terminal view shows session \(session) after a render wake")
      return
    }
    assertBlock(
      capture, view: view, cell: cell, cursor: cursor, bg: bg, "with the terminal pane focused")

    fixture.terminalChord("h", to: session)
    let left = fixture.wake { container.activeTerminal == nil }
    XCTAssertTrue(
      left,
      "premise: <C-w>h in the terminal pane must focus the editor: active terminal"
        + " \(String(describing: container.activeTerminal))")
    guard left else { return }
    XCTAssertTrue(
      blockCursor(handle.terminal_frame(session), at: 2),
      "premise: moving focus must leave the frame's cursor a block at (2, 0):"
        + " \(describeCursor(handle.terminal_frame(session)))")
    capture = fixture.render()
    assertHollow(
      capture, view: view, cell: cell, cursor: cursor, bg: bg, "with the editor pane focused")

    fixture.neovimChord("l")
    let back = fixture.wake { container.activeTerminal == session }
    XCTAssertTrue(
      back,
      "premise: <C-w>l in the editor must focus the terminal pane again: active terminal"
        + " \(String(describing: container.activeTerminal))")
    guard back else { return }
    capture = fixture.render()
    assertBlock(
      capture, view: view, cell: cell, cursor: cursor, bg: bg,
      "with the terminal pane focused again")
  }

  /// The active pane of a window that is not key is not focused: its cursor paints hollow
  /// while the window does not report key, and as the frame's block once it does again.
  func testCursorIsHollowWhileTheWindowIsNotKey() {
    guard let fixture = TerminalPaneFixture(command: blockCommand) else { return }
    defer { fixture.shutdown() }
    XCTAssertTrue(
      fixture.converge(),
      "premise: the container must host a terminal view and size the session: \(fixture.state)")
    let stands = fixture.live.waitFor(timeout: TerminalPaneFixture.timeout) {
      blockCursor(fixture.frame, at: 2)
    }
    XCTAssertTrue(
      stands,
      "premise: the cursor must stand after `AB` at (2, 0) as a block:"
        + " \(describeCursor(fixture.frame))")
    guard stands else { return }
    let (cursor, bg) = colours(of: fixture.frame)
    let cell = fixture.cellRect(col: 2, row: 0)

    var capture = fixture.render()
    guard let view = fixture.terminalViews.first else {
      XCTFail("no TerminalPaneView after a render wake: syncTerminalViews created none")
      return
    }
    assertBlock(
      capture, view: view, cell: cell, cursor: cursor, bg: bg, "with the window key")

    fixture.renderer.window.reportsKey = false
    capture = fixture.render()
    assertHollow(
      capture, view: view, cell: cell, cursor: cursor, bg: bg,
      "with the pane active but its window not key")

    fixture.renderer.window.reportsKey = true
    capture = fixture.render()
    assertBlock(
      capture, view: view, cell: cell, cursor: cursor, bg: bg, "with the window key again")
  }

  /// A focus change alone marks the terminal view for display, both when the pane loses focus
  /// and when it gains it back. No render wake runs between the change and the check, so no
  /// new frame or damage can have marked the view; on a display, nothing else repaints the
  /// cursor's row when only focus changes.
  ///
  /// A layer-backed view's own `needsDisplay` stays true in a window never ordered front, so
  /// the test reads its backing layer's `needsDisplay()` instead. `setNeedsDisplay(_:)` sets
  /// that flag, and `cacheDisplay(in:to:)`, which ends `render()`, clears it; the clearing is a
  /// premise measured here rather than assumed.
  func testAFocusChangeAloneMarksTheViewForDisplay() {
    guard let fixture = TerminalPaneFixture(command: blockCommand) else { return }
    defer { fixture.shutdown() }
    XCTAssertTrue(
      fixture.converge(),
      "premise: the container must host a terminal view and size the session: \(fixture.state)")
    let stands = fixture.live.waitFor(timeout: TerminalPaneFixture.timeout) {
      blockCursor(fixture.frame, at: 2)
    }
    XCTAssertTrue(
      stands,
      "premise: the cursor must stand after `AB` at (2, 0) as a block:"
        + " \(describeCursor(fixture.frame))")
    guard stands else { return }

    _ = fixture.render()
    guard let view = fixture.terminalViews.first else {
      XCTFail("no TerminalPaneView after a render wake: syncTerminalViews created none")
      return
    }
    XCTAssertTrue(
      view.focused,
      "premise: with the window key, the active terminal pane's view must be focused")
    XCTAssertNotNil(
      view.layer,
      "premise: the terminal view must be layer-backed under the layer-backed container,"
        + " or there is no layer whose needsDisplay() can show an invalidation")
    guard view.layer != nil else { return }
    XCTAssertEqual(
      view.layer?.needsDisplay(), false,
      "premise: render()'s cacheDisplay(in:to:) must leave the view's layer needing no display,"
        + " or a later needsDisplay() cannot tell a focus change's invalidation from a stale one")

    fixture.renderer.window.reportsKey = false
    fixture.container.syncTerminalFocus()
    XCTAssertFalse(
      view.focused,
      "premise: syncTerminalFocus() with the window not key must unfocus the view")
    XCTAssertEqual(
      view.layer?.needsDisplay(), true,
      "losing focus must mark the view for display. No wake ran, so only the change of"
        + " `focused` can have marked it; unmarked, a display never repaints the cursor hollow")

    // The renderer's render(), not the fixture's, so that no wake runs.
    _ = fixture.renderer.render()
    XCTAssertEqual(
      view.layer?.needsDisplay(), false,
      "premise: render()'s cacheDisplay(in:to:) must leave the view's layer needing no display"
        + " again")
    fixture.renderer.window.reportsKey = true
    fixture.container.syncTerminalFocus()
    XCTAssertTrue(
      view.focused, "premise: syncTerminalFocus() with the window key must focus the view again")
    XCTAssertEqual(
      view.layer?.needsDisplay(), true,
      "gaining focus must mark the view for display. No wake ran, so only the change of"
        + " `focused` can have marked it; unmarked, a display keeps the hollow cursor although"
        + " the pane is focused again")
  }

  /// The drawer's cursor paints as the frame's block while the drawer is focused, and hollow
  /// once `<C-w>k` focuses the editor above it.
  func testTheDrawerFollowsTheSameRule() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    fixture.renderer.window.reportsKey = true
    let handle = fixture.handle
    let container = fixture.container
    let metrics = fixture.live.metrics

    handle.toggle_terminal()
    var drawer: UInt64 = 0
    let opened = fixture.wake {
      drawer = Array(handle.live_terminals()).first ?? 0
      guard drawer != 0, let view = fixture.view(showing: drawer) else { return false }
      return fixture.isInDrawerStrip(view) && container.activeTerminal == drawer
    }
    XCTAssertTrue(
      opened,
      "premise: toggle_terminal() must show the drawer's session in the strip, focused: live"
        + " \(Array(handle.live_terminals())), active terminal"
        + " \(String(describing: container.activeTerminal))")
    guard opened else { return }

    // Clears the screen and homes the cursor, then prints a marker the prompt follows in row 0.
    // A typed line echoes after the prompt, so a row 0 that starts with the marker ran it.
    fixture.runLine("printf '\\033[2 q\\033[2J\\033[HMK'", in: drawer)
    var col = 0
    let prompted = fixture.wake {
      let frame = handle.terminal_frame(drawer)
      col = Int(frame.cursor_col())
      let row0 = frame.row_cells(0).map { $0.text().toString() }.joined()
      return row0.hasPrefix("MK") && col > 2 && blockCursor(frame, at: col)
    }
    XCTAssertTrue(
      prompted,
      "premise: after the clear, row 0 must hold the marker then the prompt, with the cursor"
        + " after them as a block: \(describeCursor(handle.terminal_frame(drawer))); the drawer"
        + " reads:\n\(fixture.text(of: drawer))")
    guard prompted else { return }
    let (cursor, bg) = colours(of: handle.terminal_frame(drawer))
    let cell = cellRect(metrics, col: col, row: 0)

    var capture = fixture.render()
    guard let view = fixture.view(showing: drawer) else {
      XCTFail("no terminal view shows the drawer's session \(drawer) after a render wake")
      return
    }
    assertBlock(
      capture, view: view, cell: cell, cursor: cursor, bg: bg, "with the drawer focused")

    fixture.terminalChord("k", to: drawer)
    let left = fixture.wake { container.activeTerminal == nil }
    XCTAssertTrue(
      left,
      "premise: <C-w>k in the drawer must focus the editor above it: active terminal"
        + " \(String(describing: container.activeTerminal))")
    guard left else { return }
    XCTAssertTrue(
      blockCursor(handle.terminal_frame(drawer), at: col),
      "premise: moving focus must leave the drawer's cursor a block at (\(col), 0):"
        + " \(describeCursor(handle.terminal_frame(drawer)))")
    capture = fixture.render()
    assertHollow(
      capture, view: view, cell: cell, cursor: cursor, bg: bg,
      "with the editor focused above the drawer")
  }
}
