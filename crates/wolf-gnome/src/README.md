# wolf-gnome/src

The GTK front-end's modules, one entry each. The pure seams here are the Linux counterparts of the ones in [`wolf-macos/Sources/WolfKeyMap/`](../../wolf-macos/Sources/WolfKeyMap/README.md), and the two are kept deliberately close.

**Read [DECISIONS.md](../DECISIONS.md) first** for the gtk-rs version constraints, the threading and wake model, the window structure, and why this crate has no Bazel target.

## Files

#### `app.rs`

Purpose: all GTK4/Adwaita window wiring. `run` builds the `adw::Application` and, on activate, constructs the window — header bar, file-tree sidebar, and an editor overlay holding the pane view and the picker — attached to a freshly started `WolfCore`.

**Public API**

- `run() -> glib::ExitCode` — hands GTK an empty argv, so the process command line is never parsed or rejected; files are opened from within Neovim or via the picker and tree.

**Decisions**

- Cell metrics are measured once from a throwaway 1x1 `ImageSurface` (used only to obtain a Pango context) and shared by `Rc`, so a resize arriving before the first draw already has a size to work from.
- The wake fires on a tokio worker and cannot touch GTK, so it only pushes onto a bounded channel that a `glib::spawn_future_local` task drains on the main thread. A dropped send on a full channel is correct: a redraw is already pending.
- `WolfCore` is non-`Send`, lives in an `Rc<RefCell<Option<_>>>`, and is taken out in its own statement on close so the borrow is released before `shutdown` blocks.
- The picker and tree close before the core, dropping their `FileSearch`es so in-flight traversals cancel first.

**Gotchas**

- While the picker is open, the key controller forwards nothing to the core.
- The focus-return widget handed to the picker and tree is the pane view's cmdline strip — the focusable widget in the editor region that keeps keys bubbling to the window-level controller.

#### `cursor.rs`

Purpose: pure, GTK-free cursor geometry and color logic — the pixel rectangle to fill for a shape, and the `(fill, glyph)` color pair. Keeping it widget-free is what lets it be unit-tested with no display.

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

Purpose: the pure pixel-to-grid conversion behind every resize path, kept GTK- and pango-free so it is unit-tested headlessly.

**Public API**

- `grid_size_for(width_px, height_px, cell_w, cell_h) -> (u16, u16)` — floor division per axis, each clamped into `[1, u16::MAX]`.

**Decisions**

- Any degenerate input — a non-positive pixel dimension, or a cell size that is zero, negative, NaN, or infinite — collapses the whole result to the minimum grid rather than only the offending axis. No meaningful grid exists in those cases, and a partial result would send Neovim a nonsense size for the good axis.
- The quotient is clamped in `f64` space before the cast to `u16`, which makes the truncating cast exact and in range by construction.

#### `input.rs`

Purpose: translates a GDK keyval plus modifier state into a `wolf_core::KeyEvent`. It has the exact shape of a `gtk::EventControllerKey` callback but stays a free function so it is testable without a display server.

**Public API**

- `gdk_key(keyval, state) -> Option<KeyEvent>` — `None` for bare modifier and lock keys, and for any keyval whose unicode value is absent or a control character.

**Decisions**

- Named keys resolve before the printable fallback, because their `to_unicode()` is often a control character (`Return` is `\r`) and would otherwise be swallowed by the control-character rejection.
- For printable characters `shift` is always false: GDK already delivers the shifted glyph in the keyval, and `KeyEvent` ignores `shift` for `Key::Char`. Named keys pass the real shift through, which is what keeps `<S-Tab>`, `<S-Left>`, and `<S-F5>` working.
- `ISO_Left_Tab` is what Shift+Tab actually reports on X11, so it maps to `Key::Tab` with `shift` forced true even when the platform did not also set the shift mask.

#### `layout.rs`

Purpose: the pure, GTK-free n-ary pane-layout tree behind the split-panes UI, mirroring Neovim's frame model. It owns every pane-geometry decision that can be made without a display; `pane_view.rs` is the only place that turns it into widgets.

**Public API**

- `PaneTree::split(target, new_pane, orientation)` — no-op when `target` is not a leaf.
- `PaneTree::remove(pane)` — removing the last pane is a no-op, so the tree is never empty.
- `PaneTree::leaves()` and `pane_rects(w, h)` — the same deterministic depth-first order; the rects tile the bounds exactly.
- `PaneTree::neighbor(from, dir, w, h)` — the nearest pane in `dir` that strictly overlaps `from` on the cross axis.
- `PaneTree::layout() -> PaneLayout` — a read-only mirror for the widget layer; the internal frame type stays private.
- `Orientation` — `Vertical` divides width (side by side), `Horizontal` divides height (stacked), matching `wolf_core::SplitOrientation`.

**Decisions**

- A split whose orientation matches its containing frame inserts a flat sibling after the target and re-equalizes the row; only a perpendicular split nests. A binary always-nest model turns a third same-orientation split into 25/25/50 and starves the pre-existing pane to nothing.
- `remove` normalizes bottom-up — flatten same-orientation children, collapse a one-child split into its child, re-equalize — so the tree stays canonical and matches Neovim's `equalalways`.
- Cross-axis overlap must be strictly positive within `NEIGHBOR_EPS`, so a merely shared edge never counts and a 2x2 grid has unambiguous neighbours.

