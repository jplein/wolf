# wolf-vt

A safe Rust binding over libghostty-vt, Ghostty's terminal-emulation library: program output goes in as bytes, and the screen comes back out as cells, a cursor and colours. Input goes the other way, through the key, mouse, paste and focus encoders. It is GUI-free, since libghostty-vt parses VT sequences and keeps screen state but draws nothing, and it holds every `unsafe` line and all the link wiring between Wolf and the library.

[`wolf-core`](../wolf-core/README.md) depends on it for its terminal sessions. [Terminal panes](../../specs/terminal-panes.md) is their design, and its [Building libghostty-vt](../../specs/terminal-panes.md#building-libghostty-vt) section owns the pin, the build flags, and why this is a crate of its own.

## Layout

| Path                        | Contents                                                      |
| --------------------------- | ------------------------------------------------------------- |
| [`src/`](src/README.md)     | `lib.rs`, the safe API, and `ffi.rs`, the C declarations.     |
| [`tests/`](tests/README.md) | The binding tests, headless.                                  |
| `build.rs`                  | Cargo's route to the static archive; see [Linking](#linking). |

## Public API surface

Everything is at the crate root as `wolf_vt::…`. [`src/README.md`](src/README.md#librs) has the detail.

| Area      | Entry points                                                                                                                                      |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Emulator  | `Terminal`, `Error`                                                                                                                               |
| Screen    | `Screen`, `Cell`, `Cursor`, `ScreenCursor`, `CursorStyle`, `Underline`, `CellWidth`, `Dirty`, `Rgb`                                               |
| Selection | `Terminal::select`, `clear_selection`, `selection_text`                                                                                           |
| Viewport  | `Terminal::scroll_viewport`, `scrollbar`, `viewport_at_bottom`, `ScrollViewport`, `Scrollbar`                                                     |
| Encoders  | `Terminal::encode_key`, `encode_mouse`, `paste_encode`, `focus_encode`, `KeyInput`, `KeyCode`, `Mods`, `MouseInput`, `MouseAction`, `MouseButton` |
| Effects   | `Terminal::take_effects`, `Effect`                                                                                                                |
| Version   | `version`                                                                                                                                         |

`Terminal` is `Send` and not `Sync`. libghostty-vt's handles have no thread affinity, so a terminal may move to the thread that reads its pty, but the library does not synchronize them, so every mutating call takes `&mut self` and a shared terminal needs a lock around it.

## Linking

Both builds link libghostty-vt's static archive, each its own way.

- Cargo runs `build.rs`, which reads `WOLF_GHOSTTY_VT_DIR`, the install prefix the Linux dev shell exports. It links `lib/libghostty-vt.a` as `static=`, which picks the archive over the shared library installed beside it, and it panics when the variable is unset or names a prefix with no archive.
- Bazel never runs `build.rs`. `//crates/wolf-vt:wolf_vt` takes [`//bazel/ghostty-vt:ghostty_vt`](../../bazel/ghostty-vt/README.md) through `link_deps` rather than `deps`, because rules_rust 0.71 deprecates a C++ library in `deps`.

`Cargo.toml` declares `links = "ghostty-vt"` and no dependencies, so no crates.io crate enters either lock through this crate.

## Building and testing

```sh
nix develop --command cargo test -p wolf-vt   # Linux, where WOLF_GHOSTTY_VT_DIR is set
bazel test //crates/wolf-vt:all              # macOS
```

On macOS, clippy and rustfmt reach the crate through `//tools/lint:clippy_vt` and `:rustfmt_vt`; see [`tools/lint`](../../tools/lint/README.md).
