# Building Wolf

This covers Phase 1: building and testing `wolf-core`, the platform-agnostic
Rust core. The platform front-ends (`wolf-gnome`, `wolf-macos`) are later phases.

## Prerequisites

- **NixOS (Linux).** The dev environment is a NixOS-only `flake.nix` devShell.
- **Nix with flakes enabled** (`experimental-features = nix-command flakes`).
- **[`direnv`](https://direnv.net/) + [`nix-direnv`](https://github.com/nix-community/nix-direnv).**

There are no `darwin` outputs in the flake; macOS work comes in a later phase.

## Activating the dev environment

The repo ships an `.envrc` that runs `use flake` (guarded so it is a no-op off
Linux). Activate it once per checkout:

```sh
direnv allow
```

This enters the flake devShell, which provides:

- the **pinned Rust toolchain** (stable, with `rust-src`, `rust-analyzer`,
  `clippy`, `rustfmt`), and
- a **pinned, hermetic `nvim`** binary, exported as the environment variable
  **`WOLF_NVIM_BIN`**.

`WOLF_NVIM_BIN` is the **only** way core and its tests locate Neovim. Core never
falls back to `PATH` and never scans `/nix/store`. If the variable is unset or
empty, `WolfCore::start` fails with `NvimError::BinNotSet` and the integration
tests panic loudly rather than skip.

Do not use a system `cargo`/`rustc`/`nvim`. The toolchain and the Neovim binary
are pinned by `flake.nix`; all builds and tests must run inside the devShell.

## Building

```sh
cargo build        # builds the whole workspace
```

## Testing

```sh
cargo test         # unit + integration tests for wolf-core
```

The integration tests spawn the **pinned** Neovim (via `WOLF_NVIM_BIN`) as a
real child process — there are no mocks, per the project's testing policy. They
are fully **headless**: `nvim --embed` speaks msgpack-RPC over stdio and needs no
TTY, so the suite is CI-safe. The tests fail loudly (they do not silently pass)
if `WOLF_NVIM_BIN` is unset.

You can also run the headless smoke example, which starts the core, types a
little text, and dumps the resulting grid:

```sh
cargo run -p wolf-core --example grid_dump
```

## The one hard rule and how it is enforced

> **`wolf-core` must compile with no GUI toolkit in its dependency tree.**

This is Wolf's analog of a dependency guard. Enforce it by inspecting
`wolf-core`'s (non-dev) dependency tree and confirming none of the denied crates
appear:

```sh
cargo tree -e no-dev -p wolf-core
```

The denylist (no occurrence of any of these is permitted):

```
gtk gtk4 glib gdk4 cairo cairo-rs pango libadwaita
objc objc2 cocoa core-graphics winit wgpu skia-safe softbuffer
```

If any of these appears in core's tree, the platform boundary has leaked and the
build is wrong.

## Notes

- The captive Neovim is pinned and hermetic from day one (see
  [architecture.md](architecture.md) §8). Phase 1 runs the **bare** pinned binary
  with the repo's `runtime/nvim/init.lua`.
- Full `wrapNeovim` plugin baking — treesitter grammars (markdown /
  markdown_inline) and a Markdown LSP such as `marksman` — is **deferred to a
  later phase**. Phase 1 relies on Neovim's built-in markdown syntax highlighting.
