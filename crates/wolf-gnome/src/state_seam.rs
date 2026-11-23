//! The pure, GTK-free APPLICATION-STATE seam: what to restore at launch, and
//! what to persist on each wake.
//!
//! The launch plans and the write-side rules of `specs/application-state.md`, over
//! [`wolf_core::app_state`]'s model. GTK-free, IO-free, and it never reads `$HOME`
//! or the process cwd — every input is injected, so the whole restore/persist
//! rulebook is unit-testable headlessly with no temp dirs and no window, exactly
//! like [`crate::layout`] and [`crate::workspace`].
//!
//! ## What is deliberately NOT here
//!
//! - **Window POSITION.** Wayland gives a client neither its own position nor
//!   screen geometry, so there is no "intersects some screen" check.
//!   [`window_entry`] writes `x`/`y` as `None` unconditionally and
//!   [`restored_window_size`] returns SIZE only.
//! - **A per-window directory, sidebar or drawer share.** Each tab carries its own:
//!   [`restore_plans`] plans every tab at its own saved directory, sidebar and share,
//!   and [`tab_entry`] writes them per tab.

use std::path::{Path, PathBuf};

use wolf_core::app_state::{Frame, Sidebar, SidebarMode, Tab, TabContent, Window, WindowState};
use wolf_core::Liveness;

use crate::coordinator::WindowId;
use crate::drawer::DrawerShare;
use crate::pane_layout::LiveLayout;
use crate::search_panel::{SearchFields, SearchToggles};
use crate::sidebar::SidebarPage;
use crate::stacking::{StackingOrder, TabRecency};
use crate::tabs_ui::TabId;
use crate::workspace::{self, LaunchDecision};

/// The most tabs, and so Neovim processes, [`restore_plans`] will plan across
/// every window of one saved window state.
///
/// A restore POLICY cap, not a tree limit — the
/// [`crate::pane_layout::MAX_RESTORE_LEAVES`] precedent. What it bounds is not
/// memory but PROCESSES: every restored tab costs a whole captive Neovim and a
/// tokio runtime of its own, so a corrupt or hand-edited saved window state must not be
/// able to fork dozens of them before the first frame. The number sits far above
/// any session a user assembles by hand, so refusing is the pathological case
/// rather than a limit anyone meets.
pub const MAX_RESTORE_NEOVIMS: usize = 16;

/// The most tabs [`restore_plans`] will plan for ONE saved window.
///
/// The [`MAX_RESTORE_NEOVIMS`] precedent, bounding a different resource: one
/// window's LAUNCH COST. Every tab's `WolfCore::start` — a spawn plus its setup
/// calls — runs on the GTK main thread before that window is presented, followed
/// on its first wake by up to one blocking `rebuild_layout`. The number sits far
/// above any session a user assembles by hand, so refusing is the pathological
/// case rather than a limit anyone meets.
pub const MAX_RESTORE_TABS: usize = 8;

/// One restored TAB's plan.
#[derive(Debug, Clone, PartialEq)]
pub struct TabPlan {
    /// The working directory this tab's Neovim starts in.
    pub cwd: PathBuf,
    /// Whether the tab opens in the empty state.
    pub empty: bool,
    /// The tab's saved sidebar PREFERENCE, `true` without one.
    ///
    /// Independent of [`TabPlan::empty`]: the empty state overrides the preference
    /// at APPLY time, through [`sidebar_is_shown`], never at restore time. Folding
    /// the override in here would lose the preference on the first launch into an
    /// empty state, and the next save would then persist the fold.
    pub sidebar_visible: bool,
    /// The tab as saved, or `None` for a fresh tab: its seed ([`seeded_tab`]), and
    /// its rebuild when the tab is not empty.
    pub saved: Option<Tab>,
    /// The sidebar page the tab shows when selected, from [`restored_sidebar`].
    pub sidebar_page: SidebarPage,
    /// The search panel's field and toggles the tab records, from [`restored_sidebar`].
    pub search: SearchFields,
    /// The tab's drawer share: the saved one through [`DrawerShare::restored`], else a
    /// new one.
    pub drawer_share: DrawerShare,
}

