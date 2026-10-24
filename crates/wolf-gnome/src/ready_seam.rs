//! The pure rulebook behind the test-only READINESS FILE: when a launch's windows
//! accept keystrokes, as a function of values.
//!
//! `WOLF_READY_FILE=<path>` is off by default and exists for the cage smokes. Once
//! the launch's windows are ready for input, the process writes its pid there, and
//! the smoke harness's `wait_ready` sends no key until the file names the pid it
//! launched. No fixed sleep is a sound budget for that moment: launch-to-input is a
//! few hundred milliseconds on an idle host and unbounded on a loaded one.
//!
//! [`launch_readiness`] is the predicate, and each conjunct has a reason:
//!
//! - **The core is `Alive`.** `WolfCore::start` has already attached and run every
//!   setup RPC, and a stalled or dead core would drop or strand typed keys.
//! - **A tab is selected and at least one of its panes is bound.** A pane with no
//!   grid paints nothing a pointer smoke can click.
//! - **No saved layout is pending and no restore owes a page.** Both are consumed on
//!   later wakes with blocking RPCs that would interleave with typed keys. An
//!   abandoned restore ([`restore_owed`]'s `may_create == false`) issues no further
//!   RPC, so it counts as finished.
//! - **Every launch window has painted a frame.** The ordinal of its first
//!   frame-clock `after-paint` is its map order: all windows share one Wayland
//!   connection, so the compositor processes their first commits in that order.
//! - **The last-mapped window has a focus widget, every bound pane's requested grid
//!   matches Neovim's, and its seat has a keyboard.** Sized grids make derived click
//!   targets and `winwidth` readbacks mean what they say. The keyboard is the
//!   delivery precondition: a headless `cage` seat has no keyboard capability until
//!   a virtual keyboard exists, and a key sent before the client has bound one has
//!   nothing to land on, so the first key of the first keyboard is lost. The harness
//!   primes the seat with a key-free `wtype ""`; this conjunct is the client-side
//!   half of that handshake. `gtk::Window::is_active` is no substitute: under `cage`
//!   it turns true only after a real key arrives.
//!
//! Only the last-mapped window must be sized and keyboard-ready. An older window is covered
//! and may get no frame callbacks at all (inferred from wlroots' occlusion handling,
//! not measured), so requiring it be sized could hang.
//!
//! There is no margin after the file is written. If a first keystroke is still lost,
//! a conjunct is missing, and a sleep would only hide which.

use wolf_core::Liveness;

/// The environment variable naming the readiness file.
pub const READY_FILE_ENV: &str = "WOLF_READY_FILE";

/// The prefix of every readiness line on stderr.
pub const LINE_PREFIX: &str = "wolf-ready: ";

/// One tab's pane view, as far as readiness is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneReadiness {
    /// Panes with a resolved Neovim grid.
    pub bound: usize,
    /// Bound panes whose requested grid size Neovim has applied ([`grid_settled`]).
    pub sized: usize,
    /// Whether a saved layout still awaits realization.
    pub layout_pending: bool,
}

/// One launch window, as far as readiness is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowReadiness {
    /// The core's liveness, `None` when the window has no core (or no longer exists).
    pub liveness: Option<Liveness>,
    /// The selected tab's panes, `None` when no tab is selected.
    pub panes: Option<PaneReadiness>,
    /// Whether the window's restore still owes pages ([`restore_owed`]).
    pub restore_owed: bool,
    /// The ordinal of this window's first `after-paint`, `None` before it.
    pub first_paint: Option<u32>,
    /// Whether the window's display's default seat exposes a keyboard device.
    pub has_keyboard: bool,
    /// Whether the toplevel has a focus widget for key events to bubble from.
    pub has_focus_widget: bool,
}

/// The first unmet condition, as [`launch_readiness`] checks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocker {
    /// The launch opened no window.
    NoLaunchWindow,
    /// A launch window's core is missing, stalled or dead.
    CoreNotAlive,
    /// A launch window has no selected tab.
    NoSelectedTab,
    /// A launch window's selected tab has no bound pane.
    NoPaneBound,
    /// A launch window's saved layout has not been realized.
    LayoutPending,
    /// A launch window's restore still owes pages.
    RestoreOwed,
    /// A launch window has not painted a frame.
    NotPainted,
    /// The last-mapped window has no focus widget.
    NoFocusWidget,
    /// A bound pane in the last-mapped window has a grid Neovim has not resized yet.
    GridsUnsized,
    /// The last-mapped window's seat has no keyboard device.
    NoKeyboard,
}

