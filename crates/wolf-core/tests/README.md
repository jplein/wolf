# wolf-core/tests

Integration tests for `wolf-core`. They drive a **real, hermetic Neovim** child
process — no mocks, per the project's testing policy (the whole point of core is
to talk to a Neovim process, so the tests actually start one).

## How they run

- Neovim is located via **`WOLF_NVIM_BIN`** (exported by the flake devShell),
  never `PATH`. Enter the devShell (`direnv allow`) before running, then
  `cargo test -p wolf-core`.
- They are fully **headless**: `nvim --embed` speaks msgpack-RPC over stdio and
  needs no TTY, so the suite is CI-safe.
- They **fail loud** if `WOLF_NVIM_BIN` is unset or empty: `require_nvim_bin`
  panics with an actionable message rather than silently skipping, so CI
  misconfiguration surfaces immediately.
- Each test waits for redraw `flush`es with a generous timeout
  (`FLUSH_TIMEOUT = 10s`) to absorb nvim `--embed` startup and the inherently
  asynchronous redraw.

## `common/mod.rs`

Shared helpers used across the test binaries:

- `require_nvim_bin()` — fail-loud `WOLF_NVIM_BIN` check; returns the path.
- `init_lua_path()` — resolves the in-repo `runtime/nvim/init.lua` from
  `CARGO_MANIFEST_DIR`.
- `fresh_cwd()` — a fresh, persistent temp directory (NOT a dropped guard) to
  serve as a test core's `cwd`, so the MRU cache `.wolf/` and the picker's search
  root are isolated per run.
- `config(cols, rows)` — builds a `CoreConfig` pointing at that `init.lua`, using
  a `fresh_cwd()` as the `cwd`.
- `config_in(cwd, cols, rows)` — same, but with an explicit `cwd` (used by the MRU
  and file-search tests that assert against a known directory layout).
- `wake_channel()` → `(WakeFn, FlushWaiter)`. The `WakeFn` is an
  `mpsc::Sender<()>` closure (`Send + Sync`, since the wake may fire on a tokio
  worker thread) that signals on **every** wake — both real redraw flushes and
  out-of-band wakes such as the `wolf_current_file` notification. The `FlushWaiter`
  is therefore **seq-based** (it keys on the snapshot's `seq`, the redraw-frame
  counter, not on the raw wake count, so a `current_file` wake — which does NOT
  bump `seq` — is consumed and ignored rather than mistaken for a flush):
  - `wait_for_flush(&core, timeout)` blocks until `core`'s snapshot `seq` advances
    beyond its value at the call (panics on timeout/disconnect). Used for
    post-input waits, where the input pump's coalesce delay guarantees the flush
    is still in the future when the baseline is captured.
  - `wait_for_initial_flush(&core, timeout)` blocks until `seq >= 1` (the attach
    flush). It keys on that ABSOLUTE threshold rather than an advance because the
    attach redraw (`seq` 0 → 1) lands DURING `WolfCore::start`, so it usually
    returns immediately; an advance-based wait would baseline at 1 and block
    forever. Used for the startup/attach wait in `start_core_sized` and the first
    wait in `start_core_with_temp_md_sized`. Robust to out-of-band current_file
    wakes.
  - `drain()` discards queued wakes without blocking.
- `temp_md(content)` / `temp_md_path()` — a `.md` temp file with content, or a
  not-yet-created path for `SaveAs`.
- `start_core_with_temp_md[_sized](content[, cols, rows])` — starts a core, opens
  the temp `.md`, and waits for the open to land. `start_core_sized(cols, rows)`
  starts a bare core (no file). 
- Input constructors: `key(Key)`, `ch(char)`, `ctrl(Key)`, `type_str(core, s)`.

## Coverage

Sixty-eight tests total: forty-eight integration tests (ten `tests/*.rs`
binaries, all driving a real pinned Neovim and/or real SQLite/filesystem) plus
twenty pure in-module unit tests (ten for the ranking function in `rank.rs`, ten
for layout-intent parsing in `layout.rs`).

### File picker (real SQLite / filesystem / git / Neovim — no mocks)

- **MRU cache** (`mru.rs`, real SQLite + fs, six tests) — `open` creates
  `.wolf/mru.sqlite` and `recent()` starts empty; three records come back
  most-recent-first with 0/1/2 ranks; re-recording an older path moves it to the
  front without duplicating; deleting a file omits it from `recent()` AND prunes
  its row from the cache; `recent()` is capped at `MRU_LIMIT`; `display` is
  relative to the cwd while `path` is absolute.
- **Open records MRU** (`mru_open_records.rs`, real Neovim, one test) — after
  `start_core_with_temp_md`, `mru_recent()` contains the opened path (the input
  pump recorded the `Open`).
- **Streaming traversal** (`file_search.rs`, real fs + real git via shelled
  `git init`, eight tests, polling until `complete`) — a non-repo lists plain
  files + dotfiles but no directories; symlinks are included only when they target
  a plain file; a symlink loop does not hang; a git repo respects `.gitignore`
  (cwd and nested/ancestor) and excludes `.git/` while including untracked files;
  the `.wolf/` cache is excluded; the MRU seed is present synchronously and not
  duplicated when rediscovered; `cancel` stops streaming.

### Current-file tracking (real Neovim)

`current_file.rs` (four tests) verifies `Snapshot.current_file` against a real,
hermetic nvim. Because the `wolf_current_file` notification is SEPARATE from
redraw flushes, these tests **poll** `read_snapshot().current_file` (draining
wakes) with a generous timeout, canonicalizing both sides (`/tmp` is often
symlinked):

