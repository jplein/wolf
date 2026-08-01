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
    wolf_ffi_load_state, wolf_ffi_state_flush, wolf_ffi_state_layout_commit_to_tab,
    wolf_ffi_state_layout_push_leaf, wolf_ffi_state_layout_push_split,
    wolf_ffi_state_snapshot_add_tab, wolf_ffi_state_snapshot_add_window,
    wolf_ffi_state_snapshot_begin, wolf_ffi_state_snapshot_commit,
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

// -- Pane-layout push-builder fixture (feature plan §4.2/§4.3) ------------------
/// The FFI orientation discriminant for a VERTICAL split (0 = horizontal, 1 = vertical).
const ORIENT_VERTICAL: u8 = 1;
/// The number of children in the pushed split.
const TWO_CHILDREN: u32 = 2;
/// Each child's weight in the pushed 0.5/0.5 split.
const HALF_WEIGHT: f64 = 0.5;
/// The root node's weight (always 1.0 in the token stream).
const ROOT_WEIGHT: f64 = 1.0;
/// The two leaves' buffer paths in the pushed split (DFS order: left, then right).
const LEAF_LEFT_PATH: &str = "/proj/window-one/left.md";
const LEAF_RIGHT_PATH: &str = "/proj/window-one/right.md";

// -- Preview-leaf fixture (feature plan ROUND 2 — preview restore) --------------
/// The SOURCE markdown path carried by the PREVIEW leaf (preview: true). For a
/// preview leaf the path is its source file, so `is_preview()` + `leaf_path()`
/// together let the rebuild recreate a preview of this file.
const PREVIEW_SOURCE_PATH: &str = "/proj/window-one/preview-source.md";
/// The editor sibling's path in the preview split (preview: false).
const EDITOR_SIBLING_PATH: &str = "/proj/window-one/editor.md";

