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
//! - Any open/schema/query failure is handled SILENTLY: the sqlite file (plus its
//!   `-wal`/`-shm` sidecars) is deleted and the schema recreated. Nothing panics,
//!   no error is propagated, and the only log is `tracing::debug!` (never `warn!`).
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
/// fail immediately with `SQLITE_BUSY`.
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
    /// is silent — the connection is left `None` and lazily retried (with a heal)
    /// on the next `record`/`recent`.
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
            let seq: i64 =
                conn.query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM command_mru", [], |row| {
                    row.get(0)
                })?;
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
    /// (re)opened) connection, healing once on a runtime error.
    ///
    /// Returns `None` for a disabled store or when the connection cannot be opened
    /// even after a heal. The mutex guard is locked into a NAMED LOCAL (never held
    /// in a `match`/`if let` scrutinee) so `clippy::significant_drop_in_scrutinee`
    /// does not fire and the guard's lifetime is explicit. On the first `Err` from
    /// `f`, the connection is dropped, the DB files are deleted, the store is
    /// reopened, and `f` is retried ONCE; a second `Err` is swallowed at
    /// `tracing::debug!` and returns `None`.
    fn with_conn<T>(&self, f: impl Fn(&Connection) -> rusqlite::Result<T>) -> Option<T> {
        let dir = self.dir.as_ref()?;

        let mut guard = self.conn.lock();
        if guard.is_none() {
            *guard = open_or_heal(dir);
        }
        let conn = guard.as_ref()?;

        match f(conn) {
            Ok(v) => Some(v),
            Err(first) => {
                tracing::debug!(error = %first, "command-mru query failed; healing DB");
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
                            "command-mru query failed again after heal; giving up"
                        );
                        None
                    }
                }
            }
        }
    }
}

/// The physical database path under `dir`.
fn db_path(dir: &Path) -> PathBuf {
    dir.join(COMMAND_MRU_DB_NAME)
}

/// Opens the database at `db_path`, enabling WAL journaling + a busy timeout,
/// ensuring the `command_mru` table exists, and PROBING it.
///
/// The probe (`SELECT key, seq FROM command_mru LIMIT 0`) is REQUIRED: a
/// pre-existing `command_mru` table missing the `seq` column survives
/// `CREATE TABLE IF NOT EXISTS`, so only a query referencing `seq` forces the
/// wrong-schema case to surface as an `Err` that the caller heals.
fn init(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", COMMAND_MRU_BUSY_TIMEOUT_MS)?;
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

/// Opens the database under `dir`, self-healing ONCE on failure.
///
/// Creates `dir` (best-effort), then tries [`init`]. On failure it deletes the DB
/// files ([`remove_db_files`]) and tries [`init`] ONCE more; a second failure logs
/// at `tracing::debug!` and returns `None`. Never panics, never propagates.
fn open_or_heal(dir: &Path) -> Option<Connection> {
    let _ = std::fs::create_dir_all(dir);
    let db = db_path(dir);
    match init(&db) {
        Ok(conn) => Some(conn),
        Err(first) => {
            tracing::debug!(error = %first, "command-mru DB unusable; recreating");
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
