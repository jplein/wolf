# wolf-gnome/tests

Tests for the two Tier-1 seams. They split into **headless** checks (no display,
no nvim) and one **real-Neovim** end-to-end round-trip (no mocks, per project
policy). Everything runs through `nix develop --command` so the GTK toolchain,
the pinned font, and `WOLF_NVIM_BIN` are present:

```sh
nix develop --command cargo test -p wolf-gnome
```

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
