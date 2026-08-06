//! The GTK GLUE for the `wolf-gnome` action namespace: `gio::SimpleAction`s, the
//! GNOME primary menu, the accelerator registration, the empty state, and the
//! command drain.
//!
//! Everything this module does is driven by data it does not own:
//! [`crate::actions::ACTION_TABLE`] declares the action names, accelerators and
//! their `Command` correspondence; [`crate::actions::MENU_SECTIONS`] declares the
//! menu; [`crate::command_action::ui_action_for`] decides what each drained
//! command means; [`crate::workspace`] decides whether a settle moves the sidebar's
//! root.
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
//! - [`AppShell::on_render_wake`] reads the captive nvim's LIVE working directory
//!   ONCE, settles the sidebar's root against it ([`AppShell::settle_reroot`] over
//!   the pure [`workspace::reroot_decision`]), refreshes the window
//!   TITLE from that same value ([`AppShell::refresh_title`] over the pure
//!   [`crate::title`] seam), refreshes the two navigation buttons' sensitivity from
//!   the FOCUSED pane ([`AppShell::refresh_nav_buttons`] over the pure
//!   [`crate::nav_ui`] seam — the wake a `WinEnter` fires bumps no `seq`, so this is
//!   what makes the toolbar follow the focus), asks [`crate::persistence`] for an
//!   application-state
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
//! ([`crate::palette`]) hands its whole [`crate::palette::PaletteAction`] to
//! [`AppShell::on_palette_activation`], whose command arm calls
//! [`AppShell::dispatch_command`] — the same function the drain loop calls. All
//! three end in [`UiAction`] and in [`AppShell::perform`], so they cannot diverge.
//!
//! An Open Recent palette row is the one activation that is NOT a `Command`: it
//! carries a directory, and [`AppShell::on_palette_activation`] is the one place
//! that directory becomes a window action. It re-roots this window, through the
//! same [`AppShell::chdir_to`] the native Open Folder dialog calls.
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
//! The sidebar's re-root adds NO blocking call, which is worth stating
//! because it is the one new per-wake core read this module makes:
//! `WolfCore::live_cwd` is a `parking_lot` read of a tracked cell, taken once per
//! wake in [`AppShell::on_render_wake`] and passed to both consumers.
//!
//! Exactly SEVEN `block_on_rpc` call sites are reachable from this crate, and the
//! restart path issues none of them. The first is `WolfCore::rebuild_layout`, called
//! at most once per launch from `pane_view::apply_layout_rebuild`, before the window
//! is interactive. The second is `WolfCore::list_buffers`, the buffer picker's
//! candidate snapshot — and it is reachable from a RUNNING application, which is
//! what makes `Liveness::NotResponding` a state a user can actually reach here.
//! [`crate::buffer_picker::open_decision`] guards it: the picker refuses to open
//! unless the core reads `Alive`, so the snapshot can raise a stall ONCE and can
//! never re-probe a core already known to be wedged.
//!
//! The third is the NAVIGATION press. [`AppShell::navigate`] funnels through
//! `WolfCore::history_back` / `history_forward` into `block_on_rpc`, so a Back or
//! Forward against a wedged Neovim costs a full RPC timeout budget of frozen main
//! loop. [`crate::nav_ui::nav_press`] guards it with exactly the buffer picker's
//! argument and exactly its limits: it cannot prevent the FIRST stall and must not
//! be strengthened into something that never raises one; what it prevents is the
//! repeat freeze on every later press. One difference is worth writing down — a
//! press against a LIVE core with nothing to go back to issues no RPC at all,
//! because the nav mutex answers `None` before the RPC is reached. The button
//! sensitivity ([`crate::nav_ui::nav_buttons`]) is therefore an affordance and the
//! liveness arm is the guard.
//!
//! The fourth is the COPY yank. [`AppShell::copy_selection`] funnels through
//! `WolfCore::copy_visual_selection` into `block_on_rpc`, so a Ctrl+Shift+C against
//! a wedged Neovim costs a full RPC timeout budget of frozen main loop.
//! [`crate::clipboard_ui::copy_decision`] guards it with the buffer picker's
//! argument and its limits. PASTE adds no fifth: `gdk::Clipboard::read_text_async`
//! resolves on a later main-loop turn and `WolfCore::paste` is fire-and-forget on
//! the ordered input channel, so nothing on that path can time out and nothing on
//! it can mark the core `NotResponding`. [`crate::clipboard_ui::paste_decision`]
//! gates it anyway, for a reason that is not about blocking — see that module's
//! docs.
//!
//! The last three are the MARKDOWN PREVIEW's. [`AppShell::open_preview`] and
//! [`AppShell::edit_source`] funnel through `WolfCore::open_preview` /
//! `WolfCore::edit_source`, and `pane_view::refresh_previews` issues
//! `WolfCore::render_preview` once per changed source per wake — the only one of the
//! seven that can repeat while the user simply types. All three are gated through
//! [`crate::preview_ui`] ([`preview_ui::preview_press`], [`preview_ui::edit_press`],
//! `preview_ui::may_render`) with the buffer picker's argument and its limits.
//! Neither press has an [`crate::actions`] accelerator row, so their only routes are
//! the command palette and the `:WolfPreview` / `:WolfEdit` Ex commands.
//!
//! Neither clipboard action is disabled in the empty state, for the buffer
//! picker's recorded reason: the captive Neovim runs in a project-less window too.
//!
//! The BUTTONS desensitize while the two `gio` actions stay ENABLED, which is the
//! opposite of `win.go-to-file`'s empty-state treatment and is deliberate: a
//! disabled action's accelerator falls through, and `<Alt>Left` forwarded to an
//! unmapped Neovim splits into `<Esc>` + `<Left>` — the destruction that capturing
//! the chord exists to prevent. `win.toggle-sidebar` IS disabled in the empty
//! state, for the same reason `win.go-to-file` is, and its `Ctrl+Shift+B`
//! fall-through to `<C-b>` is the accepted wart of the `Ctrl+Shift+<letter>`
//! namespace.
//!
//! The project-search overlay ([`crate::search_overlay`]) adds NO blocking call
//! either, and
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

