# wolf-gnome/src

The GTK front-end's modules, one entry each. The pure seams here are the Linux counterparts of the ones in [`wolf-macos/Sources/WolfKeyMap/`](../../wolf-macos/Sources/WolfKeyMap/README.md), and the two are kept deliberately close.

**Read [DECISIONS.md](../DECISIONS.md) first** for the gtk-rs version constraints, the threading and wake model, the window structure and focus regime, the accelerator invariant, the application-state rules, the crash-recovery model, and why this crate has no Bazel target.

## Files

#### `actions.rs`

Purpose: the action namespace as data — every `gio` action name, its accelerator, the `wolf_core::Command` behind it, its binding status, and its menu placement — plus the chord matcher and the invariant checker. It names no widget type, so it is tested with no display.

**Public API**

- `ACTION_TABLE` and `MENU_SECTIONS` — the source of truth the menu, the accelerators, and the command dispatch all read, so none can drift.
- `BindingStatus` — `Bound` is registered and matched; `Reserved` is declared for a later unit and is neither; `MenuOnly` carries no chord.
- `action_for_key(keyval, state)` — the matcher the chord controller calls. It scans `Bound` rows only: matching a reserved row would swallow the key and do nothing.
- `parse_accel` and `forbidden_reason` — the invariant checker, run over every row by this module's tests.

**Decisions**

- `parse_accel` re-implements the accelerator grammar this crate allows rather than calling `gtk::accelerator_parse`, which asserts an initialized main thread and cannot run headlessly. A checker nobody can run is not a checker. The grammar admits only `<Control>`, `<Shift>`, and `<Alt>` with a key name, so `<Super>` and `<Primary>` never reach `forbidden_reason`.
- `action_for_key` compares its three modifiers exactly and ignores every other bit: intercepting a Super-contaminated chord beats forwarding one Neovim mangles. Letters compare case-insensitively, since a shifted letter's keyval is keymap-dependent.

**Gotchas**

- Whether an action is installed follows from its command's classification, not from `status`. Promoting a reserved row means flipping `status` too, or the chord stays inert.

#### `app.rs`

Purpose: all GTK and Adwaita window wiring. `run` builds the `adw::Application` and, on activate, constructs the window — header bar, sidebar, and an editor overlay holding the pane view, the picker, the command palette, the buffer picker, the project search, the empty state, and the crash overlay — attached to a freshly started `WolfCore`.

**Public API**

- `run() -> glib::ExitCode` — hands GTK an empty argv, so the process command line is never parsed or rejected.

**Decisions**

- Before any widget exists, the state store is opened and turned into a restore plan, so the working directory, window size, and saved pane tree are known while the window is still being built. A missing state directory degrades every later state call to a no-op.
- The saved layout is queued on the pane view with no arity guard, one half of the content-predicate rule in [DECISIONS.md](../DECISIONS.md).
- Cell metrics are measured once from a throwaway 1x1 `ImageSurface` (used only to obtain a Pango context) and shared by `Rc`, so a resize arriving before the first draw already has a size.
- The close-request handler flushes state first, while the core and panes are still alive, and takes the core out in its own statement so the borrow is released before `shutdown` blocks. A flush after the take sees no core, is refused by the snapshot gate, and silently loses the session.

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

#### `command_action.rs`

Purpose: the pure mapping from a drained `wolf_core::Command` to the UI action this front-end performs for it. GTK-free, so the whole classification is tested headlessly.

**Public API**

- `ui_action_for(command) -> UiAction` — total.
- `UiAction` — the dispatched variants, plus `Deferred` and `NoGnomeEquivalent`, each carrying the concrete blocker as a string.

**Decisions**

- The match carries no catch-all arm, so appending a variant to `wolf_core::Command` fails to compile here until the new command is classified. A silently dropped command is indistinguishable from a broken one.
- A second check walks the command registry rather than the match, so the guard survives someone adding a catch-all arm.
- Every unimplemented reason is distinct and non-empty, enforced by a test, so no two rows collapse into one indistinguishable diagnostic and the printed message always names the real blocker.

