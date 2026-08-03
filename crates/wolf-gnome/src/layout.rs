//! Pure, GTK-free pane-layout tree for the `wolf-gnome` split-panes UI.
//!
//! Decision D7 (see `.claude/current-plan.md`) splits the S3 panes feature into a
//! PURE layout core (this module) and a GTK glue layer (`pane_view.rs`). This
//! module owns the pane-layout TREE and every geometry decision that can be
//! reasoned about without a display server, a widget, or a live Neovim.
//!
//! ## N-ary frame model (matches Neovim's window layout)
//!
//! A [`Frame`] is either a leaf (one pane) or an n-ary split: an [`Orientation`]
//! (a ROW of side-by-side children for [`Orientation::Vertical`], a COLUMN of
//! stacked children for [`Orientation::Horizontal`]) plus a `Vec` of child frames
//! and a parallel `Vec` of per-child `weights` (summing to 1) that drive the
//! divider positions. The invariants are: a split has `>= 2` children;
//! `weights.len() == children.len()`; and no split sits directly under a
//! same-orientation split (such nestings are FLATTENED), so each axis is a single
//! flat row/column.
//!
//! The operations this module owns:
//!
//!   - splitting a pane — a split whose orientation MATCHES the containing frame
//!     inserts a FLAT sibling (after the target) and re-equalizes the row/column;
//!     a PERPENDICULAR split wraps the target leaf in a new two-child sub-frame,
//!   - removing a pane — drops its slot, then NORMALIZES (flatten same-orientation
//!     children, collapse a 1-child split into its child, re-equalize),
//!   - enumerating leaves in a deterministic order,
//!   - computing each leaf's pixel rectangle within given bounds (cumulative
//!     normalized weight fractions), and
//!   - finding the geometric neighbour of a pane in a [`wolf_core::FocusDir`].
//!
//! Keeping this GTK-free is exactly what lets it be unit-tested headlessly, the
//! same way `geometry.rs`, `cursor.rs`, and `tree.rs` keep their decisions pure.
//! `pane_view.rs` (the GTK glue) is the only place that turns a [`PaneTree`] into
//! nested `gtk::Paned` widgets and `DrawingArea`s.

use wolf_core::FocusDir;

/// The default split weight for a fresh two-child split: each child gets half the
/// bounds. A user drag of a `gtk::Paned` divider is owned by the widget layer (and
/// not written back to these weights in v1), not this pure tree.
const DEFAULT_RATIO: f64 = 0.5;

/// Tolerance for the geometric edge/overlap comparisons in [`PaneTree::neighbor`].
/// The arithmetic is a few multiplies, so a tight epsilon is appropriate; it makes
/// the cross-axis overlap STRICTLY positive (a shared edge does not count) so a
/// clean 2x2 grid has unambiguous neighbours.
const NEIGHBOR_EPS: f64 = 1e-9;

/// The fewest children a [`Frame::Split`] may have. Enforced by
/// [`PaneTree::from_layout`], which is the only constructor that takes a split's
/// shape from OUTSIDE this module (a hand-edited `state.json`).
const MIN_SPLIT_CHILDREN: usize = 2;

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

/// One node of the n-ary pane-layout tree: either a leaf (a single pane) or a
/// split of two-or-more child frames along an [`Orientation`], with a parallel
/// per-child `weights` vector (summing to 1) driving the divider positions.
///
/// Invariants: a `Split` has `>= 2` children; `weights.len() == children.len()`;
/// the weights sum to 1; and no `Split` is a direct child of a same-orientation
/// `Split` (they are flattened into one row/column).
///
/// [`PaneTree::split`] / [`PaneTree::remove`] UPHOLD all four.
/// [`PaneTree::from_layout`] VALIDATES the first two (and rejects a non-finite,
/// negative or zero-summing weight vector) but PRESERVES the last two AS GIVEN:
/// it copies weights verbatim rather than re-equalizing, and it keeps a
/// same-orientation nested split rather than flattening it, because reshaping
/// would desynchronise the DFS leaf order the restore path zips nvim window
/// handles onto. Such a nesting is cosmetic — `pane_rects`, `neighbor` and the
/// widget builder all handle it, and the first `remove()` normalizes it away.
enum Frame {
    /// A single pane, identified by its [`PaneId`].
    Leaf(PaneId),
    /// An n-ary split: `children` divided along `orientation` by `weights`.
    Split {
        /// The split axis (a ROW for `Vertical`, a COLUMN for `Horizontal`).
        orientation: Orientation,
        /// The child frames, in order (LEFT→RIGHT for vertical, TOP→BOTTOM for
        /// horizontal).
        children: Vec<Frame>,
        /// The per-child fraction of the bounds (parallel to `children`, summing
        /// to 1).
        weights: Vec<f64>,
    },
}

/// An n-ary tree of editor panes (Neovim's frame model).
///
/// Leaves are panes; internal nodes are flat rows/columns of child frames. The tree
/// is NEVER empty (it always has at least its root leaf). See the module docs and
/// the test-contract NOTE above for the pinned conventions (a same-orientation
/// split inserts a flat sibling AFTER the target; Vertical divides width; DFS
/// child-order traversal).
pub struct PaneTree {
    root: Frame,
}

impl PaneTree {
    /// Creates a tree of a single leaf pane `root`.
    #[must_use]
    pub fn new(root: PaneId) -> Self {
        Self {
            root: Frame::Leaf(root),
        }
    }

