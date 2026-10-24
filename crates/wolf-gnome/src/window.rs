//! ONE `wolf-gnome` window and everything scoped to it: an
//! [`adw::ApplicationWindow`] over a live [`WolfCore`], with its own wake
//! channel, its own pane view, its own overlays, its own `win.`-namespace
//! actions and its own contribution to the persisted state tree.
//!
//! [`build`] is the whole window. [`crate::coordinator`] is the process-scoped
//! layer above it and owns the six things a second copy of would be wrong — the
//! state store, the config, the cell metrics, the `app.`-namespace actions, the
//! session's launch plans and the test-only readiness probe — so everything here
//! is per-window by construction.
//!
//! The window is backed by a live [`WolfCore`]:
//!
//! - **Output / wake:** [`WolfCore`] runs Neovim on its own tokio runtime and
//!   signals a single [`WakeFn`] when a new frame is ready. The wake fires on a
//!   tokio worker thread, so it cannot touch GTK directly; instead it pushes onto a
//!   bounded `async-channel` (capacity 1, coalescing a burst of flushes), and a
//!   [`glib::spawn_future_local`] task on the GTK main thread drains it and calls
//!   `shell::AppShell::on_liveness_wake` (the LIVENESS read, FIRST),
//!   `tab_view::TabsHandle::on_render_wake` (every tab through
//!   [`crate::tabs_ui::tab_wake`], which decides per tab whether its pane view
//!   redraws and consumes split/close/focus intent),
//!   `tree_view::TreeHandle::on_render_wake`, and finally
//!   `shell::AppShell::on_render_wake` (the window title, the application-state
//!   contribution, then the command drain). The shell runs LAST: a drained
//!   `close_window` tears the core down. One core per window means one wake
//!   closure per core, carrying no window token.
//! - **Application state (`state.json`):** the [`RestorePlan`] arrives already
//!   built — the working directory, the window size, and one
//!   [`crate::state_seam::TabPlan`] per saved tab — because a launch restores a
//!   SESSION and the plans are made once, above. Tab 0's layout is queued on its
//!   pane view ([`crate::pane_view::PaneHandle::request_layout_rebuild`]) and
//!   realized on the first wake with a window grid; every later tab is handed to
//!   [`crate::tab_view::TabsHandle::queue_restore`] and materialized on a later
//!   wake, one blocking `WolfCore::create_tabpage` and one blocking
//!   `WolfCore::rebuild_layout` each.
//!
//!   Those rebuilds are ONE of the TEN blocking RPCs this front-end issues, and
//!   `create_tabpage` is another. A materialized tab costs exactly one rebuild:
//!   [`crate::pane_view::PaneHandle::realize_layout`] drops the copy `open_tab`
//!   queued on the same pane view, so no second one lands on the SELECTION path,
//!   which this inventory does not cover. A restored window's launch therefore
//!   carries up to `2T - 1` of them, `T` being the restored tab count bounded by
//!   [`crate::state_seam::MAX_RESTORE_TABS`] — all before that window is
//!   interactive, and none on the crash path, which
//!   [`crate::tab_view::TabsHandle::reset_after_restart`] guarantees by discarding
//!   the queue.
//!
//!   The second is the buffer picker's candidate snapshot
//!   (`WolfCore::list_buffers`), which a RUNNING window can issue on Ctrl+Shift+E,
//!   behind [`crate::buffer_picker::open_decision`]'s guard that it be issued only
//!   while the core reads `Alive`. The third is a per-pane navigation move
//!   (`WolfCore::history_back` / `history_forward`, one call per direction and
//!   counted once) on Alt+Left / Alt+Right, the
//!   fourth the Copy yank (`WolfCore::copy_visual_selection`) on Ctrl+Shift+C, and
//!   the fifth `WolfCore::create_tabpage`, which has TWO triggers — New Tab on
//!   Ctrl+Shift+T and the restore materialization above — each
//!   behind a guard of exactly that shape ([`crate::nav_ui::nav_press`],
//!   [`crate::clipboard_ui::copy_decision`], [`crate::tabs_ui::new_tab_press`],
//!   [`crate::tabs_ui::restore_press`]).
//!   `set_current_tabpage` and `close_tabpage` add none: both are fire-and-forget
//!   on the ordered input channel.
//!
//!   The last five are the MARKDOWN PREVIEW's: `WolfCore::open_preview`,
//!   `WolfCore::edit_source`, and the `WolfCore::render_preview` that
//!   `pane_view::run_preview_render_pass` issues for a changed source — the only one
//!   of the ten that can repeat while the user simply types, which is why it is the
//!   one that is COALESCED ([`mod@crate::preview_coalescer`]): a burst of edits costs
//!   one render on its leading edge, one per
//!   [`crate::preview_coalescer::PREVIEW_MAX_STALENESS`] while it continues, and one
//!   trailing render after it ends — plus two
//!   that need a preview already on screen: `WolfCore::preview_file`, which
//!   repoints a bound preview at a file linked from the rendered document, and
//!   `WolfCore::open_in_browser`, which renders that document for the loopback
//!   server behind Preview in Browser. All five
//!   are gated through [`mod@crate::preview_ui`] with the buffer picker's argument
//!   and its limits. Any of the ten can mark the core `Liveness::NotResponding`
//!   and raise the stall overlay.
//!
//!   PASTE adds none: `gdk::Clipboard::read_text_async` resolves on a later
//!   main-loop turn and `WolfCore::paste` is fire-and-forget on the ordered input
//!   channel, so nothing on that path can time out. The project-search overlay adds
//!   NEITHER a blocking call nor a further stall path: `start_search`
//!   `spawn_blocking`s onto the tokio runtime, `open_at` is fire-and-forget, and the
//!   one blocking call it could have made — `WolfCore::ignorecase()` — is
//!   deliberately not made.
//!
//!   Writing back is [`crate::persistence`], one instance per TAB: every tab is
//!   re-gathered on every wake and as the FIRST thing [`wire_close`] does,
//!   [`WindowHandle::cached_subtree`] assembles them — plus the seeds of any tab
//!   still sitting in the restore queue, which has no page and therefore nothing
//!   else that can speak for it — into this window's subtree in
//!   tab order, and [`crate::coordinator::Coordinator`] assembles the tree and
//!   commits it. The `sidebar_visible` field is written into every tab from the
//!   user's PREFERENCE — it is per WINDOW — and read back at launch off the first
//!   tab through [`crate::state_seam::RestorePlan::sidebar_visible`].
//!
//!   **A window persists EVERY tab, in tab order, and a background tab's is as
//!   trustworthy as the selected one's.** Each leaf is resolved from
//!   `Snapshot::previews` and `Snapshot::win_buffers`, keyed by the pane's own
//!   Neovim window handle, and both maps are global across tabpages; the
//!   `[No Name]` reading belongs to `WolfCore::pane_status`, which joins through
//!   the current-tabpage-only `Snapshot::windows` and which the gather does not
//!   call. A leaf whose window is in neither map refuses that tab's whole gather
//!   ([`crate::pane_layout::resolve_leaves`]) and the tab contributes its cache.
//! - **Liveness / crash recovery:** the liveness read is UNCONDITIONAL and at the
//!   TOP of the drain-loop body, and it GATES the two paint handlers (see
//!   [`mod@crate::liveness_ui`] for the mapping and [`crate::crash_overlay`] for the
//!   widgets). Coalescing that read is safe, and the argument is worth stating
//!   because the capacity-1 channel silently DROPS a wake fired while it is full,
//!   while `liveness::mark_dead` fires exactly once and no redraw wake ever
//!   follows a death: a `try_send` returning `Full` implies an unconsumed item is
//!   queued, which implies a full handler run WILL happen after the drop, and
//!   because `mark_dead` stores `Dead` BEFORE waking and liveness is a
//!   level-triggered atomic read (not an edge-triggered message), that later run
//!   observes `Dead`. `async_channel::Receiver::recv` also frees the slot before
//!   the handler body runs, so a wake fired DURING a handler run finds the queue
//!   empty and succeeds. There is therefore no interleaving in which the death
//!   wake is lost with no subsequent handler run, and no periodic liveness poll
//!   exists (a timer would destroy this design's zero-polling property AND make
//!   the crash smoke unable to discriminate a broken wake bridge).
//! - **Paint:** per-pane painting and the mode-aware cursor live in
//!   [`crate::pane_view`] (which uses the pure [`crate::paint`]/[`crate::cursor`]
//!   seams); this module installs no draw handler of its own.
//! - **Input:** a window-level [`gtk::EventControllerKey`] offers each key to this
//!   window's own [`gtk::IMMulticontext`] and routes what the context declines
//!   through the pure [`crate::gdk_key`] seam into [`WolfCore::send_input`], while
//!   what it CLAIMS arrives as a `commit` string and reaches the same call through
//!   [`mod@crate::im_ui`] — which owns that precedence and the commit rulebook.
//!   nvim routes the result to its current window, and the cursor renders in
//!   whichever pane owns that grid.
//!   The controller has no propagation phase (i.e. Bubble), so it runs only after
//!   [`crate::shell`]'s CAPTURE-phase chord controller has had its say — which is
//!   exactly the guarantee that a bound chord is not also forwarded to Neovim, nor
//!   offered to the input method.
//!   **POINTER input, unlike keys, is not window-level at all**: it is PER-PANE and
//!   lives entirely in [`crate::pane_view`], where each pane `DrawingArea` carries
//!   its own `gtk::GestureDrag` (press → drag → release), `gtk::EventControllerScroll`
//!   (vertical) and `gtk::EventControllerMotion` (position recording), all thin glue
//!   over the pure [`crate::mouse`] and [`crate::cell_at`] seams. This module installs
//!   no pointer controller of its own.
//! - **Commands / menu:** [`crate::shell`] installs one `gio::SimpleAction` per
//!   implemented `win.` action of [`crate::actions::ACTION_TABLE`], builds the primary
//!   menu from the same data, matches the accelerators, and drains
//!   [`WolfCore::drain_commands`] on each wake. A row activated in the command
//!   palette enters that same dispatch, and the shell keeps at most one of the
//!   four transient overlays open at a time. The `app.` rows are the
//!   coordinator's, installed once for the process.
//! - **Resize:** per-pane and global grid resizing live in [`crate::pane_view`]
//!   (each pane's `connect_resize` → [`crate::grid_size_for`] → per-grid/global
//!   resize).
//! - **Lifecycle:** [`WolfCore`] lives in an `Rc<RefCell<Option<_>>>` on the main
//!   thread; closing the window tells the coordinator first, then takes the core
//!   out and runs [`WolfCore::shutdown`].
//!
//! Files are opened from within Neovim (`:e`), via the picker/tree, or via the
//! primary menu's native Open File / Open Folder dialogs (there are still no save
//! / save-as dialogs). When neither the process cwd nor the restored cwd names a
//! usable project, [`crate::workspace::launch_decision`] puts the window into an
//! EMPTY STATE: the sidebar is hidden, no `FileSearch` starts, and an
//! "Open Folder…" status page covers the editor. Mouse and vertical
//! scroll are wired (see the Input bullet above).
//!
//! The shared `wolf-config.yaml`'s `appearance` is resolved against libadwaita's
//! own light/dark reading and applied in both directions the resolved value
//! reaches — `background=dark|light` into the captive Neovim and the GTK chrome's
//! colour scheme — by [`crate::appearance::apply_at_launch`], called once per
//! window as soon as its core exists. No OS COLOUR is read into Neovim's
//! highlight groups; that direction is macOS-only. Deferred is a native command
//! line as Adw widgets: the cmdline strip, the message toast and the
//! `ext_popupmenu` completion popup are all painted from the folded
//! `Snapshot::cmdline` / `Snapshot::toast` / `Snapshot::pum`, and none of them paints
//! grid 1, which under `ext_messages` (`cmdheight=0`) is empty. The strip stays one
//! row; the toast and the popup are overlay children above it, so a message taller
//! than one line costs the pane area no height and scrolls under a pointer gesture.

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::path::Path;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use adw::prelude::*;
use gtk::{gdk, glib};
use wolf_core::{CoreConfig, InputEvent, TabState, WakeFn, WindowState, WolfConfig, WolfCore};

