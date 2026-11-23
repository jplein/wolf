// Pure geometry splitting the pane area into the editor region and the terminal drawer.

import Foundation

/// The terminal-drawer geometry: how the pane area splits between the editor tree
/// (above) and the full-width drawer strip (below), at a share the caller holds.
///
/// The editor tree tiles over `[0, editorHeight]`; the drawer occupies
/// `[editorHeight, editorHeight + drawerHeight]`. With a drawer, the two heights
/// sum to the pane-area height and neither is negative. When `hasDrawer` is false
/// the editor fills the whole pane area and `drawerRect(width:)` is `nil`.
///
/// A non-finite `paneAreaHeight`, `weight` or `minPaneHeight`, or a non-positive
/// `paneAreaHeight`, collapses both heights to zero, with or without a drawer.
public struct DrawerLayout {
  /// The share (fraction of the pane-area height) a drawer opens at: ~1/3, matching
  /// the `wolf_toggle_terminal` Lua helper's `botright <lines/3>split`. A drag on the
  /// drawer's top edge replaces it, through `draggedWeight`.
  public static let defaultWeight: Double = 1.0 / 3.0

  /// The editor region's height, in points: `hasDrawer ? paneAreaHeight -
  /// drawerHeight : paneAreaHeight`, or 0 when the input collapses. The `PaneTree`
  /// tiles over this height.
  public let editorHeight: Double
  /// The drawer strip's height, in points: with a drawer,
  /// `round(paneAreaHeight * weight)` held between the floor `max(minPaneHeight, 0)`
  /// and a ceiling that leaves the editor that floor; otherwise 0.
  public let drawerHeight: Double

  /// Whether a drawer is present (echoed input).
  private let hasDrawer: Bool

  /// Splits `paneAreaHeight` into the editor region and the drawer strip. When
  /// `hasDrawer` is false the drawer height is 0 and the editor fills the whole
  /// area; otherwise the drawer takes `round(paneAreaHeight * weight)`, and the
  /// editor the rest. One floor, `minPaneHeight`, holds both sides.
  ///
  /// An area too short for two floors keeps the drawer's floor, capped at the area,
  /// rather than a negative editor height.
  public init(paneAreaHeight: Double, hasDrawer: Bool, weight: Double, minPaneHeight: Double) {
    self.hasDrawer = hasDrawer
    // Explicit checks: Swift's min/max keep or drop a NaN by argument order.
    guard paneAreaHeight.isFinite, weight.isFinite, minPaneHeight.isFinite, paneAreaHeight > 0
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
    let floor = max(minPaneHeight, 0)
    let ceiling = paneAreaHeight >= 2 * floor ? paneAreaHeight - floor : paneAreaHeight
    self.drawerHeight = min(max((paneAreaHeight * weight).rounded(), floor), ceiling)
    self.editorHeight = paneAreaHeight - drawerHeight
  }

  /// The share a drag on the drawer's top edge gives: the part of the pane area below
  /// `pointerY` (top-left origin), held so that neither side drops below
  /// `minPaneHeight`.
  ///
  /// `nil` means "keep the current share": any non-finite input, a non-positive area,
  /// or an area too short for two floors. Unlike a tree divider's midpoint fallback,
  /// a drag in a window too short for both sides must not overwrite the share.
  public static func draggedWeight(
    pointerY: Double, paneAreaHeight: Double, minPaneHeight: Double
  ) -> Double? {
    guard pointerY.isFinite, paneAreaHeight.isFinite, minPaneHeight.isFinite,
      paneAreaHeight > 0
    else { return nil }
    let floor = max(minPaneHeight, 0)
    guard paneAreaHeight >= 2 * floor else { return nil }
    return min(max(paneAreaHeight - pointerY, floor), paneAreaHeight - floor) / paneAreaHeight
  }

  /// The share a saved drawer restores at: `share` when it is finite and strictly between 0
  /// and 1, else ``defaultWeight``, since a non-finite weight collapses the layout to zero
  /// heights.
  public static func restoredWeight(_ share: Double) -> Double {
    share.isFinite && share > 0 && share < 1 ? share : defaultWeight
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
