//! Integration test for external-file reload (`autoread` + `:checktime`).
//!
//! Drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`) — no mocks, per the project's real-nvim policy.
//!
//! The behavior under test: `runtime/nvim/init.lua` sets `autoread = true` and
//! registers a `WolfAutoRead` augroup that runs `:checktime` on
//! `FocusGained`/`BufEnter`/`CursorHold`/`CursorHoldI`. When the file shown in a
//! buffer is changed on disk from OUTSIDE the editor (`prettier --write`,
//! `git checkout`), a `:checktime` re-reads the buffer — but ONLY when the
//! in-editor copy is UNMODIFIED, so a user's unsaved edits are never clobbered.
//!
//! ## The faithful trigger (why NOT `:doautocmd CursorHold`)
//!
//! The real production path is `FocusGained` (the user leaves the app, an external
//! tool rewrites the file, the user returns) plus the `CursorHold` idle catch-all.
//! A headless embedded nvim cannot emit `FocusGained` directly, so these tests
//! drive a GENUINE `CursorHold`:
//!
//!   1. lower `updatetime` to 200ms (test-only, via `:set updatetime=200` — NOT an
//!      init.lua change; it just makes the idle-fire fast enough to observe);
//!   2. rewrite the file on disk;
//!   3. send ONE non-modifying cursor-move interaction (`j` then `k`) to RESET the
//!      idle timer, so a fresh `CursorHold` fires AFTER the external change.
//!
//! `CursorHold` is one-shot per idle period (see `:help CursorHold`: "not fired
//! again until the user has pressed a key"), so pure continued idle after the
//! write would NOT re-fire it — the single cursor-move models the user returning
//! to / interacting with the app, which is the real interaction/`FocusGained`
//! path the harness cannot emit directly. This is NOT a redraw nudge applied
//! after the reload; the cursor-move IS the legitimate event that drives it.
//!
//! `:doautocmd CursorHold` is deliberately NOT used: it fires a SYNTHETIC
//! `CursorHold` from cmdline context, and with no following interaction the
//! async reload's repaint is never surfaced — an unfaithful trigger.
//!
//! ## Load-bearing vs guard split (read before trusting green)
//!
//! - `external_change_reloads_unmodified_buffer` is LOAD-BEARING: an UNMODIFIED
//!   buffer whose file changed on disk MUST reload. It is prompt-free by design
//!   (a clean `:checktime` re-read is silent), so `!is_prompt()` is asserted.
//!
//! - `modified_buffer_is_not_reloaded_by_checktime` is the GUARD: a buffer with
//!   unsaved in-editor edits MUST NOT be clobbered even when the file also changed
//!   on disk. That both-changed collision is the W12 case, and nvim MAY raise an
//!   async confirm prompt for it — which is EXPECTED and acceptable here (a
//!   `FileChangedShell` policy was scoped out of this change). So `!is_prompt()`
//!   is NOT load-bearing for this test; the real point is that the unsaved marker
//!   survives and the external text is never applied. Each poll iteration sends a
//!   dismissing `Enter` (non-modifying in normal mode) so the prompt cannot block.
//!
//! ## Change detection
//!
//! `:checktime` compares mtime/mode/size. Every rewrite below uses a DIFFERENT
//! byte length than the original (a size mismatch always triggers, regardless of
//! filesystem mtime granularity) AND is preceded by a ~1.1s sleep so a coarse
//! mtime clock still advances.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key, WolfCore};

/// Original fixture content; row 0's heading proves the initial open.
const ORIGINAL_DOC: &str = "# Original\nbody\n";

/// External rewrite for the load-bearing (unmodified) test. Deliberately a
/// DIFFERENT byte length than [`ORIGINAL_DOC`] so `:checktime`'s size check fires.
const RELOADED_DOC: &str = "# Reloaded externally by the test\nbody\n";

/// External rewrite for the guard (modified) test. Also a different byte length;
/// its heading (`never applied`) must NEVER reach the grid, since the modified
/// buffer must not be clobbered.
const NEVER_APPLIED_DOC: &str = "# External change never applied\nbody\n";

/// Wall-clock sleep before rewriting the fixture on disk, so a coarse-grained
/// mtime clock still records a change (the differing byte length covers the size
/// check regardless, but the sleep makes mtime detection robust too).
const PRE_REWRITE_SLEEP: Duration = Duration::from_millis(1100);

/// Settle after `:set updatetime=200` so the option is applied on nvim's loop
/// before the test proceeds (this is a separate input burst, not observable via a
/// redraw flush).
const SET_UPDATETIME_SETTLE: Duration = Duration::from_millis(300);

