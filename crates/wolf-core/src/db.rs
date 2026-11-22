//! The shared state database `wolf.sqlite`: its open, the per-table versioning
//! kept in its `schema` table, its recovery, and the removal of the legacy
//! per-store files.
//!
//! The policy this module implements — which tables exist, what a version
//! mismatch does to each class, and which files are deleted when — is
//! `specs/application-state.md` §1. Each store declares a [`TableSpec`] and
//! reaches the database only through a [`Table`] built from it.
//!
//! Every failure is silent: nothing panics, nothing propagates, and the only log
//! is `tracing::debug!`, carrying the table's name. All SQLite work is
//! synchronous and guarded by a [`parking_lot::Mutex`]; the critical sections are
//! short and never span an `.await`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

/// The database's file name under [`crate::state::state_dir`].
const DB_NAME: &str = "wolf.sqlite";

/// The per-store databases this one replaces, deleted on every [`Table::open`].
const LEGACY_DB_NAMES: [&str; 3] = ["file-mru.sqlite", "folder-mru.sqlite", "command-mru.sqlite"];

/// SQLite `busy_timeout` (milliseconds) so concurrent opens back off rather than
/// fail immediately with `SQLITE_BUSY`. Installed before anything else — see
/// [`connect`].
const BUSY_TIMEOUT_MS: u32 = 2000;

/// What [`ensure_table`] does to a table whose `schema` row names another
/// version than its [`TableSpec::version`], or that has no row at all.
pub(crate) enum TablePolicy {
    /// The table is disposable: drop it and create it again, empty.
    Cache,
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

/// A handle to one table of `wolf.sqlite`: an optional state directory (`None`
/// disables it, see [`Table::disabled`]) plus a lazily (re)opened, self-healing
/// connection behind a mutex. Clones share the connection.
#[derive(Clone)]
pub(crate) struct Table {
    dir: Option<PathBuf>,
    spec: &'static TableSpec,
    conn: Arc<Mutex<Option<Connection>>>,
}

impl Table {
    /// Opens `spec`'s table in `dir/wolf.sqlite`, creating the directory, the
    /// file and the table as needed, after deleting the legacy per-store files.
    ///
    /// Infallible and eager, so the file exists as soon as this returns. A failed
    /// open leaves the connection `None`, and [`Table::with_conn`] retries it.
    pub(crate) fn open(dir: &Path, spec: &'static TableSpec) -> Table {
        remove_legacy_files(dir);
        let conn = open_or_heal(dir, spec);
        Table {
            dir: Some(dir.to_path_buf()),
            spec,
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// A disabled handle: [`Table::with_conn`] always returns `None`.
    pub(crate) fn disabled(spec: &'static TableSpec) -> Table {
        Table {
            dir: None,
            spec,
            conn: Arc::new(Mutex::new(None)),
        }
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
        let table = self.spec.name;

        let mut guard = self.conn.lock();
        if guard.is_none() {
            *guard = open_or_heal(dir, self.spec);
        }
        let conn = guard.as_ref()?;

        match f(conn) {
            Ok(v) => Some(v),
            Err(first) if is_unusable_file(&first) => {
                tracing::debug!(table, error = %first, "wolf.sqlite unusable; recreating");
                *guard = None;
                remove_db_files(&db_path(dir));
                *guard = open_or_heal(dir, self.spec);
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
                    table,
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
/// the earlier one had already recreated and written to.
fn ensure_table(conn: &mut Connection, spec: &TableSpec) -> rusqlite::Result<()> {
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
    tx.commit()
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

/// Opens `spec`'s table under `dir`, deleting and recreating the database once
/// if, and only if, [`is_unusable_file`] accepts the failure.
///
/// Any other failure logs and returns `None` without touching the file, and
/// [`Table::with_conn`] retries the open on the next use.
fn open_or_heal(dir: &Path, spec: &TableSpec) -> Option<Connection> {
    let _ = std::fs::create_dir_all(dir);
    let path = db_path(dir);
    let table = spec.name;
    let open = || -> rusqlite::Result<Connection> {
        let mut conn = connect(&path)?;
        ensure_table(&mut conn, spec)?;
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

/// Removes each of [`LEGACY_DB_NAMES`] under `dir`, with its sidecars, importing
/// nothing from them.
fn remove_legacy_files(dir: &Path) {
    for name in LEGACY_DB_NAMES {
        remove_db_files(&dir.join(name));
    }
}
