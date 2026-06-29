# wolf-core

The platform-agnostic Rust core of Wolf, a Markdown editor that embeds Neovim as
its editing engine. Core contains everything Neovim-related and draws no pixels:
it spawns a hermetic, pinned `nvim --embed`, attaches as an `ext_linegrid` +
`ext_multigrid` UI, folds the entire `redraw` event stream into a multigrid
render-state snapshot, surfaces a Neovim→shell layout-intent channel (for native
split panes), accepts abstract input events, and signals the front-end (via a
single wake callback) when a new frame is ready.

See the workspace docs for context: [`architecture.md`](../../architecture.md)
(esp. §8 and the core↔front-end contract) and [`build.md`](../../build.md).

## The one hard rule

> **`wolf-core` must compile with no GUI toolkit in its dependency tree.**

Enforced by a `cargo tree` denylist check:

```sh
cargo tree -e no-dev -p wolf-core
```

No occurrence of any of these is permitted:

```
gtk gtk4 glib gdk4 cairo cairo-rs pango libadwaita
objc objc2 cocoa core-graphics winit wgpu skia-safe softbuffer
```

The file picker adds two dependencies — `rusqlite` (with the `bundled` feature,
which vendors SQLite via `cc`) for the MRU cache and `ignore` (ripgrep's pure-Rust
walker) for the traversal. Both are deliberately GUI-free: they pull **no** GUI
toolkit into core's tree, so the denylist above is unchanged (the command and the
denied-crate list stay exactly as written) and stays clean. The flake also needs
no change (no system SQLite/libgit2 to provision).

## Building under Bazel (macOS)

On macOS `wolf-core` builds with Bazel (via bazelisk) rather than Cargo; the target label is `//crates/wolf-core:wolf_core`. Its dependencies come from the same `Cargo.toml`/`Cargo.lock` as the Cargo build, consumed through `rules_rust`'s `crate_universe` (with `rules_rust` downloading a pinned `rustc`). The one hard rule above still holds under Bazel: `wolf-gnome` is given no Bazel target, so no GUI toolkit enters `wolf-core`'s Bazel dependency graph. The Bazel-equivalent denylist check is a `bazel query` over the resolved deps:

```sh
bazel query 'deps(//crates/wolf-core:wolf_core)' | grep -iE \
  'gtk|gdk|glib|cairo|pango|adwaita|objc|cocoa|core-graphics|winit|wgpu|skia|softbuffer' \
  || echo clean
```

See [`build.md`](../../build.md) for the full macOS build and repin workflow.

## Public API surface

Re-exported from the crate root (`wolf_core::…`):

### Lifecycle — `WolfCore` (from [`api`](src/api.rs))

- `WolfCore::start(cfg: CoreConfig, wake: WakeFn) -> Result<WolfCore, CoreError>`
  — builds a multi-thread tokio runtime, spawns the captive Neovim, wires `wake`
  into the redraw handler, attaches as an `ext_linegrid` + `ext_multigrid` UI, and
  (after attach) installs current-file tracking (so `Snapshot.current_file`
  follows the active buffer) and the split-panes layout overrides (so split/focus
  chords emit `wolf_layout` intents and `wolf_active_win` tracks the active pane).
- `read_snapshot(&self) -> SnapshotReadGuard` — owned read guard over the current
  render-state `Snapshot` (the front-end holds it for a paint, then drops it).
  `Deref`s to `Snapshot`; its lifetime is independent of `&self`.
- `send_input(&self, event: InputEvent)` — non-blocking; enqueues the event on an
  ordered FIFO channel applied by a single consumer task. Errors are logged, not
  returned (FFI-shaped fire-and-forget).
- `resize(&self, cols: u16, rows: u16)` — enqueues an `InputEvent::Resize` (the
  outer/global grid) on the same ordered channel, so it never races pending
  keystrokes.
- `resize_grid(&self, grid: u64, cols: u16, rows: u16)` — enqueues an
  `InputEvent::ResizeGrid` (a single per-window grid), same ordered channel.
- `drain_intents(&self) -> Vec<LayoutIntent>` — consumes the pending `wolf_layout`
  intents (splits / focus-direction moves) the front-end applies on each wake.
- `focus_win(&self, win: i64)` — enqueues an `InputEvent::FocusWin` (click-to-focus
  or a resolved focus move) on the ordered channel.
- `shutdown(self) -> Result<(), CoreError>` — cleanly quits the captive Neovim
  and tears down the runtime.
- `mru_recent(&self) -> Vec<FileEntry>` — the most-recently-opened files (newest
  first), pruning any whose backing file no longer exists. See `MruStore::recent`.
- `start_file_search(&self, wake: WakeFn) -> FileSearch` — starts a streaming file
  search rooted at the configured cwd (see "The file-search session" below).

`CoreConfig { init_lua: PathBuf, cols: u16, rows: u16, cwd: PathBuf }` — `cwd` is
the picker's search root and the location of the MRU cache
(`<cwd>/.wolf/mru.sqlite`). `CoreError` is `Runtime(io::Error)`, `Nvim(NvimError)`,
or `Mru(MruError)`.

