//! Shared helpers for `wolf-ffi` bridge integration tests.
//!
//! These mirror `crates/wolf-core/tests/common/mod.rs`, but with one deliberate
//! policy difference: where the wolf-core suite PANICS when `WOLF_NVIM_BIN` is
//! unset (it is meaningless without a real nvim and CI always provides one), the
//! wolf-ffi bridge test must **loudly SKIP** instead. Bazel-on-macOS cannot yet
//! provision a pinned nvim, so this test must be a no-op (not a failure) there,
//! while still running on Linux/Cargo devShells where `WOLF_NVIM_BIN` is set.
//!
//! Resolution of nvim, init.lua, and the temp cwd otherwise follows the wolf-core
//! pattern exactly (real nvim, no mocks).

#![allow(dead_code)] // Not every helper is used by every test binary.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Default grid width used by the bridge smoke test, in columns.
pub const DEFAULT_COLS: u16 = 80;
/// Default grid height used by the bridge smoke test, in rows.
pub const DEFAULT_ROWS: u16 = 24;

/// Returns the `WOLF_NVIM_BIN` path, or `None` after printing a LOUD skip banner
/// to stderr.
///
/// This is the bridge suite's skip gate: a caller that gets `None` must `return`
/// from the test (a no-op pass), NOT panic and NOT silently assert success. The
/// banner makes the skip visible in `--nocapture` output so a misconfigured env
/// is never mistaken for real coverage.
#[must_use]
pub fn nvim_bin_or_skip() -> Option<PathBuf> {
    match std::env::var("WOLF_NVIM_BIN") {
        Ok(v) if !v.trim().is_empty() => Some(PathBuf::from(v)),
        _ => {
            eprintln!(
                "\n========================================================================\n\
                 SKIP: wolf-ffi bridge_smoke — WOLF_NVIM_BIN is unset or empty.\n\
                 This test drives a REAL pinned Neovim through the FFI bridge (no mocks).\n\
                 It runs on Linux/Cargo devShells that export WOLF_NVIM_BIN; on macOS\n\
                 Bazel (which cannot yet provision nvim) it is intentionally skipped.\n\
                 Enter the devShell (`direnv allow`) to run it for real.\n\
                 ========================================================================\n"
            );
            None
        }
    }
}

/// Resolves the in-repo `runtime/nvim/init.lua` from `CARGO_MANIFEST_DIR` (which
/// points at `crates/wolf-ffi`), i.e. `<manifest>/../../runtime/nvim/init.lua`.
///
/// Mirrors wolf-core's `init_lua_path`.
#[must_use]
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

/// Creates a fresh, unique temp directory to serve as the core `cwd` and returns
/// its path. Mirrors wolf-core's `fresh_cwd` (intentionally NOT auto-deleted: the
/// MRU cache lives under `<cwd>/.wolf/` and must outlive `wolf_ffi_start`).
#[must_use]
pub fn fresh_cwd() -> PathBuf {
    let n = CWD_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is before the unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("wolf-ffi-test-cwd-{pid}-{nanos}-{n}"));
    std::fs::create_dir_all(&dir).expect("failed to create temp cwd directory");
    dir
}
