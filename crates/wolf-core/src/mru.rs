//! The most-recently-used (MRU) file cache.
//!
//! Wolf records every file opened through the editor in the GLOBAL `file_mru`
//! table of the state database `<state_dir>/wolf.sqlite` (macOS
//! `~/Library/Application Support/Wolf/wolf.sqlite`), so the file picker can
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
//! - The open, the table's versioning and the recovery of an unusable file are
//!   the shared ones of `db.rs`.
//!
//! Unlike the sibling stores, ordering is by an `opened_at` millisecond timestamp
//! (most-recent first, `rowid DESC` tiebreak) rather than a monotonic `seq`, and
//! the missing-file prune is inline in `recent`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::{Table, TablePolicy, TableSpec};

/// [`FileEntry`] lives in the [`crate::files`] module; it is re-exported here
/// so existing `mru::FileEntry` references keep resolving.
pub use crate::files::FileEntry;

/// Maximum number of entries [`MruStore::recent`] returns (per directory).
pub const MRU_LIMIT: usize = 50;

/// The store's table in `wolf.sqlite`.
static TABLE: TableSpec = TableSpec {
    name: "file_mru",
    version: 1,
    columns: "directory TEXT NOT NULL, file TEXT NOT NULL, opened_at INTEGER NOT NULL, \
              PRIMARY KEY (directory, file)",
    policy: TablePolicy::Cache,
};

/// The global recent-files MRU cache: a handle to the `file_mru` table of
/// `wolf.sqlite`, a silent no-op when disabled (see [`MruStore::disabled`]).
#[derive(Clone)]
pub struct MruStore {
    table: Table,
}

impl MruStore {
    /// Opens (creating if needed) the recent-files cache in `dir/wolf.sqlite`.
    ///
    /// Infallible: it eagerly opens or creates the database, so the file exists
    /// right after `open`. A failure to open is silent, and the open is retried
    /// on the next `record`/`recent`. Does NOT create any `.wolf` directory under
    /// a project directory; the MRU lives entirely under `dir`.
    #[must_use]
    pub fn open(dir: &Path) -> MruStore {
        MruStore {
            table: Table::open(dir, &TABLE),
        }
    }

    /// A disabled store: every method is a silent no-op and [`recent`](Self::recent)
    /// is always empty. Used when [`crate::state::state_dir`] cannot be resolved.
    #[must_use]
    pub fn disabled() -> MruStore {
        MruStore {
            table: Table::disabled(&TABLE),
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

        let _ = self.table.with_conn(|conn| {
            conn.execute(
                "INSERT INTO file_mru (directory, file, opened_at) VALUES (?1, ?2, ?3) \
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

        self.table
            .with_conn(|conn| {
                let candidates: Vec<String> = {
                    let mut stmt = conn.prepare(
                        "SELECT file FROM file_mru WHERE directory = ?1 \
                     ORDER BY opened_at DESC, rowid DESC LIMIT ?2",
                    )?;
                    let rows = stmt
                        .query_map(rusqlite::params![dir_key.as_str(), limit], |row| {
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
                            "DELETE FROM file_mru WHERE directory = ?1 AND file = ?2",
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
