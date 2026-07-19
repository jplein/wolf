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
//    closePane = 8, newBuffer = 9, resetFontSize = 10, increaseFontSize = 11,
//    decreaseFontSize = 12, commandPalette = 13, copy = 14, paste = 15,
//    bufferPicker = 16, newTab = 17, preview = 18, edit = 19, terminal = 20 (the
//    range 0..20). Any out-of-range discriminant decodes to `nil`.
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
    XCTAssertEqual(CommandKind(ffi: 8), .closePane, "ffi 8 -> .closePane")
    XCTAssertEqual(CommandKind(ffi: 9), .newBuffer, "ffi 9 -> .newBuffer")
    XCTAssertEqual(
      CommandKind(ffi: 10), .resetFontSize, "ffi 10 -> .resetFontSize")
    XCTAssertEqual(
      CommandKind(ffi: 11), .increaseFontSize, "ffi 11 -> .increaseFontSize")
    XCTAssertEqual(
      CommandKind(ffi: 12), .decreaseFontSize, "ffi 12 -> .decreaseFontSize")
    XCTAssertEqual(
      CommandKind(ffi: 13), .commandPalette, "ffi 13 -> .commandPalette")
    XCTAssertEqual(CommandKind(ffi: 14), .copy, "ffi 14 -> .copy")
    XCTAssertEqual(CommandKind(ffi: 15), .paste, "ffi 15 -> .paste")
    XCTAssertEqual(
      CommandKind(ffi: 16), .bufferPicker, "ffi 16 -> .bufferPicker")
    XCTAssertEqual(CommandKind(ffi: 17), .newTab, "ffi 17 -> .newTab")
    XCTAssertEqual(CommandKind(ffi: 18), .preview, "ffi 18 -> .preview")
    XCTAssertEqual(CommandKind(ffi: 19), .edit, "ffi 19 -> .edit")
    XCTAssertEqual(CommandKind(ffi: 20), .terminal, "ffi 20 -> .terminal")
  }

  // MARK: - out-of-range discriminants decode to nil

  // The first out-of-range probe tracks the registry's high-water mark: it was 20
  // while `edit` was the last case, and moves to 21 now that `terminal` claims 20.
  // Appending a command WITHOUT bumping this probe turns the new case's decode into
  // a silent nil assertion, so it must be updated in lockstep with the enum.
  func testCommandKindOutOfRangeIsNil() {
    XCTAssertNil(CommandKind(ffi: 21), "the first out-of-range discriminant -> nil")
    XCTAssertNil(CommandKind(ffi: 255), "a far out-of-range discriminant -> nil")
  }
}
