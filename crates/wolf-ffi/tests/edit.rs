//! Integration test for the `wolf-ffi` EDIT bridge surface (plan §5): the INVERSE
//! of Preview. It exercises the "is the active pane a preview" gate
//! (`is_preview_active()`), and the `edit_source(mode)` mechanism that navigates a
//! preview pane back to its SOURCE editor in one of three placements
//! (`split_left`, `reuse_editor_pane`, `active`), mirroring `tests/preview.rs`.
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is modeled on `tests/preview.rs`, reusing
//! `tests/common/mod.rs` and the same loud SKIP-when-no-nvim guard.
//!
//! ## NOTE to coder — Bazel target
//!
//! This file needs a `//crates/wolf-ffi:edit` `rust_test` target added to
//! `crates/wolf-ffi/BUILD.bazel` (a copy of the `:preview` stanza:
//! `srcs = ["tests/common/mod.rs", "tests/edit.rs"]`,
//! `crate_root = "tests/edit.rs"`). This test file does NOT touch BUILD files
//! (the test-writer added that stanza separately).
//!
//! ## Dependency on coder work (plan §4 + §5)
//!
//! None of this exists yet — the coder adds, on `WolfCoreHandle`:
//!   * `is_preview_active(&self) -> bool` (mirrors `is_markdown_active`; reads the
//!     new `Snapshot.current_is_preview` fed by the `wolf_current_file`
//!     notification's 3rd boolean arg)
//!   * `edit_source(&self, mode: String) -> i64` (the preview's SOURCE bufnr, `0`
//!     off a preview pane / on error)
//! plus the `Command::Edit` registry row and `command_kind::EDIT = 19`. This test
//! file will not compile until that lands — expected.
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
/// `wolf_preview` / `wolf_current_file` notifications) to settle. Mirrors
/// `preview.rs`'s `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors `preview.rs`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A short settle between window-cycling `:wincmd w` attempts, long enough for the
/// `WinEnter`/`BufEnter` autocmd to re-emit `wolf_current_file` (so
/// `is_preview_active()` reflects the newly-focused window). Mirrors `preview.rs`.
const CYCLE_SETTLE: Duration = Duration::from_millis(150);

/// The lowest real WINDOW grid id (grid 1 is the outer/global grid; editor windows
/// are `>= 2` under `ext_multigrid`). Mirrors `preview.rs`.
const WINDOW_GRID_MIN: u64 = 2;

/// The single-pane window count (the sole preview pane, or after a reuse-fallback
/// rebind that adds no pane).
const ONE_PANE: usize = 1;

/// The two-pane window count reached after a split-right preview placement.
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
/// Mirrors `preview.rs::live_window_count`.
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

/// Blocks until `handle.is_preview_active()` equals `target`, then asserts it. The
/// EDIT analogue of [`wait_for_markdown_active`]: `is_preview_active()` reads the
/// snapshot's `current_is_preview`, re-asserted async via `wolf_current_file`, so a
/// synchronous read right after a focus change races that emit.
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
    assert_eq!(got, target, "is_preview_active() should be {target} ({desc})");
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

/// Blocks until `handle.active_win()` equals `target`, returning nothing; panics
/// with `desc` on timeout. Focus changes ride the async notification path.
fn wait_for_active_win(handle: &wolf_ffi::WolfCoreHandle, target: i64, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut got = handle.active_win();
    while Instant::now() < deadline {
        got = handle.active_win();
        if got == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(got, target, "active_win() should be {target} ({desc})");
}

/// Blocks until `predicate(&previews)` holds on a fresh `previews()` snapshot,
/// returning that snapshot. Panics with `desc` on timeout. Same as `preview.rs`.
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

/// Cycles editor windows with `:wincmd w` until the active pane is a PREVIEW
/// (`is_preview_active()`), so a subsequent `edit_source` gate sees a preview
/// buffer. With exactly two panes this needs at most one cycle. The EDIT analogue
/// of `preview.rs::focus_markdown_source`.
fn focus_preview(handle: &wolf_ffi::WolfCoreHandle) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if handle.is_preview_active() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out cycling windows to focus the preview pane"
        );
        send_ex_command(handle, ":wincmd w");
        sleep(CYCLE_SETTLE);
    }
}