/// Gap between poll iterations while waiting for a reload (or a settle) to land.
/// A touch longer than the 200ms test `updatetime` so each iteration spans at
/// least one potential idle-fire window.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Marker text of the unsaved in-editor edit in the guard test. Chosen to be
/// unmistakable on the grid and absent from every fixture on disk.
const UNSAVED_MARKER: &str = "ZZUNSAVED";

/// Starts a `WolfCore` rooted at `cwd` and waits for the initial attach flush, so
/// startup never races the test body. Copied VERBATIM from `swap_exists.rs`'s
/// local `start_core_in` (it is intentionally not in `common`).
fn start_core_in(cwd: &Path) -> (WolfCore, common::FlushWaiter) {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(
            cwd.to_path_buf(),
            common::DEFAULT_COLS,
            common::DEFAULT_ROWS,
        ),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, common::FLUSH_TIMEOUT);
    (core, waiter)
}

/// Lowers `updatetime` to 200ms in the captive nvim via `:set updatetime=200`
/// typed as keystrokes, so `CursorHold` fires promptly on idle. TEST-ONLY (it
/// simulates the idle timing a real user session has over seconds); it is NOT an
/// init.lua change. Settles so the option lands before the test proceeds.
fn set_fast_updatetime(core: &WolfCore) {
    core.send_input(ch(':'));
    type_str(core, "set updatetime=200");
    core.send_input(key(Key::Enter));
    std::thread::sleep(SET_UPDATETIME_SETTLE);
}

/// Sends ONE non-modifying cursor-move interaction (`j` then `k`) in normal mode.
/// In the 2-line fixture this moves down then back up to line 1, leaving the
/// buffer UNMODIFIED and the cursor where it started — but it RESETS the idle
/// timer so a fresh `CursorHold` fires afterward. This is the faithful proxy for
/// the user returning to / interacting with the app (the `FocusGained` path the
/// headless harness cannot emit directly), NOT a post-reload repaint nudge.
fn nudge_cursor_move(core: &WolfCore) {
    core.send_input(ch('j'));
    core.send_input(ch('k'));
}

