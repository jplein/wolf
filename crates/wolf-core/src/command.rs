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
/// `DecreaseFontSize`(12), `CommandPalette`(13), `Copy`(14), `Paste`(15),
/// `BufferPicker`(16), `CloseTab`(17). Adding a command is additive
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
    /// Copy the visual-mode selection to the system clipboard.
    Copy,
    /// Paste from the system clipboard into the active document.
    Paste,
    /// Open the buffer-picker overlay to fuzzy-switch among open buffers.
    BufferPicker,
    /// Close the active pane tab (the current buffer's tab in the active pane).
    CloseTab,
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
    /// The Edit menu (Copy / Paste / …).
    Edit,
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
    CommandSpec {
        command: Command::Copy,
        id: "copy",
        ex_command: "WolfCopy",
        title: "Copy",
        menu: MenuGroup::Edit,
    },
    CommandSpec {
        command: Command::Paste,
        id: "paste",
        ex_command: "WolfPaste",
        title: "Paste",
        menu: MenuGroup::Edit,
    },
    CommandSpec {
        command: Command::BufferPicker,
        id: "buffer_picker",
        ex_command: "WolfBufferPicker",
        title: "Buffer Picker…",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::CloseTab,
        id: "close_tab",
        ex_command: "WolfCloseTab",
        title: "Close Tab",
        menu: MenuGroup::File,
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
    /// `Command::Copy` (14), `Command::Paste` (15), `Command::BufferPicker` (16),
    /// and `Command::CloseTab` (discriminant 17) past `Command::CommandPalette`
    /// (13). A named constant so the magic-number lint stays quiet and the count's
    /// meaning is explicit.
    const EXPECTED_COMMAND_COUNT: usize = 18;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::CommandPalette`.
    const COMMAND_PALETTE_INDEX: usize = 13;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::Copy`: the appended discriminant 14.
    const COPY_INDEX: usize = 14;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::Paste`: the appended discriminant 15. No longer the last row
    /// after `BufferPicker` is appended at 16.
    const PASTE_INDEX: usize = 15;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::BufferPicker`: the appended discriminant 16. No longer the last
    /// row after `CloseTab` is appended at 17.
    const BUFFER_PICKER_INDEX: usize = 16;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::CloseTab`: the appended discriminant 17 and LAST row of
    /// [`COMMAND_SPECS`].
    const CLOSE_TAB_INDEX: usize = 17;

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

    /// Appending Copy, Paste, BufferPicker, and CloseTab grows the registry to
    /// exactly 18 rows.
    #[test]
    fn command_specs_has_expected_len() {
        assert_eq!(
            COMMAND_SPECS.len(),
            EXPECTED_COMMAND_COUNT,
            "COMMAND_SPECS must hold exactly {EXPECTED_COMMAND_COUNT} rows \
             after appending Copy, Paste, BufferPicker, and CloseTab"
        );
    }

    /// Append-discriminant guard: CommandPalette is at index 13, so the slice
    /// index equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn command_palette_is_at_index_13() {
        assert_eq!(
            COMMAND_SPECS[COMMAND_PALETTE_INDEX].command,
            Command::CommandPalette,
            "the row at index {COMMAND_PALETTE_INDEX} must be Command::CommandPalette"
        );
    }

    /// `from_id` resolves the canonical `copy` id to the new variant.
    #[test]
    fn copy_id_round_trips() {
        assert_eq!(
            Command::from_id("copy"),
            Some(Command::Copy),
            "\"copy\" must resolve to Command::Copy"
        );
    }

    /// `from_id` resolves the canonical `paste` id to the new variant.
    #[test]
    fn paste_id_round_trips() {
        assert_eq!(
            Command::from_id("paste"),
            Some(Command::Paste),
            "\"paste\" must resolve to Command::Paste"
        );
    }

    /// The Copy spec pins its id, ex-command, title, and (new) Edit menu group
    /// (the canonical registry row for the copy command).
    #[test]
    fn copy_spec_fields() {
        let spec = Command::Copy.spec();
        assert_eq!(spec.id, "copy", "id");
        assert_eq!(spec.ex_command, "WolfCopy", "ex_command");
        assert_eq!(spec.title, "Copy", "title");
        assert_eq!(spec.menu, MenuGroup::Edit, "menu group");
    }

    /// The Paste spec pins its id, ex-command, title, and (new) Edit menu group
    /// (the canonical registry row for the paste command).
    #[test]
    fn paste_spec_fields() {
        let spec = Command::Paste.spec();
        assert_eq!(spec.id, "paste", "id");
        assert_eq!(spec.ex_command, "WolfPaste", "ex_command");
        assert_eq!(spec.title, "Paste", "title");
        assert_eq!(spec.menu, MenuGroup::Edit, "menu group");
    }

    /// Append-discriminant guard: Copy is the row at index 14, so the slice
    /// index equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn copy_is_at_index_14() {
        assert_eq!(
            COMMAND_SPECS[COPY_INDEX].command,
            Command::Copy,
            "the row at index {COPY_INDEX} must be Command::Copy"
        );
    }

    /// Append-discriminant guard: Paste stays at fixed index 15 (no longer the
    /// last row after BufferPicker is appended at 16), so the slice index still
    /// equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn paste_is_at_index_15() {
        assert_eq!(
            COMMAND_SPECS[PASTE_INDEX].command,
            Command::Paste,
            "the row at index {PASTE_INDEX} must be Command::Paste"
        );
    }

    /// `from_id` resolves the canonical `buffer_picker` id to the new variant.
    #[test]
    fn buffer_picker_id_round_trips() {
        assert_eq!(
            Command::from_id("buffer_picker"),
            Some(Command::BufferPicker),
            "\"buffer_picker\" must resolve to Command::BufferPicker"
        );
    }

    /// The BufferPicker spec pins its id, ex-command, title, and View menu group
    /// (the canonical registry row for the buffer-picker command).
    #[test]
    fn buffer_picker_spec_fields() {
        let spec = Command::BufferPicker.spec();
        assert_eq!(spec.id, "buffer_picker", "id");
        assert_eq!(spec.ex_command, "WolfBufferPicker", "ex_command");
        assert_eq!(spec.title, "Buffer Picker…", "title");
        assert_eq!(spec.menu, MenuGroup::View, "menu group");
    }

    /// Append-discriminant guard: BufferPicker stays at fixed index 16 (no longer
    /// the last row after CloseTab is appended at 17), so the slice index still
    /// equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn buffer_picker_is_at_index_16() {
        assert_eq!(
            COMMAND_SPECS[BUFFER_PICKER_INDEX].command,
            Command::BufferPicker,
            "the row at index {BUFFER_PICKER_INDEX} must be Command::BufferPicker"
        );
    }

    /// `from_id` resolves the canonical `close_tab` id to the new variant.
    #[test]
    fn close_tab_id_round_trips() {
        assert_eq!(
            Command::from_id("close_tab"),
            Some(Command::CloseTab),
            "\"close_tab\" must resolve to Command::CloseTab"
        );
    }

    /// The CloseTab spec pins its id, ex-command, title, and File menu group (the
    /// canonical registry row for the close-tab command).
    #[test]
    fn close_tab_spec_fields() {
        let spec = Command::CloseTab.spec();
        assert_eq!(spec.id, "close_tab", "id");
        assert_eq!(spec.ex_command, "WolfCloseTab", "ex_command");
        assert_eq!(spec.title, "Close Tab", "title");
        assert_eq!(spec.menu, MenuGroup::File, "menu group");
    }

    /// Append-discriminant guard: CloseTab is the LAST row (index 17), so the
    /// slice index equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn close_tab_is_the_last_row() {
        assert_eq!(
            COMMAND_SPECS[CLOSE_TAB_INDEX].command,
            Command::CloseTab,
            "the row at index {CLOSE_TAB_INDEX} must be Command::CloseTab"
        );
        assert_eq!(
            CLOSE_TAB_INDEX,
            COMMAND_SPECS.len() - 1,
            "CloseTab must be the FINAL row of COMMAND_SPECS"
        );
    }
}
