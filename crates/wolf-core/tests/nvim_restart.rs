//! Integration test for RESTART + Tier-1 RESTORE: when the captive Neovim dies,
//! `WolfCore::restart_nvim()` respawns it in place, and the front-end reopens the
//! file-backed buffers that were open (the Tier-1 open-file mirror captured
//! BEFORE the restart wiped it).
//!
//! Tier-1 now also includes named-but-unsaved `Untitled.md` buffers (the
//! anonymous-buffer feature names the startup buffer `Untitled.md`). Reopening such a
//! buffer via `:e` is exactly what drives Neovim's swap-based recovery of unsaved
//! edits, so it legitimately joins the crash-reopen set; excluding it would
//! permanently lose those edits.
//!
//! Drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`) — no mocks.
//!
//! ## Surface under test
//!
//!   * `WolfCore::liveness(&self) -> wolf_core::Liveness`.
//!   * `WolfCore::restart_nvim(&self) -> Result<(), _>` — kill-if-needed, respawn
//!     in the same cwd, re-run the setup chain; post-`Ok` contract: `liveness()`
//!     is `Alive`, `current_tabpage()` is the fresh nvim's tabpage-1, and
//!     `list_buffers()` is empty.
//!   * `WolfCore::open_files_for_tabpage(handle: i64) -> Option<TabFiles>` where
//!     `TabFiles { active: Option<PathBuf>, files: Vec<PathBuf> }` — the
//!     out-of-band Tier-1 mirror (buflisted file set + active per tabpage).
//!
//! ## Conventions (mirroring `tabpages.rs`)
//!
//! The mirror + buffer list arrive across flushes / out-of-band notifications, so
//! we POLL with a generous timeout, draining wakes. macOS `/tmp`/`/var` are
//! symlinks, so every path comparison is canonicalized.

mod common;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{
    ch, key, poll_until, probe_values, temp_md, type_str, FlushWaiter, FLUSH_TIMEOUT, POLL_INTERVAL,
};
use wolf_core::{InputEvent, Key, Liveness, WinBuffer, WolfCore};

/// Distinctive fixture bodies so a wrong buffer cannot fake a pass.
const DOC_A: &str = "# Alpha fixture\n";
/// The second fixture's body.
const DOC_B: &str = "# Bravo fixture\n";

/// The scheme every Neovim terminal buffer's name begins with
/// (`term://<cwd>//<pid>:<command>`).
///
/// Re-typed here as a literal rather than read from core, so a change to the
/// production constant fails this test instead of moving with it.
const TERMINAL_SCHEME: &str = "term://";

/// How long to let late `wolf_open_files` emits land before the mirror is read.
///
/// Generous, and deliberately a settle rather than another poll — see
/// [`open_files_omits_a_terminal_buffer`] for why no level-triggered predicate can
/// do this job. Mirrors `nav_history_live.rs`'s `HISTORY_SETTLE`.
const MIRROR_SETTLE: Duration = Duration::from_secs(2);

/// How long to wait after the sacrificial key so the pump has taken it as a batch of its own
/// and is blocked on the toast-clear write lock. Far above `INPUT_COALESCE_DELAY` (8 ms), so
/// the events sent after it cannot join that batch.
const PUMP_STALL_SETTLE: Duration = Duration::from_millis(500);

/// How long to let the queued events land on the fresh nvim, and their redraws on the
/// snapshot, before the command line is read.
const STALE_INPUT_SETTLE: Duration = Duration::from_secs(1);

/// Text pasted while the pump is stalled across the restart. Distinct from every other marker
/// and from anything a fresh nvim's buffer could hold.
const STALE_PASTE_MARKER: &str = "stalepastekilo";

/// Text typed in insert mode while the pump is stalled across the restart.
const STALE_KEY_MARKER: &str = "stalekeyslima";

/// An Ex command typed after `:` while the pump is stalled, never followed by Enter.
const STALE_HALF_TYPED_CMD: &str = "echo 4711";

