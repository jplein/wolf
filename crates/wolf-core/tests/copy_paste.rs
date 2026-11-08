//! Integration tests for the COPY / PASTE surface against a real nvim:
//! visual-mode detection (`Snapshot::is_visual_mode`), yanking a visual selection
//! out of the captive nvim (`WolfCore::copy_visual_selection`), and pasting text
//! back in (`WolfCore::paste`).
//!
//! Requires a real, pinned nvim via `WOLF_NVIM_BIN` (or the Bazel-staged tree),
//! reusing `tests/common/mod.rs`. Modeled on `input_keys.rs` / `redraw_grid.rs`:
//! type keys through `send_input`, wait for a redraw flush, then assert on the
//! snapshot / grid.

mod common;

use common::{ch, ctrl, key, poll_until};
use wolf_core::Key;

/// A single-line fixture whose first WORD is a distinctive, space-free token so a
/// `v` + `e` (extend to word end) selects exactly that word and the yanked text is
/// unambiguous.
const COPY_FIXTURE: &str = "WOLFCOPY selection target\n";

/// The distinctive first word of [`COPY_FIXTURE`]. `ve` from the line start
/// selects exactly this token (visual is inclusive of the end char), so the
/// yanked unnamed register must CONTAIN it. This pins the copy path to nvim's
/// visual-`y` semantics (unnamed register `"`); the yank must be synchronous so
/// the register is populated before `copy_visual_selection` returns.
const COPY_WORD: &str = "WOLFCOPY";

/// A distinctive token pasted into an empty buffer; it must show up in the grid.
const PASTE_TEXT: &str = "WOLFPASTE";

#[test]
fn is_visual_mode_tracks_v_bar_and_ctrl_v() {
    let (core, waiter, _file) = common::start_core_with_temp_md(COPY_FIXTURE);

    // Fresh from open: normal mode, so NOT visual.
    assert!(
        !core.read_snapshot().is_visual_mode(),
        "a freshly opened buffer starts in normal mode (is_visual_mode == false)"
    );

    // Each of `v`, `V`, and `<C-v>` puts nvim into a visual cursor-mode that the
    // snapshot reports as visual; `<Esc>` returns to normal.
    for (name, enter) in [
        ("v", ch('v')),
        ("V", ch('V')),
        ("<C-v>", ctrl(Key::Char('v'))),
    ] {
        core.send_input(enter);
        poll_until(
            &waiter,
            &format!(
                "is_visual_mode() true after {name}; never means its mode_change was not folded \
                 as visual"
            ),
            || core.read_snapshot().is_visual_mode(),
            |&ok| ok,
        );

        core.send_input(key(Key::Esc));
        poll_until(
            &waiter,
            &format!(
                "is_visual_mode() false after <Esc> from {name}; never means the return to \
                 normal mode was not folded"
            ),
            || !core.read_snapshot().is_visual_mode(),
            |&ok| ok,
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn copy_visual_selection_yields_the_selected_text() {
    let (core, waiter, _file) = common::start_core_with_temp_md(COPY_FIXTURE);

    // Anchor at column 0 of line 1, then `v` (anchored on 'W') and `e` (to the word end) give an
    // inclusive selection of exactly COPY_WORD.
    waiter.drain();
    core.send_input(ch('g'));
    core.send_input(ch('g'));
    core.send_input(ch('0'));
    core.send_input(ch('v'));
    core.send_input(ch('e'));
    // copy_visual_selection() bypasses the input pump, so poll until visual mode lands.
    poll_until(
        &waiter,
        "is_visual_mode() true after gg0ve; never means the keys were not applied, so the copy \
         below would see normal mode",
        || core.read_snapshot().is_visual_mode(),
        |&ok| ok,
    );

    let selection = core.copy_visual_selection();
    assert!(
        selection.as_deref().is_some_and(|s| s.contains(COPY_WORD)),
        "copy_visual_selection() should return Some(text) containing the selected \
         word {COPY_WORD:?}; got {selection:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn copy_outside_visual_mode_is_none() {
    let (core, _waiter, _file) = common::start_core_with_temp_md(COPY_FIXTURE);

    // Freshly opened => normal mode. Copy must be a no-op returning None (this is
    // the mode guard that also prevents a `y` operator-hang outside visual mode).
    assert!(
        !core.read_snapshot().is_visual_mode(),
        "precondition: normal mode before copying"
    );
    assert_eq!(
        core.copy_visual_selection(),
        None,
        "copy_visual_selection() must return None when not in visual mode"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn paste_inserts_text_into_the_active_grid() {
    // Empty buffer so the pasted token is the only content and is easy to find.
    let (core, waiter, _file) = common::start_core_with_temp_md("");

    core.paste(PASTE_TEXT.to_string());
    poll_until(
        &waiter,
        &format!(
            "the pasted text {PASTE_TEXT:?} on row 0 of the active grid; never means the paste \
             did not reach the buffer or was drawn elsewhere"
        ),
        || {
            core.read_snapshot()
                .active_grid()
                .row_text(0)
                .contains(PASTE_TEXT)
        },
        |&ok| ok,
    );

    core.shutdown().expect("shutdown should be Ok");
}
