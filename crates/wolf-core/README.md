# wolf-core

The platform-agnostic Rust core of Wolf. Core contains everything Neovim-related and draws no pixels: it spawns a hermetic, pinned `nvim --embed`, attaches as a multigrid UI, folds the entire `redraw` event stream into a render-state snapshot, surfaces a Neovim-to-shell layout-intent channel for native split panes, accepts abstract input events, and signals the front-end through a single wake callback when a new frame is ready. It also owns the terminal sessions, each a shell in a pty emulated through [`wolf-vt`](../wolf-vt/README.md), which Neovim shows as placeholder windows and the front-end paints from their own frames.

The crate also holds the [`wolf` command](../../specs/cli-and-file-open.md) and the channel the command reaches a running Wolf through, with the rules that place the path it opens.

**Read [wolf-core-decisions.md](../../notes/wolf-core-decisions.md) before changing anything in this crate.** It holds the one hard rule about the dependency tree, the threading and locking model, and the constraints a refactor would otherwise undo.

For workspace context see [`architecture.md`](../../specs/architecture.md) and [`build.md`](../../build.md).

## Layout

| Directory                                 | Contents                                               |
| ----------------------------------------- | ------------------------------------------------------ |
| [`src/`](src/README.md)                   | The crate's modules, one entry each.                   |
| [`src/markdown/`](src/markdown/README.md) | Preview themes and assets.                             |
| [`src/bin/`](src/README.md#binwolfrs)     | The `wolf` command.                                    |
| [`tests/`](tests/README.md)               | Integration tests, most against a real Neovim process. |
| `examples/`                               | `grid_dump`, a headless smoke dump.                    |

## Public API surface

Everything below is re-exported from the crate root as `wolf_core::…`. The owning module's entry in [`src/README.md`](src/README.md) has the detail.

| Area               | Entry points                                                                                                                                  |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Lifecycle          | `WolfCore`, `CoreConfig`, `CoreError`, `SnapshotReadGuard`                                                                                    |
| Render state       | `Snapshot`, `SharedSnapshot`, `Grid`, `Cell`, `Cursor`, `WinInfo`, `Viewport`                                                                 |
| Wake               | `WakeFn`                                                                                                                                      |
| Input              | `InputEvent`, `KeyEvent`, `MouseEvent`, `ScrollEvent`, `key_to_notation`                                                                      |
| Layout intents     | `LayoutIntent`, `SplitOrientation`, `FocusDir`                                                                                                |
| Terminal sessions  | `Terminal*` types, `TERMINAL_*`, `terminfo_dir_from_env`, `TERMINFO_DIR_ENV`                                                                  |
| Tab IDs            | `TabId`, `WOLF_TAB_ID_ENV`                                                                                                                    |
| Opening files      | `OpenTarget`, `project_directory`, `choose_pane`, `PaneChoice`, `PanePlacement`                                                               |
| Open channel       | `OpenListener`, `OpenClient`, `OpenRequest`, `OpenReply`, `OpenWait`, `TakenRequest`, `OpenChannelError`, `open_socket_path`, `OPEN_SOCKET_*` |
| Commands           | `Command`, `CommandSpec`, `COMMAND_SPECS`, `MenuGroup`, `CloseTabAnswer`, `UnsavedAnswer`                                                     |
| File picker        | `FileSearch`, `FileEntry`, `FileSearchReadGuard`, `rank_matches`, `MruStore`                                                                  |
| Content search     | `SearchSession`, `SearchOptions`, `SearchMatch`, `project`, `context_window`                                                                  |
| Buffer picker      | `rank_buffers`, `BufInfo`, `BufferRow`                                                                                                        |
| Command palette    | `rank_palette`, `PaletteRow`, `CommandMru`                                                                                                    |
| File tree          | `children_of`, `ancestors_to_reveal`, `rel_path_for`, `TreeIndex`                                                                             |
| Navigation history | `NavHistory`, `PaneHistory`, `NavTarget`, `MenuRow`                                                                                           |
| Markdown preview   | `render_html`, `theme_css`, `PreviewTheme`, `WidthMode`, `BrowserPreview`                                                                     |
| Font licences      | `FontLicense`, `FONT_LICENSES`, `FONT_LICENSE_NOTICES`                                                                                        |
| Library licences   | `LibraryLicense`, `LIBRARY_LICENSES`, `LIBRARY_LICENSE_NOTICES`                                                                               |
| Liveness           | `Liveness`                                                                                                                                    |
| Preferences        | `WolfConfig`, `load_or_create`, `preferences_dir`, `Appearance`, `GtkTitlebarStyle`                                                           |
| Persisted state    | `AppStateStore`, `PROJECT_STATE_LIMIT`, `state_dir`, `WindowCursor`                                                                           |
| Logging            | `init_tracing`                                                                                                                                |

`app_state::…` is reached through its module rather than the root: the model `AppStateStore` saves, with the rule that picks a directory's project state and the restore helpers, among them `tab_rebuild_spec`, which renders a saved tab into the JSON `WolfCore::rebuild_layout` takes, so a front-end restoring a pane layout never hand-rolls that document ([entry](src/README.md#app_staters)). [wolf-core-decisions.md](../../notes/wolf-core-decisions.md#the-rebuild-layout-wire-format) has the contract it shares with `init.lua`.

## Building and testing

On Linux and Nix, core builds with Cargo:

```sh
cargo test -p wolf-core                      # spawns a real, pinned nvim
cargo run -p wolf-core --example grid_dump   # headless smoke dump
```

Both need an active flake devShell (`direnv allow`) so `WOLF_NVIM_BIN` and `WOLF_GHOSTTY_VT_DIR` are set. See [`tests/README.md`](tests/README.md).

On macOS core builds with Bazel instead, as `//crates/wolf-core:wolf_core`, taking its dependencies from the same `Cargo.toml` and `Cargo.lock` through `rules_rust`'s `crate_universe`, and the example runs as `bazel run //crates/wolf-core:grid_dump`. See [`build.md`](../../build.md) for the macOS build and repin workflow.

The `wolf` command builds as `//crates/wolf-core:wolf` under Bazel and as the `wolf` binary under Cargo; [`build.md`](../../build.md#the-wolf-command) has how to build and run it on each platform.

Bazel does not discover test files. A new `tests/*.rs` needs a matching `rust_test` target in `BUILD.bazel` or it runs only under `cargo test`, making it a silent macOS-only gap.
