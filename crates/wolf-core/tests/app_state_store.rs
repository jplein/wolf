//! Headless tests of `wolf_core::AppStateStore`, the application-state store, over real SQLite
//! files in tempdirs. No mocks and no Neovim: each test passes a tempdir to
//! `AppStateStore::open` as the state directory.
//!
//! They pin `specs/application-state.md` §1 for the state class and §4 ("Saving"):
//!
//! - The window state round-trips through `window_state` in `wolf.sqlite`, and both state
//!   tables are registered in `schema`.
//! - An unchanged tree writes nothing, recency included, and a burst of submits writes once.
//!   `flush` returns only once the pending write was attempted.
//! - Each write upserts the project state of every directory with an open tab, chosen by §4's
//!   rule with the window's frame, and leaves every other row untouched. Empty tabs produce no
//!   row.
//! - `project_state` keeps at most `PROJECT_STATE_LIMIT` rows, evicting the least recently used,
//!   and reading a project state for Open Folder marks it used.
//! - A state table at a newer version is neither read nor written, and the other table still is.
//! - The window state and the project states land in one transaction.
//!
//! The database is read, seeded and tampered with through raw `rusqlite::Connection`s. The file
//! and table names are the spec's literals, and the column names (`id`, `state`, `directory`,
//! `last_used`) are the plan's. The versions a test tampers with are read from `schema` after a
//! normal open, never hard-coded.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use wolf_core::app_state::{
    Cursor, Drawer, Frame, Layout, Node, Pane, PaneOrientation, PaneRef, ProjectState, Sidebar,
    SidebarMode, Tab, TabContent, Window, WindowState,
};
use wolf_core::{AppStateStore, PROJECT_STATE_LIMIT};

/// The shared database's file name, from §1.
const DB_NAME: &str = "wolf.sqlite";

/// How long a test waits for the store's writer.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// How many distinct trees [`a_burst_writes_once`] submits back to back.
const BURST: usize = 10;

/// The text [`an_unchanged_tree_writes_nothing`] writes over the stored window state, which no
/// serialization of a window state can produce.
const SENTINEL: &str = "sentinel: not written by the store";

/// The directory prefix of the rows [`seed_lru`] creates.
const LRU_PREFIX: &str = "/lru/";

/// A directory beyond the seeded rows.
const LRU_NEW: &str = "/lru/new";

fn frame(x: f64, width: f64) -> Frame {
    Frame {
        x: Some(x),
        y: Some(x + 1.0),
        width,
        height: width / 2.0,
    }
}

/// Tab content whose lone edit pane is `marker.md` under `directory`, so tabs differ by marker.
fn content(directory: &str, marker: &str) -> TabContent {
    TabContent {
        root: Some(Node::Pane(Pane::Edit {
            path: format!("{directory}/{marker}.md"),
            cursor: None,
            top_line: None,
        })),
        ..TabContent::default()
    }
}

fn tab(directory: &str, marker: &str) -> Tab {
    Tab {
        directory: directory.to_owned(),
        content: content(directory, marker),
    }
}

fn empty_tab() -> Tab {
    Tab {
        directory: String::new(),
        content: TabContent::default(),
    }
}

/// A one-window state at `frame(0, width)` holding `tabs`, the first one active.
fn one_window(width: f64, tabs: Vec<Tab>) -> WindowState {
    WindowState {
        windows: vec![Window {
            frame: frame(0.0, width),
            active_tab: 0,
            minimized: false,
            tabs,
        }],
    }
}

