//! Real-Neovim tests of the restore side of `wolf_core::app_state`: a saved tab goes through
//! `restored_tab` and `tab_rebuild_spec` into `WolfCore::rebuild_layout`, and the captive
//! Neovim's `wolf_rebuild_layout` realizes it.
//!
//! They pin `specs/application-state.md` §5 ("Restoring at launch") end to end:
//!
//! - An edit pane whose file is missing restores as an empty pane in the same position, and a
//!   zoom on it is dropped, so the rebuild does not end on it.
//! - A saved cursor and top line restore, and a cursor beyond the end of its file is clamped.
//!
//! They also pin `WolfCore::window_cursor`, the read §4's cursor capture makes: it answers each
//! window's own cursor and top line, not the current window's, and `None` for no window.
//!
//! Premises measured against the pinned Neovim rather than assumed:
//!
//! - An unzoomed rebuild ends on the last leaf it realizes, so `active_win` on return tells a
//!   kept zoom on leaf 0 from a dropped one. `rebuild_layout.rs` measures this as
//!   `rebuild_ends_on_the_zoomed_leaf`'s control.
//! - A far cursor with no saved top line scrolls to a different top line than the one
//!   [`a_saved_cursor_and_top_line_restore`] saves, so the top line it reads back is the saved
//!   one and not Neovim's own scroll. That test measures this first, on a fresh core.
//!
//! Viewport lines are read from `win_viewport`, whose `topline` and `curline` are 0-based.

mod common;

use std::path::Path;

use common::{poll_until, FlushWaiter};
use wolf_core::app_state::{
    restored_tab, tab_rebuild_spec, Cursor, Layout, Node, Pane, PaneOrientation, PaneRef,
    TabContent,
};
use wolf_core::{WindowCursor, WolfCore};

/// The name of the anonymous startup buffer an empty-path leaf inherits.
const UNTITLED_STARTUP: &str = "Untitled.md";

/// The saved cursor line of [`a_saved_cursor_and_top_line_restore`], 1-based.
const FAR_LINE: u32 = 120;

/// The saved top line of [`a_saved_cursor_and_top_line_restore`], 1-based: [`FAR_LINE`] sits on
/// the view's second row, where Neovim's own scroll to a far line would centre it.
const SAVED_TOP_LINE: u32 = FAR_LINE - 1;

/// A file of `lines` numbered lines in `dir`, named `name`, and its path.
fn numbered_file(dir: &Path, name: &str, lines: usize) -> String {
    let path = dir.join(name);
    let text: String = (1..=lines).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&path, text).expect("failed to write a numbered fixture");
    path.to_str().expect("the fixture path is UTF-8").to_owned()
}

/// An edit pane on `path` with an optional saved cursor and top line.
fn edit(path: &str, cursor: Option<Cursor>, top_line: Option<u32>) -> Node {
    Node::Pane(Pane::Edit {
        path: path.to_owned(),
        cursor,
        top_line,
    })
}

/// Tab content whose root is `root`, with nothing else saved.
fn tab_of(root: Node) -> TabContent {
    TabContent {
        root: Some(root),
        ..TabContent::default()
    }
}

/// Restores `saved` and rebuilds it on `core`, returning the handles and `active_win` read on
/// return.
fn restore(core: &WolfCore, saved: &TabContent) -> (TabContent, Vec<i64>, Option<i64>) {
    let restored = restored_tab(saved);
    let spec = tab_rebuild_spec(&restored).expect("a tab with a root has a rebuild document");
    let handles = core.rebuild_layout(&spec);
    let active = core.read_snapshot().active_win;
    (restored, handles, active)
}

/// The `(topline, curline)` of window `win`'s viewport, 0-based, if the snapshot shows it.
fn viewport_of(core: &WolfCore, win: i64) -> Option<(u64, u64)> {
    core.read_snapshot()
        .windows
        .values()
        .find(|info| info.win == win && !info.hidden)
        .map(|info| (info.viewport.topline, info.viewport.curline))
}

