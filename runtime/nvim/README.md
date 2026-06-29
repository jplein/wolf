# runtime/nvim

The in-repo Neovim configuration for Wolf's captive editor. It is
version-controlled alongside the code that depends on it, so editor behavior is
pinned to the core.

## `init.lua`

Loaded by the captive Neovim that `wolf-core` spawns:

```
nvim --embed --clean -u runtime/nvim/init.lua -i NONE -n
```

`--clean` already skips the user's config, plugins, and shada; core additionally
pins `NVIM_APPNAME=wolf` and the `XDG_*` dirs away from the user's from the
spawning side (see [`src/nvim.rs`](../../crates/wolf-core/src/nvim.rs)). `init.lua`
itself sets options that make the captive editor **hermetic**, **Markdown-only**,
and **deterministic for headless rendering tests**.

What it sets and why:

- **Hermetic / no on-disk side effects** — `swapfile = false`,
  `shadafile = "NONE"`, `undofile = false`, `backup = false`,
  `writebackup = false`. The captive editor must never touch the user's state.
- **Clean initial grid** — `shortmess += "I"` suppresses the intro message so it
  does not pollute the first frame.
- **True color** — `termguicolors = true`, so `hl_attr_define` /
  `default_colors_set` carry RGB values the front-ends paint directly.
- **Chrome-free, deterministic grid** — `number = false`, `signcolumn = "no"`,
  `ruler = false`, `showcmd = false`, `laststatus = 0` (the front-end draws its
  own chrome); `wrap = false` so test column assertions are deterministic.
- **Mouse** — `mouse = "a"`; the front-end translates OS mouse events into core
  input, which forwards them to Neovim.
- **Markdown** — `.md` files are mapped to the `markdown` filetype, and
  `syntax on` enables built-in syntax highlighting so the grid carries varied
  `hl_id`s.

### Current-file tracking — `_G.wolf_setup_current_file_tracking(chan)`

`init.lua` defines a global helper that `wolf-core` invokes over RPC **after**
`ui_attach`, passing core's own RPC channel id (see
[`src/nvim.rs`](../../crates/wolf-core/src/nvim.rs)'s
`setup_current_file_tracking`). It is the front-end's only way to learn the active
buffer's path, which the GNOME file-tree sidebar uses to highlight and reveal the
current file (including after a Neovim-driven `:e`).

The helper:

- creates a `WolfCurrentFile` augroup (`clear = true`, so re-invoking is
  idempotent);
- registers a `BufEnter` / `BufWinEnter` / `BufFilePost` autocmd whose callback
  does `vim.rpcnotify(chan, "wolf_current_file", vim.fn.expand("%:p"))` — i.e.
  notifies core of the active buffer's absolute path on every buffer switch /
  rename;
- emits once immediately (the same `rpcnotify`) so the startup buffer is reflected
  without waiting for a buffer switch.

An **unnamed buffer** yields an empty string from `expand("%:p")`, which core maps
to "no current file" (`Snapshot.current_file = None`). Core canonicalizes any
non-empty path before storing it. The channel id is passed as a Lua **vararg**
(`...`) from the `nvim_exec_lua` call, never interpolated into the code string.

### Split-panes layout — `_G.wolf_setup_layout(chan)`

A second global helper that `wolf-core` invokes over RPC **after** `ui_attach` and
current-file tracking, again passing core's own RPC channel id (see
[`src/nvim.rs`](../../crates/wolf-core/src/nvim.rs)'s `setup_layout`). Under the
split-panes design the **SHELL owns the pane layout**, so Neovim's split / close /
focus chords and the `:split` / `:vsplit` / `:close` Ex-commands are rebound to
report layout **intents** over `rpcnotify` rather than (or in addition to)
performing the native window op. The GNOME front-end consumes these intents to
create, destroy, and focus native panes.

The asymmetry (decision D4):

- **SPLIT** — `do_split(orient)` captures the window current BEFORE the split,
  performs the real `:split` (`orient == "h"`, stacked / Horizontal divider) or
  `:vsplit` (`"v"`, side-by-side / Vertical divider) so nvim creates the window +
  grid and emits `win_pos`, then `rpcnotify`s
  `wolf_layout { action = "split", orientation, from, win }` so the shell learns
  the orientation, the origin window, and the new window.
- **CLOSE** — `do_close()` performs a guarded `nvim_win_close(0, false)` only when
  more than one window remains in the tab page (so it never closes the last
  window); the shell observes the removal via the `win_close` redraw event, so
  **no intent** is emitted.
- **FOCUS move** — `do_focus(dir)` is a PURE intent: it does NOT change nvim focus,
  only `rpcnotify`s `wolf_layout { action = "focus_dir", dir, from }`; the shell
  resolves the neighbour in its own layout and drives the focus change.

The three helpers are also exposed as globals (`_G.wolf_do_split` /
`wolf_do_close` / `wolf_do_focus`) so the Ex-command abbreviations can call them.

Bindings the helper installs:

- **Normal-mode chords (the robust primary path)** via `vim.keymap.set` with
  `noremap` (so a focus chord is genuinely a no-op on nvim — a pure intent):
  `<C-w>s` / `<C-w><C-s>` → split `h`; `<C-w>v` / `<C-w><C-v>` → split `v`;
  `<C-w>c` / `<C-w>q` → close; `<C-w>h` / `<C-w><Left>` → focus left;
  `<C-w>j` / `<C-w><Down>` → down; `<C-w>k` / `<C-w><Up>` → up;
  `<C-w>l` / `<C-w><Right>` → right.
- **Ex-command overrides** via `cnoreabbrev` (the fragile, WHOLE-WORD-only path,
  guarded by `getcmdtype() == ':'` so it never fires inside a search or a larger
  command): `split` / `sp` / `new` → split `h`; `vsplit` / `vs` / `vnew` →
  split `v`; `close` → close. (`:new` / `:vnew` are treated as plain splits.)
- **Active-window tracking** — a `WolfLayout` augroup with a `WinEnter` autocmd
  that `rpcnotify`s `wolf_active_win` with `nvim_get_current_win()`, emitting once
  immediately so the startup window is reflected. Core folds this into
  `Snapshot.active_win`. (Current-file tracking already follows focus via
  `BufEnter`, so `current_file` tracks the active pane with no extra work.)

Core tolerates any malformed `wolf_layout` payload (it parses to `None` and is
dropped). The channel id is passed as a Lua **vararg**, never interpolated.

## Deferred

Treesitter grammars (`markdown` / `markdown_inline`), a Markdown LSP (e.g.
`marksman`), and a formatter are **deferred to a later phase** (plugin baking via
`wrapNeovim`). Phase 1 relies on Neovim's built-in markdown syntax highlighting,
which is enough to exercise the redraw/highlight pipeline and its tests.
