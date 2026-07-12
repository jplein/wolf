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
-- the active buffer's absolute path AND its filetype on every buffer switch /
-- rename / FileType detection, and emits once immediately so the startup buffer
-- is reflected. An unnamed buffer yields an empty string from expand('%:p'),
-- which core maps to "no current file". The filetype (2nd arg, vim.bo.filetype)
-- drives core's Snapshot.is_markdown_active (the Preview command's enable gate);
-- FileType is in the event list so the filetype tracks the active buffer even
-- when detection happens after the buffer switch.
--
-- Preview scratch buffers (carrying b:wolf_preview_source) are SKIPPED: focusing
-- one emits an EMPTY path and EMPTY filetype, so core never sees the "Preview:
-- <name>" scratch name as a real current file and never reports markdown-active
-- for the (non-markdown) preview pane.
function _G.wolf_setup_current_file_tracking(chan)
	local group = vim.api.nvim_create_augroup("WolfCurrentFile", { clear = true })
	local function emit()
		local buf = vim.api.nvim_get_current_buf()
		if vim.b[buf].wolf_preview_source ~= nil then
			vim.rpcnotify(chan, "wolf_current_file", "", "")
		else
			vim.rpcnotify(chan, "wolf_current_file", vim.fn.expand("%:p"), vim.bo.filetype)
		end
	end
	-- Expose the emit closure so wolf_open_preview can re-assert the active buffer's
	-- filetype after a reuse bind: a BACKGROUND nvim_win_set_buf into the other
	-- (preview) window fires BufWinEnter with the preview scratch as the current
	-- buffer, which would otherwise leave is_markdown_active stuck false.
	_G.wolf_emit_current_file = emit
	vim.api.nvim_create_autocmd({ "BufEnter", "BufWinEnter", "BufFilePost", "FileType" }, {
		group = group,
		callback = emit,
	})
	-- Initial emit so the startup buffer is reflected without a buffer switch.
	emit()
end

