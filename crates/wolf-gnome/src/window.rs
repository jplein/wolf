//! ONE `wolf-gnome` window and everything scoped to it: an
//! [`adw::ApplicationWindow`] over its tabs, each tab running its own captive
//! [`WolfCore`](wolf_core::WolfCore) ([`crate::tab_view`]), with the window's own overlays, its own
//! `win.`-namespace actions and its own contribution to the persisted state tree.
//!
//! [`build`] is the whole window. [`crate::coordinator`] is the process-scoped
//! layer above it and owns the six things a second copy of would be wrong — the
//! state store, the config, the cell metrics, the `app.`-namespace actions, the
//! session's launch plans and the test-only readiness probe — so everything here
//! is per-window by construction.
//!
//! - **Output / wake:** each tab's [`WolfCore`](wolf_core::WolfCore) runs Neovim on its own tokio
//!   runtime and signals that tab's own wake channel when a new frame is ready
//!   ([`crate::tab_view`]). The channel's drain runs
//!   [`crate::shell::AppShell::on_tab_wake`] on the GTK main thread with that tab.
//!   A wake carries no token: the channel it arrives on names the tab. The order
//!   inside the drain, and why liveness comes first, are in
//!   `notes/wolf-gnome-decisions.md#threading-and-the-wake`.
//! - **Window-level widgets:** the header bar, the sidebar, the three transient
//!   overlays and the crash overlay are per window. Each acts on the SELECTED
//!   tab's core, resolved at call time through
//!   [`crate::tab_view::TabsHandle::core_source`] or
//!   [`crate::tab_view::TabsHandle::selected_core`], so a switch retargets every
//!   one of them without a setter.
//! - **Application state:** the [`RestorePlan`] arrives already built — the window
//!   size, the active tab, whether it is minimized, and one
//!   [`crate::state_seam::TabPlan`] per saved tab with its own directory, empty
//!   state and sidebar preference — because a launch restores a SESSION and the
//!   plans are made once, above. [`build`] opens every planned tab with its own
//!   Neovim before it builds any other widget; each tab's layout is realized on that
//!   tab's own first wake with a window grid. The blocking calls that costs, and the
//!   caps that bound it, are in `notes/wolf-gnome/crash-and-stall-recovery.md`.
//!
//!   Writing back is [`crate::persistence`], one instance per TAB, each gathering
//!   from its own tab's Neovim: every tab is re-gathered on every commit and as the
//!   FIRST thing [`wire_close`] does, and [`WindowHandle::cached_window`] assembles
//!   them into this window's entry in page order, with the tab recency. A window
//!   gaining focus tells the coordinator, which keeps the stacking order.
//! - **Paint:** per-pane painting and the mode-aware cursor live in
//!   [`crate::pane_view`] (which uses the pure [`crate::paint`]/[`crate::cursor`]
//!   seams); this module installs no draw handler of its own.
//! - **Input:** a window-level [`gtk::EventControllerKey`] offers each key to this
//!   window's own [`gtk::IMMulticontext`] and routes what the context declines
//!   through the pure [`crate::gdk_key`] seam into the SELECTED tab's
//!   [`WolfCore::send_input`](wolf_core::WolfCore::send_input), while what it CLAIMS arrives as a `commit` string and
//!   reaches the same call through [`mod@crate::im_ui`] — which owns that precedence
//!   and the commit rulebook. While the selected tab's active pane is a terminal
//!   pane, the same key goes to its session through `WolfCore::terminal_key`
//!   instead, and a commit through [`crate::terminal::input::commit_route`].
//!   The controller has no propagation phase (i.e. Bubble), so it runs only after
//!   [`crate::shell`]'s CAPTURE-phase chord controller has had its say — which is
//!   exactly the guarantee that a bound chord is not also forwarded to Neovim, nor
//!   offered to the input method. **POINTER input is not window-level at all**: it
//!   is PER-PANE and lives entirely in [`crate::pane_view`].
//! - **Commands / menu:** [`crate::shell`] installs one `gio::SimpleAction` per
//!   implemented `win.` action of [`crate::actions::ACTION_TABLE`], builds the primary
//!   menu from the same data, matches the accelerators, and drains the selected
//!   tab's [`WolfCore::drain_commands`](wolf_core::WolfCore::drain_commands) on its wakes. The `app.` rows are the
//!   coordinator's, installed once for the process.
//! - **Lifecycle:** each tab's [`WolfCore`](wolf_core::WolfCore) lives in an `Rc<RefCell<Option<_>>>` on
//!   the main thread; closing the window tells the coordinator first, then shuts
//!   every tab's core down ([`crate::tab_view::TabsHandle::shutdown_all`]).
//!
//! When neither the process cwd nor the restored cwd names a usable project,
//! [`crate::workspace::launch_decision`] puts the window into an EMPTY STATE: the
//! sidebar is hidden, no `FileSearch` starts, and an "Open Folder…" status page
//! covers each tab's editor.
//!
//! The shared `wolf-config.yaml`'s `appearance` is resolved against libadwaita's
//! own light/dark reading and applied in both directions the resolved value
//! reaches — `background=dark|light` into each tab's captive Neovim and the GTK
//! chrome's colour scheme — by [`crate::appearance::apply_at_launch`], called once
//! per tab as soon as its core exists.

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::{gdk, glib};
use wolf_core::app_state::{Tab, Window};
use wolf_core::{InputEvent, TerminalId, WolfConfig};

use crate::buffer_picker::{self, BufferPickerHandle};
use crate::coordinator::{self, Coordinator, CoordinatorInner, WindowId};
use crate::crash_overlay;
use crate::frame_metrics;
use crate::im_ui;
use crate::open_folder;
use crate::palette::{self, PaletteHandle};
use crate::picker::{self, PickerHandle};
use crate::ready_seam;
use crate::search_panel::{self, SearchPanelHandle};
use crate::shell::{self, AppShell, ShellParts, HISTORY_BACK_ACTION, HISTORY_FORWARD_ACTION};
use crate::sidebar;
use crate::state_seam::{self, RestorePlan, TabPlan};
use crate::tab_view::{self, CoreCell, TabHandle, TabParts, TabsHandle, TabsParts};
use crate::tabs_ui::TabId;
use crate::terminal::input as terminal_input;
use crate::title::WINDOW_TITLE_FALLBACK;
use crate::titlebar_background::TitlebarStyle;
use crate::tree_view::{self, TreeHandle};
use crate::workspace;
use crate::CellMetrics;

