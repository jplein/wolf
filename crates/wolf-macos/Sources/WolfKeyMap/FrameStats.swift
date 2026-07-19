//
//  FrameStats.swift
//  The PURE, AppKit-free frame-time aggregation seam.
//
//  This type owns every decision that can be reasoned about without a window
//  server: which buckets exist, how samples accumulate, how percentiles are
//  computed, how a report is rendered, and how the environment variables are
//  parsed. The process-global glue that actually reads the clock, schedules the
//  report timer and writes to stderr lives in `WolfMacOS/FrameMetrics.swift`.
//
//  THREADING: `FrameStats` is a plain value type with NO internal locking. It is
//  documented MAIN-THREAD-ONLY; the `FrameMetrics` facade owns the single
//  instance and enforces the invariant with a `dispatchPrecondition` in DEBUG.
//  Keeping the enforcement out of this file is what lets the type stay pure and
//  unit-testable off the main thread.
//
//  COST MODEL: `record` is O(1) with no allocation after `init` (every bucket's
//  ring buffer reserves `sampleCapacity` up front). Sorting happens ONLY in
//  `summarize`, i.e. once per report interval, never per frame.
//

import Foundation

/// The fixed set of instrumented regions on the keystroke-to-pixel path.
///
/// The raw values are the STORAGE INDICES into `FrameStats`'s per-bucket
/// accumulator array, so they must stay dense and ascending from zero: this is
/// what keeps the hot path free of dictionary hashing and string allocation.
///
/// `drawPaneEditor` and `drawPanePreview` partition the per-pane paint work
/// inside a single `draw`, so their counts are per-pane-per-frame and their sum
/// is bounded above by `draw`'s total, not equal to it (`draw` also covers the
/// background fill, dividers and scrollbars).
///
/// ## DISJOINTNESS CONTRACT for the six `pane.*` sub-buckets
///
/// The six `pane.*` buckets split one `paintPane` into its phases. They are NOT
/// a partition, and this is the single most important thing to know before
/// reading a report:
///
/// * `{pane.read, pane.cells, pane.cursor, pane.status}` are MUTUALLY DISJOINT.
///   Their sum is bounded above by `draw.pane + draw.prev`, with a small
///   residual for the background fill, clip/save/restore and geometry work.
/// * `{pane.vec, pane.glyphs}` partition the INTERIOR of `pane.cells`, WITH A
///   REMAINDER. That remainder is the `RustVec<FfiCell>` teardown — the ARC
///   release of `cols*rows` `FfiCell`s after the glyph loop — and is only
///   obtainable as the derived difference
///   `T(pane.cells) - T(pane.vec) - T(pane.glyphs)`.
/// * **The six do NOT sum to `draw.pane`. Summing all six DOUBLE-COUNTS**, because
///   `pane.vec` and `pane.glyphs` are both nested inside `pane.cells`.
///
/// ASYMMETRY WORTH REMEMBERING: `pane.status` is recorded for OCCLUDED panes
/// too (the statusbar sits outside the grid rect and always paints), whereas
/// `pane.read` is not — its timer sits below the `paintsGrid` guard, so an
/// occluded pane records no `pane.read` sample. So
/// `count(pane.status) == count(draw.pane) + count(draw.prev)` while
/// `count(pane.read) == count(draw.pane)`.
///
/// Also: `pane.glyphs` is NOT "CoreText time". It is a composite of ALL per-cell
/// Swift-side work — the `CGColor` alloc, the fill, the per-cell Rust+Swift
/// `String` bridge, style resolution AND the `CTLine` draw. Read it as
/// "per-cell Swift-side work".
public enum FrameStatsBucket: Int, CaseIterable {
  /// `WolfWindowController.onRenderWake()` — the whole per-wake controller pass.
  case controllerWake = 0
  /// `PaneContainerView.onRenderWake()` — the container-only portion of a wake.
  case containerWake = 1
  /// `PaneContainerView.draw(_:)` — a full repaint.
  case draw = 2
  /// `PaneContainerView.reloadDirtyPreviews()` — the whole preview refresh pass.
  case previewReload = 3
  /// One `render_preview` FFI round-trip, including the `toString()` bridge copy.
  case previewRender = 4
  /// One synchronous `loadHTMLString` call (NOT the async WebKit work it queues).
  case previewLoadHTML = 5
  /// One `PaneContainerView.paintPane` call for an ORDINARY editor pane.
  case drawPaneEditor = 6
  /// One `PaneContainerView.paintPane` call for a PREVIEW pane, i.e. one whose
  /// grid is a Markdown-preview scratch buffer sitting under a `WKWebView`.
  case drawPanePreview = 7
  /// `handle.read_frame_for_grid(grid)` — the snapshot read lock, the per-cell
  /// Rust `String` clone in `FfiFrame::from_snapshot_grid`, and the `FfiFrame`
  /// construction. Recorded only when the pane actually paints its grid.
  case paneFrameRead = 8
  /// The WHOLE `paintCells(_:ctx:)` call, measured at its CALL SITE. Parent of
  /// `paneCellsVec` and `paneGlyphLoop`, and the only bucket that contains the
  /// `RustVec<FfiCell>` teardown.
  case paneCells = 9
  /// Just `let cells = frame.cells()` — a second full deep clone of the cell
  /// vector, in isolation.
  case paneCellsVec = 10
  /// Just the `for row` / `for col` double loop inside `paintCells`: per cell,
  /// the bg `CGColor` + fill, the text bridge, style resolution and `drawGlyph`.
  case paneGlyphLoop = 11
  /// The `drawCursor(...)` call expression in `paintGrid`, INCLUDING evaluation
  /// of its `frame.cells()` argument (a third full deep clone).
  case paneCursor = 12
  /// The `paintStatusbar(...)` call in `paintPane`. Recorded for occluded panes
  /// too, since the statusbar paints regardless.
  case paneStatusbar = 13

