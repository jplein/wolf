//! `wolf-ffi` — the swift-bridge FFI surface over [`wolf_core`].
//!
//! This crate is the seam between the platform-agnostic Rust core and the macOS
//! (AppKit) front-end. It is built as a `staticlib` (the archive the `.app`
//! links, through the generated swift-bridge glue) and as an `rlib` (so the Rust
//! integration test in `tests/bridge_smoke.rs` can link and drive the same
//! surface headlessly, with no Swift present).
//!
//! ## Surface
//!
//! - [`WolfCoreHandle`] — an opaque, main-thread-only handle wrapping a
//!   `RefCell<Option<WolfCore>>`. [`WolfCore`](wolf_core::WolfCore) is not `Send`,
//!   so the handle and all its methods are used only on the AppKit main thread.
//! - [`FfiFrame`] — an opaque, per-frame snapshot of the active grid, with the
//!   highlight table already resolved in Rust (never re-resolved in Swift). Scalar
//!   accessors plus one flat [`FfiFrame::cells`] handoff (row-major).
//! - [`FfiTerminalFrame`] — an opaque handle on one terminal session's published
//!   frame, read row by row through [`FfiTerminalFrame::row_cells`]; the session calls
//!   are `WolfCoreHandle` methods named `terminal_*`, plus `open_terminal`,
//!   `bind_terminal`, `queue_bind_terminal`, `resize_terminal`, `close_terminal` and
//!   `live_terminals`.
//! - [`FfiKeyEvent`] — a transparent struct Swift fills from an `NSEvent`; mapped
//!   here into a [`wolf_core::KeyEvent`] so nvim notation stays the single source
//!   of truth in core.
//! - [`wolf_ffi_start`] / [`WolfCoreHandle::read_frame`] /
//!   [`WolfCoreHandle::send_key`] / [`WolfCoreHandle::resize`] /
//!   [`WolfCoreHandle::shutdown`].
//!
//! ## Wake
//!
//! Core fires a [`WakeFn`](wolf_core::WakeFn) on each `flush`; the front-end must
//! marshal it onto the AppKit main thread (`DispatchQueue.main.async`) and trigger
//! a repaint. Rather than an `extern "Swift"` symbol — which a Rust-only
//! integration test could never resolve at link time — the wake is a globally
//! **registered** `extern "C"` function pointer ([`wolf_ffi_set_wake`]). Swift's
//! `WakeBridge` registers its dispatch-to-main thunk once at startup; the Rust
//! test never registers, so the wake is a harmless no-op there and the staticlib
//! and the rlib both link with no Swift runtime present.

// swift-bridge's expansion emits same-type raw-pointer casts into sibling items that a
// module-level allow cannot reach, so this one lint is allowed crate-wide.
#![allow(clippy::unnecessary_cast)]

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use wolf_core::app_state as app;
use wolf_core::{
    choose_pane, project_directory, rank_buffers, rank_matches_top_n, rank_palette, AppStateStore,
    Appearance, BufInfo, BufferRow, Cmdline, Command, CommandMru, CoreConfig, CursorShape,
    FileSearch, FocusDir, Grid, HighlightTable, InputEvent, Key, KeyEvent, LayoutIntent, Liveness,
    MarkdownCursor, MouseAction, MouseButton, MouseEvent, OpenListener, OpenRequest, OpenTarget,
    OpenWait, PaletteRow, PaletteRowKind, PaletteWindow, PaneChoice, PanePlacement, PaneStatus,
    PreviewInfo, Pum, ResolvedStyle, Rgb, ScrollDirection, ScrollEvent, SearchMatch, SearchOptions,
    SearchSession, SplitOrientation, TabFiles, TabId, TerminalCell, TerminalCellWidth,
    TerminalCommand, TerminalCursor, TerminalCursorStyle, TerminalFrame, TerminalId,
    TerminalMouseEvent, TerminalMouseKind, TerminalScrollEvent, TerminalTarget, TerminalUnderline,
    Toast, WakeFn, WinInfo, WolfCore,
};

// Re-export the bridge's transparent structs so rlib consumers (the Rust tests) can name them;
// the opaque types and `wolf_ffi_start` are already crate-root `pub` items.
pub use ffi::{FfiColor, FfiKeyEvent, FfiMouseEvent, FfiScrollEvent};

/// `FfiKeyEvent.kind` discriminants. These mirror the variant order of
/// [`wolf_core::Key`] (Char, Enter, Esc, Tab, Backspace, Delete, Left, Right, Up,
/// Down, Home, End, PageUp, PageDown, F) and MUST match the Swift `Kind` enum in
/// `WolfKeyMap` and `KeyMapTests`. They are the single source of truth consumed by
/// [`key_event_from_ffi`].
mod kind {
    /// A printable character (the glyph rides in `FfiKeyEvent::ch`).
    pub const CHAR: u8 = 0;
    /// The Enter / Return key.
    pub const ENTER: u8 = 1;
    /// The Escape key.
    pub const ESC: u8 = 2;
    /// The Tab key.
    pub const TAB: u8 = 3;
    /// The Backspace key.
    pub const BACKSPACE: u8 = 4;
    /// The forward-delete key.
    pub const DELETE: u8 = 5;
    /// The Left arrow.
    pub const LEFT: u8 = 6;
    /// The Right arrow.
    pub const RIGHT: u8 = 7;
    /// The Up arrow.
    pub const UP: u8 = 8;
    /// The Down arrow.
    pub const DOWN: u8 = 9;
    /// The Home key.
    pub const HOME: u8 = 10;
    /// The End key.
    pub const END: u8 = 11;
    /// The Page Up key.
    pub const PAGE_UP: u8 = 12;
    /// The Page Down key.
    pub const PAGE_DOWN: u8 = 13;
    /// A function key (number in `FfiKeyEvent::fnum`).
    pub const FUNCTION: u8 = 14;
}

/// Cell style flag bits packed into [`FfiCell::flags`].
mod flag {
    /// Bold text.
    pub const BOLD: u8 = 1 << 0;
    /// Italic text.
    pub const ITALIC: u8 = 1 << 1;
    /// Underlined text.
    pub const UNDERLINE: u8 = 1 << 2;
    /// Curly underline.
    pub const UNDERCURL: u8 = 1 << 3;
    /// Strikethrough text.
    pub const STRIKETHROUGH: u8 = 1 << 4;
}

/// The cursor-shape discriminants packed into [`FfiFrame`]'s `cursor_shape`,
/// mirroring [`wolf_core::CursorShape`].
mod cursor_shape {
    /// A full-cell block cursor.
    pub const BLOCK: u8 = 0;
    /// A horizontal bar cursor.
    pub const HORIZONTAL: u8 = 1;
    /// A vertical bar cursor.
    pub const VERTICAL: u8 = 2;
}

/// The appearance-mode discriminants packed into [`FfiConfig`]'s `appearance`,
/// mirroring [`wolf_core::Appearance`].
///
/// The single source of truth shared with the Swift `AppearanceMode` port (a
/// `UInt8` raw-value enum in `WolfKeyMap`) and the `AppearanceSeamTests`
/// discriminant contract.
mod appearance {
    /// Follow the macOS system appearance (`auto`).
    pub const AUTO: u8 = 0;
    /// Force dark mode (`dark`).
    pub const DARK: u8 = 1;
    /// Force light mode (`light`).
    pub const LIGHT: u8 = 2;
}

/// The layout-intent kind discriminants packed into [`FfiLayoutIntent::kind`].
///
/// The single source of truth shared with the Swift `PaneTree` port and the
/// `WolfKeyMapTests` / `tests/panes.rs` discriminant contract.
mod intent_kind {
    /// A window SPLIT was performed by nvim (carries orientation + from + win).
    pub const SPLIT: u8 = 0;
    /// A PURE focus-direction move request (carries dir + from).
    pub const FOCUS_DIR: u8 = 1;
    /// The terminal DRAWER was opened by nvim (carries just win — the new drawer
    /// window). Append-only/last: never reorder (contract with the Swift port).
    pub const DRAWER_OPEN: u8 = 2;
}

/// The command-kind discriminants packed into [`FfiCommand::kind`].
///
/// The Rust side's record of each [`wolf_core::Command`] discriminant, append-only: a
/// value never moves once shipped. `tests/commands.rs` requires each
/// [`wolf_core::COMMAND_SPECS`] row to drain as its index, and the Swift `CommandKind`
/// port is checked against these values over the bridge.
mod command_kind {
    /// Open a new editor window.
    pub const NEW_WINDOW: u8 = 0;
    /// Close the focused window.
    pub const CLOSE_WINDOW: u8 = 1;
    /// Quit the whole application.
    pub const QUIT_APPLICATION: u8 = 2;
    /// Hide the application.
    pub const HIDE_APPLICATION: u8 = 3;
    /// Minimize the focused window.
    pub const MINIMIZE_WINDOW: u8 = 4;
    /// Open the file-picker overlay to jump to a file.
    pub const GO_TO_FILE: u8 = 5;
    /// Open a native dialog to open a file anywhere.
    pub const OPEN_FILE: u8 = 6;
    /// Open a native dialog to change the working directory to a folder.
    pub const OPEN_FOLDER: u8 = 7;
    /// Close the current split (the pane), not the buffer.
    pub const CLOSE_PANE: u8 = 8;
    /// Open a new empty buffer in the current window.
    pub const NEW_BUFFER: u8 = 9;
    /// Reset the font size to the configured default.
    pub const RESET_FONT_SIZE: u8 = 10;
    /// Increase the font size by one point.
    pub const INCREASE_FONT_SIZE: u8 = 11;
    /// Decrease the font size by one point.
    pub const DECREASE_FONT_SIZE: u8 = 12;
    /// Open the command-palette overlay (the palette's own invoking command).
    pub const COMMAND_PALETTE: u8 = 13;
    /// Copy the visual-mode selection to the system clipboard.
    pub const COPY: u8 = 14;
    /// Paste from the system clipboard into the active document.
    pub const PASTE: u8 = 15;
    /// Open the buffer-picker overlay to fuzzy-switch among open buffers.
    pub const BUFFER_PICKER: u8 = 16;
    /// Open a new tab in the current window.
    pub const NEW_TAB: u8 = 17;
    /// Render the active Markdown buffer as HTML in a preview pane.
    pub const PREVIEW: u8 = 18;
    /// Navigate from a preview pane back to its source file (inverse of preview).
    pub const EDIT: u8 = 19;
    /// Toggle the terminal drawer (a bottom split on the core's drawer session).
    pub const TERMINAL: u8 = 20;
    /// Render the active Markdown buffer and open it in the system web browser.
    pub const PREVIEW_IN_BROWSER: u8 = 21;
    /// Toggle the active buffer between the ~90-char default content width and full
    /// pane width (`b:wolf_full_width`).
    pub const FULL_WIDTH: u8 = 22;
    /// Toggle the sidebar (collapse/expand) — a native-only AppKit action.
    pub const TOGGLE_SIDEBAR: u8 = 23;
    /// Reveal the sidebar showing the file/project tree — a native-only AppKit
    /// action.
    pub const SIDEBAR_SHOW_PROJECT: u8 = 24;
    /// Reveal the sidebar showing the search UI (and focus the search field) — a
    /// native-only AppKit action.
    pub const SEARCH_IN_PROJECT: u8 = 25;
    /// Preview the active pane's Markdown file in another pane (reuse or split
    /// right).
    pub const PREVIEW_IN_OTHER_PANE: u8 = 26;
    /// Edit the active preview pane's source file in another pane (reuse or
    /// split, source lands left).
    pub const EDIT_IN_OTHER_PANE: u8 = 27;
    /// Split the active pane into two stacked panes, as Vim's `:split`.
    pub const SPLIT_HORIZONTAL: u8 = 28;
    /// Split the active pane into two side-by-side panes, as Vim's `:vsplit`.
    pub const SPLIT_VERTICAL: u8 = 29;
    /// Toggle the active pane between zoomed, over the whole pane area, and not.
    pub const ZOOM_PANE: u8 = 30;
    /// Close the tab that queued it, and its window on its last tab.
    pub const CLOSE_TAB: u8 = 31;
}

/// The palette-row-kind discriminants packed into [`FfiCommandRow::row_kind`].
///
/// The single source of truth shared with the Swift `PaletteSeam.PaletteRowKind`
/// port, mirroring the variant order of [`wolf_core::PaletteRowKind`].
mod palette_row_kind {
    /// A static registry command row (dispatched via the command channel).
    pub const STATIC: u8 = 0;
    /// A "Switch to Window" row (raises the window at `window_index`).
    pub const SWITCH_WINDOW: u8 = 1;
    /// An "Open Recent Folder" row (runs Open Folder on the recent dir at `path`).
    pub const OPEN_RECENT: u8 = 2;
}

/// The split-orientation discriminants packed into [`FfiLayoutIntent::orientation`],
/// mirroring the variant order of [`wolf_core::SplitOrientation`].
mod orientation {
    /// Panes stacked top/bottom (`:split` / `<C-w>s`).
    pub const HORIZONTAL: u8 = 0;
    /// Panes side by side (`:vsplit` / `<C-w>v`).
    pub const VERTICAL: u8 = 1;
}

/// The focus-direction discriminants packed into [`FfiLayoutIntent::dir`],
/// mirroring the variant order of [`wolf_core::FocusDir`].
mod focus_dir {
    /// Move focus to the pane on the left.
    pub const LEFT: u8 = 0;
    /// Move focus to the pane on the right.
    pub const RIGHT: u8 = 1;
    /// Move focus to the pane above.
    pub const UP: u8 = 2;
    /// Move focus to the pane below.
    pub const DOWN: u8 = 3;
}

/// The mouse-button discriminants packed into [`FfiMouseEvent::button`], mirroring
/// the variant order of [`wolf_core::MouseButton`].
///
/// The single source of truth shared with the Swift `PaneContainerView` ports and
/// the `tests/mouse_scroll.rs` discriminant contract, consumed by
/// [`mouse_event_from_ffi`].
mod mouse_button {
    /// The left mouse button.
    pub const LEFT: u8 = 0;
    /// The right mouse button.
    pub const RIGHT: u8 = 1;
    /// The middle mouse button.
    pub const MIDDLE: u8 = 2;
    /// No button: motion with nothing held, for [`WolfCoreHandle::terminal_mouse`] only.
    pub const NONE: u8 = 3;
}

/// The mouse-action discriminants packed into [`FfiMouseEvent::action`], mirroring
/// the variant order of [`wolf_core::MouseAction`].
///
/// The single source of truth shared with the Swift ports and
/// `tests/mouse_scroll.rs`, consumed by [`mouse_event_from_ffi`].
mod mouse_action {
    /// Button pressed.
    pub const PRESS: u8 = 0;
    /// Button released.
    pub const RELEASE: u8 = 1;
    /// Pointer dragged with the button held.
    pub const DRAG: u8 = 2;
}

/// The pane-kind discriminants of the application-state builder and readers,
/// mirroring the variant order of [`wolf_core::app_state::Pane`].
mod pane_kind {
    /// An editor pane.
    pub const EDIT: u8 = 0;
    /// A Markdown preview pane.
    pub const PREVIEW: u8 = 1;
    /// A terminal pane.
    pub const TERMINAL: u8 = 2;
}

/// The pane-reference discriminants of a tab's `focused` and `zoomed`, carried
/// beside an index that only [`pane_ref::PANE`] reads.
mod pane_ref {
    /// No reference: nothing focused or zoomed to restore.
    pub const NONE: u8 = 0;
    /// The pane at the index, in depth-first order.
    pub const PANE: u8 = 1;
    /// The terminal drawer.
    pub const DRAWER: u8 = 2;
}

/// The sidebar-mode discriminants, mirroring [`wolf_core::app_state::SidebarMode`]
/// after an "unset" of their own.
mod sidebar_mode {
    /// No mode to restore.
    pub const UNSET: u8 = 0;
    /// The file tree.
    pub const TREE: u8 = 1;
    /// The content-search panel.
    pub const SEARCH: u8 = 2;
}

/// The discriminants of an optional boolean read back across the bridge.
mod tristate {
    /// Nothing to restore.
    pub const UNSET: u8 = 0;
    /// `false`.
    pub const OFF: u8 = 1;
    /// `true`.
    pub const ON: u8 = 2;
}

/// How long [`wolf_ffi_app_state_flush`] blocks waiting for the state writer
/// thread to drain everything pending.
const STATE_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// The scroll-direction discriminants packed into [`FfiScrollEvent::direction`],
/// mirroring the variant order of [`wolf_core::ScrollDirection`].
///
/// The single source of truth shared with the Swift ports and
/// `tests/mouse_scroll.rs`, consumed by [`scroll_event_from_ffi`].
mod scroll_dir {
    /// Scroll up (decreases the viewport `topline`).
    pub const UP: u8 = 0;
    /// Scroll down (increases the viewport `topline`).
    pub const DOWN: u8 = 1;
    /// Scroll left.
    pub const LEFT: u8 = 2;
    /// Scroll right.
    pub const RIGHT: u8 = 3;
}

/// The captive-nvim liveness discriminants returned by
/// [`WolfCoreHandle::liveness`], mirroring [`wolf_core::Liveness::to_u8`]
/// (`Alive` / `NotResponding` / `Dead` = `0` / `1` / `2`).
///
/// The single source of truth shared with the Swift front-end's
/// `NvimStatusOverlayView` (which drives the "crashed" / "not responding" overlay
/// off this value). A shut-down core reads as [`DEAD`].
mod liveness {
    /// The captive nvim is responding normally.
    pub const ALIVE: u8 = 0;
    /// The captive nvim has stalled (a synchronous RPC or an input batch outlasted
    /// the timeout).
    pub const NOT_RESPONDING: u8 = 1;
    /// The captive nvim process has died (or the core is gone).
    pub const DEAD: u8 = 2;
}

/// The `target` discriminants of [`WolfCoreHandle::open_terminal`] and
/// [`WolfCoreHandle::bind_terminal`], mapped to [`wolf_core::TerminalTarget`].
mod terminal_target {
    /// Show the session in the window, replacing its buffer.
    pub const WINDOW: u8 = 0;
    /// Split the window, stacked, and show the session in the new window.
    pub const SPLIT_HORIZONTAL: u8 = 1;
    /// Split the window side by side and show the session in the new window.
    pub const SPLIT_VERTICAL: u8 = 2;
}

/// The `kind` discriminants of [`WolfCoreHandle::terminal_mouse`], mirroring
/// [`wolf_core::TerminalMouseKind`].
mod terminal_mouse_kind {
    /// A button pressed.
    pub const PRESS: u8 = 0;
    /// A button released.
    pub const RELEASE: u8 = 1;
    /// The pointer moved, with or without a button held.
    pub const MOTION: u8 = 2;
}

/// The modifier bits of [`WolfCoreHandle::terminal_mouse`] and
/// [`WolfCoreHandle::terminal_scroll`]'s `mods`.
mod terminal_mods {
    /// Shift, which bypasses the program's mouse tracking.
    pub const SHIFT: u8 = 1 << 0;
    /// Control.
    pub const CTRL: u8 = 1 << 1;
    /// Alt (Option).
    pub const ALT: u8 = 1 << 2;
}

/// The cursor-style discriminants of [`FfiTerminalFrame::cursor_style`], mirroring
/// [`wolf_core::TerminalCursorStyle`].
mod terminal_cursor_style {
    /// A filled block.
    pub const BLOCK: u8 = 0;
    /// A vertical bar.
    pub const BAR: u8 = 1;
    /// An underline.
    pub const UNDERLINE: u8 = 2;
    /// An outlined block.
    pub const HOLLOW: u8 = 3;
}

/// The underline discriminants of [`FfiTerminalCell::underline`], mirroring
/// [`wolf_core::TerminalUnderline`].
mod terminal_underline {
    /// No underline.
    pub const NONE: u8 = 0;
    /// A single line.
    pub const SINGLE: u8 = 1;
    /// A double line.
    pub const DOUBLE: u8 = 2;
    /// A curly line.
    pub const CURLY: u8 = 3;
    /// A dotted line.
    pub const DOTTED: u8 = 4;
    /// A dashed line.
    pub const DASHED: u8 = 5;
}

/// The cell-width discriminants of [`FfiTerminalCell::width`], mirroring
/// [`wolf_core::TerminalCellWidth`].
mod terminal_cell_width {
    /// An ordinary one-column cell.
    pub const NARROW: u8 = 0;
    /// The first column of a wide character, holding its grapheme.
    pub const WIDE: u8 = 1;
    /// The second column of a wide character, painted by nothing.
    pub const SPACER_TAIL: u8 = 2;
    /// The last column of a row whose wide character wrapped to the next row.
    pub const SPACER_HEAD: u8 = 3;
}

/// The style bits of [`FfiTerminalCell::flags`].
mod terminal_flag {
    /// Bold text.
    pub const BOLD: u8 = 1 << 0;
    /// Faint (dim) text.
    pub const FAINT: u8 = 1 << 1;
    /// Italic text.
    pub const ITALIC: u8 = 1 << 2;
    /// Strikethrough text.
    pub const STRIKETHROUGH: u8 = 1 << 3;
    /// The painter swaps the foreground and background.
    pub const INVERSE: u8 = 1 << 4;
    /// The painter draws no glyph.
    pub const INVISIBLE: u8 = 1 << 5;
    /// Blinking text.
    pub const BLINK: u8 = 1 << 6;
}

/// The [`FfiOpenItem::kind`] discriminants, mirroring the variant order of
/// [`wolf_core::OpenTarget`].
mod open_kind {
    /// No path: launch or activate Wolf.
    pub const ACTIVATE: u8 = 0;
    /// An existing directory, for Open Folder.
    pub const DIRECTORY: u8 = 1;
    /// A file, existing or not.
    pub const FILE: u8 = 2;
}

/// The [`FfiPaneChoice::placement`] discriminants, mirroring the variant order of
/// [`wolf_core::PanePlacement`].
mod pane_placement {
    /// Focus the editor pane already showing the file.
    pub const FOCUS: u8 = 0;
    /// Open the file in the editor pane.
    pub const OPEN: u8 = 1;
    /// Make the preview pane an editor pane showing the file.
    pub const CONVERT: u8 = 2;
    /// Split the whole tab, the file in a new editor pane on the left.
    pub const SPLIT_LEFT: u8 = 3;
}

// swift-bridge's expansion fails this crate's stricter lints; the allow is scoped to the bridge
// module so the hand-written glue below is held to the normal bar.
#[allow(clippy::all, missing_docs, unused)]
#[swift_bridge::bridge]
mod ffi {
    // An RGB color plus a `set` flag, mirroring `Option<wolf_core::Rgb>` across
    // the bridge (`set = false` means "unset — use a fallback").
    #[swift_bridge(swift_repr = "struct")]
    struct FfiColor {
        // Packed `0x00RRGGBB`.
        rgb: u32,
        // Whether this color is set (vs. a fallback should apply).
        set: bool,
    }

    // A key event from Swift, mapped here into a `wolf_core::KeyEvent`. `kind` is a `mod kind`
    // discriminant; for a char key, `ch` is the already-shifted Unicode scalar.
    #[swift_bridge(swift_repr = "struct")]
    struct FfiKeyEvent {
        kind: u8,
        ch: u32,
        fnum: u8,
        ctrl: bool,
        shift: bool,
        alt: bool,
        meta: bool,
    }

    // A mouse event from Swift; `button`/`action` per `mod mouse_button`/`mod mouse_action`.
    // `grid` is the target window grid (>= 2 under ext_multigrid); `row`/`col` are grid-local.
    #[swift_bridge(swift_repr = "struct")]
    struct FfiMouseEvent {
        button: u8,
        action: u8,
        grid: u64,
        row: u32,
        col: u32,
        ctrl: bool,
        shift: bool,
        alt: bool,
    }

    // A scroll event from Swift; `direction` per `mod scroll_dir`. `grid` is the target window
    // grid and `row`/`col` are grid-local.
    #[swift_bridge(swift_repr = "struct")]
    struct FfiScrollEvent {
        direction: u8,
        grid: u64,
        row: u32,
        col: u32,
    }

    extern "Rust" {
        // Starts the core against the pinned Neovim (None on error). `init_lua`
        // and `cwd` are filesystem PATHS.
        fn wolf_ffi_start(
            init_lua: String,
            cwd: String,
            cols: u16,
            rows: u16,
        ) -> Option<WolfCoreHandle>;
    }

