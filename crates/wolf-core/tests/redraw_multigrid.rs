//! Integration tests for the multigrid foundation: the snapshot holds
//! MULTIPLE grids keyed by grid id (`Snapshot.grids`), a per-grid window map
//! (`Snapshot.windows`), a cursor that names its grid (`Cursor.grid`), and
//! per-grid resize. These tests drive a REAL, hermetic nvim (no mocks) via
//! `WOLF_NVIM_BIN`; the common helpers panic loudly if it is unset.
//!
//! IMPORTANT testing conventions for this file:
//!   * We assert on grid/window MAP SIZES, MEMBERSHIP, and ACTIVE-grid behavior.
//!     We do NOT assert literal `WinInfo.win` handle values — the window-handle
//!     encoding is an implementation detail. We only assert ids are
//!     internally consistent (the same `win` appears in `windows` and
//!     `grid_for_win` round-trips).
//!   * We never assume HashMap iteration order.
//!   * Multigrid notifications (win_pos / grid creation / grid_destroy) may lag
//!     and arrive across several flushes, so where we expect a structural change
//!     we POLL with a generous timeout (mirroring `current_file.rs`), draining
//!     wakes as we go, rather than waiting for a single flush.

mod common;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use common::{ch, ctrl, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Key, Snapshot, WolfCore};

/// The outer/global grid id. Per the multigrid model grid 1 always exists (nvim
/// resizes it; cmdline/messages live there) but window content lives on grids
/// >= 2 and grid 1 is never the active grid in these scenarios.
const GLOBAL_GRID: u64 = 1;

/// Gap between `read_snapshot` polls while waiting for a structural change.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Distinct dimensions used by the per-grid resize test. Chosen well inside the
/// default 80x24 outer grid so nvim can honor them, and different from both the
/// global grid dims and a freshly-split window grid's dims.
const RESIZE_GRID_COLS: u16 = 50;
const RESIZE_GRID_ROWS: u16 = 15;

