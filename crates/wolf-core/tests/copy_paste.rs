//! Integration tests for the COPY / PASTE surface against a real nvim (plan §6.2):
//! visual-mode detection (`Snapshot::is_visual_mode`), yanking a visual selection
//! out of the captive nvim (`WolfCore::copy_visual_selection`), and pasting text
//! back in (`WolfCore::paste`).
//!
//! Requires a real, pinned nvim via `WOLF_NVIM_BIN` (or the Bazel-staged tree),
//! reusing `tests/common/mod.rs`. Modeled on `input_keys.rs` / `redraw_grid.rs`:
//! type keys through `send_input`, wait for a redraw flush, then assert on the
//! snapshot / grid.
//!
//! ## Dependency on coder work (plan §3.3–§3.5)
//!
//! These do NOT exist yet — the coder adds them, so this file will not compile
//! until then (expected for test-first):
//!   * `WolfCore::copy_visual_selection(&self) -> Option<String>`
//!   * `WolfCore::paste(&self, text: String)` (fire-and-forget `InputEvent::Paste`)
//!   * `Snapshot::is_visual_mode(&self) -> bool` (true for `v` / `V` / `<C-v>`)

mod common;

use common::{ch, ctrl, key, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Key, WolfCore};

/// Bounded number of redraw flushes to wait through for a mode transition to
/// settle. `wait_for_flush` returns on ANY `seq` advance, so a stray startup-tail
/// redraw can satisfy a single wait before the `v`/`V`/`<C-v>`/`<Esc>` mode_change
/// is applied. Polling `is_visual_mode()` across a few flushes lets the transition
/// settle before we assert, without inventing a new harness helper.
const MODE_SETTLE_TRIES: usize = 8;

/// Polls `is_visual_mode()` until it equals `expected`, waiting through up to
/// [`MODE_SETTLE_TRIES`] redraw flushes (each bounded by [`FLUSH_TIMEOUT`]) so a
/// stray pre-mode_change flush cannot race the assertion. Returns as soon as the
/// snapshot settles; asserts once the bound is hit otherwise.
fn wait_until_visual_mode(core: &WolfCore, waiter: &FlushWaiter, expected: bool) {
    for _ in 0..MODE_SETTLE_TRIES {
        if core.read_snapshot().is_visual_mode() == expected {
            return;
        }
        waiter.wait_for_flush(core, FLUSH_TIMEOUT);
    }
    assert_eq!(
        core.read_snapshot().is_visual_mode(),
        expected,
        "is_visual_mode() did not settle to {expected} within {MODE_SETTLE_TRIES} flushes"
    );
}

/// A single-line fixture whose first WORD is a distinctive, space-free token so a
/// `v` + `e` (extend to word end) selects exactly that word and the yanked text is
/// unambiguous.
const COPY_FIXTURE: &str = "WOLFCOPY selection target\n";

/// The distinctive first word of [`COPY_FIXTURE`]. `ve` from the line start
/// selects exactly this token (visual is inclusive of the end char), so the
/// yanked unnamed register must CONTAIN it. NOTE to coder: this pins the copy
/// path to nvim's visual-`y` semantics (unnamed register `"`); the yank must be
/// synchronous so the register is populated before `copy_visual_selection`
/// returns.
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
    for enter in [ch('v'), ch('V'), ctrl(Key::Char('v'))] {
        waiter.drain();
        core.send_input(enter);
        // Poll until the visual mode_change settles: a stray startup-tail redraw
        // can satisfy a single wait_for_flush before the v/V/<C-v> is applied.
        wait_until_visual_mode(&core, &waiter, true);

        waiter.drain();
        core.send_input(key(Key::Esc));
        // Same hazard on the way back: poll until <Esc> returns us to normal mode.
        wait_until_visual_mode(&core, &waiter, false);
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
    // Poll until visual mode settles before copying: a stray pre-mode_change
    // flush must not let us call copy_visual_selection() while still in normal mode.
    wait_until_visual_mode(&core, &waiter, true);

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

    waiter.drain();
    core.paste(PASTE_TEXT.to_string());
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    let snap = core.read_snapshot();
    let row0 = snap.active_grid().row_text(0);
    assert!(
        row0.contains(PASTE_TEXT),
        "the pasted text {PASTE_TEXT:?} should appear on row 0 of the active grid; \
         got {row0:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}
