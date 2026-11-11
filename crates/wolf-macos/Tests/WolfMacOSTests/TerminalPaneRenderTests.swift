// Renders a terminal pane over a real core and /bin/sh session and checks the pixels it drew.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A `PaneContainerView` over a live core whose current window shows one terminal session,
/// hosted offscreen. `TerminalLighthouseColourTests` reuses it over its own `init.lua`.
///
/// Premises, each checked where a test depends on it rather than assumed: the session's
/// default colours are far from the colours its program prints, a session starts at 80 x 24,
/// and the window size below gives a content area of another size.
@MainActor
final class TerminalPaneFixture {
  /// The hosting window's size in points: well over 80 x 24 cells of any monospace font.
  static let windowSize = NSSize(width: 960, height: 640)
  /// How long a wake, a publish or a resize gets to take effect.
  static let timeout: TimeInterval = 15

  let live: LiveCore
  let win: Int64
  let session: UInt64
  let container: PaneContainerView
  let renderer: OffscreenRenderer

  /// Starts a core, over `initLua` when given, opens `command` in its current window and hosts
  /// a container; `nil`, with the test failed, when any step does not happen.
  init?(
    command: String, initLua: String? = nil, file: StaticString = #filePath, line: UInt = #line
  ) {
    guard let live = LiveCore.start(initLua: initLua, file: file, line: line) else { return nil }
    self.live = live
    let handle = live.handle
    guard live.waitFor(timeout: Self.timeout, { handle.active_win() != 0 }) else {
      XCTFail("the core never named an active window", file: file, line: line)
      live.shutdown()
      return nil
    }
    win = handle.active_win()
    session = live.openTerminal(command)
    guard session != 0 else {
      XCTFail(
        "open_terminal returned 0 for \(command): the session or its placeholder failed",
        file: file, line: line)
      live.shutdown()
      return nil
    }
    container = PaneContainerView(
      handle: handle, metrics: live.metrics, tabpageHandle: handle.current_tabpage(),
      optionAsAlt: false)
    renderer = OffscreenRenderer(hosting: container, size: Self.windowSize)
  }

  var handle: WolfCoreHandle { live.handle }
  var frame: FfiTerminalFrame { handle.terminal_frame(session) }
  var cellWidth: CGFloat { live.metrics.cellWidth }
  var cellHeight: CGFloat { live.metrics.cellHeight }

  /// The terminal views the container hosts.
  var terminalViews: [TerminalPaneView] {
    container.subviews.compactMap { $0 as? TerminalPaneView }
  }

  /// The placeholder window's grid size, which `syncGridSizes` sets from the pane's content
  /// area, or `nil` while the window has no grid.
  var gridSize: (cols: UInt16, rows: UInt16)? {
    let grid = handle.grid_for_win(win)
    guard grid != 0 else { return nil }
    let frame = handle.read_frame_for_grid(grid)
    return (frame.cols(), frame.rows())
  }

  /// A description of where convergence stands, for a failure message.
  var state: String {
    let grid = gridSize.map { "\($0.cols)x\($0.rows)" } ?? "none"
    return "\(terminalViews.count) terminal view(s), placeholder grid \(grid), session frame"
      + " \(frame.cols())x\(frame.rows()) at version \(frame.version())"
  }

  /// Runs render wakes until the container shows a terminal view and has sized the session
  /// to the placeholder grid, which it has moved off the starting 80 x 24.
  func converge() -> Bool {
    live.waitFor(timeout: Self.timeout) {
      container.onRenderWake()
      guard !terminalViews.isEmpty, let grid = gridSize else { return false }
      let frame = self.frame
      return grid.cols == frame.cols() && grid.rows == frame.rows()
        && !(grid.cols == 80 && grid.rows == 24)
    }
  }

  /// Whether cell `col` of the session's row 0 reads `text`.
  func row0(cell col: Int, reads text: String) -> Bool {
    let cells = frame.row_cells(0)
    return cells.count > col && cells[col].text().toString() == text
  }

  /// One more wake, then a capture.
  func render() -> RenderedView {
    container.onRenderWake()
    return renderer.render()
  }

  /// Cell `(col, row)` in the terminal view's flipped coordinates.
  func cellRect(col: Int, row: Int) -> NSRect {
    NSRect(
      x: CGFloat(col) * cellWidth, y: CGFloat(row) * cellHeight, width: cellWidth,
      height: cellHeight)
  }

