//! Integration test for the `wolf-ffi` PER-PANE TABS bridge surface (plan §3 / §5).
//!
//! This drives the RUST half of the tabs machinery against a REAL, hermetic
//! Neovim — NO mocks, per the project testing policy. It is modeled on
//! `tests/buffer_picker.rs` (stateful single-nvim sequences, the SKIP-when-unset
//! guard, the wait-for-settle helpers) and `tests/panes.rs` (driving a `:vsplit`
//! and observing window grids appear/vanish via `windows()` / `grid_for_win`).
//!
//! ## Dependency on coder work (plan §3 / §5)
//!
//! None of the tab surface exists yet: the coder adds, on `WolfCoreHandle`,
//!   * `pane_tabs_for_grid(&self, grid: u64) -> Vec<FfiTab>`
//!   * `close_tab(&self, win: i64, bufnr: i64)`
//!   * `switch_tab(&self, win: i64, bufnr: i64)`  (win-EXPLICIT switch)
//! plus the opaque type
//!   * `FfiTab { fn bufnr() -> i64; fn display() -> String; fn active() -> bool;
//!     fn modified() -> bool }`
//! in `crates/wolf-ffi/src/lib.rs`, regenerates `generated/`, AND adds the
//! `//crates/wolf-ffi:tabs` `rust_test` target to `BUILD.bazel` (a copy of the
//! `buffer_picker` stanza, srcs `["tests/common/mod.rs", "tests/tabs.rs"]`,
//! `crate_root = "tests/tabs.rs"`). This test file does NOT touch non-test source
//! or BUILD files, and it will not compile until that lands — expected.
//!
//! It also reuses existing accessors: `active_win()`, `grid_for_win(win)`,
//! `windows()` (`grid()` / `win()` / `hidden()`), `focus_win(win)`,
//! `read_frame_for_grid(grid)` (`topline()`), `buffer_picker().ranked(query, max)`
//! (to enumerate LISTED buffers), and `send_key`.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim (`WOLF_NVIM_BIN` or the
//! Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available.

mod common;

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiBufferRow, FfiKeyEvent, FfiTab, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus each async tab/pane
/// effect (buffer open, tab append, close, view restore) to settle. Mirrors the
/// sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A quiet window to let a NO-OP settle (nothing should change) before asserting
/// the invariant still holds. Used for the single-pane single-tab close guard.
const SETTLE_WINDOW: Duration = Duration::from_millis(500);

/// A `max` large enough that the assembled buffer set is never truncated.
const RANKED_MAX: usize = 100;

/// Line count for each long temp buffer — comfortably larger than the 24-row grid
/// so a `G` (jump to last line) forces a NONZERO topline (view round-trip test).
const LINE_COUNT: usize = 200;

/// The lowest real WINDOW grid id (grid 1 is the outer global grid / cmdline
/// strip; editor windows are >= 2 under `ext_multigrid`).
const WINDOW_GRID_MIN: u64 = 2;

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false. Same as `buffer_picker.rs` / `panes.rs`.
fn char_key(c: char) -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_CHAR,
        ch: c as u32,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Builds an Enter `FfiKeyEvent` (kind=Enter), all modifiers false.
fn enter_key() -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_ENTER,
        ch: 0,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Types `cmd` as an Ex command and presses Enter, through the same
/// `send_key`/`FfiKeyEvent` path the generated Swift glue uses. `cmd` must include
/// the leading `:`. Copy of `buffer_picker.rs`'s `send_ex_command`.
fn send_ex_command(handle: &WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// Writes a one-line temp buffer file named `name` under `dir`, returning its
/// absolute path as a string.
fn write_file(dir: &Path, name: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, format!("contents of {name}\n")).expect("failed to write temp file");
    path.to_string_lossy().into_owned()
}

/// Writes a temp buffer file of [`LINE_COUNT`] numbered lines named `name` under
/// `dir`, returning its absolute path (used by the view round-trip test).
fn write_long_file(dir: &Path, name: &str) -> String {
    let mut body = String::new();
    for i in 1..=LINE_COUNT {
        body.push_str(&format!("line {i}\n"));
    }
    let path = dir.join(name);
    std::fs::write(&path, body).expect("failed to write temp long file");
    path.to_string_lossy().into_owned()
}

