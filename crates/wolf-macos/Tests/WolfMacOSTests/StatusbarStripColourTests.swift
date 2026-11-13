// Paints panes under the vendored Lighthouse and checks each statusbar strip against its Normal.

import AppKit
import WebKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// Each pane's statusbar strip over the captive `init.lua` beside a copy of the vendored
/// Lighthouse, sampled through `OffscreenRenderer` in dark and then in light: the strip is
/// `Normal`'s background, the colour the grid below it paints, for an editor pane, a terminal
/// pane and a preview whose grid is occluded.
///
/// The strip band is the top `bandHeight` points of a pane, inset `bandInset` from its sides.
/// The strip starts at the pane's top and its text at the strip's 3-point vertical padding, so
/// the band holds no glyph. Every stage prints one table row, for the report.
///
/// Premise, measured by each stage's wake: core's frame defaults are `Normal`'s, so
/// `read_frame().default_bg()` is the oracle's `Normal` background once Lighthouse is active.
/// The window is no `MainWindow` and Lighthouse leaves `WolfTitlebar` unset, so a strip that
/// follows the titlebar paints `WolfColor.fallbackBg`, black, which is neither `Normal`.
@MainActor
final class StatusbarStripColourTests: XCTestCase {
  /// The share of a region that must be the expected colour.
  private static let minShare = 0.9
  /// The largest sRGB distance a pixel may be from the expected colour, as
  /// `OffscreenRendererTests` pins the sampler to.
  private static let tolerance: CGFloat = 0.01
  /// The strip band's height from the pane's top, and its inset from the pane's sides.
  private static let bandHeight: CGFloat = 2
  private static let bandInset: CGFloat = 2
  /// The side of a sampled block of blank grid cells.
  private static let blockSide: CGFloat = 80
  /// The width of each column of the printed table.
  private static let columnWidth = 13

  /// The pixels of one sampled region, against one expected colour.
  private struct Region {
    /// The share of the region within `tolerance` of the expected colour.
    let share: Double
    /// The region's mean colour, or `nil` when it covers no pixel.
    let mean: NSColor?

    var meanText: String { mean.map(describeColour) ?? "none" }
    var shareText: String { String(format: "%.3f", share) }
  }

  /// Samples `rect`, in `view`'s coordinates, against `expected`.
  private static func region(
    _ rect: NSRect, of view: NSView, _ capture: RenderedView, expected: NSColor
  ) -> Region {
    let pixels = capture.colors(in: rect, of: view)
    guard !pixels.isEmpty else { return Region(share: 0, mean: nil) }
    let near = pixels.filter { colorDistance($0, expected) <= tolerance }.count
    return Region(share: Double(near) / Double(pixels.count), mean: meanColour(pixels))
  }

  /// The strip band over the x span `minX` to `maxX` of a pane at the container's top.
  private static func band(minX: CGFloat, maxX: CGFloat) -> NSRect {
    NSRect(
      x: minX + bandInset, y: 0, width: maxX - minX - 2 * bandInset, height: bandHeight)
  }

  /// Whether the bridge's `colour` is set and is `rgb`.
  private static func matches(_ colour: FfiColor, _ rgb: UInt32) -> Bool {
    colour.set && colour.rgb == rgb
  }

  /// One row of the printed table, each cell padded to `columnWidth`.
  private static func row(_ cells: [String]) -> String {
    cells.map { $0.padding(toLength: max($0.count, columnWidth), withPad: " ", startingAt: 0) }
      .joined(separator: "  ")
  }

  /// The printed table's header.
  private static let header = row([
    "kind", "background", "Normal bg", "read_frame bg", "strip mean", "strip share", "grid mean",
    "grid share", "text pixel",
  ])

  /// The premise failure for a stage whose wake never saw `Normal`'s background.
  private static func unreached(
    _ kind: String, _ background: LighthouseBackground, _ normalBg: UInt32, _ seen: String
  ) -> String {
    "premise, \(kind), \(background.rawValue): the frame's default background never became"
      + " Lighthouse's \(background.rawValue) Normal background"
      + " \(String(format: "#%06x", normalBg)), it is \(seen). At the dark stage, unset or another"
      + " colour means init.lua did not activate the staged Lighthouse; at the light stage, the"
      + " dark value means set_background(false) did not reload the colorscheme"
  }

