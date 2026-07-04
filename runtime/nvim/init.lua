-- Wolf hermetic Neovim configuration (Markdown-only).
--
-- This file is version-controlled in the Wolf repo and loaded by the captive
-- Neovim that wolf-core spawns, via:
--
--     nvim --embed --clean -u runtime/nvim/init.lua -i NONE -n
--
-- `--clean` already skips the user's config/plugins/shada; we further pin the
-- XDG_* dirs and NVIM_APPNAME away from the user's from the spawning side. The
-- settings below make the captive editor deterministic for headless rendering
-- tests and tuned for editing Markdown.
--
-- Treesitter markdown highlighting + fenced-code-block injections are
-- conditionally ENABLED when the vendored `nvim-treesitter/` (main queries) and
-- `parsers/` siblings are present next to this init.lua (the macOS .app bundle;
-- see the conditional block below). On Linux/cargo — and any test that does not
-- stage those siblings — the block is a strict NO-OP and Neovim's built-in
-- `syntax on` remains the highlight source. A Markdown LSP (e.g. marksman) and a
-- formatter are still DEFERRED to a later phase; the vendored
-- nvim-treesitter-textobjects plugin is bundled-but-INERT (no keymaps wired).

local opt = vim.opt

-- Hermetic / no on-disk side effects: never touch the user's state.
opt.swapfile = false
opt.shadafile = "NONE"
opt.undofile = false
opt.backup = false
opt.writebackup = false

-- Suppress the intro message so it does not pollute the initial grid.
opt.shortmess:append("I")

-- True-color so hl_attr_define / default_colors_set carry RGB values that the
-- front-ends paint directly.
opt.termguicolors = true

-- Deterministic, chrome-free grid: the front-end draws its own chrome.
opt.number = false
opt.signcolumn = "no"
opt.ruler = false
opt.showcmd = false
opt.laststatus = 0

-- Forward mouse to Neovim; the front-end translates OS mouse events to input.
opt.mouse = "a"

-- Soft-wrap long lines at WORD BOUNDARIES: Wolf is a Markdown editor, where prose
-- lines routinely exceed the editor width. `wrap` is also nvim's own default;
-- `linebreak` makes the wrap fall on spaces/punctuation (`breakat`) instead of
-- mid-word. Determinism-sensitive rendering tests keep their asserted content
-- within the grid width, so this does not affect their column assertions.
opt.wrap = true
opt.linebreak = true

-- With soft-wrap on, `smoothscroll` scrolls by SCREEN line rather than jumping
-- past an entire wrapped logical line, so mouse-wheel / `<C-e>`/`<C-d>` scrolling
-- of long wrapped Markdown lines stays smooth and never skips wrapped fragments.
opt.smoothscroll = true

-- Treat opened files as markdown by default (Wolf is a Markdown editor).
vim.filetype.add({ extension = { md = "markdown" } })

-- Enable built-in syntax highlighting (markdown) so the grid carries varied
-- hl_ids. This is the highlight source on Linux/cargo and the fallback when
-- treesitter is unavailable; the conditional treesitter block below layers over it
-- (via its own namespace) on the macOS .app.
vim.cmd("syntax on")

