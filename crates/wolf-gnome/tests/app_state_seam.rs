//! Table-driven tests for the application-state seam of `wolf_gnome::state_seam`: the launch
//! plans `restore_plans` makes of a saved `wolf_core::app_state::WindowState`
//! (`specs/application-state.md` §5), and the entries the save side builds of the live windows
//! and tabs (§2 and §4), and the schedule `present_after` gives a restore's presents.
//!
//! Pure: no display, no Neovim and no filesystem. `is_dir` answers from a fixed list, so a
//! directory is "missing" by not being on it, and every expected value is written out by hand.
//! Tab ids are chosen so no id equals an index of the lists asserted on. Whether the coordinator
//! feeds these functions the live windows, and restores what they plan, is the two-window
//! restore smoke's question, in `smoke_headless/restore.rs`.

use std::path::{Path, PathBuf};

use wolf_core::app_state::{
    Frame, Layout, Node, Pane, PaneOrientation, PaneRef, Sidebar, Tab, TabContent, Window,
    WindowState,
};
use wolf_gnome::coordinator::WindowId;
use wolf_gnome::stacking::{StackingOrder, TabRecency};
use wolf_gnome::state_seam::{
    present_after, restore_plans, seeded_tab, tab_entry, window_entry, window_state, RestorePlan,
};
use wolf_gnome::tabs_ui::TabId;

/// The home directory every plan is made with. A directory, but never a project.
const HOME: &str = "/home/wolf-user";

/// Two saved project directories, both on [`is_dir`]'s list.
const DIR_A: &str = "/p/a";
const DIR_B: &str = "/p/b";

/// A saved directory that is not on [`is_dir`]'s list, so it reads as missing.
const GONE: &str = "/gone";

/// A process cwd on [`is_dir`]'s list that no saved tab holds.
const UNHELD: &str = "/q";

/// Three tab ids. None is 0, 1 or 2, so an index mistaken for an id shows.
const T10: TabId = TabId(10);
const T11: TabId = TabId(11);
const T12: TabId = TabId(12);

/// The directories that exist for these plans.
fn is_dir(path: &Path) -> bool {
    [HOME, DIR_A, DIR_B, UNHELD]
        .iter()
        .any(|dir| path == Path::new(dir))
}

/// A lone edit pane on `path`.
fn edit(path: &str) -> Node {
    Node::Pane(Pane::Edit {
        path: path.to_owned(),
        cursor: None,
        top_line: None,
    })
}

/// A vertical layout of edit panes on `paths`, evenly weighted.
fn layout_of(paths: &[&str]) -> Node {
    Node::Layout(Layout {
        orientation: PaneOrientation::Vertical,
        weights: vec![1.0 / paths.len() as f64; paths.len()],
        children: paths.iter().map(|path| edit(path)).collect(),
    })
}

/// A saved tab at `directory` whose lone pane shows `<directory>/<marker>.md`, with the sidebar
/// preference `sidebar`.
fn saved_tab(directory: &str, marker: &str, sidebar: bool) -> Tab {
    Tab {
        directory: directory.to_owned(),
        content: TabContent {
            root: Some(edit(&format!("{directory}/{marker}.md"))),
            sidebar: Sidebar {
                visible: sidebar,
                ..Sidebar::default()
            },
            ..TabContent::default()
        },
    }
}

/// A frame of `width` by `height` with no position, as this front-end writes one.
fn frame(width: f64, height: f64) -> Frame {
    Frame {
        x: None,
        y: None,
        width,
        height,
    }
}

/// A saved window of `tabs` at `width` by 700, with `active_tab` and `minimized`.
fn saved_window(width: f64, active_tab: usize, minimized: bool, tabs: Vec<Tab>) -> Window {
    Window {
        frame: frame(width, 700.0),
        active_tab,
        tabs,
        minimized,
    }
}

