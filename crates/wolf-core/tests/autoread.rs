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
//! ## The trigger: `set_ui_focus` (the REAL production path)
//!
//! The real production path is `FocusGained`: the file changes on disk while a tab is
//! unfocused (an external tool such as `prettier --write` or `git checkout`, or another
//! tab's Neovim saving it), then the tab regains focus. The front-end reports each tab's
//! focus to that tab's captive nvim through `WolfCore::set_ui_focus(gained)`, which calls
//! `nvim_ui_set_focus` → fires `FocusGained` → the `WolfAutoRead` autocmd runs
//! `:checktime` → the unmodified buffer reloads. These tests drive exactly that method,
//! so they exercise the true production trigger (not a proxy).
//!
//! Every test sends `set_ui_focus(false)` THEN `set_ui_focus(true)`: a loss and then a
//! gain, as a tab losing and regaining focus reports them. `nvim_ui_set_focus(true)`
//! fires `FocusGained` on every call, so the gain alone is what runs `:checktime`.
//!
//! [`a_save_in_one_core_reloads_in_another_after_the_focus_trigger`] is the per-tab
//! shape: two cores on one cwd, as two macOS tabs on one folder run, where one core's
//! save is the external change the other must pick up.
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

/// Gap between poll iterations while waiting for a reload (or a settle) to land.
/// The reload is driven by a `set_ui_focus` transition rather than idle timing, so
/// this only needs to be short enough to observe the async repaint promptly.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Marker text of the unsaved in-editor edit in the guard test. Chosen to be
/// unmistakable on the grid and absent from every fixture on disk.
const UNSAVED_MARKER: &str = "ZZUNSAVED";

/// Starts a `WolfCore` rooted at `cwd` and waits for the initial attach flush, so
/// startup never races the test body. Copied VERBATIM from `swap_recovery.rs`'s
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

/// Reports a focus loss and then a gain to the captive nvim UI, as a tab losing and
/// regaining focus does. The gain fires `FocusGained` → the `WolfAutoRead` autocmd →
/// `:checktime`.
fn refocus_ui(core: &WolfCore) {
    core.set_ui_focus(false);
    core.set_ui_focus(true);
}

