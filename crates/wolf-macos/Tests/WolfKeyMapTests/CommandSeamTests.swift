// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#commandseamtestsswift

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
    XCTAssertEqual(
      CommandKind(ffi: 21), .previewInBrowser, "ffi 21 -> .previewInBrowser")
    XCTAssertEqual(CommandKind(ffi: 22), .fullWidth, "ffi 22 -> .fullWidth")
    XCTAssertEqual(
      CommandKind(ffi: 23), .toggleSidebar, "ffi 23 -> .toggleSidebar")
    XCTAssertEqual(
      CommandKind(ffi: 24), .sidebarShowProject,
      "ffi 24 -> .sidebarShowProject")
    XCTAssertEqual(
      CommandKind(ffi: 25), .searchInProject, "ffi 25 -> .searchInProject")
    XCTAssertEqual(
      CommandKind(ffi: 26), .previewInOtherPane,
      "ffi 26 -> .previewInOtherPane")
    XCTAssertEqual(
      CommandKind(ffi: 27), .editInOtherPane, "ffi 27 -> .editInOtherPane")
  }

  // MARK: - out-of-range discriminants decode to nil

  // One past the last case (`editInOtherPane` = 27): bump it whenever a command is appended, or
  // the new case's decode becomes a silently passing nil assertion.
  func testCommandKindOutOfRangeIsNil() {
    XCTAssertNil(CommandKind(ffi: 28), "the first out-of-range discriminant -> nil")
    XCTAssertNil(CommandKind(ffi: 255), "a far out-of-range discriminant -> nil")
  }
}
