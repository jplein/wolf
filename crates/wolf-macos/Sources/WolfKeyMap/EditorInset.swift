// Pure arithmetic for the per-pane text-grid inset.

import Foundation

/// The per-pane text-grid inset: the pane-local rectangle the grid paints into
/// (inset around the statusbar strip and the pane edges), plus the horizontal text
/// inset shared by the statusbar and cmdline text.
///
/// Only `gridRect.w` / `gridRect.h` CLAMP to a non-negative floor (`max(0, ...)`),
/// so a pane narrower than `2 * padding` (or shorter than `statusbarHeight +
/// 2 * padding`) never yields negative grid dimensions. The `gridRect.x` / `.y`
/// origins are FIXED offsets and are NOT clamped. `gridRect` is PANE-LOCAL (its
/// origin is the pane's top-left); the caller offsets it into the pane's view rect.
public struct EditorInset {
  /// The uniform inset, in points, applied around each pane's text grid — the
  /// single source of truth, consumed by both `PaneContainerView` and
  /// `MainWindow`.
  public static let padding: Double = 20

  /// The pane width, in points (echoed input).
  public let paneWidth: Double
  /// The pane height, in points (echoed input).
  public let paneHeight: Double
  /// The statusbar-strip height at the top of the pane, in points (echoed input).
  public let statusbarHeight: Double
  /// The uniform inset applied around the grid, in points (echoed input).
  public let padding: Double

  /// The text grid rectangle, PANE-LOCAL (origin at the pane's top-left):
  /// `x = padding`, `y = statusbarHeight + padding`,
  /// `w = max(0, paneWidth - 2 * padding)`,
  /// `h = max(0, paneHeight - statusbarHeight - 2 * padding)`.
  /// Only `w` / `h` clamp; `x` / `y` are fixed offsets.
  public let gridRect: Rect
  /// The horizontal inset for the statusbar / cmdline text (`== padding`), so that
  /// text left-aligns with the grid text.
  public let textLeftInset: Double

  /// Computes the per-pane inset from the pane size, the statusbar-strip height,
  /// and the uniform `padding`. Only the grid width/height clamp to a non-negative
  /// floor so a too-small pane never produces a negative grid dimension.
  public init(paneWidth: Double, paneHeight: Double, statusbarHeight: Double, padding: Double) {
    self.paneWidth = paneWidth
    self.paneHeight = paneHeight
    self.statusbarHeight = statusbarHeight
    self.padding = padding

    self.gridRect = Rect(
      x: padding,
      y: statusbarHeight + padding,
      w: max(0, paneWidth - 2 * padding),
      h: max(0, paneHeight - statusbarHeight - 2 * padding))
    self.textLeftInset = padding
  }
}
