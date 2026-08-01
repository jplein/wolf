//! Integration test for the `wolf-ffi` NAVIGATION-HISTORY bridge surface (plan
//! §B7): the six `WolfCoreHandle::history_*` methods and the opaque
//! `FfiHistoryEntry` rows (`display()` / `index()` / `is_current()`) the macOS
//! History menu is built from.
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is modeled on `tests/preview.rs`, reusing
//! `tests/common/mod.rs` and the same loud SKIP-when-no-nvim guard.
//!
//! ## NOTE to coder — Bazel target
//!
//! This file needs a `//crates/wolf-ffi:history` `rust_test` target added to
//! `crates/wolf-ffi/BUILD.bazel` (a copy of the `:preview` stanza:
//! `srcs = ["tests/common/mod.rs", "tests/history.rs"]`,
//! `crate_root = "tests/history.rs"`). This test file does NOT touch BUILD files.
//!
//! ## Dependency on coder work (plan §B7)
//!
//! None of this exists yet — the coder adds, on `WolfCoreHandle`:
//!   * `history_can_back(&self, win: i64) -> bool`
//!   * `history_can_forward(&self, win: i64) -> bool`
//!   * `history_back(&self, win: i64) -> bool`
//!   * `history_forward(&self, win: i64) -> bool`
//!   * `history_jump(&self, win: i64, index: u32) -> bool`
//!   * `history_entries(&self, win: i64) -> Vec<FfiHistoryEntry>`
//!     plus the OPAQUE `FfiHistoryEntry { display() -> String, index() -> u32,
//! is_current() -> bool }` (opaque, not a `swift_repr = "struct"`, so a
//!     `Vec<FfiHistoryEntry>` is supported), and regenerates `generated/`. This test
//!     file will not compile until that lands — expected.
//!
//! ## The startup buffer is a NAMED `Untitled.md`
//!
//! The anonymous-buffer feature names the startup buffer `<cwd>/Untitled.md`, so
//! every pane's history starts with an `Edit` row for it. These assertions key on
//! the newest-first ROW PREFIX and on relative behavior, never on an absolute
//! stack depth.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim (`WOLF_NVIM_BIN` or the
//! Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available.

mod common;

use std::collections::HashSet;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiHistoryEntry, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (the `wolf_buffers` emit, the history fold) to settle. Mirrors the siblings.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long to let late emits settle before asserting that NOTHING was pushed.
const HISTORY_SETTLE: Duration = Duration::from_secs(2);

/// A window handle nvim will never issue, for the unknown-window contract.
const UNKNOWN_WIN: i64 = 987_654;

