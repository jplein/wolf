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
//! - **Tabs.** Windows are the tree and tabs are its second level:
//!   [`restore_plans`] walks every saved window and plans every saved tab of it
//!   ([`RestorePlan::tabs`], capped at [`MAX_RESTORE_TABS`]), while
//!   [`snapshot_state`] assembles every live window and [`window_subtree`] every
//!   contributing tab, in tab order. What is NOT here is a per-tab working
//!   directory: the cwd and the sidebar preference are per WINDOW, written into
//!   every tab and read back from the first.

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

/// The most tabs [`restore_plans`] will plan for ONE saved window.
///
/// The [`MAX_RESTORE_WINDOWS`] precedent, bounding a different resource: what a
/// long tab list costs is the launch's BLOCKING-RPC budget — one `create_tabpage`
/// and up to one `rebuild_layout` per restored tab, every one of them on the GTK
/// main thread before the window is interactive — whose worst case is the product
/// of the two caps. The number sits far above any session a user assembles by
/// hand, so refusing is the pathological case rather than a limit anyone meets.
pub const MAX_RESTORE_TABS: usize = 8;

/// One restored TAB's plan.
///
/// A struct rather than a bare `Option<PaneNode>` so the deferred unit that gives
/// tabs independent working directories adds a field here instead of changing every
/// signature that carries one.
#[derive(Debug, Clone, PartialEq)]
pub struct TabPlan {
    /// This tab's saved pane tree, or `None`. Attached ONLY when the chosen cwd IS
    /// the saved cwd — see [`restore_plans`].
    pub layout: Option<PaneNode>,
}