/// Default window size in pixels. Widened from the editor-only default so that,
/// after the tree sidebar takes [`TREE_PANE_WIDTH`] on the left, the editor pane
/// keeps roughly its previous width (important for the OCR smoke tests).
const WINDOW_WIDTH: i32 = 1200;
/// Default window height in pixels (see [`WINDOW_WIDTH`]).
const WINDOW_HEIGHT: i32 = 640;

/// Default width (px) of the file-tree sidebar pane (the `gtk::Paned` divider
/// position). Collapsed to `0` in the empty state by
/// [`crate::shell::AppShell::apply_selected_tab`].
pub(crate) const TREE_PANE_WIDTH: i32 = 250;

/// Icon of the header bar's Back button. From `adwaita-icon-theme`, which the flake
/// supplies and `XDG_DATA_DIRS` exposes; a missing icon renders as `image-missing` and
/// breaks no assertion.
const HISTORY_BACK_ICON: &str = "go-previous-symbolic";

/// Icon of the header bar's Forward button (see [`HISTORY_BACK_ICON`]).
const HISTORY_FORWARD_ICON: &str = "go-next-symbolic";

/// Everything [`build`] needs, as one struct rather than six positional
/// parameters (`clippy::too_many_arguments` bites at seven), following the
/// [`ShellParts`] / [`crate::persistence::StateParts`] / [`CloseParts`] precedent.
pub struct WindowParts<'a> {
    /// The application this window belongs to. The plain [`gtk::Application`]
    /// rather than [`adw::Application`], because that is what
    /// `GtkWindowExt::application()` answers on the paths that ask for a second
    /// window, and nothing here needs the libadwaita subclass.
    pub app: &'a gtk::Application,
    /// The process-scoped coordinator. Held by the window's closures only as a
    /// `Weak`; see [`mod@crate::coordinator`] on why that direction is forced.
    pub coordinator: &'a Coordinator,
    /// This window's identity, minted by the coordinator.
    pub id: WindowId,
    /// The launch plan: one [`crate::state_seam::TabPlan`] per tab to restore, the
    /// active tab, the saved window size and whether the window is minimized.
    pub plan: &'a RestorePlan,
    /// The user configuration, read once for the process.
    pub config: &'a WolfConfig,
    /// The cell metrics, measured once for the process and shared by `Rc` with
    /// this window's draw and resize closures.
    pub metrics: &'a Rc<CellMetrics>,
    /// Whether [`build`] presents the window. `false` leaves it built and registered
    /// but unmapped until [`WindowHandle::show_after`].
    pub present: bool,
}

/// What runs once a window has painted its first frame; see
/// [`WindowHandle::show_after`].
type PaintedCallback = Box<dyn FnOnce()>;

/// One window's shared state: everything the coordinator needs to ask a window
/// for — its state contribution and its readiness for input — and the shell.
struct WindowInner {
    /// This window's identity.
    id: WindowId,
    /// The toplevel.
    window: adw::ApplicationWindow,
    /// This window's tabs. The persistence glue is per TAB and lives inside them;
    /// what this window contributes to the saved window state is EVERY tab's state in
    /// page order — see [`WindowHandle::cached_window`].
    tabs: TabsHandle,
    /// The shell, owned here for exactly the window's lifetime: every closure and
    /// tab drain that reaches it holds a `Weak`, and the coordinator drops this
    /// handle when the window closes.
    shell: AppShell,
    /// The ordinal of this window's first frame-clock `after-paint`, set only while
    /// the readiness probe is pending; see [`crate::ready_seam`].
    first_paint: Cell<Option<u32>>,
    /// Presents the window, held while [`WindowParts::present`] deferred it.
    show: RefCell<Option<PaintedCallback>>,
    /// What waits for this window's first painted frame, or `None` once it has
    /// painted or been unrealized.
    painted: RefCell<Option<Vec<PaintedCallback>>>,
}

/// A cloneable handle to one live window, held STRONGLY by the coordinator.
#[derive(Clone)]
pub struct WindowHandle {
    inner: Rc<WindowInner>,
}

impl WindowHandle {
    /// This window's identity.
    #[must_use]
    pub fn id(&self) -> WindowId {
        self.inner.id
    }

    /// Presents this window. Best-effort on Wayland: a compositor is free to
    /// ignore it, and `cage` does.
    pub fn present(&self) {
        self.inner.window.present();
    }

    /// Presents this window, which [`WindowParts::present`] deferred, once each of
    /// `others` has painted its first frame or been unrealized, so this window maps
    /// after each of them. Windows built in one burst map in no order GTK promises.
    pub fn show_after(&self, others: &[WindowHandle]) {
        let remaining = Rc::new(Cell::new(others.len()));
        if others.is_empty() {
            self.run_show();
            return;
        }
        for other in others {
            let weak = Rc::downgrade(&self.inner);
            let remaining = Rc::clone(&remaining);
            other.when_painted(Box::new(move || {
                remaining.set(remaining.get().saturating_sub(1));
                if remaining.get() == 0 {
                    if let Some(inner) = weak.upgrade() {
                        WindowHandle { inner }.run_show();
                    }
                }
            }));
        }
    }

    /// Drops this window's deferred present, so it never runs, and realizes the
    /// window when it is not, mapping nothing. A window already presented is left
    /// alone.
    ///
    /// `close()` returns at once on an unrealized window, so without this a window
    /// still waiting at a quit would keep its Neovims and be presented later.
    pub fn cancel_present(&self) {
        let show = self.inner.show.borrow_mut().take();
        if show.is_some() && !self.inner.window.is_realized() {
            gtk::prelude::WidgetExt::realize(&self.inner.window);
        }
    }

