//! Shared helpers for `wolf-gnome` integration tests.
//!
//! These mirror `crates/wolf-core/tests/common/mod.rs` but are local to this
//! crate, because Cargo cannot share a `tests/common` module across crates. The
//! real-nvim tests drive a real, hermetic Neovim process (no mocks), located via
//! the `WOLF_NVIM_BIN` environment variable that the flake devShell exports. If
//! that variable is unset we panic loudly rather than silently skip, so CI
//! misconfiguration surfaces immediately (per project policy: prefer real nvim).
//!
//! All driving types are reused from `wolf_core`.
//!
//! [`config_in`] is the state-dir funnel: every core built from its config runs against
//! a throwaway `WOLF_STATE_DIR`, never the user's own Wolf state dir.

#![allow(dead_code)] // Not every helper is used by every test binary.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, OnceLock};
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
/// The real-nvim tests are meaningless without a real nvim binary, so we fail
/// loudly instead of skipping. Returns the resolved binary path for convenience.
pub fn require_nvim_bin() -> PathBuf {
    match std::env::var("WOLF_NVIM_BIN") {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => panic!(
            "WOLF_NVIM_BIN is not set (or empty). The wolf-gnome real-nvim tests \
             require a real, pinned Neovim binary located via WOLF_NVIM_BIN \
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

/// Monotonic counter to disambiguate temp dirs created within one process.
static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Creates a fresh directory `<temp>/<prefix>-<pid>-<nanos>-<n>` under the OS temp dir
/// and returns its path. Not auto-deleted.
///
/// Built on `std` rather than `tempfile`: a new dev-dependency would add a line to
/// this crate's `Cargo.lock` entry, the repin obligation explained in
/// `release_profile.rs`'s header. Creation is exclusive (`create_dir`, not
/// `create_dir_all`), so a name collision panics instead of sharing a directory.
pub fn unique_temp_dir(prefix: &str) -> PathBuf {
    let n = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is before the unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}-{pid}-{nanos}-{n}"));
    std::fs::create_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to create temp dir {}: {e}", dir.display()));
    dir
}

/// Creates a fresh, unique directory under the OS temp dir to serve as a core
/// `cwd`, and returns its path. Not auto-deleted (see the wolf-core harness for
/// the rationale): it is nvim's working dir + file-search root, so it must outlive
/// the session. The recent-files MRU lives in the global
/// `state_dir()/file-mru.sqlite`, not under the cwd.
pub fn fresh_cwd() -> PathBuf {
    unique_temp_dir("wolf-test-cwd")
}

/// Records the throwaway state dir published by [`ensure_hermetic_state_dir`] for the
/// whole test binary. `None` records that the caller had already set `WOLF_STATE_DIR`,
/// which is then left alone. The directory is never deleted, so it cannot vanish out
/// from under a running core.
static HERMETIC_STATE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Points `WOLF_STATE_DIR` at a fresh `wolf-gnome-test-state-*` dir under the system
/// temp dir, once per test binary and only if the variable is unset.
///
/// Without it `wolf_core::state_dir()` is `$XDG_STATE_HOME/wolf` or
/// `$HOME/.local/state/wolf`, the user's real Wolf state dir. Every real-nvim test
/// would then source the user's `runtime_user/init_user.lua` and write swap, MRU,
/// spell and `parsers_user` files there. `state_isolation.rs` is the guard.
///
/// Set-if-absent, so a test that sets its own `WOLF_STATE_DIR` first keeps it. The
/// write runs once per binary inside [`HERMETIC_STATE_DIR`]'s `get_or_init`, because
/// `setenv` races `getenv` on the other test threads libtest runs in parallel, and
/// `get_or_init` blocks every other caller until the write is done. So a test that
/// removes the variable afterwards must be alone in its binary: nothing would set it
/// again for its neighbours.
fn ensure_hermetic_state_dir() {
    HERMETIC_STATE_DIR.get_or_init(|| {
        if std::env::var_os("WOLF_STATE_DIR").is_some() {
            return None;
        }
        let dir = unique_temp_dir("wolf-gnome-test-state");
        std::env::set_var("WOLF_STATE_DIR", &dir);
        Some(dir)
    });
}

/// Builds a `CoreConfig` with the resolved init.lua path, the given grid size,
/// and a fresh per-call `cwd`.
///
/// Assumes `CoreConfig` has public fields `init_lua`, `cols`, `rows` and `cwd`.
/// If that changes, adapt [`config_in`] (the single literal).
pub fn config(cols: u16, rows: u16) -> CoreConfig {
    config_in(fresh_cwd(), cols, rows)
}

/// Like [`config`] but with an explicit `cwd`.
///
/// It is also the state-dir funnel: it runs [`ensure_hermetic_state_dir`] before
/// building the config, so every test must build its `CoreConfig` here (directly or
/// through [`config`]) before `WolfCore::start`.
pub fn config_in(cwd: PathBuf, cols: u16, rows: u16) -> CoreConfig {
    require_nvim_bin();
    ensure_hermetic_state_dir();
    CoreConfig {
        init_lua: init_lua_path(),
        cols,
        rows,
        cwd,
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
            Err(RecvTimeoutError::Timeout) => {
                panic!("timed out after {timeout:?} waiting for a redraw flush/wake from nvim")
            }
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
