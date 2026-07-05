# wolf-ffi

The swift-bridge FFI surface over [`wolf-core`](../wolf-core). This crate is the seam between the platform-agnostic Rust core and the macOS (AppKit) front-end in [`wolf-macos`](../wolf-macos). It holds the single `#[swift_bridge::bridge]` module and the hand-written Rust glue behind it, and is built as both a `staticlib` (the archive the `.app` links through the generated Swift) and an `rlib` (so the Rust integration test can link the same surface headlessly).

swift-bridge is confined to this crate. `wolf-core` gains no new dependency and no GUI-toolkit dependency; the Linux/GTK front-end keeps consuming `wolf-core` directly.

## Bridge surface

All types are re-exported at the crate root (`wolf_ffi::…`).

- `wolf_ffi_start(init_lua: String, cwd: String, cols: u16, rows: u16, window_index: u16) -> Option<WolfCoreHandle>` — starts the core against the pinned Neovim. `init_lua` and `cwd` are filesystem **paths** (mirroring `CoreConfig`); `window_index` is the handle's fixed slot in `state.json`'s `windows` list (used by every `state_*` setter and by the attach-time store binding). Returns `None` (after logging) on error. A fresh per-window wake token is minted (see "The wake" below) and captured by the core's `WakeFn` so wakes route back to this window.

