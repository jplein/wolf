//! The wake channel a test core signals, and the waits built on it.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wolf_core::{WakeFn, WolfCore};

use crate::{config, FLUSH_TIMEOUT};

/// A wait handle backed by an mpsc channel. The wake callback sends `()` on every wake: both
/// real redraw flushes and out-of-band wakes such as the `wolf_current_file` notification, a
/// terminal frame or a liveness change.
///
/// To tell a genuine redraw flush from an out-of-band wake, the flush waits key on the
/// snapshot's `seq` (the redraw frame counter, bumped only on a real flush), not on the raw wake
/// count. An out-of-band wake does not bump `seq`, so it is ignored there, while
/// [`wait_for_wake`](Self::wait_for_wake) still observes it.
pub struct FlushWaiter {
    rx: Receiver<()>,
}

impl FlushWaiter {
    /// Blocks until a genuine redraw flush lands, i.e. until `core`'s snapshot `seq` advances
    /// beyond its value at the moment of this call. Out-of-band wakes are consumed and ignored
    /// so they cannot be mistaken for a flush. Panics after `timeout`.
    pub fn wait_for_flush(&self, core: &WolfCore, timeout: Duration) {
        let baseline = core.read_snapshot().seq;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(remaining) {
                Ok(()) => {
                    // A wake arrived; it is only a real flush if seq advanced.
                    if core.read_snapshot().seq > baseline {
                        return;
                    }
                    // Out-of-band wake (e.g. current_file): keep waiting.
                }
                Err(RecvTimeoutError::Timeout) => panic!(
                    "timed out after {timeout:?} waiting for a redraw flush from nvim \
                     (snapshot seq never advanced past {baseline})"
                ),
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("wake channel disconnected before a flush arrived (nvim/core gone?)")
                }
            }
        }
    }

    /// Blocks until the first redraw flush has landed, i.e. until `core`'s snapshot `seq`
    /// reaches at least 1. Out-of-band wakes are consumed and ignored. Panics after `timeout`.
    ///
    /// This is the wait to use right after `WolfCore::start`. It keys on an absolute threshold
    /// rather than on `seq` advancing, because the attach flush (`seq` 0 → 1) lands during
    /// `WolfCore::start`, which `block_on`s `ui_attach` before returning. An advance-based wait
    /// would baseline at 1 and block for `seq > 1`; this one returns at once when `seq >= 1`.
    pub fn wait_for_initial_flush(&self, core: &WolfCore, timeout: Duration) {
        const INITIAL_FLUSH_SEQ: u64 = 1;
        // Common case: the attach flush already landed during WolfCore::start.
        if core.read_snapshot().seq >= INITIAL_FLUSH_SEQ {
            return;
        }
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(remaining) {
                Ok(()) => {
                    // A wake arrived; the attach flush only counts once seq >= 1.
                    if core.read_snapshot().seq >= INITIAL_FLUSH_SEQ {
                        return;
                    }
                    // Out-of-band wake (e.g. current_file): keep waiting.
                }
                Err(RecvTimeoutError::Timeout) => panic!(
                    "timed out after {timeout:?} waiting for the initial redraw flush \
                     from nvim (snapshot seq never reached {INITIAL_FLUSH_SEQ})"
                ),
                Err(RecvTimeoutError::Disconnected) => panic!(
                    "wake channel disconnected before the initial flush arrived \
                     (nvim/core gone?)"
                ),
            }
        }
    }

    /// Blocks until `core`'s snapshot has `current_file` equal to `path` (canonicalized), a `seq`
    /// past `baseline`, and, when `expect_text` is set, text on row 0 of the active grid. Panics
    /// naming `path` after `timeout`.
    ///
    /// `baseline` must be read after a blocking RPC that follows startup, so that no frame nvim
    /// had already drawn can carry `seq` past it; wolf-core's `start_core_with_temp_md_sized`
    /// does so. That fences off no frame from work nvim schedules later, such as treesitter's
    /// asynchronous parse of the startup buffer finishing, which can push `seq` past the baseline
    /// before the `:edit` draws. The startup buffer is empty and its row 0 blank, so row 0
    /// showing text tells the open's frame apart from such a stray one.
    pub fn wait_for_open(
        &self,
        core: &WolfCore,
        path: &Path,
        baseline: u64,
        expect_text: bool,
        timeout: Duration,
    ) {
        // The core canonicalizes `current_file` (macOS `/var` → `/private/var`).
        let expected = std::fs::canonicalize(path).unwrap_or_else(|e| {
            panic!(
                "failed to canonicalize the opened file {}: {e}",
                path.display()
            )
        });
        let deadline = Instant::now() + timeout;
        loop {
            // Scoped so the read guard drops before blocking on the channel.
            let (current, seq, row0) = {
                let snap = core.read_snapshot();
                (
                    snap.current_file.clone(),
                    snap.seq,
                    snap.active_grid().row_text(0),
                )
            };
            if seq > baseline
                && current.as_deref() == Some(expected.as_path())
                && (!expect_text || !row0.trim().is_empty())
            {
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            // A zero `remaining` is a timeout, so a steady stream of wakes cannot outlast it.
            let woke = if remaining.is_zero() {
                Err(RecvTimeoutError::Timeout)
            } else {
                self.rx.recv_timeout(remaining)
            };
            match woke {
                // Every snapshot change wakes after it is applied, so re-checking misses none.
                Ok(()) => {}
                Err(RecvTimeoutError::Timeout) => panic!(
                    "timed out after {timeout:?} waiting for {} to open: current_file \
                     {current:?}, seq {seq} (baseline {baseline}), row 0 {row0:?} (expect_text \
                     {expect_text}). The file never became the current buffer, no frame landed \
                     after it did, or its frame never drew the first line on row 0",
                    path.display()
                ),
                Err(RecvTimeoutError::Disconnected) => panic!(
                    "wake channel disconnected before {} opened (nvim/core gone?)",
                    path.display()
                ),
            }
        }
    }

    /// Drains any already-queued wake signals without blocking. Useful to discard wakes that
    /// occurred before a test starts observing.
    pub fn drain(&self) {
        while self.rx.try_recv().is_ok() {}
    }

    /// Blocks until any raw wake signal arrives, returning `true` on success or `false` after
    /// `timeout`.
    ///
    /// Unlike [`wait_for_flush`](Self::wait_for_flush), this does not require the snapshot `seq`
    /// to advance: it observes the wake seam directly. It exists for the liveness tests, where a
    /// state transition (e.g. nvim death, marked `Dead` by the supervisor) fires a wake so the
    /// front-end can re-read `liveness()`, but does not bump `seq`. Callers that want to
    /// attribute the wake to a specific episode should [`drain`](Self::drain) first.
    pub fn wait_for_wake(&self, timeout: Duration) -> bool {
        matches!(self.rx.recv_timeout(timeout), Ok(()))
    }
}

