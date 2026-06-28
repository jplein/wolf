# wolf-gnome/tests

Tests for the seams and the windowed application. They split into **headless**
checks (no display, no nvim), one **real-Neovim** end-to-end round-trip (no mocks,
per project policy), and one **opt-in headless-compositor** smoke test that
launches the real window. Everything runs through `nix develop --command` so the
GTK toolchain, the pinned font, and `WOLF_NVIM_BIN` are present:

```sh
nix develop --command cargo test -p wolf-gnome
```

The default run does **not** include the smoke test (it is `#[ignore]`d); see
[the smoke test section](#smoke_headlessrs--windowed-app-opt-in-real-compositor).

## `grid_size_for` unit tests (in `src/geometry.rs`, headless)

The pure pixel-to-grid geometry is unit-tested **inside its own module** (a
`#[cfg(test)] mod tests` in [`src/geometry.rs`](../src/geometry.rs)), needing no
GTK, display, or nvim. A table-driven `grid_size_for_table` covers the typical
exact-division case (`800×592` with `8×16` cells → `100×37`), flooring of partial
cells (`17×17` → `2×1`), the min-1 clamp for tiny/zero/negative dims, the
whole-result collapse to `(1, 1)` for degenerate cell sizes (zero/negative/NaN/
infinite, including a single-axis NaN), and the upper clamp of `i32::MAX` dims to
`u16::MAX`. A second test, `grid_size_for_independent_axes`, proves cols and rows
are computed separately (wide-but-short and narrow-but-tall grids).

## `cursor.rs` unit tests (in `src/cursor.rs`, headless)

The pure cursor geometry and color logic is unit-tested **inside its own module**
(a `#[cfg(test)] mod tests` in [`src/cursor.rs`](../src/cursor.rs)), needing no GTK,
display, or nvim. A table-driven `cursor_rect_table` covers the geometry per shape:
`Block` always returns the full cell (ignoring `cell_percentage`); `Vertical` is a
left bar of full height whose width is `cell_percentage` of `cell_w` (default
`0.25`), floored to `MIN_BAR_PX` for tiny percentages and capped at `cell_w` for
out-of-range ones; `Horizontal` is the analogous bottom bar against `cell_h`
(anchored to the bottom edge). Deliberately non-square cells and a non-zero origin
keep width/height math and offsets distinguishable. `named_constants_match_contract`
pins `MIN_BAR_PX` / `DEFAULT_CELL_PCT` / `PCT_DENOM`.

A second group covers `cursor_colors` over hand-built `Snapshot`s: the **honor**
path (a mode cursor highlight with distinct fg + bg → `(bg, fg)`, including with
editor defaults set); the **inversion** path (bg-only, fg-only, `attr_id` `None`,
out-of-range `mode_idx`, and empty `mode_infos` all fall through to `(cell_fg,
cell_bg)` without panicking); the **guard** cases where a degenerate honored pair
must NOT be honored (fill equal to the underlying cell background, or fill equal to
glyph) and instead inverts; and the **fallback** case where unset cell colors reach
the white/black `FALLBACK_FG` / `FALLBACK_BG`.

## `gdk_key.rs` — input translation (headless, table-driven)

Table-driven unit tests for `wolf_gnome::gdk_key`, layout-independent: assertions
cover only letter casing and named keys, never shifted-punctuation unicode (which
is keyboard-layout dependent and would be flaky). These need no display server
and no nvim — `gdk::Key` constants are compile-time and printable keyvals are
built via the local `key_from_char` helper.

Cases cover: the Enter/Return family (incl. `KP_Enter`), `ISO_Left_Tab` + SHIFT →
`Tab { shift: true }`, arrows, `Home`/`End`, `PageUp`/`PageDown`, `F1`/`F5`/`F12`,
printable chars with the shift contract (`'a'` and `'A'` both → `Char(..)` with
`shift: false`, `'<'`), modifiers on named keys (`Ctrl+Left`,
`Ctrl+Shift+Left`), modifiers on printables (`Super+s`, `Alt+x`, `Ctrl+a` carries
the base letter + ctrl, never a raw control char), and the `None` cases
(`Shift_L`, `Control_L`, `Super_L`, `Caps_Lock`).

## `paint_golden.rs` — paint (headless, offscreen)

Builds a deterministic 4×2 `Snapshot` via core's public constructors
(`Snapshot::new`, `HighlightTable::set_defaults`/`define`, `Grid::set_cell`),
including one defined `hl_id` with distinct fg/bg and a wide-trailing empty-text
cell. It measures `CellMetrics` against a throwaway 1×1 probe surface, sizes a
real offscreen `ImageSurface` (ARGB32) to exactly the grid, paints, and samples
pixels.

**Primary gate — structural ARGB center-sampling** (always on; deliberately
*not* a whole-image hash, which is too brittle across freetype/fontconfig):

- Known background fills are rasterizer-independent, so the center of the
  defined-bg empty cell and a default-bg cell are asserted for **exact** equality.
- The empty wide-trailing cell's interior is sampled at several inset points and
  must be background-only (verifying the "skip the glyph" path).
- The glyph cell's rect must contain at least one non-background pixel (the glyph
  was actually drawn).

