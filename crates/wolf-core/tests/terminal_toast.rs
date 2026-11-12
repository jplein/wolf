//! Integration tests of the message toast against terminal panes in `wolf-core`: a non-prompt
//! toast is dismissed by input sent to a terminal session (a key, text, a paste, a mouse press
//! or a scroll), by the drawer toggle, and by moving focus into a terminal pane, while a
//! `confirm()` prompt survives all of them. A real, hermetic Neovim shows the toast and real
//! `/bin/sh` sessions run in ptys beside it.
//!
//! "Dismissed" means `Snapshot.toast` is hidden and holds no message, polled to
//! [`common::FLUSH_TIMEOUT`] and still so after a settle, so a toast hidden and then shown again
//! does not pass.
//!
//! ## Premises about the components
//!
//! Each is measured by these tests against the real component, not assumed:
//!
//! - An `:echo` under `ext_messages` reaches `Snapshot.toast` as a visible message, and nothing
//!   but input clears it within a settle. [`show_toast`] checks the second half before every
//!   dismissing input, so a toast that would have gone anyway cannot pass for a dismissal.
//! - `nvim_set_current_win` into an editor window sends no `msg_clear`, so a `FocusWin` into an
//!   editor pane leaves the toast up: the first leg of the focus case measures it.
//! - A right-button press over an idle `/bin/sh` with mouse tracking off changes no frame,
//!   because the session's selection ignores every button but the left, so it publishes
//!   nothing and fires no wake by itself: the first leg of the wake case measures it.
//! - `confirm()` blocks Neovim until it is answered, and input to a terminal session never
//!   reaches it. A `ToggleTerminal` queued behind the dialog waits for it, and the `<Esc>` each
//!   such case ends with is released past it and aborts the dialog.
//! - `/bin/sh` on a tty is interactive and echoes what it is typed, so a case looks for the
//!   output of an expression (`$((2+3))`) the typed text does not contain.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::{
    active_window, interactive_sh, key, open_session, poll_until, run_line, send_ex, settle_for,
    start_core_at, start_core_recording, term_key, terminal_windows, wait_for_text, FlushWaiter,
};
use wolf_core::{
    Key, MouseButton, ScrollDirection, SplitOrientation, TerminalId, TerminalMouseEvent,
    TerminalMouseKind, TerminalScrollEvent, TerminalTarget, WolfCore,
};

/// How long an input that must not dismiss the toast is given to dismiss it anyway, and how
/// long a dismissed toast is watched for coming back. Widen it if a negative leg flakes.
const NEGATIVE_SETTLE: Duration = Duration::from_millis(1500);

/// How long a freshly shown toast is watched before the input under test, to show that nothing
/// else dismisses it.
const CONTROL_SETTLE: Duration = Duration::from_millis(500);

/// How long Neovim and the shell are given to go quiet before wakes are counted.
const QUIET_SETTLE: Duration = Duration::from_millis(500);

/// A vertical split of Neovim's current window.
const SPLIT_CURRENT: TerminalTarget = TerminalTarget::Split {
    win: 0,
    orientation: SplitOrientation::Vertical,
};

/// The line [`open_shell`] runs, and what it prints. The typed line does not hold the output.
const READY_LINE: &str = "echo ready-$((2+3))";
const READY_OUTPUT: &str = "ready-5";

/// The `confirm()` call the prompt case runs: the `&yes`/`&no` accelerators make `y` and `n`
/// answers, and nothing the case sends to Neovim before its `<Esc>` is either.
const CONFIRM_CALL: &str = r#"call confirm("Keep the session going?", "&yes\n&no")"#;

/// The canonical fresh cwd every case starts its core in, with `SHELL` pinned to `/bin/sh`
/// first so the drawer runs a known shell.
fn fresh_canonical_cwd() -> PathBuf {
    common::pin_default_shell();
    std::fs::canonicalize(common::fresh_cwd()).expect("failed to canonicalize the cwd")
}

/// Starts a core in a fresh canonical cwd with `/bin/sh` as its default shell.
fn start_core() -> (WolfCore, FlushWaiter) {
    start_core_at(fresh_canonical_cwd())
}

/// Opens an interactive `/bin/sh` in a vertical split of Neovim's current window and waits for
/// it to run a line, so it is idle at a prompt when this returns.
fn open_shell(core: &WolfCore, waiter: &FlushWaiter) -> TerminalId {
    let id = open_session(core, SPLIT_CURRENT, interactive_sh());
    run_line(core, id, READY_LINE);
    wait_for_text(core, waiter, id, READY_OUTPUT);
    id
}

