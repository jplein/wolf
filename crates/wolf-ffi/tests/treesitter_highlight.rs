//! Integration test for **treesitter highlighting + fenced-code injection** in the
//! captive `runtime/nvim/init.lua`.
//!
//! This drives a REAL, hermetic Neovim — NO mocks, per the project testing policy
//! — and proves that the captive `init.lua`'s conditional treesitter block actually
//! makes markdown highlighting ACTIVE and that a fenced ```` ```go ```` block routes
//! through the embedded (injected) go parser. It uses the REAL captive `init.lua`
//! UNMODIFIED (resolved via `common::init_lua_path`), STAGED next to its vendored
//! siblings — the block self-locates those siblings relative to its own directory
//! (`debug.getinfo`), so a bare pointer at the staged trees would not exercise the
//! real code path. We therefore reconstruct the bundle layout in a temp dir T.
//!
//! ## What this proves (two facts, one sentinel)
//!
//! After opening a markdown buffer (which fires the captive `FileType=markdown`
//! autocmd → `vim.treesitter.start`), the sentinel records:
//!   line 1: `vim.treesitter.highlighter.active[buf] ~= nil` — highlighting is
//!           genuinely ACTIVE on the markdown buffer (not merely on rtp).
//!   line 2: after forcing a full parse (`p:parse(true)`), the top-level markdown
//!           langtree has an INJECTED `go` child — proving nvim-treesitter's
//!           `queries/markdown/injections.scm` on rtp routes the fenced block to the
//!           embedded go parser. We prefer langtree presence over specific
//!           highlight-group names (which depend on the colorscheme's @capture links).
//!
//! ## Why the sibling staging (and the two vendored trees)
//!
//! The captive block GATES on a sibling `nvim-treesitter/` dir (the queries
//! provider) existing next to `init.lua`; when present it prepends `nvim-treesitter/`
//! (queries) and `parsers/` (compiled `<lang>.so`) to `runtimepath`. So we stage
//! BOTH: copy the staged `init.lua` → `T/init.lua`, recursively copy the staged
//! nvim-treesitter tree → `T/nvim-treesitter` and the staged parsers tree →
//! `T/parsers`, then launch nvim with `init_lua = T/init.lua`. On any path that does
//! NOT stage these siblings (Linux/cargo, and every existing render/grid test), the
//! captive block is a strict no-op — `syntax on` stays the highlight source.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings this test needs a pinned nvim (`nvim_bin_or_skip`); it ALSO
//! needs BOTH staged trees (`nvim_treesitter_dir_or_none`, `parsers_dir_or_none`).
//! When any is absent — e.g. the cargo / Linux path, where the vendored trees and
//! compiled parsers are not built — the test is a loud no-op pass, so it only
//! ASSERTS when there is a real treesitter layout to exercise (Bazel/macOS).

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus opening the markdown
/// buffer, parsing, and flushing the sentinel file. Mirrors the sibling tests'
/// `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for attach / the sentinel file. Mirrors the sibling
/// tests' `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

