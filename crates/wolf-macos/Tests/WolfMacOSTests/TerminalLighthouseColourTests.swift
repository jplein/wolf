// Paints a terminal pane under the vendored Lighthouse and checks its 16 ANSI colours and pixels.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A failure reading the pinned Lighthouse, naming the file and the missing piece.
private struct LighthouseError: LocalizedError {
  let message: String
  var errorDescription: String? { message }
}

/// One of Lighthouse's two variants, named as `vim.o.background` and its palette table are.
private enum LighthouseBackground: String {
  case dark
  case light
}

/// The expected colours of the pinned Lighthouse, read from its `colors/lighthouse.lua` as the
/// Rust oracle in `crates/wolf-ffi/tests/common/mod.rs` reads them: each palette is the
/// `local dark = {` or `local light = {` table of `key = "#rrggbb",` lines closed by a line
/// that is exactly `}`, and the ANSI order is the `local ansi = {` list of `[0] = p.<key>,`
/// then fifteen `p.<key>,` lines. Colours are packed `0x00RRGGBB`.
private struct LighthouseColours {
  let path: String
  let lines: [String]

  /// Reads `<tree>/lighthouse/colors/lighthouse.lua`, or the flat `<tree>/colors/…`.
  init(tree: String) throws {
    var isDirectory: ObjCBool = false
    let inner = (tree as NSString).appendingPathComponent("lighthouse")
    let root =
      FileManager.default.fileExists(atPath: inner, isDirectory: &isDirectory)
        && isDirectory.boolValue ? inner : tree
    path = (root as NSString).appendingPathComponent("colors/lighthouse.lua")
    do {
      lines = try String(contentsOfFile: path, encoding: .utf8).components(separatedBy: "\n")
    } catch {
      throw LighthouseError(
        message: "failed to read the pinned Lighthouse colorscheme at \(path): \(error)")
    }
  }

  /// `line` without trailing whitespace.
  private static func trimmedEnd(_ line: String) -> String {
    String(line.reversed().drop(while: { $0.isWhitespace }).reversed())
  }

  /// The lines strictly between the line `opener` and the next line that is exactly `}`.
  private func table(_ opener: String, _ what: String) throws -> [String] {
    guard let start = lines.firstIndex(where: { Self.trimmedEnd($0) == opener }) else {
      throw LighthouseError(message: "\(path) has no `\(opener)` line, so it has no \(what)")
    }
    var body: [String] = []
    for line in lines[(start + 1)...] {
      if Self.trimmedEnd(line) == "}" { break }
      body.append(line)
    }
    return body
  }

  /// The `background` palette's colour for `key`.
  func colour(_ background: LighthouseBackground, _ key: String) throws -> UInt32 {
    let what = "\(background.rawValue) palette"
    let prefix = "\(key) = \"#"
    for line in try table("local \(background.rawValue) = {", what) {
      let item = line.trimmingCharacters(in: .whitespaces)
      guard item.hasPrefix(prefix), item.hasSuffix("\",") else { continue }
      let hex = item.dropFirst(prefix.count).dropLast(2)
      guard let value = UInt32(hex, radix: 16) else {
        throw LighthouseError(message: "\(path)'s \(what) gives \(key) #\(hex), which is not hex")
      }
      return value
    }
    throw LighthouseError(message: "\(path)'s \(what) has no `\(key) = \"#rrggbb\",` line")
  }

  /// The palette keys of `g:terminal_color_0` to `_15`, in order.
  func ansiKeys() throws -> [String] {
    let opener = "local ansi = {"
    guard lines.contains(where: { Self.trimmedEnd($0) == opener }) else {
      throw LighthouseError(
        message: "\(path) has no `\(opener)` list, so this Lighthouse sets no g:terminal_color_N"
          + " and a terminal pane keeps libghostty-vt's built-in palette")
    }
    var keys: [String] = []
    for line in try table(opener, "ANSI list") {
      var item = line.trimmingCharacters(in: .whitespaces)
      if item.isEmpty || item.hasPrefix("--") { continue }
      if item.hasPrefix("[0] = ") { item = String(item.dropFirst("[0] = ".count)) }
      guard item.hasPrefix("p."), item.hasSuffix(",") else {
        throw LighthouseError(
          message: "\(path)'s ANSI list has the line \(line.debugDescription), not `p.<key>,`")
      }
      keys.append(String(item.dropFirst(2).dropLast()))
    }
    guard keys.count == 16 else {
      throw LighthouseError(
        message: "\(path)'s ANSI list names \(keys.count) colours, not 16: \(keys)")
    }
    return keys
  }

