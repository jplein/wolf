//! End-to-end: the pure `wolf-gnome` mouse/scroll seams -> a real `WolfCore` ->
//! a real, hermetic Neovim -> the snapshot. No mocks (per project policy).
//!
//! What the GTK glue would do inside a `gtk::GestureDrag` / `EventControllerScroll`
//! callback, this test does by hand: translate a pane-local pixel point into a
//! grid cell with [`wolf_gnome::cell_at`], encode the button + modifiers with
//! `wolf_gnome::mouse::mouse_input`, accumulate a scroll delta with
//! `wolf_gnome::mouse::{scroll_threshold, ScrollAccumulator, scroll_steps}`, and
//! push the results through `WolfCore::send_mouse` / `send_scroll`.
//!
//! ## Why this file exists
//!
//! **NOTHING IN THIS REPOSITORY CAN INJECT A POINTER EVENT.** The `cage` smokes
//! are keyboard-only — the whole smoke toolchain is `cage`, `wtype`, `grim`,
//! `tesseract` (`smoke_headless.rs`'s own `REQUIRED_TOOLS`), `wtype` is a
//! keyboard injector, and there is no pointer injector in `flake.nix` at all.
//!
//! `wlrctl` was TRIED and is proven not to work, which is why it is deliberately
//! absent from the toolchain rather than merely unused: `cage` advertises
//! `WL_SEAT_CAPABILITY_POINTER` only while a pointer device exists, and each
//! `wlrctl` invocation is a separate short-lived process that creates and
//! destroys its `zwlr_virtual_pointer_v1` inside a single dispatch batch. The GTK
//! client's own `WAYLAND_DEBUG=1` log shows `wl_seat.capabilities(3)` ->
//! `get_pointer` -> `capabilities(2)` -> `wl_pointer.release()` **59 us apart**,
//! so the client never holds a live `wl_pointer` when the events fire and wlroots
//! drops them. The consequence is NOT "drag is missing while click and scroll are
//! covered": **no pointer event of any kind reaches the client, so click and
//! scroll have no compositor-level coverage either.** (`wlrctl`'s missing
//! press/release split would separately rule out drag, but it never gets that
//! far — and shipping it would advertise a capability the toolchain does not
//! have.)
//!
//! So this file is the automated coverage for the WHOLE path from `cell_at` /
//! `mouse_input` onward — press, drag including the clamp that makes
//! drag-past-the-edge selection work, release, and scroll in both directions —
//! driven against a real `WolfCore` and a real Neovim.
//!
//! What it deliberately does NOT prove, and nothing on this machine can: that any
//! of the per-pane GTK controllers ever fires at all; that `gtk::GestureDrag`
//! emits `drag-update` during a button-held move; that its `offset_x`/`offset_y`
//! carry the point outside the widget; or that it keeps delivering after the
//! pointer leaves the pane. `crates/wolf-gnome/tests/README.md`, section
//! "Deliberate test gaps", is the full statement of the gap — keep this doc
//! consistent with it.
//!
//! ## Test wiring (read this before hunting for a BUILD file)
//!
//! `cargo test` auto-discovers every `tests/*.rs`, and **`crates/wolf-gnome` has
//! no `BUILD.bazel` at all**, so there is NO Bazel `rust_test` target to add for
//! this file — unlike `crates/wolf-core` / `crates/wolf-ffi`, where CLAUDE.md's
//! "every new test file must be wired into a target" rule bites. Do not go
//! looking for one.
//!
//! ## Requirements
//!
//! A real, pinned Neovim located via `WOLF_NVIM_BIN` (exported by the flake
//! devShell). If it is unset the test PANICS rather than skipping — see
//! `common::require_nvim_bin`.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use gdk::ModifierType;

use wolf_core::{
    InputEvent, Key, KeyEvent, MouseAction, MouseEvent, ScrollDirection, ScrollEvent, WolfCore,
};
use wolf_gnome::cell_at;
use wolf_gnome::mouse::{mouse_input, scroll_steps, scroll_threshold, ScrollAccumulator};

use common::{config, wake_channel, FlushWaiter, FLUSH_TIMEOUT};

/// Grid width the core attaches with, in columns.
const COLS: u16 = 80;
/// Grid height the core attaches with, in rows.
const ROWS: u16 = 24;

/// The lowest real WINDOW grid id: grid 1 is the outer/global grid, and editor
/// windows draw into grids `>= 2` under `ext_multigrid`. Mouse and scroll events
/// MUST target a window grid with grid-local row/col.
const WINDOW_GRID_MIN: u64 = 2;

