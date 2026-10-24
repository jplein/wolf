//! The most-recently-used (MRU) file cache.
//!
//! Wolf records every file opened through the editor in a single GLOBAL SQLite
//! database at `<state_dir>/file-mru.sqlite` (macOS
//! `~/Library/Application Support/Wolf/file-mru.sqlite`), so the file picker can
//! offer recent files synchronously (before the async traversal completes) and
//! rank them first. Every row carries the project `directory` it was recorded
//! under, and [`MruStore::recent`] FILTERS to just the rows whose normalized
//! `directory` matches — so a single store serves every project while keeping
//! their recent lists ISOLATED (per-project MRU without a per-project DB).
//!
//! Like the sibling [`crate::recent_dirs`] / [`crate::command_mru`] stores, this
//! store is deliberately transient/disposable and robust:
//! - [`MruStore::record`] is fire-and-forget: it upserts a
//!   `(directory, file, opened_at)` row and swallows every error, so a transient
//!   cache problem never blocks opening a file.
//! - [`MruStore::recent`] returns the most-recently-opened files for a directory
//!   newest-first, pruning any whose backing file has since been deleted (both
//!   from the result AND from the cache, in the same connection).
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
//!     connection in the process and silently wipe the user's entire recent-files
//!     history.
//!
//! Unlike the sibling stores, ordering is by an `opened_at` millisecond timestamp
//! (most-recent first, `rowid DESC` tiebreak) rather than a monotonic `seq`, and
//! the missing-file prune is inline in `recent`.
//!
//! All SQLite work is synchronous and guarded by a [`parking_lot::Mutex`]; the
//! critical sections are short and never span an `.await`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use rusqlite::Connection;

/// [`FileEntry`] lives in the [`crate::files`] module (S2); it is re-exported here
/// so existing `mru::FileEntry` references (and the S1 tests) keep resolving.
pub use crate::files::FileEntry;

/// Maximum number of entries [`MruStore::recent`] returns (per directory).
pub const MRU_LIMIT: usize = 50;

/// The physical SQLite file name created under [`crate::state::state_dir`].
const MRU_DB_NAME: &str = "file-mru.sqlite";

/// SQLite `busy_timeout` (milliseconds) so concurrent opens back off rather than
/// fail immediately with `SQLITE_BUSY`. Installed as the FIRST statement after
/// `Connection::open` — see [`init`].
const MRU_BUSY_TIMEOUT_MS: u32 = 2000;

/// The global recent-files MRU cache: an optional directory (the store is a no-op
/// when `None`, see [`MruStore::disabled`]) plus a lazily-(re)opened, self-healing
/// SQLite connection behind a mutex.
#[derive(Clone)]
pub struct MruStore {
    /// The directory holding `file-mru.sqlite`. `None` disables the store: every
    /// method is a silent no-op.
    dir: Option<PathBuf>,
    /// The shared connection, `None` until first (re)opened. Wrapped so a runtime
    /// failure can drop and reopen it in place.
    conn: Arc<Mutex<Option<Connection>>>,
}

