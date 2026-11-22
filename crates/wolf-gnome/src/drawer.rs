//! The terminal drawer's pure arithmetic: the drawer's share of the pane area,
//! which the divider between the editor tree and the full-width drawer strip holds,
//! and which pane a `<C-w>h/j/k/l` intent should focus once the drawer is in the
//! picture.
//!
//! GTK-, Pango- and Cairo-free, so the whole rulebook runs in the fast unit suite
//! with no display server, no compositor and no Neovim — exactly as
//! [`crate::geometry`], [`crate::editor_inset`] and [`crate::cell_metrics_math`] do.
//!
//! ## Why the drawer is not a [`PaneTree`] leaf
//!
//! It lives OUTSIDE the tree, which is what forces this module to exist at all. The
//! durable reasons (a drawer kept out of `state.json`, every `neighbor` result left
//! unperturbed) are in `notes/wolf-gnome/window-structure.md`. The price is paid
//! here: a pane the tree cannot see needs its own share and its own focus bridge.
//!
//! ## One module for the share and the bridge
//!
//! Both are the drawer's answers to questions the tree answers for every other pane:
//! how much of the pane area it takes, and where focus goes across its edge.

use wolf_core::FocusDir;

use crate::geometry;
use crate::layout::{PaneId, PaneTree, Rect};

/// The share of the pane area a window's drawer opens at, until a drag replaces it
/// through [`DrawerShare`].
///
/// One third, matching `runtime/nvim/init.lua`'s `math.max(1, floor(lines / 3))`
/// — the height the `wolf_toggle_terminal` Lua helper opens the split at. The two
/// are independent expressions of one number: nvim divides ROWS, this divides
/// PIXELS, and the front-end then drives its own `resize_grid` from the pixels.
pub const DRAWER_WEIGHT: f64 = 1.0 / 3.0;

/// The drawer's share of the pane area: the fraction of the drawer divider's
/// `gtk::Paned` span that the drawer, its end child, takes.
///
/// One per window, held by `tab_view` and shared by every tab's pane view, so a drag
/// in one tab reaches the others. It is a separate value from [`DrawerState`] so that
/// [`DrawerState::clear`] (the drawer closing, a restart) leaves it alone. A new
/// window starts at [`DRAWER_WEIGHT`].
///
/// It is never persisted, and needs no fraction floor: a tree weight keeps one
/// because a zero survives `state.json`, while every use of the share is clamped into
/// GTK's `[min_position, max_position]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawerShare(f64);

impl DrawerShare {
    /// The share a new window's drawer opens at, [`DRAWER_WEIGHT`].
    #[must_use]
    pub const fn new() -> Self {
        Self(DRAWER_WEIGHT)
    }

    /// The drawer's fraction of the span, in `[0, 1]`.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The paned position this share puts the divider at: the editor tree's height.
    ///
    /// `min` and `max` are the paned's `min_position` and `max_position`, the clamp
    /// that holds both children at their floors; `span` is the paned's height, which
    /// the share is a proportion of. The arithmetic is
    /// [`geometry::divider_position`]'s, at the editor's fraction `1 - share`.
    #[must_use]
    pub fn position(self, min: i32, max: i32, span: i32) -> i32 {
        geometry::divider_position(min, max, span, 1.0 - self.0)
    }

    /// Records a drag that left the divider at `position` of a paned `span` high. A
    /// non-positive `span`, before the first allocation, leaves the share unchanged.
    pub fn drag_to(&mut self, position: i32, span: i32) {
        if let Some(fraction) = geometry::divider_span_fraction(position, span) {
            self.0 = 1.0 - fraction;
        }
    }
}

impl Default for DrawerShare {
    fn default() -> Self {
        Self::new()
    }
}

/// The drawer's front-end state: which pane is bound to it, and which EDITOR pane
/// was most recently active.
///
/// The two travel together because they are read together: [`focus_target`] consults
/// both to answer one question, so a caller that had one without the other could not
/// ask it. The bundling buys that cohesion and nothing else — it is NOT lint headroom.
/// `clippy::too_many_arguments` fires only when the count EXCEEDS its default
/// threshold of seven, and splitting this struct into two parameters would leave
/// [`focus_target`] at seven, still legal. It is [`Copy`] so `pane_view` can hold it in
/// a `Cell` beside `active_pane` rather than a `RefCell`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawerState {
    /// The pane bound to the drawer window, or `None` while no drawer is open.
    drawer: Option<PaneId>,
    /// The most recently active editor leaf, or `None` before one is recorded.
    last_editor: Option<PaneId>,
}