    /// Splits the `target` leaf with `new_pane` along `orientation`.
    ///
    /// If the split's `orientation` MATCHES the frame that owns `target`, `new_pane`
    /// is inserted as a FLAT sibling immediately AFTER `target` and that row/column
    /// re-equalizes (so a same-orientation re-split yields equal siblings, never a
    /// growing nest). Otherwise `target` is wrapped in a new two-child sub-frame
    /// `[target, new_pane]` (NEST), with `target` first (LEFT for vertical, TOP for
    /// horizontal). The root special case (a single leaf) becomes a two-child split.
    /// No-op if `target` is not a leaf in the tree.
    pub fn split(&mut self, target: PaneId, new_pane: PaneId, orientation: Orientation) {
        if let Frame::Leaf(id) = self.root {
            if id == target {
                self.root = Frame::Split {
                    orientation,
                    children: vec![Frame::Leaf(target), Frame::Leaf(new_pane)],
                    weights: vec![DEFAULT_RATIO, DEFAULT_RATIO],
                };
            }
            return;
        }
        split_in(&mut self.root, target, new_pane, orientation);
    }

    /// Removes the leaf `pane`, then NORMALIZES the tree (flatten same-orientation
    /// children, collapse a 1-child split into its child, re-equalize). Removing the
    /// last remaining pane is a no-op (the tree is never empty).
    pub fn remove(&mut self, pane: PaneId) {
        if self.leaves().len() <= 1 {
            return;
        }
        if remove_in(&mut self.root, pane) {
            normalize(&mut self.root);
        }
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
    pub fn neighbor(&self, from: PaneId, dir: FocusDir, width: f64, height: f64) -> Option<PaneId> {
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
    /// widget layer to build nested `gtk::Paned`s from (the internal `Frame` stays
    /// private to this module).
    #[must_use]
    pub fn layout(&self) -> PaneLayout {
        frame_layout(&self.root)
    }

    /// Builds a tree from a [`PaneLayout`] mirror — the inverse of
    /// [`PaneTree::layout`] — VERBATIM: no flatten, no collapse, no re-equalize,
    /// so [`PaneTree::leaves`] enumerates in byte-for-byte the same DFS order as
    /// `layout`'s leaves. That is load-bearing: the state-restore path zips nvim
    /// window handles onto those leaves BY INDEX (see
    /// `crate::pane_layout::decode`), and any reshaping here would silently
    /// misbind them.
    ///
    /// Returns `None` when `layout` violates this type's invariants — a split
    /// with fewer than 2 children, a `weights.len() != children.len()` mismatch,
    /// a non-finite or negative weight, or a split whose weights sum to `<= 0.0`.
    /// Validating rather than repairing is deliberate for the same reason, and it
    /// is a CRASH GUARD rather than tidiness: `pane_view::build_chain` indexes
    /// `children[0]` unconditionally, so a zero-child split reaching the widget
    /// layer is a panic, and `pane_rects` divides by the weight sum.
    ///
    /// Note this constructor CAN produce a tree with a same-orientation nested
    /// split — which [`PaneTree::split`] never produces but a hand-edited
    /// `state.json` can. That is cosmetic; see the [`Frame`] invariant list.
    #[must_use]
    pub fn from_layout(layout: &PaneLayout) -> Option<Self> {
        Some(Self {
            root: frame_from_layout(layout)?,
        })
    }
}

/// Recursively validates and mirrors a [`PaneLayout`] back into a [`Frame`].
/// `None` on any invariant violation (see [`PaneTree::from_layout`]).
fn frame_from_layout(layout: &PaneLayout) -> Option<Frame> {
    match layout {
        PaneLayout::Leaf(id) => Some(Frame::Leaf(*id)),
        PaneLayout::Split {
            orientation,
            children,
            weights,
        } => {
            // A split has >= 2 children with one weight each. `build_chain`
            // indexes `children[0]`, so an empty split is a panic, not a wart.
            if children.len() < MIN_SPLIT_CHILDREN || weights.len() != children.len() {
                return None;
            }
            // A NaN/inf/negative weight would poison `pane_rects`' cumulative
            // fractions; an all-zero vector would divide by zero.
            if weights.iter().any(|w| !w.is_finite() || *w < 0.0) {
                return None;
            }
            let total: f64 = weights.iter().sum();
            if total <= 0.0 {
                return None;
            }
            let children: Option<Vec<Frame>> = children.iter().map(frame_from_layout).collect();
            Some(Frame::Split {
                orientation: *orientation,
                children: children?,
                weights: weights.clone(),
            })
        }
    }
}

/// A read-only mirror of the [`PaneTree`] structure, produced by
/// [`PaneTree::layout`]. The widget layer (`pane_view.rs`) walks this to build a
/// right-leaning chain of binary `gtk::Paned`s, reusing each leaf's `DrawingArea`
/// by [`PaneId`].
#[derive(Debug, Clone, PartialEq)]
pub enum PaneLayout {
    /// A single pane.
    Leaf(PaneId),
    /// An n-ary split of child layouts along `orientation`, with parallel per-child
    /// `weights` (summing to 1).
    Split {
        /// The split axis.
        orientation: Orientation,
        /// The child layouts, in order (LEFT→RIGHT / TOP→BOTTOM).
        children: Vec<PaneLayout>,
        /// The per-child fraction of the bounds (parallel to `children`).
        weights: Vec<f64>,
    },
}

/// Recursively mirrors `frame` into a [`PaneLayout`].
fn frame_layout(frame: &Frame) -> PaneLayout {
    match frame {
        Frame::Leaf(id) => PaneLayout::Leaf(*id),
        Frame::Split {
            orientation,
            children,
            weights,
        } => PaneLayout::Split {
            orientation: *orientation,
            children: children.iter().map(frame_layout).collect(),
            weights: weights.clone(),
        },
    }
}

/// Equalizes `weights` to `1/n` each, where `n = weights.len()`.
#[allow(
    clippy::cast_precision_loss,
    reason = "pane counts are tiny; 1/n is exact enough for divider geometry"
)]
fn equalize(weights: &mut [f64]) {
    let n = weights.len();
    if n == 0 {
        return;
    }
    let each = 1.0 / n as f64;
    for w in weights.iter_mut() {
        *w = each;
    }
}

