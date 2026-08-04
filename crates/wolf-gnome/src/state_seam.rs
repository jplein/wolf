//! The pure, GTK-free APPLICATION-STATE seam: what to restore at launch, and
//! what to persist on each wake.
//!
//! The Rust analogue of the macOS `StateSeam.swift`, plus the write-side rules
//! macOS keeps inside `AppDelegate.snapshotNow`. GTK-free, IO-free, and it never
//! reads `$HOME` or the process cwd — every input is injected, so the whole
//! restore/persist rulebook is unit-testable headlessly with no temp dirs and no
//! window, exactly like [`crate::layout`] and [`crate::workspace`].
//!
//! ## What is deliberately NOT here
//!
//! - **Window POSITION.** Wayland gives a client neither its own position nor
//!   screen geometry, so there is no Linux analogue of macOS's
//!   `restoredWindowFrame` "intersects some screen" check.
//!   [`snapshot_state`] writes `x`/`y` as `0.0` unconditionally and
//!   [`restored_window_size`] returns SIZE only.
//! - **Multiple windows / tabs.** [`snapshot_state`] writes exactly one of each;
//!   `wolf-gnome` is a single-window, single-tab front-end today, and that shape
//!   lives in ONE place so a later unit has to change it deliberately.

use std::path::{Path, PathBuf};

use wolf_core::{Liveness, PaneNode, TabState, WindowState, WolfState};

use crate::pane_layout;
use crate::workspace::{self, LaunchDecision};

/// Everything the launch path needs to decide before any widget is built.
#[derive(Debug, Clone, PartialEq)]
pub struct RestorePlan {
    /// The working directory handed to `wolf_core::CoreConfig`.
    pub core_cwd: PathBuf,
    /// Whether to show the empty state (drives `tree_view::build`'s `start` flag
    /// and `crate::shell::AppShell::set_empty_state`).
    pub empty: bool,
    /// The tab's saved pane tree, or `None`. Attached ONLY when the chosen cwd IS
    /// the saved cwd — see [`restore_plan`].
    pub layout: Option<PaneNode>,
    /// The restored window size in pixels, or `None` for the unset sentinel.
    pub window_size: Option<(i32, i32)>,
    /// The user's saved sidebar PREFERENCE, defaulting to `true` when there is no
    /// saved tab.
    ///
    /// Independent of [`RestorePlan::empty`]: the empty state overrides the
    /// preference at APPLY time, through [`sidebar_is_shown`], never at restore
    /// time. Folding the override in here would lose the preference on the first
    /// launch into an empty state, and the next save would then persist the fold.
    pub sidebar_visible: bool,
}

/// Builds the whole launch plan from the loaded `state.json`, the process cwd and
/// the home directory.
///
/// In order: take `saved.windows.first()`, then its `tabs.first()`; treat a
/// non-empty `tab.cwd` as the restored project; hand BOTH candidates to
/// [`workspace::launch_decision`] (process cwd first — see that module for why);
/// then attach the saved layout and the saved window size.
///
/// **A saved layout is restored only into the project it was saved from.** Its
/// leaf paths are absolute paths belonging to one project, so the layout is
/// attached only when the decision is [`LaunchDecision::Folder`] AND that cwd IS
/// the restored cwd. Launching from a different directory therefore restores a
/// fresh empty pane and nothing else.
///
/// `is_dir` is injected so the whole plan is pure; production passes
/// `Path::is_dir`.
#[must_use]
pub fn restore_plan(
    saved: Option<&WolfState>,
    process_cwd: Option<&Path>,
    home: Option<&Path>,
    is_dir: impl Fn(&Path) -> bool,
) -> RestorePlan {
    let window = saved.and_then(|state| state.windows.first());
    let tab = window.and_then(|window| window.tabs.first());
    // `""` is the documented "nvim never reported a cwd" sentinel, not a path.
    let restored: Option<PathBuf> = tab
        .map(|tab| tab.cwd.as_str())
        .filter(|cwd| !cwd.is_empty())
        .map(PathBuf::from);

    let (core_cwd, empty, layout) =
        match workspace::launch_decision(process_cwd, restored.as_deref(), home, is_dir) {
            LaunchDecision::Folder { cwd } => {
                let same_project = restored.as_deref() == Some(cwd.as_path());
                let layout = if same_project {
                    tab.and_then(|tab| tab.layout.clone())
                } else {
                    None
                };
                (cwd, false, layout)
            }
            LaunchDecision::EmptyState { nvim_cwd } => (nvim_cwd, true, None),
        };

    RestorePlan {
        core_cwd,
        empty,
        layout,
        window_size: window.and_then(|window| restored_window_size(window.width, window.height)),
        sidebar_visible: tab.is_none_or(|tab| tab.sidebar_visible),
    }
}

/// Whether the file-tree sidebar is SHOWN: the user wants it AND a project is open.
///
/// The ONE place that rule lives. `crate::shell::AppShell::set_empty_state` and
/// `AppShell::toggle_sidebar` both apply it through the same private applier, so a
/// user toggle cannot be silently undone by the next `set_empty_state(false)` and
/// the empty state cannot draw a file tree for a project that is not open.
///
/// The empty state always WINS: a window with no project root has nothing to list,
/// so the tree would render whatever directory nvim happened to be handed
/// (`$HOME` on a desktop-launcher start) beside a status page saying no folder is
/// open.
#[must_use]
pub fn sidebar_is_shown(user_wants: bool, empty: bool) -> bool {
    !empty && user_wants
}

