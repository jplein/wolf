//! The global "recent directories" MRU (most-recently-used) cache.
//!
//! Wolf records every directory the captive Neovim changes into — whether via
//! the "open folder" command or a typed `:cd`/`:lcd`/`:tcd` — in a single SQLite
//! database at `<state_dir>/folder-mru.sqlite` (macOS
//! `~/Library/Application Support/Wolf/folder-mru.sqlite`), so a future picker can
//! offer the user's recent project directories. Unlike the per-project file
//! [`crate::mru`] cache, this store is GLOBAL (one DB under
//! [`crate::state::state_dir`]), keyed on the directory path, and capped at
//! [`RECENT_DIRS_LIMIT`] entries newest-first.
//!
//! The store is deliberately transient/disposable and robust:
//! - [`RecentDirs::record`] is fire-and-forget: it upserts a `(path, seq)` row,
//!   move-or-add-to-top, prunes beyond the cap, and swallows every error, so a
//!   transient cache problem never blocks a directory change.
//! - [`RecentDirs::recent`] returns the recorded directories newest-first, capped,
//!   and returns an empty `Vec` on any failure.
//! - Every failure is handled SILENTLY — nothing panics, no error is propagated,
//!   and the only log is `tracing::debug!` (never `warn!`) — but the RESPONSE
//!   depends on WHY it failed (see [`warrants_recreate`]):
//!   - A CORRUPT or SCHEMA-INCOMPATIBLE database (`SQLITE_CORRUPT`,
//!     `SQLITE_NOTADB`, `SQLITE_SCHEMA`, or the `no such table`/`no such column`
//!     that this module's schema probe raises) is unusable as it stands, so the
//!     sqlite file plus its `-wal`/`-shm` sidecars is deleted and the schema
//!     recreated.
//!   - Any OTHER failure — most importantly a transient `SQLITE_BUSY`/
//!     `SQLITE_LOCKED` from another Wolf window opening the same store — degrades
//!     to an empty result or a dropped record and NEVER deletes the file. Deleting
//!     a merely-busy database would unlink it from under every other live
//!     connection in the process and silently wipe the user's whole Open-Recent
//!     folder history.
//!
//! Ordering is a monotonic `seq` column (`MAX(seq)+1` per insert), NOT a
//! timestamp, so move-to-top and the cap are deterministic even for
//! same-millisecond inserts (no sleeps needed in tests).
//!
//! All SQLite work is synchronous and guarded by a [`parking_lot::Mutex`]; the
//! critical sections are short and never span an `.await`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::Connection;

/// Maximum number of entries [`RecentDirs::recent`] returns (and the on-disk row
/// count [`RecentDirs::record`] prunes down to).
pub const RECENT_DIRS_LIMIT: usize = 20;

/// The physical SQLite file name created under [`crate::state::state_dir`].
const RECENT_DIRS_DB_NAME: &str = "folder-mru.sqlite";

/// SQLite `busy_timeout` (milliseconds) so concurrent opens back off rather than
/// fail immediately with `SQLITE_BUSY`. Installed as the FIRST statement after
/// `Connection::open` — see [`init`].
const RECENT_DIRS_BUSY_TIMEOUT_MS: u32 = 2000;

/// The global recent-directories MRU cache: an optional directory (the store is a
/// no-op when `None`, see [`RecentDirs::disabled`]) plus a lazily-(re)opened,
/// self-healing SQLite connection behind a mutex.
#[derive(Clone)]
pub struct RecentDirs {
    /// The directory holding `folder-mru.sqlite`. `None` disables the store: every
    /// method is a silent no-op.
    dir: Option<PathBuf>,
    /// The shared connection, `None` until first (re)opened. Wrapped so a runtime
    /// failure can drop and reopen it in place.
    conn: Arc<Mutex<Option<Connection>>>,
}

