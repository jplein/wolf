# wolf-gnome

The GTK 4 and libadwaita front-end: a runnable multi-window Linux application that consumes [`wolf-core`](../wolf-core/README.md) as an ordinary Rust crate dependency and paints the Neovim grid with Cairo and Pango. Each window carries a captive Neovim of its own. A Markdown preview pane is the one surface it does not paint — those pixels come from a `webkit6::WebView`, which is why the devShell supplies WebKitGTK.

**Read [wolf-gnome-decisions.md](../../notes/wolf-gnome-decisions.md) before changing anything in this crate.** In particular, this crate deliberately has no Bazel target, and that absence is load-bearing.

## Layout

| Directory                   | Contents                                            |
| --------------------------- | --------------------------------------------------- |
| [`src/`](src/README.md)     | The application and its pure seams, one entry each. |
| [`tests/`](tests/README.md) | Integration tests, including the headless smoke.    |

## Entry point

`src/main.rs` forwards the exit code from `app::run`, which builds the `adw::Application` and, on activate, builds the process-scoped coordinator and asks it to restore the saved session — one window per saved window, or one empty-state window when there is nothing to restore. The crate is also a library: every decision expressible as a function of values lives in a seam that needs no display server, and the binary is the widget glue over those seams.

The binary takes no command-line arguments. The picker, the sidebar, the primary menu's native `gtk::FileDialog`s and Neovim itself all open files the same way, through `InputEvent::Open`, which core performs as `:e`. A project-search result row opens a location rather than a file, through `InputEvent::OpenAt`, which core performs as `:edit` followed by `nvim_win_set_cursor` in one input-pump step. Saving goes the same way, through `InputEvent::Save` and `InputEvent::SaveAs`, from the primary menu's Save and Save As items, from `Ctrl+Shift+S`, and from `:w` or `:saveas` typed inside Neovim.

## Running

Everything goes through the Nix devShell, which supplies the GTK toolchain and exports `WOLF_NVIM_BIN` and `WOLF_RG_BIN`:

```sh
nix develop --command cargo run -p wolf-gnome
nix develop --command cargo test -p wolf-gnome
```

After editing `flake.nix`, use `nix develop --command` explicitly: an already-loaded direnv environment is stale and has no GTK.

Running needs a Wayland or X11 session, or the headless compositor the smoke tests launch. Five environment variables matter. `WOLF_FONT_PT` overrides the configured `font-size` and wins over it, as it does on macOS; `WOLF_CONFIG_DIR` overrides where `wolf-config.yaml` is read and created, and belongs to [`config.rs`](../wolf-core/src/README.md#configrs); `WOLF_STATE_DIR` overrides where `state.json`, the MRU databases and the per-project swap directories are kept; `WOLF_INIT_LUA` names the captive `runtime/nvim/init.lua` outright, ahead of both paths the binary would otherwise resolve, and is deliberately not checked for existence, so a typo fails loudly instead of falling through to another file.

`WOLF_READY_FILE` belongs to the smoke harness: the binary writes its pid there once its launch windows accept input ([wolf-gnome-decisions.md](../../notes/wolf-gnome-decisions.md#the-readiness-file)).

## Packaging

`nix build .#wolf-gnome` produces the installed Linux application: the release binary, wrapped so it supplies its own Neovim, ripgrep, treesitter trees and font configuration, beside a captive `init.lua` with the vendored `orlando` and `lighthouse` trees as siblings, a desktop entry, the hicolor icon set and both editor font families. The commands, the output's layout, how a NixOS configuration consumes the flake and the one script that produces evidence about the artifact are all [`build.md`](../../build.md)'s.

## Testing

`cargo test -p wolf-gnome` runs the whole suite, including the end-to-end smokes that drive the real window under a headless compositor — which is why it is slow. The authoritative full check is the Linux verification gate in [`build.md`](../../build.md).

What each target covers, what the smokes prove, and what is deliberately left unproven are in [`tests/README.md`](tests/README.md).
