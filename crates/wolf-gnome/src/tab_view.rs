//! The GTK GLUE for NATIVE TABS: an [`adw::TabBar`] over an [`adw::TabView`],
//! where every tab owns its own captive [`WolfCore`]
//! (`specs/application-state.md#8-one-neovim-per-tab`).
//!
//! Every decision this module acts on is [`mod@crate::tabs_ui`]'s — what closing
//! a tab does, the directory a new tab starts in, the string a tab draws, and
//! which tab's Neovim holds the UI focus. This file is the widget work of carrying
//! those answers out, in the same relationship [`crate::persistence`] has to
//! [`crate::state_seam`].
//!
//! ## What is per TAB and what is per WINDOW
//!
//! Per tab: the core, its wake channel and drain, its liveness and restart
//! budget, its drawer session (which lives in the core), the pane view (its
//! `DrawingArea`s, its cmdline strip, its toast and its completion popup, and the
//! drawer's divider share), the tab's own empty-state page, and its
//! [`crate::persistence::StateHandle`], which records the tab's sidebar page and search
//! fields.
//!
//! Per window: the header bar, the `win.` action namespace, the three transient
//! overlays, the crash/stall overlay and **the file-tree sidebar**. Each of them acts on the SELECTED tab's core, resolved at
//! call time through [`TabsHandle::selected_core`] or [`TabsHandle::core_source`].
//! The sidebar's placement is a measured constraint rather than a preference; see
//! `notes/wolf-gnome/window-structure.md`.
//!
//! ## The bar autohides, and that is load-bearing
//!
//! [`adw::TabBar::set_autohide`] is left at its default of `true`, so a
//! one-tab window is byte-identical in geometry to one with no tab layer at all.
//! The cage smokes' `PANE_ORIGIN_Y_EXPECTED` is only correct while the bar is
//! hidden; turning autohide off shifts the pane origin by the bar's height and
//! breaks all eight pointer smokes loudly through their shared origin assertion,
//! which is the guard.
//!
//! ## The close signal is a VETO, not a notification
//!
//! `close-page` asks whether the page may close and the handler MUST answer, with
//! [`adw::TabView::close_page_finish`], and return [`glib::Propagation::Stop`]. An
//! unanswered veto leaves the page on screen forever with no error and no message.
//! The last page's close is answered with a REFUSAL and routed into the window's own
//! close — see [`crate::tabs_ui::close_outcome`].
//!
//! ## Restore
//!
//! `window::build` opens every planned tab before it builds any other widget,
//! each with its own Neovim and its saved tab, as
//! `wolf_core::app_state::restored_tab` rewrites it, queued on its pane view
//! through [`crate::pane_view::PaneHandle::request_layout_rebuild`]. Each layout is
//! realized on its own tab's first wake with a pane, against that tab's own
//! Neovim, so a background tab is restored like the selected one. A tab restored
//! empty queues nothing.
//!
//! ## Tab recency
//!
//! The view keeps its tabs' [`TabRecency`]: a real selection change moves the
//! selected tab to the front and a closed tab is forgotten.
//! [`TabsHandle::seed_recency`] reseeds it after a restore, because opening each
//! planned tab selects it in turn.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use adw::prelude::*;
use gtk::glib;

use wolf_core::app_state::Tab;
use wolf_core::{CoreConfig, Liveness, TerminalId, WakeFn, WolfConfig, WolfCore};

use crate::appearance;
use crate::drawer::DrawerShare;
use crate::pane_view::{self, PaneHandle};
use crate::persistence::{self, StateHandle, StateParts};
use crate::picker::FocusTarget;
use crate::search_panel::SearchFields;
use crate::shell;
use crate::sidebar::SidebarPage;
use crate::stacking::TabRecency;
use crate::tabs_ui::{self, TabClose, TabId};
use crate::CellMetrics;

/// Initial grid size handed to each tab's [`WolfCore`]; the authoritative size is
/// recomputed from the real widget allocation on the first resize.
const INITIAL_COLS: u16 = 80;
/// Initial grid rows (see [`INITIAL_COLS`]).
const INITIAL_ROWS: u16 = 24;

/// Wake channel capacity, per tab. One slot is enough: it coalesces a burst of
/// flushes into a single queued redraw (a dropped "full" send just means a redraw
/// is already pending).
const WAKE_CHANNEL_CAP: usize = 1;

/// One tab's core: `None` once [`TabHandle::shutdown`] has taken it out.
pub type CoreCell = Rc<RefCell<Option<WolfCore>>>;

/// Resolves the core a window-level widget acts on — the SELECTED tab's — at call
/// time. A closure rather than a cell for [`FocusTarget`]'s reason: a value bound
/// once goes stale at the first tab switch. `None` means no tab is selected.
pub type CoreSource = Rc<dyn Fn() -> Option<CoreCell>>;

/// Opens a file the picker, the sidebar or the open dialog chose, at a `(line, col)` when
/// given, in the selected tab when its terminal drawer is the active pane, and answers whether
/// it did. On `false` the caller sends its own `InputEvent::Open`.
pub type FileOpener = Rc<dyn Fn(&Path, Option<(u64, u64)>) -> bool>;

/// Runs `f` on the core `source` resolves, or answers `None` when there is none.
///
/// The cell's borrow lives for this call alone, so a caller never holds it across
/// another core call — the borrow discipline `notes/wolf-gnome-decisions.md`
/// records.
pub fn with_core<R>(source: &CoreSource, f: impl FnOnce(&WolfCore) -> R) -> Option<R> {
    let cell = source()?;
    let core_ref = cell.borrow();
    let result = core_ref.as_ref().map(f);
    result
}

