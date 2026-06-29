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
//! - [`layout`]: the Neovim->shell layout-intent types ([`LayoutIntent`]).
//! - [`files`]: the async streaming file-list traversal and `FileSearch` session.
//! - [`mru`]: the most-recently-used file cache (SQLite).
//! - [`rank`]: pure fuzzy-match + ranking for the file picker.
//! - [`input`]: abstract input events and their translation to Neovim RPC.
//! - [`redraw`]: the `redraw` handler that folds events into the snapshot.
//! - [`nvim`]: spawning/supervising the captive, hermetic Neovim.
//! - [`api`]: the FFI-shaped front-end surface ([`WolfCore`]).

pub mod api;
pub mod files;
pub mod input;
pub mod layout;
pub mod mru;
pub mod nvim;
pub mod rank;
pub mod redraw;
pub mod render;

pub use api::{CoreConfig, CoreError, SnapshotReadGuard, WolfCore};
pub use files::{FileEntry, FileSearch, FileSearchReadGuard};
pub use input::{
    key_to_notation, InputEvent, Key, KeyEvent, MouseAction, MouseButton, MouseEvent,
    ScrollDirection, ScrollEvent,
};
pub use layout::{FocusDir, LayoutIntent, SplitOrientation};
pub use mru::{MruError, MruStore, MRU_LIMIT};
pub use rank::{rank_matches, Match, MatchKey};
pub use redraw::WakeFn;
pub use render::{
    Cell, Cursor, CursorShape, DefaultColors, Grid, HighlightTable, HlAttr, ModeInfo,
    ResolvedStyle, Rgb, SharedSnapshot, Snapshot, Viewport, WinInfo,
};