    extern "Rust" {
        type WolfCoreHandle;

        // This core's wake-routing token, minted per `wolf_ffi_start` and passed to every wake
        // callback so one process-global callback routes each wake to the right core.
        fn wake_token(&self) -> u64;
        fn read_frame(&self) -> FfiFrame;
        // One frame per pane, plus grid 1 for the cmdline strip. An absent grid yields a `0x0`
        // frame (Swift reads `cols() == 0` as "grid gone").
        fn read_frame_for_grid(&self, grid: u64) -> FfiFrame;
        fn send_key(&self, ev: FfiKeyEvent);
        // Forwards via `nvim_input_mouse` on `ev.grid` with grid-local row/col; a no-op on an
        // unrecognized button/action discriminant.
        fn send_mouse(&self, ev: FfiMouseEvent);
        // Forwards via `nvim_input_mouse("wheel", …)`; a no-op on an unrecognized direction
        // discriminant.
        fn send_scroll(&self, ev: FfiScrollEvent);
        fn resize(&self, cols: u16, rows: u16);
        // Resizes a single window `grid` (in cells); nvim echoes the authoritative
        // size back via `grid_resize`.
        fn resize_grid(&self, grid: u64, cols: u16, rows: u16);
        // Drains and returns all pending layout intents (splits + focus moves) in
        // arrival order. `FfiLayoutIntent` is opaque so a `Vec` is supported.
        fn drain_intents(&self) -> Vec<FfiLayoutIntent>;
        // Drains and returns all pending user-triggered commands in arrival order.
        // `FfiCommand` is opaque so a `Vec` is supported.
        fn drain_commands(&self) -> Vec<FfiCommand>;
        // All windows nvim currently reports (the Swift side filters live editor
        // windows). `FfiWinInfo` is opaque so a `Vec` is supported.
        fn windows(&self) -> Vec<FfiWinInfo>;
        // The grid id bound to window handle `win`, or 0 if no window maps to it.
        fn grid_for_win(&self, win: i64) -> u64;
        // The currently active Neovim window handle, or 0 if not yet known.
        fn active_win(&self) -> i64;
        // Creates a tabpage, `:tcd`'d to `cwd` unless empty, and returns its handle. Blocks briefly;
        // 0 when the core is gone (the caller drops the would-be tab).
        fn create_tabpage(&self, cwd: String) -> i64;
        // Renders `tab` (pass `FfiAppTab::restored`'s output) via `tab_rebuild_spec` and realizes
        // it in the current tabpage's lone window; returns window handles in leaf order.
        fn rebuild_tab(&self, tab: &FfiAppTab) -> Vec<i64>;
        // The current tabpage's stable HANDLE via a blocking RPC. 0 when the core is gone.
        fn current_tabpage(&self) -> i64;
        // `Snapshot.current_tabpage` (updated via `wolf_tabpage`), no RPC, so a dead core answers it.
        // 0 when the core is gone or not yet known.
        fn active_tabpage(&self) -> i64;
        // A tabpage's canonicalized `:tcd` cwd, current or not. Blocks briefly; empty when the core
        // is gone or the handle is invalid (e.g. a closed tabpage).
        fn cwd_for_tabpage(&self, tab: i64) -> String;
        // Makes tabpage `tab` current. A no-op when the core is gone.
        fn set_current_tabpage(&self, tab: i64);
        // Closes tabpage `tab`; a last-tabpage close is swallowed, since the last one goes with the
        // core's shutdown.
        fn close_tabpage(&self, tab: i64);
        // Focuses a specific Neovim window (click-to-focus / resolved FocusDir).
        fn focus_win(&self, win: i64);
        // Hands `item`'s wait to this core, queued behind the file's placement; a wait it cannot
        // take ends with 1.
        fn wait_on(&self, item: &FfiOpenItem);
        // `mod liveness`, a lock-free read with no RPC, polled on each wake to drive the crash/stall
        // overlay. DEAD when the core is gone.
        fn liveness(&self) -> u8;
        // Force-kills and respawns the captive nvim in place, re-running setup; false when the core
        // is gone or the respawn failed. Capture `open_files_for_tabpage` first: a respawn wipes it.
        fn restart(&self) -> bool;
        // The Tier-1 open-file mirror for `tab`, a snapshot read with no RPC; capture it before a
        // `restart`. Empty-backed when the core is gone or the tab is unknown.
        fn open_files_for_tabpage(&self, tab: i64) -> FfiOpenFiles;
        // `nvim_ui_set_focus`: true when the core's tab gains focus (FocusGained -> WolfAutoRead
        // `:checktime`), false when it loses it. nvim fires the autocmd on every call.
        fn set_ui_focus(&self, gained: bool);
        fn shutdown(&self);
        // A streaming file search over the core's cwd, fed by the file-search wake. Infallible:
        // empty-backed when the core is gone.
        fn start_file_search(&self) -> FfiFileSearch;
        // A separate long-lived file search for the file-tree sidebar, fed by the tree wake.
        // Same empty-backed fallback as `start_file_search`.
        fn start_tree_search(&self) -> FfiFileSearch;
        // A ripgrep-backed content search fed by the search wake; the bools are the panel's toggles.
        // Empty-backed when the core is gone or `WOLF_RG_BIN` is unset.
        fn start_search(
            &self,
            query: String,
            use_regex: bool,
            case_sensitive: bool,
            all_files: bool,
        ) -> FfiSearch;
        // nvim's `ignorecase`, read once to seed the panel's case-sensitive toggle as its inverse.
        // Blocks briefly; false on error, timeout or shutdown.
        fn ignorecase(&self) -> bool;
        // Opens `path` at `line` (1-based) and `col` (0-based byte offset) via `InputEvent::OpenAt`;
        // the input pump records the MRU. A no-op when the core is gone.
        fn open_at(&self, path: String, line: u64, col: u64);
        // A fresh palette session seeded with recent dirs + the command MRU; the front-end adds each
        // other window via `add_window`, then calls `ranked`. Empty-backed when the core is gone.
        fn command_palette(&self) -> FfiCommandPalette;
        // The absolute path of the buffer currently being edited (canonicalized
        // by core), or the empty string for an unnamed buffer / shut-down core.
        fn current_file(&self) -> String;
        // nvim's live, canonicalized cwd (tracks `:cd`/`DirChanged`); empty until nvim first reports
        // it, and after shutdown.
        fn cwd(&self) -> String;
        // `WolfCore::recent_dirs` as strings; empty once the core is gone.
        fn recent_dirs(&self) -> Vec<String>;
        // `WolfCore::record_recent_dir`, Open Folder's record of the folder it opened. A no-op once
        // the core is gone.
        fn record_recent_dir(&self, path: String);
        // The `WolfTitlebar` group's background, which macOS prefers to `Normal`'s for the titlebar;
        // unset when the group is absent or cleared, or the core is gone.
        fn theme_titlebar_bg(&self) -> FfiColor;
        // The `WolfTitlebar` group's foreground, carried and applied by no front-end. Same unset
        // convention as `theme_titlebar_bg`.
        fn theme_titlebar_fg(&self) -> FfiColor;
        // The statusbar strings for the window drawing into `grid`. An unnamed buffer, a grid with no
        // window or a shut-down core yields `[No Name]` with an empty display path.
        fn pane_status_for_grid(&self, grid: u64) -> FfiPaneStatus;
        // What window `win` shows, keyed by handle and global across tabpages, unlike
        // `pane_status_for_grid`. Not reported for an unknown window or a shut-down core.
        fn win_buffer(&self, win: i64) -> FfiWinBuffer;
        // Window `win`'s cursor and top line, read from Neovim for the state capture. Blocks
        // briefly; not found for `win <= 0`, an unknown window, an error or a shut-down core.
        fn window_cursor(&self, win: i64) -> FfiWindowCursor;
        // Opens `path` (an absolute filesystem path) in Neovim via
        // `InputEvent::Open`; the core's input pump records the MRU.
        fn open(&self, path: String);
        // Changes the current tabpage's cwd to absolute `path` via `InputEvent::Chdir`, which is
        // `:tcd`; a core with one tabpage, as a macOS tab's is, moves as a whole.
        fn chdir(&self, path: String);
        // `:set background=dark|light` via `InputEvent::SetBackground`. The macOS front-end pushes
        // the resolved appearance at launch and live in `auto` mode. A no-op when the core is gone.
        fn set_background(&self, dark: bool);
        // Closes the current split (the pane) via `InputEvent::ClosePane`, which refuses the
        // tab's last pane.
        fn close_pane(&self);
        // Splits the active pane, stacked (`:split`) or side by side (`:vsplit`), through
        // `WolfCore::split_pane`; Neovim's current window decides the new pane's kind.
        fn split_horizontal(&self);
        fn split_vertical(&self);
        // Opens an empty buffer named the next free `Untitled N.md` (markdown-typed, never written)
        // via `InputEvent::NewBuffer` (the `wolf_new_untitled` Lua helper).
        fn new_buffer(&self);
        // Toggles the terminal drawer through `WolfCore::toggle_terminal`, which
        // starts the drawer's session when none is live.
        fn toggle_terminal(&self);
        // Toggles content width between the ~90-char default and full pane width via the
        // `wolf_toggle_full_width` Lua helper, which targets a preview scratch's source buffer.
        fn toggle_full_width(&self);
        // Whether the active buffer (a preview scratch's source) has a truthy `b:wolf_full_width`.
        // Drives the View > Use full width for content checkmark; false when the core is gone.
        fn is_full_width_active(&self) -> bool;
        // The per-window full-width flag the front-end threads into a pane's geometry (`maxCols: nil`
        // vs `90`). False when the core is gone or `win` is not in the map.
        fn full_width_for_win(&self, win: i64) -> bool;
        // The visual selection yanked to text (blocks briefly); empty outside visual mode, for an empty
        // selection, or when the core is gone. The front-end writes the pasteboard only when non-empty.
        fn copy_selection(&self) -> String;
        // Pastes `text` into the active document via `InputEvent::Paste`
        // (`nvim_paste`). A no-op when the core is gone.
        fn paste(&self, text: String);
        // A fresh buffer-picker session seeded from a blocking snapshot of the listed buffers plus
        // the live cwd; the front-end filters via `ranked`. Empty-backed when the core is gone.
        fn buffer_picker(&self) -> FfiBufferPicker;
        // Switches the live active window to `bufnr` (`nvim_win_set_buf` + per-pane view restore).
        // A no-op when the core is gone or no active window is known.
        fn switch_buffer(&self, bufnr: i64);
        // Whether the active document is in Neovim visual mode (`v`/`V`/`<C-v>`).
        // Gates the Edit > Copy menu item. False when the core is gone.
        fn is_visual_mode(&self) -> bool;
        // Whether the active buffer's filetype is `markdown`; gates View > Preview and its palette
        // row. False when the core is gone.
        fn is_markdown_active(&self) -> bool;
        // Whether the active buffer is a preview (carries `b:wolf_preview_source`); gates View >
        // Edit. False when the core is gone.
        fn is_preview_active(&self) -> bool;
        // Replaces the active preview pane's buffer with its source in place and returns the source
        // bufnr. Blocks briefly; 0 off a preview, on error, or when the core is gone.
        fn edit_source(&self) -> i64;
        // Replaces the active pane's Markdown buffer with its preview in place and returns the
        // scratch bufnr. Blocks briefly; 0 off Markdown, on error, or when the core is gone.
        fn open_preview(&self) -> i64;
        // Renders Markdown buffer `source_bufnr` to the themed HTML document the web view loads.
        // Blocks briefly; empty on error or when the core is gone.
        fn render_preview(&self, source_bufnr: i64) -> String;
        // Starts or reuses the loopback-only preview server pinned at the active Markdown buffer and
        // returns its `http://127.0.0.1:<port>/<relpath>` URL. Blocks briefly; empty on failure.
        fn open_in_browser(&self) -> String;
        // Repoints preview window `win` at the Markdown file at absolute `path` (a clicked link) and
        // returns its source bufnr. Blocks briefly; 0 on failure, and the front-end falls back.
        fn preview_file(&self, win: i64, path: String) -> i64;
        // Previews open buffer `bufnr` in window `win` (0 = current); returns the source bufnr.
        // Blocks briefly; 0 if invalid, unnamed or not Markdown, and the front-end uses switch_buffer.
        fn preview_buffer(&self, win: i64, bufnr: i64) -> i64;
        // Previews the active Markdown file in `target_win` if usable, else splits right (a vertical
        // Split intent); focus stays put. Blocks briefly; returns the source bufnr, 0 on failure.
        fn preview_in_other(&self, target_win: i64) -> i64;
        // Edits the active preview's source in `target_win` if usable, else splits (source lands
        // left), and focuses that pane. Blocks briefly; returns the bound bufnr, 0 on failure.
        fn edit_in_other(&self, target_win: i64) -> i64;
        // Splits the whole tab with `path` in a new current window on the left, and returns its
        // handle. No Split intent follows. Blocks briefly; 0 on failure.
        fn open_split_left(&self, path: String) -> i64;
        // Mints a fresh tab ID, gives it to the core for the terminal sessions it starts, and
        // returns it lowercase; "" once the core is gone.
        fn assign_tab_id(&self) -> String;
        // One `FfiPreview` per window showing a preview scratch, joined to its source's live
        // changedtick. Empty when the core is gone.
        fn previews(&self) -> Vec<FfiPreview>;
        // Whether pane `win` can go Back in its navigation history; a snapshot read with no RPC.
        // False for an unknown window, a never-navigated pane, the terminal drawer, or a gone core.
        fn history_can_back(&self, win: i64) -> bool;
        // Whether pane `win` has a newer entry to go Forward to. Same contract.
        fn history_can_forward(&self, win: i64) -> bool;
        // Moves pane `win` one entry back (blocks briefly) and returns whether it actually moved;
        // false at the bottom of the stack, for an unknown window, on error, or for a gone core.
        fn history_back(&self, win: i64) -> bool;
        // Moves pane `win` one entry FORWARD. Same contract as `history_back`.
        fn history_forward(&self, win: i64) -> bool;
        // Moves pane `win` to stack `index` (an `FfiHistoryEntry::index`); false when it does not
        // move, including for the cursor's own index, so clicking the checkmarked row is a no-op.
        fn history_jump(&self, win: i64, index: u32) -> bool;
        // Pane `win`'s History-menu rows, newest first, capped at 10 distinct targets, with final
        // labels and at most one `is_current`. Empty when the core is gone.
        fn history_entries(&self, win: i64) -> Vec<FfiHistoryEntry>;
        // The native command line (`ext_cmdline`); inactive when none is showing or the core is gone.
        fn cmdline(&self) -> FfiCmdline;
        // The completion popup (`ext_popupmenu`) as an opaque `FfiPum`
        // (visible / selected / anchor + items). Hidden when the core is gone.
        fn pum(&self) -> FfiPum;
        // The message toast (`ext_messages`); chunk colors are resolved in Rust against the frame's
        // highlight table. Hidden when the core is gone.
        fn toast(&self) -> FfiToast;
        // Starts a session in `win` (0 = current) per `mod terminal_target`; "" runs the user's
        // shell, else `/bin/sh -c command`. Blocks briefly; 0 on failure or a gone core.
        fn open_terminal(&self, win: i64, target: u8, command: String) -> u64;
        // Shows live session `id` in `win` per `mod terminal_target`; false for an unknown id,
        // an unknown target or a gone core. Blocks briefly.
        fn bind_terminal(&self, id: u64, win: i64, target: u8) -> bool;
        // `bind_terminal` queued behind earlier input; false for an id with no running
        // session, an unknown target or a gone core. Never blocks.
        fn queue_bind_terminal(&self, id: u64, win: i64, target: u8) -> bool;
        // The directory session `id`'s shell is in; "" when unknown or the core is gone.
        fn terminal_cwd(&self, id: u64) -> String;
        // Session `id`'s latest frame; the empty frame (version 0, 0x0) for an unknown id or a
        // gone core.
        fn terminal_frame(&self, id: u64) -> FfiTerminalFrame;
        // A key through core's `<C-w>` router to session `id`; an unknown kind is dropped.
        fn terminal_key(&self, id: u64, ev: FfiKeyEvent);
        // Text as typed, such as an input method's commit.
        fn terminal_text(&self, id: u64, text: String);
        // A paste, bracketed when the program set mode 2004.
        fn terminal_paste(&self, id: u64, text: String);
        // `kind` per `mod terminal_mouse_kind`, `button` per `mod mouse_button` (NONE included),
        // `mods` per `mod terminal_mods`; x and y in the cell pixels of the last resize.
        fn terminal_mouse(&self, id: u64, kind: u8, button: u8, x: f64, y: f64, mods: u8);
        // A wheel scroll of `lines`; `direction` per `mod scroll_dir`, the rest as terminal_mouse.
        fn terminal_scroll(&self, id: u64, direction: u8, lines: u16, x: f64, y: f64, mods: u8);
        // Pane focus or window activation, reported to a program that set mode 1004.
        fn terminal_focus(&self, id: u64, gained: bool);
        // Sizes session `id` and its pty from the pane's content area, in cells and pixels.
        fn resize_terminal(&self, id: u64, cols: u16, rows: u16, cell_w_px: u32, cell_h_px: u32);
        // Hangs session `id` up.
        fn close_terminal(&self, id: u64);
        // The running sessions, ascending; empty when the core is gone.
        fn live_terminals(&self) -> Vec<u64>;
        // The text of session `id`'s selection; "" when nothing is selected.
        fn terminal_selection_text(&self, id: u64) -> String;
    }

    extern "Rust" {
        type FfiTerminalFrame;

        // Monotonic per session; 0 for the empty frame.
        fn version(&self) -> u64;
        fn cols(&self) -> u16;
        fn rows(&self) -> u16;
        // The frame version at which `row` last changed; 0 out of range.
        fn row_version(&self, row: u16) -> u64;
        // Row `row`'s cells, left to right; empty out of range.
        fn row_cells(&self, row: u16) -> Vec<FfiTerminalCell>;
        // Whether `row` has selected columns, from `row_selection_start` to `_end` inclusive.
        fn row_has_selection(&self, row: u16) -> bool;
        fn row_selection_start(&self, row: u16) -> u16;
        fn row_selection_end(&self, row: u16) -> u16;
        // False while the cursor is scrolled out of view; the cursor accessors then read 0.
        fn has_cursor(&self) -> bool;
        fn cursor_col(&self) -> u16;
        fn cursor_row(&self) -> u16;
        fn cursor_visible(&self) -> bool;
        fn cursor_blinking(&self) -> bool;
        // `mod terminal_cursor_style`.
        fn cursor_style(&self) -> u8;
        fn default_fg(&self) -> FfiColor;
        fn default_bg(&self) -> FfiColor;
        // Unset when the editor sets no cursor colour.
        fn cursor_color(&self) -> FfiColor;
        fn scrollbar_total(&self) -> u64;
        fn scrollbar_offset(&self) -> u64;
        fn scrollbar_len(&self) -> u64;
        // Whether the program tracks the mouse.
        fn mouse_tracking(&self) -> bool;
        fn title(&self) -> String;
        // The directory the shell last reported; "" for none.
        fn pwd(&self) -> String;
        // The basename of the session's program.
        fn shell(&self) -> String;
        fn has_exited(&self) -> bool;
        // The exit code, or 128 plus the signal; 0 while running.
        fn exit_code(&self) -> i32;
    }

    extern "Rust" {
        type FfiTerminalCell;

        // The grapheme cluster; "" for an empty cell or a spacer.
        fn text(&self) -> String;
        // Unset for the frame's default; never swapped for inverse.
        fn fg(&self) -> FfiColor;
        fn bg(&self) -> FfiColor;
        // `mod terminal_flag` bits.
        fn flags(&self) -> u8;
        // `mod terminal_underline`.
        fn underline(&self) -> u8;
        // `mod terminal_cell_width`.
        fn width(&self) -> u8;
    }

    extern "Rust" {
        type FfiOpenFiles;

        // The tabpage's active buffer's absolute path, or "" for an unnamed buffer.
        fn active_path(&self) -> String;
        // The instance-global listed file-backed paths (absolute), not per-tabpage, to reopen after
        // a restart; reopen the active one last so it lands focused.
        fn files(&self) -> Vec<String>;
    }

    extern "Rust" {
        type FfiPreview;

        // The preview WINDOW handle (the pane showing the web view).
        fn win(&self) -> i64;
        // The SOURCE Markdown buffer's number (passed to `render_preview`).
        fn source_bufnr(&self) -> i64;
        // The source file's BASENAME (e.g. "alpha.md"), for the pane statusbar.
        fn source_name(&self) -> String;
        // The source file's absolute path.
        fn source_path(&self) -> String;
        // The source\'s cwd/home-relative display path (e.g. "crates/wolf-macos/README.md"), for
        // the preview pane's secondary statusbar line.
        fn source_display_path(&self) -> String;
        // The source buffer's current `changedtick`; the front-end reloads the web
        // view when this advances (0 until the first `wolf_markdown_dirty`).
        fn source_tick(&self) -> u64;
        // Whether the source buffer has `b:wolf_full_width`; the web view then fills the inset pane
        // instead of the capped, centered ~90-char rect.
        fn source_full_width(&self) -> bool;
        // The source cursor line, 1-based to match the preview's `wl-{L}-{C}` markers. 0 until the
        // first `wolf_markdown_cursor`, meaning "unknown": the front-end does not scroll.
        fn source_cursor_line(&self) -> u32;
        // The SOURCE buffer's cursor COLUMN — a 0-based CHARACTER column, NOT a byte
        // column. `0` until the first `wolf_markdown_cursor`.
        fn source_cursor_col(&self) -> u32;
    }

    extern "Rust" {
        type FfiHistoryEntry;

        // The disambiguated menu label, e.g. "bar/README.md", "Preview: alpha.md",
        // "Terminal 2", "Untitled". Already home-abbreviated; draw it verbatim.
        fn display(&self) -> String;
        // The STACK index to hand back to `history_jump`. For the checkmarked row
        // this is the CURSOR's own index, so clicking it is a true no-op.
        fn index(&self) -> u32;
        // Whether this row draws the checkmark (at most one row per menu; NONE when
        // the cursor's target fell outside the 10-distinct-row cap).
        fn is_current(&self) -> bool;
    }

    extern "Rust" {
        type FfiFileSearch;

        // The top `max` entries for `query` against the latest streamed snapshot; an empty `query`
        // lists all entries MRU-first.
        fn ranked(&self, query: String, max: usize) -> Vec<FfiFileEntry>;
        // The immediate children of `dir` ("" = the cwd root) from the latest snapshot, directories
        // first then files, each case-insensitive by name.
        fn children_of(&self, dir: String) -> Vec<FfiTreeNode>;
        // Maps an absolute path to its tree-display rel-path (EXACT entry-path
        // match), or the empty string when no streamed entry matches.
        fn rel_path_for(&self, abs: String) -> String;
        // The current file-search snapshot version (0 when no session backs this
        // handle). The sidebar reloads its outline only when this advances.
        fn version(&self) -> u64;
    }

    extern "Rust" {
        type FfiFileEntry;

        // The display string (path relative to the search root) shown + ranked.
        fn display(&self) -> String;
        // The absolute filesystem path, handed back to `WolfCoreHandle::open`.
        fn path(&self) -> String;
    }

    extern "Rust" {
        type FfiTreeNode;

        // The immediate component name (the last segment of `rel_path`).
        fn name(&self) -> String;
        // The full relative, `/`-separated path from the cwd root to this node.
        fn rel_path(&self) -> String;
        // The absolute filesystem path for a File node; the empty string for a
        // Dir node.
        fn abs_path(&self) -> String;
        // Whether this node is a directory (vs. a leaf file).
        fn is_dir(&self) -> bool;
    }

    extern "Rust" {
        // The ancestor dir rel-paths of `rel_path`, root down to its parent (`"src/main.rs"` ->
        // `["src"]`). A free fn delegating to `wolf_core::tree`.
        fn wolf_ffi_ancestors_to_reveal(rel_path: String) -> Vec<String>;
    }

    extern "Rust" {
        type FfiSearch;

        // The content-search snapshot version (0 when no session backs this handle); the panel
        // reloads its results only when it advances.
        fn version(&self) -> u64;
        // The matched files from the latest snapshot, sorted case-insensitively by rel-path, each
        // carrying its matching lines windowed to `budget` characters.
        fn files(&self, budget: u32) -> Vec<FfiSearchFile>;
    }

    extern "Rust" {
        type FfiSearchFile;

        // The display label: "<basename> - <relative parent dir>" (just the
        // basename for a root file).
        fn label(&self) -> String;
        // The file's `/`-separated path relative to the search root (the sort key).
        fn rel_path(&self) -> String;
        // The absolute filesystem path, handed back to `WolfCoreHandle::open_at`.
        fn abs_path(&self) -> String;
        // The matching lines in this file, ordered by line number. `FfiSearchMatch`
        // is opaque so a `Vec<FfiSearchMatch>` is supported.
        fn matches(&self) -> Vec<FfiSearchMatch>;
    }

    extern "Rust" {
        type FfiSearchMatch;

        // The 1-based line number of the match.
        fn line_number(&self) -> u64;
        // The 0-based byte column of the first submatch (for cursor placement).
        fn column(&self) -> u32;
        // The visible (budget-windowed) line text; the front-end wraps it across
        // up to two lines and prefixes/suffixes an ellipsis per `truncated_*`.
        fn display_text(&self) -> String;
        // Whether context was hidden on the LEFT of the window (leading ellipsis).
        fn truncated_left(&self) -> bool;
        // Whether context was hidden on the RIGHT of the window (trailing ellipsis).
        fn truncated_right(&self) -> bool;
        // The bold ranges within `display_text` as parallel byte-offset vectors
        // (`range_starts[i]..range_ends[i]`), avoiding an extra opaque type.
        fn range_starts(&self) -> Vec<u32>;
        fn range_ends(&self) -> Vec<u32>;
    }

    extern "Rust" {
        type FfiConfig;

        // The resolved font family name (e.g. "iA Writer Mono S").
        fn font_family(&self) -> String;
        // The resolved font size, in points.
        fn font_size(&self) -> f64;
        // The resolved line-height, as a percentage of the font size (130 == 130%).
        fn line_height(&self) -> f64;
        // The resolved appearance mode as a discriminant (see the `appearance`
        // module: AUTO=0, DARK=1, LIGHT=2). Swift maps it via `AppearanceMode`.
        fn appearance(&self) -> u8;
        // On macOS, whether the Option key acts as Alt (`<M-…>` mappings) rather
        // than composing accented characters. Defaults to false (option-as-option).
        fn macos_option_as_alt(&self) -> bool;
    }

    extern "Rust" {
        // Loads the resolved user config, reading or creating `wolf-config.yaml` in the platform
        // prefs dir. A free fn needing no live core; infallible (defaults on any error).
        fn wolf_ffi_load_config() -> FfiConfig;
    }

    extern "Rust" {
        // A free fn needing no live core: `wolf_core::FONT_LICENSE_NOTICES`, verbatim.
        fn wolf_ffi_font_license_notices() -> String;
    }

    extern "Rust" {
        // A free fn needing no live core: `wolf_core::LIBRARY_LICENSE_NOTICES`, verbatim.
        fn wolf_ffi_library_license_notices() -> String;
    }

    extern "Rust" {
        type FfiAppState;

        // The saved windows, frontmost first; empty when nothing was saved.
        fn windows(&self) -> Vec<FfiAppWindow>;
    }

    extern "Rust" {
        type FfiAppWindow;

        // Whether the frame carries a position; false means x and y read 0.
        fn has_position(&self) -> bool;
        fn x(&self) -> f64;
        fn y(&self) -> f64;
        fn width(&self) -> f64;
        fn height(&self) -> f64;
        fn active_tab(&self) -> u32;
        fn minimized(&self) -> bool;
        fn tabs(&self) -> Vec<FfiAppTab>;
    }

    extern "Rust" {
        type FfiAppTab;

        // The tab's directory: "" for an empty tab, and the asked one for a project state's tab.
        fn directory(&self) -> String;
        fn has_root(&self) -> bool;
        // The root node, or an empty edit pane when `has_root` is false.
        fn root(&self) -> FfiAppNode;
        // A `pane_ref` discriminant, and the pane index that `PANE` reads.
        fn focused_kind(&self) -> u8;
        fn focused_index(&self) -> u32;
        fn zoomed_kind(&self) -> u8;
        fn zoomed_index(&self) -> u32;
        fn sidebar_visible(&self) -> bool;
        // A `sidebar_mode` discriminant.
        fn sidebar_mode(&self) -> u8;
        fn has_query(&self) -> bool;
        fn query(&self) -> String;
        // Each a `tristate` discriminant.
        fn case_sensitive(&self) -> u8;
        fn use_regex(&self) -> u8;
        fn all_files(&self) -> u8;
        fn has_drawer(&self) -> bool;
        fn drawer_open(&self) -> bool;
        fn drawer_share(&self) -> f64;
        // The tab as `app_state::restored_tab` restores it: missing files become empty panes.
        fn restored(&self) -> FfiAppTab;
    }

    extern "Rust" {
        type FfiAppNode;

        // Whether this node is a pane (vs. a layout). A field that does not apply reads 0 or "".
        fn is_pane(&self) -> bool;
        // A `pane_kind` discriminant.
        fn kind(&self) -> u8;
        fn path(&self) -> String;
        fn has_cursor(&self) -> bool;
        fn cursor_line(&self) -> u32;
        fn cursor_col(&self) -> u32;
        fn has_top_line(&self) -> bool;
        fn top_line(&self) -> u32;
        fn has_scroll(&self) -> bool;
        fn scroll(&self) -> f64;
        // An `orientation` discriminant (0 = horizontal, 1 = vertical).
        fn orientation(&self) -> u8;
        fn child_count(&self) -> u32;
        // The weight of child `index` (0.0 out of range).
        fn weight(&self, index: u32) -> f64;
        // Child `index` (an empty edit pane out of range).
        fn child(&self, index: u32) -> FfiAppNode;
    }

    extern "Rust" {
        type FfiProjectState;

        // Whether the directory has a project state; every other field reads 0 when false.
        fn found(&self) -> bool;
        fn has_position(&self) -> bool;
        fn x(&self) -> f64;
        fn y(&self) -> f64;
        fn width(&self) -> f64;
        fn height(&self) -> f64;
        fn tab(&self) -> FfiAppTab;
    }

    extern "Rust" {
        type FfiProjectTab;

        // Whether a tab is open on the directory; `window` and `tab` read 0 when false.
        fn found(&self) -> bool;
        // The window's index in the scratch, frontmost first.
        fn window(&self) -> u32;
        // The tab's index in that window, in tab-bar order.
        fn tab(&self) -> u32;
    }

    extern "Rust" {
        // Clears the app-state scratch: the window state, its recency and the node stack.
        fn wolf_ffi_app_state_begin();
        // Pushes a window; x and y are dropped when `has_position` is false.
        fn wolf_ffi_app_state_add_window(
            has_position: bool,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            active_tab: u32,
        );
        // Sets the last window's `minimized`; a no-op with no window.
        fn wolf_ffi_app_state_window_minimized(minimized: bool);
        // Pushes a tab onto the last window; each reference is a `pane_ref` kind and an index.
        fn wolf_ffi_app_state_add_tab(
            directory: String,
            sidebar_visible: bool,
            focused_kind: u8,
            focused_index: u32,
            zoomed_kind: u8,
            zoomed_index: u32,
        );
        // Sets the last tab's sidebar mode (`sidebar_mode`), query and toggles.
        fn wolf_ffi_app_state_tab_sidebar(
            mode: u8,
            query: String,
            case_sensitive: bool,
            use_regex: bool,
            all_files: bool,
        );
        // Sets the last tab's drawer.
        fn wolf_ffi_app_state_tab_drawer(open: bool, share: f64);
        // Pushes a pane (`pane_kind`) weighing `weight` in its parent: a post-order replay.
        fn wolf_ffi_app_state_push_pane(kind: u8, path: String, weight: f64);
        // Sets the cursor and top line of the pane just pushed, when it is an edit pane.
        fn wolf_ffi_app_state_pane_cursor(line: u32, col: u32, top_line: u32);
        // Sets the scroll of the pane just pushed, when it is a preview pane.
        fn wolf_ffi_app_state_pane_scroll(scroll: f64);
        // Pops `child_count` nodes into a layout (`orientation`) weighing `weight`.
        fn wolf_ffi_app_state_push_layout(orientation: u8, child_count: u32, weight: f64);
        // Moves the one remaining node to the last window's last tab's root.
        fn wolf_ffi_app_state_commit_root();
        // Appends a tab index to the last window's recency, most recent first.
        fn wolf_ffi_app_state_push_recency(tab: u32);
        // Submits the scratch window state with its recency to the shared app-state store.
        fn wolf_ffi_app_state_commit();
        // Blocks until the pending app-state write is attempted (bounded by
        // `STATE_FLUSH_TIMEOUT`); true when all flushed in time or no store exists.
        fn wolf_ffi_app_state_flush() -> bool;
        // The saved window state, for launch; empty when nothing was saved or readable.
        fn wolf_ffi_app_state_window_state() -> FfiAppState;
        // `directory`'s project state, for Open Folder, which marks it used.
        fn wolf_ffi_app_state_project_state(directory: String) -> FfiProjectState;
        // The tab Open Folder focuses for `directory`, by `app_state::project_tab` over the
        // scratch and its recency. Clears nothing and never opens the store.
        fn wolf_ffi_app_state_project_tab(directory: String) -> FfiProjectTab;
        // Where `file` goes in the scratch's last window's last tab, by `file_open::choose_pane`.
        // Reads the scratch only.
        fn wolf_ffi_app_state_choose_pane(file: String) -> FfiPaneChoice;
        // `file`'s project directory, by `file_open::project_directory` over the directories of
        // every scratch tab. Reads the scratch only.
        fn wolf_ffi_app_state_project_directory(file: String) -> String;
    }

    extern "Rust" {
        type FfiPaneChoice;

        // Whether the tab's zoom is undone first.
        fn unzoom(&self) -> bool;
        // A `pane_placement` discriminant.
        fn placement(&self) -> u8;
        // The depth-first pane index the placement names; 0 for `SPLIT_LEFT`.
        fn pane(&self) -> u32;
    }

    extern "Rust" {
        type FfiOpenItem;

        // An `open_kind` discriminant.
        fn kind(&self) -> u8;
        // The resolved path; "" for an activate request.
        fn path(&self) -> String;
        // The request's tab ID, lowercase; "" when it carried none.
        fn tab_id(&self) -> String;
        // Whether the item still holds a `wolf --wait`, for `WolfCoreHandle::wait_on`.
        fn waits(&self) -> bool;
    }

    extern "Rust" {
        // Binds the process-wide open listener in `state_dir`, or core's state dir when it is "",
        // dropping a listener already bound first. "" on success, else the error's message.
        fn wolf_ffi_open_listen(state_dir: String) -> String;
        // Drops the listener, which removes its socket. A no-op with none.
        fn wolf_ffi_open_stop();
        // The requests taken since the last drain, in arrival order; empty with no listener.
        // Take the items with `pop()`: iterating borrows them, and the vec's drop ends each wait.
        fn wolf_ffi_open_drain() -> Vec<FfiOpenItem>;
        // `path` resolved as a request with no tab ID, which is Finder's route; "" activates.
        fn wolf_ffi_open_item(path: String) -> FfiOpenItem;
    }

