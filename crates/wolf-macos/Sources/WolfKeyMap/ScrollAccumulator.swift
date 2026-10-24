// Accumulates fractional scroll deltas into signed whole-cell line steps.

import Foundation

/// Accumulates fractional scroll `deltaY` into discrete whole-cell steps.
///
/// The private `remainder` is SIGNED and CARRIES between calls, so a run of
/// sub-cell nudges crosses a cell boundary exactly once, and a direction reversal
/// is handled symmetrically.
public struct ScrollAccumulator {
  /// The signed, carried sub-cell remainder (in pixels).
  private var remainder: Double = 0

  /// Creates a zeroed accumulator.
  public init() {}

  /// Adds `deltaY` to the carried remainder and returns the signed whole-cell
  /// step count newly crossed (positive = the +deltaY direction).
  ///
  /// Guards a non-positive `cellHeight` by returning 0 (never divides by zero or
  /// loops forever). The remainder carries any sub-cell leftover to the next call.
  public mutating func consume(deltaY: Double, cellHeight: Double) -> Int {
    guard cellHeight > 0 else { return 0 }
    remainder += deltaY
    var steps = 0
    while remainder >= cellHeight {
      steps += 1
      remainder -= cellHeight
    }
    while remainder <= -cellHeight {
      steps -= 1
      remainder += cellHeight
    }
    return steps
  }

  /// Clears the carried remainder (e.g. when the scroll target pane changes).
  public mutating func reset() {
    remainder = 0
  }
}
