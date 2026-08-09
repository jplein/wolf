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
//! ## When a subtree is gathered
//!
//! On **every render wake**, with no timer and no debounce. That is affordable
//! because two coalescers already exist beneath us: `StateStore::set_snapshot` is
//! a no-op when the submitted tree is identical to the current one, and the
//! `wolf-state-writer` thread coalesces at 100 ms. Rebuilding one window's
//! one-tab subtree costs a `leaves()` walk plus one RPC-free `pane_status` per
//! pane, so a resize drag produces at most ~10 disk writes per second. This
//! preserves the crate's documented zero-polling property (no `glib::timeout` is
//! added).
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
//! **A refused gate contributes the CACHE, and never nothing.** This window keeps
//! its last good subtree and hands that over when the gate is shut, so a window
//! whose panes have not bound yet — or whose Neovim has just died — keeps
//! whatever it last persisted instead of vanishing from the tree the coordinator
//! commits. That is what makes the refusal mechanically identical to the
//! single-window behaviour it replaces, where a refused gate simply meant no call
//! to `set_snapshot`.
//!
//! This module submits nothing itself: [`crate::coordinator::Coordinator`] walks
//! every window, assembles the tree from these caches, and commits it.
//!
//! ## Borrow discipline (load-bearing)
//!
//! [`StateHandle::refresh_subtree`] runs INSIDE `window::wire_close`'s
//! `close-request` handler, which goes on to do `core.borrow_mut().take()`. Every
//! `core.borrow()` below is therefore taken and dropped in its OWN statement, and
//! none is live when [`StateHandle::refresh_subtree`] returns. An outstanding
//! `borrow()` there is a `RefCell` panic, not a warning.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;

use wolf_core::{WindowState, WolfCore};

use crate::pane_view::PaneHandle;
use crate::state_seam;

/// The live handles [`build`] needs, as one struct rather than six positional
/// parameters (the `ShellParts` precedent).
pub struct StateParts<'a> {
    /// The live core, shared with the rest of the app.
    pub core: &'a Rc<RefCell<Option<WolfCore>>>,
    /// The application window, read for its size.
    pub window: &'a adw::ApplicationWindow,
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
}

/// The persistence glue's shared state. Single GTK main thread, so `Cell` /
/// `RefCell` suffice.
struct PersistInner {
    /// The live core (liveness + live cwd reads).
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The application window (size only — position is never persisted).
    window: adw::ApplicationWindow,
    /// The pane view (layout snapshot + readiness counts).
    panes: PaneHandle,
    /// The last subtree the gate allowed, and this window's contribution to the
    /// tree whenever the gate is shut. `None` only before the first gather ever
    /// succeeds.
    subtree: RefCell<Option<WindowState>>,
    /// The launch cwd, persisted until nvim reports a live one.
    launch_cwd: PathBuf,
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
            window: parts.window.clone(),
            panes: parts.panes.clone(),
            subtree: RefCell::new(None),
            launch_cwd: parts.launch_cwd,
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

    /// This window's contribution to the persisted tree: the last subtree the
    /// gate allowed, or `None` before the first one ever succeeds.
    ///
    /// Read by [`crate::coordinator::Coordinator`] when it assembles the tree. A
    /// `None` from EVERY window is what makes
    /// [`crate::state_seam::snapshot_state`] answer `None` and the coordinator
    /// commit nothing at all.
    #[must_use]
    pub fn cached_subtree(&self) -> Option<WindowState> {
        self.inner.subtree.borrow().clone()
    }

    /// Gathers the live values into this window's cached subtree, gated by
    /// [`state_seam::may_snapshot`].
    ///
    /// Called from [`crate::shell::AppShell::on_render_wake`] via the
    /// coordinator, and as the FIRST thing `window::wire_close`'s
    /// `close-request` handler does — while the core and the panes are still
    /// alive. After that handler runs `core.borrow_mut().take()` there is no core
    /// to gather from, so this ordering is what makes a clean quit persist the
    /// session at all.
    ///
    /// A refused gate leaves the cache untouched rather than clearing it; see the
    /// module docs. Each core borrow is taken and dropped in its own statement,
    /// which is load-bearing for the same reason.
    pub fn refresh_subtree(&self) {
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

        let layout = self.inner.panes.layout_snapshot();
        let live = {
            let core_ref = self.inner.core.borrow();
            core_ref.as_ref().and_then(WolfCore::live_cwd)
        };

        let empty = self.inner.empty.get();
        let cwd = state_seam::snapshot_cwd(empty, &self.inner.launch_cwd, live.as_deref());
        let (width, height) = self.inner.window.default_size();
        // The user's PREFERENCE, never the live empty-state flag. `restore_plans`
        // reads this field back, so writing `!empty` would make any session that
        // ended in the empty state hide the sidebar on the next launch into a real
        // project.
        let subtree =
            state_seam::window_subtree(width, height, cwd, self.inner.sidebar_shown.get(), layout);
        *self.inner.subtree.borrow_mut() = Some(subtree);
    }
}