/// What [`build`] needs, as one struct rather than positional parameters (the
/// [`StateParts`] / [`crate::shell::ShellParts`] precedent).
pub struct TabsParts<'a> {
    /// The cell metrics, measured once for the process and handed to each tab's
    /// pane view.
    pub metrics: &'a Rc<CellMetrics>,
    /// The user configuration, whose appearance each tab's Neovim is started
    /// under ([`crate::appearance::apply_at_launch`]).
    pub config: &'a WolfConfig,
}

/// What [`TabsHandle::open_tab`] needs for ONE tab.
pub struct TabParts {
    /// The directory this tab's Neovim starts in, and the directory it persists
    /// until Neovim reports a live one. [`crate::tabs_ui::new_tab_cwd`] decides it
    /// for a new tab; a restored tab gets its own planned cwd.
    pub cwd: PathBuf,
    /// Whether the tab opens in the empty state, which hides its panes behind its
    /// own status page.
    pub empty: bool,
    /// The user's sidebar PREFERENCE for this tab, which is what it persists.
    pub sidebar_shown: bool,
    /// The tab as it was SAVED, or `None` for a tab the user opened.
    ///
    /// Two consumers, and both are needed. The pane view realizes its root on this
    /// tab's first wake with a window grid, unless the tab is empty. And
    /// [`crate::persistence`] keeps it as the SEED this tab contributes until its own
    /// first gather succeeds — which for a tab whose realization bound no pane is
    /// never.
    pub saved: Option<Tab>,
    /// The sidebar page the tab shows when selected.
    pub sidebar_page: SidebarPage,
    /// The search panel's fields the tab records.
    pub search: SearchFields,
    /// The tab's drawer share: its saved one, the selected tab's for New Tab, or a new
    /// one.
    pub drawer_share: DrawerShare,
}

/// One tab's restart state, read and written by [`crate::shell`]'s restart path.
///
/// Per tab because liveness is: a crash restarts its own tab, and one tab's
/// exhausted budget must not refuse another tab's first automatic attempt.
#[derive(Default)]
pub struct TabRestart {
    /// FRONT-END re-entrancy guard for the restart. Needed in addition to the
    /// core's own CAS, whose `Ok(false)` would otherwise read as "no respawn
    /// happened".
    pub restarting: Cell<bool>,
    /// Whether a deferred automatic restart is already pending, so a burst of
    /// `Dead` wakes schedules exactly one.
    pub scheduled: Cell<bool>,
    /// AUTOMATIC restart attempts since this tab's core was last seen `Alive`,
    /// capped at [`crate::liveness_ui::MAX_AUTO_RESTART_ATTEMPTS`].
    pub attempts: Cell<u32>,
}

/// One tab's shared state.
struct TabInner {
    /// This tab's identity within its window.
    id: TabId,
    /// This tab's `WOLF_TAB_ID` (`specs/cli-and-file-open.md` §3), which its core gives every
    /// terminal session it starts. Minted with the core and kept across a crash restart.
    wolf_tab_id: wolf_core::TabId,
    /// The view this page belongs to, so a tab can answer whether it is the
    /// selected one without the registry above it.
    view: adw::TabView,
    /// This tab's page.
    page: adw::TabPage,
    /// This tab's own core.
    core: CoreCell,
    /// The sending half of this tab's wake channel. The core's wake closure holds a
    /// clone; [`TabHandle::shutdown`] closes the channel, which ends the drain.
    wake: async_channel::Sender<()>,
    /// This tab's restart state.
    restart: TabRestart,
    /// This tab's pane view — its `DrawingArea`s, cmdline strip, toast and popup.
    panes: PaneHandle,
    /// This tab's own contribution to the saved window state. Every tab's is read
    /// by `window::WindowHandle::cached_window`, which orders them by
    /// [`TabHandle::page_position`].
    state: StateHandle,
    /// This tab's empty-state page, an overlay child above its panes.
    empty_state: gtk::Widget,
}

/// A cloneable handle to ONE tab.
#[derive(Clone)]
pub struct TabHandle {
    inner: Rc<TabInner>,
}

/// A handle to one tab that does not keep it alive, for a deferred callback such as
/// the restart timer.
#[derive(Clone)]
pub struct WeakTab {
    inner: Weak<TabInner>,
}

impl WeakTab {
    /// The tab, or `None` once it has closed.
    #[must_use]
    pub fn upgrade(&self) -> Option<TabHandle> {
        self.inner.upgrade().map(|inner| TabHandle { inner })
    }
}

impl TabHandle {
    /// This tab's identity within its window.
    #[must_use]
    pub fn id(&self) -> TabId {
        self.inner.id
    }

    /// This tab's `WOLF_TAB_ID`, which a request from one of its terminals carries. Not
    /// [`TabHandle::id`], which is unique only within the window.
    #[must_use]
    pub fn wolf_tab_id(&self) -> wolf_core::TabId {
        self.inner.wolf_tab_id
    }

    /// This tab's page, for the [`adw::TabView`] calls that name one.
    #[must_use]
    pub fn page(&self) -> &adw::TabPage {
        &self.inner.page
    }

    /// This tab's own core.
    #[must_use]
    pub fn core(&self) -> &CoreCell {
        &self.inner.core
    }

    /// This tab's core's [`Liveness`], or `None` once the core has been taken out.
    /// A lock-free atomic read, never an RPC.
    #[must_use]
    pub fn liveness(&self) -> Option<Liveness> {
        self.inner.core.borrow().as_ref().map(WolfCore::liveness)
    }

