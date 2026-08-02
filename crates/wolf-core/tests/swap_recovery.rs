//! Integration tests for SWAP-BASED unsaved-edit recovery (architect plan §9
//! cases c + d). This file SUPERSEDES `swap_exists.rs`: with the new design
//! `'swapfile'` is ON always and `'directory'` is the durable, Wolf-owned
//! per-project swap dir (resolved like `state_dir()` from the core's cwd), so the
//! old "plant a `.<name>.swp` next to the file" provocation no longer matches.
//!
//! The recovery-aware `SwapExists` autocmd must NEVER raise a blocking prompt:
//!   * c. DEAD owner  → `v:swapchoice = "r"` (silently RECOVER the unsaved edit);
//!   * d. LIVE owner  → `v:swapchoice = "e"` (edit the clean on-disk copy anyway).
//!
//! Drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`) — no mocks.
//!
//! ## How the swap dir is controlled (read before trusting green)
//!
//! Per plan §6 the swap dir is NOT a `WOLF_SWAP_DIR` the test sets directly (that
//! env var is core → nvim, computed by the core); it is
//! `swap_dir_for(cwd) = state_dir()/swap/<hash(canonical cwd)>`. So the test lever
//! is (a) a STABLE `state_dir()` and (b) a FIXED cwd:
//!   * `common::config_in` already points `WOLF_STATE_DIR` at a process-global
//!     hermetic temp dir (set-if-absent, stable for the whole test binary), so the
//!     swap dir is stable across a restart (case c) and shared between two cores
//!     that use the SAME cwd (case d).
//!   * Each test canonicalizes and OWNS its cwd; case c reuses it across the
//!     restart, case d passes the SAME cwd to both cores. Distinct tests use
//!     distinct cwds, so their swap subdirs never collide.
//!
//! ## Dependency on coder work (tests-first)
//!
//! Won't compile until: `'swapfile'` on + durable `'directory'` + the
//! recovery-aware `SwapExists` autocmd (init.lua §6/§7), `swap_dir_for` wired
//! through `NvimConfig` (§6), and `WolfCore::{liveness, restart_nvim}` (§1/§5).
//!
//! ## Limits (documented, per plan §Constraints)
//!
//! SIGKILL skips `preserve_exit`, so the swap only holds edits synced BEFORE the
//! kill — both tests therefore force a deterministic `:preserve<CR>` to flush the
//! swap to disk before killing, closing that loss window.

mod common;

use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key, Liveness, WolfCore};

/// The clean on-disk fixture. Row 0's heading proves the buffer opened.
const CLEAN_DOC: &str = "# Swap Fixture\nbody line\n";

/// A distinctive unsaved-edit marker, absent from [`CLEAN_DOC`], inserted at the
/// buffer head so it lands on row 0 (`i` + marker + `Esc`).
const EDIT_MARKER: &str = "WOLFEDIT ";

/// A distinctive marker typed into the startup `Untitled.md` buffer for the
/// restart-shadow test, absent from every fixture so a wrong (or shadowing) buffer
/// cannot fake a pass.
const UNTITLED_MARKER: &str = "UNTITLEDSWAPRECOVER";

/// Gap between polls.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Wall-clock settle after `:preserve<CR>` so nvim flushes the swap to disk on its
/// own loop BEFORE the next command (the input bursts are otherwise unordered
/// relative to nvim's execution, and no redraw flush marks the write).
const PRESERVE_SETTLE: Duration = Duration::from_millis(300);

/// Polls `pred` until `true` (draining wakes each iteration) or panics after
/// [`FLUSH_TIMEOUT`]. `desc` names the awaited condition.
fn poll_until(waiter: &FlushWaiter, desc: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if pred() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
        }
        waiter.drain();
        sleep(POLL_INTERVAL);
    }
}

