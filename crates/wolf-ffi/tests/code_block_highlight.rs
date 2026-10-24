//! Definitive real-nvim integration test for **treesitter syntax highlighting of
//! fenced code blocks in the Markdown preview** (task #4).
//!
//! This drives a REAL, hermetic Neovim — NO mocks, per the project testing policy —
//! and proves the END-TO-END highlight path: opening a markdown doc with a fenced
//! ```` ```go ```` block and pulling `render_preview` yields inline-styled `<span>`s
//! with resolved colors for the code tokens, while an UNKNOWN-language fence falls
//! back to plain `<pre><code>` (no styled spans).
//!
//! It combines the STAGING pattern of `tests/treesitter_highlight.rs` (copy the
//! staged `init.lua` next to recursively-copied `nvim-treesitter/` + `parsers/`
//! siblings in a temp dir T, with pre-launch `go.so` + `injections.scm` sanity
//! asserts) with the `render_preview` DRIVING pattern of `tests/preview.rs`
//! (`open_preview` → wait for the preview window → `previews()` → `source_bufnr()` →
//! `render_preview`).
//!
//! ## SKIP-when-unset policy
//!
//! Like `treesitter_highlight.rs`, this test needs a pinned nvim
//! (`nvim_bin_or_skip`) AND both trees (`nvim_treesitter_dir_or_none`,
//! `parsers_dir_or_none`). Each resolves from either provider — the Bazel-staged
//! `WOLF_TEST_*` variables first, then the Linux devShell's exports — so this test
//! asserts on both platforms and skips only on a host carrying neither.
//!
//! Skipping is correct for THIS fixture, but the reason is the FIXTURE'S LANGUAGE,
//! not a disabled highlighter: `wolf_highlight_code_blocks` sits at MODULE level in
//! `runtime/nvim/init.lua`, outside the vendored-tree gate, so it is live wherever
//! nvim runs and gates per LANGUAGE. With no vendored trees it answers `false` here
//! because the fixture fences `go`, while the seven the pinned Neovim ships (`c`,
//! `lua`, `markdown`, `markdown_inline`, `query`, `vim`, `vimdoc`) still highlight.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, FfiPreview, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (filetype detection, treesitter parse, the split + `nvim_win_set_buf`, the
/// out-of-band preview notifications) to settle. Mirrors the sibling tests.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A short settle between window-cycling `:wincmd w` attempts, long enough for the
/// `WinEnter`/`BufEnter` autocmd to re-emit `wolf_current_file`. Mirrors `preview.rs`.
const CYCLE_SETTLE: Duration = Duration::from_millis(150);

/// The lowest real WINDOW grid id (grid 1 is the outer/global grid; editor windows
/// are `>= 2` under `ext_multigrid`). Mirrors `preview.rs`/`pane_status.rs`.
const WINDOW_GRID_MIN: u64 = 2;

/// The two-pane window count reached after a typed `:vsplit`.
const TWO_PANES: usize = 2;

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false. Same as the sibling tests.
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

