//! The global "command palette" MRU (most-recently-used) cache.
//!
//! Wolf records every command palette invocation the user activates — a static
//! registry command, a "switch to window", or an "open recent folder" — in a
//! single SQLite database at `<state_dir>/command-mru.sqlite` (macOS
//! `~/Library/Application Support/Wolf/command-mru.sqlite`), so the palette can
//! float the user's recently-used commands to the top. Like the global
//! [`crate::recent_dirs`] cache this store is GLOBAL (one DB under
//! [`crate::state::state_dir`]) and capped at [`COMMAND_MRU_LIMIT`] entries
//! newest-first, but it is keyed on an OPAQUE command KEY string (a static
//! command's registry `id`, or a dynamic `switch_window:<cwd>` /
//! `open_recent:<path>` key) rather than a directory path.
//!
//! Unlike [`crate::recent_dirs`], NOTHING auto-records into this store: only an
//! explicit palette activation ([`CommandMru::record`]) writes to it, so a fresh
//! store under a new state dir reads back EMPTY.
//!
//! The store is deliberately transient/disposable and robust:
//! - [`CommandMru::record`] is fire-and-forget: it upserts a `(key, seq)` row,
//!   move-or-add-to-top, prunes beyond the cap, and swallows every error, so a
//!   transient cache problem never blocks a palette activation.
//! - [`CommandMru::recent`] returns the recorded keys newest-first, capped, and
//!   returns an empty `Vec` on any failure.
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
//!     connection in the process and silently reset the user's whole palette
//!     ordering.
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

/// Maximum number of entries [`CommandMru::recent`] returns (and the on-disk row
/// count [`CommandMru::record`] prunes down to).
pub const COMMAND_MRU_LIMIT: usize = 100;

/// The physical SQLite file name created under [`crate::state::state_dir`].
const COMMAND_MRU_DB_NAME: &str = "command-mru.sqlite";

/// SQLite `busy_timeout` (milliseconds) so concurrent opens back off rather than
/// fail immediately with `SQLITE_BUSY`. Installed as the FIRST statement after
/// `Connection::open` — see [`init`].
const COMMAND_MRU_BUSY_TIMEOUT_MS: u32 = 2000;

/// The global command-palette MRU cache: an optional directory (the store is a
/// no-op when `None`, see [`CommandMru::disabled`]) plus a lazily-(re)opened,
/// self-healing SQLite connection behind a mutex.
#[derive(Clone)]
pub struct CommandMru {
    /// The directory holding `command-mru.sqlite`. `None` disables the store:
    /// every method is a silent no-op.
    dir: Option<PathBuf>,
    /// The shared connection, `None` until first (re)opened. Wrapped so a runtime
    /// failure can drop and reopen it in place.
    conn: Arc<Mutex<Option<Connection>>>,
}

impl CommandMru {
    /// Opens (creating if needed) the command-palette MRU cache at
    /// `dir/command-mru.sqlite`.
    ///
    /// Infallible: it stores `dir` and eagerly opens/creates the database via
    /// `open_or_heal` (so the file exists right after `open`). Any failure to open
    /// is silent — the connection is left `None` and retried on the next
    /// `record`/`recent` (recreating the file only if it is corrupt or
    /// schema-incompatible; see `warrants_recreate`).
    #[must_use]
    pub fn open(dir: &Path) -> CommandMru {
        let conn = open_or_heal(dir);
        CommandMru {
            dir: Some(dir.to_path_buf()),
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// A disabled store: every method is a silent no-op and [`recent`](Self::recent)
    /// is always empty. Used when [`crate::state::state_dir`] cannot be resolved.
    #[must_use]
    pub fn disabled() -> CommandMru {
        CommandMru {
            dir: None,
            conn: Arc::new(Mutex::new(None)),
        }
    }

    /// Records `key` as the most-recently-used command (fire-and-forget).
    ///
    /// `key` is stored as-given (an opaque palette key: a registry `id`, or a
    /// dynamic `switch_window:<cwd>` / `open_recent:<path>` key). One transaction
    /// moves-or-adds the row to the top (a monotonic `MAX(seq)+1` stamp with an
    /// upsert) and prunes any rows beyond [`COMMAND_MRU_LIMIT`]. Errors are
    /// swallowed (self-healing on corruption); a palette activation is never
    /// blocked by a cache hiccup.
    pub fn record(&self, key: &str) {
        let limit = i64::try_from(COMMAND_MRU_LIMIT).unwrap_or(i64::MAX);
        let _ = self.with_conn(|conn| {
            let seq: i64 = conn.query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM command_mru",
                [],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO command_mru (key, seq) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET seq = excluded.seq",
                rusqlite::params![key, seq],
            )?;
            conn.execute(
                "DELETE FROM command_mru WHERE key NOT IN \
                 (SELECT key FROM command_mru ORDER BY seq DESC LIMIT ?1)",
                rusqlite::params![limit],
            )?;
            Ok(())
        });
    }

    /// Returns the recorded command keys, newest first, capped at
    /// [`COMMAND_MRU_LIMIT`].
    ///
    /// Returns an empty `Vec` on any failure (disabled store, open/query error).
    #[must_use]
    pub fn recent(&self) -> Vec<String> {
        let limit = i64::try_from(COMMAND_MRU_LIMIT).unwrap_or(i64::MAX);
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT key FROM command_mru ORDER BY seq DESC LIMIT ?1")?;
            let rows = stmt.query_map([limit], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
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
                    "command-mru DB corrupt or schema-incompatible; recreating"
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
                            "command-mru query failed again after recreating; giving up"
                        );
                        None
                    }
                }
            }
            Err(first) => {
                tracing::debug!(
                    error = %first,
                    "command-mru query failed transiently; leaving the DB untouched"
                );
                None
            }
        }
    }
}

