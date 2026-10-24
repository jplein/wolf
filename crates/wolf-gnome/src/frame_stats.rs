//! The PURE, GTK-free frame-time aggregation seam for the keystroke-to-pixel path.
//!
//! This module owns every decision that can be made with no display server and no
//! window: which buckets exist, how samples accumulate, how percentiles are
//! computed, how a report is rendered, how the `WOLF_FRAME_STATS*` values are
//! parsed, and whether a report interval has elapsed. **It never reads a clock,
//! never touches the process environment and holds no process-global state** —
//! [`crate::frame_metrics`] owns all three, which is what keeps this file
//! unit-testable in a parallel `cargo test` process.
//!
//! Every parser here takes the RAW value as an `Option<&str>` rather than an
//! environment map, the rule [`crate::config_seam::resolve_font_pt`] already
//! establishes.
//!
//! # Two families, not one tree
//!
//! `queue_draw` only marks a widget dirty. GTK invokes the draw funcs from the
//! frame clock AFTER the wake handler has returned, so the `draw.*` buckets are
//! **not** nested inside [`Bucket::Wake`]: they are a second top-level family.
//!
//! | parent | children |
//! | --- | --- |
//! | `wake` | `wake.tabs`, `wake.tree`, `wake.shell` |
//! | `wake.shell` | `wake.state` |
//! | `wake.tabs` | `pane.wake` |
//! | `pane.wake` | `pane.state`, `pane.prev` |
//! | `pane.prev` | `prev.render`, `prev.load` |
//! | `draw.pane` | `draw.read`, `draw.grid`, `draw.status` |
//! | `draw.cmdline` | none; a sibling of `draw.pane` |
//!
//! A `pane.prev` region is opened either by a pane wake or by the preview
//! coalescer's one-shot trailing fire ([`crate::preview_coalescer`]), which is what
//! keeps `prev.render` from ever having no parent.
//!
//! # The double-count trap
//!
//! A nested bucket's time is already inside its parent's. Summing a parent
//! together with its children double-counts. Read a child as a SHARE of its
//! parent, never as an addend beside it.
//!
//! Three groups are mutually disjoint inside their parent, each with a residual
//! that is real work rather than overhead: `{wake.tabs, wake.tree, wake.shell}`
//! inside `wake`, whose residual is the liveness read plus loop overhead;
//! `{pane.state, pane.prev}` inside `pane.wake`, whose residual is the intent,
//! close, grid-resolution, drawer, grid-sync, active-pane and message-surface
//! passes; and `{draw.read, draw.grid, draw.status}` inside `draw.pane`, whose
//! residual is the whole-allocation background fill, the geometry, the cursor and
//! the scrollbar thumb. A residual over half its parent is itself a reading: the
//! table lacks resolution there, and the answer is another bucket and another run,
//! not a repair.
//!
//! # Count identities, before reading a table
//!
//! - `count(wake) == wakes` exactly, by construction: one pass over the drain-loop
//!   body counts one wake and times one `wake` region.
//! - `count(wake.tabs) <= count(wake)`: the tab layer runs only when liveness says
//!   painting is worth anything.
//! - `count(pane.wake)` may exceed `count(wake.tabs)` by two paths that reach the
//!   tab layer from outside the drain loop: the launch path's own synchronous first
//!   pass, which is deliberately not instrumented, and `shell::AppShell`'s
//!   `restart_nvim`, which calls the tab layer's wake again after a respawn. The
//!   restart arrives from a deferred one-shot timer or the crash overlay's Restart
//!   button, so it is outside every `wake` region and its `pane.wake` samples have
//!   no parent at all — not a `wake.tabs` and not a `wake`.
//! - `count(pane.prev)` may exceed `count(pane.wake)`: a trailing preview render
//!   runs from a one-shot timer outside every `wake` region, so it opens a
//!   `pane.prev` of its own with no `pane.wake` parent.
//! - `count(draw.status) >= count(draw.grid)`: the strip is never skipped, and the
//!   grid is skipped for a preview pane.
//! - `count(draw.pane)` is NOT `count(wake) * panes`: GTK coalesces frames.
//!
//! # `draw.read` is not the macOS `pane.read`
//!
//! macOS's bucket of that name times a deep clone across FFI. Here
//! `wolf_core::WolfCore::read_snapshot` is a `parking_lot` `read_arc()` that copies
//! nothing, so a large `draw.read` means lock contention with the tokio writer,
//! never copying.

use std::time::Duration;

/// The grep-able prefix stamped on every emitted line, including the last.
///
/// The macOS front-end emits the same string so the two tables can be `diff`ed;
/// changing it desynchronizes both front-ends and every `grep` a measurement run
/// depends on.
pub const LINE_PREFIX: &str = "wolf-frame-stats ";

/// The report interval, in seconds, when `WOLF_FRAME_STATS_INTERVAL` is absent or
/// unusable.
pub const DEFAULT_INTERVAL_SECS: f64 = 5.0;

/// Per-bucket bound on the samples retained for percentile computation.
///
/// Exceeding it loses neither `count`, the running sum nor `max` — only older
/// samples drop out of the percentile window, which the row then reports as
/// truncated.
pub const SAMPLE_CAPACITY: usize = 4096;

/// The longest a [`Bucket::label`] may be.
///
/// [`format_report`] left-aligns the label in a field of `LABEL_WIDTH`, and a
/// label that fills the field butts directly against the right-aligned count, so
/// the bound is one less than the field.
pub const MAX_LABEL_WIDTH: usize = 13;

/// The report table's label column, in characters.
const LABEL_WIDTH: usize = 14;

/// The report table's count column, in characters.
const COUNT_WIDTH: usize = 5;

/// Each of the report table's four millisecond columns, in characters.
const VALUE_WIDTH: usize = 9;

/// The longest interval [`report_interval`] accepts, in seconds. Inclusive.
///
/// A cap at all exists because `Duration::from_secs_f64` panics on an overflowing
/// value and this one comes from the user's environment; one hour is far past any
/// interval a measurement session would ask for.
const MAX_INTERVAL_SECS: f64 = 3600.0;

/// Nanoseconds per millisecond, the unit every report column is printed in.
const NANOS_PER_MILLI: f64 = 1_000_000.0;

/// The fixed set of instrumented regions on the keystroke-to-pixel path.
///
/// [`Bucket::index`] is the STORAGE index into [`FrameStats`]'s accumulator array,
/// which is what keeps the hot path free of hashing and string allocation, so the
/// indices must stay dense and ascending from zero. It is also the ONLY such index:
/// the variants carry no explicit discriminants and nothing casts one, so a second
/// spelling of the same numbering cannot drift out of step with the one the tests
/// pin. The nesting, the disjointness contract and the count identities are in this
/// module's docs; read them before reading a table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bucket {
    /// The whole drain-loop body in `window::build`'s render-wake future.
    Wake,
    /// The tab layer's per-wake pass, which runs only when liveness permits paint.
    WakeTabs,
    /// The sidebar's per-wake pass, under the same liveness gate.
    WakeTree,
    /// The shell's per-wake pass: re-root, title, nav buttons, state commit and the
    /// command drain. Ungated, so it runs in all three liveness states.
    WakeShell,
    /// The coordinator's state commit inside that shell pass.
    WakeState,
    /// One tab's whole pane-view wake, once per refreshed tab.
    PaneWake,
    /// The per-pane status refresh inside a pane wake.
    PaneState,
    /// The whole preview render pass, whether it runs inside a pane wake or from the
    /// preview coalescer's trailing fire.
    PanePrev,
    /// One `render_preview` blocking RPC, alone.
    PrevRender,
    /// One synchronous web-view document load, alone.
    PrevLoad,
    /// One pane's whole draw func.
    DrawPane,
    /// The snapshot read lock taken by that draw, alone.
    DrawRead,
    /// The grid paint inside that draw. Skipped, and so not recorded, for a preview
    /// pane.
    DrawGrid,
    /// The statusbar strip inside that draw. Recorded for a preview pane too.
    DrawStatus,
    /// The command-line strip's whole draw func, a sibling of [`Bucket::DrawPane`].
    DrawCmdline,
}

impl Bucket {
    /// Every bucket, in ascending [`Bucket::index`] order with no gaps.
    ///
    /// This both sizes and enumerates [`FrameStats`]'s storage, so a gap leaves an
    /// accumulator nothing ever reads and a repeat makes two buckets share one.
    pub const ALL: [Bucket; 15] = [
        Bucket::Wake,
        Bucket::WakeTabs,
        Bucket::WakeTree,
        Bucket::WakeShell,
        Bucket::WakeState,
        Bucket::PaneWake,
        Bucket::PaneState,
        Bucket::PanePrev,
        Bucket::PrevRender,
        Bucket::PrevLoad,
        Bucket::DrawPane,
        Bucket::DrawRead,
        Bucket::DrawGrid,
        Bucket::DrawStatus,
        Bucket::DrawCmdline,
    ];

    /// This bucket's dense storage index.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Bucket::Wake => 0,
            Bucket::WakeTabs => 1,
            Bucket::WakeTree => 2,
            Bucket::WakeShell => 3,
            Bucket::WakeState => 4,
            Bucket::PaneWake => 5,
            Bucket::PaneState => 6,
            Bucket::PanePrev => 7,
            Bucket::PrevRender => 8,
            Bucket::PrevLoad => 9,
            Bucket::DrawPane => 10,
            Bucket::DrawRead => 11,
            Bucket::DrawGrid => 12,
            Bucket::DrawStatus => 13,
            Bucket::DrawCmdline => 14,
        }
    }

    /// This bucket's report-table name, at most [`MAX_LABEL_WIDTH`] characters and
    /// distinct from every other bucket's.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Bucket::Wake => "wake",
            Bucket::WakeTabs => "wake.tabs",
            Bucket::WakeTree => "wake.tree",
            Bucket::WakeShell => "wake.shell",
            Bucket::WakeState => "wake.state",
            Bucket::PaneWake => "pane.wake",
            Bucket::PaneState => "pane.state",
            Bucket::PanePrev => "pane.prev",
            Bucket::PrevRender => "prev.render",
            Bucket::PrevLoad => "prev.load",
            Bucket::DrawPane => "draw.pane",
            Bucket::DrawRead => "draw.read",
            Bucket::DrawGrid => "draw.grid",
            Bucket::DrawStatus => "draw.status",
            Bucket::DrawCmdline => "draw.cmdline",
        }
    }
}

/// One bucket's exact counters plus its bounded ring of recent samples.
#[derive(Clone, Debug)]
struct Accumulator {
    /// The exact number of samples recorded, unbounded by the ring.
    count: u64,
    /// The exact sum of every sample recorded, unbounded by the ring.
    sum_nanos: u64,
    /// The exact largest sample, tracked OUTSIDE the ring so it stays exact
    /// however long the interval runs.
    max_nanos: u64,
    /// The last [`SAMPLE_CAPACITY`] samples, in ring order, over which the
    /// percentiles are computed.
    samples: Vec<u64>,
    /// The slot the next sample overwrites once the ring is full.
    write_index: usize,
}

