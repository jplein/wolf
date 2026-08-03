//! The pure, GTK-free CODEC between the live pane tree and the persisted one.
//!
//! The GTK-side analogue of the macOS `PaneLayoutCodec`: it bridges
//! [`crate::layout`]'s [`PaneLayout`] / [`PaneId`] (whose leaves are pane IDS)
//! and [`wolf_core::PaneNode`] (whose leaves are buffer PATHS, the `state.json`
//! shape). No GTK, no serde, no IO — so every rule here is unit-testable
//! headlessly, exactly like [`crate::layout`] and [`crate::geometry`].
//!
//! ## The single-pane persistence rule (read this before touching anything)
//!
//! A tab's open buffer paths have NO persistence channel of their own: they ride
//! only inside `TabState::layout`'s leaves. The macOS bug this crate must not
//! reintroduce was a save side that suppressed the layout by ARITY —
//! `guard tree.leaves().count > 1 else { return nil }` — so the single most
//! common shape (one window, one tab, one pane, one file) persisted its cwd and
//! nothing else, and the file was not reopened on relaunch.
//!
//! The fix, on both sides, is a CONTENT predicate: [`is_empty_leaf`] suppresses
//! ONLY an unnamed, non-preview lone leaf (the empty-state "no folder" pane,
//! whose `layout: None` is a pre-existing invariant), and the restore side
//! rebuilds ANY saved layout with no arity guard at all. **Never gate persistence
//! on [`leaf_count`]** — it exists solely to check the handle count
//! `wolf_core::WolfCore::rebuild_layout` returns.
//!
//! ## Orientation, in one place
//!
//! [`crate::layout::Orientation::Vertical`] ↔ [`PaneOrientation::Vertical`] (side
//! by side, divides WIDTH) and `Horizontal` ↔ `Horizontal` (stacked, divides
//! HEIGHT). Both directions live here so an inversion is one edit to make and one
//! test to catch, rather than two half-inversions that cancel.

use std::collections::HashMap;

use wolf_core::{PaneNode, PaneOrientation};

use crate::layout::{Orientation, PaneId, PaneLayout, PaneTree};

/// The most leaves [`decode`] will realize from a saved layout.
///
/// A restore POLICY cap, not a tree limit: a corrupt or hand-edited `state.json`
/// must not make the app ask nvim to create hundreds of windows at launch.
pub const MAX_RESTORE_LEAVES: usize = 64;

/// The deepest saved layout [`decode`] will realize.
///
/// Guards the recursive builders (and `pane_view::build_chain`'s widget
/// recursion) against a pathological hand-edited nesting.
pub const MAX_RESTORE_DEPTH: usize = 32;

/// Maps a live [`PaneLayout`] (leaf IDs) into a persistable [`PaneNode`] (leaf
/// PATHS), looking each leaf id up in `paths`.
///
/// A leaf id MISSING from `paths` becomes `path: ""` (macOS parity: an unbound or
/// unnamed pane persists as an empty leaf). `preview` is always `false`:
/// `wolf-gnome` has no Markdown preview yet, so that is a VALUE choice, not a
/// shape choice — the flag is already carried end to end by `PaneNode`,
/// `wolf_core::state::rebuild_layout_spec` and `wolf_rebuild_layout`, so the
/// preview unit will add a value rather than a field.
///
/// Weights and orientation are copied VERBATIM.
#[must_use]
pub fn encode(layout: &PaneLayout, paths: &HashMap<PaneId, String>) -> PaneNode {
    match layout {
        PaneLayout::Leaf(id) => PaneNode::Leaf {
            path: paths.get(id).cloned().unwrap_or_default(),
            preview: false,
        },
        PaneLayout::Split {
            orientation,
            children,
            weights,
        } => PaneNode::Split {
            orientation: to_core_orientation(*orientation),
            weights: weights.clone(),
            children: children.iter().map(|c| encode(c, paths)).collect(),
        },
    }
}

/// The "suppress from persistence" CONTENT predicate: `true` **iff** `node` is
/// `Leaf { path: "", preview: false }`.
///
/// A preview leaf, a leaf with a non-empty path, and EVERY split — including a
/// split of two empty leaves — are `false`. **This is NEVER an arity test**; see
/// the module docs for the bug that arity guard caused.
#[must_use]
pub fn is_empty_leaf(node: &PaneNode) -> bool {
    match node {
        PaneNode::Leaf { path, preview } => path.is_empty() && !preview,
        PaneNode::Split { .. } => false,
    }
}

/// The number of leaves under `node` (`1` for a leaf).
///
/// Used ONLY to validate the handle count `wolf_core::WolfCore::rebuild_layout`
/// returns against the leaves [`decode`] produced. Never a persistence gate —
/// that is [`is_empty_leaf`].
#[must_use]
pub fn leaf_count(node: &PaneNode) -> usize {
    match node {
        PaneNode::Leaf { .. } => 1,
        PaneNode::Split { children, .. } => children.iter().map(leaf_count).sum(),
    }
}