impl DrawerState {
    /// The closed state: no drawer bound, no editor recorded.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            drawer: None,
            last_editor: None,
        }
    }

    /// The pane bound to the drawer, or `None` while it is closed.
    #[must_use]
    pub const fn id(self) -> Option<PaneId> {
        self.drawer
    }

    /// Whether a drawer is currently bound.
    #[must_use]
    pub const fn is_open(self) -> bool {
        self.drawer.is_some()
    }

    /// Whether `pane` IS the bound drawer. `false` while the drawer is closed, for
    /// every pane.
    #[must_use]
    pub fn is_drawer(self, pane: PaneId) -> bool {
        self.drawer == Some(pane)
    }

    /// Binds `pane` as the drawer.
    pub fn bind(&mut self, pane: PaneId) {
        self.drawer = Some(pane);
    }

    /// Empties BOTH fields: the bound drawer and the recorded editor pane.
    ///
    /// Clearing both is what `reset_after_restart` needs. A restart replaces every
    /// pane id in the tab, so a surviving `last_editor` would name a pane that no
    /// longer exists — and, worse, a surviving `drawer` would make
    /// `apply_drawer_open`'s `is_open()` guard true against a dead Neovim's id, so
    /// the drawer could never be opened again for the rest of the session.
    pub fn clear(&mut self) {
        self.drawer = None;
        self.last_editor = None;
    }

    /// Records `pane` as the most-recently-used editor pane, but only when it is a
    /// live tree leaf AND is not the drawer.
    ///
    /// `is_leaf` is passed in rather than read here because the tree is not this
    /// module's to hold; the caller answers it with `tree.leaves().contains(&pane)`.
    /// Refusing the drawer is what makes [`drawer_up_target`] able to leave it:
    /// recording the drawer as the MRU editor would make `<C-w>k` target the pane it
    /// was pressed in.
    pub fn record_active(&mut self, pane: PaneId, is_leaf: bool) {
        if is_leaf && !self.is_drawer(pane) {
            self.last_editor = Some(pane);
        }
    }

    /// The most recently active editor pane, or `None` before one is recorded.
    #[must_use]
    pub const fn last_editor(self) -> Option<PaneId> {
        self.last_editor
    }
}

/// The bottom-most editor leaf: the greatest `y + h` (a top-left origin, so a
/// larger `y` is lower on screen). Ties on the bottom edge break to the smallest
/// `x` (left-most), then to the smallest [`PaneId`]. `None` for an empty slice.
///
/// It is the pane physically adjacent to the drawer, so it is where `<C-w>k` lands when
/// there is no most-recently-used editor pane to prefer.
///
/// The exact `==` comparisons are deliberate rather than an oversight. A tie here is
/// two rects the SAME tiling arithmetic produced from the same bounds — see
/// [`PaneTree::pane_rects`] — so the edges are bit-identical when they are equal at
/// all, and an epsilon would merge genuinely distinct rows.
#[must_use]
pub fn bottom_most_leaf(rects: &[(PaneId, Rect)]) -> Option<PaneId> {
    let mut best: Option<(PaneId, f64, f64)> = None;
    for &(id, r) in rects {
        let bottom = r.y + r.h;
        let better = match best {
            None => true,
            Some((best_id, best_bottom, best_x)) => {
                bottom > best_bottom
                    || (bottom == best_bottom && r.x < best_x)
                    || (bottom == best_bottom && r.x == best_x && id < best_id)
            }
        };
        if better {
            best = Some((id, bottom, r.x));
        }
    }
    best.map(|(id, _, _)| id)
}

/// The `<C-w>k` target from inside the drawer: the most-recently-used EDITOR pane
/// when it is still a live leaf, else [`bottom_most_leaf`].
///
/// The MRU preference is what makes the drawer feel like a place you stepped into
/// and back out of rather than a place that drops you somewhere else. The
/// bottom-most fallback is the geometric answer for a first visit, or after the MRU
/// pane was closed.
#[must_use]
pub fn drawer_up_target(
    state: DrawerState,
    leaves: &[PaneId],
    rects: &[(PaneId, Rect)],
) -> Option<PaneId> {
    if let Some(mru) = state.last_editor() {
        if leaves.contains(&mru) {
            return Some(mru);
        }
    }
    bottom_most_leaf(rects)
}