use crate::appearance;
use crate::buffer_picker::{self, BufferPickerHandle};
use crate::coordinator::{self, Coordinator, CoordinatorInner, WindowId};
use crate::crash_overlay;
use crate::frame_metrics;
use crate::frame_stats::Bucket;
use crate::im_ui;
use crate::palette::{self, PaletteHandle};
use crate::picker::{self, PickerHandle};
use crate::ready_seam;
use crate::search_overlay::{self, SearchHandle};
use crate::shell::{
    self, ShellParts, GO_TO_FILE_ACTION, HISTORY_BACK_ACTION, HISTORY_FORWARD_ACTION,
    TOGGLE_SIDEBAR_ACTION,
};
use crate::state_seam::{self, RestorePlan};
use crate::tab_view::{self, RestoreTabs, TabParts, TabsHandle, TabsParts};
use crate::title::WINDOW_TITLE_FALLBACK;
use crate::tree_view::{self, TreeHandle};
use crate::workspace;
use crate::CellMetrics;

/// Initial grid size handed to [`WolfCore`]; the authoritative size is recomputed
/// from the real widget allocation on the first resize.
const INITIAL_COLS: u16 = 80;
/// Initial grid rows (see [`INITIAL_COLS`]).
const INITIAL_ROWS: u16 = 24;

