//! Integration tests for the S2 async streaming traversal + the `FileSearch`
//! session (`wolf_core::FileSearch`, `WolfCore::start_file_search`).
//!
//! These exercise REAL filesystem trees (via `tempfile`) and, for the git cases,
//! the REAL `git` binary (shelled out with `std::process::Command`) — no mocks,
//! consistent with the project's real-tool / real-nvim policy.
//!
//! `start_file_search` lives on a `WolfCore`, which spawns a REAL, hermetic
//! Neovim located via `WOLF_NVIM_BIN`; so these are also real-nvim integration
//! tests and fail loudly (via `common::require_nvim_bin`) if it is unset.
//!
//! Surface exercised (per plan §D3/§D4/§D6):
//! - `WolfCore::start_file_search(&self, wake: WakeFn) -> FileSearch`
//!   (SYNCHRONOUSLY seeds MRU results, then streams traversal results, bumping
//!   `version` + waking per batch, and finally sets `complete = true`).
//! - `FileSearch::read()` -> an owned guard that `Deref`s to a view exposing
//!   `entries`, `complete: bool`, and `version: u64`.
//! - `FileSearch::cancel(&self)` (and `Drop`) stop the traversal.
//!
//! NOTE to coder (D4 view shape): the tests follow §D4 literally and access the
//! read guard's fields as `view.entries`, `view.complete`, `view.version`. They
//! iterate `view.entries` by reference and `.clone()` out of it, so they work
//! whether the guard derefs to a `FileSearchView`/`FileSearchState` holding
//! `entries: Vec<FileEntry>` or a borrowed `&[FileEntry]`. If the field/accessor
//! names differ, align them with §D4 (`entries` / `complete` / `version`).

mod common;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use wolf_core::{FileEntry, FileSearch, MruStore, WakeFn, WolfCore};

/// Upper bound for a single file search to reach `complete`. Generous because it
/// includes nvim startup amortized across the test plus a full FS/git walk.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to block on a wake before re-checking `complete` (so we make
/// progress even if a wake is missed/coalesced).
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Settle window used by the cancel test: time to let any in-flight batch land,
/// and the window over which we assert the version has stopped advancing.
const CANCEL_SETTLE: Duration = Duration::from_millis(500);

/// Number of files the cancel test creates. Comfortably exceeds the traversal
/// batch size (plan: `TRAVERSAL_BATCH = 256`) so cancellation has multiple
/// batches to interrupt.
const MANY_FILES: usize = 600;

// --- Helpers ---------------------------------------------------------------

/// Creates a fresh temp dir to serve as a core `cwd` and returns both the guard
/// (kept alive by the caller) and the CANONICAL path to it.
///
/// The path is canonicalized because temp dirs can sit behind symlinks (e.g.
/// macOS `/tmp -> /private/tmp`); the MRU cache canonicalizes the cwd internally
/// and `record()` canonicalizes file paths, so passing an already-canonical cwd
/// keeps the traversal root, the MRU-stored paths, and the relative `display`
/// strings all consistent (and makes MRU/traversal dedup line up).
fn temp_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// Writes `contents` to `path`, creating parent directories as needed.
fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create parent dir");
    }
    fs::write(path, contents).expect("failed to write file");
}

/// Runs `git <args>` in `cwd`, asserting success.
fn run_git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .expect("failed to spawn git (is git available in the dev shell?)");
    assert!(status.success(), "`git {args:?}` failed with {status:?}");
}

/// Builds a `WakeFn` that signals on a channel, paired with the receiver the
/// poll loop waits on. Mirrors `common::wake_channel`, kept local because this
/// receiver is consumed by [`poll_until_complete`] (not the panic-on-timeout
/// `FlushWaiter`).
fn search_wake() -> (WakeFn, Receiver<()>) {
    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let wake: WakeFn = Arc::new(move || {
        // A send error just means the receiver was dropped (test finished).
        let _ = tx.send(());
    });
    (wake, rx)
}

