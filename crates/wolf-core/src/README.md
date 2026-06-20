# wolf-core/src

The `wolf-core` source modules. Each is a single file (not a subdirectory). Data
flows: `nvim` spawns the captive Neovim and hands its `redraw` stream to a
`redraw::RedrawHandler`, which folds events into the `render` snapshot; `input`
translates abstract events back into RPC; `api` ties it together behind the
FFI-shaped `WolfCore`.

Dependency order: `render` → `input` → `redraw` → `nvim` → `api`.

## `lib.rs`

Crate root. Declares the public modules (`api`, `input`, `nvim`, `redraw`,
`render`) and re-exports the public surface: `WolfCore`, `CoreConfig`,
`CoreError`, `SnapshotReadGuard`; the input types and `key_to_notation`; `WakeFn`;
and the render-state types. Documents the one hard rule (no GUI toolkit in the
dependency tree).

## `render.rs`

The toolkit-agnostic render-state model — the in-memory description of what
should be drawn. Nothing here references a GUI toolkit or draws pixels.

Key types:

- `Rgb { r, g, b }` — `Rgb::from_packed(u32)` unpacks Neovim's `0xRRGGBB`.
- `HlAttr` — one highlight group's attributes (`fg`/`bg`/`special` as
  `Option<Rgb>`, plus `bold`/`italic`/`underline`/`undercurl`/`strikethrough`/
  `reverse`).
- `DefaultColors { fg, bg, special }` — from `default_colors_set`.
- `ResolvedStyle` — a highlight resolved against defaults, `reverse` already
  applied (fg/bg swapped), ready to paint.
- `HighlightTable { defaults, attrs }` — `new()`, `define(hl_id, HlAttr)`,
  `set_defaults(DefaultColors)`, and `resolve(hl_id) -> ResolvedStyle`
  (`hl_id == 0` resolves to the defaults).
- `Cell { text: String, hl_id: u64 }` — `text` is one grapheme cluster, or `""`
  for the trailing placeholder cell of a double-width character.
- `Grid { cols, rows, .. }` — `new`, `resize`, `clear`, `cell(row, col)`,
  `set_cell(row, col, Cell)` (out-of-bounds ignored), `scroll(top, bot, left,
  right, rows)` (positive `rows` = content moves up), and `row_text(row)` (empty
  cells render as a single space to preserve column alignment).
- `CursorShape` (`Block` | `Horizontal` | `Vertical`), `ModeInfo { name, shape,
  cell_percentage, attr_id }`, `Cursor { row, col, mode_idx }`.
- `Viewport { topline, botline, curline, linecount }` (all `u64`, as nvim
  reports; `topline` is 1-based).
- `Snapshot { grid, highlights, cursor, mode_infos, viewport, seq }` — `seq` is a
  monotonic frame counter bumped on each `flush`. `Snapshot::new(cols, rows)`.
- `SharedSnapshot` — `Arc<RwLock<Snapshot>>` wrapper: `new`, `read()`,
  `read_arc()` (owned guard), `write()`, `write_async()` (cooperative, non-pinning
  acquire from async code), `clone_handle()`. `OwnedSnapshotReadGuard` is the
  owned read-guard type returned by `read_arc`.

**Locking invariant:** exactly one `RwLock`; the redraw handler takes the write
lock once per batch, never across an `.await`, and wakes only after it drops.

## `input.rs`

Abstract input events (front-end → core) and their translation into Neovim
notation and generic RPC.

Types: `Key` (`Char(char)`, `Enter`, `Esc`, `Tab`, `Backspace`, `Delete`,
arrows, `Home`, `End`, `PageUp`, `PageDown`, `F(u8)`); `KeyEvent { key, ctrl,
shift, alt, meta }`; `MouseButton`, `MouseAction`, `MouseEvent`;
`ScrollDirection`, `ScrollEvent`; and the top-level `InputEvent` enum.

Functions:

- `key_to_notation(&KeyEvent) -> String` (public) — modifier order `C-`, `S-`,
  `A-`, `D-`; named keys map to angle-bracket names (`<CR>`, `<Esc>`, `<Tab>`,
  `<BS>`, `<Del>`, `<Left>`, `<F1>`, …); a bare printable char maps to itself,
  except `<` → `<lt>`. **Shift contract:** `S-` is *not* emitted for printable
  chars (deliver the shifted glyph in the char); it applies only to named keys.
