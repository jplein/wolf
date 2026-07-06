//
//  EditorInsetTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.EditorInset`.
//
//  These exercise the uniform-20pt-margin arithmetic seam the coder adds in
//  `Sources/WolfKeyMap/EditorInset.swift` (plan §1, §6). Like `PaneGeometryTests`
//  and `StatusbarLayoutTests`, they import ONLY Foundation-level test machinery
//  (no AppKit / CoreGraphics, no live window) and stay in the headless
//  `WolfKeyMap` module. `PaneContainerView` / `MainWindow` are window-server-bound
//  AppKit glue and are compile + manual-smoke only; all headless coverage lands
//  here. They reuse the `eps` / `approx` + `assertRectEqual` style from
//  `PaneGeometryTests.swift`.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor, plan §1)
//
//    public struct EditorInset {
//        public static let padding: Double = 20   // single source of truth (points)
//        public let containerWidth, containerHeight, padding, stripHeight: Double
//        public let originX: Double          // = padding
//        public let originY: Double          // = padding
//        public let contentWidth: Double     // = max(0, containerWidth  - 2*padding)
//        public let contentHeight: Double    // = max(0, containerHeight - 2*padding)
//        public let paneAreaWidth: Double    // = contentWidth
//        public let paneAreaHeight: Double   // = max(0, contentHeight - stripHeight)
//        public let cmdlineStripRect: Rect   // VIEW coords: (originX, originY+paneAreaHeight, contentWidth, stripHeight)
//        public let backgroundRect: Rect     // = Rect(0, 0, containerWidth, containerHeight)
//        public init(containerWidth: Double, containerHeight: Double,
//                    padding: Double, stripHeight: Double)
//    }
//
//  `Rect` is the EXISTING value type from `PaneTree.swift` (`Rect(x:y:w:h:)`,
//  `Double` fields). Both `contentWidth/Height` and `paneAreaHeight` CLAMP to a
//  non-negative floor (max(0, ...)) so a container smaller than the padding (or a
//  strip taller than the content) never produces negative geometry. One EXACT
//  case pins the representative numbers; a second case pins the STRUCTURAL
//  invariants (additive relationships, origins == padding); the remaining cases
//  pin the clamp edges.
//

import XCTest

@testable import WolfKeyMap

final class EditorInsetTests: XCTestCase {

    /// Tolerance for comparing the `Double` rect / scalar components (ported from
    /// `PaneGeometryTests.swift`).
    private let eps: Double = 1e-9

    /// The padding the plan pins as the single source of truth (points).
    private let pinnedPadding: Double = 20.0

    // MARK: - Helpers (ported from PaneGeometryTests' `approx` / `assertRectEqual`)

    /// Returns whether two doubles are within `eps`.
    private func approx(_ a: Double, _ b: Double) -> Bool {
        abs(a - b) < eps
    }

