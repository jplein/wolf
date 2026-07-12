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

// swift-bridge's `#[swift_bridge::bridge]` expansion emits same-type raw-pointer
// casts in the generated FFI shims (e.g. `*mut FfiFrame -> *mut FfiFrame`); a
// module-level allow does not reach those sibling items, so this one lint is
// allowed crate-wide. The hand-written glue performs no such casts.
#![allow(clippy::unnecessary_cast)]

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use wolf_core::{
    rank_buffers, rank_matches_top_n, rank_palette, BufInfo, BufferRow, Cmdline, Command,
    CommandMru, CoreConfig, CursorShape, FileSearch, FocusDir, Grid, HighlightTable, InputEvent,
    Key, KeyEvent, LayoutIntent, MouseAction, MouseButton, MouseEvent, PaletteRow, PaletteRowKind,
    PaletteWindow, PaneStatus, Pum, ResolvedStyle, Rgb, ScrollDirection, ScrollEvent,
    SplitOrientation, StateStore, TabState, Toast, WakeFn, WinInfo, WindowState, WolfCore,
    WolfState,
};

// Re-export the bridge-defined transparent structs at the crate root so the Rust
// integration test (and any other rlib consumer) can name
// `wolf_ffi::{FfiColor, FfiKeyEvent}` directly. The opaque `FfiFrame`, `FfiCell`,
// and `WolfCoreHandle`, plus `wolf_ffi_start`, are already crate-root `pub` items
// defined below.
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

/// The layout-intent kind discriminants packed into [`FfiLayoutIntent::kind`].
///
/// The single source of truth shared with the Swift `PaneTree` port and the
/// `WolfKeyMapTests` / `tests/panes.rs` discriminant contract (plan §3.1).
mod intent_kind {
    /// A window SPLIT was performed by nvim (carries orientation + from + win).
    pub const SPLIT: u8 = 0;
    /// A PURE focus-direction move request (carries dir + from).
    pub const FOCUS_DIR: u8 = 1;
}

/// The command-kind discriminants packed into [`FfiCommand::kind`].
///
/// The single source of truth shared with the Swift `CommandKind` port and the
/// `WolfKeyMapTests` / `tests/commands.rs` discriminant contract (plan §3),
/// mirroring the variant order of [`wolf_core::Command`].
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
    /// An "Open Recent Folder" row (opens-or-raises the recent dir at `path`).
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

/// How long [`wolf_ffi_state_flush`] (and the shutdown-time flush) blocks
/// waiting for the state writer thread to drain everything pending.
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