    /// This tab's restart state.
    #[must_use]
    pub fn restart(&self) -> &TabRestart {
        &self.inner.restart
    }

    /// A handle that does not keep this tab alive.
    #[must_use]
    pub fn downgrade(&self) -> WeakTab {
        WeakTab {
            inner: Rc::downgrade(&self.inner),
        }
    }

    /// Queues one wake on this tab's own channel, so its drain runs once more. A
    /// full channel already holds one, and a closed one belongs to a closed tab.
    pub fn wake(&self) {
        let _ = self.inner.wake.try_send(());
    }

    /// Where this tab sits in the tab BAR, counting from zero.
    ///
    /// [`adw::TabView`] owns this number and moves it when the user DRAGS a tab along
    /// the bar — `AdwTabBar` offers no property that turns dragging off — so it is the
    /// only authority for the order the user arranged. [`TabsHandle::entries`] is
    /// insertion order and does not track it. Read by
    /// `window::WindowHandle::cached_window`, which pairs it with
    /// [`TabHandle::cached_tab`] and sorts through
    /// [`crate::state_seam::ordered_tab_states`] so the persisted order is the one
    /// the user sees.
    ///
    /// `AdwTabView:shortcuts` keeps its default, which carries a
    /// `Ctrl+Shift+Page_Up` / `Ctrl+Shift+Page_Down` reorder chord — but whether that
    /// chord reaches libadwaita is unconfirmed, and no rule here rests on it:
    /// `window::wire_keys` installs a bubble-phase key controller on the toplevel and
    /// `input::named_key` maps `Page_Up` / `Page_Down` whatever the modifiers hold,
    /// answering `Propagation::Stop`, so it very likely reaches Neovim instead.
    #[must_use]
    pub fn page_position(&self) -> i32 {
        self.inner.view.page_position(&self.inner.page)
    }

    /// This tab's pane view.
    #[must_use]
    pub fn panes(&self) -> &PaneHandle {
        &self.inner.panes
    }

    /// This tab's cmdline strip — the focus-return target every overlay resolves
    /// through [`TabsHandle::focus_target`].
    #[must_use]
    pub fn focus_area(&self) -> gtk::DrawingArea {
        self.inner.panes.cmdline_widget()
    }

    /// Whether GTK currently shows this tab.
    #[must_use]
    pub fn is_selected(&self) -> bool {
        self.inner.view.selected_page().as_ref() == Some(&self.inner.page)
    }

    /// Shows or hides this tab's empty state and keeps its persisted directory in
    /// step.
    ///
    /// The widget half is per tab because the empty state is opaque and full-bleed
    /// over a tab's own panes; the sidebar, its header-bar toggle group and the
    /// `win.` actions the empty state also drives are per window and follow the
    /// selected tab through [`crate::shell::AppShell::apply_selected_tab`].
    pub fn set_empty(&self, empty: bool) {
        self.inner.empty_state.set_visible(empty);
        self.inner.state.set_empty(empty);
    }

    /// Hides this tab's empty-state page so a file opened from outside Wolf shows, and leaves
    /// the tab empty for persistence: it is still on no folder.
    pub fn hide_empty_state(&self) {
        self.inner.empty_state.set_visible(false);
    }

    /// Whether this tab is in the empty state.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.state.is_empty()
    }

    /// Records the window's sidebar page and search panel fields as this tab's; see
    /// [`crate::persistence::StateHandle::record_sidebar`].
    pub fn record_sidebar(&self, page: SidebarPage, search: SearchFields) {
        self.inner.state.record_sidebar(page, search);
    }

    /// The sidebar page this tab records, which selecting it shows.
    #[must_use]
    pub fn sidebar_page(&self) -> SidebarPage {
        self.inner.state.sidebar_page()
    }

    /// The search panel fields this tab records.
    #[must_use]
    pub fn search_fields(&self) -> SearchFields {
        self.inner.state.search_fields()
    }

    /// Captures every pane's cursor and top line, or preview scroll; see
    /// [`crate::pane_view::PaneHandle::capture_viewports`].
    pub fn capture_viewports(&self) {
        self.inner.panes.capture_viewports();
    }

    /// Records the user's sidebar PREFERENCE for this tab.
    pub fn set_sidebar_shown(&self, shown: bool) {
        self.inner.state.set_sidebar_shown(shown);
    }

    /// The user's sidebar preference for this tab.
    #[must_use]
    pub fn sidebar_shown(&self) -> bool {
        self.inner.state.sidebar_shown()
    }

    /// The cwd this tab's core was launched with.
    #[must_use]
    pub fn launch_cwd(&self) -> PathBuf {
        self.inner.state.launch_cwd().to_path_buf()
    }

    /// This tab's live directory, `""` while it is empty; see
    /// [`crate::persistence::StateHandle::live_directory`].
    #[must_use]
    pub fn live_directory(&self) -> String {
        self.inner.state.live_directory()
    }

    /// Re-gathers this tab's persisted state.
    pub fn refresh_tab(&self) {
        self.inner.state.refresh_tab();
    }

    /// This tab's contribution to its window's entry; see
    /// [`crate::persistence::StateHandle::cached_tab`] for what a tab that has
    /// never gathered contributes.
    #[must_use]
    pub fn cached_tab(&self) -> Option<Tab> {
        self.inner.state.cached_tab()
    }

    /// Refreshes this tab from its own core: its panes, then its title.
    ///
    /// The title is compared before it is set, so a steady-state frame costs one
    /// string compare rather than a property notification on every wake — the same
    /// reason [`crate::shell::AppShell`] caches the window title.
    pub fn refresh(&self) {
        self.inner.panes.on_render_wake();
        // Own statement: the snapshot guard and the cell borrow drop before the widget call.
        let current_file = {
            let core_ref = self.inner.core.borrow();
            core_ref
                .as_ref()
                .and_then(|core| core.read_snapshot().current_file.clone())
        };
        let title = tabs_ui::tab_title(current_file.as_deref());
        if self.inner.page.title() != title {
            self.inner.page.set_title(&title);
        }
    }

    /// Shuts this tab's Neovim down: closes the wake channel, which ends the drain,
    /// then takes the core out and runs [`WolfCore::shutdown`]. Idempotent.
    ///
    /// The core is taken in a statement of its own, so the cell's borrow is released
    /// before `shutdown()` consumes the core and blocks on its teardown.
    pub fn shutdown(&self) {
        self.inner.wake.close();
        let taken = self.inner.core.borrow_mut().take();
        if let Some(core) = taken {
            if let Err(err) = core.shutdown() {
                eprintln!("wolf-gnome: core shutdown error: {err}");
            }
        }
    }
}

