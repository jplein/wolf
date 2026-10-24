//! Integration tests for the BOUNDED file-search traversal (the `$HOME`-walk
//! safety net). These exercise a REAL filesystem tree (via `tempfile`) and a
//! REAL, hermetic Neovim (via `WolfCore::start` / `common::require_nvim_bin`) —
//! no mocks, consistent with the project's real-tool / real-nvim policy.
//!
//! This is a SEPARATE test binary from `file_search.rs` on purpose: each test
//! here mutates PROCESS-GLOBAL env vars (`WOLF_SEARCH_MAX_ENTRIES` /
//! `WOLF_SEARCH_BUDGET_MS` / `WOLF_SEARCH_MAX_DEPTH`) that tune the traversal
//! caps. A separate binary is a separate process, so those mutations cannot
//! disturb the 11 existing `file_search.rs` tests (which assume the DEFAULT
//! caps). Within THIS binary the tests still run concurrently, so a
//! module-level [`BOUNDS_ENV_LOCK`] serializes the `{set_var; start_file_search;
//! remove_var}` critical section (the caps are read SYNCHRONOUSLY inside
//! `start_file_search`, so holding the lock across that call is sufficient — no
//! `serial_test` dependency needed).
//!
//! Surface exercised (per plan §files.rs / §api.rs `start_file_search`):
//! - `SearchBounds::from_env()` reads the 3 env vars (defaults 50_000 / 5s / 64).
//! - `FileSearchState.truncated: bool` — set true when the walk stops early at
//!   the entry cap or the time budget. A cap-/budget-truncated search STILL sets
//!   `complete = true` (post-loop `!cancel`). Depth pruning does NOT set
//!   `truncated` (it is invisible in the `ignore` API), so the depth test asserts
//!   via ABSENCE of a too-deep file rather than on the flag.
//!
//! NOTE to coder: the read guard fields assumed here are `entries`, `complete`,
//! `version` (existing) and the NEW `truncated`. If any differ, align them.

mod common;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use wolf_core::{FileEntry, FileSearch, WakeFn, WolfCore};

/// Upper bound for a single bounded search to reach `complete`. Generous because
/// it includes nvim startup amortized across the test plus a (bounded) FS walk.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to block on a wake before re-checking `complete` (so we make
/// progress even if a wake is missed/coalesced).
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Entry cap driven into `WOLF_SEARCH_MAX_ENTRIES` for the entry-cap test, and
/// the value asserted against. Single source of truth (the env var is set from
/// this via `.to_string()`), so the cap and the assertion can never drift.
const ENTRY_CAP: usize = 10;

/// Number of flat `.md` files the entry-cap test creates. Comfortably exceeds
/// both [`ENTRY_CAP`] and the traversal batch size (plan: `TRAVERSAL_BATCH =
/// 256`) so the walk is forced to break at the cap mid-batch.
const FLAT_FILE_COUNT: usize = 100;

/// Number of files the budget test creates. Large enough that ~this many stat
/// syscalls comfortably exceed the 1ms budget, so the walk trips the time budget
/// rather than exhausting the tree.
const LARGE_FILE_COUNT: usize = 2_000;

// --- Env serialization -----------------------------------------------------

/// Serializes the `{set_var; start_file_search; remove_var}` critical section
/// across the concurrently-run tests in this binary. The caps are read
/// synchronously inside `start_file_search`, so holding this lock across that
/// single call is sufficient to keep one test's env from leaking into another.
static BOUNDS_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Acquires [`BOUNDS_ENV_LOCK`], recovering from poisoning so a panic in one
/// test does not cascade into spurious failures in the others (we only guard a
/// tiny env-mutation window; the `()` payload has no invariant to protect).
fn lock_env() -> MutexGuard<'static, ()> {
    BOUNDS_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

// --- Helpers (copied from `file_search.rs` so this binary is self-contained) -

/// Creates a fresh temp dir to serve as a core `cwd` and returns both the guard
/// (kept alive by the caller) and the CANONICAL path to it. Canonicalized because
/// temp dirs can sit behind symlinks (e.g. macOS `/tmp -> /private/tmp`); the MRU
/// cache canonicalizes internally, so a canonical cwd keeps the traversal root
/// and the relativized `display` strings consistent.
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

/// Builds a `WakeFn` that signals on a channel, paired with the receiver the poll
/// loop waits on. Mirrors `file_search.rs`'s local `search_wake`.
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
    core
}

/// A snapshot of a completed search: the collected entries plus the `truncated`
/// flag, both read from the SAME guard as `complete` (so `truncated` reflects the
/// state at the moment the walk finished, not a later re-read).
struct SearchResult {
    entries: Vec<FileEntry>,
    truncated: bool,
}