    extern "Rust" {
        type FfiFrame;

        fn cols(&self) -> u16;
        fn rows(&self) -> u16;
        fn cursor_row(&self) -> u16;
        fn cursor_col(&self) -> u16;
        fn cursor_shape(&self) -> u8;
        fn cursor_cell_percentage(&self) -> u32;
        fn cursor_visible(&self) -> bool;
        // The grid id this frame was packed from.
        fn grid_id(&self) -> u64;
        // The grid id the cursor currently lives on (at pack time), so a pane can
        // tell whether the cursor belongs to it even when it is not its own grid.
        fn cursor_grid(&self) -> u64;
        fn default_fg(&self) -> FfiColor;
        fn default_bg(&self) -> FfiColor;
        // Reverse-video cursor colors, as two scalar accessors because swift-bridge does not emit
        // `Vec` support for transparent structs.
        fn cursor_fill(&self) -> FfiColor;
        fn cursor_glyph(&self) -> FfiColor;
        // The flat, row-major grid of resolved cells (`cols * rows` entries).
        // `FfiCell` is opaque (Vec support is generated for opaque types).
        fn cells(&self) -> Vec<FfiCell>;
        // The viewport's first visible buffer line (0-based, per `win_viewport`), for the synthetic
        // scrollbar; 0 when this grid has no window.
        fn topline(&self) -> u64;
        // One past the viewport's last visible buffer line; 0 when no window.
        fn botline(&self) -> u64;
        // The cursor's buffer line; 0 when no window.
        fn curline(&self) -> u64;
        // The buffer's total line count; 0 when no window.
        fn linecount(&self) -> u64;
    }

    extern "Rust" {
        type FfiCell;

        // The displayed text (one grapheme cluster), empty for a wide-char
        // trailing placeholder or a never-painted cell.
        fn text(&self) -> String;
        // Resolved foreground color.
        fn fg(&self) -> FfiColor;
        // Resolved background color.
        fn bg(&self) -> FfiColor;
        // Resolved special (underline) color.
        fn special(&self) -> FfiColor;
        // Packed style flags (bold/italic/underline/undercurl/strikethrough).
        fn flags(&self) -> u8;
    }

    extern "Rust" {
        type FfiLayoutIntent;

        // The intent kind (`intent_kind::SPLIT` / `FOCUS_DIR` / `DRAWER_OPEN`).
        fn kind(&self) -> u8;
        // For a SPLIT: the orientation (`orientation::HORIZONTAL` / `VERTICAL`);
        // 0 for a FOCUS_DIR / DRAWER_OPEN.
        fn orientation(&self) -> u8;
        // For a FOCUS_DIR: the direction (`focus_dir::LEFT` / `RIGHT` / `UP` /
        // `DOWN`); 0 for a SPLIT / DRAWER_OPEN.
        fn dir(&self) -> u8;
        // The origin window handle (current before the split / when the chord
        // fired); 0 for a DRAWER_OPEN.
        fn from(&self) -> i64;
        // For a SPLIT: the new window handle; for a DRAWER_OPEN: the new drawer
        // window handle; 0 for a FOCUS_DIR.
        fn win(&self) -> i64;
    }

    extern "Rust" {
        type FfiCommand;

        // The command kind, a `command_kind` constant.
        fn kind(&self) -> u8;
    }

    extern "Rust" {
        // A free fn needing no live core: the number of registered commands,
        // `wolf_core::COMMAND_SPECS.len()`, one past the highest `command_kind`.
        fn wolf_ffi_command_count() -> u8;
        // A free fn needing no live core: the `command_kind` of registry id `id`, or
        // `u8::MAX` when no command has that id.
        fn wolf_ffi_command_kind(id: String) -> u8;
    }

    extern "Rust" {
        type FfiCommandPalette;

        // Adds one other open window (the caller excludes the current one): `index` is its raise
        // target, `cwd` its working directory ("" for an empty-state window, shown as "Untitled").
        fn add_window(&self, index: u16, cwd: String);
        // The top `max` rows for `query`: static commands (minus CommandPalette), added windows and
        // recent dirs, ranked MRU-first via `wolf_core::rank_palette`.
        fn ranked(&self, query: String, max: usize) -> Vec<FfiCommandRow>;
        // Records `key` (a row's `mru_key`) into the global command MRU on
        // activation, so it floats to the top of a later `ranked("")`.
        fn record(&self, key: String);
    }

    extern "Rust" {
        type FfiCommandRow;

        // The label shown in the palette (and ranked against the query).
        fn display(&self) -> String;
        // The row kind (`palette_row_kind::STATIC` / `SWITCH_WINDOW` /
        // `OPEN_RECENT`).
        fn row_kind(&self) -> u8;
        // For a STATIC row: the `command_kind` discriminant to dispatch; 0 for a
        // dynamic row.
        fn command_kind(&self) -> u8;
        // For a SWITCH_WINDOW row: the raise-target window index; 0 otherwise.
        fn window_index(&self) -> u16;
        // For an OPEN_RECENT row: the recent-folder path; `""` otherwise.
        fn path(&self) -> String;
        // The opaque MRU key handed back to `record` on activation.
        fn mru_key(&self) -> String;
    }

    extern "Rust" {
        type FfiBufferPicker;

        // The top `max` rows for `query`, ordered by lastused among matches via `rank_buffers` over
        // the `buffer_picker()` snapshot; an empty query lists all, current buffer at row 0.
        fn ranked(&self, query: String, max: usize) -> Vec<FfiBufferRow>;
    }

    extern "Rust" {
        type FfiBufferRow;

        // The Neovim buffer handle to switch to (passed to
        // `WolfCoreHandle::switch_buffer`).
        fn bufnr(&self) -> i64;
        // The cwd-relative (or absolute / `[No Name]`) display label, also the
        // string ranked against the query.
        fn display(&self) -> String;
        // Whether the buffer has unsaved modifications (for the row chrome).
        fn modified(&self) -> bool;
    }

    extern "Rust" {
        type FfiWinInfo;

        // The grid id this window draws into (`>= 2` for editor windows).
        fn grid(&self) -> u64;
        // The Neovim window handle, as an `i64`.
        fn win(&self) -> i64;
        // Whether the window is currently hidden.
        fn hidden(&self) -> bool;
    }

    extern "Rust" {
        type FfiPaneStatus;

        // `PaneStatus::filename` of the pane.
        fn filename(&self) -> String;
        // `PaneStatus::display_path` of the pane.
        fn display_path(&self) -> String;
        // The buffer's raw ABSOLUTE path (never relativized; empty for an unnamed
        // buffer). The macOS front-end persists it as a pane-layout leaf path.
        fn abs_path(&self) -> String;
    }

    extern "Rust" {
        type FfiWinBuffer;

        // Whether core's `win_buffers` map has the window.
        fn reported(&self) -> bool;
        // The buffer's absolute path; empty when unnamed or not reported.
        fn abs_path(&self) -> String;
        // The session the window shows as a terminal pane; 0 for none or not reported.
        fn terminal(&self) -> u64;
    }

    extern "Rust" {
        type FfiWindowCursor;

        // Whether the window was found; every field reads 0 when not.
        fn found(&self) -> bool;
        // The 1-based cursor line.
        fn line(&self) -> u32;
        // The 0-based byte column of the cursor.
        fn col(&self) -> u32;
        // The 1-based first visible line.
        fn top_line(&self) -> u32;
    }

    extern "Rust" {
        type FfiCmdline;

        // Whether a command line is currently showing.
        fn active(&self) -> bool;
        // The first character of the command line (`:` / `/` / `?`), or "".
        fn firstc(&self) -> String;
        // The prompt string for an input prompt (`input()`), or "".
        fn prompt(&self) -> String;
        // The command-line text (the content chunks flattened; without firstc).
        fn text(&self) -> String;
        // The cursor position, as a BYTE offset into `text`.
        fn pos(&self) -> u32;
        // The indent (from a continued `:function` block); usually 0.
        fn indent(&self) -> u32;
        // The command-line recursion level (1 for a top-level command line).
        fn level(&self) -> u32;
    }

    extern "Rust" {
        type FfiPum;

        // Whether the completion popup is currently showing.
        fn visible(&self) -> bool;
        // The selected item index, or -1 for none.
        fn selected(&self) -> i64;
        // The anchor grid (-1 = anchored to the command line).
        fn anchor_grid(&self) -> i64;
        // The anchor row.
        fn anchor_row(&self) -> i64;
        // The anchor column (a BYTE offset into the cmdline text when
        // `anchor_grid == -1`).
        fn anchor_col(&self) -> i64;
        // The completion items. `FfiPumItem` is opaque so a `Vec` is supported.
        fn items(&self) -> Vec<FfiPumItem>;
    }

    extern "Rust" {
        type FfiPumItem;

        // The completion word (the primary label shown + inserted).
        fn word(&self) -> String;
        // The single-letter kind hint (`f`, `v`, ...), or "".
        fn kind(&self) -> String;
        // Extra menu text, or "".
        fn menu(&self) -> String;
        // Documentation / preview text, or "".
        fn info(&self) -> String;
    }

    extern "Rust" {
        type FfiToast;

        // Whether the toast is currently showing.
        fn visible(&self) -> bool;
        // The last message's kind (`emsg`, `echomsg`, `list_cmd`, ...), or "".
        fn kind(&self) -> String;
        // Every message's lines flattened in order. `FfiToastLine` is opaque so a
        // `Vec` is supported.
        fn lines(&self) -> Vec<FfiToastLine>;
    }

    extern "Rust" {
        type FfiToastLine;

        // The styled chunks of this line. `FfiToastChunk` is opaque so a `Vec` is
        // supported.
        fn chunks(&self) -> Vec<FfiToastChunk>;
    }

    extern "Rust" {
        type FfiToastChunk;

        // The chunk's text.
        fn text(&self) -> String;
        // The foreground color, RESOLVED in Rust against the highlight table.
        fn fg(&self) -> FfiColor;
        // The background color, RESOLVED in Rust against the highlight table.
        fn bg(&self) -> FfiColor;
        // The special (underline) color, RESOLVED in Rust.
        fn special(&self) -> FfiColor;
        // Packed style flags (bold/italic/underline/undercurl/strikethrough).
        fn flags(&self) -> u8;
    }
}

// --- Wake registration (Swift-free; see the crate docs) ---

/// The C ABI of the front-end wake callback. It carries the firing core's
/// wake token, so a single process-global callback serving every core can route
/// each wake to the tab that owns the core WITHOUT clobbering.
type WakeCallback = extern "C" fn(u64);

/// The next wake token to mint. Starts at 1 (0 is reserved as a "no core" /
/// unset sentinel on the Swift side); [`wolf_ffi_start`] `fetch_add`s one per
/// core so every live core has a distinct token.
static NEXT_WAKE_TOKEN: AtomicU64 = AtomicU64::new(1);

/// The globally-registered wake callback, or null when none is registered (the
/// Rust integration test path). Stored as a raw pointer in an [`AtomicPtr`].
static WAKE_FN: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Registers (or, with `None`, clears) the front-end wake callback.
///
/// Swift's `WakeBridge` calls this once at startup with a thunk that does
/// `DispatchQueue.main.async { … setNeedsDisplay … }`. Declared `extern "C"` with
/// a hand-written C header (`generated/wolf_ffi_wake.h`) rather than through the
/// swift-bridge module so there is no `extern "Swift"` symbol for a Rust-only
/// build to leave unresolved.
///
/// # Safety
///
/// `wake`, if `Some`, must be a valid `extern "C" fn(u64)` that is safe to call
/// from any thread (core fires the wake from its tokio worker thread).
#[no_mangle]
pub extern "C" fn wolf_ffi_set_wake(wake: Option<WakeCallback>) {
    let ptr = wake.map_or(std::ptr::null_mut(), |f| f as *mut ());
    WAKE_FN.store(ptr, Ordering::Release);
}

/// Builds the [`WakeFn`] core invokes on flush: it loads the registered callback
/// and calls it with this core's `token`, or does nothing if none is registered.
/// Captures only the `token` (a `u64`), so it is `Send + Sync` as `WakeFn`
/// requires.
fn registered_wake(token: u64) -> WakeFn {
    Arc::new(move || {
        let ptr = WAKE_FN.load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: `ptr` was set from a valid `extern "C" fn(u64)` by
            // `wolf_ffi_set_wake`; a non-null value is always such a pointer.
            let f: WakeCallback = unsafe { std::mem::transmute::<*mut (), WakeCallback>(ptr) };
            f(token);
        }
    })
}

/// The globally-registered file-search wake callback, or null when none is
/// registered (the Rust integration-test path). Stored as a raw pointer in an
/// [`AtomicPtr`], exactly like [`WAKE_FN`] but for the picker's traversal-batch
/// wakes.
static FILESEARCH_WAKE_FN: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Registers (or, with `None`, clears) the front-end's file-search wake callback.
///
/// Swift's `PickerWakeBridge` calls this once at startup with a thunk that does
/// `DispatchQueue.main.async { … picker.refresh() … }`. A second registered
/// `extern "C"` callback alongside [`wolf_ffi_set_wake`], declared in the
/// hand-written `generated/wolf_ffi_wake.h`, so a Rust-only build leaves no
/// `extern "Swift"` symbol unresolved.
///
/// # Safety
///
/// `wake`, if `Some`, must be a valid `extern "C" fn(u64)` that is safe to call
/// from any thread (core fires the wake from its tokio worker thread).
#[no_mangle]
pub extern "C" fn wolf_ffi_set_filesearch_wake(wake: Option<WakeCallback>) {
    let ptr = wake.map_or(std::ptr::null_mut(), |f| f as *mut ());
    FILESEARCH_WAKE_FN.store(ptr, Ordering::Release);
}

/// Builds the [`WakeFn`] core invokes on each file-search traversal batch: it
/// loads the registered file-search callback and calls it with this core's
/// `token`, or does nothing if none is registered. Captures only the `token` (a
/// `u64`), so it is `Send + Sync` as `WakeFn` requires.
fn registered_filesearch_wake(token: u64) -> WakeFn {
    Arc::new(move || {
        let ptr = FILESEARCH_WAKE_FN.load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: `ptr` was set from a valid `extern "C" fn(u64)` by `wolf_ffi_set_filesearch_wake`;
            // a non-null value is always such a pointer.
            let f: WakeCallback = unsafe { std::mem::transmute::<*mut (), WakeCallback>(ptr) };
            f(token);
        }
    })
}

/// The globally-registered file-TREE wake callback, or null when none is
/// registered (the Rust integration-test path). Stored as a raw pointer in an
/// [`AtomicPtr`], a third sibling of [`WAKE_FN`] / [`FILESEARCH_WAKE_FN`] feeding
/// the sidebar's own long-lived [`FileSearch`] traversal-batch wakes.
static TREE_WAKE_FN: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Registers (or, with `None`, clears) the front-end's file-TREE wake callback.
///
/// Swift's `TreeWakeBridge` calls this once at startup with a thunk that does
/// `DispatchQueue.main.async { … tree.refresh() … }`. A third registered
/// `extern "C"` callback alongside [`wolf_ffi_set_wake`] and
/// [`wolf_ffi_set_filesearch_wake`], declared in the hand-written
/// `generated/wolf_ffi_wake.h`, so a Rust-only build leaves no `extern "Swift"`
/// symbol unresolved.
///
/// # Safety
///
/// `wake`, if `Some`, must be a valid `extern "C" fn(u64)` that is safe to call
/// from any thread (core fires the wake from its tokio worker thread).
#[no_mangle]
pub extern "C" fn wolf_ffi_set_tree_wake(wake: Option<WakeCallback>) {
    let ptr = wake.map_or(std::ptr::null_mut(), |f| f as *mut ());
    TREE_WAKE_FN.store(ptr, Ordering::Release);
}

/// Builds the [`WakeFn`] core invokes on each file-tree traversal batch: it loads
/// the registered tree callback and calls it with this core's `token`, or does
/// nothing if none is registered. Captures only the `token` (a `u64`), so it is
/// `Send + Sync` as `WakeFn` requires.
fn registered_tree_wake(token: u64) -> WakeFn {
    Arc::new(move || {
        let ptr = TREE_WAKE_FN.load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: `ptr` was set from a valid `extern "C" fn(u64)` by
            // `wolf_ffi_set_tree_wake`; a non-null value is always such a pointer.
            let f: WakeCallback = unsafe { std::mem::transmute::<*mut (), WakeCallback>(ptr) };
            f(token);
        }
    })
}

/// The globally-registered content-SEARCH wake callback, or null when none is
/// registered (the Rust integration-test path). Stored as a raw pointer in an
/// [`AtomicPtr`], a fourth sibling of [`WAKE_FN`] / [`FILESEARCH_WAKE_FN`] /
/// [`TREE_WAKE_FN`] feeding the sidebar search panel's rg-streaming batch wakes.
static SEARCH_WAKE_FN: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Registers (or, with `None`, clears) the front-end's content-search wake
/// callback.
///
/// Swift's `SearchWakeBridge` calls this once at startup with a thunk that does
/// `DispatchQueue.main.async { … tabCore.onSearchWake() … }`. A fourth registered
/// `extern "C"` callback alongside [`wolf_ffi_set_wake`],
/// [`wolf_ffi_set_filesearch_wake`], and [`wolf_ffi_set_tree_wake`], declared in
/// the hand-written `generated/wolf_ffi_wake.h`, so a Rust-only build leaves no
/// `extern "Swift"` symbol unresolved.
///
/// # Safety
///
/// `wake`, if `Some`, must be a valid `extern "C" fn(u64)` that is safe to call
/// from any thread (core fires the wake from its tokio worker thread).
#[no_mangle]
pub extern "C" fn wolf_ffi_set_search_wake(wake: Option<WakeCallback>) {
    let ptr = wake.map_or(std::ptr::null_mut(), |f| f as *mut ());
    SEARCH_WAKE_FN.store(ptr, Ordering::Release);
}

/// Builds the [`WakeFn`] core invokes on each content-search batch: it loads the
/// registered search callback and calls it with this core's `token`, or does
/// nothing if none is registered. Captures only the `token` (a `u64`), so it is
/// `Send + Sync` as `WakeFn` requires.
fn registered_search_wake(token: u64) -> WakeFn {
    Arc::new(move || {
        let ptr = SEARCH_WAKE_FN.load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: `ptr` was set from a valid `extern "C" fn(u64)` by
            // `wolf_ffi_set_search_wake`; a non-null value is always such a pointer.
            let f: WakeCallback = unsafe { std::mem::transmute::<*mut (), WakeCallback>(ptr) };
            f(token);
        }
    })
}

// --- The open channel (`specs/cli-and-file-open.md` §2) ---

/// The globally-registered open-channel wake callback, or null when none is registered. A
/// fifth sibling of [`WAKE_FN`], called with token `0`, since the listener is process-wide.
static OPEN_WAKE_FN: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// The process-wide listener [`wolf_ffi_open_listen`] binds, `None` when nothing listens.
static OPEN_LISTENER: Mutex<Option<OpenListener>> = Mutex::new(None);

/// Locks [`OPEN_LISTENER`], surviving a panic that poisoned it.
fn open_listener() -> std::sync::MutexGuard<'static, Option<OpenListener>> {
    OPEN_LISTENER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Registers (or, with `None`, clears) the front-end's open-channel wake callback.
///
/// Swift's `OpenWakeBridge` calls this once at startup with a thunk that hops to the main
/// queue and drains the taken requests. Declared in the hand-written
/// `generated/wolf_ffi_wake.h`, like [`wolf_ffi_set_wake`].
///
/// # Safety
///
/// `wake`, if `Some`, must be a valid `extern "C" fn(u64)` that is safe to call from any
/// thread: the listener calls it from a connection's thread.
#[no_mangle]
pub extern "C" fn wolf_ffi_set_open_wake(wake: Option<WakeCallback>) {
    let ptr = wake.map_or(std::ptr::null_mut(), |f| f as *mut ());
    OPEN_WAKE_FN.store(ptr, Ordering::Release);
}

/// The [`WakeFn`] the listener calls after queueing each request: the registered open wake
/// with token `0`, or nothing when none is registered.
fn registered_open_wake() -> WakeFn {
    Arc::new(|| {
        let ptr = OPEN_WAKE_FN.load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: `ptr` was set from a valid `extern "C" fn(u64)` by
            // `wolf_ffi_set_open_wake`; a non-null value is always such a pointer.
            let f: WakeCallback = unsafe { std::mem::transmute::<*mut (), WakeCallback>(ptr) };
            f(0);
        }
    })
}

/// Binds the process-wide [`OpenListener`] in `state_dir`, or in [`wolf_core::state_dir`]
/// when `state_dir` is `""`, and returns `""`; on failure, the error's message, with nothing
/// listening. A listener already bound is dropped first, so rebinding its own directory works.
#[must_use]
pub fn wolf_ffi_open_listen(state_dir: String) -> String {
    let mut listener = open_listener();
    drop(listener.take());
    let dir = if state_dir.is_empty() {
        match wolf_core::state_dir() {
            Some(dir) => dir,
            None => return "cannot find Wolf's state directory".to_owned(),
        }
    } else {
        PathBuf::from(state_dir)
    };
    match OpenListener::bind(&dir, registered_open_wake()) {
        Ok(bound) => {
            *listener = Some(bound);
            String::new()
        }
        Err(e) => e.to_string(),
    }
}

/// Drops the listener, which removes its socket file. A no-op with none.
pub fn wolf_ffi_open_stop() {
    let stopped = open_listener().take();
    drop(stopped);
}

/// The requests the listener took since the last drain, in arrival order, each resolved
/// through [`OpenRequest::target`] and holding its wait. Empty with no listener.
///
/// Dropping an item that still holds a wait ends that wait with `1`, so a caller takes each
/// item out of the vec, rather than iterating it, before the vec drops.
#[must_use]
pub fn wolf_ffi_open_drain() -> Vec<FfiOpenItem> {
    let taken = open_listener()
        .as_ref()
        .map(OpenListener::drain_taken)
        .unwrap_or_default();
    taken
        .into_iter()
        .map(|taken| FfiOpenItem::of(&taken.request, taken.wait))
        .collect()
}

/// `path` resolved as a request with no tab ID would be, which is how
/// `specs/cli-and-file-open.md` §7 has Finder's opens handled; `""` is an activate.
#[must_use]
pub fn wolf_ffi_open_item(path: String) -> FfiOpenItem {
    let request = OpenRequest {
        path: (!path.is_empty()).then(|| PathBuf::from(path)),
        wait: false,
        tab_id: None,
    };
    FfiOpenItem::of(&request, None)
}

/// One open request, resolved (opaque to Swift).
pub struct FfiOpenItem {
    target: OpenTarget,
    tab_id: Option<TabId>,
    /// Taken by [`WolfCoreHandle::wait_on`]; ends with `1` when the item drops holding it.
    wait: RefCell<Option<OpenWait>>,
}

impl FfiOpenItem {
    fn of(request: &OpenRequest, wait: Option<OpenWait>) -> FfiOpenItem {
        FfiOpenItem {
            target: request.target(),
            tab_id: request.tab_id,
            wait: RefCell::new(wait),
        }
    }

    /// Whether the item still holds a wait, which [`WolfCoreHandle::wait_on`] takes.
    #[must_use]
    pub fn waits(&self) -> bool {
        self.wait.borrow().is_some()
    }

    /// The target's `open_kind` discriminant.
    #[must_use]
    pub fn kind(&self) -> u8 {
        match self.target {
            OpenTarget::Activate => open_kind::ACTIVATE,
            OpenTarget::Directory(_) => open_kind::DIRECTORY,
            OpenTarget::File(_) => open_kind::FILE,
        }
    }

    /// The resolved path, `""` for an activate request.
    #[must_use]
    pub fn path(&self) -> String {
        match &self.target {
            OpenTarget::Activate => String::new(),
            OpenTarget::Directory(path) | OpenTarget::File(path) => {
                path.to_string_lossy().into_owned()
            }
        }
    }

    /// The request's tab ID in its lowercase form, `""` when it carried none.
    #[must_use]
    pub fn tab_id(&self) -> String {
        self.tab_id.map(|id| id.to_string()).unwrap_or_default()
    }
}

/// Returns the ancestor directory rel-paths of `rel_path`, from the cwd root down
/// to the node's parent (`"src/main.rs"` -> `["src"]`, `"README.md"` -> `[]`).
///
/// A free FFI fn (no live session required) delegating to
/// [`wolf_core::tree::ancestors_to_reveal`]; the sidebar calls it to learn which
/// ancestor dirs to expand before revealing the current file.
#[must_use]
pub fn wolf_ffi_ancestors_to_reveal(rel_path: String) -> Vec<String> {
    wolf_core::tree::ancestors_to_reveal(&rel_path)
}

// --- FfiConfig: the resolved user configuration (opaque to Swift) ---

/// The resolved user configuration (opaque to Swift; String-in-struct is not
/// supported for transparent bridge structs, so — like [`FfiFileEntry`] — this is
/// opaque with scalar/String accessors).
///
/// Built by [`wolf_ffi_load_config`] from [`wolf_core::WolfConfig`].
pub struct FfiConfig {
    font_family: String,
    font_size: f64,
    line_height: f64,
    appearance: u8,
    macos_option_as_alt: bool,
}

impl FfiConfig {
    /// The resolved font family name (e.g. `"iA Writer Mono S"`).
    #[must_use]
    pub fn font_family(&self) -> String {
        self.font_family.clone()
    }

    /// The resolved font size, in points.
    #[must_use]
    pub fn font_size(&self) -> f64 {
        self.font_size
    }

    /// The resolved line-height, as a percentage of the font size (`130.0` == 130%).
    #[must_use]
    pub fn line_height(&self) -> f64 {
        self.line_height
    }

    /// The resolved appearance mode as a discriminant (`appearance::AUTO` /
    /// `DARK` / `LIGHT`). Swift maps it via the `AppearanceMode` raw-value enum.
    #[must_use]
    pub fn appearance(&self) -> u8 {
        self.appearance
    }

    /// On macOS, whether the Option key acts as Alt (`<M-…>` mappings) rather
    /// than composing accented characters. Defaults to `false` (option-as-option).
    #[must_use]
    pub fn macos_option_as_alt(&self) -> bool {
        self.macos_option_as_alt
    }
}

/// Loads the resolved user configuration, reading — or creating with defaults —
/// `wolf-config.yaml` in the platform preferences directory.
///
/// A free FFI fn (no live core required) delegating to [`wolf_core::config`]:
/// resolves [`preferences_dir`](wolf_core::preferences_dir) then
/// [`load_or_create`](wolf_core::load_or_create), falling back to
/// [`default_config`](wolf_core::default_config) when no preferences directory can
/// be resolved. Infallible across the bridge.
#[must_use]
pub fn wolf_ffi_load_config() -> FfiConfig {
    let config = wolf_core::preferences_dir()
        .map(|dir| wolf_core::load_or_create(&dir))
        .unwrap_or_else(wolf_core::default_config);
    let appearance = match config.appearance {
        Appearance::Auto => appearance::AUTO,
        Appearance::Dark => appearance::DARK,
        Appearance::Light => appearance::LIGHT,
    };
    FfiConfig {
        font_family: config.font_family,
        font_size: config.font_size,
        line_height: config.line_height,
        appearance,
        macos_option_as_alt: config.macos_option_as_alt,
    }
}

/// Returns [`wolf_core::FONT_LICENSE_NOTICES`] verbatim: the bundled fonts' licence
/// notices, which the macOS About panel shows as its credits.
///
/// A free FFI fn (no live core required).
#[must_use]
pub fn wolf_ffi_font_license_notices() -> String {
    wolf_core::FONT_LICENSE_NOTICES.to_owned()
}

/// Returns [`wolf_core::LIBRARY_LICENSE_NOTICES`] verbatim: the linked libraries'
/// licence notices, which the macOS About panel shows after the font notices.
///
/// A free FFI fn (no live core required).
#[must_use]
pub fn wolf_ffi_library_license_notices() -> String {
    wolf_core::LIBRARY_LICENSE_NOTICES.to_owned()
}

// --- The application-state surface: builder, submit and readers over `AppStateStore` ---

/// The process-wide application-state store, opened once over
/// [`wolf_core::state_dir`]: its unchanged-tree check is per store, so every
/// window shares one. `None` when no state dir resolved.
///
/// [`WolfCoreHandle::shutdown`] does not flush it: the final write is a front-end
/// step taken before any Neovim exits, through [`wolf_ffi_app_state_flush`].
static SHARED_APP_STATE: OnceLock<Option<AppStateStore>> = OnceLock::new();

/// Returns a clone of the process-wide [`AppStateStore`], opening it on first use.
fn shared_app_state() -> Option<AppStateStore> {
    SHARED_APP_STATE
        .get_or_init(|| wolf_core::state_dir().map(|dir| AppStateStore::open(&dir)))
        .clone()
}

/// The app-state builder's scratch: the window state, each window's recency, and
/// the `(node, weight)` stack one tab's tree is replayed onto in post order.
#[derive(Default)]
struct AppScratch {
    state: app::WindowState,
    recency: Vec<Vec<usize>>,
    stack: Vec<(app::Node, f64)>,
}

/// The process-wide app-state scratch. Driven from the AppKit main thread only, but
/// the lock keeps the free fns sound regardless.
static APP_SCRATCH: OnceLock<Mutex<AppScratch>> = OnceLock::new();

/// Locks the app-state scratch, initializing it empty on first use.
fn app_scratch() -> std::sync::MutexGuard<'static, AppScratch> {
    APP_SCRATCH
        .get_or_init(|| Mutex::new(AppScratch::default()))
        .lock()
        .expect("app-state scratch mutex poisoned")
}

/// An edit pane on no file, with nothing to restore.
fn empty_edit_pane() -> app::Pane {
    app::Pane::Edit {
        path: String::new(),
        cursor: None,
        top_line: None,
    }
}

/// Maps a `pane_ref` discriminant and its index to a reference; an unknown kind
/// maps to `None`.
fn pane_ref_from_ffi(kind: u8, index: u32) -> Option<app::PaneRef> {
    match kind {
        pane_ref::PANE => Some(app::PaneRef::Pane(index as usize)),
        pane_ref::DRAWER => Some(app::PaneRef::Drawer),
        _ => None,
    }
}

/// The `pane_ref` discriminant and index of `r`.
fn pane_ref_to_ffi(r: Option<app::PaneRef>) -> (u8, u32) {
    match r {
        None => (pane_ref::NONE, 0),
        Some(app::PaneRef::Pane(i)) => (pane_ref::PANE, u32::try_from(i).unwrap_or(u32::MAX)),
        Some(app::PaneRef::Drawer) => (pane_ref::DRAWER, 0),
    }
}

