//! The GTK GLUE for the `wolf-gnome` action namespace: `gio::SimpleAction`s, the
//! GNOME primary menu, the accelerator registration, the empty state, and the
//! command drain.
//!
//! Everything this module does is driven by data it does not own:
//! [`crate::actions::ACTION_TABLE`] declares the action names, accelerators and
//! their `Command` correspondence; [`crate::actions::MENU_SECTIONS`] declares the
//! menu; [`crate::command_action::ui_action_for`] decides what each drained
//! command means; [`crate::workspace`] decides when a pending re-root has settled.
//! Because the menu, the accelerators and the drained commands all resolve through
//! the same tables, they cannot drift apart.
//!
//! ## Two shell entry points per wake
//!
//! [`AppShell::on_liveness_wake`] runs FIRST on every wake — before the pane view
//! and the tree — and [`AppShell::on_render_wake`] runs LAST. They are not
//! interchangeable:
//!
//! - [`AppShell::on_liveness_wake`] reads the captive nvim's
//!   [`wolf_core::Liveness`] ONCE, applies the pure [`crate::liveness_ui()`]
//!   mapping, drives the crash/stall overlay, schedules an automatic restart for a
//!   `Dead` core, and RETURNS whether the pane view and the tree should paint at
//!   all. It must run before any handler that can early-return: liveness is
//!   LEVEL-triggered (an atomic read), and putting the read behind
//!   `pane_view::on_render_wake`'s `core.is_none()` guard would reintroduce an
//!   edge-triggered swallow.
//! - [`AppShell::on_render_wake`] settles a pending re-root, refreshes the window
//!   TITLE from the live cwd ([`AppShell::refresh_title`] over the pure
//!   [`crate::title`] seam), asks [`crate::persistence`] for an application-state
//!   SNAPSHOT, and then drains [`wolf_core::WolfCore::drain_commands`] and
//!   dispatches each command. A command `wolf-gnome` does not implement is
//!   PRINTED with its recorded reason — never silently dropped. The drain runs
//!   LAST because a drained `close_window` shuts the core down, and the state
//!   snapshot must happen before that. The title refresh and the snapshot run
//!   UNGATED like the drain: both are cheap, neither issues an RPC, and the
//!   snapshot's own `state_seam::may_snapshot` gate handles the non-`Alive` case.
//!
//! The drain keeps running in ALL THREE liveness states — a deliberate divergence
//! from macOS, whose `onWake` `return`s before its command drain. On Linux the
//! drain is the ONLY path by which a `close_window` / `quit_application` reaches
//! `window.close()`, so a command queued in the instant before a crash would
//! otherwise be stranded behind the crash overlay with no way out but the window
//! manager. The drain issues no RPC and paints nothing, so running it costs
//! nothing.
//!
//! ## The restart orchestration
//!
//! [`AppShell::restart_nvim`] is the ONE path both the automatic (`Dead`) restart
//! and the overlay button take. It captures the Tier-1 open-file mirror BEFORE
//! calling [`wolf_core::WolfCore::restart_nvim`] (a respawn wipes the mirror),
//! then on an actual respawn resets the pane view, reopens the captured files with
//! the ACTIVE one LAST, hides the overlay and flashes the banner. Automatic
//! attempts are capped at [`crate::liveness_ui::MAX_AUTO_RESTART_ATTEMPTS`] per
//! crash; an explicit button press RESETS that counter, because the cap exists to
//! stop a retry loop, never to refuse a user.
//!
//! ## Three entry paths, one namespace
//!
//! A command that arrives from inside Neovim (`:WolfClosePane`, `require('wolf')`,
//! `:q`/`:qa`) goes through the drain. A chord or a menu click activates the
//! matching `gio::SimpleAction` directly, without a round trip through the core's
//! command queue. A row activated in the command-palette overlay
//! ([`crate::palette`]) hands its `Command` straight to
//! [`AppShell::dispatch_command`], which is the same function the drain loop calls.
//! All three end in [`UiAction`] and in [`AppShell::perform`], so they cannot
//! diverge.
//!
//! ## Borrow discipline (load-bearing)
//!
//! `drain_and_dispatch_commands` takes a SHORT `core.borrow()`, collects the
//! commands into a `Vec`, and drops the borrow before dispatching any of them.
//! `UiAction::CloseWindow` / `UiAction::QuitApplication` synchronously run
//! `app::wire_close`'s `close-request` handler, which does `core.borrow_mut()
//! .take()`; an outstanding `borrow()` there is a `RefCell` panic, not a warning.
//!
//! The restart path adds a SECOND rule: **never hold a snapshot read guard across
//! `open_files_for_tabpage`**. [`AppShell::capture_open_files`] binds
//! `Snapshot::current_tabpage` to a local in its own statement so the
//! `SnapshotReadGuard` is dropped before the mirror read takes the same
//! `parking_lot` lock again (the `api.rs` `pane_status` precedent: a re-entrant
//! read can deadlock behind a queued writer).
//!
//! And a THIRD, which is about liveness rather than borrows but fails just as
//! loudly: **never call `WolfCore::current_tabpage()` on the restart path.** It is
//! a `block_on_rpc`, and against a dead nvim it freezes the GTK main loop for the
//! whole RPC timeout budget on every single crash. The cheap, RPC-free
//! `Snapshot::current_tabpage` field carries the same handle.
//!
//! Exactly TWO `block_on_rpc` call sites are reachable from this crate, and the
//! restart path issues neither. The first is `WolfCore::rebuild_layout`, called at
//! most once per launch from `pane_view::apply_layout_rebuild`, before the window
//! is interactive. The second is `WolfCore::list_buffers`, the buffer picker's
//! candidate snapshot — and it is reachable from a RUNNING application, which is
//! what makes `Liveness::NotResponding` a state a user can actually reach here.
//! [`crate::buffer_picker::open_decision`] guards it: the picker refuses to open
//! unless the core reads `Alive`, so the snapshot can raise a stall ONCE and can
//! never re-probe a core already known to be wedged.
//!
//! The project-search overlay ([`crate::search_overlay`]) adds NO third one, and
//! that is worth stating because the symmetry with the buffer picker suggests
//! otherwise: `start_search` resolves its root from the tracked-cwd cell and
//! `spawn_blocking`s, `open_at` is fire-and-forget on the ordered input channel, and
//! `liveness` is an atomic read. `WolfCore::ignorecase()` IS a `block_on_rpc` and is
//! deliberately never called from this crate.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};