/// Poll cadence while waiting for an input effect to surface in the snapshot.
/// Polling (rather than one long fixed sleep) keeps the test fast when nvim is
/// quick and reliable when it is slow.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Brief settle window used where there is no state change to poll for (a mouse
/// RELEASE need not produce a redraw flush of its own). Mirrors
/// `tests/roundtrip_nvim.rs`.
const SETTLE: Duration = Duration::from_millis(200);

/// A synthetic monospace cell width, in pixels.
///
/// This test needs no real font and no GTK: `cell_at` is pure, so any positive
/// cell size exercises exactly the arithmetic the glue performs with the measured
/// `render::CellMetrics`.
const CELL_W: f64 = 8.0;
/// A synthetic monospace cell height, in pixels (see [`CELL_W`]).
const CELL_H: f64 = 16.0;

/// The centre of a cell, as a fraction of that cell — used so the press pixel is
/// unambiguously inside the intended cell rather than on a boundary.
const CELL_CENTRE: f64 = 0.5;

/// The grid-local cell the press targets. Mirrors
/// `crates/wolf-ffi/tests/mouse_scroll.rs`, whose assertions pinned this exact
/// (row, col) against real nvim.
const PRESS_ROW: u32 = 5;
/// The grid-local column the press targets (see [`PRESS_ROW`]).
const PRESS_COL: u32 = 2;

/// How far BEYOND the grid's right/bottom edge the drag point reaches, in cells.
/// `cell_at` must clamp it back to the last cell — that clamp is precisely what
/// makes drag-past-the-edge selection work.
const OVERSHOOT_CELLS: f64 = 40.0;

/// How many numbered lines to `:put` into the buffer, so there is content to
/// click into and to scroll through.
const LINE_COUNT_TARGET: u64 = 200;

/// The fraction of a cell fed to the accumulator twice, to prove the sub-cell
/// carry through the real type: neither nudge alone crosses a boundary, the two
/// together cross exactly one.
const SUB_CELL_FRACTION: f64 = 0.6;

/// How many whole cells the multi-step scroll delta is worth.
const SCROLL_CELLS: f64 = 5.0;

/// The WHEEL threshold, mirrored from `mouse::WHEEL_THRESHOLD` (which is private
/// to that module, so an integration test cannot name it). One detent is one line
/// step regardless of cell height.
const WHEEL_DETENT_THRESHOLD: f64 = 1.0;

/// Builds a printable-character key event. The already-resolved glyph rides in
/// `Key::Char` with no modifiers, per the shift-glyph contract.
fn char_key(c: char) -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: Key::Char(c),
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    })
}

/// Builds an Enter key event (executes a typed Ex command).
fn enter_key() -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: Key::Enter,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    })
}

/// Types `cmd` one character at a time and presses Enter. `cmd` must include the
/// leading `:` (itself just a printable char).
fn send_ex_command(core: &WolfCore, cmd: &str) {
    for c in cmd.chars() {
        core.send_input(char_key(c));
    }
    core.send_input(enter_key());
}

/// Polls `cond` until it is true or [`FLUSH_TIMEOUT`] elapses; returns whether it
/// was ever observed. Callers assert on the returned flag with a message that
/// re-reads the observed state, so a failure reports what actually happened.
fn poll_until(mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep(POLL_INTERVAL);
    }
}

/// The first live editor WINDOW grid (`>= 2`, not hidden), or `None`.
fn live_window_grid(core: &WolfCore) -> Option<u64> {
    let snap = core.read_snapshot();
    snap.windows
        .values()
        .find(|w| w.grid >= WINDOW_GRID_MIN && !w.hidden)
        .map(|w| w.grid)
}

/// The LIVE `(cols, rows)` of `grid`, converted exactly as the GTK glue converts
/// them (`u16::try_from(..).unwrap_or(u16::MAX)`, never a cast).
fn grid_dims(core: &WolfCore, grid: u64) -> Option<(u16, u16)> {
    let snap = core.read_snapshot();
    let g = snap.grid(grid)?;
    Some((
        u16::try_from(g.cols).unwrap_or(u16::MAX),
        u16::try_from(g.rows).unwrap_or(u16::MAX),
    ))
}

/// The cursor's grid-local `(row, col)`, but only when the cursor currently lives
/// on `grid` (under `ext_multigrid` `grid_cursor_goto` carries its own grid id).
fn cursor_cell(core: &WolfCore, grid: u64) -> Option<(u32, u32)> {
    let snap = core.read_snapshot();
    if snap.cursor.grid != grid {
        return None;
    }
    Some((
        u32::try_from(snap.cursor.row).ok()?,
        u32::try_from(snap.cursor.col).ok()?,
    ))
}

