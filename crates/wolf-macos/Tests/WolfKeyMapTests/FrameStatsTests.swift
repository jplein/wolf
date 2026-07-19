//
//  FrameStatsTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.FrameStats`.
//
//  These exercise the frame-time aggregation seam the coder adds in
//  `Sources/WolfKeyMap/FrameStats.swift` (the frame-time instrumentation plan).
//  Like `PaneGeometryTests.swift` and `StatusbarLayoutTests.swift`, they import
//  ONLY Foundation-level test machinery (no AppKit, no Dispatch timers, no live
//  Neovim, no window server) and stay in the headless `WolfKeyMap` module.
//
//  The process-global glue (`Sources/WolfMacOS/FrameMetrics.swift`: the once-only
//  static `enabled`, `Timer` scheduling, stderr / file emission, the
//  `dispatchPrecondition` main-thread guard) and every instrumentation call site
//  are window-server-bound and stay compile-verified + manual-smoke only.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor)
//
//    public enum FrameStatsBucket: Int, CaseIterable {
//        case controllerWake = 0   // label "ctrl.wake"
//        case containerWake  = 1   // label "cont.wake"
//        case draw           = 2   // label "draw"
//        case previewReload  = 3   // label "prev.reload"
//        case previewRender  = 4   // label "prev.render"
//        case previewLoadHTML = 5  // label "prev.html"
//        public var label: String { ... }
//    }
//
//    public struct FrameStats {
//        public static let sampleCapacity: Int  // 4096
//
//        public struct BucketSummary {
//            public let bucket: FrameStatsBucket
//            public let count: Int
//            public let meanNanos: UInt64
//            public let p50Nanos: UInt64
//            public let p95Nanos: UInt64
//            public let maxNanos: UInt64
//            public let truncated: Bool
//            public init(bucket:count:meanNanos:p50Nanos:p95Nanos:maxNanos:truncated:)
//        }
//
//        public struct Report {
//            public let index: Int
//            public let elapsedSeconds: Double
//            public let buckets: [BucketSummary]
//            public let keystrokes: Int
//            public let wakes: Int
//            public let wakesPerKeystroke: Double?
//            public init(index:elapsedSeconds:buckets:keystrokes:wakes:wakesPerKeystroke:)
//        }
//
//        public init()
//        public mutating func record(_ bucket: FrameStatsBucket, nanos: UInt64)
//        public mutating func recordKeystroke()
//        public mutating func recordWake()
//        public mutating func reset()
//        public func summarize(index: Int, elapsedSeconds: Double) -> Report
//        public func retainedSampleCount(_ bucket: FrameStatsBucket) -> Int
//        public static func percentile(_ sorted: [UInt64], _ p: Double) -> UInt64
//        public static func delta(start: UInt64, end: UInt64) -> UInt64
//        public static func format(_ report: Report) -> String
//        public static func isEnabled(env: [String: String]) -> Bool
//        public static func reportInterval(env: [String: String]) -> TimeInterval
//    }
//
//  ## Decisions this test PINS (the coder must match the test, not vice versa)
//
//  * `meanNanos` is INTEGER division `sumNanos / UInt64(count)`, truncating; a
//    zero-count bucket yields `0` (and is omitted from `summarize`).
//  * `percentile` is NEAREST-RANK:
//        index = clamp(Int(ceil(p * Double(n))) - 1, 0, n - 1)
//    over the ALREADY-SORTED input; empty input returns `0`.
//  * `count` / `sumNanos` / `maxNanos` are EXACT and unbounded; only the
//    percentile window is bounded to the last `sampleCapacity` samples.
//    `truncated == (count > sampleCapacity)`.
//  * `format` output layout. The plan's sample report block is internally
//    inconsistent (its header row and its data rows disagree by one column of
//    padding), so this test pins ONE self-consistent layout that reproduces the
//    plan's header line EXACTLY:
//
//        prefix         "wolf-frame-stats " on every line
//        bucket label   left-aligned,  width 14
//        count          right-aligned, width  5  (with a "*" suffix when truncated)
//        mean/p50/p95/max  right-aligned, width 9 each, milliseconds, 2 decimals
//
//    Header:  "wolf-frame-stats #<index>  window=<elapsed, 1 decimal>s"
//    Footer:  "wolf-frame-stats keys=<k> wakes=<w> wakes/key=<ratio, 2 decimals>"
//             with "wakes/key=n/a" when `wakesPerKeystroke` is nil.
//    Buckets are emitted in ascending `rawValue` order regardless of the order
//    they appear in `Report.buckets`. Every line (including the last) ends in "\n".
//

