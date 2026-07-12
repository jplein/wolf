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

    // --- 3. Hide (persist): toggle_terminal() closes the window, buffer survives --
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
