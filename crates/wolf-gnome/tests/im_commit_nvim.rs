//! End-to-end: the COMMIT rulebook against a real Neovim. `im_ui::commit_decision`
//! -> `InputEvent::Key` -> `WolfCore` -> snapshot. No mocks (per project policy).
//! Requires `WOLF_NVIM_BIN`.
//!
//! [`roundtrip_nvim.rs`](./roundtrip_nvim.rs) covers the OTHER entry point — a GDK
//! keyval translated by `gdk_key` — and the two deliberately do not overlap. What is
//! new here is the second entry point the input method opens: a `commit` string the
//! window's `gtk::IMMulticontext` hands over, which carries no keyval, no modifier
//! state and no keycode, and which is how nearly every printable character reaches
//! Neovim. Its failure modes are different in kind from a missing word — a reordered
//! one, a fired Escape, a normal-mode command executed as text — which is why the
//! diagnosis lives in a file of its own.
//!
//! ## What the GTK side would do, and what this file does instead
//!
//! `window::wire_keys`' `connect_commit` handler reads the four overlay handles,
//! calls `im_ui::commit_decision(overlay_open, text)`, and on
//! `CommitDecision::Send` pushes each event onto the ordered input channel as
//! `InputEvent::Key`. [`send_commit`] is that loop with the GTK removed: same seam,
//! same event type, same order. No `gtk::IMContext` is constructed here and none
//! could be — there is no display — so what these cases pin is the rulebook plus the
//! transport, not the widget wiring. The compositor smoke
//! `composed_text_reaches_neovim_under_cage` is what joins the two.
//!
//! ## Premises about the real Neovim, stated rather than assumed
//!
//! Two of the cases below rest on a claim about what Neovim does with the bytes it
//! is handed, and both are measured HERE against the pinned binary rather than taken
//! on trust — which is the whole reason these cases are not table rows:
//!
//! * A raw ESC byte delivered through `nvim_input` LEAVES INSERT MODE, after which
//!   the next committed character is executed as a normal-mode command rather than
//!   inserted. [`a_control_character_in_a_commit_never_reaches_neovim`] is what
//!   measures it: with the `is_control` filter deleted, its row reads `"a"` — the
//!   trailing `b` having become a word-back motion — rather than `"ab"`.
//! * `dd` delivered as two `InputEvent::Key`s DELETES A LINE, where the same two
//!   characters delivered as an `InputEvent::Paste` would insert the text `dd`.
//!   [`a_normal_mode_commit_is_a_command_and_not_text`] measures that difference.
//!
//! ## Insert mode is not decoration
//!
//! Every case that expects committed text to be OBSERVABLE enters insert mode first,
//! and the refusal case
//! ([`a_commit_behind_an_open_overlay_sends_nothing`]) attempts its refused commit
//! while still IN insert mode. That is load-bearing rather than tidy: in normal mode
//! an `é` that leaked past the overlay guard would be an unmapped key that changes
//! nothing, so the assertion would hold with the guard deleted and the case would
//! prove nothing at all.

mod common;

use std::time::Duration;

use gdk::glib::translate::from_glib;
use gdk::ModifierType;

use wolf_core::{InputEvent, WolfCore};
use wolf_gnome::gdk_key;
use wolf_gnome::im_ui::{commit_decision, CommitDecision};

use common::{config, wake_channel, FlushWaiter, FLUSH_TIMEOUT};

/// gdk4-rs has no `Key::from_unicode`; this constructs the keyval exactly as GDK
/// would from a Unicode scalar, mirroring how a real keypress arrives.
///
/// RE-DECLARED rather than imported: `gdk_key.rs`, `roundtrip_nvim.rs` and
/// `actions.rs`'s own tests each carry their own copy, because Cargo compiles every
/// `tests/*.rs` into a separate binary and there is nothing to import it from. See
/// `tests/README.md`, "The `key_from_char` helper".
fn key_from_char(c: char) -> gdk::Key {
    unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) }
}

/// Grid size. Ample for the two short lines the deepest case builds.
const COLS: u16 = 80;
const ROWS: u16 = 24;

