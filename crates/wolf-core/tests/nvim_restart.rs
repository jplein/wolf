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
//!   * The restart's reset: after `Ok(true)` the snapshot holds none of the dead
//!     nvim's windows or command line, `seq` never goes back, and no layout intent
//!     or navigation history of the dead nvim's windows is left. A fresh nvim
//!     numbers its windows from 1000 again, so anything kept would be read as the
//!     fresh nvim's.
//!   * Evidence tests, green on the base code: every death mode (SIGKILL with a
//!     surviving child, SIGTERM, a scripted `qa!`, `:cq`) is seen and restarted on
//!     one core; a death releases a call waiting on nvim; a restart leaves a
//!     sibling core alone.
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
use wolf_core::{InputEvent, Key, LayoutIntent, Liveness, WinBuffer, WolfCore};

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

/// Text typed after `:` and left open, never followed by Enter, while nvim is killed from
/// outside. Holds no whitespace, so no `init.lua` abbreviation can expand it.
const OPEN_CMDLINE_MARKER: &str = "opencmdlineoscar";

/// The grid size requested while the pump is stalled. Distinct from the defaults the core
/// starts and respawns at, so `&columns` tells "applied" from "dropped".
const STALE_COLS: u16 = common::DEFAULT_COLS + 20;
/// See [`STALE_COLS`].
const STALE_ROWS: u16 = common::DEFAULT_ROWS + 6;

/// How long a freshly started core is left alone before its `seq` is read as the number of
/// flushes a fresh nvim makes by itself, and before its startup history is checked.
const STARTUP_SETTLE: Duration = Duration::from_secs(1);

/// How many times a fresh nvim's own startup flushes the flushes driven before the kill must
/// cover; see [`restart_drops_the_dead_nvims_windows_and_command_line`].
const SEQ_MARGIN_FACTOR: u64 = 4;

/// Flushes driven before the kill on top of [`SEQ_MARGIN_FACTOR`], so a startup measured at
/// one or two flushes still leaves a clear gap.
const MIN_FLUSHES_BEFORE_KILL: u64 = 20;

/// How long to let the fresh nvim's late setup emits land after the restart before the
/// snapshot, the intents or the history are read.
const RESTART_SETTLE: Duration = Duration::from_secs(1);

/// The four ways the captive nvim can end that the liveness check must see, each as the Ex
/// commands that end it, typed in order. Round 1's job is a child process that outlives the
/// SIGKILL, so the round also shows that a surviving child cannot hold nvim's stdout open.
///
/// A typed `:qa!` is not a round: `init.lua`'s `qa` abbreviation turns it into
/// `:WolfQuitAll!`, a window close by design, which `commands.rs`'s
/// `ex_qa_bang_forces_close_window_when_modified` pins. Round 3 therefore runs `qa!` through
/// `vim.cmd`, as a script or plugin would; the abbreviation's `getcmdline()` guard leaves the
/// typed `lua` line alone, and `vim.cmd` takes no abbreviations.
const DEATH_MODES: [(&str, &[&str]); 4] = [
    (
        "SIGKILL with a surviving child job",
        &[
            "lua vim.fn.jobstart({'sleep', '30'})",
            "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)",
        ],
    ),
    (
        "SIGTERM",
        &["lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 15)"],
    ),
    ("a scripted qa!", &["lua vim.cmd('qa!')"]),
    (":cq", &["cq"]),
];

/// How long nvim is wedged in [`a_death_while_requests_wait_on_nvim_releases_them`], far past
/// the kill, so only the death can end the wait.
const WEDGE_SLEEP_MS: u64 = 20_000;

/// How long after the wedge starts a child shell SIGKILLs nvim, in seconds. Long enough that
/// the timed call is surely issued while nvim is alive, short against
/// [`WAITER_RELEASE_BUDGET`].
const WEDGE_KILL_DELAY_S: f64 = 1.5;

/// How long after the wedge command the sacrificial key is sent, so that nvim has started
/// its sleep and the pump's call for the key waits on it.
const WEDGE_SETTLE: Duration = Duration::from_millis(100);

