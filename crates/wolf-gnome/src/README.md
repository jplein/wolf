# wolf-gnome/src

The GTK front-end's modules, one entry each. The pure seams here are the Linux counterparts of the ones in [`wolf-macos/Sources/WolfKeyMap/`](../../wolf-macos/Sources/WolfKeyMap/README.md), and the two are kept deliberately close.

**Read [DECISIONS.md](../DECISIONS.md) first**: it owns every constraint that spans more than one file here, and most of this directory's rules exist because of one.

## Files

#### `about.rs`

Purpose: the About dialog — a pure text seam over the bundled fonts' licence notices, plus the transient window that shows it. This is the one surface on this front-end that discharges the fonts' attribution obligation, so its content has legal weight rather than cosmetic weight.

**Public API**

- `about_text() -> String` — the whole dialog body. It takes no arguments and reads `wolf_core::FONT_LICENSE_NOTICES` itself, deliberately: a version taking the notices as a parameter would let its test pass them in, which proves nothing about what ships.
- `present(parent)` — a modal `gtk::Window` transient for the main window, titled `ABOUT_TITLE`.

**Decisions**

- Plain GTK over `adw::AboutDialog` or `adw::AboutWindow`, which need `libadwaita` version features this crate deliberately carries none of ([DECISIONS.md](../DECISIONS.md)): adding one for a single dialog would change the build contract and put the whole smoke suite on the devShell's libadwaita version.
- A separate toplevel rather than a fifth in-window overlay, so the main window's focus widget is untouched and the focus regime does not apply to it at all.

#### `actions.rs`

Purpose: the action namespace as data — one row per `gio` action — plus the chord matcher and the invariant checker.

**Public API**

- `ACTION_TABLE` and `MENU_SECTIONS` — the one table the menu, the accelerators and the command dispatch all read; see [DECISIONS.md](../DECISIONS.md).
- `action_for_key(keyval, state)` — the matcher the chord controller calls. It scans `Bound` rows only: matching a reserved row would swallow the key and do nothing.
- `parse_accel`, `ctrl_fold` and `forbidden_reason(&ParsedAccel, declared)` — the invariant checker, run over every row by this module's tests. It takes the declaration as a parameter so no caller can run half of it.

**Decisions**

- `parse_accel` re-implements the accelerator grammar this crate allows rather than calling `gtk::accelerator_parse`, which asserts an initialized main thread and cannot run headlessly.
- `action_for_key` compares its three modifiers exactly and ignores every other bit: intercepting a Super-contaminated chord beats forwarding one Neovim mangles. Letters compare case-insensitively, since a shifted letter's keyval is keymap-dependent.

**Gotchas**

