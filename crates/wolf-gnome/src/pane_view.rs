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
//! window grids the panes paint. Three surfaces make them visible again, each painted
//! NATIVELY from one folded field of the snapshot. The [`PaneInner::cmdline_area`] is
//! a one-row `DrawingArea` along the bottom, and it paints [`Snapshot::cmdline`]
//! ALONE ([`PaneInner::draw_cmdline`]). It never reads grid 1: `ext_messages`
//! implicitly enables `ext_cmdline` and sets `cmdheight=0`, so grid 1 is EMPTY and
//! painting its last row would draw nothing.
//!
//! The other two are overlay surfaces above the strip, handed to `app.rs` for
//! placement by [`PaneHandle::toast_widget`] / [`PaneHandle::pum_widget`]:
//! [`PaneInner::toast_area`] paints [`Snapshot::toast`] ([`PaneInner::draw_toast`])
//! and [`PaneInner::pum_area`] paints [`Snapshot::pum`] ([`PaneInner::draw_pum`]).
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
//! - **Save.** [`PaneHandle::layout_snapshot`] walks the live tree and resolves
//!   each leaf out of the snapshot's WINDOW-KEYED maps — `Snapshot::previews`, then
//!   `Snapshot::win_buffers` — by the pane's own Neovim window handle
//!   ([`PaneInner::leaf_contents`]), encodes the result via
//!   [`crate::pane_layout::encode`], and applies
//!   [`crate::state_seam::persistable_layout`]'s CONTENT suppression rule. It is
//!   called from [`crate::persistence`] for EVERY tab on every render wake and at
//!   `close-request`. [`PaneHandle::snapshot_readiness`] supplies the two counts
//!   `state_seam::may_snapshot` gates on, so a snapshot taken before the first
//!   pane binds can never wipe a good save.
//!
//!   Those two maps are global across tabpages, so this reads no per-pane cache and
//!   makes no core call beyond one snapshot read — which is what lets a BACKGROUND
//!   tab be gathered at all, and it reads live rather than last-known-good. A leaf
//!   whose window is in neither map refuses the whole tab's gather
//!   ([`crate::pane_layout::LayoutGather::Refuse`]), and the tab then contributes
//!   its cached subtree.
//! - **Restore.** Two routes, and which one applies is decided by whether the tab
//!   has any panes yet. Tab 0 takes [`PaneHandle::request_layout_rebuild`], which
//!   queues the saved tree at launch for the first wake at which a window grid
//!   exists ([`PaneInner::consume_pending_layout`]). A tab created by the restore
//!   materialization takes [`PaneHandle::realize_layout`], which realizes
//!   immediately, because the tabpage its `create_tabpage` just returned is current
//!   only at that instant — and which DROPS the queued copy first, since
//!   `tab_view::TabsInner::open_tab` queues one for every tab it builds and a
//!   rebuild left queued would fire a SECOND time on that tab's first unheld wake.
//!   Both end in [`PaneInner::apply_layout_rebuild`] and
//!   `WolfCore::rebuild_layout`, one of the ten blocking RPCs this front-end
//!   issues, documented there; each restored tab costs exactly one.
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
//! That invariant is more than a structural argument: it is executed end to
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
use gtk::{gio, glib};
// TARGETED, never `webkit6::prelude::*`: that glob re-exports `gtk::prelude::*`,
// which collides with the `adw::prelude::*` already in scope above and makes every
// `WidgetExt` method ambiguous. Only the web view's own methods, and the one
// `PolicyDecision` method the `decide-policy` handler answers with, are wanted here.
use webkit6::prelude::{PolicyDecisionExt, WebViewExt};

use wolf_core::{
    CursorShape, LayoutIntent, MouseAction, MouseEvent, PaneNode, PaneStatus, PreviewInfo, Rgb,
    ScrollEvent, Snapshot, SplitOrientation, ToastChunk, WolfCore,
};

use crate::cmdline_ui::{self, PumView, ToastView};
use crate::drawer::{self, DrawerState};
use crate::editor_inset::{self, ColumnCap, GridRect, EDITOR_PADDING_PX};
use crate::geometry;
use crate::layout::{Orientation, PaneId, PaneLayout, PaneTree};
use crate::mouse::{self, PaneScroll, ScrollAccumulator};
use crate::pane_layout;
use crate::preview_link::{self, PreviewLink};
use crate::preview_ui::{self, DocumentUpdate, PreviewFilePress, PreviewLoad};
use crate::render::{draw_glyph, draw_text_line, FALLBACK_BG, FALLBACK_FG};
use crate::scrollbar;
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

/// Width of the edge drawn around the toast and the completion popup, in pixels.
const SURFACE_EDGE_PX: f64 = 1.0;

/// How far the surfaces' edge is blended from the editor background towards its
/// foreground. Low enough to read as a separator rather than as content.
const SURFACE_EDGE_ALPHA: f64 = 0.35;

/// CSS class applied to the active pane's `DrawingArea` to draw a focus ring.
const ACTIVE_PANE_CSS: &str = "wolf-active-pane";

/// CSS defining [`ACTIVE_PANE_CSS`]. An `outline` (not a `border`) so the ring does
/// NOT consume layout space (no resize churn on focus changes). Loaded into the
/// default display by [`install_pane_css`], once per TAB.
const ACTIVE_PANE_CSS_DATA: &str =
    ".wolf-active-pane { outline: 2px solid #3584e4; outline-offset: -2px; }";

/// Divisor to convert an 8-bit color channel (0–255) to Cairo's 0.0–1.0 range.
const RGB_MAX: f64 = 255.0;

/// The widgets and pointer state bound to one pane: its Neovim window handle, its
/// grid (pending until `win_pos` is observed), its host overlay and drawing area
/// (both reused across rebuilds by id), the per-pane mouse/scroll bookkeeping, the
/// statusbar text cached on the last wake, and — for a preview pane — its bound
/// source, its web view and what that view was last loaded from.
///
/// Every mutable field is a `Cell` where its type allows, because all access is on
/// the single GTK main thread and a `Cell` cannot have a borrow accidentally held
/// across a [`WolfCore`] call (the rule this whole module follows). Four fields
/// cannot be: `status` ([`PaneStatus`] owns three `String`s), `preview`
/// ([`PreviewInfo`] owns a `PathBuf`), `web` (a `webkit6::WebView` is a
/// refcounted `GObject`) and `pending_fragment` (a `String`), so they are `RefCell`s
/// obeying the same rule by
/// discipline instead. Each is borrowed only inside a single statement — cloned or
/// copied out where it is read, written where [`PaneInner::refresh_pane_state`] or
/// [`PaneInner::sync_preview_view`] fills it — and no such borrow is ever live
/// across a core call.
struct PaneWidgets {
    /// The Neovim window handle this pane is bound to.
    win: i64,
    /// The grid id this pane paints, or `None` until resolved via `grid_for_win`.
    grid: Cell<Option<u64>>,
    /// The pane's leaf widget: a `gtk::Overlay` whose main child is `area` and whose
    /// only overlay child is the preview `web` view, when there is one.
    ///
    /// **This — not `area` — is what the `gtk::Paned` tree parents**, which is what
    /// keeps a web view attached across every split, close and divider drag: a
    /// rebuild re-parents the host, and the view is the host's child rather than the
    /// tree's. Unparenting it instead would cost a WebProcess respawn and a fresh
    /// cold load (~255 ms from the call) on every layout change.
    host: gtk::Overlay,
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
    /// wake by [`PaneInner::refresh_pane_state`].
    ///
    /// Cached rather than read at paint time because [`PaneInner::draw_pane`] holds
    /// a snapshot read guard for its whole body while `pane_status` takes
    /// `read_snapshot()` again internally, and `shell.rs`'s borrow-discipline
    /// section records that a re-entrant read can deadlock behind a queued writer.
    status: RefCell<PaneStatus>,
    /// This pane's window's `b:wolf_full_width`, read once per wake alongside
    /// `status`.
    ///
    /// Cached rather than read at use time so the paint, the pointer translation
    /// and the resize request cannot disagree INSIDE ONE FRAME — a click landing in
    /// a cell the painter did not draw is exactly what a mid-frame change of the cap
    /// produces. It is also read from the same place for the same reason `status`
    /// is: `WolfCore::full_width_for_win` takes `read_snapshot()` internally, which
    /// [`PaneInner::draw_pane`] already holds.
    full_width: Cell<bool>,
    /// The `(cols, rows)` this pane last ASKED Neovim for.
    ///
    /// Compared against, never against the snapshot's live grid dimensions: the
    /// resize is fire-and-forget and Neovim answers on a later flush, so comparing
    /// against the snapshot would re-request the same size on every wake until it
    /// caught up.
    requested_grid: Cell<Option<(u16, u16)>>,
    /// This pane's [`PreviewInfo`] when it is a preview pane, `None` when it is an
    /// ordinary editor pane. Filled once per wake by
    /// [`PaneInner::refresh_pane_state`] from `Snapshot::previews`, keyed by this
    /// pane's own `win`.
    ///
    /// It is the ONE per-pane preview predicate: the statusbar source,
    /// [`PaneInner::draw_pane`]'s grid skip, the persisted leaf's `preview` flag and
    /// [`PaneInner::sync_preview_view`]'s create/remove decision all read it, so a
    /// pane cannot be a preview for one of them and an editor for another.
    preview: RefCell<Option<PreviewInfo>>,
    /// This pane's web view, created when it first becomes a preview and dropped
    /// when it stops being one ([`PaneInner::sync_preview_view`]).
    ///
    /// Lazily rather than eagerly: a `WebView` spawns its own WebProcess, and N of
    /// them at launch would move every existing cage smoke's timing budget.
    web: RefCell<Option<webkit6::WebView>>,
    /// The `(source_bufnr, source_tick)` pair this pane's view was last loaded from,
    /// for [`preview_ui::needs_reload`]. `None` for a view that has never been
    /// loaded, and cleared with the view itself.
    loaded: Cell<Option<PreviewLoad>>,
    /// The join of `Snapshot::previews` and `Snapshot::markdown_ticks` for THIS
    /// wake, filled beside `preview` by [`PaneInner::refresh_pane_state`] and
    /// compared against `loaded` by [`PaneInner::refresh_previews`].
    ///
    /// Cached rather than re-read where it is used, so the one function that issues
    /// the BLOCKING `render_preview` holds no snapshot guard at all — the strongest
    /// form of the rule the rest of this file follows by keeping its guards short.
    current_load: Cell<Option<PreviewLoad>>,
    /// The byte offset of `</head>` in the document last loaded into this pane's
    /// view, or `0` when no document has been loaded into it.
    ///
    /// `0` is an unambiguous sentinel: a rendered preview always opens with a
    /// `<!doctype html><html><head>` prologue, so a real `</head>` can never sit at
    /// offset 0. See [`PaneInner::load_preview_document`] for what it guards.
    head_len: Cell<usize>,
    /// The fragment a CROSS-FILE link click asked to scroll to, waiting for the
    /// linked document's `LoadEvent::Finished` edge.
    ///
    /// It belongs to the VIEW, not to the pane — cleared beside `loaded` and
    /// `head_len` when the view goes — because a fragment held for a document that no
    /// longer exists would be applied to whatever loads next.
    ///
    /// A successful `preview_file` writes this slot UNCONDITIONALLY, to `Some` or to
    /// `None` ([`PaneInner::on_preview_link`]). Writing it only for a `Some` leaves a
    /// stale fragment behind: click a cross-file link carrying `#a`, then before the
    /// render lands click another carrying none, and the second file scrolls to the
    /// first file's anchor.
    pending_fragment: RefCell<Option<String>>,
}

impl PaneWidgets {
    /// Binds `area` to window `win` (and `grid`, when already resolved), wraps it in
    /// the pane's host overlay, and zeroes the pointer state.
    ///
    /// Both construction sites (`ensure_initial_pane`, `apply_split`) go through
    /// here, so a future field is one edit rather than two.
    fn new(win: i64, grid: Option<u64>, area: gtk::DrawingArea) -> Self {
        let host = gtk::Overlay::builder().hexpand(true).vexpand(true).build();
        host.set_child(Some(&area));
        // Focus must not be able to enter this subtree: `focusable(false)` keeps the
        // host itself out of the focus chain and `can_focus(false)` keeps every
        // descendant — including a `webkit6::WebView` added later — out of it too.
        // The pane areas' non-focusability is the invariant
        // `click_then_close_keeps_keyboard_input_alive_under_cage` executes; a
        // focusable widget under `pane_area` that a rebuild destroys is the wedge it
        // exists to prevent.
        host.set_focusable(false);
        host.set_can_focus(false);
        Self {
            win,
            grid: Cell::new(grid),
            host,
            area,
            drag_grid: Cell::new(None),
            drag_button: Cell::new(None),
            scroll: Cell::new(PaneScroll::new()),
            pointer: Cell::new((0.0, 0.0)),
            // The unnamed-buffer shape, taken from the same pure seam the live
            // value comes from rather than hand-written here.
            status: RefCell::new(wolf_core::pane_status("", None, None)),
            full_width: Cell::new(false),
            requested_grid: Cell::new(None),
            preview: RefCell::new(None),
            web: RefCell::new(None),
            loaded: Cell::new(None),
            current_load: Cell::new(None),
            head_len: Cell::new(0),
            pending_fragment: RefCell::new(None),
        }
    }
}

/// One pane's raw SNAPSHOT join: what [`PaneInner::refresh_pane_state`]'s guarded
/// read phase produces, before its guard-free core calls turn it into a
/// [`PaneState`].
struct PaneJoin {
    /// The pane this join belongs to.
    id: PaneId,
    /// Its bound grid, for `WolfCore::pane_status`.
    grid: u64,
    /// Its bound Neovim window, for `WolfCore::full_width_for_win` and as the key
    /// `Snapshot::previews` was looked up under.
    win: i64,
    /// Its `Snapshot::previews` entry, or `None` for an editor pane.
    preview: Option<PreviewInfo>,
    /// `previews` joined to `markdown_ticks` on `source_bufnr`, `None` for an editor
    /// pane.
    load: Option<PreviewLoad>,
}

/// One pane's per-wake state, carried from [`PaneInner::refresh_pane_state`]'s
/// read phase to its write-back phase.
///
/// One value rather than a five-tuple, because two of its fields are `Option`s of
/// unrelated types and a positional tuple of that shape is exactly where a preview's
/// info and a preview's load get swapped.
struct PaneState {
    /// The pane this state belongs to.
    id: PaneId,
    /// Its statusbar strings, from `WolfCore::pane_status` or
    /// [`preview_ui::preview_status`].
    status: PaneStatus,
    /// Its column-cap flag, from `WolfCore::full_width_for_win` or
    /// [`PreviewInfo::source_full_width`].
    full_width: bool,
    /// Its bound preview source, or `None` for an editor pane.
    preview: Option<PreviewInfo>,
    /// The `(source_bufnr, source_tick)` join for this wake, `None` for an editor
    /// pane.
    load: Option<PreviewLoad>,
}