// swift-bridge's generated module expands into code that does not satisfy this
// crate's stricter lints; scope a narrow allow to the bridge module only. The
// hand-written glue below the module is held to the normal bar.
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

    // A structured key event from Swift, mapped in Rust into a
    // `wolf_core::KeyEvent`. The `kind` field discriminates the logical key; for
    // `kind == Char`, `ch` is the (already-shifted) Unicode scalar.
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

    // A structured mouse event from Swift, mapped in Rust into a
    // `wolf_core::MouseEvent`. `button`/`action` discriminate via `mod
    // mouse_button` / `mod mouse_action`; `grid` is the target WINDOW grid (>= 2
    // under ext_multigrid) and `row`/`col` are GRID-LOCAL.
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

    // A structured scroll event from Swift, mapped in Rust into a
    // `wolf_core::ScrollEvent`. `direction` discriminates via `mod scroll_dir`;
    // `grid` is the target WINDOW grid and `row`/`col` are GRID-LOCAL.
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

        // The per-core wake-routing identity token, minted fresh per
        // `wolf_ffi_start` and carried as the sole arg of every wake callback so
        // a process-global callback routes each wake to the right window.
        fn wake_token(&self) -> u64;
        fn read_frame(&self) -> FfiFrame;
        // The per-grid paint-ready frame for `grid` (the pane container reads one
        // per pane, plus grid 1 for the cmdline strip). An absent grid yields an
        // empty `0x0` frame (Swift treats `cols() == 0` as "grid gone").
        fn read_frame_for_grid(&self, grid: u64) -> FfiFrame;
        fn send_key(&self, ev: FfiKeyEvent);
        // Maps a structured mouse event into a `wolf_core::MouseEvent` and forwards
        // it to Neovim (`nvim_input_mouse` targeting `ev.grid` with grid-local
        // row/col). A no-op on an unrecognized button/action discriminant.
        fn send_mouse(&self, ev: FfiMouseEvent);
        // Maps a structured scroll event into a `wolf_core::ScrollEvent` and
        // forwards it to Neovim (`nvim_input_mouse("wheel", …)`). A no-op on an
        // unrecognized direction discriminant.
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
        // Creates a new nvim TABPAGE (a new view of this PROJECT), optionally
        // rooted at `cwd` via `:tcd` (empty string = no `:tcd`), and returns its
        // stable HANDLE. Blocks briefly on the captive nvim. 0 when the core is
        // gone (the caller then drops the would-be view).
        fn create_tabpage(&self, cwd: String) -> i64;
        // The current tabpage's stable HANDLE via a blocking RPC (the first-view
        // bind). 0 when the core is gone.
        fn current_tabpage(&self) -> i64;
        // The CHEAP `Snapshot.current_tabpage` refocus gate (updated out-of-band
        // via `wolf_tabpage`), read from the snapshot with no RPC. 0 when the core
        // is gone / not yet known.
        fn active_tabpage(&self) -> i64;
        // A tabpage's tab-local `:tcd` cwd (canonicalized), authoritative for a
        // NON-current view. Blocks briefly on the captive nvim. Empty when the
        // core is gone or the handle is invalid (e.g. a closed tabpage).
        fn cwd_for_tabpage(&self, tab: i64) -> String;
        // Makes tabpage `tab` current (the front-end drives every tabpage switch
        // when a view becomes key). A no-op when the core is gone.
        fn set_current_tabpage(&self, tab: i64);
        // Closes tabpage `tab` (a non-last view close; the shared nvim stays alive
        // until its LAST view closes — the front-end shuts the core down then). A
        // no-op when the core is gone.
        fn close_tabpage(&self, tab: i64);
        // Focuses a specific Neovim window (click-to-focus / resolved FocusDir).
        fn focus_win(&self, win: i64);
        fn shutdown(&self);
        // Starts a streaming file-search session over the core's cwd, fed by the
        // registered file-search wake. Infallible: returns an empty-backed handle
        // when the core is gone (mirroring `read_frame`'s empty-frame fallback).
        fn start_file_search(&self) -> FfiFileSearch;
        // Starts a SEPARATE long-lived streaming file-search session for the
        // file-tree sidebar, fed by the registered TREE wake. Same infallible
        // empty-backed fallback as `start_file_search`.
        fn start_tree_search(&self) -> FfiFileSearch;
        // Builds a fresh command-palette session seeded with the core's recent
        // dirs + command MRU. The front-end feeds each OTHER window via
        // `add_window`, then calls `ranked`. Infallible: returns an empty-backed
        // session when the core is gone (mirroring `start_file_search`).
        fn command_palette(&self) -> FfiCommandPalette;
        // The absolute path of the buffer currently being edited (canonicalized
        // by core), or the empty string for an unnamed buffer / shut-down core.
        fn current_file(&self) -> String;
        // Neovim's LIVE, already-canonicalized working directory (tracks
        // `:cd`/`DirChanged`), or the empty string until nvim first reports it /
        // after shutdown.
        fn cwd(&self) -> String;
        // The GLOBAL recent-directories MRU, newest-first (capped at 20), as
        // owned Strings; an empty `Vec` when the store is disabled/unreadable or
        // the core is gone. Drives the File > Open Recent submenu.
        fn recent_dirs(&self) -> Vec<String>;
        // The WolfTitlebar theme group's background color (the macOS titlebar
        // background), following the `default_fg`/`default_bg` convention:
        // unset when the group is absent/cleared or the core is gone.
        fn theme_titlebar_bg(&self) -> FfiColor;
        // The WolfTitlebar theme group's foreground color (carried for future
        // title-text coloring; not applied on macOS this pass). Same unset
        // convention as `theme_titlebar_bg`.
        fn theme_titlebar_fg(&self) -> FfiColor;
        // The per-pane statusbar strings for the window drawing into `grid`
        // (base filename + display path). `FfiPaneStatus` is opaque so its
        // Strings are read back via accessor methods. An unnamed buffer / grid
        // with no window / shut-down core yields the `[No Name]` filename with
        // an empty display path.
        fn pane_status_for_grid(&self, grid: u64) -> FfiPaneStatus;
        // Opens `path` (an absolute filesystem path) in Neovim via
        // `InputEvent::Open`; the core's input pump records the MRU.
        fn open(&self, path: String);
        // Changes the current VIEW's working directory to `path` (an absolute
        // filesystem path) via `InputEvent::Chdir` (`:tcd`, the tab-local cwd —
        // each view/tabpage owns its own working directory).
        fn chdir(&self, path: String);
        // Closes the current split (the pane) via `InputEvent::ClosePane`
        // (`:close`).
        fn close_pane(&self);
        // Opens a new empty buffer in the current window via
        // `InputEvent::NewBuffer` (`:enew`).
        fn new_buffer(&self);
        // The current visual-mode selection yanked to text (blocks briefly on
        // the captive nvim), or the empty string when not in visual mode / the
        // selection is empty / the core is gone. The front-end writes it to
        // `NSPasteboard` only when non-empty.
        fn copy_selection(&self) -> String;
        // Pastes `text` into the active document via `InputEvent::Paste`
        // (`nvim_paste`). A no-op when the core is gone.
        fn paste(&self, text: String);
        // Builds a fresh buffer-picker session seeded from a blocking snapshot of
        // the core's listed buffers plus the live cwd. The front-end filters per
        // keystroke via `ranked`. Infallible: returns an empty-backed session when
        // the core is gone (mirroring `command_palette`).
        fn buffer_picker(&self) -> FfiBufferPicker;
        // Switches the ACTIVE window to buffer `bufnr` via `InputEvent::SwitchBuffer`
        // (window-scoped `nvim_win_set_buf` + per-pane view restore). Resolves the
        // active window live from the snapshot. A no-op when the core is gone or no
        // active window is known.
        fn switch_buffer(&self, bufnr: i64);
        // Whether the active document is in Neovim visual mode (`v`/`V`/`<C-v>`).
        // Gates the Edit > Copy menu item. False when the core is gone.
        fn is_visual_mode(&self) -> bool;
        // The native command line (`ext_cmdline`) as an opaque `FfiCmdline`
        // (active / firstc / prompt / text / pos / indent / level). An inactive
        // cmdline (or a shut-down core) yields an inactive value.
        fn cmdline(&self) -> FfiCmdline;
        // The completion popup (`ext_popupmenu`) as an opaque `FfiPum`
        // (visible / selected / anchor + items). Hidden when the core is gone.
        fn pum(&self) -> FfiPum;
        // The message toast (`ext_messages`) as an opaque `FfiToast`
        // (visible / kind / lines). Its chunk colors are RESOLVED in Rust against
        // the same highlight table the frame uses (like `FfiCell`). Empty/hidden
        // when the core is gone.
        fn toast(&self) -> FfiToast;
    }

    extern "Rust" {
        type FfiFileSearch;

        // The top `max` ranked entries for `query` against the latest streamed
        // search snapshot. An empty `query` lists all entries MRU-first. `FfiFileEntry`
        // is opaque so a `Vec<FfiFileEntry>` is supported (same as `FfiCell`).
        fn ranked(&self, query: String, max: usize) -> Vec<FfiFileEntry>;
        // The IMMEDIATE children of `dir` ("" = the cwd root) projected from the
        // latest streamed search snapshot: directories-first then files (each
        // case-insensitive by name). `FfiTreeNode` is opaque so a
        // `Vec<FfiTreeNode>` is supported.
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
        // The ancestor directory rel-paths of `rel_path`, from the cwd root down
        // to the node's parent (`"src/main.rs"` -> `["src"]`, `"README.md"` ->
        // `[]`). A free fn (no session needed) delegating to `wolf_core::tree`.
        fn wolf_ffi_ancestors_to_reveal(rel_path: String) -> Vec<String>;
    }

    extern "Rust" {
        type FfiConfig;

        // The resolved font family name (e.g. "iA Writer Mono S").
        fn font_family(&self) -> String;
        // The resolved font size, in points.
        fn font_size(&self) -> f64;
        // The resolved line-height, as a percentage of the font size (130 == 130%).
        fn line_height(&self) -> f64;
    }

    extern "Rust" {
        // Loads the resolved user config (reading — or creating with defaults —
        // `wolf-config.yaml` in the platform prefs dir). A free fn (no live core
        // needed) delegating to `wolf_core::config`; infallible (defaults on any
        // error). Standalone, like `wolf_ffi_ancestors_to_reveal`.
        fn wolf_ffi_load_config() -> FfiConfig;
    }

    extern "Rust" {
        type FfiState;

        // The restored per-window entries (empty when nothing was saved).
        // `FfiWindowState` is opaque so a `Vec` is supported.
        fn windows(&self) -> Vec<FfiWindowState>;
    }

    extern "Rust" {
        type FfiWindowState;

        // The window origin's x coordinate, in points.
        fn x(&self) -> f64;
        // The window origin's y coordinate, in points.
        fn y(&self) -> f64;
        // The window width, in points (`<= 0` == unset — do not restore).
        fn width(&self) -> f64;
        // The window height, in points (`<= 0` == unset — do not restore).
        fn height(&self) -> f64;
        // The window's saved tabs, in tab order. `FfiTabState` is opaque so a
        // `Vec` is supported.
        fn tabs(&self) -> Vec<FfiTabState>;
    }

    extern "Rust" {
        type FfiTabState;

        // The tab's saved working directory, or "" when never reported.
        fn cwd(&self) -> String;
        // Whether the tab's file-tree sidebar was visible.
        fn sidebar_visible(&self) -> bool;
    }

    extern "Rust" {
        // Loads the saved application state from the platform state dir (the
        // pre-start restore read). NEVER creates the file and NEVER opens the
        // live store; absent/corrupt/unresolvable all yield an empty window
        // list. A free fn, standalone like `wolf_ffi_load_config`.
        fn wolf_ffi_load_state() -> FfiState;

        // Clears the write-side scratch tree, starting a fresh whole-tree
        // snapshot the coordinator builds up window-by-window / tab-by-tab.
        fn wolf_ffi_state_snapshot_begin();
        // Pushes a window with the given geometry (points) and an empty tab list
        // onto the scratch tree. Subsequent `add_tab` calls attach to it.
        fn wolf_ffi_state_snapshot_add_window(x: f64, y: f64, width: f64, height: f64);
        // Pushes a tab (its cwd + sidebar visibility) onto the most recently
        // added window. A defensive no-op when no window has been added yet.
        fn wolf_ffi_state_snapshot_add_tab(cwd: String, sidebar_visible: bool);
        // Commits the scratch tree into the shared state store (a whole-tree
        // `set_snapshot`); an identical tree is a no-op there.
        fn wolf_ffi_state_snapshot_commit();
        // Blocks until every pending state write has been attempted (bounded by
        // an internal ~2s timeout). Returns whether everything pending flushed in
        // time (true when no store exists — there is nothing to flush).
        fn wolf_ffi_state_flush() -> bool;
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
        // Reverse-video cursor colors (so the cursor is reliably visible). Two
        // scalar accessors rather than a `Vec<FfiColor>`: swift-bridge does not
        // emit Vec support for transparent structs.
        fn cursor_fill(&self) -> FfiColor;
        fn cursor_glyph(&self) -> FfiColor;
        // The flat, row-major grid of resolved cells (`cols * rows` entries).
        // `FfiCell` is opaque (Vec support is generated for opaque types).
        fn cells(&self) -> Vec<FfiCell>;
        // The viewport's first visible buffer line (0-based, as nvim's
        // `win_viewport` reports), for the synthetic scrollbar; 0 when this grid
        // has no associated window.
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

        // The intent kind (`intent_kind::SPLIT` / `FOCUS_DIR`).
        fn kind(&self) -> u8;
        // For a SPLIT: the orientation (`orientation::HORIZONTAL` / `VERTICAL`);
        // 0 for a FOCUS_DIR.
        fn orientation(&self) -> u8;
        // For a FOCUS_DIR: the direction (`focus_dir::LEFT` / `RIGHT` / `UP` /
        // `DOWN`); 0 for a SPLIT.
        fn dir(&self) -> u8;
        // The origin window handle (current before the split / when the chord
        // fired).
        fn from(&self) -> i64;
        // For a SPLIT: the new window handle; 0 for a FOCUS_DIR.
        fn win(&self) -> i64;
    }

    extern "Rust" {
        type FfiCommand;

        // The command kind (`command_kind::NEW_WINDOW` .. `NEW_TAB`).
        fn kind(&self) -> u8;
    }

    extern "Rust" {
        type FfiCommandPalette;

        // Appends one OTHER open window to the session (the current window is
        // excluded by the caller). `index` is the front-end window index (the
        // raise target); `cwd` is the window's working directory (`""` for an
        // empty-state window, which renders as "Untitled").
        fn add_window(&self, index: u16, cwd: String);
        // The top `max` ranked palette rows for `query`. Assembles the static
        // registry commands (minus CommandPalette) + the added windows + the
        // recent dirs, ranks MRU-first via `wolf_core::rank_palette`. `FfiCommandRow`
        // is opaque so a `Vec` is supported.
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

        // The top `max` ranked buffer rows for `query`, MRU-ordered
        // (lastused-DESC) among matches via `wolf_core::rank_buffers` over the
        // snapshot taken at `buffer_picker()`. An empty `query` lists all buffers
        // MRU-first (current buffer at row 0). `FfiBufferRow` is opaque so a `Vec`
        // is supported.
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

        // The base filename shown in the pane's statusbar (or `[No Name]` for an
        // unnamed buffer).
        fn filename(&self) -> String;
        // The path shown beneath the filename (cwd-relative, `~`-prefixed, or
        // absolute; empty for an unnamed buffer).
        fn display_path(&self) -> String;
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

// ---------------------------------------------------------------------------
// Wake registration (Swift-free; see the crate docs).
// ---------------------------------------------------------------------------

/// The C ABI of the front-end wake callback. It carries the firing core's
/// per-window wake token so a single process-global callback can route each
/// wake to the right window WITHOUT clobbering.
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
            // SAFETY: `ptr` was set from a valid `extern "C" fn(u64)` by
            // `wolf_ffi_set_filesearch_wake`; a non-null value is always such a
            // pointer.
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

/// The process-wide application-state store, opened once over
/// [`wolf_core::state_dir`] and cloned into every window's handle.
///
/// Opening the same `state.json` directory more than once would race the
/// per-store writer threads (unsupported), so all windows SHARE one store. The
/// `Option` is `None` when no state dir could be resolved — state persistence
/// is then disabled process-wide (logged, never fatal). `OnceLock` makes the
/// open lazy and single-shot.
static SHARED_STORE: OnceLock<Option<StateStore>> = OnceLock::new();

/// Returns a clone of the process-wide [`StateStore`], opening it on first use
/// over [`wolf_core::state_dir`]. `None` when no state dir could be resolved.
fn shared_store() -> Option<StateStore> {
    SHARED_STORE
        .get_or_init(|| wolf_core::state_dir().map(|dir| StateStore::open(&dir)))
        .clone()
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

// ---------------------------------------------------------------------------
// FfiConfig: the resolved user configuration (opaque to Swift).
// ---------------------------------------------------------------------------

/// The resolved user configuration (opaque to Swift; String-in-struct is not
/// supported for transparent bridge structs, so — like [`FfiFileEntry`] — this is
/// opaque with scalar/String accessors).
///
/// Built by [`wolf_ffi_load_config`] from [`wolf_core::WolfConfig`].
pub struct FfiConfig {
    font_family: String,
    font_size: f64,
    line_height: f64,
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
    FfiConfig {
        font_family: config.font_family,
        font_size: config.font_size,
        line_height: config.line_height,
    }
}

// ---------------------------------------------------------------------------
// FfiState / FfiWindowState: the saved application state (opaque to Swift).
// ---------------------------------------------------------------------------

/// The saved application state as loaded by [`wolf_ffi_load_state`] (opaque to
/// Swift). A thin wrapper vending the per-window entries — kept opaque (rather
/// than returning the `Vec` from the free fn directly) so the bridge stays on
/// the proven opaque-type-method `Vec` pattern.
pub struct FfiState {
    windows: Vec<FfiWindowState>,
}

impl FfiState {
    /// The restored per-window entries (empty when nothing was saved).
    #[must_use]
    pub fn windows(&self) -> Vec<FfiWindowState> {
        self.windows.clone()
    }
}

/// One saved window's state (opaque to Swift, so a `Vec<FfiWindowState>` is
/// supported — swift-bridge emits Vec support only for opaque types).
///
/// A projection of [`wolf_core::WindowState`] exposing the saved geometry plus
/// the window's tabs through per-field accessors.
#[derive(Clone)]
pub struct FfiWindowState {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    tabs: Vec<FfiTabState>,
}

impl FfiWindowState {
    /// The window origin's x coordinate, in points.
    #[must_use]
    pub fn x(&self) -> f64 {
        self.x
    }

    /// The window origin's y coordinate, in points.
    #[must_use]
    pub fn y(&self) -> f64 {
        self.y
    }

    /// The window width, in points (`<= 0` == unset — do not restore).
    #[must_use]
    pub fn width(&self) -> f64 {
        self.width
    }

    /// The window height, in points (`<= 0` == unset — do not restore).
    #[must_use]
    pub fn height(&self) -> f64 {
        self.height
    }

    /// The window's saved tabs, in tab order.
    #[must_use]
    pub fn tabs(&self) -> Vec<FfiTabState> {
        self.tabs.clone()
    }

    /// Projects a [`wolf_core::WindowState`] into its FFI accessor form.
    fn from_window(w: &WindowState) -> Self {
        Self {
            x: w.x,
            y: w.y,
            width: w.width,
            height: w.height,
            tabs: w.tabs.iter().map(FfiTabState::from_tab).collect(),
        }
    }
}

/// One saved tab's state (opaque to Swift, so a `Vec<FfiTabState>` is supported
/// — swift-bridge emits Vec support only for opaque types).
///
/// A projection of [`wolf_core::TabState`] exposing the saved working directory
/// and sidebar visibility through per-field accessors.
#[derive(Clone)]
pub struct FfiTabState {
    cwd: String,
    sidebar_visible: bool,
}

impl FfiTabState {
    /// The saved working directory, or `""` when never reported.
    #[must_use]
    pub fn cwd(&self) -> String {
        self.cwd.clone()
    }

    /// Whether the tab's file-tree sidebar was visible.
    #[must_use]
    pub fn sidebar_visible(&self) -> bool {
        self.sidebar_visible
    }

    /// Projects a [`wolf_core::TabState`] into its FFI accessor form.
    fn from_tab(t: &TabState) -> Self {
        Self {
            cwd: t.cwd.clone(),
            sidebar_visible: t.sidebar_visible,
        }
    }
}

/// Loads the saved application state from the platform state dir — the
/// PRE-START restore read the front-end performs before `wolf_ffi_start`.
///
/// A free FFI fn delegating to [`wolf_core::state_dir`] +
/// [`wolf_core::load_state`]: it NEVER creates or modifies the file and NEVER
/// opens the live store (the handle does that at start). An absent/corrupt
/// file or an unresolvable state dir all yield an empty window list —
/// infallible across the bridge, parallel to [`wolf_ffi_load_config`].
#[must_use]
pub fn wolf_ffi_load_state() -> FfiState {
    let state = wolf_core::state_dir()
        .and_then(|dir| wolf_core::load_state(&dir))
        .unwrap_or_default();
    FfiState {
        windows: state.windows.iter().map(FfiWindowState::from_window).collect(),
    }
}

// ---------------------------------------------------------------------------
// Write-side snapshot builder (free fns over the shared store + a scratch tree).
// ---------------------------------------------------------------------------

/// The write-side scratch tree the coordinator builds up window-by-window /
/// tab-by-tab before committing it whole. A process-wide `Mutex` (the builder is
/// only ever driven from the AppKit main thread, but the lock keeps the free fns
/// sound regardless), lazily initialized by [`scratch`].
static SCRATCH: OnceLock<Mutex<WolfState>> = OnceLock::new();

/// Returns the process-wide scratch-tree mutex, initializing it empty on first
/// use.
fn scratch() -> &'static Mutex<WolfState> {
    SCRATCH.get_or_init(|| Mutex::new(WolfState::default()))
}

/// Clears the write-side scratch tree, starting a fresh whole-tree snapshot.
///
/// The coordinator calls this, then one
/// [`wolf_ffi_state_snapshot_add_window`] per live window (each followed by its
/// tabs' [`wolf_ffi_state_snapshot_add_tab`] calls), then
/// [`wolf_ffi_state_snapshot_commit`].
pub fn wolf_ffi_state_snapshot_begin() {
    scratch()
        .lock()
        .expect("state scratch mutex poisoned")
        .windows
        .clear();
}

/// Pushes a window with the given geometry (points; AppKit bottom-left global
/// coords on macOS) and an EMPTY tab list onto the scratch tree.
///
/// Subsequent [`wolf_ffi_state_snapshot_add_tab`] calls attach their tabs to
/// this window (the most recently added one).
pub fn wolf_ffi_state_snapshot_add_window(x: f64, y: f64, width: f64, height: f64) {
    scratch()
        .lock()
        .expect("state scratch mutex poisoned")
        .windows
        .push(WindowState {
            x,
            y,
            width,
            height,
            tabs: Vec::new(),
        });
}

/// Pushes a tab (its `cwd` + `sidebar_visible`) onto the most recently added
/// window. A defensive no-op when no window has been added yet.
pub fn wolf_ffi_state_snapshot_add_tab(cwd: String, sidebar_visible: bool) {
    if let Some(window) = scratch()
        .lock()
        .expect("state scratch mutex poisoned")
        .windows
        .last_mut()
    {
        window.tabs.push(TabState {
            cwd,
            sidebar_visible,
        });
    }
}

/// Commits the scratch tree into the process-wide state store via a whole-tree
/// [`StateStore::set_snapshot`] (an identical tree is a no-op there, so a
/// redundant snapshot causes no disk churn). A no-op when no state dir resolved.
pub fn wolf_ffi_state_snapshot_commit() {
    let state = scratch()
        .lock()
        .expect("state scratch mutex poisoned")
        .clone();
    if let Some(store) = shared_store() {
        store.set_snapshot(state);
    }
}

/// Blocks until every pending state write has had its disk write attempted
/// (bounded by [`STATE_FLUSH_TIMEOUT`]). Returns whether everything pending was
/// flushed in time; `true` when no store exists (there is nothing to flush).
#[must_use]
pub fn wolf_ffi_state_flush() -> bool {
    shared_store().map_or(true, |store| store.flush(STATE_FLUSH_TIMEOUT))
}

// ---------------------------------------------------------------------------
// Opaque handles.
// ---------------------------------------------------------------------------

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

    /// Creates a new nvim TABPAGE (a new view of this PROJECT), optionally rooted
    /// at `cwd` via `:tcd` (an empty string means no `:tcd` — the empty-state
    /// view), and returns its stable HANDLE.
    ///
    /// Blocks briefly on the captive nvim (via [`WolfCore::create_tabpage`]).
    /// Returns 0 when the core has been shut down, so the caller drops the
    /// would-be view.
    #[must_use]
    pub fn create_tabpage(&self, cwd: String) -> i64 {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return 0;
        };
        let path = (!cwd.is_empty()).then(|| PathBuf::from(cwd));
        core.create_tabpage(path.as_deref())
    }

    /// The current tabpage's stable HANDLE via a blocking RPC (the first-view
    /// bind). 0 when the core has been shut down.
    ///
    /// Contrast [`active_tabpage`](Self::active_tabpage), the cheap snapshot gate.
    #[must_use]
    pub fn current_tabpage(&self) -> i64 {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return 0;
        };
        core.current_tabpage()
    }

    /// The CHEAP `Snapshot.current_tabpage` refocus gate (updated out-of-band via
    /// the `wolf_tabpage` notification), read from the snapshot with NO RPC. 0
    /// when the core has been shut down or the tabpage is not yet known.
    ///
    /// Mirrors [`active_win`](Self::active_win): a cheap `read_snapshot()` read
    /// the front-end polls to decide whether its tabpage is current (the refocus
    /// rebind barrier), vs the authoritative bind-time
    /// [`current_tabpage`](Self::current_tabpage) RPC.
    #[must_use]
    pub fn active_tabpage(&self) -> i64 {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return 0;
        };
        core.read_snapshot().current_tabpage
    }

    /// A tabpage's tab-local `:tcd` cwd (canonicalized), AUTHORITATIVE for a
    /// NON-current view's cwd. Blocks briefly on the captive nvim (via
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

    /// Makes tabpage `tab` current (the front-end drives every tabpage switch when
    /// a view becomes key). A no-op once the core has been shut down.
    pub fn set_current_tabpage(&self, tab: i64) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.set_current_tabpage(tab);
        }
    }

    /// Closes tabpage `tab` (a non-last view close; the shared nvim stays alive
    /// until its LAST view closes — the front-end shuts the core down then
    /// instead). A no-op once the core has been shut down.
    pub fn close_tabpage(&self, tab: i64) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.close_tabpage(tab);
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

    /// Cleanly shuts down the captive Neovim and tears down the runtime.
    ///
    /// Takes the core out of the handle (so a second call is a no-op) and consumes
    /// it via [`WolfCore::shutdown`]; any shutdown error is logged, not returned
    /// (the bridge signature is infallible).
    pub fn shutdown(&self) {
        // Drain any pending state writes BEFORE the core (and with it the
        // process's reason to live) goes away; bounded so shutdown never hangs.
        if let Some(store) = shared_store() {
            if !store.flush(STATE_FLUSH_TIMEOUT) {
                tracing::warn!("wolf-ffi: state flush timed out during shutdown");
            }
        }
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

    /// The GLOBAL recent-directories MRU, newest-first (capped at 20), as owned
    /// Strings; an empty `Vec` when the store is disabled/unreadable or the core
    /// has been shut down.
    ///
    /// Delegates to [`WolfCore::recent_dirs`], mapping each `PathBuf` via
    /// `to_string_lossy().into_owned()`. Read lazily at Open-Recent-menu open
    /// time by the macOS shell to build the File > Open Recent submenu.
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

    /// The `WolfTitlebar` theme group's background color (the macOS titlebar
    /// background), from `Snapshot.theme.titlebar.bg`.
    ///
    /// Follows the `default_fg`/`default_bg` unset convention: `set == false`
    /// when the group is absent or cleared, or once the core has been shut
    /// down. Read on each render wake by the macOS shell, which applies it to
    /// (or reverts) `NSWindow.backgroundColor`.
    #[must_use]
    pub fn theme_titlebar_bg(&self) -> FfiColor {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiColor::unset();
        };
        FfiColor::from_opt(core.read_snapshot().theme.titlebar.bg)
    }

    /// The `WolfTitlebar` theme group's foreground color, carried across the
    /// bridge for future title-text coloring (not applied on macOS this pass).
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

    /// Changes the current VIEW's working directory to `path` (an absolute
    /// filesystem path) via [`InputEvent::Chdir`] (`:tcd`, the tab-local cwd —
    /// each view/tabpage owns its own working directory). A no-op once the core
    /// has been shut down.
    pub fn chdir(&self, path: String) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.send_input(InputEvent::Chdir {
                path: PathBuf::from(path),
            });
        }
    }

    /// Closes the current split (the pane) via [`InputEvent::ClosePane`]
    /// (`:close`). A no-op once the core has been shut down.
    pub fn close_pane(&self) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.send_input(InputEvent::ClosePane);
        }
    }

    /// Opens a new empty buffer in the current window via
    /// [`InputEvent::NewBuffer`] (`:enew`). A no-op once the core has been shut
    /// down.
    pub fn new_buffer(&self) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.send_input(InputEvent::NewBuffer);
        }
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
}

