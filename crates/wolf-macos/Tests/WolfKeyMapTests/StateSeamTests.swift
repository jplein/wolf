// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#stateseamtestsswift

import CoreGraphics
import XCTest

@testable import WolfKeyMap

/// A typical main-screen visible frame (menu bar + Dock already subtracted).
private let kMainScreen = CGRect(x: 0, y: 0, width: 1920, height: 1055)
/// A second display arranged to the right of the main screen.
private let kSecondScreen = CGRect(x: 1920, y: 0, width: 1440, height: 900)

final class StateSeamTests: XCTestCase {

  // MARK: - restoredWindowFrame

  /// One `restoredWindowFrame` table row: (description, saved geometry,
  /// screen visible frames, expected; nil == "do not restore, use defaults").
  private struct FrameCase {
    let desc: String
    let x: Double
    let y: Double
    let width: Double
    let height: Double
    let screens: [CGRect]
    let expected: CGRect?
  }

  func testRestoredWindowFrameTable() {
    let cases: [FrameCase] = [
      // --- accepted: intersects a screen visible frame -----------------
      FrameCase(
        desc: "fully on the main screen => restored as-is",
        x: 128, y: 64, width: 1180, height: 760,
        screens: [kMainScreen],
        expected: CGRect(x: 128, y: 64, width: 1180, height: 760)),
      FrameCase(
        desc: "partially off the left edge but intersecting => restored",
        x: -600, y: 100, width: 800, height: 600,
        screens: [kMainScreen],
        expected: CGRect(x: -600, y: 100, width: 800, height: 600)),
      FrameCase(
        desc: "on the SECOND screen only => restored (any screen counts)",
        x: 2000, y: 50, width: 800, height: 600,
        screens: [kMainScreen, kSecondScreen],
        expected: CGRect(x: 2000, y: 50, width: 800, height: 600)),
      // --- rejected: non-positive size (the "unset" sentinel) ----------
      FrameCase(
        desc: "zero width (unset sentinel) => nil",
        x: 128, y: 64, width: 0, height: 760,
        screens: [kMainScreen],
        expected: nil),
      FrameCase(
        desc: "zero height (unset sentinel) => nil",
        x: 128, y: 64, width: 1180, height: 0,
        screens: [kMainScreen],
        expected: nil),
      FrameCase(
        desc: "zero-everything (fresh default state) => nil",
        x: 0, y: 0, width: 0, height: 0,
        screens: [kMainScreen],
        expected: nil),
      FrameCase(
        desc: "negative width => nil",
        x: 128, y: 64, width: -100, height: 760,
        screens: [kMainScreen],
        expected: nil),
      FrameCase(
        desc: "negative height => nil",
        x: 128, y: 64, width: 1180, height: -1,
        screens: [kMainScreen],
        expected: nil),
      // --- rejected: fully offscreen -----------------------------------
      FrameCase(
        desc: "fully right of every screen => nil",
        x: 5000, y: 100, width: 800, height: 600,
        screens: [kMainScreen, kSecondScreen],
        expected: nil),
      FrameCase(
        desc: "fully below every screen => nil",
        x: 100, y: -2000, width: 800, height: 600,
        screens: [kMainScreen],
        expected: nil),
      FrameCase(
        desc: "no screens at all => nil",
        x: 128, y: 64, width: 1180, height: 760,
        screens: [],
        expected: nil),
    ]

    for c in cases {
      let got = StateSeam.restoredWindowFrame(
        x: c.x, y: c.y, width: c.width, height: c.height,
        screenVisibleFrames: c.screens)
      XCTAssertEqual(got, c.expected, "case: \(c.desc)")
    }
  }

  // MARK: - launchDecision

  /// One `launchDecision` table row: (description, restored cwd, the set of
  /// paths the injected `isDirectory` closure reports as directories, expected
  /// decision). `home` and `processCwd` are shared across the table below.
  private struct DecisionCase {
    let desc: String
    let restored: String
    let directories: Set<String>
    let expected: StateSeam.LaunchDecision
  }

