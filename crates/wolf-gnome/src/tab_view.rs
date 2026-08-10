//! The GTK GLUE for NATIVE TABS: an [`adw::TabBar`] over an [`adw::TabView`],
//! carrying one [`crate::pane_view::PaneHandle`] per tab over ONE shared
//! [`WolfCore`].
//!
//! Every DECISION this module acts on is [`mod@crate::tabs_ui`]'s — which tab a
//! wake may touch, what closing one does, whether a New Tab press may issue its
//! blocking RPC, the directory a new tab inherits, and the string a tab draws. This
//! file is the widget work of carrying those answers out, in the same relationship
//! [`crate::persistence`] has to [`crate::state_seam`].
//!
//! ## What is per TAB and what is per WINDOW
//!
//! Per tab: the pane view (its `DrawingArea`s, its cmdline strip, its toast and its
//! completion popup), the tab's own empty-state page, and its
//! [`crate::persistence::StateHandle`].
//!
//! Per window, and deliberately NOT moved in here: the header bar, the `win.`
//! action namespace, the four transient overlays, the crash/stall overlay — which
//! is a property of the CORE, and one core serves every tab — and **the file-tree
//! sidebar**. The sidebar's placement is a measured constraint rather than a
//! preference: moving it into a page forces the [`adw::TabView`] above the window's
//! `gtk::Paned`, which widens the window-level `gtk::Overlay` from the pane area to
//! the whole window and moves that overlay's measured left edge, which the crash
//! smoke's button click and the picker/preview overlap band are both derived from.
//! `DECISIONS.md` carries the follow-up for the unit that gives tabs independent
//! working directories.
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
//! close, because `close_tabpage` on Neovim's last tabpage is a measured no-op —
//! see [`crate::tabs_ui::close_outcome`].
//!
//! ## The restart collapse
//!
//! A respawned Neovim owns exactly one tabpage, so every tab's stored handle is
//! stale and [`crate::tabs_ui::tab_wake`] answers `Hold` for all of them — a window
//! frozen forever over a perfectly healthy Neovim. [`TabsHandle::reset_after_restart`]
//! collapses to the selected tab and clears its handle, and the next wake's
//! `Adopt` row re-binds it from `Snapshot::current_tabpage`. That is a SNAPSHOT
//! read: the restart path issues no blocking RPC, which `DECISIONS.md` requires and
//! which is why the tab handles are not re-created here.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::glib;

use wolf_core::{PaneNode, WindowState, WolfCore};

use crate::pane_view::{self, PaneHandle};
use crate::persistence::{self, StateHandle, StateParts};
use crate::picker::FocusTarget;
use crate::shell;
use crate::tabs_ui::{self, TabClose, TabWake};
use crate::CellMetrics;

/// What [`build`] needs, as one struct rather than positional parameters (the
/// [`StateParts`] / [`crate::shell::ShellParts`] precedent).
pub struct TabsParts<'a> {
    /// The window's ONE live core. Every tab shares it; that sharing is the whole
    /// point of a tab as against a window.
    pub core: &'a Rc<RefCell<Option<WolfCore>>>,
    /// The cell metrics, measured once for the process and handed to each tab's
    /// pane view.
    pub metrics: &'a Rc<CellMetrics>,
}

/// What [`TabsHandle::open_tab`] needs for ONE tab, as one struct rather than six
/// positional parameters (`clippy::too_many_arguments` bites at seven).
pub struct TabParts {
    /// The Neovim tabpage this tab is bound to, or `0` to bind on the first wake
    /// through [`crate::tabs_ui::tab_wake`]'s `Adopt` row.
    ///
    /// **Tab 0 passes `0` and a later tab passes `create_tabpage`'s answer, and the
    /// asymmetry is not cosmetic.** `Snapshot::current_tabpage` still reports the
    /// PREVIOUS tabpage for the wakes between the RPC returning and Neovim's
    /// `wolf_tabpage` notification landing, so a new page left to adopt would bind
    /// itself to the tab it was opened FROM — two pages claiming one tabpage.
    pub tabpage: i64,
    /// The directory this tab's persisted subtree is seeded with until Neovim
    /// reports a live one. [`crate::tabs_ui::new_tab_cwd`] decides it for a new tab;
    /// tab 0 gets the window's launch cwd.
    pub cwd: PathBuf,
    /// Whether the window is in the empty state, which hides this tab's panes
    /// behind its own status page.
    pub empty: bool,
    /// The user's sidebar PREFERENCE, which is what this tab persists.
    pub sidebar_shown: bool,
    /// A saved pane tree to realize on the first wake with a window grid. `None`
    /// for every tab a user opens; only a restored tab 0 carries one.
    pub layout: Option<PaneNode>,
}

