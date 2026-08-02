//
//  PreviewScrollSeamTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.PreviewScrollSeam`.
//
//  These exercise the pure anchor/threshold/JS-synthesis seam the coder adds in
//  `Sources/WolfKeyMap/PreviewScrollSeam.swift` (plan §5.1-§5.3). Like
//  `PreviewSeamTests` / `PreviewLinkSeamTests`, they import ONLY Foundation-level
//  test machinery (no AppKit, no WKWebView, no live Neovim) and stay in the headless
//  `WolfKeyMap` module. `PaneContainerView` / `PreviewScrollStateHandler` and the
//  actual on-screen scrolling are AppKit glue (compile + manual-smoke only); ALL the
//  arithmetic and every emitted JS string lives here and is unit-tested.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor)
//
//    public enum PreviewScrollSeam {
//        public struct CursorPos: Equatable, Hashable {
//            public let line: UInt32   // 1-based; 0 == unknown
//            public let col: UInt32    // 0-based CHARACTER column
//            public init(line: UInt32, col: UInt32)
//            public var isKnown: Bool  // line > 0
//        }
//        public struct ScrollState: Equatable {
//            public let anchorTop: Double?   // viewport-relative Y; nil = unresolved
//            public let scrollY: Double      // raw window.scrollY fallback
//            public init(anchorTop: Double?, scrollY: Double)
//        }
//        public struct RestoreRequest: Equatable {
//            public let cursor: CursorPos
//            public let targetY: Double?
//            public let fallbackScrollY: Double
//            public init(cursor: CursorPos, targetY: Double?, fallbackScrollY: Double)
//        }
//        public enum FollowDecision: Equatable {
//            case stay
//            case scroll(toViewportY: Double)
//        }
//        public enum CaptureDecision: Equatable {
//            case drop                    // source switched — caller ALSO clears the
//                                         // cached state and any pending restore
//            case keepPending             // a cache carrying NO position (nil, or the
//                                         // document-start seed) must not clobber an
//                                         // in-flight restore
//            case capture(RestoreRequest)
//        }
//
//        public static let markerColumnStride: UInt32      // 100
//        public static let minEdgeThreshold: Double        // 48
//        public static let edgeThresholdFraction: Double   // 0.15
//        public static let maxEdgeThresholdFraction: Double// 0.35
//        public static let restPositionFraction: Double    // 0.33
//        public static let maxAnchorScanLines: UInt32      // 200
//        public static let stateHandlerName: String        // "wolfPreviewScroll"
//
//        static func anchorColumn(_ col: UInt32) -> UInt32
//        static func anchorIds(line: UInt32, col: UInt32) -> [String]
//        static func edgeThreshold(viewportHeight: Double) -> Double
//        static func followDecision(anchorTop: Double, viewportHeight: Double) -> FollowDecision
//        static func winsNeedingScroll(
//            current: [Int64: CursorPos], applied: [Int64: CursorPos]) -> [Int64]
//        static func captureDecision(
//            previousLoad: PreviewSeam.PreviewLoad?,   // lastPreviewLoads[win] BEFORE
//                                                      // it is reassigned
//            currentLoad: PreviewSeam.PreviewLoad,
//            cursor: CursorPos,
//            cachedState: ScrollState?,
//            pendingRestore: RestoreRequest?
//        ) -> CaptureDecision
//        static func stateScriptJS(handlerName: String = stateHandlerName) -> String
//        static func followJS(line: UInt32, col: UInt32) -> String
//        static func restoreJS(_ request: RestoreRequest) -> String
//    }
//
//  ## WHY `captureDecision` IS IN THE SEAM AT ALL (plan §16)
//
//  Both shipping-blocker defects the reviewer found (§15 H1 and H2) lived in
//  `PaneContainerView`'s capture ORDERING, which plan §6.3 exempts from automated
//  testing. Fixing them in place would have shipped two untestable
//  regressions-in-waiting, so the decision itself moved into this pure seam and
//  `PaneContainerView` keeps only a mechanical `switch`. The three rules are ORDERED
//  and the ordering is load-bearing:
//
//   1. `.drop` when the source buffer switched (`previousLoad!.sourceBufnr !=
//      currentLoad.sourceBufnr`) — a source switch is a NAVIGATION, not an edit.
//      **H1**: clicking a plain `[see](other.md)` link with NO `#fragment` stashes
//      nothing in `pendingPreviewFragments`, so the restore branch ran with the
//      PREVIOUS file's cached scrollY and opened the new file thousands of pixels
//      into its middle — a regression against the pre-feature behavior, where
//      `loadHTMLString` reset scroll to 0.
//   2. `.keepPending` when a restore is in flight and the cache carries NO
//      position — either it is `nil`, or it is the document-start SEED
//      (`anchorTop == nil && scrollY == 0`). **H2**: the new document's
//      `.atDocumentStart` `post()` resolves nothing (`__wolfLine == 0`) and pushes
//      `(nil, 0)`, clobbering the good cache. If a second keystroke fires reload
//      N+1 before load N's `didFinish` (easily longer than the coalescer's 33ms on
//      a long document), that seed became the capture and the ORIGINAL
//      jump-to-top bug returned. The `nil` half of the rule (§16's clarification)
//      is NOT merely defensive — see
//      `testCaptureDecisionNilCacheWithAPendingRestoreKeepsIt`.
//   3. `.capture` otherwise.
//
//  Rule 3 is what stops the fix from OVER-applying: a REAL scroll position must
//  still overwrite an in-flight restore, or the restore would freeze at a stale
//  offset forever.
//
//  ## SINGLE SOURCE OF TRUTH FOR THE TUNING CONSTANTS (orchestrator decision D1)
//
//  The six tuning numbers (`48`, `0.15`, `0.35`, `0.33`, `100`, `200`) must exist in
//  exactly ONE place — the Swift constants above — and be INTERPOLATED into the
//  generated JS, never duplicated as literals in the JS string. The
//  `testStateScriptJSCarriesTheSwiftTuningConstants` test below asserts the generated
//  JS actually CONTAINS each constant's rendered form, so a future constant change
//  provably reaches the running JS instead of silently leaving the Swift tests green
//  while the shipped behavior is wrong.
//
//  ## FORMATTING ASSUMPTIONS THE CODER MUST MATCH (plan §5.1)
//
//   * `Double`s are rendered with `String(format: "%.2f", v)` — no locale argument,
//     so non-localized: `48` -> "48.00", `0.15` -> "0.15", `123.5` -> "123.50".
//     (See `jsDouble` below, which is the test's mirror of that rule.)
//   * `UInt32`s are rendered with `String(v)`: `100` -> "100".
//   * `followJS(line:col:)` emits `window.__wolfFollow&&window.__wolfFollow(7,200);`
//     — NO spaces around the argument comma (plan §5.1's literal spec).
//   * `restoreJS` emits `__wolfRestore(<line>,<col>,<targetY>,<fallback>,<0|1>)` —
//     again with no spaces around the commas, so the `hasTarget` flag is assertable
//     as the trailing `,0)` / `,1)`.
//

import XCTest

@testable import WolfKeyMap

final class PreviewScrollSeamTests: XCTestCase {

  /// The exact textual form the seam must interpolate a `Double` tuning constant or
  /// coordinate into the generated JS as (plan §5.1: `String(format: "%.2f", v)`,
  /// no locale argument => non-localized => always a `.` decimal separator).
  private func jsDouble(_ v: Double) -> String {
    String(format: "%.2f", v)
  }

  /// A viewport height tall enough that the 15% fraction (not the 48px floor and not
  /// the 35% cap) decides the edge threshold, so the follow-decision tests exercise
  /// the interesting branch.
  private let tallViewport: Double = 1000

