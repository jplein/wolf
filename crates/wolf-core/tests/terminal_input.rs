//! Integration tests of input to `wolf-core`'s terminal sessions: the `<C-w>` prefix router,
//! keys, text, paste, mouse, scroll and focus, each read back as the bytes a real program in a
//! real pty received, beside a real, hermetic Neovim that holds the placeholder windows.
//!
//! Most cases run a byte-inspection session ([`common::cat_command`]): `/bin/sh` prints a
//! prelude that sets the modes under test, makes its tty raw without echo, prints
//! [`common::CAT_READY`] and becomes `cat -v`, so the frame shows each byte the session was
//! sent, ESC as `^[`, CR as `^M` and Ctrl+W as `^W`.
//!
//! ## Premises about the components
//!
//! Each is measured by these tests against the real component, not assumed:
//!
//! - Under `stty raw -echo`, `cat -v` prints each input byte as it arrives: BSD stdio flushes
//!   line-buffered stdout before it blocks on a tty read, and GNU `cat` writes before it
//!   blocks. A key whose bytes have not shown after a settle was not sent.
//! - `/bin/sh` with no argument on a tty is interactive and echoes what it is typed, so a test
//!   looks for the output of an expression (`$((1+1))`) that the typed text does not contain.
//! - The router's split runs the origin session's program and arguments again, so a split of
//!   an interactive `/bin/sh` is an interactive `/bin/sh`.
//! - `printf '\033]7;file://%s\007' /abs/dir` emits an `OSC 7` for `/abs/dir` with an empty
//!   host, which core decodes to `/abs/dir`.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{
    active_window, cat_output, frame_of, interactive_sh, open_cat, open_session, poll_frame,
    poll_until, run_line, settle_for, sh_script, start_core_at, term_ctrl, term_key, wait_for_text,
    win_terminal, FlushWaiter, FLUSH_TIMEOUT, POLL_INTERVAL,
};
use wolf_core::{
    FocusDir, Key, KeyEvent, LayoutIntent, MouseButton, ScrollDirection, SplitOrientation,
    TerminalId, TerminalMouseEvent, TerminalMouseKind, TerminalScrollEvent, TerminalTarget,
    WolfCore, TERMINAL_PREFIX_TIMEOUT,
};

/// How long to let bytes that should not arrive have their chance to.
const NEGATIVE_SETTLE: Duration = Duration::from_millis(500);

/// The cell width every mouse and scroll case gives the session, in pixels; [`size_for_mouse`]
/// passes it to `resize_terminal` as the integer 10.
const CELL_W: f32 = 10.0;
/// The cell height, as [`CELL_W`]; passed as the integer 20.
const CELL_H: f32 = 20.0;

/// The width the mouse and scroll cases resize to: not the default 80, so the frame shows
/// when the resize, and with it the cell size, has been applied.
const MOUSE_COLS: u16 = 81;

/// A vertical split of Neovim's current window.
const SPLIT_CURRENT: TerminalTarget = TerminalTarget::Split {
    win: 0,
    orientation: SplitOrientation::Vertical,
};

/// Starts a core in a fresh canonical cwd, returning it with its waiter and the cwd.
fn start_core() -> (WolfCore, FlushWaiter, std::path::PathBuf) {
    let cwd = std::fs::canonicalize(common::fresh_cwd()).expect("failed to canonicalize the cwd");
    let (core, waiter) = start_core_at(cwd.clone());
    (core, waiter, cwd)
}

/// Sends `<C-w>` then `key` to the session.
fn chord(core: &WolfCore, id: TerminalId, key: Key) {
    core.terminal_key(id, term_ctrl(Key::Char('w')));
    core.terminal_key(id, term_key(key));
}