/// Default window size in pixels. Widened from the editor-only default so that,
/// after the tree sidebar takes [`TREE_PANE_WIDTH`] on the left, the editor pane
/// keeps roughly its previous width (important for the OCR smoke tests).
const WINDOW_WIDTH: i32 = 1200;
/// Default window height in pixels (see [`WINDOW_WIDTH`]).
const WINDOW_HEIGHT: i32 = 640;

/// Default width (px) of the file-tree sidebar pane (the `gtk::Paned` divider
/// position). Collapsed to `0` in the empty state by
/// [`crate::shell::AppShell::set_empty_state`].
pub(crate) const TREE_PANE_WIDTH: i32 = 250;

/// Wake channel capacity. One slot is enough: it coalesces a burst of flushes into
/// a single queued redraw (a dropped "full" send just means a redraw is already
/// pending).
const WAKE_CHANNEL_CAP: usize = 1;

/// Icon of the header bar's sidebar-toggle button. From `adwaita-icon-theme`,
/// which the flake supplies and `XDG_DATA_DIRS` exposes; a missing icon renders as
/// `image-missing` and breaks no assertion.
const SIDEBAR_TOGGLE_ICON: &str = "sidebar-show-symbolic";

/// Icon of the header bar's Back button (see [`SIDEBAR_TOGGLE_ICON`]).
const HISTORY_BACK_ICON: &str = "go-previous-symbolic";

/// Icon of the header bar's Forward button (see [`SIDEBAR_TOGGLE_ICON`]).
const HISTORY_FORWARD_ICON: &str = "go-next-symbolic";

/// Everything [`build`] needs, as one struct rather than six positional
/// parameters (`clippy::too_many_arguments` bites at seven), following the
/// [`ShellParts`] / [`crate::persistence::StateParts`] / [`CloseParts`] precedent.
pub struct WindowParts<'a> {
    /// The application this window belongs to. The plain [`gtk::Application`]
    /// rather than [`adw::Application`], because that is what
    /// `GtkWindowExt::application()` answers on the paths that ask for a second
    /// window, and nothing here needs the libadwaita subclass.
    pub app: &'a gtk::Application,
    /// The process-scoped coordinator. Held by the window's closures only as a
    /// `Weak`; see [`mod@crate::coordinator`] on why that direction is forced.
    pub coordinator: &'a Coordinator,
    /// This window's identity, minted by the coordinator.
    pub id: WindowId,
    /// The launch plan: working directory, empty state, one
    /// [`crate::state_seam::TabPlan`] per tab to restore with that tab's saved
    /// layout, the saved window size and the sidebar preference.
    pub plan: &'a RestorePlan,
    /// The user configuration, read once for the process.
    pub config: &'a WolfConfig,
    /// The cell metrics, measured once for the process and shared by `Rc` with
    /// this window's draw and resize closures.
    pub metrics: &'a Rc<CellMetrics>,
}

/// One window's shared state: everything the coordinator needs to ask a window
/// for — its state contribution and its readiness for input — and nothing else.
///
/// It deliberately does NOT hold the [`crate::shell::AppShell`]. The shell is
/// already owned for exactly the window's lifetime by the wake-drain future,
/// which runs until the wake channel closes and the channel closes when the core
/// is torn down; a second owner here would add a lifetime nothing reads.
struct WindowInner {
    /// This window's identity.
    id: WindowId,
    /// The toplevel.
    window: adw::ApplicationWindow,
    /// This window's tabs, live and queued. The persistence glue is per TAB and
    /// lives inside them; what this window contributes to `state.json` is EVERY
    /// tab's state in tab order — see [`WindowHandle::cached_subtree`].
    tabs: TabsHandle,
    /// This window's core, read for its liveness by [`WindowHandle::readiness`].
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The ordinal of this window's first frame-clock `after-paint`, set only while
    /// the readiness probe is pending; see [`crate::ready_seam`].
    first_paint: Cell<Option<u32>>,
}

/// A cloneable handle to one live window, held STRONGLY by the coordinator.
#[derive(Clone)]
pub struct WindowHandle {
    inner: Rc<WindowInner>,
}

impl WindowHandle {
    /// This window's identity.
    #[must_use]
    pub fn id(&self) -> WindowId {
        self.inner.id
    }

    /// Presents this window. Best-effort on Wayland: a compositor is free to
    /// ignore it, and `cage` does.
    pub fn present(&self) {
        self.inner.window.present();
    }

    /// Re-gathers EVERY tab's persisted state; see
    /// [`crate::persistence::StateHandle::refresh_tab`].
    ///
    /// Every tab, not the selected one, and it costs no RPC: a tab's leaves are
    /// resolved from the snapshot's window-keyed maps, which are global across
    /// tabpages ([`crate::pane_layout::resolve_leaves`]).
    pub fn refresh_subtree(&self) {
        for tab in self.inner.tabs.entries() {
            tab.refresh_tab();
        }
    }

    /// This window's contribution to the persisted tree: every LIVE tab that has
    /// something to contribute, in PAGE order, followed by the seeds of the tabs its
    /// launch has QUEUED but not yet materialized, under this window's size — or
    /// `None` when nothing contributed at all.
    ///
    /// The composition, the order and the commit-nothing rule are
    /// [`crate::state_seam::window_contribution`]'s; this reads the three live inputs
    /// and calls it once, so no rule sits inside widget glue no headless test can
    /// execute.
    ///
    /// **PAGE order, not the order the tabs were opened in.**
    /// [`crate::tab_view::TabsHandle::entries`] is insertion order and libadwaita
    /// lets the user rearrange the bar, so each cache is paired with its
    /// [`crate::tab_view::TabHandle::page_position`] and sorted by
    /// [`crate::state_seam::ordered_tab_states`] before it becomes the `live`
    /// argument. That seam carries the reason.
    ///
    /// **The queued tabs are contributed because nothing else can speak for them.**
    /// They have no page, so they have neither a cache nor a live gather, and the
    /// commit does not wait for the wake that materializes them — [`build`] drives
    /// one before this window is presented. Behind a wedged or dead Neovim that is
    /// the only commit there will ever be, because
    /// [`crate::tabs_ui::restore_press`] answers `Wait` forever, so the one-tab tree
    /// it would otherwise write is what the next launch reads. A restore that
    /// ABORTED part-way leaves the same queue in place for the tabs it never created;
    /// see [`crate::tab_view::RestoreTabs`].
    #[must_use]
    pub fn cached_subtree(&self) -> Option<WindowState> {
        let positioned: Vec<(i32, TabState)> = self
            .inner
            .tabs
            .entries()
            .iter()
            .filter_map(|tab| Some((tab.page_position(), tab.cached_tab()?)))
            .collect();
        let live = state_seam::ordered_tab_states(positioned);
        // Read in a statement of its own, so no borrow of the tab layer's queue is
        // live while the seeds are derived from it.
        let pending = self.inner.tabs.pending_restore();
        // With no queue there is nothing to seed, so the three window-level values
        // below are never read; the empty slice is what says so.
        let (layouts, empty, launch_cwd, sidebar_shown) =
            pending
                .as_ref()
                .map_or(([].as_slice(), false, Path::new(""), false), |restore| {
                    (
                        restore.layouts.as_slice(),
                        restore.empty,
                        restore.launch_cwd.as_path(),
                        restore.sidebar_shown,
                    )
                });
        // Position is never persisted on Wayland, so this is the whole geometry.
        let (width, height) = self.inner.window.default_size();
        state_seam::window_contribution(
            width,
            height,
            live,
            layouts,
            empty,
            launch_cwd,
            sidebar_shown,
        )
    }

