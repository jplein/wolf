//! Integration test for the `wolf-ffi` state-persistence bridge surface, rewritten
//! for the TREE-shaped snapshot model (plan Phase 2 + OD3). The target name stays
//! `//crates/wolf-ffi:state`.
//!
//! ## What this drives
//!
//! The NEW write-side snapshot-builder free functions (pure Rust over the shared
//! `StateStore` + a `Mutex` scratch — NO live nvim needed):
//!
//!   * `wolf_ffi_state_snapshot_begin()` — clears the scratch tree.
//!   * `wolf_ffi_state_snapshot_add_window(x, y, width, height)` — pushes a window
//!     with an empty tab list.
//!   * `wolf_ffi_state_snapshot_add_tab(cwd, sidebar_visible)` — pushes a tab onto
//!     the most recently added window.
//!   * `wolf_ffi_state_snapshot_commit()` — clones the scratch into the shared
//!     store via `set_snapshot`.
//!   * `wolf_ffi_state_flush() -> bool` — forces the coalesced writer out.
//!
//! and reads them back through the load side:
//!
//!   * `wolf_ffi_load_state() -> FfiState`, `FfiState::windows() -> Vec<FfiWindowState>`
//!   * `FfiWindowState::x()/y()/width()/height() -> f64`, `tabs() -> Vec<FfiTabState>`
//!   * `FfiTabState::cwd() -> String`, `sidebar_visible() -> bool`
//!
//! The old granular handle setters (`state_set_geometry` /
//! `state_set_sidebar_visible` / `state_flush`) and the start-time StateStore
//! attach are GONE (OD1: the core no longer mirrors cwd), so this test needs NO
//! nvim — it is a pure builder round-trip, driven purely through `WOLF_STATE_DIR`
//! pointed at a `tempfile::tempdir()`, like `tests/config.rs`.
//!
//! ## SINGLE #[test]
//!
//! The whole flow lives in ONE `#[test]` because it mutates the process-global
//! `WOLF_STATE_DIR` env var (the `state_dir()` determinism hook) AND opens the
//! process-wide `SHARED_STORE` `OnceLock`: with a single test in this binary there
//! is nothing to race.

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{
    wolf_ffi_load_state, wolf_ffi_state_flush, wolf_ffi_state_snapshot_add_tab,
    wolf_ffi_state_snapshot_add_window, wolf_ffi_state_snapshot_begin,
    wolf_ffi_state_snapshot_commit,
};

/// Generous overall timeout for the coalesced writer to land the committed tree.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll cadence while waiting for the write to surface.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The on-disk state file name the shared store writes into `state_dir()`.
const STATE_FILE_NAME: &str = "state.json";

/// The number of windows the builder submits (window 0 + window 1).
const TWO_WINDOWS: usize = 2;
/// Window 0's tab count.
const W0_TABS: usize = 2;
/// Window 1's tab count.
const W1_TABS: usize = 1;

// -- Distinctive per-window geometry (each axis unique so a mixed-up slot shows) --
/// Window 0 geometry.
const W0_X: f64 = 100.0;
const W0_Y: f64 = 200.0;
const W0_WIDTH: f64 = 800.0;
const W0_HEIGHT: f64 = 600.0;
/// Window 1 geometry — deliberately different from window 0 on every axis.
const W1_X: f64 = 1300.0;
const W1_Y: f64 = 90.0;
const W1_WIDTH: f64 = 1024.0;
const W1_HEIGHT: f64 = 768.0;

// -- Distinctive per-tab cwds (literal strings; load never validates them) ------
const W0_TAB0_CWD: &str = "/proj/window-zero/tab-a";
const W0_TAB1_CWD: &str = "/proj/window-zero/tab-b";
const W1_TAB0_CWD: &str = "/proj/window-one/only-tab";