    /// Runs the deferred present, once.
    fn run_show(&self) {
        let show = self.inner.show.borrow_mut().take();
        if let Some(show) = show {
            show();
        }
    }

    /// Runs `f` once this window has painted its first frame or been unrealized, or
    /// now when that has already happened.
    fn when_painted(&self, f: PaintedCallback) {
        let mut pending = self.inner.painted.borrow_mut();
        match pending.as_mut() {
            Some(pending) => pending.push(f),
            None => {
                drop(pending);
                f();
            }
        }
    }

    /// Runs everything [`WindowHandle::when_painted`] queued, once.
    fn painted(&self) {
        let pending = self.inner.painted.borrow_mut().take();
        for f in pending.into_iter().flatten() {
            f();
        }
    }

    /// Calls [`WindowHandle::painted`] on this window's first `after-paint`, or on its
    /// unrealize when it never paints. Called right after `present()`.
    fn watch_first_paint(&self) {
        let weak = Rc::downgrade(&self.inner);
        self.inner.window.connect_unrealize(move |_window| {
            if let Some(inner) = weak.upgrade() {
                WindowHandle { inner }.painted();
            }
        });
        let Some(clock) = self.inner.window.frame_clock() else {
            self.painted();
            return;
        };
        let weak = Rc::downgrade(&self.inner);
        let handler: Rc<RefCell<Option<glib::SignalHandlerId>>> = Rc::new(RefCell::new(None));
        let id = {
            let handler = Rc::clone(&handler);
            clock.connect_after_paint(move |clock| {
                if let Some(id) = handler.borrow_mut().take() {
                    clock.disconnect(id);
                }
                if let Some(inner) = weak.upgrade() {
                    WindowHandle { inner }.painted();
                }
            })
        };
        *handler.borrow_mut() = Some(id);
    }

    /// Re-gathers EVERY tab's persisted state; see
    /// [`crate::persistence::StateHandle::refresh_tab`].
    ///
    /// Every tab, not the selected one, and it costs no RPC: each tab's leaves are
    /// resolved from its own Neovim's snapshot ([`crate::pane_layout::resolve_leaves`]).
    /// The selected tab first records the window's sidebar page and search fields
    /// ([`AppShell::record_selected_sidebar`]), so its saved sidebar is the one on screen.
    pub fn refresh_subtree(&self) {
        self.inner.shell.record_selected_sidebar();
        for tab in self.inner.tabs.entries() {
            tab.refresh_tab();
        }
    }

    /// Captures every tab's pane cursors and preview scrolls, for a window closing and
    /// for quit ([`crate::tab_view::TabHandle::capture_viewports`]).
    pub fn capture_viewports(&self) {
        for tab in self.inner.tabs.entries() {
            tab.capture_viewports();
        }
    }

    /// This window's contribution to the saved window state: every tab that has
    /// something to contribute, in PAGE order, under this window's size, with its
    /// tab recency as indices into those tabs — or `None` when no tab contributed at
    /// all ([`crate::state_seam::window_entry`]).
    ///
    /// **PAGE order, not the order the tabs were opened in.**
    /// [`crate::tab_view::TabsHandle::entries`] is insertion order and libadwaita
    /// lets the user rearrange the bar, so each cache is paired with its
    /// [`crate::tab_view::TabHandle::page_position`] and sorted by
    /// [`crate::state_seam::ordered_tab_states`]. That seam carries the reason.
    #[must_use]
    pub fn cached_window(&self) -> Option<(Window, Vec<usize>)> {
        let positioned: Vec<(i32, (TabId, Tab))> = self
            .inner
            .tabs
            .entries()
            .iter()
            .filter_map(|tab| Some((tab.page_position(), (tab.id(), tab.cached_tab()?))))
            .collect();
        // Position is never persisted on Wayland, so this is the whole geometry.
        let (width, height) = self.inner.window.default_size();
        let selected = self.inner.tabs.selected().map(|tab| tab.id());
        state_seam::window_entry(
            width,
            height,
            state_seam::ordered_tab_states(positioned),
            selected,
            &self.inner.tabs.recency(),
        )
    }

    /// Where this window IS, for another window's palette: the directory its active
    /// tab contributes to the saved window state.
    ///
    /// Read out of the cached entry rather than recomputed, so the palette and
    /// the persisted tree cannot disagree about a window's directory — the rule is
    /// [`crate::state_seam::snapshot_cwd`]'s, applied once, in
    /// [`crate::persistence::StateHandle::refresh_tab`]. An empty-state window
    /// yields `""`, which `wolf_core::rank_palette` renders as "Untitled"; so does
    /// a window whose gate has not opened yet, which has by definition persisted
    /// nothing to be offered under.
    #[must_use]
    pub fn display_cwd(&self) -> String {
        self.cached_window()
            .and_then(|(window, _)| {
                window
                    .tabs
                    .get(window.active_tab)
                    .map(|tab| tab.directory.clone())
            })
            .unwrap_or_default()
    }

    /// This window as Open Folder's step reads it: its tabs in page order, each with
    /// its live directory ([`TabHandle::live_directory`]), the selected tab and the
    /// tab recency.
    #[must_use]
    pub fn live_window(&self) -> open_folder::LiveWindow {
        let mut entries: Vec<(i32, TabHandle)> = self
            .inner
            .tabs
            .entries()
            .into_iter()
            .map(|tab| (tab.page_position(), tab))
            .collect();
        entries.sort_by_key(|(position, _)| *position);
        open_folder::LiveWindow {
            id: self.inner.id,
            tabs: entries
                .iter()
                .map(|(_, tab)| (tab.id(), tab.live_directory()))
                .collect(),
            selected: self.inner.tabs.selected().map(|tab| tab.id()),
            recency: self.inner.tabs.recency(),
        }
    }

    /// Whether this window's selected tab is empty: the tab Open Folder fills.
    #[must_use]
    pub fn selected_is_empty(&self) -> bool {
        self.inner.tabs.selected().is_some_and(|tab| tab.is_empty())
    }