  func testLaunchDecisionTable() {
    let restoredDir = "/Users/alice/projects/notes"
    let home = "/Users/alice"
    // Stands in for `FileManager.default.currentDirectoryPath`; it is in no `directories` set, and
    // the seam must never return it, since that pollutes the recent-dirs MRU.
    let processCwd = "/private/var/tmp/bazel-sandbox/execroot/cwd"

    let cases: [DecisionCase] = [
      DecisionCase(
        desc: "valid restored directory => reopen that folder",
        restored: restoredDir,
        directories: [restoredDir, home],
        expected: .folder(cwd: restoredDir)),
      DecisionCase(
        desc: "empty restored (fresh / no prior session) => empty state at home",
        restored: "",
        directories: [home],
        expected: .emptyState(nvimCwd: home)),
      DecisionCase(
        desc: "restored no longer a directory (deleted since) => empty state at home",
        restored: restoredDir,
        directories: [home],
        expected: .emptyState(nvimCwd: home)),
      DecisionCase(
        desc: "restored is a plain FILE (not a dir) => empty state at home",
        restored: "/Users/alice/notes.md",
        directories: [home],
        expected: .emptyState(nvimCwd: home)),
      DecisionCase(
        desc: "restored is a non-existent path => empty state at home",
        restored: "/Users/alice/gone",
        directories: [home],
        expected: .emptyState(nvimCwd: home)),
    ]

    for c in cases {
      let got = StateSeam.launchDecision(
        restored: c.restored,
        home: home,
        isDirectory: { c.directories.contains($0) })
      XCTAssertEqual(got, c.expected, "case: \(c.desc)")

      // The key regression guard: neither branch's directory is ever the process working directory;
      // empty state uses `home`, the folder branch the restored dir.
      let resultDir: String
      switch got {
      case .emptyState(let nvimCwd): resultDir = nvimCwd
      case .folder(let cwd): resultDir = cwd
      }
      XCTAssertNotEqual(
        resultDir, processCwd,
        "launchDecision must NEVER return the process cwd, case: \(c.desc)")
    }
  }

  // MARK: - normalFrame

  func testNormalFrameKeepsTheLastNormalFrameWhileZoomedOrFullScreen() {
    let previous = CGRect(x: 100, y: 80, width: 1200, height: 800)
    let current = CGRect(x: 0, y: 0, width: 1920, height: 1055)
    XCTAssertEqual(
      StateSeam.normalFrame(previous: previous, frame: current, zoomed: false, fullScreen: false),
      current,
      "a window neither zoomed nor full screen is in its normal frame, so it must be recorded; "
        + "the previous frame means a move or resize is never saved")
    XCTAssertEqual(
      StateSeam.normalFrame(previous: previous, frame: current, zoomed: true, fullScreen: false),
      previous,
      "a zoomed window's frame is not its normal frame, so the last normal frame must be kept; "
        + "the zoomed frame means a relaunch opens the window at its zoomed size")
    XCTAssertEqual(
      StateSeam.normalFrame(previous: previous, frame: current, zoomed: false, fullScreen: true),
      previous,
      "a full-screen window's frame is not its normal frame, so the last normal frame must be "
        + "kept; the screen-sized frame means a relaunch opens at full-screen size")
    XCTAssertNil(
      StateSeam.normalFrame(previous: nil, frame: current, zoomed: true, fullScreen: false),
      "a window zoomed before any normal frame was recorded has none to report")
  }

  // MARK: - newWindowFrame

  /// The active window of the §7 cases: in its normal frame, 1000 × 700, top-left (100, 900).
  private static let activeNormal = CGRect(x: 100, y: 200, width: 1000, height: 700)
  /// The size every cascaded case expects: `activeNormal`'s.
  private static let normalSize = CGSize(width: 1000, height: 700)

  /// An active window whose recorded normal frame is `activeNormal` and whose live frame is
  /// `frame`.
  private static func active(
    frame: CGRect = activeNormal, zoomed: Bool = false, fullScreen: Bool = false
  ) -> StateSeam.ActiveWindow {
    StateSeam.ActiveWindow(
      normalFrame: activeNormal, frame: frame, zoomed: zoomed, fullScreen: fullScreen)
  }

