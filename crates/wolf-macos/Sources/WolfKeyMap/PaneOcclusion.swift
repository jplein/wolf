//
//  PaneOcclusion.swift
//  Pure, AppKit-/CoreGraphics-free arithmetic for the preview-web-view OCCLUSION
//  test: may a pane's text grid be skipped because a `WKWebView` fully covers it?
//
//  This is a headless seam in the `WolfKeyMap` module — Foundation-only, no live
//  view, no `WKWebView`, no `CGFloat`. `PaneContainerView` samples the live web
//  view into an `OccluderGeometry` value and asks `isFullyOccluded` whether the
//  pane's grid paint (the FFI `read_frame_for_grid` plus `paintGrid`) can be
//  skipped for this frame. Only the GRID may be skipped; the statusbar strip sits
//  OUTSIDE `gridRect` and is never covered, so it always paints.
//
//  Like `EditorInset` / `PickerPanelSeam` it reuses the existing `Rect` value type
//  from `PaneTree.swift` and is `Double`-based so it stays headlessly testable (see
//  `PaneOcclusionTests.swift`).
//
//  ## Why the containment tolerance is EXACTLY 0.0
//
//  THE INVARIANT THAT MAKES SKIPPING SAFE IS "the web view physically covers the
//  region whose paint was skipped" — NOT "the web view frame and the grid rect are
//  in sync". Read the rest of this section with that distinction in mind; the sync
//  argument below is the COMMON CASE, not the guarantee.
//
//  In the common case `syncPreviewWebViews` assigns the web view's frame from the
//  SAME `gridRect(ofPane:)` expression the painter is handed, so the two rectangles
//  are bit-identical whenever the view tree and the paint are in sync. Out of sync
//  — mid-live-resize, just after a split, just before a close — the two rectangles
//  disagree, and the containment test then decides on the geometry actually
//  sampled, which is exactly the point:
//
//  - If the pane GREW and `draw` runs before `syncPreviewWebViews`, the stale
//    SMALLER web frame no longer contains the new larger `gridRect`, containment
//    fails, and the grid PAINTS. Nothing is skipped that the web view does not
//    cover.
//  - If the pane SHRANK and `draw` runs before `syncPreviewWebViews`, the stale
//    LARGER web frame still contains the new smaller `gridRect`, containment
//    SUCCEEDS, and the paint IS skipped. This is still visually safe — but note
//    that it is safe because the oversized web view physically covers at least the
//    grid rect at composite time, so the skipped pixels are precisely the covered
//    ones. It is NOT safe because the frames were in sync; they were not.
//
//  So "pane geometry changes restore painting immediately" holds STRUCTURALLY
//  rather than by bookkeeping, but it holds via the coverage invariant. Do not
//  weaken the test on the strength of the sync argument alone: a future change that
//  lets the web view stop covering the skipped region — an epsilon tolerance, an
//  inset, a transform, a clipping ancestor, a differing coordinate space — breaks
//  correctness even while the frames look "in sync". In particular no epsilon may
//  be introduced here, because it would let a slightly-too-small web view suppress
//  the paint underneath it and leave a garbage region on screen.
//

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