/// Everything the launch path needs to decide before any widget is built.
#[derive(Debug, Clone, PartialEq)]
pub struct RestorePlan {
    /// The working directory handed to `wolf_core::CoreConfig`.
    pub core_cwd: PathBuf,
    /// Whether to show the empty state (drives `tree_view::build`'s `start` flag
    /// and `crate::shell::AppShell::set_empty_state`).
    pub empty: bool,
    /// The tabs to open, in saved order, NEVER empty: index 0 is the window's first
    /// tab and every later index is queued for the restore materialization
    /// ([`crate::tab_view::TabsHandle::queue_restore`]).
    pub tabs: Vec<TabPlan>,
    /// The restored window size in pixels, or `None` for the unset sentinel.
    pub window_size: Option<(i32, i32)>,
    /// The user's saved sidebar PREFERENCE, defaulting to `true` when there is no
    /// saved tab.
    ///
    /// **Window-level, while the layouts are per tab, and that asymmetry is real
    /// rather than an oversight.** The sidebar is one widget per WINDOW
    /// (`window.rs` builds it beside the tab view) and the shell fans the
    /// preference to every tab, so every tab persists the same value and restore
    /// reads it back off `tabs.first()`.
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
/// (process cwd first — see that module for why), then plans one [`TabPlan`] per
/// saved tab and attaches the saved window size.
///
/// **A saved tab list is restored only into the project it was saved from.** Every
/// tab's leaf paths are absolute paths belonging to one project, so the list is
/// planned only when the decision is [`LaunchDecision::Folder`] AND that cwd IS the
/// restored cwd. Launching a window into a different directory therefore restores
/// one fresh empty tab and nothing else.
///
/// The CWD decision reads `tabs.first()` alone, and so does the sidebar preference:
/// both are per WINDOW (see [`RestorePlan::sidebar_visible`]).
///
/// [`RestorePlan::tabs`] is never empty. A window is built with the tabs its plan
/// names, so an empty vec would be a toplevel holding a tab bar and no editor, with
/// no way to reach one — the tab-level twin of [`restore_plans`]' never-empty
/// window floor.
fn plan_for_window(
    window: Option<&WindowState>,
    process_cwd: Option<&Path>,
    home: Option<&Path>,
    is_dir: &impl Fn(&Path) -> bool,
) -> RestorePlan {
    let saved_tabs = window.map_or(&[][..], |window| window.tabs.as_slice());
    let tab = saved_tabs.first();
    // `""` is the documented "nvim never reported a cwd" sentinel, not a path.
    let restored: Option<PathBuf> = tab
        .map(|tab| tab.cwd.as_str())
        .filter(|cwd| !cwd.is_empty())
        .map(PathBuf::from);

    let (core_cwd, empty, mut tabs) =
        match workspace::launch_decision(process_cwd, restored.as_deref(), home, is_dir) {
            LaunchDecision::Folder { cwd } => {
                let same_project = restored.as_deref() == Some(cwd.as_path());
                let tabs = if same_project {
                    saved_tabs
                        .iter()
                        .take(MAX_RESTORE_TABS)
                        .map(|tab| TabPlan {
                            layout: tab.layout.clone(),
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                (cwd, false, tabs)
            }
            LaunchDecision::EmptyState { nvim_cwd } => (nvim_cwd, true, Vec::new()),
        };

    if tabs.is_empty() {
        // The floor, not a fallback: an empty vec here would leave the window
        // with no page at all.
        tabs.push(TabPlan { layout: None });
    }

    RestorePlan {
        core_cwd,
        empty,
        tabs,
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
/// **The gate is read ONCE PER TAB, against that tab's window's core**, and a
/// refusal means "contribute the cached subtree", not "vanish from the tree" — a
/// tab whose panes have not bound yet keeps whatever it last persisted.
///
/// **It is asked about every tab, including background ones, and answers usefully
/// for them.** `reported_windows` is `Snapshot::win_buffers.len() +
/// Snapshot::previews.len()`, both filled by `runtime/nvim/init.lua` from
/// `nvim_list_wins()` — GLOBAL across tabpages — so it describes the whole Neovim
/// rather than the current tabpage, which is exactly what makes it the right input
/// for a tab the user is not looking at. A background tab's own refusal route is
/// [`crate::pane_layout::resolve_leaves`], one layer down: a leaf whose window is
/// in neither map refuses that tab's gather, and its cache is contributed instead.
/// The two refusals compose — either one leaves the last good subtree standing.
///
/// A RESTORED tab is gathered LIVE from the moment it materializes:
/// `pane_view::PaneHandle::realize_layout` creates its panes synchronously, so
/// `bound_panes` is non-zero and this gate passes for it even while Neovim is on
/// another tabpage. The tabs that reach [`seeded_tab_state`] instead are the ones
/// with no bound pane at all — a tab still sitting in the restore QUEUE, whose page
/// does not exist yet ([`pending_tab_states`]), and a materialized tab whose
/// `realize_layout` bound none.
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

/// Assembles ONE tab's persisted state.
///
/// The one place a [`TabState`] is built on the write side, so the gathered tab and
/// the seeded one ([`seeded_tab_state`]) cannot drift into two shapes.
#[must_use]
pub fn tab_subtree(cwd: String, sidebar_visible: bool, layout: Option<PaneNode>) -> TabState {
    TabState {
        cwd,
        sidebar_visible,
        layout,
    }
}

/// Assembles ONE window's persisted subtree from its geometry and its contributing
/// tabs, in tab order — or `None` when no tab contributed.
///
/// This is the ONE place that shape lives. `x`/`y` are `0.0` unconditionally:
/// window POSITION is never persisted on Wayland (see the module docs), and a
/// deliberate zero is far better than a plausible-looking garbage coordinate a
/// later reader might trust.
///
/// Tab ORDER is the user's own and is what the next launch replays, because
/// [`restore_plans`] walks a window's tabs positionally.
///
/// # `None` means "commit nothing", exactly as it does for [`snapshot_state`]
///
/// An empty `tabs` vec is not a window with no tabs: every tab's gate can be shut
/// at once while its panes are still binding. Answering `Some(WindowState { tabs:
/// [], .. })` would commit a window that [`restore_plans`] reads back as having no
/// project, no layout and no sidebar preference — the empty state — so the next
/// launch opens a folderless window and the gather after that writes that emptiness
/// back. The window drops out of the contribution set instead, and if no window
/// contributes, `snapshot_state` commits nothing at all.
///
/// It consumes its `Vec<TabState>` deliberately: the caller has just built these
/// values from its tabs' caches and has no further use for them.
#[must_use]
pub fn window_subtree(width: i32, height: i32, tabs: Vec<TabState>) -> Option<WindowState> {
    if tabs.is_empty() {
        return None;
    }
    Some(WindowState {
        x: 0.0,
        y: 0.0,
        width: f64::from(width),
        height: f64::from(height),
        tabs,
    })
}

/// What a restored tab with NO BOUND PANE contributes, or `None` when it must
/// contribute nothing at all.
///
/// `Some` **iff** `restored_layout` is `Some`, carrying that layout verbatim at
/// [`snapshot_cwd`]'s answer for this window and the LIVE sidebar preference.
///
/// Two tabs reach it, and neither has a cache to fall back on: one still sitting in
/// the restore QUEUE, whose page does not exist yet ([`pending_tab_states`] maps
/// this over that queue), and a materialized tab whose `realize_layout` bound no
/// pane. A restored tab that DID materialize is gathered live from its first wake —
/// `realize_layout` creates its panes synchronously and the window-keyed maps
/// resolve them from any tabpage — and never consults this at all.
///
/// Both halves are load-bearing, in opposite directions:
///
/// - **Seeding a tab that carries a layout** is what stops a commit made before that
///   tab has a pane erasing it. It has no cache, so with no seed it would contribute
///   nothing, and the window would be written back without it. The round trip is
///   idempotent because the seeded cwd is `snapshot_cwd`'s own answer and a tab is
///   seeded only when the launch resolved to the project it was saved from.
/// - **Not seeding a tab that carries none** is what stops a launch whose project
///   has gone committing an empty-state tab over the saved one before any pane has
///   bound — the loss [`may_snapshot`] exists to prevent, arriving through the one
///   path that bypasses it.
#[must_use]
pub fn seeded_tab_state(
    restored_layout: Option<&PaneNode>,
    empty: bool,
    launch_cwd: &Path,
    sidebar_visible: bool,
) -> Option<TabState> {
    let layout = restored_layout?;
    Some(tab_subtree(
        snapshot_cwd(empty, launch_cwd, None),
        sidebar_visible,
        Some(layout.clone()),
    ))
}

/// What a window contributes for the tabs its launch has QUEUED but not yet
/// created: [`seeded_tab_state`] mapped over the queue, in queued order, with the
/// entries that take no seed dropped.
///
/// It maps and nothing else — no cwd and no preference is re-derived here — so a
/// queued tab and a materialized tab whose own gather never succeeded contribute
/// the identical [`TabState`].
///
/// # Why a queued tab must be contributed at all
///
/// A queued tab has no page, so nothing else can speak for it: no cache, because
/// [`crate::persistence`] holds one per live tab, and no live gather, because its
/// panes do not exist. `window::build` drives a commit before the window is even
/// presented, while Neovim's current tabpage is still `0`, so a window offering
/// only the tabs that already have pages commits `tabs: [one]` over the whole saved
/// session.
///
/// **The loss that would cause is PERSISTENT, not a frame of transient
/// wrongness.** That commit is ungated by liveness, while
/// [`crate::tabs_ui::restore_press`] answers `Wait` forever behind a wedged or dead
/// Neovim — so a launch that never materializes at all still writes the one-tab
/// tree, nothing afterwards re-adds the missing tabs, and that file is what the next
/// launch reads.
///
/// Dropping the entries that carry no layout is [`seeded_tab_state`]'s refusal half,
/// unchanged: a queued tab whose plan carried none would otherwise contribute an
/// empty-state tab at `cwd: ""`.
#[must_use]
pub fn pending_tab_states(
    layouts: &[Option<PaneNode>],
    empty: bool,
    launch_cwd: &Path,
    sidebar_visible: bool,
) -> Vec<TabState> {
    layouts
        .iter()
        .filter_map(|layout| seeded_tab_state(layout.as_ref(), empty, launch_cwd, sidebar_visible))
        .collect()
}

/// A window's live tabs in PAGE order — the order they sit in the tab bar — from
/// the `(page position, tab)` pairs its caller collected, with the positions
/// stripped.
///
/// # Why a reconciliation is needed at all
///
/// `crate::tab_view::TabsHandle::entries` is INSERTION order: a `Vec` that
/// `TabsInner::open_tab` pushes each new page onto, with nothing reconciling it
/// against `adw::TabView`'s page positions. `AdwTabBar` reorders by DRAG, with no
/// property that turns it off and nothing in this tree narrowing it, so the two
/// orders diverge the first time a user drags a tab along the bar. That route alone
/// is what makes this reconciliation load-bearing.
///
/// `AdwTabView:shortcuts` is also left at its default, which carries
/// `Ctrl+Shift+Page_Up` / `Ctrl+Shift+Page_Down` — but whether that chord reaches
/// libadwaita is UNCONFIRMED, so nothing here rests on it. `window::wire_keys`
/// installs a bubble-phase key controller on the toplevel and `input::named_key` maps
/// `Page_Up` / `Page_Down` whatever the modifiers hold, answering
/// `Propagation::Stop`, so the chord very likely reaches Neovim instead.
///
/// [`restore_plans`] walks a saved window's tabs POSITIONALLY, so the order here is
/// the order the next launch replays. Persisting insertion order would discard a
/// user's arrangement at that launch, and because the gather after it writes the
/// collected order back, the discard is permanent rather than one run's.
///
/// # There is no third order to fall back on
///
/// Reordering GTK pages does NOT reorder Neovim's tabpages: libadwaita moves the
/// page inside its own view and this front-end issues no Neovim call for it, so page
/// order and tabpage order genuinely diverge after a drag. Persistence follows PAGE
/// order, which is what the user sees.
///
/// The sort is STABLE. `adw::TabView` hands out one position per page, so a tie is
/// not an input production can produce; a stable sort is what gives the impossible
/// case a stated answer — the pairs' given relative order — instead of one that
/// depends on how they happened to be collected.
#[must_use]
pub fn ordered_tab_states(positioned: Vec<(i32, TabState)>) -> Vec<TabState> {
    let mut positioned = positioned;
    positioned.sort_by_key(|(position, _)| *position);
    positioned.into_iter().map(|(_, tab)| tab).collect()
}

/// ONE window's whole contribution to the persisted tree: its LIVE tabs' caches in
/// tab order, FOLLOWED BY a seed for every tab its launch has queued but not yet
/// created — or `None` when neither has anything to say.
///
/// `window::WindowHandle::cached_subtree` is one call to this, so the composition is
/// executed by the fast suite rather than modelled by it.
///
/// # The order is load-bearing
///
/// Queued tabs materialize AFTER the live ones — `tab_view::TabsInner::open_tab`
/// appends each created page to the end of the bar — and [`restore_plans`] walks a
/// window's tabs positionally. Seeds inserted anywhere but the end therefore hand
/// the next launch the user's tabs in an order they never arranged, and because the
/// gather after that writes the new order back, the rearrangement compounds on every
/// run.
///
/// `live` must ALREADY be in page order; this appends and never sorts. Putting it
/// there is [`ordered_tab_states`]', which is where the reason page order rather
/// than insertion order is the persisted one lives.
///
/// # `sidebar_visible` reaches the SEEDS alone
///
/// It is the window-level preference [`seeded_tab_state`] builds a queued tab's
/// [`TabState`] from. A live tab's cache passes through VERBATIM: the preference
/// inside it is the one that tab's own gather persisted, and rewriting it here would
/// make this seam a second, competing author of a value
/// [`crate::persistence::StateHandle::set_sidebar_shown`] already owns.
///
/// `None` means "commit nothing" rather than "commit an empty window", inherited
/// unchanged from [`window_subtree`].
#[must_use]
pub fn window_contribution(
    width: i32,
    height: i32,
    live: Vec<TabState>,
    pending: &[Option<PaneNode>],
    empty: bool,
    launch_cwd: &Path,
    sidebar_visible: bool,
) -> Option<WindowState> {
    let mut tabs = live;
    tabs.extend(pending_tab_states(
        pending,
        empty,
        launch_cwd,
        sidebar_visible,
    ));
    window_subtree(width, height, tabs)
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
    /// The three files a THREE-TAB save carries, one per tab, in saved order.
    ///
    /// All three live under [`SAVED`], because a tab list is only restored when the
    /// window's chosen project IS the saved one — a fixture whose files sat in three
    /// different projects would be testing something no launch can produce.
    ///
    /// The three are DISTINCT so "the tabs came back in saved order" is decidable
    /// from the values alone: a reversed walk, or a rule that copied tab 0's layout
    /// into every plan, is visible rather than plausible.
    const SAVED_TAB_FILES: [&str; 3] = [
        "/projects/saved/first.md",
        "/projects/saved/second.md",
        "/projects/saved/third.md",
    ];
    /// The common prefix of [`many_tab_state`]'s per-tab leaf paths.
    const MANY_TAB_FILE: &str = "/projects/saved/tab";
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

    /// The layout one saved tab carries: a lone leaf naming [`SAVED_TAB_FILES`]`[index]`.
    fn tab_layout(index: usize) -> Option<PaneNode> {
        Some(leaf(SAVED_TAB_FILES[index]))
    }

    /// The cache ONE live tab contributes, identified by its layout's leaf path alone.
    ///
    /// Every field except the layout is per WINDOW and therefore identical across a
    /// window's tabs, exactly as a real save produces — so an ordering assertion built
    /// on these can only be satisfied by the right tab in the right place, never by a
    /// sibling that happens to look like it.
    fn positioned_tab(index: usize) -> TabState {
        tab_subtree(SAVED.to_string(), true, tab_layout(index))
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

    /// One saved WINDOW at `cwd` carrying one tab per entry of `layouts`, in that
    /// order, each saved with the sidebar SHOWN.
    ///
    /// Every tab carries the SAME `cwd`, which is what a real save produces: the
    /// sidebar preference and the working directory are per WINDOW and written into
    /// every tab (see [`RestorePlan::sidebar_visible`]).
    fn saved_window_with_tabs(cwd: &str, layouts: &[Option<PaneNode>]) -> WindowState {
        WindowState {
            x: 0.0,
            y: 0.0,
            width: SAVED_W,
            height: SAVED_H,
            tabs: layouts
                .iter()
                .map(|layout| TabState {
                    cwd: cwd.to_string(),
                    sidebar_visible: true,
                    layout: layout.clone(),
                })
                .collect(),
        }
    }

    /// A `state.json` tree with ONE window at `cwd` carrying one tab per entry of
    /// `layouts`.
    fn saved_state_with_tabs(cwd: &str, layouts: &[Option<PaneNode>]) -> WolfState {
        WolfState {
            windows: vec![saved_window_with_tabs(cwd, layouts)],
        }
    }

    /// A `state.json` tree with one window at [`SAVED`] carrying `count` tabs, each
    /// with a layout naming a file of its own.
    ///
    /// Every tab here is a USABLE restore candidate, so nothing except the cap can
    /// reduce the plan's tab count: a rule that dropped layout-less tabs, or
    /// deduplicated tabs sharing a project, would still return one `TabPlan` per tab
    /// for this tree.
    fn many_tab_state(count: usize) -> WolfState {
        let layouts: Vec<Option<PaneNode>> = (0..count)
            .map(|index| Some(leaf(&format!("{MANY_TAB_FILE}{index}.md"))))
            .collect();
        saved_state_with_tabs(SAVED, &layouts)
    }

    /// The layouts a plan's `tabs` carry, in order — the value every tab-order
    /// assertion below compares against.
    fn planned_layouts(plan: &RestorePlan) -> Vec<Option<PaneNode>> {
        plan.tabs.iter().map(|tab| tab.layout.clone()).collect()
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
                // The compatibility row: nothing saved gives today's `launch_decision`, one layout-less tab, no
                // size, and the sidebar default `true` (a first launch shows the tree).
                desc: "no saved state at all => today's launch decision, one layout-less tab, no \
                       size, and the sidebar default of SHOWN",
                saved: None,
                process_cwd: Some(PROJECT),
                home: Some(HOME),
                dirs: &[PROJECT, HOME],
                want: RestorePlan {
                    core_cwd: PathBuf::from(PROJECT),
                    empty: false,
                    tabs: vec![TabPlan { layout: None }],
                    window_size: None,
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // The desktop-launcher shape: the process cwd is $HOME, so the
                // RESTORED cwd gets its turn and its layout comes with it.
                desc: "saved dir exists and the process cwd is home => Folder{saved} + one tab \
                       carrying its layout",
                saved: Some(saved_state(SAVED, Some(leaf(SAVED_FILE)))),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME, SAVED],
                want: RestorePlan {
                    core_cwd: PathBuf::from(SAVED),
                    empty: false,
                    tabs: vec![TabPlan {
                        layout: Some(PaneNode::Leaf {
                            path: SAVED_FILE.to_string(),
                            preview: false,
                        }),
                    }],
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // The saved project was deleted: its layout's leaves point into a missing directory, so
                // rebuilding it would `:edit` files that are gone.
                desc: "saved dir is GONE => EmptyState at home, layout DROPPED, size kept",
                saved: Some(saved_state(GONE, Some(leaf("/projects/deleted/x.md")))),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME],
                want: RestorePlan {
                    core_cwd: PathBuf::from(HOME),
                    empty: true,
                    tabs: vec![TabPlan { layout: None }],
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
                    tabs: vec![TabPlan { layout: None }],
                    window_size: None,
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // A tab-less window keeps its geometry (`window_size_is_independent_of_the_cwd_decision`), and
                // the tabs floor keeps it one window (`a_window_always_plans_at_least_one_tab`).
                desc: "saved `tabs: []` => no cwd and no layout, but the window SIZE survives",
                saved: Some(saved_state_without_tabs()),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME, SAVED],
                want: RestorePlan {
                    core_cwd: PathBuf::from(HOME),
                    empty: true,
                    tabs: vec![TabPlan { layout: None }],
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
                    tabs: vec![TabPlan { layout: None }],
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: true,
                },
            },
            PlanCase {
                desc: "saved cwd exists but `layout: None` => Folder{saved}, one layout-less tab",
                saved: Some(saved_state(SAVED, None)),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME, SAVED],
                want: RestorePlan {
                    core_cwd: PathBuf::from(SAVED),
                    empty: false,
                    tabs: vec![TabPlan { layout: None }],
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // The shared predicate on the restored candidate: a hand-edited `/` must not point FileSearch at
                // the whole filesystem.
                desc: "a saved cwd of `/` is refused by the SAME predicate the process cwd faces",
                saved: Some(saved_state("/", Some(leaf("/x.md")))),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &["/", HOME],
                want: RestorePlan {
                    core_cwd: PathBuf::from(HOME),
                    empty: true,
                    tabs: vec![TabPlan { layout: None }],
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: true,
                },
            },
            PlanCase {
                // The read-back row: every row above saved the sidebar shown, so a hardcoded `true` would pass
                // them all; this one makes the field a reading.
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
                    tabs: vec![TabPlan {
                        layout: Some(PaneNode::Leaf {
                            path: SAVED_FILE.to_string(),
                            preview: false,
                        }),
                    }],
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: false,
                },
            },
            PlanCase {
                // The preference is independent of the launch decision: the empty state overrides it at apply
                // time (`sidebar_is_shown`); folding it in here would lose it, then persist the loss.
                desc: "a HIDDEN sidebar survives an empty-state launch decision unchanged",
                saved: Some(saved_state_with_sidebar(GONE, None, false)),
                process_cwd: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME],
                want: RestorePlan {
                    core_cwd: PathBuf::from(HOME),
                    empty: true,
                    tabs: vec![TabPlan { layout: None }],
                    window_size: Some(SAVED_SIZE),
                    sidebar_visible: false,
                },
            },
        ]
    }

    // --- one window's launch plan ---

    #[test]
    fn a_layout_is_never_restored_into_a_different_project() {
        // A saved layout's absolute leaf paths belong to one project: rebuilt elsewhere they open that
        // project's files in this window, then persist back over the saved tree.
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
            got.tabs,
            vec![TabPlan { layout: None }],
            "the saved layout must NOT be attached when the chosen project ({PROJECT}) is not the \
             project it was saved from ({SAVED}). Its leaves are absolute paths under {SAVED}, so \
             restoring them here would open another project's files — and the next snapshot would \
             then overwrite the saved tree with them. The single layout-less `TabPlan` is the \
             never-empty FLOOR: a window is always built with one tab, so an empty vec here would \
             leave the window with no page at all."
        );
        assert_eq!(
            got.window_size,
            Some(SAVED_SIZE),
            "the WINDOW SIZE is not project-scoped and is restored regardless"
        );
    }

    #[test]
    fn a_layout_is_restored_when_the_process_cwd_is_the_saved_project() {
        // The positive twin, and the only path the S1 / S2 relaunch smokes take (same cwd, layout
        // attached). A guard dropping every layout passes the negative test alone.
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
                tabs: vec![TabPlan {
                    layout: Some(layout)
                }],
                window_size: Some(SAVED_SIZE),
                sidebar_visible: true,
            },
            "relaunching IN the saved project must restore its layout verbatim, on one tab. This \
             is the exact shape the relaunch smokes run: same cwd, same state dir, layout expected \
             back."
        );
    }

