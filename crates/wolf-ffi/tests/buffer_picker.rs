//! Integration test for the `wolf-ffi` BUFFER-PICKER bridge surface (plan §5 /
//! §8c): the opaque `FfiBufferPicker` returned by
//! `WolfCoreHandle::buffer_picker()`, its `ranked(query, max) -> Vec<FfiBufferRow>`
//! ranker, and the HANDLE-level activation `WolfCoreHandle::switch_buffer(bufnr)`
//! (which resolves `active_win()` live and window-scopes the switch via
//! `nvim_win_set_buf`, then restores that pane's saved `(winid, bufnr)` view).
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is modeled on `tests/command_palette.rs` and
//! `tests/panes.rs`, reusing `tests/common/mod.rs`.
//!
//! ## Dependency on coder work (plan §5 / §8c)
//!
//! None of this exists yet: the coder adds, on `WolfCoreHandle`,
//!   * `buffer_picker(&self) -> FfiBufferPicker` (seeded from a BLOCKING
//!     `list_buffers()` snapshot taken at construction), and
//!   * `switch_buffer(&self, bufnr: i64)` (reads `active_win()` live, then
//!     window-scopes the switch),
//! plus the opaque types
//!   * `FfiBufferPicker { fn ranked(&self, query: String, max: usize) ->
//!     Vec<FfiBufferRow> }`
//!   * `FfiBufferRow { fn bufnr() -> i64; fn display() -> String;
//!     fn modified() -> bool }`
//! in `crates/wolf-ffi/src/lib.rs`, regenerates `generated/`, AND adds the
//! `//crates/wolf-ffi:buffer_picker` `rust_test` target to `BUILD.bazel` (a copy
//! of the `command_palette` stanza, srcs `["tests/common/mod.rs",
//! "tests/buffer_picker.rs"]`, `crate_root = "tests/buffer_picker.rs"`). This test
//! file does NOT touch non-test source or BUILD files, and it will not compile
//! until that lands — expected.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim (`WOLF_NVIM_BIN` or the
//! Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available.
//!
//! ## SINGLE #[test]
//!
//! The whole flow lives in ONE `#[test]` because it walks a stateful sequence
//! against a single embedded Neovim (create buffers, snapshot bufnrs, then
//! scroll / switch / round-trip a view), mirroring `command_palette.rs`.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiBufferRow, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (buffer creation, the ordered `switch_buffer` RPC, the winsaveview/winrestview
/// round-trip) to settle. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A `max` large enough that the assembled buffer set is never truncated (a
/// handful of buffers stays well under this).
const RANKED_MAX: usize = 100;

/// Line count for each generated temp buffer — comfortably larger than the 24-row
/// grid so a `G` (jump to last line) forces a NONZERO topline.
const LINE_COUNT: usize = 200;

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false (the already-resolved glyph rides in `ch`, per the Shift contract). Same
/// as `panes.rs` / `commands.rs`.
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
/// the leading `:`. Copy of `panes.rs`'s `send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// Writes a temp buffer file of [`LINE_COUNT`] numbered lines under `dir`, named
/// `name`, and returns its absolute path as a string.
fn write_long_file(dir: &std::path::Path, name: &str) -> String {
    let mut body = String::new();
    for i in 1..=LINE_COUNT {
        body.push_str(&format!("line {i}\n"));
    }
    let path = dir.join(name);
    std::fs::write(&path, body).expect("failed to write temp buffer file");
    path.to_string_lossy().into_owned()
}

/// Fresh buffer-picker snapshot ranked over the empty query (the picker snapshots
/// `list_buffers()` at construction, so a NEW session is needed to see new
/// buffers). Mirrors the "open a fresh session" pattern in `command_palette.rs`.
fn ranked_snapshot(handle: &wolf_ffi::WolfCoreHandle) -> Vec<FfiBufferRow> {
    handle.buffer_picker().ranked(String::new(), RANKED_MAX)
}

/// True if any ranked row's `display()` ends with `suffix` (robust to
/// cwd-relative vs. absolute display and to `/var`↔`/private/var` symlinking).
fn any_display_ends_with(rows: &[FfiBufferRow], suffix: &str) -> bool {
    rows.iter().any(|r| r.display().ends_with(suffix))
}