/// The plans of a desktop-launcher start, with no process cwd.
fn plans_of(windows: Vec<Window>) -> Vec<RestorePlan> {
    restore_plans(
        Some(&WindowState { windows }),
        None,
        Some(Path::new(HOME)),
        is_dir,
    )
}

/// The start directory of each plan's first tab, in plan order.
fn first_cwds(plans: &[RestorePlan]) -> Vec<PathBuf> {
    plans.iter().map(|plan| plan.tabs[0].cwd.clone()).collect()
}

/// G3a.
#[test]
fn every_tab_restores_at_its_own_directory() {
    let window = saved_window(
        1000.0,
        0,
        false,
        vec![
            saved_tab(DIR_A, "one", true),
            saved_tab(DIR_B, "two", false),
        ],
    );
    let plans = plans_of(vec![window.clone()]);
    assert_eq!(plans.len(), 1, "one saved window must plan one window");
    let tabs = &plans[0].tabs;
    let cwds: Vec<&Path> = tabs.iter().map(|tab| tab.cwd.as_path()).collect();
    assert_eq!(
        cwds,
        [Path::new(DIR_A), Path::new(DIR_B)],
        "each tab must start at its own saved directory. [/p/a, /p/a] means every tab starts at \
         the window's first tab's directory, which the per-tab Neovim model does not need; one \
         entry means a tab was dropped"
    );
    let empties: Vec<bool> = tabs.iter().map(|tab| tab.empty).collect();
    assert_eq!(
        empties,
        [false, false],
        "both directories exist, so neither tab may open in the empty state"
    );
    let saved: Vec<Option<&Tab>> = tabs.iter().map(|tab| tab.saved.as_ref()).collect();
    assert_eq!(
        saved,
        [Some(&window.tabs[0]), Some(&window.tabs[1])],
        "each tab plan must carry its saved tab, as saved, for its rebuild and its seed"
    );
    let sidebars: Vec<bool> = tabs.iter().map(|tab| tab.sidebar_visible).collect();
    assert_eq!(
        sidebars,
        [true, false],
        "each tab must restore its own saved sidebar preference. [true, true] or \
         [false, false] means the preference is read once per window"
    );
}

/// G3b.
#[test]
fn a_tab_whose_directory_is_missing_restores_as_an_empty_tab() {
    assert!(
        !is_dir(Path::new(GONE)),
        "premise: {GONE} must read as missing"
    );
    let window = saved_window(
        1000.0,
        0,
        false,
        vec![saved_tab(DIR_A, "one", true), saved_tab(GONE, "two", true)],
    );
    let plans = plans_of(vec![window.clone()]);
    let tabs = &plans[0].tabs;
    assert_eq!(
        tabs.len(),
        2,
        "both saved tabs must be planned, the missing one included"
    );
    assert!(
        tabs[1].empty,
        "the tab whose directory is missing must restore as an empty tab; it plans a folder at \
         {:?}, so the saved directory is used without the launch rule's existence check",
        tabs[1].cwd
    );
    assert_eq!(
        tabs[1].cwd,
        PathBuf::from(HOME),
        "an empty tab's Neovim must start at the home directory, as the launch rule's empty \
         state does"
    );
    assert_eq!(
        tabs[1].saved.as_ref(),
        Some(&window.tabs[1]),
        "the empty-restored tab must keep its saved tab, so it contributes that tab until its \
         own gather replaces it, and the missing directory's project state is left alone"
    );
    assert!(
        !tabs[0].empty && tabs[0].cwd == Path::new(DIR_A),
        "the tab whose directory exists must still restore as a folder at {DIR_A}; it plans \
         {:?} (empty: {}), so one missing directory emptied the whole window",
        tabs[0].cwd,
        tabs[0].empty
    );
}

/// G3c.
#[test]
fn the_saved_active_tab_is_planned() {
    let window = saved_window(
        1000.0,
        1,
        false,
        vec![saved_tab(DIR_A, "one", true), saved_tab(DIR_A, "two", true)],
    );
    let plans = plans_of(vec![window]);
    assert_eq!(
        plans[0].active_tab, 1,
        "the window was saved with tab 1 active, so its plan must select tab 1. 0 means restore \
         always selects the first tab"
    );
}

