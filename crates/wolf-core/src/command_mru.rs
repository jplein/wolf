//! The global "command palette" MRU (most-recently-used) cache.
//!
//! Wolf records every command palette invocation the user activates — a static
//! registry command, a "switch to window", or an "open recent folder" — in the
//! `command_mru` table of the state database `<state_dir>/wolf.sqlite` (macOS
//! `~/Library/Application Support/Wolf/wolf.sqlite`), so the palette can float
//! the user's recently-used commands to the top. Like the global
//! [`crate::recent_dirs`] cache this store is GLOBAL (one table under
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
//! - The open, the table's versioning and the recovery of an unusable file are
//!   the shared ones of `db.rs`.
//!
//! Ordering is a monotonic `seq` column (`MAX(seq)+1` per insert), NOT a
//! timestamp, so move-to-top and the cap are deterministic even for
//! same-millisecond inserts (no sleeps needed in tests).

use std::path::Path;

use crate::db::{Table, TablePolicy, TableSpec};

/// Maximum number of entries [`CommandMru::recent`] returns (and the on-disk row
/// count [`CommandMru::record`] prunes down to).
pub const COMMAND_MRU_LIMIT: usize = 100;

/// The store's table in `wolf.sqlite`.
static TABLE: TableSpec = TableSpec {
    name: "command_mru",
    version: 1,
    columns: "key TEXT PRIMARY KEY, seq INTEGER NOT NULL",
    policy: TablePolicy::Cache,
};

/// The global command-palette MRU cache: a handle to the `command_mru` table of
/// `wolf.sqlite`, a silent no-op when disabled (see [`CommandMru::disabled`]).
#[derive(Clone)]
pub struct CommandMru {
    table: Table,
}

impl CommandMru {
    /// Opens (creating if needed) the command-palette MRU cache in
    /// `dir/wolf.sqlite`.
    ///
    /// Infallible: it eagerly opens or creates the database, so the file exists
    /// right after `open`. A failure to open is silent, and the open is retried
    /// on the next `record`/`recent`.
    #[must_use]
    pub fn open(dir: &Path) -> CommandMru {
        CommandMru {
            table: Table::open(dir, &TABLE),
        }
    }

    /// A disabled store: every method is a silent no-op and [`recent`](Self::recent)
    /// is always empty. Used when [`crate::state::state_dir`] cannot be resolved.
    #[must_use]
    pub fn disabled() -> CommandMru {
        CommandMru {
            table: Table::disabled(&TABLE),
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
        let _ = self.table.with_conn(|conn| {
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
        self.table
            .with_conn(|conn| {
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
}
