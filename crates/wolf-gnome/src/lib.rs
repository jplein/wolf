//! `wolf-gnome` — the GTK4/Adwaita front-end for Wolf.
//!
//! The pure, widget-free *seams* — everything that can be verified headlessly, with
//! no display server and no window:
//!
//! - [`gdk_key`]: translates a GDK keyval + modifier state into a toolkit-neutral
//!   [`wolf_core::KeyEvent`], honoring the shift-glyph contract.
//! - [`paint`]: paints a [`wolf_core::Snapshot`] onto a Cairo context (an offscreen
//!   `ImageSurface` in tests, a `DrawingArea` context in production).
//! - [`mod@glyph_style`]: the DECORATION half of that paint, as data — one cell's
//!   [`wolf_core::ResolvedStyle`] to the underline, undercurl and strikethrough
//!   Pango is asked for and the colour the first two take
//!   ([`glyph_style::glyph_decoration`]), plus the 8-bit to 16-bit channel widening
//!   Pango's colour attributes need ([`glyph_style::pango_channels`]). Pango's
//!   underline attribute is SINGLE-VALUED per run, so `underline` and `undercurl`
//!   cannot coexist and undercurl wins; the wave is Pango's own
//!   `Underline::Error`, never a hand-stroked Cairo path.
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
//! - [`mod@cell_metrics_math`]: the cell-metrics arithmetic the configured
//!   `line-height` drives — the cell height as a function of the font size alone
//!   ([`cell_metrics_math::cell_height`]) and the baseline that centres the glyph
//!   box within it ([`cell_metrics_math::baseline`]). A port of
//!   `WolfKeyMap/CellMetricsMath.swift`, with two deliberate divergences named in
//!   that module's docs.
//! - [`mod@editor_inset`]: the chrome around a pane's text grid, as arithmetic —
//!   the pane-local rectangle the grid paints into
//!   ([`editor_inset::grid_rect`]), the pane-to-grid coordinate map every pointer
//!   event routes through ([`editor_inset::grid_local`]), the grid size that
//!   survives the chrome ([`editor_inset::grid_size_for_pane`], a composition on
//!   top of the unchanged [`grid_size_for`]), and the pane content floor that
//!   still leaves one whole cell ([`editor_inset::min_pane_size`]). The
//!   content-width cap lives here too: [`editor_inset::MAX_GRID_COLS`] columns,
//!   carried as an [`editor_inset::ColumnCap`] that both the rectangle's width and
//!   the grid's column count are reduced by, and selected from a window's
//!   `b:wolf_full_width` flag in the one place that flag becomes geometry
//!   ([`editor_inset::column_cap`]). The centring offset the cap produces rides in
//!   the rectangle's `x`, so every consumer reads one origin.
//! - [`mod@scrollbar`]: the projection of a Neovim window's `win_viewport` onto a
//!   pane-local thumb rectangle ([`scrollbar::scrollbar_thumb`]) and that thumb's
//!   colour over the editor's own background
//!   ([`scrollbar::scrollbar_thumb_color`]). The thumb sits in the pane's right
//!   PADDING gutter rather than at the grid rectangle's right edge, so it covers no
//!   glyph and costs the grid no column; there is no track, and no widget.
//! - [`mod@statusbar_layout`]: the per-pane statusbar rulebook — the strip's
//!   height and its two line boxes ([`statusbar_layout::statusbar_layout`]), the
//!   second line's font size ([`statusbar_layout::secondary_font_size`]), and the
//!   colour-and-weight rule that is the WHOLE active/inactive treatment
//!   ([`statusbar_layout::statusbar_text`]), since the pane body is painted
//!   identically either way.
//! - [`mod@cmdline_ui`]: the rulebook behind the command-line strip and the two
//!   overlay surfaces above it — the column a BYTE offset into the command line
//!   lands on, which the caret and the popup's cmdline anchor share so the two
//!   cannot drift apart ([`cmdline_ui::cmdline_columns`]); the toast's height
//!   budget, the slice that budget shows, the offset a scroll gesture's step count
//!   lands on and the clamp under it, the width that slice measures, and the offset
//!   carried into the next frame ([`cmdline_ui::toast_budget_rows`],
//!   [`cmdline_ui::toast_view`], [`cmdline_ui::toast_scroll_next`],
//!   [`cmdline_ui::toast_scroll_clamp`], [`cmdline_ui::toast_width_cols`],
//!   [`cmdline_ui::carried_scroll`]); whether the surface is on screen at all
//!   ([`cmdline_ui::toast_is_shown`]) and whether a prompt that was showing has
//!   stopped ([`cmdline_ui::prompt_answered`]); and the completion popup's
//!   selection, visible slice, row label and width in cells
//!   ([`cmdline_ui::pum_selection`], [`cmdline_ui::pum_view`],
//!   [`cmdline_ui::pum_item_label`], [`cmdline_ui::pum_width_cols`]).
//! - [`mod@config_seam`]: the rules for CONSUMING a [`wolf_core::WolfConfig`] —
//!   the `WOLF_FONT_PT`-over-`font-size` precedence
//!   ([`config_seam::resolve_font_pt`], which takes the override as an argument so
//!   the rule is testable without the process environment).
//! - [`mod@appearance`]: the OS DARK/LIGHT rulebook — the TOTAL
//!   `(`[`wolf_core::Appearance`]`, os_dark)` →
//!   [`appearance::ResolvedAppearance`] resolution
//!   ([`appearance::appearance_decision`]), which answers all three of what
//!   `background` is pushed into Neovim, what colour scheme the GTK chrome is set
//!   to, and whether a live desktop toggle is followed at all. The desktop's
//!   reading is INJECTED rather than read here, because
//!   `adw::StyleManager::default()` asserts an initialized main thread;
//!   [`appearance::apply_at_launch`] is the glue that makes that one call and
//!   carries the answer out.
//! - [`layout`] / [`tree`]: the pure pane tree and the hierarchical file-tree
//!   projection.
//! - [`ui_action_for`]: the TOTAL [`wolf_core::Command`] → [`UiAction`] mapping the
//!   command drain dispatches through ([`command_action`]).
//! - [`mod@palette`]: the command-palette rulebook — the candidate list this
//!   front-end may show ([`palette::palette_rows`], which ranks unbounded and
//!   applies its cap after filtering); the recent directories it may offer as
//!   Open Recent rows, which refuse the window's own working directory, a path
//!   whose directory has gone, and a relative one
//!   ([`palette::open_recent_candidates`]); and the TOTAL
//!   [`wolf_core::PaletteRowKind`] → [`palette::PaletteAction`] classification that
//!   keeps a row `wolf-gnome` cannot perform out of the list, and turns a folder row
//!   into a [`palette::PaletteAction::OpenRecentFolder`] carrying a directory and no
//!   window target ([`palette::palette_action_for`]).
//! - [`mod@buffer_picker`]: the buffer-picker rulebook — the TOTAL
//!   `Option<`[`wolf_core::Liveness`]`>` → [`buffer_picker::OpenDecision`] guard
//!   that refuses to issue the candidate snapshot's BLOCKING RPC unless the core
//!   is alive ([`buffer_picker::open_decision`]), the first-open preselection of
//!   ROW 1 — the most-recently-used OTHER buffer, so a bare `Enter` is a switch
//!   rather than a no-op — and the `(win, bufnr)` pair `Enter` resolves to
//!   ([`buffer_picker::switch_target`], which refuses an active window of `0`
//!   exactly as `wolf-ffi` does), plus the row's display label. Selection movement
//!   and activation come from [`mod@picker`], whose `next_selection`,
//!   `restored_index` and `activation_index` all four overlays share.
//! - [`mod@search_overlay`]: the PROJECT-SEARCH rulebook — the TOTAL
//!   `(Option<`[`wolf_core::Liveness`]`>, project_open)` →
//!   [`search_overlay::SearchOpenDecision`] guard, whose no-project arm is what
//!   stops a `--hidden` ripgrep run being rooted at the user's home directory
//!   ([`search_overlay::open_decision`]); the fixed
//!   [`wolf_core::SearchOptions`] this front-end ships — literal,
//!   case-insensitive, every file, with no toggles and no `ignorecase()` read
//!   ([`search_overlay::search_options_for`]); the FLAT row assembly over
//!   [`wolf_core::project`] / [`wolf_core::file_label`] /
//!   [`wolf_core::context_window`], capped and ellipsised
//!   ([`search_overlay::result_rows`]); the TWO-WAY `Enter` decision — run the
//!   search, or open the selected row — which refuses an empty query outright
//!   ([`search_overlay::enter_action`]); the `(path, line, col)` triple that makes a
//!   result row a LOCATION rather than a file
//!   ([`search_overlay::activation_target`]); and the six-arm status line
//!   ([`search_overlay::search_status`]), which is the only surface core's
//!   `WOLF_RG_BIN unset` error ever reaches. Selection movement and activation come
//!   from [`mod@picker`], as they do for the other three.
//! - [`launch_decision`] / [`workspace::reroot_ready`] /
//!   [`workspace::reroot_decision`]: whether a project is open, where the captive
//!   Neovim starts, and whether one settle moves the sidebar's root
//!   ([`workspace`]). The launch rule is process cwd first, then the cwd restored
//!   from `state.json`, under one shared predicate. The re-root rule's authority is
//!   the directory the sidebar is CURRENTLY rooted at, compared against the one nvim
//!   reports, so a typed `:cd` moves it while a satisfied front-end request cannot
//!   move it twice; [`workspace::root_label`] is the text its synthetic root row
//!   draws.
//! - [`mod@tree_gate`]: the sidebar's refresh gate — a refresh refused while the
//!   sidebar is hidden and owed exactly ONE catch-up when it returns, and a rebuild
//!   allowed once per snapshot VERSION, which is what stops a current-file change
//!   collapsing manual expansion ([`tree_gate::TreeRefreshGate`]). A port of
//!   `WolfKeyMap/TreeRefreshGate.swift`.
//! - [`mod@refresh_coalescer`]: the sidebar's TRAILING refresh — a burst of
//!   traversal batches armed into one fire per
//!   [`refresh_coalescer::REFRESH_INTERVAL`], with a generation token that turns a
//!   cancelled fire into a genuine no-op ([`refresh_coalescer::RefreshCoalescer`]).
//!   A port of `WolfKeyMap/PickerRefreshCoalescer.swift`; unlike macOS the picker
//!   deliberately does not share it.
//! - [`window_title`]: the working directory rendered as a window title, with the
//!   home prefix abbreviated to `~` on a full path-SEGMENT boundary and an
//!   ALWAYS-INJECTED `home` (never an implicit `$HOME` read) — [`title`].
//! - [`mod@pane_layout`]: the codec between the live [`layout`] `PaneLayout` (leaf
//!   IDS) and the persisted `wolf_core::PaneNode`, whose leaves carry a
//!   [`pane_layout::LeafContent`] — a RAW absolute path plus the flag saying the pane
//!   shows a Markdown preview of it. Includes [`pane_layout::is_empty_leaf`] — the
//!   CONTENT predicate that decides whether a layout is persisted at all, and which
//!   must never be an arity test.
//! - [`restore_plan`]: the whole launch plan (cwd, empty state, saved pane tree,
//!   window size) plus the write-side rules — which snapshots may be committed,
//!   what cwd is persisted, and the one-window/one-tab shape ([`state_seam`]).
//! - [`mod@liveness_ui`]: the CRASH/STALL recovery rulebook — the TOTAL
//!   [`wolf_core::Liveness`] → `(overlay mode, auto_restart, paint)` mapping the
//!   wake drain gates on ([`liveness_ui()`]), the message each overlay mode
//!   carries ([`liveness_ui::overlay_message`]), what follows a
//!   `WolfCore::restart_nvim()` call ([`liveness_ui::restart_outcome`]), the
//!   Tier-1 reopen order with the ACTIVE file LAST so it lands focused
//!   ([`reopen_order`]), and the derived geometry of the "Restart Neovim" button
//!   inside the overlay ([`restart_button_rect`]) — one function shared by the
//!   widget builder and the cage smoke so the two cannot drift.
//! - [`mod@nav_ui`]: the PER-PANE NAVIGATION rulebook — the one place the "which
//!   window does a Back/Forward press act on" question is answered
//!   ([`nav_ui::nav_query_win`], which refuses an active window of `0` exactly as
//!   [`buffer_picker::switch_target`] does); the TOTAL
//!   `Option<`[`wolf_core::Liveness`]`>` → [`nav_ui::NavPress`] guard that refuses
//!   to issue the BLOCKING history move unless the core is alive
//!   ([`nav_ui::nav_press`]); and the sensitivity of the two toolbar buttons over
//!   core's per-pane availability reading ([`nav_ui::nav_buttons`], which answers
//!   both-insensitive for every input the press path would refuse, never the
//!   converse).
//! - [`mod@clipboard_ui`]: the CLIPBOARD rulebook — two TOTAL
//!   `Option<`[`wolf_core::Liveness`]`>` guards, one refusing the Copy yank's
//!   BLOCKING RPC ([`clipboard_ui::copy_decision`]) and one refusing a Paste for a
//!   reason that is NOT about blocking ([`clipboard_ui::paste_decision`]: nothing
//!   on that path blocks, but a paste queued behind a crash or stall overlay would
//!   land in a RESTARTED Neovim minutes later). Plus what may cross in each
//!   direction ([`clipboard_ui::clipboard_write`], whose `None` means write NOTHING
//!   rather than clear, and [`clipboard_ui::paste_payload`]).
//! - [`mod@preview_link`]: the URI arithmetic behind a link CLICKED inside the
//!   preview — the seven-row classification of a resolved link target into an
//!   external launch, a same-document scroll, a repointed pane or a refusal
//!   ([`preview_link::classify`], [`preview_link::PreviewLink`]); the `file:` base
//!   URI a document is loaded against, which names the FILE and carries no trailing
//!   slash because a relative link REPLACES the last segment, and which is `None`
//!   for an empty or relative source path rather than a synthesized cwd
//!   ([`preview_link::file_uri_for`]); and the one-line scroll script, encoded ONCE
//!   because nothing re-parses it ([`preview_link::scroll_into_view_js`]). A port of
//!   `WolfKeyMap/PreviewLinkSeam.swift` with two deliberate divergences named in
//!   that module's docs. It owns URI arithmetic and nothing else; the liveness gates
//!   and the load strategy are [`mod@preview_ui`]'s.
//! - [`mod@preview_ui`]: the MARKDOWN PREVIEW rulebook — where a Preview press opens
//!   its pane and where its inverse lands the source editor, two rules whose reuse
//!   branches differ ONLY in the polarity of `active_is_preview`
//!   ([`preview_ui::preview_placement`], [`preview_ui::edit_placement`]); five
//!   TOTAL `Option<`[`wolf_core::Liveness`]`>` guards over the five BLOCKING RPCs
//!   this adds — the two presses, a followed preview link, Preview in Browser, and
//!   the per-wake render ([`preview_ui::preview_press`], [`preview_ui::edit_press`],
//!   [`preview_ui::preview_file_press`], [`preview_ui::browser_press`],
//!   [`preview_ui::may_render`]); whether a bound web view must be reloaded, keyed
//!   on the source buffer's number AND its `changedtick` because two freshly opened
//!   files share a tick ([`preview_ui::needs_reload`], [`preview_ui::PreviewLoad`]);
//!   whether a rendered document may be swapped in place or must go through a full
//!   load, which is what preserves the reader's scroll offset across an edit
//!   ([`preview_ui::document_update`]); what may be handed to the URI launcher
//!   ([`preview_ui::browser_url`], whose `None` means launch NOTHING); and a preview
//!   pane's two statusbar lines, which come from its own source path rather than
//!   from `WolfCore::pane_status` ([`preview_ui::preview_status`]).
//! - [`actions`]: the action namespace as data — [`actions::ACTION_TABLE`],
//!   [`actions::MENU_SECTIONS`], and the [`actions::parse_accel`] /
//!   [`actions::forbidden_reason`] checker that mechanically enforces "no Neovim
//!   key becomes unreachable".
//!
//! The windowed application restores its state at launch and persists it as it
//! runs: [`app::run`] opens a `wolf_core::StateStore` over `state.json` before any
//! widget exists, turns it into a [`RestorePlan`] (working directory, window size,
//! the tab's saved pane-split tree), realizes the saved layout through
//! `WolfCore::rebuild_layout`, and — via [`persistence`] — writes the live tree
//! back on every render wake and authoritatively at `close-request`.
//!
//! [`app::run`] builds an `adw::ApplicationWindow` with a
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
//! (`:e`), the picker/tree, or the menu's native Open File / Open Folder dialogs;
//! there are no save / save-as dialogs. Click-to-position, drag-to-select and
//! vertical scroll ARE wired (per-pane, in [`pane_view`]); horizontal scroll, the
//! non-primary buttons and kinetic scrolling are not, matching macOS. Theming is
//! ONE-DIRECTIONAL: the configured `appearance` is resolved against the desktop's
//! light/dark preference and pushed into Neovim as `background=dark|light`, with
//! the GTK chrome's colour scheme set to match ([`mod@appearance`]) — but no OS
//! COLOUR is read into Neovim's highlight groups, so macOS's `nvim_set_hl` push has
//! no counterpart here. What remains
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
pub mod crash_overlay;
pub mod cursor;
pub mod editor_inset;
pub mod geometry;
pub mod glyph_style;
pub mod input;
pub mod layout;
pub mod liveness_ui;
pub mod mouse;
pub mod nav_ui;
pub mod palette;
pub mod pane_layout;
pub mod pane_view;
pub mod persistence;
pub mod picker;
pub mod preview_link;
pub mod preview_ui;
pub mod refresh_coalescer;
pub mod render;
pub mod scrollbar;
pub mod search_overlay;
pub mod shell;
pub mod state_seam;
pub mod statusbar_layout;
pub mod title;
pub mod tree;
pub mod tree_gate;
pub mod tree_view;
pub mod workspace;

pub use command_action::{ui_action_for, UiAction};
pub use geometry::{cell_at, grid_size_for};
pub use input::gdk_key;
pub use liveness_ui::{
    liveness_ui, reopen_order, restart_button_rect, LivenessUi, OverlayMode, RestartOutcome,
};
pub use render::{paint, CellMetrics};
pub use state_seam::{restore_plan, RestorePlan};
pub use title::{window_title, WINDOW_TITLE_FALLBACK};
pub use workspace::{launch_decision, LaunchDecision};
