//! Integration tests for the S2 Neovim->shell INTENT CHANNEL plus the init.lua
//! command/chord overrides and active-window tracking. These exercise CORE ONLY:
//! the front-end does not yet consume intents, so nothing in these tests "acts"
//! on a `FocusDir` intent — which is exactly what makes the "focus is pure"
//! assertion (test 4) load-bearing.
//!
//! All tests drive a REAL, hermetic nvim (no mocks) via `WOLF_NVIM_BIN`; the
//! common helpers panic loudly if it is unset.
//!
//! Testing conventions (mirroring `redraw_multigrid.rs` / `current_file.rs`):
//!   * We NEVER assert literal `WinInfo.win` / `active_win` handle values. We use
//!     relative / membership assertions (a handle round-trips through
//!     `grid_for_win`; `active_win` equals a window present in `windows`; a focus
//!     target is "the window whose handle differs from the current `active_win`").
//!   * We never assume HashMap iteration order.
//!   * Intents arrive out-of-band (a `wolf_layout` rpcnotify), and structural
//!     snapshot changes (`win_pos` / grid creation / `win_close` / `wolf_active_win`)
//!     may lag and arrive across several flushes, so we POLL with a generous
//!     timeout, draining wakes / draining the intent queue each iteration, rather
//!     than waiting for a single flush.
//!
//! NOTE to coder: these tests assume `LayoutIntent` derives `Debug` (used in
//! panic messages) and that `SplitOrientation` / `FocusDir` derive `Debug + Copy`
//! (`expect_split` returns a copied `SplitOrientation`, and both are formatted in
//! panic messages). `PartialEq` is not required by this file.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{ch, ctrl, key, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{
    FocusDir, InputEvent, Key, LayoutIntent, Snapshot, SplitOrientation, WolfCore,
};

/// The outer/global grid id. Window content lives on grids `>= 2`; grid 1 is
/// never a window grid in these scenarios.
const GLOBAL_GRID: u64 = 1;

/// Gap between polls while waiting for a structural change or an intent.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long to let a (non-)event settle before asserting that something did NOT
/// change — long enough that a stray `wolf_active_win` would have arrived if it
/// were going to. Generous on purpose so the "focus is pure" negative assertion
/// cannot pass by mere luck of timing.
const SETTLE: Duration = Duration::from_secs(1);

/// Placeholder content for temp markdown files.
const DOC: &str = "# Layout Intents\nbody\n";

