//! The native GTK pane-tree view for `wolf-gnome` (the split-panes UI).
//!
//! This is the GTK glue (decision D7/D8) that turns the pure [`crate::layout`]
//! [`PaneTree`] into a live tree of nested `gtk::Paned`s, one `gtk::DrawingArea`
//! per pane, plus a one-row **cmdline strip** along the bottom. It mirrors the
//! `tree_view.rs` / `picker.rs` patterns: `Rc`/`RefCell`/`Cell` interior mutability
//! on the single GTK main thread, a `Weak` upgrade inside every widget closure to
//! avoid reference cycles, and short snapshot/borrow scopes (never held across a
//! `WolfCore` call).
//!
//! ## Per-pane render
//!
//! Each pane binds to a Neovim window (`win`) and its grid (`>= 2`, resolved via
//! `win_pos`; pending until then). A pane's draw func paints ITS grid via the pure
//! [`crate::paint`] seam, and the cursor is drawn only in the pane that owns the
//! cursor's grid (`cursor.grid`). Panes NEVER paint grid 1.
//!
//! ## Cmdline strip (★ S3 requirement)
//!
//! Under `ext_multigrid` the command line, `/`-search, and messages live outside the
//! window grids the panes paint. The [`PaneInner::cmdline_area`] is a one-row
//! `DrawingArea` along the bottom that makes them visible again — and it paints
//! them NATIVELY, from the folded [`Snapshot::cmdline`] / [`Snapshot::toast`]. It
//! never reads grid 1: `ext_messages` implicitly enables `ext_cmdline` and sets
//! `cmdheight=0`, so grid 1 is EMPTY and painting its last row would draw nothing.
//! See [`PaneInner::draw_cmdline`].
//!
//! ## Intents
//!
//! On each render wake [`PaneHandle::on_render_wake`] creates the first pane when a
//! window grid appears, drains layout intents ([`wolf_core::WolfCore::drain_intents`])
//! to apply splits (mutate the tree + bind a new pane) and focus-direction moves
//! (resolve the neighbour in the tree + `focus_win`), detects closed windows (a
//! bound pane whose grid vanished) and collapses them, resolves pending pane grids,
//! and updates the active-pane focus ring from `active_win`.
//!
//! ## Layout persistence
//!
//! This module is both halves of the pane-layout persistence path:
//!
//! - **Save.** [`PaneHandle::layout_snapshot`] walks the live tree, reads each
//!   leaf's RAW buffer path from `WolfCore::pane_status(grid).abs_path` (RPC-free),
//!   encodes it via [`crate::pane_layout::encode`], and applies
//!   [`crate::state_seam::persistable_layout`]'s CONTENT suppression rule. It is
//!   called from [`crate::persistence`] on every render wake and at
//!   `close-request`. [`PaneHandle::snapshot_readiness`] supplies the two counts
//!   `state_seam::may_snapshot` gates on, so a snapshot taken before the first
//!   pane binds can never wipe a good save.
//! - **Restore.** [`PaneHandle::request_layout_rebuild`] queues the saved tree at
//!   launch; the first wake at which a window grid exists consumes it
//!   ([`PaneInner::consume_pending_layout`]) and realizes it
//!   ([`PaneInner::apply_layout_rebuild`]) through `WolfCore::rebuild_layout` —
//!   the ONE blocking RPC this front-end issues, documented there.
//!
//! ## Mouse and scroll
//!
//! Each pane area carries THREE pointer controllers, all of them thin glue over the
//! pure [`crate::mouse`] and [`crate::cell_at`] seams:
//!
//! 1. A `gtk::GestureDrag` (primary button only) for the whole press -> drag ->
//!    release sequence. ONE gesture, not a `GestureClick` plus something else: a
//!    gesture takes an implicit grab on the sequence and keeps delivering after the
//!    pointer leaves the widget, which is what makes drag-past-the-edge selection
//!    work (`EventControllerMotion` stops at the widget edge). Press calls
//!    [`PaneInner::on_pane_clicked`] FIRST and only then `send_mouse(Press)` —
//!    `focus_win` and `send_mouse` ride the SAME ordered input channel, so that
//!    ordering is guaranteed by construction. The press also PINS the target grid in
//!    `drag_grid`, and every subsequent drag/release event targets that pinned grid,
//!    so a drag that wanders into another pane still edits the pane it started in —
//!    and it pins the GDK BUTTON NUMBER in `drag_button` the same way, replayed for
//!    every later callback of the sequence. The MODIFIERS, by contrast, are re-read
//!    LIVE each time (macOS parity); see [`PaneInner::send_pane_mouse`] for why the
//!    two are treated differently. Drag is forwarded UNCONDITIONALLY on every
//!    callback (no throttling, no "changed cell" dedupe) — macOS parity.
//! 2. A `gtk::EventControllerScroll` with `VERTICAL` only (not `DISCRETE`, so the
//!    accumulation stays in our own tested seam; not `BOTH_AXES`, because horizontal
//!    scroll is not handled). It keeps ONE [`crate::mouse::PaneScroll`] per pane —
//!    the carried remainder AND the grid it belongs to, as one pure value, so the
//!    reset-on-grid-change rule is unit-tested instead of living in this file — and
//!    emits that many discrete `ScrollEvent`s. **Scrolling never changes focus** —
//!    no `focus_win`, no `set_active_pane`, no `grab_focus`.
//! 3. A `gtk::EventControllerMotion` whose ONLY job is to record the last
//!    widget-local pointer position, because GTK's `scroll` signal carries no
//!    coordinates and the scroll handler needs a cell.
//!
//! While the picker or the empty state is up, the pointer picks the overlay child
//! rather than a pane `DrawingArea`, so none of these controllers run — no
//! `is_open()` guard is needed here (do not add one).
//!
//! ## Focus: the pane areas are deliberately NOT focus targets
//!
//! Pane `DrawingArea`s are ephemeral — `detect_closes` destroys one whenever its
//! Neovim window closes — so they are never made focusable and nothing ever calls
//! `grab_focus` on them. GTK keyboard focus stays on the persistent
//! [`PaneInner::cmdline_area`], which is the focus-return target
//! [`PaneHandle::cmdline_widget`] already documents for the picker and the tree.
//! See `crates/wolf-gnome/src/README.md` for the defect this prevents.
//!
//! That invariant is no longer a structural argument only: it is executed end to
//! end by the cage smoke `click_then_close_keeps_keyboard_input_alive_under_cage`
//! in `crates/wolf-gnome/tests/smoke_headless.rs`, which CLICKS a pane, closes that
//! same pane, and asserts keys still reach Neovim afterwards. Read its doc comment
//! before touching any of this: making it red takes a THREE-part revert (restore
//! `focusable(true)`, restore `area.grab_focus()` in `on_pane_clicked`, AND neuter
//! the `rebuild_pane_area` guard below), and it proves the COMPOSITE invariant
//! rather than the `focusable(false)` half in isolation.
//!
//! The pane areas are not the only ephemeral widget under `pane_area`, though: the
//! inner `gtk::Paned` DIVIDERS are rebuilt from scratch on every
//! [`PaneInner::rebuild_pane_area`], and `GtkPaned` sets `focusable = TRUE` in
//! `gtk_paned_init` (GTK 4.22.4, `gtk/gtkpaned.c:1510`). We cannot make those
//! non-focusable — GTK owns them — so `rebuild_pane_area` carries a guard instead:
//! if the toplevel's focus widget is inside `pane_area` when the teardown starts,
//! focus is returned to `cmdline_area` once the rebuild has settled.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use wolf_core::{
    CursorShape, LayoutIntent, MouseAction, MouseEvent, PaneNode, ScrollEvent, Snapshot,
    SplitOrientation, WolfCore,
};

use crate::layout::{Orientation, PaneId, PaneLayout, PaneTree};
use crate::mouse::{self, PaneScroll};
use crate::pane_layout;
use crate::render::{draw_glyph, FALLBACK_BG, FALLBACK_FG};
use crate::state_seam;
use crate::{cell_at, cursor, grid_size_for, paint, CellMetrics};

/// The id of the first (root) pane. The [`PaneTree`] is constructed with this as
/// its root leaf, so the first bound pane reuses it (no `split` needed).
const FIRST_PANE_ID: PaneId = 1;

/// The lowest grid id that is an editor WINDOW grid. Grid 1 is the outer/global
/// grid (cmdline + messages); window content lives on grids `>= 2`.
const MIN_WINDOW_GRID: u64 = 2;

/// Height of the cmdline strip, in grid rows.
const CMDLINE_STRIP_ROWS: f64 = 1.0;

