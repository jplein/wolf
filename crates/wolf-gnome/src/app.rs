//! The `wolf-gnome` application entry point: activate, and delegate.
//!
//! [`run`] builds an [`adw::Application`] and does nothing else itself. The FIRST
//! `activate` builds the process-scoped [`crate::coordinator::Coordinator`],
//! installs the `app.`-namespace actions on it, and asks it to restore the saved
//! session — one window per saved window, or exactly one empty-state window when
//! there is nothing to restore. Any LATER `activate` — a second launch reaching
//! the running primary instance over D-Bus — opens one fresh window instead.
//!
//! Everything a window is lives in [`mod@crate::window`]; everything the process
//! owns exactly once lives in [`mod@crate::coordinator`], which calls the five
//! helpers below. The split is `crates/wolf-gnome/DECISIONS.md`'s to justify.
//!
//! The coordinator is held in an `Rc<RefCell<Option<_>>>` captured by the
//! `connect_activate` closure rather than in a `static`: this crate has no
//! process-global APPLICATION state, and keeping it that way is what makes two
//! `WolfCore`s in one process independent. The one carve-out is
//! [`mod@crate::frame_metrics`], whose diagnostic counters no application
//! behaviour reads.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use wolf_core::{StateStore, WolfConfig};

use crate::config_seam;
use crate::coordinator::{self, Coordinator};
use crate::frame_metrics;
use crate::init_lua;
use crate::ready_seam;
use crate::CellMetrics;

/// The GApplication id. Adwaita uses it for resource paths, the default window
/// icon, and single-instance behavior.
const APP_ID: &str = "org.wolf.Wolf";

/// Environment override for the font size in points (used by the smoke test to
/// enlarge glyphs for OCR). It WINS over the configured `font-size`; the
/// precedence rule and its reason live in [`config_seam::resolve_font_pt`].
const FONT_PT_ENV: &str = "WOLF_FONT_PT";

/// Runs the windowed `wolf-gnome` application, returning the process exit code.
#[must_use]
pub fn run() -> glib::ExitCode {
    // FIRST, because every other `frame_metrics` entry point branches on the
    // once-only read this forces. A no-op beyond one environment read when
    // `WOLF_FRAME_STATS` is unset.
    frame_metrics::install();
    let app = adw::Application::builder().application_id(APP_ID).build();
    // Built on the first activate, not here: `load_config` and `measure_metrics`
    // both need a running GTK, and `open_state_store` must not run in a process
    // that turns out to be a remote instance.
    let coordinator: Rc<RefCell<Option<Coordinator>>> = Rc::new(RefCell::new(None));
    app.connect_activate(move |app| activate(app, &coordinator));
    // The app takes no CLI args; pass an empty list so GTK does not try to parse
    // (and reject) anything from the real argv.
    let code = app.run_with_args::<&str>(&[]);
    // The one non-periodic flush, covering the final interval of a CLEAN quit. A
    // process killed by a signal never reaches it, which is why every report is
    // appended to the file as it is emitted.
    frame_metrics::flush_final();
    code
}

/// One `activate`: build the coordinator and restore the session the first time,
/// open a fresh window every time after.
///
/// The second-activate path is the single-instance behaviour a desktop gives
/// "launch it again" — unreachable from the cage smokes, which strip
/// `DBUS_SESSION_BUS_ADDRESS`, so a second process there becomes its own primary
/// instance rather than sending `activate`.
fn activate(app: &adw::Application, cell: &Rc<RefCell<Option<Coordinator>>>) {
    // Cloned out in its own statement so no borrow is live across the calls
    // below, which build windows and can re-enter nothing but must not be able
    // to.
    let existing = cell.borrow().clone();
    if let Some(coordinator) = existing {
        coordinator.open_new_window(app);
        return;
    }
    let coordinator = coordinator::build();
    *cell.borrow_mut() = Some(coordinator.clone());
    // Before any window exists, so the primary menu and the chord controller
    // resolve `app.quit` from the first window's first frame.
    coordinator.install_app_actions(app);
    coordinator.restore_session(app);
}