    /// Where this window IS, for another window's palette: the very string this one
    /// contributes to `state.json`.
    ///
    /// Read out of the cached subtree rather than recomputed, so the palette and
    /// the persisted tree cannot disagree about a window's directory — the rule is
    /// [`crate::state_seam::snapshot_cwd`]'s, applied once, in
    /// [`crate::persistence::StateHandle::refresh_tab`]. An empty-state window
    /// yields `""`, which `wolf_core::rank_palette` renders as "Untitled"; so does
    /// a window whose gate has not opened yet, which has by definition persisted
    /// nothing to be offered under.
    #[must_use]
    pub fn display_cwd(&self) -> String {
        self.cached_subtree()
            .and_then(|window| window.tabs.first().map(|tab| tab.cwd.clone()))
            .unwrap_or_default()
    }

    /// This window's reading for the test-only readiness predicate
    /// ([`crate::ready_seam::launch_readiness`]).
    ///
    /// Each value is read in its own statement, so no borrow of the core or the tab
    /// layer is held across another.
    #[must_use]
    pub fn readiness(&self) -> ready_seam::WindowReadiness {
        let liveness = self.inner.core.borrow().as_ref().map(WolfCore::liveness);
        let panes = self
            .inner
            .tabs
            .selected()
            .map(|tab| tab.panes().readiness());
        let restore_owed = self.inner.tabs.pending_restore().is_some_and(|restore| {
            ready_seam::restore_owed(restore.layouts.len(), restore.may_create)
        });
        let first_paint = self.inner.first_paint.get();
        let has_keyboard = seat_has_keyboard(&self.inner.window);
        let has_focus_widget = GtkWindowExt::focus(&self.inner.window).is_some();
        ready_seam::WindowReadiness {
            liveness,
            panes,
            restore_owed,
            first_paint,
            has_keyboard,
            has_focus_widget,
        }
    }
}

