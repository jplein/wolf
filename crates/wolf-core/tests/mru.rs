//! Direct integration tests for the global recent-files MRU cache
//! (`wolf_core::MruStore`) against a REAL SQLite database on a REAL temporary
//! filesystem. No mocks, no Neovim.
//!
//! The MRU now lives in a SINGLE global DB at `state_dir()/file-mru.sqlite`
//! (mirroring the sibling `command-mru.sqlite` / `folder-mru.sqlite` stores). It
//! is directory-SCOPED: every row carries the project `directory` it was recorded
//! under, and `recent(directory)` filters to just that directory. There is no
//! per-`cwd` `.wolf/mru.sqlite` anymore.
//!
//! Like the `command_mru.rs` / `recent_dirs.rs` sibling tests, these do NOT touch
//! `state_dir()` / `WOLF_STATE_DIR`: they construct `MruStore::open(<tempdir>)`
//! directly, passing that tempdir AS the state dir that holds `file-mru.sqlite`,
//! and pass explicit `directory` args to `record` / `recent`. So they need no
//! env-lock and never race another binary's environment. The record-on-open path
//! (driven through nvim) is covered separately in `tests/mru_open_records.rs`.
//!
//! Surface exercised:
//! - `wolf_core::MruStore::open(dir: &Path) -> MruStore` (infallible; `dir` is the
//!   state dir holding `file-mru.sqlite`, eagerly created)
//! - `MruStore::record(&self, directory: &Path, file: &Path)` (fire-and-forget
//!   upsert; `directory` is canonicalized internally, `file` stored absolute)
//! - `MruStore::recent(&self, directory: &Path) -> Vec<wolf_core::FileEntry>`
//!   (rows whose normalized `directory` matches, most-recent-first, capped at
//!   `MRU_LIMIT`, missing files pruned inline)
//! - `wolf_core::FileEntry { path: PathBuf, display: String, mru_rank: Option<u32> }`
//! - `wolf_core::MRU_LIMIT` (the 50-entry per-directory cap)
//!
//! IMPORTANT (canonicalization): `record`/`recent` normalize the `directory` arg
//! internally (`canonicalize`, so macOS `/var` -> `/private/var`), and `file` is
//! stored as an absolute (canonical-if-exists) path. So every `directory` a test
//! compares against is created via [`canon_tempdir`] (already canonical), which
//! keeps the stored `file` paths under that directory and makes the relative
//! `display` strings line up.
//!
//! NOTE (ordering determinism): several tests assert `recent()` is most-recent
//! first. This requires two `record()` calls a few milliseconds apart to receive
//! DISTINCT, monotonically increasing `opened_at` values (millisecond-or-finer
//! resolution plus a deterministic tiebreak, e.g. `ORDER BY opened_at DESC,
//! rowid DESC`). The tests insert with a small gap (`REC_GAP`) to make distinct
//! millisecond timestamps the common case; if they flake that is a signal to fix
//! the resolution/tiebreak, not the tests.
//!
//! NOTE (self-heal policy): the last FIVE tests pin the store's DATA-LOSS
//! contract, which the rest of the file does not touch.
//!
//! - The store recreates `file-mru.sqlite` (deleting it plus its `-wal`/`-shm`
//!   sidecars) only for a database that is genuinely CORRUPT or
//!   schema-incompatible. A TRANSIENT failure — most importantly losing the lock
//!   race that the rollback-journal -> WAL conversion needs when several handles
//!   open the same not-yet-converted DB at once — must back off and degrade to an
//!   empty result, NEVER delete.
//! - Deleting the file unlinks it from under every other live connection in the
//!   process, whose writes then land in an unreachable inode. That is the
//!   production shape of "the user has two Wolf windows open": one window's open
//!   silently wipes the other's — and the user's — entire recent-files history.
//! - So the guards come in a MATCHED PAIR of groups, and neither group can be
//!   satisfied on its own:
//!   * THREE tests prove a second / concurrent / lock-losing open does NOT destroy
//!     rows ([`a_second_store_open_does_not_destroy_existing_rows`],
//!     [`concurrent_opens_preserve_rows`],
//!     [`an_open_that_loses_the_lock_race_preserves_rows`]). Satisfying only these
//!     would license never recreating at all.
//!   * TWO prove an unusable file IS still healed
//!     ([`a_corrupt_database_is_still_healed`],
//!     [`wrong_schema_db_is_silently_recreated`]), which is what stops the
//!     over-correction. They cover the two DISTINCT rusqlite error SHAPES the heal
//!     path must classify — see each test's doc comment; matching only one of them
//!     silently disables half the recreation path.
//!
//! These use real threads, a real [`std::sync::Barrier`], and real second SQLite
//! connections holding real locks against the real database — no mocks, no
//! injected failures.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread::{self, sleep};
use std::time::Duration;

