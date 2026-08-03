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
//! ## When a snapshot is taken
//!
//! On **every render wake**, with no timer and no debounce. That is affordable
//! because two coalescers already exist beneath us: `StateStore::set_snapshot` is
//! a no-op when the submitted tree is identical to the current one, and the
//! `wolf-state-writer` thread coalesces at 100 ms. Rebuilding a one-window /
//! one-tab tree costs a `leaves()` walk plus one RPC-free `pane_status` per pane,
//! so a resize drag produces at most ~10 disk writes per second. This preserves
//! the crate's documented zero-polling property (no `glib::timeout` is added).
//!
//! It is also strictly better than macOS, whose snapshot is trigger-driven
//! (window move / resize-end / sidebar KVO) and therefore does NOT capture a file
//! opened in an already-open single-pane tab until the next trigger — reliably
//! only at clean quit. Here it is captured within one frame, and a SIGKILL loses
//! at most the writer's 100 ms coalesce window.
//!
//! Every snapshot is gated by [`crate::state_seam::may_snapshot`], because a
//! snapshot taken while nvim is dead or before the first pane binds would persist
//! an empty layout and destroy the user's saved one.
//!
//! ## Borrow discipline (load-bearing)
//!
//! [`StateHandle::flush_on_close`] runs INSIDE `app::wire_close`'s
//! `close-request` handler, which goes on to do `core.borrow_mut().take()`. Every
//! `core.borrow()` below is therefore taken and dropped in its OWN statement, and
//! none is live when [`StateHandle::flush_on_close`] returns. An outstanding
//! `borrow()` there is a `RefCell` panic, not a warning.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;

use wolf_core::{StateStore, WolfCore};

use crate::pane_view::PaneHandle;
use crate::state_seam;

/// How long [`StateHandle::flush_on_close`] blocks waiting for the writer thread
/// to land the final snapshot, mirroring `wolf-ffi`'s shutdown flush.
///
/// The writer coalesces at 100 ms and each write is one small `serde_json`
/// document plus a rename, so two seconds is generous; it exists to bound the
/// quit path against a wedged filesystem, not to be reached.
const STATE_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// The live handles [`build`] needs, as one struct rather than six positional
/// parameters (the `ShellParts` precedent).
pub struct StateParts<'a> {
    /// The live core, shared with the rest of the app.
    pub core: &'a Rc<RefCell<Option<WolfCore>>>,
    /// The application window, read for its size.
    pub window: &'a adw::ApplicationWindow,
    /// The pane view, which supplies the layout and the readiness counts.
    pub panes: &'a PaneHandle,
    /// The opened state store, or `None` when no state directory could be
    /// resolved (state is then neither saved nor restored).
    pub store: Option<StateStore>,
    /// The cwd the core was launched with, persisted until nvim reports a live
    /// one.
    pub launch_cwd: PathBuf,
    /// Whether the app launched into the empty state.
    pub empty: bool,
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
    /// The state store, or `None` when there is nowhere to save.
    store: Option<StateStore>,
    /// The launch cwd, persisted until nvim reports a live one.
    launch_cwd: PathBuf,
    /// The LIVE empty-state flag, kept in step by [`StateHandle::set_empty`].
    empty: Cell<bool>,
}

/// A cloneable handle to the state-persistence glue.
#[derive(Clone)]
pub struct StateHandle {
    inner: Rc<PersistInner>,
}

/// Builds the persistence glue. Installs no closures and takes no snapshot by
/// itself — the shell drives it from the wake, and `app::wire_close` from the
/// close request.
#[must_use]
pub fn build(parts: StateParts<'_>) -> StateHandle {
    StateHandle {
        inner: Rc::new(PersistInner {
            core: parts.core.clone(),
            window: parts.window.clone(),
            panes: parts.panes.clone(),
            store: parts.store,
            launch_cwd: parts.launch_cwd,
            empty: Cell::new(parts.empty),
        }),
    }
}

impl StateHandle {
    /// Records the live empty-state flag.
    ///
    /// Called from [`crate::shell::AppShell::set_empty_state`], so the persisted
    /// `cwd` and `sidebar_visible` follow the empty state rather than being
    /// frozen at whatever launch decided — opening a folder into an
    /// empty-state window must start persisting that folder.
    pub fn set_empty(&self, empty: bool) {
        self.inner.empty.set(empty);
    }

    /// Takes a snapshot on a render wake (gated by
    /// [`state_seam::may_snapshot`]). Called from
    /// [`crate::shell::AppShell::on_render_wake`].
    pub fn on_render_wake(&self) {
        self.snapshot_now();
    }

    /// Takes a FINAL snapshot and blocks until it has been written.
    ///
    /// Called as the FIRST statement of `app::wire_close`'s `close-request`
    /// handler — while the core and the panes are still alive. After that handler
    /// runs `core.borrow_mut().take()`, a snapshot would see no core and do
    /// nothing, so this ordering is what makes a clean quit persist the session
    /// at all.
    pub fn flush_on_close(&self) {
        self.snapshot_now();
        let Some(store) = &self.inner.store else {
            return;
        };
        if !store.flush(STATE_FLUSH_TIMEOUT) {
            eprintln!(
                "wolf-gnome: timed out after {STATE_FLUSH_TIMEOUT:?} waiting for state.json to \
                 be written; the last session may not have been saved"
            );
        }
    }

    /// Gathers the live values and submits one whole-tree snapshot.
    ///
    /// Each core borrow is taken and dropped in its own statement; see the module
    /// docs on why that is load-bearing here specifically.
    fn snapshot_now(&self) {
        let Some(store) = &self.inner.store else {
            return;
        };

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
        // `sidebar_visible` is the inverse of the empty-state flag: the sidebar is
        // hidden exactly when no project is open.
        store.set_snapshot(state_seam::snapshot_state(
            width, height, cwd, !empty, layout,
        ));
    }
}