use wolf_core::{Command, InputEvent, Liveness, TabFiles, WolfCore};

use crate::actions::{self, BindingStatus};
use crate::app::TREE_PANE_WIDTH;
use crate::buffer_picker::BufferPickerHandle;
use crate::command_action::{ui_action_for, UiAction};
use crate::crash_overlay::CrashOverlay;
use crate::liveness_ui::{
    liveness_ui, reopen_order, restart_outcome, OverlayMode, RestartOutcome,
    MAX_AUTO_RESTART_ATTEMPTS,
};
use crate::palette::PaletteHandle;
use crate::pane_view::PaneHandle;
use crate::persistence::StateHandle;
use crate::picker::PickerHandle;
use crate::search_overlay::SearchHandle;
use crate::title;
use crate::tree_view::TreeHandle;
use crate::workspace;

/// The action the empty state's button and the primary menu use to pick a folder.
const OPEN_FOLDER_ACTION: &str = "win.open-folder";

/// The action that opens the in-window file picker (disabled in the empty state).
/// Also the header bar's Open button target.
pub(crate) const GO_TO_FILE_ACTION: &str = "win.go-to-file";

/// The action that opens the in-window project-search overlay (disabled in the
/// empty state, where the search root would fall back to the home directory).
const SEARCH_IN_PROJECT_ACTION: &str = "win.search-in-project";

/// Title of the empty state's status page.
const EMPTY_STATE_TITLE: &str = "No folder open";

/// Description under the empty state's title.
const EMPTY_STATE_DESCRIPTION: &str = "Open a folder to start editing.";

/// Label of the empty state's button.
const EMPTY_STATE_BUTTON: &str = "Open Folder…";

/// Icon of the empty state's status page.
const EMPTY_STATE_ICON: &str = "folder-symbolic";

/// Title of the folder-selection dialog.
const OPEN_FOLDER_DIALOG_TITLE: &str = "Open Folder";

/// Title of the file-selection dialog.
const OPEN_FILE_DIALOG_TITLE: &str = "Open File";

/// How long [`AppShell::schedule_restart`] waits before running the (synchronous,
/// main-loop-blocking) respawn, so the crash overlay is actually PRESENTED first.
///
/// **A `glib::idle_add_local_once` is NOT sufficient here, and this was measured
/// rather than reasoned about.** The tempting argument — that an idle at
/// `G_PRIORITY_DEFAULT_IDLE` (200) loses to `GDK_PRIORITY_REDRAW` (120), so the
/// pending frame wins — is wrong in practice: GDK's frame clock does not attach a
/// *ready* paint source. Its paint source is gated on `min_next_frame_time` and,
/// under Wayland, on the compositor's `wl_surface.frame` callback, so it is not
/// runnable at the instant the wake handler returns while a plain idle IS. GLib
/// therefore runs the idle first and the overlay is never composited.
///
/// Verified under `cage` with a deliberately slow (2 s) respawn: with
/// `idle_add_local_once` every capture across the whole respawn showed the FROZEN
/// pre-crash frame; with this timeout every capture showed
/// [`crate::liveness_ui::CRASHED_RETRYING_MESSAGE`].
///
/// The value is one compositor frame at a pessimistic refresh rate plus slack. It
/// is a COSMETIC latency knob paid at most
/// [`MAX_AUTO_RESTART_ATTEMPTS`] times per crash, and no assertion anywhere
/// depends on it: the cage smokes' `crashed` OCR term is satisfied by
/// [`crate::liveness_ui::CRASHED_FAILED_MESSAGE`] too, which is the state that
/// genuinely dwells.
const RESTART_DEFER: Duration = Duration::from_millis(50);

/// The widgets and handles [`build`] needs, as one struct rather than eight
/// positional parameters (`clippy::too_many_arguments` bites at seven).
pub struct ShellParts<'a> {
    /// The live core, shared with the rest of the app.
    pub core: &'a Rc<RefCell<Option<WolfCore>>>,
    /// The application window the `win.*` actions are installed on.
    pub window: &'a adw::ApplicationWindow,
    /// The file picker, opened by `win.go-to-file`.
    pub picker: &'a PickerHandle,
    /// The command palette, opened by `win.command-palette`.
    pub palette: &'a PaletteHandle,
    /// The buffer picker, opened by `win.buffer-picker`.
    pub buffers: &'a BufferPickerHandle,
    /// The project-search overlay, opened by `win.search-in-project`.
    pub search: &'a SearchHandle,
    /// The file-tree sidebar, re-rooted after Open Folder.
    pub tree: &'a TreeHandle,
    /// The sidebar's root widget, hidden in the empty state.
    pub sidebar: &'a gtk::Widget,
    /// The window's outer `gtk::Paned` (sidebar | editor).
    pub paned: &'a gtk::Paned,
    /// The empty-state overlay from [`build_empty_state`].
    pub empty_state: &'a gtk::Widget,
    /// The header bar's Open button, desensitized in the empty state.
    pub open_button: &'a gtk::Button,
    /// The crash/stall overlay + banner, driven by the liveness read.
    pub crash_overlay: &'a CrashOverlay,
    /// The pane view, reset after a successful in-place Neovim restart. No cycle:
    /// [`crate::pane_view`] does not reference the shell.
    pub panes: &'a PaneHandle,
    /// The application-state persistence glue, driven from the render wake and
    /// kept in step with the empty-state flag.
    pub state: &'a StateHandle,
}

