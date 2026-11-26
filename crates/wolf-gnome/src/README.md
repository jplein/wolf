# wolf-gnome/src

The GTK front-end's modules, one entry each. The pure seams here are the Linux counterparts of the ones in [`wolf-macos/Sources/WolfKeyMap/`](../../wolf-macos/Sources/WolfKeyMap/README.md).

**Read [wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md) first**, with the topic files it links: they own every constraint that spans more than one file here, and most of this directory's rules exist because of one.

## Files

#### `about.rs`

Purpose: the About dialog — a pure text seam over the bundled fonts' and the linked libraries' licence notices, plus the transient window that shows it. This is the one surface on this front-end that discharges their attribution obligations, so its content has legal weight rather than cosmetic weight.

**Public API**

- `about_text() -> String` — the whole dialog body. It takes no arguments and reads `wolf_core::FONT_LICENSE_NOTICES` and then `wolf_core::LIBRARY_LICENSE_NOTICES` itself, deliberately: a version taking the notices as a parameter would let its test pass them in, which proves nothing about what ships.
- `present(parent)` — a modal `gtk::Window` transient for the main window, titled `ABOUT_TITLE`.

**Decisions**

- Plain GTK over `adw::AboutDialog` or `adw::AboutWindow`. `adw::AboutDialog` is an `adw::Dialog`, drawn inside the main window rather than as a toplevel of its own, which the next bullet rules out. `adw::AboutWindow` is deprecated since libadwaita 1.6, which the crate's `v1_7` feature ([gtk-rs versions](../../../notes/wolf-gnome-decisions.md#gtk-rs-versions)) turns into a warning that `-D warnings` refuses.
- A separate toplevel rather than a fifth in-window overlay, so the main window's focus widget is untouched and the focus regime does not apply to it at all.

#### `actions.rs`

Purpose: the action namespace as data — one row per `gio` action — plus the chord matcher and the invariant checker.

**Public API**

- `ACTION_TABLE` and `MENU_SECTIONS` — the one table the menu, the accelerators and the command dispatch all read; see [The accelerator namespace](../../../notes/wolf-gnome/the-accelerator-namespace.md).
- `action_for_key(keyval, state)` — the matcher the chord controller calls. It scans `Bound` rows only: matching a reserved row would swallow the key and do nothing.
- `parse_accel`, `ctrl_fold` and `forbidden_reason(&ParsedAccel, declared)` — the invariant checker, run over every row by this module's tests. It takes the declaration as a parameter so no caller can run half of it.

**Decisions**

- `parse_accel` re-implements the accelerator grammar this crate allows rather than calling `gtk::accelerator_parse`, which asserts an initialized main thread and cannot run headlessly.
- `action_for_key` compares its three modifiers exactly and ignores every other bit: intercepting a Super-contaminated chord beats forwarding one Neovim mangles. Letters compare case-insensitively, since a shifted letter's keyval is keymap-dependent.

**Gotchas**

