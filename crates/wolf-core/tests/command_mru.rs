//! Direct integration tests for the global command MRU cache
//! (`wolf_core::CommandMru`) against a REAL SQLite database on a REAL temporary
//! filesystem. No mocks, no Neovim.
//!
//! This is the command-palette sibling of `tests/recent_dirs.rs`: the store is a
//! near-clone of `RecentDirs` keyed on an opaque command KEY string (a static
//! command's registry `id`, or a dynamic `switch_window:<cwd>` /
//! `open_recent:<path>` key). Like the recent-dirs tests it does NOT touch
//! `state_dir()` / `WOLF_STATE_DIR`: it constructs a `CommandMru::open(<tempdir>)`
//! directly, so it needs no env-lock and never races another binary's environment.
//!
//! Surface exercised (per plan §2 / §5):
//! - `wolf_core::CommandMru::open(dir: &Path) -> CommandMru` (infallible; eagerly
//!   creates `dir/command-mru.sqlite`)
//! - `wolf_core::CommandMru::disabled() -> CommandMru` (all methods no-op)
//! - `CommandMru::record(&self, key: &str)` (fire-and-forget move-or-add-to-top)
//! - `CommandMru::recent(&self) -> Vec<String>` (newest-first, capped, empty on
//!   any failure)
//! - `wolf_core::COMMAND_MRU_LIMIT` (the 100-entry cap)
//!
//! Ordering is deterministic via a monotonic `seq` column (NOT a timestamp), so
//! entries recorded in the same millisecond still order by insertion recency —
//! hence NO sleeps between records anywhere in this file.

use wolf_core::{CommandMru, COMMAND_MRU_LIMIT};

/// The physical DB file name created under the store's dir. Mirrored locally
/// because it is a private implementation constant; the tests assert the file's
/// existence, so they need the name.
const DB_NAME: &str = "command-mru.sqlite";

/// How many distinct keys the cap test records — comfortably beyond the limit so
/// the oldest few must be pruned.
const OVER_LIMIT: usize = COMMAND_MRU_LIMIT + 5;

/// True if `recent` (newest-first) contains `key` at any rank.
fn contains(recent: &[String], key: &str) -> bool {
    recent.iter().any(|k| k == key)
}

#[test]
fn open_creates_sqlite_and_recent_is_empty() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");

    let store = CommandMru::open(dir.path());

    let db = dir.path().join(DB_NAME);
    assert!(
        db.exists(),
        "open() should eagerly create the cache at {}",
        db.display()
    );

    assert!(
        store.recent().is_empty(),
        "a freshly opened store should have no recent commands"
    );
}

#[test]
fn records_appear_newest_first() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let store = CommandMru::open(dir.path());

    // Record oldest -> newest. No sleeps: ordering is by monotonic seq.
    store.record("a");
    store.record("b");
    store.record("c");

    let recent = store.recent();
    assert_eq!(recent.len(), 3, "all three records should be present");

    // Newest first: c, b, a.
    assert_eq!(recent[0], "c", "newest should be first");
    assert_eq!(recent[1], "b");
    assert_eq!(recent[2], "a", "oldest should be last");
}

#[test]
fn re_recording_moves_to_front_without_duplicating() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let store = CommandMru::open(dir.path());

    store.record("a");
    store.record("b");
    store.record("c");
    // Re-record the OLDEST entry: it must jump to the front (rank 0) and the
    // list must still hold exactly three entries (upsert, not duplicate).
    store.record("a");

    let recent = store.recent();
    assert_eq!(
        recent.len(),
        3,
        "re-record must upsert, not add a duplicate"
    );

    // a jumps to front; the rest keep their relative order behind it: c, then b.
    assert_eq!(recent[0], "a", "the re-recorded entry should now be newest");
    assert_eq!(recent[1], "c");
    assert_eq!(recent[2], "b");

    let a_count = recent.iter().filter(|k| *k == "a").count();
    assert_eq!(a_count, 1, "the re-recorded entry must appear exactly once");
}