/// The shell's shared state. All access happens on the single GTK main thread, so
/// `RefCell` (not a `Mutex`) is sufficient.
struct ShellInner {
    /// The live core, shared with the rest of the app.
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The application window (`win.*` action scope, dialog parent, close target).
    window: adw::ApplicationWindow,
    /// The file picker opened by `win.go-to-file`.
    picker: PickerHandle,
    /// The command palette opened by `win.command-palette`.
    palette: PaletteHandle,
    /// The buffer picker opened by `win.buffer-picker`.
    buffers: BufferPickerHandle,
    /// The project-search overlay opened by `win.search-in-project`.
    search: SearchHandle,
    /// The file-tree sidebar, re-rooted once a pending Open Folder settles.
    tree: TreeHandle,
    /// The sidebar's root widget; hidden in the empty state.
    sidebar: gtk::Widget,
    /// The outer `gtk::Paned` whose divider collapses in the empty state.
    paned: gtk::Paned,
    /// The empty-state overlay; visible only when no project is open.
    empty_state: gtk::Widget,
    /// The header bar's Open button; insensitive in the empty state.
    open_button: gtk::Button,
    /// The `win.go-to-file` action, stashed by [`AppShell::install_actions`] so the
    /// empty state can disable it.
    go_to_file: RefCell<Option<gio::SimpleAction>>,
    /// The `win.search-in-project` action, stashed for the same reason.
    search_in_project: RefCell<Option<gio::SimpleAction>>,
    /// A directory the user chose in the Open Folder dialog, awaiting nvim's
    /// `DirChanged`. Settled on a later wake by [`workspace::reroot_ready`].
    pending_reroot: RefCell<Option<PathBuf>>,
    /// The crash/stall overlay + banner, driven by [`AppShell::on_liveness_wake`].
    crash_overlay: CrashOverlay,
    /// The pane view, reset after a successful in-place restart.
    panes: PaneHandle,
    /// The application-state persistence glue.
    state: StateHandle,
    /// The user's home directory, stringified ONCE at [`build`] time — the
    /// injected `home` for [`title::window_title`]. Captured rather than read per
    /// frame because [`title`] deliberately never touches the environment, and
    /// `$HOME` does not change under a running process.
    home: String,
    /// The last title actually pushed to the window, so
    /// [`AppShell::refresh_title`] calls `set_title` only on a real change.
    last_title: RefCell<String>,
    /// FRONT-END re-entrancy guard for [`AppShell::restart_nvim`].
    ///
    /// Needed IN ADDITION to the core's own CAS: the core's guard returns
    /// `Ok(false)`, which this front-end would otherwise misread as "no respawn
    /// happened" and act on.
    restarting: Cell<bool>,
    /// Whether an idle callback that will call [`AppShell::restart_nvim`] is
    /// already pending, so a burst of `Dead` wakes schedules exactly one.
    restart_scheduled: Cell<bool>,
    /// AUTOMATIC restart attempts made since the core was last seen `Alive`,
    /// capped at [`MAX_AUTO_RESTART_ATTEMPTS`]. Reset to 0 on an `Alive` reading,
    /// on a successful respawn, and on an explicit button press.
    restart_attempts: Cell<u32>,
}

/// A cloneable handle to the application shell: the action namespace, the primary
/// menu, the empty state, and the command drain.
#[derive(Clone)]
pub struct AppShell {
    inner: Rc<ShellInner>,
}

/// Builds the GNOME primary menu model from [`actions::MENU_SECTIONS`].
///
/// Each declared section becomes a `gio::Menu` section (rendered with a
/// separator), and each item names its action directly — `gio::Menu` resolves the
/// name against the window's / application's action map at popup time, so the menu
/// model can be built before the actions are installed.
#[must_use]
pub fn build_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    for section in actions::MENU_SECTIONS {
        let items = gio::Menu::new();
        for item in section.items {
            items.append(Some(item.label), Some(item.action));
        }
        menu.append_section(None, &items);
    }
    menu
}

/// Builds the empty-state overlay: an opaque, full-bleed container holding an
/// `adw::StatusPage` with an "Open Folder…" button. Starts HIDDEN;
/// [`AppShell::set_empty_state`] shows it.
///
/// It is meant to be added with `gtk::Overlay::add_overlay` (the picker's
/// precedent) so the editor `DrawingArea` keeps its full allocation and no
/// spurious 1x1 grid resize happens while the empty state is up. The `background`
/// style class is what makes it opaque over the editor grid.
///
/// The button activates [`OPEN_FOLDER_ACTION`] on ITSELF rather than through a
/// captured window reference: a widget's action activation walks up the same
/// action muxer the window installs, so this works from inside the overlay with no
/// ordering constraint on when the action is installed. It deliberately does not
/// use the `action-name` property, which renders a button insensitive when the
/// action does not exist yet.
#[must_use]
pub fn build_empty_state() -> gtk::Widget {
    let button = gtk::Button::builder()
        .label(EMPTY_STATE_BUTTON)
        .halign(gtk::Align::Center)
        .build();
    button.add_css_class("suggested-action");
    button.add_css_class("pill");
    button.connect_clicked(|button| {
        if let Err(err) = button.activate_action(OPEN_FOLDER_ACTION, None) {
            eprintln!("wolf-gnome: failed to activate {OPEN_FOLDER_ACTION}: {err}");
        }
    });

    let status = adw::StatusPage::builder()
        .icon_name(EMPTY_STATE_ICON)
        .title(EMPTY_STATE_TITLE)
        .description(EMPTY_STATE_DESCRIPTION)
        .child(&button)
        .hexpand(true)
        .vexpand(true)
        .build();

    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.add_css_class("background");
    container.set_hexpand(true);
    container.set_vexpand(true);
    container.set_halign(gtk::Align::Fill);
    container.set_valign(gtk::Align::Fill);
    container.append(&status);
    container.set_visible(false);
    container.upcast::<gtk::Widget>()
}