/// The `tristate` discriminant of `value`.
fn tristate_of(value: Option<bool>) -> u8 {
    match value {
        None => tristate::UNSET,
        Some(false) => tristate::OFF,
        Some(true) => tristate::ON,
    }
}

/// Clears the app-state scratch, starting a fresh window state.
///
/// The front-end then calls [`wolf_ffi_app_state_add_window`] per window,
/// frontmost first, each followed by its tabs, their trees and the window's
/// recency, then [`wolf_ffi_app_state_commit`].
pub fn wolf_ffi_app_state_begin() {
    *app_scratch() = AppScratch::default();
}

/// Pushes a window with an empty tab list and an empty recency list. `x` and `y`
/// are stored as `None` when `has_position` is false, which is written as `null`.
pub fn wolf_ffi_app_state_add_window(
    has_position: bool,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    active_tab: u32,
) {
    let mut scratch = app_scratch();
    scratch.state.windows.push(app::Window {
        frame: app::Frame {
            x: has_position.then_some(x),
            y: has_position.then_some(y),
            width,
            height,
        },
        active_tab: active_tab as usize,
        tabs: Vec::new(),
        minimized: false,
    });
    scratch.recency.push(Vec::new());
}

/// Sets the last window's `minimized`. A no-op with no window.
pub fn wolf_ffi_app_state_window_minimized(minimized: bool) {
    if let Some(window) = app_scratch().state.windows.last_mut() {
        window.minimized = minimized;
    }
}

/// Pushes a tab with no root onto the last window, its `focused` and `zoomed`
/// each a `pane_ref` kind plus an index. Sidebar mode, query, toggles and
/// drawer start as nothing to restore. A no-op with no window.
pub fn wolf_ffi_app_state_add_tab(
    directory: String,
    sidebar_visible: bool,
    focused_kind: u8,
    focused_index: u32,
    zoomed_kind: u8,
    zoomed_index: u32,
) {
    if let Some(window) = app_scratch().state.windows.last_mut() {
        window.tabs.push(app::Tab {
            directory,
            content: app::TabContent {
                root: None,
                focused: pane_ref_from_ffi(focused_kind, focused_index),
                zoomed: pane_ref_from_ffi(zoomed_kind, zoomed_index),
                sidebar: app::Sidebar {
                    visible: sidebar_visible,
                    ..app::Sidebar::default()
                },
                drawer: None,
            },
        });
    }
}

/// The last window's last tab in `scratch`, if any.
fn last_tab(scratch: &mut AppScratch) -> Option<&mut app::TabContent> {
    scratch
        .state
        .windows
        .last_mut()
        .and_then(|w| w.tabs.last_mut())
        .map(|t| &mut t.content)
}

/// Sets the last tab's sidebar mode (a `sidebar_mode` discriminant, where
/// `UNSET` or an unknown value maps to `None`), query and three toggles. A no-op
/// with no tab.
pub fn wolf_ffi_app_state_tab_sidebar(
    mode: u8,
    query: String,
    case_sensitive: bool,
    use_regex: bool,
    all_files: bool,
) {
    let mut scratch = app_scratch();
    if let Some(tab) = last_tab(&mut scratch) {
        tab.sidebar.mode = match mode {
            sidebar_mode::TREE => Some(app::SidebarMode::Tree),
            sidebar_mode::SEARCH => Some(app::SidebarMode::Search),
            _ => None,
        };
        tab.sidebar.query = Some(query);
        tab.sidebar.case_sensitive = Some(case_sensitive);
        tab.sidebar.use_regex = Some(use_regex);
        tab.sidebar.all_files = Some(all_files);
    }
}

/// Sets the last tab's drawer. A no-op with no tab.
pub fn wolf_ffi_app_state_tab_drawer(open: bool, share: f64) {
    let mut scratch = app_scratch();
    if let Some(tab) = last_tab(&mut scratch) {
        tab.drawer = Some(app::Drawer { open, share });
    }
}

/// Pushes a pane of `pane_kind` `kind` on `path`, weighing `weight` in its
/// parent, onto the node stack. An unknown kind pushes an empty edit pane, so a
/// later [`wolf_ffi_app_state_push_layout`] count still matches.
pub fn wolf_ffi_app_state_push_pane(kind: u8, path: String, weight: f64) {
    let pane = match kind {
        pane_kind::EDIT => app::Pane::Edit {
            path,
            cursor: None,
            top_line: None,
        },
        pane_kind::PREVIEW => app::Pane::Preview { path, scroll: None },
        pane_kind::TERMINAL => app::Pane::Terminal { path },
        _ => empty_edit_pane(),
    };
    app_scratch().stack.push((app::Node::Pane(pane), weight));
}

/// Sets the cursor (`line` 1-based, `col` a 0-based byte column) and the 1-based
/// `top_line` of the node on top of the stack, when it is an edit pane.
pub fn wolf_ffi_app_state_pane_cursor(line: u32, col: u32, top_line: u32) {
    if let Some((
        app::Node::Pane(app::Pane::Edit {
            cursor,
            top_line: top,
            ..
        }),
        _,
    )) = app_scratch().stack.last_mut()
    {
        *cursor = Some(app::Cursor { line, col });
        *top = Some(top_line);
    }
}

/// Sets the scroll of the node on top of the stack, when it is a preview pane.
pub fn wolf_ffi_app_state_pane_scroll(scroll: f64) {
    if let Some((app::Node::Pane(app::Pane::Preview { scroll: s, .. }), _)) =
        app_scratch().stack.last_mut()
    {
        *s = Some(scroll);
    }
}

/// Pops the top `child_count` nodes, in push order, into a layout along
/// `orientation` (an `orientation` discriminant) and pushes it back weighing
/// `weight`. A no-op when the stack holds fewer nodes.
pub fn wolf_ffi_app_state_push_layout(orientation: u8, child_count: u32, weight: f64) {
    let mut scratch = app_scratch();
    let count = child_count as usize;
    if scratch.stack.len() < count {
        tracing::warn!(
            "wolf-ffi: app-state push_layout wants {count} children but only {} on the stack",
            scratch.stack.len()
        );
        return;
    }
    let split_at = scratch.stack.len() - count;
    let popped = scratch.stack.split_off(split_at);
    let orientation = match orientation {
        orientation::VERTICAL => app::PaneOrientation::Vertical,
        _ => app::PaneOrientation::Horizontal,
    };
    let weights = popped.iter().map(|(_, w)| *w).collect();
    let children = popped.into_iter().map(|(node, _)| node).collect();
    scratch.stack.push((
        app::Node::Layout(app::Layout {
            orientation,
            weights,
            children,
        }),
        weight,
    ));
}

/// Moves the one remaining node to the last window's last tab's `root`, then
/// clears the stack. A no-op, still clearing it, unless exactly one node remains
/// and a tab exists.
pub fn wolf_ffi_app_state_commit_root() {
    let mut scratch = app_scratch();
    let stack = std::mem::take(&mut scratch.stack);
    let [(node, _)]: [(app::Node, f64); 1] = match stack.try_into() {
        Ok(one) => one,
        Err(stack) => {
            tracing::warn!(
                "wolf-ffi: app-state commit_root expected 1 root node, found {}",
                stack.len()
            );
            return;
        }
    };
    if let Some(tab) = last_tab(&mut scratch) {
        tab.root = Some(node);
    }
}

/// Appends tab index `tab` to the last window's recency, which lists the most
/// recently selected tab first. A no-op with no window.
pub fn wolf_ffi_app_state_push_recency(tab: u32) {
    if let Some(recency) = app_scratch().recency.last_mut() {
        recency.push(tab as usize);
    }
}

/// Submits the scratch window state and its recency to the shared app-state
/// store, where an unchanged tree writes nothing. A no-op with no state dir.
pub fn wolf_ffi_app_state_commit() {
    let (state, recency) = {
        let scratch = app_scratch();
        (scratch.state.clone(), scratch.recency.clone())
    };
    if let Some(store) = shared_app_state() {
        store.submit(state, recency);
    }
}

/// Blocks until the pending app-state write has been attempted, bounded by
/// `STATE_FLUSH_TIMEOUT`. Returns whether it was in time; `true` with no store.
#[must_use]
pub fn wolf_ffi_app_state_flush() -> bool {
    shared_app_state().is_none_or(|store| store.flush(STATE_FLUSH_TIMEOUT))
}

/// The saved window state, for launch, through the shared store. Empty when there
/// is no store, nothing saved, or a `window_state` table from a newer build.
#[must_use]
pub fn wolf_ffi_app_state_window_state() -> FfiAppState {
    let state = shared_app_state()
        .and_then(|store| store.window_state())
        .unwrap_or_default();
    FfiAppState {
        windows: state
            .windows
            .into_iter()
            .map(|window| FfiAppWindow { window })
            .collect(),
    }
}

/// `directory`'s project state, for Open Folder, through
/// [`AppStateStore::project_state`], so a found row is marked used.
#[must_use]
pub fn wolf_ffi_app_state_project_state(directory: String) -> FfiProjectState {
    let state = shared_app_state().and_then(|store| store.project_state(&directory));
    FfiProjectState { directory, state }
}

/// The tab Open Folder focuses for `directory`: [`app::project_tab`] over the
/// scratch's window state and recency, as fed by the builder minus the commit.
///
/// Reads the scratch only and never opens the store, so a caller outside the
/// store's lifetime (a seam test) can ask it.
#[must_use]
pub fn wolf_ffi_app_state_project_tab(directory: String) -> FfiProjectTab {
    let scratch = app_scratch();
    FfiProjectTab {
        found: app::project_tab(&scratch.state, &scratch.recency, &directory),
    }
}

/// [`wolf_ffi_app_state_project_tab`]'s answer (opaque to Swift): a window index,
/// frontmost first, and a tab index in that window, or nothing.
pub struct FfiProjectTab {
    found: Option<(usize, usize)>,
}

impl FfiProjectTab {
    /// Whether a tab is open on the directory.
    #[must_use]
    pub fn found(&self) -> bool {
        self.found.is_some()
    }

    /// The window's index, 0 when not found.
    #[must_use]
    pub fn window(&self) -> u32 {
        self.found
            .map_or(0, |(w, _)| u32::try_from(w).unwrap_or(u32::MAX))
    }

    /// The tab's index in that window, 0 when not found.
    #[must_use]
    pub fn tab(&self) -> u32 {
        self.found
            .map_or(0, |(_, t)| u32::try_from(t).unwrap_or(u32::MAX))
    }
}

/// Where `file` goes in the scratch's last window's last tab, by
/// [`wolf_core::choose_pane`]: the front-end feeds the one tab it asks about. With no tab, a
/// default tab answers [`PanePlacement::SplitLeft`].
///
/// Reads the scratch only, like [`wolf_ffi_app_state_project_tab`].
#[must_use]
pub fn wolf_ffi_app_state_choose_pane(file: String) -> FfiPaneChoice {
    let scratch = app_scratch();
    let tab = scratch
        .state
        .windows
        .last()
        .and_then(|window| window.tabs.last())
        .map(|tab| tab.content.clone())
        .unwrap_or_default();
    FfiPaneChoice {
        choice: choose_pane(&tab, std::path::Path::new(&file)),
    }
}

/// `file`'s project directory, by [`wolf_core::project_directory`] over the non-empty
/// directories of every scratch tab, which the front-end feeds as the live tabs' directories.
///
/// Reads the scratch only, like [`wolf_ffi_app_state_project_tab`].
#[must_use]
pub fn wolf_ffi_app_state_project_directory(file: String) -> String {
    let open: Vec<PathBuf> = app_scratch()
        .state
        .windows
        .iter()
        .flat_map(|window| &window.tabs)
        .filter(|tab| !tab.directory.is_empty())
        .map(|tab| PathBuf::from(&tab.directory))
        .collect();
    project_directory(std::path::Path::new(&file), &open)
        .to_string_lossy()
        .into_owned()
}

/// [`wolf_ffi_app_state_choose_pane`]'s answer (opaque to Swift).
pub struct FfiPaneChoice {
    choice: PaneChoice,
}

impl FfiPaneChoice {
    /// Whether the tab's zoom, a pane's or the drawer's, is undone first.
    #[must_use]
    pub fn unzoom(&self) -> bool {
        self.choice.unzoom
    }

    /// The placement's `pane_placement` discriminant.
    #[must_use]
    pub fn placement(&self) -> u8 {
        match self.choice.placement {
            PanePlacement::Focus(_) => pane_placement::FOCUS,
            PanePlacement::Open(_) => pane_placement::OPEN,
            PanePlacement::Convert(_) => pane_placement::CONVERT,
            PanePlacement::SplitLeft => pane_placement::SPLIT_LEFT,
        }
    }

    /// The depth-first pane index the placement names, `0` for a split.
    #[must_use]
    pub fn pane(&self) -> u32 {
        match self.choice.placement {
            PanePlacement::Focus(i) | PanePlacement::Open(i) | PanePlacement::Convert(i) => {
                u32::try_from(i).unwrap_or(u32::MAX)
            }
            PanePlacement::SplitLeft => 0,
        }
    }
}

/// The saved window state (opaque to Swift), vending its windows.
pub struct FfiAppState {
    windows: Vec<FfiAppWindow>,
}

impl FfiAppState {
    /// The saved windows, frontmost first.
    #[must_use]
    pub fn windows(&self) -> Vec<FfiAppWindow> {
        self.windows.clone()
    }
}

/// One saved window (opaque to Swift, so a `Vec` of it is supported).
#[derive(Clone)]
pub struct FfiAppWindow {
    window: app::Window,
}

impl FfiAppWindow {
    /// Whether the frame carries a position.
    #[must_use]
    pub fn has_position(&self) -> bool {
        self.window.frame.x.is_some() && self.window.frame.y.is_some()
    }

    /// The origin's x coordinate, 0 without a position.
    #[must_use]
    pub fn x(&self) -> f64 {
        self.window.frame.x.unwrap_or(0.0)
    }

    /// The origin's y coordinate, 0 without a position.
    #[must_use]
    pub fn y(&self) -> f64 {
        self.window.frame.y.unwrap_or(0.0)
    }

    /// The width.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.window.frame.width
    }

    /// The height.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.window.frame.height
    }

    /// The index of the selected tab.
    #[must_use]
    pub fn active_tab(&self) -> u32 {
        u32::try_from(self.window.active_tab).unwrap_or(u32::MAX)
    }

    /// Whether the window was minimized.
    #[must_use]
    pub fn minimized(&self) -> bool {
        self.window.minimized
    }

    /// The window's tabs, in tab-bar order.
    #[must_use]
    pub fn tabs(&self) -> Vec<FfiAppTab> {
        self.window
            .tabs
            .iter()
            .map(|tab| FfiAppTab {
                directory: tab.directory.clone(),
                content: tab.content.clone(),
            })
            .collect()
    }
}

/// One saved tab (opaque to Swift). A field with nothing to restore reads
/// `UNSET`, `NONE`, `false`, 0 or `""`.
#[derive(Clone)]
pub struct FfiAppTab {
    directory: String,
    content: app::TabContent,
}

impl FfiAppTab {
    /// The tab's directory, `""` for an empty tab.
    #[must_use]
    pub fn directory(&self) -> String {
        self.directory.clone()
    }

    /// Whether the tab has a root.
    #[must_use]
    pub fn has_root(&self) -> bool {
        self.content.root.is_some()
    }

    /// The root node, or an empty edit pane when there is none.
    #[must_use]
    pub fn root(&self) -> FfiAppNode {
        FfiAppNode {
            node: self
                .content
                .root
                .clone()
                .unwrap_or_else(|| app::Node::Pane(empty_edit_pane())),
        }
    }

    /// The `pane_ref` discriminant of `focused`.
    #[must_use]
    pub fn focused_kind(&self) -> u8 {
        pane_ref_to_ffi(self.content.focused).0
    }

    /// The pane index of `focused`, 0 unless it is a pane.
    #[must_use]
    pub fn focused_index(&self) -> u32 {
        pane_ref_to_ffi(self.content.focused).1
    }

    /// The `pane_ref` discriminant of `zoomed`.
    #[must_use]
    pub fn zoomed_kind(&self) -> u8 {
        pane_ref_to_ffi(self.content.zoomed).0
    }

    /// The pane index of `zoomed`, 0 unless it is a pane.
    #[must_use]
    pub fn zoomed_index(&self) -> u32 {
        pane_ref_to_ffi(self.content.zoomed).1
    }

    /// Whether the sidebar is visible.
    #[must_use]
    pub fn sidebar_visible(&self) -> bool {
        self.content.sidebar.visible
    }

    /// The `sidebar_mode` discriminant of the sidebar's mode.
    #[must_use]
    pub fn sidebar_mode(&self) -> u8 {
        match self.content.sidebar.mode {
            None => sidebar_mode::UNSET,
            Some(app::SidebarMode::Tree) => sidebar_mode::TREE,
            Some(app::SidebarMode::Search) => sidebar_mode::SEARCH,
        }
    }

    /// Whether the search query was saved.
    #[must_use]
    pub fn has_query(&self) -> bool {
        self.content.sidebar.query.is_some()
    }

    /// The search query, `""` when none was saved.
    #[must_use]
    pub fn query(&self) -> String {
        self.content.sidebar.query.clone().unwrap_or_default()
    }

    /// The `tristate` of the case-sensitivity toggle.
    #[must_use]
    pub fn case_sensitive(&self) -> u8 {
        tristate_of(self.content.sidebar.case_sensitive)
    }

    /// The `tristate` of the regex toggle.
    #[must_use]
    pub fn use_regex(&self) -> u8 {
        tristate_of(self.content.sidebar.use_regex)
    }

    /// The `tristate` of the all-files toggle.
    #[must_use]
    pub fn all_files(&self) -> u8 {
        tristate_of(self.content.sidebar.all_files)
    }

    /// Whether the drawer was saved.
    #[must_use]
    pub fn has_drawer(&self) -> bool {
        self.content.drawer.is_some()
    }

    /// Whether the drawer is open, `false` when none was saved.
    #[must_use]
    pub fn drawer_open(&self) -> bool {
        self.content.drawer.is_some_and(|d| d.open)
    }

    /// The drawer's share of the pane area's height, 0 when none was saved.
    #[must_use]
    pub fn drawer_share(&self) -> f64 {
        self.content.drawer.map_or(0.0, |d| d.share)
    }

    /// The tab as [`app::restored_tab`] restores it, with the same directory: a
    /// missing file becomes an empty pane in place, and a zoom on it is dropped.
    #[must_use]
    pub fn restored(&self) -> FfiAppTab {
        FfiAppTab {
            directory: self.directory.clone(),
            content: app::restored_tab(&self.content),
        }
    }
}

/// One node of a saved tab's tree (opaque to Swift), read recursively through
/// [`FfiAppNode::child`]. A field that does not apply reads 0, `""` or `false`.
pub struct FfiAppNode {
    node: app::Node,
}

impl FfiAppNode {
    /// The pane, when this node is one.
    fn pane(&self) -> Option<&app::Pane> {
        match &self.node {
            app::Node::Pane(pane) => Some(pane),
            app::Node::Layout(_) => None,
        }
    }

    /// The layout, when this node is one.
    fn layout(&self) -> Option<&app::Layout> {
        match &self.node {
            app::Node::Layout(layout) => Some(layout),
            app::Node::Pane(_) => None,
        }
    }

    /// The edit pane's cursor and top line, when this node is an edit pane.
    fn edit(&self) -> (Option<app::Cursor>, Option<u32>) {
        match self.pane() {
            Some(app::Pane::Edit {
                cursor, top_line, ..
            }) => (*cursor, *top_line),
            _ => (None, None),
        }
    }

    /// The preview pane's scroll, when this node is a preview pane.
    fn preview_scroll(&self) -> Option<f64> {
        match self.pane() {
            Some(app::Pane::Preview { scroll, .. }) => *scroll,
            _ => None,
        }
    }

    /// Whether this node is a pane (vs. a layout).
    #[must_use]
    pub fn is_pane(&self) -> bool {
        self.pane().is_some()
    }

    /// The pane's `pane_kind` discriminant, `EDIT` for a layout.
    #[must_use]
    pub fn kind(&self) -> u8 {
        match self.pane() {
            Some(app::Pane::Preview { .. }) => pane_kind::PREVIEW,
            Some(app::Pane::Terminal { .. }) => pane_kind::TERMINAL,
            Some(app::Pane::Edit { .. }) | None => pane_kind::EDIT,
        }
    }

    /// The pane's path, `""` for a layout.
    #[must_use]
    pub fn path(&self) -> String {
        self.pane().map(|p| p.path().to_owned()).unwrap_or_default()
    }

    /// Whether the edit pane's cursor was saved.
    #[must_use]
    pub fn has_cursor(&self) -> bool {
        self.edit().0.is_some()
    }

    /// The cursor's 1-based line.
    #[must_use]
    pub fn cursor_line(&self) -> u32 {
        self.edit().0.map_or(0, |c| c.line)
    }

    /// The cursor's 0-based byte column.
    #[must_use]
    pub fn cursor_col(&self) -> u32 {
        self.edit().0.map_or(0, |c| c.col)
    }

    /// Whether the edit pane's top line was saved.
    #[must_use]
    pub fn has_top_line(&self) -> bool {
        self.edit().1.is_some()
    }

    /// The 1-based first visible line.
    #[must_use]
    pub fn top_line(&self) -> u32 {
        self.edit().1.unwrap_or(0)
    }

    /// Whether the preview pane's scroll was saved.
    #[must_use]
    pub fn has_scroll(&self) -> bool {
        self.preview_scroll().is_some()
    }

    /// The preview pane's scroll fraction.
    #[must_use]
    pub fn scroll(&self) -> f64 {
        self.preview_scroll().unwrap_or(0.0)
    }

    /// The layout's `orientation` discriminant, 0 for a pane.
    #[must_use]
    pub fn orientation(&self) -> u8 {
        match self.layout().map(|l| l.orientation) {
            Some(app::PaneOrientation::Vertical) => orientation::VERTICAL,
            _ => orientation::HORIZONTAL,
        }
    }

    /// The layout's child count, 0 for a pane.
    #[must_use]
    pub fn child_count(&self) -> u32 {
        self.layout()
            .map_or(0, |l| u32::try_from(l.children.len()).unwrap_or(u32::MAX))
    }

    /// The weight of child `index`, 0.0 out of range.
    #[must_use]
    pub fn weight(&self, index: u32) -> f64 {
        self.layout()
            .and_then(|l| l.weights.get(index as usize).copied())
            .unwrap_or(0.0)
    }

    /// Child `index`, an empty edit pane out of range.
    #[must_use]
    pub fn child(&self, index: u32) -> FfiAppNode {
        FfiAppNode {
            node: self
                .layout()
                .and_then(|l| l.children.get(index as usize).cloned())
                .unwrap_or_else(|| app::Node::Pane(empty_edit_pane())),
        }
    }
}

/// One directory's project state (opaque to Swift). When [`FfiProjectState::found`]
/// is false every other field reads 0 and the tab is empty.
pub struct FfiProjectState {
    directory: String,
    state: Option<app::ProjectState>,
}

impl FfiProjectState {
    /// Whether the directory has a project state.
    #[must_use]
    pub fn found(&self) -> bool {
        self.state.is_some()
    }

    /// The stored frame, if any.
    fn frame(&self) -> Option<app::Frame> {
        self.state.as_ref().map(|s| s.frame)
    }

    /// Whether the stored frame carries a position.
    #[must_use]
    pub fn has_position(&self) -> bool {
        self.frame().is_some_and(|f| f.x.is_some() && f.y.is_some())
    }

    /// The stored origin's x coordinate, 0 without a position.
    #[must_use]
    pub fn x(&self) -> f64 {
        self.frame().and_then(|f| f.x).unwrap_or(0.0)
    }

    /// The stored origin's y coordinate, 0 without a position.
    #[must_use]
    pub fn y(&self) -> f64 {
        self.frame().and_then(|f| f.y).unwrap_or(0.0)
    }

    /// The stored width.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.frame().map_or(0.0, |f| f.width)
    }

    /// The stored height.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.frame().map_or(0.0, |f| f.height)
    }

    /// The stored tab, whose directory is the one asked for.
    #[must_use]
    pub fn tab(&self) -> FfiAppTab {
        FfiAppTab {
            directory: self.directory.clone(),
            content: self
                .state
                .as_ref()
                .map(|s| s.tab.clone())
                .unwrap_or_default(),
        }
    }
}

// --- Opaque handles ---

/// An opaque, main-thread-only handle to a live [`WolfCore`].
///
/// [`WolfCore`] is not `Send`; this handle and every method on it are used only on
/// the AppKit main thread. The interior `RefCell<Option<…>>` lets
/// [`shutdown`](WolfCoreHandle::shutdown) take the core out by value (it consumes
/// `self`) while the bridge only ever hands Swift a `&self`.
pub struct WolfCoreHandle {
    core: RefCell<Option<WolfCore>>,
    /// The per-core wake-routing identity token minted at [`wolf_ffi_start`],
    /// captured by the core's [`WakeFn`] and carried to the front-end callback.
    wake_token: u64,
}

impl WolfCoreHandle {
    /// The per-core wake-routing identity token (minted fresh per
    /// [`wolf_ffi_start`]). The front-end registers ONE process-global wake
    /// callback per channel and looks this token up to route each wake to the
    /// right window.
    #[must_use]
    pub fn wake_token(&self) -> u64 {
        self.wake_token
    }

