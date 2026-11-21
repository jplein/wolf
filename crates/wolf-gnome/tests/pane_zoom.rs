//! Table-driven tests for `wolf_gnome::pane_zoom`, the Zoom Pane rules, and for the zoomed
//! leaf's trip through `wolf_gnome::pane_layout`: which pane a toggle zooms, which events
//! unzoom it or re-zoom it, and how the mark is saved and found again.
//!
//! The K ids are those of `crates/wolf-macos/Tests/WolfKeyMapTests/PaneZoomTests.swift`, so a
//! case added for a rule both front-ends apply has its twin on the other side. K6 has no case
//! here: it tests freezing the dividers, and no divider exists while a GNOME view is zoomed.
//! K10 tests `shown_alone` over bound ids where the Swift tests `visibleRects` over rects,
//! since GTK allocates the one host shown and needs no rect. K11 to K13 compose
//! `focus_origin`, `drawer::focus_target` and `zoomed_after` as `PaneInner::apply_focus` does.
//!
//! P1 and P2 take their ids from `PaneLayoutCodecTests.swift`. P3 is this file's own: the
//! Swift P3 covers the FFI push-builder's tokens, which GNOME does not have, so here P3 is
//! the `encode` and `decode` round trip a relaunch makes.
//!
//! Pure: no display, no Neovim and no compositor. What the pane area shows for an answer is
//! `smoke_headless/zoom_pane.rs`'s question.

use std::collections::HashMap;

use wolf_core::{FocusDir, PaneNode, PaneOrientation};
use wolf_gnome::drawer::{focus_target, DrawerState};
use wolf_gnome::layout::{Orientation, PaneId, PaneLayout, PaneTree};
use wolf_gnome::other_pane_seam::Target;
use wolf_gnome::pane_layout::{decode, encode, zoomed_leaf_index, LeafContent};
use wolf_gnome::pane_zoom::{
    focus_origin, other_pane_target, shown_alone, zoomed_after, ZoomEvent,
};

const P1: PaneId = 1;
const P2: PaneId = 2;
const P3: PaneId = 3;

/// The drawer's pane id, kept apart from the tree's ids as the pane view's are.
const DRAWER: PaneId = 9;

/// A pane id absent from every fixture: a zoomed pane that is gone, or an active pane no
/// pane list holds.
const ABSENT: PaneId = 5;

/// K1: an unzoomed toggle zooms the active pane, a tree leaf or the drawer.
#[test]
fn toggle_zooms_the_active_pane_the_drawer_included() {
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, None, Some(P2), &[P1, P2]),
        Some(P2),
        "an unzoomed toggle must zoom the active pane 2. None means the toggle never zooms; \
         Some(1) means it zooms a pane other than the active one"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, None, Some(DRAWER), &[P1, DRAWER]),
        Some(DRAWER),
        "an unzoomed toggle with the drawer active must zoom the drawer, which counts among the \
         view's panes. None means the drawer cannot be zoomed"
    );
}

/// K2: a toggle while zoomed unzooms, whichever pane is active.
#[test]
fn toggle_again_unzooms() {
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, Some(P2), Some(P2), &[P1, P2]),
        None,
        "a toggle while pane 2 is zoomed and active must unzoom. Some(2) means Zoom Pane cannot \
         be undone by Zoom Pane"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, Some(P2), Some(P1), &[P1, P2]),
        None,
        "a toggle while pane 2 is zoomed must unzoom even when the active pane reads 1. Some(1) \
         means the toggle rezooms the active pane instead of unzooming; Some(2) that it keeps \
         the zoom"
    );
}

/// K3: a toggle with nothing to hide, or with no active pane among the view's panes, does
/// nothing.
#[test]
fn toggle_with_nothing_to_hide_is_a_no_op() {
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, None, Some(P1), &[P1]),
        None,
        "a toggle over a lone pane must leave the view unzoomed: Some(1) means a pane with \
         nothing to hide is reported zoomed"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, None, Some(ABSENT), &[P1, P2]),
        None,
        "a toggle whose active pane 5 is not among the view's panes must leave it unzoomed: \
         Some(5) means the toggle zooms a pane the view does not hold"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, None, None, &[P1, P2]),
        None,
        "a toggle with no active pane must leave the view unzoomed: Some(_) zooms a pane nobody \
         chose"
    );
}

