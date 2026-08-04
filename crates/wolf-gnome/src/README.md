# wolf-gnome/src

The GTK front-end's modules, one entry each. The pure seams here are the Linux counterparts of the ones in [`wolf-macos/Sources/WolfKeyMap/`](../../wolf-macos/Sources/WolfKeyMap/README.md), and the two are kept deliberately close.

**Read [DECISIONS.md](../DECISIONS.md) first** for the gtk-rs version constraints, the threading and wake model, the window structure and focus regime, the accelerator invariant, the application-state rules, the crash-recovery model, and why this crate has no Bazel target.

## Files

#### `about.rs`

Purpose: the About dialog — a pure text seam over the bundled fonts' licence notices, plus the transient window that shows it. This is the one surface on this front-end that discharges the fonts' attribution obligation, so its content has legal weight rather than cosmetic weight.

**Public API**

- `about_text() -> String` — the whole dialog body. It takes no arguments and reads `wolf_core::FONT_LICENSE_NOTICES` itself, deliberately: a version taking the notices as a parameter would let its test pass them in, which proves nothing about what ships.
- `present(parent)` — a modal `gtk::Window` transient for the main window, titled `ABOUT_TITLE`.

**Decisions**

- Plain GTK over `adw::AboutDialog` or `adw::AboutWindow`, which need the `v1_5` and `v1_2` `libadwaita` features. [DECISIONS.md](../DECISIONS.md) records that this crate's `libadwaita` carries no version feature on purpose, so adding one for a single dialog would change the crate's build contract and put the whole smoke suite on the devShell's libadwaita version.
- A separate toplevel rather than a fifth in-window overlay, so the main window's focus widget is untouched and the focus regime — which is about widgets inside the main window being destroyed — does not apply.

#### `actions.rs`

Purpose: the action namespace as data — every `gio` action name, its accelerator, the `wolf_core::Command` behind it, its binding status, and its menu placement — plus the chord matcher and the invariant checker. It names no widget type, so it is tested with no display.

**Public API**

- `ACTION_TABLE` and `MENU_SECTIONS` — the one table the menu, the accelerators and the command dispatch all read; see [DECISIONS.md](../DECISIONS.md).
- `BindingStatus` — `Bound` is registered and matched; `Reserved` is declared for a later unit and is neither; `MenuOnly`, which About uses, carries no chord.
- `action_for_key(keyval, state)` — the matcher the chord controller calls. It scans `Bound` rows only: matching a reserved row would swallow the key and do nothing.
- `parse_accel` and `forbidden_reason` — the invariant checker, run over every row by this module's tests.

**Decisions**

- `parse_accel` re-implements the accelerator grammar this crate allows rather than calling `gtk::accelerator_parse`, which asserts an initialized main thread and cannot run headlessly. The grammar admits only `<Control>`, `<Shift>`, and `<Alt>` with a key name, so `<Super>` and `<Primary>` never reach `forbidden_reason`.
- `action_for_key` compares its three modifiers exactly and ignores every other bit: intercepting a Super-contaminated chord beats forwarding one Neovim mangles. Letters compare case-insensitively, since a shifted letter's keyval is keymap-dependent.

**Gotchas**

- Whether an action is installed follows from its command's classification, not from `status`: a command-less row installs through `command_action::ui_action_for_front_end_action` instead. Promoting a reserved row means flipping `status` too, or the chord stays inert.

#### `app.rs`

Purpose: all GTK and Adwaita window wiring. `run` builds the `adw::Application` and, on activate, constructs the window over a freshly started `WolfCore`; the window's structure is in [DECISIONS.md](../DECISIONS.md).

**Public API**

- `run() -> glib::ExitCode` — hands GTK an empty argv, so the process command line is never parsed or rejected.

**Decisions**

- The state store is opened into a restore plan and the user config loaded before any widget exists, so the launch decisions are known while the window is built. Each degrades with one message: a no-op state store, or the built-in config defaults.
- Cell metrics are measured once and shared by `Rc`, so a resize arriving before the first draw already has a size.
- The close-request handler flushes state before taking the core out, and takes it out in its own statement so the borrow is released before `shutdown` blocks. A flush after the take sees no core, is refused by the snapshot gate, and silently loses the session.
- No title widget is set on the header bar, measured rather than stylistic: its height is one term of the pane origin every pointer smoke derives its targets from ([DECISIONS.md](../DECISIONS.md)).

