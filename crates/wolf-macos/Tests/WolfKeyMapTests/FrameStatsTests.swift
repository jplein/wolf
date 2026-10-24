// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#framestatstestsswift

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
  // Raw values are storage indices and labels are the report strings, so both are contract.

  func testBucketRawValuesAndLabels() {
    let cases: [(FrameStatsBucket, Int, String)] = [
      (.controllerWake, 0, "ctrl.wake"),
      (.containerWake, 1, "cont.wake"),
      (.draw, 2, "draw"),
      (.previewReload, 3, "prev.reload"),
      (.previewRender, 4, "prev.render"),
      (.previewLoadHTML, 5, "prev.html"),
      (.drawPaneEditor, 6, "draw.pane"),
      (.drawPanePreview, 7, "draw.prev"),
      (.paneFrameRead, 8, "pane.read"),
      (.paneCells, 9, "pane.cells"),
      (.paneCellsVec, 10, "pane.vec"),
      (.paneGlyphLoop, 11, "pane.glyphs"),
      (.paneCursor, 12, "pane.cursor"),
      (.paneStatusbar, 13, "pane.status"),
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

  func testBucketLabelsAreUniqueAndFitTheReportColumn() {
    // Labels must be unique, which guards the ordering test's forward-only scan, and fit width 13.
    // See notes/crates/wolf-macos/Tests/WolfKeyMapTests/FrameStatsTests.swift.md#label-rules
    let maxLabelWidth = 13

    var seen: [String: FrameStatsBucket] = [:]
    for bucket in FrameStatsBucket.allCases {
      let label = bucket.label
      XCTAssertFalse(label.isEmpty, "raw \(bucket.rawValue): the label is non-empty")
      if let prior = seen[label] {
        XCTFail(
          "duplicate label \"\(label)\": raw \(prior.rawValue) and raw \(bucket.rawValue) "
            + "would emit two identical report rows")
      }
      seen[label] = bucket
      XCTAssertLessThanOrEqual(
        label.count, maxLabelWidth,
        "\"\(label)\" is \(label.count) chars; the left-\(maxLabelWidth + 1) field needs at "
          + "least one trailing space to separate it from the count column")
    }
    XCTAssertEqual(
      seen.count, FrameStatsBucket.allCases.count,
      "every bucket has its own distinct label")
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

  func testEveryBucketRoundTripsThroughRecordAndSummarize() {
    // One distinct sample per bucket, so a mis-sized store (not derived from `allCases.count`)
    // shows up as a missing row or a value in the wrong bucket.
    var stats = FrameStats()
    var expected: [FrameStatsBucket: UInt64] = [:]
    for bucket in FrameStatsBucket.allCases {
      let nanos = UInt64(bucket.rawValue + 1) * nsPerMs
      stats.record(bucket, nanos: nanos)
      expected[bucket] = nanos
    }

    let report = stats.summarize(index: 1, elapsedSeconds: 5.0)
    XCTAssertEqual(
      report.buckets.count, FrameStatsBucket.allCases.count,
      "one sample in every bucket produces exactly one row per bucket")

    for bucket in FrameStatsBucket.allCases {
      guard let want = expected[bucket] else {
        return XCTFail("\(bucket.label): the fixture recorded a sample for every bucket")
      }
      guard let summary = report.buckets.first(where: { $0.bucket == bucket }) else {
        return XCTFail("\(bucket.label) (raw \(bucket.rawValue)) is missing from the report")
      }
      XCTAssertEqual(summary.count, 1, "\(bucket.label): exactly one sample")
      XCTAssertEqual(summary.meanNanos, want, "\(bucket.label): mean is its own sample")
      XCTAssertEqual(summary.p50Nanos, want, "\(bucket.label): p50 is its own sample")
      XCTAssertEqual(summary.p95Nanos, want, "\(bucket.label): p95 is its own sample")
      XCTAssertEqual(
        summary.maxNanos, want,
        "\(bucket.label): max is its own sample (no cross-bucket bleed)")
      XCTAssertFalse(summary.truncated, "\(bucket.label): one sample is not truncated")
      XCTAssertEqual(
        stats.retainedSampleCount(bucket), 1, "\(bucket.label): the ring holds its one sample")
    }
  }

  // MARK: - 5. Ring bound + truncation

  func testRingBoundKeepsExactCountAndWindowedPercentiles() {
    // `overflow` tiny samples, then `sampleCapacity` large ones: a window over the first samples,
    // or over all of them, drags p50 down into the tiny prefix.
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

    // The retained window is the last `capacity` samples, base ..< base+capacity; the nearest-rank
    // index for p is ceil(p * capacity) - 1.
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
    // Large samples before reset, small after: a pre-reset sample leaking into the window (a stale
    // writeIndex or wrapped flag) pushes max or the percentiles past the small range.
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
    // Pins the condition `FrameMetrics`' private `recordedAnything` flag mirrors; its mutator
    // wrappers maintain it by hand, so a new `FrameStats` mutator without a wrapper makes it stale.

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
  // The expected string is the contract that keeps two runs `diff`-able; match it byte for byte.

  func testFormatProducesTheExactPinnedTable() {
    // Buckets are supplied out of order (draw=2, ctrl.wake=0, prev.render=4) to prove `format`
    // sorts by rawValue rather than echoing the input order.
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
