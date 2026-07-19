//
//  DrawerLayout.swift
//  Pure, AppKit-/CoreGraphics-free geometry for the terminal DRAWER (terminal-drawer
//  FIX plan §4).
//
//  This is a headless seam in the `WolfKeyMap` module — Foundation-only, no live
//  view, no `CGFloat`. The drawer is a full-width, root-bottom strip at a fixed
//  ~1/3 weight held OUTSIDE the pure `PaneTree` (so the tree keeps its 1:1 parity
//  with wolf-gnome). Given the pane-area height and whether a drawer is present, it
//  splits that height into the editor region (which the tree tiles over) and the
//  drawer strip below it, and vends the full-width drawer rectangle.
//
//  Like `EditorInset` / `PaneGeometry`, it reuses the existing `Rect` value type
//  from `PaneTree.swift` and is `Double`-based so it stays headlessly testable (see
//  `DrawerLayoutTests.swift`). `PaneContainerView` wires it into `paneRects()`; that
//  AppKit glue is compile + manual-smoke only.
//

import Foundation

/// The terminal-drawer geometry: how the pane area splits between the editor tree
/// (above) and the full-width drawer strip (below).
///
/// The editor tree tiles over `[0, editorHeight]`; the drawer occupies
/// `[editorHeight, paneAreaHeight]`. Both heights clamp to a non-negative floor
/// (`max(0, ...)`), so a pane area shorter than the rounded drawer strip never
/// yields a negative editor height. When `hasDrawer` is false the editor fills the
/// whole pane area and `drawerRect(width:)` is `nil`.
public struct DrawerLayout {
  /// The fixed drawer weight (fraction of the pane-area height): ~1/3, matching
  /// the `wolf_toggle_terminal` Lua helper's `botright <lines/3>split` (FD3: no
  /// divider-drag resize this pass).
  public static let defaultWeight: Double = 1.0 / 3.0

  /// The editor region's height, in points: `hasDrawer ? max(0, paneAreaHeight -
  /// drawerHeight) : paneAreaHeight`. The `PaneTree` tiles over this height.
  public let editorHeight: Double
  /// The drawer strip's height, in points: `hasDrawer ?
  /// (paneAreaHeight * weight).rounded() : 0`.
  public let drawerHeight: Double

  /// The pane-area height, in points (echoed input).
  private let paneAreaHeight: Double
  /// Whether a drawer is present (echoed input).
  private let hasDrawer: Bool

  /// Splits `paneAreaHeight` into the editor region and the drawer strip. When
  /// `hasDrawer` is false the drawer height is 0 and the editor fills the whole
  /// area; otherwise the drawer takes `round(paneAreaHeight * weight)` and the
  /// editor takes the (non-negative) remainder.
  public init(paneAreaHeight: Double, hasDrawer: Bool, weight: Double) {
    self.paneAreaHeight = paneAreaHeight
    self.hasDrawer = hasDrawer
    self.drawerHeight = hasDrawer ? (paneAreaHeight * weight).rounded() : 0
    self.editorHeight = hasDrawer ? max(0, paneAreaHeight - drawerHeight) : paneAreaHeight
  }

  /// The full-width drawer rectangle: a bottom strip at `y == editorHeight`,
  /// `x == 0`, `w == width`, `h == drawerHeight`. `nil` when no drawer is present.
  public func drawerRect(width: Double) -> Rect? {
    guard hasDrawer else {
      return nil
    }
    return Rect(x: 0, y: editorHeight, w: width, h: drawerHeight)
  }
}
