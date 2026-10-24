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
//!   * leaf:  `{ "leaf": true, "path": "<abs path or empty>", "preview": <bool> }`
//!   * split: `{ "orientation": "h"|"v", "weights": [...], "children": [...] }`
//!
//! ## Preview leaves (feature plan ROUND 2)
//!
//! A leaf with `"preview": true` carries its SOURCE markdown path; on rebuild the
//! Lua helper turns that leaf's window into a PREVIEW of the source (via
//! `wolf_preview_file`) instead of `:edit`-ing it, so the preview-ness survives
//! restore. The reconstructed preview surfaces in the core's `previews()` snapshot
//! (keyed by the preview WINDOW handle) with the source's absolute path — the same
//! observability path `crates/wolf-ffi/tests/preview.rs` relies on, so the preview
//! binding is asserted directly (not reduced).

mod common;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use common::{FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Snapshot, WolfCore};

/// Placeholder content for the temp markdown files these tests open.
const DOC: &str = "# Rebuild Layout\nbody\n";

/// The name of the anonymous startup buffer (this branch names every new/anonymous
/// buffer `Untitled.md` instead of leaving a bare `[No Name]`), which an EMPTY-path
/// rebuilt leaf inherits from the window's current buffer.
const UNTITLED_STARTUP: &str = "Untitled.md";

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
fn poll_win_filename(core: &WolfCore, waiter: &FlushWaiter, win: i64, expected: &str) {
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

/// Like [`poll_win_filename`], but matches on the reported filename's SUFFIX
/// (basename) rather than by exact equality, so an absolute path or a
/// `/var`<->`/private/var` symlink still matches. Used for the inherited
/// `Untitled.md` startup buffer, whose reported name may be an absolute path.
/// Mirrors the `ends_with` basename match in `wolf-ffi/tests/untitled.rs` and
/// `wolf-core/tests/current_file.rs`.
fn poll_win_filename_suffix(core: &WolfCore, waiter: &FlushWaiter, win: i64, suffix: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if let Some(grid) = core.read_snapshot().grid_for_win(win) {
            if core.pane_status(grid).filename.ends_with(suffix) {
                return;
            }
        }
        if Instant::now() >= deadline {
            let observed = core
                .read_snapshot()
                .grid_for_win(win)
                .map(|g| core.pane_status(g).filename);
            panic!(
                "timed out waiting for window {win} to report a filename ending with {suffix:?}; \
                 last observed {observed:?}"
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Polls the core's snapshot until the preview WINDOW `win` reports a preview whose
/// SOURCE path basename equals `expected_name`, or panics after [`FLUSH_TIMEOUT`].
///
/// Matches on the source path's BASENAME (not the full absolute path): the temp
/// `.md` lives under `/var/folders/...` which nvim canonicalizes to
/// `/private/var/...`, so an exact absolute compare would spuriously fail (the same
/// `/var`<->`/private/var` reason `preview.rs` uses `ends_with`/basename).
fn poll_preview_source(core: &WolfCore, waiter: &FlushWaiter, win: i64, expected_name: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if let Some(info) = core.read_snapshot().previews.get(&win) {
            if info.source_path.file_name().and_then(|s| s.to_str()) == Some(expected_name) {
                return;
            }
        }
        if Instant::now() >= deadline {
            let observed: Vec<(i64, String)> = core
                .read_snapshot()
                .previews
                .iter()
                .map(|(w, i)| (*w, i.source_path.display().to_string()))
                .collect();
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for window {win} to report a preview \
                 of {expected_name:?}; observed previews {observed:?}"
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
/// path: three distinct handles in DFS order [A, inherited, B]. The empty-path leaf
/// carries no file to `:edit`, so its window keeps the buffer it INHERITED from the
/// window it was split off — which on this branch is the named `Untitled.md` startup
/// buffer (our feature names every new/anonymous buffer `Untitled.md`, so there is no
/// longer a bare `[No Name]` startup buffer to inherit).
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

    // DFS order [A, inherited, B]: the empty-path leaf keeps the `Untitled.md` startup buffer,
    // matched by suffix to survive relative paths and `/var` vs `/private/var`.
    poll_win_filename(&core, &waiter, handles[0], name_a);
    poll_win_filename_suffix(&core, &waiter, handles[1], UNTITLED_STARTUP);
    poll_win_filename(&core, &waiter, handles[2], name_b);

    core.shutdown().expect("shutdown should be Ok");
}

/// (c) A vertical split of [ EDITOR of a .md, PREVIEW of the SAME .md ] (feature
/// plan ROUND 2): two distinct handles; the editor pane shows the md file, and the
/// preview leaf is RECONSTRUCTED as a real preview of the md SOURCE (reported by
/// the core's `previews()` at the preview window handle) — NOT a blank/edit buffer.
#[test]
fn rebuild_editor_and_preview_of_same_markdown() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let file = common::temp_md(DOC);
    let md = file.path().to_str().expect("md path is UTF-8");
    let name = file.path().file_name().unwrap().to_str().unwrap();

    // Leaf 0 = editor of the md (preview: false); leaf 1 = PREVIEW of the SAME md
    // (preview: true, path = the md's abs path). DFS leaf order is [editor, preview].
    let spec = format!(
        r#"{{"orientation":"v","weights":[0.5,0.5],"children":[{{"leaf":true,"path":"{md}","preview":false}},{{"leaf":true,"path":"{md}","preview":true}}]}}"#
    );

    let handles = core.rebuild_layout(&spec);
    assert_eq!(
        handles.len(),
        2,
        "a 2-leaf editor+preview split must return 2 window handles; got {handles:?}"
    );
    assert_distinct_handles(&handles);

    poll_until(
        &core,
        &waiter,
        "two windows after the editor+preview rebuild",
        |snap| snap.windows.len() == 2,
    );

    // Handle 0 (the editor leaf) shows the md file.
    poll_win_filename(&core, &waiter, handles[0], name);

    // The preview leaf is a real preview of the md source, keyed by the preview window handle: DFS
    // order holds because preview_file binds the buffer into the existing window.
    poll_preview_source(&core, &waiter, handles[1], name);

    core.shutdown().expect("shutdown should be Ok");
}

/// (d) A LONE editor leaf (single-pane tab): a `{leaf,path,preview:false}` spec with
/// no split returns exactly ONE handle, and that one window shows the requested file.
/// This locks in single-pane persistence — the front-end now emits a single leaf for
/// an unsplit pane and rebuilds it just by opening the file.
#[test]
fn rebuild_single_leaf_editor() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let file = common::temp_md(DOC);
    let path = file.path().to_str().expect("file path is UTF-8");
    let name = file.path().file_name().unwrap().to_str().unwrap();

    let spec = format!(r#"{{"leaf":true,"path":"{path}","preview":false}}"#);

    let handles = core.rebuild_layout(&spec);
    assert_eq!(
        handles.len(),
        1,
        "a lone editor leaf must return exactly 1 window handle; got {handles:?}"
    );
    assert_distinct_handles(&handles);

    // Still exactly one editor window; the lone leaf reuses the starting window.
    poll_until(
        &core,
        &waiter,
        "one editor window after the single-leaf rebuild",
        |snap| snap.windows.len() == 1,
    );

    // That one window shows the requested file.
    poll_win_filename(&core, &waiter, handles[0], name);

    core.shutdown().expect("shutdown should be Ok");
}

/// (e) A LONE PREVIEW leaf (single-pane preview tab): a `{leaf,path,preview:true}`
/// spec with no split returns ONE handle whose window is RECONSTRUCTED as a real
/// preview of the source markdown (surfaced in `previews()` at the window handle) —
/// NOT a plain `:edit` of the file. This preserves preview-ness for a single pane.
#[test]
fn rebuild_single_leaf_preview() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let file = common::temp_md(DOC);
    let md = file.path().to_str().expect("md path is UTF-8");
    let name = file.path().file_name().unwrap().to_str().unwrap();

    let spec = format!(r#"{{"leaf":true,"path":"{md}","preview":true}}"#);

    let handles = core.rebuild_layout(&spec);
    assert_eq!(
        handles.len(),
        1,
        "a lone preview leaf must return exactly 1 window handle; got {handles:?}"
    );
    assert_distinct_handles(&handles);

    poll_until(
        &core,
        &waiter,
        "one window after the single-leaf preview rebuild",
        |snap| snap.windows.len() == 1,
    );

    // The lone window is a reconstructed preview of the md SOURCE (via previews(),
    // keyed by the window handle) — not a plain `:edit`.
    poll_preview_source(&core, &waiter, handles[0], name);

    core.shutdown().expect("shutdown should be Ok");
}

/// (f) A LONE EMPTY leaf (`{leaf,path:"",preview:false}`): one handle, and its window
/// carries no file to `:edit`, so it keeps the buffer it INHERITED from the starting
/// window — the named `Untitled.md` startup buffer (matched by basename/suffix, robust
/// to absolute-vs-relative paths and `/var`<->`/private/var`). Mirrors the empty-leaf
/// assertion in `rebuild_perpendicular_tree_with_empty_leaf`.
#[test]
fn rebuild_single_empty_leaf() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let spec = r#"{"leaf":true,"path":"","preview":false}"#;

    let handles = core.rebuild_layout(spec);
    assert_eq!(
        handles.len(),
        1,
        "a lone empty leaf must return exactly 1 window handle; got {handles:?}"
    );
    assert_distinct_handles(&handles);

    poll_until(
        &core,
        &waiter,
        "one window after the single empty-leaf rebuild",
        |snap| snap.windows.len() == 1,
    );

    // The empty-path leaf has no file to edit, so its window keeps the inherited
    // `Untitled.md` startup buffer (basename/suffix match).
    poll_win_filename_suffix(&core, &waiter, handles[0], UNTITLED_STARTUP);

    core.shutdown().expect("shutdown should be Ok");
}
