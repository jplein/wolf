# wolf-gnome/src

The GTK front-end's modules, one entry each. The pure seams here are the Linux counterparts of the ones in [`wolf-macos/Sources/WolfKeyMap/`](../../wolf-macos/Sources/WolfKeyMap/README.md). Each file's module doc carries its own rules and their reasons; an entry here keeps what a reader of one file would miss.

**Read [wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md) first**, with the topic files it links: they own every constraint that spans more than one file here, and most of this directory's rules exist because of one.

## Files

#### `about.rs`

Purpose: the About dialog: a pure text seam over the bundled fonts' and linked libraries' licence notices, and the toplevel window that shows it. It discharges their attribution obligations, so its content has legal weight. Its pure seam is `about_text`.

#### `actions.rs`

Purpose: the action namespace as data, one row per `gio` action, with the chord matcher `action_for_key` and the invariant checker. `ACTION_TABLE` and `MENU_SECTIONS` are the one table the menu, the accelerators and the command dispatch read ([The accelerator namespace](../../../notes/wolf-gnome/the-accelerator-namespace.md)). Its other pure seams are `parse_accel`, `ctrl_fold` and `forbidden_reason`.

**Decisions**

- `action_for_key` compares its three modifiers exactly and ignores every other bit: intercepting a Super-contaminated chord beats forwarding one Neovim mangles.

**Gotchas**

