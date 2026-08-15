//! Integration test for the `wolf-ffi` EDIT bridge surface: the INVERSE of
//! Preview under the pane-mode model. It exercises the "is the active pane a
//! preview" gate (`is_preview_active()`) and the IN-PLACE `edit_source()`
//! mechanism, which replaces the active preview pane's scratch with its SOURCE
//! buffer in the SAME window — no new pane, no other-pane scan — mirroring
//! `tests/preview.rs`.
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is modeled on `tests/preview.rs`, reusing
//! `tests/common/mod.rs` and the same loud SKIP-when-no-nvim guard.
//!
//! ## The surface under test
//!
//! On `WolfCoreHandle`:
//!   * `open_preview(&self) -> i64` — takes no `mode`; always acts in place
//!   * `edit_source(&self) -> i64` — takes no `mode`; always acts in place,
//!     returning the preview's SOURCE bufnr, or `0` off a preview pane / on error
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
/// (filetype detection, the in-place `nvim_win_set_buf`, the out-of-band
/// `wolf_preview` / `wolf_current_file` notifications) to settle. Mirrors
/// `preview.rs`'s `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors `preview.rs`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The lowest real WINDOW grid id (grid 1 is the outer/global grid; editor windows
/// are `>= 2` under `ext_multigrid`). Mirrors `preview.rs`.
const WINDOW_GRID_MIN: u64 = 2;

/// The single-pane window count: the in-place preview/edit pair never creates or
/// removes a pane.
const ONE_PANE: usize = 1;

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
    assert_eq!(
        got, target,
        "is_markdown_active() should be {target} ({desc})"
    );
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the core's `current_file()` ends with
/// `suffix` (a BASENAME match, robust to absolute-vs-cwd-relative display and to
/// `/var`<->`/private/var` symlinking), deterministically confirming that file is the
/// ACTIVE buffer. Copy of `preview.rs`'s helper. Unlike `is_markdown_active` — which
/// stays true across a markdown->markdown `:e` and so cannot witness the switch — this
/// only flips once the `wolf_current_file` notification for the new buffer has landed.
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

/// Blocks until `handle.is_preview_active()` equals `target`, then asserts it. The
/// EDIT analogue of [`wait_for_markdown_active`]: `is_preview_active()` reads the
/// snapshot's `current_is_preview`, re-asserted async via `wolf_current_file`, so a
/// synchronous read right after an in-place bind races that emit.
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

/// Blocks until `handle.active_win()` equals `target`, then asserts it. Focus and
/// window identity ride the async notification path, so a synchronous read right
/// after a typed `:vsplit` or an in-place bind can be stale.
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

/// Seeds a fresh temp cwd with a single real Markdown file `alpha.md`. Returns the
/// live `TempDir` (kept alive until after `shutdown()`) so it outlives the core as
/// nvim's working dir + search root.
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
    // Real switch barrier: the startup buffer is a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e alpha.md`. Wait until alpha.md is genuinely the ACTIVE
    // buffer before the SYNC preview/edit_source RPC races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");
    wait_for_preview_active(
        &handle,
        false,
        "a plain markdown editor pane is not a preview",
    );

    // --- 2. After the in-place open_preview(), the active pane IS a preview ---
    // The bind replaces the ACTIVE pane's buffer with the scratch, so no focus
    // cycling is needed: the pane under the cursor is the preview.
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
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let alpha_source_bufnr = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist")
        .source_bufnr();

    // --- 3. edit_source() replaces the preview with its source, IN PLACE ------
    // The async wolf_current_file notification (3rd arg = is_preview) must flip
    // is_preview_active() back to false — and is_markdown_active() back to true —
    // once the active buffer is the markdown source again.
    let returned = handle.edit_source();
    assert_eq!(
        returned, alpha_source_bufnr,
        "edit_source() on the active preview pane must return its SOURCE bufnr"
    );
    wait_for_preview_active(
        &handle,
        false,
        "replacing the active preview with its source flips is_preview_active back to false",
    );
    wait_for_markdown_active(
        &handle,
        true,
        "the pane shows the markdown source again, so is_markdown_active rises",
    );
    wait_for_previews(&handle, "previews() drops the edited pane's row", |p| {
        p.is_empty()
    });
    assert_eq!(
        handle.active_win(),
        active_win,
        "edit_source() must stay in the SAME window (in place, no focus move)",
    );
    assert_eq!(
        live_window_count(&handle),
        ONE_PANE,
        "edit_source() must NOT create a new pane (still one live window)",
    );

    // --- 4. Shutdown neutralizes both accessors ------------------------------
    handle.shutdown();
    assert!(
        !handle.is_preview_active(),
        "after shutdown is_preview_active() must be the neutral false"
    );
    assert_eq!(
        handle.edit_source(),
        0,
        "after shutdown edit_source() must be the neutral 0"
    );

    drop(tmp);
}

