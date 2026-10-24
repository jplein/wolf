//! The process-global glue behind [`crate::frame_stats`]: the clock, the
//! accumulator, and the writes.
//!
//! Everything decidable from values lives in the seam; this file owns the three
//! things it deliberately does not — the once-only environment read, the
//! monotonic clock, and the accumulator the whole process shares. `std` only, and
//! no GTK.
//!
//! # Off by default, and one boolean test when it is
//!
//! [`enabled`] reads a `OnceLock<bool>` stamped by [`install`]: one acquire load
//! and a branch. Every hot-path function tests it first and returns, and the
//! accumulator slot stays `None`, so a process without `WOLF_FRAME_STATS` set
//! allocates nothing and records nothing.
//!
//! # No timer, and what that costs
//!
//! Nothing here polls. [`maybe_emit`] rides the render wake — it is the last
//! statement of `window::build`'s drain-loop body, AFTER the `wake` region closes
//! so the emit's own cost is never inside a measured region — and [`flush_final`]
//! runs once after `adw::Application::run_with_args` returns. Two consequences,
//! stated rather than hidden: a report covering the final interval before the
//! application goes idle waits for the next wake, and a process killed by a
//! signal never runs the exit flush at all. That is why `WOLF_FRAME_STATS_FILE`
//! is appended to PER REPORT rather than buffered to exit.
//!
//! With N windows there are N drain loops sharing one accumulator; whichever
//! window wakes first after the interval elapses emits the process-wide report.
//!
//! # The `thread_local!` hazard
//!
//! The accumulator lives in a `thread_local!`, so nothing here needs a lock and
//! nothing needs to be `Send`. **A `record` from a non-main thread accumulates
//! into its own slot that nothing ever reports** — silent data loss rather than a
//! crash. No call site can reach a non-main thread today; the tripwire is a
//! `debug_assert!` against the thread id [`install`] stamped.
//!
//! # Two rules that look like style and are not
//!
//! Bind every [`Scope`] to a NAMED guard — `let _guard = frame_metrics::scope(..)`
//! — never `let _ = frame_metrics::scope(..)`, which drops the guard instantly and
//! measures a region of zero length.
//!
//! Never `eprintln!` a report: it PANICS if the write to stderr fails. Every write
//! here goes through `write_all` with the `Result` discarded.

use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::OnceLock;
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use crate::frame_stats::{self, Bucket, FrameStats};

/// The flag that arms the whole instrument.
const ENABLED_ENV: &str = "WOLF_FRAME_STATS";

/// The report interval, in seconds.
const INTERVAL_ENV: &str = "WOLF_FRAME_STATS_INTERVAL";

/// An optional file every report is APPENDED to, alongside stderr.
const FILE_ENV: &str = "WOLF_FRAME_STATS_FILE";

/// Whether the instrument is armed, read exactly once by [`install`].
static ENABLED: OnceLock<bool> = OnceLock::new();

/// The thread [`install`] ran on, and so the only thread whose accumulator is
/// ever reported. Stamped only when the instrument is armed.
static REPORTER_THREAD: OnceLock<ThreadId> = OnceLock::new();

thread_local! {
    /// The accumulator, `None` unless [`install`] found the flag set on this
    /// thread.
    static RECORDER: RefCell<Option<Recorder>> = const { RefCell::new(None) };
}

/// The accumulator plus everything the emit needs: the interval, its baseline,
/// the report counter and the optional file handle.
struct Recorder {
    /// The interval's samples and counters.
    stats: FrameStats,
    /// How long a report interval is.
    interval: Duration,
    /// When the current interval began.
    interval_start: Instant,
    /// The number of the last report emitted; the next one is this plus one.
    report_index: u64,
    /// `WOLF_FRAME_STATS_FILE`'s handle, or `None` when the variable is unset or
    /// the path could not be opened.
    file: Option<File>,
}

/// Reads the three environment variables ONCE and arms the instrument.
///
/// The first statement of [`crate::app::run`], because [`enabled`] is what every
/// other function here branches on. Returns immediately when the flag is not set,
/// leaving the accumulator slot `None`.
///
/// A `WOLF_FRAME_STATS_FILE` that cannot be opened degrades silently to
/// stderr-only: an unwritable path is not a reason to refuse to run.
pub fn install() {
    // Forced here and answered by `enabled()`, the single authority on whether the instrument is
    // armed, this function included, which would otherwise write a header no other call honours.
    let _ =
        ENABLED.get_or_init(|| frame_stats::is_enabled(std::env::var(ENABLED_ENV).ok().as_deref()));
    if !enabled() {
        return;
    }
    let _ = REPORTER_THREAD.set(std::thread::current().id());

    let interval = frame_stats::report_interval(std::env::var(INTERVAL_ENV).ok().as_deref());
    // The seam decides which values name a file and the path, including the trim, which must apply
    // to the value that is opened, not only the one tested for emptiness.
    let raw_file = std::env::var(FILE_ENV).ok();
    let file = frame_stats::report_file_path(raw_file.as_deref())
        .and_then(|path| OpenOptions::new().create(true).append(true).open(path).ok());

    RECORDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_some() {
            return;
        }
        *slot = Some(Recorder {
            stats: FrameStats::new(),
            interval,
            interval_start: Instant::now(),
            report_index: 0,
            file,
        });
    });

    // After the borrow above is gone: `write` takes its own.
    write(&format!(
        "{}enabled interval={:.1}s\n",
        frame_stats::LINE_PREFIX,
        interval.as_secs_f64()
    ));
}