/// A callback the shell registers, cloned out of its `RefCell` before it is called.
type TabCallback = Rc<dyn Fn(&TabHandle)>;

/// The selection callback, with the tab the selection left, if it is still open.
type SelectedCallback = Rc<dyn Fn(Option<&TabHandle>)>;

/// The tab layer's shared state. Single GTK main thread, so `Cell` / `RefCell`
/// suffice.
struct TabsInner {
    /// The view holding every page.
    view: adw::TabView,
    /// The bar drawn above the `gtk::Paned`.
    bar: adw::TabBar,
    /// The cell metrics, handed to each new tab's pane view.
    metrics: Rc<CellMetrics>,
    /// The user configuration, for each new tab's appearance.
    config: WolfConfig,
    /// Every live tab, in INSERTION order — see [`TabsHandle::entries`], which is
    /// where the divergence from page order and its one affected consumer are stated.
    entries: RefCell<Vec<TabHandle>>,
    /// The id the next opened tab gets.
    next_id: Cell<u64>,
    /// Set by [`TabsHandle::shutdown_all`]: the window is closing, and no tab may
    /// open over it.
    closing: Cell<bool>,
    /// The tab whose Neovim was last told it holds the UI focus, or `None`.
    ui_focused: Cell<Option<TabId>>,
    /// The tab last shown, so a selection change can name the tab it left.
    shown: Cell<Option<TabId>>,
    /// The tabs, most recently selected first.
    recency: RefCell<TabRecency>,
    /// What to run after a real selection change, set by
    /// [`TabsHandle::connect_selected`]. An `Rc` rather than a `Box` so it can be
    /// cloned out of the `RefCell` before it is called — the callback re-enters this
    /// module through the overlays it closes.
    on_selected: RefCell<Option<SelectedCallback>>,
    /// The body of every tab's drain, set by [`TabsHandle::connect_wake`].
    on_wake: RefCell<Option<TabCallback>>,
    /// What runs before a tab's Neovim is shut down, set by
    /// [`TabsHandle::connect_closing`].
    on_closing: RefCell<Option<TabCallback>>,
}

impl TabsInner {
    /// The selected tab, or `None` before the first page exists.
    fn selected(&self) -> Option<TabHandle> {
        let page = self.view.selected_page()?;
        self.entry_for(&page)
    }

    /// The tab owning `page`, or `None` for a page already deregistered.
    fn entry_for(&self, page: &adw::TabPage) -> Option<TabHandle> {
        self.entries
            .borrow()
            .iter()
            .find(|entry| &entry.inner.page == page)
            .cloned()
    }

    /// Drops `page`'s entry from the registry. Idempotent.
    fn deregister(&self, page: &adw::TabPage) {
        self.entries
            .borrow_mut()
            .retain(|entry| &entry.inner.page != page);
    }