/// K4: a split and a drawer opening unzoom, since a pane joins the layout.
#[test]
fn a_split_and_a_drawer_open_unzoom() {
    assert_eq!(
        zoomed_after(ZoomEvent::Split, Some(P2), Some(P2), &[P1, P2]),
        None,
        "a split while pane 2 is zoomed must unzoom before the split, so the new pane shows; \
         Some(2) means the split lands hidden behind the zoomed pane"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::DrawerOpen, Some(P2), Some(P2), &[P1, P2]),
        None,
        "a drawer opening while pane 2 is zoomed must unzoom, as a split does; Some(2) means the \
         drawer opens hidden"
    );
}

/// K5: a change of panes unzooms only when the zoomed pane is gone.
#[test]
fn only_closing_the_zoomed_pane_unzooms() {
    assert_eq!(
        zoomed_after(ZoomEvent::PanesChanged, Some(P2), Some(P1), &[P1, DRAWER]),
        None,
        "the zoomed pane 2 closing must unzoom; Some(2) means the view stays zoomed on a pane \
         that is gone"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::PanesChanged, Some(P2), Some(P2), &[P2, DRAWER]),
        Some(P2),
        "a hidden pane 1 closing must keep pane 2 zoomed; None means any close unzooms"
    );
}

/// K7: Toggle Terminal unzooms, whether the drawer is closed or open and hidden, and keeps the
/// zoom only when the drawer is the zoomed pane, whose close then unzooms through K5.
#[test]
fn toggle_terminal_unzooms_unless_the_drawer_is_zoomed() {
    assert_eq!(
        zoomed_after(
            ZoomEvent::ToggleTerminal { drawer: None },
            Some(P2),
            Some(P2),
            &[P1, P2]
        ),
        None,
        "with tree pane 2 zoomed and the drawer closed, Toggle Terminal must unzoom before the \
         drawer opens; Some(2) means the drawer opens behind the zoomed pane"
    );
    assert_eq!(
        zoomed_after(
            ZoomEvent::ToggleTerminal {
                drawer: Some(DRAWER)
            },
            Some(P2),
            Some(P2),
            &[P1, P2, DRAWER]
        ),
        None,
        "with tree pane 2 zoomed and the drawer open and hidden, Toggle Terminal must unzoom \
         before the drawer closes; Some(2) means the user closes a drawer they cannot see"
    );
    assert_eq!(
        zoomed_after(
            ZoomEvent::ToggleTerminal {
                drawer: Some(DRAWER)
            },
            Some(DRAWER),
            Some(DRAWER),
            &[P1, P2, DRAWER]
        ),
        Some(DRAWER),
        "with the drawer zoomed, Toggle Terminal must keep the zoom until the drawer's close \
         unzooms it; None flashes the hidden panes for a round trip before the close"
    );
    assert_eq!(
        zoomed_after(
            ZoomEvent::ToggleTerminal {
                drawer: Some(DRAWER)
            },
            None,
            Some(P1),
            &[P1, P2, DRAWER]
        ),
        None,
        "unzoomed, Toggle Terminal must leave the view unzoomed; Some(1) means it zooms the \
         active pane"
    );
}

/// K8: while zoomed, the other-pane commands take their split path from a tree pane and do
/// nothing from the drawer.
#[test]
fn the_other_pane_commands_split_while_zoomed() {
    assert_eq!(
        other_pane_target(P2, Some(DRAWER)),
        Some(Target::Split),
        "a zoomed tree pane beside a drawer is the only visible pane, so the other-pane commands \
         must split; None makes them do nothing, Some(Existing(_)) binds into a hidden pane"
    );
    assert_eq!(
        other_pane_target(P2, None),
        Some(Target::Split),
        "a zoomed tree pane with no drawer is the only visible pane, so the other-pane commands \
         must split; None makes them do nothing"
    );
    assert_eq!(
        other_pane_target(DRAWER, Some(DRAWER)),
        None,
        "a zoomed drawer must leave the other-pane commands doing nothing, as from the drawer \
         unzoomed; Some(Split) means the drawer is split"
    );
}

