# Plan: Frame-time instrumentation for the wolf-macos render path

## Problem statement

Typing latency in Wolf is noticeably worse than Neovim in a terminal, especially
on slower Macs. It also reproduces on a fast Mac when a Markdown preview pane is
open. Before changing any render behavior we need numbers, so we can tell which
costs actually dominate and whether later fixes land.

## Investigation findings (already established, do NOT re-derive)

Traced the keystroke-to-pixel path. Confirmed hot spots, in leverage order:

1. `PaneContainerView.reloadDirtyPreviews()`
   (`crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift:876`) runs on the
   MAIN THREAD and per dirty preview does `handle.render_preview(bufnr)` — a
   `runtime.block_on` RPC (`crates/wolf-core/src/api.rs:424`) that reads the whole
   buffer and re-renders the entire document through `pulldown-cmark` — followed by
   `web.loadHTMLString(...)`, a full WKWebView document reload. Debounced to ~33 ms
   by `PickerRefreshCoalescer.refreshInterval`
   (`Sources/WolfKeyMap/PickerRefreshCoalescer.swift:39`). Suspected cause of the
   stall reproducible even on a fast Mac.
2. Per-cell CoreText shaping every frame: `drawGlyph`
   (`PaneContainerView.swift:1508-1525`) builds a fresh attributes dict,
   `NSAttributedString`, and `CTLineCreateWithAttributedString` per non-empty cell.
   No glyph cache, no run coalescing.
3. Cell vector deep-cloned ~3x per frame: `from_snapshot_grid`
   (`crates/wolf-ffi/src/lib.rs:3456`, a `String` alloc per cell), `cells()`
   (`:3376`, clones the whole vector), and again in `drawCursor`
   (`PaneContainerView.swift:1204`). `read_frame_for_grid` is called 5-6x per wake.
4. No dirty-rect tracking: `draw(_ dirtyRect:)` ignores `dirtyRect` and repaints
   every pane, divider and scrollbar (`PaneContainerView.swift:730, 1131`).
5. Every wake is a full uncoalesced main-thread pass (`WakeBridge.swift:23`);
   out-of-band notifications each fire their own wake, so one keystroke can cause
   several full passes.
6. Fixed unconditional `tokio::time::sleep(8ms)` per input batch
   (`INPUT_COALESCE_DELAY`, `crates/wolf-core/src/api.rs:280`). Real latency, but
   machine-INDEPENDENT, so it does not explain the slower-Mac gap. Out of scope.

## Scope of THIS change: instrumentation ONLY

Do NOT change render behavior. Do NOT fix any of the above. Add measurement only.

### Required measurements

- Wall-clock duration of `PaneContainerView.onRenderWake()`
- Wall-clock duration of `PaneContainerView.draw(_:)`
- Wall-clock duration of `reloadDirtyPreviews()`, ideally split into the
  `render_preview` FFI round-trip vs the `loadHTMLString` reload
- Count of wakes per keystroke (attribute wakes to the input that caused them, or
  at minimum a wake counter alongside a keystroke counter)

### Constraints

- Off by default; enabled by a debug flag / env var. When disabled the cost must be
  negligible — no formatting, no allocation on the hot path.
- Output readable from the terminal for before/after comparison. Prefer periodic
  aggregates (count, mean, p50, p95, max) over per-frame log spam — per-frame lines
  at 30+ fps are unreadable and themselves distort the measurement.
- Must not hold the snapshot lock, nor add main-thread work when enabled beyond a
  monotonic clock read and an integer accumulate.
- Follow existing repo conventions for seams/testability (the Swift `*Seam` pattern
  used throughout `Sources/WolfKeyMap`) so the aggregation logic is unit-testable
  without a running app.

## Success criteria

With the flag enabled, typing in a buffer with a preview pane open produces terminal
output showing where main-thread time goes, sufficient to compare against a later run
after the preview fix.

## Architect's detailed plan (APPROVED — implement exactly this)

