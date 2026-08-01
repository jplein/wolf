//
//  SearchBudgetSeam.swift
//  Pure, headless conversion of a sidebar pixel width + per-character advance into
//  an integer CHARACTER budget for a matched line wrapped across up to two rows.
//
//  Toolkit-free (Foundation only), so it is unit-testable without AppKit / NSFont:
//  the `SearchView` measures its outline width + the monospace advance and passes
//  the resulting budget to `handle.start_search`'s `files(budget:)`. Keeping the
//  budget a CHAR count (not pixels) matches wolf-core's `context_window`, whose
//  char-boundary slicing never splits a multibyte scalar.
//

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