/// Builds the application shell over an already-constructed window, wires the
/// crash overlay's "Restart Neovim" button to [`AppShell::restart_nvim`], and wires
/// the command palette's row activation to [`AppShell::dispatch_command`].
///
/// Installs nothing else by itself: call [`AppShell::install_actions`] (after the
/// window exists, before `present()`) and [`AppShell::set_empty_state`] next.
#[must_use]
pub fn build(parts: ShellParts<'_>) -> AppShell {
    let inner = Rc::new(ShellInner {
        core: parts.core.clone(),
        window: parts.window.clone(),
        picker: parts.picker.clone(),
        palette: parts.palette.clone(),
        buffers: parts.buffers.clone(),
        search: parts.search.clone(),
        tree: parts.tree.clone(),
        sidebar: parts.sidebar.clone(),
        paned: parts.paned.clone(),
        empty_state: parts.empty_state.clone(),
        open_button: parts.open_button.clone(),
        go_to_file: RefCell::new(None),
        search_in_project: RefCell::new(None),
        pending_reroot: RefCell::new(None),
        crash_overlay: parts.crash_overlay.clone(),
        panes: parts.panes.clone(),
        state: parts.state.clone(),
        home: workspace::home_dir()
            .map(|home| home.to_string_lossy().into_owned())
            .unwrap_or_default(),
        last_title: RefCell::new(String::new()),
        restarting: Cell::new(false),
        restart_scheduled: Cell::new(false),
        restart_attempts: Cell::new(0),
    });
    let shell = AppShell { inner };

    // The button and the automatic `Dead` path call the SAME function. Resetting
    // the attempt counter first is deliberate: the cap exists to stop an automatic
    // retry loop, never to refuse a user who asks again.
    //
    // KNOWN, DELIBERATE CONSEQUENCE — one button press can cost TWO respawns when
    // the respawn keeps failing. The reset restores the automatic budget, so if the
    // button's own restart fails, `WolfCore::restart_nvim`'s `Err` arm re-marks the
    // core `Dead` and fires a wake; `on_liveness_wake` then sees `restart_attempts
    // == 0`, classifies as `Crashed { retrying: true }` and schedules one more
    // AUTOMATIC attempt. The user sees the message flicker
    // `…restart failed` -> `…restarting…` -> `…restart failed`.
    //
    // This is BOUNDED, not a loop: the automatic attempt increments the counter
    // again, so the sequence terminates after exactly one extra respawn and only
    // another press can restart it. It is also arguably the desired reading of "the
    // user asked for a restart" — the alternative (resetting the counter only on an
    // Alive reading) would refuse a user whose first press raced a failing
    // automatic attempt. Left as is; the flicker is the price.
    let weak = Rc::downgrade(&shell.inner);
    shell.inner.crash_overlay.connect_restart(move || {
        if let Some(shell) = upgrade(&weak) {
            shell.inner.restart_attempts.set(0);
            shell.restart_nvim();
        }
    });

    // A palette row hands its command to the SAME function the drain loop uses, so
    // the two entry paths cannot classify a command differently.
    let weak_palette = Rc::downgrade(&shell.inner);
    shell.inner.palette.connect_activate(move |command| {
        if let Some(shell) = upgrade(&weak_palette) {
            shell.dispatch_command(command);
        }
    });

    shell
}

impl AppShell {
    /// Installs one `gio::SimpleAction` per IMPLEMENTED action of
    /// [`actions::ACTION_TABLE`], registers the accelerator of every
    /// [`BindingStatus::Bound`] row, and installs the capture-phase chord
    /// controller that actually matches them
    /// ([`AppShell::install_chord_controller`]).
    ///
    /// "Implemented" means the row's command classifies to a real [`UiAction`]:
    /// a `Deferred` / `NoGnomeEquivalent` row is never installed, so the primary
    /// menu can never show an item that does nothing. Reserved rows keep their
    /// chord out of both the accelerator registration and the chord controller for
    /// the same reason.
    ///
    /// Must run AFTER the window is constructed and BEFORE `window.present()`.
    pub fn install_actions(&self, app: &adw::Application) {
        for binding in actions::ACTION_TABLE {
            let Some(command) = binding.command else {
                // A front-end-only row (nav history) with nothing to dispatch yet.
                continue;
            };
            let ui_action = ui_action_for(command);
            if matches!(
                ui_action,
                UiAction::Deferred(_) | UiAction::NoGnomeEquivalent(_)
            ) {
                continue;
            }
            let Some((namespace, name)) = binding.action.split_once('.') else {
                eprintln!(
                    "wolf-gnome: action {:?} has no `app.`/`win.` namespace prefix; not installed",
                    binding.action
                );
                continue;
            };

            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(&self.inner);
            action.connect_activate(move |_action, _parameter| {
                if let Some(shell) = upgrade(&weak) {
                    shell.perform(ui_action);
                }
            });

            if namespace == "app" {
                app.add_action(&action);
            } else {
                self.inner.window.add_action(&action);
            }
            if binding.action == GO_TO_FILE_ACTION {
                *self.inner.go_to_file.borrow_mut() = Some(action);
            } else if binding.action == SEARCH_IN_PROJECT_ACTION {
                *self.inner.search_in_project.borrow_mut() = Some(action);
            }
        }

        // Registered for the MENU's accelerator labels, not for matching — see
        // `install_chord_controller`.
        for binding in actions::bound_bindings() {
            let Some(accel) = binding.accel else {
                continue;
            };
            debug_assert_eq!(binding.status, BindingStatus::Bound);
            app.set_accels_for_action(binding.action, &[accel]);
        }

        self.install_chord_controller();
    }

