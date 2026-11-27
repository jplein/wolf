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
//! ## Two shell entry points per tab wake
//!
//! Every tab has its own wake channel, and its drain runs
//! [`AppShell::on_tab_wake`] with that tab. Inside it,
//! [`AppShell::on_liveness_wake`] runs FIRST — before the tab's pane view and the
//! tree — and [`AppShell::on_render_wake`] runs LAST. They are not
//! interchangeable:
//!
//! - [`AppShell::on_liveness_wake`] reads the tab's captive nvim's
//!   [`wolf_core::Liveness`] ONCE, applies the pure [`crate::liveness_ui()`]
//!   mapping, drives the crash/stall overlay when the tab is the selected one,
//!   schedules an automatic restart for a `Dead` core whatever its selection, and
//!   RETURNS whether the pane view and the tree should paint at all. It must run
//!   before any handler that can early-return: liveness is LEVEL-triggered (an
//!   atomic read), and putting the read behind `pane_view::on_render_wake`'s
//!   `core.is_none()` guard would reintroduce an edge-triggered swallow.
//! - [`AppShell::on_render_wake`] commits the window's state, and for the SELECTED
//!   tab alone reads its Neovim's LIVE working directory ONCE, settles the
//!   sidebar's root against it ([`AppShell::settle_reroot`] over the pure
//!   [`workspace::reroot_decision`]), refreshes the window TITLE from that same
//!   value ([`AppShell::refresh_title`] over the pure [`crate::title`] seam),
//!   refreshes the header bar's and tab bar's background
//!   ([`AppShell::refresh_titlebar`] over the pure [`mod@crate::titlebar_background`]
//!   seam), refreshes the two navigation buttons' sensitivity from the FOCUSED pane
//!   ([`AppShell::refresh_nav_buttons`] over the pure [`crate::nav_ui`] seam — the
//!   wake a `WinEnter` fires bumps no `seq`, so this is what makes the toolbar
//!   follow the focus), and then drains that tab's
//!   [`wolf_core::WolfCore::drain_commands`] and dispatches each command. A command
//!   `wolf-gnome` does not implement is PRINTED with its recorded reason — never
//!   silently dropped. The drain runs LAST because a drained `close_window`, or a
//!   `close_tab` on the window's last tab, shuts every core down, and the state
//!   commit must happen before that.
//!
//! **Commands drain from the selected tab only**, so every drained command acts on
//! the tab whose Neovim queued it: the window-level actions it reaches resolve the
//! selected tab. A background tab's queue waits for its selection, and the switch
//! wakes it ([`AppShell::on_tab_selected`]).
//!
//! The drain keeps running in ALL THREE liveness states. The drain is the ONLY
//! path by which a `close_window` / `close_tab` / `quit_application` reaches
//! `window.close()`, so a command queued in the instant before a crash would
//! otherwise be stranded behind the crash overlay with no way out but the window
//! manager. Reading the queue issues no RPC; some dispatched commands do.
//!
//! ## The restart orchestration
//!
//! [`AppShell::restart_nvim`] is the ONE path both the automatic (`Dead`) restart
//! and the overlay button take, and it restarts ONE tab: its own Neovim, its own
//! pane view and its own terminals, leaving every other tab untouched. It captures
//! the Tier-1 open-file mirror BEFORE calling [`wolf_core::WolfCore::restart_nvim`]
//! (a respawn wipes the mirror), then on an actual respawn resets that tab's pane
//! view and reopens the captured files with the ACTIVE one LAST. The overlay and the
//! banner follow the selected tab alone, so a background tab's crash restarts it
//! silently. Automatic attempts are capped at
//! [`crate::liveness_ui::MAX_AUTO_RESTART_ATTEMPTS`] per crash and per tab; the
//! overlay's button restarts the selected tab and RESETS its counter, because the
//! cap exists to stop a retry loop, never to refuse a user.
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
//! Two palette activations are NOT `Command`s, and [`AppShell::on_palette_activation`]
//! is the one place each becomes a window action. An Open Recent row carries a
//! directory and opens it, through the same [`AppShell::open_folder`] the native
//! Open Folder dialog calls. A Switch to Window row carries another window's id and
//! goes to the coordinator, which presents that window as it stands. The pair is
//! deliberate: for one directory offered as both, Open Recent focuses the tab
//! `specs/application-state.md` §4's rule picks, which can be a background tab.
//!
//! ## Borrow discipline (load-bearing)
//!
//! `drain_and_dispatch_commands` takes a SHORT `core.borrow()`, collects the
//! commands into a `Vec`, and drops the borrow before dispatching any of them.
//! `UiAction::CloseWindow` / `UiAction::QuitApplication` synchronously run
//! `window::wire_close`'s `close-request` handler, which first borrows each tab's core
//! for its unsaved check and, unless it refuses, then takes every tab's core out with
//! a `borrow_mut()`. `UiAction::CloseTab` borrows its tab's core for the ask, then
//! may take it out, and on the window's last tab it runs the same handler. An
//! outstanding `borrow()` there is a `RefCell` panic, not a warning.
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
//! because it is the one per-wake core read this module makes:
//! `WolfCore::live_cwd` is a `parking_lot` read of a tracked cell, taken once per
//! wake in [`AppShell::on_render_wake`] and passed to both consumers.
//!
//! ## The blocking calls
//!
//! The blocking core calls this crate can reach, and the gate in front of each, are
//! inventoried in `notes/wolf-gnome/crash-and-stall-recovery.md`; the restart path
//! issues none of them. This module issues these, each gated on the selected
//! tab's core reading `Alive` with the buffer picker's argument and its limits: it
//! cannot prevent the FIRST stall and must not be strengthened into something that
//! never raises one; what it prevents is re-probing a Neovim already known to be
//! wedged.
//!
//! - The NAVIGATION press: [`AppShell::navigate`] funnels through
//!   `WolfCore::history_back` / `history_forward`, gated by
//!   [`crate::nav_ui::nav_press`]. A press against a LIVE core with nothing to go
//!   back to issues no RPC at all, because the nav mutex answers `None` before the
//!   RPC is reached, so the button sensitivity ([`crate::nav_ui::nav_buttons`]) is
//!   an affordance and the liveness arm is the guard.
//! - The COPY yank: [`AppShell::copy_selection`] funnels through
//!   `WolfCore::copy_visual_selection`, gated by
//!   [`crate::clipboard_ui::copy_decision`]; in a terminal pane Copy reads the
//!   session's selection, which issues no RPC and is not gated on liveness.
//! - The MARKDOWN PREVIEW's: [`AppShell::open_preview`] and
//!   [`AppShell::edit_source`] funnel through `WolfCore::open_preview` /
//!   `WolfCore::edit_source`, gated through [`crate::preview_ui`]
//!   ([`preview_ui::preview_press`], [`preview_ui::edit_press`]), and
//!   [`AppShell::preview_in_browser`] renders the document for the loopback server
//!   through `WolfCore::open_in_browser`, gated by [`preview_ui::browser_press`].
//!   Neither press has an [`crate::actions`] accelerator row, so their only routes
//!   are the command palette and the `:WolfPreview` / `:WolfEdit` Ex commands.
//!
//! PASTE adds none: `gdk::Clipboard::read_text_async` resolves on a later main-loop
//! turn and `WolfCore::paste` is fire-and-forget on the ordered input channel, so
//! nothing on that path can time out. [`crate::clipboard_ui::paste_decision`] gates
//! it anyway, for a reason that is not about blocking — see that module's docs.
//!
//! A TAB SWITCH adds one, on the tab it leaves rather than the selected one:
//! [`AppShell::on_tab_selected`] captures that tab's panes, which reads each edit pane's
//! cursor through `WolfCore::window_cursor`, gated by [`state_seam::may_capture_cursor`]
//! on that tab's own core.
//!
//! NEW TAB adds none beyond that switch: it starts a new Neovim process for the new tab and
//! asks the selected one nothing, so it has no liveness gate. Starting a tab's core stays
//! outside the inventory, as starting a window's does.
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
//! SAVE and SAVE AS add none either. Both send on the ordered input channel
//! (`InputEvent::Save` / `InputEvent::SaveAs`) and the native dialog resolves on a
//! later main-loop turn, so nothing on either path can time out.
//! [`crate::save_ui::save_press`] and [`crate::save_ui::save_as_press`] gate them
//! anyway, for [`crate::clipboard_ui::paste_decision`]'s reason and NOT the blocking
//! one: a save queued behind an opaque crash overlay would be applied by a RESTARTED
//! Neovim to whatever buffer it happens to be on. Save As adds a second reason that
//! must not be collapsed into the first — a native modal over the stall overlay
//! covers the Restart button, which is the only route out of a stall.
//!
//! The sidebar's search panel ([`crate::search_panel`]) adds NO blocking call of its
//! own either, and that is worth stating because the symmetry with the buffer picker
//! suggests otherwise: `start_search` resolves its root from the tracked-cwd cell
//! and `spawn_blocking`s, `open_at` is fire-and-forget on the ordered input channel,
//! and `liveness` is an atomic read. A result activated over a preview pane reaches
//! `WolfCore::preview_file`, behind [`crate::search_panel::activation_route`]'s
//! `Alive` arm. `WolfCore::ignorecase()` IS a `block_on_rpc` and is deliberately
//! never called from this crate.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};

use wolf_core::{Command, InputEvent, Liveness, SplitOrientation, TabFiles, TerminalId, WolfCore};

use crate::about;
use crate::actions::{self, BindingStatus};
use crate::buffer_picker::BufferPickerHandle;
use crate::clipboard_ui::{self, CopyPress, PastePress};
use crate::command_action::{ui_action_for, ui_action_for_front_end_action, UiAction};
use crate::coordinator::{self, CoordinatorInner, WindowId};
use crate::crash_overlay::CrashOverlay;
use crate::frame_metrics;
use crate::frame_stats::Bucket;
use crate::liveness_ui::{
    liveness_ui, reopen_order, restart_outcome, terminal_restore, OverlayMode, RestartOutcome,
    MAX_AUTO_RESTART_ATTEMPTS,
};
use crate::nav_ui::{self, NavAvailability, NavPress};
use crate::other_pane_seam::Side;
use crate::palette::{PaletteAction, PaletteHandle};
use crate::pane_view;
use crate::picker::PickerHandle;
use crate::preview_ui::{self, BrowserPress, EditPress, PreviewPress};
use crate::save_ui::{self, SaveAsPress, SavePress, SaveTarget};
use crate::search_panel::{self, SearchPanelHandle, ShowDecision};
use crate::sidebar::{SidebarHandle, SidebarPage};
use crate::sidebar_toggles::{self, ToggleClick};
use crate::state_seam;
use crate::tab_view::{CoreCell, TabHandle, TabParts, TabsHandle};
use crate::title;
use crate::titlebar_background::{titlebar_background, TitlebarStyle};
use crate::tree_view::TreeHandle;
use crate::window::{self, TREE_PANE_WIDTH};
use crate::workspace;

