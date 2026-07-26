//
//  DividerDragSeam.swift
//  Pure, AppKit-free divider-drag math for the macOS split-panes UI.
//
//  Maps a pointer position along the drag axis to a clamped left-child fraction
//  for the two adjacent panes a divider governs. The clamp keeps NO pane below the
//  minimum grid size (min columns for a vertical divider, min rows for a
//  horizontal one), accounting for per-side pixel OVERHEAD (editor inset +
//  statusbar). The clamp lives HERE (it needs cell size + pixel spans), NOT in the
//  pixel-free `PaneTree`. Foundation-only, unit-tested headlessly (see
//  `DividerDragSeamTests.swift`).
//

import Foundation

/// The pure divider-drag clamp. A namespace (`enum` with only static members),
/// mirroring the other `WolfKeyMap` seams.
public enum DividerDragSeam {
  /// The midpoint fallback for a pair too small to hold both per-side minima.
  private static let midpoint: Double = 0.5

  /// Returns the clamped left-child fraction for a divider drag.
  ///
  /// The raw fraction is `(pointerPos - pairStart) / pairSpan`. Per-side minimum
  /// pixels are `minCols * cellWidth + overhead` (vertical) or `minRows *
  /// cellHeight + overhead` (horizontal), where the LEFT/TOP side uses
  /// `leadingOverhead` and the RIGHT/BOTTOM side uses `trailingOverhead`. The
  /// fraction is clamped to `[minLeft / pairSpan, 1 - minRight / pairSpan]`. A pair
  /// too small to hold both minima (an inverted range) yields the midpoint `0.5`.
  public static func clampedLeftFraction(
    pairStart: Double, pairSpan: Double, pointerPos: Double,
    orientation: Orientation, cellWidth: Double, cellHeight: Double,
    minCols: Int, minRows: Int,
    leadingOverhead: Double, trailingOverhead: Double
  ) -> Double {
    guard pairSpan > 0 else {
      return midpoint
    }

    let minCellPixels: Double
    switch orientation {
    case .vertical:
      minCellPixels = Double(minCols) * cellWidth
    case .horizontal:
      minCellPixels = Double(minRows) * cellHeight
    }

    let minLeft = minCellPixels + leadingOverhead
    let minRight = minCellPixels + trailingOverhead
    let lower = minLeft / pairSpan
    let upper = 1.0 - minRight / pairSpan
    if lower > upper {
      return midpoint
    }

    let raw = (pointerPos - pairStart) / pairSpan
    return min(max(raw, lower), upper)
  }
}
