// Pure horizontal span math for the per-pane statusbar text.

import Foundation

/// Which kind of content a pane paints, for the purpose of horizontally aligning
/// its statusbar text with that content.
public enum PaneContentKind {
  /// A normal text-grid pane, whose grid is painted at the centering
  /// `PaneGeometry.paintOffsetX` when the pane exceeds the column cap.
  case editor
  /// A Markdown-preview pane, whose `WKWebView` is now framed to the capped+CENTERED
  /// grid rect (or the full inset pane in full-width mode), so its content aligns
  /// with the editor grid — indented by `PaneGeometry.paintOffsetX` in the ~90-char
  /// default, flush at the grid's left edge in full-width mode.
  case preview
}

/// The horizontal span (absolute left edge + width) of a pane's statusbar text.
///
/// `left` is ABSOLUTE — it is `gridLeft` (the pane's grid-rect x origin in the
/// container view's coordinates) plus the grid-centering offset (for BOTH `.editor`
/// and `.preview`, now identical). `width` never goes negative: the grid width floors
/// at 0 and the offset is clamped into `[0, width]`, so `left` also never passes the
/// grid's right edge.
public struct StatusbarTextSpan {
  /// The absolute x origin of the statusbar text, in points.
  public let left: Double
  /// The width available to the statusbar text, in points (never negative).
  public let width: Double

  /// Computes the span from the pane's absolute grid-rect x origin and width, the
  /// grid-centering inset (`PaneGeometry.paintOffsetX`), and the pane's content
  /// kind.
  ///
  /// With `w = max(0, gridWidth)` and `offset = min(max(0, paintOffsetX), w)`, BOTH
  /// `.editor` and `.preview` give `left = gridLeft + offset`,
  /// `width = max(0, w - offset)` (the preview is now centered exactly like the
  /// editor). The negative-`paintOffsetX` clamp is defensive (`PaneGeometry` already
  /// floors it at 0).
  public init(gridLeft: Double, gridWidth: Double, paintOffsetX: Double, kind: PaneContentKind) {
    let w = max(0, gridWidth)
    let offset = min(max(0, paintOffsetX), w)

    switch kind {
    case .editor, .preview:
      self.left = gridLeft + offset
      self.width = max(0, w - offset)
    }
  }
}