    /// Opens one tab at page `position`, appended when `None`; see [`TabsHandle::open_tab`].
    fn open_tab(self: &Rc<Self>, parts: TabParts, position: Option<i32>) -> Option<TabHandle> {
        if self.closing.get() {
            eprintln!("wolf-gnome: new tab not opened: the window is closing");
            return None;
        }

        // Created before the core, so the startup flush's wake is queued rather than lost.
        // See notes/wolf-gnome-decisions.md#threading-and-the-wake
        let (wake_tx, wake_rx) = async_channel::bounded::<()>(WAKE_CHANNEL_CAP);
        let wake: WakeFn = {
            let wake_tx = wake_tx.clone();
            Arc::new(move || {
                let _ = wake_tx.try_send(());
            })
        };
        let config = CoreConfig {
            init_lua: crate::app::init_lua_path(),
            cols: INITIAL_COLS,
            rows: INITIAL_ROWS,
            cwd: parts.cwd.clone(),
            terminfo_dir: wolf_core::terminfo_dir_from_env(),
        };
        let core = match WolfCore::start(config, wake) {
            Ok(core) => core,
            Err(err) => {
                eprintln!("wolf-gnome: failed to start core: {err}");
                return None;
            }
        };
        // Before anything else touches the core, so no terminal session starts without it.
        let wolf_tab_id = wolf_core::TabId::random();
        core.set_tab_id(wolf_tab_id);
        let core: CoreCell = Rc::new(RefCell::new(Some(core)));
        // Before any widget, so the tab's panes first paint under the resolved scheme.
        appearance::apply_at_launch(&self.config, &core);

        let panes = pane_view::build(&core, &self.metrics, parts.drawer_share);
        // Realized at this tab's first wake with a window grid, with no arity guard. A lone empty
        // edit pane rebuilds nothing but still takes the saved focus, zoom and drawer.
        if let Some(saved) = parts.saved.as_ref().filter(|_| !parts.empty) {
            panes.request_layout_rebuild(wolf_core::app_state::restored_tab(&saved.content));
        }

        // Stacking per `window::build`: empty state, message toast, completion popup. The transient and
        // crash overlays are window-overlay children, so they paint above all of these.
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&panes.widget()));
        let empty_state = shell::build_empty_state();
        overlay.add_overlay(&empty_state);
        overlay.add_overlay(&panes.toast_widget());
        overlay.add_overlay(&panes.pum_widget());

        let state = persistence::build(StateParts {
            core: &core,
            panes: &panes,
            launch_cwd: parts.cwd,
            empty: parts.empty,
            sidebar_shown: parts.sidebar_shown,
            saved: parts.saved,
            sidebar_page: parts.sidebar_page,
            search: parts.search,
        });

        let id = TabId(self.next_id.get());
        self.next_id.set(id.0 + 1);
        let page = match position {
            Some(position) => self.view.insert(&overlay, position),
            None => self.view.append(&overlay),
        };
        let entry = TabHandle {
            inner: Rc::new(TabInner {
                id,
                wolf_tab_id,
                view: self.view.clone(),
                page: page.clone(),
                core,
                wake: wake_tx,
                restart: TabRestart::default(),
                panes,
                state,
                empty_state,
            }),
        };
        // The real title arrives with this tab's first refresh; until then the bar
        // draws the fallback rather than libadwaita's empty string.
        page.set_title(tabs_ui::TAB_TITLE_FALLBACK);
        entry.set_empty(parts.empty);

        self.entries.borrow_mut().push(entry.clone());
        // Tab 0's `append` selected its page before it was registered, so no notify recorded it.
        if self.view.selected_page().as_ref() == Some(&page) {
            self.shown.set(Some(id));
        }
        spawn_drain(Rc::downgrade(self), entry.downgrade(), wake_rx);
        // AFTER registration. `append` already selects the FIRST page, so this is a
        // no-op for tab 0 and the switch for every later one.
        self.view.set_selected_page(&page);
        Some(entry)
    }

    /// Reacts to a REAL selection change: runs the shell's callback with the tab the
    /// selection left, when it is still registered, then parks focus on the newly
    /// shown tab's strip.
    ///
    /// `connect_selected_page_notify` fires only on an actual change, so nothing
    /// here needs a same-page guard. A closed tab is deregistered before its page
    /// goes, so the selection it causes names no tab it left.
    fn on_selected_changed(&self) {
        let Some(entry) = self.selected() else {
            return;
        };
        self.recency.borrow_mut().select(entry.id());
        let previous = self
            .shown
            .replace(Some(entry.id()))
            .filter(|id| *id != entry.id())
            .and_then(|id| {
                self.entries
                    .borrow()
                    .iter()
                    .find(|tab| tab.id() == id)
                    .cloned()
            });
        // Cloned out in its own statement: the callback closes overlays, whose
        // focus-return resolves back through this module.
        let callback = self.on_selected.borrow().clone();
        if let Some(callback) = callback {
            callback(previous.as_ref());
        }
        // AFTER the overlays close, so their own focus-return cannot land on top of
        // this one. The strip belongs to the page GTK has just mapped.
        entry.focus_area().grab_focus();
    }

    /// Answers the `close-page` VETO. See the module docs: every arm must call
    /// [`adw::TabView::close_page_finish`] and return [`glib::Propagation::Stop`].
    fn on_close_page(&self, page: &adw::TabPage) -> glib::Propagation {
        // Bound in its own statement: the arms below take `borrow_mut`.
        let live_pages = self.entries.borrow().len();
        match tabs_ui::close_outcome(live_pages) {
            TabClose::CloseTab => {
                let entry = self.entry_for(page);
                if let Some(entry) = entry.as_ref() {
                    // Before the shutdown, so a session still running on this core is dropped first.
                    let callback = self.on_closing.borrow().clone();
                    if let Some(callback) = callback {
                        callback(entry);
                    }
                }
                self.deregister(page);
                self.view.close_page_finish(page, true);
                if let Some(entry) = entry {
                    self.recency.borrow_mut().remove(entry.id());
                    entry.shutdown();
                }
                // The survivor's drain re-derives the title, the bars and the drawer divider.
                if let Some(selected) = self.selected() {
                    selected.wake();
                }
            }
            TabClose::CloseWindow => {
                // Refused, so the page survives until the toplevel tears it down.
                self.view.close_page_finish(page, false);
                // No core borrow is held: `close()` runs the `close-request` handler synchronously, and that
                // takes every tab's core out with a `borrow_mut`.
                match self
                    .view
                    .root()
                    .and_then(|root| root.downcast::<gtk::Window>().ok())
                {
                    Some(window) => window.close(),
                    None => eprintln!(
                        "wolf-gnome: last tab closed but the tab view has no toplevel to close; \
                         the tab keeps its captive Neovim"
                    ),
                }
            }
        }
        glib::Propagation::Stop
    }
}