- `WolfCoreHandle` — an opaque, **main-thread-only** handle wrapping a `RefCell<Option<WolfCore>>` (`WolfCore` is not `Send`). It also carries the process-wide `StateStore` clone, the handle's fixed `window_index`, and its per-window `wake_token`. Methods:
  - `wake_token(&self) -> u64` — the per-core wake-routing identity token (minted fresh per `wolf_ffi_start`). The front-end registers ONE process-global callback per wake channel and looks this token up (in its `WindowRegistry`) to route each wake to the right window.
  - `read_frame(&self) -> FfiFrame` — the **active** grid's paint-ready frame.
  - `read_frame_for_grid(&self, grid: u64) -> FfiFrame` — the paint-ready frame for a SPECIFIC grid. The pane container reads one per pane (its bound window grid) plus grid 1 for the cmdline strip. An absent grid (e.g. a closed window) yields an empty `0x0` frame, which the front-end treats as "grid gone".
  - `send_key(&self, ev: FfiKeyEvent)`
  - `resize(&self, cols: u16, rows: u16)`
  - `resize_grid(&self, grid: u64, cols: u16, rows: u16)` — resizes a single window grid (in cells) on a per-pane resize; nvim echoes the authoritative size back via `grid_resize`. A no-op once the core is shut down.
  - `drain_intents(&self) -> Vec<FfiLayoutIntent>` — drains all pending layout intents (splits + focus moves) in arrival order, each mapped into an opaque `FfiLayoutIntent`. Empty `Vec` when the core is gone.
  - `drain_commands(&self) -> Vec<FfiCommand>` — drains all pending user-triggered commands (`:Wolf<Pascal>` Ex-commands / `require('wolf')` Lua calls) in arrival order, each mapped into an opaque `FfiCommand`. Empty `Vec` when the core is gone (and right after start — commands are user-triggered only).
  - `windows(&self) -> Vec<FfiWinInfo>` — ALL windows nvim currently reports (the Swift side filters live editor windows, mirroring the GNOME reading of `snapshot.windows`), each an opaque `FfiWinInfo`. Empty `Vec` when the core is gone.
  - `grid_for_win(&self, win: i64) -> u64` — the grid id bound to window handle `win`, or `0` if none maps to it (e.g. it was closed) / the core is gone.
  - `active_win(&self) -> i64` — the currently active Neovim window handle, or `0` if not yet known / the core is gone.
  - `focus_win(&self, win: i64)` — focuses a specific Neovim window (click-to-focus, or after resolving a focus-direction intent into a concrete neighbour). A no-op once the core is shut down.
  - `send_mouse(&self, ev: FfiMouseEvent)` — forwards a mouse click / drag / release. Rust maps the transparent `FfiMouseEvent` into a `wolf_core::MouseEvent` via `mouse_event_from_ffi` (`None` on a bad `button`/`action` discriminant, then dropped) and calls `WolfCore::send_mouse`, so it targets the pane's window grid with grid-local row/col. A no-op once the core is shut down.
  - `send_scroll(&self, ev: FfiScrollEvent)` — forwards a scroll-wheel / trackpad event. Rust maps the transparent `FfiScrollEvent` into a `wolf_core::ScrollEvent` via `scroll_event_from_ffi` (`None` on a bad `direction` discriminant) and calls `WolfCore::send_scroll`. A no-op once the core is shut down.
  - `start_file_search(&self) -> FfiFileSearch` — starts a streaming file-search session over the cwd (registering the picker wake) and wraps it as an `FfiFileSearch`. Infallible across the bridge: when no core is present the result simply yields no entries.
  - `start_tree_search(&self) -> FfiFileSearch` — starts a **separate**, long-lived streaming file-search session for the file-tree sidebar, registering the **tree** wake (`registered_tree_wake`) instead of the picker wake. Same infallible empty-backed fallback as `start_file_search`. The sidebar owns this session for its whole lifetime, independent of the transient picker session.
  - `current_file(&self) -> String` — the absolute path of the buffer currently being edited (canonicalized by core via the `wolf_current_file` notification), or the empty string for an unnamed buffer / shut-down core. Read on each editor wake to highlight + reveal the sidebar's current file.
  - `cwd(&self) -> String` — Neovim's LIVE, already-canonicalized working directory (the `wolf_cwd`/`DirChanged`-tracked cell, tracking `:cd`/`:lcd`/`:tcd`), or the empty string until nvim first reports it / after shutdown. Routes through `WolfCore::live_cwd` (a cheap synchronous read — cwd is NOT in the render snapshot, so unlike `current_file` it does not read `read_snapshot()`). Read on each editor wake by the sidebar to re-root the file tree when the cwd changed.
  - `recent_dirs(&self) -> Vec<String>` — the GLOBAL recent-directories MRU, newest-first (capped at 20), as owned `String`s; an empty `Vec` when the store is disabled/unreadable or the core is shut down. Delegates to `WolfCore::recent_dirs()`, mapping each `PathBuf` via `to_string_lossy().into_owned()`. The `Vec<String>` crosses the bridge as `RustVec<RustString>` (the same clean glue as `wolf_ffi_ancestors_to_reveal`). Read lazily at Open-Recent-menu open time by the macOS shell to build the File > Open Recent submenu.
  - `command_palette(&self) -> FfiCommandPalette` — starts a command-palette session seeded with `recent_dirs()` + `command_mru()` (empty-backed when the core is gone, mirroring `start_file_search`). The front-end feeds it the OTHER open windows one at a time (`add_window`) then calls `ranked` per keystroke; see `FfiCommandPalette` below.
  - `theme_titlebar_bg(&self) -> FfiColor` — the `WolfTitlebar` theme group's background color (`Snapshot.theme.titlebar.bg`), following the `default_fg`/`default_bg` unset convention: `set == false` when the group is absent or cleared, or once the core is shut down. Read on each render wake by the macOS shell, which applies it to (or reverts it from) `NSWindow.backgroundColor`.
  - `theme_titlebar_fg(&self) -> FfiColor` — the `WolfTitlebar` foreground, carried across the bridge for future title-text coloring (not applied on macOS this pass). Same unset convention as `theme_titlebar_bg`.
  - `open(&self, path: String)` — issues `InputEvent::Open { path }` to open the file; the MRU is recorded by core's input pump.
  - `chdir(&self, path: String)` — changes Neovim's global working directory to `path` (an absolute filesystem path) via `InputEvent::Chdir { path }` (`:cd`); the sidebar follows the live cwd. A no-op once the core is shut down.
  - `close_pane(&self)` — closes the current split via `InputEvent::ClosePane` (`:close`, NOT the buffer; errors harmlessly with E444 on the last split). A no-op once the core is shut down.
  - `new_buffer(&self)` — opens a new empty buffer in the current window via `InputEvent::NewBuffer` (`:enew`). A no-op once the core is shut down.
  - `state_set_geometry(&self, x, y, width, height: f64)` — records THIS window's frame geometry into the live state store (points; AppKit bottom-left global coords on macOS) at the handle's `window_index`. Like every `state_*` method it is a no-op when no store is attached (no state dir resolved at start).
  - `state_set_sidebar_visible(&self, visible: bool)` — records this window's sidebar visibility into the live state store at the handle's `window_index`.
  - `state_set_toolbar_mode(&self, mode: u8, set: bool)` — records this window's toolbar display mode at the handle's `window_index`: `mode` is a `toolbar_mode` discriminant (table below); `set == false` CLEARS the mode to unset (the value byte is then ignored, and the on-disk field is omitted); an unrecognized discriminant is a logged no-op (the stored mode is left unchanged, not cleared).
  - `state_set_window_count(&self, count: u16)` — truncates the persisted `windows` list to `count` entries (ghost cleanup at terminate time). The macOS shell calls it once with `highestLiveIndex + 1` (NOT a dense re-index), so it never drops a still-live higher slot — at the cost that a closed MIDDLE window's now-stale slot below the high-water mark survives (a documented v1 limitation). A no-op when no store is attached.
  - `state_flush(&self) -> bool` — blocks until every pending state mutation has had its disk write attempted (bounded by an internal ~2 s timeout, `STATE_FLUSH_TIMEOUT`). Returns whether everything pending was flushed in time (`true` when no store exists — there is nothing to flush). `shutdown` also performs this flush itself before tearing the core down.
  - `shutdown(&self)` — flushes the state store (bounded, so shutdown never hangs), then takes the core out and shuts it down (idempotent; infallible across the bridge — any error is logged).

  `wolf_ffi_start`, after a successful core start, clones the **process-wide** `wolf_core::StateStore` (opened once, lazily, over `wolf_core::state_dir()` in the `SHARED_STORE` `OnceLock` — all windows SHARE one store, since opening the same `state.json` dir twice would race the per-store writer threads) and attaches it to the core via `attach_state_store(store, window_index)` (so cwd changes persist core-side at this window's slot with no further front-end calls), stashing the store + index in the handle for the `state_*` methods. An unresolvable state dir only logs — state persistence is then disabled, never fatal.

- `FfiFileSearch` — an opaque handle wrapping core's `FileSearch` session (`Option`, so `start_file_search` / `start_tree_search` stay infallible) plus an interior `RefCell<wolf_core::tree::TreeIndexCache>` for the sidebar's per-directory projection. Methods:
  - `ranked(&self, query: String, max: usize) -> Vec<FfiFileEntry>` runs the ranking **in Rust** (`wolf_core::rank_matches` over the current results snapshot) and returns the top-`max` entries; it yields an empty `Vec` when the session is absent.
  - `children_of(&self, dir: String) -> Vec<FfiTreeNode>` projects the IMMEDIATE children of `dir` (`""` = the cwd root), served from a per-snapshot `wolf_core::tree::TreeIndex` cached in the interior `TreeIndexCache` keyed on the search snapshot's `version`: the index is rebuilt at most once per snapshot, and each per-directory lookup is then an O(children) borrowed-slice probe (so AppKit's numberOfChildren/child:index callback storm no longer re-scans the flat entry list per call). Projection semantics are unchanged: directories-first then files, each case-insensitive by name; dotfiles are listed and gitignored files are excluded. Empty `Vec` when the session is absent.
  - `rel_path_for(&self, abs: String) -> String` maps an absolute path to its tree-display rel-path via an **exact** entry-path match (`wolf_core::tree::rel_path_for`), or the empty string when no streamed entry matches.
  - `version(&self) -> u64` returns the current file-search snapshot version (0 when no session backs this handle; live snapshots start at 1). The sidebar's refresh gate reloads its outline only when this advances.

  Dropping the handle cancels the background traversal.