/// Brief settle window to drain follow-up flushes. Leaving insert mode emits
/// several, so every case that reads a snapshot drains, sleeps this, and drains
/// again — the treatment `roundtrip_nvim.rs`'s README entry requires of any case
/// that reads one. A timing knob; no assertion depends on its value.
const SETTLE: Duration = Duration::from_millis(300);

/// Starts a real Neovim behind a real `WolfCore` and waits for the attach flush.
fn start(test: &str) -> (WolfCore, FlushWaiter) {
    // Fail loud if the real nvim binary is not configured (project policy: no mocks,
    // and no silent skip either).
    common::require_nvim_bin();

    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(config(COLS, ROWS), wake).unwrap_or_else(|e| {
        panic!("{test}: WolfCore::start failed ({e}) — is WOLF_NVIM_BIN a valid nvim?")
    });
    waiter.wait_for_flush(FLUSH_TIMEOUT);
    (core, waiter)
}

/// Translates a GDK keyval + state into an `InputEvent::Key`, asserting that
/// `gdk_key` actually maps it.
///
/// Only the STRUCTURAL keys of each case go through here — `i`, Return, Escape, the
/// `gg` motion — so that the characters under test are the only thing arriving by
/// the commit route and a failure cannot be blamed on the wrong entry point.
fn key_event(test: &str, keyval: gdk::Key, state: ModifierType) -> InputEvent {
    let ev = gdk_key(keyval, state).unwrap_or_else(|| {
        panic!(
            "{test}: gdk_key returned None for {keyval:?}, a key this file only feeds because it \
             expects it to map. Every case drives its editor state through gdk_key and its \
             SUBJECT through the commit seam, so an unmapped structural key means the case never \
             reached the state it is written about."
        )
    });
    InputEvent::Key(ev)
}

/// `window::wire_keys`' commit handler with the GTK removed: ask the seam, and on
/// `Send` push each event onto the ordered input channel in order.
///
/// Returns how many events were sent, so a case can assert on a refusal directly
/// rather than inferring it from the buffer.
fn send_commit(core: &WolfCore, overlay_open: bool, text: &str) -> usize {
    match commit_decision(overlay_open, text) {
        CommitDecision::Refuse(_) => 0,
        CommitDecision::Send(events) => {
            let sent = events.len();
            for ev in events {
                core.send_input(InputEvent::Key(ev));
            }
            sent
        }
    }
}

/// Waits for the first flush after the last input, then drains brief follow-ups so
/// the snapshot read afterwards is the SETTLED frame.
fn settle(waiter: &FlushWaiter) {
    waiter.wait_for_flush(FLUSH_TIMEOUT);
    waiter.drain();
    std::thread::sleep(SETTLE);
    waiter.drain();
}

/// The active grid's first row, with the padding `row_text` writes for unpainted
/// cells trimmed off.
fn row0(core: &WolfCore) -> String {
    core.read_snapshot()
        .active_grid()
        .row_text(0)
        .trim()
        .to_string()
}