/// The lowercased name of the `ModeInfo` the cursor's `mode_idx` selects.
fn mode_name(core: &WolfCore) -> Option<String> {
    let snap = core.read_snapshot();
    snap.mode_infos
        .get(snap.cursor.mode_idx)
        .map(|m| m.name.to_lowercase())
}

/// The viewport topline of `grid` (0-based buffer line index).
fn topline(core: &WolfCore, grid: u64) -> Option<u64> {
    let snap = core.read_snapshot();
    snap.windows.get(&grid).map(|w| w.viewport.topline)
}

/// The buffer line count reported for `grid`.
fn linecount(core: &WolfCore, grid: u64) -> Option<u64> {
    let snap = core.read_snapshot();
    snap.windows.get(&grid).map(|w| w.viewport.linecount)
}

/// Starts a core against a real Neovim, waits for the attach flush, resolves the
/// live editor window grid, fills the buffer with wide numbered lines and parks
/// the view at line 1.
///
/// Returns the core, its flush waiter, and the window grid id.
fn start_with_filled_buffer() -> (WolfCore, FlushWaiter, u64) {
    // Fail loud if the real nvim binary is not configured — never skip.
    common::require_nvim_bin();

    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(config(COLS, ROWS), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_flush(FLUSH_TIMEOUT);

    let found_grid = poll_until(|| live_window_grid(&core).is_some());
    assert!(
        found_grid,
        "at least one editor window grid (>= {WINDOW_GRID_MIN}, not hidden) should become live"
    );
    let grid = live_window_grid(&core).expect("just polled for a live window grid");
    assert!(
        grid >= WINDOW_GRID_MIN,
        "mouse/scroll must target a WINDOW grid, never grid 0/1 (got {grid})"
    );

    // EMPIRICAL, copied verbatim in substance from
    // `crates/wolf-ffi/tests/mouse_scroll.rs`: `:put =range(1,200)` yields
    // 1-CHAR-WIDE lines ("1".."200"), so a click at column 2 clamps to
    // end-of-line (column 0) and the exact-column assertion below fails. Use
    // zero-padded 5-char lines instead, so column 2 (the 3rd character) is a REAL
    // position the cursor can land on.
    //
    // EMPIRICAL: a literal double-quote CANNOT appear inside `:put =<expr>`.
    // Under `:put =` Neovim treats a `"` as the start of an Ex comment and strips
    // it BEFORE evaluating the expression, so `'printf("%05d", v:val)'` fails with
    // `E115: Missing quote` and never fills the buffer. Quote the format with
    // DOUBLED SINGLE QUOTES instead (standard Vimscript single-quoted-string
    // escaping). Every character below is just a printable `Key::Char` — no shell
    // escaping and no Rust escaping, because there is no `"` anywhere in it.
    send_ex_command(&core, ":put =map(range(1,200), 'printf(''%05d'', v:val)')");

    let filled = poll_until(|| linecount(&core, grid).is_some_and(|n| n >= LINE_COUNT_TARGET));
    assert!(
        filled,
        "the buffer should hold >= {LINE_COUNT_TARGET} lines after :put (observed {:?})",
        linecount(&core, grid)
    );

    // Park the view at line 1 so `topline` has a known baseline. EMPIRICAL:
    // `win_viewport` reports `topline` 0-BASED, so after `:1` it settles to 0.
    send_ex_command(&core, ":1");
    let at_top = poll_until(|| topline(&core, grid) == Some(0));
    assert!(
        at_top,
        "after :1 the viewport topline should settle to 0 (0-based); observed {:?}",
        topline(&core, grid)
    );

    (core, waiter, grid)
}

/// A press positions the cursor at the cell the pure seam computed; a drag to a
/// point far OUTSIDE the grid clamps to the last cell and enters visual mode; the
/// release keeps the selection.
#[test]
fn mouse_press_drag_release_selects_via_seams() {
    let (core, waiter, grid) = start_with_filled_buffer();

    let (cols, rows) =
        grid_dims(&core, grid).expect("the live window grid should be in the snapshot");
    assert!(
        PRESS_ROW < u32::from(rows) && PRESS_COL < u32::from(cols),
        "the press cell ({PRESS_ROW}, {PRESS_COL}) must be INSIDE the live {cols}x{rows} grid, \
         or this test would only be asserting that cell_at clamps"
    );

    // --- 1. The pixel -> cell seam picks the cell we intended ----------------
    let press_x = (f64::from(PRESS_COL) + CELL_CENTRE) * CELL_W;
    let press_y = (f64::from(PRESS_ROW) + CELL_CENTRE) * CELL_H;
    let (row, col) = cell_at(press_x, press_y, CELL_W, CELL_H, cols, rows);
    assert_eq!(
        (row, col),
        (PRESS_ROW, PRESS_COL),
        "cell_at must map the centre pixel of cell ({PRESS_ROW}, {PRESS_COL}) back to that cell; \
         asserting this BEFORE sending is what proves the rest of the test exercises the intended \
         cell rather than whatever nvim happened to do"
    );

    // --- 2. The button/modifier seam accepts the primary button --------------
    let input = mouse_input(gdk::BUTTON_PRIMARY, ModifierType::empty())
        .expect("the PRIMARY button must be forwarded by mouse_input");

    core.send_mouse(MouseEvent {
        button: input.button,
        action: MouseAction::Press,
        grid,
        row,
        col,
        ctrl: input.ctrl,
        shift: input.shift,
        alt: input.alt,
    });

    let landed = poll_until(|| cursor_cell(&core, grid) == Some((PRESS_ROW, PRESS_COL)));
    assert!(
        landed,
        "a left PRESS at grid-local ({PRESS_ROW}, {PRESS_COL}) on grid {grid} should move the \
         cursor exactly there — the buffer is filled with 5-char-wide lines so column \
         {PRESS_COL} is a real position; observed {:?}",
        cursor_cell(&core, grid)
    );

    // --- 3. Drag to a DELIBERATELY out-of-bounds pixel -----------------------
    // In production these coordinates arrive as `start_point() + offset`, and a
    // drag routinely carries the point outside the widget. Nothing rejects it:
    // `cell_at` clamps to the nearest edge cell, which is exactly what makes
    // drag-past-the-edge selection work.
    let drag_x = (f64::from(cols) + OVERSHOOT_CELLS) * CELL_W;
    let drag_y = (f64::from(rows) + OVERSHOOT_CELLS) * CELL_H;
    let (drag_row, drag_col) = cell_at(drag_x, drag_y, CELL_W, CELL_H, cols, rows);
    assert_eq!(
        (drag_row, drag_col),
        (u32::from(rows - 1), u32::from(cols - 1)),
        "a point {OVERSHOOT_CELLS} cells beyond the right/bottom edge MUST clamp to the last cell \
         of the {cols}x{rows} grid"
    );

    core.send_mouse(MouseEvent {
        button: input.button,
        action: MouseAction::Drag,
        grid,
        row: drag_row,
        col: drag_col,
        ctrl: input.ctrl,
        shift: input.shift,
        alt: input.alt,
    });

    // --- 4. The drag put nvim into visual mode -------------------------------
    // Name-based on purpose (the style `crates/wolf-core/tests/input_keys.rs`
    // uses): do not couple this to nvim's `mode_info_set` ordering.
    let in_visual = poll_until(|| mode_name(&core).is_some_and(|n| n.contains("visual")));
    assert!(
        in_visual,
        "after PRESS + DRAG under `mouse=a`, the selected ModeInfo should denote VISUAL mode; \
         observed name {:?} at mode_idx {}",
        mode_name(&core),
        core.read_snapshot().cursor.mode_idx
    );

    // --- 5. Release keeps the selection --------------------------------------
    core.send_mouse(MouseEvent {
        button: input.button,
        action: MouseAction::Release,
        grid,
        row: drag_row,
        col: drag_col,
        ctrl: input.ctrl,
        shift: input.shift,
        alt: input.alt,
    });

    // A RELEASE need not produce a redraw flush of its own, so there is nothing
    // to poll for; give the ordered input channel a brief settle window instead
    // (the same idiom `tests/roundtrip_nvim.rs` uses) and then re-read.
    waiter.drain();
    sleep(SETTLE);
    waiter.drain();

    let moved_down = poll_until(|| cursor_cell(&core, grid).is_some_and(|(r, _)| r > PRESS_ROW));
    assert!(
        moved_down,
        "the drag should have moved the cursor DOWN from row {PRESS_ROW}; observed {:?}. \
         Directional on purpose: nvim clamps to the last BUFFER line, not the last grid row",
        cursor_cell(&core, grid)
    );
    assert!(
        mode_name(&core).is_some_and(|n| n.contains("visual")),
        "the selection must SURVIVE the release (still visual mode); observed name {:?}",
        mode_name(&core)
    );

    // --- 6. Negative control: a non-primary button is refused outright -------
    assert!(
        mouse_input(gdk::BUTTON_SECONDARY, ModifierType::empty()).is_none(),
        "a non-primary button must be refused by the seam, so no right-click can ever reach \
         Neovim (deliberate parity with macOS: no context menu, no middle-click paste)"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The scroll seams — per-unit threshold, sub-cell accumulation, direction
/// encoding — drive a real viewport in the direction real Neovim pins: a POSITIVE
/// GDK `dy` is `GDK_SCROLL_DOWN`, and scrolling down strictly INCREASES
/// `win_viewport.topline`.
#[test]
#[allow(
    clippy::float_cmp,
    reason = "the one f64 assertion here compares against 1.0, which is exactly representable \
              and is returned unmodified by `scroll_threshold`; an epsilon comparison would \
              weaken the assertion without making it more correct"
)]
fn scroll_seam_moves_viewport_via_wolfcore() {
    let (core, _waiter, grid) = start_with_filled_buffer();

    let (cols, rows) =
        grid_dims(&core, grid).expect("the live window grid should be in the snapshot");

    // The pointer cell the scroll events carry. In production this comes from the
    // `EventControllerMotion`-recorded position, because GTK's `scroll` signal
    // carries no coordinates of its own.
    let pointer_x = (f64::from(PRESS_COL) + CELL_CENTRE) * CELL_W;
    let pointer_y = (f64::from(PRESS_ROW) + CELL_CENTRE) * CELL_H;
    let (row, col) = cell_at(pointer_x, pointer_y, CELL_W, CELL_H, cols, rows);

    // --- 1. SURFACE deltas are pixels, so the threshold is the cell height ---
    let threshold = scroll_threshold(gdk::ScrollUnit::Surface, CELL_H);
    let mut acc = ScrollAccumulator::new();
    assert_eq!(
        acc.consume(CELL_H * SUB_CELL_FRACTION, threshold),
        0,
        "a single sub-cell nudge must cross no boundary"
    );
    assert_eq!(
        acc.consume(CELL_H * SUB_CELL_FRACTION, threshold),
        1,
        "two sub-cell nudges must cross exactly one boundary — the CARRY, driven through the \
         real accumulator rather than a reimplementation of it"
    );

    // --- 2. A multi-cell delta scrolls DOWN and INCREASES topline ------------
    let steps = acc.consume(CELL_H * SCROLL_CELLS, threshold);
    let (direction, count) =
        scroll_steps(steps).expect("a multi-cell delta must produce at least one step");
    assert_eq!(
        direction,
        ScrollDirection::Down,
        "a POSITIVE delta is GDK_SCROLL_DOWN. This is the OPPOSITE of the macOS convention and \
         is deliberate — see `mouse::scroll_steps`"
    );
    assert!(
        count >= 1,
        "a {SCROLL_CELLS}-cell delta must yield >= 1 step (got {count})"
    );

    let before_down = topline(&core, grid).expect("the window should report a viewport");
    for _ in 0..count {
        core.send_scroll(ScrollEvent {
            direction,
            grid,
            row,
            col,
        });
    }
    let scrolled_down = poll_until(|| topline(&core, grid).is_some_and(|t| t > before_down));
    assert!(
        scrolled_down,
        "scrolling DOWN must strictly INCREASE the viewport topline (was {before_down}, \
         observed {:?})",
        topline(&core, grid)
    );

    // --- 3. A negative delta scrolls UP and DECREASES topline ----------------
    let steps = acc.consume(-CELL_H * SCROLL_CELLS, threshold);
    let (direction, count) =
        scroll_steps(steps).expect("a negative multi-cell delta must produce steps");
    assert_eq!(
        direction,
        ScrollDirection::Up,
        "a NEGATIVE delta is GDK_SCROLL_UP — see `mouse::scroll_steps`"
    );

    let before_up = topline(&core, grid).expect("the window should report a viewport");
    assert!(
        before_up > 0,
        "the DOWN scroll should have left topline above 0 so the UP scroll has room to move \
         (got {before_up}); otherwise the assertion below would be unfalsifiable"
    );
    for _ in 0..count {
        core.send_scroll(ScrollEvent {
            direction,
            grid,
            row,
            col,
        });
    }
    let scrolled_up = poll_until(|| topline(&core, grid).is_some_and(|t| t < before_up));
    assert!(
        scrolled_up,
        "scrolling UP must strictly DECREASE the viewport topline (was {before_up}, \
         observed {:?})",
        topline(&core, grid)
    );

    // --- 4. A WHEEL detent is one step regardless of cell height -------------
    assert_eq!(
        scroll_threshold(gdk::ScrollUnit::Wheel, CELL_H),
        WHEEL_DETENT_THRESHOLD,
        "one wheel detent must be one line step whatever the cell height is — driven through \
         the same code path the SURFACE case above used"
    );

    core.shutdown().expect("shutdown should be Ok");
}
