//! Integration test for the vendored **compiled treesitter parsers** (the
//! `//bazel/treesitter-parsers:parsers_tree` TreeArtifact).
//!
//! This drives a REAL, hermetic Neovim — NO mocks, per the project testing policy
//! — and proves that a host-compiled treesitter parser `.so` shipped in the bundle
//! is ABI-COMPATIBLE with the pinned Neovim 0.12.3: nvim can `add` the language
//! from the compiled `.so` and then actually PARSE source with it, producing a
//! well-formed tree.
//!
//! ## Scope guard — this does NOT enable highlighting
//!
//! This test loads a parser by ABSOLUTE PATH via `vim.treesitter.language.add`
//! and parses a string in-memory. It does NOT call `vim.treesitter.start`, does
//! NOT wire runtimepath for highlighting, and — CRUCIALLY — uses the REAL captive
//! `runtime/nvim/init.lua` UNCHANGED (resolved via `common::init_lua_path`). It
//! stages NO sibling bundle layout and MODIFIES NO init.lua. Enablement has since
//! landed on macOS (proven separately by `//crates/wolf-ffi:treesitter_highlight`);
//! this test's scope is unchanged — it only proves compiled-parser ↔ pinned-nvim
//! ABI compatibility.
//!
//! ## Coder wiring (BUILD.bazel — NOT done here)
//!
//! This file does NOT touch BUILD files. The coder adds a
//! `//crates/wolf-ffi:treesitter_parser_abi` `rust_test` modeled on `orlando_load`
//! (`srcs = ["tests/common/mod.rs", "tests/treesitter_parser_abi.rs"]`,
//! `crate_root = "tests/treesitter_parser_abi.rs"`, `tags = ["no-sandbox"]`,
//! `size = "small"`), and MUST wire, alongside the existing WOLF_TEST_INIT_LUA /
//! WOLF_TEST_NVIM_DIR:
//!   * `data`: add `"//bazel/treesitter-parsers:parsers_tree"`
//!   * `env`:  add `"WOLF_TEST_PARSERS_DIR": "$(rootpath //bazel/treesitter-parsers:parsers_tree)"`
//!     `tempfile` is already a `[dev-dependencies]` of the crate (used by `panes`).
//!
//! ## How this proves ABI compat (no exec_lua seam on the FFI surface)
//!
//! The `wolf-ffi` bridge exposes no Lua-eval accessor (only `send_key` etc.), so —
//! staying within the existing surface and the real-nvim policy — we make nvim
//! WRITE a sentinel file via a typed `:lua vim.fn.writefile(...)` Ex command and
//! read it back in Rust. The sentinel records two facts:
//!   line 1: `pcall(vim.treesitter.language.add, 'go', {path=<abs .so>})` succeeds
//!   line 2: parsing `"package main\nfunc f() {}"` with `get_string_parser` yields
//!           a root of type `source_file` with `not root:has_error()`
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings this test needs a pinned nvim (`nvim_bin_or_skip`); it ALSO
//! needs the staged parsers tree (`parsers_dir_or_none`) and the go parser `.so`
//! within it. When any is absent — e.g. the cargo / Linux path, where the compiled
//! parsers are not built — the test is a loud no-op pass, so it only ASSERTS when
//! there is a real compiled parser to load (Bazel/macOS).

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus loading + parsing and
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

/// The two sentinel lines the `:lua` command writes: `[language.add, parse]`.
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
fn compiled_go_parser_is_abi_compatible_with_pinned_nvim() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    // SKIP too when the compiled parsers tree is not staged (cargo / Linux path):
    // the compiled parsers only exist on the Bazel/macOS bundle path, so this test
    // asserts only when there is a real `.so` to load.
    let Some(parsers_dir) = common::parsers_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi treesitter_parser_abi — WOLF_TEST_PARSERS_DIR not staged \
             (compiled treesitter parsers are Bazel/macOS-only). No-op pass.\n"
        );
        return;
    };

    // The staged TreeArtifact's inner content is `parsers/parser/<lang>.so`; be
    // defensive in case the resolved dir IS already that inner `parsers/` dir.
    let go_parser = {
        let nested = parsers_dir.join("parsers").join("parser").join("go.so");
        let flat = parsers_dir.join("parser").join("go.so");
        if nested.is_file() {
            nested
        } else if flat.is_file() {
            flat
        } else {
            eprintln!(
                "\nSKIP: wolf-ffi treesitter_parser_abi — no go.so under {parsers_dir:?} \
                 (looked for parsers/parser/go.so and parser/go.so). No-op pass.\n"
            );
            return;
        }
    };

    // Use a temp dir as the core cwd AND for the sentinel. CONTRACT: this path
    // (and the resolved `.so` path) must contain no spaces or double-quotes,
    // because both are interpolated VERBATIM into the double-quoted Lua string
    // typed below — a tempdir / runfiles path satisfies this, exactly as the
    // orlando/lighthouse sibling tests document and rely on.
    let tmp = tempfile::tempdir().expect("failed to create temp staging dir");
    let root = tmp.path();

    // The REAL captive init.lua, UNCHANGED — no sibling staging, no edit.
    let init_lua = common::init_lua_path();

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
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

    // --- Ask nvim to load the compiled go parser and parse with it ----------
    // No exec_lua accessor exists on the FFI surface, so drive a real `:lua`
    // Ex command (through the same send_key path Swift uses) that writes two
    // markers. Note the Lua string literal `"package main\nfunc f() {}"`: the
    // `\n` is typed as a backslash-n pair (Rust `\\n`) so Lua — not Rust — turns
    // it into a real newline. `{path=...}` / `func f() {}` braces are doubled for
    // `format!`. Both interpolated paths are space/quote-free (see CONTRACT above).
    let parser_str = go_parser.to_string_lossy().into_owned();
    let sentinel = root.join("parser_abi_proof.txt");
    let sentinel_str = sentinel.to_string_lossy().into_owned();
    let lua_cmd = format!(
        ":lua local ok1=select(1, pcall(vim.treesitter.language.add,'go',{{path=\"{parser_str}\"}})); \
         local ok2=false; \
         local s,p=pcall(vim.treesitter.get_string_parser,\"package main\\nfunc f() {{}}\",\"go\"); \
         if s and p then local t=p:parse(); local root=t[1]:root(); \
         ok2=(root:type()==\"source_file\") and (not root:has_error()) end; \
         vim.fn.writefile({{ok1 and \"OK\" or \"NO\", ok2 and \"OK\" or \"NO\"}}, \"{sentinel_str}\")"
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
        "nvim should have written the parser-abi sentinel with \
         {EXPECTED_SENTINEL_LINES} lines within the timeout (got {contents:?})"
    );

    let mut lines = contents.lines();
    let added = lines.next().unwrap_or("");
    let parsed = lines.next().unwrap_or("");
    assert_eq!(
        added, MARK_OK,
        "vim.treesitter.language.add('go', {{path=<compiled .so>}}) should succeed, \
         proving the shipped parser is a loadable, ABI-compatible module for pinned \
         nvim 0.12.3 (sentinel line 1 = {added:?})"
    );
    assert_eq!(
        parsed, MARK_OK,
        "parsing 'package main\\nfunc f() {{}}' with the compiled go parser should \
         yield a `source_file` root with no errors, proving the parser ABI actually \
         works end-to-end (sentinel line 2 = {parsed:?})"
    );

    // --- Shutdown: clean teardown -------------------------------------------
    handle.shutdown();
    drop(tmp);
}
