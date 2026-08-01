//! Integration test for the `WolfCoreHandle::set_background(dark)` bridge method
//! (the appearance plan §3.5 / §5.3): the macOS front-end resolves a dark/light
//! appearance and fans it out to each project's core over the FFI bridge, which
//! must flip the captive Neovim's global `&background`.
//!
//! This drives the RUST half of the swift-bridge appearance machinery against a
//! REAL, hermetic Neovim — NO mocks, per the project testing policy. It is the
//! sibling of `theme.rs` (same staging / SKIP-when-unset machinery, reusing
//! `tests/common/mod.rs`, and the same `send_key` Ex-command + `cursor_fill`
//! observation).
//!
//! ## Observing `&background` over the bridge
//!
//! The bridge exposes no direct `&background` accessor, so — exactly like the
//! wolf-core `appearance.rs` test — we observe it INDIRECTLY but deterministically:
//! define `WolfCursor guibg` to a marker color CHOSEN BY the live `&background`
//! (an `:execute` branching on `&background ==# 'light'`), fire `:doautocmd
//! ColorScheme` so the theme tracker re-reads it, then read the resolved
//! `FfiFrame::cursor_fill()` (which carries the `WolfCursor guibg`). The observed
//! marker is a direct function of `&background`, so it proves the flip.
//!
//! ## Dependency on coder work (plan §3.5)
//!
//! `set_background` does NOT exist on `WolfCoreHandle` yet: the coder adds it to
//! the bridge (`extern "Rust"`) + `impl WolfCoreHandle` in
//! `crates/wolf-ffi/src/lib.rs` (backed by `wolf_core::InputEvent::SetBackground`),
//! regenerates `generated/`, AND adds the `//crates/wolf-ffi:appearance` `rust_test`
//! target to `BUILD.bazel`. This test file does NOT touch BUILD files, and it will
//! not compile until that method lands — that is expected.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. The plain repo `init.lua` is used
//! (no lighthouse sibling), whose line-111 fallback sets `vim.o.background = "dark"`
//! at launch — so the initial observed state is `dark`.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus each background flip
/// to settle. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for attach / a background flip to surface. Mirrors
/// the sibling tests' `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- WolfCursor guibg markers (0x00RRGGBB packing, like default_fg/bg) ----------
/// `WolfCursor guibg` marker written when the live `&background` is `light`.
const LIGHT_MARKER_BG: u32 = 0x0011_FF11;
/// `WolfCursor guibg` marker written when the live `&background` is `dark`.
const DARK_MARKER_BG: u32 = 0x00FF_1111;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false (the already-resolved glyph rides in `ch`, per the Shift contract).
/// Mirrors `theme.rs`.
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

/// Builds an Enter `FfiKeyEvent` (kind=Enter), all modifiers false. Mirrors
/// `theme.rs`.
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
/// `theme.rs`.
fn send_ex_command(handle: &WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// Polls `handle.read_frame()` until it attaches (`cols() > 0`) or panics after
/// [`FLUSH_TIMEOUT`]. Mirrors the sibling tests' attach loop.
fn wait_for_attach(handle: &WolfCoreHandle) {
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

/// Sets `WolfCursor guibg` to the marker color dictated by the LIVE `&background`
/// (light -> [`LIGHT_MARKER_BG`], else [`DARK_MARKER_BG`]), fires `ColorScheme` so
/// the theme tracker re-reads it, then polls `cursor_fill()` until it carries the
/// marker for the expected `&background`. `want_light == true` asserts `&background`
/// is `light`; `false` asserts `dark`.
fn assert_background_is(handle: &WolfCoreHandle, want_light: bool) {
    // `:execute` runs legacy Vimscript, so the `?:` ternary + `..` concat pick the
    // marker from the LIVE `&background`. This is the load-bearing read of the
    // option — the marker we then observe is a direct function of `&background`.
    send_ex_command(
        handle,
        ":execute 'hi WolfCursor guibg=' .. (&background ==# 'light' ? '#11ff11' : '#ff1111')",
    );
    // A plain `:hi`/`:execute hi` does not fire ColorScheme, so fire it explicitly
    // to make the tracker re-emit the theme (see module docs / `theme.rs`).
    send_ex_command(handle, ":doautocmd ColorScheme");

    let (want, label) = if want_light {
        (LIGHT_MARKER_BG, "light")
    } else {
        (DARK_MARKER_BG, "dark")
    };

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut fill = handle.read_frame().cursor_fill();
    while Instant::now() < deadline {
        fill = handle.read_frame().cursor_fill();
        if fill.set && fill.rgb == want {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        fill.set && fill.rgb == want,
        "expected &background to be {label} (cursor_fill marker {want:#08x}); \
         got set={}, rgb={:#08x}",
        fill.set,
        fill.rgb
    );
}

#[test]
fn set_background_over_the_bridge() {
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

    wait_for_attach(&handle);

    // Baseline: the captive nvim launches with `&background == "dark"`.
    assert_background_is(&handle, false);

    // set_background(false) -> `:set background=light`.
    handle.set_background(false);
    assert_background_is(&handle, true);

    // set_background(true) -> `:set background=dark`. Also proves the fire-and-forget
    // bridge method leaves the core live (the frame + theme keep updating).
    handle.set_background(true);
    assert_background_is(&handle, false);

    handle.shutdown();
    drop(tmp);
}
