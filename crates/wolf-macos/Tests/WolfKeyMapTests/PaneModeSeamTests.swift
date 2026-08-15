//
//  PaneModeSeamTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.PaneModeSeam`.
//
//  These exercise the pure per-pane mode decision in
//  `Sources/WolfKeyMap/PaneModeSeam.swift`: each pane is in PREVIEW mode or EDIT
//  mode, derived — never stored — from whether the pane's nvim window appears in
//  the `previews()` snapshot. The Preview and Edit commands act IN PLACE on the
//  active pane, so the only front-end question is which action a picker /
//  open-dialog activation routes to (preview mode → `preview_file` /
//  `preview_buffer`; edit mode → `open` / `switch_buffer`). The pinned contract:
//  the active pane is in preview mode iff its OWN window is in the set, and
//  `activeWin == 0` (unknown / no bound pane) is always edit mode. Like
//  `PreviewSeamTests` / `CommandSeamTests`, they import ONLY Foundation-level
//  test machinery (no AppKit, no WKWebView, no live Neovim) and stay in the
//  headless `WolfKeyMap` module.
//

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
