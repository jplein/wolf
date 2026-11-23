//! Table-driven tests for `wolf_gnome::open_request`, the seam that routes a request from
//! outside Wolf (`specs/cli-and-file-open.md` §3, §4 and §5.1): a live `WOLF_TAB_ID` names the
//! current tab, or the tab a file opens in, and otherwise the current tab is the frontmost
//! window's active tab and a file goes to its project. Also the seam's mapping of core's §5.2
//! pane choice onto a tab's leaves (`placement`), the liveness gate on placing a file
//! (`may_place`), and the whole-tab split a new left pane makes (`PaneTree::split_root_left`).
//!
//! The R and M cases are those of `crates/wolf-macos/Tests/WolfKeyMapTests/FileOpenSeamTests.swift`
//! that a pure seam can hold. Pure: no display and no Neovim. Window and tab ids equal no
//! index, and every window list is passed out of stacking order, so only the stacking puts the
//! current tab right. The directories exist on no host and no `.git` lies above them, so
//! `project_directory` answers from the open directories alone. Whether the coordinator feeds
//! the seam its live tabs and acts on the route is `smoke_headless/file_open.rs`'s question.

use std::collections::HashMap;
use std::path::PathBuf;

use wolf_core::{Liveness, OpenTarget, PanePlacement, TabId as WolfTabId};
use wolf_gnome::coordinator::WindowId;
use wolf_gnome::layout::{Orientation, PaneId, PaneLayout, PaneTree};
use wolf_gnome::open_folder::LiveWindow;
use wolf_gnome::open_request::{current_tab, may_place, placement, route, Placement, Route};
use wolf_gnome::stacking::{StackingOrder, TabRecency};
use wolf_gnome::tabs_ui::TabId;

/// The directories tabs are open on. None exists on any host.
const A: &str = "/wolf-open-request-test/alpha";
const B: &str = "/wolf-open-request-test/beta";
const C: &str = "/wolf-open-request-test/gamma";
const E: &str = "/wolf-open-request-test/epsilon";

/// The back window and the front one. Neither is 0 or 1, so an index mistaken for an id shows.
const BACK: WindowId = 7;
const FRONT: WindowId = 8;
/// A window id no live window has.
const GONE: WindowId = 9;