/// Text typed after the restart, the positive control that post-swap typing is delivered.
const CONTROL_MARKER: &str = "freshkeysmike";

/// The grid size requested while the pump is stalled. Distinct from the defaults the core
/// starts and respawns at, so `&columns` tells "applied" from "dropped".
const STALE_COLS: u16 = common::DEFAULT_COLS + 20;
/// See [`STALE_COLS`].
const STALE_ROWS: u16 = common::DEFAULT_ROWS + 6;

/// Burns `dur` draining wakes, so late out-of-band notifications land BEFORE the
/// assertion that reads what they carry. Mirrors `nav_history_live.rs::settle`.
fn settle(waiter: &FlushWaiter, dur: Duration) {
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        waiter.drain();
        sleep(POLL_INTERVAL);
    }
}

/// Whether `b` is the drawer's window AND its terminal buffer already carries its
/// `term://<cwd>//<pid>:<cmd>` name.
///
/// All three clauses matter to [`open_files_omits_a_terminal_buffer`]: `is_drawer`
/// separates the drawer from a `:terminal` in an ordinary split, `is_terminal` says
/// a job is really running in it, and the name is what `init.lua`'s `listed_files()`
/// keys on — an unnamed terminal buffer is dropped from the mirror by a rule that
/// has nothing to do with the filter under test.
fn is_named_drawer(b: &WinBuffer) -> bool {
    b.is_terminal
        && b.is_drawer
        && b.path
            .as_deref()
            .and_then(|p| p.to_str())
            .is_some_and(|s| s.starts_with(TERMINAL_SCHEME))
}

/// Types an Ex command `:<cmd><CR>` one key at a time.
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// SIGKILLs the captive nvim's own pid via the input pump (a crash).
fn self_sigkill(core: &WolfCore) {
    ex_command(core, "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)");
}

/// SIGKILLs the captive nvim 300 ms after the `:lua` command that schedules it has finished.
///
/// Unlike [`self_sigkill`], nvim dies after its `cmdline_hide` flush, so the snapshot's
/// `cmdline` is not left `active`. Nothing resets `Snapshot.cmdline` across a restart.
fn deferred_sigkill(core: &WolfCore) {
    ex_command(
        core,
        "lua vim.defer_fn(function() (vim.uv or vim.loop).kill(vim.fn.getpid(), 9) end, 300)",
    );
}