/// Polls `search` until its view reports `complete`, waking on `wakes`, then
/// returns a snapshot of the collected entries and the `truncated` flag. Panics
/// on timeout.
fn poll_until_complete(search: &FileSearch, wakes: &Receiver<()>) -> SearchResult {
    let deadline = Instant::now() + SEARCH_TIMEOUT;
    loop {
        {
            let view = search.read();
            if view.complete {
                return SearchResult {
                    entries: view.entries.to_vec(),
                    truncated: view.truncated,
                };
            }
        }
        if Instant::now() >= deadline {
            panic!("bounded file search did not reach `complete` within {SEARCH_TIMEOUT:?}");
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

// --- Tests -----------------------------------------------------------------

/// (1) Entry cap: a tree with far more listable files than the cap streams only
/// up to the cap, then STOPS — marking the search both `complete` (the walk ended
/// cleanly, not cancelled) and `truncated` (it did not exhaust the tree).
#[test]
fn entry_cap_truncates_and_completes() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    // Flat tree of empty `.md` files, comfortably over the cap. The MRU is empty
    // (fresh cwd), so the synchronous seed cannot pad the entry count.
    for i in 0..FLAT_FILE_COUNT {
        write_file(&cwd.join(format!("f_{i:04}.md")), "x");
    }

    let core = start_core_in(&cwd);
    let (wake, wakes) = search_wake();

    // Set the cap, start the search (which reads it synchronously), then clear it
    // — all under the env lock so a concurrent test's caps can't leak in.
    let search = {
        let _env = lock_env();
        std::env::set_var("WOLF_SEARCH_MAX_ENTRIES", ENTRY_CAP.to_string());
        let s = core.start_file_search(wake);
        std::env::remove_var("WOLF_SEARCH_MAX_ENTRIES");
        s
    };

    let result = poll_until_complete(&search, &wakes);

    assert!(
        result.truncated,
        "hitting the entry cap must mark the search truncated; got {} entries",
        result.entries.len()
    );
    // TRAVERSAL_BATCH (256) > ENTRY_CAP, so the walk breaks exactly at the cap; `<=` rather than
    // `==` keeps the case robust to a future batch-flush boundary.
    assert!(
        result.entries.len() <= ENTRY_CAP,
        "streaming must stop at (or very near) the cap {ENTRY_CAP}; got {}",
        result.entries.len()
    );
    assert!(
        !result.entries.is_empty(),
        "the capped search should still have streamed at least one entry"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (2) Max depth: files deeper than the cap are pruned. Depth pruning is SILENT
/// (invisible in the `ignore` API), so this asserts via the ABSENCE of a too-deep
/// file rather than on `truncated`.
///
/// Depth convention (matching `WalkBuilder::max_depth`): the root is depth 0, so a
/// file directly in the root is depth 1. With `WOLF_SEARCH_MAX_DEPTH=2`:
/// - `top.md`        -> depth 1 -> KEPT
/// - `a/mid.md`      -> depth 2 -> KEPT
/// - `a/b/c/deep.md` -> depth 4 -> PRUNED
#[test]
fn max_depth_prunes_deep_files() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    write_file(&cwd.join("top.md"), "t");
    write_file(&cwd.join("a").join("mid.md"), "m");
    write_file(&cwd.join("a").join("b").join("c").join("deep.md"), "d");

    let core = start_core_in(&cwd);
    let (wake, wakes) = search_wake();

    let search = {
        let _env = lock_env();
        std::env::set_var("WOLF_SEARCH_MAX_DEPTH", "2");
        let s = core.start_file_search(wake);
        std::env::remove_var("WOLF_SEARCH_MAX_DEPTH");
        s
    };

    let result = poll_until_complete(&search, &wakes);
    let set = displays(&result.entries);

    assert!(
        set.contains("top.md"),
        "a depth-1 file must be listed under max_depth=2; got {set:?}"
    );
    assert!(
        set.contains(&rel(&["a", "mid.md"])),
        "a depth-2 file must be listed under max_depth=2; got {set:?}"
    );
    assert!(
        !set.contains(&rel(&["a", "b", "c", "deep.md"])),
        "a depth-4 file must be pruned under max_depth=2; got {set:?}"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (3) Wall-clock budget: with a 1ms budget over a large tree, the walk trips the
/// time budget long before exhausting the tree — so the search ends `complete`
/// (clean, not cancelled) AND `truncated`, and does so within the poll timeout.
///
/// Deliberately LOOSE: no exact entry count is asserted (that would be timing-
/// dependent and flaky). If this ever proves timing-flaky, the fix is to enlarge
/// the tree (more stat syscalls -> the 1ms budget trips even more reliably); the
/// entry-cap test above already covers the same "break out of the walk early"
/// path deterministically, so this test can be dropped if it can't be made stable.
#[test]
fn budget_truncates_quickly() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    for i in 0..LARGE_FILE_COUNT {
        write_file(&cwd.join(format!("f_{i:05}.md")), "x");
    }

    let core = start_core_in(&cwd);
    let (wake, wakes) = search_wake();

    let search = {
        let _env = lock_env();
        std::env::set_var("WOLF_SEARCH_BUDGET_MS", "1");
        let s = core.start_file_search(wake);
        std::env::remove_var("WOLF_SEARCH_BUDGET_MS");
        s
    };

    // Load-bearing: this RETURNS within SEARCH_TIMEOUT (poll_until_complete panics
    // otherwise), proving the budget actually stops the walk.
    let result = poll_until_complete(&search, &wakes);

    assert!(
        result.truncated,
        "tripping the time budget must mark the search truncated; got {} entries",
        result.entries.len()
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}
