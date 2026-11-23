//! Table-driven tests for the per-pane extras' pure seams: how `wolf_gnome::pane_layout`
//! writes each leaf's captured view into the saved tree and seeds the views of a restored one,
//! how it names a pane as a saved `focused` or `zoomed` reference, and the preview scroll
//! report `wolf_gnome::preview_ui` reads and the script it applies a saved scroll with
//! (`specs/application-state.md` §2, §4 and §5).
//!
//! Pure: no display, no Neovim and no WebKit. Every expected value is written out by hand, and
//! pane ids are chosen apart from every index asserted on. Whether the pane view captures at
//! the right moments, and whether the page posts and applies a scroll, is the question of
//! `smoke_headless/extras.rs`.

use std::collections::HashMap;

use wolf_core::app_state::{Cursor, Layout, Node, Pane, PaneOrientation, PaneRef};
use wolf_gnome::layout::{Orientation, PaneId, PaneTree};
use wolf_gnome::pane_layout::{
    encode, encode_with_views, pane_ref, saved_views, LeafContent, PaneView,
};
use wolf_gnome::preview_ui::{
    reported_scroll, scroll_to_fraction_js, SCROLL_REPORT_HANDLER, SCROLL_REPORT_JS,
};

const P1: PaneId = 1;
const P2: PaneId = 2;
const P3: PaneId = 3;

/// The drawer's pane id, kept apart from the tree's ids as the pane view's are.
const DRAWER: PaneId = 9;

/// `vertical[1, horizontal[2, 3]]`, built as the pane view builds its tree.
fn three_leaves() -> PaneTree {
    let mut tree = PaneTree::new(P1);
    tree.split(P1, P2, Orientation::Vertical);
    tree.split(P2, P3, Orientation::Horizontal);
    assert_eq!(
        tree.leaves(),
        vec![P1, P2, P3],
        "premise: vertical[1, horizontal[2, 3]] must read 1, 2, 3 depth-first"
    );
    tree
}

/// Pane 1 an editor, pane 2 a preview and pane 3 a terminal, each on its own path.
fn three_contents() -> HashMap<PaneId, LeafContent> {
    HashMap::from([
        (
            P1,
            LeafContent {
                path: "/p/edit.md".to_owned(),
                preview: false,
                terminal: false,
            },
        ),
        (
            P2,
            LeafContent {
                path: "/p/source.md".to_owned(),
                preview: true,
                terminal: false,
            },
        ),
        (
            P3,
            LeafContent {
                path: "/p".to_owned(),
                preview: false,
                terminal: true,
            },
        ),
    ])
}

/// The cursor and top line every edit view here carries.
const EDIT_VIEW: PaneView = PaneView::Edit {
    cursor: Cursor { line: 12, col: 4 },
    top_line: 3,
};

/// The depth-first panes of `node`, cloned, so a whole leaf is compared at once.
fn panes_of(node: &Node) -> Vec<Pane> {
    node.panes().into_iter().cloned().collect()
}

/// E11a and E11b: each leaf takes the captured view of its own kind, and nothing else.
#[test]
fn encode_with_views_carries_each_leafs_captured_view() {
    let layout = three_leaves().layout();
    let contents = three_contents();
    let views = HashMap::from([
        (P1, EDIT_VIEW),
        (P2, PaneView::Preview { scroll: 0.25 }),
        (P3, EDIT_VIEW),
    ]);
    assert_eq!(
        panes_of(&encode_with_views(&layout, &contents, &views)),
        vec![
            Pane::Edit {
                path: "/p/edit.md".to_owned(),
                cursor: Some(Cursor { line: 12, col: 4 }),
                top_line: Some(3),
            },
            Pane::Preview {
                path: "/p/source.md".to_owned(),
                scroll: Some(0.25),
            },
            Pane::Terminal {
                path: "/p".to_owned(),
            },
        ],
        "the editor must carry its captured cursor and top line, the preview its captured \
         scroll, and the terminal nothing. A cursor of None means the edit leaf ignores the \
         views; a scroll of None, that the preview leaf does"
    );

    let crossed = HashMap::from([(P2, EDIT_VIEW)]);
    assert_eq!(
        panes_of(&encode_with_views(&layout, &contents, &crossed))[1],
        Pane::Preview {
            path: "/p/source.md".to_owned(),
            scroll: None,
        },
        "an edit view on the preview leaf, as a pane whose kind changed since its capture \
         leaves, must apply nothing: a scroll means the view is applied whatever its kind"
    );

    assert_eq!(
        encode(&layout, &contents),
        encode_with_views(&layout, &contents, &HashMap::new()),
        "`encode` must be `encode_with_views` with no views, or the two writers of a saved tree \
         disagree on everything but the views"
    );
}

