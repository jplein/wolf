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
//! ## The drain
//!
//! [`AppShell::on_render_wake`] runs on every render wake, AFTER the pane view and
//! the tree (a drained `close_window` shuts the core down, so the shell must be
//! last). It settles a pending re-root, then drains
//! [`wolf_core::WolfCore::drain_commands`] and dispatches each command. A command
//! `wolf-gnome` does not implement is PRINTED with its recorded reason — never
//! silently dropped.
//!
//! ## Two entry paths, one namespace
//!
//! A command that arrives from inside Neovim (`:WolfClosePane`, `require('wolf')`,
//! `:q`/`:qa`) goes through the drain. A chord or a menu click activates the
//! matching `gio::SimpleAction` directly, without a round trip through the core's
//! command queue. Both end in [`UiAction`], so the two paths cannot diverge.
//!
//! ## Borrow discipline (load-bearing)
//!
//! `drain_and_dispatch_commands` takes a SHORT `core.borrow()`, collects the
//! commands into a `Vec`, and drops the borrow before dispatching any of them.
//! `UiAction::CloseWindow` / `UiAction::QuitApplication` synchronously run
//! `app::wire_close`'s `close-request` handler, which does `core.borrow_mut()
//! .take()`; an outstanding `borrow()` there is a `RefCell` panic, not a warning.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::{gio, glib};

use wolf_core::{Command, InputEvent, WolfCore};

use crate::actions::{self, BindingStatus};
use crate::app::TREE_PANE_WIDTH;
use crate::command_action::{ui_action_for, UiAction};
use crate::picker::PickerHandle;
use crate::tree_view::TreeHandle;
use crate::workspace;

/// The action the empty state's button and the primary menu use to pick a folder.
const OPEN_FOLDER_ACTION: &str = "win.open-folder";

/// The action that opens the in-window file picker (disabled in the empty state).
/// Also the header bar's Open button target.
pub(crate) const GO_TO_FILE_ACTION: &str = "win.go-to-file";

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

/// The widgets and handles [`build`] needs, as one struct rather than eight
/// positional parameters (`clippy::too_many_arguments` bites at seven).
pub struct ShellParts<'a> {
    /// The live core, shared with the rest of the app.
    pub core: &'a Rc<RefCell<Option<WolfCore>>>,
    /// The application window the `win.*` actions are installed on.
    pub window: &'a adw::ApplicationWindow,
    /// The file picker, opened by `win.go-to-file`.
    pub picker: &'a PickerHandle,
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
    /// A directory the user chose in the Open Folder dialog, awaiting nvim's
    /// `DirChanged`. Settled on a later wake by [`workspace::reroot_ready`].
    pending_reroot: RefCell<Option<PathBuf>>,
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

/// Builds the application shell over an already-constructed window.
///
/// Installs nothing by itself: call [`AppShell::install_actions`] (after the
/// window exists, before `present()`) and [`AppShell::set_empty_state`] next.
#[must_use]
pub fn build(parts: ShellParts<'_>) -> AppShell {
    let inner = Rc::new(ShellInner {
        core: parts.core.clone(),
        window: parts.window.clone(),
        picker: parts.picker.clone(),
        tree: parts.tree.clone(),
        sidebar: parts.sidebar.clone(),
        paned: parts.paned.clone(),
        empty_state: parts.empty_state.clone(),
        open_button: parts.open_button.clone(),
        go_to_file: RefCell::new(None),
        pending_reroot: RefCell::new(None),
    });
    AppShell { inner }
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
    /// Disabling `win.go-to-file` uses the documented global escape hatch: a
    /// disabled action's accelerator does not activate AND the event falls through,
    /// so in the empty state Ctrl+Shift+O reaches Neovim as `<C-O>` (identical to
    /// `<C-o>`). That is an accepted wart, recorded in the README.
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
    }

    /// Reacts to a render wake: settles a pending Open Folder re-root, then drains
    /// and dispatches every queued command.
    ///
    /// The app calls this LAST on each wake (after the pane view and the tree),
    /// because a drained `close_window` tears the core down.
    pub fn on_render_wake(&self) {
        self.settle_pending_reroot();
        self.drain_and_dispatch_commands();
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
            match ui_action_for(command) {
                UiAction::Deferred(reason) | UiAction::NoGnomeEquivalent(reason) => {
                    eprintln!("wolf-gnome: command {command:?} not dispatched: {reason}");
                }
                action => self.perform(action),
            }
        }
    }

    /// Performs one UI action. Exhaustive with no `_` arm, so a new [`UiAction`]
    /// variant is a compile error here.
    fn perform(&self, action: UiAction) {
        match action {
            UiAction::OpenFilePicker => self.inner.picker.open(),
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

    /// Closes the window through `close-request` so `app::wire_close` runs (picker
    /// + tree closed, then `WolfCore::shutdown`).
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
