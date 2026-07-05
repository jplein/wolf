//
//  CommandSeamTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.CommandKind` seam.
//
//  These are the sibling of `PaneTreeTests`' `Orientation(ffi:)` / `FocusDir(ffi:)`
//  discriminant coverage: a pure, display-free decode exercised with scalar inputs
//  only — NO AppKit, NO NSMenu, NO NSMenuItem, NO live Neovim. The seam decodes the
//  FFI command-kind `u8` (delivered in an `NSMenuItem.tag` or by a drained
//  `FfiCommand`) into a typed `CommandKind` case.
//
//  DISCRIMINANT CONTRACT UNDER TEST (must match `mod command_kind` in
//  `wolf-ffi/src/lib.rs` and the `wolf_core::Command` variant order, plan §3):
//    newWindow = 0, closeWindow = 1, quitApplication = 2, hideApplication = 3,
//    minimizeWindow = 4, goToFile = 5, openFile = 6, openFolder = 7,
//    closeFile = 8. Any out-of-range discriminant decodes to `nil`.
//
//  NOTE to coder: this references the pure `WolfKeyMap.CommandKind` enum the coder
//  adds in `Sources/WolfKeyMap/CommandSeam.swift` (Foundation only, no AppKit),
//  with a guarded `init?(ffi: UInt8)`. The cases + decode asserted below are the
//  contract the implementation must satisfy.
//

import XCTest

@testable import WolfKeyMap

final class CommandSeamTests: XCTestCase {

    // MARK: - in-range discriminants map to their case (FFI order)

    func testCommandKindFfiDiscriminants() {
        XCTAssertEqual(CommandKind(ffi: 0), .newWindow, "ffi 0 -> .newWindow")
        XCTAssertEqual(CommandKind(ffi: 1), .closeWindow, "ffi 1 -> .closeWindow")
        XCTAssertEqual(
            CommandKind(ffi: 2), .quitApplication, "ffi 2 -> .quitApplication")
        XCTAssertEqual(
            CommandKind(ffi: 3), .hideApplication, "ffi 3 -> .hideApplication")
        XCTAssertEqual(
            CommandKind(ffi: 4), .minimizeWindow, "ffi 4 -> .minimizeWindow")
        XCTAssertEqual(CommandKind(ffi: 5), .goToFile, "ffi 5 -> .goToFile")
        XCTAssertEqual(CommandKind(ffi: 6), .openFile, "ffi 6 -> .openFile")
        XCTAssertEqual(CommandKind(ffi: 7), .openFolder, "ffi 7 -> .openFolder")
        XCTAssertEqual(CommandKind(ffi: 8), .closeFile, "ffi 8 -> .closeFile")
    }

    // MARK: - out-of-range discriminants decode to nil

    func testCommandKindOutOfRangeIsNil() {
        XCTAssertNil(CommandKind(ffi: 9), "the first out-of-range discriminant -> nil")
        XCTAssertNil(CommandKind(ffi: 255), "a far out-of-range discriminant -> nil")
    }
}
