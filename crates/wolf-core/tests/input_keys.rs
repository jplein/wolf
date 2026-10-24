//! Tests for input handling: cursor movement / mode change against a real nvim,
//! and a pure unit test of `key_to_notation` (which needs no nvim).

mod common;

use common::{ch, key, FLUSH_TIMEOUT};
use wolf_core::{key_to_notation, CursorShape, Key, KeyEvent};

/// A small multi-line markdown buffer for cursor-movement assertions.
const MULTILINE: &str = "alpha line one\nbravo line two\ncharlie line three\n";

#[test]
fn cursor_moves() {
    let (core, waiter, _file) = common::start_core_with_temp_md(MULTILINE);

    let start = core.read_snapshot().cursor;

    // From normal mode: down one line, then right one column.
    waiter.drain();
    core.send_input(ch('j'));
    core.send_input(ch('l'));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    let moved = core.read_snapshot().cursor;
    assert!(
        moved.row > start.row,
        "cursor.row should advance after 'j' (start={}, moved={})",
        start.row,
        moved.row
    );
    assert!(
        moved.col > start.col,
        "cursor.col should advance after 'l' (start={}, moved={})",
        start.col,
        moved.col
    );

    // Enter insert mode; the selected ModeInfo should reflect insert mode.
    waiter.drain();
    core.send_input(ch('i'));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    let snap = core.read_snapshot();
    let mode = snap
        .mode_infos
        .get(snap.cursor.mode_idx)
        .expect("cursor.mode_idx should index into mode_infos");

    // Accept either a mode name containing "insert" or a vertical-bar cursor, so the check does
    // not couple to nvim's mode_info_set ordering or labels.
    let name_says_insert = mode.name.to_lowercase().contains("insert");
    let shape_says_insert = matches!(mode.shape, CursorShape::Vertical);
    assert!(
        name_says_insert || shape_says_insert,
        "after 'i', the selected ModeInfo should denote insert mode \
         (name={:?}, shape={:?})",
        mode.name,
        mode.shape
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
