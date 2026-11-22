//! Integration tests for the shared state database `wolf.sqlite` that the three cache stores
//! (`wolf_core::MruStore`, `wolf_core::RecentDirs`, `wolf_core::CommandMru`) live in, against
//! real SQLite files in real temporary directories. No mocks, no Neovim, no `WOLF_STATE_DIR`:
//! each test passes a tempdir to the stores' `open` as the state directory.
//!
//! They pin the storage rules of `specs/application-state.md` §1 for the cache class:
//!
//! - All three stores are tables (`file_mru`, `folder_mru`, `command_mru`) of one `wolf.sqlite`,
//!   and a `schema` table holds one row per table with its version.
//! - A cache table whose `schema` row holds any version other than this build's is dropped and
//!   recreated, and no other table loses a row.
//! - A file that is not a database is deleted with its `-wal` and `-shm` sidecars and recreated;
//!   the test observes the `-shm` half.
//! - The legacy `file-mru.sqlite`, `folder-mru.sqlite` and `command-mru.sqlite` and their
//!   sidecars are deleted at open, and none of their rows is imported.
//! - Concurrent opens of every store from several handles lose no row.
//!
//! The busy-database rule is pinned in `tests/mru.rs`
//! (`an_open_that_loses_the_lock_race_preserves_rows`), and the table-without-a-`schema`-row
//! rule by each store's own `wrong_schema_db_is_silently_recreated`.
//!
//! The tests reach the database only through the stores' public API plus raw
//! `rusqlite::Connection`s that read or tamper with `wolf.sqlite`. The version a test tampers
//! with is read from `schema` after a normal open, never hard-coded. The file and table names
//! are literals from the spec, not crate constants, so a rename goes red.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;

use rusqlite::{Connection, OptionalExtension};
use wolf_core::{CommandMru, MruStore, RecentDirs};

/// The shared database's file name, from `specs/application-state.md` §1.
const DB_NAME: &str = "wolf.sqlite";

/// The three cache tables of §1, in the order `SELECT name FROM schema ORDER BY name` returns.
const CACHE_TABLES: [&str; 3] = ["command_mru", "file_mru", "folder_mru"];

/// The legacy per-store databases that §1 "Files Wolf removes" deletes at launch.
const LEGACY_DB_NAMES: [&str; 3] = ["file-mru.sqlite", "folder-mru.sqlite", "command-mru.sqlite"];

/// The sidecars SQLite keeps beside a database, which every deletion must take along.
const SIDECARS: [&str; 2] = ["-wal", "-shm"];

/// The neutral name each legacy fixture is built at before it is renamed into place, so no
/// file under `crates/` opens a database named `*-mru.sqlite`.
const SEED_NAME: &str = "seed.db";

/// How many garbage bytes [`a_file_that_is_not_a_database_is_replaced`] writes over
/// `wolf.sqlite`: well past SQLite's 100-byte header, so the file is not a database rather than
/// a zero-length file SQLite would adopt.
const GARBAGE_LEN: usize = 4096;

/// The first 16 bytes of every SQLite 3 database file.
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// How many handles of each store kind race in
/// [`concurrent_opens_of_all_three_stores_keep_every_row`], besides the main thread's.
const THREADS_PER_STORE: usize = 4;

/// How many rows the main thread records into each store in
/// [`concurrent_opens_of_all_three_stores_keep_every_row`]. Under every cap, and more than one,
/// so a partial loss is distinguishable from a total one.
const CONCURRENT_ROWS: usize = 5;

/// One of the three cache stores, named by its table.
#[derive(Clone, Copy, Debug)]
enum Kind {
    File,
    Folder,
    Command,
}

const KINDS: [Kind; 3] = [Kind::File, Kind::Folder, Kind::Command];

impl Kind {
    /// The store's table in `wolf.sqlite`, from §1.
    fn table(self) -> &'static str {
        match self {
            Kind::File => "file_mru",
            Kind::Folder => "folder_mru",
            Kind::Command => "command_mru",
        }
    }

    /// Opens a handle of this kind on the state dir `state`.
    fn open(self, state: &Path) -> Store {
        match self {
            Kind::File => Store::File(MruStore::open(state)),
            Kind::Folder => Store::Folder(RecentDirs::open(state)),
            Kind::Command => Store::Command(CommandMru::open(state)),
        }
    }
}