**Gotchas**

- A `Deferred` or `NoGnomeEquivalent` command reaching the drain is printed with its reason, never dropped quietly.

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

#### `geometry.rs`

Purpose: the pure pixel conversions behind every resize, pointer event, and divider position, kept GTK- and pango-free so they are unit-tested headlessly.

**Public API**

- `grid_size_for(width_px, height_px, cell_w, cell_h) -> (u16, u16)` — floor division per axis, each clamped into `[1, u16::MAX]`.
- `cell_at(x, y, cell_w, cell_h, cols, rows) -> (u32, u32)` — returns `(row, col)`, matching `wolf_core::MouseEvent`'s field order. Coordinates are widget-local; no grid origin is subtracted.
- `divider_position(min, max, fraction)` and `divider_travel_fraction(position, max)` — the `gtk::Paned` divider conversion, exact inverses.

**Decisions**

- Any degenerate input — a non-positive dimension, a zero, negative, NaN, or infinite cell size, a paned with no travel — collapses the whole result rather than only the offending axis, because a partial result would send Neovim a nonsense size for the good axis.
- A point outside the grid clamps to the nearest edge cell rather than being rejected, which makes drag-past-the-edge selection work.
- Non-finite coordinates are two rules: NaN has no direction, so that axis reads as zero, while an infinity clamps to the far edge. Collapsing both to zero would invert an overshooting drag.
- A divider fraction is of the paned's travel (`max_position`), not of its width; that is what makes the pair exact inverses and stops the re-apply walking a weight further on every allocation pass.

**Gotchas**

- This crate has no editor insets, no column cap, and no centering, so do not import the macOS grid-origin machinery.

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
- Items in `actions`, `buffer_picker`, `crash_overlay`, `cursor`, `layout`, `mouse`, `palette`, `pane_layout`, `pane_view`, `persistence`, `picker`, `search_overlay`, `shell`, `tree`, and `tree_view` are reached by module path rather than re-exported.

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

#### `palette.rs`

Purpose: the command-palette overlay — the registry commands this front-end performs, plus one Open Recent row per usable recent directory.

**Public API**

- `build(overlay, area, core) -> PaletteHandle` — starts hidden; `area` is the focus-return widget.
- `PaletteHandle::open`, `close` (both idempotent), `is_open`, and `connect_activate`.
- `open_recent_candidates`, `palette_rows` and `palette_action_for` — the pure seams.

**Decisions**

