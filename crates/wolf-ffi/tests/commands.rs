//! Integration test for the `wolf-ffi` COMMAND CHANNEL bridge surface.
//!
//! This drives the RUST half of the swift-bridge command machinery against a REAL,
//! hermetic Neovim — NO mocks, per the project testing policy. It is the sibling
//! of `panes.rs` (same staging / SKIP-when-unset machinery, reusing
//! `tests/common/mod.rs`) but exercises the NEW command surface the coder adds per
//! plan §3:
//!
//!   * `WolfCoreHandle::drain_commands() -> Vec<FfiCommand>`
//!   * opaque `FfiCommand` with a `kind() -> u8` accessor
//!   * a `command_kind` discriminant module (single source of truth)
//!
//! ## Dependency on coder work (plan §3 + §5)
//!
//! These do not exist yet: the coder implements `FfiCommand` +
//! `WolfCoreHandle::drain_commands()` in `crates/wolf-ffi/src/lib.rs`, regenerates
//! `generated/`, AND adds the `//crates/wolf-ffi:commands` `rust_test` target to
//! `BUILD.bazel` (copy of `panes`). This test file does NOT touch BUILD files.
//!
//! ## Discriminant contract (plan §3, mirrors `mod command_kind`)
//!
//! NEW_WINDOW = 0, CLOSE_WINDOW = 1, QUIT_APPLICATION = 2, HIDE_APPLICATION = 3,
//! MINIMIZE_WINDOW = 4 (the `wolf_core::Command` variant order). These local consts
//! MUST match `mod command_kind` in `wolf-ffi/src/lib.rs` and the Swift
//! `CommandKind` port — exactly how `panes.rs` re-declares `INTENT_KIND_SPLIT`
//! locally as a contract guard.
//!
//! ## What this asserts
//!
//! From a single editor window, typing `:WolfCloseWindow<CR>` drains an
//! `FfiCommand` whose `kind()` is CLOSE_WINDOW (1); typing `:WolfMinimizeWindow<CR>`
//! drains one whose `kind()` is MINIMIZE_WINDOW (4). NOTHING acts on these drains
//! (the front-end that would perform close/minimize is absent), so no window is
//! destroyed — these are PURE FFI drains, the same way `panes.rs`'s close is
//! observed structurally rather than via a destructive side effect. We deliberately
//! do NOT trigger a command that would quit/close anything real here.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each command to
/// surface on the drain queue. Mirrors `panes.rs`'s `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for a command to surface. Mirrors `panes.rs`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
//
// Char is first, Enter second (the `wolf_core::Key` variant order); the only two
// kinds needed to type an Ex command. Same as `panes.rs`.
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- command_kind discriminants (mirror plan §3, `mod command_kind`) -----------
//
// The single-source-of-truth contract guard. Declared LOCALLY here, exactly how
// `panes.rs` re-declares `INTENT_KIND_SPLIT`, and cross-checked below so a future
// reordering breaks this test rather than silently drifting.
/// `FfiCommand::kind()` for New Window.
const CMD_KIND_NEW_WINDOW: u8 = 0;
/// `FfiCommand::kind()` for Close Window.
const CMD_KIND_CLOSE_WINDOW: u8 = 1;
/// `FfiCommand::kind()` for Quit Application.
const CMD_KIND_QUIT_APPLICATION: u8 = 2;
/// `FfiCommand::kind()` for Hide Application.
const CMD_KIND_HIDE_APPLICATION: u8 = 3;
/// `FfiCommand::kind()` for Minimize Window.
const CMD_KIND_MINIMIZE_WINDOW: u8 = 4;

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
/// the leading `:` (a printable char), e.g. `":WolfCloseWindow"`. Copy of
/// `panes.rs`'s `send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// Polls `handle.drain_commands()` until an `FfiCommand` with the given `kind`
/// appears, or panics after [`FLUSH_TIMEOUT`]. Draining accumulates across polls
/// (each `drain_commands()` call empties the queue), so any command of the target
/// kind seen in any poll counts. `desc` describes the awaited command.
fn poll_for_command_kind(handle: &wolf_ffi::WolfCoreHandle, kind: u8, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        for cmd in handle.drain_commands() {
            if cmd.kind() == kind {
                return;
            }
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for command kind {kind} ({desc})");
}

#[test]
fn commands_drain_close_and_minimize() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    // Discriminant-contract guard: the local consts MUST be contiguous 0..4 in
    // variant order (mirrors `mod command_kind`). Referencing all five here also
    // keeps them from tripping dead-code lints.
    assert_eq!(
        [
            CMD_KIND_NEW_WINDOW,
            CMD_KIND_CLOSE_WINDOW,
            CMD_KIND_QUIT_APPLICATION,
            CMD_KIND_HIDE_APPLICATION,
            CMD_KIND_MINIMIZE_WINDOW,
        ],
        [0, 1, 2, 3, 4],
        "command_kind discriminants must be contiguous 0..4 in Command variant order"
    );

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
        0,
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

    // No emit-on-attach: commands are user-triggered only, so nothing is queued
    // yet. (Not a hard assertion on timing — we simply discard any accidental
    // startup drain before triggering the real commands.)
    let _ = handle.drain_commands();

    // --- 2. :WolfCloseWindow<CR> -> an FfiCommand with kind() == CLOSE_WINDOW --
    // Nothing acts on this drain (no front-end present), so no window is closed.
    send_ex_command(&handle, ":WolfCloseWindow");
    poll_for_command_kind(&handle, CMD_KIND_CLOSE_WINDOW, ":WolfCloseWindow");

    // --- 3. :WolfMinimizeWindow<CR> -> an FfiCommand with kind() == MINIMIZE_WINDOW
    send_ex_command(&handle, ":WolfMinimizeWindow");
    poll_for_command_kind(&handle, CMD_KIND_MINIMIZE_WINDOW, ":WolfMinimizeWindow");

    // --- 4. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
