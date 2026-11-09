# wolf-vt/tests

Integration tests for [`wolf-vt`](../README.md), against the real linked libghostty-vt. They need no Neovim, no pty and no runfiles.

```sh
nix develop --command cargo test -p wolf-vt   # Linux
bazel test //crates/wolf-vt:binding           # macOS
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
