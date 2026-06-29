# wolf-core/src

The `wolf-core` source modules. Each is a single file (not a subdirectory). Data
flows: `nvim` spawns the captive Neovim and hands its `redraw` stream to a
`redraw::RedrawHandler`, which folds events into the `render` snapshot (and parses
the Neovim→shell `wolf_layout` intents into `layout::LayoutIntent`s on a sibling
queue); `input` translates abstract events back into RPC; `api` ties it together
behind the FFI-shaped `WolfCore`. The file picker adds three more modules: `mru`
(the SQLite most-recently-used cache), `files` (the async streaming file-list
traversal + the `FileSearch` session), and `rank` (the pure fuzzy-match/ranking
function the front-end calls to order results).

Dependency order: `render` → `layout` → `input` → `redraw` → `nvim` → `files` →
`mru` → `rank` → `api`. (`layout` is the pure intent types parsed by `redraw`;
`files` uses `redraw::WakeFn`; both `mru` and `rank` build on `files::FileEntry` —
`FileEntry`'s home is `files.rs`, re-exported from `mru` and the crate root.)

## `lib.rs`

Crate root. Declares the public modules (`api`, `files`, `input`, `layout`,
`mru`, `nvim`, `rank`, `redraw`, `render`) and re-exports the public surface:
`WolfCore`, `CoreConfig`, `CoreError`, `SnapshotReadGuard`; `FileEntry`,
`FileSearch`, `FileSearchReadGuard` (from `files`); `FocusDir`, `LayoutIntent`,
`SplitOrientation` (from `layout`); `MruStore`, `MruError`, `MRU_LIMIT` (from
`mru`); `rank_matches`, `Match`, `MatchKey` (from `rank`); the input types and
`key_to_notation`; `WakeFn`; and the render-state types. Documents the one hard
rule (no GUI toolkit in the dependency tree).

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
  cell_percentage, attr_id }`, `Cursor { grid, row, col, mode_idx }` — `grid` is
  the grid id the cursor lives on (the active window grid; set by
  `grid_cursor_goto`).
- `Viewport { topline, botline, curline, linecount }` (all `u64`, as nvim
  reports; `topline` is 1-based).
- `WinInfo { grid, win: i64, start_row, start_col, width, height, viewport,
  hidden }` — per-window layout + viewport, learned from `win_pos`/`win_viewport`.
  Keyed in `Snapshot.windows` by the window's grid id; `win` is the Neovim window
  handle as an `i64` (the same form used for `nvim_set_current_win`/`win_close`
  later), decoded by `redraw.rs`'s `ext_handle`.
- **Multigrid model.** Under `ext_multigrid` the snapshot holds MULTIPLE grids:
  `Snapshot { grids: HashMap<u64, Grid>, windows: HashMap<u64, WinInfo>,
  highlights, cursor, mode_infos, active_win: Option<i64>, current_file, seq }`.
  - `grids` always contains the outer grid (`OUTER_GRID_ID = 1`, the global grid
    Neovim resizes and where the command line/messages live); editor window
    content lives on grids `>= 2`. Grid 1 is never painted as window content.
  - `windows` maps each window's grid id to its `WinInfo`.
  - `active_win` is the active Neovim window handle, or `None` if not yet known.
    It is updated out-of-band via the `wolf_active_win` notification (the
    `WinEnter` autocmd; see `redraw.rs`), so the front-end can mark the active
    pane. (`Snapshot::new` starts it at `None`.)
  - `current_file: Option<PathBuf>` is the absolute, canonicalized path of the
    buffer currently being edited (or `None` for an unnamed buffer), updated
    out-of-band via the `wolf_current_file` notification (see `redraw.rs`).
  - `seq` is a monotonic frame counter bumped on each `flush` (a `current_file`
    change does NOT bump it).
  - Accessors: `Snapshot::new(cols, rows)` (inserts grid 1 sized `cols*rows`,
    `cursor.grid = 1`, empty `windows`, `active_win = None`); `active_grid_id()`
    (cursor's grid if present, else the highest non-1 grid id, else 1, else 0);
    `active_grid() -> &Grid` (the active grid, or a shared empty `0x0` grid
    fallback); `grid(id) -> Option<&Grid>`; `grid_mut(id) -> &mut Grid`
    (get-or-create a `0x0` grid a later `grid_resize` sizes); `active_viewport()
    -> Viewport`; `grid_for_win(win: i64) -> Option<u64>`.
- `SharedSnapshot` — `Arc<RwLock<Snapshot>>` wrapper: `new`, `read()`,
  `read_arc()` (owned guard), `write()`, `write_async()` (cooperative, non-pinning
  acquire from async code), `clone_handle()`. `OwnedSnapshotReadGuard` is the
  owned read-guard type returned by `read_arc`.

**Locking invariant:** exactly one `RwLock`; the redraw handler takes the write
lock once per batch, never across an `.await`, and wakes only after it drops.

## `layout.rs`

The Neovim→shell **INTENT CHANNEL** types. The shell owns the pane layout, so
`runtime/nvim/init.lua` overrides the split/focus chords and Ex-commands to
`rpcnotify` a `wolf_layout` payload; `redraw.rs` parses each payload into a
`LayoutIntent` and pushes it onto a drainable queue the front-end consumes via
`WolfCore::drain_intents`.

- `SplitOrientation` (`Horizontal` = panes stacked top/bottom, `Vertical` = side
  by side).
- `FocusDir` (`Left`/`Right`/`Up`/`Down`).
- `LayoutIntent` — `Split { orientation, from, win }` (a split nvim already
  PERFORMED: `from` is the window that was current before, `win` the new window;
  the shell splits the `from` pane and binds the new pane to `win`) and
  `FocusDir { dir, from }` (a PURE focus-move request: nvim did NOT change focus;
  the shell resolves the neighbour and drives the change). A CLOSE has no
  intent — nvim performs a guarded `nvim_win_close` and the shell observes the
  `win_close` redraw event.
- `LayoutIntent::from_payload(&rmpv::Value) -> Option<Self>` — parses the single
  map argument by its `action` discriminator (`"split"` / `"focus_dir"`),
  tolerating any malformed/partial payload by returning `None`. Window handles
  arrive as plain integers (Lua numbers) but the msgpack-`Ext` encoding is also
  tolerated (private `handle_i64`). Table-driven `#[cfg(test)]` unit tests cover
  valid splits/focus and every malformed case.

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
  so the local grid is never resized here); `ResizeGrid { grid, cols, rows }` →
  `nvim_ui_try_resize_grid` (per-window-grid resize); `FocusWin { win }` →
  `nvim_set_current_win` (focus a pane's window, the `win` handle as a plain
  `i64`); `Open` → `edit <fnameescape path>`; `Save` → `write`; `SaveAs` →
  `saveas <fnameescape path>`.
