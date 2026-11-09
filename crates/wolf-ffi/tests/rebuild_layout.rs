//! Real-nvim integration test for `WolfCoreHandle::rebuild_layout(&FfiPaneNode)`:
//! a saved split tree, read back from `state.json` as the `FfiPaneNode` the macOS
//! restore holds, is rendered in Rust by `wolf_core::state::rebuild_layout_spec`
//! and realized in the current tabpage's lone window. The
//! `//crates/wolf-ffi:rebuild_layout` `rust_test` target runs this file under Bazel.
//!
//! ## What this asserts
//!
//! A vertical two-leaf split pushed through the snapshot push-builder loads back as
//! a node whose rebuild returns two distinct, nonzero window handles in leaf order:
//! `handles[0]` shows the left leaf's file and `handles[1]` the right leaf's. A
//! renderer that drops a leaf fails the count, and one that reverses the children
//! fails the per-handle paths.
//!
//! A second saved split, of an editor leaf and a TERMINAL leaf, loads back and is
//! rebuilt on a fresh core: the terminal leaf's handle carries a core session
//! (`win_buffer(h).terminal() != 0`), and the editor leaf's carries none.
//!
//! ## SINGLE #[test]
//!
//! Like `state.rs`, the whole flow lives in ONE `#[test]`: it sets the process-global
//! `WOLF_STATE_DIR` and opens the process-wide `SHARED_STORE`, so a second test in
//! this binary would race both.
//!
//! ## SKIP-when-unset policy
//!
//! Loudly SKIPS (no-op pass) when no pinned nvim is available, via
//! `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::collections::HashSet;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{
    wolf_ffi_load_state, wolf_ffi_start, wolf_ffi_state_flush, wolf_ffi_state_layout_commit_to_tab,
    wolf_ffi_state_layout_push_leaf, wolf_ffi_state_layout_push_split,
    wolf_ffi_state_snapshot_add_tab, wolf_ffi_state_snapshot_add_window,
    wolf_ffi_state_snapshot_begin, wolf_ffi_state_snapshot_commit, WolfCoreHandle,
};

/// Generous overall timeout for the state write, nvim startup and each rebuilt
/// window's buffer to settle. Mirrors the sibling tests.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll cadence while waiting for an effect to surface.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The lowest real WINDOW grid id (grid 1 is the outer global grid).
const WINDOW_GRID_MIN: u64 = 2;
/// Live editor windows at startup, before the rebuild.
const ONE_WINDOW: usize = 1;

/// The FFI orientation discriminant for a VERTICAL split (0 = horizontal, 1 = vertical).
const ORIENT_VERTICAL: u8 = 1;
/// The number of leaves in the pushed split, and so of rebuilt windows.
const TWO_LEAVES: u32 = 2;
/// Each child's weight in the pushed 0.5/0.5 split.
const HALF_WEIGHT: f64 = 0.5;
/// The root node's weight (always 1.0 in the token stream).
const ROOT_WEIGHT: f64 = 1.0;

/// The two leaves' basenames, in leaf order. Neither is the startup buffer's name.
const LEFT_NAME: &str = "left.md";
const RIGHT_NAME: &str = "right.md";

/// The directory, under the core's cwd, the saved terminal leaf carries.
const TERMINAL_DIR_NAME: &str = "term-dir";

/// The saved geometry of the one window the builder submits (load never validates it).
const WIN_X: f64 = 100.0;
const WIN_Y: f64 = 200.0;
const WIN_WIDTH: f64 = 800.0;
const WIN_HEIGHT: f64 = 600.0;

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