/// Polls `core`'s snapshot until `pred` holds, draining wakes each iteration, or
/// panics after [`FLUSH_TIMEOUT`]. `desc` describes the awaited condition for the
/// panic message. This mirrors `current_file.rs`'s poll-with-timeout pattern,
/// which is the right tool when the change we await may arrive across multiple
/// flushes (win_pos / grid creation / grid_destroy) rather than on one.
fn poll_until(core: &WolfCore, waiter: &FlushWaiter, desc: &str, pred: impl Fn(&Snapshot) -> bool) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        {
            let snap = core.read_snapshot();
            // The guard derefs to `Snapshot`, so `&snap` coerces to the
            // `&Snapshot` the predicate expects.
            if pred(&snap) {
                return;
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Types an Ex command `:<cmd><CR>` one key at a time. init.lua's `cnoreabbrev`
/// overrides route `:split`/`:close` through `wolf_do_split`/`wolf_do_close`,
/// which still run the REAL nvim window ops, so they create/destroy windows +
/// grids and emit win_pos/win_close.
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// Collects the set of grid ids that currently have an associated window (i.e.
/// the keys of `Snapshot.windows`). Order-independent by construction.
fn window_grid_ids(snap: &Snapshot) -> HashSet<u64> {
    snap.windows.keys().copied().collect()
}

#[test]
fn attach_creates_window_grid() {
    // Open a temp md so at least one editor window (and thus a window grid >= 2)
    // exists after attach.
    let (core, waiter, _file) = common::start_core_with_temp_md("# multigrid\nbody\n");

    // win_pos / window-grid creation may lag the initial flush, so poll until the
    // window grid is present rather than assuming a single flush carried it.
    poll_until(
        &core,
        &waiter,
        "a window grid (id != 1) and a non-empty windows map to appear",
        |snap| {
            snap.grids.contains_key(&GLOBAL_GRID)
                && snap.grids.keys().any(|&id| id != GLOBAL_GRID)
                && !snap.windows.is_empty()
        },
    );

    let snap = core.read_snapshot();

    // Grid 1 is always present.
    assert!(
        snap.grids.contains_key(&GLOBAL_GRID),
        "grids must always contain the global grid {GLOBAL_GRID}; got keys {:?}",
        snap.grids.keys().collect::<Vec<_>>()
    );
    // At least one window grid with id != 1.
    assert!(
        snap.grids.keys().any(|&id| id != GLOBAL_GRID),
        "grids must contain at least one window grid (id != {GLOBAL_GRID}); got keys {:?}",
        snap.grids.keys().collect::<Vec<_>>()
    );

    // The active grid is the cursor's window grid, NOT the global grid.
    assert_ne!(
        snap.active_grid_id(),
        GLOBAL_GRID,
        "active_grid_id() should be the cursor's window grid, not the global grid"
    );

    // The cursor names an existing grid.
    assert!(
        snap.grids.contains_key(&snap.cursor.grid),
        "cursor.grid ({}) must name an existing grid; got keys {:?}",
        snap.cursor.grid,
        snap.grids.keys().collect::<Vec<_>>()
    );

    // The windows map is non-empty...
    assert!(
        !snap.windows.is_empty(),
        "windows map should be non-empty after attach"
    );

    // ...and the active grid's content is paintable (real dimensions).
    let active = snap.active_grid();
    assert!(
        active.cols > 0 && active.rows > 0,
        "active grid should have paintable dimensions; got {}x{}",
        active.cols,
        active.rows
    );

    // Each window entry is keyed by its own grid id and grid_for_win round-trips to it; literal
    // win-handle values are an implementation detail and never asserted.
    for (&grid_id, win) in &snap.windows {
        assert_eq!(
            win.grid, grid_id,
            "windows is keyed by grid id, so WinInfo.grid must equal its map key"
        );
        assert_eq!(
            snap.grid_for_win(win.win),
            Some(grid_id),
            "grid_for_win({}) should round-trip back to grid {grid_id}",
            win.win
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn split_adds_grid_and_window() {
    let (core, waiter, _file) = common::start_core_with_temp_md("# split\nbody\n");

    // Wait for the first window grid before measuring the baseline.
    poll_until(
        &core,
        &waiter,
        "the initial window grid to appear before splitting",
        |snap| !snap.windows.is_empty() && snap.grids.keys().any(|&id| id != GLOBAL_GRID),
    );

    let (base_grids, base_windows) = {
        let snap = core.read_snapshot();
        (snap.grids.len(), snap.windows.len())
    };

    // `:split<CR>` -> nvim makes a new window + grid.
    ex_command(&core, "split");

    poll_until(
        &core,
        &waiter,
        "grids.len() and windows.len() to each increase by one after :split",
        |snap| snap.grids.len() == base_grids + 1 && snap.windows.len() == base_windows + 1,
    );

    let snap = core.read_snapshot();
    // Two distinct window grids are now present.
    let win_grids = window_grid_ids(&snap);
    assert_eq!(
        win_grids.len(),
        base_windows + 1,
        "there should be two window grids after a split; got {win_grids:?}"
    );
    // Every window grid id is also a key in `grids`.
    for &g in &win_grids {
        assert!(
            snap.grids.contains_key(&g),
            "window grid {g} must also be present in grids ({:?})",
            snap.grids.keys().collect::<Vec<_>>()
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn grid_line_lands_in_active_grid() {
    // Empty buffer so the typed text is the only content on the active window grid.
    let (core, waiter, _file) = common::start_core_with_temp_md("");

    poll_until(
        &core,
        &waiter,
        "the initial window grid to appear before typing",
        |snap| !snap.windows.is_empty() && snap.active_grid_id() != GLOBAL_GRID,
    );

    // Enter insert mode, type, leave insert mode.
    core.send_input(ch('i'));
    type_str(&core, "hello");
    core.send_input(key(Key::Esc));

    poll_until(
        &core,
        &waiter,
        "the typed text to land on the active grid's row 0",
        |snap| snap.active_grid().row_text(0).contains("hello"),
    );

    let snap = core.read_snapshot();
    assert!(
        snap.active_grid().row_text(0).contains("hello"),
        "active grid row 0 should contain the typed text; got {:?}",
        snap.active_grid().row_text(0)
    );

    // Cheap cross-check: the global grid (never carrying window content) does not
    // hold the typed text. Grid 1 always exists in the model.
    if let Some(global) = snap.grid(GLOBAL_GRID) {
        assert!(
            !global.row_text(0).contains("hello"),
            "the global grid should not carry window content; got {:?}",
            global.row_text(0)
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn close_removes_grid_and_window() {
    let (core, waiter, _file) = common::start_core_with_temp_md("# close\nbody\n");

    poll_until(
        &core,
        &waiter,
        "the initial window grid to appear before splitting",
        |snap| !snap.windows.is_empty() && snap.grids.keys().any(|&id| id != GLOBAL_GRID),
    );

    let base_windows = core.read_snapshot().windows.len();

    // Split so there are two windows to close one of.
    ex_command(&core, "split");
    poll_until(
        &core,
        &waiter,
        "windows.len() to reach two after :split",
        |snap| snap.windows.len() == base_windows + 1,
    );

    // Capture the two window grid ids before closing.
    let before = window_grid_ids(&core.read_snapshot());
    let before_grids = core.read_snapshot().grids.len();

    // Close the focused window -> nvim removes that window + grid (win_close /
    // grid_destroy).
    ex_command(&core, "close");
    poll_until(
        &core,
        &waiter,
        "grids and windows to each drop back by one after :close",
        |snap| snap.windows.len() == base_windows && snap.grids.len() == before_grids - 1,
    );

    let snap = core.read_snapshot();
    let after = window_grid_ids(&snap);

    // Exactly one window grid id disappeared, and it is no longer in `grids`.
    let removed: Vec<u64> = before.difference(&after).copied().collect();
    assert_eq!(
        removed.len(),
        1,
        "exactly one window grid should be removed by :close (before={before:?}, after={after:?})"
    );
    let gone = removed[0];
    assert!(
        !snap.grids.contains_key(&gone),
        "the closed window's grid {gone} should be absent from grids ({:?})",
        snap.grids.keys().collect::<Vec<_>>()
    );
    assert!(
        !snap.windows.contains_key(&gone),
        "the closed window's grid {gone} should be absent from windows"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn active_grid_follows_focus() {
    let (core, waiter, _file) = common::start_core_with_temp_md("# focus\nbody\n");

    poll_until(
        &core,
        &waiter,
        "the initial window grid to appear before splitting",
        |snap| !snap.windows.is_empty() && snap.active_grid_id() != GLOBAL_GRID,
    );

    let base_windows = core.read_snapshot().windows.len();

    // Split so there are two windows to move focus between.
    ex_command(&core, "split");
    poll_until(
        &core,
        &waiter,
        "a second window to appear after :split",
        |snap| snap.windows.len() == base_windows + 1,
    );

    // Capture the focused grid (grid A) BEFORE switching focus.
    let grid_a = core.read_snapshot().cursor.grid;

    // <C-w>w cycles focus to the next window.
    core.send_input(ctrl(Key::Char('w')));
    core.send_input(ch('w'));

    poll_until(
        &core,
        &waiter,
        "cursor.grid to move to the other window grid",
        |snap| snap.cursor.grid != grid_a,
    );

    let snap = core.read_snapshot();
    let grid_b = snap.cursor.grid;
    assert_ne!(
        grid_b, grid_a,
        "focus switch should change the cursor's grid"
    );
    assert_ne!(
        grid_b, GLOBAL_GRID,
        "the newly focused grid should be a window grid, not the global grid"
    );
    // active_grid_id() follows the cursor's grid.
    assert_eq!(
        snap.active_grid_id(),
        grid_b,
        "active_grid_id() should follow the cursor to the newly focused grid"
    );
    // The active grid is an existing, paintable grid.
    assert!(
        snap.grids.contains_key(&grid_b),
        "the focused grid {grid_b} must exist in grids"
    );
    let active = snap.active_grid();
    assert!(
        active.cols > 0 && active.rows > 0,
        "the active grid after focus should be paintable; got {}x{}",
        active.cols,
        active.rows
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn resize_grid_changes_only_that_grid() {
    let (core, waiter, _file) = common::start_core_with_temp_md("# resize\nbody\n");

    poll_until(
        &core,
        &waiter,
        "the active window grid to appear before resizing",
        |snap| snap.active_grid_id() != GLOBAL_GRID && snap.grids.contains_key(&GLOBAL_GRID),
    );

    // Record the global grid's dims (must stay unchanged) and the active window
    // grid id (the resize target).
    let (target, global_before_cols, global_before_rows) = {
        let snap = core.read_snapshot();
        let global = snap.grid(GLOBAL_GRID).expect("global grid must exist");
        (snap.active_grid_id(), global.cols, global.rows)
    };

    // Per-grid resize through the ordered input pump (no race with redraw).
    core.resize_grid(target, RESIZE_GRID_COLS, RESIZE_GRID_ROWS);

    // Polls for the exact dims, 50x15 inside the 80x24 outer grid. If nvim clamps them, see
    // crates/wolf-core/tests/README.md#redraw_multigridrs
    poll_until(
        &core,
        &waiter,
        "the target window grid to reach the requested dimensions",
        |snap| {
            snap.grid(target).is_some_and(|g| {
                g.cols == RESIZE_GRID_COLS as usize && g.rows == RESIZE_GRID_ROWS as usize
            })
        },
    );

    let snap = core.read_snapshot();
    let resized = snap.grid(target).expect("target grid must still exist");
    assert_eq!(
        resized.cols, RESIZE_GRID_COLS as usize,
        "target grid cols should match the requested per-grid resize width"
    );
    assert_eq!(
        resized.rows, RESIZE_GRID_ROWS as usize,
        "target grid rows should match the requested per-grid resize height"
    );

    // The global grid is unaffected by a per-grid resize of a window grid.
    let global = snap
        .grid(GLOBAL_GRID)
        .expect("global grid must still exist");
    assert_eq!(
        global.cols, global_before_cols,
        "global grid cols must be unchanged by a per-grid resize of grid {target}"
    );
    assert_eq!(
        global.rows, global_before_rows,
        "global grid rows must be unchanged by a per-grid resize of grid {target}"
    );

    core.shutdown().expect("shutdown should be Ok");
}
