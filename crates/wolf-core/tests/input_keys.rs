//! Tests for input handling: cursor movement / mode change against a real nvim,
//! and a pure unit test of `key_to_notation` (which needs no nvim).

mod common;

use std::time::{Duration, Instant};

use common::{ch, key, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{key_to_notation, CursorShape, Key, KeyEvent, Snapshot, WolfCore};

/// A small multi-line markdown buffer for cursor-movement assertions.
const MULTILINE: &str = "alpha line one\nbravo line two\ncharlie line three\n";

/// How often [`poll_until`] re-reads the snapshot while waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Polls `observe` over the snapshot until `done` accepts the observed value, and returns it,
/// panicking with `desc` and the last value after [`FLUSH_TIMEOUT`]. A later frame, such as
/// treesitter's asynchronous parse finishing, can satisfy one `wait_for_flush` before the key's
/// frame lands, so a single read can measure the wrong frame. Polling the state also cannot miss
/// a frame that lands between a check and a new baseline. A copy of `redraw_grid.rs`'s
/// `poll_until`.
fn poll_until<T: std::fmt::Debug>(
    core: &WolfCore,
    waiter: &FlushWaiter,
    desc: &str,
    observe: impl Fn(&Snapshot) -> T,
    done: impl Fn(&T) -> bool,
) -> T {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        // One statement: the read guard drops before drain/sleep (see autoread.rs).
        let seen = observe(&core.read_snapshot());
        if done(&seen) {
            return seen;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for {desc}; last observed: {seen:?}");
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[test]
fn cursor_moves() {
    let (core, waiter, _file) = common::start_core_with_temp_md(MULTILINE);

    let start = core.read_snapshot().cursor;

    // From normal mode: down one line, then right one column.
    core.send_input(ch('j'));
    core.send_input(ch('l'));
    poll_until(
        &core,
        &waiter,
        &format!(
            "cursor.row should advance after 'j' and cursor.col after 'l' (start={start:?}); \
             never means a motion was not applied or its cursor_goto was not folded"
        ),
        |s| s.cursor,
        |c| c.row > start.row && c.col > start.col,
    );

    // Enter insert mode; the selected ModeInfo should reflect insert mode.
    core.send_input(ch('i'));
    // Accept either a mode name containing "insert" or a vertical-bar cursor, so the check does
    // not couple to nvim's mode_info_set ordering or labels.
    poll_until(
        &core,
        &waiter,
        "after 'i', the selected ModeInfo (name, shape) should denote insert mode; never means \
         the mode_change was not folded, or cursor.mode_idx indexes the wrong mode_infos entry",
        |s| {
            s.mode_infos
                .get(s.cursor.mode_idx)
                .map(|m| (m.name.clone(), m.shape))
        },
        |mode| {
            mode.as_ref().is_some_and(|(name, shape)| {
                name.to_lowercase().contains("insert") || matches!(shape, CursorShape::Vertical)
            })
        },
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn key_notation_unit() {
    // Pure translation asserts — no nvim involved.
    fn ev(key: Key, ctrl: bool, shift: bool, alt: bool, meta: bool) -> KeyEvent {
        KeyEvent {
            key,
            ctrl,
            shift,
            alt,
            meta,
        }
    }

    // Esc -> <Esc>
    assert_eq!(
        key_to_notation(&ev(Key::Esc, false, false, false, false)),
        "<Esc>"
    );

    // Ctrl+w -> <C-w>
    assert_eq!(
        key_to_notation(&ev(Key::Char('w'), true, false, false, false)),
        "<C-w>"
    );

    // Enter -> <CR>
    assert_eq!(
        key_to_notation(&ev(Key::Enter, false, false, false, false)),
        "<CR>"
    );

    // '<' -> <lt>
    assert_eq!(
        key_to_notation(&ev(Key::Char('<'), false, false, false, false)),
        "<lt>"
    );

    // Ctrl+Shift+Left -> <C-S-Left> (modifier order C-, S-, A-, D-)
    assert_eq!(
        key_to_notation(&ev(Key::Left, true, true, false, false)),
        "<C-S-Left>"
    );

    // plain 'a' -> "a"
    assert_eq!(
        key_to_notation(&ev(Key::Char('a'), false, false, false, false)),
        "a"
    );

    // Suppress unused warnings from the convenience constructors in this binary.
    let _ = (ch('a'), key(Key::Esc));
}

/// Function number used by the F-key notation assertion.
const FKEY_NUM: u8 = 5;

#[test]
fn key_notation_fkeys_and_mods() {
    // Pure translation asserts — no nvim involved.
    fn ev(key: Key, ctrl: bool, shift: bool, alt: bool, meta: bool) -> KeyEvent {
        KeyEvent {
            key,
            ctrl,
            shift,
            alt,
            meta,
        }
    }

    // Function key F5 -> <F5>.
    assert_eq!(
        key_to_notation(&ev(Key::F(FKEY_NUM), false, false, false, false)),
        "<F5>"
    );

    // Alt+x -> <A-x>.
    assert_eq!(
        key_to_notation(&ev(Key::Char('x'), false, false, true, false)),
        "<A-x>"
    );

    // Meta/super + s -> <D-s>; the modifier order is C-, S-, A-, D-.
    assert_eq!(
        key_to_notation(&ev(Key::Char('s'), false, false, false, true)),
        "<D-s>"
    );

    // Combined modifiers lock the ordering C-,S-,A-,D-: Ctrl+Alt+a -> <C-A-a>.
    assert_eq!(
        key_to_notation(&ev(Key::Char('a'), true, false, true, false)),
        "<C-A-a>"
    );
}