/// K9: a focus move to another pane unzooms, Neovim's own move to a hidden pane included; a
/// move with no target, or onto the zoomed pane, keeps the zoom.
#[test]
fn a_focus_move_to_another_pane_unzooms() {
    let panes = [P1, P2, DRAWER];
    assert_eq!(
        zoomed_after(
            ZoomEvent::FocusMove { target: Some(P1) },
            Some(P2),
            Some(P2),
            &panes
        ),
        None,
        "a focus move to pane 1 while 2 is zoomed must unzoom, so the pane focused shows; \
         Some(2) means the user types into a pane they cannot see"
    );
    assert_eq!(
        zoomed_after(
            ZoomEvent::FocusMove { target: None },
            Some(P2),
            Some(P2),
            &panes
        ),
        Some(P2),
        "a focus move with no target must keep pane 2 zoomed; None means a chord at the edge \
         unzooms though focus stays where it is"
    );
    assert_eq!(
        zoomed_after(
            ZoomEvent::FocusMove { target: Some(P2) },
            Some(P2),
            Some(P2),
            &panes
        ),
        Some(P2),
        "Neovim reporting the zoomed pane itself must keep the zoom; None means every wake that \
         reads the current window unzooms"
    );
    assert_eq!(
        zoomed_after(
            ZoomEvent::FocusMove { target: Some(P1) },
            None,
            Some(P2),
            &panes
        ),
        None,
        "an unzoomed focus move must leave the view unzoomed; Some(1) or Some(2) means a focus \
         move zooms"
    );
}

/// K10: a zoomed view shows its pane alone; an unzoomed one, and one zoomed on a pane that is
/// not bound, show the whole layout.
#[test]
fn a_zoomed_view_shows_its_pane_alone() {
    assert_eq!(
        shown_alone(Some(P2), &[P1, P2, DRAWER]),
        Some(P2),
        "zoomed on 2, the pane area must show pane 2 alone; None means the whole layout shows \
         and the zoom is invisible"
    );
    assert_eq!(
        shown_alone(Some(DRAWER), &[P1, DRAWER]),
        Some(DRAWER),
        "zoomed on the drawer, the pane area must show the drawer alone; None means the drawer \
         cannot be shown zoomed"
    );
    assert_eq!(
        shown_alone(None, &[P1, P2, DRAWER]),
        None,
        "unzoomed, the pane area must show the whole layout; Some(_) hides panes nobody zoomed"
    );
    assert_eq!(
        shown_alone(Some(ABSENT), &[P1, P2, DRAWER]),
        None,
        "zoomed on pane 5, which is not bound, the whole layout must show rather than nothing; \
         Some(5) blanks the pane area after a missed unzoom"
    );
}

// A plus shape above a full-width drawer: T over [L | M | R] over B, then the drawer.
const T: PaneId = 11;
const L: PaneId = 12;
const M: PaneId = 13;
const R: PaneId = 14;
const B: PaneId = 15;

/// The raw pane-area size `PaneInner::apply_focus` passes `drawer::focus_target`.
const PLUS_W: f64 = 960.0;
const PLUS_H: f64 = 640.0;

/// `horizontal[T, vertical[L, M, R], B]`, built as the view builds its tree.
fn plus_tree() -> PaneTree {
    let mut tree = PaneTree::new(T);
    tree.split(T, L, Orientation::Horizontal);
    tree.split(L, B, Orientation::Horizontal);
    tree.split(L, M, Orientation::Vertical);
    tree.split(M, R, Orientation::Vertical);
    tree
}

/// The drawer bound below the plus, with M the last editor pane, so a move up from the
/// drawer is told apart from the bottom-most fallback B.
fn plus_drawer() -> DrawerState {
    let mut state = DrawerState::new();
    state.bind(DRAWER);
    state.record_active(M, true);
    state
}

