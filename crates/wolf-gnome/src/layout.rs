//! Pure, GTK-free pane-layout tree for the `wolf-gnome` split-panes UI.
//!
//! Decision D7 (see `.claude/current-plan.md`) splits the S3 panes feature into a
//! PURE layout core (this module) and a GTK glue layer (`pane_view.rs`). This
//! module owns the binary pane-layout TREE and every geometry decision that can be
//! reasoned about without a display server, a widget, or a live Neovim:
//!
//!   - splitting a pane (replacing a leaf with an internal node of two leaves),
//!   - removing a pane (collapsing its sibling into the parent's slot),
//!   - enumerating leaves in a deterministic order,
//!   - computing each leaf's pixel rectangle within given bounds, and
//!   - finding the geometric neighbour of a pane in a [`wolf_core::FocusDir`].
//!
//! Keeping this GTK-free is exactly what lets it be unit-tested headlessly, the
//! same way `geometry.rs`, `cursor.rs`, and `tree.rs` keep their decisions pure.
//! `pane_view.rs` (the GTK glue) is the only place that turns a [`PaneTree`] into
//! nested `gtk::Paned` widgets and `DrawingArea`s.
//
// =============================================================================
// NOTE to coder: this file was bootstrapped by the test-writer with ONLY the
// tests below. It WILL NOT COMPILE until you add the items the tests reference,
// exactly as `geometry.rs`, `cursor.rs`, and `tree.rs` were bootstrapped.
// Implement the following ABOVE the `#[cfg(test)] mod tests` block (pure, NO GTK;
// do not change the test module). lib.rs must `pub mod layout;`.
//
//   pub type PaneId = u64;
//     (A newtype would also work, but the tests assume a u64 alias: leaves()
//     returns Vec<PaneId> compared against vec![1u64, 2u64, ...].)
//
//   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//   pub enum Orientation { Horizontal, Vertical }
//     - MATCHES wolf_core::SplitOrientation: Horizontal split = panes STACKED
//       top/bottom (divides HEIGHT); Vertical split = panes SIDE BY SIDE
//       left/right (divides WIDTH). (The app maps SplitOrientation -> Orientation.)
//
//   #[derive(Debug, Clone, Copy, PartialEq)]
//   pub struct Rect { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }
//     - derive Clone + Copy + Debug + PartialEq (the tests copy and compare it).
//
//   pub struct PaneTree { ... }   // a binary tree of leaves/internal nodes.
//
//   impl PaneTree {
//     pub fn new(root: PaneId) -> Self
//       - A single leaf `root`. leaves() == [root]; pane_rects covers the whole
//         bounds with that one pane.
//
//     pub fn split(&mut self, target: PaneId, new_pane: PaneId, orientation: Orientation)
//       - Replace the `target` LEAF with an internal node (carrying `orientation`,
//         ratio 0.5) whose two children are, IN ORDER:
//           child[0] = the existing `target` leaf  (stays FIRST),
//           child[1] = a new leaf `new_pane`        (goes SECOND/after).
//       - CONVENTION (pinned by tests): the NEW pane is the SECOND child, so for a
//         Vertical split the target stays on the LEFT and new_pane is on the RIGHT;
//         for a Horizontal split the target stays on TOP and new_pane is on the
//         BOTTOM. If `target` is not a leaf in the tree, leave the behaviour to your
//         discretion (the tests only split existing leaves).
//
//     pub fn remove(&mut self, pane: PaneId)
//       - Remove the leaf `pane`; its SIBLING collapses into the parent's slot
//         (the parent internal node is replaced by the sibling subtree). Removing
//         the root/last remaining pane is a NO-OP (the tree is NEVER empty).
//
//     pub fn leaves(&self) -> Vec<PaneId>
//       - All leaf pane ids in a DETERMINISTIC order: a depth-first, child[0]
//         before child[1] traversal (i.e. left-to-right / top-to-bottom). The
//         order must be stable across repeated calls.
//
//     pub fn pane_rects(&self, width: f64, height: f64) -> Vec<(PaneId, Rect)>
//       - Each leaf's rectangle within bounds (0,0,width,height), using each
//         internal node's orientation + ratio (0.5 by default):
//           * Vertical split   => divide WIDTH: child[0] gets the LEFT half,
//             child[1] the RIGHT half (x split at width*ratio).
//           * Horizontal split => divide HEIGHT: child[0] gets the TOP half,
//             child[1] the BOTTOM half (y split at height*ratio).
//       - Order matches leaves() (same DFS). Rects must TILE the bounds exactly:
//         no overlap, and the union equals the full area.
//
//     pub fn neighbor(&self, from: PaneId, dir: wolf_core::FocusDir,
//                     width: f64, height: f64) -> Option<PaneId>
//       - Using pane_rects geometry, return the adjacent pane in `dir`: the pane
//         whose rect is nearest in that direction AND OVERLAPS `from` on the cross
//         axis (Left/Right overlap on Y; Up/Down overlap on X). Return None at an
//         edge (no such pane). Overlap should be STRICT (positive overlap, not a
//         mere shared boundary) so a clean 2x2 grid has unambiguous neighbours —
//         see `neighbor_nested` below. (NOTE to coder: if you choose a tolerance
//         instead of strict positive overlap, keep the 2x2 cases unambiguous.)
//   }
//
// NOTE to coder (FocusDir source): the tests use `wolf_core::FocusDir` directly
// (Left/Right/Up/Down), since the app maps core focus-dir intents straight onto
// this method. If you decide a LOCAL FocusDir is cleaner, change the `use` in the
// test module accordingly and flag it — but the plan (D6/D7) reuses the core enum.
// =============================================================================