/// Spawns one tab's drain: each received wake runs the [`TabsHandle::connect_wake`]
/// callback with that tab, until the channel closes.
///
/// Both handles are `Weak`: the tab layer owns the tab, and a strong reference from
/// a future the main loop owns would keep a closed tab's pane view alive.
fn spawn_drain(tabs: Weak<TabsInner>, tab: WeakTab, wake_rx: async_channel::Receiver<()>) {
    glib::spawn_future_local(async move {
        while wake_rx.recv().await.is_ok() {
            let (Some(tabs), Some(tab)) = (tabs.upgrade(), tab.upgrade()) else {
                break;
            };
            // Cloned out in its own statement: the callback re-enters this module.
            let callback = tabs.on_wake.borrow().clone();
            if let Some(callback) = callback {
                callback(&tab);
            }
        }
    });
}

/// A cloneable handle to the window's tab layer.
#[derive(Clone)]
pub struct TabsHandle {
    inner: Rc<TabsInner>,
}

/// Builds the [`adw::TabView`] + [`adw::TabBar`] pair and connects the two signals
/// that make the tab layer behave: the selection notify and the `close-page` veto.
///
/// No page exists yet: `window::build` opens the planned tabs next, and
/// [`TabsHandle::open_tab`] is callable from any point after this. It needs no
/// window of its own — [`StateParts`] carries none, and the size that reaches
/// the saved window state is read at commit time by
/// `window::WindowHandle::cached_window`.
#[must_use]
pub fn build(parts: TabsParts<'_>) -> TabsHandle {
    let view = adw::TabView::new();
    let bar = adw::TabBar::new();
    bar.set_view(Some(&view));
    // Left at libadwaita's default of `true`, and stated rather than assumed: see
    // the module docs on why a one-tab window's geometry must not move.
    bar.set_autohide(true);

    let handle = TabsHandle {
        inner: Rc::new(TabsInner {
            view: view.clone(),
            bar,
            metrics: parts.metrics.clone(),
            config: parts.config.clone(),
            entries: RefCell::new(Vec::new()),
            next_id: Cell::new(0),
            closing: Cell::new(false),
            ui_focused: Cell::new(None),
            shown: Cell::new(None),
            recency: RefCell::new(TabRecency::default()),
            on_selected: RefCell::new(None),
            on_wake: RefCell::new(None),
            on_closing: RefCell::new(None),
        }),
    };

    // Both handlers hold a `Weak`: the view is owned by `TabsInner`, so a strong reference from a
    // handler the view owns would leak every tab's pane view.
    {
        let weak = Rc::downgrade(&handle.inner);
        view.connect_selected_page_notify(move |_view| {
            if let Some(tabs) = upgrade(&weak) {
                tabs.inner.on_selected_changed();
            }
        });
    }
    {
        let weak = Rc::downgrade(&handle.inner);
        view.connect_close_page(move |view, page| {
            let Some(tabs) = upgrade(&weak) else {
                // Nothing left to decide with. Refuse rather than confirm: confirming would tear a page down
                // with its Neovim still running.
                view.close_page_finish(page, false);
                return glib::Propagation::Stop;
            };
            tabs.inner.on_close_page(page)
        });
    }

    handle
}

impl TabsHandle {
    /// The tab view, for placement as the window overlay's main child.
    #[must_use]
    pub fn view_widget(&self) -> gtk::Widget {
        self.inner.view.clone().upcast::<gtk::Widget>()
    }

    /// The tab bar, for placement between the header bar and the `gtk::Paned`.
    #[must_use]
    pub fn bar_widget(&self) -> gtk::Widget {
        self.inner.bar.clone().upcast::<gtk::Widget>()
    }

    /// Opens one tab: starts its own [`WolfCore`] in [`TabParts::cwd`], then builds
    /// a fresh pane view inside a fresh page, with its own empty-state overlay, its
    /// own persistence glue and its own wake drain, SELECTED.
    ///
    /// `None` — after one message — when the core fails to start, or when the
    /// window is closing ([`TabsHandle::shutdown_all`]).
    ///
    /// The entry is registered BEFORE the page is selected, so the selection notify
    /// this triggers finds the tab it is about to switch to rather than nothing.
    pub fn open_tab(&self, parts: TabParts) -> Option<TabHandle> {
        self.inner.open_tab(parts, None)
    }

    /// Replaces `old` with a tab opened from `parts` at `old`'s page position, SELECTED,
    /// then closes `old` through the ordinary `close-page` path, which shuts its
    /// Neovim down. `None`, with `old` untouched, when the new tab's core fails to start.
    ///
    /// The new tab opens FIRST: closing a window's last page closes the window.
    /// No core borrow may be held across this call, since the close takes `old`'s
    /// core out with a `borrow_mut`.
    pub fn replace(&self, old: &TabHandle, parts: TabParts) -> Option<TabHandle> {
        let opened = self.inner.open_tab(parts, Some(old.page_position()))?;
        self.inner.view.close_page(old.page());
        Some(opened)
    }

    /// Selects `tab`'s page, which fires the selection callback as a click would.
    pub fn select(&self, tab: &TabHandle) {
        self.inner.view.set_selected_page(tab.page());
    }

    /// Reseeds the tab recency with the selected tab first and the rest in page
    /// order, as a restored window's recency starts.
    pub fn seed_recency(&self) {
        let mut tabs: Vec<(i32, TabId)> = self
            .entries()
            .iter()
            .map(|tab| (tab.page_position(), tab.id()))
            .collect();
        tabs.sort_by_key(|(position, _)| *position);
        let ids: Vec<TabId> = tabs.into_iter().map(|(_, id)| id).collect();
        let selected = self.selected().map(|tab| tab.id());
        let active = ids.iter().position(|&id| Some(id) == selected).unwrap_or(0);
        *self.inner.recency.borrow_mut() = TabRecency::seeded(&ids, active);
    }

