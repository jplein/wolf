//! Integration test for the `wolf-ffi` COMMAND CHANNEL bridge surface.
//!
//! This drives the RUST half of the swift-bridge command machinery against a REAL,
//! hermetic Neovim — NO mocks, per the project testing policy. It is the sibling
//! of `panes.rs` (same staging / SKIP-when-unset machinery, reusing
//! `tests/common/mod.rs`) but exercises the command surface:
//!
//!   * `WolfCoreHandle::drain_commands() -> Vec<FfiCommand>`
//!   * opaque `FfiCommand` with a `kind() -> u8` accessor
//!   * `wolf_ffi_command_kind(id: String) -> u8`, the bridge's lookup of an id's kind
//!   * `WolfCoreHandle::request_close_tab() -> u8`, Close Tab's check
//!   * `WolfCoreHandle::request_unsaved() -> u8` and `show_unsaved() -> bool`, the silent ask
//!     and the echo of Close Window's and Quit's check
//!
//! The `//crates/wolf-ffi:commands` `rust_test` target runs this file under Bazel.
//!
//! ## What this asserts
//!
//! For every row of `wolf_core::COMMAND_SPECS`, typing `:<ex_command><CR>` drains an
//! `FfiCommand` whose `kind()` is the row's index, and `wolf_ffi_command_kind` of the
//! row's id is that index too. The `command_kind` constants in `src/lib.rs` are the
//! Rust side's record of each discriminant, so a moved, reordered or inserted value
//! fails here, while an appended command passes with no edit to this file.
//!
//! `WolfCoreHandle::request_close_tab() -> u8` answers with `mod close_tab_answer`'s
//! discriminants: CLOSING on an unmodified tab, REFUSED on a modified one, and
//! NO_ANSWER once the core is gone. `request_unsaved() -> u8` answers with `mod
//! unsaved_answer`'s: CLEAN, UNSAVED, and NO_ANSWER once the core is gone, when
//! `show_unsaved()` answers `false`.
//!
//! Typing every command is safe: each Ex face `rpcnotify`s its id, `close_tab`'s once
//! its check passes (`_G.wolf_setup_commands` in `runtime/nvim/init.lua`), core only
//! queues it, and no front-end is present to act on a drain, so nothing closes, quits
//! or splits.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_core::COMMAND_SPECS;
use wolf_ffi::{wolf_ffi_command_kind, wolf_ffi_start, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each command to
/// surface on the drain queue. Mirrors `panes.rs`'s `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for a command to surface. Mirrors `panes.rs`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

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

/// Polls `handle.drain_commands()` until a command drains, and returns the first one's
/// `kind()`, or panics after [`FLUSH_TIMEOUT`]. Returning the first kind, rather than
/// waiting for an expected one, makes a wrong discriminant fail at once. `desc` names
/// the typed command.
fn first_drained_kind(handle: &wolf_ffi::WolfCoreHandle, desc: &str) -> u8 {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(cmd) = handle.drain_commands().first() {
            return cmd.kind();
        }
        sleep(POLL_INTERVAL);
    }
    panic!(
        "timed out after {FLUSH_TIMEOUT:?} waiting for any command after {desc}: the Ex command \
         never reached the channel, so it is not registered or its id is unknown to core"
    );
}