use wolf_core::FocusDir;

/// The default split ratio: a fresh split divides its bounds evenly. A user drag
/// of a `gtk::Paned` divider is the only thing that moves it off `0.5`, and that
/// is owned by the widget layer, not this pure tree.
const DEFAULT_RATIO: f64 = 0.5;

/// Tolerance for the geometric edge/overlap comparisons in [`PaneTree::neighbor`].
/// The arithmetic is a few multiplies, so a tight epsilon is appropriate; it makes
/// the cross-axis overlap STRICTLY positive (a shared edge does not count) so a
/// clean 2x2 grid has unambiguous neighbours.
const NEIGHBOR_EPS: f64 = 1e-9;

/// Placeholder leaf id used transiently while replacing a node in [`remove`]; it
/// is never observable (it is overwritten in the same statement).
const PLACEHOLDER_PANE: PaneId = 0;

/// A pane identifier. A plain `u64` alias (not a newtype) so the tree's
/// `leaves()`/`pane_rects()` results compare directly against literal ids.
pub type PaneId = u64;

/// The axis along which an internal node splits its bounds.
///
/// Matches [`wolf_core::SplitOrientation`]: a [`Orientation::Horizontal`] split
/// stacks its panes top/bottom (dividing HEIGHT); a [`Orientation::Vertical`]
/// split places them side by side (dividing WIDTH).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// Panes stacked top/bottom; the divider runs horizontally and splits HEIGHT.
    Horizontal,
    /// Panes side by side; the divider runs vertically and splits WIDTH.
    Vertical,
}

/// An axis-aligned pixel rectangle within a pane-area's bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge, in device pixels.
    pub x: f64,
    /// Top edge, in device pixels.
    pub y: f64,
    /// Width, in device pixels.
    pub w: f64,
    /// Height, in device pixels.
    pub h: f64,
}

/// One node of the binary pane-layout tree: either a leaf (a single pane) or an
/// internal split of two child subtrees along an [`Orientation`] at a ratio.
enum Node {
    /// A single pane, identified by its [`PaneId`].
    Leaf(PaneId),
    /// An internal split: two children divided along `orientation` at `ratio`.
    Split {
        /// The split axis.
        orientation: Orientation,
        /// The fraction (0.0–1.0) of the bounds given to `child0`.
        ratio: f64,
        /// The first child (LEFT for vertical, TOP for horizontal).
        child0: Box<Node>,
        /// The second child (RIGHT for vertical, BOTTOM for horizontal). A new
        /// pane from [`PaneTree::split`] always becomes this child.
        child1: Box<Node>,
    },
}

/// A binary tree of editor panes.
///
/// Leaves are panes; internal nodes are splits. The tree is NEVER empty (it always
/// has at least its root leaf). See the module docs and the test-contract NOTE
/// above for the pinned conventions (new pane = second child; Vertical divides
/// width; DFS child0-before-child1 ordering).
pub struct PaneTree {
    root: Node,
}

impl PaneTree {
    /// Creates a tree of a single leaf pane `root`.
    #[must_use]
    pub fn new(root: PaneId) -> Self {
        Self {
            root: Node::Leaf(root),
        }
    }