/// Everything the launch path needs to decide about one window before any widget
/// is built.
#[derive(Debug, Clone, PartialEq)]
pub struct RestorePlan {
    /// The tabs to open, in saved order, NEVER empty. `window::build` opens every
    /// one, each with its own Neovim.
    pub tabs: Vec<TabPlan>,
    /// The index into `tabs` of the tab to select, always in range.
    pub active_tab: usize,
    /// The restored window size in pixels, or `None` for the unset sentinel.
    pub window_size: Option<(i32, i32)>,
    /// Whether the window restores minimized.
    pub minimized: bool,
}

/// Builds ONE launch plan per saved window, in saved order, which is stacking
/// order, frontmost first — the whole session's launch, rather than one window's.
///
/// Restore is POSITIONAL and never grouped: a saved state holding two windows on
/// one directory restores both, because the user opened both deliberately. A saved
/// window with no tabs has nothing to restore and is dropped.
///
/// Each tab is planned at its own saved directory through
/// [`workspace::launch_decision`], so a missing directory, `""`, `$HOME` and `/`
/// all plan an empty tab whose Neovim starts where that decision says. The tab
/// keeps its saved [`Tab`] either way, for the seed.
///
/// # A terminal launch shows its directory
///
/// When `process_cwd` is a usable project that no saved tab's `directory` holds,
/// one extra plan goes first, frontmost: one fresh folder tab there, at the
/// default size. When some saved tab holds it, the session restores exactly as
/// saved. Every saved window is recreated either way.
///
/// # This never returns an empty vec
///
/// No saved state at all and a `windows: []` state both yield exactly ONE plan:
/// the empty state, or the process cwd's folder. The application always opens a
/// window, so "there is nothing to restore" means "restore the empty state" and
/// never "start with no window at all" — which on GTK is a process that exits the
/// moment `activate` returns.
///
/// # The Neovim budget
///
/// The plans hold at most [`MAX_RESTORE_NEOVIMS`] tabs between them. Plans are
/// taken in order, each with at most `min(MAX_RESTORE_TABS, remaining)` of its
/// tabs, and the walk stops when the budget is spent. A window that only partly
/// fits is truncated to its first tabs rather than dropped, so the frontmost
/// windows are the ones that come back.
///
/// `is_dir` is injected so the whole plan is pure; production passes
/// `Path::is_dir`.
#[must_use]
pub fn restore_plans(
    saved: Option<&WindowState>,
    process_cwd: Option<&Path>,
    home: Option<&Path>,
    is_dir: impl Fn(&Path) -> bool,
) -> Vec<RestorePlan> {
    let saved_windows = saved.map_or(&[][..], |state| state.windows.as_slice());

    let mut candidates: Vec<RestorePlan> = Vec::new();
    let launch = workspace::launch_decision(process_cwd, None, home, &is_dir);
    if let LaunchDecision::Folder { cwd } = &launch {
        let held = saved_windows
            .iter()
            .flat_map(|window| &window.tabs)
            .any(|tab| Path::new(&tab.directory) == cwd.as_path());
        if !held {
            candidates.push(fresh_plan(launch.clone()));
        }
    }
    candidates.extend(
        saved_windows
            .iter()
            .filter(|window| !window.tabs.is_empty())
            .map(|window| plan_for_window(window, home, &is_dir)),
    );

    let mut plans: Vec<RestorePlan> = Vec::new();
    let mut remaining = MAX_RESTORE_NEOVIMS;
    for mut plan in candidates {
        if remaining == 0 {
            break;
        }
        // Never empties the plan: it holds at least one tab, and `remaining` is positive.
        plan.tabs.truncate(MAX_RESTORE_TABS.min(remaining));
        if plan.active_tab >= plan.tabs.len() {
            plan.active_tab = 0;
        }
        remaining -= plan.tabs.len();
        plans.push(plan);
    }

    if plans.is_empty() {
        // The floor, not a fallback: an empty vec here would leave the
        // application with no window to build.
        plans.push(fresh_plan(launch));
    }

    plans
}