- `FfiFileEntry` — opaque; `display() -> String` (the relative display name) and `path() -> String` (the absolute path). Opaque because swift-bridge only emits `Vec<T>` support for opaque `T` — the same reason `FfiCell` is opaque.

- `FfiTreeNode` — opaque; a flattened `wolf_core::tree::TreeNode`. Accessors: `name() -> String` (the immediate component), `rel_path() -> String` (the full `/`-separated path from the cwd root), `abs_path() -> String` (the absolute path for a file; empty for a directory), and `is_dir() -> bool`. Opaque so a `Vec<FfiTreeNode>` is supported.

- `wolf_ffi_ancestors_to_reveal(rel_path: String) -> Vec<String>` — a free FFI fn (no live session needed) delegating to `wolf_core::tree::ancestors_to_reveal`: the ancestor directory rel-paths from the cwd root down to the node's parent (`"src/main.rs"` → `["src"]`, `"README.md"` → `[]`). swift-bridge emits clean `RustVec<RustString>` Swift glue for the `Vec<String>` return.

- `FfiConfig` — opaque; the resolved user-level preferences (`wolf-config.yaml`). Accessors `font_family() -> String`, `font_size() -> f64`, and `line_height() -> f64` (the line-height PERCENTAGE, `130` = 130%). Opaque because it wraps the Rust `wolf_core::WolfConfig` and hands its `String` field out through accessors (swift-bridge does not marshal a `String` field inside a transparent struct — the same reason `FfiFileEntry` is opaque).

- `wolf_ffi_load_config() -> FfiConfig` — a free FFI fn (no live session, does NOT hang off `WolfCoreHandle` — like `wolf_ffi_ancestors_to_reveal`). It delegates to `wolf_core::preferences_dir().map(load_or_create).unwrap_or_else(default_config)`: resolve the platform preferences dir, load-or-create `wolf-config.yaml` there, and fall back to the built-in defaults when the dir cannot be resolved. Infallible across the bridge. The macOS `AppDelegate` calls it once at launch (after registering the bundled fonts, before measuring the cell metrics) and passes the result into `CellMetrics.measured(config:)`.

- `FfiState` — opaque; the saved application state as loaded by `wolf_ffi_load_state`. One method, `windows() -> Vec<FfiWindowState>` — the restored per-window entries (empty when nothing usable was saved). `FfiWindowState` is opaque so the `Vec` is supported.

- `FfiWindowState` — opaque; one saved window's state (a flattened `wolf_core::WindowState`). Accessors: `x()` / `y()` / `width()` / `height()` (points; a non-positive width/height is the "unset" sentinel — do not restore), `cwd() -> String` (`""` when never reported), `sidebar_visible() -> bool`, and the `{value, set}` pair `toolbar_mode() -> u8` / `toolbar_mode_set() -> bool` (following the `FfiColor` precedent — the optional `Option<ToolbarMode>` is flattened, `toolbar_mode()` is meaningful only when `toolbar_mode_set()`).

- `wolf_ffi_load_state() -> FfiState` — a free FFI fn (standalone, like `wolf_ffi_load_config`): the PRE-START restore read the front-end performs before `wolf_ffi_start`. It delegates to `wolf_core::state_dir().and_then(load_state)`: it NEVER creates or modifies the file and NEVER opens the live store (the handle does that at start); an absent/corrupt file or an unresolvable state dir all yield an empty window list — infallible across the bridge.

  The `toolbar_mode` discriminant module is the **single source of truth** shared with the Swift `StateSeam.ToolbarMode` port (raw-valued `UInt8` enum) and the `tests/state.rs` discriminant contract, mirroring the variant order of `wolf_core::ToolbarMode`:

  | `toolbar_mode` | 0           | 1        | 2        |
  | -------------- | ----------- | -------- | -------- |
  | mode           | IconAndText | IconOnly | TextOnly |

