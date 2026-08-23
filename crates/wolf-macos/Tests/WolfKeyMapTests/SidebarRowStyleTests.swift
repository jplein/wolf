//
//  SidebarRowStyleTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.SidebarRowStyle` seam — the
//  file-tree sidebar's choice of label color for one row.
//
//  What matters here is the property the seam exists for: the current-buffer
//  accent depends ONLY on the current-buffer flag, never on selection or window
//  focus, so it cannot go missing the way a selection tint does after an app
//  switch. The emphasized-selection case is the single documented exception,
//  where an accent fill would otherwise swallow accent text.
//

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
}
