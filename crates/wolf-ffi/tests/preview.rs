//! Integration test for the `wolf-ffi` MARKDOWN-PREVIEW bridge surface (plan
//! §"FFI" + the "Real-nvim FFI" test bullet): the "is the active buffer markdown"
//! gate, the `open_preview(mode)` mechanism (single-pane split-right vs. multi-pane
//! active-pane, OD3), the `previews()` snapshot of `FfiPreview` rows, the blocking
//! `render_preview(source_bufnr)` HTML pull, and the fact that the preview's
//! scratch buffer surfaces in the buffer picker as `Preview: <basename>` (OD1).
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
//! ## NOTE to coder — Bazel target
//!
//! This file needs a `//crates/wolf-ffi:preview` `rust_test` target added to
//! `crates/wolf-ffi/BUILD.bazel` (a copy of the `:buffer_picker` stanza:
//! `srcs = ["tests/common/mod.rs", "tests/preview.rs"]`,
//! `crate_root = "tests/preview.rs"`). This test file does NOT touch BUILD files.
//!
//! ## Dependency on coder work (plan §FFI)
//!
//! None of this exists yet — the coder adds, on `WolfCoreHandle`:
//!   * `is_markdown_active(&self) -> bool`
//!   * `open_preview(&self, mode: String) -> i64` (preview bufnr, 0 on
//!     error/not-markdown)
//!   * `render_preview(&self, source_bufnr: i64) -> String` (blocking; empty on
//!     error)
//!   * `previews(&self) -> Vec<FfiPreview>`
//!     plus the opaque `FfiPreview { win() -> i64, source_bufnr() -> i64,
//! source_name() -> String (basename), source_path() -> String,
//! source_tick() -> u64 }` in `crates/wolf-ffi/src/lib.rs`, regenerates
//!     `generated/`, and appends `command_kind::PREVIEW = 18`. This test file will not
//!     compile until that lands — expected.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim (`WOLF_NVIM_BIN` or the
//! Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, FfiPreview};

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (filetype detection, the split + `nvim_win_set_buf`, the out-of-band
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

/// The two-pane window count reached after the split-right preview placement.
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
/// This is the deterministic barrier after `open_preview("split_right")`, which
/// leaves the freshly split PREVIEW pane current. Without it, a following
/// `focus_markdown_source` can read a STALE `is_markdown_active() == true` (the
/// notification for the new active pane not having landed yet), return without
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

#[test]
fn preview_gates_opens_lists_and_live_updates() {
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

    // --- 3. Single-pane placement: open_preview("split_right") ---------------
    // One pane at this point, so the preview opens in a NEW pane to the right.
    // `open_preview` returns the preview scratch bufnr (> 0 since markdown active).
    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");

    // --- 4. previews() surfaces one row for alpha.md -------------------------
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let alpha_row = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist");
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

    // --- 6. Live-update: editing the source advances the preview's tick -------
    // Focus the SOURCE pane (the split left the preview pane active), capture the
    // current tick, delete a line (`dd` — a normal-mode buffer change that fires
    // TextChanged), and observe the joined source_tick advance.
    focus_markdown_source(&handle);
    let initial_tick = tick_for_source(&handle.previews(), alpha_bufnr).unwrap_or(0);

    handle.send_key(char_key('d'));
    handle.send_key(char_key('d'));

    wait_for_previews(&handle, "source_tick advances after an edit", |p| {
        tick_for_source(p, alpha_bufnr).is_some_and(|t| t > initial_tick)
    });

    // --- 6b. The preview row exposes the source's cwd-relative display path ---
    // `source_display_path()` feeds the preview pane's SECONDARY statusbar line
    // (CHANGE 3), mirroring editor panes. The fixture lives at the cwd ROOT, so the
    // display path ends with the bare basename. Use `ends_with`, NOT exact equality:
    // the source path is uncanonicalized while live_cwd is canonical, so a
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

    // --- 7. Buffer picker lists the preview as `Preview: alpha.md` -----------
    // A fresh picker snapshots list_buffers(); the buflisted scratch buffer shows
    // up with its `Preview: <basename>` name (OD1) with NO front-end injection.
    let rows = handle.buffer_picker().ranked(String::new(), RANKED_MAX);
    assert!(
        rows.iter()
            .any(|r| r.display().starts_with("Preview: alpha.md")),
        "the buffer picker must include a `Preview: alpha.md` row: {:?}",
        rows.iter().map(|r| r.display()).collect::<Vec<_>>()
    );

    // --- 8. Multi-pane placement: open_preview("active") ---------------------
    // We are focused on the source pane with two panes live, so "active" replaces
    // THIS pane's buffer with the preview scratch (nvim_win_set_buf). Both panes
    // then show a preview-source buffer, so previews() reports two rows.
    let reused = handle.open_preview("active".to_string());
    assert_eq!(
        reused, preview_bufnr,
        "open_preview(\"active\") must REUSE the same scratch buffer for this source"
    );
    wait_for_previews(&handle, "both panes show the preview buffer", |p| {
        p.len() == TWO_PANES
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
        handle.open_preview("active".to_string()),
        0,
        "after shutdown open_preview() must be the neutral 0"
    );

    drop(tmp);
}

/// Seeds a fresh temp cwd with `alpha.md` AND `beta.md` (both real Markdown), the
/// two source files the reuse/fallback tests edit. Returns the live `TempDir` (kept
/// alive until after `shutdown()`) so it outlives the core as nvim's working dir +
/// search root.
fn seed_two_markdown_files() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n",
    )
    .expect("failed to write alpha.md");
    std::fs::write(
        tmp.path().join("beta.md"),
        "# Beta Heading\n\nAnother paragraph.\n",
    )
    .expect("failed to write beta.md");
    tmp
}