/// A handle of any of the three stores.
enum Store {
    File(MruStore),
    Folder(RecentDirs),
    Command(CommandMru),
}

impl Store {
    /// How many rows the store reads back; `project` scopes the file MRU.
    fn len(&self, project: &Path) -> usize {
        match self {
            Store::File(store) => store.recent(project).len(),
            Store::Folder(store) => store.recent().len(),
            Store::Command(store) => store.recent().len(),
        }
    }
}

/// One handle of each store, all on one state dir.
struct Stores {
    files: MruStore,
    folders: RecentDirs,
    commands: CommandMru,
}

fn open_all(state: &Path) -> Stores {
    Stores {
        files: MruStore::open(state),
        folders: RecentDirs::open(state),
        commands: CommandMru::open(state),
    }
}

/// A project directory with one file on disk, for the file MRU and the folder MRU to record.
///
/// The directory is canonical, because `MruStore` canonicalizes the `directory` it is given and
/// macOS temp dirs sit behind `/var -> /private/var`.
struct Project {
    _guard: tempfile::TempDir,
    dir: PathBuf,
    file: PathBuf,
}

fn project() -> Project {
    let guard = tempfile::tempdir().expect("failed to create the project tempdir");
    let dir = fs::canonicalize(guard.path()).expect("failed to canonicalize the project tempdir");
    let file = dir.join("seeded.md");
    fs::write(&file, b"x").expect("failed to create the project file");
    Project {
        _guard: guard,
        dir,
        file,
    }
}

/// The command key [`seed_one_row_each`] records.
const SEEDED_COMMAND: &str = "seeded_command";

/// Records one row in each store through the public API, then drops every handle, so no Wolf
/// connection stays live while a test tampers with the file.
fn seed_one_row_each(state: &Path, project: &Project) {
    let stores = open_all(state);
    stores.files.record(&project.dir, &project.file);
    stores.folders.record(&project.dir);
    stores.commands.record(SEEDED_COMMAND);
    assert_seeded_rows(&stores, project, "precondition, right after recording");
}

/// Asserts that each store reads back exactly the row [`seed_one_row_each`] recorded.
fn assert_seeded_rows(stores: &Stores, project: &Project, when: &str) {
    let files: Vec<PathBuf> = stores
        .files
        .recent(&project.dir)
        .into_iter()
        .map(|e| e.path)
        .collect();
    assert_eq!(
        files,
        vec![project.file.clone()],
        "{when}: the file MRU must hold exactly the seeded file; an empty list means the \
         file_mru table lost its row, and any other file means the fixture is wrong"
    );
    assert_eq!(
        stores.folders.recent(),
        vec![project.dir.clone()],
        "{when}: the folder MRU must hold exactly the seeded directory; an empty list means the \
         folder_mru table lost its row"
    );
    assert_eq!(
        stores.commands.recent(),
        vec![SEEDED_COMMAND.to_owned()],
        "{when}: the command MRU must hold exactly the seeded key; an empty list means the \
         command_mru table lost its row"
    );
}

/// A raw connection to `state/wolf.sqlite`, outside every store.
fn raw(state: &Path) -> Connection {
    Connection::open(state.join(DB_NAME)).expect("failed to open wolf.sqlite with a raw connection")
}

/// The version `schema` records for `table`, or `None` when it has no row.
fn schema_version(conn: &Connection, table: &str) -> Option<i64> {
    conn.query_row(
        "SELECT version FROM schema WHERE name = ?1",
        [table],
        |row| row.get(0),
    )
    .optional()
    .expect("failed to read the schema table of wolf.sqlite")
}

/// How many rows `table` holds, read through a raw connection.
fn row_count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap_or_else(|err| panic!("failed to count the rows of {table} in wolf.sqlite: {err}"))
}