### Files to CREATE

**`crates/wolf-macos/Sources/WolfKeyMap/FrameStats.swift`** — pure, Foundation-only,
AppKit-free. The testable seam.

- `public enum FrameStatsBucket: Int, CaseIterable` — fixed bucket set so storage is
  array-indexed by `rawValue` (no dict hashing, no string alloc on the hot path):
  `controllerWake = 0` (`"ctrl.wake"`), `containerWake = 1` (`"cont.wake"`),
  `draw = 2` (`"draw"`), `previewReload = 3` (`"prev.reload"`),
  `previewRender = 4` (`"prev.render"`), `previewLoadHTML = 5` (`"prev.html"`).
  Each case exposes `public var label: String` used only at report time.
- `public struct FrameStats` — value type, no locks, **documented main-thread-only**
  (enforcement lives in the `WolfMacOS` facade so this type stays pure/testable).
  - Storage: fixed-length array of per-bucket accumulators, each
    `count: Int`, `sumNanos: UInt64`, `maxNanos: UInt64`, bounded ring buffer
    `samples: [UInt64]` + `writeIndex: Int` + `wrapped: Bool`.
  - `public static let sampleCapacity = 4096` — per-bucket retained-sample bound.
    Total memory ~200 KB regardless of session length. count/sum/max stay EXACT and
    unbounded; only percentiles are windowed.
  - `record(_:nanos:)` — O(1), no allocation after init (reserve capacity up front).
  - `recordKeystroke()` / `recordWake()` — integer counters.
  - `BucketSummary` — bucket, count, meanNanos, p50Nanos, p95Nanos, maxNanos,
    `truncated: Bool` (true when `count > sampleCapacity`).
  - `Report` — index, elapsedSeconds, buckets (non-empty only), keystrokes, wakes,
    `wakesPerKeystroke: Double?` (nil when keystrokes == 0).
  - `summarize(index:elapsedSeconds:)` — copies each ring's live slice, sorts the
    COPY, exact percentiles over the window. Sorting only at report time, never per frame.
  - `percentile(_ sorted: [UInt64], _ p: Double) -> UInt64` — pinned algorithm:
    **nearest-rank**, `index = clamp(Int(ceil(p * Double(n))) - 1, 0, n - 1)`,
    returns `0` for empty input.
  - `reset()` — zeroes counts/sums/maxes/ring state, keeps allocated arrays, so each
    report covers only its own interval and consecutive reports are comparable.
  - `format(_ report: Report) -> String` — pure String in/out, exact output pinnable
    by test (see reporting format below).
  - `isEnabled(env: [String: String]) -> Bool` — `WOLF_FRAME_STATS` present and,
    case-insensitively trimmed, one of `1`/`true`/`yes`/`on`. Absent/empty/`0`/
    `false`/anything else → false.
  - `reportInterval(env:) -> TimeInterval` — parses `WOLF_FRAME_STATS_INTERVAL`
    (seconds, Double), default `5.0`; non-positive/unparseable → default.

**`crates/wolf-macos/Sources/WolfMacOS/FrameMetrics.swift`** — thin process-global glue
(`Foundation` + `Dispatch` + `import WolfKeyMap`).

- `static let enabled: Bool` — initialized ONCE from
  `FrameStats.isEnabled(env: ProcessInfo.processInfo.environment)`. Swift `static let`
  is lazily init'd exactly once via `swift_once`; NO per-frame `ProcessInfo` lookup.
  `AppDelegate` touches it at launch so the one-time init never lands mid-frame.
- `private static var stats = FrameStats()` — main thread only.
- `@inline(__always) static func begin() -> UInt64` —
  `enabled ? DispatchTime.now().uptimeNanoseconds : 0`.
- `@inline(__always) static func end(_ bucket:_ start:)` — `guard enabled else { return }`,
  clock read, **saturating** subtraction, `stats.record`. Disabled path is a single
  `Bool` test in each of begin/end — no formatting, no allocation.
