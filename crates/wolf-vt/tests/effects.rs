//! Headless tests of the effects `wolf-vt` queues from libghostty-vt's callbacks: the
//! `WRITE_PTY` reply to a query, the pwd and title changes, and the render hold of
//! synchronized output. Each is fed as program output and read back with `take_effects`.
//!
//! No Neovim, no pty and no runfiles: `//crates/wolf-vt:effects` links the static archive and
//! nothing else, and `cargo test -p wolf-vt` runs the same file on Linux.
//!
//! ## Premises about libghostty-vt
//!
//! Each was read from the pinned 1.0.0 `terminal.h` and is measured here against the real
//! library, not assumed:
//!
//! - A device status report (`CSI 6 n`) is answered through the `WRITE_PTY` callback with the
//!   1-based cursor position, `CSI row;col R`, during the feed that carries the query.
//! - `OSC 7` fires `PWD_CHANGED` and `OSC 2` fires `TITLE_CHANGED`; the terminal keeps the pwd
//!   as the program sent it.
//! - `RENDER_HOLD` fires with `true` when mode 2026 is set, with `false` when it is reset, and
//!   with `false` when the terminal is resized during a hold.

use wolf_vt::{Effect, Terminal};

/// Makes a terminal, failing with the binding's error code.
fn terminal(cols: u16, rows: u16) -> Terminal {
    Terminal::new(cols, rows).unwrap_or_else(|error| {
        panic!(
            "Terminal::new({cols}, {rows}) failed with libghostty-vt result code {}.",
            error.code()
        )
    })
}

/// A cursor-position query is answered with one `WritePty` holding the position, and taking
/// the effects drains them.
#[test]
fn a_status_query_queues_its_reply() {
    let mut term = terminal(20, 5);
    let _ = term.take_effects();

    term.feed(b"\x1b[3;5H\x1b[6n");
    let effects = term.take_effects();
    let writes: Vec<&Vec<u8>> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::WritePty(bytes) => Some(bytes),
            _ => None,
        })
        .collect();
    assert_eq!(
        writes,
        vec![&b"\x1b[3;5R".to_vec()],
        "CSI 6 n with the cursor at row 3, column 5 must queue exactly one WritePty holding CSI \
         3;5 R; the effects are {effects:?}. None means the WRITE_PTY callback is not \
         registered, so the shell's query would never be answered."
    );

    let again = term.take_effects();
    assert!(
        again.is_empty(),
        "a second take_effects with nothing fed in between must be empty, not {again:?}: \
         take_effects must drain the queue, or core would write each reply again."
    );
}

/// `OSC 7` queues `PwdChanged` and `OSC 2` queues `TitleChanged`, and the terminal reports
/// the values sent.
#[test]
fn pwd_and_title_reports_queue_their_effects() {
    let mut term = terminal(20, 5);
    let _ = term.take_effects();

    let uri = "file://example-host/srv/osc-reported";
    term.feed(format!("\x1b]7;{uri}\x07").as_bytes());
    let effects = term.take_effects();
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::PwdChanged)),
        "OSC 7 must queue PwdChanged; the effects are {effects:?}. None means the PWD_CHANGED \
         callback is not registered."
    );
    assert_eq!(
        term.pwd(),
        uri,
        "pwd() must return the URI the program sent, unchanged, not {:?}.",
        term.pwd()
    );

    let title = "wolf session title";
    term.feed(format!("\x1b]2;{title}\x07").as_bytes());
    let effects = term.take_effects();
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::TitleChanged)),
        "OSC 2 must queue TitleChanged; the effects are {effects:?}. None means the \
         TITLE_CHANGED callback is not registered."
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::PwdChanged)),
        "OSC 2 changes no pwd, so it must not queue PwdChanged; the effects are {effects:?}."
    );
    assert_eq!(
        term.title(),
        title,
        "title() must return the title the program sent, not {:?}.",
        term.title()
    );
}

/// Setting mode 2026 queues `RenderHold(true)`, resetting it `RenderHold(false)`, and a
/// resize during a hold ends it with `RenderHold(false)`.
#[test]
fn synchronized_output_queues_render_holds() {
    let mut term = terminal(20, 5);
    let _ = term.take_effects();

    let holds = |effects: &[Effect]| -> Vec<bool> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::RenderHold(held) => Some(*held),
                _ => None,
            })
            .collect()
    };

    term.feed(b"\x1b[?2026h");
    let effects = term.take_effects();
    assert_eq!(
        holds(&effects),
        vec![true],
        "CSI ? 2026 h must queue one RenderHold(true); the effects are {effects:?}. None means \
         the RENDER_HOLD callback is not registered."
    );

    term.feed(b"\x1b[?2026l");
    let effects = term.take_effects();
    assert_eq!(
        holds(&effects),
        vec![false],
        "CSI ? 2026 l must queue one RenderHold(false); the effects are {effects:?}. true means \
         the callback's `held` argument is not passed through."
    );

    term.feed(b"\x1b[?2026h");
    let _ = term.take_effects();
    term.resize(30, 6, 8, 16)
        .unwrap_or_else(|error| panic!("resize failed with code {}.", error.code()));
    let effects = term.take_effects();
    assert_eq!(
        holds(&effects),
        vec![false],
        "a resize during a hold must end it with RenderHold(false); the effects are {effects:?}. \
         Nothing means effects queued by `resize` are dropped, and core would keep deferring \
         publishes until its maximum."
    );
}
