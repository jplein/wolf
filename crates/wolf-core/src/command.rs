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

/// Emits the [`Command`] enum unchanged and, for the tests, `EXPECTED_COMMAND_COUNT`, its
/// number of variants.
///
/// Stable Rust cannot count an enum's variants, so a table test over [`COMMAND_SPECS`]
/// alone would not see a variant with no row. Declaring the enum through this macro lets
/// the compiler count them; do not unwrap it into a plain `enum`.
macro_rules! declare_commands {
    (
        $(#[$enum_meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident,
            )+
        }
    ) => {
        $(#[$enum_meta])*
        $vis enum $name {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
        }

        /// The number of [`Command`] variants, counted by the compiler.
        #[cfg(test)]
        const EXPECTED_COMMAND_COUNT: usize = [$(stringify!($variant)),+].len();
    };
}

declare_commands! {
    /// A no-parameter, no-return user action.
    ///
    /// The variant ORDER is the discriminant contract shared with `wolf-ffi`'s
    /// `command_kind` module and the Swift `CommandKind` port, and [`COMMAND_SPECS`] is the
    /// registry that lists it. Adding a command is additive (append a variant + a
    /// [`COMMAND_SPECS`] row); reordering is a breaking change.
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
        /// Close the current split (the pane), not the buffer; on the tab's last pane, close
        /// the tab as `:q` does there.
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
        /// Copy the current selection to the system clipboard.
        Copy,
        /// Paste from the system clipboard into the active document.
        Paste,
        /// Open the buffer-picker overlay to fuzzy-switch among open buffers.
        BufferPicker,
        /// Open a new tab in the current window: the empty tab New Window opens, with a
        /// captive Neovim of its own (`specs/application-state.md#7-new-windows-and-tabs`).
        NewTab,
        /// Render the active Markdown buffer as HTML and show it in a preview pane
        /// (only available when the active buffer is a Markdown file).
        Preview,
        /// Navigate from a preview pane back to its source file (the inverse of
        /// `Preview`; only has an effect when the active pane is a preview pane).
        Edit,
        /// Toggle the terminal drawer (a bottom split showing the core's drawer session).
        Terminal,
        /// Render the active Markdown buffer and open it in the system default web
        /// browser, backed by a lazily-started loopback-only HTTP server owned by the
        /// core (only available when the active buffer is a Markdown file).
        PreviewInBrowser,
        /// Toggle the active buffer between the ~90-char default content width and full
        /// pane width (`b:wolf_full_width`).
        FullWidth,
        /// Toggle the sidebar (collapse/expand) — a native-only AppKit action.
        ToggleSidebar,
        /// Reveal the sidebar showing the file/project tree — a native-only AppKit
        /// action.
        SidebarShowProject,
        /// Reveal the sidebar showing the search UI (and focus the search field) — a
        /// native-only AppKit action.
        SearchInProject,
        /// Preview the active pane's Markdown file in ANOTHER pane — reuse an
        /// existing other pane or split one to the right (only available when the
        /// active buffer is a Markdown file).
        PreviewInOtherPane,
        /// Edit the active preview pane's source file in ANOTHER pane — reuse an
        /// existing other pane or split one to the left (only has an effect when the
        /// active pane is a preview pane).
        EditInOtherPane,
        /// Split the active pane into two stacked top and bottom, as Vim's `:split`
        /// (`<C-w>s`). The new pane is of the active pane's kind (an editor window on
        /// the same buffer, a new terminal session, or a preview of the same source);
        /// the terminal drawer refuses it.
        SplitHorizontal,
        /// Split the active pane into two side by side, as Vim's `:vsplit`
        /// (`<C-w>v`). The new pane is of the active pane's kind (an editor window on
        /// the same buffer, a new terminal session, or a preview of the same source);
        /// the terminal drawer refuses it.
        SplitVertical,
        /// Toggle the active pane, the terminal drawer included, between zoomed
        /// (filling the whole pane area, every other pane hidden) and not. A
        /// front-end action: nothing of it reaches Neovim.
        ZoomPane,
        /// Close Tab: close the tab, and its window when that is the window's only
        /// tab, once the tab's Neovim finds nothing unsaved. It is also what the `:q`
        /// and `:qa` families send.
        CloseTab,
    }
}

/// The menu group a command belongs to in a native menu bar.
///
/// Metadata for a front-end that chooses to render the registry (the macOS menu
/// hardcodes its own item list). Kept in the canonical registry so the grouping is
/// defined in one place.
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
        title: "Close Pane",
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
        title: "Edit",
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
    CommandSpec {
        command: Command::ToggleSidebar,
        id: "toggle_sidebar",
        ex_command: "WolfToggleSidebar",
        title: "Toggle sidebar",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::SidebarShowProject,
        id: "sidebar_show_project",
        ex_command: "WolfSidebarShowProject",
        title: "Sidebar: Show project",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::SearchInProject,
        id: "search_in_project",
        ex_command: "WolfSearchInProject",
        title: "Search in project",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::PreviewInOtherPane,
        id: "preview_in_other_pane",
        ex_command: "WolfPreviewInOtherPane",
        title: "Preview in Another Pane",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::EditInOtherPane,
        id: "edit_in_other_pane",
        ex_command: "WolfEditInOtherPane",
        title: "Edit in Another Pane",
        menu: MenuGroup::View,
    },
    CommandSpec {
        command: Command::SplitHorizontal,
        id: "split_horizontal",
        ex_command: "WolfSplitHorizontal",
        title: "Split Horizontal (stacked)",
        menu: MenuGroup::File,
    },
    CommandSpec {
        command: Command::SplitVertical,
        id: "split_vertical",
        ex_command: "WolfSplitVertical",
        title: "Split Vertical (side by side)",
        menu: MenuGroup::File,
    },
    CommandSpec {
        command: Command::ZoomPane,
        id: "zoom_pane",
        ex_command: "WolfZoomPane",
        title: "Zoom Pane",
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

    /// Every variant has exactly one [`COMMAND_SPECS`] row, at the index equal to its
    /// discriminant, and its id round-trips through [`Command::from_id`]. With the row count
    /// pinned to the compiler's variant count, a variant with no row, a reordered or
    /// duplicated row, and a shared id each fail here.
    #[test]
    fn every_variant_has_one_row_at_its_discriminant() {
        assert_eq!(
            COMMAND_SPECS.len(),
            EXPECTED_COMMAND_COUNT,
            "COMMAND_SPECS must hold one row per Command variant"
        );
        for (index, spec) in COMMAND_SPECS.iter().enumerate() {
            assert_eq!(
                spec.command as usize, index,
                "COMMAND_SPECS[{index}] is {:?}, whose discriminant is {}: the row is out of \
                 variant order",
                spec.command, spec.command as usize
            );
            assert_eq!(
                Command::from_id(spec.id),
                Some(spec.command),
                "COMMAND_SPECS[{index}]'s id {:?} must resolve to {:?}",
                spec.id,
                spec.command
            );
            assert_eq!(
                spec.command.spec(),
                spec,
                "Command::spec() for COMMAND_SPECS[{index}] must resolve back to that row"
            );
        }
    }
}
