//! The terminal drawer's pure arithmetic: how the pane area splits between the
//! editor tree and the full-width drawer strip below it, and which pane a
//! `<C-w>h/j/k/l` intent should focus once the drawer is in the picture.
//!
//! GTK-, Pango- and Cairo-free, so the whole rulebook runs in the fast unit suite
//! with no display server, no compositor and no Neovim — exactly as
//! [`crate::geometry`], [`crate::editor_inset`] and [`crate::cell_metrics_math`] do.
//!
//! ## Why the drawer is not a [`PaneTree`] leaf
//!
//! It lives OUTSIDE the tree, which is what forces this module to exist at all.
//! `PaneContainerView.swift` made that choice explicitly *"so the tree keeps its 1:1
//! parity with wolf-gnome"* — the macOS decision was taken FOR this front-end, and
//! the durable reasons (a `term://` string kept out of `state.json`, a divider
//! fraction with nowhere to live, every `neighbor` result left unperturbed) are in
//! `crates/wolf-gnome/DECISIONS.md`. The price is paid here: a pane the tree cannot
//! see needs its own height rule and its own focus bridge.
//!
//! ## This module MERGES two Swift files
//!
//! macOS splits the same rules across `DrawerLayout.swift` (the height) and
//! `DrawerFocusSeam.swift` (the bridge). Both are about forty lines, neither has a
//! consumer outside the drawer, and [`focus_target`] needs both halves in one
//! expression — so they are one module here. A reader diffing against the Swift
//! should expect one file where macOS has two.

use wolf_core::FocusDir;

use crate::layout::{PaneId, PaneTree, Rect};

/// The drawer's fixed share of the pane-area height.
///
/// One third, matching `runtime/nvim/init.lua`'s `math.max(1, floor(lines / 3))`
/// — the height the `wolf_toggle_terminal` Lua helper opens the split at. The two
/// are independent expressions of one number: nvim divides ROWS, this divides
/// PIXELS, and the front-end then drives its own `resize_grid` from the pixels.
///
/// There is no divider drag, so this is a constant rather than a stored weight. A
/// draggable boundary would need somewhere to persist its position, and the drawer
/// is deliberately absent from both the tree and `state.json`.
pub const DRAWER_WEIGHT: f64 = 1.0 / 3.0;

/// How the pane area divides between the editor tree and the drawer strip.
///
/// The editor tree occupies `[0, editor_height]` and the drawer the strip below it;
/// the two always sum to the pane-area height (or to zero, for a degenerate input).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawerSplit {
    /// The editor region's height, in device pixels.
    pub editor_height: f64,
    /// The drawer strip's height, in device pixels; `0.0` when no drawer is open.
    pub drawer_height: f64,
}

/// The all-zero split a degenerate input collapses to.
const COLLAPSED: DrawerSplit = DrawerSplit {
    editor_height: 0.0,
    drawer_height: 0.0,
};