/// The bound on a call made while nvim is wedged and about to be killed. Under the default
/// 5 s RPC budget, so a call that returns only at its own timeout was not released by the
/// death; above [`WEDGE_KILL_DELAY_S`] with a margin for a loaded host.
const WAITER_RELEASE_BUDGET: Duration = Duration::from_secs(4);

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
/// `cmdline` is not left `active`.
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

/// The window handles of every entry in the snapshot's `windows`, sorted.
fn window_handles(core: &WolfCore) -> Vec<i64> {
    let mut wins: Vec<i64> = core
        .read_snapshot()
        .windows
        .values()
        .map(|w| w.win)
        .collect();
    wins.sort_unstable();
    wins
}

/// Waits for the attach to report an active window that `windows` also holds, and returns
/// its handle.
fn attach_and_first_window(core: &WolfCore, waiter: &FlushWaiter) -> i64 {
    poll_until(
        waiter,
        "the attach to report an active window that the snapshot's windows also hold",
        || {
            let snap = core.read_snapshot();
            snap.active_win
                .filter(|a| snap.windows.values().any(|w| w.win == *a))
        },
        Option::is_some,
    )
    .expect("poll_until returned only once the active window was Some")
}

/// Sends `event` and blocks until a redraw flush lands after it, read against a `seq` taken
/// before the send so a flush that lands before the wait starts cannot be missed.
fn send_and_wait_for_flush(core: &WolfCore, waiter: &FlushWaiter, event: InputEvent) {
    let before = core.read_snapshot().seq;
    core.send_input(event);
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while core.read_snapshot().seq <= before {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || !waiter.wait_for_wake(remaining) {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for a flush after a key typed in \
                 insert mode; seq stayed at {before}"
            );
        }
    }
}

/// Drives at least `n` redraw flushes by typing and erasing one character in insert mode,
/// then leaves insert mode.
fn drive_flushes(core: &WolfCore, waiter: &FlushWaiter, n: u64) {
    core.send_input(ch('i'));
    for i in 0..n {
        let event = if i % 2 == 0 {
            ch('x')
        } else {
            key(Key::Backspace)
        };
        send_and_wait_for_flush(core, waiter, event);
    }
    core.send_input(key(Key::Esc));
}

/// Types `marker` at the start of the current buffer in insert mode, then leaves it.
fn type_marker(core: &WolfCore, marker: &str) {
    core.send_input(ch('i'));
    type_str(core, marker);
    core.send_input(key(Key::Esc));
}

/// Line 1 of the live nvim's current buffer, read through a file probe.
fn first_line(core: &WolfCore, probe: &Path) -> String {
    probe_values(core, &["getline(1)"], probe).remove(0)
}

/// Polls until `core` reports `Dead`, naming `what` killed it in the timeout panic.
fn wait_for_dead(core: &WolfCore, waiter: &FlushWaiter, what: &str) {
    poll_until(
        waiter,
        &format!("liveness() to become Dead after {what}"),
        || core.liveness(),
        |l| *l == Liveness::Dead,
    );
}

/// Calls `restart_nvim()` on a dead core and asserts it respawned to `Alive`.
fn restart_from_dead(core: &WolfCore, what: &str) {
    let restarted = core
        .restart_nvim()
        .unwrap_or_else(|e| panic!("restart_nvim after {what} must be Ok, got {e:?}"));
    assert!(
        restarted,
        "restart_nvim() after {what} must report an actual respawn (Ok(true)); Ok(false) means \
         it saw an Alive core or a restart already in flight and left the dead nvim in place"
    );
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "after the restart that followed {what} the captive nvim must be Alive again"
    );
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
/// flush, so the dead nvim's command line is still open when it dies. [`deferred_sigkill`]
/// lets the command finish first, and the test asserts that precondition. The command-line
/// assertion after the restart then depends only on the queued input, not on the restart
/// clearing the dead nvim's command line, which
/// [`restart_drops_the_dead_nvims_windows_and_command_line`] pins.
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