#[test]
fn preview_picker_shows_cwd_relative_path_for_subdir_source() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Seed the markdown source in a SUBDIRECTORY of the cwd. The picker names the
    // preview scratch buffer with the cwd-relative path (`:.` -> `docs/alpha.md`),
    // which is distinct from the bare basename (`:t` -> `alpha.md`) the old naming
    // produced — this is the ONLY test that witnesses the `:.` change (CHANGE 1).
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
    // Real switch barrier: the startup buffer is now a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e docs/alpha.md`. Wait until alpha.md is genuinely the
    // ACTIVE buffer before the SYNC `open_preview` races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");

    // --- 2. Split-right preview: previews() surfaces one row for alpha.md -----
    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");
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

    // --- 4. The picker names the scratch buffer with the cwd-relative path -----
    // `:.` yields `Preview: docs/alpha.md`; the old `:t` would have yielded the bare
    // `Preview: alpha.md`. This distinguishes the new naming from the old.
    let rows = handle.buffer_picker().ranked(String::new(), RANKED_MAX);
    assert!(
        rows.iter()
            .any(|r| r.display().starts_with("Preview: docs/alpha.md")),
        "the buffer picker must include a `Preview: docs/alpha.md` row: {:?}",
        rows.iter().map(|r| r.display()).collect::<Vec<_>>()
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn preview_reuses_the_other_preview_pane() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let tmp = seed_two_markdown_files();

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
    // Real switch barrier: the startup buffer is now a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e alpha.md`. Wait until alpha.md is genuinely the ACTIVE
    // buffer before the SYNC `open_preview` races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");

    // --- 2. Split-right preview: two panes, right previews alpha.md ----------
    let alpha_preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        alpha_preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {alpha_preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");
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

    // --- 3. Focus the SOURCE pane and edit beta.md there ---------------------
    // The split left the preview pane active; cycle back to the markdown source
    // pane, then :e beta.md IN THAT source window (not the preview pane).
    focus_markdown_source(&handle);
    let active_before = handle.active_win();
    send_ex_command(&handle, ":e beta.md");
    // is_markdown_active is ALREADY true (the pane still shows alpha.md, also
    // markdown), so it cannot witness this markdown→markdown switch. Wait on
    // current_file() instead so beta.md is genuinely the ACTIVE buffer before the
    // blocking reuse call reads the current source.
    wait_for_current_file(&handle, "beta.md");

    // --- 4. Reuse placement: bind beta's preview into the OTHER preview pane --
    let beta_preview_bufnr = handle.open_preview("reuse_preview_pane".to_string());
    assert!(
        beta_preview_bufnr > 0,
        "open_preview(\"reuse_preview_pane\") on a markdown buffer must return a \
         valid preview bufnr (> 0), got {beta_preview_bufnr}"
    );

    // --- 5. Assert: beta reused the SAME preview window; nothing else moved ---
    // The deterministic emit at the end of wolf_open_preview replaces the whole
    // previews snapshot, so wait for exactly one row: beta.md at `preview_win`.
    let previews = wait_for_previews(
        &handle,
        "the sole preview row is beta.md bound into the reused preview window",
        |p| {
            p.len() == 1
                && p.iter()
                    .any(|row| row.source_name() == "beta.md" && row.win() == preview_win)
        },
    );
    assert_eq!(
        previews.len(),
        1,
        "reuse must WHOLESALE-replace the preview: exactly one row (beta.md), got {previews:?}",
    );
    assert!(
        !previews.iter().any(|row| row.source_name() == "alpha.md"),
        "alpha.md's preview row must be gone after the reuse rebind: {previews:?}",
    );
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "reuse must NOT create a new pane (still two live windows)",
    );
    assert_eq!(
        handle.active_win(),
        active_before,
        "reuse must NOT change the active window (the source pane stays focused)",
    );
    // The reuse rebind does a background-window `nvim_win_set_buf`, after which
    // `wolf_emit_current_file` re-asserts the ACTIVE (source) buffer's filetype
    // asynchronously via the redraw/notification path. Reading
    // `is_markdown_active()` synchronously here races that emit, so poll (bounded
    // by FLUSH_TIMEOUT, the same convention as the other `wait_for_*` helpers)
    // until it settles to true — the active source pane still shows markdown.
    wait_for_markdown_active(
        &handle,
        true,
        "the active (source) pane still shows a markdown buffer after the reuse rebind",
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn preview_reuse_falls_back_to_active_when_no_other_preview() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let tmp = seed_two_markdown_files();

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
    // Real switch barrier: the startup buffer is now a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e alpha.md`. Wait until alpha.md is genuinely the ACTIVE
    // buffer before the SYNC `open_preview` races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");

    // --- 2. Make a second EDITOR pane (no previews) via :vsplit --------------
    // Both panes now show the SAME markdown buffer; neither is a preview pane.
    send_ex_command(&handle, ":vsplit");
    wait_for_window_count(
        &handle,
        TWO_PANES,
        "after :vsplit into a second editor pane",
    );
    focus_markdown_source(&handle);
    assert!(
        handle.previews().is_empty(),
        "no preview windows should exist before the fallback open_preview: {:?}",
        handle.previews(),
    );
    let active_before = handle.active_win();

    // --- 3. Reuse placement with NO other preview -> fall back to active -----
    // wolf_open_preview finds no OTHER window carrying b:wolf_preview_source, so the
    // Lua fallback binds into the CURRENT (active) window.
    let preview_bufnr = handle.open_preview("reuse_preview_pane".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"reuse_preview_pane\") on a markdown buffer must return a \
         valid preview bufnr (> 0), got {preview_bufnr}"
    );

    // --- 4. Assert: exactly one preview, bound into the ACTIVE window --------
    let previews = wait_for_previews(
        &handle,
        "one preview row bound into the active (fallback) window",
        |p| p.len() == 1 && p.iter().any(|row| row.win() == active_before),
    );
    assert_eq!(
        previews.len(),
        1,
        "the fallback must produce exactly one preview row, got {previews:?}",
    );
    assert_eq!(
        previews[0].win(),
        active_before,
        "with no OTHER preview window the fallback binds into the ACTIVE window",
    );
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "the fallback must NOT create a new pane (still two live windows)",
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

    // --- 1. Attach; open math.md as the source and split-right preview it -----
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e math.md");
    wait_for_markdown_active(&handle, true, "after :e math.md");
    // Real switch barrier: the startup buffer is now a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e math.md`. Wait until math.md is genuinely the ACTIVE
    // buffer before the SYNC `open_preview` races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "math.md");

    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");

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
fn preview_file_repoints_pane_to_linked_file() {
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

    // --- 1. Attach; open alpha.md as the source and split-right preview it ----
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    // Real switch barrier: the startup buffer is now a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e alpha.md`. Wait until alpha.md is genuinely the ACTIVE
    // buffer before the SYNC `open_preview` races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");

    let alpha_preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        alpha_preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {alpha_preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");
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
        TWO_PANES,
        "preview_file must NOT create a new pane (still two live windows)",
    );

    // --- 3. render_preview(beta) carries beta's heading text + slug id --------
    let html = handle.render_preview(beta_bufnr);
    assert!(
        html.contains("Beta Heading"),
        "render_preview(beta) must contain the linked file's heading text: {html}"
    );
    assert!(
        html.contains(r#"id="beta-heading""#),
        "render_preview(beta) must carry beta's heading slug id: {html}"
    );

    // --- 4. Negative: a non-markdown link returns 0 and changes nothing -------
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

    // --- 5. Shutdown: preview_file returns the neutral 0 ---------------------
    handle.shutdown();
    assert_eq!(
        handle.preview_file(preview_win, beta_path.clone()),
        0,
        "after shutdown preview_file() must be the neutral 0"
    );

    drop(tmp);
}

#[test]
fn preview_renders_untitled_buffer() {
    // The anonymous-buffer feature: a fresh session with NO markdown file on disk
    // still lands on a real, markdown-named `Untitled.md` buffer, so the in-app
    // preview gate (which previously returned 0 for a filetype-less `[No Name]`
    // buffer) now passes and renders the buffer's live contents.
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
    // while `open_preview("split_right")` below is a SYNCHRONOUS RPC that splits and
    // binds the NOMODIFIABLE preview scratch into the current window, STEALING focus.
    // If the sync RPC wins the race, the async `i# Untitled Heading<Esc>` keys arrive
    // AFTER focus has moved to the nomodifiable preview buffer — `i` fails with E21,
    // nothing is inserted, and the source stays empty. So first WAIT until the heading
    // has actually landed in and rendered from the (still-focused, modifiable) source
    // buffer. `wait_for_frame_text` is a pure read of the already-flushed active frame
    // (no RPC), so it cannot move focus; only once the heading is on screen do we
    // split.
    wait_for_frame_text(&handle, HEADING_TEXT);

    // --- 3. open_preview("split_right") now returns a real preview bufnr ------
    // Previously this returned 0 (no-op) because the anonymous buffer had no
    // filetype; naming + `filetype=markdown` makes the gate pass.
    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on the Untitled.md buffer must return a valid \
         preview bufnr (> 0, previously 0 for a filetype-less [No Name]), got {preview_bufnr}"
    );
    wait_for_window_count(
        &handle,
        TWO_PANES,
        "after split-right preview of Untitled.md",
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

    // --- 2. Split-right preview ----------------------------------------------
    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");
    // Deterministic barrier before cycling back: the split leaves the PREVIEW pane
    // current, and `focus_markdown_source` would otherwise be able to read a stale
    // `is_markdown_active() == true` and skip the cycle.
    wait_for_preview_active(
        &handle,
        true,
        "the split-right placement leaves the new preview pane current",
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
    // `b:wolf_preview_source` guard). That is exactly why the plan deliberately does
    // NOT add a cursor re-emit to `wolf_preview_file`.
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

    // --- 1. Attach; open alpha.md and split-right preview it -----------------
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    wait_for_current_file(&handle, "alpha.md");

    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");

    // --- 2. POSITIVE control: alpha.md's preview DOES carry a known cursor ---
    // Without this the negative assertion below would be vacuous — a dead cursor
    // pipeline would also report the sentinel.
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
