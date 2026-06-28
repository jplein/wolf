# wolf-gnome/src

The `wolf-gnome` source modules. Each is a single file (not a subdirectory). The
crate is **both a library and a binary**: a `[[bin]]` (`main.rs` → `app.rs`) runs
the windowed application, while the library keeps the pure, widget-free seams
(`input.rs`, `render.rs`, `geometry.rs`, `cursor.rs`, `tree.rs`) so they stay
testable without a display. The GTK glue that binds them to live widgets lives in
`app.rs`, `picker.rs`, and `tree_view.rs`. See the crate [`README.md`](../README.md).

## `main.rs`

The thin binary entry point. `main` only forwards the process exit code from
[`app::run`](#apprs); the binary takes **no** command-line arguments (files are
opened from within Neovim via `:e`).

## `lib.rs`

Crate root. Declares the public modules (`pub mod app;`, `pub mod cursor;`,
`pub mod geometry;`, `pub mod input;`, `pub mod picker;`, `pub mod render;`,
`pub mod tree;`, `pub mod tree_view;`) and re-exports the public surface:
`gdk_key` (from `input`), `paint` + `CellMetrics` (from `render`), `grid_size_for`
(from `geometry`), and `is_picker_trigger` (from `picker`). The `cursor` module's
`cursor_rect` / `cursor_colors`, and the `tree` / `tree_view` items, are reached
via the module path rather than re-exported.

## `app.rs`

All GTK4/Adwaita application wiring. `pub fn run() -> glib::ExitCode` builds an
`adw::Application` with id `org.wolf.Wolf` and, on `activate`, `build_ui`
constructs the window and attaches a freshly started `WolfCore`:

- **Construction.** An `adw::ApplicationWindow` (title "Wolf", default 1200×640)
  whose content is a vertical `gtk::Box` of an `adw::HeaderBar` over a
  `gtk::Paned` (horizontal). The Paned's **start** child is the file-tree sidebar
  (built by [`tree_view::build`](#tree_viewrs)); its **end** child is the
  `gtk::Overlay`, whose main child is the focusable `gtk::DrawingArea` and whose
  overlay child is the file picker ([`picker::build`](#pickerrs), floating over the
  editor when open). The Paned divider position starts at `TREE_PANE_WIDTH` (250
  px). The header bar carries an **Open** button (`document-open-symbolic`, tooltip
  "Open file (Ctrl+Shift+P)") whose click opens the picker. `CoreConfig.cwd` is set
  from `std::env::current_dir()`. Cell metrics are measured **once** (via a
  throwaway 1×1 `ImageSurface` probe → `CellMetrics::for_font`) and shared by Rc
  with the draw and resize closures.
- **Wake bridge + threading.** A bounded `async-channel` (capacity 1) carries
  wake signals; the `WakeFn` (the only thing that crosses a thread boundary)
  `try_send`s from the tokio worker, and a `glib::spawn_future_local` task on the
  GTK main thread drains it and, per signal, calls `area.queue_draw()` **and**
  `tree.on_render_wake()` (so the tree highlights/reveals the current file carried
  by the same render wake). `WolfCore` is started before the window is built; if it
  fails, the error is logged and no window appears.
- **Draw func + mode-aware cursor.** Reads `read_snapshot()`, calls the pure
  `paint` seam, then draws a cursor at the cursor cell. `draw_cursor` is GTK glue
  only: it takes the shape and `cell_percentage` from
  `mode_infos[cursor.mode_idx]` (defaulting to `CursorShape::Block`), delegates the
  rectangle to `cursor::cursor_rect` and the `(fill, glyph)` colors to
  `cursor::cursor_colors`, and issues an **opaque** Cairo fill (`set_source_rgb`).
  For a `Block` cursor it then redraws the cell's glyph in the inverted glyph color
  via `render::draw_glyph` so it stays readable (skipped when the cell is empty or
  out of grid bounds); `Vertical`/`Horizontal` bars leave the glyph untouched.
- **Key controller (+ picker wiring).** A `gtk::EventControllerKey` on the window:
  while the picker is open it `Proceed`s (keys are left for the focused picker
  widgets and never forwarded to nvim/core); otherwise the trigger chord
  (`is_picker_trigger`, Ctrl+Shift+P) opens the picker and consumes the key
  (`Stop`); any other key routes via the pure `gdk_key` seam into
  `WolfCore::send_input` (`Stop` when handled, else `Proceed`).
- **Resize.** `connect_resize` → `grid_size_for(width, height, cell_w, cell_h)` →
  `WolfCore::resize`.
- **Lifecycle / shutdown.** `WolfCore` lives in `Rc<RefCell<Option<WolfCore>>>` on
  the main thread; `connect_close_request` first closes the picker **and** the tree
  (each dropping its `FileSearch` so any in-flight traversal is cancelled) before
  the core shuts down, then takes the core out (in its own statement to release the
  borrow) and runs `WolfCore::shutdown`. `wire_close` takes the `&TreeHandle`
  alongside the `&PickerHandle` for this.

Key constants: `APP_ID` (`org.wolf.Wolf`), `FONT_FAMILY` (`DejaVu Sans Mono`),
`DEFAULT_FONT_PT` (`14.0`) with the `FONT_PT_ENV` (`WOLF_FONT_PT`) override,
`INITIAL_COLS`/`INITIAL_ROWS` (80×24), `WINDOW_WIDTH`/`WINDOW_HEIGHT` (1200×640),
`TREE_PANE_WIDTH` (250), `WAKE_CHANNEL_CAP` (1), and `RGB_MAX` (255.0).
`WINDOW_WIDTH` was widened from the editor-only default so that, after the tree
takes `TREE_PANE_WIDTH` on the left, the editor pane keeps roughly its previous
width (important for the OCR smoke tests).

## `geometry.rs`

The pure pixel-to-grid geometry seam (no GTK/pango):

```rust
pub fn grid_size_for(width_px: i32, height_px: i32, cell_w: f64, cell_h: f64) -> (u16, u16)
```

`cols = floor(width_px / cell_w)`, `rows = floor(height_px / cell_h)`, each clamped
to `[1, u16::MAX]`. As a resize guard, the **whole** result collapses to `(1, 1)`
if either pixel dimension is non-positive or either cell size is degenerate (zero,
negative, NaN, or infinite). The clamp is done in `f64` space *before* the cast, so
the truncating cast to `u16` is saturating-safe by construction. `#[must_use]`.

## `input.rs`

The GDK key-translation seam.

```rust
pub fn gdk_key(keyval: gdk::Key, state: gdk::ModifierType) -> Option<wolf_core::KeyEvent>
```

- **Modifier extraction**: `ctrl` ← `CONTROL_MASK`, `shift` ← `SHIFT_MASK`,
  `alt` ← `ALT_MASK`, `meta` ← `SUPER_MASK`.
- **Mapping table** (named keys resolved *before* the printable fallback, since
  their `to_unicode()` is often a control char): `Return`/`KP_Enter` → `Enter`;
  `Escape` → `Esc`; `Tab`/`ISO_Left_Tab` → `Tab`; `BackSpace` → `Backspace`;
  `Delete` → `Delete`; `Left`/`Right`/`Up`/`Down`; `Home`/`End`;
  `Page_Up`/`Page_Down` → `PageUp`/`PageDown`. The contiguous `F1..=F12` keyval
  range maps to `Key::F(n)`, with the named base constant `F_KEY_FIRST = 1` (so
  `F1` → `n = 1`).
- **Printable fallback**: otherwise `keyval.to_unicode()`; a non-control char
  becomes `Key::Char(c)`.
- **Shift rule**: the printable branch *always* sets `shift = false` (the keyval
  already carries the shifted glyph); named keys pass the real `shift` through.
  `ISO_Left_Tab` is treated as Shift+Tab → `Key::Tab` with `shift = true` even if
  the SHIFT mask was not set.
- **`None` behavior**: bare modifier / lock keys (`is_modifier_or_lock`:
  `Shift_L/R`, `Control_L/R`, `Alt_L/R`, `Meta_L/R`, `Super_L/R`, `Hyper_L/R`,
  `Caps_Lock`, `Shift_Lock`, `Num_Lock`, `Scroll_Lock`); any keyval with no
  unicode value; and any control-character unicode value.

Internal helpers: `named_key` (the match table), `function_key` (the `F1..=F12`
range check via `into_glib()`), and `is_modifier_or_lock`.

## `render.rs`

The Cairo/Pango paint seam.

```rust
pub struct CellMetrics { pub cell_w: f64, pub cell_h: f64, pub baseline: f64, pub font: pango::FontDescription }
impl CellMetrics { pub fn for_font(cr: &cairo::Context, family: &str, size_pt: f64) -> Self }
pub fn paint(cr: &cairo::Context, snap: &wolf_core::Snapshot, metrics: &CellMetrics)
```

- **`CellMetrics` fields** (all in Cairo device units — pixels for an
  `ImageSurface`): `cell_w` advance width, `cell_h` line height, `baseline`
  offset from cell top, and `font` the base `pango::FontDescription` (cloned and
  toggled for bold/italic per cell).
- **`CellMetrics::for_font`** builds a `FontDescription` for `family` at
  `size_pt` (× `pango::SCALE`), creates a Pango context from `cr` (surface size
  irrelevant), and reads its `FontMetrics`: `cell_w` = `approximate_char_width`
  (exact for monospace), `cell_h` = ascent + descent, `baseline` = ascent — each
  divided by `pango::SCALE`.
- **`paint` algorithm**, per `(row, col)` over the grid: resolve the cell's
  highlight via `HighlightTable::resolve` (`reverse` + defaults already applied);
  choose fg/bg with fallback to the editor defaults and finally to the named
  `FALLBACK_FG` (white) / `FALLBACK_BG` (black); convert 8-bit channels via the
  `RGB_MAX = 255.0` divisor; **always** fill the background rectangle (the Cairo
  `Result` is `expect`ed, never dropped); then **skip the glyph for empty text**
  (`cell.text.is_empty()` — a wide-char trailing placeholder or never-painted
  cell, where the bg fill alone preserves alignment).
- **`draw_glyph`** (the glyph path): clones the base font, toggles
  `Weight::Bold` / `Style::Italic` per `ResolvedStyle`, creates a pangocairo
  layout, sets the text, applies single `Underline` via an `AttrList` when
  `style.underline`, sets the fg source, moves to the cell top-left, and shows
  the layout.
- **Attributes applied**: bold, italic, single underline. **Deferred** for Tier
  1: undercurl and strikethrough. No system theme is queried.

`FALLBACK_FG` / `FALLBACK_BG` and `draw_glyph` are `pub(crate)` so the `cursor`
module and `app.rs` can reuse them (no color/alignment drift).

## `cursor.rs`

The pure, GTK-free cursor geometry and color logic (mirrors `geometry.rs`: no
widget, no display, no nvim — fully unit-tested in its own `#[cfg(test)] mod
tests`). `app.rs`'s `draw_cursor` is the only GTK glue: it computes the cell box
from `CellMetrics`, calls these two functions, issues the opaque fill, and (for a
block) redraws the glyph via `render::draw_glyph`.

```rust
pub fn cursor_rect(shape: wolf_core::CursorShape, cell_x: f64, cell_y: f64, cell_w: f64, cell_h: f64, cell_percentage: Option<u32>) -> (f64, f64, f64, f64)
pub fn cursor_colors(snap: &wolf_core::Snapshot) -> (wolf_core::Rgb, wolf_core::Rgb)
```

- **`cursor_rect`** returns the `(x, y, w, h)` to fill for `shape` within the cell
  box:
  - `Block` → the whole cell, ignoring `cell_percentage`.
  - `Vertical` → a left bar of full height; width = `cell_percentage` of `cell_w`,
    floored to `MIN_BAR_PX` and capped at `cell_w`.
  - `Horizontal` → a bottom bar of full width (anchored to the cell's bottom
    edge); height follows the same rule against `cell_h`.
  - `cell_percentage` is a **PERCENT** (1–100): it is divided by `PCT_DENOM`
    (`100.0`) and clamped to `0.0..=1.0`, defaulting to `DEFAULT_CELL_PCT`
    (`0.25`, Neovim's convention) when `None`. Named constants: `MIN_BAR_PX`
    (`2.0`), `DEFAULT_CELL_PCT` (`0.25`), `PCT_DENOM` (`100.0`).
- **`cursor_colors`** returns the `(fill, glyph)` pair using **reverse video**. It
  honors the current mode's cursor highlight (`mode_infos[cursor.mode_idx].attr_id`,
  resolved) as `(fill, glyph) = (bg, fg)` **only when that yields a visible
  cursor** — both fg and bg resolve to `Some`, the fill `bg` differs from the
  underlying cell's background (no dark-on-dark over an identical cell), and `bg`
  differs from `fg` (the glyph is not invisible against its own fill). Otherwise it
  **inverts** the underlying cell: `fill` = the cell's fg, `glyph` = the cell's bg,
  each falling back through the editor defaults and finally to `render`'s
  `FALLBACK_FG` (white) / `FALLBACK_BG` (black). An out-of-range `mode_idx`, empty
  `mode_infos`, or an out-of-bounds cursor cell all fall through to inversion
  without panicking (via `.get`).

## `picker.rs`

The GNOME file-picker UI. Almost all of it is GTK widget wiring (which cannot be
unit-tested headlessly); the small pure seams (`is_picker_trigger`,
`next_selection`, `restored_index`) are factored out and unit-tested in the
module's own `#[cfg(test)] mod tests`, like `geometry.rs` / `cursor.rs`.

**Widgets.** `build(overlay, area, core) -> PickerHandle` constructs a floating
`gtk::Box` (a `SearchEntry` over a `ScrolledWindow` wrapping a single-selection
`ListBox`), adds it as an `Overlay` child over the editor `area`, wires all
signals, and returns a `PickerHandle`. The box starts hidden. `PickerHandle`
exposes `open()` / `close()` (both idempotent) and `is_open()` (the app queries it
to suppress key forwarding while the picker has focus). Trigger: **Ctrl+Shift+P**
or the header-bar Open button.

**Streaming refresh (the wake pattern).** `open()` calls
`WolfCore::start_file_search(wake)`, shows + focuses the entry, and does an initial
refresh so the MRU seed appears immediately. The `wake` (a `WakeFn` fired from a
tokio worker on each traversal batch) only `try_send`s onto a bounded
`async-channel` (capacity `PICKER_WAKE_CHANNEL_CAP = 1`, coalescing a burst; the
send is **drop-safe** so a late cancel-path wake is harmless); a
`glib::spawn_future_local` task drains it on the GTK main thread and calls
`refresh()`. `connect_search_changed` also refreshes on every query change.
`refresh()` reads the live `FileSearch` snapshot, calls `wolf_core::rank_matches`,
and rebuilds the `ListBox` from the top `PICKER_MAX_ROWS = 200` matches, keeping a
parallel `row_paths` vec (row index → absolute path). The drain task holds only a
`Weak` ref to the picker state, so dropping the handle ends the task (no cycle).

**Keyboard navigation.** A capture-phase `EventControllerKey` on the picker box:
`Esc` closes; `Up`/`Down` move the **list** selection (not the entry's text
cursor) via `move_selection` (`next_selection`, clamped `[0, n-1]`, no wrap) and
are consumed; every other key proceeds to the `SearchEntry` so typing flows.
Selection is **preserved** across a streaming rebuild unless the query changed:
`refresh()` uses `last_query` to decide reset-vs-preserve and `restored_index`
(reset → row 0; else the clamped previous index; `None` for an empty list) to pick
the row to select.

**Activation.** `Enter` in the entry (`connect_activate`) or a `row-activated`
signal calls `activate_selection`, which sends `InputEvent::Open { path }` for the
selected row (or the first row if none) to the core — the MRU is auto-recorded by
the core input pump — then closes the picker. With **zero** rows it is a true
no-op (it neither sends `Open` nor closes). `close()` drops the `FileSearch`
(cancelling the traversal), hides the box, and returns focus to the editor.

**Pure seams (unit-tested):**

```rust
pub fn is_picker_trigger(keyval: gdk::Key, state: gdk::ModifierType) -> bool
fn next_selection(current: i32, delta: i32, n_rows: i32) -> Option<usize>
fn restored_index(reset_to_top: bool, prev: Option<usize>, new_len: usize) -> Option<usize>
```

- `is_picker_trigger` — `true` iff `CONTROL && SHIFT && letter == 'p'`
  (case-insensitive on the letter; other modifiers like ALT are ignored). Pure and
  display-free, re-exported from the crate root.
- `next_selection` — `clamp(current + delta, 0, n_rows - 1)`, `None` when
  `n_rows <= 0` (no wrap; a negative/out-of-range `current` is clamped by the same
  formula).
- `restored_index` — `None` for an empty list; `Some(0)` when resetting to the
  top; else the preserved `prev` (defaulting to `0`) clamped into the new range.

## `tree.rs`

The **pure, GTK-free** hierarchical projection of the picker's flat file list,
unit-tested headlessly in its own `#[cfg(test)] mod tests` (mirroring
`geometry.rs` / `cursor.rs`). The file picker's `FileSearch` produces a flat list
of `wolf_core::FileEntry` (relative `/`-separated `display`, absolute `path`); this
module turns it into the per-directory children GTK's lazy `TreeListModel`
create-func needs, plus the reveal helpers.

```rust
pub enum TreeNode {
    Dir  { name: String, rel_path: String },
    File { name: String, rel_path: String, abs_path: std::path::PathBuf },
}
pub fn children_of(entries: &[wolf_core::FileEntry], dir: &str) -> Vec<TreeNode>
pub fn ancestors_to_reveal(rel_path: &str) -> Vec<String>
pub fn node_rel_path(node: &TreeNode) -> &str
pub fn rel_path_for(entries: &[wolf_core::FileEntry], abs: &std::path::Path) -> Option<String>
```

- `TreeNode` — one immediate child of a directory. `rel_path` is the full relative
  path from the cwd root (a dir `b` under `a` has `name = "b"`, `rel_path = "a/b"`);
  a `File`'s `rel_path` equals the originating `FileEntry::display` and its
  `abs_path` is that entry's `path`.
- `children_of(entries, dir)` (`#[must_use]`) — the IMMEDIATE children of `dir`
  (`dir == ""` is the cwd root). Each `display` is split on `/`; the component just
  after `dir` becomes a `File` (when it is the entry's final component) or a `Dir`.
  Dir nodes are **deduped** by `rel_path`, and only dirs that (transitively)
  contain a listed file appear (automatic — dirs are derived from file paths, so no
  phantom dirs). The result is sorted **directories first, then files**, each group
  **case-insensitively by `name`** with `rel_path` as a deterministic tie-break.
  O(n) per call (fine for lazy per-directory expansion).
- `ancestors_to_reveal(rel_path)` (`#[must_use]`) — the ancestor dir rel-paths from
  the root down to the file's parent: `"a/b/c.md"` → `["a", "a/b"]`; `"c.md"` →
  `[]`; `""` → `[]`.
- `node_rel_path(node)` — the `rel_path` of either variant.
- `rel_path_for(entries, abs)` (`#[must_use]`) — the current-file → tree-display
  mapping used by `tree_view`'s reveal: returns `entry.display.clone()` for the
  FIRST entry whose `path == abs` **exactly** (a parent-dir prefix does not match),
  else `None`.

The separator is always `/` (it is `FileEntry::display`'s separator).

## `tree_view.rs`

The **GTK glue** that turns `tree`'s pure projection into a live file-tree sidebar
(a `gtk::ListView` driven by a `gtk::TreeListModel`). It mirrors `picker.rs`'s
patterns: a long-lived `wolf_core::FileSearch` started at build time, an
`async-channel` + `glib::spawn_future_local` drain that refreshes on each traversal
batch, `Rc`/`RefCell` interior mutability on the single GTK main thread, and a
`Weak` upgrade in the drain task to avoid a reference cycle.

**`build(core, area) -> TreeHandle`** assembles the widgets and returns a handle.
`TreeHandle` exposes `widget()` (the root `gtk::ScrolledWindow`, placed as the
Paned start child), `on_render_wake()` (called from the app's render drain), and
`close()` (drops the `FileSearch`, cancelling the traversal — called by
`wire_close` before shutdown).

- **Model.** Each `TreeNode` is wrapped in a `glib::BoxedAnyObject` (no custom
  GObject subclass). The root `gio::ListStore` holds `children_of(entries, "")`
  (the cwd's immediate children, so the top level is "expanded by default"). A
  `gtk::TreeListModel` (passthrough = false, **autoexpand = false**) lazily
  materializes each directory's children via a create-func that re-runs
  `children_of` against the latest shared `entries` (files return `None`). A
  `gtk::SingleSelection` wraps it with `set_autoselect(false)` (no stray row-0
  highlight before a reveal), and the `gtk::ListView` uses
  `set_single_click_activate(true)`.
- **Factory.** A `gtk::SignalListItemFactory` builds a `gtk::TreeExpander` + a
  `gtk::Label` per row; bind sets `expander.set_list_row(..)`, the label text to the
  node name, and adds/removes the `CURRENT_FILE_CSS` (`wolf-current-file`, bold via
  CSS loaded once into the default display) class iff the row's `File` `abs_path`
  equals `Snapshot::current_file`.
- **Streaming refresh.** The long-lived `FileSearch` fires a drop-safe `wake`
  (bounded(1) `async-channel`) on each batch; the drain task calls `refresh()`,
  which copies the latest entries, rebuilds the root store from
  `children_of(entries, "")`, then retries any `pending_reveal`. The initial
  `refresh()` shows the MRU seed immediately.
- **Highlight + reveal.** `on_render_wake()` reads `Snapshot::current_file`; if it
  changed since `last_current`, it records it, stashes it as `pending_reveal`, and
  `refresh()`es (so fresh binds pick up the highlight) then `reveal`s. `reveal(abs)`
  maps the path via `rel_path_for`, expands its `ancestors_to_reveal` top-down
  (re-scanning row positions after each expansion via `find_row`), `selects` the
  file row, and `list_view.scroll_to(pos, …)` it into view (needs the gtk `v4_12`
  feature). Because the current file can arrive before the tree has streamed in,
  an unresolved reveal stays in `pending_reveal` and is retried on each refresh.
- **Single-click activation.** `connect_activate` → `activate(pos)`: a `File` sends
  `InputEvent::Open { path }` then `area.grab_focus()` (returns focus to the
  editor); a `Dir` toggles its row's expansion.

**v1 limitation (documented in the code):** rebuilding the root store on each
refresh collapses any manual nested expansion; the subsequent reveal re-expands the
path to the current file, but other manually-expanded directories are not restored.