  /// M1a and M1b.
  func testANewWindowTakesTheActiveWindowsSize() {
    XCTAssertEqual(
      StateSeam.newWindowFrame(
        stored: nil, active: Self.active(), screenVisibleFrames: [kMainScreen]),
      .cascaded(topLeft: CGPoint(x: 100, y: 900), size: Self.normalSize),
      "a new window with no stored frame must take the active window's 1000 × 700 and cascade"
        + " from its top-left (100, 900) (§7); .system means the active window is ignored, and a"
        + " top-left of (100, 200) means the cascade starts from the bottom-left origin")
  }

  /// M2a and M2b.
  func testANewWindowFromAZoomedWindowTakesItsNormalSize() {
    let zoomed = Self.active(frame: CGRect(x: 0, y: 0, width: 1440, height: 875), zoomed: true)
    XCTAssertEqual(
      StateSeam.newWindowFrame(stored: nil, active: zoomed, screenVisibleFrames: [kMainScreen]),
      .cascaded(topLeft: CGPoint(x: 0, y: 875), size: Self.normalSize),
      "a new window from a zoomed window must take its normal 1000 × 700, never the zoomed"
        + " size, and cascade from where the window is, its live top-left (0, 875) (§7); a size"
        + " of 1440 × 875 means the zoom is not passed to normalFrame, a top-left of (100, 900)"
        + " means the cascade starts from the normal frame instead of the live one, (0, 0) means"
        + " it starts from the bottom-left, and .system means the active window is ignored")
  }

  /// M3a.
  func testANewWindowFromAFullScreenWindowTakesItsNormalSize() {
    let fullScreen = Self.active(
      frame: CGRect(x: 0, y: 0, width: 1440, height: 900), fullScreen: true)
    XCTAssertEqual(
      StateSeam.newWindowFrame(stored: nil, active: fullScreen, screenVisibleFrames: [kMainScreen]),
      .cascaded(topLeft: CGPoint(x: 0, y: 900), size: Self.normalSize),
      "a new window from a full-screen window must take its normal 1000 × 700, never the"
        + " screen's size, and cascade from where the window is, its live top-left (0, 900)"
        + " (§7); a size of 1440 × 900 means full screen is not passed to normalFrame, a top-left"
        + " of (100, 900) means the cascade starts from the normal frame instead of the live one,"
        + " (0, 0) means it starts from the bottom-left, and .system means the active window is"
        + " ignored")
  }

  /// M4a.
  func testANewWindowWithNoActiveWindowKeepsTheDefaultSizeAndPlacement() {
    XCTAssertEqual(
      StateSeam.newWindowFrame(stored: nil, active: nil, screenVisibleFrames: [kMainScreen]),
      .system,
      "with no active window and no stored frame, the new window must keep MainWindow's default"
        + " size and AppKit's placement (§7); .cascaded means a fixed size replaces the default")
  }

  /// M5a.
  func testAStoredFrameOnAScreenIsApplied() {
    let stored = CGRect(x: 128, y: 64, width: 1180, height: 760)
    XCTAssertEqual(
      StateSeam.newWindowFrame(
        stored: stored, active: Self.active(), screenVisibleFrames: [kMainScreen]),
      .stored(stored),
      "a stored frame on a screen must be applied as it is (§6); .cascaded at 1000 × 700 means"
        + " the active window's size wins over the project state's frame")
  }

  /// M6a.
  func testAStoredFrameOnNoScreenGivesWayToTheActiveWindowsSize() {
    let offscreen = CGRect(x: -5000, y: -5000, width: 900, height: 600)
    XCTAssertEqual(
      StateSeam.newWindowFrame(
        stored: offscreen, active: Self.active(), screenVisibleFrames: [kMainScreen]),
      .cascaded(topLeft: CGPoint(x: 100, y: 900), size: Self.normalSize),
      "a stored frame that intersects no screen is discarded (§5), which leaves no stored frame"
        + " to apply, so §7's active size and top-left (100, 900) must follow; .system means the"
        + " rejected frame falls back to the default size, .stored means it is applied offscreen,"
        + " and a top-left of (-5000, -4400) means the cascade starts from the rejected frame")
  }
}
