//! `wolf-gnome` — the GTK4/Adwaita front-end for Wolf.
//!
//! The pure, widget-free *seams* — everything that can be verified headlessly, with
//! no display server and no window:
//!
//! - [`gdk_key`]: a GDK keyval and modifier state to a [`wolf_core::KeyEvent`].
//! - [`mod@im_ui`]: what a window-level key event and an input-method `commit` become.
//! - [`paint`]: paints a [`wolf_core::Snapshot`] onto a Cairo context.
//! - [`mod@glyph_style`]: one cell's style to the decorations and colours Pango is asked for.
//! - [`grid_size_for`]: the pixel-to-grid geometry the window's resize handler uses.
//! - [`cell_at`]: a widget-local pointer position to the grid cell it lands on.
//! - [`mouse`]: GDK button and scroll events to Neovim's mouse grammar.
//! - [`cursor`]: the cursor's geometry and mode-aware colours.
//! - [`mod@cell_metrics_math`]: the cell height and baseline the configured `line-height` drives.
//! - [`mod@editor_inset`]: the chrome around a pane's text grid, as arithmetic.
//! - [`mod@frame_stats`]: the off-by-default frame-time measurement's buckets and report.
//! - [`mod@ready_seam`]: the test-only readiness file's predicate and contents.
//! - [`mod@scrollbar`]: a window's `win_viewport` projected onto a pane-local thumb.
//! - [`mod@statusbar_layout`]: the per-pane statusbar's geometry, font size and colours.
//! - [`mod@cmdline_ui`]: the command-line strip, the message toast and the completion popup.
//! - [`mod@config_seam`]: the rules for consuming a [`wolf_core::WolfConfig`].
//! - [`mod@init_lua`]: where the captive `runtime/nvim/init.lua` lives, as a function of values.
//! - [`mod@appearance`]: the configured appearance resolved against the desktop's dark/light.
//! - [`layout`] / [`tree`]: the pure pane tree and the hierarchical file-tree projection.
//! - [`mod@drawer`]: the terminal drawer's split and its focus bridge to the editor panes.
//! - [`ui_action_for`]: the total [`wolf_core::Command`] → [`UiAction`] mapping
//!   ([`command_action`]).
//! - [`mod@palette`]: the command palette's rows, its Open Recent candidates and their actions.
//! - [`mod@buffer_picker`]: the buffer picker's open guard, preselection and switch target.
//! - [`mod@search_overlay`]: the project search's guard, options, rows and status line.
//! - [`launch_decision`] / [`workspace::reroot_ready`] / [`workspace::reroot_decision`]:
//!   whether a project is open, where Neovim starts, and when the sidebar re-roots.
//! - [`mod@tree_gate`]: when the sidebar's file tree may refresh and rebuild.
//! - [`mod@refresh_coalescer`]: the sidebar's trailing refresh over a burst of batches.
//! - [`window_title`]: the working directory rendered as a window title ([`title`]).
//! - [`mod@pane_layout`]: the codec between the live pane layout and the persisted tree.
//! - [`restore_plans`]: one launch plan per saved window, and the write-side rules
//!   ([`state_seam`]).
//! - [`mod@liveness_ui`]: the crash and stall recovery rules.
//! - [`mod@nav_ui`]: which pane a Back or Forward press acts on, and when it may.
//! - [`mod@clipboard_ui`]: the Copy and Paste guards and what crosses each way.
//! - [`mod@preview_link`]: the URI arithmetic behind a link clicked inside the preview.
//! - [`mod@preview_ui`]: the Markdown preview's guards, reloads and statusbar lines.
//! - [`mod@preview_coalescer`]: when the preview's render runs during a typing burst.
//! - [`mod@tabs_ui`]: the native tabs one window's captive Neovim serves.
//! - [`mod@save_ui`]: the Save and Save As rules.
//! - [`actions`]: the action namespace and accelerator table as data.
//!
//! The windowed application restores its state at launch and persists it as it
//! runs: [`app::run`] activates a [`mod@coordinator`], which opens the ONE
//! `wolf_core::StateStore` over `state.json` before any widget exists and turns
//! the loaded tree into one [`RestorePlan`] per saved window (working directory,
//! window size, and one [`state_seam::TabPlan`] per saved tab). Each
//! [`mod@window`] realizes its first tab's saved layout through
//! `WolfCore::rebuild_layout`, materializes the rest on a later render wake — one
//! `WolfCore::create_tabpage` and one rebuild each — and, via [`persistence`],
//! gathers EVERY tab on every render wake and at `close-request`; the window
//! assembles its tabs into a subtree and the coordinator assembles those subtrees
//! into the tree and commits it.
//!
//! [`window::build`] builds an `adw::ApplicationWindow` with a
//! header bar whose editor area is a **native pane view** ([`pane_view`]) — a tree
//! of nested `gtk::Paned`s with one `gtk::Overlay` host per Neovim window, whose main
//! child is that pane's `gtk::DrawingArea` and whose only overlay child is a
//! `webkit6::WebView`, present exactly while the pane shows a Markdown preview,
//! driven by the pure [`layout`] `PaneTree` — plus a bottom **cmdline strip** showing the
//! command line, with the message toast and the completion popup as the pane view's
//! two overlay surfaces above it (the command line, messages and completions all live
//! outside the window grids under `ext_multigrid`, which is why these surfaces exist).
//! It attaches a live [`wolf_core::WolfCore`], paints each pane's grid via [`paint`],
//! routes keys via [`gdk_key`] and pointer events via [`mouse`] and [`cell_at`],
//! consumes layout intents (split / focus / close) on each wake, resizes each grid via
//! [`grid_size_for`], and — through [`shell`] —
//! installs a `gio` action namespace shared by the GNOME primary menu and the
//! accelerator table, shows an empty state when no project is open, and drains
//! `WolfCore::drain_commands()` on every wake. Files are opened via Neovim commands
//! (`:e`), the picker/tree, or the menu's native Open File / Open Folder dialogs, and
//! saved through `wolf_core::InputEvent::Save` / `InputEvent::SaveAs` — the primary
//! menu's Save and Save As items and `Ctrl+Shift+S`, with Save As (and Save on a
//! nameless buffer) presenting a native dialog of its own
//! ([`mod@save_ui`]). Click-to-position, drag-to-select and
//! vertical scroll ARE wired (per-pane, in [`pane_view`]); horizontal scroll, the
//! non-primary buttons and kinetic scrolling are not. Theming is
//! ONE-DIRECTIONAL: the configured `appearance` is resolved against the desktop's
//! light/dark preference and pushed into Neovim as `background=dark|light`, with
//! the GTK chrome's colour scheme set to match ([`mod@appearance`]) — but no OS
//! COLOUR is read into Neovim's highlight groups. What remains
//! deferred is a native command line as Adw widgets: the one-row strip,
//! the message toast and the `ext_popupmenu` completion popup are all painted from
//! the folded `Snapshot::cmdline` / `Snapshot::toast` / `Snapshot::pum`, never from
//! grid 1, with the toast and the popup as overlay surfaces that cost the pane area
//! no height — so a message taller than its budget scrolls under a pointer gesture
//! rather than growing the strip. The seams above stay free functions (plus two
//! plain value types) so they remain testable without a display.

