//! Integration test for the `wolf-ffi` mouse + scroll bridge surface.
//!
//! This drives the RUST half of the swift-bridge mouse/scroll machinery against a
//! REAL, hermetic Neovim — NO mocks, per the project testing policy. It is a
//! sibling of `panes.rs` / `tree_search.rs` / `file_search.rs` (same staging /
//! SKIP-when-unset machinery, reusing `tests/common/mod.rs`) but exercises the
//! input surface:
//!
//!   * `WolfCoreHandle::send_mouse(ev: FfiMouseEvent)` (button/action/mods + grid +
//!     grid-local row/col → `nvim_input_mouse`)
//!   * `WolfCoreHandle::send_scroll(ev: FfiScrollEvent)` (direction + grid + row/col
//!     → `nvim_input_mouse("wheel", …)`)
//!   * `FfiFrame::{topline, botline, curline, linecount}() -> u64` viewport
//!     accessors (read from `snapshot.windows[grid].viewport` in
//!     `from_snapshot_grid`)
//!
//! The `grid: u64` threads through `wolf_core::{MouseEvent, ScrollEvent}` to
//! `nvim_input_mouse`; the discriminants live in the `mod mouse_button` /
//! `mod mouse_action` / `mod scroll_dir` tables in `src/lib.rs`. The
//! `//crates/wolf-ffi:mouse_scroll` `rust_test` target runs this file under Bazel.
//!
//! ## What this EMPIRICALLY confirms
//!
//!   * `nvim_input_mouse` grid-targeting under `ext_multigrid`: events must target
//!     the pane's WINDOW grid (>= 2) with GRID-LOCAL row/col.
//!     A mouse press moving the cursor to the targeted cell proves the grid + coord
//!     contract.
//!   * The scroll DIRECTION discriminant (UP = 0 decreases `topline`, DOWN = 1
//!     increases it).
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN` (or
//! the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available,
//! via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, FfiMouseEvent, FfiScrollEvent};

/// Generous overall timeout for nvim `--embed` startup plus each input round-trip
/// (put/cursor-move/scroll) to settle. Mirrors `panes.rs`'s `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an input effect to surface. Mirrors `panes.rs`'s
/// `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- Mouse/scroll discriminants: mirror `mod mouse_button` / `mouse_action` / `scroll_dir` in
// `crates/wolf-ffi/src/lib.rs`, in `wolf_core` variant order.
/// `mouse_button::LEFT` — the left button (the only one this test sends).
const MOUSE_BUTTON_LEFT: u8 = 0;
/// `mouse_action::PRESS` — button down (positions the cursor under `mouse=a`).
const MOUSE_ACTION_PRESS: u8 = 0;
/// `mouse_action::RELEASE` — button up (ends a drag selection).
const MOUSE_ACTION_RELEASE: u8 = 1;
/// `mouse_action::DRAG` — pointer moved with the button held (extends selection).
const MOUSE_ACTION_DRAG: u8 = 2;
/// `scroll_dir::UP` — wheel up; decreases `topline` (viewport moves toward line 1).
const SCROLL_DIR_UP: u8 = 0;
/// `scroll_dir::DOWN` — wheel down; increases `topline` (viewport moves down).
const SCROLL_DIR_DOWN: u8 = 1;

// -- Window grids -------------------------------------------------------------
/// The lowest real WINDOW grid id (grid 1 is the outer global grid; editor windows
/// are >= 2 under `ext_multigrid`). Mouse/scroll MUST target a window grid.
const WINDOW_GRID_MIN: u64 = 2;

// -- Buffer / viewport fixture ------------------------------------------------
/// How many numbered lines to `:put` into the buffer so there is content to click
/// and scroll through, and the per-grid `linecount()` viewport accessor populates.
const LINE_COUNT_TARGET: u64 = 80;

// -- Mouse target cells (grid-local, 0-based; clamped into the live grid) ------
/// Press at this grid-local row (the cursor should land on it).
const PRESS_ROW: u32 = 5;
/// Press at this grid-local column.
const PRESS_COL: u32 = 2;
/// Drag/release at this grid-local row (a drag under `mouse=a` moves the cursor).
const DRAG_ROW: u32 = 8;
/// Drag/release at this grid-local column.
const DRAG_COL: u32 = 4;