use crate::about;
use crate::actions::{self, BindingStatus};
use crate::app::TREE_PANE_WIDTH;
use crate::buffer_picker::BufferPickerHandle;
use crate::clipboard_ui::{self, CopyPress, PastePress};
use crate::command_action::{ui_action_for, ui_action_for_front_end_action, UiAction};
use crate::crash_overlay::CrashOverlay;
use crate::liveness_ui::{
    liveness_ui, reopen_order, restart_outcome, OverlayMode, RestartOutcome,
    MAX_AUTO_RESTART_ATTEMPTS,
};
use crate::nav_ui::{self, NavAvailability, NavPress};
use crate::palette::{PaletteAction, PaletteHandle};
use crate::pane_view::PaneHandle;
use crate::persistence::StateHandle;
use crate::picker::PickerHandle;
use crate::preview_ui::{self, EditPlacement, EditPress, PreviewPlacement, PreviewPress};
use crate::search_overlay::SearchHandle;
use crate::state_seam;
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

/// The action that moves the focused pane one entry BACK in its navigation
/// history. Also the header bar's Back button target.
pub(crate) const HISTORY_BACK_ACTION: &str = "win.history-back";

/// The action that moves the focused pane one entry FORWARD in its navigation
/// history. Also the header bar's Forward button target.
pub(crate) const HISTORY_FORWARD_ACTION: &str = "win.history-forward";

/// The action that shows or hides the file-tree sidebar (disabled in the empty
/// state, where there is no project to list). Also the header bar's sidebar
/// button target.
pub(crate) const TOGGLE_SIDEBAR_ACTION: &str = "win.toggle-sidebar";

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