    /// Asserts a `Rect` matches another `Rect` component-by-component within `eps`.
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
            "case \(desc): got \(got), want \(want)",
            file: file,
            line: line
        )
    }

    // MARK: - Exact case pinned to the representative numbers (plan §6)

    func testExactGeometryForRepresentativeContainer() {
        // container 840 x 640, padding 20, strip 16:
        //   originX = originY   = 20
        //   contentWidth        = 840 - 2*20 = 800
        //   contentHeight       = 640 - 2*20 = 600
        //   paneAreaWidth       = 800
        //   paneAreaHeight      = 600 - 16   = 584
        //   cmdlineStripRect    = Rect(20, 20 + 584, 800, 16) = Rect(20, 604, 800, 16)
        //   backgroundRect      = Rect(0, 0, 840, 640)
        let inset = EditorInset(
            containerWidth: 840, containerHeight: 640, padding: 20, stripHeight: 16)

        XCTAssertEqual(inset.originX, 20.0, accuracy: eps, "originX == padding == 20")
        XCTAssertEqual(inset.originY, 20.0, accuracy: eps, "originY == padding == 20")
        XCTAssertEqual(inset.contentWidth, 800.0, accuracy: eps, "840 - 2*20 == 800")
        XCTAssertEqual(inset.contentHeight, 600.0, accuracy: eps, "640 - 2*20 == 600")
        XCTAssertEqual(inset.paneAreaWidth, 800.0, accuracy: eps, "paneAreaWidth == contentWidth == 800")
        XCTAssertEqual(inset.paneAreaHeight, 584.0, accuracy: eps, "600 - 16 == 584")
        assertRectEqual(
            inset.cmdlineStripRect,
            Rect(x: 20, y: 604, w: 800, h: 16),
            "cmdlineStripRect == Rect(20, 604, 800, 16)")
        assertRectEqual(
            inset.backgroundRect,
            Rect(x: 0, y: 0, w: 840, h: 640),
            "backgroundRect == Rect(0, 0, 840, 640)")
    }

    // MARK: - Padding constant pinned

    func testPaddingConstantIsTwenty() {
        XCTAssertEqual(EditorInset.padding, pinnedPadding, "EditorInset.padding is pinned at 20pt")
    }

    // MARK: - Structural invariants (independent of the exact numbers)

    func testStructuralInvariants() {
        // A second, different container (1000 x 700, padding 20, strip 18): assert the
        // additive relationships hold rather than re-deriving each field.
        let padding: Double = 20
        let stripHeight: Double = 18
        let containerWidth: Double = 1000
        let containerHeight: Double = 700
        let inset = EditorInset(
            containerWidth: containerWidth, containerHeight: containerHeight,
            padding: padding, stripHeight: stripHeight)

        XCTAssertEqual(
            inset.contentWidth + 2 * padding, containerWidth, accuracy: eps,
            "contentWidth + 2*padding == containerWidth")
        XCTAssertEqual(
            inset.contentHeight + 2 * padding, containerHeight, accuracy: eps,
            "contentHeight + 2*padding == containerHeight")
        XCTAssertEqual(
            inset.paneAreaHeight + stripHeight, inset.contentHeight, accuracy: eps,
            "paneAreaHeight + stripHeight == contentHeight")
        XCTAssertEqual(
            inset.paneAreaWidth, inset.contentWidth, accuracy: eps,
            "paneAreaWidth == contentWidth")
        XCTAssertEqual(
            inset.cmdlineStripRect.y, inset.originY + inset.paneAreaHeight, accuracy: eps,
            "cmdlineStripRect.y == originY + paneAreaHeight")
        XCTAssertEqual(
            inset.cmdlineStripRect.w, inset.contentWidth, accuracy: eps,
            "cmdlineStripRect.w == contentWidth")
        XCTAssertEqual(
            inset.cmdlineStripRect.h, stripHeight, accuracy: eps,
            "cmdlineStripRect.h == stripHeight")
        XCTAssertEqual(inset.originX, padding, accuracy: eps, "originX == padding")
        XCTAssertEqual(inset.originY, padding, accuracy: eps, "originY == padding")
    }

    // MARK: - Edge: container smaller than 2*padding clamps to zero (no negatives)

    func testContainerSmallerThanPaddingClampsToZero() {
        // container 30 x 30, padding 20: 30 - 2*20 == -10, clamped to 0 on every axis.
        let inset = EditorInset(
            containerWidth: 30, containerHeight: 30, padding: 20, stripHeight: 16)

        XCTAssertEqual(inset.contentWidth, 0.0, accuracy: eps, "contentWidth clamps to 0 (never negative)")
        XCTAssertEqual(inset.contentHeight, 0.0, accuracy: eps, "contentHeight clamps to 0 (never negative)")
        XCTAssertEqual(inset.paneAreaWidth, 0.0, accuracy: eps, "paneAreaWidth clamps to 0")
        XCTAssertEqual(inset.paneAreaHeight, 0.0, accuracy: eps, "paneAreaHeight clamps to 0")
        XCTAssertEqual(inset.cmdlineStripRect.w, 0.0, accuracy: eps, "cmdlineStripRect.w clamps to 0")
    }

    // MARK: - Edge: container exactly 2*padding yields zero content

    func testContainerExactlyTwicePaddingYieldsZeroContent() {
        // container 40 x 40, padding 20: 40 - 2*20 == 0 exactly (boundary, not negative).
        let inset = EditorInset(
            containerWidth: 40, containerHeight: 40, padding: 20, stripHeight: 16)

        XCTAssertEqual(inset.contentWidth, 0.0, accuracy: eps, "40 - 2*20 == 0 content width")
        XCTAssertEqual(inset.contentHeight, 0.0, accuracy: eps, "40 - 2*20 == 0 content height")
    }

    // MARK: - Edge: strip taller than content height clamps paneAreaHeight to zero

    func testStripTallerThanContentClampsPaneAreaHeight() {
        // container 48 x 44, padding 20: content height == 44 - 2*20 == 4; strip 16 is
        // taller, so paneAreaHeight = max(0, 4 - 16) clamps to 0 (never negative).
        let inset = EditorInset(
            containerWidth: 48, containerHeight: 44, padding: 20, stripHeight: 16)

        XCTAssertEqual(inset.contentHeight, 4.0, accuracy: eps, "44 - 2*20 == 4 content height")
        XCTAssertEqual(
            inset.paneAreaHeight, 0.0, accuracy: eps,
            "a strip taller than the content clamps paneAreaHeight to 0")
    }
}
