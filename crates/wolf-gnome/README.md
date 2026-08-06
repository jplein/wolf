# wolf-gnome

The GTK 4 and libadwaita front-end: a runnable single-window Linux application that consumes [`wolf-core`](../wolf-core/README.md) as an ordinary Rust crate dependency and paints the Neovim grid with Cairo and Pango. A Markdown preview pane is the one surface it does not paint — those pixels come from a `webkit6::WebView`, which is why the devShell supplies WebKitGTK.

**Read [DECISIONS.md](DECISIONS.md) before changing anything in this crate.** In particular, this crate deliberately has no Bazel target, and that absence is load-bearing.

## Layout

| Directory                   | Contents                                            |
| --------------------------- | --------------------------------------------------- |
| [`src/`](src/README.md)     | The application and its pure seams, one entry each. |
| [`tests/`](tests/README.md) | Integration tests, including the headless smoke.    |

## Entry point

`src/main.rs` forwards the exit code from `app::run`, which builds the `adw::Application` and constructs the window on activate. The crate is also a library: every decision expressible as a function of values lives in a seam that needs no display server, and the binary is the widget glue over those seams.

The binary takes no command-line arguments. The picker, the sidebar, the primary menu's native `gtk::FileDialog`s and Neovim itself all open files the same way, through `InputEvent::Open`, which core performs as `:e`. A project-search result row opens a location rather than a file, through `InputEvent::OpenAt`, which core performs as `:edit` followed by `nvim_win_set_cursor` in one input-pump step. Saving is `:w` or `:saveas` from inside Neovim.

## Running

Everything goes through the Nix devShell, which supplies the GTK toolchain and exports `WOLF_NVIM_BIN` and `WOLF_RG_BIN`:

```sh
nix develop --command cargo run -p wolf-gnome
nix develop --command cargo test -p wolf-gnome
```

After editing `flake.nix`, use `nix develop --command` explicitly: an already-loaded direnv environment is stale and has no GTK.

Running needs a Wayland or X11 session, or the headless compositor the smoke tests launch. Three environment variables matter. `WOLF_FONT_PT` overrides the configured `font-size` and wins over it, as it does on macOS; `WOLF_CONFIG_DIR` overrides where `wolf-config.yaml` is read and created, and belongs to [`config.rs`](../wolf-core/src/README.md#configrs); `WOLF_STATE_DIR` overrides where `state.json`, the MRU databases and the per-project swap directories are kept.

## Testing

`cargo test -p wolf-gnome` runs the whole suite, including the end-to-end smokes that drive the real window under a headless compositor — which is why it is slow. The authoritative full check is the Linux verification gate in [`build.md`](../../build.md).

What each target covers, what the smokes prove, and what is deliberately left unproven are in [`tests/README.md`](tests/README.md).
