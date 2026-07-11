//! Integration test for the `wolf-ffi` RECENT-DIRECTORIES bridge accessor
//! (plan §FFI):
//!
//!   * `WolfCoreHandle::recent_dirs(&self) -> Vec<String>` — returns the GLOBAL
//!     recent-directories MRU newest-first (capped at 20), as owned Strings;
//!     an empty `Vec` when the store is disabled/unreadable or the core is gone.
//!     It delegates to `WolfCore::recent_dirs()` and maps each `PathBuf` via
//!     `to_string_lossy().into_owned()`.
//!
//! The store records on EVERY cwd change (including nvim's initial launch cwd),
//! writes to `<state_dir>/folder-mru.sqlite`, and is GLOBAL — so the returned
//! list also contains the launch cwd, not only dirs we explicitly `chdir` into.
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is modeled on `tests/file_actions.rs` (chdir +
//! poll-`cwd()`) and `tests/state.rs` (staging `WOLF_STATE_DIR` to a tempdir
//! BEFORE start), reusing `tests/common/mod.rs` and the same "poll `read_frame()`
//! until attached" pattern.
//!
//! ## Dependency on coder work (plan §FFI)
//!
//! `recent_dirs` does NOT exist on `WolfCoreHandle` yet: the coder adds it to the
//! bridge (`extern "Rust"`) and `impl WolfCoreHandle` in
//! `crates/wolf-ffi/src/lib.rs`, delegating to the existing
//! `wolf_core::WolfCore::recent_dirs()`, AND adds the
//! `//crates/wolf-ffi:recent_dirs` `rust_test` target in `BUILD.bazel`. This test
//! file does NOT touch non-test source, and it will not compile until that method
//! lands — that is expected.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.
//!
//! ## SINGLE #[test]
//!
//! The whole flow lives in ONE `#[test]` because it mutates the process-global
//! `WOLF_STATE_DIR` env var (the `state_dir()` determinism hook, where the store
//! opens `folder-mru.sqlite` once at core start): with a single test in this
//! binary there is nothing to race, so no env lock is needed (the precedent is
//! `tests/state.rs`).

mod common;

use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::wolf_ffi_start;

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (the `:cd` notification round-trip and the fire-and-forget MRU write) to
/// settle. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Canonicalizes a path string, falling back to the raw path when it cannot be
/// resolved on disk. Robust to macOS `/var` -> `/private/var` symlinking, exactly
/// like the canonicalize-compare idiom in `file_actions.rs` / `state.rs`.
fn canonicalize(path: &str) -> PathBuf {
    Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(path).to_path_buf())
}

/// Blocks (bounded by `FLUSH_TIMEOUT`) until `handle.cwd()` canonically equals
/// `target`, then asserts it. Mirrors the poll-`cwd()` loop in `file_actions.rs`.
fn wait_for_cwd(handle: &wolf_ffi::WolfCoreHandle, target: &Path) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut reported = handle.cwd();
    while Instant::now() < deadline {
        reported = handle.cwd();
        if !reported.is_empty() && canonicalize(&reported) == target {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        canonicalize(&reported),
        target,
        "cwd() should follow handle.chdir (reported={reported:?}, target={target:?})"
    );
}

#[test]
fn recent_dirs_reports_chdir_targets_newest_first() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Point the state machinery at a private tempdir for the WHOLE test, BEFORE
    // starting the core: the recent-dirs store opens `folder-mru.sqlite` at
    // `state_dir()` exactly once in `WolfCore::start`. Safe to mutate because this
    // binary has exactly one test. Keep the guard alive until after `shutdown()`.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    // A fresh temp cwd for nvim's launch (its own MRU/state must outlive the
    // session), kept alive until after `shutdown()`.
    let tmp_cwd = tempfile::tempdir().expect("failed to create temp cwd directory");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows,
    // window_index).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp_cwd.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Poll read_frame() until attached (cols > 0) ---------------------
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut frame = handle.read_frame();
    while Instant::now() < deadline {
        frame = handle.read_frame();
        if frame.cols() > 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        frame.cols() > 0,
        "core should attach and report a non-empty active frame (cols, got {})",
        frame.cols()
    );

    // --- 2. chdir into A, then B; wait for cwd() to follow each -------------
    // Two fresh temp target dirs, canonicalized so comparisons are robust to
    // macOS `/var` -> `/private/var` symlinking (as the model tests do).
    let dir_a = tempfile::tempdir().expect("failed to create temp chdir target A");
    let dir_b = tempfile::tempdir().expect("failed to create temp chdir target B");
    let canonical_a = dir_a
        .path()
        .canonicalize()
        .expect("failed to canonicalize chdir target A");
    let canonical_b = dir_b
        .path()
        .canonicalize()
        .expect("failed to canonicalize chdir target B");

    handle.chdir(dir_a.path().to_string_lossy().into_owned());
    wait_for_cwd(&handle, &canonical_a);

    handle.chdir(dir_b.path().to_string_lossy().into_owned());
    wait_for_cwd(&handle, &canonical_b);

    // --- 3. Poll recent_dirs() until it holds BOTH A and B -----------------
    // Recording is fire-and-forget (the MRU write can land a beat after cwd()
    // flips), so poll until the newest-first list contains both targets. Each
    // returned entry is canonicalize-compared. The list is GLOBAL and also holds
    // the launch cwd, so we assert membership/order, NOT exact length.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut recent: Vec<PathBuf> = Vec::new();
    let mut idx_a = None;
    let mut idx_b = None;
    while Instant::now() < deadline {
        recent = handle
            .recent_dirs()
            .iter()
            .map(|p| canonicalize(p))
            .collect();
        idx_a = recent.iter().position(|p| *p == canonical_a);
        idx_b = recent.iter().position(|p| *p == canonical_b);
        if idx_a.is_some() && idx_b.is_some() {
            break;
        }
        sleep(POLL_INTERVAL);
    }

    // --- 4. Assert: non-empty, newest-first, B before A, A present ---------
    assert!(
        !recent.is_empty(),
        "recent_dirs() should be non-empty after two chdirs, got {recent:?}"
    );
    let idx_a = idx_a.unwrap_or_else(|| {
        panic!("recent_dirs() should contain chdir target A ({canonical_a:?}), got {recent:?}")
    });
    let idx_b = idx_b.unwrap_or_else(|| {
        panic!("recent_dirs() should contain chdir target B ({canonical_b:?}), got {recent:?}")
    });
    assert_eq!(
        recent[0], canonical_b,
        "recent_dirs()[0] should be the newest chdir target B (newest-first), got {recent:?}"
    );
    assert!(
        idx_b < idx_a,
        "B ({canonical_b:?}, idx {idx_b}) should precede A ({canonical_a:?}, idx {idx_a}) \
         newest-first, got {recent:?}"
    );

    // --- 5. Shutdown: clean teardown ---------------------------------------
    handle.shutdown();

    std::env::remove_var("WOLF_STATE_DIR");
    drop(dir_a);
    drop(dir_b);
    drop(tmp_cwd);
    drop(state_dir);
}