/// A restart drops the dead nvim's windows and its open command line from the snapshot, and
/// `seq` never goes back.
///
/// ## Why the handles collide
///
/// A fresh nvim numbers its windows from 1000 again, so the dead nvim's first window and the
/// fresh one share a handle, and the dead split's handle is the one the fresh nvim's next
/// split will get. A front-end that binds a pane by handle then binds a frozen grid, nothing,
/// or a later fresh window. The poll for `active_win == Some(first_win)` after the restart
/// measures that premise: it times out if the fresh nvim's first window has another handle.
///
/// ## Why nvim is killed from outside an open command line
///
/// nvim-rs drops the notifications still queued when nvim's stdout closes, so the last frames
/// before a death may never reach the snapshot. The test therefore waits until the snapshot
/// shows [`OPEN_CMDLINE_MARKER`] on an active command line before it kills nvim.
///
/// The SIGKILL comes from the test while nvim waits for input in that line. An Enter would end
/// the line, and whether its `cmdline_hide` is flushed before a `:lua` kill's body runs is
/// nvim's internal order, which nothing pins.
///
/// The precondition after the death, that the line is still active, keeps the command-line
/// assertion after the restart from passing because the line was never open.
///
/// ## Why so many flushes before the kill
///
/// The `seq` check must tell "carried forward" from "started over at 0". A fresh nvim's own
/// setup flushes bump a restarted counter too, so `seq` before the kill must clearly exceed
/// them. The test measures them on this core at startup and drives [`SEQ_MARGIN_FACTOR`]
/// times as many, plus [`MIN_FLUSHES_BEFORE_KILL`].
///
/// ## Falsifying edits and their predicted reds
///
/// * Skip `reset_for_respawn()` in `restart_nvim_inner` (or run the base code): red at the
///   windows assertion, showing the split's handle, and at the command-line assertion.
/// * Build the fresh snapshot with `seq` 0: red at the `seq` assertion.
#[test]
fn restart_drops_the_dead_nvims_windows_and_command_line() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let first_win = attach_and_first_window(&core, &waiter);
    let fresh_windows = window_handles(&core);
    assert_eq!(
        fresh_windows,
        vec![first_win],
        "premise: a freshly started nvim shows exactly one window, its active one; the \
         assertion after the restart compares against this shape"
    );

    settle(&waiter, STARTUP_SETTLE);
    let startup_seq = core.read_snapshot().seq;
    drive_flushes(
        &core,
        &waiter,
        startup_seq * SEQ_MARGIN_FACTOR + MIN_FLUSHES_BEFORE_KILL,
    );

    ex_command(&core, "vsplit");
    let wins = poll_until(
        &waiter,
        "the snapshot to hold two windows after :vsplit",
        || window_handles(&core),
        |w| w.len() == 2 && w.contains(&first_win),
    );
    let split_win = *wins
        .iter()
        .find(|&&w| w != first_win)
        .expect("two distinct windows, one of them first_win");
    let seq_before = core.read_snapshot().seq;

    let dir = tempfile::tempdir().expect("failed to create probe tempdir");
    let probe = dir.path().join("probe.txt");
    let pid_text = probe_values(&core, &["getpid()"], &probe).remove(0);
    let pid: u32 = pid_text.parse().unwrap_or_else(|e| {
        panic!("nvim's getpid() must read back as a u32 pid, got {pid_text:?}: {e}")
    });

    core.send_input(ch(':'));
    type_str(&core, OPEN_CMDLINE_MARKER);
    poll_until(
        &waiter,
        "the snapshot to show the open command line, before nvim is killed",
        || {
            let snap = core.read_snapshot();
            snap.cmdline.active && snap.cmdline.text() == OPEN_CMDLINE_MARKER
        },
        |&ok| ok,
    );

    let status = std::process::Command::new("sh")
        .args(["-c", &format!("kill -9 {pid}")])
        .status()
        .expect("failed to run sh for the kill");
    assert!(
        status.success(),
        "`kill -9 {pid}` must succeed on the live captive nvim, got {status}: a failure means \
         the pid read through getpid() is not a process this test may signal, or it is gone"
    );
    wait_for_dead(&core, &waiter, "the SIGKILL");
    {
        let snap = core.read_snapshot();
        assert!(
            snap.windows.values().any(|w| w.win == split_win),
            "precondition: the dead nvim's split {split_win} must still be in the snapshot \
             before the restart, or its absence afterwards proves nothing; got {:?}",
            snap.windows.values().map(|w| w.win).collect::<Vec<_>>()
        );
        assert!(
            snap.cmdline.active,
            "precondition: the command line the snapshot showed open with \
             {OPEN_CMDLINE_MARKER:?} before the kill must still be active after the death, or \
             the command-line assertion after the restart proves nothing; false means a \
             cmdline_hide landed, with text now {:?}",
            snap.cmdline.text()
        );
    }

    restart_from_dead(&core, "the SIGKILL");
    let seq_after = core.read_snapshot().seq;
    assert!(
        seq_after >= seq_before,
        "seq must never go back across a restart, since waiters key on it advancing: it was \
         {seq_before} before the kill and is {seq_after} right after the restart, so the \
         fresh snapshot started its counter over (startup alone reached {startup_seq})"
    );

    poll_until(
        &waiter,
        "the fresh nvim to report its first window, which reuses the dead first window's handle",
        || core.read_snapshot().active_win,
        |a| *a == Some(first_win),
    );
    settle(&waiter, RESTART_SETTLE);

    let (wins, cmdline_active, cmdline_text) = {
        let snap = core.read_snapshot();
        let mut wins: Vec<i64> = snap.windows.values().map(|w| w.win).collect();
        wins.sort_unstable();
        (wins, snap.cmdline.active, snap.cmdline.text())
    };
    assert!(
        !wins.contains(&split_win),
        "the dead nvim's split {split_win} must be gone from the snapshot after the restart, \
         but windows holds {wins:?}: the restart kept the dead nvim's windows, and a pane bound \
         to that handle shows a frozen grid or catches the fresh nvim's next split"
    );
    assert_eq!(
        wins,
        vec![first_win],
        "after the restart the snapshot must hold exactly the fresh nvim's one window, as at \
         startup; more entries are windows kept from the dead nvim, none means the fresh \
         window's win_pos never landed"
    );
    assert!(
        !cmdline_active,
        "the dead nvim's command line must be closed after the restart, but it is active with \
         text {cmdline_text:?}: the restart kept the dead nvim's command line, which the \
         front-ends keep painting"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A restart drops the layout intents the dead nvim queued and the navigation history of its
/// windows.
///
/// Intents are drained once before the fixture work and never again before the restart, so
/// the `:vsplit`'s `Split` intent (which `layout_intents.rs` shows `:vsplit` emits) is still
/// queued when nvim dies. The history check is not vacuous: the test measures that a fresh
/// nvim's startup leaves its first window nothing to go Back to, and the fresh nvim's first
/// window reuses `first_win`'s handle, which the poll after the restart measures.
///
/// ## Falsifying edits and their predicted reds
///
/// * Skip the `intents` clear in `restart_nvim_inner` (or run the base code): red at the
///   intents assertion, showing the `Split` into the dead split.
/// * Skip the history reset: red at the history assertion.
#[test]
fn restart_drops_the_dead_nvims_layout_intents_and_history() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let first_win = attach_and_first_window(&core, &waiter);
    settle(&waiter, STARTUP_SETTLE);
    assert!(
        !core.history_can_back(first_win),
        "premise: a fresh nvim's startup must leave its first window nothing to go Back to; \
         true means the history assertion after the restart is red whatever the restart does"
    );
    let _ = core.drain_intents();

    let file_a = temp_md(DOC_A);
    let file_b = temp_md(DOC_B);
    let canon_b = norm(file_b.path());
    core.send_input(InputEvent::Open {
        path: file_a.path().to_path_buf(),
    });
    core.send_input(InputEvent::Open {
        path: file_b.path().to_path_buf(),
    });
    poll_until(
        &waiter,
        "the first window to show the second fixture with history to go Back to",
        || {
            let current = core.read_snapshot().current_file.clone();
            (core.history_can_back(first_win), current)
        },
        |(back, current)| *back && current.as_deref().map(norm) == Some(canon_b.clone()),
    );

    ex_command(&core, "vsplit");
    let wins = poll_until(
        &waiter,
        "the snapshot to hold two windows after :vsplit",
        || window_handles(&core),
        |w| w.len() == 2 && w.contains(&first_win),
    );
    let split_win = *wins
        .iter()
        .find(|&&w| w != first_win)
        .expect("two distinct windows, one of them first_win");

    self_sigkill(&core);
    wait_for_dead(&core, &waiter, "the SIGKILL");
    restart_from_dead(&core, "the SIGKILL");
    poll_until(
        &waiter,
        "the fresh nvim to report its first window, which reuses the dead first window's handle",
        || core.read_snapshot().active_win,
        |a| *a == Some(first_win),
    );
    settle(&waiter, RESTART_SETTLE);

    let intents = core.drain_intents();
    let stale: Vec<&LayoutIntent> = intents
        .iter()
        .filter(|i| match i {
            LayoutIntent::Split { from, win, .. } => *from == split_win || *win == split_win,
            LayoutIntent::FocusDir { from, .. } => *from == split_win,
            LayoutIntent::DrawerOpen { win } => *win == split_win,
        })
        .collect();
    assert!(
        stale.is_empty(),
        "no layout intent naming the dead split {split_win} may survive the restart, but \
         {stale:?} did: the front-end would split a pane for a window of the dead nvim, whose \
         handle the fresh nvim's next split reuses. All intents drained: {intents:?}"
    );
    assert!(
        !core.history_can_back(first_win),
        "the fresh nvim's first window {first_win} must have nothing to go Back to after the \
         restart, but it does: the dead nvim's history for the same handle was kept, and Back \
         would open the dead nvim's files in the fresh window"
    );

    core.shutdown().expect("shutdown should be Ok");
    drop((file_a, file_b));
}

