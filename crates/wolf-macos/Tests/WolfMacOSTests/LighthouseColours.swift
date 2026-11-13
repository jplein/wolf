// Reads the expected colours of the vendored Lighthouse and stages it beside a copy of init.lua.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A failure reading the pinned Lighthouse, naming the file and the missing piece.
struct LighthouseError: LocalizedError {
  let message: String
  var errorDescription: String? { message }
}

/// One of Lighthouse's two variants, named as `vim.o.background` and its palette table are.
enum LighthouseBackground: String {
  case dark
  case light
}

/// The expected colours of the pinned Lighthouse, read from its `colors/lighthouse.lua` as the
/// Rust oracle in `crates/wolf-ffi/tests/common/mod.rs` reads them: each palette is the
/// `local dark = {` or `local light = {` table of `key = "#rrggbb",` lines closed by a line
/// that is exactly `}`, and the ANSI order is the `local ansi = {` list of `[0] = p.<key>,`
/// then fifteen `p.<key>,` lines. Colours are packed `0x00RRGGBB`.
struct LighthouseColours {
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

  /// The lines, trimmed, that make `Normal` the palette's `foreground` on its `background`.
  private static let normalLines = [
    "local foreground = p.foreground",
    "local background = p.background",
    "vim.api.nvim_set_hl(0, \"Normal\", { fg = foreground, bg = background })",
  ]

  /// `Normal`'s colours for `background`: the palette's `foreground` and `background`.
  ///
  /// Throws, naming the line, when the file lacks one of `normalLines`, so a pin that rewires
  /// `Normal` fails as a broken premise rather than as a wrong colour.
  func normal(_ background: LighthouseBackground) throws -> (fg: UInt32, bg: UInt32) {
    let trimmed = Set(lines.map { $0.trimmingCharacters(in: .whitespaces) })
    for line in Self.normalLines where !trimmed.contains(line) {
      throw LighthouseError(
        message: "\(path) has no line `\(line)`, so its Normal is no longer the palette's"
          + " foreground on its background and the expected colours below are not Normal's")
    }
    return (try colour(background, "foreground"), try colour(background, "background"))
  }
}

/// The staged copy of `init.lua` that activates the vendored Lighthouse, and its colours;
/// `nil`, with the test failed, when the tree or `init.lua` does not resolve.
///
/// `init.lua` self-locates the colorscheme beside itself, so both are copied by content into a
/// fresh directory, as `init.lua` beside `lighthouse/colors/lighthouse.lua`.
@MainActor
func stageLighthouse(
  file: StaticString = #filePath, line: UInt = #line
) throws -> (initLua: String, colours: LighthouseColours)? {
  let key = "WOLF_TEST_LIGHTHOUSE_DIR"
  guard let tree = LiveCore.runfile(key) else {
    let raw = ProcessInfo.processInfo.environment[key] ?? "unset"
    XCTFail(
      "\(key) (\(raw)) resolves to no staged tree: the WolfMacOSTests target must stage"
        + " //bazel/lighthouse:lighthouse_tree", file: file, line: line)
    return nil
  }
  let colours = try LighthouseColours(tree: tree)
  guard let initLua = LiveCore.stagedInitLua(file: file, line: line) else { return nil }
  let base = ProcessInfo.processInfo.environment["TEST_TMPDIR"] ?? NSTemporaryDirectory()
  let root = (base as NSString).appendingPathComponent("lighthouse-\(UUID().uuidString)")
  let colors = (root as NSString).appendingPathComponent("lighthouse/colors")
  try FileManager.default.createDirectory(atPath: colors, withIntermediateDirectories: true)
  let staged = (root as NSString).appendingPathComponent("init.lua")
  try Data(contentsOf: URL(fileURLWithPath: initLua)).write(to: URL(fileURLWithPath: staged))
  try Data(contentsOf: URL(fileURLWithPath: colours.path)).write(
    to: URL(fileURLWithPath: (colors as NSString).appendingPathComponent("lighthouse.lua")))
  return (staged, colours)
}

/// A packed `0x00RRGGBB` colour as an sRGB `NSColor`.
func srgbColour(_ packed: UInt32) -> NSColor {
  NSColor(
    srgbRed: CGFloat((packed >> 16) & 0xFF) / 255, green: CGFloat((packed >> 8) & 0xFF) / 255,
    blue: CGFloat(packed & 0xFF) / 255, alpha: 1)
}

/// `#rrggbb` for a colour the bridge says is set, else `unset`.
func hex(_ colour: FfiColor) -> String {
  colour.set ? String(format: "#%06x", colour.rgb) : "unset"
}

/// The mean of `pixels` in sRGB.
func meanColour(_ pixels: [NSColor]) -> NSColor {
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
func describeColour(_ colour: NSColor) -> String {
  guard let c = colour.usingColorSpace(.sRGB) else { return "\(colour)" }
  func byte(_ v: CGFloat) -> Int { Int((v * 255).rounded()) }
  return String(
    format: "#%02x%02x%02x", byte(c.redComponent), byte(c.greenComponent), byte(c.blueComponent))
}