/// A two-window state that sets every field of the model at least once.
fn full_state() -> WindowState {
    let pane = |path: &str, line: u32| {
        Node::Pane(Pane::Edit {
            path: path.to_owned(),
            cursor: Some(Cursor { line, col: 2 }),
            top_line: Some(line - 1),
        })
    };
    let front = TabContent {
        root: Some(Node::Layout(Layout {
            orientation: PaneOrientation::Vertical,
            weights: vec![0.625, 0.375],
            children: vec![
                pane("/full/one/a.md", 12),
                Node::Layout(Layout {
                    orientation: PaneOrientation::Horizontal,
                    weights: vec![0.5, 0.5],
                    children: vec![
                        Node::Pane(Pane::Preview {
                            path: "/full/one/a.md".to_owned(),
                            scroll: Some(0.25),
                        }),
                        Node::Pane(Pane::Terminal {
                            path: "/full/one/shell".to_owned(),
                        }),
                    ],
                }),
            ],
        })),
        focused: Some(PaneRef::Pane(1)),
        zoomed: Some(PaneRef::Pane(2)),
        sidebar: Sidebar {
            visible: false,
            mode: Some(SidebarMode::Search),
            query: Some("needle".to_owned()),
            case_sensitive: Some(true),
            use_regex: Some(false),
            all_files: Some(true),
        },
        drawer: Some(Drawer {
            open: true,
            share: 0.375,
        }),
    };
    let back = TabContent {
        root: Some(pane("/full/two/b.md", 40)),
        focused: Some(PaneRef::Drawer),
        zoomed: Some(PaneRef::Drawer),
        sidebar: Sidebar {
            visible: true,
            mode: Some(SidebarMode::Tree),
            ..Sidebar::default()
        },
        drawer: Some(Drawer {
            open: true,
            share: 0.5,
        }),
    };
    WindowState {
        windows: vec![
            Window {
                frame: frame(120.0, 1400.0),
                active_tab: 1,
                minimized: false,
                tabs: vec![
                    tab("/full/two", "first"),
                    Tab {
                        directory: "/full/one".to_owned(),
                        content: front,
                    },
                ],
            },
            Window {
                frame: Frame {
                    x: None,
                    y: None,
                    width: 1024.0,
                    height: 768.0,
                },
                active_tab: 0,
                minimized: true,
                tabs: vec![
                    empty_tab(),
                    Tab {
                        directory: "/full/two".to_owned(),
                        content: back,
                    },
                ],
            },
        ],
    }
}

/// Submits `state` with `recency` and asserts the write was attempted in time.
fn submit_and_flush(store: &AppStateStore, state: &WindowState, recency: Vec<Vec<usize>>) {
    store.submit(state.clone(), recency);
    assert!(
        store.flush(FLUSH_TIMEOUT),
        "flush must return true within {FLUSH_TIMEOUT:?}: the writer never attempted the write"
    );
}

/// A raw connection to `dir/wolf.sqlite`, outside the store.
fn raw(dir: &Path) -> Connection {
    Connection::open(dir.join(DB_NAME)).expect("failed to open wolf.sqlite with a raw connection")
}

/// The version `schema` records for `table`, or `None` when it has no row.
fn schema_version(conn: &Connection, table: &str) -> Option<i64> {
    conn.query_row(
        "SELECT version FROM schema WHERE name = ?1",
        [table],
        |row| row.get(0),
    )
    .optional()
    .expect("failed to read the schema table of wolf.sqlite")
}

/// Raises `table`'s `schema` version by one, as a newer build would, and returns the new version.
fn bump_version(dir: &Path, table: &str) -> i64 {
    let conn = raw(dir);
    let current = schema_version(&conn, table)
        .unwrap_or_else(|| panic!("precondition: schema must hold a row for {table}"));
    conn.execute(
        "UPDATE schema SET version = ?1 WHERE name = ?2",
        rusqlite::params![current + 1, table],
    )
    .expect("failed to bump the schema version");
    current + 1
}

/// The stored window state's raw text, or `None` when there is no row.
fn window_row(conn: &Connection) -> Option<String> {
    conn.query_row("SELECT state FROM window_state WHERE id = 1", [], |row| {
        row.get(0)
    })
    .optional()
    .expect("failed to read the window_state table")
}

