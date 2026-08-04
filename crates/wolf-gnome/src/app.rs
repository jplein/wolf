//! The windowed `wolf-gnome` application: all GTK4/Adwaita window wiring lives here.
//!
//! [`run`] builds an [`adw::Application`], and on `activate` constructs an
//! [`adw::ApplicationWindow`] containing an [`adw::HeaderBar`] over a horizontal
//! [`gtk::Paned`] of the file-tree sidebar ([`crate::tree_view`]) and the editor
//! [`gtk::Overlay`]. The overlay's main child is the **native pane view**
//! ([`crate::pane_view`]) — a tree of nested [`gtk::Paned`]s with one
//! [`gtk::DrawingArea`] per pane plus a bottom cmdline strip — and its overlay
//! children are the empty state, the file picker ([`crate::picker`]), the command
//! palette ([`crate::palette`]), the buffer picker ([`crate::buffer_picker`]), the
//! project-search overlay ([`crate::search_overlay`]), and
//! the crash overlay with its restart banner. The window is backed by a live
//! [`WolfCore`]:
//!
//! - **Output / wake:** [`WolfCore`] runs Neovim on its own tokio runtime and
//!   signals a single [`WakeFn`] when a new frame is ready. The wake fires on a
//!   tokio worker thread, so it cannot touch GTK directly; instead it pushes onto a
//!   bounded `async-channel` (capacity 1, coalescing a burst of flushes), and a
//!   [`glib::spawn_future_local`] task on the GTK main thread drains it and calls
//!   `shell::AppShell::on_liveness_wake` (the LIVENESS read, FIRST),
//!   `pane_view::PaneHandle::on_render_wake` (per-pane redraw + split/close/focus
//!   intent consumption), `tree_view::TreeHandle::on_render_wake`, and finally
//!   `shell::AppShell::on_render_wake` (the window title, the application-state
//!   snapshot, then the command drain). The shell runs LAST: a drained
//!   `close_window` tears the core down.
//! - **Application state (`state.json`):** [`build_ui`] opens a
//!   `wolf_core::StateStore` over `wolf_core::state_dir()` and turns the loaded
//!   tree into a [`crate::state_seam::RestorePlan`] BEFORE any widget exists —
//!   the working directory, the window size, and the tab's saved pane-split tree.
//!   The layout is queued on the pane view
//!   ([`crate::pane_view::PaneHandle::request_layout_rebuild`]) and realized on
//!   the first wake with a window grid, through `WolfCore::rebuild_layout`. That
//!   is ONE of the TWO blocking RPCs this front-end issues — at most once per
//!   launch, before the window is interactive, never on the crash path. The other
//!   is the buffer picker's candidate snapshot (`WolfCore::list_buffers`), which a
//!   RUNNING window can issue on Ctrl+Shift+E, behind
//!   [`crate::buffer_picker::open_decision`]'s guard that it be issued only while
//!   the core reads `Alive`. Either can mark the core `Liveness::NotResponding` and
//!   raise the stall overlay. The project-search overlay adds NEITHER a third
//!   blocking call nor a third stall path: `start_search` `spawn_blocking`s onto the
//!   tokio runtime, `open_at` is fire-and-forget, and the one blocking call it could
//!   have made — `WolfCore::ignorecase()` — is deliberately not made. Writing back
//!   is [`crate::persistence`]: a snapshot on
//!   every wake, and a final synchronous snapshot + flush as the FIRST thing
//!   [`wire_close`] does.
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
//!   seams); the app installs no draw handler of its own.
//! - **Input:** a window-level [`gtk::EventControllerKey`] routes keys through the
//!   pure [`crate::gdk_key`] seam into [`WolfCore::send_input`]; nvim routes them to
//!   its current window, and the cursor renders in whichever pane owns that grid.
//!   The controller has no propagation phase (i.e. Bubble), so it runs only after
//!   [`crate::shell`]'s CAPTURE-phase chord controller has had its say — which is
//!   exactly the guarantee that a bound chord is not also forwarded to Neovim.
//!   **POINTER input, unlike keys, is not window-level at all**: it is PER-PANE and
//!   lives entirely in [`crate::pane_view`], where each pane `DrawingArea` carries
//!   its own `gtk::GestureDrag` (press → drag → release), `gtk::EventControllerScroll`
//!   (vertical) and `gtk::EventControllerMotion` (position recording), all thin glue
//!   over the pure [`crate::mouse`] and [`crate::cell_at`] seams. `app.rs` installs
//!   no pointer controller of its own.
//! - **Commands / menu:** [`crate::shell`] installs one `gio::SimpleAction` per
//!   implemented action of [`crate::actions::ACTION_TABLE`], builds the primary
//!   menu from the same data, matches the accelerators, and drains
//!   [`WolfCore::drain_commands`] on each wake. A row activated in the command
//!   palette enters that same dispatch, and the shell keeps at most one of the
//!   four transient overlays open at a time.
//! - **Resize:** per-pane and global grid resizing live in [`crate::pane_view`]
//!   (each pane's `connect_resize` → [`crate::grid_size_for`] → per-grid/global
//!   resize).
//! - **Lifecycle:** [`WolfCore`] lives in an `Rc<RefCell<Option<_>>>` on the main
//!   thread; closing the window flushes `state.json`, then takes the core out and
//!   runs [`WolfCore::shutdown`].
//!
//! Files are opened from within Neovim (`:e`), via the picker/tree, or via the
//! primary menu's native Open File / Open Folder dialogs (there are still no save
//! / save-as dialogs). When neither the process cwd nor the restored cwd names a
//! usable project, [`crate::workspace::launch_decision`] puts the window into an
//! EMPTY STATE: the sidebar is hidden, no `FileSearch` starts, and an
//! "Open Folder…" status page covers the editor. Mouse and vertical
//! scroll are wired (see the Input bullet above). What is still deferred is THEMING
//! and a native command line as Adw widgets: the cmdline strip already paints
//! natively from the folded `Snapshot::cmdline` / `Snapshot::toast` — it never
//! paints grid 1, which under `ext_messages` (`cmdheight=0`) is empty — but it draws
//! a single row, with no `ext_popupmenu` completion popup and no multi-line
//! scrollable toast.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::glib;
use wolf_core::{CoreConfig, InputEvent, StateStore, WakeFn, WolfCore, WolfState};

