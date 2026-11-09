//! Integration tests of the `wolf-ffi` terminal-drawer and rebind bridge surface:
//! `WolfCoreHandle::toggle_terminal`, which backs the macOS "Terminal" command (`:WolfTerminal`,
//! `` ⌃` ``) and opens or hides the drawer on a core session, `queue_bind_terminal`, which a
//! front-end uses to put live sessions back after a Neovim restart, and `terminal_cwd`, which it
//! persists for a terminal leaf.
//!
//! Each test starts a real, hermetic Neovim through `wolf_ffi_start` and a real shell in a pty,
//! with no mocks, reusing `tests/common/mod.rs`. Core's own drawer behaviour is proved by
//! `crates/wolf-core/tests/terminal_drawer.rs`; these tests prove the bridge carries it.
//!
//! We drive `toggle_terminal()` (the handle method a front-end calls) rather than
//! `:WolfTerminal`, which would only enqueue an `FfiCommand` with no front-end present to call
//! the method back.
//!
//! ## Premises about the components
//!
//! These are measured against the real components by the core suites, and re-observed here by
//! the assertions that depend on them:
//!
//! - The drawer runs the default shell, which [`common::pin_default_shell`] makes `/bin/sh`.
//!   On a tty it is interactive and echoes what it is typed, so a test looks for the output of
//!   an expression (`$((3+3))`) that the typed text does not contain.
//! - A command other than `""` runs as `/bin/sh -c <command>`, so `cd <dir> && exec /bin/sh`
//!   leaves a shell whose cwd, as the operating system reports it, is `<dir>`.
//! - The drawer opens focused, so `toggle_full_width` acts on the drawer's placeholder.
//!
//! ## SKIP-when-unset policy
//!
//! Loudly SKIPS (no-op pass) when no pinned nvim is available, via
//! `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, FfiTerminalFrame, WolfCoreHandle};

/// How long any effect may take to surface: nvim startup, a shell's output, a toggle.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// `intent_kind::SPLIT`.
const INTENT_SPLIT: u8 = 0;
/// `intent_kind::DRAWER_OPEN`, the open path's intent naming the drawer window.
const INTENT_DRAWER_OPEN: u8 = 2;

/// `terminal_target::WINDOW`: show the session in the given window.
const TARGET_WINDOW: u8 = 0;
/// `terminal_target::SPLIT_VERTICAL`: show the session in a vertical split of the window.
const TARGET_SPLIT_VERTICAL: u8 = 2;
/// A target no `terminal_target` constant names.
const TARGET_UNKNOWN: u8 = 9;

/// An id no session has.
const UNKNOWN_ID: u64 = 999;

/// `kind::ENTER`, the `FfiKeyEvent.kind` of the Return key.
const KIND_ENTER: u8 = 1;

/// Polls `pred` until it holds or [`TIMEOUT`] passes; panics with `desc` on timeout.
fn poll(desc: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if pred() {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {TIMEOUT:?} waiting for: {desc}");
}

/// Starts a core whose default shell is `/bin/sh` in a fresh cwd and waits for it to attach,
/// or `None` to skip.
fn start() -> Option<WolfCoreHandle> {
    common::nvim_bin_or_skip()?;
    common::pin_default_shell();
    let init_lua = common::init_lua_path();
    let cwd = common::fresh_cwd();
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    poll("the core to attach (read_frame cols > 0)", || {
        handle.read_frame().cols() > 0
    });
    Some(handle)
}

/// Row `row`'s text from `row_cells`, an empty cell as one space, trailing spaces trimmed.
fn row_text(frame: &FfiTerminalFrame, row: u16) -> String {
    let mut text: String = frame
        .row_cells(row)
        .iter()
        .map(|cell| {
            let t = cell.text();
            if t.is_empty() {
                " ".to_owned()
            } else {
                t
            }
        })
        .collect();
    text.truncate(text.trim_end_matches(' ').len());
    text
}

/// Every row's text, joined with newlines.
fn frame_text(frame: &FfiTerminalFrame) -> String {
    (0..frame.rows())
        .map(|row| row_text(frame, row))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Polls session `id`'s frame until it shows `needle`, panicking with the frame on timeout.
fn wait_for_text(handle: &WolfCoreHandle, id: u64, needle: &str) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let frame = handle.terminal_frame(id);
        if frame_text(&frame).contains(needle) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {TIMEOUT:?} waiting for session {id} to show {needle:?}; its \
                 frame reads:\n{}",
                frame_text(&frame)
            );
        }
        sleep(POLL_INTERVAL);
    }
}

