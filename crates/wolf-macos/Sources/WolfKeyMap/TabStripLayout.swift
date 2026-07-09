//
//  TabStripLayout.swift
//  Pure, AppKit-/CoreText-free geometry + hit-testing for the per-pane TAB STRIP
//  (plan §4).
//
//  A headless seam in the `WolfKeyMap` module — Foundation-only, no live font, no
//  NSString drawing. It tiles a row of fixed-width tabs (clamped to the strip
//  width), places each tab's close (X) hit rect flush to its trailing edge, and
//  reserves a secondary (path) line beneath the tab row, plus a `hit(x:y:)`
//  dispatch the AppKit painter (`PaneContainerView`) uses to route a click to a
//  tab body or its close glyph. The on-screen painting itself is manual-smoke
//  only; only this arithmetic + hit dispatch is unit-tested (`TabStripLayoutTests`).
//
//  Reuses the module's `Rect` value type (from `PaneTree.swift`).
//

import Foundation

/// The geometry of a per-pane tab strip: the tiled tab rects, their per-tab close
/// (X) hit rects, the secondary (path) line beneath the tab row, the total strip
/// height, and a point → hit dispatch.
public struct TabStripLayout {
    /// A hit-tested target within the strip: a tab body or a tab's close glyph.
    public enum Hit: Equatable {
        /// The body of tab `i` (switch to that tab).
        case tab(Int)
        /// The close (X) glyph of tab `i` (close that tab).
        case close(Int)
    }

    /// The total strip height (`tabHeight + secondaryLineHeight`).
    public let stripHeight: Double
    /// One rect per tab, tiled left-to-right from `x = 0`, each clamped so it never
    /// spills past `stripWidth` (a fully-overflowed tab collapses to zero width).
    public let tabRects: [Rect]
    /// One close (X) hit rect per tab: a `closeSize`-square, vertically centered in
    /// the tab row, flush to the tab's trailing edge (no extra padding). A tab
    /// narrower than `closeSize` gets a zero-area rect (it has no room for an X and
    /// must not steal clicks from adjacent tabs — see the initializer).
    public let closeRects: [Rect]
    /// The secondary (path) line rect, spanning the full strip width directly
    /// beneath the tab row.
    public let secondaryLineRect: Rect

    /// Lays out `tabCount` tabs of `tabWidth`/`tabHeight` across a `stripWidth`
    /// strip, with a `closeSize`-square close glyph per tab and a
    /// `secondaryLineHeight` path line beneath the tab row.
    public init(
        stripWidth: Double, tabCount: Int, tabWidth: Double, tabHeight: Double,
        closeSize: Double, secondaryLineHeight: Double
    ) {
        self.stripHeight = tabHeight + secondaryLineHeight

        var tabs: [Rect] = []
        var closes: [Rect] = []
        tabs.reserveCapacity(tabCount)
        closes.reserveCapacity(tabCount)
        let closeY = (tabHeight - closeSize) / 2
        for i in 0..<max(0, tabCount) {
            let x = Double(i) * tabWidth
            // Clamp the tab width to whatever strip remains from its left edge.
            let w = min(tabWidth, max(0, stripWidth - x))
            tabs.append(Rect(x: x, y: 0, w: w, h: tabHeight))
            // The close square sits flush to the tab's trailing edge, centered. A
            // tab narrower than `closeSize` (a fully- or nearly-overflowed tab) has
            // no room for an X: emitting one at `x + w - closeSize` would place it
            // BEFORE the tab origin and, since close rects are hit-tested before tab
            // bodies, could steal a click from an adjacent visible tab (NIT #3). Such
            // tabs get a zero-area close rect that never paints and never hits.
            if w < closeSize {
                closes.append(Rect(x: x, y: closeY, w: 0, h: 0))
            } else {
                closes.append(
                    Rect(x: x + w - closeSize, y: closeY, w: closeSize, h: closeSize))
            }
        }
        self.tabRects = tabs
        self.closeRects = closes
        self.secondaryLineRect = Rect(
            x: 0, y: tabHeight, w: stripWidth, h: secondaryLineHeight)
    }

    /// Dispatches the pane-local point `(x, y)`: the FIRST tab whose close rect
    /// contains it → `.close(i)` (checked before the body, since the X sits inside
    /// the tab body); else the first tab whose rect contains it → `.tab(i)`; else
    /// `nil` (the secondary line + empty area are not hit targets).
    public func hit(x: Double, y: Double) -> Hit? {
        for (i, rect) in closeRects.enumerated() where TabStripLayout.contains(rect, x, y) {
            return .close(i)
        }
        for (i, rect) in tabRects.enumerated() where TabStripLayout.contains(rect, x, y) {
            return .tab(i)
        }
        return nil
    }

    /// Whether `rect` contains the point `(x, y)` (half-open on the trailing/bottom
    /// edges, like AppKit's `NSRect.contains`).
    private static func contains(_ rect: Rect, _ x: Double, _ y: Double) -> Bool {
        x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h
    }
}