/// A committed character reaches the buffer as itself.
///
/// The plainest case, and the one that fails if `commit_keys` returns an empty vec
/// or drops anything non-ASCII. `é` is what GTK's own compose table produces for
/// `dead_acute` then `e`, measured under `cage`; feeding the composed result rather
/// than the sequence is the point — composing is GTK's job and this file's subject
/// starts at the `commit` string.
#[test]
fn a_committed_character_reaches_the_buffer() {
    const TEST: &str = "a_committed_character_reaches_the_buffer";
    // The composed character, and the whole of what row 0 may hold afterwards.
    const COMPOSED: &str = "é";

    let (core, waiter) = start(TEST);

    core.send_input(key_event(TEST, key_from_char('i'), ModifierType::empty()));
    let sent = send_commit(&core, false, COMPOSED);
    assert_eq!(
        sent, 1,
        "{TEST}: the seam produced {sent} event(s) for the commit {COMPOSED:?} rather than 1. \
         This is a PRECONDITION on `commit_keys`, whose own table covers the rule; what it buys \
         here is that a failure of the buffer assertion below is about the TRANSPORT rather than \
         about the rulebook."
    );
    core.send_input(key_event(TEST, gdk::Key::Escape, ModifierType::empty()));
    settle(&waiter);

    let row = row0(&core);
    assert_eq!(
        row, COMPOSED,
        "{TEST}: row 0 is {row:?}, not {COMPOSED:?}. An EMPTY row means the commit reached Neovim \
         as nothing — the whole defect this seam exists to close, since a dead key and a compose \
         sequence are invisible to `gdk_key`. A row holding the bare ASCII base letter would mean \
         something decomposed the glyph on the way. Equality rather than `contains` is deliberate: \
         it is what makes a DOUBLED commit visible, which a containment check would pass."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A multi-character commit arrives in source order, and whole.
///
/// An out-of-process engine commits whole words, and `gtk_im_context_reset` can
/// commit a partial sequence in one string, so this is the ordinary shape rather
/// than an exotic one. `ærø` is three distinct non-ASCII characters: its reversal
/// and its first character alone are both different strings, so one fixture
/// discriminates `text.chars().rev()` and `text.chars().next()` at once.
#[test]
fn a_multi_character_commit_arrives_in_order() {
    const TEST: &str = "a_multi_character_commit_arrives_in_order";
    // The committed word.
    const WORD: &str = "ærø";
    // What the same characters read as REVERSED — named so the failure message can
    // tell "the order was lost" apart from "the text was lost".
    const REVERSED: &str = "øræ";

    let (core, waiter) = start(TEST);

    core.send_input(key_event(TEST, key_from_char('i'), ModifierType::empty()));
    let sent = send_commit(&core, false, WORD);
    assert_eq!(
        sent,
        WORD.chars().count(),
        "{TEST}: the seam produced {sent} event(s) for the commit {WORD:?} rather than one per \
         character. A PRECONDITION, as in the single-character case."
    );
    core.send_input(key_event(TEST, gdk::Key::Escape, ModifierType::empty()));
    settle(&waiter);

    let row = row0(&core);
    assert_ne!(
        row, REVERSED,
        "{TEST}: row 0 holds the committed characters REVERSED. Every character arrived, so this \
         is not a transport failure: `commit_keys` is emitting them in the wrong order, and \
         `wolf_core::apply_batch` fuses a run of consecutive Key events into ONE `nvim_input` \
         call, so nothing downstream can put them back."
    );
    assert_eq!(
        row,
        WORD,
        "{TEST}: row 0 is {row:?}, not {WORD:?}. A row holding only {:?} means the seam emitted \
         the first character alone; an empty row means it emitted none; anything longer means a \
         character was duplicated. Equality is what distinguishes all three from each other.",
        WORD.chars().next().map(String::from).unwrap_or_default()
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A control character inside a commit never reaches Neovim — and this is the case
/// whose failure is loud in the buffer rather than merely absent from it.
///
/// The committed string is `a`, ESC, `b`. With the `is_control` filter in place both
/// printables are inserted and row 0 reads `ab`. Without it, the raw ESC byte goes
/// through `nvim_input` (a modifier-free `Key::Char(c)` renders as the bare
/// character, so there is no escaping in the way), insert mode ends, and the
/// following `b` is executed as a normal-mode word-back motion that inserts nothing
/// — leaving row 0 reading `a`. That difference is measured against the real Neovim
/// here, which is exactly what a table row could not do.
#[test]
fn a_control_character_in_a_commit_never_reaches_neovim() {
    const TEST: &str = "a_control_character_in_a_commit_never_reaches_neovim";
    // The committed string: two printables around one control character.
    const COMMITTED: &str = "a\u{1b}b";
    // What must reach the buffer.
    const SURVIVORS: &str = "ab";
    // What the buffer reads if the ESC fired: the `b` became a motion.
    const IF_THE_ESC_FIRED: &str = "a";

    let (core, waiter) = start(TEST);

    core.send_input(key_event(TEST, key_from_char('i'), ModifierType::empty()));
    let sent = send_commit(&core, false, COMMITTED);
    assert_eq!(
        sent, 2,
        "{TEST}: the seam produced {sent} event(s) for the commit {COMMITTED:?} rather than 2. A \
         PRECONDITION on `commit_keys`: 3 means the control character survived the seam (the \
         buffer assertion below is the end-to-end half of the same finding), and anything less \
         than 2 means a printable was dropped with it."
    );
    core.send_input(key_event(TEST, gdk::Key::Escape, ModifierType::empty()));
    settle(&waiter);

    let row = row0(&core);
    assert_eq!(
        row, SURVIVORS,
        "{TEST}: row 0 is {row:?}, not {SURVIVORS:?}. If it reads {IF_THE_ESC_FIRED:?}, the ESC \
         byte inside the commit reached `nvim_input`: it ended insert mode, and the `b` behind it \
         was executed as a normal-mode motion instead of being typed. That is the whole reason a \
         control character must never become a `Key::Char` — `Return`, `Escape` and `Tab` already \
         have a route through `gdk_key`'s named-key table, which renders them as `<CR>`, `<Esc>` \
         and `<Tab>`, and a second unnamed route that disagrees with the first is the bug."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A commit that arrives while an overlay is open sends nothing.
///
/// ## Why the refused commit is attempted from INSIDE insert mode
///
/// It is the only state in which a leak is observable. Attempted in normal mode, an
/// `é` that got past the guard would be an unmapped key that changes no buffer, so
/// the assertion would hold with the guard deleted — a check aimed at the right
/// property, looking where the defect cannot appear. In insert mode the same leak
/// appends the character to the marker and the row assertion fires.
///
/// ## What it does and does not cover
///
/// The refusal arm is unreachable from this front-end's own glue, because GTK's
/// built-in context commits synchronously inside `filter_keypress` and that call is
/// made only with no overlay open; an out-of-process engine commits on a bus reply
/// at an arbitrary main-loop turn, and that is the arrival the guard is for. So what
/// this case adds over `im_ui`'s own table is that the SEND LOOP — the one every
/// other case in this file uses — puts nothing on the channel on a refusal.
#[test]
fn a_commit_behind_an_open_overlay_sends_nothing() {
    const TEST: &str = "a_commit_behind_an_open_overlay_sends_nothing";
    // Typed with no overlay open, so the row carries proof that the transport works
    // at all. Deliberately unlike REFUSED in every character.
    const MARKER: &str = "zzguardalfa";
    // Committed with an overlay open, and asserted absent afterwards.
    const REFUSED: &str = "é";

    let (core, waiter) = start(TEST);

    core.send_input(key_event(TEST, key_from_char('i'), ModifierType::empty()));
    let typed = send_commit(&core, false, MARKER);
    assert_eq!(
        typed,
        MARKER.chars().count(),
        "{TEST}: the marker commit produced {typed} event(s) rather than one per character, so the \
         precondition this case rests on — that the transport works with no overlay open — is \
         already broken and nothing below is diagnosable."
    );

    // Still in INSERT mode: see this test's doc comment for why that is the only
    // state in which a leaked character would be visible.
    let refused = send_commit(&core, true, REFUSED);
    assert_eq!(
        refused, 0,
        "{TEST}: the send loop pushed {refused} event(s) for a commit that arrived with an overlay \
         open. `commit_decision` must answer Refuse whatever the text, because the characters \
         belong to the overlay's own entry: forwarding them types the user's picker query into \
         the buffer behind it, and — since offering a key to the input method is a side effect \
         that cannot be taken back — the composition that produced them was started against a \
         widget the editor cannot see."
    );

    core.send_input(key_event(TEST, gdk::Key::Escape, ModifierType::empty()));
    settle(&waiter);

    let row = row0(&core);
    assert!(
        !row.contains(REFUSED),
        "{TEST}: row 0 ({row:?}) contains {REFUSED:?}, which was committed with an overlay open \
         and must never have reached Neovim. This is the discriminator: it is what a \
         `commit_decision` that had lost its `overlay_open` parameter produces, and the marker \
         beside it is what says the transport was working at the time."
    );
    assert_eq!(
        row, MARKER,
        "{TEST}: row 0 is {row:?}, not {MARKER:?}. The refused character is absent (the assertion \
         above passed), so this says the MARKER did not arrive intact — a corroboration failure \
         rather than a guard failure. An empty row means nothing was typed at all, in which case \
         the refusal above held for the wrong reason and this case proves nothing."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A commit delivered in NORMAL mode is executed as a command, not inserted as text.
///
/// Measurement 3 behind this unit says the input method filters plain printables
/// with no sequence pending, so every normal-mode command key travels the commit
/// path: `dd`, `gg`, `<C-w>` sequences, every character of a typed `:e` command
/// line. That makes the transport choice — `InputEvent::Key` rather than
/// `InputEvent::Paste` — the single most consequential thing about the commit
/// handler, because `nvim_paste` bypasses `nvim_input` entirely, runs no mappings,
/// and would insert the text `dd` where a line deletion belongs.
///
/// **What this case discriminates, stated honestly:** nothing inside `im_ui`. The
/// seam answers `Send(events)` either way; it is [`send_commit`]'s own choice of
/// `InputEvent::Key` that this pins, and that loop is the test's rather than the
/// application's. The shipped glue's identical choice is covered by the compositor
/// smokes, every one of which drives its normal-mode keys through the commit path.
#[test]
fn a_normal_mode_commit_is_a_command_and_not_text() {
    const TEST: &str = "a_normal_mode_commit_is_a_command_and_not_text";
    // The line the normal-mode command must delete.
    const LINE_ONE: &str = "alfagone";
    // The line that must survive and become row 0. Neither line is a substring of
    // the other, so "the right line went" is distinguishable from "a line went".
    const LINE_TWO: &str = "bravostays";
    // The normal-mode command, committed as text through the seam.
    const COMMAND: &str = "dd";

    let (core, waiter) = start(TEST);

    // Build two known lines: the text through the seam, as an input method commits it while typing,
    // and the mode and newline keys through `gdk_key`.
    core.send_input(key_event(TEST, key_from_char('i'), ModifierType::empty()));
    send_commit(&core, false, LINE_ONE);
    core.send_input(key_event(TEST, gdk::Key::Return, ModifierType::empty()));
    send_commit(&core, false, LINE_TWO);
    core.send_input(key_event(TEST, gdk::Key::Escape, ModifierType::empty()));
    // Back to the first line, so the command below acts on a known one.
    core.send_input(key_event(TEST, key_from_char('g'), ModifierType::empty()));
    core.send_input(key_event(TEST, key_from_char('g'), ModifierType::empty()));
    settle(&waiter);

    // --- Precondition: the buffer really holds the two lines, first one on top ----
    let before = row0(&core);
    assert_eq!(
        before, LINE_ONE,
        "{TEST}: before the command, row 0 is {before:?} rather than {LINE_ONE:?}. This is a \
         PRECONDITION failure, not a transport one: the two-line buffer was never built, so the \
         assertion below would be reading a buffer nothing was going to change. Check \
         `a_multi_character_commit_arrives_in_order` first — if it is green, the suspect is the \
         Return key rather than the commit path."
    );

    let sent = send_commit(&core, false, COMMAND);
    assert_eq!(
        sent, 2,
        "{TEST}: the seam produced {sent} event(s) for the commit {COMMAND:?} rather than 2."
    );
    settle(&waiter);

    let after = row0(&core);
    assert!(
        !after.contains(LINE_ONE),
        "{TEST}: after committing {COMMAND:?} in normal mode, row 0 ({after:?}) still contains \
         {LINE_ONE:?}, so the two characters were TEXT rather than a command. That is what \
         delivering a commit as `InputEvent::Paste` produces: `nvim_paste` bypasses `nvim_input`, \
         runs no mappings and inserts the literal characters in one undo block. Since the input \
         method filters plain printables, normal-mode commits are the common case rather than an \
         exotic one, so this would break every `<C-w>` chord and every typed `:` command line at \
         once."
    );
    assert_eq!(
        after, LINE_TWO,
        "{TEST}: after the command, row 0 is {after:?} rather than {LINE_TWO:?}. The first line is \
         gone (the assertion above passed), so this says the wrong thing went with it — a `dd` \
         that ran twice, or a `d` that reached Neovim without its partner and left an operator \
         pending. The pair is what tells a correct deletion apart from a partial one."
    );

    core.shutdown().expect("shutdown should be Ok");
}
