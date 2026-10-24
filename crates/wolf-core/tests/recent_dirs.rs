//! Direct integration tests for the global recent-directories MRU cache
//! (`wolf_core::RecentDirs`) against a REAL SQLite database on a REAL temporary
//! filesystem. No mocks, no Neovim.
//!
//! These tests do NOT touch `state_dir()` / `WOLF_STATE_DIR`: they construct a
//! `RecentDirs::open(<tempdir>)` directly, so they need no env-lock and never race
//! any other binary's environment. The record-on-`:cd` path (driven through a real
//! nvim) is covered separately in `tests/recent_dirs_records.rs`.
//!
//! Surface exercised:
//! - `wolf_core::RecentDirs::open(dir: &Path) -> RecentDirs` (infallible; eagerly
//!   creates `dir/folder-mru.sqlite`)
//! - `wolf_core::RecentDirs::disabled() -> RecentDirs` (all methods no-op)
//! - `RecentDirs::record(&self, path: &Path)` (fire-and-forget move-or-add-to-top)
//! - `RecentDirs::recent(&self) -> Vec<PathBuf>` (newest-first, capped, empty on
//!   any failure)
//! - `wolf_core::RECENT_DIRS_LIMIT` (the 20-entry cap)
//!
//! Ordering is deterministic via a monotonic `seq` column (NOT a timestamp), so
//! entries recorded in the same millisecond still order by insertion recency —
//! hence NO sleeps between records anywhere in this file.

use std::path::{Path, PathBuf};

use wolf_core::{RecentDirs, RECENT_DIRS_LIMIT};

/// The physical DB file name created under the store's dir. Mirrored locally
/// because it is a private implementation constant; the tests assert the file's
/// existence, so they need the name.
const DB_NAME: &str = "folder-mru.sqlite";

/// How many distinct dirs the cap test records — comfortably beyond the limit so
/// the oldest few must be pruned.
const OVER_LIMIT: usize = 25;

/// Compares two paths by their canonical form when resolvable, else raw equality
/// — robust to the macOS `/var` -> `/private/var` (and `/tmp` -> `/private/tmp`)
/// tempdir symlink. Mirrors `mru.rs` / `state_unit.rs`.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// True if `recent` (newest-first) contains `path` at any rank.
fn contains(recent: &[PathBuf], path: &Path) -> bool {
    recent.iter().any(|p| paths_equal(p, path))
}

#[test]
fn open_creates_sqlite_and_recent_is_empty() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");

    let store = RecentDirs::open(dir.path());

    let db = dir.path().join(DB_NAME);
    assert!(
        db.exists(),
        "open() should eagerly create the cache at {}",
        db.display()
    );

    assert!(
        store.recent().is_empty(),
        "a freshly opened store should have no recent dirs"
    );
}

#[test]
fn records_appear_newest_first() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let store = RecentDirs::open(dir.path());

    // Synthetic subdir paths; RecentDirs does NOT check existence, so they need
    // not be created on disk.
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let c = dir.path().join("c");

    // Record oldest -> newest. No sleeps: ordering is by monotonic seq.
    store.record(&a);
    store.record(&b);
    store.record(&c);

    let recent = store.recent();
    assert_eq!(recent.len(), 3, "all three records should be present");

    // Newest first: c, b, a.
    assert!(paths_equal(&recent[0], &c), "newest should be first");
    assert!(paths_equal(&recent[1], &b));
    assert!(paths_equal(&recent[2], &a), "oldest should be last");
}

#[test]
fn re_recording_moves_to_front_without_duplicating() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let store = RecentDirs::open(dir.path());

    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let c = dir.path().join("c");

    store.record(&a);
    store.record(&b);
    store.record(&c);
    // Re-record the OLDEST entry: it must jump to the front (rank 0) and the
    // list must still hold exactly three entries (upsert, not duplicate).
    store.record(&a);

    let recent = store.recent();
    assert_eq!(
        recent.len(),
        3,
        "re-record must upsert, not add a duplicate"
    );

    // a jumps to front; the rest keep their relative order behind it: c, then b.
    assert!(
        paths_equal(&recent[0], &a),
        "the re-recorded entry should now be newest"
    );
    assert!(paths_equal(&recent[1], &c));
    assert!(paths_equal(&recent[2], &b));

    let a_count = recent.iter().filter(|p| paths_equal(p, &a)).count();
    assert_eq!(a_count, 1, "the re-recorded entry must appear exactly once");
}