/// Everything the two Markdown-preview presses are decided against, read from ONE
/// snapshot pass by [`AppShell::preview_shape`].
///
/// One value rather than five reads, because the five must describe the SAME
/// instant: a pane count taken from one snapshot and a preview-active flag from
/// another can name a shape that never existed, and the placement seams answer
/// "replace the pane the user is reading" for exactly such a shape.
#[derive(Clone, Copy)]
struct PreviewShape {
    /// `Snapshot::is_markdown_active` — the Preview press's buffer-kind gate.
    markdown_active: bool,
    /// `Snapshot::is_preview_active` — the Edit press's gate, and both placements'
    /// `active_is_preview` polarity input.
    preview_active: bool,
    /// Editor windows plus preview windows.
    pane_count: usize,
    /// Preview windows other than the active one.
    other_preview_panes: usize,
    /// Editor windows other than the active one.
    other_editor_panes: usize,
}

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
    /// The file-tree sidebar, re-rooted after a front-end directory change.
    pub tree: &'a TreeHandle,
    /// The sidebar's root widget, hidden in the empty state.
    pub sidebar: &'a gtk::Widget,
    /// The window's outer `gtk::Paned` (sidebar | editor).
    pub paned: &'a gtk::Paned,
    /// The empty-state overlay from [`build_empty_state`].
    pub empty_state: &'a gtk::Widget,
    /// The header bar's Open button, desensitized in the empty state.
    pub open_button: &'a gtk::Button,
    /// The header bar's Back button, desensitized by
    /// [`crate::nav_ui::nav_buttons`] on every wake.
    pub nav_back_button: &'a gtk::Button,
    /// The header bar's Forward button; see [`ShellParts::nav_back_button`].
    pub nav_forward_button: &'a gtk::Button,
    /// The header bar's sidebar-toggle button, desensitized in the empty state.
    pub sidebar_button: &'a gtk::Button,
    /// The user's restored sidebar PREFERENCE
    /// ([`crate::state_seam::RestorePlan::sidebar_visible`]). The empty state
    /// overrides it at apply time, never at restore time.
    pub sidebar_shown: bool,
    /// The working directory the captive nvim was launched in — the seed for
    /// [`ShellInner::rooted_cwd`]. Taken by value; `app::build_ui` already clones
    /// `RestorePlan::core_cwd` for the core config and for persistence.
    pub launch_cwd: PathBuf,
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
    /// The file-tree sidebar, re-rooted once a pending re-root settles.
    tree: TreeHandle,
    /// The sidebar's root widget; hidden in the empty state.
    sidebar: gtk::Widget,
    /// The outer `gtk::Paned` whose divider collapses in the empty state.
    paned: gtk::Paned,
    /// The empty-state overlay; visible only when no project is open.
    empty_state: gtk::Widget,
    /// The header bar's Open button; insensitive in the empty state.
    open_button: gtk::Button,
    /// The header bar's Back button; its sensitivity is recomputed on every wake
    /// by [`AppShell::refresh_nav_buttons`].
    nav_back_button: gtk::Button,
    /// The header bar's Forward button; see [`ShellInner::nav_back_button`].
    nav_forward_button: gtk::Button,
    /// The header bar's sidebar-toggle button; insensitive in the empty state.
    sidebar_button: gtk::Button,
    /// The user's sidebar PREFERENCE — what they last asked for, NOT what is on
    /// screen. [`state_seam::sidebar_is_shown`] combines it with
    /// [`ShellInner::empty`] to decide the latter.
    sidebar_shown: Cell<bool>,
    /// The LIVE empty-state flag. Stored because
    /// [`AppShell::apply_sidebar_visibility`] needs both halves of the rule and
    /// [`AppShell::toggle_sidebar`] is not given the flag.
    empty: Cell<bool>,
    /// The `win.go-to-file` action, stashed by [`AppShell::install_actions`] so the
    /// empty state can disable it.
    go_to_file: RefCell<Option<gio::SimpleAction>>,
    /// The `win.search-in-project` action, stashed for the same reason.
    search_in_project: RefCell<Option<gio::SimpleAction>>,
    /// The `win.toggle-sidebar` action, stashed for the same reason.
    toggle_sidebar: RefCell<Option<gio::SimpleAction>>,
    /// A directory the user chose — in the Open Folder dialog or as an Open Recent
    /// palette row — awaiting nvim's `DirChanged`. Both producers set it through
    /// [`AppShell::chdir_to`]; a later wake discards it via
    /// [`workspace::reroot_decision`] once nvim reports that directory.
    ///
    /// It records that a request is OUTSTANDING and nothing more: it never decides
    /// where the sidebar roots, so a `:tcd` nvim refused (core degrades that to a
    /// `tracing::warn!`) blocks no later re-root.
    pending_reroot: RefCell<Option<PathBuf>>,
    /// The directory the sidebar is CURRENTLY rooted at, canonical. The authority
    /// [`workspace::reroot_decision`] compares nvim's live cwd against.
    ///
    /// Seeded in [`build`] from the canonicalized [`ShellParts::launch_cwd`].
    /// Canonicalizing the seed is load-bearing: `live_cwd` is already canonical
    /// (core's `apply_cwd` canonicalizes) while a cwd restored from `state.json`
    /// need not be, and a textual mismatch would cost a spurious full re-root — a
    /// whole fresh traversal — on every launch.
    rooted_cwd: RefCell<Option<PathBuf>>,
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
        nav_back_button: parts.nav_back_button.clone(),
        nav_forward_button: parts.nav_forward_button.clone(),
        sidebar_button: parts.sidebar_button.clone(),
        sidebar_shown: Cell::new(parts.sidebar_shown),
        // Pessimistic until `set_empty_state` says otherwise, which `app::build_ui`
        // does immediately after `install_actions`. Nothing reads it before then.
        empty: Cell::new(true),
        go_to_file: RefCell::new(None),
        search_in_project: RefCell::new(None),
        toggle_sidebar: RefCell::new(None),
        pending_reroot: RefCell::new(None),
        // Canonicalized so the comparison against the core's already-canonical
        // `live_cwd` means what it says; see the field's docs.
        rooted_cwd: RefCell::new(Some(
            std::fs::canonicalize(&parts.launch_cwd).unwrap_or(parts.launch_cwd),
        )),
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

    // A palette row hands its whole `PaletteAction` to one function, whose command
    // arm is the SAME dispatch the drain loop uses, so the two entry paths cannot
    // classify a command differently.
    let weak_palette = Rc::downgrade(&shell.inner);
    shell.inner.palette.connect_activate(move |action| {
        if let Some(shell) = upgrade(&weak_palette) {
            shell.on_palette_activation(action);
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
            // A row resolves in exactly one of two places: through its command, or
            // — for a FRONT-END-ONLY row, which carries no `wolf_core::Command` at
            // all — through `ui_action_for_front_end_action`. A row that resolves
            // in neither is skipped with a printed reason, because its accelerator
            // IS registered: the chord would then be swallowed in the capture phase
            // and activate nothing, which is worse than not binding it.
            let ui_action = match binding.command {
                Some(command) => {
                    let ui_action = ui_action_for(command);
                    if matches!(
                        ui_action,
                        UiAction::Deferred(_) | UiAction::NoGnomeEquivalent(_)
                    ) {
                        continue;
                    }
                    ui_action
                }
                None => match ui_action_for_front_end_action(binding.action) {
                    Some(ui_action) => ui_action,
                    None => {
                        eprintln!(
                            "wolf-gnome: action {:?} carries no command and no front-end \
                             classification; not installed",
                            binding.action
                        );
                        continue;
                    }
                },
            };
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
            } else if binding.action == TOGGLE_SIDEBAR_ACTION {
                *self.inner.toggle_sidebar.borrow_mut() = Some(action);
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
        self.inner.empty.set(empty);
        // Never `sidebar.set_visible` directly: a user toggle and the empty state
        // are two inputs to ONE rule, and applying either on its own is how the
        // next `set_empty_state(false)` silently undoes what the user just asked
        // for.
        self.apply_sidebar_visibility();
        self.inner.open_button.set_sensitive(!empty);
        self.inner.sidebar_button.set_sensitive(!empty);
        if let Some(action) = self.inner.go_to_file.borrow().as_ref() {
            action.set_enabled(!empty);
        }
        if let Some(action) = self.inner.search_in_project.borrow().as_ref() {
            action.set_enabled(!empty);
        }
        if let Some(action) = self.inner.toggle_sidebar.borrow().as_ref() {
            action.set_enabled(!empty);
        }
        self.inner.search.set_project_open(!empty);
        // The persisted `cwd` follows the LIVE empty-state flag, so an Open Folder
        // into an empty-state window starts persisting that folder from the very
        // next wake. `sidebar_visible` does NOT: it persists the user's
        // preference, so an empty-state session cannot come back with the tree
        // hidden.
        self.inner.state.set_empty(empty);
    }

    /// Applies the sidebar-visibility rule to the widgets. The ONE site that calls
    /// `sidebar.set_visible` / `paned.set_position`.
    ///
    /// `sidebar.set_visible(false)` is the load-bearing half: the `gtk::Paned`
    /// divider position is advisory (a start child merely positioned at 0 can still
    /// be re-derived by a later allocation), while an invisible start child is never
    /// allocated at all.
    fn apply_sidebar_visibility(&self) {
        let shown =
            state_seam::sidebar_is_shown(self.inner.sidebar_shown.get(), self.inner.empty.get());
        self.inner.sidebar.set_visible(shown);
        self.inner
            .paned
            .set_position(if shown { TREE_PANE_WIDTH } else { 0 });
        // Relayed from the ONE site, so the sidebar's refresh gate can never
        // disagree with what is on screen: a refresh requested while hidden is
        // deferred, and re-showing owes exactly one catch-up.
        self.inner.tree.set_sidebar_visible(shown);
    }

    /// Flips the user's sidebar preference, applies it, and records it for
    /// persistence.
    ///
    /// The preference is what is persisted and restored — not what is on screen —
    /// so a session spent in the empty state cannot come back with the tree hidden.
    fn toggle_sidebar(&self) {
        let shown = !self.inner.sidebar_shown.get();
        self.inner.sidebar_shown.set(shown);
        self.apply_sidebar_visibility();
        self.inner.state.set_sidebar_shown(shown);
    }

    /// Yanks the focused Neovim's visual-mode selection and writes it to the
    /// system clipboard, or prints why it did not.
    ///
    /// **The yank is a BLOCKING RPC on the GTK main thread**
    /// (`WolfCore::copy_visual_selection` over `block_on_rpc`), so it is gated by
    /// [`clipboard_ui::copy_decision`] exactly as the buffer picker's candidate
    /// snapshot and a navigation move are; see the module docs.
    ///
    /// The core borrow is taken and dropped in a statement of its own, BEFORE the
    /// clipboard is touched. `clipboard_write` answering `None` means write
    /// NOTHING: `set_text("")` would clear the selection and destroy whatever the
    /// user copied from another application, which is why there is no `else` arm
    /// here.
    fn copy_selection(&self) {
        let liveness = self.read_liveness();

        match clipboard_ui::copy_decision(liveness) {
            CopyPress::Refuse(reason) => {
                eprintln!("wolf-gnome: copy not performed: {reason}");
            }
            CopyPress::Copy => {
                let selection = {
                    let core_ref = self.inner.core.borrow();
                    core_ref.as_ref().and_then(WolfCore::copy_visual_selection)
                };
                if let Some(text) = clipboard_ui::clipboard_write(selection.as_deref()) {
                    self.clipboard().set_text(text);
                }
            }
        }
    }

    /// Reads the system clipboard and inserts it through `nvim_paste`, or prints
    /// why it did not.
    ///
    /// **Nothing on this path blocks**, so it adds no entry to the module docs'
    /// blocking inventory: `gdk::Clipboard::read_text_async` hands GLib a
    /// trampoline and returns immediately, and `WolfCore::paste` is a send on the
    /// ordered input channel. [`clipboard_ui::paste_decision`] gates it for a
    /// different reason, which that module's docs state and which must not be
    /// collapsed into the copy gate's.
    ///
    /// The callback holds a `Weak<ShellInner>` and takes the core borrow in a
    /// statement of its own, so a read landing after `app::wire_close` has taken
    /// the core out performs nothing rather than colliding with its `borrow_mut`.
    /// An `Err` is printed, never swallowed, and then flattened to `None` for the
    /// pure rule.
    fn paste_clipboard(&self) {
        let liveness = self.read_liveness();

        match clipboard_ui::paste_decision(liveness) {
            PastePress::Refuse(reason) => {
                eprintln!("wolf-gnome: paste not performed: {reason}");
            }
            PastePress::Read => {
                let weak = Rc::downgrade(&self.inner);
                self.clipboard()
                    .read_text_async(gio::Cancellable::NONE, move |result| {
                        let Some(shell) = upgrade(&weak) else {
                            return;
                        };
                        let read = match result {
                            Ok(text) => text,
                            Err(err) => {
                                eprintln!("wolf-gnome: clipboard read failed: {err}");
                                None
                            }
                        };
                        let Some(text) = clipboard_ui::paste_payload(read.as_deref()) else {
                            return;
                        };
                        let core_ref = shell.inner.core.borrow();
                        if let Some(core) = core_ref.as_ref() {
                            core.paste(text.to_owned());
                        }
                    });
            }
        }
    }

    /// The window's `gdk::Clipboard` — the display's CLIPBOARD selection, not the
    /// primary one.
    ///
    /// Spelled as a fully qualified `WidgetExt::clipboard` call because the
    /// `adw::prelude::*` glob brings several traits carrying a `clipboard` method
    /// into scope, the same footgun `app.rs` documents for `activate_action`.
    fn clipboard(&self) -> gdk::Clipboard {
        gtk::prelude::WidgetExt::clipboard(&self.inner.window)
    }

    /// Moves the FOCUSED pane one entry back or forward in its own navigation
    /// history, or prints why it did not.
    ///
    /// **The move is a BLOCKING RPC on the GTK main thread**, so it is gated by
    /// [`nav_ui::nav_press`] exactly as the buffer picker's candidate snapshot is;
    /// see the module docs. The liveness read and the `Snapshot::active_win` read
    /// take SEPARATE short borrows, and the `SnapshotReadGuard` is bound to a local
    /// in its own statement so it is dropped before the core is touched again (the
    /// [`AppShell::capture_open_files`] precedent: a re-entrant read can deadlock
    /// behind a queued writer).
    ///
    /// The refresh at the end is what settles the buttons after a refused or failed
    /// move, which fires no wake of its own.
    fn navigate(&self, forward: bool) {
        let liveness = self.read_liveness();
        let active_win = self.read_active_win();

        match nav_ui::nav_press(liveness, active_win) {
            NavPress::Refuse(reason) => {
                eprintln!("wolf-gnome: navigation not performed: {reason}");
            }
            NavPress::Move(win) => {
                let core_ref = self.inner.core.borrow();
                if let Some(core) = core_ref.as_ref() {
                    if forward {
                        core.history_forward(win);
                    } else {
                        core.history_back(win);
                    }
                }
            }
        }

        self.refresh_nav_buttons();
    }

    /// Recomputes the two navigation buttons' sensitivity from the focused pane's
    /// availability.
    ///
    /// Runs on EVERY render wake, including the wake a `WinEnter` fires (which
    /// bumps no `seq`), which is what makes the buttons follow the focused pane.
    /// All three reads are cheap and RPC-free: the liveness read is an atomic, the
    /// `active_win` read is a snapshot field, and `history_can_back` /
    /// `history_can_forward` are brief nav-mutex reads.
    ///
    /// **No `last_*` cache.** Unlike `set_title`, `gtk_widget_set_sensitive`
    /// early-returns on an unchanged value, so a cache would be state to keep in
    /// sync for nothing.
    fn refresh_nav_buttons(&self) {
        let liveness = self.read_liveness();
        let active_win = self.read_active_win();
        // The snapshot guard from `read_active_win` is already dropped here, which
        // is what makes taking the nav mutex below safe.
        let history = match nav_ui::nav_query_win(active_win) {
            Some(win) => {
                let core_ref = self.inner.core.borrow();
                core_ref.as_ref().map(|core| NavAvailability {
                    can_back: core.history_can_back(win),
                    can_forward: core.history_can_forward(win),
                })
            }
            None => None,
        };

        let buttons = nav_ui::nav_buttons(liveness, active_win, history);
        self.inner.nav_back_button.set_sensitive(buttons.back);
        self.inner.nav_forward_button.set_sensitive(buttons.forward);
    }

    /// The focused Neovim window handle, or `None` when there is no core.
    ///
    /// The `SnapshotReadGuard` is bound to a local so it is dropped at the end of
    /// THIS function, before any caller takes another core lock.
    fn read_active_win(&self) -> Option<i64> {
        let core_ref = self.inner.core.borrow();
        let core = core_ref.as_ref()?;
        let snapshot = core.read_snapshot();
        snapshot.active_win
    }

    /// The whole window shape both preview presses are decided against, read in ONE
    /// snapshot pass — or `None` when there is no core.
    ///
    /// [`AppShell::read_active_win`]'s idiom: one core borrow, one
    /// `SnapshotReadGuard` bound to a local so it is dropped before the caller takes
    /// another core lock.
    ///
    /// **`WolfCore::is_markdown_active()` and `is_preview_active()` must never be
    /// called from a scope that already holds a snapshot guard** — they take their
    /// own, and a re-entrant read can deadlock behind a queued writer, which is the
    /// hazard this module's borrow discipline exists to prevent. The two flags are
    /// therefore read off the `Snapshot` already in hand, through
    /// [`Snapshot::is_markdown_active`] and [`Snapshot::is_preview_active`].
    ///
    /// The counts are `win_buffers` (whose keys are exactly the EDITOR windows,
    /// preview scratches being excluded from it) and `previews` (keyed by the preview
    /// WINDOW handle), which are disjoint — so their sum is the pane count and each
    /// "other" count is that map's keys minus the active window.
    ///
    /// [`Snapshot::is_markdown_active`]: wolf_core::Snapshot::is_markdown_active
    /// [`Snapshot::is_preview_active`]: wolf_core::Snapshot::is_preview_active
    fn preview_shape(&self) -> Option<PreviewShape> {
        let core_ref = self.inner.core.borrow();
        let core = core_ref.as_ref()?;
        let snapshot = core.read_snapshot();
        let active = snapshot.active_win;
        Some(PreviewShape {
            markdown_active: snapshot.is_markdown_active(),
            preview_active: snapshot.is_preview_active(),
            pane_count: snapshot.win_buffers.len() + snapshot.previews.len(),
            other_preview_panes: snapshot
                .previews
                .keys()
                .filter(|win| Some(**win) != active)
                .count(),
            other_editor_panes: snapshot
                .win_buffers
                .keys()
                .filter(|win| Some(**win) != active)
                .count(),
        })
    }

    /// Opens a Markdown preview of the active buffer at the placement
    /// [`preview_ui::preview_placement`] gives for the live window shape, or prints
    /// why it did not.
    ///
    /// **The open is a BLOCKING RPC on the GTK main thread**
    /// (`WolfCore::open_preview` over `block_on_rpc`), so it is gated by
    /// [`preview_ui::preview_press`] exactly as the buffer picker's candidate
    /// snapshot and a navigation move are; see the module docs.
    ///
    /// [`AppShell::copy_selection`]'s shape: the two reads happen first, each in its
    /// own short borrow, and the core borrow for the RPC is taken in a statement of
    /// its own.
    fn open_preview(&self) {
        let liveness = self.read_liveness();
        let shape = self.preview_shape();

        let press = preview_ui::preview_press(
            liveness,
            shape.is_some_and(|s| s.markdown_active),
            PreviewPlacement {
                pane_count: shape.map_or(0, |s| s.pane_count),
                active_is_preview: shape.is_some_and(|s| s.preview_active),
                other_preview_panes: shape.map_or(0, |s| s.other_preview_panes),
            },
        );

        match press {
            PreviewPress::Refuse(reason) => {
                eprintln!("wolf-gnome: preview not opened: {reason}");
            }
            PreviewPress::Open(mode) => {
                let core_ref = self.inner.core.borrow();
                if let Some(core) = core_ref.as_ref() {
                    // The returned bufnr is deliberately discarded: the front-end
                    // learns the new pane from the next wake's `wolf_preview`
                    // notification, never from this answer.
                    let _ = core.open_preview(mode);
                }
            }
        }
    }

    /// Returns from the active preview pane to its source file at the placement
    /// [`preview_ui::edit_placement`] gives, or prints why it did not.
    ///
    /// **The return is a BLOCKING RPC on the GTK main thread**
    /// (`WolfCore::edit_source` over `block_on_rpc`), gated by
    /// [`preview_ui::edit_press`]. [`AppShell::open_preview`]'s shape, with the
    /// polarity inverted where the seam inverts it.
    fn edit_source(&self) {
        let liveness = self.read_liveness();
        let shape = self.preview_shape();

        let press = preview_ui::edit_press(
            liveness,
            shape.is_some_and(|s| s.preview_active),
            EditPlacement {
                pane_count: shape.map_or(0, |s| s.pane_count),
                active_is_preview: shape.is_some_and(|s| s.preview_active),
                other_editor_panes: shape.map_or(0, |s| s.other_editor_panes),
            },
        );

        match press {
            EditPress::Refuse(reason) => {
                eprintln!("wolf-gnome: source not edited: {reason}");
            }
            EditPress::Edit(mode) => {
                let core_ref = self.inner.core.borrow();
                if let Some(core) = core_ref.as_ref() {
                    // Discarded for the same reason `open_preview`'s is: the pane the
                    // source lands in arrives with the next wake's notifications.
                    let _ = core.edit_source(mode);
                }
            }
        }
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

    /// Reacts to a render wake: settles a pending re-root, then drains and
    /// dispatches every queued command.
    ///
    /// The app calls this LAST on each wake (after the pane view and the tree),
    /// because a drained `close_window` tears the core down. Unlike the paint, it
    /// runs in ALL THREE liveness states — see the module docs.
    pub fn on_render_wake(&self) {
        // ONE read of the tracked-cwd cell per wake, in its own statement, shared by
        // the re-root settle and the title. `WolfCore::live_cwd` is a `parking_lot`
        // read of a tracked cell, never an RPC, so the blocking-RPC inventory is
        // unchanged by the sidebar's re-root — see the module docs.
        let live = {
            let core_ref = self.inner.core.borrow();
            core_ref.as_ref().and_then(WolfCore::live_cwd)
        };
        self.settle_reroot(live.as_deref());
        self.refresh_title(live.as_deref());
        self.refresh_nav_buttons();
        self.inner.state.on_render_wake();
        // LAST: a drained `close_window` tears the core down, so the state
        // snapshot above must already have happened.
        self.drain_and_dispatch_commands();
    }

    /// Sets the window title from `live` — the captive nvim's working directory,
    /// read once per wake by [`AppShell::on_render_wake`] — through the pure
    /// [`title::window_title`] seam.
    ///
    /// The cwd is passed IN rather than read here, so the re-root settle and the
    /// title share one read; `home` was captured once at [`build`] time, so this
    /// costs a string compare on a steady-state frame.
    ///
    /// **The `last_title` cache is not cosmetic.** `set_title` on every frame is
    /// pointless GTK churn; more to the point, the cage title smoke's red-run
    /// revert is the deletion of the single `set_title` call below, so that call
    /// has to be the only place a title is ever pushed.
    fn refresh_title(&self, live: Option<&std::path::Path>) {
        let cwd = live.map(|cwd| cwd.to_string_lossy().into_owned());
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

    /// Performs one palette-row activation: a registry command through the same
    /// [`UiAction`] funnel the drain uses, or a recent folder through
    /// [`AppShell::chdir_to`].
    ///
    /// **This is the one place a recent folder becomes a window action.** It
    /// re-roots this window, because there is one window to re-root, and it reaches
    /// it through `chdir_to` so an Open Recent activation is indistinguishable from
    /// picking the same folder in the native Open Folder dialog — one
    /// canonicalization, one [`InputEvent::Chdir`], one pending re-root, one settle.
    ///
    /// Exhaustive with no `_` arm, so a new [`PaletteAction`] variant is a compile
    /// error here. The `Unsupported` arm is unreachable while
    /// `palette::PaletteInner::activate_selection`'s own refusal stands, and is
    /// printed rather than dropped for the same reason [`AppShell::perform`]'s
    /// `Deferred | NoGnomeEquivalent` arm is.
    fn on_palette_activation(&self, action: PaletteAction) {
        match action {
            PaletteAction::Dispatch(command) => self.dispatch_command(command),
            PaletteAction::OpenRecentFolder(path) => self.chdir_to(path),
            PaletteAction::Unsupported(reason) => {
                eprintln!("wolf-gnome: palette activation not performed: {reason}");
            }
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
            UiAction::HistoryBack => self.navigate(false),
            UiAction::HistoryForward => self.navigate(true),
            UiAction::ToggleSidebar => self.toggle_sidebar(),
            UiAction::CopySelection => self.copy_selection(),
            UiAction::PasteClipboard => self.paste_clipboard(),
            UiAction::ShowAbout => about::present(&self.inner.window),
            UiAction::OpenFileDialog => self.open_file_dialog(),
            UiAction::OpenFolderDialog => self.open_folder_dialog(),
            UiAction::NewUntitledBuffer => self.send_input(InputEvent::NewBuffer),
            UiAction::ClosePane => self.send_input(InputEvent::ClosePane),
            // Over the shared ordered input channel and not `WolfCore::toggle_full_width`,
            // which is itself a one-line `send_input` onto the same channel: same path,
            // and the neighbouring arms' shape wins.
            UiAction::ToggleFullWidth => self.send_input(InputEvent::ToggleFullWidth),
            UiAction::CloseWindow => self.close_window(),
            UiAction::MinimizeWindow => self.inner.window.minimize(),
            UiAction::QuitApplication => self.quit_application(),
            UiAction::OpenPreview => self.open_preview(),
            UiAction::EditSource => self.edit_source(),
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
    /// [`AppShell::settle_reroot`].
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
    ///
    /// Two producers reach this: the native Open Folder dialog and an Open Recent
    /// palette row ([`AppShell::on_palette_activation`]). Both settle through
    /// [`AppShell::settle_reroot`], which is what makes an Open Recent activation
    /// indistinguishable from picking the same folder in the dialog.
    ///
    /// A failed `:tcd` leaves the pending re-root set until the next successful
    /// chdir — `wolf_core::input` degrades a failed apply to a `tracing::warn!`, so
    /// Neovim never reports `DirChanged`. That stranded flag blocks nothing:
    /// [`workspace::reroot_decision`] roots at the LIVE cwd regardless of it, and
    /// discards it the moment nvim ever reports that directory.
    fn chdir_to(&self, path: PathBuf) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        self.send_input(InputEvent::Chdir { path: path.clone() });
        *self.inner.pending_reroot.borrow_mut() = Some(path);
    }

    /// Settles the sidebar's root against `live`, nvim's working directory: clears
    /// the empty state and re-roots when they have diverged, and discards a
    /// satisfied front-end request. Run on every wake, BEFORE the command drain.
    ///
    /// **The ONE settle point**, shared by all three producers — the Open Folder
    /// dialog, an Open Recent palette row, and a `:cd` typed into Neovim — which is
    /// what stops them diverging in what the sidebar does afterwards. The whole
    /// decision is the pure [`workspace::reroot_decision`]; this function only
    /// performs it. The match is exhaustive with no catch-all arm, so a new
    /// [`workspace::RerootAction`] variant is a compile error here.
    fn settle_reroot(&self, live: Option<&std::path::Path>) {
        // Both borrows are cloned out in their own statements: the arms below write
        // back to `pending_reroot` and call `set_empty_state`, which touches widgets.
        let pending = self.inner.pending_reroot.borrow().clone();
        let rooted = self.inner.rooted_cwd.borrow().clone();
        match workspace::reroot_decision(pending.as_deref(), live, rooted.as_deref()) {
            workspace::RerootAction::Stay => {}
            workspace::RerootAction::ClearPending => {
                *self.inner.pending_reroot.borrow_mut() = None;
            }
            workspace::RerootAction::Reroot {
                target,
                clear_pending,
            } => {
                if clear_pending {
                    *self.inner.pending_reroot.borrow_mut() = None;
                }
                // Updated in the SAME settle as the re-root, which is what makes the
                // next settle answer `Stay` and one chdir exactly one re-root.
                *self.inner.rooted_cwd.borrow_mut() = Some(target.to_path_buf());
                self.set_empty_state(false);
                self.inner.tree.reroot(&workspace::root_label(target));
            }
        }
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