/// The two sentinel lines the `:lua` command writes: `[highlighter_active,
/// injected_go_langtree]`.
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
fn markdown_treesitter_highlight_and_go_injection_active() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    // SKIP when the vendored nvim-treesitter tree (the gate + queries provider) is
    // not staged (cargo / Linux path): the captive treesitter block is a strict
    // no-op without it, so this test asserts only when there is a real tree to stage.
    let Some(nt_dir) = common::nvim_treesitter_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi treesitter_highlight — WOLF_TEST_NVIM_TREESITTER_DIR not staged \
             (vendored nvim-treesitter queries are Bazel/macOS-only). No-op pass.\n"
        );
        return;
    };

    // SKIP too when the compiled parsers tree is not staged: injection needs a real
    // compiled go `.so` on rtp, which only exists on the Bazel/macOS bundle path.
    let Some(parsers_dir) = common::parsers_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi treesitter_highlight — WOLF_TEST_PARSERS_DIR not staged \
             (compiled treesitter parsers are Bazel/macOS-only). No-op pass.\n"
        );
        return;
    };

    // Stage the bundle layout in a temp dir T (nvim's working dir + search root,
    // which must outlive the session; kept alive until after `shutdown()`).
    let tmp = tempfile::tempdir().expect("failed to create temp staging dir");
    let root = tmp.path();

    // init.lua next to sibling nvim-treesitter/ + parsers/ — the exact
    // Contents/Resources/ layout the captive block self-locates and gates on.
    let staged_init = common::init_lua_path();
    let init_dst = root.join("init.lua");
    std::fs::copy(&staged_init, &init_dst).expect("failed to copy staged init.lua into temp dir");

    // The staged nvim-treesitter TreeArtifact's inner content is `nvim-treesitter/`;
    // be defensive in case the resolved dir IS already that inner dir.
    let nt_src = {
        let inner = nt_dir.join("nvim-treesitter");
        if inner.is_dir() {
            inner
        } else {
            nt_dir.clone()
        }
    };
    let nt_dst = root.join("nvim-treesitter");
    common::copy_dir_recursive(&nt_src, &nt_dst)
        .expect("failed to recursively copy the nvim-treesitter tree into the temp dir");

    // The staged parsers TreeArtifact's inner content is `parsers/` (holding
    // `parser/<lang>.so`); be defensive in case the resolved dir IS that inner dir.
    let parsers_src = {
        let inner = parsers_dir.join("parsers");
        if inner.is_dir() {
            inner
        } else {
            parsers_dir.clone()
        }
    };
    let parsers_dst = root.join("parsers");
    common::copy_dir_recursive(&parsers_src, &parsers_dst)
        .expect("failed to recursively copy the parsers tree into the temp dir");

    // PRE-LAUNCH sanity: catch mis-staging early with clear messages, BEFORE we pay
    // for an nvim launch. The compiled go parser must resolve at `<rtp>/parser/go.so`
    // (rtp prepends `T/parsers`), and the markdown injections query must resolve at
    // `<rtp>/queries/markdown/injections.scm` (rtp prepends `T/nvim-treesitter`).
    let go_so = parsers_dst.join("parser").join("go.so");
    assert!(
        go_so.is_file(),
        "staged compiled go parser should exist at {go_so:?} before launch \
         (rtp resolves it as <T/parsers>/parser/go.so)"
    );
    let injections = nt_dst
        .join("queries")
        .join("markdown")
        .join("injections.scm");
    assert!(
        injections.is_file(),
        "staged markdown injections query should exist at {injections:?} before launch \
         (rtp resolves it as <T/nvim-treesitter>/queries/markdown/injections.scm)"
    );

    // Write a deterministic markdown doc with a fenced go block: heading, blank line,
    // then a ```go fence around a minimal compilable go snippet. Opening this buffer
    // fires the captive FileType=markdown autocmd → vim.treesitter.start, and the
    // injections query routes the fenced region to the embedded go parser.
    let doc_md = root.join("doc.md");
    let doc_contents = "# Heading\n\n```go\npackage main\nfunc f() {}\n```\n";
    std::fs::write(&doc_md, doc_contents).expect("failed to write the markdown doc into temp dir");

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

    // --- Open the markdown buffer to trigger FileType=markdown → treesitter.start
    // CONTRACT: this path (and the sentinel path below) is a tempdir path, so it
    // contains no spaces or double-quotes — both are interpolated VERBATIM into the
    // typed Ex command / double-quoted Lua string, exactly as the orlando/lighthouse
    // sibling tests document and rely on.
    let doc_md_str = doc_md.to_string_lossy().into_owned();
    send_ex_command(&handle, &format!(":e {doc_md_str}"));

    // --- Ask nvim to write a sentinel proving highlighting is active + injection ---
    // No exec_lua accessor exists on the FFI surface, so drive a real `:lua` Ex
    // command (through the same send_key path Swift uses) that writes two markers.
    // Kept on `;`-separated statements so the whole thing is a single `:lua ...`
    // line, exactly like treesitter_parser_abi.rs. The `writefile({...})` table
    // braces are doubled for Rust `format!`; every other token uses `[...]`/`(...)`.
    //
    // line 1: highlighter is ACTIVE on the current (markdown) buffer.
    // line 2: after forcing a full parse (`p:parse(true)`, which resolves injections
    //         synchronously), the top-level langtree has an injected `go` child.
    //         PRIMARY check is `children()["go"]`; because `:children()` keying could
    //         vary, we ALSO iterate the children and match `child:lang() == "go"`.
    let sentinel = root.join("treesitter_highlight_proof.txt");
    let sentinel_str = sentinel.to_string_lossy().into_owned();
    let lua_cmd = format!(
        ":lua local b=vim.api.nvim_get_current_buf(); \
         local l1=(vim.treesitter.highlighter.active[b]~=nil) and \"OK\" or \"NO\"; \
         local l2=\"NO\"; \
         local ok,p=pcall(vim.treesitter.get_parser, b); \
         if ok and p then pcall(function() p:parse(true); local kids=p:children(); \
         if kids[\"go\"]~=nil then l2=\"OK\" \
         else for _,child in pairs(kids) do if child:lang()==\"go\" then l2=\"OK\" end end end end) end; \
         vim.fn.writefile({{l1, l2}}, \"{sentinel_str}\")"
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
        "nvim should have written the treesitter-highlight sentinel with \
         {EXPECTED_SENTINEL_LINES} lines within the timeout (got {contents:?})"
    );

    let mut lines = contents.lines();
    let highlighter_active = lines.next().unwrap_or("");
    let go_injected = lines.next().unwrap_or("");
    assert_eq!(
        highlighter_active, MARK_OK,
        "treesitter highlighting should be ACTIVE on the markdown buffer \
         (vim.treesitter.highlighter.active[buf] ~= nil) after the captive \
         FileType=markdown autocmd runs vim.treesitter.start (sentinel line 1 = \
         {highlighter_active:?}); NO here means start() did not attach — see the \
         query-loader compat risk in the plan"
    );
    assert_eq!(
        go_injected, MARK_OK,
        "the fenced ```go``` block should produce an INJECTED go langtree under the \
         markdown parser (nvim-treesitter markdown injections.scm on rtp routing to \
         the embedded go parser) after a full parse (sentinel line 2 = {go_injected:?})"
    );

    // --- Shutdown: clean teardown -------------------------------------------
    handle.shutdown();
    drop(tmp);
}