/// Canonicalizes `p`, falling back to the raw path when it cannot be resolved
/// (the mirror uses the same raw fallback), for symlink-robust comparison.
fn norm(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Waits for the initial editor window after attach, then returns the first
/// tabpage handle (mirrors `tabpages.rs::attach_and_first_tabpage`).
fn attach_and_first_tabpage(core: &WolfCore, waiter: &FlushWaiter) -> i64 {
    poll_until(
        waiter,
        "the initial editor window to appear after attach",
        || !core.read_snapshot().windows.is_empty(),
        |&ok| ok,
    );
    let t1 = core.current_tabpage();
    assert_ne!(
        t1, 0,
        "current_tabpage() should be a nonzero handle after attach"
    );
    t1
}

/// Whether `list_buffers()` currently contains a buffer whose (canonicalized) name
/// equals `want`.
fn listed_has(core: &WolfCore, want: &Path) -> bool {
    core.list_buffers()
        .iter()
        .any(|b| !b.name.is_empty() && norm(Path::new(&b.name)) == want)
}

/// A crashed nvim restarts in place and the front-end reopens the Tier-1 open-file
/// set captured before the restart.
#[test]
fn restart_respawns_and_reopens_open_files() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let tab = attach_and_first_tabpage(&core, &waiter);

    // Open two file-backed buffers in the current tabpage. `b` opens LAST, so it is
    // the active buffer.
    let file_a = temp_md(DOC_A);
    let file_b = temp_md(DOC_B);
    let canon_a = norm(file_a.path());
    let canon_b = norm(file_b.path());

    core.send_input(InputEvent::Open {
        path: file_a.path().to_path_buf(),
    });
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);
    core.send_input(InputEvent::Open {
        path: file_b.path().to_path_buf(),
    });
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // The mirror reflects BOTH file paths AND the active path (b) for this tabpage.
    poll_until(
        &waiter,
        "open_files_for_tabpage to mirror both files with active = b",
        || match core.open_files_for_tabpage(core.current_tabpage()) {
            Some(tf) => {
                let files: HashSet<PathBuf> = tf.files.iter().map(|p| norm(p)).collect();
                let active_is_b = tf.active.as_deref().map(norm) == Some(canon_b.clone());
                files.contains(&canon_a) && files.contains(&canon_b) && active_is_b
            }
            None => false,
        },
        |&ok| ok,
    );

    // Capture the Tier-1 set the front-end would grab BEFORE restart (respawn wipes
    // the mirror). Reopen order: non-active first, ACTIVE last (so it lands focused).
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
    // Three: the two fixtures plus the never-saved `Untitled.md`, which must stay in the Tier-1 set;
    // see crates/wolf-core/tests/README.md#nvim_restartrs
    assert_eq!(
        reopen.len(),
        3,
        "the captured Tier-1 set must contain the two open files PLUS the startup \
         named-but-unsaved Untitled.md, got {reopen:?}"
    );
    // Positive proof that the startup Untitled.md is in the mirror (matched by
    // basename — its path is `<cwd>/Untitled.md`, which is never written to disk).
    assert!(
        mirror
            .files
            .iter()
            .any(|p| p.file_name().and_then(|s| s.to_str()) == Some("Untitled.md")),
        "the Tier-1 mirror must include the named-but-unsaved startup Untitled.md, got {:?}",
        mirror.files
    );

    // Crash nvim and wait for the Dead transition.
    self_sigkill(&core);
    poll_until(
        &waiter,
        "liveness() to become Dead after the SIGKILL",
        || core.liveness() == Liveness::Dead,
        |&ok| ok,
    );

    // A respawn from Dead returns Ok(true). The fresh nvim's unnamed bufnr 1 is not filtered from
    // list_buffers(), so assert the named files are absent rather than emptiness.
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
        "after restart the captive nvim must be Alive again"
    );
    let fresh_tab = core.current_tabpage();
    assert_ne!(
        fresh_tab, 0,
        "current_tabpage() must be a valid (nonzero) handle after restart"
    );
    assert!(
        !listed_has(&core, &canon_a) && !listed_has(&core, &canon_b),
        "a freshly restarted nvim must carry NONE of the previously-open named files, got {:?}",
        core.list_buffers()
    );

    // Drive the reopen the front-end performs off the captured set.
    core.set_current_tabpage(fresh_tab);
    for path in &reopen {
        core.send_input(InputEvent::Open { path: path.clone() });
    }
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // Both files are open again in the restarted nvim.
    poll_until(
        &waiter,
        "both fixtures to be listed again after the reopen",
        || listed_has(&core, &canon_a) && listed_has(&core, &canon_b),
        |&ok| ok,
    );
    assert!(
        listed_has(&core, &canon_a) && listed_has(&core, &canon_b),
        "list_buffers() must contain BOTH reopened files after restart"
    );

    core.shutdown().expect("shutdown should be Ok");
    drop((file_a, file_b));
}