/// Types `cmd` as an Ex command and presses Enter, through the same
/// `send_key`/`FfiKeyEvent` path the generated Swift glue uses. `cmd` must include
/// the leading `:`. Copy of the siblings' `send_ex_command`.
fn send_ex_command(handle: &WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// The live editor WINDOW grids (grids `>= WINDOW_GRID_MIN` that are not hidden).
/// Mirrors `preview.rs::live_window_count`.
fn live_window_count(handle: &WolfCoreHandle) -> usize {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .count()
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the attached active frame reports a
/// non-empty grid (`cols > 0`), the sibling tests' attach handshake.
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

/// Blocks until `handle.is_markdown_active()` equals `target`, then asserts it.
/// Mirrors `preview.rs`.
fn wait_for_markdown_active(handle: &WolfCoreHandle, target: bool, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut got = handle.is_markdown_active();
    while Instant::now() < deadline {
        got = handle.is_markdown_active();
        if got == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        got, target,
        "is_markdown_active() should be {target} ({desc})"
    );
}

/// Blocks until exactly `n` live editor window grids exist, then asserts it.
fn wait_for_window_count(handle: &WolfCoreHandle, n: usize, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut count = live_window_count(handle);
    while Instant::now() < deadline {
        count = live_window_count(handle);
        if count == n {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(count, n, "there should be {n} live window grids ({desc})");
}

/// Blocks until `predicate(&previews)` holds on a fresh `previews()` snapshot,
/// returning that snapshot. Panics with `desc` on timeout. Mirrors `preview.rs`.
fn wait_for_previews(
    handle: &WolfCoreHandle,
    desc: &str,
    predicate: impl Fn(&[FfiPreview]) -> bool,
) -> Vec<FfiPreview> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let previews = handle.previews();
        if predicate(&previews) {
            return previews;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for previews() ({desc})");
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the core's `current_file()` ends with
/// `suffix` (a BASENAME match), deterministically confirming that file is the ACTIVE
/// buffer. Mirrors `preview.rs`.
fn wait_for_current_file(handle: &WolfCoreHandle, suffix: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut current = handle.current_file();
    while Instant::now() < deadline {
        current = handle.current_file();
        if current.ends_with(suffix) {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!(
        "timed out after {FLUSH_TIMEOUT:?} waiting for current_file() to end with \
         {suffix} (got {current})"
    );
}

/// Cycles editor windows with `:wincmd w` until the active window reports a markdown
/// buffer, so a subsequent edit lands on the SOURCE pane rather than the preview
/// scratch pane. Mirrors `preview.rs::focus_markdown_source`.
fn focus_markdown_source(handle: &WolfCoreHandle) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if handle.is_markdown_active() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out cycling windows to focus the markdown source pane"
        );
        send_ex_command(handle, ":wincmd w");
        sleep(CYCLE_SETTLE);
    }
}

/// The unambiguous inline-highlight signature: a color-carrying inline span. CSS
/// uses `color: #` (WITH a space) and markers use `class="wolf-src"`, so
/// `<span style="color:#` (no space) matches ONLY a treesitter-highlighted token.
const STYLED_SPAN_SIGNATURE: &str = "<span style=\"color:#";

#[test]
fn code_block_go_highlighted_and_unknown_lang_plain() {
    // --- Graceful SKIP guards (loud no-op pass), same shape as treesitter_highlight.rs
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let Some(nt_dir) = common::nvim_treesitter_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi code_block_highlight — no nvim-treesitter queries tree: neither \
             WOLF_TEST_NVIM_TREESITTER_DIR (Bazel) nor WOLF_NVIM_TREESITTER_DIR (Linux \
             devShell) resolves. No-op pass.\n"
        );
        return;
    };

    let Some(parsers_dir) = common::parsers_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi code_block_highlight — no treesitter parsers tree: neither \
             WOLF_TEST_PARSERS_DIR (Bazel) nor WOLF_PARSERS_DIR (Linux devShell) resolves. \
             No-op pass.\n"
        );
        return;
    };

    // --- Stage the bundle layout in a temp dir T (nvim's working dir + search root,
    // which must outlive the session; kept alive until after shutdown()). --------
    let tmp = tempfile::tempdir().expect("failed to create temp staging dir");
    let root = tmp.path();

    // init.lua next to sibling nvim-treesitter/ + parsers/ — the exact layout the
    // captive block self-locates and gates on.
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

    // The staged parsers TreeArtifact's inner content is `parsers/`; be defensive in
    // case the resolved dir IS that inner dir.
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
    // for an nvim launch.
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

    // --- Two fixtures: a highlightable go block, and an unknown-language fence -----
    let hl_md = root.join("hl.md");
    std::fs::write(&hl_md, "# H\n\n```go\npackage main\nfunc f() {}\n```\n")
        .expect("failed to write hl.md into temp dir");
    let plain_md = root.join("plain.md");
    std::fs::write(&plain_md, "# H\n\n```notalang\nsome text here\n```\n")
        .expect("failed to write plain.md into temp dir");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_dst.to_string_lossy().into_owned(),
        root.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach ------------------------------------------------------------
    wait_for_attach(&handle);

    // --- 2. POSITIVE: hl.md's go block highlights to inline-styled spans -------
    send_ex_command(&handle, ":e hl.md");
    wait_for_markdown_active(&handle, true, "after :e hl.md");
    // Barrier on hl.md being current: the startup `Untitled.md` is already markdown, so the
    // markdown gate returns at once and the sync `open_preview` would race the buffered `:e`.
    wait_for_current_file(&handle, "hl.md");

    // open_preview() binds the scratch into the active pane, so `:vsplit` first keeps a second pane
    // editing hl.md for step 3's `:e plain.md`.
    send_ex_command(&handle, ":vsplit");
    wait_for_window_count(&handle, TWO_PANES, "after the typed :vsplit");
    let preview_bufnr = handle.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {preview_bufnr}"
    );

    let previews = wait_for_previews(&handle, "one hl.md preview", |p| {
        p.iter().any(|row| row.source_name() == "hl.md")
    });
    let hl_bufnr = previews
        .iter()
        .find(|row| row.source_name() == "hl.md")
        .expect("a preview row for hl.md must exist")
        .source_bufnr();
    assert!(
        hl_bufnr > 0,
        "the preview row must carry hl.md's real source bufnr (> 0), got {hl_bufnr}"
    );

    let html = handle.render_preview(hl_bufnr);
    assert!(
        html.contains(STYLED_SPAN_SIGNATURE),
        "render_preview(hl.md) must emit an inline color-styled span for the go tokens \
         (`{STYLED_SPAN_SIGNATURE}`): {html}"
    );
    assert!(
        html.contains("package"),
        "render_preview(hl.md) must still carry the `package` code token text: {html}"
    );
    assert!(
        html.contains(r#"class="language-go""#),
        "render_preview(hl.md) must keep the `class=\"language-go\"` code wrapper: {html}"
    );

    // --- 3. Negative: an unknown-language fence falls back to plain <pre><code>.
    // Wait for the gate to go false first, or `focus_markdown_source` reads a stale true.
    wait_for_markdown_active(
        &handle,
        false,
        "the in-place bind leaves the (non-markdown) preview scratch pane current",
    );
    focus_markdown_source(&handle);
    send_ex_command(&handle, ":e plain.md");
    wait_for_current_file(&handle, "plain.md");

    let plain_preview_bufnr = handle.open_preview();
    assert!(
        plain_preview_bufnr > 0,
        "open_preview() on plain.md must return a valid preview bufnr (> 0), got \
         {plain_preview_bufnr}"
    );

    let previews = wait_for_previews(&handle, "one plain.md preview", |p| {
        p.iter().any(|row| row.source_name() == "plain.md")
    });
    let plain_bufnr = previews
        .iter()
        .find(|row| row.source_name() == "plain.md")
        .expect("a preview row for plain.md must exist")
        .source_bufnr();
    assert!(
        plain_bufnr > 0,
        "the preview row must carry plain.md's real source bufnr (> 0), got {plain_bufnr}"
    );

    let plain_html = handle.render_preview(plain_bufnr);
    assert!(
        !plain_html.contains(STYLED_SPAN_SIGNATURE),
        "render_preview(plain.md) must fall back to PLAIN <pre><code> for the \
         unknown-language fence — no inline color-styled span (`{STYLED_SPAN_SIGNATURE}`): \
         {plain_html}"
    );

    // --- 4. Shutdown ----------------------------------------------------------
    handle.shutdown();
    drop(tmp);
}
