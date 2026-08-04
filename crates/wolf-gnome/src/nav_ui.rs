//! The pure, GTK-free rulebook for PER-PANE NAVIGATION in `wolf-gnome`: which
//! window a Back/Forward press acts on, whether the press may be issued at all,
//! and whether each of the two toolbar buttons is sensitive.
//!
//! The navigation counterpart of [`crate::liveness_ui`]. Data in, data out — no
//! widget type, no `WolfCore`, no display — which is exactly what lets the whole
//! rulebook be unit-tested headlessly, the same way [`crate::command_action`],
//! [`crate::geometry`] and [`crate::cursor`] keep their decisions pure. The widget
//! work of carrying the answers out lives in [`crate::shell`].
//!
//! ## It carries no window assumption
//!
//! Every function here RECEIVES `active_win` as an argument rather than reading it
//! from anywhere. `wolf-core`'s `Snapshot::active_win` is per-core and there is one
//! core per window, so a seam that takes the handle already generalises to many
//! windows; `state_seam::snapshot_state` remains the one documented
//! single-window/single-tab site. [`nav_query_win`] is the single place the "which
//! window" question is answered, and both the press path and the button-refresh
//! path go through it, so the two cannot diverge on a focus change.
//!
//! ## The button/press asymmetry
//!
//! A SENSITIVE BUTTON IMPLIES AN ACCEPTED PRESS; the converse is deliberately
//! false. [`nav_buttons`] answers `{false, false}` for every input [`nav_press`]
//! would refuse, so no button ever offers an action the press path would decline.
//! It does NOT follow that an accepted press means a sensitive button: a live core
//! focused on a pane with nothing behind it accepts the press and desensitizes both
//! buttons, because the press is already a cheap no-op there — core's
//! `NavHistory::back` answers `None` and no RPC is issued at all.
//!
//! That asymmetry is why the BUTTONS desensitize while the `gio` actions stay
//! ENABLED, which is the opposite of `win.go-to-file`'s empty-state treatment: a
//! disabled action lets its accelerator fall through to Neovim, and `<Alt>Left`
//! forwarded to an unmapped Neovim splits into `<Esc>` + `<Left>` — the destruction
//! that CAPTURING the chord exists to prevent
//! ([`crate::actions::ACTION_TABLE`]'s own note).
//!
//! ## The blocking call
//!
//! A `Move` decision ships as a BLOCKING RPC on the GTK main thread, so
//! [`nav_press`] is liveness-gated exactly the way
//! [`crate::buffer_picker::open_decision`] is, and for the same reason. The
//! inventory of blocking calls and the reasoning behind the gate live in
//! `crates/wolf-gnome/DECISIONS.md`, "Crash and stall recovery", and are
//! deliberately not restated here.
//!
//! ## Totality is the contract
//!
//! [`nav_press`] matches `Option<Liveness>` with **no `_` arm**, so appending a
//! variant to [`wolf_core::Liveness`] is an `E0004` compile error here until it is
//! classified — the same guard [`crate::buffer_picker::open_decision`] and
//! [`crate::liveness_ui`] rely on. `Option<Liveness>` (not `Liveness`) is the input
//! type precisely so the ABSENT core, which the window's close handler leaves
//! behind, is a classified arm rather than an early return no headless test could
//! see.

use wolf_core::Liveness;

/// Core's `history_can_back` / `history_can_forward` answer for ONE pane.
///
/// Both are RPC-free reads of the navigation mutex, so a caller may take them on
/// every render wake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavAvailability {
    /// Whether the pane has an older entry to go Back to.
    pub can_back: bool,
    /// Whether the pane has a newer entry to go Forward to.
    pub can_forward: bool,
}

/// Whether each of the two toolbar navigation buttons is sensitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavButtons {
    /// Whether the Back button is sensitive.
    pub back: bool,
    /// Whether the Forward button is sensitive.
    pub forward: bool,
}

/// What pressing Back or Forward does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavPress {
    /// Issue the BLOCKING history move on this window.
    Move(i64),
    /// Do nothing and issue no RPC; the string names why, as a mid-sentence clause
    /// (the [`crate::command_action`] / [`crate::buffer_picker`] convention).
    Refuse(&'static str),
}

/// The window a navigation press acts on, or `None` when there is none.
///
/// `active_win.filter(|w| *w != 0)`, following
/// [`crate::buffer_picker::switch_target`]'s rule. Be honest about the strength of
/// the `0` guard: core would answer `false` for handle `0` anyway
/// (`NavHistory::can_back` looks the handle up and finds nothing), so this is a
/// STATED RULE rather than a live hazard — but it is the single place the "which
/// window" question is answered, so the press path and the refresh path cannot
/// disagree about it.
#[must_use]
pub fn nav_query_win(active_win: Option<i64>) -> Option<i64> {
    active_win.filter(|win| *win != 0)
}

