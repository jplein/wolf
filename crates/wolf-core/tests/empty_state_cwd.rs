//! Real-nvim end-to-end test of the EMPTY-STATE launch contract's
//! recent-directories (folder-MRU) hygiene.
//!
//! The captive nvim's one-shot startup `wolf_cwd` emit updates the tracked-cwd
//! cell ONLY — it must NOT record the launch/initial directory into the
//! recent-dirs MRU (`<state-dir>/folder-mru.sqlite`). A genuine later `:cd` (a
//! real `DirChanged`) does record into the MRU. `wolf-core` does not persist cwd
//! into a `StateStore`; the front-end reads it from each controller at snapshot
//! time.
//!
//! This drives a REAL, hermetic Neovim process (no mocks) and requires
//! `WOLF_NVIM_BIN` (the shared harness fails loudly if it is unset). Modeled
//! CLOSELY on `tests/recent_dirs_records.rs`, reusing `tests/common/mod.rs`.
//!
//! The always-on recent-dirs store reads `state_dir()`, so this test sets
//! `WOLF_STATE_DIR` to a fresh tempdir BEFORE starting the core and inspects the
//! physical `folder-mru.sqlite` under that dir.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wolf_core::{Key, RecentDirs, WolfCore};

/// The cwd record rides the async `wolf_cwd` notification through `apply_cwd`, so
/// the recorded row trails the visible redraw flush slightly. Poll rather than
/// guess a sleep. Mirrors `recent_dirs_records.rs`'s poll budget.
const RECENT_POLL_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll cadence while waiting for the recent-dirs MRU to reflect a `:cd`.
const RECENT_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[test]
fn launch_cwd_is_not_recorded_but_real_cd_is() {
    // Fail loud if the real nvim binary is missing.
    let _ = common::require_nvim_bin();

    // The recent-dirs store reads state_dir() eagerly: pin it to a fresh tempdir before the core
    // starts, so folder-mru.sqlite is inspectable and never the user's. Keep the guard alive.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    // Start nvim in tempdir X (the launch/initial cwd), retaining the waiter so we
    // can confirm the `:cd` keystrokes were processed before polling.
    let (_dir_x, cwd_x) = temp_cwd();
    let (core, waiter) = start_core_in_with_waiter(&cwd_x);

    // The startup `wolf_cwd` emit updates only the tracked-cwd cell (`apply_cwd` skips the initial
    // emit's record), so the hermetic MRU must be empty before any `:cd`.
    let before_any_cd = core.recent_dirs();
    assert!(
        before_any_cd.is_empty(),
        "the launch cwd must NOT be recorded into the recent-dirs MRU; \
         expected an empty MRU before any `:cd`, got {before_any_cd:?}"
    );

    // Now a GENUINE user `:cd <Y>` (a real DirChanged) MUST record into the MRU,
    // proving the suppression is scoped to the one-shot startup emit only.
    let (_dir_y, cwd_y) = temp_cwd();
    let path_y = cwd_y.to_str().expect("temp path Y should be valid UTF-8");
    core.send_input(common::ch(':'));
    common::type_str(&core, "cd ");
    common::type_str(&core, path_y);
    core.send_input(common::key(Key::Enter));

    // The launch cwd X is never recorded, so rank 0 must be the user-typed Y.
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);
    wait_for_recent_top(&core, state_dir.path(), &cwd_y);

    // By the time Y has landed, the setup-time initial emit has definitively been
    // processed; X must still be absent from the MRU (never recorded at all).
    let after_cd_y = core.recent_dirs();
    assert!(
        !after_cd_y.iter().any(|p| paths_equal(p, &cwd_x)),
        "the launch cwd X must never appear in the recent-dirs MRU, got {after_cd_y:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
    std::env::remove_var("WOLF_STATE_DIR");
}

/// Polls until `want` is the NEWEST (rank 0) entry via BOTH read paths, asserting
/// they agree: `core.recent_dirs()` (the in-process accessor) AND a fresh
/// `RecentDirs::open(state_dir).recent()` (the physical on-disk row). Panics after
/// a timeout with the last-seen values. Mirrors `recent_dirs_records.rs`.
fn wait_for_recent_top(core: &WolfCore, state_dir: &Path, want: &Path) {
    let deadline = Instant::now() + RECENT_POLL_TIMEOUT;
    loop {
        let via_core = core.recent_dirs();
        let via_disk = RecentDirs::open(state_dir).recent();

        let core_top = via_core.first().is_some_and(|p| paths_equal(p, want));
        let disk_top = via_disk.first().is_some_and(|p| paths_equal(p, want));
        if core_top && disk_top {
            return;
        }

        if Instant::now() >= deadline {
            panic!(
                "timed out after {RECENT_POLL_TIMEOUT:?} waiting for {} to be rank 0 \
                 of the recent-dirs MRU; core.recent_dirs() = {via_core:?}, \
                 on-disk recent() = {via_disk:?}",
                want.display()
            );
        }
        std::thread::sleep(RECENT_POLL_INTERVAL);
    }
}

/// Compares two paths by their canonical form when resolvable, else raw
/// equality. Mirrors `recent_dirs_records.rs::paths_equal`.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// Creates a fresh temp dir to serve as a core `cwd`, returning both the guard
/// (kept alive by the caller) and the CANONICAL path. Mirrors
/// `recent_dirs_records.rs::temp_cwd`.
fn temp_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// Starts a `WolfCore` rooted at `cwd` and returns it together with the
/// `FlushWaiter`. Replicated from `recent_dirs_records.rs`.
fn start_core_in_with_waiter(cwd: &Path) -> (WolfCore, common::FlushWaiter) {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(
            cwd.to_path_buf(),
            common::DEFAULT_COLS,
            common::DEFAULT_ROWS,
        ),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    // The attach flush (seq 0 -> 1) already landed during WolfCore::start, so use
    // the absolute-threshold initial-flush wait (an advance-based wait would hang).
    waiter.wait_for_initial_flush(&core, common::FLUSH_TIMEOUT);
    (core, waiter)
}
