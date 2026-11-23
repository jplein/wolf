//! The shared state database `wolf.sqlite`: its open, the per-table versioning
//! kept in its `schema` table, its recovery, and the removal of the files it
//! replaces: `state.json`, `state.json.tmp` and the per-store MRU databases.
//!
//! The policy this module implements — which tables exist, what a version
//! mismatch does to each class, and which files are deleted when — is
//! `specs/application-state.md` §1. Each store declares a [`TableSpec`] and
//! reaches the database only through a [`Table`] built from it.
//!
//! A table is of one of two classes, its [`TablePolicy`]. A cache table at any
//! other version is dropped and recreated. A state table at an older version is
//! migrated in place by its spec's migrations; one at a newer version, written by
//! a newer build, is neither read nor written again by the [`Table`] that found it,
//! across reopens and heals, which is "for the rest of the process" because each
//! front-end opens one store per process.
//!
//! Every failure is silent: nothing panics, nothing propagates, and the only log
//! is `tracing::debug!`, carrying the table's name. All SQLite work is
//! synchronous and guarded by a [`parking_lot::Mutex`]; the critical sections are
//! short and never span an `.await`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

/// The database's file name under [`crate::state::state_dir`].
const DB_NAME: &str = "wolf.sqlite";

/// The files this database replaces, deleted on every [`Table::open_all`].
const LEGACY_FILE_NAMES: [&str; 5] = [
    "state.json",
    "state.json.tmp",
    "file-mru.sqlite",
    "folder-mru.sqlite",
    "command-mru.sqlite",
];

/// SQLite `busy_timeout` (milliseconds) so concurrent opens back off rather than
/// fail immediately with `SQLITE_BUSY`. Installed before anything else — see
/// [`connect`].
const BUSY_TIMEOUT_MS: u32 = 2000;

/// One step of a state table's migration, run inside [`ensure_table`]'s
/// transaction. A failed step rolls the whole open back.
pub(crate) type Migration = fn(&rusqlite::Transaction<'_>) -> rusqlite::Result<()>;

/// What [`ensure_table`] does to a table whose `schema` row names another
/// version than its [`TableSpec::version`], or that has no row at all.
pub(crate) enum TablePolicy {
    /// The table is disposable: drop it and create it again, empty.
    Cache,
    /// The table holds what the user would lose. `migrations[k]` takes it from
    /// version `k + 1` to `k + 2`, so [`TableSpec::version`] is
    /// `migrations.len() + 1`.
    ///
    /// An older table is migrated in place, keeping its rows. A newer one is left
    /// untouched, `schema` row included, and its [`Table`] never ensures, reads or
    /// writes it again. A table with no `schema` row, or a version below 1, is
    /// dropped and recreated as a cache table is: no build writes a state table
    /// without its row, since both are created in one transaction, so only a hand
    /// edit makes one.
    State {
        /// The steps from each older version to the next.
        migrations: &'static [Migration],
    },
}

/// What [`ensure_table`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ensured {
    /// The table is at this build's version and usable.
    Current,
    /// The table is a state table written by a newer build, left untouched.
    Newer,
}

/// One table of `wolf.sqlite`, as this build defines it.
pub(crate) struct TableSpec {
    /// The table's name, and its key in `schema`.
    pub(crate) name: &'static str,
    /// This build's version of the table. Bump it whenever `columns` changes:
    /// the version is the only shape check, and a table at this build's version
    /// is trusted to have this build's columns.
    pub(crate) version: i64,
    /// The column and constraint list of the table's `CREATE TABLE`.
    pub(crate) columns: &'static str,
    /// What a version mismatch does to the table.
    pub(crate) policy: TablePolicy,
}

/// A handle to one or more tables of `wolf.sqlite`: an optional state directory
/// (`None` disables it, see [`Table::disabled`]) plus a lazily (re)opened,
/// self-healing connection behind a mutex, which every table of the handle shares.
/// Clones share the connection.
///
/// Each table carries a sticky "newer" flag, set when [`ensure_table`] finds a
/// state table written by a newer build. A flagged table is never ensured again by
/// this handle, across reopens and heals, and its store checks
/// [`Table::is_newer`] before reading or writing it.
#[derive(Clone)]
pub(crate) struct Table {
    dir: Option<PathBuf>,
    specs: Arc<[&'static TableSpec]>,
    newer: Arc<[AtomicBool]>,
    conn: Arc<Mutex<Option<Connection>>>,
}

impl Table {
    /// Opens `spec`'s table in `dir/wolf.sqlite`, as [`Table::open_all`] does.
    pub(crate) fn open(dir: &Path, spec: &'static TableSpec) -> Table {
        Table::open_all(dir, &[spec])
    }

