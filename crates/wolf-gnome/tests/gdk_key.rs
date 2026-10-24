//! Table-driven unit tests for `wolf_gnome::gdk_key`: GDK keyval + modifier state
//! -> `Option<wolf_core::KeyEvent>`.
//!
//! These run headlessly: `gdk::Key` constants and the `key_from_char` helper
//! (built on `gdk::unicode_to_keyval`) are compile-time / pure (no display
//! server, no nvim).
//!
//! Layout independence: we assert ONLY on letter casing and named keys. We do
//! NOT assert shifted-punctuation unicode (e.g. that Shift+`,` yields `<`), which
//! is keyboard-layout dependent and would be flaky. Printable characters are
//! built via `key_from_char(c)`, so the keyval already carries the exact glyph
//! regardless of layout.
//!
//! This test uses gdk4's associated `gdk::Key` constants (e.g. `gdk::Key::Return`)
//! and the `ModifierType` mask constants. If the pinned gtk-rs minor names a
//! constant differently, prefer `gdk::Key::from_name("...")` over inventing new
//! API — do not change `gdk_key`'s contract to fit the test.

use gdk::glib::translate::from_glib;
use gdk::ModifierType;

use wolf_core::{Key, KeyEvent};
use wolf_gnome::gdk_key;

fn key_from_char(c: char) -> gdk::Key {
    // gdk4-rs has no Key::from_unicode; construct the keyval exactly as GDK would
    // from a Unicode scalar, mirroring how a real keypress arrives.
    unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) }
}

/// F-key numbers asserted below (named to satisfy the "no magic numbers" rule).
const F1_NUM: u8 = 1;
const F5_NUM: u8 = 5;
const F12_NUM: u8 = 12;

/// Convenience constructor for an expected `KeyEvent`.
fn ke(key: Key, ctrl: bool, shift: bool, alt: bool, meta: bool) -> Option<KeyEvent> {
    Some(KeyEvent {
        key,
        ctrl,
        shift,
        alt,
        meta,
    })
}

#[test]
fn gdk_key_table() {
    let none = ModifierType::empty();
    let shift = ModifierType::SHIFT_MASK;
    let ctrl = ModifierType::CONTROL_MASK;
    let alt = ModifierType::ALT_MASK;
    let sup = ModifierType::SUPER_MASK;

    // (description, keyval, modifier state, expected translation)
    let cases: Vec<(&str, gdk::Key, ModifierType, Option<KeyEvent>)> = vec![
        // --- Enter / Return family ---
        (
            "Return -> Enter",
            gdk::Key::Return,
            none,
            ke(Key::Enter, false, false, false, false),
        ),
        (
            "KP_Enter -> Enter",
            gdk::Key::KP_Enter,
            none,
            ke(Key::Enter, false, false, false, false),
        ),
        // --- Other named keys ---
        (
            "Escape -> Esc",
            gdk::Key::Escape,
            none,
            ke(Key::Esc, false, false, false, false),
        ),
        (
            "Tab -> Tab",
            gdk::Key::Tab,
            none,
            ke(Key::Tab, false, false, false, false),
        ),
        (
            "ISO_Left_Tab + SHIFT -> Tab{shift:true}",
            gdk::Key::ISO_Left_Tab,
            shift,
            ke(Key::Tab, false, true, false, false),
        ),
        (
            "BackSpace -> Backspace",
            gdk::Key::BackSpace,
            none,
            ke(Key::Backspace, false, false, false, false),
        ),
        (
            "Delete -> Delete",
            gdk::Key::Delete,
            none,
            ke(Key::Delete, false, false, false, false),
        ),
        // --- Arrows ---
        (
            "Left -> Left",
            gdk::Key::Left,
            none,
            ke(Key::Left, false, false, false, false),
        ),
        (
            "Right -> Right",
            gdk::Key::Right,
            none,
            ke(Key::Right, false, false, false, false),
        ),
        (
            "Up -> Up",
            gdk::Key::Up,
            none,
            ke(Key::Up, false, false, false, false),
        ),
        (
            "Down -> Down",
            gdk::Key::Down,
            none,
            ke(Key::Down, false, false, false, false),
        ),
        // --- Home / End ---
        (
            "Home -> Home",
            gdk::Key::Home,
            none,
            ke(Key::Home, false, false, false, false),
        ),
        (
            "End -> End",
            gdk::Key::End,
            none,
            ke(Key::End, false, false, false, false),
        ),
        // --- Page Up / Page Down ---
        (
            "Page_Up -> PageUp",
            gdk::Key::Page_Up,
            none,
            ke(Key::PageUp, false, false, false, false),
        ),
        (
            "Page_Down -> PageDown",
            gdk::Key::Page_Down,
            none,
            ke(Key::PageDown, false, false, false, false),
        ),
        // --- Function keys ---
        (
            "F1 -> F(1)",
            gdk::Key::F1,
            none,
            ke(Key::F(F1_NUM), false, false, false, false),
        ),
        (
            "F5 -> F(5)",
            gdk::Key::F5,
            none,
            ke(Key::F(F5_NUM), false, false, false, false),
        ),
        (
            "F12 -> F(12)",
            gdk::Key::F12,
            none,
            ke(Key::F(F12_NUM), false, false, false, false),
        ),
        // --- Printable chars (shift contract: shift:false ALWAYS for chars) ---
        (
            "'a' -> Char('a') shift:false",
            key_from_char('a'),
            none,
            ke(Key::Char('a'), false, false, false, false),
        ),
        (
            "'A' (with SHIFT mask) -> Char('A') shift:false",
            key_from_char('A'),
            shift,
            ke(Key::Char('A'), false, false, false, false),
        ),
        (
            "'<' -> Char('<')",
            key_from_char('<'),
            none,
            ke(Key::Char('<'), false, false, false, false),
        ),
        // --- Modifiers on named keys (shift passes through for named keys) ---
        (
            "Ctrl+Left -> Left{ctrl:true}",
            gdk::Key::Left,
            ctrl,
            ke(Key::Left, true, false, false, false),
        ),
        (
            "Ctrl+Shift+Left -> Left{ctrl:true,shift:true}",
            gdk::Key::Left,
            ctrl | shift,
            ke(Key::Left, true, true, false, false),
        ),
        // --- Modifiers on printable chars ---
        (
            "Super+s -> Char('s'){meta:true,shift:false}",
            key_from_char('s'),
            sup,
            ke(Key::Char('s'), false, false, false, true),
        ),
        (
            "Alt+x -> Char('x'){alt:true}",
            key_from_char('x'),
            alt,
            ke(Key::Char('x'), false, false, true, false),
        ),
        (
            // Ctrl+letter must carry the BASE letter + ctrl flag, never a raw
            // control character (e.g. U+0001) inside Char.
            "Ctrl+a -> Char('a'){ctrl:true}",
            key_from_char('a'),
            ctrl,
            ke(Key::Char('a'), true, false, false, false),
        ),
        // --- Bare modifier / lock keys -> None ---
        ("bare Shift_L -> None", gdk::Key::Shift_L, none, None),
        ("bare Control_L -> None", gdk::Key::Control_L, none, None),
        ("bare Super_L -> None", gdk::Key::Super_L, none, None),
        ("bare Caps_Lock -> None", gdk::Key::Caps_Lock, none, None),
    ];

    for (desc, keyval, state, expected) in cases {
        let got = gdk_key(keyval, state);
        assert_eq!(got, expected, "case: {desc}");
    }
}