/// Starts the core. `init_lua` and `cwd` are filesystem paths (mirroring
/// [`CoreConfig`]). Returns `None` (after logging) on any startup error.
///
/// A fresh wake token is minted (via [`NEXT_WAKE_TOKEN`]) and captured by the
/// core's [`WakeFn`] so wakes route back to THIS window. State persistence is no
/// longer wired per-core: the coordinator walks the whole window/tab tree and
/// commits it via the write-side snapshot builder free fns (see
/// [`wolf_ffi_state_snapshot_begin`]).
#[must_use]
pub fn wolf_ffi_start(
    init_lua: String,
    cwd: String,
    cols: u16,
    rows: u16,
) -> Option<WolfCoreHandle> {
    let cfg = CoreConfig {
        init_lua: PathBuf::from(init_lua),
        cols,
        rows,
        cwd: PathBuf::from(cwd),
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

// ---------------------------------------------------------------------------
// FfiCell: one resolved grid cell (opaque to Swift).
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// FfiLayoutIntent: a drained layout intent (opaque to Swift).
// ---------------------------------------------------------------------------

/// One drained layout intent (opaque to Swift, so a `Vec<FfiLayoutIntent>` is
/// supported — swift-bridge emits Vec support only for opaque types).
///
/// Flattens [`wolf_core::LayoutIntent`]'s two variants into a tagged struct: `kind`
/// discriminates (`intent_kind::SPLIT` / `FOCUS_DIR`); a SPLIT carries
/// `orientation` + `from` + `win`; a FOCUS_DIR carries `dir` + `from` (the unused
/// scalar fields are 0).
pub struct FfiLayoutIntent {
    kind: u8,
    orientation: u8,
    dir: u8,
    from: i64,
    win: i64,
}

impl FfiLayoutIntent {
    /// The intent kind (`intent_kind::SPLIT` / `FOCUS_DIR`).
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.kind
    }

    /// For a SPLIT: the orientation (`orientation::HORIZONTAL` / `VERTICAL`); 0 for
    /// a FOCUS_DIR.
    #[must_use]
    pub fn orientation(&self) -> u8 {
        self.orientation
    }

    /// For a FOCUS_DIR: the direction (`focus_dir::LEFT` / `RIGHT` / `UP` /
    /// `DOWN`); 0 for a SPLIT.
    #[must_use]
    pub fn dir(&self) -> u8 {
        self.dir
    }

    /// The origin window handle (current before the split / when the chord fired).
    #[must_use]
    pub fn from(&self) -> i64 {
        self.from
    }

    /// For a SPLIT: the new window handle; 0 for a FOCUS_DIR.
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
        }
    }
}