/// Whether a Back/Forward press may be issued at all, given the core's liveness
/// reading (or `None` when there is no core) and the focused window.
///
/// TOTAL by construction: a `match` over `Option<Liveness>` with **no `_` arm**
/// (see the module docs). Only [`Liveness::Alive`] with a window
/// [`nav_query_win`] accepts yields [`NavPress::Move`]; every other input refuses,
/// prints its reason, and issues no RPC.
#[must_use]
pub fn nav_press(liveness: Option<Liveness>, active_win: Option<i64>) -> NavPress {
    match liveness {
        Some(Liveness::Alive) => match nav_query_win(active_win) {
            Some(win) => NavPress::Move(win),
            None => NavPress::Refuse(
                "no editor pane is focused, so there is no per-pane history to move through",
            ),
        },
        Some(Liveness::NotResponding) => NavPress::Refuse(
            "the captive Neovim is already known not to be responding, and a history move is a \
             blocking RPC that would freeze the window for another full timeout budget",
        ),
        Some(Liveness::Dead) => NavPress::Refuse(
            "the captive Neovim has died, so there is no pane left to move and an RPC against a \
             dead handle is not reliably fast",
        ),
        None => NavPress::Refuse(
            "the core has already been taken out by the window's close handler, so there is \
             nothing left to navigate",
        ),
    }
}