impl Accumulator {
    /// An empty accumulator whose ring is reserved up front, so `record` never
    /// allocates on the hot path.
    fn new() -> Self {
        Self {
            count: 0,
            sum_nanos: 0,
            max_nanos: 0,
            samples: Vec::with_capacity(SAMPLE_CAPACITY),
            write_index: 0,
        }
    }

    /// Records one sample: exact into the counters, bounded into the ring.
    ///
    /// Saturating rather than wrapping arithmetic, because a wrapped sum would
    /// report a mean smaller than every sample it was computed from.
    fn record(&mut self, nanos: u64) {
        self.count = self.count.saturating_add(1);
        self.sum_nanos = self.sum_nanos.saturating_add(nanos);
        if nanos > self.max_nanos {
            self.max_nanos = nanos;
        }
        if self.samples.len() < SAMPLE_CAPACITY {
            self.samples.push(nanos);
            self.write_index = self.samples.len() % SAMPLE_CAPACITY;
        } else {
            self.samples[self.write_index] = nanos;
            self.write_index = (self.write_index + 1) % SAMPLE_CAPACITY;
        }
    }

    /// Zeroes every counter and empties the ring, keeping the allocation.
    fn reset(&mut self) {
        self.count = 0;
        self.sum_nanos = 0;
        self.max_nanos = 0;
        self.samples.clear();
        self.write_index = 0;
    }
}

/// Accumulator for per-bucket frame timings and the keystroke/wake counters.
///
/// `count`, the running sum and `max` are EXACT and unbounded. Only the percentile
/// window is bounded, to the last [`SAMPLE_CAPACITY`] samples per bucket, which
/// caps memory regardless of session length; a row whose count exceeded that is
/// reported as truncated.
///
/// There is no interior locking and nothing here is `Send`: the type is a plain
/// value, and the single-thread invariant belongs to [`crate::frame_metrics`].
pub struct FrameStats {
    /// Per-bucket accumulators, indexed by [`Bucket::index`].
    accumulators: [Accumulator; Bucket::ALL.len()],
    /// Keys delivered to Neovim since the last [`FrameStats::reset`].
    keystrokes: u64,
    /// Core wakes observed since the last [`FrameStats::reset`].
    wakes: u64,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameStats {
    /// An empty accumulator, reserving each bucket's ring up front so the hot path
    /// never allocates.
    #[must_use]
    pub fn new() -> Self {
        Self {
            accumulators: std::array::from_fn(|_| Accumulator::new()),
            keystrokes: 0,
            wakes: 0,
        }
    }

    /// Records one timing sample, in nanoseconds, against `bucket`. O(1), no
    /// allocation.
    pub fn record(&mut self, bucket: Bucket, nanos: u64) {
        self.accumulators[bucket.index()].record(nanos);
    }

    /// Counts one key actually delivered to Neovim.
    pub fn record_keystroke(&mut self) {
        self.keystrokes = self.keystrokes.saturating_add(1);
    }

    /// Counts one core wake.
    pub fn record_wake(&mut self) {
        self.wakes = self.wakes.saturating_add(1);
    }

    /// Zeroes every counter and empties every ring, keeping the allocations, so
    /// each report covers only its own interval and two reports stay comparable.
    pub fn reset(&mut self) {
        for accumulator in &mut self.accumulators {
            accumulator.reset();
        }
        self.keystrokes = 0;
        self.wakes = 0;
    }

    /// True when nothing at all was recorded since the last [`FrameStats::reset`].
    ///
    /// COUNT-based, not sum-based: a sample of zero nanoseconds is still a sample.
    /// This is the emit gate — [`crate::frame_metrics::maybe_emit`] re-baselines
    /// and writes nothing when it answers true, which is what keeps an idle
    /// application from emitting an all-zero report every interval.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keystrokes == 0 && self.wakes == 0 && self.accumulators.iter().all(|a| a.count == 0)
    }

    /// The number of samples currently retained for percentile computation. Never
    /// exceeds [`SAMPLE_CAPACITY`].
    #[must_use]
    pub fn retained_sample_count(&self, bucket: Bucket) -> usize {
        self.accumulators[bucket.index()].samples.len()
    }

    /// Builds one interval's [`Report`], omitting every zero-count bucket.
    ///
    /// The omission is contract rather than tidiness: an absent row is how a reader
    /// — and the cage smoke — tells "this region was never entered" from "this
    /// region was entered and cost nothing".
    ///
    /// This is the ONLY place anything is sorted, once per report interval, never
    /// per frame.
    #[must_use]
    pub fn summarize(&self, index: u64, elapsed_secs: f64) -> Report {
        let capacity = u64::try_from(SAMPLE_CAPACITY).unwrap_or(u64::MAX);
        let mut buckets = Vec::with_capacity(Bucket::ALL.len());
        for bucket in Bucket::ALL {
            let accumulator = &self.accumulators[bucket.index()];
            if accumulator.count == 0 {
                continue;
            }
            let mut sorted = accumulator.samples.clone();
            sorted.sort_unstable();
            buckets.push(BucketSummary {
                bucket,
                count: accumulator.count,
                mean_nanos: accumulator.sum_nanos / accumulator.count,
                p50_nanos: percentile(&sorted, 0.5),
                p95_nanos: percentile(&sorted, 0.95),
                max_nanos: accumulator.max_nanos,
                truncated: accumulator.count > capacity,
            });
        }

        let wakes_per_keystroke = if self.keystrokes > 0 {
            Some(self.wakes as f64 / self.keystrokes as f64)
        } else {
            None
        };

        Report {
            index,
            elapsed_secs,
            buckets,
            keystrokes: self.keystrokes,
            wakes: self.wakes,
            wakes_per_keystroke,
        }
    }
}

/// The aggregated statistics for a single bucket over one report interval.
#[derive(Clone, Debug)]
pub struct BucketSummary {
    /// Which instrumented region this row describes.
    pub bucket: Bucket,
    /// The exact number of samples recorded in the interval.
    pub count: u64,
    /// Truncating integer mean, `sum / count`.
    pub mean_nanos: u64,
    /// Nearest-rank 50th percentile over the retained window.
    pub p50_nanos: u64,
    /// Nearest-rank 95th percentile over the retained window.
    pub p95_nanos: u64,
    /// The exact largest sample, tracked outside the ring.
    pub max_nanos: u64,
    /// True when `count > SAMPLE_CAPACITY`, i.e. the two percentiles are windowed
    /// while the count, the mean and the max are not.
    pub truncated: bool,
}

/// One report interval's aggregates, ready for [`format_report`].
#[derive(Clone, Debug)]
pub struct Report {
    /// Monotonically increasing report number within the process run.
    pub index: u64,
    /// Wall-clock length of the interval this report covers, in seconds.
    pub elapsed_secs: f64,
    /// Only the buckets that recorded at least one sample, in any order —
    /// [`format_report`] sorts.
    pub buckets: Vec<BucketSummary>,
    /// Keys actually delivered to Neovim during the interval.
    pub keystrokes: u64,
    /// Core wakes observed during the interval.
    pub wakes: u64,
    /// `wakes / keystrokes`, or `None` when no keystroke was recorded.
    ///
    /// An UPPER BOUND on the wakes attributable to typing rather than an
    /// attribution: wakes triggered by mouse, scroll, resize or out-of-band core
    /// notifications all inflate it.
    pub wakes_per_keystroke: Option<f64>,
}

/// Nearest-rank percentile over an ALREADY-SORTED ascending slice.
///
/// `index = clamp(ceil(p * n) - 1, 0, n - 1)`, and `0` for an empty slice.
/// Interpolating variants are rejected deliberately: nearest-rank always answers a
/// value some frame actually took, which is what makes the p95 column trustworthy
/// as a frame-budget reading.
#[must_use]
pub fn percentile(sorted: &[u64], p: f64) -> u64 {
    let n = sorted.len();
    if n == 0 {
        return 0;
    }
    // Precision loss needs a slice longer than 2^53; the ring bounds this at
    // SAMPLE_CAPACITY.
    let len = n as f64;
    let rank = (p * len).ceil();
    let index = if rank.is_nan() || rank < 1.0 {
        // `p <= 0`, or a NaN `p`: clamp up to the smallest sample rather than
        // computing a negative index.
        0
    } else if rank >= len {
        n - 1
    } else {
        // Bounded to `1.0 ..= len - 1` by the two arms above, so the conversion is
        // exact and the subtraction cannot underflow.
        (rank as usize) - 1
    };
    sorted[index]
}

/// Renders a report as a fixed-width, `diff`-able, fully prefixed table.
///
/// Rows are emitted in ascending [`Bucket::index`] order regardless of the order
/// they appear in `report.buckets`, so two runs — and the two front-ends — align
/// under `diff`. Every line, including the last, ends in a newline.
#[must_use]
pub fn format_report(report: &Report) -> String {
    let mut text = String::new();

    text.push_str(LINE_PREFIX);
    text.push_str(&format!(
        "#{}  window={:.1}s\n",
        report.index, report.elapsed_secs
    ));

    text.push_str(LINE_PREFIX);
    text.push_str(&format!(
        "{:<LABEL_WIDTH$}{:>COUNT_WIDTH$}{:>VALUE_WIDTH$}{:>VALUE_WIDTH$}\
         {:>VALUE_WIDTH$}{:>VALUE_WIDTH$}\n",
        "bucket", "count", "mean", "p50", "p95", "max"
    ));

    let mut rows: Vec<&BucketSummary> = report.buckets.iter().collect();
    rows.sort_by_key(|summary| summary.bucket.index());
    for summary in rows {
        // The truncation marker sits INSIDE the count field, so a marked row keeps
        // its column alignment.
        let count_field = if summary.truncated {
            format!("{}*", summary.count)
        } else {
            format!("{}", summary.count)
        };
        text.push_str(LINE_PREFIX);
        text.push_str(&format!(
            "{:<LABEL_WIDTH$}{:>COUNT_WIDTH$}{:>VALUE_WIDTH$}{:>VALUE_WIDTH$}\
             {:>VALUE_WIDTH$}{:>VALUE_WIDTH$}\n",
            summary.bucket.label(),
            count_field,
            millis(summary.mean_nanos),
            millis(summary.p50_nanos),
            millis(summary.p95_nanos),
            millis(summary.max_nanos)
        ));
    }

    // `n/a` rather than `0.00`, `NaN` or `inf`: each of those states a ratio for an
    // interval in which no keystroke was recorded.
    let ratio = report
        .wakes_per_keystroke
        .map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}"));
    text.push_str(LINE_PREFIX);
    text.push_str(&format!(
        "keys={} wakes={} wakes/key={ratio}\n",
        report.keystrokes, report.wakes
    ));

    text
}

