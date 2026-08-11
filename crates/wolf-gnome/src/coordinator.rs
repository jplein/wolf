//! The PROCESS-scoped layer above `wolf-gnome`'s windows: the one state store,
//! the one config read, the one cell measurement, the ordered registry of live
//! windows, and the `app.`-namespace actions.
//!
//! Everything a window needs is window-scoped and lives in [`crate::window`];
//! what lives here is only what a SECOND copy of would be wrong rather than
//! merely wasteful:
//!
//! - **The [`StateStore`].** Two stores opened over one directory silently
//!   clobber each other: both report a successful flush, the file ends up
//!   holding whichever tree was written last, and each store still claims its
//!   own tree in memory — so one window's entire session is lost with no panic
//!   and no error. `wolf-ffi` records the same rule for macOS.
//! - **The config.** One file, one read. N reads are N chances for two windows
//!   to disagree about the font mid-session.
//! - **The [`CellMetrics`].** Deterministic given the config, and already shared
//!   by `Rc` inside a window.
//! - **The `app.`-namespace actions.** `gio::ActionMap::add_action` REPLACES an
//!   existing entry, so installing them per window leaves the last window's
//!   closure holding the whole application's Quit — and closing that window
//!   would silently kill Ctrl+Shift+Q for every window still open. They are
//!   installed exactly once, here, and every window's shell installs `win.` rows
//!   only.
//! - **The launch plan.** A launch restores a SESSION, not a window:
//!   [`crate::state_seam::restore_plans`] answers with one plan per saved window.
//!
//! ## Ownership, and why the direction is forced
//!
//! [`Coordinator`] is a cloneable `Rc<CoordinatorInner>` holding STRONG
//! [`WindowHandle`]s, and every window-side closure holds a
//! `Weak<CoordinatorInner>` it upgrades per use ([`upgrade`]). The reverse — a
//! window closure owning the coordinator — is a cycle through the whole graph:
//! the coordinator owns the handle, the handle owns the window, and the window
//! owns the closures. Nothing would ever be dropped.
//!
//! ## The state protocol
//!
//! Each window CONTRIBUTES a `WindowState` subtree assembled from its own tabs'
//! contributions ([`crate::persistence::StateHandle::refresh_tab`], gathered per
//! tab), and the coordinator assembles the tree and commits it.
//!
//! **"An empty contribution set commits nothing" holds at TWO levels**, and
//! for the same reason at each. A tab whose [`crate::state_seam::may_snapshot`]
//! gate is shut — or whose leaves would not resolve — contributes what it last
//! cached rather than vanishing; a window none of whose tabs contributed
//! contributes nothing at all rather than a `tabs: []` window, which restore reads
//! back as the empty state
//! ([`crate::state_seam::window_subtree`]); and a contribution set of windows that
//! is empty commits NOTHING — see [`crate::state_seam::snapshot_state`], whose
//! `None` is what stops a launch that never bound a pane from overwriting a good
//! save with an empty file.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::{Rc, Weak};
use std::time::Duration;

use adw::prelude::*;
use gtk::gio;
use wolf_core::{PaletteWindow, StateStore, WolfConfig, WolfState};

use crate::actions;
use crate::app;
use crate::command_action::{ui_action_for, UiAction};
use crate::state_seam::{self, CloseCommit, RestorePlan};
use crate::window::{self, WindowHandle, WindowParts};
use crate::workspace;
use crate::CellMetrics;

/// How long the close and quit paths block waiting for the writer thread to land
/// the final snapshot, mirroring `wolf-ffi`'s shutdown flush.
///
/// The writer coalesces at 100 ms and each write is one small `serde_json`
/// document plus a rename, so two seconds is generous; it exists to bound the
/// quit path against a wedged filesystem, not to be reached.
const STATE_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// A live window's identity, minted by the coordinator and never reused while
/// the window it names is alive.
///
/// `u16` because it is also the raise target `wolf_core::PaletteWindow::index`
/// carries, so the palette needs no second numbering.
pub type WindowId = u16;