### File picker — search session + ranking

- `FileSearch` (from [`files`](src/files.rs)) — a live search session.
  `read(&self) -> FileSearchReadGuard` (owned guard, `Deref`s to a state owning
  `entries: Vec<FileEntry>`, `complete: bool`, `version: u64`); `cancel(&self)`
  stops the background traversal, and `Drop` cancels too.
- `FileEntry { path: PathBuf, display: String, mru_rank: Option<u32> }` — an
  absolute `path` (to `Open`), a `display` (relative to the root, what the UI
  shows + ranks), and the MRU rank (`Some(0)` = most recent, `None` = not in MRU).
- `rank_matches(query: &str, entries: &[FileEntry]) -> Vec<Match>` (from
  [`rank`](src/rank.rs)) — pure fuzzy match + ranking: empty query = all entries
  MRU-first; non-empty = case-insensitive subsequence match, MRU-first then a
  filename-word strict-prefix preference. `Match { index, key: MatchKey }`.
- `MruStore` / `MruError` / `MRU_LIMIT` (from [`mru`](src/mru.rs)) — the SQLite
  MRU cache. Front-ends normally reach it via `mru_recent()` / `start_file_search`
  rather than directly.

### Wake callback — `WakeFn` (from [`redraw`](src/redraw.rs))

`type WakeFn = Arc<dyn Fn() + Send + Sync>`. Invoked once per `flush`, after the
snapshot lock is dropped. May run on a tokio worker thread, so the front-end is
responsible for marshalling onto its UI thread.

### Input — the `InputEvent` family (from [`input`](src/input.rs))

`InputEvent` = `Key(KeyEvent) | Mouse(MouseEvent) | Scroll(ScrollEvent) |
Resize { cols, rows } | ResizeGrid { grid, cols, rows } | FocusWin { win } |
Open { path } | Save | SaveAs { path }`. Supporting types: `KeyEvent`, `Key`,
`MouseEvent`, `MouseButton`, `MouseAction`, `ScrollEvent`, `ScrollDirection`.

`key_to_notation(&KeyEvent) -> String` translates a key event into Neovim
notation (`<CR>`, `<Esc>`, `<C-w>`, …). Modifier order is `C-`, `S-`, `A-`, `D-`.
**Shift contract:** for `Key::Char`, deliver the already-shifted glyph in the
char and leave `shift = false`; `S-` is emitted only for named keys.

### Layout intents (from [`layout`](src/layout.rs))

`LayoutIntent` = `Split { orientation: SplitOrientation, from: i64, win: i64 } |
FocusDir { dir: FocusDir, from: i64 }`, with `SplitOrientation`
(`Horizontal`/`Vertical`) and `FocusDir` (`Left`/`Right`/`Up`/`Down`). These are
parsed from the captive nvim's `wolf_layout` notifications and consumed via
`WolfCore::drain_intents`; the shell uses them to own a native split-pane layout.
A CLOSE has no intent (nvim performs a guarded `nvim_win_close`, observed via the
`win_close` redraw event).

### Render state (from [`render`](src/render.rs))

`Snapshot` — the **multigrid** render model: `grids: HashMap<u64, Grid>` (the
outer grid 1 holds the command line / messages; window content lives on grids
`>= 2`), `windows: HashMap<u64, WinInfo>` (per-window layout/viewport keyed by
grid id), `highlights`, `cursor` (with the grid it lives on), `mode_infos`,
`active_win: Option<i64>` (the active window, via `wolf_active_win`),
`current_file`, and `seq`. Accessors: `active_grid()`/`active_grid_id()`,
`grid(id)`/`grid_mut(id)`, `active_viewport()`, `grid_for_win(win)`. Plus `Grid`,
`Cell`, `HighlightTable`, `HlAttr`, `ResolvedStyle`, `DefaultColors`, `Rgb`,
`Cursor`, `CursorShape`, `ModeInfo`, `Viewport`, `WinInfo`, and the lock-guarded
`SharedSnapshot`. See [`src/README.md`](src/README.md) for per-type detail.

`Snapshot.current_file: Option<PathBuf>` is the absolute, canonicalized path of
the buffer currently being edited in nvim (`None` for an unnamed buffer). It is
updated out-of-band from the redraw stream by the `wolf_current_file`
notification (a `BufEnter`/`BufWinEnter`/`BufFilePost` autocmd installed after
attach — see "How core embeds Neovim") and the wake fires after each change (it
does NOT bump `seq`). The front-end reads it to highlight and reveal the current
file in its file tree (the GNOME sidebar uses exactly this).

## How core embeds Neovim

- **Generic untyped RPC.** All calls go through `Neovim::call` with `rmpv::Value`
  arguments, and the `redraw` stream is consumed as raw `Vec<rmpv::Value>`. This
  insulates core from `nvim-rs` 0.9.2's older typed API vs. Neovim 0.12.