    #[test]
    fn window_size_is_independent_of_the_cwd_decision() {
        // Deliberate: a window whose project is gone keeps its size, a property of the window, not the
        // project. Do not generalize the layout guard to it.
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
            planned_layouts(&got),
            vec![None],
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

    // --- restore_plans: the whole session's launch, rather than one window's ---

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
            planned_layouts(&plans[0]),
            vec![Some(leaf(SAVED_FILE))],
            "plan 0 must carry the first window's own layout on its one tab: it relaunches into \
             the project the layout was saved from, so the files that window had open come back \
             with it"
        );
        assert_eq!(
            planned_layouts(&plans[1]),
            vec![Some(leaf(SAVED_B_FILE))],
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

        // Premise, measured: if `is_dir` refused a saved directory the window would fall to EmptyState
        // and the assertions below would pass for the wrong reason.
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
            planned_layouts(&plans[0]),
            vec![None],
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
            planned_layouts(&plans[1]),
            vec![Some(leaf(SAVED_B_FILE))],
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
            planned_layouts(&plans[1]),
            vec![Some(leaf(SAVED_B_FILE))],
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
                tabs: vec![TabPlan { layout: None }],
                window_size: None,
                sidebar_visible: true,
            },
            "a plan built from no saved state and no process cwd must be the empty state at home \
             with nothing restored: `empty` drives the status page and keeps the sidebar hidden, \
             ONE `TabPlan` carrying `layout: None` means a single tab holding a fresh single pane, \
             `window_size: None` means GTK's default geometry rather than another window's, and \
             `sidebar_visible: true` is the PREFERENCE default — the empty state overrides it at \
             apply time through `sidebar_is_shown`, never here"
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

        // Non-vacuous cap: exactly at the limit nothing truncates, catching a `take` off by one or a
        // cap of zero.
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

    // --- restore_plans: every saved tab, not just the selected one ---

    /// **THE RESTORE KEYSTONE.** A window saved with three tabs plans three tabs, in
    /// saved order, each carrying its own layout.
    ///
    /// The revert it exists to catch is part one's shape — building the plan from
    /// `saved_tabs.first()` alone. Under it the launch opens one tab, the other two
    /// contribute nothing, and the next gather writes the one-tab window back over
    /// the save: the user's other tabs are GONE rather than merely unopened, and no
    /// message anywhere says so.
    ///
    /// Isolated: every row of [`restore_plans_table`] holds at most one saved tab, so
    /// a first-only rule satisfies all ten of them.
    #[test]
    fn restore_plans_covers_every_saved_tab_in_order() {
        let layouts: Vec<Option<PaneNode>> = SAVED_TAB_FILES
            .into_iter()
            .map(|file| Some(leaf(file)))
            .collect();
        let saved = saved_state_with_tabs(SAVED, &layouts);

        // The relaunch shape exactly: same project, same directory the tabs were
        // saved from — which is the only shape in which a tab list is restored at all.
        let plans = restore_plans(
            Some(&saved),
            Some(Path::new(SAVED)),
            Some(Path::new(HOME)),
            dirs_predicate(&[HOME, SAVED]),
        );

        assert_eq!(
            plans.len(),
            1,
            "precondition: one saved window is still one launch plan. Tabs live INSIDE a window's \
             plan; a tab list that turned into extra windows would be a different bug entirely. \
             Plans were {plans:?}"
        );
        assert_eq!(
            plans[0].core_cwd,
            PathBuf::from(SAVED),
            "premise: the window must open the project its tabs were saved from. Anything else \
             means `same_project` is false, so the tab assertions below would be measuring the \
             DIFFERENT-project arm, which correctly plans one layout-less tab — and this test \
             would report a lost tab list where the rule was working. That arm has a test of its \
             own; see the tabs-restored-only-into-their-own-project rule."
        );
        assert_eq!(
            plans[0].tabs.len(),
            SAVED_TAB_FILES.len(),
            "a window saved with {} tabs must plan {} of them, got {}. ONE is part one's shape \
             surviving — the plan built from `saved_tabs.first()` alone — and it is not a missing \
             feature but a DATA LOSS: the extra tabs never open, so no page ever gathers them, and \
             the first snapshot after the launch writes a one-tab window back over the saved tree. \
             The user's other tabs are gone by the time they notice one is missing. ZERO would \
             leave the window with no page at all. Plan was {:?}",
            SAVED_TAB_FILES.len(),
            SAVED_TAB_FILES.len(),
            plans[0].tabs.len(),
            plans[0]
        );
        assert_eq!(
            planned_layouts(&plans[0]),
            layouts,
            "the tabs must carry their OWN layouts in SAVED ORDER. The three leaf paths are \
             distinct precisely so this is decidable from the value: a REVERSED walk restores the \
             right three tabs with the user's tab bar mirrored (and the next save writes that \
             mirror back, so it compounds), and three copies of one layout means every tab was \
             built from `saved_tabs.first()`. A `None` in the middle means a tab's layout was \
             dropped on the way into the plan — a tab's open buffer paths ride ONLY inside \
             `TabState::layout`, so that tab comes back empty."
        );
    }

    /// A hand-edited or corrupted tab list is truncated to [`MAX_RESTORE_TABS`].
    ///
    /// The [`MAX_RESTORE_WINDOWS`] precedent, bounding a different resource: what a
    /// long tab list costs is the launch's BLOCKING-RPC budget — one `create_tabpage`
    /// and up to one `rebuild_layout` per restored tab, all on the GTK main thread
    /// before the window is interactive — whose worst case is the product of the two
    /// caps. The symbol is read rather than a literal written, so the value can move
    /// without retuning anything here.
    ///
    /// Isolated: yes. No other test builds a tab list anywhere near the cap.
    #[test]
    fn restore_plans_caps_a_hand_edited_tab_list() {
        let hand_edited = many_tab_state(MAX_RESTORE_TABS + 3);
        let plans = restore_plans(
            Some(&hand_edited),
            Some(Path::new(SAVED)),
            Some(Path::new(HOME)),
            dirs_predicate(&[HOME, SAVED]),
        );

        assert_eq!(
            plans.len(),
            1,
            "precondition: one saved window, one plan. Plans were {plans:?}"
        );
        assert_eq!(
            plans[0].tabs.len(),
            MAX_RESTORE_TABS,
            "a saved window carrying {} tabs must plan {MAX_RESTORE_TABS}, got {}. Every tab in \
             this fixture is a usable restore candidate, so the cap is the only thing that can \
             reduce the count — and without it a corrupt state.json puts one blocking \
             `create_tabpage` plus one blocking `rebuild_layout` per entry on the main thread \
             before the first frame, so the window is unresponsive for as long as the file is \
             long.",
            MAX_RESTORE_TABS + 3,
            plans[0].tabs.len()
        );
        assert_eq!(
            plans[0].tabs[0].layout,
            Some(leaf(&format!("{MANY_TAB_FILE}0.md"))),
            "truncation must keep the FIRST tabs: taking the last {MAX_RESTORE_TABS} would satisfy \
             the count above while silently dropping the tabs saved earliest, and tab order is the \
             user's own"
        );

        // Non-vacuous cap: exactly at the limit nothing truncates, catching a `take` off by one or a
        // cap of zero.
        let exactly_at_cap = many_tab_state(MAX_RESTORE_TABS);
        let capped = restore_plans(
            Some(&exactly_at_cap),
            Some(Path::new(SAVED)),
            Some(Path::new(HOME)),
            dirs_predicate(&[HOME, SAVED]),
        );
        assert_eq!(
            capped[0].tabs.len(),
            MAX_RESTORE_TABS,
            "a window with exactly {MAX_RESTORE_TABS} saved tabs must restore ALL of them; without \
             this the truncation assertion above would also pass with the cap mistakenly one too \
             tight, which would lose a tab from a session the user really did assemble"
        );
    }

