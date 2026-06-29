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
//! Under `ext_multigrid` the command line, `/`-search, and messages live on grid 1,
//! which the panes do not paint. The [`PaneInner::cmdline_area`] is a one-row
//! `DrawingArea` along the bottom that paints grid 1's LAST row (translated up so
//! only that row lands in the strip), making typed `:`/`/` input and messages
//! visible again.
//!
//! ## Intents
//!
//! On each render wake [`PaneHandle::on_render_wake`] creates the first pane when a
//! window grid appears, drains layout intents ([`wolf_core::WolfCore::drain_intents`])
//! to apply splits (mutate the tree + bind a new pane) and focus-direction moves
//! (resolve the neighbour in the tree + `focus_win`), detects closed windows (a
//! bound pane whose grid vanished) and collapses them, resolves pending pane grids,
//! and updates the active-pane focus ring from `active_win`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;

use wolf_core::{CursorShape, LayoutIntent, Snapshot, SplitOrientation, WolfCore};

use crate::layout::{Orientation, PaneId, PaneLayout, PaneTree};
use crate::render::draw_glyph;
use crate::{cursor, grid_size_for, paint, CellMetrics};

/// The id of the first (root) pane. The [`PaneTree`] is constructed with this as
/// its root leaf, so the first bound pane reuses it (no `split` needed).
const FIRST_PANE_ID: PaneId = 1;

/// The lowest grid id that is an editor WINDOW grid. Grid 1 is the outer/global
/// grid (cmdline + messages); window content lives on grids `>= 2`.
const MIN_WINDOW_GRID: u64 = 2;

/// The outer/global grid id whose last row carries the command line, painted in the
/// cmdline strip. (Mirrors `wolf_core`'s private `OUTER_GRID_ID`.)
const CMDLINE_GRID_ID: u64 = 1;

/// Height of the cmdline strip, in grid rows.
const CMDLINE_STRIP_ROWS: f64 = 1.0;

/// CSS class applied to the active pane's `DrawingArea` to draw a focus ring.
const ACTIVE_PANE_CSS: &str = "wolf-active-pane";

/// CSS defining [`ACTIVE_PANE_CSS`]. An `outline` (not a `border`) so the ring does
/// NOT consume layout space (no resize churn on focus changes). Loaded once.
const ACTIVE_PANE_CSS_DATA: &str =
    ".wolf-active-pane { outline: 2px solid #3584e4; outline-offset: -2px; }";

/// Divisor to convert an 8-bit color channel (0–255) to Cairo's 0.0–1.0 range.
const RGB_MAX: f64 = 255.0;

/// The widgets bound to one pane: its Neovim window handle, its grid (pending until
/// `win_pos` is observed), and its drawing area (reused across rebuilds by id).
struct PaneWidgets {
    /// The Neovim window handle this pane is bound to.
    win: i64,
    /// The grid id this pane paints, or `None` until resolved via `grid_for_win`.
    grid: Cell<Option<u64>>,
    /// The pane's drawing area (reused by [`PaneId`] across tree rebuilds).
    area: gtk::DrawingArea,
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
        self.panes.borrow_mut().insert(
            id,
            PaneWidgets {
                win,
                grid: Cell::new(Some(grid)),
                area,
            },
        );
        self.rebuild_pane_area();
        self.set_active_pane(Some(id));
    }

    /// Drains and applies all pending layout intents (splits + focus moves).
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
        self.panes.borrow_mut().insert(
            new_id,
            PaneWidgets {
                win,
                grid: Cell::new(grid),
                area,
            },
        );
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
    /// resize (per-grid + global resize), and click-to-focus gesture.
    fn make_pane_area(self: &Rc<Self>, pane_id: PaneId) -> gtk::DrawingArea {
        let area = gtk::DrawingArea::builder()
            .hexpand(true)
            .vexpand(true)
            .focusable(true)
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
        {
            let weak = Rc::downgrade(self);
            let gesture = gtk::GestureClick::new();
            gesture.connect_pressed(move |_g, _n, _x, _y| {
                if let Some(inner) = weak.upgrade() {
                    inner.on_pane_clicked(pane_id);
                }
            });
            area.add_controller(gesture);
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
        let grid_id = self
            .panes
            .borrow()
            .get(&pane_id)
            .and_then(|p| p.grid.get());
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
        let grid = self
            .panes
            .borrow()
            .get(&pane_id)
            .and_then(|p| p.grid.get());
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

    /// Click-to-focus: focus the clicked pane's window and mark it active.
    fn on_pane_clicked(&self, pane_id: PaneId) {
        let win = self.panes.borrow().get(&pane_id).map(|p| p.win);
        if let Some(win) = win {
            if let Some(core) = self.core.borrow().as_ref() {
                core.focus_win(win);
            }
        }
        self.set_active_pane(Some(pane_id));
        if let Some(p) = self.panes.borrow().get(&pane_id) {
            p.area.grab_focus();
        }
    }

    /// Paints the cmdline strip: grid 1's LAST row, translated up so only that row
    /// lands in the one-row strip; plus the cursor when it lives on grid 1.
    fn draw_cmdline(&self, cr: &cairo::Context) {
        let core_ref = self.core.borrow();
        let Some(core) = core_ref.as_ref() else {
            return;
        };
        let snap = core.read_snapshot();
        let Some(grid1) = snap.grid(CMDLINE_GRID_ID) else {
            return;
        };
        if grid1.rows == 0 {
            return;
        }
        // Translate up so grid 1's last row lands at the strip's top; the rest of
        // the grid is painted above the visible area and clipped away.
        let last = grid1.rows - 1;
        cr.translate(0.0, -(last as f64) * self.metrics.cell_h);
        paint(cr, grid1, &snap.highlights, &self.metrics);
        if snap.cursor.grid == CMDLINE_GRID_ID {
            draw_cursor(cr, &snap, &self.metrics);
        }
    }

    /// Rebuilds `pane_area`'s child widget tree from the current [`PaneTree`],
    /// REUSING each pane's `DrawingArea` by id (so bound grids are preserved).
    fn rebuild_pane_area(&self) {
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
        let layout = self.tree.borrow().layout();
        let panes = self.panes.borrow();
        if let Some(widget) = build_layout_widget(&layout, &panes) {
            widget.set_hexpand(true);
            widget.set_vexpand(true);
            self.pane_area.append(&widget);
        }
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
}

/// Builds the pane view (container, pane area host, cmdline strip) and returns a
/// [`PaneHandle`]. No panes exist until the first window grid appears on a wake.
#[must_use]
pub fn build(
    core: &Rc<RefCell<Option<WolfCore>>>,
    metrics: &Rc<CellMetrics>,
) -> PaneHandle {
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
    });

    {
        let weak = Rc::downgrade(&inner);
        cmdline_area.set_draw_func(move |_area, cr, _w, _h| {
            if let Some(inner) = weak.upgrade() {
                inner.draw_cmdline(cr);
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
    let (x, y, w, h) = cursor::cursor_rect(shape, cell_x, cell_y, metrics.cell_w, metrics.cell_h, pct);

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
        PaneLayout::Leaf(id) => panes.get(id).map(|p| p.area.clone().upcast::<gtk::Widget>()),
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
