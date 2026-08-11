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
//! ## The restore materialization
//!
//! A launch restores every saved tab, not just the first.
//! `window::build` opens tab 0 with its layout queued the way it always was, then
//! hands the remaining layouts to [`TabsHandle::queue_restore`]. The queue is drained
//! on a later render wake by [`TabsInner::materialize_restore`], gated by
//! [`crate::tabs_ui::restore_press`] — no timer and no polling, because it rides the
//! wake that already exists.
//!
//! For each queued layout, in saved order: one blocking `create_tabpage` (which
//! ENTERS the new tabpage), then [`TabsHandle::open_tab`], then — while that tabpage
//! is still current — [`crate::pane_view::PaneHandle::realize_layout`]. Realizing
//! inside the loop is what makes the sequence deterministic: `rebuild_layout` acts on
//! whatever tabpage is current, and the instant after `create_tabpage` returns is the
//! only moment this front-end knows for certain which one that is. `realize_layout`
//! DROPS the copy `open_tab` queued on the same pane view, which is what holds each
//! materialized tab to one rebuild: left queued, the first wake that tab is not held
//! for would consume it and put a second blocking `rebuild_layout` on the SELECTION
//! path, which no blocking-call inventory lists. The queue is
//! TAKEN, so this runs exactly once per window per launch.
//!
//! A queued tab is contributed to `state.json` before it exists, through
//! [`TabsHandle::pending_restore`] and [`crate::state_seam::pending_tab_states`]:
//! it has no page, so no cache and no live gather can speak for it, and the commit
//! `window::build` drives does not wait for this materialization.
//!
//! An ABORT — `create_tabpage` answering `0`, which an RPC error or a timeout
//! produces — stops the loop and re-seats the layouts it never reached. The
//! remainder is retained as a seed source and creation is never retried, so those
//! tabs go on being written to `state.json` while no further RPC is issued for them;
//! see [`RestoreTabs`].
//!
//! ## The restart collapse
//!
//! A respawned Neovim owns exactly one tabpage, so every tab's stored handle is
//! stale and [`crate::tabs_ui::tab_wake`] answers `Hold` for all of them — a window
//! frozen forever over a perfectly healthy Neovim. [`TabsHandle::reset_after_restart`]
//! collapses to the selected tab, clears its handle and DISCARDS any pending restore,
//! and the next wake's `Adopt` row re-binds it from `Snapshot::current_tabpage`. That
//! is a SNAPSHOT read: the restart path issues no blocking RPC, which `DECISIONS.md`
//! requires and which is why the tab handles are not re-created here — and why the
//! queue is dropped rather than replayed, since replaying it is `2T-1` blocking calls
//! on exactly the path that forbids them.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::glib;

use wolf_core::{PaneNode, TabState, WolfCore};

use crate::pane_view::{self, PaneHandle};
use crate::persistence::{self, StateHandle, StateParts};
use crate::picker::FocusTarget;
use crate::shell;
use crate::tabs_ui::{self, RestorePress, TabClose, TabWake};
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
    /// The pane tree this tab was RESTORED with, or `None` for a tab the user
    /// opened.
    ///
    /// Two consumers, and both are needed. The pane view realizes it — tab 0
    /// through [`crate::pane_view::PaneHandle::request_layout_rebuild`] on the
    /// first wake with a window grid, a materialized tab through
    /// [`crate::pane_view::PaneHandle::realize_layout`] at once. And
    /// [`crate::persistence`] keeps it as the SEED this tab contributes until its
    /// own first gather succeeds — which for a tab whose realization bound no pane
    /// is never. A tab whose panes DID bind is gathered live from its first wake,
    /// selected or not.
    pub layout: Option<PaneNode>,
}