- opening a file via `InputEvent::Open` sets `current_file` to its canonical path;
- opening a file by **typing `:e <path><CR>`** as keystrokes (not via `Open`) also
  sets it — exercising the autocmd reveal path for nvim-driven buffer changes;
- a core started with no file open (unnamed buffer) reports `None` (the initial
  emit's empty `expand('%:p')` maps to `None`);
- `current_file` follows a buffer switch (open A, then open B → it moves to B).

### Ranking (pure, in-module)

`rank.rs` has ten table-driven `#[cfg(test)]` unit tests (no fs/Neovim): empty
query returns all entries MRU-first; case-insensitive matching; subsequence match
and non-match exclusion; MRU outranks a stronger non-MRU match; strict-prefix
beats a scattered subsequence; word splitting on separators; full-word match tops
the prefix group; deterministic length/lexicographic tie-breaks; and the decisive
`match_start` (key 5) and `span` (key 6) tie-breaks.

### Layout-intent parsing (pure, in-module)

`layout.rs` has ten table-driven `#[cfg(test)]` unit tests (no fs/Neovim) for
`LayoutIntent::from_payload`, the `wolf_layout` payload parser: a valid vertical
and horizontal `split` (preserving `from`/`win`); a valid `focus_dir` and that
every `dir` string (`left`/`right`/`up`/`down`) maps to the right `FocusDir`; and
the malformed cases that must all yield `None` — a missing `action`, an unknown
`action`, a `split` missing `from`/`win`, a `split` with a bad `orientation`, a
`focus_dir` with a bad `dir`, and a non-map payload (a bare integer).

### Redraw / input pipeline (real Neovim)

The original fifteen tests cover the redraw → snapshot and input pipelines against
the real pinned Neovim:

- **Spawn / attach** — grid size matches the requested cols/rows; default colors
  populated; `mode_infos` non-empty; clean `shutdown`.
- **Typing** — `i` + text + `Esc` lands the text on the grid; the typed cell's
  `hl_id` resolves to a non-`None` foreground.
- **Open existing Markdown** — an opened `# Title` shows on row 0; a heading's
  `hl_id` differs from body text (built-in markdown syntax).
- **Cursor movement** — `j`/`l` advance `cursor.row`/`cursor.col`; entering insert
  mode selects the insert `ModeInfo` (vertical-bar shape).
- **Resize** — a resize request updates the grid cols/rows
  (`nvim_ui_try_resize` → `grid_resize`).
- **Save / SaveAs** — typed text written with `Save` appears in the file on disk;
  `SaveAs` creates a new path with the content.
- **Scroll / viewport** — scrolling a tall buffer increases `viewport.topline`;
  `win_viewport.linecount` matches the buffer line count.
- **`grid_line` decoding** — direct `[text, hl, repeat]` expansion (column
  advance) and wide-character handling (a double-width glyph followed by Neovim's
  explicit empty trailing cell, with following cells correctly aligned);
  `grid_scroll` content-shift assertions (both directions).
- **`key_to_notation` unit asserts** — special keys, function keys, and
  Alt/Meta modifiers (`<A-x>`, `<D-s>`), plus the printable-char shift contract.

### Multigrid foundation (real Neovim)

`redraw_multigrid.rs` (six tests) drives a real nvim attached with
`ext_multigrid` and asserts the multigrid render model. Because multigrid
notifications (`win_pos` / grid creation / `grid_destroy`) can arrive across
several flushes, these tests **poll** with a generous timeout (draining wakes),
assert on grid/window MAP sizes, MEMBERSHIP, and active-grid behavior — never
literal `WinInfo.win` handle values or HashMap iteration order:

- attaching creates a window grid (`grids` has grid 1 + a window grid `>= 2`,
  `windows` has the active window, `active_grid_id() != 1`);
- a raw `:split<CR>` (real nvim op — no init.lua override in this file) adds a
  grid + window, the new `win` round-tripping through `grid_for_win`;
- `grid_line` lands in the active grid only;
- `:close<CR>` removes the gone grid AND its `windows` entry;
- `:split` + `<C-w>w` moves `cursor.grid` so `active_grid` follows focus;
- `resize_grid` changes only the targeted grid (grid 1 unchanged).

### Layout intents + active-window (real Neovim)

`layout_intents.rs` (eight tests) exercises the Neovim→shell intent channel with
the `init.lua` overrides installed (`setup_layout`). They drain intents / poll the
snapshot with a timeout and use membership/relative assertions (never literal
handle values):

- `<C-w>s` performs a real horizontal split AND emits
  `Split { orientation: Horizontal, .. }` (window count grows; new `win`
  round-trips through `grid_for_win`);
- `<C-w>v` → `Split { Vertical, .. }`;
- typing `:vsplit<CR>` (the fragile `cnoreabbrev` path) → a vertical split intent;
- `<C-w>l` emits `FocusDir { Right, .. }` but, because focus moves are PURE
  intents, nvim focus and `active_win` do NOT change (the load-bearing assertion);
- `focus_win(other)` changes `active_win` and `current_file` follows;
- `<C-w>c` with multiple windows drops the window count (via `win_close`);
- `<C-w>c` on the LAST window is a no-op (the Lua guard) and nvim stays alive;
- a `WinEnter` (split, then `focus_win`) updates `active_win`, always naming a
  window present in `windows`.
