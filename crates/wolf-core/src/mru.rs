//! The most-recently-used (MRU) file cache.
//!
//! Wolf records every file opened through the editor in a small SQLite database
//! at `<cwd>/.wolf/mru.sqlite`, so the file picker can offer recent files
//! synchronously (before the async traversal completes) and rank them first.
//!
//! The store is deliberately simple and robust:
//! - [`MruStore::record`] is fire-and-forget: it upserts a `(path, opened_at)`
//!   row and logs (never propagates) errors, so a transient cache problem never
//!   blocks opening a file.
//! - [`MruStore::recent`] returns the most-recently-opened files newest-first,
//!   pruning any whose backing file has since been deleted (both from the result
//!   AND from the cache, in the same connection).
//!
//! All SQLite work is synchronous and guarded by a [`parking_lot::Mutex`]; the
//! critical sections are short and never span an `.await`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use rusqlite::Connection;

/// [`FileEntry`] lives in the [`crate::files`] module (S2); it is re-exported here
/// so existing `mru::FileEntry` references (and the S1 tests) keep resolving.
pub use crate::files::FileEntry;

/// Maximum number of entries [`MruStore::recent`] returns.
pub const MRU_LIMIT: usize = 50;

/// SQLite `busy_timeout` (milliseconds) so concurrent opens back off rather than
/// fail immediately with `SQLITE_BUSY`.
const MRU_BUSY_TIMEOUT_MS: u32 = 2000;

/// Errors from opening or driving the MRU store.
#[derive(Debug, thiserror::Error)]
pub enum MruError {
    /// Failed to open or initialize the SQLite database.
    #[error("failed to open MRU cache: {0}")]
    Open(#[source] rusqlite::Error),
    /// Failed to create the `.wolf` cache directory.
    #[error("failed to prepare MRU cache directory: {0}")]
    Io(#[source] std::io::Error),
    /// A query against the MRU cache failed.
    #[error("MRU cache query failed: {0}")]
    Query(#[source] rusqlite::Error),
}

/// The MRU cache: a shared SQLite connection plus the (canonical) cwd used to
/// relativize stored paths into [`FileEntry::display`].
#[derive(Clone)]
pub struct MruStore {
    conn: Arc<Mutex<Connection>>,
    cwd: PathBuf,
}

impl MruStore {
    /// Opens (creating if needed) the MRU cache at `<cwd>/.wolf/mru.sqlite`.
    ///
    /// Creates the `.wolf` directory, enables WAL journaling and a busy timeout,
    /// and ensures the `mru` table exists. The cwd is canonicalized so that the
    /// canonical paths stored by [`record`](Self::record) relativize stably.
    pub fn open(cwd: &Path) -> Result<MruStore, MruError> {
        let wolf_dir = cwd.join(".wolf");
        std::fs::create_dir_all(&wolf_dir).map_err(MruError::Io)?;

        // The cwd exists now; canonicalize so stored canonical paths strip cleanly.
        let cwd = std::fs::canonicalize(cwd).map_err(MruError::Io)?;

        let db_path = wolf_dir.join("mru.sqlite");
        let conn = Connection::open(&db_path).map_err(MruError::Open)?;

        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(MruError::Open)?;
        conn.pragma_update(None, "busy_timeout", MRU_BUSY_TIMEOUT_MS)
            .map_err(MruError::Open)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS mru (\
                 path TEXT PRIMARY KEY, \
                 opened_at INTEGER NOT NULL\
             )",
            [],
        )
        .map_err(MruError::Open)?;

        Ok(MruStore {
            conn: Arc::new(Mutex::new(conn)),
            cwd,
        })
    }

    /// Records `path` as just-opened (fire-and-forget).
    ///
    /// The stored path is the canonical path if the file exists, otherwise the
    /// path absolutized against the cwd. `opened_at` is the current Unix time in
    /// milliseconds, giving recent() a fine-grained, monotonic-ish ordering key.
    /// Errors are logged via `tracing`, never propagated, so opening a file is
    /// never blocked by a cache hiccup.
    pub fn record(&self, path: &Path) {
        let stored = self.absolute(path);
        let stored = stored.to_string_lossy();

        let opened_at = match Self::now_millis() {
            Ok(ms) => ms,
            Err(err) => {
                tracing::warn!(error = %err, "MRU record skipped: system clock before epoch");
                return;
            }
        };

        let conn = self.conn.lock();
        if let Err(err) = conn.execute(
            "INSERT INTO mru (path, opened_at) VALUES (?1, ?2) \
             ON CONFLICT(path) DO UPDATE SET opened_at = excluded.opened_at",
            rusqlite::params![stored.as_ref(), opened_at],
        ) {
            tracing::warn!(error = %err, path = %stored, "failed to record MRU entry");
        }
    }

    /// Returns the most-recently-opened files, newest first, capped at
    /// [`MRU_LIMIT`].
    ///
    /// Entries whose backing file no longer exists on disk are omitted AND
    /// pruned from the cache (deleted via the same connection). `mru_rank` is the
    /// 0-based position in the returned list.
    #[must_use]
    pub fn recent(&self) -> Vec<FileEntry> {
        match self.recent_inner() {
            Ok(entries) => entries,
            Err(err) => {
                tracing::warn!(error = %err, "failed to read MRU cache");
                Vec::new()
            }
        }
    }

    /// Current Unix time in milliseconds, as an `i64` suitable for SQLite.
    fn now_millis() -> Result<i64, String> {
        let dur = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        i64::try_from(dur.as_millis()).map_err(|e| e.to_string())
    }

    /// Computes the absolute path to store for `path`: the canonical path if the
    /// file exists, otherwise `path` absolutized against the (canonical) cwd.
    fn absolute(&self, path: &Path) -> PathBuf {
        if let Ok(canon) = std::fs::canonicalize(path) {
            return canon;
        }
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        }
    }

    /// Builds the [`FileEntry::display`] for `path`: relative to cwd when under
    /// it, otherwise the full path string as a fallback.
    fn display_for(&self, path: &Path) -> String {
        path.strip_prefix(&self.cwd)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    /// The fallible body of [`recent`](Self::recent): reads candidate rows,
    /// prunes missing files inline, and builds ranked [`FileEntry`]s.
    fn recent_inner(&self) -> Result<Vec<FileEntry>, MruError> {
        let conn = self.conn.lock();

        let candidates: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT path FROM mru ORDER BY opened_at DESC, rowid DESC LIMIT ?1")
                .map_err(MruError::Query)?;
            let limit = i64::try_from(MRU_LIMIT).unwrap_or(i64::MAX);
            let rows = stmt
                .query_map([limit], |row| row.get::<_, String>(0))
                .map_err(MruError::Query)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(MruError::Query)?);
            }
            out
        };

        let mut entries = Vec::with_capacity(candidates.len());
        for stored in candidates {
            let path = PathBuf::from(&stored);
            if !path.exists() {
                // Prune the missing row from the cache, then skip it.
                if let Err(err) = conn.execute("DELETE FROM mru WHERE path = ?1", [&stored]) {
                    tracing::warn!(error = %err, path = %stored, "failed to prune missing MRU entry");
                }
                continue;
            }
            let display = self.display_for(&path);
            let rank = u32::try_from(entries.len()).unwrap_or(u32::MAX);
            entries.push(FileEntry {
                path,
                display,
                mru_rank: Some(rank),
            });
        }

        Ok(entries)
    }
}