/// A focus move from `from` while `zoomed` is zoomed, composed as `apply_focus` composes it:
/// the origin, its target in the unzoomed tree, and the zoom after the move.
fn zoomed_move(
    zoomed: PaneId,
    from: Option<PaneId>,
    dir: FocusDir,
) -> (Option<PaneId>, Option<PaneId>) {
    let tree = plus_tree();
    let target = focus_origin(Some(zoomed), from)
        .and_then(|origin| focus_target(&tree, origin, dir, plus_drawer(), PLUS_W, PLUS_H));
    let mut panes = tree.leaves();
    panes.push(DRAWER);
    let after = zoomed_after(
        ZoomEvent::FocusMove { target },
        Some(zoomed),
        Some(zoomed),
        &panes,
    );
    (target, after)
}

/// K11: from the zoomed pane, a move resolves in the unzoomed tree and unzooms, in every
/// direction and across both drawer bridges.
#[test]
fn a_focus_move_with_a_target_unzooms_in_each_direction() {
    assert_eq!(
        plus_tree().leaves(),
        vec![T, L, M, R, B],
        "premise: the plus fixture's leaves must read T, L, M, R, B, or the cases below test \
         another shape"
    );
    for (dir, want) in [
        (FocusDir::Up, T),
        (FocusDir::Down, B),
        (FocusDir::Left, L),
        (FocusDir::Right, R),
    ] {
        let (target, after) = zoomed_move(M, Some(M), dir);
        assert_eq!(
            target,
            Some(want),
            "zoomed on M ({M}), {dir:?} must resolve to its neighbour {want} in the unzoomed \
             tree; None means the move is resolved in the zoomed view, which has no other pane"
        );
        assert_eq!(
            after, None,
            "zoomed on M ({M}), a move {dir:?} to {want} must unzoom, so the pane focused shows; \
             Some({M}) means focus lands on a pane hidden behind the zoom"
        );
    }

    let (target, after) = zoomed_move(B, Some(B), FocusDir::Down);
    assert_eq!(
        target,
        Some(DRAWER),
        "zoomed on B ({B}), the bottom pane, down must bridge into the drawer ({DRAWER}); None \
         means the bridge is lost when the target is resolved"
    );
    assert_eq!(
        after, None,
        "zoomed on B ({B}), a move down into the drawer must unzoom; Some({B}) leaves the drawer \
         focused and hidden"
    );

    let (target, after) = zoomed_move(DRAWER, Some(DRAWER), FocusDir::Up);
    assert_eq!(
        target,
        Some(M),
        "zoomed on the drawer, up must reach the last editor pane M ({M}); None means a drawer \
         origin resolves nothing, Some({B}) that the MRU bridge fell back to the bottom-most pane"
    );
    assert_eq!(
        after, None,
        "zoomed on the drawer, a move up to M must unzoom; Some({DRAWER}) leaves M focused and \
         hidden"
    );
}

/// K12: a move with no target in the unzoomed tree keeps the zoom, at the tree's edges and
/// sideways from the drawer.
#[test]
fn a_focus_move_with_no_target_keeps_the_zoom() {
    for (zoomed, dir, name) in [
        (L, FocusDir::Left, "L moving left"),
        (T, FocusDir::Up, "T moving up"),
        (DRAWER, FocusDir::Left, "the drawer moving left"),
    ] {
        let (target, after) = zoomed_move(zoomed, Some(zoomed), dir);
        assert_eq!(
            target, None,
            "zoomed on {name} has no target; {target:?} means a direction other than down \
             bridges into the drawer, or the drawer resolves more than up"
        );
        assert_eq!(
            after,
            Some(zoomed),
            "zoomed on {name}, a move with no target must keep the zoom; None unzooms though \
             focus stays where it is"
        );
    }
}

/// K13: a zoomed move starts from the zoomed pane, whatever pane the caller resolved; an
/// unzoomed one starts from that pane.
#[test]
fn a_zoomed_focus_move_starts_from_the_zoomed_pane() {
    assert_eq!(
        focus_origin(Some(M), Some(L)),
        Some(M),
        "zoomed on M ({M}), a move must start from M even when Neovim's window reads L ({L}); \
         Some({L}) resolves the move from a hidden pane"
    );
    assert_eq!(
        focus_origin(None, Some(L)),
        Some(L),
        "unzoomed, a move must start from the pane it was made in, L ({L}); None drops every \
         unzoomed move"
    );
    assert_eq!(
        zoomed_move(M, Some(L), FocusDir::Right).0,
        Some(R),
        "zoomed on M with Neovim's window on L, right must reach M's neighbour R ({R}); \
         Some({M}) means the move started from L"
    );
}

