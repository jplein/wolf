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
//! `wolf-core` must compile with **no GUI toolkit in its dependency tree**. This
//! is enforced by a documented `cargo tree` denylist (see the crate README).
//!
//! ## Modules
//!
//! - [`render`]: the toolkit-agnostic render-state model and shared snapshot.
//! - [`command`]: the canonical, GUI-free command registry ([`Command`]).
//! - [`command_mru`]: the global command-palette MRU cache (SQLite).
//! - [`buffer_picker`]: pure candidate assembly + ranking for the buffer picker.
//! - [`palette`]: pure candidate assembly + ranking for the command palette.
//! - [`pane_status`]: pure display-string math for the per-pane statusbar.
//! - [`layout`]: the Neovim->shell layout-intent types ([`LayoutIntent`]).
//! - [`files`]: the async streaming file-list traversal and `FileSearch` session.
//! - [`mru`]: the most-recently-used file cache (SQLite).
//! - [`recent_dirs`]: the global recent-directories MRU cache (SQLite).
//! - [`rank`]: pure fuzzy-match + ranking for the file picker.
//! - [`input`]: abstract input events and their translation to Neovim RPC.
//! - [`redraw`]: the `redraw` handler that folds events into the snapshot.
//! - [`nvim`]: spawning/supervising the captive, hermetic Neovim.
//! - [`tree`]: the pure, toolkit-free hierarchical projection of the flat file
//!   list into the sidebar's per-directory children (shared by all front-ends).
//! - [`api`]: the FFI-shaped front-end surface ([`WolfCore`]).
//! - [`config`]: the GUI-free user-preferences reader ([`WolfConfig`]).
//! - [`state`]: the GUI-free application-state persistence ([`StateStore`]).

pub mod api;
pub mod buffer_picker;
pub mod command;
pub mod command_mru;
pub mod config;
pub mod files;
pub mod input;
pub mod layout;
pub mod mru;
pub mod nvim;
pub mod palette;
pub mod pane_status;
pub mod rank;
pub mod recent_dirs;
pub mod redraw;
pub mod render;
pub mod state;
pub mod tree;

pub use api::{CoreConfig, CoreError, SnapshotReadGuard, WolfCore};
pub use buffer_picker::{rank_buffers, BufInfo, BufferRow};
pub use command::{Command, CommandSpec, MenuGroup, COMMAND_SPECS};
pub use command_mru::{CommandMru, COMMAND_MRU_LIMIT};
pub use config::{default_config, load_or_create, preferences_dir, Appearance, WolfConfig};
pub use files::{FileEntry, FileSearch, FileSearchReadGuard};
pub use input::{
    key_to_notation, InputEvent, Key, KeyEvent, MouseAction, MouseButton, MouseEvent,
    ScrollDirection, ScrollEvent,
};
pub use layout::{FocusDir, LayoutIntent, SplitOrientation};
pub use mru::{MruError, MruStore, MRU_LIMIT};
pub use palette::{rank_palette, PaletteRow, PaletteRowKind, PaletteWindow};
pub use pane_status::{pane_status, PaneStatus, NO_NAME};
pub use rank::{rank_matches, rank_matches_top_n, Match, MatchKey};
pub use recent_dirs::{RecentDirs, RECENT_DIRS_LIMIT};
pub use redraw::WakeFn;
pub use render::{
    Cell, Cursor, CursorShape, DefaultColors, Grid, HighlightTable, HlAttr, ModeInfo,
    ResolvedStyle, Rgb, SharedSnapshot, Snapshot, ThemeColors, ThemeGroup, Viewport, WinInfo,
};
pub use state::{load_state, state_dir, StateStore, TabState, WindowState, WolfState};
pub use tree::{
    ancestors_to_reveal, children_of, node_rel_path, rel_path_for, TreeIndex, TreeIndexCache,
    TreeNode,
};