/// The raw `WOLF_FONT_PT` value, parsed but NOT validated — the sole environment
/// read on this path.
///
/// Every rule about what a usable override is lives in
/// [`config_seam::resolve_font_pt`], so it can be tested without touching the
/// process environment.
fn font_pt_override() -> Option<f64> {
    std::env::var(FONT_PT_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
}

/// Loads the user configuration, mirroring `wolf_ffi_load_config`'s resolution so
/// both front-ends read the same file the same way.
///
/// Degrades with ONE message, never a per-wake stream: an unresolvable
/// preferences directory yields the built-in defaults and the app runs normally.
/// `load_or_create` itself is infallible and CREATES the file when it is absent,
/// which is why every smoke pins `WOLF_CONFIG_DIR`.
#[must_use]
pub(crate) fn load_config() -> WolfConfig {
    let Some(dir) = wolf_core::preferences_dir() else {
        eprintln!(
            "wolf-gnome: no preferences directory could be resolved; using the built-in defaults"
        );
        return wolf_core::default_config();
    };
    wolf_core::load_or_create(&dir)
}

/// Measures the monospace cell metrics ONCE from `config`, using a throwaway 1x1
/// offscreen surface purely to obtain a Pango context (its pixel size is
/// irrelevant).
#[must_use]
pub(crate) fn measure_metrics(config: &WolfConfig) -> CellMetrics {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)
        .expect("failed to create probe surface for font measurement");
    let cr = cairo::Context::new(&surface).expect("failed to create probe Cairo context");
    let pt = config_seam::resolve_font_pt(font_pt_override(), config.font_size);
    CellMetrics::for_font(&cr, &config.font_family, pt, config.line_height)
}

/// Resolves the captive `runtime/nvim/init.lua` this process launches Neovim with.
///
/// GLUE only: the three environment and filesystem reads live here, and the RULE
/// that orders them lives in [`init_lua::resolve_init_lua`] — the same neighbourhood
/// convention [`FONT_PT_ENV`] and [`config_seam::resolve_font_pt`] follow.
///
/// The `CARGO_MANIFEST_DIR` fallback is what keeps `cargo run` and every compositor
/// smoke working, so it is passed unconditionally rather than guarded.
///
/// Under `wrapGAppsHook4` the installed `$out/bin/wolf-gnome` is a wrapper that
/// `exec`s `$out/bin/.wolf-gnome-wrapped`, and `current_exe()` reads
/// `/proc/self/exe` — so the parent directory is still `$out/bin` and the
/// `../share/…` join still resolves. A wrapper does not break the middle branch; do
/// not "fix" this by reaching for `argv[0]`.
#[must_use]
pub(crate) fn init_lua_path() -> PathBuf {
    let env_override = std::env::var(init_lua::INIT_LUA_ENV).ok();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let manifest_fallback =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/nvim/init.lua");
    init_lua::resolve_init_lua(
        env_override.as_deref(),
        exe_dir.as_deref(),
        &manifest_fallback,
        |p: &Path| p.exists(),
    )
}

/// The test-only readiness file `WOLF_READY_FILE` names, or `None` when it names
/// none — which is the shipped default.
///
/// GLUE only, shaped like [`init_lua_path`]: the environment read lives here and
/// the rule lives in [`ready_seam::ready_file_path`]. Called once per process, by
/// [`crate::coordinator::build`].
#[must_use]
pub(crate) fn ready_file_path() -> Option<PathBuf> {
    let raw = std::env::var(ready_seam::READY_FILE_ENV).ok();
    ready_seam::ready_file_path(raw.as_deref()).map(PathBuf::from)
}

/// Opens the application-state store over the platform state directory.
///
/// `None` — with one message, not a per-wake stream — when
/// `wolf_core::state_dir()` cannot be resolved (no `WOLF_STATE_DIR` override and
/// no home directory). Every later state operation degrades to a no-op, so the
/// app runs normally and simply neither restores nor saves.
///
/// Called exactly ONCE per process, by [`crate::coordinator::build`]: two stores
/// over one directory silently clobber each other's tree.
#[must_use]
pub(crate) fn open_state_store() -> Option<StateStore> {
    let Some(dir) = wolf_core::state_dir() else {
        eprintln!(
            "wolf-gnome: no state directory could be resolved; state will not be saved or restored"
        );
        return None;
    };
    Some(StateStore::open(&dir))
}