  // MARK: - captureDecision fixtures

  /// The source buffer a preview window was already showing.
  private let sourceBufnr: Int64 = 7
  /// A DIFFERENT source buffer — what a cross-file link navigation binds the same
  /// preview window to.
  private let otherBufnr: Int64 = 9

  /// A known cursor, deliberately with a column past one marker stride so the
  /// carried-through assertion cannot be satisfied by a zero-valued default.
  private let knownCursor = PreviewScrollSeam.CursorPos(line: 12, col: 250)

  /// Exactly what the new document's `.atDocumentStart` seeding `post()` pushes:
  /// `__wolfLine` is still 0, so `resolve` returns null and `window.scrollY` is 0.
  /// This value must NEVER be allowed to overwrite an in-flight restore (H2).
  private let seedState = PreviewScrollSeam.ScrollState(anchorTop: nil, scrollY: 0)

  /// A real, live scroll state: the cursor's anchor resolved 320px down a viewport
  /// that is itself scrolled 1500px into the document.
  private let liveState = PreviewScrollSeam.ScrollState(anchorTop: 320, scrollY: 1500)

  /// A restore already captured by an earlier reload and not yet consumed by
  /// `previewDidFinishLoad`. Deliberately carries values that appear NOWHERE else,
  /// so a test can tell "the pending one survived" from "a new one was built".
  private let inFlightRestore = PreviewScrollSeam.RestoreRequest(
    cursor: PreviewScrollSeam.CursorPos(line: 4, col: 0),
    targetY: 777,
    fallbackScrollY: 888)

  /// The `(sourceBufnr, sourceTick)` a preview window's web view was last loaded at.
  private func load(bufnr: Int64, tick: UInt64) -> PreviewSeam.PreviewLoad {
    PreviewSeam.PreviewLoad(sourceBufnr: bufnr, sourceTick: tick)
  }

  /// The `RestoreRequest` inside a `.capture`, or `nil` (with a failure) otherwise.
  private func captured(
    _ decision: PreviewScrollSeam.CaptureDecision,
    file: StaticString = #filePath,
    line: UInt = #line
  ) -> PreviewScrollSeam.RestoreRequest? {
    guard case .capture(let request) = decision else {
      XCTFail("expected a .capture decision, got \(decision)", file: file, line: line)
      return nil
    }
    return request
  }

  // MARK: - the tuning constants are the documented values

  func testTuningConstantsAreTheDocumentedValues() {
    XCTAssertEqual(
      PreviewScrollSeam.markerColumnStride, 100,
      "markerColumnStride must mirror wolf_core::markdown::MARKER_COLUMN_STRIDE (100)")
    XCTAssertEqual(
      PreviewScrollSeam.minEdgeThreshold, 48, accuracy: 1e-9,
      "the 48px floor is two rows of base.css's 24px baseline grid")
    XCTAssertEqual(
      PreviewScrollSeam.edgeThresholdFraction, 0.15, accuracy: 1e-9,
      "the comfort band is 15% of the viewport height")
    XCTAssertEqual(
      PreviewScrollSeam.maxEdgeThresholdFraction, 0.35, accuracy: 1e-9,
      "the 35% cap keeps a SHORT pane from having no comfort band at all")
    XCTAssertEqual(
      PreviewScrollSeam.restPositionFraction, 0.33, accuracy: 1e-9,
      "a required scroll lands the anchor one third down (biased ABOVE center)")
    XCTAssertEqual(
      PreviewScrollSeam.maxAnchorScanLines, 200,
      "the nearest-preceding-line scan is bounded at 200 steps")
  }

  // MARK: - anchorColumn rounds DOWN to a multiple of the marker stride

  func testAnchorColumnRoundsDownToStride() {
    let cases: [(UInt32, UInt32)] = [
      (0, 0), (1, 0), (99, 0),
      (100, 100), (101, 100), (199, 100),
      (200, 200), (250, 200),
    ]
    for (input, expected) in cases {
      XCTAssertEqual(
        PreviewScrollSeam.anchorColumn(input), expected,
        "column \(input) must round DOWN to the marker multiple \(expected)")
    }
  }

  // MARK: - anchorIds: a column inside the first stride yields ONE id (deduped)

  func testAnchorIdsColumnZeroYieldsSingleId() {
    XCTAssertEqual(
      PreviewScrollSeam.anchorIds(line: 7, col: 0), ["wl-7-0"],
      "column 0 rounds to 0, so the exact-column id IS the line id — never emitted twice")
  }

  func testAnchorIdsSubStrideColumnYieldsSingleId() {
    XCTAssertEqual(
      PreviewScrollSeam.anchorIds(line: 7, col: 99), ["wl-7-0"],
      "a column below the stride rounds to 0, so the two candidates dedupe to one")
  }

  // MARK: - anchorIds: exact-column id FIRST, then the line-0 fallback

  func testAnchorIdsOrdersExactColumnBeforeLineFallback() {
    XCTAssertEqual(
      PreviewScrollSeam.anchorIds(line: 7, col: 250), ["wl-7-200", "wl-7-0"],
      "the rounded exact-column anchor is preferred, with the line's col-0 marker next")
    XCTAssertEqual(
      PreviewScrollSeam.anchorIds(line: 7, col: 100), ["wl-7-100", "wl-7-0"],
      "a column exactly ON a stride boundary still falls back to the line's col-0 marker")
  }

  // MARK: - anchorIds: an UNKNOWN cursor (line 0) yields no candidates at all

  func testAnchorIdsUnknownLineYieldsNoCandidates() {
    XCTAssertEqual(
      PreviewScrollSeam.anchorIds(line: 0, col: 5), [String](),
      "line 0 means the cursor was never reported — there is nothing to scroll to")
  }

  // MARK: - CursorPos.isKnown gates on a 1-based line

  func testCursorPosIsKnown() {
    XCTAssertFalse(
      PreviewScrollSeam.CursorPos(line: 0, col: 0).isKnown,
      "line 0 is the `cursor unknown` sentinel (no wolf_markdown_cursor yet)")
    XCTAssertFalse(
      PreviewScrollSeam.CursorPos(line: 0, col: 250).isKnown,
      "a non-zero column cannot rescue an unknown line")
    XCTAssertTrue(
      PreviewScrollSeam.CursorPos(line: 1, col: 0).isKnown,
      "line 1 is the FIRST real line (the marker scheme's L is 1-based)")
  }

  // MARK: - edgeThreshold: the fraction wins on a tall pane

  func testEdgeThresholdFractionWinsOnTallViewport() {
    XCTAssertEqual(
      PreviewScrollSeam.edgeThreshold(viewportHeight: 1000), 150, accuracy: 1e-9,
      "1000 * 0.15 = 150 is above the 48px floor and below the 350 cap")
  }

  // MARK: - edgeThreshold: the 48px floor wins on a mid-height pane

  func testEdgeThresholdFloorWinsOnMidViewport() {
    XCTAssertEqual(
      PreviewScrollSeam.edgeThreshold(viewportHeight: 200), 48, accuracy: 1e-9,
      "200 * 0.15 = 30 is below the 48px floor, and 48 is below the 70 cap")
  }

  // MARK: - edgeThreshold: the 35% cap wins on a SHORT pane

  func testEdgeThresholdCapWinsOnShortViewport() {
    XCTAssertEqual(
      PreviewScrollSeam.edgeThreshold(viewportHeight: 100), 35, accuracy: 1e-9,
      "on a 100px pane the 48px floor would leave a 4px comfort band, so the 35% cap wins")
  }

  // MARK: - edgeThreshold: a zero-height viewport yields zero