/// G3d.
#[test]
fn an_out_of_range_active_tab_plans_the_first() {
    let window = saved_window(
        1000.0,
        9,
        false,
        vec![saved_tab(DIR_A, "one", true), saved_tab(DIR_A, "two", true)],
    );
    let plans = plans_of(vec![window]);
    assert_eq!(
        plans[0].active_tab, 0,
        "a saved active_tab of 9 names no tab of a two-tab window, so the plan must select tab \
         0; 9 means the index is passed through unclamped and the window selects nothing"
    );
}

/// G3e.
#[test]
fn windows_keep_saved_order_size_and_minimized() {
    let front = saved_window(1100.0, 0, false, vec![saved_tab(DIR_A, "one", true)]);
    let back = saved_window(900.0, 0, true, vec![saved_tab(DIR_B, "two", true)]);
    let plans = plans_of(vec![front, back]);
    assert_eq!(
        first_cwds(&plans),
        [PathBuf::from(DIR_A), PathBuf::from(DIR_B)],
        "the plans must follow the saved stacking order, frontmost first"
    );
    let sizes: Vec<Option<(i32, i32)>> = plans.iter().map(|plan| plan.window_size).collect();
    assert_eq!(
        sizes,
        [Some((1100, 700)), Some((900, 700))],
        "each plan must carry its own saved frame's size"
    );
    let minimized: Vec<bool> = plans.iter().map(|plan| plan.minimized).collect();
    assert_eq!(
        minimized,
        [false, true],
        "the second window was saved minimized and must be planned minimized. [false, false] \
         means restore drops the flag; [true, false] means it lands on the wrong window"
    );
}

/// G3f.
#[test]
fn a_terminal_launch_from_an_unheld_project_adds_a_window_in_front() {
    let window = saved_window(1000.0, 0, false, vec![saved_tab(DIR_A, "one", true)]);
    let plans = restore_plans(
        Some(&WindowState {
            windows: vec![window.clone()],
        }),
        Some(Path::new(UNHELD)),
        Some(Path::new(HOME)),
        is_dir,
    );
    assert_eq!(
        first_cwds(&plans),
        [PathBuf::from(UNHELD), PathBuf::from(DIR_A)],
        "a launch from {UNHELD}, which no saved tab holds, must add one window there in front \
         of the saved one. [/p/a] alone means the extra plan is never added and the launch \
         directory is not shown; [/q] alone means the saved window was replaced"
    );
    let fresh = &plans[0];
    assert_eq!(
        (
            fresh.tabs.len(),
            fresh.tabs[0].empty,
            fresh.tabs[0].saved.as_ref()
        ),
        (1, false, None),
        "the added window must hold one fresh folder tab with no saved tab to seed from \
         (tabs, empty, saved)"
    );
    assert_eq!(
        fresh.window_size, None,
        "the added window has no saved frame, so it opens at the default size"
    );
    assert_eq!(
        plans[1].tabs[0].saved.as_ref(),
        Some(&window.tabs[0]),
        "the saved window must still restore its saved tab behind the added one"
    );
}

/// G3g.
#[test]
fn a_terminal_launch_from_a_held_project_restores_as_saved() {
    let window = saved_window(1000.0, 0, false, vec![saved_tab(DIR_A, "one", true)]);
    let plans = restore_plans(
        Some(&WindowState {
            windows: vec![window.clone()],
        }),
        Some(Path::new(DIR_A)),
        Some(Path::new(HOME)),
        is_dir,
    );
    assert_eq!(
        plans.len(),
        1,
        "a launch from {DIR_A}, which a saved tab holds, must restore the session exactly as \
         saved; {} plans means a window was added for a directory already open",
        plans.len()
    );
    assert_eq!(
        plans[0].tabs[0].saved.as_ref(),
        Some(&window.tabs[0]),
        "the one plan must be the saved window, with its saved tab"
    );
}