/// One saved window's plan: each tab at its own directory, the saved active tab
/// (the first when out of range), the saved size and the minimized flag.
fn plan_for_window(
    window: &Window,
    home: Option<&Path>,
    is_dir: &impl Fn(&Path) -> bool,
) -> RestorePlan {
    let tabs: Vec<TabPlan> = window
        .tabs
        .iter()
        .map(|tab| {
            // `""` is an empty tab's directory, not a path.
            let directory = Some(Path::new(&tab.directory)).filter(|d| !d.as_os_str().is_empty());
            let (cwd, empty) = match workspace::launch_decision(None, directory, home, is_dir) {
                LaunchDecision::Folder { cwd } => (cwd, false),
                LaunchDecision::EmptyState { nvim_cwd } => (nvim_cwd, true),
            };
            let (sidebar_page, search) = restored_sidebar(&tab.content.sidebar);
            let drawer_share = tab
                .content
                .drawer
                .map_or_else(DrawerShare::new, |d| DrawerShare::restored(d.share));
            TabPlan {
                cwd,
                empty,
                sidebar_visible: tab.content.sidebar.visible,
                saved: Some(tab.clone()),
                sidebar_page,
                search,
                drawer_share,
            }
        })
        .collect();
    let active_tab = if window.active_tab < tabs.len() {
        window.active_tab
    } else {
        0
    };
    RestorePlan {
        tabs,
        active_tab,
        window_size: restored_window_size(window.frame.width, window.frame.height),
        minimized: window.minimized,
    }
}

/// A plan for one fresh tab, which `decision` places: a folder tab at its `cwd`,
/// or an empty tab whose Neovim starts at its `nvim_cwd`.
fn fresh_plan(decision: LaunchDecision) -> RestorePlan {
    let (cwd, empty) = match decision {
        LaunchDecision::Folder { cwd } => (cwd, false),
        LaunchDecision::EmptyState { nvim_cwd } => (nvim_cwd, true),
    };
    RestorePlan {
        tabs: vec![TabPlan {
            cwd,
            empty,
            sidebar_visible: true,
            saved: None,
            sidebar_page: SidebarPage::Files,
            search: SearchFields::default(),
            drawer_share: DrawerShare::new(),
        }],
        active_tab: 0,
        window_size: None,
        minimized: false,
    }
}

/// Whether the file-tree sidebar is SHOWN: the user wants it AND a project is open.
///
/// The ONE place that rule lives. `crate::shell::AppShell::apply_selected_tab` applies it
/// to the selected tab's preference and empty flag, so a user toggle cannot be silently
/// undone by leaving the empty state and the empty state cannot draw a file tree for a
/// project that is not open.
///
/// The empty state always WINS: a window with no project root has nothing to list,
/// so the tree would render whatever directory nvim happened to be handed
/// (`$HOME` on a desktop-launcher start) beside a status page saying no folder is
/// open.
#[must_use]
pub fn sidebar_is_shown(user_wants: bool, empty: bool) -> bool {
    !empty && user_wants
}

/// The saved window SIZE in pixels, or `None` when it should be ignored.
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

/// The working directory to persist for the tab.
///
/// The EMPTY state persists `""` ALWAYS — never the nvim fallback cwd. A
/// `LaunchDecision::EmptyState` hands nvim `$HOME` (or `/`) purely because a
/// process needs a working directory; persisting that would turn every
/// desktop-launcher start into "a project at `$HOME`" on the next launch.
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

