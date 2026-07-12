//
//  AppearanceSeamTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.AppearanceMode`
//  discriminant contract.
//
//  These are the sibling of `CommandSeamTests`' `CommandKind(ffi:)` discriminant
//  coverage: a pure, display-free check exercised with scalar inputs only — NO
//  AppKit, NO NSApp, NO NSAppearance, NO live Neovim. `AppearanceMode` mirrors the
//  FFI appearance discriminant that `FfiConfig::appearance() -> u8` delivers, which
//  `AppDelegate` decodes to resolve dark/light.
//
//  DISCRIMINANT CONTRACT UNDER TEST (must match `mod appearance` in
//  `wolf-ffi/src/lib.rs` and `wolf_core::config::Appearance`, plan OD-a / §3.5):
//    auto = 0, dark = 1, light = 2. Any other discriminant is out of range.
//
//  NOTE to coder: this references the `WolfKeyMap.AppearanceMode` enum the coder
//  adds (Foundation only, no AppKit), declared `enum AppearanceMode: UInt8 { auto =
//  0; dark = 1; light = 2 }` (plan §3.7). The raw values + round-trip asserted below
//  are the contract the implementation must satisfy.
//

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