/// The pane a `<C-w>h/j/k/l` intent should focus, or `None` for "do nothing".
///
/// This is the WHOLE editor <-> drawer bridge as one value, and it exists because
/// Neovim declines to act on a focus intent itself: `<C-w>k` from the drawer emits
/// the intent and leaves `active_win` where it was, so without a front-end answer a
/// user who opens the drawer cannot leave it.
///
/// Six rules, exhaustively:
///
/// 1. From the DRAWER going `Up` — [`drawer_up_target`].
/// 2. From the DRAWER in any other direction — `None`. The drawer is full width and
///    sits at the bottom, so nothing borders it but the editor above.
/// 3. From an EDITOR pane with a tree neighbour — that neighbour.
/// 4. From an EDITOR pane with no neighbour, going `Down`, with the drawer open —
///    the drawer. This is the bottom-edge bridge, the inverse of rule 1.
/// 5. From an EDITOR pane with no neighbour, going `Down`, with no drawer — `None`.
/// 6. From an EDITOR pane with no neighbour, in any other direction — `None`.
///
/// **The `Down` guard on rule 4 is load-bearing.** Without it every edge-of-tree
/// move in any direction would fall into the drawer, so `<C-w>k` from the top row
/// would jump to the bottom of the window.
///
/// **Deliberate divergence: `width`/`height` are the RAW pane-area size, not the
/// editor height the macOS front-end passes
/// (`crates/wolf-macos/Sources/WolfMacOS/README.md#panecontainerviewswift`).** Here
/// [`PaneTree::pane_rects`] feeds only ORDERING — `neighbor`'s cross-axis overlap
/// and distance comparisons, and [`bottom_most_leaf`]'s maximum — and both consumers
/// are scale-invariant, because the tiling scales every rect by the same factor.
/// Threading the reduced height through would be unfalsifiable precision: no input
/// exists for which it changes an answer. Said here because a reader would otherwise
/// "fix" it.
#[must_use]
pub fn focus_target(
    tree: &PaneTree,
    origin: PaneId,
    dir: FocusDir,
    state: DrawerState,
    width: f64,
    height: f64,
) -> Option<PaneId> {
    if state.is_drawer(origin) {
        if dir != FocusDir::Up {
            return None;
        }
        return drawer_up_target(state, &tree.leaves(), &tree.pane_rects(width, height));
    }
    if let Some(target) = tree.neighbor(origin, dir, width, height) {
        return Some(target);
    }
    if dir == FocusDir::Down {
        return state.id();
    }
    None
}

#[cfg(test)]
mod tests {
    //! The drawer rulebook's three tables. Every one of them is arithmetic over
    //! plain numbers and pane ids, so none of it needs GTK, a display server or a
    //! Neovim — the whole module runs in the fast unit tier.
    //!
    //! What these tables CANNOT see, stated once here rather than repeated per
    //! test: whether `pane_view` calls any of it. Each function's rule is pinned
    //! below; each call site's wiring is a cage smoke's job.

    use super::*;
    use crate::layout::Orientation;

    // -- bottom_most_leaf ---------------------------------------------------

    /// The pane-area width the leaf rows below tile.
    const PANE_W: f64 = 1000.0;
    /// The pane-area height the leaf rows below tile.
    const PANE_H: f64 = 600.0;

    /// Builds a [`Rect`] positionally, so a table row reads `x, y, w, h`.
    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    /// One [`bottom_most_leaf`] table row.
    struct BottomCase {
        desc: &'static str,
        rects: Vec<(PaneId, Rect)>,
        expected: Option<PaneId>,
    }

