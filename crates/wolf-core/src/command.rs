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
/// `BufferPicker`(16), `NewTab`(17), `Preview`(18), `Edit`(19), `Terminal`(20),
/// `PreviewInBrowser`(21), `FullWidth`(22).
/// Adding a command is additive
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
    /// Open a new tab in the current window (a new captive Neovim in the same
    /// directory).
    NewTab,
    /// Render the active Markdown buffer as HTML and show it in a preview pane
    /// (only available when the active buffer is a Markdown file).
    Preview,
    /// Navigate from a preview pane back to its source file (the inverse of
    /// `Preview`; only has an effect when the active pane is a preview pane).
    Edit,
    /// Toggle the terminal drawer (a bottom `:terminal` split, one per project).
    Terminal,
    /// Render the active Markdown buffer and open it in the system default web
    /// browser, backed by a lazily-started loopback-only HTTP server owned by the
    /// core (only available when the active buffer is a Markdown file).
    PreviewInBrowser,
    /// Toggle the active buffer between the ~90-char default content width and full
    /// pane width (`b:wolf_full_width`).
    FullWidth,
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
        command: Command::NewTab,
        id: "new_tab",
        ex_command: "WolfNewTab",
        title: "New Tab",
        menu: MenuGroup::File,
    },
    CommandSpec {
        command: Command::Preview,
        id: "preview",
        ex_command: "WolfPreview",
        title: "Preview",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::Edit,
        id: "edit",
        ex_command: "WolfEdit",
        title: "Edit Source",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::Terminal,
        id: "terminal",
        ex_command: "WolfTerminal",
        title: "Terminal",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::PreviewInBrowser,
        id: "preview_in_browser",
        ex_command: "WolfPreviewInBrowser",
        title: "Preview in Browser",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::FullWidth,
        id: "full_width",
        ex_command: "WolfFullWidth",
        title: "Use full width for content",
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
    /// `Command::Copy` (14), `Command::Paste` (15), `Command::BufferPicker`
    /// (16), `Command::NewTab` (17), `Command::Preview` (18), `Command::Edit`
    /// (19), `Command::Terminal` (20), `Command::PreviewInBrowser` (21), and
    /// `Command::FullWidth` (discriminant 22) past `Command::CommandPalette` (13).
    /// A named constant so the magic-number lint stays quiet and the count's
    /// meaning is explicit.
    const EXPECTED_COMMAND_COUNT: usize = 23;

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
    /// row after `NewTab` is appended at 17.
    const BUFFER_PICKER_INDEX: usize = 16;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::NewTab`: the appended discriminant 17. No longer the last row
    /// after `Preview` is appended at 18.
    const NEW_TAB_INDEX: usize = 17;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::Preview`: the appended discriminant 18. No longer the last row
    /// after `Edit` is appended at 19.
    const PREVIEW_INDEX: usize = 18;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::Edit`: the appended discriminant 19. No longer the last row
    /// after `Terminal` is appended at 20.
    const EDIT_INDEX: usize = 19;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::Terminal`: the appended discriminant 20. No longer the last row
    /// after `PreviewInBrowser` is appended at 21.
    const TERMINAL_INDEX: usize = 20;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::PreviewInBrowser`: the appended discriminant 21. No longer the
    /// last row after `FullWidth` is appended at 22.
    const PREVIEW_IN_BROWSER_INDEX: usize = 21;

    /// The append-only slice index (== FFI discriminant == `command_kind`) of
    /// `Command::FullWidth`: the appended discriminant 22 and LAST row of
    /// [`COMMAND_SPECS`].
    const FULL_WIDTH_INDEX: usize = 22;

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

    /// Appending Copy, Paste, BufferPicker, NewTab, Preview, Edit, Terminal,
    /// PreviewInBrowser, and FullWidth grows the registry to exactly 23 rows.
    #[test]
    fn command_specs_has_expected_len() {
        assert_eq!(
            COMMAND_SPECS.len(),
            EXPECTED_COMMAND_COUNT,
            "COMMAND_SPECS must hold exactly {EXPECTED_COMMAND_COUNT} rows \
             after appending Copy, Paste, BufferPicker, NewTab, Preview, Edit, \
             Terminal, PreviewInBrowser, and FullWidth"
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
    /// the last row after NewTab is appended at 17), so the slice index still
    /// equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn buffer_picker_is_at_index_16() {
        assert_eq!(
            COMMAND_SPECS[BUFFER_PICKER_INDEX].command,
            Command::BufferPicker,
            "the row at index {BUFFER_PICKER_INDEX} must be Command::BufferPicker"
        );
    }

    /// `from_id` resolves the canonical `new_tab` id to the new variant.
    #[test]
    fn new_tab_id_round_trips() {
        assert_eq!(
            Command::from_id("new_tab"),
            Some(Command::NewTab),
            "\"new_tab\" must resolve to Command::NewTab"
        );
    }

    /// The NewTab spec pins its id, ex-command, title, and File menu group (the
    /// canonical registry row for the new-tab command).
    #[test]
    fn new_tab_spec_fields() {
        let spec = Command::NewTab.spec();
        assert_eq!(spec.id, "new_tab", "id");
        assert_eq!(spec.ex_command, "WolfNewTab", "ex_command");
        assert_eq!(spec.title, "New Tab", "title");
        assert_eq!(spec.menu, MenuGroup::File, "menu group");
    }

    /// Append-discriminant guard: NewTab stays at fixed index 17 (no longer the
    /// last row after Preview is appended at 18), so the slice index still equals
    /// its FFI discriminant. Reordering breaks this.
    #[test]
    fn new_tab_is_at_index_17() {
        assert_eq!(
            COMMAND_SPECS[NEW_TAB_INDEX].command,
            Command::NewTab,
            "the row at index {NEW_TAB_INDEX} must be Command::NewTab"
        );
    }

    /// `from_id` resolves the canonical `preview` id to the new variant.
    #[test]
    fn preview_id_round_trips() {
        assert_eq!(
            Command::from_id("preview"),
            Some(Command::Preview),
            "\"preview\" must resolve to Command::Preview"
        );
    }

    /// The Preview spec pins its id, ex-command, title, and View menu group (the
    /// canonical registry row for the preview command).
    #[test]
    fn preview_spec_fields() {
        let spec = Command::Preview.spec();
        assert_eq!(spec.id, "preview", "id");
        assert_eq!(spec.ex_command, "WolfPreview", "ex_command");
        assert_eq!(spec.title, "Preview", "title");
        assert_eq!(spec.menu, MenuGroup::View, "menu group");
    }

    /// Append-discriminant guard: Preview stays at fixed index 18 (no longer the
    /// last row after Edit is appended at 19), so the slice index still equals its
    /// FFI discriminant. Reordering breaks this.
    #[test]
    fn preview_is_at_index_18() {
        assert_eq!(
            COMMAND_SPECS[PREVIEW_INDEX].command,
            Command::Preview,
            "the row at index {PREVIEW_INDEX} must be Command::Preview"
        );
    }

    /// `from_id` resolves the canonical `edit` id to the new variant.
    #[test]
    fn edit_id_round_trips() {
        assert_eq!(
            Command::from_id("edit"),
            Some(Command::Edit),
            "\"edit\" must resolve to Command::Edit"
        );
    }

    /// The Edit spec pins its id, ex-command, title, and View menu group (the
    /// canonical registry row for the edit command — the inverse of Preview).
    #[test]
    fn edit_spec_fields() {
        let spec = Command::Edit.spec();
        assert_eq!(spec.id, "edit", "id");
        assert_eq!(spec.ex_command, "WolfEdit", "ex_command");
        assert_eq!(spec.title, "Edit Source", "title");
        assert_eq!(spec.menu, MenuGroup::View, "menu group");
    }

    /// Append-discriminant guard: Edit stays at fixed index 19 (no longer the
    /// last row after Terminal is appended at 20), so the slice index still
    /// equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn edit_is_at_index_19() {
        assert_eq!(
            COMMAND_SPECS[EDIT_INDEX].command,
            Command::Edit,
            "the row at index {EDIT_INDEX} must be Command::Edit"
        );
    }

    /// `from_id` resolves the canonical `terminal` id to the new variant.
    #[test]
    fn terminal_id_round_trips() {
        assert_eq!(
            Command::from_id("terminal"),
            Some(Command::Terminal),
            "\"terminal\" must resolve to Command::Terminal"
        );
    }

    /// The Terminal spec pins its id, ex-command, title, and View menu group (the
    /// canonical registry row for the terminal-drawer command).
    #[test]
    fn terminal_spec_fields() {
        let spec = Command::Terminal.spec();
        assert_eq!(spec.id, "terminal", "id");
        assert_eq!(spec.ex_command, "WolfTerminal", "ex_command");
        assert_eq!(spec.title, "Terminal", "title");
        assert_eq!(spec.menu, MenuGroup::View, "menu group");
    }

    /// Append-discriminant guard: Terminal stays at fixed index 20 (no longer the
    /// last row after PreviewInBrowser is appended at 21), so the slice index still
    /// equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn terminal_is_at_index_20() {
        assert_eq!(
            COMMAND_SPECS[TERMINAL_INDEX].command,
            Command::Terminal,
            "the row at index {TERMINAL_INDEX} must be Command::Terminal"
        );
    }

    /// `from_id` resolves the canonical `preview_in_browser` id to the new variant.
    #[test]
    fn preview_in_browser_id_round_trips() {
        assert_eq!(
            Command::from_id("preview_in_browser"),
            Some(Command::PreviewInBrowser),
            "\"preview_in_browser\" must resolve to Command::PreviewInBrowser"
        );
    }

    /// The PreviewInBrowser spec pins its id, ex-command, title, and View menu
    /// group (the canonical registry row for the browser-preview command).
    #[test]
    fn preview_in_browser_spec_fields() {
        let spec = Command::PreviewInBrowser.spec();
        assert_eq!(spec.id, "preview_in_browser", "id");
        assert_eq!(spec.ex_command, "WolfPreviewInBrowser", "ex_command");
        assert_eq!(spec.title, "Preview in Browser", "title");
        assert_eq!(spec.menu, MenuGroup::View, "menu group");
    }

    /// Append-discriminant guard: PreviewInBrowser stays at fixed index 21 (no
    /// longer the last row after FullWidth is appended at 22), so the slice index
    /// still equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn preview_in_browser_is_at_index_21() {
        assert_eq!(
            COMMAND_SPECS[PREVIEW_IN_BROWSER_INDEX].command,
            Command::PreviewInBrowser,
            "the row at index {PREVIEW_IN_BROWSER_INDEX} must be Command::PreviewInBrowser"
        );
    }

    /// `from_id` resolves the canonical `full_width` id to the new variant.
    #[test]
    fn full_width_id_round_trips() {
        assert_eq!(
            Command::from_id("full_width"),
            Some(Command::FullWidth),
            "\"full_width\" must resolve to Command::FullWidth"
        );
    }

    /// The FullWidth spec pins its id, ex-command, title, and View menu group (the
    /// canonical registry row for the full-width toggle command).
    #[test]
    fn full_width_spec_fields() {
        let spec = Command::FullWidth.spec();
        assert_eq!(spec.id, "full_width", "id");
        assert_eq!(spec.ex_command, "WolfFullWidth", "ex_command");
        assert_eq!(spec.title, "Use full width for content", "title");
        assert_eq!(spec.menu, MenuGroup::View, "menu group");
    }

    /// Append-discriminant guard: FullWidth is the LAST row (index 22), so the
    /// slice index equals its FFI discriminant. Reordering breaks this.
    #[test]
    fn full_width_is_the_last_row() {
        assert_eq!(
            COMMAND_SPECS[FULL_WIDTH_INDEX].command,
            Command::FullWidth,
            "the row at index {FULL_WIDTH_INDEX} must be Command::FullWidth"
        );
        assert_eq!(
            FULL_WIDTH_INDEX,
            COMMAND_SPECS.len() - 1,
            "FullWidth must be the FINAL row of COMMAND_SPECS"
        );
    }

    /// Whole-table invariant: for EVERY row, `spec()`'s linear scan resolves back
    /// to that exact row, so the slice index (== FFI discriminant) and the variant
    /// order can never drift apart. Cheaper than one hand-written guard per
    /// command, and catches a duplicate `command` field that the per-command
    /// guards would miss.
    #[test]
    fn command_specs_index_matches_variant_order() {
        for (index, spec) in COMMAND_SPECS.iter().enumerate() {
            assert_eq!(
                spec.command.spec(),
                spec,
                "Command::spec() for COMMAND_SPECS[{index}] must resolve back to \
                 that row"
            );
            let found = COMMAND_SPECS
                .iter()
                .position(|row| row.command == spec.command)
                .expect("the row we are iterating must be findable");
            assert_eq!(
                found, index,
                "COMMAND_SPECS[{index}] ({:?}) must be the FIRST row for its \
                 variant (a duplicate would make Command::spec() ambiguous)",
                spec.command
            );
            let by_id = COMMAND_SPECS
                .iter()
                .position(|row| row.id == spec.id)
                .expect("the row we are iterating must be findable by id");
            assert_eq!(
                by_id, index,
                "COMMAND_SPECS[{index}] must own the unique id {:?}",
                spec.id
            );
        }
    }
}
