//
//  FrameMetrics.swift
//  Process-global glue around the pure `WolfKeyMap.FrameStats` seam.
//
//  All the interesting logic (bucketing, percentiles, formatting, env parsing)
//  lives in `Sources/WolfKeyMap/FrameStats.swift` and is unit-tested headlessly.
//  This file owns only the parts that need a running app: the once-only
//  `enabled` evaluation, the clock reads, the report `Timer`, and writing to
//  stderr / an optional file.
//
//  OFF BY DEFAULT. When `WOLF_FRAME_STATS` is not set the entire cost at each
//  instrumented call site is a `swift_once` token load for the lazy `enabled`
//  global plus a branch — no `ProcessInfo` lookup, no formatting, no
//  allocation. When ON, the per-sample cost is one monotonic
//  clock read, one integer subtraction and a handful of integer accumulates
//  plus one array-slot write. No locks are taken and the snapshot lock is never
//  held; all state is main-thread-only.
//
//  Usage from a terminal (a double-clicked bundle inherits launchd's stderr and
//  you will see nothing):
//
//      bazel build //crates/wolf-macos:Wolf
//      WOLF_FRAME_STATS=1 \
//        ./bazel-bin/crates/wolf-macos/Wolf.app/Contents/MacOS/Wolf
//
//  Or set `WOLF_FRAME_STATS_FILE=/tmp/wolf-frame-stats.txt` and `tail -f` it.
//  `WOLF_FRAME_STATS_INTERVAL` sets the report period in seconds (default 5).
//

import Dispatch
import Foundation
import WolfKeyMap

/// Frame-time instrumentation facade. Main thread only.
enum FrameMetrics {

  /// Evaluated exactly once, lazily, via `swift_once` — never per frame.
  ///
  /// `start()` touches this at launch so the one-time `ProcessInfo` read cannot
  /// land in the middle of a measured frame.
  static let enabled: Bool = FrameStats.isEnabled(env: ProcessInfo.processInfo.environment)

  /// The single accumulator. Main thread only; never locked.
  private static var stats = FrameStats()

  /// Monotonic stamp at which the current report interval began.
  private static var intervalStartNanos: UInt64 = 0

  /// Number of reports emitted so far in this process run.
  private static var reportIndex = 0

  /// The repeating report timer, retained so it can be invalidated.
  private static var timer: Timer?

  /// Lazily opened append handle for `WOLF_FRAME_STATS_FILE`, if configured.
  private static var fileHandle: FileHandle?

  /// True once we have tried (and possibly failed) to open the stats file.
  private static var fileHandleResolved = false

  /// True when anything at all has been recorded into `stats` since the last
  /// reset. Exactly equivalent to "some bucket is non-empty OR `keystrokes` > 0
  /// OR `wakes` > 0", because those are the only three ways to mutate `stats`.
  ///
  /// Tracked here rather than derived from a `Report` so `emit()` can decide to
  /// stay silent WITHOUT paying for `summarize()`, which allocates a summaries
  /// array and sorts each non-empty bucket's retained window.
  private static var recordedAnything = false

  // MARK: - Hot path

  /// Returns a monotonic start stamp, or `0` when instrumentation is disabled.
  ///
  /// Pair with `end(_:_:)`, ideally through `defer` so early returns are still
  /// measured.
  @inline(__always)
  static func begin() -> UInt64 {
    return enabled ? DispatchTime.now().uptimeNanoseconds : 0
  }

  /// Records the elapsed time since `start` against `bucket`. No-op when
  /// disabled. The subtraction saturates, so a backwards clock reads as `0`.
  @inline(__always)
  static func end(_ bucket: FrameStatsBucket, _ start: UInt64) {
    guard enabled else { return }
    let now = DispatchTime.now().uptimeNanoseconds
    #if DEBUG
      dispatchPrecondition(condition: .onQueue(.main))
    #endif
    stats.record(bucket, nanos: FrameStats.delta(start: start, end: now))
    recordedAnything = true
  }

  /// Counts one key actually delivered to Neovim. No-op when disabled.
  @inline(__always)
  static func noteKeystroke() {
    guard enabled else { return }
    #if DEBUG
      dispatchPrecondition(condition: .onQueue(.main))
    #endif
    stats.recordKeystroke()
    recordedAnything = true
  }

  /// Counts one core wake. No-op when disabled.
  @inline(__always)
  static func noteWake() {
    guard enabled else { return }
    #if DEBUG
      dispatchPrecondition(condition: .onQueue(.main))
    #endif
    stats.recordWake()
    recordedAnything = true
  }

  // MARK: - Lifecycle

  /// Arms the periodic reporter. No-op when disabled.
  ///
  /// The timer runs in `.common` mode so reports keep firing during tracking and
  /// scrolling run-loop modes, which are exactly the moments worth measuring.
  static func start() {
    guard enabled else { return }
    intervalStartNanos = DispatchTime.now().uptimeNanoseconds

    let interval = FrameStats.reportInterval(env: ProcessInfo.processInfo.environment)
    write("wolf-frame-stats enabled interval=\(String(format: "%.1f", interval))s\n")

    let created = Timer(timeInterval: interval, repeats: true) { _ in
      FrameMetrics.emit()
    }
    RunLoop.main.add(created, forMode: .common)
    timer = created
  }

  /// Emits the current interval's report, then resets and re-baselines.
  ///
  /// Silent when disabled or when nothing at all was recorded, so an idle app
  /// produces no output.
  static func emit() {
    guard enabled else { return }

    let now = DispatchTime.now().uptimeNanoseconds

    // Test emptiness BEFORE summarizing: a fully idle app must not allocate and
    // sort once per interval just to discover it has nothing to say.
    guard recordedAnything else {
      intervalStartNanos = now
      return
    }

    let elapsedNanos = FrameStats.delta(start: intervalStartNanos, end: now)
    reportIndex += 1
    let report = stats.summarize(
      index: reportIndex,
      elapsedSeconds: Double(elapsedNanos) / 1_000_000_000.0)

    write(FrameStats.format(report))
    stats.reset()
    recordedAnything = false
    intervalStartNanos = now
  }

  // MARK: - Output

  /// Writes one report to stderr, and additionally appends it to
  /// `WOLF_FRAME_STATS_FILE` when that is set. A file that cannot be opened
  /// degrades silently to stderr-only.
  ///
  /// Both writes use the throwing `write(contentsOf:)` API and discard the
  /// error. The non-throwing `write(_:)` raises an UNCATCHABLE `NSException` on
  /// `EPIPE`, which would kill the app the moment its output is piped into a
  /// short-lived reader such as `head` or `grep -m1`. Instrumentation must never
  /// be able to take the process down.
  private static func write(_ text: String) {
    let data = Data(text.utf8)
    try? FileHandle.standardError.write(contentsOf: data)
    if let handle = statsFileHandle() {
      try? handle.write(contentsOf: data)
    }
  }

  /// Opens (once) and caches the append handle for `WOLF_FRAME_STATS_FILE`.
  private static func statsFileHandle() -> FileHandle? {
    if fileHandleResolved { return fileHandle }
    fileHandleResolved = true

    let env = ProcessInfo.processInfo.environment
    guard let raw = env["WOLF_FRAME_STATS_FILE"] else { return nil }
    let path = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !path.isEmpty else { return nil }

    if !FileManager.default.fileExists(atPath: path) {
      FileManager.default.createFile(atPath: path, contents: nil)
    }
    guard let handle = FileHandle(forWritingAtPath: path) else { return nil }
    handle.seekToEndOfFile()
    fileHandle = handle
    return handle
  }
}