use crate::buffer_picker::{self, BufferPickerHandle};
use crate::crash_overlay;
use crate::palette::{self, PaletteHandle};
use crate::pane_view;
use crate::persistence::{self, StateHandle, StateParts};
use crate::picker::{self, PickerHandle};
use crate::search_overlay::{self, SearchHandle};
use crate::shell::{self, ShellParts, GO_TO_FILE_ACTION};
use crate::state_seam::{self, RestorePlan};
use crate::title::WINDOW_TITLE_FALLBACK;
use crate::tree_view::{self, TreeHandle};
use crate::workspace;
use crate::CellMetrics;

/// The GApplication id. Adwaita uses it for resource paths, the default window
/// icon, and single-instance behavior.
const APP_ID: &str = "org.wolf.Wolf";

/// The monospace font family used for cell measurement and glyph rendering. Pinned
/// to match the deterministic golden-render font stack provisioned by the flake.
const FONT_FAMILY: &str = "DejaVu Sans Mono";

/// Default font size in points when `WOLF_FONT_PT` is unset or unparseable.
const DEFAULT_FONT_PT: f64 = 14.0;

/// Environment override for the font size in points (used by the smoke test to
/// enlarge glyphs for OCR).
const FONT_PT_ENV: &str = "WOLF_FONT_PT";

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

/// Runs the windowed `wolf-gnome` application, returning the process exit code.
#[must_use]
pub fn run() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    // The app takes no CLI args; pass an empty list so GTK does not try to parse
    // (and reject) anything from the real argv.
    app.run_with_args::<&str>(&[])
}