/// Seeds a fresh temp cwd with a single real Markdown file `alpha.md`. Returns the
/// live `TempDir` (kept alive until after `shutdown()`) so its `.wolf/` MRU cache
/// outlives the core.
fn seed_markdown_cwd() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n",
    )
    .expect("failed to write alpha.md");
    tmp
}

/// Starts a core against `tmp` and blocks until attached. Argument order mirrors
/// the sibling tests exactly: (init_lua, cwd, cols, rows).
fn start_attached(tmp: &tempfile::TempDir) -> wolf_ffi::WolfCoreHandle {
    let init_lua = common::init_lua_path();
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    wait_for_attach(&handle);
    handle
}

/// From an already-open `alpha.md` source editor, opens a split-right preview and
/// collapses the layout to a SINGLE preview pane (via `:only` from the focused
/// preview window). Returns alpha's SOURCE bufnr (the preview row's
/// `source_bufnr()`), captured after the collapse. The precondition for the
/// `split_left` and reuse-fallback placements, both of which start from a lone
/// preview pane.
fn collapse_to_single_preview_pane(handle: &wolf_ffi::WolfCoreHandle) -> i64 {
    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(handle, TWO_PANES, "after split-right preview");
    wait_for_previews(handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });

    // The split left the preview pane active; make sure of it, then `:only` closes
    // the source editor window, leaving JUST the preview pane.
    focus_preview(handle);
    send_ex_command(handle, ":only");
    wait_for_window_count(handle, ONE_PANE, "after :only collapses to the sole preview pane");

    let previews = wait_for_previews(handle, "the sole preview row is alpha.md", |p| {
        p.len() == 1 && p.iter().any(|row| row.source_name() == "alpha.md")
    });
    previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist")
        .source_bufnr()
}

#[test]
fn edit_gates_on_preview_pane_and_neutralizes_on_shutdown() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. A plain markdown buffer is NOT a preview -------------------------
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    wait_for_preview_active(&handle, false, "a plain markdown editor pane is not a preview");

    // --- 2. After split-right preview + focusing it, the active pane IS a preview
    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");
    focus_preview(&handle);
    wait_for_preview_active(&handle, true, "the focused preview pane reports is_preview_active");

    // --- 3. edit_source("active") replaces the focused preview with its source;
    // the async wolf_current_file notification (3rd arg = is_preview) must flip
    // is_preview_active() back to false now that the active buffer is a source.
    let returned = handle.edit_source("active".to_string());
    assert!(
        returned > 0,
        "edit_source(\"active\") on the focused preview must return its SOURCE bufnr \
         (> 0), got {returned}"
    );
    wait_for_preview_active(
        &handle,
        false,
        "replacing the active preview with its source flips is_preview_active back to false",
    );

    // --- 4. Shutdown neutralizes both accessors ------------------------------
    handle.shutdown();
    assert!(
        !handle.is_preview_active(),
        "after shutdown is_preview_active() must be the neutral false"
    );
    assert_eq!(
        handle.edit_source("active".to_string()),
        0,
        "after shutdown edit_source() must be the neutral 0"
    );

    drop(tmp);
}