  func testEdgeThresholdZeroViewport() {
    XCTAssertEqual(
      PreviewScrollSeam.edgeThreshold(viewportHeight: 0), 0, accuracy: 1e-9,
      "a zero-height viewport has no comfort band (the cap is 0 and the result floors at 0)")
  }

  // MARK: - edgeThreshold invariant: 0 <= t <= vh * maxEdgeThresholdFraction

  func testEdgeThresholdStaysWithinTheCapForASpreadOfHeights() {
    for vh: Double in [0, 1, 50, 100, 137, 200, 480, 1000, 2400] {
      let t = PreviewScrollSeam.edgeThreshold(viewportHeight: vh)
      XCTAssertGreaterThanOrEqual(t, 0, "the threshold must never be negative (vh=\(vh))")
      XCTAssertLessThanOrEqual(
        t, vh * PreviewScrollSeam.maxEdgeThresholdFraction + 1e-9,
        "the threshold must never exceed the \(PreviewScrollSeam.maxEdgeThresholdFraction) "
          + "cap (vh=\(vh))")
    }
  }

  // MARK: - followDecision: an anchor comfortably mid-viewport does NOT scroll

  func testFollowDecisionStaysWhenAnchorIsMidViewport() {
    XCTAssertEqual(
      PreviewScrollSeam.followDecision(anchorTop: 500, viewportHeight: tallViewport), .stay,
      "an anchor in the comfort band must NOT scroll — otherwise the preview twitches "
        + "on every keystroke")
  }

  // MARK: - followDecision: an anchor ABOVE the viewport scrolls to the rest position

  func testFollowDecisionScrollsWhenAnchorIsAboveTheViewport() {
    let expected = tallViewport * PreviewScrollSeam.restPositionFraction
    XCTAssertEqual(
      PreviewScrollSeam.followDecision(anchorTop: -50, viewportHeight: tallViewport),
      .scroll(toViewportY: expected),
      "an off-screen-above anchor scrolls back so it lands one third down")
  }

  // MARK: - followDecision: an anchor BELOW the viewport scrolls to the rest position

  func testFollowDecisionScrollsWhenAnchorIsBelowTheViewport() {
    let expected = tallViewport * PreviewScrollSeam.restPositionFraction
    XCTAssertEqual(
      PreviewScrollSeam.followDecision(anchorTop: 1200, viewportHeight: tallViewport),
      .scroll(toViewportY: expected),
      "an off-screen-below anchor scrolls so it lands one third down")
  }

  // MARK: - followDecision: the boundaries are INCLUSIVE (pins the convention)

  func testFollowDecisionBoundariesAreInclusive() {
    let t = PreviewScrollSeam.edgeThreshold(viewportHeight: tallViewport)
    XCTAssertEqual(
      PreviewScrollSeam.followDecision(anchorTop: t, viewportHeight: tallViewport), .stay,
      "an anchor sitting EXACTLY on the top threshold is inside the band (inclusive)")
    XCTAssertEqual(
      PreviewScrollSeam.followDecision(anchorTop: tallViewport - t, viewportHeight: tallViewport),
      .stay,
      "an anchor sitting EXACTLY on the bottom threshold is inside the band (inclusive)")
  }

  // MARK: - followDecision: just OUTSIDE the top threshold scrolls

  func testFollowDecisionJustAboveTheTopThresholdScrolls() {
    let t = PreviewScrollSeam.edgeThreshold(viewportHeight: tallViewport)
    let expected = tallViewport * PreviewScrollSeam.restPositionFraction
    XCTAssertEqual(
      PreviewScrollSeam.followDecision(anchorTop: t - 1, viewportHeight: tallViewport),
      .scroll(toViewportY: expected),
      "one pixel past the top threshold leaves the comfort band and must scroll")
  }

  // MARK: - winsNeedingScroll: a first bind (no `applied` entry) is returned

