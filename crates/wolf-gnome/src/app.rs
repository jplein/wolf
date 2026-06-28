//! The windowed `wolf-gnome` application: all GTK4/Adwaita wiring lives here.
//!
//! [`run`] builds an [`adw::Application`], and on `activate` constructs a single
//! [`adw::ApplicationWindow`] containing an [`adw::HeaderBar`] and a
//! [`gtk::DrawingArea`]. The window is backed by a live [`WolfCore`]:
//!
//! - **Output:** [`WolfCore`] runs Neovim on its own tokio runtime and signals a
//!   single [`WakeFn`] when a new frame is ready. The wake fires on a tokio worker
//!   thread, so it cannot touch GTK directly; instead it pushes onto a bounded
//!   `async-channel` (capacity 1, which coalesces a burst of flushes), and a
//!   [`glib::spawn_future_local`] task running on the GTK main thread drains it and
//!   calls [`gtk::DrawingArea::queue_draw`].
//! - **Paint:** the draw func reads [`WolfCore::read_snapshot`] and hands it to the
//!   pure [`crate::paint`] seam, then draws a mode-aware cursor (an opaque block
//!   in normal mode, a thin bar in insert/replace) via the pure [`crate::cursor`]
//!   helpers, inverting the underlying cell so it stays reliably visible.
//! - **Input:** a [`gtk::EventControllerKey`] routes keys through the pure
//!   [`crate::gdk_key`] seam into [`WolfCore::send_input`].
//! - **Resize:** the drawing area's resize maps pixels to a grid size via the pure
//!   [`crate::grid_size_for`] seam and calls [`WolfCore::resize`].
//! - **Lifecycle:** [`WolfCore`] lives in an `Rc<RefCell<Option<_>>>` on the main
//!   thread; closing the window takes it out and runs [`WolfCore::shutdown`].
//!
//! Files are opened from within Neovim (`:e`); there are no file dialogs. Theming,
//! mouse/scroll, and a native command line are deferred.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::glib;
use wolf_core::{CoreConfig, CursorShape, InputEvent, Snapshot, WakeFn, WolfCore};

use crate::picker::{self, PickerHandle};
use crate::render::draw_glyph;
use crate::{cursor, grid_size_for, paint, CellMetrics};

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

/// Default window size in pixels.
const WINDOW_WIDTH: i32 = 960;
/// Default window height in pixels (see [`WINDOW_WIDTH`]).
const WINDOW_HEIGHT: i32 = 640;

/// Wake channel capacity. One slot is enough: it coalesces a burst of flushes into
/// a single queued redraw (a dropped "full" send just means a redraw is already
/// pending).
const WAKE_CHANNEL_CAP: usize = 1;

/// Divisor to convert an 8-bit color channel (0–255) to Cairo's 0.0–1.0 range.
const RGB_MAX: f64 = 255.0;

/// Runs the windowed `wolf-gnome` application, returning the process exit code.
#[must_use]
pub fn run() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();
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

    // Widgets: header bar over a drawing area in a vertical box.
    let area = gtk::DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .focusable(true)
        .build();

    // The editor area is the overlay's main child; the picker is added as an
    // overlay child (built below) so it floats over the editor when open.
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&area));

    let picker = picker::build(&overlay, &area, &core);

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
    content.append(&overlay);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Wolf")
        .default_width(WINDOW_WIDTH)
        .default_height(WINDOW_HEIGHT)
        .content(&content)
        .build();

    // Drain the wake channel on the main thread, redrawing on each frame signal.
    {
        let area = area.clone();
        glib::spawn_future_local(async move {
            while wake_rx.recv().await.is_ok() {
                area.queue_draw();
            }
        });
    }
    // Paint whatever is already there (the initial blank/attached frame).
    area.queue_draw();

    wire_draw(&area, &metrics, &core);
    wire_keys(&window, &core, &picker);
    wire_resize(&area, &metrics, &core);
    wire_close(&window, &core, &picker);

    window.present();
    // Focus the drawing area so key events have a focused target to bubble from.
    area.grab_focus();
}

/// Installs the draw function: paint the snapshot, then draw the cursor.
fn wire_draw(area: &gtk::DrawingArea, metrics: &Rc<CellMetrics>, core: &Rc<RefCell<Option<WolfCore>>>) {
    let metrics = metrics.clone();
    let core = core.clone();
    area.set_draw_func(move |_area, cr, _width, _height| {
        let core_ref = core.borrow();
        let Some(core) = core_ref.as_ref() else {
            return;
        };
        // Keep the read guard's scope short: read, paint, then drop before return.
        let snap = core.read_snapshot();
        paint(cr, &snap, &metrics);
        draw_cursor(cr, &snap, &metrics);
    });
}

/// Draws the mode-aware cursor at the snapshot's cursor cell.
///
/// The shape follows the current mode's [`CursorShape`] (block in normal mode, a
/// thin vertical/horizontal bar otherwise). The fill is opaque and contrasting:
/// [`cursor::cursor_colors`] honors a well-defined cursor highlight or inverts the
/// underlying cell. For a block cursor the cell's glyph is redrawn in the inverted
/// color so it reads against the fill; bar cursors leave the glyph untouched.
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

    // A block cursor covers the glyph, so redraw it inverted; a thin bar leaves
    // the glyph visible and needs no redraw.
    if shape == CursorShape::Block {
        if let Some(cell) = snap.grid.cell(snap.cursor.row, snap.cursor.col) {
            if !cell.text.is_empty() {
                let style = snap.highlights.resolve(cell.hl_id);
                draw_glyph(cr, metrics, cell_x, cell_y, &cell.text, glyph, &style);
            }
        }
    }
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

/// Installs the resize handler: pixels -> grid size -> [`WolfCore::resize`].
fn wire_resize(area: &gtk::DrawingArea, metrics: &Rc<CellMetrics>, core: &Rc<RefCell<Option<WolfCore>>>) {
    let metrics = metrics.clone();
    let core = core.clone();
    area.connect_resize(move |_area, width, height| {
        let (cols, rows) = grid_size_for(width, height, metrics.cell_w, metrics.cell_h);
        if let Some(core) = core.borrow().as_ref() {
            core.resize(cols, rows);
        }
    });
}

/// Installs the close handler: take the core out (releasing the RefCell borrow in
/// its own statement) and shut it down cleanly.
fn wire_close(
    window: &adw::ApplicationWindow,
    core: &Rc<RefCell<Option<WolfCore>>>,
    picker: &PickerHandle,
) {
    let core = core.clone();
    let picker = picker.clone();
    window.connect_close_request(move |_window| {
        // Close the picker first so its FileSearch is dropped (cancelling any
        // in-flight traversal) before the core shuts down.
        picker.close();
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
