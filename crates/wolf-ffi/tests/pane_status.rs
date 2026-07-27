//! Integration test for the `wolf-ffi` per-pane statusbar bridge surface.
//!
//! This drives the RUST half of the statusbar machinery against a REAL, hermetic
//! Neovim — NO mocks, per the project testing policy. It is the sibling of
//! `panes.rs` (same staging / SKIP-when-unset machinery, reusing
//! `tests/common/mod.rs`) but exercises the NEW opaque `FfiPaneStatus` surface the
//! coder adds per plan §5:
//!
//!   * `WolfCoreHandle::pane_status_for_grid(grid: u64) -> FfiPaneStatus`
//!   * `FfiPaneStatus::filename() -> String`
//!   * `FfiPaneStatus::display_path() -> String`
//!   * `FfiPaneStatus::abs_path() -> String`
//!
//! ## Dependency on coder work (plan §5 / §8)
//!
//! These do not exist yet: the coder implements `FfiPaneStatus` + the accessors +
//! `pane_status_for_grid` in `crates/wolf-ffi/src/lib.rs`, regenerates `generated/`,
//! AND adds the `//crates/wolf-ffi:pane_status` `rust_test` target to `BUILD.bazel`
//! (mirroring `panes`, srcs `["tests/common/mod.rs", "tests/pane_status.rs"]`,
//! `crate_root = "tests/pane_status.rs"`). This test file does NOT touch BUILD files.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`.
//!
//! ## What this asserts
//!
//! A file opened under the session cwd surfaces its base name via
//! `pane_status_for_grid(grid).filename()` with a non-empty cwd-relative
//! `display_path()`, and its `abs_path()` equals the opened file's CANONICAL
//! absolute path (the raw, never-relativized buffer path core stores — canonical
//! because `apply_buffers` canonicalizes each `nvim_buf_get_name`); and after a
//! `:vsplit` + opening a SECOND file in the new window, the two window grids report
//! DISTINCT filenames (per-window, not global).

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each layout change to
/// settle. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for a change to surface. Mirrors `panes.rs`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- Window grids -------------------------------------------------------------
/// The lowest real WINDOW grid id (grid 1 is the outer global grid / cmdline
/// strip; editor windows are >= 2 under `ext_multigrid`).
const WINDOW_GRID_MIN: u64 = 2;

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

/// Types `cmd` as an Ex command and presses Enter. `cmd` must include the leading
/// `:` (a printable char), e.g. `":vsplit"`. Mirrors `panes.rs::send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// The live editor WINDOW grids (id + win handle) from `windows()`: grids >=
/// `WINDOW_GRID_MIN` that are not hidden (mirrors `panes.rs::live_windows`).
fn live_windows(handle: &wolf_ffi::WolfCoreHandle) -> Vec<(u64, i64)> {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .map(|w| (w.grid(), w.win()))
        .collect()
}

#[test]
fn pane_status_reports_filename_and_path_per_grid() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd; two files created INSIDE it so their buffer paths are
    // deterministic and canonicalizable (cwd-relative display path).
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(tmp.path().join("alpha.md"), "# alpha\n").expect("failed to write alpha.md");
    std::fs::write(tmp.path().join("beta.md"), "# beta\n").expect("failed to write beta.md");

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
        "core should attach and report a non-empty active frame (got cols {})",
        frame.cols()
    );

    // --- 2. Baseline: exactly one window grid >= 2 --------------------------
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
    let (original_grid, _original_win) = baseline[0];

    // --- 3. Open a file under cwd; poll pane_status_for_grid(original) -------
    send_ex_command(&handle, ":e alpha.md");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let st = handle.pane_status_for_grid(original_grid);
        if st.filename() == "alpha.md" && !st.display_path().is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for pane_status_for_grid({original_grid}) to report alpha.md \
             with a non-empty display_path (got filename={:?}, display_path={:?})",
            st.filename(),
            st.display_path()
        );
        sleep(POLL_INTERVAL);
    }

    // --- 3b. abs_path() is the opened file's CANONICAL absolute path --------
    // `PaneStatus.abs_path` is the RAW, never-relativized buffer path core stored
    // for this window. Core canonicalizes each `nvim_buf_get_name` in
    // `apply_buffers` (redraw.rs), so it resolves the tempdir symlink (on macOS
    // `/var/folders/...` -> `/private/var/folders/...`). Canonicalize the expected
    // path the SAME way so the comparison lines up regardless of that symlink.
    let expected_abs = std::fs::canonicalize(tmp.path().join("alpha.md"))
        .expect("alpha.md should canonicalize to an absolute path")
        .to_string_lossy()
        .into_owned();
    let abs_path = handle.pane_status_for_grid(original_grid).abs_path();
    assert_eq!(
        abs_path, expected_abs,
        "pane_status_for_grid({original_grid}).abs_path() should equal alpha.md's \
         canonical absolute path"
    );

    // --- 4. vsplit; poll windows() until two grids --------------------------
    send_ex_command(&handle, ":vsplit");
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

    // --- 5. Open a DIFFERENT file in the new window; poll distinct names -----
    send_ex_command(&handle, ":e beta.md");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut names: Vec<String> = Vec::new();
    loop {
        let grids = live_windows(&handle);
        if grids.len() >= 2 {
            names = grids
                .iter()
                .map(|(g, _)| handle.pane_status_for_grid(*g).filename())
                .collect();
            if names.iter().any(|n| n == "alpha.md") && names.iter().any(|n| n == "beta.md") {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the two grids to report DISTINCT filenames \
             (last observed: {names:?})"
        );
        sleep(POLL_INTERVAL);
    }
    assert!(
        names.iter().any(|n| n == "alpha.md"),
        "one window grid should report alpha.md (got {names:?})"
    );
    assert!(
        names.iter().any(|n| n == "beta.md"),
        "the other window grid should report beta.md (got {names:?})"
    );

    // --- 6. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