// ---------------------------------------------------------------------------
// FfiCommand: one drained user-triggered command (opaque to Swift).
// ---------------------------------------------------------------------------

/// One drained command (opaque to Swift, so a `Vec<FfiCommand>` is supported —
/// swift-bridge emits Vec support only for opaque types).
///
/// Carries just the [`command_kind`] discriminant of a [`wolf_core::Command`];
/// the front-end maps it to a native action.
pub struct FfiCommand {
    kind: u8,
}

impl FfiCommand {
    /// The command kind (`command_kind::NEW_WINDOW` .. `NEW_TAB`).
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.kind
    }

    /// Packs a [`wolf_core::Command`] into the bridge-friendly discriminant from
    /// the [`command_kind`] module. The exhaustive match makes a future variant
    /// reorder/addition a compile error.
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
        };
        Self { kind }
    }
}

// ---------------------------------------------------------------------------
// FfiCommandPalette / FfiCommandRow: the command-palette session (opaque to Swift).
// ---------------------------------------------------------------------------

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

    /// For a STATIC row: the [`command_kind`] discriminant to dispatch; 0 for a
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
        let command_kind = row.command.map_or(0, |c| FfiCommand::from_command(c).kind());
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

// ---------------------------------------------------------------------------
// FfiBufferPicker / FfiBufferRow: the buffer-picker session (opaque to Swift).
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// FfiWinInfo: one window's grid + handle + hidden flag (opaque to Swift).
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// FfiPaneStatus: one pane's statusbar strings (opaque to Swift).
// ---------------------------------------------------------------------------