/// G3h.
#[test]
fn a_saved_window_with_no_tabs_is_dropped() {
    let hollow = saved_window(1100.0, 0, false, Vec::new());
    let window = saved_window(900.0, 0, false, vec![saved_tab(DIR_B, "two", true)]);
    let plans = plans_of(vec![hollow, window]);
    assert_eq!(
        first_cwds(&plans),
        [PathBuf::from(DIR_B)],
        "a saved window with no tabs has nothing to restore and must be dropped, leaving the \
         window at {DIR_B}. Two plans means the tab-less window got a fresh tab of its own"
    );
    assert_eq!(
        plans[0].window_size,
        Some((900, 700)),
        "the remaining plan must be the second saved window, at its own size"
    );
}

/// A window's live tab at `directory`, as the save side builds it: lone pane `marker`.
fn live_tab(directory: &str, marker: &str) -> Tab {
    tab_entry(
        directory.to_owned(),
        true,
        edit(&format!("{directory}/{marker}.md")),
        None,
    )
}

/// Recency over [`T10`], [`T11`], [`T12`] with `first` the most recent and the rest in order.
fn recency_from(first: usize) -> TabRecency {
    TabRecency::seeded(&[T10, T11, T12], first)
}

/// G4a.
#[test]
fn the_active_tab_is_the_selected_tab_among_contributing_tabs() {
    let tabs = vec![
        (T10, live_tab(DIR_A, "ten")),
        (T12, live_tab(DIR_A, "twelve")),
    ];
    let (window, _) = window_entry(1000, 700, tabs, Some(T12), &recency_from(0))
        .expect("a window with contributing tabs must contribute");
    assert_eq!(
        window.active_tab, 1,
        "tab 12 is selected and is the second contributing tab, so active_tab must be 1. 0 \
         means active_tab is always the first tab, or follows recency, where tab 10 leads"
    );
}

/// G4b.
#[test]
fn a_selected_tab_that_did_not_contribute_falls_back_to_the_most_recent() {
    let mut recency = recency_from(1);
    recency.select(T12);
    recency.select(T11);
    assert_eq!(
        recency.indices(&[T10, T11, T12]),
        vec![1, 2, 0],
        "premise: the recency must read 11, 12, 10"
    );
    let tabs = vec![
        (T10, live_tab(DIR_A, "ten")),
        (T12, live_tab(DIR_A, "twelve")),
    ];
    let (window, _) = window_entry(1000, 700, tabs, Some(T11), &recency)
        .expect("a window with contributing tabs must contribute");
    assert_eq!(
        window.active_tab, 1,
        "the selected tab 11 contributed nothing, so active_tab must be the most recently \
         selected contributing tab, 12, at index 1. 0 means the fallback is the first tab"
    );
}

/// G4c.
#[test]
fn recency_is_written_as_indices_into_the_contributing_tabs() {
    let tabs = vec![
        (T10, live_tab(DIR_A, "ten")),
        (T12, live_tab(DIR_A, "twelve")),
    ];
    let (_, recency) = window_entry(1000, 700, tabs, Some(T12), &recency_from(2))
        .expect("a window with contributing tabs must contribute");
    assert_eq!(
        recency,
        vec![1, 0],
        "the recency 12, 10, 11 over the contributing tabs [10, 12] must be written as [1, 0]: \
         indices into the contributing tabs, most recent first, the absent 11 skipped. [0, 1] \
         means tab order is written instead"
    );
}

