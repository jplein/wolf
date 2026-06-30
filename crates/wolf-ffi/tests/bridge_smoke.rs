//! Integration smoke test for the `wolf-ffi` Rust bridge surface.
//!
//! This drives the RUST half of the swift-bridge surface (plan §A) against a
//! REAL, hermetic Neovim — NO mocks, per the project testing policy. It does NOT
//! involve Swift, AppKit, or any window: it constructs `FfiKeyEvent`s by hand and
//! calls `wolf_ffi_start` / `read_frame` / `send_key` / `resize` / `shutdown`
//! directly, exactly as the generated Swift glue eventually will.
//!
//! ## SKIP-when-unset policy
//!
//! Like the wolf-core suite, the bridge needs a pinned nvim located via
//! `WOLF_NVIM_BIN`. UNLIKE wolf-core (which panics), this test **loudly SKIPS**
//! when `WOLF_NVIM_BIN` is unset/empty: Bazel-on-macOS cannot provision nvim yet,
//! so the test must be a no-op there rather than a failure, while still running
//! for real on Linux/Cargo devShells. See `common::nvim_bin_or_skip`.
//!
//! ## TODO (wake symbol) — NOTE to coder
//!
//! Per plan §A the production wake is `extern "Swift" { fn wolf_ffi_wake(); }`,
//! wired as `WakeFn = Arc::new(|| wolf_ffi_wake())`. In a Rust-only integration
//! test there is no Swift code providing that symbol, so a naive build of
//! `wolf_ffi_start` would FAIL TO LINK on the unresolved Swift `wolf_ffi_wake`.
//! `wolf-ffi` MUST therefore expose a wake path that links WITHOUT Swift, e.g.:
//!   * a `#[cfg(test)]` / fallback no-op `wolf_ffi_wake` in the crate, OR
//!   * the planned `extern "C" fn wolf_ffi_start(..., wake: extern "C" fn())`
//!     shim, against which this test passes a no-op `extern "C"` wake.
//! Whichever the coder picks, `wolf_ffi_start(init_lua, cwd, cols, rows)` as
//! called below must link and run with no Swift runtime present. The wake is
//! irrelevant to what this test asserts (it polls `read_frame`, see below).
//!
//! ## NOTE to coder — import paths
//!
//! The names below (`wolf_ffi_start`, `WolfCoreHandle`, `FfiFrame`, `FfiCell`,
//! `FfiKeyEvent`) must be reachable from the `wolf_ffi` crate root for this rlib
//! integration test to name them. If swift-bridge keeps them inside the `ffi`
//! bridge module, add `pub use ffi::{...};` (and a plain `pub fn wolf_ffi_start`)
//! at the crate root, or adjust the `use` below to the real module path.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiFrame, FfiKeyEvent};

/// `FfiKeyEvent.kind` discriminant for a printable Char.
///
/// NOTE to coder: must match the Rust `FfiKeyEvent` kind scheme (plan §A) and the
/// Swift `KeyMapTests` `Kind.char` (both mirror `wolf_core::Key` variant order,
/// where `Char` is first). If the bridge numbers kinds differently, update this
/// constant and the Swift `Kind` enum together.
const KIND_CHAR: u8 = 0;

/// Generous overall timeout for nvim `--embed` startup + redraw processing,
/// matching the wolf-core suite's `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for nvim to reflect input/resize. The bridge's
/// `FfiFrame` surface does NOT expose the snapshot `seq`, so — unlike wolf-core's
/// seq-based `FlushWaiter` — this test cannot key on `seq` advancing. It instead
/// POLLS `read_frame()` until the observable grid changes (or a timeout), which
/// is the same pattern the wolf-core `current_file.rs` / multigrid tests use for
/// out-of-band, non-seq-bumping updates. This is the robust choice given the
/// surface; if the coder later adds a frame sequence accessor, prefer that.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Resize target (clearly different from the 80x24 start so the change is
/// unambiguous).
const RESIZE_COLS: u16 = 100;
const RESIZE_ROWS: u16 = 40;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false (the Shift contract: the already-resolved glyph rides in `ch`).
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

/// Flattens the current frame's cell texts (row-major) into one string, for a
/// robust "did anything change / did 'x' appear" comparison that does not pin
/// exact coordinates.
fn frame_text(frame: &FfiFrame) -> String {
    frame.cells().into_iter().map(|cell| cell.text).collect()
}