    /// The tabs, most recently selected first.
    #[must_use]
    pub fn recency(&self) -> TabRecency {
        self.inner.recency.borrow().clone()
    }

    /// The selected tab, or `None` before the first page exists.
    ///
    /// The FOCUS, overlay and core answer: which tab's cmdline strip an overlay
    /// returns focus to, which tab a window-level toggle applies to, and which tab's
    /// core a window-level widget acts on. It has nothing to do with persistence — a
    /// window persists EVERY tab, through [`TabHandle::cached_tab`], and reaches them
    /// by [`TabsHandle::entries`].
    #[must_use]
    pub fn selected(&self) -> Option<TabHandle> {
        self.inner.selected()
    }

    /// The selected tab's core, or `None` before the first page exists.
    #[must_use]
    pub fn selected_core(&self) -> Option<CoreCell> {
        self.selected().map(|tab| tab.core().clone())
    }

    /// [`TabsHandle::selected_core`] as a [`CoreSource`], for the window-level
    /// widgets. The `Weak` is upgraded per call, so a widget outliving the tab layer
    /// resolves no core rather than keeping it alive.
    #[must_use]
    pub fn core_source(&self) -> CoreSource {
        let weak = Rc::downgrade(&self.inner);
        Rc::new(move || upgrade(&weak).and_then(|tabs| tabs.selected_core()))
    }

    /// The [`FileOpener`] for the window-level widgets. With the drawer active, Neovim's
    /// current window is the drawer's, which an `InputEvent::Open` would edit in, so the file
    /// goes where `specs/cli-and-file-open.md` §5.2 places it instead. The `Weak` is upgraded
    /// per call, as [`TabsHandle::core_source`]'s is.
    #[must_use]
    pub fn drawer_opener(&self) -> FileOpener {
        let weak = Rc::downgrade(&self.inner);
        Rc::new(move |path, at| {
            let Some(tab) = upgrade(&weak).and_then(|tabs| tabs.selected()) else {
                return false;
            };
            if !tab.panes().drawer_is_active() {
                return false;
            }
            tab.panes().open_picked(path.to_path_buf(), at);
            tab.wake();
            true
        })
    }

    /// Every live tab, in INSERTION order, for the shell's per-tab fan-outs.
    ///
    /// A `Vec` [`TabsInner::open_tab`] pushes each new page onto, with nothing
    /// reconciling it against [`adw::TabView`]'s page positions — so it diverges from
    /// the bar the first time the user reorders a tab. Most callers are
    /// order-insensitive fan-outs (wake each tab, count them), so this is left alone
    /// deliberately rather than sorted for the others.
    ///
    /// The callers that need the user's arrangement pair each entry with
    /// [`TabHandle::page_position`] and sort: `window::WindowHandle::cached_window`,
    /// through [`crate::state_seam::ordered_tab_states`], because
    /// [`crate::state_seam`]'s `restore_plans` replays a saved window's tabs
    /// positionally, and [`TabsHandle::seed_recency`].
    #[must_use]
    pub fn entries(&self) -> Vec<TabHandle> {
        self.inner.entries.borrow().clone()
    }

    /// How many tabs are live.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.entries.borrow().len()
    }

    /// Whether no tab is live, which is only true before [`TabsHandle::open_tab`]
    /// has ever run. Present because `clippy::len_without_is_empty` requires it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.entries.borrow().is_empty()
    }

    /// Resolves the focus-return target for the three transient overlays and the
    /// sidebar's two pages: the SELECTED tab's cmdline strip, read at CLOSE time.
    ///
    /// See [`FocusTarget`] for why this is a closure. The `Weak` is upgraded per
    /// call, so an overlay outliving the tab layer grabs nothing rather than
    /// panicking.
    #[must_use]
    pub fn focus_target(&self) -> FocusTarget {
        let weak = Rc::downgrade(&self.inner);
        Rc::new(move || {
            upgrade(&weak)
                .and_then(|tabs| tabs.selected())
                .map(|entry| entry.focus_area())
        })
    }

    /// Registers what runs after a real selection change, with the tab the selection
    /// left when it is still open — the shell records the window's sidebar into that
    /// tab and captures its panes, closes its transient overlays and wakes the newly
    /// selected tab there.
    pub fn connect_selected(&self, f: impl Fn(Option<&TabHandle>) + 'static) {
        *self.inner.on_selected.borrow_mut() = Some(Rc::new(f));
    }

    /// Registers the body of every tab's drain: what runs, with that tab, for each
    /// wake its core fires. A wake that arrives before this is set is dropped, which
    /// is why `window::build` wakes every tab once after registering it.
    pub fn connect_wake(&self, f: impl Fn(&TabHandle) + 'static) {
        *self.inner.on_wake.borrow_mut() = Some(Rc::new(f));
    }

    /// Registers what runs when one tab closes, before its Neovim is shut down —
    /// the shell drops the sidebar sessions running on that tab's core there.
    pub fn connect_closing(&self, f: impl Fn(&TabHandle) + 'static) {
        *self.inner.on_closing.borrow_mut() = Some(Rc::new(f));
    }

    /// The terminal session the selected tab's active pane shows, or `None` for an editor
    /// pane or no tab: where the window's keys, committed text, Copy and Paste go.
    #[must_use]
    pub fn active_terminal(&self) -> Option<TerminalId> {
        self.selected()
            .and_then(|entry| entry.panes().active_terminal())
    }

    /// Reports terminal focus for every tab: the selected tab's active session holds it while
    /// `window_active`, and no other session does. The other tabs report first, so a tab
    /// switch sends the loss before the gain.
    pub fn sync_terminal_focus(&self, window_active: bool) {
        let entries = self.entries();
        for entry in entries.iter().filter(|entry| !entry.is_selected()) {
            entry.panes().sync_terminal_focus(false);
        }
        for entry in entries.iter().filter(|entry| entry.is_selected()) {
            entry.panes().sync_terminal_focus(window_active);
        }
    }

    /// Reports the UI focus to the tabs' Neovims: the selected tab's holds it while
    /// `window_active`, and no other does ([`crate::tabs_ui::focused_tab`]). Each
    /// edge [`crate::tabs_ui::focus_edges`] answers is sent to its tab's core, so a
    /// tab gaining the focus runs `:checktime`; an edge for a closed tab is skipped.
    pub fn sync_ui_focus(&self, window_active: bool) {
        let selected = self.selected().map(|tab| tab.id());
        let next = tabs_ui::focused_tab(window_active, selected);
        let previous = self.inner.ui_focused.get();
        let entries = self.entries();
        for edge in tabs_ui::focus_edges(previous, next) {
            let Some(tab) = entries.iter().find(|tab| tab.id() == edge.tab) else {
                continue;
            };
            if let Some(core) = tab.core().borrow().as_ref() {
                core.set_ui_focus(edge.gained);
            }
        }
        self.inner.ui_focused.set(next);
    }

    /// Shuts every tab's Neovim down, for the window's close handler, and refuses
    /// every later [`TabsHandle::open_tab`].
    pub fn shutdown_all(&self) {
        self.inner.closing.set(true);
        for entry in self.entries() {
            entry.shutdown();
        }
    }
}