/// Reads the font size from `WOLF_FONT_PT`, falling back to [`DEFAULT_FONT_PT`] if
/// unset, empty, or not a positive finite number.
fn font_pt_from_env() -> f64 {
    std::env::var(FONT_PT_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|pt| pt.is_finite() && *pt > 0.0)
        .unwrap_or(DEFAULT_FONT_PT)
}

/// Measures the monospace cell metrics ONCE, using a throwaway 1x1 offscreen
/// surface purely to obtain a Pango context (its pixel size is irrelevant).
fn measure_metrics() -> CellMetrics {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)
        .expect("failed to create probe surface for font measurement");
    let cr = cairo::Context::new(&surface).expect("failed to create probe Cairo context");
    CellMetrics::for_font(&cr, FONT_FAMILY, font_pt_from_env())
}

/// Resolves the location of the in-repo `runtime/nvim/init.lua` relative to this
/// crate.
fn init_lua_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/nvim/init.lua")
}

/// Opens the application-state store over the platform state directory.
///
/// `None` — with one message, not a per-wake stream — when
/// `wolf_core::state_dir()` cannot be resolved (no `WOLF_STATE_DIR` override and
/// no home directory). Every later state operation degrades to a no-op, so the
/// app runs normally and simply neither restores nor saves.
fn open_state_store() -> Option<StateStore> {
    let Some(dir) = wolf_core::state_dir() else {
        eprintln!(
            "wolf-gnome: no state directory could be resolved; state will not be saved or restored"
        );
        return None;
    };
    Some(StateStore::open(&dir))
}

/// Builds the launch plan from the loaded state: canonicalizes the process cwd
/// and `$HOME` exactly as before, then defers every decision to the pure
/// [`state_seam::restore_plan`].
fn launch_plan(saved: Option<&WolfState>) -> RestorePlan {
    // Both paths are canonicalized so `launch_decision`'s comparisons mean what
    // they say (`Path` equality compares components; it resolves no symlinks).
    let process_cwd = std::env::current_dir()
        .ok()
        .map(|cwd| std::fs::canonicalize(&cwd).unwrap_or(cwd));
    let home = workspace::home_dir();
    state_seam::restore_plan(
        saved,
        process_cwd.as_deref(),
        home.as_deref(),
        std::path::Path::is_dir,
    )
}