impl MruStore {
    /// Opens (creating if needed) the recent-files cache at `dir/file-mru.sqlite`.
    ///
    /// Infallible: it stores `dir` and eagerly opens/creates the database via
    /// `open_or_heal` (so the file exists right after `open`). Any failure to open
    /// is silent — the connection is left `None` and retried on the next
    /// `record`/`recent` (recreating the file only if it is corrupt or
    /// schema-incompatible; see `warrants_recreate`). Does NOT create any `.wolf` directory under a
    /// project directory; the MRU lives entirely under `dir`.
    #[must_use]
    pub fn open(dir: &Path) -> MruStore {
        let conn = open_or_heal(dir);
        MruStore {
            dir: Some(dir.to_path_buf()),
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// A disabled store: every method is a silent no-op and [`recent`](Self::recent)
    /// is always empty. Used when [`crate::state::state_dir`] cannot be resolved.
    #[must_use]
    pub fn disabled() -> MruStore {
        MruStore {
            dir: None,
            conn: Arc::new(Mutex::new(None)),
        }
    }

    /// Records `file` (opened under project `directory`) as just-opened
    /// (fire-and-forget).
    ///
    /// `directory` is normalized (canonicalized, falling back to as-given) — the
    /// SAME normalization [`recent`](Self::recent) applies — so the row filters back
    /// out exactly. The stored `file` is the canonical path if the file exists,
    /// otherwise the path absolutized against the normalized directory. `opened_at`
    /// is the current Unix time in milliseconds, giving [`recent`](Self::recent) a
    /// fine-grained, monotonic-ish ordering key. Errors are swallowed (self-healing
    /// on corruption), so opening a file is never blocked by a cache hiccup.
    pub fn record(&self, directory: &Path, file: &Path) {
        let dir_norm = normalize_dir(directory);
        let stored = absolute(&dir_norm, file);
        let stored = stored.to_string_lossy().into_owned();
        let dir_key = dir_norm.to_string_lossy().into_owned();

        let opened_at = match now_millis() {
            Ok(ms) => ms,
            Err(err) => {
                tracing::debug!(error = %err, "MRU record skipped: system clock before epoch");
                return;
            }
        };

        let _ = self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO mru (directory, file, opened_at) VALUES (?1, ?2, ?3) \
                 ON CONFLICT(directory, file) DO UPDATE SET opened_at = excluded.opened_at",
                rusqlite::params![dir_key.as_str(), stored.as_str(), opened_at],
            )?;
            Ok(())
        });
    }

    /// Returns the most-recently-opened files under `directory`, newest first,
    /// capped at [`MRU_LIMIT`].
    ///
    /// `directory` is normalized the SAME way [`record`](Self::record) normalizes
    /// it, then rows are filtered to that directory and ordered
    /// `opened_at DESC, rowid DESC`. Entries whose backing file no longer exists on
    /// disk are omitted AND pruned from the cache (best-effort `DELETE` via the same
    /// connection — a transient prune failure never heals/recreates the whole DB).
    /// `mru_rank` is the 0-based position in the returned list. Returns an empty
    /// `Vec` on any failure (disabled store, open/query error).
    #[must_use]
    pub fn recent(&self, directory: &Path) -> Vec<FileEntry> {
        let dir_norm = normalize_dir(directory);
        let dir_key = dir_norm.to_string_lossy().into_owned();
        let limit = i64::try_from(MRU_LIMIT).unwrap_or(i64::MAX);

        self.with_conn(|conn| {
            let candidates: Vec<String> = {
                let mut stmt = conn.prepare(
                    "SELECT file FROM mru WHERE directory = ?1 \
                     ORDER BY opened_at DESC, rowid DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(rusqlite::params![dir_key.as_str(), limit], |row| {
                    row.get::<_, String>(0)
                })?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                out
            };

            let mut entries = Vec::with_capacity(candidates.len());
            for stored in candidates {
                let path = PathBuf::from(&stored);
                if !path.exists() {
                    // Best-effort prune of the missing row: NOT propagated with `?`,
                    // so a transient DELETE failure never heals/recreates the DB.
                    let _ = conn.execute(
                        "DELETE FROM mru WHERE directory = ?1 AND file = ?2",
                        rusqlite::params![dir_key.as_str(), stored.as_str()],
                    );
                    continue;
                }
                let display = display_for(&dir_norm, &path);
                let rank = u32::try_from(entries.len()).unwrap_or(u32::MAX);
                entries.push(FileEntry {
                    path,
                    display,
                    mru_rank: Some(rank),
                });
            }

            Ok(entries)
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
                    "file-mru DB corrupt or schema-incompatible; recreating"
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
                            "file-mru query failed again after recreating; giving up"
                        );
                        None
                    }
                }
            }
            Err(first) => {
                tracing::debug!(
                    error = %first,
                    "file-mru query failed transiently; leaving the DB untouched"
                );
                None
            }
        }
    }
}

/// The physical database path under `dir`.
fn db_path(dir: &Path) -> PathBuf {
    dir.join(MRU_DB_NAME)
}