- An Open Recent row carries a directory and no window target, and [`shell.rs`](#shellrs)'s `on_palette_activation` is the one place that directory becomes a window action.
- `open_recent_candidates` refuses the window's own working directory, because this front-end has one window and re-rooting it where it already is changes nothing; a relative path, which `chdir_to` would canonicalize against the application process's working directory rather than Neovim's; and a path `is_dir` rejects, because a failed `:tcd` is only a `tracing::warn!` in core, so the shell's pending re-root never settles. The shared store is never pruned, so a temporarily unmounted path returns.
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

Purpose: the GTK glue that turns `layout`'s pane tree into nested `gtk::Paned` with one `gtk::DrawingArea` per Neovim window, plus the bottom command-line strip.

**Public API**

- `build(core, metrics) -> PaneHandle` — no panes exist until the first window grid appears on a wake.
- `cmdline_widget()` — the strip, handed to the picker and sidebar as their focus-return target.
- `reset_after_restart()`, `request_layout_rebuild(saved)`, `layout_snapshot()`, `snapshot_readiness()`.

**Decisions**

- Panes paint window grids only. Grid 1 empties under `ext_messages`, so the strip paints from the folded `Snapshot::cmdline` and `Snapshot::toast`.
- Divider positions re-apply from the live tree on every `max-position` notify, because nested allocation reports an intermediate maximum before settling. A `notify::position` at an unchanged maximum is a user drag, written back through `set_divider_fraction`; rebuilding there would destroy the paned holding the live gesture.
- A one-cell content floor per area plus non-shrinking paned children keeps every pane above zero pixels.
- Pointer input is one `gtk::GestureDrag`, not a click controller: a gesture takes an implicit grab and keeps delivering after the pointer leaves the widget.
- The target grid and button are pinned at press while modifiers are re-read live, so a wandering drag still edits the pane it started in.
- `layout_snapshot` reads each leaf's raw absolute path, never the relativized display path.

**Gotchas**

- A pending saved layout is kept while no pane has bound and dropped once more than one exists, because splits before the restore leave it describing a different tab.

#### `persistence.rs`

Purpose: the glue that commits this crate's application state to `state.json`. It is thin by construction: every decision lives in [`state_seam.rs`](#state_seamrs) and [`pane_layout.rs`](#pane_layoutrs), so this module gathers live values and hands them over.

**Public API**

- `build(parts) -> StateHandle` — a value constructor that installs no closures and takes no snapshot of its own.
- `set_empty(empty)`, `on_render_wake()`, `flush_on_close()`.

**Decisions**

- An absent state store is the whole degradation story: every method early-returns, and the app runs normally while neither restoring nor saving. There is no error path and no second convention.
- The size comes from the window's default size and the position is never written, per [DECISIONS.md](../DECISIONS.md). The persisted `sidebar_visible` is the inverse of the live empty-state flag.
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

#### `render.rs`

Purpose: the Cairo/Pango paint seam. `paint` rasterizes one grid and its highlight table onto any Cairo context — an offscreen `ImageSurface` in tests, a drawing area's context in production.

**Public API**

- `CellMetrics::for_font(cr, family, size_pt)` — uses `cr` only to obtain a Pango context, so the surface's size is irrelevant; the advance width comes from Pango's approximate char width, which is exact for a monospace font.
- `paint(cr, grid, highlights, metrics)` — takes one grid rather than a whole `Snapshot`, because under `ext_multigrid` a front-end paints a single grid per surface.

**Decisions**

- Every cell's background rectangle is filled, including empty ones, which keeps wide-character trailing placeholders and never-painted cells aligned.
- A cell with empty text gets its background and no glyph.
- Bold, italic, and single underline render; undercurl and strikethrough do not.

**Gotchas**

- `FALLBACK_FG`, `FALLBACK_BG`, and `draw_glyph` are `pub(crate)` so `cursor.rs` and `pane_view.rs` reuse them. Reimplementing either in a caller lets cursor colors or glyph placement drift away from the grid paint.

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
- A same-file jump to another line records no navigation history: core folds a `NavTarget` per window, and `NavTarget::Edit` carries no line number. This crate references `nav_history` nowhere.

#### `shell.rs`

Purpose: the glue for the action namespace — `gio` actions, the primary menu, the chord controller, the empty state, the command drain, and crash-recovery orchestration. A drained command and a palette command row funnel through one dispatch.

**Public API**

- `build_menu_model()`, `build_empty_state()`, `build(parts) -> AppShell`.
- `install_actions(app)` — after the window is constructed, before `present()`, so the menu and header button resolve their actions.
- `set_empty_state(empty)`, `on_liveness_wake() -> bool`, `on_render_wake()`.

**Decisions**

- The chord controller matches keys itself rather than through GTK's own matching, which is keymap-aware and does not fire under the smokes' keystroke injector. Accelerators are registered only so the menu renders their labels.
- The controller lives on the window, so it runs ahead of any overlay's and needs no open-overlay guard: no overlay's keys are bound chords.
- Overlay side effects fire only on a real mode transition, and the focus sample is taken before the overlay is hidden, per the ordering rule in [DECISIONS.md](../DECISIONS.md).
- `on_palette_activation` is the one place an Open Recent row becomes a window action. It re-roots this window, because there is one window to re-root, and it does so through `chdir_to` — the same call the native Open Folder dialog makes, so the two settle through one pending re-root and cannot diverge.
- Neither producer re-roots the sidebar synchronously: the file search resolves its root from the live cwd, which updates only after Neovim reports it.

#### `state_seam.rs`

Purpose: the pure application-state seam — what to restore at launch, and what to persist on each wake. GTK-free and IO-free, and it reads neither the environment nor the process cwd, so the whole rulebook is testable with no temp directory.

**Public API**

- `restore_plan(saved, process_cwd, home, is_dir) -> RestorePlan` — the working directory, the empty-state flag, the tab's saved layout, and the window size.
- `restored_window_size(w, h)`, `persistable_layout(node)`, `snapshot_cwd(empty, launch_cwd, live_cwd)`, `may_snapshot(liveness, bound_panes, reported_windows)`, `snapshot_state(...)`.

**Decisions**

- A saved layout is attached only when the chosen directory is the one it was saved from, because its leaves are absolute paths belonging to one project. Reopening one project's files inside another would look like data corruption.
- `restored_window_size` collapses the whole result on any unusable dimension, following the geometry convention, because a usable width with a garbage height is not half a window size.
- `persistable_layout` suppresses by content, never by arity; see [`pane_layout.rs`](#pane_layoutrs) and [DECISIONS.md](../DECISIONS.md).
- The empty state persists an empty cwd always. A passive launch hands Neovim a home directory purely because a process needs one, and persisting it would turn every launcher start into a project at home.
- `may_snapshot` is a data-loss guard, not tidiness, and its match over `Liveness` is total so a new variant cannot silently read as "not alive".
- `snapshot_state` is the one place the single-window, single-tab shape lives, pinned by its own test so a later unit changes it deliberately.

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

#### `tree_view.rs`

Purpose: the GTK glue for the file-tree sidebar — a `gtk::ListView` over a `gtk::TreeListModel`, fed by a long-lived `FileSearch` and tracking the file being edited.

**Public API**

- `build(core, area, start) -> TreeHandle` — `area` is the focus-return widget. `start` is false in the empty state, where no project is open and nothing is walked.
- `widget()` — the scrolled window, placed as the app's `Paned` start child.
- `on_render_wake()` — reveals and highlights the current file when it changes.
- `reroot()` — restarts the traversal after a front-end directory change has settled; `close()` drops the search, cancelling the traversal.

**Decisions**

- Nodes are wrapped in `glib::BoxedAnyObject` rather than a custom GObject subclass.
- Directory children materialize lazily against the latest shared entries, so a directory expanded mid-traversal shows whatever has streamed in by then.
- The selection model disables autoselect, so row 0 is not highlighted at startup and a selection appears only when a reveal lands.
- A reveal that cannot resolve stays pending and retries on each refresh, because the current file often arrives before the traversal has reached it.
- Row positions are re-scanned after each ancestor expansion, since expanding a row shifts every later position.

**Gotchas**

- Each refresh rebuilds the root store, collapsing manual expansion; the reveal re-expands only the path to the current file.

#### `workspace.rs`

Purpose: the pure workspace decision — is a project open, where does Neovim start, and when has a pending re-root settled.

**Public API**

- `launch_decision(process_cwd, restored, home, is_dir) -> LaunchDecision` — `Folder` or `EmptyState`, the latter carrying the directory Neovim is started in.
- `reroot_ready(pending, live) -> bool` and `home_dir()`.

**Decisions**

- Two candidates are offered and the first that satisfies one shared predicate wins: it is a directory, it is not home, and it is not the root. The order is the rule: the process cwd comes first because a terminal launch from a project is the Linux idiom, and the restored cwd comes second, so it gets its turn exactly when a launcher start yields home or the root — which is precisely "no project".
- Applying that same predicate to the restored cwd is deliberate. A hand-edited state file naming the root would otherwise point the sidebar's file search at the whole filesystem, and home at the whole home tree — the pathological case core's search bounds exist to prune.
- `is_dir` is an injected predicate rather than a filesystem call, which is what makes the rule testable with no temp directories and no real home.
- `reroot_ready` is a plain equality, so a trailing separator is already ignored while `..` and symlinks are not. Callers canonicalize both sides first.

**Gotchas**

- `home_dir` is the one environment read in this crate, and the shell captures its result once at build time rather than re-reading per frame.
