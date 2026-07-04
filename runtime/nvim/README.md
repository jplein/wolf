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
- **Vendored lighthouse colorscheme (conditional activation)** — `init.lua` self-locates its own directory and looks for a sibling `lighthouse/` tree. When present — the case in the macOS `.app` bundle, where `//bazel/lighthouse` lands the colorscheme at `Contents/Resources/lighthouse/` next to this `init.lua` — it is prepended to `runtimepath`, then `vim.o.background = "dark"` is set and `vim.cmd.colorscheme("lighthouse")` is run to activate it. Unlike orlando, lighthouse is a **colorscheme** (`colors/lighthouse.lua`), NOT a plugin: a colorscheme on `runtimepath` is inert until it is explicitly selected, so putting the dir on `runtimepath` alone does nothing — `init.lua` must ALSO activate it (`:colorscheme` finds `colors/lighthouse.lua` on `runtimepath`, so no explicit source of the colors file is needed). Ordering matters: this activation runs **after** `syntax on`, because both `:colorscheme` and `syntax on` clear/redefine highlight groups, so the colorscheme must win by running last; `vim.o.background` is set **before** `:colorscheme` so a dual-variant scheme reads it. When absent (Linux/cargo, and any test that does not stage it) the whole block is a strict no-op — no `runtimepath` change, no `:colorscheme` call, no error. A future change will switch dark/light automatically from the OS appearance preference; the activation is kept isolated and simple to make that easy.
- **Vendored treesitter markdown highlighting + fenced-code injection (conditional enablement)** — `init.lua` self-locates its own directory and looks for sibling trees. When the `nvim-treesitter/` queries provider is present — the case in the macOS `.app` bundle, where `//bazel/nvim-treesitter` + `//bazel/treesitter-parsers` land `Contents/Resources/nvim-treesitter/` and `…/parsers/` next to this `init.lua` — the block **enables treesitter**: it prepends `nvim-treesitter/` (the `queries/<lang>/highlights.scm` + `queries/markdown/injections.scm` provider) to `runtimepath`, then prepends `parsers/` (so nvim resolves `<rtp>/parser/<lang>.so` for the embedded fenced languages) and the inert `nvim-treesitter-textobjects/` tree, each only if it independently exists. `markdown` / `markdown_inline` parsers already ship with nvim, so they are NOT re-vendored. It does NOT call nvim-treesitter's `.setup{}` (its `:TSInstall` machinery is non-deterministic under `--clean`); the plugin is used purely as an on-`runtimepath` query provider. A `WolfTreesitter` augroup registers a `FileType=markdown` autocmd that runs a **pcall-wrapped** `vim.treesitter.start(buf)` (a query-loader incompatibility degrades to `syntax on` rather than raising), plus an emit-once loop over already-loaded markdown buffers — so `:e doc.md` lights markdown highlighting AND routes fenced blocks (e.g. a ` ```go ` block) through the injections query on `runtimepath`. The injection is **core-native, capture-based**: it comes from nvim-treesitter's `main`-branch `queries/markdown/injections.scm`, which is why the plugin is pinned to `main` (master's `#set-lang-from-info-string!` predicate is incompatible with nvim 0.12.3's query-match API — see [`bazel/nvim-treesitter/README.md`](../../bazel/nvim-treesitter/README.md)). When the `nvim-treesitter/` sibling is absent (Linux/cargo, and any test that does not stage it) the whole block is a strict no-op — no `runtimepath` change, no autocmd, no error, and `syntax on` remains the highlight source. The `nvim-treesitter-textobjects/` tree is bundled-but-**inert**: it is only prepended to `runtimepath` (no source, no `.setup{}`, no keymaps).

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

### Theme tracking — `_G.wolf_setup_theme_tracking(chan)`

A fourth global helper that `wolf-core` invokes over RPC **after** `ui_attach`, current-file tracking, layout setup, and directory tracking, again passing core's own RPC channel id (see [`src/nvim.rs`](../../crates/wolf-core/src/nvim.rs)'s `setup_theme_tracking`). It lets a colorscheme drive two app-level colors via the optional highlight groups **`WolfCursor`** (bg = app cursor fill, fg = glyph color) and **`WolfTitlebar`** (bg = macOS titlebar background; fg carried for future title-text coloring). These groups are never painted on the grid, so the redraw stream never carries them — this notification is the only way core learns them.

The helper:

- creates a `WolfTheme` augroup (`clear = true`, so re-invoking is idempotent);
- registers a `ColorScheme` autocmd (`pattern = "*"`, covering `:colorscheme` and `:set background=...`, which re-sources the active scheme) whose callback reads both groups via `pcall(vim.api.nvim_get_hl, 0, { name = ..., link = false })` — `link = false` so a linked group resolves to its target's effective attributes — and does `vim.rpcnotify(chan, "wolf_theme", cursor.fg or -1, cursor.bg or -1, titlebar.fg or -1, titlebar.bg or -1)`;
- emits once immediately (the same `rpcnotify`) so the startup colorscheme is reflected without a theme change.

The payload is **four flat scalars** (each a packed `0xRRGGBB` color or `-1` for an unset channel) rather than a table: an absent/cleared group yields an empty Lua table from `nvim_get_hl`, whose msgpack encoding is ambiguous (map vs. array), so flat scalars with a `-1` sentinel sidestep that entirely (nvim colors are 24-bit, making `-1` unambiguous). Core folds this into `Snapshot.theme` (waking the front-end, no `seq` bump). The pinned lighthouse colorscheme defines both groups. The channel id is passed as a Lua **vararg** (`...`), never interpolated into the code string.

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

### Commands — `_G.wolf_setup_commands(chan, commands)`

The last global helper that `wolf-core` invokes over RPC, **after** `ui_attach`, current-file tracking, layout setup, directory tracking, and theme tracking, passing core's own RPC channel id AND the command list (see [`src/nvim.rs`](../../crates/wolf-core/src/nvim.rs)'s `setup_commands`). A **command** is a no-parameter, no-return user action. Unlike layout intents, commands are **only user-triggered** (there is no autocmd and no emit-on-attach). `commands` is a list of `{id, ex_command}` pairs — the canonical registry Rust passes in from `wolf-core`'s `COMMAND_SPECS` (e.g. `{"close_window", "WolfCloseWindow"}`). ONE canonical snake_case `id` derives both faces.

The helper:

- seeds `package.loaded["wolf"] = wolf_tbl` — **required** so `require("wolf")` resolves under `--clean` (Lua's `require` consults `package.loaded` first; a plain global would NOT satisfy `require`);
- for each `{id, ex_command}` pair: sets `wolf_tbl[id] = function() vim.rpcnotify(chan, "wolf_command", id) end` (the **Lua face**, `require("wolf").<id>()`) AND calls `vim.api.nvim_create_user_command(ex_command, <same callback>, { nargs = 0, force = true })` (the **Ex-command face**, `:<ex_command>`).

Both faces `rpcnotify` the same `wolf_command` channel with the canonical `id`; core resolves it via `Command::from_id`, folds it onto a drainable queue (no `seq` bump), and wakes the front-end, which maps each command to a native action (on macOS via the NSMenu / `CommandDispatcher`). An unknown id is dropped by core. The captive nvim stays hermetic: only Wolf's own bundled config uses these faces this cut — a user-config keybinding hook is **deferred**. The channel id and command list are passed as Lua **varargs**, never interpolated.

## Deferred

Plugin baking is no longer fully deferred: **orlando** is the first vendored plugin (bundled into the macOS `.app` via `//bazel/orlando`, loaded conditionally as described above), and **lighthouse** is the second vendored asset — a **colorscheme** (bundled via `//bazel/lighthouse`, activated conditionally as described above). A Markdown LSP (e.g. `marksman`) and a formatter remain **deferred to a later phase** (full plugin baking via `wrapNeovim`). Phase 1 otherwise relies on Neovim's built-in markdown syntax highlighting and the `markdown` / `markdown_inline` treesitter parsers, which is enough to exercise the redraw/highlight pipeline and its tests.

The macOS `.app` **ships** (via `//bazel/nvim-treesitter`, `//bazel/nvim-treesitter-textobjects`, and `//bazel/treesitter-parsers`) the nvim-treesitter + textobjects plugins and a curated set of host-compiled treesitter parsers at `Contents/Resources/nvim-treesitter/`, `…/nvim-treesitter-textobjects/`, and `…/parsers/parser/<lang>.so`. On macOS, treesitter markdown **highlighting and fenced-code-block injection are now ENABLED** by the conditional block described above (`vim.treesitter.start` on markdown buffers, with core-native capture-based markdown injections from nvim-treesitter's `main` branch on `runtimepath`); the parsers' ABI compatibility with the pinned Neovim is proven by `//crates/wolf-ffi:treesitter_parser_abi`, and the live highlighter + go-injection are proven by `//crates/wolf-ffi:treesitter_highlight`. Still **deferred** to later changes: the **Linux/Nix vendoring path** for these three artifacts (mirroring how orlando/lighthouse were vendored Bazel-only first), wiring **nvim-treesitter-textobjects keymaps** (the plugin stays bundled-but-inert), and a Markdown **LSP / formatter**.