/// Types an Ex command `:<cmd><CR>` one key at a time.
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// SIGKILLs the captive nvim's own pid via the input pump (a crash: SIGKILL, NOT a
/// clean `qa!`), so the swap owner dies WITHOUT cleaning up its swap file.
fn self_sigkill(core: &WolfCore) {
    ex_command(core, "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)");
}

/// Inserts [`EDIT_MARKER`] at the buffer head (an in-memory edit differing from
/// disk), then leaves insert mode and waits for the change to render.
fn type_unsaved_edit(core: &WolfCore, waiter: &FlushWaiter) {
    core.send_input(ch('i'));
    type_str(core, EDIT_MARKER);
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(core, FLUSH_TIMEOUT);
}

/// Starts a `WolfCore` rooted at `cwd` and waits for the initial attach flush.
/// Mirrors `swap_exists.rs`'s local `start_core_in`. Both cores in case d go
/// through here with the SAME `cwd`, so they resolve the SAME durable swap dir.
fn start_core_in(cwd: &Path) -> (WolfCore, FlushWaiter) {
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

/// Opens `fixture` through the core and waits for the buffer to render.
fn open_fixture(core: &WolfCore, waiter: &FlushWaiter, fixture: &Path) {
    core.send_input(InputEvent::Open {
        path: fixture.to_path_buf(),
    });
    waiter.wait_for_flush(core, FLUSH_TIMEOUT);
}

/// An owned, canonicalized temp dir to use as a core cwd (macOS `/tmp` ->
/// `/private/tmp`, so nvim's own view of paths lines up). Returned with its guard.
fn owned_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// c. DEAD owner → silent recover: a fixture with an unsaved edit whose owning nvim
/// has DIED comes back with the edit recovered (`v:swapchoice = "r"`) and NO
/// pending confirm prompt (no block).
#[test]
fn dead_owner_recovers_silently() {
    let (_dir, cwd) = owned_cwd();
    let fixture = cwd.join("note.md");
    std::fs::write(&fixture, CLEAN_DOC).expect("failed to write fixture");

    let (core, waiter) = start_core_in(&cwd);
    open_fixture(&core, &waiter, &fixture);

    // Make an unsaved edit, then FORCE the swap to disk deterministically so the
    // SIGKILL below cannot race the swap sync.
    type_unsaved_edit(&core, &waiter);
    ex_command(&core, "preserve");
    sleep(PRESERVE_SETTLE);

    // Kill the swap owner (SIGKILL), then wait for the Dead transition.
    self_sigkill(&core);
    poll_until(
        &waiter,
        "liveness() to become Dead after the SIGKILL",
        || core.liveness() == Liveness::Dead,
    );

    // Restart in the SAME cwd (=> SAME durable swap dir), so the fresh nvim finds
    // the dead owner's swap.
    core.restart_nvim().expect("restart_nvim should be Ok");
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "nvim must be Alive after restart"
    );

    // Reopen the fixture: the recovery-aware SwapExists sees a DEAD owner and
    // silently recovers ("r"), so the buffer shows the unsaved edit.
    open_fixture(&core, &waiter, &fixture);
    poll_until(
        &waiter,
        "the recovered buffer to show the unsaved edit on row 0",
        || {
            core.read_snapshot()
                .active_grid()
                .row_text(0)
                .contains("WOLFEDIT")
        },
    );

    let snap = core.read_snapshot();
    let row0 = snap.active_grid().row_text(0);
    assert!(
        row0.contains("WOLFEDIT"),
        "the dead owner's unsaved edit must be recovered; got row0 = {row0:?}"
    );
    assert!(
        !snap.toast.is_prompt(),
        "recovery must be SILENT (swapchoice 'r'), never a confirm prompt; toast kind = {:?}",
        snap.toast.kind()
    );
    drop(snap);

    core.shutdown().expect("shutdown should be Ok");
}

