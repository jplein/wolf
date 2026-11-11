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
//! It ALSO proves STARTUP theme pickup: the pinned scheme defines `WolfCursor`
//! and leaves `WolfTitlebar` unset, and because activation happens before core
//! installs the `wolf_theme` tracking helper over RPC, the helper's emit-once
//! must still surface the themed cursor (`cursor_fill()` / `cursor_glyph()`),
//! while the titlebar (`theme_titlebar_bg()` / `theme_titlebar_fg()`) stays
//! unset so the front-ends keep the system colour. The expected cursor colours
//! are read from the pinned `lighthouse.lua` through `common::Lighthouse`.
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
//! ## When this test SKIPs, and what a SKIP means
//!
//! Like its siblings this test needs a pinned nvim (`nvim_bin_or_skip`); it ALSO
//! needs the vendored lighthouse tree (`lighthouse_dir_or_none`). That tree is
//! staged on BOTH supported paths, so this test ASSERTS on macOS and on Linux
//! alike:
//!
//!   * Bazel stages `//bazel/lighthouse:lighthouse_tree` into the runfiles and
//!     wires `WOLF_TEST_LIGHTHOUSE_DIR` from the `rust_test` target's `env`.
//!   * `flake.nix`'s `linuxShell.shellHook` exports the same variable from the
//!     `wolfLighthouse` derivation, which builds the commit
//!     `bazel/lighthouse/extensions.bzl` pins, and
//!     `crates/wolf-gnome/tests/vendored_pins.rs` is what keeps those two pins
//!     equal.
//!
//! The SKIP branch is therefore what a run OUTSIDE both environments takes — a
//! bare `cargo test` on a host with no devShell — and it is correct behaviour
//! there. Inside either environment a SKIP is a LOST EXPORT, not the expected
//! state: it silently turns this file into a no-op pass and leaves the colorscheme
//! activation branch unproven.
//! `the_shellhook_still_stages_the_vendored_lua_trees_for_the_ffi_tests` in
//! `crates/wolf-gnome/tests/flake_exports.rs` fails if either vendored-tree export
//! is deleted from the `shellHook`.

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

/// The dark palette key Lighthouse sets `WolfCursor`'s `bg`, the cursor fill, from.
const CURSOR_FILL_KEY: &str = "cursorBG";
/// The dark palette key Lighthouse sets `WolfCursor`'s `fg`, the cursor glyph, from.
const CURSOR_GLYPH_KEY: &str = "cursorFG";

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

    // Skip without the lighthouse tree. Bazel stages it and the devShell exports it from
    // `wolfLighthouse`, so inside either, reaching this skip means the export was lost.
    let Some(lighthouse_tree) = common::lighthouse_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi lighthouse_load — WOLF_TEST_LIGHTHOUSE_DIR is unset or names \
             no readable tree, so this run is outside BOTH stagers: Bazel's runfiles and the \
             Nix devShell's shellHook export. Outside them that is expected; INSIDE either it \
             is a bug, and the colorscheme activation branch just went unproven. No-op pass.\n"
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

    // Stage the bundle layout in a temp dir T (nvim's working dir + search root,
    // which must outlive the session; kept alive until after `shutdown()`).
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

    // --- Sentinel: a typed `:lua` (no Lua-eval accessor) writes `colors_name` and `background`.
    // The tempdir path has no spaces or quotes, so it is typed verbatim into a Lua string.
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

    // --- Startup pickup: init.lua activates lighthouse before core installs the theme helper, so
    // this proves the helper's emit-once sees it. `wolf_theme` is async, so poll the cursor fill.
    let lighthouse = common::Lighthouse::load(&lighthouse_tree);
    let cursor_fill = lighthouse.colour(common::Background::Dark, CURSOR_FILL_KEY);
    let cursor_glyph = lighthouse.colour(common::Background::Dark, CURSOR_GLYPH_KEY);
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut themed_frame = handle.read_frame();
    while Instant::now() < deadline {
        themed_frame = handle.read_frame();
        if themed_frame.cursor_fill().rgb == cursor_fill {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        themed_frame.cursor_fill().rgb,
        cursor_fill,
        "cursor_fill() should become the pinned Lighthouse's dark WolfCursor bg ({CURSOR_FILL_KEY}, \
         #{cursor_fill:06x}) at startup; anything else means the helper's emit-once did not see \
         the colorscheme init.lua activated"
    );
    assert_eq!(
        themed_frame.cursor_glyph().rgb,
        cursor_glyph,
        "cursor_glyph() should be the pinned Lighthouse's dark WolfCursor fg ({CURSOR_GLYPH_KEY}, \
         #{cursor_glyph:06x})"
    );

    // The theme has landed, so an unset titlebar now is Lighthouse's, not a theme still on its way.
    let titlebar_bg = handle.theme_titlebar_bg();
    assert!(
        !titlebar_bg.set,
        "theme_titlebar_bg() must stay unset, since the pinned Lighthouse leaves WolfTitlebar \
         unset so the titlebar keeps the system colour; set to #{:06x} means the pin defines \
         WolfTitlebar (a Lighthouse older than 52f49c3) or core invents a titlebar colour",
        titlebar_bg.rgb
    );
    let titlebar_fg = handle.theme_titlebar_fg();
    assert!(
        !titlebar_fg.set,
        "theme_titlebar_fg() must stay unset, since the pinned Lighthouse leaves WolfTitlebar \
         unset; set to #{:06x} means the pin defines WolfTitlebar or core invents one",
        titlebar_fg.rgb
    );

    // --- Shutdown: clean teardown -------------------------------------------
    handle.shutdown();
    drop(tmp);
}