/// G4d.
#[test]
fn a_window_frame_has_no_position() {
    let first = live_tab(DIR_A, "ten");
    let second = live_tab(DIR_B, "twelve");
    let tabs = vec![(T10, first.clone()), (T12, second.clone())];
    let (window, _) = window_entry(1234, 567, tabs, Some(T10), &recency_from(0))
        .expect("a window with contributing tabs must contribute");
    assert_eq!(
        window.frame,
        frame(1234.0, 567.0),
        "the frame must carry the size given and no position, since a Wayland client is never \
         told its own; Some(0.0) for x or y would be read back as a real position"
    );
    assert_eq!(
        window.tabs,
        [first, second],
        "the window must hold its contributing tabs as given, in order"
    );
    assert!(
        !window.minimized,
        "a window entry is never minimized; the stacking order sets that"
    );
}

/// One contributing window for the stacking cases, whose frame width and tab count encode
/// `id`, so the written order shows which window is where. Its recency lists its tabs last
/// first, so each window's list differs from every other's.
fn stacked(id: WindowId) -> (WindowId, Window, Vec<usize>) {
    let count = usize::from(id);
    let window = Window {
        frame: frame(1000.0 + f64::from(id), 700.0),
        active_tab: 0,
        tabs: (0..count)
            .map(|tab| live_tab(DIR_A, &format!("window{id}tab{tab}")))
            .collect(),
        minimized: false,
    };
    (id, window, (0..count).rev().collect())
}

/// The id each written window came from, read back from the frame [`stacked`] gave it.
fn written_ids(state: &WindowState) -> Vec<f64> {
    state
        .windows
        .iter()
        .map(|window| window.frame.width - 1000.0)
        .collect()
}

/// G4e.
#[test]
fn windows_are_written_in_stacking_order() {
    let stacking = StackingOrder::seeded(&[(2, false), (3, false), (1, false)]);
    let (state, recency) = window_state(vec![stacked(1), stacked(2), stacked(3)], &stacking)
        .expect("three contributing windows must commit a window state");
    assert_eq!(
        written_ids(&state),
        [2.0, 3.0, 1.0],
        "the windows must be written in stacking order, 2, 3, 1, frontmost first. 1, 2, 3 \
         means they are written in registry order, so the saved frontmost window is wrong"
    );
    assert_eq!(
        recency,
        [vec![1, 0], vec![2, 1, 0], vec![0]],
        "each window's recency must move with its window. Lists in input order mean the \
         recency no longer matches the window it belongs to"
    );
}

/// G4f.
#[test]
fn a_minimized_window_is_written_minimized_and_last() {
    let stacking = StackingOrder::seeded(&[(2, false), (3, true), (1, false)]);
    let (state, _) = window_state(vec![stacked(1), stacked(2), stacked(3)], &stacking)
        .expect("three contributing windows must commit a window state");
    assert_eq!(
        written_ids(&state),
        [2.0, 1.0, 3.0],
        "the minimized window 3 must be written behind every window that is not"
    );
    let minimized: Vec<bool> = state
        .windows
        .iter()
        .map(|window| window.minimized)
        .collect();
    assert_eq!(
        minimized,
        [false, false, true],
        "window 3 is minimized and must be written so; [false, false, false] means the flag \
         is never written and restore cannot minimize it"
    );
}

/// G4g.
#[test]
fn no_window_commits_nothing() {
    assert_eq!(
        window_state(Vec::new(), &StackingOrder::default()),
        None,
        "with no contributing window there is nothing to commit; Some of an empty state would \
         overwrite a saved session with no window at all"
    );
}

/// G4h.
#[test]
fn a_window_with_no_contributing_tab_contributes_nothing() {
    assert_eq!(
        window_entry(1000, 700, Vec::new(), Some(T10), &recency_from(0)),
        None,
        "a window none of whose tabs contributed must contribute nothing; Some with no tabs is \
         a window §2 does not allow, and would be saved over the window's last known tabs"
    );
}

/// G5a.
#[test]
fn an_empty_tab_writes_no_root_and_no_zoom() {
    let tab = tab_entry(String::new(), true, edit("/p/a/stray.md"), Some(0));
    assert_eq!(tab.directory, "", "an empty tab keeps its empty directory");
    assert_eq!(
        (tab.content.root.as_ref(), tab.content.zoomed),
        (None, None),
        "an empty tab must write root and zoomed as null, as §2 requires (root, zoomed); a root \
         means the save side writes whatever the empty tab's Neovim shows"
    );
}

