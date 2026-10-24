//! Integration tests for SWAP-BASED unsaved-edit recovery (architect plan §9
//! cases c + d, plus the startup-naming case e added later). This file
//! SUPERSEDES `swap_exists.rs`: with the new design
//! `'swapfile'` is ON always and `'directory'` is the durable, Wolf-owned
//! per-project swap dir (resolved like `state_dir()` from the core's cwd), so the
//! old "plant a `.<name>.swp` next to the file" provocation no longer matches.
//!
//! The recovery-aware `SwapExists` autocmd must NEVER raise a blocking prompt:
//!   * c. DEAD owner  → `v:swapchoice = "r"` (silently RECOVER the unsaved edit);
//!   * d. LIVE owner  → `v:swapchoice = "e"` (edit the clean on-disk copy anyway).
//!
//! And a fourth case that `SwapExists` CANNOT cover, guarded by name avoidance
//! instead:
//!   * e. STARTUP NAMING — a stale swap for `<cwd>/Untitled.md`, left behind by a
//!     crashed nvim, must not block the next COLD start. `init.lua`'s `VimEnter`
//!     naming calls `nvim_buf_set_name(buf, "<cwd>/Untitled.md")`, and Neovim
//!     creates the buffer's swap file INSIDE that rename (`rename_buffer` →
//!     `setfname` → `buf_name_changed` → `ml_setname` → `ml_open_file`, because
//!     `'updatecount'` is 200 and the buffer is `'swapfile'`). On that code path
//!     Neovim never arms the choice machinery (`swap_exists_action` stays
//!     `SEA_NONE`), so `SwapExists` is NOT raised and a collision goes straight to
//!     the blocking `do_dialog` ATTENTION/E325 prompt (`mode() == "r?"`) — which
//!     silently swallows the user's next keystroke. See neovim/neovim#23145. The
//!     defence is therefore to AVOID the colliding name:
//!     `_G.wolf_untitled_swap_exists` makes the startup buffer take
//!     `Untitled 2.md`, and the Tier-1 reopen of `<cwd>/Untitled.md` (case c
//!     above) is what recovers the crashed edit. Pinned by
//!     [`stale_untitled_swap_does_not_block_a_fresh_start`].
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
//! ### Why one test's cwd must live under `$HOME` (read before "simplifying" it)
//!
//! Cases c and d use [`owned_cwd`], a plain `tempfile::tempdir()`. Case e uses
//! [`home_cwd`] instead, and that difference is LOAD-BEARING. Neovim writes the
//! swap owner's path into the swap header through `home_replace`, which rewrites a
//! path under `$HOME` to a home-relative form (on Unix additionally rewriting the
//! leading `~/` to `~<username>/`). Measured: a swap for
//! `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/Untitled.md` reads back as
//! `fname = "~jplein/Git/jplein/wolf/crates/wolf-gnome/Untitled.md"`. Under `/tmp`
//! (or macOS's `/private/var/folders/…`) `home_replace` does nothing, so the
//! HISTORIC, BUGGY `info.fname == abspath` comparison in
//! `_G.wolf_untitled_swap_exists` already matched — which is precisely why every
//! pre-existing test passed while the defence was dead for every real project.
//! A case-e test at a `/tmp` cwd would be green with OR without the fix.
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
//! kill — every test that kills its nvim therefore forces a deterministic
//! `:preserve<CR>` to flush the swap to disk before killing, closing that loss
//! window.

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

/// The marker [`stale_untitled_swap_does_not_block_a_fresh_start`] types into the
/// COLD-restarted startup buffer.
///
/// Character set is deliberate, because this marker is typed blind: when the
/// product bug is present the leading `i` is eaten by the blocking dialog and the
/// remaining characters are interpreted, so none of them may do something
/// destructive or modal.
///
/// * No `Z` (`ZZ` writes and quits), no `q` (starts macro recording, which then
///   eats the rest), no `:` (opens the command line) — the NORMAL-mode hazards.
/// * No `Q` and no `D` — those are the ATTENTION dialog's "(Q)uit" and
///   "(D)elete it" answers, and `D` would delete the very swap file that is this
///   test's fixture. (`O`, which the marker does contain, only answers "Open
///   Read-Only": harmless, and the test still fails because row 0 cannot then hold
///   the contiguous marker.)
///
/// It is also absent from [`CLEAN_DOC`], [`EDIT_MARKER`] and [`UNTITLED_MARKER`],
/// so no other fixture in this file can fake a pass.
const STARTUP_MARKER: &str = "WOLFSTALESWAP";

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

