//! The application-state store: the `window_state` and `project_state` tables of
//! `wolf.sqlite`, written through a coalescing writer thread.
//!
//! The front-end submits the whole [`WindowState`] plus, per window, its tabs'
//! selection recency, which is input to §4's rule and never persisted. Each write
//! is one transaction that replaces the window state, upserts the project state of
//! every directory with an open tab, chosen by [`app_state::project_states`],
//! leaves every other project state untouched, and enforces
//! [`PROJECT_STATE_LIMIT`]. See `specs/application-state.md#4-saving` and
//! `specs/application-state.md#size-limits-and-pruning`.
//!
//! Both tables are state tables (see the `db` module): one written by a newer
//! build is neither read nor written by this store for the rest of the process,
//! and the other table still is.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior};

use crate::app_state::{self, ProjectState, WindowState};
use crate::db::{Table, TablePolicy, TableSpec};

/// The most rows `project_state` keeps; each write deletes the least recently
/// used beyond it.
pub const PROJECT_STATE_LIMIT: usize = 200;

/// How long the writer sleeps after being woken, so a burst of submits coalesces
/// into one write.
const WRITE_COALESCE_DELAY: Duration = Duration::from_millis(100);

/// The writer's idle re-check cadence, at which it notices that every
/// [`AppStateStore`] handle is gone and exits.
const WRITER_IDLE_POLL: Duration = Duration::from_millis(500);

/// The one-row window state table.
static WINDOW_STATE: TableSpec = TableSpec {
    name: "window_state",
    version: 1,
    columns: "id INTEGER PRIMARY KEY CHECK (id = 1), state TEXT NOT NULL",
    policy: TablePolicy::State { migrations: &[] },
};

/// The per-directory project state table. `last_used` is a counter, not a clock,
/// as `folder_mru.seq` is, so eviction is deterministic for same-millisecond writes.
static PROJECT_STATE: TableSpec = TableSpec {
    name: "project_state",
    version: 1,
    columns: "directory TEXT PRIMARY KEY, last_used INTEGER NOT NULL, state TEXT NOT NULL",
    policy: TablePolicy::State { migrations: &[] },
};

/// What the writer thread and [`AppStateStore::flush`] rendezvous on.
/// `generation` counts changed trees; `written` is the highest generation whose
/// write was attempted, failures included, so `flush` cannot hang on a bad disk.
struct Cell {
    state: Option<WindowState>,
    recency: Vec<Vec<usize>>,
    generation: u64,
    written: u64,
}

/// The shared innards behind every clone of an [`AppStateStore`].
struct Inner {
    table: Table,
    cell: Mutex<Cell>,
    cond: Condvar,
}

/// A cloneable handle to the application-state tables of `wolf.sqlite`.
///
/// All operations are infallible: a failure logs at `debug` and reads as nothing
/// saved, or drops the write. Clones share one writer thread, which exits once
/// every handle is dropped and nothing is pending. The unchanged-tree check is per
/// store, so a front-end opens one per process.
#[derive(Clone)]
pub struct AppStateStore {
    inner: Arc<Inner>,
}

impl AppStateStore {
    /// Opens both tables of `dir/wolf.sqlite` on one connection and spawns the
    /// `wolf-app-state-writer` thread. Never fails: a store whose writer could not
    /// spawn still reads, and `flush` then times out.
    #[must_use]
    pub fn open(dir: &Path) -> AppStateStore {
        let inner = Arc::new(Inner {
            table: Table::open_all(dir, &[&WINDOW_STATE, &PROJECT_STATE]),
            cell: Mutex::new(Cell {
                state: None,
                recency: Vec::new(),
                generation: 0,
                written: 0,
            }),
            cond: Condvar::new(),
        });
        let writer_inner = Arc::clone(&inner);
        let spawned = std::thread::Builder::new()
            .name("wolf-app-state-writer".to_string())
            .spawn(move || writer_loop(&writer_inner));
        if let Err(e) = spawned {
            tracing::warn!(
                "wolf-core: could not spawn the app-state writer thread: {e}; \
                 state changes will not be persisted"
            );
        }
        AppStateStore { inner }
    }

    /// The saved window state, for launch. `None` when nothing is saved, when the
    /// row does not parse, or when `window_state` was written by a newer build.
    #[must_use]
    pub fn window_state(&self) -> Option<WindowState> {
        let table = &self.inner.table;
        let text = table
            .with_conn(|conn| {
                if table.is_newer(&WINDOW_STATE) {
                    return Ok(None);
                }
                conn.query_row("SELECT state FROM window_state WHERE id = 1", [], |row| {
                    row.get::<_, String>(0)
                })
                .optional()
            })
            .flatten()?;
        serde_json::from_str(&text)
            .map_err(|e| tracing::debug!(error = %e, "saved window state does not parse"))
            .ok()
    }