    /// The selected tab's core, or `None` before the first page exists.
    #[must_use]
    pub fn selected_core(&self) -> Option<CoreCell> {
        self.inner.tabs.selected_core()
    }

    /// Open Folder's focus: selects `tab` and presents this window, best-effort on
    /// Wayland ([`WindowHandle::present`]). Answers the tab's core, `None` when no
    /// tab of this window is `tab`.
    pub fn focus_tab(&self, tab: TabId) -> Option<CoreCell> {
        let entry = self
            .inner
            .tabs
            .entries()
            .into_iter()
            .find(|entry| entry.id() == tab)?;
        self.inner.tabs.select(&entry);
        self.present();
        Some(entry.core().clone())
    }

    /// Open Folder's fill: replaces the selected tab with the one tab `plan` plans, at
    /// its page position ([`TabsHandle::replace`]), then gives the window the plan's
    /// size when the replaced tab was its only one ([`open_folder::fill_size`]), and the
    /// search panel the new tab's recorded fields. Answers the new tab's core, `None`,
    /// with the window untouched, when it fails to start.
    pub fn fill_selected_tab(&self, plan: &RestorePlan) -> Option<CoreCell> {
        let old = self.inner.tabs.selected()?;
        let tab = plan.tabs.first()?;
        let window_tabs = self.inner.tabs.len();
        let filled = self.inner.tabs.replace(&old, tab_parts(tab))?;
        if let Some((width, height)) = open_folder::fill_size(plan, window_tabs) {
            self.inner.window.set_default_size(width, height);
        }
        self.inner.shell.restore_selected_search();
        Some(filled.core().clone())
    }

    /// This window's reading for the test-only readiness predicate
    /// ([`crate::ready_seam::launch_readiness`]).
    ///
    /// Each value is read in its own statement, so no borrow of a core or the tab
    /// layer is held across another. The liveness and the panes are the SELECTED
    /// tab's, the one typed keys reach.
    #[must_use]
    pub fn readiness(&self) -> ready_seam::WindowReadiness {
        let selected = self.inner.tabs.selected();
        let liveness = selected.as_ref().and_then(|tab| tab.liveness());
        let panes = selected.as_ref().map(|tab| tab.panes().readiness());
        let first_paint = self.inner.first_paint.get();
        let has_keyboard = seat_has_keyboard(&self.inner.window);
        let has_focus_widget = GtkWindowExt::focus(&self.inner.window).is_some();
        ready_seam::WindowReadiness {
            liveness,
            panes,
            first_paint,
            has_keyboard,
            has_focus_widget,
        }
    }
}