/// The toast's lines, each the concatenation of its chunks, joined with newlines.
fn toast_text(core: &WolfCore) -> String {
    core.read_snapshot()
        .toast
        .lines()
        .iter()
        .map(|line| line.iter().map(|c| c.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a case reads of the toast: whether it shows, how many messages it holds, its last
/// message's kind and its text. Owned, so no read guard outlives the read.
#[derive(Debug)]
struct ToastSeen {
    visible: bool,
    messages: usize,
    kind: String,
    text: String,
}

impl ToastSeen {
    fn read(core: &WolfCore) -> Self {
        let (visible, messages, kind) = {
            let snap = core.read_snapshot();
            (
                snap.toast.visible,
                snap.toast.messages.len(),
                snap.toast.kind(),
            )
        };
        Self {
            visible,
            messages,
            kind,
            text: toast_text(core),
        }
    }

    fn dismissed(&self) -> bool {
        !self.visible && self.messages == 0
    }

    fn shows(&self, marker: &str) -> bool {
        self.visible && self.text.contains(marker)
    }

    fn is_prompt(&self) -> bool {
        self.visible && self.kind == "confirm"
    }
}

/// Echoes `marker` in Neovim, waits for the toast to show it, then watches it for
/// [`CONTROL_SETTLE`] and fails if anything but the case's own input took it down.
fn show_toast(core: &WolfCore, waiter: &FlushWaiter, marker: &str) {
    send_ex(core, &format!("echo \"{marker}\""));
    poll_until(
        waiter,
        &format!("premise: the :echo to show {marker:?} in Snapshot.toast"),
        || ToastSeen::read(core),
        |seen| seen.shows(marker),
    );
    settle_for(waiter, CONTROL_SETTLE);
    let seen = ToastSeen::read(core);
    assert!(
        seen.shows(marker),
        "premise: the toast showing {marker:?} must stay up with no further input, but after \
         {CONTROL_SETTLE:?} it reads {seen:?}. Something other than the input under test clears \
         it, so a dismissal this case observes would prove nothing."
    );
}

/// Polls until the toast is dismissed, then checks it stays so for [`NEGATIVE_SETTLE`].
/// `cause` names the input and what a timeout means.
fn assert_dismissed(core: &WolfCore, waiter: &FlushWaiter, cause: &str) {
    poll_until(
        waiter,
        &format!("the toast to be dismissed (hidden, no messages) after {cause}"),
        || ToastSeen::read(core),
        ToastSeen::dismissed,
    );
    settle_for(waiter, NEGATIVE_SETTLE);
    let seen = ToastSeen::read(core);
    assert!(
        seen.dismissed(),
        "the toast was dismissed after {cause} and then came back within {NEGATIVE_SETTLE:?}: \
         {seen:?}. Something showed it again, so the dismissal does not hold."
    );
}

/// Asserts the toast still shows `marker` after [`NEGATIVE_SETTLE`]; `input` names what was
/// sent and must not have dismissed it.
fn assert_still_shows(core: &WolfCore, waiter: &FlushWaiter, marker: &str, input: &str) {
    settle_for(waiter, NEGATIVE_SETTLE);
    let seen = ToastSeen::read(core);
    assert!(
        seen.shows(marker),
        "{input} must not dismiss the toast, but {NEGATIVE_SETTLE:?} later it reads {seen:?} \
         instead of showing {marker:?}."
    );
}

/// A mouse event at the pixel `(5, 5)`, inside the session's first cell at any cell size.
fn mouse(kind: TerminalMouseKind, button: Option<MouseButton>) -> TerminalMouseEvent {
    TerminalMouseEvent {
        kind,
        button,
        x: 5.0,
        y: 5.0,
        shift: false,
        ctrl: false,
        alt: false,
    }
}

/// One wheel line up at the pixel `(5, 5)`.
fn scroll_up() -> TerminalScrollEvent {
    TerminalScrollEvent {
        direction: ScrollDirection::Up,
        lines: 1,
        x: 5.0,
        y: 5.0,
        shift: false,
        ctrl: false,
        alt: false,
    }
}

/// The window flagged `is_drawer` in `win_buffers`, with the session it shows.
fn drawer_window(core: &WolfCore) -> Option<(i64, Option<TerminalId>)> {
    core.read_snapshot()
        .win_buffers
        .iter()
        .find(|(_, buffer)| buffer.is_drawer)
        .map(|(win, buffer)| (*win, buffer.terminal))
}

/// Opening the drawer with `toggle_terminal` dismisses a showing toast.
#[test]
fn opening_the_drawer_dismisses_a_toast() {
    const MARKER: &str = "drawer toast quebec";
    let (core, waiter) = start_core();
    show_toast(&core, &waiter, MARKER);

    core.toggle_terminal();
    poll_until(
        &waiter,
        "premise: a drawer window carrying a session after toggle_terminal; without one the \
         toggle never ran and the toast says nothing about it",
        || drawer_window(&core),
        |drawer| matches!(drawer, Some((_, Some(_)))),
    );
    assert_dismissed(
        &core,
        &waiter,
        "toggle_terminal opened the drawer. A timeout with the drawer open means a \
         ToggleTerminal does not dismiss the toast, which then covers the terminal",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A key sent to a terminal session dismisses a showing toast.
#[test]
fn a_key_sent_to_a_session_dismisses_a_toast() {
    const MARKER: &str = "key toast romeo";
    let (core, waiter) = start_core();
    let id = open_shell(&core, &waiter);
    show_toast(&core, &waiter, MARKER);

    core.terminal_key(id, term_key(Key::Char('x')));
    assert_dismissed(
        &core,
        &waiter,
        "terminal_key sent `x` to the session. A timeout means a key to a terminal session \
         never dismisses the toast",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Text sent to a terminal session dismisses a showing toast.
#[test]
fn text_sent_to_a_session_dismisses_a_toast() {
    const MARKER: &str = "text toast sierra";
    let (core, waiter) = start_core();
    let id = open_shell(&core, &waiter);
    show_toast(&core, &waiter, MARKER);

    core.terminal_text(id, "y");
    assert_dismissed(
        &core,
        &waiter,
        "terminal_text sent `y` to the session. A timeout means text to a terminal session \
         never dismisses the toast",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A paste into a terminal session dismisses a showing toast.
#[test]
fn a_paste_into_a_session_dismisses_a_toast() {
    const MARKER: &str = "paste toast tango";
    let (core, waiter) = start_core();
    let id = open_shell(&core, &waiter);
    show_toast(&core, &waiter, MARKER);

    core.terminal_paste(id, "z");
    assert_dismissed(
        &core,
        &waiter,
        "terminal_paste pasted `z` into the session. A timeout means a paste into a terminal \
         session never dismisses the toast",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A mouse press over a terminal session dismisses a showing toast, while a motion and a
/// release do not.
#[test]
fn a_mouse_press_over_a_session_dismisses_a_toast_and_motion_does_not() {
    const MARKER: &str = "mouse toast uniform";
    let (core, waiter) = start_core();
    let id = open_shell(&core, &waiter);
    show_toast(&core, &waiter, MARKER);

    core.terminal_mouse(id, mouse(TerminalMouseKind::Motion, None));
    core.terminal_mouse(
        id,
        mouse(TerminalMouseKind::Release, Some(MouseButton::Left)),
    );
    assert_still_shows(
        &core,
        &waiter,
        MARKER,
        "A pointer motion and a button release over the session (no press). Gone means every \
         mouse event dismisses rather than a press alone",
    );

    core.terminal_mouse(id, mouse(TerminalMouseKind::Press, Some(MouseButton::Left)));
    assert_dismissed(
        &core,
        &waiter,
        "terminal_mouse pressed the left button over the session. A timeout means a mouse press \
         on a terminal session never dismisses the toast",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A wheel scroll over a terminal session dismisses a showing toast.
#[test]
fn a_scroll_over_a_session_dismisses_a_toast() {
    const MARKER: &str = "scroll toast victor";
    let (core, waiter) = start_core();
    let id = open_shell(&core, &waiter);
    show_toast(&core, &waiter, MARKER);

    core.terminal_scroll(id, scroll_up());
    assert_dismissed(
        &core,
        &waiter,
        "terminal_scroll scrolled the session up a line. A timeout means a scroll over a \
         terminal session never dismisses the toast",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A `FocusWin` into a terminal pane dismisses a showing toast, while one into an editor pane
/// does not.
#[test]
fn focusing_a_terminal_pane_dismisses_a_toast_and_focusing_an_editor_pane_does_not() {
    const MARKER: &str = "focus toast whiskey";
    let (core, waiter) = start_core();
    let editor_win = active_window(&core, &waiter);
    let id = open_shell(&core, &waiter);
    let term_win = terminal_windows(&core)
        .into_iter()
        .find(|(_, session)| *session == id)
        .map(|(win, _)| win)
        .expect("open_terminal returns once win_buffers binds the session's window");
    assert_ne!(
        term_win, editor_win,
        "premise: the split must give the session a window of its own, not the editor's"
    );

    core.focus_win(term_win);
    poll_until(
        &waiter,
        "premise: the terminal window to become active before the toast shows",
        || core.read_snapshot().active_win,
        |active| *active == Some(term_win),
    );
    show_toast(&core, &waiter, MARKER);

    core.focus_win(editor_win);
    poll_until(
        &waiter,
        "the editor window to become active after focus_win",
        || core.read_snapshot().active_win,
        |active| *active == Some(editor_win),
    );
    assert_still_shows(
        &core,
        &waiter,
        MARKER,
        "A FocusWin into the editor pane. Gone means every FocusWin dismisses, not only one \
         into a terminal pane, or Neovim cleared the message on the window change",
    );

    core.focus_win(term_win);
    assert_dismissed(
        &core,
        &waiter,
        "focus_win moved focus into the terminal pane. A timeout means a FocusWin into a \
         terminal pane never dismisses the toast",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A `confirm()` prompt toast survives every kind of terminal input and the drawer toggle.
#[test]
fn a_confirm_prompt_survives_terminal_input_and_the_drawer_toggle() {
    let (core, waiter) = start_core();
    let id = open_shell(&core, &waiter);

    send_ex(&core, CONFIRM_CALL);
    poll_until(
        &waiter,
        "premise: the confirm() dialog to show as a prompt toast (kind `confirm`)",
        || ToastSeen::read(&core),
        ToastSeen::is_prompt,
    );

    core.terminal_key(id, term_key(Key::Char('x')));
    core.terminal_text(id, "y");
    core.terminal_paste(id, "z");
    core.terminal_mouse(id, mouse(TerminalMouseKind::Press, Some(MouseButton::Left)));
    core.terminal_scroll(id, scroll_up());
    core.toggle_terminal();

    settle_for(&waiter, NEGATIVE_SETTLE);
    let seen = ToastSeen::read(&core);
    assert!(
        seen.is_prompt(),
        "a confirm() prompt must survive a key, text, a paste, a press and a scroll sent to a \
         terminal session and a ToggleTerminal, none of which answers it; it reads {seen:?}. \
         Hidden means one of them dismissed the prompt as if it were a plain message; another \
         kind means something replaced it."
    );

    // Aborts the dialog, which the queued ToggleTerminal waits on, so Neovim can shut down.
    core.send_input(key(Key::Esc));
    core.shutdown().expect("shutdown should be Ok");
}

/// A dismissal by terminal input fires the core's wake without bumping `seq`, and terminal
/// input with no toast showing fires none.
#[test]
fn a_dismissal_by_terminal_input_wakes_the_front_end_and_none_without_a_toast() {
    const MARKER: &str = "wake toast xray";
    let (core, waiter, wakes) = start_core_recording(fresh_canonical_cwd());
    send_ex(&core, "set updatetime=600000");
    let id = open_shell(&core, &waiter);
    settle_for(&waiter, QUIET_SETTLE);
    let right_press = mouse(TerminalMouseKind::Press, Some(MouseButton::Right));

    let before = ToastSeen::read(&core);
    assert!(
        before.dismissed(),
        "premise: no toast may show before the first leg, but it reads {before:?}"
    );
    wakes.clear();
    core.terminal_mouse(id, right_press);
    settle_for(&waiter, QUIET_SETTLE);
    let quiet = wakes.instants();
    assert!(
        quiet.is_empty(),
        "a right press over an idle shell with no toast showing must fire no wake, but {} \
         fired. Either the dismissal wakes when it hid nothing, or the press publishes a frame \
         by itself and another input must be chosen for this case.",
        quiet.len()
    );

    show_toast(&core, &waiter, MARKER);
    settle_for(&waiter, QUIET_SETTLE);
    let seq_before = core.read_snapshot().seq;
    wakes.clear();
    core.terminal_mouse(id, right_press);
    poll_until(
        &waiter,
        "the toast to be dismissed by a right press over the session",
        || ToastSeen::read(&core),
        ToastSeen::dismissed,
    );
    assert!(
        !wakes.instants().is_empty(),
        "dismissing the toast must fire the core's wake, since no redraw flush need follow; \
         none fired, so a front-end would keep painting the toast until something else woke it."
    );
    assert_eq!(
        core.read_snapshot().seq,
        seq_before,
        "a dismissal by terminal input must not bump Snapshot.seq, which only a redraw flush \
         may; it moved from {seq_before}."
    );

    core.shutdown().expect("shutdown should be Ok");
}
