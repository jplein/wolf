//! Integration test for `WolfCoreHandle::win_buffer(win) -> FfiWinBuffer`, the
//! per-window read the macOS state commit resolves each pane leaf through.
//!
//! The read is keyed by window handle and global across tabpages, unlike
//! `pane_status_for_grid`, which joins through the current-tabpage-only `windows` map
//! and answers `[No Name]` for a window on a background tabpage. A view that is not
//! its project's current view must still persist its files, so the test reads a
//! window after its tabpage has gone to the background.
//!
//! It drives a REAL, hermetic Neovim, with no mocks, reusing `tests/common/mod.rs`.
//!
//! ## Premise measured, not assumed
//!
//! That a background tabpage's window leaves the `windows` map is the reason this read
//! exists, so the test polls `grid_for_win(win1) == 0` before reading. If Neovim ever
//! kept the window bound, the premise poll fails, rather than the read passing for a
//! window the old join could reach anyway.
//!
//! ## SINGLE #[test]
//!
//! The flow is one `#[test]`, like `tabpages.rs`: it sets the process-global
//! `WOLF_STATE_DIR` to a temp dir for the shutdown flush, so a second test in this
//! binary would race it.
//!
//! ## SKIP-when-unset policy
//!
//! Loudly SKIPS (no-op pass) when no pinned nvim is available, via
//! `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup and each async effect to settle.
/// Mirrors the sibling tests.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The lowest real WINDOW grid id (grid 1 is the outer global grid under `ext_multigrid`).
const WINDOW_GRID_MIN: u64 = 2;

/// A neutral / "core is gone" tabpage handle or grid id.
const NEUTRAL: i64 = 0;

/// A window handle Neovim never allocates (they start at 1000 and count up).
const UNKNOWN_WIN: i64 = i64::MAX;

/// Polls `pred` until it returns `true` or [`FLUSH_TIMEOUT`] elapses; panics with
/// `desc` on timeout. Mirrors `tabpages.rs`.
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

/// The window handles of the live editor grids, from `windows()`. Same filter as
/// `tabpages.rs`'s `live_windows`.
fn live_wins(handle: &WolfCoreHandle) -> Vec<i64> {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .map(|w| w.win())
        .collect()
}

/// The canonical path string `win_buffer` reports for `path`.
fn canon(path: &Path) -> String {
    path.canonicalize()
        .expect("fixture should canonicalize")
        .to_string_lossy()
        .into_owned()
}

/// Whether `win_buffer(win)` reports `win` showing `expected`.
fn reports(handle: &WolfCoreHandle, win: i64, expected: &str) -> bool {
    let wb = handle.win_buffer(win);
    wb.reported() && wb.abs_path() == expected
}

#[test]
fn win_buffer_reads_a_background_tabpage_window() {
    let Some(_nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    let init_lua = common::init_lua_path();

    // Hermetic state dir for the shutdown flush (never the user's real state dir).
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    let cwd = tempfile::tempdir().expect("failed to create temp cwd directory");
    let file_a = cwd.path().join("alpha.md");
    let file_b = cwd.path().join("beta.md");
    std::fs::write(&file_a, "# Alpha\n").expect("failed to write alpha.md");
    std::fs::write(&file_b, "# Beta\n").expect("failed to write beta.md");
    let canon_a = canon(&file_a);
    let canon_b = canon(&file_b);

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    poll("the core to attach (read_frame cols > 0)", || {
        handle.read_frame().cols() > 0
    });
    poll("a single live editor window at startup", || {
        live_wins(&handle).len() == 1
    });
    let t1 = handle.current_tabpage();
    assert_ne!(
        t1, NEUTRAL,
        "current_tabpage() should be a nonzero first-view bind"
    );
    let win1 = live_wins(&handle)[0];

    // --- Tab 1 shows alpha.md.
    handle.open(file_a.to_string_lossy().into_owned());
    poll(
        "win_buffer(win1) to report alpha.md on its current tabpage",
        || reports(&handle, win1, &canon_a),
    );

    // --- Tab 2 becomes current and shows beta.md in a window of its own.
    let t2 = handle.create_tabpage(String::new());
    assert_ne!(t2, NEUTRAL, "create_tabpage should return a nonzero handle");
    assert_ne!(t2, t1, "the new tabpage handle must differ from T1");
    handle.set_current_tabpage(t2);
    poll("active_tabpage() to become T2 with a fresh window", || {
        handle.active_tabpage() == t2
            && live_wins(&handle).len() == 1
            && live_wins(&handle)[0] != win1
    });
    let win2 = live_wins(&handle)[0];
    handle.open(file_b.to_string_lossy().into_owned());
    // This emit follows the switch, so a per-tabpage sweep would already have dropped win1.
    poll("win_buffer(win2) to report beta.md on T2", || {
        reports(&handle, win2, &canon_b)
    });

    // --- The premise: win1 is on a background tabpage, out of the `windows` join.
    poll(
        "T2 current and win1 unbound from any grid (the background-tabpage premise)",
        || handle.active_tabpage() == t2 && handle.grid_for_win(win1) == 0,
    );

    let background = handle.win_buffer(win1);
    assert!(
        background.reported(),
        "win_buffer(win1) must report a window on a background tabpage: false means the read \
         joins through the current-tabpage windows map, so a background view persists no files"
    );
    assert_eq!(
        background.abs_path(),
        canon_a,
        "win_buffer(win1) must report win1's own buffer, alpha.md. beta.md means it read the \
         current window instead of win1; \"\" means it lost the path; anything else is not \
         the canonical path the persisted layout stores"
    );

    let unknown = handle.win_buffer(UNKNOWN_WIN);
    assert!(
        !unknown.reported(),
        "win_buffer must not report a window Neovim never had: true makes an unbound leaf \
         look reported, and its tab is persisted with an empty path"
    );
    assert_eq!(
        unknown.abs_path(),
        "",
        "an unreported window's abs_path must be empty"
    );

    handle.shutdown();
    let gone = handle.win_buffer(win1);
    assert!(
        !gone.reported(),
        "win_buffer must report nothing once the core is gone: true persists a dead core's \
         stale map"
    );
    assert_eq!(
        gone.abs_path(),
        "",
        "abs_path must be empty once the core is gone"
    );

    std::env::remove_var("WOLF_STATE_DIR");
    drop(cwd);
    drop(state_dir);
}
