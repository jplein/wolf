//! Integration test for the `wolf-ffi` split-panes bridge surface.
//!
//! This drives the RUST half of the swift-bridge pane machinery against a REAL,
//! hermetic Neovim — NO mocks, per the project testing policy. It is the sibling
//! of `tree_search.rs` / `file_search.rs` (same staging / SKIP-when-unset
//! machinery, reusing `tests/common/mod.rs`) but exercises the NEW pane surface
//! the coder adds per plan §3:
//!
//!   * `WolfCoreHandle::windows() -> Vec<FfiWinInfo>` (`grid()`, `win()`,
//!     `hidden()`)
//!   * `WolfCoreHandle::active_win() -> i64`
//!   * `WolfCoreHandle::grid_for_win(win: i64) -> u64`
//!   * `WolfCoreHandle::read_frame_for_grid(grid: u64) -> FfiFrame`
//!   * `WolfCoreHandle::drain_intents() -> Vec<FfiLayoutIntent>` (`kind()`,
//!     `orientation()`, `dir()`, `from()`, `win()`)
//!   * `WolfCoreHandle::resize_grid(grid: u64, cols: u16, rows: u16)`
//!   * `WolfCoreHandle::focus_win(win: i64)`
//!
//! ## Dependency on coder work (plan §3 + §5)
//!
//! These accessors do not exist yet: the coder implements them in
//! `crates/wolf-ffi/src/lib.rs` (the `FfiLayoutIntent` / `FfiWinInfo` opaque types
//! + the `WolfCoreHandle` methods), regenerates `generated/`, AND adds the
//! `//crates/wolf-ffi:panes` `rust_test` target to `BUILD.bazel` (mirroring
//! `tree_search`, srcs `["tests/common/mod.rs", "tests/panes.rs"]`,
//! `crate_root = "tests/panes.rs"`). This test file does NOT touch BUILD files.
//!
//! ## Discriminant contract (plan §3.1)
//!
//! Intent kind: SPLIT = 0, FOCUS_DIR = 1. Orientation: HORIZONTAL = 0,
//! VERTICAL = 1. These MUST match `mod intent_kind` / `mod orientation` in
//! `wolf-ffi/src/lib.rs` and the Swift `Orientation` / `FocusDir` ports.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.
//!
//! ## What this asserts
//!
//! Starting from a single editor window, a `:vsplit` emits a VERTICAL SPLIT intent
//! naming a new window distinct from the original; the new window grid then
//! appears in `windows()` and reads a non-empty per-grid frame; `resize_grid`
//! drives that grid's authoritative size; `focus_win` moves `active_win`; and a
//! `:close` from the focused window makes its grid vanish from `windows()` (no
//! close intent — close is observed via the window disappearing), with the outer
//! grid 1 still present for the cmdline strip.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each pane-layout
/// change (split/resize/focus/close) to settle. Mirrors the sibling tests'
/// `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for a pane-layout change to surface. Mirrors the
/// sibling tests' `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
//
// Char is first, Enter second (the `wolf_core::Key` variant order). These are the
// only two kinds this test needs to type an Ex command.
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- Layout-intent discriminants (mirror plan §3.1) ---------------------------
/// `FfiLayoutIntent::kind()` for a window SPLIT.
const INTENT_KIND_SPLIT: u8 = 0;
/// `SplitOrientation::Vertical` discriminant (side-by-side, `:vsplit`).
const ORIENTATION_VERTICAL: u8 = 1;

// -- Window grids -------------------------------------------------------------
/// The lowest real WINDOW grid id (grid 1 is the outer global grid / cmdline
/// strip; editor windows are >= 2 under `ext_multigrid`).
const WINDOW_GRID_MIN: u64 = 2;
/// The outer global grid id (painted by the cmdline strip).
const OUTER_GRID: u64 = 1;

// -- Resize target ------------------------------------------------------------
/// New-pane resize width — clearly smaller than the 80-col start so the change is
/// unambiguous.
const RESIZE_COLS: u16 = 40;
/// New-pane resize height.
const RESIZE_ROWS: u16 = 20;

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

/// Types `cmd` as an Ex command and presses Enter: each char goes through the
/// same `send_key`/`FfiKeyEvent` path the generated Swift glue uses, then a final
/// `<CR>` executes it. `cmd` must include the leading `:` (a printable char), e.g.
/// `":vsplit"`. The Ex-command path emits the same layout intent as the `<C-w>`
/// chords but is more deterministic headless.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// The live editor WINDOW grids (id + win handle) from `windows()`: grids >=
/// `WINDOW_GRID_MIN` that are not hidden. `windows()` returns ALL windows (the
/// Swift side filters), so this test applies the same filter the pane container
/// uses to count live panes.
fn live_windows(handle: &wolf_ffi::WolfCoreHandle) -> Vec<(u64, i64)> {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .map(|w| (w.grid(), w.win()))
        .collect()
}

