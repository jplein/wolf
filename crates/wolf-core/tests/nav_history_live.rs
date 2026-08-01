//! REAL-NVIM integration tests for the per-pane navigation history (plan §B2-§B6):
//! the `wolf_buffers` -> `Snapshot.win_buffers` parse, the `fold_nav_targets` fold
//! over `win_buffers` + `previews`, the `win_close` prune, and the six
//! `WolfCore::history_*` methods that move the pane through the two new
//! `_G.wolf_history_*` Lua helpers.
//!
//! These drive a REAL, hermetic nvim (NO mocks, per the project testing policy)
//! via `WOLF_NVIM_BIN`; the common helpers PANIC loudly if it is unset (matching
//! the rest of the wolf-core suite). They mirror `pane_buffers.rs` /
//! `current_file.rs`: the `wolf_buffers` / `wolf_preview` notifications arrive
//! SEPARATELY from redraw flushes, so every assertion POLLS with a generous
//! timeout, draining wakes as it goes.
//!
//! ## Surface under test (plan §B4/§B5/§B11)
//!
//! All of this has SHIPPED. On `WolfCore`:
//!   * `history_can_back(&self, win: i64) -> bool`
//!   * `history_can_forward(&self, win: i64) -> bool`
//!   * `history_back(&self, win: i64) -> bool`     (true == the pane MOVED)
//!   * `history_forward(&self, win: i64) -> bool`
//!   * `history_jump(&self, win: i64, index: usize) -> bool`
//!   * `history_entries(&self, win: i64) -> Vec<MenuRow>`  (newest-first, capped
//!     at `MAX_MENU_ROWS` distinct rows, home-abbreviated)
//!     plus `Snapshot.win_buffers: HashMap<i64, WinBuffer>` and the two Lua helpers
//!     `_G.wolf_history_open` / `_G.wolf_history_set_buf`. This file does NOT touch
//!     non-test source. It runs under the `//crates/wolf-core:nav_history_live`
//!     `rust_test` target, which mirrors `//crates/wolf-core:pane_buffers`.
//!
//! ## The startup buffer is a NAMED `Untitled.md`
//!
//! The anonymous-buffer feature names the startup buffer `<cwd>/Untitled.md` at
//! `VimEnter`, so EVERY pane's history starts with an `Edit` entry for it. These
//! tests therefore never assert an absolute stack depth from startup — they assert
//! RELATIVE growth ("exactly one entry was added") and the newest-first row
//! prefix, which is robust to that prefix.
//!
//! ## Terminal DRAWER exclusion, as shipped
//!
//! [`split_terminal_is_tracked_but_the_drawer_pane_never_gets_history`] and
//! [`toggling_the_drawer_never_touches_a_survivor_panes_history`] assert the locked
//! decision "terminal drawer EXCLUDED (no history)". Three mechanisms in
//! `runtime/nvim/init.lua` deliver it, and a regression in any of them is what
//! those two tests catch:
//!
//!   * `wolf_buffers` emits are COALESCED onto the next event-loop tick
//!     (`vim.schedule`), so the half-built states a multi-step command passes
//!     through never reach core's fold as entries of their own — neither the
//!     `botright split` whose brand-new window still shows the PREVIOUS buffer, nor
//!     the `:terminal` whose `buftype` only becomes `"terminal"` at `TermOpen`.
//!   * `_G.wolf_toggle_terminal`'s OPEN path runs inside `_G.wolf_suppress_buffers`,
//!     which drops emits mid-build and re-emits exactly ONCE at the end, so core
//!     sees only the fully-tagged final state. It is `pcall`-guarded, so the flag is
//!     restored even when a build step throws.
//!   * the `wolf_buffers` tuple's `is_drawer` element ORs the BUFFER tag
//!     (`b:wolf_terminal`, carried by the shared terminal buffer) with the WINDOW
//!     tag (`w:wolf_terminal`, set on the drawer window right after its split).

mod common;

use std::collections::HashSet;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Key, MenuRow, Snapshot, WolfCore};

/// Placeholder content for the temp markdown files opened by these tests.
const DOC: &str = "# Nav History\nbody\n";

/// The outer/global grid id. Window content lives on grids >= 2.
const GLOBAL_GRID: u64 = 1;

/// Gap between polls while waiting for a notification-driven change.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long to let late `wolf_buffers` / `wolf_preview` emits settle before
/// asserting that NOTHING was pushed. Generous so a late emit cannot sneak a false
/// positive past the assertion (mirrors `pane_buffers.rs`'s `UNNAMED_SETTLE`).
const HISTORY_SETTLE: Duration = Duration::from_secs(2);

/// The two-window count reached after a `:vsplit`.
const TWO_WINDOWS: usize = 2;

/// Starts a `WolfCore` rooted at an explicit `cwd` and waits for the first flush.
fn start_core_in(cwd: PathBuf) -> (WolfCore, FlushWaiter) {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(cwd, common::DEFAULT_COLS, common::DEFAULT_ROWS),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter)
}

