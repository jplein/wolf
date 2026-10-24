//! Integration test for the WolfCursor / WolfTitlebar theme bridge surface.
//!
//! This drives the RUST half of the swift-bridge theme machinery against a REAL,
//! hermetic Neovim — NO mocks, per the project testing policy. It is the sibling
//! of `panes.rs` (same staging / SKIP-when-unset machinery, reusing
//! `tests/common/mod.rs`) but exercises the theme surface:
//!
//!   * `WolfCoreHandle::theme_titlebar_bg() -> FfiColor` (`.rgb` / `.set`, the
//!     `default_fg`/`default_bg` unset convention: `set == false` when the
//!     `WolfTitlebar` group is absent or cleared)
//!   * `WolfCoreHandle::theme_titlebar_fg() -> FfiColor`
//!   * the EXISTING `FfiFrame::cursor_fill()` / `cursor_glyph()`, which carry
//!     the `WolfCursor` override pre-resolved (bg = fill, fg = glyph) — no new
//!     cursor FFI surface.
//!
//! The `//crates/wolf-ffi:theme` `rust_test` target runs this file under Bazel.
//!
//! ## NOTE on `:doautocmd ColorScheme`
//!
//! Theme detection rides a `ColorScheme` autocmd (plus an emit-once at setup).
//! A plain `:hi` change does NOT fire that autocmd, so every `:hi` mutation
//! below is followed by an explicit `:doautocmd ColorScheme` — REQUIRED by
//! design, not a test convenience.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. The plain repo `init.lua` is used
//! with NO lighthouse sibling, so no colorscheme defines the Wolf groups at
//! startup (startup pickup from the real scheme is `lighthouse_load.rs`'s job).
//!
//! ## What this asserts
//!
//! At startup both titlebar channels are unset; defining `WolfCursor
//! guifg=#000000 guibg=#009fce` and `WolfTitlebar guifg=#f1f1f1 guibg=#101010`
//! plus `:doautocmd ColorScheme` surfaces the titlebar colors on the handle and
//! the cursor colors on the frame; clearing both groups plus `:doautocmd
//! ColorScheme` reverts the titlebar to unset and the cursor fill to the
//! pre-theme reverse-video baseline.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus each theme change to
/// settle. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for attach / a theme change to surface. Mirrors
/// the sibling tests' `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long to let the startup redraws (default colors, emit-once) settle after
/// attach before capturing the pre-theme cursor-fill baseline, so the baseline
/// is not read mid-startup.
const BASELINE_SETTLE: Duration = Duration::from_millis(500);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- The themed fixture colors (0x00RRGGBB packing, like default_fg/bg) --------
/// `WolfCursor guifg=#000000` — the themed cursor GLYPH color.
const CURSOR_GLYPH: u32 = 0x0000_0000;
/// `WolfCursor guibg=#009fce` — the themed cursor FILL color.
const CURSOR_FILL: u32 = 0x0000_9FCE;
/// `WolfTitlebar guifg=#f1f1f1`.
const TITLEBAR_FG: u32 = 0x00F1_F1F1;
/// `WolfTitlebar guibg=#101010`.
const TITLEBAR_BG: u32 = 0x0010_1010;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false (the already-resolved glyph rides in `ch`, per the Shift contract).
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

