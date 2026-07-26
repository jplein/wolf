//! Real-nvim integration test for the pane-layout REBUILD API (feature plan
//! §4.4): `WolfCore::rebuild_layout(spec_json)` realizes a saved pane-split tree
//! in the current tabpage's single starting window (via `:split`/`:vsplit` +
//! `:edit`) and returns the resulting window handles in DFS order — the SAME order
//! as `PaneTree.leaves()` / `PaneTree.rebuilt`, so the front-end can zip each
//! handle to its rebuilt leaf. nvim produces only the WINDOW COUNT + TOPOLOGY +
//! per-window BUFFER; sizes come later from the front-end's weights.
//!
//! These drive a REAL, hermetic nvim (no mocks) via `WOLF_NVIM_BIN`; the common
//! helpers PANIC loudly if it is unset. Following `pane_buffers.rs` /
//! `layout_intents.rs`: window creation + the `wolf_buffers` notification arrive
//! across several flushes, so every assertion POLLS with a generous timeout,
//! draining wakes as it goes. We never assert literal handle VALUES — only their
//! count, distinctness, and the per-window buffer each maps to (via
//! `grid_for_win` -> `pane_status`).
//!
//! ## Dependency on coder work (feature plan §4.4)
//!
//! `WolfCore::rebuild_layout(&self, spec_json: &str) -> Vec<i64>` and the
//! `runtime/nvim/init.lua` `wolf_rebuild_layout` helper do not exist yet. The
//! coder adds them, plus the `//crates/wolf-core:rebuild_layout` `rust_test`
//! target. This file does NOT touch non-test source.
//!
//! The spec JSON shape matches `PaneLayoutCodec.specJSON`:
//!   * leaf:  `{ "leaf": true, "path": "<abs path or empty>" }`
//!   * split: `{ "orientation": "h"|"v", "weights": [...], "children": [...] }`

mod common;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use common::{FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Snapshot, WolfCore, NO_NAME};

/// Placeholder content for the temp markdown files these tests open.
const DOC: &str = "# Rebuild Layout\nbody\n";

/// Gap between polls while waiting for a window/buffer to settle.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Polls `core`'s snapshot until `pred` holds, draining wakes each iteration, or
/// panics after [`FLUSH_TIMEOUT`] (mirrors `pane_buffers.rs`).
fn poll_until(core: &WolfCore, waiter: &FlushWaiter, desc: &str, pred: impl Fn(&Snapshot) -> bool) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if pred(&core.read_snapshot()) {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Resolves a window handle to the base `filename` its pane reports, polling until
/// the handle is bound to a grid and `pane_status` reports the expected name.
fn poll_win_filename(
    core: &WolfCore,
    waiter: &FlushWaiter,
    win: i64,
    expected: &str,
) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if let Some(grid) = core.read_snapshot().grid_for_win(win) {
            if core.pane_status(grid).filename == expected {
                return;
            }
        }
        if Instant::now() >= deadline {
            let observed = core
                .read_snapshot()
                .grid_for_win(win)
                .map(|g| core.pane_status(g).filename);
            panic!(
                "timed out waiting for window {win} to report filename {expected:?}; \
                 last observed {observed:?}"
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Asserts every handle is a real (nonzero), distinct i64 window handle.
fn assert_distinct_handles(handles: &[i64]) {
    assert!(
        handles.iter().all(|&h| h != 0),
        "every rebuilt window handle must be a real (nonzero) i64; got {handles:?}"
    );
    let unique: HashSet<i64> = handles.iter().copied().collect();
    assert_eq!(
        unique.len(),
        handles.len(),
        "rebuilt window handles must be DISTINCT; got {handles:?}"
    );
}

/// (a) A vertical split of two real files: two distinct handles, each showing the
/// requested leaf's buffer.
#[test]
fn rebuild_vertical_split_of_two_files() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let file_a = common::temp_md(DOC);
    let file_b = common::temp_md(DOC);
    let a = file_a.path().to_str().expect("file A path is UTF-8");
    let b = file_b.path().to_str().expect("file B path is UTF-8");
    let name_a = file_a.path().file_name().unwrap().to_str().unwrap();
    let name_b = file_b.path().file_name().unwrap().to_str().unwrap();

    let spec = format!(
        r#"{{"orientation":"v","weights":[0.5,0.5],"children":[{{"leaf":true,"path":"{a}"}},{{"leaf":true,"path":"{b}"}}]}}"#
    );

    let handles = core.rebuild_layout(&spec);
    assert_eq!(
        handles.len(),
        2,
        "a 2-leaf vertical split must return 2 window handles; got {handles:?}"
    );
    assert_distinct_handles(&handles);

    // Two editor windows now exist.
    poll_until(
        &core,
        &waiter,
        "two editor windows after the rebuild",
        |snap| snap.windows.len() == 2,
    );

    // Handles are in DFS/leaf order: handle 0 -> file A, handle 1 -> file B.
    poll_win_filename(&core, &waiter, handles[0], name_a);
    poll_win_filename(&core, &waiter, handles[1], name_b);

    core.shutdown().expect("shutdown should be Ok");
}

/// (b) A perpendicular nested tree with THREE leaves, one of which has an EMPTY
/// path: three distinct handles in DFS order [A, unnamed, B]; the empty-path leaf
/// lands an UNNAMED buffer window ([No Name]).
#[test]
fn rebuild_perpendicular_tree_with_empty_leaf() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let file_a = common::temp_md(DOC);
    let file_b = common::temp_md(DOC);
    let a = file_a.path().to_str().expect("file A path is UTF-8");
    let b = file_b.path().to_str().expect("file B path is UTF-8");
    let name_a = file_a.path().file_name().unwrap().to_str().unwrap();
    let name_b = file_b.path().file_name().unwrap().to_str().unwrap();

    // Vertical split: [ leaf A, horizontal split[ leaf "", leaf B ] ].
    // DFS leaf order is [A, "", B].
    let spec = format!(
        r#"{{"orientation":"v","weights":[0.5,0.5],"children":[{{"leaf":true,"path":"{a}"}},{{"orientation":"h","weights":[0.5,0.5],"children":[{{"leaf":true,"path":""}},{{"leaf":true,"path":"{b}"}}]}}]}}"#
    );

    let handles = core.rebuild_layout(&spec);
    assert_eq!(
        handles.len(),
        3,
        "a 3-leaf perpendicular tree must return 3 window handles; got {handles:?}"
    );
    assert_distinct_handles(&handles);

    poll_until(
        &core,
        &waiter,
        "three editor windows after the rebuild",
        |snap| snap.windows.len() == 3,
    );

    // DFS order: [A, unnamed, B]. The middle (empty-path) leaf is an unnamed buffer.
    poll_win_filename(&core, &waiter, handles[0], name_a);
    poll_win_filename(&core, &waiter, handles[1], NO_NAME);
    poll_win_filename(&core, &waiter, handles[2], name_b);

    core.shutdown().expect("shutdown should be Ok");
}