/// Width of the command-line caret, in pixels.
const CMDLINE_CARET_WIDTH: f64 = 2.0;

/// CSS class applied to the active pane's `DrawingArea` to draw a focus ring.
const ACTIVE_PANE_CSS: &str = "wolf-active-pane";

/// CSS defining [`ACTIVE_PANE_CSS`]. An `outline` (not a `border`) so the ring does
/// NOT consume layout space (no resize churn on focus changes). Loaded once.
const ACTIVE_PANE_CSS_DATA: &str =
    ".wolf-active-pane { outline: 2px solid #3584e4; outline-offset: -2px; }";

/// Divisor to convert an 8-bit color channel (0–255) to Cairo's 0.0–1.0 range.
const RGB_MAX: f64 = 255.0;

/// The widgets and pointer state bound to one pane: its Neovim window handle, its
/// grid (pending until `win_pos` is observed), its drawing area (reused across
/// rebuilds by id), and the per-pane mouse/scroll bookkeeping.
///
/// Every mutable field is a `Cell`, never a `RefCell`: all access is on the single
/// GTK main thread, and a `Cell` cannot have a borrow accidentally held across a
/// [`WolfCore`] call (the rule this whole module follows).
struct PaneWidgets {
    /// The Neovim window handle this pane is bound to.
    win: i64,
    /// The grid id this pane paints, or `None` until resolved via `grid_for_win`.
    grid: Cell<Option<u64>>,
    /// The pane's drawing area (reused by [`PaneId`] across tree rebuilds).
    area: gtk::DrawingArea,
    /// The grid captured at PRESS, pinning the drag target for the rest of the
    /// sequence (macOS's `mouseDragGrid`). Set on drag-begin, read on
    /// update/end, cleared on end. Pinning is what makes a drag that leaves the
    /// pane keep targeting the pane it STARTED in.
    drag_grid: Cell<Option<u64>>,
    /// The GDK button number captured at PRESS, pinned alongside `drag_grid` for
    /// the rest of the sequence. See [`PaneInner::send_pane_mouse`] for why the
    /// BUTTON is pinned while the MODIFIERS are deliberately re-read live.
    drag_button: Cell<Option<u32>>,
    /// This pane's scroll bookkeeping: the carried sub-threshold remainder plus
    /// the grid it was last applied to, as ONE pure value
    /// ([`crate::mouse::PaneScroll`]) so the reset-on-grid-change rule is
    /// unit-tested rather than living untestably in this file.
    scroll: Cell<PaneScroll>,
    /// The last WIDGET-LOCAL pointer position, recorded by the motion controller.
    /// Its ONLY consumer is the scroll handler, because GTK's `scroll` signal
    /// carries no coordinates of its own.
    pointer: Cell<(f64, f64)>,
}

impl PaneWidgets {
    /// Binds `area` to window `win` (and `grid`, when already resolved), with the
    /// pointer state zeroed.
    ///
    /// Both construction sites (`ensure_initial_pane`, `apply_split`) go through
    /// here, so a future field is one edit rather than two.
    fn new(win: i64, grid: Option<u64>, area: gtk::DrawingArea) -> Self {
        Self {
            win,
            grid: Cell::new(grid),
            area,
            drag_grid: Cell::new(None),
            drag_button: Cell::new(None),
            scroll: Cell::new(PaneScroll::new()),
            pointer: Cell::new((0.0, 0.0)),
        }
    }
}

/// A mouse/scroll target: a grid id plus that grid's LIVE dimensions, read from the
/// snapshot at the moment the event is handled.
///
/// The three fields travel together everywhere (`cell_at` needs the dims, the
/// `MouseEvent`/`ScrollEvent` needs the id), so they are one value — which also
/// keeps `send_pane_mouse` under clippy's argument-count threshold without an
/// `#[allow]`.
#[derive(Clone, Copy)]
struct GridTarget {
    /// The Neovim grid the event targets. Always a WINDOW grid (`>= 2`).
    grid: u64,
    /// That grid's live column count.
    cols: u16,
    /// That grid's live row count.
    rows: u16,
}

/// The pane view's shared, interior-mutable state. All access is on the single GTK
/// main thread, so `Cell`/`RefCell` (not a `Mutex`) suffices.
struct PaneInner {
    /// The pure pane-layout tree (split/remove/neighbour geometry).
    tree: RefCell<PaneTree>,
    /// The per-pane widgets, keyed by [`PaneId`].
    panes: RefCell<HashMap<PaneId, PaneWidgets>>,
    /// The next pane id to allocate (starts at [`FIRST_PANE_ID`]).
    next_pane_id: Cell<PaneId>,
    /// The root container: a vertical box of `[pane_area expands][cmdline strip]`.
    container: gtk::Box,
    /// Hosts the nested `gtk::Paned`/`DrawingArea` tree (rebuilt from `tree`).
    pane_area: gtk::Box,
    /// The one-row cmdline strip painting grid 1's last row.
    cmdline_area: gtk::DrawingArea,
    /// Shared cell metrics (measured once in `app.rs`).
    metrics: Rc<CellMetrics>,
    /// The live core, shared with the rest of the app.
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The currently active pane (focus ring + cursor target), if any.
    active_pane: Cell<Option<PaneId>>,
    /// A saved pane tree awaiting realization, set ONCE at launch by
    /// [`PaneHandle::request_layout_rebuild`] and consumed by the first wake at
    /// which a window grid exists (see [`PaneInner::consume_pending_layout`]).
    pending_layout: RefCell<Option<PaneNode>>,
}

impl PaneInner {
    /// Orchestrates a render wake: ensure a first pane exists, apply layout intents,
    /// collapse closed panes, resolve pending grids, refresh the active pane, then
    /// queue redraws.
    fn on_render_wake(self: &Rc<Self>) {
        if self.core.borrow().is_none() {
            return;
        }
        self.ensure_initial_pane();
        // A restored layout replaces the whole pane set, so nothing else on this
        // wake would be looking at the tree it just built. Early-return (macOS
        // parity) and let the NEXT wake drive the normal path.
        if self.consume_pending_layout() {
            return;
        }
        self.apply_intents();
        self.detect_closes();
        self.resolve_pending_grids();
        self.update_active_pane();
        self.queue_redraw();
    }

