//! The global "recent directories" MRU (most-recently-used) cache.
//!
//! Wolf records every directory the captive Neovim changes into with a typed
//! `:cd`/`:lcd`/`:tcd`, and every folder a front-end opens (through
//! [`crate::WolfCore::record_recent_dir`]), in the `folder_mru` table of the
//! state database `<state_dir>/wolf.sqlite` (macOS
//! `~/Library/Application Support/Wolf/wolf.sqlite`), so Open Recent can offer
//! the user's recent project directories. Unlike the per-project file
//! [`crate::mru`] cache, this store is GLOBAL (one table under
//! [`crate::state::state_dir`]), keyed on the directory path, and capped at
//! [`RECENT_DIRS_LIMIT`] entries newest-first.
//!
//! The store is deliberately transient/disposable and robust:
//! - [`RecentDirs::record`] is fire-and-forget: it upserts a `(path, seq)` row,
//!   move-or-add-to-top, prunes beyond the cap, and swallows every error, so a
//!   transient cache problem never blocks a directory change.
//! - [`RecentDirs::recent`] returns the recorded directories newest-first, capped,
//!   and returns an empty `Vec` on any failure. It returns vanished directories
//!   too; [`crate::api::WolfCore::recent_dirs`] filters them out on read.
//! - The open, the table's versioning and the recovery of an unusable file are
//!   the shared ones of `db.rs`.
//!
//! Ordering is a monotonic `seq` column (`MAX(seq)+1` per insert), NOT a
//! timestamp, so move-to-top and the cap are deterministic even for
//! same-millisecond inserts (no sleeps needed in tests).

use std::path::{Path, PathBuf};

use crate::db::{Table, TablePolicy, TableSpec};

/// Maximum number of entries [`RecentDirs::recent`] returns (and the on-disk row
/// count [`RecentDirs::record`] prunes down to).
pub const RECENT_DIRS_LIMIT: usize = 20;

/// The store's table in `wolf.sqlite`.
static TABLE: TableSpec = TableSpec {
    name: "folder_mru",
    version: 1,
    columns: "path TEXT PRIMARY KEY, seq INTEGER NOT NULL",
    policy: TablePolicy::Cache,
};

/// The global recent-directories MRU cache: a handle to the `folder_mru` table
/// of `wolf.sqlite`, a silent no-op when disabled (see [`RecentDirs::disabled`]).
#[derive(Clone)]
pub struct RecentDirs {
    table: Table,
}

impl RecentDirs {
    /// Opens (creating if needed) the recent-directories cache in
    /// `dir/wolf.sqlite`.
    ///
    /// Infallible: it eagerly opens or creates the database, so the file exists
    /// right after `open`. A failure to open is silent, and the open is retried
    /// on the next `record`/`recent`.
    #[must_use]
    pub fn open(dir: &Path) -> RecentDirs {
        RecentDirs {
            table: Table::open(dir, &TABLE),
        }
    }

    /// A disabled store: every method is a silent no-op and [`recent`](Self::recent)
    /// is always empty. Used when [`crate::state::state_dir`] cannot be resolved.
    #[must_use]
    pub fn disabled() -> RecentDirs {
        RecentDirs {
            table: Table::disabled(&TABLE),
        }
    }

    /// Records `path` as the most-recently-used directory (fire-and-forget).
    ///
    /// `path` is stored as-given via `to_string_lossy` (its callers canonicalize
    /// it, so it is NOT re-canonicalized here). One
    /// transaction moves-or-adds the row to the top (a monotonic `MAX(seq)+1`
    /// stamp with an upsert) and prunes any rows beyond [`RECENT_DIRS_LIMIT`].
    /// Errors are swallowed (self-healing on corruption); a directory change is
    /// never blocked by a cache hiccup.
    pub fn record(&self, path: &Path) {
        let stored = path.to_string_lossy().into_owned();
        let limit = i64::try_from(RECENT_DIRS_LIMIT).unwrap_or(i64::MAX);
        let _ = self.table.with_conn(|conn| {
            let seq: i64 = conn.query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM folder_mru",
                [],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO folder_mru (path, seq) VALUES (?1, ?2) \
                 ON CONFLICT(path) DO UPDATE SET seq = excluded.seq",
                rusqlite::params![stored.as_str(), seq],
            )?;
            conn.execute(
                "DELETE FROM folder_mru WHERE path NOT IN \
                 (SELECT path FROM folder_mru ORDER BY seq DESC LIMIT ?1)",
                rusqlite::params![limit],
            )?;
            Ok(())
        });
    }

    /// Returns the recorded directories, newest first, capped at
    /// [`RECENT_DIRS_LIMIT`].
    ///
    /// Returns an empty `Vec` on any failure (disabled store, open/query error).
    /// Unlike [`crate::mru::MruStore::recent`] it neither prunes nor filters paths
    /// whose directory no longer exists: [`crate::api::WolfCore::recent_dirs`]
    /// omits those on read, leaving the row here so a returning folder reappears.
    #[must_use]
    pub fn recent(&self) -> Vec<PathBuf> {
        let limit = i64::try_from(RECENT_DIRS_LIMIT).unwrap_or(i64::MAX);
        self.table
            .with_conn(|conn| {
                let mut stmt =
                    conn.prepare("SELECT path FROM folder_mru ORDER BY seq DESC LIMIT ?1")?;
                let rows = stmt.query_map([limit], |row| row.get::<_, String>(0))?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(PathBuf::from(row?));
                }
                Ok(out)
            })
            .unwrap_or_default()
    }
}