#[test]
fn edit_in_place_leaves_the_other_pane_alone() {
    // Two panes: one stays an alpha.md EDITOR throughout, the other becomes a
    // preview and is then edited back in place. Pins that the in-place edit never
    // creates a pane, never moves focus, and never touches the neighbour.
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. alpha.md, then a typed :vsplit into TWO editor panes -------------
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");
    // Real switch barrier: the startup buffer is a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e alpha.md`. Wait until alpha.md is genuinely the ACTIVE
    // buffer before the SYNC preview/edit_source RPC races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");
    send_ex_command(&handle, ":vsplit");
    wait_for_window_count(&handle, TWO_PANES, "after the typed :vsplit");

    // --- 2. Preview alpha.md IN PLACE in the active (new split) pane ----------
    // The preview WINDOW is read back from the previews() row rather than from a
    // pre-captured `active_win()`, which can lag the typed `:vsplit`.
    let preview_bufnr = handle.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {preview_bufnr}"
    );
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.len() == 1 && p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let preview_win = previews[0].win();
    assert!(
        preview_win > 0,
        "the preview row must carry a real window handle (> 0), got {preview_win}"
    );
    let alpha_source_bufnr = previews[0].source_bufnr();
    wait_for_preview_active(
        &handle,
        true,
        "the in-place preview leaves the preview pane current",
    );
    wait_for_active_win(
        &handle,
        preview_win,
        "the active window settles on the (in-place) preview pane",
    );

    // --- 3. edit_source(): the SAME pane flips back to the source ------------
    let returned = handle.edit_source();
    assert_eq!(
        returned, alpha_source_bufnr,
        "edit_source() must return the preview's SOURCE bufnr"
    );

    // The only preview was in `preview_win`; replacing it leaves previews() empty.
    wait_for_previews(
        &handle,
        "the replaced preview no longer lists a preview row",
        |p| p.is_empty(),
    );
    assert_eq!(
        handle.active_win(),
        preview_win,
        "edit_source() must stay in the SAME (now source) window",
    );
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "edit_source() must NOT create or close a pane (still two live windows)",
    );
    wait_for_preview_active(
        &handle,
        false,
        "the pane shows the source again, so is_preview_active falls",
    );
    wait_for_markdown_active(
        &handle,
        true,
        "the pane shows the markdown source again, so is_markdown_active rises",
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
    // Real switch barrier: the startup buffer is a markdown `Untitled.md`, so
    // `wait_for_markdown_active(true)` returns IMMEDIATELY (markdown->markdown) and
    // cannot witness the `:e alpha.md`. Wait until alpha.md is genuinely the ACTIVE
    // buffer before the SYNC edit_source RPC races ahead of the buffered `:e`.
    wait_for_current_file(&handle, "alpha.md");
    wait_for_preview_active(
        &handle,
        false,
        "a plain markdown editor pane is not a preview",
    );

    let windows_before = live_window_count(&handle);
    assert!(
        handle.previews().is_empty(),
        "no preview windows should exist before the negative edit_source: {:?}",
        handle
            .previews()
            .iter()
            .map(FfiPreview::win)
            .collect::<Vec<_>>(),
    );

    // --- 2. edit_source off a preview pane returns 0 and changes nothing -----
    let returned = handle.edit_source();
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
        handle
            .previews()
            .iter()
            .map(FfiPreview::win)
            .collect::<Vec<_>>(),
    );

    handle.shutdown();
    drop(tmp);
}