impl Blocker {
    /// Every blocker, in check order.
    pub const ALL: [Blocker; 10] = [
        Blocker::NoLaunchWindow,
        Blocker::CoreNotAlive,
        Blocker::NoSelectedTab,
        Blocker::NoPaneBound,
        Blocker::LayoutPending,
        Blocker::RestoreOwed,
        Blocker::NotPainted,
        Blocker::NoFocusWidget,
        Blocker::GridsUnsized,
        Blocker::NoKeyboard,
    ];

    /// The label the `wolf-ready: waiting on <label>` stderr line carries.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Blocker::NoLaunchWindow => "no-launch-window",
            Blocker::CoreNotAlive => "core-not-alive",
            Blocker::NoSelectedTab => "no-selected-tab",
            Blocker::NoPaneBound => "no-pane-bound",
            Blocker::LayoutPending => "layout-pending",
            Blocker::RestoreOwed => "restore-owed",
            Blocker::NotPainted => "not-painted",
            Blocker::NoFocusWidget => "no-focus-widget",
            Blocker::GridsUnsized => "grids-unsized",
            Blocker::NoKeyboard => "no-keyboard",
        }
    }
}

/// Whether the launch's windows accept keystrokes, or the first unmet condition.
///
/// The check order is fixed: the empty launch, then every window's settle
/// conditions in slice order, then the input conditions of the window with the
/// greatest `first_paint` (the last mapped).
///
/// # Errors
///
/// The first [`Blocker`] whose condition does not hold.
pub fn launch_readiness(windows: &[WindowReadiness]) -> Result<(), Blocker> {
    if windows.is_empty() {
        return Err(Blocker::NoLaunchWindow);
    }
    for window in windows {
        match window.liveness {
            Some(Liveness::Alive) => {}
            Some(Liveness::NotResponding | Liveness::Dead) | None => {
                return Err(Blocker::CoreNotAlive)
            }
        }
        let Some(panes) = window.panes else {
            return Err(Blocker::NoSelectedTab);
        };
        if panes.bound == 0 {
            return Err(Blocker::NoPaneBound);
        }
        if panes.layout_pending {
            return Err(Blocker::LayoutPending);
        }
        if window.restore_owed {
            return Err(Blocker::RestoreOwed);
        }
        if window.first_paint.is_none() {
            return Err(Blocker::NotPainted);
        }
    }
    // Every window has `panes` and `first_paint` by the loop above, so the max is a
    // real window; the `else` arm is unreachable but answers a blocker rather than
    // panicking.
    let Some(input) = windows.iter().max_by_key(|window| window.first_paint) else {
        return Err(Blocker::NoLaunchWindow);
    };
    if !input.has_focus_widget {
        return Err(Blocker::NoFocusWidget);
    }
    if input.panes.is_none_or(|panes| panes.sized != panes.bound) {
        return Err(Blocker::GridsUnsized);
    }
    if !input.has_keyboard {
        return Err(Blocker::NoKeyboard);
    }
    Ok(())
}

/// Whether a pane's requested `(cols, rows)` is the grid size Neovim reports.
///
/// False when either side is missing: a bound pane that has requested nothing yet
/// has not been sized.
#[must_use]
pub fn grid_settled(requested: Option<(u16, u16)>, actual: Option<(usize, usize)>) -> bool {
    match (requested, actual) {
        (Some((req_cols, req_rows)), Some((cols, rows))) => {
            usize::from(req_cols) == cols && usize::from(req_rows) == rows
        }
        _ => false,
    }
}

/// Whether a restore queue still owes pages: it holds layouts AND may still create
/// them. An abandoned queue (`may_create == false`) never creates again, so it owes
/// nothing.
#[must_use]
pub fn restore_owed(queued: usize, may_create: bool) -> bool {
    queued > 0 && may_create
}

/// The path `WOLF_READY_FILE` names, trimmed, or `None` when absent or blank — the
/// rule [`crate::frame_stats::report_file_path`] applies to its own variable, for
/// the same reason.
#[must_use]
pub fn ready_file_path(raw: Option<&str>) -> Option<&str> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// The readiness file's whole contents: the pid and one newline, which is what the
/// harness's `read -r` compares against the pid it launched.
#[must_use]
pub fn ready_file_contents(pid: u32) -> String {
    format!("{pid}\n")
}

/// What one readiness check does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadyStep {
    /// Write the file.
    Write,
    /// Report a blocker that differs from the last one reported.
    Report(Blocker),
    /// Nothing: the blocker is the one already reported.
    Wait,
}