/// Polls row 0 of the active grid until it contains `needle`, bounded by
/// [`FLUSH_TIMEOUT`], panicking with the last observed row on timeout. A single
/// `wait_for_flush` can return on an intermediate redraw before the text under
/// test is on the grid (especially under parallel-test contention), so callers
/// that need a specific string to be present must poll rather than assert once.
///
/// Same lock discipline as the reload polls: `row_text` is read into an OWNED
/// `String` so the snapshot read-guard drops BEFORE `drain()`/`sleep` — a guard
/// held across the sleep starves the redraw handler's `write_async` `try_write`.
fn wait_for_row0_contains(core: &WolfCore, waiter: &common::FlushWaiter, needle: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        // Guard drops at the end of this statement (owned String).
        let row0 = core.read_snapshot().active_grid().row_text(0);
        if row0.contains(needle) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {FLUSH_TIMEOUT:?} waiting for row 0 to contain {needle:?}; \
             last observed row0 = {row0:?}"
        );
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// An UNMODIFIED buffer whose file was changed on disk from OUTSIDE the editor
/// must reload when the UI regains focus (`set_ui_focus(true)` → `FocusGained` →
/// `:checktime`). This is the load-bearing case (grid-assertion idiom mirrored
/// from `swap_recovery.rs`).
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

    // Open the fixture and confirm the ORIGINAL content is on the grid. Poll (a
    // single flush can land before the opened buffer's content is on the grid).
    core.send_input(InputEvent::Open {
        path: fixture.clone(),
    });
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);
    wait_for_row0_contains(&core, &waiter, "# Original");

    // Change the file on disk from OUTSIDE the editor. Sleep first (coarse mtime)
    // then write DIFFERENT-length content (size check always trips).
    std::thread::sleep(PRE_REWRITE_SLEEP);
    std::fs::write(&fixture, RELOADED_DOC).expect("failed to rewrite fixture externally");

    // Flush stale wakes, then drive the REAL production trigger: a focus loss and gain
    // forwarded to nvim, whose gain fires FocusGained → :checktime.
    waiter.drain();
    refocus_ui(&core);

    // Poll for the reload, which may span flushes, and never hold a snapshot read guard across
    // drain() or sleep(): see crates/wolf-core/tests/README.md#autoreadrs
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
    wait_for_row0_contains(&core, &waiter, "# Original");

    // Make an unsaved edit at the start of the heading. Poll for the marker: under parallel
    // contention a single flush can return before the text reaches the grid.
    core.send_input(ch('i'));
    type_str(&core, "ZZUNSAVED ");
    core.send_input(key(Key::Esc));
    wait_for_row0_contains(&core, &waiter, UNSAVED_MARKER);

    // Change the file on disk too (different length). This is the both-changed
    // (W12) situation once the freshness check runs.
    std::thread::sleep(PRE_REWRITE_SLEEP);
    std::fs::write(&fixture, NEVER_APPLIED_DOC).expect("failed to rewrite fixture externally");

    // Drive the same REAL refocus trigger as test 1 (the gain fires FocusGained →
    // :checktime); the guard is that the modified buffer survives it.
    waiter.drain();
    refocus_ui(&core);

    // Settle: nvim may raise the async W12 prompt, so each pass sends an Enter, harmless in normal
    // mode. Read owned values only, as above.
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
    // Best-effort: a W12 confirm is acceptable here and the guard above is the real point; the
    // per-pass Enter should have dismissed any prompt.
    let is_prompt = core.read_snapshot().toast.is_prompt();
    if is_prompt {
        eprintln!(
            "note: a W12 both-changed prompt is still pending after the dismissal \
             loop (acceptable — FileChangedShell policy is out of scope)"
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// What core A types at the head of the shared file before saving it. Absent from
/// [`ORIGINAL_DOC`], so B's row 0 can show it only by reading A's save from disk.
const SAVED_BY_A: &str = "SAVEDBYA ";

/// Two cores on one cwd, as two macOS tabs on one folder run, show one file. A saves an
/// edit; B, unmodified, picks it up from disk on the focus trigger, and only then.
///
/// B's `'updatetime'` is raised so `CursorHold`, the other `WolfAutoRead` trigger, cannot
/// reload it while the test waits, and B enters no buffer, so `BufEnter` cannot either. The
/// premise that B still shows the old text after A's save is asserted before the trigger, so
/// a reload with no trigger fails the test rather than passing it.
///
/// FALSIFYING EDITS: (a) make `WolfCore::set_ui_focus` send nothing; (b) set
/// `opt.autoread = false` in `runtime/nvim/init.lua`. RED in both: B's row 0 never shows
/// A's text.
#[test]
fn a_save_in_one_core_reloads_in_another_after_the_focus_trigger() {
    let _ = common::require_nvim_bin();

    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    let fixture = cwd.join("note.md");
    std::fs::write(&fixture, ORIGINAL_DOC).expect("failed to write fixture");
    let probe = cwd.join("probe.txt");

    let (core_a, waiter_a) = start_core_in(&cwd);
    let (core_b, waiter_b) = start_core_in(&cwd);
    for (core, waiter) in [(&core_a, &waiter_a), (&core_b, &waiter_b)] {
        core.send_input(InputEvent::Open {
            path: fixture.clone(),
        });
        waiter.wait_for_flush(core, FLUSH_TIMEOUT);
        wait_for_row0_contains(core, waiter, "# Original");
    }
    common::send_ex(&core_b, "set updatetime=3600000");

    // A coarse mtime clock must still see A's write; the new length trips the size check too.
    std::thread::sleep(PRE_REWRITE_SLEEP);
    core_a.send_input(ch('i'));
    type_str(&core_a, SAVED_BY_A);
    core_a.send_input(key(Key::Esc));
    core_a.send_input(InputEvent::Save);
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut on_disk = String::new();
    while Instant::now() < deadline {
        on_disk = std::fs::read_to_string(&fixture).unwrap_or_default();
        if on_disk.starts_with(SAVED_BY_A) {
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    assert!(
        on_disk.starts_with(SAVED_BY_A),
        "premise: A's Save must write its edit to {}; the file holds {on_disk:?}",
        fixture.display()
    );

    common::settle_for(&waiter_b, Duration::from_millis(500));
    let before = core_b.read_snapshot().active_grid().row_text(0);
    assert!(
        before.contains("# Original") && !before.contains(SAVED_BY_A.trim()),
        "premise: before any focus trigger B must still show the old text; got row0 = \
         {before:?}. A's text here means something other than the focus trigger reloaded B \
         (CursorHold, BufEnter), so the assertions below could not tell the trigger works"
    );

    waiter_b.drain();
    refocus_ui(&core_b);
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut row0 = String::new();
    while Instant::now() < deadline {
        row0 = core_b.read_snapshot().active_grid().row_text(0);
        if row0.contains(SAVED_BY_A.trim()) {
            break;
        }
        waiter_b.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
    assert!(
        row0.contains(SAVED_BY_A.trim()),
        "after a focus loss and gain B must reload A's save from disk; row0 = {row0:?}. The \
         old text means no :checktime ran (set_ui_focus sent nothing, or FocusGained runs no \
         :checktime) or it ran without 'autoread' and asked instead of reloading"
    );

    let values = common::probe_values(&core_b, &["&modified", "expand('%:t')"], &probe);
    assert_eq!(
        values[1], "note.md",
        "the probe saw B's buffer {:?}, not note.md, so &modified below is about another \
         buffer",
        values[1]
    );
    assert_eq!(
        values[0], "0",
        "B's reloaded buffer must be unmodified, a plain re-read of the file; `1` means B's \
         copy differs from disk, as a merge or a recovered swap would leave it"
    );

    core_b.shutdown().expect("core B shutdown should be Ok");
    core_a.shutdown().expect("core A shutdown should be Ok");
}