/// The action the empty state's button and the primary menu use to pick a folder.
const OPEN_FOLDER_ACTION: &str = "win.open-folder";

/// The action that opens the in-window file picker (disabled in the empty state).
pub(crate) const GO_TO_FILE_ACTION: &str = "win.go-to-file";

/// The action that shows the sidebar's project-search panel (disabled in the empty
/// state, where the search root would fall back to the home directory).
const SEARCH_IN_PROJECT_ACTION: &str = "win.search-in-project";

/// The action that moves the focused pane one entry BACK in its navigation
/// history. Also the header bar's Back button target.
pub(crate) const HISTORY_BACK_ACTION: &str = "win.history-back";

/// The action that moves the focused pane one entry FORWARD in its navigation
/// history. Also the header bar's Forward button target.
pub(crate) const HISTORY_FORWARD_ACTION: &str = "win.history-forward";

/// The action that shows or hides the file-tree sidebar (disabled in the empty
/// state, where there is no project to list).
const TOGGLE_SIDEBAR_ACTION: &str = "win.toggle-sidebar";

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

/// Title of the save-as dialog.
const SAVE_DIALOG_TITLE: &str = "Save As";

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

/// Why an other-pane press issued nothing: the active pane is no leaf of the pane tree.
const NO_OTHER_PANE_REASON: &str = "the active pane is not an editor pane of the pane tree";

/// Everything the two Markdown-preview presses are decided against, read from ONE
/// snapshot pass by [`AppShell::preview_shape`].
///
/// One value rather than two reads, because the two must describe the SAME instant:
/// Preview and Edit are inverses, so a `markdown_active` taken from one snapshot and
/// a `preview_active` from another can name a pane that is both and a pane that is
/// neither.
#[derive(Clone, Copy)]
struct PreviewShape {
    /// `Snapshot::is_markdown_active` — the Preview press's buffer-kind gate.
    markdown_active: bool,
    /// `Snapshot::is_preview_active` — the Edit press's gate.
    preview_active: bool,
}

/// Everything the two save presses are decided against, read from ONE snapshot pass
/// by [`AppShell::save_shape`].
///
/// [`PreviewShape`]'s precedent, and its reason: the three must describe the SAME
/// instant. A path taken from one snapshot and a preview flag from another can name a
/// shape that never existed, and [`save_ui::save_press`] would then answer `Write`
/// for a pane that cannot be written.
///
/// OWNED (a `PathBuf`, not a borrow), so no `SnapshotReadGuard` is held past the read.
#[derive(Clone)]
struct SaveShape {
    /// `Snapshot::current_file` — the focused buffer's path.
    path: Option<PathBuf>,
    /// `Snapshot::is_preview_active` — the focused pane is a preview scratch.
    is_preview: bool,
    /// `Snapshot::win_buffers[active_win].terminal` — the focused pane is a terminal
    /// pane.
    is_terminal: bool,
}

/// The widgets and handles [`build`] needs, as one struct rather than eight
/// positional parameters (`clippy::too_many_arguments` bites at seven).
pub struct ShellParts<'a> {
    /// The application window the `win.*` actions are installed on.
    pub window: &'a adw::ApplicationWindow,
    /// The file picker, opened by `win.go-to-file`.
    pub picker: &'a PickerHandle,
    /// The command palette, opened by `win.command-palette`.
    pub palette: &'a PaletteHandle,
    /// The buffer picker, opened by `win.buffer-picker`.
    pub buffers: &'a BufferPickerHandle,
    /// The sidebar's search panel, shown and focused by `win.search-in-project`.
    pub search: &'a SearchPanelHandle,
    /// The file-tree sidebar, re-rooted after a front-end directory change.
    pub tree: &'a TreeHandle,
    /// The sidebar container, hidden in the empty state and switched between its
    /// Files and Search pages.
    pub sidebar: &'a SidebarHandle,
    /// The window's outer `gtk::Paned` (sidebar | editor).
    pub paned: &'a gtk::Paned,
    /// The header bar's Back button, desensitized by
    /// [`crate::nav_ui::nav_buttons`] on every wake.
    pub nav_back_button: &'a gtk::Button,
    /// The header bar's Forward button; see [`ShellParts::nav_back_button`].
    pub nav_forward_button: &'a gtk::Button,
    /// The working directory the selected tab was launched in — the seed for
    /// [`ShellInner::rooted_cwd`].
    pub launch_cwd: PathBuf,
    /// The crash/stall overlay + banner, driven by the liveness read.
    pub crash_overlay: &'a CrashOverlay,
    /// This window's TABS. The empty state and the sidebar preference are each
    /// tab's own and are read off the selected tab here, every window-level action
    /// resolves the selected tab's core here, and so does the New Tab press. No cycle: [`crate::tab_view`] holds the shell's callbacks only
    /// through `Weak`s.
    pub tabs: &'a TabsHandle,
    /// The header bar's and tab bar's provider, fed on every render wake by
    /// [`AppShell::refresh_titlebar`].
    pub titlebar: &'a TitlebarStyle,
    /// The process-scoped coordinator, as a `Weak`: it owns this window's handle,
    /// and this shell is owned by that window. See [`mod@crate::coordinator`].
    pub coordinator: Weak<CoordinatorInner>,
    /// This window's identity, so the coordinator knows which window is waking or
    /// closing.
    pub window_id: WindowId,
    /// Whether the test-only readiness probe is pending, read once by
    /// `window::build`: only then does the end of each wake check readiness.
    pub ready_armed: bool,
}