/// Like [`owned_cwd`], but the temp dir is created **inside `$HOME`**. Returned
/// with its guard and already canonicalized, so it drops in wherever `owned_cwd`
/// would.
///
/// # Why this cannot be `owned_cwd()`
///
/// This is the whole point of [`stale_untitled_swap_does_not_block_a_fresh_start`]
/// and must not be "simplified" away. Neovim stores the swap owner's path in the
/// swap header (`b0_fname`) through `home_replace`, which rewrites a path under
/// `$HOME` to a home-relative form and, on Unix, additionally rewrites the leading
/// `~/` to `~<username>/`. Measured on this machine, the swap for
/// `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/Untitled.md` reads back as
/// `fname = "~jplein/Git/jplein/wolf/crates/wolf-gnome/Untitled.md"` — NOT the
/// absolute path.
///
/// `home_replace` fires ONLY for a path under `$HOME`. A `tempfile::tempdir()` cwd
/// lives at `/tmp/…` (or `/private/var/folders/…` on macOS), so `swapinfo().fname`
/// comes back plain and absolute there, the HISTORIC `info.fname == abspath`
/// comparison in `_G.wolf_untitled_swap_exists` already matched, and a test written
/// against such a cwd is green with OR without the fix. That blind spot is exactly
/// how the bug shipped: every fixture in the suite sat outside `$HOME` while every
/// real user project sits inside it.
///
/// # Why it panics instead of skipping
///
/// `HOME` unset or empty is a broken environment, not a reason to quietly drop the
/// only guard for a user-visible input-loss bug. This suite fails loud everywhere
/// else (`require_nvim_bin`), and a skip here would look like a pass.
fn home_cwd() -> (tempfile::TempDir, PathBuf) {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|h| !h.as_os_str().is_empty())
        .unwrap_or_else(|| {
            panic!(
                "HOME is unset or empty, so this test cannot build the fixture it needs. \
                 `stale_untitled_swap_does_not_block_a_fresh_start` MUST run in a cwd UNDER \
                 $HOME: Neovim `home_replace`s the owner path it stores in a swap file only \
                 for paths under $HOME (rewriting the leading `~/` to `~<username>/`), and \
                 that rewrite is the exact thing this test pins. At a /tmp cwd the test would \
                 pass whether or not the product bug is fixed. Point HOME at a writable \
                 directory and re-run. This panics rather than skipping on purpose."
            )
        });
    let dir = tempfile::Builder::new()
        .prefix("wolf-swap-home-")
        .tempdir_in(&home)
        .unwrap_or_else(|e| {
            panic!(
                "failed to create a temp cwd under HOME ({}): {e}",
                home.display()
            )
        });
    let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize $HOME temp cwd");
    (dir, cwd)
}

/// The durable, Wolf-owned swap dir for `cwd` (`state_dir()/swap/<hash>`).
///
/// Call only AFTER the first [`start_core_in`] for that cwd, so `common::config_in`
/// has already pointed the process-global `WOLF_STATE_DIR` at this binary's
/// hermetic temp dir — otherwise this resolves against (or fails to resolve) the
/// developer's real state dir.
fn swap_dir_of(cwd: &Path) -> PathBuf {
    wolf_core::state::swap_dir_for(cwd)
        .expect("swap_dir_for should resolve once WOLF_STATE_DIR is set by common::config_in")
}

