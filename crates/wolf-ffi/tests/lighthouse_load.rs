//! Integration test for the vendored **lighthouse** Neovim colorscheme.
//!
//! This drives a REAL, hermetic Neovim — NO mocks, per the project testing policy
//! — and proves that the captive `runtime/nvim/init.lua` actually finds a sibling
//! `lighthouse/` colorscheme tree, adds it to `runtimepath`, and — crucially —
//! ACTIVATES it. Unlike a plugin, a colorscheme on `runtimepath` alone does
//! nothing: `init.lua` must ALSO set `vim.o.background = "dark"` and run
//! `:colorscheme lighthouse`. This test proves the scheme is genuinely ACTIVE
//! (`vim.g.colors_name == "lighthouse"`, `vim.o.background == "dark"`), not merely
//! present on the runtimepath.
//!
//! It ALSO proves STARTUP theme pickup: the pinned scheme defines the
//! `WolfCursor` / `WolfTitlebar` groups, and because activation happens before
//! core installs the `wolf_theme` tracking helper over RPC, the helper's
//! emit-once must still surface the themed titlebar (`theme_titlebar_bg()` /
//! `theme_titlebar_fg()`) and cursor (`cursor_fill()` / `cursor_glyph()`) colors.
//!
//! ## Coder wiring (BUILD.bazel — NOT done here)
//!
//! This file does NOT touch BUILD files. The coder adds a `//crates/wolf-ffi:lighthouse_load`
//! `rust_test` modeled on `orlando_load`
//! (`srcs = ["tests/common/mod.rs", "tests/lighthouse_load.rs"]`,
//! `crate_root = "tests/lighthouse_load.rs"`), and MUST wire, alongside the existing
//! WOLF_TEST_INIT_LUA / WOLF_TEST_NVIM_DIR:
//!   * `data`: add `"//bazel/lighthouse:lighthouse_tree"`
//!   * `env`:  add `"WOLF_TEST_LIGHTHOUSE_DIR": "$(rootpath //bazel/lighthouse:lighthouse_tree)"`
//! `tempfile` is already a `[dev-dependencies]` of the crate (used by `panes`).
//!
//! ## How this proves activation (no exec_lua seam on the FFI surface)
//!
//! The `wolf-ffi` bridge exposes no Lua-eval accessor (only `send_key` etc.), so —
//! staying within the existing surface and the real-nvim policy — we make nvim
//! WRITE a sentinel file via a typed `:lua vim.fn.writefile(...)` Ex command and
//! read it back in Rust. The sentinel records the two facts activation guarantees:
//! `vim.g.colors_name == "lighthouse"` and `vim.o.background == "dark"`.
//!
//! ## Why the sibling staging
//!
//! `init.lua` self-locates the colorscheme relative to ITS OWN directory
//! (`debug.getinfo(1,"S").source`), so a bare pointer at the staged tree would not
//! exercise the real code path. We therefore reconstruct the bundle layout in a
//! temp dir T: copy the staged `init.lua` to `T/init.lua` and recursively copy the
//! staged `<lighthouse_tree>/lighthouse` to `T/lighthouse`, then launch nvim with
//! `init_lua = T/init.lua`.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings this test needs a pinned nvim (`nvim_bin_or_skip`); it ALSO
//! needs the staged lighthouse tree (`lighthouse_dir_or_none`). When either is
//! absent — e.g. the cargo / Linux path, where lighthouse is not built — the test
//! is a loud no-op pass, so it only ASSERTS when the tree is actually staged
//! (Bazel/macOS).

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus activating lighthouse
/// and flushing the sentinel file. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for attach / the sentinel file. Mirrors the sibling
/// tests' `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

/// The two sentinel lines the `:lua` command writes: `[colors_name, background]`.
const EXPECTED_SENTINEL_LINES: usize = 2;
/// The marker the `:lua` command writes for a satisfied condition.
const MARK_OK: &str = "OK";

