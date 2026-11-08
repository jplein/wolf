// Accumulates fractional scroll deltas into signed whole-cell line steps.

import Foundation

/// Accumulates fractional scroll `deltaY` into discrete whole-cell steps.
///
/// The private `remainder` is SIGNED and CARRIES between calls, so a run of
/// sub-cell nudges crosses a cell boundary exactly once, and a direction reversal
/// is handled symmetrically.
public struct ScrollAccumulator {
  /// The most steps one `consume` call returns, in either direction.
  private static let maxStepsPerEvent: Double = 256

  /// The signed, carried sub-cell remainder (in pixels).
  private var remainder: Double = 0

  /// Creates a zeroed accumulator.
  public init() {}

  /// Adds `deltaY` to the carried remainder and returns the signed whole-cell
  /// step count newly crossed (positive = the +deltaY direction), in O(1).
  ///
  /// A non-finite `deltaY`, or a non-finite or non-positive `cellHeight`, returns 0
  /// and leaves the carry untouched. A carry that overflows to a non-finite value
  /// is zeroed and the call returns 0, so one bad event cannot stop scrolling for
  /// the session. One call yields at most 256 steps either way; a call the cap cut
  /// short drops its carry. Otherwise the sub-cell leftover carries to the next call.
  public mutating func consume(deltaY: Double, cellHeight: Double) -> Int {
    guard deltaY.isFinite, cellHeight.isFinite, cellHeight > 0 else { return 0 }
    remainder += deltaY
    guard remainder.isFinite else {
      remainder = 0
      return 0
    }
    // Truncating toward zero keeps a direction reversal symmetric.
    let raw = (remainder / cellHeight).rounded(.towardZero)
    let clamped = min(max(raw, -Self.maxStepsPerEvent), Self.maxStepsPerEvent)
    remainder -= clamped * cellHeight
    if clamped != raw {
      remainder = 0
    }
    return Int(clamped)
  }

  /// Clears the carried remainder (e.g. when the scroll target pane changes).
  public mutating func reset() {
    remainder = 0
  }
}