/// Polls `pred` until it returns `true` or [`FLUSH_TIMEOUT`] elapses; panics with
/// `desc` on timeout.
fn poll(desc: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        if pred() {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
}

/// The basename of the buffer window `win` shows, or `None` while the window has no
/// grid yet or shows an unnamed buffer.
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

/// Polls until window `win` shows a buffer named `expected`, panicking with the last
/// observed basename on timeout. Matches on the basename, because nvim reports the
/// temp dir under `/private/var` where the test created it under `/var`.
fn poll_win_basename(handle: &WolfCoreHandle, win: i64, leaf: usize, expected: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
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

#[test]
fn rebuild_layout_realizes_a_loaded_two_leaf_split() {
    let Some(_) = common::nvim_bin_or_skip() else {
        return;
    };

    // A private, empty state dir, so the poll below can only see this test's commit. Safe to
    // set because this binary has exactly one test, and before the shared store first opens.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    // The core's cwd, holding the two leaves' files. It must outlive the session.
    let cwd = tempfile::tempdir().expect("failed to create temp cwd directory");
    let left = cwd.path().join(LEFT_NAME);
    let right = cwd.path().join(RIGHT_NAME);
    std::fs::write(&left, "# left\n").expect("failed to write the left leaf's file");
    std::fs::write(&right, "# right\n").expect("failed to write the right leaf's file");
    let cwd_str = cwd.path().to_string_lossy().into_owned();

    // --- 1. Save a vertical two-leaf split on the only tab, pushed post-order. -----------
    wolf_ffi_state_snapshot_begin();
    wolf_ffi_state_snapshot_add_window(WIN_X, WIN_Y, WIN_WIDTH, WIN_HEIGHT);
    wolf_ffi_state_snapshot_add_tab(cwd_str.clone(), false);
    wolf_ffi_state_layout_push_leaf(
        left.to_string_lossy().into_owned(),
        false,
        false,
        HALF_WEIGHT,
    );
    wolf_ffi_state_layout_push_leaf(
        right.to_string_lossy().into_owned(),
        false,
        false,
        HALF_WEIGHT,
    );
    wolf_ffi_state_layout_push_split(ORIENT_VERTICAL, TWO_LEAVES, ROOT_WEIGHT);
    wolf_ffi_state_layout_commit_to_tab();
    wolf_ffi_state_snapshot_commit();

    // --- 2. Read it back the way the launch restore does. ---------------------------------
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let restored = loop {
        assert!(
            wolf_ffi_state_flush(),
            "wolf_ffi_state_flush() must complete within {FLUSH_TIMEOUT:?}"
        );
        let restored = wolf_ffi_load_state();
        let ready = restored
            .windows()
            .first()
            .is_some_and(|w| w.tabs().first().is_some_and(|t| t.has_layout()));
        if ready || Instant::now() >= deadline {
            break restored;
        }
        sleep(POLL_INTERVAL);
    };
    let windows = restored.windows();
    let tabs = windows
        .first()
        .expect("the committed snapshot must load back with its one window")
        .tabs();
    let tab = tabs
        .first()
        .expect("the loaded window must carry its one tab");
    assert!(
        tab.has_layout(),
        "the loaded tab must report has_layout() == true after the layout commit"
    );
    let node = tab.layout();
    // Premise, not the behavior: the node handed to the rebuild is the two-leaf split.
    assert_eq!(
        node.child_count(),
        TWO_LEAVES,
        "the loaded layout must be the two-leaf split that was saved; a 0 means it loaded as a \
         leaf, so the rebuild below would test nothing"
    );

    // --- 3. Start a core and wait for its lone starting window. ---------------------------
    let init_lua = common::init_lua_path();
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd_str,
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

    // --- 4. Rebuild from the loaded node. -------------------------------------------------
    let handles = handle.rebuild_layout(&node);
    assert_eq!(
        handles.len(),
        TWO_LEAVES as usize,
        "rebuilding the loaded two-leaf split must return one window handle per leaf; got \
         {handles:?}. 0 means the core was gone or the rendered document did not decode, 1 \
         means the renderer dropped a leaf"
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

    // --- 5. Each handle shows its own leaf's file, in leaf order. -------------------------
    poll_win_basename(&handle, handles[0], 0, LEFT_NAME);
    poll_win_basename(&handle, handles[1], 1, RIGHT_NAME);

    handle.shutdown();

    // --- 6. A saved terminal leaf rebuilds with a session bound into its window. ---------
    common::pin_default_shell();
    let term_dir = cwd.path().join(TERMINAL_DIR_NAME);
    std::fs::create_dir(&term_dir).expect("failed to create the terminal leaf's directory");
    wolf_ffi_state_snapshot_begin();
    wolf_ffi_state_snapshot_add_window(WIN_X, WIN_Y, WIN_WIDTH, WIN_HEIGHT);
    wolf_ffi_state_snapshot_add_tab(cwd.path().to_string_lossy().into_owned(), false);
    wolf_ffi_state_layout_push_leaf(
        left.to_string_lossy().into_owned(),
        false,
        false,
        HALF_WEIGHT,
    );
    wolf_ffi_state_layout_push_leaf(
        term_dir.to_string_lossy().into_owned(),
        false,
        true,
        HALF_WEIGHT,
    );
    wolf_ffi_state_layout_push_split(ORIENT_VERTICAL, TWO_LEAVES, ROOT_WEIGHT);
    wolf_ffi_state_layout_commit_to_tab();
    wolf_ffi_state_snapshot_commit();

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let restored = loop {
        assert!(
            wolf_ffi_state_flush(),
            "wolf_ffi_state_flush() must complete within {FLUSH_TIMEOUT:?}"
        );
        let restored = wolf_ffi_load_state();
        let ready = restored.windows().first().is_some_and(|w| {
            w.tabs().first().is_some_and(|t| {
                t.has_layout()
                    && t.layout().child_count() == TWO_LEAVES
                    && t.layout().child(1).is_terminal()
            })
        });
        if ready || Instant::now() >= deadline {
            break restored;
        }
        sleep(POLL_INTERVAL);
    };
    let windows = restored.windows();
    let node = windows
        .first()
        .expect("the committed terminal snapshot must load back with its one window")
        .tabs()
        .first()
        .expect("the loaded window must carry its one tab")
        .layout();
    assert!(
        node.child_count() == TWO_LEAVES && node.child(1).is_terminal(),
        "premise: the loaded layout must be the editor + terminal split that was saved, its \
         second leaf terminal; otherwise the rebuild below tests nothing"
    );

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a second handle");
    poll("the second core to attach (read_frame cols > 0)", || {
        handle.read_frame().cols() > 0
    });
    poll("a single live editor window at the second startup", || {
        live_windows(&handle).len() == ONE_WINDOW
    });
    let handles = handle.rebuild_layout(&node);
    assert_eq!(
        handles.len(),
        TWO_LEAVES as usize,
        "rebuilding the editor + terminal split must return one handle per leaf; got {handles:?}"
    );
    poll(
        &format!(
            "the terminal leaf's window {} to carry a session; 0 means core starts no session \
             for a rebuilt terminal leaf",
            handles[1]
        ),
        || handle.win_buffer(handles[1]).terminal() != 0,
    );
    poll_win_basename(&handle, handles[0], 0, LEFT_NAME);
    assert_eq!(
        handle.win_buffer(handles[0]).terminal(),
        0,
        "the editor leaf's window must carry no session."
    );

    handle.shutdown();
    std::env::remove_var("WOLF_STATE_DIR");
    drop(state_dir);
}