/// True when `WOLF_FRAME_STATS` is set to `1`, `true`, `yes` or `on`, compared
/// case-insensitively after trimming. Anything else — absent, empty or
/// unrecognized — is false, so the instrument is never enabled by presence alone.
#[must_use]
pub fn is_enabled(raw: Option<&str>) -> bool {
    let Some(raw) = raw else {
        return false;
    };
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Parses `WOLF_FRAME_STATS_INTERVAL` as seconds, defaulting to
/// [`DEFAULT_INTERVAL_SECS`].
///
/// **The guard is not defensive noise.** `Duration::from_secs_f64` PANICS on a
/// negative, non-finite or overflowing value, and this one comes straight from the
/// user's environment. Only a finite, strictly positive value of at most
/// `MAX_INTERVAL_SECS` — inclusive — is accepted; everything else takes the
/// default.
#[must_use]
pub fn report_interval(raw: Option<&str>) -> Duration {
    let default = Duration::from_secs_f64(DEFAULT_INTERVAL_SECS);
    let Some(raw) = raw else {
        return default;
    };
    let Ok(secs) = raw.trim().parse::<f64>() else {
        return default;
    };
    if !secs.is_finite() || secs <= 0.0 || secs > MAX_INTERVAL_SECS {
        return default;
    }
    Duration::from_secs_f64(secs)
}

/// The path `WOLF_FRAME_STATS_FILE` names, trimmed, or `None` when the variable is
/// absent or names nothing.
///
/// **The trim decides both halves, and that is the whole reason this is a function
/// rather than a filter at the call site.** Testing one string for emptiness and
/// handing another to `open` makes `" "` name no file while `" /tmp/x "` names a
/// file whose first and last characters are spaces — created silently, in the
/// directory the user meant, under a name they did not write.
///
/// macOS resolves the same variable the same way (`FrameMetrics.statsFileHandle`
/// trims, rejects the empty result and opens the trimmed path), which is what keeps
/// one documented surface serving both front-ends.
#[must_use]
pub fn report_file_path(raw: Option<&str>) -> Option<&str> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Whether a report is due: `since_last >= interval`, INCLUSIVE at the boundary.
///
/// Inclusive because nothing in this crate polls. A report rides a wake, so a
/// deferred one is deferred to the NEXT wake, and an application that goes idle at
/// the boundary then emits nothing at all.
#[must_use]
pub fn interval_elapsed(since_last: Duration, interval: Duration) -> bool {
    since_last >= interval
}

/// Nanoseconds as a two-decimal millisecond string, the unit every value column is
/// printed in.
fn millis(nanos: u64) -> String {
    // Precision loss needs a sample above 2^53 nanoseconds, i.e. a frame of about
    // 104 days.
    format!("{:.2}", nanos as f64 / NANOS_PER_MILLI)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // No clock, environment or GTK here or below: durations are built directly and env values arrive
    // as `Option<&str>`, since a parallel `cargo test` must never mutate its environment.

    /// Nanoseconds per millisecond, so a fixture reads as a duration a human can
    /// picture rather than as a seven-digit integer.
    const NS_PER_MS: u64 = 1_000_000;

    /// Tolerance for the one `f64` this seam computes — the wakes/keystroke ratio.
    const RATIO_EPS: f64 = 1e-9;

    /// [`LINE_PREFIX`] re-typed as a LITERAL, per this repo's
    /// re-declare-do-not-import discipline (the smoke suite's
    /// `EXPECT_SMOKE_APPEARANCE` precedent).
    ///
    /// Reading the const and comparing it to itself would hold for whatever value it
    /// took, including one that breaks every `grep wolf-frame-stats` a measurement
    /// run depends on and desynchronizes this table from the macOS one it is meant
    /// to `diff` against.
    const EXPECT_LINE_PREFIX: &str = "wolf-frame-stats ";

    /// [`MAX_LABEL_WIDTH`] re-typed as a LITERAL, for the same reason.
    ///
    /// It is `14 - 1`: [`format_report`] left-aligns the label in a width-14 field,
    /// so a 14-character label consumes the whole field and butts directly against
    /// the right-aligned count column. Asserting label lengths against the const
    /// alone would pass for any value the const took, including 100.
    const EXPECT_MAX_LABEL_WIDTH: usize = 13;

    /// [`SAMPLE_CAPACITY`] re-typed as a LITERAL, for the same reason, and because
    /// [`ring_bound_keeps_exact_count_and_windowed_percentiles`] states its expected
    /// percentiles as literal ranks derived by hand at THIS capacity.
    const EXPECT_SAMPLE_CAPACITY: usize = 4096;

    /// A distinct, bucket-specific sample: `(index + 1)` milliseconds.
    ///
    /// Distinct per bucket on purpose — a fixture that recorded the same value
    /// everywhere would be satisfied by an accumulator that wrote every sample into
    /// one shared slot.
    fn distinct_sample(bucket: Bucket) -> u64 {
        let index = u64::try_from(bucket.index()).expect("a bucket index fits in a u64");
        (index + 1) * NS_PER_MS
    }

    /// The one summary for `bucket` in `report`, or a panic naming the bucket.
    ///
    /// A helper rather than an inline `find` so a missing row fails with the label
    /// rather than with `called Option::unwrap on a None value`.
    fn summary_for(report: &Report, bucket: Bucket) -> &BucketSummary {
        report
            .buckets
            .iter()
            .find(|s| s.bucket == bucket)
            .unwrap_or_else(|| {
                panic!(
                    "the report carries no row for {:?} ({}), so either `record` wrote the sample \
                     into another bucket's accumulator or `summarize` dropped a non-empty one. \
                     Rows present: {:?}",
                    bucket,
                    bucket.label(),
                    report
                        .buckets
                        .iter()
                        .map(|s| s.bucket.label())
                        .collect::<Vec<_>>()
                )
            })
    }

    /// Joins report lines the way [`format_report`] must: every line, including the
    /// last, terminated by `\n`.
    fn joined(lines: &[&str]) -> String {
        let mut text = String::new();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        text
    }

    /// The label of each bucket row in a formatted report, in the order the rows
    /// appear.
    ///
    /// The label is the first whitespace-separated token after the prefix, which is
    /// what makes this discriminate `wake` from `wake.tabs`: a plain `contains`
    /// cannot, because every `wake.*` label carries `wake` as a prefix.
    fn row_labels(text: &str) -> Vec<&str> {
        text.lines()
            .filter_map(|line| line.strip_prefix(EXPECT_LINE_PREFIX))
            .filter(|body| {
                !body.starts_with('#') && !body.starts_with("bucket ") && !body.starts_with("keys=")
            })
            .filter_map(|body| body.split_whitespace().next())
            .collect()
    }

    // --- 0. Bucket identity ---

    /// Pins every variant's storage index and report label, and that [`Bucket::ALL`]
    /// is `0..15` ascending with no gaps.
    ///
    /// The index is the STORAGE index into [`FrameStats`]'s accumulator array, which
    /// is what keeps the hot path free of hashing and string allocation, so a gap or
    /// a duplicate is a wrong-slot write rather than a cosmetic defect. The label is
    /// the report-table name, and the macOS front-end's table is meant to `diff`
    /// against this one, so both halves are contract.
    ///
    /// Falsifying edit (from the plan): change `DrawGrid`'s label to `"draw.cells"`.
    #[test]
    fn bucket_indices_and_labels_table() {
        let cases: [(Bucket, usize, &str); 15] = [
            (Bucket::Wake, 0, "wake"),
            (Bucket::WakeTabs, 1, "wake.tabs"),
            (Bucket::WakeTree, 2, "wake.tree"),
            (Bucket::WakeShell, 3, "wake.shell"),
            (Bucket::WakeState, 4, "wake.state"),
            (Bucket::PaneWake, 5, "pane.wake"),
            (Bucket::PaneState, 6, "pane.state"),
            (Bucket::PanePrev, 7, "pane.prev"),
            (Bucket::PrevRender, 8, "prev.render"),
            (Bucket::PrevLoad, 9, "prev.load"),
            (Bucket::DrawPane, 10, "draw.pane"),
            (Bucket::DrawRead, 11, "draw.read"),
            (Bucket::DrawGrid, 12, "draw.grid"),
            (Bucket::DrawStatus, 13, "draw.status"),
            (Bucket::DrawCmdline, 14, "draw.cmdline"),
        ];

        for (bucket, index, label) in cases {
            assert_eq!(
                bucket.index(),
                index,
                "{label}: index() must be {index}. The index is the array slot this bucket's \
                 accumulator lives in, so a wrong one silently writes {label}'s samples into \
                 another bucket's row — or, past the end of the array, panics on the hot path."
            );
            assert_eq!(
                bucket.label(),
                label,
                "index {index}: label() must be {label:?}. Every report row and every smoke \
                 assertion names the bucket by this string, and the macOS table is meant to `diff` \
                 against this one column for column."
            );
        }

        assert_eq!(
            Bucket::ALL.len(),
            cases.len(),
            "Bucket::ALL must list exactly the {} pinned buckets. A variant missing from ALL is \
             recorded into but never summarized — its samples vanish with no row and no warning — \
             and an extra one sizes the accumulator array past what `index()` can reach.",
            cases.len()
        );

        let indices: Vec<usize> = Bucket::ALL.iter().map(|b| b.index()).collect();
        let ascending: Vec<usize> = (0..cases.len()).collect();
        assert_eq!(
            indices, ascending,
            "Bucket::ALL must be in ascending index order with NO gaps, because it both sizes and \
             enumerates the storage array. A gap leaves an accumulator nothing ever reads; a \
             repeat makes two variants share one."
        );
    }

    /// Pins that no two buckets share a label and that every label fits the report's
    /// label column.
    ///
    /// UNIQUENESS matters because `label()` is a hand-written match: a copy-paste
    /// duplicate compiles, and would emit two identically-named rows that no other
    /// test here rejects — and it would disarm the smoke's per-label assertion,
    /// which searches for a label and would find the wrong row.
    ///
    /// THE WIDTH matters because [`format_report`] left-aligns the label in a
    /// width-14 field: a 14-character label fills it and butts against the
    /// right-aligned count, so the table stops being column-aligned and the two
    /// front-ends' reports stop being `diff`-able.
    ///
    /// Falsifying edit (from the plan): make `WakeTree` return `"wake.tabs"`.
    #[test]
    fn bucket_labels_are_unique_and_fit_the_report_column() {
        assert_eq!(
            MAX_LABEL_WIDTH, EXPECT_MAX_LABEL_WIDTH,
            "MAX_LABEL_WIDTH is {MAX_LABEL_WIDTH}, but the label column in `format_report` is 14 \
             wide and a label needs at least one trailing space to separate it from the count \
             column, so the bound is {EXPECT_MAX_LABEL_WIDTH}. The expected value is re-typed here \
             rather than read from the const so that widening the const cannot make the length \
             checks below vacuous."
        );

        let mut seen: Vec<&'static str> = Vec::new();
        for bucket in Bucket::ALL {
            let label = bucket.label();
            assert!(
                !label.is_empty(),
                "index {}: the label is empty, so its report row would be an unnamed line of \
                 numbers.",
                bucket.index()
            );
            assert!(
                !seen.contains(&label),
                "duplicate label {label:?} at index {}. Two buckets sharing a label emit two \
                 identically-named rows: the reader cannot tell which region either row measures, \
                 and the cage smoke's per-label search matches the FIRST of them, so one bucket \
                 can be entirely unwired while the smoke stays green.",
                bucket.index()
            );
            seen.push(label);
            assert!(
                label.len() <= EXPECT_MAX_LABEL_WIDTH,
                "label {label:?} is {} characters; the width-14 label column needs at least one \
                 trailing space to separate it from the right-aligned count, so no label may \
                 exceed {EXPECT_MAX_LABEL_WIDTH}. A longer one runs the two columns together and \
                 the table stops being `diff`-able against the macOS report.",
                label.len()
            );
        }

        assert_eq!(
            seen.len(),
            Bucket::ALL.len(),
            "every bucket must have its own distinct label"
        );
    }

    // --- 1. Environment parsing (the raw value, never the environment) ---

    /// One [`is_enabled`] row.
    struct EnabledCase {
        desc: &'static str,
        raw: Option<&'static str>,
        expected: bool,
    }

    /// Pins [`is_enabled`]: `1` / `true` / `yes` / `on`, compared case-insensitively
    /// after trimming; anything else — including absent, empty and whitespace — is
    /// false.
    ///
    /// The spellings are the macOS ones deliberately, so one documented surface
    /// serves both front-ends. The parser takes the RAW value rather than a map, so
    /// this test touches no process environment.
    ///
    /// Falsifying edit (from the plan): drop the `trim()` (row `" on "` fires) or
    /// drop the lowercasing (rows `"TRUE"` and `"Yes"` fire).
    #[test]
    fn is_enabled_table() {
        let cases = [
            EnabledCase {
                desc: "absent -> disabled, which is the shipped default",
                raw: None,
                expected: false,
            },
            EnabledCase {
                desc: "an empty value -> disabled",
                raw: Some(""),
                expected: false,
            },
            EnabledCase {
                desc: "whitespace only -> disabled (it trims to empty)",
                raw: Some("   "),
                expected: false,
            },
            EnabledCase {
                desc: "\"1\" -> enabled",
                raw: Some("1"),
                expected: true,
            },
            EnabledCase {
                desc: "\"true\" -> enabled",
                raw: Some("true"),
                expected: true,
            },
            EnabledCase {
                desc: "\"TRUE\" -> enabled (case-insensitive)",
                raw: Some("TRUE"),
                expected: true,
            },
            EnabledCase {
                desc: "\"Yes\" -> enabled (case-insensitive)",
                raw: Some("Yes"),
                expected: true,
            },
            EnabledCase {
                desc: "\"on\" -> enabled",
                raw: Some("on"),
                expected: true,
            },
            EnabledCase {
                desc: "\" on \" -> enabled (trimmed first)",
                raw: Some(" on "),
                expected: true,
            },
            EnabledCase {
                desc: "\"0\" -> disabled",
                raw: Some("0"),
                expected: false,
            },
            EnabledCase {
                desc: "\"false\" -> disabled",
                raw: Some("false"),
                expected: false,
            },
            EnabledCase {
                desc: "\"no\" -> disabled",
                raw: Some("no"),
                expected: false,
            },
            EnabledCase {
                desc: "\"off\" -> disabled",
                raw: Some("off"),
                expected: false,
            },
            EnabledCase {
                desc: "an unrecognized value -> disabled, never enabled-by-presence",
                raw: Some("maybe"),
                expected: false,
            },
        ];

        for case in &cases {
            assert_eq!(
                is_enabled(case.raw),
                case.expected,
                "case: {} — is_enabled({:?}) must be {}. A false where true is expected leaves the \
                 instrument silently off and a measurement run produces an empty file; a true \
                 where false is expected arms it for every user who has the variable set to \
                 something that is not an affirmative.",
                case.desc,
                case.raw,
                case.expected
            );
        }
    }

    /// One [`report_interval`] row.
    struct IntervalCase {
        desc: &'static str,
        raw: Option<&'static str>,
        expected: Duration,
    }

    /// Pins [`report_interval`]: seconds as an `f64`, defaulting to 5 s for anything
    /// absent, unparseable, non-positive, non-finite or longer than an hour.
    ///
    /// THE GUARD IS NOT DEFENSIVE NOISE. `Duration::from_secs_f64` **panics** on a
    /// negative, non-finite or overflowing value, and this value comes from the
    /// user's environment, so an unguarded conversion turns `WOLF_FRAME_STATS_INTERVAL=NaN`
    /// into a crash at startup. Rows `"-1"`, `"inf"`, `"NaN"` and `"1e400"` are the
    /// four shapes that panic; `"abc"` is the one that merely fails to parse.
    ///
    /// Every expectation is a literal `Duration`, never `Duration::from_secs_f64`
    /// applied to the same input the production code sees: an expectation derived
    /// through the function under test holds whatever that function does.
    ///
    /// Falsifying edit (from the plan): delete the `v > 0.0` guard — rows `"0"` and
    /// `"-1"` fire, the second by panicking rather than by comparing.
    ///
    /// ONE READING THE PLAN LEAVES OPEN, stated so it can be corrected in one place:
    /// "at most one hour" is read here as INCLUSIVE, so `"3600"` is accepted and
    /// `"3600.5"` is not.
    #[test]
    fn report_interval_table() {
        let cases = [
            IntervalCase {
                desc: "absent -> the 5 s default",
                raw: None,
                expected: Duration::from_secs(5),
            },
            IntervalCase {
                desc: "an empty value -> the default",
                raw: Some(""),
                expected: Duration::from_secs(5),
            },
            IntervalCase {
                desc: "whitespace only -> the default (it trims to empty)",
                raw: Some("   "),
                expected: Duration::from_secs(5),
            },
            IntervalCase {
                desc: "\"2.5\" -> 2.5 s",
                raw: Some("2.5"),
                expected: Duration::from_millis(2500),
            },
            IntervalCase {
                desc: "\" 2.5 \" -> 2.5 s (trimmed first)",
                raw: Some(" 2.5 "),
                expected: Duration::from_millis(2500),
            },
            IntervalCase {
                desc: "an integer string parses as a float",
                raw: Some("10"),
                expected: Duration::from_secs(10),
            },
            IntervalCase {
                desc: "zero is not strictly positive -> the default",
                raw: Some("0"),
                expected: Duration::from_secs(5),
            },
            IntervalCase {
                desc: "negative -> the default (from_secs_f64 would PANIC on it)",
                raw: Some("-1"),
                expected: Duration::from_secs(5),
            },
            IntervalCase {
                desc: "unparseable -> the default",
                raw: Some("abc"),
                expected: Duration::from_secs(5),
            },
            IntervalCase {
                desc: "\"inf\" parses to +inf -> the default (from_secs_f64 would PANIC)",
                raw: Some("inf"),
                expected: Duration::from_secs(5),
            },
            IntervalCase {
                desc: "\"NaN\" parses to NaN -> the default (from_secs_f64 would PANIC)",
                raw: Some("NaN"),
                expected: Duration::from_secs(5),
            },
            IntervalCase {
                desc: "an overflowing literal parses to +inf -> the default",
                raw: Some("1e400"),
                expected: Duration::from_secs(5),
            },
            IntervalCase {
                desc: "exactly one hour is accepted (the bound is inclusive)",
                raw: Some("3600"),
                expected: Duration::from_secs(3600),
            },
            IntervalCase {
                desc: "past one hour -> the default",
                raw: Some("3600.5"),
                expected: Duration::from_secs(5),
            },
        ];

        for case in &cases {
            assert_eq!(
                report_interval(case.raw),
                case.expected,
                "case: {} — report_interval({:?}) must be {:?}. Read a PANIC here rather than a \
                 comparison failure as the guard being gone entirely: `Duration::from_secs_f64` \
                 aborts on a negative, infinite or NaN input, and this value comes straight from \
                 the user's environment.",
                case.desc,
                case.raw,
                case.expected
            );
        }
    }

    /// One [`report_file_path`] row.
    struct FilePathCase {
        desc: &'static str,
        raw: Option<&'static str>,
        expected: Option<&'static str>,
    }

    /// Pins [`report_file_path`]: the trimmed value, or `None` when the variable is
    /// absent or trims to nothing.
    ///
    /// THE TWO HALVES MUST USE THE SAME STRING. A filter that trims to decide
    /// emptiness and then hands the UNTRIMMED value to `open` passes the absent, the
    /// empty and the plain-path rows and fails only the last one here — and in
    /// production it creates `" /tmp/x "`, a file whose name begins and ends with a
    /// space, in the directory the user meant but under a name they never wrote.
    ///
    /// The empty and whitespace-only rows are the reason the function exists at all:
    /// `File::open("")` fails with `ENOENT` rather than degrading, and the macOS side
    /// (`FrameMetrics.statsFileHandle`) rejects both, so accepting them here would put
    /// the two front-ends' handling of one documented variable out of step.
    ///
    /// Falsifying edits: return `raw` untrimmed — the surrounding-whitespace row
    /// fires; drop the `is_empty` arm — the empty and whitespace-only rows fire.
    #[test]
    fn report_file_path_table() {
        let cases = [
            FilePathCase {
                desc: "absent -> no file, which is the shipped default",
                raw: None,
                expected: None,
            },
            FilePathCase {
                desc: "an empty value -> no file, never an attempt to open \"\"",
                raw: Some(""),
                expected: None,
            },
            FilePathCase {
                desc: "whitespace only -> no file (it trims to empty)",
                raw: Some("   "),
                expected: None,
            },
            FilePathCase {
                desc: "a plain path passes through unchanged",
                raw: Some("/tmp/wolf-frame-stats.txt"),
                expected: Some("/tmp/wolf-frame-stats.txt"),
            },
            FilePathCase {
                desc: "surrounding whitespace is trimmed OFF THE VALUE, not just tested",
                raw: Some("  /tmp/wolf-frame-stats.txt  "),
                expected: Some("/tmp/wolf-frame-stats.txt"),
            },
            FilePathCase {
                desc: "a trailing newline, which a shell here-doc or a $(...) leaves behind",
                raw: Some("/tmp/wolf-frame-stats.txt\n"),
                expected: Some("/tmp/wolf-frame-stats.txt"),
            },
            FilePathCase {
                desc: "INTERIOR spaces are part of the path and are kept",
                raw: Some(" /tmp/wolf frame stats.txt "),
                expected: Some("/tmp/wolf frame stats.txt"),
            },
            FilePathCase {
                desc: "a relative path is not resolved here; that is the opener's business",
                raw: Some("stats.txt"),
                expected: Some("stats.txt"),
            },
        ];

        for case in &cases {
            assert_eq!(
                report_file_path(case.raw),
                case.expected,
                "case: {} — report_file_path({:?}) must be {:?}. A `Some` where `None` is expected \
                 sends an empty or blank path to `OpenOptions::open`, which fails and degrades the \
                 run to stderr-only for a user who did ask for a file; an untrimmed `Some` opens a \
                 DIFFERENT path from the one that was tested, so the value the emptiness check \
                 approved is not the value that gets created.",
                case.desc,
                case.raw,
                case.expected
            );
        }
    }

    /// One [`interval_elapsed`] row.
    struct ElapsedCase {
        desc: &'static str,
        since: Duration,
        interval: Duration,
        expected: bool,
    }

    /// Pins [`interval_elapsed`] as `since >= interval` — INCLUSIVE at the boundary.
    ///
    /// Every [`Duration`] is constructed directly and nothing sleeps: the emit
    /// decision is a comparison between two values, and testing it against a real
    /// clock would measure the scheduler instead.
    ///
    /// Falsifying edit (from the plan): change `>=` to `>` — the exactly-equal rows
    /// fire, and nothing else does.
    #[test]
    fn interval_elapsed_table() {
        let cases = [
            ElapsedCase {
                desc: "nothing has elapsed",
                since: Duration::ZERO,
                interval: Duration::from_secs(5),
                expected: false,
            },
            ElapsedCase {
                desc: "one nanosecond short",
                since: Duration::new(4, 999_999_999),
                interval: Duration::from_secs(5),
                expected: false,
            },
            ElapsedCase {
                desc: "EXACTLY the interval -> elapsed (the boundary is inclusive)",
                since: Duration::from_secs(5),
                interval: Duration::from_secs(5),
                expected: true,
            },
            ElapsedCase {
                desc: "one nanosecond past",
                since: Duration::new(5, 1),
                interval: Duration::from_secs(5),
                expected: true,
            },
            ElapsedCase {
                desc: "well past the interval (a long idle between wakes)",
                since: Duration::from_secs(10),
                interval: Duration::from_secs(5),
                expected: true,
            },
            ElapsedCase {
                desc: "a zero interval is always elapsed, so the seam stays total",
                since: Duration::ZERO,
                interval: Duration::ZERO,
                expected: true,
            },
        ];

        for case in &cases {
            assert_eq!(
                interval_elapsed(case.since, case.interval),
                case.expected,
                "case: {} — interval_elapsed({:?}, {:?}) must be {}. This is the whole emit gate: \
                 a false where true is expected means a report is deferred to the NEXT wake, and \
                 since nothing in this crate polls, an idle application then emits nothing at all.",
                case.desc,
                case.since,
                case.interval,
                case.expected
            );
        }
    }

    // --- 2. Accumulation ---

    /// Pins the mean as TRUNCATING integer division, `sum / count`.
    ///
    /// `10 + 20 + 31 = 61` over three samples is `20`, not `20.33` and not `21`.
    /// Rounding instead would move every mean in every report by up to half a
    /// nanosecond — harmless on its own, but the plan's repair rules rank buckets by
    /// `count * mean`, so the mean's definition is contract rather than cosmetic.
    #[test]
    fn basic_accumulation_pins_integer_mean() {
        let mut stats = FrameStats::new();
        for nanos in [10_u64, 20, 31] {
            stats.record(Bucket::DrawPane, nanos);
        }

        let report = stats.summarize(1, 5.0);
        let draw = summary_for(&report, Bucket::DrawPane);

        assert_eq!(
            draw.count, 3,
            "count is the exact number of samples recorded"
        );
        assert_eq!(
            draw.mean_nanos, 20,
            "the mean is truncating integer division: 61 / 3 == 20. A 21 here means the \
             implementation rounds, which is a different statistic from the one the report's \
             column claims and from the one macOS emits."
        );
        assert_eq!(
            draw.max_nanos, 31,
            "max is the largest sample and is tracked OUTSIDE the percentile ring, so it stays \
             exact however long the session runs"
        );
        assert!(
            !draw.truncated,
            "three samples is far below SAMPLE_CAPACITY, so the percentile window holds all of \
             them and the row must not be marked truncated"
        );
    }

    /// With exactly one sample every statistic collapses to that sample.
    ///
    /// This is the degenerate arm of the percentile arithmetic: `ceil(p * 1) - 1`
    /// clamps to index 0 for every `p`, so a p95 that differs from the single sample
    /// means the rank is being computed some other way.
    #[test]
    fn single_sample_collapses_to_one_value() {
        let mut stats = FrameStats::new();
        stats.record(Bucket::PrevRender, 42 * NS_PER_MS);

        let report = stats.summarize(1, 5.0);
        let render = summary_for(&report, Bucket::PrevRender);

        assert_eq!(render.count, 1, "one sample was recorded");
        for (name, got) in [
            ("mean", render.mean_nanos),
            ("p50", render.p50_nanos),
            ("p95", render.p95_nanos),
            ("max", render.max_nanos),
        ] {
            assert_eq!(
                got,
                42 * NS_PER_MS,
                "{name} of a single-sample bucket must be that sample. A 0 here means the rank \
                 computation ran off the front of the slice; anything else means it ran off the \
                 back."
            );
        }
    }

    // --- 3. Percentile math (nearest-rank) ---

    /// One [`percentile`] row.
    struct PercentileCase<'a> {
        desc: &'static str,
        sorted: &'a [u64],
        p: f64,
        expected: u64,
    }

    /// Pins [`percentile`] as NEAREST-RANK over an already-sorted slice:
    /// `idx = clamp(ceil(p * n) - 1, 0, n - 1)`, with `0` for an empty slice.
    ///
    /// Nearest-rank always answers a value that was actually observed, which is what
    /// makes the p95 column trustworthy as a frame-budget reading; an interpolating
    /// variant would report a frame time no frame ever took.
    ///
    /// Falsifying edits (from the plan): change `ceil` to `floor` — the `[10, 20]`
    /// p95 row and the `1..=100` p95 row fire; drop the `- 1` — the `[10, 20, 30]`
    /// p50 row and the `1..=100` rows fire.
    #[test]
    fn percentile_nearest_rank_table() {
        let one_to_hundred: Vec<u64> = (1..=100).collect();
        let cases = [
            PercentileCase {
                desc: "an empty slice p50 -> 0",
                sorted: &[],
                p: 0.5,
                expected: 0,
            },
            PercentileCase {
                desc: "an empty slice p95 -> 0",
                sorted: &[],
                p: 0.95,
                expected: 0,
            },
            PercentileCase {
                desc: "a single element p50 -> that element",
                sorted: &[7],
                p: 0.5,
                expected: 7,
            },
            PercentileCase {
                desc: "a single element p95 -> that element",
                sorted: &[7],
                p: 0.95,
                expected: 7,
            },
            PercentileCase {
                // ceil(0.5 * 2) - 1 == 0
                desc: "two elements p50 -> the lower",
                sorted: &[10, 20],
                p: 0.5,
                expected: 10,
            },
            PercentileCase {
                // ceil(0.95 * 2) - 1 == ceil(1.9) - 1 == 1
                desc: "two elements p95 -> the upper",
                sorted: &[10, 20],
                p: 0.95,
                expected: 20,
            },
            PercentileCase {
                // ceil(0.5 * 3) - 1 == ceil(1.5) - 1 == 1
                desc: "three elements p50 -> the middle",
                sorted: &[10, 20, 30],
                p: 0.5,
                expected: 20,
            },
            PercentileCase {
                // ceil(0.95 * 3) - 1 == ceil(2.85) - 1 == 2
                desc: "three elements p95 -> the largest",
                sorted: &[10, 20, 30],
                p: 0.95,
                expected: 30,
            },
            PercentileCase {
                desc: "an all-equal slice p50 -> that value",
                sorted: &[7, 7, 7, 7],
                p: 0.5,
                expected: 7,
            },
            PercentileCase {
                desc: "an all-equal slice p95 -> that value",
                sorted: &[7, 7, 7, 7],
                p: 0.95,
                expected: 7,
            },
            PercentileCase {
                // ceil(50) - 1 == 49, and the slice is 1-based in value
                desc: "1..=100 p50 -> 50",
                sorted: &one_to_hundred,
                p: 0.5,
                expected: 50,
            },
            PercentileCase {
                // ceil(95) - 1 == 94
                desc: "1..=100 p95 -> 95",
                sorted: &one_to_hundred,
                p: 0.95,
                expected: 95,
            },
            PercentileCase {
                // ceil(100) - 1 == 99
                desc: "1..=100 p100 -> the largest, never past the end",
                sorted: &one_to_hundred,
                p: 1.0,
                expected: 100,
            },
            PercentileCase {
                // ceil(0) - 1 == -1, which the lower clamp raises to 0
                desc: "p == 0 clamps up to the smallest sample, never to a negative index",
                sorted: &one_to_hundred,
                p: 0.0,
                expected: 1,
            },
            PercentileCase {
                // ceil(0.1) - 1 == 0
                desc: "a sub-rank p rounds UP to rank 1",
                sorted: &one_to_hundred,
                p: 0.001,
                expected: 1,
            },
        ];

        for case in &cases {
            assert_eq!(
                percentile(case.sorted, case.p),
                case.expected,
                "case: {} — percentile(len {}, p = {}) must be {}. A value one step LOW means \
                 `ceil` became `floor` or `round`; one step HIGH means the `- 1` is missing; a 0 \
                 on a non-empty slice means the lower clamp is swallowing a valid rank.",
                case.desc,
                case.sorted.len(),
                case.p,
                case.expected
            );
        }
    }

    /// Sweeps slice lengths 1..=50 for the structural ordering
    /// `min <= p50 <= p95 <= max`, then pins the UPPER CLAMP against the only kind
    /// of caller that can reach it.
    ///
    /// **Those are two separate claims, and the sweep is not evidence about the
    /// clamp.** `ceil(p * n) <= n` for every `p <= 1.0`, so a p50's or a p95's rank
    /// is never past the last element in the first place. Measured: with the
    /// `rank >= len` arm deleted, the sweep and every other case in this module stay
    /// green, and the only assertion that reddens is the one at the end of this test.
    /// What the sweep does prove is that the two ranks stay ordered and inside the
    /// slice at every length rather than at the four the table above picks — worth
    /// keeping, and worth not mistaking for coverage of the clamp.
    ///
    /// The clamp is reachable only from a `p` GREATER than 1.
    /// [`FrameStats::summarize`] passes no such value, but [`percentile`] is `pub`
    /// and takes an arbitrary `f64`, so a later caller — a p99.9 spelled `99.9`, say
    /// — can. That case is the second half of this test and is the clamp's only
    /// falsifier in the suite.
    ///
    /// Falsifying edit: delete the `rank >= len` arm — `percentile(&[1, 2, 3], 1.5)`
    /// then computes index `ceil(4.5) - 1 == 4` into a three-element slice and
    /// panics.
    #[test]
    fn percentile_p95_never_exceeds_max() {
        for n in 1_u64..=50 {
            let sorted: Vec<u64> = (1..=n).map(|v| v * 3).collect();
            let first = *sorted
                .first()
                .expect("the fixture is non-empty by construction");
            let last = *sorted
                .last()
                .expect("the fixture is non-empty by construction");
            let p50 = percentile(&sorted, 0.5);
            let p95 = percentile(&sorted, 0.95);

            assert!(
                p95 <= last,
                "n = {n}: p95 is {p95}, past the largest sample {last}. At a `p` of 0.95 the rank \
                 cannot exceed `n` on its own, so this is the rank formula changing SHAPE — a rank \
                 not derived from `ceil(p * n) - 1` — rather than the upper clamp going missing. \
                 The clamp has its own case at the end of this test."
            );
            assert!(
                p95 >= first,
                "n = {n}: p95 is {p95}, below the smallest sample {first}. The index ran off the \
                 FRONT, which means the lower clamp is being applied to a rank that was already \
                 valid."
            );
            assert!(
                p50 <= p95,
                "n = {n}: p50 ({p50}) exceeds p95 ({p95}). The two ranks are inverted, so the \
                 report's columns are swapped and every frame-budget reading taken from the p95 \
                 column is really a median."
            );
        }

        // The upper clamp's own case: `p = 1.5` ranks `ceil(4.5) - 1 == 4`, two past the end of a
        // three-element slice, the only input shape that reaches the clamp.
        let three = [1_u64, 2, 3];
        assert_eq!(
            percentile(&three, 1.5),
            3,
            "percentile(&[1, 2, 3], 1.5) must clamp to the largest sample, 3. Three outcomes are \
             distinguishable here: a PANIC means the `rank >= len` arm is gone and the rank \
             indexed past the slice, which in production is an out-of-bounds index on the report \
             path; a 1 means the clamp collapsed to the lower arm and every over-range percentile \
             answers the smallest sample; any other value means the clamp landed on the wrong \
             element. Nothing `summarize` passes today reaches this arm, so it is unreachable from \
             every other assertion in this module."
        );
    }

    // --- 4. Bucket isolation ---

    /// Recording one bucket leaves every other one out of the report entirely.
    ///
    /// The zero-count skip is what the cage smoke's non-vacuity assertion rests on:
    /// with no preview open, `prev.render` and `prev.load` must be ABSENT from the
    /// file, which is the only thing separating "the labels are wired" from "the
    /// formatter prints all fifteen labels whatever was recorded".
    ///
    /// Falsifying edit (from the plan): delete the `count == 0` skip in `summarize`.
    #[test]
    fn recording_one_bucket_leaves_others_empty() {
        let mut stats = FrameStats::new();
        stats.record(Bucket::DrawPane, 5 * NS_PER_MS);
        stats.record(Bucket::DrawPane, 7 * NS_PER_MS);

        let report = stats.summarize(1, 5.0);

        assert_eq!(
            report.buckets.len(),
            1,
            "exactly ONE bucket was recorded, so the report must carry exactly one row. Rows \
             present: {:?}. Every extra row is a zero-count bucket that `summarize` failed to \
             skip, which puts `prev.render` and `prev.load` into every report and disarms the cage \
             smoke's must-not-appear assertion.",
            report
                .buckets
                .iter()
                .map(|s| s.bucket.label())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            report.buckets[0].bucket,
            Bucket::DrawPane,
            "and the one row must be the bucket that was recorded"
        );
        assert!(
            report
                .buckets
                .iter()
                .all(|s| s.bucket != Bucket::PrevRender),
            "prev.render was never recorded, so it must not appear"
        );
    }

    /// Every bucket round-trips through `record` and `summarize` with no cross-bleed.
    ///
    /// One DISTINCT sample per bucket, so a mis-sized or mis-indexed accumulator
    /// array shows up as a missing row or as a value landing under the wrong label,
    /// rather than as a silently-equal duplicate. This is the tripwire for the
    /// storage ever ceasing to derive its size from `Bucket::ALL.len()`.
    ///
    /// Falsifying edit (from the plan): make two variants return the same `index()`.
    #[test]
    fn every_bucket_round_trips_through_record_and_summarize() {
        let mut stats = FrameStats::new();
        for bucket in Bucket::ALL {
            stats.record(bucket, distinct_sample(bucket));
        }

        let report = stats.summarize(1, 5.0);
        assert_eq!(
            report.buckets.len(),
            Bucket::ALL.len(),
            "one sample in every bucket must produce exactly one row per bucket. Fewer rows means \
             two buckets share a storage slot (so one bucket's samples are attributed to the \
             other); rows present: {:?}",
            report
                .buckets
                .iter()
                .map(|s| s.bucket.label())
                .collect::<Vec<_>>()
        );

        for bucket in Bucket::ALL {
            let want = distinct_sample(bucket);
            let summary = summary_for(&report, bucket);
            let label = bucket.label();

            assert_eq!(summary.count, 1, "{label}: exactly one sample was recorded");
            assert_eq!(
                summary.mean_nanos, want,
                "{label}: the mean must be its OWN sample. A value belonging to another bucket \
                 here is a cross-bleed: two variants resolving to one storage index."
            );
            assert_eq!(summary.p50_nanos, want, "{label}: p50 is its own sample");
            assert_eq!(summary.p95_nanos, want, "{label}: p95 is its own sample");
            assert_eq!(summary.max_nanos, want, "{label}: max is its own sample");
            assert!(
                !summary.truncated,
                "{label}: one sample is nowhere near SAMPLE_CAPACITY"
            );
            assert_eq!(
                stats.retained_sample_count(bucket),
                1,
                "{label}: the percentile ring retains its one sample"
            );
        }
    }

    // --- 5. The ring bound and truncation ---

    /// The percentile window is bounded to the LAST [`SAMPLE_CAPACITY`] samples,
    /// while `count`, the sum and `max` stay exact and unbounded.
    ///
    /// The fixture is deliberately DISTINGUISHABLE: 100 one-nanosecond samples
    /// first, then exactly `SAMPLE_CAPACITY` large ones. A window holding the FIRST
    /// capacity samples, or all of them, would drag p50 down toward the tiny prefix,
    /// so a wrong window is detectable rather than merely unproven.
    ///
    /// Integer pushes only — no sleeps and no clock — so the whole 4196-sample run
    /// costs microseconds.
    ///
    /// The expected ranks are LITERALS derived by hand at the pinned capacity and
    /// the capacity is re-typed locally, because a rank recomputed from
    /// `SAMPLE_CAPACITY` with the same formula the implementation uses would hold
    /// for whatever formula that was.
    ///
    /// Falsifying edit (from the plan): keep the FIRST capacity samples instead of
    /// the last.
    #[test]
    fn ring_bound_keeps_exact_count_and_windowed_percentiles() {
        assert_eq!(
            SAMPLE_CAPACITY, EXPECT_SAMPLE_CAPACITY,
            "SAMPLE_CAPACITY is {SAMPLE_CAPACITY}, but the expected percentile ranks below are \
             literals derived by hand at {EXPECT_SAMPLE_CAPACITY}. Re-derive them before changing \
             the const: p50 index = ceil(0.5 * n) - 1, p95 index = ceil(0.95 * n) - 1."
        );

        // The number of tiny samples pushed before the ring saturates.
        const OVERFLOW: u64 = 100;
        // The first large sample; the run is `BASE .. BASE + SAMPLE_CAPACITY`.
        const BASE: u64 = 1000;
        // ceil(0.5 * 4096) - 1 == 2048 - 1 == 2047
        const P50_RANK: u64 = 2047;
        // ceil(0.95 * 4096) - 1 == ceil(3891.2) - 1 == 3892 - 1 == 3891
        const P95_RANK: u64 = 3891;
        // 100 × 1 ns + sum(1000..5096) == 100 + 12_482_560 == 12_482_660 over 4196 samples;
        // 4196 * 2974 == 12_478_904, remainder 3756 < 4196, so the truncating mean is 2974.
        const EXPECT_MEAN: u64 = 2974;

        let capacity = u64::try_from(SAMPLE_CAPACITY).expect("SAMPLE_CAPACITY fits in a u64");

        let mut stats = FrameStats::new();
        for _ in 0..OVERFLOW {
            stats.record(Bucket::WakeTabs, 1);
        }
        for i in 0..capacity {
            stats.record(Bucket::WakeTabs, BASE + i);
        }

        let report = stats.summarize(1, 5.0);
        let wake = summary_for(&report, Bucket::WakeTabs);

        assert_eq!(
            wake.count,
            OVERFLOW + capacity,
            "count must be exact and unbounded by the ring. A count of {capacity} means the ring's \
             size is being reported instead, at which point every `count * mean` ranking in the \
             measurement is wrong by the overflow."
        );
        assert_eq!(
            wake.max_nanos,
            BASE + capacity - 1,
            "max must be tracked OUTSIDE the ring and so stay exact. The largest sample is the \
             last one pushed, so a smaller value here means max is recomputed from the window."
        );
        assert!(
            wake.truncated,
            "count ({}) exceeds SAMPLE_CAPACITY ({SAMPLE_CAPACITY}), so the row must be marked \
             truncated — that marker is the reader's only signal that the percentiles are \
             windowed while the count and max are not.",
            wake.count
        );

        assert_eq!(
            wake.p50_nanos,
            BASE + P50_RANK,
            "p50 must be computed over ONLY the last {SAMPLE_CAPACITY} samples. A value near 1 \
             means the 100-nanosecond prefix is still inside the window (the ring keeps the OLDEST \
             samples rather than the newest, or does not wrap at all)."
        );
        assert_eq!(
            wake.p95_nanos,
            BASE + P95_RANK,
            "p95 must be computed over ONLY the last {SAMPLE_CAPACITY} samples"
        );
        assert!(
            wake.p50_nanos > 1,
            "a p50 of 1 would mean the tiny prefix is still in the percentile window, which is the \
             exact failure the distinguishable fixture exists to expose"
        );

        assert_eq!(
            wake.mean_nanos, EXPECT_MEAN,
            "the mean must use the EXACT unbounded sum over the exact count, not the window's. A \
             mean near {BASE} means the sum was recomputed from the retained samples and the \
             tiny prefix was dropped from it."
        );

        assert_eq!(
            stats.retained_sample_count(Bucket::WakeTabs),
            SAMPLE_CAPACITY,
            "once saturated the ring retains exactly SAMPLE_CAPACITY samples. A larger number \
             means the bound is not applied at all, and a session-long run then grows without \
             limit."
        );
    }

    /// The truncation marker is `count > SAMPLE_CAPACITY`, strictly greater.
    ///
    /// At exactly capacity every sample is still inside the window, so the
    /// percentiles are exact and the row must NOT claim otherwise; one sample later
    /// it must. Both arms are asserted, because a `>=` implementation passes the
    /// first-arm-only version of this test in the wrong direction.
    ///
    /// Falsifying edit (from the plan): change `>` to `>=`.
    #[test]
    fn exactly_capacity_samples_is_not_truncated() {
        let capacity = u64::try_from(SAMPLE_CAPACITY).expect("SAMPLE_CAPACITY fits in a u64");

        let mut exact = FrameStats::new();
        for i in 0..capacity {
            exact.record(Bucket::DrawPane, i + 1);
        }
        let exact_report = exact.summarize(1, 5.0);
        let exact_row = summary_for(&exact_report, Bucket::DrawPane);
        assert_eq!(
            exact_row.count, capacity,
            "the fixture recorded exactly SAMPLE_CAPACITY samples"
        );
        assert!(
            !exact_row.truncated,
            "count == SAMPLE_CAPACITY is NOT truncated: the window holds every sample, so the \
             percentiles are exact and the `*` marker would tell the reader the opposite."
        );
        assert_eq!(
            exact.retained_sample_count(Bucket::DrawPane),
            SAMPLE_CAPACITY,
            "the ring is exactly full, and has not wrapped"
        );

        let mut over = FrameStats::new();
        for i in 0..=capacity {
            over.record(Bucket::DrawPane, i + 1);
        }
        let over_report = over.summarize(1, 5.0);
        let over_row = summary_for(&over_report, Bucket::DrawPane);
        assert_eq!(
            over_row.count,
            capacity + 1,
            "the fixture recorded one sample past SAMPLE_CAPACITY"
        );
        assert!(
            over_row.truncated,
            "one sample past SAMPLE_CAPACITY IS truncated: the oldest sample has left the window, \
             so the percentiles no longer cover the whole interval and the reader must be told."
        );
    }

    /// Below capacity the retained count is the sample count, and an untouched
    /// bucket retains nothing.
    ///
    /// The second half is what makes the first non-vacuous: a `retained_sample_count`
    /// that answered `SAMPLE_CAPACITY` unconditionally (returning the reserved
    /// capacity rather than the live length) would satisfy neither.
    #[test]
    fn retained_sample_count_below_capacity() {
        let mut stats = FrameStats::new();
        for i in 0..10_u64 {
            stats.record(Bucket::PanePrev, i);
        }

        assert_eq!(
            stats.retained_sample_count(Bucket::PanePrev),
            10,
            "below capacity the retained count is the number of samples recorded. A \
             SAMPLE_CAPACITY here means the reserved capacity is being reported instead of the \
             live length, which would also make the percentiles run over uninitialized slots."
        );
        assert_eq!(
            stats.retained_sample_count(Bucket::DrawGrid),
            0,
            "a bucket nothing ever recorded into retains nothing"
        );
    }

    // --- 6. The wakes/keystroke ratio ---

    /// One [`Report::wakes_per_keystroke`] row.
    struct RatioCase {
        desc: &'static str,
        keystrokes: u64,
        wakes: u64,
        expected: Option<f64>,
    }

    /// Pins the ratio as `wakes / keystrokes`, and `None` when no keystroke was
    /// recorded.
    ///
    /// The `None` arm is the divide-by-zero guard, and it is the reason the field is
    /// an `Option` rather than an `f64`: a report covering an interval with wakes and
    /// no typing would otherwise carry `inf`, and the footer would print it.
    ///
    /// The ratio itself is an UPPER BOUND on wakes attributable to typing — mouse,
    /// scroll, resize and out-of-band core notifications all inflate it — which is
    /// why nothing here or in the cage smoke asserts on its VALUE against a live run.
    ///
    /// Falsifying edit (from the plan): relax the `keystrokes > 0` guard.
    #[test]
    fn wakes_per_keystroke_table() {
        let cases = [
            RatioCase {
                desc: "no keystrokes and no wakes -> None",
                keystrokes: 0,
                wakes: 0,
                expected: None,
            },
            RatioCase {
                desc: "wakes with NO keystrokes -> still None, never a division",
                keystrokes: 0,
                wakes: 7,
                expected: None,
            },
            RatioCase {
                desc: "30 wakes over 10 keys -> 3.0",
                keystrokes: 10,
                wakes: 30,
                expected: Some(3.0),
            },
            RatioCase {
                desc: "0 wakes over 5 keys -> 0.0, which is a value and not an absence",
                keystrokes: 5,
                wakes: 0,
                expected: Some(0.0),
            },
            RatioCase {
                desc: "the macOS sample ratio, so the two reports read alike",
                keystrokes: 48,
                wakes: 142,
                expected: Some(142.0 / 48.0),
            },
            RatioCase {
                desc: "fewer wakes than keys -> a sub-1 ratio",
                keystrokes: 4,
                wakes: 2,
                expected: Some(0.5),
            },
        ];

        for case in &cases {
            let mut stats = FrameStats::new();
            for _ in 0..case.keystrokes {
                stats.record_keystroke();
            }
            for _ in 0..case.wakes {
                stats.record_wake();
            }

            let report = stats.summarize(1, 5.0);
            assert_eq!(
                report.keystrokes, case.keystrokes,
                "case: {}: the keystroke counter must carry through to the report",
                case.desc
            );
            assert_eq!(
                report.wakes, case.wakes,
                "case: {}: the wake counter must carry through to the report",
                case.desc
            );

            match (report.wakes_per_keystroke, case.expected) {
                (Some(got), Some(want)) => assert!(
                    (got - want).abs() < RATIO_EPS,
                    "case: {} — wakes/key is {got}, want {want}. An inverted ratio \
                     (keystrokes/wakes) is the likeliest cause and reads plausibly in the footer.",
                    case.desc
                ),
                (None, None) => {}
                (got, want) => panic!(
                    "case: {} — wakes/key is {got:?}, want {want:?}. A `Some` where `None` belongs \
                     means the `keystrokes > 0` guard is gone, and the footer then prints `inf` \
                     or `NaN` for every interval in which the user did not type.",
                    case.desc
                ),
            }
        }
    }

    // --- 7. reset() and the emit gate ---

    /// `reset` zeroes every bucket and both counters, after every mutator.
    ///
    /// Each report covers only its own interval, so a reset that left anything
    /// behind would make every report after the first cumulative — and the
    /// measurement's "before and after" comparison would be between one interval and
    /// a growing prefix of the whole session.
    ///
    /// Falsifying edit (from the plan): drop `samples.clear()`.
    #[test]
    fn reset_zeroes_everything() {
        let mut stats = FrameStats::new();
        stats.record(Bucket::DrawPane, 9 * NS_PER_MS);
        stats.record(Bucket::Wake, 3 * NS_PER_MS);
        stats.record_keystroke();
        stats.record_wake();
        stats.record_wake();

        stats.reset();

        let report = stats.summarize(2, 5.0);
        assert!(
            report.buckets.is_empty(),
            "reset() must leave NO non-empty bucket. Rows still present: {:?}",
            report
                .buckets
                .iter()
                .map(|s| s.bucket.label())
                .collect::<Vec<_>>()
        );
        assert_eq!(report.keystrokes, 0, "reset() zeroes the keystroke counter");
        assert_eq!(report.wakes, 0, "reset() zeroes the wake counter");
        assert_eq!(
            report.wakes_per_keystroke, None,
            "with the keystroke counter zeroed the ratio is None again"
        );
        assert_eq!(
            stats.retained_sample_count(Bucket::DrawPane),
            0,
            "reset() clears the percentile ring's live slice, not just the counters"
        );
        assert_eq!(
            stats.retained_sample_count(Bucket::Wake),
            0,
            "reset() clears EVERY bucket's ring, not only the first"
        );
    }

    /// A reset starts a genuinely fresh window: nothing recorded before it can reach
    /// the percentiles, the max or the mean after it.
    ///
    /// The pre-reset samples are a thousand times larger than the post-reset ones, so
    /// a stale write index — a ring that resumed mid-buffer and left old entries live
    /// — shows up as a max or a p95 far outside the small range rather than as a
    /// value that merely looks a little off.
    ///
    /// Falsifying edit (from the plan): drop the write-index reset.
    #[test]
    fn reset_starts_a_fresh_window_with_no_leakage() {
        let mut stats = FrameStats::new();
        for _ in 0..50 {
            stats.record(Bucket::DrawPane, 1_000_000_000);
        }
        stats.reset();
        for nanos in [10_u64, 20, 31] {
            stats.record(Bucket::DrawPane, nanos);
        }

        let report = stats.summarize(2, 5.0);
        let draw = summary_for(&report, Bucket::DrawPane);

        assert_eq!(
            draw.count, 3,
            "only the three post-reset samples may be counted"
        );
        assert_eq!(
            draw.mean_nanos, 20,
            "61 / 3 == 20, unaffected by the pre-reset sum. A huge mean here means the running sum \
             survived the reset."
        );
        assert_eq!(
            draw.max_nanos, 31,
            "max is re-baselined. A 1_000_000_000 here means the pre-reset maximum survived, and \
             every later report would keep reporting a one-second frame that never happened again."
        );
        assert_eq!(
            draw.p50_nanos, 20,
            "p50 over [10, 20, 31] is 20. A 1_000_000_000 means a pre-reset sample is still in the \
             percentile window, which is the stale-write-index failure."
        );
        assert_eq!(draw.p95_nanos, 31, "p95 over [10, 20, 31] is 31");
        assert!(
            !draw.truncated,
            "three samples is far below SAMPLE_CAPACITY, so a truncated marker here means the \
             count survived the reset"
        );
        assert_eq!(
            stats.retained_sample_count(Bucket::DrawPane),
            3,
            "the ring holds exactly the three post-reset samples"
        );
    }

    /// `is_empty` tracks all three mutators, and is the emit gate's whole contract.
    ///
    /// `frame_metrics::maybe_emit` re-baselines and returns WITHOUT summarizing when
    /// this answers true, so a version that ignored one mutator would either suppress
    /// a report that carries real data or emit an all-zero one every interval for the
    /// whole time the application sits idle.
    ///
    /// The zero-nanosecond row states one reading explicitly: a sample of zero is
    /// still a SAMPLE, so `is_empty` is count-based rather than sum-based.
    ///
    /// Falsifying edit (from the plan): make `is_empty` ignore the keystroke counter.
    #[test]
    fn is_empty_tracks_all_three_mutators() {
        let fresh = FrameStats::new();
        assert!(
            fresh.is_empty(),
            "a freshly constructed FrameStats has no sample and both counters at zero, so it is \
             empty. A false here means every idle interval of a running application emits an \
             all-zero report."
        );

        let mut stats = FrameStats::new();

        stats.record(Bucket::DrawPane, 8 * NS_PER_MS);
        assert!(
            !stats.is_empty(),
            "after `record` the accumulator holds a timing sample, so it is not empty — a true \
             here suppresses the very report that carries the frame times"
        );
        stats.reset();
        assert!(stats.is_empty(), "reset() restores the empty state");

        stats.record_keystroke();
        assert!(
            !stats.is_empty(),
            "after `record_keystroke` the accumulator holds a counter the footer prints, so it is \
             not empty. This is the mutator the plan names as the likeliest to be forgotten, and \
             forgetting it drops exactly the intervals in which the user typed but nothing was \
             timed."
        );
        stats.reset();
        assert!(stats.is_empty(), "reset() restores the empty state");

        stats.record_wake();
        assert!(
            !stats.is_empty(),
            "after `record_wake` the accumulator holds a counter the footer prints, so it is not \
             empty"
        );
        stats.reset();
        assert!(stats.is_empty(), "reset() restores the empty state");

        stats.record(Bucket::DrawRead, 0);
        assert!(
            !stats.is_empty(),
            "a ZERO-nanosecond sample is still a sample: `is_empty` is about whether anything was \
             RECORDED, not about whether the recorded time was non-zero. A sum-based \
             implementation answers true here and drops a whole interval's counts."
        );
    }

    // --- 8. Format stability and diffability ---

    /// The byte-exact report table, from a [`Report`] whose buckets are supplied OUT
    /// OF ORDER.
    ///
    /// THIS is the test that makes two runs — and the two front-ends — `diff`-able.
    /// The expected string is the CONTRACT, and it is derived from
    /// `WolfKeyMap/FrameStats.swift`'s `format`/`pad`/`millis` and its
    /// `FrameStatsTests.testFormatProducesTheExactPinnedTable`, using the same
    /// counts, nanosecond values, index, window and counters, so the two tables
    /// differ only in their labels:
    ///
    /// - prefix `"wolf-frame-stats "` on every line, including the last;
    /// - label left-aligned in 14; count right-aligned in 5, with a `*` suffix
    ///   INSIDE the field when truncated;
    /// - mean / p50 / p95 / max right-aligned in 9 each, milliseconds, two decimals;
    /// - header `#<index>  window=<elapsed, one decimal>s` — TWO spaces;
    /// - footer `keys=<k> wakes=<w> wakes/key=<ratio, two decimals>`;
    /// - rows in ascending `index()` order regardless of `Report.buckets` order.
    ///
    /// The three buckets are supplied as `draw.pane` (10), `wake` (0),
    /// `prev.render` (8) so that the emitted order 0, 8, 10 can only come from a
    /// sort. Their labels are 4, 9 and 11 characters and their counts 3, 2 and 3
    /// digits, so no single width can satisfy all three by coincidence.
    ///
    /// Falsifying edits (from the plan): change any width by one, drop the `*`, or
    /// drop the sort.
    #[test]
    fn format_produces_the_exact_pinned_table() {
        assert_eq!(
            LINE_PREFIX, EXPECT_LINE_PREFIX,
            "LINE_PREFIX is {LINE_PREFIX:?}, want {EXPECT_LINE_PREFIX:?}. Every line of every \
             report carries it, the cage smoke greps for it, and the macOS front-end emits the \
             same string so the two tables can be diffed — changing it silently desynchronizes all \
             three. The expected value is re-typed here rather than read from the const so this \
             check is not a comparison with itself."
        );

        let report = Report {
            index: 3,
            elapsed_secs: 5.0,
            buckets: vec![
                BucketSummary {
                    bucket: Bucket::DrawPane,
                    count: 138,
                    mean_nanos: 4_020_000,
                    p50_nanos: 3_600_000,
                    p95_nanos: 11_400_000,
                    max_nanos: 22_900_000,
                    truncated: false,
                },
                BucketSummary {
                    bucket: Bucket::Wake,
                    count: 142,
                    mean_nanos: 3_110_000,
                    p50_nanos: 2_400_000,
                    p95_nanos: 9_800_000,
                    max_nanos: 41_200_000,
                    truncated: true,
                },
                BucketSummary {
                    bucket: Bucket::PrevRender,
                    count: 15,
                    mean_nanos: 24_900_000,
                    p50_nanos: 22_000_000,
                    p95_nanos: 51_800_000,
                    max_nanos: 60_100_000,
                    truncated: false,
                },
            ],
            keystrokes: 48,
            wakes: 142,
            wakes_per_keystroke: Some(142.0 / 48.0),
        };

        let expected = joined(&[
            "wolf-frame-stats #3  window=5.0s",
            "wolf-frame-stats bucket        count     mean      p50      p95      max",
            "wolf-frame-stats wake           142*     3.11     2.40     9.80    41.20",
            "wolf-frame-stats prev.render      15    24.90    22.00    51.80    60.10",
            "wolf-frame-stats draw.pane       138     4.02     3.60    11.40    22.90",
            "wolf-frame-stats keys=48 wakes=142 wakes/key=2.96",
        ]);

        assert_eq!(
            format_report(&report),
            expected,
            "the report table must be byte-exact. Read the diff this way: rows in the INPUT order \
             (draw.pane, wake, prev.render) means the sort by index() is missing, so two runs no \
             longer line up under `diff`; a missing `*` after 142 means the truncation marker is \
             gone and a reader cannot tell a windowed percentile from an exact one; a column \
             shifted by one space means a field width moved, which breaks alignment against the \
             macOS table this layout is copied from."
        );
    }

    /// A value too wide for its column OVERFLOWS the field; it is never truncated.
    ///
    /// This is the wide case, and it is a contract rather than an accident of
    /// `format!`. The macOS `pad(left:)` / `pad(right:)` helpers in
    /// `WolfKeyMap/FrameStats.swift` return the text unchanged when the field is too
    /// narrow — `guard deficit > 0 else { return text }` — so the two front-ends
    /// disagree the moment either starts clipping, and a `diff` between their tables
    /// stops being readable exactly where the numbers are most interesting. Losing
    /// digits is also the worse failure on its own terms: a 123456-sample count
    /// clipped to `12345` and a 1234567.89 ms outlier clipped to `1234567.8` both
    /// read as plausible values, so nothing downstream can tell they were cut.
    ///
    /// The fixture puts one row past every column and one row inside every column,
    /// so a truncating implementation cannot pass by leaving narrow values alone:
    ///
    /// - `123456` in the width-5 count field — one character over;
    /// - `999.99` ms, comfortably inside the width-9 value field;
    /// - `123456.78` ms, EXACTLY nine characters, which fills the field and butts
    ///   against its neighbour with no separating space (that is the layout working,
    ///   not a defect);
    /// - `1234567.89` ms, one character over.
    ///
    /// The rows are supplied in ascending index order, so a failure here is about
    /// field widths alone; the sort is
    /// [`formatted_bucket_rows_are_in_index_order`]'s job and the `*` marker's
    /// placement inside the count field is
    /// [`format_produces_the_exact_pinned_table`]'s.
    #[test]
    fn format_report_overflows_a_field_rather_than_truncating_a_wide_value() {
        let report = Report {
            index: 9,
            elapsed_secs: 5.0,
            buckets: vec![
                BucketSummary {
                    bucket: Bucket::Wake,
                    count: 123_456,
                    mean_nanos: 12_345_670_000,
                    p50_nanos: 999_990_000,
                    p95_nanos: 123_456_780_000,
                    max_nanos: 1_234_567_890_000,
                    truncated: false,
                },
                BucketSummary {
                    bucket: Bucket::DrawCmdline,
                    count: 7,
                    mean_nanos: 1_500_000,
                    p50_nanos: 1_000_000,
                    p95_nanos: 2_250_000,
                    max_nanos: 3_000_000,
                    truncated: false,
                },
            ],
            keystrokes: 3,
            wakes: 123_456,
            wakes_per_keystroke: Some(41152.0),
        };

        let expected = joined(&[
            "wolf-frame-stats #9  window=5.0s",
            "wolf-frame-stats bucket        count     mean      p50      p95      max",
            "wolf-frame-stats wake          123456 12345.67   999.99123456.781234567.89",
            "wolf-frame-stats draw.cmdline      7     1.50     1.00     2.25     3.00",
            "wolf-frame-stats keys=3 wakes=123456 wakes/key=41152.00",
        ]);

        assert_eq!(
            format_report(&report),
            expected,
            "a value wider than its column must push the row out, not lose characters. A `12345` \
             count, a `123456.7` p95 or a `1234567.8` max in the diff means the formatter clips to \
             the field width, which silently reports a smaller number than was measured and \
             diverges from the macOS `pad` helpers this layout is copied from. The narrow \
             draw.cmdline row is the control: if it moved too, a field WIDTH changed rather than \
             the overflow behaviour."
        );
    }

    /// A counters-only report still emits its header, its column row and its footer,
    /// and a `None` ratio renders as `n/a`.
    ///
    /// The `n/a` matters because the alternative spellings are all lies: `0.00` says
    /// the user typed and nothing woke, `NaN` and `inf` say the guard is missing.
    ///
    /// Falsifying edit (from the plan): print `0.00` for the `None` ratio.
    #[test]
    fn format_emits_header_and_footer_with_no_buckets() {
        let report = Report {
            index: 1,
            elapsed_secs: 2.5,
            buckets: Vec::new(),
            keystrokes: 0,
            wakes: 4,
            wakes_per_keystroke: None,
        };

        let expected = joined(&[
            "wolf-frame-stats #1  window=2.5s",
            "wolf-frame-stats bucket        count     mean      p50      p95      max",
            "wolf-frame-stats keys=0 wakes=4 wakes/key=n/a",
        ]);

        assert_eq!(
            format_report(&report),
            expected,
            "a bucket-less report must still be byte-exact. A missing column row means the header \
             is emitted conditionally on there being rows, which breaks the `diff` alignment \
             between an idle interval and a busy one; a `0.00` or a `NaN` in place of `n/a` \
             reports a ratio for an interval in which no keystroke was recorded."
        );
    }

    /// Every emitted line carries the grep prefix, the text ends in a newline, and
    /// the line count is exactly `buckets + 3`.
    ///
    /// The prefix is what makes a report recoverable from an interleaved stderr, so a
    /// footer or a header that lost it is invisible to `grep wolf-frame-stats` and
    /// invisible to the cage smoke's parser. The `+ 3` is header, column row, footer.
    ///
    /// Falsifying edit (from the plan): drop the prefix from the footer.
    #[test]
    fn every_formatted_line_carries_the_grep_prefix() {
        let mut stats = FrameStats::new();
        for bucket in Bucket::ALL {
            stats.record(bucket, distinct_sample(bucket));
        }
        stats.record_keystroke();
        stats.record_wake();

        let text = format_report(&stats.summarize(7, 5.0));

        assert!(
            text.ends_with('\n'),
            "the report must end with a newline, or the next report appended to the same file \
             begins on the same line as this one's footer and neither is greppable. Text \
             was:\n{text}"
        );

        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines.len(),
            Bucket::ALL.len() + 3,
            "a report over all {} buckets is header + column row + one row per bucket + footer. \
             Text was:\n{text}",
            Bucket::ALL.len()
        );
        for (n, line) in lines.iter().enumerate() {
            assert!(
                line.starts_with(EXPECT_LINE_PREFIX),
                "line {} of the report does not start with {EXPECT_LINE_PREFIX:?}: {line:?}. Every \
                 line is prefixed so a report survives being interleaved with anything else on \
                 stderr; an unprefixed footer is exactly the case a `grep` would drop silently.",
                n + 1
            );
        }
    }

    /// Rows are emitted in ascending `index()` order however they were recorded.
    ///
    /// Deterministic ordering is what makes two runs align under `diff`, which is the
    /// whole comparison the measurement rests on. Recording happens in REVERSE bucket
    /// order so an implementation that echoed insertion order would emit the exact
    /// inverse.
    ///
    /// The assertion compares the whole label SEQUENCE rather than scanning forward
    /// for each label in turn: a forward scan is satisfied by a duplicated label
    /// re-matching an earlier row, and every `wake.*` label carries `wake` as a
    /// prefix, so a substring scan is the weaker check here.
    ///
    /// Falsifying edit (from the plan): remove the sort in `format_report`.
    #[test]
    fn formatted_bucket_rows_are_in_index_order() {
        let mut stats = FrameStats::new();
        for bucket in Bucket::ALL.iter().rev() {
            stats.record(*bucket, NS_PER_MS);
        }

        let text = format_report(&stats.summarize(1, 5.0));
        let got = row_labels(&text);
        let want: Vec<&str> = Bucket::ALL.iter().map(|b| b.label()).collect();

        assert_eq!(
            got, want,
            "the bucket rows must be in ascending index() order. The samples were recorded in \
             REVERSE order, so a `got` that is the reverse of `want` means the formatter echoes \
             insertion order and two runs no longer line up under `diff`. Text was:\n{text}"
        );
    }
}
