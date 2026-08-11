//! The GTK GLUE that commits `wolf-gnome`'s application state to `state.json`.
//!
//! Named after (and playing the role of) the macOS `StatePersistence.swift`, and
//! thin by construction: every DECISION already lives in the pure
//! [`crate::state_seam`] / [`crate::pane_layout`] seams, so this module only
//! gathers live values and hands them over. It follows the same conventions as
//! `shell.rs` and `crash_overlay.rs` — an `Rc` of shared inner state, interior
//! mutability via `Cell`, and short borrows that are never held across a
//! [`WolfCore`] call.
//!
//! One instance per TAB. A window's subtree is assembled from its tabs' caches by
//! `window::WindowHandle::cached_subtree`, so what lives here is one tab's
//! [`TabState`] and the rules that decide when it may be replaced.
//!
//! ## When a tab is gathered
//!
//! On **every render wake**, for every tab, with no timer and no debounce. That is
//! affordable because two coalescers already exist beneath us:
//! `StateStore::set_snapshot` is a no-op when the submitted tree is identical to
//! the current one, and the `wolf-state-writer` thread coalesces at 100 ms.
//! Rebuilding one tab costs a `leaves()` walk and one snapshot read — no RPC and no
//! `pane_status` — so a resize drag produces at most ~10 disk writes per second.
//! This preserves the crate's documented zero-polling property (no `glib::timeout`
//! is added).
//!
//! It is also strictly better than macOS, whose snapshot is trigger-driven
//! (window move / resize-end / sidebar KVO) and therefore does NOT capture a file
//! opened in an already-open single-pane tab until the next trigger — reliably
//! only at clean quit. Here it is captured within one frame, and a SIGKILL loses
//! at most the writer's 100 ms coalesce window.
//!
//! Every gather is gated by [`crate::state_seam::may_snapshot`], because a
//! subtree built while nvim is dead or before the first pane binds would persist
//! an empty layout and destroy the user's saved one.
//!
//! **A refused gather contributes the CACHE, and never nothing.** There are two
//! refusal routes and they compose: the gate above, and
//! [`crate::pane_layout::LayoutGather::Refuse`] — a leaf whose Neovim window is in
//! neither snapshot map, which the pane view answers rather than writing an empty
//! path. Either one leaves this tab's last good [`TabState`] standing, so a tab
//! whose panes have not bound yet, or whose Neovim has just died, keeps whatever it
//! last persisted instead of vanishing from the tree the coordinator commits.
//!
//! **A tab restored with a layout it has never gathered contributes the SEED.**
//! [`StateHandle::cached_tab`] derives it at read time through
//! [`crate::state_seam::seeded_tab_state`], so a tab with no bound pane — one whose
//! `realize_layout` bound none, so whose gate never opens — is not erased by the
//! next commit. The seed stops being consulted the instant a real gather succeeds,
//! which for a materialized restored tab is its first wake: its panes were created
//! synchronously, and its leaves resolve from the window-keyed maps while Neovim is
//! on another tabpage, so being unselected costs it nothing. The tab that has no
//! glue here at all — one still QUEUED for materialization — is seeded a level up,
//! by `window::WindowHandle::cached_subtree` through
//! [`crate::state_seam::pending_tab_states`].
//!
//! This module submits nothing itself: [`crate::coordinator::Coordinator`] walks
//! every window, which walks every tab, and commits the assembled tree.
//!
//! ## Borrow discipline (load-bearing)
//!
//! [`StateHandle::refresh_tab`] runs INSIDE `window::wire_close`'s
//! `close-request` handler, which goes on to do `core.borrow_mut().take()`. Every
//! `core.borrow()` below is therefore taken and dropped in its OWN statement, and
//! none is live when [`StateHandle::refresh_tab`] returns. An outstanding
//! `borrow()` there is a `RefCell` panic, not a warning.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use wolf_core::{PaneNode, TabState, WolfCore};

use crate::pane_layout::LayoutGather;
use crate::pane_view::PaneHandle;
use crate::state_seam;

/// The live handles [`build`] needs, as one struct rather than six positional
/// parameters (the `ShellParts` precedent).
pub struct StateParts<'a> {
    /// The live core, shared with the rest of the app.
    pub core: &'a Rc<RefCell<Option<WolfCore>>>,
    /// The pane view, which supplies the layout and the readiness counts.
    pub panes: &'a PaneHandle,
    /// The cwd the core was launched with, persisted until nvim reports a live
    /// one.
    pub launch_cwd: PathBuf,
    /// Whether the app launched into the empty state.
    pub empty: bool,
    /// The user's restored sidebar PREFERENCE
    /// ([`crate::state_seam::RestorePlan::sidebar_visible`]), which is what is
    /// persisted — never what is on screen.
    pub sidebar_shown: bool,
    /// The layout this tab was RESTORED with, or `None` for a tab the user opened.
    ///
    /// Held for [`crate::state_seam::seeded_tab_state`], not for the rebuild — the
    /// rebuild is the pane view's, and it has already been asked for it.
    pub restored_layout: Option<PaneNode>,
}