    /// Every plan carries at least ONE tab, whatever the save held.
    ///
    /// The tab-level twin of the never-empty window floor, and the same class of
    /// failure one level down: a window is built with the tabs its plan names, so an
    /// empty vec is a window with no page — a toplevel holding a tab bar and no
    /// editor, with no way to reach one, since New Tab needs a live core the window
    /// never wired.
    ///
    /// The three inputs are the three routes to "nothing to restore": a saved window
    /// whose `tabs` array is empty (reachable from a real save — it is what a window
    /// whose every tab's gate was shut would write), a saved project that has since
    /// been deleted, and no saved state at all.
    ///
    /// Not isolated: deleting the floor also reddens two rows of
    /// [`restore_plans_table`]. This is where the consequence is named.
    #[test]
    fn a_window_always_plans_at_least_one_tab() {
        let no_tabs = saved_state_without_tabs();
        let project_gone = saved_state(GONE, Some(leaf("/projects/deleted/x.md")));

        let probes: [(&str, Option<&WolfState>); 3] = [
            ("a saved window whose `tabs` array is empty", Some(&no_tabs)),
            (
                "a saved window whose project directory has been deleted",
                Some(&project_gone),
            ),
            ("no saved state at all", None),
        ];

        for (desc, saved) in probes {
            let plans = restore_plans(
                saved,
                Some(Path::new(HOME)),
                Some(Path::new(HOME)),
                dirs_predicate(HOME_ONLY_DIRS),
            );
            assert_eq!(
                plans.len(),
                1,
                "precondition: {desc} must still plan exactly one window. Plans were {plans:?}"
            );
            assert_eq!(
                plans[0].tabs,
                vec![TabPlan { layout: None }],
                "{desc} must plan exactly ONE layout-less tab, got {:?}. An EMPTY vec is not an \
                 empty session but a window with no page: the tab view is built from this list, so \
                 the toplevel would map with a bar and no editor and nothing the user could press \
                 to get one. MORE than one tab, or a tab carrying a layout, would mean a launch \
                 with nothing to restore invented pages the save never held — and the next \
                 snapshot would write them back as though the user had opened them.",
                plans[0].tabs
            );
        }
    }

    /// A saved tab list is restored only into the project it was saved from.
    ///
    /// The tab-level statement of the rule
    /// [`a_layout_is_never_restored_into_a_different_project`] makes for one layout,
    /// and the cost of getting it wrong scales with the tab count: every tab's leaf
    /// paths are absolute paths belonging to ONE project, so restoring the list into
    /// a window launched somewhere else opens N tabs full of another project's files
    /// — and the next snapshot writes that mistake over the saved tree, so the
    /// session the tabs came from is destroyed rather than merely mis-displayed.
    ///
    /// Isolated from [`restore_plans_covers_every_saved_tab_in_order`]: that test
    /// relaunches IN the saved project, so it cannot see this rule at all.
    #[test]
    fn tabs_are_restored_only_into_the_project_they_were_saved_from() {
        let layouts: Vec<Option<PaneNode>> = SAVED_TAB_FILES
            .into_iter()
            .map(|file| Some(leaf(file)))
            .collect();
        let saved = saved_state_with_tabs(SAVED, &layouts);

        // The process cwd is a REAL project of its own, and it wins — launching Wolf
        // from a directory is the Linux idiom.
        let plans = restore_plans(
            Some(&saved),
            Some(Path::new(PROJECT)),
            Some(Path::new(HOME)),
            dirs_predicate(&[PROJECT, SAVED, HOME]),
        );

        assert_eq!(
            plans[0].core_cwd,
            PathBuf::from(PROJECT),
            "premise: the process cwd must win, or the assertion below is about the same-project \
             arm and proves nothing"
        );
        assert!(
            !plans[0].empty,
            "premise: {PROJECT} is a real directory the injected is_dir accepts, so this is a \
             project launch rather than the empty state. Were it the empty state, the tab \
             assertion below would hold because no project resolved at all rather than because the \
             tabs were withheld from a DIFFERENT one."
        );
        assert_eq!(
            plans[0].tabs,
            vec![TabPlan { layout: None }],
            "a window launched into {PROJECT} must plan ONE layout-less tab, not the three saved \
             under {SAVED}, got {:?}. Three tabs here means the whole saved tab list was rebuilt \
             inside another project: nvim would be asked to `:edit` absolute paths belonging to \
             {SAVED} in a window rooted at {PROJECT}, the user would find another project's files \
             spread across three tabs, and the next snapshot would write that arrangement back \
             over the save. An EMPTY vec would leave the window with no page at all.",
            plans[0].tabs
        );
    }

    // --- sidebar_is_shown: the user preference versus the empty state ---

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