/// The Tier-1 crash mirror carries NO terminal buffer: with the drawer open,
/// `open_files_for_tabpage` reports no `term://` entry and its `active` is not the
/// terminal, so `liveness_ui::reopen_order` cannot replay one.
///
/// ## Why this matters — MEASURED, not assumed
///
/// The replay of a terminal buffer does not fail harmlessly. Driven through the
/// production path (`InputEvent::Open { path: "term://…" }` against a real core,
/// the post-restart analogue) against the Linux flake's nvim 0.12.6:
///
/// * **A second shell is spawned.** `ps` bash rows go 1 -> 2 on the replay and back
///   to 1 after `shutdown()`; Neovim's own `BufReadCmd term://*` autocmd re-runs
///   `termopen()`.
/// * **The pid inside the name changes** (`//78473:` -> `//78489:`) — the
///   conclusive discriminator. It is not the same buffer reopened, it is a NEW job.
///   The name otherwise matches byte for byte, including the dead core's cwd, which
///   no longer exists after a real crash.
/// * **No error, no toast, no message.** Nothing surfaces to the user.
/// * **No new grid or window** — the terminal DISPLACES the file in the pane it
///   lands in, and `is_drawer` is false: a terminal squatting in an editor pane.
/// * **It compounds.** The replayed buffer re-enters the mirror immediately, so
///   every crash+replay cycle spawns another shell and re-poisons the mirror. And
///   because `reopen_order` puts `active` LAST so it lands focused, a post-crash
///   window ends up focused on a freshly spawned shell.
/// * **Control leg**: an ordinary missing path opens an empty named buffer and
///   spawns nothing, so this is specific to the `term://` scheme.
///
/// ## Why the drawer is opened, hidden and re-opened
///
/// Because the obvious three-line version of this test can pass with the filter
/// DELETED, and nothing in its output would say so.
///
/// A FIRST open enters the terminal buffer BEFORE `termopen()` gives it its
/// `term://<cwd>//<pid>:<cmd>` name, and `init.lua`'s `listed_files()` drops unnamed
/// buffers. The `wolf_open_files` emit that `BufEnter` fires there therefore reports
/// two files and no active path — byte-identical to the answer the filter produces.
/// Whether a LATER emit re-reports the buffer once it is named rests on `BufFilePost`
/// firing at the rename, which runtime/nvim/README.md#decisions records for
/// `:terminal` (the `wolf_buffers` coalescing entry). That note is a premise this
/// test would otherwise be resting on, and no level-triggered predicate could tell
/// "the terminal was filtered out" from "the emit has not seen the name yet".
///
/// Hiding and re-opening removes the premise instead of trusting it.
/// `wolf_toggle_terminal` keeps the terminal buffer and its job alive across a hide,
/// so the re-open takes the reuse branch and its `nvim_win_set_buf` fires
/// `BufEnter` / `BufWinEnter` on a buffer that ALREADY carries its name. Those
/// events are in `wolf_setup_open_files`'s own autocmd list, so the emit that
/// follows sees the terminal in `getbufinfo({buflisted = 1})` and as the tabpage's
/// active buffer — and the only thing that can then keep it out of the mirror is the
/// filter. [`MIRROR_SETTLE`] absorbs any later emit before the read.
///
/// FALSIFYING EDIT: delete the `is_terminal_buffer_name` filter from
/// `redraw::RedrawHandler::apply_open_files`. Predicted red: the `TabFiles` M20
/// measured — `active: Some("term://…/bin/bash")` and a third `files` entry naming
/// the same shell.
#[test]
fn open_files_omits_a_terminal_buffer() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let tab = attach_and_first_tabpage(&core, &waiter);

    // One real file, so the mirror has something in it that is NOT the terminal and
    // an empty answer cannot pass for a filtered one.
    let fixture = temp_md(DOC_A);
    let canon_fixture = norm(fixture.path());
    core.send_input(InputEvent::Open {
        path: fixture.path().to_path_buf(),
    });
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // Baseline: the mirror is live and reports the fixture as the ACTIVE file.
    poll_until(
        &waiter,
        "the open-file mirror to report the fixture as the tabpage's active file",
        || {
            core.open_files_for_tabpage(tab).is_some_and(|tf| {
                tf.active.as_deref().map(norm) == Some(canon_fixture.clone())
                    && tf.files.iter().any(|p| norm(p) == canon_fixture)
            })
        },
        |&ok| ok,
    );

    // Open the drawer through the production API, as the chord does, not `:terminal`. It ends in
    // `startinsert`, so no Ex command may be sent from here on.
    core.toggle_terminal();

    // Wait for the `term://` name, which exists only once `termopen()` has run; this also keeps the
    // hide toggle below from racing the open and building a second drawer.
    poll_until(
        &waiter,
        "win_buffers to report the drawer's terminal buffer carrying its term:// name",
        || {
            core.read_snapshot()
                .win_buffers
                .values()
                .any(is_named_drawer)
        },
        |&ok| ok,
    );

    // Hide the drawer. The terminal BUFFER and its job survive (only the window
    // closes), so it stays `buflisted` and stays in `listed_files()`.
    core.toggle_terminal();
    poll_until(
        &waiter,
        "the drawer window to be gone from win_buffers after the hide toggle",
        || {
            !core
                .read_snapshot()
                .win_buffers
                .values()
                .any(|b| b.is_terminal)
        },
        |&ok| ok,
    );

    // Re-opening binds the already-named terminal buffer, so every autocmd sees `term://…`: that
    // lets the assertions below tell "filtered out" from "not reported yet".
    core.toggle_terminal();
    poll_until(
        &waiter,
        "the drawer to come back with its named terminal buffer bound",
        || {
            core.read_snapshot()
                .win_buffers
                .values()
                .any(is_named_drawer)
        },
        |&ok| ok,
    );

    // Read the drawer's live name from a different snapshot map, so the assertion compares two
    // independently reported values rather than a literal typed here.
    let drawer_name: PathBuf = {
        let snap = core.read_snapshot();
        snap.win_buffers
            .values()
            .find(|b| is_named_drawer(b))
            .and_then(|b| b.path.clone())
            .expect("the poll above already matched a NAMED drawer terminal buffer")
    };

    settle(&waiter, MIRROR_SETTLE);

    let mirror = core
        .open_files_for_tabpage(tab)
        .expect("the open-file mirror must still be present with the drawer open");

    assert_eq!(
        mirror.active, None,
        "with the drawer focused the tabpage's active buffer IS the terminal, and a \
         terminal buffer is not a file to reopen — so the mirror must report NO \
         active file. A `Some(\"term://…\")` here is M20's measured shape with the \
         filter gone, and it is the worst half of the bug: `liveness_ui::reopen_order` \
         puts `active` LAST so it lands focused, so a post-crash window would come \
         back focused on a freshly spawned shell. Full mirror: {:?}",
        mirror.files
    );

    let terminal_entries: Vec<&PathBuf> = mirror
        .files
        .iter()
        .filter(|p| p.to_str().is_some_and(|s| s.starts_with(TERMINAL_SCHEME)))
        .collect();
    assert!(
        terminal_entries.is_empty(),
        "the crash mirror must carry no terminal buffer at all, but {} of its entries \
         begin with `{TERMINAL_SCHEME}`: {terminal_entries:?}. Each one is a `:edit \
         term://…` the front-end would replay after a crash, and each such replay \
         spawns another shell with no error and no message.",
        terminal_entries.len()
    );
    assert!(
        !mirror.files.contains(&drawer_name),
        "the drawer's own buffer name, as `win_buffers` reports it ({drawer_name:?}), \
         must be absent from the mirror. Its presence is the same defect the previous \
         assertion describes, stated against the live value rather than the scheme, so \
         a renamed scheme cannot hide it. Mirror: {:?}",
        mirror.files
    );

    // Positive halves: the mirror is a real, populated set that simply excludes the
    // terminal — not an empty one that would satisfy the negatives by itself.
    assert!(
        mirror.files.iter().any(|p| norm(p) == canon_fixture),
        "the fixture opened before the drawer must still be in the mirror ({:?}); \
         without this the assertions above would also pass for a mirror that had gone \
         empty or vanished",
        mirror.files
    );
    assert!(
        mirror
            .files
            .iter()
            .any(|p| p.file_name().and_then(|s| s.to_str()) == Some("Untitled.md")),
        "the named-but-unsaved startup Untitled.md must still be in the mirror, got \
         {:?}",
        mirror.files
    );
    assert_eq!(
        mirror.files.len(),
        2,
        "a drawer-open session mirrors exactly two FILES — the startup Untitled.md \
         and the fixture. Three means the terminal buffer leaked in (M20's measured \
         shape); one means something unrelated to this filter dropped a real file. \
         Got {:?}",
        mirror.files
    );

    core.shutdown().expect("shutdown should be Ok");
    drop(fixture);
}