#### `lib.rs`

Purpose: crate root. Declares the modules and re-exports the display-free seams — `gdk_key`, `paint` with `CellMetrics`, `grid_size_for`, and `is_picker_trigger`.

**Gotchas**

- Items in `cursor`, `layout`, `pane_view`, `tree`, and `tree_view` are reached by module path rather than re-exported.

#### `main.rs`

Purpose: the binary entry point. `main` forwards the exit code from `wolf_gnome::app::run`; all wiring lives in `app.rs`.

#### `pane_view.rs`

Purpose: the GTK glue that turns `layout`'s `PaneTree` into nested `gtk::Paned`s with one `gtk::DrawingArea` per Neovim window, plus the bottom cmdline strip. It consumes layout intents, binds each pane to a grid, and paints.

**Public API**

- `build(core, metrics) -> PaneHandle` — no panes exist until the first window grid appears on a wake.
- `PaneHandle::cmdline_widget()` — the strip itself, handed to the picker and tree as their focus-return target.
- `PaneHandle::widget()` and `on_render_wake()`.

**Decisions**

- Panes paint window grids only. Grid 1 empties under `ext_messages` (which sets `cmdheight` to 0), so the strip paints from the folded `Snapshot::cmdline` and `Snapshot::toast`, not grid 1's rows.
- The active-pane ring is an `outline`, not a `border`, so a focus change consumes no layout space and causes no resize churn.
- Divider positions re-apply on every `max-position` notify rather than once, because nested allocation reports an intermediate maximum before settling and a container resize changes it again.
- Each pane area carries a one-cell content-size floor and its enclosing paneds disable child shrinking, so no pane is ever allocated zero pixels.
- Rebuilds reuse each pane's drawing area by id, preserving its bound grid and painted content.

**Gotchas**

- A pane whose grid is still pending is skipped when the widget tree is built, so resolving it triggers another rebuild.
- Panes come from split intents, so windows opened by other paths — `:help`, quickfix, preview, `:diffsplit`, floats — get a grid but no pane.

#### `picker.rs`

Purpose: the file-picker overlay — a search entry over a ranked results list floating above the editor. It opens on Ctrl+Shift+P or the header-bar button, and activation sends `InputEvent::Open`.

**Public API**

- `is_picker_trigger(keyval, state) -> bool` — the pure trigger predicate: control and shift held with the letter `p`, case-insensitive, other modifiers ignored.
- `build(overlay, area, core) -> PickerHandle` — starts hidden. `area` is the focus-return widget, not the results host.
- `PickerHandle::open`, `close` (both idempotent), and `is_open` — the app queries `is_open` to suppress key forwarding to the core.

**Decisions**

- The trigger is Ctrl+Shift+P rather than Ctrl+P so that every Neovim key, `<C-p>` included, stays available to the editor.
- The key controller runs in the capture phase, so Escape and the arrows are seen before the `SearchEntry` consumes them; arrows move the list selection while focus stays in the entry, which keeps typing working.
- The selection is preserved across a streaming rebuild unless the query changed, so an arriving traversal batch does not yank the user back to the top row.
- Activation with zero rows is a true no-op: it neither opens anything nor closes the picker.

**Gotchas**

- Closing drops the `FileSearch`, which cancels the background traversal; the wake sender ignores send errors so a late cancel-path wake is harmless.
- The drain task holds only a `Weak`, so dropping the handle ends it instead of leaking a cycle.

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

#### `tree.rs`

Purpose: a re-export shim over `wolf_core::tree`. The hierarchical projection of the flat file list and its tests live in the core as one source of truth shared by every front-end; this module keeps `crate::tree::{TreeNode, children_of, ancestors_to_reveal, node_rel_path, rel_path_for}` resolving for the GTK glue.

#### `tree_view.rs`

Purpose: the GTK glue for the file-tree sidebar — a `gtk::ListView` over a `gtk::TreeListModel`, fed by a long-lived `FileSearch` and tracking the file being edited.

**Public API**

- `build(core, area) -> TreeHandle` — `area` is the focus-return widget used after a click opens a file.
- `TreeHandle::widget()` — the scrolled window, placed as the app's `Paned` start child.
- `TreeHandle::on_render_wake()` — reveals and highlights the current file when it changes.
- `TreeHandle::close()` — drops the search, cancelling the traversal.

**Decisions**

- Nodes are wrapped in `glib::BoxedAnyObject` rather than a custom GObject subclass.
- Directory children materialize lazily against the latest shared entries, so a directory expanded mid-traversal shows whatever has streamed in by then.
- The selection model disables autoselect, so row 0 is not highlighted at startup and a selection appears only when a reveal lands.
- A reveal that cannot resolve stays pending and retries on each refresh, because the current file often arrives before the traversal has reached it.
- Row positions are re-scanned after each ancestor expansion, since expanding a row shifts every later position.

**Gotchas**

- `ListView::scroll_to` requires the gtk `v4_12` feature, which raises the minimum system GTK to 4.12.
- Each refresh rebuilds the root store, collapsing manual expansion; the reveal re-expands only the path to the current file.