-- Vendored orlando plugin (Bazel macOS bundle only).
--
-- The macOS .app places a sibling `orlando/` tree next to this init.lua at
-- Contents/Resources/orlando/ (bundled by //bazel/orlando). On Linux/cargo it is
-- ABSENT, so this whole block is a strict NO-OP: no runtimepath change, no error.
-- We self-locate init.lua's own directory (rather than assume a fixed path), look
-- for the sibling `orlando/`, and only if it exists prepend it to runtimepath and
-- deterministically source its plugin file — necessary because `--clean` does not
-- scan user plugin dirs, so a plain rtp prepend alone would not auto-source it.
-- orlando itself guards on vim.g.loaded_orlando, so the explicit source is safe.
-- orlando's wrap_indent also sets wrap/linebreak/breakindent; additive & harmless
-- with the wrap/linebreak/smoothscroll set above.
local this_file = debug.getinfo(1, "S").source:sub(2) -- strip the leading '@'
local this_dir = vim.fn.fnamemodify(this_file, ":p:h")
local orlando_dir = this_dir .. "/orlando"
local uv = vim.uv or vim.loop -- vim.uv on nvim 0.12.3; vim.loop fallback
if uv and uv.fs_stat(orlando_dir) then
	vim.opt.runtimepath:prepend(orlando_dir)
	vim.cmd.runtime("plugin/orlando.lua")
end

-- Vendored lighthouse colorscheme (Bazel macOS bundle only).
--
-- Like orlando, the macOS .app places a sibling `lighthouse/` tree next to this
-- init.lua at Contents/Resources/lighthouse/ (bundled by //bazel/lighthouse). On
-- Linux/cargo it is ABSENT, so this whole block is a strict NO-OP: no runtimepath
-- change, no colorscheme call, no error. We reuse the already-computed `this_dir`
-- / `uv` locals from the orlando block above.
--
-- Unlike a plugin, a colorscheme on runtimepath does NOTHING on its own: we must
-- ALSO activate it. When the sibling dir exists we prepend it to runtimepath, set
-- the dark background, then run `:colorscheme lighthouse` — `:colorscheme` finds
-- `colors/lighthouse.lua` on rtp itself, so we do NOT need to `runtime()` it.
-- ORDERING: this MUST come AFTER `vim.cmd("syntax on")` above, because both
-- `syntax on` and `:colorscheme` clear/redefine highlight groups — activating
-- last wins. `vim.o.background` is set BEFORE `:colorscheme` so a dual-variant
-- scheme reads it. (A future change will switch dark/light from the OS appearance
-- preference; the activation is kept isolated/simple to make that easy.)
local lighthouse_dir = this_dir .. "/lighthouse"
if uv and uv.fs_stat(lighthouse_dir) then
	vim.opt.runtimepath:prepend(lighthouse_dir)
	vim.o.background = "dark"
	vim.cmd.colorscheme("lighthouse")
end

-- Vendored treesitter highlighting + fenced-code injections (Bazel macOS bundle
-- only).
--
-- Like orlando/lighthouse, the macOS .app places sibling trees next to this
-- init.lua at Contents/Resources/: `nvim-treesitter/` (the main-branch queries
-- provider: queries/<lang>/highlights.scm + queries/markdown/injections.scm),
-- `parsers/` (host-compiled parser/<lang>.so for the embedded fenced languages),
-- and `nvim-treesitter-textobjects/` (bundled but INERT — no source, no setup, no
-- keymaps). On Linux/cargo they are ABSENT, so this whole block is a strict NO-OP:
-- no runtimepath change, no autocmd, no error — `syntax on` above stays the
-- highlight source. We reuse the already-computed `this_dir` / `uv` locals.
--
-- GATE on the queries provider (`nvim-treesitter/`). When present we prepend it to
-- runtimepath (so `vim.treesitter.start` finds highlights + injections queries),
-- then prepend `parsers/` (so nvim resolves <rtp>/parser/<lang>.so for embedded
-- languages) and the inert textobjects tree, each only if it independently exists.
-- markdown / markdown_inline parsers already ship with nvim, so they are NOT
-- re-vendored. We do NOT call nvim-treesitter's heavy `.setup{}` (its :TSInstall
-- machinery is non-deterministic under --clean); the vendored plugin is used purely
-- as an on-rtp query provider. A FileType=markdown autocmd runs
-- `vim.treesitter.start` on each markdown buffer (wrapped in pcall so a query-loader
-- incompatibility degrades to syntax-on rather than raising), which lights markdown
-- highlighting AND routes fenced blocks through the injections query on rtp.
local nvim_treesitter_dir = this_dir .. "/nvim-treesitter"
if uv and uv.fs_stat(nvim_treesitter_dir) then
	vim.opt.runtimepath:prepend(nvim_treesitter_dir)
	local parsers_dir = this_dir .. "/parsers"
	if uv.fs_stat(parsers_dir) then
		vim.opt.runtimepath:prepend(parsers_dir)
	end
	local textobjects_dir = this_dir .. "/nvim-treesitter-textobjects"
	if uv.fs_stat(textobjects_dir) then
		vim.opt.runtimepath:prepend(textobjects_dir)
	end

	-- Start treesitter on a buffer, tolerating a query-loader incompatibility: a
	-- pcall failure leaves `syntax on` as the highlight source instead of raising.
	local function wolf_treesitter_start(buf)
		pcall(vim.treesitter.start, buf)
	end

	-- FileType=markdown autocmd: lights every markdown buffer as it is opened
	-- (`:e doc.md`), routing fenced blocks via the injections query on rtp.
	local group = vim.api.nvim_create_augroup("WolfTreesitter", { clear = true })
	vim.api.nvim_create_autocmd("FileType", {
		group = group,
		pattern = "markdown",
		callback = function(args)
			wolf_treesitter_start(args.buf)
		end,
	})

	-- Emit-once for any markdown buffer already loaded before the autocmd existed.
	for _, buf in ipairs(vim.api.nvim_list_bufs()) do
		if vim.api.nvim_buf_is_loaded(buf) and vim.bo[buf].filetype == "markdown" then
			wolf_treesitter_start(buf)
		end
	end
end

-- Current-file tracking: wolf-core invokes this over RPC after ui_attach,
-- passing its own RPC channel id. It registers an autocmd that notifies core of
-- the active buffer's absolute path on every buffer switch / rename, and emits
-- once immediately so the startup buffer is reflected. An unnamed buffer yields
-- an empty string from expand('%:p'), which core maps to "no current file".
function _G.wolf_setup_current_file_tracking(chan)
	local group = vim.api.nvim_create_augroup("WolfCurrentFile", { clear = true })
	vim.api.nvim_create_autocmd({ "BufEnter", "BufWinEnter", "BufFilePost" }, {
		group = group,
		callback = function()
			vim.rpcnotify(chan, "wolf_current_file", vim.fn.expand("%:p"))
		end,
	})
	-- Initial emit so the startup buffer is reflected without a buffer switch.
	vim.rpcnotify(chan, "wolf_current_file", vim.fn.expand("%:p"))
end

-- Directory tracking: wolf-core invokes this over RPC after ui_attach (and after
-- the current-file / layout setups), passing its own RPC channel id. It registers
-- a DirChanged autocmd (pattern '*', covering :cd/:lcd/:tcd) that notifies core of
-- the new working directory via wolf_cwd, and emits once immediately so the startup
-- cwd is reflected without a directory change. Core tracks this in a sibling cell
-- (not the render snapshot) and uses it to pick the per-cwd MRU store.
function _G.wolf_setup_dir_tracking(chan)
	local group = vim.api.nvim_create_augroup("WolfDir", { clear = true })
	vim.api.nvim_create_autocmd("DirChanged", {
		group = group,
		pattern = "*",
		callback = function()
			vim.rpcnotify(chan, "wolf_cwd", vim.fn.getcwd())
		end,
	})
	-- Initial emit so the startup cwd is reflected without a :cd.
	vim.rpcnotify(chan, "wolf_cwd", vim.fn.getcwd())
end

-- Theme tracking: wolf-core invokes this over RPC after ui_attach (and after
-- the current-file / layout / dir setups), passing its own RPC channel id. It
-- registers a ColorScheme autocmd (pattern '*', covering :colorscheme and
-- :set background=..., which re-sources the active scheme) that notifies core
-- of the effective WolfCursor / WolfTitlebar highlight-group colors via
-- wolf_theme, and emits once immediately so the startup colorscheme is
-- reflected without a theme change. The payload is FOUR flat scalars
-- (cursor_fg, cursor_bg, titlebar_fg, titlebar_bg), each a packed 0xRRGGBB
-- color or -1 for an unset channel — flat scalars sidestep the empty-Lua-table
-- map-vs-array msgpack ambiguity a table payload would have for a cleared
-- group. Groups are read with link = false so a linked group resolves to its
-- target's effective attributes; the pcall is defensive (a never-defined name
-- yields {} anyway, but the pcall costs nothing).
function _G.wolf_setup_theme_tracking(chan)
	local function group(name)
		local ok, h = pcall(vim.api.nvim_get_hl, 0, { name = name, link = false })
		if not ok or type(h) ~= "table" then
			return {}
		end
		return h
	end

	local function emit()
		local cursor = group("WolfCursor")
		local titlebar = group("WolfTitlebar")
		vim.rpcnotify(
			chan,
			"wolf_theme",
			cursor.fg or -1,
			cursor.bg or -1,
			titlebar.fg or -1,
			titlebar.bg or -1
		)
	end

	local group_id = vim.api.nvim_create_augroup("WolfTheme", { clear = true })
	vim.api.nvim_create_autocmd("ColorScheme", {
		group = group_id,
		pattern = "*",
		callback = emit,
	})
	-- Initial emit so the startup colorscheme is reflected without a change.
	emit()