/// The coordinator's shared state. Single GTK main thread, so `Cell` / `RefCell`
/// suffice; there is deliberately no `static`, no `OnceLock` and no
/// `thread_local!` in this crate.
pub struct CoordinatorInner {
    /// The ONE state store, or `None` when no state directory could be resolved
    /// (state is then neither saved nor restored).
    store: Option<StateStore>,
    /// The user configuration, read once at `activate`.
    config: WolfConfig,
    /// The cell metrics measured once from [`CoordinatorInner::config`], shared
    /// by `Rc` with every window's draw and resize closures.
    metrics: Rc<CellMetrics>,
    /// The live windows, in the order they were opened — which is the order the
    /// next snapshot writes them back in.
    windows: RefCell<Vec<WindowHandle>>,
    /// The next id to mint; see [`Coordinator::mint_id`].
    next_id: Cell<WindowId>,
    /// Latched by [`Coordinator::quit`] BEFORE any window is closed, so each
    /// window's close handler reads it and skips its own commit.
    shutting_down: Cell<bool>,
}

/// A cloneable handle to the process-scoped coordinator.
#[derive(Clone)]
pub struct Coordinator {
    inner: Rc<CoordinatorInner>,
}

/// Builds the coordinator: opens the state store, reads the config, and measures
/// the cell.
///
/// Builds no window by itself — call [`Coordinator::install_app_actions`] and
/// then [`Coordinator::restore_session`].
#[must_use]
pub fn build() -> Coordinator {
    // The config is read ONCE here, beside the state store and before any widget
    // exists — matching macOS's `CellMetrics.measured(config:)` at launch. An
    // edit to `wolf-config.yaml` therefore takes effect on the next launch.
    let config = app::load_config();
    // Measure the cell once and share it (by Rc) with every window's draw and
    // resize closures so it is available even if a resize fires before the first
    // draw.
    let metrics = Rc::new(app::measure_metrics(&config));
    Coordinator {
        inner: Rc::new(CoordinatorInner {
            store: app::open_state_store(),
            config,
            metrics,
            windows: RefCell::new(Vec::new()),
            next_id: Cell::new(0),
            shutting_down: Cell::new(false),
        }),
    }
}

/// The first id at or after `next` that no live window holds: monotonic, wrapping,
/// and skipping.
///
/// The skip is two lines and removes a whole class of bug: after 65 536 windows the
/// counter wraps, and a reused id would make [`Coordinator::on_window_closing`]
/// deregister a DIFFERENT window than the one that closed, and
/// [`Coordinator::raise`] present it.
///
/// Pure, and separated from [`Coordinator::mint_id`]'s registry read for exactly
/// that reason: reaching the wrap through the coordinator would mean opening 65 536
/// windows, so the arithmetic is only testable as a function of `(next, live)`.
///
/// `live` is expected to be small — one Neovim process per entry — and is scanned
/// linearly. It must never hold every one of the 65 536 ids, which would make this
/// loop forever; a window costs a process and a tokio runtime, so that state is not
/// reachable.
#[must_use]
pub fn next_free_id(next: WindowId, live: &[WindowId]) -> WindowId {
    let mut id = next;
    while live.contains(&id) {
        id = id.wrapping_add(1);
    }
    id
}

/// Upgrades a closure's `Weak<CoordinatorInner>` back into a [`Coordinator`].
///
/// Every window-side closure holds a `Weak`, never an `Rc`; see the module docs
/// on why that direction is forced.
#[must_use]
pub fn upgrade(weak: &Weak<CoordinatorInner>) -> Option<Coordinator> {
    weak.upgrade().map(|inner| Coordinator { inner })
}

impl Coordinator {
    /// A `Weak` reference for a window-side closure to hold.
    #[must_use]
    pub fn weak(&self) -> Weak<CoordinatorInner> {
        Rc::downgrade(&self.inner)
    }

    /// Installs the `app.`-namespace actions on the application, exactly ONCE
    /// per process.
    ///
    /// The mirror of [`crate::shell::AppShell::install_actions`], which installs
    /// the `win.` rows: same table, same "an unimplemented row is never
    /// installed" rule, opposite namespace. Splitting them is not tidiness — see
    /// the module docs for what a per-window `app.` install costs.
    ///
    /// Must run BEFORE the first window is built, so the menu and the chord
    /// controller resolve `app.quit` from the moment a window exists.
    pub fn install_app_actions(&self, app: &adw::Application) {
        for binding in actions::ACTION_TABLE {
            let Some(("app", name)) = binding.action.split_once('.') else {
                continue;
            };
            // A row resolves through its command, exactly as the `win.` half
            // does; a row that classifies to `Deferred` / `NoGnomeEquivalent` is
            // never installed, so the primary menu can never show an item that
            // does nothing.
            let Some(command) = binding.command else {
                eprintln!(
                    "wolf-gnome: app-scoped action {:?} carries no command; not installed",
                    binding.action
                );
                continue;
            };
            let ui_action = ui_action_for(command);
            if matches!(
                ui_action,
                UiAction::Deferred(_) | UiAction::NoGnomeEquivalent(_)
            ) {
                continue;
            }

            let action = gio::SimpleAction::new(name, None);
            let coordinator = self.clone();
            // The application outlives every window, and it is the application's
            // action map that owns this closure — so holding the coordinator
            // STRONGLY here is the direction with no cycle in it.
            let app_for_action = app.clone();
            action.connect_activate(move |_action, _parameter| {
                coordinator.perform(ui_action, &app_for_action);
            });
            app.add_action(&action);
        }
    }

