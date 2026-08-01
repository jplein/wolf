//
//  SearchHighlightSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.SearchHighlightSeam` namespace.
//
//  These exercise the display-free seam that maps Rust BYTE offset ranges (the
//  bold match spans from `context_window`) into UTF-16 `SearchSpan`s suitable for
//  building an `NSAttributedString` / `NSRange` — scalar inputs only, NO AppKit.
//
//  CONTRACT UNDER TEST (search plan §2, `SearchHighlightSeam.swift`):
//    * struct SearchSpan { utf16Start: Int; utf16Length: Int; bold: Bool }
//    * SearchHighlightSeam.spans(text:boldByteRanges:) -> [SearchSpan]
//      — partitions the WHOLE string into consecutive, non-overlapping,
//      gap-filling spans in order (every character covered by exactly one span),
//      each flagged `bold` iff it lies inside a bold byte range. Byte offsets are
//      converted to UTF-16 offsets so multibyte scalars (accented chars, emoji
//      surrogate pairs) land on the right `utf16Start`/`utf16Length`. Empty
//      `boldByteRanges` yields a single non-bold span covering the whole string.
//
//  NOTE to coder: this test references the pure `WolfKeyMap.SearchHighlightSeam`
//  enum + `SearchSpan` struct the coder adds in
//  `Sources/WolfKeyMap/SearchHighlightSeam.swift`.
//

import XCTest

@testable import WolfKeyMap

final class SearchHighlightSeamTests: XCTestCase {

  /// Asserts that `spans` partition `text` into ordered, contiguous, non-empty,
  /// non-overlapping, gap-filling spans covering every UTF-16 unit exactly once,
  /// with `bold` true exactly for the indices in `expectedBold`.
  private func assertPartition(
    _ spans: [SearchSpan],
    text: String,
    expectedBold: Set<Int>,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    let total = text.utf16.count
    var boldAt = [Bool](repeating: false, count: total)
    var cursor = 0
    for span in spans {
      XCTAssertEqual(
        span.utf16Start, cursor,
        "spans must be contiguous and ordered (expected start \(cursor))",
        file: file, line: line)
      XCTAssertGreaterThan(
        span.utf16Length, 0,
        "no zero-length spans", file: file, line: line)
      let end = span.utf16Start + span.utf16Length
      XCTAssertLessThanOrEqual(
        end, total, "span must stay within the string", file: file, line: line)
      for i in span.utf16Start..<end {
        boldAt[i] = span.bold
      }
      cursor = end
    }
    XCTAssertEqual(
      cursor, total, "spans must cover the whole string", file: file, line: line)
    for i in 0..<total {
      XCTAssertEqual(
        boldAt[i], expectedBold.contains(i),
        "utf16 index \(i) bold flag", file: file, line: line)
    }
  }

  // MARK: - ASCII: single & multiple ranges

  func testASCIISingleRange() {
    // "hello world", bold "hello" == bytes [0,5).
    let text = "hello world"
    let spans = SearchHighlightSeam.spans(text: text, boldByteRanges: [(0, 5)])
    assertPartition(spans, text: text, expectedBold: Set(0..<5))
  }

  func testASCIIMultipleRanges() {
    // "abcdefgh", bold [1,3) and [5,7).
    let text = "abcdefgh"
    let spans = SearchHighlightSeam.spans(text: text, boldByteRanges: [(1, 3), (5, 7)])
    assertPartition(spans, text: text, expectedBold: Set([1, 2, 5, 6]))
  }

  // MARK: - adjacent & edge ranges

  func testAdjacentRanges() {
    // Two touching bold ranges [0,2) and [2,4): the union [0,4) must be bold, the
    // remainder non-bold, whether the impl merges them or keeps them separate.
    let text = "abcdef"
    let spans = SearchHighlightSeam.spans(text: text, boldByteRanges: [(0, 2), (2, 4)])
    assertPartition(spans, text: text, expectedBold: Set(0..<4))
  }

  func testRangeCoveringWholeString() {
    // A single range spanning the whole string => every char bold, one span.
    let text = "bold"
    let spans = SearchHighlightSeam.spans(text: text, boldByteRanges: [(0, 4)])
    assertPartition(spans, text: text, expectedBold: Set(0..<4))
    XCTAssertEqual(spans.count, 1, "a full-cover range yields a single bold span")
  }

  // MARK: - multibyte scalars: accented char & emoji surrogate pair

  func testAccentedCharUTF16Offsets() {
    // "café": bytes c,a,f (1 each) + é (2 bytes) = 5 bytes; UTF-16 = 4 units
    // (é is one BMP unit). Bold the é at byte [3,5) => UTF-16 index {3}.
    let text = "café"
    let spans = SearchHighlightSeam.spans(text: text, boldByteRanges: [(3, 5)])
    assertPartition(spans, text: text, expectedBold: Set([3]))
    // The bold span must be exactly the single é UTF-16 unit.
    let boldSpans = spans.filter { $0.bold }
    XCTAssertEqual(boldSpans.count, 1, "one bold span")
    XCTAssertEqual(boldSpans[0].utf16Start, 3, "é starts at UTF-16 index 3")
    XCTAssertEqual(boldSpans[0].utf16Length, 1, "é is one UTF-16 unit")
  }

  func testEmojiSurrogatePairUTF16Offsets() {
    // "a😀b": bytes a(1) 😀(4) b(1) = 6 bytes; UTF-16 = a(1) 😀(2 surrogates) b(1)
    // = 4 units. Bold the emoji at byte [1,5) => UTF-16 indices {1,2}.
    let text = "a😀b"
    let spans = SearchHighlightSeam.spans(text: text, boldByteRanges: [(1, 5)])
    assertPartition(spans, text: text, expectedBold: Set([1, 2]))
    let boldSpans = spans.filter { $0.bold }
    XCTAssertEqual(boldSpans.count, 1, "one bold span")
    XCTAssertEqual(boldSpans[0].utf16Start, 1, "emoji starts at UTF-16 index 1")
    XCTAssertEqual(
      boldSpans[0].utf16Length, 2, "an emoji is a 2-unit UTF-16 surrogate pair")
  }

  // MARK: - empty ranges => single non-bold span

  func testEmptyRangesSingleNonBoldSpan() {
    let text = "hello"
    let spans = SearchHighlightSeam.spans(text: text, boldByteRanges: [])
    assertPartition(spans, text: text, expectedBold: [])
    XCTAssertEqual(spans.count, 1, "no bold ranges => a single span")
    XCTAssertFalse(spans[0].bold, "the lone span is non-bold (dimmed context)")
    XCTAssertEqual(spans[0].utf16Start, 0)
    XCTAssertEqual(spans[0].utf16Length, text.utf16.count)
  }

  func testEmptyStringYieldsNoSpans() {
    // Degenerate: an empty string has nothing to partition.
    let spans = SearchHighlightSeam.spans(text: "", boldByteRanges: [])
    XCTAssertTrue(spans.isEmpty, "an empty string yields no spans")
  }
}