import XCTest

@testable import WolfKeyMap

final class FrameStatsTests: XCTestCase {

  // MARK: - Constants

  /// Nanoseconds per millisecond, for building readable fixtures.
  private let nsPerMs: UInt64 = 1_000_000

  /// Tolerance for the `Double` ratio assertions.
  private let eps: Double = 1e-9

  /// The default report interval the plan pins for `WOLF_FRAME_STATS_INTERVAL`.
  private let defaultInterval: TimeInterval = 5.0

  // MARK: - 0. Bucket discriminants + labels
  //
  // The raw values are the storage indices (array-indexed, no dict hashing) and
  // the labels are the report-time strings, so both are part of the contract.

  func testBucketRawValuesAndLabels() {
    let cases: [(FrameStatsBucket, Int, String)] = [
      (.controllerWake, 0, "ctrl.wake"),
      (.containerWake, 1, "cont.wake"),
      (.draw, 2, "draw"),
      (.previewReload, 3, "prev.reload"),
      (.previewRender, 4, "prev.render"),
      (.previewLoadHTML, 5, "prev.html"),
    ]
    for (bucket, raw, label) in cases {
      XCTAssertEqual(bucket.rawValue, raw, "\(label): rawValue == \(raw)")
      XCTAssertEqual(bucket.label, label, "raw \(raw): label == \"\(label)\"")
    }
    XCTAssertEqual(
      FrameStatsBucket.allCases.count, cases.count,
      "allCases covers exactly the \(cases.count) pinned buckets")
    XCTAssertEqual(
      FrameStatsBucket.allCases.map { $0.rawValue }, Array(0..<cases.count),
      "allCases is in ascending rawValue order with no gaps (it indexes the storage array)")
  }

  // MARK: - 1. Env parsing

  func testIsEnabledTable() {
    // (env dictionary, expected, description)
    let cases: [([String: String], Bool, String)] = [
      ([:], false, "absent -> false"),
      (["WOLF_FRAME_STATS": ""], false, "empty string -> false"),
      (["WOLF_FRAME_STATS": "   "], false, "whitespace only -> false"),
      (["WOLF_FRAME_STATS": "1"], true, "\"1\" -> true"),
      (["WOLF_FRAME_STATS": "true"], true, "\"true\" -> true"),
      (["WOLF_FRAME_STATS": "TRUE"], true, "\"TRUE\" -> true (case-insensitive)"),
      (["WOLF_FRAME_STATS": "Yes"], true, "\"Yes\" -> true (case-insensitive)"),
      (["WOLF_FRAME_STATS": "on"], true, "\"on\" -> true"),
      (["WOLF_FRAME_STATS": " on "], true, "\" on \" -> true (trimmed)"),
      (["WOLF_FRAME_STATS": "0"], false, "\"0\" -> false"),
      (["WOLF_FRAME_STATS": "false"], false, "\"false\" -> false"),
      (["WOLF_FRAME_STATS": "no"], false, "\"no\" -> false"),
      (["WOLF_FRAME_STATS": "off"], false, "\"off\" -> false"),
      (["WOLF_FRAME_STATS": "maybe"], false, "an unrecognized value -> false"),
      (["WOLF_FRAME_STATS_INTERVAL": "1"], false, "only the interval var set -> false"),
    ]
    for (env, want, desc) in cases {
      XCTAssertEqual(FrameStats.isEnabled(env: env), want, "isEnabled: \(desc)")
    }
  }