    /// Reads the current render-state snapshot into an owned [`FfiFrame`].
    #[must_use]
    pub fn read_frame(&self) -> FfiFrame {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiFrame::empty();
        };
        let snapshot = core.read_snapshot();
        FfiFrame::from_snapshot(&snapshot)
    }

    /// Maps `ev` into a [`wolf_core::KeyEvent`] and forwards it to Neovim.
    pub fn send_key(&self, ev: FfiKeyEvent) {
        if let Some(key) = key_event_from_ffi(&ev) {
            if let Some(core) = self.core.borrow().as_ref() {
                core.send_input(InputEvent::Key(key));
            }
        }
    }

    /// Maps `ev` into a [`wolf_core::MouseEvent`] and forwards it to Neovim
    /// (`nvim_input_mouse` targeting `ev.grid` with grid-local row/col). A no-op on
    /// an unrecognized button/action discriminant or once the core is shut down.
    pub fn send_mouse(&self, ev: FfiMouseEvent) {
        if let Some(mouse) = mouse_event_from_ffi(&ev) {
            if let Some(core) = self.core.borrow().as_ref() {
                core.send_mouse(mouse);
            }
        }
    }

    /// Maps `ev` into a [`wolf_core::ScrollEvent`] and forwards it to Neovim
    /// (`nvim_input_mouse("wheel", …)`). A no-op on an unrecognized direction
    /// discriminant or once the core is shut down.
    pub fn send_scroll(&self, ev: FfiScrollEvent) {
        if let Some(scroll) = scroll_event_from_ffi(&ev) {
            if let Some(core) = self.core.borrow().as_ref() {
                core.send_scroll(scroll);
            }
        }
    }

    /// Reads the per-grid render-state snapshot for `grid` into an owned
    /// [`FfiFrame`].
    ///
    /// The pane container reads one frame per pane (its bound window grid) plus
    /// grid 1 for the cmdline strip. An absent grid (e.g. a closed window) yields
    /// an empty `0x0` frame, which the front-end treats as "grid gone".
    #[must_use]
    pub fn read_frame_for_grid(&self, grid: u64) -> FfiFrame {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiFrame::empty();
        };
        let snapshot = core.read_snapshot();
        FfiFrame::from_snapshot_grid(&snapshot, grid)
    }

    /// Requests a grid resize.
    pub fn resize(&self, cols: u16, rows: u16) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.resize(cols, rows);
        }
    }

    /// Requests a per-grid resize of a single window `grid` (in cells). A no-op
    /// once the core has been shut down.
    pub fn resize_grid(&self, grid: u64, cols: u16, rows: u16) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.resize_grid(grid, cols, rows);
        }
    }

    /// Drains and returns all pending layout intents (splits + focus-direction
    /// moves) in arrival order, each mapped into an opaque [`FfiLayoutIntent`].
    /// Returns an empty `Vec` when the core is gone.
    #[must_use]
    pub fn drain_intents(&self) -> Vec<FfiLayoutIntent> {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return Vec::new();
        };
        core.drain_intents()
            .into_iter()
            .map(FfiLayoutIntent::from_intent)
            .collect()
    }

    /// Drains and returns all pending user-triggered commands in arrival order,
    /// each mapped into an opaque [`FfiCommand`]. Returns an empty `Vec` when the
    /// core is gone.
    #[must_use]
    pub fn drain_commands(&self) -> Vec<FfiCommand> {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return Vec::new();
        };
        core.drain_commands()
            .into_iter()
            .map(FfiCommand::from_command)
            .collect()
    }

    /// Returns ALL windows nvim currently reports (the Swift side filters live
    /// editor windows, mirroring the GNOME reading of `snapshot.windows`). Returns
    /// an empty `Vec` when the core is gone.
    #[must_use]
    pub fn windows(&self) -> Vec<FfiWinInfo> {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return Vec::new();
        };
        let snapshot = core.read_snapshot();
        snapshot
            .windows
            .values()
            .map(FfiWinInfo::from_win_info)
            .collect()
    }

    /// Returns the grid id bound to window handle `win`, or 0 if no window maps to
    /// it (e.g. it was closed) or the core is gone.
    #[must_use]
    pub fn grid_for_win(&self, win: i64) -> u64 {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return 0;
        };
        core.read_snapshot().grid_for_win(win).unwrap_or(0)
    }

    /// Returns the currently active Neovim window handle, or 0 if not yet known or
    /// the core is gone.
    #[must_use]
    pub fn active_win(&self) -> i64 {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return 0;
        };
        core.read_snapshot().active_win.unwrap_or(0)
    }

    /// Creates a new nvim TABPAGE, optionally rooted at `cwd` via `:tcd` (an
    /// empty string means no `:tcd`), and returns its stable HANDLE.
    ///
    /// Blocks briefly on the captive nvim (via [`WolfCore::create_tabpage`]).
    /// Returns 0 when the core has been shut down, so the caller drops the
    /// would-be tab.
    #[must_use]
    pub fn create_tabpage(&self, cwd: String) -> i64 {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return 0;
        };
        let path = (!cwd.is_empty()).then(|| PathBuf::from(cwd));
        core.create_tabpage(path.as_deref())
    }

    /// Realizes `tab`'s root in the current tabpage's single starting window,
    /// returning the resulting window handles in DFS/leaf order.
    ///
    /// Rust renders the spec with [`app::tab_rebuild_spec`], so no front-end renders
    /// it. It checks no file, so pass it [`FfiAppTab::restored`]'s output. Delegates
    /// to [`WolfCore::rebuild_layout`], which drives the `wolf_rebuild_layout` Lua
    /// helper (`:split`/`:vsplit` + `:edit`). Blocks briefly on the captive nvim; an
    /// empty `Vec` when the tab has no root, on error, or when the core is gone.
    /// nvim produces only the window COUNT + TOPOLOGY + per-window BUFFER; sizes
    /// come from the front-end's weights.
    ///
    /// When it returns, [`win_buffer`](Self::win_buffer) and [`previews`](Self::previews)
    /// already report every returned window, so a state commit straight after it reads
    /// the restored files rather than the startup buffer.
    #[must_use]
    pub fn rebuild_tab(&self, tab: &FfiAppTab) -> Vec<i64> {
        let Some(spec) = app::tab_rebuild_spec(&tab.content) else {
            return Vec::new();
        };
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return Vec::new();
        };
        core.rebuild_layout(&spec)
    }

    /// The current tabpage's stable HANDLE via a blocking RPC. 0 when the core
    /// has been shut down.
    ///
    /// Contrast [`active_tabpage`](Self::active_tabpage), the cheap snapshot read.
    #[must_use]
    pub fn current_tabpage(&self) -> i64 {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return 0;
        };
        core.current_tabpage()
    }

    /// The CHEAP `Snapshot.current_tabpage` (updated out-of-band via the
    /// `wolf_tabpage` notification), read from the snapshot with NO RPC. 0 when
    /// the core has been shut down or the tabpage is not yet known.
    ///
    /// Mirrors [`active_win`](Self::active_win). Because it needs no RPC, a dead
    /// core still answers it, which is why the macOS restart capture keys
    /// [`open_files_for_tabpage`](Self::open_files_for_tabpage) by it rather than
    /// by the authoritative [`current_tabpage`](Self::current_tabpage) RPC.
    #[must_use]
    pub fn active_tabpage(&self) -> i64 {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return 0;
        };
        core.read_snapshot().current_tabpage
    }

    /// A tabpage's tab-local `:tcd` cwd (canonicalized), AUTHORITATIVE for a
    /// tabpage that is not current. Blocks briefly on the captive nvim (via
    /// [`WolfCore::cwd_for_tabpage`]). Empty when the core has been shut down or
    /// the handle is invalid (e.g. a closed tabpage).
    #[must_use]
    pub fn cwd_for_tabpage(&self, tab: i64) -> String {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return String::new();
        };
        core.cwd_for_tabpage(tab)
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned())
    }

    /// Makes tabpage `tab` current. A no-op once the core has been shut down.
    pub fn set_current_tabpage(&self, tab: i64) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.set_current_tabpage(tab);
        }
    }

    /// Closes tabpage `tab`. A last-tabpage close is swallowed: a front-end ends
    /// the last tabpage by shutting the core down. A no-op once the core has been
    /// shut down.
    pub fn close_tabpage(&self, tab: i64) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.close_tabpage(tab);
        }
    }

    /// Takes `item`'s wait and hands it to this core through [`WolfCore::wait_on`], for the
    /// item's file. An item with no wait is a no-op; one whose target is not a file, or a core
    /// that has been shut down, ends the wait with `1`.
    pub fn wait_on(&self, item: &FfiOpenItem) {
        let Some(wait) = item.wait.borrow_mut().take() else {
            return;
        };
        let borrow = self.core.borrow();
        match (borrow.as_ref(), &item.target) {
            (Some(core), OpenTarget::File(path)) => core.wait_on(path, wait),
            _ => drop(wait),
        }
    }

    /// Focuses a specific Neovim window (click-to-focus, or after resolving a
    /// focus-direction intent into a concrete neighbour). A no-op once the core
    /// has been shut down.
    pub fn focus_win(&self, win: i64) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.focus_win(win);
        }
    }

    /// The captive nvim's liveness (`mod liveness`: `ALIVE` / `NOT_RESPONDING` /
    /// `DEAD`), a cheap lock-free read (no RPC). `DEAD` when the core is gone. The
    /// front-end reads this on each wake to drive the crash/stall overlay.
    #[must_use]
    pub fn liveness(&self) -> u8 {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return liveness::DEAD;
        };
        // Map through `mod liveness` so a mismatch with `wolf_core::Liveness` fails to compile
        // rather than drifting silently.
        match core.liveness() {
            Liveness::Alive => liveness::ALIVE,
            Liveness::NotResponding => liveness::NOT_RESPONDING,
            Liveness::Dead => liveness::DEAD,
        }
    }

    /// Restarts the captive nvim IN PLACE after a crash/stall (via
    /// [`WolfCore::restart_nvim`]). Returns `true` ONLY when an ACTUAL restart
    /// happened and succeeded (post-contract: liveness `Alive`, fresh tabpage-1,
    /// empty buffer list); `false` for a NO-OP (`Ok(false)` — a live nvim left
    /// undisturbed, or a concurrent restart in flight), when the respawn FAILED
    /// (logged; liveness is left `Dead`), or when the core is gone.
    ///
    /// The front-end drives the reopen ONLY when this returns `true`, off
    /// the Tier-1 files it captured via
    /// [`open_files_for_tabpage`](Self::open_files_for_tabpage) BEFORE the restart
    /// (a respawn wipes the mirror). On `false` the front-end re-reads
    /// [`liveness`](Self::liveness) and refreshes its overlay instead.
    #[must_use]
    pub fn restart(&self) -> bool {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return false;
        };
        match core.restart_nvim() {
            Ok(did_restart) => did_restart,
            Err(e) => {
                tracing::warn!("wolf-ffi: nvim restart failed: {e}");
                false
            }
        }
    }

    /// The Tier-1 open-file mirror entry KEYED BY tabpage `tab`, a cheap snapshot
    /// read (no RPC). An empty-backed [`FfiOpenFiles`] when the core is gone or the
    /// tab is unknown. The front-end captures this BEFORE a restart (a respawn
    /// wipes the mirror) and reopens the files afterward.
    ///
    /// Only `active_path` is per-tabpage; `files` is the whole instance's buflisted
    /// set (see [`FfiOpenFiles`]), so a core showing several tabpages over-reopens.
    #[must_use]
    pub fn open_files_for_tabpage(&self, tab: i64) -> FfiOpenFiles {
        let borrow = self.core.borrow();
        let mirror = borrow
            .as_ref()
            .and_then(|core| core.open_files_for_tabpage(tab));
        FfiOpenFiles::from_tab_files(mirror)
    }

    /// Forwards the focus of the tab this core serves to the captive Neovim UI via
    /// [`WolfCore::set_ui_focus`]: `true` when the tab gains focus, the app becoming
    /// active included (fires `FocusGained`, which the init.lua `WolfAutoRead`
    /// augroup rides to `:checktime` and reload an unmodified buffer changed on
    /// disk), `false` when it loses it. nvim fires the autocmd on every call, so
    /// the front-end sends each edge once. A no-op once the core has been shut down.
    pub fn set_ui_focus(&self, gained: bool) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.set_ui_focus(gained);
        }
    }

    /// Cleanly shuts down the captive Neovim and tears down the runtime.
    ///
    /// Takes the core out of the handle (so a second call is a no-op) and consumes
    /// it via [`WolfCore::shutdown`]; any shutdown error is logged, not returned
    /// (the bridge signature is infallible).
    pub fn shutdown(&self) {
        if let Some(core) = self.core.borrow_mut().take() {
            if let Err(e) = core.shutdown() {
                tracing::warn!("wolf-ffi: core shutdown failed: {e}");
            }
        }
    }

    /// Starts a streaming file-search session over the core's cwd, fed by the
    /// registered file-search wake.
    ///
    /// Infallible across the bridge: when the core has already been shut down it
    /// returns an empty-backed [`FfiFileSearch`] (whose [`FfiFileSearch::ranked`]
    /// yields no entries), mirroring [`read_frame`](WolfCoreHandle::read_frame)'s
    /// empty-frame fallback.
    #[must_use]
    pub fn start_file_search(&self) -> FfiFileSearch {
        let search = self
            .core
            .borrow()
            .as_ref()
            .map(|core| core.start_file_search(registered_filesearch_wake(self.wake_token)));
        FfiFileSearch {
            search,
            tree_cache: RefCell::new(wolf_core::tree::TreeIndexCache::new()),
        }
    }

    /// Starts a SEPARATE long-lived streaming file-search session for the
    /// file-tree sidebar, fed by the registered tree wake.
    ///
    /// Infallible across the bridge, exactly like
    /// [`start_file_search`](WolfCoreHandle::start_file_search): when the core has
    /// already been shut down it returns an empty-backed [`FfiFileSearch`] (whose
    /// projection methods yield nothing). The sidebar owns this session for its
    /// whole lifetime, independent of the transient picker session.
    #[must_use]
    pub fn start_tree_search(&self) -> FfiFileSearch {
        let search = self
            .core
            .borrow()
            .as_ref()
            .map(|core| core.start_file_search(registered_tree_wake(self.wake_token)));
        FfiFileSearch {
            search,
            tree_cache: RefCell::new(wolf_core::tree::TreeIndexCache::new()),
        }
    }

    /// Starts a ripgrep-backed CONTENT search for the sidebar search panel, fed by
    /// the registered search wake.
    ///
    /// Infallible across the bridge, exactly like
    /// [`start_file_search`](WolfCoreHandle::start_file_search): when the core has
    /// already been shut down (or `WOLF_RG_BIN` is unset) it returns an
    /// empty-backed [`FfiSearch`] (whose [`FfiSearch::files`] yields nothing).
    /// Dropping the returned handle cancels the rg subprocess.
    #[must_use]
    pub fn start_search(
        &self,
        query: String,
        use_regex: bool,
        case_sensitive: bool,
        all_files: bool,
    ) -> FfiSearch {
        let opts = SearchOptions {
            query,
            use_regex,
            case_sensitive,
            all_files,
        };
        let search = self
            .core
            .borrow()
            .as_ref()
            .map(|core| core.start_search(opts, registered_search_wake(self.wake_token)));
        FfiSearch {
            search,
            cache: RefCell::new(SearchProjectionCache::new()),
        }
    }

    /// The captive nvim's `ignorecase` option (a one-shot seed for the search
    /// panel's "case sensitive" toggle). Blocks briefly on the captive nvim; false
    /// on error/timeout or a shut-down core.
    #[must_use]
    pub fn ignorecase(&self) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(WolfCore::ignorecase)
    }

    /// Opens `path` AND jumps the cursor to `line`/`col` (a search-result click)
    /// via [`InputEvent::OpenAt`]; the core's input pump records the MRU. `line` is
    /// 1-based, `col` a 0-based byte offset. A no-op once the core has been shut
    /// down.
    pub fn open_at(&self, path: String, line: u64, col: u64) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.open_at(PathBuf::from(path), line, col);
        }
    }

    /// Builds a fresh command-palette session seeded with the core's recent
    /// directories + the global command MRU.
    ///
    /// The front-end feeds each OTHER open window via
    /// [`FfiCommandPalette::add_window`], then filters per keystroke via
    /// [`FfiCommandPalette::ranked`]. Infallible across the bridge: when the core
    /// has already been shut down it returns an empty-backed session (no recent
    /// dirs and a disabled MRU), mirroring
    /// [`start_file_search`](WolfCoreHandle::start_file_search)'s empty-frame
    /// fallback.
    #[must_use]
    pub fn command_palette(&self) -> FfiCommandPalette {
        let borrow = self.core.borrow();
        let (recent_dirs, command_mru) = borrow.as_ref().map_or_else(
            || (Vec::new(), CommandMru::disabled()),
            |core| (core.recent_dirs(), core.command_mru()),
        );
        FfiCommandPalette {
            recent_dirs,
            windows: RefCell::new(Vec::new()),
            command_mru,
        }
    }

    /// The absolute path of the buffer currently being edited (canonicalized by
    /// core via the `wolf_current_file` notification), or the empty string for an
    /// unnamed buffer or a shut-down core.
    ///
    /// Read on each editor wake by the sidebar to highlight + reveal the current
    /// file.
    #[must_use]
    pub fn current_file(&self) -> String {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return String::new();
        };
        let snapshot = core.read_snapshot();
        snapshot
            .current_file
            .as_ref()
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned())
    }

    /// Neovim's LIVE, already-canonicalized working directory (tracking
    /// `:cd`/`DirChanged`), or the empty string until nvim first reports it or
    /// after the core has been shut down.
    ///
    /// Read on each editor render wake by the sidebar to re-root the file tree
    /// when the cwd changed. Routes through [`WolfCore::live_cwd`] — cwd is NOT in
    /// the render snapshot, so (unlike [`current_file`](Self::current_file)) it
    /// must not read `read_snapshot()`.
    #[must_use]
    pub fn cwd(&self) -> String {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return String::new();
        };
        core.live_cwd()
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned())
    }

    /// [`WolfCore::recent_dirs`], each `PathBuf` mapped via
    /// `to_string_lossy().into_owned()`; an empty `Vec` once the core has been shut
    /// down.
    ///
    /// Read lazily at Open-Recent-menu open time by the macOS shell to build the
    /// File > Open Recent submenu.
    #[must_use]
    pub fn recent_dirs(&self) -> Vec<String> {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return Vec::new();
        };
        core.recent_dirs()
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    /// [`WolfCore::record_recent_dir`] of `path`; a no-op once the core has been
    /// shut down.
    ///
    /// Called by the macOS shell once per Open Folder, on the core of the tab that
    /// ends on the folder.
    pub fn record_recent_dir(&self, path: String) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.record_recent_dir(std::path::Path::new(&path));
        }
    }

    /// The `WolfTitlebar` theme group's background color, from
    /// `Snapshot.theme.titlebar.bg`.
    ///
    /// Follows the `default_fg`/`default_bg` unset convention: `set == false`
    /// when the group is absent or cleared, or once the core has been shut
    /// down. Read on each render wake by the macOS shell, which prefers it to
    /// the `Normal` background for the titlebar's `NSWindow.backgroundColor`.
    #[must_use]
    pub fn theme_titlebar_bg(&self) -> FfiColor {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiColor::unset();
        };
        FfiColor::from_opt(core.read_snapshot().theme.titlebar.bg)
    }

    /// The `WolfTitlebar` theme group's foreground color, carried across the
    /// bridge and applied by no front-end: the macOS title follows the resolved
    /// appearance.
    ///
    /// Same unset convention as [`theme_titlebar_bg`](Self::theme_titlebar_bg).
    #[must_use]
    pub fn theme_titlebar_fg(&self) -> FfiColor {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiColor::unset();
        };
        FfiColor::from_opt(core.read_snapshot().theme.titlebar.fg)
    }

    /// The per-pane statusbar strings for the window drawing into `grid`.
    ///
    /// Delegates to [`wolf_core::WolfCore::pane_status`], which joins `grid` →
    /// window → buffer path and formats it over nvim's live cwd + the user's
    /// home. Returns the `[No Name]` filename with an empty display path when the
    /// core is gone (mirroring the empty-status convention for unnamed buffers).
    #[must_use]
    pub fn pane_status_for_grid(&self, grid: u64) -> FfiPaneStatus {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiPaneStatus::empty();
        };
        FfiPaneStatus::from_pane_status(core.pane_status(grid))
    }

    /// What window `win` shows, read from core's `win_buffers` map by handle.
    ///
    /// That map spans every tabpage, so a window on a background tabpage is still
    /// reported, where [`pane_status_for_grid`](Self::pane_status_for_grid) joins through
    /// the current-tabpage-only `windows` map and answers `[No Name]`. The path is the
    /// same string `pane_status_for_grid(..).abs_path()` gives a current-tabpage window.
    /// Not reported, with an empty path, for an unknown window or a shut-down core.
    #[must_use]
    pub fn win_buffer(&self, win: i64) -> FfiWinBuffer {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiWinBuffer::unreported();
        };
        let snap = core.read_snapshot();
        snap.win_buffers
            .get(&win)
            .map_or_else(FfiWinBuffer::unreported, |wb| FfiWinBuffer {
                reported: true,
                abs_path: wb
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                terminal: wb.terminal.map_or(0, |id| id.0),
            })
    }

    /// Window `win`'s cursor and top line, read from Neovim through
    /// [`WolfCore::window_cursor`] for the application state's capture.
    ///
    /// Blocks briefly on the captive nvim. Not found, with every field `0`, for
    /// `win <= 0`, an unknown window, an RPC error or timeout, or a shut-down core.
    #[must_use]
    pub fn window_cursor(&self, win: i64) -> FfiWindowCursor {
        self.core
            .borrow()
            .as_ref()
            .and_then(|core| core.window_cursor(win))
            .map_or_else(FfiWindowCursor::default, |c| FfiWindowCursor {
                found: true,
                line: c.cursor.line,
                col: c.cursor.col,
                top_line: c.top_line,
            })
    }

    /// Opens `path` (an absolute filesystem path) in Neovim via
    /// [`InputEvent::Open`]; the core's input pump records the MRU. A no-op once
    /// the core has been shut down.
    pub fn open(&self, path: String) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.send_input(InputEvent::Open {
                path: PathBuf::from(path),
            });
        }
    }

    /// Changes the current tabpage's working directory to `path` (an absolute
    /// filesystem path) via [`InputEvent::Chdir`] (`:tcd`, the tab-local cwd). A
    /// macOS tab's core holds one tabpage, so the whole tab moves. A no-op once
    /// the core has been shut down.
    pub fn chdir(&self, path: String) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.send_input(InputEvent::Chdir {
                path: PathBuf::from(path),
            });
        }
    }

    /// Sets the global `background` option via [`WolfCore::set_background`]
    /// (`:set background=dark|light`). `dark: true` selects dark, `false` light.
    /// The macOS front-end pushes the resolved appearance at launch and, in
    /// `auto` mode, live when the OS toggles. A no-op once the core has been shut
    /// down.
    pub fn set_background(&self, dark: bool) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.set_background(dark);
        }
    }

    /// Closes the current split (the pane) via [`InputEvent::ClosePane`], which refuses the
    /// tab's last pane. A no-op once the core has been shut down.
    pub fn close_pane(&self) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.send_input(InputEvent::ClosePane);
        }
    }

    /// Splits the active pane into two stacked panes, as `:split`, via
    /// [`WolfCore::split_pane`]. A no-op once the core has been shut down.
    pub fn split_horizontal(&self) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.split_pane(SplitOrientation::Horizontal);
        }
    }

    /// Splits the active pane into two side-by-side panes, as `:vsplit`, via
    /// [`WolfCore::split_pane`]. A no-op once the core has been shut down.
    pub fn split_vertical(&self) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.split_pane(SplitOrientation::Vertical);
        }
    }

    /// Opens a new empty buffer in the current window and names it the next free
    /// `Untitled N.md` — `Untitled.md`, then `Untitled 2.md`, … (the first name
    /// colliding with neither an existing buffer nor a file on disk in cwd),
    /// markdown-typed but NOT written to disk — via [`InputEvent::NewBuffer`]
    /// (the `wolf_new_untitled` Lua helper). A no-op once the core has been shut
    /// down.
    pub fn new_buffer(&self) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.send_input(InputEvent::NewBuffer);
        }
    }

    /// Toggles the terminal drawer via [`WolfCore::toggle_terminal`], which
    /// starts the drawer's session when none is live. A no-op once the core is gone.
    pub fn toggle_terminal(&self) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.toggle_terminal();
        }
    }

    /// Toggles the active buffer's content width between the ~90-char default and
    /// full pane width via [`WolfCore::toggle_full_width`] (the
    /// `wolf_toggle_full_width` Lua helper; targets the SOURCE buffer when the active
    /// pane is a preview scratch). A no-op once the core is gone.
    pub fn toggle_full_width(&self) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.toggle_full_width();
        }
    }

    /// Whether the active buffer is in FULL-WIDTH mode (`current_is_full_width`).
    /// Drives the View > Use full width for content menu checkmark. False once the
    /// core has been shut down.
    #[must_use]
    pub fn is_full_width_active(&self) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(WolfCore::is_full_width_active)
    }

    /// Whether the window `win` is in FULL-WIDTH mode (the per-window `full_width`
    /// map). The front-end threads this into a pane's geometry (`maxCols: nil` for
    /// full width, `90` for the ~90-char default). False once the core has been shut
    /// down or when `win` is absent from the map.
    #[must_use]
    pub fn full_width_for_win(&self, win: i64) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|c| c.full_width_for_win(win))
    }

    /// The current visual-mode selection yanked to text, or the empty string
    /// when not in visual mode / the selection is empty / the core is gone.
    /// Blocks briefly on the captive nvim (see
    /// [`WolfCore::copy_visual_selection`]).
    #[must_use]
    pub fn copy_selection(&self) -> String {
        self.core
            .borrow()
            .as_ref()
            .and_then(WolfCore::copy_visual_selection)
            .unwrap_or_default()
    }

    /// Pastes `text` into the active document via [`InputEvent::Paste`]
    /// (`nvim_paste`). A no-op once the core has been shut down.
    pub fn paste(&self, text: String) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.paste(text);
        }
    }

    /// Builds a fresh buffer-picker session seeded from a blocking snapshot of the
    /// core's listed buffers plus the live cwd.
    ///
    /// The front-end filters per keystroke via [`FfiBufferPicker::ranked`].
    /// Infallible across the bridge: when the core has already been shut down it
    /// returns an empty-backed session (no buffers, no cwd), mirroring
    /// [`command_palette`](WolfCoreHandle::command_palette)'s empty fallback.
    #[must_use]
    pub fn buffer_picker(&self) -> FfiBufferPicker {
        let borrow = self.core.borrow();
        let (buffers, cwd) = borrow.as_ref().map_or_else(
            || (Vec::new(), None),
            |core| (core.list_buffers(), core.live_cwd()),
        );
        FfiBufferPicker { buffers, cwd }
    }

    /// Switches the ACTIVE window to buffer `bufnr` via [`InputEvent::SwitchBuffer`]
    /// (window-scoped `nvim_win_set_buf` + per-pane view restore).
    ///
    /// Resolves the active window live from the snapshot; a no-op when the core is
    /// gone or no active window is known (handle 0).
    pub fn switch_buffer(&self, bufnr: i64) {
        if let Some(core) = self.core.borrow().as_ref() {
            let win = core.read_snapshot().active_win.unwrap_or(0);
            if win != 0 {
                core.switch_buffer(win, bufnr);
            }
        }
    }

    /// Whether the active document is in Neovim visual mode. Gates the Edit >
    /// Copy menu item. False once the core has been shut down.
    #[must_use]
    pub fn is_visual_mode(&self) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|c| c.read_snapshot().is_visual_mode())
    }

    /// Whether the active buffer is a Markdown file. Gates the View > Preview menu
    /// item + palette row. False once the core has been shut down.
    #[must_use]
    pub fn is_markdown_active(&self) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|c| c.read_snapshot().is_markdown_active())
    }

    /// Whether the active buffer is a PREVIEW buffer (`current_is_preview`). Gates
    /// the View > Edit menu item. False once the core has been shut down.
    #[must_use]
    pub fn is_preview_active(&self) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|c| c.read_snapshot().is_preview_active())
    }

    /// Navigates from the active PREVIEW pane back to its source file in place —
    /// the active pane becomes an edit pane — and returns the source buffer's
    /// number.
    ///
    /// Blocks briefly on the captive nvim. Returns `0` when the active buffer is
    /// not a preview, on error, or once the core has been shut down.
    #[must_use]
    pub fn edit_source(&self) -> i64 {
        self.core.borrow().as_ref().map_or(0, WolfCore::edit_source)
    }

    /// Previews the active source buffer in place — the active pane becomes the
    /// preview pane — and returns the preview scratch buffer's number.
    ///
    /// Blocks briefly on the captive nvim. Returns `0` when the active buffer is
    /// not Markdown, on error, or once the core has been shut down.
    #[must_use]
    pub fn open_preview(&self) -> i64 {
        self.core
            .borrow()
            .as_ref()
            .map_or(0, WolfCore::open_preview)
    }

    /// Renders the Markdown SOURCE buffer `source_bufnr` to a themed HTML document
    /// the front-end web view loads.
    ///
    /// Blocks briefly on the captive nvim. Returns the empty string on error or
    /// once the core has been shut down.
    #[must_use]
    pub fn render_preview(&self, source_bufnr: i64) -> String {
        self.core
            .borrow()
            .as_ref()
            .map_or_else(String::new, |c| c.render_preview(source_bufnr))
    }

    /// Opens (or reuses) the loopback-only browser-preview HTTP server pinned at
    /// the active Markdown buffer and returns its `http://127.0.0.1:<port>/<relpath>`
    /// URL (the pinned file's root-relative path), which the front-end opens in the
    /// system default browser.
    ///
    /// Blocks briefly on the captive nvim to resolve the buffer. Returns the empty
    /// string when the active buffer is not Markdown, on error, or once the core
    /// has been shut down.
    #[must_use]
    pub fn open_in_browser(&self) -> String {
        self.core
            .borrow()
            .as_ref()
            .map_or_else(String::new, WolfCore::open_in_browser)
    }

    /// Repoints the preview window `win` at the Markdown file at absolute `path`
    /// (a linked file the user clicked in the preview), replacing that pane's preview
    /// with the linked file's, and returns the linked file's SOURCE bufnr.
    ///
    /// Blocks briefly on the captive nvim. Returns the linked file's source bufnr
    /// (`> 0`) on success; `0` when `path` is empty, unreadable, or not Markdown, on
    /// error, or once the core has been shut down. `path` is ABSOLUTE (no relative
    /// resolution). On `0` the front-end falls back to a plain open.
    #[must_use]
    pub fn preview_file(&self, win: i64, path: String) -> i64 {
        self.core
            .borrow()
            .as_ref()
            .map_or(0, |c| c.preview_file(win, &path))
    }

    /// Previews the already-open buffer `bufnr` in window `win` (`0` = the current
    /// window), replacing that pane's buffer with the source's preview scratch —
    /// the preview-mode buffer-picker activation path.
    ///
    /// Blocks briefly on the captive nvim. Returns the source bufnr (`> 0`) on
    /// success; `0` when `bufnr` is invalid, unnamed, or not Markdown, on error, or
    /// once the core has been shut down. On `0` the front-end falls back to
    /// [`switch_buffer`](Self::switch_buffer) (the pane converts to edit mode).
    #[must_use]
    pub fn preview_buffer(&self, win: i64, bufnr: i64) -> i64 {
        self.core
            .borrow()
            .as_ref()
            .map_or(0, |c| c.preview_buffer(win, bufnr))
    }

    /// Previews the active pane's Markdown file in ANOTHER pane: binds the
    /// preview scratch into `target_win` when it is a usable other pane, else
    /// splits a new pane to the right (surfacing as an ordinary vertical Split
    /// layout intent). Focus stays in the source-editing pane.
    ///
    /// Blocks briefly on the captive nvim. Returns the SOURCE bufnr (`> 0`) on
    /// success; `0` when the active buffer is not Markdown, on error, or once
    /// the core has been shut down.
    #[must_use]
    pub fn preview_in_other(&self, target_win: i64) -> i64 {
        self.core
            .borrow()
            .as_ref()
            .map_or(0, |c| c.preview_in_other(target_win))
    }

    /// Edits the active preview pane's SOURCE file in ANOTHER pane: binds the
    /// source into `target_win` when it is a usable other pane, else splits (the
    /// scratch keeps the new right window, the source lands in the original).
    /// Focuses the pane now editing the source.
    ///
    /// Blocks briefly on the captive nvim. Returns the bufnr bound into that
    /// pane (`> 0`) on success; `0` when the active pane is not a preview, on
    /// error, or once the core has been shut down.
    #[must_use]
    pub fn edit_in_other(&self, target_win: i64) -> i64 {
        self.core
            .borrow()
            .as_ref()
            .map_or(0, |c| c.edit_in_other(target_win))
    }

    /// Splits the whole tab vertically with `path` in a new, current window on the left, and
    /// returns its handle, through [`WolfCore::open_split_left`]. No split intent follows, so
    /// the caller binds the window itself. Blocks briefly; `0` on failure or once shut down.
    #[must_use]
    pub fn open_split_left(&self, path: String) -> i64 {
        self.core
            .borrow()
            .as_ref()
            .map_or(0, |c| c.open_split_left(std::path::Path::new(&path)))
    }

    /// Mints a fresh [`TabId`], gives it to the core through [`WolfCore::set_tab_id`], so the
    /// terminal sessions it starts from now on carry it in `WOLF_TAB_ID`, and returns it
    /// lowercase. `""` once shut down.
    #[must_use]
    pub fn assign_tab_id(&self) -> String {
        let core = self.core.borrow();
        let Some(core) = core.as_ref() else {
            return String::new();
        };
        let id = TabId::random();
        core.set_tab_id(id);
        id.to_string()
    }

    /// A snapshot of the open preview panes: one [`FfiPreview`] per window showing a
    /// preview scratch buffer, each joined to its source buffer's live
    /// `changedtick` (from `Snapshot::markdown_ticks`, `0` until the first
    /// `wolf_markdown_dirty`) AND to its source buffer's editing CURSOR (from
    /// `Snapshot::markdown_cursors`, defaulting to the `line: 0` "unknown" sentinel
    /// until the first `wolf_markdown_cursor`). Both joins key on the same
    /// `PreviewInfo::source_bufnr`. Empty once the core has been shut down.
    #[must_use]
    pub fn previews(&self) -> Vec<FfiPreview> {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return Vec::new();
        };
        let snap = core.read_snapshot();
        snap.previews
            .iter()
            .map(|(&win, info)| {
                let source_tick = snap
                    .markdown_ticks
                    .get(&info.source_bufnr)
                    .copied()
                    .unwrap_or(0);
                let cursor = snap
                    .markdown_cursors
                    .get(&info.source_bufnr)
                    .copied()
                    .unwrap_or_default();
                let source_display_path = core.display_path(&info.source_path.to_string_lossy());
                FfiPreview::from_preview(win, info, source_tick, source_display_path, cursor)
            })
            .collect()
    }

    /// Whether pane `win` has an older entry to go Back to in its per-pane
    /// navigation history.
    ///
    /// A cheap snapshot-side read (a brief mutex, no RPC), so the front-end can call
    /// it on every render wake to enable/disable the toolbar button. `false` for an
    /// unknown window, a never-navigated pane, the terminal DRAWER (which is EXCLUDED
    /// from history), or once the core has been shut down.
    #[must_use]
    pub fn history_can_back(&self, win: i64) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|c| c.history_can_back(win))
    }

    /// Whether pane `win` has a newer entry to go Forward to. Same contract as
    /// [`history_can_back`](Self::history_can_back).
    #[must_use]
    pub fn history_can_forward(&self, win: i64) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|c| c.history_can_forward(win))
    }

    /// Moves pane `win` one entry BACK in its navigation history and returns whether
    /// the pane actually MOVED.
    ///
    /// Blocks briefly on the captive nvim. `false` at the bottom of the stack, for an
    /// unknown window, when the recorded target no longer exists (the cursor is then
    /// REVERTED, so Back stays available), on RPC error, or once the core has been
    /// shut down.
    pub fn history_back(&self, win: i64) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|c| c.history_back(win))
    }

    /// Moves pane `win` one entry FORWARD. Same contract as
    /// [`history_back`](Self::history_back).
    pub fn history_forward(&self, win: i64) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|c| c.history_forward(win))
    }

    /// Moves pane `win` to the STACK index `index` (an [`FfiHistoryEntry::index`])
    /// and returns whether the pane actually MOVED.
    ///
    /// `false` for an out-of-range index AND for the cursor's own index, so clicking
    /// the checkmarked History-menu row is a true no-op. Otherwise identical to
    /// [`history_back`](Self::history_back).
    pub fn history_jump(&self, win: i64, index: u32) -> bool {
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|c| c.history_jump(win, index as usize))
    }

    /// Pane `win`'s History-menu rows as opaque [`FfiHistoryEntry`] values:
    /// NEWEST-FIRST, capped at 10 DISTINCT targets, carrying finished disambiguated
    /// labels and at most one `is_current`. Empty for an unknown window, a
    /// never-navigated pane, the terminal DRAWER, or once the core has been shut
    /// down.
    #[must_use]
    pub fn history_entries(&self, win: i64) -> Vec<FfiHistoryEntry> {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return Vec::new();
        };
        core.history_entries(win)
            .into_iter()
            .map(FfiHistoryEntry::from_menu_row)
            .collect()
    }

    /// The native command line (`ext_cmdline`) as an opaque [`FfiCmdline`].
    ///
    /// Projects [`Snapshot::cmdline`](wolf_core::Snapshot::cmdline). An inactive
    /// command line — or a shut-down core — yields the inactive default (the
    /// front-end paints nothing / falls back to the last toast line).
    #[must_use]
    pub fn cmdline(&self) -> FfiCmdline {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiCmdline::empty();
        };
        FfiCmdline::from_cmdline(&core.read_snapshot().cmdline)
    }

    /// The completion popup (`ext_popupmenu`) as an opaque [`FfiPum`].
    ///
    /// Projects [`Snapshot::pum`](wolf_core::Snapshot::pum). A hidden popup — or
    /// a shut-down core — yields the hidden default.
    #[must_use]
    pub fn pum(&self) -> FfiPum {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiPum::empty();
        };
        FfiPum::from_pum(&core.read_snapshot().pum)
    }

    /// The message toast (`ext_messages`) as an opaque [`FfiToast`].
    ///
    /// Projects [`Snapshot::toast`](wolf_core::Snapshot::toast), RESOLVING each
    /// chunk's colors in Rust against the SAME highlight table the frame uses
    /// (like [`FfiCell`]) — message color is load-bearing and must never be
    /// re-resolved in Swift. An empty toast — or a shut-down core — yields the
    /// hidden default.
    #[must_use]
    pub fn toast(&self) -> FfiToast {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiToast::empty();
        };
        let snapshot = core.read_snapshot();
        FfiToast::from_toast(&snapshot.toast, &snapshot.highlights)
    }

    /// Starts a terminal session and shows it in `target` (a [`terminal_target`]
    /// discriminant) of window `win`, `0` meaning Neovim's current window. An empty
    /// `command` runs the user's shell; any other runs as `/bin/sh -c <command>`.
    ///
    /// Returns the session id, or `0` when the session could not be started or bound,
    /// for an unknown `target`, or once the core is shut down. Blocks as
    /// [`WolfCore::open_terminal`] does.
    #[must_use]
    pub fn open_terminal(&self, win: i64, target: u8, command: String) -> u64 {
        let Some(target) = terminal_target_from_ffi(win, target) else {
            return 0;
        };
        let command = (!command.is_empty()).then(|| TerminalCommand {
            program: PathBuf::from("/bin/sh"),
            args: vec!["-c".to_owned(), command],
        });
        self.core
            .borrow()
            .as_ref()
            .and_then(|core| core.open_terminal(target, command))
            .map_or(0, |id| id.0)
    }

    /// Shows running session `id` in `target` of window `win`, as
    /// [`open_terminal`](Self::open_terminal) places a new one. `false` for an id with no
    /// running session, an unknown `target`, or a shut-down core.
    #[must_use]
    pub fn bind_terminal(&self, id: u64, win: i64, target: u8) -> bool {
        let Some(target) = terminal_target_from_ffi(win, target) else {
            return false;
        };
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|core| core.bind_terminal(TerminalId(id), target))
    }

    /// Queues a [`bind_terminal`](Self::bind_terminal) behind every input queued before it,
    /// through [`WolfCore::queue_bind_terminal`]. `false`, queueing nothing, for an id with
    /// no running session, an unknown `target`, or a shut-down core. Never blocks.
    #[must_use]
    pub fn queue_bind_terminal(&self, id: u64, win: i64, target: u8) -> bool {
        let Some(target) = terminal_target_from_ffi(win, target) else {
            return false;
        };
        self.core
            .borrow()
            .as_ref()
            .is_some_and(|core| core.queue_bind_terminal(TerminalId(id), target))
    }

    /// The directory session `id`'s shell is in, through [`WolfCore::terminal_cwd`], which a
    /// front-end persists as a terminal leaf's path. `""` for an unknown session, an
    /// unreadable cwd, or a shut-down core.
    #[must_use]
    pub fn terminal_cwd(&self, id: u64) -> String {
        self.core
            .borrow()
            .as_ref()
            .and_then(|core| core.terminal_cwd(TerminalId(id)))
            .map(|cwd| cwd.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Session `id`'s latest published frame, or the empty frame (version `0`, `0x0`)
    /// for an unknown id or a shut-down core. Shares the frame rather than copying it.
    #[must_use]
    pub fn terminal_frame(&self, id: u64) -> FfiTerminalFrame {
        FfiTerminalFrame {
            frame: self
                .core
                .borrow()
                .as_ref()
                .and_then(|core| core.terminal_frame(TerminalId(id))),
        }
    }

    /// Maps `ev` into a [`wolf_core::KeyEvent`] and sends it to session `id` through
    /// core's `<C-w>` router. A no-op on an unrecognized kind or a shut-down core.
    pub fn terminal_key(&self, id: u64, ev: FfiKeyEvent) {
        if let Some(key) = key_event_from_ffi(&ev) {
            if let Some(core) = self.core.borrow().as_ref() {
                core.terminal_key(TerminalId(id), key);
            }
        }
    }

    /// Sends `text` to session `id` as typed. A no-op once the core is shut down.
    pub fn terminal_text(&self, id: u64, text: String) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.terminal_text(TerminalId(id), &text);
        }
    }

    /// Pastes `text` into session `id`, bracketed when its program set mode 2004. A
    /// no-op once the core is shut down.
    pub fn terminal_paste(&self, id: u64, text: String) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.terminal_paste(TerminalId(id), &text);
        }
    }

    /// A mouse event over session `id`'s pane. `kind` is a [`terminal_mouse_kind`]
    /// discriminant, `button` a [`mouse_button`] one ([`mouse_button::NONE`] for motion
    /// with nothing held) and `mods` a [`terminal_mods`] bitset. `x` and `y` are pixels
    /// from the content area's top-left, in the cell pixels of the last
    /// [`resize_terminal`](Self::resize_terminal). An unknown discriminant drops it.
    pub fn terminal_mouse(&self, id: u64, kind: u8, button: u8, x: f64, y: f64, mods: u8) {
        let kind = match kind {
            terminal_mouse_kind::PRESS => TerminalMouseKind::Press,
            terminal_mouse_kind::RELEASE => TerminalMouseKind::Release,
            terminal_mouse_kind::MOTION => TerminalMouseKind::Motion,
            _ => return,
        };
        let button = match button {
            mouse_button::LEFT => Some(MouseButton::Left),
            mouse_button::RIGHT => Some(MouseButton::Right),
            mouse_button::MIDDLE => Some(MouseButton::Middle),
            mouse_button::NONE => None,
            _ => return,
        };
        if let Some(core) = self.core.borrow().as_ref() {
            core.terminal_mouse(
                TerminalId(id),
                TerminalMouseEvent {
                    kind,
                    button,
                    x: x as f32,
                    y: y as f32,
                    shift: mods & terminal_mods::SHIFT != 0,
                    ctrl: mods & terminal_mods::CTRL != 0,
                    alt: mods & terminal_mods::ALT != 0,
                },
            );
        }
    }

    /// A wheel scroll of `lines` over session `id`'s pane, `direction` a [`scroll_dir`]
    /// discriminant and the rest as [`terminal_mouse`](Self::terminal_mouse). An unknown
    /// direction drops it.
    pub fn terminal_scroll(&self, id: u64, direction: u8, lines: u16, x: f64, y: f64, mods: u8) {
        let direction = match direction {
            scroll_dir::UP => ScrollDirection::Up,
            scroll_dir::DOWN => ScrollDirection::Down,
            scroll_dir::LEFT => ScrollDirection::Left,
            scroll_dir::RIGHT => ScrollDirection::Right,
            _ => return,
        };
        if let Some(core) = self.core.borrow().as_ref() {
            core.terminal_scroll(
                TerminalId(id),
                TerminalScrollEvent {
                    direction,
                    lines,
                    x: x as f32,
                    y: y as f32,
                    shift: mods & terminal_mods::SHIFT != 0,
                    ctrl: mods & terminal_mods::CTRL != 0,
                    alt: mods & terminal_mods::ALT != 0,
                },
            );
        }
    }

    /// Reports session `id`'s pane gaining or losing focus, or the window's activation.
    /// A no-op once the core is shut down.
    pub fn terminal_focus(&self, id: u64, gained: bool) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.terminal_focus(TerminalId(id), gained);
        }
    }

    /// Sizes session `id` and its pty to `cols` by `rows` cells of `cell_w_px` by
    /// `cell_h_px` pixels. A no-op once the core is shut down.
    pub fn resize_terminal(&self, id: u64, cols: u16, rows: u16, cell_w_px: u32, cell_h_px: u32) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.resize_terminal(TerminalId(id), cols, rows, cell_w_px, cell_h_px);
        }
    }

    /// Hangs session `id` up. A no-op once the core is shut down.
    pub fn close_terminal(&self, id: u64) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.close_terminal(TerminalId(id));
        }
    }

    /// The running sessions' ids, ascending; empty once the core is shut down.
    #[must_use]
    pub fn live_terminals(&self) -> Vec<u64> {
        self.core.borrow().as_ref().map_or_else(Vec::new, |core| {
            core.live_terminals().into_iter().map(|id| id.0).collect()
        })
    }

    /// The text of session `id`'s selection, for Copy; empty when nothing is selected,
    /// for an unknown id, or once the core is shut down.
    #[must_use]
    pub fn terminal_selection_text(&self, id: u64) -> String {
        self.core
            .borrow()
            .as_ref()
            .and_then(|core| core.terminal_selection_text(TerminalId(id)))
            .unwrap_or_default()
    }
}

