//! Integration test for the `wolf-ffi` TERMINAL-DRAWER bridge surface (plan §8.3):
//! the native-facing `WolfCoreHandle::toggle_terminal()` action that backs the
//! macOS "Terminal" command (`:WolfTerminal`, `` ⌃` ``).
//!
//!   * `WolfCoreHandle::toggle_terminal(&self)` -> `InputEvent::ToggleTerminal` ->
//!     the `wolf_toggle_terminal()` Lua helper in `init.lua`, which opens / hides a
//!     bottom `:terminal` drawer WINDOW (per-tabpage) showing a single shared,
//!     tagged terminal BUFFER (per-project), recreating it if the user exited the
//!     shell.
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is the sibling of `file_actions.rs` /
//! `buffer_picker.rs` (same staging / SKIP-when-unset machinery, reusing
//! `tests/common/mod.rs`, and the same "poll `read_frame()` until attached"
//! pattern).
//!
//! ## Dependency on coder work (plan §3–§5)
//!
//! `toggle_terminal` does NOT exist on `WolfCoreHandle` yet: the coder adds it to
//! the bridge (`extern "Rust"`) and `impl WolfCoreHandle` in
//! `crates/wolf-ffi/src/lib.rs`, backed by the new `wolf_core::InputEvent::ToggleTerminal`
//! variant and the `wolf_toggle_terminal()` helper in `runtime/nvim/init.lua`. This
//! test file does NOT touch non-test source or BUILD files, and it will not compile
//! until that method lands — that is expected.
//!
//! We drive `toggle_terminal()` (the HANDLE method that actually runs the helper)
//! rather than `:WolfTerminal` — the Ex-command would only enqueue an `FfiCommand`
//! with no front-end dispatcher present to call the method back.
//!
//! ## Drawer-open layout intent (fix cycle 2/7)
//!
//! The open path additionally emits a `LayoutIntent::DrawerOpen { win }` (surfaced as
//! `FfiLayoutIntent { kind: intent_kind::DRAWER_OPEN = 2, win }`) so the front-end can
//! render the drawer as a root-bottom pane. Because nothing drains intents here they
//! accumulate; step 2b polls `drain_intents()` for `kind() == 2` with `win() != 0`.
//! The `intent_kind::DRAWER_OPEN` const, the `from_intent` arm, and `init.lua`'s
//! `wolf_emit_drawer` do NOT exist yet - the coder lands them (by design).
//!
//! ## Terminal detection (OD7)
//!
//! Terminal buffers are `buflisted`, so they surface in the buffer-picker snapshot;
//! their `display()` label is the verbatim `term://…` buffer name (the picker's
//! cwd-relative projection leaves a `term://` name untouched). We detect the drawer
//! buffer by that `term://` prefix and observe the drawer WINDOW open/hide via
//! `windows().len()`.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim (`WOLF_NVIM_BIN` or the
//! Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available, via
//! `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.
//!
//! ## SINGLE #[test]
//!
//! The whole toggle lifecycle lives in ONE `#[test]` because it walks a stateful
//! sequence against a single embedded Neovim (open, hide, reopen, wipe, recreate).

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiBufferRow, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each async effect (the
/// `toggle_terminal` notification round-trip, the drawer split, the terminal job
/// spawn) to settle. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A `max` large enough that the assembled buffer set is never truncated (a handful
/// of buffers stays well under this). Mirrors `buffer_picker.rs`'s `RANKED_MAX`.
const RANKED_MAX: usize = 100;

/// The Neovim naming convention prefix for a terminal buffer's name (`term://…`),
/// used to detect the drawer buffer in the picker snapshot (OD7).
const TERM_PREFIX: &str = "term://";

/// `FfiLayoutIntent::kind()` for a DRAWER_OPEN intent — the append-only third
/// layout-intent discriminant (mirrors `wolf-ffi/src/lib.rs` `mod intent_kind`,
/// after `SPLIT = 0` / `FOCUS_DIR = 1`; fix plan §2 / FD5). The `wolf_toggle_terminal`
/// open path emits this so the front-end renders the drawer as a root-bottom pane.
///
/// NOTE (coder): `intent_kind::DRAWER_OPEN = 2` + the `FfiLayoutIntent::from_intent`
/// arm + `init.lua`'s `wolf_emit_drawer` do NOT exist yet — this assertion only
/// passes once they land (by design).
const INTENT_KIND_DRAWER_OPEN: u8 = 2;

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false (the already-resolved glyph rides in `ch`). Same as `commands.rs`.
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

/// Builds a CTRL-modified character `FfiKeyEvent` (kind=Char, `ctrl = true`) for
/// `c` — e.g. `ctrl_char_key('\\')` + `ctrl_char_key('n')` sends `<C-\><C-n>` to
/// leave terminal-insert mode.
fn ctrl_char_key(c: char) -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_CHAR,
        ch: c as u32,
        fnum: 0,
        ctrl: true,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Builds an Enter `FfiKeyEvent` (kind=Enter), all modifiers false. Same as
/// `commands.rs`.
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
/// the leading `:`. Copy of `commands.rs`'s `send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// The bufnrs of all ranked buffer rows whose `display()` names a terminal buffer
/// (the `term://` prefix, OD7), from a FRESH picker snapshot (the picker snapshots
/// `list_buffers()` at each `buffer_picker()` call).
fn term_bufnrs(handle: &wolf_ffi::WolfCoreHandle) -> Vec<i64> {
    handle
        .buffer_picker()
        .ranked(String::new(), RANKED_MAX)
        .iter()
        .filter(|r| r.display().starts_with(TERM_PREFIX))
        .map(FfiBufferRow::bufnr)
        .collect()
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until `handle.windows().len()` equals
/// `target`, then asserts it. Observes the drawer WINDOW opening / hiding.
fn wait_for_window_count(handle: &wolf_ffi::WolfCoreHandle, target: usize, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut count = handle.windows().len();
    while Instant::now() < deadline {
        count = handle.windows().len();
        if count == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(count, target, "window count should be {target} ({desc})");
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until `predicate` holds on a FRESH set of
/// `term://` bufnrs, returning that set. Panics with `desc` on timeout.
fn wait_for_term_bufnrs(
    handle: &wolf_ffi::WolfCoreHandle,
    desc: &str,
    predicate: impl Fn(&[i64]) -> bool,
) -> Vec<i64> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let bufnrs = term_bufnrs(handle);
        if predicate(&bufnrs) {
            return bufnrs;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for term:// rows \
                 ({desc}); saw {bufnrs:?}"
            );
        }
        sleep(POLL_INTERVAL);
    }
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until `handle.full_width_for_win(win)`
/// equals `target`, then asserts it. Copy of `full_width.rs`'s per-window poll
/// helper — observes the drawer window's full-width state.
fn wait_for_win_full_width(handle: &wolf_ffi::WolfCoreHandle, win: i64, target: bool, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut got = handle.full_width_for_win(win);
    while Instant::now() < deadline {
        got = handle.full_width_for_win(win);
        if got == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        got, target,
        "full_width_for_win({win}) should be {target} ({desc})"
    );
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until `handle.is_full_width_active()`
/// equals `target`, then asserts it. Copy of `full_width.rs`'s active-buffer poll
/// helper — observes the focused (terminal) buffer's full-width flag.
fn wait_for_full_width_active(handle: &wolf_ffi::WolfCoreHandle, target: bool, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut got = handle.is_full_width_active();
    while Instant::now() < deadline {
        got = handle.is_full_width_active();
        if got == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        got, target,
        "is_full_width_active() should be {target} ({desc})"
    );
}

#[test]
fn terminal_toggle_persists_and_recreates() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd (its `<cwd>/.wolf/` MRU cache must outlive the session), kept
    // alive until after `shutdown()`, then dropped explicitly at the end.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Poll read_frame() until attached (cols > 0); record base window count.
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
    let base = handle.windows().len();

    // --- 2. Open: toggle_terminal() opens the drawer window + spawns a terminal --
    // The drawer window appears (base + 1), and exactly one term:// buffer exists.
    handle.toggle_terminal();
    wait_for_window_count(&handle, base + 1, "drawer opened");
    let opened = wait_for_term_bufnrs(&handle, "exactly one term:// buffer after open", |b| {
        b.len() == 1
    });
    let first = opened[0];
    assert_ne!(first, 0, "the opened terminal buffer must have a nonzero bufnr");

    // --- 2b. Open emits a DRAWER_OPEN layout intent naming the drawer window -----
    // The Lua open path calls `wolf_emit_drawer(win)` -> `rpcnotify(chan,
    // "wolf_layout", {action="drawer_open", win})` -> `LayoutIntent::DrawerOpen` ->
    // `FfiLayoutIntent { kind: DRAWER_OPEN, win }`. NO front-end drains intents in
    // this headless test, so they ACCUMULATE across `drain_intents()` polls and are
    // observable (mirrors `panes.rs`'s SPLIT-intent accumulation). `drain_intents()`
    // DRAINS, so keep the last DRAWER_OPEN win we see across iterations.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut drawer_win: Option<i64> = None;
    while Instant::now() < deadline {
        for intent in handle.drain_intents() {
            if intent.kind() == INTENT_KIND_DRAWER_OPEN {
                drawer_win = Some(intent.win());
            }
        }
        if drawer_win.is_some() {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    let drawer_win =
        drawer_win.expect("open should emit a DRAWER_OPEN layout intent within the timeout");
    assert_ne!(
        drawer_win, 0,
        "the DRAWER_OPEN intent's win() should be a nonzero drawer window handle"
    );

    // --- 3. Hide (persist): toggle_terminal() closes the window, buffer survives --
    // The hide path (`nvim_win_close(w, false)`) emits NO layout intent — the
    // front-end removes the drawer pane by observing the `win_close` redraw event
    // (the same close-observation path editor panes use), so there is nothing to
    // drain here.
    // The window count returns to base, but the tagged terminal buffer (bufnr ==
    // first) is STILL listed — closing the window keeps the buffer + job alive.
    handle.toggle_terminal();
    wait_for_window_count(&handle, base, "drawer hidden");
    wait_for_term_bufnrs(&handle, "terminal buffer persists after hide", |b| {
        b.contains(&first)
    });

    // --- 4. Reopen (reuse): toggle_terminal() re-shows the SAME buffer -----------
    // The drawer window reappears (base + 1) and the term:// row's bufnr is still
    // `first` (reused, not recreated).
    handle.toggle_terminal();
    wait_for_window_count(&handle, base + 1, "drawer reopened");
    let reopened = wait_for_term_bufnrs(&handle, "reused terminal buffer after reopen", |b| {
        b.contains(&first)
    });
    assert_eq!(
        reopened,
        vec![first],
        "reopening must reuse the single shared terminal buffer (bufnr {first})"
    );

    // --- 5. Recreate-if-gone: wipe the terminal, then toggle spawns a FRESH one --
    // `<C-\><C-n>` leaves terminal-insert mode; `:bwipeout!` wipes the focused
    // terminal buffer (the drawer is currently open + focused). No term:// row then
    // remains. Toggling again finds no terminal buffer and creates a fresh, tagged
    // one whose bufnr differs from the wiped `first`.
    handle.send_key(ctrl_char_key('\\'));
    handle.send_key(ctrl_char_key('n'));
    send_ex_command(&handle, ":bwipeout!");
    wait_for_term_bufnrs(&handle, "terminal buffer gone after :bwipeout!", |b| {
        b.is_empty()
    });

    handle.toggle_terminal();
    let recreated = wait_for_term_bufnrs(&handle, "fresh terminal buffer after recreate", |b| {
        !b.is_empty()
    });
    assert!(
        recreated.iter().all(|&b| b != first),
        "recreating after a wipe must spawn a NEW terminal buffer (got {recreated:?}, \
         wiped {first})"
    );

    // --- 6. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}

/// The Terminal drawer defaults to FULL WIDTH — the drawer pane resolves its
/// width through the SAME per-window `full_width_for_win` channel an editor pane
/// uses, so defaulting the terminal buffer's `b:wolf_full_width = true` (in
/// `wolf_toggle_terminal()`) makes the drawer render full-width with no Swift
/// change. A normal editor buffer must be unaffected (still the ~90-char default).
///
/// This reuses `terminal.rs`'s open path (`toggle_terminal()` + the DRAWER_OPEN
/// layout intent that names the drawer WINDOW handle) and `full_width.rs`'s poll
/// helpers.
///
/// NOTE (test-first): the `init.lua` change that sets `b:wolf_full_width = true`
/// on the terminal buffer + re-emits `wolf_full_width` is not written yet — the
/// coder lands it next, so this test fails until then (by design).
#[test]
fn terminal_drawer_defaults_to_full_width() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd (its `<cwd>/.wolf/` MRU cache must outlive the session), kept
    // alive until after `shutdown()`, then dropped explicitly at the end.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Poll read_frame() until attached (cols > 0); record base window count.
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

    // --- 2. Baseline: a normal editor buffer is NOT full width by default -----
    // The terminal's full-width default must NOT change the editor default cap.
    assert!(
        !handle.is_full_width_active(),
        "a fresh editor buffer must default to the ~90-char content width \
         (is_full_width_active == false)"
    );
    let editor_win = handle.active_win();
    assert_ne!(
        editor_win, 0,
        "attach should report a nonzero active editor window handle"
    );
    assert!(
        !handle.full_width_for_win(editor_win),
        "the active editor window must default to the ~90-char cap \
         (full_width_for_win == false)"
    );

    let base = handle.windows().len();

    // --- 3. Open the terminal drawer via the same path terminal.rs uses -------
    // `toggle_terminal()` opens the drawer window (base + 1) and spawns exactly one
    // term:// buffer.
    handle.toggle_terminal();
    wait_for_window_count(&handle, base + 1, "drawer opened");
    let opened = wait_for_term_bufnrs(&handle, "exactly one term:// buffer after open", |b| {
        b.len() == 1
    });
    assert_ne!(
        opened[0], 0,
        "the opened terminal buffer must have a nonzero bufnr"
    );

    // --- 3b. Resolve the drawer WINDOW handle from the DRAWER_OPEN intent ------
    // The open path emits `LayoutIntent::DrawerOpen { win }` surfaced as an
    // `FfiLayoutIntent { kind: DRAWER_OPEN, win }`. Nothing drains intents here so
    // they accumulate; `drain_intents()` DRAINS, so keep the last DRAWER_OPEN win.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut drawer_win: Option<i64> = None;
    while Instant::now() < deadline {
        for intent in handle.drain_intents() {
            if intent.kind() == INTENT_KIND_DRAWER_OPEN {
                drawer_win = Some(intent.win());
            }
        }
        if drawer_win.is_some() {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    let drawer_win =
        drawer_win.expect("open should emit a DRAWER_OPEN layout intent within the timeout");
    assert_ne!(
        drawer_win, 0,
        "the DRAWER_OPEN intent's win() should be a nonzero drawer window handle"
    );

    // --- 4. CORE ASSERTION: the drawer defaults to FULL WIDTH -----------------
    // The drawer window's per-window full-width flag must resolve true by default,
    // routing `maxCols: nil` (full) into the front-end pane geometry.
    wait_for_win_full_width(
        &handle,
        drawer_win,
        true,
        "the terminal drawer window defaults to full width",
    );

    // --- 5. The freshly opened drawer is focused, so the active buffer reads on
    // The open path leaves the terminal buffer focused, so the View-menu checkmark
    // (is_full_width_active) reads on for the terminal.
    wait_for_full_width_active(
        &handle,
        true,
        "the focused terminal buffer reports full width active",
    );

    // --- 6. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