/// The leaf paths under `node` in DFS order — the order
/// `wolf_core::state::rebuild_layout_spec` emits leaves in, and therefore the
/// order `wolf_core::WolfCore::rebuild_layout` returns window handles in.
#[must_use]
pub fn leaf_paths_dfs(node: &PaneNode) -> Vec<String> {
    let mut out = Vec::new();
    collect_leaf_paths(node, &mut out);
    out
}

/// Turns a saved [`PaneNode`] into a live [`PaneTree`] plus its leaf ids in DFS
/// order, so the caller can zip each nvim window handle onto its rebuilt leaf.
///
/// Ids are allocated sequentially from `first_leaf_id` in DFS order, so
/// `leaf_ids[i]` is the pane for `handles[i]` and for
/// `leaf_paths_dfs(node)[i]`.
///
/// Returns `None` when `node` is unusable: any structural violation
/// [`PaneTree::from_layout`] rejects (a split with fewer than two children, a
/// weights/children length mismatch, a non-finite or negative weight, a
/// zero-summing weight vector), a leaf count above [`MAX_RESTORE_LEAVES`], or a
/// depth above [`MAX_RESTORE_DEPTH`]. Validating BEFORE the spec is rendered is
/// what keeps nvim from ever being asked to realize a malformed tree.
///
/// Builds VERBATIM otherwise — no flatten, no collapse, no re-equalize — because
/// reshaping would desynchronise the DFS zip against handles nvim has already
/// returned.
#[must_use]
pub fn decode(node: &PaneNode, first_leaf_id: PaneId) -> Option<(PaneTree, Vec<PaneId>)> {
    if leaf_count(node) > MAX_RESTORE_LEAVES || depth(node) > MAX_RESTORE_DEPTH {
        return None;
    }
    let mut next_id = first_leaf_id;
    let mut leaf_ids = Vec::new();
    let layout = to_layout(node, &mut next_id, &mut leaf_ids);
    let tree = PaneTree::from_layout(&layout)?;
    Some((tree, leaf_ids))
}

/// [`crate::layout::Orientation`] → [`PaneOrientation`] (the persisted twin).
fn to_core_orientation(orientation: Orientation) -> PaneOrientation {
    match orientation {
        Orientation::Horizontal => PaneOrientation::Horizontal,
        Orientation::Vertical => PaneOrientation::Vertical,
    }
}

/// [`PaneOrientation`] → [`crate::layout::Orientation`] (the live twin).
fn from_core_orientation(orientation: PaneOrientation) -> Orientation {
    match orientation {
        PaneOrientation::Horizontal => Orientation::Horizontal,
        PaneOrientation::Vertical => Orientation::Vertical,
    }
}

/// Appends every leaf path under `node` to `out` in DFS order.
fn collect_leaf_paths(node: &PaneNode, out: &mut Vec<String>) {
    match node {
        PaneNode::Leaf { path, .. } => out.push(path.clone()),
        PaneNode::Split { children, .. } => {
            for child in children {
                collect_leaf_paths(child, out);
            }
        }
    }
}

/// The node depth of `node` (`1` for a leaf), for the [`MAX_RESTORE_DEPTH`] cap.
///
/// Recursing here is safe on untrusted input because `serde_json`'s own
/// deserialization recursion limit (128) rejects anything deeper long before
/// `load_state` hands a tree to this module.
fn depth(node: &PaneNode) -> usize {
    match node {
        PaneNode::Leaf { .. } => 1,
        PaneNode::Split { children, .. } => 1 + children.iter().map(depth).max().unwrap_or(0),
    }
}