#[test]
fn every_command_drains_as_its_registry_index() {
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

    // Argument order mirrors the sibling tests: (init_lua, cwd, cols, rows).
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

    // Commands are user-triggered only; discard any accidental startup drain before the real ones.
    let _ = handle.drain_commands();

    // --- 2. Each row's Ex command drains as the row's index ------------------
    for (index, spec) in COMMAND_SPECS.iter().enumerate() {
        let index = u8::try_from(index).expect("COMMAND_SPECS has more rows than a u8 holds");
        let typed = format!(":{}", spec.ex_command);

        send_ex_command(&handle, &typed);
        let kind = first_drained_kind(&handle, &typed);
        assert_eq!(
            kind, index,
            "{typed} drained kind {kind}, but {:?} is COMMAND_SPECS row {index}: its command_kind \
             constant moved, or a variant was inserted or reordered ahead of it, or a row other \
             than its own drained first",
            spec.command
        );

        let looked_up = wolf_ffi_command_kind(spec.id.to_string());
        assert_eq!(
            looked_up,
            index,
            "wolf_ffi_command_kind({:?}) is {looked_up}, but the row's index is {index}; {} means \
             the id is unknown, anything else that the lookup disagrees with the drain",
            spec.id,
            u8::MAX
        );
    }

    // --- 3. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}

/// The buffer text the bridge test writes, built in Neovim from two halves so that the typed
/// command line, which shows the halves apart, can never show it.
const EDIT_MARKER: &str = "wolfclosetabedit";

/// `request_close_tab()` crosses the bridge as `mod close_tab_answer`'s discriminants: `2`
/// (`CLOSING`) on an unmodified tab, which notifies `close_tab`; `1` (`REFUSED`) once the
/// buffer is modified; and `0` (`NO_ANSWER`) once the core is gone. The values are literal
/// here, because the Swift `CloseTabAnswer` mirrors them by value.
#[test]
fn request_close_tab_crosses_the_bridge() {
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while handle.read_frame().cols() == 0 && Instant::now() < deadline {
        sleep(POLL_INTERVAL);
    }
    assert!(
        handle.read_frame().cols() > 0,
        "core should attach and report a non-empty active frame within {FLUSH_TIMEOUT:?}"
    );
    let _ = handle.drain_commands();

    let close_tab = COMMAND_SPECS
        .iter()
        .position(|spec| spec.id == "close_tab")
        .and_then(|index| u8::try_from(index).ok())
        .expect("COMMAND_SPECS must hold a close_tab row within a u8's range");

    let answer = handle.request_close_tab();
    assert_eq!(
        answer, 2,
        "an unmodified tab must answer 2 (CLOSING); 1 means REFUSED and CLOSING trade values or \
         the check counted an unmodified buffer, and 0 that the request failed"
    );
    let kind = first_drained_kind(&handle, "request_close_tab() on an unmodified tab");
    assert_eq!(
        kind, close_tab,
        "the CLOSING answer must come with close_tab's notification, kind {close_tab}; got kind \
         {kind}"
    );

    send_ex_command(&handle, ":call setline(1, 'wolfclose' . 'tabedit')");
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut shown = frame_text(&handle);
    while !shown.contains(EDIT_MARKER) && Instant::now() < deadline {
        sleep(POLL_INTERVAL);
        shown = frame_text(&handle);
    }
    assert!(
        shown.contains(EDIT_MARKER),
        "premise: the typed setline must show {EDIT_MARKER:?} in the frame within \
         {FLUSH_TIMEOUT:?}, so the buffer is modified before the request; the frame reads \
         {shown:?}"
    );

    let answer = handle.request_close_tab();
    assert_eq!(
        answer, 1,
        "a modified buffer must answer 1 (REFUSED); 2 means REFUSED and CLOSING trade values or \
         the check was skipped, and 0 that the request failed"
    );

    handle.shutdown();
    let answer = handle.request_close_tab();
    assert_eq!(
        answer, 0,
        "a shut-down core must answer 0 (NO_ANSWER), so the front-end closes the tab unchecked; \
         2 or 1 means the bridge invents an answer for a gone core"
    );
    drop(tmp);
}

/// The buffer text the unsaved-ask bridge test writes, built in Neovim from two halves as
/// [`EDIT_MARKER`] is.
const UNSAVED_MARKER: &str = "wolfunsavededit";

/// CW7. `request_unsaved()` crosses the bridge as `mod unsaved_answer`'s discriminants: `2`
/// (`CLEAN`) on an unmodified tab, `1` (`UNSAVED`) once the buffer is modified, and `0`
/// (`NO_ANSWER`) once the core is gone. `show_unsaved()` answers `false`, `true`, then `false`
/// for the same three. The values are literal here, because the Swift `UnsavedAnswer` mirrors
/// them by value.
#[test]
fn request_unsaved_and_show_unsaved_cross_the_bridge() {
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while handle.read_frame().cols() == 0 && Instant::now() < deadline {
        sleep(POLL_INTERVAL);
    }
    assert!(
        handle.read_frame().cols() > 0,
        "core should attach and report a non-empty active frame within {FLUSH_TIMEOUT:?}"
    );

    let answer = handle.request_unsaved();
    assert_eq!(
        answer, 2,
        "an unmodified tab must answer 2 (CLEAN); 1 means UNSAVED and CLEAN trade values or the \
         ask counted an unmodified buffer, and 0 that the request failed"
    );
    assert!(
        !handle.show_unsaved(),
        "show_unsaved() on an unmodified tab must answer false; true means the bridge or core \
         answers true whatever Neovim found"
    );

    send_ex_command(&handle, ":call setline(1, 'wolfunsaved' . 'edit')");
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut shown = frame_text(&handle);
    while !shown.contains(UNSAVED_MARKER) && Instant::now() < deadline {
        sleep(POLL_INTERVAL);
        shown = frame_text(&handle);
    }
    assert!(
        shown.contains(UNSAVED_MARKER),
        "premise: the typed setline must show {UNSAVED_MARKER:?} in the frame within \
         {FLUSH_TIMEOUT:?}, so the buffer is modified before the request; the frame reads \
         {shown:?}"
    );

    let answer = handle.request_unsaved();
    assert_eq!(
        answer, 1,
        "a modified buffer must answer 1 (UNSAVED); 2 means UNSAVED and CLEAN trade values or \
         the ask was skipped, and 0 that the request failed"
    );
    assert!(
        handle.show_unsaved(),
        "show_unsaved() on a modified buffer must answer true; false means it found nothing \
         unsaved or its request failed"
    );

    handle.shutdown();
    let answer = handle.request_unsaved();
    assert_eq!(
        answer, 0,
        "a shut-down core must answer 0 (NO_ANSWER), so the front-end counts the tab clean; 2 or \
         1 means the bridge invents an answer for a gone core"
    );
    assert!(
        !handle.show_unsaved(),
        "show_unsaved() on a shut-down core must answer false; true means the bridge claims an \
         echo that no Neovim made"
    );
    drop(tmp);
}

/// The active frame's cell texts, row-major, as one string.
fn frame_text(handle: &wolf_ffi::WolfCoreHandle) -> String {
    handle
        .read_frame()
        .cells()
        .into_iter()
        .map(|cell| cell.text)
        .collect()
}