/// A restart request on a HEALTHY (Alive) core is intentionally a NO-OP: at the
/// core level `restart_nvim()` returns `Ok(false)` (false = no respawn happened)
/// without tearing down the live nvim, so the FFI `restart()` reports "nothing
/// changed" and the front-end does NOT redundantly rebind + reopen (which would
/// duplicate tabpages). The actual teardown+respawn path (`Ok(true)`) is covered
/// by `restart_respawns_and_reopens_open_files` from a Dead state. This pins the
/// no-op contract (the `Alive` check, behind the `restarting` reentrancy guard).
#[test]
fn restart_on_live_core_is_a_noop_ok_false() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let _t1 = attach_and_first_tabpage(&core, &waiter);

    // A restart on an already-Alive core must be a no-op: Ok(false), NOT a respawn.
    let respawned = core
        .restart_nvim()
        .expect("restart_nvim on a live core should be Ok");
    assert!(
        !respawned,
        "restart_nvim() on an Alive core must be a NO-OP (Ok(false)), not a respawn"
    );
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "a no-op restart must leave the live nvim Alive"
    );

    // A second request is likewise a harmless no-op (Ok(false)).
    let respawned_again = core
        .restart_nvim()
        .expect("a repeated restart on a live core should be Ok");
    assert!(
        !respawned_again,
        "a repeated restart_nvim() on an Alive core must also be a NO-OP (Ok(false))"
    );

    // The nvim was never disturbed: blocking RPCs answer normally (a valid current
    // tabpage; the buffer list query succeeds).
    assert_ne!(
        core.current_tabpage(),
        0,
        "current_tabpage() must still resolve after the no-op restart requests"
    );
    let _ = core.list_buffers();

    core.shutdown().expect("shutdown should be Ok");
}

