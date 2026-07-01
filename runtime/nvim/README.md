# runtime/nvim

The in-repo Neovim configuration for Wolf's captive editor. It is version-controlled alongside the code that depends on it, so editor behavior is pinned to the core.

## `init.lua`

Loaded by the captive Neovim that `wolf-core` spawns:

```
nvim --embed --clean -u runtime/nvim/init.lua -i NONE -n
```

`--clean` already skips the user's config, plugins, and shada; core additionally pins `NVIM_APPNAME=wolf` and the `XDG_*` dirs away from the user's from the spawning side (see [`src/nvim.rs`](../../crates/wolf-core/src/nvim.rs)). `init.lua` itself sets options that make the captive editor **hermetic**, **Markdown-only**, and **deterministic for headless rendering tests**.

What it sets and why:

- **Hermetic / no on-disk side effects** — `swapfile = false`, `shadafile = "NONE"`, `undofile = false`, `backup = false`, `writebackup = false`. The captive editor must never touch the user's state.
- **Clean initial grid** — `shortmess += "I"` suppresses the intro message so it does not pollute the first frame.
- **True color** — `termguicolors = true`, so `hl_attr_define` / `default_colors_set` carry RGB values the front-ends paint directly.
- **Chrome-free, deterministic grid** — `number = false`, `signcolumn = "no"`, `ruler = false`, `showcmd = false`, `laststatus = 0` (the front-end draws its own chrome).
- **Soft-wrap (word boundaries)** — `wrap = true` with `linebreak = true`, so long Markdown lines wrap at spaces/punctuation rather than running off-screen or breaking mid-word. Determinism-sensitive rendering tests keep their content within the grid width, so column assertions stay stable.
- **Smooth scroll** — `smoothscroll = true`, so scrolling (mouse wheel, `<C-e>`/`<C-d>`) moves by SCREEN line rather than jumping past a whole wrapped logical line — pairs with soft-wrap to keep long wrapped Markdown lines from skipping fragments while scrolling.
- **Mouse** — `mouse = "a"`; the front-end translates OS mouse events into core input, which forwards them to Neovim.
- **Markdown** — `.md` files are mapped to the `markdown` filetype, and `syntax on` enables built-in syntax highlighting so the grid carries varied `hl_id`s.
- **Vendored orlando plugin (conditional load)** — `init.lua` self-locates its own directory and looks for a sibling `orlando/` tree. When present — the case in the macOS `.app` bundle, where `//bazel/orlando` lands the plugin at `Contents/Resources/orlando/` next to this `init.lua` — it is prepended to `runtimepath` and `plugin/orlando.lua` is sourced deterministically (necessary because `--clean` does not scan plugin dirs, so a plain `runtimepath` prepend alone would not auto-source it; orlando guards on `vim.g.loaded_orlando`, so the explicit source is safe). When absent (Linux/cargo, and any test that does not stage it) the whole block is a strict no-op — no `runtimepath` change, no error. orlando's markdown `wrap_indent` is **additive** to the wrap / linebreak / smoothscroll settings above (it also sets `wrap` / `linebreak` / `breakindent`), so it is harmless alongside them.

### Current-file tracking — `_G.wolf_setup_current_file_tracking(chan)`

`init.lua` defines a global helper that `wolf-core` invokes over RPC **after** `ui_attach`, passing core's own RPC channel id (see [`src/nvim.rs`](../../crates/wolf-core/src/nvim.rs)'s `setup_current_file_tracking`). It is the front-end's only way to learn the active buffer's path, which the GNOME file-tree sidebar uses to highlight and reveal the current file (including after a Neovim-driven `:e`).

The helper:

- creates a `WolfCurrentFile` augroup (`clear = true`, so re-invoking is idempotent);
- registers a `BufEnter` / `BufWinEnter` / `BufFilePost` autocmd whose callback does `vim.rpcnotify(chan, "wolf_current_file", vim.fn.expand("%:p"))` — i.e. notifies core of the active buffer's absolute path on every buffer switch / rename;
- emits once immediately (the same `rpcnotify`) so the startup buffer is reflected without waiting for a buffer switch.

An **unnamed buffer** yields an empty string from `expand("%:p")`, which core maps to "no current file" (`Snapshot.current_file = None`). Core canonicalizes any non-empty path before storing it. The channel id is passed as a Lua **vararg** (`...`) from the `nvim_exec_lua` call, never interpolated into the code string.

### Directory tracking — `_G.wolf_setup_dir_tracking(chan)`