#[test]
fn bridge_smoke() {
    // Loud SKIP (no-op pass) when no pinned nvim is available. Otherwise export the
    // resolved binary as `WOLF_NVIM_BIN` so wolf-core (which locates nvim only via
    // that env var) launches it — the resolved path may come from an explicit
    // `WOLF_NVIM_BIN` (cargo/devShell) or the Bazel-staged tree.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let cwd = common::fresh_cwd();

    // NOTE to coder: `wolf_ffi_start` takes `init_lua: String, cwd: String` per
    // plan §A. We pass the init.lua PATH and the cwd PATH as strings (wolf-core's
    // `CoreConfig.init_lua` is itself a path). If the bridge instead expects the
    // Lua SOURCE TEXT in `init_lua`, the coder must reconcile — but the wolf-core
    // integration tests pass the PATH, so we mirror that.
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- Initial frame: dims sane, cells count == cols * rows ---------------
    let frame = handle.read_frame();
    let cols = frame.cols();
    let rows = frame.rows();
    assert!(cols > 0, "frame cols should be > 0 (got {cols})");
    assert!(rows > 0, "frame rows should be > 0 (got {rows})");

    // The flat cells Vec is row-major and dense: exactly cols * rows entries.
    let expected_cells = cols as usize * rows as usize;
    assert_eq!(
        frame.cells().len(),
        expected_cells,
        "cells() should be a dense row-major grid of cols*rows ({cols}*{rows}={expected_cells})"
    );

    // Cursor reads back within the grid bounds (loose sanity, not exact coords).
    assert!(
        frame.cursor_row() < rows,
        "cursor_row ({}) should be within the grid height ({rows})",
        frame.cursor_row()
    );
    assert!(
        frame.cursor_col() < cols,
        "cursor_col ({}) should be within the grid width ({cols})",
        frame.cursor_col()
    );
    // cursor_visible is a bool either way; just exercise the accessor.
    let _ = frame.cursor_visible();

    let before = frame_text(&handle.read_frame());

    // --- Send input: enter insert mode, type 'x' ----------------------------
    // A bare 'x' in normal mode on an empty buffer is a no-op, so we first enter
    // insert mode ('i') and then type 'x' (kind=Char, ch='x') so the grid is
    // guaranteed to change. Both go through the same FfiKeyEvent/send_key path.
    handle.send_key(char_key('i'));
    handle.send_key(char_key('x'));

    // Poll until the grid reflects the input (some cell changed). Robust: we do
    // not assert exact coordinates, only that the observable grid moved and that
    // the typed 'x' shows up somewhere.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after = before.clone();
    while Instant::now() < deadline {
        after = frame_text(&handle.read_frame());
        if after != before && after.contains('x') {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_ne!(
        after, before,
        "the grid text should change after typing 'i' then 'x' \
         (before={before:?}, after={after:?})"
    );
    assert!(
        after.contains('x'),
        "the typed 'x' should appear somewhere in the grid after insert \
         (after={after:?})"
    );

    // --- Resize: dims update to the new request -----------------------------
    // `resize` drives the GLOBAL UI size (`nvim_ui_try_resize`). Under
    // `ext_multigrid` the ACTIVE WINDOW grid (what `read_frame` reports) is the
    // requested WIDTH exactly, but the requested HEIGHT MINUS the cmdline row(s):
    // with `laststatus=0` there is no statusline, leaving ~1 cmdline line, so the
    // window grid is `RESIZE_ROWS - 1` (allow a small tolerance for chrome).
    const MIN_RESIZE_ROWS: u16 = RESIZE_ROWS - 2;
    handle.resize(RESIZE_COLS, RESIZE_ROWS);

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut resized = handle.read_frame();
    while Instant::now() < deadline {
        resized = handle.read_frame();
        if resized.cols() == RESIZE_COLS && resized.rows() >= MIN_RESIZE_ROWS {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    let rcols = resized.cols();
    let rrows = resized.rows();
    assert_eq!(
        rcols, RESIZE_COLS,
        "active-window grid width should match the requested UI width (got {rcols})"
    );
    assert!(
        (MIN_RESIZE_ROWS..=RESIZE_ROWS).contains(&rrows),
        "active-window grid height ({rrows}) should be the requested UI height \
         ({RESIZE_ROWS}) minus the cmdline row(s)"
    );
    // The cells Vec must stay consistent with the ACTUAL (post-chrome) dims.
    assert_eq!(
        resized.cells().len(),
        rcols as usize * rrows as usize,
        "cells() should track the resized grid dimensions ({rcols}x{rrows})"
    );

    // --- Shutdown: clean teardown -------------------------------------------
    // NOTE to coder: plan §A specifies `shutdown(&self)` (takes the WolfCore out
    // of the handle and shuts it down). If the bridge returns a Result here,
    // adjust to `.expect(..)`.
    handle.shutdown();
}
