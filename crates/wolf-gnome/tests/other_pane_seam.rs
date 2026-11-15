//! Table-driven tests for `wolf_gnome::other_pane_seam::target`, the pane Preview or Edit
//! in Another Pane binds into.
//!
//! The cases mirror `crates/wolf-macos/Tests/WolfKeyMapTests/OtherPaneSeamTests.swift`
//! case for case, with the same trees, ids and bounds, so the two ports cannot drift
//! silently: a case added on one side belongs on the other. Swift's `.vertical` is
//! [`Orientation::Vertical`], side by side.
//!
//! Pure: no display, no Neovim and no compositor. Which window the shell passes for the
//! answer is the preview smokes' question.

use wolf_gnome::layout::{Orientation, PaneId, PaneTree};
use wolf_gnome::other_pane_seam::{target, Side, Target};

const P1: PaneId = 1;
const P2: PaneId = 2;
const P3: PaneId = 3;
const P4: PaneId = 4;

/// A pane id never inserted into any tree: the "active pane is unknown" (drawer-focused)
/// probe.
const UNKNOWN_PANE: PaneId = 99;

/// Test bounds width (px). Non-square, so width math and height math are distinguishable.
const W: f64 = 100.0;
/// Test bounds height (px).
const H: f64 = 40.0;

/// `[P1][P2]`, side by side.
fn two_side_by_side() -> PaneTree {
    let mut tree = PaneTree::new(P1);
    tree.split(P1, P2, Orientation::Vertical);
    tree
}

/// `[P1][P2][P3]`, side by side; `leaves()` is `[1, 2, 3]`.
fn three_in_a_row() -> PaneTree {
    let mut tree = two_side_by_side();
    tree.split(P2, P3, Orientation::Vertical);
    tree
}

#[test]
fn single_leaf_splits_both_sides() {
    let tree = PaneTree::new(P1);
    assert_eq!(
        target(P1, Side::Right, &tree, W, H),
        Some(Target::Split),
        "a lone pane has no other pane: prefer-right must ask for a split. \
         Some(Existing(1)) means the single-leaf rule is gone and the fallback bound the \
         pane into itself; None means the lone pane was not recognised as a leaf"
    );
    assert_eq!(
        target(P1, Side::Left, &tree, W, H),
        Some(Target::Split),
        "a lone pane has no other pane: prefer-left must ask for a split. \
         Some(Existing(1)) means the single-leaf rule is gone; None means the lone pane \
         was not recognised as a leaf"
    );
}

#[test]
fn two_side_by_side_prefer_right_uses_right_neighbor() {
    let tree = two_side_by_side();
    assert_eq!(
        target(P1, Side::Right, &tree, W, H),
        Some(Target::Existing(P2)),
        "with a real pane to the right, prefer-right must reuse it (rule 3). \
         Some(Split) is the always-split rule that opens a third pane"
    );
}

#[test]
fn two_side_by_side_prefer_left_wraps_to_other_pane() {
    let tree = two_side_by_side();
    assert_eq!(
        target(P1, Side::Left, &tree, W, H),
        Some(Target::Existing(P2)),
        "no pane sits left of P1, so the previous-with-wrap fallback must land on the only \
         other pane (P2). Some(Split) means the fallback asks for a split instead"
    );
}

#[test]
fn three_in_a_row_middle_uses_geometric_neighbors() {
    let tree = three_in_a_row();
    assert_eq!(
        target(P2, Side::Right, &tree, W, H),
        Some(Target::Existing(P3)),
        "from the middle pane, prefer-right must reuse the pane on its right. \
         Existing(1) means the side asks for the left neighbour"
    );
    assert_eq!(
        target(P2, Side::Left, &tree, W, H),
        Some(Target::Existing(P1)),
        "from the middle pane, prefer-left must reuse the pane on its left. \
         Existing(3) means the side asks for the right neighbour"
    );
}

#[test]
fn edge_panes_wrap_in_dfs_order() {
    let tree = three_in_a_row();
    assert_eq!(
        target(P3, Side::Right, &tree, W, H),
        Some(Target::Existing(P1)),
        "the rightmost pane has no right neighbour: next-with-wrap must land on the FIRST \
         leaf (P1). Existing(2) means the fallback took the previous leaf"
    );
    assert_eq!(
        target(P1, Side::Left, &tree, W, H),
        Some(Target::Existing(P3)),
        "the leftmost pane has no left neighbour: previous-with-wrap must land on the LAST \
         leaf (P3). Existing(2) means the fallback took the next leaf for both sides"
    );
}

#[test]
fn vertical_stack_falls_back_to_other_pane() {
    let mut tree = PaneTree::new(P1);
    tree.split(P1, P2, Orientation::Horizontal); // P1 stacked above P2
    assert_eq!(
        target(P1, Side::Right, &tree, W, H),
        Some(Target::Existing(P2)),
        "a stacked layout has no lateral neighbour: prefer-right must fall back to the other \
         existing pane (P2), never a split"
    );
    assert_eq!(
        target(P2, Side::Left, &tree, W, H),
        Some(Target::Existing(P1)),
        "prefer-left from the bottom pane must fall back to the other existing pane (P1), \
         never a split"
    );
}

#[test]
fn grid_prefers_cross_axis_overlap_neighbor_not_diagonal() {
    // [P1][P2]
    // [P3][P4]
    let mut tree = PaneTree::new(P1);
    tree.split(P1, P2, Orientation::Vertical);
    tree.split(P1, P3, Orientation::Horizontal);
    tree.split(P2, P4, Orientation::Horizontal);
    assert_eq!(
        target(P1, Side::Right, &tree, W, H),
        Some(Target::Existing(P2)),
        "right from top-left must be TOP-right (strict cross-axis overlap). Existing(4) is \
         the diagonal; Existing(3) means the side asked for the pane below"
    );
}

#[test]
fn non_leaf_active_returns_none() {
    let tree = two_side_by_side();
    assert_eq!(
        target(UNKNOWN_PANE, Side::Right, &tree, W, H),
        None,
        "an active id that is no leaf of the tree (unknown pane / drawer focus) must resolve \
         to None, a no-op. Some(Split) is a stray split; Some(Existing(_)) binds a pane the \
         user never chose"
    );
}