  func testReportIntervalTable() {
    // (env dictionary, expected seconds, description)
    let cases: [([String: String], TimeInterval, String)] = [
      ([:], 5.0, "absent -> the 5.0s default"),
      (["WOLF_FRAME_STATS_INTERVAL": ""], 5.0, "empty -> default"),
      (["WOLF_FRAME_STATS_INTERVAL": "2.5"], 2.5, "\"2.5\" -> 2.5"),
      (["WOLF_FRAME_STATS_INTERVAL": "10"], 10.0, "an integer string parses as Double"),
      (["WOLF_FRAME_STATS_INTERVAL": "0"], 5.0, "zero is non-positive -> default"),
      (["WOLF_FRAME_STATS_INTERVAL": "-1"], 5.0, "negative -> default"),
      (["WOLF_FRAME_STATS_INTERVAL": "abc"], 5.0, "unparseable -> default"),
    ]
    for (env, want, desc) in cases {
      XCTAssertEqual(
        FrameStats.reportInterval(env: env), want, accuracy: eps, "reportInterval: \(desc)")
    }

    XCTAssertEqual(
      FrameStats.reportInterval(env: [:]), defaultInterval, accuracy: eps,
      "the pinned default is 5.0 seconds")
  }

  // MARK: - 2. Basic accumulation

  func testBasicAccumulationPinsIntegerMean() {
    // Sum 10 + 20 + 31 == 61 over 3 samples. The mean is INTEGER division:
    // 61 / 3 == 20 (truncated), NOT 20.33 and NOT 21.
    var stats = FrameStats()
    for nanos: UInt64 in [10, 20, 31] {
      stats.record(.draw, nanos: nanos)
    }

    let report = stats.summarize(index: 1, elapsedSeconds: 5.0)
    guard let draw = report.buckets.first(where: { $0.bucket == .draw }) else {
      return XCTFail("the .draw bucket must appear in the report after 3 samples")
    }

    XCTAssertEqual(draw.count, 3, "count is the exact number of samples")
    XCTAssertEqual(draw.meanNanos, 20, "mean is truncating integer division: 61 / 3 == 20")
    XCTAssertEqual(draw.maxNanos, 31, "max is the largest sample")
    XCTAssertFalse(draw.truncated, "3 samples is far below sampleCapacity")
  }

  func testSingleSampleCollapsesToOneValue() {
    // With exactly one sample every statistic is that sample.
    var stats = FrameStats()
    stats.record(.previewReload, nanos: 42 * nsPerMs)

    let report = stats.summarize(index: 1, elapsedSeconds: 5.0)
    guard let reload = report.buckets.first(where: { $0.bucket == .previewReload }) else {
      return XCTFail("the .previewReload bucket must appear in the report")
    }

    XCTAssertEqual(reload.count, 1, "one sample")
    XCTAssertEqual(reload.meanNanos, 42 * nsPerMs, "mean == the single sample")
    XCTAssertEqual(reload.p50Nanos, 42 * nsPerMs, "p50 == the single sample")
    XCTAssertEqual(reload.p95Nanos, 42 * nsPerMs, "p95 == the single sample")
    XCTAssertEqual(reload.maxNanos, 42 * nsPerMs, "max == the single sample")
  }

  // MARK: - 3. Percentile math (pinned nearest-rank)

