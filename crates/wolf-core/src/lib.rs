//! `wolf-core` — the platform-agnostic Rust core of Wolf, a Markdown editor that
//! embeds Neovim as its editing engine.
//!
//! Core spawns a hermetic, pinned `nvim --embed`, attaches as an `ext_linegrid`
//! and `ext_multigrid` UI, folds the entire `redraw` event stream into a render-state
//! [`Snapshot`](render::Snapshot) held behind one lock, accepts abstract
//! [`InputEvent`]s, and signals the front-end (via a single [`WakeFn`]) when a
//! new frame is ready. It draws no pixels and depends on no GUI toolkit.
//!
//! ## The one hard rule
//!
//! `wolf-core` must compile with **no GUI toolkit in its dependency tree**.
//! How it is checked, and why, is in
//! `notes/wolf-core-decisions.md#the-one-hard-rule`.
//!
//! ## Modules
//!
//! - [`render`]: the toolkit-agnostic render-state model and shared snapshot.
//! - [`command`]: the canonical, GUI-free command registry ([`Command`]).
//! - [`browser_preview`]: the lazily-started, loopback-only HTTP server that
//!   serves the rendered current Markdown buffer to the system web browser.
//! - [`command_mru`]: the global command-palette MRU cache (SQLite).
//! - [`buffer_picker`]: pure candidate assembly + ranking for the buffer picker.
//! - [`palette`]: pure candidate assembly + ranking for the command palette.
//! - [`mod@pane_status`]: pure display-string math for the per-pane statusbar.
//! - [`layout`]: the Neovim->shell layout-intent types ([`LayoutIntent`]).
//! - [`logging`]: the process-global stderr `tracing` subscriber
//!   ([`init_tracing`], installed once at startup, gated by `WOLF_LOG`).
//! - [`markdown`]: pure, GUI-free Markdown -> themed-HTML rendering for the
//!   preview pane (with forward-compatible per-line / per-100-column markers).
//! - [`files`]: the async streaming file-list traversal and `FileSearch` session.
//! - [`licenses`]: the compiled-in licence texts of the bundled fonts and of the
//!   linked libraries, plus the constant plain-text blocks the preview document's head
//!   and the front-ends' About surfaces carry.
//! - [`mru`]: the global, directory-filtered most-recently-used file cache
//!   (the `file_mru` table of `state_dir()/wolf.sqlite`).
//! - [`nav_history`]: the pure per-pane file navigation history (back/forward)
//!   backing the History menu and the two toolbar buttons.
//! - [`recent_dirs`]: the global recent-directories MRU cache (SQLite).
//! - [`rank`]: pure fuzzy-match + ranking for the file picker.
//! - [`input`]: abstract input events and their translation to Neovim RPC.
//! - [`redraw`]: the `redraw` handler that folds events into the snapshot.
//! - [`nvim`]: spawning/supervising the captive, hermetic Neovim.
//! - [`search`]: the ripgrep-backed, streaming content search and its
//!   `SearchSession` (sibling of [`files`], `rg --json` subprocess in place of the
//!   walk).
//! - [`search_tree`]: the pure, toolkit-free projection of the flat match list
//!   into per-file groups plus the balanced-context line-window display math.
//! - [`tree`]: the pure, toolkit-free hierarchical projection of the flat file
//!   list into the sidebar's per-directory children (shared by all front-ends).
//! - [`terminal`]: the terminal sessions core owns (pty, shell, libghostty-vt
//!   emulator) and the frames they publish.
//! - [`app_state`]: the application-state model, the rule that picks a
//!   directory's project state, and the restore-side helpers, reached as
//!   `wolf_core::app_state::…`.
//! - [`app_state_store`]: the application-state store ([`AppStateStore`]), the
//!   `window_state` and `project_state` tables of `state_dir()/wolf.sqlite`.
//! - [`api`]: the FFI-shaped front-end surface ([`WolfCore`]).
//! - [`config`]: the GUI-free user-preferences reader ([`WolfConfig`]).
//! - [`state`]: the platform state directory and the directories Wolf keeps under it.
//! - [`tab_id`]: the random ID of a tab ([`TabId`]), which its terminal sessions carry.
//! - [`file_open`]: the rules that place a path opened from outside Wolf: directory versus
//!   file, the project directory, and the pane.
//! - [`open_channel`]: the socket the `wolf` command reaches a running Wolf through, its
//!   client and the app's listener.