/// A stack index far past any real history, for the out-of-range contract.
const OUT_OF_RANGE_INDEX: u32 = 9_999;

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`.
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

/// Builds an Enter `FfiKeyEvent` (kind=Enter).
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

/// Types `cmd` as an Ex command and presses Enter. `cmd` must include the `:`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the attached active frame reports a
/// non-empty grid (`cols > 0`), the sibling tests' attach handshake.
fn wait_for_attach(handle: &wolf_ffi::WolfCoreHandle) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut frame = handle.read_frame();
    while Instant::now() < deadline {
        frame = handle.read_frame();
        if frame.cols() > 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        frame.cols() > 0,
        "core should attach and report a non-empty active frame (cols, got {})",
        frame.cols()
    );
}

/// Blocks until `current_file()` ends with `suffix` (a BASENAME match, robust to
/// `/var` <-> `/private/var` symlinking), deterministically confirming that file is
/// the ACTIVE buffer.
fn wait_for_current_file(handle: &wolf_ffi::WolfCoreHandle, suffix: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut current = handle.current_file();
    while Instant::now() < deadline {
        current = handle.current_file();
        if current.ends_with(suffix) {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!(
        "timed out after {FLUSH_TIMEOUT:?} waiting for current_file() to end with \
         {suffix} (got {current})"
    );
}

/// The `display()` strings of `rows`, in row order (newest-first).
fn displays(rows: &[FfiHistoryEntry]) -> Vec<String> {
    rows.iter().map(FfiHistoryEntry::display).collect()
}

/// Blocks until `predicate` holds on a fresh `history_entries(win)` snapshot,
/// returning it. Panics with `desc` on timeout.
fn wait_for_entries(
    handle: &wolf_ffi::WolfCoreHandle,
    win: i64,
    desc: &str,
    predicate: impl Fn(&[FfiHistoryEntry]) -> bool,
) -> Vec<FfiHistoryEntry> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let rows = handle.history_entries(win);
        if predicate(&rows) {
            return rows;
        }
        sleep(POLL_INTERVAL);
    }
    panic!(
        "timed out after {FLUSH_TIMEOUT:?} waiting for history_entries({win}) ({desc}); \
         last: {:?}",
        displays(&handle.history_entries(win))
    );
}

/// Blocks until `handle.active_win()` reports a real (non-zero) handle.
fn wait_for_active_win(handle: &wolf_ffi::WolfCoreHandle) -> i64 {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut win = handle.active_win();
    while Instant::now() < deadline {
        win = handle.active_win();
        if win != 0 {
            return win;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for a non-zero active_win (got {win})");
}

/// Seeds a fresh temp cwd with `a.md` / `b.md` / `c.md`.
fn seed_three_markdown_files() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    for name in ["a.md", "b.md", "c.md"] {
        std::fs::write(tmp.path().join(name), "# Heading\n\nbody\n")
            .unwrap_or_else(|e| panic!("failed to write {name}: {e}"));
    }
    tmp
}

/// The full happy path over the bridge: the rows follow core (newest-first,
/// exactly one `is_current()`), `history_back` / `history_forward` return `true`
/// and MOVE the pane, and a row's `index()` round-trips through `history_jump`.
#[test]
fn history_bridge_tracks_navigation_back_forward_and_jump() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let tmp = seed_three_markdown_files();

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; visit a.md -> b.md -> c.md in the one pane ---------------
    wait_for_attach(&handle);
    let win = wait_for_active_win(&handle);

    send_ex_command(&handle, ":e a.md");
    wait_for_current_file(&handle, "a.md");
    send_ex_command(&handle, ":e b.md");
    wait_for_current_file(&handle, "b.md");
    send_ex_command(&handle, ":e c.md");
    wait_for_current_file(&handle, "c.md");

    // --- 2. history_entries mirrors core: newest-first, one checkmark --------
    let rows = wait_for_entries(&handle, win, "c.md / b.md / a.md newest-first", |rows| {
        rows.len() >= 3
            && rows[0].display() == "c.md"
            && rows[1].display() == "b.md"
            && rows[2].display() == "a.md"
    });
    assert_eq!(
        rows.iter().filter(|r| r.is_current()).count(),
        1,
        "exactly one row draws the checkmark: {:?}",
        displays(&rows)
    );
    assert!(
        rows[0].is_current(),
        "the checkmark is on the newest (current) row"
    );
    // Indices are STACK indices: strictly decreasing newest-first, all distinct.
    let idx: Vec<u32> = rows.iter().map(FfiHistoryEntry::index).collect();
    assert!(
        idx.windows(2).all(|w| w[0] > w[1]),
        "row indices are real stack indices, descending newest-first: {idx:?}"
    );
    assert!(
        handle.history_can_back(win),
        "three entries => Back is available"
    );
    assert!(
        !handle.history_can_forward(win),
        "the cursor is at the top => Forward is not"
    );

    // --- 3. history_back returns true AND moves the pane --------------------
    assert!(
        handle.history_back(win),
        "history_back must return true when the pane moved"
    );
    wait_for_current_file(&handle, "b.md");
    sleep(HISTORY_SETTLE);
    assert!(
        handle.history_can_forward(win),
        "the self-caused emit must be ABSORBED, leaving Forward available"
    );
    assert_eq!(
        handle
            .history_entries(win)
            .iter()
            .find(|r| r.is_current())
            .map(FfiHistoryEntry::display),
        Some("b.md".to_string()),
        "the checkmark followed Back"
    );

    // --- 4. history_forward returns true AND moves the pane -----------------
    assert!(handle.history_forward(win), "history_forward must move");
    wait_for_current_file(&handle, "c.md");
    sleep(HISTORY_SETTLE);
    assert!(
        !handle.history_can_forward(win),
        "back at the top => Forward is exhausted"
    );

    // --- 5. A row's index() round-trips through history_jump ----------------
    let rows = handle.history_entries(win);
    let a_index = rows
        .iter()
        .find(|r| r.display() == "a.md")
        .map(FfiHistoryEntry::index)
        .expect("an a.md row must exist");
    assert!(
        handle.history_jump(win, a_index),
        "history_jump to a real row index must return true"
    );
    wait_for_current_file(&handle, "a.md");
    sleep(HISTORY_SETTLE);
    assert_eq!(
        handle
            .history_entries(win)
            .iter()
            .find(|r| r.is_current())
            .map(FfiHistoryEntry::display),
        Some("a.md".to_string()),
        "the checkmark followed the jump"
    );

    // --- 6. Out-of-range jump is a false no-op ------------------------------
    let before = displays(&handle.history_entries(win));
    assert!(
        !handle.history_jump(win, OUT_OF_RANGE_INDEX),
        "an out-of-range index must return false"
    );
    sleep(HISTORY_SETTLE);
    assert_eq!(
        displays(&handle.history_entries(win)),
        before,
        "a rejected jump must leave history untouched"
    );

    handle.shutdown();
    drop(tmp);
}

/// The NEUTRAL-FALLBACK contract every sibling asserts: an unknown window, the
/// terminal DRAWER window, and a SHUT-DOWN core all yield `false` / an empty
/// `Vec` from every history method.
#[test]
fn history_bridge_is_neutral_for_unknown_windows_the_drawer_and_after_shutdown() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let tmp = seed_three_markdown_files();

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    wait_for_attach(&handle);
    let win = wait_for_active_win(&handle);
    send_ex_command(&handle, ":e a.md");
    wait_for_current_file(&handle, "a.md");
    wait_for_entries(&handle, win, "an a.md row on top", |rows| {
        !rows.is_empty() && rows[0].display() == "a.md"
    });

    // --- 1. An UNKNOWN window handle is neutral -----------------------------
    assert!(
        !handle.history_can_back(UNKNOWN_WIN),
        "an unknown window can never go back"
    );
    assert!(
        !handle.history_can_forward(UNKNOWN_WIN),
        "an unknown window can never go forward"
    );
    assert!(
        !handle.history_back(UNKNOWN_WIN),
        "history_back on an unknown window is a false no-op"
    );
    assert!(
        !handle.history_forward(UNKNOWN_WIN),
        "history_forward on an unknown window is a false no-op"
    );
    assert!(
        !handle.history_jump(UNKNOWN_WIN, 0),
        "history_jump on an unknown window is a false no-op"
    );
    assert!(
        handle.history_entries(UNKNOWN_WIN).is_empty(),
        "an unknown window has no rows"
    );

    // --- 2. The terminal DRAWER window has NO history -----------------------
    // (Locked: the drawer is EXCLUDED. `toggle_terminal` ends in `startinsert`, so
    // no further Ex commands are sent after this point.)
    let before: HashSet<i64> = handle.windows().iter().map(|w| w.win()).collect();
    handle.toggle_terminal();
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut drawer_win = 0_i64;
    while Instant::now() < deadline {
        if let Some(w) = handle
            .windows()
            .iter()
            .map(|w| w.win())
            .find(|w| *w != 0 && !before.contains(w))
        {
            drawer_win = w;
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        drawer_win != 0,
        "the terminal drawer must appear as a new window"
    );
    sleep(HISTORY_SETTLE);
    assert!(
        handle.history_entries(drawer_win).is_empty(),
        "the terminal DRAWER is EXCLUDED from history entirely, got {:?}",
        displays(&handle.history_entries(drawer_win))
    );
    assert!(
        !handle.history_can_back(drawer_win),
        "the drawer pane can never go back"
    );
    assert!(
        !handle.history_can_forward(drawer_win),
        "the drawer pane can never go forward"
    );

    // --- 3. A SHUT-DOWN core yields the neutral values ----------------------
    handle.shutdown();
    assert!(
        !handle.history_can_back(win),
        "after shutdown history_can_back() must be the neutral false"
    );
    assert!(
        !handle.history_can_forward(win),
        "after shutdown history_can_forward() must be the neutral false"
    );
    assert!(
        !handle.history_back(win),
        "after shutdown history_back() must be the neutral false"
    );
    assert!(
        !handle.history_forward(win),
        "after shutdown history_forward() must be the neutral false"
    );
    assert!(
        !handle.history_jump(win, 0),
        "after shutdown history_jump() must be the neutral false"
    );
    assert!(
        handle.history_entries(win).is_empty(),
        "after shutdown history_entries() must be the neutral empty vec"
    );

    drop(tmp);
}
