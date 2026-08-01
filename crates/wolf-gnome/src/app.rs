//! The windowed `wolf-gnome` application: all GTK4/Adwaita window wiring lives here.
//!
//! [`run`] builds an [`adw::Application`], and on `activate` constructs an
//! [`adw::ApplicationWindow`] containing an [`adw::HeaderBar`] over a horizontal
//! [`gtk::Paned`] of the file-tree sidebar ([`crate::tree_view`]) and the editor
//! [`gtk::Overlay`]. The overlay's main child is the **native pane view**
//! ([`crate::pane_view`]) — a tree of nested [`gtk::Paned`]s with one
//! [`gtk::DrawingArea`] per pane plus a bottom cmdline strip — and its overlay
//! child is the file picker ([`crate::picker`]). The window is backed by a live
//! [`WolfCore`]:
//!
//! - **Output / wake:** [`WolfCore`] runs Neovim on its own tokio runtime and
//!   signals a single [`WakeFn`] when a new frame is ready. The wake fires on a
//!   tokio worker thread, so it cannot touch GTK directly; instead it pushes onto a
//!   bounded `async-channel` (capacity 1, coalescing a burst of flushes), and a
//!   [`glib::spawn_future_local`] task on the GTK main thread drains it and calls
//!   `pane_view::PaneHandle::on_render_wake` (per-pane redraw + split/close/focus
//!   intent consumption) and `tree_view::TreeHandle::on_render_wake`.
//! - **Paint:** per-pane painting and the mode-aware cursor live in
//!   [`crate::pane_view`] (which uses the pure [`crate::paint`]/[`crate::cursor`]
//!   seams); the app installs no draw handler of its own.
//! - **Input:** a window-level [`gtk::EventControllerKey`] routes keys through the
//!   pure [`crate::gdk_key`] seam into [`WolfCore::send_input`]; nvim routes them to
//!   its current window, and the cursor renders in whichever pane owns that grid.
//! - **Resize:** per-pane and global grid resizing live in [`crate::pane_view`]
//!   (each pane's `connect_resize` → [`crate::grid_size_for`] → per-grid/global
//!   resize).
//! - **Lifecycle:** [`WolfCore`] lives in an `Rc<RefCell<Option<_>>>` on the main
//!   thread; closing the window takes it out and runs [`WolfCore::shutdown`].
//!
//! Files are opened from within Neovim (`:e`) or via the picker/tree; there are no
//! file dialogs. Native command line (`ext_cmdline`/`ext_messages`), theming, and
//! mouse/scroll are deferred (the cmdline strip paints grid 1 in the meantime).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::glib;
use wolf_core::{CoreConfig, InputEvent, WakeFn, WolfCore};

use crate::pane_view;
use crate::picker::{self, PickerHandle};
use crate::tree_view::{self, TreeHandle};
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
/// position).
const TREE_PANE_WIDTH: i32 = 250;

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

    let cfg = CoreConfig {
        init_lua: init_lua_path(),
        cols: INITIAL_COLS,
        rows: INITIAL_ROWS,
        cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
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
    let editor = panes.widget();
    // The cmdline strip is a focusable widget in the editor region used as the
    // focus-return target for the picker/tree (so keys keep bubbling to the
    // window-level controller after they close).
    let focus_area = panes.cmdline_widget();

    // The editor area is the overlay's main child; the picker is added as an
    // overlay child (built below) so it floats over the editor when open.
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&editor));

    let picker = picker::build(&overlay, &focus_area, &core);

    // The file-tree sidebar on the left; the editor (overlay) on the right.
    let tree = tree_view::build(&core, &focus_area);
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_start_child(Some(&tree.widget()));
    paned.set_end_child(Some(&overlay));
    paned.set_position(TREE_PANE_WIDTH);

    // Header bar with an "Open" button that opens the picker.
    let header = adw::HeaderBar::new();
    let open_button = gtk::Button::builder()
        .icon_name("document-open-symbolic")
        .tooltip_text("Open file (Ctrl+Shift+P)")
        .build();
    {
        let picker = picker.clone();
        open_button.connect_clicked(move |_button| {
            picker.open();
        });
    }
    header.pack_start(&open_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&paned);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Wolf")
        .default_width(WINDOW_WIDTH)
        .default_height(WINDOW_HEIGHT)
        .content(&content)
        .build();

    // Drain the wake channel on the main thread. Each signal lets the pane view
    // react (create/split/close panes, consume layout intents, redraw) and the tree
    // react to a current-file change carried by the same wake.
    {
        let panes = panes.clone();
        let tree = tree.clone();
        glib::spawn_future_local(async move {
            while wake_rx.recv().await.is_ok() {
                panes.on_render_wake();
                tree.on_render_wake();
            }
        });
    }
    // React to whatever is already there (the initial attached frame).
    panes.on_render_wake();

    wire_keys(&window, &core, &picker);
    wire_close(&window, &core, &picker, &tree);

    window.present();
    // Focus the cmdline strip so key events have a focused target in the editor
    // region to bubble from to the window-level key controller.
    focus_area.grab_focus();
}

/// Installs the key controller. While the picker is open, keys are left for the
/// focused picker widgets (forwarding to the core is suppressed). Otherwise the
/// trigger chord (Ctrl+Shift+P) opens the picker, and any other key is translated
/// via the pure [`crate::gdk_key`] seam into core input.
fn wire_keys(
    window: &adw::ApplicationWindow,
    core: &Rc<RefCell<Option<WolfCore>>>,
    picker: &PickerHandle,
) {
    let core = core.clone();
    let picker = picker.clone();
    let controller = gtk::EventControllerKey::new();
    controller.connect_key_pressed(move |_controller, keyval, _keycode, state| {
        // Picker open: let the focused picker widgets handle the key; never forward
        // to nvim/core.
        if picker.is_open() {
            return glib::Propagation::Proceed;
        }
        // Trigger chord: open the picker and consume the key.
        if crate::is_picker_trigger(keyval, state) {
            picker.open();
            return glib::Propagation::Stop;
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

/// Installs the close handler: take the core out (releasing the RefCell borrow in
/// its own statement) and shut it down cleanly.
fn wire_close(
    window: &adw::ApplicationWindow,
    core: &Rc<RefCell<Option<WolfCore>>>,
    picker: &PickerHandle,
    tree: &TreeHandle,
) {
    let core = core.clone();
    let picker = picker.clone();
    let tree = tree.clone();
    window.connect_close_request(move |_window| {
        // Close the picker and tree first so their FileSearches are dropped
        // (cancelling any in-flight traversal) before the core shuts down.
        picker.close();
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
