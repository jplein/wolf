# wolf-vt/tests

Integration tests for [`wolf-vt`](../README.md), against the real linked libghostty-vt. They need no Neovim, no pty and no runfiles.

```sh
nix develop --command cargo test -p wolf-vt   # Linux
bazel test //crates/wolf-vt:all              # macOS: :binding, :render, :encoders, :effects
```

## Adding a test file

Cargo discovers every `tests/*.rs`. **Bazel does not.** A new file needs a matching `rust_test` in [`../BUILD.bazel`](../BUILD.bazel), a copy of `:binding`, or it runs under `cargo test` alone, which makes it a silent macOS-only gap. A test here links the static archive and nothing else, so its target takes no `data`, no env and no `no-sandbox` tag.

## Files

#### `binding.rs`

**Covers**

- `version()` is `1.0.0`, the pinned release.
- Text with a multi-byte character landing in cells and moving the cursor, and a CR LF moving it to the next row.
- A cursor-position escape, and `DEC` mode 25 hiding and showing the cursor.
- A true-colour foreground, a palette background read back as `palette()[1]`, an unstyled cell's `None`, and the default colours read back as set.
- A resize reaching the grid, `size()` and `screen()`, keeping the text written before it.
- `Terminal: Send`, checked at compile time.

**Gotchas**

- The version test is what fails on the side a one-sided bump leaves behind ([Building libghostty-vt](../../../specs/terminal-panes.md#building-libghostty-vt)), so its expected version is re-typed, never read from either pin.

#### `effects.rs`

**Covers**

- A device status report queuing one `WritePty` with the cursor position, and a second `take_effects()` coming back empty.
- `OSC 7` queuing `PwdChanged` with `pwd()` returning the URI as sent, and `OSC 2` queuing `TitleChanged` with `title()` returning it.
- Mode 2026 set and reset queuing `RenderHold(true)` and `RenderHold(false)`, and a resize during a hold queuing `RenderHold(false)`.

#### `encoders.rs`

**Covers**

- Keys encoded from the terminal's modes: Enter, Ctrl+C, an arrow in normal and application cursor mode, alt+`x` as `ESC x`, and Escape under the kitty keyboard protocol.
- `SGR` mouse reports at a pixel position, converted with the resized cell size, and nothing with tracking off.
- Bracketed and plain paste, an ESC in pasted text never passing through, and both focus reports.

**Gotchas**

- The alt case is the one that fails on macOS alone if the encoder stops treating alt as alt.

#### `render.rs`

**Covers**

- Every `SGR` attribute and underline style reaching its cell flag, a wide character's `Wide` cell and `SpacerTail`, and the `SpacerHead` one leaves when it wraps.
- Per-row dirty state: `Partial` naming only the written row, `Clean` on a second read, `Full` after a resize.
- The six `DECSCUSR` cursor styles, and the cursor absent while scrolled out of view.
- The cursor colour, a palette change recolouring an `SGR 31` cell, and the default colours, each read back.
- A selection's text and its per-row columns, scrolling through scrollback with the scrollbar following, and the `DEC` modes core routes input by.