    /// Performs one `app.`-scoped [`UiAction`].
    ///
    /// Deliberately NOT exhaustive over [`UiAction`]: the arms here are the ones
    /// an `app.`-namespace row can reach, and every other variant is a `win.`
    /// row's, performed by [`crate::shell::AppShell::perform`] against a
    /// particular window. An unexpected variant is printed rather than dropped,
    /// for the same reason that one's `Deferred` arm is.
    fn perform(&self, action: UiAction, app: &adw::Application) {
        match action {
            UiAction::NewWindow => self.open_new_window(app),
            UiAction::QuitApplication => self.quit(app.upcast_ref()),
            other => {
                eprintln!(
                    "wolf-gnome: app-scoped ui action not performed: {other:?} needs a window and \
                     belongs in the win. namespace"
                );
            }
        }
    }

    /// Restores the whole saved SESSION: one window per saved window, in saved
    /// order, or exactly one empty-state window when there is nothing to
    /// restore.
    ///
    /// A window whose core fails to start is skipped and does not abort the
    /// others. If none opens at all the application has no window and exits,
    /// which is the single-window behaviour unchanged.
    pub fn restore_session(&self, app: &adw::Application) {
        // Both paths are canonicalized so `launch_decision`'s comparisons mean
        // what they say (`Path` equality compares components; it resolves no
        // symlinks).
        let process_cwd = std::env::current_dir()
            .ok()
            .map(|cwd| std::fs::canonicalize(&cwd).unwrap_or(cwd));
        let home = workspace::home_dir();
        // `state()` is the in-memory tree the store loaded from `state.json`
        // (defaults on any failure — loading is silently-ignore and never
        // rewrites the file).
        let saved = self.inner.store.as_ref().map(StateStore::state);
        let plans = state_seam::restore_plans(
            saved.as_ref(),
            process_cwd.as_deref(),
            home.as_deref(),
            Path::is_dir,
        );

        let mut opened = 0_usize;
        for plan in &plans {
            if self.open_window(app.upcast_ref(), plan) {
                opened += 1;
            }
        }
        if opened == 0 {
            eprintln!(
                "wolf-gnome: no window could be opened for any of the {} restored plans; exiting",
                plans.len()
            );
        }
    }

    /// Opens a FRESH, EMPTY window with its own core.
    ///
    /// The plan is built from no saved state AND no process cwd, which by
    /// construction is the empty state at `$HOME` with no layout and no restored
    /// size — macOS's `makeNewWindow`, which always opens the empty state
    /// regardless of the originating window's directory. Passing the process cwd
    /// (or the invoking window's) here would inherit a project, which is what
    /// New TAB does and is not this row's behaviour.
    ///
    /// **Both `None`s are the behaviour, not an omission to be helpfully filled
    /// in.** `restore_plans(Some(saved), ..)` would reopen the saved session, and
    /// `restore_plans(None, Some(process_cwd), ..)` would root the new window in
    /// whatever directory the application happens to have been launched from —
    /// which on a desktop launcher is `$HOME` and from a terminal is the first
    /// window's project, so the wrong behaviour would look right exactly half the
    /// time it was tried by hand.
    ///
    /// Takes any [`gtk::Application`] — `adw::Application` from the `app.`-action
    /// closure and `activate`, the plain one `GtkWindowExt::application()` answers
    /// from [`crate::shell::AppShell`] — so no caller has to downcast to reach it.
    pub fn open_new_window(&self, app: &impl IsA<gtk::Application>) {
        let home = workspace::home_dir();
        let plans = state_seam::restore_plans(None, None, home.as_deref(), Path::is_dir);
        // `restore_plans` never returns an empty vec; this is the floor, not a
        // fallback, so an empty one would be a broken invariant rather than a
        // condition to handle silently.
        let Some(plan) = plans.into_iter().next() else {
            eprintln!("wolf-gnome: restore_plans returned no plan for a new window; none opened");
            return;
        };
        if !self.open_window(app.upcast_ref(), &plan) {
            eprintln!("wolf-gnome: the new window's core failed to start; no window was opened");
        }
    }