/// Starts the core. `init_lua` and `cwd` are filesystem paths (mirroring
/// [`CoreConfig`]). Returns `None` (after logging) on any startup error.
///
/// The terminfo directory comes from `WOLF_TERMINFO_DIR`, which `TabCore.swift` defaults to
/// the bundle's `Contents/Resources/terminfo`; see [`wolf_core::terminfo_dir_from_env`].
///
/// A fresh wake token is minted (via `NEXT_WAKE_TOKEN`) and captured by the
/// core's [`WakeFn`] so wakes route back to THIS tab. State persistence is not
/// wired per-core: the coordinator walks the whole window/tab tree and
/// commits it via the app-state builder free fns (see
/// [`wolf_ffi_app_state_begin`]).
#[must_use]
pub fn wolf_ffi_start(
    init_lua: String,
    cwd: String,
    cols: u16,
    rows: u16,
) -> Option<WolfCoreHandle> {
    wolf_core::init_tracing();
    let cfg = CoreConfig {
        init_lua: PathBuf::from(init_lua),
        cols,
        rows,
        cwd: PathBuf::from(cwd),
        terminfo_dir: wolf_core::terminfo_dir_from_env(),
    };
    let wake_token = NEXT_WAKE_TOKEN.fetch_add(1, Ordering::Relaxed);
    match WolfCore::start(cfg, registered_wake(wake_token)) {
        Ok(core) => Some(WolfCoreHandle {
            core: RefCell::new(Some(core)),
            wake_token,
        }),
        Err(e) => {
            tracing::error!("wolf-ffi: WolfCore::start failed: {e}");
            None
        }
    }
}

// --- FfiCell: one resolved grid cell (opaque to Swift) ---

/// One resolved grid cell: its text, foreground/background, and style flags.
///
/// Opaque across the bridge (so a `Vec<FfiCell>` is supported — swift-bridge does
/// not emit Vec support for transparent structs). Colors are held as
/// `Option<Rgb>` (keeping the cell `Clone`); Swift reads them via [`FfiCell::fg`]
/// / [`FfiCell::bg`] / [`FfiCell::special`], which build an [`FfiColor`] on demand.
/// The `text` field is `pub` so the Rust integration test can read `cell.text`
/// directly.
#[derive(Clone)]
pub struct FfiCell {
    /// The displayed text (one grapheme cluster), empty for a wide-char trailing
    /// placeholder or a never-painted cell.
    pub text: String,
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    special: Option<Rgb>,
    flags: u8,
}

impl FfiCell {
    /// The cell's text (one grapheme cluster), empty for a placeholder cell.
    #[must_use]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    /// The resolved foreground color.
    #[must_use]
    pub fn fg(&self) -> FfiColor {
        FfiColor::from_opt(self.fg)
    }

    /// The resolved background color.
    #[must_use]
    pub fn bg(&self) -> FfiColor {
        FfiColor::from_opt(self.bg)
    }

    /// The resolved special (underline) color.
    #[must_use]
    pub fn special(&self) -> FfiColor {
        FfiColor::from_opt(self.special)
    }

    /// The packed style flags (bold/italic/underline/undercurl/strikethrough).
    #[must_use]
    pub fn flags(&self) -> u8 {
        self.flags
    }
}

// --- FfiLayoutIntent: a drained layout intent (opaque to Swift) ---

/// One drained layout intent (opaque to Swift, so a `Vec<FfiLayoutIntent>` is
/// supported — swift-bridge emits Vec support only for opaque types).
///
/// Flattens [`wolf_core::LayoutIntent`]'s variants into a tagged struct: `kind`
/// discriminates (`intent_kind::SPLIT` / `FOCUS_DIR` / `DRAWER_OPEN`); a SPLIT
/// carries `orientation` + `from` + `win`; a FOCUS_DIR carries `dir` + `from`; a
/// DRAWER_OPEN carries just `win` (the unused scalar fields are 0).
pub struct FfiLayoutIntent {
    kind: u8,
    orientation: u8,
    dir: u8,
    from: i64,
    win: i64,
}

impl FfiLayoutIntent {
    /// The intent kind (`intent_kind::SPLIT` / `FOCUS_DIR` / `DRAWER_OPEN`).
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.kind
    }

    /// For a SPLIT: the orientation (`orientation::HORIZONTAL` / `VERTICAL`); 0 for
    /// a FOCUS_DIR / DRAWER_OPEN.
    #[must_use]
    pub fn orientation(&self) -> u8 {
        self.orientation
    }

    /// For a FOCUS_DIR: the direction (`focus_dir::LEFT` / `RIGHT` / `UP` /
    /// `DOWN`); 0 for a SPLIT / DRAWER_OPEN.
    #[must_use]
    pub fn dir(&self) -> u8 {
        self.dir
    }

    /// The origin window handle (current before the split / when the chord fired);
    /// 0 for a DRAWER_OPEN.
    #[must_use]
    pub fn from(&self) -> i64 {
        self.from
    }

    /// For a SPLIT: the new window handle; for a DRAWER_OPEN: the new drawer window
    /// handle; 0 for a FOCUS_DIR.
    #[must_use]
    pub fn win(&self) -> i64 {
        self.win
    }

    /// Flattens a [`wolf_core::LayoutIntent`] into the bridge-friendly tagged
    /// struct, packing the discriminants from the [`intent_kind`] / [`orientation`]
    /// / [`focus_dir`] modules.
    fn from_intent(intent: LayoutIntent) -> Self {
        match intent {
            LayoutIntent::Split {
                orientation: o,
                from,
                win,
            } => Self {
                kind: intent_kind::SPLIT,
                orientation: match o {
                    SplitOrientation::Horizontal => orientation::HORIZONTAL,
                    SplitOrientation::Vertical => orientation::VERTICAL,
                },
                dir: 0,
                from,
                win,
            },
            LayoutIntent::FocusDir { dir, from } => Self {
                kind: intent_kind::FOCUS_DIR,
                orientation: 0,
                dir: match dir {
                    FocusDir::Left => focus_dir::LEFT,
                    FocusDir::Right => focus_dir::RIGHT,
                    FocusDir::Up => focus_dir::UP,
                    FocusDir::Down => focus_dir::DOWN,
                },
                from,
                win: 0,
            },
            LayoutIntent::DrawerOpen { win } => Self {
                kind: intent_kind::DRAWER_OPEN,
                orientation: 0,
                dir: 0,
                from: 0,
                win,
            },
        }
    }
}

// --- FfiCommand: one drained user-triggered command (opaque to Swift) ---

/// One drained command (opaque to Swift, so a `Vec<FfiCommand>` is supported —
/// swift-bridge emits Vec support only for opaque types).
///
/// Carries just the `command_kind` discriminant of a [`wolf_core::Command`];
/// the front-end maps it to a native action.
pub struct FfiCommand {
    kind: u8,
}

impl FfiCommand {
    /// The command kind, a `command_kind` constant.
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.kind
    }

    /// Packs a [`wolf_core::Command`] into the bridge-friendly discriminant from
    /// the [`command_kind`] module. The exhaustive match makes a new variant a compile
    /// error until it has a constant; a reorder is not one, and fails `tests/commands.rs`.
    fn from_command(command: Command) -> Self {
        let kind = match command {
            Command::NewWindow => command_kind::NEW_WINDOW,
            Command::CloseWindow => command_kind::CLOSE_WINDOW,
            Command::QuitApplication => command_kind::QUIT_APPLICATION,
            Command::HideApplication => command_kind::HIDE_APPLICATION,
            Command::MinimizeWindow => command_kind::MINIMIZE_WINDOW,
            Command::GoToFile => command_kind::GO_TO_FILE,
            Command::OpenFile => command_kind::OPEN_FILE,
            Command::OpenFolder => command_kind::OPEN_FOLDER,
            Command::ClosePane => command_kind::CLOSE_PANE,
            Command::NewBuffer => command_kind::NEW_BUFFER,
            Command::ResetFontSize => command_kind::RESET_FONT_SIZE,
            Command::IncreaseFontSize => command_kind::INCREASE_FONT_SIZE,
            Command::DecreaseFontSize => command_kind::DECREASE_FONT_SIZE,
            Command::CommandPalette => command_kind::COMMAND_PALETTE,
            Command::Copy => command_kind::COPY,
            Command::Paste => command_kind::PASTE,
            Command::BufferPicker => command_kind::BUFFER_PICKER,
            Command::NewTab => command_kind::NEW_TAB,
            Command::Preview => command_kind::PREVIEW,
            Command::Edit => command_kind::EDIT,
            Command::Terminal => command_kind::TERMINAL,
            Command::PreviewInBrowser => command_kind::PREVIEW_IN_BROWSER,
            Command::FullWidth => command_kind::FULL_WIDTH,
            Command::ToggleSidebar => command_kind::TOGGLE_SIDEBAR,
            Command::SidebarShowProject => command_kind::SIDEBAR_SHOW_PROJECT,
            Command::SearchInProject => command_kind::SEARCH_IN_PROJECT,
            Command::PreviewInOtherPane => command_kind::PREVIEW_IN_OTHER_PANE,
            Command::EditInOtherPane => command_kind::EDIT_IN_OTHER_PANE,
            Command::SplitHorizontal => command_kind::SPLIT_HORIZONTAL,
            Command::SplitVertical => command_kind::SPLIT_VERTICAL,
            Command::ZoomPane => command_kind::ZOOM_PANE,
            Command::CloseTab => command_kind::CLOSE_TAB,
        };
        Self { kind }
    }
}

/// Returns the number of registered commands, [`wolf_core::COMMAND_SPECS`]'s length,
/// which is one past the highest [`FfiCommand::kind`].
///
/// A free FFI fn (no live core required). Swift reads it to check that `CommandKind`
/// decodes every command core can drain, and nothing past them.
///
/// # Panics
///
/// When the registry outgrows a `u8`, which the `command_kind` discriminants could not
/// carry either.
#[must_use]
pub fn wolf_ffi_command_count() -> u8 {
    u8::try_from(wolf_core::COMMAND_SPECS.len()).expect("COMMAND_SPECS outgrew a u8 discriminant")
}

/// Returns the [`FfiCommand::kind`] that the command with registry id `id` drains as,
/// or `u8::MAX` when no registered command has that id.
///
/// A free FFI fn (no live core required), so a test can read Rust's discriminant for
/// an id without starting Neovim.
#[must_use]
pub fn wolf_ffi_command_kind(id: String) -> u8 {
    Command::from_id(&id).map_or(u8::MAX, |command| FfiCommand::from_command(command).kind())
}

// --- FfiCommandPalette / FfiCommandRow: the command-palette session (opaque to Swift) ---

/// A command-palette session (opaque to Swift).
///
/// Assembled at [`WolfCoreHandle::command_palette`] from the core's recent dirs +
/// global command MRU; the front-end appends each OTHER open window via
/// [`add_window`](Self::add_window) and filters per keystroke via
/// [`ranked`](Self::ranked). Like [`WolfCoreHandle`] this is a MAIN-THREAD-ONLY
/// handle: the `RefCell<Vec<PaletteWindow>>` gives interior mutability for
/// `add_window` behind the bridge's `&self`, and is only ever touched on the
/// AppKit main thread.
pub struct FfiCommandPalette {
    /// The core's recent directories at construction (the "Open Recent Folder"
    /// candidate source).
    recent_dirs: Vec<PathBuf>,
    /// The OTHER open windows the front-end has appended (the "Switch to Window"
    /// candidate source).
    windows: RefCell<Vec<PaletteWindow>>,
    /// The global command MRU (read for MRU-first ranking; written by
    /// [`record`](Self::record)).
    command_mru: CommandMru,
}

impl FfiCommandPalette {
    /// Appends one OTHER open window (the current window is excluded by the
    /// caller): `index` is the raise-target front-end window index; `cwd` is its
    /// working directory (`""` renders as "Untitled").
    pub fn add_window(&self, index: u16, cwd: String) {
        self.windows.borrow_mut().push(PaletteWindow { index, cwd });
    }

    /// The top `max` ranked palette rows for `query`, each mapped into an opaque
    /// [`FfiCommandRow`].
    ///
    /// Clones the appended window list out of the `RefCell` before delegating to
    /// [`wolf_core::rank_palette`] (so the borrow is not held across the call),
    /// which assembles the static registry commands (minus CommandPalette) + the
    /// windows + the recent dirs and ranks them MRU-first.
    #[must_use]
    pub fn ranked(&self, query: String, max: usize) -> Vec<FfiCommandRow> {
        let windows = self.windows.borrow().clone();
        let mru_recent = self.command_mru.recent();
        rank_palette(&self.recent_dirs, &windows, &mru_recent, &query, max)
            .into_iter()
            .map(FfiCommandRow::from_row)
            .collect()
    }

    /// Records `key` (a row's `mru_key`) into the global command MRU, so it floats
    /// to the top of a later `ranked("")`.
    pub fn record(&self, key: String) {
        self.command_mru.record(&key);
    }
}

/// One ranked palette row (opaque to Swift, so a `Vec<FfiCommandRow>` is supported
/// — swift-bridge emits Vec support only for opaque types).
///
/// A projection of [`wolf_core::PaletteRow`]: the kind-specific payload is
/// flattened into scalar/String accessors (`command_kind` for a static row,
/// `window_index` for a switch-window row, `path` for an open-recent row; the
/// unused fields are 0 / `""`).
pub struct FfiCommandRow {
    display: String,
    row_kind: u8,
    command_kind: u8,
    window_index: u16,
    path: String,
    mru_key: String,
}

impl FfiCommandRow {
    /// The label shown in the palette (and ranked against the query).
    #[must_use]
    pub fn display(&self) -> String {
        self.display.clone()
    }

    /// The row kind (`palette_row_kind::STATIC` / `SWITCH_WINDOW` / `OPEN_RECENT`).
    #[must_use]
    pub fn row_kind(&self) -> u8 {
        self.row_kind
    }

    /// For a STATIC row: the `command_kind` discriminant to dispatch; 0 for a
    /// dynamic row.
    #[must_use]
    pub fn command_kind(&self) -> u8 {
        self.command_kind
    }

    /// For a SWITCH_WINDOW row: the raise-target window index; 0 otherwise.
    #[must_use]
    pub fn window_index(&self) -> u16 {
        self.window_index
    }

    /// For an OPEN_RECENT row: the recent-folder path; `""` otherwise.
    #[must_use]
    pub fn path(&self) -> String {
        self.path.clone()
    }

    /// The opaque MRU key handed back to [`FfiCommandPalette::record`] on
    /// activation.
    #[must_use]
    pub fn mru_key(&self) -> String {
        self.mru_key.clone()
    }

    /// Projects a [`wolf_core::PaletteRow`] into the bridge-friendly flat row,
    /// packing the discriminants from the [`palette_row_kind`] / [`command_kind`]
    /// modules (the latter reused via [`FfiCommand::from_command`] for a single
    /// source of truth). Dynamic rows carry `command_kind == 0` and a `""` path.
    fn from_row(row: PaletteRow) -> Self {
        let row_kind = match row.kind {
            PaletteRowKind::Static => palette_row_kind::STATIC,
            PaletteRowKind::SwitchWindow => palette_row_kind::SWITCH_WINDOW,
            PaletteRowKind::OpenRecent => palette_row_kind::OPEN_RECENT,
        };
        let command_kind = row
            .command
            .map_or(0, |c| FfiCommand::from_command(c).kind());
        let path = row
            .path
            .as_ref()
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned());
        Self {
            display: row.display,
            row_kind,
            command_kind,
            window_index: row.window_index.unwrap_or(0),
            path,
            mru_key: row.mru_key,
        }
    }
}

// --- FfiBufferPicker / FfiBufferRow: the buffer-picker session (opaque to Swift) ---

/// A buffer-picker session (opaque to Swift).
///
/// Assembled at [`WolfCoreHandle::buffer_picker`] from a blocking snapshot of the
/// core's listed buffers plus the live cwd; the front-end filters per keystroke
/// via [`ranked`](Self::ranked). Like [`WolfCoreHandle`] this is a
/// MAIN-THREAD-ONLY handle. Unlike the command palette it takes no per-keystroke
/// window feed and no MRU write-back — the candidate set is fixed at construction.
pub struct FfiBufferPicker {
    /// The listed buffers snapshotted at construction (the candidate source).
    buffers: Vec<BufInfo>,
    /// The live cwd at construction (used to relativize each buffer's display).
    cwd: Option<PathBuf>,
}

impl FfiBufferPicker {
    /// The top `max` ranked buffer rows for `query`, each mapped into an opaque
    /// [`FfiBufferRow`].
    ///
    /// Delegates to [`wolf_core::rank_buffers`], which MRU-orders the buffers
    /// (lastused-DESC) and ranks them best-first for the query.
    #[must_use]
    pub fn ranked(&self, query: String, max: usize) -> Vec<FfiBufferRow> {
        rank_buffers(&self.buffers, self.cwd.as_deref(), &query, max)
            .into_iter()
            .map(FfiBufferRow::from_row)
            .collect()
    }
}

/// One ranked buffer row (opaque to Swift, so a `Vec<FfiBufferRow>` is supported
/// — swift-bridge emits Vec support only for opaque types).
///
/// A projection of [`wolf_core::BufferRow`]: the buffer handle, its display label,
/// and its modified flag flattened into scalar/String accessors.
pub struct FfiBufferRow {
    bufnr: i64,
    display: String,
    modified: bool,
}

impl FfiBufferRow {
    /// The Neovim buffer handle to switch to (passed to
    /// [`WolfCoreHandle::switch_buffer`]).
    #[must_use]
    pub fn bufnr(&self) -> i64 {
        self.bufnr
    }

