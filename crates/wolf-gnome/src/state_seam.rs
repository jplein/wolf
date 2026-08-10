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
//!   [`window_subtree`] writes `x`/`y` as `0.0` unconditionally and
//!   [`restored_window_size`] returns SIZE only.
//! - **Tabs.** Windows are the tree: [`restore_plans`] walks every saved window
//!   and [`snapshot_state`] assembles every live one. A window carries N LIVE tabs
//!   and persists its SELECTED tab's subtree alone: [`window_subtree`] writes
//!   exactly one [`TabState`] and [`restore_plans`] reads the first saved tab and
//!   drops the rest. That freeze lives in ONE place so a later unit has to lift it
//!   deliberately.

use std::path::{Path, PathBuf};

use wolf_core::{Liveness, PaneNode, TabState, WindowState, WolfState};

use crate::pane_layout;
use crate::workspace::{self, LaunchDecision};

/// The most windows [`restore_plans`] will open from one saved tree.
///
/// A restore POLICY cap, not a tree limit — the
/// [`crate::pane_layout::MAX_RESTORE_LEAVES`] precedent. What it bounds is not
/// memory but PROCESSES: every restored window costs a whole captive Neovim and a
/// tokio runtime of its own, so a corrupt or hand-edited `state.json` must not be
/// able to fork dozens of them before the first frame. The number sits far above
/// any session a user assembles by hand, so refusing is the pathological case
/// rather than a limit anyone meets.
pub const MAX_RESTORE_WINDOWS: usize = 16;

