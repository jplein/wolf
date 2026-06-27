//! Shared helpers for `wolf-gnome` integration tests.
//!
//! These mirror `crates/wolf-core/tests/common/mod.rs` but are local to this
//! crate, because Cargo cannot share a `tests/common` module across crates. The
//! round-trip test drives a REAL, hermetic Neovim process (no mocks), located via
//! the `WOLF_NVIM_BIN` environment variable that the flake devShell exports. If
//! that variable is unset we panic loudly rather than silently skip, so CI
//! misconfiguration surfaces immediately (per project policy: prefer real nvim).
//!
//! All driving types are reused from `wolf_core`.

#![allow(dead_code)] // Not every helper is used by every test binary.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use wolf_core::{CoreConfig, WakeFn};

/// Default grid width used by tests, in columns.
pub const DEFAULT_COLS: u16 = 80;
/// Default grid height used by tests, in rows.
pub const DEFAULT_ROWS: u16 = 24;

/// Generous timeout to absorb nvim `--embed` startup + first redraw flush.
/// nvim startup is not instantaneous and redraw is never synchronous, so we wait
/// well beyond the expected latency before declaring failure.
pub const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Panics with a clear, actionable message if `WOLF_NVIM_BIN` is unset or empty.
///
/// The round-trip test is meaningless without a real nvim binary, so we fail
/// loudly instead of skipping. Returns the resolved binary path for convenience.
pub fn require_nvim_bin() -> PathBuf {
    match std::env::var("WOLF_NVIM_BIN") {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => panic!(
            "WOLF_NVIM_BIN is not set (or empty). The wolf-gnome round-trip test \
             requires a real, pinned Neovim binary located via WOLF_NVIM_BIN \
             (exported by the flake devShell). Run via \
             `nix develop --command cargo test -p wolf-gnome`. nvim is NOT taken \
             from PATH."
        ),
    }
}

/// Resolves the path to the in-repo `runtime/nvim/init.lua` from
/// `CARGO_MANIFEST_DIR`. For this crate the manifest dir is `crates/wolf-gnome`,
/// so the runtime lives at `<manifest>/../../runtime/nvim/init.lua`.
pub fn init_lua_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("runtime")
        .join("nvim")
        .join("init.lua")
}

/// Builds a `CoreConfig` with the resolved init.lua path and the given grid size.
///
/// NOTE to coder: assumes `CoreConfig` has public fields `init_lua`, `cols`,
/// `rows` (as it does today in wolf-core). If that changes, adapt this one helper.
pub fn config(cols: u16, rows: u16) -> CoreConfig {
    require_nvim_bin();
    CoreConfig {
        init_lua: init_lua_path(),
        cols,
        rows,
    }
}

/// A wait handle backed by an mpsc channel. The wake callback sends `()` on each
/// flush; tests call `wait_for_flush` to block until the next one.
pub struct FlushWaiter {
    rx: Receiver<()>,
}

impl FlushWaiter {
    /// Blocks until the next flush/wake or panics after `timeout`.
    pub fn wait_for_flush(&self, timeout: Duration) {
        match self.rx.recv_timeout(timeout) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => panic!(
                "timed out after {timeout:?} waiting for a redraw flush/wake from nvim"
            ),
            Err(RecvTimeoutError::Disconnected) => {
                panic!("wake channel disconnected before a flush arrived (nvim/core gone?)")
            }
        }
    }

    /// Drains any already-queued flush signals without blocking. Useful to
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
