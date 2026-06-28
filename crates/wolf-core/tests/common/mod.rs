//! Shared helpers for `wolf-core` integration tests.
//!
//! These tests drive a REAL, hermetic Neovim process (no mocks), located via the
//! `WOLF_NVIM_BIN` environment variable that the flake devShell exports. If that
//! variable is unset we panic loudly rather than silently skip, so CI
//! misconfiguration surfaces immediately.
//!
//! Public API surface consumed here mirrors the plan's `src/api` + `src/input`
//! re-exports. Where a method name is genuinely ambiguous in the plan a
//! `// NOTE:` comment flags the assumption for the coder to align.

#![allow(dead_code)] // Not every helper is used by every test binary.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use std::io::Write as _;

use wolf_core::{
    CoreConfig, InputEvent, Key, KeyEvent, WakeFn, WolfCore,
};

/// Default grid width used by most tests, in columns.
pub const DEFAULT_COLS: u16 = 80;
/// Default grid height used by most tests, in rows.
pub const DEFAULT_ROWS: u16 = 24;

/// Generous timeout to absorb nvim `--embed` startup + first redraw flush.
/// nvim startup is not instantaneous and redraw is never synchronous, so we wait
/// well beyond the expected latency before declaring failure.
pub const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Panics with a clear, actionable message if `WOLF_NVIM_BIN` is unset or empty.
///
/// The whole test suite is meaningless without a real nvim binary, so we fail
/// loudly instead of skipping. Returns the resolved binary path for convenience.
pub fn require_nvim_bin() -> PathBuf {
    match std::env::var("WOLF_NVIM_BIN") {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => panic!(
            "WOLF_NVIM_BIN is not set (or empty). These integration tests require a \
             real, pinned Neovim binary located via WOLF_NVIM_BIN (exported by the \
             flake devShell). Enter the devShell (`direnv allow`) before running tests. \
             nvim is NOT taken from PATH."
        ),
    }
}

/// Resolves the path to the in-repo `runtime/nvim/init.lua` from
/// `CARGO_MANIFEST_DIR` (which points at `crates/wolf-core`), i.e.
/// `<manifest>/../../runtime/nvim/init.lua`.
pub fn init_lua_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("runtime")
        .join("nvim")
        .join("init.lua")
}

/// Monotonic counter to disambiguate temp `cwd`s created within one process.
static CWD_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Creates a fresh, unique directory under the OS temp dir to serve as a core
/// `cwd`, and returns its path.
///
/// The directory is intentionally NOT auto-deleted: [`config`] returns only a
/// `CoreConfig`, so there is no `TempDir` guard to keep alive, and the MRU cache
/// (S1) lives at `<cwd>/.wolf/mru.sqlite`. Leaking one small temp dir per test is
/// acceptable for this hermetic suite, and avoids the directory vanishing out
/// from under `WolfCore::start` before it opens the cache.
pub fn fresh_cwd() -> PathBuf {
    let n = CWD_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is before the unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("wolf-test-cwd-{pid}-{nanos}-{n}"));
    std::fs::create_dir_all(&dir).expect("failed to create temp cwd directory");
    dir
}

/// Builds a `CoreConfig` with the resolved init.lua path, the given grid size,
/// and a fresh per-call `cwd` (so each core gets its own MRU cache directory).
///
/// NOTE: assumes `CoreConfig` has public fields `init_lua`, `cols`, `rows`, and
/// (new in S1) `cwd` (per plan §D6). If `CoreConfig` instead exposes a
/// builder/constructor, the coder should adapt [`config_in`] (the single literal).
pub fn config(cols: u16, rows: u16) -> CoreConfig {
    config_in(fresh_cwd(), cols, rows)
}

/// Like [`config`] but with an explicit `cwd`, for tests that must control where
/// the MRU cache (`<cwd>/.wolf/mru.sqlite`) lives so they can inspect it.
pub fn config_in(cwd: PathBuf, cols: u16, rows: u16) -> CoreConfig {
    require_nvim_bin();
    CoreConfig {
        init_lua: init_lua_path(),
        cols,
        rows,
        cwd,
    }
}

/// A wait handle backed by an mpsc channel. The wake callback sends `()` on
/// EVERY wake — both real redraw flushes and out-of-band wakes such as the
/// `wolf_current_file` notification. To distinguish a genuine redraw flush from
/// an out-of-band wake, `wait_for_flush` keys on the snapshot's `seq` (the redraw
/// frame counter, bumped only on a real flush) ADVANCING, not on the raw wake
/// count. A `current_file` change wakes but does NOT bump `seq`, so it is ignored
/// here while still letting `current_file.rs` observe it by polling directly.
pub struct FlushWaiter {
    rx: Receiver<()>,
}

impl FlushWaiter {
    /// Blocks until a genuine redraw flush lands, i.e. until `core`'s snapshot
    /// `seq` advances beyond its value at the moment of this call. Out-of-band
    /// wakes (e.g. `wolf_current_file`, which does not bump `seq`) are consumed
    /// and ignored so they cannot be mistaken for a flush. Panics after `timeout`.
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

