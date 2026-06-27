# wolf-gnome

The GTK 4 / libadwaita platform front-end for Wolf, the Markdown editor that
embeds Neovim (see [`architecture.md`](../../architecture.md), esp. the
front-end section and decision 2 on native paint). It consumes
[`wolf-core`](../wolf-core/README.md) as a normal Rust crate dependency.

## Tier 1 scope

This crate currently contains **only its two pure, widget-free "seams"** — the
parts of a future windowed application that can be verified *headlessly*, with no
display server, no window, and no event loop:

- `gdk_key` — GDK keyval + modifier state → `wolf_core::KeyEvent`.
- `paint` — render a `wolf_core::Snapshot` onto a Cairo context.

Both are free functions independent of any widget, which is exactly what makes
them testable without a display. The actual windowed application does **not**
exist yet (see [Deferred to later phases](#deferred-to-later-phases)).

## The two seams

### `gdk_key` (from [`input`](src/input.rs))

```rust
pub fn gdk_key(keyval: gdk::Key, state: gdk::ModifierType) -> Option<wolf_core::KeyEvent>
```

This is the exact shape of a future `gtk::EventControllerKey` `key-pressed`
callback, kept as a free function so it can be unit-tested without a display
(`gdk::Key` constants are compile-time; the printable path reads
`keyval.to_unicode()`).

Mapping:

- **Named keys** (resolved first, since their `to_unicode()` is often a control
  char): `Return` & `KP_Enter` → `Enter`; `Escape` → `Esc`; `Tab` &
  `ISO_Left_Tab` → `Tab`; `BackSpace` → `Backspace`; `Delete` → `Delete`;
  `Left`/`Right`/`Up`/`Down`; `Home`/`End`; `Page_Up`/`Page_Down` →
  `PageUp`/`PageDown`; the contiguous `F1..=F12` range → `Key::F(n)`.
- **Printable fallback**: any other keyval whose `to_unicode()` yields a
  non-control char → `Key::Char(c)`.

Modifier masks: `ctrl` = `CONTROL_MASK`, `shift` = `SHIFT_MASK`, `alt` =
`ALT_MASK`, `meta` = `SUPER_MASK`.

**Shift-glyph contract:** GDK already delivers the *shifted* glyph in the keyval
(Shift+`a` arrives as the `A` keyval, `to_unicode()` = `'A'`). So the printable
branch **always sets `shift = false`** and lets the glyph carry the case — this
matches `wolf_core::KeyEvent`, where `shift` is ignored for `Key::Char`. **Named
keys keep the real `shift`** (so `<S-Tab>`, `<S-Left>`, `<S-F5>`). The X11 quirk
`ISO_Left_Tab` *is* Shift+Tab, so it maps to `Key::Tab` with `shift = true` even
if the platform did not also set the SHIFT mask.

Returns `None` for: bare modifier / lock keys (`Shift_L/R`, `Control_L/R`,
`Alt_L/R`, `Meta_L/R`, `Super_L/R`, `Hyper_L/R`, `Caps_Lock`, `Shift_Lock`,
`Num_Lock`, `Scroll_Lock`); and any keyval whose `to_unicode()` is absent or a
control character.

### `paint` + `CellMetrics` (from [`render`](src/render.rs))

```rust
pub struct CellMetrics { pub cell_w: f64, pub cell_h: f64, pub baseline: f64, pub font: pango::FontDescription }
impl CellMetrics { pub fn for_font(cr: &cairo::Context, family: &str, size_pt: f64) -> Self }
pub fn paint(cr: &cairo::Context, snap: &wolf_core::Snapshot, metrics: &CellMetrics)
```

`paint` is pure and widget-free: it paints onto **any** Cairo surface — an
offscreen `ImageSurface` in tests, a `DrawingArea`'s context in production.

`CellMetrics::for_font` measures a monospace cell using `cr` only to obtain a
Pango context (its surface size is irrelevant): `cell_w` from Pango's approximate
char width (exact for monospace), `cell_h` = ascent + descent, `baseline` =
ascent.

Per-cell algorithm:

1. Resolve the highlight via `wolf_core::HighlightTable::resolve` (which has
   already applied `reverse` and the editor defaults).
2. Pick fg/bg, falling back to a built-in `FALLBACK_FG` (white) / `FALLBACK_BG`
   (black) when even the defaults are unset. 8-bit channels are divided by 255 to
   Cairo's 0.0–1.0 range.
3. **Always fill the cell's background rectangle** — this keeps wide-trailing /
   never-painted cells aligned. The Cairo `Result` is handled, not dropped.
4. If `cell.text` is empty (a wide-char trailing placeholder or never-painted
   cell) skip glyph rendering; otherwise lay the text out with Pango/pangocairo
   and draw it in fg.

Attributes applied: **bold**, *italic*, and single **underline**. Undercurl and
strikethrough are deferred for Tier 1. No system theme is queried (deferred).

## Building and testing

After editing `flake.nix` the already-loaded direnv environment is stale (it has
no GTK), so the build gate is to run everything through `nix develop --command`,
which re-evaluates the flake:

```sh
nix develop --command cargo build -p wolf-gnome
nix develop --command cargo test  -p wolf-gnome
```

The devShell provides the GTK toolchain (`gtk4`, `cairo`, `pango`, `glib`,
`gdk-pixbuf`, `graphene`, `harfbuzz`) and pins a deterministic font: it points
`FONTCONFIG_FILE` at a generated copy of
[`testdata/fonts.conf`](testdata/fonts.conf) exposing **only** DejaVu Sans Mono
(no host font dirs), so Pango measurement / Cairo rasterization is reproducible.

The round-trip test spawns **real Neovim** (no mocks, per project policy) located
via `WOLF_NVIM_BIN` (also exported by the devShell). See
[`tests/README.md`](tests/README.md).

## gtk-rs versions

The Tier-1 gtk-rs crates are one coherent gtk-rs release set: `gdk4` 0.11.x with
`cairo-rs` / `pango` / `pangocairo` 0.22.x. (In the gtk-rs ecosystem the gtk4-rs
repo and the "core" repo are released together but carry different minor numbers;
the invariant is "one coherent release set", not "one identical minor" —
mismatched `-sys` minors fail to link.) These are built against the flake's GTK
4.22, cairo 1.18, and pango 1.57. `cairo-rs` enables the `png` feature for the
`#[ignore]`d golden-regeneration dev-aid test.

## Deferred to later phases

The full windowed application is **not** built yet. Deferred:

- `ApplicationWindow` / `HeaderBar` and the libadwaita chrome.
- `DrawingArea` wiring (binding `paint` to a real widget context).
- The glib wake marshalling and the run loop.
- File dialogs (open / save / save-as).
- Theming (querying the OS theme and pushing it into Neovim).
- Mouse and scroll input.
- The `gtk4` (`gtk`) and `libadwaita` (`adw`) crates — deliberately not yet
  dependencies.

## Note on the GUI denylist

The `wolf-core` "no GUI toolkit in the dependency tree" denylist does **not**
apply to this crate — GTK is allowed here. It still holds for `wolf-core`,
because the dependency goes `wolf-gnome` → `wolf-core`, never the reverse, so
GTK never enters core's tree.
