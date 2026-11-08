// Pure geometry splitting the pane area into the editor region and the terminal drawer.

import Foundation

/// The terminal-drawer geometry: how the pane area splits between the editor tree
/// (above) and the full-width drawer strip (below).
///
/// The editor tree tiles over `[0, editorHeight]`; the drawer occupies
/// `[editorHeight, editorHeight + drawerHeight]`. With a drawer, the two heights
/// sum to the pane-area height and neither is negative. When `hasDrawer` is false
/// the editor fills the whole pane area and `drawerRect(width:)` is `nil`.
///
/// A non-finite `paneAreaHeight`, `weight` or `minDrawerHeight`, or a non-positive
/// `paneAreaHeight`, collapses both heights to zero, with or without a drawer, as
/// wolf-gnome's `drawer_split` does.
public struct DrawerLayout {
  /// The fixed drawer weight (fraction of the pane-area height): ~1/3, matching
  /// the `wolf_toggle_terminal` Lua helper's `botright <lines/3>split`. The drawer
  /// has no divider-drag resize.
  public static let defaultWeight: Double = 1.0 / 3.0

  /// The editor region's height, in points: `hasDrawer ? paneAreaHeight -
  /// drawerHeight : paneAreaHeight`, or 0 when the input collapses. The `PaneTree`
  /// tiles over this height.
  public let editorHeight: Double
  /// The drawer strip's height, in points: with a drawer,
  /// `round(paneAreaHeight * weight)` floored at `max(minDrawerHeight, 0)` and then
  /// capped at `paneAreaHeight`; otherwise 0.
  public let drawerHeight: Double

  /// Whether a drawer is present (echoed input).
  private let hasDrawer: Bool

  /// Splits `paneAreaHeight` into the editor region and the drawer strip. When
  /// `hasDrawer` is false the drawer height is 0 and the editor fills the whole
  /// area; otherwise the drawer takes `round(paneAreaHeight * weight)`, floored at
  /// `minDrawerHeight` and capped at the pane area, and the editor takes the rest.
  ///
  /// The cap applies after the floor, so a floor above the pane area gives the
  /// drawer the whole area rather than a negative editor height.
  public init(paneAreaHeight: Double, hasDrawer: Bool, weight: Double, minDrawerHeight: Double) {
    self.hasDrawer = hasDrawer
    // Explicit checks: Swift's min/max keep or drop a NaN by argument order.
    guard paneAreaHeight.isFinite, weight.isFinite, minDrawerHeight.isFinite, paneAreaHeight > 0
    else {
      self.drawerHeight = 0
      self.editorHeight = 0
      return
    }
    guard hasDrawer else {
      self.drawerHeight = 0
      self.editorHeight = paneAreaHeight
      return
    }
    let floored = max((paneAreaHeight * weight).rounded(), max(minDrawerHeight, 0))
    self.drawerHeight = min(floored, paneAreaHeight)
    self.editorHeight = paneAreaHeight - drawerHeight
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