/// Polls `core`'s snapshot until `pred` holds, draining wakes each iteration, or
/// panics after [`FLUSH_TIMEOUT`].
fn poll_until(core: &WolfCore, waiter: &FlushWaiter, desc: &str, pred: impl Fn(&Snapshot) -> bool) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        {
            let snap = core.read_snapshot();
            if pred(&snap) {
                return;
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Polls the snapshot until `pick` yields a window handle, and returns it.
fn poll_win_where(
    core: &WolfCore,
    waiter: &FlushWaiter,
    desc: &str,
    pick: impl Fn(&Snapshot) -> Option<i64>,
) -> i64 {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        {
            let snap = core.read_snapshot();
            if let Some(win) = pick(&snap) {
                return win;
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for a window: {desc}");
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Polls `core.pane_status(grid).filename` until it equals `name`.
fn poll_pane_filename(core: &WolfCore, waiter: &FlushWaiter, grid: u64, name: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let st = core.pane_status(grid);
        if st.filename == name {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for grid {grid} to show \
                 {name:?}; last observed filename={:?}",
                st.filename
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Polls `core.history_entries(win)` until `pred` holds, returning those rows.
fn poll_history(
    core: &WolfCore,
    waiter: &FlushWaiter,
    win: i64,
    desc: &str,
    pred: impl Fn(&[MenuRow]) -> bool,
) -> Vec<MenuRow> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let rows = core.history_entries(win);
        if pred(&rows) {
            return rows;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for {desc}; last observed \
                 rows: {:?}",
                displays(&rows)
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Burns `dur` draining wakes, so late notifications land BEFORE a
/// "nothing changed" assertion runs.
fn settle(waiter: &FlushWaiter, dur: Duration) {
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The `display` strings of `rows`, in row order (newest-first).
fn displays(rows: &[MenuRow]) -> Vec<String> {
    rows.iter().map(|r| r.display.clone()).collect()
}

/// The checkmarked row's display, or `None`.
fn current_display(rows: &[MenuRow]) -> Option<String> {
    rows.iter()
        .find(|r| r.is_current)
        .map(|r| r.display.clone())
}

/// Waits for an active WINDOW grid (id != 1) to appear and returns it.
fn wait_for_active_window_grid(core: &WolfCore, waiter: &FlushWaiter) -> u64 {
    poll_until(
        core,
        waiter,
        "an active window grid (id != 1) to appear",
        |snap| !snap.windows.is_empty() && snap.active_grid_id() != GLOBAL_GRID,
    );
    core.read_snapshot().active_grid_id()
}

/// Waits until `grid` has a resolved nvim WINDOW handle and returns it.
fn wait_for_win_handle(core: &WolfCore, waiter: &FlushWaiter, grid: u64) -> i64 {
    poll_win_where(core, waiter, &format!("a handle for grid {grid}"), |snap| {
        snap.windows.get(&grid).map(|w| w.win).filter(|w| *w != 0)
    })
}

/// Waits until window handle `win` has a grid in [`Snapshot::windows`] and returns
/// that grid id — the inverse of [`wait_for_win_handle`], for the tests that build a
/// pane by handle and then need its grid to read `pane_status`.
fn wait_for_grid_of_win(core: &WolfCore, waiter: &FlushWaiter, win: i64) -> u64 {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        {
            let snap = core.read_snapshot();
            if let Some((grid, _)) = snap.windows.iter().find(|(_, w)| w.win == win) {
                return *grid;
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for a grid for window {win}");
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The handle of the window nvim currently has FOCUSED, resolved through the grid
/// the cursor sits on, or `None` while no window grid is known.
fn active_win(core: &WolfCore) -> Option<i64> {
    let snap = core.read_snapshot();
    let grid = snap.active_grid_id();
    snap.windows.get(&grid).map(|w| w.win)
}

/// The set of live nvim window handles.
fn win_handles(core: &WolfCore) -> HashSet<i64> {
    core.read_snapshot()
        .windows
        .values()
        .map(|w| w.win)
        .collect()
}

/// Types an Ex command `:<cmd><CR>` one key at a time.
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// Seeds `cwd` with `a.md` / `b.md` / `c.md`.
fn seed_three_markdown_files(cwd: &Path) {
    for name in ["a.md", "b.md", "c.md"] {
        std::fs::write(cwd.join(name), DOC)
            .unwrap_or_else(|e| panic!("failed to write {name}: {e}"));
    }
}

/// Enters insert mode, types `text`, and returns to NORMAL mode — leaving the
/// focused window's buffer MODIFIED but never written to disk.
///
/// The trailing `<Esc>` matters: every caller sends further Ex commands through
/// [`ex_command`], which needs the cmdline, not an insert-mode `:`.
fn type_into_buffer(core: &WolfCore, text: &str) {
    core.send_input(ch('i'));
    type_str(core, text);
    core.send_input(key(Key::Esc));
}

/// The bufnr `Snapshot.win_buffers` reports for window `win`.
///
/// This reads the buffer the WINDOW actually holds (the `wolf_buffers` emit walks
/// windows, not the buffer list), which is the only place a `bufadd`-created
/// duplicate is visible: `bufadd` leaves the new buffer UNLISTED, so
/// [`buffer_is_modified`]'s `getbufinfo({'buflisted': 1})` never sees it.
fn win_bufnr(core: &WolfCore, win: i64) -> i64 {
    core.read_snapshot()
        .win_buffers
        .get(&win)
        .map(|b| b.bufnr)
        .unwrap_or_else(|| panic!("win_buffers has no entry for window {win}"))
}

/// Whether nvim reports LISTED buffer `bufnr` as modified (unsaved), via
/// `WolfCore::list_buffers` (`getbufinfo({'buflisted': 1})`).
fn buffer_is_modified(core: &WolfCore, bufnr: i64) -> bool {
    core.list_buffers()
        .iter()
        .any(|b| b.bufnr == bufnr && b.modified)
}

/// Polls until nvim reports `bufnr` as modified, or panics after [`FLUSH_TIMEOUT`].
fn poll_buffer_modified(core: &WolfCore, waiter: &FlushWaiter, bufnr: i64) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if buffer_is_modified(core, bufnr) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for buffer {bufnr} to \
                 become MODIFIED"
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

// =========================================================================
// 1. Edit navigation: build, back (LOAD-BEARING absorb), forward, truncate.
// =========================================================================

/// The core browser loop over `:e` navigation, including the **load-bearing
/// feedback-loop assertion**: `history_back` really MOVES the pane, and the
/// `wolf_buffers` emit that move causes is ABSORBED rather than pushed.
///
/// The absorb is witnessed WITHOUT reaching into private state: had the echo
/// pushed, it would have truncated `b.md` away and parked the cursor at the top,
/// so `can_forward` would be FALSE and the `b.md` row would be GONE. Both survive
/// only when the echo was absorbed.
#[test]
fn back_moves_the_pane_and_the_resulting_emit_is_absorbed() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);

    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");
    ex_command(&core, "e b.md");
    poll_pane_filename(&core, &waiter, grid, "b.md");

    // --- 1. `:e a.md` then `:e b.md` builds Edit(a) -> Edit(b) ---------------
    let rows_before = poll_history(
        &core,
        &waiter,
        win,
        "the two newest history rows to be b.md then a.md",
        |rows| rows.len() >= 2 && rows[0].display == "b.md" && rows[1].display == "a.md",
    );
    let displays_before = displays(&rows_before);
    assert!(
        core.history_can_back(win),
        "two entries => Back is available"
    );
    assert!(
        !core.history_can_forward(win),
        "the cursor is at the TOP => Forward is not available"
    );
    assert_eq!(
        current_display(&rows_before).as_deref(),
        Some("b.md"),
        "the checkmark follows the cursor"
    );

    // --- 2. history_back ACTUALLY MOVES THE PANE -----------------------------
    assert!(
        core.history_back(win),
        "history_back must report that the pane moved"
    );
    poll_pane_filename(&core, &waiter, grid, "a.md");

    // --- 3. ...and the resulting wolf_buffers emit is ABSORBED ---------------
    settle(&waiter, HISTORY_SETTLE);
    let rows_after = core.history_entries(win);
    assert_eq!(
        displays(&rows_after),
        displays_before,
        "the self-caused emit must not push: the rows (incl. b.md) are unchanged"
    );
    assert_eq!(
        current_display(&rows_after).as_deref(),
        Some("a.md"),
        "only the checkmark moved, onto the entry we navigated to"
    );
    assert!(
        core.history_can_forward(win),
        "LOAD-BEARING: Forward must still be available. A pushed echo would have \
         truncated b.md away and parked the cursor at the top."
    );

    // --- 4. history_forward returns to b.md ---------------------------------
    assert!(core.history_forward(win), "Forward must move the pane");
    poll_pane_filename(&core, &waiter, grid, "b.md");
    settle(&waiter, HISTORY_SETTLE);
    assert!(
        !core.history_can_forward(win),
        "back at the TOP => Forward is exhausted"
    );
    assert!(core.history_can_back(win), "Back is available again");
    assert_eq!(
        current_display(&core.history_entries(win)).as_deref(),
        Some("b.md"),
        "the checkmark followed Forward"
    );

    // --- 5. Truncate on new navigation: back to a.md, then `:e c.md` --------
    assert!(core.history_back(win), "Back to a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");
    settle(&waiter, HISTORY_SETTLE);

    ex_command(&core, "e c.md");
    poll_pane_filename(&core, &waiter, grid, "c.md");
    let rows = poll_history(
        &core,
        &waiter,
        win,
        "the two newest rows to be c.md then a.md",
        |rows| rows.len() >= 2 && rows[0].display == "c.md" && rows[1].display == "a.md",
    );
    assert!(
        !displays(&rows).iter().any(|d| d == "b.md"),
        "b.md was AHEAD of the cursor and must be truncated away: {:?}",
        displays(&rows)
    );
    assert!(
        !core.history_can_forward(win),
        "a new navigation parks the cursor at the top"
    );
    assert!(core.history_can_back(win), "a.md is still behind us");

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 2. Preview transition + preview link-follow.
// =========================================================================

/// A preview transition adds EXACTLY ONE entry — `Preview(a)` distinct from the
/// `Edit(a)` before it — with NO spurious entry for the transient frame in which
/// the window is in NEITHER map (the locked "no entry, no push, no prune" clause).
/// Following a preview link appends `Preview(b)`, and Back restores `Preview(a)`.
#[test]
fn preview_transition_and_link_follow_are_tracked() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);
    let b_abs = cwd.join("b.md").to_string_lossy().into_owned();

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);

    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");
    let rows_before = poll_history(&core, &waiter, win, "an a.md row on top", |rows| {
        !rows.is_empty() && rows[0].display == "a.md"
    });
    let count_before = rows_before.len();

    // --- 1. Turn THIS pane into a preview of a.md ---------------------------
    let preview_bufnr = core.open_preview("active");
    assert!(
        preview_bufnr > 0,
        "open_preview(\"active\") on a markdown buffer must return a scratch bufnr, \
         got {preview_bufnr}"
    );
    poll_until(
        &core,
        &waiter,
        "previews[win] to report a.md as the source",
        |snap| {
            snap.previews
                .get(&win)
                .is_some_and(|p| p.source_path.ends_with("a.md"))
        },
    );

    poll_history(&core, &waiter, win, "a Preview: a.md row on top", |rows| {
        !rows.is_empty() && rows[0].display == "Preview: a.md"
    });
    settle(&waiter, HISTORY_SETTLE);
    let rows = core.history_entries(win);
    assert_eq!(
        rows.len(),
        count_before + 1,
        "the preview transition adds EXACTLY ONE entry (no spurious entry for the \
         frame where the window is in neither map): {:?}",
        displays(&rows)
    );
    assert_eq!(
        rows[0].display, "Preview: a.md",
        "the newest row is the preview"
    );
    assert_eq!(
        rows[1].display, "a.md",
        "Edit(a) stays as its own DISTINCT entry beneath it"
    );

    // --- 2. Preview link-follow appends Preview(b) --------------------------
    let src = core.preview_file(win, &b_abs);
    assert!(
        src > 0,
        "preview_file(win, <abs b.md>) must return b.md's source bufnr, got {src}"
    );
    poll_until(
        &core,
        &waiter,
        "previews[win] to report b.md as the source",
        |snap| {
            snap.previews
                .get(&win)
                .is_some_and(|p| p.source_path.ends_with("b.md"))
        },
    );
    let rows = poll_history(&core, &waiter, win, "a Preview: b.md row on top", |rows| {
        !rows.is_empty() && rows[0].display == "Preview: b.md"
    });
    assert_eq!(
        rows.len(),
        count_before + 2,
        "the link-follow adds exactly one more entry: {:?}",
        displays(&rows)
    );

    // --- 3. Back restores Preview(a) ----------------------------------------
    assert!(core.history_back(win), "Back from Preview(b) must move");
    poll_until(
        &core,
        &waiter,
        "previews[win] to be restored to a.md",
        |snap| {
            snap.previews
                .get(&win)
                .is_some_and(|p| p.source_path.ends_with("a.md"))
        },
    );
    settle(&waiter, HISTORY_SETTLE);
    assert!(
        core.history_can_forward(win),
        "the preview Back must also be ABSORBED, leaving Forward available"
    );
    assert_eq!(
        current_display(&core.history_entries(win)).as_deref(),
        Some("Preview: a.md"),
        "the checkmark is on the restored preview entry"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 3. Terminals: a split :terminal IS tracked; the drawer is NOT.
// =========================================================================

/// A `:terminal` in a NORMAL split becomes a `Terminal { bufnr }` entry in THAT
/// pane's history (labeled `Terminal`), while the terminal DRAWER
/// (`toggle_terminal`) gets NO history at all.
///
/// The growth assertion guards the emit-coalescing fix: `buftype` only becomes
/// `"terminal"` at `TermOpen`, so folding every autocmd separately pushes THREE
/// entries for one `:terminal` (`Untitled{n}` -> `Edit("term://…")` ->
/// `Terminal{n}`). It is written as RELATIVE growth because the startup buffer
/// gives every pane an extra `Edit` entry, so absolute depth is not assertable.
#[test]
fn split_terminal_is_tracked_but_the_drawer_pane_never_gets_history() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win_a = wait_for_win_handle(&core, &waiter, grid);

    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");

    // --- 1. A :terminal in a NORMAL split IS in that pane's history ---------
    ex_command(&core, "vsplit");
    poll_until(&core, &waiter, "two live windows after :vsplit", |snap| {
        snap.windows.len() >= TWO_WINDOWS
    });

    // `:vsplit` focuses the NEW window, which inherits a.md. Measure that pane's
    // rows BEFORE the `:terminal`, so the growth below is relative.
    let split_win = poll_win_where(&core, &waiter, "the second (split) window", |snap| {
        snap.windows.values().map(|w| w.win).find(|w| *w != win_a)
    });
    poll_history(
        &core,
        &waiter,
        split_win,
        "an a.md row on top of the split",
        |rows| !rows.is_empty() && rows[0].display == "a.md",
    );
    settle(&waiter, HISTORY_SETTLE);
    let rows_before = displays(&core.history_entries(split_win));

    ex_command(&core, "terminal");

    let term_win = poll_win_where(
        &core,
        &waiter,
        "a non-drawer terminal window in win_buffers",
        |snap| {
            snap.win_buffers
                .iter()
                .find(|(_, b)| b.is_terminal && !b.is_drawer)
                .map(|(w, _)| *w)
        },
    );
    assert_eq!(
        term_win, split_win,
        "`:terminal` replaces the SPLIT window's buffer in place"
    );
    let rows = poll_history(&core, &waiter, term_win, "a Terminal row on top", |rows| {
        !rows.is_empty() && rows[0].display == "Terminal"
    });
    assert!(
        rows.len() >= 2 && rows[1].display == "a.md",
        "the split inherited a.md, so Terminal sits ON TOP of an Edit(a) entry: {:?}",
        displays(&rows)
    );
    assert!(
        core.history_can_back(term_win),
        "the split-terminal pane can go back to a.md"
    );

    // ONE user action => EXACTLY ONE new entry.
    settle(&waiter, HISTORY_SETTLE);
    let rows_after = displays(&core.history_entries(term_win));
    assert_eq!(
        rows_after.len(),
        rows_before.len() + 1,
        "one `:terminal` must grow the pane's stack by EXACTLY ONE entry — the \
         intermediate `Untitled{{n}}` / `Edit(\"term://…\")` states nvim passes \
         through must never be folded as their own entries. Before: {rows_before:?}, \
         after: {rows_after:?}"
    );
    assert_eq!(
        &rows_after[1..],
        &rows_before[..],
        "the Terminal entry sits on top of the pane's previous rows, unchanged"
    );

    // --- 2. The DRAWER gets no history at all -------------------------------
    // (`toggle_terminal` ends in `startinsert`, so no further Ex commands are sent
    // after this point.)
    let before = win_handles(&core);
    core.toggle_terminal();
    let drawer_win = poll_win_where(&core, &waiter, "the drawer window to appear", |snap| {
        snap.windows
            .values()
            .map(|w| w.win)
            .find(|w| !before.contains(w))
    });
    poll_until(
        &core,
        &waiter,
        "win_buffers to mark the drawer window with is_drawer",
        |snap| {
            snap.win_buffers
                .get(&drawer_win)
                .is_some_and(|b| b.is_drawer)
        },
    );

    settle(&waiter, HISTORY_SETTLE);
    assert!(
        core.history_entries(drawer_win).is_empty(),
        "the terminal DRAWER is EXCLUDED from history entirely, got {:?}",
        displays(&core.history_entries(drawer_win))
    );
    assert!(
        !core.history_can_back(drawer_win),
        "the drawer pane can never go back"
    );
    assert!(
        !core.history_can_forward(drawer_win),
        "the drawer pane can never go forward"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 4. Untitled (unnamed) buffers.
// =========================================================================

/// A RAW `:enew` leaves a genuinely UNNAMED buffer, tracked by bufnr and labeled
/// `Untitled`. Contrast with the startup buffer: `wolf_new_untitled` NAMES it
/// `<cwd>/Untitled.md`, so it is an `Edit` entry labeled `Untitled.md`. Both are
/// asserted here.
#[test]
fn unnamed_buffers_are_untitled_entries_named_ones_are_edits() {
    let cwd = common::fresh_cwd();

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);

    // The NAMED startup buffer is an Edit entry labeled with its basename.
    poll_history(
        &core,
        &waiter,
        win,
        "the named startup buffer to appear as an Untitled.md Edit row",
        |rows| rows.iter().any(|r| r.display == "Untitled.md"),
    );

    // A raw `:enew` bypasses `wolf_new_untitled`, so the buffer stays UNNAMED.
    ex_command(&core, "enew");
    poll_until(
        &core,
        &waiter,
        "win_buffers to report an unnamed buffer for this window",
        |snap| snap.win_buffers.get(&win).is_some_and(|b| b.path.is_none()),
    );

    let rows = poll_history(&core, &waiter, win, "an Untitled row on top", |rows| {
        !rows.is_empty() && rows[0].display == "Untitled"
    });
    assert!(
        rows.iter().any(|r| r.display == "Untitled.md"),
        "the NAMED startup buffer's Edit row is still distinct from the pathless \
         `Untitled` row: {:?}",
        displays(&rows)
    );
    assert!(
        core.history_can_back(win),
        "the unnamed buffer sits on top of the startup entry"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 5. `win_close` pruning.
// =========================================================================

/// Closing a window PRUNES that window's history (the `win_close` collector
/// resolves the handle from `Snapshot.windows` BEFORE the entry is removed).
#[test]
fn closing_a_window_prunes_its_history() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win_a = wait_for_win_handle(&core, &waiter, grid);

    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");

    // `:vsplit` focuses the NEW window; `:e b.md` lands there.
    ex_command(&core, "vsplit");
    poll_until(&core, &waiter, "two live windows after :vsplit", |snap| {
        snap.windows.len() >= TWO_WINDOWS
    });
    ex_command(&core, "e b.md");

    let win_b = poll_win_where(&core, &waiter, "the second (split) window", |snap| {
        snap.windows.values().map(|w| w.win).find(|w| *w != win_a)
    });
    poll_history(
        &core,
        &waiter,
        win_b,
        "a b.md row on top of the split",
        |rows| !rows.is_empty() && rows[0].display == "b.md",
    );

    // Close the focused (b.md) window.
    ex_command(&core, "close");
    poll_until(
        &core,
        &waiter,
        "the split window's grid to drop out",
        |snap| snap.windows.values().all(|w| w.win != win_b),
    );

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline && !core.history_entries(win_b).is_empty() {
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
    assert!(
        core.history_entries(win_b).is_empty(),
        "the closed window's history must be pruned, got {:?}",
        displays(&core.history_entries(win_b))
    );
    assert!(
        !core.history_can_back(win_b),
        "a pruned pane can never go back"
    );
    assert!(
        !core.history_entries(win_a).is_empty(),
        "the SURVIVING window's history is untouched"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 6. A failed restore reverts the cursor.
// =========================================================================

/// When the recorded target no longer exists (a WIPED `Terminal` bufnr), the Lua
/// helper (`_G.wolf_history_set_buf`, which returns `0` for an invalid buffer)
/// makes `history_back` report `false`, and the cursor is REVERTED — so Back stays
/// available and the pane does not silently drift.
///
/// This is the ONLY LIVE coverage of the `apply_nav` -> `NavHistory::revert`
/// wiring (revert is otherwise exercised headlessly at the `PaneHistory` level),
/// so all three properties below are load-bearing.
///
/// ## Why a `Terminal` target and not an `Untitled` one
///
/// An earlier version of this test wiped the buffer left by a raw `:enew`. That
/// premise is WRONG against the pinned nvim: `:e a.md` REUSES the empty unnamed
/// buffer `:enew` created (probed: `after :enew bufnr=1`, `after :e a.md bufnr=1,
/// reused=true`), so the "doomed" bufnr WAS a.md's buffer. Wiping it therefore
/// wiped the DISPLAYED buffer, moved the window back to the startup buffer (a
/// genuine pane change that pushed a fourth entry), and left Back targeting an
/// `Edit(a.md)` PATH target that `bufadd` happily recreates — so `history_back`
/// correctly returned `true` and nothing about the revert path was tested.
///
/// A `:terminal` buffer is immune to both problems: `:e` never reuses it, and
/// wiping a buffer that is NOT displayed anywhere does not move the window. The
/// window here is the SAME one throughout, so the pane genuinely cannot drift for
/// any reason other than a (wrongly) successful Back.
#[test]
fn a_failed_back_returns_false_and_reverts_the_cursor() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);

    // `:terminal` in THIS window (no split), recorded as `Terminal { bufnr }`.
    // A plain `:terminal` does NOT `startinsert` (only the drawer does), so the
    // Ex commands below still reach the cmdline.
    ex_command(&core, "terminal");
    poll_until(
        &core,
        &waiter,
        "win_buffers to report a non-drawer terminal for this window",
        |snap| {
            snap.win_buffers
                .get(&win)
                .is_some_and(|b| b.is_terminal && !b.is_drawer)
        },
    );
    let doomed_bufnr = core
        .read_snapshot()
        .win_buffers
        .get(&win)
        .map(|b| b.bufnr)
        .expect("the window must have a WinBuffer entry");
    assert!(doomed_bufnr > 0, "the terminal buffer has a real bufnr");
    poll_history(&core, &waiter, win, "a Terminal row on top", |rows| {
        !rows.is_empty() && rows[0].display == "Terminal"
    });

    // Navigate away in the SAME window ('hidden' is on, so the terminal buffer
    // survives as a hidden buffer), then WIPE it out from under history.
    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");
    poll_history(&core, &waiter, win, "an a.md row on top", |rows| {
        !rows.is_empty() && rows[0].display == "a.md"
    });
    assert!(
        core.history_can_back(win),
        "Back must be available BEFORE the wipe, or this test proves nothing"
    );

    ex_command(&core, &format!("bwipeout! {doomed_bufnr}"));
    settle(&waiter, HISTORY_SETTLE);
    // Wiping a buffer that is not displayed anywhere leaves the window alone, so
    // the pane is still on a.md going in.
    assert_eq!(
        core.pane_status(grid).filename,
        "a.md",
        "wiping the hidden terminal buffer must not move the pane"
    );
    let rows_before = displays(&core.history_entries(win));

    // --- 1. history_back reports FALSE when the target buffer is gone --------
    assert!(
        !core.history_back(win),
        "history_back must report FALSE when the recorded buffer is gone"
    );
    settle(&waiter, HISTORY_SETTLE);

    // --- 2. the pane did NOT move -------------------------------------------
    assert_eq!(
        core.pane_status(grid).filename,
        "a.md",
        "the pane must not have moved"
    );
    let still_showing_a_file = {
        let snap = core.read_snapshot();
        snap.win_buffers.get(&win).is_some_and(|b| !b.is_terminal)
    };
    assert!(
        still_showing_a_file,
        "the window is still showing the file buffer, not a terminal"
    );

    // --- 3. the cursor was REVERTED -----------------------------------------
    assert!(
        core.history_can_back(win),
        "the failed move must REVERT the cursor, so Back is still available"
    );
    assert_eq!(
        current_display(&core.history_entries(win)).as_deref(),
        Some("a.md"),
        "the checkmark is back on the entry we never left"
    );
    assert!(
        !core.history_can_forward(win),
        "the reverted cursor is back on the NEWEST entry, so Forward is exhausted"
    );
    assert_eq!(
        displays(&core.history_entries(win)),
        rows_before,
        "a failed move never edits the stack, only the cursor"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 7. Two panes keep independent stacks.
// =========================================================================

/// Histories are keyed by WINDOW handle, so two panes navigate independently.
#[test]
fn two_panes_have_independent_stacks() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);

    let (core, waiter) = start_core_in(cwd);
    let grid_a = wait_for_active_window_grid(&core, &waiter);
    let win_a = wait_for_win_handle(&core, &waiter, grid_a);

    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid_a, "a.md");

    ex_command(&core, "vsplit");
    poll_until(&core, &waiter, "two live windows after :vsplit", |snap| {
        snap.windows.len() >= TWO_WINDOWS
    });
    ex_command(&core, "e b.md");
    let win_b = poll_win_where(&core, &waiter, "the second (split) window", |snap| {
        snap.windows.values().map(|w| w.win).find(|w| *w != win_a)
    });
    poll_history(
        &core,
        &waiter,
        win_b,
        "a b.md row on top of pane B",
        |rows| !rows.is_empty() && rows[0].display == "b.md",
    );
    ex_command(&core, "e c.md");
    poll_history(
        &core,
        &waiter,
        win_b,
        "a c.md row on top of pane B",
        |rows| !rows.is_empty() && rows[0].display == "c.md",
    );

    // Pane A never left a.md.
    let rows_a = core.history_entries(win_a);
    assert_eq!(
        current_display(&rows_a).as_deref(),
        Some("a.md"),
        "pane A is still parked on a.md: {:?}",
        displays(&rows_a)
    );
    assert!(
        !rows_a.iter().any(|r| r.display == "c.md"),
        "pane B's navigation must NOT leak into pane A: {:?}",
        displays(&rows_a)
    );

    // Moving pane B leaves pane A alone.
    let a_before = displays(&core.history_entries(win_a));
    assert!(core.history_back(win_b), "pane B can go back");
    settle(&waiter, HISTORY_SETTLE);
    assert!(
        core.history_can_forward(win_b),
        "pane B's Back was absorbed, so Forward is available"
    );
    assert!(
        !core.history_can_forward(win_a),
        "pane A never moved, so it has no Forward"
    );
    assert_eq!(
        displays(&core.history_entries(win_a)),
        a_before,
        "pane A's rows are untouched by pane B's Back"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 8. `Snapshot.win_buffers` shape.
// =========================================================================

/// The NEW parallel map: keyed by WINDOW handle, carrying the real bufnr, the
/// canonicalized path (`None` for an unnamed buffer — unlike `Snapshot.buffers`,
/// which OMITS unnamed windows), and EXCLUDING preview scratch panes.
#[test]
fn win_buffers_carries_bufnrs_unnamed_windows_and_skips_previews() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);
    let canon_a = std::fs::canonicalize(cwd.join("a.md")).expect("a.md should canonicalize");

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);

    // --- 1. A named buffer: real bufnr + canonicalized path -----------------
    ex_command(&core, "e a.md");
    poll_until(&core, &waiter, "win_buffers[win] to report a.md", |snap| {
        snap.win_buffers
            .get(&win)
            .is_some_and(|b| b.path.as_deref() == Some(canon_a.as_path()))
    });
    let named_bufnr = {
        let snap = core.read_snapshot();
        let entry = snap
            .win_buffers
            .get(&win)
            .expect("win_buffers must contain the window");
        assert!(entry.bufnr > 0, "win_buffers carries a real nvim bufnr");
        assert!(!entry.is_terminal, "a.md is not a terminal buffer");
        assert!(!entry.is_drawer, "a.md is not the drawer");
        assert_eq!(
            snap.buffers.get(&win),
            Some(&canon_a),
            "the legacy `buffers` map still carries the same canonical path"
        );
        entry.bufnr
    };

    // --- 2. An UNNAMED window is PRESENT (path: None) -----------------------
    ex_command(&core, "enew");
    poll_until(
        &core,
        &waiter,
        "win_buffers[win] to report the unnamed buffer",
        |snap| snap.win_buffers.get(&win).is_some_and(|b| b.path.is_none()),
    );
    {
        let snap = core.read_snapshot();
        let entry = snap
            .win_buffers
            .get(&win)
            .expect("an UNNAMED window is still PRESENT in win_buffers");
        assert!(entry.bufnr > 0, "the unnamed buffer has a real bufnr");
        assert_ne!(
            entry.bufnr, named_bufnr,
            ":enew created a NEW buffer, so the bufnr changed"
        );
        assert!(
            !snap.buffers.contains_key(&win),
            "the legacy `buffers` map still OMITS unnamed windows (unchanged behavior)"
        );
    }

    // --- 3. A preview scratch pane is ABSENT from win_buffers ---------------
    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");
    let preview_bufnr = core.open_preview("split_right");
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") must return a scratch bufnr, got {preview_bufnr}"
    );
    let preview_win = poll_win_where(&core, &waiter, "a preview window", |snap| {
        snap.previews.keys().copied().next()
    });
    settle(&waiter, HISTORY_SETTLE);
    let snap = core.read_snapshot();
    assert!(
        !snap.win_buffers.contains_key(&preview_win),
        "preview scratch panes are SKIPPED by the wolf_buffers emit"
    );
    assert!(
        !snap.buffers.contains_key(&preview_win),
        "preview scratch panes are likewise absent from the legacy `buffers` map"
    );
    drop(snap);

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 9. The §B4 `bufadd` REBIND contract: Back to an UNSAVED buffer.
// =========================================================================

/// Navigating Back to an unsaved buffer rebinds **that same modified buffer**
/// instead of reading a file that was never written to disk.
///
/// This is the contract both plan §B4 and `crates/wolf-core/src/nvim.rs` call
/// **LOAD-BEARING**: the startup buffer is a NAMED `<cwd>/Untitled.md`
/// (`wolf_name_untitled`) that has no file behind it, so
/// `_G.wolf_history_open`'s `vim.fn.bufadd(<cwd>/Untitled.md)` must resolve to the
/// EXISTING buffer. [`unnamed_buffers_are_untitled_entries_named_ones_are_edits`]
/// only asserts the LABELS; nothing until now modified a buffer and navigated back
/// to it.
///
/// ## What actually discriminates
///
/// **Bufnr identity**, read from the `wolf_buffers` emit
/// ([`Snapshot::win_buffers`], which reports whatever buffer the WINDOW holds). A
/// duplicate created by `bufadd` is UNLISTED, so `getbufinfo({'buflisted': 1})`
/// never sees it and the ORIGINAL would still report modified — only the
/// per-window bufnr exposes the split. The modified flag is then asserted on the
/// bufnr actually BOUND, which is the user-visible symptom: a freshly `bufload`ed
/// duplicate reads clean and the unsaved edit appears to vanish.
///
/// ## Scope — this is the plain, non-symlinked form
///
/// `fresh_cwd()` alone cannot introduce a symlink anywhere in the path: nvim takes
/// its cwd from `getcwd(2)`, which the OS has ALREADY symlink-resolved, so every
/// buffer name nvim derives from it is canonical by construction and matches what
/// `std::fs::canonicalize` stored in the history stack.
/// [`back_to_a_symlinked_path_rebinds_the_same_buffer`] covers the same contract
/// for a file reached through an in-`cwd` directory symlink (and records, at
/// length, why that shape does not discriminate `wolf_history_open`'s
/// buffer-name scan either).
#[test]
fn back_to_an_unsaved_buffer_rebinds_the_same_modified_buffer() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);

    // --- 1. The NAMED, never-saved startup buffer ---------------------------
    poll_until(
        &core,
        &waiter,
        "win_buffers[win] to report the startup Untitled.md",
        |snap| {
            snap.win_buffers
                .get(&win)
                .and_then(|b| b.path.as_deref())
                .is_some_and(|p| p.ends_with("Untitled.md"))
        },
    );
    let untitled_bufnr = win_bufnr(&core, win);
    assert!(
        untitled_bufnr > 0,
        "the startup Untitled.md buffer has a real bufnr"
    );

    // --- 2. Make it DIRTY, without ever writing it to disk -------------------
    type_into_buffer(&core, "unsaved");
    poll_buffer_modified(&core, &waiter, untitled_bufnr);

    // --- 3. Navigate away ('hidden' is on, so it survives modified) ----------
    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");
    let rows = poll_history(&core, &waiter, win, "an a.md row on top", |rows| {
        !rows.is_empty() && rows[0].display == "a.md"
    });
    assert!(
        rows.iter().any(|r| r.display == "Untitled.md"),
        "the unsaved startup buffer is still an entry behind us: {:?}",
        displays(&rows)
    );
    assert!(
        core.history_can_back(win),
        "Back must be available, or this test proves nothing"
    );

    // --- 4. Back REBINDS the same modified buffer ---------------------------
    assert!(
        core.history_back(win),
        "Back to the unsaved buffer must report that the pane moved"
    );
    poll_pane_filename(&core, &waiter, grid, "Untitled.md");
    settle(&waiter, HISTORY_SETTLE);

    let bound = win_bufnr(&core, win);
    assert_eq!(
        bound, untitled_bufnr,
        "LOAD-BEARING (§B4): Back must rebind the EXISTING buffer for \
         <cwd>/Untitled.md, not `bufadd` a second one for a path that has no file \
         behind it"
    );
    assert!(
        buffer_is_modified(&core, bound),
        "the rebound buffer must still hold the UNSAVED edit; a duplicate loaded \
         fresh from disk would read clean (and be unlisted)"
    );
    assert!(
        core.history_can_forward(win),
        "the Back was absorbed, so a.md is still ahead of the cursor"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The general REBIND contract again, this time for a file reached through a
/// DIRECTORY symlink (`link -> real`, then `:e link/doc.md`): Back must land on
/// nvim's existing, still-MODIFIED buffer rather than a second one loaded fresh
/// from disk. Same shape as a project opened through a symlinked home directory.
///
/// ## This test does NOT discriminate the symlink hardening — recorded so a future
/// reader does not mistake it for a regression guard
///
/// `_G.wolf_history_open` scans `nvim_list_bufs()` for a buffer whose
/// `vim.fn.resolve(name)` equals the canonicalized stack path before falling back
/// to `vim.fn.bufadd(path)`. That scan was added as a fix for a suspected
/// duplicate-buffer bug, but it was then shown empirically NOT to be one: this test
/// passes with AND without the scan, because the pinned nvim 0.12.3 already covers
/// this shape twice over.
///
///   1. nvim resolves DIRECTORY symlinks in buffer names itself — after
///      `:e link/doc.md`, `nvim_buf_get_name(0)` is already `<cwd>/real/doc.md`,
///      exactly the canonical form core stores, so the two names never diverge here
///      at all.
///   2. Buffer lookup is by FILE ID (device + inode), not by name (`buflist_new` ->
///      `buflist_findname_file_id`), so even a genuinely divergent name (a FILE
///      symlink, whose name nvim does keep) still finds the SAME buffer whenever the
///      file exists on disk.
///
/// The only remaining name-only gap is a NEVER-WRITTEN buffer — and there
/// `std::fs::canonicalize` fails, so core stores the raw path and the names match
/// anyway (that case is
/// [`back_to_an_unsaved_buffer_rebinds_the_same_modified_buffer`]). Discriminating
/// the scan would require defeating nvim's directory resolution AND its file-id
/// match simultaneously; that was judged contrived and deliberately not built. The
/// scan is retained as defensive hardening — it makes the rebind independent of
/// which nvim mechanism happens to apply, and covers a buffer whose file was later
/// replaced or moved — and this test is retained as ordinary rebind coverage of the
/// symlinked-path shape.
#[test]
fn back_to_a_symlinked_path_rebinds_the_same_buffer() {
    let cwd = common::fresh_cwd();
    let real_dir = cwd.join("real");
    std::fs::create_dir_all(&real_dir).expect("failed to create the real/ directory");
    std::fs::write(real_dir.join("doc.md"), DOC).expect("failed to write real/doc.md");
    std::fs::write(cwd.join("other.md"), DOC).expect("failed to write other.md");
    symlink(&real_dir, cwd.join("link")).expect("failed to create the link -> real symlink");

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);

    // --- 1. Open doc.md THROUGH the symlink ---------------------------------
    // Core canonicalizes the emitted name to `<cwd>/real/doc.md` before pushing the
    // `Edit` entry. nvim resolves the directory symlink in the buffer name itself,
    // so its own name for the buffer is already that same resolved form.
    ex_command(&core, "e link/doc.md");
    poll_pane_filename(&core, &waiter, grid, "doc.md");
    let doc_bufnr = win_bufnr(&core, win);
    assert!(doc_bufnr > 0, "the symlinked doc.md has a real bufnr");

    // --- 2. Dirty it, so a duplicate buffer would LOSE the edit -------------
    type_into_buffer(&core, "unsaved");
    poll_buffer_modified(&core, &waiter, doc_bufnr);

    // --- 3. Navigate away, then Back ----------------------------------------
    ex_command(&core, "e other.md");
    poll_pane_filename(&core, &waiter, grid, "other.md");
    poll_history(&core, &waiter, win, "an other.md row on top", |rows| {
        !rows.is_empty() && rows[0].display == "other.md"
    });
    assert!(
        core.history_can_back(win),
        "Back must be available, or this test proves nothing"
    );
    assert!(
        core.history_back(win),
        "Back to the symlinked doc.md must report that the pane moved"
    );
    poll_pane_filename(&core, &waiter, grid, "doc.md");
    settle(&waiter, HISTORY_SETTLE);

    // --- 4. The SAME buffer, edits intact -----------------------------------
    let bound = win_bufnr(&core, win);
    assert_eq!(
        bound, doc_bufnr,
        "Back must bind nvim's EXISTING buffer for this file, never a second one \
         created for the canonicalized stack path"
    );
    assert!(
        buffer_is_modified(&core, bound),
        "the rebound buffer must still hold the UNSAVED edit — the whole point of \
         rebinding rather than re-reading from disk"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 10. Drawer toggle ROUND-TRIP leaves a survivor pane's history alone.
// =========================================================================

/// Opening AND then hiding the terminal drawer must never push into a REAL pane's
/// history.
///
/// [`split_terminal_is_tracked_but_the_drawer_pane_never_gets_history`] covers the
/// drawer's OWN (empty) history on the open path. This covers the SURVIVOR pane
/// across the full round trip, which is what plan §C1's final claim asserts and
/// nothing tested: the OPEN path needs `wolf_suppress_buffers` (the `botright
/// split` fires `WinNew`/`WinEnter` while the new window still shows the PREVIOUS
/// buffer), while the HIDE path is claimed to need no suppression at all — the
/// drawer window is gone, `win_close` prunes it, and the survivors' derived targets
/// are unchanged so the fold sees nothing new.
#[test]
fn toggling_the_drawer_never_touches_a_survivor_panes_history() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);

    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");
    poll_history(&core, &waiter, win, "an a.md row on top", |rows| {
        !rows.is_empty() && rows[0].display == "a.md"
    });
    settle(&waiter, HISTORY_SETTLE);
    let rows_before = displays(&core.history_entries(win));
    let can_back_before = core.history_can_back(win);
    let can_forward_before = core.history_can_forward(win);

    // --- 1. OPEN the drawer -------------------------------------------------
    let before = win_handles(&core);
    core.toggle_terminal();
    let drawer_win = poll_win_where(&core, &waiter, "the drawer window to appear", |snap| {
        snap.windows
            .values()
            .map(|w| w.win)
            .find(|w| !before.contains(w))
    });
    poll_until(
        &core,
        &waiter,
        "win_buffers to mark the drawer window with is_drawer",
        |snap| {
            snap.win_buffers
                .get(&drawer_win)
                .is_some_and(|b| b.is_drawer)
        },
    );
    settle(&waiter, HISTORY_SETTLE);
    assert_eq!(
        displays(&core.history_entries(win)),
        rows_before,
        "OPENING the drawer must not push into the survivor pane's history"
    );
    assert!(
        core.history_entries(drawer_win).is_empty(),
        "the drawer pane itself is excluded from history, got {:?}",
        displays(&core.history_entries(drawer_win))
    );

    // --- 2. HIDE the drawer again -------------------------------------------
    // `toggle_terminal` is an exec_lua RPC, so it works even though the drawer left
    // nvim in terminal-insert mode; no keystrokes are involved.
    core.toggle_terminal();
    poll_until(&core, &waiter, "the drawer window to disappear", |snap| {
        snap.windows.values().all(|w| w.win != drawer_win)
    });
    settle(&waiter, HISTORY_SETTLE);

    let rows_after = displays(&core.history_entries(win));
    assert_eq!(
        rows_after, rows_before,
        "§C1: the HIDE path needs no emit suppression — the drawer window is gone, \
         `win_close` prunes it, and the survivor's derived target never changed"
    );
    assert_eq!(
        core.history_can_back(win),
        can_back_before,
        "the survivor pane's Back availability is unchanged by the round trip"
    );
    assert_eq!(
        core.history_can_forward(win),
        can_forward_before,
        "the survivor pane's Forward availability is unchanged by the round trip"
    );
    assert_eq!(
        current_display(&core.history_entries(win)).as_deref(),
        Some("a.md"),
        "the survivor pane's cursor never moved: {rows_after:?}"
    );
    assert!(
        core.history_entries(drawer_win).is_empty(),
        "the closed drawer window still has no history, got {:?}",
        displays(&core.history_entries(drawer_win))
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 11. A failed Back to a PREVIEW target reverts the cursor.
// =========================================================================

/// The OTHER realistic failure plan §B6 names, alongside the wiped `Terminal`
/// bufnr that [`a_failed_back_returns_false_and_reverts_the_cursor`] covers: a
/// `Preview` whose SOURCE file has been deleted from disk.
///
/// It matters separately because it takes a DIFFERENT route out of `apply_nav` —
/// `_G.wolf_preview_file` (which returns `0` when `filereadable(path) == 0`, or
/// when the file is not Markdown) rather than `_G.wolf_history_set_buf`. The same
/// three properties are asserted: `history_back` reports `false`, the pane does not
/// move, and the cursor is REVERTED so Back stays available.
#[test]
fn a_failed_back_to_a_deleted_preview_source_reverts_the_cursor() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);
    let a_path = cwd.join("a.md");
    let b_abs = cwd.join("b.md").to_string_lossy().into_owned();

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);

    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");

    // --- 1. Preview(a), then a link-follow to Preview(b) --------------------
    let preview_bufnr = core.open_preview("active");
    assert!(
        preview_bufnr > 0,
        "open_preview(\"active\") on a markdown buffer must return a scratch bufnr, \
         got {preview_bufnr}"
    );
    poll_until(
        &core,
        &waiter,
        "previews[win] to report a.md as the source",
        |snap| {
            snap.previews
                .get(&win)
                .is_some_and(|p| p.source_path.ends_with("a.md"))
        },
    );
    poll_history(&core, &waiter, win, "a Preview: a.md row on top", |rows| {
        !rows.is_empty() && rows[0].display == "Preview: a.md"
    });

    let src = core.preview_file(win, &b_abs);
    assert!(
        src > 0,
        "preview_file(win, <abs b.md>) must return b.md's source bufnr, got {src}"
    );
    poll_until(
        &core,
        &waiter,
        "previews[win] to report b.md as the source",
        |snap| {
            snap.previews
                .get(&win)
                .is_some_and(|p| p.source_path.ends_with("b.md"))
        },
    );
    poll_history(&core, &waiter, win, "a Preview: b.md row on top", |rows| {
        !rows.is_empty() && rows[0].display == "Preview: b.md"
    });
    settle(&waiter, HISTORY_SETTLE);
    assert!(
        core.history_can_back(win),
        "Back must be available BEFORE the delete, or this test proves nothing"
    );
    let rows_before = displays(&core.history_entries(win));

    // --- 2. DELETE the previous preview's SOURCE from disk ------------------
    // The entry behind the cursor is `Preview(a.md)`. Removing the file makes
    // `wolf_preview_file`'s `filereadable` gate fail, which is the only way a
    // preview restore can report failure. No settle is needed: deleting a file
    // behind nvim's back fires no autocmd, and the history was quiesced above.
    std::fs::remove_file(&a_path).expect("failed to delete a.md");

    // --- 3. history_back reports FALSE --------------------------------------
    assert!(
        !core.history_back(win),
        "history_back must report FALSE when the recorded preview SOURCE is gone \
         (`wolf_preview_file` returns 0 for an unreadable path)"
    );
    settle(&waiter, HISTORY_SETTLE);

    // --- 4. the pane did NOT move -------------------------------------------
    let still_previewing_b = {
        let snap = core.read_snapshot();
        snap.previews
            .get(&win)
            .is_some_and(|p| p.source_path.ends_with("b.md"))
    };
    assert!(
        still_previewing_b,
        "the pane must still preview b.md — a failed restore never drifts the pane"
    );

    // --- 5. the cursor was REVERTED -----------------------------------------
    assert!(
        core.history_can_back(win),
        "the failed move must REVERT the cursor, so Back is still available"
    );
    assert!(
        !core.history_can_forward(win),
        "the reverted cursor is back on the NEWEST entry, so Forward is exhausted"
    );
    assert_eq!(
        current_display(&core.history_entries(win)).as_deref(),
        Some("Preview: b.md"),
        "the checkmark is back on the entry we never left"
    );
    assert_eq!(
        displays(&core.history_entries(win)),
        rows_before,
        "a failed move never edits the stack, only the cursor"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 12. A bind that THROWS after the validity checks reverts the cursor.
// =========================================================================

/// The THIRD way `apply_nav` can fail, and the one that used to be reported as a
/// SUCCESS: `_G.wolf_history_set_buf`'s two guards (`nvim_win_is_valid`,
/// `nvim_buf_is_valid`) both PASS and the `nvim_win_set_buf` inside
/// `_G.wolf_switch_buffer` then THROWS.
///
/// The two existing revert tests both fail at a gate, before any bind is attempted:
/// [`a_failed_back_returns_false_and_reverts_the_cursor`] wipes the target bufnr
/// (caught by `nvim_buf_is_valid`) and
/// [`a_failed_back_to_a_deleted_preview_source_reverts_the_cursor`] deletes the
/// source file (caught by `wolf_preview_file`'s `filereadable`). Neither reaches the
/// bind itself. `wolf_switch_buffer` wraps its whole body in a `pcall`, so a throw
/// there is SILENT; `wolf_history_set_buf` consumes that `pcall`'s ok flag and
/// returns `0` precisely so `apply_nav` reverts instead of leaving the history
/// cursor on an entry the pane never moved to.
///
/// ## How the throw is arranged
///
/// `'winfixbuf'` (nvim >= 0.10, so present in the pinned 0.12.3) pins a window to
/// its current buffer: `nvim_win_set_buf` into it raises `E1513` instead of
/// switching. Both handles stay VALID, so the guards above pass and the failure
/// lands exactly where this test needs it. (The other route the plan names is the
/// cmdwin, which a test cannot hold open across a blocking RPC.)
///
/// The last step — clearing `'winfixbuf'` and going Back again, successfully —
/// keeps the test honest twice over: it shows the `false` came from the pinned
/// window rather than from a Back that never works, and it shows the revert left the
/// stack usable rather than wedged.
#[test]
fn a_back_whose_bind_throws_returns_false_and_reverts_the_cursor() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);

    let (core, waiter) = start_core_in(cwd);
    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);

    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid, "a.md");
    ex_command(&core, "e b.md");
    poll_pane_filename(&core, &waiter, grid, "b.md");
    poll_history(&core, &waiter, win, "b.md over a.md", |rows| {
        rows.len() >= 2 && rows[0].display == "b.md" && rows[1].display == "a.md"
    });
    assert!(
        core.history_can_back(win),
        "Back must be available BEFORE the window is pinned, or this test proves \
         nothing"
    );

    // --- 1. PIN the window to its current buffer ----------------------------
    // 'winfixbuf' is window-local and fires no autocmd in the WolfBuffers augroup,
    // so the pane's derived target — and therefore its history — is untouched by it.
    // Both handles stay valid; only the bind will fail.
    ex_command(&core, "setlocal winfixbuf");
    settle(&waiter, HISTORY_SETTLE);
    let rows_before = displays(&core.history_entries(win));

    // --- 2. history_back reports FALSE --------------------------------------
    assert!(
        !core.history_back(win),
        "history_back must report FALSE when nvim_win_set_buf THROWS after the \
         validity checks pass — wolf_switch_buffer swallows the error in a pcall, so \
         propagating its ok flag as a 0 is the only signal core gets"
    );
    settle(&waiter, HISTORY_SETTLE);

    // --- 3. the pane did NOT move -------------------------------------------
    assert_eq!(
        core.pane_status(grid).filename,
        "b.md",
        "a pinned window cannot change buffer, so the pane is still on b.md"
    );
    let still_on_b = {
        let snap = core.read_snapshot();
        snap.win_buffers
            .get(&win)
            .and_then(|b| b.path.as_deref())
            .is_some_and(|p| p.ends_with("b.md"))
    };
    assert!(still_on_b, "win_buffers agrees the window never moved");

    // --- 4. the cursor was REVERTED -----------------------------------------
    assert!(
        core.history_can_back(win),
        "the failed move must REVERT the cursor, so Back is still available"
    );
    assert!(
        !core.history_can_forward(win),
        "the reverted cursor is back on the NEWEST entry, so Forward is exhausted"
    );
    assert_eq!(
        current_display(&core.history_entries(win)).as_deref(),
        Some("b.md"),
        "the checkmark is back on the entry we never left"
    );
    assert_eq!(
        displays(&core.history_entries(win)),
        rows_before,
        "a failed move never edits the stack, only the cursor"
    );

    // --- 5. UNPIN: the very same Back now succeeds --------------------------
    ex_command(&core, "setlocal nowinfixbuf");
    settle(&waiter, HISTORY_SETTLE);
    assert!(
        core.history_back(win),
        "with the window unpinned the very same Back must move the pane — proving \
         the FALSE above came from the failed bind, not from a Back that never works"
    );
    poll_pane_filename(&core, &waiter, grid, "a.md");
    settle(&waiter, HISTORY_SETTLE);
    assert!(
        core.history_can_forward(win),
        "the revert left the stack intact, so b.md is still ahead of the cursor"
    );
    assert_eq!(
        displays(&core.history_entries(win)),
        rows_before,
        "the successful Back was ABSORBED too: same rows, only the checkmark moved"
    );
    assert_eq!(
        current_display(&core.history_entries(win)).as_deref(),
        Some("a.md"),
        "the checkmark followed the successful Back"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// 13. Navigating a pane that is NOT nvim's current window.
// =========================================================================

/// Back in a BACKGROUND pane lands completely — the pane shows the new file, core's
/// snapshots agree, and focus never moves — with no further user input.
///
/// Every other test in this file drives the window nvim is currently in, where the
/// ordinary autocmds fire along the way. This one drives a window that is NOT
/// current, which is what `_G.wolf_history_set_buf`'s trailing
/// `_G.wolf_emit_previews()` + `_G.wolf_emit_current_file()` exist for (mirroring
/// `wolf_preview_file` / `wolf_edit_source`, which carry the same pair for the same
/// reason).
///
/// The stale-`previews` assertion is the sharp one. `derive_target` gives `previews`
/// PRECEDENCE over `win_buffers`, so an entry left behind for this window would keep
/// core deriving `Preview` for a pane that is now showing a FILE — and would keep the
/// front-end's overlay web view on a pane that has moved on.
///
/// ## What this does and does not discriminate
///
/// It asserts the observable CONTRACT (the pane is correct, unattended), not the
/// mechanism. `runtime/nvim/init.lua` makes two contradictory claims about whether a
/// background `nvim_win_set_buf` fires `BufWinEnter` —
/// `wolf_setup_current_file_tracking` says it DOES, `wolf_open_preview` says it does
/// NOT — so the state asserted here may also arrive via the ordinary autocmds, in
/// which case the test would still hold with the re-emits removed. Read it as
/// coverage of background navigation, NOT as a guard that pins the re-emits
/// themselves; see [`back_to_a_symlinked_path_rebinds_the_same_buffer`] for the same
/// distinction, drawn the hard way.
#[test]
fn back_in_a_background_pane_updates_it_without_moving_focus() {
    let cwd = common::fresh_cwd();
    seed_three_markdown_files(&cwd);

    let (core, waiter) = start_core_in(cwd);
    let grid_a = wait_for_active_window_grid(&core, &waiter);
    let win_a = wait_for_win_handle(&core, &waiter, grid_a);

    ex_command(&core, "e a.md");
    poll_pane_filename(&core, &waiter, grid_a, "a.md");

    // --- 1. A second pane, still focused, previewing a.md -------------------
    // `:vsplit` focuses the NEW window, which inherits a.md, so pane B's history
    // seeds with Edit(a); `open_preview("active")` then pushes Preview(a) on top.
    ex_command(&core, "vsplit");
    poll_until(&core, &waiter, "two live windows after :vsplit", |snap| {
        snap.windows.len() >= TWO_WINDOWS
    });
    let win_b = poll_win_where(&core, &waiter, "the second (split) window", |snap| {
        snap.windows.values().map(|w| w.win).find(|w| *w != win_a)
    });
    let grid_b = wait_for_grid_of_win(&core, &waiter, win_b);
    poll_pane_filename(&core, &waiter, grid_b, "a.md");

    let preview_bufnr = core.open_preview("active");
    assert!(
        preview_bufnr > 0,
        "open_preview(\"active\") on a markdown buffer must return a scratch bufnr, \
         got {preview_bufnr}"
    );
    poll_until(
        &core,
        &waiter,
        "previews[win_b] to report a.md as the source",
        |snap| {
            snap.previews
                .get(&win_b)
                .is_some_and(|p| p.source_path.ends_with("a.md"))
        },
    );
    let rows_b = poll_history(
        &core,
        &waiter,
        win_b,
        "a Preview: a.md row on top of pane B",
        |rows| !rows.is_empty() && rows[0].display == "Preview: a.md",
    );
    assert!(
        rows_b.len() >= 2 && rows_b[1].display == "a.md",
        "pane B inherited a.md, so Preview(a) sits on an Edit(a) entry: {:?}",
        displays(&rows_b)
    );

    // --- 2. Move focus AWAY: pane B is now a BACKGROUND window --------------
    core.focus_win(win_a);
    poll_until(&core, &waiter, "focus to land back on pane A", |snap| {
        snap.windows
            .get(&snap.active_grid_id())
            .is_some_and(|w| w.win == win_a)
    });
    settle(&waiter, HISTORY_SETTLE);
    assert_eq!(
        active_win(&core),
        Some(win_a),
        "pane A is nvim's CURRENT window going in — pane B is the background one"
    );
    assert_ne!(
        core.pane_status(grid_b).filename,
        "a.md",
        "precondition: a previewing pane is SKIPPED by the wolf_buffers emit, so \
         pane B's statusbar names no file at all right now"
    );
    let rows_b_before = displays(&core.history_entries(win_b));
    let rows_a_before = displays(&core.history_entries(win_a));

    // --- 3. Back in the pane that is NOT focused ----------------------------
    assert!(
        core.history_back(win_b),
        "Back in a background pane must report that the pane moved"
    );

    // --- 4. ...and it lands COMPLETELY, with no further user input ----------
    poll_pane_filename(&core, &waiter, grid_b, "a.md");
    poll_until(
        &core,
        &waiter,
        "previews to DROP the now-stale entry for pane B",
        |snap| !snap.previews.contains_key(&win_b),
    );
    settle(&waiter, HISTORY_SETTLE);
    {
        let snap = core.read_snapshot();
        assert!(
            !snap.previews.contains_key(&win_b),
            "a leftover previews entry would keep derive_target returning Preview for \
             a pane that is now showing a FILE (previews take PRECEDENCE)"
        );
        assert!(
            snap.win_buffers
                .get(&win_b)
                .and_then(|b| b.path.as_deref())
                .is_some_and(|p| p.ends_with("a.md")),
            "win_buffers must report the file pane B was moved to"
        );
    }
    assert_eq!(
        active_win(&core),
        Some(win_a),
        "the bind is WINDOW-SCOPED (nvim_win_set_buf, never `:edit`), so it must \
         never steal focus from pane A"
    );
    assert_eq!(
        core.pane_status(grid_a).filename,
        "a.md",
        "the focused pane is untouched by its neighbour's navigation"
    );

    // --- 5. the background move was ABSORBED, not pushed --------------------
    assert!(
        core.history_can_forward(win_b),
        "Preview(a) is still ahead of pane B's cursor, so Forward is available"
    );
    assert_eq!(
        current_display(&core.history_entries(win_b)).as_deref(),
        Some("a.md"),
        "the checkmark moved onto the Edit entry pane B navigated to"
    );
    assert_eq!(
        displays(&core.history_entries(win_b)),
        rows_b_before,
        "the self-caused emits must not push: pane B's rows are unchanged"
    );
    assert_eq!(
        displays(&core.history_entries(win_a)),
        rows_a_before,
        "pane A's history is untouched by pane B's Back"
    );

    core.shutdown().expect("shutdown should be Ok");
}