/// Builds one application window and wires it to a freshly started [`WolfCore`].
///
/// `None` — after one message — when the core fails to start; the coordinator
/// then simply has one window fewer, and a launch in which that was the only
/// window has no window at all and exits.
///
/// Registers the window with the coordinator BEFORE the first command drain, so
/// nothing the drain reaches can act on a coordinator that does not yet know the
/// window exists.
#[must_use]
pub fn build(parts: WindowParts<'_>) -> Option<WindowHandle> {
    let WindowParts {
        app,
        coordinator,
        id,
        plan,
        config,
        metrics,
    } = parts;

    // Read once: only a window built while the readiness probe is pending wires its triggers; others
    // pay one bool test per wake.
    let ready_armed = coordinator.ready_pending();
    let coordinator_weak = coordinator.weak();

    // Wake plumbing: the tokio worker only `try_send`s; a full-channel drop is correct, a redraw is
    // pending. See notes/wolf-gnome-decisions.md#threading-and-the-wake
    let (wake_tx, wake_rx) = async_channel::bounded::<()>(WAKE_CHANNEL_CAP);
    let wake: WakeFn = Arc::new(move || {
        let _ = wake_tx.try_send(());
    });

    let empty = plan.empty;

    let cfg = CoreConfig {
        init_lua: crate::app::init_lua_path(),
        cols: INITIAL_COLS,
        rows: INITIAL_ROWS,
        cwd: plan.core_cwd.clone(),
    };
    let core = match WolfCore::start(cfg, wake) {
        Ok(core) => core,
        Err(err) => {
            eprintln!("wolf-gnome: failed to start core: {err}");
            return None;
        }
    };
    // The core is non-Send; it lives only here on the main thread, shared by the
    // draw/key/resize/close closures via Rc, and taken out on close.
    let core = Rc::new(RefCell::new(Some(core)));

    // Applied after the core exists, so the `background` push has a target, and before any widget,
    // so the window realizes under the resolved scheme with no flash.
    appearance::apply_at_launch(config, &core);

    // The tab layer: each tab has its own pane view, empty state and persistence over this window's
    // one core. No page exists yet; tab 0 opens below, once the toplevel is built.
    let tabs = tab_view::build(TabsParts {
        core: &core,
        metrics,
    });

    // Main child is the tab view so the overlay spans the pane area (`OVERLAY_ORIGIN_*_EXPECTED`);
    // add order is stacking. See notes/crates/wolf-gnome/src/window.rs.md#overlay-stacking-across-tabs
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&tabs.view_widget()));

    let crash = crash_overlay::build();

    // Focus returns to the selected tab's strip, resolved at close time: a captured per-tab strip is
    // unmapped after a switch and kills key dispatch. See `picker::FocusTarget`.
    let focus_target = tabs.focus_target();

    let picker = picker::build(&overlay, focus_target.clone(), &core);
    // Switch to Window rows, from the coordinator per open. Weak: a strong ref closes the
    // coordinator -> window -> palette cycle and leaks the graph; a dropped coordinator yields none.
    let palette_windows: palette::WindowSource = {
        let weak = coordinator.weak();
        Rc::new(move || {
            coordinator::upgrade(&weak)
                .map(|coordinator| coordinator.palette_windows(id))
                .unwrap_or_default()
        })
    };
    let palette = palette::build(&overlay, focus_target.clone(), &core, palette_windows);
    let buffers = buffer_picker::build(&overlay, focus_target.clone(), &core);
    let search = search_overlay::build(&overlay, focus_target.clone(), &core);

    overlay.add_overlay(&crash.widget());
    overlay.add_overlay(&crash.banner_widget());

    // The tree reads no cwd: its root label is passed here and on each `reroot`, so the shell alone
    // knows the root. Per window, not per tab: see notes/wolf-gnome-decisions.md#window-structure
    let tree = tree_view::build(
        &core,
        focus_target.clone(),
        !empty,
        &workspace::root_label(&plan.core_cwd),
    );
    let sidebar = tree.widget();
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_start_child(Some(&sidebar));
    paned.set_end_child(Some(&overlay));
    paned.set_position(TREE_PANE_WIDTH);

    // No title widget is deliberate: one changes the header bar's natural height, which is a term of
    // `PANE_ORIGIN_Y_EXPECTED`. See notes/wolf-gnome-decisions.md#window-structure
    let header = adw::HeaderBar::new();

    let sidebar_button = gtk::Button::builder()
        .icon_name(SIDEBAR_TOGGLE_ICON)
        .tooltip_text("Toggle sidebar (Ctrl+Shift+B)")
        .build();
    header.pack_start(&sidebar_button);

    // Both start insensitive until the first render wake's `refresh_nav_buttons`: with no snapshot
    // `nav_press` refuses every press, so a sensitive button would only print a refusal.
    let nav_back_button = gtk::Button::builder()
        .icon_name(HISTORY_BACK_ICON)
        .tooltip_text("Back (Alt+Left)")
        .sensitive(false)
        .build();
    let nav_forward_button = gtk::Button::builder()
        .icon_name(HISTORY_FORWARD_ICON)
        .tooltip_text("Forward (Alt+Right)")
        .sensitive(false)
        .build();
    let nav_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    nav_box.add_css_class("linked");
    nav_box.append(&nav_back_button);
    nav_box.append(&nav_forward_button);
    header.pack_start(&nav_box);

    let open_button = gtk::Button::builder()
        .icon_name("document-open-symbolic")
        .tooltip_text("Open file (Ctrl+Shift+O)")
        .build();
    header.pack_start(&open_button);

    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Main menu")
        .menu_model(&shell::build_menu_model())
        .build();
    header.pack_end(&menu_button);

    // Header, tab bar, then the split. The bar autohides at one page, keeping one-tab geometry equal
    // to no tab layer. See notes/wolf-gnome-decisions.md#window-structure
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&tabs.bar_widget());
    content.append(&paned);

    // The restored size, or the defaults. Window POSITION is never restored:
    // Wayland gives a client neither its own position nor screen geometry.
    let (window_width, window_height) = plan.window_size.unwrap_or((WINDOW_WIDTH, WINDOW_HEIGHT));
    let window = adw::ApplicationWindow::builder()
        .application(app)
        // The same string `title::window_title` falls back to, so the title never
        // flickers between two spellings before the first cwd is reported.
        .title(WINDOW_TITLE_FALLBACK)
        .default_width(window_width)
        .default_height(window_height)
        .content(&content)
        .build();

    // Tab 0, with the first saved tab's layout. Tabpage 0 adopts the real handle on the first wake
    // via `tabs_ui::tab_wake`, a snapshot read, not `current_tabpage()`'s blocking RPC.
    if !tabs.open_tab(TabParts {
        tabpage: 0,
        cwd: plan.core_cwd.clone(),
        empty,
        sidebar_shown: plan.sidebar_visible,
        layout: plan.tabs.first().and_then(|tab| tab.layout.clone()),
    }) {
        eprintln!("wolf-gnome: failed to open the window's first tab");
        return None;
    }

    // The `win.*` actions, accelerators and command drain, installed before `present()` so the menu
    // and header button resolve them; the `app.*` rows are the coordinator's.
    let shell = shell::build(ShellParts {
        core: &core,
        window: &window,
        picker: &picker,
        palette: &palette,
        buffers: &buffers,
        search: &search,
        tree: &tree,
        sidebar: &sidebar,
        paned: &paned,
        open_button: &open_button,
        nav_back_button: &nav_back_button,
        nav_forward_button: &nav_forward_button,
        sidebar_button: &sidebar_button,
        sidebar_shown: plan.sidebar_visible,
        launch_cwd: plan.core_cwd.clone(),
        crash_overlay: &crash,
        tabs: &tabs,
        coordinator: coordinator.weak(),
        window_id: id,
    });
    shell.install_actions(app);
    shell.set_empty_state(empty);

    // Explicit activation rather than the `action-name` property: a widget naming a not-yet-installed
    // action renders insensitive.
    {
        let window = window.clone();
        open_button.connect_clicked(move |_button| {
            // Qualified: `gtk::Widget::activate_action` takes the `win.`-prefixed name and reports failure;
            // `gio::ActionGroup`'s takes a bare name and returns nothing.
            if let Err(err) =
                gtk::prelude::WidgetExt::activate_action(&window, GO_TO_FILE_ACTION, None)
            {
                eprintln!("wolf-gnome: failed to activate {GO_TO_FILE_ACTION}: {err}");
            }
        });
    }

    // The toolbar buttons activate their chords' actions in the Open button's shape. No smoke clicks
    // them, so that shape is the coverage: crates/wolf-gnome/tests/README.md#deliberate-test-gaps
    {
        let window = window.clone();
        sidebar_button.connect_clicked(move |_button| {
            if let Err(err) =
                gtk::prelude::WidgetExt::activate_action(&window, TOGGLE_SIDEBAR_ACTION, None)
            {
                eprintln!("wolf-gnome: failed to activate {TOGGLE_SIDEBAR_ACTION}: {err}");
            }
        });
    }
    {
        let window = window.clone();
        nav_back_button.connect_clicked(move |_button| {
            if let Err(err) =
                gtk::prelude::WidgetExt::activate_action(&window, HISTORY_BACK_ACTION, None)
            {
                eprintln!("wolf-gnome: failed to activate {HISTORY_BACK_ACTION}: {err}");
            }
        });
    }
    {
        let window = window.clone();
        nav_forward_button.connect_clicked(move |_button| {
            if let Err(err) =
                gtk::prelude::WidgetExt::activate_action(&window, HISTORY_FORWARD_ACTION, None)
            {
                eprintln!("wolf-gnome: failed to activate {HISTORY_FORWARD_ACTION}: {err}");
            }
        });
    }

    // Drain order is fixed: liveness first, gating the tab layer and tree; the shell's command drain
    // last and ungated. See notes/wolf-gnome-decisions.md#threading-and-the-wake
    {
        let tabs = tabs.clone();
        let tree = tree.clone();
        let shell = shell.clone();
        let coordinator_weak = coordinator_weak.clone();
        glib::spawn_future_local(async move {
            while wake_rx.recv().await.is_ok() {
                // `count(wake) == wakes` by construction: this counter and the region below share one pass.
                frame_metrics::note_wake();
                {
                    let _wake_guard = frame_metrics::scope(Bucket::Wake);
                    let paint = shell.on_liveness_wake();
                    if paint {
                        {
                            let _guard = frame_metrics::scope(Bucket::WakeTabs);
                            tabs.on_render_wake();
                        }
                        {
                            let _guard = frame_metrics::scope(Bucket::WakeTree);
                            tree.on_render_wake();
                        }
                    }
                    {
                        let _guard = frame_metrics::scope(Bucket::WakeShell);
                        shell.on_render_wake();
                    }
                }
                // After the `wake` region closes, so the emit's cost is never measured. No timer: see
                // notes/wolf-gnome-decisions.md#frame-time-instrumentation
                frame_metrics::maybe_emit();
                // The end of a wake is when a pane binds, a grid is resized and a
                // restore step lands, so it is one of the readiness triggers.
                if ready_armed {
                    if let Some(coordinator) = coordinator::upgrade(&coordinator_weak) {
                        coordinator.check_ready();
                    }
                }
            }
        });
    }
    // Later saved tabs are queued, not created: materializing needs tab 0's adopted handle (see
    // `tabs_ui::restore_press`). Queued before the wake pair below so its first wake sees them.
    if plan.tabs.len() > 1 {
        tabs.queue_restore(RestoreTabs {
            launch_cwd: plan.core_cwd.clone(),
            empty,
            sidebar_shown: plan.sidebar_visible,
            layouts: plan.tabs[1..]
                .iter()
                .map(|tab| tab.layout.clone())
                .collect(),
            // Normalized by `queue_restore` regardless; only the remainder an abort
            // re-seats is ever queued with creation withdrawn.
            may_create: true,
        });
    }

    // React to the initial frame through the liveness gate. Uninstrumented, here and below: this is
    // launch, and timing it would break `count(wake) == wakes`.
    if shell.on_liveness_wake() {
        tabs.on_render_wake();
    }

    let im = wire_keys(&window, &core, &picker, &palette, &buffers, &search);
    wire_close(CloseParts {
        window: &window,
        core: &core,
        im: &im,
        picker: &picker,
        palette: &palette,
        buffers: &buffers,
        search: &search,
        tree: &tree,
        coordinator: coordinator.weak(),
        id,
    });

    // Registered before the first drain, so a coordinator call it reaches finds this window.
    let handle = WindowHandle {
        inner: Rc::new(WindowInner {
            id,
            window: window.clone(),
            tabs: tabs.clone(),
            core: core.clone(),
            first_paint: Cell::new(None),
        }),
    };
    coordinator.register(&handle);

    // The first drain runs after `wire_close` installs `close-request`: a drained `CloseWindow` before
    // it would tear the window down without shutting the core, orphaning nvim.
    shell.on_render_wake();

    window.present();
    // Focus the SELECTED tab's cmdline strip so key events have a focused target in
    // the editor region to bubble from to the window-level key controller.
    if let Some(area) = (focus_target)() {
        area.grab_focus();
    }
    if ready_armed {
        wire_ready_triggers(&handle, coordinator_weak);
    }

    Some(handle)
}