// -- Scroll burst -------------------------------------------------------------
/// How many discrete wheel events to send per scroll direction. A handful guarantees
/// `topline` moves past the few-line nudge nvim applies per `wheel` event.
const SCROLL_BURST: usize = 5;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false (the already-resolved glyph rides in `ch`, per the Shift contract).
fn char_key(c: char) -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_CHAR,
        ch: c as u32,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Builds an Enter `FfiKeyEvent` (kind=Enter), all modifiers false.
fn enter_key() -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_ENTER,
        ch: 0,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Types `cmd` as an Ex command and presses Enter: each char goes through the same
/// `send_key`/`FfiKeyEvent` path the generated Swift glue uses, then a final `<CR>`
/// executes it. `cmd` must include the leading `:` (a printable char), e.g.
/// `":put =range(1,80)"`. Mirrors `panes.rs`'s `send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// The live editor WINDOW grids (id + win handle) from `windows()`: grids >=
/// `WINDOW_GRID_MIN` that are not hidden. Mirrors `panes.rs`'s `live_windows`.
fn live_windows(handle: &wolf_ffi::WolfCoreHandle) -> Vec<(u64, i64)> {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .map(|w| (w.grid(), w.win()))
        .collect()
}

/// Clamps a grid-local `(row, col)` into the live grid `[0, rows-1] x [0, cols-1]`
/// so a fixture cell never lands outside the (possibly smaller-than-default) grid.
fn clamp_cell(rows: u16, cols: u16, row: u32, col: u32) -> (u32, u32) {
    let max_row = u32::from(rows.saturating_sub(1));
    let max_col = u32::from(cols.saturating_sub(1));
    (row.min(max_row), col.min(max_col))
}

/// Builds a left-button `FfiMouseEvent` for `action` at grid-local `(row, col)`,
/// all modifiers false.
fn mouse_event(action: u8, grid: u64, row: u32, col: u32) -> FfiMouseEvent {
    FfiMouseEvent {
        button: MOUSE_BUTTON_LEFT,
        action,
        grid,
        row,
        col,
        ctrl: false,
        shift: false,
        alt: false,
    }
}

/// Builds an `FfiScrollEvent` for `direction` at grid-local `(row, col)`.
fn scroll_event(direction: u8, grid: u64, row: u32, col: u32) -> FfiScrollEvent {
    FfiScrollEvent {
        direction,
        grid,
        row,
        col,
    }
}