/// Whether a live gather may replace a tab's cache at all.
///
/// `true` **iff** the captive nvim is [`Liveness::Alive`], at least one pane has a
/// RESOLVED grid, and nvim has reported its per-window buffer map at least once
/// (`reported_windows` is `Snapshot::win_buffers.len()`, empty until the first
/// `wolf_buffers` notification).
///
/// This gate is not tidiness — it prevents DATA LOSS. A snapshot taken while nvim
/// is dead, before the first pane binds, or before the first buffer report would
/// persist a lone empty pane (or a topology whose leaf paths are all empty) and
/// DESTROY the user's saved layout. Gated, a crash instead preserves the last
/// good save.
///
/// **The gate is read ONCE PER TAB, against that tab's own core**, and a refusal
/// means "contribute the cached tab", not "vanish from the tree" — a tab whose
/// panes have not bound yet keeps whatever it last persisted.
///
/// `reported_windows` is `Snapshot::win_buffers.len() + Snapshot::previews.len()`,
/// both filled by `runtime/nvim/init.lua` from `nvim_list_wins()`. A tab's second
/// refusal route is [`crate::pane_layout::resolve_leaves`], one layer down: a leaf
/// whose window is in neither map refuses that tab's gather, and its cache is
/// contributed instead. The two refusals compose — either one leaves the last good
/// tab standing.
///
/// The match over [`Liveness`] is TOTAL with no `_` arm, so a new variant is an
/// `E0004` here rather than a silent "not alive" — matching
/// [`crate::liveness_ui()`], [`crate::ui_action_for`] and `pane_view::apply_intents`.
#[must_use]
pub fn may_snapshot(liveness: Liveness, bound_panes: usize, reported_windows: usize) -> bool {
    match liveness {
        Liveness::Alive => bound_panes >= 1 && reported_windows >= 1,
        // A stalled nvim's snapshot is stale by definition and a dead one's is
        // empty; either would overwrite a good save with a worse one.
        Liveness::NotResponding | Liveness::Dead => false,
    }
}

/// Assembles ONE tab's persisted entry.
///
/// The one place a [`Tab`] is built on the write side. An empty tab, whose
/// `directory` is `""`, writes its whole `sidebar` and no root, zoom, focus or drawer,
/// since §2 allows a null root only there and nothing else names a pane. Every other
/// tab writes its root, a lone empty edit pane included, `live`'s zoom and focus, and
/// its drawer, open or closed, always with its share.
#[must_use]
pub fn tab_entry(directory: String, sidebar: Sidebar, live: LiveLayout) -> Tab {
    let content = if directory.is_empty() {
        TabContent {
            root: None,
            focused: None,
            zoomed: None,
            sidebar,
            drawer: None,
        }
    } else {
        TabContent {
            root: Some(live.root),
            focused: live.focused,
            zoomed: live.zoomed,
            sidebar,
            drawer: Some(live.drawer),
        }
    };
    Tab { directory, content }
}

/// What a restored tab contributes until its own first gather succeeds, or `None`
/// for a tab with no saved tab: `saved` as it was saved, with the LIVE `sidebar` in
/// place of its saved one.
///
/// A restored tab whose layout never binds a pane has no cache, so this is all it
/// ever contributes. Seeding it is what stops a commit made before the tab has a
/// pane erasing it, and a tab restored empty because its directory vanished keeps
/// contributing its saved tab, so that directory's project state is never touched
/// by absence. It is the tab as SAVED, never as `wolf_core::app_state::restored_tab`
/// rewrote it, so a launch that finds a file missing does not write the loss back.
/// A fresh tab has no seed and contributes nothing until its gate opens.
#[must_use]
pub fn seeded_tab(saved: Option<&Tab>, sidebar: Sidebar) -> Option<Tab> {
    let mut tab = saved?.clone();
    tab.content.sidebar = sidebar;
    Some(tab)
}