/// The tabs a launch still has to materialize, handed over once by `window::build`.
///
/// Held rather than acted on immediately because the materialization needs tab 0 to
/// have adopted its tabpage handle first; see [`crate::tabs_ui::restore_press`].
///
/// It is also what a window CONTRIBUTES for those tabs in the meantime:
/// [`TabsHandle::pending_restore`] hands these fields to
/// [`crate::state_seam::pending_tab_states`], because a tab with no page yet has
/// neither a cache nor a live gather to speak for it.
///
/// ## The two jobs, and the abort that separates them
///
/// A queue is both "layouts still owed a page" and "creation may still be
/// attempted". When [`TabsInner::create_one_restored_tab`] aborts, the second stops
/// being true and the first does not: [`TabsInner::materialize_restore`] re-seats the
/// layouts the loop never reached, with `may_create` cleared. The remainder is
/// RETAINED AS A SEED SOURCE and creation is NEVER RETRIED, so an abandoned restore
/// keeps speaking for its un-created tabs in `state.json` forever while issuing no
/// further RPC.
#[derive(Clone)]
pub struct RestoreTabs {
    /// The window's launch cwd, for [`crate::tabs_ui::new_tab_cwd`] and for each
    /// restored tab's persisted seed.
    pub launch_cwd: PathBuf,
    /// Whether the window launched into the empty state.
    pub empty: bool,
    /// The user's sidebar PREFERENCE, fanned to every restored tab.
    ///
    /// Tracked live by [`TabsHandle::set_pending_sidebar_shown`] rather than frozen
    /// at queue time: a toggle before materialization would otherwise reach neither
    /// these tabs' seeds nor the `StateHandle` each of them is later opened with.
    pub sidebar_shown: bool,
    /// One entry per queued tab, in SAVED ORDER — the order they are created in,
    /// which is the order the tab bar draws and the next save writes back.
    pub layouts: Vec<Option<PaneNode>>,
    /// Whether `create_tabpage` may still be attempted for these layouts.
    ///
    /// [`TabsHandle::queue_restore`] sets it, so a launch's queue is always `true`;
    /// the ONE producer of `false` is the remainder an abort re-seats. It is
    /// [`crate::tabs_ui::restore_press`]'s input and nothing else reads it — the
    /// seeds a retained queue contributes are the same either way.
    pub may_create: bool,
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
    /// This tab's own contribution to `state.json`. Every tab's is read by
    /// `window::WindowHandle::cached_subtree`, which orders them by
    /// [`TabHandle::page_position`].
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

    /// Where this tab sits in the tab BAR, counting from zero.
    ///
    /// [`adw::TabView`] owns this number and moves it when the user DRAGS a tab along
    /// the bar — `AdwTabBar` offers no property that turns dragging off — so it is the
    /// only authority for the order the user arranged. [`TabsHandle::entries`] is
    /// insertion order and does not track it. Read by
    /// `window::WindowHandle::cached_subtree`, which pairs it with
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

    /// Re-gathers this tab's persisted state.
    pub fn refresh_tab(&self) {
        self.inner.state.refresh_tab();
    }