- Whether an action is installed follows from its command's classification, not from `status`: a command-less row installs through `command_action::ui_action_for_front_end_action` instead. Promoting a reserved row means flipping `status` too, and declaring the byte a `Ctrl+<symbol>` chord folds onto, or the chord stays inert or the checker refuses the row.
- A row's `win.` or `app.` prefix decides who installs it: [`coordinator.rs`](#coordinatorrs) the `app.` rows once per process, each window's [`shell.rs`](#shellrs) the `win.` ones ([DECISIONS.md](../DECISIONS.md#the-coordinator-and-its-windows)).

#### `app.rs`

Purpose: the entry point — activate, and delegate. `run` builds the `adw::Application`; the first activate builds the coordinator, installs the `app.` actions on it and restores the saved session, and any later one opens a fresh window.

**Public API**

- `run() -> glib::ExitCode` — hands GTK an empty argv, so the process command line is never parsed or rejected.
- `load_config`, `measure_metrics`, `open_state_store` and `init_lua_path` are `pub(crate)`, called once each by [`coordinator.rs`](#coordinatorrs).

**Decisions**

- The coordinator is held in an `Rc<RefCell<Option<_>>>` captured by the activate closure rather than in a `static`. This crate has no process-global state of its own, and keeping it that way is what makes two cores in one process independent.
- Nothing is built before the first activate: the config read and the cell measurement both need a running GTK, and the state store must not be opened in a process that turns out to be a remote instance.
- An unresolvable preferences or state directory degrades with one message — the built-in config defaults, or a store whose every operation is a no-op — never a per-wake stream.

**Gotchas**

- The later-activate path is a desktop's "launch it again, get a window". No smoke reaches it: the harness strips `DBUS_SESSION_BUS_ADDRESS`, so a second process becomes its own primary instance instead.

#### `appearance.rs`

Purpose: the OS dark/light rulebook — what a configured `appearance` resolves to against the desktop's preference — plus the glue that applies it at launch.

**Public API**

- `appearance_decision(configured, os_dark) -> ResolvedAppearance` — total over `wolf_core::Appearance` with no `_` arm, so a fourth variant is a compile error here.
- `apply_at_launch(config, core)` — makes the one `StyleManager::is_dark()` read and carries the decision out; its step order is load-bearing.

**Decisions**

- `os_dark` is a parameter and is never read here: `adw::StyleManager::default()` asserts an initialized main thread, so a seam reaching for it could not run headlessly.
- A forced appearance takes `ForceDark`/`ForceLight`, never `PreferDark`/`PreferLight`, which yield to the desktop preference the user wrote the config line to override.
- The notify handler pushes `background` and never re-sets the scheme, and is armed after the launch-time set: re-applying a property from inside its own notify handler is the shape that loops.

**Gotchas**

- What `set_color_scheme` reaches beyond the chrome, and the GTK version that ends it: [The preview's light and dark](../DECISIONS.md#the-previews-light-and-dark).

#### `buffer_picker.rs`

Purpose: the buffer-picker overlay — a search entry over the captive Neovim's listed buffers, ranked by `wolf_core::rank_buffers`. Activating a row switches the active window to that buffer.

**Public API**

- `build(overlay, area, core) -> BufferPickerHandle` — starts hidden; `area` is a [`picker.rs`](#pickerrs) `FocusTarget`.
- `BufferPickerHandle::open`, `close` (both idempotent), and `is_open`.
- `open_decision`, `initial_selection`, `switch_target` and `row_label` — the pure seams.

**Decisions**

- Candidates are snapshotted once at open and re-ranked per keystroke. The snapshot is a blocking RPC on the GTK main thread, so [`picker.rs`](#pickerrs)'s wake bridge must not be added for symmetry: re-taking it puts a whole timeout budget of frozen main loop on the typing path.
- `open_decision` refuses to open, and so refuses the RPC, for every liveness reading but `Alive`. It cannot prevent the first stall, and must not be strengthened into a guard that can never raise one.
- A refusal returns before any widget is touched, and nothing is shown until the snapshot returns.
- The first refresh preselects row 1, because row 0 is the buffer the user is already in and a bare Enter must switch. It is the one selection rule the other three overlays do not share, ported from macOS.
- `switch_target` refuses an active window of `0` as firmly as none at all, per `wolf-ffi`'s rule: `0` binds the switch into whichever window Neovim considers current.

**Gotchas**

- Closing does not clear the snapshot; the next open overwrites it.

#### `cell_metrics_math.rs`

Purpose: the pure cell-metrics arithmetic the configured `line-height` drives — a port of `WolfKeyMap/CellMetricsMath.swift`, named for its counterpart so the parity obligation is legible. No GTK, Pango or Cairo; every input an `f64`.

**Public API**

- `cell_height(font_size, line_height_percent, ascent, descent, leading)` — a positive, finite percentage gives `round(font_size * percent / 100)`, driven by the font size alone and independent of the raw metrics; anything else gives `ceil(ascent + descent + leading)`.
- `baseline(cell_height, ascent, descent)` — the baseline offset from the cell's top, splitting the leftover space evenly above and below the glyph box.
- `MIN_CELL_PX` — the floor a degenerate result collapses to.

**Decisions**

- The rule lives here and not in [`render.rs`](#renderrs) because `for_font` needs a live Cairo context, so a rule left inside it could only be executed by the golden-render target.
- The function is unit-agnostic and rounds in whatever unit its caller uses: points on macOS, device pixels here, since device pixels are what Cairo paints in.
- Two divergences from Swift, both strictly more defensive and both confined to inputs macOS also mishandles: the percentage must be finite (Swift's guard accepts `+inf`), and a non-finite or non-positive result is floored (Swift can return `0` or `NaN`).

#### `clipboard_ui.rs`

Purpose: the pure rulebook for Copy and Paste through the system clipboard. GTK-free; [`shell.rs`](#shellrs) owns the `gdk::Clipboard` handle and the asynchronous read.

**Public API**

- `copy_decision(liveness) -> CopyPress` and `paste_decision(liveness) -> PastePress` — both total over `Option<Liveness>` with no `_` arm; only `Some(Alive)` acts.
- `clipboard_write(selection)` and `paste_payload(read)` — `None` for an absent or empty string.

**Decisions**

- `clipboard_write`'s `None` means write nothing, never clear. Clearing would destroy whatever the user copied from another application, in answer to a chord that yanked nothing.
- Copy is a bounded blocking RPC and is gated for that reason ([DECISIONS.md](../DECISIONS.md)). Nothing on the paste path blocks: its refusals are about a paste queued behind an opaque overlay reaching a restarted Neovim minutes later, and the two reasons must not be collapsed.

#### `cmdline_ui.rs`

Purpose: the pure rulebook behind the command-line strip, the message toast and the completion popup.

**Public API**

- `cmdline_columns(prefix, text, byte)` — `prefix` is the dialog `prompt` plus `firstc`.
- The toast rules: `toast_budget_rows`, `toast_view`, `toast_scroll_next`, `toast_scroll_clamp`, `toast_width_cols`, `carried_scroll`, `toast_is_shown`, `prompt_answered`.
- The popup rules: `pum_selection`, `pum_view`, `pum_item_label`, `pum_width_cols`.

**Decisions**

- `cmdline_columns` is one function because the caret and the popup's cmdline anchor both need it, and a second copy is how a popup ends up a column away from the caret it belongs to.
- A zero offset pins the toast to its newest line — `toast_scroll_next` inverts the grid's sign for that reason — and `carried_scroll` drops the offset when the line count changes, since that is a different message.
- `pum_selection` refuses an out-of-range index rather than clamping: a selection the item list cannot explain is a fold error, and highlighting the last row would hide it.
- Both width seams measure the visible rows only: measuring the whole list would make the popup jump wider the moment a long item scrolled into range, and would leave a scrolled toast sized for a line it is not drawing.

**Gotchas**

- The offset is gesture-driven because key-driven paging is impossible; [DECISIONS.md](../DECISIONS.md) owns that rule.

#### `command_action.rs`

Purpose: the pure mapping from a drained `wolf_core::Command` to the UI action this front-end performs for it. GTK-free, so the whole classification is tested headlessly.

**Public API**

- `ui_action_for(command) -> UiAction` — total.
- `ui_action_for_front_end_action(action) -> Option<UiAction>` — what an `actions::ACTION_TABLE` row carrying no command performs: the two navigation rows and About. `None` for anything else, and it is the only other way an action is installed.
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

Purpose: the process-scoped layer above the windows — the one state store, the one config read, the one cell measurement, the ordered registry of live windows, and the `app.`-namespace actions. [DECISIONS.md](../DECISIONS.md#the-coordinator-and-its-windows) owns why each is process-scoped, and the state protocol.

**Public API**

- `build() -> Coordinator`, then `install_app_actions(app)` and `restore_session(app)`, in that order.
- `open_new_window(app)`, `commit(waking)`, `on_window_closing(id)`, `quit(app)`, `raise(id)`, `palette_windows(exclude)`, `register(handle)`, `window_count()`.
- `next_free_id(next, live) -> WindowId`, the pure id arithmetic, and `upgrade(&weak)` for the window-side closures.

**Decisions**

- The coordinator holds strong `WindowHandle`s and every window-side closure a `Weak` it upgrades per use, so a dropped coordinator performs nothing rather than panicking. The other direction is a cycle through the whole graph, so nothing would ever be dropped.
- `install_app_actions` runs before the first window exists, so the menu and the chord controller resolve `app.quit` from the first frame. Each window's [`shell.rs`](#shellrs) registers their accelerators, per-application and idempotent, so the menu draws the chord beside its Quit item.
- Quit and New Window funnel through here from a drained command, a palette row and a chord alike. A window that has lost its application or its coordinator opens nothing, and quits by closing itself.
- `quit` closes every window the _application_ holds rather than every registered one, since a transient toplevel such as About would otherwise keep the process alive.
- `next_free_id` is a free function because the wrap it guards is unreachable through the registry, and a reused id deregisters and raises the wrong window.

#### `crash_overlay.rs`

Purpose: the widgets for crash and stall recovery — the opaque overlay carrying the restart button, and the transient banner that confirms a restart. Every decision it acts on is made in [`liveness_ui.rs`](#liveness_uirs); this file builds widgets and toggles visibility.

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
- An out-of-range `mode_idx`, empty `mode_infos`, and an out-of-bounds cursor cell all fall through to inversion rather than panicking.

#### `drawer.rs`

Purpose: the terminal drawer's pure rulebook — how the pane area divides between the editor tree and the drawer strip below it, and which pane a `<C-w>h/j/k/l` intent focuses once a drawer exists. GTK-free, so it runs in the fast tier. Why the drawer sits outside the pane tree at all is in [DECISIONS.md](../DECISIONS.md).

**Public API**

- `DRAWER_WEIGHT`, and `drawer_split(pane_area_height, has_drawer, weight, min_drawer_height) -> DrawerSplit`, whose two heights always sum to the input.
- `focus_target(tree, origin, dir, state, width, height)` — the whole editor-to-drawer bridge as one value. Neovim emits a focus move as a pure intent and declines to act on it, so with no answer here a user who opens the drawer cannot leave it. `bottom_most_leaf` and `drawer_up_target` are its `<C-w>k` half.
- `DrawerState` — `Copy`, so [`pane_view.rs`](#pane_viewrs) holds it in a `Cell`; `clear()` empties both the bound drawer and the remembered editor pane, which is what a restart needs.

**Decisions**

- One module where macOS has two files, `DrawerLayout.swift` and `DrawerFocusSeam.swift`: neither has a consumer outside the drawer, and `focus_target` needs both halves in one expression.
- Three divergences from that pair, each pinned by a table rather than only stated: a `min_drawer_height` floor Swift has none of, a non-finite guard that collapses the whole result, and a `focus_target` taking the raw pane-area size where `applyFocus` passes the editor height, since `PaneTree::pane_rects` feeds only ordering here.

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
- Only `w` and `h` clamp at zero; a non-finite input collapses the whole rectangle, unlike Swift, since a NaN origin would poison every pointer event.
- The cap is a `min` on the width here and on the column count in `grid_size_for_pane`, which divides the uncapped rectangle: flooring a capped width to whole pixels costs the last column at a non-integral cell width.
- The centring offset is floored to whole device pixels, unlike macOS: a half-pixel `cr.translate` resamples every glyph and this crate has no HiDPI story.
- No `StatusbarTextSpan` counterpart and no `text_left_inset`: `grid_rect`'s `x` carries the centring offset and every consumer reads that one field, so strip and grid share an origin by construction. macOS needs the seam only because inset and cap live in two structs.

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
- `divider_position` and `divider_span_fraction` must share one denominator, and which one it is decides whether a fresh split is even; see [DECISIONS.md](../DECISIONS.md).

**Gotchas**

- `cell_at` takes grid-local coordinates, and the cap's centring offset is already in [`editor_inset.rs`](#editor_insetrs)'s `grid_rect(..).x`, which `grid_local` subtracts. So this function takes no origin, and macOS's `paintOffset` machinery has no counterpart. Do not add one.

#### `glyph_style.rs`

Purpose: the pure rulebook for the three attribute-borne decorations — underline, undercurl, strikethrough — and the colour the first two take. A port of `WolfKeyMap/GlyphStyle.swift`; [`render.rs`](#renderrs) applies the answer as Pango attributes.

**Public API**

- `glyph_decoration(&ResolvedStyle) -> GlyphDecoration` — total; `is_empty` is what lets an undecorated cell build no `AttrList`.
- `pango_channels(Rgb)` — widens by 257, never 256, so `255` reaches the top of Pango's range exactly.

**Decisions**

- The wave is Pango's (`Underline::Error`), never a hand-stroked Cairo path: the underline's placement comes from face metrics Pango reads and this crate does not.
- Pango carries one underline attribute per run, so `underline` and `undercurl` cannot coexist and undercurl wins. macOS folds both into a straight underline; the wave is a divergence pinned by [`paint_golden.rs`](../tests/README.md#paint_goldenrs).
- No strikethrough colour is ever set, so Pango strikes in the layout foreground — macOS's behaviour, bought for free.

#### `input.rs`

Purpose: translates a GDK keyval plus modifier state into a `wolf_core::KeyEvent`. It has the exact shape of a `gtk::EventControllerKey` callback but stays a free function so it is testable without a display server.

**Public API**

- `gdk_key(keyval, state) -> Option<KeyEvent>` — `None` for bare modifier and lock keys, and for any keyval whose unicode value is absent or a control character.

**Decisions**

- Named keys resolve before the printable fallback, because their `to_unicode()` is often a control character (`Return` is `\r`) and would otherwise be swallowed by the control-character rejection.
- For printable characters `shift` is always false: GDK already delivers the shifted glyph in the keyval, and `KeyEvent` ignores `shift` for `Key::Char`. Named keys pass the real shift through, which is what keeps `<S-Tab>`, `<S-Left>`, and `<S-F5>` working.
- `ISO_Left_Tab` is what Shift+Tab actually reports on X11, so it maps to `Key::Tab` with `shift` forced true even when the platform did not also set the shift mask.

**Gotchas**

- `SUPER_MASK` maps to `meta`, so an unmapped Super chord is forwarded for Neovim to mangle ([DECISIONS.md](../DECISIONS.md)); the chord controller intercepts only the ones this crate binds.

#### `layout.rs`

Purpose: the pure, GTK-free n-ary pane-layout tree behind the split-panes UI, mirroring Neovim's frame model.

**Public API**

- `PaneTree::split(target, new_pane, orientation)` is a no-op on a non-leaf target, and `remove(pane)` on the last pane, so the tree is never empty.
- `PaneTree::leaves()` and `pane_rects(w, h)` — the same deterministic depth-first order; the rects tile the bounds exactly.
- `PaneTree::neighbor(from, dir, w, h)` — the nearest pane in `dir` overlapping `from` on the cross axis.
- `PaneTree::layout()` and `from_layout(&PaneLayout)` — a read-only mirror for the widget layer and its inverse; the frame type stays private.
- `PaneTree::divider_fraction(path, boundary)` and `set_divider_fraction(path, boundary, f)` — one divider, addressed by its split's path and the child it follows; the fraction is that child's share of the whole remainder, per [DECISIONS.md](../DECISIONS.md).
- `Orientation` — `Vertical` divides width, `Horizontal` height, matching `wolf_core::SplitOrientation`.

**Decisions**

- A split whose orientation matches its containing frame inserts a flat sibling and re-equalizes the row; only a perpendicular split nests. A binary always-nest model turns a third same-orientation split into 25/25/50, starving the pre-existing pane.
- `remove` normalizes bottom-up — flatten same-orientation children, collapse a one-child split, re-equalize — so the tree stays canonical, matching Neovim's `equalalways`.
- Cross-axis overlap must be strictly positive within `NEIGHBOR_EPS`, so a shared edge never counts and a 2x2 grid is unambiguous.
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
- The overlay and banner strings, `MAX_AUTO_RESTART_ATTEMPTS`, and the four geometry constants.

**Decisions**

- Both matches are total, with no catch-all arm, so a new `Liveness` variant fails to compile here until it is classified.
- `Alive` and `NotResponding` are insensitive to the attempt budget: a spent budget must never stop a healthy editor painting, and a wedged Neovim must never be force-killed automatically.
- `Crashed { retrying }` means an automatic attempt is still permitted, not that one is in flight — the counter is incremented when the attempt is scheduled.
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

- Only the primary button is forwarded; every other number yields `None`, which is parity with macOS.
- Every modifier bit outside `C-`, `S-`, `A-` is dropped rather than translated, because Neovim's mouse grammar has nowhere to put it. Unlike `gdk_key`, which has a meta notation.
- An unknown scroll unit falls back to the wheel threshold on purpose: guessing wheel for a pixel delta is over-sensitive and recoverable, while guessing surface for a detent delta makes scrolling dead.
- A rejected `consume` leaves the carry untouched, so it is indistinguishable from no call; a poisoned remainder would kill scrolling for the session. `PaneScroll` discards the carry when the target grid differs from the previous call, accepted or not, because a remainder is a fraction of a line of one grid.
- **The scroll sign is the opposite of macOS's, deliberately.** A positive AppKit delta shows earlier content while a positive GDK delta moves toward the end of the buffer, so a positive step count maps to `Down` here and `Up` there. A test exists to stop that being "corrected".

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

- The buttons desensitize while the `gio` actions stay enabled — the opposite of `win.go-to-file`'s empty-state treatment, for the reason in [DECISIONS.md](../DECISIONS.md).

#### `palette.rs`

Purpose: the command-palette overlay — registry commands, one Open Recent row per usable recent directory, one Switch to Window row per other window.

**Public API**

- `build(overlay, area, core, windows) -> PaletteHandle` — starts hidden; `area` is a [`picker.rs`](#pickerrs) `FocusTarget`, `windows` a `WindowSource`.
- `PaletteHandle::open`, `close` (both idempotent), `is_open`, and `connect_activate`.
- `open_recent_candidates`, `palette_rows` and `palette_action_for` — the pure seams — and `PaletteAction`, whose `SwitchWindow` payload is the coordinator's own window id.

**Decisions**

- One directory can yield both a Switch to Window row and an Open Recent row: the first raises that window, the second re-roots this one.
- `open_recent_candidates` refuses three directories: the invoking window's own, since re-rooting where it already is changes nothing; a relative path, which `chdir_to` would canonicalize against the process cwd rather than Neovim's; and one `is_dir` rejects, since a failed `:tcd` is only a `tracing::warn!` in core. The shared store is never pruned, so a temporarily unmounted path returns.
- The window list is a closure taken fresh at each open over each window's cached subtree, so the open touches no other core and adds no RPC.
- The label is `PaletteRow::display` unchanged: [`title.rs`](#titlers)'s home abbreviation would rank one string and show another, and a query matching the visible `~` would return nothing.
- Ranking runs unbounded and `max` applies after the classification filter, since `rank_palette` truncates first and undispatched rows would otherwise eat the budget.
- Activation records, closes, then dispatches: a dispatched Close Window tears the window down synchronously.

#### `pane_layout.rs`

Purpose: the pure codec between the live pane tree, whose leaves are pane ids, and the persisted `wolf_core::PaneNode`, whose leaves are buffer paths. No GTK, no serde, no IO.

**Public API**

- `resolve_leaves(&[(PaneId, WinContent)]) -> Option<HashMap<PaneId, LeafContent>>` — the step ahead of `encode`. `WinContent` is `Preview(source)`, `Buffer(path)` or `Unknown`, the last for a window in neither snapshot map.
- `LayoutGather` — one tab's gather answer: `Ready(Option<PaneNode>)`, whose `None` is content suppression, or `Refuse`.
- `encode(layout, contents) -> PaneNode` — `contents` maps each leaf id to its `LeafContent`, a raw path plus a preview flag; a missing id becomes an empty, non-preview leaf.
- `decode(node, first_leaf_id) -> Option<(PaneTree, Vec<PaneId>)>` — leaf `i` is the pane for handle `i`, ids allocated in depth-first order.
- `is_empty_leaf(node)`, `leaf_count(node)`, `leaf_paths_dfs(node)`, `MAX_RESTORE_LEAVES`, `MAX_RESTORE_DEPTH`.

**Decisions**

- `resolve_leaves` is all-or-nothing: an empty path is a resolved answer and an absent window is not, so one `Unknown` refuses the whole tab's gather. Mapping absence onto `""` writes a well-formed `state.json` over the user's files, which no arity guard catches.
- `is_empty_leaf` is the content predicate the save path gates on, and `leaf_count` must never be it; [DECISIONS.md](../DECISIONS.md) owns why.
- `decode` validates before anything is rendered, so Neovim is never asked to realize a malformed tree. The two caps are restore policy, not a limit on the tree type.
- The orientation mapping lives here in both directions, so an inversion is one edit and one test rather than two half-inversions that cancel.

#### `pane_view.rs`

Purpose: the GTK glue turning `layout`'s pane tree into nested `gtk::Paned`, one `gtk::Overlay` host per window, and the drawer strip below them.

**Public API**

- `build(core, metrics) -> PaneHandle` — panes appear with the first grid.
- `cmdline_widget()` — the focus-return target for overlays and sidebar; plus `toast_widget()` and `pum_widget()`.
- `reset_after_restart()`, `request_layout_rebuild(saved)`, `realize_layout(saved)`, `layout_snapshot() -> LayoutGather`, `snapshot_readiness()`.

**Decisions**

- A dragged divider position is written back into the tree's weights, because a rebuild destroys the paned; positions re-apply from the tree on every `max-position` notify, since nested allocation reports intermediate values before settling.
- `realize_layout` rebuilds into whichever tabpage Neovim is current on, so the restore materialization is its only legal caller; it drops the queued copy, so a restored tab costs one rebuild. `request_layout_rebuild` defers, which is tab 0's route.
- Pointer input is one `gtk::GestureDrag`: a gesture keeps delivering after the pointer leaves the widget. Grid and button pin at press, so a wandering drag edits its starting pane.
- Each pane's status and `b:wolf_full_width` cache once per wake: reading at use time re-enters `draw_pane`'s guard, and paint, pointer and resize must agree inside one frame.
- One funnel produces every grid resize, compared against the size the pane last asked for rather than the lagging snapshot grid; it runs every wake, since a lifted content cap re-divides a still allocation.
- The drawer strip's height re-applies on every wake, so a toplevel resize converges over two frames; `reset_after_restart` clears the drawer, whose dead grid `detect_closes` can never reach after a restart.

#### `persistence.rs`

Purpose: the glue that gathers one tab's contribution to `state.json`, one instance per tab. It is thin by construction: every decision lives in [`state_seam.rs`](#state_seamrs) and [`pane_layout.rs`](#pane_layoutrs), so this module gathers live values and caches them.

**Public API**

- `build(parts) -> StateHandle` — a value constructor that installs no closures and gathers nothing of its own; `StateParts::restored_layout` is held for the seed, not for the rebuild.
- `set_empty(empty)`, `set_sidebar_shown(shown)`, `refresh_tab()`, `cached_tab() -> Option<TabState>`.

**Decisions**

- A refused gather leaves the cache untouched rather than clearing it, so a tab whose panes have not bound — or whose Neovim has just died — keeps what it last persisted instead of vanishing. Two refusals compose: [`state_seam.rs`](#state_seamrs)'s gate, and a `LayoutGather::Refuse` from the pane view.
- `cached_tab` derives the seed at read time rather than storing it, so a sidebar toggle or an Open Folder reaches a tab whose gate never opened, and the seed stops being consulted the instant a real gather succeeds.
- The persisted `sidebar_visible` is the user's preference, tracked through `set_sidebar_shown`, and never the live visibility.
- This module holds no store and submits nothing: [`window.rs`](#windowrs) assembles a window's tabs and [`coordinator.rs`](#coordinatorrs) commits the tree.

**Gotchas**

- `refresh_tab` runs inside the close handler, which goes on to take the core out, so every core borrow is taken and dropped in its own statement.

#### `picker.rs`

Purpose: the file-picker overlay — a search entry over a ranked results list. Activation sends `InputEvent::Open`, which also records the file in the MRU cache.

**Public API**

- `build(overlay, area, core) -> PickerHandle` — starts hidden. `area` is a `FocusTarget`, not the results host.
- `PickerHandle::open`, `close` (both idempotent), and `is_open`, which the app reads to suppress key forwarding.

**Decisions**

- The picker has no trigger predicate of its own: it opens from the `win.go-to-file` action; see [`actions.rs`](#actionsrs).
- The key controller runs in the capture phase, so Escape and the arrows are seen before the `SearchEntry` consumes them, and the arrows move the list selection while focus stays in the entry.
- The selection is preserved across a streaming rebuild unless the query changed, so an arriving batch does not yank the user back to the top row.
- Activation with zero rows is a true no-op: it neither opens anything nor closes the picker.

**Gotchas**

- Closing drops the `FileSearch`, which cancels the background traversal.
- `next_selection`, `restored_index` and `activation_index` are `pub(crate)` and shared with [`palette.rs`](#paletters), [`buffer_picker.rs`](#buffer_pickerrs) and [`search_overlay.rs`](#search_overlayrs), so all four overlays navigate and activate identically. They stay here because the macOS `PickerSeam` port names this file as its counterpart.
- `FocusTarget` is `pub(crate)` and shared the same way, by all four overlays and the sidebar; [DECISIONS.md](../DECISIONS.md#window-structure) owns why it is a closure rather than a captured widget.

#### `preview_link.rs`

Purpose: the pure, GTK-free URI rulebook for a link clicked inside the Markdown preview — what the click means, what base URI the document loads against, and the JavaScript that scrolls a fragment into view. A port of macOS's `PreviewLinkSeam`: this module owns URI arithmetic, [`preview_ui.rs`](#preview_uirs) owns liveness gates and load strategy.

**Public API**

- `classify(uri, source_path) -> PreviewLink` — total over the URI WebKit has already resolved against the base, so no relative-to-absolute arithmetic happens here. The variants are `OpenExternally`, `ScrollToFragment`, `PreviewFile` and `Refuse`.
- `file_uri_for(source_path) -> Option<String>` — the base URI, percent-encoded; `None` for an empty or relative path. Its exact form is load-bearing, and [DECISIONS.md](../DECISIONS.md#following-a-link-in-the-preview) states it.
- `scroll_into_view_js(fragment) -> Result<String, serde_json::Error>`.

**Decisions**

- Four shapes are refused where macOS calls `NSWorkspace.open`: a non-anchor `about:` URI, a `file:` URI with an empty path, a target with neither scheme nor path, and a `file:` authority other than empty or `localhost`.
- Whether a linked file is Markdown needs the filesystem, so this module never decides it: it always answers `PreviewFile` and core's own gate declines.

#### `preview_ui.rs`

Purpose: the pure, GTK-free rulebook for the Markdown preview pane — its placements, its gates, how a rendered document reaches the view, and what the pane's statusbar reads. [`preview_link.rs`](#preview_linkrs) owns a clicked link's URI arithmetic.

**Public API**

- The five `MODE_*` strings `runtime/nvim/init.lua` dispatches on, chosen by `preview_placement` and `edit_placement` — one rule at opposite polarities of `active_is_preview`.
- `preview_press`, `edit_press`, `may_render`, `preview_file_press` and `browser_press` — the gate on every blocking preview RPC, total over `Option<Liveness>` with no `_` arm.
- `browser_url(raw)` — `None` for an empty or all-whitespace address; the loopback-prefix check is core's contract, deliberately not duplicated.
- `document_update(current, loaded, fragment_pending)` — swap in place, or full load.
- `PreviewLoad`, `needs_reload(current, loaded)` and `preview_status(source_path, display_path)`.

**Decisions**

- A mode string renamed here compiles and does nothing there: the Lua binds the current window instead, replacing the pane the user was reading.
- Liveness is read before the buffer-kind gate, so a wedged Neovim is reported as wedged rather than as a non-Markdown buffer. `preview_file_press` has no buffer-kind gate at all: a linked file's Markdown-ness is core's, unanswerable here without the filesystem.
- Both `PreviewLoad` comparisons key on the source bufnr, never the `changedtick`: two freshly opened files share a tick, so a tick-keyed source switch would swap a linked file's body under the previous file's base URI.
- `preview_status`'s `abs_path` is the source path verbatim: [`pane_view.rs`](#pane_viewrs) persists it as a pane-layout leaf, and `state.json` records no working directory to resolve a relative one against.

#### `refresh_coalescer.rs`

Purpose: the pure, GTK-free state machine behind the sidebar's trailing refresh — a burst of streaming traversal batches collapsed into one rebuild per `REFRESH_INTERVAL`. A port of the macOS `PickerRefreshCoalescer`.

**Public API**

- `RefreshCoalescer::request() -> Option<u64>` — `Some(generation)` when the caller must arm a one-shot `glib` timeout carrying that generation, `None` when the request coalesced into an armed one.
- `fire(generation) -> bool` — whether that timeout performs its refresh. `cancel()` disarms.
- `REFRESH_INTERVAL` — the one site in this crate where the trailing delay lives.

**Decisions**

- `fire` clears the armed flag before answering, so a batch arriving during a refresh arms a fresh fire rather than coalescing into one that has already performed. That is what puts a traversal's final batch inside some later fire, and why the mechanism is a trailing timer rather than a frame budget, which would drop a wake with nothing armed.
- `cancel` bumps a generation token instead of removing the GLib source, because removing an already-fired source is a GLib warning; the queued closure still runs and performs nothing. The same pattern as [`crash_overlay.rs`](#crash_overlayrs)'s banner.
- **The picker deliberately does not share it**, unlike macOS, where one coalescer serves both the picker and the file tree. Widening the rule would put the two picker smokes at risk for nothing this crate needs.

**Gotchas**

- The interval's value is asserted nowhere — a constant compared against its own literal states nothing. What the tests pin is the mechanism.

#### `render.rs`

Purpose: the Cairo/Pango paint seam — one grid and its highlight table onto any Cairo context.

**Public API**

- `CellMetrics::for_font(cr, family, size_pt, line_height_percent)` — uses `cr` only for a Pango context, so the surface's size is irrelevant; the cell height and baseline come from [`cell_metrics_math`](#cell_metrics_mathrs).
- `paint(cr, grid, highlights, metrics)` — takes one grid rather than a whole `Snapshot`, because under `ext_multigrid` a front-end paints a single grid per surface.
- `draw_text_line(cr, font, x, y, text, fg)` — one line of free text with its top at `y`, for the statusbar strip.

**Decisions**

- `draw_glyph` moves to `y + baseline - ascent`, not the cell top: Pango puts the layout's top at the move-to point, so a taller cell would hang its glyphs from the top edge.
- The three decorations arrive as Pango attributes out of [`glyph_style.rs`](#glyph_stylers).
- The 96 dpi fallback in the point-to-pixel conversion is the live path on every headless run, not a guard: `context_get_resolution` returns `-1.0` over an `ImageSurface`.
- Every cell's background rectangle is filled, including empty ones, which keeps wide-character trailing placeholders and never-painted cells aligned.

**Gotchas**

- `FALLBACK_FG`, `FALLBACK_BG`, `draw_glyph` and `draw_text_line` are `pub(crate)` so `cursor.rs` and `pane_view.rs` reuse them; reimplementing one lets cursor colors, glyph placement or the strip's text drift from the grid paint.

#### `scrollbar.rs`

Purpose: a Neovim window's `win_viewport` projected onto a pane-local thumb rectangle and its colour. GTK-, Cairo- and Pango-free, so the whole rulebook runs in the fast suite; [`pane_view.rs`](#pane_viewrs) only fills the rectangle.

**Public API**

- `scrollbar_thumb(pane_w, grid, viewport) -> Option<ScrollbarThumb>` — `None` when the whole buffer fits, when the pane is too narrow for the gutter, or on degenerate input.
- `scrollbar_thumb_color(bg) -> Rgb`, over the editor's own background.
- The five constants those two read, `SCROLLBAR_THICKNESS_PX` through `SCROLLBAR_GREY`.

**Decisions**

- A scrollbar here is a projection and never a control, painted rather than a `gtk::Scrollbar` ([DECISIONS.md](../DECISIONS.md)).
- `x` is measured from the pane's right edge, not the grid rectangle's, so the thumb lies wholly inside the right padding gutter and covers no glyph at any pane width. macOS's `drawScrollbars` documents that rule and computes the other.
- It never enters `grid_size_for_pane`: a thumb consuming grid width would drop a column the instant a buffer overflowed its window and restore it when it fit, reflowing text on the scroll path.
- The height clamps to the track as well as to its own floor; macOS omits the upper clamp and draws a thumb taller than its track in a short pane.
- The colour uses [`statusbar_layout.rs`](#statusbar_layoutrs)'s `blend`, which is what keeps the thumb off an alpha-capable draw path.

**Gotchas**

- `Viewport::curline` is deliberately unread, matching macOS: a cursor move that scrolled nothing must not make the indicator jump.

#### `search_overlay.rs`

Purpose: the project-search overlay — a `Ctrl+Shift+F` entry over a flat, streaming list of ripgrep matches. Activating a row opens a location, not a file.

**Public API**

- `build(overlay, area, core) -> SearchHandle` — starts hidden; `area` is a [`picker.rs`](#pickerrs) `FocusTarget`.
- `SearchHandle::open`, `close` (both idempotent), `is_open`, and `set_project_open`, mirrored from the shell's empty-state flag.
- `open_decision`, `search_options_for`, `result_rows`, `activation_target`, `enter_action` and `search_status` — the pure seams.

**Decisions**

- Typing runs nothing: the first Enter runs the search, the second activates the selected row. An empty query is refused, since core would hand `rg` a pattern matching every line of every file.
- Closing and re-running both cancel: dropping the `SearchSession` kills the `rg` child; see [DECISIONS.md](../DECISIONS.md).
- `open_decision` refuses without a project, since the search root would otherwise fall back to the home directory.
- The options shipped are literal, case-insensitive and over all files, with no toggle buttons and no `WolfCore::ignorecase()` read, so macOS's `SearchToggleSeam` seeds have no counterpart.
- Rows are flat, one per matched line, which is what lets [`picker.rs`](#pickerrs)'s selection seams drive them unchanged; the order and the file labels come from core's projection.

**Gotchas**

- `all_files: true` reaches everything `rg --hidden` lists, `.git/` included; the measured counts are in [`macos-followups.md`](../../../macos-followups.md).
- A same-file jump to another line records no history, while a cross-file one does: core folds a `NavTarget` per window, and `NavTarget::Edit` carries no line number.

#### `shell.rs`

Purpose: the glue for the action namespace — `gio` actions, the primary menu, the chord controller, the empty state, the command drain, and crash-recovery orchestration.

**Public API**

- `build_menu_model()`, `build_empty_state()`, `build(parts) -> AppShell`; `ShellParts` carries a `Weak` coordinator and this window's id.
- `install_actions(app)` — the `win.` rows only, after the window is constructed and before `present()`, so the menu and header button resolve their actions.
- `set_empty_state(empty)`, `on_liveness_wake() -> bool`, `on_render_wake()`.

**Decisions**

- The chord controller matches keys itself: GTK's keymap-aware matching does not fire under the smokes' keystroke injector. Accelerators are registered only so the menu renders their labels, `app.` rows included ([`coordinator.rs`](#coordinatorrs)).
- The controller lives on the window, so it runs ahead of any overlay's and needs no open-overlay guard: no overlay's keys are bound chords.
- One settle point moves the sidebar's root, over [`workspace.rs`](#workspacers)'s `reroot_decision`, so `chdir_to` and a typed `:cd` cannot diverge. Neither is synchronous: the live cwd updates only after Neovim reports it.
- The nav-button refresh caches nothing: `set_sensitive` already early-returns on an unchanged value.
- The three Markdown-preview presses read one `preview_shape` pass each: two snapshot reads would describe no window that existed. Each is a blocking RPC gated through [`preview_ui.rs`](#preview_uirs).
- `sidebar.set_visible` has one caller, over [`state_seam.rs`](#state_seamrs)'s `sidebar_is_shown`, or `set_empty_state(false)` would undo a user toggle.
- The per-tab fan-outs go through [`tab_view.rs`](#tab_viewrs): the empty state, the sidebar preference — which also reaches the tabs a restore has queued — the focus return and the restart collapse. New Tab is gated by [`tabs_ui.rs`](#tabs_uirs)'s `new_tab_press`; [DECISIONS.md](../DECISIONS.md) owns the boundary.

#### `state_seam.rs`

Purpose: the pure application-state seam — what to restore at launch, and what each tab persists per wake. GTK-free, IO-free, every input injected. [DECISIONS.md](../DECISIONS.md#application-state) owns the rules.

**Public API**

- `restore_plans(saved, process_cwd, home, is_dir) -> Vec<RestorePlan>` — one plan per saved window in saved order, capped at `MAX_RESTORE_WINDOWS`, each carrying one `TabPlan` per saved tab, capped at `MAX_RESTORE_TABS` and never empty.
- `tab_subtree`, `window_subtree`, `seeded_tab_state`, `pending_tab_states`, `ordered_tab_states`, `window_contribution` — the write side, one tab's `TabState` up to a window's whole contribution.
- `snapshot_state(windows) -> Option<WolfState>`, `close_commit(shutting_down, other_live_windows) -> CloseCommit`.
- `restored_window_size`, `persistable_layout`, `sidebar_is_shown`, `snapshot_cwd`, `may_snapshot`.

**Decisions**

- A saved layout attaches only to the directory it saved from, and the process cwd is offered to the first saved window alone: offering it further merges every window onto one project and drops every layout.
- Four answers mean "commit nothing" rather than "commit an empty thing": `seeded_tab_state` with no layout, `window_subtree` with no tabs, `snapshot_state` with no windows, and `restored_window_size` on any unusable dimension. `restore_plans` is the inverse and never answers an empty vec.
- `window_contribution` puts live tabs' caches before queued tabs' seeds, which is the order they materialize in and restore replays. `ordered_tab_states` stably sorts the live half into page order first, because the user can rearrange the bar.
- The empty state persists an empty cwd always, or every launcher start would become a project at home.
- `sidebar_visible` is per window and reaches the seeds alone; a live tab's cache passes through verbatim.

#### `statusbar_layout.rs`

Purpose: the per-pane statusbar's rulebook, decidable from values. The vertical half ports `WolfKeyMap/StatusbarLayout.swift` constant for constant; the divergences are in [`macos-followups.md`](../../../macos-followups.md).

**Public API**

- `statusbar_layout(primary_font_size, secondary_font_size, vertical_padding, line_gap) -> StatusbarLayout`.
- `secondary_font_size(primary_font_size) -> f64`, and `statusbar_text(fg, bg, active) -> StatusbarText` over the editor's own default foreground and background.
- The seven named constants those two read, `STATUSBAR_LINE_FACTOR` through `STATUSBAR_INACTIVE_ALPHA`.

**Decisions**

- Unit-agnostic ([`cell_metrics_math.rs`](#cell_metrics_mathrs)); [`pane_view.rs`](#pane_viewrs) calls it in device pixels, from `CellMetrics::size_px`.
- Both colours blend toward the background rather than toward black, which is what compositing over the already-filled band gives, so the strip needs no alpha-capable draw path.
- A degenerate input collapses the whole layout to zeros and a non-finite font size falls to the floor, where Swift guards neither: an infinite strip height leaves an editor painting one cell.

**Gotchas**

- `blend` is `pub(crate)` for a second consumer, [`scrollbar.rs`](#scrollbarrs)'s thumb colour. Reimplementing it there would give two colours on one scale two rounding rules, which is how they drift apart.

#### `tab_view.rs`

Purpose: the GTK glue for native tabs — an `adw::TabBar` over an `adw::TabView`, one [`pane_view.rs`](#pane_viewrs) and one [`persistence.rs`](#persistencers) handle per tab. Every decision it acts on is [`tabs_ui.rs`](#tabs_uirs)'s.

**Public API**

- `build(TabsParts) -> TabsHandle` — no page exists until `open_tab`.
- `TabsHandle::open_tab(TabParts) -> bool`, `view_widget`, `bar_widget`, `selected`, `entries`, `len`, `on_render_wake`, `reset_after_restart`, `focus_target`, `connect_selected`, plus `TabHandle`'s per-tab relays.
- `queue_restore(RestoreTabs)`, `pending_restore()` and `set_pending_sidebar_shown(shown)` — the tabs a launch still owes a page, which [`window.rs`](#windowrs) seeds.

**Decisions**

- `TabParts::tabpage` is `0` for tab 0 and `create_tabpage`'s answer for every later one: `Snapshot::current_tabpage` lags, so a page left to adopt binds to the tab it was opened from.
- The restore materialization realizes each queued tab's layout before leaving its tabpage, because `rebuild_layout` acts on whichever tabpage is current. An abort retains the layouts it never reached as a seed source with creation disabled, so those tabs keep their place in `state.json`.
- `close-page` is a veto: every arm must answer `close_page_finish` and return `Propagation::Stop`, or the page stays on screen forever with no error. Closing the last page closes the window ([DECISIONS.md](../DECISIONS.md)). Both handlers hold a `Weak`; a strong reference from a handler the view owns leaks every tab's pane view.

**Gotchas**

- `selected` answers the focus, overlay and empty state; persistence reads every tab, through `entries`.
- `entries` is insertion order, not page order — nothing reconciles it with the `adw::TabView`. The one caller that needs the user's arrangement pairs each entry with `TabHandle::page_position`.

#### `tabs_ui.rs`

Purpose: the pure, GTK-free rulebook for the N tabs one window's captive Neovim serves: which tab a render wake may touch, whether a New Tab press or a queued restore may issue its blocking RPC, what closing one does, and what a new tab inherits. [`tab_view.rs`](#tab_viewrs) carries the answers out.

**Public API**

- `tab_wake(current_tabpage, my_tabpage, selected) -> TabWake` — `Adopt(handle)`, `Refresh` or `Hold`, total over all three inputs.
- `restore_press(first_tabpage, liveness, pending, may_create) -> RestorePress` — `Create`, `Wait` or `Idle`, total.
- `switch_target`, `close_outcome`, `new_tab_press`, `new_tab_cwd`, `tab_title` and `TAB_TITLE_FALLBACK`.

**Decisions**

- `Hold` does nothing at all, and that is the whole gate: what it protects is a background tab's statusbar text, title and grid resize, each of which reads `pane_status`. Persistence is safe by another route, [`pane_layout.rs`](#pane_layoutrs)'s window-keyed leaf resolution.
- The authority is Neovim, not GTK's selection: a tab Neovim considers current refreshes while the widget lags, and a selected one holds until Neovim catches up.
- `restore_press` refuses until tab 0 has adopted and the core reads `Alive`. A refusal answers `Wait`, not `Idle`: a queued tab has no page and no cache, so declaring the queue empty is tabs that never open. `Idle` is the abandoned queue.
- `switch_target` and `close_outcome` refuse the two inputs that fail silently: Neovim reads a handle of `0` as "the current tabpage", and `close_tabpage` on the last tabpage does nothing.
- `new_tab_cwd` answers `None` in the empty state always, so a new tab is never `:tcd`'d into `$HOME`; it must agree with [`state_seam.rs`](#state_seamrs)'s `snapshot_cwd`.

#### `title.rs`

Purpose: the pure window-title seam — the working directory, rendered, with the home prefix abbreviated. A port of the macOS title seam, so the two front-ends abbreviate identically.

**Public API**

- `display_title(cwd, home) -> String` and `window_title(cwd, home) -> String`, which substitutes `WINDOW_TITLE_FALLBACK` for an empty result.

**Decisions**

- `home` is always injected, never read from the environment, which is what makes the rule decidable from its arguments. The single environment read in this crate lives in [`workspace.rs`](#workspacers), and the shell captures it once at build time.
- Abbreviation matches on a full path-segment boundary only. A directory sharing a character prefix with home is not under it and must be returned verbatim.
- A home of `/` disables abbreviation entirely, because abbreviating every absolute path would be actively misleading.
- The window builder seeds the same fallback constant as the initial title, so the title never flickers between two spellings before the first directory is reported.

#### `tree.rs`

Purpose: a re-export shim over `wolf_core::tree`. The hierarchical projection of the flat file list and its tests live in the core as one source of truth shared by every front-end; this module keeps the `crate::tree::…` paths resolving for the GTK glue.

#### `tree_gate.rs`

Purpose: the pure, GTK-free gate every sidebar refresh passes — visibility first, snapshot version second. A port of the macOS `TreeRefreshGate`.

**Public API**

- `TreeRefreshGate::new(visible)`, `request_refresh() -> bool`, `set_visible(visible) -> bool`, `should_rebuild(version) -> bool`, `reset_version_tracking()`.
- A refresh requested while hidden is refused and recorded as owed; `set_visible` answers `true` for the one catch-up that owes, and `false` on every other transition.

**Decisions**

- The version half is what stops a current-file change collapsing manual expansion: rebuilding the root store destroys each row's `TreeListRow` along with its expanded state, and a current-file change advances no version.
- `reset_version_tracking` exists because `wolf_core::files::FileSearchState` numbers versions per `FileSearch` rather than globally. A search started by a re-root begins again at 1, so a gate that kept its tracking would answer "no rebuild" to the new project's first batch and leave the departed project's files on screen.

**Gotchas**

- Hiding the sidebar destroys no store, so a catch-up at an unchanged version correctly rebuilds nothing. The deferral is for a version that advanced while nobody could see it.

#### `tree_view.rs`

Purpose: the GTK glue for the file-tree sidebar — a `gtk::ListView` over a `gtk::TreeListModel`.

**Public API**

- `build(core, area, start, root_label) -> TreeHandle` — `area` is a [`picker.rs`](#pickerrs) `FocusTarget`; `start` is false in the empty state, where nothing is walked.
- `widget()`, `on_render_wake()`, `close()`, `set_sidebar_visible(shown)` and `reroot(root_label)`, the last two relayed from [`shell.rs`](#shellrs)'s one visibility site and one settle point; this module reads no cwd.

**Decisions**

- The root store holds one item: a synthetic `Dir` named after the working directory whose rel-path is the empty cwd sentinel, so the create-func needs no special case. `remove_all` destroys its expanded state, so it is re-expanded after every rebuild and before a reveal walks ancestors.
- Children materialize lazily through `wolf_core::tree::TreeIndexCache` under rule R1, on `children_for`: the memo and the entries are borrowed inside one statement touching no widget, because GTK re-enters the create-func and a borrow held across `set_expanded` or `set_list_row` is a panic.
- Only a traversal batch is coalesced ([`refresh_coalescer.rs`](#refresh_coalescerrs)); every other refresh is immediate, and all pass both gates in [`tree_gate.rs`](#tree_gaters). A pending reveal retries even when the rebuild is skipped.
- The current-file bold moves through a registry of bound labels, which is what the version gate costs: a suppressed rebuild re-binds no row.

**Gotchas**

- Manual expansion below the root does not survive a root-store rebuild; [DECISIONS.md](../DECISIONS.md) owns that limitation and what preserving it would cost.

#### `window.rs`

Purpose: one window and everything scoped to it — an `adw::ApplicationWindow` over a live `WolfCore`, with its own wake channel, tab layer, overlays and state subtree. Its widget structure and overlay stacking are [DECISIONS.md](../DECISIONS.md#window-structure)'s.

**Public API**

- `build(parts) -> Option<WindowHandle>` — `None` when the core fails to start; a launch whose only window fails exits.
- `WindowParts` — the construction inputs as a struct, under `clippy::too_many_arguments`.
- `WindowHandle::id`, `present`, `refresh_subtree`, `cached_subtree` and `display_cwd`.

**Decisions**

- Telling the coordinator is the close handler's first statement, while the core and panes are still alive: a subtree gathered after the core is taken out sees nothing, the gate refuses it, and the session is silently lost.
- The window registers with the coordinator before its first command drain, or the drain reaches a coordinator that does not know the window exists.
- `refresh_subtree` and `cached_subtree` answer for every tab; `cached_subtree` pairs each live cache with its page position and appends the queued tabs' seeds, over [`state_seam.rs`](#state_seamrs)'s ordering rules. Both keep their one-window signatures, so [`coordinator.rs`](#coordinatorrs) never learns tabs exist.
- `display_cwd` reads the cached subtree, so the palette and `state.json` cannot disagree about where a window is.
- `WindowInner` does not hold the shell: the wake-drain future owns it for exactly the window's lifetime, and a second owner adds a lifetime nothing reads.

**Gotchas**

- The key controller carries no propagation phase, so it runs after [`shell.rs`](#shellrs)'s capture-phase chord controller, which stops a bound chord also reaching Neovim.

#### `workspace.rs`

Purpose: the pure workspace decisions — is a project open, where does Neovim start, and where is the sidebar rooted.

**Public API**

- `launch_decision(process_cwd, restored, home, is_dir) -> LaunchDecision` — `Folder`, or `EmptyState` carrying the directory Neovim starts in.
- `reroot_decision(pending, live, rooted) -> RerootAction` — stay, discard a satisfied request, or re-root where Neovim is; `root_label(cwd)` is the root row's text.
- `reroot_ready(pending, live) -> bool` and `home_dir()`, the one environment read here, which the shell captures once at build time.

**Decisions**

- Two candidates are offered and the first to satisfy one shared predicate wins: a directory, not home, not the root. The process cwd comes first, because a terminal launch from a project is the Linux idiom; the restored cwd takes its turn when a launcher start yields home or the root. The same predicate on the restored cwd stops a hand-edited state file aiming the file search at the whole filesystem.
- A re-root's authority is `rooted`, where the sidebar sits, against the cwd Neovim reports; a request decides only whether its own flag is discarded. So a typed `:cd` moves the sidebar with no request, and one `chdir` is exactly one re-root.
- No project predicate guards a re-root: a typed `:cd /` is an explicit act, not the launch-time guess `is_usable_project` judges, with core's search bounds as the backstop.
- Comparisons are plain equality, so callers canonicalize both sides: a trailing separator is ignored, `..` and symlinks are not.
