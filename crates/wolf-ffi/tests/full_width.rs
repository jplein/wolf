//! Integration test for the `wolf-ffi` FULL-WIDTH (per-buffer content width)
//! bridge surface (feature plan §3–§7): the native-facing editor-affecting toggle
//! that backs the macOS "Use full width for content" command (`:WolfFullWidth`).
//!
//!   * `WolfCoreHandle::toggle_full_width(&self)` -> `InputEvent::ToggleFullWidth`
//!     -> the `wolf_toggle_full_width()` Lua helper in `init.lua`, which flips the
//!     effective buffer's `b:wolf_full_width` (targeting the SOURCE buffer when the
//!     active pane is a preview scratch — OQ-1) and re-emits the current-file /
//!     per-window full-width / previews notifications.
//!   * `WolfCoreHandle::is_full_width_active(&self) -> bool` — the active buffer's
//!     flag (drives the View-menu checkmark).
//!   * `WolfCoreHandle::full_width_for_win(&self, win: i64) -> bool` — the
//!     per-window map that threads `maxCols: nil` (full) vs `90` (default) into the
//!     front-end pane geometry.
//!   * `FfiPreview::source_full_width() -> bool` — the preview's SOURCE-buffer flag.
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is the sibling of `terminal.rs` (the other
//! editor-affecting toggle) and `preview.rs` (same staging / SKIP-when-unset
//! machinery, reusing `tests/common/mod.rs`).
//!
//! ## Dependency on coder work (plan §3–§7)
//!
//! None of `toggle_full_width` / `is_full_width_active` / `full_width_for_win` /
//! `FfiPreview::source_full_width` exists yet: the coder adds them to the bridge
//! (`extern "Rust"`) and `impl WolfCoreHandle` in `crates/wolf-ffi/src/lib.rs`,
//! backed by the new `wolf_core::InputEvent::ToggleFullWidth` variant, the
//! `Snapshot.current_is_full_width` / `full_width` state, the `wolf_full_width`
//! notification, and the `wolf_toggle_full_width()` / `wolf_setup_full_width()`
//! helpers in `runtime/nvim/init.lua`, then regenerates `generated/` and appends
//! `command_kind::FULL_WIDTH = 22`. This test file does NOT touch non-test source,
//! and it will not compile until that lands — expected.
//!
//! We drive `toggle_full_width()` (the HANDLE method that actually runs the helper)
//! rather than relying on `:WolfFullWidth` to toggle — the Ex-command only enqueues
//! an `FfiCommand` with no front-end dispatcher present to call the method back
//! (exactly as `terminal.rs` drives `toggle_terminal()` directly). The command
//! channel itself is proven separately below by draining `CMD_KIND_FULL_WIDTH`.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim (`WOLF_NVIM_BIN` or the
//! Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available, via
//! `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, FfiPreview};

/// Generous overall timeout for nvim `--embed` startup plus each async effect (the
/// `toggle_full_width` notification round-trip, the out-of-band `wolf_full_width` /
/// `wolf_current_file` / `wolf_preview` re-emits) to settle. Mirrors the sibling
/// tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A short settle between window-cycling `:wincmd w` attempts, long enough for the
/// `WinEnter`/`BufEnter` autocmd to re-emit `wolf_current_file`. Mirrors
/// `preview.rs`'s `CYCLE_SETTLE`.
const CYCLE_SETTLE: Duration = Duration::from_millis(150);

/// The two-pane window count reached after the split-right preview placement.
const TWO_PANES: usize = 2;

/// The lowest real WINDOW grid id (grid 1 is the outer/global grid; editor windows
/// are `>= 2` under `ext_multigrid`). Mirrors `preview.rs`.
const WINDOW_GRID_MIN: u64 = 2;

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

/// `FfiCommand::kind()` for Use Full Width — the appended discriminant 22 (mirrors
/// `mod command_kind` in `wolf-ffi/src/lib.rs` and `commands.rs`'s local const).
const CMD_KIND_FULL_WIDTH: u8 = 22;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false. Same as the sibling tests.
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
/// the leading `:`. Copy of the siblings' `send_ex_command`.
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

/// Polls `handle.drain_commands()` until an `FfiCommand` with the given `kind`
/// appears, or panics after [`FLUSH_TIMEOUT`]. Copy of `commands.rs`'s poll helper.
fn poll_for_command_kind(handle: &wolf_ffi::WolfCoreHandle, kind: u8, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        for cmd in handle.drain_commands() {
            if cmd.kind() == kind {
                return;
            }
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for command kind {kind} ({desc})");
}

/// Blocks until `handle.is_full_width_active()` equals `target`, then asserts it.
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

/// Blocks until `handle.full_width_for_win(win)` equals `target`, then asserts it.
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

/// Blocks until `handle.is_markdown_active()` equals `target`, then asserts it.
/// Copy of `preview.rs`'s helper.
fn wait_for_markdown_active(handle: &wolf_ffi::WolfCoreHandle, target: bool, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut got = handle.is_markdown_active();
    while Instant::now() < deadline {
        got = handle.is_markdown_active();
        if got == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        got, target,
        "is_markdown_active() should be {target} ({desc})"
    );
}

/// The live editor WINDOW grids (grids `>= WINDOW_GRID_MIN` that are not hidden).
/// Mirrors `preview.rs::live_window_count`.
fn live_window_count(handle: &wolf_ffi::WolfCoreHandle) -> usize {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .count()
}

/// Blocks until exactly `n` live editor window grids exist, then asserts it.
/// Mirrors `preview.rs`.
fn wait_for_window_count(handle: &wolf_ffi::WolfCoreHandle, n: usize, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut count = live_window_count(handle);
    while Instant::now() < deadline {
        count = live_window_count(handle);
        if count == n {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(count, n, "there should be {n} live window grids ({desc})");
}

/// Blocks until `predicate(&previews)` holds on a fresh `previews()` snapshot,
/// returning that snapshot. Panics with `desc` on timeout. Copy of `preview.rs`.
fn wait_for_previews(
    handle: &wolf_ffi::WolfCoreHandle,
    desc: &str,
    predicate: impl Fn(&[FfiPreview]) -> bool,
) -> Vec<FfiPreview> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let previews = handle.previews();
        if predicate(&previews) {
            return previews;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for previews() ({desc})");
}

/// Cycles editor windows with `:wincmd w` until the active window reports a
/// markdown buffer, so a subsequent toggle lands on the SOURCE pane rather than the
/// (non-markdown) preview scratch pane. Copy of `preview.rs`'s helper.
fn focus_markdown_source(handle: &wolf_ffi::WolfCoreHandle) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if handle.is_markdown_active() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out cycling windows to focus the markdown source pane"
        );
        send_ex_command(handle, ":wincmd w");
        sleep(CYCLE_SETTLE);
    }
}

#[test]
fn full_width_toggles_active_buffer_and_routes_the_command() {
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

    // --- 1. Attach ----------------------------------------------------------
    wait_for_attach(&handle);

    // --- 2. A fresh buffer defaults to the ~90-char width (not full width) ---
    assert!(
        !handle.is_full_width_active(),
        "a fresh buffer must default to the ~90-char content width (is_full_width_active == false)"
    );

    // --- 3. :WolfFullWidth<CR> routes to the command channel -----------------
    // Proves `:WolfFullWidth` is auto-registered as a COMMAND_SPECS row and drains
    // the appended discriminant 22. NOTHING acts on this drain (the front-end
    // dispatcher that would call `toggle_full_width()` back is absent), so it does
    // NOT itself flip the buffer's flag — mirrors `terminal.rs` driving the toggle
    // through the handle method rather than the Ex-command. Drain any startup noise
    // first so we observe THIS command.
    let _ = handle.drain_commands();
    send_ex_command(&handle, ":WolfFullWidth");
    poll_for_command_kind(&handle, CMD_KIND_FULL_WIDTH, ":WolfFullWidth");
    // The routed-but-undispatched command left the flag untouched.
    assert!(
        !handle.is_full_width_active(),
        "an undispatched :WolfFullWidth command must NOT toggle the flag on its own"
    );

    // --- 4. Direct editor-affecting toggle flips the flag ON -----------------
    let win = handle.active_win();
    assert_ne!(win, 0, "attach should report a nonzero active window handle");
    handle.toggle_full_width();
    wait_for_full_width_active(&handle, true, "after the first toggle_full_width()");
    wait_for_win_full_width(&handle, win, true, "the active window's per-window flag flips true");

    // --- 5. Toggling again flips the flag back to the ~90-char default -------
    handle.toggle_full_width();
    wait_for_full_width_active(&handle, false, "after the second toggle_full_width()");
    wait_for_win_full_width(
        &handle,
        win,
        false,
        "the active window's per-window flag flips back to false",
    );

    // --- 6. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}

#[test]
fn full_width_flag_flows_to_the_preview_source() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd holding a markdown file, kept alive until after `shutdown()`.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n",
    )
    .expect("failed to write alpha.md");

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open alpha.md as the source and split-right preview it ----
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");

    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");

    // --- 2. previews() surfaces one row for alpha.md, defaulting to NOT full width
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let alpha_row = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist");
    assert!(
        !alpha_row.source_full_width(),
        "a fresh markdown source defaults to the ~90-char width, so the preview row's \
         source_full_width() must start false"
    );

    // --- 3. Toggle the SOURCE buffer full width; the preview row's flag follows
    // Focus the markdown SOURCE pane (the split left the preview pane active) so the
    // toggle lands on alpha.md, then flip it. The Lua helper re-emits wolf_preview,
    // so the preview row's source_full_width() must flip to true.
    focus_markdown_source(&handle);
    handle.toggle_full_width();
    wait_for_previews(
        &handle,
        "the preview row's source_full_width() flips true after toggling the source",
        |p| {
            p.iter()
                .any(|row| row.source_name() == "alpha.md" && row.source_full_width())
        },
    );

    // --- 4. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