/// A tab's saved sidebar: `visible`, the `page` as its mode, and the panel's toggles,
/// with the query only when it is not `""`.
#[must_use]
pub fn sidebar_entry(visible: bool, page: SidebarPage, search: &SearchFields) -> Sidebar {
    let mode = match page {
        SidebarPage::Files => SidebarMode::Tree,
        SidebarPage::Search => SidebarMode::Search,
    };
    Sidebar {
        visible,
        mode: Some(mode),
        query: Some(search.query.clone()).filter(|q| !q.is_empty()),
        case_sensitive: Some(search.toggles.case_sensitive),
        use_regex: Some(search.toggles.use_regex),
        all_files: Some(search.toggles.all_files),
    }
}

/// The page and panel fields a saved sidebar restores: the Search page for a saved
/// search mode and Files otherwise, a missing query as `""`, and a missing toggle at
/// [`SearchToggles::INITIAL`]'s value.
#[must_use]
pub fn restored_sidebar(sidebar: &Sidebar) -> (SidebarPage, SearchFields) {
    let page = match sidebar.mode {
        Some(SidebarMode::Search) => SidebarPage::Search,
        Some(SidebarMode::Tree) | None => SidebarPage::Files,
    };
    let initial = SearchToggles::INITIAL;
    let fields = SearchFields {
        query: sidebar.query.clone().unwrap_or_default(),
        toggles: SearchToggles {
            case_sensitive: sidebar.case_sensitive.unwrap_or(initial.case_sensitive),
            use_regex: sidebar.use_regex.unwrap_or(initial.use_regex),
            all_files: sidebar.all_files.unwrap_or(initial.all_files),
        },
    };
    (page, fields)
}

/// Whether a launch runs the selected tab's restored search: only on the Search page,
/// in a tab with a project, and for a query that is not blank. An empty tab's search
/// would be rooted at its Neovim's home directory, which
/// [`crate::search_panel::show_decision`] refuses.
#[must_use]
pub fn restore_runs_search(page: SidebarPage, empty: bool, query: &str) -> bool {
    page == SidebarPage::Search && !empty && !query.trim().is_empty()
}

/// Whether a viewport capture may issue its BLOCKING `WolfCore::window_cursor`: only
/// against an [`Liveness::Alive`] Neovim, so a capture never stalls the main thread
/// for the RPC timeout on a wedged or dead one.
///
/// TOTAL, with no `_` arm, as [`may_snapshot`] is.
#[must_use]
pub fn may_capture_cursor(liveness: Option<Liveness>) -> bool {
    match liveness {
        Some(Liveness::Alive) => true,
        Some(Liveness::NotResponding | Liveness::Dead) | None => false,
    }
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
/// The sort is STABLE. `adw::TabView` hands out one position per page, so a tie is
/// not an input production can produce; a stable sort is what gives the impossible
/// case a stated answer — the pairs' given relative order — instead of one that
/// depends on how they happened to be collected.
#[must_use]
pub fn ordered_tab_states<T>(positioned: Vec<(i32, T)>) -> Vec<T> {
    let mut positioned = positioned;
    positioned.sort_by_key(|(position, _)| *position);
    positioned.into_iter().map(|(_, tab)| tab).collect()
}

/// Assembles ONE window's entry from its size and its contributing tabs, in page
/// order with their ids, plus its recency as indices into those tabs — or `None`
/// when no tab contributed.
///
/// `x`/`y` are `None` unconditionally: window POSITION is never known on Wayland
/// (see the module docs). `active_tab` is the `selected` tab's index among the
/// contributing tabs, or, when it contributed nothing, the most recently selected
/// contributing tab's. `minimized` is left `false`; [`window_state`] sets it.
///
/// # `None` means "commit nothing", exactly as it does for [`window_state`]
///
/// An empty `tabs` vec is not a window with no tabs: every tab's gate can be shut
/// at once while its panes are still binding. A window with no tabs is not one §2
/// allows, and committing it would replace the window's last known tabs with none.
/// The window drops out of the contribution set instead.
#[must_use]
pub fn window_entry(
    width: i32,
    height: i32,
    tabs: Vec<(TabId, Tab)>,
    selected: Option<TabId>,
    recency: &TabRecency,
) -> Option<(Window, Vec<usize>)> {
    if tabs.is_empty() {
        return None;
    }
    let ids: Vec<TabId> = tabs.iter().map(|(id, _)| *id).collect();
    let recent = recency.indices(&ids);
    let active_tab = selected
        .and_then(|selected| ids.iter().position(|&id| id == selected))
        .or_else(|| recent.first().copied())
        .unwrap_or(0);
    let window = Window {
        frame: Frame {
            x: None,
            y: None,
            width: f64::from(width),
            height: f64::from(height),
        },
        active_tab,
        tabs: tabs.into_iter().map(|(_, tab)| tab).collect(),
        minimized: false,
    };
    Some((window, recent))
}

/// Assembles the whole window state from the contributing windows, in stacking
/// order, each marked minimized as `stacking` says, with each window's recency in
/// the same order — or `None` when there are none.
///
/// # `None` means "commit nothing", never "commit nothing useful"
///
/// An empty contribution set is not an empty session: it is a launch whose only
/// core died before its first pane bound, or a moment when every window's
/// [`may_snapshot`] gate is shut. Answering `Some` of an empty state would replace
/// the user's whole saved session with no window at all. `None` means no submit,
/// so the store keeps what it holds.
#[must_use]
pub fn window_state(
    windows: Vec<(WindowId, Window, Vec<usize>)>,
    stacking: &StackingOrder,
) -> Option<(WindowState, Vec<Vec<usize>>)> {
    if windows.is_empty() {
        return None;
    }
    let ids: Vec<WindowId> = windows.iter().map(|(id, ..)| *id).collect();
    let mut slots: Vec<Option<(WindowId, Window, Vec<usize>)>> =
        windows.into_iter().map(Some).collect();
    let mut state = WindowState::default();
    let mut recency = Vec::new();
    for id in stacking.order(&ids) {
        let slot = slots
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|(w, ..)| *w == id));
        if let Some((_, mut window, recent)) = slot.and_then(Option::take) {
            window.minimized = stacking.is_minimized(id);
            state.windows.push(window);
            recency.push(recent);
        }
    }
    Some((state, recency))
}