/// Whether each toolbar navigation button is sensitive.
///
/// `{false, false}` unless [`nav_press`] would ACCEPT for the same
/// `(liveness, active_win)`; otherwise it mirrors `history`. A `history` of `None`
/// — the caller did not ask core, or asked about no window at all — likewise yields
/// `{false, false}`, so a caller that did not ask cannot produce a sensitive
/// button.
#[must_use]
pub fn nav_buttons(
    liveness: Option<Liveness>,
    active_win: Option<i64>,
    history: Option<NavAvailability>,
) -> NavButtons {
    const BOTH_INSENSITIVE: NavButtons = NavButtons {
        back: false,
        forward: false,
    };

    if !matches!(nav_press(liveness, active_win), NavPress::Move(_)) {
        return BOTH_INSENSITIVE;
    }
    match history {
        Some(history) => NavButtons {
            back: history.can_back,
            forward: history.can_forward,
        },
        None => BOTH_INSENSITIVE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    // =========================================================================
    // The whole input domain
    // =========================================================================
    //
    // Several rules below are stated over EVERY input rather than over one table
    // row, so that a failure NAMES the rule instead of quoting an index — the
    // `liveness_ui` / `buffer_picker` convention.

    /// A real nvim window handle. Deliberately not `0` (which [`nav_query_win`]
    /// refuses) and not `1` (the outer/global grid id), so a result that had
    /// fabricated a handle rather than passing this one through is visible.
    const WIN: i64 = 7;

    /// A SECOND real window handle, for the per-pane rule. Distinct from [`WIN`]
    /// and from every other integer in this module, so
    /// [`the_buttons_follow_the_focused_pane`] cannot confuse the two panes.
    const OTHER_WIN: i64 = 12;

    /// The handle [`nav_query_win`] refuses, and `wolf-ffi`'s
    /// `active_win.unwrap_or(0)` sentinel.
    const REFUSED_WIN: i64 = 0;

    /// Every input [`nav_press`] is total over on its LIVENESS axis: the three
    /// readings a core can produce, plus the ABSENT core (`None`) the window's
    /// close handler leaves behind.
    const ALL_LIVENESS_INPUTS: [Option<Liveness>; 4] = [
        Some(Liveness::Alive),
        Some(Liveness::NotResponding),
        Some(Liveness::Dead),
        None,
    ];

    /// Every shape `Snapshot::active_win` can take: a real handle, the refused `0`
    /// sentinel, and no focused window at all.
    const ALL_WIN_INPUTS: [Option<i64>; 3] = [Some(WIN), Some(REFUSED_WIN), None];

    /// The pane has been navigated nowhere: neither direction is available.
    const NO_HISTORY: NavAvailability = NavAvailability {
        can_back: false,
        can_forward: false,
    };

    /// The cursor is at the BOTTOM of the stack: nothing behind, something ahead.
    const AT_THE_BOTTOM: NavAvailability = NavAvailability {
        can_back: false,
        can_forward: true,
    };

    /// The cursor is at the TOP of the stack: something behind, nothing ahead.
    const AT_THE_TOP: NavAvailability = NavAvailability {
        can_back: true,
        can_forward: false,
    };

    /// The cursor is MID-STACK: both directions available.
    const MID_STACK: NavAvailability = NavAvailability {
        can_back: true,
        can_forward: true,
    };

    /// Every shape the caller's history reading can take: the four
    /// [`NavAvailability`] values, plus the `None` of a caller that did not ask.
    const ALL_HISTORY_INPUTS: [Option<NavAvailability>; 5] = [
        None,
        Some(NO_HISTORY),
        Some(AT_THE_BOTTOM),
        Some(AT_THE_TOP),
        Some(MID_STACK),
    ];

    /// Both buttons insensitive — the answer for every refused press and for every
    /// pane with nothing to navigate to.
    const BOTH_OFF: NavButtons = NavButtons {
        back: false,
        forward: false,
    };

    /// Both buttons sensitive.
    const BOTH_ON: NavButtons = NavButtons {
        back: true,
        forward: true,
    };

    /// Back off, Forward on — the pane sits at the bottom of its stack.
    const FORWARD_ONLY: NavButtons = NavButtons {
        back: false,
        forward: true,
    };

    /// Back on, Forward off — the pane sits at the top of its stack.
    const BACK_ONLY: NavButtons = NavButtons {
        back: true,
        forward: false,
    };

    // =========================================================================
    // nav_query_win
    // =========================================================================

    /// One [`nav_query_win`] table row.
    struct QueryCase {
        desc: &'static str,
        active_win: Option<i64>,
        expected: Option<i64>,
    }

    /// Which window the seam answers the navigation question about.
    ///
    /// The `Some(0)` row is the one that discriminates the guard: deleting
    /// `.filter(|w| *w != 0)` turns it into `Some(0)`, and every caller would then
    /// ask core about a handle that names no pane.
    #[test]
    fn nav_query_win_table() {
        let cases = [
            QueryCase {
                desc: "no focused window at all => no window to navigate",
                active_win: None,
                expected: None,
            },
            QueryCase {
                desc: "the 0 sentinel is refused as firmly as None, per switch_target's rule",
                active_win: Some(REFUSED_WIN),
                expected: None,
            },
            QueryCase {
                desc: "a real handle is passed THROUGH, never derived or rewritten",
                active_win: Some(WIN),
                expected: Some(WIN),
            },
        ];

        for case in &cases {
            assert_eq!(
                nav_query_win(case.active_win),
                case.expected,
                "nav_query_win({:?}) must be {:?} — {}. A handle of 0 is `wolf-ffi`'s \
                 \"no window\" sentinel (`active_win.unwrap_or(0)`), so answering Some(0) here \
                 sends every later question about a pane the user never focused; rewriting a real \
                 handle would navigate the WRONG pane, which is worse than navigating none.",
                case.active_win,
                case.expected,
                case.desc
            );
        }

        // Non-vacuity: with a `nav_query_win` of `|_| None` the two refusal rows
        // above pass and the whole seam silently stops navigating anything.
        assert_eq!(
            nav_query_win(Some(WIN)),
            Some(WIN),
            "nav_query_win must ACCEPT a real handle. A function that refused everything would \
             satisfy both refusal rows above while making Back and Forward permanently dead, and \
             nothing else in this module would notice."
        );
    }

    // =========================================================================
    // nav_press
    // =========================================================================

    /// What a [`PressCase`] row expects [`nav_press`] to return.
    ///
    /// A refusal is matched on its CLASS, not on its reason string: the reasons are
    /// documentation, and their real contract (non-empty, distinct, clause-shaped)
    /// is asserted by [`nav_press_reasons_are_distinct_and_non_empty`] — the
    /// `buffer_picker::tests::WantOpen` precedent.
    #[derive(Clone, Copy)]
    enum WantPress {
        /// `NavPress::Move(win)` — the BLOCKING history move is issued on `win`.
        Move(i64),
        /// `NavPress::Refuse(_)` — no RPC at all.
        Refuse,
    }

    /// One [`nav_press`] table row.
    struct PressCase {
        desc: &'static str,
        liveness: Option<Liveness>,
        active_win: Option<i64>,
        want: WantPress,
    }

    /// Pins the press decision for EVERY `(Option<Liveness>, Option<i64>)` input.
    ///
    /// The row-count assertion is what makes the table non-vacuous: the domain is
    /// four liveness inputs times three window shapes, so a row that quietly went
    /// missing cannot hide behind the eleven that remain. The row most likely to be
    /// dropped is `NotResponding`, whose refusal is the only thing that stops every
    /// later `<Alt>Left` re-blocking the GTK main loop against a wedged Neovim.
    #[test]
    fn nav_press_table() {
        let cases = [
            // --- A LIVE core: the only arm that may ever issue the RPC ---------
            PressCase {
                desc: "a live core with a real focused window is the ONLY accepted input",
                liveness: Some(Liveness::Alive),
                active_win: Some(WIN),
                want: WantPress::Move(WIN),
            },
            PressCase {
                desc: "a live core focused on the 0 sentinel refuses, per nav_query_win",
                liveness: Some(Liveness::Alive),
                active_win: Some(REFUSED_WIN),
                want: WantPress::Refuse,
            },
            PressCase {
                desc: "a live core with no focused window has no pane to navigate",
                liveness: Some(Liveness::Alive),
                active_win: None,
                want: WantPress::Refuse,
            },
            // --- A WEDGED core: blocking on it is the entire hazard ------------
            PressCase {
                desc: "a wedged core refuses even with a real window",
                liveness: Some(Liveness::NotResponding),
                active_win: Some(WIN),
                want: WantPress::Refuse,
            },
            PressCase {
                desc: "a wedged core with the 0 sentinel refuses",
                liveness: Some(Liveness::NotResponding),
                active_win: Some(REFUSED_WIN),
                want: WantPress::Refuse,
            },
            PressCase {
                desc: "a wedged core with no window refuses",
                liveness: Some(Liveness::NotResponding),
                active_win: None,
                want: WantPress::Refuse,
            },
            // --- A DEAD core: an RPC against a dead handle is not reliably fast -
            PressCase {
                desc: "a dead core refuses even with a real window",
                liveness: Some(Liveness::Dead),
                active_win: Some(WIN),
                want: WantPress::Refuse,
            },
            PressCase {
                desc: "a dead core with the 0 sentinel refuses",
                liveness: Some(Liveness::Dead),
                active_win: Some(REFUSED_WIN),
                want: WantPress::Refuse,
            },
            PressCase {
                desc: "a dead core with no window refuses",
                liveness: Some(Liveness::Dead),
                active_win: None,
                want: WantPress::Refuse,
            },
            // --- NO core at all: the post-shutdown shape ------------------------
            PressCase {
                desc: "no core (the close handler took it out) refuses with a real window",
                liveness: None,
                active_win: Some(WIN),
                want: WantPress::Refuse,
            },
            PressCase {
                desc: "no core with the 0 sentinel refuses",
                liveness: None,
                active_win: Some(REFUSED_WIN),
                want: WantPress::Refuse,
            },
            PressCase {
                desc: "no core and no window refuses",
                liveness: None,
                active_win: None,
                want: WantPress::Refuse,
            },
        ];

        assert_eq!(
            cases.len(),
            ALL_LIVENESS_INPUTS.len() * ALL_WIN_INPUTS.len(),
            "the nav_press table must carry exactly one row per (liveness, active_win) input it is \
             total over: {} liveness inputs (three readings plus the absent core) times {} window \
             shapes (a real handle, the 0 sentinel, and none) is {} rows, not {}. A missing row \
             means an arm nothing checks.",
            ALL_LIVENESS_INPUTS.len(),
            ALL_WIN_INPUTS.len(),
            ALL_LIVENESS_INPUTS.len() * ALL_WIN_INPUTS.len(),
            cases.len()
        );

        for case in &cases {
            let got = nav_press(case.liveness, case.active_win);
            match case.want {
                WantPress::Move(win) => assert_eq!(
                    got,
                    NavPress::Move(win),
                    "case: {} — nav_press({:?}, {:?}) returned {got:?}, but this input MUST be \
                     Move({win}). A refusal here makes Alt+Left and Alt+Right do nothing at all on \
                     a perfectly healthy editor, and prints a reason for a hazard that is not \
                     present.",
                    case.desc,
                    case.liveness,
                    case.active_win
                ),
                WantPress::Refuse => assert!(
                    matches!(got, NavPress::Refuse(_)),
                    "case: {} — nav_press({:?}, {:?}) returned {got:?}, but this input MUST refuse. \
                     A Move here funnels through `WolfCore::history_back`/`history_forward` into \
                     `block_on_rpc` — a BLOCKING call on the GTK main thread — against a core that \
                     is known not to answer or a window that names no pane: every press then \
                     freezes the window for a full `rpc_timeout` budget, which is the exact \
                     regression this guard exists to prevent.",
                    case.desc,
                    case.liveness,
                    case.active_win
                ),
            }
        }
    }

    /// The rule stated over the WHOLE domain rather than over one table row, so a
    /// failure names the rule instead of quoting an index. Mirrors
    /// `buffer_picker::tests::only_a_live_core_is_snapshotted`.
    ///
    /// It also pins that the accepted window is the one that was PASSED IN: a
    /// derived or fabricated handle would navigate a pane the user never focused,
    /// which is worse than navigating none.
    #[test]
    fn only_a_live_core_with_a_real_window_may_move() {
        let mut moving: Vec<(Option<Liveness>, Option<i64>)> = Vec::new();

        for liveness in ALL_LIVENESS_INPUTS {
            for active_win in ALL_WIN_INPUTS {
                let NavPress::Move(win) = nav_press(liveness, active_win) else {
                    continue;
                };
                moving.push((liveness, active_win));

                assert_eq!(
                    Some(win),
                    active_win,
                    "nav_press({liveness:?}, {active_win:?}) accepted a move on window {win}, which \
                     is not the handle it was given. The window is passed THROUGH, never derived: \
                     a different handle here sends the history move into some other pane, so the \
                     user's Back lands in a pane they never focused."
                );
                assert_ne!(
                    win, REFUSED_WIN,
                    "nav_press({liveness:?}, {active_win:?}) accepted a move on window 0. That is \
                     `wolf-ffi`'s \"no window\" sentinel, so core resolves it to whichever window \
                     Neovim happens to consider current — see nav_query_win's `.filter(|w| *w != \
                     0)`."
                );
            }
        }

        let expected = [(Some(Liveness::Alive), Some(WIN))];
        assert_eq!(
            moving.as_slice(),
            expected.as_slice(),
            "exactly ONE of the twelve (liveness, active_win) inputs may issue the blocking history \
             move — a core reading Alive, focused on a real window handle. An EXTRA entry means the \
             move is issued against a core already known to be wedged or dead (so every press after \
             the first stall re-freezes the main loop) or against a window handle that names no \
             pane. An entry MISSING means Back and Forward can never work at all, since Alive with \
             a focused window is what a healthy editor always reports."
        );
    }

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a mid-sentence
    /// clause. Copied from `buffer_picker.rs`'s helper of the same name, including
    /// its deliberate allowance for an ALL-CAPS opener such as `GNOME`.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    /// The four refusal reasons are non-empty, mutually DISTINCT, and read as a
    /// mid-sentence clause.
    ///
    /// Distinctness is the load-bearing half, for the same reason it is in
    /// `buffer_picker::refuse_reasons_are_distinct_and_non_empty`: the reason is the
    /// whole of what a reader sees when the chord appears to do nothing, and two
    /// arms sharing a string make "the editor is wedged" indistinguishable from
    /// "the editor has crashed" — two states with completely different routes back,
    /// one of which resolves by itself.
    ///
    /// The `Some(0)` window is deliberately NOT probed here. It may legitimately
    /// share the no-window reason (both mean "there is no pane to act on"), so
    /// demanding a fifth distinct string would pin an implementation choice rather
    /// than a contract. That it REFUSES at all is pinned by [`nav_press_table`].
    #[test]
    fn nav_press_reasons_are_distinct_and_non_empty() {
        let probes = [
            (
                "a wedged (NotResponding) core",
                Some(Liveness::NotResponding),
                Some(WIN),
            ),
            ("a dead core", Some(Liveness::Dead), Some(WIN)),
            (
                "an absent core, after shutdown took it out",
                None,
                Some(WIN),
            ),
            (
                "a live core with no focused window",
                Some(Liveness::Alive),
                None,
            ),
        ];

        let mut seen: HashSet<&'static str> = HashSet::new();
        for (source, liveness, active_win) in probes {
            let decision = nav_press(liveness, active_win);
            let NavPress::Refuse(reason) = decision else {
                panic!(
                    "{source} produced {decision:?} rather than a refusal, so this test cannot \
                     check the reason it exists to check. Either that arm now issues the blocking \
                     move (in which case `nav_press_table` and \
                     `only_a_live_core_with_a_real_window_may_move` are also red, and one of them \
                     is the finding) or the probe no longer selects the arm it names."
                )
            };

            assert!(
                !reason.trim().is_empty(),
                "the refusal reason for {source} is empty. It is printed as \
                 \"wolf-gnome: navigation not performed: {{reason}}\", so an empty one makes a \
                 deliberate refusal indistinguishable from a press that silently did nothing."
            );
            assert!(
                !is_sentence_cased(reason),
                "the refusal reason for {source} is sentence-cased ({reason:?}). It is \
                 interpolated MID-SENTENCE into the printed diagnostic, exactly as \
                 `command_action.rs`'s and `buffer_picker.rs`'s reasons are, so it must start \
                 lowercase (an ALL-CAPS proper noun such as \"GNOME\" is fine)."
            );
            assert!(
                seen.insert(reason),
                "the refusal reason for {source} ({reason:?}) duplicates another arm's reason. The \
                 reasons must be DISTINCT so the printed line names the real blocker: a wedged \
                 Neovim is recoverable and a dead one is not, an absent core means the window is \
                 already tearing down, and no focused window is not a core problem at all. One \
                 shared string collapses them into a message that cannot be acted on."
            );
        }

        assert_eq!(
            seen.len(),
            probes.len(),
            "the four probed refusals produced {} distinct reason(s), not {}. See the per-probe \
             message above for which pair collapsed.",
            seen.len(),
            probes.len()
        );
    }

    // =========================================================================
    // nav_buttons
    // =========================================================================

    /// One [`nav_buttons`] table row.
    struct ButtonCase {
        desc: &'static str,
        liveness: Option<Liveness>,
        active_win: Option<i64>,
        history: Option<NavAvailability>,
        expected: NavButtons,
    }

    /// The sensitivity decision, one row per named case of the plan's decision
    /// table.
    ///
    /// A `const` item rather than a local so the count assertion in
    /// [`nav_buttons_table`] can name it, and so a later test can walk the same
    /// rows without copying them.
    const BUTTON_CASES: &[ButtonCase] = &[
        ButtonCase {
            desc: "no focused window: nothing to navigate, so both buttons are dead",
            liveness: Some(Liveness::Alive),
            active_win: None,
            history: None,
            expected: BOTH_OFF,
        },
        ButtonCase {
            desc: "a window that has never navigated anywhere: both directions empty",
            liveness: Some(Liveness::Alive),
            active_win: Some(WIN),
            history: Some(NO_HISTORY),
            expected: BOTH_OFF,
        },
        ButtonCase {
            desc: "at the BOTTOM of the stack: nothing behind, something ahead",
            liveness: Some(Liveness::Alive),
            active_win: Some(WIN),
            history: Some(AT_THE_BOTTOM),
            expected: FORWARD_ONLY,
        },
        ButtonCase {
            desc: "at the TOP of the stack: something behind, nothing ahead",
            liveness: Some(Liveness::Alive),
            active_win: Some(WIN),
            history: Some(AT_THE_TOP),
            expected: BACK_ONLY,
        },
        ButtonCase {
            desc: "MID-STACK: both directions available, the only both-on row",
            liveness: Some(Liveness::Alive),
            active_win: Some(WIN),
            history: Some(MID_STACK),
            expected: BOTH_ON,
        },
        ButtonCase {
            desc: "a DEAD core: a rich history buys nothing, the press would refuse",
            liveness: Some(Liveness::Dead),
            active_win: Some(WIN),
            history: Some(MID_STACK),
            expected: BOTH_OFF,
        },
        ButtonCase {
            desc: "a WEDGED core: same, and this is the one that stops the repeat freeze",
            liveness: Some(Liveness::NotResponding),
            active_win: Some(WIN),
            history: Some(MID_STACK),
            expected: BOTH_OFF,
        },
        ButtonCase {
            desc: "NO core at all (post-shutdown): both buttons dead",
            liveness: None,
            active_win: Some(WIN),
            history: Some(MID_STACK),
            expected: BOTH_OFF,
        },
        ButtonCase {
            desc: "the 0 sentinel: refused by nav_query_win, so both buttons are dead",
            liveness: Some(Liveness::Alive),
            active_win: Some(REFUSED_WIN),
            history: Some(MID_STACK),
            expected: BOTH_OFF,
        },
    ];

    /// How many named rows the decision table carries. Written as a literal rather
    /// than as `BUTTON_CASES.len()` so the table is measured against an independent
    /// statement of the contract: a row deleted along with this number would
    /// otherwise be invisible.
    const NAMED_BUTTON_CASES: usize = 9;

    /// Pins the sensitivity of the two toolbar buttons, one row per named case.
    ///
    /// Rows 6 to 9 are the LIVENESS half — each pairs a rich `MID_STACK` history
    /// with an input the press path refuses, so a `nav_buttons` that consulted only
    /// `history` would light both buttons on a dead, wedged, absent or
    /// unfocused core. Rows 2 to 5 are the HISTORY half, all against a live core
    /// and a real window, so a `nav_buttons` that consulted only the press decision
    /// would light both buttons on a pane with nothing to navigate to.
    #[test]
    fn nav_buttons_table() {
        assert_eq!(
            BUTTON_CASES.len(),
            NAMED_BUTTON_CASES,
            "the nav_buttons table must carry the {NAMED_BUTTON_CASES} named rows of the plan's \
             decision table, but carries {}. The rows are not a sample: four of them pair a rich \
             history with a refused press (dead / wedged / absent core, and the 0 sentinel) and \
             four pair a live core with each history reading, so dropping any one leaves a whole \
             half of the rule unmeasured.",
            BUTTON_CASES.len()
        );

        for case in BUTTON_CASES {
            let got = nav_buttons(case.liveness, case.active_win, case.history);
            assert_eq!(
                got, case.expected,
                "nav_buttons({:?}, {:?}, {:?}) must be {:?}, got {got:?} — {}. A button that is \
                 sensitive here offers the user an action the press path would refuse; one that is \
                 insensitive where it should not be makes a working navigation look unavailable, \
                 and the chord is the only way left to reach it.",
                case.liveness, case.active_win, case.history, case.expected, case.desc
            );
        }

        // Every named row of the decision table really is present, so the count
        // above cannot be satisfied by nine copies of one row.
        for liveness in ALL_LIVENESS_INPUTS {
            assert!(
                BUTTON_CASES.iter().any(|case| case.liveness == liveness),
                "no nav_buttons table row covers a liveness input of {liveness:?}, so that arm's \
                 contribution to the sensitivity rule is unmeasured"
            );
        }
        for history in ALL_HISTORY_INPUTS {
            assert!(
                BUTTON_CASES.iter().any(|case| case.history == history),
                "no nav_buttons table row covers a history reading of {history:?}, so that reading's \
                 contribution to the sensitivity rule is unmeasured"
            );
        }

        // --- The `history: None` rule, stated on its own --------------------
        //
        // The decision table's row 1 pairs `history: None` with an ABSENT window, so
        // it cannot tell "no history was read" apart from "no window to read it
        // for". This states the rule where the press would be ACCEPTED, which is the
        // only place it can bite: a caller that skipped the mutex read must not be
        // able to produce a sensitive button out of nothing.
        for liveness in ALL_LIVENESS_INPUTS {
            for active_win in ALL_WIN_INPUTS {
                assert_eq!(
                    nav_buttons(liveness, active_win, None),
                    BOTH_OFF,
                    "nav_buttons({liveness:?}, {active_win:?}, None) lit a button. A `history` of \
                     None means the caller never asked core whether this pane can go anywhere, so \
                     there is no reading to mirror: answering anything but both-insensitive invents \
                     an affordance from an absent measurement."
                );
            }
        }
    }

    // =========================================================================
    // The two rules stated over the whole domain
    // =========================================================================

    /// How many inputs the seam is total over: four liveness readings times three
    /// window shapes times five history readings.
    ///
    /// Written as LITERALS rather than as the product of the three domain consts'
    /// lengths, and that is the whole point of it — the [`NAMED_BUTTON_CASES`]
    /// precedent, and `buffer_picker`'s two independent consts. Counting iterations
    /// of loops over those same three arrays and comparing the answer to their own
    /// product is a TAUTOLOGY: an array that lost an entry shrinks both sides
    /// together, so the check passes while the sweep silently covers less than the
    /// domain it claims to.
    const SWEEP_INPUTS: usize = 4 * 3 * 5;

    /// No button is ever sensitive where [`nav_press`] would refuse, over the WHOLE
    /// 60-input domain.
    ///
    /// This is the button/press asymmetry from the module docs, made executable in
    /// the direction that holds. The CONVERSE is deliberately NOT asserted, and the
    /// block at the end of this test says so with a witness: a live core focused on
    /// a pane with an empty stack ACCEPTS the press and lights neither button,
    /// because the press is already a cheap no-op there.
    ///
    /// The non-vacuity assertion is what keeps the implication from holding for the
    /// wrong reason: `|_, _, _| NavButtons { back: false, forward: false }` would
    /// satisfy every iteration of the loop while making both buttons permanently
    /// dead.
    #[test]
    fn a_button_is_sensitive_only_where_a_press_is_accepted() {
        let mut probed = 0_usize;
        let mut sensitive = 0_usize;

        for liveness in ALL_LIVENESS_INPUTS {
            for active_win in ALL_WIN_INPUTS {
                for history in ALL_HISTORY_INPUTS {
                    probed += 1;
                    let buttons = nav_buttons(liveness, active_win, history);
                    if !buttons.back && !buttons.forward {
                        continue;
                    }
                    sensitive += 1;

                    let press = nav_press(liveness, active_win);
                    assert!(
                        matches!(press, NavPress::Move(_)),
                        "nav_buttons({liveness:?}, {active_win:?}, {history:?}) returned \
                         {buttons:?} — at least one SENSITIVE button — while nav_press for the same \
                         core and window returned {press:?}. A sensitive button that the press path \
                         refuses is a control that does nothing when clicked, and prints a refusal \
                         reason to stderr where the user cannot see it. The two must be decided \
                         from the same rule: nav_buttons answers both-insensitive for every input \
                         nav_press would refuse."
                    );
                }
            }
        }

        assert_eq!(
            probed, SWEEP_INPUTS,
            "the sweep visited {probed} inputs, not the {SWEEP_INPUTS} the seam is total over. It \
             walked {} liveness input(s) x {} window shape(s) x {} history reading(s); \
             whichever of those three is not 4, 3 and 5 is the one that changed. A SHRUNK \
             domain means the rule above is stated over less than the whole input space, and \
             the arm most likely to go missing is the ABSENT core (`None`) — the one that keeps \
             a window whose close handler has already taken the core out from offering a live \
             button. A GROWN one means `wolf_core::Liveness` gained a variant, in which case \
             `nav_press`'s `_`-free match is an E0004 already and SWEEP_INPUTS is the second \
             thing to update.",
            ALL_LIVENESS_INPUTS.len(),
            ALL_WIN_INPUTS.len(),
            ALL_HISTORY_INPUTS.len()
        );
        assert!(
            sensitive > 0,
            "NOT ONE of the {probed} inputs produced a sensitive button, so the implication above \
             held vacuously and this test proves nothing. A nav_buttons that answered \
             both-insensitive unconditionally would pass it while making the toolbar's Back and \
             Forward permanently dead — and `nav_buttons_table` is the test that would be red \
             alongside it."
        );

        // --- The converse, deliberately NOT asserted -------------------------
        //
        // Recorded with a witness rather than left as prose. An ACCEPTED press does
        // NOT imply a sensitive button: a live core focused on a pane whose stack is
        // empty accepts the press (core's `NavHistory::back` answers None and no RPC
        // is ever issued) while both buttons stay insensitive, because there is
        // nothing to offer. Anyone "completing" the equivalence would light both
        // buttons on every freshly-opened pane.
        let press = nav_press(Some(Liveness::Alive), Some(WIN));
        assert!(
            matches!(press, NavPress::Move(_)),
            "the witness for the deliberately-unasserted converse is broken: nav_press(Alive, \
             Some({WIN})) returned {press:?} rather than a Move, so the paragraph below is talking \
             about an input that no longer exists. `nav_press_table` is red too."
        );
        assert_eq!(
            nav_buttons(Some(Liveness::Alive), Some(WIN), Some(NO_HISTORY)),
            BOTH_OFF,
            "an ACCEPTED press with an EMPTY history must still leave both buttons insensitive. \
             This is the witness that the implication asserted above is one-way: a sensitive button \
             implies an accepted press, never the converse. Lighting a button here offers a \
             navigation that would immediately no-op, on every pane the user has just opened."
        );
    }

    /// The buttons are a pure function of the window they are asked about, so a
    /// focus change refreshes them through the same path a repaint does.
    ///
    /// ## What each half discriminates
    ///
    /// The WINDOW-REFUSAL clauses are the discriminators: with the same rich
    /// history, a real handle lights both buttons while the `0` sentinel and an
    /// absent window light neither. A `nav_buttons` that ignored `active_win`
    /// entirely reddens there.
    ///
    /// The differing-answers and re-ask clauses are a CHARACTERIZATION of purity
    /// rather than a discriminator of the `active_win` parameter: two panes with
    /// two different availabilities would give two different answers even from a
    /// seam that read only `history`. What they pin is that nothing is cached
    /// per-window and nothing is mutated — a later "optimisation" that remembered
    /// the last pane's answer would make the buttons show the PREVIOUS pane's
    /// availability for one wake after every focus move.
    #[test]
    fn the_buttons_follow_the_focused_pane() {
        // Pane A is mid-stack; pane B has only ever been navigated backwards out of,
        // so it sits at the bottom of its own stack.
        let a = nav_buttons(Some(Liveness::Alive), Some(WIN), Some(MID_STACK));
        let b = nav_buttons(Some(Liveness::Alive), Some(OTHER_WIN), Some(AT_THE_BOTTOM));

        assert_eq!(
            a, BOTH_ON,
            "pane {WIN} is mid-stack, so both of its buttons must be sensitive; got {a:?}"
        );
        assert_eq!(
            b, FORWARD_ONLY,
            "pane {OTHER_WIN} sits at the bottom of its stack, so Back must be insensitive and \
             Forward sensitive; got {b:?}"
        );
        assert_ne!(
            a, b,
            "the two panes' answers are identical ({a:?}), so the seam is not resolving the \
             availability it was handed for each pane. The buttons are refreshed on every render \
             wake — including the wake a WinEnter fires, which bumps no seq — so identical answers \
             mean a focus move would leave the toolbar showing the other pane's state."
        );

        // Nothing was mutated, and nothing is cached: asking again, in the other
        // order, gives the same two answers.
        assert_eq!(
            nav_buttons(Some(Liveness::Alive), Some(OTHER_WIN), Some(AT_THE_BOTTOM)),
            b,
            "re-asking about pane {OTHER_WIN} gave a different answer the second time, so the seam \
             is carrying state across calls. It must be a pure function of its arguments: the \
             refresh path calls it once per wake, and an answer that depended on call ORDER would \
             show the previously-focused pane's availability for a frame after every focus move."
        );
        assert_eq!(
            nav_buttons(Some(Liveness::Alive), Some(WIN), Some(MID_STACK)),
            a,
            "re-asking about pane {WIN} after asking about pane {OTHER_WIN} gave a different answer, \
             so the intervening call mutated something. Same consequence as above."
        );

        // The discriminating half: the WINDOW is read, not merely accepted. With the
        // same availability, a handle nav_query_win refuses lights nothing.
        assert_eq!(
            nav_buttons(Some(Liveness::Alive), Some(REFUSED_WIN), Some(MID_STACK)),
            BOTH_OFF,
            "the 0 sentinel with a MID_STACK history lit a button. `nav_buttons` must resolve its \
             window through `nav_query_win` rather than trusting `Snapshot::active_win` verbatim: 0 \
             is `wolf-ffi`'s \"no window\" value, so a press there would be sent about a pane the \
             user never focused. A seam that ignored `active_win` entirely fails here and at the \
             next assertion, and nowhere else in this test."
        );
        assert_eq!(
            nav_buttons(Some(Liveness::Alive), None, Some(MID_STACK)),
            BOTH_OFF,
            "an ABSENT focused window with a MID_STACK history lit a button. There is no pane for \
             the press to act on, so the availability describes nothing that can be navigated."
        );

        // And the resolution really is the shared one, so the press path and the
        // refresh path cannot answer the "which window" question differently.
        assert_eq!(
            nav_query_win(Some(WIN)),
            Some(WIN),
            "precondition: nav_query_win must accept pane {WIN}, or the two answers above are \
             about a window the press path would refuse and this test measures nothing"
        );
        assert_eq!(
            nav_query_win(Some(OTHER_WIN)),
            Some(OTHER_WIN),
            "precondition: nav_query_win must accept pane {OTHER_WIN} too"
        );
    }
}
