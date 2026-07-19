//
//  PaneOcclusionTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.PaneOcclusion`.
//
//  These exercise the preview-web-view OCCLUSION seam the coder adds in
//  `Sources/WolfKeyMap/PaneOcclusion.swift` (phase B of the occlusion plan).
//  Like `PaneGeometryTests.swift` / `EditorInsetTests.swift` they import ONLY
//  Foundation-level test machinery (no AppKit, no `WKWebView`, no live Neovim)
//  and stay in the headless `WolfKeyMap` module.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor)
//
//    public struct OccluderGeometry {
//        public let frame: Rect
//        public let isHidden: Bool
//        public let isAttached: Bool
//        public let alpha: Double
//        public let hasLoadedContent: Bool
//        public init(frame:isHidden:isAttached:alpha:hasLoadedContent:)
//    }
//
//    public enum PaneOcclusion {
//        public static func isFullyOccluded(gridRect: Rect, by occluder: OccluderGeometry?) -> Bool
//    }
//
//  `Rect` is the EXISTING type from `PaneTree.swift` (`Rect(x:y:w:h:)`, `Double`
//  fields, `y` is the TOP edge and grows DOWNWARD). It has no `maxX` / `maxY`
//  accessors today, so these tests construct every fixture from explicit
//  `x` / `y` / `w` / `h` and never assume such accessors exist.
//
//  `true` means "fully occluded — SAFE TO SKIP PAINTING". `false` means "PAINT".
//  ALL of the following must hold for `true`; any one failing yields `false`:
//
//    1. `occluder != nil`                            (web view does not exist yet)
//    2. `hasLoadedContent`                           (exists, nothing loaded yet)
//    3. `isAttached`                                 (detached from the view tree)
//    4. `!isHidden`                                  (hidden / hidden ancestor)
//    5. `alpha >= 1.0`                               (transparent)
//    6. `frame.w > 0 && frame.h > 0`                 (zero-size)
//    7. `gridRect.w > 0 && gridRect.h > 0`           (degenerate grid rect)
//    8. containment with TOLERANCE 0.0:
//         `frame.x <= gridRect.x`, `frame.y <= gridRect.y`,
//         `frame.x + frame.w >= gridRect.x + gridRect.w`,
//         `frame.y + frame.h >= gridRect.y + gridRect.h`
//
//  Zero tolerance is DELIBERATE, not an oversight: `syncPreviewWebViews`
//  assigns the web view's frame from the SAME `gridRect(ofPane:)` expression the
//  painter is handed, so the values are bit-identical when in sync, and when OUT
//  of sync (mid-live-resize, post-split, pre-close) containment fails and the
//  grid paints. `testOccluderShortByAHairOnMaxXEdge` PINS that tolerance — do not
//  "fix" it by introducing an epsilon.
//

import XCTest

@testable import WolfKeyMap

final class PaneOcclusionTests: XCTestCase {

  // MARK: - Fixture geometry
  //
  // A non-square grid rect at a non-zero origin, so x/y and w/h confusions are
  // detectable. Derived edges are named rather than recomputed inline.

  /// The grid rect's left edge (px).
  private let gridX: Double = 100.0
  /// The grid rect's top edge (px). `Rect.y` grows DOWNWARD (see `PaneTree.swift`).
  private let gridY: Double = 200.0
  /// The grid rect's width (px).
  private let gridW: Double = 400.0
  /// The grid rect's height (px).
  private let gridH: Double = 300.0

  /// The grid rect's right edge: `gridX + gridW`.
  private var gridMaxX: Double { gridX + gridW }
  /// The grid rect's bottom edge: `gridY + gridH`.
  private var gridMaxY: Double { gridY + gridH }

  /// The pane's grid rect — the rectangle the painter would fill.
  private var gridRect: Rect {
    Rect(x: gridX, y: gridY, w: gridW, h: gridH)
  }

  /// How far a "strictly larger" occluder overhangs the grid rect on every side (px).
  private let overhang: Double = 10.0

  /// How far a partially-covering occluder falls short of an edge (px). Large
  /// enough to be unmistakably a partial cover, unlike `hairGap`.
  private let shortfall: Double = 50.0

