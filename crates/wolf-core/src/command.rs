//! Commands: the canonical, platform-agnostic Wolf COMMAND REGISTRY.
//!
//! A Command is a no-parameter, no-return user action. Unlike a layout intent
//! (see [`crate::layout`]), a command is ONLY user-triggered — from inside the
//! captive Neovim (an Ex-command `:Wolf<Pascal>` or the Lua function
//! `require('wolf').<snake>()`), from the native front-end menu, and — in future
//! work — a command picker. There is no emit-on-attach, so a freshly attached
//! core has an EMPTY command queue.
//!
//! This module is PURE data + enums (no GUI toolkit, honoring the crate's one
//! hard rule). [`crate::redraw`] parses a `wolf_command` notification's id into a
//! [`Command`] via [`Command::from_id`] and pushes it onto a drainable queue; the
//! front-end consumes commands on wake via [`crate::api::WolfCore::drain_commands`]
//! and maps each to a native action.
//!
//! Naming rule (locked design): ONE canonical snake_case id (e.g. `close_window`)
//! derives both faces — a PascalCase Ex-command `:WolfCloseWindow` (Neovim forces
//! the uppercase start) and a snake_case Lua function `require('wolf').close_window()`.
//! The id → ex_command mapping is stored EXPLICITLY as data in [`COMMAND_SPECS`]
//! (no snake→Pascal transform code), so the registry is the single source of truth.

/// A no-parameter, no-return user action.
///
/// The variant ORDER is the discriminant contract shared with `wolf-ffi`'s
/// `command_kind` module and the Swift `CommandKind` port: `NewWindow`(0),
/// `CloseWindow`(1), `QuitApplication`(2), `HideApplication`(3),
/// `MinimizeWindow`(4). Adding a command is additive (append a variant + a
/// [`COMMAND_SPECS`] row); reordering is a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Open a new editor window.
    NewWindow,
    /// Close the focused window.
    CloseWindow,
    /// Quit the whole application.
    QuitApplication,
    /// Hide the application.
    HideApplication,
    /// Minimize the focused window.
    MinimizeWindow,
}

/// The menu group a command belongs to in a native menu bar.
///
/// Metadata for a front-end that chooses to render the registry (deferred for
/// this cut — the macOS menu hardcodes its own item list). Kept in the canonical
/// registry so the grouping is defined in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuGroup {
    /// The application menu (About / Hide / Quit).
    App,
    /// The File menu (New Window / …).
    File,
    /// The Window menu (Close Window / Minimize / …).
    Window,
}

/// The canonical metadata for one [`Command`]: its enum value, its canonical
/// snake_case `id`, its PascalCase `ex_command` name, a human `title`, and its
/// `menu` group.
///
/// Fields are PUBLIC so the registry ([`COMMAND_SPECS`]) can be inspected directly
/// (e.g. to build the Neovim registration list or a menu). The `id` → `ex_command`
/// mapping is stored explicitly (no transform code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    /// The command this row describes.
    pub command: Command,
    /// The canonical snake_case id (the `wolf_command` notification payload and
    /// the `require('wolf').<id>()` Lua function name).
    pub id: &'static str,
    /// The PascalCase Ex-command name (`:Wolf<Pascal>`; Neovim forces the
    /// uppercase-first user-command name).
    pub ex_command: &'static str,
    /// A human-readable title (a menu-item label).
    pub title: &'static str,
    /// The menu group this command belongs to.
    pub menu: MenuGroup,
}

/// The canonical command registry: one [`CommandSpec`] per [`Command`], in
/// variant order (so the slice index equals the FFI discriminant).
///
/// The single source of truth for every command's two faces (id / ex_command),
/// title, and menu group.
pub const COMMAND_SPECS: &[CommandSpec] = &[
    CommandSpec {
        command: Command::NewWindow,
        id: "new_window",
        ex_command: "WolfNewWindow",
        title: "New Window",
        menu: MenuGroup::File,
    },
    CommandSpec {
        command: Command::CloseWindow,
        id: "close_window",
        ex_command: "WolfCloseWindow",
        title: "Close Window",
        menu: MenuGroup::File,
    },
    CommandSpec {
        command: Command::QuitApplication,
        id: "quit_application",
        ex_command: "WolfQuitApplication",
        title: "Quit Wolf",
        menu: MenuGroup::App,
    },
    CommandSpec {
        command: Command::HideApplication,
        id: "hide_application",
        ex_command: "WolfHideApplication",
        title: "Hide Wolf",
        menu: MenuGroup::App,
    },
    CommandSpec {
        command: Command::MinimizeWindow,
        id: "minimize_window",
        ex_command: "WolfMinimizeWindow",
        title: "Minimize",
        menu: MenuGroup::Window,
    },
];

impl Command {
    /// Resolves a canonical snake_case `id` to its [`Command`], or `None` if no
    /// registered command matches (a linear scan of [`COMMAND_SPECS`]).
    #[must_use]
    pub fn from_id(id: &str) -> Option<Command> {
        COMMAND_SPECS
            .iter()
            .find(|spec| spec.id == id)
            .map(|spec| spec.command)
    }

    /// Returns the canonical [`CommandSpec`] for this command.
    #[must_use]
    pub fn spec(self) -> &'static CommandSpec {
        COMMAND_SPECS
            .iter()
            .find(|spec| spec.command == self)
            .expect("every Command variant has a COMMAND_SPECS row")
    }
}