/// The raw `(state, last_used)` of `directory`'s project state, or `None` when it has no row.
fn project_row(conn: &Connection, directory: &str) -> Option<(String, i64)> {
    conn.query_row(
        "SELECT state, last_used FROM project_state WHERE directory = ?1",
        [directory],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .expect("failed to read the project_state table")
}

/// Every directory `project_state` holds, sorted.
fn directories(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT directory FROM project_state ORDER BY directory")
        .expect("wolf.sqlite must have a project_state table");
    stmt.query_map([], |row| row.get(0))
        .expect("failed to query project_state")
        .collect::<rusqlite::Result<_>>()
        .expect("failed to read a project_state row")
}

/// The window state the raw row holds, parsed, or `None` when there is no row.
fn stored_window_state(conn: &Connection) -> Option<WindowState> {
    window_row(conn).map(|text| {
        serde_json::from_str::<WindowState>(&text)
            .unwrap_or_else(|err| panic!("the stored window state must parse: {err}\n{text}"))
    })
}

/// The `/lru/NNN` directory of seeded row `i`.
fn lru_dir(i: usize) -> String {
    format!("{LRU_PREFIX}{i:03}")
}

/// The project state seeded for `directory`.
fn seeded_project(directory: &str) -> ProjectState {
    ProjectState {
        frame: frame(5.0, 640.0),
        tab: content(directory, "seeded"),
    }
}

/// Fills `project_state` to the limit: rows `/lru/000` to `/lru/199`, with `last_used` 1 to 200,
/// so `/lru/000` is the least recently used.
fn seed_lru(dir: &Path) {
    let mut conn = raw(dir);
    let tx = conn.transaction().expect("failed to begin the seed");
    for i in 0..PROJECT_STATE_LIMIT {
        let directory = lru_dir(i);
        let json = serde_json::to_string(&seeded_project(&directory))
            .expect("failed to serialize a seeded project state");
        tx.execute(
            "INSERT INTO project_state (directory, last_used, state) VALUES (?1, ?2, ?3)",
            rusqlite::params![directory, i as i64 + 1, json],
        )
        .expect("failed to seed a project_state row");
    }
    tx.commit().expect("failed to commit the seed");
}

#[test]
fn the_limit_is_the_spec_s() {
    assert_eq!(
        PROJECT_STATE_LIMIT, 200,
        "§1 sets PROJECT_STATE_LIMIT to 200 rows"
    );
}

#[test]
fn the_window_state_round_trips_through_the_store() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let state = full_state();
    {
        let store = AppStateStore::open(dir.path());
        assert_eq!(
            store.window_state(),
            None,
            "premise: a fresh state dir has no saved window state"
        );
        submit_and_flush(&store, &state, vec![vec![1, 0], vec![0, 1]]);
    }

    assert_eq!(
        AppStateStore::open(dir.path()).window_state(),
        Some(state),
        "a second store on the same dir must read back the submitted window state; None means \
         the writer never stored it, and a different state means a field is lost on the way"
    );
    let conn = raw(dir.path());
    for table in ["window_state", "project_state"] {
        assert!(
            schema_version(&conn, table).is_some(),
            "schema must register {table}, so its version is tracked"
        );
    }
}

#[test]
fn an_unchanged_tree_writes_nothing() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let store = AppStateStore::open(dir.path());
    let state = one_window(800.0, vec![tab("/same/a", "a"), tab("/same/a", "b")]);
    submit_and_flush(&store, &state, vec![vec![0, 1]]);

    let conn = raw(dir.path());
    let replaced = conn
        .execute(
            "UPDATE window_state SET state = ?1 WHERE id = 1",
            [SENTINEL],
        )
        .expect("failed to overwrite the stored window state");
    assert_eq!(
        replaced, 1,
        "premise: the first submit must have stored a row"
    );

    submit_and_flush(&store, &state, vec![vec![0, 1]]);
    assert_eq!(
        window_row(&conn).as_deref(),
        Some(SENTINEL),
        "resubmitting the same tree must write nothing; the row was rewritten"
    );

    submit_and_flush(&store, &state, vec![vec![1, 0]]);
    assert_eq!(
        window_row(&conn).as_deref(),
        Some(SENTINEL),
        "the same tree with another recency must write nothing, since recency is not persisted; \
         the row was rewritten"
    );
}

#[test]
fn a_burst_writes_once() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let store = AppStateStore::open(dir.path());
    let conn = raw(dir.path());
    conn.execute_batch(
        "CREATE TABLE writes (n INTEGER);
         CREATE TRIGGER count_insert AFTER INSERT ON window_state BEGIN
             INSERT INTO writes (n) VALUES (1);
         END;
         CREATE TRIGGER count_update AFTER UPDATE ON window_state BEGIN
             INSERT INTO writes (n) VALUES (1);
         END;",
    )
    .expect("failed to install the write counter");

    let trees: Vec<WindowState> = (0..BURST)
        .map(|i| one_window(800.0 + i as f64, vec![tab("/burst", "a")]))
        .collect();
    for tree in &trees {
        store.submit(tree.clone(), vec![vec![0]]);
    }
    assert!(store.flush(FLUSH_TIMEOUT), "flush must return true");

    let writes: i64 = conn
        .query_row("SELECT COUNT(*) FROM writes", [], |row| row.get(0))
        .expect("failed to count the writes");
    assert_eq!(
        writes, 1,
        "{BURST} submits in a tight loop must coalesce into one write of window_state; {writes} \
         means each submit wrote on its own, and 0 means nothing was written"
    );
    assert_eq!(
        stored_window_state(&conn).as_ref(),
        trees.last(),
        "the one write must store the last tree of the burst"
    );
}

