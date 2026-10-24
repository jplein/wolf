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
//! ## The all-or-nothing resolution rule
//!
//! [`resolve_leaves`] sits one step ahead of [`encode`] and answers the other
//! half of the same question: an empty PATH is a resolved answer (a genuinely
//! unnamed buffer), while an absent WINDOW is not, so a single unresolvable leaf
//! refuses the whole tab's gather rather than being written as `""`. That refusal
//! is what makes the silent-loss shape — a well-formed multi-leaf tree whose paths
//! are all empty, which [`is_empty_leaf`] never suppresses — unreachable.
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

/// What one live pane contributes to a persisted leaf: the buffer path it holds
/// and whether that buffer is a Markdown PREVIEW rather than an editable file.
///
/// The two travel together because [`is_empty_leaf`] reads both, and a caller that
/// carried only the path would have to re-derive the flag at the encode site —
/// where the pane it belongs to is no longer in hand.
///
/// [`Default`] is the "no such pane" value, which is what makes a leaf id missing
/// from [`encode`]'s map an empty, non-preview leaf without a second lookup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeafContent {
    /// The pane's RAW absolute buffer path (`""` for an unbound or unnamed pane).
    pub path: String,
    /// Whether the pane shows a Markdown preview.
    pub preview: bool,
}

/// What ONE leaf's Neovim window resolves to in the live snapshot.
///
/// The input half of [`resolve_leaves`], and the reason a background tab's gather
/// is trustworthy: every variant is read from a map that is GLOBAL across
/// tabpages, keyed by the pane's own `PaneWidgets::win`, so a tab Neovim is not
/// currently on answers exactly what a foreground one would.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WinContent {
    /// The window is a Markdown PREVIEW of the carried source path
    /// (`Snapshot::previews[win]`).
    Preview(String),
    /// The window holds an ordinary buffer at the carried path
    /// (`Snapshot::win_buffers[win]`), `""` for a genuinely unnamed one.
    Buffer(String),
    /// The window is in NEITHER map, or the leaf has no live pane at all.
    Unknown,
}

/// One tab's gather answer, as [`crate::pane_view::PaneHandle::layout_snapshot`]
/// reports it.
///
/// Two refusals with different meanings would be one `Option` too few:
/// [`LayoutGather::Ready`]`(None)` is [`crate::state_seam::persistable_layout`]'s
/// CONTENT suppression, a real answer that says "this tab holds an empty-state lone
/// pane"; [`LayoutGather::Refuse`] says the tab could not be read at all and its
/// caller must contribute the CACHE.
#[derive(Clone, Debug, PartialEq)]
pub enum LayoutGather {
    /// The tab was read. The payload is [`crate::state_seam::persistable_layout`]'s
    /// answer, so `None` is content suppression rather than failure.
    Ready(Option<PaneNode>),
    /// At least one leaf was unresolvable, so the whole tab's gather is refused and
    /// the caller contributes its last good subtree instead.
    Refuse,
}

/// Resolves every leaf's [`WinContent`] into the [`LeafContent`] [`encode`] reads,
/// or `None` if ANY leaf is unresolvable.
///
/// Pure, and the whole of the all-or-nothing rule:
///
/// - [`WinContent::Preview`]`(p)` → `{path: p, preview: true}`;
/// - [`WinContent::Buffer`]`(p)` → `{path: p, preview: false}`;
/// - **any** [`WinContent::Unknown`] collapses the WHOLE answer to `None`.
///
/// **An empty path is a RESOLVED answer and an absent window is not.** A `""` from
/// the buffer arm is a real unnamed buffer — `:enew`, the empty-state pane — and
/// refusing on it would stop persisting every tab holding a scratch buffer.
/// Mapping the absent window onto that same `""` is the opposite error and a far
/// worse one: it is the exact shape that writes a well-formed `state.json` over the
/// user's files, because [`is_empty_leaf`] suppresses only a LONE empty leaf, so a
/// multi-leaf tree of empty leaves is persisted verbatim and read back on the next
/// launch as a genuine topology of blank panes. No arity guard anywhere can catch
/// that; the refusal here, at the only point where "empty" and "absent" are still
/// distinguishable, is what makes it unreachable.
///
/// An empty `leaves` slice resolves to an empty map: nothing is unresolvable, so
/// there is nothing to refuse. [`encode`] then answers its missing-id fallback for
/// every leaf, which the caller's own suppression rule handles as before.
#[must_use]
pub fn resolve_leaves(leaves: &[(PaneId, WinContent)]) -> Option<HashMap<PaneId, LeafContent>> {
    leaves
        .iter()
        .map(|(id, content)| {
            let content = match content {
                WinContent::Preview(path) => LeafContent {
                    path: path.clone(),
                    preview: true,
                },
                WinContent::Buffer(path) => LeafContent {
                    path: path.clone(),
                    preview: false,
                },
                WinContent::Unknown => return None,
            };
            Some((*id, content))
        })
        .collect()
}

