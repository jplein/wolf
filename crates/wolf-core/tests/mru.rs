//! Direct integration tests for the S1 MRU cache (`wolf_core::mru::MruStore`)
//! against a REAL SQLite database on a REAL temporary filesystem. No mocks.
//!
//! These tests do NOT touch Neovim, so they do not require `WOLF_NVIM_BIN`. The
//! record-on-open path (driven through nvim) is covered separately in
//! `tests/mru_open_records.rs`.
//!
//! Surface exercised (per the S1 plan):
//! - `wolf_core::mru::MruStore::open(cwd: &Path) -> Result<MruStore, MruError>`
//! - `MruStore::record(&self, path: &Path)` (fire-and-forget upsert)
//! - `MruStore::recent(&self) -> Vec<wolf_core::FileEntry>`
//! - `wolf_core::FileEntry { path: PathBuf, display: String, mru_rank: Option<u32> }`
//!
//! NOTE to coder (ordering determinism): several tests assert that `recent()`
//! returns entries most-recent-first. This requires that two `record()` calls a
//! few milliseconds apart receive DISTINCT, monotonically increasing `opened_at`
//! values. Recommend millisecond (or finer) resolution for `opened_at` AND a
//! deterministic tiebreak (`ORDER BY opened_at DESC, rowid DESC`) so that inserts
//! that happen to share a timestamp still order by insertion recency. If the
//! implementation stores second-granularity timestamps without a tiebreak, these
//! tests will be flaky — that is a signal to fix the resolution/tiebreak, not the
//! tests. The tests insert with a small gap (`REC_GAP`) to make distinct
//! millisecond timestamps the common case.

use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use wolf_core::mru::MruStore;
use wolf_core::FileEntry;

/// Must match the implementation's `MRU_LIMIT`.
/// NOTE to coder: mirrored locally because the const's public path is not yet
/// pinned by the plan; keep these in sync (or re-export and import it here).
const MRU_LIMIT: usize = 50;

/// How many files test #5 records to exceed the cap.
const OVER_LIMIT: usize = 55;

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

#[test]
fn open_creates_sqlite_and_recent_is_empty() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = dir.path();

    let store = MruStore::open(cwd).expect("MruStore::open should succeed on a fresh cwd");

    let db = cwd.join(".wolf").join("mru.sqlite");
    assert!(
        db.exists(),
        "open() should create the cache at {}",
        db.display()
    );

    assert!(
        store.recent().is_empty(),
        "a freshly opened store should have no MRU entries"
    );
}

#[test]
fn records_appear_most_recent_first_with_ranks() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = dir.path();
    let store = MruStore::open(cwd).expect("MruStore::open should succeed");

    let a = cwd.join("a.md");
    let b = cwd.join("b.md");
    let c = cwd.join("c.md");
    for p in [&a, &b, &c] {
        touch(p);
    }

    // Record oldest -> newest with a gap so opened_at is distinct.
    store.record(&a);
    sleep(REC_GAP);
    store.record(&b);
    sleep(REC_GAP);
    store.record(&c);

    let recent = store.recent();
    assert_eq!(recent.len(), 3, "all three records should be present");

    // Most-recent first: c, b, a, with ascending ranks 0, 1, 2.
    assert!(ends_with(&recent[0].path, "c.md"), "newest should be first");
    assert!(ends_with(&recent[1].path, "b.md"));
    assert!(ends_with(&recent[2].path, "a.md"), "oldest should be last");

    assert_eq!(recent[0].mru_rank, Some(0), "rank 0 = most recent");
    assert_eq!(recent[1].mru_rank, Some(1));
    assert_eq!(recent[2].mru_rank, Some(2));
}