  func testPercentileNearestRankTable() {
    // Each case: (already-sorted input, p, expected, description). The pinned
    // algorithm is index = clamp(Int(ceil(p * n)) - 1, 0, n - 1).
    let oneToHundred: [UInt64] = (1...100).map { UInt64($0) }
    let cases: [([UInt64], Double, UInt64, String)] = [
      ([], 0.5, 0, "empty p50 -> 0"),
      ([], 0.95, 0, "empty p95 -> 0"),
      ([7], 0.5, 7, "single element p50 -> that element"),
      ([7], 0.95, 7, "single element p95 -> that element"),
      ([10, 20], 0.5, 10, "two elements p50: ceil(0.5*2)-1 == 0 -> 10"),
      ([10, 20], 0.95, 20, "two elements p95: ceil(1.9)-1 == 1 -> 20"),
      ([10, 20, 30], 0.5, 20, "three elements p50: ceil(1.5)-1 == 1 -> 20"),
      ([10, 20, 30], 0.95, 30, "three elements p95: ceil(2.85)-1 == 2 -> 30"),
      ([7, 7, 7, 7], 0.5, 7, "all-equal p50 -> 7"),
      ([7, 7, 7, 7], 0.95, 7, "all-equal p95 -> 7"),
      (oneToHundred, 0.5, 50, "1...100 p50: ceil(50)-1 == 49 -> 50"),
      (oneToHundred, 0.95, 95, "1...100 p95: ceil(95)-1 == 94 -> 95"),
      (oneToHundred, 1.0, 100, "1...100 p100: ceil(100)-1 == 99 -> 100"),
      (oneToHundred, 0.0, 1, "p == 0 clamps the -1 index up to 0 -> the smallest sample"),
      (oneToHundred, 0.001, 1, "a sub-rank p rounds UP to rank 1 -> 1"),
    ]
    for (sorted, p, want, desc) in cases {
      XCTAssertEqual(FrameStats.percentile(sorted, p), want, "percentile: \(desc)")
    }
  }

  func testPercentileP95NeverExceedsMax() {
    // Structural invariant across a sweep: the nearest-rank index never runs off
    // the end of the array, so p95 <= the last (largest) element.
    for n in 1...50 {
      let sorted: [UInt64] = (1...n).map { UInt64($0) * 3 }
      let p95 = FrameStats.percentile(sorted, 0.95)
      let p50 = FrameStats.percentile(sorted, 0.5)
      guard let last = sorted.last, let first = sorted.first else {
        return XCTFail("n=\(n): the fixture is non-empty by construction")
      }
      XCTAssertLessThanOrEqual(p95, last, "n=\(n): p95 <= max")
      XCTAssertGreaterThanOrEqual(p95, first, "n=\(n): p95 >= min")
      XCTAssertLessThanOrEqual(p50, p95, "n=\(n): p50 <= p95")
    }
  }

  // MARK: - 4. Bucket isolation

  func testRecordingOneBucketLeavesOthersEmpty() {
    // Recording .draw must not leak into any other bucket, and summarize must
    // OMIT zero-count buckets entirely (an idle bucket prints no row).
    var stats = FrameStats()
    stats.record(.draw, nanos: 5 * nsPerMs)
    stats.record(.draw, nanos: 7 * nsPerMs)

    let report = stats.summarize(index: 1, elapsedSeconds: 5.0)

    XCTAssertEqual(report.buckets.count, 1, "only the recorded bucket is summarized")
    XCTAssertEqual(report.buckets.first?.bucket, .draw, "and it is .draw")
    XCTAssertNil(
      report.buckets.first(where: { $0.bucket == .previewRender }),
      ".previewRender was never recorded, so it is omitted")
    XCTAssertEqual(
      FrameStats.percentile([], 0.5), 0, "an unrecorded bucket has no samples to percentile")
  }

  func testMultipleBucketsAccumulateIndependently() {
    var stats = FrameStats()
    stats.record(.previewRender, nanos: 20 * nsPerMs)
    stats.record(.previewRender, nanos: 30 * nsPerMs)
    stats.record(.previewLoadHTML, nanos: 4 * nsPerMs)

    let report = stats.summarize(index: 1, elapsedSeconds: 5.0)
    XCTAssertEqual(report.buckets.count, 2, "exactly the two recorded buckets appear")

    guard
      let render = report.buckets.first(where: { $0.bucket == .previewRender }),
      let html = report.buckets.first(where: { $0.bucket == .previewLoadHTML })
    else {
      return XCTFail("both recorded buckets must be present")
    }
    XCTAssertEqual(render.count, 2, ".previewRender saw 2 samples")
    XCTAssertEqual(render.maxNanos, 30 * nsPerMs, ".previewRender max is its own")
    XCTAssertEqual(html.count, 1, ".previewLoadHTML saw 1 sample")
    XCTAssertEqual(html.maxNanos, 4 * nsPerMs, ".previewLoadHTML max is its own")
  }

  // MARK: - 5. Ring bound + truncation