    /// Creates the first pane (reusing [`FIRST_PANE_ID`], the tree root) once a
    /// window grid (`>= 2`) appears. No-op once any pane exists.
    fn ensure_initial_pane(self: &Rc<Self>) {
        if !self.panes.borrow().is_empty() {
            return;
        }
        let win_grid = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            pick_initial_window(&core.read_snapshot())
        };
        let Some((win, grid)) = win_grid else {
            return;
        };
        let id = self.alloc_pane_id();
        let area = self.make_pane_area(id);
        self.panes
            .borrow_mut()
            .insert(id, PaneWidgets::new(win, Some(grid), area));
        self.rebuild_pane_area();
        self.set_active_pane(Some(id));
    }

    /// Consumes a pending restored layout, if one is both set and realizable now.
    /// Returns whether the layout was applied (the caller then skips the rest of
    /// the wake).
    ///
    /// Three non-applying cases, deliberately distinguished:
    ///
    /// - nothing pending → `false`;
    /// - no pane bound yet (no window grid has appeared) → `false` and the
    ///   pending value is **kept**, so the next wake retries;
    /// - more than one pane already exists → the pending value is **taken** and
    ///   dropped with a message. Splits made before the restore could run mean the
    ///   saved topology no longer describes this tab, and rebuilding over it would
    ///   discard live windows.
    fn consume_pending_layout(self: &Rc<Self>) -> bool {
        if self.pending_layout.borrow().is_none() {
            return false;
        }
        let pane_count = self.panes.borrow().len();
        if pane_count == 0 {
            // No window grid yet. Do NOT take the pending value.
            return false;
        }
        let Some(saved) = self.pending_layout.borrow_mut().take() else {
            return false;
        };
        if pane_count != 1 {
            eprintln!(
                "wolf-gnome: {pane_count} panes already exist at restore time; \
                 dropping the saved pane layout"
            );
            return false;
        }
        self.apply_layout_rebuild(&saved);
        true
    }

    /// Realizes a saved pane tree: ask nvim to rebuild the windows, then rebind
    /// every pane onto the returned handles in DFS order.
    ///
    /// The order below is load-bearing:
    ///
    /// 1. **Validate first.** [`pane_layout::decode`] rejects a malformed tree
    ///    BEFORE any spec is rendered, so nvim is never asked to realize one.
    /// 2. Render the spec via `wolf_core::state::rebuild_layout_spec`.
    /// 3. Call `rebuild_layout` under a SHORT borrow in its own statement.
    ///
    ///    **This is the only BLOCKING RPC `wolf-gnome` issues.** Every other core
    ///    call this crate makes is a snapshot read or a queued input, which is why
    ///    `NotResponding` was previously unreachable from this front-end. It is
    ///    acceptable HERE and nowhere else: it happens at most once per launch,
    ///    before the window is interactive, and never on the crash/restart path —
    ///    see `shell.rs`'s "never call a `block_on_rpc` on the restart path" rule,
    ///    which stands unchanged. A wedged nvim at launch can now genuinely mark
    ///    the core `NotResponding`, and the crash overlay presents it.
    /// 4. A handle count that does not match the decoded leaf count aborts,
    ///    leaving the single already-bound pane intact (macOS parity).
    fn apply_layout_rebuild(self: &Rc<Self>, saved: &PaneNode) {
        let first_id = self.next_pane_id.get();
        let Some((tree, leaf_ids)) = pane_layout::decode(saved, first_id) else {
            eprintln!("wolf-gnome: saved pane layout is unusable; keeping the fresh single pane");
            return;
        };
        let spec = wolf_core::state::rebuild_layout_spec(saved);
        let handles = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            core.rebuild_layout(&spec)
        };
        if handles.len() != leaf_ids.len() {
            eprintln!(
                "wolf-gnome: rebuild_layout returned {} window handles for {} saved leaves; \
                 keeping the fresh single pane",
                handles.len(),
                leaf_ids.len()
            );
            return;
        }

        let mut new_panes: HashMap<PaneId, PaneWidgets> = HashMap::new();
        for (win, id) in handles.iter().zip(leaf_ids.iter()) {
            let grid = {
                let core_ref = self.core.borrow();
                core_ref
                    .as_ref()
                    .and_then(|c| c.read_snapshot().grid_for_win(*win))
            };
            let area = self.make_pane_area(*id);
            new_panes.insert(*id, PaneWidgets::new(*win, grid, area));
        }

        *self.tree.borrow_mut() = tree;
        *self.panes.borrow_mut() = new_panes;
        self.next_pane_id
            .set(first_id + u64::try_from(leaf_ids.len()).unwrap_or(0));

        // The guarded path: it samples `focus_inside_pane_area()` BEFORE tearing
        // anything down, and its `pane_area.remove(..)` loop unparents the old
        // single pane's area — which, in the guaranteed one-pane pre-state, is
        // `pane_area`'s only child and is no longer in `panes`.
        self.rebuild_pane_area();
        self.set_active_pane(leaf_ids.first().copied());
        self.resolve_pending_grids();
        self.queue_redraw();
    }

    /// The tab's persistable pane tree, or `None` for an unnamed lone pane.
    ///
    /// Reads each leaf's buffer path from `WolfCore::pane_status(grid).abs_path`
    /// — the RAW absolute path, NOT `display_path` (which is relativized for the
    /// statusbar and would be useless to restore from). `pane_status` is RPC-free
    /// (a snapshot read plus `live_cwd`), so this is cheap enough to run on every
    /// wake.
    ///
    /// The `panes` borrow is dropped before the core borrow is taken; the `None`
    /// suppression is [`state_seam::persistable_layout`]'s CONTENT rule, never an
    /// arity test.
    fn layout_snapshot(&self) -> Option<PaneNode> {
        let leaves: Vec<(PaneId, Option<u64>)> = {
            let tree = self.tree.borrow();
            let panes = self.panes.borrow();
            tree.leaves()
                .into_iter()
                .map(|id| (id, panes.get(&id).and_then(|p| p.grid.get())))
                .collect()
        };
        let paths: HashMap<PaneId, String> = {
            let core_ref = self.core.borrow();
            let core = core_ref.as_ref()?;
            leaves
                .iter()
                .filter_map(|(id, grid)| grid.map(|g| (*id, core.pane_status(g).abs_path)))
                .collect()
        };
        let layout = self.tree.borrow().layout();
        state_seam::persistable_layout(pane_layout::encode(&layout, &paths))
    }

    /// `(panes with a RESOLVED grid, windows nvim has reported buffers for)` —
    /// the two readiness counts [`state_seam::may_snapshot`] gates on.
    fn snapshot_readiness(&self) -> (usize, usize) {
        let bound = self
            .panes
            .borrow()
            .values()
            .filter(|p| p.grid.get().is_some())
            .count();
        let reported = {
            let core_ref = self.core.borrow();
            core_ref
                .as_ref()
                .map_or(0, |core| core.read_snapshot().win_buffers.len())
        };
        (bound, reported)
    }

    /// Drains and applies all pending layout intents (splits + focus moves).
    ///
    /// [`LayoutIntent::DrawerOpen`] is drained and DROPPED: the terminal drawer is
    /// a macOS-only front-end feature today (`wolf-macos` renders it as a
    /// full-width, root-bottom pane held outside the pure `PaneTree`). wolf-gnome
    /// exposes no way to reach it — it binds no `wolf_toggle_terminal` trigger — so
    /// this arm is unreachable in practice. It exists so that adding a variant to
    /// the shared `wolf_core::LayoutIntent` cannot silently break the GTK build,
    /// and so the drop is a deliberate, documented decision rather than a `_`
    /// wildcard that would swallow the NEXT variant too. Building the GTK drawer is
    /// tracked as a wolf-gnome gap (see `crates/wolf-gnome/src/README.md`).
    fn apply_intents(self: &Rc<Self>) {
        let intents = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            core.drain_intents()
        };
        for intent in intents {
            match intent {
                LayoutIntent::Split {
                    orientation,
                    from,
                    win,
                } => self.apply_split(orientation, from, win),
                LayoutIntent::FocusDir { dir, from } => self.apply_focus(dir, from),
                LayoutIntent::DrawerOpen { .. } => {}
            }
        }
    }

    /// Applies a split intent: split the `from` pane (fallback: active pane), then
    /// bind a new pane to `win` (grid resolved now or pending). Idempotent if a pane
    /// already exists for `win`.
    fn apply_split(self: &Rc<Self>, orientation: SplitOrientation, from: i64, win: i64) {
        if self.pane_for_win(win).is_some() {
            return;
        }
        let Some(target) = self.pane_for_win(from).or_else(|| self.active_pane.get()) else {
            return;
        };
        let orient = match orientation {
            SplitOrientation::Vertical => Orientation::Vertical,
            SplitOrientation::Horizontal => Orientation::Horizontal,
        };
        let new_id = self.alloc_pane_id();
        self.tree.borrow_mut().split(target, new_id, orient);

        let grid = {
            let core_ref = self.core.borrow();
            core_ref
                .as_ref()
                .and_then(|c| c.read_snapshot().grid_for_win(win))
        };
        let area = self.make_pane_area(new_id);
        self.panes
            .borrow_mut()
            .insert(new_id, PaneWidgets::new(win, grid, area));
        self.rebuild_pane_area();
        self.set_active_pane(Some(new_id));
    }

    /// Applies a focus-direction intent: resolve the neighbour of the `from` pane
    /// (fallback: active pane) in `dir`, then `focus_win` its window.
    fn apply_focus(&self, dir: wolf_core::FocusDir, from: i64) {
        let width = f64::from(self.pane_area.width());
        let height = f64::from(self.pane_area.height());
        let Some(origin) = self.pane_for_win(from).or_else(|| self.active_pane.get()) else {
            return;
        };
        let target = self.tree.borrow().neighbor(origin, dir, width, height);
        let Some(target) = target else {
            return;
        };
        let win = self.panes.borrow().get(&target).map(|p| p.win);
        if let Some(win) = win {
            if let Some(core) = self.core.borrow().as_ref() {
                core.focus_win(win);
            }
        }
    }

    /// Detects panes whose bound grid has vanished (a `win_close`/`grid_destroy`)
    /// and collapses them from the tree, never emptying the tree.
    fn detect_closes(&self) {
        let closed: Vec<PaneId> = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            let snap = core.read_snapshot();
            let panes = self.panes.borrow();
            panes
                .iter()
                .filter_map(|(id, p)| match p.grid.get() {
                    Some(g) if snap.grid(g).is_none() => Some(*id),
                    _ => None,
                })
                .collect()
        };
        if closed.is_empty() {
            return;
        }

        {
            let mut tree = self.tree.borrow_mut();
            let mut panes = self.panes.borrow_mut();
            for id in &closed {
                // Never empty the tree (nvim guards last-window close, but be safe).
                if tree.leaves().len() <= 1 {
                    break;
                }
                tree.remove(*id);
                panes.remove(id);
            }
        }
        self.rebuild_pane_area();

        if self.active_pane.get().is_some_and(|a| closed.contains(&a)) {
            let first = self.tree.borrow().leaves().first().copied();
            self.set_active_pane(first);
        }
    }

    /// Resolves any pane whose grid is still pending via `grid_for_win`, sizing the
    /// newly bound grid to the pane's current allocation.
    fn resolve_pending_grids(&self) {
        let updates: Vec<(PaneId, u64, i32, i32)> = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            let snap = core.read_snapshot();
            let panes = self.panes.borrow();
            panes
                .iter()
                .filter_map(|(id, p)| {
                    if p.grid.get().is_some() {
                        return None;
                    }
                    let g = snap.grid_for_win(p.win)?;
                    Some((*id, g, p.area.width(), p.area.height()))
                })
                .collect()
        };
        // Every entry in `updates` is a pane whose grid transitioned None -> Some
        // this pass (the filter above requires `p.grid` was None and now resolves).
        let bound_any = !updates.is_empty();
        for (id, g, w, h) in updates {
            if let Some(p) = self.panes.borrow().get(&id) {
                p.grid.set(Some(g));
            }
            if w > 0 && h > 0 {
                let (cols, rows) = grid_size_for(w, h, self.metrics.cell_w, self.metrics.cell_h);
                if let Some(core) = self.core.borrow().as_ref() {
                    core.resize_grid(g, cols, rows);
                }
            }
        }
        // A pane created while its grid was still pending was skipped by
        // `build_layout_widget` (None-handling), so its DrawingArea was never
        // parented into the gtk::Paned tree. Now that the grid is bound, rebuild
        // so the area is inserted. Only rebuild when something actually changed —
        // an already-bound pane never re-enters `updates`, so this cannot loop.
        if bound_any {
            self.rebuild_pane_area();
        }
    }

    /// Refreshes the active pane from `active_win` (fallback: the pane owning the
    /// cursor's grid), updating the focus ring when it changes.
    fn update_active_pane(&self) {
        let target = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            let snap = core.read_snapshot();
            let panes = self.panes.borrow();
            let by_win = snap
                .active_win
                .and_then(|aw| panes.iter().find(|(_, p)| p.win == aw).map(|(id, _)| *id));
            by_win.or_else(|| {
                panes
                    .iter()
                    .find(|(_, p)| p.grid.get() == Some(snap.cursor.grid))
                    .map(|(id, _)| *id)
            })
        };
        if let Some(id) = target {
            if self.active_pane.get() != Some(id) {
                self.set_active_pane(Some(id));
            }
        }
    }

    /// Queues a redraw of every pane area and the cmdline strip.
    fn queue_redraw(&self) {
        {
            let panes = self.panes.borrow();
            for p in panes.values() {
                p.area.queue_draw();
            }
        }
        self.cmdline_area.queue_draw();
    }

    /// Allocates the next pane id.
    fn alloc_pane_id(&self) -> PaneId {
        let id = self.next_pane_id.get();
        self.next_pane_id.set(id + 1);
        id
    }

    /// Returns the pane bound to window handle `win`, if any.
    fn pane_for_win(&self, win: i64) -> Option<PaneId> {
        self.panes
            .borrow()
            .iter()
            .find(|(_, p)| p.win == win)
            .map(|(id, _)| *id)
    }

    /// Sets the active pane and refreshes the focus-ring CSS across all panes.
    fn set_active_pane(&self, pane: Option<PaneId>) {
        self.active_pane.set(pane);
        let panes = self.panes.borrow();
        for (pid, p) in panes.iter() {
            if Some(*pid) == pane {
                p.area.add_css_class(ACTIVE_PANE_CSS);
            } else {
                p.area.remove_css_class(ACTIVE_PANE_CSS);
            }
        }
    }

    /// Builds a pane's `DrawingArea`, wiring its draw func (paint its grid + cursor),
    /// resize (per-grid + global resize), and the three pointer controllers.
    ///
    /// The area is deliberately NOT `focusable`: it is destroyed whenever its Neovim
    /// window closes, and a destroyed focus widget leaves the toplevel with none —
    /// see this module's "Focus" section and `crates/wolf-gnome/src/README.md`.
    fn make_pane_area(self: &Rc<Self>, pane_id: PaneId) -> gtk::DrawingArea {
        let area = gtk::DrawingArea::builder()
            .hexpand(true)
            .vexpand(true)
            .build();
        // A one-cell minimum so that, with `shrink-*-child(false)` on the enclosing
        // paneds, a child can never be allocated to zero pixels (a starved pane).
        area.set_content_width(cell_floor(self.metrics.cell_w));
        area.set_content_height(cell_floor(self.metrics.cell_h));

        {
            let weak = Rc::downgrade(self);
            area.set_draw_func(move |_area, cr, _w, _h| {
                if let Some(inner) = weak.upgrade() {
                    inner.draw_pane(cr, pane_id);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            area.connect_resize(move |_area, w, h| {
                if let Some(inner) = weak.upgrade() {
                    inner.on_pane_resize(pane_id, w, h);
                }
            });
        }
        // ONE gesture for the whole press -> drag -> release sequence.
        //
        // Not a `GestureClick` (+ anything): a `GtkGesture` takes an implicit grab
        // on the sequence and keeps delivering after the pointer leaves the widget,
        // which is exactly what drag-past-the-edge selection needs — an
        // `EventControllerMotion` would silently stop at the pane edge. And one
        // gesture rather than two removes any question of the pair fighting over
        // sequence claiming.
        //
        // VERIFIED against the GTK 4.22.4 C source (the version this devShell
        // builds against): `GtkGestureDrag` installs `gesture_class->begin`, and
        // `GtkGesture::handle_event` emits BEGIN straight from `GDK_BUTTON_PRESS`
        // once the point count matches `n-points` (default 1). `drag-begin` is
        // therefore NOT motion-gated — a plain press with no movement fires it, so
        // the press path (focus + `MouseAction::Press`) is sound.
        {
            let weak = Rc::downgrade(self);
            let gesture = gtk::GestureDrag::new();
            // Explicit rather than leaning on `GtkGestureSingle`'s default, so the
            // "primary button only" decision is visible where it is made (the pure
            // `mouse::mouse_input` seam refuses every other button as well).
            gesture.set_button(gdk::BUTTON_PRIMARY);
            {
                let weak = weak.clone();
                gesture.connect_drag_begin(move |g, x, y| {
                    if let Some(inner) = weak.upgrade() {
                        inner.on_pane_press(pane_id, x, y, g);
                    }
                });
            }
            {
                let weak = weak.clone();
                gesture.connect_drag_update(move |g, off_x, off_y| {
                    if let Some(inner) = weak.upgrade() {
                        inner.on_pane_drag(pane_id, off_x, off_y, g);
                    }
                });
            }
            gesture.connect_drag_end(move |g, off_x, off_y| {
                if let Some(inner) = weak.upgrade() {
                    inner.on_pane_release(pane_id, off_x, off_y, g);
                }
            });
            area.add_controller(gesture);
        }

        // Vertical scroll. `VERTICAL` alone on purpose:
        //   * NOT `DISCRETE` — that would move the accumulation inside GTK, where
        //     the sub-cell remainder policy, the per-unit threshold and the
        //     reset-on-grid-change rule could not be unit-tested. They belong in
        //     the pure `mouse` seam.
        //   * NOT `BOTH_AXES` — horizontal scroll is not handled (macOS parity), so
        //     claiming horizontal events would be dishonest.
        //   * NOT `KINETIC` — that only adds the `decelerate` momentum signal, which
        //     macOS does not special-case either.
        // The handler always stops propagation: a scroll over a pane belongs to that
        // pane, and no ancestor (Paned -> Overlay -> Paned -> Box) scrolls.
        {
            let weak = Rc::downgrade(self);
            let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
            scroll.connect_scroll(move |controller, _dx, dy| {
                if let Some(inner) = weak.upgrade() {
                    inner.on_pane_scroll(pane_id, dy, controller.unit());
                }
                glib::Propagation::Stop
            });
            area.add_controller(scroll);
        }

        // Position recording ONLY. GTK's `scroll` signal carries no coordinates, so
        // this is the sole way to give the scroll handler a cell to aim at. It is
        // explicitly NOT used for drag — it stops delivering the moment the pointer
        // leaves the widget, which is precisely what the gesture above avoids.
        {
            let weak = Rc::downgrade(self);
            let motion = gtk::EventControllerMotion::new();
            {
                let weak = weak.clone();
                motion.connect_motion(move |_c, x, y| {
                    if let Some(inner) = weak.upgrade() {
                        inner.record_pointer(pane_id, x, y);
                    }
                });
            }
            motion.connect_enter(move |_c, x, y| {
                if let Some(inner) = weak.upgrade() {
                    inner.record_pointer(pane_id, x, y);
                }
            });
            area.add_controller(motion);
        }

        area
    }

    /// Paints pane `pane_id`: its bound grid (fallback active grid) plus the cursor
    /// iff this pane owns the cursor's grid.
    fn draw_pane(&self, cr: &cairo::Context, pane_id: PaneId) {
        let core_ref = self.core.borrow();
        let Some(core) = core_ref.as_ref() else {
            return;
        };
        let snap = core.read_snapshot();
        let grid_id = self.panes.borrow().get(&pane_id).and_then(|p| p.grid.get());
        let grid = grid_id
            .and_then(|g| snap.grid(g))
            .unwrap_or_else(|| snap.active_grid());
        paint(cr, grid, &snap.highlights, &self.metrics);
        if let Some(g) = grid_id {
            if snap.cursor.grid == g {
                draw_cursor(cr, &snap, &self.metrics);
            }
        }
    }

    /// Handles a pane area resize: resize the pane's own grid, then the global grid.
    fn on_pane_resize(&self, pane_id: PaneId, width: i32, height: i32) {
        let grid = self.panes.borrow().get(&pane_id).and_then(|p| p.grid.get());
        if let Some(grid) = grid {
            if width > 0 && height > 0 {
                let (cols, rows) =
                    grid_size_for(width, height, self.metrics.cell_w, self.metrics.cell_h);
                if let Some(core) = self.core.borrow().as_ref() {
                    core.resize_grid(grid, cols, rows);
                }
            }
        }
        self.resize_global();
    }

    /// Resizes the outer/global grid (grid 1) to the full editor area, so the
    /// cmdline spans the width and nvim's overall layout tracks the window.
    fn resize_global(&self) {
        let width = self.pane_area.width();
        let height = self.pane_area.height() + self.cmdline_area.height();
        if width <= 0 || height <= 0 {
            return;
        }
        let (cols, rows) = grid_size_for(width, height, self.metrics.cell_w, self.metrics.cell_h);
        if let Some(core) = self.core.borrow().as_ref() {
            core.resize(cols, rows);
        }
    }

    /// Click-to-focus: focus the clicked pane's Neovim window, mark it active, and
    /// park GTK keyboard focus on the PERSISTENT cmdline strip.
    ///
    /// The GTK focus target is deliberately the cmdline strip and NOT the pane's own
    /// `DrawingArea`. Pane areas are ephemeral — `detect_closes` destroys one as soon
    /// as its Neovim window closes — and GTK does not hand focus back when the focus
    /// widget is destroyed, so the toplevel is left with no focus widget and
    /// window-level key dispatch dies outright (keys stop reaching BOTH nvim and the
    /// `wolf-gnome` chord controller). Focusing the strip instead is the same
    /// focus-return contract [`PaneHandle::cmdline_widget`] already gives the picker
    /// and the file tree, and it still pulls focus out of the sidebar list on a
    /// click. See `crates/wolf-gnome/src/README.md`.
    fn on_pane_clicked(&self, pane_id: PaneId) {
        let win = self.panes.borrow().get(&pane_id).map(|p| p.win);
        if let Some(win) = win {
            if let Some(core) = self.core.borrow().as_ref() {
                core.focus_win(win);
            }
        }
        self.set_active_pane(Some(pane_id));
        self.cmdline_area.grab_focus();
    }

    /// The pane's bound grid plus that grid's LIVE dimensions from the snapshot.
    ///
    /// `None` when the pane is unbound, or its grid is not (yet) in the snapshot —
    /// in which case no mouse or scroll event is sent at all, because there is no
    /// grid to name and no dimensions to clamp against.
    ///
    /// The snapshot guard is dropped before this returns; nothing here calls into
    /// [`WolfCore`] while holding it.
    fn grid_and_dims(&self, pane_id: PaneId) -> Option<GridTarget> {
        let grid = self
            .panes
            .borrow()
            .get(&pane_id)
            .and_then(|p| p.grid.get())?;
        self.dims_for_grid(grid)
    }

    /// The LIVE dimensions of `grid`, as a [`GridTarget`].
    ///
    /// Used by the drag/release path, which must NOT re-resolve the pane's current
    /// grid: the drag targets the grid PINNED at press, so following a rebind
    /// mid-drag would silently retarget the selection.
    ///
    /// `usize -> u16` goes through `try_from`, never `as`: a grid wider than
    /// `u16::MAX` is impossible in practice, but saturating is the only honest
    /// answer if it ever happened, and a wrapping cast would produce a cell inside
    /// a grid that does not exist.
    fn dims_for_grid(&self, grid: u64) -> Option<GridTarget> {
        let core_ref = self.core.borrow();
        let core = core_ref.as_ref()?;
        let snap = core.read_snapshot();
        let g = snap.grid(grid)?;
        Some(GridTarget {
            grid,
            cols: u16::try_from(g.cols).unwrap_or(u16::MAX),
            rows: u16::try_from(g.rows).unwrap_or(u16::MAX),
        })
    }

    /// Press: FOCUS FIRST, then the mouse event, then pin the drag grid + button.
    ///
    /// The ordering is the macOS ordering and it is guaranteed rather than hoped
    /// for: `focus_win` and `send_mouse` are enqueued on the SAME ordered input
    /// channel, so nvim sees the focus change before the press.
    fn on_pane_press(self: &Rc<Self>, pane_id: PaneId, x: f64, y: f64, g: &gtk::GestureDrag) {
        // Reusing the existing handler verbatim is what preserves focus-on-click.
        self.on_pane_clicked(pane_id);
        let Some(target) = self.grid_and_dims(pane_id) else {
            return;
        };
        // The press is the ONE callback that reads the live button; every later
        // one reuses this number (see `send_pane_mouse`).
        let button = g.current_button();
        self.send_pane_mouse(target, MouseAction::Press, button, x, y, g);
        if let Some(p) = self.panes.borrow().get(&pane_id) {
            p.drag_grid.set(Some(target.grid));
            p.drag_button.set(Some(button));
        }
    }

    /// Drag: forwarded UNCONDITIONALLY on every callback — no throttling and no
    /// "only when the cell changed" dedupe, matching macOS's `mouseDragged`.
    fn on_pane_drag(&self, pane_id: PaneId, off_x: f64, off_y: f64, g: &gtk::GestureDrag) {
        let Some((target, button, x, y)) = self.drag_point(pane_id, off_x, off_y, g) else {
            return;
        };
        self.send_pane_mouse(target, MouseAction::Drag, button, x, y, g);
    }

    /// Release at the current point, then unpin the drag grid and button.
    fn on_pane_release(&self, pane_id: PaneId, off_x: f64, off_y: f64, g: &gtk::GestureDrag) {
        if let Some((target, button, x, y)) = self.drag_point(pane_id, off_x, off_y, g) {
            self.send_pane_mouse(target, MouseAction::Release, button, x, y, g);
        }
        // Unpin even when the send was skipped, so a failed lookup cannot strand a
        // stale grid (or button) into the NEXT drag.
        if let Some(p) = self.panes.borrow().get(&pane_id) {
            p.drag_grid.set(None);
            p.drag_button.set(None);
        }
    }

    /// Reconstructs the absolute widget-local point of a drag update/end and pairs
    /// it with the grid AND button PINNED at press.
    ///
    /// `GestureDrag`'s update/end callbacks carry OFFSETS FROM THE START POINT, not
    /// absolute coordinates, so the point is `start_point() + offset`. That
    /// reconstructed point routinely lands OUTSIDE the widget — negative, or past
    /// the right/bottom edge — and nothing here rejects it: [`cell_at`] clamps it to
    /// the nearest edge cell, which is exactly what makes drag-past-the-edge
    /// selection extend to the last row/column instead of stalling.
    ///
    /// Returns `None` when the gesture is no longer active (no start point) or the
    /// sequence never pinned a grid/button.
    fn drag_point(
        &self,
        pane_id: PaneId,
        off_x: f64,
        off_y: f64,
        g: &gtk::GestureDrag,
    ) -> Option<(GridTarget, u32, f64, f64)> {
        let (start_x, start_y) = g.start_point()?;
        let (grid, button) = {
            let panes = self.panes.borrow();
            let p = panes.get(&pane_id)?;
            (p.drag_grid.get()?, p.drag_button.get()?)
        };
        // The PINNED grid's dims, re-read live — NOT `grid_and_dims(pane_id)`, which
        // would follow a rebind and retarget the drag mid-sequence.
        let target = self.dims_for_grid(grid)?;
        Some((target, button, start_x + off_x, start_y + off_y))
    }

    /// Encodes button + modifiers via the pure [`mouse::mouse_input`] seam, maps the
    /// widget-local point to a grid cell via [`cell_at`], and sends the event.
    ///
    /// A button the seam refuses (anything but the primary) sends nothing at all.
    ///
    /// **The BUTTON is pinned at press; the MODIFIERS are not, and that asymmetry is
    /// deliberate.** `button` is the number [`Self::on_pane_press`] captured and
    /// stashed in `drag_button`, replayed for every drag/release callback of the
    /// sequence rather than re-read from the gesture. The GTK 4.22.4 source says
    /// `current_button()` is still valid inside `drag-end` (the button is cleared
    /// only AFTER the signal emission), but nothing in this repo's harness can
    /// execute that path (`crates/wolf-gnome/tests/README.md`, "Deliberate test
    /// gaps"), and if it ever reported `0` the seam would refuse it, the `Release`
    /// would be dropped silently, and nvim would be stranded mid-drag in visual
    /// mode. Pinning costs one `Cell` and removes that failure mode entirely. The
    /// MODIFIERS, by contrast, are re-read LIVE on every callback — macOS parity
    /// (`PaneContainerView.mouseEvent` reads `event.modifierFlags` per event), so a
    /// modifier pressed or released mid-drag takes effect at once. Both still go
    /// through the one pure seam, which is why the pinned value is the raw GDK
    /// NUMBER rather than an already-translated `wolf_core::MouseButton`: the seam
    /// stays the single place that decides which buttons are forwarded and how the
    /// modifier bits map.
    fn send_pane_mouse(
        &self,
        target: GridTarget,
        action: MouseAction,
        button: u32,
        x: f64,
        y: f64,
        g: &gtk::GestureDrag,
    ) {
        let Some(input) = mouse::mouse_input(button, g.current_event_state()) else {
            return;
        };
        let (row, col) = cell_at(
            x,
            y,
            self.metrics.cell_w,
            self.metrics.cell_h,
            target.cols,
            target.rows,
        );
        if let Some(core) = self.core.borrow().as_ref() {
            core.send_mouse(MouseEvent {
                button: input.button,
                action,
                grid: target.grid,
                row,
                col,
                ctrl: input.ctrl,
                shift: input.shift,
                alt: input.alt,
            });
        }
    }

    /// Scroll: pick the threshold from the reported unit, hand the delta to this
    /// pane's [`PaneScroll`], and emit that many discrete `ScrollEvent`s.
    ///
    /// The reset-the-carry-on-a-grid-change rule is NOT here: it lives inside
    /// [`PaneScroll::steps_for`], where it is decidable from its arguments and
    /// unit-tested headlessly. That move was made when pointer input had no
    /// end-to-end coverage on Linux at all, so a rule left in this file was a rule
    /// nothing verified. This method IS covered end to end now —
    /// `smoke_headless.rs`'s `wheel_scroll_moves_the_viewport_under_cage` drives it
    /// from a real `wl_pointer.axis` frame and pins one detent to exactly one line
    /// — but the rule stays in `PaneScroll`: discriminating a per-grid carry would
    /// need two panes and a pointer that scrolls across both, which no smoke does,
    /// and a rule decidable from its arguments is far better pinned headlessly.
    ///
    /// **This method NEVER changes focus** — no `focus_win`, no `set_active_pane`,
    /// no `grab_focus`. Scrolling a background pane must leave the keyboard exactly
    /// where it was.
    fn on_pane_scroll(&self, pane_id: PaneId, dy: f64, unit: gdk::ScrollUnit) {
        let Some(target) = self.grid_and_dims(pane_id) else {
            return;
        };
        let threshold = mouse::scroll_threshold(unit, self.metrics.cell_h);

        // One scoped borrow: accumulate and read the recorded pointer, then drop it
        // before touching the core. `PaneScroll` is `Copy`, so this is a
        // get/modify/set rather than a held borrow.
        let Some((steps, pointer)) = ({
            let panes = self.panes.borrow();
            panes.get(&pane_id).map(|p| {
                let mut scroll = p.scroll.get();
                let steps = scroll.steps_for(target.grid, dy, threshold);
                p.scroll.set(scroll);
                (steps, p.pointer.get())
            })
        }) else {
            return;
        };

        let Some((direction, count)) = mouse::scroll_steps(steps) else {
            return;
        };
        // GTK's `scroll` signal carries no coordinates, so the cell comes from the
        // position the motion controller recorded.
        let (px, py) = pointer;
        let (row, col) = cell_at(
            px,
            py,
            self.metrics.cell_w,
            self.metrics.cell_h,
            target.cols,
            target.rows,
        );
        if let Some(core) = self.core.borrow().as_ref() {
            for _ in 0..count {
                core.send_scroll(ScrollEvent {
                    direction,
                    grid: target.grid,
                    row,
                    col,
                });
            }
        }
    }

    /// Records the last widget-local pointer position for the scroll handler.
    fn record_pointer(&self, pane_id: PaneId, x: f64, y: f64) {
        if let Some(p) = self.panes.borrow().get(&pane_id) {
            p.pointer.set((x, y));
        }
    }

    /// Paints the native command-line / message strip (OD4 minimal-but-correct).
    ///
    /// Under `ext_messages` (which implicitly enables `ext_cmdline` and sets
    /// `cmdheight=0`) the outer grid 1 empties, so this paints from the folded
    /// [`Snapshot::cmdline`] / [`Snapshot::toast`] rather than grid 1's last row:
    /// when a command line is active, `firstc` + the flattened cmdline text with a
    /// caret at the byte position `pos`; otherwise the LAST toast line (so the
    /// emptied strip never shows blank). The Adw completion popup + the multi-line
    /// scrollable toast are deferred (OD4).
    fn draw_cmdline(&self, cr: &cairo::Context, width: f64) {
        let core_ref = self.core.borrow();
        let Some(core) = core_ref.as_ref() else {
            return;
        };
        let snap = core.read_snapshot();
        let metrics = &self.metrics;

        // Default colors for the strip (grid 1 is empty under `cmdheight=0`).
        let style = snap.highlights.resolve(0);
        let fg = style.fg.unwrap_or(FALLBACK_FG);
        let bg = style.bg.unwrap_or(FALLBACK_BG);

        // Background band so the emptied grid 1 never shows blank/stale.
        cr.set_source_rgb(
            f64::from(bg.r) / RGB_MAX,
            f64::from(bg.g) / RGB_MAX,
            f64::from(bg.b) / RGB_MAX,
        );
        cr.rectangle(0.0, 0.0, width, metrics.cell_h);
        cr.fill().expect("cairo cmdline background fill failed");

        if snap.cmdline.active {
            // `firstc` + the flattened command-line text on the strip's single row.
            let text = snap.cmdline.text();
            let line = format!("{}{}", snap.cmdline.firstc, text);
            if !line.is_empty() {
                draw_glyph(cr, metrics, 0.0, 0.0, &line, fg, &style);
            }
            // Caret at the byte offset `pos` (snapped to a char boundary), placed
            // after the `firstc` prefix. Column-approximate (single-width cells).
            let mut byte = snap.cmdline.pos.min(text.len());
            while byte > 0 && !text.is_char_boundary(byte) {
                byte -= 1;
            }
            let caret_cols = snap.cmdline.firstc.chars().count() + text[..byte].chars().count();
            let caret_x = caret_cols as f64 * metrics.cell_w;
            cr.set_source_rgb(
                f64::from(fg.r) / RGB_MAX,
                f64::from(fg.g) / RGB_MAX,
                f64::from(fg.b) / RGB_MAX,
            );
            cr.rectangle(caret_x, 0.0, CMDLINE_CARET_WIDTH, metrics.cell_h);
            cr.fill().expect("cairo cmdline caret fill failed");
        } else if snap.toast.visible {
            // Inactive: paint the LAST toast line so the strip is not blank (the
            // multi-line scrollable toast is deferred — OD4).
            if let Some(last_line) = snap.toast.lines().last() {
                let text: String = last_line.iter().map(|c| c.text.as_str()).collect();
                if !text.is_empty() {
                    draw_glyph(cr, metrics, 0.0, 0.0, &text, fg, &style);
                }
            }
        }
    }

    /// Rebuilds `pane_area`'s child widget tree from the current [`PaneTree`],
    /// REUSING each pane's `DrawingArea` by id (so bound grids are preserved).
    ///
    /// The pane `DrawingArea`s survive (they are only detached and re-parented),
    /// but every inner `gtk::Paned` [`build_chain`] created is DESTROYED here, on
    /// every call — and a `GtkPaned` is `focusable` by default
    /// (`gtk_paned_init` calls `gtk_widget_set_focusable (…, TRUE)`, GTK 4.22.4
    /// `gtk/gtkpaned.c:1510`). So a divider that held GTK focus would be destroyed
    /// with no focus-return, which is the SAME wedge the pane areas' missing
    /// `focusable(true)` prevents structurally. Hence the guard below: if focus is
    /// anywhere inside `pane_area` when the teardown starts, it is parked on the
    /// persistent `cmdline_area` afterwards. See this module's "Focus" section and
    /// `crates/wolf-gnome/src/README.md`.
    fn rebuild_pane_area(&self) {
        // Must be sampled BEFORE anything is unparented: once the focus widget is
        // gone the root reports no focus at all and there is nothing left to test.
        let had_focus = self.focus_inside_pane_area();

        // Detach every reusable pane area from its current parent so it can be
        // re-inserted into the freshly built widget tree.
        {
            let panes = self.panes.borrow();
            for p in panes.values() {
                if p.area.parent().is_some() {
                    p.area.unparent();
                }
            }
        }
        while let Some(child) = self.pane_area.first_child() {
            self.pane_area.remove(&child);
        }
        {
            let layout = self.tree.borrow().layout();
            let panes = self.panes.borrow();
            if let Some(widget) = build_layout_widget(&layout, &panes) {
                widget.set_hexpand(true);
                widget.set_vexpand(true);
                self.pane_area.append(&widget);
            }
        }

        // AFTER the rebuild, so the strip is focused against the settled tree.
        if had_focus {
            self.cmdline_area.grab_focus();
        }
    }

    /// Resets the pane view to its pre-first-pane state after a SUCCESSFUL in-place
    /// Neovim restart, so the next render wake re-creates the first pane against
    /// the FRESH nvim's window and grid. See
    /// [`PaneHandle::reset_after_restart`] for why this is load-bearing rather
    /// than cosmetic.
    fn reset_after_restart(&self) {
        *self.tree.borrow_mut() = PaneTree::new(FIRST_PANE_ID);
        self.panes.borrow_mut().clear();
        self.next_pane_id.set(FIRST_PANE_ID);
        self.active_pane.set(None);
        // Unparents and destroys everything and builds nothing (an empty `panes`
        // map makes `build_layout_widget` return `None` for the root leaf) — and,
        // critically, runs its own `focus_inside_pane_area()` guard, parking focus
        // on the persistent `cmdline_area` BEFORE the widgets go away. Reusing
        // that path rather than open-coding the teardown is what keeps the
        // focus-wedge fix intact through a restart.
        self.rebuild_pane_area();
    }

    /// Whether the toplevel's focus widget is `pane_area` itself or anything under
    /// it — i.e. whether [`Self::rebuild_pane_area`]'s teardown is about to destroy
    /// the focus widget.
    ///
    /// `false` when the widget is not yet rooted (no window), which is the case for
    /// the very first rebuild, and `false` in the normal steady state: nothing under
    /// `pane_area` is focusable by wolf-gnome's own doing. It is the `gtk::Paned`
    /// dividers that make this reachable at all.
    fn focus_inside_pane_area(&self) -> bool {
        let Some(root) = self.pane_area.root() else {
            return false;
        };
        let Some(focus) = root.focus() else {
            return false;
        };
        focus.is_ancestor(&self.pane_area) || &focus == self.pane_area.upcast_ref::<gtk::Widget>()
    }
}

/// A handle to the built pane view, used by the app to place its widget, feed it
/// render wakes, and obtain the cmdline strip (the focus-return target for the
/// picker/tree).
#[derive(Clone)]
pub struct PaneHandle {
    inner: Rc<PaneInner>,
}

impl PaneHandle {
    /// Returns the pane view's root widget (the vertical box of pane area + cmdline
    /// strip) for placement in the app's editor overlay.
    #[must_use]
    pub fn widget(&self) -> gtk::Widget {
        self.inner.container.clone().upcast::<gtk::Widget>()
    }

    /// Returns the cmdline strip drawing area. It is a focusable widget in the
    /// editor region, handed to the picker/tree as their focus-return target so keys
    /// keep bubbling to the window-level controller after they close.
    #[must_use]
    pub fn cmdline_widget(&self) -> gtk::DrawingArea {
        self.inner.cmdline_area.clone()
    }

    /// Reacts to a render wake (see [`PaneInner::on_render_wake`]).
    pub fn on_render_wake(&self) {
        self.inner.on_render_wake();
    }

    /// Queues a saved pane tree to be realized ONCE, on the first wake at which a
    /// window grid exists.
    ///
    /// Called from `app::build_ui` before `present()`, with the layout
    /// `state_seam::restore_plan` attached. It cannot be applied here: no Neovim
    /// window has appeared yet, so there is nothing to rebuild from. See
    /// [`PaneInner::consume_pending_layout`] for the three outcomes.
    pub fn request_layout_rebuild(&self, saved: PaneNode) {
        *self.inner.pending_layout.borrow_mut() = Some(saved);
    }

    /// The tab's persistable pane tree for `state.json`, or `None` for an
    /// unnamed lone pane (see [`PaneInner::layout_snapshot`]).
    #[must_use]
    pub fn layout_snapshot(&self) -> Option<PaneNode> {
        self.inner.layout_snapshot()
    }

    /// `(panes with a resolved grid, windows nvim has reported buffers for)`, for
    /// `state_seam::may_snapshot` (see [`PaneInner::snapshot_readiness`]).
    #[must_use]
    pub fn snapshot_readiness(&self) -> (usize, usize) {
        self.inner.snapshot_readiness()
    }

    /// Resets the pane view to its pre-first-pane state after a SUCCESSFUL in-place
    /// Neovim restart ([`crate::shell::AppShell`] calls this from its restart
    /// orchestration). The next render wake re-creates the first pane against the
    /// FRESH nvim's window/grid.
    ///
    /// ## Why this is load-bearing, not cosmetic — the grid-size argument
    ///
    /// `WolfCore::restart_nvim` re-attaches the fresh nvim with
    /// `NvimConfig::initial_cols`/`initial_rows` — 80x24 for `wolf-gnome`
    /// ([`crate::app`]'s `INITIAL_COLS`/`INITIAL_ROWS`). The live pane is a very
    /// different size (45x14 cells in the cage smoke's window), and the ONLY thing
    /// that ever tells the core otherwise is a `::resize` on a pane
    /// `DrawingArea`, which GTK emits only when a widget's allocation CHANGES.
    /// After a restart the old `DrawingArea` is still there at exactly the same
    /// size, so no `::resize` fires — and the fresh nvim would keep an 80x24 grid
    /// inside a 45x14 pane for the rest of the session, rendering the editor at
    /// the wrong size with no way back.
    ///
    /// Destroying the old area and letting [`PaneInner::ensure_initial_pane`]
    /// create a new one means GTK allocates a FRESH widget (whose recorded size
    /// starts at 0x0), which necessarily emits `::resize` ->
    /// [`PaneInner::on_pane_resize`] -> `resize_grid` + `resize_global`. That
    /// chain is what re-establishes the correct grid size, and it is exactly what
    /// `nvim_crash_recovers_and_reopens_the_file_under_cage`'s
    /// `winwidth`/`winheight` equality assertion discriminates — deleting this
    /// call turns those dimensions into the config's 80x24 while every OCR term
    /// still passes, because the buffer text is simply drawn at the top-left of an
    /// over-large grid.
    ///
    /// ## Known v1 limitations
    ///
    /// - A crash while the window was SPLIT leaves the dead nvim's extra
    ///   window/grid entries in the snapshot: no `win_close`/`grid_destroy` is
    ///   ever sent for a killed process, and nothing clears the snapshot on
    ///   restart. The reset binds only the fresh nvim's single window, which is
    ///   correct; the stale entries linger harmlessly because no pane is bound to
    ///   them.
    /// - The pane TOPOLOGY is not restored ACROSS A CRASH RESTART — only the
    ///   Tier-1 file set is. Do not confuse this with the QUIT/RELAUNCH case,
    ///   which the layout persistence in this module's "Layout persistence"
    ///   section DOES restore: that path reads a saved tree out of `state.json`
    ///   at launch, whereas an in-place restart has no saved tree to read and
    ///   deliberately rebinds a single fresh pane. Restoring the split layout
    ///   (and cursor/scroll position) across a RESTART remains out of scope.
    pub fn reset_after_restart(&self) {
        self.inner.reset_after_restart();
    }
}

/// Builds the pane view (container, pane area host, cmdline strip) and returns a
/// [`PaneHandle`]. No panes exist until the first window grid appears on a wake.
#[must_use]
pub fn build(core: &Rc<RefCell<Option<WolfCore>>>, metrics: &Rc<CellMetrics>) -> PaneHandle {
    install_pane_css();

    let cmdline_area = gtk::DrawingArea::builder()
        .hexpand(true)
        .vexpand(false)
        .focusable(true)
        .build();
    cmdline_area.set_content_height(strip_height(metrics.cell_h));

    let pane_area = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pane_area.set_hexpand(true);
    pane_area.set_vexpand(true);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.append(&pane_area);
    container.append(&cmdline_area);

    let inner = Rc::new(PaneInner {
        tree: RefCell::new(PaneTree::new(FIRST_PANE_ID)),
        panes: RefCell::new(HashMap::new()),
        next_pane_id: Cell::new(FIRST_PANE_ID),
        container,
        pane_area,
        cmdline_area: cmdline_area.clone(),
        metrics: metrics.clone(),
        core: core.clone(),
        active_pane: Cell::new(None),
        pending_layout: RefCell::new(None),
    });

    {
        let weak = Rc::downgrade(&inner);
        cmdline_area.set_draw_func(move |_area, cr, w, _h| {
            if let Some(inner) = weak.upgrade() {
                inner.draw_cmdline(cr, f64::from(w));
            }
        });
    }

    PaneHandle { inner }
}

/// Draws the mode-aware cursor at the snapshot's cursor cell (moved from `app.rs`).
///
/// The shape follows the current mode's [`CursorShape`] (block in normal mode, a
/// thin bar otherwise). The fill is opaque and contrasting via
/// [`cursor::cursor_colors`]; for a block cursor the glyph is redrawn inverted.
fn draw_cursor(cr: &cairo::Context, snap: &Snapshot, metrics: &CellMetrics) {
    let mode = snap.mode_infos.get(snap.cursor.mode_idx);
    let shape = mode.map_or(CursorShape::Block, |m| m.shape);
    let pct = mode.and_then(|m| m.cell_percentage);

    let (fill, glyph) = cursor::cursor_colors(snap);

    let cell_x = snap.cursor.col as f64 * metrics.cell_w;
    let cell_y = snap.cursor.row as f64 * metrics.cell_h;
    let (x, y, w, h) =
        cursor::cursor_rect(shape, cell_x, cell_y, metrics.cell_w, metrics.cell_h, pct);

    cr.set_source_rgb(
        f64::from(fill.r) / RGB_MAX,
        f64::from(fill.g) / RGB_MAX,
        f64::from(fill.b) / RGB_MAX,
    );
    cr.rectangle(x, y, w, h);
    cr.fill().expect("cairo cursor fill failed");

    if shape == CursorShape::Block {
        if let Some(cell) = snap.active_grid().cell(snap.cursor.row, snap.cursor.col) {
            if !cell.text.is_empty() {
                let style = snap.highlights.resolve(cell.hl_id);
                draw_glyph(cr, metrics, cell_x, cell_y, &cell.text, glyph, &style);
            }
        }
    }
}

/// Recursively builds the GTK widget for a [`PaneLayout`], reusing each leaf's
/// `DrawingArea` by id. Returns `None` for a leaf whose pane is not (yet) bound. An
/// n-ary split is rendered as a RIGHT-LEANING chain of binary `gtk::Paned`s (see
/// [`build_chain`]).
fn build_layout_widget(
    layout: &PaneLayout,
    panes: &HashMap<PaneId, PaneWidgets>,
) -> Option<gtk::Widget> {
    match layout {
        PaneLayout::Leaf(id) => panes
            .get(id)
            .map(|p| p.area.clone().upcast::<gtk::Widget>()),
        PaneLayout::Split {
            orientation,
            children,
            weights,
        } => {
            // Orientation::Vertical (side by side) -> a HORIZONTAL gtk::Paned
            // (children left/right); Horizontal (stacked) -> a VERTICAL paned.
            let gtk_orient = match orientation {
                Orientation::Vertical => gtk::Orientation::Horizontal,
                Orientation::Horizontal => gtk::Orientation::Vertical,
            };
            build_chain(gtk_orient, children, weights, panes)
        }
    }
}

/// Renders the `children` of one n-ary split as a RIGHT-LEANING chain of binary
/// `gtk::Paned`s: the first child is the `start` child of a paned whose `end` child
/// is the chain over the remaining children. Each paned's divider is positioned at
/// the first child's SHARE of the children it spans (so equal thirds give an outer
/// 1/3 divider over an inner 1/2 divider). Unbound leaves are skipped: a `None`
/// start collapses to the end chain and vice versa.
fn build_chain(
    gtk_orient: gtk::Orientation,
    children: &[PaneLayout],
    weights: &[f64],
    panes: &HashMap<PaneId, PaneWidgets>,
) -> Option<gtk::Widget> {
    if children.len() == 1 {
        return build_layout_widget(&children[0], panes);
    }

    let start = build_layout_widget(&children[0], panes);
    let end = build_chain(gtk_orient, &children[1..], &weights[1..], panes);

    let (start, end) = match (start, end) {
        (Some(start), Some(end)) => (start, end),
        // One side is entirely unbound: collapse to whichever side has a widget.
        (start, end) => return start.or(end),
    };

    // The divider's share = this (first) child's weight over the total weight of
    // the children THIS paned spans (first + rest).
    let total: f64 = weights.iter().sum();
    let ratio = if total > 0.0 { weights[0] / total } else { 0.5 };

    let paned = gtk::Paned::new(gtk_orient);
    paned.set_hexpand(true);
    paned.set_vexpand(true);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);
    paned.set_start_child(Some(&start));
    paned.set_end_child(Some(&end));
    install_paned_position(&paned, ratio);
    Some(paned.upcast::<gtk::Widget>())
}

/// Keeps the paned divider at `ratio` of its travel. Re-applies on EVERY
/// `max-position` notify (not one-shot): the nested multi-pass allocation reports an
/// intermediate `max-position` before settling, and a window/container resize
/// changes it again, so re-deriving the position from `ratio` each time converges
/// correctly and keeps every child a proportional, non-collapsed size. (A user drag
/// changes `position`, not `max-position`, so it is not fought here; in v1 a drag is
/// reset to the equal ratio on the next resize.)
fn install_paned_position(paned: &gtk::Paned, ratio: f64) {
    paned.connect_notify_local(Some("max-position"), move |p, _| {
        let max = p.max_position();
        if max > 0 {
            p.set_position(ratio_position(max, ratio));
        }
    });
}

/// Computes a divider position as `round(max * ratio)`, clamped into `[0, max]`.
fn ratio_position(max: i32, ratio: f64) -> i32 {
    let v = (f64::from(max) * ratio).round().clamp(0.0, f64::from(max));
    #[allow(
        clippy::cast_possible_truncation,
        reason = "v is clamped to [0, max] and rounded, so the cast is exact and in range"
    )]
    {
        v as i32
    }
}