  /// The sub-pixel shortfall that pins tolerance 0.0 (px).
  private let hairGap: Double = 0.5

  // MARK: - Helpers

  /// Builds an occluder whose flags are ALL in the "occluding" state by default,
  /// so each test can toggle exactly one thing and attribute the result to it.
  private func occluder(
    frame: Rect,
    isHidden: Bool = false,
    isAttached: Bool = true,
    alpha: Double = 1.0,
    hasLoadedContent: Bool = true
  ) -> OccluderGeometry {
    OccluderGeometry(
      frame: frame,
      isHidden: isHidden,
      isAttached: isAttached,
      alpha: alpha,
      hasLoadedContent: hasLoadedContent
    )
  }

  /// An occluder frame EXACTLY equal to the grid rect — the real, common case
  /// (`syncPreviewWebViews` assigns the identical `gridRect(ofPane:)` value).
  private var exactFrame: Rect { gridRect }

  /// An occluder frame strictly larger than the grid rect on all four sides.
  private var largerFrame: Rect {
    Rect(
      x: gridX - overhang,
      y: gridY - overhang,
      w: gridW + 2 * overhang,
      h: gridH + 2 * overhang
    )
  }

  /// Asserts `isFullyOccluded` returns `want` for a case, with a describing message.
  private func assertOcclusion(
    gridRect rect: Rect,
    by candidate: OccluderGeometry?,
    want: Bool,
    _ desc: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    let got = PaneOcclusion.isFullyOccluded(gridRect: rect, by: candidate)
    XCTAssertEqual(
      got,
      want,
      "case \(desc): got \(got), want \(want)",
      file: file,
      line: line
    )
  }

  // MARK: - The occluding cases (the only two that may return `true`)

  func testExactContainmentIsOccluded() {
    // THE common case: the web view's frame is assigned from the identical
    // `gridRect(ofPane:)` expression, so the rects are bit-identical. Zero
    // tolerance must still treat equality as containment (<= / >=, not < / >).
    assertOcclusion(
      gridRect: gridRect,
      by: occluder(frame: exactFrame),
      want: true,
      "an exactly-equal, loaded, attached, visible, opaque occluder fully occludes"
    )
  }

  func testStrictlyLargerOccluderIsOccluded() {
    // A frame that overhangs on all four sides also fully covers the grid.
    assertOcclusion(
      gridRect: gridRect,
      by: occluder(frame: largerFrame),
      want: true,
      "a strictly larger containing occluder fully occludes"
    )
  }

  func testOpaqueAlphaAtExactlyOneIsOccluded() {
    // The alpha rule is `>= 1.0`; exactly 1.0 is opaque and DOES occlude. This is
    // the inclusive half of the boundary pinned by `testTransparentAlphaPaints`.
    assertOcclusion(
      gridRect: gridRect,
      by: occluder(frame: exactFrame, alpha: 1.0),
      want: true,
      "alpha exactly 1.0 is opaque and occludes"
    )
  }

  // MARK: - Rule 1: no web view yet

  func testNilOccluderPaints() {
    // `previewWebViews[win] == nil` — the documented "web view does not exist
    // yet" path in `reloadDirtyPreviews`. Nothing is covering the pane.
    assertOcclusion(
      gridRect: gridRect,
      by: nil,
      want: false,
      "a nil occluder (no web view yet) never occludes"
    )
  }

  // MARK: - Rules 2-5: the per-flag disqualifiers, one at a time
  //
  // Each of these uses an EXACTLY-CONTAINING frame, so geometry alone would say
  // "occluded" — only the toggled flag can be responsible for the `false`.

  func testFlagDisqualifiersPaintTable() {
    struct FlagCase {
      let name: String
      let candidate: OccluderGeometry
    }

    let cases: [FlagCase] = [
      FlagCase(
        name: "hasLoadedContent == false (exists but blank/white, never loaded)",
        candidate: occluder(frame: exactFrame, hasLoadedContent: false)
      ),
      FlagCase(
        name: "isAttached == false (detached from the view tree / no window)",
        candidate: occluder(frame: exactFrame, isAttached: false)
      ),
      FlagCase(
        name: "isHidden == true (hidden, or has a hidden ancestor)",
        candidate: occluder(frame: exactFrame, isHidden: true)
      ),
    ]

    for testCase in cases {
      assertOcclusion(
        gridRect: gridRect,
        by: testCase.candidate,
        want: false,
        testCase.name
      )
    }
  }