  /// Samples `strip` and `grid`, in `container`'s coordinates, prints the stage's table row,
  /// and asserts that both are `Normal`'s background and, when `sameMeans`, that their means
  /// agree.
  private func assertStage(
    _ kind: String, _ background: LighthouseBackground, normal: (fg: UInt32, bg: UInt32),
    strip: NSRect, grid: NSRect, sameMeans: Bool, in container: NSView, _ capture: RenderedView,
    handle: WolfCoreHandle, textPixel: String = "-"
  ) {
    let expected = srgbColour(normal.bg)
    let stripRegion = Self.region(strip, of: container, capture, expected: expected)
    let gridRegion = Self.region(grid, of: container, capture, expected: expected)
    let normalBg = String(format: "#%06x", normal.bg)
    let table =
      Self.header + "\n"
      + Self.row([
        kind, background.rawValue, normalBg, hex(handle.read_frame().default_bg()),
        stripRegion.meanText, stripRegion.shareText, gridRegion.meanText, gridRegion.shareText,
        textPixel,
      ])
    print(table)

    XCTAssertGreaterThanOrEqual(
      gridRegion.share, Self.minShare,
      "premise, \(kind), \(background.rawValue): at least \(Self.minShare) of the grid block"
        + " \(grid) must be within \(Self.tolerance) sRGB of Normal's background \(normalBg),"
        + " as the grid paints its frame's default background, but its share is"
        + " \(gridRegion.shareText) with mean \(gridRegion.meanText). The other background's"
        + " Normal means the container did not repaint after the switch; anything else means"
        + " the block is not blank cells. Either way the strip is not what failed.\n\(table)")
    XCTAssertGreaterThanOrEqual(
      stripRegion.share, Self.minShare,
      "\(kind), \(background.rawValue): at least \(Self.minShare) of the strip band \(strip)"
        + " must be within \(Self.tolerance) sRGB of Normal's background \(normalBg), but its"
        + " share is \(stripRegion.shareText) with mean \(stripRegion.meanText). Black means"
        + " the strip follows the titlebar, through theme_titlebar_bg()'s fallback; the window's"
        + " background colour means it follows the titlebar through the window; the other"
        + " background's Normal means it kept the colours of an earlier draw.\n\(table)")
    guard sameMeans, let stripMean = stripRegion.mean, let gridMean = gridRegion.mean else {
      return
    }
    XCTAssertLessThanOrEqual(
      colorDistance(stripMean, gridMean), Self.tolerance,
      "\(kind), \(background.rawValue): the strip's mean \(stripRegion.meanText) and the grid's"
        + " mean \(gridRegion.meanText) must be within \(Self.tolerance) sRGB: the strip must"
        + " match the grid it sits on, and a gap means the two take different colours.\n\(table)"
    )
  }

  /// With Lighthouse active, an editor pane's strip is its grid's colour and `Normal`'s
  /// background, in dark and, after `set_background(false)`, in light.
  func testAnEditorPaneStripMatchesItsGridAndLighthousesNormalInDarkAndLight() throws {
    guard let lighthouse = try stageLighthouse() else { return }
    guard let fixture = TerminalViewFixture(initLua: lighthouse.initLua) else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    for background in [LighthouseBackground.dark, .light] {
      let normal = try lighthouse.colours.normal(background)
      if background == .light { handle.set_background(false) }
      guard fixture.wake(until: { Self.matches(handle.read_frame().default_bg(), normal.bg) })
      else {
        XCTFail(
          Self.unreached("editor", background, normal.bg, hex(handle.read_frame().default_bg())))
        return
      }
      let capture = fixture.render()
      let bounds = container.bounds
      assertStage(
        "editor", background, normal: normal,
        strip: Self.band(minX: bounds.minX, maxX: bounds.maxX),
        grid: NSRect(x: bounds.midX, y: bounds.midY, width: Self.blockSide, height: Self.blockSide),
        sameMeans: true, in: container, capture, handle: handle)
    }
  }