end

-- Split-panes layout overrides: wolf-core invokes this over RPC after ui_attach
-- (and after current-file tracking), passing its own RPC channel id. The SHELL
-- owns the pane layout, so Neovim's split/close/focus chords and the
-- :split/:vsplit/:close Ex-commands are rebound to report layout INTENTS over
-- `rpcnotify` rather than (or in addition to) performing the native window op.
--
-- Asymmetry (decision D4):
--   * SPLIT: nvim PERFORMS the real :split/:vsplit (so it creates the window +
--     grid and emits win_pos) AND reports a wolf_layout{action='split'} intent so
--     the shell learns the orientation, the origin window, and the new window.
--   * CLOSE: nvim PERFORMS a guarded nvim_win_close (refusing to close the last
--     window); the shell observes the removal via the win_close redraw event, so
--     NO intent is emitted for a close.
--   * FOCUS move: a PURE intent — nvim does NOT change focus; it only reports
--     wolf_layout{action='focus_dir'} and the shell drives the focus change.
--
-- A WinEnter autocmd reports the active window via wolf_active_win (emitting once
-- immediately) so core's snapshot tracks the focused pane. BufEnter (current-file
-- tracking, above) already follows focus, so current_file tracks the active pane.
function _G.wolf_setup_layout(chan)
	-- SPLIT: capture the window that is current BEFORE the split, perform the real
	-- split (the new window becomes current), then report both handles. Orientation
	-- 'h' = :split (stacked / Horizontal divider), 'v' = :vsplit (side-by-side /
	-- Vertical divider).
	local function do_split(orient)
		local from = vim.api.nvim_get_current_win()
		vim.cmd(orient == "h" and "split" or "vsplit")
		local win = vim.api.nvim_get_current_win()
		vim.rpcnotify(chan, "wolf_layout", {
			action = "split",
			orientation = orient,
			from = from,
			win = win,
		})
	end

	-- CLOSE: guarded against closing the last window in the tab page. No intent.
	local function do_close()
		if #vim.api.nvim_tabpage_list_wins(0) > 1 then
			vim.api.nvim_win_close(0, false)
		end
	end

	-- FOCUS: PURE intent. Do NOT change nvim focus here; just report the request.
	local function do_focus(dir)
		vim.rpcnotify(chan, "wolf_layout", {
			action = "focus_dir",
			dir = dir,
			from = vim.api.nvim_get_current_win(),
		})
	end

	-- Expose the helpers as globals so the cnoreabbrev Ex-command overrides below
	-- can invoke them from the command line via `lua ...`.
	_G.wolf_do_split = do_split
	_G.wolf_do_close = do_close
	_G.wolf_do_focus = do_focus

	-- Normal-mode window chords. `noremap` (the default) fully replaces the native
	-- behavior, so a focus chord is genuinely a no-op-on-nvim pure intent.
	local map = vim.keymap.set
	local silent = { silent = true }
	map("n", "<C-w>s", function() do_split("h") end, silent)
	map("n", "<C-w><C-s>", function() do_split("h") end, silent)
	map("n", "<C-w>v", function() do_split("v") end, silent)
	map("n", "<C-w><C-v>", function() do_split("v") end, silent)
	map("n", "<C-w>c", function() do_close() end, silent)
	map("n", "<C-w>q", function() do_close() end, silent)
	map("n", "<C-w>h", function() do_focus("left") end, silent)
	map("n", "<C-w><Left>", function() do_focus("left") end, silent)
	map("n", "<C-w>j", function() do_focus("down") end, silent)
	map("n", "<C-w><Down>", function() do_focus("down") end, silent)
	map("n", "<C-w>k", function() do_focus("up") end, silent)
	map("n", "<C-w><Up>", function() do_focus("up") end, silent)
	map("n", "<C-w>l", function() do_focus("right") end, silent)
	map("n", "<C-w><Right>", function() do_focus("right") end, silent)

	-- Ex-command overrides via cnoreabbrev. A command-line abbreviation expands
	-- only a WHOLE word, guarded by getcmdtype()==':' so it never fires inside a
	-- search or a larger command. This is the fragile path (whole-word only); the
	-- chords above are the robust primary. :new/:vnew are treated as plain splits.
	local function abbrev(word, replacement)
		vim.cmd(string.format(
			"cnoreabbrev <expr> %s (getcmdtype()==':' && getcmdline()==%q) ? %q : %q",
			word,
			word,
			replacement,
			word
		))
	end
	abbrev("split", "lua wolf_do_split('h')")
	abbrev("sp", "lua wolf_do_split('h')")
	abbrev("vsplit", "lua wolf_do_split('v')")
	abbrev("vs", "lua wolf_do_split('v')")
	abbrev("close", "lua wolf_do_close()")
	abbrev("new", "lua wolf_do_split('h')")
	abbrev("vnew", "lua wolf_do_split('v')")

	-- Active-window tracking: report the focused window on every WinEnter, and emit
	-- once immediately so the startup window is reflected.
	local group = vim.api.nvim_create_augroup("WolfLayout", { clear = true })
	vim.api.nvim_create_autocmd("WinEnter", {
		group = group,
		callback = function()
			vim.rpcnotify(chan, "wolf_active_win", vim.api.nvim_get_current_win())
		end,
	})
	vim.rpcnotify(chan, "wolf_active_win", vim.api.nvim_get_current_win())
