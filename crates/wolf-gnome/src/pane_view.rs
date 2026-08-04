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
//! A pane's `DrawingArea` is larger than its grid: it also carries a two-line
//! **statusbar strip** along the top and a uniform padding on all four sides, both
//! painted into the same area by [`PaneInner::draw_pane`] rather than being
//! widgets of their own (`crates/wolf-gnome/DECISIONS.md` records why). Every
//! pixel-to-grid conversion in this file therefore routes through
//! [`crate::editor_inset`] — the resizer subtracts the chrome before dividing, and
//! the pointer paths translate a pane-local point into grid-local space before
//! [`cell_at`]. The strip's own rulebook is [`crate::statusbar_layout`].
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
//!   leaf's RAW buffer path out of the per-pane status cache
//!   [`PaneInner::refresh_pane_status`] fills once per wake from the RPC-free
//!   `WolfCore::pane_status(grid)`, encodes it via [`crate::pane_layout::encode`],
//!   and applies [`crate::state_seam::persistable_layout`]'s CONTENT suppression
//!   rule. It is called from [`crate::persistence`] on every render wake and at
//!   `close-request`. [`PaneHandle::snapshot_readiness`] supplies the two counts
//!   `state_seam::may_snapshot` gates on, so a snapshot taken before the first
//!   pane binds can never wipe a good save.
//!
//!   The cache is refreshed inside `panes.on_render_wake()`, which runs strictly
//!   before `shell.on_render_wake()` drives the persistence path, so the wake path
//!   always reads the current wake's values. `close-request` is the one bounded
//!   exception: [`crate::persistence`]'s final flush reads the cache as of the LAST
//!   wake, so a buffer change made in the window between that wake and the close
//!   request is not persisted. Every `:e` produces a redraw flush and therefore a
//!   wake, so the gap is one frame.
//! - **Restore.** [`PaneHandle::request_layout_rebuild`] queues the saved tree at
//!   launch; the first wake at which a window grid exists consumes it
//!   ([`PaneInner::consume_pending_layout`]) and realizes it
//!   ([`PaneInner::apply_layout_rebuild`]) through `WolfCore::rebuild_layout` —
//!   the ONE blocking RPC this front-end issues, documented there.
//!
//! A user's DIVIDER DRAG feeds the save half. [`install_paned_position`] writes the
//! dragged `gtk::Paned` position back into the tree's weights through
//! [`PaneTree::set_divider_fraction`], which is what puts it on the save path at
//! all: the encoder copies whatever weights the tree holds, so a drag left in the
//! widget alone would never be persisted and would not survive the next
//! re-allocation either.
//!
//! Those divider closures capture the tree as an `Rc<RefCell<PaneTree>>` rather
//! than a `Weak<PaneInner>`, and that is deliberate. The tree is all they need, and
//! the ownership edges run `PaneInner` -> `container` -> paned -> closure -> tree
//! and `PaneInner` -> tree, with none back to `PaneInner` — so there is no cycle to
//! break and no upgrade that can fail mid-drag. Do not "correct" it to a `Weak`.
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
    CursorShape, LayoutIntent, MouseAction, MouseEvent, PaneNode, PaneStatus, ScrollEvent,
    Snapshot, SplitOrientation, WolfCore,
};

use crate::editor_inset::{self, GridRect, EDITOR_PADDING_PX};
use crate::geometry;
use crate::layout::{Orientation, PaneId, PaneLayout, PaneTree};
use crate::mouse::{self, PaneScroll};
use crate::pane_layout;
use crate::render::{draw_glyph, draw_text_line, FALLBACK_BG, FALLBACK_FG};
use crate::state_seam;
use crate::statusbar_layout::{
    self, StatusbarLayout, STATUSBAR_LINE_GAP, STATUSBAR_VERTICAL_PADDING,
};
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
/// rebuilds by id), the per-pane mouse/scroll bookkeeping, and the statusbar text
/// cached on the last wake.
///
/// Every mutable field is a `Cell` — with ONE exception, `status` — because all
/// access is on the single GTK main thread and a `Cell` cannot have a borrow
/// accidentally held across a [`WolfCore`] call (the rule this whole module
/// follows). `status` is a `RefCell` because [`PaneStatus`] owns three `String`s
/// and is therefore not `Copy`; it obeys the same rule by discipline instead. It is
/// borrowed in exactly two places — cloned out inside a single statement in
/// [`PaneInner::draw_pane`], written inside a single statement in
/// [`PaneInner::refresh_pane_status`] — and neither borrow is ever live across a
/// core call.
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
    /// This pane's statusbar strings, read from `WolfCore::pane_status` once per
    /// wake by [`PaneInner::refresh_pane_status`].
    ///
    /// Cached rather than read at paint time because [`PaneInner::draw_pane`] holds
    /// a snapshot read guard for its whole body while `pane_status` takes
    /// `read_snapshot()` again internally, and `shell.rs`'s borrow-discipline
    /// section records that a re-entrant read can deadlock behind a queued writer.
    status: RefCell<PaneStatus>,
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
            // The unnamed-buffer shape, taken from the same pure seam the live
            // value comes from rather than hand-written here.
            status: RefCell::new(wolf_core::pane_status("", None, None)),
        }
    }
}

