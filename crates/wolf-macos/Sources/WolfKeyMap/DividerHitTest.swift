//
//  DividerHitTest.swift
//  Pure, AppKit-free divider picking for the macOS split-panes UI.
//
//  Picks, from a PRE-COMPUTED list of `PaneTree.Divider`s (produced by
//  `PaneTree.dividers(width:height:)`), the NEAREST divider whose zero-thickness
//  line is within `tolerance` of a point AND whose point falls within the
//  divider's span. Enumeration lives on `PaneTree` (it needs the private `Frame`);
//  picking is pure over the public list, so it lives here in a Foundation-only
//  seam and is unit-tested headlessly (see `DividerHitTestTests.swift`).
//

import Foundation

/// The pure divider hit-test. A namespace (`enum` with only static members),
/// mirroring the other `WolfKeyMap` seams.
public enum DividerHitTest {
  /// Returns the nearest divider in `dividers` whose line is within `tolerance` of
  /// `point` AND whose point falls within the divider's span, or `nil` when none
  /// qualifies.
  ///
  /// A `.vertical` divider is a vertical segment at `lineRect.x` spanning
  /// `[lineRect.y, lineRect.y + lineRect.h]`; its distance is `|point.x -
  /// lineRect.x|` and the point must fall within that y-span. A `.horizontal`
  /// divider is the transpose. A point beyond the span (off the ends of the line)
  /// does NOT hit even when on the line's axis.
  public static func hit(
    _ dividers: [PaneTree.Divider], point: (x: Double, y: Double), tolerance: Double
  ) -> PaneTree.Divider? {
    var best: (divider: PaneTree.Divider, distance: Double)?
    for divider in dividers {
      guard let distance = distance(to: divider, point: point) else {
        continue
      }
      if distance <= tolerance, best == nil || distance < best!.distance {
        best = (divider, distance)
      }
    }
    return best?.divider
  }

  /// The perpendicular distance from `point` to `divider`'s line, or `nil` when
  /// the point falls outside the line segment's span.
  private static func distance(
    to divider: PaneTree.Divider, point: (x: Double, y: Double)
  ) -> Double? {
    let line = divider.lineRect
    switch divider.orientation {
    case .vertical:
      guard point.y >= line.y, point.y <= line.y + line.h else {
        return nil
      }
      return abs(point.x - line.x)
    case .horizontal:
      guard point.x >= line.x, point.x <= line.x + line.w else {
        return nil
      }
      return abs(point.y - line.y)
    }
  }
}