pub mod about;
pub mod actions;
pub mod app;
pub mod appearance;
pub mod buffer_picker;
pub mod cell_metrics_math;
pub mod clipboard_ui;
pub mod cmdline_ui;
pub mod command_action;
pub mod config_seam;
pub mod coordinator;
pub mod crash_overlay;
pub mod cursor;
pub mod drawer;
pub mod editor_inset;
pub mod frame_metrics;
pub mod frame_stats;
pub mod geometry;
pub mod glyph_style;
pub mod im_ui;
pub mod init_lua;
pub mod input;
pub mod layout;
pub mod liveness_ui;
pub mod mouse;
pub mod nav_ui;
mod overlay_style;
pub mod palette;
pub mod pane_layout;
pub mod pane_view;
pub mod persistence;
pub mod picker;
pub mod preview_coalescer;
pub mod preview_link;
pub mod preview_ui;
pub mod ready_seam;
pub mod refresh_coalescer;
pub mod render;
pub mod save_ui;
pub mod scrollbar;
pub mod search_overlay;
pub mod shell;
pub mod state_seam;
pub mod statusbar_layout;
pub mod tab_view;
pub mod tabs_ui;
pub mod title;
pub mod tree;
pub mod tree_gate;
pub mod tree_view;
pub mod window;
pub mod workspace;

pub use command_action::{ui_action_for, UiAction};
pub use geometry::{cell_at, grid_size_for};
pub use input::gdk_key;
pub use liveness_ui::{
    liveness_ui, reopen_order, restart_button_rect, LivenessUi, OverlayMode, RestartOutcome,
};
pub use render::{paint, CellMetrics};
pub use state_seam::{restore_plans, RestorePlan};
pub use title::{window_title, WINDOW_TITLE_FALLBACK};
pub use workspace::{launch_decision, LaunchDecision};