    /// Opens every table of `specs` in `dir/wolf.sqlite` on one connection,
    /// creating the directory, the file and the tables as needed, after deleting
    /// the files it replaces.
    ///
    /// Infallible and eager, so the file exists as soon as this returns. A failed
    /// open leaves the connection `None`, and [`Table::with_conn`] retries it.
    pub(crate) fn open_all(dir: &Path, specs: &[&'static TableSpec]) -> Table {
        remove_legacy_files(dir);
        let table = Table {
            dir: Some(dir.to_path_buf()),
            specs: specs.into(),
            newer: specs.iter().map(|_| AtomicBool::new(false)).collect(),
            conn: Arc::new(Mutex::new(None)),
        };
        *table.conn.lock() = open_or_heal(dir, &table.specs, &table.newer);
        table
    }

    /// A disabled handle: [`Table::with_conn`] always returns `None`.
    pub(crate) fn disabled(spec: &'static TableSpec) -> Table {
        Table {
            dir: None,
            specs: Arc::new([spec]),
            newer: Arc::new([AtomicBool::new(false)]),
            conn: Arc::new(Mutex::new(None)),
        }
    }

    /// Whether `spec`'s table was found written by a newer build, so this handle
    /// must neither read nor write it. Takes no lock, so a [`Table::with_conn`]
    /// closure may call it; inside one it reflects the connection `f` runs on.
    /// `false` for a spec this handle was not opened with.
    pub(crate) fn is_newer(&self, spec: &TableSpec) -> bool {
        self.specs
            .iter()
            .position(|s| std::ptr::eq(*s, spec))
            .is_some_and(|i| self.newer[i].load(Ordering::Acquire))
    }

    /// The handle's table names, joined for its log lines.
    fn label(&self) -> String {
        let names: Vec<&str> = self.specs.iter().map(|s| s.name).collect();
        names.join(",")
    }

    /// The single lock and self-heal choke point: runs `f` against the lazily
    /// (re)opened connection.
    ///
    /// Returns `None` for a disabled handle, when the connection cannot be
    /// opened, or when `f` fails. On a first `Err` that [`is_unusable_file`]
    /// accepts, the connection is dropped, the database files are deleted, the
    /// table is reopened and `f` is retried once. Any other `Err` — a busy,
    /// locked or I/O failure — leaves both the file and the connection alone, so
    /// the next call simply tries again.
    ///
    /// The guard is a named local, never a `match` scrutinee, so
    /// `clippy::significant_drop_in_scrutinee` stays quiet and its lifetime is
    /// explicit.
    pub(crate) fn with_conn<T>(&self, f: impl Fn(&Connection) -> rusqlite::Result<T>) -> Option<T> {
        let dir = self.dir.as_ref()?;

        let mut guard = self.conn.lock();
        if guard.is_none() {
            *guard = open_or_heal(dir, &self.specs, &self.newer);
        }
        let conn = guard.as_ref()?;

        match f(conn) {
            Ok(v) => Some(v),
            Err(first) if is_unusable_file(&first) => {
                let table = self.label();
                tracing::debug!(table, error = %first, "wolf.sqlite unusable; recreating");
                *guard = None;
                remove_db_files(&db_path(dir));
                *guard = open_or_heal(dir, &self.specs, &self.newer);
                let conn = guard.as_ref()?;
                match f(conn) {
                    Ok(v) => Some(v),
                    Err(second) => {
                        tracing::debug!(
                            table,
                            error = %second,
                            "query failed again after recreating wolf.sqlite; giving up"
                        );
                        None
                    }
                }
            }
            Err(first) => {
                tracing::debug!(
                    table = self.label(),
                    error = %first,
                    "query failed transiently; leaving wolf.sqlite untouched"
                );
                None
            }
        }
    }
}

/// The database path under the state directory `dir`.
fn db_path(dir: &Path) -> PathBuf {
    dir.join(DB_NAME)
}

/// Opens the database at `path`, installing a busy timeout and then enabling WAL
/// journaling.
///
/// The pragma order is load-bearing: `busy_timeout` must be installed before
/// `journal_mode=WAL`, because the rollback-journal -> WAL conversion is the one
/// statement here that can need an exclusive lock, and a connection with no
/// back-off installed fails it immediately with `SQLITE_BUSY` when several handles
/// open the same not-yet-converted database at once. (`rusqlite` happens to install
/// a default busy timeout of its own inside `Connection::open`, so this ordering is
/// belt-and-braces rather than the sole line of defence — but the invariant must not
/// depend on a dependency's implementation detail, and only this pragma makes
/// [`BUSY_TIMEOUT_MS`] the value actually in force during the conversion.)
fn connect(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "busy_timeout", BUSY_TIMEOUT_MS)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    Ok(conn)
}

/// Brings `spec`'s table to this build's version: creates `schema` if needed,
/// applies `spec.policy` when the stored version differs or is missing, records
/// this build's version, and creates the table if it does not exist.
///
/// It all runs in one `BEGIN IMMEDIATE` transaction. Two handles opening at once
/// would otherwise both read the mismatch, and the later one would drop a table
/// the earlier one had already recreated and written to. A state table's
/// migrations run inside it too, so a failed step leaves the table at its old
/// version and fails the open.
///
/// Returns [`Ensured::Newer`] for a state table at a newer version, after rolling
/// the transaction back: nothing is created, migrated or recorded.
fn ensure_table(conn: &mut Connection, spec: &TableSpec) -> rusqlite::Result<Ensured> {
    let (name, columns) = (spec.name, spec.columns);
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "CREATE TABLE IF NOT EXISTS schema (\
             name TEXT PRIMARY KEY, \
             version INTEGER NOT NULL\
         )",
        [],
    )?;
    let stored: Option<i64> = tx
        .query_row(
            "SELECT version FROM schema WHERE name = ?1",
            [name],
            |row| row.get(0),
        )
        .optional()?;
    if stored != Some(spec.version) {
        tracing::debug!(
            table = name,
            ?stored,
            version = spec.version,
            "table version mismatch"
        );
        match spec.policy {
            TablePolicy::Cache => {
                tx.execute(&format!("DROP TABLE IF EXISTS {name}"), [])?;
            }
            TablePolicy::State { migrations } => {
                debug_assert_eq!(
                    spec.version,
                    migrations.len() as i64 + 1,
                    "a state table's version is its migration count plus one"
                );
                match stored {
                    Some(v) if v > spec.version => return Ok(Ensured::Newer),
                    Some(v) if v >= 1 => {
                        let from = usize::try_from(v - 1).unwrap_or(usize::MAX);
                        for step in migrations.get(from..).unwrap_or_default() {
                            step(&tx)?;
                        }
                    }
                    _ => {
                        tx.execute(&format!("DROP TABLE IF EXISTS {name}"), [])?;
                    }
                }
            }
        }
        tx.execute(
            "INSERT INTO schema (name, version) VALUES (?1, ?2) \
             ON CONFLICT(name) DO UPDATE SET version = excluded.version",
            rusqlite::params![name, spec.version],
        )?;
    }
    tx.execute(
        &format!("CREATE TABLE IF NOT EXISTS {name} ({columns})"),
        [],
    )?;
    tx.commit()?;
    Ok(Ensured::Current)
}