- `noteKeystroke()` / `noteWake()` — same `guard enabled` shape.
- `start()` — no-op when disabled; else records `intervalStartNanos` and installs a
  repeating `Timer` on the main run loop in **`.common` mode** (so reports keep firing
  during tracking/scroll run-loop modes) calling `emit()`.
- `emit()` — no-op when disabled or when all buckets and both counters are zero (idle
  app is silent). Else build Report → `format` → write → `stats.reset()` → re-baseline.
- `write(_:)` — one `FileHandle.standardError.write(Data(text.utf8))` per report;
  additionally appends to `WOLF_FRAME_STATS_FILE` when set (opened once, cached
  `FileHandle`, seek-to-end; open failure degrades silently to stderr-only).
- `#if DEBUG` only: `dispatchPrecondition(condition: .onQueue(.main))` in
  `end`/`noteKeystroke`/`noteWake`. Compiled out of release.

**`crates/wolf-macos/Tests/WolfKeyMapTests/FrameStatsTests.swift`** — see test plan.

### Files to MODIFY

`WolfWindowController.swift` — `onRenderWake()` (:339): `let t = FrameMetrics.begin()`
as FIRST statement + `defer { FrameMetrics.end(.controllerWake, t) }`. `defer` is the
mechanism throughout: survives early returns without restructuring control flow.

**Wake-boundary decision:** instrument BOTH controller and container. The per-wake
`current_file()` (:342), `cwd()` (:353) and `refreshTabTitles` (:377) are real
main-thread work — including two FFI round-trips — outside the container. Separate
buckets nest cleanly; the difference between the two means isolates controller-only
overhead, answering "is the container the whole story?" without a second run.

`PaneContainerView.swift`:

- `onRenderWake()` (:704): begin + `defer` → `.containerWake`, at the top, BEFORE the
  `pendingRefocusRebind` branch so the early-return path is measured.
- `draw(_ dirtyRect:)` (:1131): same → `.draw`, placed BEFORE the `guard let ctx` so a
  no-context early return is still counted (a near-zero sample is itself informative).
- `reloadDirtyPreviews()` (:865): same → `.previewReload`.
- Inside the `for win in needing` loop use EXPLICIT start/stop (not `defer` — these are
  mid-loop statements and we want one sample per preview, not per function call):
  - around `handle.render_preview(...).toString()` (:881) → `.previewRender`.
    Deliberately includes the `.toString()` bridge copy — part of the FFI round-trip cost.
  - around `web.loadHTMLString(html, baseURL: base)` (:887) → `.previewLoadHTML`.
    **Add a code comment** noting this measures only the SYNCHRONOUS call, not the async
    WebKit parse/layout/paint that follows off this call stack. Document this limitation.
- `keyDown(with:)` (:476): `FrameMetrics.noteKeystroke()` immediately BEFORE
  `handle.send_key(ev)` (:501) — after the picker-trigger early return and the
  `WolfKeyMap.translate` guard, so it counts only keys actually delivered to Neovim.

`Project.swift` — `onWake()` (:136): `FrameMetrics.noteWake()` before
`currentView?.onRenderWake()`.

**Wakes-per-keystroke decision:** simple counter-ratio, NOT real attribution. Wakes
counted at `Project.onWake` (exactly one per core wake through `WakeBridge`, independent
of view count), keystrokes at `keyDown`, reported as a ratio. **Stated limitation:**
wakes from non-key sources (mouse, scroll, resize, out-of-band core notifications, `:cd`,
autocmds) inflate it, so it is an UPPER BOUND, valid only for a session where the user
does nothing but type. True attribution (a generation/seq stamp threaded through
wolf-core and the wake ABI) is a cross-language change beyond an instrumentation-only
pass — not justified; revisit only if the ratio proves ambiguous.

`AppDelegate.swift` — `applicationDidFinishLaunching`: call `FrameMetrics.start()`
(forces the one-time `enabled` eval, arms the timer, writes a one-line run header).
`applicationWillTerminate` (add if absent): `FrameMetrics.emit()` to flush the final
partial interval. Best-effort — not guaranteed on force-quit.

