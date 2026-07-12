//! Integration test for the `wolf-ffi` MARKDOWN-PREVIEW bridge surface (plan
//! §"FFI" + the "Real-nvim FFI" test bullet): the "is the active buffer markdown"
//! gate, the `open_preview(mode)` mechanism (single-pane split-right vs. multi-pane
//! active-pane, OD3), the `previews()` snapshot of `FfiPreview` rows, the blocking
//! `render_preview(source_bufnr)` HTML pull, and the fact that the preview's
//! scratch buffer surfaces in the buffer picker as `Preview: <basename>` (OD1).
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
//! plus the opaque `FfiPreview { win() -> i64, source_bufnr() -> i64,
//! source_name() -> String (basename), source_path() -> String,
//! source_tick() -> u64 }` in `crates/wolf-ffi/src/lib.rs`, regenerates
//! `generated/`, and appends `command_kind::PREVIEW = 18`. This test file will not
//! compile until that lands — expected.
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
    assert_eq!(got, target, "is_markdown_active() should be {target} ({desc})");
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