/// d. LIVE owner → edit anyway: while owner A (ALIVE) holds a swap for an unsaved
/// edit, a second core B opening the SAME fixture sees a LIVE owner and edits the
/// CLEAN on-disk copy (`v:swapchoice = "e"`, NOT "r"), with NO confirm prompt.
#[test]
fn live_owner_edits_anyway() {
    // ONE cwd shared by both cores, so they resolve the SAME durable swap dir.
    let (_dir, cwd) = owned_cwd();
    let fixture = cwd.join("note.md");
    std::fs::write(&fixture, CLEAN_DOC).expect("failed to write fixture");

    // Core A: open, make an unsaved in-memory edit differing from disk, force the
    // swap to disk. A stays ALIVE (its nvim is the swap owner).
    let (core_a, waiter_a) = start_core_in(&cwd);
    open_fixture(&core_a, &waiter_a, &fixture);
    type_unsaved_edit(&core_a, &waiter_a);
    ex_command(&core_a, "preserve");
    sleep(PRESERVE_SETTLE);

    // Core B: open the SAME fixture. Its SwapExists sees a LIVE owner (A) and
    // chooses "e", so B shows the CLEAN on-disk content, NOT A's unsaved edit.
    let (core_b, waiter_b) = start_core_in(&cwd);
    open_fixture(&core_b, &waiter_b, &fixture);
    poll_until(
        &waiter_b,
        "B's buffer to render the clean on-disk heading",
        || {
            core_b
                .read_snapshot()
                .active_grid()
                .row_text(0)
                .contains("# Swap Fixture")
        },
    );

    let snap_b = core_b.read_snapshot();
    let row0 = snap_b.active_grid().row_text(0);
    assert!(
        row0.contains("# Swap Fixture"),
        "B must render the clean on-disk copy; got row0 = {row0:?}"
    );
    assert!(
        !row0.contains("WOLFEDIT"),
        "B must NOT recover the LIVE owner's unsaved edit (swapchoice 'e', not 'r'); \
         got row0 = {row0:?}"
    );
    assert!(
        !snap_b.toast.is_prompt(),
        "a live-owner swap must never raise a confirm prompt; toast kind = {:?}",
        snap_b.toast.kind()
    );
    drop(snap_b);

    // Shut both down cleanly (A last, so it owns the swap for B's whole lifetime).
    core_b.shutdown().expect("core B shutdown should be Ok");
    core_a.shutdown().expect("core A shutdown should be Ok");
}