#[test]
fn panes_split_resize_focus_close() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd (its `<cwd>/.wolf/` MRU cache must outlive the session), kept
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

    // --- 2. Baseline: exactly one window grid >= 2 --------------------------
    // The single editor window may surface a beat after the first frame, so poll
    // until at least one live window grid appears.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut baseline = live_windows(&handle);
    while Instant::now() < deadline {
        baseline = live_windows(&handle);
        if !baseline.is_empty() {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        baseline.len(),
        1,
        "exactly one editor window grid at startup (got {baseline:?})"
    );
    let (original_grid, original_win) = baseline[0];
    assert!(
        original_grid >= WINDOW_GRID_MIN,
        "the editor window grid should be >= {WINDOW_GRID_MIN} (got {original_grid})"
    );

    // active_win() is nonzero and equals the only window.
    let active = handle.active_win();
    assert_ne!(active, 0, "active_win() should be nonzero with a live window");
    assert_eq!(
        active, original_win,
        "active_win() should equal the only window (active={active}, win={original_win})"
    );

    // grid_for_win(win) round-trips to the baseline grid.
    assert_eq!(
        handle.grid_for_win(original_win),
        original_grid,
        "grid_for_win(original_win) should equal the baseline grid"
    );

    // The baseline grid reads a non-empty per-grid frame.
    assert!(
        handle.read_frame_for_grid(original_grid).cols() > 0,
        "read_frame_for_grid(original_grid) should report a non-empty frame"
    );

    // --- 3. Trigger a vsplit; poll drain_intents() for a SPLIT intent -------
    // PREFER `:vsplit<CR>` over `<C-w>v`: the Ex-command path also emits the Split
    // intent and is more deterministic headless.
    send_ex_command(&handle, ":vsplit");

    // drain_intents() DRAINS, so accumulate across polls until a SPLIT appears,
    // capturing its scalar fields (the opaque intent itself is not retained).
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut split: Option<(u8, i64, i64)> = None; // (orientation, from, win)
    while Instant::now() < deadline {
        for intent in handle.drain_intents() {
            if intent.kind() == INTENT_KIND_SPLIT {
                split = Some((intent.orientation(), intent.from(), intent.win()));
            }
        }
        if split.is_some() {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    let (split_orientation, split_from, new_win) =
        split.expect("a :vsplit should emit a SPLIT layout intent within the timeout");
    assert_eq!(
        split_orientation, ORIENTATION_VERTICAL,
        "a :vsplit should carry orientation VERTICAL ({ORIENTATION_VERTICAL})"
    );
    assert_eq!(
        split_from, original_win,
        "the SPLIT intent's from() should be the original window \
         (from={split_from}, original={original_win})"
    );
    assert_ne!(new_win, 0, "the SPLIT intent's win() should be a nonzero new handle");
    assert_ne!(
        new_win, original_win,
        "the SPLIT intent's win() should be DISTINCT from the original window"
    );

    // --- 4. Poll windows() until TWO window grids ---------------------------
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after_split = live_windows(&handle);
    while Instant::now() < deadline {
        after_split = live_windows(&handle);
        if after_split.len() >= 2 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        after_split.len(),
        2,
        "after :vsplit there should be exactly two window grids (got {after_split:?})"
    );
    assert!(
        after_split.iter().any(|(_, win)| *win == new_win),
        "the new window {new_win} should be present in windows() (got {after_split:?})"
    );

    // grid_for_win(newWin) is nonzero and distinct from the original grid.
    let new_grid = handle.grid_for_win(new_win);
    assert_ne!(new_grid, 0, "grid_for_win(new_win) should be a nonzero grid");
    assert_ne!(
        new_grid, original_grid,
        "the new window's grid should be DISTINCT from the original grid \
         (new={new_grid}, original={original_grid})"
    );
    assert!(
        handle.read_frame_for_grid(new_grid).cols() > 0,
        "read_frame_for_grid(new_grid) should report a non-empty frame"
    );

    // --- 5. resize_grid(newGrid, 40, 20); poll until nvim reflects it -------
    // nvim is authoritative (it echoes the size back via grid_resize), so the
    // per-grid frame's cols() should settle to exactly RESIZE_COLS. If exact
    // equality proves brittle headless, the fallback is `cols() > 0 && cols() <=
    // RESIZE_COLS` — but we PREFER exact here.
    handle.resize_grid(new_grid, RESIZE_COLS, RESIZE_ROWS);

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut resized = handle.read_frame_for_grid(new_grid);
    while Instant::now() < deadline {
        resized = handle.read_frame_for_grid(new_grid);
        if resized.cols() == RESIZE_COLS {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        resized.cols(),
        RESIZE_COLS,
        "the new grid's width should settle to the requested {RESIZE_COLS} \
         (got {}); fallback if flaky: 0 < cols <= {RESIZE_COLS}",
        resized.cols()
    );

    // --- 6. focus_win(originalWin); poll active_win() until it matches ------
    handle.focus_win(original_win);

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut focused = handle.active_win();
    while Instant::now() < deadline {
        focused = handle.active_win();
        if focused == original_win {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        focused, original_win,
        "active_win() should follow focus_win(original_win) (got {focused})"
    );

    // --- 7. Close the focused (original) window; poll windows() to ONE ------
    // We focused original_win in step 6, so `:close` closes THAT window. The close
    // is observed via the window vanishing from windows() — there is NO close
    // intent. The survivor is the new window.
    let closed_win = original_win;
    send_ex_command(&handle, ":close");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after_close = live_windows(&handle);
    while Instant::now() < deadline {
        after_close = live_windows(&handle);
        if after_close.len() == 1 && handle.grid_for_win(closed_win) == 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        after_close.len(),
        1,
        "after :close there should be exactly one window grid again (got {after_close:?})"
    );
    assert_eq!(
        handle.grid_for_win(closed_win),
        0,
        "grid_for_win(closed_win) should be 0 once the window has vanished"
    );

    // --- 8. Outer grid 1 stays present for the cmdline strip ----------------
    assert!(
        handle.read_frame_for_grid(OUTER_GRID).rows() > 0,
        "the outer grid {OUTER_GRID} should still report rows for the cmdline strip"
    );

    // --- 9. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