/// What closing one window does to the persisted tree.
///
/// Returned by [`close_commit`], which is the whole rule; this enum only names its
/// three answers so a caller cannot express a fourth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseCommit {
    /// Commit nothing at all: the tree has already been written by the quit path.
    Skip,
    /// Commit a tree that still CONTAINS the closing window.
    KeepSubtree,
    /// Commit a tree with the closing window REMOVED.
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

/// The restore's present schedule: for each opened window, in build order
/// (backmost first) with whether it is minimized, the index of the window whose
/// first paint it waits for, or `None` to be presented at once.
///
/// A window that is not minimized waits for the nearest window built before it
/// that is not minimized, and the first such window is presented at once. Windows
/// presented in one burst map in no order GTK promises, and a compositor that
/// focuses each new toplevel turns every map into a focus, so the map order is the
/// stacking order the next save writes. A window paints its first frame on the one
/// Wayland connection before the next is presented, so the chain maps them backmost
/// first.
///
/// Every minimized window is presented at once, and nothing waits for one. The
/// stacking order puts it behind every window that is not minimized whatever its map
/// order, and whether a window minimized before it maps ever paints is unknown, so
/// waiting on one could keep every window in front of it unmapped. Presented ahead
/// of the chain's frontmost, it never takes the focus from that window.
#[must_use]
pub fn present_after(minimized: &[bool]) -> Vec<Option<usize>> {
    let mut behind = None;
    minimized
        .iter()
        .enumerate()
        .map(|(index, &minimized)| {
            if minimized {
                return None;
            }
            behind.replace(index)
        })
        .collect()
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

    use wolf_core::app_state::{Node, Pane};

    // --- Paths (absolute and distinct, so every `==` is unambiguous) ------------
    /// The process working directory in the terminal-launch rows.
    const PROJECT: &str = "/projects/wolf";
    /// A saved tab's working directory.
    const SAVED: &str = "/projects/saved";
    /// The user's home directory (the desktop-launcher shape).
    const HOME: &str = "/home/tester";
    /// The three files [`positioned_tab`] names, one per tab, distinct so an
    /// ordering mistake is visible in the values alone.
    const SAVED_TAB_FILES: [&str; 3] = [
        "/projects/saved/first.md",
        "/projects/saved/second.md",
        "/projects/saved/third.md",
    ];

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

    /// The tab ONE live window contributes, identified by its lone pane's path alone.
    fn positioned_tab(index: usize) -> Tab {
        let root = Node::Pane(Pane::Edit {
            path: SAVED_TAB_FILES[index].to_owned(),
            cursor: None,
            top_line: None,
        });
        let live = LiveLayout {
            root,
            zoomed: None,
            focused: None,
            drawer: wolf_core::app_state::Drawer {
                open: false,
                share: DrawerShare::new().get(),
            },
        };
        tab_entry(SAVED.to_string(), Sidebar::default(), live)
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
                 place the rule lives: `AppShell::apply_selected_tab` applies it on every \
                 toggle and every selection change, so a wrong answer here means the next \
                 selection silently undoes whatever the user just toggled.",
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

    // --- ordered_tab_states: the persisted order is page order, the user's arrangement ---

    /// One [`ordered_tab_states`] row: the `(page position, tab)` pairs as they were
    /// COLLECTED, and the tabs as they must be CONTRIBUTED.
    ///
    /// Both sides are DATA. A `want` derived by sorting the input inside the test would
    /// agree with whatever the seam did, which is exactly the failure this table exists
    /// to catch.
    struct OrderCase {
        desc: &'static str,
        positioned: Vec<(i32, Tab)>,
        want: Vec<Tab>,
    }

    /// A window's tabs are persisted in PAGE order — the order they sit in the tab bar
    /// — whatever order they were collected in.
    ///
    /// `tab_view::TabsHandle::entries` is INSERTION order: a `Vec` that
    /// `TabsInner::open_tab` pushes to, with nothing reconciling it against
    /// `adw::TabView`'s page positions. Dragging a tab along the bar reorders the
    /// pages — `AdwTabBar` does that with no property that turns it off, and nothing
    /// in this tree disables it — so the two orders diverge the first time a user
    /// rearranges their tabs. `window::WindowHandle::cached_window` therefore pairs
    /// each cached tab with `view.page_position(page)` and runs the pairs through here
    /// before handing the result to [`window_entry`]. This is the ONE place that
    /// reconciliation happens.
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
    /// FALSIFYING EDIT — strip the positions and return the pairs' tabs in the
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
                // `window_entry` reads as commit-nothing.
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
                 `window_entry` writes into the saved window state as the window's tabs, and \
                 `restore_plans` walks a saved window's tabs POSITIONALLY, so the order here is \
                 literally the order the next launch replays. THREE failures are \
                 distinguishable. The tabs in the order the pairs ARRIVED is insertion order — \
                 the order the pages were created — so a user who drags a tab along the bar has \
                 their arrangement silently discarded at the next launch, and because the gather \
                 after that writes the collected order back the discard is permanent rather than \
                 one run's. Persistence follows what the user SEES. A SHORT vec \
                 drops tabs outright, and a tab's files ride only inside its root. A \
                 LONGER one duplicates a tab, which the next launch opens twice and then saves \
                 twice, so the count doubles every run.",
                case.positioned, case.want, case.desc
            );
        }

        // --- Non-vacuity, in two directions -----------------------------------
        assert!(
            cases.iter().any(|case| {
                let given: Vec<&Tab> = case.positioned.iter().map(|(_, tab)| tab).collect();
                let want: Vec<&Tab> = case.want.iter().collect();
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
             vanished here hands `window_entry` an empty vec, so the whole window drops out of \
             the committed tree and the next launch opens one fewer window than the user had."
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
    /// saved window state would come back holding nothing and the readback would answer
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
