//! Integration tests for the per-pane statusbar's CORE surface: the snapshot now
//! carries a per-window buffer-path map (`Snapshot.buffers`, plan §2) fed by the
//! `wolf_buffers` notification (plan §3), and `WolfCore::pane_status(grid)` joins a
//! grid -> window -> buffer path and formats it via the pure `pane_status` fn
//! (plan §4/§5).
//!
//! These drive a REAL, hermetic nvim (no mocks) via `WOLF_NVIM_BIN`; the common
//! helpers PANIC loudly if it is unset (matching the rest of the wolf-core suite —
//! wolf-core has no SKIP path, unlike wolf-ffi). They mirror `current_file.rs` /
//! `redraw_multigrid.rs`: because the `wolf_buffers` notification arrives SEPARATE
//! from redraw flushes and per-window layout changes lag across several flushes,
//! every assertion POLLS with a generous timeout, draining wakes as it goes.
//!
//! ## Dependency on coder work (plan §5)
//!
//! `WolfCore::pane_status(grid: u64) -> PaneStatus` does not exist yet. The coder
//! adds it in `crates/wolf-core/src/api.rs`, plus `Snapshot.buffers`, the
//! `wolf_buffers` handler, `setup_buffer_tracking`, and the Lua tracker; and adds
//! the `//crates/wolf-core:pane_buffers` `rust_test` target to `BUILD.bazel`. This
//! file does NOT touch non-test source.
//!
//! ## Determinism of the cwd-relative display path
//!
//! To make `display_path` deterministic these tests set an explicit temp `cwd`
//! (`common::config_in`) and open files INSIDE it, then compute the expected
//! relative form by canonicalizing BOTH sides (macOS `/var`/`/tmp` are symlinks,
//! and the core canonicalizes cwd + buffer paths).

mod common;

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key, Snapshot, WolfCore, NO_NAME};

/// Placeholder content for the temp markdown files opened by these tests.
const DOC: &str = "# Pane Buffers\nbody\n";

/// The outer/global grid id. Window content lives on grids >= 2; grid 1 is never
/// the active window grid in these scenarios (mirrors `redraw_multigrid.rs`).
const GLOBAL_GRID: u64 = 1;

/// Gap between polls while waiting for a structural / notification change.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long to let the initial `wolf_buffers` emit settle before asserting that an
/// unnamed buffer reports `[No Name]`. Generous so a late emit can't sneak a false
/// positive past the assertion (mirrors `current_file.rs`'s `UNNAMED_SETTLE`).
const UNNAMED_SETTLE: Duration = Duration::from_secs(2);

/// Starts a `WolfCore` rooted at an explicit `cwd` (so files opened inside it have
/// a deterministic cwd-relative display path) and waits for the initial flush.
fn start_core_in(cwd: PathBuf) -> (WolfCore, FlushWaiter) {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(cwd, common::DEFAULT_COLS, common::DEFAULT_ROWS),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter)
}

