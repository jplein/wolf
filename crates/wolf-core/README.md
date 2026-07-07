# wolf-core

The platform-agnostic Rust core of Wolf, a Markdown editor that embeds Neovim as its editing engine. Core contains everything Neovim-related and draws no pixels: it spawns a hermetic, pinned `nvim --embed`, attaches as an `ext_linegrid` + `ext_multigrid` UI, folds the entire `redraw` event stream into a multigrid render-state snapshot, surfaces a Neovim→shell layout-intent channel (for native split panes), accepts abstract input events, and signals the front-end (via a single wake callback) when a new frame is ready.

See the workspace docs for context: [`architecture.md`](../../architecture.md) (esp. §8 and the core↔front-end contract) and [`build.md`](../../build.md).

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

The file picker adds two dependencies — `rusqlite` (with the `bundled` feature, which vendors SQLite via `cc`) for the MRU cache and `ignore` (ripgrep's pure-Rust walker) for the traversal. Both are deliberately GUI-free: they pull **no** GUI toolkit into core's tree, so the denylist above is unchanged (the command and the denied-crate list stay exactly as written) and stays clean. The flake also needs no change (no system SQLite/libgit2 to provision).

The `config` module adds three more dependencies — `serde` (with `derive`) and `serde_yaml` (0.9) for the `wolf-config.yaml` (de)serialization and `dirs` (5) for resolving the platform preferences directory. All three are deliberately GUI-free: they pull **no** GUI toolkit into core's tree, so the denylist above is unchanged (the command and the denied-crate list stay exactly as written) and stays clean.

The `state` module adds one more dependency — `serde_json` (1) for the pretty-printed `state.json` (de)serialization (it reuses `serde` and `dirs` from the `config` module). It is likewise deliberately GUI-free, so the denylist above is unchanged and stays clean.

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

- `WolfCore::start(cfg: CoreConfig, wake: WakeFn) -> Result<WolfCore, CoreError>` — builds a multi-thread tokio runtime, spawns the captive Neovim, wires `wake` into the redraw handler, attaches as an `ext_linegrid` + `ext_multigrid` UI, and (after attach) installs current-file tracking (so `Snapshot.current_file` follows the active buffer), per-window buffer tracking (so `Snapshot.buffers` follows each window's buffer path via `wolf_buffers`, backing the macOS per-pane statusbar), the split-panes layout overrides (so split/focus chords emit `wolf_layout` intents and `wolf_active_win` tracks the active pane), directory tracking (`wolf_cwd`), theme tracking (so `Snapshot.theme` follows the `WolfCursor` / `WolfTitlebar` groups via `wolf_theme`), and — last — the command registrations (so `:Wolf<Pascal>` Ex-commands and `require('wolf').<snake>()` Lua calls emit `wolf_command`).
- `read_snapshot(&self) -> SnapshotReadGuard` — owned read guard over the current render-state `Snapshot` (the front-end holds it for a paint, then drops it). `Deref`s to `Snapshot`; its lifetime is independent of `&self`.
- `send_input(&self, event: InputEvent)` — non-blocking; enqueues the event on an ordered FIFO channel applied by a single consumer task. Errors are logged, not returned (FFI-shaped fire-and-forget).
- `resize(&self, cols: u16, rows: u16)` — enqueues an `InputEvent::Resize` (the outer/global grid) on the same ordered channel, so it never races pending keystrokes.
- `resize_grid(&self, grid: u64, cols: u16, rows: u16)` — enqueues an `InputEvent::ResizeGrid` (a single per-window grid), same ordered channel.
- `drain_intents(&self) -> Vec<LayoutIntent>` — consumes the pending `wolf_layout` intents (splits / focus-direction moves) the front-end applies on each wake.
- `drain_commands(&self) -> Vec<Command>` — consumes the pending `wolf_command` commands (user-triggered `:Wolf<Pascal>` Ex-commands / `require('wolf')` Lua calls) the front-end maps to native actions on each wake. Unlike intents there is no emit-on-attach, so this is empty right after start.
- `focus_win(&self, win: i64)` — enqueues an `InputEvent::FocusWin` (click-to-focus or a resolved focus move) on the ordered channel.
- `send_mouse(&self, ev: MouseEvent)` — enqueues an `InputEvent::Mouse` (click / drag / release) on the ordered channel; applied via `nvim_input_mouse` targeting `MouseEvent::grid` with grid-local row/col.
- `send_scroll(&self, ev: ScrollEvent)` — enqueues an `InputEvent::Scroll` (wheel / trackpad) on the ordered channel; applied via `nvim_input_mouse("wheel", …)` targeting `ScrollEvent::grid`.
- `copy_visual_selection(&self) -> Option<String>` — the BLOCKING copy path: `runtime.block_on`s `NvimProcess::yank_visual_selection` (a mode-guarded yank of the current visual selection over RPC), returning the yanked text or `None` when nvim is not in visual mode (or the selection is empty / the RPC fails). The front-end writes the returned string to the system clipboard. It must be blocking because Copy needs a synchronous return value, unlike the fire-and-forget input events.
- `paste(&self, text: String)` — enqueues an `InputEvent::Paste { text }` on the ordered channel; applied via `nvim_paste` (correct in normal/insert/visual/cmdline modes). The front-end reads the string from the system clipboard first.
- `shutdown(self) -> Result<(), CoreError>` — cleanly quits the captive Neovim and tears down the runtime.
- `attach_state_store(&self, store: StateStore, window_index: usize)` — attaches an application-state store bound to `window_index` (see "Application state — `state`" below): from then on every `wolf_cwd` notification (the `DirChanged` autocmd, covering both UI- and `:cd`-initiated changes) is mirrored into the store as THIS window's cwd (at `window_index`). The current tracked cwd — if nvim has already reported one — is pushed immediately, so the setup-time `wolf_cwd` emission that may have raced ahead of the attach is not lost. Multiple cores can share ONE store, each attaching at its own index; the front-end passes a distinct index per window.
- `mru_recent(&self) -> Vec<FileEntry>` — the most-recently-opened files (newest first), pruning any whose backing file no longer exists. Reads the per-cwd store for the captive Neovim's LIVE cwd (the tracked cell, falling back to the configured `cwd`), so the entries' `display` strings relativize against the same directory the picker is rooted at. See `MruStore::recent`.
- `recent_dirs(&self) -> Vec<PathBuf>` — the GLOBAL recent directories (newest first, capped at `RECENT_DIRS_LIMIT = 20`). These are the directories the captive Neovim has changed into (the "open folder" command or a typed `:cd`/`:lcd`/`:tcd`), recorded from the `wolf_cwd` fold point (`apply_cwd`) into the always-on `<state_dir>/folder-mru.sqlite`. Empty when the store is disabled (`state_dir()` unresolved) or unreadable. Reuses the existing `rusqlite` dependency — no new dep. See `RecentDirs::recent`.
- `command_mru(&self) -> CommandMru` — a cheap `Arc`-backed clone of the always-on GLOBAL command-palette MRU (`<state_dir>/command-mru.sqlite`, capped at `COMMAND_MRU_LIMIT = 100`), keyed on an opaque command KEY string. Unlike `recent_dirs`, NOTHING auto-records into it — only an explicit palette `record()` writes to it (it is NOT wired into the redraw handler), so a fresh store reads back EMPTY. The command palette reads/records through it; the pure `rank_palette` (below) ranks candidates against `command_mru.recent()`. See `CommandMru`.
- `pane_status(&self, grid: u64) -> PaneStatus` — the macOS per-pane statusbar strings (base `filename` + `display_path`) for the window drawing into `grid`. It joins `grid` → `WinInfo.win` → `Snapshot.buffers` under a read guard that is DROPPED before it reads `live_cwd()` (mirroring `mru_recent`), then formats the buffer path via the pure `pane_status` over the live cwd and the canonicalized home directory; a window with no matching buffer (unnamed, or a grid with no window) yields `NO_NAME` (`[No Name]`) with an empty display path. See the pure `pane_status` / `PaneStatus` / `NO_NAME` re-exports (from [`pane_status`](src/pane_status.rs)).
- `start_file_search(&self, wake: WakeFn) -> FileSearch` — starts a streaming file search rooted at the captive Neovim's live working directory (`getcwd()`), falling back to the configured cwd (see "The file-search session" below).

`CoreConfig { init_lua: PathBuf, cols: u16, rows: u16, cwd: PathBuf }` — `cwd` is the picker's FALLBACK search root, the location of the MRU cache's pre-warmed store (`<cwd>/.wolf/mru.sqlite`), and the directory the captive Neovim is launched in. The picker's LIVE search root AND its MRU store both follow the captive Neovim's `getcwd()`; the configured `cwd` is used only when that query fails or returns an unusable path. `CoreError` is `Runtime(io::Error)`, `Nvim(NvimError)`, or `Mru(MruError)` (the `Mru` variant is retained for the public surface but `start` no longer produces it — per-cwd stores open lazily and gracefully).

### File picker — search session + ranking

- `FileSearch` (from [`files`](src/files.rs)) — a live search session. `read(&self) -> FileSearchReadGuard` (owned guard, `Deref`s to a state owning `entries: Vec<FileEntry>`, `complete: bool`, `version: u64`); `cancel(&self)` stops the background traversal, and `Drop` cancels too.
- `FileEntry { path: PathBuf, display: String, mru_rank: Option<u32> }` — an absolute `path` (to `Open`), a `display` (relative to the root, what the UI shows + ranks), and the MRU rank (`Some(0)` = most recent, `None` = not in MRU).
- `rank_matches(query: &str, entries: &[FileEntry]) -> Vec<Match>` (from [`rank`](src/rank.rs)) — pure fuzzy match + ranking: empty query = all entries MRU-first; non-empty = case-insensitive subsequence match, MRU-first then a filename-word strict-prefix preference. `Match { index, key: MatchKey }`.
- `MruStore` / `MruError` / `MRU_LIMIT` (from [`mru`](src/mru.rs)) — the SQLite MRU cache. Front-ends normally reach it via `mru_recent()` / `start_file_search` rather than directly.
- `rank_palette(recent_dirs, windows, command_mru_recent, query, max) -> Vec<PaletteRow>` (from [`palette`](src/palette.rs)) — the pure command-palette analog of `rank_matches`, which it REUSES for the fuzzy match + MRU-first ordering. It assembles the candidate rows (every `COMMAND_SPECS` row except `CommandPalette` → a `Static` row; one `PaletteWindow` → `SwitchWindow` row, labeled "Untitled" for an empty cwd; one recent dir → `OpenRecent` row), ranks them against `command_mru_recent`, and truncates to `max`. `PaletteRow { display, mru_key, kind: PaletteRowKind, command, window_index, path }`, `PaletteRowKind` (`Static`/`SwitchWindow`/`OpenRecent`), and `PaletteWindow { index, cwd }` are the value types; `CommandMru` / `COMMAND_MRU_LIMIT` (from [`command_mru`](src/command_mru.rs)) back the MRU. Front-ends reach this over the FFI as the `FfiCommandPalette` session rather than calling it directly.

### Wake callback — `WakeFn` (from [`redraw`](src/redraw.rs))

`type WakeFn = Arc<dyn Fn() + Send + Sync>`. Invoked once per `flush`, after the snapshot lock is dropped. May run on a tokio worker thread, so the front-end is responsible for marshalling onto its UI thread.

### Input — the `InputEvent` family (from [`input`](src/input.rs))

`InputEvent` = `Key(KeyEvent) | Mouse(MouseEvent) | Scroll(ScrollEvent) | Resize { cols, rows } | ResizeGrid { grid, cols, rows } | FocusWin { win } | Open { path } | Save | SaveAs { path } | Paste { text: String }`. `Paste` is applied via `nvim_paste` (fed by `WolfCore::paste`, the clipboard-paste face; the yank half of copy is the blocking `copy_visual_selection` rather than an input event, since it returns text). Supporting types: `KeyEvent`, `Key`, `MouseEvent { button, action, grid, row, col, ctrl, shift, alt }`, `MouseButton`, `MouseAction`, `ScrollEvent { direction, grid, row, col }`, `ScrollDirection`. `MouseEvent`/`ScrollEvent` carry a target `grid: u64` with GRID-LOCAL row/col, so under `ext_multigrid` a front-end targets the pane's window grid (`>= 2`) rather than the global grid 0.

`key_to_notation(&KeyEvent) -> String` translates a key event into Neovim notation (`<CR>`, `<Esc>`, `<C-w>`, …). Modifier order is `C-`, `S-`, `A-`, `D-`. **Shift contract:** for `Key::Char`, deliver the already-shifted glyph in the char and leave `shift = false`; `S-` is emitted only for named keys.

### Layout intents (from [`layout`](src/layout.rs))

`LayoutIntent` = `Split { orientation: SplitOrientation, from: i64, win: i64 } | FocusDir { dir: FocusDir, from: i64 }`, with `SplitOrientation` (`Horizontal`/`Vertical`) and `FocusDir` (`Left`/`Right`/`Up`/`Down`). These are parsed from the captive nvim's `wolf_layout` notifications and consumed via `WolfCore::drain_intents`; the shell uses them to own a native split-pane layout. A CLOSE has no intent (nvim performs a guarded `nvim_win_close`, observed via the `win_close` redraw event).

### Commands (from [`command`](src/command.rs))

A `Command` is a no-parameter, no-return user action: `NewWindow`, `CloseWindow`, `QuitApplication`, `HideApplication`, `MinimizeWindow`, `GoToFile`, `OpenFile`, `OpenFolder`, `ClosePane`, `NewBuffer`, `ResetFontSize`, `IncreaseFontSize`, `DecreaseFontSize`, `CommandPalette`, `Copy`, `Paste` (the variant order is the APPEND-ONLY discriminant contract shared with `wolf-ffi` and the macOS front-end; `ClosePane` is the former `CloseFile` renamed in place at 8, `NewBuffer`..`DecreaseFontSize` are appended at 9..12, `CommandPalette` at 13, and `Copy`(14) / `Paste`(15) are appended past it). `COMMAND_SPECS: &[CommandSpec]` is the canonical registry — 16 rows, one per command, in variant order, carrying the canonical snake_case `id`, the PascalCase `ex_command` (`:Wolf<Pascal>`), a `title`, and a `MenuGroup` (`App`/`File`/`Window`/`View`/`Edit`, the `View` group carrying the three font-size commands and `CommandPalette` — id `command_palette`, `:WolfCommandPalette`, title "Command Palette…" — and the `Edit` group carrying `Copy` (id `copy`, `:WolfCopy`, title "Copy") and `Paste` (id `paste`, `:WolfPaste`, title "Paste")). ONE canonical id derives both faces; the id→ex_command mapping is stored explicitly as data (no transform code). `Command::from_id(id) -> Option<Command>` resolves the `wolf_command` payload; `Command::spec(self) -> &CommandSpec` returns the row. Commands are ONLY user-triggered (no emit-on-attach) and are consumed via `WolfCore::drain_commands`. This module is pure data + enums (no GUI toolkit).

### Render state (from [`render`](src/render.rs))

`Snapshot` — the **multigrid** render model: `grids: HashMap<u64, Grid>` (the outer grid 1 holds the command line / messages; window content lives on grids `>= 2`), `windows: HashMap<u64, WinInfo>` (per-window layout/viewport keyed by grid id), `highlights`, `cursor` (with the grid it lives on), `mode_infos`, `active_win: Option<i64>` (the active window, via `wolf_active_win`), `current_file`, `buffers: HashMap<i64, PathBuf>` (the per-window buffer-path map, keyed by the Neovim window handle — each an absolute canonicalized buffer path, unnamed windows ABSENT — updated out-of-band via the `wolf_buffers` notification, WHOLESALE-replaced on each emit so stale/closed windows vanish, no `seq` bump; the macOS front-end joins grid → window → path via `WolfCore::pane_status`), `theme: ThemeColors` (the `WolfCursor` / `WolfTitlebar` app-theme colors, via `wolf_theme`; each channel `Option<Rgb>`, all-`None` when a group is absent or cleared), and `seq`. Accessors: `active_grid()`/`active_grid_id()`, `grid(id)`/`grid_mut(id)`, `active_viewport()`, `grid_for_win(win)`, `is_visual_mode()` (true when the cursor's cached `ModeInfo.name == "visual"`, i.e. `v`/`V`/`<C-v>` — excludes `"visual_select"`; gates the macOS Copy menu item over the FFI). Plus `Grid`, `Cell`, `HighlightTable`, `HlAttr`, `ResolvedStyle`, `DefaultColors`, `Rgb`, `Cursor`, `CursorShape`, `ModeInfo`, `Viewport`, `WinInfo`, `ThemeGroup`/`ThemeColors`, and the lock-guarded `SharedSnapshot`. See [`src/README.md`](src/README.md) for per-type detail.

`Snapshot.current_file: Option<PathBuf>` is the absolute, canonicalized path of the buffer currently being edited in nvim (`None` for an unnamed buffer). It is updated out-of-band from the redraw stream by the `wolf_current_file` notification (a `BufEnter`/`BufWinEnter`/`BufFilePost` autocmd installed after attach — see "How core embeds Neovim") and the wake fires after each change (it does NOT bump `seq`). The front-end reads it to highlight and reveal the current file in its file tree (the GNOME sidebar uses exactly this).

### User preferences — `config` (from [`config`](src/config.rs))

A GUI-toolkit-free reader for the user-level `wolf-config.yaml` preferences file (font family / size / line-height). It is standalone — it does not hang off `WolfCore` — and is exposed to the macOS front-end over the `wolf-ffi` bridge (`wolf_ffi_load_config` / `FfiConfig`).

- `WolfConfig { font_family: String, font_size: f64, line_height: f64 }` — the resolved configuration. `line_height` is the PERCENTAGE of the font size (`130` = 130%).
- `preferences_dir() -> Option<PathBuf>` — the platform preferences directory. It honors a `WOLF_CONFIG_DIR` environment override FIRST (a test/determinism hook); otherwise it is `dirs::preference_dir()/"Wolf"` (`~/Library/Preferences/Wolf`) on macOS and `dirs::config_dir()/"wolf"` (`~/.config/wolf`) on Linux. Both platform paths are resolved in code even though only macOS consumes prefs this increment.
- `default_config() -> WolfConfig` — font-family `"iA Writer Mono S"`, line-height `130`, and a platform-specific font-size (14 on macOS, 10 on Linux, via `cfg!`).
- `load_or_create(dir: &Path) -> WolfConfig` — **infallible** (defaults-on-error). Reads `<dir>/wolf-config.yaml`: when the file is ABSENT it creates the parent and writes the serialized defaults, returning them; when it PARSES it returns the parsed config; when it is present but malformed / the wrong shape it logs a `tracing::warn!` and returns the defaults WITHOUT overwriting the user's file (it only ever writes when the file is absent).

The on-disk schema has a top-level `config:` key with kebab-case fields:

```yaml
config:
  font-family: "iA Writer Mono S"
  font-size: 14
  line-height: 130
```

### Application state — `state` (from [`state`](src/state.rs))

A GUI-toolkit-free persistence layer for per-window application state (window geometry, the embedded Neovim's working directory, and sidebar visibility), written to a pretty-printed **`state.json`** in the platform state directory. It is exposed to the macOS front-end over the `wolf-ffi` bridge (`wolf_ffi_load_state` / `FfiState` for the pre-start restore read, `WolfCoreHandle::state_set_*` / `state_flush` for the live store); a future wolf-gnome consumes this same API directly (`state_dir()` + `StateStore::open` + `WolfCore::attach_state_store`).

- `WolfState { windows: Vec<WindowState> }` — the whole persisted state: a LIST of windows, one entry per open OS window. The macOS front-end populates one slot per window (each window's `WolfCoreHandle` writes at its fixed `window_index`); `truncate_windows(len)` prunes trailing slots at terminate time.
- `WindowState { x, y, width, height: f64, cwd: String, sidebar_visible: bool }` — one window's state. Geometry is in `f64` points, interpreted by the writing front-end (macOS stores `NSWindow.frame`, AppKit bottom-left global coordinates); a non-positive `width`/`height` is the "unset" sentinel (restore treats the geometry as absent). `cwd` is the absolute canonicalized path nvim reported, or `""` when not yet reported (restore validates it is a non-empty existing directory before use).
- `state_dir() -> Option<PathBuf>` — the platform state directory, parallel to `preferences_dir()`. It honors a `WOLF_STATE_DIR` environment override FIRST (a test/determinism hook); otherwise it is `dirs::data_dir()/"Wolf"` (`~/Library/Application Support/Wolf`) on macOS and `dirs::state_dir()/"wolf"` (`~/.local/state/wolf`, falling back to `dirs::data_dir()`) on Linux.
- `load_state(dir: &Path) -> Option<WolfState>` — **infallible and strictly read-only** (the silently-ignore contract): it NEVER creates, rewrites, or deletes the file. An absent file is a silent `None`; ANY other failure — unreadable, unparseable, wrong shape, missing required field, or a `version` other than `1` — logs a `tracing::warn!` and returns `None`, leaving the on-disk bytes untouched (all-or-nothing, matching the `config` precedent). Unknown extra fields are tolerated, so newer writers do not break older readers.
- `StateStore` — the live, cloneable store handle. `open(dir: &Path)` (explicit directory — the injection seam; production callers pass `state_dir()`) loads the existing file per `load_state` (defaults on `None`) and spawns ONE dedicated writer thread (`wolf-state-writer`) that serializes ALL disk writes: every setter (`set_geometry(idx, x, y, w, h)`, `set_cwd(idx, &Path)`, `set_sidebar_visible(idx, visible)`) merges into the in-memory `WolfState` under a brief mutex (growing the window list with defaults up to `idx`, skipping unchanged writes) and wakes the writer, which sleeps a short coalesce delay (~100 ms, so a burst of mutations produces one write), then atomically replaces the file via a sibling tmp file + rename — a crash never leaves a partial file. `state()` returns a clone of the in-memory truth; `flush(timeout) -> bool` blocks until every pending mutation has had its write ATTEMPTED (write errors log but still count, so flush cannot hang on a read-only disk). `truncate_windows(len)` shrinks the `windows` list to `len` entries (dropping trailing slots, bumping the generation and waking the writer) — the terminate-time ghost cleanup for closed windows. All operations are infallible (log-not-fail). The writer thread exits on its own once every handle is dropped and nothing is pending — a mutation made just before the last drop is still written (no explicit shutdown call is needed).

Concurrency notes: opening the SAME directory twice in one process is unsupported (two writer threads would race on the shared tmp file — attach ONE store and clone it per window instead; the `wolf-ffi` bridge realizes exactly this with a process-wide `SHARED_STORE` cloned into every window's handle); concurrent Wolf PROCESSES are out of scope — the last writer wins, but each write is atomic so the file is never torn.

The cwd facet is saved by core itself: `WolfCore::attach_state_store(store, window_index)` wires the store into the redraw handler (installing only the store slot — it does NOT push the tracked cwd at attach time), whose `wolf_cwd` fold point (`apply_cwd`) mirrors every GENUINE directory change into the store at that `window_index` — so both UI-initiated and `:cd`-initiated changes persist with no front-end involvement. The one-shot INITIAL (setup-time) `wolf_cwd` emit is tracked-only and deliberately NOT mirrored, so a passive-launch / empty-state window persists `cwd == ""` rather than its launch directory; a restored window's cwd comes from `StateStore::open`'s disk load and is updated only by a later real `:cd`. Geometry / sidebar changes are pushed by the front-end through the store's setters (at the same per-window index).

The on-disk schema (`version` is `1`; any other version is silently ignored):

```json
{
  "version": 1,
  "windows": [
    {
      "x": 128.0,
      "y": 64.0,
      "width": 1180.0,
      "height": 760.0,
      "cwd": "/Users/alice/projects/notes",
      "sidebar_visible": true
    }
  ]
}
```

## How core embeds Neovim

- **Generic untyped RPC.** All calls go through `Neovim::call` with `rmpv::Value` arguments, and the `redraw` stream is consumed as raw `Vec<rmpv::Value>`. This insulates core from `nvim-rs` 0.9.2's older typed API vs. Neovim 0.12.
- **`ext_linegrid` + `ext_multigrid`.** `nvim_ui_attach` is called with `ext_linegrid = true`, `ext_multigrid = true`, and `rgb = true`; `ext_multigrid` is what gives each Neovim window its own grid so the shell can own a native split-pane layout. `ext_cmdline`/`ext_messages`/`ext_popupmenu` remain deferred.
- **`WOLF_NVIM_BIN` discovery.** The binary is located _only_ via the `WOLF_NVIM_BIN` env var (exported by the flake devShell), never `PATH` and never `/nix/store` scanning.
- **Hermetic launch.** `nvim --embed --clean -u runtime/nvim/init.lua -i NONE -n` with `NVIM_APPNAME=wolf` and isolated `XDG_{CONFIG,DATA,STATE,CACHE}_HOME` under a private temp dir, so the captive editor cannot read the user's config/state.
- **Current-file tracking.** Immediately after `ui_attach`, core calls `setup_current_file_tracking`: it reads its own RPC channel id from `nvim_get_api_info` and invokes `_G.wolf_setup_current_file_tracking(chan)` (in `runtime/nvim/init.lua`) over `nvim_exec_lua`, which registers a `BufEnter`/`BufWinEnter`/`BufFilePost` autocmd that `rpcnotify`s `wolf_current_file` with `expand('%:p')` (plus one initial emit). The handler folds each notification into `Snapshot.current_file` and wakes the front-end.
- **Per-window buffer tracking.** After current-file tracking, core calls `setup_buffer_tracking` (the same channel-id-via-`nvim_get_api_info` pattern), which invokes `_G.wolf_setup_buffer_tracking(chan)` (in `runtime/nvim/init.lua`). That registers `BufEnter`/`BufWinEnter`/`BufFilePost`/`WinEnter`/`WinNew`/`WinClosed` autocmds that `rpcnotify` `wolf_buffers` with a list of `[win, nvim_buf_get_name(buf)]` pairs for ALL windows of the current tabpage (plus one initial emit). The handler folds each notification WHOLESALE into `Snapshot.buffers` (win handle → canonicalized buffer path, unnamed windows skipped) and wakes the front-end (no `seq` bump), so the macOS per-pane statusbar can join a grid → window → buffer path via `WolfCore::pane_status`.
- **Split-panes layout overrides.** After current-file tracking, core calls `setup_layout`, which (the same channel-id-via-`nvim_get_api_info` pattern) invokes `_G.wolf_setup_layout(chan)` (in `runtime/nvim/init.lua`). That rebinds the `<C-w>` split/close/focus chords and the `:split`/`:vsplit`/`:close` Ex-commands to `rpcnotify` `wolf_layout` intents, and registers a `WinEnter` autocmd that `rpcnotify`s `wolf_active_win`. The handler parses each `wolf_layout` into a `LayoutIntent` on a drainable queue and folds `wolf_active_win` into `Snapshot.active_win`.
- **Theme tracking.** After directory tracking, core calls `setup_theme_tracking`, which (the same channel-id pattern) invokes `_G.wolf_setup_theme_tracking(chan)` (in `runtime/nvim/init.lua`). That registers a `ColorScheme` autocmd that `rpcnotify`s `wolf_theme` with the effective `WolfCursor` / `WolfTitlebar` highlight-group colors (read with `link = false`; four flat scalars, `-1` = unset; plus one initial emit so the startup colorscheme is picked up). The handler folds each notification into `Snapshot.theme` and wakes the front-end (no `seq` bump).
- **Command registration.** Last, after theme tracking, core calls `setup_commands`, which (the same channel-id pattern) invokes `_G.wolf_setup_commands(chan, commands)` (in `runtime/nvim/init.lua`), passing the `COMMAND_SPECS` list as `[id, ex_command]` pairs. That seeds `package.loaded['wolf']` (so `require('wolf')` resolves under `--clean`) and, per pair, registers a `require('wolf').<id>()` Lua function and a `:<ex_command>` user command (`nargs=0, force=true`), each `rpcnotify`ing `wolf_command '<id>'`. The handler resolves the id via `Command::from_id`, pushes onto a drainable queue, and wakes the front-end (no `seq` bump). No autocmd (user-triggered only).

## The file-search session

The picker drives a `FileSearch` session that mirrors the render-state snapshot+wake mechanism rather than streaming callbacks across the boundary:

- `start_file_search(wake)` first resolves the traversal root to the captive Neovim's **live working directory** (`getcwd()`, queried over RPC via a brief `runtime.block_on`), so the picker follows wherever Neovim is (`:cd`/`:lcd`/ `:tcd`); if that query fails or returns an unusable path it falls back to the configured `cwd`. It then seeds the MRU cache **synchronously** into the shared `FileSearchState` (so the very first `read()` already shows recent files), then spawns the `ignore`-crate traversal on the core's tokio runtime (`spawn_blocking`, since the walk is blocking IO). **The MRU seed follows nvim's live cwd:** it is taken from the per-cwd store at `<resolved-root>/.wolf` (the SAME resolved search root the traversal uses), so seeded entries lie inside the search root and their `display` strings are root-relative — per-project MRU isolation, never absolute and never relative to a stale configured cwd. (The write path is symmetric: the input pump records each `InputEvent::Open` into the per-cwd store for nvim's live cwd, tracked out-of-band via the `wolf_cwd`/`DirChanged` notification.)
- The traversal streams discovered files into the shared state in batches, bumping `version` and firing the same `wake` callback after each batch (and once more on `complete`) — exactly the marshal-onto-the-UI-thread-then-read pattern the front-end already implements for redraws.
- The front-end reads the current entries via the owned `FileSearchReadGuard`, ranks them with `rank_matches`, and renders. Dropping (or `cancel`-ing) the `FileSearch` stops the traversal (e.g. closing the picker or starting a new search).
- **Opening a file records the MRU.** Selection feeds `InputEvent::Open { path }` through the normal input channel; the ordered input pump records each drained `Open` into the `MruStore` (synchronously, never across an `.await`), so no separate "select" call exists and `input.rs` stays storage-free.

## Threading and locking model

- Core owns a **multi-thread tokio runtime on its own thread(s)**; the `nvim-rs` handler receives `redraw` notifications there.
- A **single `parking_lot::RwLock`** guards the snapshot. The redraw handler takes the write lock **once per whole batch**, never holds it across an `.await`, and fires the wake callback only **after** the lock is dropped (on `flush`).
- The write lock is acquired cooperatively (`write_async`, a `try_write` retry loop yielding to the scheduler) so a front-end holding a read guard cannot pin a worker thread.
- Input is delivered through an **ordered FIFO channel** to a single consumer task, which fuses a run of consecutive `Key` events into one `nvim_input` call. This keeps keystrokes in order and makes a burst produce a single redraw flush.

## Running the example and tests

```sh
cargo run -p wolf-core --example grid_dump   # headless smoke dump
cargo test -p wolf-core                       # spawns real, pinned nvim
```

Both require an active flake devShell (`direnv allow`) so `WOLF_NVIM_BIN` is set. See [`tests/README.md`](tests/README.md).

## Phase 1 scope and what is deferred

In scope: hermetic pinned Neovim, `ext_linegrid` + `ext_multigrid` redraw → multigrid snapshot, the Neovim→shell layout-intent channel (native split panes), abstract input translation, the FFI-shaped surface, and headless verification.

Deferred to later phases: full `wrapNeovim` plugin baking (treesitter grammars, `marksman` LSP, formatter), native cmdline/messages/popupmenu (`ext_cmdline`/ `ext_messages`/`ext_popupmenu`), theming, and the non-layout intents (open/save dialogs, window title). The layout intents (split/focus) and the GTK (`wolf-gnome`) front-end are realized; the macOS (`wolf-macos`) front-end and its swift-bridge bindings (in the `wolf-ffi` crate) are realized — a **multi-window** AppKit shell over the swift-bridge surface (each window backed by its own core / `WolfCoreHandle`), with native panes, the picker, the file tree, mouse/scroll, and bundled Neovim.
