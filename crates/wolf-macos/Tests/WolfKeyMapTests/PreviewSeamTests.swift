//
//  PreviewSeamTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.PreviewSeam`.
//
//  These exercise the pure pane-placement decision the coder adds in
//  `Sources/WolfKeyMap/PreviewSeam.swift` (plan §"Pane placement", OD3). Like
//  `FontSizeSeamTests` / `CommandSeamTests`, they import ONLY Foundation-level test
//  machinery (no AppKit, no WKWebView, no live Neovim) and stay in the headless
//  `WolfKeyMap` module. The actual split / `nvim_win_set_buf` mechanism is core +
//  AppKit glue (manual-smoke); only the placement string arithmetic is asserted.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor)
//
//    public enum PreviewSeam {
//        // A single-pane (or empty) window splits a new pane to the RIGHT; a
//        // window that already has more than one pane shows the preview in the
//        // ACTIVE pane. (Requirement 4 / OD3.)
//        static func placementMode(paneCount: Int) -> String  // "split_right" | "active"
//    }
//
//  The returned strings are the exact `mode` argument the coder threads into core's
//  `wolf_open_preview(mode)` Lua helper (`"split_right"` reuses the `:vsplit`
//  intent path; `"active"` is a bare `nvim_win_set_buf`).
//

import XCTest

@testable import WolfKeyMap

final class PreviewSeamTests: XCTestCase {

    // MARK: - a single pane splits a new pane to the right

    func testSinglePaneSplitsRight() {
        XCTAssertEqual(
            PreviewSeam.placementMode(paneCount: 1), "split_right",
            "a single-pane window opens the preview in a new pane to the RIGHT")
    }

    // MARK: - more than one pane replaces the active pane

    func testMultiPaneUsesActivePane() {
        XCTAssertEqual(
            PreviewSeam.placementMode(paneCount: 2), "active",
            "a multi-pane window opens the preview in the ACTIVE pane")
    }

    // MARK: - a degenerate/empty count is treated as single-pane (split right)

    func testZeroPaneCountSplitsRight() {
        XCTAssertEqual(
            PreviewSeam.placementMode(paneCount: 0), "split_right",
            "an empty pane count (<= 1) falls to the split-right branch")
    }
}
