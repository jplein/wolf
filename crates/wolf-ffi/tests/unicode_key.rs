//! Real-nvim integration test for the COMPOSED-CHARACTER delivery path (the macOS
//! option-as-option feature): a single Unicode scalar such as "ü" (U+00FC),
//! delivered as ONE modifier-free `FfiKeyEvent { kind: char, ch, all mods false }`,
//! must land verbatim in the Neovim buffer in insert mode.
//!
//! This is the Rust-side proof for the Swift `KeyComposer` → `charEvents` →
//! `send_key` path: the AppKit/Carbon composition glue is not headlessly testable,
//! but the composed SCALAR crossing the FFI bridge and reaching nvim IS. It does NOT
//! involve Swift, AppKit, or any window — it constructs `FfiKeyEvent`s by hand and
//! calls `wolf_ffi_start` / `read_frame` / `send_key` directly, exactly like
//! `bridge_smoke`.
//!
//! ## SKIP-when-unset policy
//!
//! Like `bridge_smoke`, this needs a pinned nvim located via `WOLF_NVIM_BIN` (an
//! explicit env var, or the Bazel-staged tree). It **loudly SKIPS** (no-op pass)
//! when no pinned nvim is available. See `common::nvim_bin_or_skip`.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiFrame, FfiKeyEvent};

/// `FfiKeyEvent.kind` discriminant for a printable Char (mirrors `wolf_core::Key`
/// variant order, where `Char` is first). Must match `bridge_smoke`'s `KIND_CHAR`.
const KIND_CHAR: u8 = 0;

/// The composed "ü" scalar (U+00FC, LATIN SMALL LETTER U WITH DIAERESIS) — the
/// canonical Option-u-then-u dead-key result the feature must deliver.
const U_UMLAUT: u32 = 0x00FC;

/// A composed "é" scalar (U+00E9), used by the two-scalar sequence case.
const E_ACUTE: u32 = 0x00E9;

/// Generous overall timeout for nvim `--embed` startup + redraw processing, matching
/// `bridge_smoke`'s `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for nvim to reflect input (the `FfiFrame` surface does
/// not expose a snapshot `seq`, so we POLL `read_frame()` until the grid changes).
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Builds a modifier-free printable-character `FfiKeyEvent` (kind=Char) for the given
/// Unicode scalar value — the exact shape `KeyComposition.charEvents` emits per
/// composed scalar (all four modifiers false, the resolved glyph riding in `ch`).
fn scalar_key(ch: u32) -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_CHAR,
        ch,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Flattens the current frame's cell texts (row-major) into one string, for a robust
/// "did the composed glyph appear" comparison that does not pin exact coordinates.
fn frame_text(frame: &FfiFrame) -> String {
    frame.cells().into_iter().map(|cell| cell.text).collect()
}

#[test]
fn composed_scalar_lands_in_buffer() {
    // Loud SKIP (no-op pass) when no pinned nvim is available.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let cwd = common::fresh_cwd();

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- Single composed scalar: in insert mode, ü as one modifier-free Char event, exactly what
    // `KeyComposition.charEvents("ü")` yields, must land verbatim.
    handle.send_key(scalar_key('i' as u32));
    handle.send_key(scalar_key(U_UMLAUT));

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after = frame_text(&handle.read_frame());
    while Instant::now() < deadline {
        after = frame_text(&handle.read_frame());
        if after.contains('ü') {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        after.contains('ü'),
        "the composed 'ü' scalar should appear in the grid after insert (after={after:?})"
    );

    // --- Two-scalar sequence: one modifier-free event per scalar, which wolf-core fuses into one
    // nvim_input; both glyphs must land, in order.
    handle.send_key(scalar_key(E_ACUTE));
    handle.send_key(scalar_key(U_UMLAUT));

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after = frame_text(&handle.read_frame());
    while Instant::now() < deadline {
        after = frame_text(&handle.read_frame());
        // After the first "ü", appending "é" then "ü" yields the run "üéü".
        if after.contains("üéü") {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        after.contains('é'),
        "the composed 'é' scalar should appear in the grid (after={after:?})"
    );
    assert!(
        after.contains("üéü"),
        "both composed scalars should land, in order, after the first 'ü' (after={after:?})"
    );

    // --- Shutdown: clean teardown -------------------------------------------
    handle.shutdown();
}
