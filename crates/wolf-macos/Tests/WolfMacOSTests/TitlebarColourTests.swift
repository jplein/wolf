// Hosts panes in a real MainWindow under Lighthouse and checks the titlebar colour and appearance.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A `PaneContainerView` over a `LiveCore` on the staged Lighthouse `init.lua`, hosted in a
/// production `MainWindow` that is never shown, so `applyTitlebarTheme` reaches the window.
///
/// The window is not pinned to Aqua: the container paints explicit sRGB, and a window that
/// set its own `appearance` would hide whether it inherits the one `AppDelegate` forces.
@MainActor
private final class TitlebarFixture {
  /// How long a wake, a theme change or an appearance change gets to take effect.
  static let timeout: TimeInterval = 15
  /// The size `MainWindow` gives the editor pane, as `WolfWindowController` builds it.
  private static let cols = 80
  private static let rows = 24

  /// `FfiKeyEvent.kind` of a printable character.
  private static let kindChar: UInt8 = 0
  /// `FfiKeyEvent.kind` of the Return key.
  private static let kindEnter: UInt8 = 1

  let live: LiveCore
  let container: PaneContainerView
  let window: MainWindow

  /// Starts a core over `initLua`, hosts a container in a `MainWindow` and wakes it until its
  /// first pane is bound; `nil`, with the test failed, when any step does not happen.
  init?(initLua: String, file: StaticString = #filePath, line: UInt = #line) {
    guard let live = LiveCore.start(initLua: initLua, file: file, line: line) else { return nil }
    self.live = live
    // AppKit's drawing machinery is set up by the shared application.
    _ = NSApplication.shared
    let handle = live.handle
    let container = PaneContainerView(
      handle: handle, metrics: live.metrics, optionAsAlt: false)
    self.container = container
    let sidebar = SidebarContainerView(
      tree: FileTreeView(handle: handle, paneContainer: container),
      search: SearchView(handle: handle, paneContainer: container))
    window = MainWindow(
      sidebar: sidebar, paneContainer: container, metrics: live.metrics, cols: Self.cols,
      rows: Self.rows)
    window.layoutIfNeeded()
    guard
      live.waitFor(
        timeout: Self.timeout,
        {
          container.onRenderWake()
          return container.paneCount == 1 && handle.active_win() != 0
        })
    else {
      XCTFail("premise: the container never bound its first pane", file: file, line: line)
      live.shutdown()
      return nil
    }
    guard container.window === window else {
      XCTFail(
        "premise: the container must be hosted in the MainWindow, or applyTitlebarTheme skips"
          + " everything, but its window is \(String(describing: container.window))",
        file: file, line: line)
      live.shutdown()
      return nil
    }
  }

  var handle: WolfCoreHandle { live.handle }

  /// Runs render wakes until `condition` holds or `timeout` passes, and returns whether it held.
  @discardableResult
  func wake(until condition: () -> Bool) -> Bool {
    live.waitFor(timeout: Self.timeout) {
      container.onRenderWake()
      return condition()
    }
  }

  /// Types `:command` and Return into Neovim.
  func typeEx(_ command: String) {
    for char in ":" + command {
      handle.send_key(
        FfiKeyEvent(
          kind: Self.kindChar, ch: char.unicodeScalars.first?.value ?? 0, fnum: 0, ctrl: false,
          shift: false, alt: false, meta: false))
    }
    handle.send_key(
      FfiKeyEvent(
        kind: Self.kindEnter, ch: 0, fnum: 0, ctrl: false, shift: false, alt: false, meta: false))
  }

  func shutdown() {
    live.shutdown()
  }
}

/// The titlebar of a `MainWindow` over the vendored Lighthouse: its background is `Normal`'s,
/// the grid's, in dark and in light; a defined `WolfTitlebar` overrides it and clearing it
/// restores `Normal`; an unchanged theme writes nothing; and the window draws in the
/// appearance Wolf forces on the app, whatever the host's.
///
/// Premise, measured by each stage's wake: core's frame defaults are `Normal`'s, so
/// `read_frame().default_bg()` is the oracle's `Normal` background once Lighthouse is active.
/// The window's `backgroundColor` is what shows through the transparent titlebar; it is
/// compared in sRGB, the grid's colour space.
@MainActor
final class TitlebarColourTests: XCTestCase {
  /// The largest sRGB distance the window's colour may be from the expected one. A packed
  /// colour built in sRGB is exact; one built in another colour space is several times this.
  private static let windowTolerance: CGFloat = 0.002
  /// The largest sRGB distance a pixel may be from the expected colour, as
  /// `OffscreenRendererTests` pins the sampler to.
  private static let tolerance: CGFloat = 0.01
  /// The share of the grid block that must be `Normal`'s background.
  private static let minShare = 0.9
  /// The side of the sampled block of blank grid cells.
  private static let blockSide: CGFloat = 80
  /// The `WolfTitlebar` background the override test defines: no Lighthouse `Normal`.
  private static let seaGreen: UInt32 = 0x2E_8B57
  /// The grid of the cmdline strip, whose frame defaults the titlebar reads.
  private static let outerGrid: UInt64 = 1
  /// The width of each column of the printed table.
  private static let columnWidth = 13

