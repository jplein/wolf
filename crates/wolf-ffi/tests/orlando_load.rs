//! Integration test for the vendored **orlando** Neovim plugin load branch.
//!
//! This drives a REAL, hermetic Neovim — NO mocks, per the project testing policy
//! — and proves that the captive `runtime/nvim/init.lua` actually finds, adds to
//! `runtimepath`, and sources a sibling `orlando/` plugin tree (the macOS bundle
//! layout `Contents/Resources/{init.lua,orlando/}`). It is the ONLY automated
//! proof of the new "orlando present" load branch (the absent-path no-op is
//! already covered, unchanged, by `bridge_smoke`).
//!
//! ## Coder wiring (BUILD.bazel — NOT done here)
//!
//! This file does NOT touch BUILD files. The coder adds a `//crates/wolf-ffi:orlando_load`
//! `rust_test` modeled on `bridge_smoke` / `panes`
//! (`srcs = ["tests/common/mod.rs", "tests/orlando_load.rs"]`,
//! `crate_root = "tests/orlando_load.rs"`, `tags = ["no-sandbox"]`, `size = "small"`),
//! and MUST wire, alongside the existing WOLF_TEST_INIT_LUA / WOLF_TEST_NVIM_DIR:
//!   * `data`: add `"//bazel/orlando:orlando_tree"`
//!   * `env`:  add `"WOLF_TEST_ORLANDO_DIR": "$(rootpath //bazel/orlando:orlando_tree)"`
//! `tempfile` is already a `[dev-dependencies]` of the crate (used by `panes`).
//!
//! ## How this proves the load (no exec_lua seam on the FFI surface)
//!
//! The `wolf-ffi` bridge exposes no Lua-eval accessor (only `send_key` etc.), so —
//! staying within the existing surface and the real-nvim policy — we make nvim
//! WRITE a sentinel file via a typed `:lua vim.fn.writefile(...)` Ex command and
//! read it back in Rust. The sentinel records two facts the plugin's load
//! guarantees: `vim.g.loaded_orlando` is truthy (set by `plugin/orlando.lua`) and
//! `require("orlando")` succeeds (the `lua/orlando/init.lua` module is on rtp).
//!
//! ## Why the sibling staging
//!
//! `init.lua` self-locates the plugin relative to ITS OWN directory
//! (`debug.getinfo(1,"S").source`), so a bare pointer at the staged tree would not
//! exercise the real code path. We therefore reconstruct the bundle layout in a
//! temp dir T: copy the staged `init.lua` to `T/init.lua` and recursively copy the
//! staged `<orlando_tree>/orlando` to `T/orlando`, then launch nvim with
//! `init_lua = T/init.lua`.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings this test needs a pinned nvim (`nvim_bin_or_skip`); it ALSO
//! needs the staged orlando tree (`orlando_dir_or_none`). When either is absent —
//! e.g. the cargo / Linux path, where orlando is not built — the test is a loud
//! no-op pass, so it only ASSERTS when the tree is actually staged (Bazel/macOS).

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus sourcing orlando and
/// flushing the sentinel file. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for attach / the sentinel file. Mirrors the sibling
/// tests' `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

/// The two sentinel lines the `:lua` command writes: `[loaded_orlando, require]`.
const EXPECTED_SENTINEL_LINES: usize = 2;
/// The marker the `:lua` command writes for a satisfied condition.
const MARK_OK: &str = "OK";

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
fn orlando_loads_from_sibling_dir() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    // SKIP too when the vendored orlando tree is not staged (cargo / Linux path):
    // the load branch only exists on the Bazel/macOS bundle path, so this test
    // asserts only when there is a real tree to stage.
    let Some(orlando_tree) = common::orlando_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi orlando_load — WOLF_TEST_ORLANDO_DIR not staged \
             (vendored orlando plugin is Bazel/macOS-only). No-op pass.\n"
        );
        return;
    };

    // The staged TreeArtifact's inner content is `orlando/`; be defensive in case
    // the resolved dir IS already that inner dir.
    let orlando_src = {
        let inner = orlando_tree.join("orlando");
        if inner.is_dir() {
            inner
        } else {
            orlando_tree.clone()
        }
    };

    // Stage the bundle layout in a temp dir T (its `<cwd>/.wolf/` MRU cache must
    // outlive the session; kept alive until after `shutdown()`).
    let tmp = tempfile::tempdir().expect("failed to create temp staging dir");
    let root = tmp.path();

    // init.lua next to a sibling orlando/ — the exact Contents/Resources/ layout.
    let staged_init = common::init_lua_path();
    let init_dst = root.join("init.lua");
    std::fs::copy(&staged_init, &init_dst).expect("failed to copy staged init.lua into temp dir");

    let orlando_dst = root.join("orlando");
    common::copy_dir_recursive(&orlando_src, &orlando_dst)
        .expect("failed to recursively copy the orlando tree into the temp dir");

    // Sanity: the sibling plugin file init.lua sources must be present after copy.
    let plugin_file = orlando_dst.join("plugin").join("orlando.lua");
    assert!(
        plugin_file.is_file(),
        "staged sibling plugin file should exist at {plugin_file:?} before launch"
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

    // --- Ask nvim to write a sentinel proving orlando loaded ----------------
    // No exec_lua accessor exists on the FFI surface, so drive a real `:lua`
    // Ex command (through the same send_key path Swift uses) that writes two
    // markers: whether `vim.g.loaded_orlando` is truthy (set by
    // plugin/orlando.lua) and whether `require("orlando")` succeeds (module on
    // rtp). The path has no spaces/quotes (a tempdir path), so typing it verbatim
    // inside a Lua double-quoted string is safe.
    let sentinel = root.join("orlando_proof.txt");
    let sentinel_str = sentinel.to_string_lossy().into_owned();
    let lua_cmd = format!(
        ":lua vim.fn.writefile({{(vim.g.loaded_orlando and \"OK\" or \"NO\"), \
         (pcall(require, \"orlando\") and \"OK\" or \"NO\")}}, \"{sentinel_str}\")"
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
        "nvim should have written the orlando sentinel with \
         {EXPECTED_SENTINEL_LINES} lines within the timeout (got {contents:?})"
    );

    let mut lines = contents.lines();
    let loaded = lines.next().unwrap_or("");
    let required = lines.next().unwrap_or("");
    assert_eq!(
        loaded, MARK_OK,
        "vim.g.loaded_orlando should be truthy after init.lua sources the sibling \
         plugin (sentinel line 1 = {loaded:?})"
    );
    assert_eq!(
        required, MARK_OK,
        "require(\"orlando\") should succeed after init.lua prepends the sibling \
         orlando dir to runtimepath (sentinel line 2 = {required:?})"
    );

    // --- Shutdown: clean teardown -------------------------------------------
    handle.shutdown();
    drop(tmp);
}