  func testWinsNeedingScrollFirstBind() {
    let current: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 3, col: 0)
    ]
    XCTAssertEqual(
      Set(PreviewScrollSeam.winsNeedingScroll(current: current, applied: [:])),
      Set<Int64>([10]),
      "a preview win with no applied cursor yet must be followed on first bind")
  }

  // MARK: - winsNeedingScroll: an unchanged cursor is NOT returned

  func testWinsNeedingScrollUnchangedCursorIsNoOp() {
    let same: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 3, col: 100)
    ]
    XCTAssertEqual(
      Set(PreviewScrollSeam.winsNeedingScroll(current: same, applied: same)),
      Set<Int64>(),
      "an unchanged cursor must NOT re-evaluate JS (that is the anti-twitch gate)")
  }

  // MARK: - winsNeedingScroll: a changed LINE is returned

  func testWinsNeedingScrollChangedLine() {
    let applied: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 3, col: 0)
    ]
    let current: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 4, col: 0)
    ]
    XCTAssertEqual(
      Set(PreviewScrollSeam.winsNeedingScroll(current: current, applied: applied)),
      Set<Int64>([10]),
      "moving to another hard line changes the anchor and must be followed")
  }

  // MARK: - winsNeedingScroll: a changed COLUMN on the SAME line is returned

  func testWinsNeedingScrollChangedColumnSameLine() {
    let applied: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 3, col: 0)
    ]
    let current: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 3, col: 250)
    ]
    XCTAssertEqual(
      Set(PreviewScrollSeam.winsNeedingScroll(current: current, applied: applied)),
      Set<Int64>([10]),
      "a column move can cross a marker stride, so the gate keys on (line, col)")
  }

  // MARK: - winsNeedingScroll: wins are judged independently

  func testWinsNeedingScrollJudgesWinsIndependently() {
    let applied: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 3, col: 0),
      11: PreviewScrollSeam.CursorPos(line: 9, col: 0),
    ]
    let current: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 3, col: 0),
      11: PreviewScrollSeam.CursorPos(line: 10, col: 0),
    ]
    XCTAssertEqual(
      Set(PreviewScrollSeam.winsNeedingScroll(current: current, applied: applied)),
      Set<Int64>([11]),
      "only the win whose cursor changed is followed; the unchanged win is left alone")
  }

  // MARK: - winsNeedingScroll: a win present only in `applied` is never returned

  func testWinsNeedingScrollDroppedWinNotReturned() {
    let applied: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 3, col: 0),
      99: PreviewScrollSeam.CursorPos(line: 7, col: 0),
    ]
    let current: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 3, col: 0)
    ]
    XCTAssertEqual(
      Set(PreviewScrollSeam.winsNeedingScroll(current: current, applied: applied)),
      Set<Int64>(),
      "only `current` is iterated; a closed preview win is never returned")
  }

  // MARK: - winsNeedingScroll: an empty `current` scrolls nothing

  func testWinsNeedingScrollEmptyCurrent() {
    let applied: [Int64: PreviewScrollSeam.CursorPos] = [
      10: PreviewScrollSeam.CursorPos(line: 3, col: 0)
    ]
    XCTAssertEqual(
      Set(PreviewScrollSeam.winsNeedingScroll(current: [:], applied: applied)),
      Set<Int64>(),
      "no current previews means nothing to follow regardless of prior cursors")
  }

  // MARK: - THE design-question-6 regression guard: the two gates are independent

  func testCursorOnlyChangeDoesNotArmReload() {
    // The SAME preview window, with an unchanged (sourceBufnr, sourceTick) load but a
    // MOVED cursor. `PreviewSeam.PreviewLoad` must stay exactly (sourceBufnr,
    // sourceTick) — no cursor fields — so a cursor-only change is STRUCTURALLY
    // incapable of arming the HTML-reload coalescer, while still arming the (cheap)
    // scroll coalescer.
    let win: Int64 = 10
    let loads: [Int64: PreviewSeam.PreviewLoad] = [
      win: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
    ]
    XCTAssertEqual(
      Set(PreviewSeam.winsNeedingReload(current: loads, loaded: loads)),
      Set<Int64>(),
      "a cursor move must NEVER trigger a full loadHTMLString reload (that is the "
        + "jump-to-top bug this feature exists to fix)")

    let appliedCursors: [Int64: PreviewScrollSeam.CursorPos] = [
      win: PreviewScrollSeam.CursorPos(line: 3, col: 0)
    ]
    let currentCursors: [Int64: PreviewScrollSeam.CursorPos] = [
      win: PreviewScrollSeam.CursorPos(line: 42, col: 0)
    ]
    XCTAssertEqual(
      Set(PreviewScrollSeam.winsNeedingScroll(current: currentCursors, applied: appliedCursors)),
      Set<Int64>([win]),
      "the same cursor move DOES arm the scroll gate — the two gates are independent")
  }

  // MARK: - T-5 / H1: a SOURCE SWITCH is a navigation, so the restore is DROPPED

  func testCaptureDecisionDropsWhenTheSourceBufferSwitched() {
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: load(bufnr: sourceBufnr, tick: 5),
      currentLoad: load(bufnr: otherBufnr, tick: 5),
      cursor: PreviewScrollSeam.CursorPos(line: 0, col: 0),
      cachedState: liveState,
      pendingRestore: nil)
    XCTAssertEqual(
      decision, .drop,
      "a changed sourceBufnr means the window navigated to ANOTHER FILE, so the "
        + "previous file's scroll offset must not be restored into it — otherwise "
        + "clicking [appendix](appendix.md) from a guide.md scrolled to y=3000 opens "
        + "appendix.md 3000px into its middle (H1)")
  }

  /// The tick is IRRELEVANT to rule 1: two freshly opened files can legitimately
  /// share a `changedtick`, so only the bufnr may decide a navigation.
  func testCaptureDecisionDropsOnASourceSwitchAtAnEqualTick() {
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: load(bufnr: sourceBufnr, tick: 2),
      currentLoad: load(bufnr: otherBufnr, tick: 2),
      cursor: knownCursor,
      cachedState: liveState,
      pendingRestore: nil)
    XCTAssertEqual(
      decision, .drop,
      "two fresh buffers share a changedtick, so a tick-equal source switch is still "
        + "a navigation")
  }

  // MARK: - T-5: rule 1 WINS over rule 2 (the ordering is load-bearing)

  func testCaptureDecisionSourceSwitchWinsOverAPendingRestore() {
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: load(bufnr: sourceBufnr, tick: 5),
      currentLoad: load(bufnr: otherBufnr, tick: 6),
      cursor: knownCursor,
      cachedState: seedState,
      pendingRestore: inFlightRestore)
    XCTAssertEqual(
      decision, .drop,
      "the rules are ORDERED: a source switch must drop even when a restore is in "
        + "flight and the cache is the document-start seed. Keeping the pending "
        + "restore here would apply the OLD file's offset to the NEW file — exactly "
        + "the H1 regression, reintroduced through rule 2")
  }

  // MARK: - T-5: a FIRST BIND is not a navigation

  func testCaptureDecisionFirstBindIsNotANavigation() {
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: nil,
      currentLoad: load(bufnr: sourceBufnr, tick: 1),
      cursor: knownCursor,
      cachedState: liveState,
      pendingRestore: nil)
    XCTAssertNotEqual(
      decision, .drop,
      "rule 1 requires a PREVIOUS load to compare against; `previousLoad == nil` is a "
        + "first bind, not a switch away from something")
    XCTAssertEqual(
      captured(decision),
      PreviewScrollSeam.RestoreRequest(
        cursor: knownCursor, targetY: liveState.anchorTop, fallbackScrollY: liveState.scrollY),
      "a first bind with a real cached state captures it like any other reload")
  }

  // MARK: - T-5 / H2: the document-start SEED must not clobber an in-flight restore

  func testCaptureDecisionKeepsAnInFlightRestoreAgainstTheDocumentStartSeed() {
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: load(bufnr: sourceBufnr, tick: 5),
      currentLoad: load(bufnr: sourceBufnr, tick: 6),
      cursor: knownCursor,
      cachedState: seedState,
      pendingRestore: inFlightRestore)
    XCTAssertEqual(
      decision, .keepPending,
      "reload N+1 can be armed before load N's didFinish arrives, and by then the new "
        + "document's atDocumentStart post() has already reset the cache to (nil, 0). "
        + "Capturing that seed would overwrite the GOOD pending restore with a "
        + "no-target one and the page would stay at the top — the original "
        + "jump-to-top bug, returned (H2)")
  }

  // MARK: - T-5: the seed with NOTHING in flight has nothing to protect

  func testCaptureDecisionCapturesTheSeedWhenNoRestoreIsInFlight() {
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: load(bufnr: sourceBufnr, tick: 5),
      currentLoad: load(bufnr: sourceBufnr, tick: 6),
      cursor: knownCursor,
      cachedState: seedState,
      pendingRestore: nil)
    XCTAssertEqual(
      captured(decision),
      PreviewScrollSeam.RestoreRequest(cursor: knownCursor, targetY: nil, fallbackScrollY: 0),
      "rule 2 exists ONLY to protect an in-flight restore; with nothing pending the "
        + "seed is simply the current truth and is captured normally")
  }

  // MARK: - T-5: a REAL scroll position still overwrites an in-flight restore

  /// The boundary that stops the H2 fix from OVER-applying. If `.keepPending` were
  /// returned for any non-nil `pendingRestore`, a preview whose `didFinish` was slow
  /// would freeze on the first captured offset and ignore every subsequent scroll.
  func testCaptureDecisionARealScrollPositionOverwritesAnInFlightRestore() {
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: load(bufnr: sourceBufnr, tick: 5),
      currentLoad: load(bufnr: sourceBufnr, tick: 6),
      cursor: knownCursor,
      cachedState: liveState,
      pendingRestore: inFlightRestore)
    XCTAssertEqual(
      captured(decision),
      PreviewScrollSeam.RestoreRequest(
        cursor: knownCursor, targetY: liveState.anchorTop, fallbackScrollY: liveState.scrollY),
      "rule 2 keys on the cache being the SEED, not on a restore merely being in "
        + "flight: a live cache is fresher than the pending capture and must win, or "
        + "the restore would freeze at a stale offset")
    XCTAssertNotEqual(
      decision, .keepPending,
      "over-applying the H2 fix to every pending restore would trade one freeze for "
        + "another")
  }

  // MARK: - T-5: the ordinary edit — same buffer, bumped tick

  func testCaptureDecisionCapturesAnOrdinaryEdit() {
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: load(bufnr: sourceBufnr, tick: 5),
      currentLoad: load(bufnr: sourceBufnr, tick: 6),
      cursor: knownCursor,
      cachedState: liveState,
      pendingRestore: nil)
    XCTAssertEqual(
      captured(decision),
      PreviewScrollSeam.RestoreRequest(
        cursor: knownCursor, targetY: liveState.anchorTop, fallbackScrollY: liveState.scrollY),
      "a keystroke in the SAME buffer is the whole reason this feature exists: the "
        + "pre-reload anchor offset is captured so the edited region stays stationary")
  }

  // MARK: - T-5: no cached state yet — an empty request, never a crash

  func testCaptureDecisionWithNoCachedStateCapturesAnEmptyRequest() {
    // Reachable before the page's first post() has ever arrived, and again right
    // after a `.drop` cleared the cache. This test pins only the `pendingRestore ==
    // nil` half; the OTHER half (`cachedState == nil` TOGETHER with a non-nil
    // `pendingRestore`) is rule 2's territory and is pinned by
    // `testCaptureDecisionNilCacheWithAPendingRestoreKeepsIt` below.
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: load(bufnr: sourceBufnr, tick: 5),
      currentLoad: load(bufnr: sourceBufnr, tick: 6),
      cursor: knownCursor,
      cachedState: nil,
      pendingRestore: nil)
    guard let request = captured(decision) else { return }
    XCTAssertEqual(
      request,
      PreviewScrollSeam.RestoreRequest(cursor: knownCursor, targetY: nil, fallbackScrollY: 0),
      "with no cached state there is no anchor to aim at (targetY nil => the JS's "
        + "hasTarget flag is 0) and the raw scrollY fallback is 0")
    XCTAssertNil(
      request.targetY,
      "an absent cache must NOT be papered over with a 0 target — a 0 target would "
        + "pin the anchor to the very top of the viewport")
  }

  // MARK: - T-7 / §16 clarification: a nil cache must not clobber an in-flight restore

  /// The second sub-case of rule 2, and the one an earlier revision of this file
  /// wrongly called unreachable.
  ///
  /// A `nil` `cachedState` carries NO information about scroll position — exactly
  /// like the document-start seed — so it must not overwrite a good in-flight
  /// restore either. Under a rule 2 that keyed LITERALLY on the seed shape
  /// (`anchorTop == nil && scrollY == 0`), a `nil` state is not the seed, so this
  /// combination would fall through to rule 3 and `.capture` an empty request,
  /// reintroducing H2 (the page would sit at the top with `hasTarget == 0`).
  ///
  /// It is REACHABLE in production, and precisely BECAUSE of the `.drop` branch this
  /// same feature added: `.drop` nils `previewScrollStates[win]`, so every reload for
  /// that window until the new document's first `post()` arrives sees a `nil` cache —
  /// while the first of those reloads has already installed its own capture as
  /// `pendingPreviewRestores[win]`. The very next reload in that window (the
  /// coalescer serializes to one per ~33ms, and does NOT wait for `didFinish`) hits
  /// `cachedState == nil` together with a non-nil `pendingRestore`.
  func testCaptureDecisionNilCacheWithAPendingRestoreKeepsIt() {
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: load(bufnr: sourceBufnr, tick: 5),
      currentLoad: load(bufnr: sourceBufnr, tick: 6),
      cursor: knownCursor,
      cachedState: nil,
      pendingRestore: inFlightRestore)
    XCTAssertEqual(
      decision, .keepPending,
      "a nil cache carries no position at all, so it must be treated exactly like the "
        + "document-start seed: the in-flight restore is the better capture and must "
        + "survive. Capturing here would replace it with a target-less request and "
        + "the preview would jump to the top — H2, through the door `.drop` opened. "
        + "Do NOT `simplify` rule 2 back to the literal seed check")
  }

  // MARK: - T-5: the built request carries the PASSED cursor through unchanged

  func testCaptureDecisionCarriesTheCursorThrough() {
    let cursor = PreviewScrollSeam.CursorPos(line: 4321, col: 199)
    let decision = PreviewScrollSeam.captureDecision(
      previousLoad: load(bufnr: sourceBufnr, tick: 5),
      currentLoad: load(bufnr: sourceBufnr, tick: 6),
      cursor: cursor,
      cachedState: liveState,
      pendingRestore: nil)
    XCTAssertEqual(
      captured(decision)?.cursor, cursor,
      "the capture must carry the cursor VERBATIM — restoreJS passes the raw column "
        + "on to the JS, which does its own rounding to a marker stride, so any "
        + "rounding or defaulting here would silently disagree with the page")
  }

  // MARK: - T-5: the seed needs BOTH conditions, not either one

  func testCaptureDecisionSeedRequiresBothAnUnresolvedAnchorAndZeroScrollY() {
    // An unresolvable anchor partway down a scrolled document: the cursor's line was
    // deleted, but scrollY 1500 is a REAL user-visible position worth restoring.
    let unresolvedButScrolled = PreviewScrollSeam.ScrollState(anchorTop: nil, scrollY: 1500)
    XCTAssertEqual(
      captured(
        PreviewScrollSeam.captureDecision(
          previousLoad: load(bufnr: sourceBufnr, tick: 5),
          currentLoad: load(bufnr: sourceBufnr, tick: 6),
          cursor: knownCursor,
          cachedState: unresolvedButScrolled,
          pendingRestore: inFlightRestore)),
      PreviewScrollSeam.RestoreRequest(cursor: knownCursor, targetY: nil, fallbackScrollY: 1500),
      "anchorTop == nil ALONE is not the document-start seed — a non-zero scrollY "
        + "proves the page really is scrolled, so this must still be captured")

    // The mirror case: an anchor resolved at the very top of an unscrolled document.
    let resolvedAtTop = PreviewScrollSeam.ScrollState(anchorTop: 0, scrollY: 0)
    XCTAssertEqual(
      captured(
        PreviewScrollSeam.captureDecision(
          previousLoad: load(bufnr: sourceBufnr, tick: 5),
          currentLoad: load(bufnr: sourceBufnr, tick: 6),
          cursor: knownCursor,
          cachedState: resolvedAtTop,
          pendingRestore: inFlightRestore)),
      PreviewScrollSeam.RestoreRequest(cursor: knownCursor, targetY: 0, fallbackScrollY: 0),
      "scrollY == 0 ALONE is not the seed either — the seed post() cannot resolve an "
        + "anchor at all (__wolfLine is still 0), so a NON-nil anchorTop proves the "
        + "state came from a real follow or scroll event")
  }

  // MARK: - the state handler name is distinct from the click-focus handler name

  func testStateHandlerNameIsDistinct() {
    XCTAssertEqual(
      PreviewScrollSeam.stateHandlerName, "wolfPreviewScroll",
      "the scroll-state message handler must be named `wolfPreviewScroll`")
    XCTAssertNotEqual(
      PreviewScrollSeam.stateHandlerName, PreviewSeam.clickFocusHandlerName,
      "both handlers are registered on the SAME WKUserContentController, so their "
        + "names must differ")
  }

  // MARK: - followJS calls __wolfFollow with the raw (line, col) scalars

  func testFollowJSShape() {
    let js = PreviewScrollSeam.followJS(line: 7, col: 200)
    XCTAssertTrue(
      js.contains("__wolfFollow"),
      "the follow JS must call the injected `__wolfFollow` entry point: \(js)")
    XCTAssertTrue(
      js.contains("(7,200)"),
      "the follow JS must pass the RAW 1-based line and 0-based CHARACTER column as "
        + "two bare integers with no spaces: \(js)")
    XCTAssertFalse(
      js.contains("7.00"),
      "line/col are UInt32 and must NOT be rendered through the Double formatter: \(js)")
  }

  // MARK: - followJS is a no-op guard when the script has not been injected

  func testFollowJSGuardsOnTheEntryPointExisting() {
    let js = PreviewScrollSeam.followJS(line: 1, col: 0)
    XCTAssertTrue(
      js.contains("&&"),
      "the follow JS must short-circuit when `window.__wolfFollow` is absent (a web "
        + "view whose document-start script has not run yet): \(js)")
  }

  // MARK: - restoreJS with a target carries the target and the hasTarget flag 1

  func testRestoreJSWithTargetCarriesTargetAndFlag() {
    let request = PreviewScrollSeam.RestoreRequest(
      cursor: PreviewScrollSeam.CursorPos(line: 12, col: 250),
      targetY: 123.5,
      fallbackScrollY: 40)
    let js = PreviewScrollSeam.restoreJS(request)
    XCTAssertTrue(
      js.contains("__wolfRestore"),
      "the restore JS must call the injected `__wolfRestore` entry point: \(js)")
    XCTAssertTrue(
      js.contains("12"),
      "the restore JS must carry the cursor LINE: \(js)")
    XCTAssertTrue(
      js.contains("250"),
      "the restore JS must carry the RAW cursor column (the JS rounds it down): \(js)")
    XCTAssertTrue(
      js.contains("123.50"),
      "the captured pre-reload anchor offset must be interpolated as %.2f: \(js)")
    XCTAssertTrue(
      js.contains(",1)"),
      "a present targetY must set the trailing hasTarget flag to 1: \(js)")
    XCTAssertFalse(
      js.contains(",0)"),
      "a present targetY must NOT emit the no-target flag: \(js)")
  }

  // MARK: - restoreJS with NO target still emits a call, with the flag 0

  func testRestoreJSWithoutTargetStillEmitsACallWithFlagZero() {
    let request = PreviewScrollSeam.RestoreRequest(
      cursor: PreviewScrollSeam.CursorPos(line: 12, col: 0),
      targetY: nil,
      fallbackScrollY: 55)
    let js = PreviewScrollSeam.restoreJS(request)
    XCTAssertTrue(
      js.contains("__wolfRestore"),
      "an unresolved anchor must STILL emit a restore call (the raw scrollY fallback "
        + "is the whole point): \(js)")
    XCTAssertTrue(
      js.contains(jsDouble(55)),
      "the restore JS must carry the fallback window.scrollY: \(js)")
    XCTAssertTrue(
      js.contains(",0)"),
      "an absent targetY must set the trailing hasTarget flag to 0: \(js)")
    XCTAssertFalse(
      js.contains(",1)"),
      "an absent targetY must NOT emit the has-target flag: \(js)")
  }

  // MARK: - restoreJS re-applies late, guarded by the user-scroll flag

  func testRestoreJSReAppliesLateUnderTheUserScrollGuard() {
    let request = PreviewScrollSeam.RestoreRequest(
      cursor: PreviewScrollSeam.CursorPos(line: 3, col: 0),
      targetY: 10,
      fallbackScrollY: 0)
    let js = PreviewScrollSeam.restoreJS(request)
    XCTAssertTrue(
      js.contains("requestAnimationFrame"),
      "didFinish can fire before layout settles, so the restore must re-apply on the "
        + "next frame: \(js)")
    XCTAssertTrue(
      js.contains("__wolfUserScrolled"),
      "the late re-applies must be guarded so they never yank a user who scrolled in "
        + "the interim: \(js)")
    XCTAssertTrue(
      js.contains("fonts"),
      "the inlined base64 @font-face faces settle layout after didFinish, so the "
        + "restore must also re-apply on document.fonts.ready: \(js)")
  }

  // MARK: - stateScriptJS: the injected document-start script's shape

  func testStateScriptJSShape() {
    let js = PreviewScrollSeam.stateScriptJS()
    XCTAssertTrue(
      js.contains(PreviewScrollSeam.stateHandlerName),
      "the script must post to the `\(PreviewScrollSeam.stateHandlerName)` message handler")
    XCTAssertTrue(
      js.contains("getElementById"),
      "anchor resolution is an O(1) id lookup, not a selector scan: \(js)")
    XCTAssertTrue(
      js.contains("'wl-'"),
      "the script must assemble the `wl-{L}-{C}` marker ids from the numeric line/col")
    XCTAssertTrue(
      js.contains("data-col"),
      "the deep fallback walks the `.wolf-src[data-col=\"0\"]` markers backwards")
    XCTAssertTrue(
      js.contains("requestAnimationFrame"),
      "the scroll listener must be rAF-throttled so it cannot flood the Swift side")
    XCTAssertTrue(
      js.contains("passive"),
      "the scroll listener must be registered passively so it never blocks scrolling")
    XCTAssertTrue(
      js.contains("scrollTo"),
      "the script scrolls with window.scrollTo(0, y): \(js)")
  }

  // MARK: - stateScriptJS must NEVER request smooth scrolling

  func testStateScriptJSNeverUsesSmoothScrolling() {
    XCTAssertFalse(
      PreviewScrollSeam.stateScriptJS().contains("behavior"),
      "a smooth animation would still be running when the next keystroke's follow "
        + "fires, making the restore inexact — the script must scroll instantly")
  }

  // MARK: - D1: the Swift tuning constants actually REACH the generated JS

  func testStateScriptJSCarriesTheSwiftTuningConstants() {
    let js = PreviewScrollSeam.stateScriptJS()
    XCTAssertTrue(
      js.contains(jsDouble(PreviewScrollSeam.minEdgeThreshold)),
      "minEdgeThreshold must be INTERPOLATED into the JS, not duplicated as a literal")
    XCTAssertTrue(
      js.contains(jsDouble(PreviewScrollSeam.edgeThresholdFraction)),
      "edgeThresholdFraction must be INTERPOLATED into the JS")
    XCTAssertTrue(
      js.contains(jsDouble(PreviewScrollSeam.maxEdgeThresholdFraction)),
      "maxEdgeThresholdFraction must be INTERPOLATED into the JS")
    XCTAssertTrue(
      js.contains(jsDouble(PreviewScrollSeam.restPositionFraction)),
      "restPositionFraction must be INTERPOLATED into the JS")
    XCTAssertTrue(
      js.contains(String(PreviewScrollSeam.markerColumnStride)),
      "markerColumnStride must be INTERPOLATED into the JS's Math.floor(col/N)*N")
    XCTAssertTrue(
      js.contains(String(PreviewScrollSeam.maxAnchorScanLines)),
      "maxAnchorScanLines must be INTERPOLATED into the JS's bounded backward scan")
  }

  // MARK: - the programmatic-scroll flag is ARMED before every programmatic scroll

  /// Half of the DEV-2 guard: every scroll the script performs ITSELF must first set
  /// `window.__wolfProgrammatic`, because a `window.scrollTo` fires a real `scroll`
  /// event that is otherwise indistinguishable from a user's wheel.
  func testStateScriptJSArmsTheProgrammaticFlagBeforeScrolling() {
    let js = PreviewScrollSeam.stateScriptJS()
    XCTAssertTrue(
      js.contains("__wolfProgrammatic"),
      "the injected script must carry the programmatic-scroll suppression flag: \(js)")
    XCTAssertTrue(
      js.contains("__wolfProgrammatic = false;"),
      "the flag must be INITIALIZED false at document start, beside __wolfUserScrolled, "
        + "so the first genuine user scroll is not swallowed: \(js)")
    guard let armed = js.range(of: "__wolfProgrammatic = true"),
      let scrolled = js.range(of: "scrollTo(0, y)")
    else {
      return XCTFail(
        "the script must funnel every programmatic scroll through one helper that sets "
          + "__wolfProgrammatic and then calls window.scrollTo(0, y): \(js)")
    }
    XCTAssertLessThan(
      armed.lowerBound, scrolled.lowerBound,
      "the flag must be set BEFORE the scrollTo, not after — the scroll event can be "
        + "dispatched synchronously: \(js)")
  }

  // MARK: - DEV-2: a programmatic scroll must NOT set the user-scrolled flag

  /// The other half, and the actual regression this pins. `restoreJS` re-applies the
  /// restore on the next animation frame and again on `document.fonts.ready`, both
  /// guarded by `!window.__wolfUserScrolled`. `__wolfRestore`'s OWN `scrollTo` fires a
  /// `scroll` event; if the throttled listener treated that as a user scroll, the flag
  /// would already be true by the time either late re-apply ran, so BOTH would always
  /// be skipped — silently killing the exact font-settling mitigation they exist for
  /// while every test stayed green. The listener must therefore CONSUME
  /// `__wolfProgrammatic` and only mark `__wolfUserScrolled` in the `else` branch.
  func testStateScriptJSDoesNotTreatAProgrammaticScrollAsAUserScroll() {
    let js = PreviewScrollSeam.stateScriptJS()
    XCTAssertEqual(
      js.components(separatedBy: "__wolfUserScrolled = true").count - 1, 1,
      "there must be exactly ONE place that marks a scroll as the user's, so this test "
        + "can prove it is unreachable for a programmatic scroll: \(js)")

    guard let listener = js.range(of: "addEventListener('scroll'") else {
      return XCTFail("the script must install a `scroll` listener to keep the cache fresh: \(js)")
    }
    let body = js[listener.lowerBound...]
    guard let consumed = body.range(of: "__wolfProgrammatic"),
      let userScrolled = body.range(of: "__wolfUserScrolled = true")
    else {
      return XCTFail(
        "the scroll listener must consult __wolfProgrammatic before deciding a scroll was "
          + "the user's: \(js)")
    }
    XCTAssertLessThan(
      consumed.lowerBound, userScrolled.lowerBound,
      "the listener must check (and clear) __wolfProgrammatic FIRST; marking "
        + "__wolfUserScrolled unconditionally would make restoreJS's rAF and "
        + "document.fonts.ready re-applies ALWAYS skip: \(js)")
    XCTAssertTrue(
      body.contains("else"),
      "the __wolfUserScrolled assignment must sit in the ELSE branch of the "
        + "programmatic-flag check, so a genuine user scroll still sets it: \(js)")
    XCTAssertTrue(
      body.contains("__wolfProgrammatic = false"),
      "the flag must be CONSUMED (cleared) by the scroll it suppressed, so the next "
        + "scroll is judged on its own merits: \(js)")
  }

  // MARK: - T-6 / L1: the flag is armed ONLY when the offset actually changes

  /// The third half of the DEV-2 guard, and the fix for review finding L1.
  ///
  /// `__wolfProgrammatic` is armed by `moveTo` and CONSUMED by the scroll listener's
  /// rAF callback — but `window.scrollTo(0, y)` to the offset the page is ALREADY at
  /// fires no `scroll` event at all, so an unconditionally armed flag is never
  /// consumed and stays `true`. That is reachable on every restore whose computed
  /// target is 0 (an anchor near the top of the document, or `moveTo(fallbackY)` with
  /// `fallbackY == 0`). The next GENUINE user scroll's first throttled callback then
  /// eats the stale flag and skips setting `__wolfUserScrolled`, so if
  /// `document.fonts.ready` resolves inside that one-frame window the late re-apply
  /// yanks the user back to the restore target. The flag must therefore be armed only
  /// when a `scroll` event will actually follow: `y !== W.scrollY`.
  func testStateScriptJSArmsTheProgrammaticFlagOnlyOnAnActualOffsetChange() {
    let js = PreviewScrollSeam.stateScriptJS()
    XCTAssertEqual(
      js.components(separatedBy: "__wolfProgrammatic = true").count - 1, 1,
      "there must be exactly ONE place that arms the flag, so this test can prove "
        + "that place is guarded: \(js)")

    guard let helper = js.range(of: "moveTo = function") else {
      return XCTFail(
        "every programmatic scroll must funnel through a single `moveTo` helper, which "
          + "is where the arming guard lives: \(js)")
    }
    let body = js[helper.upperBound...]
    guard let armed = body.range(of: "__wolfProgrammatic = true"),
      let scrolled = body.range(of: "scrollTo(0, y)")
    else {
      return XCTFail(
        "the `moveTo` helper must both arm the flag and perform the scroll: \(js)")
    }

    let preamble = body[body.startIndex..<armed.lowerBound]
    XCTAssertFalse(
      preamble.contains("function"),
      "the arming must stay INSIDE the `moveTo` helper — a `function` token between "
        + "moveTo's start and the arming means the arming migrated into some later "
        + "helper, where this test's guard analysis would not apply: \(js)")
    XCTAssertLessThan(
      armed.lowerBound, scrolled.lowerBound,
      "the flag must still be armed BEFORE the scrollTo — the scroll event can be "
        + "dispatched synchronously: \(js)")
    XCTAssertTrue(
      preamble.contains("if"),
      "the arming must be CONDITIONAL, not a straight-line statement: \(js)")

    // Whitespace-insensitive so reformatting the JS string cannot break this, but
    // still exact about WHAT is compared to what.
    let squeezed = preamble.filter { !$0.isWhitespace }
    var accepted: [String] = []
    for offset in ["W.scrollY", "window.scrollY"] {
      for op in ["!==", "!="] {
        accepted.append("y\(op)\(offset)")
        accepted.append("\(offset)\(op)y")
      }
    }
    XCTAssertTrue(
      accepted.contains { squeezed.contains($0) },
      "the guard must compare the REQUESTED offset against the CURRENT one (any of "
        + "\(accepted) ignoring whitespace) — scrolling to where the page already is "
        + "fires no scroll event, so an unconditionally armed flag is never consumed "
        + "and swallows the next genuine user scroll (L1): \(js)")
  }

  // MARK: - T-8 / F-1: the arming guard compares the CLAMPED offset, not the raw one

  /// L1's fix (`y !== W.scrollY`) is bounded, not total — this is the SECOND instance
  /// of the same bug class, and the reason the clamp has to move.
  ///
  /// `__wolfRestore`'s no-anchor branch calls `moveTo(fallbackY)` with the RAW captured
  /// `window.scrollY`. When the reloaded document is SHORTER than the one that offset
  /// came from (text deleted, a fenced block collapsed), the request exceeds the new
  /// document's max scroll; WebKit clamps it, the page ends up somewhere other than the
  /// requested `y`, and if that clamped destination is where the page already sits, NO
  /// `scroll` event is dispatched. The unconsumed `__wolfProgrammatic` then stays armed
  /// and swallows the next GENUINE user scroll — exactly the L1 failure, reached through
  /// a different door. `scrollAnchorTo` already clamps to the document's max scroll
  /// before it calls `moveTo`; the raw-`fallbackY` path is the one still exposed, so the
  /// clamp belongs INSIDE `moveTo`, AHEAD of the comparison, where every programmatic
  /// scroll must pass through it.
  ///
  /// Clamping AFTER the comparison would not help: the comparison would still be judging
  /// an offset the page will never actually reach.
  ///
  /// This test and `testStateScriptJSArmsTheProgrammaticFlagOnlyOnAnActualOffsetChange`
  /// must both hold simultaneously. That one pins the comparison's operand as `y`, so
  /// the clamp must REASSIGN `y` (`y = Math.min(...)`) rather than introduce a second,
  /// differently named variable that the guard and the `scrollTo` would then disagree
  /// about.
  func testStateScriptJSClampsAProgrammaticScrollBeforeArming() {
    let js = PreviewScrollSeam.stateScriptJS()
    guard let helper = js.range(of: "moveTo = function") else {
      return XCTFail(
        "every programmatic scroll must funnel through a single `moveTo` helper, which "
          + "is where the clamp belongs: \(js)")
    }
    let body = js[helper.upperBound...]
    guard let armed = body.range(of: "__wolfProgrammatic = true") else {
      return XCTFail("the `moveTo` helper must arm the programmatic-scroll flag: \(js)")
    }
    let preamble = body[body.startIndex..<armed.lowerBound]
    XCTAssertFalse(
      preamble.contains("function"),
      "the arming must stay INSIDE `moveTo` — a `function` token before it means the "
        + "arming migrated into some later helper, and the clamp this test then finds "
        + "would not be moveTo's: \(js)")

    // Whitespace-insensitive so reformatting the JS string cannot break this, but still
    // exact about WHAT is compared to what, and in which order.
    let squeezed = String(preamble.filter { !$0.isWhitespace })
    var accepted: [String] = []
    for offset in ["W.scrollY", "window.scrollY"] {
      for op in ["!==", "!="] {
        accepted.append("y\(op)\(offset)")
        accepted.append("\(offset)\(op)y")
      }
    }
    var comparisonAt: String.Index?
    for form in accepted {
      guard let at = squeezed.range(of: form)?.lowerBound else { continue }
      if let best = comparisonAt, best <= at { continue }
      comparisonAt = at
    }
    guard let guardAt = comparisonAt else {
      return XCTFail(
        "the arming guard must compare the offset `moveTo` is about to scroll to against "
          + "the current one (any of \(accepted), ignoring whitespace). The clamp must "
          + "REASSIGN `y` so both this guard and the trailing scrollTo(0, y) see the "
          + "clamped value: \(js)")
    }
    guard let clampAt = squeezed.range(of: "Math.min")?.lowerBound,
      let heightAt = squeezed.range(of: "scrollHeight")?.lowerBound
    else {
      return XCTFail(
        "`moveTo` must clamp its argument to the document's max scroll — a Math.min "
          + "against a scrollHeight-derived bound — before it decides whether a scroll "
          + "event will follow: \(js)")
    }
    XCTAssertLessThan(
      clampAt, guardAt,
      "the Math.min clamp must sit BEFORE the offset comparison: comparing the RAW "
        + "target leaves the flag armed whenever WebKit clamps an over-target scroll "
        + "into a no-op, which swallows the next genuine user scroll (F-1): \(js)")
    XCTAssertLessThan(
      heightAt, guardAt,
      "the clamp's upper bound must be derived from the document's scrollHeight, and "
        + "computed before the comparison — an arbitrary Math.min would not bound the "
        + "over-target case F-1 is about: \(js)")
    XCTAssertTrue(
      squeezed.contains("innerHeight") || squeezed.contains("clientHeight"),
      "the max scroll is scrollHeight MINUS the viewport height, so moveTo's bound must "
        + "subtract innerHeight (or clientHeight) — clamping to scrollHeight alone would "
        + "still leave every bottom-of-document restore over-target: \(js)")
  }

  // MARK: - N2: EVERY scroll the script performs funnels through the single `moveTo`

  /// The structural pin the other three DEV-2 guards are missing.
  ///
  /// They assert that the ONE `__wolfProgrammatic = true` sits inside `moveTo`, that it
  /// is guarded, and that the guard sees the clamped offset — but nothing stops a future
  /// edit from adding a SECOND, direct `W.scrollTo(...)` somewhere else in the script
  /// (say, straight inside `__wolfRestore`'s no-anchor branch, or a new "jump to top"
  /// helper). Such an edit would keep every existing test green: there would still be
  /// exactly one arming site, still inside `moveTo`, still guarded and clamped. Yet the
  /// bypassing scroll would fire a `scroll` event with the flag NOT armed, the throttled
  /// listener would classify the script's OWN scroll as the user's, `__wolfUserScrolled`
  /// would go `true`, and `restoreJS`'s `requestAnimationFrame` and
  /// `document.fonts.ready` re-applies would both be skipped — the DEV-2 bug for the
  /// third time, silently, with a green suite.
  ///
  /// So the invariant is not "the arming is correct" but "there is exactly ONE scroll,
  /// and it is `moveTo`'s". Only that composes: `scrollAnchorTo`, `__wolfFollow` and
  /// `__wolfRestore` must all reach the viewport through `moveTo`.
  ///
  /// Scope note: this is about `stateScriptJS` ONLY. `PreviewLinkSeam.scrollIntoViewJS`
  /// emits `e.scrollIntoView(true)` — a DIFFERENT function in a DIFFERENT seam that
  /// legitimately bypasses `moveTo`, because a fragment jump SHOULD be recorded as a user
  /// scroll so a late restore re-apply does not fight a deliberate anchor jump. The
  /// substring matched here is `scrollTo(` with its paren, which cannot match
  /// `scrollIntoView`; the sweep below additionally proves the injected script contains no
  /// `scrollIntoView` (or any other viewport-mutating API) of its own.
  func testStateScriptJSFunnelsEveryScrollThroughMoveTo() {
    let js = PreviewScrollSeam.stateScriptJS()
    // Whitespace-insensitive so reformatting the JS string cannot break this test, while
    // the failure messages still print the readable original.
    let squeezed = String(js.filter { !$0.isWhitespace })

    XCTAssertEqual(
      squeezed.components(separatedBy: "scrollTo(").count - 1, 1,
      "the injected script must contain EXACTLY ONE `scrollTo(` — every programmatic "
        + "scroll funnels through the single `moveTo` helper, which is the only place "
        + "that arms __wolfProgrammatic. A second, direct scrollTo would fire a scroll "
        + "event with the flag unarmed, the listener would mark it as a USER scroll, and "
        + "restoreJS's rAF / document.fonts.ready re-applies would then always be "
        + "skipped (DEV-2). Route the new scroll through moveTo instead: \(js)")

    guard let helper = squeezed.range(of: "moveTo=function") else {
      return XCTFail(
        "every programmatic scroll must funnel through a single `moveTo` helper, which "
          + "is what this test locates the sole scrollTo inside: \(js)")
    }
    let body = squeezed[helper.upperBound...]
    guard let scrolled = body.range(of: "scrollTo(") else {
      return XCTFail(
        "the script's sole `scrollTo(` sits BEFORE the `moveTo` helper, so it is not "
          + "moveTo's — a scroll outside the funnel is not preceded by the "
          + "__wolfProgrammatic arming and would be misread as a user scroll (DEV-2): "
          + "\(js)")
    }
    XCTAssertFalse(
      body[body.startIndex..<scrolled.lowerBound].contains("function"),
      "the sole `scrollTo(` must sit INSIDE `moveTo`'s body — a `function` token between "
        + "moveTo's start and the scroll means the scroll migrated into some LATER "
        + "helper, i.e. it bypasses the funnel and its arming/clamping guards (DEV-2): "
        + "\(js)")

    // No other viewport-mutating API may appear either: each is an equally valid way to
    // scroll around `moveTo`'s back, and would reproduce the same DEV-2 failure.
    for bypass in [".scroll(", "scrollBy(", "scrollIntoView", "scrollTop="] {
      XCTAssertFalse(
        squeezed.contains(bypass),
        "the injected script must not move the viewport with `\(bypass)` — every scroll "
          + "has to go through `moveTo` so __wolfProgrammatic is armed first, otherwise "
          + "the script's own scroll is classified as the USER's and restoreJS's late "
          + "re-applies are silently killed (DEV-2). Note this bans the API only in "
          + "stateScriptJS: PreviewLinkSeam.scrollIntoViewJS's e.scrollIntoView(true) is "
          + "a different seam and legitimately bypasses moveTo: \(js)")
    }
  }

  // MARK: - locale safety: interpolated Doubles always use a `.` decimal separator

  func testInterpolatedDoublesAreNotLocalized() {
    let request = PreviewScrollSeam.RestoreRequest(
      cursor: PreviewScrollSeam.CursorPos(line: 1, col: 0),
      targetY: 1234.5,
      fallbackScrollY: 0)
    let js = PreviewScrollSeam.restoreJS(request)
    XCTAssertTrue(
      js.contains("1234.50"),
      "String(format:) without a locale argument is non-localized, so the decimal "
        + "separator is always `.`: \(js)")
    XCTAssertFalse(
      js.contains("1234,50"),
      "a comma decimal separator would corrupt the JS argument list (it would read as "
        + "two arguments): \(js)")
  }
}