/// The step for this check, given the last blocker reported.
///
/// A blocker is reported once per change rather than once per check, because the
/// checks run on every wake and every frame.
#[must_use]
pub fn next_step(last: Option<Blocker>, now: Result<(), Blocker>) -> ReadyStep {
    match now {
        Ok(()) => ReadyStep::Write,
        Err(blocker) if last != Some(blocker) => ReadyStep::Report(blocker),
        Err(_) => ReadyStep::Wait,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wolf_core::Liveness;

    // Nothing here touches GTK, the environment or a clock, and neither does the
    // seam: every window is a value built below. Expected values (labels, the
    // variable name, the prefix, file contents) are re-typed as literals rather than
    // read back from the seam, because comparing a const with itself holds for
    // whatever value it takes — including one that silently breaks the harness's
    // `wait_ready` or a `grep wolf-ready:` over a red run's stderr.

    /// [`READY_FILE_ENV`] re-typed as a literal. The smoke harness's `cage_command`
    /// sets this exact name; a rename here and not there leaves every smoke waiting
    /// 30 s for a file nobody writes.
    const EXPECT_READY_FILE_ENV: &str = "WOLF_READY_FILE";

    /// [`LINE_PREFIX`] re-typed as a literal. `wait_ready`'s timeout message tells
    /// the reader to look for `wolf-ready:` lines, so this text is the diagnosis path.
    const EXPECT_LINE_PREFIX: &str = "wolf-ready: ";

    /// [`Blocker::ALL`]'s length re-typed as a literal.
    const EXPECT_BLOCKER_COUNT: usize = 10;

    /// A launch window that meets every condition, painted `first_paint`-th.
    ///
    /// Two bound panes rather than one, so `sized` can fall short of `bound` without
    /// reaching zero, and a zero-pane row is distinguishable from an unsized one.
    fn ready_window(first_paint: u32) -> WindowReadiness {
        WindowReadiness {
            liveness: Some(Liveness::Alive),
            panes: Some(PaneReadiness {
                bound: 2,
                sized: 2,
                layout_pending: false,
            }),
            restore_owed: false,
            first_paint: Some(first_paint),
            has_keyboard: true,
            has_focus_widget: true,
        }
    }

    /// One `launch_readiness` row.
    struct LaunchCase {
        desc: &'static str,
        windows: Vec<WindowReadiness>,
        expected: Result<(), Blocker>,
    }

    /// Runs `launch_readiness` over each row, naming the row and the windows on a
    /// mismatch. `meaning` says what a wrong answer means for the smokes.
    fn check_launch_cases(cases: &[LaunchCase], meaning: &str) {
        for case in cases {
            assert_eq!(
                launch_readiness(&case.windows),
                case.expected,
                "case: {} — launch_readiness must answer {:?}. {meaning}\nwindows: {:#?}",
                case.desc,
                case.expected,
                case.windows
            );
        }
    }

    /// T1. A launch whose windows meet every condition is ready.
    ///
    /// The other launch tests all expect an `Err`, so without this one a predicate
    /// that never answers `Ok` would pass every one of them while leaving every smoke
    /// waiting 30 s and exiting 40.
    ///
    /// Falsifying edit: make `launch_readiness` always return
    /// `Err(Blocker::NoKeyboard)` — every row fires.
    #[test]
    fn a_settled_single_window_launch_is_ready() {
        let one_pane = WindowReadiness {
            panes: Some(PaneReadiness {
                bound: 1,
                sized: 1,
                layout_pending: false,
            }),
            ..ready_window(1)
        };
        let three_panes = WindowReadiness {
            panes: Some(PaneReadiness {
                bound: 3,
                sized: 3,
                layout_pending: false,
            }),
            ..ready_window(1)
        };
        let cases = [
            LaunchCase {
                desc: "one bound, sized pane",
                windows: vec![one_pane],
                expected: Ok(()),
            },
            LaunchCase {
                desc: "two bound, sized panes (the baseline the other tests flip)",
                windows: vec![ready_window(1)],
                expected: Ok(()),
            },
            LaunchCase {
                desc: "three bound, sized panes",
                windows: vec![three_panes],
                expected: Ok(()),
            },
            LaunchCase {
                desc: "the paint ordinal's value is irrelevant, only its presence and rank",
                windows: vec![ready_window(0)],
                expected: Ok(()),
            },
        ];
        check_launch_cases(
            &cases,
            "An Err here means the readiness file is never written for a launch that is ready, \
             so every smoke's wait_ready times out and exits 40.",
        );
    }

    /// T2. Each condition, unmet alone, is reported under its own blocker.
    ///
    /// Each row is the baseline with exactly one field changed. The label is what the
    /// app prints on stderr while a smoke waits, so a condition reported under the
    /// wrong name sends whoever reads a red run after the wrong cause.
    ///
    /// Falsifying edits: delete any one conjunct and its row answers `Ok` or a later
    /// blocker. Named case: drop the `bound == 0` check and the zero-pane row answers
    /// `Ok`, because `sized == bound` holds at 0 == 0.
    #[test]
    fn each_unmet_condition_names_its_own_blocker() {
        type Flip = fn(&mut WindowReadiness);
        let rows: [(&str, Flip, Blocker); 11] = [
            (
                "no core at all (liveness None)",
                |w| w.liveness = None,
                Blocker::CoreNotAlive,
            ),
            (
                "the core is stalled (NotResponding)",
                |w| w.liveness = Some(Liveness::NotResponding),
                Blocker::CoreNotAlive,
            ),
            (
                "the core is dead",
                |w| w.liveness = Some(Liveness::Dead),
                Blocker::CoreNotAlive,
            ),
            (
                "no selected tab (panes None)",
                |w| w.panes = None,
                Blocker::NoSelectedTab,
            ),
            (
                "zero bound panes, and so zero sized ones",
                |w| {
                    w.panes = Some(PaneReadiness {
                        bound: 0,
                        sized: 0,
                        layout_pending: false,
                    });
                },
                Blocker::NoPaneBound,
            ),
            (
                "a saved layout is still pending",
                |w| {
                    w.panes = Some(PaneReadiness {
                        bound: 2,
                        sized: 2,
                        layout_pending: true,
                    });
                },
                Blocker::LayoutPending,
            ),
            (
                "the restore still owes pages",
                |w| w.restore_owed = true,
                Blocker::RestoreOwed,
            ),
            (
                "no frame painted yet",
                |w| w.first_paint = None,
                Blocker::NotPainted,
            ),
            (
                "no focus widget",
                |w| w.has_focus_widget = false,
                Blocker::NoFocusWidget,
            ),
            (
                "one of two bound panes not yet resized by Neovim",
                |w| {
                    w.panes = Some(PaneReadiness {
                        bound: 2,
                        sized: 1,
                        layout_pending: false,
                    });
                },
                Blocker::GridsUnsized,
            ),
            (
                "the seat has no keyboard",
                |w| w.has_keyboard = false,
                Blocker::NoKeyboard,
            ),
        ];
        let cases: Vec<LaunchCase> = rows
            .iter()
            .map(|&(desc, flip, blocker)| {
                let mut window = ready_window(1);
                flip(&mut window);
                LaunchCase {
                    desc,
                    windows: vec![window],
                    expected: Err(blocker),
                }
            })
            .collect();
        check_launch_cases(
            &cases,
            "Ok means the file is written while this condition is unmet, so a smoke types into \
             a window that drops or misroutes the keys; a different blocker means the \
             `wolf-ready: waiting on` line names the wrong cause.",
        );
    }

    /// T3. Only the last-mapped window (greatest `first_paint`) must accept input.
    ///
    /// The older window here has no focus widget, unsized grids and no keyboard. It
    /// is covered and may get no frame callbacks, so holding it to the input
    /// conditions could hang every multi-window relaunch. Both slice orders are
    /// listed because slice order is not map order.
    ///
    /// Falsifying edits: pick `windows.last()` instead of the greatest `first_paint`
    /// — the first row fires (`NoFocusWidget`); apply the input checks to every
    /// window — both rows fire.
    #[test]
    fn only_the_last_mapped_window_must_accept_input() {
        let older = WindowReadiness {
            panes: Some(PaneReadiness {
                bound: 2,
                sized: 0,
                layout_pending: false,
            }),
            has_keyboard: false,
            has_focus_widget: false,
            ..ready_window(1)
        };
        let newest = ready_window(2);
        let cases = [
            LaunchCase {
                desc: "slice order [newest (painted 2), older (painted 1)]",
                windows: vec![newest, older],
                expected: Ok(()),
            },
            LaunchCase {
                desc: "slice order [older (painted 1), newest (painted 2)]",
                windows: vec![older, newest],
                expected: Ok(()),
            },
        ];
        check_launch_cases(
            &cases,
            "An Err means the input conditions were applied to a window other than the one \
             painted last: either to every window, which can hang on a covered window that \
             gets no frames, or to the window last in the slice rather than last mapped.",
        );
    }

    /// T4. The last-mapped window must have a keyboard, whatever an older one has.
    ///
    /// Falsifying edit: check `windows.iter().any(|w| w.has_keyboard)` instead of
    /// the input window's own field — both rows answer `Ok`.
    #[test]
    fn the_last_mapped_window_must_have_a_keyboard() {
        let older = ready_window(1);
        let newest = WindowReadiness {
            has_keyboard: false,
            ..ready_window(2)
        };
        let cases = [
            LaunchCase {
                desc: "slice order [older with keyboard, newest without]",
                windows: vec![older, newest],
                expected: Err(Blocker::NoKeyboard),
            },
            LaunchCase {
                desc: "slice order [newest without keyboard, older with]",
                windows: vec![newest, older],
                expected: Err(Blocker::NoKeyboard),
            },
        ];
        check_launch_cases(
            &cases,
            "Ok means an older window's keyboard stood in for the window the compositor \
             focuses, so the first key can reach a client with no keyboard bound and be lost.",
        );
    }

    /// T5. Every launch window, not just the input one, must finish its restore.
    ///
    /// Restore work runs on later wakes in every window and issues blocking RPCs that
    /// interleave with typed keys, so an older window's pending restore delays the
    /// newest window's keys too.
    ///
    /// Falsifying edit: run the settle checks on the input window only — every row
    /// answers `Ok`.
    #[test]
    fn every_launch_window_must_finish_its_restore() {
        let older_layout_pending = WindowReadiness {
            panes: Some(PaneReadiness {
                bound: 2,
                sized: 2,
                layout_pending: true,
            }),
            ..ready_window(1)
        };
        let older_restore_owed = WindowReadiness {
            restore_owed: true,
            ..ready_window(1)
        };
        let older_core_dead = WindowReadiness {
            liveness: Some(Liveness::Dead),
            ..ready_window(1)
        };
        let newest = ready_window(2);
        let cases = [
            LaunchCase {
                desc: "older window's saved layout pending, older first in the slice",
                windows: vec![older_layout_pending, newest],
                expected: Err(Blocker::LayoutPending),
            },
            LaunchCase {
                desc: "older window's saved layout pending, older last in the slice",
                windows: vec![newest, older_layout_pending],
                expected: Err(Blocker::LayoutPending),
            },
            LaunchCase {
                desc: "older window's restore owes pages",
                windows: vec![older_restore_owed, newest],
                expected: Err(Blocker::RestoreOwed),
            },
            LaunchCase {
                desc: "older window's core is dead",
                windows: vec![newest, older_core_dead],
                expected: Err(Blocker::CoreNotAlive),
            },
        ];
        check_launch_cases(
            &cases,
            "Ok means the file is written while a covered window still has restore RPCs to \
             issue, which can interleave with the smoke's first typed keys.",
        );
    }

    /// T6. A launch that opened no window is never ready.
    ///
    /// The second row is the discriminating contrast: one ready window answers `Ok`,
    /// so the first row's answer is about emptiness rather than about every input.
    ///
    /// Falsifying edit: make the `is_empty` arm return `Ok(())` — the empty row
    /// fires. (Deleting the arm alone does not: the max's `else` also answers
    /// `NoLaunchWindow` for an empty slice.)
    #[test]
    fn an_empty_launch_is_never_ready() {
        let cases = [
            LaunchCase {
                desc: "no launch window",
                windows: vec![],
                expected: Err(Blocker::NoLaunchWindow),
            },
            LaunchCase {
                desc: "contrast: one ready launch window",
                windows: vec![ready_window(1)],
                expected: Ok(()),
            },
        ];
        check_launch_cases(
            &cases,
            "Ok for no windows writes the file before the session restore opened anything, \
             so the smoke types into nothing.",
        );
    }

    /// T7. One unpainted window holds readiness even when another has painted.
    ///
    /// `Option`'s order puts `None` below every `Some`, so a predicate that only
    /// looked for the greatest `first_paint` would pick the painted window and never
    /// notice the other.
    ///
    /// Falsifying edit: delete the loop's `first_paint.is_none()` check — both rows
    /// answer `Ok`.
    #[test]
    fn an_unpainted_window_holds_readiness() {
        let unpainted = WindowReadiness {
            first_paint: None,
            ..ready_window(1)
        };
        let painted = ready_window(1);
        let cases = [
            LaunchCase {
                desc: "slice order [unpainted, painted 1]",
                windows: vec![unpainted, painted],
                expected: Err(Blocker::NotPainted),
            },
            LaunchCase {
                desc: "slice order [painted 1, unpainted]",
                windows: vec![painted, unpainted],
                expected: Err(Blocker::NotPainted),
            },
        ];
        check_launch_cases(
            &cases,
            "Ok means a window that has not been mapped yet is ignored: it will map later, \
             take keyboard focus from the window the smoke is already typing into, and the \
             first keys land in the wrong window.",
        );
    }

    /// Pins the fixed check order by fixing the conditions one at a time, in
    /// order, from a window that meets none of them.
    ///
    /// The order decides which label a red run's `wolf-ready: waiting on` line shows
    /// first, and [`Blocker::ALL`] documents itself as being in check order, so the
    /// last assertion ties the two together.
    ///
    /// Falsifying edit: swap the `NoFocusWidget` and `GridsUnsized` checks in
    /// `launch_readiness` — the "focus widget" and "grids" rows fire.
    #[test]
    fn blockers_are_reported_in_check_order() {
        let mut window = WindowReadiness {
            liveness: None,
            panes: None,
            restore_owed: true,
            first_paint: None,
            has_keyboard: false,
            has_focus_widget: false,
        };
        type Fix = fn(&mut WindowReadiness);
        let steps: [(&str, Fix, Result<(), Blocker>); 10] = [
            ("nothing is met", |_| {}, Err(Blocker::CoreNotAlive)),
            (
                "core alive",
                |w| w.liveness = Some(Liveness::Alive),
                Err(Blocker::NoSelectedTab),
            ),
            (
                "a tab with no bound pane, a pending layout and an unsized grid",
                |w| {
                    w.panes = Some(PaneReadiness {
                        bound: 0,
                        sized: 0,
                        layout_pending: true,
                    });
                },
                Err(Blocker::NoPaneBound),
            ),
            (
                "two panes bound, one sized, layout still pending",
                |w| {
                    w.panes = Some(PaneReadiness {
                        bound: 2,
                        sized: 1,
                        layout_pending: true,
                    });
                },
                Err(Blocker::LayoutPending),
            ),
            (
                "layout realized",
                |w| {
                    w.panes = Some(PaneReadiness {
                        bound: 2,
                        sized: 1,
                        layout_pending: false,
                    });
                },
                Err(Blocker::RestoreOwed),
            ),
            (
                "restore finished",
                |w| w.restore_owed = false,
                Err(Blocker::NotPainted),
            ),
            (
                "painted",
                |w| w.first_paint = Some(1),
                Err(Blocker::NoFocusWidget),
            ),
            (
                "focus widget",
                |w| w.has_focus_widget = true,
                Err(Blocker::GridsUnsized),
            ),
            (
                "grids sized",
                |w| {
                    w.panes = Some(PaneReadiness {
                        bound: 2,
                        sized: 2,
                        layout_pending: false,
                    });
                },
                Err(Blocker::NoKeyboard),
            ),
            ("keyboard", |w| w.has_keyboard = true, Ok(())),
        ];
        let mut reported = Vec::new();
        for (desc, fix, expected) in steps {
            fix(&mut window);
            let got = launch_readiness(&[window]);
            assert_eq!(
                got, expected,
                "after fixing \"{desc}\", launch_readiness must answer {expected:?}: the \
                 conditions are checked core, tab, bound pane, layout, restore, paint, focus \
                 widget, grids, keyboard. A different blocker means two checks swapped places, \
                 so a red run's `wolf-ready: waiting on` line names a later condition while an \
                 earlier one is still unmet.\nwindow: {window:#?}"
            );
            if let Err(blocker) = got {
                reported.push(blocker);
            }
        }
        assert_eq!(
            reported.as_slice(),
            &Blocker::ALL[1..],
            "Blocker::ALL is documented as every blocker in check order; after its first entry \
             (NoLaunchWindow, which only an empty slice reaches) it must match the order \
             launch_readiness reported them in above."
        );
    }

    /// One [`grid_settled`] row.
    struct GridCase {
        desc: &'static str,
        requested: Option<(u16, u16)>,
        actual: Option<(usize, usize)>,
        expected: bool,
    }

    /// T8. A grid is settled only when both sides are known and equal in both axes.
    ///
    /// Falsifying edits: answer `requested.is_some()` alone — the missing-actual and
    /// both mismatch rows fire; compare cols only — the row-mismatch row fires.
    #[test]
    fn grid_settled_table() {
        let cases = [
            GridCase {
                desc: "nothing requested, nothing reported",
                requested: None,
                actual: None,
                expected: false,
            },
            GridCase {
                desc: "requested, but Neovim has reported no grid yet",
                requested: Some((80, 24)),
                actual: None,
                expected: false,
            },
            GridCase {
                desc: "Neovim reports a grid the pane has not requested a size for",
                requested: None,
                actual: Some((80, 24)),
                expected: false,
            },
            GridCase {
                desc: "equal in both axes",
                requested: Some((80, 24)),
                actual: Some((80, 24)),
                expected: true,
            },
            GridCase {
                desc: "cols differ",
                requested: Some((80, 24)),
                actual: Some((79, 24)),
                expected: false,
            },
            GridCase {
                desc: "rows differ",
                requested: Some((80, 24)),
                actual: Some((80, 23)),
                expected: false,
            },
            GridCase {
                desc: "cols and rows transposed",
                requested: Some((80, 24)),
                actual: Some((24, 80)),
                expected: false,
            },
        ];
        for case in &cases {
            assert_eq!(
                grid_settled(case.requested, case.actual),
                case.expected,
                "case: {} — grid_settled({:?}, {:?}) must be {}. A true where false is expected \
                 lets readiness through while Neovim still has the old grid size, so a pointer \
                 smoke's derived click lands on the wrong cell and a `winwidth` readback reads \
                 the pre-resize width; false where true is expected holds readiness forever.",
                case.desc,
                case.requested,
                case.actual,
                case.expected
            );
        }
    }

    /// T9. A restore owes pages only while it has queued layouts and may still create
    /// them.
    ///
    /// Falsifying edit: ignore `may_create` (answer `queued > 0`) — the
    /// `(2, false)` row fires: an abandoned restore would hold readiness forever.
    #[test]
    fn restore_owed_table() {
        let cases: [(&str, usize, bool, bool); 5] = [
            ("empty queue that may create", 0, true, false),
            ("empty abandoned queue", 0, false, false),
            ("one layout queued, may create", 1, true, true),
            ("two layouts queued, may create", 2, true, true),
            ("two layouts queued, abandoned", 2, false, false),
        ];
        for (desc, queued, may_create, expected) in cases {
            assert_eq!(
                restore_owed(queued, may_create),
                expected,
                "case: {desc} — restore_owed({queued}, {may_create}) must be {expected}. True \
                 for an abandoned queue (may_create false) blocks readiness on pages that will \
                 never be created, so the smoke exits 40; false for a live queue writes the file \
                 while restore RPCs are still due."
            );
        }
    }

    /// One [`ready_file_path`] row.
    struct FilePathCase {
        desc: &'static str,
        raw: Option<&'static str>,
        expected: Option<&'static str>,
    }

    /// T10. `WOLF_READY_FILE` resolves by `report_file_path`'s rule: absent or blank
    /// is off, anything else is trimmed.
    ///
    /// Falsifying edits: return `raw` untrimmed — the surrounding-whitespace and
    /// trailing-newline rows fire; drop the `is_empty` arm — the empty and
    /// whitespace-only rows fire.
    #[test]
    fn ready_file_path_table() {
        let cases = [
            FilePathCase {
                desc: "absent -> off, the shipped default",
                raw: None,
                expected: None,
            },
            FilePathCase {
                desc: "an empty value -> off, never an attempt to write \"\"",
                raw: Some(""),
                expected: None,
            },
            FilePathCase {
                desc: "whitespace only -> off (it trims to empty)",
                raw: Some("   "),
                expected: None,
            },
            FilePathCase {
                desc: "a plain path passes through unchanged",
                raw: Some("/tmp/wolf-smoke-ready-1-0"),
                expected: Some("/tmp/wolf-smoke-ready-1-0"),
            },
            FilePathCase {
                desc: "surrounding whitespace is trimmed off the value",
                raw: Some("  /tmp/wolf-smoke-ready-1-0  "),
                expected: Some("/tmp/wolf-smoke-ready-1-0"),
            },
            FilePathCase {
                desc: "a trailing newline, which a $(...) leaves behind",
                raw: Some("/tmp/wolf-smoke-ready-1-0\n"),
                expected: Some("/tmp/wolf-smoke-ready-1-0"),
            },
            FilePathCase {
                desc: "interior spaces are part of the path and are kept",
                raw: Some(" /tmp/wolf smoke ready "),
                expected: Some("/tmp/wolf smoke ready"),
            },
            FilePathCase {
                desc: "a relative path is not resolved here",
                raw: Some("ready.pid"),
                expected: Some("ready.pid"),
            },
        ];
        for case in &cases {
            assert_eq!(
                ready_file_path(case.raw),
                case.expected,
                "case: {} — ready_file_path({:?}) must be {:?}. A Some where None is expected \
                 arms the probe with a path that cannot be written; an untrimmed Some writes a \
                 different path from the one the harness polls, so wait_ready times out (exit \
                 40) on a launch that was ready.",
                case.desc,
                case.raw,
                case.expected
            );
        }
    }

    /// T11. The file holds the pid and exactly one newline.
    ///
    /// `wait_ready` reads it with `read -r` and compares the result to the pid it
    /// launched with `=`, so any prefix, suffix or missing newline fails the match.
    ///
    /// Falsifying edits: drop the newline, or add a prefix such as `pid=` — every
    /// row fires.
    #[test]
    fn ready_file_contents_is_the_pid_and_one_newline() {
        let cases: [(u32, &str); 3] = [
            (4242, "4242\n"),
            (1, "1\n"),
            (4_294_967_295, "4294967295\n"),
        ];
        for (pid, expected) in cases {
            assert_eq!(
                ready_file_contents(pid),
                expected,
                "ready_file_contents({pid}) must be exactly {expected:?}. wait_ready's `read -r` \
                 drops one trailing newline and compares the rest to the pid with `=`: a prefix \
                 or extra text never matches (exit 40 on a ready launch), and no newline makes \
                 `read` return non-zero at end of file."
            );
        }
    }

    /// One [`next_step`] row.
    struct StepCase {
        desc: &'static str,
        last: Option<Blocker>,
        now: Result<(), Blocker>,
        expected: ReadyStep,
    }

    /// T12. A blocker is reported once per change, not once per check.
    ///
    /// Falsifying edit: always answer `Report` for an `Err` — the same-blocker row
    /// fires, which in the app would print a stderr line on every wake and frame.
    #[test]
    fn next_step_table() {
        let cases = [
            StepCase {
                desc: "ready with nothing reported yet",
                last: None,
                now: Ok(()),
                expected: ReadyStep::Write,
            },
            StepCase {
                desc: "ready after a blocker was reported",
                last: Some(Blocker::GridsUnsized),
                now: Ok(()),
                expected: ReadyStep::Write,
            },
            StepCase {
                desc: "the first blocker",
                last: None,
                now: Err(Blocker::NotPainted),
                expected: ReadyStep::Report(Blocker::NotPainted),
            },
            StepCase {
                desc: "a blocker different from the last one reported",
                last: Some(Blocker::NotPainted),
                now: Err(Blocker::NoKeyboard),
                expected: ReadyStep::Report(Blocker::NoKeyboard),
            },
            StepCase {
                desc: "the same blocker as the last one reported",
                last: Some(Blocker::NoKeyboard),
                now: Err(Blocker::NoKeyboard),
                expected: ReadyStep::Wait,
            },
        ];
        for StepCase {
            desc,
            last,
            now,
            expected,
        } in cases
        {
            assert_eq!(
                next_step(last, now),
                expected,
                "case: {desc} — next_step({last:?}, {now:?}) must be {expected:?}. Write where \
                 another step is expected writes the file early or never; Report for an \
                 unchanged blocker floods stderr with one line per wake and frame; Wait for a \
                 changed one hides the condition a red run is stuck on."
            );
        }
    }

    /// T13. Every blocker has its own non-empty label, pinned literally.
    ///
    /// The labels are what a red smoke run shows (`wolf-ready: waiting on <label>`)
    /// and what the docs tell a reader to look for.
    ///
    /// Falsifying edits: give two blockers the same label — the distinctness
    /// assertion and that label's row fire; add or drop a variant from `ALL` — the
    /// length assertion fires.
    #[test]
    fn blocker_labels_are_distinct_and_nonempty() {
        let expected: [(Blocker, &str); 10] = [
            (Blocker::NoLaunchWindow, "no-launch-window"),
            (Blocker::CoreNotAlive, "core-not-alive"),
            (Blocker::NoSelectedTab, "no-selected-tab"),
            (Blocker::NoPaneBound, "no-pane-bound"),
            (Blocker::LayoutPending, "layout-pending"),
            (Blocker::RestoreOwed, "restore-owed"),
            (Blocker::NotPainted, "not-painted"),
            (Blocker::NoFocusWidget, "no-focus-widget"),
            (Blocker::GridsUnsized, "grids-unsized"),
            (Blocker::NoKeyboard, "no-keyboard"),
        ];
        assert_eq!(
            Blocker::ALL.len(),
            EXPECT_BLOCKER_COUNT,
            "Blocker::ALL must list all {EXPECT_BLOCKER_COUNT} blockers; a variant missing from \
             it escapes the distinctness check below."
        );
        for (blocker, label) in expected {
            assert!(
                Blocker::ALL.contains(&blocker),
                "Blocker::ALL is missing {blocker:?}, so its label is never checked for \
                 distinctness."
            );
            assert_eq!(
                blocker.label(),
                label,
                "{blocker:?} must be labelled {label:?}: the label is the `wolf-ready: waiting \
                 on` text a red run prints and the docs name."
            );
        }
        for (i, a) in Blocker::ALL.iter().enumerate() {
            assert!(
                !a.label().is_empty(),
                "{a:?} has an empty label, so its stderr line reads `waiting on ` and names \
                 nothing."
            );
            for b in &Blocker::ALL[i + 1..] {
                assert_ne!(
                    a.label(),
                    b.label(),
                    "{a:?} and {b:?} share a label, so a red run's stderr cannot say which of \
                     the two conditions is unmet."
                );
            }
        }
    }

    /// T14. The variable name and the stderr prefix are the contract with the smoke
    /// harness and with whoever reads a red run.
    ///
    /// Falsifying edit: rename either constant's value — its row fires.
    #[test]
    fn the_env_var_and_prefix_are_the_harness_contract() {
        let cases: [(&str, &str, &str); 2] = [
            ("READY_FILE_ENV", READY_FILE_ENV, EXPECT_READY_FILE_ENV),
            ("LINE_PREFIX", LINE_PREFIX, EXPECT_LINE_PREFIX),
        ];
        for (name, actual, expected) in cases {
            assert_eq!(
                actual, expected,
                "{name} must be {expected:?}. READY_FILE_ENV is the name cage_command sets, so \
                 a rename leaves the probe unarmed and every smoke's wait_ready exits 40; \
                 LINE_PREFIX is the text wait_ready's timeout message tells the reader to look \
                 for in stderr."
            );
        }
    }
}