/// Typing queued before a restart's handle swap never reaches the fresh nvim, so a half-typed
/// `:` command cannot arrive there as keys. State queued alongside it (a resize) still applies.
///
/// ## Why the pump is stalled with a read guard
///
/// The simple version of this test (kill, send keys while `Dead`, restart, look) passes with
/// or without the discard, so it cannot fail. nvim-rs 0.9.2 writes each call straight to the
/// child's stdin with no buffer, so a call against a dead nvim fails at once with EPIPE. The
/// pump drains its queue `INPUT_COALESCE_DELAY` (8 ms) after the first event, against
/// whatever handle it reads, and a respawn takes far longer. Keys sent while `Dead` therefore
/// die against the dead handle before the swap. Only keys still queued at the swap can reach
/// the fresh nvim, and the stall puts them there deterministically.
///
/// For a batch holding any toast-dismissing event, the pump takes the snapshot write lock
/// (the toast clear) before applying it. Holding a [`wolf_core::SnapshotReadGuard`] blocks
/// that, so the sacrificial `Esc` parks the pump and every event sent after it stays queued
/// until the guard drops after the restart. Holding the guard across `restart_nvim()` is safe:
/// the redraw handler takes the lock with a `try_write` / `yield_now` loop while nvim-rs keeps
/// answering RPCs, and no setup step waits on a redraw. Nothing else may read the snapshot
/// while the guard is held.
///
/// ## Why the kill is deferred
///
/// [`self_sigkill`] kills nvim inside its own `:lua` command, before the `cmdline_hide`
/// flush. Nothing resets `Snapshot.cmdline` across a restart, so that command line would stay
/// `active` and poison the command-line assertion here. [`deferred_sigkill`] lets the command
/// finish first, and the test asserts that precondition.
///
/// ## What `&columns` pins
///
/// The resize is queued in the same stall as the typing. `&columns` equal to [`STALE_COLS`]
/// proves stale state still applies, so the rule drops typing rather than everything queued.
///
/// ## Falsifying edits and their predicted reds
///
/// * Delete the pump's discard (or run the base code): red at the command-line assertion,
///   which shows the half-typed [`STALE_HALF_TYPED_CMD`] open on the fresh nvim.
/// * Drop `Paste` from `InputEvent::is_typing`: red at the buffer assertion, showing
///   [`STALE_PASTE_MARKER`].
/// * Discard every stale event, not just typing: red at the `&columns` assertion, showing the
///   default 80.
/// * Stamp a constant generation in `send_input`: every post-swap key is dropped too, so the
///   first probe times out.
#[test]
fn typing_queued_before_restart_never_reaches_the_fresh_nvim() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let _tab = attach_and_first_tabpage(&core, &waiter);

    deferred_sigkill(&core);
    poll_until(
        &waiter,
        "liveness() to become Dead after the deferred SIGKILL",
        || core.liveness() == Liveness::Dead,
        |&ok| ok,
    );
    {
        let snap = core.read_snapshot();
        assert!(
            !snap.cmdline.active,
            "precondition: the snapshot's command line must be closed once the old nvim is \
             dead, or the command-line assertion after the restart would read the dead nvim's \
             leftover state; got {:?}",
            snap.cmdline.text()
        );
    }

    // Park the pump on the toast-clear write lock: the sacrificial Esc is the batch that blocks.
    let stall = core.read_snapshot();
    core.send_input(key(Key::Esc));
    sleep(PUMP_STALL_SETTLE);

    core.paste(STALE_PASTE_MARKER.into());
    core.send_input(ch('i'));
    type_str(&core, STALE_KEY_MARKER);
    core.send_input(key(Key::Esc));
    core.send_input(ch(':'));
    type_str(&core, STALE_HALF_TYPED_CMD);
    core.resize(STALE_COLS, STALE_ROWS);

    let restarted = core
        .restart_nvim()
        .expect("restart_nvim should be Ok from a Dead state");
    assert!(
        restarted,
        "restart_nvim() from a Dead state must report an actual respawn (Ok(true))"
    );
    drop(stall);

    // The stale `:` must be checked before any probe, whose own `:call` it would garble.
    settle(&waiter, STALE_INPUT_SETTLE);
    {
        let snap = core.read_snapshot();
        assert!(
            !snap.cmdline.active && snap.cmdline.text().is_empty(),
            "the fresh nvim must show no command line after the restart, but active = {} with \
             text {:?}. Text {STALE_HALF_TYPED_CMD:?} means typing queued before the handle swap \
             was delivered to the fresh nvim (the discard is missing); any other open command \
             line comes from something other than this test's stale input",
            snap.cmdline.active,
            snap.cmdline.text()
        );
    }

    let dir = tempfile::tempdir().expect("failed to create probe tempdir");
    let probe = dir.path().join("probe.txt");
    let values = probe_values(&core, &["join(getline(1, '$'), ',')", "&columns"], &probe);
    assert_eq!(
        values[0], "",
        "the fresh nvim's buffer must be empty: {STALE_PASTE_MARKER:?} in it means a paste \
         queued before the swap was delivered (Paste is not classified as typing), \
         {STALE_KEY_MARKER:?} means queued keys were; got {:?}",
        values[0]
    );
    assert_eq!(
        values[1],
        STALE_COLS.to_string(),
        "the resize queued before the swap is state and must still apply to the fresh nvim. \
         The default {} means stale state was discarded along with the typing; any other \
         width means the resize was misapplied",
        common::DEFAULT_COLS
    );

    // Positive control: typing sent after the swap is delivered, so the empty buffer above
    // is the discard at work and not a pump that delivers nothing.
    core.send_input(ch('i'));
    type_str(&core, CONTROL_MARKER);
    core.send_input(key(Key::Esc));
    let after = probe_values(&core, &["join(getline(1, '$'), ',')"], &probe);
    assert_eq!(
        after[0], CONTROL_MARKER,
        "typing sent after the restart must reach the fresh nvim's buffer; anything else means \
         post-swap input is dropped or misrouted, and the empty buffer asserted above proves \
         nothing about stale typing"
    );

    core.shutdown().expect("shutdown should be Ok");
}
