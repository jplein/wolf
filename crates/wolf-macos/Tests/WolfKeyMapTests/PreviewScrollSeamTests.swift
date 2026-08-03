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
//        public struct DocumentSplit: Equatable {
//            public let headLength: Int   // UTF-8 count of everything up to AND
//                                         // INCLUDING the `<body …>` tag
//            public let body: String      // the body element's INNER html
//            public init(headLength: Int, body: String)
//        }
//        public struct LiveDocument: Equatable {
//            public let sourceBufnr: Int64 // what is COMMITTED in the web view — NOT
//            public let headLength: Int    // PreviewSeam.PreviewLoad, which is what
//                                          // was last DISPATCHED (D3)
//            public init(sourceBufnr: Int64, headLength: Int)
//        }
//        public enum UpdateStrategy: Equatable {
//            case fullLoad                // loadHTMLString; didFinish drives
//                                         // capture/restore
//            case inPlace(body: String)   // callAsyncJavaScript(__wolfUpdate); NO
//                                         // navigation
//        }
//        public enum LoadCommitOutcome: Equatable {
//            case stillOutstanding        // more loads pending: leave the live AND
//                                         // the pending descriptor ALONE
//            case commit(LiveDocument?)   // the last load, and it SUCCEEDED:
//                                         // live = pending, then clear pending
//            case discard                 // the last load, and it FAILED: leave live
//                                         // UNCHANGED, clear pending
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
//        static func splitDocument(_ html: String) -> DocumentSplit?
//        static func updateStrategy(
//            html: String,
//            currentLoad: PreviewSeam.PreviewLoad,
//            liveDocument: LiveDocument?,   // previewLiveDocuments[win]
//            loadInFlight: Bool,            // (previewLoadsInFlight[win] ?? 0) > 0
//            hasPendingFragment: Bool       // pendingPreviewFragments[win] != nil
//        ) -> UpdateStrategy
//        static func loadCommitOutcome(
//            remaining: Int,                // previewLoadsInFlight[win] AFTER the
//                                           // callback decremented it
//            pending: LiveDocument?,        // pendingPreviewDocuments[win]
//            succeeded: Bool                // didFinish => true, didFail => false
//        ) -> LoadCommitOutcome
//        public static let inPlaceUpdateArgumentNames:
//            (html: String, line: String, col: String)
//        static func inPlaceUpdateJS() -> String
//        static func stateScriptJS(handlerName: String = stateHandlerName) -> String
//        static func followJS(line: UInt32, col: UInt32) -> String
//        static func restoreJS(_ request: RestoreRequest) -> String
//    }
//
//  ## THE TWO-PATH UPDATE MODEL (the flicker fix)
//
//  Navigating the preview web view on every keystroke re-ships ~965 KiB of inlined
//  base64 `@font-face` CSS across the WKWebView IPC boundary at the reload
//  coalescer's ~33ms cadence, re-parses it, and commits a whole new document — which
//  commits at scrollY 0 and only THEN jumps to the restored offset. That visible
//  round trip is the flicker. So the typing path stops navigating: `updateStrategy`
//  decides per window between the existing full `loadHTMLString` and an IN-PLACE body
//  swap, and `splitDocument` carves the `<body>` inner HTML out of `render_html`'s
//  fixed wrapper so only that ~200 KB travels.
//
//  The `<head>` is INVARIANT for the in-app preview: `render_preview` hardcodes
//  `PreviewTheme::Default` + `WidthMode::Full`, and dark/light is a
//  `prefers-color-scheme` media query INSIDE that constant CSS, so even an appearance
//  flip changes only the body's inline `style="color:#…"` highlight spans. That is
//  what makes a body-only swap correct. `updateStrategy` rule 6 (`headLength`
//  equality) makes the premise SELF-ENFORCING rather than comment-only — the class of
//  latent assumption that produced H1/H2.
//
//  `updateStrategy`'s six `.fullLoad` rules are ORDERED, and each one is a
//  correctness rule, not an optimization:
//
//   1. a PENDING FRAGMENT — the `#anchor` is delivered from `previewDidFinishLoad`,
//      which an in-place update never fires, so the fragment would be stranded;
//   2. a LOAD IN FLIGHT — mutating the outgoing document is pointless and would race
//      that load's `didFinish` restore. This rule is ALSO what guarantees
//      `pendingPreviewRestores[win] == nil` on every in-place path, since a pending
//      restore exists only between `loadHTMLString` and `didFinish`;
//   3. NO LIVE DOCUMENT — nothing is committed, so `window.__wolfUpdate` may not
//      exist (first bind, and post-failure recovery);
//   4. a SOURCE SWITCH — a navigation; the new file must open at its top. This is
//      H1's guarantee mirrored at the strategy layer;
//   5. UNSPLITTABLE HTML — e.g. the `""` a failed `render_preview` returns;
//   6. a CHANGED HEAD LENGTH — a body-only swap would leave stale CSS.
//
//  ## TWO NAVIGATIONS CAN STRADDLE ONE PAGE LOAD (M1)
//
//  `previewLoadsInFlight` is a per-window COUNT, never a Set, because the reload
//  coalescer serializes to one reload per ~33ms and does NOT wait for `didFinish` —
//  the very same straddling window H2 exists to survive. With a Set, `didFinish(N)`
//  — or `didFail(N)`, since WebKit reports a SUPERSEDED load as a failure — cleared
//  the flag while load N+1 was still outstanding AND destructively consumed N+1's
//  pending descriptor into `previewLiveDocuments`. The next coalescer tick then saw
//  rule 2 false and rule 3 false and chose `.inPlace`, swapping a body into the
//  document N+1 was about to replace; N+1 then committed, discarded the swap and
//  restored — a visible flicker, i.e. the exact symptom this whole change exists to
//  remove — and N+1's own `didFinish` finally nil'd the live descriptor and forced
//  one more pointless full load.
//
//  ``PreviewScrollSeam/loadCommitOutcome(remaining:pending:succeeded:)`` is the pure
//  decision that closes it, moved into the seam for the same reason `captureDecision`
//  was: the bug lived in `PaneContainerView` bookkeeping, which is compile +
//  manual-smoke only. BOTH callbacks decrement the count first and then hand the
//  REMAINING count, the pending descriptor and whether the navigation succeeded to
//  the seam:
//
//   * `remaining > 0` -> `.stillOutstanding`, which carries NO payload at all, so a
//     superseded load's callback can neither commit nor discard anything;
//   * `remaining == 0 && succeeded` -> `.commit(pending)` — including `.commit(nil)`
//     when the HTML could not be split, since an unsplittable document must never
//     become a live document;
//   * `remaining == 0 && !succeeded` -> `.discard`, which is deliberately NOT
//     `.commit(nil)`: a failed navigation never commits, so WebKit keeps displaying
//     the PREVIOUS document and the previous live descriptor stays accurate.
//     Conflating the two would wipe a good live document (M1 step 7).
//
//  `updateStrategy`'s own signature is unchanged; only the CALLER changes, deriving
//  `loadInFlight` as `(previewLoadsInFlight[win] ?? 0) > 0`.
//
//  ## `captureDecision` IS UNCHANGED AND IS A FULL-LOAD-ONLY CONCERN
//
//  `captureDecision` gains NO case, loses NO rule, and is still called exactly where
//  it is called today: immediately before `loadHTMLString`. All 12 of its tests below
//  stay valid VERBATIM — they are the H1/H2 regression guards and the fix must not
//  silently retire them. The two functions answer DIFFERENT questions at DIFFERENT
//  times: `captureDecision` keys on `lastPreviewLoads[win]` ("what did we last
//  DISPATCH"), `updateStrategy` keys on `previewLiveDocuments[win]` ("what is
//  COMMITTED"). `updateStrategy` rule 4 and `captureDecision` rule 1 are
//  near-duplicates BY DESIGN and must always agree on a real source switch — which is
//  what `testASourceSwitchIsBothAFullLoadAndADrop` pins. Both are required because one
//  runs before the navigation exists and the other before the restore is consumed.
//
//  ## WHY THE HTML TRAVELS AS AN ARGUMENT, NOT AS SOURCE
//
//  `inPlaceUpdateJS()` is the body handed to
//  `WKWebView.callAsyncJavaScript(_:arguments:in:contentWorld:)`, whose free
//  identifiers must be EXACTLY the keys of the caller's `arguments:` dictionary. Both
//  sides read those names from ``inPlaceUpdateArgumentNames``, so the body and the
//  dictionary cannot drift; `testInPlaceUpdateJSUsesExactlyTheDeclaredArgumentNames`
//  is that anti-drift pin. A ~200 KB body string therefore never gets escaped into,
//  or re-lexed as, JS source — which is why `testInPlaceUpdateJSInterpolatesNoDocument`
//  asserts the returned string contains no `<` at all.
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
  ///
  /// NEW CALLER — `__wolfUpdate` (the in-place body swap). It is the fourth entry point
  /// that moves the viewport, and it is bound by exactly the same invariant: it must
  /// reach the viewport ONLY through `scrollAnchorTo` / `moveTo` / `__wolfFollow`, and
  /// must introduce neither a second `scrollTo(` nor any of the bypass APIs swept below.
  /// It is deliberately defined AFTER `moveTo` in the script, which is what keeps the
  /// "no `function` token between `moveTo=function` and the sole `scrollTo(`" analysis
  /// above valid. Do NOT "optimize" a direct `window.scrollTo` / `el.scrollIntoView` into
  /// the swap to save the re-anchor's layout flush: the swap's whole point is that the
  /// replaceChildren AND the re-anchor happen in ONE JavaScript task, and routing the
  /// re-anchor around `moveTo` would fire an unarmed `scroll` event that the listener
  /// classifies as the USER's — DEV-2, for the fourth time. If this test ever fails after
  /// `__wolfUpdate` is added, the fix is to move `__wolfUpdate`'s DEFINITION after
  /// `moveTo`; it is never to loosen an assertion here.
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

  // MARK: - in-place-update fixtures

  /// `render_html`'s FIXED wrapper up to AND INCLUDING the `<body …>` open tag — i.e.
  /// exactly the prefix whose UTF-8 count a ``PreviewScrollSeam/DocumentSplit``'s
  /// `headLength` reports. Transcribed from the `format!` in
  /// `crates/wolf-core/src/markdown.rs` (`render_html`), with the giant
  /// `{INTER_FONT_FACE}{MATH_FONT_FACE}{BASE_CSS}{MATH_CSS}{theme}{width_css}`
  /// concatenation standing in as one short `head` string.
  ///
  /// Written out VERBATIM rather than derived from `splitDocument`, so a bug in the
  /// scan cannot define its own expected value into existence.
  private func headPrefix(head: String = "CSS") -> String {
    "<!doctype html><html><head><meta charset=\"utf-8\"><style>\(head)</style></head>"
      + "<body class=\"wolf-markdown\">"
  }

  /// A whole preview document in that wrapper's shape.
  private func document(head: String = "CSS", body: String) -> String {
    headPrefix(head: head) + body + "</body></html>"
  }

  /// The `headLength` a split of `document(head:body:)` must report.
  private func headLength(head: String = "CSS") -> Int {
    headPrefix(head: head).utf8.count
  }

  /// What is COMMITTED in a preview web view currently showing `document(head:)` for
  /// `bufnr` — the `previewLiveDocuments[win]` input to `updateStrategy`.
  private func live(bufnr: Int64, head: String = "CSS") -> PreviewScrollSeam.LiveDocument {
    PreviewScrollSeam.LiveDocument(sourceBufnr: bufnr, headLength: headLength(head: head))
  }

  /// `updateStrategy` with the "ordinary same-buffer edit" defaults, so each test below
  /// varies exactly the ONE input it is about.
  private func strategy(
    html: String,
    bufnr: Int64,
    tick: UInt64 = 6,
    liveDocument: PreviewScrollSeam.LiveDocument?,
    loadInFlight: Bool = false,
    hasPendingFragment: Bool = false
  ) -> PreviewScrollSeam.UpdateStrategy {
    PreviewScrollSeam.updateStrategy(
      html: html,
      currentLoad: load(bufnr: bufnr, tick: tick),
      liveDocument: liveDocument,
      loadInFlight: loadInFlight,
      hasPendingFragment: hasPendingFragment)
  }

  /// The body carried by an `.inPlace` strategy, or `nil` (with a failure) otherwise.
  private func inPlaceBody(
    _ strategy: PreviewScrollSeam.UpdateStrategy,
    file: StaticString = #filePath,
    line: UInt = #line
  ) -> String? {
    guard case .inPlace(let body) = strategy else {
      XCTFail("expected an .inPlace strategy, got \(strategy)", file: file, line: line)
      return nil
    }
    return body
  }

  // MARK: - splitDocument: the real wrapper splits into (head length, body)

  func testSplitDocumentExtractsTheBodyOfTheRealWrapper() {
    let body = "<h1 id=\"title\">Title</h1><p>text</p>"
    let split = PreviewScrollSeam.splitDocument(document(body: body))
    XCTAssertEqual(
      split,
      PreviewScrollSeam.DocumentSplit(headLength: headLength(), body: body),
      "splitDocument must carve render_html's fixed "
        + "`<!doctype html><html><head>…</head><body class=\"wolf-markdown\">{body}"
        + "</body></html>` wrapper into the body's INNER html plus the UTF-8 length of "
        + "everything up to and INCLUDING the `<body …>` open tag")
    XCTAssertFalse(
      split?.body.hasPrefix("<body") ?? true,
      "the `<body …>` open tag belongs to the HEAD side of the split — including it "
        + "would make replaceChildren nest a second body inside the live one")
    XCTAssertFalse(
      split?.body.hasSuffix("</body>") ?? true,
      "the wrapper's closing tag is a delimiter, not content")
  }

  // MARK: - splitDocument: an empty document is the RPC-failure sentinel

  func testSplitDocumentReturnsNilForEmptyHTML() {
    XCTAssertNil(
      PreviewScrollSeam.splitDocument(""),
      "`WolfCore::render_preview` returns \"\" when the Neovim RPC fails, and an empty "
        + "document must NEVER be swapped into a live preview — returning nil is what "
        + "sends that tick down updateStrategy rule 5 to the (unchanged) full path")
  }

  // MARK: - splitDocument: a document with no body element cannot be split

  func testSplitDocumentReturnsNilWithoutABodyTag() {
    XCTAssertNil(
      PreviewScrollSeam.splitDocument("<!doctype html><html><head></head></html>"),
      "with no `<body` there is nothing to swap; the wrapper must have changed shape, "
        + "so degrade to a full load rather than guess")
    XCTAssertNil(
      PreviewScrollSeam.splitDocument("<p>a bare fragment with no wrapper at all</p>"),
      "a fragment is not a document — splitDocument must not treat the whole string as "
        + "a body")
  }

  // MARK: - splitDocument: an unterminated body cannot be split

  func testSplitDocumentReturnsNilWithoutAClosingBodyTag() {
    XCTAssertNil(
      PreviewScrollSeam.splitDocument(
        "<!doctype html><html><head></head><body class=\"wolf-markdown\">truncated"),
      "a truncated document has no closing `</body>`, so there is no way to know where "
        + "the body ends — full load")
    XCTAssertNil(
      PreviewScrollSeam.splitDocument("</body><body class=\"wolf-markdown\">inverted"),
      "a `</body>` that PRECEDES the open tag does not close it; the closing tag must "
        + "be found at or after the open tag or the substring range would be inverted")
  }

  // MARK: - splitDocument: the LAST `</body>` wins (raw HTML passthrough)

  func testSplitDocumentUsesTheLastClosingBodyTag() {
    // pulldown-cmark passes raw HTML through (there is no disable flag in
    // `markdown_options()`), so a source file can put a LITERAL `</body>` in the
    // rendered body. The wrapper's own closing tag is always the last one.
    let body = "<p>before</p></body><p>after</p>"
    let split = PreviewScrollSeam.splitDocument(document(body: body))
    XCTAssertEqual(
      split?.body, body,
      "the scan must take the LAST `</body>` — taking the first would truncate the "
        + "document at a user-authored literal and silently drop everything after it")
    XCTAssertTrue(
      split?.body.contains("</body>") ?? false,
      "the embedded literal must survive INSIDE the extracted body, untouched")
  }

  // MARK: - splitDocument: the FIRST `<body` wins (raw HTML passthrough)

  /// The mirror image of ``testSplitDocumentUsesTheLastClosingBodyTag``, and the other
  /// half of the scan's deliberate first-open / last-close asymmetry. Raw HTML passes
  /// straight through pulldown-cmark, so a source file can put a literal `<body` into
  /// the RENDERED BODY just as easily as a literal `</body>` — but any such tag is
  /// always AFTER the wrapper's own open tag, which is why the FIRST one is the
  /// wrapper's.
  func testSplitDocumentUsesTheFirstOpeningBodyTag() {
    let body = "<p>before</p><body class=\"from-the-source-file\"><p>after</p>"
    let split = PreviewScrollSeam.splitDocument(document(body: body))
    XCTAssertEqual(
      split?.headLength, headLength(),
      "headLength must be measured through the WRAPPER's open tag. Taking the LAST "
        + "`<body` instead would report a head that runs into the middle of the "
        + "content, so rule 6's `did the head change?` comparison would start tracking "
        + "the body's text and every keystroke would look like a head change")
    XCTAssertEqual(
      split?.body, body,
      "and the body extracted from the wrapper's open tag is the WHOLE body: taking a "
        + "later `<body` would silently drop everything the user wrote before their "
        + "literal tag")
    XCTAssertTrue(
      split?.body.contains("<body") ?? false,
      "the embedded literal must survive INSIDE the extracted body, untouched — "
        + "splitDocument is a scan, not a parser, and the page's DOMParser is what "
        + "decides what a nested `<body` means")
  }

  // MARK: - splitDocument: a `<body>` carrying no attributes at all

  /// The scan finds the first `>` AFTER the first `<body`, so it must not depend on
  /// an attribute being present to terminate the open tag. `render_html` emits
  /// `<body class="wolf-markdown">` today, but the browser-preview wrapper and any
  /// future render change need not, and an attribute-dependent scan would fail by
  /// returning a WRONG split rather than `nil` — the one failure mode rule 5's
  /// degrade-to-full-load safety net cannot catch.
  func testSplitDocumentHandlesABodyTagWithNoAttributes() {
    let prefix = "<!doctype html><html><head><style>CSS</style></head><body>"
    let body = "<p>content</p>"
    let split = PreviewScrollSeam.splitDocument(prefix + body + "</body></html>")
    XCTAssertEqual(
      split,
      PreviewScrollSeam.DocumentSplit(headLength: prefix.utf8.count, body: body),
      "a bare `<body>` must split exactly like `<body class=\"wolf-markdown\">`: the "
        + "head runs up to and INCLUDING the tag's own `>`, and the body is everything "
        + "after it")
    XCTAssertFalse(
      split?.body.hasPrefix(">") ?? true,
      "the open tag's `>` belongs to the HEAD side of the split — leaving it on the "
        + "body side would prepend a stray `>` text node to the live document on every "
        + "in-place swap")
  }

  // MARK: - splitDocument: MathML and highlight spans survive byte-for-byte

  func testSplitDocumentPreservesMathMLAndHighlightSpans() {
    // Everything the in-place DOMParser path has to reproduce faithfully: the
    // MathRenderer's inline `<math>` markup, the CodeHighlighter's inline-styled
    // spans, and the `.wolf-src` position markers the whole anchor scheme depends on.
    let body =
      "<span class=\"wolf-src\" id=\"wl-1-0\" data-line=\"1\" data-col=\"0\"></span>"
      + "<math><mi>x</mi><mo>&#8804;</mo><mn>2</mn></math>"
      + "<pre><code><span style=\"color:#ff0000;font-weight:bold\">fn</span></code></pre>"
    let split = PreviewScrollSeam.splitDocument(document(body: body))
    XCTAssertEqual(
      split?.body, body,
      "splitDocument is a SCAN, not a parser: it must never sanitize, re-encode or "
        + "normalize the body. The markers are what `resolve` looks up by id, the "
        + "inline styles are the only thing an appearance flip actually changes, and "
        + "the `<math>` markup is ordinary HTML-parser input")
  }

  // MARK: - splitDocument: headLength tracks the head, in UTF-8 BYTES

  func testSplitDocumentHeadLengthChangesWithTheHeadContent() {
    let body = "<p>same</p>"
    let short = PreviewScrollSeam.splitDocument(document(head: "A", body: body))
    let long = PreviewScrollSeam.splitDocument(document(head: "AAAA", body: body))
    XCTAssertEqual(short?.body, body, "the body is unaffected by the head")
    XCTAssertEqual(long?.body, body, "the body is unaffected by the head")
    XCTAssertEqual(
      (long?.headLength ?? 0) - (short?.headLength ?? 0), 3,
      "headLength must track the head's content exactly — it is the ONLY detector "
        + "updateStrategy rule 6 has for `the CSS changed, so a body-only swap would "
        + "leave stale styles`")

    // The wrapper is `format!`ed from a Rust `String`, so the length that matters is
    // the UTF-8 one. A head carrying any non-ASCII character makes the two counts
    // disagree, which is what pins WHICH count is meant.
    let prefix = headPrefix(head: "—")
    let split = PreviewScrollSeam.splitDocument(document(head: "—", body: body))
    XCTAssertEqual(
      split?.headLength, prefix.utf8.count,
      "headLength is a UTF-8 BYTE count, matching the Rust side's notion of length")
    XCTAssertNotEqual(
      prefix.utf8.count, prefix.count,
      "this fixture only pins the UTF-8 rule if the two counts actually differ")
  }

  // MARK: - updateStrategy rule 1: a pending fragment must navigate

  func testUpdateStrategyFullLoadWhenAFragmentIsPending() {
    XCTAssertEqual(
      strategy(
        html: document(body: "<p>edited</p>"),
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr),
        hasPendingFragment: true),
      .fullLoad,
      "a cross-file `other.md#anchor` link stashes the fragment in "
        + "pendingPreviewFragments and it is delivered from previewDidFinishLoad — "
        + "which an in-place update NEVER fires. Swapping the body here would strand "
        + "the anchor forever")
  }

  // MARK: - updateStrategy rule 2: never mutate a document that is being replaced

  func testUpdateStrategyFullLoadWhenALoadIsInFlight() {
    XCTAssertEqual(
      strategy(
        html: document(body: "<p>edited</p>"),
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr),
        loadInFlight: true),
      .fullLoad,
      "an otherwise PERFECT same-buffer edit must still take the full path while a "
        + "loadHTMLString is outstanding: the in-flight navigation will discard the "
        + "swap, and the swap would race that load's didFinish restore. This rule is "
        + "also what structurally guarantees pendingPreviewRestores[win] == nil on "
        + "every in-place path, since a pending restore exists ONLY between "
        + "loadHTMLString and didFinish")
  }

  // MARK: - updateStrategy rule 3: nothing is committed yet

  func testUpdateStrategyFullLoadOnFirstBind() {
    XCTAssertEqual(
      strategy(
        html: document(body: "<p>first</p>"),
        bufnr: sourceBufnr,
        liveDocument: nil),
      .fullLoad,
      "with no live document there is no page to swap into and `window.__wolfUpdate` "
        + "may not exist at all. This covers first bind AND recovery after a failed "
        + "in-place update, which clears previewLiveDocuments[win] precisely to force "
        + "this rule")
  }

  // MARK: - updateStrategy rule 4: a source switch is a NAVIGATION (H1, mirrored)

  func testUpdateStrategyFullLoadWhenTheSourceBufferSwitched() {
    XCTAssertEqual(
      strategy(
        html: document(body: "<p>the other file</p>"),
        bufnr: otherBufnr,
        tick: 5,
        liveDocument: live(bufnr: sourceBufnr)),
      .fullLoad,
      "the committed document belongs to another SOURCE BUFFER, so this is a "
        + "navigation and the new file must open at its TOP — H1's guarantee, mirrored "
        + "at the strategy layer. Note the ticks are EQUAL here: two freshly opened "
        + "files can legitimately share a changedtick, so only the bufnr may decide")
  }

  // MARK: - updateStrategy rule 5: an unsplittable document

  func testUpdateStrategyFullLoadWhenTheHTMLCannotBeSplit() {
    XCTAssertEqual(
      strategy(html: "", bufnr: sourceBufnr, liveDocument: live(bufnr: sourceBufnr)),
      .fullLoad,
      "`render_preview` returns \"\" on an RPC failure; there is no body to swap, so "
        + "the tick degrades to exactly today's behavior")
    XCTAssertEqual(
      strategy(
        html: "<!doctype html><html><head></head></html>",
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr)),
      .fullLoad,
      "a wrapper whose `<body>` cannot be located must degrade to a full load, never "
        + "to breakage — that is what keeps splitDocument's coupling to render_html's "
        + "wrapper safe")
  }

  // MARK: - updateStrategy rule 6: the head-is-invariant premise, self-enforcing

  func testUpdateStrategyFullLoadWhenTheHeadLengthChanged() {
    let html = document(head: "CSS-AND-MORE", body: "<p>edited</p>")
    XCTAssertEqual(
      strategy(html: html, bufnr: sourceBufnr, liveDocument: live(bufnr: sourceBufnr)),
      .fullLoad,
      "a body-only swap leaves the committed document's `<head>` in place, so a "
        + "changed head would mean stale CSS. Today this is unreachable — the in-app "
        + "preview pins PreviewTheme::Default + WidthMode::Full, so the head is "
        + "byte-identical on every render, INCLUDING across a dark/light flip — but "
        + "encoding it as a RULE makes the premise self-enforcing rather than "
        + "comment-only, which is the class of latent assumption that produced H1/H2")
    XCTAssertEqual(
      strategy(
        html: html,
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr, head: "CSS-AND-MORE")),
      .inPlace(body: "<p>edited</p>"),
      "and the head length is what flipped it: the same html against a live document "
        + "whose head MATCHES is an ordinary in-place update")
  }

  // MARK: - updateStrategy rule 7: the typing path

  func testUpdateStrategyInPlaceForAnOrdinaryEdit() {
    let body = "<p>the user just typed a character</p>"
    XCTAssertEqual(
      strategy(
        html: document(body: body),
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr)),
      .inPlace(body: body),
      "same source buffer, nothing in flight, no fragment pending, a splittable "
        + "document with an unchanged head: the keystroke path, and the whole reason "
        + "this strategy exists. The carried payload is the body's INNER html, which "
        + "is what `__wolfUpdate` DOMParses and replaceChildren-s in")
  }

  // MARK: - updateStrategy: the TICK is not an input to this decision

  func testUpdateStrategyInPlaceIgnoresTheTick() {
    let body = "<p>edited</p>"
    for tick: UInt64 in [1, 2, 99] {
      XCTAssertEqual(
        strategy(
          html: document(body: body),
          bufnr: sourceBufnr,
          tick: tick,
          liveDocument: live(bufnr: sourceBufnr)),
        .inPlace(body: body),
        "LiveDocument is deliberately (sourceBufnr, headLength) with NO tick: what is "
          + "committed is a DOCUMENT, and how many edits it is behind is irrelevant to "
          + "whether its body can be swapped. Two ticks apart, or ninety-eight, is "
          + "still an in-place update (tick=\(tick))")
    }
  }

  // MARK: - updateStrategy: the rules are ORDERED (rule 1 over rule 7)

  func testUpdateStrategyRuleOrderFragmentBeatsAValidInPlace() {
    let html = document(body: "<p>edited</p>")
    XCTAssertEqual(
      strategy(
        html: html,
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr),
        hasPendingFragment: true),
      .fullLoad,
      "every other input here describes a textbook in-place candidate, and the pending "
        + "fragment alone must still force a navigation")
    XCTAssertEqual(
      strategy(
        html: html,
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr),
        hasPendingFragment: false),
      .inPlace(body: "<p>edited</p>"),
      "and with the fragment cleared the SAME inputs go in place — so the fragment is "
        + "provably what decided it, not some incidental mismatch")
  }

  // MARK: - updateStrategy: the rules are ORDERED (rule 4 over rule 6/7)

  func testUpdateStrategyRuleOrderSourceSwitchBeatsAMatchingHeadLength() {
    let html = document(body: "<p>the other file</p>")
    XCTAssertEqual(
      strategy(html: html, bufnr: otherBufnr, liveDocument: live(bufnr: sourceBufnr)),
      .fullLoad,
      "the head lengths MATCH (every in-app preview shares the same constant head), so "
        + "rule 6 is satisfied and only rule 4 stands between a cross-file link and an "
        + "in-place swap that would open the new file at the old one's scroll offset — "
        + "the H1 regression, reached through the new path")
    XCTAssertEqual(
      strategy(html: html, bufnr: sourceBufnr, liveDocument: live(bufnr: sourceBufnr)),
      .inPlace(body: "<p>the other file</p>"),
      "and the bufnr is provably what decided it: the identical html against a live "
        + "document for the SAME buffer goes in place")
  }

  // MARK: - updateStrategy: rules 1 and 2 are INDEPENDENT, not one masking the other

  /// Both flags are true at once in a real, reachable sequence: a cross-file
  /// `other.md#anchor` click stashes the fragment in `pendingPreviewFragments` AND
  /// issues the `loadHTMLString` that sets `previewLoadsInFlight`, and a keystroke
  /// arriving before that load's `didFinish` sees both. Each must force `.fullLoad` on
  /// its own, for its own reason — the fragment because it is delivered from
  /// `previewDidFinishLoad`, which an in-place update never fires; the in-flight load
  /// because the swap would be discarded by the navigation and would race its restore.
  /// If one silently masked the other, removing the masked rule would look harmless.
  func testUpdateStrategyFullLoadWhenALoadIsInFlightAndAFragmentIsPending() {
    let html = document(body: "<p>edited</p>")
    XCTAssertEqual(
      strategy(
        html: html,
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr),
        loadInFlight: true,
        hasPendingFragment: true),
      .fullLoad,
      "a keystroke landing while a fragment-carrying load is still in flight must "
        + "navigate — the anchor would otherwise be stranded AND the swap would race "
        + "the outstanding load's didFinish restore")
    XCTAssertEqual(
      strategy(
        html: html,
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr),
        loadInFlight: true,
        hasPendingFragment: false),
      .fullLoad,
      "the in-flight load ALONE is enough (rule 2), with no fragment to help it")
    XCTAssertEqual(
      strategy(
        html: html,
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr),
        loadInFlight: false,
        hasPendingFragment: true),
      .fullLoad,
      "and the pending fragment ALONE is enough (rule 1), with nothing in flight to "
        + "help it — so neither rule is merely riding on the other")
    XCTAssertEqual(
      strategy(
        html: html,
        bufnr: sourceBufnr,
        liveDocument: live(bufnr: sourceBufnr),
        loadInFlight: false,
        hasPendingFragment: false),
      .inPlace(body: "<p>edited</p>"),
      "the control: with BOTH flags cleared these otherwise identical inputs are a "
        + "textbook in-place update, so the three .fullLoad results above are provably "
        + "caused by the flags and not by some incidental mismatch")
  }

  // MARK: - E1: the in-place payload sheds the ~965 KiB base64 head

  func testUpdateStrategyInPlaceCarriesTheExactBodyNotTheWholeDocument() {
    let head = "@font-face{font-family:Inter;src:url(data:font/woff2;base64,AAAABBBB)}"
    let body = "<p>edited</p>"
    let html = document(head: head, body: body)
    let decision = strategy(
      html: html,
      bufnr: sourceBufnr,
      liveDocument: live(bufnr: sourceBufnr, head: head))
    guard let carried = inPlaceBody(decision) else { return }
    XCTAssertEqual(carried, body, "the payload is the body's inner html, exactly")
    XCTAssertTrue(
      html.contains("@font-face") && html.contains("<style"),
      "this fixture is only meaningful if the whole document really does carry the "
        + "head payload")
    for shed in ["<style", "@font-face", "base64", "<!doctype", "<head"] {
      XCTAssertFalse(
        carried.contains(shed),
        "the in-place payload must NOT carry `\(shed)`. The real head inlines two "
          + "woff2 faces as ~965 KiB of base64; re-shipping that across the WKWebView "
          + "IPC boundary and re-parsing it as CSS at the reload coalescer's ~33ms "
          + "cadence — ~30 MB/s while the user types — is the cost this whole change "
          + "exists to remove: \(carried)")
    }
  }

  // MARK: - composition: the two gates must AGREE on a real source switch

  /// `updateStrategy` rule 4 and `captureDecision` rule 1 are near-duplicates BY
  /// DESIGN, and both are required: one runs before the navigation exists, the other
  /// before the restore is consumed. They key on different state —
  /// `previewLiveDocuments[win]` (what is COMMITTED) versus `lastPreviewLoads[win]`
  /// (what was last DISPATCHED) — so nothing but a test stops them from drifting
  /// apart. If they ever disagreed, a cross-file link would either swap the new file's
  /// body into the old document, or restore the old file's offset into the new one.
  /// Both are H1.
  func testASourceSwitchIsBothAFullLoadAndADrop() {
    let previous = load(bufnr: sourceBufnr, tick: 5)
    let current = load(bufnr: otherBufnr, tick: 5)
    XCTAssertEqual(
      PreviewScrollSeam.updateStrategy(
        html: document(body: "<p>the other file</p>"),
        currentLoad: current,
        liveDocument: live(bufnr: sourceBufnr),
        loadInFlight: false,
        hasPendingFragment: false),
      .fullLoad,
      "the strategy gate must navigate on a source switch")
    XCTAssertEqual(
      PreviewScrollSeam.captureDecision(
        previousLoad: previous,
        currentLoad: current,
        cursor: knownCursor,
        cachedState: liveState,
        pendingRestore: nil),
      .drop,
      "and the capture gate — which the full path then consults, exactly as it does "
        + "today — must drop the previous file's offset. H1 is guaranteed DOUBLY, and "
        + "that redundancy is deliberate: neither gate may be removed on the strength "
        + "of the other")
  }

  // MARK: - composition: captureDecision is a FULL-LOAD-ONLY concern

  /// `captureDecision` is FROZEN by this change: no new case, no rule relaxed, and it
  /// is still called only where it is called today — immediately before
  /// `loadHTMLString`. The in-place path NEVER consults it, never reads or writes
  /// `pendingPreviewRestores`, and never emits `restoreJS`. That is not merely a
  /// convention: `updateStrategy` rule 2 (`loadInFlight`) makes an in-place update and
  /// a pending restore mutually exclusive per window BY CONSTRUCTION, because a
  /// pending restore exists only between `loadHTMLString` and its `didFinish`.
  ///
  /// The absence of a call cannot be asserted from a pure seam, so what is asserted
  /// structurally is the `.inPlace` result itself — reaching it is what proves the
  /// caller's `switch` took the branch that has no `captureDecision` in it. The second
  /// assertion records what the full path WOULD have captured, so a reader can see the
  /// two paths are answering different questions rather than duplicating one.
  func testAnOrdinaryEditIsInPlaceAndNeverConsultsCaptureDecision() {
    let previous = load(bufnr: sourceBufnr, tick: 5)
    let current = load(bufnr: sourceBufnr, tick: 6)
    let body = "<p>edited</p>"
    XCTAssertEqual(
      PreviewScrollSeam.updateStrategy(
        html: document(body: body),
        currentLoad: current,
        liveDocument: live(bufnr: sourceBufnr),
        loadInFlight: false,
        hasPendingFragment: false),
      .inPlace(body: body),
      "a keystroke in the same buffer takes the in-place branch, which issues "
        + "callAsyncJavaScript(__wolfUpdate) and touches NONE of the capture/restore "
        + "machinery")
    XCTAssertEqual(
      captured(
        PreviewScrollSeam.captureDecision(
          previousLoad: previous,
          currentLoad: current,
          cursor: knownCursor,
          cachedState: liveState,
          pendingRestore: nil)),
      PreviewScrollSeam.RestoreRequest(
        cursor: knownCursor, targetY: liveState.anchorTop, fallbackScrollY: liveState.scrollY),
      "captureDecision still answers this correctly and is still exercised on every "
        + "full load — first bind, source switch, pending fragment, in-flight load, "
        + "unsplittable HTML and failure recovery. It is unchanged, not retired")
  }

  // MARK: - inPlaceUpdateJS analysis helpers

  /// JS source with every string/template literal blanked out, so an identifier-shaped
  /// word inside a literal such as `'text/html'` cannot be mistaken for a variable.
  private func withoutStringLiterals(_ source: String) -> String {
    var out = ""
    var quote: Character?
    var escaped = false
    for c in source {
      guard let q = quote else {
        if c == "'" || c == "\"" || c == "`" {
          quote = c
        } else {
          out.append(c)
        }
        continue
      }
      if escaped {
        escaped = false
      } else if c == "\\" {
        escaped = true
      } else if c == q {
        quote = nil
      }
    }
    return out
  }

  /// Every identifier-shaped token in `source`, in order, paired with whether it is a
  /// MEMBER name — i.e. immediately preceded by a `.`, so it is a property lookup on
  /// some object rather than a name resolved against the enclosing scope.
  private func jsIdentifierTokens(_ source: String) -> [(name: String, isMember: Bool)] {
    func isStart(_ c: Character) -> Bool { c == "_" || c == "$" || c.isLetter }
    func isPart(_ c: Character) -> Bool { isStart(c) || c.isNumber }
    let chars = Array(source)
    var tokens: [(name: String, isMember: Bool)] = []
    var i = 0
    while i < chars.count {
      guard isStart(chars[i]) else {
        i += 1
        continue
      }
      var j = i
      while j < chars.count && isPart(chars[j]) { j += 1 }
      var k = i - 1
      while k >= 0 && chars[k].isWhitespace { k -= 1 }
      tokens.append((String(chars[i..<j]), k >= 0 && chars[k] == "."))
      i = j
    }
    return tokens
  }

  /// The names `js` resolves against its ENCLOSING scope: identifier tokens that are
  /// neither property lookups, nor reserved words, nor page globals, nor declared
  /// inside the snippet itself.
  ///
  /// For a `callAsyncJavaScript` body those must be exactly the keys of the caller's
  /// `arguments:` dictionary — an extra one is a `ReferenceError` at call time (and so
  /// a permanent silent fallback to full loads), and a missing one is an argument that
  /// is shipped and then ignored.
  private func freeIdentifiers(inJS js: String) -> Set<String> {
    let tokens = jsIdentifierTokens(withoutStringLiterals(js))
    let binders: Set<String> = ["var", "let", "const", "function", "catch"]
    var declared: Set<String> = []
    for (index, token) in tokens.enumerated() where index > 0 && !token.isMember {
      let previous = tokens[index - 1]
      if !previous.isMember && binders.contains(previous.name) {
        declared.insert(token.name)
      }
    }
    let reserved: Set<String> = [
      "async", "await", "break", "case", "catch", "class", "const", "continue",
      "default", "delete", "do", "else", "false", "finally", "for", "function", "if",
      "in", "instanceof", "let", "new", "null", "of", "return", "switch", "this",
      "throw", "true", "try", "typeof", "undefined", "var", "void", "while",
    ]
    let globals: Set<String> = [
      "Array", "Boolean", "JSON", "Number", "Object", "String", "document",
      "globalThis", "self", "window",
    ]
    return Set(
      tokens
        .filter { !$0.isMember }
        .map { $0.name }
        .filter { !reserved.contains($0) && !globals.contains($0) && !declared.contains($0) })
  }

  // MARK: - inPlaceUpdateJS: the entry point is guarded, with a falsy sentinel

  func testInPlaceUpdateJSGuardsOnTheEntryPointExisting() {
    let js = PreviewScrollSeam.inPlaceUpdateJS()
    let squeezed = String(js.filter { !$0.isWhitespace })
    XCTAssertTrue(
      js.contains("__wolfUpdate"),
      "the callAsyncJavaScript body must call the injected `__wolfUpdate` entry point: "
        + "\(js)")
    XCTAssertTrue(
      js.contains("!") || js.contains("&&"),
      "the body must short-circuit when `window.__wolfUpdate` is absent — a page that "
        + "is not ours, or a wrong content world — rather than throwing: \(js)")
    let sentinels = ["return0", ":0", "||0", "?0"]
    XCTAssertTrue(
      sentinels.contains { squeezed.contains($0) },
      "the absent-entry-point branch must return a FALSY `0` sentinel (one of "
        + "\(sentinels) ignoring whitespace), because that is what the Swift "
        + "completion handler bridges and routes to previewInPlaceUpdateFailed. "
        + "Returning undefined would be indistinguishable from a successful swap that "
        + "simply returned nothing: \(js)")
    XCTAssertTrue(
      js.contains("return"),
      "this string is a FUNCTION BODY, not an expression — it has to return its "
        + "result for the completion handler to see it: \(js)")
    XCTAssertFalse(
      js.contains("throw"),
      "a throw would surface as a .failure and trigger the fallback reload, but it "
        + "would also log a JS exception on every call in a wrong content world; the "
        + "sentinel is the designed signal: \(js)")
  }

  // MARK: - inPlaceUpdateJS: the anti-drift pin between body and `arguments:`

  func testInPlaceUpdateJSUsesExactlyTheDeclaredArgumentNames() {
    let names = PreviewScrollSeam.inPlaceUpdateArgumentNames
    let declared = Set([names.html, names.line, names.col])
    XCTAssertEqual(
      declared.count, 3,
      "the three argument names must be DISTINCT — a collision would silently drop one "
        + "of the three values from the arguments dictionary: \(declared)")
    XCTAssertFalse(
      declared.contains(""),
      "an empty argument name is not a valid JS identifier: \(declared)")

    let js = PreviewScrollSeam.inPlaceUpdateJS()
    XCTAssertEqual(
      freeIdentifiers(inJS: js), declared,
      "the body's free identifiers must be EXACTLY the exported argument names. The "
        + "caller builds its `arguments:` dictionary from the same constants, so this "
        + "is what makes the two provably unable to drift: an extra free name is a "
        + "ReferenceError on every keystroke (which degrades the feature to permanent "
        + "full loads, silently, with a green suite), and a missing one is a payload "
        + "shipped and ignored: \(js)")
  }

  // MARK: - inPlaceUpdateJS: the document travels as an ARGUMENT, never as source

  func testInPlaceUpdateJSInterpolatesNoDocument() {
    let js = PreviewScrollSeam.inPlaceUpdateJS()
    XCTAssertFalse(
      js.contains("<"),
      "the returned body must contain no `<` at all. `callAsyncJavaScript` takes real "
        + "arguments, so the ~200 KB body string is handed over as a VALUE — it is "
        + "never escaped into, or re-lexed as, JS source. A `<` here would mean some "
        + "markup got interpolated, which is both a quoting hazard and the exact cost "
        + "this path exists to avoid: \(js)")
    XCTAssertFalse(
      js.contains("wolf-markdown"),
      "nor may the body hard-code any part of render_html's wrapper: \(js)")
  }

  // MARK: - inPlaceUpdateJS: the string is a function BODY, not a whole function

  /// `callAsyncJavaScript(_:arguments:in:in:completionHandler:)` SYNTHESIZES the async
  /// function itself, from the `arguments:` keys plus this string as its BODY. A whole
  /// function expression here — `function (h, l, c) { … }`, `(h, l, c) => { … }`, or a
  /// parenthesized wrapper — is therefore rejected at CALL time, not at Swift compile
  /// time.
  ///
  /// This is the same silent-degradation class as the `.page` content world (OD-4),
  /// and the reason it needs its own test is that NOTHING else here would catch it:
  /// `testInPlaceUpdateJSUsesExactlyTheDeclaredArgumentNames` would happily pass a body
  /// wrapped in `function (h, l, c) { … }`, because the wrapper's own parameters are
  /// DECLARED names and so the free-identifier set is unchanged; the guard, sentinel
  /// and no-`<` tests would all still pass too. Every in-place update would fail,
  /// `previewInPlaceUpdateFailed` would clear `previewLiveDocuments[win]` and pin each
  /// window onto updateStrategy rule 3's full-load path, and the typing flicker would
  /// return in full — with a completely green test suite.
  func testInPlaceUpdateJSIsAFunctionBodyNotAWholeFunction() {
    let js = PreviewScrollSeam.inPlaceUpdateJS()
    let trimmed = js.drop { $0.isWhitespace }
    let consequence =
      "callAsyncJavaScript builds the async function AROUND this string and evaluates "
      + "it as that function's BODY, so a function expression is a syntax error at call "
      + "time. Every in-place update would then fail, previewInPlaceUpdateFailed would "
      + "clear previewLiveDocuments[win] and pin every window onto the full-load path, "
      + "and the typing flicker would come back WITH A COMPLETELY GREEN TEST SUITE"
    for wrapper in ["function", "async", "("] {
      XCTAssertFalse(
        trimmed.hasPrefix(wrapper),
        "the body must not START with `\(wrapper)` — every accidentally-wrapped form "
          + "(`function (h, l, c) { … }`, `async (h, l, c) => { … }`, "
          + "`(function (h, l, c) { … })(…)`) begins with one of those three tokens. "
          + consequence + ": \(js)")
    }
    XCTAssertFalse(
      js.contains("=>"),
      "nor may the body BE an arrow function, in any position. " + consequence
        + ": \(js)")
  }

  // MARK: - stateScriptJS region helper

  /// The whitespace-squeezed `stateScriptJS` text that is `__wolfUpdate`'s definition:
  /// from its `= function` up to the scroll listener that follows it.
  ///
  /// `__wolfUpdate` is defined AFTER `moveTo` and `__wolfRestore` and BEFORE the
  /// listener, so slicing this way keeps `__wolfRestore`'s statements — notably its
  /// `__wolfUserScrolled = false` — OUT of the region. That placement is also what
  /// keeps `testStateScriptJSFunnelsEveryScrollThroughMoveTo`'s "no `function` token
  /// between `moveTo=function` and the sole `scrollTo(`" analysis valid (OD-3).
  private func inPlaceUpdateRegion(
    _ squeezed: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) -> Substring? {
    guard let start = squeezed.range(of: "__wolfUpdate=function") else {
      XCTFail(
        "stateScriptJS must define `window.__wolfUpdate = function (html, line, col)` "
          + "at DOCUMENT START, beside __wolfFollow and __wolfRestore — a per-load "
          + "user script would mean tearing down and re-adding every injected script "
          + "on each keystroke: \(squeezed)",
        file: file, line: line)
      return nil
    }
    let tail = squeezed[start.upperBound...]
    guard let listener = tail.range(of: "addEventListener('scroll'") else {
      XCTFail(
        "__wolfUpdate must be defined BEFORE the scroll listener, so this test can "
          + "bound its body: \(squeezed)",
        file: file, line: line)
      return nil
    }
    return tail[tail.startIndex..<listener.lowerBound]
  }

  // MARK: - stateScriptJS: the in-place update entry point exists at document start

  func testStateScriptJSDefinesTheInPlaceUpdateEntryPoint() {
    let js = PreviewScrollSeam.stateScriptJS()
    XCTAssertTrue(
      js.contains("__wolfUpdate"),
      "the injected script must install the in-place update entry point: \(js)")
    XCTAssertTrue(
      js.contains("DOMParser"),
      "the incoming body is parsed with DOMParser — the SAME HTML parser a document "
        + "load uses, including the foreign-content rules that make `<math>` markup "
        + "parse correctly: \(js)")
    XCTAssertTrue(
      js.contains("parseFromString"),
      "DOMParser is used via parseFromString: \(js)")
    XCTAssertTrue(
      js.contains("text/html"),
      "the parse MIME type must be `text/html`, not an XML mode — the rendered body "
        + "is HTML-parser input, and an XML parse would reject it: \(js)")
    XCTAssertTrue(
      js.contains("createDocumentFragment"),
      "the parsed children are collected into a document fragment first: \(js)")
    XCTAssertTrue(
      js.contains("replaceChildren"),
      "the swap itself is ONE replaceChildren, so the whole update — swap and "
        + "re-anchor — happens in a single JavaScript task and no frame at scrollY 0 "
        + "can ever be painted: \(js)")
  }

  // MARK: - stateScriptJS: the swap must not spread thousands of nodes into apply()

  func testStateScriptJSInPlaceUpdateDoesNotUseApplyForTheSwap() {
    let js = PreviewScrollSeam.stateScriptJS()
    let squeezed = String(js.filter { !$0.isWhitespace })
    for hazard in ["replaceChildren.apply", ".apply(", "Array.from("] {
      XCTAssertFalse(
        squeezed.contains(hazard),
        "the swap must not go through `\(hazard)`. A long README has thousands of "
          + "top-level children, and Function.prototype.apply with a very large "
          + "argument list can throw RangeError — which would fail the update on "
          + "exactly the documents that flicker worst: \(js)")
    }
    guard let region = inPlaceUpdateRegion(squeezed) else { return }
    XCTAssertTrue(
      region.contains("while("),
      "the children are moved with a LOOP instead: \(js)")
    XCTAssertTrue(
      region.contains("firstChild"),
      "the loop drains the parsed body's firstChild until it is empty: \(js)")
    XCTAssertTrue(
      region.contains("appendChild("),
      "appendChild into the fragment ADOPTS each node into the live document "
        + "automatically, so no explicit document.adoptNode is needed: \(js)")
  }

  // MARK: - stateScriptJS: the anchor is read BEFORE the swap (the load-bearing order)

  /// The whole flicker fix is this ordering. `beforeTop` must be measured on the OLD
  /// document with the INCOMING `(line, col)` — not the stale `__wolfLine`/`__wolfCol`
  /// — so the freshly moved cursor is what stays visually stationary; reading
  /// `getBoundingClientRect()` there also forces the pending layout, so the value is
  /// exact. After the swap the anchor is resolved AGAIN in the new DOM and
  /// `scrollAnchorTo(afterEl, beforeTop)` pins it back to the same viewport offset —
  /// which is exactly the correction needed when content ABOVE the cursor changed
  /// height. Measure after the swap and the old offset is gone; re-anchor before it
  /// and you re-anchor to a node that is about to be discarded.
  func testStateScriptJSInPlaceUpdateReadsTheAnchorBeforeTheSwap() {
    let squeezed = String(PreviewScrollSeam.stateScriptJS().filter { !$0.isWhitespace })
    guard let region = inPlaceUpdateRegion(squeezed) else { return }
    guard let firstResolve = region.range(of: "resolve("),
      let measured = region.range(of: "getBoundingClientRect"),
      let swapped = region.range(of: "replaceChildren")
    else {
      return XCTFail(
        "__wolfUpdate must resolve the anchor, measure it with getBoundingClientRect "
          + "and then swap with replaceChildren: \(region)")
    }
    XCTAssertLessThan(
      firstResolve.lowerBound, measured.lowerBound,
      "the anchor must be RESOLVED before it is measured: \(region)")
    XCTAssertLessThan(
      measured.lowerBound, swapped.lowerBound,
      "the measurement must happen BEFORE replaceChildren — after the swap the old "
        + "element is detached and its viewport offset is unrecoverable, so the update "
        + "would have nothing to re-anchor to and the preview would sit wherever the "
        + "new content put it: \(region)")

    let afterSwap = region[swapped.upperBound...]
    guard let secondResolve = afterSwap.range(of: "resolve("),
      let follow = afterSwap.range(of: "__wolfFollow(")
    else {
      return XCTFail(
        "after the swap __wolfUpdate must resolve the anchor AGAIN in the new DOM and "
          + "then compose __wolfFollow on top: \(region)")
    }
    XCTAssertLessThan(
      secondResolve.lowerBound, follow.lowerBound,
      "the re-anchor must precede the follow, exactly mirroring __wolfRestore's "
        + "restore-then-follow composition: the re-anchor satisfies `the edited region "
        + "stays stationary`, and the follow is then a no-op unless the cursor left "
        + "the comfort band (and it also post()s, refreshing the Swift cache): "
        + "\(region)")
    XCTAssertTrue(
      afterSwap.contains("scrollAnchorTo(") || afterSwap.contains("moveTo("),
      "the re-anchor must reach the viewport through scrollAnchorTo/moveTo — the "
        + "single funnel that arms and clamps __wolfProgrammatic (OD-3): \(region)")
  }

  // MARK: - stateScriptJS: the in-place path must NOT clear the user-scroll flag

  /// `__wolfRestore` clears `__wolfUserScrolled` for one reason only: to re-open the
  /// gate on `restoreJS`'s `requestAnimationFrame` and `document.fonts.ready`
  /// re-applies. There are NO late re-applies on the in-place path — the fonts were
  /// never unloaded, and `getBoundingClientRect()` forces layout synchronously — so
  /// clearing the flag inside `__wolfUpdate` would buy nothing and would silently
  /// discard a genuine user scroll that the NEXT full load's restore then has to
  /// respect.
  func testStateScriptJSInPlaceUpdateDoesNotClearTheUserScrollFlag() {
    let squeezed = String(PreviewScrollSeam.stateScriptJS().filter { !$0.isWhitespace })
    guard let region = inPlaceUpdateRegion(squeezed) else { return }
    XCTAssertFalse(
      region.contains("__wolfUserScrolled"),
      "__wolfUpdate must not READ or WRITE the user-scroll flag at all: \(region)")

    // The two legitimate assignment sites, and only those: the document-start
    // initialization (so the first genuine user scroll is not swallowed) and
    // __wolfRestore's deliberate clear.
    let expectedAssignments = 2
    XCTAssertEqual(
      squeezed.components(separatedBy: "__wolfUserScrolled=false").count - 1,
      expectedAssignments,
      "there must be exactly \(expectedAssignments) `__wolfUserScrolled = false` "
        + "assignments in the whole script — the document-start initialization and "
        + "__wolfRestore's clear. A third would mean some other entry point (almost "
        + "certainly __wolfUpdate) started discarding user scrolls: \(squeezed)")
  }

  // MARK: - loadCommitOutcome fixtures (M1)

  /// The head string identifying the descriptor full load N stages in
  /// `pendingPreviewDocuments[win]` while it navigates.
  private let documentNHead = "CSS-N"
  /// The head string identifying the descriptor the NEXT coalescer tick's full load
  /// (N+1) overwrites that pending slot with, while N is STILL navigating.
  private let documentNPlus1Head = "CSS-N-PLUS-1"

  /// Load N's pending descriptor. The straddling descriptors carry deliberately
  /// DIFFERENT head lengths, purely as identity tags: in production the head is
  /// invariant (`updateStrategy` rule 6), but a test that cannot tell D_N from D_N+1
  /// cannot tell a correct commit from M1's commit of the wrong one.
  private var documentN: PreviewScrollSeam.LiveDocument {
    live(bufnr: sourceBufnr, head: documentNHead)
  }

  /// Load N+1's pending descriptor.
  private var documentNPlus1: PreviewScrollSeam.LiveDocument {
    live(bufnr: sourceBufnr, head: documentNPlus1Head)
  }

  /// `loadCommitOutcome` with every argument labelled at the call site, mirroring the
  /// `strategy(...)` helper above.
  private func commitOutcome(
    remaining: Int,
    pending: PreviewScrollSeam.LiveDocument?,
    succeeded: Bool
  ) -> PreviewScrollSeam.LoadCommitOutcome {
    PreviewScrollSeam.loadCommitOutcome(
      remaining: remaining, pending: pending, succeeded: succeeded)
  }

  // MARK: - T-E: remaining > 0 is .stillOutstanding, whatever the callback reported

  /// A SUPERSEDED load's callback must neither commit nor discard anything.
  /// `.stillOutstanding` carries NO payload at all — no descriptor comes back out of
  /// the seam — which is precisely what makes a contract-following caller leave BOTH
  /// the live slot and the pending slot exactly as they were, so load N+1's descriptor
  /// survives load N's callback (M1 step 4, where the old `Set` destructively consumed
  /// it). WebKit reports a superseded load as a FAILURE, so `succeeded: false` is the
  /// COMMON case here, not the exotic one; both must answer identically, because at
  /// `remaining > 0` the callback simply is not the one that decides.
  func testLoadCommitOutcomeIsStillOutstandingWhileLoadsRemain() {
    let remainingCounts = [1, 2, 5]
    let pendings: [PreviewScrollSeam.LiveDocument?] = [documentN, documentNPlus1, nil]
    for remaining in remainingCounts {
      for succeeded in [true, false] {
        for pending in pendings {
          XCTAssertEqual(
            commitOutcome(remaining: remaining, pending: pending, succeeded: succeeded),
            .stillOutstanding,
            "with \(remaining) load(s) still outstanding the callback decides NOTHING "
              + "(succeeded: \(succeeded), pending: \(String(describing: pending))). The "
              + "case is payload-free by design: the caller is handed no descriptor to "
              + "commit and is told nothing that would let it clear the pending slot")
        }
      }
    }
  }

  // MARK: - T-E: the LAST load, succeeded => .commit(pending)

  /// The only outcome that may move a descriptor into `previewLiveDocuments[win]`, and
  /// it must carry the descriptor of the load that actually COMMITTED — not some older
  /// one. `.commit` is what makes `updateStrategy` rule 3 stop forcing full loads.
  func testLoadCommitOutcomeCommitsThePendingDescriptorOnTheLastSuccess() {
    let outcome = commitOutcome(remaining: 0, pending: documentNPlus1, succeeded: true)
    XCTAssertEqual(
      outcome, .commit(documentNPlus1),
      "the last outstanding load succeeded, so the pending descriptor becomes live")
    guard case .commit(let committed) = outcome else {
      return XCTFail("expected a .commit outcome, got \(outcome)")
    }
    XCTAssertEqual(
      committed?.sourceBufnr, sourceBufnr,
      "the committed descriptor must be the EXACT pending value, passed through whole")
    XCTAssertEqual(
      committed?.headLength, documentNPlus1.headLength,
      "including its headLength — rule 6 compares against it, so a wrong one would "
        + "either wedge the preview into permanent full loads or authorize a body-only "
        + "swap against stale CSS")
    XCTAssertNotEqual(
      outcome, .commit(documentN),
      "committing the SUPERSEDED load's descriptor is the mirror image of M1: the live "
        + "slot would describe a document the web view is not showing")
  }

  // MARK: - T-E: the LAST load, succeeded, nothing splittable => .commit(nil)

  /// `splitDocument` returns nil for the `""` a failed `render_preview` yields, so the
  /// caller stages NO pending descriptor even though the navigation itself succeeds.
  /// The commit must then clear the live slot: an unsplittable document must never
  /// become a live document, or rule 6 would compare against a headLength that
  /// describes nothing on screen.
  func testLoadCommitOutcomeCommitsNilForAnUnsplittableDocument() {
    XCTAssertEqual(
      commitOutcome(remaining: 0, pending: nil, succeeded: true), .commit(nil),
      "a successful load with no splittable document commits `nil`, which positively "
        + "CLEARS the live slot and sends the next update back down the full-load path")
  }

  // MARK: - T-E: the LAST load, FAILED => .discard, which is NOT .commit(nil)

  /// The heart of the fix. A failed navigation never commits, so WebKit keeps
  /// displaying the PREVIOUS document and the previous live descriptor stays accurate —
  /// `.discard` therefore leaves the live slot UNTOUCHED and only clears the pending
  /// descriptor that will now never commit. Conflating it with `.commit(nil)` is
  /// exactly how M1 step 7 wiped a perfectly good live document and forced a pointless
  /// extra full load; the two cases are kept distinct so a caller's mechanical `switch`
  /// physically cannot make that mistake.
  func testLoadCommitOutcomeDiscardsOnTheLastFailureAndNeverCommits() {
    let outcome = commitOutcome(remaining: 0, pending: documentN, succeeded: false)
    XCTAssertEqual(
      outcome, .discard,
      "the last outstanding load FAILED, so nothing is committed and the pending "
        + "descriptor is dropped")
    XCTAssertNotEqual(
      outcome, .commit(nil),
      "`.discard` is deliberately NOT `.commit(nil)`: the first leaves a good live "
        + "document in place (the web view still shows it), the second would erase it. "
        + "That erasure IS M1 step 7")
    XCTAssertNotEqual(
      outcome, .commit(documentN),
      "and a failed navigation certainly may not promote the descriptor of a document "
        + "that never rendered")
    XCTAssertEqual(
      commitOutcome(remaining: 0, pending: nil, succeeded: false), .discard,
      "a failure with nothing pending is still a discard — the caller must not be "
        + "handed a `.commit` it would apply to the live slot")
  }

  // MARK: - T-E / M1: the shipping blocker — two loads straddling one page load

  /// THE regression guard for M1, walked step by step through `loadCommitOutcome` with
  /// `remaining` / live / pending modelled as locals and the caller's whole mechanical
  /// `switch` reproduced verbatim.
  ///
  /// The failure it guards: the reload coalescer serializes to one reload per ~33ms and
  /// does NOT wait for `didFinish`, so full load N+1 is issued while N is still
  /// navigating and OVERWRITES the pending descriptor. With the old `Set<Int64>`,
  /// `didFinish(N)` — or `didFail(N)`, since WebKit reports a superseded load as a
  /// failure — removed the window from the set, so the in-flight flag read CLEAR while
  /// N+1 was still navigating, AND that same callback consumed N+1's descriptor into
  /// the live slot. The next tick then saw rule 2 false and rule 3 false, chose
  /// `.inPlace`, and swapped a body into the document N+1 was about to replace; N+1
  /// committed, discarded the swap and reverted to older content — a visible flicker,
  /// the exact symptom this whole change exists to remove — and N+1's own callback
  /// finally nil'd the live descriptor, forcing one more pointless full load.
  ///
  /// Run for BOTH flavors of N's callback, because which one WebKit delivers for a
  /// superseded load is not under our control and must not change the answer.
  func testStraddlingNavigationsNeverCommitEarlyAndNeverWipeTheLiveDocument() {
    for supersededSucceeded in [true, false] {
      let flavor = supersededSucceeded ? "didFinish(N)" : "didFail(N)"
      let alreadyShowing = live(bufnr: sourceBufnr)

      // Step 0: the preview already shows a committed document, nothing in flight.
      var remaining = 0
      var liveSlot: PreviewScrollSeam.LiveDocument? = alreadyShowing
      var pendingSlot: PreviewScrollSeam.LiveDocument?

      // The caller's ENTIRE bookkeeping: decrement first, hand the REMAINING count to
      // the seam, then apply its answer. `PaneContainerView` must contain nothing else.
      func callback(succeeded: Bool) {
        remaining -= 1
        switch PreviewScrollSeam.loadCommitOutcome(
          remaining: remaining, pending: pendingSlot, succeeded: succeeded)
        {
        case .stillOutstanding:
          break  // leave the live AND the pending descriptor ALONE
        case .commit(let document):
          liveSlot = document
          pendingSlot = nil
        case .discard:
          pendingSlot = nil
        }
      }

      // Step 1: full load N is issued for a long document.
      remaining += 1
      pendingSlot = documentN
      XCTAssertEqual(remaining, 1, "\(flavor): load N is outstanding")
      XCTAssertEqual(
        liveSlot, alreadyShowing,
        "\(flavor): N has not committed, so the web view still shows the OLD document")
      XCTAssertEqual(pendingSlot, documentN, "\(flavor): N staged its descriptor")

      // Step 2: a coalescer tick issues full load N+1 before N finished; the pending
      // descriptor is overwritten and the COUNT — not a set membership — goes to 2.
      remaining += 1
      pendingSlot = documentNPlus1
      let bothOutstanding = 2
      XCTAssertEqual(
        remaining, bothOutstanding,
        "\(flavor): a COUNT distinguishes two straddling loads; the old Set could not")
      XCTAssertEqual(liveSlot, alreadyShowing, "\(flavor): still nothing committed")
      XCTAssertEqual(
        pendingSlot, documentNPlus1, "\(flavor): N+1's descriptor replaced N's")

      // Step 3: N's callback lands while N+1 is still navigating.
      callback(succeeded: supersededSucceeded)
      XCTAssertEqual(
        remaining, 1,
        "\(flavor): N+1 is STILL outstanding, so the in-flight count stays non-zero and "
          + "an in-place update remains forbidden. This is the assertion the Set-based "
          + "bookkeeping could not make")
      XCTAssertEqual(
        liveSlot, alreadyShowing,
        "\(flavor): a superseded load's callback commits NOTHING — the live descriptor "
          + "must still describe what the web view is actually displaying")
      XCTAssertEqual(
        pendingSlot, documentNPlus1,
        "\(flavor): and it must NOT destructively consume N+1's pending descriptor, "
          + "which N+1's own callback still needs (M1 step 4)")

      // Steps 4-5: the next coalescer tick. Every other full-load rule is satisfied —
      // same buffer as the live document, matching head length, no pending fragment —
      // so rule 2 (a load in flight) is the ONLY thing standing between the preview and
      // the body swap that M1 performed into a doomed document.
      XCTAssertEqual(
        strategy(
          html: document(body: "<p>a third keystroke</p>"),
          bufnr: sourceBufnr,
          liveDocument: liveSlot,
          loadInFlight: remaining > 0),
        .fullLoad,
        "\(flavor): with N+1 outstanding the tick must NOT choose .inPlace. M1 chose it "
          + "here, swapped a body into the document N+1 was about to replace, and the "
          + "commit then reverted the swap — the visible flicker")

      // Step 6: N+1 commits.
      callback(succeeded: true)
      XCTAssertEqual(remaining, 0, "\(flavor): nothing is outstanding any more")
      XCTAssertEqual(
        liveSlot, documentNPlus1,
        "\(flavor): M1 step 7 — the live slot must hold D_N+1, NOT nil. The Set-based "
          + "code had already consumed the descriptor in step 3, so this callback set "
          + "the live slot to nil and forced a pointless extra full load")
      XCTAssertNil(
        pendingSlot, "\(flavor): the descriptor moved to the live slot, so nothing pends")

      // Step 7: the very next edit is therefore the cheap in-place swap this whole
      // change exists to enable, instead of another ~965 KiB navigation.
      let nextBody = "<p>after the straddle</p>"
      XCTAssertEqual(
        strategy(
          html: document(head: documentNPlus1Head, body: nextBody),
          bufnr: sourceBufnr,
          liveDocument: liveSlot,
          loadInFlight: remaining > 0),
        .inPlace(body: nextBody),
        "\(flavor): a correct live descriptor is what lets rule 3 and rule 6 pass; "
          + "wiping it in step 6 is what made M1 pay for one more full load")
    }
  }

  // MARK: - T-F: the three outcomes are mutually distinguishable values

  /// A caller's `switch` is only as safe as the type's ability to tell its cases apart.
  /// `.discard` and `.commit(nil)` are the pair that matters — one preserves a live
  /// document, the other erases it — but all three are pinned, so no future
  /// `Equatable` hand-rolling (or a `commit` payload made non-optional and defaulted)
  /// can quietly collapse two of them into one.
  func testLoadCommitOutcomeCasesAreMutuallyDistinguishable() {
    let discard = PreviewScrollSeam.LoadCommitOutcome.discard
    let stillOutstanding = PreviewScrollSeam.LoadCommitOutcome.stillOutstanding
    let commitNothing = PreviewScrollSeam.LoadCommitOutcome.commit(nil)
    XCTAssertNotEqual(
      discard, stillOutstanding,
      "`.discard` ends the load and clears the pending descriptor; "
        + "`.stillOutstanding` must change nothing at all")
    XCTAssertNotEqual(
      discard, commitNothing,
      "the M1 distinction: a FAILED last load leaves the live document alone, a "
        + "SUCCESSFUL one with nothing splittable erases it")
    XCTAssertNotEqual(
      stillOutstanding, commitNothing,
      "a superseded load's callback must never be mistaken for the commit of an "
        + "unsplittable document")
    XCTAssertNotEqual(
      commitNothing, .commit(documentN),
      "and the commit payload participates in equality, so committing a descriptor is "
        + "distinguishable from clearing the live slot")
  }
}