/// Starts a `WolfCore` rooted at `cwd` (default grid size) and waits for the
/// initial attach/flush so startup never races the test body.
fn start_core_in(cwd: &Path) -> WolfCore {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(cwd.to_path_buf(), common::DEFAULT_COLS, common::DEFAULT_ROWS),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    // This is the attach wait directly after start with no intervening input.
    // The attach flush (seq 0 -> 1) already landed during WolfCore::start, so use
    // the absolute-threshold initial-flush wait (an advance-based wait would hang).
    waiter.wait_for_initial_flush(&core, common::FLUSH_TIMEOUT);
    core
}

/// Polls `search` until its view reports `complete`, waking on `wakes`, then
/// returns a snapshot of the collected entries. Panics on timeout.
fn poll_until_complete(search: &FileSearch, wakes: &Receiver<()>) -> Vec<FileEntry> {
    let deadline = Instant::now() + SEARCH_TIMEOUT;
    loop {
        {
            let view = search.read();
            if view.complete {
                return view.entries.to_vec();
            }
        }
        if Instant::now() >= deadline {
            panic!("file search did not reach `complete` within {SEARCH_TIMEOUT:?}");
        }
        // Bounded wait: a wake nudges us promptly; the timeout guarantees we
        // re-check even if a wake was coalesced/missed.
        let _ = wakes.recv_timeout(POLL_INTERVAL);
    }
}

/// The set of `display` strings of `entries` (traversal order is nondeterministic,
/// so tests assert on the SET rather than a sequence).
fn displays(entries: &[FileEntry]) -> HashSet<String> {
    entries.iter().map(|e| e.display.clone()).collect()
}

/// A `display` for a nested path, built with the platform separator so the
/// assertions match the relativized `display` strings on every OS.
fn rel(parts: &[&str]) -> String {
    let mut p = PathBuf::new();
    for part in parts {
        p.push(part);
    }
    p.to_string_lossy().into_owned()
}

/// True if any entry is the directory `top` itself or lives beneath it.
fn any_under(entries: &[FileEntry], top: &str) -> bool {
    let prefix = format!("{top}{}", std::path::MAIN_SEPARATOR);
    entries
        .iter()
        .any(|e| e.display == top || e.display.starts_with(&prefix))
}

// --- Tests -----------------------------------------------------------------

