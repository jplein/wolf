//
//  TabStripLayoutTests.swift
//  Pure, headless unit tests for the AppKit-/CoreText-free `WolfKeyMap.TabStripLayout`.
//
//  These exercise the per-pane TAB-STRIP geometry + hit-testing seam the coder adds
//  in `Sources/WolfKeyMap/TabStripLayout.swift` (plan §4). Like `StatusbarLayoutTests`
//  and `PaneGeometryTests`, they import ONLY Foundation-level test machinery (no
//  AppKit / CoreText, no live font) and stay in the headless `WolfKeyMap` module,
//  reusing the existing `Rect` value type (from `PaneTree.swift`) and the
//  `eps`/`approx`/`assertRectEqual` style. The on-screen painting (bg fills, NSString
//  drawing, hover chrome) is AppKit glue and is manual-smoke only; only the pure
//  rect arithmetic + hit dispatch is asserted.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor — plan §4)
//
//  NOTE TO CODER: plan §4 names the seam's responsibilities (stripHeight, tabRects,
//  closeRects, secondaryLineRect, `hit(point) -> .tab(i)|.close(i)|nil`) but leaves
//  the exact init signature for you to finalize. These tests pin the following
//  concrete contract; if you deviate, reconcile the impl or these tests (see the
//  test-writer report):
//
//    public struct TabStripLayout {
//        public enum Hit: Equatable { case tab(Int); case close(Int) }
//        public let stripHeight: Double
//        public let tabRects: [Rect]          // one per tab, tiled L->R from x = 0
//        public let closeRects: [Rect]        // one per tab: the X (close) hit rect
//        public let secondaryLineRect: Rect   // the path line beneath the tab row
//        public init(stripWidth: Double, tabCount: Int, tabWidth: Double,
//                    tabHeight: Double, closeSize: Double, secondaryLineHeight: Double)
//        public func hit(x: Double, y: Double) -> Hit?
//    }
//
//  Arithmetic (pinned):
//    stripHeight       = tabHeight + secondaryLineHeight
//    tabRects[i]       = Rect(x: i*tabWidth, y: 0,
//                             w: min(tabWidth, max(0, stripWidth - i*tabWidth)),
//                             h: tabHeight)                    // CLAMPED to stripWidth
//    closeRects[i]     = a `closeSize`-square, vertically centered in the tab row,
//                        flush to the tab's trailing edge:
//                        Rect(x: tabRects[i].maxX - closeSize,
//                             y: (tabHeight - closeSize)/2, w: closeSize, h: closeSize)
//    secondaryLineRect = Rect(x: 0, y: tabHeight, w: stripWidth, h: secondaryLineHeight)
//    hit(x,y)          = the FIRST i whose closeRect contains the point -> .close(i);
//                        else the first i whose tabRect contains it -> .tab(i);
//                        else nil (the secondary line + empty area are NOT hit targets)
//
//  `hit(x:y:)` uses Double args (like `PaneGeometry.cellAt(x:y:)`) rather than the
//  plan's `hit(point)` wording, to stay in the AppKit-/CoreGraphics-free seam; the
//  caller passes the pane-local point components. The close rect is placed flush to
//  the trailing edge with no extra padding param — adjust if you add one.
//

import XCTest

@testable import WolfKeyMap

final class TabStripLayoutTests: XCTestCase {

    // MARK: - Representative dimensions

    /// Per-tab width (px).
    private let tabWidth: Double = 120.0
    /// Tab-row height (px).
    private let tabHeight: Double = 24.0
    /// Close-glyph (X) hit-square side (px).
    private let closeSize: Double = 12.0
    /// Secondary (path) line height (px).
    private let secondaryLineHeight: Double = 14.0
    /// A strip wide enough that three 120px tabs are never clamped.
    private let wideWidth: Double = 1000.0

    /// Tolerance for the Double rect comparisons (ported from `PaneGeometryTests`).
    private let eps: Double = 1e-9

    // MARK: - Helpers

    /// Whether two doubles are within `eps`.
    private func approx(_ a: Double, _ b: Double) -> Bool {
        abs(a - b) < eps
    }

    /// Asserts a `Rect` matches another component-by-component within `eps`.
    private func assertRectEqual(
        _ got: Rect,
        _ want: Rect,
        _ desc: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        XCTAssertTrue(
            approx(got.x, want.x) && approx(got.y, want.y)
                && approx(got.w, want.w) && approx(got.h, want.h),
            "\(desc): got \(got), want \(want)",
            file: file,
            line: line
        )
    }

    /// Builds a wide-strip layout with `count` tabs (never clamped).
    private func wideLayout(count: Int) -> TabStripLayout {
        TabStripLayout(
            stripWidth: wideWidth, tabCount: count, tabWidth: tabWidth,
            tabHeight: tabHeight, closeSize: closeSize,
            secondaryLineHeight: secondaryLineHeight)
    }

    // MARK: - Strip height

    func testStripHeightIsTabRowPlusSecondaryLine() {
        let l = wideLayout(count: 3)
        XCTAssertEqual(
            l.stripHeight, tabHeight + secondaryLineHeight, accuracy: eps,
            "stripHeight == tabHeight + secondaryLineHeight (24 + 14 == 38)")
    }

    // MARK: - Tabs tile left-to-right

    func testTabRectsTileLeftToRight() {
        let l = wideLayout(count: 3)
        XCTAssertEqual(l.tabRects.count, 3, "one rect per tab")
        for i in 0..<3 {
            assertRectEqual(
                l.tabRects[i],
                Rect(x: Double(i) * tabWidth, y: 0, w: tabWidth, h: tabHeight),
                "tab \(i) is tiled at x = \(i) * tabWidth, full width, at the top")
        }
    }