#[test]
fn flush_returns_once_the_pending_write_is_attempted() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let store = AppStateStore::open(dir.path());
    let state = one_window(900.0, vec![tab("/flush", "a")]);
    store.submit(state.clone(), vec![vec![0]]);
    assert!(store.flush(FLUSH_TIMEOUT), "flush must return true");
    assert_eq!(
        stored_window_state(&raw(dir.path())),
        Some(state),
        "right after flush returns the row must hold the submitted tree, with no poll; None \
         means flush returned before the writer ran"
    );
}

#[test]
fn a_directory_whose_tabs_close_keeps_its_row_untouched() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let store = AppStateStore::open(dir.path());
    let (d1, d2) = ("/close/one", "/close/two");
    submit_and_flush(
        &store,
        &one_window(800.0, vec![tab(d1, "a"), tab(d2, "b")]),
        vec![vec![0, 1]],
    );
    let conn = raw(dir.path());
    let before = project_row(&conn, d2).expect("premise: the first write must store D2's row");

    submit_and_flush(
        &store,
        &one_window(800.0, vec![tab(d1, "c")]),
        vec![vec![0]],
    );
    assert_eq!(
        project_row(&conn, d2),
        Some(before),
        "D2 has no open tab, so its row must be left byte for byte, last_used included; None \
         means absence deleted it, and a new last_used means the write touched every row"
    );
    assert_eq!(
        store.project_state(d2),
        Some(ProjectState {
            frame: frame(0.0, 800.0),
            tab: content(d2, "b"),
        }),
        "Open Folder on D2 must read back the tab it had when it was last open"
    );
}

#[test]
fn an_empty_tab_produces_no_project_state() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let store = AppStateStore::open(dir.path());
    submit_and_flush(
        &store,
        &one_window(800.0, vec![empty_tab(), tab("/only", "a")]),
        vec![vec![0, 1]],
    );
    assert_eq!(
        directories(&raw(dir.path())),
        ["/only"],
        "the empty tab, the active one, must produce no project state; a row for \"\" means \
         empty tabs are not skipped"
    );
}

#[test]
fn the_limit_evicts_the_least_recently_used() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let store = AppStateStore::open(dir.path());
    seed_lru(dir.path());
    let oldest = lru_dir(0);
    let next_oldest = lru_dir(1);

    submit_and_flush(
        &store,
        &one_window(800.0, vec![tab(&oldest, "open"), tab(LRU_NEW, "open")]),
        vec![vec![0, 1]],
    );
    let held = directories(&raw(dir.path()));
    assert_eq!(
        held.len(),
        PROJECT_STATE_LIMIT,
        "the write must prune project_state back to {PROJECT_STATE_LIMIT} rows; more means it \
         does not enforce the limit"
    );
    for kept in [oldest.as_str(), LRU_NEW] {
        assert!(
            held.iter().any(|d| d == kept),
            "{kept} was written just now, so it must be kept. A missing {oldest} means the write \
             did not refresh last_used on an existing row, or the prune keeps the oldest rows"
        );
    }
    assert!(
        !held.contains(&next_oldest),
        "{next_oldest} is the least recently used row after the write, so it must be evicted"
    );
}

#[test]
fn opening_a_folder_marks_its_project_state_used() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let store = AppStateStore::open(dir.path());
    seed_lru(dir.path());
    let oldest = lru_dir(0);
    let next_oldest = lru_dir(1);

    assert_eq!(
        store.project_state(&oldest),
        Some(seeded_project(&oldest)),
        "Open Folder must read back the seeded project state"
    );
    submit_and_flush(
        &store,
        &one_window(800.0, vec![tab(LRU_NEW, "open")]),
        vec![vec![0]],
    );
    let held = directories(&raw(dir.path()));
    assert!(
        held.contains(&oldest),
        "{oldest} was read by Open Folder, which marks it used, so the prune must keep it; its \
         eviction means the read did not set last_used"
    );
    assert!(
        !held.contains(&next_oldest),
        "{next_oldest} is then the least recently used row, so it must be evicted"
    );
}

