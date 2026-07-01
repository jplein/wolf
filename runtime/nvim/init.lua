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
-- Treesitter grammars (markdown / markdown_inline), a Markdown LSP (e.g.
-- marksman) and a formatter are DEFERRED to a later phase (plugin baking via
-- wrapNeovim). Phase 1 relies on Neovim's built-in markdown syntax highlighting,
-- which is enough for the redraw/highlight pipeline and its tests.

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
-- hl_ids. Treesitter is deferred; this is the Phase 1 highlight source.
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