use wolf_core::{FileEntry, MruStore, MRU_LIMIT};

/// The physical DB file name created under the store's state dir. Mirrored
/// locally because it is a private implementation constant; the tests assert the
/// file's existence, so they need the name.
const MRU_DB_NAME: &str = "file-mru.sqlite";

/// How many files the cap test records — comfortably beyond `MRU_LIMIT` so the
/// oldest few must fall off.
const OVER_LIMIT: usize = MRU_LIMIT + 5;

/// Gap between successive `record()` calls in ordering-sensitive tests, so each
/// gets a distinct `opened_at` (see the module-level NOTE on determinism).
const REC_GAP: Duration = Duration::from_millis(5);

/// How many extra `MruStore` handles race the main one in
/// [`concurrent_opens_preserve_rows`]. Deliberately small: enough handles to
/// contend for the one lock the WAL conversion needs, few enough that the test
/// stays a `size = "small"` target.
const CONCURRENT_OPENS: usize = 8;

/// How many rows [`concurrent_opens_preserve_rows`] records through the main
/// handle. Comfortably under `MRU_LIMIT` so the cap can never explain a shortfall,
/// and more than one so a partial wipe is distinguishable from a total one.
const CONCURRENT_FILES: usize = 20;

/// How many garbage bytes [`a_corrupt_database_is_still_healed`] writes over the
/// database file. Well past SQLite's 100-byte header, so the file is unambiguously
/// "not a database" rather than a zero-length file SQLite would happily adopt.
const GARBAGE_LEN: usize = 4096;

/// How many rows [`an_open_that_loses_the_lock_race_preserves_rows`] records before
/// the lock is taken. Comfortably under `MRU_LIMIT` so the cap can never explain a
/// shortfall, and well above one so a partial loss is distinguishable from a total
/// wipe.
const LOCKED_FILES: usize = 12;

/// Creates an empty file at `path`, making parent directories as needed.
fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create parent dir");
    }
    fs::write(path, b"x").expect("failed to create file");
}

/// True if `path`'s final component is `name`.
fn ends_with(path: &Path, name: &str) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some(name)
}

/// How many entries in `recent` end with file name `name`.
fn count_named(recent: &[FileEntry], name: &str) -> usize {
    recent.iter().filter(|e| ends_with(&e.path, name)).count()
}

/// Creates a fresh tempdir to serve as a project `directory` arg and returns both
/// the guard (kept alive by the caller) and the CANONICAL path to it.
///
/// The path is canonicalized because temp dirs can sit behind symlinks (macOS
/// `/tmp -> /private/tmp`, `/var -> /private/var`); `record`/`recent` canonicalize
/// the `directory` internally and `record` stores an absolute (canonical) `file`,
/// so passing an already-canonical directory keeps the stored paths under it and
/// the relative `display` strings consistent.
fn canon_tempdir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let path = fs::canonicalize(dir.path()).expect("failed to canonicalize tempdir");
    (dir, path)
}

/// Creates a fresh tempdir to serve as the STATE dir that holds `file-mru.sqlite`.
/// Returned as a guard + path; the guard keeps the DB alive for the test.
fn state_tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("failed to create state tempdir")
}