/// RESTART-SHADOW guard (architect follow-up, round 2): an unsaved `Untitled.md` must
/// survive a crash-restart via swap recovery. On restart the fresh nvim's `VimEnter`
/// naming names its OWN startup buffer `<cwd>/Untitled.md` WITHOUT loading it; if the
/// subsequent Tier-1 reopen `:e <cwd>/Untitled.md` merely switched to that
/// already-named empty buffer, `SwapExists` would NOT fire and the dead owner's
/// unsaved edit would be lost ("shadowed"). This drives EXACTLY the `nvim_restart.rs`
/// Tier-1 reopen path (`open_files_for_tabpage` -> reopen) and asserts the typed text
/// is recovered, pinning whether the shadow happens.
#[test]
fn restart_recovers_unsaved_untitled_buffer() {
    // Fresh, canonicalized cwd (stable across the restart => stable durable swap dir).
    // Deliberately EMPTY: the startup buffer must be the anonymous `Untitled.md`.
    let (_dir, cwd) = owned_cwd();

    let (core, waiter) = start_core_in(&cwd);

    // Wait for the initial editor window, then capture tabpage 1.
    poll_until(&waiter, "the initial editor window after attach", || {
        !core.read_snapshot().windows.is_empty()
    });
    let tab = core.current_tabpage();
    assert_ne!(tab, 0, "current_tabpage() should be nonzero after attach");

    // The Tier-1 mirror must carry the startup Untitled.md (named-but-unsaved).
    poll_until(
        &waiter,
        "open_files_for_tabpage to mirror the startup Untitled.md",
        || match core.open_files_for_tabpage(tab) {
            Some(tf) => tf
                .files
                .iter()
                .any(|p| p.file_name().and_then(|s| s.to_str()) == Some("Untitled.md")),
            None => false,
        },
    );

    // Type a distinctive unsaved edit into the Untitled.md buffer (the active buffer),
    // then force the swap to disk deterministically so the SIGKILL cannot race the
    // swap sync (mirrors the other swap_recovery tests).
    core.send_input(ch('i'));
    type_str(&core, UNTITLED_MARKER);
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);
    ex_command(&core, "preserve");
    sleep(PRESERVE_SETTLE);

    // Capture the Tier-1 set BEFORE the restart (respawn wipes the mirror). Reopen
    // order: non-active first, ACTIVE last — EXACTLY as `nvim_restart.rs` does.
    let mirror = core
        .open_files_for_tabpage(tab)
        .expect("mirror must be present before the restart");
    let active = mirror.active.clone();
    let mut reopen: Vec<PathBuf> = mirror
        .files
        .iter()
        .filter(|p| active.as_deref() != Some(p.as_path()))
        .cloned()
        .collect();
    if let Some(a) = active {
        reopen.push(a);
    }
    assert!(
        reopen
            .iter()
            .any(|p| p.file_name().and_then(|s| s.to_str()) == Some("Untitled.md")),
        "the captured Tier-1 reopen set must include Untitled.md, got {reopen:?}"
    );

    // Crash nvim (SIGKILL: the swap owner dies WITHOUT cleaning up), wait for Dead.
    self_sigkill(&core);
    poll_until(
        &waiter,
        "liveness() to become Dead after the SIGKILL",
        || core.liveness() == Liveness::Dead,
    );

    // Restart in the SAME cwd (=> SAME durable swap dir) from a Dead state: an ACTUAL
    // respawn, so restart_nvim() returns Ok(true).
    let restarted = core
        .restart_nvim()
        .expect("restart_nvim should be Ok from a Dead state");
    assert!(
        restarted,
        "restart_nvim() from a Dead state must report an ACTUAL respawn (Ok(true))"
    );
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "nvim must be Alive after the restart"
    );
    let fresh_tab = core.current_tabpage();
    assert_ne!(
        fresh_tab, 0,
        "current_tabpage() must be valid after restart"
    );

    // Drive the reopen the front-end performs off the captured set. Reopening
    // <cwd>/Untitled.md is what must trigger the dead owner's swap recovery — even
    // though the fresh nvim's VimEnter already named its own startup buffer the same
    // name (the "shadow" risk this test exists to catch).
    core.set_current_tabpage(fresh_tab);
    for path in &reopen {
        core.send_input(InputEvent::Open { path: path.clone() });
    }
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // The recovered Untitled.md buffer must show the unsaved edit on row 0. If the
    // restart shadow occurred, SwapExists never fired and row 0 is empty — this fails
    // loudly, which is exactly the risk this test pins.
    poll_until(
        &waiter,
        "the recovered Untitled.md buffer to show the unsaved edit on row 0",
        || {
            core.read_snapshot()
                .active_grid()
                .row_text(0)
                .contains(UNTITLED_MARKER)
        },
    );
    let snap = core.read_snapshot();
    let row0 = snap.active_grid().row_text(0);
    assert!(
        row0.contains(UNTITLED_MARKER),
        "the crashed nvim's unsaved Untitled.md edit must be recovered across restart; \
         got row0 = {row0:?}"
    );
    assert!(
        !snap.toast.is_prompt(),
        "swap recovery of Untitled.md must be SILENT (swapchoice 'r'), never a confirm \
         prompt; toast kind = {:?}",
        snap.toast.kind()
    );
    drop(snap);

    core.shutdown().expect("shutdown should be Ok");
}