/// Splits `pane_area_height` into the editor region and the drawer strip.
///
/// With no drawer the editor takes the whole height. With one, the drawer takes
/// `round(pane_area_height * weight)` clamped into
/// `[min_drawer_height, pane_area_height]` and the editor takes the remainder, so
/// the two always sum to the input.
///
/// **Two divergences from `DrawerLayout.swift`, both strictly more defensive, both
/// deliberate.** They follow [`crate::cell_metrics_math`]'s precedent of naming a
/// divergence rather than silently carrying it:
///
/// - **The `min_drawer_height` floor.** Swift has none, so a short window yields a
///   drawer thinner than one grid row — a strip that is allocated, painted and
///   focusable while showing no text at all. The caller passes
///   [`crate::editor_inset::min_pane_size`]'s height, which is the same floor every
///   other pane already gets.
/// - **The non-finite guard.** Any non-finite input collapses the WHOLE result to
///   zeros, following [`crate::geometry`]'s whole-result-collapse convention; a
///   non-positive height does the same. `weight` and `min_drawer_height` are
///   included even though `has_drawer == false` would not read them, because a
///   partial answer built from a poisoned term is worse than an obvious zero.
///
/// The lower bound is floored at `0.0` and then capped at `pane_area_height`. The
/// cap is a PANIC GUARD rather than tidiness, exactly as
/// [`crate::geometry::divider_position`]'s is: `f64::clamp` panics outright on an
/// inverted range, and a pane area shorter than one whole pane's minimum is a real
/// transient while a window allocation settles.
#[must_use]
pub fn drawer_split(
    pane_area_height: f64,
    has_drawer: bool,
    weight: f64,
    min_drawer_height: f64,
) -> DrawerSplit {
    if !pane_area_height.is_finite()
        || !weight.is_finite()
        || !min_drawer_height.is_finite()
        || pane_area_height <= 0.0
    {
        return COLLAPSED;
    }
    if !has_drawer {
        return DrawerSplit {
            editor_height: pane_area_height,
            drawer_height: 0.0,
        };
    }
    let low = min_drawer_height.max(0.0).min(pane_area_height);
    let drawer_height = (pane_area_height * weight)
        .round()
        .clamp(low, pane_area_height);
    DrawerSplit {
        editor_height: pane_area_height - drawer_height,
        drawer_height,
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
    /// pane id in the window, so a surviving `last_editor` would name a pane that no
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
/// A verbatim port of `DrawerFocusSeam.bottomMostLeaf`, tie-breaks included: it is
/// the pane physically adjacent to the drawer, so it is where `<C-w>k` lands when
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
/// **Deliberate divergence from `PaneContainerView.applyFocus`: `width`/`height` are
/// the RAW pane-area size, where macOS passes `DrawerLayout.editorHeight`.** Here
/// [`PaneTree::pane_rects`] feeds only ORDERING — `neighbor`'s cross-axis overlap
/// and distance comparisons, and [`bottom_most_leaf`]'s maximum — and both consumers
/// are scale-invariant, because the tiling scales every rect by the same factor.
/// Threading the reduced height through would be unfalsifiable precision: no input
/// exists for which it changes an answer. Said here because a reader diffing the two
/// files would otherwise "fix" it.
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
    //! The drawer rulebook's four tables. Every one of them is arithmetic over
    //! plain numbers and pane ids, so none of it needs GTK, a display server or a
    //! Neovim — the whole module runs in the fast unit tier.
    //!
    //! What these tables CANNOT see, stated once here rather than repeated per
    //! test: whether `pane_view` calls any of it. Each function's rule is pinned
    //! below; each call site's wiring is a cage smoke's job.

    use super::*;
    use crate::layout::Orientation;

    // -- drawer_split -------------------------------------------------------

    /// One third, re-typed here as a LITERAL rather than read from
    /// [`DRAWER_WEIGHT`].
    ///
    /// A table that fed the production constant in and expected numbers derived
    /// from it would hold for whatever value the constant took, including one that
    /// no longer matches `init.lua`'s `floor(lines / 3)`. Stating the third
    /// independently means a change to [`DRAWER_WEIGHT`] fails
    /// [`drawer_split_table`]'s opening guard instead of moving silently with it.
    const THIRD: f64 = 1.0 / 3.0;

    /// A drawer floor typical of what [`crate::editor_inset::min_pane_size`]
    /// produces at the smokes' cell size — the same 63 px
    /// `geometry`'s divider table carries, and the value `pane_view` passes.
    const MIN_DRAWER_PX: f64 = 63.0;

    /// One [`drawer_split`] table row.
    struct SplitCase {
        desc: &'static str,
        height: f64,
        has_drawer: bool,
        weight: f64,
        min_drawer: f64,
        expected_editor: f64,
        expected_drawer: f64,
        /// Whether this row's two expected heights must add up to its `height`.
        ///
        /// Checked BEFORE the call, so it guards the ROW rather than the function:
        /// a row whose literals do not tile the pane area describes a split that
        /// cannot exist, and would assert about nothing. `false` for the rows that
        /// collapse the whole result, where zero is deliberately not the height.
        sums_to_height: bool,
    }

    /// Pins [`drawer_split`]: a closed drawer reserves no height, an open one takes
    /// `round(h * weight)` clamped into `[min_drawer_height, h]`, the two always
    /// tile the pane area, and any degenerate or non-finite input collapses BOTH
    /// fields to zero.
    ///
    /// FALSIFYING EDITS, each of which reddens a named row:
    ///
    /// - Delete the `has_drawer` short-circuit: the "a closed drawer reserves NO
    ///   height" row reports `drawer_height` 200.0 where 0.0 was wanted, at
    ///   `h = 600`.
    /// - Drop the `low` bound from the clamp (`.clamp(0.0, h)`): the floor row
    ///   reports 30.0 rather than 63.0 at `h = 90`. Without this row the
    ///   `min_drawer_height` divergence from `DrawerLayout.swift` is documented and
    ///   unasserted.
    /// - Drop the `.min(pane_area_height)` cap on that lower bound: the predicted
    ///   red is **a panic from inside the standard library**, not an assertion
    ///   failure — `f64::clamp` refuses an inverted range with `min > max, or
    ///   either was NaN. min = 63.0, max = 40.0`. The same shape
    ///   `geometry::divider_position_table` already records, and the reason the cap
    ///   is a panic guard rather than tidiness.
    /// - Drop `.max(0.0)` from it: the negative-weight-and-negative-floor row hands
    ///   the drawer a height of -50.0 and the editor 650.0 out of a 600 px area.
    /// - Replace `.clamp(low, pane_area_height)` with `.max(low)`: the
    ///   weight-past-one row gives the drawer 1200.0 and the editor -600.0.
    /// - Replace `.round()` with `.floor()`: the 605 px row gives 201.0/404.0
    ///   instead of 202.0/403.0.
    /// - Drop `!weight.is_finite()` from the guard: the NaN-weight row comes back
    ///   with BOTH fields `NaN`, because `round(h * NaN).clamp(63.0, 600.0)` is
    ///   `NaN` and the editor is then `600 - NaN`. That is why the guard is
    ///   deliberately broader than "a non-finite HEIGHT": a partial answer built
    ///   from a poisoned term is worse than an obvious zero.
    /// - Drop `!min_drawer_height.is_finite()`: `NaN.max(0.0)` is `0.0` (`f64::max`
    ///   ignores NaN), so that row does NOT go `NaN` — it silently loses the floor
    ///   and returns 200.0/400.0. The `+inf` row loses the whole editor instead,
    ///   returning drawer 600.0.
    /// - Move the non-finite guard AFTER the `has_drawer` short-circuit: the "even
    ///   with NO drawer" row returns editor 600.0 rather than collapsing.
    #[test]
    fn drawer_split_table() {
        // Guard on the FIXTURE, not on the function: every expected number below
        // describes the production regime, and this is the one line tying the two
        // together. `init.lua:1231` opens the split at `floor(lines / 3)`, so a
        // DRAWER_WEIGHT that is not a third means the pixels and the rows have
        // parted company and the table describes a drawer this app never opens.
        assert_eq!(
            DRAWER_WEIGHT, THIRD,
            "DRAWER_WEIGHT is {DRAWER_WEIGHT}, not one third. The rows below are \
             computed at a third because that is what `runtime/nvim/init.lua`'s \
             `math.max(1, math.floor(vim.o.lines / 3))` divides the drawer's ROWS \
             by; changing the pixel weight without that expression makes the GTK \
             strip and the Neovim split different sizes."
        );

        let cases = [
            // --- A degenerate pane area collapses the WHOLE result, matching this
            // --- crate's `geometry` / `editor_inset` convention. The collapse runs
            // --- BEFORE the `has_drawer` branch, so the closed row collapses too.
            SplitCase {
                desc: "a zero pane area collapses both fields",
                height: 0.0,
                has_drawer: true,
                weight: THIRD,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 0.0,
                expected_drawer: 0.0,
                sums_to_height: true,
            },
            SplitCase {
                desc: "a zero pane area collapses with NO drawer as well",
                height: 0.0,
                has_drawer: false,
                weight: THIRD,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 0.0,
                expected_drawer: 0.0,
                sums_to_height: true,
            },
            SplitCase {
                desc: "a negative pane area collapses both fields",
                height: -10.0,
                has_drawer: true,
                weight: THIRD,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 0.0,
                expected_drawer: 0.0,
                sums_to_height: false,
            },
            SplitCase {
                desc: "a NaN pane area collapses both fields",
                height: f64::NAN,
                has_drawer: true,
                weight: THIRD,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 0.0,
                expected_drawer: 0.0,
                sums_to_height: false,
            },
            // --- The ordinary answers, at a pane area whose third is a whole number
            // --- of pixels and comfortably above the floor.
            SplitCase {
                desc: "a closed drawer reserves NO height: the editor takes all of it",
                height: 600.0,
                has_drawer: false,
                weight: THIRD,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 600.0,
                expected_drawer: 0.0,
                sums_to_height: true,
            },
            SplitCase {
                // 600 * (1/3) = 199.999... -> 200.
                desc: "an open drawer takes a third, and the two tile the pane area",
                height: 600.0,
                has_drawer: true,
                weight: THIRD,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 400.0,
                expected_drawer: 200.0,
                sums_to_height: true,
            },
            SplitCase {
                // 605 * (1/3) = 201.666... -> 202, which `.floor()` would call 201.
                desc: "a fractional third ROUNDS rather than truncating",
                height: 605.0,
                has_drawer: true,
                weight: THIRD,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 403.0,
                expected_drawer: 202.0,
                sums_to_height: true,
            },
            // --- The FLOOR: divergence 1 from `DrawerLayout.swift`, which has no
            // --- floor at all. 90 px is a pane area short enough that a third of it
            // --- (30 px) is under one pane's minimum, so the strip would be
            // --- allocated, painted and focusable while showing no text.
            SplitCase {
                desc: "a short pane area floors the drawer at min_drawer_height",
                height: 90.0,
                has_drawer: true,
                weight: THIRD,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 27.0,
                expected_drawer: 63.0,
                sums_to_height: true,
            },
            // --- The INVERTED range, which is a PANIC GUARD rather than tidiness:
            // --- `f64::clamp` panics outright when its lower bound exceeds its
            // --- upper one, and a pane area shorter than one pane's minimum is a
            // --- real transient while a window allocation settles. The drawer takes
            // --- the whole area and the editor is left with nothing, which is the
            // --- only total answer available.
            SplitCase {
                desc: "min_drawer_height above the pane area caps instead of panicking",
                height: 40.0,
                has_drawer: true,
                weight: THIRD,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 0.0,
                expected_drawer: 40.0,
                sums_to_height: true,
            },
            // --- Both ends of the clamp, driven from the WEIGHT rather than from
            // --- the floor, so a dropped bound shows up as a negative height.
            SplitCase {
                // round(600 * 2) = 1200, capped at the pane area.
                desc: "a weight past 1 cannot give the editor a negative height",
                height: 600.0,
                has_drawer: true,
                weight: 2.0,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 0.0,
                expected_drawer: 600.0,
                sums_to_height: true,
            },
            SplitCase {
                // round(600 * -1) = -600, lifted to the floor.
                desc: "a negative weight lifts to min_drawer_height",
                height: 600.0,
                has_drawer: true,
                weight: -1.0,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 537.0,
                expected_drawer: 63.0,
                sums_to_height: true,
            },
            SplitCase {
                // The `.max(0.0)` on the lower bound is what this row is for: with a
                // negative floor AND a negative weight there is nothing else holding
                // the drawer's height at or above zero.
                desc: "a negative floor and a negative weight still cannot go below zero",
                height: 600.0,
                has_drawer: true,
                weight: -1.0,
                min_drawer: -50.0,
                expected_editor: 600.0,
                expected_drawer: 0.0,
                sums_to_height: true,
            },
            // --- Divergence 2: the guard covers `weight` and `min_drawer_height`
            // --- too, not just the height. Narrowing it is the tempting
            // --- "simplification" (neither is read when there is no drawer), and
            // --- these five rows are what make that visible.
            SplitCase {
                desc: "a NaN weight collapses BOTH fields rather than poisoning one",
                height: 600.0,
                has_drawer: true,
                weight: f64::NAN,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 0.0,
                expected_drawer: 0.0,
                sums_to_height: false,
            },
            SplitCase {
                desc: "an infinite weight collapses BOTH fields",
                height: 600.0,
                has_drawer: true,
                weight: f64::INFINITY,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 0.0,
                expected_drawer: 0.0,
                sums_to_height: false,
            },
            SplitCase {
                desc: "a NaN min_drawer_height collapses BOTH fields",
                height: 600.0,
                has_drawer: true,
                weight: THIRD,
                min_drawer: f64::NAN,
                expected_editor: 0.0,
                expected_drawer: 0.0,
                sums_to_height: false,
            },
            SplitCase {
                desc: "an infinite min_drawer_height collapses BOTH fields",
                height: 600.0,
                has_drawer: true,
                weight: THIRD,
                min_drawer: f64::INFINITY,
                expected_editor: 0.0,
                expected_drawer: 0.0,
                sums_to_height: false,
            },
            SplitCase {
                // The guard runs BEFORE the `has_drawer` short-circuit, so a
                // poisoned term collapses the result even on the branch that would
                // never have read it.
                desc: "a NaN weight collapses even with NO drawer",
                height: 600.0,
                has_drawer: false,
                weight: f64::NAN,
                min_drawer: MIN_DRAWER_PX,
                expected_editor: 0.0,
                expected_drawer: 0.0,
                sums_to_height: false,
            },
        ];

        for case in &cases {
            if case.sums_to_height {
                assert_eq!(
                    case.expected_editor + case.expected_drawer,
                    case.height,
                    "row {:?} is malformed, not the function: its expected editor \
                     ({}) plus its expected drawer ({}) is {}, which is not the pane \
                     area ({}) they are supposed to tile.",
                    case.desc,
                    case.expected_editor,
                    case.expected_drawer,
                    case.expected_editor + case.expected_drawer,
                    case.height
                );
            }

            let got = drawer_split(case.height, case.has_drawer, case.weight, case.min_drawer);

            assert_eq!(
                got.drawer_height, case.expected_drawer,
                "case: {} (h {}, has_drawer {}, weight {}, min_drawer {}) — \
                 drawer_height. `round(h * weight)` where 0.0 was wanted means the \
                 `has_drawer` short-circuit or the non-finite collapse is gone; a NaN \
                 means a poisoned term reached the arithmetic instead of collapsing \
                 the whole result; an unclamped value means one of the clamp's two \
                 bounds was dropped.",
                case.desc, case.height, case.has_drawer, case.weight, case.min_drawer
            );
            assert_eq!(
                got.editor_height, case.expected_editor,
                "case: {} (h {}, has_drawer {}, weight {}, min_drawer {}) — \
                 editor_height, which is the pane area LESS the drawer. A negative \
                 value here means the drawer was allowed past the pane area, and a \
                 NaN means the drawer height was NaN — in both cases read the \
                 drawer_height assertion above first.",
                case.desc, case.height, case.has_drawer, case.weight, case.min_drawer
            );
        }
    }

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
    ///   than 3. That clause is a verbatim port of
    ///   `DrawerFocusSeam.bottomMostLeaf`, and it is unreachable through
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
                // Same bottom edge, different x: the LEFT leaf wins, it is listed
                // SECOND, and it carries the GREATER id — so neither "keep the first"
                // nor the id tie-break can produce this answer by accident.
                desc: "a tie on the bottom edge breaks to the SMALLEST x",
                rects: vec![
                    (LESSER_ID, rect(500.0, 300.0, 500.0, 300.0)),
                    (GREATER_ID, rect(0.0, 300.0, 500.0, 300.0)),
                ],
                expected: Some(GREATER_ID),
            },
            BottomCase {
                // Identical rects: only the id can separate them. `pane_rects` never
                // produces this, and that is the point — the clause is a verbatim
                // port and this row is what stops it being deleted as unreachable.
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
    /// Two thirds of [`PANE_H`], which is the alternative input
    /// `PaneContainerView.applyFocus` would pass where [`focus_target`] takes the
    /// raw pane-area size.
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
    /// Every row runs twice: once at the raw pane-area height and once at two
    /// thirds of it, the editor region an open drawer leaves. That second pass is
    /// the only assertion anywhere on this seam's THIRD documented divergence —
    /// [`focus_target`] takes the raw pane-area size where
    /// `PaneContainerView.applyFocus` passes `DrawerLayout.editorHeight`, because
    /// [`PaneTree::pane_rects`] feeds only ORDERING here and both consumers are
    /// scale-invariant. That is exactly the claim a reader diffing the two files
    /// would "fix" by threading the reduced height through, and nothing was
    /// measuring it.
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

        // Guards on the FIXTURE. A `PaneTree` that stacked these two side by side,
        // or ordered them the other way, would make every row below assert about a
        // window shape this front-end never builds.
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

        // Guards on the SECOND pass's height, measured against the real `PaneTree`
        // rather than assumed of it. The first says the reduced height genuinely
        // changes the geometry the seam works over — without it, a height that
        // happened to equal `PANE_H` would make the second pass a re-run of the
        // first and prove nothing. The next two say the tree's own answers are the
        // same at both heights, which is the premise the scale-invariance divergence
        // rests on and the half that is `PaneTree`'s behaviour, not this module's.
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
            // --- Rule 3: an ordinary tree neighbour is the answer, and it takes
            // --- precedence over the bridge — otherwise `<C-w>j` from the top row
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
                "case: {} (origin {}, dir {:?}, drawer {:?}, mru {:?}) at the REDUCED \
                 height {EDITOR_REGION_H}, where the pass above used {PANE_H}. The row \
                 answered differently at the two heights, so `focus_target` is no \
                 longer scale-invariant and WHICH height the caller passes has become \
                 observable. That is the premise the seam's recorded divergence rests \
                 on — it takes the raw pane-area size where \
                 `PaneContainerView.applyFocus` passes `DrawerLayout.editorHeight`, on \
                 the grounds that `pane_rects` feeds only ordering here. Either thread \
                 the editor height through from `pane_view` and delete that paragraph, \
                 or restore whatever made the tiling scale with its bounds.",
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
