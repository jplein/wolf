//! The DISABLED half of [`wolf_gnome::frame_metrics`]: with `WOLF_FRAME_STATS`
//! unset, the instrument is off, `begin` hands back no clock reading, and every
//! other entry point is a no-op.
//!
//! # Why this needs a test binary of its own
//!
//! `frame_metrics::enabled()` is backed by a process-wide `OnceLock` that
//! `frame_metrics::install()` writes exactly once. Whichever test in a binary
//! reaches `install()` first fixes that answer for every test the harness runs
//! after it, in any order and on any thread — so "the instrument was never armed"
//! is a property of a PROCESS, not of a test function. Cargo gives each
//! `tests/*.rs` file its own binary and therefore its own process, and that is the
//! only isolation available.
//!
//! **Folding these cases into an existing test file would leave them green while
//! proving nothing**, because that binary's other tests could arm the instrument
//! before or after them. For the same reason, nothing in this file may call
//! `install()`.
//!
//! # Why the enabled path is out of reach here, and where it IS covered
//!
//! Arming the instrument from inside this process would mean writing
//! `WOLF_FRAME_STATS` into the environment. `std::env::set_var` is `unsafe` in this
//! edition precisely because it races every other thread's reads, and the harness
//! runs tests on several; it would also burn the `OnceLock` for the rest of the
//! binary, so nothing afterwards could observe the disabled state again. This file
//! therefore only READS the environment.
//!
//! The enabled path belongs to a separate process:
//! `frame_stats_report_covers_the_live_typing_path_under_cage` in
//! `smoke_headless.rs` launches the built binary under `cage` with
//! `WOLF_FRAME_STATS=1` and reads back the report file the application appends to.
//!
//! # What a green run establishes, and what it does not
//!
//! Establishes three things about a process nothing armed: `enabled()` answers
//! false rather than defaulting to true or panicking on an unwritten `OnceLock`;
//! `begin()` answers `None`, which is the whole "an unset flag costs one boolean
//! test and no clock read" claim; and every entry point tolerates an ABSENT
//! recorder, since no `install()` has run, the thread-local slot is `None`, and
//! each function must take its early return instead of unwrapping.
//!
//! Does NOT establish that nothing reached stderr. Capturing file descriptor 2
//! needs a dev-dependency this crate declines for the lock-file reason
//! `release_profile.rs`'s module doc records, and the check would be close to
//! vacuous anyway: with no recorder installed, even an `enabled()` forced to true
//! writes nothing, because `emit_report` returns on the empty slot before it
//! formats a byte. Does NOT establish what the counters did either —
//! `frame_metrics` deliberately exposes no reader, so the recording half is the
//! cage smoke's alone.

use std::time::Instant;

use wolf_gnome::frame_metrics;
use wolf_gnome::frame_stats::{self, Bucket};

/// The variable that arms the instrument, re-typed as a literal because
/// `frame_metrics` does not export its name.
const ENABLED_ENV: &str = "WOLF_FRAME_STATS";

/// Fails when the ambient environment arms the instrument, so the conditions every
/// test here describes are the conditions that actually ran.
///
/// This is a PREMISE check rather than an assertion about the parser. Nothing in
/// this binary calls `install()`, so `enabled()` answers false whatever the variable
/// holds — which is exactly why the premise has to be checked instead of inferred:
/// a reader who found `WOLF_FRAME_STATS=1` exported and this file green would
/// otherwise be entitled to conclude something false about `frame_stats::is_enabled`,
/// whose own coverage is `is_enabled_table` in the seam.
fn require_the_flag_unset() {
    let raw = std::env::var(ENABLED_ENV).ok();
    assert!(
        !frame_stats::is_enabled(raw.as_deref()),
        "{ENABLED_ENV} is set to {raw:?} in the environment running this test, and every assertion \
         in this binary describes a process in which the instrument was never armed. Nothing here \
         calls `frame_metrics::install()`, so the assertions below still hold — but this file \
         would be documenting conditions that did not occur. Re-run without the variable: the \
         cage smoke sets it on the child process it spawns, never on the test process."
    );
}

/// `enabled()` answers false in a process where `install()` never ran.
///
/// That is the state every shipped run without the flag is in, and the state the
/// three tests below make their calls from. Two defects it discriminates: a
/// default-on read of the unwritten `OnceLock` — an `unwrap_or(true)` — which arms
/// the instrument for every user who never asked for it; and an `expect`/`unwrap` on
/// it, which turns the first instrumented call site into a panic in exactly the
/// build that carries no instrumentation.
#[test]
fn enabled_is_false_in_a_process_that_never_armed_the_instrument() {
    require_the_flag_unset();

    assert!(
        !frame_metrics::enabled(),
        "frame_metrics::enabled() answered true in a process where install() never ran. Every \
         hot-path function branches on this one boolean, so a true here puts a clock read, a \
         thread-local borrow and a debug_assert around fifteen regions of a build nobody asked to \
         instrument."
    );

    assert!(
        !frame_metrics::enabled(),
        "frame_metrics::enabled() changed its answer between two calls in one process. It reads a \
         OnceLock that only install() writes and that nothing in this binary touches, so the \
         answer must hold for the process's whole life; an unstable one means the enabled state is \
         being recomputed per call rather than read once."
    );
}

