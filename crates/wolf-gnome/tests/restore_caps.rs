//! Table-driven tests for the restore caps of `wolf_gnome::state_seam::restore_plans`: how many
//! tabs, and so Neovim processes, one launch restores across every saved window.
//!
//! Pure: no display, no Neovim and no filesystem, since `is_dir` answers `true` for every path.
//! Every saved window sits on a directory of its own and every saved tab carries a root, so
//! nothing but the caps can shorten a plan. Every expected count is written out by hand.

use std::path::{Path, PathBuf};

use wolf_core::app_state::{Frame, Node, Pane, Sidebar, Tab, TabContent, Window, WindowState};
use wolf_gnome::state_seam::{restore_plans, RestorePlan, MAX_RESTORE_NEOVIMS, MAX_RESTORE_TABS};

/// What every expected count below was worked out against.
const NEOVIMS_PREMISE: usize = 16;

/// The per-window cap the expected counts assume.
const TABS_PREMISE: usize = 8;

/// The saved directory of window `window`. Distinct per window, and neither `/` nor a home.
fn window_dir(window: usize) -> String {
    format!("/projects/w{window}")
}

/// The file tab `tab` of window `window` shows, unique across the whole tree.
fn tab_file(window: usize, tab: usize) -> String {
    format!("/projects/w{window}/t{tab}.md")
}

/// The lone pane tab `tab` of window `window` saved as its root.
fn tab_root(window: usize, tab: usize) -> Option<Node> {
    Some(Node::Pane(Pane::Edit {
        path: tab_file(window, tab),
        cursor: None,
        top_line: None,
    }))
}

/// A saved window state of one window per entry of `tab_counts`, in that order, each with that
/// many tabs on its own directory.
fn saved_tree(tab_counts: &[usize]) -> WindowState {
    WindowState {
        windows: tab_counts
            .iter()
            .enumerate()
            .map(|(window, &count)| Window {
                frame: Frame {
                    x: None,
                    y: None,
                    width: 1280.0,
                    height: 720.0,
                },
                active_tab: 0,
                tabs: (0..count)
                    .map(|tab| Tab {
                        directory: window_dir(window),
                        content: TabContent {
                            root: tab_root(window, tab),
                            sidebar: Sidebar::default(),
                            ..TabContent::default()
                        },
                    })
                    .collect(),
                minimized: false,
            })
            .collect(),
    }
}

/// The plans a desktop-launcher start makes of `tab_counts`: no process cwd and no home.
fn plan(tab_counts: &[usize]) -> Vec<RestorePlan> {
    restore_plans(Some(&saved_tree(tab_counts)), None, None, |_: &Path| true)
}

/// Each plan's tab count, in plan order.
fn tab_counts(plans: &[RestorePlan]) -> Vec<usize> {
    plans.iter().map(|plan| plan.tabs.len()).collect()
}

/// Pins the caps every expected count was written against, so a changed cap fails here, by
/// name, rather than as a count that no longer adds up.
fn assert_premises() {
    assert_eq!(
        MAX_RESTORE_NEOVIMS, NEOVIMS_PREMISE,
        "MAX_RESTORE_NEOVIMS is no longer {NEOVIMS_PREMISE}, the value every expected count in \
         this file was worked out against; rework the counts by hand rather than deriving them"
    );
    assert_eq!(
        MAX_RESTORE_TABS, TABS_PREMISE,
        "MAX_RESTORE_TABS is no longer {TABS_PREMISE}, the value every expected count in this \
         file was worked out against; rework the counts by hand rather than deriving them"
    );
}

/// Asserts that `plans` open the first windows of the saved tree, in saved order.
fn assert_saved_order(plans: &[RestorePlan], case: &str) {
    for (window, plan) in plans.iter().enumerate() {
        assert_eq!(
            plan.tabs[0].cwd,
            PathBuf::from(window_dir(window)),
            "{case}: plan {window} opens {:?}, not saved window {window}'s directory. The walk \
             must plan windows in saved order and drop from the end; another directory means it \
             skipped or reordered windows",
            plan.tabs[0].cwd
        );
    }
}

/// RC1.
#[test]
fn the_session_restores_at_most_max_restore_neovims_tabs() {
    assert_premises();

    let plans = plan(&[8, 8, 8]);
    assert_eq!(
        tab_counts(&plans),
        vec![8, 8],
        "three saved windows of 8 tabs each must plan two windows of 8, the 16 Neovims the \
         session budget allows. [8, 8, 8] means the cap counts windows rather than tabs, so a \
         corrupt saved window state can still start 8 processes per window for 16 windows"
    );
    assert_saved_order(&plans, "three windows of 8 tabs");

    let plans = plan(&[1; 17]);
    assert_eq!(
        tab_counts(&plans),
        vec![1; 16],
        "seventeen saved one-tab windows must plan sixteen. Seventeen means nothing stops the \
         walk once the budget is spent"
    );
    assert_saved_order(&plans, "seventeen one-tab windows");
}

/// RC2.
#[test]
fn a_window_that_partly_fits_is_truncated() {
    assert_premises();

    let plans = plan(&[8, 5, 5]);
    assert_eq!(
        tab_counts(&plans),
        vec![8, 5, 3],
        "windows of 8, 5 and 5 tabs must plan 8, 5 and 3: the third window gets the 3 Neovims \
         the budget has left. [8, 5] means a window that does not fit whole is dropped instead \
         of truncated, and the user loses a whole window where three of its tabs would fit; \
         [8, 5, 5] means the session cap is not applied"
    );
    assert_saved_order(&plans, "windows of 8, 5 and 5 tabs");

    let kept: Vec<Option<Node>> = plans[2]
        .tabs
        .iter()
        .map(|tab| {
            tab.saved
                .as_ref()
                .and_then(|saved| saved.content.root.clone())
        })
        .collect();
    assert_eq!(
        kept,
        vec![tab_root(2, 0), tab_root(2, 1), tab_root(2, 2)],
        "the truncated third window must keep its FIRST three tabs' roots, in saved order. \
         Later tabs mean the truncation drops from the front; None means the saved tabs were \
         lost while the tab count was kept"
    );
}

/// RC3.
#[test]
fn a_session_exactly_at_the_budget_restores_whole() {
    assert_premises();

    let plans = plan(&[8, 8]);
    assert_eq!(
        tab_counts(&plans),
        vec![8, 8],
        "two saved windows of 8 tabs are exactly the 16-Neovim budget and must restore whole. \
         [8, 7] means the budget is one short, so the last tab of a session at the cap is lost"
    );
    assert_saved_order(&plans, "two windows of 8 tabs");

    let plans = plan(&[1; 16]);
    assert_eq!(
        tab_counts(&plans),
        vec![1; 16],
        "sixteen saved one-tab windows are exactly the budget and must all restore. Fifteen \
         means the budget is one short"
    );
    assert_saved_order(&plans, "sixteen one-tab windows");
}