    /// Splits the `target` leaf, inserting `new_pane` as the SECOND child of a new
    /// internal node carrying `orientation` (at [`DEFAULT_RATIO`]). The `target`
    /// stays first (LEFT for vertical, TOP for horizontal). No-op if `target` is
    /// not a leaf in the tree.
    pub fn split(&mut self, target: PaneId, new_pane: PaneId, orientation: Orientation) {
        split_node(&mut self.root, target, new_pane, orientation);
    }

    /// Removes the leaf `pane`, collapsing its sibling subtree into the parent's
    /// slot. Removing the root/last remaining pane is a no-op (the tree is never
    /// empty).
    pub fn remove(&mut self, pane: PaneId) {
        remove_node(&mut self.root, pane);
    }

    /// Returns every leaf pane id in a deterministic DFS order (child0 before
    /// child1, i.e. left-to-right / top-to-bottom).
    #[must_use]
    pub fn leaves(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        collect_leaves(&self.root, &mut out);
        out
    }

    /// Returns each leaf's pixel rectangle within bounds `(0, 0, width, height)`,
    /// in the same DFS order as [`PaneTree::leaves`]. The rects tile the bounds
    /// exactly (no overlap; union equals the full area).
    #[must_use]
    pub fn pane_rects(&self, width: f64, height: f64) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            w: width,
            h: height,
        };
        collect_rects(&self.root, bounds, &mut out);
        out
    }

    /// Returns the geometric neighbour of `from` in `dir`: the nearest pane in that
    /// direction whose rect STRICTLY overlaps `from` on the cross axis (Left/Right
    /// overlap on Y; Up/Down overlap on X). Returns `None` at an edge or for an
    /// unknown pane.
    #[must_use]
    pub fn neighbor(
        &self,
        from: PaneId,
        dir: FocusDir,
        width: f64,
        height: f64,
    ) -> Option<PaneId> {
        let rects = self.pane_rects(width, height);
        let from_rect = rects.iter().find(|(id, _)| *id == from).map(|(_, r)| *r)?;

        let mut best: Option<(PaneId, f64)> = None;
        for (id, r) in &rects {
            if *id == from {
                continue;
            }
            let Some(distance) = directional_distance(from_rect, *r, dir) else {
                continue;
            };
            if best.is_none_or(|(_, d)| distance < d) {
                best = Some((*id, distance));
            }
        }
        best.map(|(id, _)| id)
    }

    /// Returns a GTK-free, read-only description of the tree's structure for the
    /// widget layer to build nested `gtk::Paned`s from (the internal `Node` stays
    /// private to this module).
    #[must_use]
    pub fn layout(&self) -> PaneLayout {
        node_layout(&self.root)
    }
}

/// A read-only mirror of the [`PaneTree`] structure, produced by
/// [`PaneTree::layout`]. The widget layer (`pane_view.rs`) walks this to build
/// nested `gtk::Paned`s, reusing each leaf's `DrawingArea` by [`PaneId`].
#[derive(Debug, Clone, PartialEq)]
pub enum PaneLayout {
    /// A single pane.
    Leaf(PaneId),
    /// A split of two child layouts along `orientation` at `ratio`.
    Split {
        /// The split axis.
        orientation: Orientation,
        /// The fraction (0.0–1.0) of the bounds given to `first`.
        ratio: f64,
        /// The first child (LEFT for vertical, TOP for horizontal).
        first: Box<PaneLayout>,
        /// The second child (RIGHT for vertical, BOTTOM for horizontal).
        second: Box<PaneLayout>,
    },
}

/// Recursively mirrors `node` into a [`PaneLayout`].
fn node_layout(node: &Node) -> PaneLayout {
    match node {
        Node::Leaf(id) => PaneLayout::Leaf(*id),
        Node::Split {
            orientation,
            ratio,
            child0,
            child1,
        } => PaneLayout::Split {
            orientation: *orientation,
            ratio: *ratio,
            first: Box::new(node_layout(child0)),
            second: Box::new(node_layout(child1)),
        },
    }
}