- Promoting a reserved row means flipping `status` and declaring the byte a `Ctrl+<symbol>` chord folds onto, or the chord stays inert or the checker refuses it.
- A row's `win.` or `app.` prefix decides who installs it: [`coordinator.rs`](#coordinatorrs) the `app.` rows once per process, each window's [`shell.rs`](#shellrs) the `win.` ones.

#### `app.rs`

Purpose: the entry point. The first activate builds the coordinator, installs its `app.` actions and restores the session; a later one opens a fresh window. `run` hands GTK an empty argv, since a path arrives over the open channel ([§2](../../../specs/cli-and-file-open.md#2-the-channel)).

**Decisions**

- Every activate sets an unset `gtk-xft-dpi` to `FALLBACK_XFT_DPI`: with no portal it is unset, and WebKitGTK draws a preview created on an unallocated host blank for good ([measured](../../../notes/crates/wolf-gnome/src/app.rs.md#pinning-gtk-xft-dpi)). Once fired, the pin outranks a dpi a late portal or `xsettingsd` supplies.

**Gotchas**

- `XFT_DPI_PINNED_PREFIX` begins the line the pin prints when it fires; [`preview.rs`](../tests/smoke_headless/README.md#previewrs) matches its own copy, so retype a rename there.

#### `appearance.rs`

Purpose: what a configured `appearance` resolves to against the desktop's preference (`appearance_decision`, the seam), and the glue applying it to each tab at launch (`apply_at_launch`).

**Decisions**

- A forced appearance takes `ForceDark`/`ForceLight`, never `PreferDark`/`PreferLight`, which yield to the desktop preference the config line overrides.
- `apply_at_launch`'s step order is load-bearing: the notify handler is armed after the launch-time set, and pushes `background` without re-setting the scheme, since re-applying a property from its own notify handler loops. It holds the core as a `Weak`, so an armed handler cannot keep the core alive past the close.

**Gotchas**

- What `set_color_scheme` reaches beyond the chrome, and the GTK version that ends it: [The preview's light and dark](../../../notes/wolf-gnome-decisions.md#the-previews-light-and-dark).

#### `buffer_picker.rs`

Purpose: the buffer-picker overlay, a search entry over the selected tab's listed buffers ranked by `wolf_core::rank_buffers`; a row switches the active window to its buffer. The pure seams are `open_decision`, `initial_selection`, `switch_target` and `row_label`.

**Decisions**

- Candidates are snapshotted once at open, by a blocking RPC, so [`picker.rs`](#pickerrs)'s wake bridge must not be added for symmetry. A refusal returns before any widget is touched, and nothing is shown until the snapshot returns.

#### `cell_metrics_math.rs`

Purpose: the pure cell-metrics arithmetic the configured `line-height` drives. `cell_height` answers `round(font_size * percent / 100)` for a positive finite percentage, whatever the raw metrics, and `ceil(ascent + descent + leading)` otherwise; `baseline` splits the leftover space evenly.

**Decisions**

- It lives outside [`render.rs`](#renderrs) because `for_font` needs a live Cairo context, so a rule inside it could only run in the golden-render target.
- It is unit-agnostic; this crate calls it in device pixels, which Cairo paints in.
- Two guards, for degenerate inputs only: the percentage must be finite (a bare `> 0` accepts `+inf`), and a non-finite or non-positive result floors to `MIN_CELL_PX` (unfloored, it can be `0` or `NaN`).

#### `clipboard_ui.rs`

Purpose: the pure rulebook for Copy and Paste: `copy_decision`, `paste_decision`, `clipboard_write` and `paste_payload`. [`shell.rs`](#shellrs) owns the `gdk::Clipboard` and the read, and captures a paste's target at the press, so a focus change during the read cannot redirect it.

#### `cmdline_ui.rs`

Purpose: the pure rulebook behind the command-line strip, the message toast and the completion popup: `cmdline_columns`, the `toast_*` rules with `carried_scroll` and `prompt_answered`, the `pum_*` rules, and the cell rules `text_cells`, `cell_clusters` and `clip_to_cells`.

**Decisions**

- Widths are grid cells by GLib's Unicode tables, a wide character two and a zero-width one none; [`render.rs`](#renderrs)'s `draw_cells` draws on those cells, so a box sized here holds its text.

#### `command_action.rs`

Purpose: the pure mapping from a drained `wolf_core::Command` to the `UiAction` this front-end performs, with no catch-all arm. The mapping is `ui_action_for`. `ui_action_for_front_end_action` covers the `actions::ACTION_TABLE` rows carrying no command, and is the only other way an action is installed.

#### `config_seam.rs`

Purpose: the pure configuration seam, every rule about consuming a `wolf_core::WolfConfig`; [`app.rs`](#apprs) reads the environment and the file. `resolve_font_pt` takes the first of the override and the configured size that is finite and positive, else `FALLBACK_FONT_PT`, because any other value reaching Pango draws nothing, with no error.

**Decisions**

- `appearance` resolves in [`appearance.rs`](#appearancers), whose answer names an `adw::ColorScheme` and would break this module's GTK-freedom.

#### `coordinator.rs`

Purpose: the process-scoped layer above the windows, and their ordered registry. [Its note](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md) owns what lives there, why, and the state protocol. Its pure seam is `next_free_id`.

**Decisions**

- A window that has lost its application or coordinator opens nothing, and quits by closing itself.
- `install_app_actions` runs before the first window, so the menu and chord controller resolve `app.quit` from the first frame; each window's [`shell.rs`](#shellrs) registers their accelerators, so the menu draws them.
- `quit` asks every tab first ([why](../../../notes/wolf-gnome-decisions.md#closing-a-window-and-quitting-ask-every-tab)), and captures every window's panes before any Neovim exits. A close captures only on `KeepSubtree` ([why](../../../notes/wolf-gnome/application-state.md#capture-moments)).
- The readiness probe waits for `restore_session` to open every launch window, so no window is judged alone. `restore_session` maps the windows backmost first ([why](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md)), and `listen` binds before it ([why](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md#opening-from-outside-wolf)).

#### `crash_overlay.rs`

Purpose: the crash and stall widgets: the opaque overlay with the restart button, and the banner confirming a restart, one per window showing the selected tab's liveness. [`liveness_ui.rs`](#liveness_uirs) makes every decision. `set_mode` answers whether the mode changed, so the caller's side effects fire on the transition only.

**Decisions**

- The banner is independent of liveness, because a successful restart clears the overlay at once and would leave no confirmation. Its hide is generation-tokened, so rapid restarts coalesce into one banner.

#### `cursor.rs`

Purpose: pure cursor geometry and colour: `cursor_rect`, whose `cell_percentage` is a percent in 1–100 that `Block` ignores, and `cursor_colors`, fill first. A mode's highlight is honoured only when it yields a visible cursor, and inverting the cell is the fallback.

**Gotchas**

- `wolf-ffi` carries a port of `cursor_colors`; change both together. [`terminal/canvas.rs`](terminal/README.md#canvasrs) passes `Some(0)` to `cursor_rect` for a bar exactly `MIN_BAR_PX` thick.

#### `drawer.rs`

Purpose: the terminal drawer's pure rulebook: its share of the pane area (`DrawerShare`, one per tab, which a drawer's close or a restart keeps), and which pane `<C-w>h/j/k/l` focuses once a drawer exists (`focus_target`). Its other pure seams are `bottom_most_leaf`, `drawer_up_target` and `DrawerState`. Why the drawer sits outside the pane tree is in [Window structure](../../../notes/wolf-gnome/window-structure.md).

**Decisions**

- `focus_target` is the whole editor-to-drawer bridge: Neovim emits a focus move as an intent and declines to act on it, so without an answer here a user cannot leave the drawer.
- `focus_target` takes the raw pane-area size, not the editor height the [macOS front-end passes](../../wolf-macos/Sources/WolfMacOS/README.md#panecontainerviewswift), since `PaneTree::pane_rects` feeds only ordering and both consumers are scale-invariant.

#### `editor_inset.rs`

Purpose: a pane's chrome as arithmetic, which every pixel-to-grid conversion routes through: the rectangle its grid paints into (`grid_rect`), the content-width cap and its centring (`ColumnCap`, and `column_cap`, where `b:wolf_full_width` becomes geometry), the grid size left (`grid_size_for_pane`), and a pane's content floor (`min_pane_size`). Its other pure seam is `grid_local`.

**Decisions**

- Only `w` and `h` clamp at zero; a non-finite input collapses the whole rectangle, since a NaN origin would poison every pointer event.
- The centring offset floors to whole device pixels, unlike the [macOS offset](../../wolf-macos/Sources/WolfKeyMap/README.md#panegeometryswift): a half-pixel `cr.translate` resamples every glyph, and this crate has no HiDPI story.

#### `frame_metrics.rs`

Purpose: the process-global glue behind [`frame_stats.rs`](#frame_statsrs): the once-only environment read, the clock, the `thread_local!` accumulator and the writes. `install()` is the first statement of [`app.rs`](#apprs)'s `run`, and `maybe_emit()` ends [`window.rs`](#windowrs)'s drain-loop body. [Frame-time instrumentation](../../../notes/wolf-gnome/frame-time-instrumentation.md) owns the cadence and the process-global carve-out.

#### `frame_stats.rs`

Purpose: the pure frame-time seam: the `Bucket` set, accumulation, percentiles, report formatting, the `WOLF_FRAME_STATS*` parsers and the interval decision. Its pure seams are `FrameStats`, `Report`, `BucketSummary`, `percentile`, `format_report`, `is_enabled`, `report_interval`, `report_file_path` and `interval_elapsed`. It reads no clock or environment and holds nothing process-global, so it runs in a parallel `cargo test`; [`frame_metrics.rs`](#frame_metricsrs) owns all three.

#### `geometry.rs`

Purpose: the pure pixel conversions behind every resize, pointer event and divider position. `cell_at` takes grid-local coordinates and answers `(row, col)`, `wolf_core::MouseEvent`'s order; `grid_size_for` never answers below one cell. Its other pure seams are `divider_position` and `divider_span_fraction`.

**Decisions**

- A degenerate input collapses the whole result, not only the bad axis, which would send Neovim a nonsense size for the good one. A point outside the grid clamps to the nearest edge cell, which makes drag-past-the-edge selection work, and an infinity clamps to the far edge.

#### `glyph_style.rs`

Purpose: the pure rulebook for underline, undercurl and strikethrough, and the colour of the first two: `glyph_decoration` and `pango_channels`. [`render.rs`](#renderrs) applies them as Pango attributes.

**Decisions**

- Pango carries one underline per run, so with both bits set undercurl wins; [`paint_golden.rs`](../tests/README.md#paint_goldenrs) pins the wave.

#### `im_ui.rs`

Purpose: the pure rulebook for composed and input-method text: where a window-level key event goes (`key_route`), and what a `commit` string becomes (`commit_keys`, `commit_decision`). [`window.rs`](#windowrs) owns the context, and routes a terminal pane's `Send` through [`commit_route`](terminal/README.md#inputrs).

#### `input.rs`

Purpose: a GDK keyval and modifier state as a `wolf_core::KeyEvent` (`gdk_key`), a free function so it runs with no display. It answers `None` for bare modifiers and locks, and for a keyval whose unicode is absent or a control character.

**Gotchas**

- `SUPER_MASK` maps to `meta`, so an unbound Super chord is forwarded for Neovim to mangle ([The accelerator namespace](../../../notes/wolf-gnome/the-accelerator-namespace.md)).
- A plain printable reaches Neovim through the input method's `commit`, since the context claims one with no sequence pending; `gdk_key` carries the named keys, the chords and what the context declines ([`im_ui.rs`](#im_uirs)).

#### `init_lua.rs`

Purpose: where the captive `runtime/nvim/init.lua` lives, as a function of injected values (`resolve_init_lua`): the `INIT_LUA_ENV` override, unchecked, then `INSTALLED_INIT_LUA_RELATIVE` beside the executable, which [`packaged_layout.rs`](../tests/README.md#packaged_layoutrs) joins on, then the caller's fallback.

**Gotchas**

- Under `wrapGAppsHook4` the executable execs a wrapped sibling and `current_exe()` reads `/proc/self/exe`, so the parent is still `bin/`.

#### `layout.rs`

Purpose: the pure n-ary pane-layout tree behind split panes, mirroring Neovim's frame model. The pure seams are `PaneTree` and `Orientation`. `split` is a no-op on a non-leaf, and `remove` on the last pane, so the tree is never empty; `split_root_left` is [§5.2](../../../specs/cli-and-file-open.md#52-choosing-the-pane)'s whole-tab split. A divider's fraction is its child's share of the remainder ([Pane weights and dividers](../../../notes/wolf-gnome/pane-weights-and-dividers.md)).

**Decisions**

- A split matching its frame's orientation inserts a flat sibling and re-equalizes; only a perpendicular one nests, so `split_root_left` gives a `Vertical` root a flat first child. Always nesting turns a third split into 25/25/50, starving the older pane.

#### `lib.rs`

Purpose: the crate root: the modules, and re-exports of the display-free seams.

**Gotchas**

- `cell_at` is re-exported with `grid_size_for`, its pair. `restart_button_rect` is re-exported so the crash-overlay smoke derives its click from the function the widget realizes.

#### `liveness_ui.rs`

Purpose: the pure rulebook for crash and stall recovery: `liveness_ui`, `overlay_message`, `restart_outcome`, `reopen_order`, `restart_button_rect`, and `terminal_restore`, a restart's [steps](../../../specs/terminal-panes.md#when-neovim-dies) as a `TerminalRestore`. `restart_button_rect` collapses on a too-small allocation, so a caller deriving a click must check the size first.

#### `main.rs`

Purpose: the binary entry point, forwarding `wolf_gnome::app::run`'s exit code; the wiring starts in [`app.rs`](#apprs).

#### `mouse.rs`

Purpose: the GDK pointer rulebook, [`input.rs`](#inputrs)'s analogue: `mouse_input`, `scroll_threshold`, `ScrollAccumulator`, `PaneScroll` and `scroll_steps`, with no widget. Only the primary button is forwarded; terminal panes map theirs through [`mouse_button`](terminal/README.md#inputrs).

**Decisions**

- A rejected `consume` leaves the carry untouched, as if never called, since a poisoned remainder would kill scrolling for the session. `PaneScroll` drops the carry when the target grid changes.
- **A positive step count is `Down`, deliberately.** A GDK `dy > 0` is `GDK_SCROLL_DOWN`, toward the buffer's end, and [`mouse_scroll.rs`](../../wolf-ffi/tests/README.md#mouse_scrollrs) pins `Down` as raising `topline`, so nobody "corrects" it.

#### `nav_ui.rs`

Purpose: the pure rulebook for per-pane Back and Forward: which window a press acts on (`nav_query_win`, which refuses `0` per [`buffer_picker.rs`](#buffer_pickerrs)'s rule), whether it may be issued (`nav_press`), and each button's sensitivity (`nav_buttons`). A sensitive button implies an accepted press, but not the converse.

#### `open_folder.rs`

Purpose: the pure seam of Open Folder ([§6](../../../specs/application-state.md#6-opening-a-folder)): the step a directory takes (`step`), the plan of the tab it fills or opens (`plan`), and the size a fill applies (`fill_size`, only for its window's only tab), over `LiveWindow` readings. [`coordinator.rs`](#coordinatorrs) gathers the windows and acts ([why there](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md#opening-a-folder)).

**Decisions**

- `plan` shares `state_seam`'s per-tab helpers with the launch restore, so the two cannot drift.

#### `open_request.rs`

Purpose: the pure routing of a `wolf` request ([§3–§5](../../../specs/cli-and-file-open.md#3-tab-ids)) to Activate, Open Folder, the tab its `WOLF_TAB_ID` names, or its file's project: `route`, `current_tab`, `placement` and `may_place`. [`coordinator.rs`](#coordinatorrs) acts on it. A port of [`FileOpenSeam.swift`](../../wolf-macos/Sources/WolfKeyMap/README.md#fileopenseamswift), keyed by `TabRef`, the `(WindowId, TabId)` pair.

#### `other_pane_seam.rs`

Purpose: the pure rule for the pane Preview or Edit in Another Pane binds into (`target`, over `Side` and `Target`); [`pane_view.rs`](#pane_viewrs) resolves it to a Neovim window. An `active` that is no leaf, such as the drawer, yields `None`, a no-op rather than a stray split.

**Decisions**

- A port of [`OtherPaneSeam.swift`](../../wolf-macos/Sources/WolfKeyMap/README.md#otherpaneseamswift) with no divergence; [`tests/other_pane_seam.rs`](../tests/README.md#other_pane_seamrs) mirrors its tests.
- The caller passes the raw pane-area size, as for [`drawer.rs`](#drawerrs)'s `focus_target`, so the preferred neighbour is the pane `<C-w>h/l` reaches.

#### `overlay_style.rs`

Purpose: the shared look of the transient overlays ([`picker.rs`](#pickerrs), [`palette.rs`](#paletters), [`buffer_picker.rs`](#buffer_pickerrs)), a GNOME-menu surface as one CSS class in libadwaita's named colours. Call `apply` instead of `add_css_class("background")`, whose window colours it replaces.

**Decisions**

- The scroller, its viewport and the list are forced transparent, since GTK gives a bare `list` the view background. The viewport paints nothing under Adwaita dark, captures byte-identical, and is named for the light path and future themes.
- A thread-local flag installs the provider once per process ([compared](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md)). `mod overlay_style;` is private in [`lib.rs`](#librs), since nothing outside the crate consumes it.

**Gotchas**

- The `entry`/`label` bottom margin is scoped to direct children: a new direct child of the box gains it, and row labels deliberately do not.

#### `palette.rs`

Purpose: the command-palette overlay: registry commands, an Open Recent row per usable recent directory, and a Switch to Window row per other window, whose `PaletteAction::SwitchWindow` carries the coordinator's window id. The pure seams are `open_recent_candidates`, `palette_rows` and `palette_action_for`.

**Decisions**

- One directory can yield both a Switch to Window row, raising that window as it stands, and an Open Recent row, Open Folder, which can focus a background tab.
- Activation records, closes, then dispatches: Close Window may tear the window down synchronously.

#### `pane_layout.rs`

Purpose: the pure codec between the live pane tree and the saved `wolf_core::app_state::Node`: `resolve_leaves`, all-or-nothing, `encode_with_views`, `saved_views`, `pane_ref`, `leaf_index`, `decode`, and the `MAX_RESTORE_LEAVES` and `MAX_RESTORE_DEPTH` caps, which are restore policy rather than a tree limit. Its other pure seams are `encode`, `is_empty_leaf`, `leaf_count`, `leaf_paths_dfs`, `WinContent`, `PaneView` and `LayoutGather`.

#### `pane_view.rs`

Purpose: one tab's GTK glue: the tree as nested `gtk::Paned`, a `gtk::Overlay` per window, and the drawer. Panes appear with the first grid. `other_pane_target` answers `Some(0)` to split and `None` for a no-op, and `toggle_zoom` and `toggle_terminal` carry out [`pane_zoom.rs`](#pane_zoomrs)'s rules.

**Decisions**

- A dragged divider writes the tree's weights, since rebuilds destroy paneds, re-applying per `max-position` notify; a resize's clamp is refused ([why](../../../notes/wolf-gnome/pane-weights-and-dividers.md)).
- `request_layout_rebuild` applies the [extras](../../../notes/wolf-gnome/application-state.md#restoring-the-extras) once a wake brings a grid; views are captured only at [§4's moments](../../../notes/wolf-gnome/application-state.md#capture-moments).
- An area's pointer input is one all-button `gtk::GestureDrag` ([why](../../../notes/crates/wolf-gnome/src/pane_view.rs.md#one-drag-gesture)), pinning target and button at press; editors take only the primary.
- Each pane's status and `b:wolf_full_width` cache per wake: a use-time read re-enters `draw_pane`'s guard, and paint, pointer and resize must agree.
- One funnel sends each wake's grid resizes against the last requested size, not the lagging snapshot, so a lifted cap re-divides a still allocation.
- The drawer's paned exists only while bound, re-taking its share each wake; a zoom shows its host alone ([why](../../../notes/wolf-gnome/window-structure.md)). `reset_after_restart` clears the drawer `detect_closes` misses.

#### `pane_zoom.rs`

Purpose: the pure Zoom Pane rules: which pane each event leaves zoomed (`zoomed_after`), and where focus (`focus_origin`), a restore (`restore_targets`) and the other-pane commands (`other_pane_target`) go while zoomed. `zoomed_after`'s `panes` must be the tree's leaves plus the open drawer, or the drawer cannot zoom. Its other pure seam is `ZoomEvent`.

**Decisions**

- Ported from [`PaneZoom.swift`](../../wolf-macos/Sources/WolfKeyMap/README.md#panezoomswift). Nothing freezes a divider, since none exists while zoomed, and `shown_alone` replaces `visibleRects`, as GTK allocates only the host shown.

**Gotchas**

- `pane_view.rs` applies `Split` and `DrawerOpen` after the guards that drop a split of the drawer or a second drawer, so a dropped one keeps the zoom, and `OpenFile` only when `wolf_core::choose_pane` answers `unzoom`. A restart unzooms; a relaunch re-zooms.

#### `persistence.rs`

Purpose: the glue gathering one tab's contribution to the saved state, one instance per tab, each from its own Neovim. Every decision is [`state_seam.rs`](#state_seamrs)'s or [`pane_layout.rs`](#pane_layoutrs)'s; it holds no store and submits nothing, since [`window.rs`](#windowrs) assembles a window's tabs and [`coordinator.rs`](#coordinatorrs) commits.

**Gotchas**

- `refresh_tab` runs inside the close handler, which then takes every core out, so each core borrow is taken and dropped in its own statement.

#### `picker.rs`

Purpose: the file-picker overlay, a search entry over ranked results, opened from `win.go-to-file` ([`actions.rs`](#actionsrs)). Activation tries `opener`, then sends `InputEvent::Open`, which records the file in the MRU. Its `pub(crate)` `next_selection`, `restored_index` and `activation_index` are every overlay's, and `next_selection` also [`search_panel.rs`](#search_panelrs)'s.

**Gotchas**

- `FocusTarget` is `pub(crate)`, shared by the overlays, the tree and the search panel; [Window structure](../../../notes/wolf-gnome/window-structure.md) says why it is a closure.

#### `preview_coalescer.rs`

Purpose: the pure state machine behind the preview's coalesced render; every entry point takes `now`. [wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md#the-preview-renders-budget) owns the budget, whose three numbers live here alone, and [`pane_view.rs`](#pane_viewrs) the glue. Call `request` only for a due pane, or a timer stays armed for nothing.

**Decisions**

- The leading edge needs both conjuncts, nothing pending and the last render at least `PREVIEW_LEADING_IDLE` old; dropping either renders once per keystroke under steady typing.
- No `cancel`, because the fire re-derives the pane set, the liveness and the document, so there is no wrong content to withdraw.

#### `preview_link.rs`

Purpose: the pure URI rulebook for a link clicked in the preview: what it means (`classify`, over the URI WebKit already resolved against the base), the base URI a document loads against (`file_uri_for`, whose exact form [Following a link in the preview](../../../notes/wolf-gnome/following-a-link-in-the-preview.md) states), and the fragment-scroll script (`scroll_into_view_js`). [`preview_ui.rs`](#preview_uirs) owns the gates and load strategy.

**Decisions**

- Four shapes are refused where the [macOS seam](../../wolf-macos/Sources/WolfKeyMap/README.md#previewlinkseamswift) launches or reads through: a non-anchor `about:` URI, a `file:` URI with an empty path, a target with neither scheme nor path, and a `file:` authority other than empty or `localhost` ([why](../../../notes/wolf-gnome/following-a-link-in-the-preview.md)).
- A linked file's Markdown-ness needs the filesystem, so this module always answers `PreviewFile` and core's gate declines.

#### `preview_ui.rs`

Purpose: the pure rulebook for the preview pane: its gates (`preview_press`, `edit_press`, `may_render`, `preview_file_press`, `browser_press`), how a document reaches the view (`document_update`, `needs_reload`), the pane's status (`preview_status`), and the page's scroll report ([why](../../../notes/wolf-gnome/application-state.md#capture-moments)). A search result's repoint is [`search_panel.rs`](#search_panelrs)'s gate.

Its other pure seams are `reported_scroll`, `scroll_to_fraction_js`, `browser_url` and `PreviewLoad`.

#### `preview_width.rs`

Purpose: the pure root style that caps and centres a preview page (`root_style`, with a `max-width` only where [`editor_inset.rs`](#editor_insetrs)'s `grid_rect` would cap), the document carrying it on `<html>` (`styled_document`), and the script setting it live (`apply_root_style_js`). [The preview's content width](../../../notes/wolf-gnome-decisions.md#the-previews-content-width) says why the page caps; [`pane_view.rs`](#pane_viewrs) is the glue.

#### `ready_seam.rs`

Purpose: the pure predicate behind the test-only readiness file (`launch_readiness`), over per-window readings from each selected tab. Its other pure seams are `PaneReadiness`, `WindowReadiness`, `Blocker`, `grid_settled`, `ready_file_path`, `ready_file_contents`, `next_step` and `ReadyStep`. [wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md#the-readiness-file) says why each condition is there; `READY_FILE_ENV` and `LINE_PREFIX` are the contract with the harness's `wait_ready`.

#### `refresh_coalescer.rs`

Purpose: the pure state machine (`RefreshCoalescer`) behind the sidebar's trailing refresh: a burst of traversal batches collapses into one rebuild per `REFRESH_INTERVAL`. `request` answers `Some(generation)` when the caller must arm a one-shot timeout.

**Decisions**

- `fire` clears the armed flag before answering, so a batch during a refresh arms a fresh fire. That puts a traversal's last batch in some later fire, and is why this is a trailing timer, not a frame budget, which would drop a wake with nothing armed.
- **The picker deliberately does not share it**, unlike the [macOS coalescer](../../wolf-macos/Sources/WolfKeyMap/README.md#pickerrefreshcoalescerswift): widening it would risk the picker smokes for nothing this crate needs.

#### `render.rs`

Purpose: the Cairo/Pango paint seam: one grid and its highlights onto any Cairo context. `paint` takes one grid, not a `Snapshot`, since under `ext_multigrid` each surface paints its own; `draw_cells` draws the toast's and popup's text. `CellMetrics::for_font` measures the cell from `family` alone, and its `font` names `family_list(family)`: the configured family, then `SYMBOLS_FAMILY` unless a member names it.

**Decisions**

- The face order, and why the cell ignores the symbols face: [Typography](../../../notes/wolf-gnome-decisions.md#typography).
- `paint` and `draw_cells` draw cell by cell through one `GlyphPainter`, one layout per paint: a Pango run advances by rounded glyph widths, not `metrics.cell_w`, outgrowing its cells, and each `create_layout` costs a `PangoContext` and a font-map lookup.

**Gotchas**

- `GlyphPainter::new(cr)` and `draw(cr, …)` take the context twice, unbound; pass the same one.
- The fallback colours, `GlyphPainter`, `draw_glyph`, `set_source` and `draw_text_line` are `pub(crate)` for [`cursor.rs`](#cursorrs), [`pane_view.rs`](#pane_viewrs) and [`terminal/canvas.rs`](terminal/README.md#canvasrs), so every text takes the one Pango path; a copy drifts from the grid paint.

#### `save_ui.rs`

Purpose: the pure rulebook for Save and Save As: whether a press writes or opens a dialog (`save_press`, `save_as_press`), what the dialog proposes (`save_dialog_proposal`), and what a chosen path becomes (`save_as_target`). `SaveTarget`'s fields must come from one snapshot pass, and [`shell.rs`](#shellrs) carries the answers out.

#### `scrollbar.rs`

Purpose: a Neovim window's `win_viewport` projected onto a pane-local thumb (`scrollbar_thumb`) and its colour (`scrollbar_thumb_color`, by [`statusbar_layout.rs`](#statusbar_layoutrs)'s `blend`). `terminal_viewport` gives a terminal pane's thumb the same path, and [`pane_view.rs`](#pane_viewrs) only fills the rectangle, drawing no track.

**Decisions**

- `x` is measured from the pane's right edge, so the thumb lies in the right padding gutter and covers no glyph. It never enters `grid_size_for_pane`, where it would drop a column whenever a buffer overflowed, reflowing text on the scroll path.

#### `search_panel.rs`

Purpose: [`sidebar.rs`](#sidebarrs)'s Search page, a field and three toggles over streaming ripgrep matches grouped by file. Its pure seams, among them `SearchToggles`, `search_options_for`, `enter_action`, `activation_route` and `search_status`, are in the module doc.

**Decisions**

- Regex on and `All` off (Markdown-only) match the counterpart; case starts off, since `ignorecase()` blocks.
- The list's `navigation-sidebar` class keeps the selection tint neutral, for OCR of the preselected row.
- Escape, focus, the case default and the status line diverge from the [macOS panel](../../wolf-macos/Sources/WolfMacOS/README.md#searchviewswift); the module doc says why.

**Gotchas**

- Only a cross-file jump records history: `NavTarget::Edit` carries no line number.

#### `shell.rs`

Purpose: the glue for the action namespace: `gio` actions, the primary menu, the chord controller, the empty state, the command drain, and per-tab crash recovery. `install_actions` installs the `win.` rows before `present()`, so the menu resolves them.

**Decisions**

- The chord controller sits on the window ahead of any overlay's, with no open-overlay guard, since no overlay key is a bound chord. Accelerators are registered only for the menu's labels, `app.` rows included ([`coordinator.rs`](#coordinatorrs)).
- One settle point, over [`workspace.rs`](#workspacers)'s `reroot_decision`, moves the sidebar's root for a typed `:cd`, a switch and a filled tab alike.
- The search chord and a [toggle click](#sidebar_togglesrs) reveal the sidebar through [`state_seam.rs`](#state_seamrs)'s `sidebar_is_shown`, and New Tab captures what it shows ([why](../../../notes/wolf-gnome/application-state.md#new-tab)). A switch first records the sidebar into, and captures, the tab it leaves ([why](../../../notes/wolf-gnome/application-state.md#the-extras)).
- Actions resolve the selected tab's core per use, and commands drain from it alone ([why](../../../notes/wolf-gnome/window-structure.md)).
- The empty state's `background` class makes it opaque and colours it until [`EmptyStateStyle`](#titlebar_backgroundrs) applies `Normal`.

#### `sidebar.rs`

Purpose: the window's sidebar, a `gtk::Stack` of Files ([`tree_view.rs`](#tree_viewrs)) and Search ([`search_panel.rs`](#search_panelrs)) with no switcher, both built for the window's life; it owns the header bar's [toggle group](#sidebar_togglesrs). `panel_visibility` is the pure seam: a page is live only while the sidebar is shown and the page current.

**Decisions**

- One relay carries every switch, show and hide into both pages' refresh gates, and is the only site setting the toggle group, from `active_toggle`, so the chords, the palette, the empty state and launch need no sync of their own.

**Gotchas**

- It opens on Files with no toggle active until the shell relays the sidebar shown; each tab records its page ([how](../../../notes/wolf-gnome/application-state.md#the-extras)).

#### `sidebar_toggles.rs`

Purpose: the header bar's sidebar control, an `adw::ToggleGroup` of Files and Search after Back/Forward, which shows the sidebar's page, switches it and hides the sidebar. The counterpart of [`SidebarToolbarSeam.swift`](../../wolf-macos/Sources/WolfKeyMap/Sidebar/README.md#sidebartoolbarseamswift); the pure seams are `active_toggle`, `toggle_click` (the active toggle hides, another shows its page) and `toggle_at`.

**Decisions**

- `adw::ToggleGroup` emits nothing for a click on its active toggle, which must hide the sidebar. So a capture-phase `gtk::GestureClick` claims every primary press before the toggles see it, and maps the release to a half with `toggle_at`; no click reaches the group's own activation.
- The group never takes focus, so Space and Enter stay with Neovim and keyboard activation cannot run the group's own logic around the rule.

**Gotchas**

- The Files tooltip names no chord: Ctrl+Shift+B toggles whichever page is current.

#### `stacking.rs`

Purpose: the pure window stacking order and per-window tab recency (`TabRecency`) of [application-state.md §3](../../../specs/application-state.md#3-stacking-order-and-tab-recency), kept from events because Wayland lets a client query neither.

**Decisions**

- It diverges from [`StackingOrder.swift`](../../wolf-macos/Sources/WolfKeyMap/Persistence/README.md#stackingorderswift) on purpose. Windows rank, not tabs, because a GNOME window's tabs never move to another window. Only Wolf's own Minimize marks a window, because Wayland never tells a client it is minimized, and any focus unmarks it ([why](../../../notes/wolf-gnome/application-state.md)).

#### `state_seam.rs`

Purpose: the pure application-state seam: what to restore at launch (`restore_plans`, never an empty vec), and the write side, what each tab and window contributes to the saved state and whether a commit or a close writes anything (`tab_entry`, `seeded_tab`, `sidebar_entry`, `ordered_tab_states`, `window_entry`, `window_state`, `close_commit`). [Application state](../../../notes/wolf-gnome/application-state.md) owns the rules.

Its other pure seams are `TabPlan`, `present_after`, `new_window_size`, `active_size`, `restored_window_size`, `sidebar_is_shown`, `snapshot_cwd`, `may_snapshot`, `restored_sidebar`, `restore_runs_search` and `may_capture_cursor`.

**Decisions**

- `ordered_tab_states` stably sorts tabs into page order, since the user can rearrange the bar.
- The empty state always persists an empty directory, or every launcher start becomes a project at home.

#### `statusbar_layout.rs`

Purpose: the per-pane statusbar's rulebook: `statusbar_layout`, `secondary_font_size`, and `statusbar_text` over the editor's own default colours. It is unit-agnostic; [`pane_view.rs`](#pane_viewrs) calls it in device pixels.

**Decisions**

- The band is the editor's default background with no label-tier fallback, by the user's decision: this front-end takes no colour from the GTK theme ([Window structure](../../../notes/wolf-gnome/window-structure.md)). The macOS [`StatusbarText.swift`](../../wolf-macos/Sources/WolfKeyMap/README.md#statusbartextswift) applies the same constants, blend and half-away-from-zero rounding and repeats `statusbar_text_table`, so change both together.
- A degenerate input collapses the whole layout to zeros, since an infinite strip leaves an editor one cell; a zero-height strip leaves a usable pane.

**Gotchas**

- `blend` is `pub(crate)` for [`scrollbar.rs`](#scrollbarrs)'s thumb colour; a second copy would round one scale two ways.

#### `tab_bar_slot.rs`

Purpose: where the tab bar sits, in a row below the header bar or inside it as its title widget, by `gtk-titlebar-style` and the page count: `tab_bar_placement`, the seam, answers `InHeader` under `Tabs` from two pages. [Window structure](../../../notes/wolf-gnome/window-structure.md) owns each placement's geometry, and `IN_HEADER_CSS` keeps the in-header bar at a tab's height. `TabBarSlot::install` needs `header` already a child of `content`.

**Decisions**

- The move runs from an idle, and only when the answer changed: `notify::n-pages` fires inside libadwaita's close and drop paths, where a reparent would unroot a tab mid-gesture.
- The input is the view's `n_pages`, not `len`: a tab dragged to or from another window changes the view alone.

#### `tab_view.rs`

Purpose: the GTK glue for native tabs, each owning its `WolfCore`, wake drain, restart state, [`pane_view.rs`](#pane_viewrs) and [`persistence.rs`](#persistencers) handle. Hold no core borrow across `replace` or `close`. `sync_ui_focus` is the `:checktime` trigger, and `sync_terminal_focus` a terminal's ([how](../../../notes/wolf-gnome/terminal-panes.md)).

**Decisions**

- A tab's wake channel exists before its core starts, so the startup flush's wake is queued. The core's [tab ID](../../../specs/cli-and-file-open.md#3-tab-ids) is set first, so no terminal session lacks it.
- `close-page` is a veto that asks first ([why](../../../notes/wolf-gnome-decisions.md#closing-a-tab-asks-its-neovim)). Closing the last page closes the window, so `replace` opens first and confirms the tab ([why](../../../notes/wolf-gnome-decisions.md#closing-a-window-and-quitting-ask-every-tab)).
- A tab is titled from its own Neovim's `live_cwd`, never the shell's read ([why](../../../notes/wolf-gnome-decisions.md#threading-and-the-wake)).

#### `tabs_ui.rs`

Purpose: the pure rulebook for a window's native tabs: what a close does (`close_outcome`, `close_page_now`), a tab's title and tooltip (`tab_label`), Close Window's and Quit's decisions (`tab_unsaved`, `window_close`, `quit`), and which tab's Neovim holds the UI focus (`focused_tab`, `focus_edges`), over `TabId` and `FocusEdge`. [`tab_view.rs`](#tab_viewrs) carries them out.

**Public API**

- `close_page_now`'s `None` liveness is no core ([rule](../../../notes/wolf-gnome-decisions.md#closing-a-tab-asks-its-neovim)). `tab_unsaved`, `window_close` and `quit` pull nothing past the first `Unsaved` ([rule](../../../notes/wolf-gnome-decisions.md#closing-a-window-and-quitting-ask-every-tab)).
- `tab_label`'s title is `title::window_title` of the cwd, so the selected tab's equals its window's, and its tooltip the cwd in full. An empty tab, or one with no cwd yet, is `title::WINDOW_TITLE_FALLBACK` with no tooltip.

#### `terminal.rs`

Purpose: the root of the terminal-pane modules. A pane whose window shows a `wolf-core` terminal session paints from the session's frame, not the placeholder's grid; the seams and painter are in [`terminal/`](terminal/README.md), and [Terminal panes](../../../specs/terminal-panes.md) owns the contract.

**Decisions**

- `smoke_terminal_command` is the test-only `SMOKE_TERMINAL_ENV` hook: the trimmed value is the program, with no arguments, and a blank one opens nothing. It opens the session before the app reports ready, so a smoke's first typed key reaches its program; [`app.rs`](#apprs) reads the variable and [`pane_view.rs`](#pane_viewrs) opens the session in the first wake with a pane ([why there](../../../notes/wolf-gnome/terminal-panes.md)).

#### `title.rs`

Purpose: the pure title seam of the window and each tab: the working directory with home abbreviated on a whole path segment (`display_title`, and `window_title`, which substitutes `WINDOW_TITLE_FALLBACK` for an empty result). `home` is injected; the crate's one environment read is [`workspace.rs`](#workspacers)'s.

**Decisions**

- The window builder seeds `WINDOW_TITLE_FALLBACK` as its initial title, so the title never flickers between two spellings before the first directory arrives.

#### `titlebar_background.rs`

Purpose: the background of the header bar, the tab bar and an empty tab's view, by [Theming](../../../specs/architecture.md#theming)'s rule: the pure decisions (`titlebar_background`, `empty_state_background`, `stylesheet_update`), their stylesheets (`titlebar_css`, `empty_state_css`), and the providers applying them. The counterpart of [`TitlebarBackground.swift`](../../wolf-macos/Sources/WolfKeyMap/README.md#titlebarbackgroundswift).

**Decisions**

- One provider per window for the bars, following the selected tab: [`shell.rs`](#shellrs) applies the tab's colour, `stylesheet_update` replaces the stylesheet only when it changed, and [`window.rs`](#windowrs)'s close handler uninstalls it, since a closed window's id is minted again. [`tab_view.rs`](#tab_viewrs) installs one per tab for the view.
- Both rules declare `background-color` alone, at application priority with no state selector ([why](../../../notes/wolf-gnome/window-structure.md)); the view's button and text keep libadwaita's foreground and geometry.

#### `tree.rs`

Purpose: a re-export shim over `wolf_core::tree`, which projects the flat file list for every front-end; it keeps the `crate::tree::…` paths resolving.

#### `tree_gate.rs`

Purpose: the pure gate (`TreeRefreshGate`) every refresh of the sidebar's two pages passes, visibility first and version second. A refresh requested while hidden is refused and owed, and `set_visible` answers `true` only for the catch-up that owes.

**Decisions**

- The search panel resets the version tracking on every run, since each `SearchSession` numbers its versions from 1, as a re-root's `FileSearch` does.

**Gotchas**

- Hiding destroys no store, so a catch-up at an unchanged version rightly rebuilds nothing.

#### `tree_view.rs`

Purpose: the file-tree sidebar, a `gtk::ListView` over a `gtk::TreeListModel` and [`sidebar.rs`](#sidebarrs)'s Files page. A file click tries `opener` first, and `start` is false in the empty state, where nothing is walked. It reads no cwd: [`shell.rs`](#shellrs) calls `reroot`, and `release_core` runs at a tab's close.

**Decisions**

- A reveal expands the current file's ancestors and selects its row at once, retrying on every refresh, and a rebuild re-reveals it. The scroll runs later, from a frame-clock tick while mapped and after layout, since an earlier `scroll_to` draws rows blank ([The sidebar reveal](../../../notes/wolf-gnome/the-sidebar-reveal.md)).
- The current-file bold moves through a registry of bound labels, which is what the [version gate](#tree_gaters) costs: a suppressed rebuild re-binds no row.

**Gotchas**

- Manual expansion below the root does not survive a rebuild ([Present limitations](../../../notes/wolf-gnome/present-limitations.md)).

#### `window.rs`

Purpose: one window and what is scoped to it, an `adw::ApplicationWindow` over its tabs ([`tab_view.rs`](#tab_viewrs)) and its overlays. [Window structure](../../../notes/wolf-gnome/window-structure.md) owns its widgets and stacking. `build` answers `None` when no planned tab's core starts, and a launch whose only window fails exits.

**Decisions**

- The close handler's order is load-bearing: its unsaved check comes first ([why](../../../notes/wolf-gnome-decisions.md#closing-a-window-and-quitting-ask-every-tab)), and releasing the input method comes last.
- Keys, commits and focus reach the selected tab ([terminal panes](../../../notes/wolf-gnome/terminal-panes.md)).

#### `workspace.rs`

Purpose: the pure workspace decisions: is a project open, and where does Neovim start (`launch_decision`), and where is the sidebar rooted (`reroot_decision`, `root_label`). `home_dir()` is its one environment read, which the shell captures once. Callers canonicalize both sides of every comparison.
