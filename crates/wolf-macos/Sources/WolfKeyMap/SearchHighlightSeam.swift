//
//  SearchHighlightSeam.swift
//  Pure, headless mapping of Rust BYTE-offset bold ranges into UTF-16 spans.
//
//  wolf-core's `context_window` returns the visible line text plus the bold match
//  ranges as BYTE `[start, end)` offsets into that text. AppKit's
//  `NSAttributedString` / `NSRange` are UTF-16-indexed, so this seam partitions the
//  whole string into consecutive, gap-filling spans (every character covered by
//  exactly one span), each flagged `bold` iff it lies inside a bold byte range,
//  with the offsets/lengths converted to UTF-16. The `SearchView` then applies a
//  bold font to bold spans and a dimmed color to the rest.
//
//  Toolkit-free (Foundation only), so it is unit-testable without AppKit.
//

import Foundation

/// One consecutive run of the display line with a uniform bold flag, measured in
/// UTF-16 units (ready for an `NSRange`).
public struct SearchSpan: Equatable {
  /// The UTF-16 offset of the span's first unit into the display string.
  public let utf16Start: Int
  /// The span's length in UTF-16 units.
  public let utf16Length: Int
  /// Whether this run is part of a match (bold) vs. surrounding context (dimmed).
  public let bold: Bool

  public init(utf16Start: Int, utf16Length: Int, bold: Bool) {
    self.utf16Start = utf16Start
    self.utf16Length = utf16Length
    self.bold = bold
  }
}

/// Display-free BYTE-range -> UTF-16-span partitioning for search-result lines.
public enum SearchHighlightSeam {
  /// Partitions `text` into ordered, contiguous, non-overlapping spans covering
  /// every UTF-16 unit exactly once, each flagged `bold` iff its bytes lie within
  /// any range in `boldByteRanges` (byte `[start, end)` offsets into `text`'s
  /// UTF-8). Empty `boldByteRanges` yields a single non-bold span over the whole
  /// string; an empty string yields no spans.
  public static func spans(text: String, boldByteRanges: [(Int, Int)]) -> [SearchSpan] {
    var result: [SearchSpan] = []
    var byteIndex = 0
    var utf16Cursor = 0
    var current: (start: Int, length: Int, bold: Bool)?

    for scalar in text.unicodeScalars {
      let byteLen = String(scalar).utf8.count
      let utf16Len = scalar.value > 0xFFFF ? 2 : 1
      let bold = boldByteRanges.contains { byteIndex >= $0.0 && byteIndex < $0.1 }

      if var cur = current, cur.bold == bold {
        cur.length += utf16Len
        current = cur
      } else {
        if let cur = current {
          result.append(
            SearchSpan(utf16Start: cur.start, utf16Length: cur.length, bold: cur.bold))
        }
        current = (start: utf16Cursor, length: utf16Len, bold: bold)
      }

      byteIndex += byteLen
      utf16Cursor += utf16Len
    }

    if let cur = current {
      result.append(SearchSpan(utf16Start: cur.start, utf16Length: cur.length, bold: cur.bold))
    }
    return result
  }
}