#[test]
fn open_creates_sqlite_and_recent_is_empty() {
    let state = state_tempdir();
    let store = MruStore::open(state.path());

    let db = state.path().join(MRU_DB_NAME);
    assert!(
        db.exists(),
        "open() should eagerly create the cache at {}",
        db.display()
    );

    let (_dir, dir_a) = canon_tempdir();
    assert!(
        store.recent(&dir_a).is_empty(),
        "a freshly opened store should have no MRU entries for any directory"
    );
}

/// (a) `record` writes a row with the correct directory/file, readable back via
/// `recent(directory)` with a directory-relative `display`.
#[test]
fn record_writes_row_with_correct_directory_and_file() {
    let state = state_tempdir();
    let store = MruStore::open(state.path());

    let (_dir, dir_a) = canon_tempdir();
    let file = dir_a.join("x.md");
    touch(&file);
    store.record(&dir_a, &file);

    let recent = store.recent(&dir_a);
    assert_eq!(recent.len(), 1, "the single record should be present");
    assert!(
        ends_with(&recent[0].path, "x.md"),
        "the stored path should end with the recorded file name"
    );
    assert_eq!(
        recent[0].display, "x.md",
        "display should be the file relative to its directory"
    );
}

/// (b) `recent` filters by directory: two directories recorded into the SAME
/// store are isolated — neither leaks into the other's results.
#[test]
fn recent_filters_by_directory() {
    let state = state_tempdir();
    let store = MruStore::open(state.path());

    let (_da, dir_a) = canon_tempdir();
    let (_db, dir_b) = canon_tempdir();

    let a_file = dir_a.join("a.md");
    let b_file = dir_b.join("b.md");
    touch(&a_file);
    touch(&b_file);
    store.record(&dir_a, &a_file);
    store.record(&dir_b, &b_file);

    let recent_a = store.recent(&dir_a);
    assert_eq!(recent_a.len(), 1, "A should see only its own file");
    assert!(ends_with(&recent_a[0].path, "a.md"));
    assert!(
        !recent_a.iter().any(|e| ends_with(&e.path, "b.md")),
        "B's file must NOT leak into A's results"
    );

    let recent_b = store.recent(&dir_b);
    assert_eq!(recent_b.len(), 1, "B should see only its own file");
    assert!(ends_with(&recent_b[0].path, "b.md"));
    assert!(
        !recent_b.iter().any(|e| ends_with(&e.path, "a.md")),
        "A's file must NOT leak into B's results"
    );
}

/// (c) `recent` is most-recent-first with ascending ranks, and re-recording an
/// entry moves it to the front (rank 0) without duplicating it.
#[test]
fn records_are_recent_first_and_rerecord_moves_to_front() {
    let state = state_tempdir();
    let store = MruStore::open(state.path());
    let (_dir, dir_a) = canon_tempdir();

    let a = dir_a.join("a.md");
    let b = dir_a.join("b.md");
    let c = dir_a.join("c.md");
    for p in [&a, &b, &c] {
        touch(p);
    }

    // Record oldest -> newest with a gap so opened_at is distinct.
    store.record(&dir_a, &a);
    sleep(REC_GAP);
    store.record(&dir_a, &b);
    sleep(REC_GAP);
    store.record(&dir_a, &c);

    let recent = store.recent(&dir_a);
    assert_eq!(recent.len(), 3, "all three records should be present");

    // Most-recent first: c, b, a, with ascending ranks 0, 1, 2.
    assert!(ends_with(&recent[0].path, "c.md"), "newest should be first");
    assert!(ends_with(&recent[1].path, "b.md"));
    assert!(ends_with(&recent[2].path, "a.md"), "oldest should be last");
    assert_eq!(recent[0].mru_rank, Some(0), "rank 0 = most recent");
    assert_eq!(recent[1].mru_rank, Some(1));
    assert_eq!(recent[2].mru_rank, Some(2));

    // Re-record the OLDEST entry: it must jump to the front (rank 0) and the list
    // must still hold exactly three entries (upsert, not duplicate).
    sleep(REC_GAP);
    store.record(&dir_a, &a);

    let recent = store.recent(&dir_a);
    assert_eq!(
        recent.len(),
        3,
        "re-record must upsert, not add a duplicate"
    );
    assert!(
        ends_with(&recent[0].path, "a.md"),
        "the re-recorded entry should now be most recent"
    );
    assert_eq!(recent[0].mru_rank, Some(0));
    assert_eq!(
        count_named(&recent, "a.md"),
        1,
        "the re-recorded entry must appear exactly once"
    );
    // The rest keep their relative order behind the moved entry: c, then b.
    assert!(ends_with(&recent[1].path, "c.md"));
    assert!(ends_with(&recent[2].path, "b.md"));
}

