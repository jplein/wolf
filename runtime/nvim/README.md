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

## Deferred

Treesitter grammars (`markdown` / `markdown_inline`), a Markdown LSP (e.g.
`marksman`), and a formatter are **deferred to a later phase** (plugin baking via
`wrapNeovim`). Phase 1 relies on Neovim's built-in markdown syntax highlighting,
which is enough to exercise the redraw/highlight pipeline and its tests.
