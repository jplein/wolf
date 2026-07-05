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
/// `MinimizeWindow`(4), `GoToFile`(5), `OpenFile`(6), `OpenFolder`(7),
/// `ClosePane`(8), `NewBuffer`(9), `ResetFontSize`(10), `IncreaseFontSize`(11),
/// `DecreaseFontSize`(12), `CommandPalette`(13). Adding a command is additive
/// (append a variant + a [`COMMAND_SPECS`] row); reordering is a breaking change.
/// `CommandPalette` is the palette's own invoking command and is EXCLUDED from the
/// palette's candidate list (a palette cannot dispatch itself).
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
    /// Open the file-picker overlay to jump to a file.
    GoToFile,
    /// Open a native dialog to open a file anywhere.
    OpenFile,
    /// Open a native dialog to change the working directory to a folder.
    OpenFolder,
    /// Close the current split (the pane), not the buffer.
    ClosePane,
    /// Open a new empty buffer in the current window.
    NewBuffer,
    /// Reset the font size to the configured default.
    ResetFontSize,
    /// Increase the font size by one point.
    IncreaseFontSize,
    /// Decrease the font size by one point.
    DecreaseFontSize,
    /// Open the command-palette overlay (its own invoking command; EXCLUDED from
    /// the palette's candidate list).
    CommandPalette,
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
    /// The View menu (font-size commands / …).
    View,
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
    CommandSpec {
        command: Command::GoToFile,
        id: "go_to_file",
        ex_command: "WolfGoToFile",
        title: "Go to File…",
        menu: MenuGroup::File,
    },
    CommandSpec {
        command: Command::OpenFile,
        id: "open_file",
        ex_command: "WolfOpenFile",
        title: "Open File…",
        menu: MenuGroup::File,
    },
    CommandSpec {
        command: Command::OpenFolder,
        id: "open_folder",
        ex_command: "WolfOpenFolder",
        title: "Open Folder…",
        menu: MenuGroup::File,
    },
    CommandSpec {
        command: Command::ClosePane,
        id: "close_pane",
        ex_command: "WolfClosePane",
        title: "Close",
        menu: MenuGroup::File,
    },
    CommandSpec {
        command: Command::NewBuffer,
        id: "new_buffer",
        ex_command: "WolfNewBuffer",
        title: "New",
        menu: MenuGroup::File,
    },
    CommandSpec {
        command: Command::ResetFontSize,
        id: "reset_font_size",
        ex_command: "WolfResetFontSize",
        title: "Reset Font Size",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::IncreaseFontSize,
        id: "increase_font_size",
        ex_command: "WolfIncreaseFontSize",
        title: "Increase Font Size",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::DecreaseFontSize,
        id: "decrease_font_size",
        ex_command: "WolfDecreaseFontSize",
        title: "Decrease Font Size",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::CommandPalette,
        id: "command_palette",
        ex_command: "WolfCommandPalette",
        title: "Command Palette…",
        menu: MenuGroup::View,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The total number of registered commands AFTER appending
    /// `Command::CommandPalette` (discriminant 13). A named constant so the
    /// magic-number lint stays quiet and the count's meaning is explicit.
    const EXPECTED_COMMAND_COUNT: usize = 14;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::CommandPalette`: the LAST row of [`COMMAND_SPECS`].
    const COMMAND_PALETTE_INDEX: usize = 13;

    /// `from_id` resolves the canonical `command_palette` id to the new variant.
    #[test]
    fn command_palette_id_round_trips() {
        assert_eq!(
            Command::from_id("command_palette"),
            Some(Command::CommandPalette),
            "\"command_palette\" must resolve to Command::CommandPalette"
        );
    }

    /// The CommandPalette spec pins its id, ex-command, title, and menu group
    /// (the canonical registry row for the palette command).
    #[test]
    fn command_palette_spec_fields() {
        let spec = Command::CommandPalette.spec();
        assert_eq!(spec.id, "command_palette", "id");
        assert_eq!(spec.ex_command, "WolfCommandPalette", "ex_command");
        assert_eq!(spec.title, "Command Palette…", "title");
        assert_eq!(spec.menu, MenuGroup::View, "menu group");
    }

    /// Appending CommandPalette grows the registry to exactly 14 rows.
    #[test]
    fn command_specs_has_expected_len() {
        assert_eq!(
            COMMAND_SPECS.len(),
            EXPECTED_COMMAND_COUNT,
            "COMMAND_SPECS must hold exactly {EXPECTED_COMMAND_COUNT} rows \
             after appending CommandPalette"
        );
    }

    /// Append-discriminant guard: CommandPalette is the LAST row (index 13), so
    /// the slice index equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn command_palette_is_the_last_row() {
        assert_eq!(
            COMMAND_SPECS[COMMAND_PALETTE_INDEX].command,
            Command::CommandPalette,
            "the row at index {COMMAND_PALETTE_INDEX} must be Command::CommandPalette"
        );
    }
}