/// One tab's shared state.
struct TabInner {
    /// The view this page belongs to, so a tab can answer whether it is the
    /// selected one without the registry above it.
    view: adw::TabView,
    /// This tab's page.
    page: adw::TabPage,
    /// This tab's pane view — its `DrawingArea`s, cmdline strip, toast and popup.
    panes: PaneHandle,
    /// This tab's contribution to `state.json`. See [`TabsHandle::selected`] on why
    /// only one of them is ever read.
    state: StateHandle,
    /// This tab's empty-state page, an overlay child above its panes.
    empty_state: gtk::Widget,
    /// The Neovim tabpage handle this tab is bound to, `0` until adopted.
    tabpage: Cell<i64>,
}

/// A cloneable handle to ONE tab.
#[derive(Clone)]
pub struct TabHandle {
    inner: Rc<TabInner>,
}

impl TabHandle {
    /// This tab's page, for the [`adw::TabView`] calls that name one.
    #[must_use]
    pub fn page(&self) -> &adw::TabPage {
        &self.inner.page
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

    /// The Neovim tabpage handle this tab is bound to, `0` until adopted.
    #[must_use]
    pub fn tabpage(&self) -> i64 {
        self.inner.tabpage.get()
    }

    /// Binds this tab to a Neovim tabpage handle.
    pub fn set_tabpage(&self, handle: i64) {
        self.inner.tabpage.set(handle);
    }

    /// Whether GTK currently shows this tab.
    #[must_use]
    pub fn is_selected(&self) -> bool {
        self.inner.view.selected_page().as_ref() == Some(&self.inner.page)
    }

    /// Shows or hides this tab's empty state and keeps its persisted `cwd` in step.
    ///
    /// The widget half is per tab because the empty state is opaque and full-bleed
    /// over a tab's own panes; the sidebar, the header buttons and the `win.`
    /// actions the empty state also drives are per window and stay in
    /// [`crate::shell::AppShell::set_empty_state`].
    pub fn set_empty(&self, empty: bool) {
        self.inner.empty_state.set_visible(empty);
        self.inner.state.set_empty(empty);
    }

    /// Records the user's sidebar PREFERENCE for this tab's persisted subtree.
    pub fn set_sidebar_shown(&self, shown: bool) {
        self.inner.state.set_sidebar_shown(shown);
    }

    /// Re-gathers this tab's persisted subtree.
    pub fn refresh_subtree(&self) {
        self.inner.state.refresh_subtree();
    }

    /// This tab's last successfully gathered subtree.
    #[must_use]
    pub fn cached_subtree(&self) -> Option<WindowState> {
        self.inner.state.cached_subtree()
    }

    /// Refreshes this tab: its panes, then its title. Called ONLY for a
    /// [`TabWake::Refresh`] or [`TabWake::Adopt`]; see [`mod@crate::tabs_ui`] on why
    /// a held tab must not reach here.
    ///
    /// The title is compared before it is set, so a steady-state frame costs one
    /// string compare rather than a property notification on every wake — the same
    /// reason [`crate::shell::AppShell::refresh_title`] caches the window title.
    fn refresh(&self, current_file: Option<&std::path::Path>) {
        self.inner.panes.on_render_wake();
        let title = tabs_ui::tab_title(current_file);
        if self.inner.page.title() != title {
            self.inner.page.set_title(&title);
        }
    }
}

/// The tab layer's shared state. Single GTK main thread, so `Cell` / `RefCell`
/// suffice.
struct TabsInner {
    /// The view holding every page.
    view: adw::TabView,
    /// The bar drawn above the `gtk::Paned`.
    bar: adw::TabBar,
    /// The window's ONE core, shared with every tab.
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The cell metrics, handed to each new tab's pane view.
    metrics: Rc<CellMetrics>,
    /// Every live tab, in page order.
    entries: RefCell<Vec<TabHandle>>,
    /// Whether [`TabsHandle::reset_after_restart`] is collapsing the window.
    ///
    /// The close handler reads it and takes a different arm: a restart closes its
    /// own pages, and telling a FRESH Neovim to close a tabpage handle that belonged
    /// to the dead one would close a live tab at random.
    restarting: Cell<bool>,
    /// What to run after a real selection change, set by
    /// [`TabsHandle::connect_selected`]. An `Rc` rather than a `Box` so it can be
    /// cloned out of the `RefCell` before it is called — the callback re-enters this
    /// module through the overlays it closes.
    on_selected: RefCell<Option<Rc<dyn Fn()>>>,
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