  /// The short, fixed-width-table-friendly name used only at report time.
  public var label: String {
    switch self {
    case .controllerWake: return "ctrl.wake"
    case .containerWake: return "cont.wake"
    case .draw: return "draw"
    case .previewReload: return "prev.reload"
    case .previewRender: return "prev.render"
    case .previewLoadHTML: return "prev.html"
    case .drawPaneEditor: return "draw.pane"
    case .drawPanePreview: return "draw.prev"
    case .paneFrameRead: return "pane.read"
    case .paneCells: return "pane.cells"
    case .paneCellsVec: return "pane.vec"
    case .paneGlyphLoop: return "pane.glyphs"
    case .paneCursor: return "pane.cursor"
    case .paneStatusbar: return "pane.status"
    }
  }
}

/// Main-thread-only accumulator for per-bucket frame timings and the
/// keystroke/wake counters, plus the pure report formatting.
///
/// `count`, the running sum and `max` are EXACT and unbounded. Only the
/// percentile window is bounded, to the last `sampleCapacity` samples per
/// bucket, which caps memory at roughly 200 KB regardless of session length.
public struct FrameStats {

  /// Per-bucket bound on retained samples for percentile computation.
  ///
  /// Exceeding it does not lose `count`, the sum or `max` — only older samples
  /// drop out of the percentile window, which is then reported as `truncated`.
  public static let sampleCapacity = 4096

  /// The grep-able prefix stamped on every emitted line.
  private static let linePrefix = "wolf-frame-stats "

  /// The default report interval, in seconds, when `WOLF_FRAME_STATS_INTERVAL`
  /// is absent, unparseable or non-positive.
  private static let defaultReportInterval: TimeInterval = 5.0

  /// One bucket's exact counters plus its bounded ring of recent samples.
  private struct Accumulator {
    var count: Int = 0
    var sumNanos: UInt64 = 0
    var maxNanos: UInt64 = 0
    var samples: [UInt64] = []
    var writeIndex: Int = 0

    mutating func record(_ nanos: UInt64) {
      count += 1
      sumNanos &+= nanos
      if nanos > maxNanos { maxNanos = nanos }
      if samples.count < FrameStats.sampleCapacity {
        samples.append(nanos)
        writeIndex = samples.count % FrameStats.sampleCapacity
      } else {
        samples[writeIndex] = nanos
        writeIndex = (writeIndex + 1) % FrameStats.sampleCapacity
      }
    }

    mutating func reset() {
      count = 0
      sumNanos = 0
      maxNanos = 0
      samples.removeAll(keepingCapacity: true)
      writeIndex = 0
    }
  }

  /// The aggregated statistics for a single bucket over one report interval.
  public struct BucketSummary {
    /// Which instrumented region this row describes.
    public let bucket: FrameStatsBucket
    /// The exact number of samples recorded in the interval.
    public let count: Int
    /// Truncating integer mean, `sumNanos / count`; `0` when `count == 0`.
    public let meanNanos: UInt64
    /// Nearest-rank 50th percentile over the retained window.
    public let p50Nanos: UInt64
    /// Nearest-rank 95th percentile over the retained window.
    public let p95Nanos: UInt64
    /// The exact largest sample, tracked outside the ring.
    public let maxNanos: UInt64
    /// True when `count > sampleCapacity`, i.e. the percentiles are windowed.
    public let truncated: Bool