Pixel reads assume little-endian ARGB32 (in-memory byte order `B, G, R, A`),
which holds on the x86_64 Linux target.

**Dev aid (not part of the gate)** — `regenerate_png_golden` is `#[ignore]`d; it
writes `tests/testdata/paint_golden.png` via `ImageSurface::write_to_png` (which
needs the `cairo-rs` `png` feature). Run it explicitly:

```sh
nix develop --command cargo test -p wolf-gnome --test paint_golden \
    -- --ignored regenerate_png_golden
```

Font determinism relies on the pinned `DejaVu Sans Mono` exposed via
`FONTCONFIG_FILE` (set by the flake devShell from
[`testdata/fonts.conf`](../testdata/fonts.conf)).

## `roundtrip_nvim.rs` — end-to-end (real Neovim)

Exercises the full input seam against a real, hermetic Neovim (no mocks):
`gdk_key` translates GDK keyvals → `InputEvent::Key` → `WolfCore` → nvim, then the
snapshot is asserted. It enters insert mode, types `"Hello"` (the leading `H` is
fed as the already-uppercased keyval *with* the SHIFT mask, exercising the
shift-glyph contract end-to-end), leaves insert mode with `Escape`, waits for and
drains flushes, and asserts row 0 contains the word. Requires `WOLF_NVIM_BIN`
(`common::require_nvim_bin` fails loud if unset).

## `smoke_headless.rs` — windowed app (opt-in, real compositor)

A single `#[test] #[ignore]` (`window_renders_typed_text_under_cage`) that proves
the whole stack actually draws a window. It is opt-in because it needs a compositor
and an OCR toolchain a normal `cargo test` will not have. Run it explicitly:

```sh
nix develop --command cargo test -p wolf-gnome -- --ignored
```

Mechanism: it locates the built binary via `CARGO_BIN_EXE_wolf-gnome` and runs the
whole interactive scenario as one `sh -c` string **inside `cage`** (the headless
wlroots compositor) so that `wtype` and `grim` inherit cage's `WAYLAND_DISPLAY`
(cage exports it only to its child process tree). The scenario launches the app in
the background, polls (not a fixed sleep) until the window maps, types `ihello`
(insert mode + the word) and `Escape` via `wtype`, captures the live window with
`grim` to a PNG under `XDG_RUNTIME_DIR`, then OCRs it on the host with `tesseract
… stdout --psm 11` ("sparse text" mode — the default page-segmentation mode returns
empty on the sparse light-on-dark editor grid) and asserts the lowercased output
loosely `contains("hello")`. The app is launched with `WOLF_FONT_PT=28` so glyphs
are large enough to OCR.

Required toolchain + env (all from the flake devShell smoke toolchain): `cage`,
`wtype`, `grim`, `tesseract` on `PATH`; `WOLF_NVIM_BIN` set; and a set, writable
`XDG_RUNTIME_DIR`. Per project policy `require_smoke_env` **fails loud** (not a
silent skip) on any missing precondition, since absence means the smoke toolchain
was not provisioned. A stale screenshot is removed first so a previous run cannot
make the test pass.

**NOTE:** the smoke test asserts only the typed `hello`, not the cursor; cursor
geometry and color are covered by the pure `cursor.rs` unit tests above plus visual
confirmation (OCR is too fragile to couple to the cursor).

## `common/mod.rs`

Local test helpers (Cargo cannot share a `tests/common` module across crates, so
these mirror `wolf-core`'s): `require_nvim_bin()` (fail-loud `WOLF_NVIM_BIN`
check), `init_lua_path()` (resolves `../../runtime/nvim/init.lua` from this
crate's `CARGO_MANIFEST_DIR`), `config(cols, rows)`, and `wake_channel()` →
`(WakeFn, FlushWaiter)` with `wait_for_flush(timeout)` / `drain()` and
`FLUSH_TIMEOUT = 10s`.

## The `key_from_char` helper

Both `gdk_key.rs` and `roundtrip_nvim.rs` define a small `key_from_char(c: char)
-> gdk::Key` helper, because gdk4-rs has no `Key::from_unicode`. It builds the
keyval exactly as GDK would from a Unicode scalar:

```rust
use gdk::glib::translate::from_glib;
fn key_from_char(c: char) -> gdk::Key { unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) } }
```

This is test-side only and does not change `gdk_key`'s contract: printables still
carry the exact glyph, and `gdk_key` reads `keyval.to_unicode()` and sets
`shift: false` for `Key::Char`.