    /// Reacts to a REAL selection change: tells Neovim to switch tabpage, closes the
    /// four transient overlays, and parks focus on the newly shown tab's strip.
    ///
    /// `connect_selected_page_notify` fires only on an actual change, so nothing
    /// here needs a same-page guard.
    fn on_selected_changed(&self) {
        let Some(entry) = self.selected() else {
            return;
        };
        // Fire-and-forget on the ordered input channel, so it can never cross the
        // `close_tabpage` of a page closed a moment earlier.
        if let Some(handle) = tabs_ui::switch_target(entry.tabpage()) {
            if let Some(core) = self.core.borrow().as_ref() {
                core.set_current_tabpage(handle);
            }
        }
        // Cloned out in its own statement: the callback closes overlays, whose
        // focus-return resolves back through this module.
        let callback = self.on_selected.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
        // AFTER the overlays close, so their own focus-return cannot land on top of
        // this one. The strip belongs to the page GTK has just mapped.
        entry.focus_area().grab_focus();
    }

    /// Answers the `close-page` VETO. See the module docs: every arm must call
    /// [`adw::TabView::close_page_finish`] and return [`glib::Propagation::Stop`].
    fn on_close_page(&self, page: &adw::TabPage) -> glib::Propagation {
        if self.restarting.get() {
            self.deregister(page);
            self.view.close_page_finish(page, true);
            return glib::Propagation::Stop;
        }
        // Bound in its own statement: the arms below take `borrow_mut`.
        let live_pages = self.entries.borrow().len();
        match tabs_ui::close_outcome(live_pages) {
            TabClose::CloseTabpage => {
                if let Some(entry) = self.entry_for(page) {
                    if let Some(handle) = tabs_ui::switch_target(entry.tabpage()) {
                        if let Some(core) = self.core.borrow().as_ref() {
                            core.close_tabpage(handle);
                        }
                    }
                }
                // Synchronously, without waiting for Neovim's `TabClosed`: nothing
                // in this unit reads anything about the dead handle afterwards, and
                // the survivor's gate answers `Hold` for exactly the wakes between
                // here and the next `wolf_tabpage` notification.
                self.deregister(page);
                self.view.close_page_finish(page, true);
            }
            TabClose::CloseWindow => {
                // The veto is answered with a REFUSAL, so the page survives until
                // the toplevel tears it down. `close_tabpage` is deliberately NOT
                // called: on the last tabpage it is a measured no-op, which would
                // leave a live Neovim behind a page that quietly refuses to close.
                self.view.close_page_finish(page, false);
                // No core borrow is held here: `close()` runs the window's
                // `close-request` handler synchronously, and that takes the core
                // out with a `borrow_mut`.
                match self
                    .view
                    .root()
                    .and_then(|root| root.downcast::<gtk::Window>().ok())
                {
                    Some(window) => window.close(),
                    None => eprintln!(
                        "wolf-gnome: last tab closed but the tab view has no toplevel to close; \
                         the window keeps its captive Neovim"
                    ),
                }
            }
        }
        glib::Propagation::Stop
    }
}

/// A cloneable handle to the window's tab layer.
#[derive(Clone)]
pub struct TabsHandle {
    inner: Rc<TabsInner>,
}

/// Builds the [`adw::TabView`] + [`adw::TabBar`] pair and connects the two signals
/// that make the tab layer behave: the selection notify and the `close-page` veto.
///
/// No page exists yet — call [`TabsHandle::open_tab`] once the window exists (the
/// persistence glue reads the window's size).
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
            core: parts.core.clone(),
            metrics: parts.metrics.clone(),
            entries: RefCell::new(Vec::new()),
            restarting: Cell::new(false),
            on_selected: RefCell::new(None),
        }),
    };

    // Both handlers hold a `Weak` and upgrade per use: the view is owned by
    // `TabsInner`, and a strong reference from a handler the view owns would close
    // that cycle and leak every tab's pane view.
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
                // Nothing left to decide with. REFUSE rather than confirm: a
                // confirmed close here would tear a page down with its tabpage
                // still live inside Neovim.
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

    /// Opens one tab: a fresh pane view inside a fresh page, with its own
    /// empty-state overlay and its own persistence glue, SELECTED.
    ///
    /// `false` — after one message — when the window has no core left to open a tab
    /// over, which is what a close handler that has already taken it out leaves
    /// behind.
    ///
    /// The entry is registered BEFORE the page is selected, so the selection notify
    /// this triggers finds the tab it is about to switch to rather than nothing.
    #[must_use]
    pub fn open_tab(&self, window: &adw::ApplicationWindow, parts: TabParts) -> bool {
        if self.inner.core.borrow().is_none() {
            eprintln!(
                "wolf-gnome: new tab not opened: the window's core has already been taken out by \
                 its close handler, so a tab would have no Neovim behind it"
            );
            return false;
        }

        let panes = pane_view::build(&self.inner.core, &self.inner.metrics);
        // Realized on the first wake at which a window grid exists. NO arity guard,
        // for `window::build`'s recorded reason: a single-leaf layout is rebuilt
        // exactly like a multi-leaf one.
        if let Some(layout) = parts.layout {
            panes.request_layout_rebuild(layout);
        }

        // The page's own overlay stack, in the order `window::build` documents for
        // the three that moved in here: the empty state, then the message toast,
        // then the completion popup. The four transient overlays and the crash
        // overlay are the WINDOW overlay's children and therefore paint above every
        // one of these, which is the same GTK stacking rule the preview relies on.
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&panes.widget()));
        let empty_state = shell::build_empty_state();
        overlay.add_overlay(&empty_state);
        overlay.add_overlay(&panes.toast_widget());
        overlay.add_overlay(&panes.pum_widget());

        let state = persistence::build(StateParts {
            core: &self.inner.core,
            window,
            panes: &panes,
            launch_cwd: parts.cwd,
            empty: parts.empty,
            sidebar_shown: parts.sidebar_shown,
        });

        let page = self.inner.view.append(&overlay);
        let entry = TabHandle {
            inner: Rc::new(TabInner {
                view: self.inner.view.clone(),
                page: page.clone(),
                panes,
                state,
                empty_state,
                tabpage: Cell::new(parts.tabpage),
            }),
        };
        // The real title arrives with the first wake this tab is not held for; until
        // then the bar draws the fallback rather than libadwaita's empty string.
        page.set_title(tabs_ui::TAB_TITLE_FALLBACK);
        entry.set_empty(parts.empty);

        self.inner.entries.borrow_mut().push(entry);
        // AFTER registration. `append` already selects the FIRST page, so this is a
        // no-op for tab 0 and the switch for every later one.
        self.inner.view.set_selected_page(&page);
        true
    }

    /// The selected tab, or `None` before the first page exists.
    ///
    /// **This is the whole of what a window persists, and that is FROZEN here on
    /// purpose.** A window contributes its selected tab's subtree and nothing else,
    /// so a two-tab session comes back as one tab. Every other tab's
    /// [`StateHandle`] is built and kept in step but never read. Do NOT "finish"
    /// this by walking [`TabsHandle::entries`] here: a background tab is never
    /// refreshed (see [`mod@crate::tabs_ui`]), so its `WolfCore::pane_status` would
    /// answer `[No Name]` with an EMPTY absolute path for every leaf, and the result
    /// would be a well-formed `state.json` that has silently lost the user's files.
    /// Persisting N tabs needs the background-tab read solved first, and
    /// `DECISIONS.md` carries that as its own unit.
    #[must_use]
    pub fn selected(&self) -> Option<TabHandle> {
        self.inner.selected()
    }

    /// Every live tab, in page order, for the shell's per-tab fan-outs.
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

    /// Resolves the focus-return target for the four transient overlays and the
    /// sidebar: the SELECTED tab's cmdline strip, read at CLOSE time.
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

    /// Registers what runs after a real selection change — the shell closes its four
    /// transient overlays there, so an overlay opened over one tab does not hang
    /// over the next.
    pub fn connect_selected(&self, f: impl Fn() + 'static) {
        *self.inner.on_selected.borrow_mut() = Some(Rc::new(f));
    }

    /// Drives ONE render wake through [`crate::tabs_ui::tab_wake`], per tab.
    ///
    /// EVERY tab is driven and the rule decides, rather than the window driving only
    /// the selected page: that keeps the gate the single authority and keeps it
    /// reachable from a table.
    ///
    /// The snapshot read guard is bound to a local and dropped BEFORE any pane call,
    /// because [`crate::pane_view`] takes its own — the borrow discipline
    /// `DECISIONS.md` records. `Snapshot::current_tabpage` is a plain field, never
    /// `WolfCore::current_tabpage()`, which is a blocking RPC.
    pub fn on_render_wake(&self) {
        let (current, current_file) = {
            let core_ref = self.inner.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            let snapshot = core.read_snapshot();
            (snapshot.current_tabpage, snapshot.current_file.clone())
        };

        // Cloned out so the registry is not borrowed while a pane refresh runs.
        for entry in self.entries() {
            match tabs_ui::tab_wake(current, entry.tabpage(), entry.is_selected()) {
                TabWake::Adopt(handle) => {
                    entry.set_tabpage(handle);
                    entry.refresh(current_file.as_deref());
                }
                TabWake::Refresh => entry.refresh(current_file.as_deref()),
                // Nothing at all. This is the gate; see `tabs_ui`'s module docs.
                TabWake::Hold => {}
            }
        }
    }

    /// Collapses the window to its selected tab after a SUCCESSFUL in-place Neovim
    /// restart, and clears that tab's stale handle.
    ///
    /// See the module docs on why doing nothing here freezes the window over a
    /// healthy Neovim. The `restarting` flag routes every close through the arm that
    /// does NOT tell the fresh Neovim to close a dead one's tabpage handle.
    pub fn reset_after_restart(&self) {
        self.inner.restarting.set(true);
        let survivor = self.selected();
        for entry in self.entries() {
            if !entry.is_selected() {
                self.inner.view.close_page(entry.page());
            }
        }
        if let Some(entry) = survivor {
            entry.panes().reset_after_restart();
            // Re-bound from `Snapshot::current_tabpage` by the next wake's `Adopt`
            // row — a snapshot read, not an RPC.
            entry.set_tabpage(0);
        }
        self.inner.restarting.set(false);
    }
}

/// Upgrades a signal handler's `Weak<TabsInner>` back into a [`TabsHandle`].
///
/// Every closure this module installs holds a `Weak`, never an `Rc`; see [`build`].
fn upgrade(weak: &Weak<TabsInner>) -> Option<TabsHandle> {
    weak.upgrade().map(|inner| TabsHandle { inner })
}
