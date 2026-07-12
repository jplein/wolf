//
//  EditSeamTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.EditSeam`.
//
//  These exercise the pure pane-placement decision the coder adds in
//  `Sources/WolfKeyMap/EditSeam.swift` (plan §3 + §5). `EditSeam` is the INVERSE
//  of `PreviewSeam`: it decides where the SOURCE editor lands when the Edit command
//  fires from a preview pane. Like `PreviewSeamTests` / `CommandSeamTests`, they
//  import ONLY Foundation-level test machinery (no AppKit, no WKWebView, no live
//  Neovim) and stay in the headless `WolfKeyMap` module. The actual split /
//  `nvim_win_set_buf` mechanism is core + AppKit glue (manual-smoke); only the
//  placement string arithmetic is asserted.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor)
//
//    public enum EditSeam {
//        // Pure pane-placement decision. `paneCount` is the number of leaf panes
//        // in the active window; `activePaneIsPreview` is whether the active pane
//        // itself shows a preview (defaults to `true` — Edit is only meaningful
//        // FROM a preview pane); `otherEditorPaneCount` is how many OTHER panes
//        // are editors (non-preview).
//        static func placementMode(
//            paneCount: Int,
//            activePaneIsPreview: Bool = true,
//            otherEditorPaneCount: Int = 0
//        ) -> String  // "split_left" | "active" | "reuse_editor_pane"
//    }
//
//  The three modes:
//    * "split_left"        — `paneCount <= 1`: the preview is the ONLY pane, so open
//                            the source editor in a NEW pane to the LEFT
//                            (unconditional; ignores the two enriched inputs).
//    * "reuse_editor_pane" — exactly two panes, the active pane IS a preview, and
//                            exactly ONE other pane is an editor: bind the source
//                            into that OTHER (editor) pane and focus it.
//    * "active"            — every other case (incl. two panes with the reuse
//                            condition unmet, active pane NOT a preview, and any
//                            `paneCount > 2`): replace the preview in the ACTIVE
//                            pane with the source editor.
//
//  The returned strings are the exact `mode` argument the coder threads into core's
//  `wolf_edit_source(mode)` Lua helper.
//

import XCTest

@testable import WolfKeyMap

final class EditSeamTests: XCTestCase {

    // MARK: - a single (preview) pane opens the source in a new pane to the left

    func testSinglePaneSplitsLeft() {
        XCTAssertEqual(
            EditSeam.placementMode(paneCount: 1), "split_left",
            "a single-pane (preview-only) window opens the source in a new pane to the LEFT")
    }

    // MARK: - a degenerate/empty count is treated as single-pane (split left)

    func testZeroPaneCountSplitsLeft() {
        XCTAssertEqual(
            EditSeam.placementMode(paneCount: 0), "split_left",
            "an empty pane count (<= 1) falls to the split-left branch")
    }

    // MARK: - two panes, active is preview, the OTHER pane is an editor -> reuse it

    func testTwoPanesOtherEditorReusesEditorPane() {
        XCTAssertEqual(
            EditSeam.placementMode(
                paneCount: 2, activePaneIsPreview: true, otherEditorPaneCount: 1),
            "reuse_editor_pane",
            "two panes with a preview active and the OTHER pane an editor reuses that editor pane")
    }

    // MARK: - two panes, active is preview, but no OTHER editor -> active

    func testTwoPanesNoOtherEditorUsesActivePane() {
        XCTAssertEqual(
            EditSeam.placementMode(
                paneCount: 2, activePaneIsPreview: true, otherEditorPaneCount: 0),
            "active",
            "two panes with no OTHER editor pane fall to the active-pane branch")
    }

    // MARK: - defensive: the active pane is not itself a preview -> active

    func testTwoPanesActiveNotPreviewUsesActivePane() {
        XCTAssertEqual(
            EditSeam.placementMode(
                paneCount: 2, activePaneIsPreview: false, otherEditorPaneCount: 1),
            "active",
            "when the active pane is not itself a preview the reuse branch is skipped (active)")
    }

    // MARK: - scope: three panes stay on the active pane even with an editor present

    func testThreePanesWithEditorUsesActivePane() {
        XCTAssertEqual(
            EditSeam.placementMode(
                paneCount: 3, activePaneIsPreview: true, otherEditorPaneCount: 1),
            "active",
            "the reuse branch is scoped to exactly two panes; 3+ panes stay active")
    }
}