/// Recursively splits the leaf matching `target` within a `Split` `frame`. A
/// same-orientation split inserts a flat sibling after `target` and re-equalizes; a
/// perpendicular split wraps `target` in a new two-child sub-frame. Returns `true`
/// if a matching leaf was found and split.
fn split_in(frame: &mut Frame, target: PaneId, new_pane: PaneId, orientation: Orientation) -> bool {
    let Frame::Split {
        orientation: own,
        children,
        weights,
    } = frame
    else {
        return false;
    };

    if let Some(i) = children
        .iter()
        .position(|c| matches!(c, Frame::Leaf(id) if *id == target))
    {
        if *own == orientation {
            // FLAT: insert the new pane immediately after the target, re-equalize.
            children.insert(i + 1, Frame::Leaf(new_pane));
            weights.insert(i + 1, 0.0);
            equalize(weights);
        } else {
            // NEST: wrap the target leaf in a new perpendicular two-child frame.
            children[i] = Frame::Split {
                orientation,
                children: vec![Frame::Leaf(target), Frame::Leaf(new_pane)],
                weights: vec![DEFAULT_RATIO, DEFAULT_RATIO],
            };
        }
        return true;
    }

    children
        .iter_mut()
        .any(|child| split_in(child, target, new_pane, orientation))
}

/// Recursively removes the leaf `pane` (and its weight) from the `Split` that owns
/// it. Returns `true` if `pane` was found and removed. Caller normalizes after.
fn remove_in(frame: &mut Frame, pane: PaneId) -> bool {
    let Frame::Split {
        children, weights, ..
    } = frame
    else {
        return false;
    };

    if let Some(i) = children
        .iter()
        .position(|c| matches!(c, Frame::Leaf(id) if *id == pane))
    {
        children.remove(i);
        weights.remove(i);
        return true;
    }

    children.iter_mut().any(|child| remove_in(child, pane))
}

/// Bottom-up normalization after a remove: recurse children first, then FLATTEN any
/// same-orientation child split into this one (scaling its weights), COLLAPSE a
/// single remaining child into this frame, else re-EQUALIZE the weights.
fn normalize(frame: &mut Frame) {
    let Frame::Split {
        orientation,
        children,
        weights,
    } = frame
    else {
        return;
    };

    for child in children.iter_mut() {
        normalize(child);
    }

    // FLATTEN: splice any same-orientation child's children + scaled weights in.
    let mut new_children: Vec<Frame> = Vec::with_capacity(children.len());
    let mut new_weights: Vec<f64> = Vec::with_capacity(weights.len());
    for (child, w) in children.drain(..).zip(weights.drain(..)) {
        match child {
            Frame::Split {
                orientation: child_orient,
                children: grandchildren,
                weights: grand_weights,
            } if child_orient == *orientation => {
                for (gc, gw) in grandchildren.into_iter().zip(grand_weights) {
                    new_children.push(gc);
                    new_weights.push(gw * w);
                }
            }
            other => {
                new_children.push(other);
                new_weights.push(w);
            }
        }
    }
    *children = new_children;
    *weights = new_weights;

    // COLLAPSE a 1-child split into its sole child; else re-equalize the row/column.
    if children.len() == 1 {
        let only = children.drain(..).next().expect("len checked == 1");
        *frame = only;
    } else {
        equalize(weights);
    }
}

/// Appends every leaf id under `frame` to `out` in DFS (child-order) order.
fn collect_leaves(frame: &Frame, out: &mut Vec<PaneId>) {
    match frame {
        Frame::Leaf(id) => out.push(*id),
        Frame::Split { children, .. } => {
            for child in children {
                collect_leaves(child, out);
            }
        }
    }
}

/// Appends each leaf's `(id, rect)` under `frame` (occupying `rect`) to `out`,
/// dividing `rect` among the children by cumulative normalized weight fractions
/// (Vertical divides WIDTH; Horizontal divides HEIGHT), in DFS order.
fn collect_rects(frame: &Frame, rect: Rect, out: &mut Vec<(PaneId, Rect)>) {
    match frame {
        Frame::Leaf(id) => out.push((*id, rect)),
        Frame::Split {
            orientation,
            children,
            weights,
        } => {
            let total: f64 = weights.iter().sum();
            let mut offset = 0.0;
            for (child, w) in children.iter().zip(weights.iter()) {
                let frac = if total > 0.0 { w / total } else { 0.0 };
                let child_rect = match orientation {
                    Orientation::Vertical => {
                        let cw = rect.w * frac;
                        let r = Rect {
                            x: rect.x + offset,
                            w: cw,
                            ..rect
                        };
                        offset += cw;
                        r
                    }
                    Orientation::Horizontal => {
                        let ch = rect.h * frac;
                        let r = Rect {
                            y: rect.y + offset,
                            h: ch,
                            ..rect
                        };
                        offset += ch;
                        r
                    }
                };
                collect_rects(child, child_rect, out);
            }
        }
    }
}