  /// The pixels a cell's interior covers: the cell inset by 1 point, so no pixel straddles
  /// the neighbouring cell.
  func interior(col: Int, row: Int) -> NSRect {
    cellRect(col: col, row: row).insetBy(dx: 1, dy: 1)
  }

  /// Sends a left press at cell `from`, motion to cell `to` and a release there, all in
  /// row 0, at the cells' centres in the pixel units the container sized the session in.
  func drag(row: Int, from: Int, to: Int) {
    let scale = Double(container.window?.backingScaleFactor ?? 1)
    let size = TerminalGeometry.size(
      cols: Int(frame.cols()), rows: Int(frame.rows()), cellWidth: Double(cellWidth),
      cellHeight: Double(cellHeight), scale: scale)
    func pixels(_ col: Int) -> (x: Double, y: Double) {
      TerminalGeometry.pointerPixels(
        x: (Double(col) + 0.5) * Double(cellWidth), y: (Double(row) + 0.5) * Double(cellHeight),
        originX: 0, originY: 0, cellWidth: Double(cellWidth), cellHeight: Double(cellHeight),
        size: size)
    }
    // terminal_mouse_kind PRESS 0, RELEASE 1, MOTION 2; mouse_button LEFT 0; no mods.
    let start = pixels(from)
    let end = pixels(to)
    handle.terminal_mouse(session, 0, 0, start.x, start.y, 0)
    handle.terminal_mouse(session, 2, 0, end.x, end.y, 0)
    handle.terminal_mouse(session, 1, 0, end.x, end.y, 0)
  }

  func shutdown() {
    live.shutdown()
  }
}

/// A packed `0x00RRGGBB` colour as an sRGB `NSColor`.
private func srgb(_ packed: UInt32) -> NSColor {
  NSColor(
    srgbRed: CGFloat((packed >> 16) & 0xFF) / 255, green: CGFloat((packed >> 8) & 0xFF) / 255,
    blue: CGFloat(packed & 0xFF) / 255, alpha: 1)
}

@MainActor
final class TerminalPaneRenderTests: XCTestCase {

  /// The colours the session prints: a crimson background, then bold blue text.
  private let crimson = srgb(0x00DC_143C)
  private let blue = srgb(0x0000_00FF)
  /// Four crimson-background spaces in cells 0 to 3, then bold blue `WWWW` in cells 4 to 7.
  private let colouredCommand =
    "printf '\\033[48;2;220;20;60m    \\033[0m\\033[1;38;2;0;0;255mWWWW\\033[0m'; exec sleep 600"

  /// The fraction of `rect`'s pixels within `tolerance` of `color`.
  private func fraction(
    of color: NSColor, in rect: NSRect, of view: NSView, capture: RenderedView,
    tolerance: CGFloat
  ) -> Double {
    let pixels = capture.colors(in: rect, of: view)
    guard !pixels.isEmpty else { return 0 }
    let near = pixels.filter { colorDistance($0, color) <= tolerance }.count
    return Double(near) / Double(pixels.count)
  }

  /// The number of `rect`'s pixels within `tolerance` of `color`.
  private func count(
    of color: NSColor, in rect: NSRect, of view: NSView, capture: RenderedView,
    tolerance: CGFloat
  ) -> Int {
    capture.colors(in: rect, of: view).filter { colorDistance($0, color) <= tolerance }.count
  }

  /// The session's default colours, failing the test when either is near `colors`.
  private func defaultColors(
    of fixture: TerminalPaneFixture, farFrom colors: [(String, NSColor)],
    file: StaticString = #filePath, line: UInt = #line
  ) -> (fg: NSColor, bg: NSColor) {
    let frame = fixture.frame
    let fg = srgb(frame.default_fg().rgb)
    let bg = srgb(frame.default_bg().rgb)
    for (name, color) in colors {
      XCTAssertGreaterThan(
        colorDistance(bg, color), 0.5,
        "premise: the default background \(bg) must be far from \(name), or a cell painted in"
          + " the default passes for one painted in \(name)", file: file, line: line)
    }
    return (fg, bg)
  }

