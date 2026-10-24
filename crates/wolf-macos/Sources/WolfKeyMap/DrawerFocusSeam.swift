// Pure choice of the editor pane `<C-w>k` from the terminal drawer focuses.

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
