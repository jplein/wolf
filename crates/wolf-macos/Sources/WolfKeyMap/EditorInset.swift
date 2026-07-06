//
//  EditorInset.swift
//  Pure, AppKit-/CoreGraphics-free arithmetic for the uniform editor margin
//  (plan §1).
//
//  This is a headless seam in the `WolfKeyMap` module — Foundation-only, no live
//  view, no `CGFloat`. It insets the editor content region (panes + per-pane
//  statusbar strips + the bottom cmdline strip) by a uniform `padding` on all four
//  sides of the content view, so the AppKit glue (`PaneContainerView`) can offset
//  every pane rect and place the cmdline strip deterministically, and `MainWindow`
//  can grow its initial content by `2 * padding` while keeping the editor grid at
//  exactly `cols × rows`. The margin is painted in the editor background by the
//  view filling its whole `bounds`; only this arithmetic is unit-tested.
//
//  It reuses the existing `Rect` value type from `PaneTree.swift` and, like
//  `PaneGeometry` / `StatusbarLayout`, is `Double`-based so it stays headlessly
//  testable (see `EditorInsetTests.swift`).
//

import Foundation

/// The layout of the inset editor content region: the padded origin, the content
/// and pane-area dimensions, and the cmdline-strip + background rectangles.
///
/// `contentWidth`/`contentHeight` and `paneAreaHeight` all CLAMP to a non-negative
/// floor (`max(0, ...)`), so a container smaller than `2 * padding` (or a strip
/// taller than the content) never yields negative geometry. All rectangles are in
/// VIEW coordinates (origin offset by `padding`), except `backgroundRect`, which
/// spans the whole container so the margins paint in the editor background.
public struct EditorInset {
    /// The uniform margin, in points, applied to all four sides of the editor
    /// content region — the single source of truth, consumed by both
    /// `PaneContainerView` and `MainWindow`.
    public static let padding: Double = 20

    /// The container (content-view) width, in points.
    public let containerWidth: Double
    /// The container (content-view) height, in points.
    public let containerHeight: Double
    /// The uniform margin applied on each side, in points (echoed input).
    public let padding: Double
    /// The one-row cmdline-strip height, in points (echoed input).
    public let stripHeight: Double

    /// The left edge of the content region (`== padding`).
    public let originX: Double
    /// The top edge of the content region (`== padding`).
    public let originY: Double
    /// The content region width: `max(0, containerWidth - 2 * padding)`.
    public let contentWidth: Double
    /// The content region height: `max(0, containerHeight - 2 * padding)`.
    public let contentHeight: Double
    /// The pane-area width (`== contentWidth`).
    public let paneAreaWidth: Double
    /// The pane-area height (the content minus the cmdline strip):
    /// `max(0, contentHeight - stripHeight)`.
    public let paneAreaHeight: Double
    /// The cmdline strip rectangle in VIEW coords, along the bottom of the content
    /// region: `Rect(originX, originY + paneAreaHeight, contentWidth, stripHeight)`.
    public let cmdlineStripRect: Rect
    /// The whole container rectangle: `Rect(0, 0, containerWidth, containerHeight)`
    /// — the view fills this with the editor background so the margins are painted.
    public let backgroundRect: Rect

    /// Computes the inset layout from the container size, the uniform `padding`, and
    /// the cmdline `stripHeight`. All derived dimensions clamp to a non-negative
    /// floor so a too-small container never produces negative geometry.
    public init(
        containerWidth: Double, containerHeight: Double, padding: Double, stripHeight: Double
    ) {
        self.containerWidth = containerWidth
        self.containerHeight = containerHeight
        self.padding = padding
        self.stripHeight = stripHeight

        self.originX = padding
        self.originY = padding
        self.contentWidth = max(0, containerWidth - 2 * padding)
        self.contentHeight = max(0, containerHeight - 2 * padding)
        self.paneAreaWidth = self.contentWidth
        self.paneAreaHeight = max(0, self.contentHeight - stripHeight)
        self.cmdlineStripRect = Rect(
            x: self.originX, y: self.originY + self.paneAreaHeight,
            w: self.contentWidth, h: stripHeight)
        self.backgroundRect = Rect(x: 0, y: 0, w: containerWidth, h: containerHeight)
    }
}