/// (d) A file deleted from disk is omitted by `recent` AND pruned from the cache:
/// re-opening the SAME state dir proves the row was `DELETE`d, not just filtered.
#[test]
fn deleted_file_is_omitted_and_pruned_from_cache() {
    let state = state_tempdir();
    let (_dir, dir_a) = canon_tempdir();

    let keep = dir_a.join("keep.md");
    let gone = dir_a.join("gone.md");
    touch(&keep);
    touch(&gone);

    {
        let store = MruStore::open(state.path());
        store.record(&dir_a, &keep);
        sleep(REC_GAP);
        store.record(&dir_a, &gone);
        assert_eq!(
            store.recent(&dir_a).len(),
            2,
            "both records present before deletion"
        );

        // Delete one file from disk; recent() must omit it (and prune the row).
        fs::remove_file(&gone).expect("failed to delete file");

        let recent = store.recent(&dir_a);
        assert_eq!(recent.len(), 1, "the missing file should be omitted");
        assert!(ends_with(&recent[0].path, "keep.md"));
    }

    // Re-open a SECOND store against the SAME state dir: the pruned row must be
    // gone for good, proving recent() DELETEd it (not just filtered it).
    let store2 = MruStore::open(state.path());
    let recent2 = store2.recent(&dir_a);
    assert_eq!(
        recent2.len(),
        1,
        "pruned row must not reappear after re-open"
    );
    assert!(
        !recent2.iter().any(|e| ends_with(&e.path, "gone.md")),
        "the deleted path must never reappear"
    );
    assert!(ends_with(&recent2[0].path, "keep.md"));
}

/// (e) The DB lives at `<state_dir>/file-mru.sqlite`, and NO `.wolf` directory is
/// ever created under a project `directory` arg.
#[test]
fn db_lives_in_state_dir_and_no_wolf_dir_is_created() {
    let state = state_tempdir();
    let store = MruStore::open(state.path());

    let db = state.path().join(MRU_DB_NAME);
    assert!(
        db.exists(),
        "open() should create the cache at {}",
        db.display()
    );

    let (_dir, dir_a) = canon_tempdir();
    let file = dir_a.join("x.md");
    touch(&file);
    store.record(&dir_a, &file);
    let _ = store.recent(&dir_a);

    assert!(
        !dir_a.join(".wolf").exists(),
        "no .wolf dir should be created under the project directory {}",
        dir_a.display()
    );
}

/// `recent` is capped at `MRU_LIMIT` per directory, keeping the newest entries.
#[test]
fn recent_is_capped_at_mru_limit() {
    let state = state_tempdir();
    let store = MruStore::open(state.path());
    let (_dir, dir_a) = canon_tempdir();

    // Record more than the cap, oldest -> newest, each on disk.
    let mut paths = Vec::with_capacity(OVER_LIMIT);
    for i in 0..OVER_LIMIT {
        let p = dir_a.join(format!("file_{i:02}.md"));
        touch(&p);
        store.record(&dir_a, &p);
        sleep(REC_GAP);
        paths.push(p);
    }

    let recent = store.recent(&dir_a);
    assert_eq!(
        recent.len(),
        MRU_LIMIT,
        "recent() must return at most MRU_LIMIT entries"
    );

    // The most-recent MRU_LIMIT survive; the oldest fall off the end.
    let newest = paths[OVER_LIMIT - 1].file_name();
    let oldest = paths[0].file_name();
    assert_eq!(
        recent[0].path.file_name(),
        newest,
        "the most-recently recorded file should be rank 0"
    );
    assert_eq!(recent[0].mru_rank, Some(0));
    assert!(
        !recent.iter().any(|e| e.path.file_name() == oldest),
        "the oldest record should be beyond the cap and not returned"
    );
}