/// (1) Non-repo: plain files + dotfiles are listed (with nesting); directories
/// (including empty ones) are NOT listed.
#[test]
fn non_repo_lists_plain_files_and_dotfiles_but_no_dirs() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    write_file(&cwd.join("a.md"), "a");
    write_file(&cwd.join(".hidden.md"), "h");
    write_file(&cwd.join("sub").join("b.md"), "b");
    fs::create_dir_all(cwd.join("emptydir")).expect("failed to create emptydir");

    let core = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let entries = poll_until_complete(&search, &wakes);
    let set = displays(&entries);

    assert!(set.contains("a.md"), "plain file should be listed; got {set:?}");
    assert!(
        set.contains(".hidden.md"),
        "dotfiles should be listed (only .gitignore filters); got {set:?}"
    );
    assert!(
        set.contains(&rel(&["sub", "b.md"])),
        "nested plain file should be listed; got {set:?}"
    );

    assert!(
        !set.contains("sub"),
        "directories must NOT be listed; got {set:?}"
    );
    assert!(
        !set.contains("emptydir"),
        "empty directories must NOT be listed; got {set:?}"
    );
    assert!(
        !any_under(&entries, ".wolf"),
        "the .wolf cache dir must be excluded; got {set:?}"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (2) Symlinks: include a symlink to a plain file; exclude symlinks to a
/// directory and broken symlinks.
#[cfg(unix)]
#[test]
fn symlinks_only_to_plain_files_are_included() {
    use std::os::unix::fs::symlink;

    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    write_file(&cwd.join("target.md"), "t");
    fs::create_dir_all(cwd.join("sub")).expect("failed to create sub");

    symlink(cwd.join("target.md"), cwd.join("link_to_file")).expect("symlink -> file");
    symlink(cwd.join("sub"), cwd.join("link_to_dir")).expect("symlink -> dir");
    symlink(cwd.join("nonexistent"), cwd.join("link_broken")).expect("broken symlink");

    let core = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let entries = poll_until_complete(&search, &wakes);
    let set = displays(&entries);

    assert!(set.contains("target.md"), "the real target should be listed; got {set:?}");
    assert!(
        set.contains("link_to_file"),
        "a symlink targeting a plain file must be included; got {set:?}"
    );
    assert!(
        !set.contains("link_to_dir"),
        "a symlink targeting a directory must be excluded; got {set:?}"
    );
    assert!(
        !set.contains("link_broken"),
        "a broken symlink must be excluded; got {set:?}"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (3) A symlink cycle must not hang: the search still reaches `complete`.
#[cfg(unix)]
#[test]
fn symlink_loop_does_not_hang() {
    use std::os::unix::fs::symlink;

    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    let loopdir = cwd.join("loopdir");
    fs::create_dir_all(&loopdir).expect("failed to create loopdir");
    write_file(&loopdir.join("real.md"), "r");
    // A symlink inside loopdir pointing back at loopdir: a directory cycle.
    symlink(&loopdir, loopdir.join("loop")).expect("loop symlink");

    let core = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);

    // The load-bearing assertion is simply that this RETURNS within the timeout
    // (no infinite recursion). We also confirm the real nested file is found.
    let entries = poll_until_complete(&search, &wakes);
    let set = displays(&entries);
    assert!(
        set.contains(&rel(&["loopdir", "real.md"])),
        "the real file under loopdir should still be listed; got {set:?}"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (4) Git repo: respect .gitignore, include untracked-non-ignored files, never
/// list anything under .git/ (or .wolf/).
#[test]
fn git_repo_respects_gitignore_and_excludes_git_dir() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    run_git(&cwd, &["init"]);
    write_file(&cwd.join(".gitignore"), "ignored.md\nignored_dir/\n");
    write_file(&cwd.join("ignored.md"), "x");
    write_file(&cwd.join("ignored_dir").join("x.md"), "x");
    write_file(&cwd.join("tracked.md"), "t");
    write_file(&cwd.join("untracked.md"), "u");
    // Track one file; commit is intentionally skipped (not needed for `ignore`).
    run_git(&cwd, &["add", "tracked.md"]);

    let core = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let entries = poll_until_complete(&search, &wakes);
    let set = displays(&entries);

    assert!(set.contains("tracked.md"), "tracked file should be listed; got {set:?}");
    assert!(
        set.contains("untracked.md"),
        "untracked-but-not-ignored file should be listed; got {set:?}"
    );
    assert!(
        set.contains(".gitignore"),
        ".gitignore itself is not ignored and should be listed; got {set:?}"
    );

    assert!(
        !set.contains("ignored.md"),
        "a gitignored file must be excluded; got {set:?}"
    );
    assert!(
        !set.contains(&rel(&["ignored_dir", "x.md"])),
        "files under a gitignored directory must be excluded; got {set:?}"
    );

    assert!(
        !any_under(&entries, ".git"),
        "nothing under .git/ may appear; got {set:?}"
    );
    assert!(
        !any_under(&entries, ".wolf"),
        "nothing under .wolf/ may appear; got {set:?}"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (5) An ancestor .gitignore (here the repo root, which is the cwd) is honored
/// down into nested subdirectories.
#[test]
fn gitignore_applies_to_nested_subdirs() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    run_git(&cwd, &["init"]);
    write_file(&cwd.join(".gitignore"), "*.log\n");
    write_file(&cwd.join("sub").join("skip.log"), "l");
    write_file(&cwd.join("sub").join("keep.md"), "k");

    let core = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let entries = poll_until_complete(&search, &wakes);
    let set = displays(&entries);

    assert!(
        set.contains(&rel(&["sub", "keep.md"])),
        "a non-ignored nested file should be listed; got {set:?}"
    );
    assert!(
        !set.contains(&rel(&["sub", "skip.log"])),
        "a root .gitignore rule must apply down into subdirs; got {set:?}"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (6) The `.wolf` cache dir (created by the core opening the MRU cache) is
/// excluded from traversal.
#[test]
fn wolf_cache_dir_is_excluded() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("real.md"), "r");

    // start_core_in opens the MRU cache, creating <cwd>/.wolf/mru.sqlite.
    let core = start_core_in(&cwd);
    assert!(
        cwd.join(".wolf").exists(),
        "opening the core should have created the .wolf cache dir"
    );

    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let entries = poll_until_complete(&search, &wakes);
    let set = displays(&entries);

    assert!(set.contains("real.md"), "the real file should be listed; got {set:?}");
    assert!(
        !any_under(&entries, ".wolf"),
        "no entry under .wolf/ may appear; got {set:?}"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (7) The MRU seed is present SYNCHRONOUSLY (before `complete`), and an MRU file
/// rediscovered by traversal is listed exactly once, keeping its `mru_rank`.
#[test]
fn mru_seed_is_synchronous_and_not_duplicated() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    write_file(&cwd.join("seeded.md"), "s");
    write_file(&cwd.join("other.md"), "o");

    // Record seeded.md into the MRU BEFORE starting the core/search, via a direct
    // MruStore on the same cwd (same on-disk sqlite the core will then open).
    {
        let store = MruStore::open(&cwd).expect("MruStore::open should succeed");
        store.record(&cwd.join("seeded.md"));
    }

    let core = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);

    // SYNCHRONOUS seed: immediately after start (traversal may or may not have
    // finished yet), the MRU entry must already be present with an mru_rank.
    {
        let view = search.read();
        let seeded: Vec<&FileEntry> =
            view.entries.iter().filter(|e| e.display == "seeded.md").collect();
        assert_eq!(
            seeded.len(),
            1,
            "the MRU seed must be present synchronously right after start; entries={:?}",
            view.entries.iter().map(|e| e.display.clone()).collect::<Vec<_>>()
        );
        assert!(
            seeded[0].mru_rank.is_some(),
            "the seeded entry must carry an mru_rank (Some)"
        );
    }

    let entries = poll_until_complete(&search, &wakes);

    let seeded: Vec<&FileEntry> = entries.iter().filter(|e| e.display == "seeded.md").collect();
    assert_eq!(
        seeded.len(),
        1,
        "an MRU file rediscovered by traversal must be listed exactly once; got {:?}",
        displays(&entries)
    );
    assert!(
        seeded[0].mru_rank.is_some(),
        "the deduped entry must KEEP its mru_rank (Some), not be reset to None"
    );
    assert!(
        displays(&entries).contains("other.md"),
        "the non-MRU file should also be discovered by traversal"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (8) `cancel()` stops the streaming traversal: after cancelling, the version
/// no longer advances. Tolerant by design — it is fine if `complete` was already
/// reached; the point is that nothing hangs and the version stops growing.
#[test]
fn cancel_stops_streaming() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    for i in 0..MANY_FILES {
        write_file(&cwd.join(format!("f_{i:04}.md")), "x");
    }

    let core = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);

    // Cancel immediately; the traversal task should stop bumping version.
    search.cancel();

    // Let any in-flight batch land, then sample the version across a window.
    std::thread::sleep(CANCEL_SETTLE);
    let v1 = search.read().version;
    std::thread::sleep(CANCEL_SETTLE);
    let v2 = search.read().version;

    assert_eq!(
        v1, v2,
        "after cancel the search version must stop increasing (was {v1}, now {v2})"
    );

    drop(wakes);
    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}