/// Whether the instrument is armed. One acquire load and a branch.
#[inline]
#[must_use]
pub fn enabled() -> bool {
    ENABLED.get().copied().unwrap_or(false)
}

/// Opens a timing region, or `None` when the instrument is disabled.
///
/// Pair with [`end`]; the `None` is what makes a disabled call site one boolean
/// test and no clock read.
#[inline]
#[must_use]
pub fn begin() -> Option<Instant> {
    if enabled() {
        Some(Instant::now())
    } else {
        None
    }
}

/// Closes a timing region opened by [`begin`] and records it against `bucket`.
///
/// A `None` start is a disabled instrument and records nothing.
#[inline]
pub fn end(bucket: Bucket, start: Option<Instant>) {
    let Some(start) = start else {
        return;
    };
    let elapsed = Instant::now().saturating_duration_since(start);
    // `as` would silently wrap a duration past 584 years; the saturation is the
    // honest answer and cannot happen.
    let nanos = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
    with_stats(|stats| stats.record(bucket, nanos));
}

/// Opens a timing region that closes when the returned guard drops.
///
/// **Bind the result to a named guard.** `let _ = scope(..)` drops it immediately
/// and measures nothing.
#[inline]
#[must_use]
pub fn scope(bucket: Bucket) -> Scope {
    Scope {
        bucket,
        start: begin(),
    }
}

/// A timing region that closes on drop, for a statement list or a body with early
/// returns.
pub struct Scope {
    /// The bucket the region is recorded against.
    bucket: Bucket,
    /// When the region opened, or `None` when the instrument is disabled.
    start: Option<Instant>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        end(self.bucket, self.start.take());
    }
}

/// Counts one key actually delivered to Neovim.
#[inline]
pub fn note_keystroke() {
    with_stats(FrameStats::record_keystroke);
}

/// Counts one core wake.
#[inline]
pub fn note_wake() {
    with_stats(FrameStats::record_wake);
}

/// Emits a report if the interval has elapsed, and otherwise does nothing.
///
/// The last statement of the drain-loop body, outside every measured region. An
/// interval that elapsed with nothing recorded is re-baselined and NOT summarized,
/// so an idle application emits no all-zero reports.
pub fn maybe_emit() {
    if !enabled() {
        return;
    }
    emit_report(false);
}

/// [`maybe_emit`] with the interval check skipped, for the one call after
/// `adw::Application::run_with_args` returns.
///
/// Still silent when nothing was recorded, and never reached at all when the
/// process is killed by a signal.
pub fn flush_final() {
    if !enabled() {
        return;
    }
    emit_report(true);
}

/// The body of [`maybe_emit`] and [`flush_final`], differing only in whether the
/// interval gate applies.
fn emit_report(force: bool) {
    let now = Instant::now();
    let report = RECORDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let recorder = slot.as_mut()?;
        let elapsed = now.saturating_duration_since(recorder.interval_start);
        if !force && !frame_stats::interval_elapsed(elapsed, recorder.interval) {
            return None;
        }
        if recorder.stats.is_empty() {
            recorder.interval_start = now;
            return None;
        }
        recorder.report_index += 1;
        let report = recorder
            .stats
            .summarize(recorder.report_index, elapsed.as_secs_f64());
        recorder.stats.reset();
        recorder.interval_start = now;
        Some(report)
    });
    // Formatted and written with NO borrow live, so nothing on the write path can
    // re-enter the accumulator.
    if let Some(report) = report {
        write(&frame_stats::format_report(&report));
    }
}

/// Applies `f` to the accumulator, or does nothing when the instrument is
/// disabled.
///
/// One `enabled()` test per call site, then one `RefCell` borrow.
#[inline]
fn with_stats(f: impl FnOnce(&mut FrameStats)) {
    if !enabled() {
        return;
    }
    debug_assert!(
        REPORTER_THREAD
            .get()
            .is_none_or(|id| *id == std::thread::current().id()),
        "frame_metrics: recorded from a thread other than the one `install()` ran on. The \
         accumulator is a `thread_local!`, so this sample lands in a slot nothing ever reports."
    );
    RECORDER.with(|cell| {
        if let Some(recorder) = cell.borrow_mut().as_mut() {
            f(&mut recorder.stats);
        }
    });
}

/// Writes one already-formatted block to stderr and, when it is open, to the
/// report file.
///
/// Never `eprintln!`, which panics on a failed stderr write; both `Result`s are
/// discarded, because a diagnostic that can take the application down is worse
/// than a lost diagnostic.
fn write(text: &str) {
    let bytes = text.as_bytes();
    let _ = std::io::stderr().write_all(bytes);
    RECORDER.with(|cell| {
        if let Some(recorder) = cell.borrow_mut().as_mut() {
            if let Some(file) = recorder.file.as_mut() {
                let _ = file.write_all(bytes);
            }
        }
    });
}