/// Builds one application window and opens its planned tabs, each with a freshly
/// started [`WolfCore`](wolf_core::WolfCore) of its own.
///
/// `None` — after one message — when no planned tab's core starts; the
/// coordinator then simply has one window fewer, and a launch in which that was
/// the only window has no window at all and exits. A tab whose core fails is
/// skipped, and the window opens with the others.
///
/// Registers the window with the coordinator BEFORE any tab's first drain, so
/// nothing the drain reaches can act on a coordinator that does not yet know the
/// window exists.
#[must_use]
pub fn build(parts: WindowParts<'_>) -> Option<WindowHandle> {
    let WindowParts {
        app,
        coordinator,
        id,
        plan,
        config,
        metrics,
        present,
    } = parts;

    // Read once: only a window built while the readiness probe is pending wires its triggers; others
    // pay one bool test per wake.
    let ready_armed = coordinator.ready_pending();
    let coordinator_weak = coordinator.weak();

    // Every planned tab first, before any other widget: each starts its own Neovim at its own
    // planned directory, and the window exists only if one of them did.
    let tabs = tab_view::build(TabsParts { metrics, config });
    let opened: Vec<Option<TabHandle>> = plan
        .tabs
        .iter()
        .map(|tab| tabs.open_tab(tab_parts(tab)))
        .collect();
    let Some(first) = tabs.entries().first().cloned() else {
        eprintln!(
            "wolf-gnome: no tab of the window could start its core; the window is not opened"
        );
        return None;
    };
    // Each `open_tab` selects the page it appends; the launch ends on the planned active tab, or
    // the first opened when that one failed to start.
    let selected = opened
        .get(plan.active_tab)
        .cloned()
        .flatten()
        .unwrap_or_else(|| first.clone());
    tabs.select(&selected);
    tabs.seed_recency();
    let empty = selected.is_empty();
    let launch_cwd = selected.launch_cwd();
    // Test-only (WOLF_SMOKE_TERMINAL): a terminal pane in tab 0's first pane before the app
    // reports ready.
    if let Some(command) = crate::app::smoke_terminal_command() {
        first.panes().request_terminal(command);
    }

    // Main child is the tab view so the overlay spans the pane area (`OVERLAY_ORIGIN_*_EXPECTED`);
    // add order is stacking. See notes/crates/wolf-gnome/src/window.rs.md#overlay-stacking-across-tabs
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&tabs.view_widget()));

    let crash = crash_overlay::build();

    // Focus returns to the selected tab's strip, resolved at close time: a captured per-tab strip is
    // unmapped after a switch and kills key dispatch. See `picker::FocusTarget`.
    let focus_target = tabs.focus_target();

    let picker = picker::build(&overlay, focus_target.clone(), tabs.core_source());
    // Switch to Window rows, from the coordinator per open. Weak: a strong ref closes the
    // coordinator -> window -> palette cycle and leaks the graph; a dropped coordinator yields none.
    let palette_windows: palette::WindowSource = {
        let weak = coordinator.weak();
        Rc::new(move || {
            coordinator::upgrade(&weak)
                .map(|coordinator| coordinator.palette_windows(id))
                .unwrap_or_default()
        })
    };
    let palette = palette::build(
        &overlay,
        focus_target.clone(),
        tabs.core_source(),
        palette_windows,
    );
    let buffers = buffer_picker::build(&overlay, focus_target.clone(), tabs.core_source());

    overlay.add_overlay(&crash.widget());
    overlay.add_overlay(&crash.banner_widget());

    // The tree reads no cwd: its root label is passed here and on each `reroot`, so the shell alone
    // knows the root. Per window, not per tab: see notes/wolf-gnome/window-structure.md
    let tree = tree_view::build(
        tabs.core_source(),
        focus_target.clone(),
        !empty,
        &workspace::root_label(&launch_cwd),
    );
    let search = search_panel::build(tabs.core_source(), focus_target.clone());
    let sidebar = sidebar::build(&tree, &search);
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_start_child(Some(&sidebar.widget()));
    paned.set_end_child(Some(&overlay));
    paned.set_position(TREE_PANE_WIDTH);

    // No title widget is deliberate: one changes the header bar's natural height, which is a term of
    // `PANE_ORIGIN_Y_EXPECTED`. See notes/wolf-gnome/window-structure.md
    let header = adw::HeaderBar::new();

    // Both start insensitive until the first render wake's `refresh_nav_buttons`: with no snapshot
    // `nav_press` refuses every press, so a sensitive button would only print a refusal.
    let nav_back_button = gtk::Button::builder()
        .icon_name(HISTORY_BACK_ICON)
        .tooltip_text("Back (Alt+Left)")
        .sensitive(false)
        .build();
    let nav_forward_button = gtk::Button::builder()
        .icon_name(HISTORY_FORWARD_ICON)
        .tooltip_text("Forward (Alt+Right)")
        .sensitive(false)
        .build();
    let nav_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    nav_box.add_css_class("linked");
    nav_box.append(&nav_back_button);
    nav_box.append(&nav_forward_button);
    header.pack_start(&nav_box);
    header.pack_start(&sidebar.toggles().widget());

    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Main menu")
        .menu_model(&shell::build_menu_model())
        .build();
    header.pack_end(&menu_button);
    let titlebar = TitlebarStyle::install(id, &header, &tabs.bar_widget());

    // Header, tab bar, then the split. The bar autohides at one page, keeping one-tab geometry equal
    // to no tab layer. See notes/wolf-gnome/window-structure.md
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&tabs.bar_widget());
    content.append(&paned);

    // The restored size, or the defaults. Window POSITION is never restored:
    // Wayland gives a client neither its own position nor screen geometry.
    let (window_width, window_height) = plan.window_size.unwrap_or((WINDOW_WIDTH, WINDOW_HEIGHT));
    let window = adw::ApplicationWindow::builder()
        .application(app)
        // The same string `title::window_title` falls back to, so the title never
        // flickers between two spellings before the first cwd is reported.
        .title(WINDOW_TITLE_FALLBACK)
        .default_width(window_width)
        .default_height(window_height)
        .content(&content)
        .build();

    // The `win.*` actions, accelerators and tab drains, installed before `present()` so the menu
    // resolves them; the `app.*` rows are the coordinator's.
    let shell = shell::build(ShellParts {
        window: &window,
        picker: &picker,
        palette: &palette,
        buffers: &buffers,
        search: &search,
        tree: &tree,
        sidebar: &sidebar,
        paned: &paned,
        nav_back_button: &nav_back_button,
        nav_forward_button: &nav_forward_button,
        launch_cwd,
        crash_overlay: &crash,
        tabs: &tabs,
        titlebar: &titlebar,
        coordinator: coordinator.weak(),
        window_id: id,
        ready_armed,
    });
    shell.install_actions(app);
    shell.apply_selected_tab();
    // The panel is per window, so it takes the selected tab's record; a background tab's gives way
    // to it once that tab is selected.
    shell.restore_selected_search();

    // Explicit activation, not `action-name`, which renders insensitive while its action is absent.
    // No smoke clicks these: crates/wolf-gnome/tests/README.md#deliberate-test-gaps
    {
        let window = window.clone();
        nav_back_button.connect_clicked(move |_button| {
            // Qualified: `gtk::Widget::activate_action` takes the `win.`-prefixed name and reports failure;
            // `gio::ActionGroup`'s takes a bare name and returns nothing.
            if let Err(err) =
                gtk::prelude::WidgetExt::activate_action(&window, HISTORY_BACK_ACTION, None)
            {
                eprintln!("wolf-gnome: failed to activate {HISTORY_BACK_ACTION}: {err}");
            }
        });
    }
    {
        let window = window.clone();
        nav_forward_button.connect_clicked(move |_button| {
            if let Err(err) =
                gtk::prelude::WidgetExt::activate_action(&window, HISTORY_FORWARD_ACTION, None)
            {
                eprintln!("wolf-gnome: failed to activate {HISTORY_FORWARD_ACTION}: {err}");
            }
        });
    }

    // Activation moves terminal focus as a pane change does, and the UI focus that drives the
    // selected tab's `:checktime`; mode 1004 programs see the first.
    {
        let tabs = tabs.clone();
        let coordinator = coordinator.weak();
        window.connect_is_active_notify(move |window| {
            tabs.sync_terminal_focus(window.is_active());
            tabs.sync_ui_focus(window.is_active());
            if window.is_active() {
                if let Some(coordinator) = coordinator::upgrade(&coordinator) {
                    coordinator.on_window_focused(id);
                }
            }
        });
    }

    let im = wire_keys(&window, &tabs, &picker, &palette, &buffers, &search);
    wire_close(CloseParts {
        window: &window,
        tabs: &tabs,
        im: &im,
        picker: &picker,
        palette: &palette,
        buffers: &buffers,
        search: &search,
        tree: &tree,
        titlebar: &titlebar,
        coordinator: coordinator.weak(),
        id,
    });

    // Registered before the first drain, so a coordinator call it reaches finds this window.
    let handle = WindowHandle {
        inner: Rc::new(WindowInner {
            id,
            window: window.clone(),
            tabs: tabs.clone(),
            shell,
            first_paint: Cell::new(None),
            show: RefCell::new(None),
            painted: RefCell::new(Some(Vec::new())),
        }),
    };
    coordinator.register(&handle);

    // Every tab's first drain runs on the main loop after this, so after `wire_close` installs
    // `close-request`: a drained `CloseWindow` before it would orphan every tab's nvim.
    for tab in tabs.entries() {
        tab.wake();
    }

    let show: PaintedCallback = {
        let weak = Rc::downgrade(&handle.inner);
        let minimized = plan.minimized;
        Box::new(move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let handle = WindowHandle { inner };
            // GTK minimizes a window asked before it maps right after it maps.
            if minimized {
                handle.inner.window.minimize();
            }
            handle.inner.window.present();
            handle.watch_first_paint();
            // Focus the SELECTED tab's cmdline strip so key events have a focused target in
            // the editor region to bubble from to the window-level key controller.
            if let Some(area) = (focus_target)() {
                area.grab_focus();
            }
            if ready_armed {
                wire_ready_triggers(&handle, coordinator_weak);
            }
        })
    };
    if present {
        show();
    } else {
        *handle.inner.show.borrow_mut() = Some(show);
    }

    Some(handle)
}