/// Builds the single application window and wires it to a freshly started
/// [`WolfCore`]. If the core fails to start, logs and returns without building a
/// window (so the application simply exits).
fn build_ui(app: &adw::Application) {
    // Measure the cell once and share it (by Rc) with the draw and resize closures
    // so it is available even if a resize fires before the first draw.
    let metrics = Rc::new(measure_metrics());

    // Wake plumbing: the tokio worker fires `wake` (Send + Sync); it only pushes
    // onto the channel. Dropping a "full" send is correct — a redraw is already
    // pending. The GTK-side drain task is spawned after the window exists.
    let (wake_tx, wake_rx) = async_channel::bounded::<()>(WAKE_CHANNEL_CAP);
    let wake: WakeFn = Arc::new(move || {
        let _ = wake_tx.try_send(());
    });

    // The application-state restore prologue. `state()` is the in-memory tree the
    // store loaded from `state.json` (defaults on any failure — loading is
    // silently-ignore and never rewrites the file).
    let store = open_state_store();
    let saved = store.as_ref().map(StateStore::state);

    // Is a project open, and which one? The Linux rule is process cwd first, then
    // the restored cwd; the plan also carries the saved pane layout (only when the
    // chosen cwd IS the saved one) and the saved window size.
    let plan = launch_plan(saved.as_ref());
    let empty = plan.empty;

    let cfg = CoreConfig {
        init_lua: init_lua_path(),
        cols: INITIAL_COLS,
        rows: INITIAL_ROWS,
        cwd: plan.core_cwd.clone(),
    };
    let core = match WolfCore::start(cfg, wake) {
        Ok(core) => core,
        Err(err) => {
            eprintln!("wolf-gnome: failed to start core: {err}");
            return;
        }
    };
    // The core is non-Send; it lives only here on the main thread, shared by the
    // draw/key/resize/close closures via Rc, and taken out on close.
    let core = Rc::new(RefCell::new(Some(core)));

    // The native pane tree (one DrawingArea per pane + a cmdline strip), replacing
    // the former single editor DrawingArea. Its container is the overlay's main
    // child; the picker floats over it.
    let panes = pane_view::build(&core, &metrics);
    // Queue the saved pane tree, if any. It is realized on the first wake at
    // which a window grid exists — see `pane_view::consume_pending_layout`. NOTE
    // there is deliberately NO arity guard here: a single-leaf layout is rebuilt
    // exactly like a multi-leaf one, which is half of the single-pane fix (the
    // other half is `state_seam::persistable_layout`'s content predicate).
    if let Some(layout) = plan.layout.clone() {
        panes.request_layout_rebuild(layout);
    }
    let editor = panes.widget();
    // The cmdline strip is a focusable widget in the editor region used as the
    // focus-return target for the picker/tree (so keys keep bubbling to the
    // window-level controller after they close).
    let focus_area = panes.cmdline_widget();

    // The editor area is the overlay's main child. Overlay children stack in the
    // order they are added, and this order is a decision:
    //
    //   empty_state -> picker -> command palette -> buffer picker -> project search
    //   -> crash overlay -> restart banner
    //
    // All four transient overlays sit ABOVE the empty state, which is opaque and
    // full-bleed: below it, opening any one of them in a project-less window would
    // draw it behind the status page with no visible effect. Their order RELATIVE
    // TO EACH OTHER is not observable, because `shell::perform` keeps at most one
    // of the four open (it closes the other three when it opens one).
    //
    // The crash overlay covers all five (the picker's Open, a palette row, a
    // buffer-picker row and a search row would all send into a dead nvim; the empty
    // state's Open Folder would `Chdir` one — the captive nvim runs in the empty
    // state too, so it can crash there). The banner is topmost and click-through, so
    // it confirms a restart without ever swallowing a click meant for the editor.
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&editor));

    let empty_state = shell::build_empty_state();
    overlay.add_overlay(&empty_state);

    let crash = crash_overlay::build();

    let picker = picker::build(&overlay, &focus_area, &core);
    let palette = palette::build(&overlay, &focus_area, &core);
    let buffers = buffer_picker::build(&overlay, &focus_area, &core);
    let search = search_overlay::build(&overlay, &focus_area, &core);

    overlay.add_overlay(&crash.widget());
    overlay.add_overlay(&crash.banner_widget());

    // The file-tree sidebar on the left; the editor (overlay) on the right. In the
    // empty state there is no project to walk, so no FileSearch starts.
    let tree = tree_view::build(&core, &focus_area, !empty);
    let sidebar = tree.widget();
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_start_child(Some(&sidebar));
    paned.set_end_child(Some(&overlay));
    paned.set_position(TREE_PANE_WIDTH);

    // Header bar: an "Open" button that triggers the file picker via its action,
    // plus the GNOME primary (hamburger) menu.
    let header = adw::HeaderBar::new();
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

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
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

    // The application-state persistence glue. Built after the window (it reads
    // the window size) and before the shell (which drives it from the wake).
    let state = persistence::build(StateParts {
        core: &core,
        window: &window,
        panes: &panes,
        store,
        launch_cwd: plan.core_cwd.clone(),
        empty,
    });

    // The action namespace (win.* / app.*), the accelerators, and the command
    // drain. Actions must exist before `present()` so the menu and the header
    // button resolve them.
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
        empty_state: &empty_state,
        open_button: &open_button,
        crash_overlay: &crash,
        panes: &panes,
        state: &state,
    });
    shell.install_actions(app);
    shell.set_empty_state(empty);

    // The header Open button activates the same action the Ctrl+Shift+O
    // accelerator does. Explicit activation (rather than the `action-name`
    // property) sidesteps the ordering footgun where a widget naming an
    // as-yet-uninstalled action renders insensitive.
    {
        let window = window.clone();
        open_button.connect_clicked(move |_button| {
            // Qualified because `adw::ApplicationWindow` implements BOTH
            // `gio::ActionGroup` (whose `activate_action` takes a bare, unprefixed
            // name and returns nothing) and `gtk::Widget` (whose takes the full
            // `win.`-prefixed name and reports failure). We want the latter.
            if let Err(err) =
                gtk::prelude::WidgetExt::activate_action(&window, GO_TO_FILE_ACTION, None)
            {
                eprintln!("wolf-gnome: failed to activate {GO_TO_FILE_ACTION}: {err}");
            }
        });
    }

    // Drain the wake channel on the main thread. Each signal first asks the shell
    // to read the captive nvim's LIVENESS (`on_liveness_wake`), which drives the
    // crash/stall overlay, schedules an automatic restart for a dead core, and
    // reports whether painting this wake is worth anything at all. Only then does
    // the pane view react (create/split/close panes, consume layout intents,
    // redraw) and the tree react to a current-file change carried by the same
    // wake. The shell's command drain runs LAST and UNGATED: a drained
    // `close_window` shuts the core down, the other two must have had their turn
    // first, and a `close_window` queued just before a crash must not be stranded
    // behind the overlay.
    {
        let panes = panes.clone();
        let tree = tree.clone();
        let shell = shell.clone();
        glib::spawn_future_local(async move {
            while wake_rx.recv().await.is_ok() {
                let paint = shell.on_liveness_wake();
                if paint {
                    panes.on_render_wake();
                    tree.on_render_wake();
                }
                shell.on_render_wake();
            }
        });
    }
    // React to whatever is already there (the initial attached frame), through the
    // same liveness gate as every later wake.
    if shell.on_liveness_wake() {
        panes.on_render_wake();
    }

    wire_keys(&window, &core, &picker, &palette, &buffers, &search);
    wire_close(CloseParts {
        window: &window,
        core: &core,
        picker: &picker,
        palette: &palette,
        buffers: &buffers,
        search: &search,
        tree: &tree,
        state: &state,
    });

    // The shell's first drain runs only AFTER `wire_close` has installed the
    // `close-request` handler. A drained `CloseWindow` calls `window.close()`,
    // which is a no-op teardown-wise until that handler exists — it would tear the
    // window down without shutting the core down, orphaning the captive nvim. The
    // queue is empty at this point today (commands are user-triggered only, so a
    // freshly attached core has none), so this ordering costs nothing and removes
    // the hazard.
    shell.on_render_wake();

    window.present();
    // Focus the cmdline strip so key events have a focused target in the editor
    // region to bubble from to the window-level key controller.
    focus_area.grab_focus();
}

