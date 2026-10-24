// Pure conversion of a sidebar width into a character budget for a matched line.

import Foundation

/// Display-free char-budget math for the search-results line window.
public enum SearchBudgetSeam {
  /// The character budget for a matched line: `max(1, floor(width / advance)) *
  /// lines`, i.e. the characters that fit on one row times the number of wrap rows
  /// (`lines`, defaulting to 2 — the "wrapped to no more than two lines" rule).
  ///
  /// A degenerate `charAdvance <= 0` (an unmeasurable font) is guarded: it floors
  /// the per-row count to 1, so the result is a positive `lines` rather than a
  /// crash or divide-by-zero.
  public static func charBudget(sidebarWidth: Double, charAdvance: Double, lines: Int = 2) -> Int {
    let perRow: Int
    if charAdvance > 0 {
      perRow = max(1, Int((sidebarWidth / charAdvance).rounded(.down)))
    } else {
      perRow = 1
    }
    return perRow * lines
  }
}
