// Paints a terminal pane under the vendored Lighthouse and checks its 16 ANSI colours and pixels.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// The 16 ANSI colours of a terminal pane over the captive `init.lua` beside a copy of the
/// vendored Lighthouse, so `init.lua` activates the real colorscheme: the session's cells, and
/// the pixels the pane paints for them through `OffscreenRenderer`, in sRGB.
///
/// The colour script prints `X` in SGR 30–37 and 90–97 on row 0 and a space in SGR 40–47 and
/// 100–107 on row 1, each followed by `ESC[0m`, so ANSI colour `i` is column `i`: row 0's
/// foreground and row 1's background. No cell is bold, so no brightening applies. Every run
/// prints a table of expected, cell and pixel colours, for the report.
///
/// Premise, measured here: Neovim reloads the active colorscheme when `background` changes,
/// which sets Lighthouse's other `g:terminal_color_N` and fires `ColorScheme`.
@MainActor
final class TerminalLighthouseColourTests: XCTestCase {
  /// The row of foreground cells and the row of background cells.
  private static let fgRow: UInt16 = 0
  private static let bgRow: UInt16 = 1
  /// The share of a cell's interior that must be the expected colour.
  private static let minShare = 0.9
  /// The largest sRGB distance a pixel may be from the expected colour, as
  /// `OffscreenRendererTests` pins the sampler to.
  private static let tolerance: CGFloat = 0.01

  /// The SGR foreground code of ANSI colour `i`; the background code is ten more.
  private static func sgr(_ i: Int) -> Int { i < 8 ? 30 + i : 90 + i - 8 }

  /// The two colour rows, then a sleep that keeps the session open.
  private static var colourScript: String {
    let fg = (0..<16).map { "\\033[\(sgr($0))mX\\033[0m" }.joined()
    let bg = (0..<16).map { "\\033[\(sgr($0) + 10)m \\033[0m" }.joined()
    return "printf '\(fg)\\r\\n\(bg)\\r\\n'; exec sleep 600"
  }

  /// The pinned Lighthouse and a converged pane running the colour script over it; `nil`, with
  /// the test failed, when any step does not happen.
  private func start() throws -> (TerminalPaneFixture, LighthouseColours)? {
    guard let lighthouse = try stageLighthouse() else { return nil }
    let colours = lighthouse.colours
    guard
      let fixture = TerminalPaneFixture(command: Self.colourScript, initLua: lighthouse.initLua)
    else {
      return nil
    }
    guard fixture.converge() else {
      XCTFail("the container must host a terminal view and size the session: \(fixture.state)")
      fixture.shutdown()
      return nil
    }
    return (fixture, colours)
  }

  /// The indices whose row-0 foreground or row-1 background in the session's frame is not
  /// `expected`; all 16 while the rows are not printed.
  private func cellMismatches(_ fixture: TerminalPaneFixture, _ expected: [UInt32]) -> [Int] {
    let frame = fixture.frame
    let fg = frame.row_cells(Self.fgRow)
    let bg = frame.row_cells(Self.bgRow)
    guard fg.count >= 16, bg.count >= 16 else { return Array(0..<16) }
    return (0..<16).filter { i in
      let f = fg[i].fg()
      let b = bg[i].bg()
      return !(f.set && f.rgb == expected[i] && b.set && b.rgb == expected[i])
    }
  }