// -- Lone-leaf fixture (single-pane tab persistence) ----------------------------
/// The buffer path of a LONE editor leaf pushed with NO surrounding split, proving a
/// single-pane tab persists its open file as a single top-level leaf layout.
const SOLO_PATH: &str = "/proj/window-one/solo.md";

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
    assert!(
        w0_tabs[0].sidebar_visible(),
        "window 0 tab 0 sidebar_visible"
    );
    assert_eq!(w0_tabs[1].cwd(), W0_TAB1_CWD, "window 0 tab 1 cwd");
    assert!(
        !w0_tabs[1].sidebar_visible(),
        "window 0 tab 1 sidebar_visible"
    );

    // Window 1: geometry + a single tab.
    let w1 = &windows[1];
    assert_eq!(w1.x(), W1_X, "window 1 x");
    assert_eq!(w1.y(), W1_Y, "window 1 y");
    assert_eq!(w1.width(), W1_WIDTH, "window 1 width");
    assert_eq!(w1.height(), W1_HEIGHT, "window 1 height");
    let w1_tabs = w1.tabs();
    assert_eq!(w1_tabs.len(), W1_TABS, "window 1 tab count");
    assert_eq!(w1_tabs[0].cwd(), W1_TAB0_CWD, "window 1 tab 0 cwd");
    assert!(
        !w1_tabs[0].sidebar_visible(),
        "window 1 tab 0 sidebar_visible"
    );

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

    // --- 3b. Attach a pane layout to the last tab and round-trip it ----------
    // Rebuild the SAME 2-window / 3-tab tree, then attach a vertical 2-leaf split
    // to the LAST tab (window 1, tab 0) via the post-order layout push-builder:
    // push the two leaves, wrap them in a split, then commit-to-tab. The whole
    // scratch tree (now carrying the layout) is committed and read back through
    // the FfiTabState.has_layout()/layout() + FfiPaneNode accessors.
    wolf_ffi_state_snapshot_begin();
    wolf_ffi_state_snapshot_add_window(W0_X, W0_Y, W0_WIDTH, W0_HEIGHT);
    wolf_ffi_state_snapshot_add_tab(W0_TAB0_CWD.to_string(), true);
    wolf_ffi_state_snapshot_add_tab(W0_TAB1_CWD.to_string(), false);
    wolf_ffi_state_snapshot_add_window(W1_X, W1_Y, W1_WIDTH, W1_HEIGHT);
    wolf_ffi_state_snapshot_add_tab(W1_TAB0_CWD.to_string(), false);
    // Post-order token stream for split[ leaf(LEFT), leaf(RIGHT) ]: leaves first,
    // then the split that pops the top two. Both leaves are plain EDITOR leaves
    // (preview: false); the preview case is exercised in step 3c below.
    wolf_ffi_state_layout_push_leaf(LEAF_LEFT_PATH.to_string(), false, HALF_WEIGHT);
    wolf_ffi_state_layout_push_leaf(LEAF_RIGHT_PATH.to_string(), false, HALF_WEIGHT);
    wolf_ffi_state_layout_push_split(ORIENT_VERTICAL, TWO_CHILDREN, ROOT_WEIGHT);
    wolf_ffi_state_layout_commit_to_tab();
    wolf_ffi_state_snapshot_commit();

    // Flush + poll the load side until the last tab reports a layout.
    let layout_deadline = Instant::now() + FLUSH_TIMEOUT;
    let with_layout = loop {
        assert!(
            wolf_ffi_state_flush(),
            "wolf_ffi_state_flush() must complete after the layout commit"
        );
        let restored = wolf_ffi_load_state();
        let windows = restored.windows();
        if windows.len() == TWO_WINDOWS && windows[1].tabs().last().is_some_and(|t| t.has_layout())
        {
            break restored;
        }
        if Instant::now() >= layout_deadline {
            break restored;
        }
        sleep(POLL_INTERVAL);
    };

    let w1_windows = with_layout.windows();
    let w1_tabs = w1_windows[1].tabs();
    let last_tab = &w1_tabs[W1_TABS - 1];
    assert!(
        last_tab.has_layout(),
        "the last tab must report has_layout() == true after the layout commit"
    );

    // The root node is the vertical split.
    let root = last_tab.layout();
    assert!(!root.is_leaf(), "the root pane node is a split, not a leaf");
    assert_eq!(
        root.orientation(),
        ORIENT_VERTICAL,
        "the root split's orientation discriminant must round-trip as VERTICAL (1)"
    );
    assert_eq!(
        root.child_count(),
        TWO_CHILDREN,
        "the root split has two children"
    );
    assert_eq!(root.weight(0), HALF_WEIGHT, "child 0 weight round-trips");
    assert_eq!(root.weight(1), HALF_WEIGHT, "child 1 weight round-trips");

    // Both children are leaves whose paths round-trip in DFS order.
    let left = root.child(0);
    assert!(left.is_leaf(), "child 0 is a leaf");
    assert_eq!(left.leaf_path(), LEAF_LEFT_PATH, "child 0 leaf path");
    let right = root.child(1);
    assert!(right.is_leaf(), "child 1 is a leaf");
    assert_eq!(right.leaf_path(), LEAF_RIGHT_PATH, "child 1 leaf path");

    // --- 3c. A PREVIEW leaf round-trips its is_preview() marker + source path -
    // Rebuild the SAME tree, but attach a vertical split to the LAST tab whose
    // FIRST child is a PREVIEW leaf (source path, preview=true) and whose second
    // child is a plain EDITOR leaf (preview=false). After load, walk the FfiPaneNode
    // and assert is_preview() true/false per child and that the preview leaf's
    // leaf_path() is the pushed SOURCE path.
    wolf_ffi_state_snapshot_begin();
    wolf_ffi_state_snapshot_add_window(W0_X, W0_Y, W0_WIDTH, W0_HEIGHT);
    wolf_ffi_state_snapshot_add_tab(W0_TAB0_CWD.to_string(), true);
    wolf_ffi_state_snapshot_add_tab(W0_TAB1_CWD.to_string(), false);
    wolf_ffi_state_snapshot_add_window(W1_X, W1_Y, W1_WIDTH, W1_HEIGHT);
    wolf_ffi_state_snapshot_add_tab(W1_TAB0_CWD.to_string(), false);
    // Post-order: preview leaf (preview=true), then editor sibling (preview=false),
    // then the split that pops the top two.
    wolf_ffi_state_layout_push_leaf(PREVIEW_SOURCE_PATH.to_string(), true, HALF_WEIGHT);
    wolf_ffi_state_layout_push_leaf(EDITOR_SIBLING_PATH.to_string(), false, HALF_WEIGHT);
    wolf_ffi_state_layout_push_split(ORIENT_VERTICAL, TWO_CHILDREN, ROOT_WEIGHT);
    wolf_ffi_state_layout_commit_to_tab();
    wolf_ffi_state_snapshot_commit();

    // Flush + poll until the last tab's layout reports a preview first child.
    let preview_deadline = Instant::now() + FLUSH_TIMEOUT;
    let with_preview = loop {
        assert!(
            wolf_ffi_state_flush(),
            "wolf_ffi_state_flush() must complete after the preview-layout commit"
        );
        let restored = wolf_ffi_load_state();
        let windows = restored.windows();
        if windows.len() == TWO_WINDOWS
            && windows[1]
                .tabs()
                .last()
                .is_some_and(|t| t.has_layout() && t.layout().child(0).is_preview())
        {
            break restored;
        }
        if Instant::now() >= preview_deadline {
            break restored;
        }
        sleep(POLL_INTERVAL);
    };

    let pv_windows = with_preview.windows();
    let pv_tab = &pv_windows[1].tabs()[W1_TABS - 1];
    assert!(
        pv_tab.has_layout(),
        "the last tab must report has_layout() == true after the preview commit"
    );
    let pv_root = pv_tab.layout();
    assert_eq!(
        pv_root.child_count(),
        TWO_CHILDREN,
        "the preview split has two children"
    );

    // Child 0: the PREVIEW leaf — is_preview() true, path is the SOURCE path.
    let pv_left = pv_root.child(0);
    assert!(pv_left.is_leaf(), "the preview child is a leaf");
    assert!(
        pv_left.is_preview(),
        "the preview child must report is_preview() == true"
    );
    assert_eq!(
        pv_left.leaf_path(),
        PREVIEW_SOURCE_PATH,
        "the preview leaf's path is its SOURCE markdown path"
    );

    // Child 1: the EDITOR sibling — is_preview() false, its own path.
    let pv_right = pv_root.child(1);
    assert!(pv_right.is_leaf(), "the editor sibling is a leaf");
    assert!(
        !pv_right.is_preview(),
        "the editor sibling must report is_preview() == false"
    );
    assert_eq!(
        pv_right.leaf_path(),
        EDITOR_SIBLING_PATH,
        "the editor sibling keeps its own buffer path"
    );

    // --- 3d. A LONE leaf (single-pane tab) round-trips as a top-level leaf ----
    // Rebuild the SAME tree, but attach a SINGLE editor leaf (NO split) to the LAST
    // tab: push one leaf, then commit-to-tab. This is the single-pane persistence
    // case — the layout's root is the leaf itself, not a split. After load, assert
    // the last tab has_layout(), its root is a (non-preview) leaf, and its path is
    // the pushed SOLO_PATH.
    wolf_ffi_state_snapshot_begin();
    wolf_ffi_state_snapshot_add_window(W0_X, W0_Y, W0_WIDTH, W0_HEIGHT);
    wolf_ffi_state_snapshot_add_tab(W0_TAB0_CWD.to_string(), true);
    wolf_ffi_state_snapshot_add_tab(W0_TAB1_CWD.to_string(), false);
    wolf_ffi_state_snapshot_add_window(W1_X, W1_Y, W1_WIDTH, W1_HEIGHT);
    wolf_ffi_state_snapshot_add_tab(W1_TAB0_CWD.to_string(), false);
    // A single editor leaf with NO surrounding split: push the lone leaf, then
    // commit it directly to the tab as the layout's root.
    wolf_ffi_state_layout_push_leaf(SOLO_PATH.to_string(), false, ROOT_WEIGHT);
    wolf_ffi_state_layout_commit_to_tab();
    wolf_ffi_state_snapshot_commit();

    // Flush + poll until the last tab reports a (leaf) layout.
    let solo_deadline = Instant::now() + FLUSH_TIMEOUT;
    let with_solo = loop {
        assert!(
            wolf_ffi_state_flush(),
            "wolf_ffi_state_flush() must complete after the solo-leaf commit"
        );
        let restored = wolf_ffi_load_state();
        let windows = restored.windows();
        if windows.len() == TWO_WINDOWS
            && windows[1]
                .tabs()
                .last()
                .is_some_and(|t| t.has_layout() && t.layout().is_leaf())
        {
            break restored;
        }
        if Instant::now() >= solo_deadline {
            break restored;
        }
        sleep(POLL_INTERVAL);
    };

    let solo_windows = with_solo.windows();
    let solo_tab = &solo_windows[1].tabs()[W1_TABS - 1];
    assert!(
        solo_tab.has_layout(),
        "the last tab must report has_layout() == true after the solo-leaf commit"
    );
    let solo_root = solo_tab.layout();
    assert!(
        solo_root.is_leaf(),
        "the root pane node of a single-pane tab is a LEAF, not a split"
    );
    assert!(
        !solo_root.is_preview(),
        "the lone editor leaf must report is_preview() == false"
    );
    assert_eq!(
        solo_root.leaf_path(),
        SOLO_PATH,
        "the lone leaf's path round-trips as the single-pane tab's open file"
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