/// Drains layout intents until `pick` accepts one, returning what it picked. Panics with
/// `desc` and every intent seen after [`FLUSH_TIMEOUT`].
fn wait_for_intent<T>(core: &WolfCore, desc: &str, pick: impl Fn(&LayoutIntent) -> Option<T>) -> T {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut seen = Vec::new();
    loop {
        for intent in core.drain_intents() {
            if let Some(picked) = pick(&intent) {
                return picked;
            }
            seen.push(intent);
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for {desc}; intents seen: {seen:?}");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Waits until a cat session shows some output, lets the rest arrive, and returns all of it.
fn cat_shows(core: &WolfCore, waiter: &FlushWaiter, id: TerminalId, desc: &str) -> String {
    poll_frame(core, waiter, id, desc, |frame| {
        !cat_output(frame).is_empty()
    });
    settle_for(waiter, NEGATIVE_SETTLE);
    cat_output(&frame_of(core, id))
}

/// A cat session's output after [`NEGATIVE_SETTLE`], for cases that expect nothing new.
fn cat_after_settle(core: &WolfCore, waiter: &FlushWaiter, id: TerminalId) -> String {
    settle_for(waiter, NEGATIVE_SETTLE);
    cat_output(&frame_of(core, id))
}

/// Resizes the session to [`MOUSE_COLS`] x 24 with [`CELL_W`] x [`CELL_H`] cells and waits for
/// the frame to show it.
fn size_for_mouse(core: &WolfCore, waiter: &FlushWaiter, id: TerminalId) {
    core.resize_terminal(id, MOUSE_COLS, 24, 10, 20);
    poll_frame(
        core,
        waiter,
        id,
        "the frame to take the mouse cases' width",
        |frame| frame.cols == MOUSE_COLS,
    );
}

/// A mouse event at the centre of cell `(col, row)`.
fn mouse_at(
    kind: TerminalMouseKind,
    button: Option<MouseButton>,
    col: u16,
    row: u16,
    shift: bool,
) -> TerminalMouseEvent {
    TerminalMouseEvent {
        kind,
        button,
        x: f32::from(col) * CELL_W + CELL_W / 2.0,
        y: f32::from(row) * CELL_H + CELL_H / 2.0,
        shift,
        ctrl: false,
        alt: false,
    }
}

/// A wheel scroll of `lines` at the centre of cell `(col, row)`.
fn scroll_at(direction: ScrollDirection, lines: u16, col: u16, row: u16) -> TerminalScrollEvent {
    TerminalScrollEvent {
        direction,
        lines,
        x: f32::from(col) * CELL_W + CELL_W / 2.0,
        y: f32::from(row) * CELL_H + CELL_H / 2.0,
        shift: false,
        ctrl: false,
        alt: false,
    }
}

/// The text of the first output row starting with `prefix`, polling until there is one.
fn output_row(core: &WolfCore, waiter: &FlushWaiter, id: TerminalId, prefix: &str) -> String {
    let frame = poll_frame(
        core,
        waiter,
        id,
        &format!("an output row starting with {prefix:?}"),
        |frame| (0..frame.rows).any(|row| frame.row_text(row).starts_with(prefix)),
    );
    (0..frame.rows)
        .map(|row| frame.row_text(row))
        .find(|text| text.starts_with(prefix))
        .expect("poll_frame returned only once such a row existed")
}

/// Splits the session with `<C-w>{key}` and checks the intent, the new session and its binding.
fn router_split_makes_a_new_session(key: char, orientation: SplitOrientation) {
    let (core, waiter, _cwd) = start_core();
    let win = active_window(&core, &waiter);
    let id = open_session(&core, TerminalTarget::Window(0), interactive_sh());
    let _ = core.drain_intents();

    chord(&core, id, Key::Char(key));
    let (got, from, new_win) = wait_for_intent(
        &core,
        &format!("a Split intent from <C-w>{key}; none means the chord went to the shell"),
        |intent| match intent {
            LayoutIntent::Split {
                orientation,
                from,
                win,
            } => Some((*orientation, *from, *win)),
            _ => None,
        },
    );
    assert_eq!(
        (got, from),
        (orientation, win),
        "<C-w>{key} must split the placeholder window {win} as {orientation:?}, not {got:?} from \
         {from}."
    );

    let live = poll_until(
        &waiter,
        "a second session in live_terminals after the router split",
        || core.live_terminals(),
        |live| live.len() == 2,
    );
    let new = *live
        .iter()
        .find(|other| **other != id)
        .expect("two live sessions, one of them new");
    poll_until(
        &waiter,
        "the split's new window to carry the new session",
        || win_terminal(&core, new_win),
        |bound| *bound == Some(new),
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `<C-w>s` splits the placeholder window horizontally and shows a new session in the new
/// window.
#[test]
fn ctrl_w_s_splits_horizontally_with_a_new_session() {
    router_split_makes_a_new_session('s', SplitOrientation::Horizontal);
}

/// `<C-w>v` splits the placeholder window vertically and shows a new session in the new window.
#[test]
fn ctrl_w_v_splits_vertically_with_a_new_session() {
    router_split_makes_a_new_session('v', SplitOrientation::Vertical);
}

/// `<C-w>c` and `<C-w>q` close the placeholder window of a two-window tabpage, and its session
/// leaves the registry.
#[test]
fn ctrl_w_c_and_q_close_the_pane_beside_another() {
    for key in ['c', 'q'] {
        let (core, waiter, _cwd) = start_core();
        let other = active_window(&core, &waiter);
        let id = open_session(&core, SPLIT_CURRENT, interactive_sh());
        let placeholder = poll_until(
            &waiter,
            "the split placeholder to become the active window",
            || core.read_snapshot().active_win,
            |active| active.is_some_and(|win| win != other),
        )
        .expect("poll_until returned only once active_win was set");

        chord(&core, id, Key::Char(key));
        poll_until(
            &waiter,
            &format!(
                "<C-w>{key} to close window {placeholder} and end its session; the window staying \
                 means the chord went to the shell"
            ),
            || {
                let snap = core.read_snapshot();
                (
                    snap.win_buffers.contains_key(&placeholder),
                    snap.win_buffers.contains_key(&other),
                    core.terminal_frame(id).is_some(),
                )
            },
            |(placeholder_left, other_left, session_left)| {
                !*placeholder_left && *other_left && !*session_left
            },
        );
        core.shutdown().expect("shutdown should be Ok");
    }
}

/// `<C-w>c` and `<C-w>q` in a tabpage's last window are refused, as `wolf_do_close` refuses
/// them: the window keeps its session and the session keeps running.
#[test]
fn ctrl_w_c_and_q_in_the_last_window_keep_the_pane() {
    let (core, waiter, _cwd) = start_core();
    let win = active_window(&core, &waiter);
    let id = open_session(&core, TerminalTarget::Window(0), interactive_sh());

    chord(&core, id, Key::Char('c'));
    chord(&core, id, Key::Char('q'));
    settle_for(&waiter, NEGATIVE_SETTLE * 3);
    assert_eq!(
        win_terminal(&core, win),
        Some(id),
        "a tabpage's last window must keep its session after <C-w>c and <C-w>q."
    );
    assert_eq!(
        core.live_terminals(),
        vec![id],
        "the session must keep running after a refused close; gone means it was hung up before \
         the close was attempted."
    );
    assert_eq!(
        frame_of(&core, id).exited,
        None,
        "the shell must not have exited after a refused close."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `<C-w>` followed by `h`, `j`, `k`, `l` or an arrow emits a `FocusDir` intent from the
/// placeholder window and sends the session nothing.
#[test]
fn focus_chords_emit_focus_dir_from_the_placeholder() {
    let (core, waiter, _cwd) = start_core();
    let win = active_window(&core, &waiter);
    let id = open_cat(&core, &waiter, TerminalTarget::Window(0), "");
    let _ = core.drain_intents();

    let chords = [
        (Key::Char('h'), FocusDir::Left),
        (Key::Char('j'), FocusDir::Down),
        (Key::Char('k'), FocusDir::Up),
        (Key::Char('l'), FocusDir::Right),
        (Key::Left, FocusDir::Left),
        (Key::Down, FocusDir::Down),
        (Key::Up, FocusDir::Up),
        (Key::Right, FocusDir::Right),
    ];
    for (key, want) in chords {
        chord(&core, id, key);
        let (dir, from) = wait_for_intent(
            &core,
            &format!(
                "a FocusDir intent from <C-w>{key:?}; none means the chord went to the session"
            ),
            |intent| match intent {
                LayoutIntent::FocusDir { dir, from } => Some((*dir, *from)),
                _ => None,
            },
        );
        assert_eq!(
            (dir, from),
            (want, win),
            "<C-w>{key:?} must emit FocusDir {want:?} from the placeholder window {win}, not \
             {dir:?} from {from}."
        );
    }

    let output = cat_after_settle(&core, &waiter, id);
    assert_eq!(
        output, "",
        "a focus chord must send the session nothing, but it received {output:?}."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `<C-w>` followed by a key that is no chord sends `^W` and then that key, in order.
#[test]
fn ctrl_w_then_another_key_sends_both_in_order() {
    let (core, waiter, _cwd) = start_core();
    let id = open_cat(&core, &waiter, TerminalTarget::Window(0), "");

    chord(&core, id, Key::Char('x'));
    let output = cat_shows(&core, &waiter, id, "the bytes of <C-w>x");
    assert_eq!(
        output, "^Wx",
        "<C-w>x must send ^W then x, not {output:?}: `x` alone means the held ^W was dropped, \
         x^W means it was sent after the key."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A lone `<C-w>` is held until `TERMINAL_PREFIX_TIMEOUT` passes, then sent.
#[test]
fn a_lone_ctrl_w_is_flushed_after_the_prefix_timeout() {
    let (core, waiter, _cwd) = start_core();
    let id = open_cat(&core, &waiter, TerminalTarget::Window(0), "");

    let sent = Instant::now();
    core.terminal_key(id, term_ctrl(Key::Char('w')));
    let held_until = sent + TERMINAL_PREFIX_TIMEOUT.mul_f64(0.8);
    while Instant::now() < held_until {
        let output = cat_output(&frame_of(&core, id));
        assert_eq!(
            output,
            "",
            "a lone <C-w> must be held for {TERMINAL_PREFIX_TIMEOUT:?}, but the session received \
             {output:?} after {:?}.",
            sent.elapsed()
        );
        std::thread::sleep(POLL_INTERVAL);
    }

    let deadline = sent + TERMINAL_PREFIX_TIMEOUT + Duration::from_secs(2);
    loop {
        let output = cat_output(&frame_of(&core, id));
        if !output.is_empty() {
            assert_eq!(
                output, "^W",
                "the flushed prefix must be ^W, not {output:?}."
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "a lone <C-w> was never sent, {:?} after it was pressed: the held ^W has no flush \
             deadline.",
            sent.elapsed()
        );
        std::thread::sleep(POLL_INTERVAL);
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// A `meta` chord is a front-end accelerator and never reaches the session.
#[test]
fn meta_chords_never_reach_the_session() {
    let (core, waiter, _cwd) = start_core();
    let id = open_cat(&core, &waiter, TerminalTarget::Window(0), "");

    core.terminal_key(
        id,
        KeyEvent {
            meta: true,
            ..term_key(Key::Char('a'))
        },
    );
    core.terminal_key(id, term_key(Key::Char('b')));
    let output = cat_shows(&core, &waiter, id, "the plain `b` after a meta chord");
    assert_eq!(
        output, "b",
        "meta+a then b must send only b, not {output:?}: an `a` means meta chords reach the \
         session."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Keys are encoded from the session's modes: Up follows cursor-key application mode, and
/// Ctrl+A is `^A`.
#[test]
fn keys_encode_from_the_sessions_modes() {
    let (core, waiter, _cwd) = start_core();
    let app = open_cat(&core, &waiter, TerminalTarget::Window(0), "\\033[?1h");
    let plain = open_cat(&core, &waiter, SPLIT_CURRENT, "");

    core.terminal_key(app, term_key(Key::Up));
    let output = cat_shows(&core, &waiter, app, "Up in cursor-key application mode");
    assert_eq!(
        output, "^[OA",
        "Up after CSI ? 1 h must send SS3 A, not {output:?}: CSI A means the key encoder is not \
         configured from the session's modes."
    );

    core.terminal_key(plain, term_key(Key::Up));
    core.terminal_key(plain, term_ctrl(Key::Char('a')));
    let output = cat_shows(&core, &waiter, plain, "Up and Ctrl+A in normal mode");
    assert_eq!(
        output, "^[[A^A",
        "Up then Ctrl+A in normal mode must send CSI A then 0x01, not {output:?}."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A paste is bracketed when the program set mode 2004, and plain otherwise.
#[test]
fn paste_is_bracketed_when_the_program_asks() {
    let (core, waiter, _cwd) = start_core();
    let bracketed = open_cat(&core, &waiter, TerminalTarget::Window(0), "\\033[?2004h");
    let plain = open_cat(&core, &waiter, SPLIT_CURRENT, "");

    core.terminal_paste(bracketed, "ab");
    let output = cat_shows(&core, &waiter, bracketed, "a bracketed paste");
    assert_eq!(
        output, "^[[200~ab^[[201~",
        "a paste with mode 2004 set must be bracketed, not {output:?}."
    );

    core.terminal_paste(plain, "ab");
    let output = cat_shows(&core, &waiter, plain, "a plain paste");
    assert_eq!(
        output, "ab",
        "a paste with mode 2004 unset must arrive as the text alone, not {output:?}."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// With mouse tracking on, a press is encoded at the cell the position falls in, while a
/// shifted drag bypasses tracking and selects.
#[test]
fn mouse_events_report_with_tracking_and_shift_selects() {
    let (core, waiter, _cwd) = start_core();
    let id = open_cat(
        &core,
        &waiter,
        TerminalTarget::Window(0),
        "\\033[?1000h\\033[?1006h",
    );
    size_for_mouse(&core, &waiter, id);

    let press = mouse_at(
        TerminalMouseKind::Press,
        Some(MouseButton::Left),
        2,
        3,
        false,
    );
    core.terminal_mouse(id, press);
    let output = cat_shows(&core, &waiter, id, "the report of a left press");
    assert_eq!(
        output, "^[[<0;3;4M",
        "a left press at cell (2, 3) with tracking on must be reported as CSI < 0;3;4 M, not \
         {output:?}."
    );

    for (kind, col) in [
        (TerminalMouseKind::Press, 1),
        (TerminalMouseKind::Motion, 4),
        (TerminalMouseKind::Release, 4),
    ] {
        core.terminal_mouse(id, mouse_at(kind, Some(MouseButton::Left), col, 0, true));
    }
    let after = cat_after_settle(&core, &waiter, id);
    assert_eq!(
        after, output,
        "a shifted drag must bypass tracking and send nothing, but the session received {after:?} \
         after the first report."
    );
    let frame = poll_frame(
        &core,
        &waiter,
        id,
        "the shifted drag's selection",
        |frame| frame.selection.first().copied().flatten().is_some(),
    );
    assert_eq!(
        frame.selection[0],
        Some((1, 4)),
        "a shifted drag from cell (1, 0) to (4, 0) must select columns 1 to 4 of row 0."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// With tracking off, a drag selects and its text can be copied, and a later click clears it.
#[test]
fn a_drag_selects_without_tracking_and_a_click_clears() {
    let (core, waiter, _cwd) = start_core();
    let id = open_session(
        &core,
        TerminalTarget::Window(0),
        sh_script("printf 'hello world'; exec sleep 1000"),
    );
    wait_for_text(&core, &waiter, id, "hello world");
    size_for_mouse(&core, &waiter, id);

    let left = Some(MouseButton::Left);
    core.terminal_mouse(id, mouse_at(TerminalMouseKind::Press, left, 6, 0, false));
    core.terminal_mouse(id, mouse_at(TerminalMouseKind::Motion, left, 10, 0, false));
    core.terminal_mouse(id, mouse_at(TerminalMouseKind::Release, left, 10, 0, false));
    let frame = poll_frame(
        &core,
        &waiter,
        id,
        "a selection on row 0 after a drag; none means motion does not select",
        |frame| frame.selection.first().copied().flatten().is_some(),
    );
    assert_eq!(
        frame.selection[0],
        Some((6, 10)),
        "a drag from cell (6, 0) to (10, 0) must select columns 6 to 10 of row 0."
    );
    assert_eq!(
        core.terminal_selection_text(id).as_deref(),
        Some("world"),
        "the selection over `world` must copy as \"world\"."
    );

    core.terminal_mouse(id, mouse_at(TerminalMouseKind::Press, left, 0, 2, false));
    core.terminal_mouse(id, mouse_at(TerminalMouseKind::Release, left, 0, 2, false));
    poll_frame(
        &core,
        &waiter,
        id,
        "the selection to clear after a click; still set means a press does not clear it",
        |frame| frame.selection.iter().all(Option::is_none),
    );
    assert_ne!(
        core.terminal_selection_text(id).as_deref(),
        Some("world"),
        "a click must clear the selection, so `world` can no longer be copied."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// On the primary screen with tracking off, the wheel scrolls the viewport through
/// scrollback, and typed text snaps it back to the bottom.
#[test]
fn the_wheel_scrolls_scrollback_and_typing_snaps_back() {
    let (core, waiter, _cwd) = start_core();
    let id = open_session(
        &core,
        TerminalTarget::Window(0),
        sh_script("i=1; while [ $i -le 100 ]; do echo L$i; i=$((i+1)); done; exec sleep 1000"),
    );
    let bottom = poll_frame(&core, &waiter, id, "L100 on screen", |frame| {
        (0..frame.rows).any(|row| frame.row_text(row) == "L100")
    });
    let top_line: u32 = bottom
        .row_text(0)
        .strip_prefix('L')
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "row 0 must hold one of the numbered lines, not {:?}.",
                bottom.row_text(0)
            )
        });
    let at_bottom = bottom.scrollbar.total - bottom.scrollbar.len;
    assert_eq!(
        bottom.scrollbar.offset, at_bottom,
        "before any scroll the viewport must be at the bottom."
    );

    core.terminal_scroll(id, scroll_at(ScrollDirection::Up, 5, 1, 1));
    let scrolled = poll_frame(
        &core,
        &waiter,
        id,
        "the viewport to scroll up 5 rows; unchanged means the wheel does not scroll scrollback",
        |frame| frame.scrollbar.offset != at_bottom,
    );
    assert_eq!(
        (scrolled.scrollbar.offset, scrolled.row_text(0)),
        (at_bottom - 5, format!("L{}", top_line - 5)),
        "a wheel scroll up of 5 lines must lower the offset by 5 and show the line 5 above."
    );

    core.terminal_text(id, "x");
    poll_frame(
        &core,
        &waiter,
        id,
        "typed text to snap the viewport back to the bottom",
        |frame| frame.scrollbar.offset == frame.scrollbar.total - frame.scrollbar.len,
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// On the alternate screen with mode 1007 and tracking off, the wheel sends arrow keys.
#[test]
fn the_wheel_sends_arrows_on_the_alternate_screen_with_1007() {
    let (core, waiter, _cwd) = start_core();
    let id = open_cat(
        &core,
        &waiter,
        TerminalTarget::Window(0),
        "\\033[?1049h\\033[?1007h",
    );

    core.terminal_scroll(id, scroll_at(ScrollDirection::Up, 2, 1, 1));
    let output = cat_shows(&core, &waiter, id, "arrows from a wheel scroll");
    assert_eq!(
        output, "^[[A^[[A",
        "a wheel scroll up of 2 on the alternate screen with mode 1007 must send Up twice, not \
         {output:?}."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// With mouse tracking on, the wheel is reported to the program.
#[test]
fn the_wheel_is_reported_with_tracking() {
    let (core, waiter, _cwd) = start_core();
    let id = open_cat(
        &core,
        &waiter,
        TerminalTarget::Window(0),
        "\\033[?1000h\\033[?1006h",
    );
    size_for_mouse(&core, &waiter, id);

    core.terminal_scroll(id, scroll_at(ScrollDirection::Up, 1, 2, 3));
    let output = cat_shows(&core, &waiter, id, "the report of a wheel scroll");
    assert_eq!(
        output, "^[[<64;3;4M",
        "a wheel scroll up at cell (2, 3) with tracking on must be reported as CSI < 64;3;4 M, \
         not {output:?}."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Focus changes are reported to a program that set mode 1004, and to no other.
#[test]
fn focus_reports_follow_mode_1004() {
    let (core, waiter, _cwd) = start_core();
    let reporting = open_cat(&core, &waiter, TerminalTarget::Window(0), "\\033[?1004h");
    let plain = open_cat(&core, &waiter, SPLIT_CURRENT, "");

    core.terminal_focus(reporting, true);
    core.terminal_focus(reporting, false);
    let output = cat_shows(&core, &waiter, reporting, "focus reports");
    assert_eq!(
        output, "^[[I^[[O",
        "focus gained then lost with mode 1004 set must send CSI I then CSI O, not {output:?}."
    );

    core.terminal_focus(plain, true);
    core.terminal_focus(plain, false);
    let output = cat_after_settle(&core, &waiter, plain);
    assert_eq!(
        output, "",
        "a program that did not set mode 1004 must get no focus report, but got {output:?}."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Splits `origin` with `<C-w>v` and returns the new session once it is live.
fn split_session(core: &WolfCore, waiter: &FlushWaiter, origin: TerminalId) -> TerminalId {
    chord(core, origin, Key::Char('v'));
    let live = poll_until(
        waiter,
        "a second session after <C-w>v",
        || core.live_terminals(),
        |live| live.len() == 2,
    );
    *live
        .iter()
        .find(|id| **id != origin)
        .expect("two live sessions, one of them new")
}

/// The basename of the directory a session's interactive shell is in, as `at:<name>`.
fn shell_dir_name(core: &WolfCore, waiter: &FlushWaiter, id: TerminalId) -> String {
    run_line(core, id, "echo \"at:$(basename \"$(pwd -P)\")\"");
    output_row(core, waiter, id, "at:")
}

/// A router split of an interactive shell starts in the directory the origin shell is in: the
/// directory it reported with `OSC 7` when there is one, else its process cwd.
#[test]
fn a_router_split_starts_where_the_origin_shell_is() {
    let (core, waiter, cwd) = start_core();
    std::fs::create_dir(cwd.join("split-origin-cd")).expect("failed to create split-origin-cd");
    let origin = open_session(&core, TerminalTarget::Window(0), interactive_sh());
    run_line(&core, origin, "cd split-origin-cd && echo moved-$((1+1))");
    wait_for_text(&core, &waiter, origin, "moved-2");

    let split = split_session(&core, &waiter, origin);
    let at = shell_dir_name(&core, &waiter, split);
    assert_eq!(
        at, "at:split-origin-cd",
        "a split of a shell that cd'd into split-origin-cd must start there, not in {at:?}: the \
         core cwd's name means the shell's process cwd is not read."
    );
    assert_eq!(
        frame_of(&core, split).shell,
        frame_of(&core, origin).shell,
        "the split must run the origin's program."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A shell's `OSC 7` report wins over its process cwd for a router split.
#[test]
fn a_router_split_prefers_the_reported_pwd() {
    let (core, waiter, cwd) = start_core();
    let reported = cwd.join("osc-reported");
    std::fs::create_dir(cwd.join("cd-elsewhere")).expect("failed to create cd-elsewhere");
    std::fs::create_dir(&reported).expect("failed to create osc-reported");
    let origin = open_session(&core, TerminalTarget::Window(0), interactive_sh());
    run_line(
        &core,
        origin,
        &format!(
            "cd cd-elsewhere && printf '\\033]7;file://%s\\007' '{}' && echo moved-$((2+2))",
            path_text(&reported)
        ),
    );
    wait_for_text(&core, &waiter, origin, "moved-4");

    let split = split_session(&core, &waiter, origin);
    let at = shell_dir_name(&core, &waiter, split);
    assert_eq!(
        at, "at:osc-reported",
        "the origin reported osc-reported with OSC 7 while its process sat in cd-elsewhere, so \
         the split must start in osc-reported, not {at:?}: at:cd-elsewhere means the OSC 7 pwd \
         is ignored."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `path` as UTF-8, failing on a temp path that is not.
fn path_text(path: &Path) -> &str {
    path.to_str()
        .unwrap_or_else(|| panic!("the temp path {} is not UTF-8", path.display()))
}