/// Polls until window `win`'s pane reports a filename ending with `suffix`.
fn poll_win_filename(core: &WolfCore, waiter: &FlushWaiter, win: i64, suffix: &str, why: &str) {
    poll_until(
        waiter,
        &format!("window {win} to show a file ending with {suffix:?}: {why}"),
        || {
            common::grid_of_win(core, win)
                .map(|grid| core.pane_status(grid).filename)
                .unwrap_or_default()
        },
        |name: &String| name.ends_with(suffix),
    );
}

/// Polls until window `win`'s viewport satisfies `done`, and returns it.
fn poll_viewport(
    core: &WolfCore,
    waiter: &FlushWaiter,
    win: i64,
    desc: &str,
    done: impl Fn(&(u64, u64)) -> bool,
) -> (u64, u64) {
    poll_until(
        waiter,
        desc,
        || viewport_of(core, win).unwrap_or((u64::MAX, u64::MAX)),
        done,
    )
}

#[test]
fn a_tab_with_a_missing_file_rebuilds_with_an_empty_pane_and_no_zoom() {
    let dir = tempfile::tempdir().expect("failed to create the fixture tempdir");
    let kept = numbered_file(dir.path(), "kept.md", 3);
    let gone = dir.path().join("gone.md");
    assert!(!gone.exists(), "premise: gone.md must not exist");
    let mut saved = tab_of(Node::Layout(Layout {
        orientation: PaneOrientation::Vertical,
        weights: vec![0.5, 0.5],
        children: vec![
            edit(
                gone.to_str().expect("the fixture path is UTF-8"),
                None,
                None,
            ),
            edit(&kept, None, None),
        ],
    }));
    saved.zoomed = Some(PaneRef::Pane(0));

    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let (restored, handles, active) = restore(&core, &saved);
    assert_eq!(
        restored.zoomed, None,
        "pane 0's file is missing, so restored_tab must drop its zoom"
    );
    assert_eq!(
        handles.len(),
        2,
        "a two-pane tab must rebuild into two windows; got {handles:?}"
    );
    assert_eq!(
        active,
        Some(handles[1]),
        "with the zoom dropped the rebuild must end on its last leaf; handles {handles:?}. The \
         first handle means the rendered document still zooms the missing pane"
    );

    poll_win_filename(
        &core,
        &waiter,
        handles[0],
        UNTITLED_STARTUP,
        "the missing file restores as an empty pane, which keeps the startup buffer; gone.md \
         means the helper was asked to :edit the missing file",
    );
    poll_win_filename(
        &core,
        &waiter,
        handles[1],
        "kept.md",
        "the existing file restores in its own position",
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn a_cursor_beyond_the_end_of_its_file_is_clamped() {
    let dir = tempfile::tempdir().expect("failed to create the fixture tempdir");
    let short = numbered_file(dir.path(), "short.md", 3);
    let saved = tab_of(edit(&short, Some(Cursor { line: 100, col: 0 }), None));

    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let (_, handles, _) = restore(&core, &saved);
    assert_eq!(handles.len(), 1, "a lone pane rebuilds into one window");
    poll_win_filename(
        &core,
        &waiter,
        handles[0],
        "short.md",
        "the lone pane's file",
    );

    poll_viewport(
        &core,
        &waiter,
        handles[0],
        "the cursor saved on line 100 of a 3-line file to land on its last line (curline 2); \
         curline 0 means the cursor was not applied, or Neovim rejected the unclamped line",
        |&(_, curline)| curline == 2,
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Rebuilds a lone edit pane on `path` with the cursor on [`FAR_LINE`] and `top_line` on a fresh
/// core, and returns the viewport once the cursor is there.
fn rebuild_far_cursor(path: &str, top_line: Option<u32>, desc: &str) -> (u64, u64) {
    let saved = tab_of(edit(
        path,
        Some(Cursor {
            line: FAR_LINE,
            col: 3,
        }),
        top_line,
    ));
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let (_, handles, _) = restore(&core, &saved);
    assert_eq!(handles.len(), 1, "a lone pane rebuilds into one window");
    let want_top = top_line.map(|t| u64::from(t) - 1);
    let viewport = poll_viewport(&core, &waiter, handles[0], desc, |&(topline, curline)| {
        curline == u64::from(FAR_LINE) - 1 && want_top.is_none_or(|t| topline == t)
    });
    core.shutdown().expect("shutdown should be Ok");
    viewport
}

#[test]
fn a_saved_cursor_and_top_line_restore() {
    let dir = tempfile::tempdir().expect("failed to create the fixture tempdir");
    let long = numbered_file(dir.path(), "long.md", 200);

    let (own_top, _) = rebuild_far_cursor(
        &long,
        None,
        "premise: the cursor saved on line 120, with no top line, to land on line 120 (curline \
         119); curline 0 means the cursor is not applied",
    );
    assert_ne!(
        own_top,
        u64::from(SAVED_TOP_LINE) - 1,
        "premise: with no saved top line Neovim's own scroll must leave another top line than \
         the one the next rebuild saves, or that rebuild could not show the top line applied"
    );

    rebuild_far_cursor(
        &long,
        Some(SAVED_TOP_LINE),
        &format!(
            "the cursor saved on line {FAR_LINE} with top line {SAVED_TOP_LINE} to restore both \
             (curline {}, topline {}); topline {own_top} is Neovim's own scroll, so the top line \
             was not applied, and curline 0 means the cursor was not",
            FAR_LINE - 1,
            SAVED_TOP_LINE - 1
        ),
    );
}

#[test]
fn window_cursor_reads_each_windows_own_cursor_and_top_line() {
    let dir = tempfile::tempdir().expect("failed to create the fixture tempdir");
    let long = numbered_file(dir.path(), "long.md", 200);
    let short = numbered_file(dir.path(), "short.md", 3);
    let saved = tab_of(Node::Layout(Layout {
        orientation: PaneOrientation::Vertical,
        weights: vec![0.5, 0.5],
        children: vec![
            edit(
                &long,
                Some(Cursor {
                    line: FAR_LINE,
                    col: 3,
                }),
                Some(SAVED_TOP_LINE),
            ),
            edit(&short, None, None),
        ],
    }));

    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let (_, handles, active) = restore(&core, &saved);
    assert_eq!(
        handles.len(),
        2,
        "a two-pane tab must rebuild into two windows; got {handles:?}"
    );
    assert_eq!(
        active,
        Some(handles[1]),
        "premise: the rebuild must end on leaf b, so that reading leaf a is reading a window \
         other than the current one; handles {handles:?}"
    );
    poll_viewport(
        &core,
        &waiter,
        handles[0],
        &format!(
            "premise: leaf a's saved cursor (line {FAR_LINE}) and top line ({SAVED_TOP_LINE}) \
             to apply (curline {}, topline {})",
            FAR_LINE - 1,
            SAVED_TOP_LINE - 1
        ),
        |&(topline, curline)| {
            topline == u64::from(SAVED_TOP_LINE) - 1 && curline == u64::from(FAR_LINE) - 1
        },
    );
    poll_win_filename(&core, &waiter, handles[1], "short.md", "leaf b's file");

    assert_eq!(
        core.window_cursor(handles[0]),
        Some(WindowCursor {
            cursor: Cursor {
                line: FAR_LINE,
                col: 3,
            },
            top_line: SAVED_TOP_LINE,
        }),
        "leaf a, which is not the current window, must read its own cursor and top line. A top \
         line of 1 means the snippet reads line(\"w0\") of the current window, leaf b; col 4 \
         means the 0-based column is reported 1-based; None means a valid window reads as none"
    );
    assert_eq!(
        core.window_cursor(handles[1]),
        Some(WindowCursor {
            cursor: Cursor { line: 1, col: 0 },
            top_line: 1,
        }),
        "leaf b, current and on a 3-line file with no saved cursor, must read line 1, col 0, top \
         line 1; line {FAR_LINE} means every read answers leaf a"
    );
    assert_eq!(
        core.window_cursor(999_999),
        None,
        "a window handle Neovim does not know must read as None; a zeroed Some means a nil \
         reply is mapped to a cursor"
    );
    assert_eq!(
        core.window_cursor(0),
        None,
        "window 0 must read as None: Neovim takes 0 for the current window, so Some means the \
         read went to Neovim and answered leaf b"
    );

    core.shutdown().expect("shutdown should be Ok");
}
