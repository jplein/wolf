//
//  StatusbarTextSpan.swift
//  Pure, AppKit-/CoreGraphics-free HORIZONTAL span math for the per-pane statusbar
//  text (plan §1).
//
//  This is a headless seam in the `WolfKeyMap` module — Foundation-only, no live
//  view, no `CGFloat` — and the horizontal sibling of `StatusbarLayout`, which owns
//  the VERTICAL math (strip height, line tops/heights). Given a pane's ABSOLUTE
//  grid rect (x origin + width, as produced by `PaneContainerView.gridRect(ofPane:)`),
//  the grid-centering inset `PaneGeometry.paintOffsetX`, and the pane's CONTENT
//  KIND, it yields the left edge and width of the statusbar's text span.
//
//  The two kinds differ because their CONTENT is painted at different origins:
//
//    - `.editor` — the text grid itself is painted at `gridRect.minX + paintOffsetX`
//      (the 90-column centering inset), so the statusbar text must be indented to
//      match. This branch reproduces the pre-existing expression EXACTLY
//      (`textLeft = gRect.minX + offset`, `textWidth = gRect.maxX - textLeft`), so
//      editor panes are bit-identical; only the clamp below is new, and it is a
//      no-op for every real input.
//    - `.preview` — `PaneContainerView.syncPreviewWebViews` frames the preview
//      `WKWebView` to the UNCENTERED `gridRect(ofPane:)` and the preview CSS is
//      flush (`body.wolf-markdown { margin: 0; padding: 0 }`), so the content's left
//      edge IS `gridRect.minX`. This branch therefore deliberately IGNORES
//      `paintOffsetX` and spans the whole grid rect, putting BOTH text edges on the
//      webview frame. If the preview webview is ever CENTERED instead, this branch
//      must revert to `.editor` semantics.
//
//  Only `width` clamps to a non-negative floor; `left` is a fixed offset from
//  `gridLeft` (bounded above by the grid's right edge via the offset clamp). Like
//  `EditorInset` / `PaneGeometry` / `StatusbarLayout` it is `Double`-based so it
//  stays headlessly testable (see `StatusbarTextSpanTests.swift`).
//

import Foundation

/// Which kind of content a pane paints, for the purpose of horizontally aligning
/// its statusbar text with that content.
public enum PaneContentKind {
  /// A normal text-grid pane, whose grid is painted at the centering
  /// `PaneGeometry.paintOffsetX` when the pane exceeds the column cap.
  case editor
  /// A Markdown-preview pane, whose `WKWebView` is framed to the UNCENTERED grid
  /// rect with flush CSS, so its content starts at the grid rect's left edge.
  case preview
}

/// The horizontal span (absolute left edge + width) of a pane's statusbar text.
///
/// `left` is ABSOLUTE — it is `gridLeft` (the pane's grid-rect x origin in the
/// container view's coordinates) plus, for `.editor` only, the grid-centering
/// offset. `width` never goes negative: the grid width floors at 0 and the offset
/// is clamped into `[0, width]`, so `left` also never passes the grid's right edge.
public struct StatusbarTextSpan {
  /// The absolute x origin of the statusbar text, in points.
  public let left: Double
  /// The width available to the statusbar text, in points (never negative).
  public let width: Double

  /// Computes the span from the pane's absolute grid-rect x origin and width, the
  /// grid-centering inset (`PaneGeometry.paintOffsetX`), and the pane's content
  /// kind.
  ///
  /// With `w = max(0, gridWidth)` and `offset = min(max(0, paintOffsetX), w)`:
  /// `.editor` gives `left = gridLeft + offset`, `width = max(0, w - offset)`;
  /// `.preview` gives `left = gridLeft`, `width = w`. The negative-`paintOffsetX`
  /// clamp is defensive (`PaneGeometry` already floors it at 0).
  public init(gridLeft: Double, gridWidth: Double, paintOffsetX: Double, kind: PaneContentKind) {
    let w = max(0, gridWidth)
    let offset = min(max(0, paintOffsetX), w)

    switch kind {
    case .editor:
      self.left = gridLeft + offset
      self.width = max(0, w - offset)
    case .preview:
      self.left = gridLeft
      self.width = w
    }
  }
}