/// K14: a restore re-zooms its pane only when that pane is restored and the view has another
/// pane for it to hide.
#[test]
fn a_restore_rezooms_only_a_restored_pane_with_something_to_hide() {
    assert_eq!(
        zoomed_after(ZoomEvent::Restore(P2), None, Some(P1), &[P1, P2]),
        Some(P2),
        "a restore of pane 2 over [1, 2] must zoom 2; None means the saved zoom is never \
         applied, Some(1) that the active pane is zoomed instead"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::Restore(ABSENT), None, Some(P1), &[P1, P2]),
        None,
        "a restore of pane 5, which was not restored, must leave the tab unzoomed; Some(5) zooms \
         a pane that is not there, Some(1) the wrong one"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::Restore(P1), None, Some(P1), &[P1]),
        None,
        "a restore of a lone pane must leave it unzoomed: it has nothing to hide, and Some(1) \
         reports a zoom nothing shows"
    );
}

/// The `LeafContent` of an editor pane showing `path`.
fn editor(path: &str) -> LeafContent {
    LeafContent {
        path: path.to_owned(),
        preview: false,
        terminal: false,
    }
}

/// A persisted editor leaf at `path`, marked when `zoomed`.
fn leaf(path: &str, zoomed: bool) -> PaneNode {
    PaneNode::Leaf {
        path: path.to_owned(),
        preview: false,
        terminal: false,
        zoomed,
    }
}

/// P1: `encode` marks the leaf whose id is `zoomed` and no other; a `zoomed` id that is not a
/// leaf of the layout, the drawer's, marks none, and neither does an unzoomed view.
#[test]
fn encode_marks_only_the_zoomed_leaf() {
    let mut tree = PaneTree::new(P1);
    tree.split(P1, P2, Orientation::Vertical);
    tree.split(P2, P3, Orientation::Vertical);
    let layout = tree.layout();
    let PaneLayout::Split {
        orientation: Orientation::Vertical,
        weights: thirds,
        children,
    } = &layout
    else {
        panic!("premise: splitting 1 and then 2 vertically must give a vertical split: {layout:?}");
    };
    assert_eq!(
        children,
        &vec![
            PaneLayout::Leaf(P1),
            PaneLayout::Leaf(P2),
            PaneLayout::Leaf(P3)
        ],
        "premise: the split must hold 1, 2 and 3 flat, or the middle leaf is not pane 2"
    );
    let contents: HashMap<PaneId, LeafContent> = HashMap::from([
        (P1, editor("/notes/a.md")),
        (P2, editor("/notes/b.md")),
        (P3, editor("/notes/c.md")),
    ]);
    let split = |children: Vec<PaneNode>| PaneNode::Split {
        orientation: PaneOrientation::Vertical,
        children,
        weights: thirds.clone(),
    };
    let unmarked = || {
        vec![
            leaf("/notes/a.md", false),
            leaf("/notes/b.md", false),
            leaf("/notes/c.md", false),
        ]
    };

    assert_eq!(
        encode(&layout, &contents, Some(P2)),
        split(vec![
            leaf("/notes/a.md", false),
            leaf("/notes/b.md", true),
            leaf("/notes/c.md", false),
        ]),
        "zoomed on 2, only the middle leaf may carry zoomed: true. No mark means encode ignores \
         `zoomed` and the zoom is never saved; a mark on 1 or 3 too means it marks every leaf"
    );
    assert_eq!(
        encode(&layout, &contents, Some(DRAWER)),
        split(unmarked()),
        "zoomed on the drawer, which is not a layout leaf, no leaf may be marked: the drawer is \
         not persisted, so a zoomed drawer persists unzoomed. A mark means encode marks on any \
         zoom rather than on the leaf's id"
    );
    assert_eq!(
        encode(&layout, &contents, None),
        split(unmarked()),
        "unzoomed, no leaf may be marked; a mark means a relaunch zooms a tab nobody zoomed"
    );
}

