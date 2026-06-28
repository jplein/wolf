# wolf-gnome/src

The `wolf-gnome` source modules. Each is a single file (not a subdirectory). The
crate is **both a library and a binary**: a `[[bin]]` (`main.rs` → `app.rs`) runs
the windowed application, while the library keeps the three pure, widget-free seams
(`input.rs`, `render.rs`, `geometry.rs`) so they stay testable without a display.
See the crate [`README.md`](../README.md).

## `main.rs`

The thin binary entry point. `main` only forwards the process exit code from
[`app::run`](#apprs); the binary takes **no** command-line arguments (files are
opened from within Neovim via `:e`).

## `lib.rs`

Crate root. Declares the public modules (`pub mod app;`, `pub mod geometry;`,
`pub mod input;`, `pub mod render;`) and re-exports the public surface: `gdk_key`
(from `input`), `paint` + `CellMetrics` (from `render`), and `grid_size_for` (from
`geometry`).

## `app.rs`

All GTK4/Adwaita application wiring. `pub fn run() -> glib::ExitCode` builds an
`adw::Application` with id `org.wolf.Wolf` and, on `activate`, `build_ui`
constructs the window and attaches a freshly started `WolfCore`:

- **Construction.** An `adw::ApplicationWindow` (title "Wolf", default 960×640)
  whose content is a vertical `gtk::Box` of an `adw::HeaderBar` over a focusable
  `gtk::DrawingArea`. Cell metrics are measured **once** (via a throwaway 1×1
  `ImageSurface` probe → `CellMetrics::for_font`) and shared by Rc with the draw
  and resize closures.
- **Wake bridge + threading.** A bounded `async-channel` (capacity 1) carries
  wake signals; the `WakeFn` (the only thing that crosses a thread boundary)
  `try_send`s from the tokio worker, and a `glib::spawn_future_local` task on the
  GTK main thread drains it and calls `queue_draw`. `WolfCore` is started before
  the window is built; if it fails, the error is logged and no window appears.
- **Draw func + block cursor.** Reads `read_snapshot()`, calls the pure `paint`
  seam, then overlays a semi-transparent (alpha 0.5) filled block cursor at the
  cursor cell, colored from the mode's cursor highlight (bg, then fg), else the
  editor default fg, else white.
- **Key controller.** A `gtk::EventControllerKey` on the window routes keys via
  the pure `gdk_key` seam into `WolfCore::send_input` (`Stop` when handled, else
  `Proceed`).
- **Resize.** `connect_resize` → `grid_size_for(width, height, cell_w, cell_h)` →
  `WolfCore::resize`.
- **Lifecycle / shutdown.** `WolfCore` lives in `Rc<RefCell<Option<WolfCore>>>` on
  the main thread; `connect_close_request` takes it out (in its own statement to
  release the borrow) and runs `WolfCore::shutdown`.

Key constants: `APP_ID` (`org.wolf.Wolf`), `FONT_FAMILY` (`DejaVu Sans Mono`),
`DEFAULT_FONT_PT` (`14.0`) with the `FONT_PT_ENV` (`WOLF_FONT_PT`) override,
`INITIAL_COLS`/`INITIAL_ROWS` (80×24), `WINDOW_WIDTH`/`WINDOW_HEIGHT` (960×640),
`WAKE_CHANNEL_CAP` (1), `CURSOR_ALPHA` (0.5), `RGB_MAX` (255.0), and
`FALLBACK_CURSOR` (white).

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