/// The swap files under `cwd`'s durable swap dir that belong to the UN-NUMBERED
/// startup buffer `<cwd>/Untitled.md`.
///
/// `'directory'` ends in `//`, so Neovim names a swap after the buffer's FULL path
/// with every `/` replaced by `%`, plus a `.swp` suffix — e.g.
/// `%home%jplein%wolf-swap-home-ab12%Untitled.md.swp`. Selecting on "file name
/// CONTAINS `Untitled.md`" AND "ends with `.swp`" therefore picks out exactly that
/// buffer's swap: `Untitled 2.md.swp` does NOT contain the substring `Untitled.md`,
/// so a swap belonging to the ADVANCED name can never be mistaken for the fixture.
///
/// A missing swap dir yields an empty vec rather than an error — callers assert on
/// emptiness themselves, with a message that reports the dir and its real contents.
fn untitled_swap_files(cwd: &Path) -> Vec<PathBuf> {
    let dir = swap_dir_of(cwd);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains("Untitled.md") && n.ends_with(".swp"))
        })
        .collect()
}

/// A human-readable "<swap dir> containing [<every entry>]" line for assertion
/// messages, so a failure says what actually was (or was not) on disk.
fn swap_dir_report(cwd: &Path) -> String {
    let dir = swap_dir_of(cwd);
    let listing: Vec<String> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(e) => vec![format!("<unreadable: {e}>")],
    };
    format!("{} containing {listing:?}", dir.display())
}