/// A mouse/scroll target: a grid id, that grid's LIVE dimensions read from the
/// snapshot at the moment the event is handled, and the pane-local rectangle the
/// grid paints into.
///
/// The four fields travel together everywhere (`cell_at` needs the dims, the
/// translation needs the rect, the `MouseEvent`/`ScrollEvent` needs the id), so
/// they are one value — which also keeps `send_pane_mouse` under clippy's
/// argument-count threshold without an `#[allow]`.
#[derive(Clone, Copy)]
struct GridTarget {
    /// The Neovim grid the event targets. Always a WINDOW grid (`>= 2`).
    grid: u64,
    /// That grid's live column count.
    cols: u16,
    /// That grid's live row count.
    rows: u16,
    /// The pane-local rectangle this grid paints into, derived from the pane's
    /// CURRENT allocation. Pointer coordinates arrive pane-local and are
    /// translated through [`editor_inset::grid_local`] before [`cell_at`], so a
    /// click in the statusbar strip or a padding gutter still names a real cell.
    grid_rect: GridRect,
}

/// The pane view's shared, interior-mutable state. All access is on the single GTK
/// main thread, so `Cell`/`RefCell` (not a `Mutex`) suffices.
struct PaneInner {
    /// The pure pane-layout tree (split/remove/neighbour geometry).
    ///
    /// Shared by `Rc` because the divider closures [`install_paned_position`]
    /// installs need it and nothing else. See this module's "Layout persistence"
    /// section for why that is an `Rc` rather than the crate's usual `Weak`.
    tree: Rc<RefCell<PaneTree>>,
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
    /// The per-pane statusbar strip's vertical layout, in device pixels.
    ///
    /// Measured ONCE in [`build`] from `metrics.size_px`. The metrics never change
    /// for the life of the process, so the strip is not re-derived per frame.
    statusbar: StatusbarLayout,
    /// The strip's primary (filename) font at regular weight, for an INACTIVE pane.
    statusbar_primary: pango::FontDescription,
    /// The strip's primary font at bold weight, for the ACTIVE pane.
    statusbar_primary_bold: pango::FontDescription,
    /// The strip's secondary (path) font.
    statusbar_secondary: pango::FontDescription,
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
    /// collapse closed panes, resolve pending grids, refresh each pane's statusbar
    /// text and the active pane, then queue redraws.
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
        // AFTER `resolve_pending_grids` — that is the last step that can bind a
        // grid, and a pane with no grid has no status to read — and BEFORE
        // `queue_redraw`, or the frame this wake queues paints last wake's filename
        // in a pane that just changed buffers. Placing it ahead of
        // `update_active_pane` keeps "read core state" before "decide the active
        // pane and paint", mirroring the order already here.
        self.refresh_pane_status();
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
        // Without this every restored pane paints `[No Name]` for one frame: this
        // path early-returns from the wake, so the normal refresh never runs.
        self.refresh_pane_status();
        self.queue_redraw();
    }

    /// Reads every bound pane's statusbar strings from
    /// `WolfCore::pane_status(grid)` and caches them on the pane.
    ///
    /// **This is the only place `pane_status` is called**, and the cache is what
    /// makes that possible: [`Self::draw_pane`] holds a snapshot read guard for its
    /// whole body while `pane_status` takes `read_snapshot()` again internally, and
    /// `shell.rs`'s borrow-discipline section records that a re-entrant read can
    /// deadlock behind a queued writer. The call is RPC-free (a snapshot read plus
    /// `live_cwd`), and it costs exactly one call per bound pane per wake — the
    /// same budget [`Self::layout_snapshot`] spends, since that reads this cache
    /// rather than calling again, so all three of the result's fields come out of
    /// one read.
    ///
    /// Two-phase, like [`Self::layout_snapshot`]: the `panes` borrow that collects
    /// the targets is dropped before the core borrow is taken, and the core borrow
    /// is dropped before the results are written back.
    fn refresh_pane_status(&self) {
        let targets: Vec<(PaneId, u64)> = {
            let panes = self.panes.borrow();
            panes
                .iter()
                .filter_map(|(id, p)| p.grid.get().map(|g| (*id, g)))
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        let statuses: Vec<(PaneId, PaneStatus)> = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            targets
                .into_iter()
                .map(|(id, g)| (id, core.pane_status(g)))
                .collect()
        };
        {
            let panes = self.panes.borrow();
            for (id, status) in statuses {
                if let Some(p) = panes.get(&id) {
                    *p.status.borrow_mut() = status;
                }
            }
        }
    }

    /// The tab's persistable pane tree, or `None` for an unnamed lone pane.
    ///
    /// Reads each leaf's buffer path out of the per-pane status cache
    /// [`Self::refresh_pane_status`] fills — the RAW absolute path, NOT
    /// `display_path` (which is relativized for the statusbar and would be useless
    /// to restore from). No core call happens here at all.
    ///
    /// The cache is refreshed inside `panes.on_render_wake()`, which runs strictly
    /// before `shell.on_render_wake()` and therefore before `crate::persistence`
    /// reads this, so the values are always the current wake's. The one bounded
    /// staleness is `flush_on_close`: it reads the cache as of the LAST wake, so a
    /// buffer change in the window between that wake and the close request is not
    /// persisted. Every `:e` produces a redraw flush and therefore a wake, so the
    /// gap is one frame.
    ///
    /// The `None` suppression is [`state_seam::persistable_layout`]'s CONTENT rule,
    /// never an arity test.
    fn layout_snapshot(&self) -> Option<PaneNode> {
        let paths: HashMap<PaneId, String> = {
            let tree = self.tree.borrow();
            let panes = self.panes.borrow();
            tree.leaves()
                .into_iter()
                .filter_map(|id| {
                    let p = panes.get(&id)?;
                    // A pane with no bound grid has no status to have been read,
                    // and contributes no path.
                    p.grid.get()?;
                    Some((id, p.status.borrow().abs_path.clone()))
                })
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
                let (cols, rows) = self.pane_grid_size(w, h);
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

    /// The Neovim grid size for a pane allocated `width` x `height` device pixels,
    /// once its statusbar strip and its four padding gutters are removed.
    ///
    /// The one place a PANE's allocation becomes a grid size. `geometry::grid_size_for`
    /// is still reachable and still correct — [`Self::resize_global`] uses it — but a
    /// pane must never call it directly, or the grid it asks Neovim for is the size of
    /// the whole `DrawingArea` and the bottom rows land under the neighbouring pane.
    fn pane_grid_size(&self, width: i32, height: i32) -> (u16, u16) {
        editor_inset::grid_size_for_pane(
            width,
            height,
            self.statusbar.height,
            EDITOR_PADDING_PX,
            self.metrics.cell_w,
            self.metrics.cell_h,
        )
    }

    /// The pane-local rectangle the grid paints into, for a pane of `width` x
    /// `height` device pixels.
    ///
    /// The paint path has its allocation handed to it by the draw func; the pointer
    /// paths read it off the widget through [`Self::pane_grid_rect`]. Both derive
    /// the rect fresh rather than caching it: the allocation moves with every
    /// divider drag and window resize, and a stale rect would offset every click by
    /// the difference.
    fn pane_grid_rect_for(&self, width: f64, height: f64) -> GridRect {
        editor_inset::grid_rect(width, height, self.statusbar.height, EDITOR_PADDING_PX)
    }

    /// The pane-local rectangle `area`'s grid paints into, from its CURRENT
    /// allocation.
    fn pane_grid_rect(&self, area: &gtk::DrawingArea) -> GridRect {
        self.pane_grid_rect_for(f64::from(area.width()), f64::from(area.height()))
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
        // A one-cell-OF-GRID minimum so that, with `shrink-*-child(false)` on the
        // enclosing paneds, a child can never be allocated to zero pixels (a
        // starved pane). The floor covers the strip and both padding gutters as
        // well: a floor of one bare cell would let GTK allocate a pane whose grid
        // rounds down to nothing, and Neovim would reflow that window's whole
        // buffer to one column.
        let (min_w, min_h) = editor_inset::min_pane_size(
            self.metrics.cell_w,
            self.metrics.cell_h,
            self.statusbar.height,
            EDITOR_PADDING_PX,
        );
        area.set_content_width(cell_floor(min_w));
        area.set_content_height(cell_floor(min_h));

        {
            let weak = Rc::downgrade(self);
            area.set_draw_func(move |_area, cr, w, h| {
                if let Some(inner) = weak.upgrade() {
                    inner.draw_pane(cr, pane_id, f64::from(w), f64::from(h));
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

    /// Paints the whole of pane `pane_id`: the editor background across the entire
    /// allocation, the statusbar strip's two lines of text, then its bound grid
    /// (fallback active grid) plus the cursor iff this pane owns the cursor's grid.
    ///
    /// The strip, the padding and the grid are all painted here rather than being
    /// separate widgets, and the background fill is why: the padding must be the
    /// EDITOR's background, a per-frame value out of the snapshot, whereas a GTK
    /// widget's background is theme-owned. See `crates/wolf-gnome/DECISIONS.md`.
    ///
    /// **`WolfCore::pane_status` must never be called from here.** This function
    /// holds a `SnapshotReadGuard` for its whole body and `pane_status` takes
    /// `read_snapshot()` again internally; a re-entrant read can deadlock behind a
    /// queued writer (`shell.rs`'s borrow-discipline section). The strings come out
    /// of the per-pane cache [`Self::refresh_pane_status`] fills on the wake.
    ///
    /// **The grid clip is required, not decorative.** The resize round trip is
    /// asynchronous — GTK allocates, we call `resize_grid`, Neovim answers on a
    /// later flush — so `snap.grid(g)` can transiently be LARGER than the current
    /// allocation. Without the clip those extra rows paint into this pane's bottom
    /// padding and under the neighbouring pane's strip: the widget bounds are wider
    /// than the grid rectangle, so they do not bound the overflow.
    ///
    /// The strip's clip bounds a long path to its own pane. There is no ellipsis:
    /// macOS truncates the tail, and reproducing that here needs a second Pango
    /// layout builder in the widget layer, which is exactly what `render.rs`'s
    /// one-glyph-path rule exists to prevent.
    fn draw_pane(&self, cr: &cairo::Context, pane_id: PaneId, width: f64, height: f64) {
        let core_ref = self.core.borrow();
        let Some(core) = core_ref.as_ref() else {
            return;
        };
        let snap = core.read_snapshot();

        // The editor defaults — the same source `draw_cmdline` resolves for its own
        // band, and the only colour channel this front-end has (it reads no OS
        // theme, in either direction).
        let style = snap.highlights.resolve(0);
        let fg = style.fg.unwrap_or(FALLBACK_FG);
        let bg = style.bg.unwrap_or(FALLBACK_BG);

        // The WHOLE allocation, so the strip band and all four padding gutters are
        // the editor's background rather than an unpainted or theme-coloured frame.
        cr.set_source_rgb(
            f64::from(bg.r) / RGB_MAX,
            f64::from(bg.g) / RGB_MAX,
            f64::from(bg.b) / RGB_MAX,
        );
        cr.rectangle(0.0, 0.0, width, height);
        cr.fill().expect("cairo pane background fill failed");

        // One borrow, one statement: `PaneStatus` is not `Copy`, so it is cloned
        // out rather than held, and no core call happens while it is live.
        let (grid_id, status) = {
            let panes = self.panes.borrow();
            panes.get(&pane_id).map_or_else(
                || (None, wolf_core::pane_status("", None, None)),
                |p| (p.grid.get(), p.status.borrow().clone()),
            )
        };
        let rect = self.pane_grid_rect_for(width, height);
        let text =
            statusbar_layout::statusbar_text(fg, bg, self.active_pane.get() == Some(pane_id));

        cr.save().expect("cairo save failed before the statusbar");
        cr.rectangle(0.0, 0.0, width, self.statusbar.height);
        cr.clip();
        let primary_font = if text.primary_bold {
            &self.statusbar_primary_bold
        } else {
            &self.statusbar_primary
        };
        // `rect.x` and not a separate inset: the strip's text and the grid's text
        // share ONE origin by construction (see `editor_inset`'s module docs).
        draw_text_line(
            cr,
            primary_font,
            rect.x,
            self.statusbar.primary_line_top,
            &status.filename,
            text.primary,
        );
        // An unnamed buffer has no path line at all (macOS parity), rather than a
        // blank one that still consumes its box.
        if !status.display_path.is_empty() {
            draw_text_line(
                cr,
                &self.statusbar_secondary,
                rect.x,
                self.statusbar.secondary_line_top,
                &status.display_path,
                text.secondary,
            );
        }
        cr.restore()
            .expect("cairo restore failed after the statusbar");

        cr.save().expect("cairo save failed before the grid");
        cr.rectangle(rect.x, rect.y, rect.w, rect.h);
        cr.clip();
        // Translating the context covers every origin `paint` and `draw_cursor`
        // compute, which is what leaves both of them unchanged.
        cr.translate(rect.x, rect.y);
        let grid = grid_id
            .and_then(|g| snap.grid(g))
            .unwrap_or_else(|| snap.active_grid());
        paint(cr, grid, &snap.highlights, &self.metrics);
        if let Some(g) = grid_id {
            if snap.cursor.grid == g {
                draw_cursor(cr, &snap, &self.metrics);
            }
        }
        cr.restore().expect("cairo restore failed after the grid");
    }

    /// Handles a pane area resize: resize the pane's own grid, then the global grid.
    fn on_pane_resize(&self, pane_id: PaneId, width: i32, height: i32) {
        let grid = self.panes.borrow().get(&pane_id).and_then(|p| p.grid.get());
        if let Some(grid) = grid {
            if width > 0 && height > 0 {
                let (cols, rows) = self.pane_grid_size(width, height);
                if let Some(core) = self.core.borrow().as_ref() {
                    core.resize_grid(grid, cols, rows);
                }
            }
        }
        self.resize_global();
    }

    /// Resizes the outer/global grid (grid 1) to the full editor area, so the
    /// cmdline spans the width and nvim's overall layout tracks the window.
    ///
    /// **Deliberately the un-inset [`grid_size_for`], not
    /// [`Self::pane_grid_size`].** Grid 1 is not a pane: it covers the whole editor
    /// area, so with N panes there is no single statusbar strip to subtract and no
    /// one set of padding gutters. Under `ext_multigrid` every window grid is sized
    /// independently by `resize_grid`, which makes this size advisory (`&columns` /
    /// `&lines`), so the bare division is both the status quo and the only
    /// arithmetically meaningful answer here. A reader who greps `grid_size_for`,
    /// finds three call sites and sees two changed is looking at this one.
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
        let (grid, rect) = {
            let panes = self.panes.borrow();
            let p = panes.get(&pane_id)?;
            (p.grid.get()?, self.pane_grid_rect(&p.area))
        };
        self.dims_for_grid(grid, rect)
    }

    /// The LIVE dimensions of `grid`, paired with the pane rectangle `rect` the
    /// event's coordinates are relative to, as a [`GridTarget`].
    ///
    /// Used by the drag/release path, which must NOT re-resolve the pane's current
    /// grid: the drag targets the grid PINNED at press, so following a rebind
    /// mid-drag would silently retarget the selection. `rect` is the caller's
    /// because it belongs to the pane the gesture started in, whatever grid is
    /// pinned.
    ///
    /// `usize -> u16` goes through `try_from`, never `as`: a grid wider than
    /// `u16::MAX` is impossible in practice, but saturating is the only honest
    /// answer if it ever happened, and a wrapping cast would produce a cell inside
    /// a grid that does not exist.
    fn dims_for_grid(&self, grid: u64, rect: GridRect) -> Option<GridTarget> {
        let core_ref = self.core.borrow();
        let core = core_ref.as_ref()?;
        let snap = core.read_snapshot();
        let g = snap.grid(grid)?;
        Some(GridTarget {
            grid,
            cols: u16::try_from(g.cols).unwrap_or(u16::MAX),
            rows: u16::try_from(g.rows).unwrap_or(u16::MAX),
            grid_rect: rect,
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
        let (grid, button, rect) = {
            let panes = self.panes.borrow();
            let p = panes.get(&pane_id)?;
            (
                p.drag_grid.get()?,
                p.drag_button.get()?,
                // The gesture's coordinates are always widget-local to `pane_id`'s
                // OWN area, whatever grid is pinned, so the rect is this pane's.
                self.pane_grid_rect(&p.area),
            )
        };
        // The PINNED grid's dims, re-read live — NOT `grid_and_dims(pane_id)`, which
        // would follow a rebind and retarget the drag mid-sequence.
        let target = self.dims_for_grid(grid, rect)?;
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
        // GTK delivers PANE-local coordinates; `cell_at` speaks GRID-local ones.
        // A press in the statusbar strip or a padding gutter translates to a
        // negative coordinate, which `cell_at` clamps to the nearest edge cell.
        let (gx, gy) = editor_inset::grid_local(x, y, target.grid_rect);
        let (row, col) = cell_at(
            gx,
            gy,
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
        // position the motion controller recorded — pane-local, and translated into
        // grid-local space exactly as the mouse path's is.
        let (px, py) = pointer;
        let (gx, gy) = editor_inset::grid_local(px, py, target.grid_rect);
        let (row, col) = cell_at(
            gx,
            gy,
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
            let ctx = BuildCtx {
                panes: &panes,
                tree: &self.tree,
            };
            if let Some(widget) = build_layout_widget(&layout, &ctx, &[]) {
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

    // The strip is measured ONCE: `metrics` never changes for the life of the
    // process, so nothing here belongs on the paint path. Everything is in DEVICE
    // PIXELS, which is the unit `statusbar_layout` is called in on this front-end
    // and the unit Cairo paints in.
    let primary_px = metrics.size_px;
    let secondary_px = statusbar_layout::secondary_font_size(primary_px);
    let statusbar = statusbar_layout::statusbar_layout(
        primary_px,
        secondary_px,
        STATUSBAR_VERTICAL_PADDING,
        STATUSBAR_LINE_GAP,
    );

    let inner = Rc::new(PaneInner {
        tree: Rc::new(RefCell::new(PaneTree::new(FIRST_PANE_ID))),
        panes: RefCell::new(HashMap::new()),
        next_pane_id: Cell::new(FIRST_PANE_ID),
        container,
        pane_area,
        cmdline_area: cmdline_area.clone(),
        statusbar,
        statusbar_primary: strip_font(&metrics.font, primary_px, false),
        statusbar_primary_bold: strip_font(&metrics.font, primary_px, true),
        statusbar_secondary: strip_font(&metrics.font, secondary_px, false),
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

/// What the widget builders need besides the layout they are walking: the bound
/// panes to reuse a `DrawingArea` from, and the live tree every divider closure
/// reads its fraction out of.
///
/// One value rather than two parameters, following the [`GridTarget`] precedent —
/// it keeps [`build_chain`] under clippy's argument-count threshold without an
/// `#[allow]`.
struct BuildCtx<'a> {
    /// The per-pane widgets, keyed by [`PaneId`].
    panes: &'a HashMap<PaneId, PaneWidgets>,
    /// The live pane tree, shared into every divider closure.
    tree: &'a Rc<RefCell<PaneTree>>,
}

/// Recursively builds the GTK widget for a [`PaneLayout`], reusing each leaf's
/// `DrawingArea` by id. Returns `None` for a leaf whose pane is not (yet) bound. An
/// n-ary split is rendered as a RIGHT-LEANING chain of binary `gtk::Paned`s (see
/// [`build_chain`]).
///
/// `path` is this node's address in the tree — the root-to-here list of child
/// indices, `&[]` at the root — which is what lets each paned name the divider it
/// is, in the coordinates [`PaneTree::divider_fraction`] speaks.
fn build_layout_widget(
    layout: &PaneLayout,
    ctx: &BuildCtx<'_>,
    path: &[usize],
) -> Option<gtk::Widget> {
    match layout {
        PaneLayout::Leaf(id) => ctx
            .panes
            .get(id)
            .map(|p| p.area.clone().upcast::<gtk::Widget>()),
        PaneLayout::Split {
            orientation,
            children,
            ..
        } => {
            // Orientation::Vertical (side by side) -> a HORIZONTAL gtk::Paned
            // (children left/right); Horizontal (stacked) -> a VERTICAL paned.
            let gtk_orient = match orientation {
                Orientation::Vertical => gtk::Orientation::Horizontal,
                Orientation::Horizontal => gtk::Orientation::Vertical,
            };
            build_chain(gtk_orient, children, ctx, path, 0)
        }
    }
}

/// Renders the `children` of one n-ary split as a RIGHT-LEANING chain of binary
/// `gtk::Paned`s: the first child is the `start` child of a paned whose `end` child
/// is the chain over the remaining children. Unbound leaves are skipped: a `None`
/// start collapses to the end chain and vice versa.
///
/// `path` addresses the SPLIT being rendered and `boundary` counts how many of its
/// children this recursion has already peeled off, so the paned built here IS the
/// divider `(path, boundary)` — the identity [`PaneTree::divider_fraction`] and
/// [`PaneTree::set_divider_fraction`] take. `children` is the split's suffix
/// `[boundary..]`, and the fraction that divider sits at is its head's share of
/// that suffix ([`crate::layout::split_head_fraction`], reached from the tree
/// through [`PaneTree::divider_fraction`]): each divider governs its head child
/// against everything the chain nests beneath it, not against the adjacent sibling.
/// Equal thirds therefore give an outer 1/3 divider over an inner 1/2 one.
///
/// No position is set here, and the split's weights are deliberately not a
/// parameter. A paned has no travel before its first allocation, so the only honest
/// position is the one [`install_paned_position`] derives from the live tree on each
/// `max-position` notify — the first of which arrives with that allocation — and the
/// tree is therefore the only thing that ever reads those weights.
fn build_chain(
    gtk_orient: gtk::Orientation,
    children: &[PaneLayout],
    ctx: &BuildCtx<'_>,
    path: &[usize],
    boundary: usize,
) -> Option<gtk::Widget> {
    if children.len() == 1 {
        return build_layout_widget(&children[0], ctx, &child_path(path, boundary));
    }

    let start = build_layout_widget(&children[0], ctx, &child_path(path, boundary));
    let end = build_chain(gtk_orient, &children[1..], ctx, path, boundary + 1);

    let (start, end) = match (start, end) {
        (Some(start), Some(end)) => (start, end),
        // One side is entirely unbound: collapse to whichever side has a widget.
        (start, end) => return start.or(end),
    };

    let paned = gtk::Paned::new(gtk_orient);
    paned.set_hexpand(true);
    paned.set_vexpand(true);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);
    paned.set_start_child(Some(&start));
    paned.set_end_child(Some(&end));
    // `gtk_orient` rather than a read-back `paned.orientation()`: this function is
    // where the axis is DECIDED (from the split's own `Orientation`), so passing it
    // down keeps one source of truth. A read-back would agree today and would be a
    // second thing to keep in step.
    install_paned_position(&paned, gtk_orient, ctx.tree, path.to_vec(), boundary);
    Some(paned.upcast::<gtk::Widget>())
}

/// A paned's own extent along the axis its divider travels: `width()` for a
/// horizontal paned, `height()` for a vertical one.
///
/// This is what a divider fraction is a proportion OF (see
/// [`geometry::divider_position`]), and it is deliberately not `max_position`: that
/// excludes the handle and the end child's minimum, so scaling by it makes a fresh
/// equal-weight split come out lopsided by exactly that minimum.
fn paned_span(paned: &gtk::Paned, axis: gtk::Orientation) -> i32 {
    match axis {
        gtk::Orientation::Vertical => paned.height(),
        _ => paned.width(),
    }
}

/// The path of child `index` under the split addressed by `path`.
fn child_path(path: &[usize], index: usize) -> Vec<usize> {
    let mut out = Vec::with_capacity(path.len() + 1);
    out.extend_from_slice(path);
    out.push(index);
    out
}

/// The per-paned state the two divider closures share.
struct DividerGuard {
    /// Set around this module's own `set_position` calls. `set_position` emits
    /// `notify::position` SYNCHRONOUSLY, so without it the re-apply would read its
    /// own write back as a user drag.
    applying: Cell<bool>,
    /// The `max-position` the last re-apply ran against. A `notify::position` whose
    /// maximum has moved since is one GTK produced while re-allocating, and the
    /// `max-position` handler is about to correct it.
    last_max: Cell<i32>,
    /// The paned's SPAN at the last re-apply, alongside `last_max`. The span is what
    /// [`geometry::divider_position`] scales by, so a `notify::position` whose span
    /// has moved is one GTK produced mid-resize and would read back as a fraction of
    /// an extent the re-apply never used.
    last_span: Cell<i32>,
}

/// Ties one `gtk::Paned` to the divider `(path, boundary)` in `tree`, in both
/// directions.
///
/// **Tree to widget.** On every `max-position` notify the divider's position is
/// re-derived from the tree's current fraction. Re-applying on every notify rather
/// than once is load-bearing: the nested multi-pass allocation reports an
/// intermediate maximum before settling, and a window or container resize changes it
/// again, so re-deriving each time is what converges and keeps every child a
/// proportional, non-collapsed size. Reading the LIVE tree each time (rather than a
/// ratio captured when the widget was built) is what makes a user's drag survive
/// that re-apply.
///
/// **Widget to tree.** A `notify::position` that is not this module's own write, and
/// whose `max-position` and SPAN have both held still, is a user drag, and is
/// written into the tree through [`PaneTree::set_divider_fraction`]. That is what
/// puts a drag on the save path (see this module's "Layout persistence" section).
/// The span is the quantity [`geometry::divider_position`] scales by, so it belongs
/// in that staleness gate beside the maximum rather than being trusted to move with
/// it — a resize can change one without the other.
///
/// Two rules here are not stylistic:
///
/// - The `applying` check comes BEFORE any tree borrow. `set_position` emits
///   `notify::position` synchronously, so a `Ref` still held from the re-apply plus
///   a `borrow_mut` here is a `RefCell` PANIC rather than a warning. Every borrow is
///   taken and dropped in its own statement, per the crate's borrow discipline.
/// - The write-back rebuilds NOTHING — no [`PaneInner::rebuild_pane_area`], no
///   redraw, no snapshot request. A rebuild destroys every paned, including the one
///   holding the live gesture. The widget already shows the new position; only the
///   tree is behind. The drag reaches `state.json` on the next render wake anyway,
///   because resizing a pane resizes its grid.
fn install_paned_position(
    paned: &gtk::Paned,
    axis: gtk::Orientation,
    tree: &Rc<RefCell<PaneTree>>,
    path: Vec<usize>,
    boundary: usize,
) {
    let guard = Rc::new(DividerGuard {
        applying: Cell::new(false),
        last_max: Cell::new(0),
        last_span: Cell::new(0),
    });

    {
        let tree = tree.clone();
        let guard = guard.clone();
        let path = path.clone();
        paned.connect_notify_local(Some("max-position"), move |p, _| {
            let max = p.max_position();
            let span = paned_span(p, axis);
            // BOTH guards come before the staleness gate is armed. A pass that
            // applied nothing must not record a `last_max`/`last_span`, or the very
            // next `notify::position` compares against a pair no `set_position` ever
            // ran with and is accepted as a user drag.
            if max <= 0 || span <= 0 {
                return;
            }
            // Its own statement: the `Ref` must be gone before any GTK call, which
            // can re-enter this module through `notify::position`.
            let fraction = tree.borrow().divider_fraction(&path, boundary);
            let Some(fraction) = fraction else {
                return;
            };
            guard.last_max.set(max);
            guard.last_span.set(span);
            guard.applying.set(true);
            p.set_position(geometry::divider_position(
                p.min_position(),
                max,
                span,
                fraction,
            ));
            guard.applying.set(false);
        });
    }

    {
        let tree = tree.clone();
        paned.connect_notify_local(Some("position"), move |p, _| {
            // FIRST, before any borrow: this notify can arrive SYNCHRONOUSLY from
            // the re-apply's own `set_position`, so a borrow taken above this check
            // would be taken from inside that call.
            if guard.applying.get() {
                return;
            }
            let max = p.max_position();
            let span = paned_span(p, axis);
            // The span joins `max` in the staleness gate because it is what the
            // fraction below is a proportion OF: a notify arriving mid-resize would
            // otherwise store a weight measured against an extent the re-apply never
            // used, and the tree would drift by the difference.
            if max <= 0 || max != guard.last_max.get() || span != guard.last_span.get() {
                return;
            }
            let Some(fraction) = geometry::divider_span_fraction(p.position(), span) else {
                return;
            };
            tree.borrow_mut()
                .set_divider_fraction(&path, boundary, fraction);
        });
    }
}

/// The grid font at `size_px` DEVICE PIXELS, for one of the statusbar strip's two
/// lines.
///
/// **`set_absolute_size`, not `set_size`, and the difference is deliberate.**
/// [`CellMetrics::for_font`] uses `set_size` because that is a POINT size and
/// changing it would change how the GRID face is rasterized — which every OCR
/// assertion in the smoke suite rides on. That argument does not transfer to a font
/// no assertion reads: the strip's line boxes, padding and gap are already in
/// device pixels (which is why [`CellMetrics::size_px`] exists), so measuring its
/// font in the same unit keeps the whole strip in one unit. At the 96 dpi this
/// crate assumes the two spellings give the same physical size; above it they would
/// not, which is subsumed by the crate's recorded absence of HiDPI handling.
fn strip_font(base: &pango::FontDescription, size_px: f64, bold: bool) -> pango::FontDescription {
    let mut font = base.clone();
    font.set_absolute_size(size_px * f64::from(pango::SCALE));
    if bold {
        font.set_weight(pango::Weight::Bold);
    }
    font
}

/// Ceils a pane dimension (px) to at least one pixel, as a content-size floor under
/// a non-shrinking paned child. Fed from [`editor_inset::min_pane_size`], so the
/// floor covers the statusbar strip and the padding gutters as well as the cell.
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
