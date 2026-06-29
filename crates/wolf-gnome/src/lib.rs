//! `wolf-gnome` — the GTK4/Adwaita front-end for Wolf.
//!
//! Tier-1 scope: only the two pure, widget-free *seams* that can be verified
//! headlessly (no display server, no window):
//!
//! - [`gdk_key`]: translates a GDK keyval + modifier state into a toolkit-neutral
//!   [`wolf_core::KeyEvent`], honoring the shift-glyph contract.
//! - [`paint`]: paints a [`wolf_core::Snapshot`] onto a Cairo context (an offscreen
//!   `ImageSurface` in tests, a `DrawingArea` context in production).
//! - [`grid_size_for`]: the pure pixel-to-grid geometry used by the window's
//!   resize handler (testable without GTK).
//! - [`cursor`]: the pure, GTK-free cursor geometry ([`cursor::cursor_rect`]) and
//!   mode-aware color logic ([`cursor::cursor_colors`]), unit-tested headlessly.
//!
//! The windowed application now exists: [`app::run`] builds an
//! `adw::ApplicationWindow` with a header bar whose editor area is a **native
//! pane view** ([`pane_view`]) — a tree of nested `gtk::Paned`s with one
//! `gtk::DrawingArea` per Neovim window, driven by the pure [`layout`] `PaneTree`
//! — plus a bottom **cmdline strip** painting the outer grid (where the command
//! line and messages live under `ext_multigrid`). It attaches a live
//! [`wolf_core::WolfCore`], paints each pane's grid via [`paint`], routes keys via
//! [`gdk_key`], consumes layout intents (split / focus / close) on each wake, and
//! resizes each grid via [`grid_size_for`]. Files are opened via Neovim commands
//! (`:e`); there are no file dialogs. Theming, mouse/scroll, and a NATIVE command
//! line (`ext_cmdline` / `ext_messages` as Adw widgets) remain deferred — the
//! cmdline strip renders the grid-drawn command line instead. The seams above stay
//! free functions so they remain testable without a display.

pub mod app;
pub mod cursor;
pub mod geometry;
pub mod input;
pub mod layout;
pub mod pane_view;
pub mod picker;
pub mod render;
pub mod tree;
pub mod tree_view;

pub use geometry::grid_size_for;
pub use input::gdk_key;
pub use picker::is_picker_trigger;
pub use render::{paint, CellMetrics};
