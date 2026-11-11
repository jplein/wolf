//! Integration tests that a terminal pane's 16 ANSI colours are the vendored Lighthouse's, for
//! the dark background and, after a switch, for the light one.
//!
//! A real, hermetic Neovim starts through `wolf_ffi_start` over the captive `init.lua` beside a
//! copy of the vendored `lighthouse/` tree, as `lighthouse_load.rs` stages it, so `init.lua`
//! activates the real colorscheme. A real `/bin/sh` prints every colour, and the tests read the
//! session's cells through the bridge, which resolve palette indices to the colours core holds.
//! The expected values come from the pinned `lighthouse.lua` through [`common::Lighthouse`].
//!
//! The colour script prints `X` in SGR 30–37 and 90–97 on row 0 and a space in SGR 40–47 and
//! 100–107 on row 1, each followed by `ESC[0m`, so ANSI colour `i` is column `i`: row 0's
//! foreground and row 1's background. No cell is bold, so no brightening applies.
//!
//! Every run prints a table of index, SGR, expected and measured colours, for the report.
//!
//! ## Premises about the components
//!
//! Each is measured by these tests against the real component, not assumed:
//!
//! - Lighthouse sets `g:terminal_color_0` to `_15` when it loads, and Neovim reloads the active
//!   colorscheme when `background` changes, which fires `ColorScheme` and so `wolf_theme`.
//! - A terminal cell's `fg()` and `bg()` carry the session palette's colour for an ANSI index.
//!
//! ## When these tests SKIP
//!
//! As `lighthouse_load.rs`: without a pinned Neovim, or without `WOLF_TEST_LIGHTHOUSE_DIR`.
//! Bazel stages the tree and the devShell exports it, so inside either a skip is a lost export.

mod common;

use std::fmt::Write as _;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{Background, Lighthouse};
use wolf_ffi::{wolf_ffi_start, FfiColor, FfiTerminalFrame, WolfCoreHandle};

/// How long nvim startup, the shell's output or a palette change may take to surface.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// `terminal_target::WINDOW`: show the session in the given window.
const TARGET_WINDOW: u8 = 0;

/// The row of foreground cells, `X` in SGR 30–37 then 90–97.
const FG_ROW: u16 = 0;

/// The row of background cells, a space in SGR 40–47 then 100–107.
const BG_ROW: u16 = 1;

/// The SGR foreground code of ANSI colour `i`; the background code is ten more.
fn sgr_fg(i: usize) -> usize {
    if i < 8 {
        30 + i
    } else {
        90 + i - 8
    }
}

/// The colour script: the two rows, then a sleep that keeps the session open.
fn colour_script() -> String {
    let mut fg = String::new();
    let mut bg = String::new();
    for i in 0..16 {
        let _ = write!(fg, "\\033[{}mX\\033[0m", sgr_fg(i));
        let _ = write!(bg, "\\033[{}m \\033[0m", sgr_fg(i) + 10);
    }
    format!("printf '{fg}\\r\\n{bg}\\r\\n'; exec sleep 600")
}

/// What one ANSI colour's two cells show: row 0's foreground and row 1's background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Measured {
    fg: Option<u32>,
    bg: Option<u32>,
}

/// A colour as the bridge gives it, `None` when unset.
fn colour(c: FfiColor) -> Option<u32> {
    c.set.then_some(c.rgb)
}

/// The 16 measured colours of a session's frame, or `None` while its rows are not printed.
fn measure(frame: &FfiTerminalFrame) -> Option<[Measured; 16]> {
    let fg_row = frame.row_cells(FG_ROW);
    let bg_row = frame.row_cells(BG_ROW);
    if fg_row.len() < 16 || bg_row.len() < 16 || fg_row[15].text() != "X" {
        return None;
    }
    Some(std::array::from_fn(|i| Measured {
        fg: colour(fg_row[i].fg()),
        bg: colour(bg_row[i].bg()),
    }))
}

/// `#rrggbb`, or `unset`.
fn hex(c: Option<u32>) -> String {
    c.map_or_else(|| "unset".to_owned(), |rgb| format!("#{rgb:06x}"))
}

/// The table of index, SGR, palette key, expected and measured colours.
fn table(
    lighthouse: &Lighthouse,
    expected: &[u32; 16],
    measured: Option<&[Measured; 16]>,
) -> String {
    let keys = lighthouse.ansi_keys();
    let mut out =
        String::from("index  sgr      key                expected  fg (row 0)  bg (row 1)\n");
    for i in 0..16 {
        let (fg, bg) = measured.map_or((None, None), |m| (m[i].fg, m[i].bg));
        out.push_str(&format!(
            "{i:>5}  {:>3}/{:<3}  {:<17}  #{:06x}   {:<10}  {}\n",
            sgr_fg(i),
            sgr_fg(i) + 10,
            keys[i],
            expected[i],
            hex(fg),
            hex(bg)
        ));
    }
    out
}