  /// With Lighthouse active, a terminal pane's strip is its session's background and
  /// `Normal`'s, and its filename is drawn in `Normal`'s foreground, in dark and in light.
  ///
  /// Premise, checked below: a terminal pane's status has no path line, so the filename is the
  /// only text in the strip.
  func testATerminalPaneStripMatchesItsGridAndLighthousesNormalInDarkAndLight() throws {
    guard let lighthouse = try stageLighthouse() else { return }
    guard
      let fixture = TerminalPaneFixture(command: "exec sleep 600", initLua: lighthouse.initLua)
    else { return }
    defer { fixture.shutdown() }
    guard fixture.converge() else {
      XCTFail("the container must host a terminal view and size the session: \(fixture.state)")
      return
    }
    let handle = fixture.handle
    let container = fixture.container
    let status = handle.pane_status_for_grid(handle.grid_for_win(fixture.win))
    let filename = status.filename().toString()
    let path = status.display_path().toString()
    guard !filename.isEmpty, path.isEmpty else {
      XCTFail(
        "premise: a terminal pane's status must have a filename and no path line, so the"
          + " filename is the strip's only text, but it has filename \(filename.debugDescription)"
          + " and path \(path.debugDescription)")
      return
    }

    for background in [LighthouseBackground.dark, .light] {
      let normal = try lighthouse.colours.normal(background)
      if background == .light { handle.set_background(false) }
      let reached = fixture.live.waitFor(timeout: TerminalPaneFixture.timeout) {
        container.onRenderWake()
        return Self.matches(fixture.frame.default_bg(), normal.bg)
          && Self.matches(handle.read_frame().default_bg(), normal.bg)
      }
      guard reached else {
        XCTFail(
          Self.unreached(
            "terminal", background, normal.bg,
            "\(hex(fixture.frame.default_bg())) in the session and"
              + " \(hex(handle.read_frame().default_bg())) in read_frame()"))
        return
      }
      let capture = fixture.render()
      guard let view = fixture.terminalViews.first else {
        XCTFail("no TerminalPaneView after a render wake: syncTerminalViews created none")
        return
      }
      let pane = view.frame
      let bounds = container.bounds

      // The strip above the terminal view, less the padding between them, over the text's span.
      let text = NSRect(
        x: pane.minX, y: 0, width: pane.width,
        height: pane.minY - CGFloat(EditorInset.padding) - 1)
      let bg = srgbColour(normal.bg)
      let fg = srgbColour(normal.fg)
      let farthest = capture.colors(in: text, of: container).max {
        colorDistance($0, bg) < colorDistance($1, bg)
      }
      let textPixel = farthest.map(describeColour) ?? "none"

      assertStage(
        "terminal", background, normal: normal,
        strip: Self.band(minX: bounds.minX, maxX: bounds.maxX),
        grid: NSRect(
          x: pane.midX - Self.blockSide / 2, y: pane.midY + Self.blockSide / 2,
          width: Self.blockSide, height: Self.blockSide),
        sameMeans: true, in: container, capture, handle: handle, textPixel: textPixel)

      guard let farthest else {
        XCTFail("terminal, \(background.rawValue): the strip's text region \(text) is empty")
        return
      }
      let toFg = colorDistance(farthest, fg)
      let toBg = colorDistance(farthest, bg)
      XCTAssertLessThan(
        toFg, toBg,
        "terminal, \(background.rawValue): the active pane's filename \(filename.debugDescription)"
          + " is drawn at full strength in Normal's foreground \(String(format: "#%06x", normal.fg))"
          + " over its background \(String(format: "#%06x", normal.bg)), so the strip's pixel"
          + " farthest from the background must be nearer the foreground, but \(textPixel) is"
          + " \(toFg) from it and \(toBg) from the background. The background itself means the"
          + " text is drawn in the background colour or not drawn; near black in dark means the"
          + " filename takes AppKit's label colour, not Normal's foreground")
    }
  }

  /// With Lighthouse active, a preview's strip is `Normal`'s background while its web view
  /// occludes its grid, so the grid's frame is never read, in dark and in light.
  ///
  /// `previewDidFinishLoad(win:)` stands in for WebKit's `didFinish`, which never comes where
  /// the gate runs (see `PreviewViewFixture`), and marks the preview loaded, so `PaneOcclusion`
  /// skips its grid. The grid block sampled is the source pane's, in the other half.
  func testAnOccludedPreviewStripMatchesLighthousesNormalInDarkAndLight() throws {
    guard let lighthouse = try stageLighthouse() else { return }
    guard let fixture = PreviewViewFixture(initLua: lighthouse.initLua) else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    for background in [LighthouseBackground.dark, .light] {
      let normal = try lighthouse.colours.normal(background)
      if background == .light { handle.set_background(false) }
      guard fixture.base.wake(until: { Self.matches(handle.read_frame().default_bg(), normal.bg) })
      else {
        XCTFail(
          Self.unreached("preview", background, normal.bg, hex(handle.read_frame().default_bg())))
        return
      }
      container.previewDidFinishLoad(win: fixture.previewWin)
      let capture = fixture.base.render()
      let webViews = container.subviews.compactMap { $0 as? WKWebView }
      guard webViews.count == 1, let web = webViews.first else {
        XCTFail("premise: the container must host one WKWebView, not \(webViews.count)")
        return
      }
      let occludes =
        web.superview === container && web.window != nil && !web.isHiddenOrHasHiddenAncestor
        && web.alphaValue == 1
      guard occludes else {
        XCTFail(
          "premise, \(background.rawValue): the preview's web view must be attached, visible and"
            + " opaque for PaneOcclusion to skip its grid, but it is attached"
            + " \(web.superview === container), in a window \(web.window != nil), hidden"
            + " \(web.isHiddenOrHasHiddenAncestor), at alpha \(web.alphaValue)")
        return
      }
      let w = web.frame
      let bounds = container.bounds
      let sourceMidX =
        w.midX < bounds.midX ? bounds.minX + bounds.width * 3 / 4 : bounds.minX + bounds.width / 4
      assertStage(
        "preview", background, normal: normal,
        strip: Self.band(minX: w.minX, maxX: w.maxX),
        grid: NSRect(
          x: sourceMidX - Self.blockSide / 2, y: bounds.midY, width: Self.blockSide,
          height: Self.blockSide),
        sameMeans: false, in: container, capture, handle: handle)
    }
  }
}