/// Maps a live [`PaneLayout`] (leaf IDs) into a persistable [`PaneNode`] (leaf
/// PATHS), looking each leaf id up in `contents`.
///
/// A leaf id MISSING from `contents` becomes `Leaf { path: "", preview: false }`
/// (macOS parity: an unbound or unnamed pane persists as an empty leaf) rather than
/// being dropped, because dropping it would change the topology and desynchronise
/// the DFS zip the restore path depends on. That fallback is DEFENSIVE: the gather
/// path builds `contents` from [`resolve_leaves`], which answers one entry per leaf
/// or refuses outright, so a missing id cannot reach here from it.
///
/// Weights and orientation are copied VERBATIM.
#[must_use]
pub fn encode(layout: &PaneLayout, contents: &HashMap<PaneId, LeafContent>) -> PaneNode {
    match layout {
        PaneLayout::Leaf(id) => {
            // `LeafContent::default()` IS the "no such pane" value, which is why a
            // missing id needs no second branch.
            let content = contents.get(id).cloned().unwrap_or_default();
            PaneNode::Leaf {
                path: content.path,
                preview: content.preview,
            }
        }
        PaneLayout::Split {
            orientation,
            children,
            weights,
        } => PaneNode::Split {
            orientation: to_core_orientation(*orientation),
            children: children.iter().map(|c| encode(c, contents)).collect(),
            // VERBATIM. Equalizing here would silently re-centre every dragged
            // divider on the next launch while the restore side looked correct.
            weights: weights.clone(),
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

    // --- Weights: non-equal literals, since `split`/`remove` equalize and an equal round trip cannot
    // tell preserved from recomputed; dragged vectors are the `*_dragged_weight_vector` tests' job.
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

    /// A PREVIEW leaf bound to `path` (the predicate tests need one to prove
    /// `is_empty_leaf` is not just "the path is empty").
    fn preview_leaf(path: &str) -> PaneNode {
        PaneNode::Leaf {
            path: path.to_string(),
            preview: true,
        }
    }

    /// One live pane holding an ordinary editable buffer at `path`.
    fn editor_pane(path: &str) -> LeafContent {
        LeafContent {
            path: path.to_string(),
            preview: false,
        }
    }

    /// One live pane showing a Markdown PREVIEW of `path`.
    fn preview_pane(path: &str) -> LeafContent {
        LeafContent {
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

    // === encode ===

    /// `encode` carries each leaf id's [`LeafContent`] — path AND preview flag —
    /// onto the persisted node, and an id it has no entry for becomes the empty,
    /// non-preview leaf.
    ///
    /// The three leaves are deliberately three DIFFERENT shapes, so one comparison
    /// discriminates all of them: an editor pane, a pane with no entry at all, and a
    /// PREVIEW pane. A seam that wrote `preview: false` unconditionally reddens on
    /// the third alone, and one that wrote `preview: true` unconditionally reddens
    /// on the first two — neither could be mistaken for a topology or weight
    /// failure, since those are pinned by the same value.
    ///
    /// FALSIFYING EDIT: write `preview: false` on every leaf. The preview pane then
    /// persists as an ordinary editor leaf, so the next launch reopens the source
    /// file in what was the preview pane — and, worse, `is_empty_leaf` starts
    /// suppressing an unnamed preview leaf, which is the one thing it exists not to
    /// do.
    #[test]
    fn encode_maps_ids_to_paths() {
        let mut contents = HashMap::new();
        contents.insert(P0, editor_pane(PATH_A));
        // P1 is deliberately absent: an unnamed pane encodes as an empty leaf, not dropped (macOS
        // parity), since dropping it changes the topology and desynchronises the DFS zip.
        contents.insert(P2, preview_pane(PATH_C));

        let got = encode(&asymmetric_layout(), &contents);

        // One whole-value comparison: editor, unmapped and preview ids, both orientations and both
        // weight vectors verbatim.
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
                    preview_leaf(PATH_C),
                ],
            ),
            "encode must map each leaf id through `contents` — carrying the pane's PREVIEW flag as \
             well as its path, and turning an ABSENT id into `Leaf {{ path: \"\", preview: false }}` \
             rather than dropping it — and copy both orientations and both weight vectors verbatim"
        );

        // The two rules the whole-value comparison above could be read past, stated
        // where a failure names them.
        let PaneNode::Split { children, .. } = &got else {
            panic!("encode turned the asymmetric layout into {got:?}, which is not a split at all");
        };
        assert_eq!(
            children[1],
            preview_leaf(PATH_C),
            "the leaf for the PREVIEW pane must carry `preview: true`. The flag is the only thing \
             that tells a restored preview pane from a restored editor pane, and `is_empty_leaf` \
             reads it too: a preview leaf that lost its flag and its path becomes the one shape \
             the save path suppresses outright."
        );
        assert_eq!(
            encode(&PaneLayout::Leaf(P1), &contents),
            leaf(""),
            "a leaf id with NO entry in `contents` must encode as `Leaf {{ path: \"\", \
             preview: false }}`. `LeafContent::default()` is that value, which is why the lookup \
             needs no second branch — but a missing id that came back as a PREVIEW leaf would \
             survive `is_empty_leaf` and persist an empty pane the restore path then rebuilds."
        );
    }

    // === resolve_leaves: the all-or-nothing rule the whole gather rests on ===

    /// One [`resolve_leaves`] table row.
    ///
    /// `want` is an association LIST so a row can be written without building a
    /// `HashMap` at the site; the comparison itself is against a MAP built from it,
    /// which is what puts the KEYS inside the expectation — a seam that resolved the
    /// right contents under the wrong pane ids would paint each pane with another
    /// pane's file after a restore.
    struct ResolveCase {
        desc: &'static str,
        leaves: Vec<(PaneId, WinContent)>,
        want: Option<Vec<(PaneId, LeafContent)>>,
    }

    /// The variant NAME of one leaf's answer, matched TOTALLY with no `_` arm.
    ///
    /// A new [`WinContent`] variant is an `E0004` here rather than an input the table
    /// below silently stops covering — the `may_snapshot_table` / `new_tab_press`
    /// convention, applied to an enum this module owns.
    fn win_content_kind(content: &WinContent) -> &'static str {
        match content {
            WinContent::Preview(_) => "Preview",
            WinContent::Buffer(_) => "Buffer",
            WinContent::Unknown => "Unknown",
        }
    }

    /// Every shape [`resolve_leaves`] is total over, as LITERALS.
    ///
    /// Deliberately not derived from the enum (Rust offers no such derivation) and
    /// deliberately not derived from the table, which would make the coverage sweep a
    /// tautology: a table that lost its `Unknown` rows would shrink both sides
    /// together. [`win_content_kind`]'s total match is what ties these strings back to
    /// the enum.
    const ALL_WIN_CONTENT_KINDS: [&str; 3] = ["Preview", "Buffer", "Unknown"];

    /// A leaf whose Neovim window resolves to an ordinary editable buffer at `path`.
    fn buffer_win(path: &str) -> WinContent {
        WinContent::Buffer(path.to_string())
    }

    /// A leaf whose Neovim window resolves to a Markdown PREVIEW of `path`.
    fn preview_win(path: &str) -> WinContent {
        WinContent::Preview(path.to_string())
    }

    /// The association list a row expects, as the map [`resolve_leaves`] returns.
    fn contents_map(entries: &[(PaneId, LeafContent)]) -> HashMap<PaneId, LeafContent> {
        entries.iter().cloned().collect()
    }

    /// The rows of the resolution rule, in one place so the coverage sweep in
    /// [`resolve_leaves_table`] can walk the same data the assertions do.
    fn resolve_cases() -> Vec<ResolveCase> {
        vec![
            ResolveCase {
                // The key row: two leaves of different kinds and paths, so one comparison checks the preview
                // flag, the per-id mapping and the path pass-through at once.
                desc: "a preview leaf and a buffer leaf resolve side by side, each keeping its \
                       OWN path and its own preview flag",
                leaves: vec![(P0, preview_win(PATH_A)), (P1, buffer_win(PATH_B))],
                want: Some(vec![(P0, preview_pane(PATH_A)), (P1, editor_pane(PATH_B))]),
            },
            ResolveCase {
                // Same kind, different paths: otherwise a seam answering `leaves[0]`'s content for every id fails
                // the row above only via the kinds, reading as a preview-flag bug.
                desc: "two buffer leaves keep their own paths — each id is resolved from its own \
                       window, never from the first leaf's",
                leaves: vec![(P0, buffer_win(PATH_A)), (P1, buffer_win(PATH_B))],
                want: Some(vec![(P0, editor_pane(PATH_A)), (P1, editor_pane(PATH_B))]),
            },
            ResolveCase {
                // T3's row. An UNNAMED buffer is a resolved answer, not an absent one.
                desc: "an unnamed buffer resolves to an EMPTY path beside a named leaf, rather \
                       than refusing",
                leaves: vec![(P0, buffer_win("")), (P1, buffer_win(PATH_B))],
                want: Some(vec![(P0, editor_pane("")), (P1, editor_pane(PATH_B))]),
            },
            ResolveCase {
                // The preview twin of the row above, the only place the empty-path rule is stated for a preview:
                // `is_empty_leaf` reads the flag too, so an empty-sourced preview is still persistable.
                desc: "a preview whose source path is empty resolves too, and keeps `preview: \
                       true`",
                leaves: vec![(P0, preview_win("")), (P1, buffer_win(PATH_B))],
                want: Some(vec![(P0, preview_pane("")), (P1, editor_pane(PATH_B))]),
            },
            ResolveCase {
                // T2's row, and the keystone of the whole design.
                desc: "ONE Unknown among two resolved leaves refuses the WHOLE gather",
                leaves: vec![
                    (P0, buffer_win(PATH_A)),
                    (P1, WinContent::Unknown),
                    (P2, buffer_win(PATH_C)),
                ],
                want: None,
            },
            ResolveCase {
                desc: "an Unknown beside a PREVIEW refuses too — the refusal is a property of the \
                       unresolvable leaf, not of what the others hold",
                leaves: vec![(P0, preview_win(PATH_A)), (P1, WinContent::Unknown)],
                want: None,
            },
            ResolveCase {
                desc: "an Unknown ALONE refuses: a one-pane tab whose window is in neither map is \
                       exactly the shape a naive gather would write as an empty leaf",
                leaves: vec![(P0, WinContent::Unknown)],
                want: None,
            },
        ]
    }

    /// Every leaf's content is carried onto its own [`LeafContent`], and any
    /// unresolvable leaf collapses the whole answer.
    ///
    /// FALSIFYING EDIT 1 — swap the two resolved arms' `preview` flag. Rows 1 and 4
    /// fail: a preview pane would persist as an ordinary editor leaf, so the next
    /// launch `:edit`s the source file into what was a preview, and an unnamed
    /// preview leaf starts being suppressed by `is_empty_leaf` — the one thing that
    /// predicate exists not to do.
    ///
    /// FALSIFYING EDIT 2 — route `Preview(p)` through the buffer arm. Same rows, same
    /// cost.
    ///
    /// FALSIFYING EDIT 3 — map `Unknown` to `LeafContent::default()`. The three
    /// refusal rows fail; [`an_unresolved_leaf_refuses_the_whole_gather`] is where
    /// that cost is spelled out.
    #[test]
    fn resolve_leaves_table() {
        let cases = resolve_cases();

        for case in &cases {
            let got = resolve_leaves(&case.leaves);
            let want = case.want.as_deref().map(contents_map);
            assert_eq!(
                got, want,
                "case: {} — resolve_leaves({:?}) must be {want:?}, got {got:?}. Each wrong answer \
                 is its own data loss. A leaf carrying ANOTHER leaf's path binds the wrong file to \
                 the wrong pane on the next launch. A lost `preview: true` reopens the source in \
                 an editor pane and makes an unnamed preview leaf suppressible. A `Some` where a \
                 `None` is due is the silent-loss shape this seam exists to make unreachable — see \
                 `an_unresolved_leaf_refuses_the_whole_gather`. A `None` where a `Some` is due \
                 refuses every gather, so the tab contributes only its cache and the user's edits \
                 stop being persisted at all.",
                case.desc, case.leaves
            );
        }

        // --- The table covers the domain: every WinContent kind appears as an input, crossed against
        // `win_content_kind`'s total match, so a new variant fails to compile there.
        for kind in ALL_WIN_CONTENT_KINDS {
            assert!(
                cases.iter().any(|case| case
                    .leaves
                    .iter()
                    .any(|(_, content)| win_content_kind(content) == kind)),
                "no resolve_leaves row carries a {kind} leaf, so that arm's answer is untested. \
                 Every one of the three is reachable from a real snapshot: a converted pane is a \
                 Preview, an ordinary pane is a Buffer, and a pane whose Neovim window is in \
                 neither `previews` nor `win_buffers` is Unknown."
            );
        }

        // --- Non-vacuity, in both directions ----------------------------------
        assert!(
            cases.iter().any(|case| case.want.is_some()),
            "no resolve_leaves row expects a resolved map, so a seam that answered None for \
             everything would satisfy every row above while NO tab ever gathered a layout again — \
             every window would persist only its cache, forever."
        );
        assert!(
            cases.iter().any(|case| case.want.is_none()),
            "no resolve_leaves row expects a refusal, so the all-or-nothing rule is unstated and a \
             seam that resolved everything would pass. That seam is the one that writes a \
             well-formed `state.json` whose leaf paths are all empty."
        );
    }

    /// **THE KEYSTONE.** One leaf whose Neovim window is in neither snapshot map
    /// refuses the WHOLE tab's gather, and an unnamed buffer beside it does not.
    ///
    /// The pair is the point. `""` and "absent" are the two answers a naive gather
    /// cannot tell apart, and mapping the second onto the first is precisely what
    /// writes a well-formed layout over the user's files: `is_empty_leaf` suppresses
    /// only a LONE empty leaf, so a multi-leaf tree of empty leaves is persisted
    /// verbatim, `load_state` reads it back as a real topology, and the next launch
    /// rebuilds a tab of blank panes where the files were. Nothing on screen says so
    /// and no arity guard can catch it — which is why the refusal lives here, at the
    /// only point where the difference is still visible.
    ///
    /// FALSIFYING EDIT: in [`resolve_leaves`], map `WinContent::Unknown` to
    /// `LeafContent { path: String::new(), preview: false }` instead of collapsing to
    /// `None`.
    #[test]
    fn an_unresolved_leaf_refuses_the_whole_gather() {
        let with_unknown = [
            (P0, buffer_win(PATH_A)),
            (P1, WinContent::Unknown),
            (P2, buffer_win(PATH_C)),
        ];
        let got = resolve_leaves(&with_unknown);
        assert_eq!(
            got, None,
            "one leaf whose window is in NEITHER `Snapshot::previews` nor \
             `Snapshot::win_buffers` must refuse the whole tab's gather, got {got:?}. The `Some` \
             here is the SILENT-LOSS SHAPE: two real files and one `\"\"`, encoded into a \
             well-formed `PaneNode::Split` that `is_empty_leaf` never suppresses (it suppresses \
             only a LONE empty leaf), committed over the saved tree, and read back on the next \
             launch as a genuine three-pane topology with one pane blank. There is no arity guard \
             anywhere that could catch it and nothing on screen that could report it. The refusal \
             means `contribute the CACHE`, which is last-known-good rather than wrong."
        );

        // The discriminating negative: the refusal is about the absent window, never the empty path.
        // Same leaves and ids, `Unknown` swapped for a genuinely unnamed buffer.
        let with_unnamed = [
            (P0, buffer_win(PATH_A)),
            (P1, buffer_win("")),
            (P2, buffer_win(PATH_C)),
        ];
        let resolved = resolve_leaves(&with_unnamed);
        assert_eq!(
            resolved,
            Some(contents_map(&[
                (P0, editor_pane(PATH_A)),
                (P1, editor_pane("")),
                (P2, editor_pane(PATH_C)),
            ])),
            "the SAME three leaves with the unresolvable one replaced by a genuinely UNNAMED \
             buffer must RESOLVE, got {resolved:?}. Without this the refusal above is satisfied by \
             a seam that refuses whenever any path is empty — which would refuse every gather of \
             every tab holding an unnamed buffer, so a user who opened a scratch buffer beside a \
             file would silently stop persisting that tab. An empty path is an ANSWER; an absent \
             window is not."
        );

        // A lone Unknown refuses too: there the wrong answer looks harmless, since `is_empty_leaf`
        // suppresses a single empty leaf and the tab would silently forget its file.
        let alone = [(P0, WinContent::Unknown)];
        assert_eq!(
            resolve_leaves(&alone),
            None,
            "a LONE unresolvable leaf must refuse as well. Mapped to an empty leaf it would take \
             the other route to the same loss: `persistable_layout` suppresses a lone empty \
             non-preview leaf, so the tab would persist `layout: None`, and a tab's open buffer \
             paths ride ONLY inside `TabState::layout` — the file would not be reopened at all. \
             That is the macOS single-pane bug arriving by a new road."
        );
    }

    /// An unnamed buffer is a RESOLVED answer whose path is `""`.
    ///
    /// The positive half of the rule above, on the smallest input there is, so a
    /// failure names the confusion directly rather than through a three-leaf fixture.
    ///
    /// FALSIFYING EDIT: treat an empty path as [`WinContent::Unknown`] — i.e. refuse
    /// when `path.is_empty()`.
    #[test]
    fn an_unnamed_buffer_resolves_rather_than_refusing() {
        let Some(contents) = resolve_leaves(&[(P0, WinContent::Buffer(String::new()))]) else {
            panic!(
                "resolve_leaves REFUSED a single unnamed buffer. A `[No Name]` buffer is a real \
                 thing a user opens — the empty-state pane is one, and so is every `:enew` — so a \
                 seam that treats `\"\"` as unresolvable refuses the gather for the whole tab \
                 holding it, and that tab silently stops persisting anything but its cache. The \
                 refusal exists for a window that is in NEITHER snapshot map, which is a different \
                 fact about a different thing."
            );
        };

        let content = contents.get(&P0).unwrap_or_else(|| {
            panic!(
                "resolve_leaves resolved, but produced no entry for pane {P0} — the only id it was \
                 given. `encode` looks each leaf id up in this map and turns a MISSING id into \
                 `Leaf {{ path: \"\", preview: false }}`, so a dropped key reaches the same empty \
                 leaf by a route no assertion downstream can distinguish. Map was {contents:?}"
            )
        });
        assert_eq!(
            content,
            &LeafContent {
                path: String::new(),
                preview: false,
            },
            "an unnamed buffer must resolve to an EMPTY, non-preview leaf — the value \
             `is_empty_leaf` suppresses when it is alone, which is the pre-existing empty-state \
             invariant. A `preview: true` here would make the empty-state pane persist as a \
             preview leaf that `persistable_layout` no longer suppresses, so every project-less \
             launch would start writing a layout it never had."
        );

        // Non-vacuity: a seam answering the empty leaf for everything satisfies the assertions above.
        assert_eq!(
            resolve_leaves(&[(P0, WinContent::Buffer(PATH_A.to_string()))]),
            Some(contents_map(&[(P0, editor_pane(PATH_A))])),
            "a NAMED buffer must resolve to its own path. Without this the empty-path assertions \
             above would hold for a seam that wrote `LeafContent::default()` for every leaf — the \
             exact silent-loss shape, passing a test written to forbid it."
        );
    }

    // === is_empty_leaf: suppression by content, never by arity ===

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
                // Anti-arity row: two empty leaves. An arity test and a content test agree on `false` by accident;
                // a "the tree carries nothing" test would say `true` and drop a real two-pane topology.
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

    // === DFS order: the property the whole restore zip rests on ===

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
        // `pane_view::apply_layout_rebuild` sets `next_pane_id = first_leaf_id + leaf_ids.len()`, which
        // is right only if `decode` allocates contiguously from `first_leaf_id`.
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

    // === Round trip, with non-equal weights ===

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "every expected weight (0.25 / 0.75 / 0.5) is exactly representable in binary \
                  f64 and is copied through encode/decode unmodified rather than computed with, \
                  so an epsilon comparison would weaken the assertion without making it more \
                  correct — the `mouse::scroll_threshold_table` precedent"
    )]
    fn decode_round_trips_encode() {
        // Literal non-equal weights, so this asserts the codec alone; a real drag reaching `state.json`
        // and back is `dragged_divider_weights_survive_relaunch_under_cage`.
        let layout = PaneLayout::Split {
            orientation: Orientation::Vertical,
            children: vec![PaneLayout::Leaf(P0), PaneLayout::Leaf(P1)],
            weights: vec![WEIGHT_SMALL, WEIGHT_LARGE],
        };
        let mut contents = HashMap::new();
        contents.insert(P0, editor_pane(PATH_A));
        contents.insert(P1, editor_pane(PATH_B));

        let node = encode(&layout, &contents);
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
        let mut tri_contents = HashMap::new();
        tri_contents.insert(P0, editor_pane(PATH_A));
        tri_contents.insert(P1, editor_pane(PATH_B));
        tri_contents.insert(P2, editor_pane(PATH_C));
        let tri_node = encode(&asymmetric_layout(), &tri_contents);
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

    // === Rejection: a crash guard, not tidiness ===

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

        // Non-vacuous caps: a tree exactly at each limit is accepted, so `> MAX` turned `>= MAX`, or a
        // zero cap, is caught here rather than passing as correct rejection.
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

    // === A dragged weight vector, through the codec both ways ===
    // Weights from `PaneTree::set_divider_fraction`, as a drag makes them; the relaunch smoke's twins.

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

    /// The `PaneId` -> [`LeafContent`] map the drag tests encode with. Both panes
    /// are ordinary editors: the subject here is the WEIGHT vector, not the leaves.
    fn dragged_contents() -> HashMap<PaneId, LeafContent> {
        let mut contents = HashMap::new();
        contents.insert(P0, editor_pane(PATH_A));
        contents.insert(P1, editor_pane(PATH_B));
        contents
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

        let node = encode(&tree.layout(), &dragged_contents());

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
        let node = encode(&dragged_two_leaf_tree().layout(), &dragged_contents());

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