/// Returns the non-negative gap distance from `from` to candidate `cand` in `dir`
/// if `cand` lies in that direction AND strictly overlaps `from` on the cross axis;
/// otherwise `None`. A smaller distance means a nearer neighbour.
fn directional_distance(from: Rect, cand: Rect, dir: FocusDir) -> Option<f64> {
    match dir {
        FocusDir::Left => (cand.x + cand.w <= from.x + NEIGHBOR_EPS && y_overlap(from, cand))
            .then_some(from.x - (cand.x + cand.w)),
        FocusDir::Right => (cand.x + NEIGHBOR_EPS >= from.x + from.w && y_overlap(from, cand))
            .then_some(cand.x - (from.x + from.w)),
        FocusDir::Up => (cand.y + cand.h <= from.y + NEIGHBOR_EPS && x_overlap(from, cand))
            .then_some(from.y - (cand.y + cand.h)),
        FocusDir::Down => (cand.y + NEIGHBOR_EPS >= from.y + from.h && x_overlap(from, cand))
            .then_some(cand.y - (from.y + from.h)),
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
    /// One third of the test width — a flat 3-way Vertical split column width
    /// (and the x of the second column).
    const THIRD_W: f64 = W / 3.0;
    /// Two thirds of the test width — the x of the third column in a flat 3-way
    /// Vertical split.
    const TWO_THIRD_W: f64 = 2.0 * W / 3.0;
    /// One quarter of the test width — a flat 4-way Vertical split column width.
    const QUARTER_W: f64 = W / 4.0;

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
        assert_eq!(
            tree.leaves(),
            vec![P1],
            "a fresh tree has exactly its root leaf"
        );

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
        assert_rect(
            rect_for(&rects, P2),
            HALF_W,
            0.0,
            HALF_W,
            H,
            "P2 right half",
        );
    }

    #[test]
    fn split_horizontal_stacked() {
        // Horizontal split divides HEIGHT: target (P1) stays TOP, new (P2) BOTTOM.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Horizontal);

        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 2, "two leaves -> two rects");
        assert_rect(rect_for(&rects, P1), 0.0, 0.0, W, HALF_H, "P1 top half");
        assert_rect(
            rect_for(&rects, P2),
            0.0,
            HALF_H,
            W,
            HALF_H,
            "P2 bottom half",
        );
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
        assert!(
            approx(total, W * H),
            "leaf areas must sum to the full bounds"
        );
    }

    // =====================================================================
    // n-ary FLAT (same-orientation) split behavior
    //
    // These pin the n-ary frame model (architect's N-ARY plan): a split whose
    // orientation MATCHES the containing frame FLATTENS into a flat, equalized
    // sibling row/column (no nesting); a PERPENDICULAR split still NESTS (as the
    // existing perpendicular tests above already assert). The flat cases are THE
    // 3-pane regression: the old binary always-nest model produced nested
    // 25/25/50 columns, blanking the pre-existing right pane.
    // =====================================================================

    #[test]
    fn vsplit_flattens_into_row() {
        // Start with a row [P1, P2] (Vertical = side by side), then Vertical-split
        // P1 AGAIN. Because the new split's orientation MATCHES the row, P3 is
        // inserted as a FLAT sibling AFTER the target P1 (architect's "insert after
        // target" rule) and the row re-equalizes — NOT a nested sub-split.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Vertical);

        // Insert-after-target pins the DFS order to [P1, P3, P2].
        assert_eq!(
            tree.leaves(),
            vec![P1, P3, P2],
            "same-orientation vsplit inserts the new pane AFTER the target (flat row)"
        );

        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 3, "three flat leaves -> three rects");

        let r1 = rect_for(&rects, P1);
        let r3 = rect_for(&rects, P3);
        let r2 = rect_for(&rects, P2);

        // Three equal full-height thirds tiling [0, W] left to right: P1, P3, P2.
        // THIS is the regression assertion: the old nested model gave 25/25/50.
        assert_rect(r1, 0.0, 0.0, THIRD_W, H, "P1 left third");
        assert_rect(r3, THIRD_W, 0.0, THIRD_W, H, "P3 middle third");
        assert_rect(r2, TWO_THIRD_W, 0.0, THIRD_W, H, "P2 right third");

        // No rect is zero-width (the pre-existing right pane is NOT starved).
        for (id, r) in &rects {
            assert!(r.w > EPS, "pane {id} must not be zero-width: {r:?}");
        }

        // No pairwise overlap.
        assert!(!rects_overlap(r1, r3), "P1/P3 must not overlap");
        assert!(!rects_overlap(r3, r2), "P3/P2 must not overlap");
        assert!(!rects_overlap(r1, r2), "P1/P2 must not overlap");

        // Areas tile the full bounds.
        let total: f64 = rects.iter().map(|(_, r)| r.w * r.h).sum();
        assert!(
            approx(total, W * H),
            "leaf areas must sum to the full bounds"
        );
    }

    #[test]
    fn vsplit_twice_stays_flat_four_equal() {
        // Repeated same-orientation vsplits of P1 keep the row FLAT (no growing
        // nesting): four equal-width full-height columns.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Vertical);
        tree.split(P1, P4, Orientation::Vertical);

        let leaves = tree.leaves();
        assert_eq!(
            leaves.len(),
            4,
            "four flat leaves after three same-orientation vsplits"
        );

        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 4, "four flat leaves -> four rects");

        // Each column is an equal quarter of the width, full height; none zero.
        for (id, r) in &rects {
            assert!(
                approx(r.w, QUARTER_W),
                "pane {id} must be an equal quarter-width: {r:?}"
            );
            assert!(approx(r.h, H), "pane {id} must be full-height: {r:?}");
            assert!(r.w > EPS, "pane {id} must not be zero-width: {r:?}");
        }

        // Areas tile the full bounds.
        let total: f64 = rects.iter().map(|(_, r)| r.w * r.h).sum();
        assert!(
            approx(total, W * H),
            "leaf areas must sum to the full bounds"
        );
    }

    #[test]
    fn hsplit_of_row_member_nests() {
        // PERPENDICULAR split still NESTS: row [P1, P2], then HORIZONTAL-split P1.
        // Horizontal is perpendicular to the Vertical row, so P1 is wrapped in a
        // stacked sub-frame [P1, P3] occupying the left half; P2 keeps the full-
        // height right half. (Same shape as `nested_split_rects` via a different
        // sequence — confirming nest-only-when-perpendicular.)
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Horizontal);

        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 3, "three leaves -> three rects");

        let r1 = rect_for(&rects, P1);
        let r3 = rect_for(&rects, P3);
        let r2 = rect_for(&rects, P2);

        assert_rect(r1, 0.0, 0.0, HALF_W, HALF_H, "P1 top-left (nested column)");
        assert_rect(
            r3,
            0.0,
            HALF_H,
            HALF_W,
            HALF_H,
            "P3 bottom-left (nested column)",
        );
        assert_rect(r2, HALF_W, 0.0, HALF_W, H, "P2 full-height right");

        for (id, r) in &rects {
            assert!(
                r.w > EPS && r.h > EPS,
                "pane {id} must not be zero-area: {r:?}"
            );
        }
        assert!(!rects_overlap(r1, r2), "P1/P2 must not overlap");
        assert!(!rects_overlap(r1, r3), "P1/P3 must not overlap");
        assert!(!rects_overlap(r2, r3), "P2/P3 must not overlap");

        let total: f64 = rects.iter().map(|(_, r)| r.w * r.h).sum();
        assert!(
            approx(total, W * H),
            "leaf areas must sum to the full bounds"
        );
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

        assert_eq!(
            tree.leaves(),
            vec![P1],
            "removing the sibling collapses to P1"
        );
        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 1, "one leaf remains");
        assert_rect(
            rect_for(&rects, P1),
            0.0,
            0.0,
            W,
            H,
            "P1 reclaims the full bounds",
        );
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
        assert!(
            leaves.contains(&P2) && leaves.contains(&P3),
            "P2 and P3 remain"
        );

        // The surviving subtree now occupies the FULL bounds (its parent's slot):
        // P2 top half, P3 bottom half across the entire width.
        let rects = tree.pane_rects(W, H);
        assert_rect(
            rect_for(&rects, P2),
            0.0,
            0.0,
            W,
            HALF_H,
            "P2 spans full width, top",
        );
        assert_rect(
            rect_for(&rects, P3),
            0.0,
            HALF_H,
            W,
            HALF_H,
            "P3 spans full width, bottom",
        );
    }

    #[test]
    fn remove_last_is_noop() {
        // The tree is never empty: removing the only pane is a no-op.
        let mut tree = PaneTree::new(P1);
        tree.remove(P1);
        assert_eq!(tree.leaves(), vec![P1], "removing the last pane is a no-op");
    }

    #[test]
    fn remove_re_equalizes_row() {
        // Flat row [P1, P3, P2] (see `vsplit_flattens_into_row`); removing the
        // middle pane P3 leaves a flat [P1, P2] that RE-EQUALIZES to 50/50 — NOT
        // 25/75 (nvim equalalways).
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Vertical);
        tree.remove(P3);

        assert_eq!(
            tree.leaves(),
            vec![P1, P2],
            "removing P3 leaves the flat [P1, P2] row"
        );

        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 2, "two leaves -> two rects");
        assert_rect(
            rect_for(&rects, P1),
            0.0,
            0.0,
            HALF_W,
            H,
            "P1 re-equalized left half",
        );
        assert_rect(
            rect_for(&rects, P2),
            HALF_W,
            0.0,
            HALF_W,
            H,
            "P2 re-equalized right half",
        );
    }

    #[test]
    fn remove_collapses_single_child() {
        // Row [P1, P2], then a PERPENDICULAR (Horizontal) split of P1 nests a
        // column [P1, P3] in the left half. Removing P3 leaves the column with a
        // single child P1, which COLLAPSES into the parent slot; P1 then re-
        // equalizes with P2 into 50/50 halves.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Horizontal);
        tree.remove(P3);

        assert_eq!(
            tree.leaves(),
            vec![P1, P2],
            "the 1-child column collapses, leaving the flat [P1, P2] row"
        );

        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 2, "two leaves -> two rects");
        assert_rect(
            rect_for(&rects, P1),
            0.0,
            0.0,
            HALF_W,
            H,
            "P1 reclaims+equalizes left half",
        );
        assert_rect(
            rect_for(&rects, P2),
            HALF_W,
            0.0,
            HALF_W,
            H,
            "P2 right half",
        );
    }

    #[test]
    fn remove_collapses_and_merges_same_orientation() {
        // The MERGE case (architect's stated sequence):
        //   new(P1)
        //   vsplit P1            -> row[P1, P2]
        //   hsplit P2 (perp)     -> row[P1, col[P2, P3]]
        //   vsplit P2 (perp to   -> row[P1, col[ row[P2, P4], P3 ]]
        //     the column)
        //   remove P3            -> the column drops to a single child row[P2, P4],
        //                           which COLLAPSES, and because it is the SAME
        //                           orientation as the root row it MERGES upward:
        //                           => flat row[P1, P2, P4].
        //
        // NOTE to coder: this pins the final shape — a single FLAT row [P1, P2, P4]
        // of equal thirds. The impl's normalize() must (a) collapse the 1-child
        // column AND (b) merge the surviving same-orientation row into the parent
        // (no same-orientation Split may persist nested), then re-equalize.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P2, P3, Orientation::Horizontal);
        tree.split(P2, P4, Orientation::Vertical);
        tree.remove(P3);

        assert_eq!(
            tree.leaves(),
            vec![P1, P2, P4],
            "collapse + same-orientation merge yields a flat [P1, P2, P4] row"
        );

        let rects = tree.pane_rects(W, H);
        assert_eq!(rects.len(), 3, "three flat leaves -> three rects");

        let r1 = rect_for(&rects, P1);
        let r2 = rect_for(&rects, P2);
        let r4 = rect_for(&rects, P4);
        assert_rect(r1, 0.0, 0.0, THIRD_W, H, "P1 left third");
        assert_rect(r2, THIRD_W, 0.0, THIRD_W, H, "P2 middle third");
        assert_rect(r4, TWO_THIRD_W, 0.0, THIRD_W, H, "P4 right third");

        for (id, r) in &rects {
            assert!(r.w > EPS, "pane {id} must not be zero-width: {r:?}");
        }

        let total: f64 = rects.iter().map(|(_, r)| r.w * r.h).sum();
        assert!(
            approx(total, W * H),
            "leaf areas must sum to the full bounds"
        );
    }

    // =====================================================================
    // neighbor
    // =====================================================================

    #[test]
    fn neighbor_basic_vertical() {
        // Side-by-side: P1 left, P2 right.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);

        assert_eq!(
            tree.neighbor(P1, FocusDir::Right, W, H),
            Some(P2),
            "left->right is P2"
        );
        assert_eq!(
            tree.neighbor(P2, FocusDir::Left, W, H),
            Some(P1),
            "right->left is P1"
        );
        assert_eq!(
            tree.neighbor(P1, FocusDir::Left, W, H),
            None,
            "P1 is at the left edge"
        );
        assert_eq!(
            tree.neighbor(P2, FocusDir::Right, W, H),
            None,
            "P2 is at the right edge"
        );
        // No vertical neighbours in a purely side-by-side layout.
        assert_eq!(
            tree.neighbor(P1, FocusDir::Up, W, H),
            None,
            "no pane above P1"
        );
        assert_eq!(
            tree.neighbor(P1, FocusDir::Down, W, H),
            None,
            "no pane below P1"
        );
    }

    #[test]
    fn neighbor_basic_horizontal() {
        // Stacked: P1 top, P2 bottom.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Horizontal);

        assert_eq!(
            tree.neighbor(P1, FocusDir::Down, W, H),
            Some(P2),
            "top->down is P2"
        );
        assert_eq!(
            tree.neighbor(P2, FocusDir::Up, W, H),
            Some(P1),
            "bottom->up is P1"
        );
        assert_eq!(
            tree.neighbor(P1, FocusDir::Up, W, H),
            None,
            "P1 is at the top edge"
        );
        assert_eq!(
            tree.neighbor(P2, FocusDir::Down, W, H),
            None,
            "P2 is at the bottom edge"
        );
        // No horizontal neighbours in a purely stacked layout.
        assert_eq!(
            tree.neighbor(P1, FocusDir::Left, W, H),
            None,
            "no pane left of P1"
        );
        assert_eq!(
            tree.neighbor(P1, FocusDir::Right, W, H),
            None,
            "no pane right of P1"
        );
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
        assert_rect(
            rect_for(&rects, P1),
            0.0,
            0.0,
            HALF_W,
            HALF_H,
            "P1 top-left",
        );
        assert_rect(
            rect_for(&rects, P2),
            HALF_W,
            0.0,
            HALF_W,
            HALF_H,
            "P2 top-right",
        );
        assert_rect(
            rect_for(&rects, P3),
            0.0,
            HALF_H,
            HALF_W,
            HALF_H,
            "P3 bottom-left",
        );
        assert_rect(
            rect_for(&rects, P4),
            HALF_W,
            HALF_H,
            HALF_W,
            HALF_H,
            "P4 bottom-right",
        );

        // Unambiguous neighbours (each shares a full edge with exactly one pane
        // per direction).
        assert_eq!(
            tree.neighbor(P1, FocusDir::Right, W, H),
            Some(P2),
            "TL right -> TR"
        );
        assert_eq!(
            tree.neighbor(P1, FocusDir::Down, W, H),
            Some(P3),
            "TL down -> BL"
        );
        assert_eq!(
            tree.neighbor(P4, FocusDir::Left, W, H),
            Some(P3),
            "BR left -> BL"
        );
        assert_eq!(
            tree.neighbor(P4, FocusDir::Up, W, H),
            Some(P2),
            "BR up -> TR"
        );
        assert_eq!(
            tree.neighbor(P2, FocusDir::Left, W, H),
            Some(P1),
            "TR left -> TL"
        );
        assert_eq!(
            tree.neighbor(P3, FocusDir::Right, W, H),
            Some(P4),
            "BL right -> BR"
        );

        // Edges return None.
        assert_eq!(
            tree.neighbor(P1, FocusDir::Up, W, H),
            None,
            "TL has no pane above"
        );
        assert_eq!(
            tree.neighbor(P1, FocusDir::Left, W, H),
            None,
            "TL has no pane left"
        );
        assert_eq!(
            tree.neighbor(P4, FocusDir::Right, W, H),
            None,
            "BR has no pane right"
        );
        assert_eq!(
            tree.neighbor(P4, FocusDir::Down, W, H),
            None,
            "BR has no pane below"
        );
    }

    #[test]
    fn neighbor_unknown_pane_is_none() {
        // A pane id not in the tree has no neighbour in any direction.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        let absent: PaneId = 99;
        assert_eq!(
            tree.neighbor(absent, FocusDir::Left, W, H),
            None,
            "unknown pane -> None"
        );
        assert_eq!(
            tree.neighbor(absent, FocusDir::Right, W, H),
            None,
            "unknown pane -> None"
        );
    }

    #[test]
    fn neighbor_flat_row_of_three() {
        // Flat row [P1, P3, P2] (see `vsplit_flattens_into_row`): left-to-right
        // neighbours chain across the three columns, with None at each edge.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Vertical);

        assert_eq!(
            tree.neighbor(P1, FocusDir::Right, W, H),
            Some(P3),
            "P1 right -> P3"
        );
        assert_eq!(
            tree.neighbor(P3, FocusDir::Right, W, H),
            Some(P2),
            "P3 right -> P2"
        );
        assert_eq!(
            tree.neighbor(P2, FocusDir::Right, W, H),
            None,
            "P2 is at the right edge"
        );
        assert_eq!(
            tree.neighbor(P3, FocusDir::Left, W, H),
            Some(P1),
            "P3 left -> P1"
        );
        assert_eq!(
            tree.neighbor(P1, FocusDir::Left, W, H),
            None,
            "P1 is at the left edge"
        );
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
        assert_eq!(
            first,
            tree.leaves(),
            "pane_rects order matches leaves order"
        );
    }

    // =====================================================================
    // from_layout — the inverse of `layout()`, for the state-restore path
    // =====================================================================

    /// The four canonical shapes the rest of this module already builds, each as a
    /// `(description, tree)` pair. Reused by the round-trip test so the constructor
    /// is exercised against exactly the trees `split`/`remove` really produce.
    fn canonical_trees() -> Vec<(&'static str, PaneTree)> {
        let mut two = PaneTree::new(P1);
        two.split(P1, P2, Orientation::Vertical);

        let mut row_of_three = PaneTree::new(P1);
        row_of_three.split(P1, P2, Orientation::Vertical);
        row_of_three.split(P1, P3, Orientation::Vertical);

        let mut nested = PaneTree::new(P1);
        nested.split(P1, P2, Orientation::Vertical);
        nested.split(P1, P3, Orientation::Horizontal);

        vec![
            ("a single leaf", PaneTree::new(P1)),
            ("a 2-leaf vertical split", two),
            ("a flat row of three", row_of_three),
            ("a nested perpendicular split", nested),
        ]
    }

    #[test]
    fn from_layout_round_trips_layout() {
        for (desc, tree) in canonical_trees() {
            let mirror = tree.layout();
            let Some(rebuilt) = PaneTree::from_layout(&mirror) else {
                panic!("from_layout rejected {desc}, which `split` itself produced: {mirror:?}");
            };

            assert_eq!(
                rebuilt.layout(),
                mirror,
                "from_layout must be the exact inverse of layout() for {desc} — orientation, \
                 child order and weights all verbatim"
            );
            assert_eq!(
                rebuilt.leaves(),
                tree.leaves(),
                "…and the DFS leaf ORDER must survive for {desc}: the state-restore path zips \
                 nvim window handles onto these leaves BY INDEX"
            );
            assert_eq!(
                rebuilt.pane_rects(W, H),
                tree.pane_rects(W, H),
                "…and the rebuilt tree must tile the bounds identically for {desc}, which is what \
                 proves the weights came through and not just the topology"
            );
        }
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "0.25 / 0.75 are exactly representable in binary f64 and `from_layout` copies \
                  them through unmodified rather than computing with them, so an epsilon \
                  comparison would weaken the assertion — its whole point is that the weights are \
                  EXACTLY as given, not re-equalized (the `mouse::scroll_threshold_table` \
                  precedent)"
    )]
    fn from_layout_does_not_reshape() {
        // A SAME-ORIENTATION nested split. `PaneTree::split` never produces one (it
        // inserts a flat sibling instead) and `remove`'s `normalize` flattens any it
        // finds — but a hand-edited `state.json` can absolutely contain one, and
        // `from_layout` must PRESERVE it rather than tidying it away. Flattening
        // would not change the leaf SET, but it would change the tree the widget
        // builder walks, and the whole point of this constructor is that what nvim
        // was asked to build and what GTK builds stay in lockstep.
        let nested_same_orientation = PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![
                PaneLayout::Split {
                    orientation: Orientation::Vertical,
                    children: vec![PaneLayout::Leaf(P1), PaneLayout::Leaf(P2)],
                    weights: vec![0.25, 0.75],
                },
                PaneLayout::Leaf(P3),
            ],
            weights: vec![0.5, 0.5],
        };

        let Some(rebuilt) = PaneTree::from_layout(&nested_same_orientation) else {
            panic!("from_layout rejected a same-orientation nesting, which is legal (if unusual)");
        };

        assert_eq!(
            rebuilt.layout(),
            nested_same_orientation,
            "a same-orientation nested split must survive VERBATIM: no flatten, no collapse, no \
             re-equalize. Reshaping here desynchronises the DFS leaf order from the window handles \
             `WolfCore::rebuild_layout` has already returned, silently binding each pane to \
             another pane's grid."
        );
        assert_eq!(
            rebuilt.leaves(),
            vec![P1, P2, P3],
            "the DFS leaf order is unchanged by the nesting — this is the property the restore \
             zip depends on"
        );

        // The un-equalized weights survive too: `0.25 / 0.75` is not something
        // `split()` can produce, so its presence proves nothing re-equalized.
        let PaneLayout::Split { children, .. } = rebuilt.layout() else {
            panic!("the rebuilt root must still be a split");
        };
        let PaneLayout::Split { weights, .. } = &children[0] else {
            panic!("the rebuilt first child must still be the nested split");
        };
        assert_eq!(
            weights.as_slice(),
            [0.25, 0.75].as_slice(),
            "the nested split's NON-EQUAL weights must be copied verbatim; `split`/`remove` always \
             equalize, so seeing them here is what proves nothing re-equalized"
        );
    }

    #[test]
    fn from_layout_rejects_broken_invariants() {
        // Every input below is reachable from a hand-edited or corrupted
        // `state.json`. Rejecting is a CRASH GUARD, not tidiness: `pane_view`'s
        // `build_chain` indexes `children[0]` unconditionally, so a zero-child split
        // reaching the widget layer is a PANIC, and `collect_rects` divides by the
        // weight sum.
        let cases: [(&str, PaneLayout); 8] = [
            (
                "a zero-child split (build_chain would index children[0] and panic)",
                PaneLayout::Split {
                    orientation: Orientation::Vertical,
                    children: vec![],
                    weights: vec![],
                },
            ),
            (
                "a one-child split (below MIN_SPLIT_CHILDREN)",
                PaneLayout::Split {
                    orientation: Orientation::Vertical,
                    children: vec![PaneLayout::Leaf(P1)],
                    weights: vec![1.0],
                },
            ),
            (
                "more weights than children",
                PaneLayout::Split {
                    orientation: Orientation::Vertical,
                    children: vec![PaneLayout::Leaf(P1), PaneLayout::Leaf(P2)],
                    weights: vec![0.3, 0.3, 0.4],
                },
            ),
            (
                "fewer weights than children",
                PaneLayout::Split {
                    orientation: Orientation::Vertical,
                    children: vec![PaneLayout::Leaf(P1), PaneLayout::Leaf(P2)],
                    weights: vec![1.0],
                },
            ),
            (
                "a NaN weight (would poison every cumulative rect fraction)",
                PaneLayout::Split {
                    orientation: Orientation::Vertical,
                    children: vec![PaneLayout::Leaf(P1), PaneLayout::Leaf(P2)],
                    weights: vec![f64::NAN, 0.5],
                },
            ),
            (
                "an infinite weight",
                PaneLayout::Split {
                    orientation: Orientation::Horizontal,
                    children: vec![PaneLayout::Leaf(P1), PaneLayout::Leaf(P2)],
                    weights: vec![f64::INFINITY, 0.5],
                },
            ),
            (
                "a negative weight (would place a pane outside its parent's bounds)",
                PaneLayout::Split {
                    orientation: Orientation::Vertical,
                    children: vec![PaneLayout::Leaf(P1), PaneLayout::Leaf(P2)],
                    weights: vec![-0.5, 1.5],
                },
            ),
            (
                "an all-zero weight vector (collect_rects would divide by zero)",
                PaneLayout::Split {
                    orientation: Orientation::Vertical,
                    children: vec![PaneLayout::Leaf(P1), PaneLayout::Leaf(P2)],
                    weights: vec![0.0, 0.0],
                },
            ),
        ];

        for (desc, layout) in &cases {
            assert!(
                PaneTree::from_layout(layout).is_none(),
                "from_layout must REJECT {desc}: {layout:?}"
            );
        }

        // Validation must RECURSE: a violation buried under a well-formed root is
        // exactly as fatal as one at the top.
        let nested_violation = PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![
                PaneLayout::Leaf(P1),
                PaneLayout::Split {
                    orientation: Orientation::Horizontal,
                    children: vec![],
                    weights: vec![],
                },
            ],
            weights: vec![0.5, 0.5],
        };
        assert!(
            PaneTree::from_layout(&nested_violation).is_none(),
            "from_layout must reject a zero-child split NESTED under a well-formed root; \
             `build_chain` recurses, so a buried violation panics just the same"
        );

        // Non-vacuity: the constructor must still accept the well-formed shapes, or
        // every rejection above would hold for the wrong reason.
        assert!(
            PaneTree::from_layout(&PaneLayout::Leaf(P1)).is_some(),
            "a bare leaf is the single-pane restore shape and must be ACCEPTED"
        );
        assert!(
            PaneTree::from_layout(&PaneLayout::Split {
                orientation: Orientation::Vertical,
                children: vec![PaneLayout::Leaf(P1), PaneLayout::Leaf(P2)],
                weights: vec![0.25, 0.75],
            })
            .is_some(),
            "a well-formed two-child split with NON-EQUAL weights must be ACCEPTED; the validator \
             checks the weights are finite, non-negative and positively-summing, never that they \
             are equal shares"
        );
    }
}