A third global helper that `wolf-core` invokes over RPC **after** `ui_attach`, current-file tracking, and layout setup, again passing core's own RPC channel id (see [`src/nvim.rs`](../../crates/wolf-core/src/nvim.rs)'s `setup_dir_tracking`). It lets core follow Neovim's live working directory so the file picker's MRU cache resolves to the right per-project store (`<nvim-cwd>/.wolf/mru.sqlite`) for BOTH seeding the picker and recording opened files.

The helper:

- creates a `WolfDir` augroup (`clear = true`, so re-invoking is idempotent);
- registers a `DirChanged` autocmd (`pattern = "*"`, covering `:cd` / `:lcd` / `:tcd`) whose callback does `vim.rpcnotify(chan, "wolf_cwd", vim.fn.getcwd())` — i.e. notifies core of the new working directory on every directory change;
- emits once immediately (the same `rpcnotify`) so the startup cwd is reflected without a `:cd`.

Core stores this in a sibling `TrackedCwd` cell (NOT the render snapshot, and it does NOT bump the snapshot `seq`), since nvim's cwd is not render state — the `:cd` that triggered it already produces its own redraw flush. The channel id is passed as a Lua **vararg** (`...`), never interpolated into the code string.

### Split-panes layout — `_G.wolf_setup_layout(chan)`

A second global helper that `wolf-core` invokes over RPC **after** `ui_attach` and current-file tracking, again passing core's own RPC channel id (see [`src/nvim.rs`](../../crates/wolf-core/src/nvim.rs)'s `setup_layout`). Under the split-panes design the **SHELL owns the pane layout**, so Neovim's split / close / focus chords and the `:split` / `:vsplit` / `:close` Ex-commands are rebound to report layout **intents** over `rpcnotify` rather than (or in addition to) performing the native window op. The GNOME front-end consumes these intents to create, destroy, and focus native panes.

The asymmetry (decision D4):

- **SPLIT** — `do_split(orient)` captures the window current BEFORE the split, performs the real `:split` (`orient == "h"`, stacked / Horizontal divider) or `:vsplit` (`"v"`, side-by-side / Vertical divider) so nvim creates the window + grid and emits `win_pos`, then `rpcnotify`s `wolf_layout { action = "split", orientation, from, win }` so the shell learns the orientation, the origin window, and the new window.
- **CLOSE** — `do_close()` performs a guarded `nvim_win_close(0, false)` only when more than one window remains in the tab page (so it never closes the last window); the shell observes the removal via the `win_close` redraw event, so **no intent** is emitted.
- **FOCUS move** — `do_focus(dir)` is a PURE intent: it does NOT change nvim focus, only `rpcnotify`s `wolf_layout { action = "focus_dir", dir, from }`; the shell resolves the neighbour in its own layout and drives the focus change.

The three helpers are also exposed as globals (`_G.wolf_do_split` / `wolf_do_close` / `wolf_do_focus`) so the Ex-command abbreviations can call them.

Bindings the helper installs:

- **Normal-mode chords (the robust primary path)** via `vim.keymap.set` with `noremap` (so a focus chord is genuinely a no-op on nvim — a pure intent): `<C-w>s` / `<C-w><C-s>` → split `h`; `<C-w>v` / `<C-w><C-v>` → split `v`; `<C-w>c` / `<C-w>q` → close; `<C-w>h` / `<C-w><Left>` → focus left; `<C-w>j` / `<C-w><Down>` → down; `<C-w>k` / `<C-w><Up>` → up; `<C-w>l` / `<C-w><Right>` → right.
- **Ex-command overrides** via `cnoreabbrev` (the fragile, WHOLE-WORD-only path, guarded by `getcmdtype() == ':'` so it never fires inside a search or a larger command): `split` / `sp` / `new` → split `h`; `vsplit` / `vs` / `vnew` → split `v`; `close` → close. (`:new` / `:vnew` are treated as plain splits.)
- **Active-window tracking** — a `WolfLayout` augroup with a `WinEnter` autocmd that `rpcnotify`s `wolf_active_win` with `nvim_get_current_win()`, emitting once immediately so the startup window is reflected. Core folds this into `Snapshot.active_win`. (Current-file tracking already follows focus via `BufEnter`, so `current_file` tracks the active pane with no extra work.)

Core tolerates any malformed `wolf_layout` payload (it parses to `None` and is dropped). The channel id is passed as a Lua **vararg**, never interpolated.

## Deferred

Plugin baking is no longer fully deferred: **orlando** is the first vendored plugin (bundled into the macOS `.app` via `//bazel/orlando`, loaded conditionally as described above). A Markdown LSP (e.g. `marksman`) and a formatter remain **deferred to a later phase** (full plugin baking via `wrapNeovim`). Phase 1 otherwise relies on Neovim's built-in markdown syntax highlighting and the `markdown` / `markdown_inline` treesitter parsers, which is enough to exercise the redraw/highlight pipeline and its tests.