/// `display` is relative to the `directory` arg (nested path preserved) and
/// `path` is absolute.
#[test]
fn display_is_relative_to_directory_and_path_is_absolute() {
    let state = state_tempdir();
    let store = MruStore::open(state.path());
    let (_dir, dir_a) = canon_tempdir();

    let nested = dir_a.join("sub").join("a.md");
    touch(&nested);
    store.record(&dir_a, &nested);

    let recent = store.recent(&dir_a);
    assert_eq!(recent.len(), 1);
    let entry = &recent[0];

    assert!(
        entry.path.is_absolute(),
        "FileEntry.path should be absolute (used for Open); got {}",
        entry.path.display()
    );

    // `display` is the path relative to the directory, rendered with the platform
    // separator (so "sub/a.md" on unix).
    let expected = Path::new("sub").join("a.md").to_string_lossy().into_owned();
    assert_eq!(
        entry.display, expected,
        "display should be the path relative to the directory"
    );
}

/// A SECOND `MruStore::open` on the same state dir must not destroy rows the
/// FIRST handle already recorded.
///
/// This is the production hazard stated directly: a second Wolf window opens its
/// own `MruStore` against the same global state dir, exactly as a second
/// `WolfCore::start` does. The store self-heals by DELETING `file-mru.sqlite`
/// (plus its `-wal`/`-shm` sidecars) and recreating the schema, so a heal fired
/// for any reason OTHER than genuine corruption silently wipes the user's whole
/// recent-files history — and unlinks the file from under the first, still-live
/// connection, so that handle's later writes go nowhere either.
///
/// Distinct from [`deleted_file_is_omitted_and_pruned_from_cache`], which also
/// re-opens the same state dir but only AFTER the first store has been dropped.
/// Here the first handle stays ALIVE across the second open, which is the case
/// that matches two concurrent windows.
#[test]
fn a_second_store_open_does_not_destroy_existing_rows() {
    let state = state_tempdir();
    let (_dir, dir_a) = canon_tempdir();

    let file = dir_a.join("survivor.md");
    touch(&file);

    let first = MruStore::open(state.path());
    first.record(&dir_a, &file);
    assert_eq!(
        count_named(&first.recent(&dir_a), "survivor.md"),
        1,
        "precondition: the first handle must read back the row it just recorded"
    );

    // A second handle on the SAME state dir, opened while the first is still live.
    let second = MruStore::open(state.path());

    assert_eq!(
        count_named(&second.recent(&dir_a), "survivor.md"),
        1,
        "opening a SECOND MruStore on a state dir that already holds rows must not \
         recreate the database: survivor.md is gone, so the second open deleted \
         file-mru.sqlite. In production that is a second Wolf window silently wiping \
         the user's recent files."
    );
    assert_eq!(
        count_named(&first.recent(&dir_a), "survivor.md"),
        1,
        "the FIRST handle must still see its own row after the second open. If it does \
         not, the database file was unlinked from under its live connection and every \
         further write from that handle is lost."
    );
}