/// The persistence glue's shared state. Single GTK main thread, so `Cell` /
/// `RefCell` suffice.
struct PersistInner {
    /// The live core (liveness + live cwd reads).
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The pane view (layout snapshot + readiness counts).
    panes: PaneHandle,
    /// The last [`TabState`] a successful gather produced, and this tab's
    /// contribution whenever a later one refuses. `None` until the first gather
    /// succeeds, which for a restored tab whose `realize_layout` bound no pane is
    /// forever — see [`StateHandle::cached_tab`].
    tab: RefCell<Option<TabState>>,
    /// The launch cwd, persisted until nvim reports a live one.
    launch_cwd: PathBuf,
    /// The layout this tab was restored with, for the seed.
    restored_layout: Option<PaneNode>,
    /// The LIVE empty-state flag, kept in step by [`StateHandle::set_empty`].
    empty: Cell<bool>,
    /// The user's LIVE sidebar preference, kept in step by
    /// [`StateHandle::set_sidebar_shown`].
    sidebar_shown: Cell<bool>,
}

/// A cloneable handle to the state-persistence glue.
#[derive(Clone)]
pub struct StateHandle {
    inner: Rc<PersistInner>,
}

/// Builds the persistence glue. Installs no closures and gathers nothing by
/// itself — the coordinator drives it from the wake and from the close request.
#[must_use]
pub fn build(parts: StateParts<'_>) -> StateHandle {
    StateHandle {
        inner: Rc::new(PersistInner {
            core: parts.core.clone(),
            panes: parts.panes.clone(),
            tab: RefCell::new(None),
            launch_cwd: parts.launch_cwd,
            restored_layout: parts.restored_layout,
            empty: Cell::new(parts.empty),
            sidebar_shown: Cell::new(parts.sidebar_shown),
        }),
    }
}

impl StateHandle {
    /// Records the live empty-state flag.
    ///
    /// Called from [`crate::shell::AppShell::set_empty_state`], so the persisted
    /// `cwd` follows the empty state rather than being frozen at whatever launch
    /// decided — opening a folder into an empty-state window must start persisting
    /// that folder.
    pub fn set_empty(&self, empty: bool) {
        self.inner.empty.set(empty);
    }

    /// Records the user's sidebar PREFERENCE, mirroring [`StateHandle::set_empty`].
    ///
    /// Called from [`crate::shell::AppShell::toggle_sidebar`]. The preference is
    /// what is persisted, deliberately not what is on screen: an empty-state
    /// session shows no sidebar, and persisting that would hand the next launch
    /// into a real project a hidden tree the user never asked for.
    pub fn set_sidebar_shown(&self, shown: bool) {
        self.inner.sidebar_shown.set(shown);
    }

    /// This tab's contribution to its window's subtree: the last [`TabState`] a
    /// gather produced, or — for a tab restored with a layout it has never gathered
    /// — [`state_seam::seeded_tab_state`]'s answer.
    ///
    /// The seed is derived HERE, at read time, rather than written into the cache
    /// at build time, for two reasons. It follows live preference changes, so a
    /// sidebar toggle or an Open Folder reaches a tab whose gate never opened; and
    /// it stops being consulted the instant a real gather succeeds, so a seeded
    /// value can never outlive the fact it was standing in for.
    ///
    /// `None` from EVERY tab of a window is what makes
    /// [`crate::state_seam::window_subtree`] answer `None`, and a `None` from every
    /// window is what makes [`crate::state_seam::snapshot_state`] answer `None` and
    /// the coordinator commit nothing at all.
    #[must_use]
    pub fn cached_tab(&self) -> Option<TabState> {
        // Bound in its own statement so the seed below is not derived under a live
        // borrow of the cache it may replace.
        let cached = self.inner.tab.borrow().clone();
        cached.or_else(|| {
            state_seam::seeded_tab_state(
                self.inner.restored_layout.as_ref(),
                self.inner.empty.get(),
                &self.inner.launch_cwd,
                self.inner.sidebar_shown.get(),
            )
        })
    }

    /// Gathers the live values into this tab's cache, gated by
    /// [`state_seam::may_snapshot`] and by the pane view's own refusal.
    ///
    /// Called from [`crate::shell::AppShell::on_render_wake`] via the
    /// coordinator, for every tab, and as the FIRST thing `window::wire_close`'s
    /// `close-request` handler does — while the core and the panes are still
    /// alive. After that handler runs `core.borrow_mut().take()` there is no core
    /// to gather from, so this ordering is what makes a clean quit persist the
    /// session at all.
    ///
    /// Either refusal leaves the cache untouched rather than clearing it; see the
    /// module docs. Each core borrow is taken and dropped in its own statement,
    /// which is load-bearing for the same reason.
    pub fn refresh_tab(&self) {
        // `Liveness` is `Copy` and `WolfCore::liveness` is a lock-free atomic
        // read, never an RPC.
        let liveness = {
            let core_ref = self.inner.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            core.liveness()
        };

        let (bound_panes, reported_windows) = self.inner.panes.snapshot_readiness();
        if !state_seam::may_snapshot(liveness, bound_panes, reported_windows) {
            return;
        }

        // The SECOND refusal route: a leaf whose Neovim window is in neither
        // snapshot map. Returning here contributes the cache, exactly as a shut
        // gate does.
        let LayoutGather::Ready(layout) = self.inner.panes.layout_snapshot() else {
            return;
        };

        let live = {
            let core_ref = self.inner.core.borrow();
            core_ref.as_ref().and_then(WolfCore::live_cwd)
        };

        let empty = self.inner.empty.get();
        let cwd = state_seam::snapshot_cwd(empty, &self.inner.launch_cwd, live.as_deref());
        // The user's PREFERENCE, never the live empty-state flag. `restore_plans`
        // reads this field back, so writing `!empty` would make any session that
        // ended in the empty state hide the sidebar on the next launch into a real
        // project.
        let tab = state_seam::tab_subtree(cwd, self.inner.sidebar_shown.get(), layout);
        *self.inner.tab.borrow_mut() = Some(tab);
    }
}