impl RecentDirs {
    /// Opens (creating if needed) the recent-directories cache at
    /// `dir/folder-mru.sqlite`.
    ///
    /// Infallible: it stores `dir` and eagerly opens/creates the database via
    /// `open_or_heal` (so the file exists right after `open`). Any failure to open
    /// is silent — the connection is left `None` and retried on the next
    /// `record`/`recent` (recreating the file only if it is corrupt or
    /// schema-incompatible; see `warrants_recreate`).
    #[must_use]
    pub fn open(dir: &Path) -> RecentDirs {
        let conn = open_or_heal(dir);
        RecentDirs {
            dir: Some(dir.to_path_buf()),
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// A disabled store: every method is a silent no-op and [`recent`](Self::recent)
    /// is always empty. Used when [`crate::state::state_dir`] cannot be resolved.
    #[must_use]
    pub fn disabled() -> RecentDirs {
        RecentDirs {
            dir: None,
            conn: Arc::new(Mutex::new(None)),
        }
    }

    /// Records `path` as the most-recently-used directory (fire-and-forget).
    ///
    /// `path` is stored as-given via `to_string_lossy` (it is already canonical
    /// when it arrives from `apply_cwd`, so it is NOT re-canonicalized here). One
    /// transaction moves-or-adds the row to the top (a monotonic `MAX(seq)+1`
    /// stamp with an upsert) and prunes any rows beyond [`RECENT_DIRS_LIMIT`].
    /// Errors are swallowed (self-healing on corruption); a directory change is
    /// never blocked by a cache hiccup.
    pub fn record(&self, path: &Path) {
        let stored = path.to_string_lossy().into_owned();
        let limit = i64::try_from(RECENT_DIRS_LIMIT).unwrap_or(i64::MAX);
        let _ = self.with_conn(|conn| {
            let seq: i64 = conn.query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM recent_dirs",
                [],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO recent_dirs (path, seq) VALUES (?1, ?2) \
                 ON CONFLICT(path) DO UPDATE SET seq = excluded.seq",
                rusqlite::params![stored.as_str(), seq],
            )?;
            conn.execute(
                "DELETE FROM recent_dirs WHERE path NOT IN \
                 (SELECT path FROM recent_dirs ORDER BY seq DESC LIMIT ?1)",
                rusqlite::params![limit],
            )?;
            Ok(())
        });
    }

    /// Returns the recorded directories, newest first, capped at
    /// [`RECENT_DIRS_LIMIT`].
    ///
    /// Returns an empty `Vec` on any failure (disabled store, open/query error).
    /// Unlike [`crate::mru::MruStore::recent`] it does NOT prune paths whose
    /// directory no longer exists.
    #[must_use]
    pub fn recent(&self) -> Vec<PathBuf> {
        let limit = i64::try_from(RECENT_DIRS_LIMIT).unwrap_or(i64::MAX);
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT path FROM recent_dirs ORDER BY seq DESC LIMIT ?1")?;
            let rows = stmt.query_map([limit], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(PathBuf::from(row?));
            }
            Ok(out)
        })
        .unwrap_or_default()
    }

    /// The single lock + self-heal choke point: runs `f` against the (lazily
    /// (re)opened) connection, recreating the database once IF the failure means
    /// the file is unusable.
    ///
    /// Returns `None` for a disabled store or when the connection cannot be opened.
    /// The mutex guard is locked into a NAMED LOCAL (never held in a `match`/
    /// `if let` scrutinee) so `clippy::significant_drop_in_scrutinee` does not fire
    /// and the guard's lifetime is explicit. On a first `Err` from `f` that
    /// [`warrants_recreate`], the connection is dropped, the DB files are deleted,
    /// the store is reopened, and `f` is retried ONCE; a second `Err` is swallowed
    /// at `tracing::debug!` and returns `None`. Any OTHER `Err` — a transient busy
    /// / locked / I/O failure — is swallowed at `tracing::debug!` and returns
    /// `None` while LEAVING BOTH the file and the open connection alone, so the
    /// next call simply tries again.
    fn with_conn<T>(&self, f: impl Fn(&Connection) -> rusqlite::Result<T>) -> Option<T> {
        let dir = self.dir.as_ref()?;

        let mut guard = self.conn.lock();
        if guard.is_none() {
            *guard = open_or_heal(dir);
        }
        let conn = guard.as_ref()?;

        match f(conn) {
            Ok(v) => Some(v),
            Err(first) if warrants_recreate(&first) => {
                tracing::debug!(
                    error = %first,
                    "recent-dirs DB corrupt or schema-incompatible; recreating"
                );
                // Drop the bad connection, delete the files, and reopen fresh.
                *guard = None;
                remove_db_files(&db_path(dir));
                *guard = open_or_heal(dir);
                let conn = guard.as_ref()?;
                match f(conn) {
                    Ok(v) => Some(v),
                    Err(second) => {
                        tracing::debug!(
                            error = %second,
                            "recent-dirs query failed again after recreating; giving up"
                        );
                        None
                    }
                }
            }
            Err(first) => {
                tracing::debug!(
                    error = %first,
                    "recent-dirs query failed transiently; leaving the DB untouched"
                );
                None
            }
        }
    }
}

/// The physical database path under `dir`.
fn db_path(dir: &Path) -> PathBuf {
    dir.join(RECENT_DIRS_DB_NAME)
}

/// Opens the database at `db_path`, installing a busy timeout, enabling WAL
/// journaling, ensuring the `recent_dirs` table exists, and PROBING it.
///
/// The pragma ORDER is load-bearing: `busy_timeout` MUST be installed before
/// `journal_mode=WAL`, because the rollback-journal -> WAL conversion is the one
/// statement here that can need an exclusive lock, and a connection with no
/// back-off installed fails it immediately with `SQLITE_BUSY` when several handles
/// open the same not-yet-converted database at once. (`rusqlite` happens to install
/// a default busy timeout of its own inside `Connection::open`, so this ordering is
/// belt-and-braces rather than the sole line of defence — but the invariant must not
/// depend on a dependency's implementation detail, and only this pragma makes
/// [`RECENT_DIRS_BUSY_TIMEOUT_MS`] the value actually in force during the
/// conversion.)
///
/// The probe (`SELECT path, seq FROM recent_dirs LIMIT 0`) is REQUIRED: a
/// pre-existing `recent_dirs` table missing the `seq` column survives
/// `CREATE TABLE IF NOT EXISTS`, so only a query referencing `seq` forces the
/// wrong-schema case to surface as an `Err` that the caller heals. It surfaces as
/// `SQLITE_ERROR` + `no such column`, which [`warrants_recreate`] classifies as
/// recreate-worthy.
fn init(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "busy_timeout", RECENT_DIRS_BUSY_TIMEOUT_MS)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS recent_dirs (\
             path TEXT PRIMARY KEY, \
             seq INTEGER NOT NULL\
         )",
        [],
    )?;
    // Schema probe: fails on a garbage DB or a wrong-schema table (missing `seq`),
    // forcing the corruption-recovery path in the caller.
    conn.prepare("SELECT path, seq FROM recent_dirs LIMIT 0")?;
    Ok(conn)
}

