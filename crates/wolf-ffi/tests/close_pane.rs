//! Integration test for `WolfCoreHandle::close_pane()` (plan §5): the FFI method
//! backing the macOS "Close" command, which runs `:close` to drop the CURRENT
//! split (NOT the buffer — that is `:bdelete`, exercised elsewhere).
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is the sibling of `panes.rs` (same staging /
//! SKIP-when-unset machinery, reusing `tests/common/mod.rs`, and the same
//! `windows()` live-grid polling pattern).
//!
//! ## Dependency on coder work (plan §5)
//!
//! `WolfCoreHandle::close_pane()` does NOT exist yet: the coder adds it to the
//! bridge (`extern "Rust"`) and `impl WolfCoreHandle` in
//! `crates/wolf-ffi/src/lib.rs`, backed by `wolf_core::InputEvent::ClosePane` ->
//! `nvim_command("close")`, AND adds the `//crates/wolf-ffi:close_pane` `rust_test`
//! target to `BUILD.bazel` (copy of `panes`). This test file does NOT touch BUILD
//! files, and it will not compile until that method lands — that is expected.
//!
//! ## Why the FFI method, NOT typed `:close`
//!
//! The FFI method issues `nvim_command("close")` over RPC, which bypasses the
//! captive `init.lua` interactive command-line overrides. A typed `:close<CR>` is
//! intercepted by those overrides, so this test drives `handle.close_pane()`
//! DIRECTLY (only the setup `:vsplit` is typed — that path emits a layout intent
//! and is deterministic headless, exactly as in `panes.rs`).
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.
//!
//! ## What this asserts
//!
//! From a single editor window, a typed `:vsplit` yields two live editor window
//! grids; calling `handle.close_pane()` drops one, returning to exactly one live
//! editor window grid. (The `:close` split-drop coverage that also inspects the
//! layout intent lives in `panes.rs`; this file proves the FFI method itself
//! reaches `:close` over RPC.)

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each pane-layout
/// change (split/close) to settle. Mirrors `panes.rs`'s `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for a pane-layout change to surface. Mirrors
/// `panes.rs`'s `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
//
// Char is first, Enter second (the `wolf_core::Key` variant order); the only two
// kinds needed to type an Ex command. Same as `panes.rs`.
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- Window grids -------------------------------------------------------------
/// The lowest real WINDOW grid id (grid 1 is the outer global grid / cmdline
/// strip; editor windows are >= 2 under `ext_multigrid`).
const WINDOW_GRID_MIN: u64 = 2;

/// Expected live editor window grids at startup and after `close_pane()`.
const ONE_WINDOW: usize = 1;
/// Expected live editor window grids after a `:vsplit`.
const TWO_WINDOWS: usize = 2;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false (the already-resolved glyph rides in `ch`). Same as `panes.rs`.
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

/// Builds an Enter `FfiKeyEvent` (kind=Enter), all modifiers false. Same as
/// `panes.rs`.
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

/// Types `cmd` as an Ex command and presses Enter, through the same
/// `send_key`/`FfiKeyEvent` path the generated Swift glue uses. `cmd` must include
/// the leading `:` (a printable char), e.g. `":vsplit"`. Copy of `panes.rs`'s
/// `send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// The number of live editor WINDOW grids from `windows()`: grids >=
/// `WINDOW_GRID_MIN` that are not hidden. `windows()` returns ALL windows (the
/// Swift side filters), so this applies the same filter the pane container uses to
/// count live panes. Copy of `panes.rs`'s `live_windows`, reduced to a count.
fn live_window_count(handle: &wolf_ffi::WolfCoreHandle) -> usize {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .count()
}

#[test]
fn close_pane_drops_one_split() {
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

    // --- 2. Baseline: poll until exactly one live editor window grid --------
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut baseline = live_window_count(&handle);
    while Instant::now() < deadline {
        baseline = live_window_count(&handle);
        if baseline == ONE_WINDOW {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        baseline, ONE_WINDOW,
        "exactly one editor window grid at startup (got {baseline})"
    );

    // --- 3. Type `:vsplit<CR>`; poll windows() until TWO live grids ---------
    send_ex_command(&handle, ":vsplit");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after_split = live_window_count(&handle);
    while Instant::now() < deadline {
        after_split = live_window_count(&handle);
        if after_split == TWO_WINDOWS {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        after_split, TWO_WINDOWS,
        "after :vsplit there should be exactly two live editor window grids (got {after_split})"
    );

    // --- 4. close_pane() -> :close over RPC; poll back to ONE live grid -----
    // Call the FFI method DIRECTLY (nvim_command("close") over RPC). A typed
    // `:close<CR>` would be intercepted by the captive init.lua overrides; the FFI
    // method bypasses them.
    handle.close_pane();

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after_close = live_window_count(&handle);
    while Instant::now() < deadline {
        after_close = live_window_count(&handle);
        if after_close == ONE_WINDOW {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        after_close, ONE_WINDOW,
        "after close_pane() there should be exactly one live editor window grid again \
         (got {after_close})"
    );

    // --- 5. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