/// The indices whose foreground or background differs from `expected`.
fn mismatches(expected: &[u32; 16], measured: &[Measured; 16]) -> Vec<usize> {
    (0..16)
        .filter(|&i| measured[i].fg != Some(expected[i]) || measured[i].bg != Some(expected[i]))
        .collect()
}

/// Polls session `id` until all 32 cells show `background`'s palette, prints the table, and
/// fails naming every mismatch when they never do.
fn assert_palette(
    handle: &WolfCoreHandle,
    id: u64,
    lighthouse: &Lighthouse,
    background: Background,
    context: &str,
) {
    let expected = lighthouse.ansi(background);
    let deadline = Instant::now() + TIMEOUT;
    let measured = loop {
        let measured = measure(&handle.terminal_frame(id));
        if measured.is_some_and(|m| mismatches(&expected, &m).is_empty())
            || Instant::now() >= deadline
        {
            break measured;
        }
        sleep(POLL_INTERVAL);
    };
    let shown = table(lighthouse, &expected, measured.as_ref());
    eprintln!(
        "{} palette, {context}, against Lighthouse's {} values:\n{shown}",
        background.name(),
        background.name()
    );
    let Some(measured) = measured else {
        panic!(
            "timed out after {TIMEOUT:?} waiting for session {id} to print its two colour rows; \
             the shell did not run the colour script"
        );
    };
    let wrong = mismatches(&expected, &measured);
    assert!(
        wrong.is_empty(),
        "{context}: ANSI colours {wrong:?} must be Lighthouse's {} values after {TIMEOUT:?}. A \
         measured value from no Lighthouse palette means the session kept libghostty-vt's \
         built-in palette, so g:terminal_color_N was not sent or not laid over it; the other \
         background's values mean the change did not reach the open session; a foreground that \
         differs from its background means one of the two paths resolves the palette wrongly.\n\
         {shown}",
        background.name()
    );
}

/// A core over `init.lua` beside a copy of the vendored Lighthouse, with the session running
/// the colour script, and the parsed colorscheme; `None` to skip.
fn start() -> Option<(WolfCoreHandle, u64, Lighthouse, tempfile::TempDir)> {
    common::nvim_bin_or_skip()?;
    let Some(tree) = common::lighthouse_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi terminal_lighthouse — WOLF_TEST_LIGHTHOUSE_DIR is unset or names \
             no readable tree, so this run is outside BOTH stagers: Bazel's runfiles and the \
             Nix devShell's shellHook export. Outside them that is expected; INSIDE either it \
             is a bug, and the terminal palette just went unproven. No-op pass.\n"
        );
        return None;
    };
    let lighthouse = Lighthouse::load(&tree);
    let tmp = tempfile::tempdir().expect("failed to create the staging dir");
    let root = tmp.path();
    let init = root.join("init.lua");
    std::fs::copy(common::init_lua_path(), &init).expect("failed to copy init.lua");
    let inner = tree.join("lighthouse");
    let src = if inner.is_dir() { inner } else { tree };
    common::copy_dir_recursive(&src, &root.join("lighthouse"))
        .expect("failed to copy the lighthouse tree");
    assert!(
        root.join(Path::new("lighthouse/colors/lighthouse.lua"))
            .is_file(),
        "precondition: the staged sibling colorscheme must exist under {}",
        root.display()
    );

    let handle = wolf_ffi_start(
        init.to_string_lossy().into_owned(),
        root.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    let deadline = Instant::now() + TIMEOUT;
    while handle.read_frame().cols() == 0 {
        assert!(
            Instant::now() < deadline,
            "timed out after {TIMEOUT:?} waiting for the core to attach (read_frame cols > 0)"
        );
        sleep(POLL_INTERVAL);
    }
    let id = handle.open_terminal(0, TARGET_WINDOW, colour_script());
    assert_ne!(
        id, 0,
        "open_terminal returned 0: the pty or the shell could not be started"
    );
    Some((handle, id, lighthouse, tmp))
}

/// With the vendored Lighthouse active on the dark background, the session's 16 ANSI colours
/// are Lighthouse's dark `g:terminal_color_0` to `_15`.
#[test]
fn a_session_takes_lighthouses_dark_palette() {
    let Some((handle, id, lighthouse, _tmp)) = start() else {
        return;
    };
    assert_palette(&handle, id, &lighthouse, Background::Dark, "at startup");
    handle.shutdown();
}

/// After `set_background(false)`, an open session's 16 ANSI colours become Lighthouse's light
/// values.
#[test]
fn a_background_switch_gives_an_open_session_lighthouses_light_palette() {
    let Some((handle, id, lighthouse, _tmp)) = start() else {
        return;
    };
    assert_palette(
        &handle,
        id,
        &lighthouse,
        Background::Dark,
        "precondition, before the switch",
    );
    handle.set_background(false);
    assert_palette(
        &handle,
        id,
        &lighthouse,
        Background::Light,
        "after set_background(false)",
    );
    handle.shutdown();
}