#[test]
fn the_chosen_tab_and_its_window_frame_become_the_project_state() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let store = AppStateStore::open(dir.path());
    let d = "/chosen";
    let state = WindowState {
        windows: vec![
            Window {
                frame: frame(10.0, 800.0),
                active_tab: 0,
                minimized: false,
                tabs: vec![tab("/x", "x"), tab(d, "a"), tab(d, "b")],
            },
            Window {
                frame: frame(500.0, 1200.0),
                active_tab: 0,
                minimized: false,
                tabs: vec![tab("/y", "y"), tab(d, "c")],
            },
        ],
    };
    submit_and_flush(&store, &state, vec![vec![0, 2, 1], vec![0, 1]]);
    assert_eq!(
        store.project_state(d),
        Some(ProjectState {
            frame: frame(10.0, 800.0),
            tab: content(d, "b"),
        }),
        "no active tab is in D, so the front window's most recent D tab, b, must be stored with \
         that window's frame. Tab a means the recency was not passed to the rule; tab c or the \
         second frame means the windows were searched from the back"
    );
}

#[test]
fn a_newer_window_state_table_is_neither_read_nor_written() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let d = "/newer/window";
    let first = one_window(800.0, vec![tab(d, "first")]);
    {
        let store = AppStateStore::open(dir.path());
        submit_and_flush(&store, &first, vec![vec![0]]);
    }
    let newer = bump_version(dir.path(), "window_state");
    let conn = raw(dir.path());
    let row = window_row(&conn).expect("premise: the first store must have written the row");

    let store = AppStateStore::open(dir.path());
    assert_eq!(
        store.window_state(),
        None,
        "window_state is at a newer version, so launch must read nothing saved"
    );
    let second = one_window(900.0, vec![tab(d, "second")]);
    submit_and_flush(&store, &second, vec![vec![0]]);
    assert_eq!(
        schema_version(&conn, "window_state"),
        Some(newer),
        "the newer build's version must survive; an older number means this build recorded its \
         own version over it"
    );
    assert_eq!(
        window_row(&conn),
        Some(row),
        "the newer build's window state must survive this build's write unchanged"
    );
    assert_eq!(
        store.project_state(d),
        Some(ProjectState {
            frame: frame(0.0, 900.0),
            tab: content(d, "second"),
        }),
        "project_state is at this build's version, so it must still be written; the first tab \
         means a newer window_state stopped the whole write"
    );
}

#[test]
fn a_newer_project_state_table_is_neither_read_nor_written() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let d = "/newer/project";
    let first = one_window(800.0, vec![tab(d, "first")]);
    {
        let store = AppStateStore::open(dir.path());
        submit_and_flush(&store, &first, vec![vec![0]]);
    }
    let newer = bump_version(dir.path(), "project_state");
    let conn = raw(dir.path());
    let row = project_row(&conn, d).expect("premise: the first store must have written D's row");

    let store = AppStateStore::open(dir.path());
    assert_eq!(
        store.project_state(d),
        None,
        "project_state is at a newer version, so Open Folder must read nothing saved"
    );
    let second = one_window(900.0, vec![tab(d, "second")]);
    submit_and_flush(&store, &second, vec![vec![0]]);
    assert_eq!(
        schema_version(&conn, "project_state"),
        Some(newer),
        "the newer build's version must survive; an older number means this build recorded its \
         own version over it"
    );
    assert_eq!(
        project_row(&conn, d),
        Some(row),
        "the newer build's row for D must survive unchanged, state and last_used, through this \
         build's read and write"
    );
    assert_eq!(
        AppStateStore::open(dir.path()).window_state(),
        Some(second),
        "window_state is at this build's version, so it must still be written; the first state \
         means a newer project_state stopped the whole write"
    );
}

#[test]
fn the_window_state_and_project_states_commit_in_one_transaction() {
    let dir = tempfile::tempdir().expect("failed to create the state tempdir");
    let store = AppStateStore::open(dir.path());
    let (d1, d2) = ("/tx/one", "/tx/two");
    let first = one_window(800.0, vec![tab(d1, "a")]);
    submit_and_flush(&store, &first, vec![vec![0]]);

    let conn = raw(dir.path());
    conn.execute_batch(
        "CREATE TRIGGER fail_project BEFORE INSERT ON project_state BEGIN
             SELECT RAISE(ABORT, 'test');
         END;",
    )
    .expect("failed to install the failing trigger");
    // The window state is written before the project states, so only a transaction undoes it.
    let second = one_window(900.0, vec![tab(d1, "b"), tab(d2, "c")]);
    submit_and_flush(&store, &second, vec![vec![0, 1]]);

    assert_eq!(
        AppStateStore::open(dir.path()).window_state(),
        Some(first),
        "a write whose project state insert fails must leave the window state as it was. The \
         second state means the statements ran outside one transaction; None means the failure \
         was taken for a broken file and the database was deleted"
    );
    assert_eq!(
        project_row(&conn, d2),
        None,
        "the failing insert must leave no row for D2"
    );
}