- `apply_batch(nvim, &[InputEvent])` (crate-internal) — applies a batch in order,
  fusing a run of consecutive `Key` events into one `nvim_input` call (so a burst
  yields a single redraw flush); non-key events are applied individually.

All RPC is generic/untyped; key delivery flows through a single shared
`nvim_input` helper. Paths are escaped with a Vim-style `fnameescape`.

## `redraw.rs`

The `redraw` handler: folds Neovim's redraw stream into the shared snapshot.

- `WakeFn = Arc<dyn Fn() + Send + Sync>` — fired once per `flush`, after the lock
  drops.
- `IntentQueue = Arc<Mutex<VecDeque<LayoutIntent>>>` — the shared, drainable queue
  of layout intents. It is a SIBLING of the render snapshot (NOT part of it):
  pushing an intent does not bump the snapshot `seq`. Owned by `WolfCore`
  (drained via `drain_intents`) and cloned into the handler.
- `RedrawHandler { snapshot, intents, wake }` — `RedrawHandler::new(SharedSnapshot,
  IntentQueue, WakeFn)`. Implements `nvim_rs::Handler` with `type Writer =
  Compat<ChildStdin>`; `handle_notify` routes four notifications and ignores the
  rest: `"redraw"` → `apply_redraw`; `CURRENT_FILE_EVENT` (`"wolf_current_file"`)
  → `apply_current_file`; `ACTIVE_WIN_EVENT` (`"wolf_active_win"`) →
  `apply_active_win`; and `LAYOUT_EVENT` (`"wolf_layout"`) →
  `LayoutIntent::from_payload` then push onto the intent queue. Every path fires
  the wake AFTER its lock (snapshot write or intent `Mutex`) drops.

`apply_redraw` takes the write lock once via `write_async`, walks the batch, and
returns whether a `flush` was seen so the caller can wake after the lock drops.