/// The `bufnr()` of the first ranked row whose `display()` ends with `suffix`.
fn bufnr_for_suffix(rows: &[FfiBufferRow], suffix: &str) -> Option<i64> {
    rows.iter()
        .find(|r| r.display().ends_with(suffix))
        .map(FfiBufferRow::bufnr)
}

/// The topline reported by the ACTIVE pane's grid (0-based, per `win_viewport`).
fn active_topline(handle: &wolf_ffi::WolfCoreHandle) -> u64 {
    let grid = handle.grid_for_win(handle.active_win());
    handle.read_frame_for_grid(grid).topline()
}

/// The base filename shown by the ACTIVE pane's statusbar.
fn active_filename(handle: &wolf_ffi::WolfCoreHandle) -> String {
    let grid = handle.grid_for_win(handle.active_win());
    handle.pane_status_for_grid(grid).filename()
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until `predicate(&rows)` holds on a FRESH
/// ranked snapshot, returning that snapshot. Panics with `desc` on timeout.
fn wait_for_ranked(
    handle: &wolf_ffi::WolfCoreHandle,
    desc: &str,
    predicate: impl Fn(&[FfiBufferRow]) -> bool,
) -> Vec<FfiBufferRow> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let rows = ranked_snapshot(handle);
        if predicate(&rows) {
            return rows;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for ranked snapshot ({desc})");
}

/// Blocks until the active pane's filename equals `target`, then asserts it.
fn wait_for_active_filename(handle: &wolf_ffi::WolfCoreHandle, target: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut name = active_filename(handle);
    while Instant::now() < deadline {
        name = active_filename(handle);
        if name == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        name, target,
        "active pane should show {target} (got {name})"
    );
}

/// Blocks until the active pane's topline equals `target`, then asserts it.
fn wait_for_active_topline(handle: &wolf_ffi::WolfCoreHandle, target: u64, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut topline = active_topline(handle);
    while Instant::now() < deadline {
        topline = active_topline(handle);
        if topline == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(topline, target, "active topline should be {target} ({desc})");
}

#[test]
fn buffer_picker_lists_switches_and_restores_view() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd (also the directory holding the buffer files), kept alive
    // until after `shutdown()`.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");

    // Distinctive suffixes so end-of-display matching is unambiguous.
    let path_a = write_long_file(tmp.path(), "buffer_a.md");
    let path_b = write_long_file(tmp.path(), "buffer_b.md");
    let path_c = write_long_file(tmp.path(), "buffer_c.md");
    let path_z = write_long_file(tmp.path(), "unlisted_z.md");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows,
    // window_index).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Poll read_frame() until attached (cols > 0) ---------------------
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

    // --- 2. Create buffers ---------------------------------------------------
    // Visit A then B in the single editor window (so lastused(B) > lastused(A)).
    // `:badd C` adds C to the buffer list as a LISTED buffer WITHOUT displaying it
    // in this window (so this pane has never shown C — used for the top-of-buffer
    // case). Then display Z and make it UNLISTED (so it is filtered out).
    // `getbufinfo().lastused` has 1-SECOND resolution, so A and B must become
    // current in DIFFERENT wall-clock seconds for B to strictly outrank A on
    // lastused (otherwise they TIE and the locked bufnr-ascending tie-break returns
    // [A, B]). `send_ex_command` is fire-and-forget through the async input pump, so
    // we must AWAIT each buffer actually becoming current around the clock gap —
    // otherwise A's keystrokes drain together with the post-sleep burst and A/B
    // still land in the same second.
    send_ex_command(&handle, &format!(":e {path_a}"));
    wait_for_active_filename(&handle, "buffer_a.md"); // A is current at t0, BEFORE the gap
    sleep(Duration::from_millis(1100));
    send_ex_command(&handle, &format!(":e {path_b}"));
    wait_for_active_filename(&handle, "buffer_b.md"); // B enters in the later second
    send_ex_command(&handle, &format!(":badd {path_c}"));
    send_ex_command(&handle, &format!(":e {path_z}"));
    send_ex_command(&handle, ":setlocal nobuflisted");

    // --- 3. buflisted-only filtering + snapshot bufnrs ----------------------
    // Wait until the fresh snapshot lists A, B, and C (all buflisted). The current
    // buffer Z is UNLISTED and must be excluded.
    let rows = wait_for_ranked(&handle, "A, B, C all listed", |rows| {
        any_display_ends_with(rows, "buffer_a.md")
            && any_display_ends_with(rows, "buffer_b.md")
            && any_display_ends_with(rows, "buffer_c.md")
    });
    assert!(
        !any_display_ends_with(&rows, "unlisted_z.md"),
        "the UNLISTED buffer (unlisted_z.md) must be excluded from ranked(\"\"): {:?}",
        rows.iter().map(FfiBufferRow::display).collect::<Vec<_>>()
    );

    let bufnr_a = bufnr_for_suffix(&rows, "buffer_a.md").expect("buffer_a.md must have a bufnr");
    let bufnr_b = bufnr_for_suffix(&rows, "buffer_b.md").expect("buffer_b.md must have a bufnr");
    let bufnr_c = bufnr_for_suffix(&rows, "buffer_c.md").expect("buffer_c.md must have a bufnr");

    // --- 4. lastused MRU ordering -------------------------------------------
    // B was visited AFTER A, so among the listed buffers B (highest lastused) is
    // the current/most-recent and sorts to index 0; A sorts ahead of C (never
    // visited).
    assert!(
        rows.first().is_some_and(|r| r.display().ends_with("buffer_b.md")),
        "the most-recently-used listed buffer (buffer_b.md) must sort to index 0: {:?}",
        rows.iter().map(FfiBufferRow::display).collect::<Vec<_>>()
    );
    let pos = |bufnr: i64| {
        rows.iter()
            .position(|r| r.bufnr() == bufnr)
            .expect("bufnr must be present in the ranked snapshot")
    };
    assert!(
        pos(bufnr_a) < pos(bufnr_c),
        "the visited buffer A must sort ahead of the never-visited buffer C"
    );

    // --- 5. switch_buffer switches the ACTIVE pane --------------------------
    // The active pane currently shows the unlisted Z. Switch it to A (window-scoped
    // via nvim_win_set_buf) and observe the pane's statusbar follow.
    handle.switch_buffer(bufnr_a);
    wait_for_active_filename(&handle, "buffer_a.md");

    // --- 6. (winid, bufnr) VIEW ROUND-TRIP ----------------------------------
    // Scroll A to the bottom (`G`) so the pane's topline is NONZERO, capture it,
    // switch away to B, switch back to A, and assert the saved view is restored.
    // (An RPC `switch_buffer` leaves the editor in normal mode, so `G` jumps to the
    // last line.)
    handle.send_key(char_key('G'));

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut t_scrolled = active_topline(&handle);
    while Instant::now() < deadline {
        t_scrolled = active_topline(&handle);
        if t_scrolled > 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        t_scrolled > 0,
        "jumping to the last line of A should force a NONZERO topline (got {t_scrolled})"
    );

    // Leave A (winsaveview captured for (win, A)) ...
    handle.switch_buffer(bufnr_b);
    wait_for_active_filename(&handle, "buffer_b.md");

    // ... then return to A: the saved view for THIS pane + A is restored.
    handle.switch_buffer(bufnr_a);
    wait_for_active_filename(&handle, "buffer_a.md");
    wait_for_active_topline(&handle, t_scrolled, "restored view for (win, A)");

    // --- 7. TOP-OF-BUFFER when the pane never showed the buffer -------------
    // C was `:badd`ed and never displayed in this pane, so it has NO saved view:
    // switching to it forces the top of the buffer (topline 0). The active topline
    // is currently t_scrolled (> 0), so the transition to 0 is observable.
    handle.switch_buffer(bufnr_c);
    wait_for_active_filename(&handle, "buffer_c.md");
    wait_for_active_topline(&handle, 0, "top-of-buffer for a never-shown buffer");

    // --- 8. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
