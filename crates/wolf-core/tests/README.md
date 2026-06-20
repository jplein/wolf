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
- `config(cols, rows)` — builds a `CoreConfig` pointing at that `init.lua`.
- `wake_channel()` → `(WakeFn, FlushWaiter)`; `FlushWaiter::wait_for_flush(timeout)`
  blocks until the next flush (panics on timeout/disconnect), `drain()` discards
  queued wakes. The `WakeFn` is an `mpsc::Sender<()>` closure (`Send + Sync`,
  since the wake may fire on a tokio worker thread).
- `temp_md(content)` / `temp_md_path()` — a `.md` temp file with content, or a
  not-yet-created path for `SaveAs`.
- `start_core_with_temp_md[_sized](content[, cols, rows])` — starts a core, opens
  the temp `.md`, and waits for the open to land. `start_core_sized(cols, rows)`
  starts a bare core (no file). 
- Input constructors: `key(Key)`, `ch(char)`, `ctrl(Key)`, `type_str(core, s)`.

## Coverage

Fifteen tests cover the redraw → snapshot and input pipelines against the real
pinned Neovim:

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