**Multigrid routing.** The grid events carry their target grid id in `params[0]`
and are routed to that grid: `grid_resize` (`grid_mut(id).resize(..)` —
get-or-create), `grid_clear`/`grid_line`/`grid_scroll` (operate on the existing
grid; an unknown grid id is dropped quietly so no phantom grid is created),
`grid_cursor_goto` (sets `cursor.grid` + row/col). Window events: `grid_destroy`
(removes the grid and its `windows` entry), `win_pos` (upserts `windows[grid]`
with the decoded `win` + layout rect), `win_viewport` (upserts the per-window
`viewport`), `win_hide` (sets `hidden`), `win_close` (removes both the `windows`
entry and the grid — idempotent with the `grid_destroy` Neovim also emits).
`win_float_pos`, `win_external_pos`, and `msg_set_pos` are intentionally ignored.
`grid_line` walks cell tuples `[text]` / `[text, hl]` / `[text, hl, repeat]`,
carrying `hl_id` forward, expanding `repeat`, and storing Neovim's empty trailing
cell of a wide glyph **verbatim** (no synthesized placeholder). Non-grid events:
`hl_attr_define`, `default_colors_set`, `mode_info_set`, `mode_change`, and
`flush` (bumps `seq`). Unknown events are ignored.

`ext_handle(&rmpv::Value) -> Option<i64>` decodes a Neovim window handle, which
`nvim-rs` delivers either as a plain `Value::Integer` or as a `Value::Ext` (type
1 = Window) wrapping a msgpack-encoded integer; the inner bytes are decoded with
`rmpv::decode::read_value`. The handle is stored as `i64` so the same form serves
`nvim_set_current_win`/`nvim_win_close` later.

**Current-file tracking.** `CURRENT_FILE_EVENT = "wolf_current_file"` names an
out-of-band notification carrying the active buffer's absolute path
(`expand('%:p')`), emitted by the `WolfCurrentFile` autocmd in
`runtime/nvim/init.lua`. `apply_current_file` parses the first argument: an empty
string (unnamed buffer) becomes `None`; otherwise the path is canonicalized (to
match the canonical paths the file picker emits, falling back to the raw path if
canonicalization fails) and stored in `Snapshot.current_file`. It acquires the
write lock via `write_async` and does **not** hold it across an `.await`; it does
**not** bump `seq` (a current-file change is not a redraw frame). The lock drops
before returning, then `handle_notify` fires the wake — same wake-after-unlock
invariant as the redraw path — so the front-end re-reads the snapshot (e.g. to
highlight/reveal the file tree's current file).

**Active-window + layout intents.** Two more out-of-band notifications come from
the `WolfLayout` setup in `runtime/nvim/init.lua`. `ACTIVE_WIN_EVENT =
"wolf_active_win"` carries the active window handle (from a `WinEnter` autocmd);
`apply_active_win` decodes it via `ext_handle` and stores it in
`Snapshot.active_win` (no `seq` bump). `LAYOUT_EVENT = "wolf_layout"` carries a
split/focus payload; `handle_notify` parses it with `LayoutIntent::from_payload`
and pushes the result onto the sibling `IntentQueue` (a brief `Mutex`, never held
across an `.await`, no `seq` bump). Both wake the front-end after their lock drops
so it can re-read `active_win` and `drain_intents`.

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
  (attaches `ext_linegrid` + `ext_multigrid` + `rgb`), `try_resize(cols, rows)`
  (`nvim_ui_try_resize`, drives the outer grid 1), `try_resize_grid(grid, cols,
  rows)` (`nvim_ui_try_resize_grid`, drives a single window grid),
  `setup_current_file_tracking()` (see below), `setup_layout()` (see below), and
  `shutdown(self)` (sends `qa!` bounded by a grace timeout, awaits the io loop
  bounded by a shutdown timeout, force-kills and reaps on overrun).
- `setup_current_file_tracking(&self) -> Result<(), NvimError>` — installs the
  current-file autocmd inside the captive nvim. It looks up this RPC channel's id
  via `nvim_get_api_info` (which returns `[channel_id, metadata]`), then calls the
  `_G.wolf_setup_current_file_tracking` helper (defined in
  `runtime/nvim/init.lua`) over `nvim_exec_lua`, passing the channel id as a Lua
  vararg (`...`) rather than interpolating it into the code string (no
  injection/formatting hazard). The helper registers a
  `BufEnter`/`BufWinEnter`/`BufFilePost` autocmd that `rpcnotify`s
  `wolf_current_file` and emits once immediately. `api::WolfCore::start` calls
  this after `ui_attach`, so the front-end learns the active buffer's path as soon
  as the UI is attached and on every later buffer switch / rename.
- `setup_layout(&self) -> Result<(), NvimError>` — installs the split-panes
  layout overrides inside the captive nvim. Like the current-file setup, it looks
  up this RPC channel's id via `nvim_get_api_info`, then calls the
  `_G.wolf_setup_layout` helper (defined in `runtime/nvim/init.lua`) over
  `nvim_exec_lua`, passing the channel id as a Lua vararg. The helper rebinds the
  `<C-w>` split/close/focus chords and the `:split`/`:vsplit`/`:close` Ex-commands
  to `rpcnotify` `wolf_layout` intents, and registers a `WinEnter` autocmd that
  `rpcnotify`s `wolf_active_win`. `api::WolfCore::start` calls it after
  `setup_current_file_tracking`. (There is no standalone `set_current_win` method:
  pane focus flows through the ordered input pump as `InputEvent::FocusWin`, so it
  never races pending keystrokes.)

All RPC is generic/untyped (`Neovim::call`).

## `files.rs`

The async, streaming file-list traversal and its `FileSearch` session. This is
the picker's view of "all files under the search root": the synchronous MRU seed
plus the background walk's results, streamed in as they are found.

Key types:

- `FileEntry { path: PathBuf, display: String, mru_rank: Option<u32> }` — the
  canonical home of `FileEntry` (re-exported from `mru` and the crate root).
  `path` is absolute (used to issue `InputEvent::Open`); `display` is the path
  relative to the search root (what the UI shows and `rank` ranks); `mru_rank`
  is `Some(0)` for the most recently opened file, `None` if not in the MRU.
- `FileSearchState { entries: Vec<FileEntry>, complete: bool, version: u64, .. }`
  — the collected state behind one `RwLock`. `entries` is the MRU seed first,
  then streamed traversal results in discovery order; `complete` flips true once
  the walk finishes; `version` is a monotonic counter bumped on every append so
  readers can detect change. A private `seen: HashSet<PathBuf>` dedups the MRU
  seed against rediscovered files. `FileSearchState::seeded(seed)` (crate-internal)
  builds the initial state at `version = 1`, `complete = false`.
- `FileSearchReadGuard` — an OWNED read guard (`ArcRwLockReadGuard`) that `Deref`s
  to `FileSearchState`, so a reader holds `guard.entries` / `.complete` /
  `.version` independently of the `FileSearch`'s lifetime (mirrors
  `OwnedSnapshotReadGuard`).
