//! Real-nvim end-to-end test of the EMPTY-STATE launch contract (launch-behavior
//! plan §2 + §3): the captive Neovim's launch/initial working directory must NOT
//! be persisted into `state.json`. The one-shot startup `wolf_cwd` emit updates
//! the tracked-cwd cell ONLY — it neither records into the recent-dirs MRU nor
//! mirrors into the attached `StateStore`; and `attach_state_store` no longer
//! pushes the tracked cwd on attach. So an empty-state window persists `cwd == ""`
//! (the "no folder open" sentinel), NOT the process/launch dir. A genuine later
//! `:cd` (a real `DirChanged`) still records + persists as before.
//!
//! This drives a REAL, hermetic Neovim process (no mocks) and requires
//! `WOLF_NVIM_BIN` (the shared harness fails loudly if it is unset). Modeled
//! CLOSELY on `tests/state_save.rs`, reusing `tests/common/mod.rs`.
//!
//! Like `state_save.rs`, the state dir is an explicit tempdir passed to
//! `StateStore::open`, so `WOLF_STATE_DIR` is never touched here.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wolf_core::{load_state, Key, StateStore, WolfCore};

/// The cwd/geometry saves ride the async `wolf_cwd` notification plus the store's
/// ~100ms write-coalesce delay, so the file trails the visible redraw flush.
/// Poll rather than guess a sleep. Mirrors `state_save.rs`'s poll budget.
const STATE_POLL_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll cadence while waiting for `state.json` / `live_cwd()` to settle.
const STATE_POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Generous flush bound for the attached `StateStore` (well beyond the ~100ms
/// coalesce delay), so a timeout always means a real bug.
const STORE_FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

// -- Distinctive window-0 geometry the "front-end" pushes so window 0 EXISTS in
//    state.json even though nothing writes its cwd (named per the mnd rule). ----
/// Pushed window-0 x.
const GEO_X: f64 = 120.0;
/// Pushed window-0 y.
const GEO_Y: f64 = 64.0;
/// Pushed window-0 width.
const GEO_WIDTH: f64 = 1100.0;
/// Pushed window-0 height.
const GEO_HEIGHT: f64 = 720.0;

#[test]
fn launch_cwd_is_not_persisted_but_real_cd_is() {
    // Fail loud if the real nvim binary is missing.
    let _ = common::require_nvim_bin();

    // Start nvim in tempdir X (the launch/initial cwd), retaining the waiter so
    // we can confirm the `:cd` keystrokes were processed before polling.
    let (_dir_x, cwd_x) = temp_cwd();
    let (core, waiter) = start_core_in_with_waiter(&cwd_x);

    // The state file lives in its OWN tempdir (the explicit injection seam). Keep
    // a clone so this test can play the front-end: push geometry and force a
    // flush. Both handles share one inner store, exactly like the real FFI glue.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    let store = StateStore::open(state_dir.path());
    let front_end = store.clone();
    core.attach_state_store(store, 0);

    // The front-end reports window geometry; this is what CREATES window 0 in
    // state.json. Crucially, it does NOT touch cwd — so window 0's cwd stays at
    // its default `""` UNLESS something erroneously persists the launch cwd.
    front_end.set_geometry(0, GEO_X, GEO_Y, GEO_WIDTH, GEO_HEIGHT);

    // Wait until the captive nvim's startup emit has been PROCESSED (tracked cwd
    // == X). This closes the false-pass race: only after the initial emit is
    // known-handled can we be sure that a regression (initial emit mirrored into
    // the store, or attach pushing the tracked cwd) would already have written X.
    wait_for_live_cwd(&core, &cwd_x);

    // Force the pending geometry write out, then assert the PERSISTED window 0.
    assert!(
        front_end.flush(STORE_FLUSH_TIMEOUT),
        "state store flush after the geometry push should complete within {STORE_FLUSH_TIMEOUT:?}"
    );
    let persisted = load_state(state_dir.path())
        .expect("state.json should exist after the geometry push");
    assert_eq!(
        persisted.windows.len(),
        1,
        "the front-end geometry push must have created exactly window 0, got {:?}",
        persisted.windows
    );
    let w0 = &persisted.windows[0];
    assert_eq!(w0.x, GEO_X, "window 0 geometry x should be persisted");
    assert_eq!(w0.width, GEO_WIDTH, "window 0 geometry width should be persisted");
    // THE load-bearing assertion: the launch/initial cwd X was NOT persisted.
    assert_eq!(
        w0.cwd, "",
        "an empty-state window must persist cwd == \"\" (the no-folder sentinel); \
         the launch cwd {} must NOT be written, got {:?}",
        cwd_x.display(),
        w0.cwd
    );

    // Now a GENUINE user `:cd <Y>` (a real DirChanged) MUST record + persist,
    // proving the suppression is scoped to the one-shot startup emit only.
    let (_dir_y, cwd_y) = temp_cwd();
    let path_y = cwd_y.to_str().expect("temp path Y should be valid UTF-8");
    core.send_input(common::ch(':'));
    common::type_str(&core, "cd ");
    common::type_str(&core, path_y);
    core.send_input(common::key(Key::Enter));

    // Wait for nvim to process the `:cd` and flush, then poll the file until the
    // DirChanged -> wolf_cwd -> apply_cwd -> StateStore hook has persisted Y.
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);
    wait_for_state_cwd(state_dir.path(), &cwd_y);

    core.shutdown().expect("shutdown should be Ok");
}

/// Polls `core.live_cwd()` until it equals `want` (canonicalize-compared), or
/// panics after a timeout. Proves the captive nvim's startup `wolf_cwd` emit has
/// been processed into the tracked-cwd cell.
fn wait_for_live_cwd(core: &WolfCore, want: &Path) {
    let deadline = Instant::now() + STATE_POLL_TIMEOUT;
    loop {
        if let Some(cwd) = core.live_cwd() {
            if paths_equal(&cwd, want) {
                return;
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {STATE_POLL_TIMEOUT:?} waiting for live_cwd() to become {} \
                 (last seen: {:?})",
                want.display(),
                core.live_cwd()
            );
        }
        std::thread::sleep(STATE_POLL_INTERVAL);
    }
}

/// Polls `load_state(state_dir)` until window 0's cwd equals `want` (compared
/// canonicalized, robust to the macOS `/var` -> `/private/var` symlink), or
/// panics after a timeout with the last-seen value. Mirrors
/// `state_save.rs::wait_for_state_cwd`.
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
/// equality. Mirrors `state_save.rs::paths_equal`.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// Creates a fresh temp dir to serve as a core `cwd`, returning both the guard
/// (kept alive by the caller) and the CANONICAL path. Mirrors
/// `state_save.rs::temp_cwd`.
fn temp_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// Starts a `WolfCore` rooted at `cwd` and returns it together with the
/// `FlushWaiter`. Replicated from `state_save.rs`.
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