  /// The colours of `g:terminal_color_0` to `_15` for `background`.
  func ansi(_ background: LighthouseBackground) throws -> [UInt32] {
    try ansiKeys().map { try colour(background, $0) }
  }
}

/// A packed `0x00RRGGBB` colour as an sRGB `NSColor`.
private func srgbColour(_ packed: UInt32) -> NSColor {
  NSColor(
    srgbRed: CGFloat((packed >> 16) & 0xFF) / 255, green: CGFloat((packed >> 8) & 0xFF) / 255,
    blue: CGFloat(packed & 0xFF) / 255, alpha: 1)
}

/// `#rrggbb` for a colour the bridge says is set, else `unset`.
private func hex(_ colour: FfiColor) -> String {
  colour.set ? String(format: "#%06x", colour.rgb) : "unset"
}

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

  /// Copies `init.lua` and Lighthouse's colorscheme, by content, into a fresh directory as
  /// `init.lua` beside `lighthouse/colors/lighthouse.lua`, and returns the copy of `init.lua`.
  private func stage(initLua: String, colours: LighthouseColours) throws -> String {
    let base = ProcessInfo.processInfo.environment["TEST_TMPDIR"] ?? NSTemporaryDirectory()
    let root = (base as NSString).appendingPathComponent("lighthouse-\(UUID().uuidString)")
    let colors = (root as NSString).appendingPathComponent("lighthouse/colors")
    try FileManager.default.createDirectory(atPath: colors, withIntermediateDirectories: true)
    let staged = (root as NSString).appendingPathComponent("init.lua")
    try Data(contentsOf: URL(fileURLWithPath: initLua)).write(to: URL(fileURLWithPath: staged))
    try Data(contentsOf: URL(fileURLWithPath: colours.path)).write(
      to: URL(fileURLWithPath: (colors as NSString).appendingPathComponent("lighthouse.lua")))
    return staged
  }

  /// The pinned Lighthouse and a converged pane running the colour script over it; `nil`, with
  /// the test failed, when any step does not happen.
  private func start() throws -> (TerminalPaneFixture, LighthouseColours)? {
    let key = "WOLF_TEST_LIGHTHOUSE_DIR"
    guard let tree = LiveCore.runfile(key) else {
      let raw = ProcessInfo.processInfo.environment[key] ?? "unset"
      XCTFail(
        "\(key) (\(raw)) resolves to no staged tree: the WolfMacOSTests target must stage"
          + " //bazel/lighthouse:lighthouse_tree")
      return nil
    }
    let colours = try LighthouseColours(tree: tree)
    guard let initLua = LiveCore.stagedInitLua() else { return nil }
    let staged = try stage(initLua: initLua, colours: colours)
    guard let fixture = TerminalPaneFixture(command: Self.colourScript, initLua: staged) else {
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
      let mean = pixels.isEmpty ? "none" : Self.describe(Self.mean(pixels))
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

  /// The mean of `pixels` in sRGB.
  private static func mean(_ pixels: [NSColor]) -> NSColor {
    var red: CGFloat = 0
    var green: CGFloat = 0
    var blue: CGFloat = 0
    for pixel in pixels {
      let c = pixel.usingColorSpace(.sRGB) ?? pixel
      red += c.redComponent
      green += c.greenComponent
      blue += c.blueComponent
    }
    let n = CGFloat(pixels.count)
    return NSColor(srgbRed: red / n, green: green / n, blue: blue / n, alpha: 1)
  }

  /// `colour` as `#rrggbb` in sRGB.
  private static func describe(_ colour: NSColor) -> String {
    guard let c = colour.usingColorSpace(.sRGB) else { return "\(colour)" }
    func byte(_ v: CGFloat) -> Int { Int((v * 255).rounded()) }
    return String(
      format: "#%02x%02x%02x", byte(c.redComponent), byte(c.greenComponent), byte(c.blueComponent))
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