/// Types `line` into session `id` and presses Enter, through the bridge.
fn run_line(handle: &WolfCoreHandle, id: u64, line: &str) {
    handle.terminal_text(id, line.to_owned());
    handle.terminal_key(
        id,
        FfiKeyEvent {
            kind: KIND_ENTER,
            ch: 0,
            fnum: 0,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        },
    );
}

/// Drains layout intents until one of `kind` arrives, returning its `win()`.
fn wait_for_intent(handle: &WolfCoreHandle, kind: u8, desc: &str) -> i64 {
    let deadline = Instant::now() + TIMEOUT;
    let mut seen = Vec::new();
    loop {
        for intent in handle.drain_intents() {
            if intent.kind() == kind {
                return intent.win();
            }
            seen.push(intent.kind());
        }
        if Instant::now() >= deadline {
            panic!("timed out after {TIMEOUT:?} waiting for {desc}; intent kinds seen: {seen:?}");
        }
        sleep(POLL_INTERVAL);
    }
}

/// Toggles the drawer open and returns its window, from the `DRAWER_OPEN` intent, and the
/// session that window shows.
fn open_drawer(handle: &WolfCoreHandle, desc: &str) -> (i64, u64) {
    handle.toggle_terminal();
    let win = wait_for_intent(handle, INTENT_DRAWER_OPEN, desc);
    assert_ne!(
        win, 0,
        "the DRAWER_OPEN intent must name a real window ({desc})"
    );
    poll(
        &format!(
            "win_buffer({win}).terminal() to name a session ({desc}); 0 means the drawer shows \
             no core session"
        ),
        || handle.win_buffer(win).terminal() != 0,
    );
    (win, handle.win_buffer(win).terminal())
}

/// Polls until the current tabpage has `count` windows.
fn wait_for_window_count(handle: &WolfCoreHandle, count: usize, desc: &str) {
    poll(
        &format!(
            "{count} windows ({desc}); last saw {}",
            handle.windows().len()
        ),
        || handle.windows().len() == count,
    );
}

/// The drawer opens on a core session that answers a typed line, hides with its session kept,
/// reopens on the same session, closes when its shell exits, and the next toggle starts a new
/// session.
#[test]
fn drawer_lifecycle_over_the_bridge() {
    let Some(handle) = start() else {
        return;
    };
    let base = handle.windows().len();
    let _ = handle.drain_intents();

    let (win, id) = open_drawer(&handle, "the first toggle");
    wait_for_window_count(&handle, base + 1, "the drawer opened");
    run_line(&handle, id, "echo bridge-$((3+3))");
    wait_for_text(&handle, id, "bridge-6");

    handle.toggle_terminal();
    wait_for_window_count(&handle, base, "the drawer hidden");
    assert!(
        !handle.win_buffer(win).reported(),
        "the hidden drawer window {win} must be gone from win_buffers."
    );
    assert!(
        handle.live_terminals().contains(&id),
        "hiding the drawer must keep session {id} live; live_terminals() reads {:?}",
        handle.live_terminals()
    );

    let (_win, again) = open_drawer(&handle, "the reopening toggle");
    assert_eq!(
        again, id,
        "the reopened drawer must show the same session {id}, not {again}: a new id means \
         toggle_terminal starts a shell even when the drawer's is live."
    );

    run_line(&handle, id, "exit");
    wait_for_window_count(&handle, base, "the drawer closed by its shell's exit");
    poll(
        &format!("session {id} to leave live_terminals() after its shell exited"),
        || !handle.live_terminals().contains(&id),
    );

    let (_win, next) = open_drawer(&handle, "the toggle after the exit");
    assert_ne!(
        next, id,
        "the toggle after the drawer's shell exited must start a new session, not show {id} \
         again."
    );

    handle.shutdown();
}