  func testRingBoundKeepsExactCountAndWindowedPercentiles() {
    // Feed a DISTINGUISHABLE pattern: `overflow` tiny samples first, then exactly
    // `sampleCapacity` large samples. If the percentile window were wrong (e.g.
    // the FIRST capacity samples, or all of them) the p50 would be dragged down
    // by the tiny prefix, so a wrong window is detectable.
    let capacity = FrameStats.sampleCapacity
    let overflow = 100
    let base: UInt64 = 1000

    var stats = FrameStats()
    for _ in 0..<overflow {
      stats.record(.containerWake, nanos: 1)
    }
    for i in 0..<capacity {
      stats.record(.containerWake, nanos: base + UInt64(i))
    }

    let report = stats.summarize(index: 1, elapsedSeconds: 5.0)
    guard let wake = report.buckets.first(where: { $0.bucket == .containerWake }) else {
      return XCTFail("the .containerWake bucket must appear in the report")
    }

    // count / max stay EXACT and unbounded.
    XCTAssertEqual(
      wake.count, overflow + capacity, "count is exact and unbounded by the ring")
    XCTAssertEqual(
      wake.maxNanos, base + UInt64(capacity - 1),
      "max is tracked OUTSIDE the ring, so it is exact")
    XCTAssertTrue(wake.truncated, "count > sampleCapacity marks the bucket truncated")

    // The retained window is exactly the LAST `capacity` samples: base ..< base+capacity.
    // Nearest-rank over that sorted window:
    //   p50 index = ceil(0.5 * capacity) - 1
    //   p95 index = ceil(0.95 * capacity) - 1
    let p50Index = Int((0.5 * Double(capacity)).rounded(.up)) - 1
    let p95Index = Int((0.95 * Double(capacity)).rounded(.up)) - 1
    XCTAssertEqual(
      wake.p50Nanos, base + UInt64(p50Index),
      "p50 reflects ONLY the last sampleCapacity samples (no tiny-prefix drag)")
    XCTAssertEqual(
      wake.p95Nanos, base + UInt64(p95Index),
      "p95 reflects ONLY the last sampleCapacity samples")
    XCTAssertGreaterThan(
      wake.p50Nanos, 1, "a p50 of 1 would mean the tiny prefix was still in the window")

    // The mean uses the EXACT sum over all samples, not the window.
    let tinySum = UInt64(overflow)
    let largeSum = (0..<capacity).reduce(UInt64(0)) { $0 + base + UInt64($1) }
    XCTAssertEqual(
      wake.meanNanos, (tinySum + largeSum) / UInt64(overflow + capacity),
      "mean uses the exact unbounded sum / exact count")

    // The retained storage never grows past the bound.
    XCTAssertEqual(
      stats.retainedSampleCount(.containerWake), capacity,
      "the ring retains exactly sampleCapacity samples once saturated")
    XCTAssertLessThanOrEqual(
      stats.retainedSampleCount(.containerWake), FrameStats.sampleCapacity,
      "the retained array never exceeds sampleCapacity")
  }

  func testExactlyCapacitySamplesIsNotTruncated() {
    // The truncation boundary is INCLUSIVE-safe: exactly `sampleCapacity`
    // samples all fit in the window, so `truncated` stays false.
    let capacity = FrameStats.sampleCapacity
    var stats = FrameStats()
    for i in 0..<capacity {
      stats.record(.draw, nanos: UInt64(i + 1))
    }

    let report = stats.summarize(index: 1, elapsedSeconds: 5.0)
    guard let draw = report.buckets.first(where: { $0.bucket == .draw }) else {
      return XCTFail("the .draw bucket must appear in the report")
    }
    XCTAssertEqual(draw.count, capacity, "count == sampleCapacity")
    XCTAssertFalse(draw.truncated, "count == sampleCapacity is NOT truncated (only > is)")
    XCTAssertEqual(
      stats.retainedSampleCount(.draw), capacity, "the ring is exactly full, not wrapped")
  }