pub mod api;
pub mod app_state;
pub mod app_state_store;
pub mod browser_preview;
pub mod buffer_picker;
pub mod command;
pub mod command_mru;
pub mod config;
mod db;
pub mod file_open;
pub mod files;
pub mod input;
pub mod layout;
pub mod licenses;
pub mod liveness;
pub mod logging;
pub mod markdown;
pub mod mru;
pub mod nav_history;
pub mod nvim;
pub mod open_channel;
pub mod palette;
pub mod pane_status;
mod pump;
pub mod rank;
pub mod recent_dirs;
pub mod redraw;
pub mod render;
pub mod search;
pub mod search_tree;
pub mod state;
pub mod tab_id;
pub mod terminal;
pub mod tree;

pub use api::{CoreConfig, CoreError, SnapshotReadGuard, WindowCursor, WolfCore};
pub use app_state_store::{AppStateStore, PROJECT_STATE_LIMIT};
pub use browser_preview::BrowserPreview;
pub use buffer_picker::{rank_buffers, BufInfo, BufferRow};
pub use command::{Command, CommandSpec, MenuGroup, COMMAND_SPECS};
pub use command_mru::{CommandMru, COMMAND_MRU_LIMIT};
pub use config::{default_config, load_or_create, preferences_dir, Appearance, WolfConfig};
pub use file_open::{choose_pane, project_directory, OpenTarget, PaneChoice, PanePlacement};
pub use files::{FileEntry, FileSearch, FileSearchReadGuard};
pub use input::{
    key_to_notation, InputEvent, Key, KeyEvent, MouseAction, MouseButton, MouseEvent,
    ScrollDirection, ScrollEvent,
};
pub use layout::{FocusDir, LayoutIntent, SplitOrientation};
pub use licenses::{
    FontLicense, LibraryLicense, FONT_LICENSES, FONT_LICENSE_NOTICES, LIBRARY_LICENSES,
    LIBRARY_LICENSE_NOTICES,
};
pub use liveness::Liveness;
pub use logging::init_tracing;
pub use markdown::{
    code_blocks, render_html, render_html_with_highlights, theme_css, BlockHighlight,
    CodeBlockInfo, HighlightSpan, HighlightStyle, PreviewTheme, WidthMode, MARKER_COLUMN_STRIDE,
};
pub use mru::{MruStore, MRU_LIMIT};
pub use nav_history::{
    derive_target, disambiguate, MenuRow, NavHistory, NavMove, NavTarget, PaneHistory, MAX_ENTRIES,
    MAX_MENU_ROWS,
};
pub use open_channel::{
    open_socket_path, OpenChannelError, OpenClient, OpenListener, OpenReply, OpenRequest, OpenWait,
    TakenRequest, OPEN_SOCKET_NAME, OPEN_SOCKET_PATH_MAX,
};
pub use palette::{rank_palette, PaletteRow, PaletteRowKind, PaletteWindow};
pub use pane_status::{pane_status, PaneStatus, NO_NAME};
pub use rank::{rank_matches, rank_matches_top_n, Match, MatchKey};
pub use recent_dirs::{RecentDirs, RECENT_DIRS_LIMIT};
pub use redraw::WakeFn;
pub use render::{
    Cell, Cmdline, CmdlineChunk, Cursor, CursorShape, DefaultColors, Grid, HighlightTable, HlAttr,
    MarkdownCursor, Message, ModeInfo, PreviewInfo, Pum, PumItem, ResolvedStyle, Rgb,
    SharedSnapshot, Snapshot, TabFiles, ThemeColors, ThemeGroup, Toast, ToastChunk, Viewport,
    WinBuffer, WinInfo,
};
pub use search::{SearchMatch, SearchOptions, SearchReadGuard, SearchSession};
pub use search_tree::{context_window, file_label, project, ContextWindow, SearchFileGroup};
pub use state::{
    parsers_user_dir, runtime_user_dir, seed_runtime_user_dir, spell_dir, state_dir,
    INIT_USER_FILE_NAME,
};
pub use tab_id::{TabId, WOLF_TAB_ID_ENV};
pub use terminal::{
    terminfo_dir_from_env, TerminalCell, TerminalCellWidth, TerminalCommand, TerminalCursor,
    TerminalCursorStyle, TerminalFrame, TerminalId, TerminalMouseEvent, TerminalMouseKind,
    TerminalScrollEvent, TerminalScrollbar, TerminalTarget, TerminalUnderline,
    TERMINAL_HANGUP_GRACE, TERMINAL_PREFIX_TIMEOUT, TERMINAL_PUBLISH_INTERVAL,
    TERMINAL_RENDER_HOLD_MAX, TERMINFO_DIR_ENV,
};
pub use tree::{
    ancestors_to_reveal, children_of, node_rel_path, rel_path_for, TreeIndex, TreeIndexCache,
    TreeNode,
};