    /// Installs the CAPTURE-phase key controller that matches Wolf's chords
    /// against [`actions::action_for_key`] and activates the corresponding action.
    ///
    /// ## Why this exists instead of relying on `set_accels_for_action`
    ///
    /// GTK's own accelerator matching is keymap-aware: `gdk_key_event_matches`
    /// re-translates the event's KEYCODE through the display's current keymap and
    /// compares levels. `wtype` — the only way the cage smokes can type — uploads a
    /// fresh, single-keycode keymap for each keystroke, so a `Ctrl+Shift+<letter>`
    /// event fails to match even a trigger built from that same event's own keyval
    /// and modifier state (verified directly: `GdkKeyEvent::matches` returns
    /// `KeyMatch::None` for `(self.keyval(), self.modifier_state())`). Under
    /// `set_accels_for_action` alone, NO chord fired under `cage` and every one was
    /// forwarded to Neovim instead.
    ///
    /// Matching the delivered keyval and modifiers directly is keymap-independent,
    /// so it works under `wtype` and on a real keyboard alike.
    ///
    /// `set_accels_for_action` is still called, purely so `gio::Menu` renders the
    /// chord next to its item. If GTK's matching also fires on some setup, only ONE
    /// activation results: both controllers live on the window in the CAPTURE phase
    /// and both claim the event, and a widget stops running controllers for a phase
    /// once one has handled the event.
    ///
    /// A DISABLED action is deliberately left to fall through — the same
    /// documented behaviour `gio::SimpleAction::set_enabled(false)` gives a real
    /// accelerator, which is what makes the empty state's `win.go-to-file` wart
    /// (Ctrl+Shift+O reaches Neovim as `<C-O>` while no project is open) hold for
    /// this path too.
    fn install_chord_controller(&self) {
        let weak = Rc::downgrade(&self.inner);
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        controller.connect_key_pressed(move |_controller, keyval, _keycode, state| {
            let Some(shell) = upgrade(&weak) else {
                return glib::Propagation::Proceed;
            };
            let Some(action) = actions::action_for_key(keyval, state) else {
                return glib::Propagation::Proceed;
            };
            if !shell.action_is_enabled(action) {
                return glib::Propagation::Proceed;
            }
            if let Err(err) =
                gtk::prelude::WidgetExt::activate_action(&shell.inner.window, action, None)
            {
                eprintln!("wolf-gnome: failed to activate {action}: {err}");
                return glib::Propagation::Proceed;
            }
            glib::Propagation::Stop
        });
        self.inner.window.add_controller(controller);
    }

    /// Whether `action` (a `win.`- or `app.`-prefixed name) is installed AND
    /// enabled.
    fn action_is_enabled(&self, action: &str) -> bool {
        let Some((namespace, name)) = action.split_once('.') else {
            return false;
        };
        let found = if namespace == "app" {
            self.inner
                .window
                .application()
                .and_then(|app| app.lookup_action(name))
        } else {
            self.inner.window.lookup_action(name)
        };
        found.is_some_and(|action| action.is_enabled())
    }

    /// Shows or hides the empty state.
    ///
    /// `sidebar.set_visible(false)` is the load-bearing half: the `gtk::Paned`
    /// divider position is advisory (a start child that is merely positioned at 0
    /// can still be re-derived by a later allocation), while an invisible start
    /// child is never allocated at all.
    ///
    /// Disabling `win.go-to-file` and `win.search-in-project` uses the documented
    /// global escape hatch: a disabled action's accelerator does not activate AND the
    /// event falls through, so in the empty state Ctrl+Shift+O reaches Neovim as
    /// `<C-O>` (identical to `<C-o>`) and Ctrl+Shift+F reaches it as `<C-F>`
    /// (identical to `<C-f>`, page forward). That is an accepted wart, recorded in
    /// the README.
    ///
    /// The project-search overlay is guarded TWICE, and both halves are needed: the
    /// disabled action is what the chord and the menu item see, while
    /// [`crate::search_overlay::open_decision`]'s `project_open` arm — fed by the
    /// `set_project_open` call below — is what the command palette's row sees, since
    /// the palette offers a live row for every dispatched command.
    pub fn set_empty_state(&self, empty: bool) {
        self.inner.empty_state.set_visible(empty);
        self.inner.sidebar.set_visible(!empty);
        self.inner
            .paned
            .set_position(if empty { 0 } else { TREE_PANE_WIDTH });
        self.inner.open_button.set_sensitive(!empty);
        if let Some(action) = self.inner.go_to_file.borrow().as_ref() {
            action.set_enabled(!empty);
        }
        if let Some(action) = self.inner.search_in_project.borrow().as_ref() {
            action.set_enabled(!empty);
        }
        self.inner.search.set_project_open(!empty);
        // The persisted `cwd` / `sidebar_visible` follow the LIVE empty-state
        // flag, so an Open Folder into an empty-state window starts persisting
        // that folder from the very next wake.
        self.inner.state.set_empty(empty);
    }

    /// Reads the captive nvim's [`Liveness`] ONCE per wake, applies the pure
    /// [`liveness_ui`] mapping, and reports whether the pane view and the file
    /// tree should run.
    ///
    /// Called FIRST on every wake, before `panes.on_render_wake()`. The placement
    /// is load-bearing: the death of the captive nvim fires exactly ONE wake
    /// (`liveness::mark_dead` stores `Dead` and then wakes, once), after which no
    /// redraw wake will ever come again. Liveness is LEVEL-triggered — an atomic
    /// read, not a message — so a coalesced or dropped wake loses nothing PROVIDED
    /// the read is unconditional and happens before any handler that can
    /// early-return. `pane_view::on_render_wake` starts with a
    /// `core.borrow().is_none()` guard; reading liveness behind it would turn a
    /// level-triggered signal back into an edge-triggered one.
    ///
    /// Returns `false` (do not paint) when there is no core at all.
    pub fn on_liveness_wake(&self) -> bool {
        // Short borrow in its own statement; `Liveness` is `Copy`, and
        // `WolfCore::liveness` is a lock-free atomic read, never an RPC.
        let liveness = {
            let core_ref = self.inner.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return false;
            };
            core.liveness()
        };

        let may_auto_restart = self.inner.restart_attempts.get() < MAX_AUTO_RESTART_ATTEMPTS;
        let ui = liveness_ui(liveness, may_auto_restart);

        // A healthy core clears the budget, so a LATER, unrelated crash gets its
        // own automatic attempt rather than inheriting an exhausted counter.
        if liveness == Liveness::Alive {
            self.inner.restart_attempts.set(0);
        }