end

-- Commands: wolf-core invokes this over RPC after ui_attach (last, after the
-- current-file / layout / dir / theme setups), passing its own RPC channel id and
-- the command list. A Command is a no-parameter, no-return user action. Unlike
-- layout intents, commands are ONLY user-triggered: there is no autocmd and no
-- emit-on-attach.
--
-- `commands` is a list of {id, ex_command} pairs (the canonical registry Rust
-- passes in from COMMAND_SPECS). For each pair this:
--   * seeds package.loaded['wolf'][id] = function() rpcnotify(chan,'wolf_command',id) end
--     so `require('wolf').<id>()` resolves under --clean (require consults
--     package.loaded first; a plain global would NOT satisfy `require`); and
--   * creates a :<ex_command> user command (nargs=0, force=true) whose callback
--     rpcnotifies the same wolf_command channel with the id.
-- Core folds the id back into a Command via Command::from_id and drains it.
function _G.wolf_setup_commands(chan, commands)
	-- Seed the `wolf` module so `require('wolf')` resolves under --clean.
	local wolf_tbl = package.loaded["wolf"] or {}
	package.loaded["wolf"] = wolf_tbl

	for _, pair in ipairs(commands) do
		local id = pair[1]
		local ex_command = pair[2]
		local function fire()
			vim.rpcnotify(chan, "wolf_command", id)
		end
		-- Lua face: require('wolf').<id>()
		wolf_tbl[id] = fire
		-- Ex-command face: :<ex_command>
		vim.api.nvim_create_user_command(ex_command, fire, { nargs = 0, force = true })
	end
end