/// The physical database path under `dir`.
fn db_path(dir: &Path) -> PathBuf {
    dir.join(COMMAND_MRU_DB_NAME)
}

/// Opens the database at `db_path`, installing a busy timeout, enabling WAL
/// journaling, ensuring the `command_mru` table exists, and PROBING it.
///
/// The pragma ORDER is load-bearing: `busy_timeout` MUST be installed before
/// `journal_mode=WAL`, because the rollback-journal -> WAL conversion is the one
/// statement here that can need an exclusive lock, and a connection with no
/// back-off installed fails it immediately with `SQLITE_BUSY` when several handles
/// open the same not-yet-converted database at once. (`rusqlite` happens to install
/// a default busy timeout of its own inside `Connection::open`, so this ordering is
/// belt-and-braces rather than the sole line of defence — but the invariant must not
/// depend on a dependency's implementation detail, and only this pragma makes
/// [`COMMAND_MRU_BUSY_TIMEOUT_MS`] the value actually in force during the
/// conversion.)
///
/// The probe (`SELECT key, seq FROM command_mru LIMIT 0`) is REQUIRED: a
/// pre-existing `command_mru` table missing the `seq` column survives
/// `CREATE TABLE IF NOT EXISTS`, so only a query referencing `seq` forces the
/// wrong-schema case to surface as an `Err` that the caller heals. It surfaces as
/// `SQLITE_ERROR` + `no such column`, which [`warrants_recreate`] classifies as
/// recreate-worthy.
fn init(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "busy_timeout", COMMAND_MRU_BUSY_TIMEOUT_MS)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS command_mru (\
             key TEXT PRIMARY KEY, \
             seq INTEGER NOT NULL\
         )",
        [],
    )?;
    // Schema probe: fails on a garbage DB or a wrong-schema table (missing `seq`),
    // forcing the corruption-recovery path in the caller.
    conn.prepare("SELECT key, seq FROM command_mru LIMIT 0")?;
    Ok(conn)
}

/// Whether `err` means the database on disk is UNUSABLE AS IT STANDS and must be
/// deleted and recreated, as opposed to a transient failure that will pass.
///
/// This is the guard on the store's only destructive operation
/// ([`remove_db_files`]). Deleting the file unlinks it from under every other live
/// connection in the process — on Linux those connections keep writing to the
/// now-unreachable inode — so recreating for a merely-busy database silently resets
/// the user's whole palette ordering the moment a second Wolf window opens the same
/// store. Mirrors the identically-named private classifier in [`crate::mru`] and
/// [`crate::recent_dirs`]; the three are deliberately duplicated rather than shared,
/// so each store's classification stays readable next to the schema probe that feeds
/// it.
///
/// Recreate-worthy:
/// - `SQLITE_CORRUPT` / `SQLITE_NOTADB`: a damaged file or one that is not a
///   SQLite database at all (e.g. truncated by a crash, or garbage bytes).
/// - `SQLITE_SCHEMA`: the schema changed underneath a prepared statement.
/// - `SQLITE_ERROR` (`ErrorCode::Unknown`) whose message names a missing table or
///   column: the WRONG-SCHEMA case that [`init`]'s probe exists to raise, e.g. a
///   `command_mru` table left by an older Wolf with no `seq` column.
///
/// Everything else — `SQLITE_BUSY`, `SQLITE_LOCKED`, I/O failures, read-only or
/// full disks, and every variant that carries no SQLite error code at all — is
/// `false`: back off and leave the file alone.
fn warrants_recreate(err: &rusqlite::Error) -> bool {
    // BOTH error variants must be classified. `rusqlite` splits a failure that
    // SQLite can attribute to a byte offset in the SQL text out into
    // `SqlInputError` instead of `SqliteFailure`, and that is exactly the shape
    // the schema probe's `prepare` raises — matching only `SqliteFailure` here
    // would silently disable the wrong-schema recreation path.
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
                "command-mru DB corrupt or schema-incompatible; recreating"
            );
            remove_db_files(&db);
            match init(&db) {
                Ok(conn) => Some(conn),
                Err(second) => {
                    tracing::debug!(
                        error = %second,
                        "command-mru DB could not be recreated; disabling this handle"
                    );
                    None
                }
            }
        }
        Err(first) => {
            tracing::debug!(
                error = %first,
                "command-mru DB temporarily unavailable; leaving it untouched and retrying later"
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
