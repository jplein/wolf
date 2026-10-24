//! GDK key translation seam: `gdk_key(keyval, state) -> Option<KeyEvent>`.
//!
//! This is the exact shape of a future `gtk::EventControllerKey` `key-pressed`
//! callback (GDK keyval + `ModifierType`), but kept as a free function so it can
//! be unit-tested headlessly — the `gdk::Key` constants are compile-time and the
//! printable path reads `keyval.to_unicode()`, both pure and needing no display
//! server.
//!
//! ## Shift-glyph contract
//!
//! GDK already delivers the *shifted glyph* in the keyval (e.g. Shift+`a` arrives
//! as the `A` keyval whose `to_unicode()` is `'A'`). So for printable characters
//! we ALWAYS set `shift = false` and let the glyph itself carry the case — this
//! matches [`wolf_core::KeyEvent`]'s contract, where `shift` is ignored for
//! [`wolf_core::Key::Char`]. For *named* keys the shift modifier is meaningful and
//! is passed through (`<S-Tab>`, `<S-Left>`, `<S-F5>`). The X11 quirk
//! `ISO_Left_Tab` (what Shift+Tab actually reports) maps to `Key::Tab` with
//! `shift = true`.
//!
//! Returns `None` for keyvals that should produce no input: bare modifier/lock
//! keys, and any keyval whose `to_unicode()` is absent or a control character.

use gdk::glib::translate::IntoGlib;
use wolf_core::{Key, KeyEvent};

/// `Key::F(n)` numbering starts at 1 (`F1`), so the first function key maps to
/// `n = 1`. Used to convert a keyval offset within the `F1..=F12` range.
const F_KEY_FIRST: u8 = 1;

/// Translates a GDK keyval + modifier state into a [`wolf_core::KeyEvent`].
///
/// Returns `None` for unmapped/bare-modifier/lock keyvals and for keyvals with no
/// printable (non-control) unicode value.
#[must_use]
pub fn gdk_key(keyval: gdk::Key, state: gdk::ModifierType) -> Option<KeyEvent> {
    let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
    let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
    let alt = state.contains(gdk::ModifierType::ALT_MASK);
    let meta = state.contains(gdk::ModifierType::SUPER_MASK);

    // Named keys first: their `to_unicode()` is often a control char (Return -> '\r'), so they
    // resolve before the printable fallback, keeping the real `shift` modifier.
    if let Some(key) = named_key(keyval) {
        // `ISO_Left_Tab` *is* Shift+Tab; ensure shift is reflected even if the
        // platform did not also set the SHIFT mask.
        let shift = shift || keyval == gdk::Key::ISO_Left_Tab;
        return Some(KeyEvent {
            key,
            ctrl,
            shift,
            alt,
            meta,
        });
    }

    // Bare modifier / lock keys produce no input.
    if is_modifier_or_lock(keyval) {
        return None;
    }

    // Printable fallback: the keyval already carries the shifted glyph, so the
    // shift modifier is dropped (set false) for `Key::Char` per the contract.
    let c = keyval.to_unicode()?;
    if c.is_control() {
        return None;
    }
    Some(KeyEvent {
        key: Key::Char(c),
        ctrl,
        shift: false,
        alt,
        meta,
    })
}

/// Maps a GDK keyval to a named [`wolf_core::Key`], or `None` if it is not a
/// recognized named key (it may still be a printable char).
fn named_key(keyval: gdk::Key) -> Option<Key> {
    let key = match keyval {
        gdk::Key::Return | gdk::Key::KP_Enter => Key::Enter,
        gdk::Key::Escape => Key::Esc,
        gdk::Key::Tab | gdk::Key::ISO_Left_Tab => Key::Tab,
        gdk::Key::BackSpace => Key::Backspace,
        gdk::Key::Delete => Key::Delete,
        gdk::Key::Left => Key::Left,
        gdk::Key::Right => Key::Right,
        gdk::Key::Up => Key::Up,
        gdk::Key::Down => Key::Down,
        gdk::Key::Home => Key::Home,
        gdk::Key::End => Key::End,
        gdk::Key::Page_Up => Key::PageUp,
        gdk::Key::Page_Down => Key::PageDown,
        _ => return function_key(keyval),
    };
    Some(key)
}

/// Maps the contiguous `F1..=F12` keyval range to `Key::F(n)`.
fn function_key(keyval: gdk::Key) -> Option<Key> {
    let first = gdk::Key::F1.into_glib();
    let last = gdk::Key::F12.into_glib();
    let kv = keyval.into_glib();
    if (first..=last).contains(&kv) {
        let n = u8::try_from(kv - first).ok()? + F_KEY_FIRST;
        Some(Key::F(n))
    } else {
        None
    }
}

/// True for bare modifier/lock keys, which must not generate input on their own.
fn is_modifier_or_lock(keyval: gdk::Key) -> bool {
    matches!(
        keyval,
        gdk::Key::Shift_L
            | gdk::Key::Shift_R
            | gdk::Key::Control_L
            | gdk::Key::Control_R
            | gdk::Key::Alt_L
            | gdk::Key::Alt_R
            | gdk::Key::Meta_L
            | gdk::Key::Meta_R
            | gdk::Key::Super_L
            | gdk::Key::Super_R
            | gdk::Key::Hyper_L
            | gdk::Key::Hyper_R
            | gdk::Key::Caps_Lock
            | gdk::Key::Shift_Lock
            | gdk::Key::Num_Lock
            | gdk::Key::Scroll_Lock
    )
}