/// The live editor WINDOW grids (id + win handle) from `windows()`: grids >=
/// `WINDOW_GRID_MIN` that are not hidden. Copy of `panes.rs`'s `live_windows`.
fn live_windows(handle: &WolfCoreHandle) -> Vec<(u64, i64)> {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .map(|w| (w.grid(), w.win()))
        .collect()
}

/// The current tabs for `grid`.
fn pane_tabs(handle: &WolfCoreHandle, grid: u64) -> Vec<FfiTab> {
    handle.pane_tabs_for_grid(grid)
}

/// The ordered `display()` strings of a tab list.
fn tab_displays(tabs: &[FfiTab]) -> Vec<String> {
    tabs.iter().map(FfiTab::display).collect()
}

/// The `display()` of the ACTIVE tab (the one tracking the pane's current
/// buffer), or `None` when no tab is active.
fn active_tab_display(tabs: &[FfiTab]) -> Option<String> {
    tabs.iter().find(|t| t.active()).map(FfiTab::display)
}

/// The `bufnr()` of the first tab whose `display()` ends with `suffix`.
fn tab_bufnr_for_suffix(tabs: &[FfiTab], suffix: &str) -> Option<i64> {
    tabs.iter()
        .find(|t| t.display().ends_with(suffix))
        .map(FfiTab::bufnr)
}

/// True if some tab's `display()` ends with `suffix`.
fn tabs_contain_suffix(tabs: &[FfiTab], suffix: &str) -> bool {
    tabs.iter().any(|t| t.display().ends_with(suffix))
}

/// A fresh buffer-picker snapshot ranked over the empty query. The picker
/// snapshots `list_buffers()` (buflisted only) at construction, so this is the
/// canonical "is this buffer still a LISTED buffer?" probe.
fn ranked_snapshot(handle: &WolfCoreHandle) -> Vec<FfiBufferRow> {
    handle.buffer_picker().ranked(String::new(), RANKED_MAX)
}

/// True if any ranked row's `display()` ends with `suffix`.
fn any_display_ends_with(rows: &[FfiBufferRow], suffix: &str) -> bool {
    rows.iter().any(|r| r.display().ends_with(suffix))
}

/// The topline reported by a grid's per-grid frame (0-based, per `win_viewport`).
fn grid_topline(handle: &WolfCoreHandle, grid: u64) -> u64 {
    handle.read_frame_for_grid(grid).topline()
}

