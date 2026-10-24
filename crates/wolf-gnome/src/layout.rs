//! Pure, GTK-free pane-layout tree for the `wolf-gnome` split-panes UI.
//!
//! The crate's pure-seam rule (`notes/wolf-gnome-decisions.md`, "The pure seams")
//! splits the panes feature into a PURE layout core (this module) and a GTK glue
//! layer (`pane_view.rs`). This module owns the pane-layout TREE and every geometry
//! decision that can be reasoned about without a display server, a widget, or a
//! live Neovim. The weight and divider decisions it carries — the suffix
//! convention, why a zero weight is dangerous, and why a non-finite fraction is
//! refused rather than clamped — are stated in that file's "Pane weights and
//! dividers" section.
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
//!   - reweighting one split from a DIVIDER FRACTION — the head child of the run a
//!     divider spans takes that share of the run, and the rest of the run is
//!     rescaled in proportion ([`PaneTree::set_divider_fraction`], with
//!     [`PaneTree::divider_fraction`] as its exact inverse),
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

/// The weight a fresh two-child split gives each child: half the bounds. It is the
/// default a split STARTS at; a user drag of the `gtk::Paned` divider replaces it
/// through [`PaneTree::set_divider_fraction`]. It doubles as the answer
/// [`split_head_fraction`] gives for a slice with no usable total, which is the
/// same "an even share is the safe fallback" rule.
const DEFAULT_RATIO: f64 = 0.5;

/// The arithmetic floor on a divider fraction: [`PaneTree::set_divider_fraction`]
/// clamps into `[MIN_DIVIDER_FRACTION, 1 - MIN_DIVIDER_FRACTION]`, so neither side
/// of a split can be driven to zero weight. Mirrors macOS's `pairFractionEps` in
/// `crates/wolf-macos/Sources/WolfKeyMap/PaneTree.swift`.
///
/// The hazard a zero weight carries is that nothing REJECTS it, so it is durable
/// rather than loud. [`PaneTree::from_layout`] validates only that each weight is
/// finite and non-negative and that the split's TOTAL is positive, so a lone zero
/// among positives passes; `collect_rects` divides by that total and hands the
/// child a rect with ZERO EXTENT on the split axis; [`PaneTree::neighbor`]
/// resolves against those rects through a STRICTLY positive cross-axis overlap
/// test, so a collapsed pane is never returned for a move along the perpendicular
/// direction (a zero-HEIGHT pane is unreachable by Left/Right); and
/// [`PaneTree::layout`] copies the zero out again verbatim, so it survives the
/// `state.json` round trip and comes back on every later launch.
///
/// This is the ARITHMETIC floor only. The pixel minimum is GTK's:
/// `shrink_*_child(false)` plus the one-cell `set_content_width` floor in
/// `pane_view.rs`.
const MIN_DIVIDER_FRACTION: f64 = 1e-6;

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
/// [`PaneTree::split`] / [`PaneTree::remove`] / [`PaneTree::set_divider_fraction`]
/// UPHOLD all four.
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
/// is NEVER empty (it always has at least its root leaf). See the module docs
/// for the pinned conventions (a same-orientation
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

    /// The fraction of its run that the divider `(path, boundary)` sits at: the
    /// share `children[boundary]` holds of `children[boundary..]` together.
    ///
    /// `path` is the root-to-split list of child indices (`&[]` is the root); a
    /// `boundary` of `b` names the divider BETWEEN `children[b]` and everything
    /// after it. That pairing is exactly the identity of one `gtk::Paned` in
    /// `pane_view::build_chain`'s right-leaning chain, which is why the fraction is
    /// taken over the suffix and not over the adjacent pair.
    ///
    /// `None` when there is no such divider: `path` reaches a leaf or runs off the
    /// end of a split's children, or `boundary` names the last child (a divider
    /// needs something on both sides). The exact inverse of
    /// [`PaneTree::set_divider_fraction`].
    #[must_use]
    pub fn divider_fraction(&self, path: &[usize], boundary: usize) -> Option<f64> {
        let Frame::Split { weights, .. } = frame_at(&self.root, path)? else {
            return None;
        };
        if boundary + 1 >= weights.len() {
            return None;
        }
        Some(split_head_fraction(&weights[boundary..]))
    }

    /// Reweights the split at `path` so the divider `(path, boundary)` sits at
    /// `fraction`, and reports whether it applied.
    ///
    /// This is what a `gtk::Paned` drag writes into the tree, and its SUFFIX shape
    /// is a deliberate divergence from macOS's `PaneTree.setPairFraction`
    /// (`crates/wolf-macos/Sources/WolfKeyMap/PaneTree.swift`), which reweights the
    /// adjacent PAIR. `pane_view::build_chain` renders an n-ary split as a
    /// right-leaning chain of binary paneds, so the divider after child `b` governs
    /// child `b` against every later child TOGETHER, and GTK keeps that remainder's
    /// internal proportions. Reweighting only the adjacent pair here would move a
    /// second divider the user never touched.
    ///
    /// Concretely, with `S = sum(weights[boundary..])`: `weights[boundary]` becomes
    /// `clamped * S`, the rest of `S` is spread over `weights[boundary + 1..]` in
    /// proportion to what those children already held (an all-zero remainder is
    /// equalized first, since proportions of zero say nothing), and `S` itself and
    /// every weight BEFORE `boundary` are left alone. `clamped` is `fraction`
    /// clamped into `[MIN_DIVIDER_FRACTION, 1 - MIN_DIVIDER_FRACTION]`.
    ///
    /// The whole split is then renormalized to sum to 1, restoring the documented
    /// [`Frame`] invariant. That is not merely drift control: [`PaneTree::from_layout`]
    /// copies a saved weight vector verbatim and never checks its total, so a
    /// `state.json` carrying a denormalized split produces a live tree with one, and
    /// this is where it is repaired.
    ///
    /// A NON-FINITE `fraction` is REFUSED (returns `false`, mutates nothing) rather
    /// than clamped: a NaN weight makes `from_layout` reject the whole tree at the
    /// next launch, which loses the layout silently. `false` is also the answer when
    /// `(path, boundary)` names no divider, exactly as for
    /// [`PaneTree::divider_fraction`]'s `None`.
    ///
    /// One input makes the `true` OVERSTATE what happened, and it is on the record
    /// rather than special-cased: a split whose whole run from `boundary` is zero
    /// (`[0.5, 0.5, 0.0, 0.0]` at `boundary = 2`), which no mutator here produces
    /// and which only a hand-edited `state.json` can supply. `S` is then 0, so the
    /// head is set to `clamped * 0` and the equalized remainder is scaled straight
    /// back to 0: the weights do not move, and [`PaneTree::divider_fraction`]
    /// answers [`DEFAULT_RATIO`] afterwards whatever was requested. There is no run
    /// for a fraction to be a fraction OF, so there is nothing else to do with it.
    pub fn set_divider_fraction(&mut self, path: &[usize], boundary: usize, fraction: f64) -> bool {
        if !fraction.is_finite() {
            return false;
        }
        let Some(Frame::Split { weights, .. }) = frame_at_mut(&mut self.root, path) else {
            return false;
        };
        if boundary + 1 >= weights.len() {
            return false;
        }

        let clamped = fraction.clamp(MIN_DIVIDER_FRACTION, 1.0 - MIN_DIVIDER_FRACTION);
        let run: f64 = weights[boundary..].iter().sum();
        let mut tail: f64 = weights[boundary + 1..].iter().sum();

        // An all-zero remainder carries no proportions, so equalize it to scale from; `equalize`, not
        // an open-coded `1/n`, keeps this module's one float cast in one place.
        if tail <= 0.0 {
            equalize(&mut weights[boundary + 1..]);
            tail = weights[boundary + 1..].iter().sum();
        }

        weights[boundary] = clamped * run;
        let remainder = (1.0 - clamped) * run;
        if tail > 0.0 {
            let scale = remainder / tail;
            for w in &mut weights[boundary + 1..] {
                *w *= scale;
            }
        }

        // Restore the sum-to-1 invariant across the WHOLE split, prefix included.
        let total: f64 = weights.iter().sum();
        if total > 0.0 {
            for w in weights.iter_mut() {
                *w /= total;
            }
        }
        true
    }
}

