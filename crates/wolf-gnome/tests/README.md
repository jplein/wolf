# wolf-gnome/tests

Integration tests for [`wolf-gnome`](../README.md). This crate is Cargo-only by design — it has no Bazel target, so there is no `rust_test` wiring to keep in sync, and `cargo test` discovers every file here.

```sh
nix develop --command cargo test -p wolf-gnome
```

The devShell is not optional: it supplies GTK, exports `WOLF_NVIM_BIN`, and pins the font stack through a generated `FONTCONFIG_FILE`, which is what makes Pango measurement and Cairo rasterization reproducible enough to assert on pixels.

## Files

#### `gdk_key.rs`

**Covers**

- Table-driven `gdk_key(keyval, state) -> Option<KeyEvent>`: named keys, arrows, F-keys, the printable shift contract (`shift` is always false for `Key::Char`; the keyval carries the case), modifiers on both named and printable keys, and bare modifier/lock keys yielding `None`. Runs with no display and no nvim — `gdk::Key` constants are compile-time and printables are built with the local `key_from_char` helper, since gdk4-rs has no `Key::from_unicode`.

**Gotchas**

- Assert only on letter casing and named keys. Shifted-punctuation unicode (that Shift+`,` yields `<`) is keyboard-layout dependent and would be flaky.

#### `paint_golden.rs`

**Covers**

- `paint` onto an offscreen ARGB32 `ImageSurface` from a hand-built 4x2 `Snapshot`: exact equality on known background fills, background-only interiors for a wide-trailing empty-text cell (the skip-glyph path), and at least one non-background pixel in a glyph cell. Needs no display and no nvim, but does need the pinned DejaVu Sans Mono via `FONTCONFIG_FILE`.

**Gotchas**

- Deliberately not a whole-image hash, which is too brittle across freetype/fontconfig versions. Pixel reads assume little-endian ARGB32, in-memory byte order `B, G, R, A`.
- The PNG golden is a dev aid nothing asserts against; regenerate it with `nix develop --command cargo test -p wolf-gnome --test paint_golden -- --ignored regenerate_png_golden`, which needs the `png` feature on the `cairo` dependency.

#### `roundtrip_nvim.rs`

**Covers**

- The full input seam against a real Neovim: `gdk_key` to `InputEvent::Key` to `WolfCore` to snapshot. It types `Hello` in insert mode, feeding the leading `H` as the already-uppercased keyval plus the `SHIFT` modifier mask exactly as GDK reports it, and asserts row 0 contains the word — the shift-glyph contract end to end.

**Gotchas**

- Requires `WOLF_NVIM_BIN`; `common::require_nvim_bin` panics rather than skipping. No display needed.
- Leaving insert mode emits several follow-up flushes, so the test drains and sleeps a settle window before reading the snapshot. A new case that reads a snapshot needs the same treatment.

#### `smoke_headless.rs`

**Covers**

- Nine `#[ignore]`d end-to-end smokes that launch the built binary under `cage`, drive it with `wtype`, screenshot the live window with `grim`, and OCR with `tesseract --psm 11`: text rendering, two picker scenarios, tree-sidebar population, and five split-pane scenarios. Run with `nix develop --command cargo test -p wolf-gnome -- --ignored`.

**Gotchas**

- Needs `cage`, `wtype`, `grim`, and `tesseract` on `PATH` plus `WOLF_NVIM_BIN` and a writable `XDG_RUNTIME_DIR`; `require_smoke_env` fails loud rather than skipping, because a missing tool means the smoke toolchain was not provisioned.
- Every test holds `CAGE_LOCK` for its whole body: two cage instances started concurrently contend for the same Wayland socket and both fail.
- A new scenario must park the block cursor off the asserted row (a normal-mode `o` then Escape) — the inverted cursor block corrupts OCR of the cell it covers — and must keep `--psm 11`, since the default page-segmentation mode returns empty on the sparse editor grid.

#### `common/mod.rs`

**Covers**

- Shared helpers for this crate's integration tests: `require_nvim_bin`, `init_lua_path`, `fresh_cwd`, `config`/`config_in`, and `wake_channel` returning a `WakeFn` paired with a `FlushWaiter` (`wait_for_flush`, `drain`).

**Gotchas**

- These duplicate `wolf-core`'s equivalents because Cargo cannot share a `tests/common` module across crates; they are kept in sync by hand.
- `fresh_cwd` directories are not auto-deleted — the cwd is nvim's working dir and file-search root, so it must outlive the session.

#### `../testdata/fonts.conf`

Template fontconfig that exposes only the pinned DejaVu font directory and deliberately omits every host font dir, so Pango measurement and Cairo rasterization are reproducible across machines. The flake's `shellHook` substitutes `@DEJAVU_FONTS@` with the Nix store path of `pkgs.dejavu_fonts`, writes the result to a temp file, and points `FONTCONFIG_FILE` at that copy — so this file is never consumed directly. Edit it here; the generated copy is rebuilt on each `nix develop`.

#### `testdata/paint_golden.png`

A write-only render of `paint_golden.rs`'s fixture snapshot, kept for eyeballing a renderer change. No test reads it, so it can never fail the suite and does not have to be updated when `paint` changes; refresh it with the `regenerate_png_golden` command above when a visual diff is useful.
