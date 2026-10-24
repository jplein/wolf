// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#statusbartextspantestsswift
// See notes/crates/wolf-macos/Tests/WolfKeyMapTests/StatusbarTextSpanTests.swift.md#manual-check

import XCTest

@testable import WolfKeyMap

final class StatusbarTextSpanTests: XCTestCase {

  /// Tolerance for the exact-valued `Double` assertions below (ported from the
  /// sibling headless tests' `eps`).
  private let eps: Double = 1e-9

  /// A representative cell width (px), used to express the 90-column cap.
  private let cellWidth: Double = 8.0

  /// The hard-coded column cap `PaneContainerView` passes to `PaneGeometry`.
  private let maxGridCols: Double = 90.0

  /// A representative pane-local grid origin: `P.minX + EditorInset.padding`
  /// for a pane at x == 0.
  private let gridLeftAtOrigin: Double = 20.0

  /// A representative grid origin for a right-hand split.
  private let gridLeftSplit: Double = 512.0

  // MARK: - Helpers

  /// Builds a span from the four inputs, in the initializer's argument order.
  private func span(
    gridLeft: Double, gridWidth: Double, paintOffsetX: Double, kind: PaneContentKind
  ) -> StatusbarTextSpan {
    StatusbarTextSpan(
      gridLeft: gridLeft,
      gridWidth: gridWidth,
      paintOffsetX: paintOffsetX,
      kind: kind)
  }

  /// The centering inset `PaneGeometry` derives for a grid wider than the cap:
  /// `max(0, (gridWidth - min(gridWidth, cap)) / 2)`.
  private func expectedPaintOffsetX(gridWidth: Double) -> Double {
    let cap = maxGridCols * cellWidth
    return max(0.0, (gridWidth - min(gridWidth, cap)) / 2.0)
  }

  /// Asserts the universal invariants every span must satisfy regardless of kind,
  /// mirroring `assertInvariants` in `StatusbarLayoutTests`.
  private func assertInvariants(
    _ s: StatusbarTextSpan,
    gridLeft: Double,
    gridWidth: Double,
    _ desc: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    // The width never goes negative, however large the offset.
    XCTAssertGreaterThanOrEqual(
      s.width, 0.0, "\(desc): width >= 0 (got \(s.width))", file: file, line: line)

    // The text never starts left of the grid's own left edge.
    XCTAssertGreaterThanOrEqual(
      s.left, gridLeft, "\(desc): left >= gridLeft (got \(s.left))", file: file, line: line)

    // The text never extends past the grid's right edge.
    XCTAssertLessThanOrEqual(
      s.left + s.width, gridLeft + max(0.0, gridWidth) + eps,
      "\(desc): left + width <= gridLeft + max(0, gridWidth) "
        + "(got \(s.left + s.width), limit \(gridLeft + max(0.0, gridWidth)))",
      file: file, line: line)
  }

  // MARK: - Case 1: editor, narrow pane (no centering offset)

  func testEditorNarrowPaneIsFlushWithTheGrid() {
    // A pane below the 90-column cap has paintOffsetX == 0, so the text starts at
    // the grid's left edge and spans the whole grid.
    let gridWidth: Double = 600.0
    XCTAssertEqual(
      expectedPaintOffsetX(gridWidth: gridWidth), 0.0, accuracy: eps,
      "a 600px grid is below the 720px cap, so PaneGeometry yields no offset")

    let s = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: 0, kind: .editor)