/// Everything the launch path needs to decide before any widget is built.
#[derive(Debug, Clone, PartialEq)]
pub struct RestorePlan {
    /// The working directory handed to `wolf_core::CoreConfig`.
    pub core_cwd: PathBuf,
    /// Whether to show the empty state (drives `tree_view::build`'s `start` flag
    /// and `crate::shell::AppShell::set_empty_state`).
    pub empty: bool,
    /// The tab's saved pane tree, or `None`. Attached ONLY when the chosen cwd IS
    /// the saved cwd — see [`restore_plans`].
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

/// Builds ONE launch plan per saved window, in saved order — the whole session's
/// launch, rather than one window's.
///
/// Restore is POSITIONAL and never grouped: a saved tree holding two windows on
/// one directory restores both, because the user opened both deliberately.
///
/// # This never returns an empty vec
///
/// No saved state at all and a `windows: []` tree both yield exactly ONE plan:
/// the plan a launch with nothing to restore produces, unchanged. The application
/// always opens a window, so "there is nothing to restore" means "restore the
/// empty state" and never "start with no window at all" — which on GTK is a
/// process that exits the moment `activate` returns.
///
/// # The process cwd is offered to the FIRST saved window only
///
/// Index 0 is handed `process_cwd`; every later index is handed `None`, which
/// lands on [`workspace::launch_decision`]'s already-tested desktop-launcher
/// shape (its `[process_cwd, restored]` scan simply skips the `None`).
///
/// This is a DATA-LOSS rule, not a preference. Offering the process cwd at every
/// index would resolve every window to `Folder { cwd: process_cwd }`, so
/// `same_project` is false for each one past the first: every window merged onto
/// a single project and EVERY saved layout dropped. The failure is silent — the
/// right number of windows open, all showing the same wrong thing — and the next
/// snapshot writes the merge back over the saved tree, so the session is gone
/// rather than merely mis-displayed.
///
/// At most [`MAX_RESTORE_WINDOWS`] plans are returned; a hand-edited tree longer
/// than that is truncated.
///
/// `is_dir` is injected so the whole plan is pure; production passes
/// `Path::is_dir`.
#[must_use]
pub fn restore_plans(
    saved: Option<&WolfState>,
    process_cwd: Option<&Path>,
    home: Option<&Path>,
    is_dir: impl Fn(&Path) -> bool,
) -> Vec<RestorePlan> {
    let saved_windows = saved.map_or(&[][..], |state| state.windows.as_slice());

    let mut plans: Vec<RestorePlan> = saved_windows
        .iter()
        .take(MAX_RESTORE_WINDOWS)
        .enumerate()
        .map(|(index, window)| {
            // The rule above: only the first window is a candidate for the
            // directory this process was launched from.
            let offered_cwd = if index == 0 { process_cwd } else { None };
            plan_for_window(Some(window), offered_cwd, home, &is_dir)
        })
        .collect();

    if plans.is_empty() {
        // The floor, not a fallback: an empty vec here would leave the
        // application with no window to build.
        plans.push(plan_for_window(None, process_cwd, home, &is_dir));
    }

    plans
}

/// One window's launch plan: the restore rule applied to a single saved
/// [`WindowState`], or to no saved window at all.
///
/// Takes the window's `tabs.first()`, treats a non-empty `tab.cwd` as the
/// restored project, hands BOTH candidates to [`workspace::launch_decision`]
/// (process cwd first — see that module for why), then attaches the saved layout
/// and the saved window size.
///
/// **A saved layout is restored only into the project it was saved from.** Its
/// leaf paths are absolute paths belonging to one project, so the layout is
/// attached only when the decision is [`LaunchDecision::Folder`] AND that cwd IS
/// the restored cwd. Launching a window into a different directory therefore
/// restores a fresh empty pane and nothing else.
fn plan_for_window(
    window: Option<&WindowState>,
    process_cwd: Option<&Path>,
    home: Option<&Path>,
    is_dir: &impl Fn(&Path) -> bool,
) -> RestorePlan {
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
/// **The gate is read ONCE PER GATHER, against that window's own core**, and a
/// refusal means "contribute the cached subtree", not "vanish from the tree" — a
/// window whose panes have not bound yet keeps whatever it last persisted.
///
/// **This gate cannot protect a BACKGROUND tab, and must not be asked to.** A
/// window owns one core and N tabs, and it persists its SELECTED tab's subtree
/// alone ([`crate::tab_view::TabsHandle::selected`]), so the gate is only ever
/// asked about the tab whose pixels the user is looking at. Widening the gather
/// would not be caught here: `reported_windows` is `Snapshot::win_buffers.len()`,
/// which `runtime/nvim/init.lua` fills from `nvim_list_wins()` — GLOBAL across
/// tabpages — so it never drops for a background tab and this gate never shuts.
/// The empty leaf paths a background gather would write come from
/// `WolfCore::pane_status`, one layer down, and no liveness/readiness triple can
/// see them.
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

/// Assembles ONE window's persisted subtree: its geometry, and its single tab.
///
/// This is the ONE place that shape lives. `x`/`y` are `0.0` unconditionally:
/// window POSITION is never persisted on Wayland (see the module docs), and a
/// deliberate zero is far better than a plausible-looking garbage coordinate a
/// later reader might trust.
///
/// The single TAB is equally deliberate and equally located here: a `wolf-gnome`
/// window carries N tabs but persists its SELECTED tab's subtree alone, so the vec
/// carries one entry, and the unit that lifts that freeze has to change it here
/// rather than discover it spread across the crate. See
/// [`crate::tab_view::TabsHandle::selected`] for what a background tab would write
/// instead.
#[must_use]
pub fn window_subtree(
    width: i32,
    height: i32,
    cwd: String,
    sidebar_visible: bool,
    layout: Option<PaneNode>,
) -> WindowState {
    WindowState {
        x: 0.0,
        y: 0.0,
        width: f64::from(width),
        height: f64::from(height),
        tabs: vec![TabState {
            cwd,
            sidebar_visible,
            layout,
        }],
    }
}

/// Assembles the whole persisted tree from the live windows' subtrees, in window
/// order — or `None` when there are none.
///
/// # `None` means "commit nothing", never "commit nothing useful"
///
/// An empty contribution set is not an empty session: it is a launch whose only
/// core died before its first pane bound, or a moment when every window's
/// [`may_snapshot`] gate is shut. Answering `Some(WolfState { windows: vec![] })`
/// would hand the store a well-formed tree holding nothing, which overwrites the
/// user's whole saved session with an empty file and loses it silently. `None` is
/// the mechanical equivalent of the refused gate the single-window path already
/// relies on: no call to `set_snapshot`, so the store keeps the tree it loaded
/// from disk.
#[must_use]
pub fn snapshot_state(windows: Vec<WindowState>) -> Option<WolfState> {
    if windows.is_empty() {
        return None;
    }
    Some(WolfState { windows })
}

/// What closing one window does to the persisted tree.
///
/// Returned by [`close_commit`], which is the whole rule; this enum only names its
/// three answers so a caller cannot express a fourth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseCommit {
    /// Commit nothing at all: the tree has already been written by the quit path.
    Skip,
    /// Commit a tree that still CONTAINS the closing window's subtree.
    KeepSubtree,
    /// Commit a tree with the closing window's subtree REMOVED.
    DropSubtree,
}

/// Whether closing one window commits a tree, and whether that tree still holds
/// the closing window.
///
/// Three answers, each guarding a different way to lose a session:
///
/// - **Not shutting down, no other live window** ⇒ [`CloseCommit::KeepSubtree`].
///   Closing the last window IS quitting — `:qa` reaches exactly here — so the
///   session must survive it. Dropping the subtree instead would make every quit
///   save an empty tree and every relaunch open the empty state.
/// - **Not shutting down, at least one other live window** ⇒
///   [`CloseCommit::DropSubtree`]. The user closed one of several and meant it; a
///   window they deliberately dismissed must not reappear at the next launch.
/// - **Shutting down** ⇒ [`CloseCommit::Skip`], whatever else is live. The quit
///   path commits the WHOLE tree once, while every core is still alive, and then
///   closes each window in turn; committing again per window would shrink that
///   tree to nothing one window at a time.
#[must_use]
pub fn close_commit(shutting_down: bool, other_live_windows: usize) -> CloseCommit {
    match (shutting_down, other_live_windows) {
        // One arm, not one per window count: the quit path's commit is already
        // done and the number of survivors cannot change that.
        (true, _) => CloseCommit::Skip,
        (false, 0) => CloseCommit::KeepSubtree,
        (false, _) => CloseCommit::DropSubtree,
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
    /// The SECOND saved window's working directory, distinct from [`SAVED`] so a
    /// two-window expectation can tell the windows apart by value alone.
    const SAVED_B: &str = "/projects/second";
    /// A leaf's buffer path inside the SECOND saved window's layout. Distinct from
    /// [`SAVED_FILE`] so a rule that read window 0's tab for every plan is visible
    /// in the LAYOUT as well as in the cwd.
    const SAVED_B_FILE: &str = "/projects/second/beta.md";
    /// The directory the PROCESS was launched from in the two-window rows: a real
    /// project, and neither saved directory, so "this plan was offered the process
    /// cwd" is decidable from the value alone.
    const LAUNCHED: &str = "/projects/launched";
    /// The common prefix of [`many_window_tree`]'s per-window directories.
    const MANY: &str = "/projects/many/w";

    // --- Injected `is_dir` sets --------------------------------------------------
    /// Home only: the launch that has nothing at all to restore.
    const HOME_ONLY_DIRS: &[&str] = &[HOME];
    /// A terminal launch with nothing saved.
    const PROJECT_AND_HOME_DIRS: &[&str] = &[PROJECT, HOME];
    /// Home plus BOTH saved projects — the two-window rows launched with no process
    /// cwd of their own.
    const TWO_SAVED_DIRS: &[&str] = &[HOME, SAVED, SAVED_B];
    /// Home, the process cwd, and BOTH saved projects.
    ///
    /// Accepting the saved directories is load-bearing rather than incidental: an
    /// `is_dir` that omitted them would drop every window past the first onto
    /// [`LaunchDecision::EmptyState`], and
    /// [`the_process_cwd_is_offered_to_the_first_saved_window_only`] would then pass
    /// because those directories do not exist rather than because the process cwd
    /// was withheld from them.
    const LAUNCH_AND_TWO_SAVED_DIRS: &[&str] = &[HOME, LAUNCHED, SAVED, SAVED_B];

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

    // --- `close_commit` domain ---------------------------------------------------
    /// Both values of the quit latch, so the table below covers the whole boolean.
    const SHUTTING_DOWN_STATES: [bool; 2] = [false, true];
    /// Other-live-window counts probed: NONE (the closing window is the last one —
    /// the boundary the whole rule turns on), exactly one, and several.
    const OTHER_LIVE_WINDOW_COUNTS: [usize; 3] = [0, 1, 2];

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

    /// One saved WINDOW carrying one tab at `cwd` with `layout`, and the sidebar
    /// preference the tab persisted.
    fn saved_window_with_sidebar(
        cwd: &str,
        layout: Option<PaneNode>,
        sidebar_visible: bool,
    ) -> WindowState {
        WindowState {
            x: 0.0,
            y: 0.0,
            width: SAVED_W,
            height: SAVED_H,
            tabs: vec![TabState {
                cwd: cwd.to_string(),
                sidebar_visible,
                layout,
            }],
        }
    }

    /// One saved WINDOW at `cwd` with `layout`, saved with the sidebar SHOWN.
    fn saved_window(cwd: &str, layout: Option<PaneNode>) -> WindowState {
        saved_window_with_sidebar(cwd, layout, true)
    }

    /// A `state.json` tree with one window carrying one tab at `cwd` with `layout`,
    /// and the sidebar preference the tab persisted.
    fn saved_state_with_sidebar(
        cwd: &str,
        layout: Option<PaneNode>,
        sidebar_visible: bool,
    ) -> WolfState {
        WolfState {
            windows: vec![saved_window_with_sidebar(cwd, layout, sidebar_visible)],
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

    /// A `state.json` tree with TWO windows, saved in this order: [`SAVED`] with a
    /// layout naming [`SAVED_FILE`], then [`SAVED_B`] with a layout naming
    /// [`SAVED_B_FILE`].
    ///
    /// EVERY value that could identify a window differs between the two — the cwd
    /// and the leaf path inside the layout — so no expectation below can be
    /// satisfied by the wrong window, and a rule that copied window 0's tab into
    /// every plan is visible rather than plausible.
    fn two_window_tree() -> WolfState {
        WolfState {
            windows: vec![
                saved_window(SAVED, Some(leaf(SAVED_FILE))),
                saved_window(SAVED_B, Some(leaf(SAVED_B_FILE))),
            ],
        }
    }

    /// A `state.json` tree of `count` windows, each a project of its own under
    /// [`MANY`], each with a layout of its own.
    ///
    /// Every window here is USABLE under [`many_window_dirs`], so nothing except the
    /// cap can reduce the plan count: a rule that dropped windows whose directory
    /// had gone, or that deduplicated windows sharing a project, would still return
    /// one plan per window for this tree.
    fn many_window_tree(count: usize) -> WolfState {
        WolfState {
            windows: (0..count)
                .map(|index| {
                    let cwd = format!("{MANY}{index}");
                    let file = format!("{cwd}/only.md");
                    saved_window(&cwd, Some(leaf(&file)))
                })
                .collect(),
        }
    }

    /// The injected `is_dir` for [`many_window_tree`]: every per-window directory
    /// under [`MANY`], plus home. A prefix test rather than a list, because the
    /// window count is read from [`MAX_RESTORE_WINDOWS`] and must stay free to move.
    fn many_window_dirs(path: &Path) -> bool {
        path == Path::new(HOME) || path.to_string_lossy().starts_with(MANY)
    }

    /// Builds the injected `is_dir` predicate from a list of directories.
    fn dirs_predicate(dirs: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |path: &Path| dirs.iter().any(|dir| Path::new(dir) == path)
    }

    /// One launch-plan table row, for [`restore_plan`] and [`restore_plans`] alike.
    struct PlanCase {
        desc: &'static str,
        saved: Option<WolfState>,
        process_cwd: Option<&'static str>,
        home: Option<&'static str>,
        dirs: &'static [&'static str],
        want: RestorePlan,
    }

    /// The ten single-window launch rows [`restore_plans_table`] walks.
    ///
    /// Held as data rather than inlined on purpose: the table's whole claim is
    /// that the session-wide rule answers a single-window save with the SAME plan
    /// the single-window rule produced, and the `[PlanCase; 10]` return type turns
    /// a row that quietly went missing into a compile error rather than a smaller
    /// table.
    fn plan_cases() -> [PlanCase; 10] {
        [
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
        ]
    }

    // =====================================================================
    // one window's launch plan
    // =====================================================================

    #[test]
    fn a_layout_is_never_restored_into_a_different_project() {
        // A saved layout's leaf paths are ABSOLUTE paths belonging to ONE project.
        // Rebuilding them inside a different project would open that project's files
        // in this project's window — and then persist them back over the saved tree.
        let saved = saved_state(SAVED, Some(split(vec![leaf(SAVED_FILE), leaf("")])));
        let got = restore_plans(
            Some(&saved),
            Some(Path::new(PROJECT)),
            Some(Path::new(HOME)),
            dirs_predicate(&[PROJECT, SAVED, HOME]),
        )
        .remove(0);

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
        let got = restore_plans(
            Some(&saved),
            Some(Path::new(SAVED)),
            Some(Path::new(HOME)),
            dirs_predicate(&[SAVED, HOME]),
        )
        .remove(0);

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
        let got = restore_plans(
            Some(&saved),
            Some(Path::new(HOME)),
            Some(Path::new(HOME)),
            dirs_predicate(&[HOME]),
        )
        .remove(0);

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
    // restore_plans — the whole SESSION's launch, rather than one window's
    // =====================================================================

    /// The behaviour-neutrality proof for the restore half: every single-window save
    /// in [`plan_cases`] comes back through [`restore_plans`] as exactly ONE plan,
    /// carrying the expectation the single-window rule already produced for it.
    ///
    /// ## What these ten rows cannot see
    ///
    /// Every row carries at most one saved window, and every row's process cwd is
    /// either the home directory (which is never a usable project) or the saved
    /// directory itself. No row can therefore tell "the process cwd is offered to
    /// the FIRST saved window" apart from "the process cwd is offered to NO saved
    /// window": the only row that moves when the offer is withdrawn is "no saved
    /// state at all", and that row reaches its plan through the never-empty FLOOR
    /// rather than through the per-window walk.
    /// [`the_process_cwd_is_offered_to_the_first_saved_window_only`] is where that
    /// rule is actually measured.
    ///
    /// ## Not isolated
    ///
    /// Deleting the never-empty floor reddens the "no saved state at all" and
    /// "saved `windows: []`" rows here beside
    /// [`a_launch_with_no_saved_windows_opens_exactly_one`], which is the test that
    /// names the consequence. Recorded rather than avoided: the two rows are what
    /// make the floor part of the compatibility claim as well as a rule of its own.
    #[test]
    fn restore_plans_table() {
        let cases = plan_cases();

        for case in &cases {
            let plans = restore_plans(
                case.saved.as_ref(),
                case.process_cwd.map(Path::new),
                case.home.map(Path::new),
                dirs_predicate(case.dirs),
            );

            assert_eq!(
                plans.len(),
                1,
                "case: {} — a save holding at most ONE window must launch exactly one window, and \
                 the two failures are different bugs. Zero plans leaves the application with no \
                 window at all: on GTK `activate` returns having built nothing, so the process \
                 exits without ever drawing. More than one opens windows the saved session never \
                 held. Plans were {plans:?}",
                case.desc
            );
            assert_eq!(
                plans[0], case.want,
                "case: {} — the single plan must be EXACTLY the plan the single-window rule \
                 already produces for this input. A difference here is a behaviour change to \
                 every existing launch — including every cage smoke, all of which launch with one \
                 window — rather than a multi-window feature.",
                case.desc
            );
        }
    }

    /// The read side of the measured silent session loss: a saved tree of two
    /// windows launches TWO windows, in saved order, each carrying its own project
    /// and its own layout.
    ///
    /// This is the headless keystone of the multi-window claim. The revert it exists
    /// to catch is the single-window shape — building the plan from
    /// `saved.windows.first()` alone — under which the second window's whole session
    /// is dropped at launch and then written back over by the next snapshot, so the
    /// loss is silent and permanent rather than merely a missing window.
    ///
    /// Isolated: [`restore_plans_table`]'s rows all hold at most one saved window, so
    /// a first-only rule satisfies every one of them.
    #[test]
    fn restore_plans_covers_every_saved_window_in_order() {
        let saved = two_window_tree();
        let plans = restore_plans(
            Some(&saved),
            // No process cwd: the desktop-launcher shape, and the one where each
            // window's own saved directory is the only candidate it has.
            None,
            Some(Path::new(HOME)),
            dirs_predicate(TWO_SAVED_DIRS),
        );

        assert_eq!(
            plans.len(),
            2,
            "a saved tree of two windows must produce two launch plans. One plan is the \
             single-window rule surviving: the second window's project, layout and geometry are \
             all dropped at launch, and the next snapshot writes the one-window tree back over \
             the save, so the session is GONE rather than merely unopened. Zero plans would leave \
             the application with no window at all. Plans were {plans:?}"
        );
        assert_eq!(
            plans[0].core_cwd,
            PathBuf::from(SAVED),
            "plan 0 must open the FIRST saved window's project. Asserting both positions is what \
             catches a reversed walk, which a set-wise check would accept — and the user's window \
             order is the order the next snapshot writes back."
        );
        assert_eq!(
            plans[1].core_cwd,
            PathBuf::from(SAVED_B),
            "plan 1 must open the SECOND saved window's project. Seeing {SAVED} here instead means \
             either the walk is reversed or every plan was built from window 0's tab; seeing \
             {HOME} means window 1 fell to the empty state, which for a directory the injected \
             is_dir accepts can only be a lost cwd."
        );
        assert_eq!(
            plans[0].layout,
            Some(leaf(SAVED_FILE)),
            "plan 0 must carry the first window's own layout: it relaunches into the project the \
             layout was saved from, so the files that window had open come back with it"
        );
        assert_eq!(
            plans[1].layout,
            Some(leaf(SAVED_B_FILE)),
            "plan 1 must carry the SECOND window's own layout, and the leaf path is what says so — \
             {SAVED_FILE} here would mean window 0's tab was read for both plans, and `None` would \
             mean layouts past the first are dropped (which \
             `every_window_past_the_first_keeps_its_own_project` names on its own)"
        );
        assert!(
            !plans[0].empty && !plans[1].empty,
            "premise: with both saved directories accepted by the injected is_dir, NEITHER plan is \
             the empty state. If either were, the cwd assertions above would be comparing against \
             a launch decision that never consulted the saved tree at all."
        );
    }

    /// The process cwd is a candidate for the FIRST saved window and for no other.
    ///
    /// Offering it at every index is not a preference but silent destruction: every
    /// window resolves to `Folder { cwd: process_cwd }`, so `same_project` is false
    /// for each one past the first, and all N windows open on one project with EVERY
    /// saved layout dropped. The right NUMBER of windows appear, all showing the same
    /// wrong thing, and the next snapshot writes the merge back over the saved tree.
    ///
    /// Isolated: yes. [`restore_plans_covers_every_saved_window_in_order`] and
    /// [`every_window_past_the_first_keeps_its_own_project`] pass `None` as the
    /// process cwd, so neither can see this rule at all.
    #[test]
    fn the_process_cwd_is_offered_to_the_first_saved_window_only() {
        let saved = two_window_tree();
        let is_dir = dirs_predicate(LAUNCH_AND_TWO_SAVED_DIRS);

        // PREMISE, measured rather than assumed. If the injected predicate refused
        // either saved directory, the window at that index would fall to
        // `LaunchDecision::EmptyState` and the assertions below would hold because
        // the directory does not exist rather than because the process cwd was
        // withheld from it — the test passing for the wrong reason.
        for dir in [LAUNCHED, SAVED, SAVED_B] {
            assert!(
                is_dir(Path::new(dir)),
                "fixture premise: the injected is_dir must accept {dir}. Both saved projects have \
                 to be real directories here, or index 1 lands on the empty state and this test \
                 proves nothing about where the process cwd was offered."
            );
        }

        let plans = restore_plans(
            Some(&saved),
            Some(Path::new(LAUNCHED)),
            Some(Path::new(HOME)),
            is_dir,
        );

        assert_eq!(
            plans.len(),
            2,
            "precondition: both saved windows are still planned. Plans were {plans:?}"
        );
        assert_eq!(
            plans[0].core_cwd,
            PathBuf::from(LAUNCHED),
            "plan 0 must take the PROCESS cwd: launching Wolf from a directory is the Linux idiom \
             and it wins over what the first saved window remembers. Seeing {SAVED} here means the \
             process cwd was offered to no window at all."
        );
        assert_eq!(
            plans[0].layout, None,
            "plan 0 opens {LAUNCHED} while its saved layout belongs to {SAVED}, so the layout must \
             NOT be attached — its leaves are absolute paths in another project, and restoring \
             them would open that project's files here and then persist them back"
        );
        assert_eq!(
            plans[1].core_cwd,
            PathBuf::from(SAVED_B),
            "plan 1 must keep its OWN saved project. {LAUNCHED} here is the merge this rule exists \
             to prevent: every window opened on the directory the process happened to start in, \
             which looks like a working launch and is a destroyed session."
        );
        assert!(
            !plans[1].empty,
            "plan 1 must be a real project, not the empty state. The empty state here would mean \
             the assertion above was satisfied by a directory that does not exist rather than by \
             the rule."
        );
        assert_eq!(
            plans[1].layout,
            Some(leaf(SAVED_B_FILE)),
            "plan 1 relaunches into the project its layout was saved from, so the layout must come \
             with it. `None` here is the second half of the merge: the windows would open on one \
             project AND lose every saved pane tree."
        );
    }

    /// A window past the first keeps its own project AND its own layout, with no
    /// process cwd anywhere in the picture.
    ///
    /// The companion [`the_process_cwd_is_offered_to_the_first_saved_window_only`]
    /// needs. That test's failure symptom — `plans[1].layout == None` — has two
    /// possible causes: the process cwd was offered to window 1 (so `same_project`
    /// is false), or layouts past index 0 are dropped outright. Here there is no
    /// process cwd to offer, so a missing layout at index 1 can only be the second,
    /// and the two bugs are distinguishable rather than sharing one symptom.
    ///
    /// Not isolated: [`restore_plans_covers_every_saved_window_in_order`] asserts the
    /// same two values on the same fixture and reddens beside it. What this adds is
    /// that the rule has a name a failure can print.
    #[test]
    fn every_window_past_the_first_keeps_its_own_project() {
        let saved = two_window_tree();
        let plans = restore_plans(
            Some(&saved),
            None,
            Some(Path::new(HOME)),
            dirs_predicate(TWO_SAVED_DIRS),
        );

        assert_eq!(
            plans.len(),
            2,
            "precondition: both saved windows are still planned. Plans were {plans:?}"
        );
        assert_eq!(
            plans[1].core_cwd,
            PathBuf::from(SAVED_B),
            "with no process cwd offered anywhere, window 1 has exactly one candidate — its own \
             saved directory — and must open it"
        );
        assert_eq!(
            plans[1].layout,
            Some(leaf(SAVED_B_FILE)),
            "window 1 opens the project its layout was saved from, so the layout must be attached. \
             `None` here, with no process cwd in play, means layouts past index 0 are dropped \
             outright: every window past the first would come back as a bare empty pane and the \
             next snapshot would persist that."
        );
    }

    /// The never-empty floor: a launch with nothing to restore opens exactly one
    /// window, whether the save is absent or holds `windows: []`.
    ///
    /// "There is nothing to restore" means "restore the empty state", never "start
    /// with no window at all" — which on GTK is a process that exits the moment
    /// `activate` returns, with no window, no error and no way to reach the
    /// application again.
    ///
    /// Not isolated: dropping the floor also reddens two rows of
    /// [`restore_plans_table`], which assert the same length for the same two
    /// inputs. This test is where the consequence is named.
    #[test]
    fn a_launch_with_no_saved_windows_opens_exactly_one() {
        let nothing_saved = restore_plans(
            None,
            Some(Path::new(PROJECT)),
            Some(Path::new(HOME)),
            dirs_predicate(PROJECT_AND_HOME_DIRS),
        );
        let empty_tree = WolfState {
            windows: Vec::new(),
        };
        let empty_saved = restore_plans(
            Some(&empty_tree),
            Some(Path::new(PROJECT)),
            Some(Path::new(HOME)),
            dirs_predicate(PROJECT_AND_HOME_DIRS),
        );

        assert_eq!(
            nothing_saved.len(),
            1,
            "a launch with NO saved state must plan exactly one window. Zero plans is not an empty \
             session but no application: GTK sees no toplevel and the process exits. Plans were \
             {nothing_saved:?}"
        );
        assert_eq!(
            empty_saved.len(),
            1,
            "a saved tree with `windows: []` must plan exactly one window, for the same reason. \
             This is the shape a save written while every window's snapshot gate was shut would \
             have, so it is reachable from a real state.json rather than only from a hand edit. \
             Plans were {empty_saved:?}"
        );
        assert_eq!(
            nothing_saved, empty_saved,
            "the two must produce the SAME plan: `windows: []` carries no more information than no \
             saved state at all, and a launch that treated them differently would make the empty \
             tree a third launch mode nothing else in the crate knows about"
        );
    }

    /// The shape a window with nothing to restore takes: the empty state at home,
    /// no layout, no saved size, and the sidebar preference's default.
    ///
    /// This is the plan a New Window is built from — no saved state and no process
    /// cwd — so it is named here rather than left implicit in a table row.
    ///
    /// ## It is not an independent discriminator, and must not be dressed as one
    ///
    /// It shares `workspace::launch_decision`'s empty-state arm with several
    /// [`restore_plans_table`] rows, so no edit reddens it alone. In particular it
    /// CANNOT see the rule it exists to describe:
    /// that a new window inherits nothing from the window that opened it. That rule
    /// lives in the CHOICE of arguments at the call site — `None` for the saved
    /// state and `None` for the process cwd — and passing the invoking window's cwd
    /// instead would leave this test perfectly green. Only the smoke that reads the
    /// new window's `getcwd()` back from its own Neovim can falsify that. The
    /// precedent for a shape guard with a name and no falsifier of its own is
    /// `palette::tests::open_recent_activation_targets_the_single_window`.
    #[test]
    fn a_new_windows_plan_is_a_fresh_empty_window() {
        let plans = restore_plans(
            None,
            None,
            Some(Path::new(HOME)),
            dirs_predicate(HOME_ONLY_DIRS),
        );

        assert_eq!(
            plans.len(),
            1,
            "precondition: one plan, which is the never-empty floor. Plans were {plans:?}"
        );
        assert_eq!(
            plans[0],
            RestorePlan {
                core_cwd: PathBuf::from(HOME),
                empty: true,
                layout: None,
                window_size: None,
                sidebar_visible: true,
            },
            "a plan built from no saved state and no process cwd must be the empty state at home \
             with nothing restored: `empty` drives the status page and keeps the sidebar hidden, \
             `layout: None` means a fresh single pane, `window_size: None` means GTK's default \
             geometry rather than another window's, and `sidebar_visible: true` is the PREFERENCE \
             default — the empty state overrides it at apply time through `sidebar_is_shown`, \
             never here"
        );
    }

    /// A hand-edited or corrupted tree is truncated to [`MAX_RESTORE_WINDOWS`]
    /// plans.
    ///
    /// A restore POLICY cap, not a tree limit: what it bounds is PROCESSES, since
    /// every restored window costs a captive Neovim and a tokio runtime of its own.
    /// The symbol is read rather than a literal written, so the value can move
    /// without retuning anything here.
    ///
    /// Isolated: yes. No other test builds a tree anywhere near the cap.
    #[test]
    fn restore_plans_caps_a_hand_edited_tree() {
        let hand_edited = many_window_tree(MAX_RESTORE_WINDOWS + 3);
        let plans = restore_plans(
            Some(&hand_edited),
            None,
            Some(Path::new(HOME)),
            many_window_dirs,
        );

        assert_eq!(
            plans.len(),
            MAX_RESTORE_WINDOWS,
            "a saved tree of {} windows must yield {MAX_RESTORE_WINDOWS} plans. Every window in \
             this fixture is a usable project of its own, so the cap is the only thing that can \
             reduce the count — and without it a corrupt state.json forks one Neovim process and \
             one tokio runtime per entry before the first frame is drawn. Got {} plans.",
            MAX_RESTORE_WINDOWS + 3,
            plans.len()
        );
        assert_eq!(
            plans[0].core_cwd,
            PathBuf::from(format!("{MANY}0")),
            "truncation must keep the FIRST windows: taking the last {MAX_RESTORE_WINDOWS} would \
             satisfy the count above while silently dropping the windows saved earliest"
        );

        // The cap must be non-vacuous: exactly AT the limit nothing is truncated, so
        // a `take` that tightened by one — or a cap that reached zero — is caught
        // here rather than mistaken for correct truncation above.
        let exactly_at_cap = many_window_tree(MAX_RESTORE_WINDOWS);
        let capped_plans = restore_plans(
            Some(&exactly_at_cap),
            None,
            Some(Path::new(HOME)),
            many_window_dirs,
        );
        assert_eq!(
            capped_plans.len(),
            MAX_RESTORE_WINDOWS,
            "a tree of exactly {MAX_RESTORE_WINDOWS} windows must restore ALL of them; without \
             this the truncation assertion above would also pass with the cap mistakenly one too \
             tight, which would lose a window from a session the user really did assemble"
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
    /// falsifies `!empty` — a rule that reads only the project state — which
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

        let state = snapshot_state(vec![window_subtree(
            WIDTH,
            HEIGHT,
            SAVED.to_string(),
            true,
            Some(leaf(SAVED_FILE)),
        )])
        .expect("one window's subtree must assemble into a tree");

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
        let hidden = snapshot_state(vec![window_subtree(
            WIDTH,
            HEIGHT,
            String::new(),
            false,
            None,
        )])
        .expect("one window's subtree must assemble into a tree");
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

    // =====================================================================
    // window_subtree — ONE window's persisted subtree
    // =====================================================================

    /// One window's subtree carries exactly one TAB, a deliberate `0.0` position,
    /// and every value it was handed.
    ///
    /// The single-tab shape lives HERE and nowhere else, so a later tabs unit has to
    /// change it deliberately rather than discover it spread across the crate. The
    /// window COUNT is no part of this claim — that half belongs to
    /// [`snapshot_state`], and
    /// [`snapshot_state_carries_every_window_in_order`] is where it is pinned.
    ///
    /// Reverts that must redden it, both inherited from
    /// [`snapshot_state_is_one_window_one_tab`]: hardcoding `sidebar_visible: true`
    /// fails the second half, and writing a non-zero `x` or `y` fails the position
    /// assertions. Neither touches the single-window assembler beside it, which
    /// carries its own copy of this body until it is deleted, so while the
    /// duplication lasts each red is confined to the function that was edited.
    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "x/y are asserted to be the literal 0.0 this function writes unconditionally, \
                  and the dimensions are exact `f64::from(i32)` widenings — an epsilon would let \
                  a garbage coordinate through, which is the whole point of the assertion \
                  (the `mouse::scroll_threshold_table` precedent)"
    )]
    fn window_subtree_is_one_tab() {
        const WIDTH: i32 = 1200;
        const HEIGHT: i32 = 640;

        let window = window_subtree(
            WIDTH,
            HEIGHT,
            SAVED.to_string(),
            true,
            Some(leaf(SAVED_FILE)),
        );

        assert_eq!(
            window.tabs.len(),
            1,
            "a wolf-gnome window carries a single TAB, and this is the ONE place that shape lives; \
             a later tabs unit has to change it deliberately"
        );

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
            "`sidebar_visible` must round-trip the argument it was given. `restore_plans` READS \
             this field back, so a write side that ignored its argument would hand the next launch \
             a preference the user never expressed"
        );
        assert_eq!(
            tab.layout,
            Some(leaf(SAVED_FILE)),
            "the layout round-trips verbatim"
        );

        // The other side of `sidebar_visible`, so the field is proven to follow its
        // argument rather than being hardcoded true.
        let hidden = window_subtree(WIDTH, HEIGHT, String::new(), false, None);
        assert!(
            !hidden.tabs[0].sidebar_visible,
            "`sidebar_visible` must follow its argument; a hardcoded `true` would make a session \
             the user spent with the tree hidden come back with it shown"
        );
        assert_eq!(
            hidden.tabs[0].layout, None,
            "a suppressed layout is written as `None`, which is what `load_state` reads back as \
             the empty-state lone pane"
        );
    }

    // =====================================================================
    // snapshot_state — assembling the whole tree from the live windows
    // =====================================================================

    /// The committed tree holds EVERY contributing window, in window order.
    ///
    /// The write-side twin of
    /// [`restore_plans_covers_every_saved_window_in_order`]: restore walks the tree
    /// positionally, so the order this assembles in is the order the next launch
    /// opens.
    ///
    /// The comparison is against a whole [`WolfState`] built from the same two
    /// subtrees, which uses the DERIVED `PartialEq` — no float comparison happens in
    /// this test's own source, so it needs no `clippy::float_cmp` allow (unlike
    /// [`window_subtree_is_one_tab`], which asserts on `x` directly).
    ///
    /// Isolated: yes at the unit level.
    #[test]
    fn snapshot_state_carries_every_window_in_order() {
        const WIDTH_A: i32 = 1200;
        const HEIGHT_A: i32 = 640;
        const WIDTH_B: i32 = 900;
        const HEIGHT_B: i32 = 500;

        // Every field that could identify a window differs: geometry, cwd, sidebar
        // preference and layout. So "the right two windows, in the right order" is
        // decidable from the value alone.
        let first = window_subtree(
            WIDTH_A,
            HEIGHT_A,
            SAVED.to_string(),
            true,
            Some(leaf(SAVED_FILE)),
        );
        let second = window_subtree(
            WIDTH_B,
            HEIGHT_B,
            SAVED_B.to_string(),
            false,
            Some(leaf(SAVED_B_FILE)),
        );

        let state = snapshot_state(vec![first.clone(), second.clone()]);

        assert_eq!(
            state,
            Some(WolfState {
                windows: vec![first, second],
            }),
            "the assembled tree must carry both windows in contribution order. Three failures are \
             distinguishable here and each loses something different: `None` commits nothing at \
             all, so the store keeps yesterday's tree; a one-element `windows` drops a whole \
             window's session and the next launch reopens only one; and a REVERSED order restores \
             the same two projects into swapped windows, because restore is positional."
        );
    }

    /// An empty contribution set commits NOTHING, rather than committing an empty
    /// tree.
    ///
    /// An empty set is not an empty session: it is a launch whose only core died
    /// before its first pane bound, or a moment when every window's
    /// [`may_snapshot`] gate is shut. `Some(WolfState { windows: vec![] })` is a
    /// well-formed tree holding nothing, and committing it overwrites the user's
    /// whole saved session with an empty file — the launch that failed to start
    /// would destroy the save it was about to restore. `None` is the mechanical
    /// equivalent of the refused gate the single-window path already relies on: no
    /// call to `set_snapshot`, so the store keeps the tree it loaded from disk.
    ///
    /// Isolated: yes.
    #[test]
    fn an_empty_contribution_set_commits_nothing() {
        assert_eq!(
            snapshot_state(Vec::new()),
            None,
            "an empty contribution set must commit NOTHING. `Some(WolfState {{ windows: [] }})` \
             here is the value that overwrites a good save with an empty file: the store would \
             flush a tree holding no windows, and the next launch would open the empty state with \
             every project, layout and geometry gone — silently, since a successful write looks \
             exactly like a successful write."
        );

        // Non-vacuity: the rule above is satisfied by an assembler that answers
        // `None` for everything, which would persist nothing at all, ever.
        assert!(
            snapshot_state(vec![saved_window(SAVED, Some(leaf(SAVED_FILE)))]).is_some(),
            "one contributing window MUST assemble into a committable tree — otherwise nothing is \
             ever persisted and the assertion above holds vacuously"
        );
    }

    // =====================================================================
    // close_commit — what closing one window does to the persisted tree
    // =====================================================================

    /// One [`close_commit`] table row. The expectation is DATA, deliberately not
    /// re-derived from the same match the function under test uses — a mirror
    /// implementation would agree with any bug it also contained (the
    /// [`may_snapshot_table`] precedent).
    struct CloseCase {
        desc: &'static str,
        shutting_down: bool,
        other_live_windows: usize,
        want: CloseCommit,
    }

    /// The whole (`shutting_down`, `other_live_windows`) domain, one row each.
    ///
    /// Every falsifier is an arm of the same match, and each loses a session a
    /// different way:
    ///
    /// - `(false, 0)` answering [`CloseCommit::DropSubtree`] makes every quit save
    ///   an empty tree, because closing the last window IS quitting and `:qa`
    ///   reaches exactly here.
    /// - `(false, 1)` or `(false, 2)` answering [`CloseCommit::KeepSubtree`] brings
    ///   a window the user deliberately dismissed back at the next launch.
    /// - Any `(true, _)` row answering anything but [`CloseCommit::Skip`] re-commits
    ///   per window while the quit path is already closing them, shrinking the tree
    ///   it just wrote one window at a time.
    ///
    /// The `(true, 0)` row is not a formality: it is the row that catches a match
    /// whose zero-survivors arm is written BEFORE the shutting-down arm, which
    /// answers `KeepSubtree` for the last window of a quit and leaves every
    /// `(true, >= 1)` row green.
    ///
    /// ## Not isolated, and that is the value
    ///
    /// Once the window-close path routes through this rule, reverting the
    /// `(false, 0)` arm reddens every relaunch smoke as well as this table: each of
    /// them quits run 1 with `:qa`, which closes the only window, so the frozen
    /// `state.json` would come back holding nothing and the readback would answer
    /// the empty state. That ties this pure rule to shipped, already-green
    /// behaviour. While `close_commit` still has no caller the reds are confined
    /// here, which is a fact about the wiring rather than about the rule.
    #[test]
    fn close_commit_table() {
        let cases = [
            CloseCase {
                desc: "closing the LAST window is quitting, and the session must survive it",
                shutting_down: false,
                other_live_windows: 0,
                want: CloseCommit::KeepSubtree,
            },
            CloseCase {
                desc: "one of two windows closed on purpose must not reappear at the next launch",
                shutting_down: false,
                other_live_windows: 1,
                want: CloseCommit::DropSubtree,
            },
            CloseCase {
                desc: "one of three, same rule — the survivor count is not the question",
                shutting_down: false,
                other_live_windows: 2,
                want: CloseCommit::DropSubtree,
            },
            CloseCase {
                // The LAST window of a quit. An arm order that tested "no other live
                // windows" first would answer KeepSubtree here and stay green on
                // every other shutting-down row.
                desc: "the last window closing DURING a quit commits nothing; the tree is written",
                shutting_down: true,
                other_live_windows: 0,
                want: CloseCommit::Skip,
            },
            CloseCase {
                desc: "a quit closing the first of two windows commits nothing",
                shutting_down: true,
                other_live_windows: 1,
                want: CloseCommit::Skip,
            },
            CloseCase {
                desc: "a quit closing the first of three commits nothing, for the same reason",
                shutting_down: true,
                other_live_windows: 2,
                want: CloseCommit::Skip,
            },
        ];

        for case in &cases {
            assert_eq!(
                close_commit(case.shutting_down, case.other_live_windows),
                case.want,
                "close_commit(shutting_down = {}, other_live_windows = {}) must be {:?} — {}. \
                 Each wrong answer is its own data loss: DropSubtree where KeepSubtree is due \
                 empties the saved tree on every ordinary quit; KeepSubtree where DropSubtree is \
                 due resurrects a window the user dismissed; and anything but Skip while shutting \
                 down re-commits per window over the whole tree the quit path already wrote.",
                case.shutting_down,
                case.other_live_windows,
                case.want,
                case.desc
            );
        }

        // Assert the table's OWN size, so a row that quietly went missing cannot hide
        // behind the rows that remain.
        assert_eq!(
            cases.len(),
            SHUTTING_DOWN_STATES.len() * OTHER_LIVE_WINDOW_COUNTS.len(),
            "the close_commit table must carry one row per (shutting_down, other_live_windows) \
             combination: {} x {}",
            SHUTTING_DOWN_STATES.len(),
            OTHER_LIVE_WINDOW_COUNTS.len()
        );
        // ...and every declared combination really does appear, so the count above
        // cannot be satisfied by six rows of the same input.
        for shutting_down in SHUTTING_DOWN_STATES {
            for other in OTHER_LIVE_WINDOW_COUNTS {
                assert!(
                    cases.iter().any(|case| case.shutting_down == shutting_down
                        && case.other_live_windows == other),
                    "no close_commit table row covers (shutting_down = {shutting_down}, \
                     other_live_windows = {other})"
                );
            }
        }
        // ...and all three answers are still expected somewhere, so a table whose
        // expectations were retyped to one variant cannot pass.
        for want in [
            CloseCommit::Skip,
            CloseCommit::KeepSubtree,
            CloseCommit::DropSubtree,
        ] {
            assert!(
                cases.iter().any(|case| case.want == want),
                "no close_commit table row expects {want:?}. All three answers must be reachable, \
                 or one of the three ways to lose a session is untested."
            );
        }
    }

    /// The last window keeps its subtree, stated on its own so a failure NAMES the
    /// rule rather than quoting a row index.
    ///
    /// Closing the only window IS quitting — `:qa` reaches exactly here — so the
    /// session has to survive it. Dropping the subtree instead would make every
    /// ordinary quit save an empty tree and every relaunch open the empty state, and
    /// the user would find that out one launch too late.
    ///
    /// It shares [`close_commit_table`]'s revert (the `(false, 0)` arm) and has no
    /// falsifier of its own; the [`the_empty_state_always_wins`] precedent. Once the
    /// close path is wired to this rule, that same revert also reddens every
    /// relaunch smoke.
    #[test]
    fn the_last_window_keeps_its_subtree() {
        assert_eq!(
            close_commit(false, 0),
            CloseCommit::KeepSubtree,
            "closing the last window must COMMIT a tree that still holds it. This is the ordinary \
             quit — `:qa` closes the only window and nothing has latched a shutdown — so a \
             DropSubtree here means the saved session is emptied by the very act of ending it, and \
             a Skip means the last window's final edits are never persisted at all."
        );

        // Non-vacuity: the rule above is satisfied by a `close_commit` that always
        // answers KeepSubtree, which would resurrect every deliberately closed
        // window at the next launch.
        assert_eq!(
            close_commit(false, 1),
            CloseCommit::DropSubtree,
            "with another window still live, the closing window's subtree MUST be dropped — \
             otherwise the rule above holds vacuously and a window the user closed on purpose \
             comes back at the next launch"
        );
    }
}