#[test]
fn snapshot_builder_round_trips_the_whole_tree() {
    // Point the state machinery at a private tempdir for the WHOLE test. This is
    // the `state_dir()` determinism hook AND where the shared store's writer lands
    // state.json; safe to mutate because this binary has exactly one test.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());
    let state_path = state_dir.path().join(STATE_FILE_NAME);

    // --- 1. Pre-build restore read on an EMPTY dir -> zero windows -----------
    let empty = wolf_ffi_load_state();
    assert!(
        empty.windows().is_empty(),
        "wolf_ffi_load_state() over an empty state dir must yield 0 windows"
    );
    assert!(
        !state_path.exists(),
        "the restore read must NEVER create state.json"
    );

    // --- 2. Build a 2-window / 3-tab tree via the snapshot builder -----------
    // Window 0 gets TWO tabs, window 1 gets ONE (tabs attach to the most recently
    // added window), interleaved exactly as the coordinator's snapshot walk emits.
    wolf_ffi_state_snapshot_begin();
    wolf_ffi_state_snapshot_add_window(W0_X, W0_Y, W0_WIDTH, W0_HEIGHT);
    wolf_ffi_state_snapshot_add_tab(W0_TAB0_CWD.to_string(), true);
    wolf_ffi_state_snapshot_add_tab(W0_TAB1_CWD.to_string(), false);
    wolf_ffi_state_snapshot_add_window(W1_X, W1_Y, W1_WIDTH, W1_HEIGHT);
    wolf_ffi_state_snapshot_add_tab(W1_TAB0_CWD.to_string(), false);
    wolf_ffi_state_snapshot_commit();

    // Force the coalesced writer out, then poll the load side until the tree lands.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let restored = loop {
        assert!(
            wolf_ffi_state_flush(),
            "wolf_ffi_state_flush() must complete (true) within {FLUSH_TIMEOUT:?}"
        );
        let restored = wolf_ffi_load_state();
        if restored.windows().len() == TWO_WINDOWS {
            break restored;
        }
        if Instant::now() >= deadline {
            break restored;
        }
        sleep(POLL_INTERVAL);
    };

    // --- 3. Assert the whole tree round-trips through the accessors ----------
    let windows = restored.windows();
    assert_eq!(
        windows.len(),
        TWO_WINDOWS,
        "the committed snapshot must load back as exactly two windows (got {})",
        windows.len()
    );

    // Window 0: geometry + two tabs in order.
    let w0 = &windows[0];
    assert_eq!(w0.x(), W0_X, "window 0 x");
    assert_eq!(w0.y(), W0_Y, "window 0 y");
    assert_eq!(w0.width(), W0_WIDTH, "window 0 width");
    assert_eq!(w0.height(), W0_HEIGHT, "window 0 height");
    let w0_tabs = w0.tabs();
    assert_eq!(w0_tabs.len(), W0_TABS, "window 0 tab count");
    assert_eq!(w0_tabs[0].cwd(), W0_TAB0_CWD, "window 0 tab 0 cwd");
    assert!(w0_tabs[0].sidebar_visible(), "window 0 tab 0 sidebar_visible");
    assert_eq!(w0_tabs[1].cwd(), W0_TAB1_CWD, "window 0 tab 1 cwd");
    assert!(!w0_tabs[1].sidebar_visible(), "window 0 tab 1 sidebar_visible");

    // Window 1: geometry + a single tab.
    let w1 = &windows[1];
    assert_eq!(w1.x(), W1_X, "window 1 x");
    assert_eq!(w1.y(), W1_Y, "window 1 y");
    assert_eq!(w1.width(), W1_WIDTH, "window 1 width");
    assert_eq!(w1.height(), W1_HEIGHT, "window 1 height");
    let w1_tabs = w1.tabs();
    assert_eq!(w1_tabs.len(), W1_TABS, "window 1 tab count");
    assert_eq!(w1_tabs[0].cwd(), W1_TAB0_CWD, "window 1 tab 0 cwd");
    assert!(!w1_tabs[0].sidebar_visible(), "window 1 tab 0 sidebar_visible");

    // The on-disk form is pretty-printed version-1 JSON.
    let text = std::fs::read_to_string(&state_path).expect("state.json should exist after commit");
    assert!(
        text.contains("\"version\": 1"),
        "state.json must carry a `\"version\": 1` header, got:\n{text}"
    );
    assert!(
        text.contains("\n  "),
        "state.json must be pretty-printed (newline + indentation), got:\n{text}"
    );

    // --- 4. Old (flat, pre-tabs) format is rejected -> zero windows ----------
    // The committed tree is now stable on disk (flush returned). Overwrite it with
    // a FLAT, pre-tabs file (window-level cwd/sidebar, NO `tabs`); the new schema
    // requires `tabs`, so `wolf_ffi_load_state()` must silently yield 0 windows.
    // No further `commit` fires, so the shared store's writer never re-touches it.
    const FLAT_STATE_JSON: &str = r#"{
  "version": 1,
  "windows": [
    {
      "x": 128.0,
      "y": 64.0,
      "width": 1180.0,
      "height": 760.0,
      "cwd": "/legacy/window-zero",
      "sidebar_visible": true
    }
  ]
}
"#;
    std::fs::write(&state_path, FLAT_STATE_JSON).expect("failed to seed the flat state.json");
    let rejected = wolf_ffi_load_state();
    assert!(
        rejected.windows().is_empty(),
        "wolf_ffi_load_state() over a flat (pre-tabs) state.json must yield 0 windows"
    );
    // The rejected file is left byte-for-byte intact (load never rewrites it).
    assert_eq!(
        std::fs::read_to_string(&state_path).expect("failed to re-read the flat state.json"),
        FLAT_STATE_JSON,
        "the restore read must preserve a rejected flat file byte-for-byte"
    );

    std::env::remove_var("WOLF_STATE_DIR");
    drop(state_dir);
}