    /// Builds one window for `plan` and registers it. Answers whether it opened.
    fn open_window(&self, app: &gtk::Application, plan: &RestorePlan) -> bool {
        let id = self.mint_id();
        window::build(WindowParts {
            app,
            coordinator: self,
            id,
            plan,
            config: &self.inner.config,
            metrics: &self.inner.metrics,
        })
        .is_some()
    }

    /// Registers a freshly built window. Called by [`crate::window::build`]
    /// BEFORE that window's first command drain, so nothing the drain reaches
    /// can act on a coordinator that does not yet know the window exists.
    pub fn register(&self, handle: &WindowHandle) {
        self.inner.windows.borrow_mut().push(handle.clone());
    }

    /// The next window id, minted through [`next_free_id`] and recorded.
    fn mint_id(&self) -> WindowId {
        let live: Vec<WindowId> = self
            .inner
            .windows
            .borrow()
            .iter()
            .map(WindowHandle::id)
            .collect();
        let id = next_free_id(self.inner.next_id.get(), &live);
        self.inner.next_id.set(id.wrapping_add(1));
        id
    }

    /// Commits the tree on `waking`'s render wake: refresh that window's subtree,
    /// then assemble from every window's cache.
    ///
    /// O(1) gather and O(N) assembly, and `StateStore::set_snapshot` still no-ops
    /// on an identical tree — so "a snapshot on every wake is affordable" holds
    /// unchanged.
    pub fn commit(&self, waking: WindowId) {
        self.refresh(waking);
        self.commit_tree();
    }

    /// Handles one window's `close-request`, BEFORE its core is torn down.
    ///
    /// The decision is [`state_seam::close_commit`]'s and the glue is this
    /// `match`; each arm's reason lives with the rule rather than here. The
    /// deregistration is last, so `KeepSubtree` really does commit a tree that
    /// still holds the closing window.
    pub fn on_window_closing(&self, id: WindowId) {
        let others = self
            .inner
            .windows
            .borrow()
            .iter()
            .filter(|handle| handle.id() != id)
            .count();
        match state_seam::close_commit(self.inner.shutting_down.get(), others) {
            CloseCommit::Skip => {}
            CloseCommit::KeepSubtree => {
                // While this window's core and panes are still alive: a subtree
                // gathered after `core.borrow_mut().take()` sees no core, the
                // gate refuses it, and the session is silently not saved.
                self.refresh(id);
                self.commit_tree();
                self.flush();
            }
            CloseCommit::DropSubtree => {
                // Deregistered BEFORE assembling, so the tree really is committed
                // without this window. The arm is only ever reached with at least
                // one other window live, so what is assembled is never empty —
                // which matters, because an empty contribution set means "commit
                // nothing" ([`state_seam::snapshot_state`]) rather than "commit an
                // empty tree", and a drop that emptied the set would leave the
                // closed window's session in `state.json` instead of removing it.
                self.deregister(id);
                self.commit_tree();
                self.flush();
            }
        }
        self.deregister(id);
    }

    /// Quits: commits the WHOLE tree once, while every core and every pane is
    /// still alive, and only then closes the windows.
    ///
    /// The order is macOS's `applicationWillTerminate`, and every step of it is
    /// load-bearing. `shutting_down` is latched FIRST so each window's close
    /// handler answers [`CloseCommit::Skip`] and does not re-commit a tree that
    /// shrinks by one window per close. The refresh runs BEFORE any window is
    /// closed, because a window whose core has gone contributes nothing. The
    /// flush runs before the closes so a wedged writer cannot lose the tree that
    /// was just assembled.
    ///
    /// Every window the APPLICATION holds is closed, not merely every window the
    /// coordinator registered: a transient toplevel such as the About window is
    /// the application's too, and leaving one up would keep the process alive
    /// after a Quit.
    ///
    /// Deliberately not `gio::Application::quit()`, which bypasses
    /// `close-request` and leaves every captive Neovim orphaned.
    pub fn quit(&self, app: &gtk::Application) {
        self.inner.shutting_down.set(true);
        self.refresh_all();
        self.commit_tree();
        self.flush();
        for window in app.windows() {
            window.close();
        }
    }

    /// How many windows are live.
    #[must_use]
    pub fn window_count(&self) -> usize {
        self.inner.windows.borrow().len()
    }