/// Several `MruStore::open`s racing on ONE state dir must not lose rows.
///
/// `CONCURRENT_OPENS` threads open their own handle against the same state dir at
/// the same instant — a [`Barrier`] makes the opens genuinely simultaneous, which
/// is what puts them on the one piece of SQLite work in the open path that needs
/// an exclusive lock: converting a fresh, rollback-journal database to WAL. The
/// main thread joins that same rendezvous, opens its own handle, and records
/// `CONCURRENT_FILES` rows; a second barrier then releases every thread to read.
///
/// The contract: a handle that loses that lock race must BACK OFF, not read the
/// transient failure as corruption and delete the database. One handle recreating
/// the file makes every already-recorded row vanish — for the readers AND for the
/// main handle, whose connection is left writing to an unlinked inode. So every
/// reader must see all `CONCURRENT_FILES` rows, and the main handle must still see
/// them after the join.
#[test]
fn concurrent_opens_preserve_rows() {
    let state = state_tempdir();
    let (_dir, dir_a) = canon_tempdir();

    let mut files = Vec::with_capacity(CONCURRENT_FILES);
    for i in 0..CONCURRENT_FILES {
        let path = dir_a.join(format!("conc_{i:02}.md"));
        touch(&path);
        files.push(path);
    }

    // `+ 1` for the main thread, which takes part in both rendezvous so its own
    // open races the readers' opens and its writes overlap them.
    let opened = Arc::new(Barrier::new(CONCURRENT_OPENS + 1));
    let recorded = Arc::new(Barrier::new(CONCURRENT_OPENS + 1));

    let mut readers = Vec::with_capacity(CONCURRENT_OPENS);
    for i in 0..CONCURRENT_OPENS {
        let state_path = state.path().to_path_buf();
        let dir = dir_a.clone();
        let opened = Arc::clone(&opened);
        let recorded = Arc::clone(&recorded);
        readers.push(thread::spawn(move || {
            opened.wait();
            let store = MruStore::open(&state_path);
            // Read only once every open has finished, so a short read is genuine data
            // loss and never just "the writer had not written that row yet".
            recorded.wait();
            (i, store.recent(&dir).len())
        }));
    }

    opened.wait();
    let store = MruStore::open(state.path());
    for file in &files {
        store.record(&dir_a, file);
    }
    recorded.wait();

    for reader in readers {
        let (i, seen) = reader.join().expect("a reader thread panicked");
        assert_eq!(
            seen, CONCURRENT_FILES,
            "reader {i} saw {seen} of {CONCURRENT_FILES} rows: a concurrent \
             MruStore::open recreated the database instead of backing off on a \
             transient lock failure"
        );
    }

    assert_eq!(
        store.recent(&dir_a).len(),
        CONCURRENT_FILES,
        "the original handle must still see all {CONCURRENT_FILES} rows after \
         {CONCURRENT_OPENS} concurrent opens; a shortfall means another handle \
         deleted file-mru.sqlite out from under this connection"
    );
}