/// Polls `read_frame()` until the core attaches (cols > 0), or panics.
fn wait_for_attached(handle: &WolfCoreHandle) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut frame = handle.read_frame();
    while Instant::now() < deadline {
        frame = handle.read_frame();
        if frame.cols() > 0 {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(frame.cols() > 0, "core should attach (cols > 0, got {})", frame.cols());
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until `grid`'s ACTIVE tab display ends
/// with `suffix`, then returns the settled tab list. Panics on timeout. This is
/// the primary settle point after an `:e` / `switch_tab`, since the active tab
/// tracks the pane's current buffer.
fn wait_for_active_tab_suffix(
    handle: &WolfCoreHandle,
    grid: u64,
    suffix: &str,
    desc: &str,
) -> Vec<FfiTab> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let tabs = pane_tabs(handle, grid);
        if active_tab_display(&tabs).is_some_and(|d| d.ends_with(suffix)) {
            return tabs;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out waiting for active tab ending with {suffix:?} on grid {grid} ({desc})");
}

/// Blocks until `grid` reports exactly `n` tabs, then returns them. Panics on
/// timeout.
fn wait_for_tab_count(handle: &WolfCoreHandle, grid: u64, n: usize, desc: &str) -> Vec<FfiTab> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut tabs = pane_tabs(handle, grid);
    while Instant::now() < deadline {
        tabs = pane_tabs(handle, grid);
        if tabs.len() == n {
            return tabs;
        }
        sleep(POLL_INTERVAL);
    }
    panic!(
        "timed out waiting for {n} tabs on grid {grid} ({desc}); last = {:?}",
        tab_displays(&tabs)
    );
}

/// Blocks until a fresh ranked snapshot's containment of `suffix` equals
/// `present`, then returns. Panics on timeout. Drives the "still LISTED" vs
/// "no longer LISTED" assertions.
fn wait_for_ranked_presence(handle: &WolfCoreHandle, suffix: &str, present: bool, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        if any_display_ends_with(&ranked_snapshot(handle), suffix) == present {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out waiting for ranked presence of {suffix:?} == {present} ({desc})");
}

/// Blocks until exactly `n` live window grids exist, returning them. Panics on
/// timeout.
fn wait_for_window_count(handle: &WolfCoreHandle, n: usize, desc: &str) -> Vec<(u64, i64)> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut wins = live_windows(handle);
    while Instant::now() < deadline {
        wins = live_windows(handle);
        if wins.len() == n {
            return wins;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out waiting for {n} live windows ({desc}); got {wins:?}");
}

/// Starts a wolf-core session in a fresh temp cwd, returning the handle. The temp
/// dir is returned too so the caller keeps it alive until after `shutdown()`.
fn start_session() -> (WolfCoreHandle, tempfile::TempDir) {
    let init_lua = common::init_lua_path();
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
        0,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    (handle, tmp)
}

/// The single baseline editor window (grid, win) after attach. Panics if a single
/// window never surfaces.
fn baseline_window(handle: &WolfCoreHandle) -> (u64, i64) {
    let wins = wait_for_window_count(handle, 1, "one editor window at startup");
    wins[0]
}

// =============================================================================
// 1. Accumulation order + per-pane isolation (plan §5 cases 1 + 2)
// =============================================================================

#[test]
fn tabs_accumulate_in_open_order_and_are_isolated_per_pane() {
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let (handle, tmp) = start_session();
    let a = write_file(tmp.path(), "acc_a.md");
    let b = write_file(tmp.path(), "acc_b.md");
    let c = write_file(tmp.path(), "acc_c.md");
    let d = write_file(tmp.path(), "acc_d.md");

    wait_for_attached(&handle);
    let (grid1, win1) = baseline_window(&handle);

    // --- Accumulate A, B, C in one pane: tabs list [A, B, C] in OPEN order, and
    //     the active tab tracks the current buffer (C). ---------------------------
    send_ex_command(&handle, &format!(":e {a}"));
    wait_for_active_tab_suffix(&handle, grid1, "acc_a.md", "after :e A");
    send_ex_command(&handle, &format!(":e {b}"));
    wait_for_active_tab_suffix(&handle, grid1, "acc_b.md", "after :e B");
    send_ex_command(&handle, &format!(":e {c}"));
    let tabs = wait_for_active_tab_suffix(&handle, grid1, "acc_c.md", "after :e C");

    let displays = tab_displays(&tabs);
    assert_eq!(displays.len(), 3, "pane should have three tabs (got {displays:?})");
    assert!(displays[0].ends_with("acc_a.md"), "tab 0 is A (got {displays:?})");
    assert!(displays[1].ends_with("acc_b.md"), "tab 1 is B (got {displays:?})");
    assert!(displays[2].ends_with("acc_c.md"), "tab 2 is C (got {displays:?})");
    assert_eq!(
        active_tab_display(&tabs),
        Some(displays[2].clone()),
        "the active tab is the current buffer C"
    );
    // Freshly loaded files are unmodified — exercise modified().
    assert!(
        tabs.iter().all(|t| !t.modified()),
        "freshly loaded buffers report modified() == false"
    );

    // --- Switch BACK to A (win-explicit): the tab ORDER stays [A, B, C] (NO
    //     reordering), only the active tab moves to A. -----------------------------
    let bufnr_a = tab_bufnr_for_suffix(&tabs, "acc_a.md").expect("A has a tab bufnr");
    handle.switch_tab(win1, bufnr_a);
    let tabs = wait_for_active_tab_suffix(&handle, grid1, "acc_a.md", "after switch back to A");
    let displays = tab_displays(&tabs);
    assert_eq!(displays.len(), 3, "still three tabs after switching (got {displays:?})");
    assert!(displays[0].ends_with("acc_a.md"), "tab 0 stays A (got {displays:?})");
    assert!(displays[1].ends_with("acc_b.md"), "tab 1 stays B (got {displays:?})");
    assert!(displays[2].ends_with("acc_c.md"), "tab 2 stays C (got {displays:?})");

    // Return the pane to C so the upcoming split inherits C.
    let bufnr_c = tab_bufnr_for_suffix(&tabs, "acc_c.md").expect("C has a tab bufnr");
    handle.switch_tab(win1, bufnr_c);
    wait_for_active_tab_suffix(&handle, grid1, "acc_c.md", "return pane1 to C before split");

    // --- Per-pane isolation: :vsplit spawns pane2 (showing C, so its tabs start
    //     [C]); opening D there gives pane2 [C, D] while pane1 stays [A, B, C]. -----
    send_ex_command(&handle, ":vsplit");
    let wins = wait_for_window_count(&handle, 2, "two windows after :vsplit");
    let win2 = wins
        .iter()
        .map(|(_, w)| *w)
        .find(|w| *w != win1)
        .expect("a new window distinct from pane1");
    let grid2 = handle.grid_for_win(win2);
    // The new pane inherited C.
    wait_for_active_tab_suffix(&handle, grid2, "acc_c.md", "pane2 inherits C");

    // Open D in the new pane (which is now current after the split).
    send_ex_command(&handle, &format!(":e {d}"));
    let tabs2 = wait_for_active_tab_suffix(&handle, grid2, "acc_d.md", "pane2 after :e D");
    let displays2 = tab_displays(&tabs2);
    assert_eq!(displays2.len(), 2, "pane2 has exactly [C, D] (got {displays2:?})");
    assert!(displays2[0].ends_with("acc_c.md"), "pane2 tab 0 is C (got {displays2:?})");
    assert!(displays2[1].ends_with("acc_d.md"), "pane2 tab 1 is D (got {displays2:?})");
    assert!(
        !tabs_contain_suffix(&tabs2, "acc_a.md") && !tabs_contain_suffix(&tabs2, "acc_b.md"),
        "pane2 must NOT show pane1's A or B tabs (got {displays2:?})"
    );

    // pane1 is untouched by the new pane's D: still [A, B, C], no D.
    let tabs1 = wait_for_tab_count(&handle, grid1, 3, "pane1 unchanged after pane2 :e D");
    let displays1 = tab_displays(&tabs1);
    assert!(displays1[0].ends_with("acc_a.md"), "pane1 tab 0 stays A (got {displays1:?})");
    assert!(displays1[1].ends_with("acc_b.md"), "pane1 tab 1 stays B (got {displays1:?})");
    assert!(displays1[2].ends_with("acc_c.md"), "pane1 tab 2 stays C (got {displays1:?})");
    assert!(
        !tabs_contain_suffix(&tabs1, "acc_d.md"),
        "pane1 must NOT gain the D tab opened in pane2 (got {displays1:?})"
    );

    handle.shutdown();
    drop(tmp);
}

// =============================================================================
// 2. CloseTab: preserve-when-tabbed-elsewhere + right neighbor, delete-when-last,
//    last-tab-closes-pane, and the single-pane single-tab no-op (plan §5 cases
//    3 + 4 + 5).
// =============================================================================

#[test]
fn close_tab_preserves_deletes_and_manages_pane_lifecycle() {
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let (handle, tmp) = start_session();
    let x = write_file(tmp.path(), "life_x.md");
    let z = write_file(tmp.path(), "life_z.md");
    let w = write_file(tmp.path(), "life_w.md");

    wait_for_attached(&handle);
    let (grid1, win1) = baseline_window(&handle);

    // --- Build pane1 = [X, Z] with X ACTIVE. ---------------------------------
    send_ex_command(&handle, &format!(":e {x}"));
    wait_for_active_tab_suffix(&handle, grid1, "life_x.md", "pane1 :e X");
    send_ex_command(&handle, &format!(":e {z}"));
    let tabs1 = wait_for_active_tab_suffix(&handle, grid1, "life_z.md", "pane1 :e Z");
    let bufnr_x = tab_bufnr_for_suffix(&tabs1, "life_x.md").expect("X tab bufnr");
    handle.switch_tab(win1, bufnr_x);
    wait_for_active_tab_suffix(&handle, grid1, "life_x.md", "pane1 active back to X");

    // --- :vsplit -> pane2 inherits X ([X]); open W there -> pane2 = [X, W]. ----
    send_ex_command(&handle, ":vsplit");
    let wins = wait_for_window_count(&handle, 2, "two windows after :vsplit");
    let win2 = wins
        .iter()
        .map(|(_, ww)| *ww)
        .find(|ww| *ww != win1)
        .expect("a new window distinct from pane1");
    let grid2 = handle.grid_for_win(win2);
    wait_for_active_tab_suffix(&handle, grid2, "life_x.md", "pane2 inherits X");
    send_ex_command(&handle, &format!(":e {w}"));
    wait_for_active_tab_suffix(&handle, grid2, "life_w.md", "pane2 :e W");

    // === Case 3: close the ACTIVE X tab in pane1 (index 0). X is tabbed in pane2
    //     too, so the BUFFER is preserved; pane1 activates the RIGHT neighbor Z. ===
    handle.close_tab(win1, bufnr_x);
    let tabs1 = wait_for_tab_count(&handle, grid1, 1, "pane1 loses X, keeps Z");
    assert!(
        tabs1[0].display().ends_with("life_z.md"),
        "pane1's surviving tab is Z (got {:?})",
        tab_displays(&tabs1)
    );
    assert_eq!(
        active_tab_display(&tabs1),
        Some(tabs1[0].display()),
        "closing active X activates the RIGHT neighbor Z"
    );
    // pane2 still shows X, and X is still a LISTED buffer (not deleted).
    assert!(
        tabs_contain_suffix(&pane_tabs(&handle, grid2), "life_x.md"),
        "pane2 still holds the X tab"
    );
    wait_for_ranked_presence(&handle, "life_x.md", true, "X preserved while tabbed in pane2");

    // === Case 4: close the LAST reference to X (its inactive tab in pane2). Now
    //     no pane tabs X, so the BUFFER is deleted (no longer LISTED). ===========
    handle.close_tab(win2, bufnr_x);
    let tabs2 = wait_for_tab_count(&handle, grid2, 1, "pane2 loses X, keeps W");
    assert!(
        tabs2[0].display().ends_with("life_w.md"),
        "pane2's surviving tab is W (got {:?})",
        tab_displays(&tabs2)
    );
    assert!(
        !tabs_contain_suffix(&tabs2, "life_x.md"),
        "pane2 no longer shows X"
    );
    wait_for_ranked_presence(&handle, "life_x.md", false, "X deleted when its last tab closes");

    // === Case 5a: close pane2's ONLY tab (W). With two windows the pane FALLS
    //     THROUGH to closing itself; pane1 survives. W was pane2-only -> deleted. ==
    let bufnr_w = tab_bufnr_for_suffix(&tabs2, "life_w.md").expect("W tab bufnr");
    handle.close_tab(win2, bufnr_w);
    let survivors = wait_for_window_count(&handle, 1, "pane2 closes on last-tab close");
    assert_eq!(survivors[0].1, win1, "pane1 is the survivor");
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline && handle.grid_for_win(win2) != 0 {
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        handle.grid_for_win(win2),
        0,
        "pane2's grid vanishes once its last tab (and the pane) closes"
    );
    wait_for_ranked_presence(&handle, "life_w.md", false, "W deleted with pane2's last tab");

    // === Case 5b (E444 guard): single pane, single tab -> close_tab is a SAFE
    //     NO-OP. The window count must NOT change (the last window is never
    //     closed). pane1 now holds only Z. ========================================
    let bufnr_z = tab_bufnr_for_suffix(&pane_tabs(&handle, grid1), "life_z.md")
        .expect("Z tab bufnr on the sole survivor");
    assert_eq!(live_windows(&handle).len(), 1, "one window before the no-op close");
    handle.close_tab(win1, bufnr_z);
    // Give the (no-)op time to NOT close the window, then assert the invariant.
    sleep(SETTLE_WINDOW);
    assert_eq!(
        live_windows(&handle).len(),
        1,
        "closing the only tab of the only pane leaves the window count unchanged (E444 guard)"
    );

    // Decision #5 (buffer + tab PRESERVATION): the no-op must also leave the tab and
    // its buffer intact — the X close affordance bypasses the ⌘W menu gate, so this
    // preservation is user-reachable and is PINNED here (not just window-count). The
    // pane keeps its single Z tab, that tab is still Z, and Z is still a LISTED
    // buffer. `wait_for_ranked_presence(..., true, ...)` returns immediately when Z is
    // already present (the no-op never delisted it), preserving the settle discipline.
    let survivor_tabs = pane_tabs(&handle, grid1);
    assert_eq!(
        survivor_tabs.len(),
        1,
        "the no-op close keeps the pane's single tab (got {:?})",
        tab_displays(&survivor_tabs)
    );
    assert!(
        survivor_tabs[0].display().ends_with("life_z.md"),
        "the surviving tab is still Z after the no-op close (got {:?})",
        tab_displays(&survivor_tabs)
    );
    wait_for_ranked_presence(&handle, "life_z.md", true, "Z preserved by the E444 no-op guard");

    handle.shutdown();
    drop(tmp);
}

// =============================================================================
// 3. CloseTab of the active LAST tab activates the LEFT neighbor, and deletes the
//    closed buffer when it was its last tab anywhere (plan §5 cases 3-left + 4).
// =============================================================================

#[test]
fn close_active_last_tab_activates_left_neighbor_and_deletes() {
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let (handle, tmp) = start_session();
    let a = write_file(tmp.path(), "left_a.md");
    let b = write_file(tmp.path(), "left_b.md");

    wait_for_attached(&handle);
    let (grid1, win1) = baseline_window(&handle);

    // Single pane [A, B], B active (the LAST tab).
    send_ex_command(&handle, &format!(":e {a}"));
    wait_for_active_tab_suffix(&handle, grid1, "left_a.md", "pane :e A");
    send_ex_command(&handle, &format!(":e {b}"));
    let tabs = wait_for_active_tab_suffix(&handle, grid1, "left_b.md", "pane :e B");
    let bufnr_b = tab_bufnr_for_suffix(&tabs, "left_b.md").expect("B tab bufnr");

    // Close the active LAST tab B -> activate the LEFT neighbor A; B had no other
    // tab anywhere, so its buffer is deleted (no longer LISTED). The pane itself
    // survives (A remains).
    handle.close_tab(win1, bufnr_b);
    let tabs = wait_for_tab_count(&handle, grid1, 1, "pane loses B, keeps A");
    assert!(
        tabs[0].display().ends_with("left_a.md"),
        "the surviving tab is A (got {:?})",
        tab_displays(&tabs)
    );
    assert_eq!(
        active_tab_display(&tabs),
        Some(tabs[0].display()),
        "closing the active LAST tab activates the LEFT neighbor A"
    );
    assert_eq!(live_windows(&handle).len(), 1, "the pane survives (A remains)");
    wait_for_ranked_presence(&handle, "left_b.md", false, "B deleted when its only tab closes");

    handle.shutdown();
    drop(tmp);
}

// =============================================================================
// 4. Neighbor VIEW round-trip (plan §5 case 6).
//
// FLAKY-SENSITIVE: this asserts that closing the active tab RESTORES the right
// neighbor's saved `(winid, bufnr)` view (topline), via the winsaveview/winrestview
// round-trip the close path reuses (buffer_picker.rs ~319-342). Real-nvim viewport
// timing can jitter; if this proves flaky in CI, keep it QUARANTINED but do not let
// it mask the robust core assertions in tests 1-3.
// =============================================================================

#[test]
fn closing_active_tab_restores_right_neighbor_view() {
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let (handle, tmp) = start_session();
    let a = write_long_file(tmp.path(), "view_a.md");
    let b = write_long_file(tmp.path(), "view_b.md");

    wait_for_attached(&handle);
    let (grid1, win1) = baseline_window(&handle);

    // Pane = [A, B]. Open A, then B (active B, the right neighbor of A).
    send_ex_command(&handle, &format!(":e {a}"));
    wait_for_active_tab_suffix(&handle, grid1, "view_a.md", "pane :e A");
    send_ex_command(&handle, &format!(":e {b}"));
    let tabs = wait_for_active_tab_suffix(&handle, grid1, "view_b.md", "pane :e B");
    let bufnr_a = tab_bufnr_for_suffix(&tabs, "view_a.md").expect("A tab bufnr");

    // Scroll B to its bottom (`G`) so B's topline is NONZERO; capture it.
    handle.send_key(char_key('G'));
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut topline_b = grid_topline(&handle, grid1);
    while Instant::now() < deadline {
        topline_b = grid_topline(&handle, grid1);
        if topline_b > 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(topline_b > 0, "G should force a NONZERO topline for B (got {topline_b})");

    // Switch away to A (win-explicit) — this SAVES B's view for (win1, B).
    handle.switch_tab(win1, bufnr_a);
    wait_for_active_tab_suffix(&handle, grid1, "view_a.md", "switch to A (saves B's view)");

    // Close the active A tab (index 0). The RIGHT neighbor is B, so the close path
    // restores (win1, B)'s saved view: the topline returns to topline_b.
    handle.close_tab(win1, bufnr_a);
    wait_for_active_tab_suffix(&handle, grid1, "view_b.md", "closing A activates right neighbor B");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut restored = grid_topline(&handle, grid1);
    while Instant::now() < deadline {
        restored = grid_topline(&handle, grid1);
        if restored == topline_b {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        restored, topline_b,
        "closing active A restores the right neighbor B's saved view (topline {topline_b})"
    );

    handle.shutdown();
    drop(tmp);
}