/// Whether `err` means the database on disk is UNUSABLE AS IT STANDS and must be
/// deleted and recreated, as opposed to a transient failure that will pass.
///
/// This is the guard on the store's only destructive operation
/// ([`remove_db_files`]). Deleting the file unlinks it from under every other live
/// connection in the process — on Linux those connections keep writing to the
/// now-unreachable inode — so recreating for a merely-busy database silently wipes
/// the user's whole Open-Recent folder history the moment a second Wolf window
/// opens the same store. Mirrors the identically-named private classifier in
/// [`crate::mru`] and [`crate::command_mru`]; the three are deliberately duplicated
/// rather than shared, so each store's classification stays readable next to the
/// schema probe that feeds it.
///
/// Recreate-worthy:
/// - `SQLITE_CORRUPT` / `SQLITE_NOTADB`: a damaged file or one that is not a
///   SQLite database at all (e.g. truncated by a crash, or garbage bytes).
/// - `SQLITE_SCHEMA`: the schema changed underneath a prepared statement.
/// - `SQLITE_ERROR` (`ErrorCode::Unknown`) whose message names a missing table or
///   column: the WRONG-SCHEMA case that [`init`]'s probe exists to raise, e.g. a
///   `recent_dirs` table left by an older Wolf with no `seq` column.
///
/// Everything else — `SQLITE_BUSY`, `SQLITE_LOCKED`, I/O failures, read-only or
/// full disks, and every variant that carries no SQLite error code at all — is
/// `false`: back off and leave the file alone.
fn warrants_recreate(err: &rusqlite::Error) -> bool {
    // Classify both variants: rusqlite reports the schema probe's `prepare` failure as
    // `SqlInputError`, so matching only `SqliteFailure` disables wrong-schema recreation.
    let (ffi_err, msg) = match err {
        rusqlite::Error::SqliteFailure(ffi_err, msg) => (ffi_err, msg.as_deref()),
        rusqlite::Error::SqlInputError { error, msg, .. } => (error, Some(msg.as_str())),
        _ => return false,
    };
    match ffi_err.code {
        rusqlite::ErrorCode::DatabaseCorrupt
        | rusqlite::ErrorCode::NotADatabase
        | rusqlite::ErrorCode::SchemaChanged => true,
        rusqlite::ErrorCode::Unknown => {
            msg.is_some_and(|m| m.contains("no such table") || m.contains("no such column"))
        }
        _ => false,
    }
}

/// Opens the database under `dir`, recreating it ONCE if — and only if — the
/// failure means the file itself is unusable ([`warrants_recreate`]).
///
/// Creates `dir` (best-effort), then tries [`init`]. On a recreate-worthy failure
/// it deletes the DB files ([`remove_db_files`]) and tries [`init`] ONCE more; a
/// second failure logs at `tracing::debug!` and returns `None`. On any OTHER
/// failure it logs and returns `None` WITHOUT touching the file: the handle is
/// simply disabled for now, and `with_conn` retries the open on the next
/// `record`/`recent`. Never panics, never propagates.
fn open_or_heal(dir: &Path) -> Option<Connection> {
    let _ = std::fs::create_dir_all(dir);
    let db = db_path(dir);
    match init(&db) {
        Ok(conn) => Some(conn),
        Err(first) if warrants_recreate(&first) => {
            tracing::debug!(
                error = %first,
                "recent-dirs DB corrupt or schema-incompatible; recreating"
            );
            remove_db_files(&db);
            match init(&db) {
                Ok(conn) => Some(conn),
                Err(second) => {
                    tracing::debug!(
                        error = %second,
                        "recent-dirs DB could not be recreated; disabling this handle"
                    );
                    None
                }
            }
        }
        Err(first) => {
            tracing::debug!(
                error = %first,
                "recent-dirs DB temporarily unavailable; leaving it untouched and retrying later"
            );
            None
        }
    }
}

/// Removes the SQLite file at `db_path` plus its `-wal` and `-shm` sidecars,
/// ignoring "not found". Best-effort: any other error is ignored (the subsequent
/// `init` will surface a real problem).
fn remove_db_files(db_path: &Path) {
    let _ = std::fs::remove_file(db_path);
    for suffix in ["-wal", "-shm"] {
        let mut name: OsString = db_path.as_os_str().to_os_string();
        name.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(name));
    }
}