  /// Whether the bridge's `colour` is set and is `rgb`.
  private static func matches(_ colour: FfiColor, _ rgb: UInt32) -> Bool {
    colour.set && colour.rgb == rgb
  }

  /// `background` as `#rrggbb`, or `system`.
  private static func describe(_ background: TitlebarBackground) -> String {
    switch background {
    case .colour(let rgb): return String(format: "#%06x", rgb)
    case .system: return "system"
    }
  }

  /// One row of the printed table, each cell padded to `columnWidth`.
  private static func row(_ cells: [String]) -> String {
    cells.map { $0.padding(toLength: max($0.count, columnWidth), withPad: " ", startingAt: 0) }
      .joined(separator: "  ")
  }

  /// The printed table's header.
  private static let header = row([
    "background", "Normal bg", "read_frame bg", "computed", "window", "grid mean", "grid share",
  ])

  /// Whether `appearance` is the dark one of Aqua and Dark Aqua.
  private static func isDark(_ appearance: NSAppearance) -> Bool {
    appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
  }

  /// The premise failure for a stage whose wake never saw `Normal`'s background.
  private static func unreached(
    _ background: LighthouseBackground, _ normalBg: UInt32, _ handle: WolfCoreHandle
  ) -> String {
    "premise, \(background.rawValue): the frame's default background never became"
      + " Lighthouse's \(background.rawValue) Normal background"
      + " \(String(format: "#%06x", normalBg)); read_frame() has"
      + " \(hex(handle.read_frame().default_bg())) and grid 1"
      + " \(hex(handle.read_frame_for_grid(outerGrid).default_bg())). At the dark stage, unset or"
      + " another colour means init.lua did not activate the staged Lighthouse; at the light"
      + " stage, the dark value means set_background(false) did not reload the colorscheme"
  }

  /// Wakes until both `read_frame()` and grid 1 carry `normalBg`, then once more, since the
  /// condition is checked after the wake that applied it. Fails the test and returns false
  /// when they never do.
  private func reachNormal(
    _ fixture: TitlebarFixture, _ background: LighthouseBackground, _ normalBg: UInt32
  ) -> Bool {
    let handle = fixture.handle
    let reached = fixture.wake(until: {
      Self.matches(handle.read_frame().default_bg(), normalBg)
        && Self.matches(handle.read_frame_for_grid(Self.outerGrid).default_bg(), normalBg)
    })
    guard reached else {
      XCTFail(Self.unreached(background, normalBg, handle))
      return false
    }
    fixture.container.onRenderWake()
    return true
  }

