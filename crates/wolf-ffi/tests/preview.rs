//! Integration test for the `wolf-ffi` MARKDOWN-PREVIEW bridge surface: the "is
//! the active buffer markdown" gate, the IN-PLACE `open_preview()` mechanism (the
//! pane-mode model: the active pane's buffer is replaced by the preview scratch —
//! no new pane, no pane-hunting heuristic), the `previews()` snapshot of
//! `FfiPreview` rows, the blocking `render_preview(source_bufnr)` HTML pull, the
//! `preview_file(win, path)` / `preview_buffer(win, bufnr)` binds, and the buffer
//! picker's per-file union listing (the preview scratch is UNLISTED, so no
//! `Preview: <rel>` rows appear; a previewed source is made buflisted, so every
//! source file gets exactly ONE picker row whether it is edited, previewed, or
//! both).
//!
//! It ALSO covers the scroll-sync CURSOR join: `WolfCoreHandle::previews()` looks
//! each row's `source_bufnr` up in `wolf_core::Snapshot::markdown_cursors` and hands
//! the result to `FfiPreview::source_cursor_line()` / `source_cursor_col()`, with an
//! `unwrap_or_default()` producing the `line == 0` "cursor unknown" sentinel for a
//! source that has never reported one. That join is otherwise covered only by
//! compilation, so the last two tests in this file drive it end to end.
//!
//! NOTE on ground truth: `WolfCoreHandle`'s inner `WolfCore` is private, so an FFI
//! test cannot read `Snapshot::markdown_cursors` directly to compare against. The
//! ground truth used here is therefore the KEYSTROKES — an absolute, normalized
//! cursor position driven into the source buffer — which is a strictly stronger,
//! end-to-end assertion: a broken join reports the `(0, 0)` default (or another
//! buffer's position) rather than the position we just navigated to.
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is modeled on `tests/buffer_picker.rs` and
//! `tests/pane_status.rs`, reusing `tests/common/mod.rs` and the same loud
//! SKIP-when-no-nvim guard.
//!
//! ## Dependency on coder work (pane-mode plan Q2/Q4/Q6)
//!
//! The reworked surface these tests compile against, on `WolfCoreHandle`:
//!   * `open_preview(&self) -> i64` — no `mode` argument; always in place
//!   * `edit_source(&self) -> i64` — no `mode` argument; always in place
//!   * `preview_buffer(&self, win: i64, bufnr: i64) -> i64` — binds an already-open
//!     markdown BUFFER's preview into `win` (0 → current), returning the source
//!     bufnr or the neutral 0 (non-markdown / invalid bufnr / shut down)
//! plus the unlisted scratch + buflisted-source picker union in
//! `runtime/nvim/init.lua`. This file will not compile until that lands — expected.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim (`WOLF_NVIM_BIN` or the
//! Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiBufferRow, FfiKeyEvent, FfiPreview};

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (filetype detection, the in-place `nvim_win_set_buf`, the out-of-band
/// `wolf_preview` / `wolf_markdown_dirty` notifications) to settle. Mirrors the
/// sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A short settle between window-cycling `:wincmd w` attempts, long enough for the
/// `WinEnter`/`BufEnter` autocmd to re-emit `wolf_current_file` (so
/// `is_markdown_active()` reflects the newly-focused window).
const CYCLE_SETTLE: Duration = Duration::from_millis(150);

/// How long to keep re-reading `previews()` while asserting that a cursor did NOT
/// appear for a source buffer that has never reported one. Generous, so a late
/// out-of-band `wolf_markdown_cursor` cannot sneak past the NEGATIVE assertion.
const NEGATIVE_SETTLE: Duration = Duration::from_millis(1500);

/// The `line` an `FfiPreview` reports until the first `wolf_markdown_cursor` lands
/// for its source buffer: the "cursor unknown" sentinel produced by the
/// `unwrap_or_default()` in `WolfCoreHandle::previews()`'s `markdown_cursors` join.
/// The front-end reads it as "do not scroll".
const CURSOR_UNKNOWN: (u32, u32) = (0, 0);

/// A Markdown fixture whose SECOND line is a plain, non-empty paragraph, so `gg0`,
/// `j` and `l` land on predictable ABSOLUTE (line, column) pairs rather than on
/// wherever nvim happened to leave the cursor.
const CURSOR_DOC: &str = "# Alpha Heading\nalpha bravo\ncharlie\n";

/// The `(line, col)` [`CURSOR_DOC`] reports after `gg0`: the 1-based FIRST line and
/// the 0-based FIRST column.
const CURSOR_AT_START: (u32, u32) = (1, 0);

/// The `(line, col)` after one `j` from [`CURSOR_AT_START`] — down one hard line,
/// column retained.
const CURSOR_AFTER_J: (u32, u32) = (2, 0);

/// The `(line, col)` after one further `l` — right one CHARACTER, so the 0-BASED
/// column is 1 (a byte/1-based regression would report 2).
const CURSOR_AFTER_L: (u32, u32) = (2, 1);

/// A `max` large enough that the assembled buffer set is never truncated.
const RANKED_MAX: usize = 100;

/// The lowest real WINDOW grid id (grid 1 is the outer/global grid; editor windows
/// are `>= 2` under `ext_multigrid`). Mirrors `pane_status.rs`.
const WINDOW_GRID_MIN: u64 = 2;

/// The single-pane window count: the in-place preview NEVER creates a pane, so a
/// lone editor pane previewing stays a lone pane.
const ONE_PANE: usize = 1;

/// The two-pane window count reached after a typed `:vsplit`.
const TWO_PANES: usize = 2;

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;
/// The Escape key (leaves insert mode back to normal mode).
const KIND_ESC: u8 = 2;

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

/// Builds an Escape `FfiKeyEvent` (kind=Esc), all modifiers false.
fn esc_key() -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_ESC,
        ch: 0,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Types `text` into the active buffer via the insert-mode path: an `i` to enter
/// insert, then each printable character, then Escape back to normal mode so the
/// buffer change is committed and observable.
fn type_in_insert(handle: &wolf_ffi::WolfCoreHandle, text: &str) {
    handle.send_key(char_key('i'));
    for c in text.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(esc_key());
}

/// Types `cmd` as an Ex command and presses Enter, through the same
/// `send_key`/`FfiKeyEvent` path the generated Swift glue uses. `cmd` must include
/// the leading `:`. Copy of the siblings' `send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// The live editor WINDOW grids (grids `>= WINDOW_GRID_MIN` that are not hidden).
/// Mirrors `pane_status.rs::live_windows`.
fn live_window_count(handle: &wolf_ffi::WolfCoreHandle) -> usize {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .count()
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the attached active frame reports a
/// non-empty grid (`cols > 0`), the sibling tests' attach handshake.
fn wait_for_attach(handle: &wolf_ffi::WolfCoreHandle) {
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
fn wait_for_markdown_active(handle: &wolf_ffi::WolfCoreHandle, target: bool, desc: &str) {
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

/// Blocks until `handle.is_preview_active()` equals `target`, then asserts it.
///
/// This is the deterministic barrier after an in-place `open_preview()`, which
/// turns the ACTIVE pane into the preview pane. Without it, a following
/// `focus_markdown_source` can read a STALE `is_markdown_active() == true` (the
/// notification for the rebound active pane not having landed yet), return without
/// cycling, and leave subsequent motions landing in the preview pane.
fn wait_for_preview_active(handle: &wolf_ffi::WolfCoreHandle, target: bool, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut got = handle.is_preview_active();
    while Instant::now() < deadline {
        got = handle.is_preview_active();
        if got == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        got, target,
        "is_preview_active() should be {target} ({desc})"
    );
}

/// Blocks until exactly `n` live editor window grids exist, then asserts it.
fn wait_for_window_count(handle: &wolf_ffi::WolfCoreHandle, n: usize, desc: &str) {
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
/// returning that snapshot. Panics with `desc` on timeout.
fn wait_for_previews(
    handle: &wolf_ffi::WolfCoreHandle,
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

/// The `source_tick()` of the preview row whose `source_bufnr()` is `bufnr`, or
/// `None` if no such row is present in this snapshot.
fn tick_for_source(previews: &[FfiPreview], bufnr: i64) -> Option<u64> {
    previews
        .iter()
        .find(|p| p.source_bufnr() == bufnr)
        .map(FfiPreview::source_tick)
}

/// The `(source_cursor_line(), source_cursor_col())` pair of the preview row whose
/// `source_bufnr()` is `bufnr`, or `None` when no such row is present.
///
/// Looking the pair up through the SAME `source_bufnr` key that
/// `WolfCoreHandle::previews()` joins `Snapshot::markdown_cursors` on is what makes
/// the cursor assertions a test OF that join: a join keyed on anything else (the
/// preview scratch bufnr, the window handle) falls through `unwrap_or_default()` and
/// yields [`CURSOR_UNKNOWN`] instead of the position the test navigated to.
fn cursor_for_source(previews: &[FfiPreview], bufnr: i64) -> Option<(u32, u32)> {
    previews
        .iter()
        .find(|p| p.source_bufnr() == bufnr)
        .map(|p| (p.source_cursor_line(), p.source_cursor_col()))
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the core's `current_file()` ends
/// with `suffix` (a BASENAME match, robust to absolute-vs-cwd-relative display and
/// to `/var`↔`/private/var` symlinking), deterministically confirming that file is
/// the ACTIVE buffer. Unlike `is_markdown_active` — which stays true across a
/// markdown→markdown `:e` and so cannot witness the switch — this only flips once
/// the `wolf_current_file` notification for the new buffer has landed. Panics with
/// `suffix` on timeout.
fn wait_for_current_file(handle: &wolf_ffi::WolfCoreHandle, suffix: &str) {
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

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the ACTIVE frame's rendered text
/// contains `needle`. The frame is reconstructed by concatenating every resolved
/// cell's text (row-major) from `read_frame()`. This deterministically witnesses
/// that asynchronously-typed text has landed in AND rendered from the currently
/// focused buffer — a pure read of the already-flushed frame that does NOT issue an
/// RPC (so it cannot itself move focus). Panics with `needle` on timeout.
fn wait_for_frame_text(handle: &wolf_ffi::WolfCoreHandle, needle: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut text = String::new();
    while Instant::now() < deadline {
        text = handle
            .read_frame()
            .cells()
            .iter()
            .map(|c| c.text())
            .collect();
        if text.contains(needle) {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!(
        "timed out after {FLUSH_TIMEOUT:?} waiting for the active frame to render \
         {needle:?} (last frame text: {text:?})"
    );
}

/// Cycles editor windows with `:wincmd w` until the active window reports a
/// markdown buffer (`is_markdown_active()`), so a subsequent edit lands on the
/// SOURCE pane rather than the (non-markdown) preview scratch pane. With exactly
/// two panes this needs at most one cycle.
fn focus_markdown_source(handle: &wolf_ffi::WolfCoreHandle) {
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

/// A fresh buffer-picker snapshot ranked over the empty query (the picker snapshots
/// `list_buffers()` at construction, so every read constructs a NEW picker).
fn ranked_snapshot(handle: &wolf_ffi::WolfCoreHandle) -> Vec<FfiBufferRow> {
    handle.buffer_picker().ranked(String::new(), RANKED_MAX)
}

/// The `display()` strings of a ranked snapshot, for assertion messages.
fn displays(rows: &[FfiBufferRow]) -> Vec<String> {
    rows.iter().map(FfiBufferRow::display).collect()
}

/// The number of ranked rows whose `display()` ends with `suffix` (robust to
/// cwd-relative vs. absolute display and `/var`↔`/private/var` symlinking).
fn rows_ending_with(rows: &[FfiBufferRow], suffix: &str) -> usize {
    rows.iter()
        .filter(|r| r.display().ends_with(suffix))
        .count()
}

/// The `bufnr()` of the first ranked row whose `display()` ends with `suffix`.
fn bufnr_for_suffix(rows: &[FfiBufferRow], suffix: &str) -> Option<i64> {
    rows.iter()
        .find(|r| r.display().ends_with(suffix))
        .map(FfiBufferRow::bufnr)
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until `predicate(&rows)` holds on a FRESH
/// ranked picker snapshot, returning that snapshot. Panics with `desc` on timeout.
fn wait_for_ranked(
    handle: &wolf_ffi::WolfCoreHandle,
    desc: &str,
    predicate: impl Fn(&[FfiBufferRow]) -> bool,
) -> Vec<FfiBufferRow> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let rows = ranked_snapshot(handle);
        if predicate(&rows) {
            return rows;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for a ranked picker snapshot ({desc})");
}

/// Asserts the pane-mode picker-union invariant on a ranked snapshot: the preview
/// scratch is UNLISTED, so no `Preview: ` row may ever appear.
fn assert_no_preview_rows(rows: &[FfiBufferRow], desc: &str) {
    assert!(
        !rows.iter().any(|r| r.display().starts_with("Preview: ")),
        "the buffer picker must list the per-file UNION with no `Preview: ` rows \
         ({desc}): {:?}",
        displays(rows)
    );
}

#[test]
fn preview_opens_in_place_and_round_trips_with_edit() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd holding a markdown file (heading + paragraph) and a plain
    // text file (the non-markdown gate case), kept alive until after `shutdown()`.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n",
    )
    .expect("failed to write alpha.md");
    std::fs::write(tmp.path().join("notes.txt"), "plain text notes\n")
        .expect("failed to write notes.txt");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach -----------------------------------------------------------
    wait_for_attach(&handle);

    // --- 2. The "is markdown" gate follows the active buffer's filetype -------
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");

    send_ex_command(&handle, ":e notes.txt");
    wait_for_markdown_active(&handle, false, "after :e notes.txt");

    // Back on the markdown buffer for the preview.
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "back on alpha.md");
    // Real switch barrier: is_markdown_active cannot witness a markdown->markdown
    // switch, so wait until alpha.md is genuinely the ACTIVE buffer before the SYNC
    // `open_preview` races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");

    // --- 3. In-place placement: open_preview() -------------------------------
    // The single editing pane becomes the preview pane: NO new pane, same window.
    // `open_preview` returns the preview scratch bufnr (> 0 since markdown active).
    let active_win = handle.active_win();
    assert!(
        active_win > 0,
        "the editing pane must carry a real window handle (> 0), got {active_win}"
    );
    let preview_bufnr = handle.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, ONE_PANE, "in-place preview creates no pane");
    wait_for_preview_active(
        &handle,
        true,
        "the in-place preview leaves the SAME (now preview) pane current",
    );

    // --- 4. previews() surfaces one row for alpha.md, in the SAME window ------
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    assert_eq!(
        previews.len(),
        1,
        "exactly one preview row after the in-place open: {previews:?}"
    );
    let alpha_row = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist");
    assert_eq!(
        alpha_row.win(),
        active_win,
        "the in-place preview must be bound into the previously-active window"
    );
    let alpha_bufnr = alpha_row.source_bufnr();
    assert!(
        alpha_bufnr > 0,
        "the preview row must carry alpha.md's real source bufnr (> 0), got {alpha_bufnr}"
    );

    // --- 5. render_preview() returns themed HTML with the heading + markers ---
    let html = handle.render_preview(alpha_bufnr);
    assert!(
        html.contains("Alpha Heading"),
        "render_preview must contain the source heading text: {html}"
    );
    assert!(
        html.contains(r#"id="wl-1-0""#),
        "render_preview must carry the first hard line's `wl-1-0` anchor: {html}"
    );
    // The heading slug id ships through the nvim -> render_preview path (proves the
    // `HeadingIdAssigner` output survives the FFI round-trip, not just the pure
    // `render_html` unit tests). `# Alpha Heading` slugs to `alpha-heading`.
    assert!(
        html.contains(r#"id="alpha-heading""#),
        "render_preview must carry the heading slug id from the nvim path: {html}"
    );

    // --- 5b. The preview row exposes the source's cwd-relative display path ---
    // `source_display_path()` feeds the preview pane's SECONDARY statusbar line,
    // mirroring editor panes. The fixture lives at the cwd ROOT, so the display
    // path ends with the bare basename. Use `ends_with`, NOT exact equality: the
    // source path is uncanonicalized while live_cwd is canonical, so a
    // `/var`<->`/private/var` mismatch can leave `source_display_path()` absolute.
    let alpha_display = alpha_row.source_display_path();
    assert!(
        !alpha_display.is_empty(),
        "the preview row must expose a non-empty source_display_path(), got {alpha_display:?}"
    );
    assert!(
        alpha_display.ends_with("alpha.md"),
        "source_display_path() must end with the source file path (alpha.md), got {alpha_display}"
    );

    // --- 6. Buffer picker: the per-file UNION, no `Preview: ` rows ------------
    // The scratch is created UNLISTED and the previewed source stays buflisted, so
    // the picker shows exactly ONE row for alpha.md and no preview-scratch row.
    let rows = wait_for_ranked(&handle, "one alpha.md row while previewing", |rows| {
        rows_ending_with(rows, "alpha.md") == 1
    });
    assert_no_preview_rows(&rows, "while alpha.md is previewed in place");
    assert_eq!(
        rows_ending_with(&rows, "alpha.md"),
        1,
        "exactly ONE picker row for the previewed source file: {:?}",
        displays(&rows)
    );

    // --- 7. edit_source() flips the pane back to the source, in place ---------
    let returned = handle.edit_source();
    assert_eq!(
        returned, alpha_bufnr,
        "edit_source() on the active preview pane must return its SOURCE bufnr"
    );
    wait_for_preview_active(
        &handle,
        false,
        "replacing the preview with its source flips is_preview_active back off",
    );
    wait_for_markdown_active(
        &handle,
        true,
        "the pane shows the markdown source again after edit_source()",
    );
    wait_for_previews(&handle, "no preview rows after the in-place edit", |p| {
        p.is_empty()
    });
    assert_eq!(
        live_window_count(&handle),
        ONE_PANE,
        "the in-place edit must NOT create a pane either",
    );
    assert_eq!(
        handle.active_win(),
        active_win,
        "the preview/edit round trip stays in the SAME window",
    );

    // --- 8. Re-opening the preview REUSES the same scratch buffer -------------
    let reused = handle.open_preview();
    assert_eq!(
        reused, preview_bufnr,
        "open_preview() after an edit/preview cycle must REUSE the same scratch \
         buffer for this source"
    );
    wait_for_previews(&handle, "the reused preview row is back", |p| {
        p.len() == 1 && p.iter().any(|row| row.source_bufnr() == alpha_bufnr)
    });

    // --- 9. Shutdown: handle calls return neutral values ---------------------
    handle.shutdown();
    assert!(
        !handle.is_markdown_active(),
        "after shutdown is_markdown_active() must be the neutral false"
    );
    assert!(
        handle.previews().is_empty(),
        "after shutdown previews() must be the neutral empty vec"
    );
    assert!(
        handle.render_preview(alpha_bufnr).is_empty(),
        "after shutdown render_preview() must be the neutral empty string"
    );
    assert_eq!(
        handle.open_preview(),
        0,
        "after shutdown open_preview() must be the neutral 0"
    );

    drop(tmp);
}

#[test]
fn preview_picker_shows_one_cwd_relative_row_for_subdir_source() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Seed the markdown source in a SUBDIRECTORY of the cwd. The picker names the
    // SOURCE buffer with its cwd-relative path (`docs/alpha.md`), and the preview
    // scratch is unlisted — this is the test that pins the union listing for a
    // relative-path source (no bare-basename duplicate, no `Preview: ` row).
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    let docs_dir = tmp.path().join("docs");
    std::fs::create_dir(&docs_dir).expect("failed to create docs subdir");
    std::fs::write(
        docs_dir.join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n",
    )
    .expect("failed to write docs/alpha.md");

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open docs/alpha.md (in the subdir) as the source ---------
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e docs/alpha.md");
    wait_for_markdown_active(&handle, true, "after :e docs/alpha.md");
    // Real switch barrier: the startup buffer is a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e docs/alpha.md`. Wait until alpha.md is genuinely the
    // ACTIVE buffer before the SYNC `open_preview` races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");

    // --- 2. In-place preview: previews() surfaces one row for alpha.md --------
    let preview_bufnr = handle.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, ONE_PANE, "in-place preview creates no pane");
    // `source_name()` is the BASENAME, so the row is found by `alpha.md` regardless
    // of the subdirectory it lives in.
    let previews = wait_for_previews(&handle, "one docs/alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let alpha_row = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for docs/alpha.md must exist");

    // --- 3. The preview row's display path is the cwd-relative subdir path ----
    // `ends_with` (not exact equality): uncanonicalized source path vs canonical
    // live_cwd can leave this absolute on a `/var`<->`/private/var` mismatch.
    let display = alpha_row.source_display_path();
    assert!(
        display.ends_with("docs/alpha.md"),
        "source_display_path() must be the cwd-relative subdir path docs/alpha.md, got {display}"
    );

    // --- 4. The picker lists ONE row for the source, and no scratch row -------
    // The previewed source keeps its cwd-relative name (`docs/alpha.md`); the
    // scratch (named `Preview: docs/alpha.md`) is unlisted and must not appear.
    let rows = wait_for_ranked(&handle, "one docs/alpha.md row while previewing", |rows| {
        rows_ending_with(rows, "docs/alpha.md") == 1
    });
    assert_no_preview_rows(&rows, "while docs/alpha.md is previewed in place");
    assert_eq!(
        rows_ending_with(&rows, "alpha.md"),
        1,
        "the subdir source must have exactly ONE picker row (its cwd-relative \
         `docs/alpha.md` entry, no bare-basename duplicate): {:?}",
        displays(&rows)
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn two_panes_previewing_the_same_source_share_one_scratch_and_one_picker_row() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n\nAnd another.\n",
    )
    .expect("failed to write alpha.md");

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open alpha.md; split into TWO panes via typed :vsplit -----
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    wait_for_current_file(&handle, "alpha.md");
    send_ex_command(&handle, ":vsplit");
    wait_for_window_count(&handle, TWO_PANES, "after the typed :vsplit");

    // --- 2. Preview alpha.md IN PLACE in the active (new split) pane ----------
    // The preview WINDOW is read back from the previews() row rather than from a
    // pre-captured `active_win()`, which can lag the typed `:vsplit`.
    let scratch_bufnr = handle.open_preview();
    assert!(
        scratch_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {scratch_bufnr}"
    );
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.len() == 1 && p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let alpha_bufnr = previews[0].source_bufnr();
    let first_preview_win = previews[0].win();
    assert!(
        first_preview_win > 0,
        "the preview row must carry a real window handle (> 0), got {first_preview_win}"
    );

    // --- 3. Live-update: editing the SOURCE advances the preview's tick -------
    // The other pane still edits alpha.md. Focus it, capture the current tick,
    // delete a line (`dd` — a normal-mode buffer change that fires TextChanged),
    // and observe the joined source_tick advance.
    wait_for_preview_active(
        &handle,
        true,
        "the in-place preview leaves the preview pane current before cycling",
    );
    focus_markdown_source(&handle);
    // `active_win()` rides the async notification path, so poll it off the
    // preview window rather than reading it once.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut source_win = handle.active_win();
    while Instant::now() < deadline {
        source_win = handle.active_win();
        if source_win > 0 && source_win != first_preview_win {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_ne!(
        source_win, first_preview_win,
        "the source pane must be the OTHER window"
    );
    let initial_tick = tick_for_source(&handle.previews(), alpha_bufnr).unwrap_or(0);

    handle.send_key(char_key('d'));
    handle.send_key(char_key('d'));

    wait_for_previews(&handle, "source_tick advances after an edit", |p| {
        tick_for_source(p, alpha_bufnr).is_some_and(|t| t > initial_tick)
    });

    // --- 4. Preview alpha.md in THIS pane too: same scratch, two windows ------
    let second_scratch = handle.open_preview();
    assert_eq!(
        second_scratch, scratch_bufnr,
        "a second pane previewing the SAME source must reuse the same scratch buffer"
    );
    let previews = wait_for_previews(&handle, "both panes preview alpha.md", |p| {
        p.len() == TWO_PANES && p.iter().all(|row| row.source_bufnr() == alpha_bufnr)
    });
    let wins: Vec<i64> = previews.iter().map(FfiPreview::win).collect();
    assert!(
        wins.contains(&first_preview_win) && wins.contains(&source_win),
        "previews() must list BOTH windows once each pane previews alpha.md: {wins:?}"
    );
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "in-place previews never create panes (still two live windows)",
    );

    // --- 5. The picker still shows exactly ONE row for the shared source ------
    let rows = wait_for_ranked(&handle, "one alpha.md row with two previews", |rows| {
        rows_ending_with(rows, "alpha.md") == 1
    });
    assert_no_preview_rows(&rows, "while two panes preview the same source");
    assert_eq!(
        rows_ending_with(&rows, "alpha.md"),
        1,
        "two panes previewing one file must still yield ONE picker row: {:?}",
        displays(&rows)
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn render_preview_renders_latex_math_and_inlines_fonts() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd holding a markdown file with BOTH inline `$...$` and display
    // `$$...$$` LaTeX math. Kept alive until after `shutdown()`.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("math.md"),
        "# Math\n\n$e=mc^2$\n\n$$\\int_0^1 x\\,dx$$\n",
    )
    .expect("failed to write math.md");

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open math.md as the source and preview it in place --------
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e math.md");
    wait_for_markdown_active(&handle, true, "after :e math.md");
    // Real switch barrier: the startup buffer is a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e math.md`. Wait until math.md is genuinely the ACTIVE
    // buffer before the SYNC `open_preview` races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "math.md");

    let preview_bufnr = handle.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, ONE_PANE, "in-place preview creates no pane");

    // --- 2. previews() surfaces one row for math.md --------------------------
    let previews = wait_for_previews(&handle, "one math.md preview", |p| {
        p.iter().any(|row| row.source_name() == "math.md")
    });
    let math_bufnr = previews
        .iter()
        .find(|row| row.source_name() == "math.md")
        .expect("a preview row for math.md must exist")
        .source_bufnr();
    assert!(
        math_bufnr > 0,
        "the preview row must carry math.md's real source bufnr (> 0), got {math_bufnr}"
    );

    // --- 3. render_preview() carries the MathML + inlined fonts through nvim ---
    // Proves the LaTeX->MathML + inlined Inter/Latin Modern Math fonts survive the
    // real nvim -> render_preview FFI round-trip, not just the pure unit tests.
    // Assertions key off the STABLE contracts, NOT exact MathML byte sequences.
    let html = handle.render_preview(math_bufnr);
    assert!(
        html.contains("<math"),
        "render_preview must render the LaTeX math as a <math> element: {html}"
    );
    assert!(
        html.contains("display=\"block\""),
        "render_preview must render the display `$$...$$` math as `display=\"block\"`: {html}"
    );
    assert!(
        html.contains("@font-face"),
        "render_preview must inline an @font-face rule: {html}"
    );
    assert!(
        html.contains("Inter"),
        "render_preview must inline the `Inter` body font: {html}"
    );
    assert!(
        html.contains("Latin Modern Math"),
        "render_preview must bundle + pin the `Latin Modern Math` font: {html}"
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn preview_file_repoints_pane_and_keeps_the_source_buflisted() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd holding alpha.md (which LINKS to beta.md at an anchor), the
    // linked beta.md, and a plain-text file (the non-markdown `preview_file` gate
    // case). Kept alive until after `shutdown()`.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\n[to beta](beta.md#beta-heading)\n",
    )
    .expect("failed to write alpha.md");
    std::fs::write(
        tmp.path().join("beta.md"),
        "# Beta Heading\n\nAnother paragraph.\n",
    )
    .expect("failed to write beta.md");
    std::fs::write(tmp.path().join("notes.txt"), "plain text notes\n")
        .expect("failed to write notes.txt");

    // Absolute paths (as WebKit would resolve a `file://` link; Lua does NO
    // relative resolution). `tmp.path()` is already absolute.
    let beta_path = tmp.path().join("beta.md").to_string_lossy().into_owned();
    let txt_path = tmp.path().join("notes.txt").to_string_lossy().into_owned();

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open alpha.md as the source and preview it in place -------
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    // Real switch barrier: the startup buffer is a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e alpha.md`. Wait until alpha.md is genuinely the ACTIVE
    // buffer before the SYNC `open_preview` races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");

    let alpha_preview_bufnr = handle.open_preview();
    assert!(
        alpha_preview_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {alpha_preview_bufnr}"
    );
    wait_for_window_count(&handle, ONE_PANE, "in-place preview creates no pane");
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let preview_win = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist")
        .win();
    assert!(
        preview_win > 0,
        "the alpha.md preview must carry a real preview WINDOW handle (> 0), got {preview_win}"
    );

    // --- 2. preview_file repoints that preview WINDOW at beta.md --------------
    // Returns beta.md's SOURCE bufnr (> 0); it must NOT open a new pane.
    let beta_bufnr = handle.preview_file(preview_win, beta_path.clone());
    assert!(
        beta_bufnr > 0,
        "preview_file(win, <abs beta.md>) must return beta.md's source bufnr (> 0), got {beta_bufnr}"
    );

    // The preview row for THIS window now reports beta.md (a re-emit replaces the
    // snapshot), and the live window count is unchanged (view-only repoint).
    wait_for_previews(&handle, "the preview window now previews beta.md", |p| {
        p.iter()
            .any(|row| row.win() == preview_win && row.source_name() == "beta.md")
    });
    assert_eq!(
        live_window_count(&handle),
        ONE_PANE,
        "preview_file must NOT create a new pane (still one live window)",
    );

    // --- 3. The link-navigated SOURCE becomes buflisted: its own picker row ---
    // beta.md was loaded in the BACKGROUND (`bufadd`/`bufload`) and never edited,
    // yet `wolf_preview_file` marks it buflisted, so the union listing gives the
    // preview-only file exactly one row — and still no `Preview: ` scratch row.
    let rows = wait_for_ranked(&handle, "a beta.md row after the link-follow", |rows| {
        rows_ending_with(rows, "beta.md") == 1
    });
    assert_no_preview_rows(&rows, "after the preview_file link-follow");
    assert_eq!(
        rows_ending_with(&rows, "beta.md"),
        1,
        "the preview-only (never-edited) source must get exactly ONE picker row: {:?}",
        displays(&rows)
    );

    // --- 4. render_preview(beta) carries beta's heading text + slug id --------
    let html = handle.render_preview(beta_bufnr);
    assert!(
        html.contains("Beta Heading"),
        "render_preview(beta) must contain the linked file's heading text: {html}"
    );
    assert!(
        html.contains(r#"id="beta-heading""#),
        "render_preview(beta) must carry beta's heading slug id: {html}"
    );

    // --- 5. Negative: a non-markdown link returns 0 and changes nothing -------
    let before = handle.previews();
    let non_md = handle.preview_file(preview_win, txt_path.clone());
    assert_eq!(
        non_md, 0,
        "preview_file(win, <abs .txt>) must return the neutral 0 (non-markdown gate)"
    );
    // The preview window still previews beta.md (the failed call left it alone).
    let after = handle.previews();
    assert!(
        after
            .iter()
            .any(|row| row.win() == preview_win && row.source_name() == "beta.md"),
        "a rejected non-markdown preview_file must leave the previews() snapshot unchanged: \
         before {:?} after {:?}",
        before
            .iter()
            .map(|r| (r.win(), r.source_name()))
            .collect::<Vec<_>>(),
        after
            .iter()
            .map(|r| (r.win(), r.source_name()))
            .collect::<Vec<_>>(),
    );

    // --- 5b. The rejected file gains NO picker row (success-gated buflisting) --
    // `wolf_preview_file` marks the source buflisted only AFTER the markdown gate
    // passes, so a rejected non-markdown path must not leak a listed buffer into
    // the picker. `preview_file` is a SYNCHRONOUS RPC, so a fresh ranked snapshot
    // taken after the 0 return is a valid negative read — had the helper listed
    // the buffer before gating, the row would already be here.
    let rows = ranked_snapshot(&handle);
    assert_eq!(
        rows_ending_with(&rows, "notes.txt"),
        0,
        "a preview_file call rejected by the markdown gate must NOT buflist the \
         file (no picker row for notes.txt): {:?}",
        displays(&rows)
    );

    // --- 6. Shutdown: preview_file returns the neutral 0 ---------------------
    handle.shutdown();
    assert_eq!(
        handle.preview_file(preview_win, beta_path.clone()),
        0,
        "after shutdown preview_file() must be the neutral 0"
    );

    drop(tmp);
}

#[test]
fn preview_buffer_binds_markdown_and_gates_non_markdown() {
    // The buffer-flavored bind behind preview-mode buffer-picker activation:
    // `preview_buffer(win, bufnr)` previews an ALREADY-OPEN buffer (no disk path
    // round-trip), returning the source bufnr — or the neutral 0 for a
    // non-markdown buffer, which is the front-end's cue to fall back to a plain
    // `switch_buffer` (the pane converts to edit mode).
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    /// The heading typed into the startup `Untitled.md` buffer (the unsaved case).
    const HEADING: &str = "# Untitled Heading";
    const HEADING_TEXT: &str = "Untitled Heading";

    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n",
    )
    .expect("failed to write alpha.md");
    std::fs::write(tmp.path().join("notes.txt"), "plain text notes\n")
        .expect("failed to write notes.txt");
    // gamma.md is only ever `:badd`-ed (buflisted but UNLOADED), never displayed
    // or `:e`-ed — the bufload-inside-preview_buffer case.
    std::fs::write(
        tmp.path().join("gamma.md"),
        "# Gamma Heading\n\nGamma paragraph text.\n",
    )
    .expect("failed to write gamma.md");

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; type a heading into the UNSAVED startup Untitled.md -------
    // The startup buffer is a named, never-written `Untitled.md` with
    // `filetype=markdown`. Type the heading while it is still focused and wait for
    // it to RENDER (a pure frame read, no RPC), so the later render assertion has
    // deterministic content.
    wait_for_attach(&handle);
    wait_for_current_file(&handle, "Untitled.md");
    type_in_insert(&handle, HEADING);
    wait_for_frame_text(&handle, HEADING_TEXT);

    let rows = wait_for_ranked(&handle, "an Untitled.md picker row", |rows| {
        bufnr_for_suffix(rows, "Untitled.md").is_some()
    });
    let untitled_bufnr =
        bufnr_for_suffix(&rows, "Untitled.md").expect("Untitled.md must have a bufnr");

    // --- 2. Open alpha.md and badd notes.txt, snapshotting their bufnrs -------
    send_ex_command(&handle, ":e alpha.md");
    wait_for_current_file(&handle, "alpha.md");
    send_ex_command(&handle, ":badd notes.txt");
    let rows = wait_for_ranked(&handle, "alpha.md and notes.txt picker rows", |rows| {
        bufnr_for_suffix(rows, "alpha.md").is_some() && bufnr_for_suffix(rows, "notes.txt").is_some()
    });
    let alpha_bufnr = bufnr_for_suffix(&rows, "alpha.md").expect("alpha.md must have a bufnr");
    let notes_bufnr = bufnr_for_suffix(&rows, "notes.txt").expect("notes.txt must have a bufnr");

    // --- 3. preview_buffer binds a markdown BUFFER into the given window ------
    let active_win = handle.active_win();
    assert!(
        active_win > 0,
        "the editing pane must carry a real window handle (> 0), got {active_win}"
    );
    let returned = handle.preview_buffer(active_win, alpha_bufnr);
    assert_eq!(
        returned, alpha_bufnr,
        "preview_buffer(win, <markdown bufnr>) must bind and return the SOURCE bufnr"
    );
    wait_for_previews(&handle, "the window previews alpha.md", |p| {
        p.iter()
            .any(|row| row.win() == active_win && row.source_bufnr() == alpha_bufnr)
    });
    assert_eq!(
        live_window_count(&handle),
        ONE_PANE,
        "preview_buffer must NOT create a new pane (still one live window)",
    );

    // --- 4. Negative: a non-markdown bufnr returns 0 and changes nothing ------
    let non_md = handle.preview_buffer(active_win, notes_bufnr);
    assert_eq!(
        non_md, 0,
        "preview_buffer(win, <non-markdown bufnr>) must return the neutral 0 — the \
         front-end's cue to fall back to switch_buffer (edit mode)"
    );
    let after = handle.previews();
    assert!(
        after
            .iter()
            .any(|row| row.win() == active_win && row.source_bufnr() == alpha_bufnr),
        "a rejected non-markdown preview_buffer must leave the window previewing \
         alpha.md: {:?}",
        after
            .iter()
            .map(|r| (r.win(), r.source_bufnr()))
            .collect::<Vec<_>>(),
    );

    // --- 5. An UNLOADED markdown buffer is bufload-ed and previews correctly --
    // `:badd gamma.md` creates a buflisted but UNLOADED buffer: it has no lines
    // in memory and no detected filetype (detection runs at load). The helper's
    // markdown gate must fall through to `vim.filetype.match` on the NAME, and
    // its `bufload` must pull the file's real contents off disk before binding —
    // otherwise the render below would carry an empty buffer.
    send_ex_command(&handle, ":badd gamma.md");
    let rows = wait_for_ranked(&handle, "a gamma.md picker row after :badd", |rows| {
        bufnr_for_suffix(rows, "gamma.md").is_some()
    });
    let gamma_bufnr = bufnr_for_suffix(&rows, "gamma.md").expect("gamma.md must have a bufnr");
    let returned = handle.preview_buffer(active_win, gamma_bufnr);
    assert_eq!(
        returned, gamma_bufnr,
        "preview_buffer(win, <unloaded markdown bufnr>) must load, bind and return \
         the SOURCE bufnr — the gate matches the NAME when no filetype is detected yet"
    );
    wait_for_previews(&handle, "the window previews gamma.md", |p| {
        p.iter()
            .any(|row| row.win() == active_win && row.source_bufnr() == gamma_bufnr)
    });
    // The render must carry gamma.md's ON-DISK contents (the `bufload` result),
    // not the empty line set an unloaded buffer would render as. The pull is
    // blocking but the bind's re-emit is async, so poll like the siblings.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut html = handle.render_preview(gamma_bufnr);
    while Instant::now() < deadline {
        html = handle.render_preview(gamma_bufnr);
        if html.contains("Gamma Heading") {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        html.contains("Gamma Heading"),
        "render_preview must contain the UNLOADED source's on-disk heading — an \
         un-bufload-ed buffer renders empty: {html}"
    );

    // --- 6. The UNSAVED Untitled.md markdown buffer previews too --------------
    // `win == 0` binds into the CURRENT window. The buffer has never been written
    // to disk, so this pins the gate on FILETYPE rather than file existence, and
    // the render must reflect the buffer's live (typed) contents.
    let returned = handle.preview_buffer(0, untitled_bufnr);
    assert_eq!(
        returned, untitled_bufnr,
        "preview_buffer(0, <unsaved Untitled.md bufnr>) must bind the CURRENT window \
         and return the source bufnr — the markdown gate is the filetype, not a file \
         on disk"
    );
    wait_for_previews(&handle, "the window previews Untitled.md", |p| {
        p.iter()
            .any(|row| row.win() == active_win && row.source_bufnr() == untitled_bufnr)
    });
    // The render reflects the source buffer's current lines; poll the blocking pull
    // until the typed heading shows up.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut html = handle.render_preview(untitled_bufnr);
    while Instant::now() < deadline {
        html = handle.render_preview(untitled_bufnr);
        if html.contains(HEADING_TEXT) {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        html.contains(HEADING_TEXT),
        "render_preview must contain the heading typed into the unsaved Untitled.md \
         buffer ({HEADING_TEXT:?}): {html}"
    );

    // --- 7. Shutdown: preview_buffer returns the neutral 0 --------------------
    handle.shutdown();
    assert_eq!(
        handle.preview_buffer(0, alpha_bufnr),
        0,
        "after shutdown preview_buffer() must be the neutral 0"
    );

    drop(tmp);
}

#[test]
fn preview_renders_untitled_buffer() {
    // The anonymous-buffer feature: a fresh session with NO markdown file on disk
    // still lands on a real, markdown-named `Untitled.md` buffer, so the in-app
    // preview gate passes and renders the buffer's live contents.
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // The un-numbered first name and the heading text we type into the buffer.
    const UNTITLED: &str = "Untitled.md";
    const HEADING: &str = "# Untitled Heading";
    const HEADING_TEXT: &str = "Untitled Heading";

    // A fresh, EMPTY temp cwd — deliberately NO markdown file to open. The startup
    // buffer must be the anonymous `Untitled.md` document. Kept alive until after
    // `shutdown()`.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach: the startup buffer is a markdown-named Untitled.md --------
    wait_for_attach(&handle);
    // `filetype=markdown` is set at naming time, so the "is markdown" gate is true
    // even though nothing was `:e`-opened.
    wait_for_markdown_active(&handle, true, "startup anonymous Untitled.md buffer");
    // And `current_file()` names it `Untitled.md` (basename `ends_with`, robust to
    // `/var` <-> `/private/var` symlinking).
    wait_for_current_file(&handle, UNTITLED);

    // --- 2. Type a heading into the (empty) Untitled.md buffer ----------------
    type_in_insert(&handle, HEADING);

    // CRITICAL ordering barrier: the heading is typed via the ASYNC key channel,
    // while `open_preview()` below is a SYNCHRONOUS RPC that binds the NOMODIFIABLE
    // preview scratch into the current window. If the sync RPC wins the race, the
    // async `i# Untitled Heading<Esc>` keys arrive AFTER the rebind — `i` fails
    // with E21, nothing is inserted, and the source stays empty. So first WAIT
    // until the heading has actually landed in and rendered from the
    // (still-bound, modifiable) source buffer. `wait_for_frame_text` is a pure
    // read of the already-flushed active frame (no RPC), so it cannot move focus;
    // only once the heading is on screen do we preview.
    wait_for_frame_text(&handle, HEADING_TEXT);

    // --- 3. open_preview() returns a real preview bufnr, in place -------------
    let preview_bufnr = handle.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() on the Untitled.md buffer must return a valid preview bufnr \
         (> 0, the anonymous buffer's markdown filetype passes the gate), got {preview_bufnr}"
    );
    wait_for_window_count(
        &handle,
        ONE_PANE,
        "the in-place preview of Untitled.md creates no pane",
    );

    // --- 4. previews() surfaces the Untitled.md source row --------------------
    let previews = wait_for_previews(&handle, "one Untitled.md preview", |p| {
        p.iter().any(|row| row.source_name() == UNTITLED)
    });
    let source_bufnr = previews
        .iter()
        .find(|row| row.source_name() == UNTITLED)
        .expect("a preview row for Untitled.md must exist")
        .source_bufnr();
    assert!(
        source_bufnr > 0,
        "the preview row must carry Untitled.md's real source bufnr (> 0), got {source_bufnr}"
    );

    // --- 5. render_preview() carries the typed heading text -------------------
    // The insert-mode text is buffered on the async key channel, and the render
    // reflects the source buffer's current lines. A one-shot `render_preview` can
    // race ahead of the typed heading landing, so POLL the blocking pull (bounded by
    // FLUSH_TIMEOUT) until the rendered HTML CONTAINS the heading text.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut html = handle.render_preview(source_bufnr);
    while Instant::now() < deadline {
        html = handle.render_preview(source_bufnr);
        if html.contains(HEADING_TEXT) {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        html.contains(HEADING_TEXT),
        "render_preview must contain the heading typed into the Untitled.md buffer \
         ({HEADING_TEXT:?}): {html}"
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn preview_carries_the_source_buffers_cursor() {
    // THE scroll-sync FFI join, end to end: `WolfCoreHandle::previews()` looks each
    // row's `source_bufnr` up in `wolf_core::Snapshot::markdown_cursors` and exposes
    // the result as `FfiPreview::source_cursor_line()` / `source_cursor_col()`. This
    // is the one layer of the cursor-follow feature that is otherwise covered only
    // by compilation: the Lua -> core half is pinned by
    // `//crates/wolf-core:markdown_cursor` and the Swift arithmetic by
    // `PreviewScrollSeamTests`, but nothing else witnesses the JOIN.
    //
    // The contract pinned here is exactly the one the front-end consumes: LINE is
    // 1-based (matching the `wl-{L}-{C}` marker scheme's `L`), COLUMN is a 0-based
    // CHARACTER column (its `C`), and both describe the row's OWN SOURCE buffer.
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(tmp.path().join("alpha.md"), CURSOR_DOC).expect("failed to write alpha.md");

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open alpha.md as the source ------------------------------
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    // Real switch barrier: the startup buffer is itself a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY and cannot witness the
    // `:e alpha.md`. Wait until alpha.md is genuinely the ACTIVE buffer before the
    // SYNC `open_preview` races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");

    // --- 2. Split (typed :vsplit), then preview IN PLACE in the new pane ------
    // A second pane is needed so the SOURCE keeps a window for the motions below.
    send_ex_command(&handle, ":vsplit");
    wait_for_window_count(&handle, TWO_PANES, "after the typed :vsplit");
    let preview_bufnr = handle.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {preview_bufnr}"
    );
    // Deterministic barrier before cycling back: the in-place bind leaves the
    // PREVIEW pane current, and `focus_markdown_source` would otherwise be able to
    // read a stale `is_markdown_active() == true` and skip the cycle.
    wait_for_preview_active(
        &handle,
        true,
        "the in-place bind leaves the preview pane current",
    );

    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let alpha_bufnr = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist")
        .source_bufnr();
    assert!(
        alpha_bufnr > 0,
        "the preview row must carry alpha.md's real source bufnr (> 0), got {alpha_bufnr}"
    );

    // --- 3. Drive the SOURCE buffer's cursor to an ABSOLUTE position ---------
    // Back to the source pane first, so the motions move the SOURCE cursor rather
    // than the (nomodifiable, guarded) preview scratch's.
    focus_markdown_source(&handle);
    handle.send_key(char_key('g'));
    handle.send_key(char_key('g'));
    handle.send_key(char_key('0'));

    let previews = wait_for_previews(
        &handle,
        "the joined preview cursor to settle at line 1, column 0 after `gg0`",
        |p| cursor_for_source(p, alpha_bufnr) == Some(CURSOR_AT_START),
    );
    assert_eq!(
        cursor_for_source(&previews, alpha_bufnr),
        Some(CURSOR_AT_START),
        "the preview row must report its SOURCE buffer's cursor as a 1-based line and \
         a 0-based column — a broken markdown_cursors join would fall through \
         unwrap_or_default() to {CURSOR_UNKNOWN:?}"
    );

    // --- 4. A pure motion moves the JOINED cursor with it --------------------
    handle.send_key(char_key('j'));
    let previews = wait_for_previews(
        &handle,
        "the joined preview cursor line to advance to 2 after `j`",
        |p| cursor_for_source(p, alpha_bufnr).is_some_and(|(line, _)| line == CURSOR_AFTER_J.0),
    );
    assert_eq!(
        cursor_for_source(&previews, alpha_bufnr),
        Some(CURSOR_AFTER_J),
        "a single `j` must advance the joined LINE by one hard line and retain the \
         column — this is the signal that makes the preview follow the cursor at all"
    );

    handle.send_key(char_key('l'));
    let previews = wait_for_previews(
        &handle,
        "the joined preview cursor column to advance to 1 after `l`",
        |p| cursor_for_source(p, alpha_bufnr) == Some(CURSOR_AFTER_L),
    );
    assert_eq!(
        cursor_for_source(&previews, alpha_bufnr),
        Some(CURSOR_AFTER_L),
        "one `l` from column 0 must report the 0-BASED column 1 (not the 1-based 2) — \
         the front-end rounds this DOWN to a marker stride to build the `wl-{{L}}-{{C}}` \
         anchor id, so an off-by-one here silently mis-anchors every scroll"
    );

    // --- 5. The join keys on the SOURCE bufnr, not the preview scratch -------
    // No row's `source_bufnr()` is the scratch buffer, so a lookup by the scratch's
    // number finds nothing at all — which is what makes step 3/4's values provably
    // the SOURCE buffer's cursor rather than anything the preview pane reported.
    assert!(
        cursor_for_source(&previews, preview_bufnr).is_none(),
        "the preview SCRATCH bufnr {preview_bufnr} must never appear as a row's \
         source_bufnr(); previews were {previews:?}"
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn preview_cursor_defaults_to_zero_when_unreported() {
    // The "cursor unknown" sentinel: `WolfCoreHandle::previews()`'s
    // `markdown_cursors` join ends in `unwrap_or_default()`, so a preview whose
    // SOURCE buffer has never reported a cursor reports `(0, 0)`. The front-end
    // gates on that (`CursorPos.isKnown` is `line > 0`) and refuses to scroll — so
    // if the sentinel ever became, say, `1`, every such preview would silently jump
    // to the top of the document instead of staying put.
    //
    // `preview_file` is the deterministic way to reach that state: it makes the
    // linked file a source via a background `bufadd`/`bufload` and binds only the
    // PREVIEW SCRATCH into the window, so the linked source never becomes current
    // and never fires the cursor autocmds (`emit_cursor` bails on any buffer that is
    // not `nvim_get_current_buf()`, and the scratch is excluded by the explicit
    // `b:wolf_preview_source` guard). That is exactly why there is deliberately NO
    // cursor re-emit in `wolf_preview_file`.
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(tmp.path().join("alpha.md"), CURSOR_DOC).expect("failed to write alpha.md");
    std::fs::write(
        tmp.path().join("beta.md"),
        "# Beta Heading\n\nAnother paragraph.\n",
    )
    .expect("failed to write beta.md");
    // Absolute, as WebKit would resolve a `file://` link; Lua does NO relative
    // resolution. `tmp.path()` is already absolute.
    let beta_path = tmp.path().join("beta.md").to_string_lossy().into_owned();

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open alpha.md and preview it in place ---------------------
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    wait_for_current_file(&handle, "alpha.md");

    let preview_bufnr = handle.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, ONE_PANE, "in-place preview creates no pane");

    // --- 2. POSITIVE control: alpha.md's preview DOES carry a known cursor ---
    // Without this the negative assertion below would be vacuous — a dead cursor
    // pipeline would also report the sentinel. (alpha.md's cursor was recorded
    // while it was the current buffer, before the in-place bind.)
    let previews = wait_for_previews(
        &handle,
        "alpha.md's preview row to carry a KNOWN cursor (line >= 1)",
        |p| {
            p.iter()
                .any(|row| row.source_name() == "alpha.md" && row.source_cursor_line() >= 1)
        },
    );
    let preview_win = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist")
        .win();
    assert!(
        preview_win > 0,
        "the alpha.md preview must carry a real preview WINDOW handle (> 0), got {preview_win}"
    );

    // --- 3. Repoint that window at beta.md, which has NO reported cursor -----
    let beta_bufnr = handle.preview_file(preview_win, beta_path);
    assert!(
        beta_bufnr > 0,
        "preview_file(win, <abs beta.md>) must return beta.md's source bufnr (> 0), got {beta_bufnr}"
    );
    let previews = wait_for_previews(&handle, "the preview window now previews beta.md", |p| {
        p.iter()
            .any(|row| row.win() == preview_win && row.source_name() == "beta.md")
    });
    assert_eq!(
        cursor_for_source(&previews, beta_bufnr),
        Some(CURSOR_UNKNOWN),
        "a preview whose SOURCE buffer has never reported a cursor must expose the \
         `line == 0` unknown sentinel from the join's unwrap_or_default(), which is \
         what makes the front-end leave the freshly bound preview's scroll position \
         alone; previews were {previews:?}"
    );

    // --- 4. And it STAYS the sentinel (no late emit sneaks in) ---------------
    let deadline = Instant::now() + NEGATIVE_SETTLE;
    while Instant::now() < deadline {
        let now = handle.previews();
        assert_eq!(
            cursor_for_source(&now, beta_bufnr),
            Some(CURSOR_UNKNOWN),
            "a background `bufadd`/`bufload` source is never CURRENT, so no \
             wolf_markdown_cursor may ever land for it; previews were {now:?}"
        );
        sleep(POLL_INTERVAL);
    }

    handle.shutdown();
    drop(tmp);
}