/// Types `cmd` as an Ex command and presses Enter: each char goes through the
/// same `send_key` / `FfiKeyEvent` path the generated Swift glue uses, then a
/// final `<CR>` executes it. `cmd` must include the leading `:`. Mirrors
/// `panes.rs`.
fn send_ex_command(handle: &WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

#[test]
fn theme_titlebar_and_cursor_follow_wolf_groups() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd (it is nvim's working dir + search root, so it must outlive the session),
    // kept alive until after `shutdown()`, then dropped explicitly at the end.
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

    // --- 2. Baseline: no Wolf groups, so titlebar channels are unset, as before the first emit.
    let baseline_bg = handle.theme_titlebar_bg();
    assert!(
        !baseline_bg.set,
        "with no WolfTitlebar group defined, theme_titlebar_bg() must be unset \
         (got set with rgb {:#08x})",
        baseline_bg.rgb
    );
    let baseline_fg = handle.theme_titlebar_fg();
    assert!(
        !baseline_fg.set,
        "with no WolfTitlebar group defined, theme_titlebar_fg() must be unset \
         (got set with rgb {:#08x})",
        baseline_fg.rgb
    );

    // Let startup settle, then capture the pre-theme reverse-video fill that step 5 must restore.
    sleep(BASELINE_SETTLE);
    let baseline_fill = handle.read_frame().cursor_fill();
    assert!(
        baseline_fill.set,
        "the reverse-video cursor fill is always set (got unset)"
    );
    assert_ne!(
        baseline_fill.rgb, CURSOR_FILL,
        "sanity: the pre-theme reverse-video fill must differ from the themed \
         fill, or the revert assertion below would be vacuous"
    );

    // --- 3. Define both groups, then fire ColorScheme: a plain `:hi` does not fire the autocmd
    // the tracker rides.
    send_ex_command(&handle, ":hi WolfCursor guifg=#000000 guibg=#009fce");
    send_ex_command(&handle, ":hi WolfTitlebar guifg=#f1f1f1 guibg=#101010");
    send_ex_command(&handle, ":doautocmd ColorScheme");

    // Poll until the titlebar bg surfaces as set with the exact packed rgb.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut titlebar_bg = handle.theme_titlebar_bg();
    while Instant::now() < deadline {
        titlebar_bg = handle.theme_titlebar_bg();
        if titlebar_bg.set {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        titlebar_bg.set,
        "theme_titlebar_bg() should become set after :hi WolfTitlebar + \
         :doautocmd ColorScheme"
    );
    assert_eq!(
        titlebar_bg.rgb, TITLEBAR_BG,
        "theme_titlebar_bg() should carry the exact packed guibg (#101010)"
    );

    // The fg rides the same notification, so it is set by now too.
    let titlebar_fg = handle.theme_titlebar_fg();
    assert!(
        titlebar_fg.set,
        "theme_titlebar_fg() should be set once the theme notification landed"
    );
    assert_eq!(
        titlebar_fg.rgb, TITLEBAR_FG,
        "theme_titlebar_fg() should carry the exact packed guifg (#f1f1f1)"
    );

    // --- 4. cursor_fill/cursor_glyph resolve at pack time from the snapshot the titlebar poll
    // just observed, so no further waiting is needed.
    let themed = handle.read_frame();
    assert_eq!(
        themed.cursor_fill().rgb,
        CURSOR_FILL,
        "cursor_fill() should be the WolfCursor guibg (#009fce)"
    );
    assert_eq!(
        themed.cursor_glyph().rgb,
        CURSOR_GLYPH,
        "cursor_glyph() should be the WolfCursor guifg (#000000)"
    );

    // --- 5. Clear both groups + ColorScheme -> full fallback ----------------
    send_ex_command(&handle, ":hi clear WolfCursor");
    send_ex_command(&handle, ":hi clear WolfTitlebar");
    send_ex_command(&handle, ":doautocmd ColorScheme");

    // Poll until BOTH fallbacks land: the titlebar unset again AND the cursor
    // fill back at the pre-theme reverse-video baseline captured in step 2.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut cleared_bg = handle.theme_titlebar_bg();
    let mut cleared_fill = handle.read_frame().cursor_fill();
    while Instant::now() < deadline {
        cleared_bg = handle.theme_titlebar_bg();
        cleared_fill = handle.read_frame().cursor_fill();
        if !cleared_bg.set && cleared_fill.rgb == baseline_fill.rgb {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        !cleared_bg.set,
        "theme_titlebar_bg() should revert to unset after :hi clear WolfTitlebar \
         + :doautocmd ColorScheme"
    );
    assert!(
        !handle.theme_titlebar_fg().set,
        "theme_titlebar_fg() should revert to unset after the clear too"
    );
    assert_eq!(
        cleared_fill.rgb, baseline_fill.rgb,
        "cursor_fill() should return to the pre-theme reverse-video baseline \
         ({:#08x}) after WolfCursor is cleared",
        baseline_fill.rgb
    );

    // --- 6. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