/// Recursively mirrors `node` into a [`PaneLayout`], allocating a fresh
/// [`PaneId`] for each leaf in DFS order and recording it in `leaf_ids`.
fn to_layout(node: &PaneNode, next_id: &mut PaneId, leaf_ids: &mut Vec<PaneId>) -> PaneLayout {
    match node {
        PaneNode::Leaf { .. } => {
            let id = *next_id;
            *next_id += 1;
            leaf_ids.push(id);
            PaneLayout::Leaf(id)
        }
        PaneNode::Split {
            orientation,
            weights,
            children,
        } => PaneLayout::Split {
            orientation: from_core_orientation(*orientation),
            children: children
                .iter()
                .map(|c| to_layout(c, next_id, leaf_ids))
                .collect(),
            weights: weights.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Pane ids (contiguous, so a decode from the same first id round-trips) ---
    /// First leaf id used by the round-trip tests; also the `first_leaf_id` handed
    /// to [`decode`], so the rebuilt ids match the originals exactly.
    const P0: PaneId = 10;
    /// Second leaf id, DFS order.
    const P1: PaneId = 11;
    /// Third leaf id, DFS order.
    const P2: PaneId = 12;

    // --- Buffer paths -----------------------------------------------------------
    /// The DFS-first leaf's buffer path.
    const PATH_A: &str = "/proj/a.md";
    /// The DFS-second leaf's buffer path.
    const PATH_B: &str = "/proj/b.md";
    /// The DFS-third leaf's buffer path.
    const PATH_C: &str = "/proj/c.md";

    // --- Weights ---------------------------------------------------------------
    //
    // NON-EQUAL on purpose. `PaneTree::split` and `remove` always equalize, so an
    // equal-weight round trip could not tell "the saved weights were preserved" from
    // "they were recomputed to the same numbers". These are hand-written literals,
    // which keeps the codec's round trip independent of the one thing in this crate
    // that produces a non-equal vector — `PaneTree::set_divider_fraction`, exercised
    // against the codec by `encode_carries_a_dragged_weight_vector` and
    // `decode_restores_a_dragged_weight_vector`.
    /// The DFS-first child's share in the non-equal round-trip tree.
    const WEIGHT_SMALL: f64 = 0.25;
    /// The DFS-second child's share in the non-equal round-trip tree.
    const WEIGHT_LARGE: f64 = 0.75;
    /// An equal share, for the nested split whose weights are not the subject.
    const WEIGHT_HALF: f64 = 0.5;

    /// How many leaves [`asymmetric_tree`] has, as a [`PaneId`] so the contiguous-id
    /// range can be built from it without a cast.
    const ASYMMETRIC_LEAVES: PaneId = 3;

    /// A non-preview leaf bound to `path`.
    fn leaf(path: &str) -> PaneNode {
        PaneNode::Leaf {
            path: path.to_string(),
            preview: false,
        }
    }

    /// A PREVIEW leaf bound to `path` (never produced on Linux yet; the predicate
    /// tests need one to prove `is_empty_leaf` is not just "the path is empty").
    fn preview_leaf(path: &str) -> PaneNode {
        PaneNode::Leaf {
            path: path.to_string(),
            preview: true,
        }
    }

    /// A [`PaneNode`] split along `orientation`.
    fn split(orientation: PaneOrientation, weights: &[f64], children: Vec<PaneNode>) -> PaneNode {
        PaneNode::Split {
            orientation,
            weights: weights.to_vec(),
            children,
        }
    }

    /// The exact tree the S1 relaunch smoke builds:
    /// `Vertical[ Horizontal[a, b], c ]` — a left COLUMN of two beside a full-height
    /// right pane. DFS leaves are `[a, b, c]`.
    fn asymmetric_tree() -> PaneNode {
        split(
            PaneOrientation::Vertical,
            &[WEIGHT_SMALL, WEIGHT_LARGE],
            vec![
                split(
                    PaneOrientation::Horizontal,
                    &[WEIGHT_HALF, WEIGHT_HALF],
                    vec![leaf(PATH_A), leaf(PATH_B)],
                ),
                leaf(PATH_C),
            ],
        )
    }

    /// The same shape as [`asymmetric_tree`] in [`PaneLayout`] terms, with leaf ids
    /// [`P0`], [`P1`], [`P2`] in DFS order.
    fn asymmetric_layout() -> PaneLayout {
        PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![
                PaneLayout::Split {
                    orientation: Orientation::Horizontal,
                    children: vec![PaneLayout::Leaf(P0), PaneLayout::Leaf(P1)],
                    weights: vec![WEIGHT_HALF, WEIGHT_HALF],
                },
                PaneLayout::Leaf(P2),
            ],
            weights: vec![WEIGHT_SMALL, WEIGHT_LARGE],
        }
    }

    /// A nested chain of two-child splits exactly `depth` nodes deep (`depth == 1`
    /// is a bare leaf), for the [`MAX_RESTORE_DEPTH`] boundary rows. Its leaf count
    /// is also `depth`, which stays well under [`MAX_RESTORE_LEAVES`] at the depths
    /// tested, so the two caps are exercised independently.
    fn nested_chain(depth: usize) -> PaneNode {
        if depth <= 1 {
            return leaf(PATH_A);
        }
        split(
            PaneOrientation::Vertical,
            &[WEIGHT_HALF, WEIGHT_HALF],
            vec![leaf(PATH_A), nested_chain(depth - 1)],
        )
    }

    /// A FLAT split of `count` equally-weighted empty leaves, for the
    /// [`MAX_RESTORE_LEAVES`] boundary rows.
    fn flat_split(count: usize) -> PaneNode {
        split(
            PaneOrientation::Vertical,
            &vec![WEIGHT_HALF; count],
            (0..count).map(|_| leaf(PATH_A)).collect(),
        )
    }

    /// The weights of a split node, or a panic naming what was found instead.
    fn weights_of(node: &PaneLayout) -> Vec<f64> {
        match node {
            PaneLayout::Split { weights, .. } => weights.clone(),
            PaneLayout::Leaf(id) => panic!("expected a split, found leaf {id}"),
        }
    }

    // =====================================================================
    // encode
    // =====================================================================

    #[test]
    fn encode_maps_ids_to_paths() {
        let mut paths = HashMap::new();
        paths.insert(P0, PATH_A.to_string());
        // P1 is DELIBERATELY absent: an unbound / unnamed pane must encode as an
        // empty leaf rather than being dropped from the tree (macOS parity), because
        // dropping it would change the topology and desynchronise the DFS zip.
        paths.insert(P2, PATH_C.to_string());

        let got = encode(&asymmetric_layout(), &paths);

        // The WHOLE value in one comparison: mapped id -> its path, unmapped id ->
        // "", both orientations, both weight vectors verbatim, and `preview: false`
        // on every leaf.
        assert_eq!(
            got,
            split(
                PaneOrientation::Vertical,
                &[WEIGHT_SMALL, WEIGHT_LARGE],
                vec![
                    split(
                        PaneOrientation::Horizontal,
                        &[WEIGHT_HALF, WEIGHT_HALF],
                        vec![leaf(PATH_A), leaf("")],
                    ),
                    leaf(PATH_C),
                ],
            ),
            "encode must map each leaf id through `paths` (an ABSENT id becoming the empty leaf \
             `\"\"`), copy both orientations and both weight vectors verbatim, and set \
             `preview: false` on every leaf"
        );
    }

    // =====================================================================
    // the CONTENT predicate — the single-pane fix
    // =====================================================================

    /// One [`is_empty_leaf`] table row.
    struct EmptyLeafCase {
        desc: &'static str,
        node: PaneNode,
        want: bool,
    }

    #[test]
    fn is_empty_leaf_table() {
        let cases = [
            EmptyLeafCase {
                desc: "an unnamed, non-preview lone leaf is the ONE suppressed shape",
                node: leaf(""),
                want: true,
            },
            EmptyLeafCase {
                desc: "an unnamed PREVIEW leaf still carries state and is not suppressed",
                node: preview_leaf(""),
                want: false,
            },
            EmptyLeafCase {
                desc: "a NAMED lone leaf is not suppressed (this is the single-pane fix)",
                node: leaf(PATH_A),
                want: false,
            },
            EmptyLeafCase {
                desc: "a named preview leaf is not suppressed",
                node: preview_leaf(PATH_A),
                want: false,
            },
            EmptyLeafCase {
                // THE ANTI-ARITY ROW. Two leaves, both empty: an arity test
                // (`leaf_count > 1`) and a content test agree on `false` here only by
                // accident, but a "the tree carries nothing" test would say `true` and
                // silently drop a real two-pane topology.
                desc: "a SPLIT of two empty leaves is NOT an empty leaf (every split is false)",
                node: split(
                    PaneOrientation::Vertical,
                    &[WEIGHT_HALF, WEIGHT_HALF],
                    vec![leaf(""), leaf("")],
                ),
                want: false,
            },
        ];

        for case in &cases {
            assert_eq!(
                is_empty_leaf(&case.node),
                case.want,
                "case: {} (node = {:?})",
                case.desc,
                case.node
            );
        }
    }

    #[test]
    fn arity_is_never_the_predicate() {
        // The headless twin of the single-pane regression. Stated on its own so the
        // failure NAMES the bug rather than pointing at a table row.
        let lone_named = leaf(PATH_A);
        assert_eq!(
            leaf_count(&lone_named),
            1,
            "precondition: a lone named leaf really is at leaf count 1, which is the arity an \
             arity guard would suppress"
        );
        assert!(
            !is_empty_leaf(&lone_named),
            "a LONE leaf holding a real file ({PATH_A}) must NOT be suppressed from persistence. \
             This is the exact macOS bug `fix/state-persistence-single-pane-bug` fixed: the save \
             side gated on `tree.leaves().count > 1`, so the single most common shape — one \
             window, one tab, one pane, one file — persisted its cwd and NOTHING ELSE, and the \
             file was not reopened on relaunch (a tab's open buffer paths ride ONLY inside \
             `TabState::layout`'s leaves). The predicate must be CONTENT (`is_empty_leaf`), never \
             `leaf_count(..) > 1`."
        );

        let two_empty = split(
            PaneOrientation::Vertical,
            &[WEIGHT_HALF, WEIGHT_HALF],
            vec![leaf(""), leaf("")],
        );
        assert_eq!(
            leaf_count(&two_empty),
            2,
            "precondition: the split of two empty leaves really is at leaf count 2"
        );
        assert!(
            !is_empty_leaf(&two_empty),
            "EVERY split is `false`, including a split of two empty leaves: the TOPOLOGY is state \
             worth persisting even when neither pane holds a file. `is_empty_leaf` must match on \
             the node kind and the leaf's own content, never on how many leaves are underneath."
        );
    }

    // =====================================================================
    // DFS order — the property the whole restore zip rests on
    // =====================================================================

    #[test]
    fn leaf_paths_dfs_matches_the_spec_order() {
        assert_eq!(
            leaf_paths_dfs(&asymmetric_tree()),
            vec![PATH_A.to_string(), PATH_B.to_string(), PATH_C.to_string()],
            "leaves must come out in DFS child-0-first order over `Vertical[Horizontal[a, b], c]` \
             — the SAME order `wolf_core::state::rebuild_layout_spec` emits them in and therefore \
             the order `WolfCore::rebuild_layout` returns window handles in. An inversion here \
             binds each handle to the wrong pane while every count-based assertion stays green."
        );

        assert_eq!(
            leaf_count(&asymmetric_tree()),
            3,
            "the S1 tree has exactly three leaves, which is the handle count the restore path \
             checks `rebuild_layout`'s result against"
        );
    }

    #[test]
    fn decode_preserves_leaf_order_for_an_asymmetric_tree() {
        let node = asymmetric_tree();
        let Some((tree, leaf_ids)) = decode(&node, P0) else {
            panic!("decode rejected the S1 asymmetric tree {node:?}, which is well-formed");
        };

        assert_eq!(
            tree.leaves(),
            leaf_ids,
            "the ids `decode` reports must be exactly `PaneTree::leaves()`, in the same order: the \
             restore path zips `rebuild_layout`'s handles onto `leaf_ids` BY INDEX and then binds \
             each id's `DrawingArea`, so a disagreement here paints each pane with another pane's \
             grid"
        );
        assert_eq!(
            leaf_ids,
            vec![P0, P1, P2],
            "DFS order over `Vertical[Horizontal[a, b], c]` is [left-top, left-bottom, right]"
        );
        assert_eq!(
            tree.layout(),
            asymmetric_layout(),
            "`decode` must rebuild the tree VERBATIM — no flatten, no collapse, no re-equalize — \
             because any reshaping desynchronises the DFS zip against handles nvim has already \
             returned"
        );
    }

    #[test]
    fn decode_allocates_contiguous_ids_from_first_leaf_id() {
        // `pane_view::apply_layout_rebuild` sets `next_pane_id = first_leaf_id +
        // leaf_ids.len()` after a successful decode. That arithmetic is only correct
        // if `decode` allocates CONTIGUOUSLY starting at `first_leaf_id`.
        const FIRST_IDS: [PaneId; 4] = [0, 1, 7, 4096];

        for first in FIRST_IDS {
            let node = asymmetric_tree();
            let Some((_tree, leaf_ids)) = decode(&node, first) else {
                panic!("decode rejected the well-formed S1 tree at first_leaf_id {first}");
            };
            let want: Vec<PaneId> = (first..first + ASYMMETRIC_LEAVES).collect();
            assert_eq!(
                leaf_ids, want,
                "`decode(node, {first})` must allocate CONTIGUOUS ids starting at {first}. \
                 `pane_view::apply_layout_rebuild` advances the allocator with `next_pane_id = \
                 first_leaf_id + leaf_ids.len()`, so a gap or a different starting point makes a \
                 LATER `alloc_pane_id` hand out an id this decode already used — two panes would \
                 then share one `DrawingArea` slot in the `HashMap<PaneId, PaneWidgets>`, and one \
                 of them would silently stop painting."
            );
        }

        // The single-leaf case is the one the S2 smoke restores, and it is where an
        // off-by-one would be least visible.
        let Some((_tree, leaf_ids)) = decode(&leaf(PATH_A), P0) else {
            panic!("decode rejected a lone named leaf, which is the single-pane restore shape");
        };
        assert_eq!(
            leaf_ids,
            vec![P0],
            "a lone leaf must take `first_leaf_id` itself, so `next_pane_id` advances by exactly 1"
        );
    }

    // =====================================================================
    // round trip, with NON-EQUAL weights
    // =====================================================================

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "every expected weight (0.25 / 0.75 / 0.5) is exactly representable in binary \
                  f64 and is copied through encode/decode unmodified rather than computed with, \
                  so an epsilon comparison would weaken the assertion without making it more \
                  correct — the `mouse::scroll_threshold_table` precedent"
    )]
    fn decode_round_trips_encode() {
        // A two-leaf tree whose weights are NOT equal shares, written as literals
        // rather than dragged, so this asserts the CODEC alone: whatever vector goes
        // in comes back out. The end-to-end path — a real drag reaching `state.json`
        // and being reapplied on the next launch — is
        // `dragged_divider_weights_survive_relaunch_under_cage`.
        let layout = PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![PaneLayout::Leaf(P0), PaneLayout::Leaf(P1)],
            weights: vec![WEIGHT_SMALL, WEIGHT_LARGE],
        };
        let mut paths = HashMap::new();
        paths.insert(P0, PATH_A.to_string());
        paths.insert(P1, PATH_B.to_string());

        let node = encode(&layout, &paths);
        let Some((tree, leaf_ids)) = decode(&node, P0) else {
            panic!("decode rejected the encoding of a well-formed two-leaf tree: {node:?}");
        };

        assert_eq!(
            tree.layout(),
            layout,
            "encode -> decode -> PaneTree::layout() must reproduce the ORIGINAL shape (ids \
             included, because decode was handed the original first id)"
        );
        assert_eq!(
            weights_of(&tree.layout()),
            vec![WEIGHT_SMALL, WEIGHT_LARGE],
            "the NON-EQUAL saved weights must survive the round trip verbatim. Equal shares are \
             the only thing wolf-gnome produces today, so an equal-weight round trip could not \
             distinguish `the saved weights were preserved` from `they were recomputed to the \
             same numbers`; these two can."
        );
        assert_eq!(
            leaf_ids,
            vec![P0, P1],
            "leaf_ids must line up index-for-index with the original leaves in DFS order"
        );
        assert_eq!(
            leaf_paths_dfs(&node),
            vec![PATH_A.to_string(), PATH_B.to_string()],
            "and index-for-index with the leaf PATHS, which is what makes `leaf_ids[i]` the pane \
             for `handles[i]`"
        );

        // The same round trip over the three-leaf asymmetric tree, whose nested split
        // carries its own (equal) weights beneath a non-equal outer pair.
        let mut tri_paths = HashMap::new();
        tri_paths.insert(P0, PATH_A.to_string());
        tri_paths.insert(P1, PATH_B.to_string());
        tri_paths.insert(P2, PATH_C.to_string());
        let tri_node = encode(&asymmetric_layout(), &tri_paths);
        assert_eq!(
            tri_node,
            asymmetric_tree(),
            "encoding the asymmetric layout must produce exactly the asymmetric PaneNode tree"
        );
        let Some((tri_tree, tri_ids)) = decode(&tri_node, P0) else {
            panic!("decode rejected the encoding of the asymmetric tree: {tri_node:?}");
        };
        assert_eq!(
            tri_tree.layout(),
            asymmetric_layout(),
            "the three-leaf round trip must also reproduce the original shape and weights"
        );
        assert_eq!(tri_ids, vec![P0, P1, P2], "three leaves, DFS order");
    }

    // =====================================================================
    // rejection — a CRASH GUARD, not tidiness
    // =====================================================================

    /// One [`decode`] rejection row.
    struct MalformedCase {
        desc: &'static str,
        node: PaneNode,
    }

    /// `decode` must reject every structurally impossible saved tree BEFORE the spec
    /// is rendered, so nvim is never asked to realize one.
    ///
    /// This is a crash guard rather than tidiness: `pane_view::build_chain` indexes
    /// `children[0]` unconditionally, so a ZERO-CHILD split reaching the widget layer
    /// is a **panic**, and `layout::pane_rects` divides by the weight sum, so an
    /// all-zero vector is a division by zero. Every input here is reachable from a
    /// hand-edited or corrupted `state.json`, which `load_state` deserializes without
    /// any structural validation of its own.
    #[test]
    fn decode_rejects_malformed() {
        // MAX_RESTORE_LEAVES + 1 and MAX_RESTORE_DEPTH + 1, named so the two boundary
        // rows below cannot drift from the consts they are testing.
        let too_many_leaves = MAX_RESTORE_LEAVES + 1;
        let too_deep = MAX_RESTORE_DEPTH + 1;

        let cases = [
            MalformedCase {
                desc: "a ZERO-child split (build_chain would index children[0] and PANIC)",
                node: split(PaneOrientation::Vertical, &[], vec![]),
            },
            MalformedCase {
                desc: "a ONE-child split (not a split at all; the tree type forbids it)",
                node: split(
                    PaneOrientation::Vertical,
                    &[WEIGHT_HALF],
                    vec![leaf(PATH_A)],
                ),
            },
            MalformedCase {
                desc: "more weights than children",
                node: split(
                    PaneOrientation::Vertical,
                    &[WEIGHT_HALF, WEIGHT_HALF, WEIGHT_HALF],
                    vec![leaf(PATH_A), leaf(PATH_B)],
                ),
            },
            MalformedCase {
                desc: "fewer weights than children",
                node: split(
                    PaneOrientation::Vertical,
                    &[WEIGHT_HALF],
                    vec![leaf(PATH_A), leaf(PATH_B)],
                ),
            },
            MalformedCase {
                desc: "a NaN weight (would poison every cumulative rect fraction)",
                node: split(
                    PaneOrientation::Vertical,
                    &[f64::NAN, WEIGHT_HALF],
                    vec![leaf(PATH_A), leaf(PATH_B)],
                ),
            },
            MalformedCase {
                desc: "an infinite weight",
                node: split(
                    PaneOrientation::Vertical,
                    &[f64::INFINITY, WEIGHT_HALF],
                    vec![leaf(PATH_A), leaf(PATH_B)],
                ),
            },
            MalformedCase {
                desc: "a NEGATIVE weight (would place a pane outside its parent's bounds)",
                node: split(
                    PaneOrientation::Vertical,
                    &[-WEIGHT_HALF, WEIGHT_HALF],
                    vec![leaf(PATH_A), leaf(PATH_B)],
                ),
            },
            MalformedCase {
                desc: "an ALL-ZERO weight vector (pane_rects would divide by zero)",
                node: split(
                    PaneOrientation::Vertical,
                    &[0.0, 0.0],
                    vec![leaf(PATH_A), leaf(PATH_B)],
                ),
            },
            MalformedCase {
                desc: "a violation NESTED under a well-formed root (validation must recurse)",
                node: split(
                    PaneOrientation::Vertical,
                    &[WEIGHT_HALF, WEIGHT_HALF],
                    vec![
                        leaf(PATH_A),
                        split(PaneOrientation::Horizontal, &[], vec![]),
                    ],
                ),
            },
            MalformedCase {
                desc: "MAX_RESTORE_LEAVES + 1 leaves (a restore POLICY cap, not a tree limit)",
                node: flat_split(too_many_leaves),
            },
            MalformedCase {
                desc: "MAX_RESTORE_DEPTH + 1 deep (guards the recursive builders' stack)",
                node: nested_chain(too_deep),
            },
        ];

        for case in &cases {
            assert!(
                decode(&case.node, P0).is_none(),
                "decode must REJECT {} — a hand-edited or corrupted state.json can produce it, and \
                 `load_state` performs no structural validation of its own. Rejecting BEFORE the \
                 spec is rendered is what keeps nvim from being asked to realize it. Node was: \
                 {:?}",
                case.desc,
                case.node
            );
        }

        // The caps must be non-vacuous: exactly AT each limit the tree is accepted, so
        // a `> MAX` that silently became `>= MAX` (or a cap set to 0) is caught here
        // rather than being mistaken for correct rejection above.
        assert!(
            decode(&flat_split(MAX_RESTORE_LEAVES), P0).is_some(),
            "a tree with exactly MAX_RESTORE_LEAVES ({MAX_RESTORE_LEAVES}) leaves must be \
             ACCEPTED; without this the rejection row above would also pass with the cap \
             mistakenly tightened to `>=`, or set to zero"
        );
        assert!(
            decode(&nested_chain(MAX_RESTORE_DEPTH), P0).is_some(),
            "a tree exactly MAX_RESTORE_DEPTH ({MAX_RESTORE_DEPTH}) deep must be ACCEPTED, for the \
             same reason"
        );
    }

    // =====================================================================
    // a DRAGGED weight vector, through the codec both ways
    //
    // The pure twins of the divider-relaunch smoke's first two assertions. The
    // weights here are not hand-written: they are produced by
    // `PaneTree::set_divider_fraction`, the same call a `gtk::Paned` drag makes, so
    // these two tests cover the codec against the exact values the front-end can
    // really put on disk.
    // =====================================================================

    /// The head fraction the drag tests aim at. Not an equal share, and not a value
    /// `PaneTree::split` or `remove` can produce, so seeing it survive a codec hop
    /// proves the weights were CARRIED rather than recomputed.
    const DRAGGED_HEAD_FRACTION: f64 = 0.7;

    /// Tolerance for a weight the drag COMPUTED (as opposed to one copied through
    /// unmodified). `set_divider_fraction` scales and renormalizes, so the stored
    /// value is `0.7` only to within float noise.
    const WEIGHT_EPS: f64 = 1e-9;

    /// The smallest gap between the two dragged weights that no equal-share vector
    /// could show. `[0.5, 0.5]` has a gap of exactly zero.
    const MIN_DRAGGED_WEIGHT_GAP: f64 = 0.1;

    /// A two-leaf vertical split whose divider has been dragged to
    /// [`DRAGGED_HEAD_FRACTION`] — the only way anything in this crate can produce a
    /// non-equal weight vector.
    fn dragged_two_leaf_tree() -> PaneTree {
        let mut tree = PaneTree::new(P0);
        tree.split(P0, P1, Orientation::Vertical);
        assert!(
            tree.set_divider_fraction(&[], 0, DRAGGED_HEAD_FRACTION),
            "precondition: a finite fraction on a real divider must apply, or nothing below is \
             testing a dragged weight at all"
        );
        tree
    }

    /// The `PaneId` -> path map the drag tests encode with.
    fn dragged_paths() -> HashMap<PaneId, String> {
        let mut paths = HashMap::new();
        paths.insert(P0, PATH_A.to_string());
        paths.insert(P1, PATH_B.to_string());
        paths
    }

    /// [`encode`] carries a DRAGGED weight vector onto the persisted node verbatim.
    ///
    /// The pure twin of the divider-relaunch smoke's save-side assertion. That smoke
    /// reads the same property off a real `state.json`; this one is what says WHERE
    /// to look when it goes red.
    ///
    /// FALSIFYING EDIT: in [`encode`], replace `weights: weights.clone()` with
    /// `vec![0.5; children.len()]`.
    ///
    /// MEASURED: the persisted-weights assertion fires with left `[0.5, 0.5]` and
    /// right `[0.7, 0.30000000000000004]` — the live tree's own vector, which is
    /// what makes the exact comparison the right one here.
    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "the expected weights are the LIVE tree's own weights, which `encode` is required \
                  to copy through unmodified — an epsilon comparison would weaken the assertion \
                  without making it more correct, since its whole point is that the two vectors are \
                  the SAME numbers (the `decode_round_trips_encode` precedent)"
    )]
    fn encode_carries_a_dragged_weight_vector() {
        let tree = dragged_two_leaf_tree();
        let live = weights_of(&tree.layout());

        // Non-vacuity: an equal-share vector would round-trip through an `encode`
        // that ignored its input entirely.
        assert!(
            (live[0] - live[1]).abs() > MIN_DRAGGED_WEIGHT_GAP,
            "precondition: the drag must leave a visibly NON-EQUAL pair, got {live:?}"
        );
        assert!(
            (live[0] - DRAGGED_HEAD_FRACTION).abs() < WEIGHT_EPS,
            "precondition: the dragged head weight should be {DRAGGED_HEAD_FRACTION}, got {live:?}"
        );

        let node = encode(&tree.layout(), &dragged_paths());

        let PaneNode::Split {
            orientation,
            weights,
            children,
        } = &node
        else {
            panic!("encode turned a two-leaf split into {node:?}, which is not a split at all");
        };
        assert_eq!(
            *orientation,
            PaneOrientation::Vertical,
            "a side-by-side split must persist as PaneOrientation::Vertical"
        );
        assert_eq!(
            weights.as_slice(),
            live.as_slice(),
            "encode must copy the DRAGGED weights onto the persisted node verbatim. Equal shares \
             are what every other code path in this crate produces, so a `[0.5, 0.5]` here is \
             indistinguishable from `the weights were recomputed` — which is precisely the \
             confusion these two tests exist to remove."
        );
        assert_eq!(
            children.len(),
            2,
            "the topology must survive alongside the weights: {node:?}"
        );
    }

    /// [`decode`] restores a DRAGGED weight vector, as the fraction the widget layer
    /// will position the `gtk::Paned` at.
    ///
    /// The pure twin of the divider-relaunch smoke's restore-side assertion. It
    /// asserts through `PaneTree::divider_fraction` rather than on the raw weights
    /// on purpose: that is the call `install_paned_position` makes on every
    /// `max-position` notify, so it is the value a restored divider is really placed
    /// from.
    ///
    /// FALSIFYING EDIT: in `to_layout`, replace `weights: weights.clone()` with an
    /// equalized vector.
    ///
    /// MEASURED: `the restored divider fraction is 0.5, not the dragged 0.7`.
    #[test]
    fn decode_restores_a_dragged_weight_vector() {
        let node = encode(&dragged_two_leaf_tree().layout(), &dragged_paths());

        let Some((rebuilt, leaf_ids)) = decode(&node, P0) else {
            panic!("decode rejected the encoding of a dragged two-leaf split: {node:?}");
        };
        assert_eq!(
            leaf_ids,
            vec![P0, P1],
            "the two leaves must still come back in DFS order"
        );

        let Some(fraction) = rebuilt.divider_fraction(&[], 0) else {
            panic!(
                "the restored tree reports NO divider at (&[], 0), but it is a two-child split: \
                 {node:?}"
            );
        };
        assert!(
            (fraction - DRAGGED_HEAD_FRACTION).abs() < WEIGHT_EPS,
            "the restored divider fraction is {fraction}, not the dragged \
             {DRAGGED_HEAD_FRACTION}. `install_paned_position` reads exactly this value on every \
             `max-position` notify, so a `0.5` here means every relaunch silently re-centres the \
             user's dividers while the save side looks perfectly correct."
        );
    }
}