/// The head weight's share of `weights`' TOTAL — the fraction a `gtk::Paned`
/// divider sits at within the run of children it spans.
///
/// [`DEFAULT_RATIO`] for a slice with no usable total: empty, non-finite, or
/// summing to `<= 0`.
///
/// Its only production caller is [`PaneTree::divider_fraction`], and that is the
/// route every `gtk::Paned` position takes: `pane_view::install_paned_position`
/// asks the live tree for the divider's fraction on each `max-position` notify and
/// converts it with `geometry::divider_position`. Reading a divider back and
/// inverting a user's drag are therefore the same arithmetic, so the position
/// source and the drag's inverse cannot disagree about what a fraction means.
#[must_use]
pub fn split_head_fraction(weights: &[f64]) -> f64 {
    let Some(head) = weights.first() else {
        return DEFAULT_RATIO;
    };
    let total: f64 = weights.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return DEFAULT_RATIO;
    }
    head / total
}

/// Walks `path` (root-to-split child indices) from `frame`. `None` when a step
/// lands on a leaf or indexes past a split's children; `&[]` returns `frame`.
fn frame_at<'a>(frame: &'a Frame, path: &[usize]) -> Option<&'a Frame> {
    let mut node = frame;
    for index in path {
        let Frame::Split { children, .. } = node else {
            return None;
        };
        node = children.get(*index)?;
    }
    Some(node)
}