/// An `MruStore::open` that LOSES the database lock outright must preserve every
/// existing row.
///
/// The DETERMINISTIC counterpart to [`concurrent_opens_preserve_rows`]. That test
/// manufactures contention by racing real threads, so it can only ever catch the
/// bug probabilistically — on a fast machine every thread may win. This one holds
/// the lock explicitly, from a second real SQLite connection, so the contended open
/// is GUARANTEED to fail; it goes red the moment `warrants_recreate` starts
/// classifying `SQLITE_BUSY` as corruption again.
///
/// # Why the fixture converts the database BACK to rollback-journal mode
///
/// This step is the whole test, not boilerplate. `BEGIN EXCLUSIVE` against a WAL
/// database does nothing useful here: in WAL mode SQLite treats `EXCLUSIVE` as
/// `IMMEDIATE`, readers are not blocked, and the store's
/// `CREATE TABLE IF NOT EXISTS` is a no-op that takes no write lock — so the
/// contended `open` would simply SUCCEED and this test would be green with OR
/// without the fix. The one piece of work in the open path that genuinely needs an
/// exclusive lock is the rollback-journal -> WAL CONVERSION, and it only runs on a
/// database that is not already WAL. Hence the raw `PRAGMA journal_mode=DELETE;`
/// before the blocker is taken. Do not "simplify" it away.
///
/// # Why it is neither slow nor flaky
///
/// `MRU_BUSY_TIMEOUT_MS` (2 s) is installed before `journal_mode`, so the very
/// worst case is one 2 s wait before the open gives up — comfortably inside a
/// `size = "small"` target, and bounded rather than racy. Do NOT "fix" the runtime
/// by removing or shortening that busy timeout: its ordering before `journal_mode`
/// is the thing [`concurrent_opens_preserve_rows`] depends on.
///
/// # The contract
///
/// The contended handle must back off with no connection, leave the file alone, and
/// self-heal by lazily re-opening on its next use. Pre-fix, `warrants_recreate`
/// matched everything, so the failed open DELETED `file-mru.sqlite` and recreated an
/// empty one — the blocker's lock was left on the now-unlinked inode, so the retry
/// succeeded and the wipe was silent. That is one Wolf window erasing another's (and
/// the user's) entire recent-files history.
#[test]
fn an_open_that_loses_the_lock_race_preserves_rows() {
    let state = state_tempdir();
    let (_dir, dir_a) = canon_tempdir();
    let db = state.path().join(MRU_DB_NAME);

    let mut files = Vec::with_capacity(LOCKED_FILES);
    for i in 0..LOCKED_FILES {
        let path = dir_a.join(format!("locked_{i:02}.md"));
        touch(&path);
        files.push(path);
    }

    // Write rows only through the public API and drop the handle, so no Wolf connection stays live
    // and the test never hand-rolls the schema, `directory` normalization or `opened_at` units.
    {
        let store = MruStore::open(state.path());
        for file in &files {
            store.record(&dir_a, file);
        }
        assert_eq!(
            store.recent(&dir_a).len(),
            LOCKED_FILES,
            "precondition: the store must read back all {LOCKED_FILES} rows it just recorded"
        );
    }

    // Convert BACK to rollback-journal mode (see the doc comment: without this the
    // blocker below cannot block anything the open path actually does).
    {
        let convert = rusqlite::Connection::open(&db)
            .expect("failed to open the MRU DB to convert it to rollback-journal mode");
        convert
            .execute_batch("PRAGMA journal_mode=DELETE;")
            .expect("failed to set journal_mode=DELETE on the MRU DB");
    }

    // Take a real EXCLUSIVE lock from a second connection. In rollback-journal mode
    // that blocks readers AND writers, so the store's open is guaranteed to lose.
    let blocker = rusqlite::Connection::open(&db).expect("failed to open the blocking connection");
    blocker
        .execute_batch("BEGIN EXCLUSIVE")
        .expect("failed to take an EXCLUSIVE lock on the MRU DB");

    // The contended open. It must fail to open, back off, and leave the file alone.
    let contended = MruStore::open(state.path());

    assert!(
        db.exists(),
        "an MruStore::open that lost the database lock must NOT delete {}. The pre-fix code \
         classified a transient SQLITE_BUSY as corruption and removed file-mru.sqlite plus its \
         -wal/-shm sidecars; in production that is one Wolf window wiping another window's — and \
         the user's — entire recent-files history",
        db.display()
    );

    blocker
        .execute_batch("ROLLBACK")
        .expect("failed to release the EXCLUSIVE lock");
    // Explicit: the assertions below need the connection GONE, not merely rolled
    // back, so the store's lazy re-open sees an unlocked file.
    drop(blocker);

    assert_eq!(
        contended.recent(&dir_a).len(),
        LOCKED_FILES,
        "the handle whose open lost the lock race must self-heal by lazily re-opening once the \
         lock is released, and then see all {LOCKED_FILES} rows. A shortfall means the failed \
         open recreated the database instead of backing off"
    );

    let reopened = MruStore::open(state.path());
    assert_eq!(
        reopened.recent(&dir_a).len(),
        LOCKED_FILES,
        "a FRESH MruStore::open must still find all {LOCKED_FILES} rows, proving they survived \
         ON DISK and not merely inside a cached handle"
    );
}