  func testTransparentAlphaPaints() {
    // Anything below 1.0 lets the grid show through and must still paint. 0.999
    // pins that the comparison is `>= 1.0` and NOT a rounded/epsilon compare.
    let alphas: [Double] = [0.0, 0.5, 0.999]

    for alpha in alphas {
      assertOcclusion(
        gridRect: gridRect,
        by: occluder(frame: exactFrame, alpha: alpha),
        want: false,
        "alpha \(alpha) is not fully opaque and must paint"
      )
    }
  }

  // MARK: - Rule 6: zero-size occluder frames

  func testZeroSizeOccluderFramePaints() {
    struct SizeCase {
      let name: String
      let frame: Rect
    }

    let cases: [SizeCase] = [
      SizeCase(
        name: "zero-width occluder frame",
        frame: Rect(x: gridX, y: gridY, w: 0.0, h: gridH)
      ),
      SizeCase(
        name: "zero-height occluder frame",
        frame: Rect(x: gridX, y: gridY, w: gridW, h: 0.0)
      ),
      SizeCase(
        name: "zero-by-zero occluder frame",
        frame: Rect(x: gridX, y: gridY, w: 0.0, h: 0.0)
      ),
    ]

    for testCase in cases {
      assertOcclusion(
        gridRect: gridRect,
        by: occluder(frame: testCase.frame),
        want: false,
        testCase.name
      )
    }
  }

  // MARK: - Rule 7: degenerate grid rect
  //
  // Conservative by design: a zero-area grid rect reports NOT occluded. There is
  // no paint work to save, and reporting `true` would be a claim about a
  // rectangle that does not exist.

  func testDegenerateGridRectPaints() {
    struct GridCase {
      let name: String
      let rect: Rect
    }

    let cases: [GridCase] = [
      GridCase(
        name: "zero-width gridRect with an otherwise-valid occluder",
        rect: Rect(x: gridX, y: gridY, w: 0.0, h: gridH)
      ),
      GridCase(
        name: "zero-height gridRect with an otherwise-valid occluder",
        rect: Rect(x: gridX, y: gridY, w: gridW, h: 0.0)
      ),
      GridCase(
        name: "zero-by-zero gridRect with an otherwise-valid occluder",
        rect: Rect(x: gridX, y: gridY, w: 0.0, h: 0.0)
      ),
    ]

    for testCase in cases {
      // The occluder is the LARGER frame, so it geometrically contains every one
      // of these degenerate rects — only rule 7 can produce the `false`.
      assertOcclusion(
        gridRect: testCase.rect,
        by: occluder(frame: largerFrame),
        want: false,
        testCase.name
      )
    }
  }

  // MARK: - Rule 8: partial coverage
  //
  // Named for the `Rect` EDGE that is uncovered rather than for a visual
  // direction, so the cases stay unambiguous under either y convention:
  //
  //   minX edge — the occluder starts to the RIGHT of `gridRect.x`
  //   maxX edge — the occluder ends to the LEFT of `gridRect.x + gridRect.w`
  //   minY edge — the occluder starts BELOW `gridRect.y` (larger y)
  //   maxY edge — the occluder ends ABOVE `gridRect.y + gridRect.h` (smaller y)

