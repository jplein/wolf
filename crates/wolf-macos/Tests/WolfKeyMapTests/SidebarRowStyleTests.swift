// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#sidebarrowstyletestsswift

import XCTest

@testable import WolfKeyMap

final class SidebarRowStyleTests: XCTestCase {

  /// The current buffer's row is accented, and an ordinary row is not.
  func testCurrentRowIsAccented() {
    XCTAssertEqual(
      SidebarRowStyle.labelColor(isCurrent: true, isEmphasized: false), .current)
    XCTAssertEqual(
      SidebarRowStyle.labelColor(isCurrent: false, isEmphasized: false), .normal)
  }

  /// An emphasized (accent-filled) selection wins over the current-buffer accent,
  /// whichever row it lands on: accent text on an accent fill is unreadable.
  func testEmphasizedSelectionWins() {
    XCTAssertEqual(
      SidebarRowStyle.labelColor(isCurrent: true, isEmphasized: true), .selected)
    XCTAssertEqual(
      SidebarRowStyle.labelColor(isCurrent: false, isEmphasized: true), .selected)
  }

  /// A previewing pane marks its Markdown source, not the edited buffer — the
  /// case nvim reports as an empty `current_file`, and the one an edit pane's
  /// nil source falls through.
  func testPreviewSourceIsMarked() {
    XCTAssertEqual(
      SidebarRowStyle.markedFile(previewSource: "/w/a.md", currentFile: ""), "/w/a.md")
    XCTAssertEqual(
      SidebarRowStyle.markedFile(previewSource: "/w/a.md", currentFile: "/w/b.swift"),
      "/w/a.md")
    XCTAssertEqual(
      SidebarRowStyle.markedFile(previewSource: nil, currentFile: "/w/b.swift"), "/w/b.swift")
  }

  /// A pathless preview source falls back to the edited buffer rather than
  /// clearing the mark, and with neither there is nothing to mark.
  func testEmptyPreviewSourceFallsBack() {
    XCTAssertEqual(
      SidebarRowStyle.markedFile(previewSource: "", currentFile: "/w/b.swift"), "/w/b.swift")
    XCTAssertEqual(SidebarRowStyle.markedFile(previewSource: nil, currentFile: ""), "")
  }
}