    /// Pins [`bottom_most_leaf`]: the greatest `y + h` wins, ties break to the
    /// smallest `x` and then to the smallest [`PaneId`], and an empty slice has no
    /// answer.
    ///
    /// Every multi-leaf row puts a LOSER first in the slice, and the three-leaf row
    /// puts its winner in the middle, so an implementation that simply kept the
    /// first entry — or the last — cannot pass them.
    ///
    /// FALSIFYING EDITS:
    ///
    /// - Flip `bottom > best_bottom` to `<`: the stacked pair returns the TOP leaf.
    /// - Compare `r.y` instead of `r.y + r.h`: the "a SMALLER y can still be
    ///   bottom-most" row returns the short lower leaf, because the tall leaf that
    ///   actually reaches further down starts higher.
    /// - Delete the `r.x < best_x` clause: the side-by-side tie returns the RIGHT
    ///   leaf (id 3) rather than the left one (id 9). The left leaf deliberately
    ///   carries the GREATER id, so the row cannot come out right through the id
    ///   tie-break instead.
    /// - Delete the `id < best_id` clause: the identical-rect tie returns 9 rather
    ///   than 3. That clause is unreachable through
    ///   [`PaneTree::pane_rects`] — which is exactly why it needs a row, or it gets
    ///   deleted as dead the first time somebody reads it.
    #[test]
    fn bottom_most_leaf_table() {
        /// The greater of the two tie-break ids.
        const GREATER_ID: PaneId = 9;
        /// The lesser of the two tie-break ids.
        const LESSER_ID: PaneId = 3;
        /// A third id, for the row whose winner sits in the middle of the slice.
        const MIDDLE_ID: PaneId = 6;

        let cases = [
            BottomCase {
                desc: "no leaves at all has no bottom-most leaf",
                rects: vec![],
                expected: None,
            },
            BottomCase {
                desc: "a lone leaf is its own bottom-most",
                rects: vec![(GREATER_ID, rect(0.0, 0.0, PANE_W, PANE_H))],
                expected: Some(GREATER_ID),
            },
            BottomCase {
                desc: "a stacked pair: the LOWER leaf wins, and it is listed FIRST so \
                       'keep the last entry' cannot pass",
                rects: vec![
                    (GREATER_ID, rect(0.0, 300.0, PANE_W, 300.0)),
                    (LESSER_ID, rect(0.0, 0.0, PANE_W, 300.0)),
                ],
                expected: Some(GREATER_ID),
            },
            BottomCase {
                // Bottoms are 600 and 300: the tall leaf reaches further down even
                // though it starts higher. A comparison on `y` alone picks the other.
                desc: "a SMALLER y can still be bottom-most, because the edge is y + h",
                rects: vec![
                    (LESSER_ID, rect(0.0, 100.0, PANE_W, 200.0)),
                    (GREATER_ID, rect(0.0, 0.0, PANE_W, PANE_H)),
                ],
                expected: Some(GREATER_ID),
            },
            BottomCase {
                desc: "three leaves: the winner sits in the MIDDLE of the slice",
                rects: vec![
                    (LESSER_ID, rect(0.0, 0.0, PANE_W, 200.0)),
                    (MIDDLE_ID, rect(0.0, 400.0, PANE_W, 200.0)),
                    (GREATER_ID, rect(0.0, 200.0, PANE_W, 200.0)),
                ],
                expected: Some(MIDDLE_ID),
            },
            BottomCase {
                // Same bottom edge, different x: the left leaf wins while listed second with the greater id,
                // so neither "keep the first" nor the id tie-break can produce this by accident.
                desc: "a tie on the bottom edge breaks to the SMALLEST x",
                rects: vec![
                    (LESSER_ID, rect(500.0, 300.0, 500.0, 300.0)),
                    (GREATER_ID, rect(0.0, 300.0, 500.0, 300.0)),
                ],
                expected: Some(GREATER_ID),
            },
            BottomCase {
                // Identical rects: only the id separates them. `pane_rects` never produces this; the row stops
                // the id tie-break being deleted as unreachable.
                desc: "a tie on the bottom edge AND on x breaks to the SMALLEST id",
                rects: vec![
                    (GREATER_ID, rect(0.0, 300.0, PANE_W, 300.0)),
                    (LESSER_ID, rect(0.0, 300.0, PANE_W, 300.0)),
                ],
                expected: Some(LESSER_ID),
            },
        ];

        for case in &cases {
            assert_eq!(
                bottom_most_leaf(&case.rects),
                case.expected,
                "case: {} — rects {:?}. This is where `<C-w>k` from the drawer lands \
                 when there is no most-recently-used editor pane, so a wrong answer \
                 sends the user to a pane that does not border the drawer; `None` \
                 where a leaf was expected leaves them in the drawer with no exit.",
                case.desc,
                case.rects
            );
        }
    }

    // -- focus_target -------------------------------------------------------