- `FfiFrame` — an opaque, owned, paint-ready snapshot of a single grid (the **active** grid via `read_frame`, or any grid via `read_frame_for_grid`), with every cell already resolved against the highlight table **in Rust** (reusing `HighlightTable::resolve` / `ResolvedStyle` — never re-resolved in Swift). Accessors: `cols`, `rows`, `cursor_row`, `cursor_col`, `cursor_shape` (0 = Block, 1 = Horizontal, 2 = Vertical), `cursor_cell_percentage`, `cursor_visible` (whether the cursor lives on THIS frame's grid — **per-grid**, so only the pane owning the cursor's grid draws it), `grid_id` (the grid this frame was packed from), `cursor_grid` (the grid the cursor currently lives on, so a pane can tell whether the cursor belongs to it even when it is not its own grid), `default_fg`, `default_bg`, `cursor_fill`, `cursor_glyph` (cursor colors, ported from — and kept in lockstep with — wolf-gnome's `cursor.rs`, computed against the REQUESTED grid's cell under the cursor: the `WolfCursor` theme group wins first when its bg is set — fill = theme bg, glyph = theme fg or the cell bg — else the reverse-video/mode-attr logic applies, so the themed cursor needs no new FFI surface), `cells() -> Vec<FfiCell>` (one flat, row-major handoff per frame), and the four **viewport** accessors that drive the synthetic scrollbar — `topline` (the 0-based first-visible buffer line index, as nvim's `win_viewport` reports), `botline` (one-past-the-last-visible line), `curline` (the 0-based cursor buffer line), and `linecount` (the buffer's total line count). The viewport values are read in `from_snapshot_grid` from `snapshot.windows[grid].viewport` (defaulting to all-zero when the grid has no associated window, e.g. grid 1 or a closed pane).

- `FfiLayoutIntent` — opaque; one drained layout intent (a flattened `wolf_core::LayoutIntent`). Accessors `kind() -> u8` (`intent_kind::SPLIT` / `FOCUS_DIR`), `orientation() -> u8` (for a SPLIT: `orientation::HORIZONTAL` / `VERTICAL`; 0 otherwise), `dir() -> u8` (for a FOCUS_DIR: `focus_dir::LEFT` / `RIGHT` / `UP` / `DOWN`; 0 otherwise), `from() -> i64` (the origin window handle), and `win() -> i64` (for a SPLIT: the new window handle; 0 otherwise). Opaque because swift-bridge only emits `Vec<T>` support for opaque `T` — the same reason `FfiTreeNode` is opaque.

- `FfiCommand` — opaque; one drained command (a `wolf_core::Command`). Single accessor `kind() -> u8` (a `command_kind` discriminant). Opaque for the same `Vec<T>` reason as `FfiLayoutIntent` / `FfiTreeNode`. The `command_kind` module is the **single source of truth** shared with the Swift `CommandKind` port (`CommandKind.init?(ffi:)`) and the `WolfKeyMapTests` / `tests/commands.rs` discriminant contracts; it mirrors the variant order of `wolf_core::Command`:

  | `command_kind` | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 |
  | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
  | kind | NewWindow | CloseWindow | QuitApplication | HideApplication | MinimizeWindow | GoToFile | OpenFile | OpenFolder | ClosePane | NewBuffer | ResetFontSize | IncreaseFontSize | DecreaseFontSize | CommandPalette |

  Only this discriminant crosses the FFI; the macOS menu hardcodes its own item list this cut. The full registry's titles ARE now surfaced — but only through the new command palette (`CommandPalette`, `13`), whose Rust-side `rank_palette` builds the fuzzy-searchable rows (see `FfiCommandPalette` below), not the hardcoded menu. The `ClosePane` (`:close`) and `NewBuffer` (`:enew`) commands additionally have `WolfCoreHandle` methods (`close_pane` / `new_buffer`) for the native-menu face; the three `*FontSize` commands are front-end-only (per-window ephemeral font size) and have NO FFI handle method — only their discriminants cross for the nvim-drained face; `CommandPalette` (`13`) dispatches to the front-end's `openCommandPalette()`. `FfiCommand::from_command`'s exhaustive match makes a future variant reorder/addition a compile error. Commands ride the **existing editor render wake** (there is no new wake channel), so `wolf_ffi_wake.h` is **unchanged** by this surface (the `close_buffer`→`close_pane` rename + the added `new_buffer` change only `wolf-ffi.swift` / `wolf-ffi.h`) — regenerated via `bazel run //crates/wolf-ffi:gen_bindings` (below).