  /// Wakes the container until the session's cells show `background`'s palette, renders, and
  /// asserts the 32 cells and the 16 row-1 cells' pixels, after printing the table.
  private func assertPalette(
    _ fixture: TerminalPaneFixture, _ colours: LighthouseColours,
    _ background: LighthouseBackground, _ context: String
  ) throws {
    let expected = try colours.ansi(background)
    let keys = try colours.ansiKeys()
    fixture.live.waitFor(timeout: TerminalPaneFixture.timeout) {
      fixture.container.onRenderWake()
      return cellMismatches(fixture, expected).isEmpty
    }
    let capture = fixture.render()
    guard let view = fixture.terminalViews.first else {
      XCTFail("no TerminalPaneView after a render wake: syncTerminalViews created none")
      return
    }
    let frame = fixture.frame
    let fgCells = frame.row_cells(Self.fgRow)
    let bgCells = frame.row_cells(Self.bgRow)
    let defaultBg = frame.default_bg()
    var table =
      "\(background.rawValue) palette, \(context), against Lighthouse's \(background.rawValue)"
      + " values (default bg \(hex(defaultBg))):\n"
      + [
        Self.pad("index", 5), Self.pad("sgr", 7), Self.pad("key", 17), Self.pad("expected", 8),
        Self.pad("fg (row 0)", 10), Self.pad("bg (row 1)", 10), Self.pad("pixel share", 11),
        "mean pixel",
      ].joined(separator: "  ") + "\n"
    var pixelFailures: [String] = []
    for i in 0..<16 {
      let want = srgbColour(expected[i])
      let pixels = capture.colors(in: fixture.interior(col: i, row: Int(Self.bgRow)), of: view)
      let near = pixels.filter { colorDistance($0, want) <= Self.tolerance }.count
      let share = pixels.isEmpty ? 0 : Double(near) / Double(pixels.count)
      let mean = pixels.isEmpty ? "none" : describeColour(meanColour(pixels))
      let fg = i < fgCells.count ? hex(fgCells[i].fg()) : "absent"
      let bg = i < bgCells.count ? hex(bgCells[i].bg()) : "absent"
      table +=
        [
          Self.pad(String(i), 5, right: true),
          Self.pad("\(Self.sgr(i))/\(Self.sgr(i) + 10)", 7), Self.pad(keys[i], 17),
          Self.pad(String(format: "#%06x", expected[i]), 8), Self.pad(fg, 10), Self.pad(bg, 10),
          Self.pad(String(format: "%.3f", share), 11, right: true), mean,
        ].joined(separator: "  ") + "\n"
      if share < Self.minShare {
        pixelFailures.append("\(i) (\(keys[i]): share \(String(format: "%.3f", share)))")
      }
    }
    print(table)

    if defaultBg.set {
      XCTAssertFalse(
        expected.contains(defaultBg.rgb),
        "premise: the pane's default background \(hex(defaultBg)) must be none of the expected"
          + " colours, or an unpainted cell passes for a painted one")
    }
    let wrongCells = cellMismatches(fixture, expected)
    XCTAssertTrue(
      wrongCells.isEmpty,
      "\(context): the cells of ANSI colours \(wrongCells) must hold Lighthouse's"
        + " \(background.rawValue) values. A value from no Lighthouse palette means the session"
        + " kept libghostty-vt's built-in palette, so g:terminal_color_N was not sent or not laid"
        + " over it; the other background's values mean the change did not reach the open"
        + " session.\n\(table)")
    XCTAssertTrue(
      pixelFailures.isEmpty,
      "\(context): at least \(Self.minShare) of each row-1 cell's interior must be within"
        + " \(Self.tolerance) sRGB of its expected colour, but \(pixelFailures) are not. With the"
        + " cells right, a mean pixel off by a small, even shift means the pane paints through a"
        + " colour space other than sRGB, and the default background means the cell is not"
        + " painted.\n\(table)")
  }

  /// `text` padded with spaces to `width`, on the left when `right`.
  private static func pad(_ text: String, _ width: Int, right: Bool = false) -> String {
    let fill = String(repeating: " ", count: max(0, width - text.count))
    return right ? fill + text : text + fill
  }

  /// With the vendored Lighthouse active on the dark background, the pane's 16 ANSI colours
  /// are Lighthouse's dark ones, in its cells and in the pixels painted for them.
  func testTerminalPanePaintsLighthousesDarkAnsiColours() throws {
    guard let started = try start() else { return }
    let (fixture, colours) = started
    defer { fixture.shutdown() }
    try assertPalette(fixture, colours, .dark, "at startup")
  }

  /// After `set_background(false)`, the open pane repaints in Lighthouse's light ANSI colours.
  func testABackgroundSwitchRepaintsThePaneInLighthousesLightAnsiColours() throws {
    guard let started = try start() else { return }
    let (fixture, colours) = started
    defer { fixture.shutdown() }
    try assertPalette(fixture, colours, .dark, "precondition, before the switch")
    fixture.handle.set_background(false)
    try assertPalette(fixture, colours, .light, "after set_background(false)")
  }
}