    XCTAssertEqual(s.left, gridLeftAtOrigin, accuracy: eps, "editor narrow: left == gridLeft")
    XCTAssertEqual(s.width, gridWidth, accuracy: eps, "editor narrow: width == gridWidth")
  }

  // MARK: - Case 2: editor, wide pane — REGRESSION GUARD

  func testEditorWidePaneKeepsThePaintOffsetAndTheRightEdge() {
    // The editor span is left = gRect.minX + paintOffsetX, width = gridWidth - paintOffsetX,
    // using PaneGeometry's real offset for this width: (1400 - 720) / 2 = 340.
    let gridWidth: Double = 1400.0
    let paintOffsetX = expectedPaintOffsetX(gridWidth: gridWidth)
    XCTAssertEqual(
      paintOffsetX, 340.0, accuracy: eps,
      "a 1400px grid is 680px past the 720px cap, so PaneGeometry centers it by 340px")

    let s = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: paintOffsetX,
      kind: .editor)

    XCTAssertEqual(
      s.left, gridLeftAtOrigin + paintOffsetX, accuracy: eps,
      "editor wide: left == gridLeft + paintOffsetX (the grid-centering inset)")
    XCTAssertEqual(
      s.width, gridWidth - paintOffsetX, accuracy: eps,
      "editor wide: width == gridWidth - paintOffsetX == 1060")

    // Regression guard: the editor's right edge stays gRect.maxX. Dropping paintOffsetX from the
    // .editor branch, or clamping its right edge, fails here.
    XCTAssertEqual(
      s.left + s.width, gridLeftAtOrigin + gridWidth, accuracy: eps,
      "REGRESSION: editor textRight must stay at gridLeft + gridWidth. The .editor "
        + "branch must keep adding paintOffsetX to left and subtracting it from width; "
        + "do not 'simplify' it to match the .preview branch.")
  }

  // MARK: - Case 3: preview, narrow pane — flush with the grid below the cap

  func testPreviewNarrowPaneMatchesTheEditorCase() {
    // Below the cap there is no centering offset, so both kinds sit at the grid's left edge; this
    // is also the full-width preview case, where paintOffsetX == 0.
    let gridWidth: Double = 600.0
    let s = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: 0, kind: .preview)

    XCTAssertEqual(s.left, gridLeftAtOrigin, accuracy: eps, "preview narrow: left == gridLeft")
    XCTAssertEqual(s.width, gridWidth, accuracy: eps, "preview narrow: width == gridWidth")
  }

  // MARK: - Case 4: preview, wide pane — centered like the editor, plus full-width

  func testPreviewWidePaneIsCenteredAndFullWidthIsFlush() {
    // A wide preview at the ~90-char default is framed to the capped, centered gridRect, so its
    // statusbar text indents by paintOffsetX exactly like the editor's.
    let gridWidth: Double = 1400.0
    let paintOffsetX = expectedPaintOffsetX(gridWidth: gridWidth)
    XCTAssertGreaterThan(
      paintOffsetX, 0.0,
      "this width must be past the cap, or there would be no centering offset")

    let centered = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: paintOffsetX,
      kind: .preview)

    XCTAssertEqual(
      centered.left, gridLeftAtOrigin + paintOffsetX, accuracy: eps,
      "preview wide default: left == gridLeft + paintOffsetX (centered like the editor)")
    XCTAssertEqual(
      centered.width, gridWidth - paintOffsetX, accuracy: eps,
      "preview wide default: width == gridWidth - paintOffsetX")

    // Full-width preview: the web view fills the inset pane, so paintOffsetX == 0 and the text sits
    // flush at the left edge across the whole grid.
    let full = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: 0, kind: .preview)
    XCTAssertEqual(
      full.left, gridLeftAtOrigin, accuracy: eps,
      "preview full-width: left == gridLeft (no centering inset)")
    XCTAssertEqual(
      full.width, gridWidth, accuracy: eps,
      "preview full-width: width == gridWidth (the whole inset pane)")
  }

  // MARK: - Case 5: the two kinds are now IDENTICAL (the redefinition)

  func testPreviewAndEditorSpansAreIdenticalForTheSameInputs() {
    // Both kinds are centered alike, so `.preview` and `.editor` must give the same span for the
    // same inputs, on a wide (centered) pane and a narrow (flush) one.
    let wideWidth: Double = 1400.0
    let widePaintOffsetX = expectedPaintOffsetX(gridWidth: wideWidth)
    XCTAssertGreaterThan(
      widePaintOffsetX, 0.0,
      "this width must be past the cap, or the centering would trivially be zero")

    let wideEditor = span(
      gridLeft: gridLeftAtOrigin, gridWidth: wideWidth, paintOffsetX: widePaintOffsetX,
      kind: .editor)
    let widePreview = span(
      gridLeft: gridLeftAtOrigin, gridWidth: wideWidth, paintOffsetX: widePaintOffsetX,
      kind: .preview)

    XCTAssertEqual(
      wideEditor.left, widePreview.left, accuracy: eps,
      "wide: preview.left must EQUAL editor.left (the preview is centered like the editor)")
    XCTAssertEqual(
      wideEditor.width, widePreview.width, accuracy: eps,
      "wide: preview.width must EQUAL editor.width")

    // Narrow (below the cap): offset 0, so both kinds sit flush at gridLeft.
    let narrowWidth: Double = 600.0
    let narrowEditor = span(
      gridLeft: gridLeftAtOrigin, gridWidth: narrowWidth, paintOffsetX: 0, kind: .editor)
    let narrowPreview = span(
      gridLeft: gridLeftAtOrigin, gridWidth: narrowWidth, paintOffsetX: 0, kind: .preview)

    XCTAssertEqual(
      narrowEditor.left, narrowPreview.left, accuracy: eps,
      "narrow: both kinds share the same left")
    XCTAssertEqual(
      narrowEditor.width, narrowPreview.width, accuracy: eps,
      "narrow: both kinds share the same width")
  }

  // MARK: - Case 6: gridLeft is ABSOLUTE, never pane-local

  func testNonZeroGridLeftIsAbsoluteForBothKinds() {
    // `gridLeft` is the absolute gRect.minX, so a right-hand split's span just shifts right. Tuples
    // are (gridLeft, gridWidth), with offsets derived as PaneGeometry does (1400 -> 340).
    let narrowWidth: Double = 600.0
    let wideWidth: Double = 1400.0
    let cases: [(Double, Double)] = [
      (gridLeftAtOrigin, narrowWidth),
      (gridLeftAtOrigin, wideWidth),
      (gridLeftSplit, narrowWidth),
      (gridLeftSplit, wideWidth),
    ]
    for (gridLeft, gridWidth) in cases {
      let paintOffsetX = expectedPaintOffsetX(gridWidth: gridWidth)
      let desc = "gridLeft=\(gridLeft) gridWidth=\(gridWidth) offset=\(paintOffsetX)"

      let editor = span(
        gridLeft: gridLeft, gridWidth: gridWidth, paintOffsetX: paintOffsetX, kind: .editor)
      XCTAssertEqual(
        editor.left, gridLeft + paintOffsetX, accuracy: eps,
        "\(desc): editor left == gridLeft + offset (absolute)")

      // The preview is now centered like the editor, so its absolute left is the same
      // gridLeft + offset (a right-hand split shifts it right by gridLeft).
      let preview = span(
        gridLeft: gridLeft, gridWidth: gridWidth, paintOffsetX: paintOffsetX, kind: .preview)
      XCTAssertEqual(
        preview.left, gridLeft + paintOffsetX, accuracy: eps,
        "\(desc): preview left == gridLeft + offset (absolute, centered like the editor)")

      assertInvariants(editor, gridLeft: gridLeft, gridWidth: gridWidth, "editor \(desc)")
      assertInvariants(preview, gridLeft: gridLeft, gridWidth: gridWidth, "preview \(desc)")
    }
  }

  // MARK: - Case 7: zero-width pane

  func testZeroWidthPaneProducesZeroWidthSpanForBothKinds() {
    // A degenerate grid yields a zero-width span at the grid origin — never negative.
    for kind in [PaneContentKind.editor, PaneContentKind.preview] {
      let s = span(gridLeft: gridLeftAtOrigin, gridWidth: 0, paintOffsetX: 0, kind: kind)
      XCTAssertEqual(s.width, 0.0, accuracy: eps, "\(kind): zero-width grid -> width == 0")
      XCTAssertEqual(s.left, gridLeftAtOrigin, accuracy: eps, "\(kind): left == gridLeft")
      XCTAssertGreaterThanOrEqual(s.width, 0.0, "\(kind): width is never negative")
    }
  }

  // MARK: - Case 8: a pane narrower than the padding

  func testPaneNarrowerThanThePaddingProducesNoNegativeWidth() {
    // EditorInset clamps a sub-2*padding grid width to 0 but not the x origin: a 10px pane at x 100
    // gets gridWidth 0 and gridLeft 120, past its right edge, and the span must still degenerate.
    let paneMinX: Double = 100.0
    let paneWidth: Double = 10.0
    let gridLeft = paneMinX + gridLeftAtOrigin
    XCTAssertLessThan(
      paneWidth, 2 * gridLeftAtOrigin,
      "the pane is narrower than 2*padding, so EditorInset clamps its grid width to 0")
    XCTAssertGreaterThan(
      gridLeft, paneMinX + paneWidth,
      "the unclamped grid origin sits beyond the narrow pane's right edge")

    for kind in [PaneContentKind.editor, PaneContentKind.preview] {
      let s = span(gridLeft: gridLeft, gridWidth: 0, paintOffsetX: 0, kind: kind)
      XCTAssertEqual(
        s.width, 0.0, accuracy: eps,
        "\(kind): EditorInset's clamped 0 grid width -> zero-width span")
      XCTAssertGreaterThanOrEqual(s.width, 0.0, "\(kind): width is never negative")
      assertInvariants(s, gridLeft: gridLeft, gridWidth: 0, "\(kind) sub-padding pane")
    }
  }

  // MARK: - Case 9: paintOffsetX larger than the grid

  func testOffsetLargerThanTheGridClampsToTheRightEdge() {
    // Defensive: an offset past the grid's right edge clamps so the editor width
    // floors at 0 and the left never passes gridLeft + gridWidth.
    let gridWidth: Double = 100.0
    let paintOffsetX: Double = 400.0

    let editor = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: paintOffsetX,
      kind: .editor)
    XCTAssertEqual(
      editor.width, 0.0, accuracy: eps, "editor: an over-large offset floors width at 0")
    XCTAssertEqual(
      editor.left, gridLeftAtOrigin + gridWidth, accuracy: eps,
      "editor: left clamps to the grid's right edge, never past it")

    // The preview branch clamps like the editor's: an over-large offset floors its width at 0 and
    // clamps its left to the grid's right edge.
    let preview = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: paintOffsetX,
      kind: .preview)
    XCTAssertEqual(
      preview.width, 0.0, accuracy: eps, "preview: an over-large offset floors width at 0")
    XCTAssertEqual(
      preview.left, gridLeftAtOrigin + gridWidth, accuracy: eps,
      "preview: left clamps to the grid's right edge, never past it")
  }

  // MARK: - Case 10: negative paintOffsetX (defensive)

  func testNegativeOffsetClampsToZero() {
    // A negative offset is impossible today (PaneGeometry already does max(0, ...)),
    // but the clamp must make the editor span identical to the no-offset case.
    let gridWidth: Double = 600.0
    let negative = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: -75.0, kind: .editor)
    let zero = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: 0.0, kind: .editor)

    XCTAssertEqual(
      negative.left, zero.left, accuracy: eps,
      "editor: a negative offset clamps to 0 (left == the narrow case)")
    XCTAssertEqual(
      negative.width, zero.width, accuracy: eps,
      "editor: a negative offset clamps to 0 (width == the narrow case)")
    XCTAssertEqual(
      negative.left, gridLeftAtOrigin, accuracy: eps,
      "editor: left never moves left of gridLeft")
  }

  // MARK: - Case 11: exactly at the 90-column cap

  func testExactCapBoundaryHasNoOffsetAndBothKindsCoincide() {
    // At gridWidth == 90 * cellWidth the centering inset is 0, so the kinds coincide; paintOffsetX
    // grows continuously past the threshold, with no jump.
    let capWidth = maxGridCols * cellWidth
    XCTAssertEqual(
      expectedPaintOffsetX(gridWidth: capWidth), 0.0, accuracy: eps,
      "an exactly-cap-wide grid yields no centering offset")

    let editor = span(
      gridLeft: gridLeftAtOrigin, gridWidth: capWidth, paintOffsetX: 0, kind: .editor)
    let preview = span(
      gridLeft: gridLeftAtOrigin, gridWidth: capWidth, paintOffsetX: 0, kind: .preview)

    XCTAssertEqual(editor.left, preview.left, accuracy: eps, "at the cap: same left")
    XCTAssertEqual(editor.width, preview.width, accuracy: eps, "at the cap: same width")
    XCTAssertEqual(
      editor.left, gridLeftAtOrigin, accuracy: eps, "at the cap: left == gridLeft")
    XCTAssertEqual(
      editor.width, capWidth, accuracy: eps, "at the cap: width == gridWidth")
  }

  // MARK: - Case 12: universal invariants over every case

  func testUniversalInvariantsHoldForEveryCase() {
    // Each tuple: (gridLeft, gridWidth, paintOffsetX, description). Covers every case
    // above plus the degenerate and defensive inputs.
    let cases: [(Double, Double, Double, String)] = [
      (gridLeftAtOrigin, 600.0, 0.0, "narrow"),
      (gridLeftAtOrigin, 1400.0, 250.0, "wide"),
      (gridLeftSplit, 1400.0, 250.0, "wide right-hand split"),
      (0.0, 1400.0, 250.0, "wide at x == 0"),
      (gridLeftAtOrigin, maxGridCols * cellWidth, 0.0, "exactly at the cap"),
      (gridLeftAtOrigin, 0.0, 0.0, "zero-width grid"),
      (gridLeftAtOrigin, 0.0, 250.0, "zero-width grid with a stale offset"),
      (gridLeftAtOrigin, 100.0, 400.0, "offset larger than the grid"),
      (gridLeftAtOrigin, 600.0, -75.0, "negative offset"),
      (gridLeftAtOrigin, -50.0, 0.0, "negative grid width"),
    ]
    for (gridLeft, gridWidth, paintOffsetX, name) in cases {
      for kind in [PaneContentKind.editor, PaneContentKind.preview] {
        let s = span(
          gridLeft: gridLeft, gridWidth: gridWidth, paintOffsetX: paintOffsetX, kind: kind)
        assertInvariants(
          s, gridLeft: gridLeft, gridWidth: gridWidth,
          "\(kind) \(name) (gridLeft=\(gridLeft) gridWidth=\(gridWidth) "
            + "offset=\(paintOffsetX))")
      }
    }
  }

  // MARK: - Case 13: editor sweep across the cap — REGRESSION GUARD

  func testEditorSpanEqualsTheLegacyExpressionAcrossTheCapThreshold() {
    // Sweeps case 2's claim across the 90-cell cap. Expectations are `paintStatusbar`'s expression
    // written longhand (left = minX + p, width = maxX - left), so this is not a tautology.
    let capWidth = maxGridCols * cellWidth
    let widths: [Double] = [
      100.0,  // far below the cap
      600.0,  // below the cap
      capWidth - 1.0,  // just below
      capWidth,  // exactly at the cap
      capWidth + 1.0,  // just above
      800.0,  // above
      1000.0,
      1400.0,
      2000.0,
      3000.0,  // far above the cap
    ]

    // Sanity: the sweep really does straddle the threshold in both directions.
    XCTAssertTrue(
      widths.contains(where: { $0 < capWidth }) && widths.contains(capWidth)
        && widths.contains(where: { $0 > capWidth }),
      "the sweep must span below, at, and above the \(capWidth)px cap")

    for gridLeft in [gridLeftAtOrigin, gridLeftSplit] {
      for gridWidth in widths {
        let p = expectedPaintOffsetX(gridWidth: gridWidth)

        // The legacy expression, spelled out independently of the seam.
        let legacyLeft = gridLeft + p
        let legacyWidth = gridWidth - p

        let editor = span(
          gridLeft: gridLeft, gridWidth: gridWidth, paintOffsetX: p, kind: .editor)
        let desc = "gridLeft=\(gridLeft) gridWidth=\(gridWidth) paintOffsetX=\(p)"
        let guardNote =
          "EDITOR-PANE REGRESSION GUARD (\(desc)): the .editor branch must remain "
          + "ALGEBRAICALLY IDENTICAL to the pre-refactor code, "
          + "left = gridLeft + paintOffsetX and width = gridWidth - paintOffsetX. "
          + "It is NOT to be 'simplified' to match the .preview branch — dropping "
          + "paintOffsetX here would un-center every editor pane wider than "
          + "\(maxGridCols) cells. Only PREVIEW panes ignore the offset."

        XCTAssertEqual(
          editor.left, legacyLeft, accuracy: eps,
          "\(guardNote) Expected left \(legacyLeft), got \(editor.left).")
        XCTAssertEqual(
          editor.width, legacyWidth, accuracy: eps,
          "\(guardNote) Expected width \(legacyWidth), got \(editor.width).")

        // The legacy right edge was gRect.maxX by construction; it must stay there.
        XCTAssertEqual(
          editor.left + editor.width, gridLeft + gridWidth, accuracy: eps,
          "\(guardNote) The editor's right edge must stay at gridLeft + gridWidth "
            + "(\(gridLeft + gridWidth)), got \(editor.left + editor.width).")

        assertInvariants(editor, gridLeft: gridLeft, gridWidth: gridWidth, "editor \(desc)")
      }
    }
  }
}