/// G5b.
#[test]
fn a_folder_tab_writes_its_root_and_zoom() {
    let root = layout_of(&["/p/a/left.md", "/p/a/right.md"]);
    let tab = tab_entry(DIR_A.to_owned(), false, root.clone(), Some(1));
    assert_eq!(
        tab,
        Tab {
            directory: DIR_A.to_owned(),
            content: TabContent {
                root: Some(root),
                focused: None,
                zoomed: Some(PaneRef::Pane(1)),
                sidebar: Sidebar {
                    visible: false,
                    ..Sidebar::default()
                },
                drawer: None,
            },
        },
        "a folder tab must write its directory, its root, its zoom as the pane's index, its \
         sidebar preference and nothing else. zoomed None means the zoom is dropped; visible \
         true means the preference is ignored; a focus, drawer or sidebar mode means a field \
         this front-end does not save is written"
    );
}

/// G5c.
#[test]
fn a_lone_empty_pane_is_a_folder_tab_s_root() {
    let tab = tab_entry(DIR_A.to_owned(), true, edit(""), None);
    assert_eq!(
        tab.content.root,
        Some(edit("")),
        "a folder tab whose lone pane holds no file must still write that pane as its root, \
         since §2 allows a null root only for an empty tab; None means the save side suppresses \
         it"
    );
}

/// G5d and G5e.
#[test]
fn a_restored_tab_contributes_its_saved_tab() {
    let saved = saved_tab(DIR_A, "one", true);
    let mut expected = saved.clone();
    expected.content.sidebar.visible = false;
    assert_eq!(
        seeded_tab(Some(&saved), false),
        Some(expected),
        "a restored tab must contribute its saved tab, with the live sidebar preference false \
         over the saved true. None means a restored tab contributes nothing until its gather \
         opens; visible true means the saved preference wins over the live one"
    );

    let hidden = saved_tab(DIR_A, "one", false);
    let mut shown = hidden.clone();
    shown.content.sidebar.visible = true;
    assert_eq!(
        seeded_tab(Some(&hidden), true),
        Some(shown),
        "the live preference true must win over the saved false as well"
    );

    assert_eq!(
        seeded_tab(None, true),
        None,
        "a tab with no saved tab has no seed"
    );
}

/// G7a. Input and output are in build order, backmost first; `Some(j)` waits for window `j`.
#[test]
fn restored_windows_present_backmost_first_each_after_the_one_behind() {
    assert_eq!(
        present_after(&[]),
        Vec::<Option<usize>>::new(),
        "no window has nothing to schedule"
    );
    assert_eq!(
        present_after(&[false]),
        [None],
        "a lone window must be presented at once; Some(0) waits on itself and never maps"
    );
    assert_eq!(
        present_after(&[false, false, false]),
        [None, Some(0), Some(1)],
        "each window must wait for the one built just before it, so the three map backmost \
         first. [None, None, Some(1)] is a burst of the back windows, which map in no promised \
         order; [None, Some(0), Some(0)] lets window 2 map before window 1"
    );
}

/// G7b.
#[test]
fn a_minimized_window_is_presented_at_once_and_nothing_waits_for_it() {
    assert_eq!(
        present_after(&[true, false, false]),
        [None, None, Some(1)],
        "a minimized backmost window must be presented at once, and window 1, the first not \
         minimized, must too; Some(0) for window 1 waits on a minimized window that may never \
         paint, which would keep every window in front of it unmapped"
    );
    assert_eq!(
        present_after(&[false, true, false]),
        [None, None, Some(0)],
        "the minimized window 1 must be presented at once, and window 2 must wait for window 0, \
         the nearest before it that is not minimized; Some(1) for window 2 waits on the \
         minimized window, and Some(0) for window 1 chains a minimized window"
    );
}
