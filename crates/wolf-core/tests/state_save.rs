//! Real-nvim end-to-end test of the save-on-cwd-change flow (plan §4): with a
//! `StateStore` attached to a live `WolfCore`, the core itself must persist
//! nvim's working directory into `<state-dir>/state.json` — both the setup-time
//! `wolf_cwd` emission (covered by the attach-time push of `tracked_cwd`, risk
//! note 3) and a later `:cd` typed through the normal input path.
//!
//! This drives a REAL, hermetic Neovim process (no mocks) and requires
//! `WOLF_NVIM_BIN` (the shared harness fails loudly if it is unset). Modeled on
//! `tests/mru_open_records.rs`, reusing `tests/common/mod.rs`.
//!
//! No env vars are needed for the state machinery itself: the state dir is an
//! explicit tempdir passed to `StateStore::open`, so `WOLF_STATE_DIR` is never
//! touched here.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wolf_core::state::STATE_FILE_NAME;
use wolf_core::{load_state, Key, StateStore, WolfCore};

/// The cwd save rides the async `wolf_cwd` notification plus the store's
/// ~100ms write-coalesce delay, so the file trails the visible redraw flush.
/// Poll rather than guess a sleep.
const STATE_POLL_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll cadence while waiting for `state.json` to reflect a cwd.
const STATE_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[test]
fn cwd_changes_persist_to_state_json() {
    // Fail loud if the real nvim binary is missing.
    let _ = common::require_nvim_bin();

    // Start nvim in tempdir A, retaining the waiter so we can confirm the `:cd`
    // keystrokes were processed before polling for their persisted effect.
    let (_dir_a, cwd_a) = temp_cwd();
    let (core, waiter) = start_core_in_with_waiter(&cwd_a);

    // The state file lives in its OWN tempdir B (the explicit injection seam).
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    let store = StateStore::open(state_dir.path());
    core.attach_state_store(store);

    // The setup-time `wolf_cwd` emission may have raced ahead of the attach;
    // the attach-time push of `tracked_cwd` must close that race (risk note 3),
    // so window 0's cwd settles to A without any further input.
    wait_for_state_cwd(state_dir.path(), &cwd_a);

    // Drive `:cd <C>` one key at a time through the same input path the front
    // ends use, mirroring mru_open_records' `:cd` sequence.
    let (_dir_c, cwd_c) = temp_cwd();
    let path_c = cwd_c.to_str().expect("temp path C should be valid UTF-8");
    core.send_input(common::ch(':'));
    common::type_str(&core, "cd ");
    common::type_str(&core, path_c);
    core.send_input(common::key(Key::Enter));

    // Wait for nvim to process the `:cd` and flush, then poll the file: the
    // DirChanged -> wolf_cwd -> apply_cwd -> StateStore hook must persist C.
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);
    wait_for_state_cwd(state_dir.path(), &cwd_c);

    // The on-disk form is pretty-printed version-1 JSON.
    let text = fs::read_to_string(state_dir.path().join(STATE_FILE_NAME))
        .expect("state.json should exist after the cwd saves");
    assert!(
        text.contains("\"version\": 1"),
        "state.json must carry a `\"version\": 1` header, got:\n{text}"
    );
    assert!(
        text.contains("\n  "),
        "state.json must be pretty-printed (newline + indentation), got:\n{text}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Polls `load_state(state_dir)` until window 0's cwd equals `want` (compared
/// canonicalized, robust to the macOS `/var` -> `/private/var` symlink), or
/// panics after a timeout with the last-seen value.
fn wait_for_state_cwd(state_dir: &Path, want: &Path) {
    let deadline = Instant::now() + STATE_POLL_TIMEOUT;
    loop {
        let cwd = load_state(state_dir)
            .and_then(|s| s.windows.first().map(|w| w.cwd.clone()))
            .unwrap_or_default();
        if !cwd.is_empty() && paths_equal(Path::new(&cwd), want) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {STATE_POLL_TIMEOUT:?} waiting for state.json window 0 \
                 cwd to become {} (last seen: {cwd:?})",
                want.display()
            );
        }
        std::thread::sleep(STATE_POLL_INTERVAL);
    }
}

/// Compares two paths by their canonical form when resolvable, else raw
/// equality. Mirrors `mru_open_records.rs::paths_equal`.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// Creates a fresh temp dir to serve as a core `cwd`, returning both the guard
/// (kept alive by the caller) and the CANONICAL path. Mirrors
/// `mru_open_records.rs::temp_cwd`: temp dirs can sit behind symlinks (macOS
/// `/tmp -> /private/tmp`) and nvim reports resolved paths, so comparing
/// against an already-canonical cwd keeps the assertions stable.
fn temp_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// Starts a `WolfCore` rooted at `cwd` and returns it together with the
/// `FlushWaiter`, so the test can drive a `:cd` via keystrokes and wait for
/// nvim to process it. Replicated from `mru_open_records.rs` (the same
/// `wake_channel`/`config_in`/`WolfCore::start` parts).
fn start_core_in_with_waiter(cwd: &Path) -> (WolfCore, common::FlushWaiter) {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(cwd.to_path_buf(), common::DEFAULT_COLS, common::DEFAULT_ROWS),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    // The attach flush (seq 0 -> 1) already landed during WolfCore::start, so use
    // the absolute-threshold initial-flush wait (an advance-based wait would hang).
    waiter.wait_for_initial_flush(&core, common::FLUSH_TIMEOUT);
    (core, waiter)
}