/// Recursively splits the leaf matching `target` within `node`. Returns `true` if a
/// matching leaf was found and split.
fn split_node(node: &mut Node, target: PaneId, new_pane: PaneId, orientation: Orientation) -> bool {
    match node {
        Node::Leaf(id) if *id == target => {
            let existing = Node::Leaf(*id);
            *node = Node::Split {
                orientation,
                ratio: DEFAULT_RATIO,
                child0: Box::new(existing),
                child1: Box::new(Node::Leaf(new_pane)),
            };
            true
        }
        Node::Leaf(_) => false,
        Node::Split { child0, child1, .. } => {
            split_node(child0, target, new_pane, orientation)
                || split_node(child1, target, new_pane, orientation)
        }
    }
}

/// Recursively removes the leaf `pane`, collapsing the sibling into the parent
/// slot. Returns `true` if `pane` was found and removed.
fn remove_node(node: &mut Node, pane: PaneId) -> bool {
    // Index of the child to KEEP when a DIRECT child of this split is `pane`:
    // if child0 is the target, keep child1 (index 1), and vice versa.
    let keep: Option<usize> = match node {
        Node::Split { child0, child1, .. } => {
            if matches!(&**child0, Node::Leaf(id) if *id == pane) {
                Some(1)
            } else if matches!(&**child1, Node::Leaf(id) if *id == pane) {
                Some(0)
            } else {
                None
            }
        }
        Node::Leaf(_) => None,
    };

    if let Some(keep) = keep {
        let old = std::mem::replace(node, Node::Leaf(PLACEHOLDER_PANE));
        if let Node::Split { child0, child1, .. } = old {
            *node = if keep == 0 { *child0 } else { *child1 };
        }
        return true;
    }

    match node {
        Node::Split { child0, child1, .. } => {
            remove_node(child0, pane) || remove_node(child1, pane)
        }
        Node::Leaf(_) => false,
    }
}

/// Appends every leaf id under `node` to `out` in DFS (child0-first) order.
fn collect_leaves(node: &Node, out: &mut Vec<PaneId>) {
    match node {
        Node::Leaf(id) => out.push(*id),
        Node::Split { child0, child1, .. } => {
            collect_leaves(child0, out);
            collect_leaves(child1, out);
        }
    }
}

/// Appends each leaf's `(id, rect)` under `node` (occupying `rect`) to `out`,
/// dividing `rect` per each split's orientation + ratio, in DFS order.
fn collect_rects(node: &Node, rect: Rect, out: &mut Vec<(PaneId, Rect)>) {
    match node {
        Node::Leaf(id) => out.push((*id, rect)),
        Node::Split {
            orientation,
            ratio,
            child0,
            child1,
        } => match orientation {
            Orientation::Vertical => {
                let split = rect.w * ratio;
                let r0 = Rect {
                    w: split,
                    ..rect
                };
                let r1 = Rect {
                    x: rect.x + split,
                    w: rect.w - split,
                    ..rect
                };
                collect_rects(child0, r0, out);
                collect_rects(child1, r1, out);
            }
            Orientation::Horizontal => {
                let split = rect.h * ratio;
                let r0 = Rect {
                    h: split,
                    ..rect
                };
                let r1 = Rect {
                    y: rect.y + split,
                    h: rect.h - split,
                    ..rect
                };
                collect_rects(child0, r0, out);
                collect_rects(child1, r1, out);
            }
        },
    }
}

/// Returns the non-negative gap distance from `from` to candidate `cand` in `dir`
/// if `cand` lies in that direction AND strictly overlaps `from` on the cross axis;
/// otherwise `None`. A smaller distance means a nearer neighbour.
fn directional_distance(from: Rect, cand: Rect, dir: FocusDir) -> Option<f64> {
    match dir {
        FocusDir::Left => {
            (cand.x + cand.w <= from.x + NEIGHBOR_EPS && y_overlap(from, cand))
                .then_some(from.x - (cand.x + cand.w))
        }
        FocusDir::Right => {
            (cand.x + NEIGHBOR_EPS >= from.x + from.w && y_overlap(from, cand))
                .then_some(cand.x - (from.x + from.w))
        }
        FocusDir::Up => {
            (cand.y + cand.h <= from.y + NEIGHBOR_EPS && x_overlap(from, cand))
                .then_some(from.y - (cand.y + cand.h))
        }
        FocusDir::Down => {
            (cand.y + NEIGHBOR_EPS >= from.y + from.h && x_overlap(from, cand))
                .then_some(cand.y - (from.y + from.h))
        }
    }
}

/// Whether `a` and `b` overlap with strictly positive extent on the Y axis.
fn y_overlap(a: Rect, b: Rect) -> bool {
    a.y.max(b.y) < (a.y + a.h).min(b.y + b.h) - NEIGHBOR_EPS
}

