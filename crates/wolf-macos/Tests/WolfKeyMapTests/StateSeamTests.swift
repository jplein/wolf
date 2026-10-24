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
}