#[test]
fn re_recording_moves_entry_to_front_without_duplicating() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = dir.path();
    let store = MruStore::open(cwd).expect("MruStore::open should succeed");

    let a = cwd.join("a.md");
    let b = cwd.join("b.md");
    let c = cwd.join("c.md");
    for p in [&a, &b, &c] {
        touch(p);
    }

    store.record(&a);
    sleep(REC_GAP);
    store.record(&b);
    sleep(REC_GAP);
    store.record(&c);
    sleep(REC_GAP);
    // Re-record the OLDEST entry: it must jump to the front (rank 0) and the
    // list must still hold exactly three entries (upsert, not duplicate).
    store.record(&a);

    let recent = store.recent();
    assert_eq!(recent.len(), 3, "re-record must upsert, not add a duplicate");

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

#[test]
fn deleted_file_is_omitted_and_pruned_from_cache() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = dir.path();
    let store = MruStore::open(cwd).expect("MruStore::open should succeed");

    let keep = cwd.join("keep.md");
    let gone = cwd.join("gone.md");
    touch(&keep);
    touch(&gone);

    store.record(&keep);
    sleep(REC_GAP);
    store.record(&gone);
    assert_eq!(store.recent().len(), 2, "both records present before deletion");

    // Delete one file from disk; recent() must omit it (and prune the row).
    fs::remove_file(&gone).expect("failed to delete file");

    let recent = store.recent();
    assert_eq!(recent.len(), 1, "the missing file should be omitted");
    assert!(ends_with(&recent[0].path, "keep.md"));

    // Re-open a SECOND store on the same cwd: the pruned row must be gone for
    // good, proving recent() deleted it from the cache (not just filtered it).
    drop(store);
    let store2 = MruStore::open(cwd).expect("re-open on same cwd should succeed");
    let recent2 = store2.recent();
    assert_eq!(recent2.len(), 1, "pruned row must not reappear after re-open");
    assert!(
        !recent2.iter().any(|e| ends_with(&e.path, "gone.md")),
        "the deleted path must never reappear"
    );
}

#[test]
fn recent_is_capped_at_mru_limit() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = dir.path();
    let store = MruStore::open(cwd).expect("MruStore::open should succeed");

    // Record more than the cap, oldest -> newest, each on disk.
    let mut paths = Vec::with_capacity(OVER_LIMIT);
    for i in 0..OVER_LIMIT {
        let p = cwd.join(format!("file_{i:02}.md"));
        touch(&p);
        store.record(&p);
        sleep(REC_GAP);
        paths.push(p);
    }

    let recent = store.recent();
    assert_eq!(
        recent.len(),
        MRU_LIMIT,
        "recent() must return at most MRU_LIMIT entries"
    );

    // The most-recent MRU_LIMIT survive; the oldest (OVER_LIMIT - MRU_LIMIT) fall
    // off the end of the list.
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

#[test]
fn display_is_relative_to_cwd_and_path_is_absolute() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    // Canonicalize the cwd so that record()'s own canonicalization of the file
    // path yields a path under cwd (temp dirs can sit behind symlinks, e.g.
    // /tmp -> /private/tmp); otherwise relativization would be ambiguous.
    let cwd = fs::canonicalize(dir.path()).expect("failed to canonicalize cwd");
    let store = MruStore::open(&cwd).expect("MruStore::open should succeed");

    let nested = cwd.join("sub").join("a.md");
    touch(&nested);
    store.record(&nested);

    let recent = store.recent();
    assert_eq!(recent.len(), 1);
    let entry = &recent[0];

    assert!(
        entry.path.is_absolute(),
        "FileEntry.path should be absolute (used for Open); got {}",
        entry.path.display()
    );

    // NOTE to coder: `display` is the path relative to cwd, rendered with the
    // platform separator (so "sub/a.md" on unix). If the plan settles on a
    // different relativization (e.g. always '/'), update this assertion.
    let expected = Path::new("sub").join("a.md").to_string_lossy().into_owned();
    assert_eq!(
        entry.display, expected,
        "display should be the path relative to cwd"
    );
}
