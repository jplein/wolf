//! Real-nvim end-to-end test of the record-on-`:cd` path for the GLOBAL
//! recent-directories MRU: a `:cd <dir>` typed into the embedded nvim must flow
//! through `apply_cwd` and land `<dir>` at the TOP of the always-on recent-dirs
//! store (`<state-dir>/folder-mru.sqlite`).
//!
//! This drives a REAL, hermetic Neovim process (no mocks) and requires
//! `WOLF_NVIM_BIN` (the shared harness fails loudly if it is unset). Modeled on
//! `tests/state_save.rs` / `tests/mru_open_records.rs`, reusing
//! `tests/common/mod.rs`.
//!
//! Unlike `state_save.rs` (which injects an explicit `StateStore` into its own
//! tempdir), the recent-dirs store is ALWAYS-ON and reads `state_dir()`. So this
//! test sets `WOLF_STATE_DIR` to a fresh tempdir BEFORE starting the core (the
//! set-if-absent hermetic default in `common/mod.rs` then leaves it alone), and
//! inspects the physical `folder-mru.sqlite` under that dir.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wolf_core::{Key, RecentDirs, WolfCore};

/// The cwd record rides the async `wolf_cwd` notification through `apply_cwd`, so
/// the recorded row trails the visible redraw flush slightly. Poll rather than
/// guess a sleep. Mirrors `state_save.rs`'s poll budget.
const RECENT_POLL_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll cadence while waiting for the recent-dirs MRU to reflect a `:cd`.
const RECENT_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[test]
fn cd_records_directory_at_top_of_recent_dirs() {
    // Fail loud if the real nvim binary is missing.
    let _ = common::require_nvim_bin();

    // The always-on recent-dirs store reads state_dir(); pin it to a fresh
    // tempdir BEFORE starting the core so folder-mru.sqlite lands somewhere we can
    // inspect (and never in the user's real state dir). Keep the guard alive.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    // Start nvim in tempdir A, retaining the waiter so we can confirm the `:cd`
    // keystrokes were processed before polling for their persisted effect.
    let (_dir_a, cwd_a) = temp_cwd();
    let (core, waiter) = start_core_in_with_waiter(&cwd_a);

    // MRU-hygiene regression guard: the launch/initial cwd A is deliberately NOT
    // recorded into the recent-dirs MRU. Only directories the user actively
    // opens (a typed `:cd`, or the Open-Folder command) should populate the MRU.
    // The captive nvim's child-workdir launch fires the one-shot startup
    // `wolf_cwd` emit, which now updates the tracked-cwd cell ONLY (it is flagged
    // as the initial emit and skipped by `apply_cwd`'s record/store path). This
    // test runs against a fresh, hermetic WOLF_STATE_DIR, so before any `:cd` the
    // MRU must be EMPTY (and in particular must not contain A).
    let before_any_cd = core.recent_dirs();
    assert!(
        before_any_cd.is_empty(),
        "the launch cwd must NOT be recorded into the recent-dirs MRU; \
         expected an empty MRU before any `:cd`, got {before_any_cd:?}"
    );

    // Drive `:cd <B>` one key at a time through the same input path the front
    // ends use, mirroring state_save.rs' `:cd` sequence.
    let (_dir_b, cwd_b) = temp_cwd();
    let path_b = cwd_b.to_str().expect("temp path B should be valid UTF-8");
    core.send_input(common::ch(':'));
    common::type_str(&core, "cd ");
    common::type_str(&core, path_b);
    core.send_input(common::key(Key::Enter));

    // Wait for nvim to process the `:cd` and flush, then poll BOTH read paths:
    // the launch cwd A is NEVER recorded (see the pre-`:cd` guard above), so the
    // FIRST recorded entry — and thus rank 0 — must be the user-typed B.
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);
    wait_for_recent_top(&core, state_dir.path(), &cwd_b);

    // By the time B has landed, the setup-time initial emit has definitively been
    // processed; A must still be absent from the MRU (it is not merely ranked
    // below B — it was never recorded at all).
    let after_cd_b = core.recent_dirs();
    assert!(
        !after_cd_b.iter().any(|p| paths_equal(p, &cwd_a)),
        "the launch cwd A must never appear in the recent-dirs MRU, got {after_cd_b:?}"
    );

    // Optional move-to-top: `:cd <C>` makes C rank 0 and B drops to rank 1.
    let (_dir_c, cwd_c) = temp_cwd();
    let path_c = cwd_c.to_str().expect("temp path C should be valid UTF-8");
    core.send_input(common::ch(':'));
    common::type_str(&core, "cd ");
    common::type_str(&core, path_c);
    core.send_input(common::key(Key::Enter));

    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);
    wait_for_recent_top(&core, state_dir.path(), &cwd_c);

    // Once C is on top, B must have dropped to rank 1 (moved down, not gone).
    let recent = core.recent_dirs();
    assert!(
        recent.len() >= 2,
        "recent_dirs() should hold at least B and C; got {:?}",
        recent
    );
    assert!(
        paths_equal(&recent[1], &cwd_b),
        "after cd C, B should be rank 1; got {:?}",
        recent
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Polls until `want` is the NEWEST (rank 0) entry via BOTH read paths, asserting
/// they agree: `core.recent_dirs()` (the in-process accessor) AND a fresh
/// `RecentDirs::open(state_dir).recent()` (the physical on-disk row). Panics after
/// a timeout with the last-seen values. Follows `state_save.rs::wait_for_state_cwd`.
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
/// equality. Mirrors `state_save.rs::paths_equal`.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// Creates a fresh temp dir to serve as a core `cwd`, returning both the guard
/// (kept alive by the caller) and the CANONICAL path. Mirrors
/// `state_save.rs::temp_cwd`: temp dirs can sit behind symlinks (macOS
/// `/tmp -> /private/tmp`) and nvim reports resolved paths, so comparing against
/// an already-canonical cwd keeps the assertions stable.
fn temp_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// Starts a `WolfCore` rooted at `cwd` and returns it together with the
/// `FlushWaiter`, so the test can drive a `:cd` via keystrokes and wait for nvim
/// to process it. Replicated from `state_save.rs` (the same
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