/// Whether `err` means `wolf.sqlite` is unusable as a file and must be deleted
/// and recreated: `SQLITE_CORRUPT` or `SQLITE_NOTADB`, and nothing else.
///
/// This guards the only operation that destroys every table's rows. Deleting the
/// file unlinks it from under every other live connection, so a busy, locked or
/// schema error must never reach it; a wrong-shaped table is handled by
/// [`ensure_table`] alone.
fn is_unusable_file(err: &rusqlite::Error) -> bool {
    let code = match err {
        rusqlite::Error::SqliteFailure(ffi_err, _) => ffi_err.code,
        rusqlite::Error::SqlInputError { error, .. } => error.code,
        _ => return false,
    };
    matches!(
        code,
        rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
    )
}

/// Opens the tables of `specs` under `dir` on one connection, deleting and
/// recreating the database once if, and only if, [`is_unusable_file`] accepts the
/// failure.
///
/// A spec whose `newer` flag is set is skipped, and one [`ensure_table`] finds
/// newer gets its flag set. Any other failure logs and returns `None` without
/// touching the file, and [`Table::with_conn`] retries the open on the next use.
fn open_or_heal(
    dir: &Path,
    specs: &[&'static TableSpec],
    newer: &[AtomicBool],
) -> Option<Connection> {
    let _ = std::fs::create_dir_all(dir);
    let path = db_path(dir);
    let names: Vec<&str> = specs.iter().map(|s| s.name).collect();
    let table = names.join(",");
    let open = || -> rusqlite::Result<Connection> {
        let mut conn = connect(&path)?;
        for (spec, flag) in specs.iter().zip(newer) {
            if flag.load(Ordering::Acquire) {
                continue;
            }
            if ensure_table(&mut conn, spec)? == Ensured::Newer {
                tracing::debug!(
                    table = spec.name,
                    "table written by a newer build; leaving it untouched"
                );
                flag.store(true, Ordering::Release);
            }
        }
        Ok(conn)
    };
    match open() {
        Ok(conn) => Some(conn),
        Err(first) if is_unusable_file(&first) => {
            tracing::debug!(table, error = %first, "wolf.sqlite unusable; recreating");
            remove_db_files(&path);
            match open() {
                Ok(conn) => Some(conn),
                Err(second) => {
                    tracing::debug!(
                        table,
                        error = %second,
                        "wolf.sqlite could not be recreated; disabling this handle"
                    );
                    None
                }
            }
        }
        Err(first) => {
            tracing::debug!(
                table,
                error = %first,
                "wolf.sqlite temporarily unavailable; leaving it untouched and retrying later"
            );
            None
        }
    }
}

/// Removes the SQLite file at `path` plus its `-wal` and `-shm` sidecars,
/// ignoring every error: a real problem surfaces at the next open.
fn remove_db_files(path: &Path) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut name: OsString = path.as_os_str().to_os_string();
        name.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(name));
    }
}