  /// With Lighthouse active, the window's background, which shows through the titlebar, is
  /// `Normal`'s background and the grid's, in dark and, after `set_background(false)`, in light.
  func testTheTitlebarTakesLighthousesNormalBackgroundInDarkAndLight() throws {
    guard let lighthouse = try stageLighthouse() else { return }
    guard let fixture = TitlebarFixture(initLua: lighthouse.initLua) else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let window = fixture.window
    for background in [LighthouseBackground.dark, .light] {
      let normal = try lighthouse.colours.normal(background)
      if background == .light { handle.set_background(false) }
      guard reachNormal(fixture, background, normal.bg) else { return }

      let capture = OffscreenRenderer.capture(container)
      let bounds = container.bounds
      let block = NSRect(
        x: bounds.midX, y: bounds.midY, width: Self.blockSide, height: Self.blockSide)
      let expected = srgbColour(normal.bg)
      let pixels = capture.colors(in: block, of: container)
      let near = pixels.filter { colorDistance($0, expected) <= Self.tolerance }.count
      let share = pixels.isEmpty ? 0 : Double(near) / Double(pixels.count)
      let gridMean = pixels.isEmpty ? nil : meanColour(pixels)
      let gridMeanText = gridMean.map(describeColour) ?? "none"
      let windowColour: NSColor = window.backgroundColor
      let windowText = describeColour(windowColour)
      let normalBg = String(format: "#%06x", normal.bg)
      let computed = TitlebarBackground(
        titlebar: handle.theme_titlebar_bg(),
        normal: handle.read_frame_for_grid(Self.outerGrid).default_bg())
      let table =
        Self.header + "\n"
        + Self.row([
          background.rawValue, normalBg, hex(handle.read_frame().default_bg()),
          Self.describe(computed), windowText, gridMeanText, String(format: "%.3f", share),
        ])
      print(table)

      XCTAssertGreaterThanOrEqual(
        share, Self.minShare,
        "premise, \(background.rawValue): at least \(Self.minShare) of the grid block \(block)"
          + " must be within \(Self.tolerance) sRGB of Normal's background \(normalBg), as the"
          + " grid paints its frame's default background, but its share is"
          + " \(String(format: "%.3f", share)) with mean \(gridMeanText). The titlebar is not what"
          + " failed.\n\(table)")
      XCTAssertLessThanOrEqual(
        colorDistance(windowColour, expected), Self.windowTolerance,
        "\(background.rawValue): the window's background, which shows through the titlebar, must"
          + " be Lighthouse's Normal background \(normalBg) in sRGB, but it is \(windowText)."
          + " The system window colour means the titlebar follows WolfTitlebar alone, which"
          + " Lighthouse leaves unset; about 0.06 off \(normalBg) means it is built in another"
          + " colour space than the grid's sRGB; the other background's Normal means it did not"
          + " follow set_background; black means it took theme_titlebar_bg()'s fallback.\n\(table)")
      if let gridMean {
        XCTAssertLessThanOrEqual(
          colorDistance(windowColour, gridMean), Self.tolerance,
          "\(background.rawValue): the window's background \(windowText) must match the grid it"
            + " sits above, whose mean is \(gridMeanText): a gap means the titlebar and the grid"
            + " take different colours or colour spaces.\n\(table)")
      }
    }
  }

  /// A wake with no theme change makes no AppKit write: a colour set on the window behind
  /// `applyTitlebarTheme`'s back survives three wakes.
  func testAWakeWithNoThemeChangeLeavesTheWindowBackgroundAlone() throws {
    guard let lighthouse = try stageLighthouse() else { return }
    guard let fixture = TitlebarFixture(initLua: lighthouse.initLua) else { return }
    defer { fixture.shutdown() }
    let window = fixture.window
    let normal = try lighthouse.colours.normal(.dark)
    guard reachNormal(fixture, .dark, normal.bg) else { return }
    guard colorDistance(window.backgroundColor, srgbColour(normal.bg)) <= Self.windowTolerance
    else {
      XCTFail(
        "premise: the window must first take Normal's background"
          + " \(String(format: "#%06x", normal.bg)), so the wake is known to reach it, but it is"
          + " \(describeColour(window.backgroundColor)); the background test says why")
      return
    }

    let magenta = NSColor(srgbRed: 1, green: 0, blue: 1, alpha: 1)
    window.backgroundColor = magenta
    for _ in 0..<3 { fixture.container.onRenderWake() }
    XCTAssertLessThanOrEqual(
      colorDistance(window.backgroundColor, magenta), Self.windowTolerance,
      "three wakes with an unchanged theme must leave the window's background alone, but the"
        + " magenta set on it became \(describeColour(window.backgroundColor)): Normal's"
        + " \(String(format: "#%06x", normal.bg)) means applyTitlebarTheme writes the window on"
        + " every wake instead of only when the titlebar background changes")
  }

