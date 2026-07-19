//
//  DrawerFocusSeam.swift
//  Pure, AppKit-/CoreGraphics-free focus-target selection for editor<->drawer
//  navigation (terminal-drawer navigation addendum §1).
//
//  The terminal drawer stays OUT of the pure `PaneTree` (preserving its 1:1 parity
//  with wolf-gnome), so `<C-w>k` from the drawer cannot resolve its target via
//  `PaneTree.neighbor`. Instead the front-end bridges into the editor by picking a
//  concrete editor leaf. This seam is that pure computation over the editor's leaf
//  rects; the `applyFocus` AppKit glue that calls it is compile + manual-smoke only.
//
//  Like `PaneTree` / `DrawerLayout`, it reuses the existing `Rect` / `PaneId` types
//  from `PaneTree.swift` and is `Double`-based so it stays headlessly testable (see
//  `DrawerFocusSeamTests.swift`).
//

import Foundation

/// Selects the editor pane `<C-w>k` (drawer -> up) should focus.
public enum DrawerFocusSeam {
  /// The bottom-most editor leaf: the one with the greatest `y + h` (top-left
  /// origin, so a larger `y` is lower on screen). Ties on `y + h` break to the
  /// smallest `x` (left-most), then to the smallest `PaneId`. `nil` when `rects`
  /// is empty.
  public static func bottomMostLeaf(_ rects: [(PaneId, Rect)]) -> PaneId? {
    var best: (id: PaneId, bottom: Double, x: Double)?
    for (id, r) in rects {
      let bottom = r.y + r.h
      guard let current = best else {
        best = (id, bottom, r.x)
        continue
      }
      if bottom > current.bottom
        || (bottom == current.bottom && r.x < current.x)
        || (bottom == current.bottom && r.x == current.x && id < current.id)
      {
        best = (id, bottom, r.x)
      }
    }
    return best?.id
  }

  /// The `<C-w>k` target from the drawer: the most-recently-used editor pane
  /// (`lastEditorPane`) when it is non-nil AND still a live leaf; otherwise the
  /// `bottomMostLeaf(rects)` fallback (the pane adjacent to the drawer).
  public static func drawerUpTarget(
    lastEditorPane: PaneId?, leaves: [PaneId], rects: [(PaneId, Rect)]
  ) -> PaneId? {
    if let mru = lastEditorPane, leaves.contains(mru) {
      return mru
    }
    return bottomMostLeaf(rects)
  }
}