`crates/wolf-macos/BUILD.bazel`:

- add `"Sources/WolfKeyMap/FrameStats.swift"` to `swift_library(name = "wolf_key_map").srcs`
  (explicitly enumerated, not globbed)
- add `"Tests/WolfKeyMapTests/FrameStatsTests.swift"` to `swift_test(name = "WolfKeyMapTests").srcs`
- NO change for `Sources/WolfMacOS/FrameMetrics.swift` — `wolf_macos_lib` globs
  `Sources/WolfMacOS/*.swift`. NO change to the `swift_format_srcs` filegroup.

### Reporting format

Repeating main-run-loop `Timer` at `WOLF_FRAME_STATS_INTERVAL` (default 5 s) + a final
flush from `applicationWillTerminate`. Interval-scoped and reset after each emit.
Zero-sample intervals emit nothing. Explicitly NOT per-frame lines (unreadable at 30+ fps
and the I/O would distort the measurement).

```
wolf-frame-stats #3  window=5.0s
wolf-frame-stats bucket        count     mean      p50      p95      max
wolf-frame-stats ctrl.wake       142    3.11     2.40     9.80    41.20
wolf-frame-stats cont.wake       142    2.85     2.20     9.10    39.70
wolf-frame-stats draw            138    4.02     3.60    11.40    22.90
wolf-frame-stats prev.reload      15   31.70    28.10    62.30    71.50
wolf-frame-stats prev.render      15   24.90    22.00    51.80    60.10
wolf-frame-stats prev.html        15    5.10     4.80     9.20    10.40
wolf-frame-stats keys=48 wakes=142 wakes/key=2.96
```

Milliseconds, two decimals. `*` suffix on `count` marks a truncated (windowed-percentile)
bucket. Fixed column widths so `diff` between runs aligns. Every line prefixed for `grep`.

**Where:** `FileHandle.standardError`, one write per report. NOT `NSLog` (timestamps and
prefixes every line, mangling the fixed-width table and duplicating into the unified log)
and NOT `tracing` (Rust-side; this is Swift with no logging dep today).

**How the developer sees it:** `Wolf.app` is a bundled AppKit app, so a double-clicked
app's stderr is inherited from `launchd` and never reaches a terminal. Two documented paths:

1. Recommended for development — run the bundle's inner executable directly after
   `bazel build //crates/wolf-macos:Wolf`:
   `WOLF_FRAME_STATS=1 ./bazel-bin/crates/wolf-macos/Wolf.app/Contents/MacOS/Wolf`
   Inherits the terminal's stderr; `Bundle.main` still resolves to the `.app` so bundled
   Neovim/fonts/init.lua load normally. (`bazel run` works for launching but its stderr
   plumbing through the rules_apple runner is less predictable.)
2. For a double-clicked app — set `WOLF_FRAME_STATS_FILE=/tmp/wolf-frame-stats.txt` and
   `tail -f` it.

### Clock

`DispatchTime.now().uptimeNanoseconds` — backed by `mach_absolute_time` scaled by the
timebase: monotonic (unaffected by wall-clock/NTP adjustments), cheap (commpage/rdtsc-class,
tens of ns, no syscall on Apple silicon), allocation-free, already integer nanoseconds so
the hot path is one read + one `UInt64` subtraction with NO `Duration` construction. Does
not advance during sleep, which is correct for frame timing.

Rejected: `ContinuousClock` (returns `Duration`, needs per-sample conversion arithmetic,
ticks through sleep); `Date`/`CFAbsoluteTimeGetCurrent` (wall-clock, NOT monotonic —
disqualified); raw `mach_absolute_time` (same source but requires manual
`mach_timebase_info` handling `DispatchTime` already does).

### Test plan