    /// The cwd-relative (or absolute / `[No Name]`) display label, also the string
    /// ranked against the query.
    #[must_use]
    pub fn display(&self) -> String {
        self.display.clone()
    }

    /// Whether the buffer has unsaved modifications (for the row chrome).
    #[must_use]
    pub fn modified(&self) -> bool {
        self.modified
    }

    /// Projects a [`wolf_core::BufferRow`] into the bridge-friendly flat row.
    fn from_row(row: BufferRow) -> Self {
        Self {
            bufnr: row.bufnr,
            display: row.display,
            modified: row.modified,
        }
    }
}

// --- FfiWinInfo: one window's grid + handle + hidden flag (opaque to Swift) ---

/// One window's `(grid, win, hidden)` triple (opaque to Swift, so a
/// `Vec<FfiWinInfo>` is supported — swift-bridge emits Vec support only for opaque
/// types). A projection of [`wolf_core::WinInfo`]'s pane-relevant fields.
pub struct FfiWinInfo {
    grid: u64,
    win: i64,
    hidden: bool,
}

impl FfiWinInfo {
    /// The grid id this window draws into (`>= 2` for editor windows).
    #[must_use]
    pub fn grid(&self) -> u64 {
        self.grid
    }

    /// The Neovim window handle, as an `i64`.
    #[must_use]
    pub fn win(&self) -> i64 {
        self.win
    }

    /// Whether the window is currently hidden.
    #[must_use]
    pub fn hidden(&self) -> bool {
        self.hidden
    }

    /// Projects a [`wolf_core::WinInfo`]'s pane-relevant fields.
    fn from_win_info(info: &WinInfo) -> Self {
        Self {
            grid: info.grid,
            win: info.win,
            hidden: info.hidden,
        }
    }
}

// --- FfiPreview: one open preview pane's window + source + tick (opaque to Swift) ---

/// One open preview pane (opaque to Swift, so a `Vec<FfiPreview>` is supported —
/// swift-bridge emits Vec support only for opaque types).
///
/// A projection of one [`wolf_core::render::Snapshot::previews`] entry joined to
/// its source buffer's live `changedtick`: the preview WINDOW handle, the SOURCE
/// Markdown buffer's number + absolute path (+ basename for the pane statusbar),
/// and the tick the front-end reloads the web view on.
#[derive(Debug)]
pub struct FfiPreview {
    win: i64,
    source_bufnr: i64,
    source_path: String,
    source_name: String,
    source_display_path: String,
    source_tick: u64,
    source_full_width: bool,
    source_cursor_line: u32,
    source_cursor_col: u32,
}

impl FfiPreview {
    /// The preview WINDOW handle (the pane the front-end overlays a web view on).
    #[must_use]
    pub fn win(&self) -> i64 {
        self.win
    }

    /// The SOURCE Markdown buffer's number (passed back to
    /// [`WolfCoreHandle::render_preview`]).
    #[must_use]
    pub fn source_bufnr(&self) -> i64 {
        self.source_bufnr
    }

    /// The source file's BASENAME (e.g. `"alpha.md"`), for the `Preview: <name>`
    /// pane statusbar.
    #[must_use]
    pub fn source_name(&self) -> String {
        self.source_name.clone()
    }

    /// The source file's absolute path.
    #[must_use]
    pub fn source_path(&self) -> String {
        self.source_path.clone()
    }

    /// The source file's cwd/home-relative DISPLAY path (e.g.
    /// `"crates/wolf-macos/README.md"`), for the preview pane's secondary
    /// statusbar line. Computed in [`WolfCoreHandle::previews`] via
    /// [`WolfCore::display_path`](wolf_core::WolfCore::display_path); empty for a
    /// pathless source.
    #[must_use]
    pub fn source_display_path(&self) -> String {
        self.source_display_path.clone()
    }

    /// The source buffer's current `changedtick` (`0` until the first
    /// `wolf_markdown_dirty`); the front-end reloads the web view when it advances.
    #[must_use]
    pub fn source_tick(&self) -> u64 {
        self.source_tick
    }

    /// Whether the SOURCE Markdown buffer is in FULL-WIDTH mode
    /// ([`PreviewInfo::source_full_width`]). The front-end lets this preview's content
    /// fill the inset pane when `true`, or caps and centres it at ~90 characters inside
    /// the page when `false`.
    #[must_use]
    pub fn source_full_width(&self) -> bool {
        self.source_full_width
    }

    /// The SOURCE buffer's cursor LINE — 1-based, matching the `wl-{L}-{C}` position
    /// markers `wolf_core::markdown` emits into the preview HTML. `0` until the
    /// first `wolf_markdown_cursor`, which the front-end reads as "cursor unknown"
    /// and does NOT scroll for.
    #[must_use]
    pub fn source_cursor_line(&self) -> u32 {
        self.source_cursor_line
    }

    /// The SOURCE buffer's cursor COLUMN — a 0-based CHARACTER column (NOT a byte
    /// column), matching the marker scheme's `C`. The front-end rounds it DOWN to a
    /// multiple of `wolf_core::markdown::MARKER_COLUMN_STRIDE` to build the anchor
    /// id. `0` until the first `wolf_markdown_cursor`.
    #[must_use]
    pub fn source_cursor_col(&self) -> u32 {
        self.source_cursor_col
    }

    /// Projects one [`PreviewInfo`] (keyed by the preview window `win`) plus its
    /// joined `source_tick`, its joined `cursor`, and its cwd/home-relative
    /// `source_display_path` (computed by the caller via
    /// [`WolfCore::display_path`](wolf_core::WolfCore::display_path)) into the
    /// bridge-friendly flat row. The basename is derived from `source_path`'s final
    /// component (empty for a pathless source).
    fn from_preview(
        win: i64,
        info: &PreviewInfo,
        source_tick: u64,
        source_display_path: String,
        cursor: MarkdownCursor,
    ) -> Self {
        let source_name = info
            .source_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            win,
            source_bufnr: info.source_bufnr,
            source_path: info.source_path.to_string_lossy().into_owned(),
            source_name,
            source_display_path,
            source_tick,
            source_full_width: info.source_full_width,
            source_cursor_line: cursor.line,
            source_cursor_col: cursor.col,
        }
    }
}

// --- FfiHistoryEntry: one History-menu row (opaque to Swift) ---

/// One rendered History-menu row (opaque to Swift, so a `Vec<FfiHistoryEntry>` is
/// supported — swift-bridge emits Vec support only for opaque types, exactly like
/// [`FfiPreview`]).
///
/// A projection of one [`wolf_core::MenuRow`]: the finished, disambiguated display
/// label, the STACK index to hand back to
/// [`WolfCoreHandle::history_jump`](WolfCoreHandle::history_jump), and whether this
/// row draws the checkmark.
#[derive(Debug)]
pub struct FfiHistoryEntry {
    display: String,
    index: u32,
    is_current: bool,
}

impl FfiHistoryEntry {
    /// The disambiguated menu label (e.g. `"bar/README.md"`,
    /// `"Preview: alpha.md"`, `"Terminal 2"`). Already home-abbreviated to `~`;
    /// the front-end draws it verbatim.
    #[must_use]
    pub fn display(&self) -> String {
        self.display.clone()
    }

    /// The STACK index to hand back to
    /// [`WolfCoreHandle::history_jump`](WolfCoreHandle::history_jump). For the
    /// checkmarked row this is the CURSOR's own index, so clicking it is a no-op.
    #[must_use]
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Whether this row draws the checkmark. At most one row per menu, and NONE
    /// when the cursor's target fell outside the 10-distinct-row cap.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.is_current
    }

    /// Projects one [`wolf_core::MenuRow`] into the bridge-friendly flat row.
    ///
    /// The `usize` stack index narrows via `u32::try_from` (never `as`, per
    /// `clippy::cast_possible_truncation`); an index that cannot fit — impossible,
    /// the stack is capped at 100 — degrades to `0`.
    fn from_menu_row(row: wolf_core::MenuRow) -> Self {
        Self {
            display: row.display,
            index: u32::try_from(row.index).unwrap_or(0),
            is_current: row.is_current,
        }
    }
}

/// The Tier-1 open-file mirror entry for one tabpage, opaque to Swift.
///
/// Built by [`WolfCoreHandle::open_files_for_tabpage`] from
/// [`wolf_core::TabFiles`], as owned Strings. The front-end captures this per view
/// BEFORE a restart and reopens `files` afterward (ACTIVE last so it lands
/// focused).
pub struct FfiOpenFiles {
    active: String,
    files: Vec<String>,
}

impl FfiOpenFiles {
    /// The tabpage's active buffer's absolute path, or `""` for an unnamed buffer.
    #[must_use]
    pub fn active_path(&self) -> String {
        self.active.clone()
    }

    /// The instance-global buflisted file-backed buffer paths (absolute) — the
    /// same list in every tabpage's entry, not the tabpage's own windows.
    #[must_use]
    pub fn files(&self) -> Vec<String> {
        self.files.clone()
    }

    /// Projects an optional [`wolf_core::TabFiles`] into the opaque bridge type
    /// (an empty-backed value for `None` — a gone core or unknown tabpage).
    fn from_tab_files(mirror: Option<TabFiles>) -> Self {
        match mirror {
            Some(tf) => Self {
                active: tf
                    .active
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                files: tf
                    .files
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect(),
            },
            None => Self {
                active: String::new(),
                files: Vec::new(),
            },
        }
    }
}

// --- FfiPaneStatus: one pane's statusbar strings (opaque to Swift) ---

/// One pane's statusbar strings — the base `filename` and the `display_path`
/// beneath it (opaque to Swift, so its Strings are read back via accessor
/// methods). A projection of [`wolf_core::PaneStatus`].
pub struct FfiPaneStatus {
    filename: String,
    display_path: String,
    abs_path: String,
}

impl FfiPaneStatus {
    /// The pane's [`PaneStatus::filename`].
    #[must_use]
    pub fn filename(&self) -> String {
        self.filename.clone()
    }

    /// The pane's [`PaneStatus::display_path`].
    #[must_use]
    pub fn display_path(&self) -> String {
        self.display_path.clone()
    }

    /// The buffer's raw ABSOLUTE path (never relativized; empty for an unnamed
    /// buffer). The macOS front-end persists it as a pane-layout leaf path.
    #[must_use]
    pub fn abs_path(&self) -> String {
        self.abs_path.clone()
    }

    /// The empty-status fallback (`[No Name]` filename, empty display + abs path),
    /// used when the core has been shut down.
    fn empty() -> Self {
        Self::from_pane_status(PaneStatus {
            filename: wolf_core::NO_NAME.to_string(),
            display_path: String::new(),
            abs_path: String::new(),
        })
    }

    /// Projects a [`wolf_core::PaneStatus`] into the opaque bridge type.
    fn from_pane_status(status: PaneStatus) -> Self {
        Self {
            filename: status.filename,
            display_path: status.display_path,
            abs_path: status.abs_path,
        }
    }
}

// --- FfiWinBuffer: what one window shows, by handle (opaque to Swift) ---

/// What one window shows, as [`WolfCoreHandle::win_buffer`] reads it from core's
/// `win_buffers` map. A projection of [`wolf_core::WinBuffer`]'s path and session.
pub struct FfiWinBuffer {
    reported: bool,
    abs_path: String,
    terminal: u64,
}

impl FfiWinBuffer {
    /// Whether core's `win_buffers` map has the window.
    #[must_use]
    pub fn reported(&self) -> bool {
        self.reported
    }

    /// The buffer's absolute, canonical path; empty for an unnamed buffer and when
    /// the window is not reported.
    #[must_use]
    pub fn abs_path(&self) -> String {
        self.abs_path.clone()
    }

    /// The id of the terminal session the window shows, which makes its pane a terminal
    /// pane; `0` for none and when the window is not reported.
    #[must_use]
    pub fn terminal(&self) -> u64 {
        self.terminal
    }

    /// The answer for an unknown window or a shut-down core.
    fn unreported() -> Self {
        Self {
            reported: false,
            abs_path: String::new(),
            terminal: 0,
        }
    }
}

// --- FfiWindowCursor: one window's cursor and top line (opaque to Swift) ---

/// One window's cursor and top line, as [`WolfCoreHandle::window_cursor`] reads them.
/// A projection of [`wolf_core::WindowCursor`], with `found` for its `None`.
#[derive(Default)]
pub struct FfiWindowCursor {
    found: bool,
    line: u32,
    col: u32,
    top_line: u32,
}

impl FfiWindowCursor {
    /// Whether the window was found; every other field is `0` when not.
    #[must_use]
    pub fn found(&self) -> bool {
        self.found
    }

    /// The 1-based cursor line.
    #[must_use]
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The 0-based byte column of the cursor.
    #[must_use]
    pub fn col(&self) -> u32 {
        self.col
    }

    /// The 1-based first visible line.
    #[must_use]
    pub fn top_line(&self) -> u32 {
        self.top_line
    }
}

// --- FfiTerminalFrame: one session's published frame (opaque to Swift) ---

/// One terminal session's published frame, as [`WolfCoreHandle::terminal_frame`] reads
/// it, with flat per-row accessors mirroring the grid read path. Shares core's
/// [`TerminalFrame`] rather than copying its cells. Every accessor answers `0`, `false`,
/// `""` or an unset colour out of range and on the empty frame.
pub struct FfiTerminalFrame {
    frame: Option<Arc<TerminalFrame>>,
}

impl FfiTerminalFrame {
    /// The frame's version, monotonic per session; `0` for the empty frame.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.frame.as_ref().map_or(0, |f| f.version)
    }

    /// The width in cells.
    #[must_use]
    pub fn cols(&self) -> u16 {
        self.frame.as_ref().map_or(0, |f| f.cols)
    }

    /// The height in cells.
    #[must_use]
    pub fn rows(&self) -> u16 {
        self.frame.as_ref().map_or(0, |f| f.rows)
    }

    /// The frame version at which row `row` last changed.
    #[must_use]
    pub fn row_version(&self, row: u16) -> u64 {
        self.frame
            .as_ref()
            .and_then(|f| f.row_versions.get(usize::from(row)).copied())
            .unwrap_or(0)
    }

    /// Row `row`'s cells, left to right; empty out of range.
    #[must_use]
    pub fn row_cells(&self, row: u16) -> Vec<FfiTerminalCell> {
        let Some(f) = self.frame.as_ref().filter(|f| row < f.rows) else {
            return Vec::new();
        };
        let cols = usize::from(f.cols);
        let start = usize::from(row) * cols;
        f.cells
            .get(start..start + cols)
            .unwrap_or_default()
            .iter()
            .map(FfiTerminalCell::from_cell)
            .collect()
    }

    /// Row `row`'s selected columns, inclusive.
    fn selection(&self, row: u16) -> Option<(u16, u16)> {
        self.frame
            .as_ref()
            .and_then(|f| f.selection.get(usize::from(row)).copied().flatten())
    }

    /// Whether row `row` has selected columns.
    #[must_use]
    pub fn row_has_selection(&self, row: u16) -> bool {
        self.selection(row).is_some()
    }

    /// Row `row`'s first selected column.
    #[must_use]
    pub fn row_selection_start(&self, row: u16) -> u16 {
        self.selection(row).map_or(0, |(start, _)| start)
    }

    /// Row `row`'s last selected column, inclusive.
    #[must_use]
    pub fn row_selection_end(&self, row: u16) -> u16 {
        self.selection(row).map_or(0, |(_, end)| end)
    }

    /// The cursor, `None` while it is scrolled out of view.
    fn cursor(&self) -> Option<TerminalCursor> {
        self.frame.as_ref().and_then(|f| f.cursor)
    }

    /// Whether the frame has a cursor; `false` while it is scrolled out of view.
    #[must_use]
    pub fn has_cursor(&self) -> bool {
        self.cursor().is_some()
    }

    /// The cursor's column.
    #[must_use]
    pub fn cursor_col(&self) -> u16 {
        self.cursor().map_or(0, |c| c.col)
    }

    /// The cursor's row.
    #[must_use]
    pub fn cursor_row(&self) -> u16 {
        self.cursor().map_or(0, |c| c.row)
    }

    /// Whether the program shows the cursor.
    #[must_use]
    pub fn cursor_visible(&self) -> bool {
        self.cursor().is_some_and(|c| c.visible)
    }

    /// Whether the cursor blinks.
    #[must_use]
    pub fn cursor_blinking(&self) -> bool {
        self.cursor().is_some_and(|c| c.blinking)
    }

    /// The cursor's [`terminal_cursor_style`] discriminant.
    #[must_use]
    pub fn cursor_style(&self) -> u8 {
        self.cursor()
            .map_or(terminal_cursor_style::BLOCK, |c| match c.style {
                TerminalCursorStyle::Block => terminal_cursor_style::BLOCK,
                TerminalCursorStyle::Bar => terminal_cursor_style::BAR,
                TerminalCursorStyle::Underline => terminal_cursor_style::UNDERLINE,
                TerminalCursorStyle::BlockHollow => terminal_cursor_style::HOLLOW,
            })
    }

    /// The default foreground; unset only for the empty frame.
    #[must_use]
    pub fn default_fg(&self) -> FfiColor {
        FfiColor::from_opt(self.frame.as_ref().map(|f| f.default_fg))
    }

    /// The default background; unset only for the empty frame.
    #[must_use]
    pub fn default_bg(&self) -> FfiColor {
        FfiColor::from_opt(self.frame.as_ref().map(|f| f.default_bg))
    }

    /// The cursor colour; unset when the editor sets none.
    #[must_use]
    pub fn cursor_color(&self) -> FfiColor {
        FfiColor::from_opt(self.frame.as_ref().and_then(|f| f.cursor_color))
    }

    /// The scrollable area's height in rows.
    #[must_use]
    pub fn scrollbar_total(&self) -> u64 {
        self.frame.as_ref().map_or(0, |f| f.scrollbar.total)
    }

    /// The viewport's first row; `total - len` at the bottom.
    #[must_use]
    pub fn scrollbar_offset(&self) -> u64 {
        self.frame.as_ref().map_or(0, |f| f.scrollbar.offset)
    }

    /// The viewport's height in rows.
    #[must_use]
    pub fn scrollbar_len(&self) -> u64 {
        self.frame.as_ref().map_or(0, |f| f.scrollbar.len)
    }

    /// Whether the program tracks the mouse.
    #[must_use]
    pub fn mouse_tracking(&self) -> bool {
        self.frame.as_ref().is_some_and(|f| f.mouse_tracking)
    }

    /// The title the program set.
    #[must_use]
    pub fn title(&self) -> String {
        self.frame
            .as_ref()
            .map(|f| f.title.clone())
            .unwrap_or_default()
    }

    /// The directory the shell last reported; empty for none.
    #[must_use]
    pub fn pwd(&self) -> String {
        self.frame
            .as_ref()
            .and_then(|f| f.pwd.as_ref())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The basename of the session's program, such as `zsh`.
    #[must_use]
    pub fn shell(&self) -> String {
        self.frame
            .as_ref()
            .map(|f| f.shell.clone())
            .unwrap_or_default()
    }

    /// Whether the session's program has exited.
    #[must_use]
    pub fn has_exited(&self) -> bool {
        self.frame.as_ref().is_some_and(|f| f.exited.is_some())
    }

    /// The exit code, or 128 plus the signal for a signal death; `0` while running.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        self.frame.as_ref().and_then(|f| f.exited).unwrap_or(0)
    }
}

// --- FfiTerminalCell: one cell of a terminal frame (opaque to Swift) ---

/// One cell of an [`FfiTerminalFrame`], with its style packed into discriminants and
/// bits. Opaque so a `Vec<FfiTerminalCell>` crosses the bridge.
pub struct FfiTerminalCell {
    text: String,
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    flags: u8,
    underline: u8,
    width: u8,
}

impl FfiTerminalCell {
    /// The grapheme cluster; empty for an empty cell or a spacer.
    #[must_use]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    /// The foreground, unset for the frame's default. Not swapped for inverse.
    #[must_use]
    pub fn fg(&self) -> FfiColor {
        FfiColor::from_opt(self.fg)
    }

    /// The background, unset for the frame's default. Not swapped for inverse.
    #[must_use]
    pub fn bg(&self) -> FfiColor {
        FfiColor::from_opt(self.bg)
    }

    /// The [`terminal_flag`] bits.
    #[must_use]
    pub fn flags(&self) -> u8 {
        self.flags
    }

    /// The [`terminal_underline`] discriminant.
    #[must_use]
    pub fn underline(&self) -> u8 {
        self.underline
    }

    /// The [`terminal_cell_width`] discriminant.
    #[must_use]
    pub fn width(&self) -> u8 {
        self.width
    }

    /// Packs one of core's cells.
    fn from_cell(cell: &TerminalCell) -> Self {
        let bits = [
            (cell.bold, terminal_flag::BOLD),
            (cell.faint, terminal_flag::FAINT),
            (cell.italic, terminal_flag::ITALIC),
            (cell.strikethrough, terminal_flag::STRIKETHROUGH),
            (cell.inverse, terminal_flag::INVERSE),
            (cell.invisible, terminal_flag::INVISIBLE),
            (cell.blink, terminal_flag::BLINK),
        ];
        Self {
            text: cell.text.clone(),
            fg: cell.fg,
            bg: cell.bg,
            flags: bits
                .iter()
                .filter(|(on, _)| *on)
                .fold(0, |acc, (_, bit)| acc | bit),
            underline: match cell.underline {
                TerminalUnderline::None => terminal_underline::NONE,
                TerminalUnderline::Single => terminal_underline::SINGLE,
                TerminalUnderline::Double => terminal_underline::DOUBLE,
                TerminalUnderline::Curly => terminal_underline::CURLY,
                TerminalUnderline::Dotted => terminal_underline::DOTTED,
                TerminalUnderline::Dashed => terminal_underline::DASHED,
            },
            width: match cell.width {
                TerminalCellWidth::Narrow => terminal_cell_width::NARROW,
                TerminalCellWidth::Wide => terminal_cell_width::WIDE,
                TerminalCellWidth::SpacerTail => terminal_cell_width::SPACER_TAIL,
                TerminalCellWidth::SpacerHead => terminal_cell_width::SPACER_HEAD,
            },
        }
    }
}

// --- FfiCmdline: the native command line (opaque to Swift) ---

/// The native command line (`ext_cmdline`), opaque to Swift.
///
/// A projection of [`wolf_core::Cmdline`] with its content chunks already
/// flattened to a single [`text`](FfiCmdline::text) String (the cmdline is
/// flattened across the FFI; only the toast keeps styled chunks). The front-end
/// paints `firstc` + `text` with the caret at `pos` when `active`.
pub struct FfiCmdline {
    active: bool,
    firstc: String,
    prompt: String,
    text: String,
    pos: u32,
    indent: u32,
    level: u32,
}

impl FfiCmdline {
    /// Whether a command line is currently showing.
    #[must_use]
    pub fn active(&self) -> bool {
        self.active
    }

    /// The first character of the command line (`:` / `/` / `?`), or `""`.
    #[must_use]
    pub fn firstc(&self) -> String {
        self.firstc.clone()
    }

    /// The prompt string for an input prompt (`input()`), or `""`.
    #[must_use]
    pub fn prompt(&self) -> String {
        self.prompt.clone()
    }

    /// The command-line text (content chunks flattened; without the `firstc`).
    #[must_use]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    /// The cursor position, as a BYTE offset into [`text`](Self::text).
    #[must_use]
    pub fn pos(&self) -> u32 {
        self.pos
    }

    /// The indent (from a continued `:function` block); usually 0.
    #[must_use]
    pub fn indent(&self) -> u32 {
        self.indent
    }

    /// The command-line recursion level (1 for a top-level command line).
    #[must_use]
    pub fn level(&self) -> u32 {
        self.level
    }

    /// The inactive default, used when no command line shows or the core is gone.
    fn empty() -> Self {
        Self {
            active: false,
            firstc: String::new(),
            prompt: String::new(),
            text: String::new(),
            pos: 0,
            indent: 0,
            level: 0,
        }
    }

    /// Projects a [`wolf_core::Cmdline`] into the opaque bridge type, flattening
    /// its content chunks to a single text String.
    fn from_cmdline(cmdline: &Cmdline) -> Self {
        Self {
            active: cmdline.active,
            firstc: cmdline.firstc.clone(),
            prompt: cmdline.prompt.clone(),
            text: cmdline.text(),
            pos: clamp_u32(cmdline.pos),
            indent: clamp_u32(cmdline.indent),
            level: cmdline.level,
        }
    }
}

// --- FfiPum / FfiPumItem: the completion popup (opaque to Swift) ---

/// The completion popup (`ext_popupmenu`), opaque to Swift.
///
/// A projection of [`wolf_core::Pum`]. When `anchor_grid == -1` the popup is
/// anchored to the command line and `anchor_col` is a BYTE offset into the
/// cmdline text (the front-end shows `word` primarily with `kind` as a small
/// trailing hint).
pub struct FfiPum {
    visible: bool,
    selected: i64,
    anchor_grid: i64,
    anchor_row: i64,
    anchor_col: i64,
    items: Vec<FfiPumItem>,
}

impl FfiPum {
    /// Whether the completion popup is currently showing.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// The selected item index, or `-1` for none.
    #[must_use]
    pub fn selected(&self) -> i64 {
        self.selected
    }

    /// The anchor grid (`-1` = anchored to the command line).
    #[must_use]
    pub fn anchor_grid(&self) -> i64 {
        self.anchor_grid
    }

    /// The anchor row.
    #[must_use]
    pub fn anchor_row(&self) -> i64 {
        self.anchor_row
    }

    /// The anchor column (a BYTE offset into the cmdline text when
    /// `anchor_grid == -1`).
    #[must_use]
    pub fn anchor_col(&self) -> i64 {
        self.anchor_col
    }

    /// The completion items.
    #[must_use]
    pub fn items(&self) -> Vec<FfiPumItem> {
        self.items.clone()
    }

    /// The hidden default, used when the popup is hidden or the core is gone.
    fn empty() -> Self {
        Self {
            visible: false,
            selected: -1,
            anchor_grid: 0,
            anchor_row: 0,
            anchor_col: 0,
            items: Vec::new(),
        }
    }

    /// Projects a [`wolf_core::Pum`] into the opaque bridge type.
    fn from_pum(pum: &Pum) -> Self {
        Self {
            visible: pum.visible,
            selected: pum.selected,
            anchor_grid: pum.grid,
            anchor_row: pum.row,
            anchor_col: pum.col,
            items: pum.items.iter().map(FfiPumItem::from_item).collect(),
        }
    }
}

/// One completion-popup item (opaque to Swift, so a `Vec<FfiPumItem>` is
/// supported — swift-bridge emits Vec support only for opaque types).
///
/// A projection of [`wolf_core::PumItem`].
#[derive(Clone)]
pub struct FfiPumItem {
    word: String,
    kind: String,
    menu: String,
    info: String,
}

impl FfiPumItem {
    /// The completion word (the primary label shown + inserted).
    #[must_use]
    pub fn word(&self) -> String {
        self.word.clone()
    }

    /// The single-letter kind hint (`f`, `v`, ...), or `""`.
    #[must_use]
    pub fn kind(&self) -> String {
        self.kind.clone()
    }

    /// Extra menu text, or `""`.
    #[must_use]
    pub fn menu(&self) -> String {
        self.menu.clone()
    }

    /// Documentation / preview text, or `""`.
    #[must_use]
    pub fn info(&self) -> String {
        self.info.clone()
    }

    /// Projects a [`wolf_core::PumItem`] into the opaque bridge type.
    fn from_item(item: &wolf_core::PumItem) -> Self {
        Self {
            word: item.word.clone(),
            kind: item.kind.clone(),
            menu: item.menu.clone(),
            info: item.info.clone(),
        }
    }
}

// --- FfiToast / FfiToastLine / FfiToastChunk: the message toast (opaque to Swift) ---

/// The message toast (`ext_messages`), opaque to Swift.
///
/// A projection of [`wolf_core::Toast`]. Each chunk's colors are RESOLVED in
/// Rust against the highlight table at pack time (message color is load-bearing
/// and must never be re-resolved in Swift).
pub struct FfiToast {
    visible: bool,
    kind: String,
    lines: Vec<FfiToastLine>,
}

impl FfiToast {
    /// Whether the toast is currently showing.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// The last message's kind (`emsg`, `echomsg`, `list_cmd`, ...), or `""`.
    #[must_use]
    pub fn kind(&self) -> String {
        self.kind.clone()
    }

    /// Every message's lines flattened in order.
    #[must_use]
    pub fn lines(&self) -> Vec<FfiToastLine> {
        self.lines.clone()
    }

    /// The hidden default, used when the toast is empty or the core is gone.
    fn empty() -> Self {
        Self {
            visible: false,
            kind: String::new(),
            lines: Vec::new(),
        }
    }

    /// Projects a [`wolf_core::Toast`] into the opaque bridge type, resolving
    /// each chunk's colors against `highlights` (the SAME table the frame uses).
    fn from_toast(toast: &Toast, highlights: &HighlightTable) -> Self {
        let lines = toast
            .lines()
            .iter()
            .map(|line| FfiToastLine::from_line(line, highlights))
            .collect();
        Self {
            visible: toast.visible,
            kind: toast.kind(),
            lines,
        }
    }
}

/// One toast line (opaque to Swift, so a `Vec<FfiToastLine>` is supported —
/// swift-bridge emits Vec support only for opaque types).
#[derive(Clone)]
pub struct FfiToastLine {
    chunks: Vec<FfiToastChunk>,
}

impl FfiToastLine {
    /// The styled chunks of this line.
    #[must_use]
    pub fn chunks(&self) -> Vec<FfiToastChunk> {
        self.chunks.clone()
    }

    /// Builds a line from a slice of [`wolf_core::ToastChunk`]s, resolving each
    /// chunk's colors against `highlights`.
    fn from_line(line: &[wolf_core::ToastChunk], highlights: &HighlightTable) -> Self {
        Self {
            chunks: line
                .iter()
                .map(|c| FfiToastChunk::from_chunk(c, highlights))
                .collect(),
        }
    }
}