    /// Every live window EXCEPT `exclude`, as the palette's Switch to Window rows.
    ///
    /// `exclude` is the window whose palette is opening: a row raising the window
    /// the user is already looking at does nothing, and it would rank against the
    /// query like any other.
    ///
    /// Each entry's `cwd` is that window's [`WindowHandle::display_cwd`] — the
    /// same string it contributes to `state.json` — so a window is offered under
    /// the directory it would be RESTORED at, and an empty-state window carries
    /// `""`, which core renders as "Switch to Window: Untitled". One rule with two
    /// consumers, rather than a second notion of where a window is.
    ///
    /// `index` is this crate's [`WindowId`], which is what
    /// [`Coordinator::raise`] takes back — the palette does no numbering of its
    /// own.
    #[must_use]
    pub fn palette_windows(&self, exclude: WindowId) -> Vec<PaletteWindow> {
        self.handles()
            .iter()
            .filter(|handle| handle.id() != exclude)
            .map(|handle| PaletteWindow {
                index: handle.id(),
                cwd: handle.display_cwd(),
            })
            .collect()
    }

    /// Presents the window with `id`.
    ///
    /// **Best-effort on Wayland**, and that is a property of the platform rather
    /// than of this call: a Wayland client cannot raise itself, so `present()` is a
    /// request the compositor may decline. `cage` declines it — it shows a single
    /// toplevel and keeps the newest on top — so under the smoke harness this
    /// raises nothing, and no test can observe it. On a session compositor
    /// xdg-activation normally carries it.
    ///
    /// A silent no-op when no live window holds `id`, which is ordinary rather than
    /// an error: the palette snapshots the window list when it OPENS, so a window
    /// closed between that snapshot and the Enter leaves a row behind it.
    pub fn raise(&self, id: WindowId) {
        if let Some(handle) = self.handles().into_iter().find(|handle| handle.id() == id) {
            handle.present();
        }
    }

    /// Every live window's handle, cloned out so no `RefCell` borrow is held
    /// across a call that could re-enter the registry (a close handler does).
    fn handles(&self) -> Vec<WindowHandle> {
        self.inner.windows.borrow().clone()
    }

    /// Re-gathers one window's subtree, if it is still registered.
    fn refresh(&self, id: WindowId) {
        if let Some(handle) = self.handles().into_iter().find(|handle| handle.id() == id) {
            handle.refresh_subtree();
        }
    }

    /// Re-gathers every window's subtree.
    fn refresh_all(&self) {
        for handle in self.handles() {
            handle.refresh_subtree();
        }
    }

    /// Assembles the tree from every window's cached subtree, in window order.
    fn assemble(&self) -> Option<WolfState> {
        state_seam::snapshot_state(
            self.handles()
                .iter()
                .filter_map(WindowHandle::cached_subtree)
                .collect(),
        )
    }

    /// Submits the assembled tree to the store, or submits nothing at all.
    fn commit_tree(&self) {
        let Some(store) = &self.inner.store else {
            return;
        };
        let Some(tree) = self.assemble() else {
            return;
        };
        store.set_snapshot(tree);
    }