  func testRetainedSampleCountBelowCapacity() {
    var stats = FrameStats()
    for i in 0..<10 {
      stats.record(.previewReload, nanos: UInt64(i))
    }
    XCTAssertEqual(
      stats.retainedSampleCount(.previewReload), 10,
      "below capacity the retained count is the sample count")
    XCTAssertEqual(
      stats.retainedSampleCount(.draw), 0, "an untouched bucket retains nothing")
  }

  // MARK: - 6. Ratio math

  func testWakesPerKeystrokeTable() {
    // (keystrokes, wakes, expected ratio or nil, description)
    let cases: [(Int, Int, Double?, String)] = [
      (0, 0, nil, "no keystrokes -> nil (divide-by-zero edge)"),
      (0, 7, nil, "wakes with no keystrokes -> still nil"),
      (10, 30, 3.0, "30 wakes / 10 keys -> 3.0"),
      (5, 0, 0.0, "0 wakes / 5 keys -> 0.0"),
      (48, 142, 142.0 / 48.0, "the plan's sample ratio"),
      (4, 2, 0.5, "fewer wakes than keys -> a sub-1 ratio"),
    ]
    for (keys, wakes, want, desc) in cases {
      var stats = FrameStats()
      for _ in 0..<keys { stats.recordKeystroke() }
      for _ in 0..<wakes { stats.recordWake() }

      let report = stats.summarize(index: 1, elapsedSeconds: 5.0)
      XCTAssertEqual(report.keystrokes, keys, "\(desc): keystrokes counter")
      XCTAssertEqual(report.wakes, wakes, "\(desc): wakes counter")

      if let want {
        guard let got = report.wakesPerKeystroke else {
          return XCTFail("\(desc): expected \(want), got nil")
        }
        XCTAssertEqual(got, want, accuracy: eps, "\(desc): ratio")
      } else {
        XCTAssertNil(report.wakesPerKeystroke, "\(desc): expected nil")
      }
    }
  }

  // MARK: - 7. reset()

  func testResetZeroesEverything() {
    var stats = FrameStats()
    stats.record(.draw, nanos: 9 * nsPerMs)
    stats.record(.controllerWake, nanos: 3 * nsPerMs)
    stats.recordKeystroke()
    stats.recordWake()
    stats.recordWake()

    stats.reset()

    let report = stats.summarize(index: 2, elapsedSeconds: 5.0)
    XCTAssertTrue(report.buckets.isEmpty, "reset() leaves no non-empty buckets")
    XCTAssertEqual(report.keystrokes, 0, "reset() zeroes the keystroke counter")
    XCTAssertEqual(report.wakes, 0, "reset() zeroes the wake counter")
    XCTAssertNil(report.wakesPerKeystroke, "0 keystrokes after reset -> nil ratio")
    XCTAssertEqual(
      stats.retainedSampleCount(.draw), 0, "reset() clears the ring's live slice")
    XCTAssertEqual(
      stats.retainedSampleCount(.controllerWake), 0, "reset() clears every bucket's ring")
  }

  func testResetStartsAFreshWindowWithNoLeakage() {
    // Pre-reset samples are LARGE; post-reset samples are SMALL. If any pre-reset
    // sample leaked into the post-reset window (a stale writeIndex / wrapped flag)
    // the max or the percentiles would blow past the small range.
    var stats = FrameStats()
    for _ in 0..<50 {
      stats.record(.draw, nanos: 1_000_000_000)
    }
    stats.reset()
    for nanos: UInt64 in [10, 20, 31] {
      stats.record(.draw, nanos: nanos)
    }

    let report = stats.summarize(index: 2, elapsedSeconds: 5.0)
    guard let draw = report.buckets.first(where: { $0.bucket == .draw }) else {
      return XCTFail("the .draw bucket must appear after the post-reset samples")
    }
    XCTAssertEqual(draw.count, 3, "only the post-reset samples are counted")
    XCTAssertEqual(draw.meanNanos, 20, "61 / 3 == 20, unaffected by the pre-reset sum")
    XCTAssertEqual(draw.maxNanos, 31, "max is re-baselined, not the pre-reset 1e9")
    XCTAssertEqual(draw.p50Nanos, 20, "p50 over [10, 20, 31]: ceil(1.5)-1 == 1 -> 20")
    XCTAssertEqual(draw.p95Nanos, 31, "p95 over [10, 20, 31]: ceil(2.85)-1 == 2 -> 31")
    XCTAssertFalse(draw.truncated, "the post-reset window is far below capacity")
    XCTAssertEqual(
      stats.retainedSampleCount(.draw), 3, "the ring holds exactly the 3 post-reset samples")
  }