        // Non-vacuity: the loop covers only the `empty` half, which `|_, _| false` satisfies while hiding
        // the tree in every project.
        assert!(
            sidebar_is_shown(true, false),
            "the happy path (the user wants the sidebar, a project is open) MUST show it — \
             otherwise the file tree is never drawn at all and every assertion above holds \
             vacuously."
        );
    }

    // --- restored_window_size ---

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
                // `f64 as i64` saturates rather than wrapping, so this leaves i32 and `try_from` answers None
                // rather than a clamped nonsense size.
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

    // --- persistable_layout: the single-pane fix, from the save side ---

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
                // The regression row: an arity guard (`leaf_count(..) > 1`) answers `None` here, the macOS
                // single-pane bug where the file is not reopened on relaunch.
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

    // --- snapshot_cwd ---

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
        // The rule alone, so a failure names it: EmptyState hands nvim `$HOME` only because a process
        // needs a cwd; persisting it makes every launcher start a project at `$HOME` next time.
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

    // --- may_snapshot: the data-loss gate ---

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

    // --- snapshot_state ---

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

        let subtree = window_subtree(
            WIDTH,
            HEIGHT,
            vec![tab_subtree(SAVED.to_string(), true, Some(leaf(SAVED_FILE)))],
        )
        .expect(
            "a window contributing ONE tab must assemble into a subtree; `None` is reserved for \
             the empty contribution set",
        );
        let state =
            snapshot_state(vec![subtree]).expect("one window's subtree must assemble into a tree");

        assert_eq!(
            state.windows.len(),
            1,
            "one contributing window must assemble into exactly one persisted window — the \
             assembler adds none and drops none. `snapshot_state_carries_every_window_in_order` is \
             where the N-window claim lives."
        );
        let window = &state.windows[0];
        assert_eq!(
            window.tabs.len(),
            1,
            "…and one contributed tab into exactly one persisted tab; \
             `window_subtree_writes_every_tab_in_order` is where the N-tab claim lives"
        );

        // Window position is never persisted on Wayland (a client gets no position or screen geometry),
        // so these are a deliberate zero, not a coordinate to trust.
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
        let hidden_subtree =
            window_subtree(WIDTH, HEIGHT, vec![tab_subtree(String::new(), false, None)])
                .expect("a window contributing one tab must assemble into a subtree");
        let hidden = snapshot_state(vec![hidden_subtree])
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

    // --- window_subtree: one window's persisted subtree, N tabs ---

    /// One window's subtree carries EVERY tab it was handed, in tab order, plus a
    /// deliberate `0.0` position and the size it was given.
    ///
    /// The write-side twin of [`restore_plans_covers_every_saved_tab_in_order`]:
    /// restore walks a window's tabs positionally, so the order this assembles in is
    /// the order the next launch opens them.
    ///
    /// FALSIFYING EDIT: `tabs.truncate(1)`. The window persists whichever tab happens
    /// to be first and the user's other tabs are gone at the next launch — silently,
    /// because a one-tab window is exactly what a correct save of a one-tab session
    /// looks like.
    ///
    /// Reverts inherited from [`snapshot_state_is_one_window_one_tab`]: writing a
    /// non-zero `x` or `y` fails the position assertions there as well as here.
    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "x/y are asserted to be the literal 0.0 this function writes unconditionally, \
                  and the dimensions are exact `f64::from(i32)` widenings — an epsilon would let \
                  a garbage coordinate through, which is the whole point of the assertion \
                  (the `mouse::scroll_threshold_table` precedent)"
    )]
    fn window_subtree_writes_every_tab_in_order() {
        const WIDTH: i32 = 1200;
        const HEIGHT: i32 = 640;

        // Three tabs with differing layouts, so order is decidable from the value; cwd and sidebar
        // preference match because both are per window, fanned to each tab.
        let tabs: Vec<TabState> = SAVED_TAB_FILES
            .into_iter()
            .map(|file| tab_subtree(SAVED.to_string(), true, Some(leaf(file))))
            .collect();

        let window = window_subtree(WIDTH, HEIGHT, tabs.clone()).unwrap_or_else(|| {
            panic!(
                "window_subtree refused a window contributing {} tabs. `None` means \"commit \
                 nothing\" and is reserved for the EMPTY contribution set; answering it here drops \
                 a live window out of the committed tree, and the next launch reopens one fewer \
                 window than the user had.",
                SAVED_TAB_FILES.len()
            )
        });

        assert_eq!(
            window.tabs, tabs,
            "the subtree must carry every contributed tab, in order. THREE failures are \
             distinguishable and each loses something different: a SHORTER vec drops whole tabs — \
             their files ride only inside `TabState::layout`, so those tabs do not come back and \
             the save that dropped them is what the next launch reads; a REVERSED vec restores the \
             user's tabs mirrored, and since the next gather writes the mirror back the swap \
             compounds every run; and a tab whose `layout` is `None` where a layout was \
             contributed comes back as an empty pane."
        );
        assert_eq!(
            window.tabs.len(),
            SAVED_TAB_FILES.len(),
            "…and exactly that many, so a duplicated tab cannot hide inside an equality that \
             happens to line up"
        );

        // Window position is never persisted on Wayland (a client gets no position or screen geometry),
        // so these are a deliberate zero, not a coordinate to trust.
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

        // `tab_subtree` is the only way into this vec, so its round trip is stated here: cwd, preference
        // and layout are the values passed.
        let tab = &window.tabs[0];
        assert_eq!(tab.cwd, SAVED, "the cwd round-trips verbatim");
        assert!(
            tab.sidebar_visible,
            "`sidebar_visible` must round-trip the argument it was given. `restore_plans` READS \
             this field back off `tabs.first()`, so a write side that ignored its argument would \
             hand the next launch a preference the user never expressed"
        );
        assert_eq!(
            tab.layout,
            Some(leaf(SAVED_TAB_FILES[0])),
            "the layout round-trips verbatim"
        );

        // The other side of `sidebar_visible`, proving it follows its argument, beside the
        // suppressed-layout shape.
        let hidden = window_subtree(WIDTH, HEIGHT, vec![tab_subtree(String::new(), false, None)])
            .expect("a window contributing one tab must assemble into a subtree");
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

    /// A window whose tabs contributed NOTHING commits nothing, rather than
    /// committing a window with an empty `tabs` array.
    ///
    /// The tab-level twin of [`an_empty_contribution_set_commits_nothing`], and the
    /// reason it must hold at this level too is already written down in
    /// [`restore_plans_table`]'s "saved `tabs: []`" row: restore reads a window whose
    /// `tabs` array is empty as having NO project, no layout and no sidebar
    /// preference — the empty state. So committing one is not "a window with no tabs
    /// yet", it is the destruction of that window's whole session, written by a
    /// process that is running perfectly and reported as a successful save.
    ///
    /// An empty contribution set is reachable without any bug at all: every one of a
    /// window's tabs refuses while its panes are still binding, and a window whose
    /// restore has not materialized yet holds no cache for its queued tabs until
    /// [`pending_tab_states`] supplies their seeds.
    ///
    /// Isolated: yes. No other test hands `window_subtree` an empty vec.
    #[test]
    fn a_window_with_no_tab_contribution_commits_nothing() {
        const WIDTH: i32 = 1200;
        const HEIGHT: i32 = 640;

        assert_eq!(
            window_subtree(WIDTH, HEIGHT, Vec::new()),
            None,
            "a window contributing NO tabs must commit nothing. \
             `Some(WindowState {{ tabs: [], .. }})` here is the value that destroys a project: \
             `restore_plans` reads a window's project, its layout and its sidebar preference off \
             `tabs.first()`, so a `tabs: []` window restores as the EMPTY STATE — the next launch \
             opens a window with no folder, and the gather after that writes that emptiness back. \
             `None` is the mechanical equivalent of the refused gate: the window drops out of the \
             contribution set, `snapshot_state` assembles the windows that did contribute, and if \
             none did it commits nothing at all."
        );

        // Non-vacuity: the rule above is satisfied by an assembler that answers `None`
        // for everything, which would persist no window ever.
        assert!(
            window_subtree(
                WIDTH,
                HEIGHT,
                vec![tab_subtree(SAVED.to_string(), true, Some(leaf(SAVED_FILE)))]
            )
            .is_some(),
            "one contributing tab MUST assemble into a committable subtree — otherwise the rule \
             above holds vacuously and nothing is ever persisted at all"
        );
    }

    // --- seeded_tab_state: what a restored tab contributes before it is ever selected ---

    /// One [`seeded_tab_state`] table row. The expectation is DATA — the cwd is a
    /// literal rather than a second call to `snapshot_cwd`, because a mirror
    /// implementation would agree with any bug it also contained (the
    /// [`may_snapshot_table`] precedent).
    struct SeedCase {
        desc: &'static str,
        restored_layout: Option<PaneNode>,
        empty: bool,
        launch_cwd: &'static str,
        sidebar_visible: bool,
        want: Option<TabState>,
    }

    /// What a restored tab contributes while its own gate has never opened.
    ///
    /// TWO tabs reach it, and neither has a cache to fall back on: one still sitting in
    /// the restore QUEUE, whose page does not exist yet ([`pending_tab_states`]), and a
    /// materialized one whose `realize_layout` bound no pane, so `may_snapshot` refuses
    /// it and its cache stays `None`. A restored tab that DID materialize is gathered
    /// live from its first wake — `realize_layout` creates its panes synchronously —
    /// and never consults the seed at all. Both halves of the rule are load-bearing, in
    /// opposite directions:
    ///
    /// FALSIFYING EDIT 1 — return `None` unconditionally. Neither of those two tabs
    /// contributes anything, so the commit writes a window holding only the tabs with
    /// bound panes: a queued tab is dropped from the saved session while the launch is
    /// still coming up, and nothing re-adds it, because its page does not exist.
    /// [`pending_tab_states_table`] carries that same red one level up.
    ///
    /// FALSIFYING EDIT 2 — return `Some` unconditionally (a seed even where no layout
    /// was restored). A launch whose project has gone plans `layout: None`, so the
    /// seed would be an empty-state tab at `cwd: ""`, and committing it writes that
    /// over the saved project before any pane has bound — which is exactly the loss
    /// `may_snapshot` exists to prevent, arriving through the one path that bypasses
    /// it.
    #[test]
    fn seeded_tab_state_table() {
        let cases = [
            SeedCase {
                // THE ROW THE SEED EXISTS FOR: a tab restored with a layout carries
                // that layout back out, unchanged, before any pane of its own exists.
                desc: "a restored layout is seeded verbatim, at the launch cwd",
                restored_layout: Some(leaf(SAVED_FILE)),
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: true,
                want: Some(TabState {
                    cwd: SAVED.to_string(),
                    sidebar_visible: true,
                    layout: Some(leaf(SAVED_FILE)),
                }),
            },
            SeedCase {
                // The preference is read at the same instant, so hiding the sidebar and quitting without touching
                // the extra tabs does not bring it back next launch.
                desc: "the live sidebar preference is carried, not assumed",
                restored_layout: Some(leaf(SAVED_FILE)),
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: false,
                want: Some(TabState {
                    cwd: SAVED.to_string(),
                    sidebar_visible: false,
                    layout: Some(leaf(SAVED_FILE)),
                }),
            },
            SeedCase {
                // A SPLIT seeds as readily as a leaf: the seed is the restore plan's
                // own tree and nothing is re-derived from it.
                desc: "a multi-pane restored layout is seeded whole",
                restored_layout: Some(split(vec![leaf(SAVED_FILE), leaf("")])),
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: true,
                want: Some(TabState {
                    cwd: SAVED.to_string(),
                    sidebar_visible: true,
                    layout: Some(split(vec![leaf(SAVED_FILE), leaf("")])),
                }),
            },
            SeedCase {
                // The empty-state cwd rule from `snapshot_cwd`: always `""`, never the fallback handed Neovim,
                // or the next launch declares `$HOME` a project.
                desc: "an empty-state launch seeds the \"\" cwd sentinel, never $HOME",
                restored_layout: Some(leaf(SAVED_FILE)),
                empty: true,
                launch_cwd: HOME,
                sidebar_visible: true,
                want: Some(TabState {
                    cwd: String::new(),
                    sidebar_visible: true,
                    layout: Some(leaf(SAVED_FILE)),
                }),
            },
            SeedCase {
                // The refusal row: no restored layout, no seed. The tab whose project has gone contributes
                // nothing until its own gather succeeds.
                desc: "a tab restored with NO layout takes no seed",
                restored_layout: None,
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: true,
                want: None,
            },
            SeedCase {
                desc: "…and an empty-state launch with no restored layout takes none either",
                restored_layout: None,
                empty: true,
                launch_cwd: HOME,
                sidebar_visible: true,
                want: None,
            },
        ];

        for case in &cases {
            let got = seeded_tab_state(
                case.restored_layout.as_ref(),
                case.empty,
                Path::new(case.launch_cwd),
                case.sidebar_visible,
            );
            assert_eq!(
                got,
                case.want,
                "seeded_tab_state(restored_layout = {:?}, empty = {}, launch_cwd = {:?}, \
                 sidebar_visible = {}) must be {:?}, got {got:?} — {}. A `None` where a seed is \
                 due ERASES a tab that has no cache to fall back on — one still queued for \
                 materialization, or one whose `realize_layout` bound no pane — so it contributes \
                 nothing and the next commit writes the window back without it. A `Some` where \
                 `None` is due commits an empty-state tab over a saved project before any pane has \
                 bound. A wrong cwd re-roots the tab somewhere the window is not; a wrong layout \
                 is the tab's files, which ride only inside `TabState::layout`.",
                case.restored_layout,
                case.empty,
                case.launch_cwd,
                case.sidebar_visible,
                case.want,
                case.desc
            );
        }

        // --- Non-vacuity, in both directions ----------------------------------
        assert!(
            cases.iter().any(|case| case.want.is_some()),
            "no seeded_tab_state row expects a seed, so a function returning None for everything \
             would satisfy every row above — and every tab with no cache of its own, queued or \
             bound to no pane, would be erased by the next commit."
        );
        assert!(
            cases.iter().any(|case| case.want.is_none()),
            "no seeded_tab_state row expects a refusal, so a function returning Some for \
             everything would pass — and a launch whose project has gone would commit an \
             empty-state tab over the saved one before a single pane had bound."
        );

        // The join the round trip rests on: the seeded cwd is `snapshot_cwd`'s answer. If they diverge,
        // launch, early quit and relaunch stop being idempotent and each run persists the drift.
        for (empty, launch_cwd) in [(false, SAVED), (true, HOME), (false, PROJECT)] {
            let seeded =
                seeded_tab_state(Some(&leaf(SAVED_FILE)), empty, Path::new(launch_cwd), true)
                    .unwrap_or_else(|| {
                        panic!(
                    "precondition: a seed carrying a layout must exist for (empty = {empty}, \
                     launch_cwd = {launch_cwd:?}), or the cwd comparison below is vacuous"
                )
                    });
            assert_eq!(
                seeded.cwd,
                snapshot_cwd(empty, Path::new(launch_cwd), None),
                "the SEEDED cwd and the GATHERED cwd must be the same string for (empty = \
                 {empty}, launch_cwd = {launch_cwd:?}). A tab with no gather of its own \
                 contributes the seed and every other tab contributes `snapshot_cwd`'s answer, so \
                 a divergence makes one saved window's tabs disagree about where the project is \
                 depending on which of them had bound when the commit happened — and the round \
                 trip stops being idempotent, since the seeded value is what the next launch reads \
                 back."
            );
        }
    }

    // --- pending_tab_states: what a window contributes for tabs queued but not yet created ---

    /// One [`pending_tab_states`] table row. The expectation is DATA for the reason
    /// [`SeedCase`] gives: a `want` computed by calling `seeded_tab_state` again would
    /// agree with any bug that call also contained.
    struct PendingCase {
        desc: &'static str,
        layouts: Vec<Option<PaneNode>>,
        empty: bool,
        launch_cwd: &'static str,
        sidebar_visible: bool,
        want: Vec<TabState>,
    }

    /// What a window contributes for the tabs its launch has QUEUED but not yet
    /// materialized: one seed per queued layout, in queued order.
    ///
    /// The queue is drained on a render wake, and the commit does not wait for it.
    /// `window::build` drives `shell::on_render_wake` before the window is ever
    /// presented, while Neovim's current tabpage is still `0` and tabs 1..N have no
    /// pages at all, so that commit assembles the window from tab 0 alone unless the
    /// queued tabs are contributed too.
    ///
    /// FALSIFYING EDIT — return `vec![]` unconditionally. A window restored with three
    /// tabs commits `tabs: [one]` over the saved three-tab session, and the two the
    /// user has not seen yet are gone.
    ///
    /// **The loss is PERSISTENT rather than a frame of transient wrongness**, which is
    /// the half worth reading twice. `shell::on_render_wake` commits UNGATED by
    /// liveness, while [`crate::tabs_ui::restore_press`] answers `Wait` forever behind
    /// a wedged or dead Neovim — so a launch that never materializes at all still
    /// writes the one-tab tree, and nothing later re-adds the queued tabs, because the
    /// queue is the only record of them: their pages do not exist, so no cache and no
    /// live gather can speak for them. That file is what the next launch reads.
    #[test]
    fn pending_tab_states_table() {
        let cases = [
            PendingCase {
                // The row the seam exists for: queued layouts return in order, and a layout-less entry is
                // dropped, so a `None` in the middle exposes a position-preserving rule.
                desc: "two queued layouts around one carrying none => two seeds, in queued order",
                layouts: vec![tab_layout(0), None, tab_layout(1)],
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: true,
                want: vec![
                    TabState {
                        cwd: SAVED.to_string(),
                        sidebar_visible: true,
                        layout: Some(leaf(SAVED_TAB_FILES[0])),
                    },
                    TabState {
                        cwd: SAVED.to_string(),
                        sidebar_visible: true,
                        layout: Some(leaf(SAVED_TAB_FILES[1])),
                    },
                ],
            },
            PendingCase {
                // The steady state: an empty queue contributes nothing, or every later commit grows a tab.
                desc: "an empty queue contributes nothing",
                layouts: Vec::new(),
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: true,
                want: Vec::new(),
            },
            PendingCase {
                // The refusal half, lifted to the vec: a layout-less queued tab (project gone) contributes
                // nothing, not an empty-state tab.
                desc: "a queue of tabs that carry no layout contributes nothing",
                layouts: vec![None, None],
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: true,
                want: Vec::new(),
            },
            PendingCase {
                // The empty-state cwd rule from `snapshot_cwd`: always `""`, never the fallback handed Neovim,
                // or the next launch declares `$HOME` a project.
                desc: "an empty-state launch seeds the \"\" cwd sentinel, never $HOME",
                layouts: vec![Some(leaf(SAVED_FILE))],
                empty: true,
                launch_cwd: HOME,
                sidebar_visible: true,
                want: vec![TabState {
                    cwd: String::new(),
                    sidebar_visible: true,
                    layout: Some(leaf(SAVED_FILE)),
                }],
            },
            PendingCase {
                // The live preference reaches every queued tab, so hiding the sidebar and quitting before the
                // restore ran keeps it hidden on all of them.
                desc: "the live sidebar preference is carried to every queued tab",
                layouts: vec![tab_layout(0), tab_layout(1)],
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: false,
                want: vec![
                    TabState {
                        cwd: SAVED.to_string(),
                        sidebar_visible: false,
                        layout: Some(leaf(SAVED_TAB_FILES[0])),
                    },
                    TabState {
                        cwd: SAVED.to_string(),
                        sidebar_visible: false,
                        layout: Some(leaf(SAVED_TAB_FILES[1])),
                    },
                ],
            },
        ];

        for case in &cases {
            let got = pending_tab_states(
                &case.layouts,
                case.empty,
                Path::new(case.launch_cwd),
                case.sidebar_visible,
            );
            assert_eq!(
                got,
                case.want,
                "pending_tab_states(layouts = {:?}, empty = {}, launch_cwd = {:?}, \
                 sidebar_visible = {}) must be {:?}, got {got:?} — {}. A SHORT answer \
                 drops queued tabs out of every commit made before the restore \
                 materializes, and `window::build` makes one of those before the window \
                 is even presented: the window writes a one-tab tree over the saved \
                 multi-tab session, and a tab with no page yet has nothing else that can \
                 speak for it. A REORDERED answer hands the next launch the user's tabs \
                 in an order they never arranged, and the gather after that writes the \
                 swap back. A wrong cwd re-roots the tab somewhere the window is not; a \
                 wrong layout is the tab's files, which ride only inside \
                 `TabState::layout`.",
                case.layouts,
                case.empty,
                case.launch_cwd,
                case.sidebar_visible,
                case.want,
                case.desc
            );
        }

        // --- Non-vacuity, in both directions ----------------------------------
        assert!(
            cases.iter().any(|case| !case.want.is_empty()),
            "no pending_tab_states row expects a contribution, so a function answering \
             `vec![]` for everything would satisfy every row above — and that function \
             IS the durability gap this seam exists to close. A launch's queued tabs \
             live nowhere else: their pages do not exist, so neither a cache nor a live \
             gather can contribute them, and a window offering only the tabs that \
             already have pages commits `tabs: [one]` over the whole saved session. Nor \
             is that a frame of transient wrongness — `shell::on_render_wake` commits \
             ungated by liveness while `tabs_ui::restore_press` answers Wait forever \
             behind a wedged or dead Neovim, so a launch that NEVER materializes still \
             writes the one-tab tree, nothing afterwards re-adds the missing tabs, and \
             that file is what the next launch reads."
        );
        assert!(
            cases.iter().any(|case| case.want.is_empty()),
            "no pending_tab_states row expects an EMPTY answer, so a function that \
             manufactured a seed for every queued entry would pass. A queued tab \
             carrying no layout must contribute nothing: its seed would be an \
             empty-state tab at cwd \"\", committed over the saved one before any pane \
             had bound — the loss `seeded_tab_state`'s refusal half exists to prevent."
        );

        // The join with the per-tab seed: `seeded_tab_state` mapped over the queue, so a queued tab and
        // an ungathered live one agree; re-deriving here would let one window's tabs disagree on cwd.
        let layouts = [tab_layout(0), tab_layout(1)];
        let pending = pending_tab_states(&layouts, false, Path::new(SAVED), true);
        for (index, layout) in layouts.iter().enumerate() {
            assert_eq!(
                pending.get(index),
                seeded_tab_state(layout.as_ref(), false, Path::new(SAVED), true).as_ref(),
                "queued tab {index} must contribute exactly what `seeded_tab_state` \
                 contributes for the same layout. `None` here means the queued tab \
                 contributes nothing at all, which is the one-tab commit above; a \
                 DIFFERENT value means this seam re-derives the cwd or the preference \
                 rather than mapping the one seed rule, so two tabs of the same window \
                 disagree about where the project is and the disagreement is what gets \
                 saved."
            );
        }
    }

    // --- ordered_tab_states: the persisted order is page order, the user's arrangement ---

    /// One [`ordered_tab_states`] row: the `(page position, tab)` pairs as they were
    /// COLLECTED, and the tabs as they must be CONTRIBUTED.
    ///
    /// Both sides are DATA. A `want` derived by sorting the input inside the test would
    /// agree with whatever the seam did, which is exactly the failure this table exists
    /// to catch.
    struct OrderCase {
        desc: &'static str,
        positioned: Vec<(i32, TabState)>,
        want: Vec<TabState>,
    }

    /// A window's tabs are persisted in PAGE order — the order they sit in the tab bar
    /// — whatever order they were collected in.
    ///
    /// `tab_view::TabsHandle::entries` is INSERTION order: a `Vec` that
    /// `TabsInner::open_tab` pushes to, with nothing reconciling it against
    /// `adw::TabView`'s page positions. Dragging a tab along the bar reorders the
    /// pages — `AdwTabBar` does that with no property that turns it off, and nothing
    /// in this tree disables it — so the two orders diverge the first time a user
    /// rearranges their tabs. `window::WindowHandle::cached_subtree` therefore pairs
    /// each cached tab with `view.page_position(page)` and runs the pairs through here
    /// before handing the result to [`window_contribution`] as its `live`. This is the
    /// ONE place that reconciliation happens.
    ///
    /// libadwaita's other reorder route — the `Ctrl+Shift+Page_Up` /
    /// `Ctrl+Shift+Page_Down` shortcuts `AdwTabView:shortcuts` enables by default — is
    /// UNCONFIRMED as a user-reachable action, and no row here rests on it.
    /// `window::wire_keys` installs a bubble-phase key controller on the toplevel and
    /// `input::named_key` maps `Page_Up` / `Page_Down` regardless of modifiers,
    /// returning `Propagation::Stop`, so the chord may reach Neovim and never
    /// `AdwTabView`'s own controller. Nothing has measured which one wins. The drag
    /// route alone is what makes this reconciliation load-bearing.
    ///
    /// FALSIFYING EDIT — strip the positions and return the pairs' `TabState`s in the
    /// order they were given. That is insertion order restored, which is the bug this
    /// seam exists to close rather than a hypothetical one; only the rows whose given
    /// order DIFFERS from their expected order can see it, which is what the first
    /// non-vacuity guard below keeps true.
    ///
    /// # What the TIE row does and does not discriminate
    ///
    /// `adw::TabView` hands out one position per page, so two pairs sharing a position
    /// is not an input production can produce. The row is here to pin WHICH answer the
    /// seam gives for it — the given relative order — so a later rewrite cannot quietly
    /// pick the other one and leave the result depending on the order the pairs happened
    /// to be collected in. It does NOT prove the sort is stable in general: Rust's
    /// `sort_unstable_by_key` falls back to insertion sort on a slice this short, so a
    /// three-element tie cannot tell the two sorts apart. What the row supports is that
    /// a tie has a STATED answer at all.
    #[test]
    fn ordered_tab_states_table() {
        let cases = [
            OrderCase {
                // The reorder row: pairs arrive in creation order after a user drag. No element sits where it
                // arrived, so returning the pairs unsorted fails at every index.
                desc: "pairs given OUT of page order come back in PAGE order",
                positioned: vec![
                    (2, positioned_tab(1)),
                    (0, positioned_tab(2)),
                    (1, positioned_tab(0)),
                ],
                want: vec![positioned_tab(2), positioned_tab(0), positioned_tab(1)],
            },
            OrderCase {
                // The steady state: page and collection order agree and must be left alone; a reversing or
                // descending rule shows here and nowhere else.
                desc: "pairs already in page order pass through unchanged",
                positioned: vec![
                    (0, positioned_tab(0)),
                    (1, positioned_tab(1)),
                    (2, positioned_tab(2)),
                ],
                want: vec![positioned_tab(0), positioned_tab(1), positioned_tab(2)],
            },
            OrderCase {
                // Every live gate shut (launch, before any pane binds): the answer stays empty, which
                // `window_contribution` and `window_subtree` read as queued-seeds-only and commit-nothing.
                desc: "no pairs at all => no tabs at all",
                positioned: Vec::new(),
                want: Vec::new(),
            },
            OrderCase {
                // One tab at a non-zero position: the position is stripped, never an index, or the answer
                // would pad with tabs that do not exist and the next launch would open them.
                desc: "a single pair contributes that pair's tab, position stripped",
                positioned: vec![(2, positioned_tab(1))],
                want: vec![positioned_tab(1)],
            },
            OrderCase {
                // The tie row (see the fn docs): positions 1 and 1 given as tab 0 then tab 1 stay in that order
                // behind the tab at 0, under a stable sort.
                desc: "two pairs sharing a position keep the order they were GIVEN",
                positioned: vec![
                    (1, positioned_tab(0)),
                    (0, positioned_tab(2)),
                    (1, positioned_tab(1)),
                ],
                want: vec![positioned_tab(2), positioned_tab(0), positioned_tab(1)],
            },
        ];

        for case in &cases {
            let got = ordered_tab_states(case.positioned.clone());
            assert_eq!(
                got, case.want,
                "ordered_tab_states({:?}) must be {:?}, got {got:?} — {}. This vec is what \
                 `window_contribution` writes into `state.json` as the window's live tabs, and \
                 `restore_plans` walks a saved window's tabs POSITIONALLY, so the order here is \
                 literally the order the next launch replays. THREE failures are \
                 distinguishable. The tabs in the order the pairs ARRIVED is insertion order — \
                 the order the pages were created — so a user who drags a tab along the bar has \
                 their arrangement silently discarded at the next launch, and because the gather \
                 after that writes the collected order back the discard is permanent rather than \
                 one run's. There is no third order to fall back on: reordering GTK pages does \
                 NOT reorder Neovim's tabpages, so page order and tabpage order genuinely \
                 diverge after a drag, and persistence follows what the user SEES. A SHORT vec \
                 drops tabs outright, and a tab's files ride only inside `TabState::layout`. A \
                 LONGER one duplicates a tab, which the next launch opens twice and then saves \
                 twice, so the count doubles every run.",
                case.positioned, case.want, case.desc
            );
        }

        // --- Non-vacuity, in two directions -----------------------------------
        assert!(
            cases.iter().any(|case| {
                let given: Vec<&TabState> = case.positioned.iter().map(|(_, tab)| tab).collect();
                let want: Vec<&TabState> = case.want.iter().collect();
                given != want
            }),
            "no ordered_tab_states row's GIVEN order differs from its expected order, so a \
             function that stripped the positions and returned the pairs untouched would satisfy \
             every row above — and that function is insertion order, which is the divergence this \
             seam exists to close. A window whose tabs were never dragged already agrees with \
             both answers, so a table of only such rows proves nothing at all."
        );
        assert!(
            cases.iter().any(|case| !case.want.is_empty()),
            "no ordered_tab_states row expects a tab to SURVIVE, so a function answering `vec![]` \
             for everything would satisfy every row above — and a window whose live tabs all \
             vanished here hands `window_contribution` an empty `live`, so unless its restore \
             queue still holds seeds the whole window drops out of the committed tree and the \
             next launch opens one fewer window than the user had."
        );
    }

    // --- window_contribution: live tabs' caches, then queued tabs' seeds ---

    /// One [`window_contribution`] table row. The expectation is DATA — a `want`
    /// assembled by calling the two seams this composes would agree with any bug in
    /// the composition it also contained (the [`SeedCase`] precedent).
    struct ContributionCase {
        desc: &'static str,
        live: Vec<TabState>,
        pending: Vec<Option<PaneNode>>,
        empty: bool,
        launch_cwd: &'static str,
        sidebar_visible: bool,
        want: Option<WindowState>,
    }

    /// What ONE window contributes to `state.json`: every live tab's cache in tab
    /// order, FOLLOWED BY a seed for every tab its launch has queued but not yet
    /// created, under the window's own size — and `None` when neither has anything.
    ///
    /// `window::WindowHandle::cached_subtree` is one call to this, so the composition
    /// is EXECUTED by the fast suite rather than modelled by it: the glue needs a GTK
    /// window and a live core, and a rule left inside it is a rule no headless test
    /// can reach.
    ///
    /// FALSIFYING EDIT 1 — PREPEND the seeds instead of appending them. Every count
    /// survives and every layout is still present, so only the exact-vec rows fail:
    /// the user's tabs come back rearranged, and because the next gather writes the
    /// new order back, the rearrangement compounds on every launch.
    ///
    /// FALSIFYING EDIT 2 — read `pending` and discard it. That is the durability gap
    /// [`pending_tab_states`] exists to close, arriving one level up: `window::build`
    /// drives a commit before the window is presented, while only tab 0 has a page,
    /// so the window writes a one-tab tree over the whole saved session — and behind
    /// a wedged or dead Neovim that commit is the only one there will ever be,
    /// because [`crate::tabs_ui::restore_press`] answers `Wait` forever.
    ///
    /// FALSIFYING EDIT 3 — answer `Some` for the wholly empty contribution. Inherited
    /// from [`window_subtree`], and [`a_window_with_no_tab_contribution_commits_nothing`]
    /// carries the reason: `restore_plans` reads a `tabs: []` window as the EMPTY
    /// STATE, so committing one destroys that window's project silently.
    #[test]
    fn window_contribution_table() {
        // Each `want` carries these as literal f64s, so a widening that drops or swaps a dimension shows
        // in the expectation.
        const WIDTH: i32 = 1200;
        const HEIGHT: i32 = 640;

        let cases = [
            ContributionCase {
                // The ordering row: the live layout names a file no queued layout does, so only the order tells
                // a correct answer from one that prepended the seeds.
                desc: "one live tab and two queued layouts => three tabs, the LIVE one first",
                live: vec![tab_subtree(SAVED.to_string(), true, Some(leaf(SAVED_FILE)))],
                pending: vec![tab_layout(0), tab_layout(1)],
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: true,
                want: Some(WindowState {
                    x: 0.0,
                    y: 0.0,
                    width: 1200.0,
                    height: 640.0,
                    tabs: vec![
                        TabState {
                            cwd: SAVED.to_string(),
                            sidebar_visible: true,
                            layout: Some(leaf(SAVED_FILE)),
                        },
                        TabState {
                            cwd: SAVED.to_string(),
                            sidebar_visible: true,
                            layout: Some(leaf(SAVED_TAB_FILES[0])),
                        },
                        TabState {
                            cwd: SAVED.to_string(),
                            sidebar_visible: true,
                            layout: Some(leaf(SAVED_TAB_FILES[1])),
                        },
                    ],
                }),
            },
            ContributionCase {
                // The pre-materialization launch: `window::build`'s commit runs before present and can find every
                // live gate shut, so only the queue has anything to say.
                desc: "no live tab at all and two queued layouts => the two seeds in queued order",
                live: Vec::new(),
                pending: vec![tab_layout(0), tab_layout(1)],
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: true,
                want: Some(WindowState {
                    x: 0.0,
                    y: 0.0,
                    width: 1200.0,
                    height: 640.0,
                    tabs: vec![
                        TabState {
                            cwd: SAVED.to_string(),
                            sidebar_visible: true,
                            layout: Some(leaf(SAVED_TAB_FILES[0])),
                        },
                        TabState {
                            cwd: SAVED.to_string(),
                            sidebar_visible: true,
                            layout: Some(leaf(SAVED_TAB_FILES[1])),
                        },
                    ],
                }),
            },
            ContributionCase {
                // The steady state after materialization: no queue, exactly the live tabs. Two of them, so
                // truncation or duplication shows.
                desc: "live tabs and an empty queue => exactly the live tabs",
                live: vec![
                    tab_subtree(SAVED.to_string(), true, Some(leaf(SAVED_TAB_FILES[0]))),
                    tab_subtree(SAVED.to_string(), true, Some(leaf(SAVED_TAB_FILES[1]))),
                ],
                pending: Vec::new(),
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: true,
                want: Some(WindowState {
                    x: 0.0,
                    y: 0.0,
                    width: 1200.0,
                    height: 640.0,
                    tabs: vec![
                        TabState {
                            cwd: SAVED.to_string(),
                            sidebar_visible: true,
                            layout: Some(leaf(SAVED_TAB_FILES[0])),
                        },
                        TabState {
                            cwd: SAVED.to_string(),
                            sidebar_visible: true,
                            layout: Some(leaf(SAVED_TAB_FILES[1])),
                        },
                    ],
                }),
            },
            ContributionCase {
                // The commit-nothing row from `window_subtree`: nothing live or queued is a window whose gates
                // are all shut while its panes bind, not a window with no tabs.
                desc: "no live tab and nothing queued => commit NOTHING",
                live: Vec::new(),
                pending: Vec::new(),
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: true,
                want: None,
            },
            ContributionCase {
                // Two rules: a layout-less queued entry is dropped, and the window's `sidebar_visible` reaches
                // the seeds alone; a live cache passes verbatim, carrying its own gather's preference.
                desc: "a queued entry with no layout is dropped, and a live cache passes through \
                       verbatim",
                live: vec![tab_subtree(SAVED.to_string(), true, Some(leaf(SAVED_FILE)))],
                pending: vec![None, tab_layout(2)],
                empty: false,
                launch_cwd: SAVED,
                sidebar_visible: false,
                want: Some(WindowState {
                    x: 0.0,
                    y: 0.0,
                    width: 1200.0,
                    height: 640.0,
                    tabs: vec![
                        TabState {
                            cwd: SAVED.to_string(),
                            sidebar_visible: true,
                            layout: Some(leaf(SAVED_FILE)),
                        },
                        TabState {
                            cwd: SAVED.to_string(),
                            sidebar_visible: false,
                            layout: Some(leaf(SAVED_TAB_FILES[2])),
                        },
                    ],
                }),
            },
            ContributionCase {
                // The seeds use the window-level values: `snapshot_cwd`'s `""` for an empty-state launch,
                // never the `$HOME` handed Neovim.
                desc: "an empty-state launch seeds the \"\" cwd sentinel, never $HOME",
                live: Vec::new(),
                pending: vec![Some(leaf(SAVED_FILE))],
                empty: true,
                launch_cwd: HOME,
                sidebar_visible: true,
                want: Some(WindowState {
                    x: 0.0,
                    y: 0.0,
                    width: 1200.0,
                    height: 640.0,
                    tabs: vec![TabState {
                        cwd: String::new(),
                        sidebar_visible: true,
                        layout: Some(leaf(SAVED_FILE)),
                    }],
                }),
            },
        ];

        for case in &cases {
            let got = window_contribution(
                WIDTH,
                HEIGHT,
                case.live.clone(),
                &case.pending,
                case.empty,
                Path::new(case.launch_cwd),
                case.sidebar_visible,
            );
            assert_eq!(
                got,
                case.want,
                "window_contribution(live = {:?}, pending = {:?}, empty = {}, launch_cwd = \
                 {:?}, sidebar_visible = {}) must be {:?}, got {got:?} — {}. FIVE failures \
                 are distinguishable and each loses something different. A `None` where a \
                 subtree is due drops this whole window out of the committed tree, so the \
                 next launch opens one fewer window than the user had. A SHORT vec drops \
                 tabs, whose files ride only inside `TabState::layout`. A REORDERED vec — \
                 the seeds PREPENDED rather than appended — hands the next launch the \
                 user's tabs in an order they never arranged, and the gather after that \
                 writes the swap back, so it compounds every run. A wrong cwd or a wrong \
                 preference on a seed re-roots a tab somewhere the window is not. A wrong \
                 size is the window geometry, which is not the tabs' to lose.",
                case.live,
                case.pending,
                case.empty,
                case.launch_cwd,
                case.sidebar_visible,
                case.want,
                case.desc
            );
        }

        // --- Non-vacuity, in three directions ---------------------------------
        assert!(
            cases.iter().any(|case| case
                .want
                .as_ref()
                .is_some_and(|window| window.tabs.len() > 1)),
            "no window_contribution row expects more than ONE tab, so a function that answered \
             with the first contribution and stopped would satisfy every row above — and a \
             window's second tab onwards would never be written at all."
        );
        assert!(
            cases
                .iter()
                .any(|case| !case.pending.is_empty() && case.want.is_some()),
            "no window_contribution row expects a QUEUED layout to reach the answer, so a \
             function that read `pending` and discarded it would satisfy every row above — which \
             is the durability gap `pending_tab_states` exists to close: the commit \
             `window::build` drives before the window is presented would write a one-tab tree \
             over the whole saved session."
        );
        assert!(
            cases.iter().any(|case| case.want.is_none()),
            "no window_contribution row expects a REFUSAL, so a function that always answered \
             `Some` would satisfy every row above — and a window whose tabs have all refused \
             would commit `tabs: []`, which the next launch reads back as the empty state."
        );

        // --- The two JOINS: this composes those seams, it does not restate them ---
        let live: Vec<TabState> = SAVED_TAB_FILES
            .into_iter()
            .map(|file| tab_subtree(SAVED.to_string(), true, Some(leaf(file))))
            .collect();
        assert_eq!(
            window_contribution(
                WIDTH,
                HEIGHT,
                live.clone(),
                &[],
                false,
                Path::new(SAVED),
                true
            ),
            window_subtree(WIDTH, HEIGHT, live),
            "with nothing queued, a window's contribution must be EXACTLY `window_subtree`'s \
             answer for its live tabs. The geometry, the deliberate 0.0 position and the \
             commit-nothing rule all live in that one function, so a second copy of any of them \
             here is a second place to fix and a place for the two to disagree."
        );
        let queued = [tab_layout(0), tab_layout(1)];
        let seeded = window_contribution(
            WIDTH,
            HEIGHT,
            Vec::new(),
            &queued,
            false,
            Path::new(SAVED),
            true,
        );
        assert_eq!(
            seeded.map(|window| window.tabs),
            Some(pending_tab_states(&queued, false, Path::new(SAVED), true)),
            "the queued tabs' half must be EXACTLY `pending_tab_states`' answer for the same \
             queue. A cwd or a preference re-derived here would let a queued tab and a \
             materialized one of the same window disagree about where the project is, and \
             whichever the last wake wrote is what the next launch reads back."
        );
    }

    /// A window whose restore has not run yet contributes EVERY tab it will have: its
    /// live ones first, then its queued ones, in tab order.
    ///
    /// [`window_contribution_table`] pins that rule over the whole domain; this one
    /// states the CONSEQUENCE, so a failure names what is lost rather than an index,
    /// and carries the premise that makes the loss silent.
    ///
    /// FALSIFYING EDIT — drop the queued seeds, at either level: `pending_tab_states`
    /// returning `vec![]`, or [`window_contribution`] ignoring the queue it is handed.
    /// This window commits ONE tab where the user has three, and the premise assertion
    /// at the bottom is what says why that is silent rather than loud: the one-tab
    /// commit is a perfectly well-formed `WindowState`, indistinguishable at read time
    /// from an honest save of a one-tab session.
    #[test]
    fn a_window_whose_restore_has_not_run_yet_contributes_every_tab() {
        const WIDTH: i32 = 1200;
        const HEIGHT: i32 = 640;

        // Tab 0 has gathered; tabs 1 and 2 are queued layouts not yet materialized. Three distinct
        // files, so order is decidable from the values.
        let live = tab_subtree(SAVED.to_string(), true, tab_layout(0));
        let queued = [tab_layout(1), tab_layout(2)];

        let contributed = window_contribution(
            WIDTH,
            HEIGHT,
            vec![live.clone()],
            &queued,
            false,
            Path::new(SAVED),
            true,
        );
        let window = contributed.expect("one live tab and two queued must contribute a subtree");

        // Cwd and preference are per window, so live tab and seeds are indistinguishable in the file:
        // a session restores identically whether or not its tabs materialized before the commit.
        let want: Vec<TabState> = SAVED_TAB_FILES
            .into_iter()
            .map(|file| tab_subtree(SAVED.to_string(), true, Some(leaf(file))))
            .collect();

        assert_eq!(
            window.tabs.len(),
            want.len(),
            "a window with one live tab and {} queued layouts must contribute {} tabs, \
             not {}. ONE is the durability gap: between `window::build` returning and \
             the wake that materializes the queue, only tab 0 has a page, and \
             `shell::on_render_wake` commits in that gap without waiting — so the queued \
             tabs must be contributed as seeds or the window writes a one-tab tree over \
             the user's whole saved session. Behind a wedged or dead Neovim that wake is \
             the ONLY one there will ever be, because `tabs_ui::restore_press` answers \
             Wait forever, so the loss is permanent rather than sub-second.",
            queued.len(),
            want.len(),
            window.tabs.len()
        );
        assert_eq!(
            window.tabs, want,
            "the contributed tabs must be the live one FOLLOWED BY the queued ones, in \
             queue order — that is the order they materialize in and the order the next \
             launch replays. Seeds inserted anywhere else restore the user's tabs \
             rearranged, and since the next gather writes the new order back the swap \
             compounds on every run. A tab whose `layout` is `None` where a layout was \
             queued comes back as an empty pane, because a tab's files ride only inside \
             `TabState::layout`."
        );

        // The premise behind the falsifier: dropping queued tabs commits a smaller tree rather than
        // refusing, asserted through the same function.
        let live_only = window_contribution(
            WIDTH,
            HEIGHT,
            vec![live],
            &[],
            false,
            Path::new(SAVED),
            true,
        )
        .expect("a window contributing one tab must assemble into a subtree");
        assert_eq!(
            live_only.tabs.len(),
            1,
            "premise: the live tab ALONE assembles into a perfectly valid one-tab \
             window. That is why contributing nothing for the queued tabs is a SILENT \
             loss rather than a refused save — nothing downstream can tell this window \
             from an honest one-tab session, so the commit succeeds, the file is well \
             formed, and the next launch restores exactly what it was told."
        );
    }

    /// A restore that ABORTED part-way keeps speaking for the tabs it never created.
    ///
    /// `WolfCore::create_tabpage` answers `0` on an RPC error or a timeout — a Neovim
    /// still coming up under load at launch is all it takes — and
    /// `tab_view::TabsInner::create_one_restored_tab` then refuses that handle through
    /// `tabs_ui::switch_target` and stops the loop. The layouts it never reached have
    /// no page and never will get one, so an abort that DROPS them leaves nothing that
    /// can contribute them: no cache, because a cache is per live tab, and no live
    /// gather, because their panes do not exist. The next commit then writes this
    /// window back holding only the tabs created before the failure — the same
    /// durability gap [`pending_tab_states`] closes for the launch, narrowed to the
    /// abort.
    ///
    /// The rule is therefore that an abandoned queue is RETAINED as a seed source
    /// while no further creation is attempted. This test owns the retention half, over
    /// the seam the retained queue feeds; the "no further creation" half is
    /// `tabs_ui::tests::an_abandoned_restore_never_attempts_creation_again`. Re-seating
    /// the queue inside `materialize_restore` is glue no headless test reaches, and
    /// `tests/README.md` records the abort path as unproven end to end.
    ///
    /// FALSIFYING EDIT — the same one [`window_contribution_table`] names: contribute
    /// nothing for the queue. Here that is the abort's remainder, so the red says the
    /// abandoned tabs were dropped rather than that the launch's were.
    #[test]
    fn an_abandoned_restore_keeps_seeding_its_un_created_tabs() {
        const WIDTH: i32 = 1200;
        const HEIGHT: i32 = 640;

        // Four tabs restored: tab 0 and the first queued one gathered, then `create_tabpage` answered 0
        // for the last two. Four distinct files, so the missing ones are decidable.
        let live = vec![
            tab_subtree(SAVED.to_string(), true, Some(leaf(SAVED_FILE))),
            tab_subtree(SAVED.to_string(), true, Some(leaf(SAVED_TAB_FILES[0]))),
        ];
        let un_created = [tab_layout(1), tab_layout(2)];

        let contributed = window_contribution(
            WIDTH,
            HEIGHT,
            live.clone(),
            &un_created,
            false,
            Path::new(SAVED),
            true,
        );
        let window = contributed.expect("two live tabs must contribute a subtree");

        // The four files this window must come back with, in tab order.
        let files = [
            SAVED_FILE,
            SAVED_TAB_FILES[0],
            SAVED_TAB_FILES[1],
            SAVED_TAB_FILES[2],
        ];
        let want: Vec<TabState> = files
            .into_iter()
            .map(|file| tab_subtree(SAVED.to_string(), true, Some(leaf(file))))
            .collect();

        assert_eq!(
            window.tabs, want,
            "an abandoned restore must keep contributing the tabs it never created, AFTER \
             the ones it did: four tabs here, in saved order. THREE wrong answers are \
             distinguishable. TWO tabs is the remainder being dropped on abort — the \
             durability gap narrowed to the abort, since those layouts have no page and \
             never will, so nothing else can speak for them and the next commit writes the \
             window back without them; it needs no bug of ours to reach, because \
             `create_tabpage` answers 0 on an RPC error or a timeout, which is what a slow \
             Neovim at launch produces. FIVE tabs, carrying the created tab's layout twice, \
             is the abort retaining the WHOLE queue rather than its un-created remainder: \
             that tab then has both a page and a seed, so the save carries a duplicate, the \
             next launch opens it twice, and since that launch saves what it opened the \
             count doubles every run. A REORDERED four restores the user's tabs rearranged, \
             and the gather after that writes the new order back."
        );

        // The PREMISE that makes the retention rule worth stating: dropping the
        // remainder does not refuse the commit, it commits a smaller window.
        let dropped = window_contribution(WIDTH, HEIGHT, live, &[], false, Path::new(SAVED), true)
            .expect("two live tabs must contribute a subtree");
        assert_eq!(
            dropped.tabs.len(),
            2,
            "premise: the tabs that WERE created assemble into a perfectly valid two-tab \
             window. That is why an abort which forgets its remainder is a SILENT loss \
             rather than a refused save — nothing downstream can tell this window from an \
             honest two-tab session, so the commit succeeds, `state.json` is well formed, \
             and the next launch restores exactly what it was told."
        );
    }

    // --- snapshot_state: assembling the whole tree from the live windows ---

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
    /// [`window_subtree_writes_every_tab_in_order`], which asserts on `x` directly).
    ///
    /// Isolated: yes at the unit level.
    #[test]
    fn snapshot_state_carries_every_window_in_order() {
        const WIDTH_A: i32 = 1200;
        const HEIGHT_A: i32 = 640;
        const WIDTH_B: i32 = 900;
        const HEIGHT_B: i32 = 500;

        // Every identifying field differs (geometry, cwd, sidebar, layout), so window order is
        // decidable from the value.
        let first = window_subtree(
            WIDTH_A,
            HEIGHT_A,
            vec![tab_subtree(SAVED.to_string(), true, Some(leaf(SAVED_FILE)))],
        )
        .expect("a window contributing one tab must assemble into a subtree");
        let second = window_subtree(
            WIDTH_B,
            HEIGHT_B,
            vec![tab_subtree(
                SAVED_B.to_string(),
                false,
                Some(leaf(SAVED_B_FILE)),
            )],
        )
        .expect("a window contributing one tab must assemble into a subtree");

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

    // --- close_commit: what closing one window does to the persisted tree ---

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
                // The last window of a quit: testing "no other live windows" first answers KeepSubtree here
                // while every other shutting-down row stays green.
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

        // Non-vacuity: an always-KeepSubtree `close_commit` satisfies the rule above and resurrects every
        // closed window next launch.
        assert_eq!(
            close_commit(false, 1),
            CloseCommit::DropSubtree,
            "with another window still live, the closing window's subtree MUST be dropped — \
             otherwise the rule above holds vacuously and a window the user closed on purpose \
             comes back at the next launch"
        );
    }
}