/// Polls `core`'s snapshot until `pred` holds, draining wakes each iteration, or
/// panics after [`FLUSH_TIMEOUT`] (mirrors `redraw_multigrid.rs`).
fn poll_until(core: &WolfCore, waiter: &FlushWaiter, desc: &str, pred: impl Fn(&Snapshot) -> bool) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        {
            let snap = core.read_snapshot();
            if pred(&snap) {
                return;
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Polls `core.pane_status(grid)` until `pred` holds, or panics after
/// [`FLUSH_TIMEOUT`]. The last observed status is reported in the panic (formatted
/// field-wise so this test does not depend on `PaneStatus: Debug`).
fn poll_pane_status(
    core: &WolfCore,
    waiter: &FlushWaiter,
    grid: u64,
    desc: &str,
    pred: impl Fn(&str, &str) -> bool,
) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let st = core.pane_status(grid);
        if pred(&st.filename, &st.display_path) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for {desc}; last observed \
                 (filename={:?}, display_path={:?})",
                st.filename, st.display_path
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Waits for an active WINDOW grid (id != 1) to appear and returns it.
fn wait_for_active_window_grid(core: &WolfCore, waiter: &FlushWaiter) -> u64 {
    poll_until(
        core,
        waiter,
        "an active window grid (id != 1) to appear",
        |snap| !snap.windows.is_empty() && snap.active_grid_id() != GLOBAL_GRID,
    );
    core.read_snapshot().active_grid_id()
}

/// Polls until at least two window grids report the two given filenames, and
/// returns `(grid_for_a, grid_for_b)`. Proves the buffer map is PER-WINDOW.
fn wait_two_grids_by_name(
    core: &WolfCore,
    waiter: &FlushWaiter,
    name_a: &str,
    name_b: &str,
) -> (u64, u64) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let grids: Vec<u64> = core.read_snapshot().windows.keys().copied().collect();
        if grids.len() >= 2 {
            let mut grid_a = None;
            let mut grid_b = None;
            for &g in &grids {
                let name = core.pane_status(g).filename;
                if name == name_a {
                    grid_a = Some(g);
                } else if name == name_b {
                    grid_b = Some(g);
                }
            }
            if let (Some(a), Some(b)) = (grid_a, grid_b) {
                return (a, b);
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for two window grids reporting \
                 {name_a:?} and {name_b:?}; grids seen: {grids:?}"
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Burns `dur` draining wakes, so a late `wolf_buffers` emit lands BEFORE a
/// "nothing changed" assertion runs.
fn settle(waiter: &FlushWaiter, dur: Duration) {
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Resolves the nvim WINDOW handle backing `grid`, polling until it appears.
fn wait_for_win_handle(core: &WolfCore, waiter: &FlushWaiter, grid: u64) -> i64 {
    poll_until(
        core,
        waiter,
        &format!("a window handle for grid {grid}"),
        |snap| snap.windows.get(&grid).is_some_and(|w| w.win != 0),
    );
    core.read_snapshot()
        .windows
        .get(&grid)
        .map(|w| w.win)
        .expect("the grid must carry a window handle")
}

/// Types an Ex command `:<cmd><CR>` one key at a time (mirrors `redraw_multigrid.rs`).
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// The cwd-relative form the coder's `pane_status` should produce for `file`
/// opened under `cwd`, computed by canonicalizing BOTH sides (the core does the
/// same, and `/var`/`/tmp` are symlinks on macOS).
fn expected_cwd_relative(cwd: &PathBuf, file: &PathBuf) -> String {
    let cwd_canon = std::fs::canonicalize(cwd).expect("cwd should canonicalize");
    let file_canon = std::fs::canonicalize(file).expect("file should canonicalize");
    file_canon
        .strip_prefix(&cwd_canon)
        .expect("the opened file must live under cwd")
        .to_string_lossy()
        .into_owned()
}

/// Opening a file nested under cwd makes `pane_status(active_grid)` report its base
/// name as `filename` and the cwd-RELATIVE remainder as `display_path`.
#[test]
fn open_reports_filename_and_cwd_relative_path() {
    let cwd = common::fresh_cwd();
    let docs = cwd.join("docs");
    std::fs::create_dir_all(&docs).expect("failed to create docs subdir");
    let file = docs.join("note.md");
    std::fs::write(&file, DOC).expect("failed to write note.md");

    let (core, waiter) = start_core_in(cwd.clone());
    core.send_input(InputEvent::Open { path: file.clone() });

    let grid = wait_for_active_window_grid(&core, &waiter);
    let rel = expected_cwd_relative(&cwd, &file);

    poll_pane_status(
        &core,
        &waiter,
        grid,
        &format!("pane_status filename == note.md and display_path == {rel:?}"),
        |filename, display| filename == "note.md" && display == rel,
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// PER-WINDOW proof: open A, `:vsplit`, then `:e` a DIFFERENT file B in the new
/// window; the two window grids must report DISTINCT filenames (A vs B), not one
/// global filename.
#[test]
fn split_windows_report_distinct_filenames() {
    let cwd = common::fresh_cwd();
    let file_a = cwd.join("alpha.md");
    let file_b = cwd.join("beta.md");
    std::fs::write(&file_a, DOC).expect("failed to write alpha.md");
    std::fs::write(&file_b, DOC).expect("failed to write beta.md");

    let (core, waiter) = start_core_in(cwd.clone());
    core.send_input(InputEvent::Open {
        path: file_a.clone(),
    });

    let grid = wait_for_active_window_grid(&core, &waiter);
    poll_pane_status(
        &core,
        &waiter,
        grid,
        "the initial window to report alpha.md before splitting",
        |filename, _| filename == "alpha.md",
    );

    // New split window; `:e beta.md` is resolved against nvim's cwd (== our cwd).
    ex_command(&core, "vsplit");
    ex_command(&core, "e beta.md");

    let (grid_a, grid_b) = wait_two_grids_by_name(&core, &waiter, "alpha.md", "beta.md");
    assert_ne!(
        grid_a, grid_b,
        "alpha.md and beta.md must live on DISTINCT window grids (per-window map)"
    );
    assert_eq!(core.pane_status(grid_a).filename, "alpha.md");
    assert_eq!(core.pane_status(grid_b).filename, "beta.md");

    core.shutdown().expect("shutdown should be Ok");
}

/// A genuinely unnamed buffer (empty `nvim_buf_get_name`) is absent from the buffer
/// map, so `pane_status(grid)` reports `[No Name]` with an EMPTY display path.
///
/// The startup buffer is no longer unnamed — the anonymous-buffer feature names it
/// `Untitled.md` at `VimEnter` — so this drives a RAW `:enew` buffer to exercise the
/// `[No Name]` path. A raw `:enew` Ex command does NOT route through the
/// `wolf_new_untitled` naming helper (that helper is wired ONLY to the `NewBuffer`
/// InputEvent), so the resulting buffer keeps an empty name.
#[test]
fn unnamed_buffer_reports_no_name() {
    let cwd = common::fresh_cwd();
    let (core, waiter) = start_core_in(cwd);

    let grid = wait_for_active_window_grid(&core, &waiter);

    // Replace the (now named `Untitled.md`) startup buffer with a genuinely UNNAMED
    // buffer via a raw `:enew`. This bypasses the `wolf_new_untitled` naming helper,
    // so the buffer keeps an empty name and must report NO_NAME.
    ex_command(&core, "enew");

    // Wait until the `wolf_buffers` emit reflects the unnamed buffer (pane_status for
    // this grid reports NO_NAME rather than the pre-`:enew` `Untitled.md`).
    poll_pane_status(
        &core,
        &waiter,
        grid,
        "the raw :enew buffer to report NO_NAME",
        |filename, _| filename == NO_NAME,
    );

    // Let any late `wolf_buffers` emit settle, draining wakes, to be sure the unnamed
    // buffer STAYS at NO_NAME / "" (no stray rename sneaks in).
    let settle_deadline = Instant::now() + UNNAMED_SETTLE;
    while Instant::now() < settle_deadline {
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }

    let st = core.pane_status(grid);
    assert_eq!(
        st.filename, NO_NAME,
        "a genuinely unnamed buffer should report NO_NAME; got {:?}",
        st.filename
    );
    assert_eq!(
        st.display_path, "",
        "a genuinely unnamed buffer should report an empty display_path; got {:?}",
        st.display_path
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Closing one split drops THAT window's grid from `Snapshot.windows`, and the
/// SURVIVOR keeps reporting its own file. The focused (new/`beta`) window is the
/// one `:close` removes.
#[test]
fn closing_split_drops_that_window_survivor_keeps_file() {
    let cwd = common::fresh_cwd();
    let file_a = cwd.join("alpha.md");
    let file_b = cwd.join("beta.md");
    std::fs::write(&file_a, DOC).expect("failed to write alpha.md");
    std::fs::write(&file_b, DOC).expect("failed to write beta.md");

    let (core, waiter) = start_core_in(cwd.clone());
    core.send_input(InputEvent::Open {
        path: file_a.clone(),
    });

    let grid = wait_for_active_window_grid(&core, &waiter);
    poll_pane_status(
        &core,
        &waiter,
        grid,
        "the initial window to report alpha.md before splitting",
        |filename, _| filename == "alpha.md",
    );

    ex_command(&core, "vsplit");
    ex_command(&core, "e beta.md");

    let (alpha_grid, beta_grid) = wait_two_grids_by_name(&core, &waiter, "alpha.md", "beta.md");

    // Close the focused (beta) window; its grid must vanish from `windows`.
    ex_command(&core, "close");
    poll_until(
        &core,
        &waiter,
        "the closed (beta) window's grid to drop out of windows",
        |snap| snap.windows.len() == 1 && !snap.windows.contains_key(&beta_grid),
    );

    // The survivor (alpha) still reports its own file.
    poll_pane_status(
        &core,
        &waiter,
        alpha_grid,
        "the surviving window to still report alpha.md",
        |filename, _| filename == "alpha.md",
    );

    let survivors: HashSet<u64> = core.read_snapshot().windows.keys().copied().collect();
    assert!(
        !survivors.contains(&beta_grid),
        "the closed window grid {beta_grid} must be gone; survivors: {survivors:?}"
    );
    assert!(
        survivors.contains(&alpha_grid),
        "the surviving window grid {alpha_grid} must remain; survivors: {survivors:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// =========================================================================
// REGRESSION GUARDS for the nav-history `wolf_buffers` tuple extension (§B5).
//
// `wolf_buffers` grows from a 2-element `{ win, name }` tuple to a 5-element
// `{ win, name, bufnr, is_terminal, is_drawer }` tuple feeding a NEW parallel
// `Snapshot.win_buffers` map. `Snapshot.buffers` — and therefore
// `WolfCore::pane_status(grid)`, its ONLY production consumer — must stay
// BEHAVIORALLY IDENTICAL. These two tests pin that contract directly, at the map
// level rather than only through the formatted display strings above.
// =========================================================================

/// `Snapshot.buffers` still holds CANONICALIZED paths keyed by WINDOW handle and
/// still OMITS unnamed windows; `pane_status(grid)` is unchanged for a NAMED
/// buffer, an UNNAMED buffer, and a grid with NO window at all.
#[test]
fn buffers_map_stays_canonicalized_and_omits_unnamed_windows() {
    /// A grid id no window will ever draw into.
    const ABSENT_GRID: u64 = 9999;

    let cwd = common::fresh_cwd();
    let docs = cwd.join("docs");
    std::fs::create_dir_all(&docs).expect("failed to create docs subdir");
    let file = docs.join("note.md");
    std::fs::write(&file, DOC).expect("failed to write note.md");

    let (core, waiter) = start_core_in(cwd.clone());
    core.send_input(InputEvent::Open { path: file.clone() });

    let grid = wait_for_active_window_grid(&core, &waiter);
    let win = wait_for_win_handle(&core, &waiter, grid);
    let canon = std::fs::canonicalize(&file).expect("note.md should canonicalize");

    // (a) Keyed by WINDOW handle, holding the CANONICAL path.
    poll_until(
        &core,
        &waiter,
        "buffers[win] to equal the canonical note.md path",
        |snap| snap.buffers.get(&win) == Some(&canon),
    );

    // (b) pane_status for a NAMED buffer is unchanged.
    let rel = expected_cwd_relative(&cwd, &file);
    let st = core.pane_status(grid);
    assert_eq!(st.filename, "note.md", "filename is still the base name");
    assert_eq!(
        st.display_path, rel,
        "display_path is still the cwd-relative remainder"
    );
    assert_eq!(
        st.abs_path,
        canon.to_string_lossy(),
        "abs_path is still the raw canonical path (persisted as a layout leaf)"
    );

    // (c) An UNNAMED window is still ABSENT from the map.
    ex_command(&core, "enew");
    poll_until(
        &core,
        &waiter,
        "the now-unnamed window to drop out of buffers",
        |snap| !snap.buffers.contains_key(&win),
    );
    settle(&waiter, UNNAMED_SETTLE);
    assert!(
        !core.read_snapshot().buffers.contains_key(&win),
        "a genuinely unnamed buffer keeps its window OUT of the buffers map"
    );
    let st = core.pane_status(grid);
    assert_eq!(
        st.filename, NO_NAME,
        "an unnamed buffer still reports NO_NAME"
    );
    assert_eq!(st.display_path, "", "…with an empty display_path");
    assert_eq!(st.abs_path, "", "…and an empty abs_path");

    // (d) A grid with NO window is still the [No Name] shape.
    let st = core.pane_status(ABSENT_GRID);
    assert_eq!(
        st.filename, NO_NAME,
        "a grid with no window still reports NO_NAME"
    );
    assert_eq!(st.display_path, "", "…with an empty display_path");
    assert_eq!(st.abs_path, "", "…and an empty abs_path");

    core.shutdown().expect("shutdown should be Ok");
}

/// Wholesale replacement still DROPS a closed window's entry, and PREVIEW scratch
/// panes are still SKIPPED by the emit (so their grid reports the `[No Name]`
/// shape from `pane_status`).
#[test]
fn buffers_map_skips_preview_panes_and_drops_closed_windows() {
    let cwd = common::fresh_cwd();
    let file_a = cwd.join("alpha.md");
    let file_b = cwd.join("beta.md");
    std::fs::write(&file_a, DOC).expect("failed to write alpha.md");
    std::fs::write(&file_b, DOC).expect("failed to write beta.md");

    let (core, waiter) = start_core_in(cwd);
    core.send_input(InputEvent::Open {
        path: file_a.clone(),
    });

    let grid = wait_for_active_window_grid(&core, &waiter);
    let win_a = wait_for_win_handle(&core, &waiter, grid);
    poll_pane_status(
        &core,
        &waiter,
        grid,
        "the initial window to report alpha.md before splitting",
        |filename, _| filename == "alpha.md",
    );

    // (a) A closed window's entry vanishes with the next wholesale replacement.
    ex_command(&core, "vsplit");
    ex_command(&core, "e beta.md");
    let (_alpha_grid, beta_grid) = wait_two_grids_by_name(&core, &waiter, "alpha.md", "beta.md");
    let win_b = wait_for_win_handle(&core, &waiter, beta_grid);
    poll_until(
        &core,
        &waiter,
        "both windows to be present in the buffers map",
        |snap| snap.buffers.contains_key(&win_a) && snap.buffers.contains_key(&win_b),
    );

    ex_command(&core, "close");
    poll_until(
        &core,
        &waiter,
        "the closed window's entry to drop out of buffers",
        |snap| !snap.buffers.contains_key(&win_b),
    );
    assert!(
        core.read_snapshot().buffers.contains_key(&win_a),
        "the SURVIVING window keeps its buffers entry"
    );

    // (b) A preview scratch pane is still SKIPPED by the emit.
    // A typed `:vsplit` first (the in-place open_preview() binds into the ACTIVE
    // pane), so the surviving alpha.md editor window keeps its buffers entry while
    // the new split pane becomes the preview.
    ex_command(&core, "vsplit");
    poll_until(&core, &waiter, "two live windows after :vsplit", |snap| {
        snap.windows.len() >= 2
    });
    let preview_bufnr = core.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() on a markdown buffer must return a scratch bufnr (> 0), \
         got {preview_bufnr}"
    );
    poll_until(&core, &waiter, "a preview window to appear", |snap| {
        !snap.previews.is_empty()
    });
    let preview_win = *core
        .read_snapshot()
        .previews
        .keys()
        .next()
        .expect("a preview window handle must exist");
    poll_until(
        &core,
        &waiter,
        "the preview window's grid to be known",
        |snap| snap.windows.values().any(|w| w.win == preview_win),
    );
    let preview_grid = core
        .read_snapshot()
        .windows
        .values()
        .find(|w| w.win == preview_win)
        .map(|w| w.grid)
        .expect("the preview window must draw into a grid");

    settle(&waiter, UNNAMED_SETTLE);
    assert!(
        !core.read_snapshot().buffers.contains_key(&preview_win),
        "preview scratch panes are SKIPPED, so their window never enters buffers"
    );
    assert!(
        core.read_snapshot().buffers.contains_key(&win_a),
        "the alpha.md editor window keeps its buffers entry beside the preview pane"
    );
    let st = core.pane_status(preview_grid);
    assert_eq!(
        st.filename, NO_NAME,
        "a preview pane's grid still reports NO_NAME from pane_status"
    );
    assert_eq!(st.display_path, "", "…with an empty display_path");
    assert_eq!(st.abs_path, "", "…and an empty abs_path");

    core.shutdown().expect("shutdown should be Ok");
}