  func testSummarizeOfUntouchedStatsIsEmpty() {
    // WHY: `FrameMetrics.emit()` skips output for a fully-idle interval using a
    // private `recordedAnything` flag, hand-maintained in its three mutator
    // wrappers (`record`, `recordKeystroke`, `recordWake`) and cleared on reset.
    // That flag mirrors exactly the condition asserted here: all buckets empty
    // AND both counters zero. This test pins the seam-side contract, so if
    // `FrameStats` ever gains a fourth mutator and the flag goes stale, there is
    // a tripwire on the invariant the flag is standing in for.

    // A freshly constructed value is empty on all three counts.
    let fresh = FrameStats()
    let freshReport = fresh.summarize(index: 1, elapsedSeconds: 5.0)
    XCTAssertTrue(freshReport.buckets.isEmpty, "a fresh FrameStats summarizes to no buckets")
    XCTAssertEqual(freshReport.keystrokes, 0, "a fresh FrameStats has no keystrokes")
    XCTAssertEqual(freshReport.wakes, 0, "a fresh FrameStats has no wakes")

    // And reset() restores exactly that state after real activity through every
    // mutator: two distinct buckets plus both counters.
    var stats = FrameStats()
    stats.record(.draw, nanos: 8 * nsPerMs)
    stats.record(.draw, nanos: 12 * nsPerMs)
    stats.record(.previewRender, nanos: 25 * nsPerMs)
    stats.recordKeystroke()
    stats.recordKeystroke()
    stats.recordWake()
    stats.recordWake()
    stats.recordWake()

    stats.reset()

    let resetReport = stats.summarize(index: 2, elapsedSeconds: 5.0)
    XCTAssertTrue(resetReport.buckets.isEmpty, "reset() restores the no-buckets state")
    XCTAssertEqual(resetReport.keystrokes, 0, "reset() restores the zero-keystroke state")
    XCTAssertEqual(resetReport.wakes, 0, "reset() restores the zero-wake state")
  }

  // MARK: - 8. Format stability / diffability
  //
  // THIS is the test that guarantees two runs are `diff`-able. The expected
  // string below is the CONTRACT: the implementation must match it exactly
  // (fixed column widths, rawValue bucket ordering, the grep-able prefix on
  // every line, the "*" truncation marker). See the layout note in the file
  // header for why this differs by one space from the plan's sample data rows.

  func testFormatProducesTheExactPinnedTable() {
    // Buckets are supplied DELIBERATELY OUT OF ORDER (draw=2, ctrl.wake=0,
    // prev.render=4) so the test proves `format` sorts by rawValue rather than
    // echoing the input / dictionary iteration order.
    let report = FrameStats.Report(
      index: 3,
      elapsedSeconds: 5.0,
      buckets: [
        FrameStats.BucketSummary(
          bucket: .draw,
          count: 138,
          meanNanos: 4_020_000,
          p50Nanos: 3_600_000,
          p95Nanos: 11_400_000,
          maxNanos: 22_900_000,
          truncated: false),
        FrameStats.BucketSummary(
          bucket: .controllerWake,
          count: 142,
          meanNanos: 3_110_000,
          p50Nanos: 2_400_000,
          p95Nanos: 9_800_000,
          maxNanos: 41_200_000,
          truncated: true),
        FrameStats.BucketSummary(
          bucket: .previewRender,
          count: 15,
          meanNanos: 24_900_000,
          p50Nanos: 22_000_000,
          p95Nanos: 51_800_000,
          maxNanos: 60_100_000,
          truncated: false),
      ],
      keystrokes: 48,
      wakes: 142,
      wakesPerKeystroke: 142.0 / 48.0)

    let expected = [
      "wolf-frame-stats #3  window=5.0s",
      "wolf-frame-stats bucket        count     mean      p50      p95      max",
      "wolf-frame-stats ctrl.wake      142*     3.11     2.40     9.80    41.20",
      "wolf-frame-stats draw            138     4.02     3.60    11.40    22.90",
      "wolf-frame-stats prev.render      15    24.90    22.00    51.80    60.10",
      "wolf-frame-stats keys=48 wakes=142 wakes/key=2.96",
    ].map { $0 + "\n" }.joined()

    XCTAssertEqual(FrameStats.format(report), expected, "the report table is byte-exact")
  }

