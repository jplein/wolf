# wolf-gnome

The GTK 4 and libadwaita front-end: a runnable single-window Linux application that consumes [`wolf-core`](../wolf-core/README.md) as an ordinary Rust crate dependency and paints the Neovim grid with Cairo and Pango.

**Read [DECISIONS.md](DECISIONS.md) before changing anything in this crate.** In particular, this crate deliberately has no Bazel target, and that absence is load-bearing.

## Layout

| Directory                   | Contents                                            |
| --------------------------- | --------------------------------------------------- |
| [`src/`](src/README.md)     | The application and its pure seams, one entry each. |
| [`tests/`](tests/README.md) | Integration tests, including the headless smoke.    |

## Running

Everything goes through the Nix devShell, which supplies the GTK toolchain and exports `WOLF_NVIM_BIN`:

```sh
nix develop --command cargo run -p wolf-gnome
nix develop --command cargo test -p wolf-gnome
```

After editing `flake.nix`, use `nix develop --command` explicitly: an already-loaded direnv environment is stale and has no GTK.

The binary takes no command-line arguments. Files are opened through Neovim — the picker and sidebar send `InputEvent::Open`, which core performs as `:e`.