**Gotchas**

- The wake-channel capacity stays private; its tests live here and read the production constant, not a literal.

#### `buffer_picker.rs`

Purpose: the buffer-picker overlay — a search entry over the captive Neovim's listed buffers, ranked by `wolf_core::rank_buffers`. Activating a row switches the active window to that buffer.

**Public API**

- `build(overlay, area, core) -> BufferPickerHandle` — starts hidden; `area` is the focus-return widget.
- `BufferPickerHandle::open`, `close` (both idempotent), and `is_open`.
- `open_decision`, `initial_selection`, `switch_target` and `row_label` — the pure seams.

**Decisions**

- Candidates are snapshotted once at open and re-ranked per keystroke. The snapshot is a blocking RPC on the GTK main thread, so [`picker.rs`](#pickerrs)'s wake bridge must not be added for symmetry: re-taking it puts a whole timeout budget of frozen main loop on the typing path.
- `open_decision` refuses to open, and so refuses the RPC, for every liveness reading but `Alive`. It cannot prevent the first stall, and must not be strengthened into a guard that can never raise one.
- A refusal returns before any widget is touched, and nothing is shown until the snapshot returns; neither is observable to any test.
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
- `unsupported_settings(config) -> Vec<UnsupportedSetting>` — one row per configured setting this front-end reads and does not honour, each carrying the key, the value and the reason.

**Decisions**

- The override is injected rather than read here, which is what makes the precedence rule testable without touching the process environment — something a parallel test suite cannot safely do.
- A forced `appearance` produces a row; `macos_option_as_alt` produces none, however it is set. The asymmetry is deliberate: one config file is shared across both platforms and that key's own name says which platform it is for, so a warning would fire on every Linux launch for a setting that is correct where its author wrote it. A named test pins it.

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

#### `editor_inset.rs`

Purpose: the chrome around a pane's text grid as arithmetic — the rectangle the grid paints into, the map into the space [`cell_at`](#geometryrs) speaks, the content-width cap with its centring, and the grid size left over. A port of `WolfKeyMap/EditorInset.swift`.

**Public API**

- `EDITOR_PADDING_PX`, the inset on all four sides, and `MAX_GRID_COLS`, the cap in columns.
- `ColumnCap { max_cols: Option<u16>, cell_w }`, `ColumnCap::uncapped(cell_w)`, `apply(cols) -> u16`. The cell width rides inside the cap, so one call cannot be given two of them.
- `column_cap(full_width, cell_w) -> ColumnCap` — where a window's `b:wolf_full_width` becomes geometry.
- `grid_rect(pane_w, pane_h, statusbar_h, padding, cap) -> GridRect`, `grid_local(x, y, rect) -> (f64, f64)`.
- `grid_size_for_pane(pane_w, pane_h, statusbar_h, padding, cell_h, cap) -> (u16, u16)`.
- `min_pane_size(cell_w, cell_h, statusbar_h, padding) -> (f64, f64)`, a pane's content floor.

**Decisions**

- The subtraction and the division stay separate: [`geometry.rs`](#geometryrs) states its composition guarantee over the division alone, and `resize_global` wants that un-inset division for grid 1.
- Only `w` and `h` clamp at zero, so a pane smaller than its chrome still reports where the grid would start. A non-finite input collapses the whole rectangle, unlike Swift: a NaN origin would poison every pointer event in the pane.
- The cap is a `min` on the width here and a `min` on the column count in `grid_size_for_pane`, the second taken over an uncapped rectangle: flooring a capped width to whole pixels first costs the last column at any non-integral cell width. A property test holds the two together.
- The centring offset is floored to whole device pixels, unlike macOS: a half-pixel `cr.translate` resamples every glyph and this crate has no HiDPI handling, at the cost of a left margin up to a pixel wider than the right.
- No `StatusbarTextSpan` counterpart and no `text_left_inset`: one rectangle carries the centring offset in its `x`, and the paint, the clip, the strip's text and both pointer paths all read that field, so strip and grid share an origin by construction. macOS needs the seam only because it splits the inset from the cap across two structs.

**Gotchas**

- A point in the strip or a gutter comes back negative, and nothing is clamped or collapsed here, so `cell_at`'s edge and non-finite rules apply unchanged.

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
- `divider_position` scales by the paned's own span, with `min_position`/`max_position` as the clamp rather than the scale, because a weight is a proportion of width ([DECISIONS.md](../DECISIONS.md)). The two must share that denominator, and which one it is the inverse property does not constrain: a pair sharing the wrong one round-trips perfectly while a fresh split comes out lopsided by the end child's minimum, which is what `equal_weights_give_equal_widths` catches.

**Gotchas**

- `cell_at` takes grid-local coordinates; the per-pane chrome is [`editor_inset.rs`](#editor_insetrs)'s. The content cap's centring offset lives in that module's `grid_rect(..).x` and `grid_local` subtracts it, so this function still takes no origin and macOS's `paintOffset` machinery has no counterpart. Do not add one.

#### `input.rs`

Purpose: translates a GDK keyval plus modifier state into a `wolf_core::KeyEvent`. It has the exact shape of a `gtk::EventControllerKey` callback but stays a free function so it is testable without a display server.

**Public API**

- `gdk_key(keyval, state) -> Option<KeyEvent>` — `None` for bare modifier and lock keys, and for any keyval whose unicode value is absent or a control character.

**Decisions**

- Named keys resolve before the printable fallback, because their `to_unicode()` is often a control character (`Return` is `\r`) and would otherwise be swallowed by the control-character rejection.
- For printable characters `shift` is always false: GDK already delivers the shifted glyph in the keyval, and `KeyEvent` ignores `shift` for `Key::Char`. Named keys pass the real shift through, which is what keeps `<S-Tab>`, `<S-Left>`, and `<S-F5>` working.
- `ISO_Left_Tab` is what Shift+Tab actually reports on X11, so it maps to `Key::Tab` with `shift` forced true even when the platform did not also set the shift mask.

**Gotchas**

- `SUPER_MASK` maps to `meta`, so an unmapped Super chord is forwarded and Neovim mangles it. The chord controller intercepts the ones this crate binds; the rest is a known limitation.

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

Purpose: crate root. Declares the modules and re-exports the display-free seams a consumer reaches for: the key and geometry conversions, the paint, the command classification, the crash rulebook, the launch and restore plans, and the window title.

**Gotchas**

- `cell_at` is re-exported because `grid_size_for` is, and the two are a pair whose composition is the point.
- `restart_button_rect` is re-exported for one concrete consumer: the crash-overlay smoke derives the pixel it clicks from the same function the widget builder realizes, so the two cannot drift.
- Items in `about`, `actions`, `buffer_picker`, `cell_metrics_math`, `config_seam`, `crash_overlay`, `cursor`, `editor_inset`, `layout`, `mouse`, `nav_ui`, `palette`, `pane_layout`, `pane_view`, `persistence`, `picker`, `refresh_coalescer`, `scrollbar`, `search_overlay`, `shell`, `statusbar_layout`, `tree`, `tree_gate`, and `tree_view` are reached by module path rather than re-exported.

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

Purpose: the binary entry point. `main` forwards the exit code from `wolf_gnome::app::run`; all wiring lives in `app.rs`.

#### `mouse.rs`

Purpose: the GDK pointer-translation rulebook, the analogue of [`input.rs`](#inputrs). GDK value types in, `wolf_core` input values out, with no widget and no event loop.

**Public API**

- `mouse_input(button, state)` and `scroll_threshold(unit, cell_h)` — the button plus the modifier bools Neovim's mouse grammar carries, and the delta that makes one line step.
- `ScrollAccumulator::consume`, `PaneScroll::steps_for(grid, delta, threshold)`, and `scroll_steps(steps)`.

**Decisions**

- Only the primary button is forwarded; every other number yields `None`. That is parity with macOS, and the one place the decision is recorded and tested.
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

Purpose: the command-palette overlay — the registry commands this front-end performs, plus one Open Recent row per usable recent directory.

**Public API**

- `build(overlay, area, core) -> PaletteHandle` — starts hidden; `area` is the focus-return widget.
- `PaletteHandle::open`, `close` (both idempotent), `is_open`, and `connect_activate`.
- `open_recent_candidates`, `palette_rows` and `palette_action_for` — the pure seams.

**Decisions**

- An Open Recent row carries a directory and no window target, and [`shell.rs`](#shellrs)'s `on_palette_activation` is the one place that directory becomes a window action.
- `open_recent_candidates` refuses the window's own working directory, because this front-end has one window and re-rooting it where it already is changes nothing; a relative path, which `chdir_to` would canonicalize against the application process's working directory rather than Neovim's; and a path `is_dir` rejects, because a failed `:tcd` is only a `tracing::warn!` in core, so Neovim reports no directory change and the row does nothing at all. The shared store is never pruned, so a temporarily unmounted path returns.
- The label is `PaletteRow::display` unchanged, so the string ranked against the query and the string on screen are one. [`title.rs`](#titlers)'s home abbreviation is not applied here: it would rank one string and show another, and a query matching the visible `~` would return nothing.
- Ranking runs unbounded and `max` applies after the classification filter, since `rank_palette` truncates first and undispatched rows would otherwise eat the budget.
- Activation records, closes, then dispatches: a dispatched Close Window tears the window down synchronously.

#### `pane_layout.rs`

Purpose: the pure codec between the live pane tree, whose leaves are pane ids, and the persisted `wolf_core::PaneNode`, whose leaves are buffer paths. No GTK, no serde, no IO.

**Public API**

- `encode(layout, paths) -> PaneNode` — an id missing from `paths` becomes an empty leaf.
- `decode(node, first_leaf_id) -> Option<(PaneTree, Vec<PaneId>)>` — ids allocated sequentially in depth-first order, so leaf `i` is the pane for handle `i`.
- `is_empty_leaf(node)`, `leaf_count(node)`, `leaf_paths_dfs(node)`.
- `MAX_RESTORE_LEAVES` and `MAX_RESTORE_DEPTH`.

**Decisions**

- `is_empty_leaf` is the content predicate the save path gates on: true only for an unnamed, non-preview lone leaf. `leaf_count` exists solely to validate the handle count a rebuild returns, and gating persistence on it reintroduces the macOS bug described in [DECISIONS.md](../DECISIONS.md). A named test states that rule on its own so a regression names it.
- `decode` validates before anything is rendered, so Neovim is never asked to realize a malformed tree. The two caps are restore policy against a hand-edited file, not a limit on the tree type.
- The orientation mapping lives here in both directions, so an inversion is one edit to make and one test to catch rather than two half-inversions that cancel.
- `preview` is always written false. The flag is already carried end to end by the schema and the rebuild spec, so a preview pane adds a value here, not a field.

#### `pane_view.rs`

Purpose: the GTK glue that turns `layout`'s pane tree into nested `gtk::Paned`, one `gtk::DrawingArea` per Neovim window, plus the bottom command-line strip.

**Public API**

- `build(core, metrics) -> PaneHandle` — no panes exist until the first window grid appears on a wake.
- `cmdline_widget()` — the strip the overlays and the sidebar return focus to.
- `reset_after_restart()`, `request_layout_rebuild(saved)`, `layout_snapshot()`, `snapshot_readiness()`.

**Decisions**

- Panes paint window grids only; grid 1 empties under `ext_messages`, so the strip paints from the folded `Snapshot::cmdline` and `Snapshot::toast`.
- Divider positions re-apply from the live tree on every `max-position` notify, because nested allocation reports intermediate values before settling. A `notify::position` whose maximum and span both held still is a user drag, written back through `set_divider_fraction`; rebuilding there would destroy the paned holding the live gesture.
- Every pixel-to-grid conversion here routes through [`editor_inset.rs`](#editor_insetrs), because the strip and padding are painted into the pane's own area ([DECISIONS.md](../DECISIONS.md)).
- Pointer input is one `gtk::GestureDrag`, not a click controller: a gesture takes an implicit grab and keeps delivering after the pointer leaves the widget. The target grid and button are pinned at press while modifiers are re-read live, so a wandering drag still edits the pane it started in.
- Each pane's status and its window's `b:wolf_full_width` are read once per wake and cached, because `draw_pane` holds a snapshot read guard while both core calls take one internally: a re-entrant read can deadlock. Caching the flag is also what keeps the paint, the pointer translation and the resize request agreeing inside one frame. `layout_snapshot` reads the raw absolute path from that cache, not the relativized display path.
- One funnel produces every grid resize, comparing against the size the pane last asked for rather than against the snapshot's live grid, which lags a fire-and-forget request. It runs on each wake as well as from the resize signal, because lifting the content cap changes a pane's column count while its allocation holds still, so no resize signal fires for it.
- The scrollbar thumb is painted after the grid clip is dropped, since it lives in the right padding gutter; its rectangle and colour are [`scrollbar.rs`](#scrollbarrs)'s. A pane with no bound grid draws none, because `Snapshot::windows` is keyed by grid id and no viewport belongs to it.

**Gotchas**

- A pending saved layout is dropped rather than applied once a second pane exists: splits made before the restore leave it describing a different tab.

#### `persistence.rs`

Purpose: the glue that commits this crate's application state to `state.json`. It is thin by construction: every decision lives in [`state_seam.rs`](#state_seamrs) and [`pane_layout.rs`](#pane_layoutrs), so this module gathers live values and hands them over.

**Public API**

- `build(parts) -> StateHandle` — a value constructor that installs no closures and takes no snapshot of its own.
- `set_empty(empty)`, `set_sidebar_shown(shown)`, `on_render_wake()`, `flush_on_close()`.

**Decisions**

- An absent state store is the whole degradation story: every method early-returns, and the app runs normally while neither restoring nor saving. There is no error path and no second convention.
- The size comes from the window's default size and the position is never written, per [DECISIONS.md](../DECISIONS.md). The persisted `sidebar_visible` is the user's preference, tracked through `set_sidebar_shown`, and never the live visibility.
- `flush_on_close` blocks on a bounded flush rather than trusting the writer thread, and a timeout prints rather than panicking. It exists to bound the quit path against a wedged filesystem, not to be reached.

**Gotchas**

- Borrow discipline is load-bearing here specifically. `flush_on_close` runs inside the close-request handler, which then takes the core out, so every core borrow in this module is taken and dropped in its own statement.
- This module has no unit tests and needs none: it holds no rules. Its coverage is the relaunch smokes plus the seam tables.

#### `picker.rs`

Purpose: the file-picker overlay — a search entry over a ranked results list. Activation sends `InputEvent::Open`, which also records the file in the MRU cache.

**Public API**

- `build(overlay, area, core) -> PickerHandle` — starts hidden. `area` is the focus-return widget, not the results host.
- `PickerHandle::open`, `close` (both idempotent), and `is_open`, which the app reads to suppress key forwarding.

**Decisions**

- The picker has no trigger predicate of its own: it opens from the `win.go-to-file` action; see [`actions.rs`](#actionsrs).
- The key controller runs in the capture phase, so Escape and the arrows are seen before the `SearchEntry` consumes them, and the arrows move the list selection while focus stays in the entry.
- The selection is preserved across a streaming rebuild unless the query changed, so an arriving batch does not yank the user back to the top row.
- Activation with zero rows is a true no-op: it neither opens anything nor closes the picker.
- No shared overlay widget is extracted for the four overlays, and that is a judgement: a common type would need four configuration hooks, and the widget layer's only coverage is the cage smokes a refactor would risk.

**Gotchas**

- Closing drops the `FileSearch`, which cancels the background traversal.
- `next_selection`, `restored_index` and `activation_index` are `pub(crate)` and shared with [`palette.rs`](#paletters), [`buffer_picker.rs`](#buffer_pickerrs) and [`search_overlay.rs`](#search_overlayrs), so all four overlays navigate and activate identically. They stay here because the macOS `PickerSeam` port names this file as its counterpart.

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

Purpose: the Cairo/Pango paint seam — one grid and its highlight table onto any Cairo context, offscreen in tests and a drawing area in production.

**Public API**

- `CellMetrics::for_font(cr, family, size_pt, line_height_percent)` — uses `cr` only for a Pango context, so the surface's size is irrelevant. The cell height and baseline come from [`cell_metrics_math`](#cell_metrics_mathrs); the measured `ascent`/`descent` and `size_px`, the font size in device pixels, ride along on the result.
- `paint(cr, grid, highlights, metrics)` — takes one grid rather than a whole `Snapshot`, because under `ext_multigrid` a front-end paints a single grid per surface.
- `draw_text_line(cr, font, x, y, text, fg)` — one line of free text with its top at `y`, for the statusbar strip; weight and size ride on the caller's font description.

**Decisions**

- `draw_glyph` moves to `y + baseline - ascent`, not the cell top: Pango puts the layout's top at the move-to point, so a cell taller than the glyph box would hang the glyphs from its top edge.
- The 96 dpi fallback behind the point-to-pixel conversion is the live path on every headless run, not a guard: `context_get_resolution` returns `-1.0` over an `ImageSurface`.
- Every cell's background rectangle is filled, including empty ones, which keeps wide-character trailing placeholders and never-painted cells aligned.

**Gotchas**

- `FALLBACK_FG`, `FALLBACK_BG`, `draw_glyph` and `draw_text_line` are `pub(crate)` so `cursor.rs` and `pane_view.rs` reuse them; reimplementing one lets cursor colors, glyph placement or the strip's text drift from the grid paint.

#### `scrollbar.rs`

Purpose: the pure projection of a Neovim window's `win_viewport` onto a pane-local thumb rectangle and its colour. GTK-, Cairo- and Pango-free, so the whole rulebook runs in the fast suite; [`pane_view.rs`](#pane_viewrs) fills the rectangle and decides nothing.

**Public API**

- `scrollbar_thumb(pane_w, grid, viewport) -> Option<ScrollbarThumb>` — `None` when the whole buffer fits its window, when the pane is too narrow to hold the gutter, or on any degenerate input.
- `scrollbar_thumb_color(bg) -> Rgb`, over the editor's own background.
- The five constants those two read, `SCROLLBAR_THICKNESS_PX` through `SCROLLBAR_GREY`.

**Decisions**

- `x` is measured from the pane's right edge, not the grid rectangle's, so the thumb lies wholly inside the right padding gutter and covers no glyph at any pane width. macOS's `drawScrollbars` documents that rule and computes the other, putting its bar over the last text column below the cap.
- It never enters `grid_size_for_pane`, and that is the load-bearing half: a thumb consuming grid width would drop a column the instant a buffer overflowed its window and restore it when it fit, reflowing text on the scroll path.
- The height clamps to the track as well as to its own floor; macOS omits the upper clamp and draws a thumb taller than its track in a short pane.
- The colour blends toward `bg` rather than compositing with an alpha, which keeps the crate's no-`set_source_rgba` draw path. The blend is [`statusbar_layout.rs`](#statusbar_layoutrs)'s.

**Gotchas**

- `Viewport::curline` is deliberately unread, matching macOS: a cursor move that scrolled nothing must not make the indicator jump.

#### `search_overlay.rs`

Purpose: the project-search overlay — a `Ctrl+Shift+F` entry over a flat, streaming list of ripgrep matches. Activating a row opens a location, not a file.

**Public API**

- `build(overlay, area, core) -> SearchHandle` — starts hidden; `area` is the focus-return widget.
- `SearchHandle::open`, `close` (both idempotent), `is_open`, and `set_project_open`, mirrored from the shell's empty-state flag.
- `open_decision`, `search_options_for`, `result_rows`, `activation_target`, `enter_action` and `search_status` — the pure seams.

**Decisions**

- Typing runs nothing: the first Enter runs the search, the second activates the selected row. An empty query is refused, since core would hand `rg` a pattern matching every line of every file.
- Closing and re-running both cancel: dropping the `SearchSession` kills the `rg` child; see [DECISIONS.md](../DECISIONS.md).
- `open_decision` refuses without a project, since the search root would otherwise fall back to the home directory.
- The options shipped are literal, case-insensitive and over all files, with no toggle buttons and no `WolfCore::ignorecase()` read, so macOS's `SearchToggleSeam` seeds have no counterpart.
- Rows are flat, one per matched line, which is what lets [`picker.rs`](#pickerrs)'s selection seams drive them unchanged; the order and the file labels come from core's projection.

**Gotchas**

- `all_files: true` reaches everything `rg --hidden` lists. Measured at this repository's root: 131 `.git/` paths out of 566, and a content search returns `.git/hooks` hits.
- A same-file jump to another line records no history, while a cross-file one does: core folds a `NavTarget` per window, and `NavTarget::Edit` carries no line number.

#### `shell.rs`

Purpose: the glue for the action namespace — `gio` actions, the primary menu, the chord controller, the empty state, the command drain, and crash-recovery orchestration.

**Public API**

- `build_menu_model()`, `build_empty_state()`, `build(parts) -> AppShell`.
- `install_actions(app)` — after the window is constructed, before `present()`, so the menu and header button resolve their actions.
- `set_empty_state(empty)`, `on_liveness_wake() -> bool`, `on_render_wake()`.

**Decisions**

- The chord controller matches keys itself, not through GTK's keymap-aware matching, which does not fire under the smokes' keystroke injector. Accelerators are registered only so the menu renders their labels.
- The controller lives on the window, so it runs ahead of any overlay's and needs no open-overlay guard: no overlay's keys are bound chords.
- Overlay side effects fire only on a real mode transition, and the focus sample precedes the hide, per [DECISIONS.md](../DECISIONS.md).
- One settle point moves the sidebar's root, over [`workspace.rs`](#workspacers)'s `reroot_decision`, so a front-end `chdir_to` and a typed `:cd` cannot diverge. Neither front-end producer re-roots synchronously: the search resolves its root from the live cwd, which updates only after Neovim reports it.
- The nav-button refresh caches nothing, unlike the title refresh: `set_sensitive` early-returns on an unchanged value.
- One private applier is the only caller of `sidebar.set_visible`, over [`state_seam.rs`](#state_seamrs)'s `sidebar_is_shown`, or the next `set_empty_state(false)` would undo a user toggle.

#### `state_seam.rs`

Purpose: the pure application-state seam — what to restore at launch, and what to persist on each wake. GTK-free and IO-free, and every input is injected, so the whole rulebook is testable with no temp directory.

**Public API**

- `restore_plan(saved, process_cwd, home, is_dir) -> RestorePlan` — the working directory, the empty-state flag, the tab's saved layout, the window size, and the saved sidebar preference.
- `restored_window_size(w, h)`, `persistable_layout(node)`, `sidebar_is_shown(user_wants, empty)`, `snapshot_cwd(empty, launch_cwd, live_cwd)`, `may_snapshot(liveness, bound_panes, reported_windows)`, `snapshot_state(...)`.

**Decisions**

- A saved layout is attached only when the chosen directory is the one it was saved from, because its leaves are absolute paths belonging to one project. Reopening one project's files inside another would look like data corruption.
- `restored_window_size` collapses the whole result on any unusable dimension, per the geometry convention: a usable width with a garbage height is not half a window size.
- `persistable_layout` suppresses by content, never by arity; see [`pane_layout.rs`](#pane_layoutrs) and [DECISIONS.md](../DECISIONS.md).
- The empty state persists an empty cwd always. A passive launch hands Neovim a home directory purely because a process needs one, and persisting it would turn every launcher start into a project at home.
- `may_snapshot`'s match over `Liveness` is total, so a new variant cannot silently read as "not alive".
- `snapshot_state` is the one place the single-window, single-tab shape lives, pinned by its own test so a later unit changes it deliberately.
- `RestorePlan::sidebar_visible` is independent of the empty state; `sidebar_is_shown` applies that separately.

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

- `build(core, area, start, root_label) -> TreeHandle` — `area` is the focus-return widget; `start` is false in the empty state, where nothing is walked.
- `widget()`, `on_render_wake()`, `close()`, `set_sidebar_visible(shown)` and `reroot(root_label)`, the last two relayed from [`shell.rs`](#shellrs)'s one visibility site and one settle point; this module reads no cwd.

**Decisions**

- The root store holds one item: a synthetic `Dir` named after the working directory whose rel-path is the empty cwd sentinel, so the create-func needs no special case. `remove_all` destroys its expanded state, so it is re-expanded after every rebuild and before a reveal walks ancestors.
- Children materialize lazily through `wolf_core::tree::TreeIndexCache` under rule R1, on `children_for`: the memo and the entries are borrowed inside one statement touching no widget, because GTK re-enters the create-func and a borrow held across `set_expanded` or `set_list_row` is a panic.
- Only a traversal batch is coalesced ([`refresh_coalescer.rs`](#refresh_coalescerrs)); every other refresh is immediate, and all pass both gates in [`tree_gate.rs`](#tree_gaters). A pending reveal retries even when the rebuild is skipped.
- The current-file bold moves through a registry of bound labels, which is what the version gate costs: a suppressed rebuild re-binds no row.

**Gotchas**

- Manual expansion below the root does not survive a version advance: every streaming batch and every re-root rebuilds the root store. Preserving it means owning each expanded directory's `gio::ListStore` and diffing into it per snapshot; GTK has no identity-keyed reload.

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