    /// Memberwise initializer, public so tests and the facade can build fixtures.
    public init(
      bucket: FrameStatsBucket,
      count: Int,
      meanNanos: UInt64,
      p50Nanos: UInt64,
      p95Nanos: UInt64,
      maxNanos: UInt64,
      truncated: Bool
    ) {
      self.bucket = bucket
      self.count = count
      self.meanNanos = meanNanos
      self.p50Nanos = p50Nanos
      self.p95Nanos = p95Nanos
      self.maxNanos = maxNanos
      self.truncated = truncated
    }
  }

  /// One report interval's worth of aggregates, ready for `format`.
  public struct Report {
    /// Monotonically increasing report number within the process run.
    public let index: Int
    /// Wall-clock length of the interval this report covers, in seconds.
    public let elapsedSeconds: Double
    /// Only the buckets that recorded at least one sample.
    public let buckets: [BucketSummary]
    /// Keys actually delivered to Neovim during the interval.
    public let keystrokes: Int
    /// Core wakes observed during the interval.
    public let wakes: Int
    /// `wakes / keystrokes`, or `nil` when no keystrokes were recorded.
    ///
    /// This is an UPPER BOUND on wakes attributable to typing: wakes triggered
    /// by mouse, scroll, resize or out-of-band core notifications inflate it.
    public let wakesPerKeystroke: Double?

    /// Memberwise initializer, public so tests and the facade can build fixtures.
    public init(
      index: Int,
      elapsedSeconds: Double,
      buckets: [BucketSummary],
      keystrokes: Int,
      wakes: Int,
      wakesPerKeystroke: Double?
    ) {
      self.index = index
      self.elapsedSeconds = elapsedSeconds
      self.buckets = buckets
      self.keystrokes = keystrokes
      self.wakes = wakes
      self.wakesPerKeystroke = wakesPerKeystroke
    }
  }

  /// Per-bucket accumulators, indexed by `FrameStatsBucket.rawValue`.
  private var accumulators: [Accumulator]
  /// Keys delivered to Neovim since the last `reset()`.
  private var keystrokes: Int = 0
  /// Core wakes observed since the last `reset()`.
  private var wakes: Int = 0

  /// Creates an empty accumulator, reserving each bucket's ring up front so the
  /// hot path never allocates.
  public init() {
    var storage = [Accumulator]()
    storage.reserveCapacity(FrameStatsBucket.allCases.count)
    for _ in FrameStatsBucket.allCases {
      var accumulator = Accumulator()
      accumulator.samples.reserveCapacity(FrameStats.sampleCapacity)
      storage.append(accumulator)
    }
    self.accumulators = storage
  }

  /// Records one timing sample, in nanoseconds, against `bucket`. O(1), no
  /// allocation. Main thread only.
  public mutating func record(_ bucket: FrameStatsBucket, nanos: UInt64) {
    accumulators[bucket.rawValue].record(nanos)
  }

  /// Counts one key actually delivered to Neovim. Main thread only.
  public mutating func recordKeystroke() {
    keystrokes += 1
  }

  /// Counts one core wake. Main thread only.
  public mutating func recordWake() {
    wakes += 1
  }

  /// Zeroes every counter and empties every ring, keeping the allocated storage
  /// so each report covers only its own interval and reports stay comparable.
  public mutating func reset() {
    for index in accumulators.indices {
      accumulators[index].reset()
    }
    keystrokes = 0
    wakes = 0
  }

  /// The number of samples currently retained for percentile computation. Never
  /// exceeds `sampleCapacity`.
  public func retainedSampleCount(_ bucket: FrameStatsBucket) -> Int {
    return accumulators[bucket.rawValue].samples.count
  }

  /// Builds an immutable `Report` for the interval, omitting zero-count buckets.
  ///
  /// Copies and sorts each bucket's retained window; this is the ONLY sorting in
  /// the design and it happens once per report, never per frame.
  public func summarize(index: Int, elapsedSeconds: Double) -> Report {
    var summaries = [BucketSummary]()
    summaries.reserveCapacity(FrameStatsBucket.allCases.count)

    for bucket in FrameStatsBucket.allCases {
      let accumulator = accumulators[bucket.rawValue]
      guard accumulator.count > 0 else { continue }
      let sorted = accumulator.samples.sorted()
      summaries.append(
        BucketSummary(
          bucket: bucket,
          count: accumulator.count,
          meanNanos: accumulator.sumNanos / UInt64(accumulator.count),
          p50Nanos: FrameStats.percentile(sorted, 0.5),
          p95Nanos: FrameStats.percentile(sorted, 0.95),
          maxNanos: accumulator.maxNanos,
          truncated: accumulator.count > FrameStats.sampleCapacity))
    }

    let ratio: Double? = keystrokes > 0 ? Double(wakes) / Double(keystrokes) : nil
    return Report(
      index: index,
      elapsedSeconds: elapsedSeconds,
      buckets: summaries,
      keystrokes: keystrokes,
      wakes: wakes,
      wakesPerKeystroke: ratio)
  }