/// Whether `window`'s display's default seat exposes a keyboard device — the
/// readiness file's delivery precondition ([`crate::ready_seam`]).
///
/// It reads the seat's non-logical devices, `devices(KEYBOARD)`, which lists the
/// `wtype` virtual keyboard, not `SeatExt::keyboard`: that answers GDK's logical
/// "Core Keyboard", which exists (and `capabilities()` reports `KEYBOARD`) on a seat
/// with no keyboard at all, so it would make the conjunct vacuous. Measured under
/// `cage`: `devices(KEYBOARD)` is empty until a virtual keyboard has existed, and
/// stays non-empty after the `wtype` that created it exits.
fn seat_has_keyboard(window: &adw::ApplicationWindow) -> bool {
    WidgetExt::display(window)
        .default_seat()
        .is_some_and(|seat| !seat.devices(gdk::SeatCapabilities::KEYBOARD).is_empty())
}

/// Connects the two readiness triggers no wake covers: the seat gaining a device
/// (`device-added`, which is how a keyboard appears) and a painted frame (the frame
/// clock's `after-paint`, whose first firing also records this window's map
/// ordinal).
///
/// Called by [`build`] only while the readiness probe is pending, after
/// `present()`, so the frame clock exists. Both closures hold `Weak`s, so neither
/// keeps the coordinator or the window alive.
fn wire_ready_triggers(handle: &WindowHandle, coordinator: Weak<CoordinatorInner>) {
    if let Some(seat) = WidgetExt::display(&handle.inner.window).default_seat() {
        let coordinator = coordinator.clone();
        seat.connect_device_added(move |_seat, _device| {
            if let Some(coordinator) = coordinator::upgrade(&coordinator) {
                coordinator.check_ready();
            }
        });
    }
    let Some(clock) = handle.inner.window.frame_clock() else {
        // Not `eprintln!`, which panics on a failed stderr write: a diagnostic must
        // never take the application down.
        let _ = std::io::stderr().write_all(
            b"wolf-gnome: the window has no frame clock after present(); the readiness file \
              will never be written\n",
        );
        return;
    };
    let window = Rc::downgrade(&handle.inner);
    clock.connect_after_paint(move |_clock| {
        let Some(coordinator) = coordinator::upgrade(&coordinator) else {
            return;
        };
        if let Some(inner) = window.upgrade() {
            if inner.first_paint.get().is_none() {
                inner
                    .first_paint
                    .set(Some(coordinator.next_paint_ordinal()));
            }
        }
        coordinator.check_ready();
    });
}

