//! End-to-end round-trip: real `gdk_key` translation -> `WolfCore` -> real Neovim
//! -> snapshot. No mocks (per project policy). Requires `WOLF_NVIM_BIN`.
//!
//! This exercises the full input seam: GDK keyvals are translated by
//! `wolf_gnome::gdk_key`, fed into the core as `InputEvent::Key`, and we then
//! assert that the typed word — including an UPPERCASE letter — lands in the grid.
//! That uppercase letter validates the shift-glyph contract end-to-end: GDK
//! delivers the already-shifted keyval (and may also set the SHIFT mask), and the
//! word must still arrive uppercased in the buffer.

mod common;

use std::time::Duration;

use gdk::glib::translate::from_glib;
use gdk::ModifierType;

use wolf_core::{InputEvent, WolfCore};
use wolf_gnome::gdk_key;

use common::{config, wake_channel, FLUSH_TIMEOUT};

fn key_from_char(c: char) -> gdk::Key {
    // gdk4-rs has no Key::from_unicode; construct the keyval exactly as GDK would
    // from a Unicode scalar, mirroring how a real keypress arrives.
    unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) }
}

/// Grid size for the round-trip (small but ample for a single typed word).
const COLS: u16 = 80;
const ROWS: u16 = 24;

/// The word we type. The leading 'H' is uppercase to exercise the shift contract.
const WORD: &str = "Hello";

/// Brief settle window to drain follow-up flushes (insert -> normal emits a few),
/// mirroring `examples/grid_dump.rs`.
const SETTLE: Duration = Duration::from_millis(200);

/// Translates a GDK keyval + state into an `InputEvent::Key`, asserting that
/// `gdk_key` actually maps it (the test only feeds mappable keys).
fn key_event(keyval: gdk::Key, state: ModifierType) -> InputEvent {
    let ev = gdk_key(keyval, state)
        .unwrap_or_else(|| panic!("gdk_key returned None for a key the test expects to map"));
    InputEvent::Key(ev)
}

#[test]
fn type_word_via_gdk_key_appears_in_grid() {
    // Fail loud if the real nvim binary is not configured.
    common::require_nvim_bin();

    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(config(COLS, ROWS), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");

    // Wait for the initial attach flush.
    waiter.wait_for_flush(FLUSH_TIMEOUT);

    // Enter insert mode: 'i'.
    core.send_input(key_event(key_from_char('i'), ModifierType::empty()));

    // The uppercase letter is fed uppercased with `SHIFT_MASK`, as GDK reports Shift+letter;
    // gdk_key must drop the shift for printable chars and still produce the uppercase glyph.
    for c in WORD.chars() {
        let state = if c.is_ascii_uppercase() {
            ModifierType::SHIFT_MASK
        } else {
            ModifierType::empty()
        };
        core.send_input(key_event(key_from_char(c), state));
    }

    // Leave insert mode: Escape.
    core.send_input(key_event(gdk::Key::Escape, ModifierType::empty()));

    // Wait for the first post-typing flush, then drain brief follow-ups so we
    // observe the settled frame (insert -> normal emits a few flushes).
    waiter.wait_for_flush(FLUSH_TIMEOUT);
    waiter.drain();
    std::thread::sleep(SETTLE);
    waiter.drain();

    let snap = core.read_snapshot();
    let row0 = snap.active_grid().row_text(0);
    assert!(
        row0.contains(WORD),
        "row 0 should contain the typed word {WORD:?} (shift contract end-to-end); \
         got {row0:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}