/// The file BASENAMES the Tier-1 open-files mirror carries for `tab`, or an empty
/// vec when the mirror has not landed yet.
fn mirror_basenames(core: &WolfCore, tab: i64) -> Vec<String> {
    let Some(tf) = core.open_files_for_tabpage(tab) else {
        return Vec::new();
    };
    tf.files
        .iter()
        .filter_map(|p| p.file_name().and_then(|s| s.to_str()))
        .map(String::from)
        .collect()
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
/// survive a crash-restart via swap recovery. The `:preserve` + SIGKILL below leaves a
/// stale swap for `<cwd>/Untitled.md`, so on restart the fresh nvim's `VimEnter` naming
/// consults `_G.wolf_untitled_swap_exists`, sees that swap and names its OWN startup
/// buffer `<cwd>/Untitled 2.md` — leaving `<cwd>/Untitled.md` unclaimed, so the
/// subsequent Tier-1 reopen `:e <cwd>/Untitled.md` genuinely LOADS the file, `SwapExists`
/// fires on a dead owner and silently recovers (`v:swapchoice = "r"`). Without that name
/// avoidance the startup buffer would itself have taken `<cwd>/Untitled.md` WITHOUT
/// loading it, the reopen would merely have switched to that already-named empty buffer,
/// `SwapExists` would NOT have fired and the dead owner's unsaved edit would be lost
/// ("shadowed"). This drives EXACTLY the `nvim_restart.rs` Tier-1 reopen path
/// (`open_files_for_tabpage` -> reopen) and asserts the typed text is recovered, so it is
/// the end-to-end guard that the shadow stays PREVENTED.
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

    // Type an unsaved edit into Untitled.md, then force the swap to disk so the SIGKILL cannot race
    // the swap sync.
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

    // Reopening <cwd>/Untitled.md must recover the dead owner's swap. It is a real load only because
    // the fresh nvim named its startup buffer `Untitled 2.md`; otherwise that buffer would shadow it.
    core.set_current_tabpage(fresh_tab);
    for path in &reopen {
        core.send_input(InputEvent::Open { path: path.clone() });
    }
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // The recovered buffer must show the edit on row 0; under the restart shadow SwapExists never
    // fires and row 0 is empty.
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

/// e. STARTUP NAMING — a stale swap for `<cwd>/Untitled.md` left by a crashed nvim
/// must not block the next COLD start, and must not swallow the user's first
/// keystroke.
///
/// This is the headless, deterministic guard for the bug the cage smoke only ever
/// caught by accident (by self-poisoning its own fixed cwd): open Wolf after a
/// crash and the very first character you type vanishes.
///
/// ## The mechanism being pinned
///
/// `init.lua`'s `VimEnter` naming calls `nvim_buf_set_name(buf,
/// "<cwd>/Untitled.md")`. Neovim creates the buffer's swap file INSIDE that rename
/// (`ml_setname` → `ml_open_file`), on a path where it never arms the swap-choice
/// machinery — so `SwapExists` does NOT fire and a pre-existing swap for that exact
/// name goes straight to the blocking `do_dialog` ATTENTION/E325 prompt. The
/// surrounding `pcall` gives zero protection: the dialog is not an error, it blocks
/// and consumes input. The only defence is name avoidance —
/// `_G.wolf_untitled_swap_exists` must see the stale swap so the startup buffer
/// takes `Untitled 2.md` instead.
///
/// ## Shape
///
/// Deliberately a COLD start (a second, independent `WolfCore` over the same cwd,
/// hence the same durable swap dir), not the in-process `restart_nvim` that
/// [`restart_recovers_unsaved_untitled_buffer`] already covers — a cold start is
/// the shape that actually shipped broken.
///
/// The cwd comes from [`home_cwd`], NOT [`owned_cwd`]. See `home_cwd`'s docs: at a
/// `/tmp` cwd this test passes whether or not the product bug is fixed.
#[test]
fn stale_untitled_swap_does_not_block_a_fresh_start() {
    // 1. A cwd UNDER $HOME. Load-bearing — see `home_cwd`.
    let (_home_guard, cwd) = home_cwd();

    // 2. Core A: the session that crashes and leaves the fixture behind.
    let (core_a, waiter_a) = start_core_in(&cwd);

    poll_until(&waiter_a, "A's initial editor window after attach", || {
        !core_a.read_snapshot().windows.is_empty()
    });
    let tab_a = core_a.current_tabpage();
    assert_ne!(tab_a, 0, "current_tabpage() should be nonzero after attach");

    // 3. Precondition: in a fresh cwd the startup buffer takes the un-numbered name, or the fixture
    //    below is a swap for some other name and the test proves nothing.
    poll_until(
        &waiter_a,
        "A's Tier-1 mirror to carry the un-numbered startup Untitled.md \
         (precondition: a fresh cwd must yield the un-numbered name)",
        || {
            mirror_basenames(&core_a, tab_a)
                .iter()
                .any(|n| n == "Untitled.md")
        },
    );

    // 4. Type into it and force the swap to disk deterministically, so the SIGKILL
    //    below cannot race the swap sync (mirrors the sibling tests).
    core_a.send_input(ch('i'));
    type_str(&core_a, STARTUP_MARKER);
    core_a.send_input(key(Key::Esc));
    waiter_a.wait_for_flush(&core_a, FLUSH_TIMEOUT);
    ex_command(&core_a, "preserve");
    sleep(PRESERVE_SETTLE);

    // 5. ASSERT THE FIXTURE EXISTS. Without this the test could silently prove
    //    nothing: no swap on disk means nothing for the fresh start to collide with.
    assert!(
        !untitled_swap_files(&cwd).is_empty(),
        "precondition failed: after typing into <cwd>/Untitled.md and `:preserve`, the \
         durable swap dir must hold a `%…%Untitled.md.swp`. Found {}. Without that file \
         this test has no fixture and proves nothing.",
        swap_dir_report(&cwd)
    );

    // 6. Crash the swap owner (SIGKILL, NOT a clean quit) so the swap is orphaned.
    self_sigkill(&core_a);
    poll_until(
        &waiter_a,
        "A's liveness() to become Dead after the SIGKILL",
        || core_a.liveness() == Liveness::Dead,
    );

    // 7. ASSERT THE FIXTURE SURVIVED THE KILL. A SIGKILLed nvim must leave its swap
    //    behind; that stale swap IS this test's fixture.
    assert!(
        !untitled_swap_files(&cwd).is_empty(),
        "a SIGKILLed nvim must leave its swap behind — that stale swap IS this test's \
         fixture. Found {}.",
        swap_dir_report(&cwd)
    );

    // 8. Reap the dead child. `shutdown()` is Ok for an already-dead core; the
    //    Result is deliberately discarded because A is expected to be gone.
    let _ = core_a.shutdown();

    // 9. A COLD start in the SAME cwd => the SAME durable swap dir, so the fresh
    //    nvim's VimEnter naming meets the stale swap.
    let (core_b, waiter_b) = start_core_in(&cwd);

    poll_until(&waiter_b, "B's initial editor window after attach", || {
        !core_b.read_snapshot().windows.is_empty()
    });
    let tab_b = core_b.current_tabpage();
    assert_ne!(
        tab_b, 0,
        "current_tabpage() should be nonzero after B's attach"
    );

    // 10. ASSERTION A — the mechanism. The startup naming must ADVANCE past the
    //     name whose swap is stale.
    poll_until(
        &waiter_b,
        "B's Tier-1 mirror to carry `Untitled 2.md` (the startup naming must advance \
         past the stale swap for `Untitled.md`; a mirror that never lands at all is \
         itself a symptom — nvim is blocked in the ATTENTION dialog)",
        || {
            mirror_basenames(&core_b, tab_b)
                .iter()
                .any(|n| n == "Untitled 2.md")
        },
    );
    let names_b = mirror_basenames(&core_b, tab_b);
    assert!(
        names_b.iter().any(|n| n == "Untitled 2.md"),
        "`_G.wolf_untitled_swap_exists` did not match the stale swap for \
         `{}/Untitled.md`; the startup naming must advance to `Untitled 2.md`. Under $HOME \
         this is the `swapinfo().fname` home-relative `~user` rewrite: nvim stores the \
         owner path `home_replace`d, so an `info.fname == abspath` comparison never \
         matches. Mirror carried {names_b:?}; swap dir is {}.",
        cwd.display(),
        swap_dir_report(&cwd)
    );
    assert!(
        !names_b.iter().any(|n| n == "Untitled.md"),
        "the fresh start must NOT name its startup buffer `Untitled.md` while a stale swap \
         for `{}/Untitled.md` exists: `nvim_buf_set_name` creates the swap file inside the \
         rename and raises a BLOCKING ATTENTION dialog there. Mirror carried {names_b:?}; \
         swap dir is {}.",
        cwd.display(),
        swap_dir_report(&cwd)
    );

    // 11. Assertion B, primary: type blind as a user would. If the leading `i` answers a dialog, at
    //     most len-1 chars land, and `contains` demands the whole marker.
    core_b.send_input(ch('i'));
    type_str(&core_b, STARTUP_MARKER);
    core_b.send_input(key(Key::Esc));
    poll_until(
        &waiter_b,
        "B's row 0 to show the blind-typed startup marker — if it never appears the FIRST \
         keystroke was swallowed by a blocking `do_dialog` ATTENTION prompt raised by \
         `nvim_buf_set_name` finding a stale swap (`mode() == \"r?\"`)",
        || {
            core_b
                .read_snapshot()
                .active_grid()
                .row_text(0)
                .contains(STARTUP_MARKER)
        },
    );
    let snap_b = core_b.read_snapshot();
    let row0 = snap_b.active_grid().row_text(0);
    assert!(
        row0.contains(STARTUP_MARKER),
        "the FIRST keystroke was swallowed — nvim was in MODE_CONFIRM (`mode() == \"r?\"`), \
         a blocking `do_dialog` ATTENTION prompt raised by `nvim_buf_set_name` finding a \
         stale swap; see `runtime/nvim/init.lua`'s anonymous-buffer naming section. The \
         leading `i` answered the dialog instead of entering insert mode, so \
         {STARTUP_MARKER:?} ran as NORMAL-mode commands. got row0 = {row0:?}; swap dir is \
         {}.",
        swap_dir_report(&cwd)
    );

    // 12. Assertion C, diagnostic: `do_dialog` under `ext_messages` emits a `confirm` `msg_show`,
    //     which `Toast::is_prompt` detects, naming the cause directly.
    assert!(
        !snap_b.toast.is_prompt(),
        "a fresh start over a stale `Untitled.md` swap must never raise a confirm prompt \
         (that is the blocking ATTENTION dialog); toast kind = {:?}",
        snap_b.toast.kind()
    );
    drop(snap_b);

    core_b.shutdown().expect("core B shutdown should be Ok");
}
