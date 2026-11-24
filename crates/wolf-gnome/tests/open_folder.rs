//! Table-driven tests for `wolf_gnome::open_folder`, the seam of `specs/application-state.md` §6:
//! which step Open Folder takes for a directory `D` over the live windows (`step`), the plan of
//! the tab it fills or opens (`plan`), with the sidebar New Tab captured for a tab it made, and
//! the size a fill applies (`fill_size`).
//!
//! Pure: no display, no Neovim and no filesystem. Window ids and tab ids are chosen so no id
//! equals an index of the lists asserted on, and every live window list is passed out of
//! stacking order, so only the stacking can put it right. `D` is a directory that does not exist
//! on any host, so a plan that consulted the filesystem would plan it as an empty tab. Whether
//! the coordinator feeds `step` the live windows and acts on its answer is the Open Folder
//! smokes' question, in `smoke_headless/open_folder.rs` and `smoke_headless/sidebar.rs`.

use std::path::{Path, PathBuf};

use wolf_core::app_state::{
    Drawer, Frame, Layout, Node, Pane, PaneOrientation, PaneRef, ProjectState, Sidebar,
    SidebarMode, Tab, TabContent,
};
use wolf_gnome::coordinator::WindowId;
use wolf_gnome::drawer::DrawerShare;
use wolf_gnome::open_folder::{fill_size, plan, step, LiveWindow, Step};
use wolf_gnome::search_panel::{SearchFields, SearchToggles};
use wolf_gnome::sidebar::SidebarPage;
use wolf_gnome::stacking::{StackingOrder, TabRecency};
use wolf_gnome::tabs_ui::TabId;

/// The directory being opened. Not on any host, so only an explicit folder plan opens it.
const D: &str = "/wolf-open-folder-test/project";
/// A directory other tabs are open on.
const E: &str = "/wolf-open-folder-test/elsewhere";
/// A third directory, for a back window's selected tab.
const C: &str = "/wolf-open-folder-test/other";

/// Three window ids. None is 0, 1 or 2, so an index mistaken for an id shows.
const W7: WindowId = 7;
const W8: WindowId = 8;
const W9: WindowId = 9;

/// The stored frame's size, every dimension distinct from the other's.
const STORED: (i32, i32) = (1234, 567);

/// A live window `id` of `tabs` in page order, `selected` selected, its recency built by
/// selecting `selections` in order, so the last is the most recent.
fn live(id: WindowId, tabs: &[(u64, &str)], selected: u64, selections: &[u64]) -> LiveWindow {
    let ids: Vec<TabId> = tabs.iter().map(|&(t, _)| TabId(t)).collect();
    let mut recency = TabRecency::default();
    for &t in selections {
        recency.select(TabId(t));
    }
    LiveWindow {
        id,
        tabs: ids
            .into_iter()
            .zip(tabs.iter().map(|&(_, dir)| dir.to_owned()))
            .collect(),
        selected: Some(TabId(selected)),
        recency,
    }
}

/// The stacking that focuses `windows` in order, so the last is frontmost.
fn stacking(windows: &[WindowId]) -> StackingOrder {
    let mut order = StackingOrder::default();
    for &w in windows {
        order.focus(w);
    }
    order
}

/// G1.
#[test]
fn a_tab_on_the_directory_in_a_back_window_is_focused() {
    let front = live(W7, &[(11, E)], 11, &[11]);
    let back = live(W8, &[(21, C), (22, D)], 21, &[22, 21]);
    assert_eq!(
        step(D, &[back, front], &stacking(&[W8, W7]), false),
        Step::Focus {
            window: W8,
            tab: TabId(22)
        },
        "D is open only in tab 22, a background tab of the back window 8, so §6 step 1 must \
         focus it. NewWindow means the lookup found no tab on D; a window or tab of 0 or 1 means \
         an index was answered for an id"
    );
}

/// G2.
#[test]
fn the_frontmost_window_whose_selected_tab_is_on_the_directory_wins() {
    let behind = live(W7, &[(31, D)], 31, &[31]);
    let ahead = live(W8, &[(41, D)], 41, &[41]);
    let invoking = live(W9, &[(51, E)], 51, &[51]);
    assert_eq!(
        step(
            D,
            &[behind, ahead, invoking],
            &stacking(&[W7, W8, W9]),
            false
        ),
        Step::Focus {
            window: W8,
            tab: TabId(41)
        },
        "windows 7 and 8 both have their selected tab on D, and 8 is ahead of 7 in the stacking, \
         so §4 rule 1 picks tab 41. Focus on window 7, tab 31 means the state was built in input \
         order rather than stacking order"
    );
}