/// `begin()` answers `None` while disabled, which is the cost-when-off claim itself.
///
/// Every instrumented region is a `begin`/`end` pair or a `scope` built on one, so
/// this single answer is what decides whether a flag-less build reads the monotonic
/// clock twice per region or not at all.
#[test]
fn begin_answers_none_while_disabled_so_no_call_site_reads_a_clock() {
    require_the_flag_unset();

    assert!(
        frame_metrics::begin().is_none(),
        "frame_metrics::begin() handed back a start instant with the instrument disabled. The \
         `None` IS the no-cost property: a Some here means two `Instant::now()` calls per region — \
         fifteen regions, several of them per pane per frame — plus the subtraction and the u64 \
         conversion in `end`, all for a sample no report will ever carry."
    );
}

/// Every hot-path entry point tolerates the disabled state: none panics, and none
/// arms the instrument as a side effect.
///
/// The calls are made over EVERY bucket, and `end` is handed a real `Instant` as
/// well as the `None` a disabled `begin` produces. The `Some` arm matters: it is the
/// only route by which a disabled process reaches the recorder lookup at all, and a
/// caller holding a start across a flag change would hand it exactly that.
///
/// Reaching the closing assertions is the no-panic observation. With no `install()`
/// behind them the thread-local recorder slot is `None`, so each of these must take
/// an early return rather than unwrap an absent recorder — and a [`Scope`] must
/// survive its own `Drop` against the same empty slot.
///
/// [`Scope`]: wolf_gnome::frame_metrics::Scope
#[test]
fn every_hot_path_entry_point_is_a_no_op_while_disabled() {
    require_the_flag_unset();

    for bucket in Bucket::ALL {
        frame_metrics::end(bucket, None);
        frame_metrics::end(bucket, Some(Instant::now()));

        // A named guard, never `let _ = …`, which drops instantly unlike the shipped call sites.
        // Its drop at the end of this iteration calls `end` a third time.
        let _guard = frame_metrics::scope(bucket);
    }

    frame_metrics::note_keystroke();
    frame_metrics::note_wake();

    assert!(
        !frame_metrics::enabled(),
        "one of the calls above armed the instrument. `enabled()` reads a OnceLock only install() \
         may write, so a true here means an entry point writes it — after which a flag-less build \
         starts recording, and eventually reporting, on its own."
    );
    assert!(
        frame_metrics::begin().is_none(),
        "begin() answered Some after end / scope / note_keystroke / note_wake had run, so the \
         disabled path is not stable: one of them changed what a later call site sees."
    );
}

/// `maybe_emit` and `flush_final` do nothing while disabled, and survive having no
/// recorder behind them.
///
/// `maybe_emit` is the last statement of the drain-loop body, so it runs once per
/// render wake in every build whether or not the flag is set; `flush_final` runs
/// once after `adw::Application::run_with_args` returns. Both are called more than
/// once here, because a second call is what would find state the first left behind.
///
/// What this observes is narrow, and worth stating exactly: both calls are TOTAL on
/// a process that never installed a recorder, which is the state the drain loop of
/// every flag-less build calls them from. Two edits it catches are an unwrap of the
/// recorder slot reached ahead of the enabled branch, and an emit path that panics on
/// an interval baseline nothing ever set — either of which lands its panic in a
/// user's drain loop, where no report was wanted and nobody is reading stderr. What
/// it cannot catch is either function writing, for the reason the module doc gives.
#[test]
fn maybe_emit_and_flush_final_do_nothing_while_disabled() {
    require_the_flag_unset();

    for _ in 0..4 {
        frame_metrics::maybe_emit();
    }
    frame_metrics::flush_final();
    frame_metrics::flush_final();

    assert!(
        !frame_metrics::enabled(),
        "maybe_emit() or flush_final() armed the instrument. Both must return on the disabled \
         branch before they touch the clock, the recorder or a write."
    );
    assert!(
        frame_metrics::begin().is_none(),
        "begin() answered Some after the emit calls, so an emit path left the process in a state a \
         later call site can see. A disabled emit must be indistinguishable from not being called."
    );
}
