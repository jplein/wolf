# wolf-gnome/src

The `wolf-gnome` source modules. Each is a single file (not a subdirectory). For
Tier 1 the crate is a **library** of two pure, widget-free seams; there is no
`main.rs` and no `gtk4`/`libadwaita` import anywhere yet (deferred — see the crate
[`README.md`](../README.md)).

## `lib.rs`

Crate root. Declares the public modules (`pub mod input;`, `pub mod render;`) and
re-exports the public surface: `gdk_key` (from `input`), and `paint` +
`CellMetrics` (from `render`). It imports no GTK widget toolkit (`gtk4`/`adw`) —
only the input-translation and paint seams live here.

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