/// One pane whose preview document has to be re-rendered on this wake, with
/// everything the render and the load need — all of it read in ONE `panes` borrow.
///
/// A named value rather than [`PaneInner::refresh_previews`]'s former
/// `(PaneId, PreviewLoad)` tuple, the file's own [`PaneJoin`] / [`PaneState`]
/// precedent. The four fields are read at the SAME INSTANT, and that is the whole
/// reason the load strategy is decided here rather than at the load: `update`
/// depends on `loaded` and on whether a fragment is pending, and both can move
/// between the borrow and the blocking `render_preview` that follows it.
struct DueRender {
    /// The pane to load into.
    id: PaneId,
    /// The `(source_bufnr, source_tick)` pair being rendered, recorded as `loaded`
    /// once the document is on screen.
    load: PreviewLoad,
    /// The source file's absolute path, for [`preview_link::file_uri_for`]. Empty
    /// for a nameless buffer, which is exactly the case that yields no base URI.
    source_path: String,
    /// Whether this document may be swapped in place or must go through a full
    /// `load_html` ([`preview_ui::document_update`]).
    update: DocumentUpdate,
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
    /// The one-row cmdline strip, painted from [`Snapshot::cmdline`].
    cmdline_area: gtk::DrawingArea,
    /// The message toast, an overlay surface sitting directly above the strip.
    ///
    /// An overlay child rather than a taller strip: the strip's height is
    /// subtracted from the pane area, so a strip that grew with its content would
    /// issue a `resize_grid` on every message and reflow the buffer under the
    /// reader. It takes pointer input, which is what consumes a scroll gesture
    /// landing on it instead of forwarding it into Neovim.
    toast_area: gtk::DrawingArea,
    /// The completion popup, an overlay surface anchored to a cell.
    ///
    /// `can_target(false)`: it accepts no input at all, for the reason
    /// [`crate::scrollbar`] gives — an indicator that never claimed to take any
    /// beats a control that visibly refuses it.
    pum_area: gtk::DrawingArea,
    /// How many lines back from the newest the toast is scrolled.
    toast_scroll: Cell<usize>,
    /// The toast's own carried sub-threshold scroll remainder.
    ///
    /// A bare [`ScrollAccumulator`] rather than a [`PaneScroll`]: the toast has no
    /// target grid, so `PaneScroll`'s reset-on-grid-change rule would have nothing
    /// to compare and a synthetic grid id would make it silently inert. The
    /// consequence is that nothing clears the carry — neither the prompt-answered
    /// branch of [`refresh_message_surfaces`], which zeroes `toast_scroll` and
    /// `toast_lines`, nor [`cmdline_ui::carried_scroll`] — so a sub-threshold
    /// remainder left by one message counts toward the first step on the next. It is
    /// bounded by one threshold and self-correcting, which is why it is left alone.
    ///
    /// [`refresh_message_surfaces`]: PaneInner::refresh_message_surfaces
    toast_scroll_acc: Cell<ScrollAccumulator>,
    /// The toast's line count on the previous wake, for
    /// [`cmdline_ui::carried_scroll`].
    toast_lines: Cell<usize>,
    /// Whether the previous wake's toast was an unanswered `confirm` prompt, for
    /// [`cmdline_ui::prompt_answered`].
    toast_was_prompt: Cell<bool>,
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
    /// The terminal drawer's binding: the pane bound to the drawer window, plus the
    /// most recently active EDITOR pane the focus bridge returns to.
    ///
    /// A `Cell` rather than a `RefCell` because [`DrawerState`] is [`Copy`], the same
    /// reason `active_pane` is one — every read is a copy out and every write a copy
    /// back, so no borrow can be held across a [`WolfCore`] call.
    ///
    /// The drawer is deliberately NOT a [`PaneTree`] leaf; `crate::drawer` and
    /// `crates/wolf-gnome/DECISIONS.md` hold the reasons. It IS an ordinary `panes`
    /// entry, which is what gives it a statusbar, a column cap, a grid resize and a
    /// paint with no drawer branch anywhere on those paths.
    drawer: Cell<DrawerState>,
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
        self.refresh_pane_state();
        // AFTER `refresh_pane_state`, which is what fills the per-pane `preview` and
        // `current_load` this reads, and BEFORE `sync_pane_grids`, so a pane that
        // just became (or stopped being) a preview has its web view attached or gone
        // before any resize is issued against the frame it shares.
        self.refresh_previews();
        // BEFORE `sync_pane_grids`, which is what turns an allocation into a
        // `resize_grid`: re-stating the strip's height request here is what makes a
        // toplevel resize reach the drawer at all, since a content height set on a
        // frame is only allocated on the next one.
        self.apply_drawer_frame();
        // AFTER `refresh_pane_state`, because the cap it re-requests against depends
        // on the flag that call just cached; BEFORE `update_active_pane`, because
        // "read core state, then decide, then paint" is the order already here.
        self.sync_pane_grids();
        self.update_active_pane();
        // AFTER `sync_pane_grids`, because a grid-anchored popup reads the pane
        // rectangle that call may have just re-requested, and BEFORE `queue_redraw`,
        // because it decides whether either surface is on screen at all.
        self.refresh_message_surfaces();
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
    ///    **This is a BLOCKING RPC**, one of the ten this crate issues
    ///    (`crates/wolf-gnome/DECISIONS.md`, "Crash and stall recovery"). Every
    ///    other core call this crate makes is a snapshot read or a queued input. It
    ///    is acceptable HERE for a reason the others do not share: it happens on
    ///    the LAUNCH path only — at most once per restored tab, bounded by
    ///    `state_seam::MAX_RESTORE_TABS`, before the window is interactive — and
    ///    never on the crash/restart path, see `shell.rs`'s "never call a
    ///    `block_on_rpc` on the restart path" rule, which stands unchanged. A
    ///    wedged nvim at launch marks the core `NotResponding`, and the crash
    ///    overlay presents it.
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
        // `panes` was replaced wholesale, so a bound drawer's pane is gone with it —
        // and the drawer is never one of the restored leaves, because it is excluded
        // from the saved layout by construction (see `Self::layout_snapshot`).
        //
        // This call is DEFENSIVE, not dead, and it can never actually fire: both
        // callers run on the LAUNCH path — `consume_pending_layout` while exactly
        // one pane exists, and `PaneHandle::realize_layout` on a page created
        // moments earlier by the restore materialization — so no chord has had a
        // window to open a drawer in yet. It stays because the alternative is a
        // `panes` replacement that leaves `DrawerState` pointing at a pane id
        // nothing holds.
        self.clear_drawer();