/// The shell's shared state. All access happens on the single GTK main thread, so
/// `RefCell` (not a `Mutex`) is sufficient.
struct ShellInner {
    /// The application window (`win.*` action scope, dialog parent, close target).
    window: adw::ApplicationWindow,
    /// The file picker opened by `win.go-to-file`.
    picker: PickerHandle,
    /// The command palette opened by `win.command-palette`.
    palette: PaletteHandle,
    /// The buffer picker opened by `win.buffer-picker`.
    buffers: BufferPickerHandle,
    /// The sidebar's search panel, shown and focused by `win.search-in-project`.
    search: SearchPanelHandle,
    /// The file-tree sidebar, re-rooted by [`AppShell::settle_reroot`].
    tree: TreeHandle,
    /// The sidebar container; hidden in the empty state.
    sidebar: SidebarHandle,
    /// The outer `gtk::Paned` whose divider collapses in the empty state.
    paned: gtk::Paned,
    /// The header bar's Back button; its sensitivity is recomputed on every wake
    /// by [`AppShell::refresh_nav_buttons`].
    nav_back_button: gtk::Button,
    /// The header bar's Forward button; see [`ShellInner::nav_back_button`].
    nav_forward_button: gtk::Button,
    /// The `win.go-to-file` action, stashed by [`AppShell::install_actions`] so the
    /// empty state can disable it.
    go_to_file: RefCell<Option<gio::SimpleAction>>,
    /// The `win.search-in-project` action, stashed for the same reason.
    search_in_project: RefCell<Option<gio::SimpleAction>>,
    /// The `win.toggle-sidebar` action, stashed for the same reason.
    toggle_sidebar: RefCell<Option<gio::SimpleAction>>,
    /// The directory the sidebar is CURRENTLY rooted at, canonical. The authority
    /// [`workspace::reroot_decision`] compares nvim's live cwd against.
    ///
    /// Seeded in [`build`] from the canonicalized [`ShellParts::launch_cwd`].
    /// Canonicalizing the seed is load-bearing: `live_cwd` is already canonical
    /// (core's `apply_cwd` canonicalizes) while a restored directory need not be,
    /// and a textual mismatch would cost a spurious full re-root — a whole fresh
    /// traversal — on every launch.
    rooted_cwd: RefCell<Option<PathBuf>>,
    /// The crash/stall overlay + banner, driven by [`AppShell::on_liveness_wake`].
    crash_overlay: CrashOverlay,
    /// This window's tabs; see [`ShellParts::tabs`].
    tabs: TabsHandle,
    /// The header bar's and tab bar's provider; see [`ShellParts::titlebar`].
    titlebar: TitlebarStyle,
    /// The process-scoped coordinator, upgraded per use.
    coordinator: Weak<CoordinatorInner>,
    /// This window's identity, passed to every coordinator call.
    window_id: WindowId,
    /// The user's home directory, stringified ONCE at [`build`] time — the injected
    /// `home` for [`title::window_title`] and [`crate::tabs_ui::tab_label`]. Captured
    /// rather than read per frame because [`title`] deliberately never touches the
    /// environment, and `$HOME` does not change under a running process.
    home: String,
    /// The last title actually pushed to the window, so
    /// [`AppShell::refresh_title`] calls `set_title` only on a real change.
    last_title: RefCell<String>,
    /// See [`ShellParts::ready_armed`].
    ready_armed: bool,
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
/// [`crate::tab_view::TabHandle::set_empty`] shows it.
///
/// It is meant to be added with `gtk::Overlay::add_overlay` (the picker's
/// precedent) so the editor `DrawingArea` keeps its full allocation and no
/// spurious 1x1 grid resize happens while the empty state is up. The `background`
/// style class makes it opaque over the editor grid and gives libadwaita's colour
/// until the tab's first frame. After that, the tab's
/// [`crate::titlebar_background::EmptyStateStyle`] overrides the background colour alone.
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
/// window exists, before `present()`) and [`AppShell::apply_selected_tab`] next.
#[must_use]
pub fn build(parts: ShellParts<'_>) -> AppShell {
    let inner = Rc::new(ShellInner {
        window: parts.window.clone(),
        picker: parts.picker.clone(),
        palette: parts.palette.clone(),
        buffers: parts.buffers.clone(),
        search: parts.search.clone(),
        tree: parts.tree.clone(),
        sidebar: parts.sidebar.clone(),
        paned: parts.paned.clone(),
        nav_back_button: parts.nav_back_button.clone(),
        nav_forward_button: parts.nav_forward_button.clone(),
        go_to_file: RefCell::new(None),
        search_in_project: RefCell::new(None),
        toggle_sidebar: RefCell::new(None),
        // Canonicalized so the comparison against the core's already-canonical
        // `live_cwd` means what it says; see the field's docs.
        rooted_cwd: RefCell::new(Some(
            std::fs::canonicalize(&parts.launch_cwd).unwrap_or_else(|_| parts.launch_cwd.clone()),
        )),
        crash_overlay: parts.crash_overlay.clone(),
        tabs: parts.tabs.clone(),
        titlebar: parts.titlebar.clone(),
        coordinator: parts.coordinator,
        window_id: parts.window_id,
        home: workspace::home_dir()
            .map(|home| home.to_string_lossy().into_owned())
            .unwrap_or_default(),
        last_title: RefCell::new(String::new()),
        ready_armed: parts.ready_armed,
    });
    let shell = AppShell { inner };

    // The button and the `Dead` path share this; resetting the counter first means the cap stops a
    // retry loop, never a user. See notes/crates/wolf-gnome/src/shell.rs.md#one-press-two-respawns
    let weak = Rc::downgrade(&shell.inner);
    shell.inner.crash_overlay.connect_restart(move || {
        if let Some(shell) = upgrade(&weak) {
            if let Some(tab) = shell.inner.tabs.selected() {
                tab.restart().attempts.set(0);
                shell.restart_nvim(&tab);
            }
        }
    });

    // A tab switch closes all three overlays: one left over the next tab keeps `window::wire_keys`'
    // guard suppressing every key, and its focus-return lands in the strip it opened from.
    let weak_tabs = Rc::downgrade(&shell.inner);
    shell.inner.tabs.connect_selected(move |previous| {
        if let Some(shell) = upgrade(&weak_tabs) {
            shell.on_tab_selected(previous);
        }
    });

    let weak_wake = Rc::downgrade(&shell.inner);
    shell.inner.tabs.connect_wake(move |tab| {
        if let Some(shell) = upgrade(&weak_wake) {
            shell.on_tab_wake(tab);
        }
    });

    let weak_closing = Rc::downgrade(&shell.inner);
    shell.inner.tabs.connect_closing(move |tab| {
        if let Some(shell) = upgrade(&weak_closing) {
            shell.on_tab_closing(tab);
        }
    });

    let weak_toggles = Rc::downgrade(&shell.inner);
    shell.inner.sidebar.toggles().connect_click(move |clicked| {
        if let Some(shell) = upgrade(&weak_toggles) {
            shell.on_sidebar_toggle_click(clicked);
        }
    });

    // A palette row's command arm is the drain loop's dispatch, so the two cannot classify differently.
    let weak_palette = Rc::downgrade(&shell.inner);
    shell.inner.palette.connect_activate(move |action| {
        if let Some(shell) = upgrade(&weak_palette) {
            shell.on_palette_activation(action);
        }
    });

    shell
}

impl AppShell {
    /// Installs one `gio::SimpleAction` per IMPLEMENTED `win.`-namespace action
    /// of [`actions::ACTION_TABLE`], registers the accelerator of every
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
    /// **`app.`-namespace rows are deliberately NOT installed here.** They are
    /// [`crate::coordinator::Coordinator::install_app_actions`]', installed once
    /// for the process, because `gio::ActionMap::add_action` REPLACES an existing
    /// entry: installing them per window would leave the last window's closure
    /// owning the whole application's Quit, and closing that window would kill
    /// Ctrl+Shift+Q for every window still open. Nothing else moves — the chord
    /// controller's `activate_action(&window, "app.quit")` and
    /// [`AppShell::action_is_enabled`]'s `app.` lookup both resolve up the
    /// window's action muxer to the application, exactly as before.
    ///
    /// `set_accels_for_action` below still covers `app.` rows: it is idempotent,
    /// it is per-application rather than per-window, and it is what makes
    /// `gio::Menu` draw "Ctrl+Shift+Q" beside the Quit item.
    ///
    /// Must run AFTER the window is constructed and BEFORE `window.present()`.
    pub fn install_actions(&self, app: &gtk::Application) {
        for binding in actions::ACTION_TABLE {
            // The coordinator's (see the doc comment), checked before classification so an `app.` row never
            // reaches `window.add_action`.
            if binding.action.starts_with("app.") {
                continue;
            }
            // A row resolves through its command or, if front-end-only, `ui_action_for_front_end_action`.
            // One resolving in neither is skipped loudly: its registered chord would be swallowed silently.
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
            let Some(("win", name)) = binding.action.split_once('.') else {
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

            self.inner.window.add_action(&action);
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

    /// Applies the selected tab's empty state and sidebar preference to the
    /// window's widgets: the sidebar, its header-bar toggle group, and the
    /// `win.go-to-file`, `win.search-in-project` and `win.toggle-sidebar` enables. No
    /// selected tab reads as the empty state. Re-applied on every real selection
    /// change, since each tab has its own of both.
    ///
    /// Disabling `win.go-to-file` and `win.search-in-project` uses the documented
    /// global escape hatch: a disabled action's accelerator does not activate AND the
    /// event falls through, so in the empty state Ctrl+Shift+O reaches Neovim as
    /// `<C-O>` (identical to `<C-o>`) and Ctrl+Shift+F reaches it as `<C-F>`
    /// (identical to `<C-f>`, page forward). That is an accepted wart, recorded in
    /// the README.
    ///
    /// The search panel is guarded TWICE, and both halves are needed: the disabled
    /// action is what the chord and the menu item see, while
    /// [`crate::search_panel::show_decision`] over the selected tab's empty flag is
    /// what the command palette's row and `:WolfSearchInProject` meet in
    /// [`AppShell::perform`], since the palette offers a live row for every
    /// dispatched command.
    ///
    /// `win.terminal` is deliberately NOT among the actions disabled here, for the
    /// buffer picker's recorded reason: the captive Neovim runs in a project-less
    /// window too, and a drawer there is a working shell rather than a dead
    /// surface.
    ///
    /// It shows the selected tab's recorded sidebar page, and leaves the search panel's
    /// query, toggles and rows as they are: the panel is per window.
    pub fn apply_selected_tab(&self) {
        let empty = self.selected_is_empty();
        // Never `sidebar.set_visible` directly: a toggle and the empty state are two inputs to one rule,
        // or the next selection undoes the user's toggle.
        self.apply_sidebar_visibility();
        if let Some(tab) = self.inner.tabs.selected() {
            self.inner.sidebar.show_page(tab.sidebar_page());
        }
        self.inner.sidebar.toggles().set_sensitive(!empty);
        if let Some(action) = self.inner.go_to_file.borrow().as_ref() {
            action.set_enabled(!empty);
        }
        if let Some(action) = self.inner.search_in_project.borrow().as_ref() {
            action.set_enabled(!empty);
        }
        if let Some(action) = self.inner.toggle_sidebar.borrow().as_ref() {
            action.set_enabled(!empty);
        }
    }

    /// Records the window's sidebar page and search panel fields into the selected tab,
    /// so its saved sidebar follows the widgets. `window::WindowHandle::refresh_subtree`
    /// calls it before every gather of the window.
    pub fn record_selected_sidebar(&self) {
        if let Some(tab) = self.inner.tabs.selected() {
            tab.record_sidebar(self.inner.sidebar.page(), self.inner.search.fields());
        }
    }

    /// Applies the sidebar-visibility rule to the widgets. The ONE site that calls
    /// `sidebar.set_visible` / `paned.set_position`.
    ///
    /// `sidebar.set_visible(false)` is the load-bearing half: the `gtk::Paned`
    /// divider position is advisory (a start child merely positioned at 0 can still
    /// be re-derived by a later allocation), while an invisible start child is never
    /// allocated at all.
    ///
    /// Hiding the sidebar while focus is inside it, in the search panel's field, first
    /// parks focus on the selected tab's strip: hiding the focus widget leaves the
    /// toplevel with no focus widget at all, which kills window-level key dispatch
    /// outright.
    fn apply_sidebar_visibility(&self) {
        let wants = self
            .inner
            .tabs
            .selected()
            .is_some_and(|tab| tab.sidebar_shown());
        let shown = state_seam::sidebar_is_shown(wants, self.selected_is_empty());
        // Sampled before `set_visible`, for `apply_overlay`'s reason: a hidden focus widget reads as none.
        if !shown && self.inner.sidebar.focus_is_inside() {
            self.focus_selected_strip();
        }
        self.inner.sidebar.widget().set_visible(shown);
        self.inner
            .paned
            .set_position(if shown { TREE_PANE_WIDTH } else { 0 });
        // Relayed from the one site so the refresh gates match the screen: a refresh while hidden is
        // deferred, and re-showing owes exactly one catch-up.
        self.inner.sidebar.set_sidebar_visible(shown);
    }

    /// Gives keyboard focus to the SELECTED tab's cmdline strip: a background tab's
    /// is unmapped, and focusing an unmapped widget leaves the toplevel with no
    /// focus widget.
    fn focus_selected_strip(&self) {
        if let Some(tab) = self.inner.tabs.selected() {
            tab.focus_area().grab_focus();
        }
    }

    /// Whether the selected tab is in the empty state; `true` with no selected tab.
    fn selected_is_empty(&self) -> bool {
        self.inner.tabs.selected().is_none_or(|tab| tab.is_empty())
    }

    /// Flips the selected tab's sidebar preference; see
    /// [`AppShell::set_sidebar_preference`].
    fn toggle_sidebar(&self) {
        let shown = self
            .inner
            .tabs
            .selected()
            .is_some_and(|tab| tab.sidebar_shown());
        self.set_sidebar_preference(!shown);
    }

    /// Sets the selected tab's sidebar preference to `shown`, records it for
    /// persistence, and applies it.
    ///
    /// The preference is what is persisted and restored — not what is on screen —
    /// so a session spent in the empty state cannot come back with the tree hidden.
    /// The widget is per window (see `window::build`) but the preference is per tab,
    /// so only the selected tab's changes.
    fn set_sidebar_preference(&self, shown: bool) {
        if let Some(tab) = self.inner.tabs.selected() {
            tab.set_sidebar_shown(shown);
        }
        self.apply_sidebar_visibility();
    }

    /// Yanks the focused Neovim's visual-mode selection and writes it to the
    /// system clipboard, or prints why it did not. In a terminal pane it copies the
    /// session's selection instead.
    ///
    /// **The yank is a BLOCKING RPC on the GTK main thread**
    /// (`WolfCore::copy_visual_selection` over `block_on_rpc`), so it is gated by
    /// [`clipboard_ui::copy_decision`] exactly as the buffer picker's candidate
    /// snapshot and a navigation move are; see the module docs. A terminal's
    /// `WolfCore::terminal_selection_text` issues no RPC, which is why that arm is not.
    ///
    /// The core borrow is taken and dropped in a statement of its own, BEFORE the
    /// clipboard is touched. `clipboard_write` answering `None` means write
    /// NOTHING: `set_text("")` would clear the selection and destroy whatever the
    /// user copied from another application, which is why there is no `else` arm
    /// here.
    fn copy_selection(&self) {
        let liveness = self.read_liveness();
        let terminal = self.inner.tabs.active_terminal();

        match clipboard_ui::copy_decision(liveness, terminal) {
            CopyPress::Refuse(reason) => {
                eprintln!("wolf-gnome: copy not performed: {reason}");
            }
            CopyPress::Terminal(session) => {
                let selection = {
                    let cell = self.selected_core();
                    let core_ref = cell.borrow();
                    core_ref
                        .as_ref()
                        .and_then(|core| core.terminal_selection_text(session))
                };
                if let Some(text) = clipboard_ui::clipboard_write(selection.as_deref()) {
                    self.clipboard().set_text(text);
                }
            }
            CopyPress::Copy => {
                let selection = {
                    let cell = self.selected_core();
                    let core_ref = cell.borrow();
                    core_ref.as_ref().and_then(WolfCore::copy_visual_selection)
                };
                if let Some(text) = clipboard_ui::clipboard_write(selection.as_deref()) {
                    self.clipboard().set_text(text);
                }
            }
        }
    }

    /// Reads the system clipboard and inserts it through `nvim_paste`, or into the
    /// active terminal pane's session through `WolfCore::terminal_paste`, or prints
    /// why it did not. The target — the selected tab's core and its session — is
    /// captured at the press, so a focus change or a tab switch while the read is in
    /// flight cannot redirect the text.
    ///
    /// **Nothing on this path blocks**, so it adds no entry to the module docs'
    /// blocking inventory: `gdk::Clipboard::read_text_async` hands GLib a
    /// trampoline and returns immediately, and `WolfCore::paste` is a send on the
    /// ordered input channel. [`clipboard_ui::paste_decision`] gates it for a
    /// different reason, which that module's docs state and which must not be
    /// collapsed into the copy gate's.
    ///
    /// The callback holds that core's cell as a `Weak` and borrows it in a statement
    /// of its own, so a read landing after the tab's core has been taken out
    /// performs nothing rather than colliding with its `borrow_mut`.
    /// An `Err` is printed, never swallowed, and then flattened to `None` for the
    /// pure rule.
    fn paste_clipboard(&self) {
        let liveness = self.read_liveness();

        match clipboard_ui::paste_decision(liveness) {
            PastePress::Refuse(reason) => {
                eprintln!("wolf-gnome: paste not performed: {reason}");
            }
            PastePress::Read => {
                let terminal = self.inner.tabs.active_terminal();
                let core = Rc::downgrade(&self.selected_core());
                self.clipboard()
                    .read_text_async(gio::Cancellable::NONE, move |result| {
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
                        let Some(cell) = core.upgrade() else {
                            return;
                        };
                        let core_ref = cell.borrow();
                        if let Some(core) = core_ref.as_ref() {
                            match terminal {
                                Some(session) => core.terminal_paste(session, text),
                                None => core.paste(text.to_owned()),
                            }
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
                let cell = self.selected_core();
                let core_ref = cell.borrow();
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
                let cell = self.selected_core();
                let core_ref = cell.borrow();
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
        let cell = self.selected_core();
        let core_ref = cell.borrow();
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
    /// [`Snapshot::is_markdown_active`]: wolf_core::Snapshot::is_markdown_active
    /// [`Snapshot::is_preview_active`]: wolf_core::Snapshot::is_preview_active
    fn preview_shape(&self) -> Option<PreviewShape> {
        let cell = self.selected_core();
        let core_ref = cell.borrow();
        let core = core_ref.as_ref()?;
        let snapshot = core.read_snapshot();
        Some(PreviewShape {
            markdown_active: snapshot.is_markdown_active(),
            preview_active: snapshot.is_preview_active(),
        })
    }

    /// Binds a Markdown preview of the active buffer into the active pane, or prints
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

        let press = preview_ui::preview_press(liveness, shape.is_some_and(|s| s.markdown_active));

        match press {
            PreviewPress::Refuse(reason) => {
                eprintln!("wolf-gnome: preview not opened: {reason}");
            }
            PreviewPress::Open => {
                let cell = self.selected_core();
                let core_ref = cell.borrow();
                if let Some(core) = core_ref.as_ref() {
                    // The bufnr is discarded: the next wake's `wolf_preview` notification carries the new bind.
                    let _ = core.open_preview();
                }
            }
        }
    }

    /// Returns the active preview pane to its source file, or prints why it did
    /// not.
    ///
    /// **The return is a BLOCKING RPC on the GTK main thread**
    /// (`WolfCore::edit_source` over `block_on_rpc`), gated by
    /// [`preview_ui::edit_press`]. [`AppShell::open_preview`]'s shape, with the
    /// polarity inverted where the seam inverts it.
    fn edit_source(&self) {
        let liveness = self.read_liveness();
        let shape = self.preview_shape();

        let press = preview_ui::edit_press(liveness, shape.is_some_and(|s| s.preview_active));

        match press {
            EditPress::Refuse(reason) => {
                eprintln!("wolf-gnome: source not edited: {reason}");
            }
            EditPress::Edit => {
                let cell = self.selected_core();
                let core_ref = cell.borrow();
                if let Some(core) = core_ref.as_ref() {
                    // Discarded for the same reason `open_preview`'s is: the bind the
                    // source lands in arrives with the next wake's notifications.
                    let _ = core.edit_source();
                }
            }
        }
    }

    /// Binds the active buffer's Markdown preview into ANOTHER pane, leaving the
    /// source on screen and focused, or prints why it did not.
    ///
    /// **The bind is a BLOCKING RPC on the GTK main thread**
    /// (`WolfCore::preview_in_other` over `block_on_rpc`), gated by
    /// [`preview_ui::preview_press`] — the SAME decision [`AppShell::open_preview`]
    /// is gated by, because the two differ only in which pane receives the bind and
    /// not in whether the buffer can be previewed at all.
    ///
    /// The pane is [`crate::other_pane_seam::target`]'s with [`Side::Right`], resolved by the
    /// selected tab's pane view: the pane to the right of the active one, else the next
    /// leaf, else a `target_win` of `0`, which asks core to split a lone pane. With no
    /// tree leaf active, such as the terminal drawer, it prints why and issues nothing.
    /// Core's helper returns focus to the source; a split surfaces as an ordinary
    /// vertical `Split` layout intent on the next wake.
    fn preview_in_other_pane(&self) {
        let liveness = self.read_liveness();
        let shape = self.preview_shape();

        let press = preview_ui::preview_press(liveness, shape.is_some_and(|s| s.markdown_active));

        match press {
            PreviewPress::Refuse(reason) => {
                eprintln!("wolf-gnome: preview not opened in another pane: {reason}");
            }
            PreviewPress::Open => {
                let Some(target) = self.other_pane_target(Side::Right) else {
                    eprintln!(
                        "wolf-gnome: preview not opened in another pane: {NO_OTHER_PANE_REASON}"
                    );
                    return;
                };
                let cell = self.selected_core();
                let core_ref = cell.borrow();
                if let Some(core) = core_ref.as_ref() {
                    // Discarded for `open_preview`'s reason: the pane's new contents, or
                    // the new pane, arrive with the next wake's notifications.
                    let _ = core.preview_in_other(target);
                }
            }
        }
    }

    /// Binds the active preview's source into ANOTHER pane and moves focus there,
    /// leaving the preview on screen, or prints why it did not.
    ///
    /// [`AppShell::preview_in_other_pane`]'s inverse, gated by
    /// [`preview_ui::edit_press`]: the same target rule with [`Side::Left`], so the pane
    /// to the left, else the previous leaf, else a split. Core's helper moves focus to the
    /// pane that receives the source.
    fn edit_in_other_pane(&self) {
        let liveness = self.read_liveness();
        let shape = self.preview_shape();

        let press = preview_ui::edit_press(liveness, shape.is_some_and(|s| s.preview_active));

        match press {
            EditPress::Refuse(reason) => {
                eprintln!("wolf-gnome: source not edited in another pane: {reason}");
            }
            EditPress::Edit => {
                let Some(target) = self.other_pane_target(Side::Left) else {
                    eprintln!(
                        "wolf-gnome: source not edited in another pane: {NO_OTHER_PANE_REASON}"
                    );
                    return;
                };
                let cell = self.selected_core();
                let core_ref = cell.borrow();
                if let Some(core) = core_ref.as_ref() {
                    let _ = core.edit_in_other(target);
                }
            }
        }
    }

    /// The `target_win` an other-pane press passes to core, from the selected tab's pane
    /// view, or `None` when the press must be a no-op.
    fn other_pane_target(&self, prefer: Side) -> Option<i64> {
        self.inner
            .tabs
            .selected()
            .and_then(|tab| tab.panes().other_pane_target(prefer))
    }

    /// Serves the active Markdown buffer from core's loopback preview server and
    /// hands its address to the platform's URI launcher, or prints why it did not.
    ///
    /// **The render is a BLOCKING RPC on the GTK main thread**
    /// (`WolfCore::open_in_browser` renders the document and binds the loopback
    /// socket), so it is gated by [`preview_ui::browser_press`] exactly as
    /// [`AppShell::open_preview`] is gated by `preview_press`; see the module docs.
    ///
    /// **The two refusal lines are deliberately DISTINGUISHABLE**, and that
    /// separation is what makes a captured stderr log readable. A `BrowserPress::Refuse`
    /// says the RPC was never issued and names the gate that stopped it; the
    /// [`preview_ui::browser_url`] refusal says the RPC RAN and answered an empty
    /// string, which core has already logged the real cause of at `warn`. Collapsing
    /// them into one message would make "the gate declined" and "core failed" the same
    /// observation.
    ///
    /// The launch itself is asynchronous and its failure is printed by
    /// [`pane_view::launch_uri`], the same function a foreign-scheme link clicked
    /// inside the preview goes through.
    fn preview_in_browser(&self) {
        let liveness = self.read_liveness();
        let shape = self.preview_shape();

        let press = preview_ui::browser_press(liveness, shape.is_some_and(|s| s.markdown_active));

        match press {
            BrowserPress::Refuse(reason) => {
                eprintln!("wolf-gnome: preview in browser not opened: {reason}");
            }
            BrowserPress::Open => {
                // The core borrow is taken in a statement of its own and dropped
                // before the launcher runs, `copy_selection`'s discipline.
                let url = {
                    let cell = self.selected_core();
                    let core_ref = cell.borrow();
                    core_ref.as_ref().map(WolfCore::open_in_browser)
                };
                let url = url.unwrap_or_default();
                match preview_ui::browser_url(&url) {
                    Some(url) => pane_view::launch_uri(
                        Some(self.inner.window.upcast_ref::<gtk::Window>()),
                        url,
                        "preview in browser",
                    ),
                    None => eprintln!(
                        "wolf-gnome: preview in browser not opened: the loopback preview server \
                         answered an empty address, which is what open_in_browser returns when the \
                         render RPC failed, the buffer was not Markdown, or the socket could not \
                         be bound — core has already logged which at warn"
                    ),
                }
            }
        }
    }

    /// The whole buffer shape both save presses are decided against, read in ONE
    /// snapshot pass — or `None` when there is no core.
    ///
    /// [`AppShell::read_active_win`]'s idiom: one core borrow, one
    /// `SnapshotReadGuard` bound to a local so it is dropped at the end of THIS
    /// function, before the caller takes another core lock.
    ///
    /// The flags are read off the `Snapshot` already in hand rather than through
    /// `WolfCore::is_preview_active()`, for [`AppShell::preview_shape`]'s recorded
    /// reason: that method takes its own guard, and a re-entrant read can deadlock
    /// behind a queued writer.
    fn save_shape(&self) -> Option<SaveShape> {
        let cell = self.selected_core();
        let core_ref = cell.borrow();
        let core = core_ref.as_ref()?;
        let snapshot = core.read_snapshot();
        Some(SaveShape {
            path: snapshot.current_file.clone(),
            is_preview: snapshot.is_preview_active(),
            is_terminal: snapshot
                .active_win
                .and_then(|win| snapshot.win_buffers.get(&win))
                .is_some_and(|buffer| buffer.terminal.is_some()),
        })
    }

    /// Writes the focused buffer, opens the save dialog for a nameless one, or prints
    /// why it did neither.
    ///
    /// **Neither branch blocks.** `InputEvent::Save` is fire-and-forget on the ordered
    /// input channel and the dialog resolves on a later main-loop turn, so this adds
    /// no blocking call; see the module docs for why it is gated anyway.
    ///
    /// [`AppShell::open_preview`]'s shape: the two reads happen first, each in its own
    /// short borrow, before anything else touches the core.
    fn save_buffer(&self) {
        let liveness = self.read_liveness();
        let shape = self.save_shape();

        let press = save_ui::save_press(
            liveness,
            SaveTarget {
                path: shape.as_ref().and_then(|s| s.path.as_deref()),
                is_preview: shape.as_ref().is_some_and(|s| s.is_preview),
                is_terminal: shape.as_ref().is_some_and(|s| s.is_terminal),
            },
        );

        match press {
            SavePress::Write => self.send_input(InputEvent::Save),
            SavePress::Choose => self.open_save_dialog(shape.as_ref()),
            SavePress::Refuse(reason) => {
                eprintln!("wolf-gnome: save not performed: {reason}");
            }
        }
    }

    /// Opens the save dialog to give the focused buffer a new path, or prints why it
    /// did not.
    ///
    /// [`AppShell::save_buffer`]'s shape over [`save_ui::save_as_press`], whose
    /// accepted arm is always the dialog: a named buffer is what Save As exists to
    /// rename.
    fn save_buffer_as(&self) {
        let liveness = self.read_liveness();
        let shape = self.save_shape();

        let press = save_ui::save_as_press(
            liveness,
            SaveTarget {
                path: shape.as_ref().and_then(|s| s.path.as_deref()),
                is_preview: shape.as_ref().is_some_and(|s| s.is_preview),
                is_terminal: shape.as_ref().is_some_and(|s| s.is_terminal),
            },
        );

        match press {
            SaveAsPress::Choose => self.open_save_dialog(shape.as_ref()),
            SaveAsPress::Refuse(reason) => {
                eprintln!("wolf-gnome: save as not performed: {reason}");
            }
        }
    }

    /// Opens a native save chooser at [`save_ui::save_dialog_proposal`]'s folder and
    /// name; on success asks the core to `saveas!` the chosen path.
    ///
    /// [`AppShell::open_file_dialog`]'s shape exactly, with the proposal carried in
    /// from the seam — this function decides nothing, including the `force` flag,
    /// which comes out of [`save_ui::save_as_target`] with its reason.
    fn open_save_dialog(&self, shape: Option<&SaveShape>) {
        // Cloned out in its own statement, the borrow discipline the rest of this
        // module follows; it needs no core borrow at all and follows every re-root.
        let rooted = self.inner.rooted_cwd.borrow().clone();
        let proposal =
            save_ui::save_dialog_proposal(shape.and_then(|s| s.path.as_deref()), rooted.as_deref());

        let mut builder = gtk::FileDialog::builder()
            .title(SAVE_DIALOG_TITLE)
            .modal(true)
            .initial_name(proposal.name);
        if let Some(dir) = proposal.folder {
            builder = builder.initial_folder(&gio::File::for_path(dir));
        }
        let dialog = builder.build();

        let weak = Rc::downgrade(&self.inner);
        dialog.save(
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
                        if let Some(request) = save_ui::save_as_target(&path) {
                            shell.send_input(InputEvent::SaveAs {
                                path: request.path,
                                force: request.force,
                            });
                        }
                    }
                }
            },
        );
    }

    /// The body of one tab's drain: runs once per wake that tab's core fires,
    /// registered through [`crate::tab_view::TabsHandle::connect_wake`].
    ///
    /// The order is fixed: liveness first, gating the tab's paint and the tree;
    /// the UI-focus sync; then [`AppShell::on_render_wake`], last and ungated. See
    /// notes/wolf-gnome-decisions.md#threading-and-the-wake for why each step sits
    /// where it does.
    pub fn on_tab_wake(&self, tab: &TabHandle) {
        // `count(wake) == wakes` by construction: this counter and the region below share one pass.
        frame_metrics::note_wake();
        {
            let _wake_guard = frame_metrics::scope(Bucket::Wake);
            let paint = self.on_liveness_wake(tab);
            if paint {
                {
                    let _guard = frame_metrics::scope(Bucket::WakeTabs);
                    tab.refresh(&self.inner.home);
                    self.inner
                        .tabs
                        .sync_terminal_focus(self.inner.window.is_active());
                }
                // The tree shows the selected tab's files; a background tab's wake has nothing for it.
                if tab.is_selected() {
                    let _guard = frame_metrics::scope(Bucket::WakeTree);
                    self.inner.tree.on_render_wake();
                }
            }
            self.inner.tabs.sync_ui_focus(self.inner.window.is_active());
            {
                let _guard = frame_metrics::scope(Bucket::WakeShell);
                self.on_render_wake(tab);
            }
        }
        // After the `wake` region closes, so the emit's cost is never measured. No timer: see
        // notes/wolf-gnome/frame-time-instrumentation.md
        frame_metrics::maybe_emit();
        // The end of a wake is when a pane binds and a grid is resized, so it is a readiness trigger.
        if self.inner.ready_armed {
            if let Some(coordinator) = coordinator::upgrade(&self.inner.coordinator) {
                coordinator.check_ready();
            }
        }
    }

    /// Reads `tab`'s captive nvim's [`Liveness`] ONCE per wake, applies the pure
    /// [`liveness_ui`] mapping, and reports whether the tab's pane view and the file
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
    /// The overlay is per window and shows the SELECTED tab, so it is applied only
    /// for that tab; the automatic restart is scheduled for any tab, so a
    /// background tab's crash restarts it without the user seeing an overlay.
    ///
    /// Returns `false` (do not paint) when the tab has no core at all.
    pub fn on_liveness_wake(&self, tab: &TabHandle) -> bool {
        let Some(liveness) = tab.liveness() else {
            return false;
        };
        let restart = tab.restart();

        let may_auto_restart = restart.attempts.get() < MAX_AUTO_RESTART_ATTEMPTS;
        let ui = liveness_ui(liveness, may_auto_restart);

        // A healthy core clears the budget, so a LATER, unrelated crash gets its
        // own automatic attempt rather than inheriting an exhausted counter.
        if liveness == Liveness::Alive {
            restart.attempts.set(0);
        }

        if tab.is_selected() {
            self.apply_overlay(ui.overlay);
        }

        if ui.auto_restart && !restart.restarting.get() && !restart.scheduled.get() {
            self.schedule_restart(tab);
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
    /// - hidden -> visible: close ALL THREE transient overlays. A crashed core must
    ///   not leave the picker's Open reachable underneath, nor a palette row whose
    ///   activation would send into a dead nvim (`New`, `Close`) or open another
    ///   overlay over the crash one (`Go to File…`). The buffer picker is the one
    ///   this cannot skip: a stall is discovered BY its candidate snapshot timing
    ///   out, so the transition to `Stalled` always finds it open with zero rows,
    ///   and an overlay left open keeps `window::wire_keys`' guard suppressing every
    ///   key to Neovim — including after a later restart.
    /// - visible -> hidden: if focus was inside the overlay (i.e. on the restart
    ///   button), park it back on the persistent cmdline strip. Hiding the focus
    ///   widget without a focus-return leaves the toplevel with no focus widget at
    ///   all, which kills window-level key dispatch outright.
    fn apply_overlay(&self, mode: OverlayMode) {
        // Sampled before `set_mode` hides the overlay: once a focus widget is unparented the root
        // reports no focus (see `pane_view::rebuild_pane_area`). Do not move this below `set_mode`.
        let had_focus = self.inner.crash_overlay.focus_is_inside();
        if !self.inner.crash_overlay.set_mode(mode) {
            return;
        }
        if mode == OverlayMode::Hidden {
            if had_focus {
                // The selected tab's strip: a background tab's is unmapped, and focusing an unmapped widget
                // leaves the toplevel with no focus widget.
                if let Some(tab) = self.inner.tabs.selected() {
                    tab.focus_area().grab_focus();
                }
            }
        } else {
            self.inner.picker.close();
            self.inner.palette.close();
            self.inner.buffers.close();
        }
    }

    /// Schedules ONE automatic restart attempt of `tab` [`RESTART_DEFER`] after the
    /// wake handler returns.
    ///
    /// **Deferred rather than called inline, deliberately.**
    /// [`WolfCore::restart_nvim`] is synchronous — a `runtime.block_on` over a
    /// kill, a spawn and twelve setup RPCs — so calling it from inside the wake
    /// handler would block the GTK main loop for the whole respawn with the crash
    /// overlay shown-but-never-presented. Deferring also guarantees the
    /// restart runs with no `RefCell` borrow live anywhere and lets
    /// [`AppShell::on_render_wake`]'s command drain have its turn first.
    fn schedule_restart(&self, tab: &TabHandle) {
        let restart = tab.restart();
        restart.scheduled.set(true);
        restart
            .attempts
            .set(restart.attempts.get().saturating_add(1));

        let weak = Rc::downgrade(&self.inner);
        let weak_tab = tab.downgrade();
        glib::timeout_add_local_once(RESTART_DEFER, move || {
            let (Some(shell), Some(tab)) = (upgrade(&weak), weak_tab.upgrade()) else {
                return;
            };
            tab.restart().scheduled.set(false);
            shell.restart_nvim(&tab);
        });
    }

    /// Restarts `tab`'s captive Neovim in place and restores its Tier-1 file set
    /// and its terminals, into that tab alone.
    ///
    /// The ONE path shared by the automatic (`Dead`) restart and the overlay
    /// button. Order matters: the open-file mirror is captured BEFORE the call
    /// (a respawn wipes it), the front-end re-entrancy flag is cleared BEFORE
    /// anything is acted on (so a re-entrant call from a widget callback cannot
    /// strand it set), and the banner is flashed on the SUCCESS path only, and only
    /// when the restarted tab is the one on screen.
    fn restart_nvim(&self, tab: &TabHandle) {
        let restart = tab.restart();
        if restart.restarting.get() {
            return;
        }
        restart.restarting.set(true);

        let captured = self.capture_open_files(tab);
        let drawer_open = tab.panes().drawer_is_open();
        let respawned = self.perform_restart(tab);
        let fresh = tab.liveness();

        // Cleared before acting on the outcome, so every exit path below — early
        // return included — leaves the guard down.
        restart.restarting.set(false);

        let Some(fresh) = fresh else {
            return;
        };
        match restart_outcome(respawned, fresh) {
            // No respawn (self-recovered, a concurrent restart held the CAS, or it failed): re-apply the
            // overlay and nothing else; acting on a `Refresh`'s `auto_restart` would loop forever.
            RestartOutcome::Refresh(ui) => {
                if tab.is_selected() {
                    self.apply_overlay(ui.overlay);
                }
            }
            RestartOutcome::Respawned => {
                restart.attempts.set(0);
                tab.panes().reset_after_restart();
                self.reopen_captured(tab, captured.as_ref());
                self.restore_terminals(tab, drawer_open);
                if tab.is_selected() {
                    self.apply_overlay(OverlayMode::Hidden);
                    self.inner.crash_overlay.flash_banner();
                }
                // Belt and braces, as `app.rs` does: a no-op if the fresh nvim has not reported `win_pos` yet.
                tab.refresh(&self.inner.home);
            }
        }
    }

    /// Captures `tab`'s Tier-1 open-file mirror BEFORE a restart wipes it. Both
    /// reads are RPC-free.
    ///
    /// **Never `WolfCore::current_tabpage()`**: that is a `block_on_rpc`, and
    /// against a dead nvim it would freeze the GTK main loop for the whole timeout
    /// budget on every crash. `Snapshot::current_tabpage` carries the same handle
    /// as a plain field, emitted once at setup by `runtime/nvim/init.lua`'s
    /// `wolf_setup_tabs`. A `current_tabpage` of `0` simply yields `None` — that
    /// is "nothing to reopen", not an error.
    fn capture_open_files(&self, tab: &TabHandle) -> Option<TabFiles> {
        let tabpage = {
            let core_ref = tab.core().borrow();
            let core = core_ref.as_ref()?;
            // Bound to a local so the `SnapshotReadGuard` is dropped at the end of
            // THIS block, before `open_files_for_tabpage` takes the same lock.
            let snapshot = core.read_snapshot();
            snapshot.current_tabpage
        };
        let core_ref = tab.core().borrow();
        core_ref.as_ref()?.open_files_for_tabpage(tabpage)
    }

    /// Hands `tab`, the one restarted, the steps that put its live terminal sessions
    /// back: every one but the drawer's as a split, then the drawer when it was open.
    /// RPC-free; the tab runs them on its first wake with a pane.
    /// See notes/crates/wolf-gnome/src/pane_view.rs.md#terminal-restore
    fn restore_terminals(&self, tab: &TabHandle, drawer_open: bool) {
        let (splits, live) = {
            let core_ref = tab.core().borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            let live = core.live_terminals();
            let drawer = core.drawer_terminal();
            let splits: Vec<TerminalId> = live
                .iter()
                .copied()
                .filter(|id| Some(*id) != drawer)
                .collect();
            (splits, live)
        };
        tab.panes()
            .request_terminal_restore(terminal_restore(&splits, drawer_open, &live));
    }

    /// Calls [`WolfCore::restart_nvim`] on `tab`'s core under a short borrow and
    /// reports whether an ACTUAL respawn happened.
    ///
    /// An `Err` is logged, never swallowed, and reported as "no respawn" — the
    /// core has already re-marked liveness `Dead`, so the caller's fresh read
    /// keeps the overlay up.
    fn perform_restart(&self, tab: &TabHandle) -> bool {
        let result = {
            let core_ref = tab.core().borrow();
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

    /// The selected tab's core, or an empty cell when no tab is selected, so a
    /// caller borrows it exactly as it would a core that has been taken out.
    fn selected_core(&self) -> CoreCell {
        self.inner.tabs.selected_core().unwrap_or_default()
    }

    /// The selected tab's core's [`Liveness`], or `None` when there is no core.
    fn read_liveness(&self) -> Option<Liveness> {
        self.inner.tabs.selected().and_then(|tab| tab.liveness())
    }

    /// Reopens a captured Tier-1 file set in `tab`'s own Neovim, in
    /// [`reopen_order`] — non-active files first, the ACTIVE one LAST so it lands
    /// focused.
    ///
    /// `InputEvent::Open` is the same event the picker and the file tree send, so
    /// the MRU is recorded identically: one path, no drift. Nothing in the loop can
    /// re-enter the `RefCell`, so one short borrow covers the whole reopen.
    fn reopen_captured(&self, tab: &TabHandle, captured: Option<&TabFiles>) {
        let Some(captured) = captured else {
            return;
        };
        let order = reopen_order(&captured.files, captured.active.as_deref());
        let core_ref = tab.core().borrow();
        let Some(core) = core_ref.as_ref() else {
            return;
        };
        for path in order {
            core.send_input(InputEvent::Open { path });
        }
    }

    /// Reacts to the end of `tab`'s wake: commits the window's state, and for the
    /// SELECTED tab settles the sidebar's root, refreshes the title, the bars and the
    /// navigation buttons, then drains and dispatches that tab's queued commands.
    ///
    /// Runs LAST on each wake (after the pane view and the tree), because a drained
    /// `close_window`, or a `close_tab` on the window's last tab, tears every core down.
    /// Unlike the paint, it runs in ALL THREE liveness states — see the module docs.
    pub fn on_render_wake(&self, tab: &TabHandle) {
        let selected = tab.is_selected();
        if selected {
            // One read of the tracked-cwd cell per wake, shared by re-root and title; `live_cwd` is a
            // `parking_lot` read, never an RPC.
            let live = {
                let core_ref = tab.core().borrow();
                core_ref.as_ref().and_then(WolfCore::live_cwd)
            };
            self.settle_reroot(live.as_deref());
            // After the settle, which takes a tab out of the empty state on a `:cd`.
            self.refresh_title(live.as_deref().filter(|_| !tab.is_empty()));
            self.refresh_titlebar();
            self.refresh_nav_buttons();
        }
        // The coordinator re-gathers this window and commits the whole tree: one window's wake must not
        // commit a tree that forgot the others.
        if let Some(coordinator) = coordinator::upgrade(&self.inner.coordinator) {
            let _guard = frame_metrics::scope(Bucket::WakeState);
            coordinator.commit(self.inner.window_id);
        }
        // LAST: a drained `close_window`, or `close_tab` on the last tab, tears every core down,
        // so the state commit above must already have happened.
        if selected {
            self.drain_and_dispatch_commands(tab);
        }
    }

    /// Sets the window title from `live` — the selected tab's Neovim's working directory,
    /// read once per wake by [`AppShell::on_render_wake`] — through the pure
    /// [`title::window_title`] seam. `live` is `None` for an empty tab, whose Neovim
    /// runs at the home directory without it being a folder, so its window is titled
    /// [`title::WINDOW_TITLE_FALLBACK`].
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

    /// Gives the header bar and the tab bar the background
    /// [`titlebar_background`] picks from the snapshot's `WolfTitlebar` and `Normal`.
    ///
    /// `Normal` is `resolve(0)`'s background, the one `PaneView::draw_pane` fills the grid
    /// from, so the bars match the grid. Read from the selected tab's core. The snapshot guard
    /// and the core borrow are dropped before the provider is touched. With no core the bars
    /// keep their last colour.
    fn refresh_titlebar(&self) {
        let wanted = {
            let cell = self.selected_core();
            let core_ref = cell.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            let snap = core.read_snapshot();
            titlebar_background(snap.theme.titlebar.bg, snap.highlights.resolve(0).bg)
        };
        self.inner.titlebar.apply(wanted);
    }

    /// Drains `tab`'s core's command queue and dispatches each command.
    ///
    /// The borrow is released in its own statement BEFORE anything is dispatched;
    /// see the module docs.
    fn drain_and_dispatch_commands(&self, tab: &TabHandle) {
        let commands: Vec<Command> = {
            let core_ref = tab.core().borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            core.drain_commands()
        };
        for command in commands {
            // Neovim notifies close_tab only once its own check passed, so the close asks no more.
            if command == Command::CloseTab {
                tab.confirm_close();
            }
            self.dispatch_command(command);
        }
    }

    /// Performs one palette-row activation: a registry command through the same
    /// [`UiAction`] funnel the drain uses, a recent folder through
    /// [`AppShell::open_folder`], or another window through the coordinator.
    ///
    /// **This is the one place a recent folder becomes a window action.** It opens
    /// the folder from THIS window — the window whose palette the row was picked in —
    /// through `open_folder`, so an Open Recent activation is indistinguishable from
    /// picking the same folder in the native Open Folder dialog.
    ///
    /// **And the one place a switch row becomes a raise.** It touches this window's
    /// project not at all; the coordinator presents the other window, best-effort
    /// (see [`crate::coordinator::Coordinator::raise`]).
    ///
    /// Exhaustive with no `_` arm, so a new [`PaletteAction`] variant is a compile
    /// error here. The `Unsupported` arm is unreachable while
    /// `palette::PaletteInner::activate_selection`'s own refusal stands, and is
    /// printed rather than dropped for the same reason [`AppShell::perform`]'s
    /// `Deferred | NoGnomeEquivalent` arm is.
    fn on_palette_activation(&self, action: PaletteAction) {
        match action {
            PaletteAction::Dispatch(command) => self.dispatch_command(command),
            PaletteAction::OpenRecentFolder(path) => self.open_folder(path),
            PaletteAction::SwitchWindow(index) => self.raise_window(index),
            PaletteAction::Unsupported(reason) => {
                eprintln!("wolf-gnome: palette activation not performed: {reason}");
            }
        }
    }

    /// Asks the coordinator to present another window.
    ///
    /// Nothing of this window changes, and nothing is done here beyond the
    /// forwarding: which window `index` names, whether it is still open, and what a
    /// Wayland compositor does with the request are all
    /// [`crate::coordinator::Coordinator::raise`]'s to answer.
    fn raise_window(&self, index: WindowId) {
        let Some(coordinator) = coordinator::upgrade(&self.inner.coordinator) else {
            eprintln!(
                "wolf-gnome: Switch to Window not performed: the coordinator is gone, so no \
                 window can be looked up to raise"
            );
            return;
        };
        coordinator.raise(index);
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
    /// **There are three transient overlays and at most one is open at a time; this
    /// is where that is arbitrated**: opening any one of them closes the other two
    /// first. Every `close()` is idempotent. This module holds all three handles, so
    /// the rule costs the three overlay modules no knowledge of each other — and it
    /// is what keeps `window::wire_keys`' open-overlay guard honest and makes "Escape
    /// closes the thing you can see" true.
    ///
    /// The sidebar's search panel is not an overlay and stays out of that rule.
    /// Showing it closes the three, because they would cover the editor it opens
    /// into, and nothing closes it.
    fn perform(&self, action: UiAction) {
        match action {
            UiAction::OpenFilePicker => {
                self.inner.palette.close();
                self.inner.buffers.close();
                self.inner.picker.open();
            }
            UiAction::OpenCommandPalette => {
                self.inner.picker.close();
                self.inner.buffers.close();
                self.inner.palette.open();
            }
            UiAction::OpenBufferPicker => {
                self.inner.picker.close();
                self.inner.palette.close();
                self.inner.buffers.open();
            }
            UiAction::ShowSearchPanel => self.show_search_panel(),
            UiAction::ShowSidebarProject => self.show_sidebar_project(),
            UiAction::HistoryBack => self.navigate(false),
            UiAction::HistoryForward => self.navigate(true),
            UiAction::ToggleSidebar => self.toggle_sidebar(),
            UiAction::CopySelection => self.copy_selection(),
            UiAction::PasteClipboard => self.paste_clipboard(),
            UiAction::ShowAbout => about::present(&self.inner.window),
            UiAction::OpenFileDialog => self.open_file_dialog(),
            UiAction::OpenFolderDialog => self.open_folder_dialog(),
            UiAction::SaveBuffer => self.save_buffer(),
            UiAction::SaveBufferAs => self.save_buffer_as(),
            UiAction::NewUntitledBuffer => self.send_input(InputEvent::NewBuffer),
            UiAction::NewTab => self.new_tab(),
            UiAction::NewWindow => self.new_window(),
            UiAction::ClosePane => self.send_input(InputEvent::ClosePane),
            UiAction::SplitHorizontal => self.send_input(InputEvent::SplitPane {
                orientation: SplitOrientation::Horizontal,
            }),
            UiAction::SplitVertical => self.send_input(InputEvent::SplitPane {
                orientation: SplitOrientation::Vertical,
            }),
            UiAction::ZoomPane => self.zoom_pane(),
            // Over the ordered input channel directly: `WolfCore::toggle_full_width` is a one-line
            // `send_input` onto it, and the neighbouring arms' shape wins.
            UiAction::ToggleFullWidth => self.send_input(InputEvent::ToggleFullWidth),
            UiAction::ToggleTerminal => self.toggle_terminal(),
            UiAction::CloseWindow => self.close_window(),
            UiAction::CloseTab => self.close_tab(),
            UiAction::MinimizeWindow => self.minimize_window(),
            UiAction::QuitApplication => self.quit_application(),
            UiAction::OpenPreview => self.open_preview(),
            UiAction::EditSource => self.edit_source(),
            UiAction::PreviewInOtherPane => self.preview_in_other_pane(),
            UiAction::EditInOtherPane => self.edit_in_other_pane(),
            UiAction::OpenInBrowser => self.preview_in_browser(),
            // Unreachable through an installed action (install_actions skips these)
            // and already reported by the drain, but never silent either way.
            UiAction::Deferred(reason) | UiAction::NoGnomeEquivalent(reason) => {
                eprintln!("wolf-gnome: ui action not performed: {reason}");
            }
        }
    }

    /// Shows the sidebar's search panel and focuses its field, or prints why not:
    /// [`search_panel::show_decision`] refuses with no project open.
    ///
    /// The sidebar is revealed through the PREFERENCE, as a toggle would, so a later
    /// Ctrl+Shift+B hides it again and the reveal is persisted. Nothing is run; the
    /// field keeps whatever query it holds.
    fn show_search_panel(&self) {
        if let ShowDecision::Refuse(reason) = search_panel::show_decision(!self.selected_is_empty())
        {
            eprintln!("wolf-gnome: project search not shown: {reason}");
            return;
        }
        self.inner.picker.close();
        self.inner.palette.close();
        self.inner.buffers.close();
        self.set_sidebar_preference(true);
        self.inner.sidebar.show_page(SidebarPage::Search);
        self.inner.search.focus_entry();
    }

    /// Shows the sidebar on its Files page.
    ///
    /// Focus inside the search panel is parked on the strip first, because hiding a
    /// focus widget leaves the toplevel with none.
    fn show_sidebar_project(&self) {
        if self.inner.search.owns_keyboard() {
            self.focus_selected_strip();
        }
        self.set_sidebar_preference(true);
        self.inner.sidebar.show_page(SidebarPage::Files);
    }

    /// Applies a click on the header bar's `clicked` toggle through
    /// [`sidebar_toggles::toggle_click`], over the sidebar's own shown flag and page, into the
    /// arms the chords take.
    fn on_sidebar_toggle_click(&self, clicked: SidebarPage) {
        match sidebar_toggles::toggle_click(
            self.inner.sidebar.is_shown(),
            self.inner.sidebar.page(),
            clicked,
        ) {
            ToggleClick::Hide => self.set_sidebar_preference(false),
            ToggleClick::Show(SidebarPage::Files) => self.show_sidebar_project(),
            ToggleClick::Show(SidebarPage::Search) => self.show_search_panel(),
        }
    }

    /// Closes the window through `close-request` so `window::wire_close` runs (picker,
    /// palette and buffer picker closed, the search panel's `rg` and the tree's
    /// traversal cancelled, then `WolfCore::shutdown`). That handler's unsaved check
    /// may keep the window open, showing the first tab with unsaved changes.
    ///
    /// NEVER `destroy()`: that skips `close-request` entirely and leaks the nvim
    /// child. The `adw::ApplicationWindow` is owned by the application, so the app
    /// exits once its last window closes.
    fn close_window(&self) {
        self.inner.window.close();
    }

    /// Closes the selected tab through [`TabsHandle::close`]; on the window's last tab,
    /// `tabs_ui::close_outcome` closes the window instead. A drained `close_tab`, which
    /// commands drain from the selected tab only, arrives confirmed and closes at once;
    /// a palette or menu Close Tab asks the tab's Neovim first.
    fn close_tab(&self) {
        if let Some(tab) = self.inner.tabs.selected() {
            self.inner.tabs.close(&tab);
        }
    }

    /// Minimizes the window, telling the coordinator first: Wayland never reports a
    /// minimize to the client, so Wolf's own is the only one the stacking order sees.
    fn minimize_window(&self) {
        if let Some(coordinator) = coordinator::upgrade(&self.inner.coordinator) {
            coordinator.on_window_minimized(self.inner.window_id);
        }
        self.inner.window.minimize();
    }

    /// Opens a new TAB in this window: the empty tab New Window opens, with a Neovim of
    /// its own (`specs/application-state.md#7-new-windows-and-tabs`). It is planned by
    /// the same [`state_seam::restore_plans`] call as
    /// [`crate::coordinator::Coordinator::open_new_window`], so the two cannot diverge.
    ///
    /// It takes one value from the selected tab: whether its sidebar shows now, through
    /// [`state_seam::sidebar_is_shown`] rather than the stored preference. A folder later
    /// opened into the new tab with no project state shows the sidebar by it, and it is
    /// captured here because the selected tab may close or change before then.
    ///
    /// No liveness gate and no blocking call: it starts a new process and asks the
    /// selected Neovim nothing, so a wedged or dead selected tab does not stop the user
    /// opening another.
    fn new_tab(&self) {
        let Some(selected) = self.inner.tabs.selected() else {
            return;
        };
        let shown = state_seam::sidebar_is_shown(selected.sidebar_shown(), selected.is_empty());
        let home = workspace::home_dir();
        let plans = state_seam::restore_plans(None, None, home.as_deref(), std::path::Path::is_dir);
        let Some(tab) = plans.first().and_then(|plan| plan.tabs.first()) else {
            eprintln!("wolf-gnome: restore_plans returned no tab for a new tab; none opened");
            return;
        };
        let parts = TabParts {
            new_tab_sidebar: Some(shown),
            ..window::tab_parts(tab)
        };
        // A refusal has already printed why; the window keeps its tabs as they were.
        let _ = self.inner.tabs.open_tab(parts);
    }

    /// Reacts to a tab switch. It first records the window's sidebar page and search
    /// fields into `previous`, the tab left, and captures its panes' cursors and scrolls
    /// (`specs/application-state.md` §4). It then closes all three transient overlays,
    /// applies the newly selected tab's empty state, sidebar preference and page,
    /// re-applies the crash overlay for it, moves the UI focus to its Neovim, and wakes
    /// it, so the title, the bars, the navigation buttons, the drawer divider, the
    /// sidebar's root and its queued commands follow on its drain. The sidebar's search
    /// panel is per window and keeps its query, toggles and rows.
    ///
    /// Wired through [`crate::tab_view::TabsHandle::connect_selected`], which fires
    /// only on a REAL selection change. Every `close()` is idempotent, and each
    /// returns focus through the resolved-at-close-time target — the newly selected
    /// tab's strip, never the one the overlay was opened over.
    ///
    /// The buffer picker is the one this cannot skip, for
    /// [`AppShell::apply_overlay`]'s recorded reason: an overlay left flagged open
    /// keeps `window::wire_keys`' guard suppressing every key to Neovim.
    fn on_tab_selected(&self, previous: Option<&TabHandle>) {
        if let Some(previous) = previous {
            previous.record_sidebar(self.inner.sidebar.page(), self.inner.search.fields());
            previous.capture_viewports();
        }
        self.inner.picker.close();
        self.inner.palette.close();
        self.inner.buffers.close();
        self.apply_selected_tab();
        let Some(selected) = self.inner.tabs.selected() else {
            return;
        };
        self.on_liveness_wake(&selected);
        self.inner.tabs.sync_ui_focus(self.inner.window.is_active());
        selected.wake();
    }

    /// Drops the sidebar sessions running on a closing tab's core, before that core
    /// is shut down. Wired through
    /// [`crate::tab_view::TabsHandle::connect_closing`].
    fn on_tab_closing(&self, tab: &TabHandle) {
        self.inner.tree.release_core(tab.core());
        self.inner.search.release_core(tab.core());
    }

    /// Opens a fresh, empty window through
    /// [`crate::coordinator::Coordinator::open_new_window`], which is the process's
    /// job rather than this window's: the new window gets its own tab and core, its own
    /// [`AppShell`] and its own subtree of the persisted tree.
    ///
    /// Nothing of THIS window is passed along. The new window takes the active
    /// window's normal size and nothing else of a window, not its working directory,
    /// and the coordinator is where that rule lives, so a reader wondering what the
    /// new window inherits finds one answer rather than two. Its tab is the one
    /// [`AppShell::new_tab`] opens, which also records the selected tab's sidebar.
    ///
    /// Reached from the drained `Command::NewWindow` (`:WolfNewWindow`) and from
    /// the palette's New Window row; the primary menu's item and the `app.`
    /// namespace reach the coordinator directly, without passing through here.
    /// Both routes end in the same call, so they cannot diverge.
    ///
    /// Refused with a printed reason when the window has no application (nothing to
    /// add a window to) or the coordinator is gone (nothing to own it), matching
    /// [`AppShell::quit_application`]'s shape — except that there is no useful
    /// fallback here: closing or opening nothing is the only honest answer.
    fn new_window(&self) {
        let Some(app) = self.inner.window.application() else {
            eprintln!(
                "wolf-gnome: New Window not performed: this window belongs to no application, so \
                 there is nothing to open a second window on"
            );
            return;
        };
        let Some(coordinator) = coordinator::upgrade(&self.inner.coordinator) else {
            eprintln!(
                "wolf-gnome: New Window not performed: the coordinator is gone, so nothing would \
                 own the new window's state"
            );
            return;
        };
        coordinator.open_new_window(&app);
    }

    /// Quits through [`crate::coordinator::Coordinator::quit`], which refuses while a tab
    /// holds unsaved changes, and otherwise commits the whole tree once while every core
    /// is still alive and only then closes every window — for the same clean-shutdown
    /// reason as [`AppShell::close_window`].
    ///
    /// Routing the drained `Command::QuitApplication` and the palette's Quit Wolf
    /// row through the coordinator is what makes them take the same funnel as the
    /// `app.quit` chord, so a quit cannot mean one thing when Neovim asks for it
    /// and another when the chord does.
    ///
    /// The fallback closes this window alone: without an application there are no
    /// other windows to close, and without a coordinator there is no tree to
    /// commit. Deliberately not `gio::Application::quit()`, which bypasses
    /// `close-request` and leaves the captive Neovim orphaned.
    fn quit_application(&self) {
        let Some(app) = self.inner.window.application() else {
            self.close_window();
            return;
        };
        let Some(coordinator) = coordinator::upgrade(&self.inner.coordinator) else {
            self.close_window();
            return;
        };
        coordinator.quit(&app);
    }

    /// Opens a native folder chooser; on success opens the chosen folder through
    /// [`AppShell::open_folder`].
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
                        shell.open_folder(path);
                    }
                }
            },
        );
    }

    /// Opens a native file chooser; on success hands the file to the tabs' drawer opener, or
    /// else asks the selected tab's core to `:e` it.
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
                        if !shell.inner.tabs.drawer_opener()(&path, None) {
                            shell.send_input(InputEvent::Open { path });
                        }
                    }
                }
            },
        );
    }