        self.apply_overlay(ui.overlay);

        if ui.auto_restart && !self.inner.restarting.get() && !self.inner.restart_scheduled.get() {
            self.schedule_restart();
        }

        ui.paint
    }

    /// Applies an overlay mode, performing the focus/picker side effects ONLY on a
    /// real transition.
    ///
    /// [`crate::crash_overlay::CrashOverlay::set_mode`] reports whether the mode
    /// changed, and gating on that is what keeps this from firing on every wake:
    /// closing the picker (or yanking focus back to the cmdline strip) once per
    /// `Alive` frame would make the picker unusable.
    ///
    /// - hidden -> visible: close ALL FOUR transient overlays. A crashed core must
    ///   not leave the picker's Open reachable underneath, nor a palette row whose
    ///   activation would send into a dead nvim (`New`, `Close`) or open another
    ///   overlay over the crash one (`Go to File…`). The buffer picker is the one
    ///   this cannot skip: a stall is discovered BY its candidate snapshot timing
    ///   out, so the transition to `Stalled` always finds it open with zero rows,
    ///   and an overlay left open keeps `app::wire_keys`' guard suppressing every
    ///   key to Neovim — including after a later restart.
    /// - visible -> hidden: if focus was inside the overlay (i.e. on the restart
    ///   button), park it back on the persistent cmdline strip. Hiding the focus
    ///   widget without a focus-return leaves the toplevel with no focus widget at
    ///   all, which kills window-level key dispatch outright.
    fn apply_overlay(&self, mode: OverlayMode) {
        // Sampled BEFORE `set_mode`, which is what hides the overlay. GTK4 hides
        // rather than unroots, so the root would still report the focus widget
        // afterwards today — but `pane_view::rebuild_pane_area` states the
        // convention for this codebase ("must be sampled BEFORE anything is
        // unparented: once the focus widget is gone the root reports no focus at
        // all"), and the moment this overlay is ever unparented instead of hidden
        // a post-hoc sample would silently stop returning focus. Do not "tidy"
        // this back below the `set_mode` call.
        let had_focus = self.inner.crash_overlay.focus_is_inside();
        if !self.inner.crash_overlay.set_mode(mode) {
            return;
        }
        if mode == OverlayMode::Hidden {
            if had_focus {
                self.inner.panes.cmdline_widget().grab_focus();
            }
        } else {
            self.inner.picker.close();
            self.inner.palette.close();
            self.inner.buffers.close();
            // A fourth idempotent close, for the same reason as the buffer picker's:
            // an overlay left flagged open keeps `app::wire_keys`' guard suppressing
            // every key to Neovim, including after a restart. Here it also cancels
            // any `rg` still running behind the overlay.
            self.inner.search.close();
        }
    }

    /// Schedules ONE automatic restart attempt [`RESTART_DEFER`] after the wake
    /// handler returns.
    ///
    /// **Deferred rather than called inline, deliberately.**
    /// [`WolfCore::restart_nvim`] is synchronous — a `runtime.block_on` over a
    /// kill, a spawn and twelve setup RPCs — so calling it from inside the wake
    /// handler would block the GTK main loop for the whole respawn with the crash
    /// overlay shown-but-never-presented, which is precisely why macOS's `.dead`
    /// overlay is never drawn on the success path. Deferring also guarantees the
    /// restart runs with no `RefCell` borrow live anywhere and lets
    /// [`AppShell::on_render_wake`]'s command drain have its turn first.
    fn schedule_restart(&self) {
        self.inner.restart_scheduled.set(true);
        self.inner
            .restart_attempts
            .set(self.inner.restart_attempts.get().saturating_add(1));

        let weak = Rc::downgrade(&self.inner);
        glib::timeout_add_local_once(RESTART_DEFER, move || {
            let Some(shell) = upgrade(&weak) else {
                return;
            };
            shell.inner.restart_scheduled.set(false);
            shell.restart_nvim();
        });
    }

    /// Restarts the captive Neovim in place and restores the Tier-1 file set.
    ///
    /// The ONE path shared by the automatic (`Dead`) restart and the overlay
    /// button. Order matters: the open-file mirror is captured BEFORE the call
    /// (a respawn wipes it), the front-end re-entrancy flag is cleared BEFORE
    /// anything is acted on (so a re-entrant call from a widget callback cannot
    /// strand it set), and the banner is flashed on the SUCCESS path only.
    fn restart_nvim(&self) {
        if self.inner.restarting.get() {
            return;
        }
        self.inner.restarting.set(true);

        let captured = self.capture_open_files();
        let respawned = self.perform_restart();
        let fresh = self.read_liveness();

        // Cleared before acting on the outcome, so every exit path below — early
        // return included — leaves the guard down.
        self.inner.restarting.set(false);

        let Some(fresh) = fresh else {
            return;
        };
        match restart_outcome(respawned, fresh) {
            // No respawn: the core self-recovered before the click, a concurrent
            // restart held the core's CAS, or the respawn failed. Re-apply the
            // overlay from the fresh reading and do NOTHING else — no reset, no
            // reopen, no banner, and above all no rescheduling (the `auto_restart`
            // field of a `Refresh` is always `false`, and acting on it would be an
            // infinite respawn loop).
            RestartOutcome::Refresh(ui) => self.apply_overlay(ui.overlay),
            RestartOutcome::Respawned => {
                self.inner.restart_attempts.set(0);
                self.inner.panes.reset_after_restart();
                self.reopen_captured(captured.as_ref());
                self.apply_overlay(OverlayMode::Hidden);
                self.inner.crash_overlay.flash_banner();
                // Belt and braces, mirroring `app.rs`'s "react to whatever is
                // already there" call: a harmless no-op if the fresh nvim has not
                // reported `win_pos` yet.
                self.inner.panes.on_render_wake();
            }
        }
    }

    /// Captures the Tier-1 open-file mirror for the current tabpage BEFORE a
    /// restart wipes it. Both reads are RPC-free.
    ///
    /// **Never `WolfCore::current_tabpage()`**: that is a `block_on_rpc`, and
    /// against a dead nvim it would freeze the GTK main loop for the whole timeout
    /// budget on every crash. `Snapshot::current_tabpage` carries the same handle
    /// as a plain field, emitted once at setup by `runtime/nvim/init.lua`'s
    /// `wolf_setup_tabs`. A `current_tabpage` of `0` simply yields `None` — that
    /// is "nothing to reopen", not an error.
    fn capture_open_files(&self) -> Option<TabFiles> {
        let tabpage = {
            let core_ref = self.inner.core.borrow();
            let core = core_ref.as_ref()?;
            // Bound to a local so the `SnapshotReadGuard` is dropped at the end of
            // THIS block, before `open_files_for_tabpage` takes the same lock.
            let snapshot = core.read_snapshot();
            snapshot.current_tabpage
        };
        let core_ref = self.inner.core.borrow();
        core_ref.as_ref()?.open_files_for_tabpage(tabpage)
    }

    /// Calls [`WolfCore::restart_nvim`] under a short borrow and reports whether an
    /// ACTUAL respawn happened.
    ///
    /// An `Err` is logged, never swallowed, and reported as "no respawn" — the
    /// core has already re-marked liveness `Dead`, so the caller's fresh read
    /// keeps the overlay up.
    fn perform_restart(&self) -> bool {
        let result = {
            let core_ref = self.inner.core.borrow();
            core_ref.as_ref().map(WolfCore::restart_nvim)
        };
        match result {
            Some(Ok(respawned)) => respawned,
            Some(Err(err)) => {
                eprintln!("wolf-gnome: failed to restart Neovim: {err}");
                false
            }
            None => false,
        }
    }

    /// The captive nvim's current [`Liveness`], or `None` when there is no core.
    fn read_liveness(&self) -> Option<Liveness> {
        let core_ref = self.inner.core.borrow();
        Some(core_ref.as_ref()?.liveness())
    }

    /// Reopens a captured Tier-1 file set in [`reopen_order`] — non-active files
    /// first, the ACTIVE one LAST so it lands focused.
    ///
    /// `InputEvent::Open` is the same event the picker and the file tree send, so
    /// the MRU is recorded identically: one path, no drift. Nothing in the loop can
    /// re-enter the `RefCell`, so one short borrow covers the whole reopen.
    fn reopen_captured(&self, captured: Option<&TabFiles>) {
        let Some(captured) = captured else {
            return;
        };
        let order = reopen_order(&captured.files, captured.active.as_deref());
        let core_ref = self.inner.core.borrow();
        let Some(core) = core_ref.as_ref() else {
            return;
        };
        for path in order {
            core.send_input(InputEvent::Open { path });
        }
    }

    /// Reacts to a render wake: settles a pending Open Folder re-root, then drains
    /// and dispatches every queued command.
    ///
    /// The app calls this LAST on each wake (after the pane view and the tree),
    /// because a drained `close_window` tears the core down. Unlike the paint, it
    /// runs in ALL THREE liveness states — see the module docs.
    pub fn on_render_wake(&self) {
        self.settle_pending_reroot();
        self.refresh_title();
        self.inner.state.on_render_wake();
        // LAST: a drained `close_window` tears the core down, so the state
        // snapshot above must already have happened.
        self.drain_and_dispatch_commands();
    }

    /// Sets the window title from the captive nvim's LIVE working directory,
    /// through the pure [`title::window_title`] seam.
    ///
    /// The cwd read is RPC-free (`WolfCore::live_cwd` is a tracked-cell read) and
    /// `home` was captured once at [`build`] time, so this costs a string compare
    /// on a steady-state frame.
    ///
    /// **The `last_title` cache is not cosmetic.** `set_title` on every frame is
    /// pointless GTK churn; more to the point, the cage title smoke's red-run
    /// revert is the deletion of the single `set_title` call below, so that call
    /// has to be the only place a title is ever pushed.
    fn refresh_title(&self) {
        // Short borrow in its own statement, dropped before anything else.
        let cwd = {
            let core_ref = self.inner.core.borrow();
            core_ref.as_ref().and_then(WolfCore::live_cwd)
        };
        let cwd = cwd.map(|cwd| cwd.to_string_lossy().into_owned());
        let title = title::window_title(cwd.as_deref().unwrap_or_default(), &self.inner.home);
        // Bound to a local so the `RefCell` borrow cannot still be alive when the
        // `borrow_mut()` below runs.
        let unchanged = *self.inner.last_title.borrow() == title;
        if unchanged {
            return;
        }
        self.inner.window.set_title(Some(&title));
        *self.inner.last_title.borrow_mut() = title;
    }

    /// Drains the core's command queue and dispatches each command.
    ///
    /// The borrow is released in its own statement BEFORE anything is dispatched;
    /// see the module docs.
    fn drain_and_dispatch_commands(&self) {
        let commands: Vec<Command> = {
            let core_ref = self.inner.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            core.drain_commands()
        };
        for command in commands {
            self.dispatch_command(command);
        }
    }

    /// Classifies one [`Command`] through [`ui_action_for`] and performs it, or
    /// PRINTS the recorded reason when this front-end does not implement it.
    ///
    /// The single funnel shared by the command drain and the command palette's row
    /// activation, so a command cannot mean one thing when Neovim queues it and
    /// another when the user picks it out of a list.
    fn dispatch_command(&self, command: Command) {
        match ui_action_for(command) {
            UiAction::Deferred(reason) | UiAction::NoGnomeEquivalent(reason) => {
                eprintln!("wolf-gnome: command {command:?} not dispatched: {reason}");
            }
            action => self.perform(action),
        }
    }

    /// Performs one UI action. Exhaustive with no `_` arm, so a new [`UiAction`]
    /// variant is a compile error here.
    ///
    /// **There are four transient overlays and at most one is open at a time; this
    /// is where that is arbitrated**: opening any one of them closes the other three
    /// first. Every `close()` is idempotent. This module holds all four handles, so
    /// the rule costs the four overlay modules no knowledge of each other — and it
    /// is what keeps `app::wire_keys`' open-overlay guard honest and makes "Escape
    /// closes the thing you can see" true.
    fn perform(&self, action: UiAction) {
        match action {
            UiAction::OpenFilePicker => {
                self.inner.palette.close();
                self.inner.buffers.close();
                self.inner.search.close();
                self.inner.picker.open();
            }
            UiAction::OpenCommandPalette => {
                self.inner.picker.close();
                self.inner.buffers.close();
                self.inner.search.close();
                self.inner.palette.open();
            }
            UiAction::OpenBufferPicker => {
                self.inner.picker.close();
                self.inner.palette.close();
                self.inner.search.close();
                self.inner.buffers.open();
            }
            UiAction::OpenSearchOverlay => {
                self.inner.picker.close();
                self.inner.palette.close();
                self.inner.buffers.close();
                self.inner.search.open();
            }
            UiAction::OpenFileDialog => self.open_file_dialog(),
            UiAction::OpenFolderDialog => self.open_folder_dialog(),
            UiAction::NewUntitledBuffer => self.send_input(InputEvent::NewBuffer),
            UiAction::ClosePane => self.send_input(InputEvent::ClosePane),
            UiAction::CloseWindow => self.close_window(),
            UiAction::MinimizeWindow => self.inner.window.minimize(),
            UiAction::QuitApplication => self.quit_application(),
            // Unreachable through an installed action (install_actions skips these)
            // and already reported by the drain, but never silent either way.
            UiAction::Deferred(reason) | UiAction::NoGnomeEquivalent(reason) => {
                eprintln!("wolf-gnome: ui action not performed: {reason}");
            }
        }
    }

    /// Closes the window through `close-request` so `app::wire_close` runs (picker,
    /// palette, buffer picker, project search and tree closed, then
    /// `WolfCore::shutdown`).
    ///
    /// NEVER `destroy()`: that skips `close-request` entirely and leaks the nvim
    /// child. The `adw::ApplicationWindow` is owned by the application, so the app
    /// exits once its last window closes.
    fn close_window(&self) {
        self.inner.window.close();
    }

    /// Quits by closing EVERY application window, for the same clean-shutdown
    /// reason as [`AppShell::close_window`].
    ///
    /// Deliberately not `gio::Application::quit()`, which bypasses `close-request`
    /// and leaves the captive Neovim orphaned.
    fn quit_application(&self) {
        let Some(app) = self.inner.window.application() else {
            self.close_window();
            return;
        };
        for window in app.windows() {
            window.close();
        }
    }

    /// Opens a native folder chooser; on success `:tcd`s the core there and records
    /// the choice as a pending re-root.
    ///
    /// The sidebar is NOT re-rooted synchronously: `start_file_search` resolves its
    /// root from `live_cwd()`, which only updates once nvim reports `DirChanged`,
    /// so an immediate re-root would re-root at the OLD cwd. See
    /// [`AppShell::settle_pending_reroot`].
    fn open_folder_dialog(&self) {
        let dialog = gtk::FileDialog::builder()
            .title(OPEN_FOLDER_DIALOG_TITLE)
            .modal(true)
            .build();
        let weak = Rc::downgrade(&self.inner);
        dialog.select_folder(
            Some(&self.inner.window),
            gio::Cancellable::NONE,
            move |result| {
                let Some(shell) = upgrade(&weak) else {
                    return;
                };
                // An Err here is overwhelmingly "the user dismissed the dialog",
                // which is not a condition worth reporting.
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        shell.chdir_to(path);
                    }
                }
            },
        );
    }

    /// Opens a native file chooser; on success asks the core to `:e` the file.
    fn open_file_dialog(&self) {
        let dialog = gtk::FileDialog::builder()
            .title(OPEN_FILE_DIALOG_TITLE)
            .modal(true)
            .build();
        let weak = Rc::downgrade(&self.inner);
        dialog.open(
            Some(&self.inner.window),
            gio::Cancellable::NONE,
            move |result| {
                let Some(shell) = upgrade(&weak) else {
                    return;
                };
                // As above: a dismissed dialog is not an error worth printing.
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        shell.send_input(InputEvent::Open { path });
                    }
                }
            },
        );
    }

    /// Changes the core's working directory to `path` and records the pending
    /// re-root (canonicalized, because `reroot_ready` compares it against the
    /// core's already-canonical `live_cwd`).
    fn chdir_to(&self, path: PathBuf) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        self.send_input(InputEvent::Chdir { path: path.clone() });
        *self.inner.pending_reroot.borrow_mut() = Some(path);
    }

    /// If a pending Open Folder re-root has been applied by nvim, clears the empty
    /// state and re-roots the sidebar. Run on every wake, BEFORE the command drain.
    fn settle_pending_reroot(&self) {
        let pending = self.inner.pending_reroot.borrow().clone();
        let Some(pending) = pending else {
            return;
        };
        let live = {
            let core_ref = self.inner.core.borrow();
            core_ref.as_ref().and_then(WolfCore::live_cwd)
        };
        if !workspace::reroot_ready(Some(pending.as_path()), live.as_deref()) {
            return;
        }
        *self.inner.pending_reroot.borrow_mut() = None;
        self.set_empty_state(false);
        self.inner.tree.reroot();
    }

    /// Sends one input event to the core, holding the borrow only for the call.
    fn send_input(&self, event: InputEvent) {
        if let Some(core) = self.inner.core.borrow().as_ref() {
            core.send_input(event);
        }
    }
}

/// Upgrades a closure's `Weak<ShellInner>` back into an [`AppShell`].
///
/// Every closure the shell installs holds a `Weak`, never an `Rc`: `ShellInner`
/// owns the window, and the window owns the actions whose closures would otherwise
/// own `ShellInner` right back.
fn upgrade(weak: &Weak<ShellInner>) -> Option<AppShell> {
    weak.upgrade().map(|inner| AppShell { inner })
}
