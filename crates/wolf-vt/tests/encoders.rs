//! Headless tests of `wolf-vt`'s input encoders: keys and mouse events encoded from the
//! terminal's modes, and the paste and focus encoders, compared byte for byte with what an
//! xterm-compatible program reads.
//!
//! No Neovim, no pty and no runfiles: `//crates/wolf-vt:encoders` links the static archive
//! and nothing else, and `cargo test -p wolf-vt` runs the same file on Linux.
//!
//! ## Premises about libghostty-vt
//!
//! Each was read from the pinned 1.0.0 headers (`key/encoder.h`, `mouse/encoder.h`,
//! `paste.h`, `focus.h`) and is measured here against the real library, not assumed:
//!
//! - The key and mouse encoders hold their own copies of the modes they encode by;
//!   `*_set_from_terminal` copies them from a terminal. Encoding without that copy encodes as
//!   a fresh terminal would, so the mode cases below fail.
//! - On macOS the key encoder treats alt as a text-producing modifier unless
//!   `GHOSTTY_KEY_ENCODER_OPT_MACOS_OPTION_AS_ALT` is set; the spec makes alt the encoder's
//!   alt, so alt+`x` must give `ESC x` on both platforms.
//! - The mouse encoder converts pixel positions to cells with the size it is given, and
//!   encodes nothing while no tracking mode is set.
//! - `ghostty_paste_encode` replaces unsafe control bytes, ESC among them, with spaces, wraps
//!   the text in `ESC [200~` and `ESC [201~` when bracketed, and otherwise turns each newline
//!   into a carriage return.

use wolf_vt::{
    focus_encode, paste_encode, KeyCode, KeyInput, Mods, MouseAction, MouseButton, MouseInput,
    Terminal,
};

/// Makes a terminal, failing with the binding's error code.
fn terminal(cols: u16, rows: u16) -> Terminal {
    Terminal::new(cols, rows).unwrap_or_else(|error| {
        panic!(
            "Terminal::new({cols}, {rows}) failed with libghostty-vt result code {}.",
            error.code()
        )
    })
}

/// A key with no text and no modifier.
fn named(code: KeyCode) -> KeyInput {
    KeyInput {
        code,
        text: String::new(),
        unshifted: None,
        mods: Mods::default(),
    }
}

/// A US-layout character key carrying its own text, with `mods`.
fn char_key(c: char, mods: Mods) -> KeyInput {
    KeyInput {
        code: KeyCode::Ascii(c),
        text: c.to_string(),
        unshifted: Some(c),
        mods,
    }
}

/// A mouse event at pixel `(x, y)` with no modifier.
fn mouse(action: MouseAction, button: Option<MouseButton>, x: f32, y: f32) -> MouseInput {
    MouseInput {
        action,
        button,
        mods: Mods::default(),
        x,
        y,
    }
}

/// Shows bytes with escapes visible, for failure messages.
fn shown(bytes: &[u8]) -> String {
    bytes.escape_ascii().to_string()
}

/// Keys encode as a fresh xterm-compatible terminal expects, and the encoder follows the
/// cursor-key and kitty keyboard modes the program sets.
#[test]
fn keys_encode_from_the_terminals_modes() {
    let mut term = terminal(80, 24);

    let enter = term.encode_key(&named(KeyCode::Enter));
    assert_eq!(
        enter,
        b"\r",
        "Enter must encode as CR, not {:?}; empty means the key encoder is never called.",
        shown(&enter)
    );

    let ctrl = Mods {
        ctrl: true,
        ..Mods::default()
    };
    let ctrl_c = term.encode_key(&char_key('c', ctrl));
    assert_eq!(
        ctrl_c,
        b"\x03",
        "ctrl+c must encode as 0x03, not {:?}; `c` means the ctrl modifier is dropped.",
        shown(&ctrl_c)
    );

    let alt = Mods {
        alt: true,
        ..Mods::default()
    };
    let alt_x = term.encode_key(&char_key('x', alt));
    assert_eq!(
        alt_x,
        b"\x1bx",
        "alt+x must encode as ESC x, not {:?}. A bare `x` means alt is dropped, or on macOS that \
         the encoder still treats option as a text modifier (MACOS_OPTION_AS_ALT unset).",
        shown(&alt_x)
    );

    let up = term.encode_key(&named(KeyCode::Up));
    assert_eq!(
        up,
        b"\x1b[A",
        "Up in normal cursor-key mode must encode as CSI A, not {:?}.",
        shown(&up)
    );
    term.feed(b"\x1b[?1h");
    let up_app = term.encode_key(&named(KeyCode::Up));
    assert_eq!(
        up_app,
        b"\x1bOA",
        "after CSI ? 1 h (cursor-key application mode) Up must encode as SS3 A, not {:?}. CSI A \
         means the encoder is not configured from the terminal's modes on this call.",
        shown(&up_app)
    );

    term.feed(b"\x1b[>1u");
    let escape = term.encode_key(&named(KeyCode::Escape));
    assert_eq!(
        escape,
        b"\x1b[27u",
        "after CSI > 1 u (kitty disambiguate) Escape must encode as CSI 27 u, not {:?}. A bare \
         ESC means the kitty flags are not copied from the terminal.",
        shown(&escape)
    );
}