- `FfiCommandPalette` + `FfiCommandRow` — the command-palette session surface (both opaque; `FfiCommandRow` opaque so a `Vec<FfiCommandRow>` is supported, like `FfiCommand`). `FfiCommandPalette` is **main-thread-only** (a `RefCell<Vec<PaletteWindow>>` gives interior mutability behind the bridge's `&self`): `add_window(index: u16, cwd: String)` pushes one OTHER open window (the current window is excluded by the caller — no bridge fn takes a `Vec` argument, so the window list is fed incrementally); `ranked(query: String, max: usize) -> Vec<FfiCommandRow>` delegates to `wolf_core::rank_palette` (assembling the static commands minus `CommandPalette` + the added windows + the recent dirs, ranked MRU-first) and maps each `PaletteRow` through `FfiCommandRow::from_row`; `record(key: String)` writes the row's `mru_key` into the global `command-mru.sqlite`. There is NO wake — candidates are fixed at open and `ranked()` re-filters synchronously per keystroke. `FfiCommandRow` flattens a `PaletteRow` with accessors `display()` / `row_kind() -> u8` (the new `palette_row_kind` module: `STATIC = 0` / `SWITCH_WINDOW = 1` / `OPEN_RECENT = 2`, the single source of truth shared with the Swift `PaletteSeam.PaletteRowKind` port) / `command_kind() -> u8` (reusing `FfiCommand::from_command`'s mapping for a `Static` row, `0` for dynamic rows) / `window_index() -> u16` (the raise target for a `SwitchWindow` row, `0` otherwise) / `path() -> String` (the recent-folder path for an `OpenRecent` row, `""` otherwise) / `mru_key() -> String`.

- `FfiWinInfo` — opaque; a projection of `wolf_core::WinInfo`'s pane-relevant fields. Accessors `grid() -> u64` (the grid this window draws into, `>= 2` for editor windows), `win() -> i64` (the Neovim window handle), and `hidden() -> bool`. Opaque for the same `Vec<T>` reason as `FfiLayoutIntent` / `FfiTreeNode`.

  The discriminants packed into `FfiLayoutIntent` live in the `intent_kind` / `orientation` / `focus_dir` modules and, like the `FfiKeyEvent` `kind` table, are the **single source of truth** shared with the Swift `PaneTree` port (`Orientation.init?(ffi:)` / `FocusDir.init?(ffi:)`) and the `WolfKeyMapTests` / `tests/panes.rs` discriminant contracts. They mirror the variant order of `wolf_core::SplitOrientation` / `wolf_core::FocusDir`:

  | `intent_kind` | 0     | 1         |
  | ------------- | ----- | --------- |
  | kind          | SPLIT | FOCUS_DIR |

  | `orientation` | 0          | 1        |
  | ------------- | ---------- | -------- |
  | orientation   | Horizontal | Vertical |

  | `focus_dir` | 0    | 1     | 2   | 3    |
  | ----------- | ---- | ----- | --- | ---- |
  | dir         | Left | Right | Up  | Down |

- `FfiCell` — opaque; `text()`, `fg()`, `bg()`, `special()` (the resolved special / underline color), `flags()`. (Opaque because swift-bridge does not emit `Vec` support for transparent structs.) Its `text` field is `pub` so the Rust integration test reads it directly.

- `FfiColor` — transparent `{ rgb: u32, set: bool }` (a packed `0x00RRGGBB` plus an "is set" flag; `set == false` means a front-end fallback applies).

- `FfiKeyEvent` — transparent `{ kind: u8, ch: u32, fnum: u8, ctrl, shift, alt, meta: bool }`. Swift fills it from an `NSEvent`; Rust maps it into a `wolf_core::KeyEvent` (so nvim notation stays the single source of truth in core). `kind` mirrors the variant order of `wolf_core::input::Key`:

  | kind | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 |
  | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
  | key | Char | Enter | Esc | Tab | Backspace | Delete | Left | Right | Up | Down | Home | End | PageUp | PageDown | F(fnum) |

  These discriminants are the single source of truth shared with the Swift `WolfKeyMap` and the test suites.

- `FfiMouseEvent` — transparent `{ button: u8, action: u8, grid: u64, row: u32, col: u32, ctrl, shift, alt: bool }`. Swift fills it from an `NSEvent` (button/action discriminants + the pane's window grid and grid-local row/col); Rust maps it into a `wolf_core::MouseEvent` via `mouse_event_from_ffi`. `button` mirrors `wolf_core::MouseButton` and `action` mirrors `wolf_core::MouseAction`:

  | `mouse_button` | 0    | 1     | 2      |
  | -------------- | ---- | ----- | ------ |
  | button         | LEFT | RIGHT | MIDDLE |

  | `mouse_action` | 0     | 1       | 2    |
  | -------------- | ----- | ------- | ---- |
  | action         | PRESS | RELEASE | DRAG |

- `FfiScrollEvent` — transparent `{ direction: u8, grid: u64, row: u32, col: u32 }`. Swift fills it from an accumulated scroll-wheel delta; Rust maps it into a `wolf_core::ScrollEvent` via `scroll_event_from_ffi`. `direction` mirrors `wolf_core::ScrollDirection`:

  | `scroll_dir` | 0   | 1    | 2    | 3     |
  | ------------ | --- | ---- | ---- | ----- |
  | direction    | UP  | DOWN | LEFT | RIGHT |

  Like the `FfiKeyEvent` `kind` table, the `mouse_button` / `mouse_action` / `scroll_dir` discriminant modules are the **single source of truth** shared with the Swift `PaneContainerView` consts and the `//crates/wolf-ffi:mouse_scroll` test's local consts, mirroring the variant order of the corresponding `wolf_core` enums.

## The wake (Swift-free link path)

`wolf-core` fires a `WakeFn` on each `flush`; the front-end must marshal it onto the AppKit main thread and trigger a repaint. Rather than an `extern "Swift"` symbol — which a Rust-only integration test could never resolve at link time — the wake is a globally **registered** `extern "C"` function pointer:

```c
void wolf_ffi_set_wake(void (*wake)(uint64_t token)); // generated/wolf_ffi_wake.h
```

Each callback carries the firing core's per-window wake **token** (a `u64`, minted in `wolf_ffi_start` from the `NEXT_WAKE_TOKEN` counter and captured by the core's `WakeFn` closure), so a SINGLE process-global callback can route each wake to the right window WITHOUT clobbering — the multi-window wake-routing layer. The wake fires on a tokio worker thread and must not touch the `!Send` handle, so identity crosses the boundary as a plain `Send` scalar. Swift's `WakeBridge` registers ONE `DispatchQueue.main.async { WindowRegistry.shared.controller(for: token)?.onRenderWake() }` thunk once at startup (capturing only the global registry, so it stays `@convention(c)`). The Rust integration test registers its own recording callback; the wake is a harmless no-op when unregistered, and `wolf_ffi_start` links and runs with no Swift runtime present.

There is a **second**, parallel registered wake for the file picker:

```c
void wolf_ffi_set_filesearch_wake(void (*wake)(uint64_t token)); // generated/wolf_ffi_wake.h
```

It is a separate global `extern "C"` callback (its own `registered_filesearch_wake(token)`), fired after each streaming traversal batch of a `start_file_search` session, so the picker can refresh its results independently of editor repaints (Swift's `PickerWakeBridge` routes `token` to `WindowRegistry.shared.controller(for: token)?.picker?.refresh()`). It is otherwise identical in shape to the editor wake — token-carrying, no-op when unregistered.

There is a **third** parallel registered wake for the file-tree sidebar:

```c
void wolf_ffi_set_tree_wake(void (*wake)(uint64_t token)); // generated/wolf_ffi_wake.h
```

It is its own global `extern "C"` callback (`registered_tree_wake(token)`), fired after each streaming traversal batch of a `start_tree_search` session, so the sidebar can rebuild its outline rows independently of the picker and editor repaints (Swift's `TreeWakeBridge` routes `token` to `WindowRegistry.shared.controller(for: token)?.tree?.refresh()`). The sidebar's current-file reveal + bold rides the **editor** wake instead (the controller's `onRenderWake()` calls `FileTreeView.onRenderWake()` right after `setNeedsDisplay`, reading `current_file()`), because a `current_file` change is delivered on the render wake, not a separate traversal batch. The tree wake is otherwise identical in shape — token-carrying, no-op when unregistered.

## Generated bindings

The committed `generated/` tree is the swift-bridge output plus two hand-written files:

- `SwiftBridgeCore.swift` / `SwiftBridgeCore.h` — the shared swift-bridge runtime.
- `wolf-ffi.swift` / `wolf-ffi.h` — this crate's bridge.
- `wolf_ffi_wake.h` — the hand-written wake-registration declaration.
- `module.modulemap` — exposes the C headers to Swift as the `WolfFFI` module (matched by the `import WolfFFI` line `gen_bindings` prepends to the generated Swift, and by the `swift_interop_hint` in `BUILD.bazel`).

Regenerate after changing the bridge:

```sh
bazel run //crates/wolf-ffi:gen_bindings   # rewrites generated/, then commit
```

`gen_bindings` writes in-tree (it reads `BUILD_WORKSPACE_DIRECTORY`, which only `bazel run` provides — a genrule cannot write into the source tree). After changing Cargo dependencies, repin:

```sh
CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-ffi:wolf_ffi   # then commit lockfiles
```

## Bazel targets

- `:wolf_ffi` — the `rlib` (used by the Rust integration test).
- `:wolf_ffi_staticlib` — `libwolf_ffi.a` (linked into the app).
- `:wolf_ffi_cc` — a `cc_library` exposing the generated headers + the wake header as the `WolfFFI` Clang module and linking the archive; consumed by the Swift targets.
- `:generated_swift` — a filegroup of the generated Swift glue, compiled into `wolf-macos`'s `wolf_key_map` module.
- `:gen_bindings` — the codegen binary (run via `bazel run`).
- `:bridge_smoke` — the Rust integration test for the editor round-trip.
- `:file_search` — the Rust integration test for the file-search surface (real pinned nvim, modeled on `bridge_smoke`).
- `:tree_search` — the Rust integration test for the file-tree surface (`start_tree_search` / `children_of` / `rel_path_for` / `FfiTreeNode` accessors / `current_file` / the free `wolf_ffi_ancestors_to_reveal`), real pinned nvim, modeled on `file_search`.
- `:panes` — the Rust integration test for the split-pane surface (`drain_intents` / `FfiLayoutIntent` / `windows` / `FfiWinInfo` / `grid_for_win` / `active_win` / `focus_win` / `resize_grid` / `read_frame_for_grid`), real pinned nvim, modeled on `tree_search`.
- `:commands` — the Rust integration test for the command surface (`drain_commands` / `FfiCommand::kind()`, guarding the `command_kind` discriminant contract over the contiguous `0..=13` including `:WolfCommandPalette` → kind `13`), real pinned nvim, modeled on `panes`.
- `:command_palette` — the Rust integration test for the command-palette session surface (`command_palette()` / `FfiCommandPalette::add_window` / `ranked` / `record` / the `FfiCommandRow` accessors and the `palette_row_kind` discriminants), real pinned nvim under a hermetic `WOLF_STATE_DIR`: it asserts the static rows (excluding `CommandPalette`), a `SwitchWindow` row after `add_window`, an `OpenRecent` row after a `:cd` populates `recent_dirs`, and the MRU-first round-trip through the physical `command-mru.sqlite` after `record`. A single `#[test]` per binary (it mutates the process-global `WOLF_STATE_DIR`).
- `:file_actions` — the Rust integration test for the file-action handle methods (`chdir` / `new_buffer`), real pinned nvim, modeled on `commands` (a copy with `tempfile` from `all_crate_deps` `normal_dev`): a `chdir` polls `cwd()` until it equals the canonicalized target, then an `open` followed by `new_buffer()` (`:enew`) polls `current_file()` back to the empty string.
- `:close_pane` — the Rust integration test for the `close_pane()` handle method (`:close`), real pinned nvim, modeled on `panes` (reusing `tests/common/mod.rs`): start → one grid → `:vsplit` → live editor grids from `windows()` == 2 → `handle.close_pane()` → poll `windows()` back to 1.
- `:mouse_scroll` — the Rust integration test for the mouse/scroll + viewport surface (`send_mouse` / `send_scroll` / the `FfiMouseEvent` / `FfiScrollEvent` discriminants / the `FfiFrame` viewport accessors), real pinned nvim, modeled on `panes`.
- `:config` — the Rust integration test for the config surface (`wolf_ffi_load_config` / `FfiConfig` accessors). **Self-contained**: it points `WOLF_CONFIG_DIR` at a tempdir and needs no nvim staging (no `data` runfiles, no `no-sandbox`/`local` tag), so it runs deterministically regardless of `WOLF_NVIM_BIN`.
- `:state` — the Rust integration test for the state-persistence surface (`wolf_ffi_load_state` / `FfiState` / `FfiWindowState`, the `state_set_*` setters, `state_flush`, and the start-time store attach), real pinned nvim, modeled on `tree_search`.
- `:multi_window` — the Rust integration test for the MULTI-WINDOW surface (`wolf_ffi_start`'s trailing `window_index`, `wake_token()`, `state_set_window_count()`, the token-carrying `wolf_ffi_set_wake` ABI, and the shared-store per-index writes), driving TWO real pinned nvim instances, modeled on `state` (single `#[test]`, mutates process-global `WOLF_STATE_DIR` + registers the process-global wake callback).
- `:theme` — the Rust integration test for the WolfCursor / WolfTitlebar theme surface (`theme_titlebar_bg` / `theme_titlebar_fg`, plus the themed `cursor_fill` / `cursor_glyph` on the existing frame accessors), real pinned nvim, modeled on `panes`.

## Testing

`tests/bridge_smoke.rs` drives the Rust half against a **real** pinned Neovim (no mocks), exercising the full live round-trip (start → type → the grid reflects the typed char → resize → shutdown). It runs for real on Linux/Cargo devShells **and passes under `bazel test` on the macOS Bazel host**: the `bridge_smoke` `rust_test` stages the captive `init.lua` and the pinned Neovim tree (`//bazel/nvim:nvim_tree`) as runfiles (`data` + `$(rootpath)` env vars `WOLF_TEST_INIT_LUA` / `WOLF_TEST_NVIM_DIR`, resolved by `common::{init_lua_path, nvim_bin_or_skip}`), so it has a real nvim to drive hermetically on every run; an explicit `WOLF_NVIM_BIN` still wins for the cargo/devShell path. It runs `tags = ["no-sandbox"]` (unsandboxed, but still test-result-cached) because spawning a real nvim child does not work under the macOS build sandbox. The test only **loudly skips** if NEITHER an explicit `WOLF_NVIM_BIN` NOR the staged tree is available:

```sh
bazel test //crates/wolf-ffi:bridge_smoke
```

`tests/file_search.rs` covers the file-search surface against the same real pinned Neovim (modeled on `bridge_smoke`, reusing `tests/common/mod.rs` and the same `data` / `env` / `no-sandbox` runfile staging): it seeds a temp cwd with a couple of files, calls `start_file_search()`, polls `ranked(query, max)` until the traversal streams them in, and asserts the displays are present (`path()` absolute, `display()` relative, ranking honored) and that `open(path())` round-trips without panicking.

```sh
bazel test //crates/wolf-ffi:file_search
```

`tests/tree_search.rs` covers the file-tree surface against the same real pinned Neovim (modeled on `file_search`, reusing `tests/common/mod.rs` and the same `data` / `env` / `no-sandbox` runfile staging). It seeds a nested temp cwd (`src/main.rs`, `src/lib.rs`, `docs/intro.md`, `README.md`, a dotfile `.config.md`, and a `.gitignore` + the file it ignores), calls `start_tree_search()`, and polls `children_of("")` until the traversal streams in. It asserts directories-before-files (case-insensitive), the dotfile present, the gitignored file absent; `children_of("src")` yields the two leaf files (`is_dir() == false`, `rel_path()` under `src/`, `abs_path()` absolute under the cwd); `rel_path_for(abs)` maps a streamed entry to its display and an unknown path to `""`; `wolf_ffi_ancestors_to_reveal` returns the ancestor chain; and `open(abs)` round-trips into `current_file()` (canonicalized, robust to the macOS `/var` → `/private/var` symlink) while the core stays responsive.

```sh
bazel test //crates/wolf-ffi:tree_search
```

`tests/panes.rs` covers the split-pane surface against the same real pinned Neovim (modeled on `tree_search`, reusing `tests/common/mod.rs` and the same `data` / `env` / `no-sandbox` runfile staging). One test (`panes_split_resize_focus_close`) drives a full round-trip: attach → a baseline single editor window (`windows()` with `grid() >= 2`, `active_win()` and `grid_for_win(win)` agreeing) → a `:vsplit` whose `drain_intents()` yields a `kind() == SPLIT` intent (`orientation() == VERTICAL`, `from() ==` the original window, a new distinct `win()`) → two window grids → `resize_grid(newGrid, 40, 20)` settling to the nvim-authoritative `cols() == 40` → `focus_win(originalWin)` reflected in `active_win()` → `:close` leaving ONE window with `grid_for_win(closedWin) == 0` → grid 1 still present (`read_frame_for_grid(1).rows() > 0`) → shutdown. It also guards the discriminant contract (the `intent_kind` / `orientation` values match the Swift `PaneTree` port).

`tests/commands.rs` covers the command surface against the same real pinned Neovim (modeled on `panes`, reusing `tests/common/mod.rs` and the same `data` / `env` / `no-sandbox` runfile staging). It types `:WolfCloseWindow<CR>` and polls `drain_commands()` until an `FfiCommand` with `kind() == CLOSE_WINDOW (1)` appears, then `:WolfMinimizeWindow<CR>` for `kind() == MINIMIZE_WINDOW (4)`, `:WolfGoToFile<CR>` for `kind() == GO_TO_FILE (5)`, and `:WolfCommandPalette<CR>` for `kind() == COMMAND_PALETTE (13)` — pure FFI drains (no front-end acts on them, so nothing is destroyed) — and guards the `command_kind` discriminant contract locally (the local consts assert contiguous `0..=13`). `tests/command_palette.rs` (the `:command_palette` target) covers the palette session surface separately.

`tests/file_actions.rs` (`file_actions_chdir_and_new_buffer`) covers the file-action handle methods against the same real pinned Neovim (modeled on `commands`, reusing `tests/common/mod.rs` and the same `data` / `env` / `no-sandbox` runfile staging, plus `tempfile`): after attach it `chdir`s to a fresh temp directory and polls `cwd()` until it equals the canonicalized target, then `open`s a seeded file (polling `current_file()` until it matches) and `new_buffer()`s (`:enew`), polling `current_file()` back to the empty string, before `shutdown()`. The `:close` split-drop coverage lives in the sibling `tests/close_pane.rs`.

`tests/close_pane.rs` covers the `close_pane()` handle method (`:close`) against the same real pinned Neovim (modeled on `panes`, reusing `tests/common/mod.rs` and the same `data` / `env` / `no-sandbox` runfile staging): after attach it types `:vsplit` to reach two live editor grids (`windows()`), calls `handle.close_pane()` directly over RPC (bypassing init.lua's interactive `:close` override), and polls `windows()` back to a single grid.

```sh
bazel test //crates/wolf-ffi:panes
```

`tests/mouse_scroll.rs` covers the mouse/scroll + viewport surface against the same real pinned Neovim (modeled on `panes`, reusing `tests/common/mod.rs` and the same `data` / `env` / `no-sandbox` runfile staging). `mouse_scroll_click_drag_and_wheel` puts 80 wide lines into a live editor grid, then: `send_mouse(LEFT, PRESS, grid, row 5, col 2)` moves the cursor to exactly `cursor_row() == 5 && cursor_col() == 2` (grid-targeted, grid-local coords); a DRAG + RELEASE at `(8, 4)` keeps the core responsive; `send_scroll(DOWN)` raises `topline()` and `send_scroll(UP)` lowers it (0-based `topline` baseline). It also carries local discriminant consts mirroring the Rust `mouse_button` / `mouse_action` / `scroll_dir` mods.

```sh
bazel test //crates/wolf-ffi:mouse_scroll
```

`tests/config.rs` covers the config surface **without** any Neovim (deterministic, self-contained): it points `WOLF_CONFIG_DIR` at a tempdir, writes a known `wolf-config.yaml`, and asserts `wolf_ffi_load_config()` round-trips its `font_family()` / `font_size()` / `line_height()`; a second case asserts an absent file yields the built-in defaults. Because it stages no nvim, the `config` `rust_test` carries no `data` / `env` runfiles and no `no-sandbox`/`local` tag.

```sh
bazel test //crates/wolf-ffi:config
```

`tests/state.rs` covers the state-persistence surface against the same real pinned Neovim (modeled on `tree_search`, reusing `tests/common/mod.rs` and the same `data` / `env` / `no-sandbox` runfile staging). It is deliberately a SINGLE `#[test]` (`state_bridge_saves_and_restores_all_facets`) because it mutates the process-global `WOLF_STATE_DIR` env var, which is only safe with one test per binary. The flow: point `WOLF_STATE_DIR` at a tempdir; `wolf_ffi_load_state()` on the empty dir yields zero windows (and never creates the file); a corrupt `state.json` also yields zero windows (and is preserved); then `wolf_ffi_start` (which opens + attaches the live store), `state_set_geometry` / `state_set_sidebar_visible` / `state_set_toolbar_mode`, a `:cd` typed through `send_key`, and `state_flush()` — asserting all five persisted facets appear in the pretty-printed `state.json`; `wolf_ffi_load_state()` then round-trips them through the `FfiWindowState` accessors (including the `toolbar_mode` / `toolbar_mode_set` pair). It also guards the FFI behaviors: an unrecognized toolbar discriminant is a no-op (the stored mode is unchanged, not cleared), `(0, false)` clears the mode (the file omits the key), and local consts pin the `toolbar_mode` discriminant table (the `mouse_scroll` pattern).

```sh
bazel test //crates/wolf-ffi:state
```

`tests/theme.rs` covers the WolfCursor / WolfTitlebar theme surface against the same real pinned Neovim (modeled on `panes`, reusing `tests/common/mod.rs` and the same `data` / `env` / `no-sandbox` runfile staging). At startup (plain repo `init.lua`, no lighthouse staged, so no Wolf groups exist) both titlebar channels are unset; defining `WolfCursor` / `WolfTitlebar` via `:hi` plus an explicit `:doautocmd ColorScheme` (a plain `:hi` does NOT fire the `ColorScheme` autocmd the tracker rides) surfaces the exact packed titlebar colors on `theme_titlebar_bg()` / `theme_titlebar_fg()` and the themed cursor on `read_frame().cursor_fill()` / `cursor_glyph()`; clearing both groups (+ `:doautocmd ColorScheme`) reverts the titlebar to unset and the cursor fill to the pre-theme reverse-video baseline. Startup pickup from a REAL colorscheme (the vendored lighthouse, whose pinned commit defines both groups) is asserted by `tests/lighthouse_load.rs` instead, which polls `theme_titlebar_bg()` after attach and pins the dark-variant values. The deterministic packing rules (theme wins over the mode attr; bg-only theme keeps the reverse-video glyph) live in `tests/frame_pack.rs`, which needs no nvim.

```sh
bazel test //crates/wolf-ffi:theme
```