/// One pane's statusbar strings — the base `filename` and the `display_path`
/// beneath it (opaque to Swift, so its Strings are read back via accessor
/// methods). A projection of [`wolf_core::PaneStatus`].
pub struct FfiPaneStatus {
    filename: String,
    display_path: String,
}

impl FfiPaneStatus {
    /// The base filename shown in the pane's statusbar (or `[No Name]` for an
    /// unnamed buffer).
    #[must_use]
    pub fn filename(&self) -> String {
        self.filename.clone()
    }

    /// The path shown beneath the filename (cwd-relative, `~`-prefixed, or
    /// absolute; empty for an unnamed buffer).
    #[must_use]
    pub fn display_path(&self) -> String {
        self.display_path.clone()
    }

    /// The empty-status fallback (`[No Name]` filename, empty display path), used
    /// when the core has been shut down.
    fn empty() -> Self {
        Self::from_pane_status(PaneStatus {
            filename: wolf_core::NO_NAME.to_string(),
            display_path: String::new(),
        })
    }

    /// Projects a [`wolf_core::PaneStatus`] into the opaque bridge type.
    fn from_pane_status(status: PaneStatus) -> Self {
        Self {
            filename: status.filename,
            display_path: status.display_path,
        }
    }
}

// ---------------------------------------------------------------------------
// FfiCmdline: the native command line (opaque to Swift).
// ---------------------------------------------------------------------------