#[test]
fn recent_is_capped_and_oldest_are_pruned() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let store = RecentDirs::open(dir.path());

    // Record more than the cap, oldest -> newest. Synthetic, non-existent subdirs.
    let mut paths = Vec::with_capacity(OVER_LIMIT);
    for i in 0..OVER_LIMIT {
        let p = dir.path().join(format!("d{i:02}"));
        store.record(&p);
        paths.push(p);
    }

    let recent = store.recent();
    assert_eq!(
        recent.len(),
        RECENT_DIRS_LIMIT,
        "recent() must return at most RECENT_DIRS_LIMIT entries"
    );

    // The most-recently recorded dir is rank 0.
    assert!(
        paths_equal(&recent[0], &paths[OVER_LIMIT - 1]),
        "the newest recorded dir should be rank 0"
    );

    // The (OVER_LIMIT - RECENT_DIRS_LIMIT) oldest fell off and must be absent.
    let dropped = OVER_LIMIT - RECENT_DIRS_LIMIT;
    for old in &paths[..dropped] {
        assert!(
            !contains(&recent, old),
            "the oldest dir {} should be beyond the cap and pruned",
            old.display()
        );
    }

    // Re-open a SECOND store on the same dir: the pruned rows must stay gone,
    // proving record() DELETEd them rather than recent() merely applying a LIMIT.
    drop(store);
    let store2 = RecentDirs::open(dir.path());
    let recent2 = store2.recent();
    assert_eq!(
        recent2.len(),
        RECENT_DIRS_LIMIT,
        "the on-disk row count must already be capped after re-open"
    );
    for old in &paths[..dropped] {
        assert!(
            !contains(&recent2, old),
            "pruned dir {} must not reappear after re-open (DELETE, not just LIMIT)",
            old.display()
        );
    }
}

#[test]
fn garbage_db_is_silently_recreated() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let db = dir.path().join(DB_NAME);

    // Establish a valid DB with one row, then drop the store so no live handle
    // holds the file while we clobber it.
    let one = dir.path().join("one");
    {
        let store = RecentDirs::open(dir.path());
        store.record(&one);
        assert_eq!(store.recent().len(), 1, "precondition: one row recorded");
    }

    // Overwrite the sqlite file with garbage bytes — a bit-for-bit non-database.
    std::fs::write(&db, b"this is not a sqlite database, just some bytes\n")
        .expect("failed to overwrite db with garbage");

    // Re-open: must NOT panic, must silently delete + recreate the DB.
    let store = RecentDirs::open(dir.path());
    assert!(
        store.recent().is_empty(),
        "a recreated store should read as empty (the garbage row is gone)"
    );

    // A subsequent record/recent must round-trip through the recreated DB.
    let after = dir.path().join("after-heal");
    store.record(&after);
    let recent = store.recent();
    assert_eq!(recent.len(), 1, "record must round-trip after self-heal");
    assert!(
        paths_equal(&recent[0], &after),
        "the post-heal record should be readable back"
    );

    assert!(
        db.exists(),
        "the sqlite file must exist again after self-heal"
    );
}

#[test]
fn wrong_schema_db_is_silently_recreated() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let db = dir.path().join(DB_NAME);

    // A valid sqlite DB whose `recent_dirs` table lacks the `seq` column the store queries: its
    // schema probe must detect this and recreate the DB rather than error.
    {
        let conn = rusqlite::Connection::open(&db).expect("failed to open sqlite for bad schema");
        conn.execute_batch("CREATE TABLE recent_dirs (path TEXT PRIMARY KEY);")
            .expect("failed to create wrong-schema table");
    }

    // Re-open: the bad schema must be healed silently (no panic, no error).
    let store = RecentDirs::open(dir.path());
    assert!(
        store.recent().is_empty(),
        "a store opened over a wrong-schema DB should read as empty after heal"
    );

    // And it must be usable afterwards.
    let d = dir.path().join("healed");
    store.record(&d);
    let recent = store.recent();
    assert_eq!(recent.len(), 1, "record must round-trip after schema heal");
    assert!(paths_equal(&recent[0], &d));

    assert!(db.exists(), "the sqlite file must exist after schema heal");
}

#[test]
fn disabled_store_is_a_noop() {
    let store = RecentDirs::disabled();

    // No panic on record, and recent() is always empty.
    let some = Path::new("/some/dir");
    store.record(some);
    assert!(
        store.recent().is_empty(),
        "a disabled store must report no recent dirs"
    );
}