/// A GENUINELY corrupt database is still healed: the store recreates it and works
/// normally afterwards.
///
/// The counterweight to [`a_second_store_open_does_not_destroy_existing_rows`] and
/// [`concurrent_opens_preserve_rows`]. Those pin that a transient failure must not
/// delete the database, and the obvious over-correction is to stop recreating it
/// at all — which would leave a user whose `file-mru.sqlite` was truncated by a
/// crash, or written by an older schema, with a permanently dead cache. Garbage
/// bytes where the SQLite header belongs make the open fail with "file is not a
/// database", which is exactly the case the recreation path exists for: `open`
/// must swallow it, recreate the schema, and `record`/`recent` must then work.
#[test]
fn a_corrupt_database_is_still_healed() {
    let state = state_tempdir();
    let db = state.path().join(MRU_DB_NAME);

    fs::write(&db, vec![b'Z'; GARBAGE_LEN]).expect("failed to write garbage over the MRU DB");

    let store = MruStore::open(state.path());

    let (_dir, dir_a) = canon_tempdir();
    let file = dir_a.join("after_heal.md");
    touch(&file);
    store.record(&dir_a, &file);

    let recent = store.recent(&dir_a);
    assert_eq!(
        recent.len(),
        1,
        "a corrupt file-mru.sqlite must be RECREATED, not left unusable: the row \
         recorded after the heal did not read back"
    );
    assert!(
        ends_with(&recent[0].path, "after_heal.md"),
        "the recreated database should hold the row recorded after the heal"
    );
    assert!(
        db.exists(),
        "the healed database should exist again at {}",
        db.display()
    );
}

/// A VALID SQLite file carrying the WRONG schema is silently recreated.
///
/// Deliberately shares its name with the identical guards in
/// `crates/wolf-core/tests/command_mru.rs` and `crates/wolf-core/tests/recent_dirs.rs`
/// so the three stores' wrong-schema coverage is greppable as one set.
///
/// # Why this is NOT redundant with [`a_corrupt_database_is_still_healed`]
///
/// They exercise two DIFFERENT `rusqlite::Error` VARIANTS, and the store's
/// classifier has to match both:
///
/// * Garbage bytes make the open fail with `SQLITE_NOTADB`, which rusqlite surfaces
///   as `Error::SqliteFailure`.
/// * A valid file with the wrong schema survives `CREATE TABLE IF NOT EXISTS` and
///   only fails at the schema probe's `prepare`, where SQLite can attribute the
///   failure to a byte offset in the SQL text — so rusqlite converts the
///   `ErrorCode::Unknown` into `Error::SqlInputError` instead
///   (`rusqlite-0.31.0/src/error.rs:430-448`).
///
/// A classifier that matches only `SqliteFailure` passes the corrupt-file test and
/// silently disables the wrong-schema recreation path entirely. That exact
/// over-narrowing was found in `command_mru`; without this test the identical bug in
/// `mru.rs` would have shipped undetected, leaving anyone whose `file-mru.sqlite`
/// still holds the OLD single-`path` schema with a permanently dead cache.
///
/// The fixture is that historically real wrong schema (`mru.rs`'s own probe comment
/// names it): a `mru` table with a single `path` column, missing every column the
/// probe `SELECT directory, file, opened_at FROM mru LIMIT 0` reads.
#[test]
fn wrong_schema_db_is_silently_recreated() {
    let state = state_tempdir();
    let db = state.path().join(MRU_DB_NAME);

    // A valid sqlite DB whose `mru` table has the old single-`path` schema: `CREATE TABLE IF NOT
    // EXISTS` cannot repair it, so only the schema probe can surface it as an error the caller heals.
    {
        let conn = rusqlite::Connection::open(&db).expect("failed to open sqlite for bad schema");
        conn.execute_batch("CREATE TABLE mru (path TEXT PRIMARY KEY);")
            .expect("failed to create the wrong-schema mru table");
    }

    // Must not panic, and must read as empty: the file was recreated from scratch.
    let store = MruStore::open(state.path());

    let (_dir, dir_a) = canon_tempdir();
    assert!(
        store.recent(&dir_a).is_empty(),
        "a store opened over a wrong-schema file-mru.sqlite must read as empty after the heal"
    );

    // And it must be usable afterwards, through the ordinary public API.
    let file = dir_a.join("healed.md");
    touch(&file);
    store.record(&dir_a, &file);

    let recent = store.recent(&dir_a);
    assert_eq!(
        recent.len(),
        1,
        "record must round-trip after the schema heal: a wrong-schema file-mru.sqlite must be \
         RECREATED, not left permanently unusable"
    );
    assert!(
        ends_with(&recent[0].path, "healed.md"),
        "the recreated database should hold the row recorded after the heal"
    );

    assert!(
        db.exists(),
        "the sqlite file must exist again after the schema heal, at {}",
        db.display()
    );
}