/// Mouse events encode nothing without tracking, and with tracking and SGR reporting they
/// encode the cell the pixel position falls in, using the cell size of the last resize.
#[test]
fn mouse_events_encode_with_tracking_and_the_cell_size() {
    let mut term = terminal(80, 24);
    // (25, 65) px is column 2, row 3 with 10x20 px cells: 1-based 3;4 on the wire.
    let (x, y) = (25.0, 65.0);
    let untracked = term.encode_mouse(&mouse(MouseAction::Press, Some(MouseButton::Left), x, y));
    assert!(
        untracked.is_empty(),
        "with no tracking mode set a press must encode nothing, not {:?}: the program never \
         asked for mouse reports.",
        shown(&untracked)
    );

    term.feed(b"\x1b[?1000h\x1b[?1006h");
    term.resize(80, 24, 10, 20)
        .unwrap_or_else(|error| panic!("resize failed with code {}.", error.code()));

    let press = term.encode_mouse(&mouse(MouseAction::Press, Some(MouseButton::Left), x, y));
    assert_eq!(
        press,
        b"\x1b[<0;3;4M",
        "a left press at ({x}, {y}) px with 10x20 px cells must encode as CSI < 0;3;4 M, not \
         {:?}. Empty means the modes are not copied from the terminal; other coordinates mean \
         the cell size of the resize is not given to the encoder.",
        shown(&press)
    );

    let release = term.encode_mouse(&mouse(MouseAction::Release, Some(MouseButton::Left), x, y));
    assert_eq!(
        release,
        b"\x1b[<0;3;4m",
        "the matching release must encode as CSI < 0;3;4 m, not {:?}.",
        shown(&release)
    );

    let wheel = term.encode_mouse(&mouse(MouseAction::Press, Some(MouseButton::WheelUp), x, y));
    assert_eq!(
        wheel,
        b"\x1b[<64;3;4M",
        "a wheel-up press must encode as CSI < 64;3;4 M, not {:?}.",
        shown(&wheel)
    );
}

/// Paste text is wrapped when bracketed, has its newlines turned into carriage returns when
/// not, and never passes an ESC through.
#[test]
fn paste_is_bracketed_or_converted_and_never_passes_escape() {
    let bracketed = paste_encode("a\nb", true);
    assert_eq!(
        bracketed,
        b"\x1b[200~a\nb\x1b[201~",
        "a bracketed paste of \"a\\nb\" must be wrapped in CSI 200~ and CSI 201~ with its newline \
         kept, not {:?}.",
        shown(&bracketed)
    );

    let plain = paste_encode("a\nb", false);
    assert_eq!(
        plain,
        b"a\rb",
        "an unbracketed paste of \"a\\nb\" must turn the newline into CR, not {:?}. The text \
         unchanged means it is not encoded; brackets mean `bracketed` is ignored.",
        shown(&plain)
    );

    let escaped = paste_encode("x\x1by", false);
    assert!(
        !escaped.contains(&0x1b),
        "a paste holding an ESC must not pass the ESC to the program, but encoded as {:?}.",
        shown(&escaped)
    );
    let inner = paste_encode("x\x1by", true);
    let body = inner
        .strip_prefix(b"\x1b[200~".as_slice())
        .and_then(|rest| rest.strip_suffix(b"\x1b[201~".as_slice()))
        .unwrap_or_else(|| {
            panic!(
                "a bracketed paste must be wrapped, got {:?}.",
                shown(&inner)
            )
        });
    assert!(
        !body.contains(&0x1b),
        "inside the brackets, a pasted ESC must not reach the program (it could end the paste \
         early), but the body is {:?}.",
        shown(body)
    );
}

/// Focus reports encode as CSI I and CSI O.
#[test]
fn focus_reports_encode_gained_and_lost() {
    let gained = focus_encode(true);
    assert_eq!(
        gained,
        b"\x1b[I",
        "focus gained must encode as CSI I, not {:?}.",
        shown(&gained)
    );
    let lost = focus_encode(false);
    assert_eq!(
        lost,
        b"\x1b[O",
        "focus lost must encode as CSI O, not {:?}; CSI I means the two are swapped.",
        shown(&lost)
    );
}