    /// The pane id of the top editor leaf in the two-leaf fixture tree.
    const TOP: PaneId = 1;
    /// The pane id of the bottom editor leaf, the one that borders the drawer.
    const BOTTOM: PaneId = 2;
    /// The pane id bound to the drawer, which is NEVER a leaf of the tree.
    const DRAWER: PaneId = 9;
    /// A pane id that was recorded as the MRU editor and has since been closed.
    const GONE: PaneId = 4;

    /// The height the editor region is left with while a drawer is open: the pane
    /// area less its third.
    ///
    /// Two thirds of [`PANE_H`]: the editor height that [`focus_target`]'s doc says
    /// it deliberately does not take.
    ///
    /// A literal fraction, NOT [`DRAWER_WEIGHT`]. Reading the production constant
    /// would mean a `DRAWER_WEIGHT` of zero silently made this height equal
    /// `PANE_H`, turning the second pass into a re-run of the first — the vacuity
    /// the rect guard in [`focus_target_table`] also watches for, and one worth
    /// being unable to introduce at all. `600 * 2 / 3` is exactly 400.0, so the
    /// failure messages carry a round number.
    const EDITOR_REGION_H: f64 = PANE_H * 2.0 / 3.0;

    /// One [`focus_target`] table row.
    struct FocusCase {
        desc: &'static str,
        origin: PaneId,
        dir: FocusDir,
        state: DrawerState,
        expected: Option<PaneId>,
    }