/// Installs this window's input method and its key controller, and returns the
/// context so [`wire_close`] can tear it down.
///
/// While any transient overlay (the file picker, the command palette, the buffer
/// picker or the project search) is open, keys are left for that overlay's focused
/// widgets: nothing is forwarded to the core, and — the half that is easy to lose —
/// nothing is OFFERED to the input method either. Otherwise every press is offered
/// to this window's [`gtk::IMMulticontext`] first, and only a press the context
/// DECLINES is translated via the pure [`crate::gdk_key`] seam into core input; a
/// press it claims arrives later as a `commit` string, which becomes core input
/// through [`crate::im_ui::commit_decision`] instead. The precedence and the commit
/// rulebook are [`mod@crate::im_ui`]'s, not this function's — every branch below
/// reads its answer from [`crate::im_ui::key_route`]. No smoke discriminates the
/// overlay guard — see "What is still unproven" in
/// `crates/wolf-gnome/tests/README.md`.
///
/// The controller carries NO propagation phase (i.e. Bubble), so a chord Wolf
/// binds — Ctrl+Shift+O opens the picker, Ctrl+Shift+P the palette, Ctrl+Shift+E
/// the buffer picker, Ctrl+Shift+F the project search — never reaches it:
/// [`crate::shell`]'s chord controller matches it in the CAPTURE phase and stops
/// propagation. That is deliberate; it is what guarantees a bound chord is not ALSO
/// forwarded to Neovim.
///
/// **Do not unify the two controllers, and do not reach for
/// `EventControllerKey::set_im_context`.** Both are one-line "simplifications" and
/// both invert the ordering above. GTK runs every capture-phase controller
/// root-to-target before any bubble-phase one, so the chord controller has first
/// refusal by CONSTRUCTION rather than by cooperation: a bound, enabled chord is
/// matched and stopped before the input method is ever offered the key. Attaching
/// the context to the chord controller, or moving this controller into the capture
/// phase, would offer every bound chord to the context first and let it swallow
/// them. `set_im_context` fails a different way: GTK would make the offer before
/// this function's overlay guard ran, which moves the overlay-versus-input-method
/// precedence out of this crate and makes it unstatable and untestable. Calling
/// `filter_keypress` by hand from inside `key-pressed` was measured to behave
/// identically, so the hand-rolled call costs nothing.
///
/// Releases are forwarded to the context for the same measurement's sake:
/// `set_im_context` offers BOTH phases, so a press-only forward silently diverges
/// from the thing that measurement licensed, and an input method does claim events
/// while a sequence is pending. Nothing else in this crate consumes key releases, so
/// the forward costs nothing and closes a class of wedged-state-machine failures
/// that would otherwise be silent.
fn wire_keys(
    window: &adw::ApplicationWindow,
    core: &Rc<RefCell<Option<WolfCore>>>,
    picker: &PickerHandle,
    palette: &PaletteHandle,
    buffers: &BufferPickerHandle,
    search: &SearchHandle,
) -> gtk::IMMulticontext {
    // Constructed here rather than passed in: nothing above needs to supply it, and
    // returning it keeps this function at six parameters.
    let im = gtk::IMMulticontext::new();
    // BEFORE `focus_in` below, so the context already has a surface to work against
    // at the moment it is told it is focused.
    im.set_client_widget(Some(window));

    {
        let core = core.clone();
        let picker = picker.clone();
        let palette = palette.clone();
        let buffers = buffers.clone();
        let search = search.clone();
        im.connect_commit(move |_im, text| {
            let overlay_open =
                picker.is_open() || palette.is_open() || buffers.is_open() || search.is_open();
            match im_ui::commit_decision(overlay_open, text) {
                im_ui::CommitDecision::Refuse(reason) => {
                    eprintln!("wolf-gnome: commit not forwarded: {reason}");
                }
                im_ui::CommitDecision::Send(events) => {
                    // A commit with no core performs nothing: the window is tearing down. The borrow lives in this
                    // one statement, per notes/wolf-gnome-decisions.md#threading-and-the-wake
                    if let Some(core) = core.borrow().as_ref() {
                        for event in events {
                            core.send_input(InputEvent::Key(event));
                            // One per Key sent, so an N-character commit counts N: the contract is keys delivered, and most
                            // keys arrive as commits since the input method filters plain printables.
                            frame_metrics::note_keystroke();
                        }
                    }
                }
            }
        });
    }

    im.focus_in();

    let controller = gtk::EventControllerKey::new();
    {
        let core = core.clone();
        let picker = picker.clone();
        let palette = palette.clone();
        let buffers = buffers.clone();
        let search = search.clone();
        let im = im.clone();
        controller.connect_key_pressed(move |controller, keyval, _keycode, state| {
            let overlay_open =
                picker.is_open() || palette.is_open() || buffers.is_open() || search.is_open();
            // Asked with `None` FIRST: the answer is what licenses the offer, and the
            // offer cannot be taken back once made.
            let route = match im_ui::key_route(im_ui::KeyPhase::Press, overlay_open, None) {
                im_ui::KeyRoute::AskInputMethod => {
                    // No current event means the input method declined: falling through loses at most a composition,
                    // claiming loses the key. So `is_some_and`, never `map_or(true, …)`.
                    let filtered = controller
                        .current_event()
                        .is_some_and(|event| im.filter_keypress(&event));
                    im_ui::key_route(im_ui::KeyPhase::Press, overlay_open, Some(filtered))
                }
                settled => settled,
            };
            match route {
                // Nothing forwarded, nothing offered, nothing counted.
                im_ui::KeyRoute::Overlay => glib::Propagation::Proceed,
                im_ui::KeyRoute::Consumed => glib::Propagation::Stop,
                im_ui::KeyRoute::Translate => match crate::gdk_key(keyval, state) {
                    Some(event) => {
                        if let Some(core) = core.borrow().as_ref() {
                            core.send_input(InputEvent::Key(event));
                            // Counted where delivered, so keys the overlay guard swallowed, the input method claimed or that
                            // reached no core stay out of the report's `wakes/key` denominator.
                            frame_metrics::note_keystroke();
                        }
                        glib::Propagation::Stop
                    }
                    None => glib::Propagation::Proceed,
                },
                // Both unreachable on a press and spelled out, not `_`, so a new `KeyRoute` is an E0004 here:
                // `Ignore` answers a release, and the re-ask passed `Some(_)`, never `AskInputMethod`.
                im_ui::KeyRoute::Ignore | im_ui::KeyRoute::AskInputMethod => {
                    glib::Propagation::Proceed
                }
            }
        });
    }

    {
        let picker = picker.clone();
        let palette = palette.clone();
        let buffers = buffers.clone();
        let search = search.clone();
        let im = im.clone();
        controller.connect_key_released(move |controller, _keyval, _keycode, _state| {
            let overlay_open =
                picker.is_open() || palette.is_open() || buffers.is_open() || search.is_open();
            let route = match im_ui::key_route(im_ui::KeyPhase::Release, overlay_open, None) {
                im_ui::KeyRoute::AskInputMethod => {
                    // `is_some_and` for the press handler's reason: a missing event means the input method declined.
                    // It matters less here, but the two handlers must not disagree on it.
                    let filtered = controller
                        .current_event()
                        .is_some_and(|event| im.filter_keypress(&event));
                    im_ui::key_route(im_ui::KeyPhase::Release, overlay_open, Some(filtered))
                }
                settled => settled,
            };
            // `key-released` is a void signal: a claimed release cannot be stopped, a declined one needs
            // nothing. So the offer above is the point; the match stays `_`-free to force classification.
            match route {
                im_ui::KeyRoute::Overlay
                | im_ui::KeyRoute::Consumed
                | im_ui::KeyRoute::Ignore
                | im_ui::KeyRoute::AskInputMethod => {}
                // Never `crate::gdk_key`: the press was already forwarded, so translating the release doubles
                // every key.
                im_ui::KeyRoute::Translate => {}
            }
        });
    }

    window.add_controller(controller);
    im
}

/// The handles [`wire_close`] needs, as one struct rather than nine positional
/// parameters (`clippy::too_many_arguments` bites at seven), following the
/// [`ShellParts`] / [`crate::persistence::StateParts`] precedent.
struct CloseParts<'a> {
    /// The window whose `close-request` is being handled.
    window: &'a adw::ApplicationWindow,
    /// The live core, taken out and shut down by the handler.
    core: &'a Rc<RefCell<Option<WolfCore>>>,
    /// This window's input method, focused out and detached from its client widget
    /// as the LAST thing the handler does — after the core has been taken.
    im: &'a gtk::IMMulticontext,
    /// The file picker, whose close drops its `FileSearch`.
    picker: &'a PickerHandle,
    /// The command palette, closed for teardown symmetry.
    palette: &'a PaletteHandle,
    /// The buffer picker, closed for teardown symmetry.
    buffers: &'a BufferPickerHandle,
    /// The project-search overlay, whose close CANCELS an in-flight `rg`.
    search: &'a SearchHandle,
    /// The file-tree sidebar, whose close drops its `FileSearch`.
    tree: &'a TreeHandle,
    /// The coordinator, told FIRST — as a `Weak`, because it owns this window's
    /// handle and this closure is owned by the window.
    coordinator: Weak<CoordinatorInner>,
    /// The closing window's identity.
    id: WindowId,
}