/// G3.
#[test]
fn the_more_recent_of_two_background_tabs_on_the_directory_wins() {
    let window = live(W7, &[(61, E), (62, D), (63, D)], 61, &[62, 63, 61]);
    assert_eq!(
        step(D, &[window], &stacking(&[W7]), false),
        Step::Focus {
            window: W7,
            tab: TabId(63)
        },
        "no selected tab is on D, so §4 rule 2 picks the D tab selected most recently, 63, the \
         later in page order. Tab 62 means no recency reached project_tab and page order decided"
    );
}

/// G4.
#[test]
fn an_empty_current_tab_is_filled_when_no_tab_is_on_the_directory() {
    let other = live(W7, &[(81, E)], 81, &[81]);
    let invoking = live(W8, &[(71, "")], 71, &[71]);
    assert_eq!(
        step(D, &[other, invoking], &stacking(&[W7, W8]), true),
        Step::FillCurrent,
        "no tab is on D and the current tab is empty, so §6 step 2 must fill it. NewWindow means \
         current_empty was ignored; Focus means the empty tab's \"\" matched D"
    );
}

/// G5.
#[test]
fn a_current_tab_with_a_directory_opens_a_new_window() {
    let other = live(W7, &[(101, "")], 101, &[101]);
    let invoking = live(W8, &[(91, E)], 91, &[91]);
    assert_eq!(
        step(D, &[other, invoking], &stacking(&[W7, W8]), false),
        Step::NewWindow,
        "no tab is on D and the current tab has a directory, so §6 step 3 must open a new window, \
         though window 7 holds an empty tab. FillCurrent means a tab with a directory would be \
         replaced, which §6 forbids"
    );
}

/// A vertical layout of two edit panes, `a.md` and `b.md` under [`D`].
fn two_panes() -> Node {
    let pane = |name: &str| {
        Node::Pane(Pane::Edit {
            path: format!("{D}/{name}"),
            cursor: None,
            top_line: None,
        })
    };
    Node::Layout(Layout {
        orientation: PaneOrientation::Vertical,
        weights: vec![0.5, 0.5],
        children: vec![pane("a.md"), pane("b.md")],
    })
}

/// The project state G6 plans from: two panes, a hidden sidebar on Search with a query and
/// toggles away from their initial values, an open drawer, and a [`STORED`] frame.
fn project() -> ProjectState {
    ProjectState {
        frame: Frame {
            x: None,
            y: None,
            width: f64::from(STORED.0),
            height: f64::from(STORED.1),
        },
        tab: TabContent {
            root: Some(two_panes()),
            focused: Some(PaneRef::Pane(1)),
            zoomed: None,
            sidebar: Sidebar {
                visible: false,
                mode: Some(SidebarMode::Search),
                query: Some("needle".to_owned()),
                case_sensitive: Some(!SearchToggles::INITIAL.case_sensitive),
                use_regex: Some(!SearchToggles::INITIAL.use_regex),
                all_files: Some(!SearchToggles::INITIAL.all_files),
            },
            drawer: Some(Drawer {
                open: true,
                share: 0.3,
            }),
        },
    }
}