    /// Pins all six rules of the editor <-> drawer focus bridge over one fixed
    /// two-leaf tree.
    ///
    /// The tree's own answers are asserted first as FIXTURE GUARDS rather than
    /// assumed: every bridge row below is only meaningful if the bottom leaf really
    /// has no neighbour below it and the top leaf really has one, and that is a
    /// property of [`PaneTree`], not of this module.
    ///
    /// FALSIFYING EDITS:
    ///
    /// - Delete the `dir == FocusDir::Down` guard on the bridge arm: the "editor,
    ///   Up, no neighbour" row returns `Some(9)` — the drawer — where `None` was
    ///   wanted, so `<C-w>k` from the top row would jump past the whole tree into
    ///   the bottom of the window. The two sideways rows go with it.
    /// - Delete the drawer branch's `if dir != FocusDir::Up { return None; }` guard:
    ///   the "drawer, Down" row returns `Some(1)`, the MRU editor leaf, so `<C-w>j`
    ///   pressed INSIDE the drawer would eject the user upward.
    /// - Delete the whole `state.is_drawer(origin)` branch instead: the three
    ///   drawer-origin `Up` rows return `None` — the drawer is not in the tree, so
    ///   `neighbor` finds nothing and only `Down` has a fallback. That is exactly
    ///   the trap this seam exists to remove.
    /// - Drop the MRU preference in [`drawer_up_target`]: the live-MRU row returns
    ///   the BOTTOM leaf rather than the top one the user came from.
    /// - Drop the `leaves.contains(&mru)` liveness check: the stale-MRU row returns
    ///   `Some(4)`, a pane that no longer exists, and the focus move silently does
    ///   nothing.
    ///
    /// ## The second pass, at [`EDITOR_REGION_H`]
    ///
    /// Every row runs twice: once at the raw pane-area height and once at two thirds of
    /// it, the editor region an open drawer leaves. That second pass is the only
    /// assertion anywhere on the divergence [`focus_target`]'s doc records — it takes
    /// the raw pane-area size rather than the editor height, because
    /// [`PaneTree::pane_rects`] feeds only ORDERING here and both consumers are
    /// scale-invariant. That is exactly the claim a reader would "fix" by threading the
    /// reduced height through, and nothing was measuring it.
    ///
    /// **Its falsifier is not an edit to [`focus_target`].** Passing a
    /// proportionally smaller height is precisely the change the divergence says is
    /// unobservable, so making it leaves both passes green — that IS the claim. What
    /// the second pass converts is a premise about [`PaneTree`] into a measurement
    /// against the real one: if its tiling ever stopped scaling with its bounds, the
    /// reduced-height pass would diverge while the full-height pass stayed green,
    /// and the divergence recorded in `focus_target`'s doc comment would be a bug
    /// rather than a documented simplification. The guard that the two heights
    /// really do produce different rects is what stops the second pass being a
    /// re-run of the first. Which height the CALLER passes is `pane_view`'s wiring,
    /// and stays a cage smoke's evidence.
    #[test]
    fn focus_target_table() {
        let mut tree = PaneTree::new(TOP);
        tree.split(TOP, BOTTOM, Orientation::Horizontal);

        // Fixture guard: any other stacking or order would make every row below assert about a window
        // shape this front-end never builds.
        assert_eq!(
            tree.leaves(),
            vec![TOP, BOTTOM],
            "fixture: a horizontal split of the root must leave exactly two leaves, \
             top first. The bridge rows below name TOP and BOTTOM by position."
        );
        assert_eq!(
            tree.neighbor(TOP, FocusDir::Down, PANE_W, PANE_H),
            Some(BOTTOM),
            "fixture: the top leaf must have the bottom leaf below it, or the \
             'a tree neighbour WINS over the bridge' row proves nothing."
        );
        assert_eq!(
            tree.neighbor(BOTTOM, FocusDir::Down, PANE_W, PANE_H),
            None,
            "fixture: the bottom leaf must have NO neighbour below it, or the bridge \
             rows are never reached and would pass with the bridge deleted."
        );

        // Measured against the real `PaneTree`: the reduced height changes the rects (else pass two
        // proves nothing), and the tree's answers match at both heights, which scale invariance rests on.
        assert_ne!(
            tree.pane_rects(PANE_W, EDITOR_REGION_H),
            tree.pane_rects(PANE_W, PANE_H),
            "fixture: the reduced height must produce DIFFERENT rects from the full \
             pane area, or running every row again at it says nothing at all. \
             EDITOR_REGION_H is the pane area less its third, so equal rects mean \
             that third has gone to zero."
        );
        assert_eq!(
            tree.neighbor(TOP, FocusDir::Down, PANE_W, EDITOR_REGION_H),
            Some(BOTTOM),
            "fixture: the tree's own neighbour answers must not move with the height. \
             If they do, `focus_target`'s recorded divergence — that it may take the \
             RAW pane-area size because `pane_rects` feeds only ordering — has stopped \
             being true, and which height the caller passes is suddenly observable."
        );
        assert_eq!(
            tree.neighbor(BOTTOM, FocusDir::Down, PANE_W, EDITOR_REGION_H),
            None,
            "fixture: the bottom leaf must have no neighbour below it at the reduced \
             height either, or the bridge rows of the second pass are unreachable and \
             pass for the wrong reason."
        );

        // The drawer is open and the user stepped into it from the TOP leaf.
        let mut open_with_mru = DrawerState::new();
        open_with_mru.bind(DRAWER);
        open_with_mru.record_active(TOP, true);

        // The drawer is open, and the editor pane the user came from has since been
        // closed — so the MRU names a pane that is no longer a leaf.
        let mut open_stale_mru = DrawerState::new();
        open_stale_mru.bind(DRAWER);
        open_stale_mru.record_active(GONE, true);

        // The drawer is open and nothing has been recorded yet (a first visit).
        let mut open_no_mru = DrawerState::new();
        open_no_mru.bind(DRAWER);

        // No drawer at all.
        let closed = DrawerState::new();

        let cases = [
            // --- Rule 1: from the drawer, Up goes to the MRU editor leaf.
            FocusCase {
                desc: "drawer, Up, live MRU -> the pane the user stepped in from",
                origin: DRAWER,
                dir: FocusDir::Up,
                state: open_with_mru,
                expected: Some(TOP),
            },
            FocusCase {
                desc: "drawer, Up, STALE MRU -> the bottom-most leaf instead",
                origin: DRAWER,
                dir: FocusDir::Up,
                state: open_stale_mru,
                expected: Some(BOTTOM),
            },
            FocusCase {
                desc: "drawer, Up, no MRU recorded -> the bottom-most leaf",
                origin: DRAWER,
                dir: FocusDir::Up,
                state: open_no_mru,
                expected: Some(BOTTOM),
            },
            // --- Rule 2: the drawer is full width and sits at the bottom, so
            // --- nothing borders it but the editor region above.
            FocusCase {
                desc: "drawer, Down -> nothing is below the drawer",
                origin: DRAWER,
                dir: FocusDir::Down,
                state: open_with_mru,
                expected: None,
            },
            FocusCase {
                desc: "drawer, Left -> nothing is beside the drawer",
                origin: DRAWER,
                dir: FocusDir::Left,
                state: open_with_mru,
                expected: None,
            },
            FocusCase {
                desc: "drawer, Right -> nothing is beside the drawer",
                origin: DRAWER,
                dir: FocusDir::Right,
                state: open_with_mru,
                expected: None,
            },
            // --- Rule 3: an ordinary tree neighbour wins over the bridge, or `<C-w>j` from the top row
            // --- would skip the pane directly below it.
            FocusCase {
                desc: "editor with a neighbour below: a tree neighbour WINS over the bridge",
                origin: TOP,
                dir: FocusDir::Down,
                state: open_with_mru,
                expected: Some(BOTTOM),
            },
            FocusCase {
                desc: "editor with a neighbour above: the ordinary tree answer",
                origin: BOTTOM,
                dir: FocusDir::Up,
                state: open_with_mru,
                expected: Some(TOP),
            },
            // --- Rules 4 and 5: the bottom-edge bridge, and its absence.
            FocusCase {
                desc: "editor, Down, no neighbour, drawer OPEN -> the drawer",
                origin: BOTTOM,
                dir: FocusDir::Down,
                state: open_with_mru,
                expected: Some(DRAWER),
            },
            FocusCase {
                desc: "editor, Down, no neighbour, NO drawer -> nothing happens",
                origin: BOTTOM,
                dir: FocusDir::Down,
                state: closed,
                expected: None,
            },
            // --- Rule 6: every other direction off the edge of the tree stays put.
            // --- The first of these is the row the `Down` guard exists for.
            FocusCase {
                desc: "editor, Up, no neighbour, drawer open -> NOT the drawer",
                origin: TOP,
                dir: FocusDir::Up,
                state: open_with_mru,
                expected: None,
            },
            FocusCase {
                desc: "editor, Left, no neighbour, drawer open -> NOT the drawer",
                origin: BOTTOM,
                dir: FocusDir::Left,
                state: open_with_mru,
                expected: None,
            },
            FocusCase {
                desc: "editor, Right, no neighbour, drawer open -> NOT the drawer",
                origin: BOTTOM,
                dir: FocusDir::Right,
                state: open_with_mru,
                expected: None,
            },
        ];

        for case in &cases {
            assert_eq!(
                focus_target(&tree, case.origin, case.dir, case.state, PANE_W, PANE_H),
                case.expected,
                "case: {} (origin {}, dir {:?}, drawer {:?}, mru {:?}). Neovim emits \
                 `<C-w>h/j/k/l` as a PURE intent and declines to act on it, so this \
                 value is the whole focus move: `Some(9)` where `None` was wanted \
                 drops the user into the drawer from the wrong edge, and `None` where \
                 a pane was wanted leaves the press doing nothing at all — which, \
                 from inside the drawer, means no exit.",
                case.desc,
                case.origin,
                case.dir,
                case.state.id(),
                case.state.last_editor()
            );
        }

        // --- The same rows again, at the height an open drawer leaves ---------
        for case in &cases {
            assert_eq!(
                focus_target(
                    &tree,
                    case.origin,
                    case.dir,
                    case.state,
                    PANE_W,
                    EDITOR_REGION_H
                ),
                case.expected,
                "case: {} (origin {}, dir {:?}, drawer {:?}, mru {:?}) at the REDUCED height \
                 {EDITOR_REGION_H}, where the pass above used {PANE_H}. The row answered \
                 differently at the two heights, so `focus_target` is no longer scale-invariant \
                 and WHICH height the caller passes has become observable. That is the premise \
                 `focus_target`'s doc rests on — it takes the raw pane-area size rather than the \
                 editor height, on the grounds that `pane_rects` feeds only ordering here. Either \
                 thread the editor height through from `pane_view` and delete that paragraph, or \
                 restore whatever made the tiling scale with its bounds.",
                case.desc,
                case.origin,
                case.dir,
                case.state.id(),
                case.state.last_editor()
            );
        }
    }