/// Polls `core`'s snapshot until `pred` holds, draining wakes each iteration, or
/// panics after [`FLUSH_TIMEOUT`]. `desc` describes the awaited condition.
fn poll_until(
    core: &WolfCore,
    waiter: &FlushWaiter,
    desc: &str,
    pred: impl Fn(&Snapshot) -> bool,
) {
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

/// Polls `core.drain_intents()` until it yields a non-empty batch (or panics
/// after `timeout`), returning the first non-empty drain. Draining is what wakes
/// the queue side, so we simply call it on a fixed interval. `desc` describes the
/// awaited intent for the panic message.
///
/// This is the plan's `drain_until_intent` helper, generalized with a `desc` for
/// clearer failures. Each <C-w> chord / ex-command under test enqueues exactly
/// one `LayoutIntent`, so the first non-empty drain is the batch we want; callers
/// then assert on the specific variant inside it.
fn drain_until_intent(core: &WolfCore, timeout: Duration, desc: &str) -> Vec<LayoutIntent> {
    let deadline = Instant::now() + timeout;
    loop {
        let intents = core.drain_intents();
        if !intents.is_empty() {
            return intents;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {timeout:?} waiting for intent: {desc}");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Sends a `<C-w>` chord followed by a single trailing key `c`. The harness sends
/// the ctrl-chord and the plain key as two separate input events (they coalesce
/// into one `nvim_input` in the pump), matching how `redraw_multigrid.rs` drives
/// `<C-w>w`.
fn ctrl_w_then(core: &WolfCore, c: char) {
    core.send_input(ctrl(Key::Char('w')));
    core.send_input(ch(c));
}

/// Types an Ex command `:<cmd><CR>` one key at a time (the `cnoreabbrev` path).
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    for c in cmd.chars() {
        core.send_input(ch(c));
    }
    core.send_input(key(Key::Enter));
}

/// Waits until a single editor window grid (`>= 2`) exists and returns the
/// initial window count (the per-test baseline).
fn wait_for_initial_window(core: &WolfCore, waiter: &FlushWaiter) -> usize {
    poll_until(
        core,
        waiter,
        "the initial editor window grid (id != 1) to appear",
        |snap| !snap.windows.is_empty() && snap.grids.keys().any(|&id| id != GLOBAL_GRID),
    );
    core.read_snapshot().windows.len()
}

/// Returns the first `Split` intent in `intents`, or panics.
fn expect_split(intents: &[LayoutIntent], desc: &str) -> (SplitOrientation, i64, i64) {
    for i in intents {
        if let LayoutIntent::Split {
            orientation,
            from,
            win,
        } = i
        {
            return (*orientation, *from, *win);
        }
    }
    panic!("expected a Split intent ({desc}); got {intents:?}");
}

/// Convenience: does `snap.current_file` canonical-equal `expected`?
fn current_file_is(snap: &Snapshot, expected: &Path) -> bool {
    let want = std::fs::canonicalize(expected).ok();
    match (&snap.current_file, want) {
        (Some(have), Some(want)) => have == &want,
        _ => false,
    }
}

/// `<C-w>s` performs a real horizontal (stacked) split AND emits a
/// `Split { orientation: Horizontal, .. }` intent. The split must actually grow
/// the window count, and the new window handle must round-trip through
/// `grid_for_win`.
#[test]
fn ctrl_w_s_emits_horizontal_split() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    let base = wait_for_initial_window(&core, &waiter);
    // Discard any intents emitted during setup so we observe only the chord's.
    let _ = core.drain_intents();

    ctrl_w_then(&core, 's');

    let intents = drain_until_intent(&core, FLUSH_TIMEOUT, "Split from <C-w>s");
    let (orientation, from, win) = expect_split(&intents, "<C-w>s");
    assert!(
        matches!(orientation, SplitOrientation::Horizontal),
        "<C-w>s must map to a HORIZONTAL (stacked) split; got {orientation:?}"
    );
    assert_ne!(win, 0, "the split's new window handle should be a real (nonzero) i64");
    assert_ne!(
        from, win,
        "the split's `from` (origin window) and `win` (new window) must differ"
    );

    // The split was PERFORMED: a second window now exists...
    poll_until(
        &core,
        &waiter,
        "windows.len() to grow by one after <C-w>s",
        |snap| snap.windows.len() == base + 1,
    );
    // ...and the intent's new window handle is bound to a grid we know about.
    poll_until(
        &core,
        &waiter,
        "the split's new window handle to appear via grid_for_win",
        |snap| snap.grid_for_win(win).is_some(),
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `<C-w>v` performs a real vertical (side-by-side) split AND emits a
/// `Split { orientation: Vertical, .. }` intent.
#[test]
fn ctrl_w_v_emits_vertical_split() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    let base = wait_for_initial_window(&core, &waiter);
    let _ = core.drain_intents();

    ctrl_w_then(&core, 'v');

    let intents = drain_until_intent(&core, FLUSH_TIMEOUT, "Split from <C-w>v");
    let (orientation, _from, win) = expect_split(&intents, "<C-w>v");
    assert!(
        matches!(orientation, SplitOrientation::Vertical),
        "<C-w>v must map to a VERTICAL (side-by-side) split; got {orientation:?}"
    );

    poll_until(
        &core,
        &waiter,
        "windows.len() to grow by one after <C-w>v",
        |snap| snap.windows.len() == base + 1,
    );
    poll_until(
        &core,
        &waiter,
        "the split's new window handle to appear via grid_for_win",
        |snap| snap.grid_for_win(win).is_some(),
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Typing `:vsplit<CR>` (the `cnoreabbrev` path) emits a vertical split intent
/// and performs the split.
///
/// NOTE to coder: this exercises the EX-COMMAND (cabbrev) override, which is the
/// FRAGILE path — `cnoreabbrev` only expands a whole word at the command line and
/// is guarded by `getcmdtype()==':' && getcmdline()==word`. If this proves flaky
/// in practice, the robust coverage is the `<C-w>v` chord test above; do NOT
/// weaken that one to compensate. We type the full word `vsplit` (not the `vs`
/// alias) to maximize the chance the abbrev fires.
#[test]
fn ex_vsplit_emits_vertical_split() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    let base = wait_for_initial_window(&core, &waiter);
    let _ = core.drain_intents();

    ex_command(&core, "vsplit");

    let intents = drain_until_intent(&core, FLUSH_TIMEOUT, "Split from :vsplit");
    let (orientation, _from, _win) = expect_split(&intents, ":vsplit");
    assert!(
        matches!(orientation, SplitOrientation::Vertical),
        ":vsplit must map to a VERTICAL split; got {orientation:?}"
    );

    poll_until(
        &core,
        &waiter,
        "windows.len() to grow by one after :vsplit",
        |snap| snap.windows.len() == base + 1,
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `<C-w>l` emits a `FocusDir { dir: Right, .. }` intent but, because focus moves
/// are PURE intents (the SHELL performs the focus change, and in this core-only
/// test nothing acts on the intent), the nvim focus — and hence `active_win` —
/// does NOT change. This is the load-bearing "focus is pure" assertion.
#[test]
fn ctrl_w_l_emits_focus_dir_and_does_not_change_nvim_focus() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    let base = wait_for_initial_window(&core, &waiter);
    let _ = core.drain_intents();

    // Make two side-by-side windows so a "focus right" is meaningful, then drain
    // (and discard) that split intent.
    ctrl_w_then(&core, 'v');
    let _ = drain_until_intent(&core, FLUSH_TIMEOUT, "Split from <C-w>v (setup)");
    poll_until(
        &core,
        &waiter,
        "two windows after the setup <C-w>v",
        |snap| snap.windows.len() == base + 1,
    );

    // Capture the active window AFTER the split settles (call it A). The vsplit
    // moved nvim focus to the new window, which fires WinEnter -> wolf_active_win.
    poll_until(
        &core,
        &waiter,
        "active_win to be populated after the split",
        |snap| snap.active_win.is_some(),
    );
    let win_a = core
        .read_snapshot()
        .active_win
        .expect("active_win should be Some after a split");

    // Now the focus chord: a PURE intent. It must NOT run the real <C-w>l.
    ctrl_w_then(&core, 'l');

    let intents = drain_until_intent(&core, FLUSH_TIMEOUT, "FocusDir from <C-w>l");
    assert!(
        intents
            .iter()
            .any(|i| matches!(i, LayoutIntent::FocusDir { dir: FocusDir::Right, .. })),
        "<C-w>l must emit a FocusDir {{ dir: Right, .. }} intent; got {intents:?}"
    );

    // Let any (erroneous) focus change settle, then assert focus did NOT move:
    // active_win is still A because no shell acted on the pure intent.
    let settle_deadline = Instant::now() + SETTLE;
    while Instant::now() < settle_deadline {
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
    let win_after = core.read_snapshot().active_win;
    assert_eq!(
        win_after,
        Some(win_a),
        "a <C-w>l focus move is a PURE intent: with no shell acting on it, nvim focus \
         (active_win) must stay put"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `focus_win(other)` changes `active_win` to the requested window, and
/// `current_file` follows to that window's buffer.
#[test]
fn focus_win_changes_active_and_current_file() {
    let (core, waiter, file_a) = common::start_core_with_temp_md(DOC);
    let base = wait_for_initial_window(&core, &waiter);
    let _ = core.drain_intents();

    // Two windows, focus now on the NEW window (both showing file A for the moment).
    ctrl_w_then(&core, 'v');
    let _ = drain_until_intent(&core, FLUSH_TIMEOUT, "Split from <C-w>v (setup)");
    poll_until(
        &core,
        &waiter,
        "two windows after the setup <C-w>v",
        |snap| snap.windows.len() == base + 1,
    );

    // Open a DIFFERENT file in the focused (new) window so the two panes show
    // distinct buffers; then current_file should track file B.
    let file_b = common::temp_md(DOC);
    core.send_input(InputEvent::Open {
        path: file_b.path().to_path_buf(),
    });
    poll_until(
        &core,
        &waiter,
        "active_win populated and current_file == file B after :edit in the new window",
        |snap| snap.active_win.is_some() && current_file_is(snap, file_b.path()),
    );

    // The currently-active window (showing B); the focus TARGET is the OTHER
    // window (showing A): the one whose handle differs from active_win. We pick
    // it by membership, never by literal value.
    let (active_b, target) = {
        let snap = core.read_snapshot();
        let active = snap.active_win.expect("active_win should be Some");
        let target = snap
            .windows
            .values()
            .map(|w| w.win)
            .find(|&w| w != 0 && w != active)
            .expect("a second, distinct window handle should exist after the split");
        (active, target)
    };
    assert_ne!(target, active_b, "the focus target must differ from the active window");

    // Drive focus from the shell side.
    core.focus_win(target);

    // active_win flips to the requested window...
    poll_until(
        &core,
        &waiter,
        "active_win to become the focus_win target",
        |snap| snap.active_win == Some(target),
    );
    // ...and current_file follows to the target window's buffer (file A).
    poll_until(
        &core,
        &waiter,
        "current_file to follow focus back to file A",
        |snap| current_file_is(snap, file_a.path()),
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `<C-w>c` with multiple windows performs a guarded `nvim_win_close`, so the
/// window count drops back by one (observed via the `win_close` redraw event).
#[test]
fn ctrl_w_c_closes_pane_when_multiple() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    let base = wait_for_initial_window(&core, &waiter);
    let _ = core.drain_intents();

    // Make two windows to have one to close.
    ctrl_w_then(&core, 'v');
    let _ = drain_until_intent(&core, FLUSH_TIMEOUT, "Split from <C-w>v (setup)");
    poll_until(
        &core,
        &waiter,
        "two windows after the setup <C-w>v",
        |snap| snap.windows.len() == base + 1,
    );

    // Close the focused window.
    ctrl_w_then(&core, 'c');
    poll_until(
        &core,
        &waiter,
        "windows.len() to drop back by one after <C-w>c",
        |snap| snap.windows.len() == base,
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Typing `:q<CR>` (the `cnoreabbrev` -> `WolfQuit` path) with multiple windows
/// performs a guarded `nvim_win_close`, so the window count drops back by one —
/// exactly like `<C-w>c`. The buffer is unmodified, so `force = false` closes the
/// active pane cleanly. Mirrors `ctrl_w_c_closes_pane_when_multiple`.
#[test]
fn ex_q_closes_pane_when_multiple() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    let base = wait_for_initial_window(&core, &waiter);
    let _ = core.drain_intents();

    // Make two windows to have one to close.
    ctrl_w_then(&core, 'v');
    let _ = drain_until_intent(&core, FLUSH_TIMEOUT, "Split from <C-w>v (setup)");
    poll_until(
        &core,
        &waiter,
        "two windows after the setup <C-w>v",
        |snap| snap.windows.len() == base + 1,
    );

    // Close the focused pane via `:q` (routes through WolfQuit -> do_quit).
    ex_command(&core, "q");
    poll_until(
        &core,
        &waiter,
        "windows.len() to drop back by one after :q",
        |snap| snap.windows.len() == base,
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Typing `:qa<CR>` (the `cnoreabbrev` -> `WolfQuitAll` path) with multiple
/// windows must NOT merely drop the pane count — unlike `:q`, which closes just the
/// active pane (see `ex_q_closes_pane_when_multiple` above). `:qa` always takes the
/// close-window branch, firing `close_window` for the front-end to close the WHOLE
/// window, and in this core-only test nothing acts on that command, so the nvim
/// pane count stays put. This is the NEGATIVE half of the differentiator; the
/// POSITIVE close_window proof lives in
/// `commands.rs::ex_qa_emits_close_window_when_multiple_panes`. Mirrors
/// `ex_q_closes_pane_when_multiple`'s setup and the SETTLE style of
/// `ctrl_w_c_noop_on_last_window`.
#[test]
fn ex_qa_does_not_merely_drop_pane_count() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    let base = wait_for_initial_window(&core, &waiter);
    let _ = core.drain_intents();

    // Make two windows so this is genuinely the multi-pane case.
    ctrl_w_then(&core, 'v');
    let _ = drain_until_intent(&core, FLUSH_TIMEOUT, "Split from <C-w>v (setup)");
    poll_until(
        &core,
        &waiter,
        "two windows after the setup <C-w>v",
        |snap| snap.windows.len() == base + 1,
    );

    // `:qa` routes through WolfQuitAll -> do_quit({ all = true }) -> always-close
    // branch. It fires `close_window` (asserted in commands.rs), NOT a pane close,
    // so the nvim pane count must NOT drop back to `base`.
    ex_command(&core, "qa");

    // Let any (erroneous) pane close settle, draining wakes on the interval like the
    // negative tests do, then assert the pane count did NOT drop to `base`.
    let settle_deadline = Instant::now() + SETTLE;
    while Instant::now() < settle_deadline {
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
    assert_ne!(
        core.read_snapshot().windows.len(),
        base,
        "with multiple panes, :qa must NOT merely drop the pane count (it closes the whole \
         window via close_window instead); the pane count should stay above the single-pane base"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `<C-w>c` on the LAST window is a no-op (the Lua guard refuses to close the
/// final window), so the window count is unchanged and nvim stays alive (a
/// subsequent edit still produces a flush).
#[test]
fn ctrl_w_c_noop_on_last_window() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    let base = wait_for_initial_window(&core, &waiter);
    // NOTE to coder: a freshly opened single file is exactly one editor window,
    // so `base == 1` here; the guard must refuse to close it.
    assert_eq!(base, 1, "expected exactly one window before the last-window close test");
    let _ = core.drain_intents();

    // Attempt to close the only window.
    ctrl_w_then(&core, 'c');

    // Let any (erroneous) close settle, then assert nothing was removed.
    let settle_deadline = Instant::now() + SETTLE;
    while Instant::now() < settle_deadline {
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
    assert_eq!(
        core.read_snapshot().windows.len(),
        base,
        "the last-window guard must refuse to close the only window"
    );

    // nvim is still alive: a subsequent edit produces a redraw flush.
    core.send_input(ch('i'));
    core.send_input(ch('x'));
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    core.shutdown().expect("shutdown should be Ok");
}

/// A `WinEnter` (here triggered by a split and then by `focus_win`) updates
/// `active_win`, and the value always names a window present in `windows`.
#[test]
fn win_enter_updates_active_win() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    let base = wait_for_initial_window(&core, &waiter);
    let _ = core.drain_intents();

    // Split: WinEnter on the new window updates active_win.
    ctrl_w_then(&core, 'v');
    let _ = drain_until_intent(&core, FLUSH_TIMEOUT, "Split from <C-w>v (setup)");
    poll_until(
        &core,
        &waiter,
        "two windows after the setup <C-w>v",
        |snap| snap.windows.len() == base + 1,
    );
    poll_until(
        &core,
        &waiter,
        "active_win to be populated and present in windows after the split",
        |snap| {
            snap.active_win
                .is_some_and(|a| snap.windows.values().any(|w| w.win == a))
        },
    );

    // Focus the other window; active_win updates to it (also a WinEnter).
    let (active, target) = {
        let snap = core.read_snapshot();
        let active = snap.active_win.expect("active_win should be Some");
        let target = snap
            .windows
            .values()
            .map(|w| w.win)
            .find(|&w| w != 0 && w != active)
            .expect("a second, distinct window handle should exist after the split");
        (active, target)
    };
    assert_ne!(target, active, "the focus target must differ from the active window");

    core.focus_win(target);
    poll_until(
        &core,
        &waiter,
        "active_win to update to the focused window via WinEnter",
        |snap| {
            snap.active_win == Some(target)
                && snap.windows.values().any(|w| w.win == target)
        },
    );

    core.shutdown().expect("shutdown should be Ok");
}
