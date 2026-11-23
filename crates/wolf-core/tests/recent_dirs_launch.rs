//! Real-nvim test of `WolfCore::record_recent_dir`, the record a front-end makes when Open
//! Folder lands a tab on a directory (`specs/application-state.md` §6).
//!
//! A tab Open Folder fills or opens starts its Neovim in the folder, and `apply_cwd` never
//! records a launch directory, so without this call the folder would never reach the
//! recent-directories list. The test starts a core in `A`, proves the launch left `A`
//! unrecorded, then records `A` through a non-canonical spelling and expects canonical `A` on
//! top, through the in-process accessor and the on-disk table alike.
//!
//! A file of its own: it sets the process-global `WOLF_STATE_DIR` before the core starts, so it
//! must not share a binary with another test that sets it.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use wolf_core::RecentDirs;

#[test]
fn record_recent_dir_puts_the_canonical_launch_directory_on_top() {
    let _ = common::require_nvim_bin();

    // Pin the store to a fresh dir before the core opens it; the guard keeps it alive.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    let dir_a = tempfile::tempdir().expect("failed to create tempdir A");
    let cwd_a: PathBuf = fs::canonicalize(dir_a.path()).expect("failed to canonicalize A");
    fs::create_dir(cwd_a.join("sub")).expect("failed to create A/sub");

    let (core, waiter) = common::start_core_at(cwd_a.clone());

    // The startup `wolf_cwd` emit has been applied once the tracked cwd reads A.
    common::poll_until(
        &waiter,
        "live_cwd() to report the launch directory A; never means the startup `wolf_cwd` \
         notification did not reach apply_cwd",
        || core.live_cwd(),
        |cwd| cwd.as_deref() == Some(cwd_a.as_path()),
    );

    let before = core.recent_dirs();
    assert!(
        !before.iter().any(|p| p == &cwd_a),
        "precondition: the launch directory A must not be in the recent-dirs MRU before \
         record_recent_dir; if it is, apply_cwd records launch directories and this test \
         cannot tell record_recent_dir's record from that one. Got {before:?}"
    );

    // A spelling of A that only canonicalization turns back into A.
    let spelled = cwd_a.join("sub").join("..");
    core.record_recent_dir(&spelled);

    assert_top(
        &core.recent_dirs(),
        &cwd_a,
        &spelled,
        "core.recent_dirs() (the in-process accessor)",
    );
    assert_top(
        &RecentDirs::open(state_dir.path()).recent(),
        &cwd_a,
        &spelled,
        "RecentDirs::open(state_dir).recent() (the on-disk folder_mru row)",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Asserts that `want`, byte for byte, is rank 0 of `recent`, read through `via`.
fn assert_top(recent: &[PathBuf], want: &Path, spelled: &Path, via: &str) {
    let top = recent.first();
    assert!(
        top.is_some_and(|p| p == want),
        "{via}: expected the canonical launch directory {} at rank 0 after \
         record_recent_dir({}). An empty list means record_recent_dir recorded nothing; \
         {} at rank 0 means it stored the path as given, without canonicalizing. Got {recent:?}",
        want.display(),
        spelled.display(),
        spelled.display(),
    );
}
