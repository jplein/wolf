//! `wolf-gnome` — the GTK4/Adwaita front-end for Wolf.
//!
//! The pure, widget-free *seams* — everything that can be verified headlessly, with
//! no display server and no window:
//!
//! - [`gdk_key`]: translates a GDK keyval + modifier state into a toolkit-neutral
//!   [`wolf_core::KeyEvent`], honoring the shift-glyph contract.
//! - [`paint`]: paints a [`wolf_core::Snapshot`] onto a Cairo context (an offscreen
//!   `ImageSurface` in tests, a `DrawingArea` context in production).
//! - [`grid_size_for`]: the pure pixel-to-grid geometry used by the window's
//!   resize handler (testable without GTK).
//! - [`cursor`]: the pure, GTK-free cursor geometry ([`cursor::cursor_rect`]) and
//!   mode-aware color logic ([`cursor::cursor_colors`]).
//! - [`layout`] / [`tree`]: the pure pane tree and the hierarchical file-tree
//!   projection.
//! - [`ui_action_for`]: the TOTAL [`wolf_core::Command`] → [`UiAction`] mapping the
//!   command drain dispatches through ([`command_action`]).
//! - [`launch_decision`] / [`workspace::reroot_ready`]: whether a project is open,
//!   where the captive Neovim starts, and when an Open Folder re-root has settled
//!   ([`workspace`]).
//! - [`actions`]: the action namespace as data — [`actions::ACTION_TABLE`],
//!   [`actions::MENU_SECTIONS`], and the [`actions::parse_accel`] /
//!   [`actions::forbidden_reason`] checker that mechanically enforces "no Neovim
//!   key becomes unreachable".
//!
//! The windowed application: [`app::run`] builds an `adw::ApplicationWindow` with a
//! header bar whose editor area is a **native pane view** ([`pane_view`]) — a tree
//! of nested `gtk::Paned`s with one `gtk::DrawingArea` per Neovim window, driven by
//! the pure [`layout`] `PaneTree` — plus a bottom **cmdline strip** painting the
//! outer grid (where the command line and messages live under `ext_multigrid`). It
//! attaches a live [`wolf_core::WolfCore`], paints each pane's grid via [`paint`],
//! routes keys via [`gdk_key`], consumes layout intents (split / focus / close) on
//! each wake, resizes each grid via [`grid_size_for`], and — through [`shell`] —
//! installs a `gio` action namespace shared by the GNOME primary menu and the
//! accelerator table, shows an empty state when no project is open, and drains
//! `WolfCore::drain_commands()` on every wake. Files are opened via Neovim commands
//! (`:e`), the picker/tree, or the menu's native Open File / Open Folder dialogs;
//! there are no save / save-as dialogs. Theming, mouse/scroll, and a NATIVE command
//! line (`ext_cmdline` / `ext_messages` as Adw widgets) remain deferred — the
//! cmdline strip renders the grid-drawn command line instead. The seams above stay
//! free functions so they remain testable without a display.

pub mod actions;
pub mod app;
pub mod command_action;
pub mod cursor;
pub mod geometry;
pub mod input;
pub mod layout;
pub mod pane_view;
pub mod picker;
pub mod render;
pub mod shell;
pub mod tree;
pub mod tree_view;
pub mod workspace;

pub use command_action::{ui_action_for, UiAction};
pub use geometry::grid_size_for;
pub use input::gdk_key;
pub use render::{paint, CellMetrics};
pub use workspace::{launch_decision, LaunchDecision};