/// Installs the close handler: tell the coordinator, then take the core out
/// (releasing the RefCell borrow in its own statement) and shut it down cleanly.
///
/// **The `coordinator.on_window_closing(id)` call is the FIRST statement in the
/// handler, and that ordering is load-bearing.** It must run while the core and
/// the panes are still alive: a subtree gathered after `core.borrow_mut().take()`
/// sees no core, `state_seam::may_snapshot` refuses it, and the session is
/// silently not saved. Moving it below any of the three statements that follow
/// would leave the relaunch smokes
/// (`pane_layout_survives_relaunch_under_cage`,
/// `single_pane_file_survives_relaunch_under_cage`) with an empty `state.json`.
///
/// What that call DOES with the subtree is [`crate::state_seam::close_commit`]'s
/// three-arm rule, not this handler's: closing the last window keeps the session,
/// closing one of several drops that window, and a close during a quit commits
/// nothing because the quit path already committed everything.
///
/// The input-method teardown is the LAST statement, and that ordering is
/// load-bearing for its own reason: `focus_out` on a context holding a pending
/// compose sequence may COMMIT it, and running the teardown after
/// `core.borrow_mut().take()` means such a commit finds `None` and performs nothing
/// rather than pushing into a core that is being shut down. Clearing the client
/// widget is what keeps the context from holding the closed window's whole widget
/// tree alive — `gtk_im_context_set_client_widget` stores the widget in the context,
/// which the controller closures own and which the window owns, and this application
/// closes windows routinely.
fn wire_close(parts: CloseParts<'_>) {
    let core = parts.core.clone();
    let im = parts.im.clone();
    let picker = parts.picker.clone();
    let palette = parts.palette.clone();
    let buffers = parts.buffers.clone();
    let search = parts.search.clone();
    let tree = parts.tree.clone();
    let weak = parts.coordinator;
    let id = parts.id;
    parts.window.connect_close_request(move |_window| {
        // First, while core and panes live: synchronous gather, commit and flush, releasing every core
        // borrow so the `borrow_mut().take()` below cannot panic.
        if let Some(coordinator) = coordinator::upgrade(&weak) {
            coordinator.on_window_closing(id);
        }
        // Close these first so background work drops before the core shuts down: `FileSearch`es cancel,
        // `rg` dies. The palette and buffer picker closes are symmetry; all five are idempotent.
        picker.close();
        palette.close();
        buffers.close();
        search.close();
        tree.close();
        // Take in its own statement so the RefCell borrow is released before
        // shutdown() (which consumes the core and blocks on teardown).
        let taken = core.borrow_mut().take();
        if let Some(core) = taken {
            if let Err(err) = core.shutdown() {
                eprintln!("wolf-gnome: core shutdown error: {err}");
            }
        }
        // LAST, and after the core is gone: see the doc comment above for both
        // reasons.
        im.focus_out();
        im.set_client_widget(None::<&gtk::Widget>);
        glib::Propagation::Proceed
    });
}

#[cfg(test)]
mod tests {
    use super::WAKE_CHANNEL_CAP;

    // Pins the transport half of the wake bridge's safety argument against an `async-channel` change.
    // See notes/crates/wolf-gnome/src/window.rs.md#wake-bridge-tests

    #[test]
    fn a_dropped_wake_implies_a_pending_wake() {
        let (wake_tx, wake_rx) = async_channel::bounded::<()>(WAKE_CHANNEL_CAP);

        for slot in 0..WAKE_CHANNEL_CAP {
            assert!(
                wake_tx.try_send(()).is_ok(),
                "filling the wake channel: send {slot} of {WAKE_CHANNEL_CAP} must be accepted, \
                 or the capacity is not what the drain loop was designed against"
            );
        }

        let overflow = wake_tx.try_send(());
        assert!(
            // Matched through a reference so `overflow` survives for the message.
            matches!(&overflow, Err(async_channel::TrySendError::Full(_))),
            "a send into a FULL wake channel must be refused with Full (the receiver is still \
             alive here, so a Closed would mean something else entirely); got {overflow:?}"
        );

        // The load-bearing implication: the wake that was just dropped left a
        // queued one behind it, so a handler run is still guaranteed to follow.
        assert!(
            wake_rx.try_recv().is_ok(),
            "a DROPPED wake must imply a PENDING wake: the send above was refused because the \
             queue was full, so an unconsumed item must still be there for the drain loop to \
             pop. If this ever fails, a death wake can be lost with no handler run after it, and \
             the crash overlay needs a periodic liveness poll to appear at all."
        );
    }

    #[test]
    fn consuming_a_wake_frees_the_slot_before_the_handler_runs() {
        let (wake_tx, wake_rx) = async_channel::bounded::<()>(WAKE_CHANNEL_CAP);

        for slot in 0..WAKE_CHANNEL_CAP {
            assert!(
                wake_tx.try_send(()).is_ok(),
                "filling the wake channel: send {slot} of {WAKE_CHANNEL_CAP} must be accepted"
            );
        }
        assert!(
            wake_tx.try_send(()).is_err(),
            "precondition: the wake channel must be full before the receive below can show \
             anything"
        );

        assert!(
            wake_rx.try_recv().is_ok(),
            "the queued wake must be receivable"
        );

        // `recv()` resolves over `try_recv`, popping the item before the future resolves, so a wake fired
        // during a handler run (the `mark_dead` case) is accepted, not dropped.
        assert!(
            wake_tx.try_send(()).is_ok(),
            "consuming a wake must free its slot BEFORE the handlers run: a death wake fired \
             while the drain loop is mid-handler must be accepted, not dropped"
        );
    }

    #[test]
    fn wake_channel_capacity_is_at_least_one() {
        assert_ne!(
            WAKE_CHANNEL_CAP, 0,
            "the wake channel capacity must be at least 1: async_channel::bounded panics on a \
             capacity of 0, and a zero-capacity channel would drop every wake fired while the \
             drain loop was not already parked in recv()"
        );

        let (wake_tx, wake_rx) = async_channel::bounded::<()>(WAKE_CHANNEL_CAP);
        assert!(
            wake_tx.try_send(()).is_ok(),
            "at least one wake must fit in the channel while nothing is draining it — that is \
             what makes a wake fired from a tokio worker reach the GTK main loop at all"
        );
        assert!(
            wake_rx.try_recv().is_ok(),
            "and the drain loop must be able to pop it"
        );
    }
}