/// One styled toast chunk (opaque to Swift, so a `Vec<FfiToastChunk>` is
/// supported). Its colors are RESOLVED in Rust against the highlight table (like
/// [`FfiCell`]); Swift reads them via [`fg`](Self::fg) / [`bg`](Self::bg) /
/// [`special`](Self::special).
#[derive(Clone)]
pub struct FfiToastChunk {
    text: String,
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    special: Option<Rgb>,
    flags: u8,
}

impl FfiToastChunk {
    /// The chunk's text.
    #[must_use]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    /// The resolved foreground color.
    #[must_use]
    pub fn fg(&self) -> FfiColor {
        FfiColor::from_opt(self.fg)
    }

    /// The resolved background color.
    #[must_use]
    pub fn bg(&self) -> FfiColor {
        FfiColor::from_opt(self.bg)
    }

    /// The resolved special (underline) color.
    #[must_use]
    pub fn special(&self) -> FfiColor {
        FfiColor::from_opt(self.special)
    }

    /// The packed style flags (bold/italic/underline/undercurl/strikethrough).
    #[must_use]
    pub fn flags(&self) -> u8 {
        self.flags
    }

    /// Projects a [`wolf_core::ToastChunk`] into the opaque bridge type,
    /// resolving its `attr_id` against `highlights` (like a cell).
    fn from_chunk(chunk: &wolf_core::ToastChunk, highlights: &HighlightTable) -> Self {
        let style = highlights.resolve(chunk.attr_id);
        Self {
            text: chunk.text.clone(),
            fg: style.fg,
            bg: style.bg,
            special: style.special,
            flags: pack_flags(&style),
        }
    }
}

// --- FfiFileSearch / FfiFileEntry: the file-picker search surface (opaque to Swift) ---

/// A live file-search session handle (opaque to Swift).
///
/// Wraps an `Option<`[`FileSearch`]`>` so
/// [`start_file_search`](WolfCoreHandle::start_file_search) stays infallible
/// across the bridge: `None` means the core was already gone at start, and
/// [`ranked`](FfiFileSearch::ranked) then yields no entries. Dropping this handle
/// drops the inner [`FileSearch`], cancelling its background traversal.
pub struct FfiFileSearch {
    search: Option<FileSearch>,
    /// A version-keyed cache of the per-directory [`TreeIndex`], so AppKit's
    /// numberOfChildren / child:index callback storm — many `children_of` calls
    /// per reload at a single snapshot version — rebuilds the index at most once
    /// per snapshot instead of re-scanning the flat entry list per call.
    tree_cache: RefCell<wolf_core::tree::TreeIndexCache>,
}

impl FfiFileSearch {
    /// The top `max` ranked entries for `query` against the latest streamed
    /// search snapshot.
    ///
    /// An empty (or all-whitespace) `query` lists all known entries MRU-first; a
    /// non-empty `query` is tokenized on whitespace into terms and keeps only
    /// entries where EVERY term is a case-insensitive subsequence (order-
    /// independent), best-first (see [`rank_matches_top_n`]). The snapshot read
    /// guard is held only for the duration of the rank + clone and dropped before
    /// returning.
    #[must_use]
    pub fn ranked(&self, query: String, max: usize) -> Vec<FfiFileEntry> {
        let Some(search) = &self.search else {
            return Vec::new();
        };
        let view = search.read();
        let entries: Vec<FfiFileEntry> = rank_matches_top_n(&query, &view.entries, max)
            .into_iter()
            .map(|m| {
                let entry = &view.entries[m.index];
                FfiFileEntry {
                    path: entry.path.clone(),
                    display: entry.display.clone(),
                }
            })
            .collect();
        entries
    }

    /// The IMMEDIATE children of `dir` (`""` = the cwd root) projected from the
    /// latest streamed search snapshot.
    ///
    /// Served from a per-snapshot [`wolf_core::tree::TreeIndex`] cached by
    /// [`TreeIndexCache`](wolf_core::tree::TreeIndexCache) keyed on the search
    /// snapshot's `version`: the index is rebuilt at most once per snapshot, and a
    /// per-directory lookup is then an O(children) borrowed-slice probe. Projection
    /// semantics are unchanged: directories-first then files, each case-insensitive
    /// by name; dotfiles are listed and gitignored files are excluded (both already
    /// handled by the streaming traversal). Yields an empty `Vec` when the session
    /// is absent.
    #[must_use]
    pub fn children_of(&self, dir: String) -> Vec<FfiTreeNode> {
        let Some(search) = &self.search else {
            return Vec::new();
        };
        let view = search.read();
        let mut cache = self.tree_cache.borrow_mut();
        let index = cache.index_for(view.version, &view.entries);
        index
            .children_of(&dir)
            .iter()
            .cloned()
            .map(FfiTreeNode::from_node)
            .collect()
    }

    /// The current file-search snapshot version, or 0 when no session is backing
    /// this handle. Live snapshots start at `version = 1`, so 0 uniquely marks an
    /// absent session; the front-end's refresh gate reloads only when this
    /// advances.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.search.as_ref().map_or(0, |s| s.read().version)
    }

    /// Maps an absolute path to its tree-display rel-path via an EXACT entry-path
    /// match (see [`wolf_core::tree::rel_path_for`]), or the empty string when no
    /// streamed entry matches (or the session is absent).
    #[must_use]
    pub fn rel_path_for(&self, abs: String) -> String {
        let Some(search) = &self.search else {
            return String::new();
        };
        let view = search.read();
        wolf_core::tree::rel_path_for(&view.entries, std::path::Path::new(&abs)).unwrap_or_default()
    }
}

/// One projected tree node (opaque to Swift, so a `Vec<FfiTreeNode>` is supported
/// — swift-bridge emits Vec support only for opaque types).
///
/// Flattens [`wolf_core::tree::TreeNode`]'s two variants into a tagged struct:
/// `is_dir` discriminates, and `abs_path` is the empty string for a `Dir`.
pub struct FfiTreeNode {
    name: String,
    rel_path: String,
    abs_path: Option<PathBuf>,
    is_dir: bool,
}

impl FfiTreeNode {
    /// The immediate component name (the last segment of `rel_path`).
    #[must_use]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// The full relative, `/`-separated path from the cwd root to this node.
    #[must_use]
    pub fn rel_path(&self) -> String {
        self.rel_path.clone()
    }

    /// The absolute filesystem path for a File node; the empty string for a Dir
    /// node.
    #[must_use]
    pub fn abs_path(&self) -> String {
        self.abs_path
            .as_ref()
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned())
    }

    /// Whether this node is a directory (vs. a leaf file).
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Flattens a [`wolf_core::tree::TreeNode`] into the bridge-friendly tagged
    /// struct.
    fn from_node(node: wolf_core::tree::TreeNode) -> Self {
        match node {
            wolf_core::tree::TreeNode::Dir { name, rel_path } => Self {
                name,
                rel_path,
                abs_path: None,
                is_dir: true,
            },
            wolf_core::tree::TreeNode::File {
                name,
                rel_path,
                abs_path,
            } => Self {
                name,
                rel_path,
                abs_path: Some(abs_path),
                is_dir: false,
            },
        }
    }
}

// --- FfiSearch / FfiSearchFile / FfiSearchMatch: the sidebar content search (opaque) ---

/// A live content-search session handle (opaque to Swift).
///
/// Wraps an `Option<`[`SearchSession`]`>` so [`WolfCoreHandle::start_search`] stays
/// infallible across the bridge: `None` means the core was gone (or `WOLF_RG_BIN`
/// unset) at start, and [`files`](FfiSearch::files) then yields nothing. Dropping
/// this handle drops the inner [`SearchSession`], cancelling the rg subprocess.
pub struct FfiSearch {
    search: Option<SearchSession>,
    /// Memoizes the projected + windowed `Vec<FfiSearchFile>` keyed on
    /// `(snapshot version, char budget)`, so AppKit's reload storm rebuilds the
    /// projection at most once per `(version, budget)` (the analog of
    /// [`FfiFileSearch`]'s `tree_cache`).
    cache: RefCell<SearchProjectionCache>,
}

/// A `(version, budget)`-keyed memo of the projected search results.
struct SearchProjectionCache {
    key: Option<(u64, u32)>,
    files: Vec<FfiSearchFile>,
}

impl SearchProjectionCache {
    fn new() -> Self {
        Self {
            key: None,
            files: Vec::new(),
        }
    }
}

impl FfiSearch {
    /// The current content-search snapshot version, or 0 when no session backs
    /// this handle. Live snapshots start at `version = 1`, so 0 uniquely marks an
    /// absent session; the panel's refresh gate reloads only when this advances.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.search.as_ref().map_or(0, |s| s.read().version)
    }

    /// The matched files projected + sorted from the latest streamed snapshot,
    /// each carrying its matching lines windowed to `budget` CHARACTERS.
    ///
    /// Reads the snapshot briefly, runs [`wolf_core::project`] + one
    /// [`wolf_core::context_window`] per match under the guard, and clones out an
    /// owned `Vec<FfiSearchFile>` (dropping the guard). Memoized per
    /// `(version, budget)`; an empty `Vec` when the session is absent.
    #[must_use]
    pub fn files(&self, budget: u32) -> Vec<FfiSearchFile> {
        let Some(search) = &self.search else {
            return Vec::new();
        };
        let view = search.read();
        let version = view.version;
        let mut cache = self.cache.borrow_mut();
        if cache.key != Some((version, budget)) {
            cache.files = build_search_files(&view.matches, budget);
            cache.key = Some((version, budget));
        }
        cache.files.clone()
    }
}

/// Projects `matches` into per-file groups and windows each match's line to
/// `budget` characters, producing the owned bridge representation.
fn build_search_files(matches: &[SearchMatch], budget: u32) -> Vec<FfiSearchFile> {
    let budget = budget as usize;
    wolf_core::project(matches)
        .into_iter()
        .map(|group| {
            let file_matches: Vec<FfiSearchMatch> = group
                .match_indices
                .iter()
                .map(|&i| {
                    let m = &matches[i];
                    let cw = wolf_core::context_window(&m.line_text, &m.match_ranges, budget);
                    FfiSearchMatch {
                        line_number: m.line_number,
                        column: m.column,
                        display_text: cw.text,
                        truncated_left: cw.truncated_left,
                        truncated_right: cw.truncated_right,
                        range_starts: cw.ranges.iter().map(|&(s, _)| s).collect(),
                        range_ends: cw.ranges.iter().map(|&(_, e)| e).collect(),
                    }
                })
                .collect();
            FfiSearchFile {
                label: group.label,
                rel_path: group.rel_path,
                abs_path: group.abs_path,
                matches: file_matches,
            }
        })
        .collect()
}

/// One matched file in the projected search tree (opaque to Swift, so a
/// `Vec<FfiSearchFile>` is supported).
#[derive(Clone)]
pub struct FfiSearchFile {
    label: String,
    rel_path: String,
    abs_path: PathBuf,
    matches: Vec<FfiSearchMatch>,
}

impl FfiSearchFile {
    /// The display label: `"<basename> - <relative parent dir>"` (just the
    /// basename for a root file).
    #[must_use]
    pub fn label(&self) -> String {
        self.label.clone()
    }

    /// The file's `/`-separated path relative to the search root (the sort key).
    #[must_use]
    pub fn rel_path(&self) -> String {
        self.rel_path.clone()
    }

    /// The absolute filesystem path, handed back to
    /// [`WolfCoreHandle::open_at`](WolfCoreHandle::open_at).
    #[must_use]
    pub fn abs_path(&self) -> String {
        self.abs_path.to_string_lossy().into_owned()
    }

    /// The matching lines in this file, ordered by line number.
    #[must_use]
    pub fn matches(&self) -> Vec<FfiSearchMatch> {
        self.matches.clone()
    }
}

/// One matching line within a file (opaque to Swift, so a `Vec<FfiSearchMatch>`
/// is supported).
#[derive(Clone)]
pub struct FfiSearchMatch {
    line_number: u64,
    column: u32,
    display_text: String,
    truncated_left: bool,
    truncated_right: bool,
    range_starts: Vec<u32>,
    range_ends: Vec<u32>,
}

impl FfiSearchMatch {
    /// The 1-based line number of the match.
    #[must_use]
    pub fn line_number(&self) -> u64 {
        self.line_number
    }

    /// The 0-based byte column of the first submatch (for cursor placement via
    /// `open_at`; indexes the ORIGINAL line, not `display_text`).
    #[must_use]
    pub fn column(&self) -> u32 {
        self.column
    }

    /// The visible (budget-windowed) line text.
    #[must_use]
    pub fn display_text(&self) -> String {
        self.display_text.clone()
    }

    /// Whether context was hidden on the LEFT of the window (leading ellipsis).
    #[must_use]
    pub fn truncated_left(&self) -> bool {
        self.truncated_left
    }

    /// Whether context was hidden on the RIGHT of the window (trailing ellipsis).
    #[must_use]
    pub fn truncated_right(&self) -> bool {
        self.truncated_right
    }

    /// The start byte offsets (into `display_text`) of the bold match ranges.
    #[must_use]
    pub fn range_starts(&self) -> Vec<u32> {
        self.range_starts.clone()
    }

    /// The end byte offsets (into `display_text`) of the bold match ranges,
    /// parallel to [`range_starts`](Self::range_starts).
    #[must_use]
    pub fn range_ends(&self) -> Vec<u32> {
        self.range_ends.clone()
    }
}

/// One ranked file-search result (opaque to Swift, so a `Vec<FfiFileEntry>` is
/// supported — swift-bridge emits Vec support only for opaque types).
pub struct FfiFileEntry {
    path: PathBuf,
    display: String,
}

impl FfiFileEntry {
    /// The display string (the path relative to the search root) shown + ranked.
    #[must_use]
    pub fn display(&self) -> String {
        self.display.clone()
    }

    /// The absolute filesystem path, handed back to [`WolfCoreHandle::open`].
    #[must_use]
    pub fn path(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

// --- FfiFrame: an owned, resolved snapshot of the active grid ---

/// An opaque, owned, paint-ready frame: the active grid with every cell already
/// resolved against the highlight table in Rust (never re-resolved in Swift).
pub struct FfiFrame {
    cols: u16,
    rows: u16,
    cursor_row: u16,
    cursor_col: u16,
    cursor_shape: u8,
    cursor_cell_percentage: u32,
    cursor_visible: bool,
    grid_id: u64,
    cursor_grid: u64,
    default_fg: FfiColor,
    default_bg: FfiColor,
    cursor_fill: FfiColor,
    cursor_glyph: FfiColor,
    cells: Vec<FfiCell>,
    topline: u64,
    botline: u64,
    curline: u64,
    linecount: u64,
}

impl FfiFrame {
    /// Grid width in columns.
    #[must_use]
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// Grid height in rows.
    #[must_use]
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Cursor row (grid coordinate).
    #[must_use]
    pub fn cursor_row(&self) -> u16 {
        self.cursor_row
    }

    /// Cursor column (grid coordinate).
    #[must_use]
    pub fn cursor_col(&self) -> u16 {
        self.cursor_col
    }

    /// Cursor shape (0 = Block, 1 = Horizontal, 2 = Vertical).
    #[must_use]
    pub fn cursor_shape(&self) -> u8 {
        self.cursor_shape
    }

    /// Bar-cursor cell coverage percentage (1–100), or 0 if unspecified.
    #[must_use]
    pub fn cursor_cell_percentage(&self) -> u32 {
        self.cursor_cell_percentage
    }

    /// Whether the cursor lives on THIS frame's grid (so it should be drawn here).
    #[must_use]
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// The grid id this frame was packed from.
    #[must_use]
    pub fn grid_id(&self) -> u64 {
        self.grid_id
    }

    /// The grid id the cursor currently lives on (at pack time), so a pane can
    /// tell whether the cursor belongs to it even when it is not this frame's grid.
    #[must_use]
    pub fn cursor_grid(&self) -> u64 {
        self.cursor_grid
    }

    /// The editor default foreground color.
    #[must_use]
    pub fn default_fg(&self) -> FfiColor {
        self.default_fg.dup()
    }

    /// The editor default background color.
    #[must_use]
    pub fn default_bg(&self) -> FfiColor {
        self.default_bg.dup()
    }

    /// The cursor fill color (reverse-video; ported from wolf-gnome's `cursor.rs`).
    #[must_use]
    pub fn cursor_fill(&self) -> FfiColor {
        self.cursor_fill.dup()
    }

    /// The cursor glyph color (the readable color over the fill).
    #[must_use]
    pub fn cursor_glyph(&self) -> FfiColor {
        self.cursor_glyph.dup()
    }

    /// The flat, row-major grid of resolved cells (`cols * rows` entries).
    #[must_use]
    pub fn cells(&self) -> Vec<FfiCell> {
        self.cells.clone()
    }

    /// The viewport's first visible buffer line (0-based, as nvim's
    /// `win_viewport` reports; botline is one-past-last-visible), or 0 when this
    /// grid has no associated window.
    #[must_use]
    pub fn topline(&self) -> u64 {
        self.topline
    }

    /// One past the viewport's last visible buffer line, or 0 when no window.
    #[must_use]
    pub fn botline(&self) -> u64 {
        self.botline
    }

    /// The cursor's buffer line, or 0 when no window.
    #[must_use]
    pub fn curline(&self) -> u64 {
        self.curline
    }

    /// The buffer's total line count, or 0 when no window.
    #[must_use]
    pub fn linecount(&self) -> u64 {
        self.linecount
    }

    /// An empty `0x0` frame (used when the core has already been shut down).
    fn empty() -> Self {
        Self {
            cols: 0,
            rows: 0,
            cursor_row: 0,
            cursor_col: 0,
            cursor_shape: cursor_shape::BLOCK,
            cursor_cell_percentage: 0,
            cursor_visible: false,
            grid_id: 0,
            cursor_grid: 0,
            default_fg: FfiColor::unset(),
            default_bg: FfiColor::unset(),
            cursor_fill: FfiColor::unset(),
            cursor_glyph: FfiColor::unset(),
            cells: Vec::new(),
            topline: 0,
            botline: 0,
            curline: 0,
            linecount: 0,
        }
    }

    /// Builds a frame from a render-state snapshot's ACTIVE grid, resolving its
    /// cells against the highlight table in Rust.
    ///
    /// Delegates to [`FfiFrame::from_snapshot_grid`] with the snapshot's active
    /// grid id. Public so the headless `tests/frame_pack.rs` integration test can
    /// drive the snapshot-level packing directly (no live `WolfCore`); `read_frame`
    /// keeps delegating to it.
    #[must_use]
    pub fn from_snapshot(snapshot: &wolf_core::Snapshot) -> Self {
        Self::from_snapshot_grid(snapshot, snapshot.active_grid_id())
    }

    /// Builds a frame from a SPECIFIC `grid_id` of a render-state snapshot,
    /// resolving that grid's cells against the highlight table in Rust.
    ///
    /// An absent grid yields `FfiFrame::empty` (`cols()`/`rows()` 0 — the
    /// front-end treats that as "grid gone"). `cursor_visible` is per-grid
    /// (`snapshot.cursor.grid == grid_id`), and the cursor reverse-video colors are
    /// computed against the REQUESTED grid's cell under the cursor (irrelevant when
    /// the cursor is not on this grid).
    #[must_use]
    pub fn from_snapshot_grid(snapshot: &wolf_core::Snapshot, grid_id: u64) -> Self {
        let Some(grid) = snapshot.grid(grid_id) else {
            return Self::empty();
        };
        let highlights = &snapshot.highlights;

        let mut cells = Vec::with_capacity(grid.cols * grid.rows);
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let (text, style) = match grid.cell(row, col) {
                    Some(cell) => (cell.text.clone(), highlights.resolve(cell.hl_id)),
                    None => (String::new(), ResolvedStyle::default()),
                };
                cells.push(FfiCell {
                    text,
                    fg: style.fg,
                    bg: style.bg,
                    special: style.special,
                    flags: pack_flags(&style),
                });
            }
        }

        let (shape, cell_percentage) = snapshot
            .mode_infos
            .get(snapshot.cursor.mode_idx)
            .map_or((CursorShape::default(), None), |m| {
                (m.shape, m.cell_percentage)
            });

        let (fill, glyph) = cursor_colors(snapshot, grid);

        let vp = snapshot
            .windows
            .get(&grid_id)
            .map(|w| w.viewport)
            .unwrap_or_default();

        Self {
            cols: clamp_u16(grid.cols),
            rows: clamp_u16(grid.rows),
            cursor_row: clamp_u16(snapshot.cursor.row),
            cursor_col: clamp_u16(snapshot.cursor.col),
            cursor_shape: pack_cursor_shape(shape),
            cursor_cell_percentage: cell_percentage.unwrap_or(0),
            cursor_visible: snapshot.cursor.grid == grid_id,
            grid_id,
            cursor_grid: snapshot.cursor.grid,
            default_fg: FfiColor::from_opt(highlights.defaults.fg),
            default_bg: FfiColor::from_opt(highlights.defaults.bg),
            cursor_fill: FfiColor::from_rgb(fill),
            cursor_glyph: FfiColor::from_rgb(glyph),
            cells,
            topline: vp.topline,
            botline: vp.botline,
            curline: vp.curline,
            linecount: vp.linecount,
        }
    }
}

// --- Color / style / key helpers ---

/// Fallback foreground (white) when neither a highlight nor the defaults set one,
/// mirroring wolf-gnome's `render::FALLBACK_FG`.
const FALLBACK_FG: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};
/// Fallback background (black), mirroring wolf-gnome's `render::FALLBACK_BG`.
const FALLBACK_BG: Rgb = Rgb { r: 0, g: 0, b: 0 };

/// Bit shift for the red channel when packing an [`Rgb`] into `0x00RRGGBB`.
const RED_SHIFT: u32 = 16;
/// Bit shift for the green channel.
const GREEN_SHIFT: u32 = 8;

impl FfiColor {
    /// An unset color (`set = false`).
    fn unset() -> Self {
        Self { rgb: 0, set: false }
    }

    /// Duplicates this color (swift-bridge transparent structs are not `Copy`).
    fn dup(&self) -> Self {
        Self {
            rgb: self.rgb,
            set: self.set,
        }
    }

    /// Packs an [`Rgb`] into a set [`FfiColor`].
    fn from_rgb(c: Rgb) -> Self {
        Self {
            rgb: (u32::from(c.r) << RED_SHIFT) | (u32::from(c.g) << GREEN_SHIFT) | u32::from(c.b),
            set: true,
        }
    }

    /// Packs an `Option<Rgb>` into an [`FfiColor`] (`None` → unset).
    fn from_opt(c: Option<Rgb>) -> Self {
        c.map_or_else(Self::unset, Self::from_rgb)
    }
}

/// Packs a [`ResolvedStyle`]'s boolean attributes into the [`FfiCell::flags`]
/// bitset.
fn pack_flags(style: &ResolvedStyle) -> u8 {
    let mut f = 0u8;
    if style.bold {
        f |= flag::BOLD;
    }
    if style.italic {
        f |= flag::ITALIC;
    }
    if style.underline {
        f |= flag::UNDERLINE;
    }
    if style.undercurl {
        f |= flag::UNDERCURL;
    }
    if style.strikethrough {
        f |= flag::STRIKETHROUGH;
    }
    f
}

/// Packs a [`CursorShape`] into its `u8` discriminant.
fn pack_cursor_shape(shape: CursorShape) -> u8 {
    match shape {
        CursorShape::Block => cursor_shape::BLOCK,
        CursorShape::Horizontal => cursor_shape::HORIZONTAL,
        CursorShape::Vertical => cursor_shape::VERTICAL,
    }
}

/// Clamps a `usize` grid dimension/coordinate into the `u16` the bridge uses.
fn clamp_u16(v: usize) -> u16 {
    u16::try_from(v).unwrap_or(u16::MAX)
}

/// Clamps a `usize` to a `u32` (saturating at `u32::MAX`), for the command-line
/// byte-offset / indent / recursion-level scalars crossing the bridge.
fn clamp_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// Computes the cursor's `(fill, glyph)` colors (reverse video, honoring the
/// `WolfCursor` app-theme group first, then a visible mode cursor highlight),
/// ported from — and kept in lockstep with — wolf-gnome's
/// `cursor::cursor_colors`.
///
/// The `WolfCursor` theme group (`Snapshot.theme.cursor`) is honored FIRST and
/// unconditionally (an explicit app-level opt-in): a set `theme.cursor.bg`
/// becomes the fill, with the glyph from `theme.cursor.fg` or — when unset —
/// the underlying cell's background (the reverse-video glyph).
///
/// Reads the cell under the cursor from `grid` (the REQUESTED grid being packed),
/// not the active grid, so a per-pane frame computes the cursor colors against its
/// own grid (irrelevant when the cursor is not on that grid).
fn cursor_colors(snap: &wolf_core::Snapshot, grid: &Grid) -> (Rgb, Rgb) {
    let defaults = &snap.highlights.defaults;

    let (cell_fg, cell_bg) = match grid.cell(snap.cursor.row, snap.cursor.col) {
        Some(cell) => {
            let style = snap.highlights.resolve(cell.hl_id);
            (
                style.fg.unwrap_or(FALLBACK_FG),
                style.bg.unwrap_or(FALLBACK_BG),
            )
        }
        None => (
            defaults.fg.unwrap_or(FALLBACK_FG),
            defaults.bg.unwrap_or(FALLBACK_BG),
        ),
    };

    // The WolfCursor theme override wins unconditionally (explicit opt-in, unlike the guarded
    // mode-attr path below); a bg-only group keeps the reverse-video glyph (the cell's bg).
    if let Some(bg) = snap.theme.cursor.bg {
        return (bg, snap.theme.cursor.fg.unwrap_or(cell_bg));
    }

    if let Some(attr_id) = snap
        .mode_infos
        .get(snap.cursor.mode_idx)
        .and_then(|mode| mode.attr_id)
    {
        let style = snap.highlights.resolve(attr_id);
        if let (Some(f), Some(b)) = (style.fg, style.bg) {
            if b != cell_bg && b != f {
                return (b, f);
            }
        }
    }

    (cell_fg, cell_bg)
}

/// Maps an [`FfiKeyEvent`] into a [`wolf_core::KeyEvent`], or `None` if the event
/// carries no logical key (e.g. an unrecognized `kind`).
///
/// `kind` discriminants are the single source of truth shared with the Swift
/// `WolfKeyMap`; see the [`kind`] module.
fn key_event_from_ffi(ev: &FfiKeyEvent) -> Option<KeyEvent> {
    let key = match ev.kind {
        kind::CHAR => Key::Char(char::from_u32(ev.ch)?),
        kind::ENTER => Key::Enter,
        kind::ESC => Key::Esc,
        kind::TAB => Key::Tab,
        kind::BACKSPACE => Key::Backspace,
        kind::DELETE => Key::Delete,
        kind::LEFT => Key::Left,
        kind::RIGHT => Key::Right,
        kind::UP => Key::Up,
        kind::DOWN => Key::Down,
        kind::HOME => Key::Home,
        kind::END => Key::End,
        kind::PAGE_UP => Key::PageUp,
        kind::PAGE_DOWN => Key::PageDown,
        kind::FUNCTION => Key::F(ev.fnum),
        _ => return None,
    };
    Some(KeyEvent {
        key,
        ctrl: ev.ctrl,
        shift: ev.shift,
        alt: ev.alt,
        meta: ev.meta,
    })
}

/// Maps a [`terminal_target`] discriminant and window into a [`TerminalTarget`], or
/// `None` for an unknown discriminant.
fn terminal_target_from_ffi(win: i64, target: u8) -> Option<TerminalTarget> {
    let orientation = match target {
        terminal_target::WINDOW => return Some(TerminalTarget::Window(win)),
        terminal_target::SPLIT_HORIZONTAL => SplitOrientation::Horizontal,
        terminal_target::SPLIT_VERTICAL => SplitOrientation::Vertical,
        _ => return None,
    };
    Some(TerminalTarget::Split { win, orientation })
}

/// Maps an [`FfiMouseEvent`] into a [`wolf_core::MouseEvent`], or `None` if the
/// `button` or `action` carries an unrecognized discriminant.
///
/// The `button` / `action` discriminants are the single source of truth shared
/// with the Swift `PaneContainerView`; see the [`mouse_button`] / [`mouse_action`]
/// modules. `grid` / `row` / `col` are forwarded verbatim (grid-local under
/// `ext_multigrid`).
fn mouse_event_from_ffi(ev: &FfiMouseEvent) -> Option<MouseEvent> {
    let button = match ev.button {
        mouse_button::LEFT => MouseButton::Left,
        mouse_button::RIGHT => MouseButton::Right,
        mouse_button::MIDDLE => MouseButton::Middle,
        _ => return None,
    };
    let action = match ev.action {
        mouse_action::PRESS => MouseAction::Press,
        mouse_action::RELEASE => MouseAction::Release,
        mouse_action::DRAG => MouseAction::Drag,
        _ => return None,
    };
    Some(MouseEvent {
        button,
        action,
        grid: ev.grid,
        row: ev.row,
        col: ev.col,
        ctrl: ev.ctrl,
        shift: ev.shift,
        alt: ev.alt,
    })
}

/// Maps an [`FfiScrollEvent`] into a [`wolf_core::ScrollEvent`], or `None` if the
/// `direction` carries an unrecognized discriminant.
///
/// The `direction` discriminants are the single source of truth shared with the
/// Swift `PaneContainerView`; see the [`scroll_dir`] module. `grid` / `row` / `col`
/// are forwarded verbatim (grid-local under `ext_multigrid`).
fn scroll_event_from_ffi(ev: &FfiScrollEvent) -> Option<ScrollEvent> {
    let direction = match ev.direction {
        scroll_dir::UP => ScrollDirection::Up,
        scroll_dir::DOWN => ScrollDirection::Down,
        scroll_dir::LEFT => ScrollDirection::Left,
        scroll_dir::RIGHT => ScrollDirection::Right,
        _ => return None,
    };
    Some(ScrollEvent {
        direction,
        grid: ev.grid,
        row: ev.row,
        col: ev.col,
    })
}