/// The native command line (`ext_cmdline`), opaque to Swift.
///
/// A projection of [`wolf_core::Cmdline`] with its content chunks already
/// flattened to a single [`text`](FfiCmdline::text) String (OD1 — the cmdline is
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

// ---------------------------------------------------------------------------
// FfiPum / FfiPumItem: the completion popup (opaque to Swift).
// ---------------------------------------------------------------------------

/// The completion popup (`ext_popupmenu`), opaque to Swift.
///
/// A projection of [`wolf_core::Pum`]. When `anchor_grid == -1` the popup is
/// anchored to the command line and `anchor_col` is a BYTE offset into the
/// cmdline text (OD2 — the front-end shows `word` primarily with `kind` as a
/// small trailing hint).
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

// ---------------------------------------------------------------------------
// FfiToast / FfiToastLine / FfiToastChunk: the message toast (opaque to Swift).
// ---------------------------------------------------------------------------

/// The message toast (`ext_messages`), opaque to Swift.
///
/// A projection of [`wolf_core::Toast`]. Each chunk's colors are RESOLVED in
/// Rust against the highlight table at pack time (OD1 — message color is
/// load-bearing and must never be re-resolved in Swift).
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

// ---------------------------------------------------------------------------
// FfiFileSearch / FfiFileEntry: the file-picker search surface (opaque to Swift).
// ---------------------------------------------------------------------------

/// A live file-search session handle (opaque to Swift).
///
/// Wraps an `Option<`[`FileSearch`]`>` so [`start_file_search`] stays infallible
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
        wolf_core::tree::rel_path_for(&view.entries, std::path::Path::new(&abs))
            .unwrap_or_default()
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

// ---------------------------------------------------------------------------
// FfiFrame: an owned, resolved snapshot of the active grid.
// ---------------------------------------------------------------------------

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
    /// An absent grid yields [`FfiFrame::empty`] (`cols()`/`rows()` 0 — the
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

// ---------------------------------------------------------------------------
// Color / style / key helpers.
// ---------------------------------------------------------------------------

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

    // WolfCursor theme override: honored unconditionally (explicit app-level
    // opt-in, unlike the guarded mode-attr path below). A bg-only group keeps
    // the reverse-video glyph (the cell's bg).
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