    /// Blocks until the writer thread has landed everything submitted so far.
    fn flush(&self) {
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

    /// Drops a window from the registry. Idempotent: `on_window_closing`'s
    /// `DropSubtree` arm deregisters before assembling, and the tail
    /// deregisters again.
    fn deregister(&self, id: WindowId) {
        self.inner
            .windows
            .borrow_mut()
            .retain(|handle| handle.id() != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One [`next_free_id`] row: the counter's current value, the ids live at that
    /// moment, and the id that must be minted.
    struct IdCase {
        desc: &'static str,
        next: WindowId,
        live: &'static [WindowId],
        want: WindowId,
    }

    /// [`next_free_id`] over the shapes the registry can present it with,
    /// INCLUDING the wrap that no test could otherwise reach.
    ///
    /// The wrap is why the arithmetic is a free function at all: reaching it
    /// through [`Coordinator::mint_id`] would mean opening 65 536 windows, each
    /// costing a Neovim process and a tokio runtime, so the only place the rule is
    /// decidable is as a function of `(next, live)`. That is the same reason
    /// `state_seam`'s decisions are pure functions rather than methods on the
    /// widgets that consume them.
    ///
    /// **What a wrong answer costs.** The id is the registry key: an id handed out
    /// while a live window still holds it makes
    /// [`Coordinator::on_window_closing`] deregister a DIFFERENT window than the
    /// one that closed — so the closing window stays in the tree that
    /// [`Coordinator::assemble`] writes to `state.json`, and the surviving one is
    /// dropped from it — and it makes [`Coordinator::raise`] present the wrong
    /// window for a palette row the user picked by its directory.
    ///
    /// FALSIFYING EDIT: delete the `while live.contains(&id)` loop, leaving
    /// `next_free_id` returning `next` verbatim. PREDICTED: the four rows whose
    /// `want` differs from their `next` redden, the first being "an id a live
    /// window still holds is skipped", which reports `0` where `1` was wanted.
    /// PREDICTED ISOLATED: yes — [`next_free_id`] has exactly one caller
    /// ([`Coordinator::mint_id`]), no other test in the workspace calls either, and
    /// no test opens enough windows for `mint_id` to collide.
    ///
    /// The three rows whose `want` EQUALS their `next` are the opposite guard: an
    /// implementation that incremented unconditionally would pass every other row
    /// here while making the id sequence a function of how often a window was
    /// opened rather than of which ids are free.
    ///
    /// One of them carries a NON-CONTIGUOUS registry — live ids strictly ABOVE the
    /// counter — and it is there for a defect the others cannot see. Every other row
    /// presents `live` ids that are either below `next` or start exactly at it, so
    /// an implementation that scanned `live` for ANY member `>= next` (rather than
    /// for `id` itself) would answer identically on all of them. It would not here:
    /// it would skip past two ids nothing holds and hand out `8` where `5` is free.
    /// The cost is not the wasted number — it is that the id would stop being "the
    /// first free one", which is the property the wrap rows below depend on to mean
    /// anything.
    #[test]
    fn next_free_id_table() {
        const CASES: &[IdCase] = &[
            IdCase {
                desc: "a fresh counter with no window open mints 0",
                next: 0,
                live: &[],
                want: 0,
            },
            IdCase {
                desc: "an id no live window holds is minted unchanged",
                next: 1,
                live: &[0],
                want: 1,
            },
            IdCase {
                desc: "an id a live window still holds is skipped",
                next: 0,
                live: &[0],
                want: 1,
            },
            IdCase {
                desc: "a RUN of live ids is skipped in one go",
                next: 5,
                live: &[5, 6],
                want: 7,
            },
            IdCase {
                desc: "live ids ABOVE the counter are not skipped past: the gap is free",
                next: 5,
                live: &[6, 7],
                want: 5,
            },
            IdCase {
                desc: "the counter WRAPS at u16::MAX rather than saturating",
                next: WindowId::MAX,
                live: &[WindowId::MAX],
                want: 0,
            },
            IdCase {
                desc: "the wrap keeps skipping: past u16::MAX, 0 is live too",
                next: WindowId::MAX,
                live: &[WindowId::MAX, 0],
                want: 1,
            },
        ];

        assert!(
            CASES.iter().any(|case| case.want != case.next),
            "no row expects an id DIFFERENT from the counter it was asked for, so \
             `next_free_id` could be `|next, _| next` — the live-id skip deleted outright — and \
             this table would still pass. The skip is the whole reason this function exists."
        );
        assert!(
            CASES.iter().any(|case| case.want == case.next),
            "no row expects the counter's OWN value, so an implementation that incremented \
             unconditionally would pass this table while making the minted id a function of how \
             many windows have ever been opened rather than of which ids are free."
        );
        assert!(
            CASES
                .iter()
                .any(|case| case.want == case.next && case.live.iter().any(|id| *id > case.next)),
            "no row presents a live id STRICTLY ABOVE the counter while still expecting the \
             counter's own value, so an implementation that scanned `live` for any member `>= \
             next` — rather than for `id` itself — would satisfy every row here. The distinction \
             is what `next_free_id` means: the FIRST free id at or after the counter, not the \
             first id past everything live."
        );

        for case in CASES {
            assert_eq!(
                next_free_id(case.next, case.live),
                case.want,
                "case: {} — next_free_id({}, {:?}) is not {}. The id is the REGISTRY KEY: an id \
                 a live window already holds makes `on_window_closing` deregister the wrong \
                 window (so the closing window's subtree stays in `state.json` and the surviving \
                 window's is dropped) and makes `raise` present the wrong window for a palette \
                 row the user picked by its directory. An id that is free but skipped costs \
                 nothing but means the counter no longer describes what it claims to.",
                case.desc,
                case.next,
                case.live,
                case.want
            );
        }
    }
}
