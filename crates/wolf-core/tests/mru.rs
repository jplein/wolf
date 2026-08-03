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

use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
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
/// ever created under a project `directory` arg (the MRU no longer lives there).
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
