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
//        // Pure pane-placement decision. `paneCount` is the number of leaf panes
//        // in the active window; `activePaneIsPreview` is whether the active pane
//        // itself shows a preview; `otherPreviewPaneCount` is how many OTHER panes
//        // show a Markdown preview. The two enriched inputs default to
//        // `false` / `0`, preserving the original single-arg behavior.
//        static func placementMode(
//            paneCount: Int,
//            activePaneIsPreview: Bool = false,
//            otherPreviewPaneCount: Int = 0
//        ) -> String  // "split_right" | "active" | "reuse_preview_pane"
//    }
//
//  The three modes:
//    * "split_right"        — `paneCount <= 1`: split a NEW pane to the RIGHT
//                             (unconditional; ignores the two preview inputs).
//    * "reuse_preview_pane" — exactly two panes, the active pane is NOT itself a
//                             preview, and exactly ONE other pane is a preview:
//                             bind the preview into that OTHER (preview) pane.
//    * "active"             — every other case (incl. two panes with the reuse
//                             condition unmet, and any `paneCount > 2`): bind the
//                             preview into the ACTIVE pane via `nvim_win_set_buf`.
//
//  The returned strings are the exact `mode` argument the coder threads into core's
//  `wolf_open_preview(mode)` Lua helper.
//
//  ## CONTRACT UNDER TEST — preview reload gate (winsNeedingReload)
//
//    public enum PreviewSeam {
//        // A per-preview-WINDOW load key. Two freshly-opened files can share a
//        // `sourceTick`, so the gate must react to the bound SOURCE BUFFER
//        // identity changing as well as the tick advancing.
//        public struct PreviewLoad: Equatable {
//            public let sourceBufnr: Int64
//            public let sourceTick: UInt64
//            public init(sourceBufnr: Int64, sourceTick: UInt64)
//        }
//
//        // Returns the preview WINDOW handles (keys of `current`) whose
//        // `PreviewLoad` differs from the `loaded` entry for that same win.
//        static func winsNeedingReload(
//            current: [Int64: PreviewLoad],
//            loaded: [Int64: PreviewLoad]
//        ) -> [Int64]
//    }
//
//  A win is returned iff `loaded[win] != current[win]`, which fires on:
//    * FIRST BIND      — no `loaded` entry for the win yet.
//    * SOURCE CHANGED  — a different `sourceBufnr` even at an EQUAL `sourceTick`
//                        (the reused-preview-pane regression: a single window
//                        switching which source it previews must reload).
//    * TICK ADVANCED   — the same `sourceBufnr` but a newer `sourceTick`.
//  It does NOT fire when all of (win, sourceBufnr, sourceTick) match. Only
//  `current` is iterated, so a win present only in `loaded` is never returned.
//  The result ORDER is unspecified (dictionary iteration): assert membership via
//  `Set<Int64>`, never array equality.
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

    // MARK: - two panes whose OTHER pane is a preview reuses that preview pane

    func testTwoPanesOtherPreviewReusesPreviewPane() {
        XCTAssertEqual(
            PreviewSeam.placementMode(
                paneCount: 2, activePaneIsPreview: false, otherPreviewPaneCount: 1),
            "reuse_preview_pane",
            "two panes with an editor active and the OTHER pane a preview reuses that preview pane")
    }

    // MARK: - defensive: the active pane is itself a preview -> active

    func testTwoPanesActiveIsPreviewUsesActivePane() {
        XCTAssertEqual(
            PreviewSeam.placementMode(
                paneCount: 2, activePaneIsPreview: true, otherPreviewPaneCount: 1),
            "active",
            "when the active pane is itself a preview the reuse branch is skipped (active)")
    }

    // MARK: - two panes but the other pane is not a preview -> active

    func testTwoPanesNoOtherPreviewUsesActivePane() {
        XCTAssertEqual(
            PreviewSeam.placementMode(
                paneCount: 2, activePaneIsPreview: false, otherPreviewPaneCount: 0),
            "active",
            "two panes with no OTHER preview pane fall to the active-pane branch")
    }

    // MARK: - scope: three panes stay on the active pane even with a preview present

    func testThreePanesWithPreviewUsesActivePane() {
        XCTAssertEqual(
            PreviewSeam.placementMode(
                paneCount: 3, activePaneIsPreview: false, otherPreviewPaneCount: 1),
            "active",
            "the reuse branch is scoped to exactly two panes; 3+ panes stay active")
    }

    // MARK: - a single pane dominates regardless of the preview inputs

    func testSinglePaneDominatesPreviewInputs() {
        XCTAssertEqual(
            PreviewSeam.placementMode(
                paneCount: 1, activePaneIsPreview: false, otherPreviewPaneCount: 1),
            "split_right",
            "paneCount <= 1 splits right unconditionally, ignoring the preview inputs")
    }

    // MARK: - winsNeedingReload: first bind always reloads

    func testFirstBindNeedsReload() {
        let current: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
        ]
        XCTAssertEqual(
            Set(PreviewSeam.winsNeedingReload(current: current, loaded: [:])),
            Set<Int64>([10]),
            "a preview win with no prior loaded entry must reload on first bind")
    }

    // MARK: - winsNeedingReload: source buffer changed at an EQUAL tick (THE regression)

    func testSourceBufnrChangedSameTickNeedsReload() {
        let loaded: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
        ]
        let current: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 2, sourceTick: 5)
        ]
        XCTAssertEqual(
            Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
            Set<Int64>([10]),
            "a reused preview pane switching sources at an equal changedtick must still reload")
    }

    // MARK: - winsNeedingReload: tick advanced for the same source buffer

    func testTickAdvancedSameBufnrNeedsReload() {
        let loaded: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
        ]
        let current: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 6)
        ]
        XCTAssertEqual(
            Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
            Set<Int64>([10]),
            "an edit to the same source (advanced tick) must reload the preview")
    }

    // MARK: - winsNeedingReload: identical (win, bufnr, tick) is a no-op

    func testIdenticalIsNoReload() {
        let loaded: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
        ]
        let current: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
        ]
        XCTAssertEqual(
            Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
            Set<Int64>(),
            "an unchanged (win, sourceBufnr, sourceTick) must NOT reload (avoid redundant render)")
    }

    // MARK: - winsNeedingReload: wins are judged independently

    func testMultipleWinsIndependent() {
        let loaded: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
            11: PreviewSeam.PreviewLoad(sourceBufnr: 2, sourceTick: 6),
        ]
        let current: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
            11: PreviewSeam.PreviewLoad(sourceBufnr: 2, sourceTick: 7),
        ]
        XCTAssertEqual(
            Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
            Set<Int64>([11]),
            "only the win whose load changed reloads; an unchanged win is left alone")
    }

    // MARK: - winsNeedingReload: two wins previewing the SAME source both reload

    func testTwoWinsSameSourceBothReload() {
        let loaded: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
            11: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
        ]
        let current: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 6),
            11: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 6),
        ]
        XCTAssertEqual(
            Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
            Set<Int64>([10, 11]),
            "two panes previewing the same source must EACH reload on a source edit (keyed by win)")
    }

    // MARK: - winsNeedingReload: a win present only in `loaded` is never returned

    func testDroppedWinNotReturned() {
        let loaded: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
            99: PreviewSeam.PreviewLoad(sourceBufnr: 2, sourceTick: 3),
        ]
        let current: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
        ]
        XCTAssertEqual(
            Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
            Set<Int64>(),
            "only `current` is iterated; a win dropped from `current` is never returned")
    }

    // MARK: - winsNeedingReload: an empty `current` reloads nothing

    func testEmptyCurrentIsNoReload() {
        let loaded: [Int64: PreviewSeam.PreviewLoad] = [
            10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
        ]
        XCTAssertEqual(
            Set(PreviewSeam.winsNeedingReload(current: [:], loaded: loaded)),
            Set<Int64>(),
            "no current previews means nothing to reload regardless of prior loads")
    }

    // MARK: - clickFocus: the message-handler name is the pane-focus channel

    func testClickFocusHandlerName() {
        XCTAssertEqual(
            PreviewSeam.clickFocusHandlerName, "wolfPaneFocus",
            "the click-to-focus message handler must be named `wolfPaneFocus`")
    }

    // MARK: - clickFocus: the injected JS is a capture-phase mousedown that posts to the handler

    func testClickFocusJSCaptureMousedownPostsToHandler() {
        let js = PreviewSeam.clickFocusJS()
        XCTAssertTrue(
            js.contains("mousedown"),
            "the click-to-focus JS must listen for `mousedown`: \(js)")
        XCTAssertTrue(
            js.contains("true"),
            "the listener must register in the CAPTURE phase (the `true` argument): \(js)")
        XCTAssertTrue(
            js.contains("window.webkit.messageHandlers.wolfPaneFocus"),
            "the JS must post to the `wolfPaneFocus` script message handler: \(js)")
    }
}
