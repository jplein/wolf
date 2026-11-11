//! Temp dirs, the hermetic `WOLF_STATE_DIR`, and the `CoreConfig` builders.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use wolf_core::CoreConfig;

use crate::resolve::{init_lua_path, require_nvim_bin};

/// Monotonic counter to disambiguate temp dirs created within one process.
static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Creates a fresh directory `<temp>/<prefix>-<pid>-<nanos>-<n>` under the OS temp dir and
/// returns its path. Not auto-deleted.
///
/// Creation is exclusive (`create_dir`, not `create_dir_all`), so a name collision panics
/// instead of letting two tests share a directory.
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

/// Creates a fresh, unique directory under the OS temp dir to serve as a core `cwd`, and
/// returns its path.
///
/// The directory is intentionally not auto-deleted: [`config`] returns only a `CoreConfig`, so
/// there is no guard to keep alive, and the dir must not vanish out from under a running core,
/// whose nvim working dir and search root it is. The recent-files MRU lives in the global
/// `state_dir()/file-mru.sqlite`, not under the cwd.
pub fn fresh_cwd() -> PathBuf {
    unique_temp_dir("wolf-test-cwd")
}

/// Records the throwaway state dir published by [`ensure_hermetic_state_dir`] for the whole test
/// binary. `None` records that the caller had already set `WOLF_STATE_DIR`, which is then left
/// alone. The directory is never deleted, so it cannot vanish out from under a running core.
static HERMETIC_STATE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Points `WOLF_STATE_DIR` at a fresh `wolf-test-state-*` dir under the system temp dir, once
/// per test binary and only if the variable is unset.
///
/// Without it `wolf_core::state_dir()` is the user's real Wolf state dir on both platforms:
/// Bazel strips `HOME` from the test env, but `dirs` then falls back to `getpwuid`, the real
/// home, and `cargo test` inherits `HOME` outright. Every real-nvim test would then source the
/// user's `runtime_user/init_user.lua` and write swap, MRU, spell and `parsers_user` files there.
///
/// Set-if-absent, so a test that sets its own `WOLF_STATE_DIR` first keeps it. The write runs
/// once per binary inside [`HERMETIC_STATE_DIR`]'s `get_or_init`, because `setenv` races
/// `getenv` on the other test threads libtest runs in parallel, and `get_or_init` blocks every
/// other caller until the write is done. So a test that removes the variable afterwards must be
/// alone in its binary: nothing would set it again for its neighbours.
pub(crate) fn ensure_hermetic_state_dir() {
    HERMETIC_STATE_DIR.get_or_init(|| {
        if std::env::var_os("WOLF_STATE_DIR").is_some() {
            return None;
        }
        let dir = unique_temp_dir("wolf-test-state");
        std::env::set_var("WOLF_STATE_DIR", &dir);
        Some(dir)
    });
}

/// Points `WOLF_STATE_DIR` at the process's throwaway dir if it is unset, as [`config_in`]
/// does, and returns `wolf_core::state_dir()`.
///
/// The recent-files MRU lives in one global DB at `state_dir()/file-mru.sqlite`
/// (directory-filtered), so a real-nvim test that must pre-seed or inspect the MRU opens a
/// `MruStore` against this dir, the same dir the core resolves.
pub fn hermetic_state_dir() -> PathBuf {
    ensure_hermetic_state_dir();
    wolf_core::state_dir().expect("state_dir() should resolve once WOLF_STATE_DIR is set")
}

/// Builds a `CoreConfig` with the resolved init.lua path, the given grid size, and a fresh
/// per-call [`fresh_cwd`], so each core gets its own working directory.
pub fn config(cols: u16, rows: u16) -> CoreConfig {
    config_in(fresh_cwd(), cols, rows)
}

/// Like [`config`] but with an explicit `cwd`, for tests that must control nvim's working
/// directory: the search root, and the `directory` scope under which opens are recorded in the
/// global MRU.
///
/// It is the state-dir funnel: it resolves and publishes the pinned nvim
/// ([`crate::require_nvim_bin`]) and points `WOLF_STATE_DIR` at a throwaway dir before building
/// the config, so every test must build its `CoreConfig` here, directly or through [`config`],
/// before `WolfCore::start`.
pub fn config_in(cwd: PathBuf, cols: u16, rows: u16) -> CoreConfig {
    require_nvim_bin();
    ensure_hermetic_state_dir();
    CoreConfig {
        init_lua: init_lua_path(),
        cols,
        rows,
        cwd,
        terminfo_dir: None,
    }
}
