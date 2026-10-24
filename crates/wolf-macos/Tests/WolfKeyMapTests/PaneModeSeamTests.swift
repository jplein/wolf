// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#panemodeseamtestsswift

import XCTest

@testable import WolfKeyMap

final class PaneModeSeamTests: XCTestCase {

  // MARK: - the active win previewing -> preview mode

  func testActiveWinInPreviewWinsIsPreview() {
    XCTAssertEqual(
      PaneModeSeam.mode(activeWin: 10, previewWins: Set<Int64>([10, 11])), .preview,
      "an active window that carries a preview puts the pane in PREVIEW mode")
  }

  // MARK: - the active win absent from the set -> edit mode

  func testActiveWinAbsentFromPreviewWinsIsEdit() {
    XCTAssertEqual(
      PaneModeSeam.mode(activeWin: 12, previewWins: Set<Int64>([10, 11])), .edit,
      "an active window with no preview of its own is an EDIT pane")
  }

  // MARK: - the unknown/degenerate activeWin == 0 -> edit mode

  func testZeroActiveWinIsEdit() {
    XCTAssertEqual(
      PaneModeSeam.mode(activeWin: 0, previewWins: Set<Int64>([10])), .edit,
      "activeWin == 0 (no bound pane yet) must fall back to EDIT mode, even with previews live elsewhere"
    )
  }

  // MARK: - an empty preview set -> edit mode

  func testEmptyPreviewWinsIsEdit() {
    XCTAssertEqual(
      PaneModeSeam.mode(activeWin: 10, previewWins: Set<Int64>()), .edit,
      "with no previews anywhere every pane is an EDIT pane")
  }

  // MARK: - only ANOTHER pane previewing -> the active pane stays edit mode

  func testOtherPanePreviewingKeepsActivePaneEdit() {
    XCTAssertEqual(
      PaneModeSeam.mode(activeWin: 10, previewWins: Set<Int64>([11])), .edit,
      "a preview in a DIFFERENT pane must not flip the active pane to preview mode — the mode is per-pane, keyed on the active pane's own window"
    )
  }
}