- `apply(nvim, &InputEvent)` (crate-internal) — `Key` → `nvim_input(notation)`;
  `Mouse` → `nvim_input_mouse`; `Scroll` → wheel via `nvim_input_mouse`; `Resize`
  → `nvim_ui_try_resize` (the authoritative grid size arrives via `grid_resize`,
  so the local grid is never resized here); `Open` → `edit <fnameescape path>`;
  `Save` → `write`; `SaveAs` → `saveas <fnameescape path>`.
- `apply_batch(nvim, &[InputEvent])` (crate-internal) — applies a batch in order,
  fusing a run of consecutive `Key` events into one `nvim_input` call (so a burst
  yields a single redraw flush); non-key events are applied individually.

All RPC is generic/untyped; key delivery flows through a single shared
`nvim_input` helper. Paths are escaped with a Vim-style `fnameescape`.

## `redraw.rs`

The `redraw` handler: folds Neovim's redraw stream into the shared snapshot.

- `WakeFn = Arc<dyn Fn() + Send + Sync>` — fired once per `flush`, after the lock
  drops.
- `RedrawHandler { snapshot, wake }` — `RedrawHandler::new(SharedSnapshot,
  WakeFn)`. Implements `nvim_rs::Handler` with `type Writer = Compat<ChildStdin>`;
  `handle_notify` ignores everything but `name == "redraw"`.

`apply_redraw` takes the write lock once via `write_async`, walks the batch, and
returns whether a `flush` was seen so the caller can wake after the lock drops.
Handled events: `grid_resize`, `grid_clear`, `grid_line` (walks cell tuples
`[text]` / `[text, hl]` / `[text, hl, repeat]`, carrying `hl_id` forward, expanding
`repeat`, and storing Neovim's empty trailing cell of a wide glyph **verbatim** —
no synthesized placeholder), `grid_scroll`, `hl_attr_define`,
`default_colors_set`, `mode_info_set`, `mode_change`, `grid_cursor_goto`,
`win_viewport`, and `flush` (bumps `seq`). Unknown events are ignored.

## `nvim.rs`

Spawning and supervising the captive, hermetic Neovim child process.

- `NvimConfig { nvim_bin, init_lua, appname, state_root, initial_cols,
  initial_rows, .. }` — `from_env(init_lua, cols, rows)` reads `WOLF_NVIM_BIN`
  (erroring `BinNotSet` if unset/empty) and creates a private temp dir to host the
  isolated `XDG_*` state. The temp dir is kept alive by an internal guard.
- `NvimError` — `BinNotSet`, `StateDir(io::Error)`, `Spawn(io::Error)`,
  `Rpc(String)`, `Attach(String)`.
- `spawn(&NvimConfig, RedrawHandler) -> Result<NvimProcess, NvimError>` — creates
  the isolated XDG dirs and launches `nvim --embed --clean -u <init.lua> -i NONE
  -n` with `NVIM_APPNAME` and `XDG_{CONFIG,DATA,STATE,CACHE}_HOME` pinned under
  `state_root` (stdin/stdout piped, stderr null), wiring the handler via
  `new_child_cmd`.
- `NvimProcess` — `handle()` (clone the RPC handle), `ui_attach(cols, rows)`
  (attaches `ext_linegrid` + `rgb`), `try_resize(cols, rows)`
  (`nvim_ui_try_resize`), and `shutdown(self)` (sends `qa!` bounded by a grace
  timeout, awaits the io loop bounded by a shutdown timeout, force-kills and reaps
  on overrun).

All RPC is generic/untyped (`Neovim::call`).

## `api.rs`

The FFI-shaped front-end surface. Documented in detail in the crate
[`README.md`](../README.md). In brief: `WolfCore` owns the tokio runtime, the
`NvimProcess`, the `SharedSnapshot`, and the ordered input channel; `CoreConfig`,
`CoreError`, and `SnapshotReadGuard` (an owned read guard that `Deref`s to
`Snapshot`) round out the surface. A single `INPUT_COALESCE_DELAY` lets a
synchronous burst of `send_input` calls coalesce into one applied batch.