- **`ext_linegrid` + `ext_multigrid`.** `nvim_ui_attach` is called with
  `ext_linegrid = true`, `ext_multigrid = true`, and `rgb = true`; `ext_multigrid`
  is what gives each Neovim window its own grid so the shell can own a native
  split-pane layout. `ext_cmdline`/`ext_messages`/`ext_popupmenu` remain deferred.
- **`WOLF_NVIM_BIN` discovery.** The binary is located _only_ via the
  `WOLF_NVIM_BIN` env var (exported by the flake devShell), never `PATH` and
  never `/nix/store` scanning.
- **Hermetic launch.** `nvim --embed --clean -u runtime/nvim/init.lua -i NONE -n`
  with `NVIM_APPNAME=wolf` and isolated `XDG_{CONFIG,DATA,STATE,CACHE}_HOME` under
  a private temp dir, so the captive editor cannot read the user's config/state.
- **Current-file tracking.** Immediately after `ui_attach`, core calls
  `setup_current_file_tracking`: it reads its own RPC channel id from
  `nvim_get_api_info` and invokes `_G.wolf_setup_current_file_tracking(chan)` (in
  `runtime/nvim/init.lua`) over `nvim_exec_lua`, which registers a
  `BufEnter`/`BufWinEnter`/`BufFilePost` autocmd that `rpcnotify`s
  `wolf_current_file` with `expand('%:p')` (plus one initial emit). The handler
  folds each notification into `Snapshot.current_file` and wakes the front-end.
- **Split-panes layout overrides.** After current-file tracking, core calls
  `setup_layout`, which (the same channel-id-via-`nvim_get_api_info` pattern)
  invokes `_G.wolf_setup_layout(chan)` (in `runtime/nvim/init.lua`). That rebinds
  the `<C-w>` split/close/focus chords and the `:split`/`:vsplit`/`:close`
  Ex-commands to `rpcnotify` `wolf_layout` intents, and registers a `WinEnter`
  autocmd that `rpcnotify`s `wolf_active_win`. The handler parses each
  `wolf_layout` into a `LayoutIntent` on a drainable queue and folds
  `wolf_active_win` into `Snapshot.active_win`.

## The file-search session

The picker drives a `FileSearch` session that mirrors the render-state
snapshot+wake mechanism rather than streaming callbacks across the boundary:

- `start_file_search(wake)` first seeds the MRU cache **synchronously** into the
  shared `FileSearchState` (so the very first `read()` already shows recent files),
  then spawns the `ignore`-crate traversal on the core's tokio runtime
  (`spawn_blocking`, since the walk is blocking IO).
- The traversal streams discovered files into the shared state in batches, bumping
  `version` and firing the same `wake` callback after each batch (and once more on
  `complete`) — exactly the marshal-onto-the-UI-thread-then-read pattern the
  front-end already implements for redraws.
- The front-end reads the current entries via the owned `FileSearchReadGuard`,
  ranks them with `rank_matches`, and renders. Dropping (or `cancel`-ing) the
  `FileSearch` stops the traversal (e.g. closing the picker or starting a new
  search).
- **Opening a file records the MRU.** Selection feeds `InputEvent::Open { path }`
  through the normal input channel; the ordered input pump records each drained
  `Open` into the `MruStore` (synchronously, never across an `.await`), so no
  separate "select" call exists and `input.rs` stays storage-free.

## Threading and locking model

- Core owns a **multi-thread tokio runtime on its own thread(s)**; the `nvim-rs`
  handler receives `redraw` notifications there.
- A **single `parking_lot::RwLock`** guards the snapshot. The redraw handler takes
  the write lock **once per whole batch**, never holds it across an `.await`, and
  fires the wake callback only **after** the lock is dropped (on `flush`).
- The write lock is acquired cooperatively (`write_async`, a `try_write` retry
  loop yielding to the scheduler) so a front-end holding a read guard cannot pin a
  worker thread.
- Input is delivered through an **ordered FIFO channel** to a single consumer
  task, which fuses a run of consecutive `Key` events into one `nvim_input` call.
  This keeps keystrokes in order and makes a burst produce a single redraw flush.

## Running the example and tests

```sh
cargo run -p wolf-core --example grid_dump   # headless smoke dump
cargo test -p wolf-core                       # spawns real, pinned nvim
```

Both require an active flake devShell (`direnv allow`) so `WOLF_NVIM_BIN` is set.
See [`tests/README.md`](tests/README.md).

## Phase 1 scope and what is deferred

In scope: hermetic pinned Neovim, `ext_linegrid` + `ext_multigrid` redraw →
multigrid snapshot, the Neovim→shell layout-intent channel (native split panes),
abstract input translation, the FFI-shaped surface, and headless verification.

Deferred to later phases: full `wrapNeovim` plugin baking (treesitter grammars,
`marksman` LSP, formatter), the macOS (`wolf-macos`) front-end, swift-bridge
bindings, native cmdline/messages/popupmenu (`ext_cmdline`/`ext_messages`/
`ext_popupmenu`), theming, and the non-layout intents (open/save dialogs, window
title). The layout intents (split/focus) and the GTK (`wolf-gnome`) front-end are
now realized.