/// The mutable twin of [`frame_at`].
fn frame_at_mut<'a>(frame: &'a mut Frame, path: &[usize]) -> Option<&'a mut Frame> {
    let mut node = frame;
    for index in path {
        let Frame::Split { children, .. } = node else {
            return None;
        };
        node = children.get_mut(*index)?;
    }
    Some(node)
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

    // --- Pane ids used across the tests (small and distinct) ----------------
    const P1: PaneId = 1;
    const P2: PaneId = 2;
    const P3: PaneId = 3;
    const P4: PaneId = 4;
    /// A fifth pane, used only by `neighbor_resolves_against_a_dragged_divider`'s
    /// five-leaf case (a row of three above a row of two), which is the smallest
    /// shape in which a dragged divider CHANGES a neighbour answer.
    const P5: PaneId = 5;

    // Bounds used across the geometry tests: non-square so width and height math are distinguishable.
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

    // --- construction ---

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

    // --- split ---

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
        // Vertical split (P1 | P2), then split P1 horizontally into P1 / P3: the three rects must tile
        // the bounds with no overlap.
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

    // --- n-ary flat split: a same-orientation split flattens into an equalized row, a perpendicular
    // one nests. See crates/wolf-gnome/src/README.md#layoutrs

    #[test]
    fn vsplit_flattens_into_row() {
        // Row [P1, P2], then Vertical-split P1 again: the orientation matches, so P3 is a flat sibling
        // inserted after P1 and the row re-equalizes, not a nested sub-split.
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
        // A perpendicular split still nests: Horizontal-splitting P1 of row [P1, P2] wraps it in [P1, P3]
        // in the left half, the shape `nested_split_rects` builds by a different sequence.
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

    // --- remove / collapse ---

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
        // P1 left, [P2 / P3] right: removing P1 collapses the sibling subtree into the root slot, which
        // then fills the bounds.
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
        // Flat row [P1, P3, P2]: removing P3 re-equalizes [P1, P2] to 50/50, not 25/75 (equalalways).
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
        // Row [P1, P2] with a perpendicular column [P1, P3] on the left: removing P3 collapses the
        // one-child column, and P1 re-equalizes with P2 to 50/50.
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
        // The merge case: row[P1, col[row[P2, P4], P3]] minus P3 collapses the column, and its row merges
        // into the same-orientation root: a flat row[P1, P2, P4] of equal thirds.
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

    // --- neighbor ---

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
        // A clean 2x2 grid in 100x40: P1 (0,0) P2 (50,0) top, P3 (0,20) P4 (50,20) bottom, each 50x20.
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

    // --- determinism ---

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

    // --- from_layout: the inverse of `layout()`, for the state-restore path ---

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
        // A same-orientation nested split, which `split` never makes but a hand-edited `state.json` can:
        // `from_layout` preserves it, so the tree nvim was asked for and the one GTK builds stay in step.
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
        // Every input is reachable from a hand-edited `state.json`; rejecting is a crash guard, since
        // `build_chain` indexes `children[0]` blindly and `collect_rects` divides by the weight sum.
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

    // --- divider fractions. `set_divider_fraction` is the only writer of a non-equal weight vector,
    // so these tests keep `pane_layout`'s and the smokes' saved-weight assertions from being vacuous.

    /// A two-leaf vertical split at [`DEFAULT_RATIO`] — the shape one `<C-w>v`
    /// produces.
    fn two_leaf_row() -> PaneTree {
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree
    }

    /// A FLAT row of three equal thirds, whose DFS order is `[P1, P3, P2]` — a
    /// same-orientation split inserts the new pane AFTER its target (see
    /// `vsplit_flattens_into_row`).
    fn flat_row_of_three() -> PaneTree {
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Vertical);
        tree
    }

    /// The weights of the split that `path` reaches in `tree`, panicking with a
    /// readable message when it reaches something else.
    ///
    /// It reads through [`PaneTree::layout`] rather than the private [`Frame`], so
    /// it sees exactly what the widget builder and the persistence codec see.
    fn weights_at(tree: &PaneTree, path: &[usize]) -> Vec<f64> {
        let mut node = tree.layout();
        for (depth, index) in path.iter().enumerate() {
            let PaneLayout::Split { children, .. } = node else {
                panic!("path {path:?} hits a LEAF at depth {depth}, so it reaches no split");
            };
            let count = children.len();
            node = children.into_iter().nth(*index).unwrap_or_else(|| {
                panic!(
                    "path {path:?} indexes child {index} at depth {depth}, but that split has \
                     {count} children"
                )
            });
        }
        match node {
            PaneLayout::Split { weights, .. } => weights,
            PaneLayout::Leaf(id) => panic!("path {path:?} reaches leaf {id}, not a split"),
        }
    }

    /// The head weight's share of the slice's TOTAL, plus [`DEFAULT_RATIO`] for
    /// every degenerate slice.
    ///
    /// This is the expression EVERY `gtk::Paned` position is derived from, hoisted
    /// here so a drag's inverse and the position source cannot disagree about what
    /// a divider fraction means. The path is
    /// `pane_view::install_paned_position` → [`PaneTree::divider_fraction`] → this
    /// → `geometry::divider_position`, run on each `max-position` notify.
    ///
    /// `pane_view::build_chain` sets NO position: a paned has no travel before its
    /// first allocation, so a seed there would park the head child at its minimum.
    /// It only threads the `(path, boundary)` identity through. Do not go looking
    /// for a `set_position` in the builder, and do not add one.
    ///
    /// FALSIFYING EDIT: divide by `weights.len()` instead of by the sum.
    ///
    /// MEASURED: the first row to fire is the SINGLE-WEIGHT one — `case a lone
    /// weight IS the whole slice: split_head_fraction([0.7]) = 0.7, want 1`. A lone
    /// weight is its own total but only ever one child of one, so the two divisors
    /// disagree there just as loudly as they do lower down the table; the
    /// `[0.25, 0.75]` row (`0.125`) and the `[1, 1, 2]` row (`0.333…`) are equally
    /// wrong under the edit and are simply reached later.
    #[test]
    fn split_head_fraction_table() {
        let cases: [(&str, &[f64], f64); 8] = [
            (
                "an empty slice has no head, so there is no share to take",
                &[],
                DEFAULT_RATIO,
            ),
            ("a lone weight IS the whole slice", &[0.7], 1.0),
            (
                "the head's share is of the TOTAL, not of the child COUNT",
                &[0.25, 0.75],
                0.25,
            ),
            (
                "three children at 1:1:2 — the head spans a quarter of the run",
                &[1.0, 1.0, 2.0],
                0.25,
            ),
            (
                "a zero-summing slice would divide by zero",
                &[0.0, 0.0],
                DEFAULT_RATIO,
            ),
            (
                "a negative total is degenerate the same way",
                &[-1.0, 0.5],
                DEFAULT_RATIO,
            ),
            (
                "a NaN anywhere poisons the total",
                &[f64::NAN, 0.5],
                DEFAULT_RATIO,
            ),
            (
                "an infinite weight likewise",
                &[f64::INFINITY, 0.5],
                DEFAULT_RATIO,
            ),
        ];

        for (desc, weights, want) in cases {
            let got = split_head_fraction(weights);
            assert!(
                approx(got, want),
                "case {desc}: split_head_fraction({weights:?}) = {got}, want {want}"
            );
        }
    }

    /// **The macOS divergence, pinned.** A divider reweights its head child against
    /// the WHOLE SUFFIX it spans, and that suffix keeps its internal proportions.
    ///
    /// `pane_view::build_chain` renders an n-ary split as a right-leaning chain of
    /// binary `gtk::Paned`s, so the divider after child 0 of a row of three governs
    /// child 0 against `[child 1, child 2]` TOGETHER, and GTK keeps the pair's own
    /// proportion. macOS's `PaneTree.setPairFraction`
    /// (`crates/wolf-macos/Sources/WolfKeyMap/PaneTree.swift`) reweights only the
    /// ADJACENT PAIR, which on this front-end would move a second divider the user
    /// never touched.
    ///
    /// FALSIFYING EDIT: put all of `(1 - clamped) * S` into `weights[boundary + 1]`
    /// (macOS pair semantics).
    ///
    /// MEASURED: `dragging the FIRST divider of equal thirds to 0.5 must yield
    /// [0.5, 0.25, 0.25], got [0.375, 0.375, 0.25]` — child 2 was never rescaled,
    /// and the suffix's 1:1 proportion is gone.
    #[test]
    fn set_divider_fraction_reweights_head_against_the_suffix() {
        let mut tree = flat_row_of_three();
        assert!(
            tree.set_divider_fraction(&[], 0, 0.5),
            "a finite fraction on the first boundary of a flat row of three must APPLY"
        );

        let w = weights_at(&tree, &[]);
        assert_eq!(w.len(), 3, "the row still has three children: {w:?}");
        assert!(
            approx(w[0], 0.5) && approx(w[1], 0.25) && approx(w[2], 0.25),
            "dragging the FIRST divider of equal thirds to 0.5 must yield [0.5, 0.25, 0.25], got \
             {w:?}. The head takes half of the whole suffix it spans, and the remaining half is \
             spread across the suffix IN PROPORTION to what its members already held."
        );
        assert!(
            approx(w[1], w[2]),
            "the suffix must keep its internal 1:1 proportion, got {w:?}. This is the macOS \
             divergence: `setPairFraction` would have pushed the entire remainder into child 1 and \
             left child 2 where it was, which on GTK moves the SECOND divider — one the user never \
             touched — because `build_chain` nests [child 1, child 2] inside one paned."
        );
        let total: f64 = w.iter().sum();
        assert!(
            approx(total, 1.0),
            "the split's weights must still sum to 1 (the documented `Frame` invariant), got \
             {total} from {w:?}"
        );

        // The same statement in pixels, which is what the user actually sees.
        let rects = tree.pane_rects(W, H);
        assert_rect(rect_for(&rects, P1), 0.0, 0.0, HALF_W, H, "P1 takes half");
        assert_rect(
            rect_for(&rects, P3),
            HALF_W,
            0.0,
            W / 4.0,
            H,
            "P3 keeps its half of the suffix",
        );
        assert_rect(
            rect_for(&rects, P2),
            HALF_W + W / 4.0,
            0.0,
            W / 4.0,
            H,
            "P2 keeps the other half of the suffix",
        );
    }

    /// A drag at an INNER boundary leaves everything before it alone: only the
    /// suffix the divider spans is redistributed, and the prefix's weights are not
    /// even read.
    ///
    /// FALSIFYING EDIT: make the mutator always operate on `boundary = 0`.
    ///
    /// MEASURED: the child-0 clause fires first — `child 0 sits BEFORE the dragged
    /// boundary and must be untouched: was 0.3333333333333333, now 0.75` — so the
    /// symptom is stated as the prefix moving rather than as the whole row reading
    /// `[0.75, 0.125, 0.125]`. No divider between children 1 and 2 can reach child
    /// 0 at all, which is why that clause is the one worth reading first.
    #[test]
    fn set_divider_fraction_at_an_inner_boundary() {
        let mut tree = flat_row_of_three();
        let before = weights_at(&tree, &[]);

        assert!(
            tree.set_divider_fraction(&[], 1, 0.75),
            "the second boundary of a row of three is a real divider and must APPLY"
        );

        let w = weights_at(&tree, &[]);
        assert!(
            approx(w[0], before[0]),
            "child 0 sits BEFORE the dragged boundary and must be untouched: was {}, now {}. The \
             suffix sum `S` is what absorbs the drag, so nothing outside `weights[boundary..]` may \
             move.",
            before[0],
            w[0]
        );
        assert!(
            approx(w[1], 0.5) && approx(w[2], 1.0 / 6.0),
            "dragging the SECOND divider of equal thirds to 0.75 must yield [1/3, 0.5, 1/6] — the \
             head takes three quarters of the 2/3 the divider spans — got {w:?}"
        );
        let total: f64 = w.iter().sum();
        assert!(
            approx(total, 1.0),
            "the split's weights must still sum to 1, got {total} from {w:?}"
        );
    }

    /// The `path` really is walked: a drag on a NESTED split touches that split and
    /// nothing above it.
    ///
    /// FALSIFYING EDIT: make `frame_at_mut` ignore its `path` and return the root.
    ///
    /// MEASURED: `the OUTER row must be untouched by a drag inside its first child,
    /// got [0.8, 0.19999999999999996]` — the drag landed on the root instead of the
    /// nested column, which leaves the inner column at `[0.5, 0.5]` and every rect
    /// below wrong.
    #[test]
    fn set_divider_fraction_walks_a_path() {
        // Vertical[Horizontal[P1, P3], P2]: a stacked pair in the left half, one full-height pane right.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P1, P3, Orientation::Horizontal);

        assert!(
            tree.set_divider_fraction(&[0], 0, 0.8),
            "child 0 of the root IS a split, so the path `[0]` reaches a real divider"
        );

        let outer = weights_at(&tree, &[]);
        assert!(
            approx(outer[0], DEFAULT_RATIO) && approx(outer[1], DEFAULT_RATIO),
            "the OUTER row must be untouched by a drag inside its first child, got {outer:?}"
        );
        let inner = weights_at(&tree, &[0]);
        assert!(
            approx(inner[0], 0.8) && approx(inner[1], 0.2),
            "the INNER column must carry the drag, got {inner:?}"
        );

        let rects = tree.pane_rects(W, H);
        assert_rect(
            rect_for(&rects, P1),
            0.0,
            0.0,
            HALF_W,
            0.8 * H,
            "P1 takes 80% of the left column's height",
        );
        assert_rect(
            rect_for(&rects, P3),
            0.0,
            0.8 * H,
            HALF_W,
            0.2 * H,
            "P3 takes the remaining 20%",
        );
        assert_rect(
            rect_for(&rects, P2),
            HALF_W,
            0.0,
            HALF_W,
            H,
            "P2 still spans the full height of the right half",
        );
    }

    /// A fraction outside `(0, 1)` is CLAMPED so neither side of a split can be
    /// driven to zero weight; a NON-FINITE fraction is REJECTED outright rather
    /// than clamped.
    ///
    /// The two are deliberately different answers. A clamped `0.0` still leaves a
    /// usable (if tiny) pane, whereas a NaN weight makes
    /// [`PaneTree::from_layout`] reject the whole tree at the next launch — silent
    /// layout loss rather than a visible bug — so it must never be written.
    ///
    /// FALSIFYING EDITS, two, one per half.
    ///
    /// - Delete the `clamp`. MEASURED: `case 0.0 would collapse the head child to
    ///   nothing: weight 0 is 0 … Weights were [0.0, 1.0].` — the
    ///   strictly-positive clause, as expected.
    /// - Delete the `is_finite` guard. MEASURED: `case NaN is REJECTED, never
    ///   clamped: set_divider_fraction(.., NaN) returned true`, left `true` and
    ///   right `false`. So the RETURN VALUE is what fires, one assertion earlier
    ///   than the bit-identity clause below it — which is the better failure of the
    ///   two to read, since it says the mutator accepted the fraction rather than
    ///   only that the weights moved.
    #[test]
    fn set_divider_fraction_clamps_and_rejects() {
        struct Case {
            desc: &'static str,
            fraction: f64,
            applies: bool,
        }

        let cases = [
            Case {
                desc: "0.0 would collapse the head child to nothing",
                fraction: 0.0,
                applies: true,
            },
            Case {
                desc: "1.0 would collapse the tail to nothing",
                fraction: 1.0,
                applies: true,
            },
            Case {
                desc: "a negative fraction clamps like 0.0",
                fraction: -1.0,
                applies: true,
            },
            Case {
                desc: "a fraction above 1 clamps like 1.0",
                fraction: 2.0,
                applies: true,
            },
            Case {
                desc: "NaN is REJECTED, never clamped",
                fraction: f64::NAN,
                applies: false,
            },
            Case {
                desc: "+inf is REJECTED, never clamped",
                fraction: f64::INFINITY,
                applies: false,
            },
            Case {
                desc: "-inf is REJECTED, never clamped",
                fraction: f64::NEG_INFINITY,
                applies: false,
            },
        ];

        for case in &cases {
            let mut tree = two_leaf_row();
            let before: Vec<u64> = weights_at(&tree, &[]).iter().map(|w| w.to_bits()).collect();

            let applied = tree.set_divider_fraction(&[], 0, case.fraction);
            assert_eq!(
                applied, case.applies,
                "case {}: set_divider_fraction(.., {}) returned {applied}",
                case.desc, case.fraction
            );

            let after = weights_at(&tree, &[]);
            if case.applies {
                for (index, weight) in after.iter().enumerate() {
                    assert!(
                        weight.is_finite() && *weight > 0.0,
                        "case {}: weight {index} is {weight}, but every weight must stay finite \
                         and STRICTLY positive. A zero weight is a pane allocated no pixels at \
                         all — `pane_rects` hands it an empty rect and the user cannot drag it \
                         back — which is what MIN_DIVIDER_FRACTION exists to prevent. Weights were \
                         {after:?}.",
                        case.desc
                    );
                }
                let total: f64 = after.iter().sum();
                assert!(
                    approx(total, 1.0),
                    "case {}: the clamped drag left the weights summing to {total} ({after:?}), \
                     not 1",
                    case.desc
                );
            } else {
                let bits: Vec<u64> = after.iter().map(|w| w.to_bits()).collect();
                assert_eq!(
                    bits, before,
                    "case {}: a REJECTED fraction must leave the weights BIT-IDENTICAL, got \
                     {after:?}. Comparing bit patterns rather than values is deliberate: a NaN \
                     that slipped into the vector compares unequal to everything including \
                     itself, so a value comparison could not report what actually landed there.",
                    case.desc
                );
            }
        }
    }

    /// Every path that does not name a real divider is a no-op for the mutator and
    /// `None` for the reader — the two agree case for case, which is what lets
    /// `install_paned_position` treat `None` as "this paned no longer exists in the
    /// tree" rather than guessing.
    ///
    /// FALSIFYING EDIT: remove the `boundary + 1 < weights.len()` guard from
    /// `set_divider_fraction`.
    ///
    /// MEASURED: `case the LAST boundary of a two-child split has no child after it:
    /// set_divider_fraction([], 1, 0.75) must report that it did NOT apply`. The
    /// reader's `None` clause above it stays green — the edit is in the mutator
    /// only — so what fires is the mutator claiming a divider the reader says does
    /// not exist, which is precisely the disagreement this test exists to forbid.
    #[test]
    fn set_divider_fraction_is_a_noop_off_the_tree() {
        let cases: Vec<(&str, PaneTree, Vec<usize>, usize)> = vec![
            (
                "a bare-leaf root has no split to reweight at all",
                PaneTree::new(P1),
                vec![],
                0,
            ),
            ("the path walks INTO a leaf", two_leaf_row(), vec![0], 0),
            ("the path walks PAST a leaf", two_leaf_row(), vec![0, 0], 0),
            (
                "the path indexes a child that does not exist",
                two_leaf_row(),
                vec![5],
                0,
            ),
            (
                "the LAST boundary of a two-child split has no child after it",
                two_leaf_row(),
                vec![],
                1,
            ),
            (
                "the last boundary of a row of THREE, likewise",
                flat_row_of_three(),
                vec![],
                2,
            ),
            ("a boundary far past the end", two_leaf_row(), vec![], 9),
        ];

        for (desc, mut tree, path, boundary) in cases {
            let before = tree.layout();

            assert_eq!(
                tree.divider_fraction(&path, boundary),
                None,
                "case {desc}: divider_fraction({path:?}, {boundary}) must be None — there is no \
                 `gtk::Paned` at that (path, boundary), so there is no fraction to report"
            );
            assert!(
                !tree.set_divider_fraction(&path, boundary, 0.75),
                "case {desc}: set_divider_fraction({path:?}, {boundary}, 0.75) must report that it \
                 did NOT apply"
            );
            assert_eq!(
                tree.layout(),
                before,
                "case {desc}: a refused set_divider_fraction must leave the tree byte-for-byte \
                 unchanged; a partial mutation would persist to `state.json` on the next wake"
            );
        }
    }

    /// [`PaneTree::divider_fraction`] is the exact inverse of
    /// [`PaneTree::set_divider_fraction`], which is the property
    /// `install_paned_position` leans on: every `max-position` notify re-reads the
    /// tree and re-derives the position, so a reader that disagreed with the writer
    /// would walk the divider a little further on every allocation pass.
    ///
    /// The INNER boundary rows are the discriminating ones. At boundary 0 the
    /// suffix is the whole split, so `weights[0]` and the head's share of the
    /// suffix are the same number and a reader that returned the raw weight would
    /// look correct.
    ///
    /// FALSIFYING EDITS, two:
    ///
    /// - Make `divider_fraction` return `weights[boundary]` instead of the head's
    ///   share of the suffix. Every boundary-1 row fails and every boundary-0 row
    ///   stays green. MEASURED, on the first boundary-1 row the loop reaches:
    ///   `set 0.1 at boundary 1, read back 0.06666666666666667` — the raw weight
    ///   `0.1 * 2/3`, not the head's share of the two-thirds the divider spans.
    /// - Make it ignore `boundary` whenever `path` is non-empty — answering about
    ///   the whole nested split. Only the LAST case below fails: the other path
    ///   case is at boundary 0, where the two agree, and the flat cases have an
    ///   empty path. MEASURED: `the nested column's INNER divider read back as
    ///   0.3333333333333333, not the 0.25 it was set to`.
    #[test]
    fn divider_fraction_round_trips_set_divider_fraction() {
        const FRACTIONS: [f64; 5] = [0.1, 0.25, 0.5, 0.75, 0.9];

        for fraction in FRACTIONS {
            for boundary in [0_usize, 1] {
                let mut tree = flat_row_of_three();
                assert!(
                    tree.set_divider_fraction(&[], boundary, fraction),
                    "boundary {boundary} of a row of three is a real divider"
                );
                let Some(got) = tree.divider_fraction(&[], boundary) else {
                    panic!(
                        "divider_fraction(&[], {boundary}) returned None right after a successful \
                         set_divider_fraction of the same divider"
                    );
                };
                assert!(
                    approx(got, fraction),
                    "set {fraction} at boundary {boundary}, read back {got} (weights {:?})",
                    weights_at(&tree, &[])
                );
            }
        }

        // …and through a path, so the reader walks the tree the same way the writer
        // does rather than only ever answering about the root.
        let mut nested = PaneTree::new(P1);
        nested.split(P1, P2, Orientation::Vertical);
        nested.split(P1, P3, Orientation::Horizontal);
        assert!(nested.set_divider_fraction(&[0], 0, 0.8));
        let Some(got) = nested.divider_fraction(&[0], 0) else {
            panic!("divider_fraction(&[0], 0) returned None for a nested split that exists");
        };
        assert!(
            approx(got, 0.8),
            "the nested column's fraction read back as {got}, not 0.8"
        );

        // …and at an inner boundary of a nested split, the only shape where `path` and `boundary` can be
        // confused: Vertical[Horizontal[P1, P3, P4], P2]. A reader conflating them passes every case above.
        let mut deep = PaneTree::new(P1);
        deep.split(P1, P2, Orientation::Vertical);
        deep.split(P1, P3, Orientation::Horizontal);
        deep.split(P3, P4, Orientation::Horizontal);
        assert_eq!(
            deep.leaves(),
            vec![P1, P3, P4, P2],
            "precondition: the left column really holds three panes, so it HAS an inner boundary"
        );

        assert!(
            deep.set_divider_fraction(&[0], 1, 0.25),
            "boundary 1 of a three-child nested split is a real divider"
        );
        let Some(got) = deep.divider_fraction(&[0], 1) else {
            panic!(
                "divider_fraction(&[0], 1) returned None. The root has only two children, so a \
                 reader that dropped the path and answered about the ROOT would report None here"
            );
        };
        assert!(
            approx(got, 0.25),
            "the nested column's INNER divider read back as {got}, not the 0.25 it was set to. \
             `1/3` means the reader ignored `boundary` and answered about the whole nested split; \
             `1/6` means it returned the raw weight instead of the head's share of the suffix. \
             Inner weights are {:?}.",
            weights_at(&deep, &[0])
        );

        let inner = weights_at(&deep, &[0]);
        assert!(
            approx(inner[0], 1.0 / 3.0),
            "the column's PREFIX child sits before the dragged boundary and must be untouched, \
             got {inner:?}"
        );
        let outer = weights_at(&deep, &[]);
        assert!(
            approx(outer[0], DEFAULT_RATIO) && approx(outer[1], DEFAULT_RATIO),
            "and the OUTER row must be untouched by a drag two levels down, got {outer:?}"
        );
    }

    /// Twenty drags in a row, alternating between two boundaries and including the
    /// clamped extremes, leave the split's invariants intact: every weight finite
    /// and strictly positive, and the whole vector still summing to 1.
    ///
    /// A divider drag emits a `notify::position` per motion event, so a real drag
    /// is tens of these, and a user resizes panes all session.
    ///
    /// FALSIFYING EDIT: delete the `clamp`. The `1.0` row of the sequence then
    /// hands the whole split to the head child and leaves the suffix at zero, and
    /// the strictly-positive assertion fires naming the drag index. The `0.0` row
    /// two drags later would do the same from the other side, and a drag whose
    /// suffix sums to zero is also how the zero-tail fallback gets exercised.
    ///
    /// MEASURED: `after drag 2 (fraction 1 at boundary 0) weight 1 is 0. Weights
    /// were [1.0, 0.0, 0.0].` — the third drag of the sequence, which is the first
    /// one at a clamped extreme.
    ///
    /// NOTE for the red run: deleting the FINAL RENORMALIZATION does NOT falsify
    /// this test, and it is not claimed to. This tree starts out summing to 1 and
    /// the suffix arithmetic preserves that analytically, so twenty drags
    /// accumulate a drift of order `1e-15` — seven orders of magnitude inside
    /// [`EPS`]. The renormalization is pinned instead by
    /// `set_divider_fraction_normalizes_a_denormalized_split`, from the one input
    /// that can actually violate the invariant. What this test pins is that the
    /// clamp holds up under REPEATED drags, including the case where an earlier
    /// drag has left the next drag's suffix almost empty.
    #[test]
    fn repeated_divider_drags_preserve_the_weight_sum() {
        // Fractions cycled through the drags, deliberately including both clamped
        // extremes.
        const SEQUENCE: [f64; 5] = [0.9, 0.1, 1.0, 0.0, 0.6];
        // How many drags to apply.
        const DRAGS: usize = 20;

        let mut tree = flat_row_of_three();
        for drag in 0..DRAGS {
            let boundary = drag % 2;
            let fraction = SEQUENCE[drag % SEQUENCE.len()];
            assert!(
                tree.set_divider_fraction(&[], boundary, fraction),
                "drag {drag}: a finite fraction ({fraction}) at boundary {boundary} must apply"
            );

            let w = weights_at(&tree, &[]);
            for (index, weight) in w.iter().enumerate() {
                assert!(
                    weight.is_finite() && *weight > 0.0,
                    "after drag {drag} (fraction {fraction} at boundary {boundary}) weight {index} \
                     is {weight}. Weights were {w:?}."
                );
            }
            let total: f64 = w.iter().sum();
            assert!(
                approx(total, 1.0),
                "after drag {drag} the weights sum to {total}, not 1. Weights were {w:?}."
            );
        }
    }

    /// The final renormalization, pinned from the one input that can actually
    /// violate the `Frame` invariant: a DENORMALIZED split.
    ///
    /// [`PaneTree::from_layout`] validates that every weight is finite and
    /// non-negative and that the total is POSITIVE. It does not check that the total
    /// is 1, and it copies the vector verbatim. `state.json` carries those weights
    /// as plain JSON and `pane_layout::decode` feeds them straight to `from_layout`,
    /// so a hand-edited or foreign-written state file really does produce a live
    /// tree whose weights sum to anything positive — the precondition below STATES
    /// that reachability rather than assuming it.
    ///
    /// The SUM is the discriminating assertion and the fraction deliberately is not:
    /// [`split_head_fraction`] divides by the slice's own total, so
    /// [`PaneTree::divider_fraction`] reads back the requested value whether or not
    /// anything renormalized, and every divider would still be POSITIONED correctly.
    /// What a denormalized tree loses is the invariant every later reader is
    /// entitled to assume; the fraction clause is here to document exactly which
    /// half of that this test can see.
    ///
    /// This does not contradict `set_divider_fraction_at_an_inner_boundary`'s "child
    /// 0 is untouched". There the input already summed to 1, so the renormalization
    /// was a no-op. What is preserved for an untouched child is its SHARE, not its
    /// raw weight — which is why the three-child case below asserts on a prefix
    /// weight the mutator never wrote.
    ///
    /// FALSIFYING EDIT: delete the final renormalization from
    /// `set_divider_fraction`.
    ///
    /// MEASURED: `after one drag the split must sum to 1, got 4 from [1.0, 3.0]` —
    /// the two-child case's SUM clause, so the three-child case is never reached
    /// (its sum would stay at 4 too), and both divider fractions still read back
    /// exactly as requested.
    #[test]
    fn set_divider_fraction_normalizes_a_denormalized_split() {
        // The sum a hand-edited `state.json` is standing in for. Any positive
        // non-1 total would do; 4 keeps every expected weight exactly representable.
        const DENORMALIZED_TOTAL: f64 = 4.0;

        // --- Two children, dragged at the only boundary ------------------------
        let pair = PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![PaneLayout::Leaf(P1), PaneLayout::Leaf(P2)],
            weights: vec![3.0, 1.0],
        };
        let Some(mut tree) = PaneTree::from_layout(&pair) else {
            panic!(
                "precondition: from_layout must ACCEPT a denormalized split — it checks that the \
                 weights are finite, non-negative and positively-summing, never that they sum to \
                 1. If this starts failing, a sum check was added there and this whole test is \
                 about an unreachable input: {pair:?}"
            );
        };
        let before: f64 = weights_at(&tree, &[]).iter().sum();
        assert!(
            approx(before, DENORMALIZED_TOTAL),
            "precondition: the tree must really start out summing to {DENORMALIZED_TOTAL}, got \
             {before}. `from_layout` copies weights VERBATIM, so a 1 here means it normalized on \
             the way in and the drag below would have nothing to repair."
        );

        assert!(
            tree.set_divider_fraction(&[], 0, 0.25),
            "a finite fraction on a real divider must apply, denormalized input or not"
        );
        let w = weights_at(&tree, &[]);
        let total: f64 = w.iter().sum();
        assert!(
            approx(total, 1.0),
            "after one drag the split must sum to 1, got {total} from {w:?}. The tree came out of \
             `from_layout` summing to {DENORMALIZED_TOTAL} — which a hand-edited `state.json` can \
             produce — so the mutator is where the documented `Frame` invariant is restored. \
             Leaving it at {DENORMALIZED_TOTAL} persists a denormalized vector back to disk on the \
             next wake, and every drag after this one compounds it."
        );
        assert!(
            approx(w[0], 0.25) && approx(w[1], 0.75),
            "the renormalized pair must be [0.25, 0.75] — the dragged fraction of the whole — got \
             {w:?}"
        );
        let Some(fraction) = tree.divider_fraction(&[], 0) else {
            panic!("divider_fraction returned None for the two-child split just dragged");
        };
        assert!(
            approx(fraction, 0.25),
            "the divider fraction must read back as 0.25, got {fraction}. NOTE this clause cannot \
             fail from a missing renormalization: `split_head_fraction` divides by the slice's own \
             total, so it answers 0.25 for [1, 3] exactly as it does for [0.25, 0.75]. The SUM \
             assertion above is the one that discriminates."
        );

        // --- Three children, dragged at the inner boundary: renormalization scales the whole vector,
        // including the unwritten prefix: `weights[0]` goes in at 2 of 4 and comes out at 0.5 of 1.
        let row = PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![
                PaneLayout::Leaf(P1),
                PaneLayout::Leaf(P2),
                PaneLayout::Leaf(P3),
            ],
            weights: vec![2.0, 1.0, 1.0],
        };
        let Some(mut tree) = PaneTree::from_layout(&row) else {
            panic!("precondition: from_layout must accept the denormalized row of three: {row:?}");
        };

        assert!(
            tree.set_divider_fraction(&[], 1, 0.75),
            "the inner boundary of a denormalized row of three is still a real divider"
        );
        let w = weights_at(&tree, &[]);
        let total: f64 = w.iter().sum();
        assert!(
            approx(total, 1.0),
            "after a drag at an INNER boundary the whole split must sum to 1, got {total} from \
             {w:?}. The mutator writes only `weights[1..]`, so this is what says the \
             renormalization spans the entire vector rather than the suffix it touched."
        );
        assert!(
            approx(w[0], 0.5),
            "the PREFIX weight the drag never wrote must be rescaled with the rest: it went in at \
             2 of {DENORMALIZED_TOTAL} and must come out at 0.5 of 1, got {w:?}. Its SHARE is \
             unchanged, which is the sense in which a prefix child is `untouched`."
        );
        assert!(
            approx(w[1], 0.375) && approx(w[2], 0.125),
            "the dragged suffix must be [0.375, 0.125] — three quarters and one quarter of the \
             half the divider spans — got {w:?}"
        );
        let Some(fraction) = tree.divider_fraction(&[], 1) else {
            panic!("divider_fraction returned None for the inner boundary just dragged");
        };
        assert!(
            approx(fraction, 0.75),
            "the inner divider's fraction must read back as 0.75, got {fraction}. As above, this \
             clause is normalization-independent and cannot be the one that fires."
        );
    }

    /// The zero-tail fallback, pinned from the one input that can reach it: a split
    /// whose SUFFIX is all zeros.
    ///
    /// No sequence of drags can produce one — the clamp keeps every weight this
    /// module writes strictly positive — but [`PaneTree::from_layout`] can.
    /// It rejects a negative weight and a zero-summing WHOLE vector, and
    /// `[1.0, 0.0, 0.0]` is none of those: finite, non-negative, summing to 1. So it
    /// is copied in verbatim, and `state.json` carries exactly such a vector as
    /// plain JSON through `pane_layout::decode`.
    ///
    /// Without the fallback the arithmetic has nothing to work with. Scaling a zero
    /// tail by any factor leaves it zero, so the remainder the drag hands back
    /// evaporates and the renormalization divides by the head alone: BOTH tail panes
    /// are stranded at zero width, and there is no divider left to drag them back
    /// because `pane_rects` gives each of them an empty rect to press.
    ///
    /// FALSIFYING EDIT: delete the zero-tail `equalize` branch.
    ///
    /// MEASURED: `weight 1 is 0 ([1.0, 0.0, 0.0]). A drag against an all-zero tail
    /// must REVIVE it` — the strictly-positive clause fires first, so the fraction
    /// clause (which would read `1.0` rather than the `0.5` requested) and the
    /// zero-width rect clause are never reached.
    #[test]
    fn set_divider_fraction_revives_an_all_zero_tail() {
        let all_zero_tail = PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![
                PaneLayout::Leaf(P1),
                PaneLayout::Leaf(P2),
                PaneLayout::Leaf(P3),
            ],
            weights: vec![1.0, 0.0, 0.0],
        };
        let Some(mut tree) = PaneTree::from_layout(&all_zero_tail) else {
            panic!(
                "precondition: from_layout must ACCEPT a split whose TAIL is all zeros — it \
                 rejects a negative weight and a zero-SUMMING vector, and this one is finite, \
                 non-negative and sums to 1. If this ever starts failing, the branch under test \
                 became unreachable and should be deleted rather than tested: {all_zero_tail:?}"
            );
        };

        assert!(
            tree.set_divider_fraction(&[], 0, 0.5),
            "a finite fraction on a real divider must apply, zero tail or not"
        );

        let w = weights_at(&tree, &[]);
        for (index, weight) in w.iter().enumerate() {
            assert!(
                weight.is_finite() && *weight > 0.0,
                "weight {index} is {weight} ({w:?}). A drag against an all-zero tail must REVIVE \
                 it: scaling zeros leaves zeros, so without the equalize fallback the two tail \
                 panes are stranded at zero width with no divider left to drag them back."
            );
        }
        assert!(
            approx(w[1], w[2]),
            "the revived tail must be EQUAL shares, got {w:?}. There is no prior proportion to \
             preserve — that is what makes this case different from every other drag — so equal \
             is the only non-arbitrary answer."
        );
        assert!(
            approx(w[0], 0.5) && approx(w[1], 0.25) && approx(w[2], 0.25),
            "the whole split must read [0.5, 0.25, 0.25]: the head takes the fraction it was \
             dragged to, and the revived tail splits the remainder evenly. Got {w:?} — a \
             [1/3, 1/3, 1/3] here means the tail was equalized but never RESCALED to the \
             remainder, which puts the divider somewhere the user did not drag it."
        );
        let total: f64 = w.iter().sum();
        assert!(
            approx(total, 1.0),
            "the revived split must still sum to 1, got {total} from {w:?}"
        );

        let Some(fraction) = tree.divider_fraction(&[], 0) else {
            panic!("divider_fraction returned None for the three-child split just dragged");
        };
        assert!(
            approx(fraction, 0.5),
            "the divider must sit where it was dragged, got {fraction}. A 1.0 here is the \
             stranded-tail symptom: the head owns the entire split."
        );

        // The same statement in pixels, which is the form the user meets it in.
        for (id, rect) in tree.pane_rects(W, H) {
            assert!(
                rect.w > EPS,
                "pane {id} came out ZERO-WIDTH ({rect:?}) from weights {w:?}. A pane with no \
                 rect has no divider beside it to grab, so this state is unrecoverable by any \
                 drag the user can perform."
            );
        }
    }

    /// CHARACTERIZATION: a drag whose whole RUN is zero reports `true` and moves
    /// nothing, and the divider afterwards reads [`DEFAULT_RATIO`] whatever was
    /// asked for.
    ///
    /// The `true` overstates what happened, and that is worth pinning precisely
    /// because it is not obviously right. With `S = sum(weights[boundary..]) = 0`
    /// the head becomes `clamped * 0`, the equalized remainder is scaled straight
    /// back to `0`, and the renormalization divides by a total the PREFIX alone
    /// supplies — so every weight lands exactly where it started while the call
    /// says it applied. `divider_fraction` then has no run to take a share of and
    /// answers the fallback.
    ///
    /// The behaviour is defensible: there is no run for a fraction to be a fraction
    /// OF, so there is nothing else the call could do with one, and the input is
    /// reachable only from a hand-edited `state.json` because no mutator in this
    /// module produces a zero weight. Sibling coverage does not reach it —
    /// `set_divider_fraction_revives_an_all_zero_tail`'s run is `1.0`, only its TAIL
    /// is zero — so without this test the behaviour is invisible.
    ///
    /// The prefix `[0.5, 0.5]` is chosen to sum to 1 deliberately. The
    /// renormalization would otherwise rescale the whole vector and the
    /// bit-identity assertion below would be measuring float arithmetic rather than
    /// "the drag did nothing".
    ///
    /// FALSIFYING EDIT: return `false` when the run is zero. That is exactly the
    /// change this test exists to make visible, and no other test in this module
    /// fires on it. It is also a defensible change rather than a bug — `false`
    /// would tell `install_paned_position` that its write-back did not land — but
    /// `false` currently means "this divider does not exist" or "the fraction was
    /// NaN", and the widget layer treats both as stop-and-do-nothing. Whoever makes
    /// it should say what the caller then does differently.
    #[test]
    fn set_divider_fraction_reports_true_on_a_zero_run() {
        let zero_run = PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![
                PaneLayout::Leaf(P1),
                PaneLayout::Leaf(P2),
                PaneLayout::Leaf(P3),
                PaneLayout::Leaf(P4),
            ],
            weights: vec![0.5, 0.5, 0.0, 0.0],
        };
        let Some(mut tree) = PaneTree::from_layout(&zero_run) else {
            panic!(
                "precondition: from_layout must ACCEPT a split with a zero RUN — every weight is \
                 finite and non-negative and the vector sums to 1, which is all it checks. That \
                 acceptance is the whole reachability this test rests on: {zero_run:?}"
            );
        };
        let before: Vec<u64> = weights_at(&tree, &[]).iter().map(|w| w.to_bits()).collect();

        assert!(
            tree.set_divider_fraction(&[], 2, 0.8),
            "boundary 2 of a four-child split names a real divider, so the call reports that it \
             APPLIED — even though the run it would reweight is empty. A `false` here is the \
             falsifying edit in this test's doc comment, not a regression to fix silently."
        );

        let after = weights_at(&tree, &[]);
        let bits: Vec<u64> = after.iter().map(|w| w.to_bits()).collect();
        assert_eq!(
            bits, before,
            "the weights must be BIT-IDENTICAL after a drag against a zero run, got {after:?}. \
             There is nothing for the fraction to divide, so the head takes `0.8 * 0` and the \
             remainder scales back to zero; anything else here means the zero-tail fallback \
             leaked into a case where the whole run — not just the tail — is empty, and the two \
             panes that vector strands at zero width would have moved without being revived."
        );

        let Some(fraction) = tree.divider_fraction(&[], 2) else {
            panic!("divider_fraction must still answer for a divider that exists, zero run or not");
        };
        assert!(
            approx(fraction, DEFAULT_RATIO),
            "the divider must read back as DEFAULT_RATIO ({DEFAULT_RATIO}), got {fraction}. The \
             0.8 that was requested is NOT recoverable: `split_head_fraction` divides by the \
             slice's total, which is zero, so the fallback is the only answer available. A test \
             that expected 0.8 here would be asserting the drag took effect when it provably did \
             not."
        );
    }

    /// A dragged weight survives [`PaneTree::layout`] →
    /// [`PaneTree::from_layout`] unchanged — the hop `pane_layout::encode` and
    /// `decode` sit on either side of, and therefore the hop every persisted weight
    /// makes.
    ///
    /// The non-equality assertions are what make the round trip mean something: an
    /// EQUAL vector round-trips through any implementation, including one that
    /// re-equalizes at both ends.
    ///
    /// FALSIFYING EDIT: call `equalize` on the split's weights before the
    /// renormalization at the end of `set_divider_fraction`.
    ///
    /// MEASURED: `the drag must have taken effect before the round trip is worth
    /// testing, got [0.3333…, 0.3333…, 0.3333…]` — the first clause, so the "more
    /// than twice its neighbour" one would have fired next. The round trip ITSELF
    /// stays green under that edit: both sides are then equalized and agree
    /// perfectly, which is exactly why the round trip alone would not have been
    /// enough.
    #[test]
    fn dragged_weights_survive_a_layout_round_trip() {
        let mut tree = flat_row_of_three();
        assert!(tree.set_divider_fraction(&[], 0, 0.8));

        let dragged = weights_at(&tree, &[]);
        assert!(
            approx(dragged[0], 0.8),
            "the drag must have taken effect before the round trip is worth testing, got {dragged:?}"
        );
        assert!(
            dragged[0] > dragged[1] * 2.0,
            "the head must be more than twice its neighbour ({dragged:?}); `equalize` and \
             `DEFAULT_RATIO` cannot produce such a vector, which is what makes the round trip \
             below discriminating"
        );

        let mirror = tree.layout();
        let Some(rebuilt) = PaneTree::from_layout(&mirror) else {
            panic!("from_layout rejected a DRAGGED tree, which is well-formed: {mirror:?}");
        };

        assert_eq!(
            rebuilt.layout(),
            mirror,
            "a dragged weight vector must survive layout() -> from_layout() VERBATIM: that pair is \
             what `pane_layout::encode`/`decode` wrap, so a re-equalize here silently turns every \
             saved drag back into an equal share on the next launch"
        );
        assert_eq!(
            rebuilt.pane_rects(W, H),
            tree.pane_rects(W, H),
            "…and the rebuilt tree must tile the bounds identically, which is what proves the \
             WEIGHTS came through and not merely the topology"
        );
        let Some(fraction) = rebuilt.divider_fraction(&[], 0) else {
            panic!(
                "the rebuilt tree reports no divider at (&[], 0), but it is a three-child split"
            );
        };
        assert!(
            approx(fraction, 0.8),
            "the rebuilt tree's divider fraction is {fraction}, not the 0.8 that was dragged"
        );
    }

    /// [`PaneTree::neighbor`] resolves against the DRAGGED geometry, not against
    /// idealized equal shares: `pane_rects` divides by the live weights and
    /// `neighbor` reads its output, so a drag moves focus-direction answers with it.
    ///
    /// Two halves, and only the second can fail on its own:
    ///
    /// - A 2x2 grid with its middle divider dragged far off centre keeps every one
    ///   of its eight relations. A drag must not BREAK neighbour resolution. Its
    ///   two `assert_rect` calls are preconditions on that grid's shape, not
    ///   neighbour claims.
    /// - A row of three above a row of two, where dragging the top row's first
    ///   divider moves the middle pane clear of the bottom-left one. The answer for
    ///   "below the middle pane" CHANGES from the bottom-left pane to the
    ///   bottom-right one, which is the smallest shape in which idealized and
    ///   dragged geometry give different answers at all.
    ///
    /// FALSIFYING EDIT: in [`PaneTree::neighbor`] ONLY, replace the
    /// `self.pane_rects(width, height)` call with an inline even-division rect
    /// walk, leaving the public `pane_rects` correct.
    ///
    /// MEASURED: the only failing test in the whole 124-test suite is this one, and
    /// it fires on half 2 — `after the drag the middle top column no longer
    /// overlaps the bottom-LEFT pane at all, so the pane below it is the
    /// bottom-RIGHT one`, left `Some(4)` (the bottom-LEFT pane), right `Some(5)`.
    ///
    /// The BROADER edit — making `pane_rects` itself ignore its weights — is not
    /// the one to run, because it never reaches the assertion that discriminates.
    /// MEASURED: half 1's rect preconditions fire first (`case P1 top-left, widened
    /// by the drag: got Rect { x: 0.0, y: 0.0, w: 50.0, h: 20.0 }, want (x:0, y:0,
    /// w:80, h:20)`), and several existing tests in this module go red alongside
    /// it. That also corrects the standing description of half 1: what a drag
    /// leaves invariant is its NEIGHBOUR RELATIONS — all eight stayed green under
    /// the `neighbor`-only edit — and NOT its rect preconditions, which read the
    /// weights by construction and move with them, which is why the broader edit
    /// stops there.
    #[test]
    fn neighbor_resolves_against_a_dragged_divider() {
        // --- Half 1: a 2x2 grid whose vertical divider is dragged far right -----
        let mut grid = PaneTree::new(P1);
        grid.split(P1, P2, Orientation::Vertical);
        grid.split(P1, P3, Orientation::Horizontal);
        grid.split(P2, P4, Orientation::Horizontal);
        assert!(grid.set_divider_fraction(&[], 0, 0.8));

        let rects = grid.pane_rects(W, H);
        assert_rect(
            rect_for(&rects, P1),
            0.0,
            0.0,
            0.8 * W,
            HALF_H,
            "P1 top-left, widened by the drag",
        );
        assert_rect(
            rect_for(&rects, P2),
            0.8 * W,
            0.0,
            0.2 * W,
            HALF_H,
            "P2 top-right, narrowed by the drag",
        );

        for (from, dir, want, desc) in [
            (P1, FocusDir::Right, Some(P2), "TL right -> TR"),
            (P1, FocusDir::Down, Some(P3), "TL down -> BL"),
            (P2, FocusDir::Left, Some(P1), "TR left -> TL"),
            (P2, FocusDir::Down, Some(P4), "TR down -> BR"),
            (P3, FocusDir::Right, Some(P4), "BL right -> BR"),
            (P3, FocusDir::Up, Some(P1), "BL up -> TL"),
            (P4, FocusDir::Left, Some(P3), "BR left -> BL"),
            (P4, FocusDir::Up, Some(P2), "BR up -> TR"),
            (P1, FocusDir::Left, None, "TL has no pane to its left"),
            (P4, FocusDir::Down, None, "BR has no pane below"),
        ] {
            assert_eq!(
                grid.neighbor(from, dir, W, H),
                want,
                "case {desc}: a divider dragged to 0.8 must not BREAK neighbour resolution; the \
                 rects are still a clean 2x2 tiling, only an asymmetric one"
            );
        }

        // Half 2: Horizontal[Vertical[P1, P2, P3], Vertical[P4, P5]]. At equal shares the middle column
        // overlaps both bottom panes and ties to DFS-first P4; a 0.6 drag moves it to 60..80, giving P5.
        let mut tree = PaneTree::new(P1);
        tree.split(P1, P4, Orientation::Horizontal);
        tree.split(P1, P2, Orientation::Vertical);
        tree.split(P2, P3, Orientation::Vertical);
        tree.split(P4, P5, Orientation::Vertical);
        assert_eq!(
            tree.leaves(),
            vec![P1, P2, P3, P4, P5],
            "precondition: the five leaves are a row of three over a row of two, in DFS order"
        );
        assert_eq!(
            tree.neighbor(P2, FocusDir::Down, W, H),
            Some(P4),
            "precondition, at EQUAL shares: the middle top column overlaps both bottom panes, and \
             the tie resolves to the DFS-first one. If this already answers P5 the drag below \
             proves nothing."
        );

        assert!(tree.set_divider_fraction(&[0], 0, 0.6));
        let rects = tree.pane_rects(W, H);
        assert_rect(
            rect_for(&rects, P2),
            0.6 * W,
            0.0,
            0.2 * W,
            HALF_H,
            "the middle top column now starts to the RIGHT of the bottom-left pane's right edge",
        );
        assert_rect(
            rect_for(&rects, P4),
            0.0,
            HALF_H,
            HALF_W,
            HALF_H,
            "the bottom row is untouched by a drag inside the top row",
        );
        assert_eq!(
            tree.neighbor(P2, FocusDir::Down, W, H),
            Some(P5),
            "after the drag the middle top column no longer overlaps the bottom-LEFT pane at all, \
             so the pane below it is the bottom-RIGHT one. Answering the bottom-left pane here \
             means `neighbor` resolved against idealized equal shares rather than against the \
             dragged geometry — the behaviour `architecture.md` used to document as a limitation."
        );
    }
}