    /// This tab's contribution to its window's subtree; see
    /// [`crate::persistence::StateHandle::cached_tab`] for what a tab that has
    /// never gathered contributes.
    #[must_use]
    pub fn cached_tab(&self) -> Option<TabState> {
        self.inner.state.cached_tab()
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
    /// Every live tab, in INSERTION order — see [`TabsHandle::entries`], which is
    /// where the divergence from page order and its one affected consumer are stated.
    entries: RefCell<Vec<TabHandle>>,
    /// The tabs a launch still owes a page, or `None` once it owes none (or when
    /// there were never any). `take`n by [`TabsInner::materialize_restore`], which is
    /// what makes the materialization run exactly once — an ABORT re-seats what it
    /// never created, with `may_create` cleared so no later wake retries it.
    pending_restore: RefCell<Option<RestoreTabs>>,
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

    /// Opens one tab; see [`TabsHandle::open_tab`], which is the public face of
    /// this and its only caller besides [`TabsInner::create_one_restored_tab`].
    fn open_tab(&self, parts: TabParts) -> bool {
        if self.core.borrow().is_none() {
            eprintln!(
                "wolf-gnome: new tab not opened: the window's core has already been taken out by \
                 its close handler, so a tab would have no Neovim behind it"
            );
            return false;
        }

        let panes = pane_view::build(&self.core, &self.metrics);
        // Realized on the first wake at which a window grid exists. NO arity guard,
        // for `window::build`'s recorded reason: a single-leaf layout is rebuilt
        // exactly like a multi-leaf one. This queue is TAB 0's route; a tab the
        // restore materialization creates realizes its own layout immediately
        // instead, and `PaneHandle::realize_layout` drops the copy queued here so
        // the rebuild stays one blocking call.
        if let Some(layout) = parts.layout.clone() {
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
            core: &self.core,
            panes: &panes,
            launch_cwd: parts.cwd,
            empty: parts.empty,
            sidebar_shown: parts.sidebar_shown,
            restored_layout: parts.layout,
        });

        let page = self.view.append(&overlay);
        let entry = TabHandle {
            inner: Rc::new(TabInner {
                view: self.view.clone(),
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

        self.entries.borrow_mut().push(entry);
        // AFTER registration. `append` already selects the FIRST page, so this is a
        // no-op for tab 0 and the switch for every later one.
        self.view.set_selected_page(&page);
        true
    }

    /// Materializes the tabs [`TabsHandle::queue_restore`] left, if this wake may.
    ///
    /// Runs at the END of every render wake and costs one `Option` borrow when
    /// nothing is queued; see the module docs for the sequence and
    /// [`crate::tabs_ui::restore_press`] for why the two refusals differ.
    ///
    /// ## The abort RETAINS what it did not create
    ///
    /// A [`TabsInner::create_one_restored_tab`] that fails stops the loop and the
    /// layouts it never reached — the failing one included, since no page was opened
    /// for it — are re-seated here with `may_create` cleared. The remainder goes on
    /// seeding this window's contribution
    /// ([`crate::state_seam::pending_tab_states`]) and creation is never retried:
    /// nothing else can speak for a tab with no page, and a retry would put a
    /// blocking `create_tabpage` on every frame behind a Neovim that has just failed
    /// to answer one. The tabs that WERE created are dropped from the re-seated
    /// queue, because each of them now has its own cache and would otherwise be
    /// contributed twice.
    ///
    /// ## Borrow discipline (load-bearing)
    ///
    /// The four inputs are read in three statements of their own — the queue's two
    /// come from one borrow — the queue is `take`n in a fourth, the re-seat after an
    /// abort is a statement of its own, and `entries` is
    /// NEVER borrowed across [`TabsInner::open_tab`], `create_tabpage` or
    /// `set_selected_page`. That last one fires `on_selected_changed`
    /// SYNCHRONOUSLY, which borrows `entries` and the core again and re-enters the
    /// shell through the overlays it closes, so a borrow held here is a `RefCell`
    /// panic rather than a warning.
    fn materialize_restore(&self) {
        let (pending, may_create) = self
            .pending_restore
            .borrow()
            .as_ref()
            .map_or((0, false), |restore| {
                (restore.layouts.len(), restore.may_create)
            });
        let first_tabpage = self.entries.borrow().first().map_or(0, TabHandle::tabpage);
        // A lock-free atomic read, never an RPC.
        let liveness = self.core.borrow().as_ref().map(WolfCore::liveness);

        match tabs_ui::restore_press(first_tabpage, liveness, pending, may_create) {
            // `Wait` leaves the queue where it is and a later wake asks again; no
            // timer and no polling, because this rides the wake that already
            // exists. `Idle` is the steady state — and, after an abort, the answer
            // that keeps the retained remainder a seed source rather than work.
            RestorePress::Idle | RestorePress::Wait => return,
            RestorePress::Create => {}
        }

        // TAKEN in a statement of its own, which is what makes the loop below run
        // exactly once per window per launch.
        let Some(restore) = self.pending_restore.borrow_mut().take() else {
            return;
        };
        // The page the launch must END on. Cloned out before any tab is created,
        // because each `open_tab` selects the page it just appended.
        let first_page = self
            .entries
            .borrow()
            .first()
            .map(|entry| entry.page().clone());

        let RestoreTabs {
            launch_cwd,
            empty,
            sidebar_shown,
            layouts,
            may_create: _,
        } = restore;
        let mut created = 0_usize;
        for layout in &layouts {
            if !self.create_one_restored_tab(&launch_cwd, empty, sidebar_shown, layout.clone()) {
                // One message has already been printed. Creating the REST would
                // build pages over tabpages this window cannot address.
                break;
            }
            created += 1;
        }
        if created < layouts.len() {
            // The un-created remainder, re-seated as a SEED SOURCE and nothing more.
            // A statement of its own: the borrow is dropped before
            // `set_selected_page` below re-enters this module.
            *self.pending_restore.borrow_mut() = Some(RestoreTabs {
                launch_cwd,
                empty,
                sidebar_shown,
                layouts: layouts[created..].to_vec(),
                may_create: false,
            });
        }

        if let Some(page) = first_page {
            // Back to tab 0. `on_selected_changed` issues the last of this loop's
            // `set_current_tabpage` calls, all of them fire-and-forget on the
            // ordered input channel, so the last one wins.
            self.view.set_selected_page(&page);
        }
    }

    /// Creates ONE restored tab: its Neovim tabpage, its page, and its layout —
    /// realized while that tabpage is still current. `false` aborts the rest.
    ///
    /// Factored out of [`TabsInner::materialize_restore`] rather than inlined so
    /// neither function needs a `too_many_lines` allow.
    fn create_one_restored_tab(
        &self,
        launch_cwd: &Path,
        empty: bool,
        sidebar_shown: bool,
        layout: Option<PaneNode>,
    ) -> bool {
        // The SAME rule a user's New Tab press uses, so a restored tab and a
        // pressed one cannot be rooted differently.
        let live = {
            let core_ref = self.core.borrow();
            core_ref.as_ref().and_then(WolfCore::live_cwd)
        };
        let cwd = tabs_ui::new_tab_cwd(empty, launch_cwd, live.as_deref());

        // BLOCKING, and it ENTERS the new tabpage: from here until the next
        // `create_tabpage`, Neovim is current on the page this call just made,
        // which is what `realize_layout` below depends on. Its core borrow is
        // taken and dropped in a statement of its own — `open_tab` borrows the
        // core again.
        let Some(handle) = ({
            let core_ref = self.core.borrow();
            core_ref
                .as_ref()
                .map(|core| core.create_tabpage(cwd.as_deref()))
        }) else {
            eprintln!(
                "wolf-gnome: restored tab not opened: the window's core has already been taken out"
            );
            return false;
        };
        // `create_tabpage` answers `0` on an RPC error or timeout, and Neovim reads
        // `0` as "the current tabpage" — so a page bound to it would aim every
        // later switch and close at whatever tab happens to be current.
        // `switch_target` is the one place that refusal is stated.
        if tabs_ui::switch_target(handle).is_none() {
            eprintln!(
                "wolf-gnome: restored tab not opened: create_tabpage answered no handle, so the \
                 remaining saved tabs are not restored for this launch"
            );
            return false;
        }

        // The persisted seed, which is a real directory even where the `:tcd` was
        // deliberately skipped: `state_seam::snapshot_cwd` answers `""` for an
        // empty-state window regardless of it.
        let seed = cwd.unwrap_or_else(|| launch_cwd.to_path_buf());
        if !self.open_tab(TabParts {
            tabpage: handle,
            cwd: seed,
            empty,
            sidebar_shown,
            layout: layout.clone(),
        }) {
            eprintln!(
                "wolf-gnome: restore not completed: the tabpage was created inside Neovim but no \
                 page could be opened for it, so that tabpage is now unreachable from the tab bar"
            );
            return false;
        }

        let Some(created) = self.entries.borrow().last().cloned() else {
            eprintln!("wolf-gnome: restore not completed: the page just opened is not registered");
            return false;
        };
        if let Some(layout) = layout {
            // NOW, while `handle` is still Neovim's current tabpage:
            // `rebuild_layout` acts on whichever one that is, and this instant is
            // the only moment this front-end knows for certain which.
            created.panes().realize_layout(&layout);
        }
        true
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
/// No page exists yet: `window::build` opens tab 0 once the toplevel is built, and
/// [`TabsHandle::open_tab`] is callable from any point after this. It needs no
/// window of its own — [`StateParts`] carries none, and the size that reaches
/// `state.json` is read at commit time by `window::WindowHandle::cached_subtree`.
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
            pending_restore: RefCell::new(None),
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
    pub fn open_tab(&self, parts: TabParts) -> bool {
        self.inner.open_tab(parts)
    }

    /// The selected tab, or `None` before the first page exists.
    ///
    /// The FOCUS, overlay and empty-state answer: which tab's cmdline strip an
    /// overlay returns focus to, which tab a window-level toggle applies to, and
    /// which tab survives [`TabsHandle::reset_after_restart`]. It has nothing to do
    /// with persistence — a window persists EVERY tab, through
    /// [`TabHandle::cached_tab`], and reaches them by [`TabsHandle::entries`].
    #[must_use]
    pub fn selected(&self) -> Option<TabHandle> {
        self.inner.selected()
    }

    /// Hands over the tabs a launch still has to materialize.
    ///
    /// Called ONCE by `window::build`, before the first synchronous wake, and only
    /// when the plan carries more than one tab. Materialization itself happens on a
    /// later wake through [`TabsHandle::on_render_wake`]; see the module docs.
    ///
    /// `may_create` is set here rather than trusted from the caller: a launch's queue
    /// may always attempt creation, and the one queue that may not — the remainder an
    /// abort re-seats — is written inside [`TabsInner::materialize_restore`] and never
    /// passes through this.
    pub fn queue_restore(&self, restore: RestoreTabs) {
        *self.inner.pending_restore.borrow_mut() = Some(RestoreTabs {
            may_create: true,
            ..restore
        });
    }

    /// The tabs this launch owes a page, or `None` once it owes none — which for a
    /// window with nothing to restore is from the start.
    ///
    /// Read by `window::WindowHandle::cached_subtree`, which turns it into the seeds
    /// those tabs contribute ([`crate::state_seam::pending_tab_states`]) and appends
    /// them to the live tabs' caches. Cloned out rather than borrowed across that
    /// composition: the borrow discipline this module keeps everywhere, and the same
    /// `RefCell` [`TabsInner::materialize_restore`] takes from.
    ///
    /// A restore that ABORTED still answers `Some`, carrying the tabs it never
    /// created; see [`RestoreTabs`]. What it never again answers is `Create` at
    /// [`crate::tabs_ui::restore_press`].
    #[must_use]
    pub fn pending_restore(&self) -> Option<RestoreTabs> {
        self.inner.pending_restore.borrow().clone()
    }

    /// Records the user's live sidebar PREFERENCE against the tabs that have no page
    /// yet, so a toggle made before materialization is not lost.
    ///
    /// The preference is per window and every tab persists it, so a queued tab needs
    /// it twice over: it is what [`crate::state_seam::pending_tab_states`] seeds that
    /// tab's contribution with, and what its own [`StateHandle`] is opened with once
    /// [`TabsInner::materialize_restore`] creates it. `AppShell::toggle_sidebar` calls
    /// this beside its fan-out over [`TabsHandle::entries`], which reaches the live
    /// tabs alone.
    pub fn set_pending_sidebar_shown(&self, shown: bool) {
        if let Some(restore) = self.inner.pending_restore.borrow_mut().as_mut() {
            restore.sidebar_shown = shown;
        }
    }

    /// Every live tab, in INSERTION order, for the shell's per-tab fan-outs.
    ///
    /// A `Vec` [`TabsInner::open_tab`] pushes each new page onto, with nothing
    /// reconciling it against [`adw::TabView`]'s page positions — so it diverges from
    /// the bar the first time the user reorders a tab. Every caller here is an
    /// order-insensitive fan-out (refresh each tab, toggle each tab's sidebar, count
    /// them), so this is left alone deliberately rather than sorted for one consumer.
    ///
    /// The ONE caller that needs the user's arrangement is
    /// `window::WindowHandle::cached_subtree`, because [`crate::state_seam`]'s
    /// `restore_plans` replays a saved window's tabs positionally. It pairs each entry
    /// with [`TabHandle::page_position`] and sorts through
    /// [`crate::state_seam::ordered_tab_states`].
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

        // AFTER the loop: tab 0 adopts its handle there, and
        // `tabs_ui::restore_press` refuses until it has. Steady-state cost is one
        // `Option::is_none()`.
        self.inner.materialize_restore();
    }

    /// Collapses the window to its selected tab after a SUCCESSFUL in-place Neovim
    /// restart, and clears that tab's stale handle.
    ///
    /// See the module docs on why doing nothing here freezes the window over a
    /// healthy Neovim. The `restarting` flag routes every close through the arm that
    /// does NOT tell the fresh Neovim to close a dead one's tabpage handle.
    ///
    /// **Any pending restore is DISCARDED, not replayed.** Materializing it here
    /// would put `2T-1` blocking RPCs on the restart path, which `shell.rs`'s
    /// "never call a `block_on_rpc` on the restart path" rule forbids outright —
    /// and the tabpage handles it would create belong to a Neovim that has just
    /// been replaced.
    pub fn reset_after_restart(&self) {
        self.inner.restarting.set(true);
        // In its own statement, before any page is closed: a close handler
        // re-enters this module.
        self.inner.pending_restore.borrow_mut().take();
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