    // MARK: - Clamping to the strip width

    func testLastTabIsClampedToStripWidth() {
        // stripWidth 250, three 120px tabs: tab2 starts at x = 240 and is clamped
        // to width 10 (250 - 240) so it never spills past the strip.
        let l = TabStripLayout(
            stripWidth: 250, tabCount: 3, tabWidth: tabWidth,
            tabHeight: tabHeight, closeSize: closeSize,
            secondaryLineHeight: secondaryLineHeight)
        assertRectEqual(l.tabRects[0], Rect(x: 0, y: 0, w: 120, h: tabHeight), "tab 0 full width")
        assertRectEqual(l.tabRects[1], Rect(x: 120, y: 0, w: 120, h: tabHeight), "tab 1 full width")
        assertRectEqual(
            l.tabRects[2], Rect(x: 240, y: 0, w: 10, h: tabHeight),
            "tab 2 is clamped to the remaining 10px of strip width")
    }

    func testFullyOverflowedTabHasZeroWidth() {
        // stripWidth 200, three 120px tabs: tab1 clamps to 80px and tab2 starts at
        // x = 240 (past the strip) so it collapses to zero width.
        let l = TabStripLayout(
            stripWidth: 200, tabCount: 3, tabWidth: tabWidth,
            tabHeight: tabHeight, closeSize: closeSize,
            secondaryLineHeight: secondaryLineHeight)
        assertRectEqual(l.tabRects[1], Rect(x: 120, y: 0, w: 80, h: tabHeight), "tab 1 clamped to 80")
        XCTAssertEqual(
            l.tabRects[2].w, 0.0, accuracy: eps,
            "a tab starting past the strip width collapses to zero width")
    }

    // MARK: - Per-tab close (X) rects

    func testCloseRectsSitFlushTrailingAndCentered() {
        let l = wideLayout(count: 3)
        XCTAssertEqual(l.closeRects.count, 3, "one close rect per tab")
        let expectedY = (tabHeight - closeSize) / 2  // (24 - 12) / 2 == 6
        for i in 0..<3 {
            let tab = l.tabRects[i]
            assertRectEqual(
                l.closeRects[i],
                Rect(x: tab.x + tab.w - closeSize, y: expectedY, w: closeSize, h: closeSize),
                "close \(i) is a \(closeSize)px square flush to the tab's trailing edge, centered")
        }
    }

    // MARK: - Secondary (path) line rect

    func testSecondaryLineRectSpansBelowTabRow() {
        let l = wideLayout(count: 3)
        assertRectEqual(
            l.secondaryLineRect,
            Rect(x: 0, y: tabHeight, w: wideWidth, h: secondaryLineHeight),
            "the secondary path line spans the full width, directly beneath the tab row")
    }

    // MARK: - hit() dispatch at boundary points

    func testHitInTabBodyReturnsTab() {
        // A point in the LEFT area of each tab (well clear of the trailing X) hits
        // that tab's body.
        let l = wideLayout(count: 3)
        XCTAssertEqual(l.hit(x: 10, y: 10), .tab(0), "left of tab 0 -> .tab(0)")
        XCTAssertEqual(l.hit(x: 130, y: 10), .tab(1), "left of tab 1 -> .tab(1)")
        XCTAssertEqual(l.hit(x: 250, y: 10), .tab(2), "left of tab 2 -> .tab(2)")
    }

    func testHitInCloseRectReturnsClose() {
        // A point inside a tab's trailing X square hits .close(i) — checked BEFORE
        // .tab even though the X sits inside the tab body.
        let l = wideLayout(count: 3)
        // tab 0 close rect: x in [108, 120], y in [6, 18]. Center ~ (114, 12).
        XCTAssertEqual(l.hit(x: 114, y: 12), .close(0), "inside tab 0's X -> .close(0)")
        // tab 1 close rect: x in [228, 240].
        XCTAssertEqual(l.hit(x: 234, y: 12), .close(1), "inside tab 1's X -> .close(1)")
    }

    func testHitOutsideAllTabsReturnsNil() {
        let l = wideLayout(count: 3)
        // In the secondary line region (y within [24, 38)) — not a hit target.
        XCTAssertNil(l.hit(x: 10, y: 30), "a point in the secondary line row -> nil")
        // Below the whole strip.
        XCTAssertNil(l.hit(x: 10, y: 100), "a point below the strip -> nil")
        // To the right of the last tab (three tabs end at x = 360).
        XCTAssertNil(l.hit(x: 500, y: 10), "a point past the last tab -> nil")
        // Above the strip (negative y).
        XCTAssertNil(l.hit(x: 10, y: -5), "a point above the strip -> nil")
    }

    // MARK: - Empty strip (no tabs)

    func testEmptyStripHasNoTabsButStillHasHeightAndSecondaryLine() {
        let l = wideLayout(count: 0)
        XCTAssertTrue(l.tabRects.isEmpty, "no tabs -> no tab rects")
        XCTAssertTrue(l.closeRects.isEmpty, "no tabs -> no close rects")
        assertRectEqual(
            l.secondaryLineRect,
            Rect(x: 0, y: tabHeight, w: wideWidth, h: secondaryLineHeight),
            "the secondary line still spans the strip with no tabs")
        XCTAssertNil(l.hit(x: 10, y: 10), "any hit on an empty strip -> nil")
    }
}