/// Removes each of [`LEGACY_FILE_NAMES`] under `dir`, with its sidecars, importing
/// nothing from them.
fn remove_legacy_files(dir: &Path) {
    for name in LEGACY_FILE_NAMES {
        remove_db_files(&dir.join(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Version 1 of [`MIGRATED`] has `(k, v)`; version 2 adds `w`; version 3 fills it.
    static MIGRATED: TableSpec = TableSpec {
        name: "migrated",
        version: 3,
        columns: "k TEXT PRIMARY KEY, v TEXT NOT NULL, w TEXT NOT NULL DEFAULT ''",
        policy: TablePolicy::State {
            migrations: &[add_w, fill_w],
        },
    };

    fn add_w(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
        tx.execute(
            "ALTER TABLE migrated ADD COLUMN w TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map(drop)
    }

    fn fill_w(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
        tx.execute("UPDATE migrated SET w = v || '!'", []).map(drop)
    }

    /// Seeds `dir/wolf.sqlite` with `migrated` at `version`, holding the row `('a', 'x')`.
    fn seed(dir: &Path, version: i64, columns: &str) {
        let conn = Connection::open(db_path(dir)).expect("open the seed connection");
        conn.execute_batch(&format!(
            "CREATE TABLE schema (name TEXT PRIMARY KEY, version INTEGER NOT NULL);
             INSERT INTO schema (name, version) VALUES ('migrated', {version});
             CREATE TABLE migrated ({columns});
             INSERT INTO migrated (k, v) VALUES ('a', 'x');"
        ))
        .expect("seed the older table");
    }

    /// The `(k, v, w)` rows of `migrated` and its `schema` version, read through `table`.
    fn read(table: &Table) -> (Vec<(String, String, String)>, i64) {
        table
            .with_conn(|conn| {
                let mut stmt = conn.prepare("SELECT k, v, w FROM migrated ORDER BY k")?;
                let rows = stmt
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let version = conn.query_row(
                    "SELECT version FROM schema WHERE name = 'migrated'",
                    [],
                    |row| row.get(0),
                )?;
                Ok((rows, version))
            })
            .expect("the migrated table must open and read")
    }

    #[test]
    fn a_state_table_at_an_older_version_is_migrated_in_place() {
        let dir = tempfile::tempdir().expect("create the state tempdir");
        seed(dir.path(), 1, "k TEXT PRIMARY KEY, v TEXT NOT NULL");
        let table = Table::open(dir.path(), &MIGRATED);
        assert_eq!(
            read(&table),
            (vec![("a".to_owned(), "x".to_owned(), "x!".to_owned())], 3),
            "a version-1 state table must keep its row through both migrations and record \
             version 3; no row means it was dropped as a cache table is"
        );
    }

    #[test]
    fn migrations_start_at_the_stored_version() {
        let dir = tempfile::tempdir().expect("create the state tempdir");
        seed(
            dir.path(),
            2,
            "k TEXT PRIMARY KEY, v TEXT NOT NULL, w TEXT NOT NULL DEFAULT ''",
        );
        let table = Table::open(dir.path(), &MIGRATED);
        assert_eq!(
            read(&table),
            (vec![("a".to_owned(), "x".to_owned(), "x!".to_owned())], 3),
            "a version-2 table must run only the step to version 3; a failed open means the \
             version-1 step ran again on a table that already has its column"
        );
    }
}