/// Every way the captive nvim can end is seen as `Dead`, and a restart recovers from each,
/// several times in a row on one core.
///
/// An evidence test, green on the base code. Each death closes the stdout the supervisor
/// watches; round 1's child job shows that a process outliving nvim cannot hold it open.
/// Each restart gets its own supervisor on the shared liveness cell, which the later rounds
/// show by being seen at all. A round that never reaches `Dead` is a finding about that mode.
///
/// ## Falsifying edits and their predicted reds
///
/// * Remove `mark_dead` from the supervisor in `nvim::spawn`: red in round 1.
/// * Pass a fresh `Arc<AtomicU8>` to `nvim::spawn` inside `restart_nvim_inner`: red in
///   round 2, whose death is then never seen.
#[test]
fn every_death_mode_is_seen_and_each_restart_recovers() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let _first_win = attach_and_first_window(&core, &waiter);
    let dir = tempfile::tempdir().expect("failed to create probe tempdir");
    let probe = dir.path().join("probe.txt");

    for (round, (mode, commands)) in DEATH_MODES.iter().enumerate() {
        let what = format!("round {} ({mode})", round + 1);
        for cmd in commands.iter() {
            ex_command(&core, cmd);
        }
        wait_for_dead(&core, &waiter, &what);
        restart_from_dead(&core, &what);

        let marker = format!("deathround{}marker", round + 1);
        type_marker(&core, &marker);
        let line = first_line(&core, &probe);
        assert_eq!(
            line, marker,
            "after {what} and its restart, typing must reach the fresh nvim's buffer: an empty \
             line means the keys never arrived (the pump kept the dead handle), and an earlier \
             round's marker means the fresh buffer recovered a swap of a dead one"
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// A death while calls wait on nvim releases them: a front-end call made while nvim is
/// wedged returns once nvim dies, well inside its own budget, and the input pump, waiting on
/// a key sent into the wedge, delivers typing to the fresh nvim after the restart.
///
/// An evidence test, green on the base code, with the default 5 s RPC budget. The release
/// at EOF is nvim-rs 0.9.2's (`send_error_to_callers`), which no edit in this repository
/// removes, so the timing half records evidence rather than guarding code here.
///
/// FALSIFYING EDIT: skip the pump's handle swap in `restart_nvim_inner`, so the pump keeps
/// the dead handle. Predicted red: the marker never reaches the fresh buffer, and the probe,
/// typed through the same pump, times out.
#[test]
fn a_death_while_requests_wait_on_nvim_releases_them() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let _first_win = attach_and_first_window(&core, &waiter);

    ex_command(
        &core,
        &format!(
            "lua vim.fn.jobstart({{'sh', '-c', 'sleep {WEDGE_KILL_DELAY_S}; kill -9 ' .. \
             vim.fn.getpid()}}); (vim.uv or vim.loop).sleep({WEDGE_SLEEP_MS})"
        ),
    );
    sleep(WEDGE_SETTLE);
    core.send_input(key(Key::Esc));
    sleep(WEDGE_SETTLE);

    assert_ne!(
        core.liveness(),
        Liveness::Dead,
        "precondition: nvim must still be alive when the timed call is made, or the call fails \
         on a closed pipe and shows nothing about a waiter being released"
    );
    let started = Instant::now();
    let _ = core.list_buffers();
    let waited = started.elapsed();
    assert!(
        waited < WAITER_RELEASE_BUDGET,
        "a call waiting on a wedged nvim must return once nvim dies (about \
         {WEDGE_KILL_DELAY_S} s in), but it took {waited:?}: about 5 s means it ran to its \
         own RPC timeout and the death did not release it; longer means it was never bounded"
    );

    wait_for_dead(&core, &waiter, "the SIGKILL during the wedge");
    restart_from_dead(&core, "the SIGKILL during the wedge");

    let dir = tempfile::tempdir().expect("failed to create probe tempdir");
    let probe = dir.path().join("probe.txt");
    let marker = "afterwedgeoscar";
    type_marker(&core, marker);
    assert_eq!(
        first_line(&core, &probe),
        marker,
        "typing after the restart must reach the fresh nvim's buffer: anything else means the \
         pump, released from its wait on the dead nvim, did not move to the fresh handle"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Each core owns its liveness, its restart and its nvim: killing and restarting one leaves
/// a sibling core alive and untouched.
///
/// An evidence test, green on the base code.
///
/// FALSIFYING EDIT: have `WolfCore::start` take its liveness cell from a process-global
/// `OnceLock`, so every core shares it. Predicted red: the sibling reads `Dead` while only
/// the first core's nvim died.
#[test]
fn a_restart_leaves_a_sibling_core_alone() {
    let (core_a, waiter_a) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let (core_b, waiter_b) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let _ = attach_and_first_window(&core_a, &waiter_a);
    let _ = attach_and_first_window(&core_b, &waiter_b);

    self_sigkill(&core_a);
    wait_for_dead(&core_a, &waiter_a, "core A's SIGKILL");
    assert_eq!(
        core_b.liveness(),
        Liveness::Alive,
        "core B must stay Alive while only core A's nvim died: anything else means the two \
         cores share liveness state, and one project's crash shows on every window"
    );

    restart_from_dead(&core_a, "core A's SIGKILL");
    assert_eq!(
        core_b.liveness(),
        Liveness::Alive,
        "core B must stay Alive across core A's restart"
    );
    let b_restarted = core_b
        .restart_nvim()
        .expect("restart_nvim on the live core B should be Ok");
    assert!(
        !b_restarted,
        "core B's restart_nvim() must be the Alive no-op (Ok(false)): true means B believed its \
         own nvim dead and tore it down"
    );

    let dir = tempfile::tempdir().expect("failed to create probe tempdir");
    let probe_a = dir.path().join("probe-a.txt");
    let probe_b = dir.path().join("probe-b.txt");
    let marker_a = "siblingalpha";
    let marker_b = "siblingbravo";
    type_marker(&core_a, marker_a);
    type_marker(&core_b, marker_b);
    let line_a = first_line(&core_a, &probe_a);
    let line_b = first_line(&core_b, &probe_b);
    assert_eq!(
        line_a, marker_a,
        "core A's restarted nvim must hold only what was typed into core A; {marker_b:?} in it \
         means typing crossed between cores, empty means A's input was lost"
    );
    assert_eq!(
        line_b, marker_b,
        "core B's nvim must hold only what was typed into core B; {marker_a:?} in it means \
         typing crossed between cores, empty means B's input was lost"
    );

    core_a.shutdown().expect("core A shutdown should be Ok");
    core_b.shutdown().expect("core B shutdown should be Ok");
}