    /// Blocks until the FIRST redraw flush has landed, i.e. until `core`'s
    /// snapshot `seq` reaches at least 1.
    ///
    /// Unlike [`wait_for_flush`](Self::wait_for_flush), this keys on an ABSOLUTE
    /// threshold (`seq >= 1`) rather than on `seq` advancing from a captured
    /// baseline. That is required for the startup/attach wait: the attach redraw
    /// flush (`seq` 0 -> 1) lands DURING `WolfCore::start` (it `block_on`s
    /// `ui_attach` before returning), so by the time a test calls this the snapshot
    /// is already at `seq == 1`. An advance-based wait would baseline at 1 and then
    /// block forever for `seq > 1`. This method therefore RETURNS IMMEDIATELY when
    /// `seq >= 1` already (the common case), and only otherwise blocks on the wake
    /// channel. Out-of-band wakes (e.g. `wolf_current_file`, which does not bump
    /// `seq`) are consumed and ignored. Panics after `timeout`.
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

    /// Drains any already-queued wake signals without blocking. Useful to
    /// discard wakes that occurred before a test starts observing.
    pub fn drain(&self) {
        while self.rx.try_recv().is_ok() {}
    }
}

/// Builds a `WakeFn` (`Arc<dyn Fn() + Send + Sync>`) that signals on each flush,
/// paired with the `FlushWaiter` that observes it.
///
/// The wake may fire on a tokio worker thread, so the closure must be
/// `Send + Sync`; an mpsc `Sender<()>` is both.
pub fn wake_channel() -> (WakeFn, FlushWaiter) {
    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let wake: WakeFn = Arc::new(move || {
        // A send error only means the receiver was dropped (test finished); ignore.
        let _ = tx.send(());
    });
    (wake, FlushWaiter { rx })
}

/// Writes `content` to a fresh temp file with a `.md` extension and returns it.
/// The returned `NamedTempFile` keeps the file alive for the test's duration.
pub fn temp_md(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .prefix("wolf-test-")
        .suffix(".md")
        .tempfile()
        .expect("failed to create temp .md file");
    f.write_all(content.as_bytes())
        .expect("failed to write temp .md content");
    f.flush().expect("failed to flush temp .md content");
    f
}

/// Allocates a path for a not-yet-existing `.md` file (for SaveAs). The file is
/// NOT created on disk; the directory guard keeps the parent alive.
pub fn temp_md_path() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().join("wolf-saveas.md");
    (dir, path)
}

/// Starts a `WolfCore` at default grid size, opens a temp `.md` containing
/// `content`, and waits for the first flush so the grid reflects the open file.
///
/// Returns the live core, its flush waiter, and the temp file (kept alive).
pub fn start_core_with_temp_md(
    content: &str,
) -> (WolfCore, FlushWaiter, tempfile::NamedTempFile) {
    start_core_with_temp_md_sized(content, DEFAULT_COLS, DEFAULT_ROWS)
}

/// Like `start_core_with_temp_md` but with an explicit grid size.
pub fn start_core_with_temp_md_sized(
    content: &str,
    cols: u16,
    rows: u16,
) -> (WolfCore, FlushWaiter, tempfile::NamedTempFile) {
    let file = temp_md(content);
    let (wake, waiter) = wake_channel();

    let core = WolfCore::start(config(cols, rows), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");

    // Wait for the initial attach/flush before opening so we don't race startup.
    // The attach flush (seq 0 -> 1) lands during WolfCore::start, so this keys on
    // the absolute threshold (seq >= 1) and returns immediately in the common case;
    // an advance-based wait would baseline at 1 and block forever.
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);

    // Open the file; nvim will emit a redraw + flush once the buffer is shown.
    core.send_input(InputEvent::Open {
        path: file.path().to_path_buf(),
    });
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    (core, waiter, file)
}

/// Starts a bare `WolfCore` (no file opened) at the given grid size and waits
/// for the first flush.
pub fn start_core_sized(cols: u16, rows: u16) -> (WolfCore, FlushWaiter) {
    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(config(cols, rows), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    // Attach flush (seq 0 -> 1) lands during WolfCore::start; key on seq >= 1.
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter)
}

// --- Small input constructors to cut repetition ---------------------------

/// A plain (unmodified) key press.
pub fn key(k: Key) -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: k,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    })
}

/// A single plain character key press.
pub fn ch(c: char) -> InputEvent {
    key(Key::Char(c))
}

/// A Ctrl-modified key press.
pub fn ctrl(k: Key) -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: k,
        ctrl: true,
        shift: false,
        alt: false,
        meta: false,
    })
}

/// Sends each character in `s` as an individual key event, in order.
pub fn type_str(core: &WolfCore, s: &str) {
    for c in s.chars() {
        core.send_input(ch(c));
    }
}
