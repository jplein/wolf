// Pure Zoom Pane rules: which pane is zoomed, what unzooms, and what a zoom freezes.

import Foundation

/// The rules of a view's pane zoom, so `PaneContainerView` holds only the zoomed
/// pane's id. A namespace (`enum` with only static members).
///
/// Zoom is front-end geometry: nothing here reaches Neovim, the pane tree or its
/// weights, which is what lets an unzoom restore the layout exactly.
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
    }
  }

  /// Whether a zoom freezes the layout: focus moves do nothing, and the tree
  /// dividers and the drawer's edge are neither hit-tested nor given a cursor.
  /// The user chose this over unzooming, since no other pane is visible.
  public static func freezesLayout(zoomed: PaneId?) -> Bool {
    zoomed != nil
  }

  /// Whether Toggle Terminal acts: always unzoomed, and while zoomed only when the
  /// drawer is the zoomed pane, where closing it is closing the zoomed pane. The
  /// user chose this over unzooming on a drawer toggle.
  public static func allowsToggleTerminal(zoomed: PaneId?, drawer: PaneId?) -> Bool {
    zoomed == nil || zoomed == drawer
  }

  /// The other-pane commands' target while zoomed: the zoomed pane is the only
  /// pane, so a tree pane takes the split path, which unzooms, and the drawer
  /// gets `nil`, as it does unzoomed. Read only while zoomed.
  public static func otherPaneTarget(zoomed: PaneId, drawer: PaneId?) -> OtherPaneSeam.Target? {
    zoomed == drawer ? nil : .split
  }

  /// Whether Neovim's current window, resolved to `resolved`, must be sent back
  /// to the zoomed pane: it left by a path the view does not intercept
  /// (`<C-w>w`, `:wincmd`), and the user would otherwise type into a hidden pane.
  public static func refocuses(resolved: PaneId?, zoomed: PaneId?) -> Bool {
    zoomed != nil && resolved != nil && resolved != zoomed
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