/// G6.
#[test]
fn a_project_state_plans_its_saved_tab_at_the_directory() {
    let project = project();
    let planned = plan(Path::new(D), Some(&project), None);

    assert_eq!(
        planned.tabs.len(),
        1,
        "Open Folder opens one tab, whatever the project holds"
    );
    let tab = &planned.tabs[0];
    assert_eq!(tab.cwd, PathBuf::from(D), "the tab's Neovim starts in D");
    assert!(
        !tab.empty,
        "a tab Open Folder opens is a folder tab on D, though D does not exist on this host; \
         empty means the plan consulted the filesystem or launch_decision"
    );
    assert_eq!(
        tab.saved,
        Some(Tab {
            directory: D.to_owned(),
            content: project.tab.clone(),
        }),
        "the tab's seed and rebuild are the project's saved tab at D. None means plan ignored \
         the project; another directory means the tab was keyed elsewhere"
    );
    assert!(
        !tab.sidebar_visible,
        "the project hides its sidebar, so the plan must too; true means the fresh tab's \
         default was planned instead"
    );
    assert_eq!(
        tab.sidebar_page,
        SidebarPage::Search,
        "the project's sidebar shows Search; Files means the saved mode was not restored"
    );
    assert_eq!(
        tab.search,
        SearchFields {
            query: "needle".to_owned(),
            toggles: SearchToggles {
                case_sensitive: !SearchToggles::INITIAL.case_sensitive,
                use_regex: !SearchToggles::INITIAL.use_regex,
                all_files: !SearchToggles::INITIAL.all_files,
            },
        },
        "the search panel's query and toggles come from the project; the initial fields mean \
         they were not restored"
    );
    assert_eq!(
        tab.drawer_share,
        DrawerShare::restored(0.3),
        "the drawer share comes from the project; DrawerShare::new() means it was not restored"
    );
    assert_eq!(
        planned.window_size,
        Some(STORED),
        "the window takes the project's stored size; None means plan dropped the frame"
    );
    assert_eq!(planned.active_tab, 0, "the one tab is selected");
    assert!(!planned.minimized, "an opened folder is never minimized");
}

/// G7.
#[test]
fn no_project_state_plans_a_fresh_folder_tab() {
    let planned = plan(Path::new(D), None, None);
    assert_eq!(
        planned.tabs.len(),
        1,
        "Open Folder opens one tab, project state or not"
    );
    let tab = &planned.tabs[0];
    assert_eq!(
        tab.cwd,
        PathBuf::from(D),
        "the fresh tab's Neovim starts in D"
    );
    assert!(
        !tab.empty,
        "with no project state the tab is still a folder tab on D, though D does not exist on \
         this host; empty means launch_decision decided it"
    );
    assert_eq!(
        tab.saved, None,
        "a fresh tab has no seed and nothing to rebuild"
    );
    assert_eq!(
        planned.window_size, None,
        "with no stored frame there is no size to apply"
    );
    assert_eq!(planned.active_tab, 0, "the one tab is selected");
    assert!(!planned.minimized, "an opened folder is never minimized");
}

/// G8.
#[test]
fn a_fill_applies_the_stored_size_only_to_a_lone_tab() {
    let sized = plan(Path::new(D), Some(&project()), None);
    assert_eq!(
        sized.window_size,
        Some(STORED),
        "premise: the plan carries the project's stored size"
    );
    assert_eq!(
        fill_size(&sized, 1),
        Some(STORED),
        "a fill of its window's only tab applies the stored size; None means it was dropped"
    );
    assert_eq!(
        fill_size(&sized, 2),
        None,
        "a fill of one of two tabs keeps the window's size; a size means window_tabs is ignored"
    );
    assert_eq!(
        fill_size(&plan(Path::new(D), None, None), 1),
        None,
        "with no stored size there is nothing to apply"
    );
}

/// OP1.
#[test]
fn a_fill_of_a_new_tab_with_no_project_state_takes_its_captured_sidebar() {
    let planned = plan(Path::new(D), None, Some(false));
    let tab = &planned.tabs[0];
    assert_eq!(
        tab.saved, None,
        "premise: with no project state the tab is a fresh one, so nothing stored decides"
    );
    assert!(
        !tab.sidebar_visible,
        "New Tab captured a hidden sidebar and D has no project state, so §6 plans the sidebar \
         hidden. true means plan ignored new_tab_sidebar and planned the fresh tab's default"
    );
}

/// OP2.
#[test]
fn a_stored_sidebar_wins_over_a_captured_one() {
    let project = project();
    assert!(
        !project.tab.sidebar.visible,
        "premise: the fixture's project hides its sidebar, the opposite of the captured value"
    );
    let planned = plan(Path::new(D), Some(&project), Some(true));
    assert!(
        !planned.tabs[0].sidebar_visible,
        "D's project state hides the sidebar and New Tab captured it shown, so the stored \
         setting wins. true means the captured value was applied over the project's saved tab"
    );
}

/// OP3.
#[test]
fn with_nothing_captured_a_fresh_tab_shows_the_sidebar() {
    let planned = plan(Path::new(D), None, None);
    assert!(
        planned.tabs[0].sidebar_visible,
        "a fill of an empty tab New Tab did not make, or a new window, with no project state \
         keeps the fresh tab's default, shown. false means a missing capture was read as hidden"
    );
}