- `FileSearch { .. }` — a live search session: `read() -> FileSearchReadGuard`,
  `cancel()` (sets an `AtomicBool`), and `Drop` also cancels (so closing the
  picker or starting a new search stops the old walk promptly).
  `FileSearch::from_parts(state, cancel)` is crate-internal.

`spawn_traversal(root, state, cancel, wake, handle)` (crate-internal) runs the
walk on `handle.spawn_blocking` (the `ignore` walk is blocking IO). It appends
found files in `TRAVERSAL_BATCH` (256) chunks — bumping `version` and calling
`wake` **after** the write lock drops — checks `cancel` each iteration, and on
finish appends the remainder, sets `complete` (unless cancelled), and fires a
final `wake`. The write lock is held only for the short, IO-free append.

**Traversal rules (plan §D3/§D6).** The `ignore` crate (ripgrep's walker) is
configured `parents(true)`, `hidden(false)`, `git_ignore`/`git_global`/
`git_exclude(true)`, `ignore(false)`, `require_git(false)`, `follow_links(false)`
— behaving like `git ls-files --others --exclude-standard` inside a repo and a
plain recursive walk outside one. So: dotfiles are shown (only `.gitignore`
filters), `.gitignore` is honored in the cwd or any ancestor up to the repo root,
untracked-but-not-ignored files are included, symlink loops cannot hang it, and
`.git/` and our own `.wolf/` cache are never descended into (via `filter_entry`,
with a defensive `has_excluded_component` backstop). `is_listable_file` classifies
each entry: plain files are kept, a symlink is kept only when its target is a
plain file (`std::fs::metadata` follows the link), and directories / broken or
directory symlinks are dropped. Display strings are built via `display_for`
(relative to the root using the platform separator).

## `mru.rs`

The most-recently-used file cache: a small SQLite database at
`<cwd>/.wolf/mru.sqlite` so the picker can offer recent files synchronously
(before the async traversal completes) and rank them first.

- `MruStore` (`Clone`, `Arc`-backed) — holds the shared connection
  (`Arc<parking_lot::Mutex<rusqlite::Connection>>`) plus the canonical cwd used
  to relativize stored paths into `FileEntry::display`.
- `MruError` (`thiserror`) — `Open` (open/init failed), `Io` (`.wolf` dir),
  `Query` (a query failed).
- `MruStore::open(cwd: &Path) -> Result<MruStore, MruError>` — creates the
  `.wolf` directory, canonicalizes the cwd, opens the database, enables WAL
  journaling + a `busy_timeout` (`MRU_BUSY_TIMEOUT_MS = 2000`), and ensures the
  `mru(path TEXT PRIMARY KEY, opened_at INTEGER NOT NULL)` table exists.
- `record(&self, path: &Path)` — **fire-and-forget**: upserts `(path,
  opened_at = now-millis)`, storing the canonical path if the file exists else
  the path absolutized against the cwd; errors are logged via `tracing`, never
  propagated, so opening a file is never blocked by a cache hiccup.
- `recent(&self) -> Vec<FileEntry>` — the most-recently-opened files newest-first
  (`ORDER BY opened_at DESC, rowid DESC LIMIT MRU_LIMIT`, `MRU_LIMIT = 50`).
  Entries whose backing file no longer exists are **pruned inline** (omitted from
  the result AND `DELETE`d via the same connection); `mru_rank` is the 0-based
  position in the returned list.

`MRU_LIMIT` is re-exported from the crate root. `FileEntry` is re-exported here
from `files.rs` (its true home). All SQLite work is synchronous, guarded by the
mutex, with short critical sections that never span an `.await`.

## `rank.rs`

Pure, IO-free fuzzy-match + ranking for the picker — unit-tested headlessly (no
filesystem, Neovim, or GUI), mirroring how the GNOME `geometry.rs` / `cursor.rs`
seams stay testable.

- `rank_matches(query: &str, entries: &[FileEntry]) -> Vec<Match>` (`#[must_use]`)
  — an EMPTY query returns ALL entries **MRU-first** (ascending `mru_rank`, then
  non-MRU entries in stable original order); a NON-EMPTY query keeps only entries
  whose `display` contains `query` as a **case-insensitive subsequence** (both
  sides ASCII-lowercased) and orders them by `MatchKey`.
- `Match { index: usize, key: MatchKey }` — `index` points into the input
  `entries` slice (callers map results back by index); `key` is the sort key.
- `MatchKey` (derived `Ord`, every field "lower = better", declared in priority
  order): (1) `mru_group` (`0` if in the MRU cache else `1` — **MRU first, LOCKED**),
  (2) `mru_rank` (`u32::MAX` if not in MRU), (3) `prefix_group` (`0` if the query
  is a ci-prefix of some filename word else `1`), (4) `word_index` of the earliest
  prefixed word, (5) `match_start` (first matched char index), (6) `span`
  (`last - first`), (7) `len` (`display.chars().count()`), (8) `display`
  lexicographic.

A "word" comes from the FILENAME only (the last path component, split on
`/ \ _ - . space`); a directory component contributes no word. "Strict prefix of
a word" is `word.starts_with(query)` case-insensitively, so a full-word match is
still group `0` and wins the later length tie-break against a longer word sharing
the prefix. Word splitting uses `WORD_SEPARATORS`; the empty-query path is handled
by `rank_empty`. The module has table-driven `#[cfg(test)]` unit tests.

## `api.rs`

The FFI-shaped front-end surface. Documented in detail in the crate
[`README.md`](../README.md). In brief: `WolfCore` owns the tokio runtime, the
`NvimProcess`, the `SharedSnapshot`, the sibling `IntentQueue`, the ordered input
channel, the `MruStore`, and the configured `cwd`; `CoreConfig` (now including
`cwd`), `CoreError` (now including a `Mru` variant), and `SnapshotReadGuard` (an
owned read guard that `Deref`s to `Snapshot`) round out the surface. `start`
attaches the UI then calls `setup_current_file_tracking` and `setup_layout` so the
captive nvim is wired for both current-file and split-panes intents before input
flows. A single `INPUT_COALESCE_DELAY`
lets a synchronous burst of `send_input` calls coalesce into one applied batch;
the input pump also records every drained `InputEvent::Open` into the MRU cache.
`mru_recent()` exposes the cache and `start_file_search(&self, wake) -> FileSearch`
seeds the MRU synchronously then spawns the streaming traversal on the runtime.
`resize(cols, rows)` enqueues a global `Resize` (outer grid 1) and
`resize_grid(grid, cols, rows)` enqueues a per-window-grid `ResizeGrid`, both on
the ordered input channel so they never race pending keystrokes. `drain_intents()
-> Vec<LayoutIntent>` consumes the pending `wolf_layout` intents (in arrival
order) the front-end applies on each wake, and `focus_win(win: i64)` enqueues an
`InputEvent::FocusWin` (so click-to-focus and resolved `FocusDir` moves order with
keystrokes).
