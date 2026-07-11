//! Integration test for the `wolf-ffi` TABPAGE bridge surface (plan §2.1 + §5.2):
//! the per-project/tabpage-per-view handle methods the macOS front-end drives to
//! bind a view to an nvim tabpage, switch the current view, read a non-current
//! view's cwd, and close a view's tabpage (keeping the shared nvim alive until the
//! last view closes).
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is the sibling of `panes.rs` / `close_pane.rs` (same
//! staging / SKIP-when-unset machinery, reusing `tests/common/mod.rs`, and the same
//! `read_frame()` / `windows()` live-grid polling pattern).
//!
//! ## Dependency on coder work (plan §2.1)
//!
//! These `WolfCoreHandle` methods do NOT exist yet: the coder adds them to the
//! bridge (`extern "Rust"`) and `impl WolfCoreHandle` in `crates/wolf-ffi/src/lib.rs`
//! (backed by the new `wolf_core` tabpage API), regenerates `generated/`, AND adds
//! the `//crates/wolf-ffi:tabpages` `rust_test` target to `BUILD.bazel` (copy of
//! `close_pane`). This test file does NOT touch BUILD files, and it will not compile
//! until they land — that is expected:
//!
//!   * `create_tabpage(&self, cwd: String) -> i64` — `tabnew` (+ `:tcd cwd` when
//!     non-empty); the new tabpage handle (0 when the core is gone).
//!   * `current_tabpage(&self) -> i64` — the current tabpage handle via a blocking
//!     RPC (the first-view bind); 0 when gone.
//!   * `active_tabpage(&self) -> i64` — the CHEAP `Snapshot.current_tabpage` gate; 0
//!     when gone.
//!   * `cwd_for_tabpage(&self, tab: i64) -> String` — a tabpage's tab-local `:tcd`
//!     cwd (canonicalized); empty when gone / the handle is invalid.
//!   * `set_current_tabpage(&self, tab: i64)` / `close_tabpage(&self, tab: i64)` —
//!     no-ops when gone.
//!
//! ## SINGLE #[test]
//!
//! The whole flow lives in ONE `#[test]` (like `file_actions.rs`): it sets the
//! process-global `WOLF_STATE_DIR` (shutdown-flush hygiene) and drives one core
//! through create/switch/split-rebind/non-current-cwd/close, then a second `shutdown`
//! neutral-value check — with a single test there is nothing to race.
//!
//! ## SKIP-when-unset policy
//!
//! Loudly SKIPS (no-op pass) when no pinned nvim is available, via
//! `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (tabpage create/switch/close redraw) to settle. Mirrors the sibling tests.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- Window grids -------------------------------------------------------------
/// The lowest real WINDOW grid id (grid 1 is the outer global grid / cmdline
/// strip; editor windows are >= 2 under `ext_multigrid`).
const WINDOW_GRID_MIN: u64 = 2;

/// Expected live editor window grids on a single-window tabpage.
const ONE_WINDOW: usize = 1;
/// Expected live editor window grids after a `:vsplit`.
const TWO_WINDOWS: usize = 2;

/// A neutral / "core is gone" tabpage handle or grid id.
const NEUTRAL: i64 = 0;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`. Same as `panes.rs`.
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

/// Builds an Enter `FfiKeyEvent` (kind=Enter). Same as `panes.rs`.
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

/// Types `cmd` as an Ex command and presses Enter. `cmd` must include the leading
/// `:`. Same as `panes.rs`'s `send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// The live editor WINDOW grids (id + win handle) from `windows()`: grids >=
/// `WINDOW_GRID_MIN` that are not hidden. Copy of `panes.rs`'s `live_windows`.
fn live_windows(handle: &wolf_ffi::WolfCoreHandle) -> Vec<(u64, i64)> {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .map(|w| (w.grid(), w.win()))
        .collect()
}

/// Polls `pred` until it returns `true` or [`FLUSH_TIMEOUT`] elapses; panics with
/// `desc` on timeout. Mirrors the sibling tests' inline poll loops.
fn poll(desc: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        if pred() {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
}

/// Canonicalizes a path string for a symlink-robust comparison (macOS `/var` ->
/// `/private/var`); falls back to the raw path if canonicalization fails.
fn canon(p: &str) -> std::path::PathBuf {
    Path::new(p)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(p).to_path_buf())
}

#[test]
fn tabpage_lifecycle_over_the_bridge() {
    // Loud SKIP (no-op pass) when no pinned nvim is available. Otherwise export the
    // resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Hermetic state dir for the shutdown flush (never the user's real state dir).
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    // The core's cwd (its `<cwd>/.wolf/` MRU cache must outlive the session).
    let cwd = tempfile::tempdir().expect("failed to create temp cwd directory");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach + first-view bind ----------------------------------------
    poll("the core to attach (read_frame cols > 0)", || {
        handle.read_frame().cols() > 0
    });
    poll("a single live editor window at startup", || {
        live_windows(&handle).len() == ONE_WINDOW
    });

    let t1 = handle.current_tabpage();
    assert_ne!(t1, NEUTRAL, "current_tabpage() should be a nonzero first-view bind");
    // active_tabpage() (the cheap snapshot gate) should also report T1 after attach.
    poll("active_tabpage() to report T1", || handle.active_tabpage() == t1);
    let w1 = live_windows(&handle)[0].1;

    // --- 2. create_tabpage(dirB) -> new tabpage current ----------------------
    let dir_b = tempfile::tempdir().expect("failed to create dir B");
    let canon_b = dir_b
        .path()
        .canonicalize()
        .expect("dir B should canonicalize");
    let t2 = handle.create_tabpage(dir_b.path().to_string_lossy().into_owned());
    assert_ne!(t2, NEUTRAL, "create_tabpage should return a nonzero handle");
    assert_ne!(t2, t1, "the new tabpage handle must differ from T1");

    // active_tabpage() moves to T2 and a FRESH window (distinct from w1) appears
    // and reads a non-empty per-grid frame.
    poll("active_tabpage() to become T2", || handle.active_tabpage() == t2);
    poll("a fresh live window on T2 (distinct from w1)", || {
        let live = live_windows(&handle);
        live.len() == ONE_WINDOW
            && live[0].1 != w1
            && handle.read_frame_for_grid(live[0].0).cols() > 0
    });

    // --- 3. Switch back to T1; T2's window unbinds, T1's rebinds -------------
    handle.set_current_tabpage(t1);
    poll(
        "active_tabpage() to return to T1 with w1 rebound to a live grid",
        || handle.active_tabpage() == t1 && handle.grid_for_win(w1) != 0,
    );
    assert_ne!(
        handle.grid_for_win(w1),
        0,
        "grid_for_win(w1) should re-resolve to a live grid once T1 is current again"
    );

    // --- 4. Grid re-bind contract across a split + switch (OD2) --------------
    // Split T1 into two windows, switch away to T2 and back, and assert each window
    // handle re-resolves via grid_for_win to a live, paintable grid.
    send_ex_command(&handle, ":vsplit");
    poll("T1 to reach two live windows after :vsplit", || {
        live_windows(&handle).len() == TWO_WINDOWS
    });
    let split_wins: Vec<i64> = live_windows(&handle).iter().map(|(_, w)| *w).collect();

    handle.set_current_tabpage(t2);
    poll("active_tabpage() to become T2 again", || handle.active_tabpage() == t2);
    handle.set_current_tabpage(t1);
    poll("active_tabpage() to return to T1 with two windows again", || {
        handle.active_tabpage() == t1 && live_windows(&handle).len() == TWO_WINDOWS
    });

    for win in &split_wins {
        let grid = handle.grid_for_win(*win);
        assert_ne!(
            grid, 0,
            "grid_for_win({win}) must re-resolve to a live grid after the switch back (OD2)"
        );
        assert!(
            handle.read_frame_for_grid(grid).cols() > 0,
            "the rebound grid {grid} for window {win} should read a non-empty frame"
        );
    }

    // --- 5. cwd_for_tabpage for a NON-current tabpage ------------------------
    // T1 is current; T2 (rooted at dirB via :tcd) is NOT. Its tab-local cwd is
    // still readable and equals dirB.
    let reported_b = handle.cwd_for_tabpage(t2);
    assert!(
        !reported_b.is_empty(),
        "cwd_for_tabpage(T2) should be non-empty for a live non-current tabpage"
    );
    assert_eq!(
        canon(&reported_b),
        canon_b,
        "cwd_for_tabpage(T2) should be dirB for the NON-current tabpage \
         (reported={reported_b:?})"
    );

    // --- 6. Last-tabpage-close vs shutdown -----------------------------------
    // Two tabpages exist (T1, T2). Close the (non-current) T2: the core stays alive
    // and T2's handle goes invalid (cwd_for_tabpage(T2) -> "").
    handle.close_tabpage(t2);
    poll("T2's handle to become invalid after close_tabpage(T2)", || {
        handle.cwd_for_tabpage(t2).is_empty()
    });
    // The core is still alive: T1 is still readable.
    assert_ne!(
        handle.current_tabpage(),
        NEUTRAL,
        "the core must stay alive after closing a non-last tabpage"
    );

    // Now shut the whole core down and confirm subsequent handle calls return
    // neutral values (core gone).
    handle.shutdown();
    assert_eq!(
        handle.active_tabpage(),
        NEUTRAL,
        "active_tabpage() should be 0 after shutdown (core gone)"
    );
    assert_eq!(
        handle.current_tabpage(),
        NEUTRAL,
        "current_tabpage() should be 0 after shutdown (core gone)"
    );
    assert_eq!(
        handle.read_frame().cols(),
        0,
        "read_frame() should be an empty 0x0 frame after shutdown (core gone)"
    );
    assert!(
        handle.cwd_for_tabpage(t1).is_empty(),
        "cwd_for_tabpage() should be empty after shutdown (core gone)"
    );

    std::env::remove_var("WOLF_STATE_DIR");
    drop(dir_b);
    drop(cwd);
    drop(state_dir);
}