    /// Open Folder from this window: canonicalizes `path`, falling back to it as
    /// given, and hands it to [`crate::coordinator::Coordinator::open_folder`], which
    /// spans windows. Reached from the native Open Folder dialog, the empty state's
    /// button through it, and an Open Recent palette row.
    ///
    /// Canonicalized here, against the process cwd, so it must be absolute:
    /// [`crate::palette::open_recent_candidates`] refuses a relative path.
    fn open_folder(&self, path: PathBuf) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let Some(app) = self.inner.window.application() else {
            eprintln!("wolf-gnome: Open Folder not performed: the window has no application");
            return;
        };
        let Some(coordinator) = coordinator::upgrade(&self.inner.coordinator) else {
            eprintln!("wolf-gnome: Open Folder not performed: the coordinator is gone");
            return;
        };
        let current = self
            .inner
            .tabs
            .selected()
            .map(|tab| (self.inner.window_id, tab.id()));
        coordinator.open_folder(&app, current, path);
    }

    /// Shows the selected tab's recorded search fields in the window's panel, running
    /// the search when [`state_seam::restore_runs_search`] says so: a launch's selected
    /// tab and a tab Open Folder fills take their record as the panel's.
    pub fn restore_selected_search(&self) {
        let Some(tab) = self.inner.tabs.selected() else {
            return;
        };
        let fields = tab.search_fields();
        let run =
            state_seam::restore_runs_search(tab.sidebar_page(), tab.is_empty(), &fields.query);
        self.inner.search.restore(&fields, run);
    }