- Whether an action is installed follows from its command's classification, not from `status`: a command-less row installs through `command_action::ui_action_for_front_end_action` instead. Promoting a reserved row means flipping `status` too, and declaring the byte a `Ctrl+<symbol>` chord folds onto, or the chord stays inert or the checker refuses the row.
- A row's `win.` or `app.` prefix decides who installs it: [`coordinator.rs`](#coordinatorrs) the `app.` rows once per process, each window's [`shell.rs`](#shellrs) the `win.` ones ([The coordinator and its windows](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md)).

#### `app.rs`

Purpose: the entry point; the first activate builds the coordinator, installs its `app.` actions and restores the saved session, and any later one opens a fresh window.

**Public API**

- `run() -> glib::ExitCode` — hands GTK an empty argv; a path arrives over the open channel ([§2](../../../specs/cli-and-file-open.md#2-the-channel)).
- `XFT_DPI_PINNED_PREFIX` — begins the stderr line the dpi pin prints only when it fires; [`smoke_headless/preview.rs`](../tests/README.md#smoke_headlesspreviewrs) matches its own copy, so retype a rename there.
- Six `pub(crate)` helpers: [`coordinator.rs`](#coordinatorrs) calls `load_config`, `measure_metrics`, `open_state_store` and `ready_file_path` once each, [`tab_view.rs`](#tab_viewrs) calls `init_lua_path` per tab, and [`window.rs`](#windowrs) calls `smoke_terminal_command` per window. [`init_lua.rs`](#init_luars), [`ready_seam.rs`](#ready_seamrs) and [`terminal.rs`](#terminalrs) own the rules behind `init_lua_path`, `ready_file_path` and `smoke_terminal_command`.

**Decisions**

- The coordinator lives in an `Rc<RefCell<Option<_>>>` captured by the activate closure, not a `static`: no process-global _application_ state is what keeps two cores in one process independent, [`frame_metrics.rs`](#frame_metricsrs)'s counters aside.
- Nothing is built before the first activate: the config read and cell measurement need a running GTK, and a remote instance must not open the state store.
- Every activate first sets an unset `gtk-xft-dpi` to `FALLBACK_XFT_DPI`, leaving a session's value alone: without a desktop portal it is unset, and WebKitGTK draws a preview created on a not-yet-allocated host, as a split creates, blank for good ([measured](../../../notes/crates/wolf-gnome/src/app.rs.md#pinning-gtk-xft-dpi)).

**Gotchas**

- Once fired, the pin outranks the session for the process's life, as any app-set `GtkSettings` value does: a dpi a late-starting portal or `xsettingsd` supplies is ignored.

#### `appearance.rs`

Purpose: the OS dark/light rulebook — what a configured `appearance` resolves to against the desktop's preference — plus the glue that applies it at launch.

**Public API**

- `appearance_decision(configured, os_dark) -> ResolvedAppearance` — total over `wolf_core::Appearance` with no `_` arm, so a fourth variant is a compile error here.
- `apply_at_launch(config, core)` — called once per tab, as soon as its core exists; each call makes one `StyleManager::is_dark()` read and carries the decision out for that tab's Neovim. Its step order is load-bearing.

**Decisions**

- `os_dark` is a parameter and is never read here: `adw::StyleManager::default()` asserts an initialized main thread, so a seam reaching for it could not run headlessly.
- A forced appearance takes `ForceDark`/`ForceLight`, never `PreferDark`/`PreferLight`, which yield to the desktop preference the user wrote the config line to override.
- The notify handler pushes `background` and never re-sets the scheme, and is armed after the launch-time set: re-applying a property from inside its own notify handler is the shape that loops.

**Gotchas**

- What `set_color_scheme` reaches beyond the chrome, and the GTK version that ends it: [The preview's light and dark](../../../notes/wolf-gnome-decisions.md#the-previews-light-and-dark).

#### `buffer_picker.rs`

Purpose: the buffer-picker overlay — a search entry over the selected tab's Neovim's listed buffers, ranked by `wolf_core::rank_buffers`. Activating a row switches the active window to that buffer.

**Public API**

- `build(overlay, area, core) -> BufferPickerHandle` — starts hidden; `area` is a [`picker.rs`](#pickerrs) `FocusTarget`.
- `BufferPickerHandle::open`, `close` (both idempotent), and `is_open`.
- `open_decision`, `initial_selection`, `switch_target` and `row_label` — the pure seams.

**Decisions**

- Candidates are snapshotted once at open and re-ranked per keystroke. The snapshot is a blocking RPC on the GTK main thread, so [`picker.rs`](#pickerrs)'s wake bridge must not be added for symmetry: re-taking it puts a whole timeout budget of frozen main loop on the typing path.
- `open_decision` refuses to open, and so refuses the RPC, for every liveness reading but `Alive`. It cannot prevent the first stall, and must not be strengthened into a guard that can never raise one.
- A refusal returns before any widget is touched, and nothing is shown until the snapshot returns.
- The first refresh preselects row 1, because row 0 is the buffer the user is already in and a bare Enter must switch. No other overlay shares that rule.
- `switch_target` refuses an active window of `0` as firmly as none at all, per `wolf-ffi`'s rule: `0` binds the switch into whichever window Neovim considers current.

**Gotchas**

- Closing does not clear the snapshot; the next open overwrites it. A tab switch closes the picker, so its snapshot is always the selected tab's.

#### `cell_metrics_math.rs`

Purpose: the pure cell-metrics arithmetic the configured `line-height` drives. No GTK, Pango or Cairo; every input an `f64`.

**Public API**

- `cell_height(font_size, line_height_percent, ascent, descent, leading)` — a positive, finite percentage gives `round(font_size * percent / 100)`, driven by the font size alone and independent of the raw metrics; anything else gives `ceil(ascent + descent + leading)`.
- `baseline(cell_height, ascent, descent)` — the baseline offset from the cell's top, splitting the leftover space evenly above and below the glyph box.
- `MIN_CELL_PX` — the floor a degenerate result collapses to.

**Decisions**

- The rule lives here and not in [`render.rs`](#renderrs) because `for_font` needs a live Cairo context, so a rule left inside it could only be executed by the golden-render target.
- The function is unit-agnostic and rounds in whatever unit its caller uses; this crate calls it in device pixels, since device pixels are what Cairo paints in.
- Two guards, confined to degenerate inputs: the percentage must be finite (a bare `> 0` accepts `+inf`), and a non-finite or non-positive result is floored (unfloored, it can be `0` or `NaN`).

#### `clipboard_ui.rs`

Purpose: the pure rulebook for Copy and Paste through the system clipboard. GTK-free; [`shell.rs`](#shellrs) owns the `gdk::Clipboard` handle and the asynchronous read.

**Public API**

- `copy_decision(liveness, terminal) -> CopyPress` and `paste_decision(liveness) -> PastePress` — both total with no `_` arm. `terminal` is the active pane's session: with one, Copy answers `Terminal(id)` whenever a core exists; otherwise only `Some(Alive)` acts.
- `clipboard_write(selection)` and `paste_payload(read)` — `None` for an absent or empty string.

**Decisions**

- `clipboard_write`'s `None` means write nothing, never clear. Clearing would destroy whatever the user copied from another application, in answer to a chord that yanked nothing.
- Copy is a bounded blocking RPC and is gated for that reason ([Crash and stall recovery](../../../notes/wolf-gnome/crash-and-stall-recovery.md)). Nothing on the paste path blocks: its refusals are about a paste queued behind an opaque overlay reaching a restarted Neovim minutes later, and the two reasons must not be collapsed.
- A terminal pane's Copy is ungated on liveness, and must stay so: `terminal_selection_text` reads the session and issues no RPC, so a wedged or dead Neovim cannot cost it a timeout. Paste keeps its gate in a terminal pane, since an opaque overlay hides the terminal too, and [`shell.rs`](#shellrs) captures the target at the press, so a focus change during the read cannot redirect the text.

#### `cmdline_ui.rs`

Purpose: the pure rulebook behind the command-line strip, the message toast and the completion popup.

**Public API**

- `cmdline_columns(prefix, text, byte)` — `prefix` is the dialog `prompt` plus `firstc`.
- The toast rules: `toast_budget_rows`, `toast_view`, `toast_scroll_next`, `toast_scroll_clamp`, `toast_width_cols`, `carried_scroll`, `toast_is_shown`, `prompt_answered`.
- The popup rules: `pum_selection`, `pum_view`, `pum_item_label`, `pum_width_cols`.
- The cell rule: `text_cells`, `cell_clusters`, `clip_to_cells`.

**Decisions**

- `cmdline_columns` is one function because the caret and the popup's cmdline anchor both need it: a second copy puts the popup a column off its caret.
- A zero offset pins the toast to its newest line — `toast_scroll_next` inverts the grid's sign for that reason — and `carried_scroll` drops the offset when the line count changes, since that is a different message.
- `pum_selection` refuses an out-of-range index rather than clamping: a selection the item list cannot explain is a fold error, and highlighting the last row would hide it.
- Both width seams measure the visible rows only: the whole list would make the popup jump wider as a long item scrolled into range, and size a scrolled toast for a line it is not drawing.
- Widths are grid cells by GLib's Unicode tables: a wide character is two cells, and a zero-width one none, joining the cluster before it. [`render.rs`](#renderrs)'s `draw_cells` draws the toast and popup on those cells, so a box sized here holds its text.

**Gotchas**

- The offset is gesture-driven because key-driven paging is impossible; [wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md) owns that rule.

#### `command_action.rs`

Purpose: the pure mapping from a drained `wolf_core::Command` to the UI action this front-end performs for it. GTK-free, so the whole classification is tested headlessly.

**Public API**

- `ui_action_for(command) -> UiAction` — total.
- `ui_action_for_front_end_action(action) -> Option<UiAction>` — what an `actions::ACTION_TABLE` row carrying no command performs: the two navigation rows, About, and Save and Save As. `None` for anything else, and it is the only other way an action is installed.
- `UiAction` — the dispatched variants, plus `Deferred` and `NoGnomeEquivalent`, each carrying the concrete blocker as a string.

**Decisions**

- The match carries no catch-all arm, so appending a variant to `wolf_core::Command` fails to compile here until the new command is classified. A silently dropped command is indistinguishable from a broken one.
- The front-end-only classification lives here rather than in [`actions.rs`](#actionsrs) because this module owns `UiAction` and `actions.rs` stays free of it. Nothing outside the tests here imports `actions`, so the two do not cycle.
- A second check walks the command registry rather than the match, so the guard survives someone adding a catch-all arm.
- Every unimplemented reason is distinct and non-empty, enforced by a test, so no two rows collapse into one indistinguishable diagnostic and the printed message always names the real blocker.

**Gotchas**

- A `Deferred` or `NoGnomeEquivalent` command reaching the drain is printed with its reason, never dropped quietly.

#### `config_seam.rs`

Purpose: the pure configuration seam — every rule about consuming a `wolf_core::WolfConfig`. GTK-free and environment-free: [`app.rs`](#apprs) does the environment read and the file load, this module decides what the values mean. The Linux counterpart of [`state_seam.rs`](#state_seamrs).

**Public API**

- `resolve_font_pt(env_override, config_font_size) -> f64` — the first of the two that is finite and strictly positive, else `FALLBACK_FONT_PT`. Both inputs come from a human, so `0`, a negative and a non-finite value are all reachable, and any of them reaching Pango draws nothing with no error anywhere.

**Decisions**

- The override is injected rather than read here, which is what makes the precedence rule testable without touching the process environment — something a parallel test suite cannot safely do.
- The configured `appearance` is not resolved here: it belongs to [`appearance.rs`](#appearancers), whose answer names an `adw::ColorScheme` and would falsify this module's GTK-free property.

#### `coordinator.rs`

Purpose: the process-scoped layer above the windows, and their ordered registry. [Its note](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md) owns what lives there, why, and the state protocol.

**Public API**

- `build() -> Coordinator`, then `install_app_actions(app)`, `listen(app)` and `restore_session(app)`.
- `open_new_window(app)`, `open_folder(app, current, directory) -> Option<TabRef>`, `commit(waking)`, `on_window_closing(id)`, `quit(app)`, `raise(id)`, `palette_windows(exclude)`, `register(handle)`, `window_count()`, `on_window_focused(id)`, `on_window_minimized(id)`.
- `next_free_id(next, live) -> WindowId`, and `upgrade(&weak)` for window-side closures.
- `ready_pending()`, `next_paint_ordinal()`, `check_ready()` — the readiness probe.

**Decisions**

- The coordinator holds strong `WindowHandle`s and every window-side closure a `Weak` it upgrades per use: the other direction is a cycle, and a dropped coordinator performs nothing.
- `install_app_actions` runs before the first window exists, so the menu and the chord controller resolve `app.quit` from the first frame. Each window's [`shell.rs`](#shellrs) registers their accelerators, idempotently, so the menu draws the chord.
- A window that has lost its application or its coordinator opens nothing, and quits by closing itself.
- `quit` captures every window's panes before any Neovim exits, and closes every window the _application_ holds: a transient toplevel such as About would keep the process alive. A close captures only on `KeepSubtree` ([why](../../../notes/wolf-gnome/application-state.md#capture-moments)).
- `next_free_id` is free because the registry cannot reach the wrap it guards; a reused id targets the wrong window.
- The readiness probe judges nothing until `restore_session` has opened every launch window, so no window is judged alone.
- `restore_session` maps the windows backmost first, each after the one behind it ([why](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md)).
- `listen` binds before `restore_session`, whose windows its requests still follow ([why](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md#opening-from-outside-wolf)).

#### `crash_overlay.rs`

Purpose: the widgets for crash and stall recovery — the opaque overlay carrying the restart button, and the transient banner that confirms a restart. Every decision it acts on is made in [`liveness_ui.rs`](#liveness_uirs); this file builds widgets and toggles visibility. One per window, it shows the selected tab's liveness.

**Public API**

- `build() -> CrashOverlay`, with `widget()` and `banner_widget()` for the two overlay children.
- `set_mode(mode) -> bool` — returns whether the mode changed. The caller's side effects must fire on the transition only.
- `connect_restart`, `flash_banner`, `focus_is_inside`.

**Decisions**

- The overlay's layout is deliberately derivable: a zero-spacing box with two equal expanding spacers around a fixed message-plus-button block, so `restart_button_rect` describes the button's real position. Do not substitute an `adw::StatusPage` and do not give the box a non-zero spacing — a status page's padding is theme-owned, and box spacing applies between every pair of children, so either change silently invalidates the pixel the smoke clicks.
- It never grabs focus on the button. A focusable widget inside a container that is later hidden is exactly the wedge the focus regime exists to prevent.
- It does not disable pointer targeting: swallowing clicks is half its job, and a visible overlay child takes the pointer instead of the panes beneath.
- The banner is independent of liveness, because a successful restart clears the overlay immediately and a user would otherwise see no confirmation. Its hide is generation-tokened, so rapid restarts coalesce into one continuous banner.

#### `cursor.rs`

Purpose: pure, GTK-free cursor geometry and color logic — the pixel rectangle to fill for a shape, and the `(fill, glyph)` color pair.

**Public API**

- `cursor_rect(shape, cell_x, cell_y, cell_w, cell_h, cell_percentage)` — `cell_percentage` is a percent in 1–100, not a fraction; `Block` ignores it entirely.
- `cursor_colors(&Snapshot) -> (Rgb, Rgb)` — returns fill first, glyph second.

**Decisions**

- A mode's cursor highlight is honored only when it demonstrably yields a visible cursor: both channels resolve, the fill differs from the underlying cell background, and the fill differs from the glyph. A naive "both channels set" test lets an invisible cursor through, because a background-only Cursor highlight has its foreground filled in from the editor defaults and then paints dark on dark.
- The `WolfCursor` theme group is honored first and unconditionally, because it is an explicit app-level opt-in; a background-only group keeps the reverse-video glyph.
- Inverting the underlying cell is the fallback because it is visible by construction.
- Bar thickness floors at `MIN_BAR_PX` so a tiny percentage still renders something wider than a hairline.

**Gotchas**

- `wolf-ffi` carries a port of `cursor_colors`; the two are kept in lockstep, so change both together.
- [`terminal/canvas.rs`](terminal/README.md#canvasrs) is a second consumer of `cursor_rect`, passing `Some(0)` so a terminal bar or underline is exactly `MIN_BAR_PX` thick; the floor is load-bearing there too.
- An out-of-range `mode_idx`, empty `mode_infos`, and an out-of-bounds cursor cell all fall through to inversion rather than panicking.

#### `drawer.rs`

Purpose: the terminal drawer's pure rulebook — the drawer's share of the pane area, and which pane a `<C-w>h/j/k/l` intent focuses once a drawer exists. GTK-free, so it runs in the fast tier. Why the drawer sits outside the pane tree at all is in [Window structure](../../../notes/wolf-gnome/window-structure.md).

**Public API**

- `DRAWER_WEIGHT`, every new tab's share, and `DrawerShare` — `new()`, `restored(share)`, `get()`, `position(min, max, span)`, `drag_to(position, span)` — the drawer's fraction of its divider's paned span, whose position is the editor tree's height.
- `focus_target(tree, origin, dir, state, width, height)` — the whole editor-to-drawer bridge as one value. Neovim emits a focus move as a pure intent and declines to act on it, so with no answer here a user who opens the drawer cannot leave it. `bottom_most_leaf` and `drawer_up_target` are its `<C-w>k` half.
- `DrawerState` — `Copy`, so [`pane_view.rs`](#pane_viewrs) holds it in a `Cell`; `clear()` empties both the bound drawer and the remembered editor pane, which is what a restart needs.

**Decisions**

- `DrawerShare` is a value apart from `DrawerState`, so `clear()` — a drawer closing, a restart — keeps the size the user dragged to. There is one per tab, held by its pane view, and `restored` refuses a saved share outside `(0, 1)` ([why no fraction floor](../../../notes/wolf-gnome/pane-weights-and-dividers.md)).
- `focus_target` takes the raw pane-area size, not the editor height the [macOS front-end passes](../../wolf-macos/Sources/WolfMacOS/README.md#panecontainerviewswift), since `PaneTree::pane_rects` feeds only ordering and both its consumers are scale-invariant. A table pins it rather than only stating it.

#### `editor_inset.rs`

Purpose: a pane's chrome as arithmetic — the rectangle its grid paints into, the content-width cap with its centring, and the grid size left over.

**Public API**

- `EDITOR_PADDING_PX`, the four-side inset, and `MAX_GRID_COLS`, the cap in columns.
- `ColumnCap { max_cols: Option<u16>, cell_w }`, `ColumnCap::uncapped(cell_w)`, `apply(cols) -> u16`.
- `column_cap(full_width, cell_w) -> ColumnCap` — where a window's `b:wolf_full_width` becomes geometry.
- `grid_rect(pane_w, pane_h, statusbar_h, padding, cap) -> GridRect`, `grid_local(x, y, rect) -> (f64, f64)`.
- `grid_size_for_pane(pane_w, pane_h, statusbar_h, padding, cell_h, cap) -> (u16, u16)`.
- `min_pane_size(cell_w, cell_h, statusbar_h, padding) -> (f64, f64)`, a pane's content floor.

**Decisions**

- The division stays in [`geometry.rs`](#geometryrs), un-inset for `resize_global`'s grid 1.
- Only `w` and `h` clamp at zero; a non-finite input collapses the whole rectangle, since a NaN origin would poison every pointer event.
- The cap is a `min` on the width here and on the column count in `grid_size_for_pane`, which divides the uncapped rectangle: flooring a capped width to whole pixels costs the last column at a non-integral cell width.
- The centring offset is floored to whole device pixels, unlike the [macOS offset](../../wolf-macos/Sources/WolfKeyMap/README.md#panegeometryswift): a half-pixel `cr.translate` resamples every glyph and this crate has no HiDPI story.
- There is no `text_left_inset`: `grid_rect(..).x` carries the offset and every consumer reads that one field, so the strip and the grid share an origin by construction.

#### `frame_metrics.rs`

Purpose: the process-global glue behind [`frame_stats.rs`](#frame_statsrs) — the once-only environment read, the monotonic clock, the accumulator and the writes. [Frame-time instrumentation](../../../notes/wolf-gnome/frame-time-instrumentation.md) owns the cadence and the process-global carve-out.

**Public API**

- `install()` — arms the instrument, and is the first statement of [`app.rs`](#apprs)'s `run` because every other function here branches on `enabled()`. A `WOLF_FRAME_STATS_FILE` that cannot be opened degrades to stderr-only rather than refusing to run.
- `enabled()`, `begin()`, `end(bucket, start)`, `scope(bucket) -> Scope`, `note_keystroke()`, `note_wake()`.
- `maybe_emit()` — the last statement of [`window.rs`](#windowrs)'s drain-loop body, outside every measured region — and `flush_final()`, after `Application::run_with_args` returns.

**Decisions**

- Bind every `Scope` to a named guard. `let _ = scope(..)` drops it at once and measures a region of zero length.
- Never `eprintln!` a report: it panics when the stderr write fails. Every write goes through `write_all` with the `Result` discarded — a diagnostic that can take the application down is worse than a lost one.
- The accumulator is a `thread_local!`, so nothing needs a lock or `Send` — and a record from another thread lands in a slot nothing reports. The tripwire is a `debug_assert!` against the thread `install()` stamped.

**Gotchas**

- Which values name a report file, and what path they name, are [`frame_stats.rs`](#frame_statsrs)'s `report_file_path`; `install()` opens whatever it answers and decides nothing itself.

#### `frame_stats.rs`

Purpose: the pure frame-time seam — bucket identity, accumulation, percentiles, report formatting, the `WOLF_FRAME_STATS*` parsers and the interval-elapsed decision. It reads no clock, touches no environment and holds nothing process-global, which is what keeps it testable in a parallel `cargo test` process; [`frame_metrics.rs`](#frame_metricsrs) owns all three.

**Public API**

- `Bucket`, with `ALL`, `index()` and `label()`. `index()` is the storage index into `FrameStats`'s accumulator array and the only numbering there is.
- `FrameStats::{record, record_keystroke, record_wake, reset, is_empty, retained_sample_count, summarize}`, and the `Report` / `BucketSummary` values `summarize` answers.
- `percentile`, `format_report`, `is_enabled`, `report_interval`, `report_file_path`, `interval_elapsed`.

**Decisions**

- Every parser takes the raw `Option<&str>` rather than an environment map, per [`config_seam.rs`](#config_seamrs): a parallel test suite must not mutate the process environment.
- `report_interval` guards before `Duration::from_secs_f64`, which panics on the negative, non-finite and overflowing values an environment can supply.
- `report_file_path` trims the value it returns and not only the one it tests, so the path opened is the path approved.
- `percentile` is nearest-rank, never interpolating: the p95 column must name a frame time some frame actually took.
- `summarize` omits every zero-count bucket, so an absent row means "never entered" rather than "entered and cost nothing".

**Gotchas**

- A child bucket's time is already inside its parent's, so a child reads as a share of its parent and never as an addend beside it. The module docs carry the nesting, the disjointness contract and the count identities.

#### `geometry.rs`

Purpose: the pure pixel conversions behind every resize, pointer event, and divider position, kept GTK- and pango-free so they are unit-tested headlessly.

**Public API**

- `grid_size_for(width_px, height_px, cell_w, cell_h) -> (u16, u16)` — floor division per axis, never below one cell.
- `cell_at(x, y, cell_w, cell_h, cols, rows) -> (u32, u32)` — returns `(row, col)`, matching `wolf_core::MouseEvent`'s field order.
- `divider_position(min, max, span, fraction)` and `divider_span_fraction(position, span)` — the `gtk::Paned` divider conversion.

**Decisions**

- Any degenerate input collapses the whole result rather than only the offending axis, because a partial result would send Neovim a nonsense size for the good axis.
- A point outside the grid clamps to the nearest edge cell rather than being rejected, which makes drag-past-the-edge selection work.
- Non-finite coordinates are two rules: a NaN axis reads as zero, an infinity clamps to the far edge. Collapsing both to zero would invert an overshooting drag.
- `divider_position` and `divider_span_fraction` must share one denominator, and which one it is decides whether a fresh split is even; see [Pane weights and dividers](../../../notes/wolf-gnome/pane-weights-and-dividers.md).

**Gotchas**

- `cell_at` takes grid-local coordinates, and the cap's centring offset is already in [`editor_inset.rs`](#editor_insetrs)'s `grid_rect(..).x`, which `grid_local` subtracts. So this function takes no origin; do not add one.

#### `glyph_style.rs`

Purpose: the pure rulebook for the three attribute-borne decorations — underline, undercurl, strikethrough — and the colour the first two take. [`render.rs`](#renderrs) applies the answer as Pango attributes.

**Public API**

- `glyph_decoration(&ResolvedStyle) -> GlyphDecoration` — total; `is_empty` is what lets an undecorated cell build no `AttrList`.
- `pango_channels(Rgb)` — widens by 257, never 256, so `255` reaches the top of Pango's range exactly.

**Decisions**

- The wave is Pango's (`Underline::Error`), never a hand-stroked Cairo path: the underline's placement comes from face metrics Pango reads and this crate does not.
- Pango carries one underline attribute per run, so `underline` and `undercurl` cannot coexist and undercurl wins. The wave is pinned by [`paint_golden.rs`](../tests/README.md#paint_goldenrs).
- No strikethrough colour is ever set, so Pango strikes in the layout foreground, and a decorated cell carries one attribute fewer.

#### `im_ui.rs`

Purpose: the pure, GTK-free rulebook for composed and input-method text entry — where one window-level key event goes, and what one `commit` string becomes. [`window.rs`](#windowrs) owns the context and the `filter_keypress` call.

**Public API**

- `key_route(phase, overlay_open, im_filtered) -> KeyRoute` — `Overlay`, `AskInputMethod`, `Consumed`, `Translate` or `Ignore`, each an obligation on the glue. `im_filtered` is `None` until the context has been asked.
- `commit_keys(text) -> Vec<KeyEvent>` — one modifier-free `Key::Char` per character, in order, control characters dropped.
- `commit_decision(overlay_open, text) -> CommitDecision` — `Send(events)` or `Refuse(reason)`, the reason a mid-sentence clause the caller prints.

**Decisions**

- The overlay early return precedes the offer, because `filter_keypress` is a side effect rather than a query: an accepted key is held in a pending compose state and cannot be un-offered.
- Both matches are `_`-free, so a new phase or route is an `E0004` until classified; `Ignore` is what keeps a release from doubling every key.
- A control character never becomes a `Key::Char`: `key_to_notation` renders one as a raw byte, a second unnamed route to the named keys [`input.rs`](#inputrs) owns.
- An empty commit is a `Send` of nothing, not a refusal: a refusal exists to be printed.

**Gotchas**

- The refusal arm is unreachable from this crate's glue, which asks the context only with no overlay open and gets `commit` synchronously from inside that call. It is there for an out-of-process engine committing on a later main-loop turn.
- In a terminal pane [`window.rs`](#windowrs) routes a `Send` through [`commit_route`](terminal/README.md#inputrs) instead.

#### `input.rs`

Purpose: translates a GDK keyval plus modifier state into a `wolf_core::KeyEvent`. It has the exact shape of a `gtk::EventControllerKey` callback but stays a free function so it is testable without a display server.

**Public API**

- `gdk_key(keyval, state) -> Option<KeyEvent>` — `None` for bare modifier and lock keys, and for any keyval whose unicode value is absent or a control character.

**Decisions**

- Named keys resolve before the printable fallback, because their `to_unicode()` is often a control character (`Return` is `\r`) and would otherwise be swallowed by the control-character rejection.
- For printable characters `shift` is always false: GDK already delivers the shifted glyph in the keyval, and `KeyEvent` ignores `shift` for `Key::Char`. Named keys pass the real shift through, which is what keeps `<S-Tab>`, `<S-Left>`, and `<S-F5>` working.
- `ISO_Left_Tab` is what Shift+Tab actually reports on X11, so it maps to `Key::Tab` with `shift` forced true even when the platform did not also set the shift mask.

**Gotchas**

- `SUPER_MASK` maps to `meta`, so an unmapped Super chord is forwarded for Neovim to mangle ([The accelerator namespace](../../../notes/wolf-gnome/the-accelerator-namespace.md)); the chord controller intercepts only the ones this crate binds.
- A plain printable normally reaches Neovim through the input method's `commit` rather than through this function, since the window's context claims one even with no sequence pending. What `gdk_key` carries is the named keys, the modifier chords and whatever the context declines ([`im_ui.rs`](#im_uirs)).

#### `init_lua.rs`

Purpose: where the captive `runtime/nvim/init.lua` lives, as a function of values. A pure seam in [`config_seam.rs`](#config_seamrs)'s shape: the environment read, the `current_exe()` read and the existence probe are all injected, so every branch is decidable with no process environment and no filesystem.

**Public API**

- `INIT_LUA_ENV` and `INSTALLED_INIT_LUA_RELATIVE` — the override's variable name, and the installed file's path relative to the directory holding the executable.
- `resolve_init_lua(env_override, exe_dir, manifest_fallback, exists) -> PathBuf` — total and never panicking: the override, then the executable-relative candidate, then the caller's fallback.

**Decisions**

- The override is returned unchecked while the executable-relative candidate is probed. An override the user typed is an instruction, so checking it would let a typo fall silently through to a different `init.lua`; the candidate is a guess, and must fall through in a checkout build.
- `INSTALLED_INIT_LUA_RELATIVE` is the contract with the Nix install layout — [`packaged_layout.rs`](../tests/README.md#packaged_layoutrs) joins on it rather than retyping the path.
- The candidate keeps its `bin/..` step: Neovim is handed that string as `-u` and reports it back through `debug.getinfo`, which is how the captive `init.lua` finds its sibling trees.

**Gotchas**

- Under `wrapGAppsHook4` the installed executable execs a wrapped sibling, and `current_exe()` reads `/proc/self/exe`, so the parent directory is still `bin/` and the join still resolves.

#### `layout.rs`

Purpose: the pure, GTK-free n-ary pane-layout tree behind the split-panes UI, mirroring Neovim's frame model.

**Public API**

- `PaneTree::split(target, new_pane, orientation)` is a no-op on a non-leaf target, and `remove(pane)` on the last pane, so the tree is never empty. `split_root_left(new_pane)` is the whole-tab split of [§5.2](../../../specs/cli-and-file-open.md#52-choosing-the-pane).
- `PaneTree::leaves()` and `pane_rects(w, h)` — the same deterministic depth-first order; the rects tile the bounds exactly.
- `PaneTree::layout()` and `from_layout(&PaneLayout)` — a read-only mirror for the widget layer and its inverse.
- `PaneTree::divider_fraction(path, boundary)` and `set_divider_fraction(path, boundary, f)` — one divider, addressed by its split's path and the child it follows; the fraction is that child's share of the whole remainder, per [Pane weights and dividers](../../../notes/wolf-gnome/pane-weights-and-dividers.md).
- `Orientation` — `Vertical` divides width, `Horizontal` height, matching `wolf_core::SplitOrientation`.

**Decisions**

- A split matching its containing frame's orientation inserts a flat sibling and re-equalizes the row; only a perpendicular one nests. So `split_root_left` gives a `Vertical` root a flat first child and wraps any other at halves. Always nesting turns a third same-orientation split into 25/25/50, starving the older pane.
- `remove` normalizes bottom-up — flatten same-orientation children, collapse a one-child split, re-equalize — so the tree stays canonical, matching Neovim's `equalalways`.
- `PaneTree::neighbor`'s cross-axis overlap must be strictly positive within `NEIGHBOR_EPS`, so a shared edge never counts and a 2x2 grid is unambiguous.
- `from_layout` rebuilds verbatim and validates rather than repairs, because the restore path zips Neovim window handles onto its leaves by index; a malformed split reaching the widget builder panics.

#### `lib.rs`

Purpose: crate root. Declares the modules and re-exports the display-free seams a consumer reaches for.

**Gotchas**

- `cell_at` is re-exported because `grid_size_for` is, and the two are a pair whose composition is the point.
- `restart_button_rect` is re-exported for one concrete consumer: the crash-overlay smoke derives the pixel it clicks from the same function the widget builder realizes, so the two cannot drift.

#### `liveness_ui.rs`

Purpose: the pure, GTK-free rulebook for crash and stall recovery — everything about a `wolf_core::Liveness` reading that can be decided without a widget.

**Public API**

- `liveness_ui(liveness, may_auto_restart) -> LivenessUi` — the overlay mode, whether to schedule an automatic restart, and whether painting this wake is worth anything.
- `overlay_message(mode)`, `restart_outcome(respawned, fresh)`, `reopen_order(files, active)`, `restart_button_rect(w, h)`.
- `terminal_restore(splits, drawer_open, live)` — a restart's `TerminalRestore` steps ([order](../../../specs/terminal-panes.md#when-neovim-dies)).
- The overlay and banner strings, `MAX_AUTO_RESTART_ATTEMPTS`, and the four geometry constants.

**Decisions**

- Both matches are total, with no catch-all arm, so a new `Liveness` variant fails to compile here until it is classified.
- `Alive` and `NotResponding` are insensitive to the attempt budget: a spent budget must never stop a healthy editor painting, and a wedged Neovim must never be force-killed automatically.
- `Crashed { retrying }` means an automatic attempt is still permitted, not that one is in flight — the counter counts scheduled attempts.
- A `Refresh` outcome never requests an automatic restart, or a persistently failing respawn would retry from inside the restart path and bypass the cap entirely.
- `reopen_order` appends the active path last, and appends it even when it was absent from the set. Reopening rides one ordered channel, so the last path opened is the one that ends up focused.
- `restart_button_rect` collapses the whole result on a degenerate or too-small allocation rather than returning a partial rect, so a caller deriving a click point must check the size before trusting the origin.

#### `main.rs`

Purpose: the binary entry point. `main` forwards the exit code from `wolf_gnome::app::run`; the wiring starts in [`app.rs`](#apprs).

#### `mouse.rs`

Purpose: the GDK pointer-translation rulebook, the analogue of [`input.rs`](#inputrs). GDK value types in, `wolf_core` input values out, with no widget and no event loop.

**Public API**

- `mouse_input(button, state)` and `scroll_threshold(unit, cell_h)` — the button plus the modifier bools Neovim's mouse grammar carries, and the delta that makes one line step.
- `ScrollAccumulator::consume`, `PaneScroll::steps_for(grid, delta, threshold)`, and `scroll_steps(steps)`.

**Decisions**

- Only the primary button is forwarded to Neovim; every other number yields `None`, since a context menu and middle-click paste are unimplemented features. Terminal panes map theirs through [`mouse_button`](terminal/README.md#inputrs).
- Every modifier bit outside `C-`, `S-`, `A-` is dropped rather than translated, because Neovim's mouse grammar has nowhere to put it. Unlike `gdk_key`, which has a meta notation.
- An unknown scroll unit falls back to the wheel threshold on purpose: guessing wheel for a pixel delta is over-sensitive and recoverable, while guessing surface for a detent delta makes scrolling dead.
- A rejected `consume` leaves the carry untouched, so it is indistinguishable from no call; a poisoned remainder would kill scrolling for the session. `PaneScroll` discards the carry when the target grid differs from the previous call, accepted or not, because a remainder is a fraction of a line of one grid.
- **A positive step count is `Down`, deliberately.** A GDK `dy > 0` is `GDK_SCROLL_DOWN`, which moves toward the end of the buffer, and [`mouse_scroll.rs`](../../wolf-ffi/tests/README.md#mouse_scrollrs) pins `Down` as raising `topline`. A test exists to stop that being "corrected".

#### `nav_ui.rs`

Purpose: the pure, GTK-free rulebook for per-pane navigation — which window a Back or Forward press acts on, whether the press may be issued, and whether each toolbar button is sensitive. The navigation counterpart of [`liveness_ui.rs`](#liveness_uirs).

**Public API**

- `nav_query_win(active_win) -> Option<i64>` — the single place the "which window" question is answered, so the press path and the button refresh cannot disagree. It refuses `0`, per [`buffer_picker.rs`](#buffer_pickerrs)'s rule.
- `nav_press(liveness, active_win) -> NavPress` — `Move(win)` or `Refuse(reason)`, the reason a mid-sentence clause the caller prints.
- `nav_buttons(liveness, active_win, history) -> NavButtons` — the two buttons' sensitivity over core's per-pane availability.

**Decisions**

- Every function receives `active_win` rather than reading it, so the seam carries no single-window assumption and a focus change is only a change of argument.
- `nav_press`'s match over `Option<Liveness>` has no catch-all arm, so a new variant fails to compile until classified. `None` is a classified arm, because the close handler leaves the core absent.
- A sensitive button implies an accepted press; the converse is deliberately false. A live core on a pane with nothing behind it accepts the press and desensitizes both buttons, because core answers `None` and no RPC is issued.

**Gotchas**

- The buttons desensitize while the `gio` actions stay enabled — the opposite of `win.go-to-file`'s empty-state treatment, for the reason in [The accelerator namespace](../../../notes/wolf-gnome/the-accelerator-namespace.md).

#### `open_folder.rs`

Purpose: the pure, GTK-free seam of Open Folder ([§6](../../../specs/application-state.md#6-opening-a-folder)): which step a directory takes, the plan of the tab it fills or opens, and the size a fill applies. [`coordinator.rs`](#coordinatorrs)'s `open_folder` gathers the windows and acts ([why there](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md#opening-a-folder)).

**Public API**

- `step(directory, windows, stacking, current_empty) -> Step` — `Focus { window, tab }` for the tab §4's rule picks, else `FillCurrent` when the invoking tab is empty, else `NewWindow`. `directory` is already canonical; a `LiveWindow`'s tabs are in page order, `""` for an empty one.
- `plan(directory, project, new_tab_sidebar) -> RestorePlan` — one tab on `directory`, from `project`'s saved tab and stored size, its sidebar setting included, or a fresh tab with no size. A fresh tab shows its sidebar by `new_tab_sidebar` when it is `Some`, the visibility New Tab captured for the empty tab being filled, and otherwise shows it.
- `fill_size(plan, window_tabs)` — the plan's size only when the filled tab is its window's only one.

**Decisions**

- `step` puts the windows in stacking order and hands them, directory-only, to `wolf_core::app_state::project_tab`, so the focus is §4's and not list order.
- Its directories are live (`TabHandle::live_directory`), never a tab's cached contribution, which a shut gate can leave naming another directory.
- A planned tab is a folder tab whatever `launch_decision` would say of `directory`: opening a folder is an explicit act, as a typed `:cd /` is.
- `plan` shares `state_seam`'s per-tab plan helpers with the launch restore, so the two cannot drift.

#### `open_request.rs`

Purpose: the pure, GTK-free routing of a request from the `wolf` command ([§3–§5](../../../specs/cli-and-file-open.md#3-tab-ids)): Activate, Open Folder, the tab its `WOLF_TAB_ID` names, or its file's project. [`coordinator.rs`](#coordinatorrs) gathers the windows and acts. A port of [`FileOpenSeam.swift`](../../wolf-macos/Sources/WolfKeyMap/README.md#fileopenseamswift), keyed by the `(WindowId, TabId)` pair a tab is unique by.

**Public API**

- `route(target, tab_id, windows, tab_ids, stacking) -> Route` — `tab_ids` maps each tab's `WOLF_TAB_ID` to its `TabRef`, live only when `windows` holds that tab. `current` is the live named tab, else `current_tab`.
- `current_tab(windows, stacking)` — the frontmost window's selected tab, else its most recent one; `None` with no window.
- `placement(choice, leaves) -> Option<Placement>` — core's `PanePlacement` index resolved to a `PaneTree::leaves()` id; `None` past the last leaf.
- `may_place(liveness)` — `Alive` alone.

**Decisions**

- A live named tab routes `OpenFile` whatever project the file lies in, as §5.1 rule 1 has it; any other file goes through Open Folder on `wolf_core::project_directory` over every live tab's non-empty directory.
- `may_place` holds a file until Neovim is `Alive`, since the whole-tab split is a blocking call ([inventory](../../../notes/wolf-gnome/crash-and-stall-recovery.md)).

#### `other_pane_seam.rs`

Purpose: the pure, GTK-free rule for which pane Preview or Edit in Another Pane binds into: an existing pane of the pane tree, or a split core makes for the bind. [`pane_view.rs`](#pane_viewrs) resolves the answer to a Neovim window, and [`shell.rs`](#shellrs) passes it to core.

**Public API**

- `Side` — the preferred side: Preview prefers `Right`, Edit `Left`.
- `Target` — `Split`, or `Existing(PaneId)`.
- `target(active, prefer, tree, width, height) -> Option<Target>` — rules in order: an `active` that is no leaf of `tree` (the drawer, an unknown pane) yields `None`, so the press is a no-op rather than a stray split; a single leaf yields `Split`; a `PaneTree::neighbor` on the preferred side yields that pane; otherwise the next leaf in `leaves()` order for `Right`, the previous for `Left`, wrapping.

**Decisions**

- A port of [`OtherPaneSeam.swift`](../../wolf-macos/Sources/WolfKeyMap/README.md#otherpaneseamswift) with no divergence; [`tests/other_pane_seam.rs`](../tests/README.md#other_pane_seamrs) mirrors its tests case for case.
- Purely geometric, with no preference for what a pane shows, so a fallback target that is itself a preview is repointed under ordinary bind semantics.
- The caller passes the bounds `drawer::focus_target` gets, the raw pane-area size, for the reason the [`drawer.rs`](#drawerrs) entry gives, so the preferred neighbour is the pane `<C-w>h/l` reaches.

#### `overlay_style.rs`

Purpose: the shared look of the three transient overlays ([`picker.rs`](#pickerrs), [`palette.rs`](#paletters), [`buffer_picker.rs`](#buffer_pickerrs)) — a GNOME-menu surface with menu-sized rows, as one CSS class.

**Public API**

- `apply(overlay_box)` — `pub(crate)`; adds the class and installs the stylesheet on first call. Call it instead of `add_css_class("background")`, whose window colours it replaces.

**Decisions**

- Colours are libadwaita named colours, never literals, so the overlays follow whichever scheme [`appearance.rs`](#appearancers) resolves.
- The scroller, its viewport and the list are forced transparent: GTK gives a bare `list` the view background, which would punch a panel through the popover surface. The viewport paints nothing under Adwaita dark (captures are byte-identical either way) and is named for the light path and future themes.
- A thread-local flag installs the provider once per process rather than once per overlay per window; [The coordinator and its windows](../../../notes/wolf-gnome/the-coordinator-and-its-windows.md) compares that with the crate's other display-global providers.
- `mod overlay_style;` is private, alone in [`lib.rs`](#librs): nothing outside the crate consumes or tests it, so `pub mod` would widen the crate's public surface for no gain.

**Gotchas**

- The `entry`/`label` bottom margin is scoped to direct children: a fifth direct child of the box silently gains it, and row labels deliberately do not match.

#### `palette.rs`

Purpose: the command-palette overlay — registry commands, one Open Recent row per usable recent directory, one Switch to Window row per other window.

**Public API**

- `build(overlay, area, core, windows) -> PaletteHandle` — starts hidden; `area` is a [`picker.rs`](#pickerrs) `FocusTarget`, `windows` a `WindowSource`.
- `PaletteHandle::open`, `close` (both idempotent), `is_open`, and `connect_activate`.
- `open_recent_candidates`, `palette_rows` and `palette_action_for` — the pure seams — and `PaletteAction`, whose `SwitchWindow` payload is the coordinator's own window id.

**Decisions**

- One directory can yield a Switch to Window row and an Open Recent row: one raises that window as it stands, the other is Open Folder, which can focus a background tab.
- `open_recent_candidates` refuses three directories: the selected tab's own, which Open Folder would focus as it is; a relative path, which `AppShell::open_folder` canonicalizes against the process cwd, not Neovim's; and one `is_dir` rejects, where no Neovim starts. What the store keeps is [`WolfCore::recent_dirs`](../../wolf-core/src/README.md#apirs)'s concern; the seam rechecks because it takes any slice.
- The window list is a closure taken fresh at each open over each window's cached entry, so the open touches no other core and adds no RPC.
- The label is `PaletteRow::display` unchanged: [`title.rs`](#titlers)'s home abbreviation would rank one string and show another, and a query matching the visible `~` would return nothing.
- Ranking runs unbounded and `max` applies after the classification filter, since `rank_palette` truncates first and undispatched rows would otherwise eat the budget.
- Activation records, closes, then dispatches: a dispatched Close Window tears the window down synchronously.

#### `pane_layout.rs`

Purpose: the pure codec between the live pane tree, whose leaves are pane ids, and the saved `wolf_core::app_state::Node`, whose leaves are panes with paths. No GTK, no serde, no IO.

**Public API**

- `resolve_leaves(&[(PaneId, WinContent)]) -> Option<HashMap<PaneId, LeafContent>>` — the step ahead of `encode`; `WinContent::Unknown` is a window in neither snapshot map.
- `LayoutGather` — `Ready(LiveLayout)` or `Refuse`.
- `encode_with_views(layout, contents, views) -> Node` — a leaf takes only a `PaneView` of its kind; a leaf id missing from `contents` becomes an empty edit pane. `encode` passes no views.
- `saved_views(node, leaf_ids)` and `pane_ref(layout, drawer, pane)`, which names the drawer `Drawer`.
- `leaf_index(layout, id)` — a leaf's depth-first index; `None` for the drawer.
- `decode(node, first_leaf_id) -> Option<(PaneTree, Vec<PaneId>)>` — leaf `i` is the pane for handle `i`, ids allocated in depth-first order.
- `is_empty_leaf(node)`, `leaf_count(node)`, `leaf_paths_dfs(node)`, `MAX_RESTORE_LEAVES`, `MAX_RESTORE_DEPTH`.

**Decisions**

- `resolve_leaves` is all-or-nothing: an empty path is a resolved answer and an absent window is not, so one `Unknown` refuses the whole tab's gather. Mapping absence onto `""` would write a well-formed tab over the user's files.
- `is_empty_leaf` is the content predicate the restore side skips a rebuild on, and `leaf_count` must never be it ([why](../../../notes/wolf-gnome/application-state.md)).
- `decode` validates before anything is rendered, so Neovim is never asked to realize a malformed tree. The caps are restore policy, not a tree limit.
- The orientation mapping lives here in both directions, so an inversion is one edit and one test rather than two half-inversions that cancel.

#### `pane_view.rs`

Purpose: one tab's GTK glue: `layout`'s tree as nested `gtk::Paned`, a `gtk::Overlay` per window, the drawer.

**Public API**

- `build(core, metrics, drawer_share) -> PaneHandle` — panes appear with the first grid.
- `cmdline_widget()`, the focus-return target; `toast_widget()`, `pum_widget()`.
- `reset_after_restart()`, `drawer_is_open()`, `request_terminal_restore(steps)`, `request_layout_rebuild(restored)`, `layout_snapshot() -> LayoutGather`, `capture_viewports()`, `snapshot_readiness()`, `readiness()`.
- `active_terminal()`, `sync_terminal_focus(focused)`, `request_terminal(command)`, `open_file(path, wait)`, `open_picked(path, at)`, `drawer_is_active()`.
- `other_pane_target(prefer)` — [`other_pane_seam.rs`](#other_pane_seamrs)'s window: `Some(0)` splits, `None` no-ops.
- `toggle_zoom()`, `toggle_terminal()` — [`pane_zoom.rs`](#pane_zoomrs)'s rules.

**Decisions**

- A dragged divider writes the tree's weights, since rebuilds destroy paneds, re-applying per `max-position` notify against nested allocation's intermediate values; a resize's clamp is refused ([why](../../../notes/wolf-gnome/pane-weights-and-dividers.md)).
- `request_layout_rebuild` applies the [extras](../../../notes/wolf-gnome/application-state.md#restoring-the-extras) once a wake brings a grid; views are captured only at [§4's moments](../../../notes/wolf-gnome/application-state.md#capture-moments).
- [`preview_coalescer.rs`](#preview_coalescerrs)'s state is per tab: a rebuild replaces `panes`.
- An area's pointer input is one all-button `gtk::GestureDrag` ([why](../../../notes/crates/wolf-gnome/src/pane_view.rs.md#one-drag-gesture)), pinning target and button at press; editors take only the primary. A preview page spans its pane below the strip ([why](../../../notes/wolf-gnome-decisions.md#the-previews-content-width)); its capture-phase primary click activates the pane, claiming nothing ([why](../../../notes/wolf-gnome/window-structure.md)).
- A terminal pane paints through [`TerminalCanvas`](terminal/README.md#canvasrs) ([why](../../../notes/wolf-gnome/terminal-panes.md)).
- Each pane's status and `b:wolf_full_width` cache per wake: a use-time read re-enters `draw_pane`'s guard, and paint, pointer and resize must agree.
- One funnel sends each wake's grid resizes against the last requested size, not the lagging snapshot: a lifted cap re-divides a still allocation.
- The drawer's paned exists only while bound, re-taking its share each wake; a zoom shows its host alone ([why](../../../notes/wolf-gnome/window-structure.md)). `reset_after_restart` clears the drawer `detect_closes` misses.

#### `pane_zoom.rs`

Purpose: the pure, GTK-free Zoom Pane rules: which pane each event leaves zoomed, and where focus moves and other-pane commands go while zoomed.

**Public API**

- `zoomed_after(event, zoomed, active, panes) -> Option<PaneId>` — the zoomed pane after a `ZoomEvent`; `panes` must be the tree's leaves plus the open drawer, or the drawer cannot be zoomed and a lone leaf beside the drawer reads as nothing to hide.
- `ZoomEvent::FocusMove { target }` unzooms for another pane, `ToggleTerminal { drawer }` unless the drawer is zoomed, and `Restore(id)` zooms a pane in `panes` that is not alone.
- `focus_origin(zoomed, from)` — the zoomed pane, which `drawer::focus_target` resolves from.
- `restore_targets(zoomed, focused, drawer_open, leaf_ids)` — a saved zoom and focus, restored.
- `other_pane_target(zoomed, drawer)` — read only while zoomed, in place of `other_pane_seam::target`.
- `shown_alone(zoomed, bound) -> Option<PaneId>` — the host the pane area shows alone; a zoomed id with no host shows the whole layout, so a missed unzoom never blanks it.

**Decisions**

- Ported from [`PaneZoom.swift`](../../wolf-macos/Sources/WolfKeyMap/README.md#panezoomswift). Nothing freezes a divider, since none exists while zoomed; `shown_alone` replaces `visibleRects`, as GTK allocates only the host shown.
- A zoomed drawer's Toggle Terminal keeps the zoom until the drawer closes, so no hidden pane shows first.

**Gotchas**

- `pane_view.rs` applies `Split` and `DrawerOpen` after the guards that drop a split of the drawer or a second drawer, so a dropped one keeps the zoom. `OpenFile`, which unzooms like them, it applies only when `wolf_core::choose_pane` answers `unzoom`. A restart unzooms; a relaunch re-zooms.

#### `persistence.rs`

Purpose: the glue that gathers one tab's contribution to the saved window state, one per tab. It is thin by construction: every decision lives in [`state_seam.rs`](#state_seamrs) and [`pane_layout.rs`](#pane_layoutrs), so this module gathers live values and caches them.

**Public API**

- `build(parts) -> StateHandle` — a value constructor that installs no closures and gathers nothing of its own; `StateParts::saved` is held for the seed, not for the rebuild.
- `set_empty(empty)`, `set_sidebar_shown(shown)`, `record_sidebar(page, search)`, `is_empty()`, `sidebar_shown()`, `sidebar_page()`, `search_fields()`, `launch_cwd()`, `live_directory()`, `refresh_tab()`, `cached_tab() -> Option<Tab>`.

**Decisions**

- A refused gather leaves the cache untouched, so a tab whose panes have not bound — or whose Neovim has just died — keeps what it last persisted instead of vanishing. Two refusals compose: [`state_seam.rs`](#state_seamrs)'s gate, and a `LayoutGather::Refuse` from the pane view.
- `cached_tab` derives the seed at read time, so a sidebar toggle, page or query reaches a tab whose gate never opened, and the seed stops being consulted once a real gather succeeds.
- The persisted `sidebar.visible` is the tab's preference, tracked through `set_sidebar_shown`, and never the live visibility.
- This module holds no store and submits nothing: [`window.rs`](#windowrs) assembles a window's tabs and [`coordinator.rs`](#coordinatorrs) commits the window state.

**Gotchas**

- `refresh_tab` runs inside the window's close handler, which goes on to take every tab's core out, so every core borrow is taken and dropped in its own statement.
- Each instance gathers from its own tab's Neovim, so a background tab is gathered live, as the selected one is.

#### `picker.rs`

Purpose: the file-picker overlay — a search entry over a ranked results list. Activation sends `InputEvent::Open`, which also records the file in the MRU cache.

**Public API**

- `build(overlay, area, core, opener) -> PickerHandle` — starts hidden. `area` is a `FocusTarget`, not the results host; `opener` takes a chosen file first.
- `PickerHandle::open`, `close` (both idempotent), and `is_open`, read to suppress key forwarding.

**Decisions**

- The picker has no trigger predicate of its own: it opens from `win.go-to-file`; see [`actions.rs`](#actionsrs).
- The key controller runs in the capture phase, so Escape and the arrows are seen before the `SearchEntry` consumes them, and the arrows move the list selection while focus stays in the entry.
- The selection is preserved across a streaming rebuild unless the query changed, so a batch does not yank the user to the top row.
- No shared overlay widget is extracted, by judgement: the three overlays already share the three seams below, and the leftover widget construction is covered only by the cage smokes, so a common type would risk three working overlays for boilerplate.

**Gotchas**

- `next_selection`, `restored_index` and `activation_index` are `pub(crate)` and shared with [`palette.rs`](#paletters) and [`buffer_picker.rs`](#buffer_pickerrs), so all three overlays navigate and activate identically. `next_selection` also moves [`search_panel.rs`](#search_panelrs)'s selection.
- `FocusTarget` is `pub(crate)` and shared by the three overlays, the tree and the search panel; [Window structure](../../../notes/wolf-gnome/window-structure.md) owns why it is a closure rather than a captured widget.
- A tab switch closes it, so one open never spans two tabs.

#### `preview_coalescer.rs`

Purpose: the pure, GTK-free state machine behind the Markdown preview's coalesced render. It reads no clock: every entry point takes `now` from the caller. [wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md#the-preview-renders-budget) owns the budget, [`pane_view.rs`](#pane_viewrs) the glue.

**Public API**

- `request(now) -> RenderDecision` — `Render` inline on this wake, or `Arm { generation, delay }` for a one-shot `glib` timeout; call it only for a due pane, or a timer stays armed for nothing.
- `fire(generation) -> bool`, and `rendered(now)`, which the render pass calls as its last statement.
- `PREVIEW_TRAILING_DELAY`, `PREVIEW_MAX_STALENESS` and `PREVIEW_LEADING_IDLE` — the one site in this crate where the three budget numbers live.

**Decisions**

- The leading edge needs both conjuncts, nothing pending and the last render at least `PREVIEW_LEADING_IDLE` old; dropping either turns steady typing back into one render per keystroke.
- The armed delay is the lesser of `PREVIEW_TRAILING_DELAY` and the cap's remainder; a spent budget answers a zero delay.
- One fresh one-shot per request, each carrying a new generation, so the sources armed earlier in a burst perform nothing.
- Not [`refresh_coalescer.rs`](#refresh_coalescerrs) widened: that seam is clock-free by construction and wants neither a leading edge nor a cap.
- No `cancel`, because the fire re-derives the pane set, the liveness reading and the document, so there is no wrong content to withdraw.

#### `preview_link.rs`

Purpose: the pure, GTK-free URI rulebook for a link clicked inside the Markdown preview — what the click means, what base URI the document loads against, and the JavaScript that scrolls a fragment into view. This module owns URI arithmetic, [`preview_ui.rs`](#preview_uirs) owns liveness gates and load strategy.

**Public API**

- `classify(uri, source_path) -> PreviewLink` — total over the URI WebKit has already resolved against the base, so no relative-to-absolute arithmetic happens here. The variants are `OpenExternally`, `ScrollToFragment`, `PreviewFile` and `Refuse`.
- `file_uri_for(source_path) -> Option<String>` — the base URI, percent-encoded; `None` for an empty or relative path. Its exact form is load-bearing, and [Following a link in the preview](../../../notes/wolf-gnome/following-a-link-in-the-preview.md) states it.
- `scroll_into_view_js(fragment) -> Result<String, serde_json::Error>`.

**Decisions**

- Four shapes are refused where the [macOS seam](../../wolf-macos/Sources/WolfKeyMap/README.md#previewlinkseamswift) launches or reads through: a non-anchor `about:` URI, a `file:` URI with an empty path, a target with neither scheme nor path, and a `file:` authority other than empty or `localhost`. [Following a link in the preview](../../../notes/wolf-gnome/following-a-link-in-the-preview.md) gives the reason.
- Whether a linked file is Markdown needs the filesystem, so this module never decides it: it always answers `PreviewFile` and core's own gate declines.

#### `preview_ui.rs`

Purpose: the pure, GTK-free rulebook for the Markdown preview pane — its gates, how a rendered document reaches the view, and what the pane's statusbar reads. [`preview_link.rs`](#preview_linkrs) owns a clicked link's URI arithmetic.

**Public API**

- `preview_press`, `edit_press`, `may_render`, `preview_file_press` and `browser_press` — the gate on every blocking preview RPC but a search result's repoint, which [`search_panel.rs`](#search_panelrs) gates, total over `Option<Liveness>` with no `_` arm.
- `browser_url(raw)` — `None` for an empty or blank address; the loopback-prefix check is core's, not duplicated.
- `document_update(current, loaded, fragment_pending)` — swap in place, or full load.
- `PreviewLoad`, `needs_reload(current, loaded)` and `preview_status(source_path, display_path)`.
- `SCROLL_REPORT_JS`, `SCROLL_REPORT_HANDLER`, `reported_scroll(value)` and `scroll_to_fraction_js(fraction)` — the page pushes its scroll ([why](../../../notes/wolf-gnome/application-state.md#capture-moments)).

**Decisions**

- Placement is not decided here. Both presses act in place on the active pane, the other-pane commands bind where [`other_pane_seam.rs`](#other_pane_seamrs) says, and a pane's mode is derived from `Snapshot::previews` rather than stored, so no mode argument is threaded.
- Liveness is read before the buffer-kind gate, so a wedged Neovim reads as wedged, not as a non-Markdown buffer. `preview_file_press` has no buffer-kind gate at all: a linked file's Markdown-ness is core's, unanswerable here without the filesystem.
- Both `PreviewLoad` comparisons key on the source bufnr, never the `changedtick`: two freshly opened files share a tick, so a tick-keyed source switch would swap a linked file's body under the previous file's base URI.
- `preview_status`'s `abs_path` is the source path verbatim: [`pane_view.rs`](#pane_viewrs) persists it as a preview pane's `path`, which [§2](../../../specs/application-state.md#pane) requires to be absolute.

#### `preview_width.rs`

Purpose: the pure, GTK-free root style that caps and centres a preview page's content, the document that carries it in its `<html>` tag, and the script that sets it live. [The preview's content width](../../../notes/wolf-gnome-decisions.md#the-previews-content-width) owns why the page caps rather than the view; [`pane_view.rs`](#pane_viewrs) is the glue.

**Public API**

- `root_style(cap, padding) -> String` — `name:value` declarations joined by `;`, padding on every side, and a `max-width` only where [`editor_inset.rs`](#editor_insetrs)'s `grid_rect` would cap.
- `styled_document(html, root_style) -> String` — the style inserted after the first `<html` ahead of `<head>`, with `&` and `"` escaped; `html` unchanged for an empty style or no such tag.
- `apply_root_style_js(root_style) -> String` — sets the root's `style` attribute to exactly the value, carried as a JSON literal.

**Decisions**

- Nothing from `<head>` on is touched, so a caller reads the head offset and the body from the raw document and compares raw to raw.

**Gotchas**

- `grid_rect` floors its centring offset while the page centres at the exact one, so capped content can start up to 1 px right of a capped editor's column in the same pane.

#### `ready_seam.rs`

Purpose: the pure predicate behind the test-only readiness file — whether a launch's windows accept keystrokes, as a function of per-window readings, each taken from that window's selected tab. [wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md#the-readiness-file) owns why each condition is there.

**Public API**

- `launch_readiness(&[WindowReadiness]) -> Result<(), Blocker>` — `Ok`, or the first unmet condition in a fixed order: every window's settle conditions in slice order, then the input conditions of the window with the greatest `first_paint`.
- `PaneReadiness`, `WindowReadiness`, and `Blocker` with `ALL` and `label()`, the text of the `wolf-ready: waiting on <label>` stderr line.
- `grid_settled`, `ready_file_path`, `ready_file_contents`, and `next_step` over `ReadyStep`.
- `READY_FILE_ENV` and `LINE_PREFIX`, the contract with the smoke harness's `wait_ready`.

**Decisions**

- `next_step` reports a blocker once per change, because the checks run on every wake and every frame.
- `PaneReadiness::layout_pending` also covers a drawer the restore reopened, until it binds, and a shown pane's restored view, until it is queued, so no typed key lands before either.

#### `refresh_coalescer.rs`

Purpose: the pure, GTK-free state machine behind the sidebar's trailing refresh — a burst of streaming traversal batches collapsed into one rebuild per `REFRESH_INTERVAL`.

**Public API**

- `RefreshCoalescer::request() -> Option<u64>` — `Some(generation)` when the caller must arm a one-shot `glib` timeout carrying that generation, `None` when the request coalesced into an armed one.
- `fire(generation) -> bool` — whether that timeout performs its refresh. `cancel()` disarms.
- `REFRESH_INTERVAL` — the one site in this crate where the trailing delay lives.

**Decisions**

- `fire` clears the armed flag before answering, so a batch arriving during a refresh arms a fresh fire rather than coalescing into one that has already performed. That is what puts a traversal's final batch inside some later fire, and why the mechanism is a trailing timer rather than a frame budget, which would drop a wake with nothing armed.
- `cancel` bumps a generation token instead of removing the GLib source, because removing an already-fired source is a GLib warning; the queued closure still runs and performs nothing. The same pattern as [`crash_overlay.rs`](#crash_overlayrs)'s banner.
- **The picker deliberately does not share it**, unlike the [macOS coalescer](../../wolf-macos/Sources/WolfKeyMap/README.md#pickerrefreshcoalescerswift). Widening the rule would put the two picker smokes at risk for nothing this crate needs.

**Gotchas**

- The interval's value is asserted nowhere — a constant compared against its own literal states nothing. What the tests pin is the mechanism.

#### `render.rs`

Purpose: the Cairo/Pango paint seam — one grid and its highlight table onto any Cairo context.

**Public API**

- `CellMetrics::for_font(cr, family, size_pt, line_height_percent)` — `cr` supplies only a Pango context. The cell is measured from `family` alone; `font` names `family_list(family)`.
- `family_list(configured)` — `configured`, then `SYMBOLS_FAMILY` unless a member names it, as fontconfig compares names.
- `paint(cr, grid, highlights, metrics)` — one grid, not a `Snapshot`: under `ext_multigrid` each surface paints its own grid.
- `draw_cells(cr, metrics, x, y, text, fg, style) -> usize` — the toast's and popup's text; returns the cells covered.

**Decisions**

- The face order, and why the cell ignores the symbols face: [Typography](../../../notes/wolf-gnome-decisions.md#typography).
- `paint` and `draw_cells` draw cell by cell through one `GlyphPainter`: a Pango run advances by whole-pixel-rounded glyph widths, not `metrics.cell_w`, outgrowing its cells; each `create_layout` costs a `PangoContext` and a font-map lookup.
- The painter clears its attributes before an undecorated cell and re-sets the face when `(bold, italic)` changes, or one cell's underline or weight runs on.
- A glyph's move-to is `y + baseline - ascent`, not the cell top, where a taller cell's glyphs would hang.
- A row's backgrounds precede its glyphs, so no fill cuts off a symbols icon or a wide character's right half.

**Gotchas**

- `GlyphPainter::new(cr)` and `draw(cr, …)` take the context twice, unbound; every caller passes the same one.
- The fallback colours, `GlyphPainter`, `draw_glyph`, `set_source` and `draw_text_line` are `pub(crate)` for [`cursor.rs`](#cursorrs), [`pane_view.rs`](#pane_viewrs) and [`terminal/canvas.rs`](terminal/README.md#canvasrs); a copy drifts from the grid paint.

#### `save_ui.rs`

Purpose: the pure, GTK-free rulebook for Save and Save As — whether a press writes or opens a dialog, what that dialog proposes, and what a chosen path becomes. [`shell.rs`](#shellrs) carries the answers out.

**Public API**

- `save_press(liveness, target) -> SavePress` (`Write`, `Choose`, `Refuse(reason)`) and `save_as_press`, whose accepted arm is always `Choose`. Both are total over `Option<Liveness>` with no `_` arm, and decide liveness ahead of the buffer's shape.
- `SaveTarget` — path, preview flag and terminal-pane flag, which must come from one snapshot pass.
- `save_dialog_proposal(path, cwd)`, `save_as_target(chosen)`, and `UNTITLED_SAVE_NAME`.

**Decisions**

- A named buffer is written, never prompted for, a never-written `Untitled N.md` included: `runtime/nvim/init.lua` has already chosen and claimed that name, which the pane's statusbar shows. Only a buffer with no file name takes the dialog. An existence probe would make `Ctrl+Shift+S` disagree with a typed `:w`, and would leave the buffer unwritten whenever the dialog failed to present.
- The proposal opens in the buffer's own parent, falling back to the window's cwd, the empty state's home directory included, which no tab persists as its project. A dialog's folder is a starting point, not a claim that a directory is a project.
- A chosen path is used verbatim, with no extension appended: rewriting it would mean the dialog confirmed an overwrite of a different file. `force` is `true` because that confirmation has already happened.

**Gotchas**

- The `Alive` gate is not the blocking-call gate: neither press blocks ([Crash and stall recovery](../../../notes/wolf-gnome/crash-and-stall-recovery.md)).

#### `scrollbar.rs`

Purpose: a Neovim window's `win_viewport` projected onto a pane-local thumb rectangle and its colour. GTK-, Cairo- and Pango-free, so the whole rulebook runs in the fast suite; [`pane_view.rs`](#pane_viewrs) only fills the rectangle.

**Public API**

- `scrollbar_thumb(pane_w, grid, viewport) -> Option<ScrollbarThumb>` — `None` when the whole buffer fits, when the pane is too narrow for the gutter, or on degenerate input.
- `scrollbar_thumb_color(bg) -> Rgb`, over the editor's own background.
- `terminal_viewport(bar) -> Viewport` — a terminal frame's scrollbar as the viewport `scrollbar_thumb` projects, so a terminal pane's thumb takes the same path.
- The five constants those two read, `SCROLLBAR_THICKNESS_PX` through `SCROLLBAR_GREY`.

**Decisions**

- A scrollbar here is a projection and never a control, painted rather than a `gtk::Scrollbar` ([Window structure](../../../notes/wolf-gnome/window-structure.md)).
- `x` is measured from the pane's right edge, not the grid rectangle's, so the thumb lies wholly inside the right padding gutter and covers no glyph at any pane width.
- It never enters `grid_size_for_pane`: a thumb consuming grid width would drop a column the instant a buffer overflowed its window and restore it when it fit, reflowing text on the scroll path.
- The height clamps to the track as well as to its own floor, so a pane shorter than the floor never draws a thumb over the pane below.
- The colour uses [`statusbar_layout.rs`](#statusbar_layoutrs)'s `blend`, which is what keeps the thumb off an alpha-capable draw path.

**Gotchas**

- `Viewport::curline` is deliberately unread: a cursor move that scrolled nothing must not make the indicator jump.

#### `search_panel.rs`

Purpose: [`sidebar.rs`](#sidebarrs)'s Search page — a field and three toggles over streaming ripgrep matches grouped by file.

**Public API**

- `build(core, opener, area) -> SearchPanelHandle` — gate shut until the sidebar shows it; `area` is a [`picker.rs`](#pickerrs) `FocusTarget`.
- `SearchPanelHandle::widget`, `focus_entry`, `owns_keyboard`, `set_sidebar_visible`, `fields`, `restore(fields, run)`, which moves no focus, `close`, cancelling `rg`, `release_core`.
- Pure seams, among them `SearchToggles` (`INITIAL`), `search_options_for`, `enter_action`, `activation_route` and `search_status`, in the module doc.

**Decisions**

- Typing and toggling run nothing; a toggle stales the status. Enter runs a changed query or toggles, activating on unchanged ones; an empty query clears, since `rg` would match every line.
- The list never takes focus, since each batch replaces every row; Up and Down move the selection from the field.
- `activation_route` reads liveness first, as nothing covers the rows on a crash, keeping the blocking `preview_file` on `Alive`; a declined repoint falls back to `open_at`, and an admitted route tries `opener` first.
- Regex on and `All` off (Markdown-only) match the counterpart; case starts off, since `ignorecase()` blocks.
- A toggle never takes focus on click: it would take Enter and flip back.
- `focus_entry` grabs synchronously: the palette closes before dispatching, so nothing retakes focus.
- The list's `navigation-sidebar` class keeps the selection tint neutral, for OCR of the preselected row.
- Escape, focus, the case default and the status line diverge from the [macOS panel](../../wolf-macos/Sources/WolfMacOS/README.md#searchviewswift); the module doc says why.

**Gotchas**

- Only a cross-file jump records history: `NavTarget::Edit` carries no line number.

#### `shell.rs`

Purpose: the glue for the action namespace — `gio` actions, the primary menu, the chord controller, the empty state, the command drain, and per-tab crash recovery.

**Public API**

- `build_menu_model()`, `build_empty_state()`, `build(parts) -> AppShell`; `ShellParts` carries a `Weak` coordinator and [`TitlebarStyle`](#titlebar_backgroundrs).
- `install_actions(app)` — the `win.` rows, before `present()`, so the menu resolves them.
- `apply_selected_tab()`, `record_selected_sidebar()`, `on_tab_wake(tab)`, `on_liveness_wake(tab) -> bool`, `on_render_wake(tab)`, `restore_selected_search()`.

**Decisions**

- The chord controller matches keys itself: GTK's keymap-aware matching does not fire under the smokes' keystroke injector. Accelerators are registered only for the menu's labels, `app.` rows included ([`coordinator.rs`](#coordinatorrs)).
- The controller lives on the window, ahead of any overlay's, and needs no open-overlay guard: no overlay key is a bound chord.
- One settle point, over [`workspace.rs`](#workspacers)'s `reroot_decision`, moves the sidebar's root for a typed `:cd`, a switch and a filled tab alike.
- Each Markdown-preview press reads one `preview_shape` pass, since two would describe no window that existed, and is a blocking RPC gated by [`preview_ui.rs`](#preview_uirs).
- Save and Save As read one `save_shape` pass each for the same reason and issue no blocking call.
- `sidebar.set_visible` has one caller, over [`state_seam.rs`](#state_seamrs)'s `sidebar_is_shown`, or a tab switch would undo a user toggle. The search chord and a [toggle click](#sidebar_togglesrs) reveal it through that preference. New Tab captures what it shows ([why](../../../notes/wolf-gnome/application-state.md#new-tab)).
- A switch first records the sidebar into, and captures, the tab it leaves ([why](../../../notes/wolf-gnome/application-state.md#the-extras)).
- Actions resolve the selected tab's core per use, and commands drain from it alone ([why](../../../notes/wolf-gnome/window-structure.md)).

#### `sidebar.rs`

Purpose: the window's sidebar — a `gtk::Stack` with two pages, Files ([`tree_view.rs`](#tree_viewrs)) and Search ([`search_panel.rs`](#search_panelrs)), and no switcher. It owns the header bar's [toggle group](#sidebar_togglesrs), which chooses the page.

**Public API**

- `build(tree, search) -> SidebarHandle` — opens on Files, both pages dormant and no toggle active until the shell relays the sidebar shown.
- `SidebarHandle::widget`, `toggles`, `page`, `show_page(page)`, `set_sidebar_visible(shown)`, `is_shown` and `focus_is_inside`.
- `panel_visibility(sidebar_shown, page) -> PanelVisibility` — the pure seam: a page is live only when the sidebar is shown and the page is current.

**Decisions**

- Both pages stay built, so a switch keeps the tree's expansion and the panel's query, rows and running `rg`. Every switch and every show or hide is relayed into both pages' refresh gates, so the page nobody can see rebuilds nothing and catches up once.
- The same relay is the only site that sets the toggle group, from `active_toggle`, so the chords, the palette, the empty state and launch keep the group right with no sync of their own.
- `show_page` is the only writer of the page, and lands in the stack's `visible-child-name` notify, which relays and focuses the panel's field on a switch to Search.

**Gotchas**

- Each tab records the page ([how](../../../notes/wolf-gnome/application-state.md#the-extras)); the stack itself opens on Files.

#### `sidebar_toggles.rs`

Purpose: the header bar's sidebar control, an `adw::ToggleGroup` of a Files and a Search toggle after Back/Forward, the last item at the bar's start, which shows the sidebar's page, switches it and hides the sidebar. The counterpart of [`SidebarToolbarSeam.swift`](../../wolf-macos/Sources/WolfKeyMap/Sidebar/README.md#sidebartoolbarseamswift).

**Public API**

- `active_toggle(shown, page)`, `toggle_click(shown, page, clicked) -> ToggleClick` and `toggle_at(x, width, rtl)` — the pure seams: the active toggle, the click rule (the active toggle hides, any other click shows its page), and which toggle a group-local `x` falls on.
- `SidebarToggles::build`, `widget`, `show_state(shown, page)`, `set_sensitive` and `connect_click(f)`.

**Decisions**

- `adw::ToggleGroup` emits nothing for a click on its active toggle, which must hide the sidebar. So a capture-phase `gtk::GestureClick` claims every primary press before the toggles see it and maps the release to a half with `toggle_at`, which holds only while the group is homogeneous. The claim costs the toggles their pressed look.
- Nothing may connect to the group's `active` or `active-name` notify: the relay's `show_state` would then re-enter the click path.
- The group never takes focus, so Space and Enter stay with Neovim and keyboard activation cannot run the group's own logic around the rule.

**Gotchas**

- The Files tooltip names no chord: Ctrl+Shift+B toggles whichever page is current.

#### `stacking.rs`

Purpose: the pure window stacking order and per-window tab recency of [application-state.md §3](../../../specs/application-state.md#3-stacking-order-and-tab-recency), kept from events because Wayland lets a client query neither.

**Public API**

- `StackingOrder` — `seeded(windows)` from the saved state, frontmost first with each window's minimized flag; `focus`, `minimize`, `remove`, `is_minimized`; `order(live)`, every live window once, frontmost first: windows not minimized before minimized ones, each class by rank, never-ranked windows after the ranked ones of their class in `live`'s order.
- `TabRecency` — `seeded(tabs, active)`, `select`, `remove`, and `indices(tabs)`, most recent first with never-selected tabs following in order.

**Decisions**

- It diverges from [`StackingOrder.swift`](../../wolf-macos/Sources/WolfKeyMap/Persistence/README.md#stackingorderswift) on purpose. Windows rank, not tabs, because a GNOME window's tabs never move to another window. Only Wolf's own Minimize marks a window, because Wayland never tells a client it is minimized; any focus unmarks it ([why](../../../notes/wolf-gnome/application-state.md)).

#### `state_seam.rs`

Purpose: the pure application-state seam — what to restore at launch, and what each tab and window contributes to the saved window state. GTK-free, IO-free, every input injected. [Application state](../../../notes/wolf-gnome/application-state.md) owns the rules.

**Public API**

- `restore_plans(saved, process_cwd, home, is_dir) -> Vec<RestorePlan>` — one plan per saved window with tabs, frontmost first, plus a fresh one in front for a terminal launch from a directory no saved tab holds.
- `tab_entry`, `seeded_tab`, `sidebar_entry`, `ordered_tab_states`, `window_entry` — the write side, one tab's `Tab` up to a window's `Window` and tab recency. `TabPlan` carries `restored_sidebar`'s page and fields and the drawer share.
- `window_state(windows, stacking)`, `close_commit(shutting_down, other_live_windows) -> CloseCommit`.
- `present_after(minimized) -> Vec<Option<usize>>` — the restore's present schedule, backmost first: the index of the window each waits to paint, `None` to present at once.
- `new_window_size`, `active_size` — [§7](../../../specs/application-state.md#7-new-windows-and-tabs)'s size: stored, else the frontmost window's normal size, else `None`: the default.
- `restored_window_size`, `sidebar_is_shown`, `snapshot_cwd`, `may_snapshot`, `restore_runs_search`, and `may_capture_cursor`, `Alive` alone.

**Decisions**

- Three answers mean "commit nothing" rather than "commit an empty thing": `window_entry` with no tabs, `window_state` with no windows, and `restored_window_size` on any unusable dimension. `restore_plans` is the inverse and never answers an empty vec.
- `MAX_RESTORE_NEOVIMS` caps the session's processes, one per restored tab, against a corrupt saved state; `MAX_RESTORE_TABS` caps one window's launch cost.
- `ordered_tab_states` stably sorts tabs into page order: the user can rearrange the bar.
- The empty state always persists an empty directory, or every launcher start becomes a project at home.

#### `statusbar_layout.rs`

Purpose: the per-pane statusbar's rulebook, decidable from values.

**Public API**

- `statusbar_layout(primary_font_size, secondary_font_size, vertical_padding, line_gap) -> StatusbarLayout`.
- `secondary_font_size(primary_font_size) -> f64`, and `statusbar_text(fg, bg, active) -> StatusbarText` over the editor's own default foreground and background.
- The seven named constants those two read, `STATUSBAR_LINE_FACTOR` through `STATUSBAR_INACTIVE_ALPHA`.

**Decisions**

- Unit-agnostic ([`cell_metrics_math.rs`](#cell_metrics_mathrs)); [`pane_view.rs`](#pane_viewrs) calls it in device pixels, from `CellMetrics::size_px`.
- The band is the editor's default background and the text has no label-tier fallback, by the user's decision: this front-end takes no colour from the GTK theme ([Window structure](../../../notes/wolf-gnome/window-structure.md)). The macOS [`StatusbarText.swift`](../../wolf-macos/Sources/WolfKeyMap/README.md#statusbartextswift) applies the same constants, blend and rounding, and its tests repeat `statusbar_text_table`, so change both together.
- Both colours blend toward the background rather than toward black, which is what compositing over the already-filled band gives, so the strip needs no alpha-capable draw path.
- A degenerate input collapses the whole layout to zeros: an infinite strip height leaves an editor painting one cell. `secondary_font_size` answers the floor for any non-finite font size, so an infinity never reaches the layout.

**Gotchas**

- `blend` is `pub(crate)` for a second consumer, [`scrollbar.rs`](#scrollbarrs)'s thumb colour. Reimplementing it there would give two colours on one scale two rounding rules, which is how they drift apart.

#### `tab_view.rs`

Purpose: the GTK glue for native tabs, each owning its `WolfCore`, wake drain, restart state, [`pane_view.rs`](#pane_viewrs) and [`persistence.rs`](#persistencers) handle, carrying out [`tabs_ui.rs`](#tabs_uirs)'s decisions.

**Public API**

- `build(TabsParts) -> TabsHandle`, with no page until `open_tab`.
- `TabsHandle::open_tab(TabParts) -> Option<TabHandle>` starts its core and selects it, or logs once. `replace(old, parts)` does so in `old`'s place; hold no core borrow across it.
- `select`, `selected`, `selected_core`, `core_source`, `entries`, `len`, `focus_target`, `shutdown_all`, `view_widget`, `bar_widget`, `seed_recency`, `recency`, `drawer_opener`.
- `connect_selected`, given the tab left; `connect_wake`; `connect_closing`, before a tab's shutdown.
- `sync_ui_focus(window_active)`, the `:checktime` trigger; `active_terminal()` and `sync_terminal_focus(window_active)` ([how](../../../notes/wolf-gnome/terminal-panes.md)).
- `TabHandle`'s `core`, `liveness`, `restart`, `wake`, `refresh`, `refresh_label`, `capture_viewports`, `shutdown`, `set_empty`, `hide_empty_state`, `is_empty`, `wolf_tab_id`, `launch_cwd`, `live_directory`, `new_tab_sidebar` and sidebar accessors; `CoreSource` and `with_core`.

**Decisions**

- A tab's wake channel exists before its core starts, so the startup flush's wake is queued. The core's [tab ID](../../../specs/cli-and-file-open.md#3-tab-ids) is set first, so no terminal session lacks it.
- Window-level widgets take a `CoreSource`, never a cell, stale after a switch, and try `drawer_opener`'s `FileOpener` before `InputEvent::Open`, which would edit in an active drawer ([why](../README.md#entry-point)).
- `close-page` is a veto: every arm answers `close_page_finish` and returns `Propagation::Stop`, or the page stays. Closing the last page closes the window, so `replace` opens first, keeping `old` on failure.
- Handlers and drains hold only `Weak`s: a strong one leaks every tab's pane view.
- A tab is titled from its own Neovim's `live_cwd`, never the shell's read ([why](../../../notes/wolf-gnome-decisions.md#threading-and-the-wake)).

**Gotchas**

- `entries` is in insertion order; `TabHandle::page_position` gives the user's arrangement.

#### `tabs_ui.rs`

Purpose: the pure, widget-free rulebook for a window's native tabs: what closing one does, the title and tooltip a tab shows, and which tab's Neovim holds the UI focus. [`tab_view.rs`](#tab_viewrs) carries the answers out.

**Public API**

- `close_outcome(live_pages) -> TabClose` — `CloseTab` or `CloseWindow`; `live_pages` counts the page being closed.
- `tab_label(empty, cwd, home) -> TabLabel` — the title is `title::window_title` of `cwd`, so the selected tab's equals its window's. An empty tab, or one whose cwd is not yet reported, is titled `title::WINDOW_TITLE_FALLBACK` with no tooltip, so libadwaita shows the title. Otherwise the tooltip is `cwd` in full, as Pango markup.
- `TabId`, `FocusEdge`, `focused_tab(window_active, selected)` and `focus_edges(previous, next)` — the focus trigger.

**Decisions**

- `close_outcome` routes the last tab's close into the window's: a window with no tab holds a tab bar and no editor, with no way to reach one.
- `focus_edges` is computed against the last focus sent, because Neovim fires `FocusGained` on every `set_ui_focus` with no de-duplication. An unchanged focus sends nothing, and a switch sends the loss before the gain, so two Neovims never both hold it.
- The tooltip is escaped because `AdwTabPage:tooltip` is Pango markup: a `&` or `<` in a directory name would fail to parse rather than show.
- The title is never shortened or measured in Wolf: libadwaita fades one too long for its tab.

#### `terminal.rs`

Purpose: the root of the terminal-pane modules. A pane whose window shows a `wolf-core` terminal session paints from the session's frame rather than from the placeholder's grid; the seams and the painter are in [`terminal/`](terminal/README.md), and [Terminal panes](../../../specs/terminal-panes.md) owns the contract.

**Public API**

- `SMOKE_TERMINAL_ENV` and `smoke_terminal_command(raw)` — the test-only `WOLF_SMOKE_TERMINAL` hook and its rule: the trimmed value is the program, with no arguments, and an absent or blank value opens nothing.

**Decisions**

- The hook opens the session before the app reports ready, so a smoke's first typed key reaches its program. [`app.rs`](#apprs) reads the variable, and [`pane_view.rs`](#pane_viewrs) opens the session in the first wake with a pane ([why there](../../../notes/wolf-gnome/terminal-panes.md)).

#### `title.rs`

Purpose: the pure title seam of the window and of each tab — the working directory, rendered, with the home prefix abbreviated.

**Public API**

- `display_title(cwd, home) -> String` and `window_title(cwd, home) -> String`, which substitutes `WINDOW_TITLE_FALLBACK` for an empty result.

**Decisions**

- `home` is always injected, never read from the environment, which is what makes the rule decidable from its arguments. The single environment read in this crate lives in [`workspace.rs`](#workspacers), and the shell captures it once at build time.
- Abbreviation matches on a full path-segment boundary only. A directory sharing a character prefix with home is not under it and must be returned verbatim.
- A home of `/` disables abbreviation entirely, because abbreviating every absolute path would be actively misleading.
- The window builder seeds the same fallback constant as the initial title, so the title never flickers between two spellings before the first directory is reported.

#### `titlebar_background.rs`

Purpose: the background of the header bar and the tab bar, by the rule [Theming](../../../specs/architecture.md#theming) states: the pure decision, the stylesheet that carries it, and the per-window provider that applies it. The counterpart of [`TitlebarBackground.swift`](../../wolf-macos/Sources/WolfKeyMap/README.md#titlebarbackgroundswift).

**Public API**

- `titlebar_background(titlebar, normal) -> TitlebarBackground` — `normal` is the resolved default background before the grid's fallback colour, so an unknown `Normal` keeps libadwaita's colour rather than painting the fallback.
- `titlebar_css(class, background)`, `""` for `Libadwaita`, and `stylesheet_update(applied, wanted, class)`, `None` when the two are equal, so a wake that changes nothing does no CSS work.
- `TitlebarStyle::install(id, header, tab_bar)`, `apply(wanted)` and `uninstall()` — [`shell.rs`](#shellrs) applies the selected tab's colour on that tab's render wakes, and [`window.rs`](#windowrs)'s close handler uninstalls.

**Decisions**

- One provider per window, following the selected tab, since each tab's Neovim may run its own colorscheme, scoped by a class on the header bar and the tab bar themselves. A descendant selector under the window would also match a header bar that a child widget brings.
- `uninstall` at close is required: a closed window's id is minted again, and its provider would restyle the new window.
- The rule declares `background-color` alone, at application priority and with no state selector ([why](../../../notes/wolf-gnome/window-structure.md)).
- The tab bar's node is its `box.box`, the one libadwaita colours; the `tabbar` node paints nothing.

#### `tree.rs`

Purpose: a re-export shim over `wolf_core::tree`. The hierarchical projection of the flat file list and its tests live in the core as one source of truth shared by every front-end; this module keeps the `crate::tree::…` paths resolving for the GTK glue.

#### `tree_gate.rs`

Purpose: the pure, GTK-free gate every refresh of the sidebar's two pages passes — visibility first, snapshot version second.

**Public API**

- `TreeRefreshGate::new(visible)`, `request_refresh() -> bool`, `set_visible(visible) -> bool`, `should_rebuild(version) -> bool`, `reset_version_tracking()`.
- A refresh requested while hidden is refused and recorded as owed; `set_visible` answers `true` for the one catch-up that owes, and `false` on every other transition.

**Decisions**

- The version half is what stops a current-file change collapsing manual expansion: rebuilding the root store destroys each row's `TreeListRow` along with its expanded state, and a current-file change advances no version.
- `reset_version_tracking` exists because `wolf_core::files::FileSearchState` numbers versions per `FileSearch` rather than globally. A search started by a re-root begins again at 1, so a gate that kept its tracking would answer "no rebuild" to the new project's first batch and leave the departed project's files on screen. The search panel resets on every run for the same reason: each `SearchSession` numbers its versions from 1.

**Gotchas**

- Hiding the sidebar destroys no store, so a catch-up at an unchanged version correctly rebuilds nothing. The deferral is for a version that advanced while nobody could see it.

#### `tree_view.rs`

Purpose: the file-tree sidebar, a `gtk::ListView` over a `gtk::TreeListModel`.

**Public API**

- `build(core, opener, area, start, root_label) -> TreeHandle` — a file click tries `opener` first; `area` is a [`picker.rs`](#pickerrs) `FocusTarget`; `start` is false in the empty state, where nothing is walked.
- `widget()` — [`sidebar.rs`](#sidebarrs)'s Files page.
- `release_core(core)`, on a tab's close.
- `on_render_wake()`, `close()`, `set_sidebar_visible(shown)`, relayed through `sidebar.rs` and true only while a shown sidebar is on Files, and `reroot(root_label)`, from [`shell.rs`](#shellrs); this module reads no cwd.

**Decisions**

- The root store holds one synthetic `Dir` named after the working directory, whose rel-path is the empty cwd sentinel, so the create-func needs no special case. `remove_all` destroys its expanded state, so every rebuild re-expands it.
- Children materialize lazily through `wolf_core::tree::TreeIndexCache` under rule R1: memo and entries are borrowed in one statement touching no widget, because GTK re-enters the create-func and a borrow held across `set_expanded` or `set_list_row` panics.
- Only a traversal batch is coalesced ([`refresh_coalescer.rs`](#refresh_coalescerrs)); every other refresh is immediate, and all pass both gates in [`tree_gate.rs`](#tree_gaters).
- A pending reveal retries on every refresh, and a rebuild re-reveals the current file. The scroll runs from a frame-clock tick, while mapped and after layout: an earlier `scroll_to` draws rows blank ([The sidebar reveal](../../../notes/wolf-gnome/the-sidebar-reveal.md)).
- The current-file bold moves through a registry of bound labels, which is what the version gate costs: a suppressed rebuild re-binds no row.

**Gotchas**

- Manual expansion below the root does not survive a rebuild; see [Present limitations](../../../notes/wolf-gnome/present-limitations.md).

#### `window.rs`

Purpose: one window and everything scoped to it — an `adw::ApplicationWindow` over its tabs ([`tab_view.rs`](#tab_viewrs)) and the window's own overlays. [Window structure](../../../notes/wolf-gnome/window-structure.md) owns its widgets and stacking.

**Public API**

- `build(parts) -> Option<WindowHandle>` — `None` when no planned tab's core starts; a launch whose only window fails exits.
- `WindowHandle::id`, `present`, `raise`, `show_after(others)`, `cancel_present`, `refresh_subtree`, recording the selected tab's sidebar first, `capture_viewports`, `cached_window`, `normal_size`, `display_cwd` and `readiness`, of the selected tab.
- `live_window`, `selected_tab`, `tab_is_empty`, `new_tab_sidebar`, `focus_tab`, `fill_tab`, `wolf_tab_ids`, `open_file` — [Open Folder](#open_folderrs)'s and [a request's](#open_requestrs).

**Decisions**

- The close handler's statement order is load-bearing at both ends. Telling the coordinator comes first, or the gathered entry sees nothing and the session is lost; focusing the input method out and clearing its client widget comes last, after every core shuts down, so a focus-out commit finds no core and the context releases the widget tree.
- Each tab's first drain is a wake queued after the window registers and installs its close handler: an earlier drain could reach a coordinator unaware of it, or close and orphan every tab's Neovim.
- `display_cwd` is the cached entry's active `directory`, so palette and save cannot disagree.
- Keys, commits and focus reach the selected tab ([terminal panes](../../../notes/wolf-gnome/terminal-panes.md)).

**Gotchas**

- The key controller carries no propagation phase and offers each press to the window's `gtk::IMMulticontext` before translating, so a bound chord reaches neither Neovim nor the context — [`shell.rs`](#shellrs)'s capture-phase controller has already stopped it.

#### `workspace.rs`

Purpose: the pure workspace decisions — is a project open, where does Neovim start, and where is the sidebar rooted.

**Public API**

- `launch_decision(process_cwd, restored, home, is_dir) -> LaunchDecision` — `Folder`, or `EmptyState` carrying the directory Neovim starts in.
- `reroot_decision(live, rooted) -> RerootAction` — stay, or re-root where Neovim is; `root_label(cwd)` is the root row's text.
- `home_dir()`, the one environment read here, which the shell captures once at build time.

**Decisions**

- Two candidates are offered and the first to satisfy one shared predicate wins: a directory, not home, not the root. [`state_seam.rs`](#state_seamrs)'s `restore_plans` offers them one at a time: the process cwd alone, because a terminal launch from a project is the Linux idiom, and each saved tab's directory alone. The same predicate on a restored directory stops a hand-edited saved state aiming the file search at the whole filesystem.
- A re-root's authority is `rooted`, where the sidebar sits, against the cwd Neovim reports, and the settle that re-roots updates `rooted`, so one directory change is exactly one re-root. No front-end request takes part: Open Folder never chdirs a tab.
- No project predicate guards a re-root: a typed `:cd /` is an explicit act, not the launch-time guess `is_usable_project` judges, with core's search bounds as the backstop.
- Comparisons are plain equality, so callers canonicalize both sides: a trailing separator is ignored, `..` and symlinks are not.