/// E12a and E12b: only a leaf whose saved fields make a whole view seeds one, keyed by the
/// restored leaf at its depth-first position.
#[test]
fn saved_views_seed_each_restored_leaf() {
    let node = Node::Layout(Layout {
        orientation: PaneOrientation::Vertical,
        weights: vec![0.25; 4],
        children: vec![
            Node::Pane(Pane::Edit {
                path: "/p/a.md".to_owned(),
                cursor: Some(Cursor { line: 5, col: 1 }),
                top_line: Some(2),
            }),
            Node::Pane(Pane::Preview {
                path: "/p/a.md".to_owned(),
                scroll: Some(0.5),
            }),
            Node::Pane(Pane::Edit {
                path: "/p/b.md".to_owned(),
                cursor: Some(Cursor { line: 7, col: 0 }),
                top_line: None,
            }),
            Node::Pane(Pane::Terminal {
                path: "/p".to_owned(),
            }),
        ],
    });
    let leaf_ids: [PaneId; 4] = [7, 8, 9, 10];
    assert_eq!(
        saved_views(&node, &leaf_ids),
        HashMap::from([
            (
                7,
                PaneView::Edit {
                    cursor: Cursor { line: 5, col: 1 },
                    top_line: 2,
                },
            ),
            (8, PaneView::Preview { scroll: 0.5 }),
        ]),
        "leaf 7 must seed its saved cursor and top line and leaf 8 its saved scroll. 8 missing \
         means a preview's saved scroll is not seeded, so a capture before its page loads writes \
         it as lost. Ids shifted by one mean the zip is off by a leaf. An entry for 9 means an \
         edit pane with no top line seeds a made-up one; an entry for 10, that a terminal seeds \
         a view"
    );
}

/// E13a and E13b: a tree leaf is named by its depth-first index and the bound drawer as the
/// drawer, and anything else is named as nothing.
#[test]
fn pane_ref_names_a_leaf_or_the_drawer() {
    let layout = three_leaves().layout();
    let cases = [
        (
            Some(DRAWER),
            Some(P3),
            Some(PaneRef::Pane(2)),
            "pane 3, the third leaf",
        ),
        (
            Some(DRAWER),
            Some(P1),
            Some(PaneRef::Pane(0)),
            "pane 1, the first leaf",
        ),
        (
            Some(DRAWER),
            Some(DRAWER),
            Some(PaneRef::Drawer),
            "the bound drawer",
        ),
        (
            Some(DRAWER),
            Some(42),
            None,
            "pane 42, which is no pane of the view",
        ),
        (Some(DRAWER), None, None, "no pane"),
        (None, Some(DRAWER), None, "pane 9 with no drawer bound"),
    ];
    for (drawer, pane, want, what) in cases {
        assert_eq!(
            pane_ref(&layout, drawer, pane),
            want,
            "pane_ref with drawer {drawer:?} and pane {pane:?}, {what}: Pane(i) must be the \
             pane's depth-first index, Drawer only the bound drawer, and None anything else. \
             None for the drawer means a focused or zoomed drawer is saved as nothing; Some for \
             pane 42 means an unknown pane is saved as a leaf nobody focused"
        );
    }
}

/// E14a and E14b: a reported scroll is a fraction of the scrollable height, and a report that
/// names none is dropped.
#[test]
fn a_reported_scroll_is_a_fraction_from_zero_to_one() {
    let cases = [
        (0.25, Some(0.25)),
        (0.0, Some(0.0)),
        (1.0, Some(1.0)),
        (-0.5, Some(0.0)),
        (1.7, Some(1.0)),
        (f64::NAN, None),
        (f64::INFINITY, None),
        (f64::NEG_INFINITY, None),
    ];
    for (value, want) in cases {
        let got = reported_scroll(value);
        assert_eq!(
            got, want,
            "the page reported {value} and `reported_scroll` kept {got:?}, want {want:?}. 1.7 \
             kept means an overscrolled page is saved past its end, which §2 does not allow; \
             Some(NaN) means a report that names no position is saved and restores at the top"
        );
    }
}

/// E15a and E15b: the report script posts to the handler the pane view registers, and the
/// restore script applies only a finite fraction from 0 to 1.
#[test]
fn the_scroll_scripts_name_one_handler_and_a_finite_fraction() {
    let post = format!("messageHandlers.{SCROLL_REPORT_HANDLER}.postMessage");
    assert!(
        SCROLL_REPORT_JS.contains(&post),
        "SCROLL_REPORT_JS does not contain {post:?}, so the page posts to a handler other than \
         the one registered as SCROLL_REPORT_HANDLER, and no scroll is ever reported. The \
         script was:\n{SCROLL_REPORT_JS}"
    );

    let half = scroll_to_fraction_js(0.5);
    assert!(
        half.contains("0.5"),
        "scroll_to_fraction_js(0.5) does not contain 0.5, so the fraction never reaches the \
         page. The script was:\n{half}"
    );
    let top = scroll_to_fraction_js(0.0);
    let end = scroll_to_fraction_js(1.0);
    assert!(
        half != top && half != end,
        "scroll_to_fraction_js(0.5) is the script of 0 or of 1, so it ignores its fraction"
    );
    for (fraction, clamped, script) in [
        (2.0, 1.0, &end),
        (-1.0, 0.0, &top),
        (f64::NAN, 0.0, &top),
        (f64::INFINITY, 0.0, &top),
    ] {
        let got = scroll_to_fraction_js(fraction);
        assert_eq!(
            &got, script,
            "scroll_to_fraction_js({fraction}) must be the script of {clamped}: a fraction \
             outside 0 to 1 is clamped and a non-finite one is 0. A script holding `NaN`, `inf` \
             or the raw fraction means it is formatted unchecked into JavaScript"
        );
    }
}