/// Ceils a cell dimension (px) to at least one pixel, as a content-size floor under
/// a non-shrinking paned child.
fn cell_floor(cell_px: f64) -> i32 {
    let v = cell_px.ceil().clamp(1.0, f64::from(i32::MAX));
    #[allow(
        clippy::cast_possible_truncation,
        reason = "v is clamped to [1, i32::MAX] and ceil'd, so the cast is exact and in range"
    )]
    {
        v as i32
    }
}

/// Computes the cmdline strip's pixel height from the cell height.
fn strip_height(cell_h: f64) -> i32 {
    let h = (cell_h * CMDLINE_STRIP_ROWS)
        .ceil()
        .clamp(1.0, f64::from(i32::MAX));
    #[allow(
        clippy::cast_possible_truncation,
        reason = "h is clamped to [1, i32::MAX] and ceil'd, so the cast is exact and in range"
    )]
    {
        h as i32
    }
}

/// Picks the window to bind the first pane to: the active window if known (and a
/// real, visible window grid), else the lowest-numbered visible window grid.
fn pick_initial_window(snap: &Snapshot) -> Option<(i64, u64)> {
    if let Some(aw) = snap.active_win {
        if let Some(w) = snap
            .windows
            .values()
            .find(|w| w.win == aw && w.grid >= MIN_WINDOW_GRID && !w.hidden)
        {
            return Some((w.win, w.grid));
        }
    }
    snap.windows
        .values()
        .filter(|w| w.grid >= MIN_WINDOW_GRID && !w.hidden)
        .min_by_key(|w| w.grid)
        .map(|w| (w.win, w.grid))
}

/// Loads the [`ACTIVE_PANE_CSS`] definition once into the default display.
fn install_pane_css() {
    if let Some(display) = gdk::Display::default() {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(ACTIVE_PANE_CSS_DATA);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