/// Installs the key controller. While any transient overlay (the file picker, the
/// command palette, the buffer picker or the project search) is open, keys are left
/// for that overlay's focused widgets (forwarding to the core is suppressed).
/// Otherwise every key is translated via the pure [`crate::gdk_key`] seam into core
/// input. No smoke discriminates it — see "What is still unproven" in
/// `crates/wolf-gnome/tests/README.md`.
///
/// The controller carries NO propagation phase (i.e. Bubble), so a chord Wolf
/// binds — Ctrl+Shift+O opens the picker, Ctrl+Shift+P the palette, Ctrl+Shift+E
/// the buffer picker, Ctrl+Shift+F the project search — never reaches it:
/// [`crate::shell`]'s chord controller matches it in the CAPTURE phase and stops
/// propagation. That is deliberate; it is what guarantees a bound chord is not ALSO
/// forwarded to Neovim.
fn wire_keys(
    window: &adw::ApplicationWindow,
    core: &Rc<RefCell<Option<WolfCore>>>,
    picker: &PickerHandle,
    palette: &PaletteHandle,
    buffers: &BufferPickerHandle,
    search: &SearchHandle,
) {
    let core = core.clone();
    let picker = picker.clone();
    let palette = palette.clone();
    let buffers = buffers.clone();
    let search = search.clone();
    let controller = gtk::EventControllerKey::new();
    controller.connect_key_pressed(move |_controller, keyval, _keycode, state| {
        // An overlay is open: let its focused widgets handle the key; never forward
        // to nvim/core.
        if picker.is_open() || palette.is_open() || buffers.is_open() || search.is_open() {
            return glib::Propagation::Proceed;
        }
        match crate::gdk_key(keyval, state) {
            Some(event) => {
                if let Some(core) = core.borrow().as_ref() {
                    core.send_input(InputEvent::Key(event));
                }
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        }
    });
    window.add_controller(controller);
}

/// The handles [`wire_close`] needs, as one struct rather than eight positional
/// parameters (`clippy::too_many_arguments` bites at seven), following the
/// [`ShellParts`] / [`StateParts`] precedent.
struct CloseParts<'a> {
    /// The window whose `close-request` is being handled.
    window: &'a adw::ApplicationWindow,
    /// The live core, taken out and shut down by the handler.
    core: &'a Rc<RefCell<Option<WolfCore>>>,
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
    /// The application-state persistence glue, flushed FIRST.
    state: &'a StateHandle,
}