/// The drawer window defaults to full width while an editor window does not, and the
/// full-width toggle turns it off, through the same per-window channel an editor pane uses.
#[test]
fn drawer_defaults_to_full_width() {
    let Some(handle) = start() else {
        return;
    };
    let editor_win = handle.active_win();
    assert_ne!(
        editor_win, 0,
        "attach should report a nonzero active editor window"
    );
    assert!(
        !handle.full_width_for_win(editor_win),
        "the editor window must default to the ~90-char cap (full_width_for_win == false)."
    );
    let _ = handle.drain_intents();

    let (drawer_win, _id) = open_drawer(&handle, "the drawer toggle");
    poll(
        &format!(
            "full_width_for_win({drawer_win}) to be true; false means the drawer placeholder \
             lacks b:wolf_full_width"
        ),
        || handle.full_width_for_win(drawer_win),
    );
    assert!(
        !handle.full_width_for_win(editor_win),
        "opening the drawer must leave the editor window's width default alone."
    );

    handle.toggle_full_width();
    poll(
        &format!("full_width_for_win({drawer_win}) to be false after toggle_full_width"),
        || !handle.full_width_for_win(drawer_win),
    );

    handle.shutdown();
}

/// `terminal_cwd` reads a session's directory, `queue_bind_terminal` shows a live session in a
/// new split through the input queue, and both refuse what names no session or target.
#[test]
fn queue_bind_and_cwd() {
    let Some(handle) = start() else {
        return;
    };
    let dir = tempfile::tempdir().expect("failed to create the session's directory");
    let canon = dir
        .path()
        .canonicalize()
        .expect("the session's directory should canonicalize");
    let id = handle.open_terminal(
        0,
        TARGET_WINDOW,
        format!("cd '{}' && exec /bin/sh", canon.display()),
    );
    assert_ne!(id, 0, "open_terminal must start the session");

    poll(
        &format!(
            "terminal_cwd({id}) to read {}; \"\" means the bridge or core reports no cwd",
            canon.display()
        ),
        || {
            let cwd = handle.terminal_cwd(id);
            !cwd.is_empty() && PathBuf::from(&cwd).canonicalize().ok().as_ref() == Some(&canon)
        },
    );
    assert_eq!(
        handle.terminal_cwd(UNKNOWN_ID),
        "",
        "terminal_cwd of an id no session has must be \"\"."
    );

    let base = handle.windows().len();
    let _ = handle.drain_intents();
    assert!(
        handle.queue_bind_terminal(id, 0, TARGET_SPLIT_VERTICAL),
        "queue_bind_terminal of a live session into a vertical split must return true."
    );
    let new_win = wait_for_intent(
        &handle,
        INTENT_SPLIT,
        "a SPLIT intent from the queued bind; none means the target was not mapped to a split",
    );
    poll(
        &format!("the split window {new_win} to show session {id}"),
        || handle.win_buffer(new_win).terminal() == id,
    );
    wait_for_window_count(&handle, base + 1, "the queued bind's split");

    assert!(
        !handle.queue_bind_terminal(id, 0, TARGET_UNKNOWN),
        "queue_bind_terminal with an unknown target must return false."
    );
    assert!(
        !handle.queue_bind_terminal(UNKNOWN_ID, 0, TARGET_SPLIT_VERTICAL),
        "queue_bind_terminal of an id no session has must return false."
    );

    handle.shutdown();
    drop(dir);
}