/// What [`TabsHandle::open_tab`] needs to open the tab `tab` plans.
fn tab_parts(tab: &TabPlan) -> TabParts {
    TabParts {
        cwd: tab.cwd.clone(),
        empty: tab.empty,
        sidebar_shown: tab.sidebar_visible,
        saved: tab.saved.clone(),
        sidebar_page: tab.sidebar_page,
        search: tab.search.clone(),
        drawer_share: tab.drawer_share,
    }
}

/// Whether `window`'s display's default seat exposes a keyboard device — the
/// readiness file's delivery precondition ([`crate::ready_seam`]).
///
/// It reads the seat's non-logical devices, `devices(KEYBOARD)`, which lists the
/// `wtype` virtual keyboard, not `SeatExt::keyboard`: that answers GDK's logical
/// "Core Keyboard", which exists (and `capabilities()` reports `KEYBOARD`) on a seat
/// with no keyboard at all, so it would make the conjunct vacuous. Measured under
/// `cage`: `devices(KEYBOARD)` is empty until a virtual keyboard has existed, and
/// stays non-empty after the `wtype` that created it exits.
fn seat_has_keyboard(window: &adw::ApplicationWindow) -> bool {
    WidgetExt::display(window)
        .default_seat()
        .is_some_and(|seat| !seat.devices(gdk::SeatCapabilities::KEYBOARD).is_empty())
}

/// Connects the two readiness triggers no wake covers: the seat gaining a device
/// (`device-added`, which is how a keyboard appears) and a painted frame (the frame
/// clock's `after-paint`, whose first firing also records this window's map
/// ordinal).
///
/// Called by [`build`] only while the readiness probe is pending, after
/// `present()`, so the frame clock exists. Both closures hold `Weak`s, so neither
/// keeps the coordinator or the window alive.
fn wire_ready_triggers(handle: &WindowHandle, coordinator: Weak<CoordinatorInner>) {
    if let Some(seat) = WidgetExt::display(&handle.inner.window).default_seat() {
        let coordinator = coordinator.clone();
        seat.connect_device_added(move |_seat, _device| {
            if let Some(coordinator) = coordinator::upgrade(&coordinator) {
                coordinator.check_ready();
            }
        });
    }
    let Some(clock) = handle.inner.window.frame_clock() else {
        // Not `eprintln!`, which panics on a failed stderr write: a diagnostic must
        // never take the application down.
        let _ = std::io::stderr().write_all(
            b"wolf-gnome: the window has no frame clock after present(); the readiness file \
              will never be written\n",
        );
        return;
    };
    let window = Rc::downgrade(&handle.inner);
    clock.connect_after_paint(move |_clock| {
        let Some(coordinator) = coordinator::upgrade(&coordinator) else {
            return;
        };
        if let Some(inner) = window.upgrade() {
            if inner.first_paint.get().is_none() {
                inner
                    .first_paint
                    .set(Some(coordinator.next_paint_ordinal()));
            }
        }
        coordinator.check_ready();
    });
}

