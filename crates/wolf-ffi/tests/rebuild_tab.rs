//! Real-nvim integration test for `WolfCoreHandle::rebuild_tab(&FfiAppTab)`: a saved tab,
//! committed through the `wolf_ffi_app_state_*` builder, read back through
//! `wolf_ffi_app_state_window_state` and passed through `FfiAppTab::restored` as the macOS
//! launch does, is rendered in Rust by `wolf_core::app_state::tab_rebuild_spec` and realized in
//! the current tabpage's lone window.
//!
//! ## What this asserts
//!
//! A vertical tab of two edit panes rebuilds into two distinct, nonzero window handles in leaf
//! order: `handles[0]` shows the left pane's file and `handles[1]` the right pane's. A renderer
//! that drops a pane fails the count, and one that reverses the children fails the per-handle
//! paths.
//!
//! A second tab, of an edit pane and a terminal pane, is rebuilt on a fresh core: the terminal
//! pane's handle carries a core session (`win_buffer(h).terminal() != 0`), and the edit pane's
//! carries none.
//!
//! ## One `#[test]`
//!
//! The whole flow lives in one `#[test]`: it sets the process-global `WOLF_STATE_DIR` before the
//! process-wide app-state store first opens, so a second test in this binary would race both.
//!
//! ## Skip when no nvim
//!
//! Loudly skips (a no-op pass) when no pinned nvim is available, through
//! `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::collections::HashSet;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{
    wolf_ffi_app_state_add_tab, wolf_ffi_app_state_add_window, wolf_ffi_app_state_begin,
    wolf_ffi_app_state_commit, wolf_ffi_app_state_commit_root, wolf_ffi_app_state_flush,
    wolf_ffi_app_state_push_layout, wolf_ffi_app_state_push_pane, wolf_ffi_app_state_push_recency,
    wolf_ffi_app_state_window_state, wolf_ffi_start, FfiAppTab, WolfCoreHandle,
};

/// Generous overall timeout for nvim startup and each rebuilt window's buffer to settle.
/// Mirrors the sibling tests.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll cadence while waiting for an effect to surface.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The lowest real WINDOW grid id (grid 1 is the outer global grid).
const WINDOW_GRID_MIN: u64 = 2;
/// Live editor windows at startup, before the rebuild.
const ONE_WINDOW: usize = 1;

// `pane_kind`, mirroring the bridge's private table.
const EDIT: u8 = 0;
const TERMINAL: u8 = 2;
// `pane_ref`
const REF_NONE: u8 = 0;
// `orientation`
const VERTICAL: u8 = 1;

/// The number of panes in each saved tab, and so of rebuilt windows.
const TWO_PANES: u32 = 2;
/// Each child's weight in the saved 0.5/0.5 layout.
const HALF_WEIGHT: f64 = 0.5;
/// The root node's weight.
const ROOT_WEIGHT: f64 = 1.0;

/// The two edit panes' basenames, in leaf order. Neither is the startup buffer's name.
const LEFT_NAME: &str = "left.md";
const RIGHT_NAME: &str = "right.md";

/// The directory, under the core's cwd, the saved terminal pane starts in.
const TERMINAL_DIR_NAME: &str = "term-dir";

/// The saved geometry of the one window each commit submits (the rebuild never reads it).
const WIN_FRAME: (f64, f64, f64, f64) = (100.0, 200.0, 800.0, 600.0);

/// The live editor WINDOW grids (id + win handle) from `windows()`: grids >=
/// `WINDOW_GRID_MIN` that are not hidden. Copy of `panes.rs`'s `live_windows`.
fn live_windows(handle: &WolfCoreHandle) -> Vec<(u64, i64)> {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .map(|w| (w.grid(), w.win()))
        .collect()
}

/// Polls `pred` until it returns `true` or [`SETTLE_TIMEOUT`] elapses; panics with `desc` on
/// timeout.
fn poll(desc: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + SETTLE_TIMEOUT;
    while Instant::now() < deadline {
        if pred() {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {SETTLE_TIMEOUT:?} waiting for: {desc}");
}

/// The basename of the buffer window `win` shows, or `None` while the window has no grid yet or
/// shows an unnamed buffer.
fn win_basename(handle: &WolfCoreHandle, win: i64) -> Option<String> {
    let grid = handle.grid_for_win(win);
    if grid == 0 {
        return None;
    }
    let abs = handle.pane_status_for_grid(grid).abs_path();
    Path::new(&abs)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned)
}

/// Polls until window `win` shows a buffer named `expected`, panicking with the last observed
/// basename on timeout. Matches on the basename, because nvim reports the temp dir under
/// `/private/var` where the test created it under `/var`.
fn poll_win_basename(handle: &WolfCoreHandle, win: i64, leaf: usize, expected: &str) {
    let deadline = Instant::now() + SETTLE_TIMEOUT;
    loop {
        let observed = win_basename(handle, win);
        if observed.as_deref() == Some(expected) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "rebuilt window {win} (leaf {leaf}) must show {expected:?}; last observed \
                 {observed:?}. None means the handle never got a grid (not a live window); \
                 the other leaf's name means the leaves were realized out of order"
            );
        }
        sleep(POLL_INTERVAL);
    }
}

