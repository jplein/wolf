//! Integration test for the COPY / PASTE / visual-mode FFI surface (plan §6.4):
//! `WolfCoreHandle::is_visual_mode()`, `copy_selection()`, and `paste(text)`.
//!
//! This drives the RUST half of the swift-bridge command machinery against a REAL,
//! hermetic Neovim — NO mocks, per the project testing policy. It is the sibling of
//! `close_pane.rs` (same staging / SKIP-when-unset machinery, reusing
//! `tests/common/mod.rs`, and the same `read_frame` live-grid polling pattern).
//!
//! ## Dependency on coder work (plan §4)
//!
//! These bridge methods do NOT exist yet — the coder adds them to the `extern
//! "Rust"` block and `impl WolfCoreHandle` in `crates/wolf-ffi/src/lib.rs`
//! (backed by `WolfCore::copy_visual_selection` / `paste` / `Snapshot::is_visual_mode`),
//! and adds the `//crates/wolf-ffi:copy_paste` `rust_test` target to `BUILD.bazel`
//! (copy of `close_pane`). This file does NOT touch BUILD files, and will not
//! compile until those methods land — that is expected for test-first.
//!
//!   * `fn is_visual_mode(&self) -> bool;`
//!   * `fn copy_selection(&self) -> String;` (empty string when not visual / empty)
//!   * `fn paste(&self, text: String);`
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each observable change
/// to settle. Mirrors `close_pane.rs`'s `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for a mode/grid change to surface. Mirrors
/// `close_pane.rs`'s `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
//
// These mirror the `wolf_core::Key` variant order (Char = 0, Enter = 1, Esc = 2),
// same as `multi_window.rs` / `orlando_bg_stress.rs`.
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Escape key (leaves insert / visual mode).
const KIND_ESC: u8 = 2;

/// The distinctive, space-free word typed then visually selected. `v e` from its
/// first char selects exactly this token, so `copy_selection()` must CONTAIN it.
const SELECT_WORD: &str = "WOLF";

/// A single distinctive glyph pasted into the buffer; it must surface in the grid.
const PASTE_GLYPH: char = 'Z';

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false (the already-resolved glyph rides in `ch`). Same as `close_pane.rs`.
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

/// Builds an Escape `FfiKeyEvent` (kind=Esc), all modifiers false.
fn esc_key() -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_ESC,
        ch: 0,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Sends each character of `s` as an individual `send_key`, in order.
fn type_chars(handle: &wolf_ffi::WolfCoreHandle, s: &str) {
    for c in s.chars() {
        handle.send_key(char_key(c));
    }
}

/// Flattens the current active frame's cell texts (row-major) into one string, so
/// a "did the glyph appear" check does not pin exact coordinates. Same shape as
/// `bridge_smoke.rs`'s `frame_text`.
fn frame_text(handle: &wolf_ffi::WolfCoreHandle) -> String {
    handle
        .read_frame()
        .cells()
        .into_iter()
        .map(|cell| cell.text)
        .collect()
}

/// Polls `pred` until it returns true or [`FLUSH_TIMEOUT`] elapses; returns the
/// last observed value. Copy of the poll pattern used across the ffi suite.
fn poll_until(mut pred: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut ok = pred();
    while !ok && Instant::now() < deadline {
        sleep(POLL_INTERVAL);
        ok = pred();
    }
    ok
}

#[test]
fn copy_selection_and_paste_round_trip() {
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

    // Fresh, empty buffer: normal mode, so NOT visual.
    assert!(
        poll_until(|| !handle.is_visual_mode()),
        "a freshly attached buffer starts in normal mode (is_visual_mode == false)"
    );

    // --- 2. Type a known word, escape, anchor at line start -----------------
    // `i` -> insert, type SELECT_WORD, `<Esc>` -> normal (cursor on last char).
    handle.send_key(char_key('i'));
    type_chars(&handle, SELECT_WORD);
    handle.send_key(esc_key());
    // Wait until the typed word shows in the grid so the buffer is settled.
    assert!(
        poll_until(|| frame_text(&handle).contains(SELECT_WORD)),
        "the typed word {SELECT_WORD:?} should appear in the grid after insert"
    );

    // --- 3. Enter a character-wise visual selection of the word -------------
    // `0` -> first column, `v` -> visual (anchored on 'W'), `e` -> extend to the
    // word end, giving an inclusive selection of exactly SELECT_WORD.
    handle.send_key(char_key('0'));
    handle.send_key(char_key('v'));
    handle.send_key(char_key('e'));

    assert!(
        poll_until(|| handle.is_visual_mode()),
        "after `v e` the handle should report visual mode (is_visual_mode == true)"
    );

    // copy_selection() yanks the visual selection out of nvim and returns it.
    let selection = handle.copy_selection();
    assert!(
        selection.contains(SELECT_WORD),
        "copy_selection() should return the selected word {SELECT_WORD:?}; got {selection:?}"
    );

    // --- 4. Leave visual mode -> is_visual_mode() goes false ----------------
    handle.send_key(esc_key());
    assert!(
        poll_until(|| !handle.is_visual_mode()),
        "after <Esc> the handle should report normal mode (is_visual_mode == false)"
    );

    // --- 5. paste() inserts the glyph; poll the grid until it appears -------
    // In normal mode, `paste` uses nvim_paste, which inserts at the cursor.
    handle.paste(PASTE_GLYPH.to_string());
    assert!(
        poll_until(|| frame_text(&handle).contains(PASTE_GLYPH)),
        "the pasted glyph {PASTE_GLYPH:?} should appear somewhere in the grid after paste"
    );

    // --- 6. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