/// Whether `a` and `b` overlap with strictly positive extent on the X axis.
fn x_overlap(a: Rect, b: Rect) -> bool {
    a.x.max(b.x) < (a.x + a.w).min(b.x + b.w) - NEIGHBOR_EPS
}

#[cfg(test)]
mod tests {
    use super::*;

    use wolf_core::FocusDir;

    // --- Pane ids used across the tests (small, distinct; < 5 so no mnd) -----
    const P1: PaneId = 1;
    const P2: PaneId = 2;
    const P3: PaneId = 3;
    const P4: PaneId = 4;

    // --- Bounds used across the geometry tests ------------------------------
    //
    // Non-square (W != H) so width math and height math are distinguishable.
    /// Test bounds width (px).
    const W: f64 = 100.0;
    /// Test bounds height (px).
    const H: f64 = 40.0;
    /// Half the width — a 0.5 Vertical split boundary.
    const HALF_W: f64 = 50.0;
    /// Half the height — a 0.5 Horizontal split boundary.
    const HALF_H: f64 = 20.0;

    // --- Float comparison ---------------------------------------------------

    /// Tolerance for comparing the `f64` rect components. The arithmetic is a few
    /// multiplies, so a tight epsilon is appropriate.
    const EPS: f64 = 1e-9;

    /// Returns whether two floats are within [`EPS`].
    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    /// Looks up the [`Rect`] for `id` in a `pane_rects` result, panicking with a
    /// readable message if the pane is missing.
    fn rect_for(rects: &[(PaneId, Rect)], id: PaneId) -> Rect {
        rects
            .iter()
            .find(|(pid, _)| *pid == id)
            .unwrap_or_else(|| panic!("pane {id} not found in rects {rects:?}"))
            .1
    }

    /// Asserts a [`Rect`] matches the expected `(x, y, w, h)` within [`EPS`].
    fn assert_rect(got: Rect, x: f64, y: f64, w: f64, h: f64, desc: &str) {
        assert!(
            approx(got.x, x) && approx(got.y, y) && approx(got.w, w) && approx(got.h, h),
            "case {desc}: got {got:?}, want (x:{x}, y:{y}, w:{w}, h:{h})"
        );
    }

    /// Whether two rects overlap with strictly positive area (a shared edge does
    /// NOT count as overlap).
    fn rects_overlap(a: Rect, b: Rect) -> bool {
        let x_overlap = (a.x.max(b.x)) < (a.x + a.w).min(b.x + b.w) - EPS;
        let y_overlap = (a.y.max(b.y)) < (a.y + a.h).min(b.y + b.h) - EPS;
        x_overlap && y_overlap
    }

    // =====================================================================
    // construction
    // =====================================================================