#[test]
fn recent_is_capped_and_oldest_are_pruned() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let store = CommandMru::open(dir.path());

    // Record more than the cap, oldest -> newest.
    let mut keys = Vec::with_capacity(OVER_LIMIT);
    for i in 0..OVER_LIMIT {
        let k = format!("cmd{i:03}");
        store.record(&k);
        keys.push(k);
    }

    let recent = store.recent();
    assert_eq!(
        recent.len(),
        COMMAND_MRU_LIMIT,
        "recent() must return at most COMMAND_MRU_LIMIT entries"
    );

    // The most-recently recorded key is rank 0.
    assert_eq!(
        recent[0],
        keys[OVER_LIMIT - 1],
        "the newest recorded key should be rank 0"
    );

    // The (OVER_LIMIT - COMMAND_MRU_LIMIT) oldest fell off and must be absent.
    let dropped = OVER_LIMIT - COMMAND_MRU_LIMIT;
    for old in &keys[..dropped] {
        assert!(
            !contains(&recent, old),
            "the oldest key {old} should be beyond the cap and pruned"
        );
    }

    // Re-open a SECOND store on the same dir: the pruned rows must stay gone,
    // proving record() DELETEd them rather than recent() merely applying a LIMIT.
    drop(store);
    let store2 = CommandMru::open(dir.path());
    let recent2 = store2.recent();
    assert_eq!(
        recent2.len(),
        COMMAND_MRU_LIMIT,
        "the on-disk row count must already be capped after re-open"
    );
    for old in &keys[..dropped] {
        assert!(
            !contains(&recent2, old),
            "pruned key {old} must not reappear after re-open (DELETE, not just LIMIT)"
        );
    }
}

#[test]
fn garbage_db_is_silently_recreated() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let db = dir.path().join(DB_NAME);

    // Establish a valid DB with one row, then drop the store so no live handle
    // holds the file while we clobber it.
    {
        let store = CommandMru::open(dir.path());
        store.record("one");
        assert_eq!(store.recent().len(), 1, "precondition: one row recorded");
    }

    // Overwrite the sqlite file with garbage bytes — a bit-for-bit non-database.
    std::fs::write(&db, b"this is not a sqlite database, just some bytes\n")
        .expect("failed to overwrite db with garbage");

    // Re-open: must NOT panic, must silently delete + recreate the DB.
    let store = CommandMru::open(dir.path());
    assert!(
        store.recent().is_empty(),
        "a recreated store should read as empty (the garbage row is gone)"
    );

    // A subsequent record/recent must round-trip through the recreated DB.
    store.record("after_heal");
    let recent = store.recent();
    assert_eq!(recent.len(), 1, "record must round-trip after self-heal");
    assert_eq!(
        recent[0], "after_heal",
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

    // A valid sqlite DB whose `command_mru` table lacks `seq`: the schema probe (`SELECT key, seq
    // FROM command_mru LIMIT 0`) must detect this and recreate the DB rather than error.
    {
        let conn = rusqlite::Connection::open(&db).expect("failed to open sqlite for bad schema");
        conn.execute_batch("CREATE TABLE command_mru (key TEXT PRIMARY KEY);")
            .expect("failed to create wrong-schema table");
    }

    // Re-open: the bad schema must be healed silently (no panic, no error).
    let store = CommandMru::open(dir.path());
    assert!(
        store.recent().is_empty(),
        "a store opened over a wrong-schema DB should read as empty after heal"
    );

    // And it must be usable afterwards.
    store.record("healed");
    let recent = store.recent();
    assert_eq!(recent.len(), 1, "record must round-trip after schema heal");
    assert_eq!(recent[0], "healed");

    assert!(db.exists(), "the sqlite file must exist after schema heal");
}

#[test]
fn disabled_store_is_a_noop() {
    let store = CommandMru::disabled();

    // No panic on record, and recent() is always empty.
    store.record("some_command");
    assert!(
        store.recent().is_empty(),
        "a disabled store must report no recent commands"
    );
}