Pure headless `XCTest`, `@testable import WolfKeyMap`, matching the table-driven style of
existing `PaneGeometryTests` / `StatusbarLayoutTests`. No Neovim, no window, no AppKit.

1. **Env parsing** — `isEnabled`: absent → false; `""` → false; `1`/`true`/`TRUE`/`Yes`/`on`
   → true; `0`/`false`/`maybe` → false. `reportInterval`: absent → 5.0; `"2.5"` → 2.5;
   `"0"`/`"-1"`/`"abc"` → 5.0.
2. **Basic accumulation** — known sequence into one bucket; assert count, meanNanos
   (integer-division semantics pinned exactly), maxNanos.
3. **Percentile math** — pinned nearest-rank: empty → 0; single element → that element for
   p50 and p95; `1...100` → p50 = 50, p95 = 95, max = 100; two- and three-element inputs
   pinning the exact rounding boundary; an all-equal input.
4. **Bucket isolation** — recording `.draw` leaves `.previewRender` empty; `summarize`
   omits zero-count buckets.
5. **Ring bound + truncation** — record `sampleCapacity + 100`; assert count exact and
   unbounded, maxNanos exact (tracked outside the ring), `truncated == true`, percentiles
   reflect only the LAST `sampleCapacity` samples (feed a distinguishable pattern — small
   values first, large last — so a wrong window is detectable), and the retained array
   length never exceeds `sampleCapacity`.
6. **Ratio math** — keystrokes == 0 → nil (divide-by-zero edge); 30/10 → 3.0; 0/5 → 0.0.
7. **`reset()`** — all zeroed, `summarize` yields empty bucket list, a subsequent `record`
   starts a fresh window (no pre-reset leakage into post-reset percentiles).
8. **Format stability / diffability** — fixed Report → assert `format` produces an EXACT
   expected string: fixed column widths, deterministic bucket ordering (by `rawValue`, not
   dictionary iteration order), the `wolf-frame-stats` prefix on every line, truncation
   marker present when truncated. This test is what guarantees two runs are `diff`-able.
9. **Saturating delta** — if the delta helper is exposed, an end-stamp earlier than the
   start-stamp yields 0, not a `UInt64` underflow.

**Cannot be unit-tested** (record as compile-verified + manual-smoke in `Tests/README.md`,
consistent with how `PreviewSeamTests` / `StateSeamTests` document their glue): the
instrumentation call sites, `FrameMetrics`'s once-only static init, `Timer` scheduling,
stderr/file emission, `dispatchPrecondition`. These need a running app with a window
server; the repo has no macOS UI test target and creating one is out of scope. The repo's
"prefer real integration tests over mocks" rule targets the Neovim boundary and is
satisfied by the existing real-nvim `wolf-ffi` suite — this change adds no FFI surface.

No existing test is modified other than the `BUILD.bazel` srcs list.

### Dependencies

NONE. Foundation + Dispatch only, both already available. No new `bazel_dep`, no
`CARGO_BAZEL_REPIN`, no lockfile regeneration.

### Implementation order

1. `FrameStats.swift` (compiles alone) → 2. BUILD srcs → 3. tests + BUILD test srcs, run
`bazel test //crates/wolf-macos:WolfKeyMapTests` green BEFORE any call site exists →
4. `FrameMetrics.swift` → 5. `AppDelegate` start/flush → 6. call sites in order:
`Project.swift` → `WolfWindowController.swift` → `PaneContainerView.swift` →
7. `bazel build //crates/wolf-macos:Wolf` + `bazel test //crates/wolf-macos:WolfKeyMapTests
//crates/wolf-macos:bundle_layout_test` → 8. docs + prettier → 9. optional advisory
`bazel test //tools/lint:swift_format`.

### Lint

