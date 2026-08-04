# wolf-core

The platform-agnostic Rust core of Wolf. Core contains everything Neovim-related and draws no pixels: it spawns a hermetic, pinned `nvim --embed`, attaches as a multigrid UI, folds the entire `redraw` event stream into a render-state snapshot, surfaces a Neovim-to-shell layout-intent channel for native split panes, accepts abstract input events, and signals the front-end through a single wake callback when a new frame is ready.

**Read [DECISIONS.md](DECISIONS.md) before changing anything in this crate.** It holds the one hard rule about the dependency tree, the threading and locking model, and the constraints a refactor would otherwise undo.

For workspace context see [`architecture.md`](../../architecture.md) and [`build.md`](../../build.md).

## Layout

| Directory                                 | Contents                                               |
| ----------------------------------------- | ------------------------------------------------------ |
| [`src/`](src/README.md)                   | The crate's modules, one entry each.                   |
| [`src/markdown/`](src/markdown/README.md) | Preview themes and assets.                             |
| [`tests/`](tests/README.md)               | Integration tests, most against a real Neovim process. |
| `examples/`                               | `grid_dump`, a headless smoke dump.                    |

## Public API surface

Everything below is re-exported from the crate root as `wolf_core::…`. The owning module's entry in [`src/README.md`](src/README.md) has the detail.

| Area               | Entry points                                                                  |
| ------------------ | ----------------------------------------------------------------------------- |
| Lifecycle          | `WolfCore`, `CoreConfig`, `CoreError`, `SnapshotReadGuard`                    |
| Render state       | `Snapshot`, `SharedSnapshot`, `Grid`, `Cell`, `Cursor`, `WinInfo`, `Viewport` |
| Wake               | `WakeFn`                                                                      |
| Input              | `InputEvent`, `KeyEvent`, `MouseEvent`, `ScrollEvent`, `key_to_notation`      |
| Layout intents     | `LayoutIntent`, `SplitOrientation`, `FocusDir`                                |
| Commands           | `Command`, `CommandSpec`, `COMMAND_SPECS`, `MenuGroup`                        |
| File picker        | `FileSearch`, `FileEntry`, `FileSearchReadGuard`, `rank_matches`, `MruStore`  |
| Content search     | `SearchSession`, `SearchOptions`, `SearchMatch`, `project`, `context_window`  |
| Buffer picker      | `rank_buffers`, `BufInfo`, `BufferRow`                                        |
| Command palette    | `rank_palette`, `PaletteRow`, `CommandMru`                                    |
| File tree          | `children_of`, `ancestors_to_reveal`, `rel_path_for`, `TreeIndex`             |
| Navigation history | `NavHistory`, `PaneHistory`, `NavTarget`, `MenuRow`                           |
| Markdown preview   | `render_html`, `theme_css`, `PreviewTheme`, `WidthMode`, `BrowserPreview`     |
| Font licences      | `FontLicense`, `FONT_LICENSES`, `FONT_LICENSE_NOTICES`                        |
| Liveness           | `Liveness`                                                                    |
| Preferences        | `WolfConfig`, `load_or_create`, `preferences_dir`, `Appearance`               |
| Persisted state    | `StateStore`, `WolfState`, `WindowState`, `TabState`, `PaneNode`              |
| Logging            | `init_tracing`                                                                |

One public entry point is reached through its module rather than the root: `state::rebuild_layout_spec` renders a saved `PaneNode` into the JSON `WolfCore::rebuild_layout` takes, so a front-end restoring a pane layout never hand-rolls that document. See [`src/README.md`](src/README.md#staters) for the module entry and [DECISIONS.md](DECISIONS.md) for the contract it shares with the Swift renderer.

## Building and testing

On Linux and Nix, core builds with Cargo:

```sh
cargo test -p wolf-core                      # spawns a real, pinned nvim
cargo run -p wolf-core --example grid_dump   # headless smoke dump
```

Both need an active flake devShell (`direnv allow`) so `WOLF_NVIM_BIN` is set. See [`tests/README.md`](tests/README.md).

On macOS core builds with Bazel instead, as `//crates/wolf-core:wolf_core`, taking its dependencies from the same `Cargo.toml` and `Cargo.lock` through `rules_rust`'s `crate_universe`. See [`build.md`](../../build.md) for the macOS build and repin workflow.

Bazel does not discover test files. A new `tests/*.rs` needs a matching `rust_test` target in `BUILD.bazel` or it runs only under `cargo test`, making it a silent macOS-only gap.