-- Per-window buffer tracking: wolf-core invokes this over RPC after ui_attach (and
-- right after the current-file setup), passing its own RPC channel id. It registers
-- autocmds that notify core, via wolf_buffers, of the buffer path shown in EVERY
-- window of the current tabpage -- a list of { win, nvim_buf_get_name(buf) } pairs
-- -- so the front-end can render a per-pane statusbar keyed by window handle. It
-- emits once immediately so the startup layout is reflected without a change.
--
-- The whole current window set is re-emitted on each autocmd; core WHOLESALE-
-- REPLACES its per-window map, so stale/closed windows drop out. WinClosed fires
-- while the closing window is still listed by nvim_list_wins(), but re-emitting the
-- full current set is still correct: the survivor set replaces the previous one.
-- An unnamed buffer yields an empty string from nvim_buf_get_name, which core skips
-- (absent from the map -> the pane reports "[No Name]").
function _G.wolf_setup_buffer_tracking(chan)
	local group = vim.api.nvim_create_augroup("WolfBuffers", { clear = true })
	local function emit()
		local list = {}
		for _, win in ipairs(vim.api.nvim_list_wins()) do
			local buf = vim.api.nvim_win_get_buf(win)
			-- Skip preview scratch panes (b:wolf_preview_source): the front-end
			-- overlays a web view and paints its own "Preview: <name>" statusbar, so
			-- core must not track the bogus scratch name as a real per-window path.
			if vim.b[buf].wolf_preview_source == nil then
				list[#list + 1] = { win, vim.api.nvim_buf_get_name(buf) }
			end
		end
		vim.rpcnotify(chan, "wolf_buffers", list)
	end
	vim.api.nvim_create_autocmd(
		{ "BufEnter", "BufWinEnter", "BufFilePost", "WinEnter", "WinNew", "WinClosed" },
		{
			group = group,
			callback = emit,
		}
	)
	-- Initial emit so the startup window layout is reflected without a change.
	emit()
end

-- Directory tracking: wolf-core invokes this over RPC after ui_attach (and after
-- the current-file / layout setups), passing its own RPC channel id. It registers
-- a DirChanged autocmd (pattern '*', covering :cd/:lcd/:tcd) that notifies core of
-- the new working directory via wolf_cwd, and emits once immediately so the startup
-- cwd is reflected without a directory change. Core tracks this in a sibling cell
-- (not the render snapshot) and uses it to pick the per-cwd MRU store.
--
-- The one-shot INITIAL emit carries a distinguishing `true` second arg; the
-- DirChanged callback carries no second arg (treated as false). Core uses this
-- flag to TRACK the launch/initial cwd (in its sibling cell) WITHOUT persisting
-- it to state.json or recording it in the recent-dirs MRU — the passive launch
-- cwd is not a directory the user opened, so only genuine DirChanged events
-- (every real :cd/:lcd/:tcd, including the Open-Folder chdir) are persisted and
-- recorded.
function _G.wolf_setup_dir_tracking(chan)
	local group = vim.api.nvim_create_augroup("WolfDir", { clear = true })
	vim.api.nvim_create_autocmd("DirChanged", {
		group = group,
		pattern = "*",
		callback = function()
			vim.rpcnotify(chan, "wolf_cwd", vim.fn.getcwd())
		end,
	})
	-- Initial emit so the startup cwd is reflected without a :cd. Flagged `true`
	-- so core tracks-but-does-not-persist/record it.
	vim.rpcnotify(chan, "wolf_cwd", vim.fn.getcwd(), true)
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

-- Tabpage helpers: module-level globals wolf-core invokes over `nvim_exec_lua`
-- to realize the project/tabpage-per-view model (one nvim TABPAGE per front-end
-- view). Defined at module level so they exist regardless of setup ordering.

-- Create a new tabpage, optionally rooting it at `cwd` via `:tcd` (the tab-local
-- cwd -- each view owns its own working directory), and return its STABLE handle
-- (nvim_get_current_tabpage()). An empty/nil cwd leaves the new tab at the global
-- cwd (the empty-state view). The path is fnameescape'd for the command line.
function _G.wolf_create_tabpage(cwd)
	vim.cmd("tabnew")
	if cwd ~= nil and cwd ~= "" then
		vim.cmd("tcd " .. vim.fn.fnameescape(cwd))
	end
	return vim.api.nvim_get_current_tabpage()
end

-- Close the tabpage named by its STABLE handle. Tabpage NUMBERS renumber as tabs
-- open/close, so we resolve the handle to its CURRENT number and run `<n>tabclose`
-- inside a pcall: this swallows a last-tabpage E784 (the front-end never closes
-- the last view -- the Project shuts the whole nvim down instead). A stale/invalid
-- handle also degrades to a no-op via the guarding pcall.
function _G.wolf_close_tabpage(handle)
	local ok, n = pcall(vim.api.nvim_tabpage_get_number, handle)
	if not ok or n == nil then
		return
	end
	pcall(vim.cmd, n .. "tabclose")
end

-- Return the tab-local working directory of the tabpage named by its STABLE
-- handle (authoritative for a NON-current view's cwd). Maps the stable handle to
-- its current NUMBER (which getcwd(-1, n) needs) and returns getcwd(-1, n), which
-- falls back to the global cwd for a tab that was never `:tcd`'d. A stale/invalid
-- handle returns "" (core maps that to "no cwd").
function _G.wolf_tab_cwd(handle)
	local ok, n = pcall(vim.api.nvim_tabpage_get_number, handle)
	if not ok or n == nil then
		return ""
	end
	local ok2, dir = pcall(vim.fn.getcwd, -1, n)
	if not ok2 or type(dir) ~= "string" then
		return ""
	end
	return dir
end

-- Tabpage tracking + navigation/creation lockout: wolf-core invokes this over RPC
-- after the command setup, passing its own RPC channel id. It notifies core of the
-- current tabpage handle via wolf_tabpage on every TabEnter/TabNewEntered (emitting
-- once immediately), so core's snapshot tracks the current tabpage (the cheap
-- refocus gate the front-end uses).
--
-- LOCKOUT (OD4): Wolf owns ALL tabpage lifecycle (create/close/switch), so the
-- nvim-internal tabpage navigation/creation chords are mapped to <Nop> and the
-- typed :tab* Ex-commands are best-effort intercepted. A user-typed :tabnew would
-- create an ORPHAN tabpage no view is bound to; :tabnext/:tabonly/:tabclose would
-- switch/close a tabpage out from under a view and desync the front-end-authoritative
-- current view. The per-view cwd command :tcd is DELIBERATELY not locked out (it is
-- a different word from the :tab* commands, so the whole-word abbrevs never catch
-- :tcd/:tcd!/:lcd/:cd). Residual gaps (obscure typed variants / abbreviations) are
-- an accepted v1 limitation (documented).
function _G.wolf_setup_tabs(chan)
	local group = vim.api.nvim_create_augroup("WolfTabs", { clear = true })
	vim.api.nvim_create_autocmd({ "TabEnter", "TabNewEntered" }, {
		group = group,
		callback = function()
			vim.rpcnotify(chan, "wolf_tabpage", vim.api.nvim_get_current_tabpage())
		end,
	})
	-- Initial emit so the startup tabpage is reflected without a switch.
	vim.rpcnotify(chan, "wolf_tabpage", vim.api.nvim_get_current_tabpage())

	-- Lock out the nvim-internal tabpage navigation/creation chords. `noremap`
	-- (the default) fully replaces the native behavior.
	local map = vim.keymap.set
	local silent = { silent = true }
	for _, lhs in ipairs({ "gt", "gT", "<C-w>gt", "<C-w>gT", "<C-PageUp>", "<C-PageDown>" }) do
		map("n", lhs, "<Nop>", silent)
	end

	-- Best-effort intercept of the typed tabpage Ex-commands via cnoreabbrev: a
	-- command-line abbreviation expands only a WHOLE word, guarded by
	-- getcmdtype()==':' so it never fires inside a search or a larger command, and
	-- it expands to an EMPTY command line (a no-op on <CR>). Whole-word matching is
	-- why :tcd/:tcd!/:lcd/:cd are NOT caught -- they are different words.
	local function lock(word)
		vim.cmd(string.format(
			"cnoreabbrev <expr> %s (getcmdtype()==':' && getcmdline()==%q) ? %q : %q",
			word,
			word,
			"",
			word
		))
	end
	for _, w in ipairs({ "tabnew", "tabnext", "tabprevious", "tabonly", "tabclose", "tabmove" }) do
		lock(w)
	end
end

-- Per-pane view tracking for the buffer picker: wolf-core invokes this over RPC
-- after the command setup. Unlike the other setups it takes NO channel id and
-- emits NO rpcnotify — it only maintains an in-nvim (winid, bufnr) -> winsaveview
-- cache and defines the wolf_switch_buffer helper that reads it, so no
-- view/scroll payload ever crosses the FFI boundary. The cache is an upvalue so
-- wolf_switch_buffer closes over it.
function _G.wolf_setup_view_tracking()
	local view_cache = {}

	local group = vim.api.nvim_create_augroup("WolfView", { clear = true })

	-- Capture the view of the pane/buffer being LEFT, keyed by (win, buf).
	vim.api.nvim_create_autocmd({ "BufLeave", "WinLeave" }, {
		group = group,
		callback = function()
			local win = vim.api.nvim_get_current_win()
			local buf = vim.api.nvim_win_get_buf(win)
			view_cache[win] = view_cache[win] or {}
			view_cache[win][buf] = vim.fn.winsaveview()
		end,
	})

	-- Prune a window's cache when it closes (ev.match is the closing window id).
	vim.api.nvim_create_autocmd("WinClosed", {
		group = group,
		callback = function(ev)
			view_cache[tonumber(ev.match)] = nil
		end,
	})

	-- Switch window `win` to buffer `buf` (window-scoped, NOT :edit), then restore
	-- that pane's saved view for that buffer; if the pane never showed the buffer,
	-- force the top of the buffer. Keyed on (win, buf) so each pane remembers its
	-- OWN scroll of a shared buffer, and never borrows another pane's scroll.
	-- Wrapped in pcall so a stale bufnr degrades to a no-op.
	function _G.wolf_switch_buffer(win, buf)
		pcall(function()
			vim.api.nvim_win_set_buf(win, buf)
			if view_cache[win] and view_cache[win][buf] then
				local view = view_cache[win][buf]
				vim.api.nvim_win_call(win, function()
					vim.fn.winrestview(view)
				end)
			else
				vim.api.nvim_win_call(win, function()
					vim.fn.winrestview({ lnum = 1, col = 0, topline = 1, leftcol = 0 })
				end)
			end
		end)
	end
end

-- Per-source preview scratch-buffer map (source bufnr -> preview bufnr), a global
-- so wolf_open_preview reuses one preview buffer per source across invocations.
_G.wolf_preview_bufs = _G.wolf_preview_bufs or {}

-- Opens (or reuses) the Markdown preview pane for the ACTIVE source buffer and
-- returns the preview scratch buffer's number. wolf-core calls this over
-- nvim_exec_lua from WolfCore::open_preview, passing `mode`:
--   * "split_right": single-pane placement -- reuse wolf_do_split('v') (which
--     emits a wolf_layout Vertical Split intent so the shell creates+binds the new
--     pane) then bind the preview buffer into the new (current) window.
--   * "reuse_preview_pane": two-pane placement whose OTHER pane already shows a
--     preview -- bind the preview buffer into that OTHER window (located by its
--     b:wolf_preview_source marker) WITHOUT changing the current window, falling
--     back to the current window when no other preview window is found.
--   * "active": multi-pane placement -- just bind the preview buffer into the
--     current window (nvim_win_set_buf), replacing its content.
-- No-op (returns 0) unless the active buffer's filetype is markdown (the same gate
-- as the Preview command). The preview pane is a real nvim buffer (OD1 = Design S):
-- a listed + nofile + nomodifiable scratch buffer named "Preview: <cwd-relative path>"
-- carrying b:wolf_preview_source (its source's abs path) and b:wolf_preview_source_bufnr
-- (its source's bufnr). Being buflisted, it surfaces in the buffer picker as
-- "Preview: <cwd-relative path>" with no front-end injection; the path-based trackers skip
-- it via b:wolf_preview_source.
-- Creates (or reuses, via the _G.wolf_preview_bufs map) the preview scratch
-- buffer for `source_buf` (whose abs path is `source_path`) and returns its
-- bufnr. Factored out of wolf_open_preview so wolf_preview_file can create the
-- same kind of preview buffer for a file the user navigated to via a link. The
-- preview buffer is a listed + scratch (nofile) + nomodifiable buffer named
-- "Preview: <cwd-relative path>" carrying b:wolf_preview_source / b:wolf_preview_source_bufnr.
function _G.wolf_ensure_preview_buf(source_buf, source_path)
	local preview_buf = _G.wolf_preview_bufs[source_buf]
	if not preview_buf or not vim.api.nvim_buf_is_valid(preview_buf) then
		-- listed = true (surfaces in the picker), scratch = true (nofile /
		-- bufhidden=hide / noswapfile defaults).
		preview_buf = vim.api.nvim_create_buf(true, true)
		local rel = vim.fn.fnamemodify(source_path, ":.")
		-- Name the buffer "Preview: <cwd-relative path>" so it surfaces in the picker
		-- with that label (OD1). nvim_buf_set_name raises E95 when that name is
		-- already taken by ANOTHER buffer (two sources sharing a cwd-relative path,
		-- e.g. two README.md at the same relative location); the reuse map above
		-- already short-circuits a re-preview of the SAME source, so a collision here
		-- is only a DIFFERENT source. On collision fall back to a bufnr-disambiguated
		-- name so the second preview still opens (rather than open_preview returning
		-- 0). The common, non-colliding case keeps the exact "Preview: <cwd-relative
		-- path>" name the picker row asserts on.
		local wanted = "Preview: " .. rel
		if not pcall(vim.api.nvim_buf_set_name, preview_buf, wanted) then
			pcall(vim.api.nvim_buf_set_name, preview_buf, wanted .. " (" .. source_buf .. ")")
		end
		vim.b[preview_buf].wolf_preview_source = source_path
		vim.b[preview_buf].wolf_preview_source_bufnr = source_buf
		vim.bo[preview_buf].modifiable = false
		_G.wolf_preview_bufs[source_buf] = preview_buf
	end
	return preview_buf
end

function _G.wolf_open_preview(mode)
	if vim.bo.filetype ~= "markdown" then
		return 0
	end
	local source_buf = vim.api.nvim_get_current_buf()
	local source_path = vim.api.nvim_buf_get_name(source_buf)

	local preview_buf = _G.wolf_ensure_preview_buf(source_buf, source_path)

	if mode == "split_right" then
		-- Single-pane placement: split a new pane to the right, then bind the
		-- preview into it (the new split is the current window).
		wolf_do_split("v")
		vim.api.nvim_win_set_buf(0, preview_buf)
	elseif mode == "reuse_preview_pane" then
		-- Two-pane placement whose OTHER pane already previews: bind into that
		-- OTHER preview window (found by its b:wolf_preview_source marker) WITHOUT
		-- changing the current window. Falls back to the current window when no
		-- other preview window is found (defensive; the seam already gates this).
		local cur = vim.api.nvim_get_current_win()
		local target = nil
		for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
			if win ~= cur then
				local src = vim.b[vim.api.nvim_win_get_buf(win)].wolf_preview_source
				if src ~= nil and src ~= "" then
					target = win
					break
				end
			end
		end
		vim.api.nvim_win_set_buf(target or cur, preview_buf)
	else
		-- "active" (and any unknown mode): replace the current window's buffer.
		vim.api.nvim_win_set_buf(0, preview_buf)
	end

	-- A background-window nvim_win_set_buf (the reuse path) does not fire
	-- BufWinEnter, so the front-end would never learn the new bind; nudge it to
	-- re-emit the previews snapshot. Done for ALL modes (the emit is idempotent --
	-- the handler wholesale-replaces its previews snapshot).
	if _G.wolf_emit_previews then
		_G.wolf_emit_previews()
	end
	-- Re-assert the active buffer's filetype: a reuse bind's BACKGROUND
	-- nvim_win_set_buf fires BufWinEnter with the preview scratch as the current
	-- buffer, which would leave is_markdown_active stuck false for the (still
	-- markdown) active source pane. Unconditional for all modes -- symmetric and
	-- idempotent (the current-file emit wholesale-replaces the tracked filetype).
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end
	return preview_buf
end

-- Repoints the preview window `win` at the Markdown file `path` (an ABSOLUTE
-- path, resolved by the front-end from a clicked file:// link), replacing that
-- pane's current preview with a preview of the linked file. wolf-core calls this
-- over nvim_exec_lua from WolfCore::preview_file.
--
-- Returns the linked file's SOURCE bufnr (> 0) on success; 0 when `path` is
-- empty, unreadable, or not Markdown. On 0 the front-end leaves the pane as-is
-- (and applies its own O3 fallback for a real-but-non-previewable file).
--
-- Contract: this does NO relative-path resolution -- `path` is already absolute.
function _G.wolf_preview_file(win, path)
	if path == nil or path == "" then
		return 0
	end
	if vim.fn.filereadable(path) == 0 then
		return 0
	end

	local source_buf = vim.fn.bufadd(path)
	vim.fn.bufload(source_buf)

	-- Markdown gate: honor an already-set filetype, else ask vim.filetype.match
	-- (by buffer + filename) and set it; a non-markdown file yields 0.
	if vim.bo[source_buf].filetype ~= "markdown" then
		local ft = vim.filetype.match({ buf = source_buf, filename = path })
		if ft == "markdown" then
			vim.bo[source_buf].filetype = "markdown"
		else
			return 0
		end
	end

	local source_path = vim.api.nvim_buf_get_name(source_buf)
	local preview_buf = _G.wolf_ensure_preview_buf(source_buf, source_path)

	-- Bind the preview into the requested window, falling back to the current
	-- window when `win` is missing/zero/invalid.
	if win and win ~= 0 and vim.api.nvim_win_is_valid(win) then
		vim.api.nvim_win_set_buf(win, preview_buf)
	else
		vim.api.nvim_win_set_buf(0, preview_buf)
	end

	-- Re-emit so the front-end learns the pane now previews the linked file (a
	-- background-window bind fires no window event) and re-asserts the active
	-- source pane's filetype. Both are idempotent.
	if _G.wolf_emit_previews then
		_G.wolf_emit_previews()
	end
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end

	return source_buf
end

-- Preview tracking + dirty signal: wolf-core invokes this over RPC after ui_attach
-- (last, after view tracking), passing its own RPC channel id. It registers the
-- WolfPreview augroup:
--   * a window-scan (WinEnter/BufWinEnter/WinNew/WinClosed/BufWinLeave) that
--     rpcnotifies wolf_preview with a list of { preview_win, source_bufnr,
--     source_abs_path } triples -- one per window showing a b:wolf_preview_source
--     scratch buffer -- so the front-end learns which pane previews what source
--     (and, via the source bufnr, joins the live changedtick). Emits once
--     immediately so an already-open preview is reflected.
--   * markdown-guarded TextChanged/TextChangedI/BufWritePost autocmds that
--     rpcnotify wolf_markdown_dirty with { source_bufnr, changedtick } so the
--     front-end pulls fresh HTML on the next wake (OD6 live-on-change). The
--     changedtick advances on any buffer change, so a keystroke burst that the
--     shell debounces yields one reload.
function _G.wolf_setup_preview(chan)
	local group = vim.api.nvim_create_augroup("WolfPreview", { clear = true })

	local function emit_previews()
		local list = {}
		for _, win in ipairs(vim.api.nvim_list_wins()) do
			local buf = vim.api.nvim_win_get_buf(win)
			local src = vim.b[buf].wolf_preview_source
			if src ~= nil and src ~= "" then
				local source_bufnr = vim.b[buf].wolf_preview_source_bufnr or -1
				list[#list + 1] = { win, source_bufnr, src }
			end
		end
		vim.rpcnotify(chan, "wolf_preview", list)
	end

	-- Expose the emit closure (it closes over `chan`) so wolf_open_preview can
	-- re-emit after a background-window bind that fires no window event.
	_G.wolf_emit_previews = emit_previews

	vim.api.nvim_create_autocmd(
		{ "WinEnter", "BufWinEnter", "WinNew", "WinClosed", "BufWinLeave" },
		{
			group = group,
			callback = emit_previews,
		}
	)

	vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI", "BufWritePost" }, {
		group = group,
		callback = function(ev)
			if vim.bo[ev.buf].filetype == "markdown" then
				vim.rpcnotify(
					chan,
					"wolf_markdown_dirty",
					ev.buf,
					vim.api.nvim_buf_get_changedtick(ev.buf)
				)
			end
		end,
	})

	-- Initial emit so an already-open preview is reflected without a window event.
	emit_previews()
end