/// Opens the database at `db_path`, installing a busy timeout, enabling WAL
/// journaling, ensuring the `mru` table exists, and PROBING it.
///
/// The pragma ORDER is load-bearing: `busy_timeout` MUST be installed before
/// `journal_mode=WAL`, because the rollback-journal -> WAL conversion is the one
/// statement here that can need an exclusive lock, and a connection with no
/// back-off installed fails it immediately with `SQLITE_BUSY` when several handles
/// open the same not-yet-converted database at once. (`rusqlite` happens to install
/// a default busy timeout of its own inside `Connection::open`, so this ordering is
/// belt-and-braces rather than the sole line of defence — but the invariant must not
/// depend on a dependency's implementation detail, and only this pragma makes
/// [`MRU_BUSY_TIMEOUT_MS`] the value actually in force during the conversion.)
///
/// The probe (`SELECT directory, file, opened_at FROM mru LIMIT 0`) is REQUIRED: a
/// pre-existing `mru` table with the OLD single-`path` schema survives
/// `CREATE TABLE IF NOT EXISTS`, so only a query referencing the new columns forces
/// the wrong-schema case to surface as an `Err` that the caller heals. It surfaces
/// as `SQLITE_ERROR` + `no such column`, which [`warrants_recreate`] classifies as
/// recreate-worthy.
fn init(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "busy_timeout", MRU_BUSY_TIMEOUT_MS)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS mru (\
             directory TEXT NOT NULL, \
             file TEXT NOT NULL, \
             opened_at INTEGER NOT NULL, \
             PRIMARY KEY (directory, file)\
         )",
        [],
    )?;
    // Schema probe: fails on a garbage DB or the OLD single-`path` schema, forcing
    // the corruption-recovery path in the caller.
    conn.prepare("SELECT directory, file, opened_at FROM mru LIMIT 0")?;
    Ok(conn)
}

/// Whether `err` means the database on disk is UNUSABLE AS IT STANDS and must be
/// deleted and recreated, as opposed to a transient failure that will pass.
///
/// This is the guard on the store's only destructive operation
/// ([`remove_db_files`]). Deleting the file unlinks it from under every other live
/// connection in the process — on Linux those connections keep writing to the
/// now-unreachable inode — so recreating for a merely-busy database silently wipes
/// the user's whole recent-files history the moment a second Wolf window opens the
/// same store. Mirrors the identically-named private classifier in
/// [`crate::recent_dirs`] and [`crate::command_mru`]; the three are deliberately
/// duplicated rather than shared, so each store's classification stays readable next
/// to the schema probe that feeds it.
///
/// Recreate-worthy:
/// - `SQLITE_CORRUPT` / `SQLITE_NOTADB`: a damaged file or one that is not a
///   SQLite database at all (e.g. truncated by a crash, or garbage bytes).
/// - `SQLITE_SCHEMA`: the schema changed underneath a prepared statement.
/// - `SQLITE_ERROR` (`ErrorCode::Unknown`) whose message names a missing table or
///   column: the WRONG-SCHEMA case that [`init`]'s probe exists to raise, e.g. a
///   database left by an older Wolf with the single-`path` `mru` table.
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
                "file-mru DB corrupt or schema-incompatible; recreating"
            );
            remove_db_files(&db);
            match init(&db) {
                Ok(conn) => Some(conn),
                Err(second) => {
                    tracing::debug!(
                        error = %second,
                        "file-mru DB could not be recreated; disabling this handle"
                    );
                    None
                }
            }
        }
        Err(first) => {
            tracing::debug!(
                error = %first,
                "file-mru DB temporarily unavailable; leaving it untouched and retrying later"
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

/// Normalizes a project directory for storage/lookup: the canonical path if it
/// resolves (so macOS `/var` → `/private/var`), otherwise the path as-given.
///
/// Applied IDENTICALLY on write ([`MruStore::record`]) and read
/// ([`MruStore::recent`]) so `WHERE directory = ?` matches exactly.
fn normalize_dir(dir: &Path) -> PathBuf {
    std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
}

/// Computes the absolute path to store for `file`: the canonical path if the file
/// exists, otherwise `file` absolutized against the (normalized) `dir`.
fn absolute(dir: &Path, file: &Path) -> PathBuf {
    if let Ok(canon) = std::fs::canonicalize(file) {
        return canon;
    }
    if file.is_absolute() {
        file.to_path_buf()
    } else {
        dir.join(file)
    }
}

/// Builds the [`FileEntry::display`] for `path`: relative to `dir` when under it,
/// otherwise the full path string as a fallback.
fn display_for(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Current Unix time in milliseconds, as an `i64` suitable for SQLite.
fn now_millis() -> Result<i64, String> {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    i64::try_from(dur.as_millis()).map_err(|e| e.to_string())
}