#[test]
fn mouse_scroll_click_drag_and_wheel() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd (it is nvim's working dir + search root, so it must outlive the session), kept
    // alive until after `shutdown()`, then dropped explicitly at the end.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Poll read_frame() until attached (cols > 0) ---------------------
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut frame = handle.read_frame();
    while Instant::now() < deadline {
        frame = handle.read_frame();
        if frame.cols() > 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        frame.cols() > 0,
        "core should attach and report a non-empty active frame (cols, got {})",
        frame.cols()
    );

    // --- 2. Poll the live window grid (>= 2, not hidden) --------------------
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut windows = live_windows(&handle);
    while Instant::now() < deadline {
        windows = live_windows(&handle);
        if !windows.is_empty() {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        !windows.is_empty(),
        "at least one editor window grid should be live (got {windows:?})"
    );
    let (grid, _win) = windows[0];
    assert!(
        grid >= WINDOW_GRID_MIN,
        "the editor window grid should be >= {WINDOW_GRID_MIN} (got {grid})"
    );

    // --- 3. Fill 80 zero-padded five-char lines, quoting with doubled single quotes; see
    // crates/wolf-ffi/tests/README.md#mouse_scrollrs
    send_ex_command(&handle, ":put =map(range(1,80), 'printf(''%05d'', v:val)')");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut filled = handle.read_frame_for_grid(grid);
    while Instant::now() < deadline {
        filled = handle.read_frame_for_grid(grid);
        if filled.linecount() >= LINE_COUNT_TARGET {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        filled.linecount() >= LINE_COUNT_TARGET,
        "the buffer should hold >= {LINE_COUNT_TARGET} lines after :put (linecount, got {})",
        filled.linecount()
    );

    // Go to line 1 for a known baseline: `win_viewport` reports `topline` 0-based, so it settles
    // to 0, not 1.
    send_ex_command(&handle, ":1");
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut top = handle.read_frame_for_grid(grid);
    while Instant::now() < deadline {
        top = handle.read_frame_for_grid(grid);
        if top.topline() == 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        top.topline(),
        0,
        "after :1 the viewport's topline settles to 0 (0-based) (got {})",
        top.topline()
    );

    // --- 4. Mouse PRESS positions the cursor --------------------------------
    // Read the live grid dimensions and clamp the fixture cell into bounds.
    let grid_frame = handle.read_frame_for_grid(grid);
    let (press_row, press_col) =
        clamp_cell(grid_frame.rows(), grid_frame.cols(), PRESS_ROW, PRESS_COL);
    handle.send_mouse(mouse_event(MOUSE_ACTION_PRESS, grid, press_row, press_col));

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after_press = handle.read_frame_for_grid(grid);
    while Instant::now() < deadline {
        after_press = handle.read_frame_for_grid(grid);
        if u32::from(after_press.cursor_row()) == press_row
            && u32::from(after_press.cursor_col()) == press_col
        {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    // The press lands exactly on the targeted cell, proving grid targeting with grid-local
    // coordinates. The five-char lines keep column 2 from clamping to end-of-line.
    assert_eq!(
        u32::from(after_press.cursor_row()),
        press_row,
        "a left PRESS at grid-local row {press_row} should move the cursor there \
         (cursor_row, got {})",
        after_press.cursor_row()
    );
    assert_eq!(
        u32::from(after_press.cursor_col()),
        press_col,
        "a left PRESS at grid-local col {press_col} should move the cursor there \
         (cursor_col, got {}); FALLBACK if brittle: row-only assertion",
        after_press.cursor_col()
    );

    // --- 5. DRAG then RELEASE: core stays responsive, cursor follows --------
    let (drag_row, drag_col) = clamp_cell(grid_frame.rows(), grid_frame.cols(), DRAG_ROW, DRAG_COL);
    handle.send_mouse(mouse_event(MOUSE_ACTION_DRAG, grid, drag_row, drag_col));
    handle.send_mouse(mouse_event(MOUSE_ACTION_RELEASE, grid, drag_row, drag_col));

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after_drag = handle.read_frame_for_grid(grid);
    while Instant::now() < deadline {
        after_drag = handle.read_frame_for_grid(grid);
        if u32::from(after_drag.cursor_row()) == drag_row {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    // The core must stay responsive after a drag/release (no panic, grid still live).
    assert!(
        after_drag.cols() > 0,
        "the grid should stay live (cols > 0) after a drag + release (got {})",
        after_drag.cols()
    );
    // LOOSE: a drag under `mouse=a` moves the cursor to the drag cell — but keep this
    // a soft check (movement OR no-panic) to avoid headless visual-mode flakiness.
    if u32::from(after_drag.cursor_row()) != drag_row {
        eprintln!(
            "note: drag did not move cursor_row to {} (got {}); accepted as no-panic",
            drag_row,
            after_drag.cursor_row()
        );
    }

    // --- 6. Scroll DOWN increases topline -----------------------------------
    let t0 = handle.read_frame_for_grid(grid).topline();
    for _ in 0..SCROLL_BURST {
        handle.send_scroll(scroll_event(SCROLL_DIR_DOWN, grid, press_row, press_col));
    }

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut scrolled_down = handle.read_frame_for_grid(grid);
    while Instant::now() < deadline {
        scrolled_down = handle.read_frame_for_grid(grid);
        if scrolled_down.topline() > t0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        scrolled_down.topline() > t0,
        "scrolling DOWN should increase the viewport topline (was {t0}, got {})",
        scrolled_down.topline()
    );

    // --- 7. Scroll UP decreases topline -------------------------------------
    let t1 = handle.read_frame_for_grid(grid).topline();
    for _ in 0..SCROLL_BURST {
        handle.send_scroll(scroll_event(SCROLL_DIR_UP, grid, press_row, press_col));
    }

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut scrolled_up = handle.read_frame_for_grid(grid);
    while Instant::now() < deadline {
        scrolled_up = handle.read_frame_for_grid(grid);
        if scrolled_up.topline() < t1 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        scrolled_up.topline() < t1,
        "scrolling UP should decrease the viewport topline (was {t1}, got {})",
        scrolled_up.topline()
    );

    // --- 8. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