    // -- DrawerState --------------------------------------------------------

    /// Pins [`DrawerState`]'s two write rules: [`DrawerState::record_active`]
    /// records only a live editor LEAF, and [`DrawerState::clear`] empties BOTH
    /// fields.
    ///
    /// **This proves the rule, not the wiring.** Nothing here says
    /// `reset_after_restart` calls `clear()`, and no edit to this module can make it
    /// say so. That wiring has exactly one falsifier — the cage smoke that opens the
    /// drawer, kills the captive Neovim, waits for the in-place restart and presses
    /// the chord again — because a restart leaves the dead drawer's grid and
    /// `WinInfo` in the snapshot, so `detect_closes` can never fire for it. A
    /// surviving `drawer` id would keep `apply_drawer_open`'s `is_open()` guard true
    /// against a dead Neovim's pane, and the drawer could never be opened again for
    /// the rest of the session.
    ///
    /// FALSIFYING EDITS:
    ///
    /// - Make `clear()` reset only the MRU: `id()` is `Some(7)` after the clear,
    ///   where `None` was wanted.
    /// - Make `clear()` reset only the drawer: `last_editor()` survives as `Some(3)`.
    /// - Drop `!self.is_drawer(pane)` from `record_active`: the MRU becomes the
    ///   drawer itself, so `<C-w>k` from the drawer would target the drawer.
    /// - Drop the `is_leaf` guard: the MRU becomes a non-leaf pane, and
    ///   `drawer_up_target`'s liveness check is the only thing left between it and a
    ///   focus move to a pane that is not on screen.
    #[test]
    fn drawer_state_records_only_editor_leaves_and_clear_empties_both() {
        /// The pane bound to the drawer.
        const DRAWER_ID: PaneId = 7;
        /// A live editor leaf.
        const EDITOR_ID: PaneId = 3;
        /// A pane that is in `panes` but is NOT a leaf of the tree.
        const NON_LEAF_ID: PaneId = 5;

        let mut state = DrawerState::new();
        assert_eq!(
            state.id(),
            None,
            "a fresh DrawerState has no drawer bound; a `Some` here would make \
             `apply_drawer_open`'s idempotence guard true before any drawer exists, \
             so the first chord press would do nothing"
        );
        assert!(
            !state.is_open(),
            "a fresh DrawerState is not open (is_open must agree with id)"
        );
        assert!(
            !state.is_drawer(DRAWER_ID),
            "with no drawer bound, NO pane is the drawer — including the id that will \
             later be bound to it"
        );
        assert_eq!(
            state.last_editor(),
            None,
            "a fresh DrawerState has recorded no editor pane"
        );

        state.bind(DRAWER_ID);
        assert_eq!(
            state.id(),
            Some(DRAWER_ID),
            "bind must publish the drawer's pane id, which is what `focus_target`'s \
             bridge arm hands back"
        );
        assert!(state.is_open(), "a bound drawer is open");
        assert!(
            state.is_drawer(DRAWER_ID),
            "the bound pane IS the drawer, which is what makes `focus_target` take \
             the drawer branch for it"
        );
        assert!(
            !state.is_drawer(EDITOR_ID),
            "an editor pane is NOT the drawer, even while one is open"
        );

        state.record_active(EDITOR_ID, true);
        assert_eq!(
            state.last_editor(),
            Some(EDITOR_ID),
            "a live editor LEAF becoming active is exactly what the MRU records — it \
             is where `<C-w>k` from the drawer returns to"
        );

        state.record_active(DRAWER_ID, true);
        assert_eq!(
            state.last_editor(),
            Some(EDITOR_ID),
            "the DRAWER becoming active must NOT overwrite the MRU. Recording it \
             would make `drawer_up_target` answer the drawer itself, so `<C-w>k` \
             pressed inside the drawer would land back in the drawer"
        );

        state.record_active(NON_LEAF_ID, false);
        assert_eq!(
            state.last_editor(),
            Some(EDITOR_ID),
            "a pane that is not a live tree leaf must NOT overwrite the MRU, or the \
             remembered target can be a pane that is not on screen"
        );

        state.clear();
        assert_eq!(
            state.id(),
            None,
            "clear() must empty the BOUND DRAWER. A surviving id keeps \
             `apply_drawer_open`'s `is_open()` guard true against a pane that no \
             longer exists, and the drawer can never be opened again"
        );
        assert!(
            !state.is_open(),
            "a cleared DrawerState is closed (is_open must agree with id)"
        );
        assert_eq!(
            state.last_editor(),
            None,
            "clear() must empty the MRU as well. A restart replaces every pane id in \
             the window, so a surviving last_editor names a pane that no longer \
             exists"
        );
    }
}