/// `path` with `suffix` appended to its file name, as SQLite names a sidecar.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// The entries of `dir` whose names contain `.sqlite`, sorted.
fn sqlite_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("failed to list the state dir")
        .map(|entry| {
            entry
                .expect("failed to read a state dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.contains(".sqlite"))
        .collect();
    names.sort();
    names
}

/// All three stores live in one `wolf.sqlite`, whose `schema` table names each of their tables,
/// and each table holds the row its store recorded.
#[test]
fn all_three_stores_share_one_wolf_sqlite() {
    let state = tempfile::tempdir().expect("failed to create the state tempdir");
    let project = project();

    let stores = open_all(state.path());
    stores.files.record(&project.dir, &project.file);
    stores.folders.record(&project.dir);
    stores.commands.record(SEEDED_COMMAND);

    let db = state.path().join(DB_NAME);
    assert!(
        db.exists(),
        "the stores must keep their rows in {}, which does not exist; they write elsewhere",
        db.display()
    );
    let allowed: Vec<String> = std::iter::once(DB_NAME.to_owned())
        .chain(SIDECARS.iter().map(|s| format!("{DB_NAME}{s}")))
        .collect();
    let names = sqlite_names(state.path());
    let strays: Vec<&String> = names.iter().filter(|n| !allowed.contains(n)).collect();
    assert!(
        strays.is_empty(),
        "the state dir must hold no database but wolf.sqlite and its sidecars; it also holds \
         {strays:?}, so a store still opens a database of its own"
    );

    let conn = raw(state.path());
    let mut stmt = conn
        .prepare("SELECT name FROM schema ORDER BY name")
        .expect("wolf.sqlite must have a schema table");
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("failed to query the schema table")
        .collect::<rusqlite::Result<_>>()
        .expect("failed to read a schema row");
    assert_eq!(
        tables, CACHE_TABLES,
        "schema must hold exactly one row per cache table; a missing name means that store's \
         table is not registered, and an extra one means a table is registered twice or under \
         another name"
    );
    for table in CACHE_TABLES {
        assert_eq!(
            row_count(&conn, table),
            1,
            "{table} in wolf.sqlite must hold the one row its store recorded; 0 means the store \
             wrote somewhere else"
        );
    }
}

/// Seeds one row per store, sets the `schema` version of `table` to `version(v)` for this
/// build's `v`, and reopens all three stores. Returns the reopened stores and `v`.
fn retag_and_reopen(
    state: &Path,
    project: &Project,
    table: &str,
    version: impl Fn(i64) -> i64,
) -> (Stores, i64) {
    seed_one_row_each(state, project);
    let current = {
        let conn = raw(state);
        let current = schema_version(&conn, table)
            .unwrap_or_else(|| panic!("precondition: schema must hold a row for {table}"));
        conn.execute(
            "UPDATE schema SET version = ?1 WHERE name = ?2",
            rusqlite::params![version(current), table],
        )
        .expect("failed to retag the schema row");
        current
    };
    (open_all(state), current)
}

/// A cache table whose `schema` row holds a version newer than this build's is dropped and
/// recreated, its `schema` row is written back at this build's version, and the other two
/// tables keep their rows.
#[test]
fn a_cache_table_at_a_newer_version_is_recreated_alone() {
    let state = tempfile::tempdir().expect("failed to create the state tempdir");
    let project = project();

    let (stores, current) = retag_and_reopen(state.path(), &project, "folder_mru", |v| v + 1);

    assert!(
        stores.folders.recent().is_empty(),
        "folder_mru was tagged at version {} while this build is at {current}, so it must be \
         dropped and read empty; it still holds {:?}, so a newer version was taken for a match",
        current + 1,
        stores.folders.recent()
    );
    assert_eq!(
        stores.files.recent(&project.dir).len(),
        1,
        "file_mru is at this build's version and must keep its row; it lost it, so recreating \
         folder_mru dropped other tables too"
    );
    assert_eq!(
        stores.commands.recent(),
        vec![SEEDED_COMMAND.to_owned()],
        "command_mru is at this build's version and must keep its row; it lost it, so \
         recreating folder_mru dropped other tables too"
    );
    assert_eq!(
        schema_version(&raw(state.path()), "folder_mru"),
        Some(current),
        "after recreating folder_mru its schema row must read this build's version {current}; \
         {} means the row was not rewritten, and None means it was deleted",
        current + 1
    );

    let healed = project.dir.join("after_heal");
    fs::create_dir(&healed).expect("failed to create the post-heal directory");
    stores.folders.record(&healed);
    drop(stores);
    assert_eq!(
        RecentDirs::open(state.path()).recent(),
        vec![healed.clone()],
        "a row recorded after the recreate must survive dropping every handle and reopening; \
         an empty list means the reopen dropped folder_mru again, because its schema row still \
         names another version"
    );
}

/// A cache table whose `schema` row holds a version older than this build's is dropped and
/// recreated, and the other two tables keep their rows.
#[test]
fn a_cache_table_at_an_older_version_is_recreated_alone() {
    let state = tempfile::tempdir().expect("failed to create the state tempdir");
    let project = project();

    let (stores, current) = retag_and_reopen(state.path(), &project, "file_mru", |v| v - 1);

    let files = stores.files.recent(&project.dir);
    assert!(
        files.is_empty(),
        "file_mru was tagged at version {} while this build is at {current}, so it must be \
         dropped and read empty; it still holds {files:?}, so an older version was taken for a \
         match",
        current - 1
    );
    assert_eq!(
        stores.folders.recent(),
        vec![project.dir.clone()],
        "folder_mru is at this build's version and must keep its row; it lost it, so \
         recreating file_mru dropped other tables too"
    );
    assert_eq!(
        stores.commands.recent(),
        vec![SEEDED_COMMAND.to_owned()],
        "command_mru is at this build's version and must keep its row; it lost it, so \
         recreating file_mru dropped other tables too"
    );
    assert_eq!(
        schema_version(&raw(state.path()), "file_mru"),
        Some(current),
        "after recreating file_mru its schema row must read this build's version {current}"
    );
}

/// A `wolf.sqlite` that is not a database is deleted with its sidecars and recreated, and the
/// store works on the new file. Of the sidecars, the test observes the `-shm`.
///
/// The garbage `-shm` is hard-linked to a witness name first, so its inode stays allocated and a
/// recreated `-shm` cannot reuse its number: a `-shm` that was deleted is then either absent or
/// carries another inode, while one SQLite merely opened in place keeps the witness's.
///
/// Premises, measured against SQLite 3.45.0, the version rusqlite bundles, by probing `db.rs`
/// with this test's fixture shape:
///
/// - A garbage `-wal` beside the garbage main file makes the failing open read through WAL
///   mode, and SQLite deletes both sidecars itself when that connection closes. That would hide
///   the heal, so the fixture writes no `-wal`.
/// - With no `-wal`, the failing open leaves the `-shm` alone. Over the fresh main file, SQLite
///   opens the stale `-shm` in place and keeps its inode. So the inode tells a heal that deleted
///   the sidecars from one that deleted the main file alone.
/// - Over a fresh, zero-page main file, SQLite deletes a stale `-wal` by itself, so no fixture
///   here can observe the `-wal` half of the heal.
/// - The last connection's close may unlink the `-shm`, depending on how SQLite was built,
///   which would hide the difference, so the `-shm` is checked while the store's handle is
///   still alive.
#[test]
fn a_file_that_is_not_a_database_is_replaced() {
    let state = tempfile::tempdir().expect("failed to create the state tempdir");
    let db = state.path().join(DB_NAME);

    fs::write(&db, vec![b'Z'; GARBAGE_LEN]).expect("failed to write garbage over wolf.sqlite");
    let shm = with_suffix(&db, "-shm");
    fs::write(&shm, b"stale sidecar").expect("failed to write a garbage -shm");
    let witness = state.path().join("witness-shm");
    fs::hard_link(&shm, &witness).expect("failed to hard-link the -shm witness");

    let store = CommandMru::open(state.path());
    store.record("after_heal");
    assert_eq!(
        store.recent(),
        vec!["after_heal".to_owned()],
        "a wolf.sqlite that is not a database must be replaced so record and recent work; an \
         empty list means the file was left in place and every query fails"
    );

    let mut header = [0u8; 16];
    let bytes = fs::read(&db).expect("wolf.sqlite must exist after the heal");
    let len = bytes.len().min(header.len());
    header[..len].copy_from_slice(&bytes[..len]);
    assert_eq!(
        &header, SQLITE_HEADER,
        "wolf.sqlite must be a SQLite database after the heal; it starts with {header:?}"
    );

    if let Ok(meta) = fs::metadata(&shm) {
        let old = fs::metadata(&witness)
            .expect("the witness must survive")
            .ino();
        assert_ne!(
            meta.ino(),
            old,
            "wolf.sqlite-shm still has the garbage -shm's inode, so the heal deleted the main \
             file but not its -shm sidecar (a deleted -shm is absent or has a new inode)"
        );
    }
    drop(store);
}

/// Builds the three legacy databases in `state`, each holding one row that names `project`, and
/// writes stale bytes to each of their `-wal` and `-shm` sidecars.
///
/// Each database is built at [`SEED_NAME`] and renamed into place. Their schemas are the ones
/// the per-store databases had, so an import would find rows to copy. The stale sidecars do not
/// stop one: an `ATTACH` of such a file still reads its row, as measured against both
/// Python's SQLite 3.54 and the SQLite 3.45.0 that rusqlite bundles.
fn seed_legacy_files(state: &Path, project: &Project) {
    let dir = project.dir.to_string_lossy().into_owned();
    let file = project.file.to_string_lossy().into_owned();
    let seed = state.join(SEED_NAME);
    for name in LEGACY_DB_NAMES {
        {
            let conn = Connection::open(&seed).expect("failed to create a legacy fixture");
            match name {
                "file-mru.sqlite" => {
                    conn.execute_batch(
                        "CREATE TABLE mru (directory TEXT NOT NULL, file TEXT NOT NULL, \
                         opened_at INTEGER NOT NULL, PRIMARY KEY (directory, file));",
                    )
                    .expect("failed to create the legacy mru table");
                    conn.execute(
                        "INSERT INTO mru (directory, file, opened_at) VALUES (?1, ?2, 1)",
                        [dir.as_str(), file.as_str()],
                    )
                    .expect("failed to seed the legacy mru row");
                }
                "folder-mru.sqlite" => {
                    conn.execute_batch(
                        "CREATE TABLE recent_dirs (path TEXT PRIMARY KEY, seq INTEGER NOT NULL);",
                    )
                    .expect("failed to create the legacy recent_dirs table");
                    conn.execute(
                        "INSERT INTO recent_dirs (path, seq) VALUES (?1, 1)",
                        [dir.as_str()],
                    )
                    .expect("failed to seed the legacy recent_dirs row");
                }
                _ => {
                    conn.execute_batch(
                        "CREATE TABLE command_mru (key TEXT PRIMARY KEY, seq INTEGER NOT NULL); \
                         INSERT INTO command_mru (key, seq) VALUES ('legacy_command', 1);",
                    )
                    .expect("failed to seed the legacy command_mru table");
                }
            }
        }
        let legacy = state.join(name);
        fs::rename(&seed, &legacy).expect("failed to move a legacy fixture into place");
        for suffix in SIDECARS {
            fs::write(with_suffix(&legacy, suffix), b"stale sidecar")
                .expect("failed to write a legacy sidecar");
        }
    }
}

/// Every legacy database path: the three main files and their sidecars.
fn legacy_paths(state: &Path) -> Vec<PathBuf> {
    LEGACY_DB_NAMES
        .iter()
        .flat_map(|name| {
            let main = state.join(name);
            let sidecars: Vec<PathBuf> = SIDECARS.iter().map(|s| with_suffix(&main, s)).collect();
            std::iter::once(main).chain(sidecars)
        })
        .collect()
}

/// Opening any one store deletes the three legacy databases and their sidecars, and none of
/// their rows reaches `wolf.sqlite`.
///
/// It runs once per store kind, each in a fresh state dir, because each store's `open` is a
/// separate path to the removal and to any import, and only the first open finds the files.
#[test]
fn legacy_mru_files_are_removed_at_open_and_nothing_is_imported() {
    for kind in KINDS {
        let state = tempfile::tempdir().expect("failed to create the state tempdir");
        let project = project();
        seed_legacy_files(state.path(), &project);

        let first = kind.open(state.path());

        let left: Vec<PathBuf> = legacy_paths(state.path())
            .into_iter()
            .filter(|p| p.exists())
            .collect();
        assert!(
            left.is_empty(),
            "opening the {} store must delete every legacy database and its sidecars; {left:?} \
             are left. Only sidecars left means the removal skips them; main files too means \
             this open removes nothing",
            kind.table()
        );
        assert!(
            state.path().join(DB_NAME).exists(),
            "opening the {} store must create wolf.sqlite",
            kind.table()
        );

        let stores = open_all(state.path());
        let files = stores.files.recent(&project.dir);
        assert!(
            files.is_empty(),
            "after opening the {} store first, the file MRU must read empty; it holds {files:?}, \
             imported from the legacy file-mru database",
            kind.table()
        );
        assert!(
            stores.folders.recent().is_empty(),
            "after opening the {} store first, the folder MRU must read empty; it holds {:?}, \
             imported from the legacy folder-mru database",
            kind.table(),
            stores.folders.recent()
        );
        assert!(
            stores.commands.recent().is_empty(),
            "after opening the {} store first, the command MRU must read empty; it holds {:?}, \
             imported from the legacy command-mru database",
            kind.table(),
            stores.commands.recent()
        );
        drop(first);
    }
}

/// Handles of all three stores opened at once on a state dir with no database yet lose no row:
/// every handle, the racing ones and the writer's, reads back every row its store recorded.
///
/// A [`Barrier`] makes the opens simultaneous, so they contend for the creation of the file,
/// its conversion to WAL and the `schema` bookkeeping; the main thread's open races them too.
/// A second barrier holds every read until the main thread has recorded, so a short read is
/// lost data and never a row not yet written.
#[test]
fn concurrent_opens_of_all_three_stores_keep_every_row() {
    let state = tempfile::tempdir().expect("failed to create the state tempdir");
    let project = project();

    let mut files = Vec::with_capacity(CONCURRENT_ROWS);
    let mut folders = Vec::with_capacity(CONCURRENT_ROWS);
    for i in 0..CONCURRENT_ROWS {
        let file = project.dir.join(format!("conc_{i}.md"));
        fs::write(&file, b"x").expect("failed to create a concurrent fixture file");
        files.push(file);
        let folder = project.dir.join(format!("conc_{i}"));
        fs::create_dir(&folder).expect("failed to create a concurrent fixture directory");
        folders.push(folder);
    }

    let parties = KINDS.len() * THREADS_PER_STORE + 1;
    let opened = Arc::new(Barrier::new(parties));
    let recorded = Arc::new(Barrier::new(parties));

    let mut readers = Vec::new();
    for kind in KINDS {
        for i in 0..THREADS_PER_STORE {
            let state = state.path().to_path_buf();
            let dir = project.dir.clone();
            let opened = Arc::clone(&opened);
            let recorded = Arc::clone(&recorded);
            readers.push(thread::spawn(move || {
                opened.wait();
                let store = kind.open(&state);
                recorded.wait();
                (kind, i, store.len(&dir))
            }));
        }
    }

    opened.wait();
    let stores = open_all(state.path());
    for (i, (file, folder)) in files.iter().zip(&folders).enumerate() {
        stores.files.record(&project.dir, file);
        stores.folders.record(folder);
        stores.commands.record(&format!("conc_command_{i}"));
    }
    recorded.wait();

    for reader in readers {
        let (kind, i, seen) = reader.join().expect("a reader thread panicked");
        assert_eq!(
            seen,
            CONCURRENT_ROWS,
            "racing {} handle {i} read {seen} of {CONCURRENT_ROWS} rows. 0 means it could not \
             open or read the database while the others held it; a partial count means a \
             concurrent open dropped or recreated the table after rows were written",
            kind.table()
        );
    }
    let main = [
        (Kind::File, stores.files.recent(&project.dir).len()),
        (Kind::Folder, stores.folders.recent().len()),
        (Kind::Command, stores.commands.recent().len()),
    ];
    for (kind, seen) in main {
        assert_eq!(
            seen,
            CONCURRENT_ROWS,
            "the writing {} handle read {seen} of the {CONCURRENT_ROWS} rows it recorded while \
             {THREADS_PER_STORE} handles of each store opened beside it; its writes were dropped \
             or another open removed them",
            kind.table()
        );
    }
}