  /// With Lighthouse active, a `WolfTitlebar` defined at runtime colours the titlebar instead
  /// of `Normal`, and clearing it restores `Normal`'s background.
  ///
  /// `doautocmd ColorScheme` runs the `wolf_theme` emit that a `:colorscheme` would, so core
  /// learns the group; `hi clear` leaves it with no background, which core reads as unset.
  func testADefinedWolfTitlebarOverridesNormalAndClearingItRestoresNormal() throws {
    guard let lighthouse = try stageLighthouse() else { return }
    guard let fixture = TitlebarFixture(initLua: lighthouse.initLua) else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let window = fixture.window
    let normal = try lighthouse.colours.normal(.dark)
    let normalBg = String(format: "#%06x", normal.bg)
    let green = String(format: "#%06x", Self.seaGreen)
    guard reachNormal(fixture, .dark, normal.bg) else { return }

    fixture.typeEx("hi WolfTitlebar guibg=\(green)")
    fixture.typeEx("doautocmd ColorScheme")
    guard fixture.wake(until: { Self.matches(handle.theme_titlebar_bg(), Self.seaGreen) }) else {
      XCTFail(
        "premise: theme_titlebar_bg() never became \(green) after defining WolfTitlebar and"
          + " firing ColorScheme; it is \(hex(handle.theme_titlebar_bg())), so the typed"
          + " commands or the wolf_theme emit did not reach core")
      return
    }
    fixture.container.onRenderWake()
    XCTAssertLessThanOrEqual(
      colorDistance(window.backgroundColor, srgbColour(Self.seaGreen)), Self.windowTolerance,
      "a defined WolfTitlebar must colour the titlebar \(green), but the window is"
        + " \(describeColour(window.backgroundColor)). Normal's \(normalBg) means Normal is"
        + " preferred to a set WolfTitlebar; the system colour means neither is applied")

    fixture.typeEx("hi clear WolfTitlebar")
    fixture.typeEx("doautocmd ColorScheme")
    guard fixture.wake(until: { !handle.theme_titlebar_bg().set }) else {
      XCTFail(
        "premise: theme_titlebar_bg() never became unset after clearing WolfTitlebar and firing"
          + " ColorScheme; it is \(hex(handle.theme_titlebar_bg()))")
      return
    }
    fixture.container.onRenderWake()
    XCTAssertLessThanOrEqual(
      colorDistance(window.backgroundColor, srgbColour(normal.bg)), Self.windowTolerance,
      "clearing WolfTitlebar must restore Normal's background \(normalBg), but the window is"
        + " \(describeColour(window.backgroundColor)). The system colour means an unset"
        + " WolfTitlebar reverts the titlebar instead of falling back to Normal; \(green) means"
        + " the clear never reached the window")
  }

  /// The window, and so its title, draws in the appearance `AppDelegate.appAppearance(for:)`
  /// forces on the app for each forced mode, whatever the host's. The leg that differs from the
  /// host is the request's case: on a light host, `appearance: dark` must draw dark.
  ///
  /// Premise: a window whose own `appearance` is `nil` inherits `NSApp.appearance`, and AppKit
  /// draws the title in that appearance's title colours; the printed rows record the colour.
  func testTheWindowDrawsInTheAppearanceWolfForcesWhateverTheSystem() throws {
    guard let lighthouse = try stageLighthouse() else { return }
    guard let fixture = TitlebarFixture(initLua: lighthouse.initLua) else { return }
    defer { fixture.shutdown() }
    let window = fixture.window
    let app = NSApplication.shared
    let saved = app.appearance
    defer { app.appearance = saved }

    app.appearance = nil
    let hostDark = Self.isDark(app.effectiveAppearance)
    let host = hostDark ? "dark" : "light"
    print("host appearance: \(host)")

    for mode in [AppearanceMode.dark, .light] {
      app.appearance = AppDelegate.appAppearance(for: mode)
      let wantDark = mode == .dark
      let reached = fixture.live.waitFor(timeout: TitlebarFixture.timeout) {
        Self.isDark(window.effectiveAppearance) == wantDark
      }
      let windowDark = Self.isDark(window.effectiveAppearance)
      var titleColour = "?"
      window.effectiveAppearance.performAsCurrentDrawingAppearance {
        titleColour = describeColour(NSColor.windowFrameTextColor)
      }
      print(
        "mode \(mode)  host \(host)  window \(windowDark ? "dark" : "light")"
          + "  windowFrameTextColor \(titleColour)")

      XCTAssertTrue(
        reached,
        "appearance: \(mode) on a \(host) host: the window must draw in the"
          + " \(wantDark ? "dark" : "light") appearance, so its title takes that appearance's"
          + " colours, but it draws \(windowDark ? "dark" : "light"). The host's appearance means"
          + " the window pins its own appearance or appAppearance(for:) forces nothing; the"
          + " opposite of both means appAppearance(for:) maps the mode to the wrong appearance")
      XCTAssertEqual(
        windowDark, mode.resolvesDark(systemDark: hostDark),
        "appearance: \(mode) on a \(host) host: the window's appearance must be the one"
          + " resolvesDark pushes to Neovim, or the title and the editor follow different"
          + " appearances")
    }
  }
}