  func testPartialCoveragePaintsTable() {
    struct CoverageCase {
      let name: String
      let frame: Rect
    }

    let cases: [CoverageCase] = [
      CoverageCase(
        name: "uncovered at the minX edge (occluder.x > gridRect.x)",
        frame: Rect(
          x: gridX + shortfall,
          y: gridY,
          w: gridW - shortfall,
          h: gridH
        )
      ),
      CoverageCase(
        name: "uncovered at the maxX edge (occluder right edge < gridRect right edge)",
        frame: Rect(
          x: gridX,
          y: gridY,
          w: gridW - shortfall,
          h: gridH
        )
      ),
      CoverageCase(
        name: "uncovered at the minY edge (occluder.y > gridRect.y)",
        frame: Rect(
          x: gridX,
          y: gridY + shortfall,
          w: gridW,
          h: gridH - shortfall
        )
      ),
      CoverageCase(
        name: "uncovered at the maxY edge (occluder bottom edge < gridRect bottom edge)",
        frame: Rect(
          x: gridX,
          y: gridY,
          w: gridW,
          h: gridH - shortfall
        )
      ),
      CoverageCase(
        name: "inset on all four edges (a strictly smaller centered occluder)",
        frame: Rect(
          x: gridX + shortfall,
          y: gridY + shortfall,
          w: gridW - 2 * shortfall,
          h: gridH - 2 * shortfall
        )
      ),
    ]

    for testCase in cases {
      assertOcclusion(
        gridRect: gridRect,
        by: occluder(frame: testCase.frame),
        want: false,
        testCase.name
      )
    }
  }

  func testOccluderShortByAHairOnMaxXEdge() {
    // TOLERANCE PIN. The occluder covers all but the last HALF PIXEL of the grid
    // rect's right edge. That is `false` (paint). Any epsilon slack added to the
    // containment comparison would flip this and break the "geometry changes
    // restore painting immediately" guarantee that mid-live-resize relies on.
    let frame = Rect(x: gridX, y: gridY, w: gridW - hairGap, h: gridH)

    assertOcclusion(
      gridRect: gridRect,
      by: occluder(frame: frame),
      want: false,
      "occluder maxX short by \(hairGap)px must paint (tolerance is exactly 0.0)"
    )
  }

  func testNonIntersectingOccluderPaints() {
    // The occluder is entirely elsewhere — a different pane, or a stale frame
    // from before a split. No overlap at all.
    let frame = Rect(
      x: gridMaxX + shortfall,
      y: gridMaxY + shortfall,
      w: gridW,
      h: gridH
    )

    assertOcclusion(
      gridRect: gridRect,
      by: occluder(frame: frame),
      want: false,
      "an occluder with no intersection at all never occludes"
    )
  }

  // MARK: - Flag interaction: the rules AND, they do not short-circuit on geometry
  //
  // These are the regression guard against an implementation that returns `true`
  // as soon as containment holds. Every case below is FULLY CONTAINING (the
  // strictly-larger frame, so geometry is not marginal) with exactly ONE
  // disqualifying flag set — each must still paint.

  func testContainingOccluderWithASingleDisqualifyingFlagPaints() {
    struct InteractionCase {
      let name: String
      let candidate: OccluderGeometry
    }

    let cases: [InteractionCase] = [
      InteractionCase(
        name: "fully containing but not loaded",
        candidate: occluder(frame: largerFrame, hasLoadedContent: false)
      ),
      InteractionCase(
        name: "fully containing but detached",
        candidate: occluder(frame: largerFrame, isAttached: false)
      ),
      InteractionCase(
        name: "fully containing but hidden",
        candidate: occluder(frame: largerFrame, isHidden: true)
      ),
      InteractionCase(
        name: "fully containing but fully transparent",
        candidate: occluder(frame: largerFrame, alpha: 0.0)
      ),
      InteractionCase(
        name: "fully containing but semi-transparent",
        candidate: occluder(frame: largerFrame, alpha: 0.5)
      ),
    ]

    for testCase in cases {
      assertOcclusion(
        gridRect: gridRect,
        by: testCase.candidate,
        want: false,
        testCase.name
      )
    }

    // ...and the control: the SAME containing frame with every flag good DOES
    // occlude, proving the cases above fail on the flag and not on the geometry.
    assertOcclusion(
      gridRect: gridRect,
      by: occluder(frame: largerFrame),
      want: true,
      "control — the same containing frame with all flags good occludes"
    )
  }

  func testFlagsAloneDoNotOccludeWithoutContainment() {
    // The mirror image of the test above: every flag in the occluding state but
    // the geometry only partially covers. Flags must not rescue bad geometry.
    let partial = Rect(x: gridX, y: gridY, w: gridW - shortfall, h: gridH)

    assertOcclusion(
      gridRect: gridRect,
      by: occluder(frame: partial),
      want: false,
      "all flags good but only partial coverage must paint"
    )
  }
}