  func testTerminalPaneDrawsItsSessionsColours() {
    guard let fixture = TerminalPaneFixture(command: colouredCommand) else { return }
    defer { fixture.shutdown() }
    XCTAssertTrue(
      fixture.converge(),
      "the container must host a terminal view and size the session: \(fixture.state)")
    guard
      fixture.live.waitFor(
        timeout: TerminalPaneFixture.timeout,
        { fixture.row0(cell: 4, reads: "W") })
    else {
      XCTFail("premise: the session never printed WWWW into cell 4 of row 0: \(fixture.state)")
      return
    }
    let defaults = defaultColors(of: fixture, farFrom: [("crimson", crimson), ("blue", blue)])
    XCTAssertGreaterThan(
      colorDistance(defaults.fg, blue), 0.5,
      "premise: the default foreground must be far from blue, or default-coloured glyphs pass")

    let capture = fixture.render()
    guard let view = fixture.terminalViews.first else {
      XCTFail("no TerminalPaneView after a render wake: syncTerminalViews created none")
      return
    }
    for col in 0..<4 {
      let crimsonShare = fraction(
        of: crimson, in: fixture.interior(col: col, row: 0), of: view, capture: capture,
        tolerance: 0.05)
      XCTAssertGreaterThanOrEqual(
        crimsonShare, 0.9,
        "cell \(col) of row 0 has a crimson background, so at least 90% of its interior must be"
          + " crimson, not \(crimsonShare): the default background means the cell's bg is not"
          + " painted")
    }
    var blueTotal = 0
    for col in 4..<8 {
      let blueInk = count(
        of: blue, in: fixture.cellRect(col: col, row: 0), of: view, capture: capture,
        tolerance: 0.4)
      XCTAssertGreaterThan(
        blueInk, 0,
        "cell \(col) of row 0 holds a bold blue W, so some of its pixels must be blue: none"
          + " means the glyph is drawn in another colour, the default fg, or not at all")
      blueTotal += blueInk
    }
    XCTAssertGreaterThanOrEqual(
      blueTotal, 8, "the four blue Ws must leave at least 8 blue pixels between them")
  }

  func testSessionIsSizedFromThePaneContentArea() {
    guard let fixture = TerminalPaneFixture(command: "exec sleep 600") else { return }
    defer { fixture.shutdown() }
    let converged = fixture.converge()
    XCTAssertTrue(
      converged,
      "the session's frame must take the size of the placeholder grid, which syncGridSizes"
        + " sizes from the same content area, and that size is not 80x24: \(fixture.state)."
        + " A frame stuck at 80x24 means resize_terminal is never sent; a grid of 80x24 means"
        + " the window size no longer gives another size, so pick another")
    guard converged, let grid = fixture.gridSize else { return }
    XCTAssertEqual(
      [fixture.frame.cols(), fixture.frame.rows()], [grid.cols, grid.rows],
      "the session must be as many cells as the pane's content area holds")
  }

  func testCursorCellPaintsInTheCursorColour() {
    // CSI 2 SP q makes the cursor a steady block, which fills the whole cell.
    guard let fixture = TerminalPaneFixture(command: "printf '\\033[2 qAB'; exec sleep 600")
    else { return }
    defer { fixture.shutdown() }
    XCTAssertTrue(
      fixture.converge(),
      "the container must host a terminal view and size the session: \(fixture.state)")
    guard
      fixture.live.waitFor(
        timeout: TerminalPaneFixture.timeout,
        {
          let frame = fixture.frame
          return frame.has_cursor() && frame.cursor_visible() && frame.cursor_col() == 2
            && frame.cursor_row() == 0 && frame.cursor_style() == 0
        })
    else {
      XCTFail(
        "premise: the cursor never stood at cell (2, 0) after `AB` as a block (style 0):"
          + " \(fixture.state)")
      return
    }
    let frame = fixture.frame
    let cursorColor = frame.cursor_color()
    let expected = srgb(cursorColor.set ? cursorColor.rgb : frame.default_fg().rgb)
    let defaults = defaultColors(of: fixture, farFrom: [("the cursor colour", expected)])

    let capture = fixture.render()
    guard let view = fixture.terminalViews.first else {
      XCTFail("no TerminalPaneView after a render wake: syncTerminalViews created none")
      return
    }
    let cell = fixture.cellRect(col: 2, row: 0)
    let centre = capture.color(at: NSPoint(x: cell.midX, y: cell.midY), in: view)
    XCTAssertLessThan(
      colorDistance(centre, expected), 0.1,
      "the centre of the block cursor's cell (2, 0) must be the cursor colour \(expected), not"
        + " \(centre): the default background means the cursor is not drawn")
    let next = fixture.cellRect(col: 3, row: 0)
    let past = capture.color(at: NSPoint(x: next.midX, y: next.midY), in: view)
    XCTAssertLessThan(
      colorDistance(past, defaults.bg), 0.1,
      "cell (3, 0), past the cursor, must be the default background, not \(past): the cursor"
        + " colour there means the cursor is drawn wider or in the wrong cell")
  }