// -- Pinned WolfCursor / WolfTitlebar values (dark variant) --------------------
//
// These constants are the literal `guifg`/`guibg` values the vendored lighthouse
// colorscheme defines for the two Wolf highlight groups at the pinned commit
// 9da2bba190305b2c8d5d300c254a48c487dd4a1e (dark variant — init.lua sets
// `background=dark`). If a lighthouse bump changes them, update these pins.
/// `WolfTitlebar guibg=#1b1b1b` at the pinned commit (dark).
const LIGHTHOUSE_TITLEBAR_BG: u32 = 0x001B_1B1B;
/// `WolfTitlebar guifg=#f1f1f1` at the pinned commit (dark).
const LIGHTHOUSE_TITLEBAR_FG: u32 = 0x00F1_F1F1;
/// `WolfCursor guibg=#009fce` at the pinned commit (dark) — the cursor FILL.
const LIGHTHOUSE_CURSOR_FILL: u32 = 0x0000_9FCE;
/// `WolfCursor guifg=#000000` at the pinned commit (dark) — the cursor GLYPH.
const LIGHTHOUSE_CURSOR_GLYPH: u32 = 0x0000_0000;

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
fn lighthouse_colorscheme_active_from_sibling_dir() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    // SKIP too when the vendored lighthouse tree is not staged (cargo / Linux
    // path): the activation branch only exists on the Bazel/macOS bundle path, so
    // this test asserts only when there is a real tree to stage.
    let Some(lighthouse_tree) = common::lighthouse_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi lighthouse_load — WOLF_TEST_LIGHTHOUSE_DIR not staged \
             (vendored lighthouse colorscheme is Bazel/macOS-only). No-op pass.\n"
        );
        return;
    };

    // The staged TreeArtifact's inner content is `lighthouse/`; be defensive in
    // case the resolved dir IS already that inner dir.
    let lighthouse_src = {
        let inner = lighthouse_tree.join("lighthouse");
        if inner.is_dir() {
            inner
        } else {
            lighthouse_tree.clone()
        }
    };

    // Stage the bundle layout in a temp dir T (its `<cwd>/.wolf/` MRU cache must
    // outlive the session; kept alive until after `shutdown()`).
    let tmp = tempfile::tempdir().expect("failed to create temp staging dir");
    let root = tmp.path();

    // init.lua next to a sibling lighthouse/ — the exact Contents/Resources/ layout.
    let staged_init = common::init_lua_path();
    let init_dst = root.join("init.lua");
    std::fs::copy(&staged_init, &init_dst).expect("failed to copy staged init.lua into temp dir");

    let lighthouse_dst = root.join("lighthouse");
    common::copy_dir_recursive(&lighthouse_src, &lighthouse_dst)
        .expect("failed to recursively copy the lighthouse tree into the temp dir");

    // Sanity: the sibling colorscheme file init.lua activates must be present after
    // copy (the analog of orlando's plugin/orlando.lua check).
    let colors_file = lighthouse_dst.join("colors").join("lighthouse.lua");
    assert!(
        colors_file.is_file(),
        "staged sibling colorscheme file should exist at {colors_file:?} before launch"
    );

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_dst.to_string_lossy().into_owned(),
        root.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
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

    // --- Ask nvim to write a sentinel proving lighthouse is active ----------
    // No exec_lua accessor exists on the FFI surface, so drive a real `:lua`
    // Ex command (through the same send_key path Swift uses) that writes two
    // markers: whether `vim.g.colors_name == "lighthouse"` (activation ran) and
    // whether `vim.o.background == "dark"` (dark variant selected). The path has
    // no spaces/quotes (a tempdir path), so typing it verbatim inside a Lua
    // double-quoted string is safe.
    let sentinel = root.join("lighthouse_proof.txt");
    let sentinel_str = sentinel.to_string_lossy().into_owned();
    let lua_cmd = format!(
        ":lua vim.fn.writefile({{(vim.g.colors_name == \"lighthouse\" and \"OK\" or \"NO\"), \
         (vim.o.background == \"dark\" and \"OK\" or \"NO\")}}, \"{sentinel_str}\")"
    );
    send_ex_command(&handle, &lua_cmd);

    // --- Poll for the sentinel file, then assert both markers ---------------
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut contents = String::new();
    while Instant::now() < deadline {
        if let Ok(s) = std::fs::read_to_string(&sentinel) {
            if s.lines().count() >= EXPECTED_SENTINEL_LINES {
                contents = s;
                break;
            }
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        contents.lines().count() >= EXPECTED_SENTINEL_LINES,
        "nvim should have written the lighthouse sentinel with \
         {EXPECTED_SENTINEL_LINES} lines within the timeout (got {contents:?})"
    );

    let mut lines = contents.lines();
    let colors_name = lines.next().unwrap_or("");
    let background = lines.next().unwrap_or("");
    assert_eq!(
        colors_name, MARK_OK,
        "colorscheme lighthouse should be active (vim.g.colors_name == \"lighthouse\") \
         after init.lua prepends the sibling lighthouse dir and runs :colorscheme \
         (sentinel line 1 = {colors_name:?})"
    );
    assert_eq!(
        background, MARK_OK,
        "vim.o.background should be \"dark\" after init.lua selects the dark variant \
         (sentinel line 2 = {background:?})"
    );

    // --- Startup theme pickup: WolfTitlebar / WolfCursor from the scheme ----
    // lighthouse@9da2bba (dark) defines both Wolf groups (see the pinned
    // constants above). init.lua activates the colorscheme while nvim boots —
    // BEFORE core installs the theme-tracking helper over RPC — so this proves
    // the helper's emit-once picks up a colorscheme activated before setup ran.
    // The `wolf_theme` notification arrives asynchronously; poll until the
    // titlebar bg surfaces as set.
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
        "theme_titlebar_bg() should become set at startup from the vendored \
         lighthouse colorscheme's WolfTitlebar group"
    );
    assert_eq!(
        titlebar_bg.rgb, LIGHTHOUSE_TITLEBAR_BG,
        "titlebar bg should be lighthouse@9da2bba's dark WolfTitlebar guibg (#1b1b1b)"
    );

    let titlebar_fg = handle.theme_titlebar_fg();
    assert!(
        titlebar_fg.set,
        "theme_titlebar_fg() should be set once the theme notification landed"
    );
    assert_eq!(
        titlebar_fg.rgb, LIGHTHOUSE_TITLEBAR_FG,
        "titlebar fg should be lighthouse@9da2bba's dark WolfTitlebar guifg (#f1f1f1)"
    );

    // The WolfCursor override rides the EXISTING frame accessors (resolved at
    // frame-pack time from the same snapshot the poll above just observed).
    let themed_frame = handle.read_frame();
    assert_eq!(
        themed_frame.cursor_fill().rgb,
        LIGHTHOUSE_CURSOR_FILL,
        "cursor_fill() should be lighthouse@9da2bba's dark WolfCursor guibg (#009fce)"
    );
    assert_eq!(
        themed_frame.cursor_glyph().rgb,
        LIGHTHOUSE_CURSOR_GLYPH,
        "cursor_glyph() should be lighthouse@9da2bba's dark WolfCursor guifg (#000000)"
    );

    // --- Shutdown: clean teardown -------------------------------------------
    handle.shutdown();
    drop(tmp);
}
