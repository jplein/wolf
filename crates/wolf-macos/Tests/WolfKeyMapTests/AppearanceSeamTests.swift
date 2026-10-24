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
}