    /// `directory`'s project state, for Open Folder, which also marks it the most
    /// recently used, as §1 says. `None` when there is none, when its row does
    /// not parse, which leaves the row as it is, or when `project_state` was
    /// written by a newer build, which touches nothing.
    #[must_use]
    pub fn project_state(&self, directory: &str) -> Option<ProjectState> {
        let table = &self.inner.table;
        table
            .with_conn(|conn| {
                if table.is_newer(&PROJECT_STATE) {
                    return Ok(None);
                }
                let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
                let text: Option<String> = tx
                    .query_row(
                        "SELECT state FROM project_state WHERE directory = ?1",
                        [directory],
                        |row| row.get(0),
                    )
                    .optional()?;
                let Some(state) = text.and_then(|text| parse_project(directory, &text)) else {
                    return Ok(None);
                };
                tx.execute(
                    "UPDATE project_state \
                     SET last_used = (SELECT MAX(last_used) FROM project_state) + 1 \
                     WHERE directory = ?1",
                    [directory],
                )?;
                tx.commit()?;
                Ok(Some(state))
            })
            .flatten()
    }

    /// Submits the whole window state, with `recency[w]` window `w`'s tab indices,
    /// most recently selected first.
    ///
    /// Wakes the writer only when `state` differs from the last submitted tree.
    /// A changed `recency` with an equal tree is kept for the next write and
    /// writes nothing, since recency is not persisted. The first submit always
    /// writes.
    pub fn submit(&self, state: WindowState, recency: Vec<Vec<usize>>) {
        let mut cell = self.inner.cell.lock();
        cell.recency = recency;
        if cell.state.as_ref() == Some(&state) {
            return;
        }
        cell.state = Some(state);
        cell.generation += 1;
        self.inner.cond.notify_all();
    }

    /// Blocks until every submit so far has had its write attempted, failures
    /// included, or until `timeout` elapses. Returns whether everything pending
    /// was attempted in time.
    #[must_use]
    pub fn flush(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut cell = self.inner.cell.lock();
        while cell.written < cell.generation {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            self.inner.cond.wait_for(&mut cell, deadline - now);
        }
        true
    }
}

/// Parses a stored project state, logging a row that does not parse.
fn parse_project(directory: &str, text: &str) -> Option<ProjectState> {
    serde_json::from_str(text)
        .map_err(|e| tracing::debug!(directory, error = %e, "saved project state does not parse"))
        .ok()
}

/// The `wolf-app-state-writer` thread body.
///
/// It waits for a pending generation, sleeps [`WRITE_COALESCE_DELAY`] with the
/// lock dropped, takes the latest tree and recency, writes them with no lock held,
/// then advances `written` and wakes any flusher.
fn writer_loop(inner: &Arc<Inner>) {
    loop {
        {
            let mut cell = inner.cell.lock();
            while cell.generation <= cell.written {
                let timed_out = inner.cond.wait_for(&mut cell, WRITER_IDLE_POLL).timed_out();
                // Exit once every handle is gone. Re-check `generation <= written`: a last
                // handle can submit and drop between the timeout and the relock.
                if timed_out && Arc::strong_count(inner) == 1 && cell.generation <= cell.written {
                    return;
                }
            }
        }
        std::thread::sleep(WRITE_COALESCE_DELAY);
        let (state, recency, target) = {
            let cell = inner.cell.lock();
            (cell.state.clone(), cell.recency.clone(), cell.generation)
        };
        if let Some(state) = state {
            write(&inner.table, &state, &recency);
        }
        let mut cell = inner.cell.lock();
        if target > cell.written {
            cell.written = target;
        }
        inner.cond.notify_all();
    }
}

/// Writes `state` and its project states in one immediate transaction: the
/// window state first, then every chosen project state with one fresh
/// `last_used`, then the prune. A table written by a newer build is skipped.
///
/// The JSON is serialized up front, because `with_conn` may run its closure twice.
fn write(table: &Table, state: &WindowState, recency: &[Vec<usize>]) {
    let window_json = match serde_json::to_string(state) {
        Ok(json) => json,
        Err(e) => {
            tracing::debug!(error = %e, "could not serialize the window state");
            return;
        }
    };
    let projects: Vec<(String, String)> = app_state::project_states(state, recency)
        .into_iter()
        .filter_map(
            |(directory, project)| match serde_json::to_string(&project) {
                Ok(json) => Some((directory, json)),
                Err(e) => {
                    tracing::debug!(directory, error = %e, "could not serialize a project state");
                    None
                }
            },
        )
        .collect();
    let limit = i64::try_from(PROJECT_STATE_LIMIT).unwrap_or(i64::MAX);
    table.with_conn(|conn| {
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
        if !table.is_newer(&WINDOW_STATE) {
            tx.execute(
                "INSERT INTO window_state (id, state) VALUES (1, ?1) \
                 ON CONFLICT(id) DO UPDATE SET state = excluded.state",
                [&window_json],
            )?;
        }
        if !table.is_newer(&PROJECT_STATE) {
            let next: i64 = tx.query_row(
                "SELECT COALESCE(MAX(last_used), 0) + 1 FROM project_state",
                [],
                |row| row.get(0),
            )?;
            for (directory, json) in &projects {
                tx.execute(
                    "INSERT INTO project_state (directory, last_used, state) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(directory) DO UPDATE SET \
                     last_used = excluded.last_used, state = excluded.state",
                    rusqlite::params![directory, next, json],
                )?;
            }
            tx.execute(
                "DELETE FROM project_state WHERE directory NOT IN (\
                     SELECT directory FROM project_state \
                     ORDER BY last_used DESC, directory LIMIT ?1\
                 )",
                [limit],
            )?;
        }
        tx.commit()
    });
}