/// The Linux half of macOS's `StateSeam.restoredWindowFrame`: the saved window
/// SIZE in pixels, or `None` when it should be ignored.
///
/// A non-positive, non-finite, or out-of-`i32`-range dimension is the unset
/// sentinel and collapses the WHOLE result to `None` (the `geometry.rs`
/// convention — never a per-axis partial answer). There is no
/// screen-intersection check (Wayland gives the client no screen geometry) and no
/// position at all; see the module docs.
#[must_use]
pub fn restored_window_size(width: f64, height: f64) -> Option<(i32, i32)> {
    // Both are resolved before either is returned: a usable width with a garbage
    // height is not half a window size, it is no window size.
    let w = usable_dimension(width)?;
    let h = usable_dimension(height)?;
    Some((w, h))
}

/// The tab's persistable layout: `Some(node)` unless `node` is an unnamed,
/// non-preview LONE leaf.
///
/// **THIS IS THE SINGLE-PANE FIX.** The predicate is
/// [`pane_layout::is_empty_leaf`] — a CONTENT test — and never
/// `pane_layout::leaf_count(..) > 1`. An arity guard here is exactly the macOS
/// bug: a lone pane showing a file would persist no layout, and since a tab's
/// open buffer paths ride only inside the layout, that file would not be
/// reopened on relaunch. See [`crate::pane_layout`]'s module docs.
///
/// The suppressed case is the pre-existing invariant that must be PRESERVED: the
/// empty-state "no folder" lone pane persists `layout: None`.
#[must_use]
pub fn persistable_layout(node: PaneNode) -> Option<PaneNode> {
    if pane_layout::is_empty_leaf(&node) {
        None
    } else {
        Some(node)
    }
}

/// The working directory to persist for the tab.
///
/// The EMPTY state persists `""` ALWAYS — never the nvim fallback cwd. A
/// `LaunchDecision::EmptyState` hands nvim `$HOME` (or `/`) purely because a
/// process needs a working directory; persisting that would turn every
/// desktop-launcher start into "a project at `$HOME`" on the next launch. macOS
/// has the same rule (its passive launch cwd is never persisted).
///
/// With a project open it is nvim's LIVE cwd once reported, else the launch cwd —
/// so quitting before the first `DirChanged` does not forget the project.
#[must_use]
pub fn snapshot_cwd(empty: bool, launch_cwd: &Path, live_cwd: Option<&Path>) -> String {
    if empty {
        return String::new();
    }
    live_cwd
        .unwrap_or(launch_cwd)
        .to_string_lossy()
        .into_owned()
}

/// Whether a live snapshot may be committed at all.
///
/// `true` **iff** the captive nvim is [`Liveness::Alive`], at least one pane has a
/// RESOLVED grid, and nvim has reported its per-window buffer map at least once
/// (`reported_windows` is `Snapshot::win_buffers.len()`, empty until the first
/// `wolf_buffers` notification).
///
/// This gate is not tidiness — it prevents DATA LOSS. A snapshot taken while nvim
/// is dead, before the first pane binds, or before the first buffer report would
/// persist `layout: None` (or a topology whose leaf paths are all empty) and
/// DESTROY the user's saved layout. Gated, a crash instead preserves the last
/// good save.
///
/// The match over [`Liveness`] is TOTAL with no `_` arm, so a new variant is an
/// `E0004` here rather than a silent "not alive" — matching
/// [`crate::liveness_ui`], [`crate::ui_action_for`] and `pane_view::apply_intents`.
#[must_use]
pub fn may_snapshot(liveness: Liveness, bound_panes: usize, reported_windows: usize) -> bool {
    match liveness {
        Liveness::Alive => bound_panes >= 1 && reported_windows >= 1,
        // A stalled nvim's snapshot is stale by definition and a dead one's is
        // empty; either would overwrite a good save with a worse one.
        Liveness::NotResponding | Liveness::Dead => false,
    }
}

/// Assembles the whole persisted tree for `wolf-gnome`'s single window and single
/// tab.
///
/// This is the ONE place that shape lives. `x`/`y` are `0.0` unconditionally:
/// window POSITION is never persisted on Wayland (see the module docs), and a
/// deliberate zero is far better than a plausible-looking garbage coordinate a
/// later reader might trust.
#[must_use]
pub fn snapshot_state(
    width: i32,
    height: i32,
    cwd: String,
    sidebar_visible: bool,
    layout: Option<PaneNode>,
) -> WolfState {
    WolfState {
        windows: vec![WindowState {
            x: 0.0,
            y: 0.0,
            width: f64::from(width),
            height: f64::from(height),
            tabs: vec![TabState {
                cwd,
                sidebar_visible,
                layout,
            }],
        }],
    }
}