`//tools/lint:swift_format` is advisory and tagged `manual` so it will not gate this, but
write to pass it: no force-unwraps or IUOs (`NeverForceUnwrap`,
`NeverUseImplicitlyUnwrappedOptionals`), lowerCamelCase, explicit `public` on the seam API,
doc comments on every public declaration. No `// swift-format-ignore` directives expected;
if the line-length rule fires on the fixed-width format construction, break the expression
across lines rather than adding an ignore. Per `build.md` the swift-format check and fix
enforce different rule sets, so a `:swift_format.fix` diff is not a failure.

### Documentation to update (docs are source of truth in this repo)

- `crates/wolf-macos/Sources/README.md` — `FrameStats` (WolfKeyMap) and `FrameMetrics`
  (WolfMacOS): bucket set, env vars, main-thread-only invariant, sample bound, and the
  `loadHTMLString`-is-synchronous-only caveat.
- `crates/wolf-macos/Tests/README.md` — `FrameStatsTests.swift` section in the existing
  per-file style.
- `build.md` — "Frame-time instrumentation (macOS)" subsection under the macOS app section:
  the three env vars and the exact terminal invocation. Extend the existing
  `//crates/wolf-macos:WolfKeyMapTests` bullet to mention the frame-stats seam.
- `crates/wolf-macos/README.md` — one paragraph on the instrumentation surface.
- `prettier --write` every touched Markdown file; no hard wraps inside paragraphs.

### Explicitly NOT changing

All six investigated hot spots stay exactly as they are. This pass does NOT: move
`reloadDirtyPreviews`/`render_preview` off the main thread, make it incremental, or replace
`loadHTMLString`; change `PickerRefreshCoalescer.refreshInterval` (33 ms) or the preview
debounce policy; add a glyph cache/run coalescing or touch `drawGlyph`/`paintCells`; reduce
the cell-vector clones or the 5–6 `read_frame_for_grid` calls per wake; introduce
dirty-rect tracking (`dirtyRect` remains ignored); coalesce wakes in `WakeBridge`; touch
`INPUT_COALESCE_DELAY` or anything in `wolf-core`/`wolf-ffi`. **No Rust is modified at
all**, no FFI surface added, so no binding regeneration and no `CARGO_BAZEL_REPIN`.

Flag OFF: the only deltas are two `Bool` tests per instrumented call and one lazily-init'd
static. Flag ON: per sample, one monotonic clock read, one integer subtraction, a handful
of integer accumulates and one array-slot write — no locks, no snapshot lock held, no
formatting or allocation outside the once-per-interval report.

## Contract amendments from the test-writer (BINDING — implement to match the tests)

The tests are written and pin the API. Where these conflict with the architect's plan
above, **these win** — the coder implements to match the tests, not the other way around.

**The plan's sample report block was internally inconsistent** — its header row and data
rows disagree by one column of padding. Header-consistent layout was chosen and pinned:
`bucket` left-14, `count` right-5, then four right-9 fields. Load-bearing pinned line:

```
wolf-frame-stats ctrl.wake      142*     3.11     2.40     9.80    41.20
```

**Formatting decisions pinned by the tests:**

- `wakesPerKeystroke == nil` renders as `wakes/key=n/a`
- every line, including the last, ends in `\n`
- `elapsedSeconds` formats with ONE decimal (`window=5.0s`); ms values with TWO
- the truncation marker is a `*` appended to the count INSIDE the 5-wide right-aligned field
- a counters-only report still emits the header and column rows (only `FrameMetrics.emit()`
  suppresses fully-idle intervals — that is glue, not `format`)

**API surface the tests require (plan named the behavior but not the signature):**

- `FrameStats.retainedSampleCount(_ bucket:) -> Int` — non-mutating; needed to assert the
  ring never exceeds `sampleCapacity` without reaching into private state
- `FrameStats.delta(start:end:) -> UInt64` — saturating subtraction, pinned as a STATIC on
  `FrameStats` (not `FrameMetrics`, which is untestable); `FrameMetrics.end` calls it
- `BucketSummary` and `Report` need PUBLIC MEMBERWISE INITS in the argument order listed in
  the architect's plan, so format-test fixtures can be constructed