    /// Settles the sidebar's root against `live`, the selected tab's Neovim's working
    /// directory: clears that tab's empty state and re-roots when they have diverged.
    /// Run on every wake of the selected tab, BEFORE the command drain.
    ///
    /// An empty selected tab whose `live` is still its own launch cwd settles nothing:
    /// the sidebar may be rooted at another tab's folder, and selecting the empty tab
    /// must not take it out of the empty state. A typed `:cd` moves its `live`, and
    /// then it leaves the empty state and settles as any tab does, even when the
    /// sidebar is already rooted at its new `live`, as it is after New Tab from a
    /// folder tab and a `:cd` into that folder.
    ///
    /// **The ONE settle point** for every way the selected tab's directory changes —
    /// a typed `:cd`, a tab switch, a tab Open Folder fills — which is what stops
    /// them diverging in what the sidebar does afterwards. The whole decision is the
    /// pure [`workspace::reroot_decision`]; this function only performs it. The match
    /// is exhaustive with no catch-all arm, so a new [`workspace::RerootAction`]
    /// variant is a compile error here.
    fn settle_reroot(&self, live: Option<&std::path::Path>) {
        let selected = self.inner.tabs.selected();
        if let Some(tab) = selected.as_ref().filter(|tab| tab.is_empty()) {
            let launch = tab.launch_cwd();
            let launch = std::fs::canonicalize(&launch).unwrap_or(launch);
            if live == Some(launch.as_path()) {
                return;
            }
        }
        // Cloned out in its own statement: the arm below writes `rooted_cwd` back and calls
        // `apply_selected_tab`, which touches widgets.
        let rooted = self.inner.rooted_cwd.borrow().clone();
        match workspace::reroot_decision(live, rooted.as_deref()) {
            workspace::RerootAction::Stay => {
                // A `:cd` into the folder another tab rooted the sidebar at re-roots nothing.
                if let Some(tab) = selected
                    .as_ref()
                    .filter(|tab| live.is_some() && tab.is_empty())
                {
                    tab.set_empty(false);
                    tab.refresh_label(&self.inner.home);
                    self.apply_selected_tab();
                }
            }
            workspace::RerootAction::Reroot { target } => {
                // Updated in the SAME settle as the re-root, which is what makes the
                // next settle answer `Stay` and one directory change exactly one re-root.
                *self.inner.rooted_cwd.borrow_mut() = Some(target.to_path_buf());
                if let Some(tab) = selected.as_ref() {
                    tab.set_empty(false);
                    // This wake's `refresh` titled the tab while it was still empty.
                    tab.refresh_label(&self.inner.home);
                }
                self.apply_selected_tab();
                self.inner.tree.reroot(&workspace::root_label(target));
            }
        }
    }

    /// Sends one input event to the selected tab's core, holding the borrow only for
    /// the call.
    fn send_input(&self, event: InputEvent) {
        let cell = self.selected_core();
        let core_ref = cell.borrow();
        if let Some(core) = core_ref.as_ref() {
            core.send_input(event);
        }
    }

    /// Toggles the zoom of the selected tab's active pane ([`pane_view::PaneHandle::toggle_zoom`]).
    fn zoom_pane(&self) {
        if let Some(tab) = self.inner.tabs.selected() {
            tab.panes().toggle_zoom();
        }
    }

    /// Opens, focuses or hides the terminal drawer through [`WolfCore::toggle_terminal`], which
    /// starts its session (a pty and a fork, no RPC) before it queues the toggle. It unzooms
    /// the selected tab first unless the drawer is the zoomed pane
    /// ([`pane_view::PaneHandle::toggle_terminal`]).
    fn toggle_terminal(&self) {
        if let Some(tab) = self.inner.tabs.selected() {
            tab.panes().toggle_terminal();
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