  func testFormatEmitsHeaderAndFooterWithNoBuckets() {
    // A counters-only report (no timed samples) still emits the header and the
    // footer, and the nil ratio renders as "n/a" rather than crashing or NaN.
    let report = FrameStats.Report(
      index: 1,
      elapsedSeconds: 2.5,
      buckets: [],
      keystrokes: 0,
      wakes: 4,
      wakesPerKeystroke: nil)

    let expected = [
      "wolf-frame-stats #1  window=2.5s",
      "wolf-frame-stats bucket        count     mean      p50      p95      max",
      "wolf-frame-stats keys=0 wakes=4 wakes/key=n/a",
    ].map { $0 + "\n" }.joined()

    XCTAssertEqual(FrameStats.format(report), expected, "the empty-bucket report is byte-exact")
  }

  func testEveryFormattedLineCarriesTheGrepPrefix() {
    // Structural guard: no matter the content, every emitted line is greppable.
    var stats = FrameStats()
    for bucket in FrameStatsBucket.allCases {
      stats.record(bucket, nanos: UInt64(bucket.rawValue + 1) * nsPerMs)
    }
    stats.recordKeystroke()
    stats.recordWake()

    let text = FrameStats.format(stats.summarize(index: 7, elapsedSeconds: 5.0))
    XCTAssertTrue(text.hasSuffix("\n"), "the report ends with a newline")

    let lines = text.split(separator: "\n", omittingEmptySubsequences: false).dropLast()
    XCTAssertEqual(
      lines.count, FrameStatsBucket.allCases.count + 3,
      "header + column row + one row per bucket + footer")
    for line in lines {
      XCTAssertTrue(
        line.hasPrefix("wolf-frame-stats "),
        "every line is prefixed for grep (got \"\(line)\")")
    }
  }

  func testFormattedBucketRowsAreInRawValueOrder() {
    // Deterministic ordering is what makes two runs align under `diff`.
    var stats = FrameStats()
    // Record in REVERSE bucket order; the output must still be ascending.
    for bucket in FrameStatsBucket.allCases.reversed() {
      stats.record(bucket, nanos: nsPerMs)
    }

    let text = FrameStats.format(stats.summarize(index: 1, elapsedSeconds: 5.0))
    var searchStart = text.startIndex
    for bucket in FrameStatsBucket.allCases {
      guard let found = text.range(of: bucket.label, range: searchStart..<text.endIndex) else {
        return XCTFail("\(bucket.label) must appear at or after the previous bucket's row")
      }
      searchStart = found.upperBound
    }
  }

  // MARK: - 9. Saturating delta

  func testDeltaSaturatesInsteadOfUnderflowing() {
    // (start, end, expected, description). An end-stamp EARLIER than the start
    // must yield 0, never a UInt64 wraparound to ~1.8e19.
    let cases: [(UInt64, UInt64, UInt64, String)] = [
      (100, 250, 150, "a normal forward delta"),
      (100, 100, 0, "an identical stamp -> 0"),
      (250, 100, 0, "an end BEFORE the start saturates to 0"),
      (0, 0, 0, "the zeroed / disabled sentinel -> 0"),
      (UInt64.max, 0, 0, "a maximal backwards jump still saturates to 0"),
      (0, UInt64.max, UInt64.max, "a maximal forward delta is preserved"),
    ]
    for (start, end, want, desc) in cases {
      XCTAssertEqual(FrameStats.delta(start: start, end: end), want, "delta: \(desc)")
    }
  }
}