    #[test]
    fn new_is_single_leaf() {
        let tree = PaneTree::new(P1);
        assert_eq!(tree.leaves(), vec![P1], "a fresh tree has exactly its root leaf");

        // The single pane fills the whole bounds.
        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 1, "one leaf -> one rect");
        assert_rect(rect_for(&rects, P1), 0.0, 0.0, W, H, "root fills bounds");
    }

    // =====================================================================
    // split
    // =====================================================================

    #[test]
    fn split_vertical_two_leaves() {
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);

        let leaves = tree.leaves();
        assert_eq!(leaves.len(), 2, "a single split yields two leaves");
        assert!(leaves.contains(&P1), "the split target survives as a leaf");
        assert!(leaves.contains(&P2), "the new pane is a leaf");
        // CONVENTION: new pane is the SECOND child, so DFS order is [target, new].
        assert_eq!(leaves, vec![P1, P2], "new pane goes second (DFS order)");
    }

    #[test]
    fn split_then_pane_rects_side_by_side() {
        // Vertical split divides WIDTH: target (P1) stays LEFT, new (P2) on RIGHT.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);

        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 2, "two leaves -> two rects");
        assert_rect(rect_for(&rects, P1), 0.0, 0.0, HALF_W, H, "P1 left half");
        assert_rect(rect_for(&rects, P2), HALF_W, 0.0, HALF_W, H, "P2 right half");
    }

    #[test]
    fn split_horizontal_stacked() {
        // Horizontal split divides HEIGHT: target (P1) stays TOP, new (P2) BOTTOM.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Horizontal);

        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 2, "two leaves -> two rects");
        assert_rect(rect_for(&rects, P1), 0.0, 0.0, W, HALF_H, "P1 top half");
        assert_rect(rect_for(&rects, P2), 0.0, HALF_H, W, HALF_H, "P2 bottom half");
    }

    #[test]
    fn nested_split_rects() {
        // Vertical split (P1 left, P2 right), then split the LEFT child (P1)
        // horizontally into P1 (top) / P3 (bottom). The three rects must tile the
        // bounds with no overlap and a union equal to the full area.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Horizontal);

        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 3, "three leaves -> three rects");

        let r1 = rect_for(&rects, P1);
        let r2 = rect_for(&rects, P2);
        let r3 = rect_for(&rects, P3);

        // Exact tiling of the 100x40 area.
        assert_rect(r1, 0.0, 0.0, HALF_W, HALF_H, "P1 top-left");
        assert_rect(r3, 0.0, HALF_H, HALF_W, HALF_H, "P3 bottom-left");
        assert_rect(r2, HALF_W, 0.0, HALF_W, H, "P2 full-height right");

        // No pairwise overlap.
        assert!(!rects_overlap(r1, r2), "P1/P2 must not overlap");
        assert!(!rects_overlap(r1, r3), "P1/P3 must not overlap");
        assert!(!rects_overlap(r2, r3), "P2/P3 must not overlap");

        // Areas sum to the full bounds.
        let total: f64 = rects.iter().map(|(_, r)| r.w * r.h).sum();
        assert!(approx(total, W * H), "leaf areas must sum to the full bounds");
    }

    // =====================================================================
    // remove / collapse
    // =====================================================================

    #[test]
    fn remove_collapses_sibling() {
        // Simple case: split then remove the new pane -> back to a single leaf
        // covering the whole bounds.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.remove(P2);

        assert_eq!(tree.leaves(), vec![P1], "removing the sibling collapses to P1");
        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 1, "one leaf remains");
        assert_rect(rect_for(&rects, P1), 0.0, 0.0, W, H, "P1 reclaims the full bounds");
    }

    #[test]
    fn remove_collapses_sibling_subtree() {
        // Nested case: P1 on the left, and a horizontally-split (P2 top / P3
        // bottom) subtree on the right. Removing P1 collapses its sibling SUBTREE
        // up into the root slot, which then fills the whole bounds.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P2, P3, Orientation::Horizontal);
        tree.remove(P1);

        let leaves = tree.leaves();
        assert_eq!(leaves.len(), 2, "P1 gone, the right subtree survives");
        assert!(leaves.contains(&P2) && leaves.contains(&P3), "P2 and P3 remain");

        // The surviving subtree now occupies the FULL bounds (its parent's slot):
        // P2 top half, P3 bottom half across the entire width.
        let rects = tree.pane_rects(W, H);
        assert_rect(rect_for(&rects, P2), 0.0, 0.0, W, HALF_H, "P2 spans full width, top");
        assert_rect(rect_for(&rects, P3), 0.0, HALF_H, W, HALF_H, "P3 spans full width, bottom");
    }

    #[test]
    fn remove_last_is_noop() {
        // The tree is never empty: removing the only pane is a no-op.
        let mut tree = PaneTree::new(P1);
        tree.remove(P1);
        assert_eq!(tree.leaves(), vec![P1], "removing the last pane is a no-op");
    }

    // =====================================================================
    // neighbor
    // =====================================================================

    #[test]
    fn neighbor_basic_vertical() {
        // Side-by-side: P1 left, P2 right.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);

        assert_eq!(tree.neighbor(P1, FocusDir::Right, W, H), Some(P2), "left->right is P2");
        assert_eq!(tree.neighbor(P2, FocusDir::Left, W, H), Some(P1), "right->left is P1");
        assert_eq!(tree.neighbor(P1, FocusDir::Left, W, H), None, "P1 is at the left edge");
        assert_eq!(tree.neighbor(P2, FocusDir::Right, W, H), None, "P2 is at the right edge");
        // No vertical neighbours in a purely side-by-side layout.
        assert_eq!(tree.neighbor(P1, FocusDir::Up, W, H), None, "no pane above P1");
        assert_eq!(tree.neighbor(P1, FocusDir::Down, W, H), None, "no pane below P1");
    }

    #[test]
    fn neighbor_basic_horizontal() {
        // Stacked: P1 top, P2 bottom.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Horizontal);

        assert_eq!(tree.neighbor(P1, FocusDir::Down, W, H), Some(P2), "top->down is P2");
        assert_eq!(tree.neighbor(P2, FocusDir::Up, W, H), Some(P1), "bottom->up is P1");
        assert_eq!(tree.neighbor(P1, FocusDir::Up, W, H), None, "P1 is at the top edge");
        assert_eq!(tree.neighbor(P2, FocusDir::Down, W, H), None, "P2 is at the bottom edge");
        // No horizontal neighbours in a purely stacked layout.
        assert_eq!(tree.neighbor(P1, FocusDir::Left, W, H), None, "no pane left of P1");
        assert_eq!(tree.neighbor(P1, FocusDir::Right, W, H), None, "no pane right of P1");
    }

    #[test]
    fn neighbor_nested() {
        // Build a clean 2x2 grid:
        //   split V (P1 left, P2 right), then split EACH side horizontally:
        //     split P1 -> P1 (top-left)    / P3 (bottom-left)
        //     split P2 -> P2 (top-right)   / P4 (bottom-right)
        // Resulting rects in 100x40:
        //   P1 (0, 0,50,20)   P2 (50, 0,50,20)
        //   P3 (0,20,50,20)   P4 (50,20,50,20)
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Horizontal);
        tree.split(P2, P4, Orientation::Horizontal);

        // Sanity-check the geometry the neighbour cases rely on.
        let rects = tree.pane_rects(W, H);
        assert_rect(rect_for(&rects, P1), 0.0, 0.0, HALF_W, HALF_H, "P1 top-left");
        assert_rect(rect_for(&rects, P2), HALF_W, 0.0, HALF_W, HALF_H, "P2 top-right");
        assert_rect(rect_for(&rects, P3), 0.0, HALF_H, HALF_W, HALF_H, "P3 bottom-left");
        assert_rect(rect_for(&rects, P4), HALF_W, HALF_H, HALF_W, HALF_H, "P4 bottom-right");

        // Unambiguous neighbours (each shares a full edge with exactly one pane
        // per direction).
        assert_eq!(tree.neighbor(P1, FocusDir::Right, W, H), Some(P2), "TL right -> TR");
        assert_eq!(tree.neighbor(P1, FocusDir::Down, W, H), Some(P3), "TL down -> BL");
        assert_eq!(tree.neighbor(P4, FocusDir::Left, W, H), Some(P3), "BR left -> BL");
        assert_eq!(tree.neighbor(P4, FocusDir::Up, W, H), Some(P2), "BR up -> TR");
        assert_eq!(tree.neighbor(P2, FocusDir::Left, W, H), Some(P1), "TR left -> TL");
        assert_eq!(tree.neighbor(P3, FocusDir::Right, W, H), Some(P4), "BL right -> BR");

        // Edges return None.
        assert_eq!(tree.neighbor(P1, FocusDir::Up, W, H), None, "TL has no pane above");
        assert_eq!(tree.neighbor(P1, FocusDir::Left, W, H), None, "TL has no pane left");
        assert_eq!(tree.neighbor(P4, FocusDir::Right, W, H), None, "BR has no pane right");
        assert_eq!(tree.neighbor(P4, FocusDir::Down, W, H), None, "BR has no pane below");
    }

    #[test]
    fn neighbor_unknown_pane_is_none() {
        // A pane id not in the tree has no neighbour in any direction.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        let absent: PaneId = 99;
        assert_eq!(tree.neighbor(absent, FocusDir::Left, W, H), None, "unknown pane -> None");
        assert_eq!(tree.neighbor(absent, FocusDir::Right, W, H), None, "unknown pane -> None");
    }

    // =====================================================================
    // determinism
    // =====================================================================

    #[test]
    fn leaves_and_rects_order_is_stable() {
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Horizontal);
        tree.split(P2, P4, Orientation::Horizontal);

        // Repeated calls must yield identical ordering.
        assert_eq!(tree.leaves(), tree.leaves(), "leaves() order is stable");

        let first: Vec<PaneId> = tree.pane_rects(W, H).iter().map(|(id, _)| *id).collect();
        let second: Vec<PaneId> = tree.pane_rects(W, H).iter().map(|(id, _)| *id).collect();
        assert_eq!(first, second, "pane_rects() order is stable");

        // pane_rects order matches leaves order (same DFS).
        assert_eq!(first, tree.leaves(), "pane_rects order matches leaves order");
    }
}