        // The guarded path: it samples `focus_inside_pane_area()` BEFORE tearing
        // anything down, and its `pane_area.remove(..)` loop unparents the old
        // single pane's area — which, in the guaranteed one-pane pre-state, is
        // `pane_area`'s only child and is no longer in `panes`.
        self.rebuild_pane_area();
        self.set_active_pane(leaf_ids.first().copied());
        self.resolve_pending_grids();
        // Without this every restored pane paints `[No Name]` for one frame: this
        // path early-returns from the wake, so the normal refresh never runs.
        self.refresh_pane_state();
        // Same early return, same reason: a restored preview pane would otherwise
        // wait a whole wake for its web view, showing the empty scratch grid where
        // the document belongs.
        self.refresh_previews();
        // Same reason again: a restored pane whose buffer carries the full-width
        // flag would otherwise spend a frame at the capped width, since nothing else
        // on this path re-requests a grid size.
        self.sync_pane_grids();
        self.queue_redraw();
    }

    /// Reads every bound pane's statusbar strings, its column-cap flag and — for a
    /// preview pane — its bound source and that source's `changedtick`, and caches
    /// all of them on the pane.
    ///
    /// **This is the only place `WolfCore::pane_status`, `full_width_for_win` and
    /// `display_path` are called**, and the cache is what makes that possible:
    /// [`Self::draw_pane`] holds a snapshot read guard for its whole body while all
    /// three take `read_snapshot()` or `live_cwd()` again internally, and
    /// `shell.rs`'s borrow-discipline section records that a re-entrant read can
    /// deadlock behind a queued writer. All of them are RPC-free, and they cost
    /// exactly two per bound pane per wake — the same budget [`Self::layout_snapshot`]
    /// spends, since that reads this cache rather than calling again.
    ///
    /// An EDITOR pane takes `pane_status` + `full_width_for_win`. A PREVIEW pane
    /// takes `display_path` + [`preview_ui::preview_status`] and its cap from
    /// [`PreviewInfo::source_full_width`], and **`pane_status` is not called for it
    /// at all**: preview windows are omitted from `win_buffers` entirely, so core
    /// answers `[No Name]` for one. `full_width_for_win` is skipped for the same
    /// reason — its entry for a preview window describes the scratch buffer, not the
    /// source the pane is showing.
    ///
    /// The full-width flag is per WINDOW and never per application, because
    /// `b:wolf_full_width` is a BUFFER variable reported per window: two panes side
    /// by side can legitimately differ, and one window's flag must never reframe
    /// another's.
    ///
    /// Two-phase, like [`Self::layout_snapshot`]: the `panes` borrow that collects
    /// the targets is dropped before the core borrow is taken, the ONE snapshot
    /// guard that joins `Snapshot::previews` to `Snapshot::markdown_ticks` is bound
    /// to a local and dropped before any core call, and the core borrow is dropped
    /// before the results are written back.
    ///
    /// **The terminal drawer takes this path with NO branch of its own**, because it
    /// iterates `panes` and the drawer is an ordinary entry in it. That is how the
    /// drawer gets a statusbar — `bash` over the raw `term://…` string, which is
    /// exactly what macOS shows, since `paintStatusbar` has no drawer case either —
    /// and how it gets the `b:wolf_full_width` flag that makes it span the window.
    fn refresh_pane_state(&self) {
        let targets: Vec<(PaneId, u64, i64)> = {
            let panes = self.panes.borrow();
            panes
                .iter()
                .filter_map(|(id, p)| p.grid.get().map(|g| (*id, g, p.win)))
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        let states: Vec<PaneState> = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            // Phase one: the snapshot join, under a guard bound to a local so it is
            // gone before the guard-free core calls below. `previews` is keyed by the
            // preview WINDOW handle, which is `PaneWidgets::win`, so this is a map
            // lookup per pane rather than a scan.
            let joined: Vec<PaneJoin> = {
                let snap = core.read_snapshot();
                targets
                    .into_iter()
                    .map(|(id, grid, win)| {
                        let preview = snap.previews.get(&win).cloned();
                        let load = preview.as_ref().map(|info| PreviewLoad {
                            source_bufnr: info.source_bufnr,
                            // A source nvim has never reported a tick for reads 0,
                            // which is a real key rather than a missing one: the pair
                            // still changes when the tick arrives.
                            source_tick: snap
                                .markdown_ticks
                                .get(&info.source_bufnr)
                                .copied()
                                .unwrap_or(0),
                        });
                        PaneJoin {
                            id,
                            grid,
                            win,
                            preview,
                            load,
                        }
                    })
                    .collect()
            };
            // Phase two: the core calls, with no guard held.
            joined
                .into_iter()
                .map(|j| match j.preview {
                    Some(info) => {
                        let source = info.source_path.to_string_lossy().into_owned();
                        let display = core.display_path(&source);
                        PaneState {
                            id: j.id,
                            status: preview_ui::preview_status(&source, &display),
                            full_width: info.source_full_width,
                            preview: Some(info),
                            load: j.load,
                        }
                    }
                    None => PaneState {
                        id: j.id,
                        status: core.pane_status(j.grid),
                        full_width: core.full_width_for_win(j.win),
                        preview: None,
                        load: None,
                    },
                })
                .collect()
        };
        {
            let panes = self.panes.borrow();
            for state in states {
                if let Some(p) = panes.get(&state.id) {
                    *p.status.borrow_mut() = state.status;
                    p.full_width.set(state.full_width);
                    *p.preview.borrow_mut() = state.preview;
                    p.current_load.set(state.load);
                }
            }
        }
    }

    /// Each leaf's persisted content, resolved from the snapshot's WINDOW-KEYED
    /// maps by the pane's own Neovim window handle — or `None` if any leaf is
    /// unresolvable.
    ///
    /// `Snapshot::previews` and `Snapshot::win_buffers` are GLOBAL across tabpages
    /// (`runtime/nvim/init.lua` sweeps `nvim_list_wins()`), so this answers the same
    /// values for a background tab as for the selected one, with no RPC and no
    /// refresh. That is the whole reason a window may persist every tab: the
    /// `[No Name]` reading is a property of `WolfCore::pane_status`, which joins
    /// through the current-tabpage-only `Snapshot::windows`, and which this does not
    /// call.
    ///
    /// **There is no grid filter here, deliberately.** The grid is irrelevant to a
    /// window-keyed read, and a leaf whose grid has not resolved is now covered by
    /// the map lookup — it answers [`pane_layout::WinContent::Unknown`] and refuses
    /// the tab, rather than being silently dropped into an empty leaf.
    ///
    /// The `""` a leaf may carry comes only from `win_buffers[win].path` being
    /// genuinely `None` — a real unnamed buffer. Absence is never mapped onto it;
    /// see [`pane_layout::resolve_leaves`].
    fn leaf_contents(&self) -> Option<HashMap<PaneId, pane_layout::LeafContent>> {
        let leaves: Vec<(PaneId, pane_layout::WinContent)> = {
            let core_ref = self.core.borrow();
            let core = core_ref.as_ref()?;
            // ONE guard, bound to a local, dropped with the tree and panes borrows
            // at the end of this block and before any other call.
            let snap = core.read_snapshot();
            let tree = self.tree.borrow();
            let panes = self.panes.borrow();
            tree.leaves()
                .into_iter()
                .map(|id| {
                    let content = panes
                        .get(&id)
                        .map_or(pane_layout::WinContent::Unknown, |p| {
                            match snap.previews.get(&p.win) {
                                Some(info) => pane_layout::WinContent::Preview(
                                    info.source_path.to_string_lossy().into_owned(),
                                ),
                                None => match snap.win_buffers.get(&p.win) {
                                    Some(buf) => pane_layout::WinContent::Buffer(
                                        buf.path
                                            .as_ref()
                                            // `None` is a real UNNAMED buffer, which is
                                            // a resolved answer of `""` — never the
                                            // absent-window case above.
                                            .map(|path| path.to_string_lossy().into_owned())
                                            .unwrap_or_default(),
                                    ),
                                    None => pane_layout::WinContent::Unknown,
                                },
                            }
                        });
                    (id, content)
                })
                .collect()
        };
        pane_layout::resolve_leaves(&leaves)
    }

    /// The tab's gather answer for `state.json`.
    ///
    /// [`pane_layout::LayoutGather::Refuse`] when [`Self::leaf_contents`] could not
    /// resolve every leaf, which means "contribute the CACHE" one layer up rather
    /// than "contribute nothing"; otherwise
    /// [`pane_layout::LayoutGather::Ready`] carrying
    /// [`state_seam::persistable_layout`]'s answer, whose `None` is the CONTENT
    /// suppression rule and never an arity test.
    ///
    /// No core call beyond the one snapshot read [`Self::leaf_contents`] takes, so
    /// this is affordable for every tab on every render wake.
    ///
    /// **The terminal drawer is excluded BY CONSTRUCTION and needs no branch here.**
    /// This walks `tree.leaves()`, and the drawer is not a leaf, so its
    /// `term://<cwd>//<pid>:…` buffer name can never enter `state.json` — where the
    /// restore path would hand it to `:edit` at the next launch and Neovim would
    /// spawn a shell for it. Putting the drawer in the tree is what would break
    /// this, not editing this function.
    fn layout_snapshot(&self) -> pane_layout::LayoutGather {
        let Some(contents) = self.leaf_contents() else {
            return pane_layout::LayoutGather::Refuse;
        };
        let layout = self.tree.borrow().layout();
        pane_layout::LayoutGather::Ready(state_seam::persistable_layout(pane_layout::encode(
            &layout, &contents,
        )))
    }

    /// `(panes with a RESOLVED grid, windows nvim has reported a buffer or a preview
    /// source for)` — the two readiness counts [`state_seam::may_snapshot`] gates on.
    ///
    /// The reported count is `win_buffers.len() + previews.len()`, not
    /// `win_buffers.len()` alone: preview windows are omitted from `win_buffers`
    /// entirely, so a tab in which EVERY window is a preview reports zero, and
    /// `may_snapshot` refuses. Persistence then silently stops updating for that tab
    /// — reachable by previewing into the active pane of a one-pane window. The two
    /// maps are disjoint by construction, so summing them cannot double-count.
    fn snapshot_readiness(&self) -> (usize, usize) {
        let bound = self
            .panes
            .borrow()
            .values()
            .filter(|p| p.grid.get().is_some())
            .count();
        let reported = {
            let core_ref = self.core.borrow();
            core_ref.as_ref().map_or(0, |core| {
                let snap = core.read_snapshot();
                snap.win_buffers.len() + snap.previews.len()
            })
        };
        (bound, reported)
    }

    /// Drains and applies all pending layout intents (splits, focus moves and the
    /// terminal drawer).
    ///
    /// [`LayoutIntent::DrawerOpen`] binds the drawer through
    /// [`Self::apply_drawer_open`]. There is no matching CLOSE intent and there is
    /// not meant to be one — `wolf_core::layout` records that hiding the drawer is a
    /// plain window close, so [`Self::detect_closes`] is what unbinds it. The
    /// drawer's own rulebook is [`crate::drawer`] and its durable constraints are in
    /// `crates/wolf-gnome/DECISIONS.md`.
    ///
    /// The match stays EXHAUSTIVE rather than taking a `_` wildcard, so adding a
    /// variant to the shared `wolf_core::LayoutIntent` breaks this build rather than
    /// being silently swallowed here.
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
                LayoutIntent::DrawerOpen { win } => self.apply_drawer_open(win),
            }
        }
    }

    /// Binds the terminal drawer to Neovim window `win`: a new pane recorded in
    /// [`Self::drawer`] and in `panes`, but NOT in [`PaneTree`].
    ///
    /// A port of `PaneContainerView.applyDrawerOpen`. The pane is an ordinary
    /// `panes` entry, so it picks up its statusbar, its column cap, its own
    /// `resize_grid` and its paint from the existing per-pane paths; what it does
    /// not pick up is a place in the tree, and [`Self::rebuild_pane_area`] is
    /// therefore the one thing that has to know about it structurally.
    ///
    /// **The idempotence guard reads [`DrawerState::is_open`], never
    /// `panes.contains_key`, and that is deliberate.** A guard that consulted
    /// `panes` would SELF-HEAL after an in-place Neovim restart — the restart
    /// clears `panes` but not this field — and would thereby delete the only
    /// falsifier the restart smoke has. The clearing belongs to
    /// [`Self::reset_after_restart`], where it can be observed to be missing; it
    /// must not be papered over here.
    ///
    /// The grid may be unresolved at this point and is left pending if so, the same
    /// as [`Self::apply_split`]. Nothing races it: the drawer's grid is created
    /// before the intent is emitted, so `grid_for_win` answers on this very wake.
    fn apply_drawer_open(self: &Rc<Self>, win: i64) {
        let mut state = self.drawer.get();
        if state.is_open() || self.pane_for_win(win).is_some() {
            return;
        }
        let id = self.alloc_pane_id();
        state.bind(id);
        // Published BEFORE the rebuild, because `rebuild_pane_area` reads this field
        // to find the host it has to append below the tree.
        self.drawer.set(state);

        let grid = {
            let core_ref = self.core.borrow();
            core_ref
                .as_ref()
                .and_then(|c| c.read_snapshot().grid_for_win(win))
        };
        let area = self.make_pane_area(id);
        self.panes
            .borrow_mut()
            .insert(id, PaneWidgets::new(win, grid, area));
        self.rebuild_pane_area();
        self.set_active_pane(Some(id));
    }

    /// Applies a split intent: split the `from` pane (fallback: active pane), then
    /// bind a new pane to `win` (grid resolved now or pending). Idempotent if a pane
    /// already exists for `win`.
    ///
    /// A split fired from inside the DRAWER is dropped, matching
    /// `PaneContainerView.applySplit`. The drawer is not a tree leaf, so
    /// [`PaneTree::split`] would no-op on it while the rest of this function still
    /// inserted a [`PaneWidgets`] — an invisible pane bound to a real Neovim window,
    /// which nothing would ever paint, size or close.
    fn apply_split(self: &Rc<Self>, orientation: SplitOrientation, from: i64, win: i64) {
        if self.pane_for_win(win).is_some() {
            return;
        }
        let Some(target) = self.pane_for_win(from).or_else(|| self.active_pane.get()) else {
            return;
        };
        if self.drawer.get().is_drawer(target) {
            return;
        }
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

    /// Applies a focus-direction intent: resolve the target of the `from` pane
    /// (fallback: active pane) in `dir` through [`drawer::focus_target`], then
    /// `focus_win` its window.
    ///
    /// **The whole answer comes from the pure seam, including the editor <-> drawer
    /// bridge**, because Neovim emits `<C-w>h/j/k/l` as a PURE intent and declines to
    /// act on it: `<C-w>k` pressed inside the drawer leaves `active_win` where it
    /// was, so without the value this function computes the drawer is a room with no
    /// exit. The six rules, and why the raw pane-area size is the right thing to
    /// resolve them over, are [`drawer::focus_target`]'s.
    ///
    /// The tree borrow is a temporary bound to the `target` statement and is
    /// therefore dropped before the core borrow below — this file's rule, and not
    /// optional here: `focus_win` is a core call.
    fn apply_focus(&self, dir: wolf_core::FocusDir, from: i64) {
        let width = f64::from(self.pane_area.width());
        let height = f64::from(self.pane_area.height());
        let Some(origin) = self.pane_for_win(from).or_else(|| self.active_pane.get()) else {
            return;
        };
        let target = drawer::focus_target(
            &self.tree.borrow(),
            origin,
            dir,
            self.drawer.get(),
            width,
            height,
        );
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
    ///
    /// **This is also the drawer's only unbind path.** Hiding the drawer emits no
    /// intent at all (`wolf_core::layout` records why), so the vanished grid is the
    /// whole signal. The drawer is handled BEFORE the loop and separately from it,
    /// for three reasons, each of which is a bug if it is undone:
    ///
    /// - no [`PaneTree::remove`], because it was never a leaf;
    /// - no last-leaf guard, because that guard exists to keep the TREE non-empty
    ///   and the drawer is not in it;
    /// - and it is skipped by the loop rather than merely handled first, because a
    ///   drawer left in the list could trip the loop's `leaves().len() <= 1` `break`
    ///   and leave a genuinely closed editor pane on screen.
    ///
    /// A restart is a different matter and this function cannot help there: the dead
    /// Neovim's grid SURVIVES in the snapshot, so no pane looks closed. See
    /// [`Self::reset_after_restart`].
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

        let closed_drawer = self.drawer.get().id().filter(|d| closed.contains(d));
        {
            let mut tree = self.tree.borrow_mut();
            let mut panes = self.panes.borrow_mut();
            if let Some(drawer_id) = closed_drawer {
                panes.remove(&drawer_id);
                // `clear()` drops the remembered EDITOR pane along with the drawer,
                // where macOS keeps its `lastEditorPane` across a drawer close. A
                // deliberate simplification: it costs one MRU hop the first time the
                // drawer is reopened — `drawer_up_target` falls back to the
                // bottom-most leaf — and removes a second lifetime to reason about.
                self.clear_drawer();
            }
            for id in closed.iter().filter(|id| Some(**id) != closed_drawer) {
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
            if let Some((grid, cols, rows)) = self.pending_resize(id, w, h) {
                if let Some(core) = self.core.borrow().as_ref() {
                    core.resize_grid(grid, cols, rows);
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
    ///
    /// Every pane area means the drawer's too: it iterates `panes`, and
    /// [`Self::draw_pane`] resolves whatever grid a pane is bound to without ever
    /// looking at its buffer type, so a terminal grid paints through the same
    /// [`crate::paint`] seam an editor grid does.
    fn queue_redraw(&self) {
        {
            let panes = self.panes.borrow();
            for p in panes.values() {
                p.area.queue_draw();
            }
        }
        self.cmdline_area.queue_draw();
        self.toast_area.queue_draw();
        self.pum_area.queue_draw();
    }

    /// Unbinds the drawer and forgets the remembered editor pane
    /// ([`DrawerState::clear`]).
    ///
    /// Its three callers are the three places a pane set stops describing the live
    /// Neovim: the drawer's own window closing ([`Self::detect_closes`]), a restored
    /// layout replacing `panes` wholesale ([`Self::apply_layout_rebuild`]), and an
    /// in-place restart ([`Self::reset_after_restart`]). A `Cell` cannot be mutated
    /// through a method, so the copy-out/copy-back lives here once rather than three
    /// times.
    fn clear_drawer(&self) {
        let mut state = self.drawer.get();
        state.clear();
        self.drawer.set(state);
    }

    /// Sizes the drawer strip: no vertical expansion, and a content height of
    /// [`drawer::drawer_split`]'s share of the CURRENT pane-area height.
    ///
    /// A no-op when no drawer is bound, which is the overwhelmingly common case.
    ///
    /// ## Why a fixed height and not a `gtk::Paned`
    ///
    /// A paned would hand GTK the whole re-derivation for free, and it is still the
    /// wrong control. Its handle is DRAGGABLE, and a drag of it has nowhere to go:
    /// the drawer is not in the [`PaneTree`], so [`PaneTree::set_divider_fraction`]
    /// — the sole path a dragged position takes to the tree, and from there to
    /// `state.json` — cannot represent it. The position would also be re-applied
    /// from the weights on the next `max-position` notify, so the handle would snap
    /// back under the user's pointer. That is a control which visibly refuses input,
    /// which is exactly the trade [`crate::scrollbar`] and
    /// `crates/wolf-gnome/DECISIONS.md` already rule out.
    ///
    /// ## Two frames to converge on a resize
    ///
    /// The height is computed from `pane_area`'s allocation, and setting a content
    /// height does not change an allocation — GTK does that on the next layout pass.
    /// So a toplevel resize settles in two frames: this reads the OLD pane-area
    /// height on the frame the resize lands, and the corrected one on the next wake.
    /// **No test observes it**: nothing under `cage` can resize the toplevel, so the
    /// convergence is an unproven row in `crates/wolf-gnome/tests/README.md` rather
    /// than an asserted behaviour.
    ///
    /// A collapsed split — which only a pane area of zero or non-finite height
    /// produces, i.e. before the first allocation — leaves the minimum content
    /// height [`Self::make_pane_area`] set, so the strip is never REQUESTED at zero
    /// pixels and recovers on the next wake.
    fn apply_drawer_frame(&self) {
        let Some(id) = self.drawer.get().id() else {
            return;
        };
        let panes = self.panes.borrow();
        let Some(p) = panes.get(&id) else {
            return;
        };
        // The same floor every other pane gets, so a short window cannot allocate a
        // strip too thin to show one row of its grid.
        let (_, min_h) = editor_inset::min_pane_size(
            self.metrics.cell_w,
            self.metrics.cell_h,
            self.statusbar.height,
            EDITOR_PADDING_PX,
        );
        let split = drawer::drawer_split(
            f64::from(self.pane_area.height()),
            true,
            drawer::DRAWER_WEIGHT,
            min_h,
        );
        // The HOST is what `pane_area` parents, so the expansion flag belongs to it;
        // the height request belongs to the `DrawingArea` the overlay measures.
        p.host.set_vexpand(false);
        if split.drawer_height > 0.0 {
            p.area.set_content_height(px(split.drawer_height));
        }
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

    /// Sets the active pane, records it as the drawer bridge's most-recently-used
    /// EDITOR pane, and refreshes the focus-ring CSS across all panes.
    ///
    /// The MRU is recorded HERE and nowhere else, so every route into a pane —
    /// intent, click, restore, `active_win` reconciliation — funnels through one
    /// place. [`DrawerState::record_active`] applies the rule: only a live tree LEAF
    /// that is not the drawer is remembered, which is what makes `<C-w>k` from the
    /// drawer able to leave it.
    fn set_active_pane(&self, pane: Option<PaneId>) {
        self.active_pane.set(pane);
        if let Some(pane) = pane {
            let is_leaf = self.tree.borrow().leaves().contains(&pane);
            let mut state = self.drawer.get();
            state.record_active(pane, is_leaf);
            self.drawer.set(state);
        }
        let panes = self.panes.borrow();
        for (pid, p) in panes.iter() {
            if Some(*pid) == pane {
                p.area.add_css_class(ACTIVE_PANE_CSS);
            } else {
                p.area.remove_css_class(ACTIVE_PANE_CSS);
            }
        }
    }

    /// The column cap this pane's grid is measured against, for a window whose
    /// buffer carries `full_width`.
    ///
    /// The flag comes from the pane's own cached `full_width`, never from a global:
    /// `b:wolf_full_width` is a buffer variable reported per window, so two panes
    /// side by side can legitimately disagree.
    fn pane_cap(&self, full_width: bool) -> ColumnCap {
        editor_inset::column_cap(full_width, self.metrics.cell_w)
    }

    /// The Neovim grid size for a pane allocated `width` x `height` device pixels,
    /// once its statusbar strip and its four padding gutters are removed and `cap`
    /// is applied.
    ///
    /// The one place a PANE's allocation becomes a grid size. `geometry::grid_size_for`
    /// is still reachable and still correct — [`Self::resize_global`] uses it — but a
    /// pane must never call it directly, or the grid it asks Neovim for is the size of
    /// the whole `DrawingArea` and the bottom rows land under the neighbouring pane.
    fn pane_grid_size(&self, width: i32, height: i32, cap: ColumnCap) -> (u16, u16) {
        editor_inset::grid_size_for_pane(
            width,
            height,
            self.statusbar.height,
            EDITOR_PADDING_PX,
            self.metrics.cell_h,
            cap,
        )
    }

    /// The pane-local rectangle the grid paints into, for a pane of `width` x
    /// `height` device pixels under `cap`.
    ///
    /// The paint path has its allocation handed to it by the draw func; the pointer
    /// paths read it off the widget through [`Self::pane_grid_rect`]. Both derive
    /// the rect fresh rather than caching it: the allocation moves with every
    /// divider drag and window resize, and a stale rect would offset every click by
    /// the difference.
    fn pane_grid_rect_for(&self, width: f64, height: f64, cap: ColumnCap) -> GridRect {
        editor_inset::grid_rect(width, height, self.statusbar.height, EDITOR_PADDING_PX, cap)
    }

    /// The pane-local rectangle `area`'s grid paints into, from its CURRENT
    /// allocation and under `cap`.
    fn pane_grid_rect(&self, area: &gtk::DrawingArea, cap: ColumnCap) -> GridRect {
        self.pane_grid_rect_for(f64::from(area.width()), f64::from(area.height()), cap)
    }

    /// The `(grid, cols, rows)` pane `pane_id` owes Neovim, or `None` when the size
    /// it last asked for is still current.
    ///
    /// **It RECORDS the returned size as requested**, so a caller that drops the
    /// answer cannot cause the same request to be produced again on the next wake.
    /// The comparison is against `requested_grid` and never against the snapshot's
    /// live grid: `resize_grid` is fire-and-forget and Neovim answers on a later
    /// flush, so comparing against the snapshot would re-request on every wake until
    /// it caught up.
    fn pending_resize(&self, pane_id: PaneId, width: i32, height: i32) -> Option<(u64, u16, u16)> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let panes = self.panes.borrow();
        let p = panes.get(&pane_id)?;
        let grid = p.grid.get()?;
        let size = self.pane_grid_size(width, height, self.pane_cap(p.full_width.get()));
        if p.requested_grid.get() == Some(size) {
            return None;
        }
        p.requested_grid.set(Some(size));
        Some((grid, size.0, size.1))
    }

    /// Re-requests every bound pane's grid size.
    ///
    /// **This is what makes the full-width toggle take effect.** Lifting the cap
    /// changes a pane's COLUMN COUNT while its widget allocation holds still, so
    /// `connect_resize` never fires for it and nothing else in this file would ever
    /// tell Neovim. [`Self::pending_resize`]'s `requested_grid` comparison is what
    /// keeps this from issuing a resize per wake in the overwhelmingly common case
    /// where nothing moved.
    ///
    /// **This is also what sizes the terminal drawer's grid, with no branch for it**,
    /// since it iterates `panes`. That is not a convenience: an outer `resize`
    /// re-divides only the grids the front-end has NOT sized itself, so a drawer left
    /// unsized would keep Neovim's own third-of-`&lines` split and drift away from
    /// its GTK strip on the first window resize.
    fn sync_pane_grids(&self) {
        let ids: Vec<PaneId> = self.panes.borrow().keys().copied().collect();
        for id in ids {
            let alloc = self
                .panes
                .borrow()
                .get(&id)
                .map(|p| (p.area.width(), p.area.height()));
            let Some((w, h)) = alloc else {
                continue;
            };
            if let Some((grid, cols, rows)) = self.pending_resize(id, w, h) {
                if let Some(core) = self.core.borrow().as_ref() {
                    core.resize_grid(grid, cols, rows);
                }
            }
        }
    }

    /// Brings every pane's web view into line with its preview state: creates or
    /// removes the view, frames it, and re-loads the ones whose bound source moved.
    ///
    /// Called from [`Self::on_render_wake`] AFTER [`Self::refresh_pane_state`], which
    /// is what fills `preview` and `current_load`, and BEFORE [`Self::sync_pane_grids`];
    /// and from [`Self::apply_layout_rebuild`] after its own `refresh_pane_state`, for
    /// the same reason that path re-runs the other per-wake steps.
    ///
    /// **Nothing on this path may be gated on `Snapshot::seq`.** The `wolf_preview`
    /// and `wolf_markdown_dirty` notifications that carry a preview's source and its
    /// `changedtick` fire the wake WITHOUT bumping `seq`, so a `seq` gate would
    /// suppress every reload this function exists to issue.
    fn refresh_previews(self: &Rc<Self>) {
        let ids: Vec<PaneId> = self.panes.borrow().keys().copied().collect();
        for id in &ids {
            self.sync_preview_view(*id);
            self.apply_preview_frame(*id);
        }

        // `WolfCore::render_preview` is a BLOCKING RPC on the GTK main thread, so it
        // carries the same liveness gate the two presses do rather than a weaker one
        // (`crates/wolf-gnome/DECISIONS.md`, "Crash and stall recovery"). The read is
        // a lock-free atomic in a borrow of its own.
        let liveness = {
            let core_ref = self.core.borrow();
            core_ref.as_ref().map(WolfCore::liveness)
        };
        if !preview_ui::may_render(liveness) {
            return;
        }

        // Every field of every row is read inside THIS ONE borrow, at one instant.
        // `update` in particular is decided here rather than at the load, because it
        // is a function of `loaded` and of whether a fragment is pending, and the
        // blocking `render_preview` below sits between this borrow and that load.
        let due: Vec<DueRender> = {
            let panes = self.panes.borrow();
            ids.iter()
                .filter_map(|id| {
                    let p = panes.get(id)?;
                    let current = p.current_load.get()?;
                    let loaded = p.loaded.get();
                    if !preview_ui::needs_reload(Some(current), loaded) {
                        return None;
                    }
                    // Each `RefCell` read is its own statement, so no borrow outlives
                    // the value it produces.
                    let source_path = p
                        .preview
                        .borrow()
                        .as_ref()
                        .map(|info| info.source_path.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let fragment_pending = p.pending_fragment.borrow().is_some();
                    Some(DueRender {
                        id: *id,
                        load: current,
                        source_path,
                        update: preview_ui::document_update(current, loaded, fragment_pending),
                    })
                })
                .collect()
        };
        for DueRender {
            id,
            load,
            source_path,
            update,
        } in due
        {
            let html = {
                let core_ref = self.core.borrow();
                let Some(core) = core_ref.as_ref() else {
                    return;
                };
                core.render_preview(load.source_bufnr)
            };
            self.load_preview_document(id, &html, &source_path, update);
            // Recorded WHATEVER the HTML was, the empty string included: an RPC
            // failure answers empty, and leaving `loaded` behind on that would
            // re-arm the reload and re-issue the blocking render on every wake for
            // ever. The next real edit bumps the tick and reloads anyway.
            //
            // It is also recorded on `load_preview_document`'s no-view early return,
            // and that is sound only because of an INVARIANT this loop depends on:
            // `sync_preview_view` ran over these same ids above, and `current_load` is
            // `Some` exactly when `preview` is, so every pane that reaches here already
            // has a web view. Move either of those and this records a document that
            // was loaded into nothing, leaving the pane blank until the source's tick
            // moves again.
            if let Some(p) = self.panes.borrow().get(&id) {
                p.loaded.set(Some(load));
            }
        }
    }

    /// Creates pane `pane_id`'s web view when it becomes a preview, and removes and
    /// drops it when it stops being one. No-op when the two already agree.
    ///
    /// **Lazy, never eager.** A `WebView` spawns its own WebProcess; creating one per
    /// pane at launch would move every existing cage smoke's timing budget for panes
    /// that will never show a preview.
    ///
    /// The view is an OVERLAY child of the pane's `host`, so it composites above the
    /// `DrawingArea` and below the window-level overlay the pickers live in — GTK 4
    /// paints overlay children after the main child and all its descendants, so a
    /// picker is over a preview by construction.
    ///
    /// The two signal handlers are connected HERE, on the one path that creates a
    /// view, so a view can never exist without them: `decide-policy` routes a clicked
    /// link through [`Self::on_preview_link`], and `load-changed` applies a waiting
    /// cross-file fragment through [`Self::on_preview_load_finished`]. Both capture a
    /// [`Weak`] rather than the `Rc`, since the view outlives nothing but is owned by
    /// a `PaneWidgets` this `PaneInner` owns.
    ///
    /// [`Weak`]: std::rc::Weak
    fn sync_preview_view(self: &Rc<Self>, pane_id: PaneId) {
        let panes = self.panes.borrow();
        let Some(p) = panes.get(&pane_id) else {
            return;
        };
        let wants = p.preview.borrow().is_some();
        let has = p.web.borrow().is_some();
        if wants == has {
            return;
        }
        if wants {
            let view = webkit6::WebView::new();
            // Focus must not enter the view, matching its host: `focus_inside_pane_area`
            // guards everything under `pane_area`, and a focusable widget there that a
            // rebuild destroys is the wedge that guard exists to prevent.
            view.set_focusable(false);
            view.set_can_focus(false);
            {
                let weak = Rc::downgrade(self);
                view.connect_decide_policy(move |_view, decision, decision_type| {
                    // `PolicyDecisionType` FIRST, and it is not redundant with the
                    // downcast below: a `NewWindowAction` decision is the SAME class,
                    // so a downcast-only filter would treat a `target="_blank"` as an
                    // in-page navigation. Answering `false` for one is correct — the
                    // default handler opens it the way WebKit would.
                    if decision_type != webkit6::PolicyDecisionType::NavigationAction {
                        return false;
                    }
                    let Some(nav) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>()
                    else {
                        return false;
                    };
                    let Some(action) = nav.navigation_action() else {
                        return false;
                    };
                    // `Other` is what the initial `load_html` itself arrives as, so
                    // filtering on `LinkClicked` is exactly "anything that is not a
                    // user click is allowed through untouched".
                    if action.navigation_type() != webkit6::NavigationType::LinkClicked {
                        return false;
                    }
                    let uri = action.request().and_then(|r| r.uri());
                    let Some(uri) = uri.filter(|u| !u.is_empty()) else {
                        return false;
                    };
                    // ANSWER FIRST, act second. `on_preview_link` can issue the
                    // BLOCKING `preview_file`, and running it before `ignore()` would
                    // hold WebKit's signal emission open for a whole timeout budget.
                    decision.ignore();
                    if let Some(inner) = weak.upgrade() {
                        inner.on_preview_link(pane_id, &uri);
                    }
                    // `true` means "this handler answered the decision"; returning
                    // `false` here would let the default handler call `use_()` on a
                    // decision already answered. Returning `true` UNCONDITIONALLY is
                    // the opposite trap: the initial `load_html`'s own decision would
                    // never be answered and the preview would be permanently blank.
                    true
                });
            }
            {
                let weak = Rc::downgrade(self);
                view.connect_load_changed(move |_view, event| {
                    if event != webkit6::LoadEvent::Finished {
                        return;
                    }
                    if let Some(inner) = weak.upgrade() {
                        inner.on_preview_load_finished(pane_id);
                    }
                });
            }
            p.host.add_overlay(&view);
            // EXPLICIT, even though `false` is GTK's default, because the reason it
            // must stay false is exactly what gets simplified away: a measured overlay
            // child adds its own size request to the overlay's, and a `WebView`'s would
            // inflate the pane minimum this whole crate is calibrated against
            // (`editor_inset::min_pane_size` and every smoke's window geometry).
            p.host.set_measure_overlay(&view, false);
            *p.web.borrow_mut() = Some(view);
        } else {
            // Taken out in its own statement, so no borrow of `web` is live across the
            // GTK teardown below.
            let existing = p.web.borrow_mut().take();
            if let Some(view) = existing {
                p.host.remove_overlay(&view);
                // All three belong to the VIEW, not to the pane: a pane that becomes a
                // preview again gets a fresh view with an empty document, a surviving
                // `head_len` would let the first load into it take the body-swap branch
                // against a `<head>` that is not there, and a surviving
                // `pending_fragment` would scroll the next document loaded into the
                // pane to an anchor the previous one named.
                p.loaded.set(None);
                p.head_len.set(0);
                *p.pending_fragment.borrow_mut() = None;
            }
        }
    }

    /// Carries out what a click on a link inside pane `pane_id`'s preview means,
    /// having already answered WebKit's navigation decision.
    ///
    /// `uri` is what WebKit resolved against the document's base URI, so the
    /// relative-to-absolute arithmetic is already done and
    /// [`preview_link::classify`] only has to name the shape. The match carries no
    /// `_` arm, so a new [`PreviewLink`] variant is an `E0004` here.
    ///
    /// The pane's window handle and its bound source path are looked up PER EVENT
    /// rather than captured in the signal closure ([`Self::on_pane_press`]'s pattern):
    /// both move when the pane is rebound, and following the previous binding would
    /// repoint a pane the user is no longer looking at.
    ///
    /// A `file:` link core declines is PRINTED and never handed to the desktop's
    /// default handler; [`crate::preview_link`] states why that divergence from macOS
    /// is deliberate.
    fn on_preview_link(self: &Rc<Self>, pane_id: PaneId, uri: &str) {
        let win = self.panes.borrow().get(&pane_id).map(|p| p.win);
        let Some(win) = win else {
            return;
        };
        let source_path = self
            .panes
            .borrow()
            .get(&pane_id)
            .map(|p| {
                p.preview
                    .borrow()
                    .as_ref()
                    .map(|info| info.source_path.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        match preview_link::classify(uri, &source_path) {
            PreviewLink::OpenExternally { uri } => {
                let parent = self.toplevel_window();
                launch_uri(parent.as_ref(), &uri, "preview link");
            }
            PreviewLink::ScrollToFragment { fragment } => {
                self.scroll_preview_to_fragment(pane_id, &fragment);
            }
            PreviewLink::PreviewFile { path, fragment } => {
                // `preview_file` is a BLOCKING RPC on the GTK main thread, so it
                // carries the same liveness gate every other one in this crate does
                // (`crates/wolf-gnome/DECISIONS.md`, "Crash and stall recovery").
                let liveness = {
                    let core_ref = self.core.borrow();
                    core_ref.as_ref().map(WolfCore::liveness)
                };
                match preview_ui::preview_file_press(liveness) {
                    PreviewFilePress::Refuse(reason) => {
                        eprintln!("wolf-gnome: preview link not followed: {reason}");
                    }
                    PreviewFilePress::Follow => {
                        let source_bufnr = {
                            let core_ref = self.core.borrow();
                            core_ref
                                .as_ref()
                                .map_or(0, |core| core.preview_file(win, &path))
                        };
                        if source_bufnr > 0 {
                            // UNCONDITIONALLY, to `Some` or to `None`. Writing only a
                            // `Some` leaves a stale fragment behind: click a link
                            // carrying `#a`, then before the render lands click another
                            // carrying none, and the second file scrolls to the first
                            // file's anchor.
                            if let Some(p) = self.panes.borrow().get(&pane_id) {
                                *p.pending_fragment.borrow_mut() = fragment;
                            }
                        } else {
                            eprintln!(
                                "wolf-gnome: preview link not followed: the captive Neovim \
                                 declined to preview {path:?} — it is unreadable, it is not \
                                 Markdown, or the RPC failed"
                            );
                        }
                    }
                }
            }
            PreviewLink::Refuse(reason) => {
                eprintln!("wolf-gnome: preview link not followed: {reason}");
            }
        }
    }

    /// Applies a waiting cross-file fragment once pane `pane_id`'s new document has
    /// finished loading.
    ///
    /// Connected to `load-changed` and reached on [`webkit6::LoadEvent::Finished`]
    /// alone. That edge is produced by a real `load_html` and NEVER by an in-place
    /// body swap, which is why [`preview_ui::document_update`] forces a full load
    /// whenever this slot is occupied.
    ///
    /// The slot is TAKEN in a statement of its own, so the scroll runs once and a
    /// later unrelated load — the next edit's re-render — does not repeat it.
    fn on_preview_load_finished(&self, pane_id: PaneId) {
        let fragment = self
            .panes
            .borrow()
            .get(&pane_id)
            .and_then(|p| p.pending_fragment.borrow_mut().take());
        let Some(fragment) = fragment else {
            return;
        };
        self.scroll_preview_to_fragment(pane_id, &fragment);
    }

    /// Scrolls pane `pane_id`'s preview to the element with id `fragment`.
    ///
    /// Shared by the same-document arm of [`Self::on_preview_link`] and by
    /// [`Self::on_preview_load_finished`], because the two differ only in WHEN they
    /// run. An encoding failure prints and issues nothing rather than falling back to
    /// a raw interpolation, which would be a script-injection seam
    /// ([`preview_link::scroll_into_view_js`]).
    fn scroll_preview_to_fragment(&self, pane_id: PaneId, fragment: &str) {
        let view = self
            .panes
            .borrow()
            .get(&pane_id)
            .and_then(|p| p.web.borrow().clone());
        let Some(view) = view else {
            return;
        };
        match preview_link::scroll_into_view_js(fragment) {
            Ok(script) => {
                view.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, |result| {
                    if let Err(err) = result {
                        eprintln!("wolf-gnome: preview fragment scroll failed: {err}");
                    }
                })
            }
            Err(err) => eprintln!(
                "wolf-gnome: preview fragment {fragment:?} could not be encoded ({err}); \
                 no scroll issued"
            ),
        }
    }

    /// The toplevel [`gtk::Window`] this pane view is inside, or `None` before it is
    /// rooted.
    ///
    /// Only [`launch_uri`] needs it, as the parent the desktop portal centres its
    /// chooser on, and `None` is legal there — it costs the dialog its parent and
    /// nothing else.
    fn toplevel_window(&self) -> Option<gtk::Window> {
        self.pane_area.root().and_downcast::<gtk::Window>()
    }

    /// Frames pane `pane_id`'s web view to the rectangle its grid would have used —
    /// the same [`Self::pane_grid_rect_for`] under the same [`Self::pane_cap`] — by
    /// margins on the overlay child.
    ///
    /// The cap comes from the pane's own cached `full_width`, which for a preview pane
    /// is [`PreviewInfo::source_full_width`]: the SOURCE buffer's flag, never the
    /// preview window's, whose entry describes a scratch buffer nobody reads. The
    /// HTML itself is rendered at full width (`crates/wolf-core/src/nvim.rs`), so this
    /// frame is the only thing that applies the column cap at all.
    fn apply_preview_frame(&self, pane_id: PaneId) {
        let panes = self.panes.borrow();
        let Some(p) = panes.get(&pane_id) else {
            return;
        };
        let view = p.web.borrow().clone();
        let Some(view) = view else {
            return;
        };
        let width = f64::from(p.area.width());
        let height = f64::from(p.area.height());
        let rect = self.pane_grid_rect_for(width, height, self.pane_cap(p.full_width.get()));
        view.set_margin_start(px(rect.x));
        view.set_margin_top(px(rect.y));
        view.set_margin_end(px(width - (rect.x + rect.w)));
        view.set_margin_bottom(px(height - (rect.y + rect.h)));
    }

    /// Puts `html` on screen in pane `pane_id`'s web view: IN PLACE by swapping the
    /// body where that is sound, and by a full `load_html` where it is not.
    ///
    /// ## The scroll measurement — do NOT simplify this back to `load_html`
    ///
    /// A full `load_html` DESTROYS the scroll offset. Measured under `cage` on a
    /// 1,114,719-byte document from the real renderer: `window.scrollY` read **0 in 24
    /// of 24** samples having been 2000 immediately before, while the JSON body swap
    /// preserved it in 24 of 24 — at 5.3 ms against 55.7 ms, shipping ~102 KB per
    /// keystroke instead of 1.11 MB, and re-decoding no inlined base64 font at all.
    /// The symptom of "simplifying" this is the reader thrown back to the top of the
    /// document on every keystroke, which is exactly what a live preview is not.
    ///
    /// ## The two guards, and why the head alone is not enough
    ///
    /// `crates/wolf-core/src/nvim.rs` documents the rendered `<head>` as
    /// BYTE-IDENTICAL on every call — theme and width mode are fixed, so only the body
    /// changes — and documents this same guard as the macOS front-end's. A body swap
    /// leaves the committed `<head>` in place, so a head that MOVED would mean stale
    /// CSS. A first load into a view, an absent `</head>`, an offset differing from
    /// the recorded one, a body that cannot be carved out, or a JSON encoding that
    /// fails all fall back to `load_html` and re-record the offset.
    ///
    /// That byte-identity is also precisely why the head offset cannot see a SOURCE
    /// SWITCH: two different files render to the same head at the same offset. So the
    /// caller's [`preview_ui::document_update`] is a SEPARATE conjunct ahead of it,
    /// and it is what forces a full load when the pane's source buffer changed or a
    /// cross-file fragment is waiting. Without it, following a link would swap the
    /// linked file's body into a document whose base URI still names the previous
    /// file's directory, and the waiting fragment would have no `LoadEvent::Finished`
    /// edge to fire on.
    ///
    /// ## The base URI
    ///
    /// A full load hands `load_html` [`preview_link::file_uri_for`]`(source_path)`,
    /// which names the SOURCE FILE rather than its directory — so a relative
    /// `beta.md` in the document resolves by replacing the last segment. `None` for a
    /// nameless buffer, which is passed straight through, and what a click in THAT
    /// document delivers is half documented and half measured.
    /// `webkit_web_view_load_html` says a `NULL` base "defaults to `about:blank`", so
    /// a fragment-only href is EXPECTED to arrive as `about:blank#frag` — unmeasured
    /// here, and the reason [`preview_link::classify`]'s first two rows exist. A
    /// RELATIVE href was MEASURED to arrive RAW (`uri="lnkinto.md"`), which is that
    /// table's row 9 rather than anything `about:`. Neither shape is reachable from
    /// the running application, because [`preview_link::file_uri_for`] answers `Some`
    /// for every real source path; its own docs carry the two inputs that answer
    /// `None` and why neither occurs. One observable consequence of having a file
    /// origin at all is that a relative `![](img.png)` can resolve from disk;
    /// everything else the renderer emits is inlined.
    ///
    /// ## The documented "an absolute path kills the web process" hazard does NOT fire
    ///
    /// `webkit_web_view_load_html`'s own documentation — still shipped in the pinned
    /// 2.52.4 GIR — says that when `base_uri` is non-null, "absolute local paths that
    /// are not children of @base_uri will cause the web process to terminate". Core
    /// passes `Tag::Image` and raw HTML blocks through verbatim, so an ordinary user
    /// document arrives here carrying whatever absolute path its author wrote. Were
    /// that sentence true on this port, a base URI would be WORSE than none: merely
    /// previewing a file would kill the pane.
    ///
    /// MEASURED under `cage` against webkitgtk 2.52.4 on 2026-08-06, one run per
    /// document, each read from a temporarily connected `web-process-terminated`
    /// handler: an `<img>` naming a real file by absolute path OUTSIDE the base's
    /// directory (it rendered), an absolute path that does not exist,
    /// `<img src="/etc/hosts">`, `/etc/passwd`, `file:///etc/shadow`, a `../`
    /// traversal, an out-of-base `<link rel=stylesheet>` and `<script src>` (both
    /// applied), an out-of-base `<iframe>`, and the relative and in-base controls.
    ///
    /// **The signal fired in none of the eleven.** Every run reached
    /// `LoadEvent::Finished` with the preview still bound and the app alive. The
    /// handler's own falsifier was run rather than assumed: `SIGKILL` to the
    /// WebProcess produced `reason=Crashed` on the same build, so the silence is
    /// evidence and not an unwired signal.
    ///
    /// Pixels are NOT a reading of this and a re-measurement must not use them: after
    /// that deliberate kill the capture was byte-identical to the healthy one, because
    /// the last committed frame stays on screen. Connect the signal.
    ///
    /// ## The one behavioural asymmetry between the two paths
    ///
    /// A `<script>` element created by `DOMParser` is flagged "already started" and
    /// does NOT run when it is adopted into the live document, whereas `load_html`
    /// runs it. pulldown-cmark passes raw HTML straight through, so a Markdown source
    /// containing a literal `<script>` behaves differently on the two paths. The
    /// in-place behaviour is the safer of the two, and making them uniform is not
    /// worth trading the measurement above for.
    fn load_preview_document(
        &self,
        pane_id: PaneId,
        html: &str,
        source_path: &str,
        update: DocumentUpdate,
    ) {
        let Some((view, recorded)) = ({
            let panes = self.panes.borrow();
            panes
                .get(&pane_id)
                .and_then(|p| p.web.borrow().clone().map(|v| (v, p.head_len.get())))
        }) else {
            return;
        };

        let head = head_offset(html);
        // The whole in-place branch, or nothing: the caller's strategy must allow it
        // AND the head must be present AND unmoved AND a body must be carveable out
        // of the document.
        let swap = match (head, body_inner(html)) {
            (Some(offset), Some(body))
                if update == DocumentUpdate::InPlaceEligible
                    && recorded != 0
                    && offset == recorded =>
            {
                Some(body)
            }
            _ => None,
        };

        if let Some(body) = swap {
            match preview_swap_payload(body) {
                Ok(literal) => {
                    let script = preview_swap_script(&literal);
                    view.evaluate_javascript(
                        &script,
                        None,
                        None,
                        gio::Cancellable::NONE,
                        // A failed swap leaves the PREVIOUS document on screen and
                        // prints; it is not re-armed, because the next edit bumps the
                        // tick and reloads anyway.
                        |result| {
                            if let Err(err) = result {
                                eprintln!("wolf-gnome: preview body swap failed: {err}");
                            }
                        },
                    );
                    return;
                }
                Err(err) => {
                    eprintln!(
                        "wolf-gnome: preview body could not be encoded ({err}); \
                         falling back to a full load"
                    );
                }
            }
        }

        view.load_html(html, preview_link::file_uri_for(source_path).as_deref());
        if let Some(p) = self.panes.borrow().get(&pane_id) {
            p.head_len.set(head.unwrap_or(0));
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
    /// **`WolfCore::pane_status` must never be called from here**, and neither may
    /// `display_path` or `render_preview` — the same rule, for the same reason, over
    /// the three calls a preview pane adds. This function
    /// holds a `SnapshotReadGuard` for its whole body and `pane_status` takes
    /// `read_snapshot()` again internally; a re-entrant read can deadlock behind a
    /// queued writer (`shell.rs`'s borrow-discipline section). The strings come out
    /// of the per-pane cache [`Self::refresh_pane_state`] fills on the wake.
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
        // band, and the only colour channel this front-end has (it takes no colour
        // from the GTK theme).
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
        // out rather than held, and no core call happens while it is live. The
        // full-width flag comes out of the same borrow, so the rect this frame is
        // painted into and the rect its clicks are translated through are one, and
        // so does the preview predicate, so the strip and the grid cannot disagree
        // about what kind of pane this is inside one frame.
        let (grid_id, status, full_width, is_preview) = {
            let panes = self.panes.borrow();
            panes.get(&pane_id).map_or_else(
                || (None, wolf_core::pane_status("", None, None), false, false),
                |p| {
                    (
                        p.grid.get(),
                        p.status.borrow().clone(),
                        p.full_width.get(),
                        p.preview.borrow().is_some(),
                    )
                },
            )
        };
        let rect = self.pane_grid_rect_for(width, height, self.pane_cap(full_width));
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

        // A preview pane skips EXACTLY the grid block and the scrollbar thumb, and
        // nothing else: the whole-allocation background fill above and the entire
        // statusbar strip are painted for it exactly as for an editor pane, so the
        // margins around the web view are the editor's background rather than
        // garbage and the preview's filename and path still read. The cursor goes
        // with the grid — under an opaque web view it would paint invisibly — and
        // the thumb goes with it because the web view scrolls itself and a synthetic
        // thumb would track the scratch buffer instead. The skip needs no
        // containment test: the preview scratch buffer has no lines, so its grid
        // would paint background and nothing else anyway.
        if !is_preview {
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

            // Deliberately OUTSIDE the grid clip: the thumb lives in the pane's right
            // padding gutter, which the clip above excludes. `Snapshot::windows` is
            // keyed by GRID id, so an unbound pane — which falls back to the active
            // grid for its paint — correctly gets no thumb, because no viewport
            // belongs to it.
            let viewport = grid_id
                .and_then(|g| snap.windows.get(&g))
                .map(|w| w.viewport);
            if let Some(thumb) = viewport.and_then(|v| scrollbar::scrollbar_thumb(width, rect, v)) {
                let c = scrollbar::scrollbar_thumb_color(bg);
                cr.set_source_rgb(
                    f64::from(c.r) / RGB_MAX,
                    f64::from(c.g) / RGB_MAX,
                    f64::from(c.b) / RGB_MAX,
                );
                cr.rectangle(thumb.x, thumb.y, thumb.w, thumb.h);
                cr.fill().expect("cairo scrollbar thumb fill failed");
            }
        }
    }

    /// Handles a pane area resize: resize the pane's own grid, re-frame a preview's
    /// web view, then resize the global grid.
    fn on_pane_resize(&self, pane_id: PaneId, width: i32, height: i32) {
        if let Some((grid, cols, rows)) = self.pending_resize(pane_id, width, height) {
            if let Some(core) = self.core.borrow().as_ref() {
                core.resize_grid(grid, cols, rows);
            }
        }
        // Re-framed HERE as well as on the wake, because the frame is margins in
        // ABSOLUTE PIXELS: a divider drag that moves the allocation without changing
        // the cell-rounded grid size issues no `resize_grid`, so no flush and no wake
        // follow it, and the view would stay framed for the old allocation until some
        // unrelated redraw arrived. A pane with no web view returns immediately.
        self.apply_preview_frame(pane_id);
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
            (
                p.grid.get()?,
                self.pane_grid_rect(&p.area, self.pane_cap(p.full_width.get())),
            )
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
                // OWN area, whatever grid is pinned, so the rect — and the cap it
                // is measured under — are this pane's.
                self.pane_grid_rect(&p.area, self.pane_cap(p.full_width.get())),
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

    /// Paints the native command-line strip.
    ///
    /// Under `ext_messages` (which implicitly enables `ext_cmdline` and sets
    /// `cmdheight=0`) the outer grid 1 empties, so this paints from the folded
    /// [`Snapshot::cmdline`] rather than grid 1's last row: the dialog `prompt`,
    /// then `firstc`, then the flattened cmdline text, with a caret at the byte
    /// position `pos`.
    ///
    /// **The `prompt` is drawn, and that is what makes a `confirm()` dialog
    /// answerable.** A confirm's question is a `confirm`-kind message and its options
    /// line arrives as the cmdline's `prompt`; dropping the prompt leaves the user
    /// pressing keys at a strip that shows nothing.
    ///
    /// Messages are NOT drawn here. They are their own surface ([`draw_toast`]), so a
    /// prompt's question and its options line are on screen at the same time rather
    /// than taking turns in one row.
    ///
    /// [`draw_toast`]: PaneInner::draw_toast
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

        if !snap.cmdline.active {
            return;
        }
        let prefix = format!("{}{}", snap.cmdline.prompt, snap.cmdline.firstc);
        let text = snap.cmdline.text();
        let line = format!("{prefix}{text}");
        if !line.is_empty() {
            draw_glyph(cr, metrics, 0.0, 0.0, &line, fg, &style);
        }
        let caret_cols = cmdline_ui::cmdline_columns(&prefix, &text, snap.cmdline.pos);
        let caret_x = caret_cols as f64 * metrics.cell_w;
        cr.set_source_rgb(
            f64::from(fg.r) / RGB_MAX,
            f64::from(fg.g) / RGB_MAX,
            f64::from(fg.b) / RGB_MAX,
        );
        cr.rectangle(caret_x, 0.0, CMDLINE_CARET_WIDTH, metrics.cell_h);
        cr.fill().expect("cairo cmdline caret fill failed");
    }

    /// The line count, budget and view the toast surface would draw this frame.
    ///
    /// One function so [`refresh_message_surfaces`] and [`draw_toast`] cannot
    /// disagree about which lines are on screen: the first sizes the widget and the
    /// second fills it, and a second copy of the arithmetic is how a surface ends up
    /// one line taller than its content.
    ///
    /// [`refresh_message_surfaces`]: PaneInner::refresh_message_surfaces
    /// [`draw_toast`]: PaneInner::draw_toast
    fn toast_layout(&self, snap: &Snapshot) -> (Vec<Vec<ToastChunk>>, ToastView) {
        let lines = snap.toast.lines();
        let available = f64::from(self.pane_area.height().max(0));
        let budget = cmdline_ui::toast_budget_rows(available, self.metrics.cell_h);
        let view = cmdline_ui::toast_view(lines.len(), budget, self.toast_scroll.get());
        (lines, view)
    }

    /// Re-measures the toast against the CURRENT scroll offset and re-places the
    /// surface, without touching the per-wake bookkeeping in
    /// [`refresh_message_surfaces`].
    ///
    /// Two callers with two reasons: a render wake, where the message itself may have
    /// changed; and a scroll gesture, which changes only which lines are visible — and
    /// [`place_toast`] sizes the box from the widest VISIBLE line, so the size is a
    /// function of the offset either way.
    ///
    /// [`refresh_message_surfaces`]: PaneInner::refresh_message_surfaces
    /// [`place_toast`]: PaneInner::place_toast
    fn reflow_toast(&self, snap: &Snapshot) {
        let (lines, view) = self.toast_layout(snap);
        self.place_toast(snap.toast.visible, &lines, view);
    }

    /// Sizes, positions and shows or hides the two overlay surfaces for this frame.
    ///
    /// Both are sized to their own content rather than filling the overlay, which is
    /// what keeps the toast's pointer target confined to the box it draws — an
    /// overlay child spanning the window would swallow every click meant for the
    /// buffer underneath it.
    fn refresh_message_surfaces(&self) {
        let core_ref = self.core.borrow();
        let Some(core) = core_ref.as_ref() else {
            return;
        };
        let snap = core.read_snapshot();

        // A prompt that stops being shown takes its scroll offset with it: core
        // stashes and hides a confirm toast on EVERY hide, restoring it only for a
        // matching re-prompt, so the next frame carries either a different message
        // or a re-prompt — and neither may open part-scrolled into lines the reader
        // has not seen. Nothing here dismisses a prompt on its own.
        if cmdline_ui::prompt_answered(self.toast_was_prompt.get(), snap.toast.visible) {
            self.toast_scroll.set(0);
            self.toast_lines.set(0);
        }
        self.toast_was_prompt
            .set(snap.toast.visible && snap.toast.is_prompt());

        let lines = snap.toast.lines();
        let available = f64::from(self.pane_area.height().max(0));
        let budget = cmdline_ui::toast_budget_rows(available, self.metrics.cell_h);
        let scroll = cmdline_ui::carried_scroll(
            self.toast_lines.get(),
            lines.len(),
            budget,
            self.toast_scroll.get(),
        );
        self.toast_scroll.set(scroll);
        self.toast_lines.set(lines.len());

        self.reflow_toast(&snap);
        self.place_pum(&snap);
    }

    /// Sizes and places the toast surface, or hides it.
    fn place_toast(&self, visible: bool, lines: &[Vec<ToastChunk>], view: ToastView) {
        if !cmdline_ui::toast_is_shown(visible, lines.len()) {
            self.toast_area.set_visible(false);
            return;
        }
        let metrics = &self.metrics;
        let cols = cmdline_ui::toast_width_cols(lines, view);
        let container_w = f64::from(self.container.width().max(0));
        let w = (cols as f64).mul_add(metrics.cell_w, 2.0 * cmdline_ui::SURFACE_PADDING_PX);
        let w = if container_w > 0.0 {
            w.min(container_w)
        } else {
            w
        };
        let h = (view.len as f64).mul_add(metrics.cell_h, 2.0 * cmdline_ui::SURFACE_PADDING_PX);
        self.toast_area.set_content_width(px(w));
        self.toast_area.set_content_height(px(h));
        self.toast_area
            .set_margin_bottom(strip_height(metrics.cell_h));
        self.toast_area.set_visible(true);
    }

    /// The selection, the visible slice and the width in cells the completion popup
    /// would draw this frame.
    ///
    /// One function so [`place_pum`] and [`draw_pum`] cannot disagree about which items
    /// are on screen or how wide the surface is: the first sizes the widget and the
    /// second fills it, and a second copy of the arithmetic is how a popup ends up a
    /// column narrower than the label it truncates. The budget is the toast's, capped at
    /// [`cmdline_ui::PUM_MAX_ROWS`], because both surfaces share the space above the
    /// strip.
    ///
    /// [`place_pum`]: PaneInner::place_pum
    /// [`draw_pum`]: PaneInner::draw_pum
    fn pum_layout(&self, snap: &Snapshot) -> (Option<usize>, PumView, usize) {
        let selected = cmdline_ui::pum_selection(snap.pum.items.len(), snap.pum.selected);
        let available = f64::from(self.pane_area.height().max(0));
        let budget = cmdline_ui::toast_budget_rows(available, self.metrics.cell_h)
            .min(cmdline_ui::PUM_MAX_ROWS);
        let view = cmdline_ui::pum_view(snap.pum.items.len(), selected, budget);
        let cols = cmdline_ui::pum_width_cols(&snap.pum.items, view);
        (selected, view, cols)
    }

    /// Sizes and anchors the completion popup, or hides it.
    ///
    /// Two anchors, both reusing arithmetic that already exists.
    /// `Pum::grid == -1` anchors to the command line, whose `col` is a BYTE offset
    /// into the cmdline text — [`cmdline_ui::cmdline_columns`] converts it, the same
    /// call the caret makes. Any other grid anchors to that pane's cell, which is
    /// [`editor_inset::grid_rect`]'s rectangle (the centring offset, in the one place
    /// it lives) offset by the pane's own position inside the container.
    fn place_pum(&self, snap: &Snapshot) {
        if !snap.pum.visible || snap.pum.items.is_empty() {
            self.pum_area.set_visible(false);
            return;
        }
        let metrics = &self.metrics;
        let (_, view, cols) = self.pum_layout(snap);
        let w = (cols as f64).mul_add(metrics.cell_w, 2.0 * cmdline_ui::SURFACE_PADDING_PX);
        let h = (view.len as f64).mul_add(metrics.cell_h, 2.0 * cmdline_ui::SURFACE_PADDING_PX);

        let container_w = f64::from(self.container.width().max(0));
        let anchor = self.pum_anchor(snap, metrics);
        let x = match container_w {
            cw if cw > 0.0 => anchor.0.clamp(0.0, (cw - w).max(0.0)),
            _ => anchor.0.max(0.0),
        };

        self.pum_area.set_content_width(px(w));
        self.pum_area.set_content_height(px(h));
        self.pum_area.set_margin_start(px(x));
        match anchor.1 {
            // Anchored to the command line: the popup sits directly above the strip.
            None => {
                self.pum_area.set_valign(gtk::Align::End);
                self.pum_area.set_margin_top(0);
                self.pum_area
                    .set_margin_bottom(strip_height(metrics.cell_h));
            }
            // Anchored to a grid cell: `row` is the first row BELOW the completed
            // word, so the popup's top edge is that row's top edge.
            Some(y) => {
                self.pum_area.set_valign(gtk::Align::Start);
                self.pum_area.set_margin_bottom(0);
                self.pum_area.set_margin_top(px(y.max(0.0)));
            }
        }
        self.pum_area.set_visible(true);
    }

    /// The popup's container-local `(x, Some(y))`, or `(x, None)` when it is anchored
    /// to the command line and belongs directly above the strip.
    fn pum_anchor(&self, snap: &Snapshot, metrics: &CellMetrics) -> (f64, Option<f64>) {
        if snap.pum.grid < 0 {
            let prefix = format!("{}{}", snap.cmdline.prompt, snap.cmdline.firstc);
            let byte = usize::try_from(snap.pum.col).unwrap_or(0);
            let cols = cmdline_ui::cmdline_columns(&prefix, &snap.cmdline.text(), byte);
            return (cols as f64 * metrics.cell_w, None);
        }
        let grid = u64::try_from(snap.pum.grid).unwrap_or(0);
        let panes = self.panes.borrow();
        let Some(pane) = panes.values().find(|p| p.grid.get() == Some(grid)) else {
            return (0.0, None);
        };
        let Some(bounds) = pane.area.compute_bounds(&self.container) else {
            return (0.0, None);
        };
        let rect = self.pane_grid_rect(&pane.area, self.pane_cap(pane.full_width.get()));
        let col = snap.pum.col.max(0) as f64;
        let row = snap.pum.row.max(0) as f64;
        (
            f64::from(bounds.x()) + rect.x + col * metrics.cell_w,
            Some(f64::from(bounds.y()) + rect.y + row * metrics.cell_h),
        )
    }

    /// Paints the message toast: an opaque band in the editor's own background with a
    /// low-contrast edge, carrying the visible slice of the message lines with each
    /// chunk in its own highlight.
    fn draw_toast(&self, cr: &cairo::Context, width: f64, height: f64) {
        let core_ref = self.core.borrow();
        let Some(core) = core_ref.as_ref() else {
            return;
        };
        let snap = core.read_snapshot();
        let metrics = &self.metrics;
        let base = snap.highlights.resolve(0);
        let fg = base.fg.unwrap_or(FALLBACK_FG);
        let bg = base.bg.unwrap_or(FALLBACK_BG);
        fill_surface(cr, width, height, fg, bg);

        let (lines, view) = self.toast_layout(&snap);
        let pad = cmdline_ui::SURFACE_PADDING_PX;
        for (i, line) in lines.iter().skip(view.first).take(view.len).enumerate() {
            let y = (i as f64).mul_add(metrics.cell_h, pad);
            let mut x = pad;
            for chunk in line {
                let style = snap.highlights.resolve(chunk.attr_id);
                let chunk_fg = style.fg.unwrap_or(fg);
                if !chunk.text.is_empty() {
                    draw_glyph(cr, metrics, x, y, &chunk.text, chunk_fg, &style);
                }
                x += chunk.text.chars().count() as f64 * metrics.cell_w;
            }
        }
    }

    /// Paints the completion popup: one row per visible item, the selected row drawn
    /// in reverse video so the selection reads without a theme ACCENT colour, which
    /// this front-end has no way to obtain — it resolves a light/dark preference
    /// ([`mod@crate::appearance`]) and takes no colour from the GTK theme at all.
    fn draw_pum(&self, cr: &cairo::Context, width: f64, height: f64) {
        let core_ref = self.core.borrow();
        let Some(core) = core_ref.as_ref() else {
            return;
        };
        let snap = core.read_snapshot();
        let metrics = &self.metrics;
        let base = snap.highlights.resolve(0);
        let fg = base.fg.unwrap_or(FALLBACK_FG);
        let bg = base.bg.unwrap_or(FALLBACK_BG);
        fill_surface(cr, width, height, fg, bg);

        let (selected, view, cols) = self.pum_layout(&snap);
        let pad = cmdline_ui::SURFACE_PADDING_PX;

        for (i, item) in snap
            .pum
            .items
            .iter()
            .skip(view.first)
            .take(view.len)
            .enumerate()
        {
            let y = (i as f64).mul_add(metrics.cell_h, pad);
            let is_selected = selected == Some(view.first + i);
            let text_fg = if is_selected {
                if width > 0.0 {
                    cr.set_source_rgb(
                        f64::from(fg.r) / RGB_MAX,
                        f64::from(fg.g) / RGB_MAX,
                        f64::from(fg.b) / RGB_MAX,
                    );
                    cr.rectangle(pad, y, width - 2.0 * pad, metrics.cell_h);
                    cr.fill().expect("cairo pum selection fill failed");
                }
                bg
            } else {
                fg
            };
            let label: String = cmdline_ui::pum_item_label(item)
                .chars()
                .take(cols)
                .collect();
            if !label.is_empty() {
                draw_glyph(cr, metrics, pad, y, &label, text_fg, &base);
            }
        }
    }

    /// Applies a scroll gesture on the toast, moving its window back through the older
    /// lines. Returns whether anything moved.
    ///
    /// The delta is routed through the same seam a pane's is: `mouse::scroll_threshold`
    /// picks the per-`gdk::ScrollUnit` threshold and a carried [`ScrollAccumulator`]
    /// turns it into whole line steps, so a `ScrollUnit::Surface` device's many sub-line
    /// deltas park a carry instead of stepping a line per callback, and one flick moves
    /// as many lines as it earned. The accumulator is this surface's own rather than a
    /// [`PaneScroll`]: the toast has no target grid, so the reset-on-grid-change rule
    /// `PaneScroll` exists for would have nothing to compare. The non-finite and
    /// zero-delta cases are `ScrollAccumulator::consume`'s and are not re-stated here.
    ///
    /// The step count becomes an offset through [`cmdline_ui::toast_scroll_next`].
    ///
    /// **The surface is re-placed before the draw is queued.** [`place_toast`] sizes the
    /// box from the widest VISIBLE line, and this gesture is deliberately never
    /// forwarded to core, so no render wake follows it: a `queue_draw` on its own would
    /// repaint longer lines into a box measured for shorter ones and clip them
    /// mid-glyph until some unrelated flush arrived.
    ///
    /// The gesture is CONSUMED by the caller either way, which is the front-end half of
    /// core's toast-dismissal rule: `InputEvent::is_toast_dismissing` counts a scroll as
    /// dismissing, so a scroll forwarded from over the toast would clear the message the
    /// user was scrolling.
    ///
    /// [`place_toast`]: PaneInner::place_toast
    fn scroll_toast(&self, dy: f64, unit: gdk::ScrollUnit) -> bool {
        let threshold = mouse::scroll_threshold(unit, self.metrics.cell_h);
        let mut acc = self.toast_scroll_acc.get();
        let steps = acc.consume(dy, threshold);
        self.toast_scroll_acc.set(acc);
        if steps == 0 {
            return false;
        }
        let lines = self.toast_lines.get();
        let available = f64::from(self.pane_area.height().max(0));
        let budget = cmdline_ui::toast_budget_rows(available, self.metrics.cell_h);
        let next = cmdline_ui::toast_scroll_next(self.toast_scroll.get(), steps, lines, budget);
        if next == self.toast_scroll.get() {
            return false;
        }
        self.toast_scroll.set(next);
        let snap = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return true;
            };
            core.read_snapshot()
        };
        self.reflow_toast(&snap);
        self.toast_area.queue_draw();
        true
    }

    /// Rebuilds `pane_area`'s child widget tree from the current [`PaneTree`],
    /// REUSING each pane's `DrawingArea` by id (so bound grids are preserved).
    ///
    /// `pane_area`'s children end up as `[editor tree (expands)][drawer strip
    /// (fixed height)]`. The unparent loop below needs no drawer case — the drawer
    /// is an ordinary `panes` entry, so its host is detached with every other — but
    /// the RE-parenting does, because [`build_layout_widget`] walks the tree and the
    /// drawer is deliberately not in it.
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

        // Detach every reusable pane HOST from its current parent so it can be
        // re-inserted into the freshly built widget tree. The host, not the area:
        // unparenting the area would tear it out of its own overlay and leave a
        // preview's web view parented to a host that is no longer in the tree.
        {
            let panes = self.panes.borrow();
            for p in panes.values() {
                if p.host.parent().is_some() {
                    p.host.unparent();
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
            // The drawer strip, AFTER the tree: `pane_area` is a vertical box, so
            // appending last is what puts it along the bottom. It is not in the
            // layout `build_layout_widget` walks and never will be, so this append is
            // the one place the widget TREE knows the drawer exists. Its height is a
            // separate matter and `apply_drawer_frame` below owns it.
            if let Some(p) = self.drawer.get().id().and_then(|id| panes.get(&id)) {
                self.pane_area.append(&p.host);
            }
        }
        // The tree widget expands and the strip does not, so the strip's own height
        // request is what divides them. See its "why not a `gtk::Paned`" note.
        self.apply_drawer_frame();

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
        // The drawer is NOT reachable through `panes`, so clearing that map does not
        // clear it — and nothing else can. `detect_closes` is the drawer's only other
        // unbind path and it cannot fire here: a restart leaves the dead Neovim's
        // grid and its `WinInfo` in the snapshot, so the drawer's grid never appears
        // to vanish. A surviving id would keep `apply_drawer_open`'s `is_open()`
        // guard true against a pane of a process that no longer exists, and the
        // drawer could never be opened again for the rest of the session.
        self.clear_drawer();
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

    /// The message toast surface, for placement in the app's editor overlay.
    ///
    /// An overlay child rather than a taller strip, because the strip's height is
    /// subtracted from the pane area: a strip that grew with its content would issue
    /// a `resize_grid` on every message.
    #[must_use]
    pub fn toast_widget(&self) -> gtk::DrawingArea {
        self.inner.toast_area.clone()
    }

    /// The completion popup surface, for placement in the app's editor overlay.
    #[must_use]
    pub fn pum_widget(&self) -> gtk::DrawingArea {
        self.inner.pum_area.clone()
    }

    /// Reacts to a render wake (see [`PaneInner::on_render_wake`]).
    pub fn on_render_wake(&self) {
        self.inner.on_render_wake();
    }

    /// Queues a saved pane tree to be realized ONCE, on the first wake at which a
    /// window grid exists.
    ///
    /// Called from `window::build` before `present()`, with the layout carried on
    /// that window's `state_seam::RestorePlan`. It cannot be applied here: no Neovim
    /// window has appeared yet, so there is nothing to rebuild from. See
    /// [`PaneInner::consume_pending_layout`] for the three outcomes.
    pub fn request_layout_rebuild(&self, saved: PaneNode) {
        *self.inner.pending_layout.borrow_mut() = Some(saved);
    }

    /// Realizes a saved pane tree IMMEDIATELY, with no pending step and no
    /// one-pane precondition.
    ///
    /// **The tabpage this rebuilds into is whichever one Neovim is current on right
    /// now.** `WolfCore::rebuild_layout` acts on the current tabpage, so the only
    /// legal caller is the restore materialization
    /// ([`crate::tab_view::TabsHandle::queue_restore`]), immediately after that
    /// tab's `create_tabpage` returned — the one instant at which this front-end
    /// knows for certain which tabpage that is. Calling it from anywhere else
    /// rebuilds some other tab's windows.
    ///
    /// [`PaneHandle::request_layout_rebuild`] is the OTHER route and the one tab 0
    /// takes: it defers to the first wake at which a window grid exists, which is
    /// what a tab that has no panes yet needs.
    ///
    /// This is a BLOCKING RPC; see [`PaneInner::apply_layout_rebuild`] for the
    /// budget it belongs to. Exactly ONE per restored tab, which is why the queued
    /// copy is dropped below.
    pub fn realize_layout(&self, saved: &PaneNode) {
        // `open_tab` stays single-pathed and queues the same layout for every tab
        // it builds. Dropping that copy here is what keeps this ONE rebuild: left
        // queued, the first wake this tab is not held for would consume it and
        // rebuild a second time — a blocking RPC on the SELECTION path, which is
        // neither the launch path nor anywhere in the blocking-call inventory.
        self.inner.pending_layout.borrow_mut().take();
        self.inner.apply_layout_rebuild(saved);
    }

    /// The tab's gather answer for `state.json` (see
    /// [`PaneInner::layout_snapshot`]): `Refuse` means "contribute the cache", and
    /// `Ready(None)` is the content suppression of an unnamed lone pane.
    #[must_use]
    pub fn layout_snapshot(&self) -> pane_layout::LayoutGather {
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

    // Both surfaces are sized to their own content on each wake and start hidden, so
    // neither covers the buffer or intercepts a pointer event while there is nothing
    // to show. `halign`/`valign` keep them at their measured size inside the overlay;
    // filling it would make the toast's pointer target the whole window.
    let toast_area = gtk::DrawingArea::builder()
        .halign(gtk::Align::Start)
        .valign(gtk::Align::End)
        .visible(false)
        .build();
    let pum_area = gtk::DrawingArea::builder()
        .halign(gtk::Align::Start)
        .valign(gtk::Align::End)
        .visible(false)
        .can_target(false)
        .build();

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
        toast_area: toast_area.clone(),
        pum_area: pum_area.clone(),
        toast_scroll: Cell::new(0),
        toast_scroll_acc: Cell::new(ScrollAccumulator::new()),
        toast_lines: Cell::new(0),
        toast_was_prompt: Cell::new(false),
        statusbar,
        statusbar_primary: strip_font(&metrics.font, primary_px, false),
        statusbar_primary_bold: strip_font(&metrics.font, primary_px, true),
        statusbar_secondary: strip_font(&metrics.font, secondary_px, false),
        metrics: metrics.clone(),
        core: core.clone(),
        active_pane: Cell::new(None),
        drawer: Cell::new(DrawerState::new()),
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
    {
        let weak = Rc::downgrade(&inner);
        toast_area.set_draw_func(move |_area, cr, w, h| {
            if let Some(inner) = weak.upgrade() {
                inner.draw_toast(cr, f64::from(w), f64::from(h));
            }
        });
    }
    {
        let weak = Rc::downgrade(&inner);
        pum_area.set_draw_func(move |_area, cr, w, h| {
            if let Some(inner) = weak.upgrade() {
                inner.draw_pum(cr, f64::from(w), f64::from(h));
            }
        });
    }
    {
        // The toast's own scroll controller. It always stops propagation, whether or
        // not the offset moved: the point is that a gesture landing on the toast
        // never reaches a pane's controller and so never reaches core, where
        // `InputEvent::is_toast_dismissing` would clear the message being read.
        let weak = Rc::downgrade(&inner);
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll(move |controller, _dx, dy| {
            if let Some(inner) = weak.upgrade() {
                inner.scroll_toast(dy, controller.unit());
            }
            glib::Propagation::Stop
        });
        toast_area.add_controller(scroll);
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
        // The HOST, never the bare `area`: the web view is the host's overlay child,
        // so parenting the host is what carries a live preview through a rebuild.
        PaneLayout::Leaf(id) => ctx
            .panes
            .get(id)
            .map(|p| p.host.clone().upcast::<gtk::Widget>()),
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

/// The byte offset of `</head>` in a rendered preview document, or `None` when the
/// document has no head at all (an empty string, which is what
/// `WolfCore::render_preview` answers on an RPC failure, is the common case).
///
/// `0` is never a real answer — the render always opens with a
/// `<!doctype html><html><head>` prologue — which is what lets
/// [`PaneWidgets::head_len`] use it as its "nothing loaded" sentinel.
fn head_offset(html: &str) -> Option<usize> {
    html.find("</head>")
}

/// The INNER html of a rendered preview document's `<body>`, or `None` when the
/// document does not have the renderer's fixed wrapper shape.
///
/// FIRST-open, LAST-close, and the asymmetry is deliberate: raw HTML passes straight
/// through pulldown-cmark, so a Markdown source can put a literal `</body>` into the
/// rendered body — but the wrapper's own closing tag is always the LAST one, and any
/// `<body` in content is always AFTER the wrapper's opening tag.
///
/// A SCAN, not a parse. The body is returned byte for byte, never sanitized or
/// re-encoded: the `.wolf-src` position markers are looked up by id and the inline
/// `style="color:#…"` highlight spans are what a re-render actually changes.
fn body_inner(html: &str) -> Option<&str> {
    let open = html.find("<body")?;
    let start = open + html[open..].find('>')? + 1;
    let end = html.rfind("</body>")?;
    if end < start {
        return None;
    }
    Some(&html[start..end])
}

/// `body` as a JavaScript string literal whose VALUE is `body`'s JSON text, ready to
/// embed in [`preview_swap_script`]'s `JSON.parse(…)`.
///
/// **Encoded TWICE, and both halves are load-bearing.** The inner
/// `serde_json::to_string` turns the html into a JSON text; the outer one turns that
/// text into a JS string literal. The script's `JSON.parse` undoes exactly the inner
/// encoding, so what reaches the DOM is `body` byte for byte.
///
/// Dropping either half is a runtime failure, not a style change, and it was
/// MEASURED: with one encoding, the literal's value is the html itself, and
/// `JSON.parse` on html throws `SyntaxError: JSON Parse error: Unrecognized token '<'`
/// on the very first swap — the preview then silently keeps showing the previous
/// document while the user types.
///
/// What the encoding buys is that an arbitrary document is safe to embed: `"` and
/// `\` and every control character (a newline becomes `\n`) are escaped, so an
/// apostrophe, a quote, a backslash or a literal `</script>` anywhere in the user's
/// prose is inert — this is JS SOURCE handed to `evaluate_javascript`, never HTML, so
/// `</script>` has nothing to close. **Never build such a script by concatenation.**
///
/// The two replacements close JSON's one gap as a JS literal: U+2028 and U+2029 are
/// legal raw inside a JSON string but are LINE TERMINATORS to a JavaScript grammar
/// before ES2019, where they would end the literal mid-string. Escaping them keeps
/// the value identical and removes the dependency on the engine's conformance —
/// applied to the OUTER text, since that is the one the JS parser reads.
fn preview_swap_payload(body: &str) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(body)?;
    Ok(serde_json::to_string(&json)?
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}

/// The body-swap script, over an already-encoded `literal` carrying the new body's
/// inner html (see [`preview_swap_payload`]).
///
/// The children are moved into a `DocumentFragment` by a `while (doc.body.firstChild)`
/// loop and committed with ONE `replaceChildren(frag)` — deliberately **not**
/// `replaceChildren.apply(body, Array.from(...))`, which spreads every top-level node
/// as an argument: a long README has thousands of them, and
/// `Function.prototype.apply` can throw `RangeError` on exactly the documents where
/// the swap matters most.
fn preview_swap_script(literal: &str) -> String {
    format!(
        "(function () {{ \
           var doc = new DOMParser().parseFromString(JSON.parse({literal}), \"text/html\"); \
           if (!doc.body || !document.body) {{ return; }} \
           var frag = document.createDocumentFragment(); \
           while (doc.body.firstChild) {{ frag.appendChild(doc.body.firstChild); }} \
           document.body.replaceChildren(frag); \
         }})();"
    )
}

/// Hands `uri` to the platform's URI launcher — the desktop portal on a session
/// that has one, `xdg-open`'s equivalent otherwise — and PRINTS on failure.
///
/// Shared by the two places that ask the desktop to open something: this module's
/// [`PaneInner::on_preview_link`], for a foreign-scheme link clicked inside a
/// preview ([`preview_link::PreviewLink::OpenExternally`]), and
/// `shell::AppShell::preview_in_browser`, for core's loopback address. One function
/// rather than two because the failure diagnostic is the whole observable behaviour
/// on a host with no browser, and two copies would drift into two different
/// diagnostics for the same failure. It lives here rather than in a module of its
/// own because `shell` already depends on `pane_view`, so the direction stays
/// acyclic.
///
/// `what` names the caller in the printed line, which is what tells the two apart
/// in a captured stderr log. The launch is ASYNCHRONOUS: `launch` returns as soon as
/// the request is made, and the callback carries the portal's answer, so nothing on
/// this path blocks the GTK main thread.
///
/// `parent` is the toplevel the portal parents its chooser on; `None` is legal and
/// only costs the dialog its parent window.
pub(crate) fn launch_uri(parent: Option<&gtk::Window>, uri: &str, what: &str) {
    let launcher = gtk::UriLauncher::new(uri);
    // Owned before the closure, because the callback outlives this frame.
    let uri = uri.to_string();
    let what = what.to_string();
    launcher.launch(parent, gio::Cancellable::NONE, move |result| {
        if let Err(err) = result {
            eprintln!("wolf-gnome: {what} launch failed for {uri}: {err}");
        }
    });
}

/// Rounds a device-pixel extent up to the `i32` GTK sizing calls take.
fn px(v: f64) -> i32 {
    let v = if v.is_finite() { v } else { 0.0 };
    let v = v.ceil().clamp(0.0, f64::from(i32::MAX));
    #[allow(
        clippy::cast_possible_truncation,
        reason = "v is clamped to [0, i32::MAX] and ceil'd, so the cast is exact and in range"
    )]
    {
        v as i32
    }
}

/// Fills an overlay surface: the editor's own background, then a one-pixel edge.
///
/// The background is a per-frame snapshot value, which is the reason both surfaces
/// are painted rather than being widgets — a theme-owned widget background cannot be
/// the colour Neovim just reported. The edge is a blend rather than a second theme
/// colour, so a surface stays legible against a buffer of the same background
/// without introducing a colour from outside the snapshot.
fn fill_surface(cr: &cairo::Context, width: f64, height: f64, fg: Rgb, bg: Rgb) {
    cr.set_source_rgb(
        f64::from(bg.r) / RGB_MAX,
        f64::from(bg.g) / RGB_MAX,
        f64::from(bg.b) / RGB_MAX,
    );
    cr.rectangle(0.0, 0.0, width, height);
    cr.fill().expect("cairo surface background fill failed");

    let edge = statusbar_layout::blend(fg, bg, SURFACE_EDGE_ALPHA);
    cr.set_source_rgb(
        f64::from(edge.r) / RGB_MAX,
        f64::from(edge.g) / RGB_MAX,
        f64::from(edge.b) / RGB_MAX,
    );
    cr.set_line_width(SURFACE_EDGE_PX);
    cr.rectangle(
        SURFACE_EDGE_PX / 2.0,
        SURFACE_EDGE_PX / 2.0,
        (width - SURFACE_EDGE_PX).max(0.0),
        (height - SURFACE_EDGE_PX).max(0.0),
    );
    cr.stroke().expect("cairo surface edge stroke failed");
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

/// Adds a display-global provider carrying the [`ACTIVE_PANE_CSS`] definition.
///
/// [`build`] calls this, and a pane view is built per TAB, so every tab of every
/// window adds another identical provider to the same default display. Identical
/// providers are inert — the rule resolves the same either way — and the call is
/// deliberately NOT hoisted into [`crate::coordinator`]: no smoke observes this
/// outline (the OCR smokes read text, not chrome), so the hoist would be an
/// unverifiable edit to the startup path.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// One [`preview_swap_payload`] row: a body, and the reason it is here.
    struct PayloadCase {
        desc: &'static str,
        body: &'static str,
    }

    /// How many named rows the payload table carries.
    ///
    /// It is the ARRAY'S LENGTH in the type below, not something a runtime assertion
    /// compares against: dropping a row is then a compile error rather than a check
    /// that could itself be edited away. The rows are not a sample — each names one
    /// character class that can end a JS string literal early — so losing one silently
    /// is exactly the failure to prevent.
    const NAMED_PAYLOAD_ROWS: usize = 9;

    /// Every character class that can end a JavaScript string literal early, plus the
    /// two that end it early only in JS and not in JSON.
    ///
    /// The bodies are what a Markdown document really produces: an apostrophe in
    /// prose, a quoted phrase, a Windows path in a fenced block, a paragraph break,
    /// and — because pulldown-cmark passes raw HTML straight through — a literal
    /// `</script>`.
    const PAYLOAD_CASES: [PayloadCase; NAMED_PAYLOAD_ROWS] = [
        PayloadCase {
            desc: "empty: the render answers this on an RPC failure",
            body: "",
        },
        PayloadCase {
            desc: "ordinary html, no escaping needed at all",
            body: "<h1>plain</h1>",
        },
        PayloadCase {
            desc: "an APOSTROPHE in prose",
            body: "<p>don't</p>",
        },
        PayloadCase {
            desc: "a DOUBLE QUOTE, which is the literal's own delimiter",
            body: "<p>she said \"go\"</p>",
        },
        PayloadCase {
            desc: "a BACKSLASH, which is the literal's own escape character",
            body: "<pre><code>C:\\Users\\wolf</code></pre>",
        },
        PayloadCase {
            desc: "a NEWLINE: a raw one ends a JS string literal",
            body: "<p>one</p>\n<p>two</p>",
        },
        PayloadCase {
            desc: "a literal </script>, which pulldown-cmark passes straight through",
            body: "<p>text</p><script>alert(1)</script>",
        },
        PayloadCase {
            desc: "U+2028 LINE SEPARATOR: legal raw in JSON, a line terminator in JS",
            body: "<p>before\u{2028}after</p>",
        },
        PayloadCase {
            desc: "U+2029 PARAGRAPH SEPARATOR: U+2028's twin, and just as fatal",
            body: "<p>before\u{2029}after</p>",
        },
    ];

    /// [`preview_swap_payload`] produces a well-formed JS string literal whose
    /// DOUBLE decode is the body, byte for byte.
    ///
    /// ## What "well-formed JS string literal" means here, executably
    ///
    /// JSON's string grammar is a subset of JavaScript's with exactly one gap: U+2028
    /// and U+2029 are legal raw inside a JSON string but are LINE TERMINATORS to a
    /// JavaScript grammar before ES2019, where they end the literal mid-string. So
    /// `serde_json::from_str::<String>` parsing the payload, plus the absence of those
    /// two code points and of any raw line break, is the whole property — checked
    /// against a real JSON parser rather than by eyeballing the escaping.
    ///
    /// ## The two decodes, and why the first must NOT return the body
    ///
    /// The inner `serde_json::to_string` turns the html into JSON TEXT; the outer one
    /// turns that text into a JS string literal, whose VALUE is therefore JSON text
    /// rather than html — and [`preview_swap_script`]'s `JSON.parse` is what undoes
    /// the inner encoding at the far end. `assert_ne!(once, body)` is the row that
    /// states that: with a single encoding the literal's value IS the html, `JSON.parse`
    /// is handed `<`, and every swap throws `SyntaxError: JSON Parse error:
    /// Unrecognized token '<'` while the preview silently keeps showing the previous
    /// document. That was a measured bug, not a hypothetical.
    ///
    /// **`</script>` is deliberately NOT escaped**, and its row asserts only the round
    /// trip. The payload is JS SOURCE handed to `evaluate_javascript`, never HTML
    /// injected into a `<script>` element, so there is no element for it to close.
    ///
    /// Falsifier: replace the body with a single `serde_json::to_string`. Every row
    /// fails at the `assert_ne!`, and the rows carrying `<` fail the second decode as
    /// well — which is the same pair of symptoms the real defect produced.
    #[test]
    fn preview_swap_payload_double_encodes_and_round_trips() {
        // The row count is enforced by [`PAYLOAD_CASES`]' own type, so there is no
        // arity assertion here: one would be a tautology against the same constant.
        for case in &PAYLOAD_CASES {
            let (desc, body) = (case.desc, case.body);
            let payload = preview_swap_payload(body).unwrap_or_else(|e| {
                panic!(
                    "preview_swap_payload({body:?}) failed to encode ({e}) — {desc}. Both encodings \
                     are infallible for a `&str`, so an error here means the function does \
                     something other than encode a string."
                )
            });

            // --- well-formedness of the LITERAL --------------------------------
            assert!(
                payload.starts_with('"') && payload.ends_with('"') && payload.len() >= 2,
                "preview_swap_payload({body:?}) produced {payload:?}, which is not delimited by \
                 double quotes — {desc}. It is interpolated into `JSON.parse({{literal}})` as JS \
                 SOURCE, so anything that is not a complete string literal is a syntax error in the \
                 whole swap script."
            );
            for (name, ch) in [
                ("a raw newline", '\n'),
                ("a raw carriage return", '\r'),
                ("U+2028 LINE SEPARATOR", '\u{2028}'),
                ("U+2029 PARAGRAPH SEPARATOR", '\u{2029}'),
            ] {
                assert!(
                    !payload.contains(ch),
                    "preview_swap_payload({body:?}) left {name} raw in {payload:?} — {desc}. All \
                     four are LINE TERMINATORS to a JavaScript grammar before ES2019, so each ends \
                     the literal mid-string and turns the swap script into a syntax error. JSON \
                     escapes the first two for us; the last two it does NOT, which is exactly what \
                     the two explicit replacements are for."
                );
            }

            // --- the first decode: JSON text, NOT the body ---------------------
            let once: String = serde_json::from_str(&payload).unwrap_or_else(|e| {
                panic!(
                    "preview_swap_payload({body:?}) produced {payload:?}, which a JSON parser \
                     rejects ({e}) — {desc}. JSON's string grammar is a subset of JavaScript's, so \
                     a payload JSON cannot read is a payload JavaScript cannot read either."
                )
            });
            assert_ne!(
                once, body,
                "preview_swap_payload({body:?}) produced a literal whose VALUE is the body itself \
                 — {desc}. That is the SINGLE-ENCODING defect: `preview_swap_script` hands this \
                 value to `JSON.parse`, which is then parsing html and throws `Unrecognized token \
                 '<'` on the very first swap, leaving the previous document on screen while the \
                 user types. The value must be the body's JSON TEXT, which the script's `JSON.parse` \
                 decodes."
            );

            // --- the second decode: the body, byte for byte --------------------
            let twice: String = serde_json::from_str(&once).unwrap_or_else(|e| {
                panic!(
                    "the value of preview_swap_payload({body:?}) is {once:?}, which is not JSON text \
                     ({e}) — {desc}. `preview_swap_script`'s `JSON.parse` performs exactly this \
                     decode in the web view, so what fails here fails there."
                )
            });
            assert_eq!(
                twice, body,
                "double-decoding preview_swap_payload({body:?}) gave {twice:?} — {desc}. The two \
                 encodings and the script's one `JSON.parse` must compose to the identity, or the \
                 document that reaches the DOM is not the document `render_preview` produced."
            );
        }
    }

    /// The payload and the script are ONE mechanism: the script's `JSON.parse` is what
    /// undoes the payload's inner encoding.
    ///
    /// Property: [`preview_swap_script`] embeds the literal verbatim, inside a
    /// `JSON.parse(` call. Without this, deleting the `JSON.parse` from the script
    /// would leave [`preview_swap_payload_double_encodes_and_round_trips`] green while
    /// the web view was handed JSON text as if it were html — the two halves are only
    /// correct together, so the pairing is asserted rather than assumed.
    ///
    /// Falsifier: drop `JSON.parse(` from [`preview_swap_script`], or embed the literal
    /// through anything that rewrites it.
    #[test]
    fn preview_swap_script_parses_the_literal_it_is_given() {
        let body = "<p>she said \"go\"</p>";
        let payload = preview_swap_payload(body).expect("encoding a &str cannot fail");
        let script = preview_swap_script(&payload);

        assert!(
            script.contains(&format!("JSON.parse({payload})")),
            "preview_swap_script did not embed the payload inside a `JSON.parse(...)` call. The \
             payload is DOUBLE encoded precisely because this parse undoes the inner half: without \
             it the DOMParser is handed JSON text — quotes, backslash escapes and all — and \
             renders the document's source instead of the document. Script was:\n{script}"
        );
        assert!(
            script.contains("replaceChildren"),
            "preview_swap_script no longer commits through `replaceChildren`. The body swap is what \
             preserves the reader's scroll offset (measured: 2000 px preserved in 24 of 24 against \
             0 in 24 of 24 for a full `load_html`), and this assertion is what keeps this test from \
             passing against a script that merely parses the payload and drops it. Script \
             was:\n{script}"
        );
    }

    // -----------------------------------------------------------------------------
    // The head guard. [`PaneInner::load_preview_document`] takes the in-place branch
    // only when the recorded offset is not the `0` sentinel, `</head>` is present, the
    // offset is UNMOVED, and a body can be carved out; the two functions below are what
    // decide three of those four, and nothing else in this crate executes them.
    // -----------------------------------------------------------------------------

    /// The Markdown fixture the real-renderer tests below render. Prose and a heading:
    /// enough for a non-empty body, and carrying none of the head markers those tests
    /// assert are ABSENT from a carved body.
    const RENDER_SOURCE: &str = "# Alfa bravo\n\nCharlie delta echo.\n";

    /// A second source, differing from [`RENDER_SOURCE`] in LENGTH as well as content,
    /// so two documents cannot agree on a head offset by coincidence.
    const RENDER_SOURCE_OTHER: &str = "## Foxtrot\n\nGolf hotel india juliett kilo.\n";

    /// A document from the REAL renderer, in the exact configuration
    /// `WolfCore::render_preview` calls it with: `PreviewTheme::Default` and
    /// `WidthMode::Full`. Using the real one is the point — every claim these tests
    /// make about "the head" is a claim about `wolf-core`'s output, so measuring it
    /// against a hand-typed prologue would measure the fixture instead.
    fn rendered(source: &str) -> String {
        wolf_core::markdown::render_html(
            source,
            wolf_core::markdown::PreviewTheme::Default,
            wolf_core::markdown::WidthMode::Full,
        )
    }

    /// [`head_offset`] answers the POSITION of `</head>`, and `None` for a document
    /// that has no head — so `0` is free to mean "nothing loaded" in
    /// [`PaneWidgets::head_len`].
    ///
    /// Property, over the real renderer's output: `Some(n)` with `n != 0` and the
    /// document reading `</head>` at `n`. Two of the guard's four fall-backs rest on
    /// exactly this. A first load into a view is recognised by the recorded offset
    /// being the `0` sentinel, which a real document must therefore never produce; and
    /// a document with no `</head>` — the empty string is the common case, being what
    /// `WolfCore::render_preview` answers on an RPC failure — is refused outright.
    ///
    /// The `starts_with` assertion is what stops this passing on a function that finds
    /// something else. `find("<head>")` also answers a non-zero `Some`, and the guard
    /// built on it would compare OPENING-tag positions, which are identical in every
    /// possible document: the head could then change freely and the swap would still
    /// fire, putting a new body under a stylesheet that moved.
    ///
    /// Falsifier: `html.find("<head>")` — the `starts_with` row fails. Second
    /// falsifier: `Some(html.find("</head>").unwrap_or(0))`, the "simplify the Option
    /// away" edit — both `None` rows fail.
    #[test]
    fn head_offset_answers_the_head_position_and_never_the_sentinel() {
        let html = rendered(RENDER_SOURCE);
        let offset = head_offset(&html).unwrap_or_else(|| {
            panic!(
                "head_offset found no `</head>` in a {} byte document from the real renderer. \
                 Nothing crashes when that happens — the guard simply falls back forever, which is \
                 a preview that re-loads its whole ~1 MB document on every keystroke and throws \
                 the reader back to the top each time.",
                html.len()
            )
        });

        assert_ne!(
            offset, 0,
            "head_offset answered 0 for a document from the real renderer. `PaneWidgets::head_len` \
             uses 0 as its NOTHING-LOADED sentinel, so a real document producing it would be read \
             as a first load on every wake and the body swap would never fire at all."
        );
        assert!(
            html[offset..].starts_with("</head>"),
            "head_offset answered {offset}, where the document reads {:?} rather than `</head>`. \
             The guard compares this number across renders to decide the head has not MOVED, so a \
             number that points at anything constant — `<head>`'s own position is the likely edit \
             — makes that comparison hold for every document and swaps a body under a stylesheet \
             that changed.",
            html.get(offset..offset + 16).unwrap_or("<end of document>")
        );

        let empty = head_offset("");
        assert_eq!(
            empty, None,
            "head_offset(\"\") answered a position. The empty string is what \
             `WolfCore::render_preview` answers on an RPC failure, so it arrives here on any \
             timeout — and any number would let the guard's `offset == recorded` test succeed \
             against a document that does not exist."
        );

        let headless = head_offset("<html><body><p>one</p></body></html>");
        assert_eq!(
            headless, None,
            "head_offset answered a position for a document with a body and no head. A document \
             with no `</head>` has no head to compare against, so the swap must not be permitted \
             into it: the branch leaves the committed head in place, and here there is none."
        );
    }

    /// The recorded head offset moves when the HEAD moves, and not when the body does
    /// — measured against the real renderer rather than assumed of it.
    ///
    /// `crates/wolf-core/src/nvim.rs` documents the rendered `<head>` as BYTE-IDENTICAL
    /// on every call, and that is the premise `load_preview_document`'s
    /// `offset == recorded` test stands on. It is a claim about a DEPENDENCY, so it is
    /// measured here against `wolf_core::markdown::render_html` in the configuration
    /// `render_preview` uses: two different sources give one offset, while the same
    /// source at `WidthMode::Capped` — the one parameter whose CSS lands in the head —
    /// gives a different one. Without the second half the first is satisfied by a
    /// function that answers a constant.
    ///
    /// What this does not establish, and what no test of this function can: the guard
    /// compares an OFFSET, so a head that changed without changing LENGTH is invisible
    /// to it. That is why the invariant is `wolf-core`'s to keep, and why adding a
    /// theme or width parameter to `render_preview` costs the swap.
    ///
    /// Falsifier: return `Some(html.len())`. Every render then records a different
    /// number, `offset == recorded` never holds on a live edit, and the swap silently
    /// degrades to the full `load_html` the measurement in this module exists to avoid
    /// — a regression nothing else here reddens, and one whose only symptom is the
    /// reader losing their place on every keystroke.
    #[test]
    fn head_offset_moves_only_when_the_head_moves() {
        let one = rendered(RENDER_SOURCE);
        let two = rendered(RENDER_SOURCE_OTHER);

        let (body_one, body_two) = (body_inner(&one), body_inner(&two));
        assert_ne!(
            body_one, body_two,
            "the two fixture sources rendered to the same BODY, so the offsets below could agree \
             for a reason that has nothing to do with the head. Change RENDER_SOURCE_OTHER."
        );

        let (head_one, head_two) = (head_offset(&one), head_offset(&two));
        assert_eq!(
            head_one, head_two,
            "two documents from the real renderer, differing only in their source, report \
             different `</head>` offsets. `crates/wolf-core/src/nvim.rs` documents that head as \
             byte-identical on every call, and this is that premise made executable: if it does \
             not hold, every live edit falls back to a full `load_html`. Suspect a new \
             non-constant in `render_html`'s head — a timestamp, a source-derived title, a \
             per-render theme — before suspecting this function."
        );

        let capped = wolf_core::markdown::render_html(
            RENDER_SOURCE,
            wolf_core::markdown::PreviewTheme::Default,
            wolf_core::markdown::WidthMode::Capped,
        );
        let head_capped = head_offset(&capped);
        assert_ne!(
            head_one, head_capped,
            "the same source rendered at `WidthMode::Full` and at `WidthMode::Capped` reports the \
             SAME `</head>` offset, although the width mode's CSS is emitted INTO the head. The \
             guard's whole job is to notice a head that moved, so with these equal it would swap a \
             fresh body in under the previous stylesheet and the document would render at the \
             wrong width until something forced a full load."
        );
    }

    /// [`body_inner`] carves the rendered document's BODY, and none of the ~1 MB head
    /// that precedes it.
    ///
    /// Property, over the real renderer's output: the returned slice is the document's
    /// tail up to the wrapper's closing tags, it carries the rendered markdown, and it
    /// carries no `@font-face`, no `<style`, no `<body` and no `</head>` — the four
    /// head markers that would appear if the first-`<body` scan had landed anywhere but
    /// the wrapper's own start tag.
    ///
    /// That the head contains no `<body` is a PREMISE about `wolf-core`, not a property
    /// of this function, and this is where it is measured rather than assumed: the head
    /// embeds four font licences inside an HTML comment, and `wolf-core`'s
    /// `the_notices_cannot_terminate_an_html_comment` checks those texts for `<!--`,
    /// `-->` and `--!>` and for nothing else. A licence bump that introduced the five
    /// characters `<body` reddens this test — see
    /// [`body_inner_starts_inside_a_head_that_carries_a_literal_body_tag`] for what the
    /// function does in that case.
    ///
    /// Falsifiers, either alone: scan for `<style` instead of `<body`, and the slice
    /// arrives carrying the inlined base64 faces; or drop the `find('>')? + 1` so the
    /// slice opens on the start tag itself, which the `<body` row catches.
    #[test]
    fn body_inner_carves_the_real_documents_body_and_none_of_its_head() {
        let html = rendered(RENDER_SOURCE);
        let inner = body_inner(&html).unwrap_or_else(|| {
            panic!(
                "body_inner carved nothing out of a {} byte document from the real renderer, so \
                 the guard falls back to a full `load_html` on every keystroke. Either the \
                 renderer stopped emitting the fixed `<body …>` wrapper this scan depends on, or \
                 the scan stopped matching it.",
                html.len()
            )
        });

        assert!(
            html.ends_with(&format!("{inner}</body></html>")),
            "the carved body is not the document's tail up to its closing tags, so the slice's END \
             is wrong: `rfind(\"</body>\")` is what places it, and the wrapper's close is always \
             the document's last one. Carved {} bytes out of {}.",
            inner.len(),
            html.len()
        );
        assert!(
            inner.contains("delta"),
            "the carved body does not contain a word of the rendered source, so it is not the body \
             at all — this is the row that keeps the four absence checks below from holding \
             vacuously against an empty slice. Carved: {inner:?}"
        );

        for (what, needle) in [
            ("the inlined base64 faces", "@font-face"),
            ("the stylesheet element", "<style"),
            ("a body START TAG", "<body"),
            ("the head's own closing tag", "</head>"),
        ] {
            assert!(
                !inner.contains(needle),
                "the carved body carries {needle:?} — {what} — so the scan started inside the \
                 HEAD. What reaches the live document is then ~1 MB of stylesheet adopted as \
                 content, on the second load into a view and never on the first, which is the \
                 shape of defect this whole branch is most able to hide. Carved {} bytes out of \
                 {}.",
                inner.len(),
                html.len()
            );
        }
    }

    /// One [`body_inner`] row: a document, the carve it must produce, and the reason
    /// the row is here.
    struct BodyCase {
        desc: &'static str,
        html: &'static str,
        want: Option<&'static str>,
    }

    /// How many named rows the carve table carries — the ARRAY'S length, for exactly
    /// the reason [`NAMED_PAYLOAD_ROWS`] is: dropping a row is a compile error rather
    /// than a check that could itself be edited away.
    const NAMED_BODY_ROWS: usize = 8;

    /// The first-open / last-close asymmetry, and every shape the carve refuses.
    ///
    /// Rows 2 and 3 are the asymmetry itself, stated in the only inputs that can tell
    /// the two scans apart: a document whose CONTENT carries a literal `</body>`, and
    /// one whose content carries a literal `<body …>`. Both are reachable — raw HTML
    /// passes straight through pulldown-cmark — and neither is exotic in a Markdown
    /// file about HTML.
    const BODY_CASES: [BodyCase; NAMED_BODY_ROWS] = [
        BodyCase {
            desc: "the wrapper shape, with nothing unusual inside it",
            html: "<html><head></head><body class=\"wolf-markdown\"><p>one</p></body></html>",
            want: Some("<p>one</p>"),
        },
        BodyCase {
            desc: "LAST close: a literal </body> in the CONTENT stays inside the carve",
            html: "<html><head></head><body class=\"wolf-markdown\"><p>a</p></body>\
                   <p>b</p></body></html>",
            want: Some("<p>a</p></body><p>b</p>"),
        },
        BodyCase {
            desc: "FIRST open: a literal <body ...> in the CONTENT stays inside the carve",
            html: "<html><head></head><body class=\"wolf-markdown\"><p>a</p>\
                   <body id=\"x\"><p>b</p></body></html>",
            want: Some("<p>a</p><body id=\"x\"><p>b</p>"),
        },
        BodyCase {
            desc: "empty: the render answers this on an RPC failure",
            html: "",
            want: None,
        },
        BodyCase {
            desc: "a head and no body at all",
            html: "<html><head></head></html>",
            want: None,
        },
        BodyCase {
            desc: "a start tag no `>` ever closes",
            html: "<html><head></head><body class=\"wolf-markdown\"",
            want: None,
        },
        BodyCase {
            desc: "a start tag with no matching close",
            html: "<html><head></head><body><p>one</p></html>",
            want: None,
        },
        BodyCase {
            desc: "the close BEFORE the open, which is refused rather than reversed",
            html: "</body><body><p>one</p>",
            want: None,
        },
    ];

    /// [`body_inner`] opens at the FIRST `<body` and closes at the LAST `</body>`, and
    /// refuses every document that does not have the wrapper shape.
    ///
    /// The asymmetry is the decision this test exists for, and it is not symmetric by
    /// accident: the wrapper's closing tag is always the document's last one, so
    /// `rfind` is right there; and any `<body` in CONTENT is always after the wrapper's
    /// opening tag, so `find` is right there. Reversing either one is a silent
    /// truncation that only appears for documents carrying the other tag in their
    /// prose, which is why both rows carry one.
    ///
    /// The last row is the `end < start` refusal. Without it that pair would index a
    /// reversed range and panic inside a draw-driven refresh, which takes the window
    /// down rather than the preview.
    ///
    /// Falsifiers, one per direction: `rfind("</body>")` → `find("</body>")` fails row
    /// 2, and `find("<body")` → `rfind("<body")` fails row 3. Deleting the `end < start`
    /// guard turns row 8 into a panic rather than a failed comparison.
    #[test]
    fn body_inner_opens_at_the_first_body_and_closes_at_the_last() {
        // The row count is enforced by [`BODY_CASES`]' own type, so there is no arity
        // assertion here: one would be a tautology against the same constant.
        for case in &BODY_CASES {
            let (desc, html, want) = (case.desc, case.html, case.want);
            let got = body_inner(html);
            assert_eq!(
                got, want,
                "body_inner({html:?}) — {desc}. A wrong `Some` is swapped into the live document \
                 as content and a wrong `None` costs a full `load_html` (and the reader's scroll \
                 position) on every keystroke, so neither direction is the safe one."
            );
        }
    }

    /// A document whose HEAD carries a literal `<body` is carved from INSIDE the head,
    /// and the head guard cannot notice.
    ///
    /// This states what the function DOES rather than that it is safe, because it is
    /// not: `wolf_core::markdown::render_html` embeds four font licences in an HTML
    /// comment in the head, and `wolf-core`'s
    /// `the_notices_cannot_terminate_an_html_comment` checks those texts for `<!--`,
    /// `-->` and `--!>` and for nothing else. A licence bump introducing the five
    /// characters `<body` moves the carve's start into the head — and the second
    /// assertion is the reason that is worse than it sounds: the head offset is
    /// unchanged, so a CONSISTENTLY wrong carve passes `offset == recorded` on every
    /// render and the guard degrades to nothing. `wolf-core`'s
    /// `markup_in_stylesheet_failure` names the same hazard for the stylesheets, which
    /// it does guard; the licence texts are the half nothing covers.
    ///
    /// The macOS front-end's `PreviewScrollSeam.splitDocument` scans the same way, so
    /// this is a shared exposure rather than a regression of this crate's, and
    /// [`body_inner_carves_the_real_documents_body_and_none_of_its_head`] is what
    /// measures that today's head does not trip it.
    ///
    /// It has no falsifier in production, which is deliberate: the edit that reddens it
    /// is the FIX — starting the scan after `</head>` — at which point delete this test
    /// and rewrite the paragraph of [`body_inner`]'s doc it belongs to.
    #[test]
    fn body_inner_starts_inside_a_head_that_carries_a_literal_body_tag() {
        fn hazard(body: &str) -> String {
            format!(
                "<html><head><!-- font licence FAQ: put the font in <body> -->\
                 <style>@font-face{{}}</style></head>\
                 <body class=\"wolf-markdown\">{body}</body></html>"
            )
        }

        // What the carve actually is: the comment's own terminator, the whole
        // stylesheet, the head's closing tag and the real body start tag, all arriving
        // as CONTENT — with the document's genuine body tacked on the end.
        const CARVED_FROM_THE_HEAD: &str =
            " --><style>@font-face{}</style></head><body class=\"wolf-markdown\"><p>one</p>";

        let one = hazard("<p>one</p>");
        let carved = body_inner(&one);
        assert_eq!(
            carved,
            Some(CARVED_FROM_THE_HEAD),
            "body_inner did not carve from the FIRST `<body` in the whole document. If the scan \
             was deliberately moved past `</head>`, this test has done its job: delete it, and \
             rewrite the first-open paragraph of `body_inner`'s doc, which promises this \
             behaviour. If it was not deliberate, the carve now depends on the head's contents in \
             some new way, which is worse than either fixed rule. Document was:\n{one}"
        );

        let two = hazard("<p>two</p>");
        let (head_one, head_two) = (head_offset(&one), head_offset(&two));
        assert_eq!(
            head_one, head_two,
            "the two hazard documents report different head offsets, which would make this test's \
             point moot rather than making the hazard safe. The point is that they agree: the head \
             guard compares `</head>`'s position, that position is unmoved by a body edit, and a \
             carve that is CONSISTENTLY wrong therefore passes the guard on every render — so the \
             stylesheet is swapped in as content on the second load and forever after."
        );
    }
}
