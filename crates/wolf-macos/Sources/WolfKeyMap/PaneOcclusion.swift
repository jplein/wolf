// Pure test for whether a preview web view fully covers a pane's grid; tolerance is exactly 0.
// See notes/crates/wolf-macos/Sources/WolfKeyMap/PaneOcclusion.swift.md#why-the-tolerance-is-zero

import Foundation

/// A headless sample of a live preview `WKWebView`, taken by `PaneContainerView`
/// at paint time — everything `isFullyOccluded` needs to decide whether the view
/// genuinely, fully, opaquely covers a pane's grid rect.
///
/// Public memberwise init (matching the `FrameStats.BucketSummary` convention) so
/// tests can build fixtures directly without an AppKit view.
public struct OccluderGeometry: Equatable {
  /// The web view's frame, in the SAME coordinate space as the pane's `gridRect`.
  public let frame: Rect
  /// Whether the view is hidden, or has a hidden ancestor
  /// (`isHiddenOrHasHiddenAncestor`, NOT the bare `isHidden`).
  public let isHidden: Bool
  /// Whether the view is attached to the painting view AND is in a window.
  public let isAttached: Bool
  /// The view's alpha. Only a fully opaque `1.0` can occlude.
  public let alpha: Double
  /// Whether the view has ever finished loading a document. A web view that
  /// exists but has loaded nothing is blank/white and must NOT suppress the grid.
  public let hasLoadedContent: Bool

  /// Creates an occluder sample from its components.
  public init(
    frame: Rect,
    isHidden: Bool,
    isAttached: Bool,
    alpha: Double,
    hasLoadedContent: Bool
  ) {
    self.frame = frame
    self.isHidden = isHidden
    self.isAttached = isAttached
    self.alpha = alpha
    self.hasLoadedContent = hasLoadedContent
  }
}

/// The occlusion decision: is a pane's grid rect fully hidden behind an opaque
/// preview web view, and therefore safe to skip painting?
public enum PaneOcclusion {
  /// Returns `true` only when `occluder` genuinely, fully and opaquely covers
  /// `gridRect` — i.e. when skipping the grid paint is invisible to the user.
  ///
  /// ALL of the following must hold; any one failing yields `false` (PAINT):
  ///
  /// 1. `occluder != nil` — there is no web view yet (`previewWebViews[win] == nil`).
  /// 2. `hasLoadedContent` — the view exists but has loaded nothing (blank/white).
  /// 3. `isAttached` — the view is detached from the view tree or has no window.
  /// 4. `!isHidden` — the view, or an ancestor, is hidden.
  /// 5. `alpha >= 1.0` — anything less lets the grid show through.
  /// 6. `frame.w > 0 && frame.h > 0` — a zero-size view covers nothing.
  /// 7. `gridRect.w > 0 && gridRect.h > 0` — a degenerate grid rect reports NOT
  ///    occluded, conservatively: there is no paint work to save, and `true`
  ///    would be a claim about a rectangle that does not exist.
  /// 8. Containment on all four edges with tolerance EXACTLY 0.0 (see the file
  ///    header). Equality counts as containment (`<=` / `>=`), because the common
  ///    in-sync case is bit-identical rectangles.
  ///
  /// PARTIAL coverage always returns `false`: this seam answers only "fully
  /// occluded", never "partly occluded", so the caller's choice stays binary.
  public static func isFullyOccluded(gridRect: Rect, by occluder: OccluderGeometry?) -> Bool {
    // Rule 1: no web view yet.
    guard let occluder else { return false }

    // Rules 2-5: the per-view disqualifiers.
    guard occluder.hasLoadedContent else { return false }
    guard occluder.isAttached else { return false }
    guard !occluder.isHidden else { return false }
    guard occluder.alpha >= 1.0 else { return false }

    // Rule 6: a zero-size occluder covers nothing.
    guard occluder.frame.w > 0, occluder.frame.h > 0 else { return false }

    // Rule 7: a degenerate grid rect is conservatively NOT occluded.
    guard gridRect.w > 0, gridRect.h > 0 else { return false }

    // Rule 8: containment on all four edges, tolerance exactly 0.0. `Rect` has no
    // `maxX` / `maxY` accessors, so the far edges are computed inline.
    let frame = occluder.frame
    return frame.x <= gridRect.x
      && frame.y <= gridRect.y
      && (frame.x + frame.w) >= (gridRect.x + gridRect.w)
      && (frame.y + frame.h) >= (gridRect.y + gridRect.h)
  }
}
