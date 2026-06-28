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

## Deferred

Treesitter grammars (`markdown` / `markdown_inline`), a Markdown LSP (e.g.
`marksman`), and a formatter are **deferred to a later phase** (plugin baking via
`wrapNeovim`). Phase 1 relies on Neovim's built-in markdown syntax highlighting,
which is enough to exercise the redraw/highlight pipeline and its tests.