  func testDragSelectionPaintsSwappedColours() {
    guard let fixture = TerminalPaneFixture(command: colouredCommand) else { return }
    defer { fixture.shutdown() }
    XCTAssertTrue(
      fixture.converge(),
      "the container must host a terminal view and size the session: \(fixture.state)")
    guard
      fixture.live.waitFor(
        timeout: TerminalPaneFixture.timeout,
        { fixture.row0(cell: 4, reads: "W") })
    else {
      XCTFail("premise: the session never printed its cells: \(fixture.state)")
      return
    }
    let defaults = defaultColors(of: fixture, farFrom: [])
    XCTAssertGreaterThan(
      colorDistance(defaults.fg, crimson), 0.3,
      "premise: the default foreground must be far from crimson, or a swapped cell passes for"
        + " an unselected one")

    fixture.drag(row: 0, from: 0, to: 3)
    guard
      fixture.live.waitFor(
        timeout: TerminalPaneFixture.timeout,
        {
          let frame = fixture.frame
          return frame.row_has_selection(0) && frame.row_selection_start(0) == 0
            && frame.row_selection_end(0) == 3
        })
    else {
      let frame = fixture.frame
      XCTFail(
        "premise: a drag from cell 0 to cell 3 must select columns 0 to 3 of row 0, but row 0"
          + " has selection \(frame.row_has_selection(0)) from \(frame.row_selection_start(0))"
          + " to \(frame.row_selection_end(0)): the pointer pixels do not match the cell size"
          + " the container sent")
      return
    }

    let capture = fixture.render()
    guard let view = fixture.terminalViews.first else {
      XCTFail("no TerminalPaneView after a render wake: syncTerminalViews created none")
      return
    }
    for col in 0..<4 {
      let swapped = fraction(
        of: defaults.fg, in: fixture.interior(col: col, row: 0), of: view, capture: capture,
        tolerance: 0.05)
      XCTAssertGreaterThanOrEqual(
        swapped, 0.9,
        "selected cell \(col) has a crimson background and the default foreground, so it must"
          + " draw swapped, in the default foreground \(defaults.fg), over at least 90% of its"
          + " interior, not \(swapped): crimson means the selection is not painted")
    }
  }

  func testAPaneWhoseShellExitedDropsItsTerminalView() {
    guard let fixture = TerminalPaneFixture(command: "sleep 3; exit 0") else { return }
    defer { fixture.shutdown() }
    let shown = fixture.live.waitFor(timeout: TerminalPaneFixture.timeout) {
      fixture.container.onRenderWake()
      return !fixture.terminalViews.isEmpty
    }
    XCTAssertTrue(
      shown, "a terminal view must exist while the shell runs: \(fixture.state)")
    guard shown else { return }

    let unbound = fixture.live.waitFor(timeout: TerminalPaneFixture.timeout) {
      fixture.handle.win_buffer(fixture.win).terminal() == 0
    }
    XCTAssertTrue(
      unbound,
      "premise: once the shell exits, its window must show no session: \(fixture.state)")
    fixture.container.onRenderWake()
    XCTAssertTrue(
      fixture.terminalViews.isEmpty,
      "a render wake after the shell exited must remove the terminal view, but"
        + " \(fixture.terminalViews.count) remain: a stale view keeps painting a dead session"
        + " over the empty buffer")
    XCTAssertEqual(
      fixture.container.paneCount, 1,
      "the pane itself stays, showing the empty buffer core gave the tabpage's last window")
  }
}
