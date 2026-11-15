// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#appearanceseamtestsswift

import XCTest

@testable import WolfKeyMap

final class AppearanceSeamTests: XCTestCase {

  // MARK: - raw values match the FFI `mod appearance` discriminants

  func testAppearanceModeRawValues() {
    XCTAssertEqual(AppearanceMode.auto.rawValue, 0, ".auto must be discriminant 0")
    XCTAssertEqual(AppearanceMode.dark.rawValue, 1, ".dark must be discriminant 1")
    XCTAssertEqual(AppearanceMode.light.rawValue, 2, ".light must be discriminant 2")
  }

  // MARK: - the u8 from `FfiConfig::appearance()` round-trips to its case

  func testAppearanceModeFromRawValue() {
    XCTAssertEqual(AppearanceMode(rawValue: 0), .auto, "raw 0 -> .auto")
    XCTAssertEqual(AppearanceMode(rawValue: 1), .dark, "raw 1 -> .dark")
    XCTAssertEqual(AppearanceMode(rawValue: 2), .light, "raw 2 -> .light")
  }

  // MARK: - out-of-range discriminants decode to nil

  func testAppearanceModeOutOfRangeIsNil() {
    XCTAssertNil(AppearanceMode(rawValue: 3), "the first out-of-range discriminant -> nil")
    XCTAssertNil(AppearanceMode(rawValue: 255), "a far out-of-range discriminant -> nil")
  }

  // MARK: - the appearance each mode forces on the app, and the one every title draws in

  /// One mode on one system appearance, with what it must force and resolve.
  private struct ModeRow {
    let mode: AppearanceMode
    let systemDark: Bool
    let forced: Bool?
    let resolved: Bool
    /// What the row stands for, for its failure message.
    let why: String
  }

  /// Each mode × system appearance: the appearance forced on the whole app, which every window
  /// and its title inherit, and the resolved appearance pushed to Neovim. A forced mode ignores
  /// the system, so an OS in light mode under `appearance: dark` draws the dark-appearance title.
  func testEachModeForcesAndResolvesItsAppearanceWhateverTheSystem() {
    let rows = [
      ModeRow(
        mode: .auto, systemDark: true, forced: nil, resolved: true, why: "auto follows a dark OS"),
      ModeRow(
        mode: .auto, systemDark: false, forced: nil, resolved: false, why: "auto follows a light OS"
      ),
      ModeRow(
        mode: .dark, systemDark: true, forced: true, resolved: true, why: "dark on a dark OS"),
      ModeRow(
        mode: .dark, systemDark: false, forced: true, resolved: true,
        why: "an OS in light mode under appearance: dark must draw the dark-appearance title,"
          + " since the light one is nearly invisible on Wolf's dark titlebar"),
      ModeRow(
        mode: .light, systemDark: true, forced: false, resolved: false,
        why: "light on a dark OS must draw the light-appearance title"),
      ModeRow(
        mode: .light, systemDark: false, forced: false, resolved: false, why: "light on a light OS"),
    ]
    for row in rows {
      let label = "(\(row.mode), OS \(row.systemDark ? "dark" : "light"))"
      let forced = String(describing: row.forced)
      XCTAssertEqual(
        row.mode.forcedDark, row.forced,
        "\(label): forcedDark must be \(forced): \(row.why). nil for a forced mode means the"
          + " app follows the OS; a value for auto means the app ignores the OS")
      XCTAssertEqual(
        row.mode.resolvesDark(systemDark: row.systemDark), row.resolved,
        "\(label): resolvesDark must be \(row.resolved): \(row.why). The OS's value under a"
          + " forced mode means Neovim and the title follow the OS, not Wolf's appearance")
    }
  }
}