/// `vertical[a, horizontal[b, c]]`, with the leaves whose paths are in `marked` zoomed.
fn nested(marked: &[&str]) -> PaneNode {
    let leaf_of = |path: &str| leaf(path, marked.contains(&path));
    PaneNode::Split {
        orientation: PaneOrientation::Vertical,
        weights: vec![0.5, 0.5],
        children: vec![
            leaf_of("/a.md"),
            PaneNode::Split {
                orientation: PaneOrientation::Horizontal,
                weights: vec![0.5, 0.5],
                children: vec![leaf_of("/b.md"), leaf_of("/c.md")],
            },
        ],
    }
}

/// P2: `zoomed_leaf_index` is the depth-first index of the first marked leaf, which the
/// restore zips onto its rebuilt leaf ids.
#[test]
fn zoomed_leaf_index_is_the_depth_first_index() {
    assert_eq!(
        zoomed_leaf_index(&nested(&["/b.md"])),
        Some(1),
        "b, the first leaf of the nested split, is depth-first leaf 1; Some(0) means the index \
         counts within its split, None that the mark is not found"
    );
    assert_eq!(
        zoomed_leaf_index(&nested(&["/c.md"])),
        Some(2),
        "c is depth-first leaf 2; Some(1) means the index is the top-level child's"
    );
    assert_eq!(
        zoomed_leaf_index(&nested(&["/b.md", "/c.md"])),
        Some(1),
        "with two marks, from a hand edit, the first in depth-first order wins, as the leaf \
         init.lua leaves current; Some(2) means the last mark wins"
    );
    assert_eq!(
        zoomed_leaf_index(&nested(&[])),
        None,
        "an unmarked layout has no zoomed leaf; Some(_) zooms a tab nobody zoomed"
    );
}

/// The first id `decode` allocates in P3, apart from the saved tree's own ids.
const FIRST_RESTORED: PaneId = 10;

/// P3: a zoomed leaf saved by `encode` is found again after `decode`, at its depth-first
/// position among the rebuilt ids, and the restore event re-zooms it.
#[test]
fn the_zoomed_leaf_round_trips_through_encode_and_decode() {
    let mut tree = PaneTree::new(P1);
    tree.split(P1, P2, Orientation::Vertical);
    tree.split(P2, P3, Orientation::Horizontal);
    assert_eq!(
        tree.leaves(),
        vec![P1, P2, P3],
        "premise: vertical[1, horizontal[2, 3]] must read 1, 2, 3 depth-first, or pane 3 is not \
         the third leaf"
    );
    let contents: HashMap<PaneId, LeafContent> = HashMap::from([
        (P1, editor("/notes/a.md")),
        (P2, editor("/notes/b.md")),
        (P3, editor("/notes/c.md")),
    ]);
    let node = encode(&tree.layout(), &contents, Some(P3));
    let (rebuilt, ids) =
        decode(&node, FIRST_RESTORED).expect("a well-formed three-leaf tree must decode");
    assert_eq!(
        ids,
        rebuilt.leaves(),
        "premise: decode's ids must be the rebuilt tree's leaves in depth-first order"
    );

    let zoomed = zoomed_leaf_index(&node).and_then(|i| ids.get(i).copied());
    assert_eq!(
        zoomed,
        ids.get(2).copied(),
        "zoomed on 3 before the save, the restore must find the rebuilt leaf at 3's depth-first \
         position, {:?}. None means the mark was not saved or not found; another id means the \
         index and the rebuilt ids disagree on the order",
        ids.get(2)
    );
    assert_eq!(
        zoomed.and_then(|id| zoomed_after(
            ZoomEvent::Restore(id),
            None,
            ids.first().copied(),
            &ids
        )),
        ids.get(2).copied(),
        "the restore event over the rebuilt leaves must re-zoom the found leaf; None means a \
         saved zoom is found and then dropped"
    );
}