/// A live window `id` of `tabs` in page order, `selected` selected, its recency built by
/// selecting `selections` in order, so the last is the most recent.
fn live(
    id: WindowId,
    tabs: &[(u64, &str)],
    selected: Option<u64>,
    selections: &[u64],
) -> LiveWindow {
    let mut recency = TabRecency::default();
    for &t in selections {
        recency.select(TabId(t));
    }
    LiveWindow {
        id,
        tabs: tabs
            .iter()
            .map(|&(t, dir)| (TabId(t), dir.to_owned()))
            .collect(),
        selected: selected.map(TabId),
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

/// The tabs and tab IDs most cases route over: the back window holds tab 11 on A, in the
/// background, and tab 12 on E, selected; the front window holds tab 21 on C and tab 22 on B,
/// 22 selected. The list puts the back window first.
struct Fixture {
    windows: Vec<LiveWindow>,
    stacking: StackingOrder,
    ids: HashMap<WolfTabId, (WindowId, TabId)>,
    /// Tab 11's ID.
    id_11: WolfTabId,
    /// An ID the map does not hold: a closed tab, or a relaunched Wolf.
    unmapped: WolfTabId,
    /// An ID mapped to tab 99 of the back window, which that window does not hold.
    missing_tab: WolfTabId,
    /// An ID mapped to tab 11 of a window that is not live, though tab 11 lives in another.
    missing_window: WolfTabId,
}

fn fixture() -> Fixture {
    let id_11 = WolfTabId::random();
    let id_22 = WolfTabId::random();
    let missing_tab = WolfTabId::random();
    let missing_window = WolfTabId::random();
    let ids = HashMap::from([
        (id_11, (BACK, TabId(11))),
        (id_22, (FRONT, TabId(22))),
        (missing_tab, (BACK, TabId(99))),
        (missing_window, (GONE, TabId(11))),
    ]);
    Fixture {
        windows: vec![
            live(BACK, &[(11, A), (12, E)], Some(12), &[11, 12]),
            live(FRONT, &[(21, C), (22, B)], Some(22), &[21, 22]),
        ],
        stacking: stacking(&[BACK, FRONT]),
        ids,
        id_11,
        unmapped: WolfTabId::random(),
        missing_tab,
        missing_window,
    }
}

impl Fixture {
    fn route(&self, target: &OpenTarget, tab_id: Option<WolfTabId>) -> Route {
        route(target, tab_id, &self.windows, &self.ids, &self.stacking)
    }
}

/// The front window's selected tab, the current tab whenever no live ID names one.
const FRONT_ACTIVE: (WindowId, TabId) = (FRONT, TabId(22));

/// R1.
#[test]
fn an_activate_takes_the_frontmost_windows_selected_tab() {
    let f = fixture();
    assert_eq!(
        current_tab(&f.windows, &f.stacking),
        Some(FRONT_ACTIVE),
        "the current tab is the frontmost window's selected tab, 22 of window 8. Window 7's \
         tab 12 means the first window in list order was taken as frontmost"
    );
    assert_eq!(
        f.route(&OpenTarget::Activate, None),
        Route::Activate {
            current: Some(FRONT_ACTIVE)
        },
        "a bare `wolf` activates the frontmost window's selected tab, 22 of window 8. None means \
         the current tab is dropped; window 7 means list order decided the front"
    );
    assert_eq!(
        current_tab(&[], &StackingOrder::default()),
        None,
        "with no window there is no current tab"
    );
    assert_eq!(
        route(
            &OpenTarget::Activate,
            None,
            &[],
            &HashMap::new(),
            &StackingOrder::default()
        ),
        Route::Activate { current: None },
        "an activate with no window carries no current tab"
    );
}

/// R2.
#[test]
fn a_window_with_no_selected_tab_takes_its_most_recent_tab() {
    let back = live(BACK, &[(11, A)], Some(11), &[11]);
    let front = live(FRONT, &[(21, C), (22, B)], None, &[21, 22]);
    assert_eq!(
        current_tab(&[back, front], &stacking(&[BACK, FRONT])),
        Some((FRONT, TabId(22))),
        "with no selected tab, the frontmost window's active tab is its most recently selected, \
         22. Tab 21 means the first tab in page order was taken; None means a window with no \
         selected tab has no current tab"
    );
}

/// R3.
#[test]
fn a_directory_with_a_live_tab_id_takes_that_tab_as_current() {
    let f = fixture();
    assert_eq!(
        f.route(&OpenTarget::Directory(PathBuf::from(C)), Some(f.id_11)),
        Route::OpenFolder {
            directory: PathBuf::from(C),
            current: Some((BACK, TabId(11))),
        },
        "a directory sent from tab 11's terminal is Open Folder with tab 11, a background tab of \
         the back window, as the current tab. Tab 22 of window 8 means directories ignore the \
         tab ID; tab 12 means the ID's window was taken but not its tab"
    );
}

/// R4.
#[test]
fn a_stale_tab_id_is_handled_as_unset() {
    let f = fixture();
    let file = PathBuf::from(format!("{B}/notes.md"));
    let stale = [
        ("an ID the map does not hold", f.unmapped),
        (
            "an ID mapped to a tab its window does not hold",
            f.missing_tab,
        ),
        (
            "an ID mapped to a window that is not live",
            f.missing_window,
        ),
    ];
    for (what, id) in stale {
        assert_eq!(
            f.route(&OpenTarget::Activate, Some(id)),
            Route::Activate {
                current: Some(FRONT_ACTIVE)
            },
            "an activate with {what} must take the frontmost window's selected tab, 22 of \
             window 8. Tab 99, or window 9, means the map was trusted without checking the live \
             windows"
        );
        assert_eq!(
            f.route(&OpenTarget::Directory(PathBuf::from(C)), Some(id)),
            Route::OpenFolder {
                directory: PathBuf::from(C),
                current: Some(FRONT_ACTIVE),
            },
            "a directory with {what} is handled as if no ID were set, so the current tab is \
             22 of window 8. Tab 99, or window 9, means the map was trusted without checking \
             the live windows"
        );
        assert_eq!(
            f.route(&OpenTarget::File(file.clone()), Some(id)),
            Route::OpenFileInProject {
                file: file.clone(),
                project: PathBuf::from(B),
                current: Some(FRONT_ACTIVE),
            },
            "a file with {what} goes to its project, B, as if no ID were set. OpenFile means a \
             stale ID was taken as live, or fell back to the current tab rather than to the \
             project"
        );
    }
}

/// R5.
#[test]
fn a_file_with_a_live_tab_id_opens_in_that_tab_whatever_its_project() {
    let f = fixture();
    let file = PathBuf::from(format!("{B}/notes.md"));
    assert_eq!(
        f.route(&OpenTarget::File(file.clone()), Some(f.id_11)),
        Route::OpenFile {
            file,
            tab: (BACK, TabId(11)),
        },
        "§5.1 rule 1: a file sent from tab 11 opens in tab 11, though it lies in B, open in tab \
         22. OpenFileInProject means the project was resolved ahead of the tab ID"
    );
}

/// R6.
#[test]
fn a_file_without_a_live_tab_id_goes_to_its_project() {
    let outer = "/wolf-open-request-test/outer";
    let sub = "/wolf-open-request-test/outer/sub";
    let back = live(BACK, &[(31, sub), (32, "")], Some(32), &[31, 32]);
    let front = live(FRONT, &[(41, outer), (42, E)], Some(42), &[41, 42]);
    let file = PathBuf::from(format!("{sub}/deep/f.md"));
    assert_eq!(
        route(
            &OpenTarget::File(file.clone()),
            None,
            &[back, front],
            &HashMap::new(),
            &stacking(&[BACK, FRONT]),
        ),
        Route::OpenFileInProject {
            file,
            project: PathBuf::from(sub),
            current: Some((FRONT, TabId(42))),
        },
        "with tabs open on {outer} and {sub}, the nearest open directory containing the file is \
         {sub}, so it is the project, and the current tab is the front window's selected 42. A \
         project of / means route gave project_directory no open directories, so it fell back \
         to the file's nearest existing ancestor; {outer} means the farthest open directory won"
    );
}

const L10: PaneId = 10;
const L20: PaneId = 20;
const L30: PaneId = 30;

/// M1.
#[test]
fn a_placement_names_the_leaf_at_its_index() {
    let leaves = [L10, L20, L30];
    let rows = [
        (PanePlacement::Focus(1), Some(Placement::Focus(L20))),
        (PanePlacement::Open(2), Some(Placement::Open(L30))),
        (PanePlacement::Convert(1), Some(Placement::Convert(L20))),
        (PanePlacement::SplitLeft, Some(Placement::SplitLeft)),
        (PanePlacement::Focus(3), None),
        (PanePlacement::Open(3), None),
        (PanePlacement::Convert(7), None),
    ];
    for (choice, expected) in rows {
        assert_eq!(
            placement(choice, &leaves),
            expected,
            "{choice:?} over the leaves {leaves:?} must name the leaf at its depth-first index, \
             keeping its kind, and None past the last leaf. Leaf 10 means every index mapped to \
             the first leaf; a kind changed means Focus, Open and Convert were merged"
        );
    }
}

/// G1.
#[test]
fn placing_waits_for_an_alive_neovim() {
    assert!(
        may_place(Some(Liveness::Alive)),
        "a file is placed in a tab whose Neovim is alive; false means no file is ever placed"
    );
    for state in [Some(Liveness::NotResponding), Some(Liveness::Dead), None] {
        assert!(
            !may_place(state),
            "a file must wait while the tab's Neovim is {state:?}, since placing it can make a \
             blocking call that a stalled or dead Neovim never answers"
        );
    }
}

/// The weights of `layout`'s root split, with its orientation and children.
fn root_split(layout: &PaneLayout) -> (Orientation, &[PaneLayout], &[f64]) {
    match layout {
        PaneLayout::Split {
            orientation,
            children,
            weights,
        } => (*orientation, children, weights),
        PaneLayout::Leaf(id) => {
            panic!("the root must be a split after split_root_left; it is leaf {id}")
        }
    }
}

fn approx(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

/// T1.
#[test]
fn split_root_left_puts_the_new_pane_left_of_the_whole_tree() {
    const NEW: PaneId = 9;

    let mut leaf = PaneTree::new(1);
    leaf.split_root_left(NEW);
    assert_eq!(
        leaf.layout(),
        PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![PaneLayout::Leaf(NEW), PaneLayout::Leaf(1)],
            weights: vec![0.5, 0.5],
        },
        "a lone pane must become Vertical[new, pane] at halves. Leaf 1 first means the new pane \
         went right; Horizontal means the split was not vertical"
    );

    let mut row = PaneTree::from_layout(&PaneLayout::Split {
        orientation: Orientation::Vertical,
        children: vec![PaneLayout::Leaf(1), PaneLayout::Leaf(2)],
        weights: vec![0.3, 0.7],
    })
    .expect("a valid two-pane row");
    row.split_root_left(NEW);
    let row = row.layout();
    let (orientation, children, weights) = root_split(&row);
    assert_eq!(
        (orientation, children),
        (
            Orientation::Vertical,
            &[
                PaneLayout::Leaf(NEW),
                PaneLayout::Leaf(1),
                PaneLayout::Leaf(2)
            ][..]
        ),
        "a vertical root must gain the new pane as its first child, flat. [1, 2, 9] means it \
         was appended; a nested Vertical[9, Vertical[1, 2]] means the row was wrapped instead"
    );
    assert!(
        approx(weights, &[1.0 / 3.0; 3]),
        "the widened row's weights must all be equal; they are {weights:?}"
    );

    let column = PaneLayout::Split {
        orientation: Orientation::Horizontal,
        children: vec![PaneLayout::Leaf(1), PaneLayout::Leaf(2)],
        weights: vec![0.3, 0.7],
    };
    let mut tree = PaneTree::from_layout(&column).expect("a valid two-pane column");
    tree.split_root_left(NEW);
    assert_eq!(
        tree.layout(),
        PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![PaneLayout::Leaf(NEW), column.clone()],
            weights: vec![0.5, 0.5],
        },
        "a horizontal root must become Vertical[new, root] at halves, the column kept whole. \
         The new pane inside the column, or after it, means the split was not of the whole tab"
    );
}