/// Upgrades a signal handler's `Weak<TabsInner>` back into a [`TabsHandle`].
///
/// Every closure this module installs holds a `Weak`, never an `Rc`; see [`build`].
fn upgrade(weak: &Weak<TabsInner>) -> Option<TabsHandle> {
    weak.upgrade().map(|inner| TabsHandle { inner })
}

#[cfg(test)]
mod tests {
    use super::WAKE_CHANNEL_CAP;

    // Pins the transport half of the wake bridge's safety argument against an `async-channel` change.
    // See notes/crates/wolf-gnome/src/tab_view.rs.md#wake-bridge-tests

    #[test]
    fn a_dropped_wake_implies_a_pending_wake() {
        let (wake_tx, wake_rx) = async_channel::bounded::<()>(WAKE_CHANNEL_CAP);

        for slot in 0..WAKE_CHANNEL_CAP {
            assert!(
                wake_tx.try_send(()).is_ok(),
                "filling the wake channel: send {slot} of {WAKE_CHANNEL_CAP} must be accepted, \
                 or the capacity is not what the drain loop was designed against"
            );
        }

        let overflow = wake_tx.try_send(());
        assert!(
            // Matched through a reference so `overflow` survives for the message.
            matches!(&overflow, Err(async_channel::TrySendError::Full(_))),
            "a send into a FULL wake channel must be refused with Full (the receiver is still \
             alive here, so a Closed would mean something else entirely); got {overflow:?}"
        );

        // The load-bearing implication: the wake that was just dropped left a
        // queued one behind it, so a handler run is still guaranteed to follow.
        assert!(
            wake_rx.try_recv().is_ok(),
            "a DROPPED wake must imply a PENDING wake: the send above was refused because the \
             queue was full, so an unconsumed item must still be there for the drain loop to \
             pop. If this ever fails, a death wake can be lost with no handler run after it, and \
             the crash overlay needs a periodic liveness poll to appear at all."
        );
    }

    #[test]
    fn consuming_a_wake_frees_the_slot_before_the_handler_runs() {
        let (wake_tx, wake_rx) = async_channel::bounded::<()>(WAKE_CHANNEL_CAP);

        for slot in 0..WAKE_CHANNEL_CAP {
            assert!(
                wake_tx.try_send(()).is_ok(),
                "filling the wake channel: send {slot} of {WAKE_CHANNEL_CAP} must be accepted"
            );
        }
        assert!(
            wake_tx.try_send(()).is_err(),
            "precondition: the wake channel must be full before the receive below can show \
             anything"
        );

        assert!(
            wake_rx.try_recv().is_ok(),
            "the queued wake must be receivable"
        );

        // `recv()` resolves over `try_recv`, popping the item before the future resolves, so a wake fired
        // during a handler run (the `mark_dead` case) is accepted, not dropped.
        assert!(
            wake_tx.try_send(()).is_ok(),
            "consuming a wake must free its slot BEFORE the handlers run: a death wake fired \
             while the drain loop is mid-handler must be accepted, not dropped"
        );
    }

    #[test]
    fn wake_channel_capacity_is_at_least_one() {
        assert_ne!(
            WAKE_CHANNEL_CAP, 0,
            "the wake channel capacity must be at least 1: async_channel::bounded panics on a \
             capacity of 0, and a zero-capacity channel would drop every wake fired while the \
             drain loop was not already parked in recv()"
        );

        let (wake_tx, wake_rx) = async_channel::bounded::<()>(WAKE_CHANNEL_CAP);
        assert!(
            wake_tx.try_send(()).is_ok(),
            "at least one wake must fit in the channel while nothing is draining it — that is \
             what makes a wake fired from a tokio worker reach the GTK main loop at all"
        );
        assert!(
            wake_rx.try_recv().is_ok(),
            "and the drain loop must be able to pop it"
        );
    }
}