/// One window dimension, validated and rounded, or `None` when it is the unset
/// sentinel.
///
/// `f64 as i64` SATURATES at the `i64` bounds in Rust (never wraps, never UB), so
/// an absurd saved value lands outside `i32` and `i32::try_from` turns it into the
/// documented `None` rather than a clamped nonsense size.
fn usable_dimension(value: f64) -> Option<i32> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    let rounded = value.round() as i64;
    i32::try_from(rounded).ok().filter(|px| *px > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    use wolf_core::PaneOrientation;

    // --- Paths (absolute and distinct, so every `==` is unambiguous) ------------
    /// The process working directory in the terminal-launch rows.
    const PROJECT: &str = "/projects/wolf";
    /// The working directory recovered from `state.json`.
    const SAVED: &str = "/projects/saved";
    /// The user's home directory (the desktop-launcher shape).
    const HOME: &str = "/home/tester";
    /// A saved cwd whose directory no longer exists.
    const GONE: &str = "/projects/deleted";
    /// A leaf's buffer path inside the saved layout.
    const SAVED_FILE: &str = "/projects/saved/only.md";

    // --- Saved window geometry ---------------------------------------------------
    /// The saved window width in the fixtures.
    const SAVED_W: f64 = 1200.0;
    /// The saved window height in the fixtures.
    const SAVED_H: f64 = 640.0;
    /// [`SAVED_W`] / [`SAVED_H`] as the restored pixel pair.
    const SAVED_SIZE: (i32, i32) = (1200, 640);

    // --- `may_snapshot` domain ---------------------------------------------------
    /// Every `Liveness` variant, so the table below covers the whole enum.
    const LIVENESS_VARIANTS: [Liveness; 3] =
        [Liveness::Alive, Liveness::NotResponding, Liveness::Dead];
    /// Bound-pane counts probed: none, exactly one (the boundary), and several.
    const BOUND_PANE_COUNTS: [usize; 3] = [0, 1, 3];
    /// Reported-window counts probed: none (nvim has never sent `wolf_buffers`) and
    /// one.
    const REPORTED_WINDOW_COUNTS: [usize; 2] = [0, 1];

    /// A non-preview leaf bound to `path`.
    fn leaf(path: &str) -> PaneNode {
        PaneNode::Leaf {
            path: path.to_string(),
            preview: false,
        }
    }

    /// A PREVIEW leaf bound to `path`.
    fn preview_leaf(path: &str) -> PaneNode {
        PaneNode::Leaf {
            path: path.to_string(),
            preview: true,
        }
    }

    /// A two-child vertical split of `children`, equally weighted.
    fn split(children: Vec<PaneNode>) -> PaneNode {
        PaneNode::Split {
            orientation: PaneOrientation::Vertical,
            weights: vec![0.5, 0.5],
            children,
        }
    }

    /// A `state.json` tree with one window carrying one tab at `cwd` with `layout`,
    /// and the sidebar preference the tab persisted.
    fn saved_state_with_sidebar(
        cwd: &str,
        layout: Option<PaneNode>,
        sidebar_visible: bool,
    ) -> WolfState {
        WolfState {
            windows: vec![WindowState {
                x: 0.0,
                y: 0.0,
                width: SAVED_W,
                height: SAVED_H,
                tabs: vec![TabState {
                    cwd: cwd.to_string(),
                    sidebar_visible,
                    layout,
                }],
            }],
        }
    }

    /// A `state.json` tree with one window carrying one tab at `cwd` with `layout`,
    /// saved with the sidebar SHOWN — the shape every pre-existing row uses.
    fn saved_state(cwd: &str, layout: Option<PaneNode>) -> WolfState {
        saved_state_with_sidebar(cwd, layout, true)
    }

    /// A `state.json` tree with one window and NO tabs at all.
    fn saved_state_without_tabs() -> WolfState {
        WolfState {
            windows: vec![WindowState {
                x: 0.0,
                y: 0.0,
                width: SAVED_W,
                height: SAVED_H,
                tabs: Vec::new(),
            }],
        }
    }

    /// Builds the injected `is_dir` predicate from a list of directories.
    fn dirs_predicate(dirs: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |path: &Path| dirs.iter().any(|dir| Path::new(dir) == path)
    }

    /// One [`restore_plan`] table row.
    struct PlanCase {
        desc: &'static str,
        saved: Option<WolfState>,
        process_cwd: Option<&'static str>,
        home: Option<&'static str>,
        dirs: &'static [&'static str],
        want: RestorePlan,
    }

    // =====================================================================
    // restore_plan
    // =====================================================================

    #[test]
    fn restore_plan_table() {
        let cases = [
            PlanCase {
                // The compatibility row: with nothing saved, the plan is exactly
                // today's `launch_decision` plus two `None`s — and the sidebar
                // DEFAULT, which is `true`: a first launch shows the file tree.
                desc: "no saved state at all => today's launch decision, no layout, no size, and \
                       the sidebar default of SHOWN",
                saved: None,
                process_cwd: Some(PROJECT),
                home: Some(HOME),
                dirs: &[PROJECT, HOME],
                want: RestorePlan {
                    core_cwd: PathBuf::from(PROJECT),
                    empty: false,
                    layout: None,
                    window_size: None,
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // The desktop-launcher shape: the process cwd is $HOME, so the
                // RESTORED cwd gets its turn and its layout comes with it.
                desc: "saved dir exists and the process cwd is home => Folder{saved} + layout",
                saved: Some(saved_state(SAVED, Some(leaf(SAVED_FILE)))),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME, SAVED],
                want: RestorePlan {
                    core_cwd: PathBuf::from(SAVED),
                    empty: false,
                    layout: Some(PaneNode::Leaf {
                        path: SAVED_FILE.to_string(),
                        preview: false,
                    }),
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // The saved project was deleted between runs. The layout's leaf paths
                // point into a directory that no longer exists, so rebuilding it would
                // ask nvim to `:edit` files that are gone.
                desc: "saved dir is GONE => EmptyState at home, layout DROPPED, size kept",
                saved: Some(saved_state(GONE, Some(leaf("/projects/deleted/x.md")))),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME],
                want: RestorePlan {
                    core_cwd: PathBuf::from(HOME),
                    empty: true,
                    layout: None,
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: true,
                },
            },
            PlanCase {
                desc: "saved `windows: []` => nothing to restore at all",
                saved: Some(WolfState {
                    windows: Vec::new(),
                }),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME, SAVED],
                want: RestorePlan {
                    core_cwd: PathBuf::from(HOME),
                    empty: true,
                    layout: None,
                    window_size: None,
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // A window with no tabs still carries GEOMETRY, and the geometry is
                // not the tab's to lose. See `window_size_is_independent_of_the_cwd_decision`.
                desc: "saved `tabs: []` => no cwd and no layout, but the window SIZE survives",
                saved: Some(saved_state_without_tabs()),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME, SAVED],
                want: RestorePlan {
                    core_cwd: PathBuf::from(HOME),
                    empty: true,
                    layout: None,
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // The empty-state save: `cwd: ""` is the documented "nvim never
                // reported a cwd" sentinel and must never be treated as a path.
                desc: "saved cwd is the \"\" sentinel => not a candidate project",
                saved: Some(saved_state("", None)),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME],
                want: RestorePlan {
                    core_cwd: PathBuf::from(HOME),
                    empty: true,
                    layout: None,
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: true,
                },
            },
            PlanCase {
                desc: "saved cwd exists but `layout: None` => Folder{saved}, no layout",
                saved: Some(saved_state(SAVED, None)),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME, SAVED],
                want: RestorePlan {
                    core_cwd: PathBuf::from(SAVED),
                    empty: false,
                    layout: None,
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // The shared predicate applied to the RESTORED candidate: a
                // hand-edited state.json naming `/` must not point FileSearch at the
                // whole filesystem.
                desc: "a saved cwd of `/` is refused by the SAME predicate the process cwd faces",
                saved: Some(saved_state("/", Some(leaf("/x.md")))),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &["/", HOME],
                want: RestorePlan {
                    core_cwd: PathBuf::from(HOME),
                    empty: true,
                    layout: None,
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // THE READ-BACK ROW. Every row above saved the sidebar SHOWN, so a
                // `restore_plan` that hardcoded `sidebar_visible: true` and never
                // looked at the tab would satisfy all eight of them. This is the one
                // that makes the field a reading rather than an assumption.
                desc: "a tab saved with the sidebar HIDDEN restores hidden",
                saved: Some(saved_state_with_sidebar(
                    SAVED,
                    Some(leaf(SAVED_FILE)),
                    false,
                )),
                process_cwd: Some(SAVED),
                home: Some(HOME),
                dirs: &[HOME, SAVED],
                want: RestorePlan {
                    core_cwd: PathBuf::from(SAVED),
                    empty: false,
                    layout: Some(PaneNode::Leaf {
                        path: SAVED_FILE.to_string(),
                        preview: false,
                    }),
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: false,
                },
            },
            PlanCase {
                // The preference is INDEPENDENT of the launch decision: the empty
                // state overrides it at APPLY time (`state_seam::sidebar_is_shown`),
                // not at restore time. Folding the override in here would lose the
                // user's preference the first time they launched into an empty state,
                // and the next save would then persist the fold.
                desc: "a HIDDEN sidebar survives an empty-state launch decision unchanged",
                saved: Some(saved_state_with_sidebar(GONE, None, false)),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME],
                want: RestorePlan {
                    core_cwd: PathBuf::from(HOME),
                    empty: true,
                    layout: None,
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: false,
                },
            },
        ];

        for case in &cases {
            let got = restore_plan(
                case.saved.as_ref(),
                case.process_cwd.map(Path::new),
                case.home.map(Path::new),
                dirs_predicate(case.dirs),
            );
            assert_eq!(got, case.want, "case: {}", case.desc);
        }
    }

    #[test]
    fn a_layout_is_never_restored_into_a_different_project() {
        // A saved layout's leaf paths are ABSOLUTE paths belonging to ONE project.
        // Rebuilding them inside a different project would open that project's files
        // in this project's window — and then persist them back over the saved tree.
        let saved = saved_state(SAVED, Some(split(vec![leaf(SAVED_FILE), leaf("")])));
        let got = restore_plan(
            Some(&saved),
            Some(Path::new(PROJECT)),
            Some(Path::new(HOME)),
            dirs_predicate(&[PROJECT, SAVED, HOME]),
        );

        assert_eq!(
            got.core_cwd,
            PathBuf::from(PROJECT),
            "the process cwd wins over the restored one — launching Wolf from a directory is the \
             Linux idiom, and silently reopening yesterday's project instead would be surprising"
        );
        assert!(
            !got.empty,
            "a real project directory is not the empty state"
        );
        assert_eq!(
            got.layout, None,
            "the saved layout must NOT be attached when the chosen project ({PROJECT}) is not the \
             project it was saved from ({SAVED}). Its leaves are absolute paths under {SAVED}, so \
             restoring them here would open another project's files — and the next snapshot would \
             then overwrite the saved tree with them."
        );
        assert_eq!(
            got.window_size,
            Some(SAVED_SIZE),
            "the WINDOW SIZE is not project-scoped and is restored regardless"
        );
    }

    #[test]
    fn a_layout_is_restored_when_the_process_cwd_is_the_saved_project() {
        // The positive twin of the guard above, and the ONLY path the S1 / S2
        // relaunch smokes exercise: both relaunch in the SAME temp cwd they saved
        // from, so `process_cwd == restored` and the layout must be attached. A guard
        // that dropped every layout would still pass the negative test alone.
        let layout = split(vec![leaf(SAVED_FILE), leaf("")]);
        let saved = saved_state(SAVED, Some(layout.clone()));
        let got = restore_plan(
            Some(&saved),
            Some(Path::new(SAVED)),
            Some(Path::new(HOME)),
            dirs_predicate(&[SAVED, HOME]),
        );

        assert_eq!(
            got,
            RestorePlan {
                core_cwd: PathBuf::from(SAVED),
                empty: false,
                layout: Some(layout),
                window_size: Some(SAVED_SIZE),
                sidebar_visible: true,
            },
            "relaunching IN the saved project must restore its layout verbatim. This is the exact \
             shape the relaunch smokes run: same cwd, same state dir, layout expected back."
        );
    }

    #[test]
    fn window_size_is_independent_of_the_cwd_decision() {
        // DELIBERATE: a saved window whose project is gone still restores its SIZE.
        // Window geometry is a property of the window, not of the project, and the
        // user resized it once for their screen. Pinned here so it is not "fixed" in
        // the wrong direction by someone who reads the layout guard and generalizes.
        let saved = saved_state(GONE, Some(leaf("/projects/deleted/x.md")));
        let got = restore_plan(
            Some(&saved),
            Some(Path::new(HOME)),
            Some(Path::new(HOME)),
            dirs_predicate(&[HOME]),
        );

        assert!(
            got.empty,
            "precondition: with the saved directory gone and the process cwd at home, this really \
             is the empty state"
        );
        assert_eq!(
            got.layout, None,
            "precondition: the layout really was dropped, so the size below is being asserted \
             against the WORST case for it"
        );
        assert_eq!(
            got.window_size,
            Some(SAVED_SIZE),
            "the restored window SIZE must survive a dropped cwd and a dropped layout. It is a \
             property of the window, not of the project."
        );
    }

    // =====================================================================
    // sidebar_is_shown — the user preference versus the empty state
    // =====================================================================

    /// One [`sidebar_is_shown`] table row. The expectation is DATA, deliberately
    /// not re-derived from the same boolean expression the function under test uses
    /// — a mirror implementation would agree with any bug it also contained (the
    /// [`may_snapshot_table`] precedent).
    struct SidebarCase {
        desc: &'static str,
        user_wants: bool,
        empty: bool,
        want: bool,
    }

    /// The whole two-by-two domain: the sidebar is shown exactly when the user
    /// wants it AND there is a project to show.
    ///
    /// Every row is load-bearing, and each falsifies a different wrong rule. Row 1
    /// falsifies a constant `false` (which would hide the tree permanently). Row 2
    /// falsifies `!empty` — the rule the persistence side used to write — which
    /// would ignore the user's toggle entirely. Row 3 falsifies `user_wants`, which
    /// would show an empty-state sidebar listing the files of a project that is not
    /// open. Row 4 is the agreement corner, and on its own says nothing.
    #[test]
    fn sidebar_is_shown_table() {
        let cases = [
            SidebarCase {
                desc: "a project is open and the user wants the tree => shown",
                user_wants: true,
                empty: false,
                want: true,
            },
            SidebarCase {
                desc: "a project is open and the user toggled the tree away => hidden",
                user_wants: false,
                empty: false,
                want: false,
            },
            SidebarCase {
                desc: "the EMPTY state overrides the preference: there is no project to list",
                user_wants: true,
                empty: true,
                want: false,
            },
            SidebarCase {
                desc: "the empty state with the preference already off => still hidden",
                user_wants: false,
                empty: true,
                want: false,
            },
        ];

        assert_eq!(
            cases.len(),
            4,
            "the sidebar_is_shown table must carry all four (user_wants, empty) inputs — two \
             booleans is four rows, not {}. A missing row leaves one of the two conjuncts \
             unmeasured, and each conjunct is the whole of a different bug: without `empty` the \
             empty state draws a file tree for a project that is not open, and without \
             `user_wants` the toggle chord and its button do nothing at all.",
            cases.len()
        );

        for case in &cases {
            assert_eq!(
                sidebar_is_shown(case.user_wants, case.empty),
                case.want,
                "sidebar_is_shown(user_wants = {}, empty = {}) must be {} — {}. This is the ONE \
                 place the rule lives: `AppShell::set_empty_state` and `AppShell::toggle_sidebar` \
                 both apply it through the same private applier, so a wrong answer here means the \
                 next `set_empty_state(false)` silently undoes whatever the user just toggled.",
                case.user_wants,
                case.empty,
                case.want,
                case.desc
            );
        }
    }

    /// The override direction stated on its own, so a failure names the rule rather
    /// than quoting a row index.
    ///
    /// The empty state is not a preference and never becomes one: whatever the user
    /// wants, a window with no project open has nothing to list.
    #[test]
    fn the_empty_state_always_wins() {
        for user_wants in [true, false] {
            assert!(
                !sidebar_is_shown(user_wants, true),
                "sidebar_is_shown({user_wants}, empty = true) said the sidebar is SHOWN. The empty \
                 state has no project root, so the tree would render the file list of whatever \
                 directory nvim happened to be handed — `$HOME` on a desktop-launcher start — \
                 beside a status page that says no folder is open."
            );
        }

        // Non-vacuity: the loop above is an implication over the `empty` half alone,
        // so a `sidebar_is_shown` of `|_, _| false` would satisfy it while hiding
        // the tree in every project too.
        assert!(
            sidebar_is_shown(true, false),
            "the happy path (the user wants the sidebar, a project is open) MUST show it — \
             otherwise the file tree is never drawn at all and every assertion above holds \
             vacuously."
        );
    }

    // =====================================================================
    // restored_window_size
    // =====================================================================

    /// One [`restored_window_size`] table row.
    struct SizeCase {
        desc: &'static str,
        width: f64,
        height: f64,
        want: Option<(i32, i32)>,
    }

    #[test]
    fn restored_window_size_table() {
        let cases = [
            SizeCase {
                desc: "a real saved size restores verbatim",
                width: SAVED_W,
                height: SAVED_H,
                want: Some(SAVED_SIZE),
            },
            SizeCase {
                desc: "fractional dimensions ROUND (0.7 up, 0.2 down)",
                width: 1200.7,
                height: 640.2,
                want: Some((1201, 640)),
            },
            SizeCase {
                desc: "both zero is the unset sentinel",
                width: 0.0,
                height: 0.0,
                want: None,
            },
            SizeCase {
                desc: "a negative WIDTH collapses the whole result",
                width: -1.0,
                height: 600.0,
                want: None,
            },
            SizeCase {
                desc: "a zero HEIGHT collapses the whole result (never a per-axis answer)",
                width: 1200.0,
                height: 0.0,
                want: None,
            },
            SizeCase {
                desc: "NaN width",
                width: f64::NAN,
                height: SAVED_H,
                want: None,
            },
            SizeCase {
                desc: "NaN height",
                width: SAVED_W,
                height: f64::NAN,
                want: None,
            },
            SizeCase {
                desc: "+inf width",
                width: f64::INFINITY,
                height: SAVED_H,
                want: None,
            },
            SizeCase {
                desc: "-inf height",
                width: SAVED_W,
                height: f64::NEG_INFINITY,
                want: None,
            },
            SizeCase {
                // `f64 as i64` SATURATES rather than wrapping, so this lands outside
                // i32 and `try_from` turns it into the documented None instead of a
                // clamped nonsense size.
                desc: "an absurd out-of-i32-range dimension is refused, not clamped",
                width: 1e18,
                height: SAVED_H,
                want: None,
            },
            SizeCase {
                desc: "a positive value that ROUNDS to zero is still the unset sentinel",
                width: 0.4,
                height: SAVED_H,
                want: None,
            },
        ];

        for case in &cases {
            assert_eq!(
                restored_window_size(case.width, case.height),
                case.want,
                "case: {} ({} x {})",
                case.desc,
                case.width,
                case.height
            );
        }
    }

    // =====================================================================
    // persistable_layout — the single-pane fix, from the save side
    // =====================================================================

    /// One [`persistable_layout`] table row.
    struct PersistCase {
        desc: &'static str,
        node: PaneNode,
        want: Option<PaneNode>,
    }

    #[test]
    fn persistable_layout_table() {
        let cases = [
            PersistCase {
                // The pre-existing invariant that must be PRESERVED: the empty-state
                // "no folder" lone pane persists no layout.
                desc: "an unnamed, non-preview LONE leaf is the one suppressed shape",
                node: leaf(""),
                want: None,
            },
            PersistCase {
                // THE REGRESSION ROW. An arity guard (`leaf_count(..) > 1`) answers
                // `None` here, and that is exactly the macOS single-pane bug: the file
                // would not be reopened on relaunch.
                desc: "a NAMED lone leaf IS persisted (the single-pane fix)",
                node: leaf(SAVED_FILE),
                want: Some(leaf(SAVED_FILE)),
            },
            PersistCase {
                desc: "a PREVIEW lone leaf is persisted (preview is content, not emptiness)",
                node: preview_leaf(SAVED_FILE),
                want: Some(preview_leaf(SAVED_FILE)),
            },
            PersistCase {
                desc: "a split of two EMPTY leaves is persisted — the topology is state",
                node: split(vec![leaf(""), leaf("")]),
                want: Some(split(vec![leaf(""), leaf("")])),
            },
        ];

        for case in &cases {
            assert_eq!(
                persistable_layout(case.node.clone()),
                case.want,
                "case: {} (node = {:?})",
                case.desc,
                case.node
            );
        }
    }

    // =====================================================================
    // snapshot_cwd
    // =====================================================================

    /// One [`snapshot_cwd`] table row.
    struct CwdCase {
        desc: &'static str,
        empty: bool,
        launch_cwd: &'static str,
        live_cwd: Option<&'static str>,
        want: &'static str,
    }

    #[test]
    fn snapshot_cwd_table() {
        let cases = [
            CwdCase {
                desc: "empty state with a live cwd => still \"\"",
                empty: true,
                launch_cwd: HOME,
                live_cwd: Some(HOME),
                want: "",
            },
            CwdCase {
                desc: "empty state with no live cwd => \"\"",
                empty: true,
                launch_cwd: HOME,
                live_cwd: None,
                want: "",
            },
            CwdCase {
                desc: "empty state whose nvim wandered to a real directory => STILL \"\"",
                empty: true,
                launch_cwd: HOME,
                live_cwd: Some(PROJECT),
                want: "",
            },
            CwdCase {
                desc: "a project with a live cwd persists the LIVE one",
                empty: false,
                launch_cwd: PROJECT,
                live_cwd: Some(SAVED),
                want: SAVED,
            },
            CwdCase {
                desc: "a project whose nvim has not reported yet persists the LAUNCH cwd",
                empty: false,
                launch_cwd: PROJECT,
                live_cwd: None,
                want: PROJECT,
            },
        ];

        for case in &cases {
            assert_eq!(
                snapshot_cwd(
                    case.empty,
                    Path::new(case.launch_cwd),
                    case.live_cwd.map(Path::new)
                ),
                case.want,
                "case: {}",
                case.desc
            );
        }
    }

    #[test]
    fn the_empty_state_persists_no_cwd() {
        // The rule stated on its own so a failure NAMES it. `LaunchDecision::EmptyState`
        // hands nvim `$HOME` (or `/`) purely because a process needs a working
        // directory; persisting that would turn every desktop-launcher start into "a
        // project at $HOME" on the NEXT launch — and then point the sidebar's
        // FileSearch at the whole home tree.
        for live in [None, Some(HOME), Some(PROJECT), Some("/")] {
            assert_eq!(
                snapshot_cwd(true, Path::new(HOME), live.map(Path::new)),
                "",
                "the empty state must persist the \"\" sentinel for EVERY live cwd (here {live:?}) \
                 and every launch cwd. Persisting the nvim fallback cwd would make the next launch \
                 open $HOME as a project."
            );
        }
    }

    // =====================================================================
    // may_snapshot — the data-loss gate
    // =====================================================================

    /// One [`may_snapshot`] table row. The expectation is DATA, deliberately not
    /// re-derived from the same boolean expression the function under test uses — a
    /// mirror implementation would agree with any bug it also contained.
    struct SnapshotGateCase {
        liveness: Liveness,
        bound_panes: usize,
        reported_windows: usize,
        want: bool,
    }

    #[test]
    fn may_snapshot_table() {
        let cases = [
            // --- Alive: the only variant that may ever snapshot -----------------
            SnapshotGateCase {
                liveness: Liveness::Alive,
                bound_panes: 0,
                reported_windows: 0,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::Alive,
                bound_panes: 0,
                reported_windows: 1,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::Alive,
                bound_panes: 1,
                reported_windows: 0,
                want: false,
            },
            SnapshotGateCase {
                // The one allowed shape.
                liveness: Liveness::Alive,
                bound_panes: 1,
                reported_windows: 1,
                want: true,
            },
            SnapshotGateCase {
                liveness: Liveness::Alive,
                bound_panes: 3,
                reported_windows: 0,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::Alive,
                bound_panes: 3,
                reported_windows: 1,
                want: true,
            },
            // --- NotResponding: a stalled nvim's snapshot is stale by definition -
            SnapshotGateCase {
                liveness: Liveness::NotResponding,
                bound_panes: 0,
                reported_windows: 0,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::NotResponding,
                bound_panes: 0,
                reported_windows: 1,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::NotResponding,
                bound_panes: 1,
                reported_windows: 0,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::NotResponding,
                bound_panes: 1,
                reported_windows: 1,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::NotResponding,
                bound_panes: 3,
                reported_windows: 0,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::NotResponding,
                bound_panes: 3,
                reported_windows: 1,
                want: false,
            },
            // --- Dead: a dead nvim's snapshot is empty --------------------------
            SnapshotGateCase {
                liveness: Liveness::Dead,
                bound_panes: 0,
                reported_windows: 0,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::Dead,
                bound_panes: 0,
                reported_windows: 1,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::Dead,
                bound_panes: 1,
                reported_windows: 0,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::Dead,
                bound_panes: 1,
                reported_windows: 1,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::Dead,
                bound_panes: 3,
                reported_windows: 0,
                want: false,
            },
            SnapshotGateCase {
                liveness: Liveness::Dead,
                bound_panes: 3,
                reported_windows: 1,
                want: false,
            },
        ];

        for case in &cases {
            assert_eq!(
                may_snapshot(case.liveness, case.bound_panes, case.reported_windows),
                case.want,
                "may_snapshot({:?}, bound_panes = {}, reported_windows = {}) must be {}",
                case.liveness,
                case.bound_panes,
                case.reported_windows,
                case.want
            );
        }

        // Assert the table's OWN size, so a row that quietly went missing cannot hide
        // behind the rows that remain.
        assert_eq!(
            cases.len(),
            LIVENESS_VARIANTS.len() * BOUND_PANE_COUNTS.len() * REPORTED_WINDOW_COUNTS.len(),
            "the may_snapshot table must carry one row per (liveness, bound_panes, \
             reported_windows) combination: {} x {} x {}",
            LIVENESS_VARIANTS.len(),
            BOUND_PANE_COUNTS.len(),
            REPORTED_WINDOW_COUNTS.len()
        );
        assert_eq!(
            LIVENESS_VARIANTS.len(),
            3,
            "`Liveness` has three variants and all three must appear above. `may_snapshot` matches \
             it with NO `_` arm, so a new variant is an E0004 there — this assertion is what makes \
             the missing table rows a test failure here rather than an untested arm"
        );
        // And every declared variant really does appear, so the count above cannot be
        // satisfied by six rows of `Alive`.
        for liveness in LIVENESS_VARIANTS {
            assert!(
                cases.iter().any(|case| case.liveness == liveness),
                "no may_snapshot table row covers {liveness:?}"
            );
        }
    }

    #[test]
    fn only_alive_with_a_bound_pane_may_snapshot() {
        // The rule over the whole domain, stated on its own so the failure names the
        // consequence rather than a table row.
        for liveness in LIVENESS_VARIANTS {
            for bound in BOUND_PANE_COUNTS {
                for reported in REPORTED_WINDOW_COUNTS {
                    let got = may_snapshot(liveness, bound, reported);
                    if got {
                        assert_eq!(
                            liveness,
                            Liveness::Alive,
                            "may_snapshot said YES while nvim is {liveness:?}. A snapshot taken \
                             while nvim is dead or stalled persists `layout: None` (or a topology \
                             whose leaf paths are all empty) and DESTROYS the user's saved layout \
                             — a crash must preserve the last good save, not wipe it."
                        );
                        assert!(
                            bound >= 1,
                            "may_snapshot said YES with {bound} bound panes. Before the first pane \
                             binds there is no grid to read a buffer path from, so the snapshot \
                             would persist an empty layout OVER the user's saved one."
                        );
                        assert!(
                            reported >= 1,
                            "may_snapshot said YES with {reported} reported windows. \
                             `Snapshot::win_buffers` is empty until nvim's first `wolf_buffers` \
                             notification, so every leaf path would be \"\" and the saved layout \
                             would be replaced by a topology holding no files at all."
                        );
                    }
                }
            }
        }

        // Non-vacuity: the loop above is an implication, so it would pass trivially if
        // `may_snapshot` were `|_, _, _| false`. Exactly one shape must be allowed.
        assert!(
            may_snapshot(Liveness::Alive, 1, 1),
            "the happy path (Alive, one bound pane, one reported window) MUST be allowed — \
             otherwise nothing is ever persisted and every assertion above holds vacuously"
        );
    }

    // =====================================================================
    // snapshot_state
    // =====================================================================

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "x/y are asserted to be the literal 0.0 this function writes unconditionally, \
                  and the dimensions are exact `f64::from(i32)` widenings — an epsilon would let \
                  a garbage coordinate through, which is the whole point of the assertion \
                  (the `mouse::scroll_threshold_table` precedent)"
    )]
    fn snapshot_state_is_one_window_one_tab() {
        const WIDTH: i32 = 1200;
        const HEIGHT: i32 = 640;

        let state = snapshot_state(
            WIDTH,
            HEIGHT,
            SAVED.to_string(),
            true,
            Some(leaf(SAVED_FILE)),
        );

        assert_eq!(
            state.windows.len(),
            1,
            "wolf-gnome is a single-WINDOW front-end today and this is the ONE place that shape \
             lives; a later multi-window unit has to change it deliberately"
        );
        let window = &state.windows[0];
        assert_eq!(window.tabs.len(), 1, "…and a single-TAB one; same argument");

        // D7: window POSITION is never persisted on Wayland (a client is given
        // neither its own position nor any screen geometry), so these are a
        // DELIBERATE zero rather than a plausible-looking coordinate a later reader
        // might trust.
        assert_eq!(
            window.x, 0.0,
            "window.x must be a deliberate 0.0: Wayland gives the client no position, so anything \
             else here is a fabricated coordinate that a future restore might act on"
        );
        assert_eq!(
            window.y, 0.0,
            "window.y must be a deliberate 0.0, for the same reason"
        );
        assert_eq!(window.width, f64::from(WIDTH), "the width round-trips");
        assert_eq!(window.height, f64::from(HEIGHT), "the height round-trips");

        let tab = &window.tabs[0];
        assert_eq!(tab.cwd, SAVED, "the cwd round-trips verbatim");
        assert!(
            tab.sidebar_visible,
            "`sidebar_visible` must round-trip the argument it was given. `restore_plan` READS this \
             field back, so a write side that ignored its argument would hand the next launch a \
             preference the user never expressed"
        );
        assert_eq!(
            tab.layout,
            Some(leaf(SAVED_FILE)),
            "the layout round-trips verbatim"
        );

        // The other side of `sidebar_visible`, so the field is proven to follow its
        // argument rather than being hardcoded true.
        let hidden = snapshot_state(WIDTH, HEIGHT, String::new(), false, None);
        assert!(
            !hidden.windows[0].tabs[0].sidebar_visible,
            "`sidebar_visible` must follow its argument; a hardcoded `true` would make a session \
             the user spent with the tree hidden come back with it shown"
        );
        assert_eq!(
            hidden.windows[0].tabs[0].layout, None,
            "a suppressed layout is written as `None`, which is what `load_state` reads back as \
             the empty-state lone pane"
        );
    }
}
