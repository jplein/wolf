//! Integration test for **per-cell text ATTRIBUTES + the special (underline)
//! color** crossing the FFI bridge.
//!
//! This drives a REAL, hermetic Neovim — NO mocks, per the project testing policy —
//! and proves that when a highlight group carrying bold/italic/underline attributes
//! AND a `special` (underline) color is applied to a buffer cell, the resolved
//! [`wolf_ffi`] `FfiCell` exposes BOTH the correct packed `flags()` bits AND the
//! `special()` color across the bridge. It is the primary guard for the macOS
//! front-end's attribute-painting work (`FfiCell::special()` is a new accessor).
//!
//! ## What this proves
//!
//! After putting a single `X` in the buffer, defining a `WolfAttr` highlight group
//! with `bold=true, italic=true, underline=true, special='#00ff00', fg='#ffffff'`,
//! and applying it to the `X` via `vim.fn.matchadd`, the cell whose `text() == "X"`
//! has the BOLD, ITALIC and UNDERLINE flag bits set and reports `special().set ==
//! true` with `special().rgb == 0x0000FF00`.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings this test needs a pinned nvim (`nvim_bin_or_skip`); when none
//! is available (the cargo / Linux path where Bazel does not stage the pinned nvim
//! tree) it is a loud no-op pass, so it only ASSERTS when there is a real nvim to
//! exercise (Bazel/macOS).

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus applying the highlight
/// and repainting the grid. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for attach / the styled repaint. Mirrors the sibling
/// tests' `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- Style-flag bit layout, copied verbatim from `src/lib.rs`'s `mod flag` (the
// single source of truth the test asserts against — do not diverge). --
/// Bold bit.
const BOLD: u8 = 1 << 0;
/// Italic bit.
const ITALIC: u8 = 1 << 1;
/// Underline bit.
const UNDERLINE: u8 = 1 << 2;

/// The special (underline) color the `WolfAttr` group defines, packed as
/// 0x00RRGGBB (`#00ff00` → `0x0000FF00`).
const EXPECTED_SPECIAL: u32 = 0x0000_FF00;

/// The single known glyph the highlight is applied to.
const MARKED_CHAR: &str = "X";

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

/// Types `cmd` as an Ex command and presses Enter: each char goes through the same
/// `send_key` / `FfiKeyEvent` path the generated Swift glue uses, then a final
/// `<CR>` executes it. `cmd` must include the leading `:`. Mirrors `panes.rs`.
fn send_ex_command(handle: &WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

#[test]
fn attribute_highlight_flags_and_special_cross_ffi() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    // A fresh temp cwd (its `<cwd>/.wolf/` MRU cache must outlive the session).
    let cwd = common::fresh_cwd();
    let init_lua = common::init_lua_path();

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
        0,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- Poll read_frame() until attached (cols > 0) ------------------------
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

    // --- Put a known glyph in the buffer, define an attribute highlight group,
    // and apply it to the glyph via matchadd. One `:lua` line, `;`-separated, so it
    // rides the same send_key path Swift uses. No path interpolation here, so the
    // lua literal stays simple (single `{`/`}` — NOT doubled, since this is a plain
    // `&str`, not a Rust `format!`).
    let lua_cmd = ":lua vim.api.nvim_buf_set_lines(0,0,-1,false,{'X'}); \
         vim.api.nvim_set_hl(0,'WolfAttr',{bold=true,italic=true,underline=true,special='#00ff00',fg='#ffffff'}); \
         vim.fn.matchadd('WolfAttr','X')";
    send_ex_command(&handle, lua_cmd);

    // --- Poll for the styled repaint: scan the frame for the `X` cell and wait
    // until its resolved attributes carry BOLD|ITALIC|UNDERLINE and a set special
    // color (the repaint may lag the command by a frame or two).
    let want = BOLD | ITALIC | UNDERLINE;
    let mut saw_x = false;
    let mut got_flags: u8 = 0;
    let mut got_special_set = false;
    let mut got_special_rgb: u32 = 0;
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let frame = handle.read_frame();
        if let Some(cell) = frame.cells().into_iter().find(|c| c.text() == MARKED_CHAR) {
            saw_x = true;
            let special = cell.special();
            got_flags = cell.flags();
            got_special_set = special.set;
            got_special_rgb = special.rgb;
            if got_flags & want == want && got_special_set {
                break;
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        sleep(POLL_INTERVAL);
    }

    assert!(
        saw_x,
        "the buffer's '{MARKED_CHAR}' glyph should appear as a cell in the frame"
    );
    assert!(
        got_flags & BOLD != 0,
        "the WolfAttr group's bold=true should set the BOLD flag bit (flags {got_flags:#010b})"
    );
    assert!(
        got_flags & ITALIC != 0,
        "the WolfAttr group's italic=true should set the ITALIC flag bit (flags {got_flags:#010b})"
    );
    assert!(
        got_flags & UNDERLINE != 0,
        "the WolfAttr group's underline=true should set the UNDERLINE flag bit (flags {got_flags:#010b})"
    );
    assert!(
        got_special_set,
        "the WolfAttr group's special='#00ff00' should surface as a SET special color"
    );
    assert_eq!(
        got_special_rgb, EXPECTED_SPECIAL,
        "the special color should pack to 0x00RRGGBB exactly (#00ff00 -> 0x0000FF00)"
    );

    // --- Shutdown: clean teardown -------------------------------------------
    handle.shutdown();
}
