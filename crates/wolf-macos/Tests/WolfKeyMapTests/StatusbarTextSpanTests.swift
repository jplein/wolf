//
//  StatusbarTextSpanTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.StatusbarTextSpan`.
//
//  These exercise the per-pane statusbar HORIZONTAL text-span seam the coder adds in
//  `Sources/WolfKeyMap/StatusbarTextSpan.swift` (plan §1). Like `PaneGeometryTests`,
//  `EditorInsetTests` and `StatusbarLayoutTests` (the VERTICAL sibling), they import
//  ONLY Foundation-level test machinery (no AppKit / CoreGraphics, no live window, no
//  Neovim) and stay in the headless `WolfKeyMap` module. Pane kind arrives as a plain
//  enum, so no Neovim state participates and the rest is rect arithmetic.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor, plan §1)
//
//    public enum PaneContentKind { case editor; case preview }
//
//    public struct StatusbarTextSpan {
//        public let left: Double
//        public let width: Double
//        public init(gridLeft: Double, gridWidth: Double,
//                    paintOffsetX: Double, kind: PaneContentKind)
//    }
//
//  Arithmetic (plan §1), with `w = max(0, gridWidth)` and
//  `offset = min(max(0, paintOffsetX), w)`:
//    .editor  -> left = gridLeft + offset,  width = max(0, gridWidth - offset)
//    .preview -> left = gridLeft,           width = w
//
//  The `.editor` branch reproduces today's expression EXACTLY, so editor panes are
//  bit-identical; only the clamp is new, and it is a no-op for every real input. The
//  `.preview` branch deliberately IGNORES `paintOffsetX` because `syncPreviewWebViews`
//  frames the preview `WKWebView` to the UNCENTERED `gridRect(ofPane:)` and the preview
//  CSS is flush (`body.wolf-markdown { margin: 0; padding: 0 }`).
//
//  ## WHAT THESE TESTS CANNOT PROVE (plan §5)
//
//  That the statusbar text origin and the LIVE `WKWebView` frame actually coincide on
//  screen is an integration property spanning `paintStatusbar`, `syncPreviewWebViews`
//  and the markdown CSS's zero left padding. No unit test can pin it — these tests pin
//  only the shared arithmetic both consumers derive from. The remainder rests on visual
//  review: open a preview in a window wider than 90 columns and confirm the statusbar
//  filename's left edge sits on the first heading's left edge. Note also that in a
//  vertical split each pane is typically narrower than 90 cells, so `paintOffsetX == 0`
//  and the two kinds coincide — the bug and the fix are only observable on WIDE panes.
//

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
  /// for a pane at x == 0 (plan §3).
  private let gridLeftAtOrigin: Double = 20.0

  /// A representative grid origin for a right-hand split (plan §5 case 6).
  private let gridLeftSplit: Double = 512.0

  // MARK: - Helpers

  /// Builds a span from the four inputs, in the plan's argument order.
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
  /// `max(0, (gridWidth - min(gridWidth, cap)) / 2)` (plan §3).
  private func expectedPaintOffsetX(gridWidth: Double) -> Double {
    let cap = maxGridCols * cellWidth
    return max(0.0, (gridWidth - min(gridWidth, cap)) / 2.0)
  }

  /// Asserts the universal invariants every span must satisfy regardless of kind
  /// (plan §5 case 12), mirroring `assertInvariants` in `StatusbarLayoutTests`.
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
    // The editor branch must reproduce today's expression exactly:
    //   textLeft  = gRect.minX + paintOffsetX
    //   textWidth = gRect.maxX - textLeft == gridWidth - paintOffsetX
    // The offset is the REAL one PaneGeometry derives for this width, not an arbitrary
    // number: (1400 - 720) / 2 == 340.
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

    // REGRESSION GUARD (plan §10, top risk): the editor's right edge must remain
    // gRect.maxX. If this fails, someone has "simplified" the .editor branch to also
    // drop paintOffsetX (or clamped the right edge). Editor panes must be
    // BIT-IDENTICAL to today's behavior — this fix is for PREVIEW panes only.
    XCTAssertEqual(
      s.left + s.width, gridLeftAtOrigin + gridWidth, accuracy: eps,
      "REGRESSION: editor textRight must stay at gridLeft + gridWidth. The .editor "
        + "branch must keep adding paintOffsetX to left and subtracting it from width; "
        + "do not 'simplify' it to match the .preview branch.")
  }

  // MARK: - Case 3: preview, narrow pane — the fix is a no-op below the cap

  func testPreviewNarrowPaneMatchesTheEditorCase() {
    // Below the cap there is no offset to ignore, so preview and editor coincide.
    let gridWidth: Double = 600.0
    let s = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: 0, kind: .preview)

    XCTAssertEqual(s.left, gridLeftAtOrigin, accuracy: eps, "preview narrow: left == gridLeft")
    XCTAssertEqual(s.width, gridWidth, accuracy: eps, "preview narrow: width == gridWidth")
  }

  // MARK: - Case 4: preview, wide pane — spans the whole (uncentered) grid rect

  func testPreviewWidePaneIgnoresThePaintOffset() {
    // The preview WKWebView is framed to the UNCENTERED gridRect, so the statusbar
    // text must span that same rect: both edges land on the webview frame.
    let gridWidth: Double = 1400.0
    let paintOffsetX = expectedPaintOffsetX(gridWidth: gridWidth)
    XCTAssertGreaterThan(
      paintOffsetX, 0.0,
      "this width must be past the cap, or there would be no offset to ignore")

    let s = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: paintOffsetX,
      kind: .preview)

    XCTAssertEqual(
      s.left, gridLeftAtOrigin, accuracy: eps,
      "preview wide: left == gridLeft, ignoring paintOffsetX (== web.frame.minX)")
    XCTAssertEqual(
      s.width, gridWidth, accuracy: eps,
      "preview wide: width == gridWidth (the full grid rect)")
    XCTAssertEqual(
      s.left + s.width, gridLeftAtOrigin + gridWidth, accuracy: eps,
      "preview wide: textRight == gridLeft + gridWidth == web.frame.maxX")
  }

  // MARK: - Case 5: the kind-difference invariant — THE BUG

  func testKindDifferenceIsExactlyThePaintOffsetOnWidePanes() {
    // THIS IS THE ASSERTION THAT MOST DIRECTLY ENCODES THE BUG BEING FIXED.
    let gridWidth: Double = 1400.0
    let paintOffsetX = expectedPaintOffsetX(gridWidth: gridWidth)
    XCTAssertGreaterThan(
      paintOffsetX, 0.0,
      "this width must be past the cap, or the two kinds would trivially coincide")

    let editor = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: paintOffsetX,
      kind: .editor)
    let preview = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: paintOffsetX,
      kind: .preview)

    XCTAssertEqual(
      editor.left - preview.left, paintOffsetX, accuracy: eps,
      "THE BUG: on a WIDE preview pane the statusbar text must NOT be indented by the "
        + "grid-centering offset (paintOffsetX == \(paintOffsetX)). The preview "
        + "WKWebView is framed to the UNCENTERED gridRect and the preview CSS is flush "
        + "(margin: 0; padding: 0), so the preview statusbar's left edge must sit at "
        + "gridRect.minX == web.frame.minX. Only EDITOR panes add paintOffsetX, because "
        + "only the editor GRID is painted at that centered offset. Got "
        + "editor.left == \(editor.left), preview.left == \(preview.left).")

    // Below the cap there is no offset, so the two kinds must be indistinguishable —
    // the fix changes nothing for narrow panes.
    let narrowWidth: Double = 600.0
    let narrowEditor = span(
      gridLeft: gridLeftAtOrigin, gridWidth: narrowWidth, paintOffsetX: 0, kind: .editor)
    let narrowPreview = span(
      gridLeft: gridLeftAtOrigin, gridWidth: narrowWidth, paintOffsetX: 0, kind: .preview)

    XCTAssertEqual(
      narrowEditor.left, narrowPreview.left, accuracy: eps,
      "narrow: both kinds share the same left (the fix is a no-op below the cap)")
    XCTAssertEqual(
      narrowEditor.width, narrowPreview.width, accuracy: eps,
      "narrow: both kinds share the same width (the fix is a no-op below the cap)")
  }

  // MARK: - Case 6: gridLeft is ABSOLUTE, never pane-local

  func testNonZeroGridLeftIsAbsoluteForBothKinds() {
    // `gridLeft` is the ABSOLUTE gRect.minX in the container view's coordinates, not a
    // pane-local inset: a right-hand split's span is simply shifted right. Each tuple:
    // (gridLeft, gridWidth); the offset is DERIVED from the width the way PaneGeometry
    // derives it, so the wide rows pin realistic inputs (1400 -> 340, not a made-up 250).
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

      let preview = span(
        gridLeft: gridLeft, gridWidth: gridWidth, paintOffsetX: paintOffsetX, kind: .preview)
      XCTAssertEqual(
        preview.left, gridLeft, accuracy: eps,
        "\(desc): preview left == gridLeft (absolute)")

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
    // `EditorInset` clamps a sub-`2*padding` pane's grid width to 0 (see
    // `EditorInsetTests.testNarrowPaneClampsGridWidthToZero`), while the grid's x
    // origin is a FIXED, UNCLAMPED offset — so gridLeft can sit beyond the pane's own
    // right edge. The span must still degenerate cleanly.
    // A 10px-wide pane at x == 100: EditorInset gives gridWidth == max(0, 10 - 40) == 0
    // and gridLeft == 100 + 20 == 120, which is past the pane's right edge (110).
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

    // The preview branch never reads the offset, so it is unaffected.
    let preview = span(
      gridLeft: gridLeftAtOrigin, gridWidth: gridWidth, paintOffsetX: paintOffsetX,
      kind: .preview)
    XCTAssertEqual(
      preview.left, gridLeftAtOrigin, accuracy: eps,
      "preview: unaffected by an over-large offset (left == gridLeft)")
    XCTAssertEqual(
      preview.width, gridWidth, accuracy: eps,
      "preview: unaffected by an over-large offset (width == gridWidth)")
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
    // At gridWidth == 90 * cellWidth the centering inset is max(0, 0/2) == 0, so the
    // two kinds coincide. paintOffsetX is CONTINUOUS across the threshold: the gap
    // widens from zero as the window grows, with no jump or flicker.
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
    // Case 2 pins the "bit-identical editor" claim at a single point. This sweeps it
    // across the whole 90-cell cap threshold: below, exactly at, and well above.
    //
    // The expected values below are the LITERAL pre-refactor expression from
    // `paintStatusbar`, written out longhand rather than reusing the seam's own
    // arithmetic, so this is an independent check and not a tautology:
    //
    //     let p         = geo.paintOffsetX
    //     let textLeft  = gRect.minX + p
    //     let textWidth = gRect.maxX - textLeft      // == gridWidth - p
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