#[test]
fn edit_split_left_opens_source_in_a_new_pane() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. Collapse to a SINGLE preview pane (the split_left precondition) ---
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    let alpha_source_bufnr = collapse_to_single_preview_pane(&handle);

    // --- 2. edit_source("split_left"): open the SOURCE in a NEW pane ----------
    // Lua binds the source into the ORIGINAL (formerly-preview) window and moves the
    // preview scratch into the newly-split window, then focuses the source window.
    let returned = handle.edit_source("split_left".to_string());
    assert_eq!(
        returned, alpha_source_bufnr,
        "edit_source(\"split_left\") must return the preview's SOURCE bufnr"
    );

    // --- 3. Assert: one new pane, one surviving preview, source in a non-preview
    wait_for_window_count(&handle, TWO_PANES, "split_left grows the layout by one pane");
    let previews = wait_for_previews(&handle, "exactly one preview remains after split_left", |p| {
        p.len() == 1
    });
    let preview_win = previews[0].win();
    let source_win = handle.active_win();
    assert_ne!(
        source_win, preview_win,
        "the focused SOURCE window must NOT be the (surviving) preview window"
    );
    assert!(
        !previews.iter().any(|row| row.win() == source_win),
        "the SOURCE editor must land in a window that is NOT a preview: {:?}",
        previews.iter().map(FfiPreview::win).collect::<Vec<_>>()
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn edit_reuses_the_editor_pane() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. alpha.md editor pane; capture its window BEFORE the split --------
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    let editor_win = handle.active_win();
    assert!(
        editor_win > 0,
        "the alpha.md editor must carry a real window handle (> 0), got {editor_win}"
    );

    // --- 2. Split-right preview -> two panes (editor + preview), focus preview
    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let alpha_source_bufnr = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist")
        .source_bufnr();
    focus_preview(&handle);

    // --- 3. edit_source("reuse_editor_pane"): bind source into the OTHER editor
    let returned = handle.edit_source("reuse_editor_pane".to_string());
    assert_eq!(
        returned, alpha_source_bufnr,
        "edit_source(\"reuse_editor_pane\") must return the preview's SOURCE bufnr"
    );

    // --- 4. Assert: the editor window is now focused; no new pane ------------
    wait_for_active_win(
        &handle,
        editor_win,
        "reuse_editor_pane focuses the OTHER (editor) window",
    );
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "reuse_editor_pane must NOT create a new pane (still two live windows)",
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn edit_active_replaces_the_preview_with_the_source() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. alpha.md; split-right preview; focus the preview -----------------
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let alpha_source_bufnr = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist")
        .source_bufnr();
    focus_preview(&handle);
    let preview_win = handle.active_win();

    // --- 2. edit_source("active"): replace THIS preview with its source ------
    let returned = handle.edit_source("active".to_string());
    assert_eq!(
        returned, alpha_source_bufnr,
        "edit_source(\"active\") must return the preview's SOURCE bufnr"
    );

    // --- 3. Assert: the previously-preview window now shows the source -------
    // The only preview was in `preview_win`; replacing it leaves previews() empty.
    wait_for_previews(&handle, "the replaced preview no longer lists a preview row", |p| {
        p.is_empty()
    });
    assert_eq!(
        handle.active_win(),
        preview_win,
        "edit_source(\"active\") must stay in the SAME (now source) window",
    );
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "edit_source(\"active\") must NOT create a new pane (still two live windows)",
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn edit_reuse_falls_back_to_active_when_no_other_editor() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. Collapse to a SINGLE preview pane (no OTHER editor pane) ---------
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    let alpha_source_bufnr = collapse_to_single_preview_pane(&handle);
    let active_before = handle.active_win();

    // --- 2. reuse_editor_pane with NO other editor -> fall back to active ----
    // The Lua scan finds no OTHER non-preview window, so it binds the source into
    // the CURRENT (active) window: no new pane.
    let returned = handle.edit_source("reuse_editor_pane".to_string());
    assert_eq!(
        returned, alpha_source_bufnr,
        "the reuse fallback must still return the preview's SOURCE bufnr"
    );

    // --- 3. Assert: source bound into the active window; no new pane ---------
    wait_for_previews(&handle, "the sole preview is replaced by its source (fallback)", |p| {
        p.is_empty()
    });
    assert_eq!(
        handle.active_win(),
        active_before,
        "the fallback must bind into the ACTIVE window (unchanged focus)",
    );
    assert_eq!(
        live_window_count(&handle),
        ONE_PANE,
        "the fallback must NOT create a new pane (still one live window)",
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn edit_source_is_a_noop_off_a_preview_pane() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. A plain markdown editor pane (NOT a preview) --------------------
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    wait_for_preview_active(&handle, false, "a plain markdown editor pane is not a preview");

    let windows_before = live_window_count(&handle);
    assert!(
        handle.previews().is_empty(),
        "no preview windows should exist before the negative edit_source: {:?}",
        handle.previews().iter().map(FfiPreview::win).collect::<Vec<_>>(),
    );

    // --- 2. edit_source off a preview pane returns 0 and changes nothing -----
    let returned = handle.edit_source("active".to_string());
    assert_eq!(
        returned, 0,
        "edit_source() off a NON-preview pane must return the neutral 0"
    );
    assert_eq!(
        live_window_count(&handle),
        windows_before,
        "a rejected edit_source must not change the window layout",
    );
    assert!(
        handle.previews().is_empty(),
        "a rejected edit_source must leave previews() empty: {:?}",
        handle.previews().iter().map(FfiPreview::win).collect::<Vec<_>>(),
    );

    handle.shutdown();
    drop(tmp);
}