/// Installs the close handler: persist the session, then take the core out
/// (releasing the RefCell borrow in its own statement) and shut it down cleanly.
///
/// **The `state.flush_on_close()` call is the FIRST statement in the handler, and
/// that ordering is load-bearing.** It must run while the core and the panes are
/// still alive: a snapshot taken after `core.borrow_mut().take()` sees no core,
/// `state_seam::may_snapshot` refuses it, and the session is silently not saved.
/// Moving it below any of the three statements that follow would leave the
/// relaunch smokes (`pane_layout_survives_relaunch_under_cage`,
/// `single_pane_file_survives_relaunch_under_cage`) with an empty `state.json`.
fn wire_close(parts: CloseParts<'_>) {
    let core = parts.core.clone();
    let picker = parts.picker.clone();
    let palette = parts.palette.clone();
    let buffers = parts.buffers.clone();
    let search = parts.search.clone();
    let tree = parts.tree.clone();
    let state = parts.state.clone();
    parts.window.connect_close_request(move |_window| {
        // FIRST, while the core and the panes are still alive. Synchronous
        // snapshot + blocking flush; every core borrow it takes is released
        // before it returns, so the `borrow_mut().take()` below cannot panic.
        state.flush_on_close();
        // Close the picker, the project search and the tree first so their
        // background work is dropped before the core shuts down: the picker's and
        // the tree's `FileSearch`es cancel an in-flight traversal, and the search
        // overlay's `SearchSession` kills its `rg` child. Neither the palette nor
        // the buffer picker holds either, so their closes are teardown symmetry
        // rather than cancellation — and all five are idempotent.
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
        glib::Propagation::Proceed
    });
}

#[cfg(test)]
mod tests {
    use super::WAKE_CHANNEL_CAP;

    // The wake bridge's SAFETY ARGUMENT, made executable.
    //
    // The bounded wake channel silently DROPS a wake fired while it is full, and
    // `wolf_core::liveness::mark_dead` fires exactly once — after which a dead
    // Neovim produces no further redraw wakes at all. So the crash overlay only
    // ever appears because of this implication: a dropped wake means an unconsumed
    // one is still queued, which means a full handler run WILL happen after the
    // drop, and because `mark_dead` stores `Dead` BEFORE waking and liveness is a
    // level-triggered atomic read, that later run observes the death. These tests
    // pin the transport half of that argument against a future change to
    // `async-channel`'s bounded semantics (e.g. a ring buffer that drops the OLD
    // item instead of refusing the new one), which would silently invalidate it.
    //
    // They deliberately read the production `WAKE_CHANNEL_CAP` rather than a
    // literal `1`, so raising the capacity cannot quietly turn them into tests of
    // something else.

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

        // `recv()` resolves over `try_recv`, which pops the item BEFORE the future
        // resolves — so the drain loop's handler body runs against an already-empty
        // slot, and a wake fired DURING a handler run (the `mark_dead` case that
        // matters most) is accepted rather than dropped.
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