/// Installs this window's input method and its key controller, and returns the
/// context so [`wire_close`] can tear it down.
///
/// While any transient overlay (the file picker, the command palette or the buffer
/// picker) is open, or keyboard focus is inside the sidebar's search panel, keys are
/// left for those focused widgets: nothing is forwarded to the core, and — the half
/// that is easy to lose — nothing is OFFERED to the input method either. Otherwise every press is offered
/// to this window's [`gtk::IMMulticontext`] first, and only a press the context
/// DECLINES is translated via the pure [`crate::gdk_key`] seam into core input; a
/// press it claims arrives later as a `commit` string, which becomes core input
/// through [`crate::im_ui::commit_decision`] instead. The precedence and the commit
/// rulebook are [`mod@crate::im_ui`]'s, not this function's — every branch below
/// reads its answer from [`crate::im_ui::key_route`]. No smoke discriminates the
/// overlay guard — see "What is still unproven" in
/// `crates/wolf-gnome/tests/README.md`.
///
/// Where a key or a commit goes is decided at that moment, against the SELECTED
/// tab's core ([`TabsHandle::selected_core`]) and [`TabsHandle::active_terminal`]: a
/// terminal pane's session takes it through `WolfCore::terminal_key`, or
/// [`crate::terminal::input::commit_route`] for a commit, whose one-printable rule
/// lets core's `<C-w>` router see the keys an input method claims; any other pane's
/// goes to that tab's Neovim. The overlay guard and the input-method precedence are
/// the same for both.
///
/// The controller carries NO propagation phase (i.e. Bubble), so a chord Wolf
/// binds — Ctrl+Shift+O opens the picker, Ctrl+Shift+P the palette, Ctrl+Shift+E
/// the buffer picker, Ctrl+Shift+F the project search — never reaches it:
/// [`crate::shell`]'s chord controller matches it in the CAPTURE phase and stops
/// propagation. That is deliberate; it is what guarantees a bound chord is not ALSO
/// forwarded to Neovim.
///
/// **Do not unify the two controllers, and do not reach for
/// `EventControllerKey::set_im_context`.** Both are one-line "simplifications" and
/// both invert the ordering above. GTK runs every capture-phase controller
/// root-to-target before any bubble-phase one, so the chord controller has first
/// refusal by CONSTRUCTION rather than by cooperation: a bound, enabled chord is
/// matched and stopped before the input method is ever offered the key. Attaching
/// the context to the chord controller, or moving this controller into the capture
/// phase, would offer every bound chord to the context first and let it swallow
/// them. `set_im_context` fails a different way: GTK would make the offer before
/// this function's overlay guard ran, which moves the overlay-versus-input-method
/// precedence out of this crate and makes it unstatable and untestable. Calling
/// `filter_keypress` by hand from inside `key-pressed` was measured to behave
/// identically, so the hand-rolled call costs nothing.
///
/// Releases are forwarded to the context for the same measurement's sake:
/// `set_im_context` offers BOTH phases, so a press-only forward silently diverges
/// from the thing that measurement licensed, and an input method does claim events
/// while a sequence is pending. Nothing else in this crate consumes key releases, so
/// the forward costs nothing and closes a class of wedged-state-machine failures
/// that would otherwise be silent.
fn wire_keys(
    window: &adw::ApplicationWindow,
    tabs: &TabsHandle,
    picker: &PickerHandle,
    palette: &PaletteHandle,
    buffers: &BufferPickerHandle,
    search: &SearchPanelHandle,
) -> gtk::IMMulticontext {
    // Constructed here rather than passed in: nothing above needs to supply it, and
    // returning it keeps this function under clippy's seven-parameter ceiling.
    let im = gtk::IMMulticontext::new();
    // BEFORE `focus_in` below, so the context already has a surface to work against
    // at the moment it is told it is focused.
    im.set_client_widget(Some(window));

    {
        let tabs = tabs.clone();
        let picker = picker.clone();
        let palette = palette.clone();
        let buffers = buffers.clone();
        let search = search.clone();
        im.connect_commit(move |_im, text| {
            let overlay_open = picker.is_open()
                || palette.is_open()
                || buffers.is_open()
                || search.owns_keyboard();
            match im_ui::commit_decision(overlay_open, text) {
                im_ui::CommitDecision::Refuse(reason) => {
                    eprintln!("wolf-gnome: commit not forwarded: {reason}");
                }
                im_ui::CommitDecision::Send(events) => {
                    let Some(core) = tabs.selected_core() else {
                        return;
                    };
                    if let Some(session) = tabs.active_terminal() {
                        send_terminal_commit(&core, session, text);
                        return;
                    }
                    // A commit with no core performs nothing: the window is tearing down. The borrow lives in this
                    // one statement, per notes/wolf-gnome-decisions.md#threading-and-the-wake
                    let core_ref = core.borrow();
                    if let Some(core) = core_ref.as_ref() {
                        for event in events {
                            core.send_input(InputEvent::Key(event));
                            // One per Key sent, so an N-character commit counts N: the contract is keys delivered, and most
                            // keys arrive as commits since the input method filters plain printables.
                            frame_metrics::note_keystroke();
                        }
                    }
                }
            }
        });
    }

    im.focus_in();

    let controller = gtk::EventControllerKey::new();
    {
        let tabs = tabs.clone();
        let picker = picker.clone();
        let palette = palette.clone();
        let buffers = buffers.clone();
        let search = search.clone();
        let im = im.clone();
        controller.connect_key_pressed(move |controller, keyval, _keycode, state| {
            let overlay_open = picker.is_open()
                || palette.is_open()
                || buffers.is_open()
                || search.owns_keyboard();
            // Asked with `None` FIRST: the answer is what licenses the offer, and the
            // offer cannot be taken back once made.
            let route = match im_ui::key_route(im_ui::KeyPhase::Press, overlay_open, None) {
                im_ui::KeyRoute::AskInputMethod => {
                    // No current event means the input method declined: falling through loses at most a composition,
                    // claiming loses the key. So `is_some_and`, never `map_or(true, …)`.
                    let filtered = controller
                        .current_event()
                        .is_some_and(|event| im.filter_keypress(&event));
                    im_ui::key_route(im_ui::KeyPhase::Press, overlay_open, Some(filtered))
                }
                settled => settled,
            };
            match route {
                // Nothing forwarded, nothing offered, nothing counted.
                im_ui::KeyRoute::Overlay => glib::Propagation::Proceed,
                im_ui::KeyRoute::Consumed => glib::Propagation::Stop,
                im_ui::KeyRoute::Translate => match crate::gdk_key(keyval, state) {
                    Some(event) => {
                        let terminal = tabs.active_terminal();
                        let cell = tabs.selected_core().unwrap_or_default();
                        if let Some(core) = cell.borrow().as_ref() {
                            match terminal {
                                Some(session) => core.terminal_key(session, event),
                                None => core.send_input(InputEvent::Key(event)),
                            }
                            // Counted where delivered, so keys the overlay guard swallowed, the input method claimed or that
                            // reached no core stay out of the report's `wakes/key` denominator.
                            frame_metrics::note_keystroke();
                        }
                        glib::Propagation::Stop
                    }
                    None => glib::Propagation::Proceed,
                },
                // Both unreachable on a press and spelled out, not `_`, so a new `KeyRoute` is an E0004 here:
                // `Ignore` answers a release, and the re-ask passed `Some(_)`, never `AskInputMethod`.
                im_ui::KeyRoute::Ignore | im_ui::KeyRoute::AskInputMethod => {
                    glib::Propagation::Proceed
                }
            }
        });
    }

    {
        let picker = picker.clone();
        let palette = palette.clone();
        let buffers = buffers.clone();
        let search = search.clone();
        let im = im.clone();
        controller.connect_key_released(move |controller, _keyval, _keycode, _state| {
            let overlay_open = picker.is_open()
                || palette.is_open()
                || buffers.is_open()
                || search.owns_keyboard();
            let route = match im_ui::key_route(im_ui::KeyPhase::Release, overlay_open, None) {
                im_ui::KeyRoute::AskInputMethod => {
                    // `is_some_and` for the press handler's reason: a missing event means the input method declined.
                    // It matters less here, but the two handlers must not disagree on it.
                    let filtered = controller
                        .current_event()
                        .is_some_and(|event| im.filter_keypress(&event));
                    im_ui::key_route(im_ui::KeyPhase::Release, overlay_open, Some(filtered))
                }
                settled => settled,
            };
            // `key-released` is a void signal: a claimed release cannot be stopped, a declined one needs
            // nothing. So the offer above is the point; the match stays `_`-free to force classification.
            match route {
                im_ui::KeyRoute::Overlay
                | im_ui::KeyRoute::Consumed
                | im_ui::KeyRoute::Ignore
                | im_ui::KeyRoute::AskInputMethod => {}
                // Never `crate::gdk_key`: the press was already forwarded, so translating the release doubles
                // every key.
                im_ui::KeyRoute::Translate => {}
            }
        });
    }

    window.add_controller(controller);
    im
}