/// An UNMODIFIED buffer whose file was changed on disk from OUTSIDE the editor
/// must reload when a genuine `CursorHold` fires after the change. This is the
/// load-bearing case (grid-assertion idiom mirrored from `swap_exists.rs`).
#[test]
fn external_change_reloads_unmodified_buffer() {
    let _ = common::require_nvim_bin();

    // Own temp dir, canonicalized so the path lines up with nvim's own view of it
    // (macOS /tmp -> /private/tmp), like the other real-nvim tests.
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");

    let fixture = cwd.join("note.md");
    std::fs::write(&fixture, ORIGINAL_DOC).expect("failed to write fixture");

    let (core, waiter) = start_core_in(&cwd);

    // Open the fixture and confirm the ORIGINAL content is on the grid.
    core.send_input(InputEvent::Open {
        path: fixture.clone(),
    });
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);

    let row0 = core.read_snapshot().active_grid().row_text(0);
    assert!(
        row0.contains("# Original"),
        "the fixture must open and render its original heading; got row0 = {row0:?}"
    );

    // Make CursorHold fire promptly on idle (test-only), then flush stale wakes.
    set_fast_updatetime(&core);
    waiter.drain();

    // Change the file on disk from OUTSIDE the editor. Sleep first (coarse mtime)
    // then write DIFFERENT-length content (size check always trips).
    std::thread::sleep(PRE_REWRITE_SLEEP);
    std::fs::write(&fixture, RELOADED_DOC).expect("failed to rewrite fixture externally");

    // ONE non-modifying interaction resets the idle timer so a genuine CursorHold
    // fires AFTER the change (the faithful FocusGained/interaction proxy). Send no
    // further input.
    nudge_cursor_move(&core);

    // Poll for the reload: the reload notification is not guaranteed to ride a
    // single redraw flush, so loop draining wakes with short sleeps, bounded by
    // FLUSH_TIMEOUT, until the new heading appears (or panic with the last row).
    //
    // CRITICAL: never hold a snapshot read-guard across `drain()`/`sleep()`. The
    // redraw handler applies `grid_line`/`flush` via `write_async()`, which spins
    // on `try_write()` and FAILS while ANY reader is held — so a guard held across
    // the sleep STARVES the write-lock exactly when the reload's repaint would
    // land. We therefore read only OWNED values (`row_text` returns a `String`,
    // `is_prompt` a `bool`) and let each guard drop at the end of its statement,
    // BEFORE the drain/sleep.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        // Guard drops at the end of this statement (owned String).
        let current_row0 = core.read_snapshot().active_grid().row_text(0);
        if current_row0.contains("# Reloaded externally") {
            // Re-read only to check the prompt; guard drops immediately again.
            // The unmodified reload is silent — no confirm prompt may be pending.
            let is_prompt = core.read_snapshot().toast.is_prompt();
            assert!(
                !is_prompt,
                "an unmodified external reload must be silent (no prompt)"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {FLUSH_TIMEOUT:?} waiting for the buffer to reload the \
             external change; last observed row0 = {current_row0:?}"
        );
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// A buffer with UNSAVED in-editor modifications must NOT be reloaded by a
/// `:checktime`, even when the file also changed on disk (the W12 both-changed
/// case). The guard: the unsaved marker stays on the grid and the external text
/// is never applied. The W12 confirm prompt is EXPECTED here (a FileChangedShell
/// policy is out of scope), so it is dismissed each poll iteration with a
/// non-modifying `Enter` rather than asserted absent.
#[test]
fn modified_buffer_is_not_reloaded_by_checktime() {
    let _ = common::require_nvim_bin();

    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");

    let fixture = cwd.join("note.md");
    std::fs::write(&fixture, ORIGINAL_DOC).expect("failed to write fixture");

    let (core, waiter) = start_core_in(&cwd);

    core.send_input(InputEvent::Open {
        path: fixture.clone(),
    });
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);

    let row0 = core.read_snapshot().active_grid().row_text(0);
    assert!(
        row0.contains("# Original"),
        "the fixture must open and render its original heading; got row0 = {row0:?}"
    );

    // Make CursorHold fire promptly on idle (test-only), then flush stale wakes.
    set_fast_updatetime(&core);
    waiter.drain();

    // Make an UNSAVED in-editor edit at the start of the heading line: insert the
    // marker, then leave insert mode. The buffer is now modified (unsaved).
    core.send_input(ch('i'));
    type_str(&core, "ZZUNSAVED ");
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);

    let row0 = core.read_snapshot().active_grid().row_text(0);
    assert!(
        row0.contains(UNSAVED_MARKER),
        "the unsaved in-editor edit must be visible before the external change; \
         got row0 = {row0:?}"
    );

    // Change the file on disk too (different length). This is the both-changed
    // (W12) situation once the freshness check runs.
    std::thread::sleep(PRE_REWRITE_SLEEP);
    std::fs::write(&fixture, NEVER_APPLIED_DOC).expect("failed to rewrite fixture externally");

    // ONE non-modifying interaction resets the idle timer so a genuine CursorHold
    // fires AFTER the change (same faithful trigger as test 1).
    nudge_cursor_move(&core);

    // Settle window: throughout, the buffer must remain modified with the marker
    // intact and the external text must never appear. Because the buffer is
    // modified AND the file changed, nvim MAY raise the async W12 confirm prompt,
    // so each iteration sends a dismissing Enter (non-modifying in normal mode —
    // it just moves down a line — so it cannot corrupt the buffer or assertions).
    //
    // CRITICAL: never hold a snapshot read-guard across `send_input`/`drain`/`sleep`.
    // The redraw handler applies events via `write_async()`, which spins on
    // `try_write()` and FAILS while ANY reader is held — a guard held across the
    // sleep starves the write-lock. So we read `row_text(0)` into an OWNED String
    // and let the guard drop at the end of the statement, BEFORE the Enter send,
    // drain, and sleep.
    let settle_deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < settle_deadline {
        // Guard drops at the end of this statement (owned String).
        let row0 = core.read_snapshot().active_grid().row_text(0);
        // Fail fast if the external text ever reaches the grid (clobbered edit).
        assert!(
            !row0.contains("never applied"),
            "a modified buffer must NOT be reloaded with the external change; \
             external heading appeared on the grid: row0 = {row0:?}"
        );
        // Fail fast if the unsaved marker ever disappears (clobbered edit).
        assert!(
            row0.contains(UNSAVED_MARKER),
            "the unsaved marker must remain on the grid throughout (modified buffer \
             not clobbered); got row0 = {row0:?}"
        );
        core.send_input(key(Key::Enter));
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }

    // Final guard check. Read each value into an owned local so no guard is held
    // across the (final) reads either — same starvation discipline as the loop.
    let row0 = core.read_snapshot().active_grid().row_text(0);
    assert!(
        row0.contains(UNSAVED_MARKER),
        "the unsaved marker must STILL be on the grid after settle (modified buffer \
         not clobbered); got row0 = {row0:?}"
    );
    assert!(
        !row0.contains("never applied"),
        "the external change must never be applied to the modified buffer; \
         got row0 = {row0:?}"
    );
    // Best-effort only: the W12 confirm is acceptable for this collision, so this
    // is NOT load-bearing — the guard above is the real point. The per-iteration
    // Enter should have dismissed any prompt by now.
    let is_prompt = core.read_snapshot().toast.is_prompt();
    if is_prompt {
        eprintln!(
            "note: a W12 both-changed prompt is still pending after the dismissal \
             loop (acceptable — FileChangedShell policy is out of scope)"
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}