/// Builds a `WakeFn` (`Arc<dyn Fn() + Send + Sync>`) that signals on each wake, paired with the
/// [`FlushWaiter`] that observes it.
///
/// The wake may fire on a tokio worker thread, so the closure must be `Send + Sync`; an mpsc
/// `Sender<()>` is both.
pub fn wake_channel() -> (WakeFn, FlushWaiter) {
    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let wake: WakeFn = Arc::new(move || {
        // A send error only means the receiver was dropped (test finished); ignore.
        let _ = tx.send(());
    });
    (wake, FlushWaiter { rx })
}

/// The instants at which a core's wake fired, recorded by [`recording_wake_channel`].
#[derive(Clone, Default)]
pub struct WakeLog(Arc<Mutex<Vec<Instant>>>);

impl WakeLog {
    /// Every wake recorded since the last [`clear`](Self::clear), in order.
    pub fn instants(&self) -> Vec<Instant> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Forgets every recorded wake.
    pub fn clear(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// Like [`wake_channel`], and also records the instant of every wake in the returned
/// [`WakeLog`], taken on the thread that fired it.
pub fn recording_wake_channel() -> (WakeFn, FlushWaiter, WakeLog) {
    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let log = WakeLog::default();
    let recorder = log.clone();
    let wake: WakeFn = Arc::new(move || {
        recorder
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Instant::now());
        let _ = tx.send(());
    });
    (wake, FlushWaiter { rx }, log)
}

/// Starts a bare `WolfCore` (no file opened) at the given grid size in a fresh cwd, and waits
/// for the attach flush with [`FlushWaiter::wait_for_initial_flush`].
pub fn start_core_sized(cols: u16, rows: u16) -> (WolfCore, FlushWaiter) {
    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(config(cols, rows), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter)
}