/// Sends an input method's commit `text` to terminal `session` through
/// [`crate::terminal::input::commit_route`]: one printable ASCII character as a key, so core's
/// `<C-w>` router sees it, other text as text, and nothing for a control-only commit.
fn send_terminal_commit(core: &CoreCell, session: TerminalId, text: &str) {
    let core_ref = core.borrow();
    let Some(core) = core_ref.as_ref() else {
        return;
    };
    match terminal_input::commit_route(text) {
        terminal_input::TerminalCommit::Nothing => {}
        terminal_input::TerminalCommit::Key(event) => {
            core.terminal_key(session, event);
            frame_metrics::note_keystroke();
        }
        terminal_input::TerminalCommit::Text(text) => {
            core.terminal_text(session, &text);
            for _ in text.chars() {
                frame_metrics::note_keystroke();
            }
        }
    }
}

/// The handles [`wire_close`] needs, as one struct rather than nine positional
/// parameters (`clippy::too_many_arguments` bites at seven), following the
/// [`ShellParts`] / [`crate::persistence::StateParts`] precedent.
struct CloseParts<'a> {
    /// The window whose `close-request` is being handled.
    window: &'a adw::ApplicationWindow,
    /// The window's tabs, whose cores the handler shuts down.
    tabs: &'a TabsHandle,
    /// This window's input method, focused out and detached from its client widget
    /// as the LAST thing the handler does — after every core has been taken.
    im: &'a gtk::IMMulticontext,
    /// The file picker, whose close drops its `FileSearch`.
    picker: &'a PickerHandle,
    /// The command palette, closed for teardown symmetry.
    palette: &'a PaletteHandle,
    /// The buffer picker, closed for teardown symmetry.
    buffers: &'a BufferPickerHandle,
    /// The sidebar's search panel, whose close CANCELS an in-flight `rg`.
    search: &'a SearchPanelHandle,
    /// The file-tree sidebar, whose close drops its `FileSearch`.
    tree: &'a TreeHandle,
    /// The header bar's and tab bar's provider, removed from the display at close.
    titlebar: &'a TitlebarStyle,
    /// The coordinator, told FIRST — as a `Weak`, because it owns this window's
    /// handle and this closure is owned by the window.
    coordinator: Weak<CoordinatorInner>,
    /// The closing window's identity.
    id: WindowId,
}

/// Installs the close handler: tell the coordinator, then shut every tab's core
/// down cleanly ([`TabsHandle::shutdown_all`]).
///
/// **The `coordinator.on_window_closing(id)` call is the FIRST statement in the
/// handler, and that ordering is load-bearing.** It must run while the cores and
/// the panes are still alive: a subtree gathered after a core is taken out sees no
/// core, `state_seam::may_snapshot` refuses it, and the session is silently not
/// saved. Moving it below any of the three statements that follow
/// would leave the relaunch smokes
/// (`pane_layout_survives_relaunch_under_cage`,
/// `single_pane_file_survives_relaunch_under_cage`) with an empty saved window state.
///
/// What that call DOES with the subtree is [`crate::state_seam::close_commit`]'s
/// three-arm rule, not this handler's: closing the last window keeps the session,
/// closing one of several drops that window, and a close during a quit commits
/// nothing because the quit path already committed everything.
///
/// The input-method teardown is the LAST statement, and that ordering is
/// load-bearing for its own reason: `focus_out` on a context holding a pending
/// compose sequence may COMMIT it, and running the teardown after every core is
/// taken out means such a commit finds `None` and performs nothing rather than
/// pushing into a core that is being shut down. Clearing the client
/// widget is what keeps the context from holding the closed window's whole widget
/// tree alive — `gtk_im_context_set_client_widget` stores the widget in the context,
/// which the controller closures own and which the window owns, and this application
/// closes windows routinely.
fn wire_close(parts: CloseParts<'_>) {
    let tabs = parts.tabs.clone();
    let im = parts.im.clone();
    let picker = parts.picker.clone();
    let palette = parts.palette.clone();
    let buffers = parts.buffers.clone();
    let search = parts.search.clone();
    let tree = parts.tree.clone();
    let titlebar = parts.titlebar.clone();
    let weak = parts.coordinator;
    let id = parts.id;
    parts.window.connect_close_request(move |_window| {
        // First, while cores and panes live: synchronous gather, commit and flush, releasing every core
        // borrow so the takes in `shutdown_all` below cannot panic.
        if let Some(coordinator) = coordinator::upgrade(&weak) {
            coordinator.on_window_closing(id);
        }
        // Close these first so background work drops before the core shuts down: `FileSearch`es cancel,
        // `rg` dies. The palette and buffer picker closes are symmetry; all five are idempotent.
        picker.close();
        palette.close();
        buffers.close();
        search.close();
        tree.close();
        // An id is minted again after its window closes, and a stale provider would restyle that window.
        titlebar.uninstall();
        tabs.shutdown_all();
        // LAST, and after every core is gone: see the doc comment above for both
        // reasons.
        im.focus_out();
        im.set_client_widget(None::<&gtk::Widget>);
        glib::Propagation::Proceed
    });
}
