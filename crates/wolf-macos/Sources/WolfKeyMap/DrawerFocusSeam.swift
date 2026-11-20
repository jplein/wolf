// Pure resolution of a focus move's target pane, bridging the tree and the terminal drawer.

import Foundation

/// Resolves the pane a `<C-w>` focus move reaches, across the tree and the drawer, which
/// lives outside the tree, and the editor pane `<C-w>k` (drawer -> up) should focus.
public enum DrawerFocusSeam {
  /// The pane a focus move `dir` from `from` reaches, or `nil` when it has none.
  ///
  /// - From the `drawer`: only `.up` resolves, to `drawerUpTarget` over the tree's
  ///   rectangles; the drawer is full-width, so the other directions have no target.
  /// - From a tree pane: its `tree.neighbor`, else the open `drawer` for `.down`, the
  ///   bottom-edge bridge.
  ///
  /// `width` and `editorHeight` must be those the view lays its tree out in, the
  /// drawer-reduced editor height, so the neighbour matches what the user sees.
  public static func focusTarget(
    from: PaneId, dir: FocusDir, tree: PaneTree, drawer: PaneId?, lastEditorPane: PaneId?,
    width: Double, editorHeight: Double
  ) -> PaneId? {
    if from == drawer {
      guard dir == .up else { return nil }
      return drawerUpTarget(
        lastEditorPane: lastEditorPane, leaves: tree.leaves(),
        rects: tree.paneRects(width: width, height: editorHeight))
    }
    if let neighbor = tree.neighbor(from, dir, width: width, height: editorHeight) {
      return neighbor
    }
    return dir == .down ? drawer : nil
  }

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