- static: `format`, `percentile`, `isEnabled`, `reportInterval`, `delta`
- mutating: `record`, `recordKeystroke`, `recordWake`, `reset`
- non-mutating: `summarize`, `retainedSampleCount`

**Truncation boundary:** plan said `truncated` is true when `count > sampleCapacity`
(strictly greater). Pinned: `count == sampleCapacity` → `truncated == false`, since at
exactly capacity the window is lossless.

**Test file also adds** `testBucketRawValuesAndLabels`, pinning the `FrameStatsBucket`
rawValue→label contract, since the rawValues are the storage indices.

## CORRECTION to the architect's plan

The dev invocation path given above (and copied into `build.md`) was WRONG.
`macos_application` emits `Wolf.zip` plus an unzipped bundle under `Wolf_archive-root/`;
there is no `bazel-bin/crates/wolf-macos/Wolf.app`. The working path, verified by
actually launching it:

```
WOLF_FRAME_STATS=1 ./bazel-bin/crates/wolf-macos/Wolf_archive-root/Wolf.app/Contents/MacOS/Wolf
```

`build.md` is corrected. `.claude/verify-results.md` from the earlier Neovim-bundling
change had already recorded this ("an archive, NOT an unzipped Wolf.app dir") — the plan
contradicted an existing verified record.

## FIRST MEASUREMENTS (captured, real output)

Launch-and-type session, no preview open, `WOLF_FRAME_STATS_INTERVAL=2`:

```
wolf-frame-stats #3  window=2.0s
wolf-frame-stats bucket        count     mean      p50      p95      max
wolf-frame-stats ctrl.wake        22     5.46     4.96     8.83     9.11
wolf-frame-stats cont.wake        22     5.42     4.93     8.78     9.06
wolf-frame-stats draw             22    28.50    27.65    36.91    37.49
wolf-frame-stats keys=22 wakes=22 wakes/key=1.00
```

**This reprioritizes the follow-up work. Three findings:**

1. **`draw` dominates: ~28 ms mean, ~37 ms p95, on a FAST Mac with NO preview open.**
   That alone caps the frame rate near 35 fps and puts ~28 ms of latency behind every
   keystroke. It is roughly 5x the entire wake pass. Hypothesis #2 (per-cell CoreText
   shaping in `drawGlyph`) + #3 (triple cell-vector clone) are the prime suspects and are
   now the TOP priority — ahead of the preview work.
2. **`wakes/key = 1.00` — there is NO wake amplification.** Hypothesis #5 (uncoalesced
   wakes, several per keystroke) is FALSIFIED for the plain-typing case. Wake coalescing
   is NOT worth doing. Drop it.
3. **`ctrl.wake` (5.46) ≈ `cont.wake` (5.42) — the controller adds ~0.04 ms.** The
   architect's reason for instrumenting both is answered: the container is essentially the
   whole wake cost, and the controller's `current_file()`/`cwd()`/`refreshTabTitles` FFI
   round-trips are NOT a problem. Do not optimize the controller.

Preview buckets (`prev.*`) did not appear because no preview was open in this session.
A preview-open capture is still needed to size hypothesis #1.

## Status

- [x] architect plan
- [x] tests — `Tests/WolfKeyMapTests/FrameStatsTests.swift` + BUILD srcs (21 cases)
- [x] implementation — `FrameStats.swift`, `FrameMetrics.swift`, 5 call sites, BUILD
- [x] review — approved, no blocking findings; EPIPE + idle-ordering fixes applied
- [x] docs — `Sources/README.md`, `Tests/README.md`, `build.md`, `wolf-macos/README.md`
- [x] smoke test — app launches, reports emit to stderr, numbers captured above
- Tests: `WolfKeyMapTests` 179 pass / 0 fail, `bundle_layout_test` pass,
  `bazel build //crates/wolf-macos:Wolf` succeeds, `//tools/lint:swift_format` passes.

## Status: COMPLETE (instrumentation only — no render behavior changed, no Rust touched)