/// Commits one window holding one tab on `directory`, a vertical layout of the two panes
/// `(kind, path)`, flushes it, and returns the tab read back and passed through `restored()`.
fn commit_and_restore(directory: &str, panes: [(u8, String); 2]) -> FfiAppTab {
    wolf_ffi_app_state_begin();
    wolf_ffi_app_state_add_window(true, WIN_FRAME.0, WIN_FRAME.1, WIN_FRAME.2, WIN_FRAME.3, 0);
    wolf_ffi_app_state_add_tab(directory.to_owned(), false, REF_NONE, 0, REF_NONE, 0);
    for (kind, path) in panes {
        wolf_ffi_app_state_push_pane(kind, path, HALF_WEIGHT);
    }
    wolf_ffi_app_state_push_layout(VERTICAL, TWO_PANES, ROOT_WEIGHT);
    wolf_ffi_app_state_commit_root();
    wolf_ffi_app_state_push_recency(0);
    wolf_ffi_app_state_commit();
    assert!(
        wolf_ffi_app_state_flush(),
        "premise: the committed window state's write must be attempted within the flush timeout"
    );

    let windows = wolf_ffi_app_state_window_state().windows();
    let window = windows
        .first()
        .expect("premise: the committed window state must read back with its one window");
    let tabs = window.tabs();
    let tab = tabs
        .first()
        .expect("premise: the read-back window must carry its one tab")
        .restored();
    assert!(
        tab.has_root() && tab.root().child_count() == TWO_PANES,
        "premise: the restored tab must be the two-pane layout that was saved; otherwise the \
         rebuild below tests nothing"
    );
    tab
}

/// Starts a core on `cwd` and waits for it to attach with its lone starting window.
fn start_core(cwd: &str) -> WolfCoreHandle {
    let handle = wolf_ffi_start(
        common::init_lua_path().to_string_lossy().into_owned(),
        cwd.to_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    poll("the core to attach (read_frame cols > 0)", || {
        handle.read_frame().cols() > 0
    });
    poll("a single live editor window at startup", || {
        live_windows(&handle).len() == ONE_WINDOW
    });
    handle
}

#[test]
fn rebuild_tab_realizes_a_restored_saved_tab() {
    let Some(_) = common::nvim_bin_or_skip() else {
        return;
    };

    // A private, empty state dir, so the read-back can only see this test's commits. Safe to
    // set because this binary has exactly one test, and before the app-state store first opens.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    // The core's cwd, holding the two panes' files. It must outlive the session.
    let cwd = tempfile::tempdir().expect("failed to create temp cwd directory");
    let left = cwd.path().join(LEFT_NAME);
    let right = cwd.path().join(RIGHT_NAME);
    std::fs::write(&left, "# left\n").expect("failed to write the left pane's file");
    std::fs::write(&right, "# right\n").expect("failed to write the right pane's file");
    let cwd_str = cwd.path().to_string_lossy().into_owned();
    let left_str = left.to_string_lossy().into_owned();

    // --- 1. Two edit panes rebuild in leaf order. ------------------------------------------
    let tab = commit_and_restore(
        &cwd_str,
        [
            (EDIT, left_str.clone()),
            (EDIT, right.to_string_lossy().into_owned()),
        ],
    );
    let handle = start_core(&cwd_str);
    let handles = handle.rebuild_tab(&tab);
    assert_eq!(
        handles.len(),
        TWO_PANES as usize,
        "rebuilding the restored two-pane tab must return one window handle per pane; got \
         {handles:?}. 0 means the core was gone, the tab had no root or the rendered document \
         did not decode, 1 means the renderer dropped a pane"
    );
    assert!(
        handles.iter().all(|&h| h != 0),
        "every rebuilt window handle must be a real (nonzero) window; got {handles:?}"
    );
    let unique: HashSet<i64> = handles.iter().copied().collect();
    assert_eq!(
        unique.len(),
        handles.len(),
        "rebuilt window handles must be distinct windows; got {handles:?}"
    );
    poll_win_basename(&handle, handles[0], 0, LEFT_NAME);
    poll_win_basename(&handle, handles[1], 1, RIGHT_NAME);
    handle.shutdown();

    // --- 2. A terminal pane rebuilds with a session bound into its window only. ------------
    common::pin_default_shell();
    let term_dir = cwd.path().join(TERMINAL_DIR_NAME);
    std::fs::create_dir(&term_dir).expect("failed to create the terminal pane's directory");
    let tab = commit_and_restore(
        &cwd_str,
        [
            (EDIT, left_str),
            (TERMINAL, term_dir.to_string_lossy().into_owned()),
        ],
    );
    let handle = start_core(&cwd_str);
    let handles = handle.rebuild_tab(&tab);
    assert_eq!(
        handles.len(),
        TWO_PANES as usize,
        "rebuilding the edit + terminal tab must return one handle per pane; got {handles:?}"
    );
    poll(
        &format!(
            "the terminal pane's window {} to carry a session; 0 means the pane was not saved or \
             restored as a terminal, or core starts no session for a rebuilt terminal pane",
            handles[1]
        ),
        || handle.win_buffer(handles[1]).terminal() != 0,
    );
    poll_win_basename(&handle, handles[0], 0, LEFT_NAME);
    assert_eq!(
        handle.win_buffer(handles[0]).terminal(),
        0,
        "the edit pane's window must carry no session"
    );

    handle.shutdown();
    std::env::remove_var("WOLF_STATE_DIR");
    drop(state_dir);
}
