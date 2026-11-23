// Pure Zoom Pane rules: which pane is zoomed, what unzooms or re-zooms, and what a zoom freezes.

import Foundation

/// The rules of a view's pane zoom, so `PaneContainerView` holds only the zoomed
/// pane's id. A namespace (`enum` with only static members).
///
/// Zoom is front-end geometry: nothing here reaches Neovim, the pane tree or its
/// weights, which is what lets an unzoom restore the layout exactly. Only the saved window
/// state records it, as the tab's zoomed pane.
public enum PaneZoom {
  /// What happened to the view, for `zoomed(after:zoomed:active:panes:)`.
  public enum Event: Equatable {
    /// Zoom Pane: zoom the active pane, or unzoom.
    case toggle
    /// The view applies a split, from any source; it unzooms first, so the new pane shows.
    case split
    /// The view opens the drawer; a pane joins the layout, as with a split.
    case drawerOpen
    /// The view's panes changed, as after a close.
    case panesChanged
    /// A focus move resolved, in the unzoomed layout, to `target`, or to `nil` when it
    /// has none; Neovim's current window resolving to a pane is one too. A move to
    /// another pane unzooms, so that pane shows; one with no target keeps the zoom.
    case focusMove(target: PaneId?)
    /// Toggle Terminal, with the drawer's id when it is open. It unzooms, so the drawer
    /// toggles as it would unzoomed, unless the drawer is the zoomed pane: that zoom
    /// lasts until `.panesChanged` sees the drawer close, so no hidden pane flashes first.
    case toggleTerminal(drawer: PaneId?)
    /// A launch restore rebuilt the pane `PaneId` from a leaf saved as zoomed. It zooms
    /// that pane when it is among `panes` and they number more than one.
    case restore(PaneId)
  }

  /// The zoomed pane after `event`, or `nil` for an unzoomed view.
  ///
  /// `panes` is every pane of the view, the tree's leaves plus the drawer when it
  /// is open, so the drawer can be zoomed and one pane with no drawer, which has
  /// nothing to hide, never is. Only the zoomed pane leaving `panes` unzooms on
  /// `.panesChanged`; a hidden pane closing keeps the zoom.
  public static func zoomed(
    after event: Event, zoomed: PaneId?, active: PaneId?, panes: [PaneId]
  ) -> PaneId? {
    switch event {
    case .toggle:
      if zoomed != nil { return nil }
      guard let active, panes.contains(active), panes.count > 1 else { return nil }
      return active
    case .split, .drawerOpen:
      return nil
    case .panesChanged:
      guard let zoomed, panes.contains(zoomed) else { return nil }
      return zoomed
    case .focusMove(let target):
      guard let zoomed, target == nil || target == zoomed else { return nil }
      return zoomed
    case .toggleTerminal(let drawer):
      guard let zoomed, let drawer, zoomed == drawer else { return nil }
      return zoomed
    case .restore(let pane):
      guard panes.contains(pane), panes.count > 1 else { return nil }
      return pane
    }
  }

  /// The pane a focus move starts from: the zoomed pane while zoomed, whatever pane
  /// Neovim's window resolved to, else `from`. The move is resolved in the unzoomed
  /// layout, as if made unzoomed from the pane the user sees.
  public static func focusOrigin(zoomed: PaneId?, from: PaneId?) -> PaneId? {
    zoomed ?? from
  }

  /// Whether a zoom freezes the tree dividers and the drawer's edge, which are then
  /// neither hit-tested nor given a cursor, since they are hidden. Focus moves are not
  /// frozen: `.focusMove` unzooms on them.
  public static func freezesLayout(zoomed: PaneId?) -> Bool {
    zoomed != nil
  }

  /// The other-pane commands' target while zoomed: the zoomed pane is the only
  /// pane, so a tree pane takes the split path, which unzooms, and the drawer
  /// gets `nil`, as it does unzoomed. Read only while zoomed.
  public static func otherPaneTarget(zoomed: PaneId, drawer: PaneId?) -> OtherPaneSeam.Target? {
    zoomed == drawer ? nil : .split
  }

  /// The panes a view shows and where: the zoomed pane alone over `bounds`, or
  /// `rects` unchanged when unzoomed. A zoomed id absent from `rects` shows every
  /// pane, so a missed unzoom never blanks the view.
  public static func visibleRects(
    zoomed: PaneId?, rects: [(PaneId, Rect)], bounds: Rect
  ) -> [(PaneId, Rect)] {
    guard let zoomed, rects.contains(where: { $0.0 == zoomed }) else { return rects }
    return [(zoomed, bounds)]
  }
}
