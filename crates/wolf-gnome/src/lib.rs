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
//! - [`cell_at`]: the POINTER half of that same geometry, in the same file — a
//!   WIDGET-LOCAL pixel position plus the live grid extent to the grid cell
//!   `(row, col)` it lands on, clamped to the NEAREST EDGE cell (never rejected),
//!   which is what makes a drag past the pane edge keep extending.
//! - [`mouse`]: the pointer-translation rulebook, the analogue of [`gdk_key`] — a
//!   GDK button number + modifier state to a [`wolf_core::MouseButton`] plus the
//!   three modifier bools Neovim's mouse grammar carries
//!   ([`mouse::mouse_input`]); the per-`gdk::ScrollUnit` "one line step"
//!   threshold ([`mouse::scroll_threshold`]); the carried signed sub-threshold
//!   scroll remainder ([`mouse::ScrollAccumulator`]) and the per-pane
//!   remainder-plus-target-grid value that discards the carry when the grid
//!   changes ([`mouse::PaneScroll`]); and the signed step count to a
//!   [`wolf_core::ScrollDirection`] plus a POSITIVE repeat count
//!   ([`mouse::scroll_steps`]).
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
//! the pure [`layout`] `PaneTree` — plus a bottom **cmdline strip** showing the
//! command line and messages (which live outside the window grids under
//! `ext_multigrid`). It attaches a live [`wolf_core::WolfCore`], paints each pane's
//! grid via [`paint`], routes keys via [`gdk_key`] and pointer events via
//! [`mouse`] and [`cell_at`], consumes layout intents (split / focus / close) on
//! each wake, resizes each grid via [`grid_size_for`], and — through [`shell`] —
//! installs a `gio` action namespace shared by the GNOME primary menu and the
//! accelerator table, shows an empty state when no project is open, and drains
//! `WolfCore::drain_commands()` on every wake. Files are opened via Neovim commands
//! (`:e`), the picker/tree, or the menu's native Open File / Open Folder dialogs;
//! there are no save / save-as dialogs. Click-to-position, drag-to-select and
//! vertical scroll ARE wired (per-pane, in [`pane_view`]); horizontal scroll, the
//! non-primary buttons and kinetic scrolling are not, matching macOS. What remains
//! deferred is THEMING and a native command line as Adw widgets: the strip already
//! paints natively from the folded `Snapshot::cmdline` / `Snapshot::toast` (never
//! grid 1), but it draws a single row, with no `ext_popupmenu` completion popup and
//! no multi-line scrollable toast. The seams above stay free functions (plus two
//! plain value types) so they remain testable without a display.

pub mod actions;
pub mod app;
pub mod command_action;
pub mod cursor;
pub mod geometry;
pub mod input;
pub mod layout;
pub mod mouse;
pub mod pane_view;
pub mod picker;
pub mod render;
pub mod shell;
pub mod tree;
pub mod tree_view;
pub mod workspace;

pub use command_action::{ui_action_for, UiAction};
pub use geometry::{cell_at, grid_size_for};
pub use input::gdk_key;
pub use render::{paint, CellMetrics};
pub use workspace::{launch_decision, LaunchDecision};