  /// Nearest-rank percentile over an ALREADY-SORTED ascending input.
  ///
  /// `index = clamp(Int(ceil(p * n)) - 1, 0, n - 1)`. Returns `0` for an empty
  /// input. Interpolating variants were rejected: nearest-rank always returns a
  /// value that was actually observed, which is what makes the table trustworthy.
  public static func percentile(_ sorted: [UInt64], _ p: Double) -> UInt64 {
    let n = sorted.count
    guard n > 0 else { return 0 }
    let rank = Int((p * Double(n)).rounded(.up)) - 1
    let index = min(max(rank, 0), n - 1)
    return sorted[index]
  }

  /// Saturating monotonic-clock subtraction: returns `0` rather than wrapping
  /// when `end` precedes `start` (or when either is the disabled `0` sentinel).
  public static func delta(start: UInt64, end: UInt64) -> UInt64 {
    return end > start ? end - start : 0
  }

  /// Renders a report as a fixed-width, `diff`-able, fully grep-prefixed table.
  ///
  /// Layout: label left-14, count right-5 (with a `*` suffix inside the field
  /// when truncated), then mean/p50/p95/max right-9 each in milliseconds with two
  /// decimals. Rows are emitted in ascending `rawValue` order regardless of the
  /// order they appear in `report.buckets`, so two runs align under `diff`. Every
  /// line, including the last, ends in a newline.
  public static func format(_ report: Report) -> String {
    var text = linePrefix
    text += "#\(report.index)  window=\(String(format: "%.1f", report.elapsedSeconds))s\n"

    text += linePrefix
    text += pad(left: "bucket", 14)
    text += pad(right: "count", 5)
    text += pad(right: "mean", 9)
    text += pad(right: "p50", 9)
    text += pad(right: "p95", 9)
    text += pad(right: "max", 9)
    text += "\n"

    for summary in report.buckets.sorted(by: { $0.bucket.rawValue < $1.bucket.rawValue }) {
      let countField = "\(summary.count)" + (summary.truncated ? "*" : "")
      text += linePrefix
      text += pad(left: summary.bucket.label, 14)
      text += pad(right: countField, 5)
      text += pad(right: millis(summary.meanNanos), 9)
      text += pad(right: millis(summary.p50Nanos), 9)
      text += pad(right: millis(summary.p95Nanos), 9)
      text += pad(right: millis(summary.maxNanos), 9)
      text += "\n"
    }

    let ratio: String
    if let value = report.wakesPerKeystroke {
      ratio = String(format: "%.2f", value)
    } else {
      ratio = "n/a"
    }
    text += linePrefix
    text += "keys=\(report.keystrokes) wakes=\(report.wakes) wakes/key=\(ratio)\n"
    return text
  }

  /// True when `WOLF_FRAME_STATS` is set to `1`/`true`/`yes`/`on`, compared
  /// case-insensitively after trimming whitespace. Anything else is false.
  public static func isEnabled(env: [String: String]) -> Bool {
    guard let raw = env["WOLF_FRAME_STATS"] else { return false }
    let value = raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    return value == "1" || value == "true" || value == "yes" || value == "on"
  }

  /// Parses `WOLF_FRAME_STATS_INTERVAL` as seconds. Absent, unparseable or
  /// non-positive values fall back to the 5.0 second default.
  public static func reportInterval(env: [String: String]) -> TimeInterval {
    guard let raw = env["WOLF_FRAME_STATS_INTERVAL"] else { return defaultReportInterval }
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard let value = Double(trimmed), value > 0 else { return defaultReportInterval }
    return value
  }

  /// Nanoseconds as a two-decimal millisecond string.
  private static func millis(_ nanos: UInt64) -> String {
    return String(format: "%.2f", Double(nanos) / 1_000_000.0)
  }

  /// Left-aligns `text` in a field of `width`, never truncating.
  private static func pad(left text: String, _ width: Int) -> String {
    let deficit = width - text.count
    guard deficit > 0 else { return text }
    return text + String(repeating: " ", count: deficit)
  }

  /// Right-aligns `text` in a field of `width`, never truncating.
  private static func pad(right text: String, _ width: Int) -> String {
    let deficit = width - text.count
    guard deficit > 0 else { return text }
    return String(repeating: " ", count: deficit) + text
  }
}
