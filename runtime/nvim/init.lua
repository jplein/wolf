-- Wolf's captive Neovim config. Contract and decisions: runtime/nvim/README.md.

local opt = vim.opt

-- 'directory' and 'spellfile' are comma-separated lists, so a raw comma silently splits the value.
-- Spaces need no escaping; the parentheses drop gsub's second return value.
local function escape_option_path(p)
	return (p:gsub(",", "\\,"))
end

-- Swaps: see runtime/nvim/README.md#decisions. Escape the path first, then append
-- "//": it is nvim's full-path marker, not part of the path, and must stay unescaped.
opt.swapfile = true
local swap_dir = vim.env.WOLF_SWAP_DIR
if swap_dir ~= nil and swap_dir ~= "" then
	opt.directory = escape_option_path(swap_dir) .. "//"
end

-- Never let SwapExists raise E325: a dead owner gets "r" (recover, then delete the swap), else "e".
-- Fires only on the file-load path (neovim/neovim#23145); see Anonymous-buffer naming below.
local swap_group = vim.api.nvim_create_augroup("WolfSwap", { clear = true })
vim.api.nvim_create_autocmd("SwapExists", {
	group = swap_group,
	pattern = "*",
	callback = function()
		local uv = vim.uv or vim.loop
		local swapname = vim.v.swapname
		local info = vim.fn.swapinfo(swapname)
		local dead
		if type(info) ~= "table" or info.error ~= nil then
			-- Unreadable / malformed swap: treat as a stale (dead) owner.
			dead = true
		elseif type(info.host) == "string" and info.host ~= "" and info.host ~= vim.fn.hostname() then
			-- Written on another machine: we cannot probe that pid -> uncertain -> edit.
			dead = false
		elseif type(info.pid) == "number" and info.pid > 0 then
			-- Existence probe: signal 0 succeeds iff the process is alive; ESRCH (or
			-- any failure) means the owner is gone (dead).
			local ok = uv.kill(info.pid, 0)
			dead = not (ok == 0 or ok == true)
		else
			-- No usable owner pid: treat as dead.
			dead = true
		end

		if dead then
			vim.v.swapchoice = "r"
			-- Delete the consumed swap AFTER recovery completes (post-SwapExists).
			vim.schedule(function()
				pcall(vim.fn.delete, swapname)
			end)
		else
			vim.v.swapchoice = "e"
		end
	end,
})

-- Reload files changed outside the editor; :checktime never clobbers a modified buffer.
-- pcall + silent! so the terminal and preview buffers can neither raise nor pollute the grid.
opt.autoread = true
local autoread_group = vim.api.nvim_create_augroup("WolfAutoRead", { clear = true })
vim.api.nvim_create_autocmd({ "FocusGained", "BufEnter", "CursorHold", "CursorHoldI" }, {
	group = autoread_group,
	pattern = "*",
	callback = function()
		pcall(vim.cmd, "silent! checktime")
	end,
})

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
-- The default 1 draws an unpainted tabline once a second tabpage exists, stealing a grid row
-- (80x24 -> 80x23) and staling WinInfo start_row. Both front-ends draw their own tab chrome.
opt.showtabline = 0

-- Forward mouse to Neovim; the front-end translates OS mouse events to input.
opt.mouse = "a"

-- Vertical popupmenu_show completion, so the ext_popupmenu popup anchors above the command line
-- (ext_messages implies ext_cmdline and cmdheight=0).
opt.wildoptions:append("pum")

-- Soft-wrap prose at word boundaries (breakat). Rendering tests keep asserted content within the
-- grid width, so wrapping does not move their columns.
opt.wrap = true
opt.linebreak = true

-- Scroll wrapped lines by screen line, so wheel / <C-e> / <C-d> never skip a wrapped fragment.
opt.smoothscroll = true

-- Treat opened files as markdown by default (Wolf is a Markdown editor).
vim.filetype.add({ extension = { md = "markdown" } })

-- Register before syntax on, or a markdown startup file never fires it; see the spell entry in
-- runtime/nvim/README.md#decisions. SPELL_LANG also names the spellfile below, which must match it.
local SPELL_LANG = "en"
local spell_prose = { markdown = true, text = true, gitcommit = true }
local spell_group = vim.api.nvim_create_augroup("WolfSpell", { clear = true })
vim.api.nvim_create_autocmd("FileType", {
	group = spell_group,
	pattern = "*",
	callback = function(args)
		local prose = spell_prose[args.match] == true
		vim.opt_local.spell = prose
		if prose then
			vim.opt_local.spelllang = SPELL_LANG
		end
	end,
})

-- Global and set once: a new FileType autocmd would miss markdown startup files (syntax on order).
-- See notes/runtime/nvim/init.lua.md#spellfile-is-global.
local spell_dir = vim.env.WOLF_SPELL_DIR
if spell_dir ~= nil and spell_dir ~= "" then
	opt.spellfile = escape_option_path(spell_dir) .. "/" .. SPELL_LANG .. ".utf-8.add"
end

-- The highlight source for buffers treesitter does not attach to. Not markdown's: nvim starts
-- treesitter on markdown itself, gate or no gate; see runtime/nvim/README.md#decisions.
vim.cmd("syntax on")

-- Vendored orlando: macOS bundle sibling only, a no-op elsewhere. Sourced explicitly so it is
-- defined during init; its vim.g.loaded_orlando guard makes the later plugin phase a no-op.
local this_file = debug.getinfo(1, "S").source:sub(2) -- strip the leading '@'
local this_dir = vim.fn.fnamemodify(this_file, ":p:h")
local orlando_dir = this_dir .. "/orlando"
local uv = vim.uv or vim.loop -- vim.uv since Neovim 0.10; vim.loop fallback
if uv and uv.fs_stat(orlando_dir) then
	vim.opt.runtimepath:prepend(orlando_dir)
	vim.cmd.runtime("plugin/orlando.lua")
end

-- Vendored lighthouse: macOS bundle sibling only. Must follow syntax on (the last highlight writer
-- wins); background is set first and is only the fallback until the front-end pushes its own.
local lighthouse_dir = this_dir .. "/lighthouse"
if uv and uv.fs_stat(lighthouse_dir) then
	vim.opt.runtimepath:prepend(lighthouse_dir)
	vim.o.background = "dark"
	vim.cmd.colorscheme("lighthouse")
end

-- Vendored treesitter trees resolve sibling first, then environment. Do not reorder; see
-- runtime/nvim/README.md and notes/runtime/nvim/init.lua.md#vendored-tree-resolution.
local function resolve_vendored_dir(sibling, env_name)
	if uv and uv.fs_stat(sibling) then
		return sibling
	end
	local from_env = vim.env[env_name]
	if from_env ~= nil and from_env ~= "" and uv and uv.fs_stat(from_env) then
		return from_env
	end
	return nil
end

local nvim_treesitter_dir =
	resolve_vendored_dir(this_dir .. "/nvim-treesitter", "WOLF_NVIM_TREESITTER_DIR")
if nvim_treesitter_dir then
	vim.opt.runtimepath:prepend(nvim_treesitter_dir)
	local parsers_dir = resolve_vendored_dir(this_dir .. "/parsers", "WOLF_PARSERS_DIR")
	if parsers_dir then
		vim.opt.runtimepath:prepend(parsers_dir)
	end
	-- Bare sibling probe, no environment fallback: see runtime/nvim/README.md#limitations.
	local textobjects_dir = this_dir .. "/nvim-treesitter-textobjects"
	if uv.fs_stat(textobjects_dir) then
		vim.opt.runtimepath:prepend(textobjects_dir)
	end

	-- Inside the gate: require needs the vendored lua/ on rtp. setup() must precede the prepend.
	-- See notes/runtime/nvim/init.lua.md#user-parser-activation.
	local parsers_user_dir = vim.env.WOLF_PARSERS_USER_DIR
	if parsers_user_dir ~= nil and parsers_user_dir ~= "" and uv.fs_stat(parsers_user_dir) then
		vim.cmd.runtime("plugin/nvim-treesitter.lua")
		pcall(function()
			require("nvim-treesitter").setup({ install_dir = parsers_user_dir })
		end)
		vim.opt.runtimepath:prepend(parsers_user_dir)
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

-- See runtime/nvim/README.md#wolf_setup_current_file_trackingchan.
function _G.wolf_setup_current_file_tracking(chan)
	local group = vim.api.nvim_create_augroup("WolfCurrentFile", { clear = true })
	local function emit()
		local buf = vim.api.nvim_get_current_buf()
		if vim.b[buf].wolf_preview_source ~= nil then
			-- A preview scratch reports the SOURCE buffer's full-width flag (4th arg)
			-- so the View-menu checkmark matches the document.
			local source_bufnr = vim.b[buf].wolf_preview_source_bufnr
			local full_width = false
			if source_bufnr ~= nil and vim.api.nvim_buf_is_valid(source_bufnr) then
				full_width = vim.b[source_bufnr].wolf_full_width == true
			end
			vim.rpcnotify(chan, "wolf_current_file", "", "", true, full_width)
		else
			vim.rpcnotify(
				chan,
				"wolf_current_file",
				vim.fn.expand("%:p"),
				vim.bo.filetype,
				false,
				vim.b[buf].wolf_full_width == true
			)
		end
	end
	-- Exposed so a background bind, whose autocmd emit describes the wrong pane, can re-assert it.
	_G.wolf_emit_current_file = emit
	vim.api.nvim_create_autocmd({ "BufEnter", "BufWinEnter", "BufFilePost", "FileType" }, {
		group = group,
		callback = emit,
	})
	-- Initial emit so the startup buffer is reflected without a buffer switch.
	emit()
end

-- Set only through wolf_suppress_buffers: a flag left true kills buffer tracking for the session.
-- See runtime/nvim/README.md#wolf_suppress_buffersfn.
_G.wolf_buffers_suppressed = false

-- Nests: only the outermost call re-emits, and an error is re-raised after the flag is restored.
function _G.wolf_suppress_buffers(fn)
	local prev = _G.wolf_buffers_suppressed
	_G.wolf_buffers_suppressed = true
	local ok, err = pcall(fn)
	_G.wolf_buffers_suppressed = prev
	if not prev and _G.wolf_emit_buffers then
		_G.wolf_emit_buffers()
	end
	if not ok then
		error(err, 0)
	end
end

-- Spans every tabpage (nvim_list_wins). Coalesced through vim.schedule, never synchronous; see
-- runtime/nvim/README.md#decisions and notes/runtime/nvim/init.lua.md#buffer-tracking-emits.
function _G.wolf_setup_buffer_tracking(chan)
	local group = vim.api.nvim_create_augroup("WolfBuffers", { clear = true })
	-- Scans every window and notifies core. Only ever called from the scheduled
	-- callback below, i.e. at a settled point between event-loop iterations.
	local function emit_now()
		local list = {}
		for _, win in ipairs(vim.api.nvim_list_wins()) do
			local buf = vim.api.nvim_win_get_buf(win)
			-- Skip preview scratches: their name is not a real path, and win_buffers / previews must stay
			-- disjoint for the history fold.
			if vim.b[buf].wolf_preview_source == nil then
				list[#list + 1] = {
					win,
					vim.api.nvim_buf_get_name(buf),
					buf,
					vim.bo[buf].buftype == "terminal",
					vim.b[buf].wolf_terminal == true or vim.w[win].wolf_terminal == true,
				}
			end
		end
		vim.rpcnotify(chan, "wolf_buffers", list)
	end

	-- Arms a coalesced emit. A burst of autocmds within one event-loop iteration
	-- produces exactly ONE notification, carrying the SETTLED state.
	local pending = false
	local function emit()
		-- Suppression is checked when armed and again at delivery: a suppressed region may open between.
		if _G.wolf_buffers_suppressed or pending then
			return
		end
		pending = true
		vim.schedule(function()
			pending = false
			if _G.wolf_buffers_suppressed then
				return
			end
			emit_now()
		end)
	end
	-- Expose the emit closure so wolf_suppress_buffers can re-emit the final state
	-- once a multi-step window build finishes.
	_G.wolf_emit_buffers = emit
	vim.api.nvim_create_autocmd(
		-- TermOpen is where a `:terminal` buffer's buftype finally becomes
		-- "terminal"; the earlier BufEnter/BufFilePost see it as a plain buffer.
		{ "BufEnter", "BufWinEnter", "BufFilePost", "WinEnter", "WinNew", "WinClosed", "TermOpen" },
		{
			group = group,
			callback = emit,
		}
	)
	-- Initial emit so the startup window layout is reflected without a change.
	emit()
end

-- See runtime/nvim/README.md#wolf_setup_full_widthchan.
function _G.wolf_setup_full_width(chan)
	local group = vim.api.nvim_create_augroup("WolfFullWidth", { clear = true })
	local function emit()
		local list = {}
		for _, win in ipairs(vim.api.nvim_list_wins()) do
			local buf = vim.api.nvim_win_get_buf(win)
			list[#list + 1] = { win, vim.b[buf].wolf_full_width == true }
		end
		vim.rpcnotify(chan, "wolf_full_width", list)
	end
	-- Expose the emit closure so wolf_toggle_full_width can re-emit after flipping the
	-- flag (a background flip fires no window event).
	_G.wolf_emit_full_width = emit
	vim.api.nvim_create_autocmd(
		{ "BufEnter", "BufWinEnter", "WinEnter", "WinNew", "WinClosed" },
		{
			group = group,
			callback = emit,
		}
	)
	-- Initial emit so the startup window layout is reflected without a change.
	emit()
end

-- See runtime/nvim/README.md#wolf_setup_dir_trackingchan.
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

-- See runtime/nvim/README.md#wolf_setup_theme_trackingchan.
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

-- Never return an empty table for a block (msgpack reads it as a map): use false. Module level on
-- purpose; see runtime/nvim/README.md#wolf_highlight_code_blocksblocks.
function _G.wolf_highlight_code_blocks(blocks)
	-- Resolve a capture to its style via @name, trimming dotted suffixes like treesitter's fallback.
	-- nil when there is no fg and no emphasis, so no empty <span style=""> is emitted.
	local function resolve(name)
		local hl = nil
		local candidate = name
		while candidate ~= nil and candidate ~= "" do
			local ok, h = pcall(vim.api.nvim_get_hl, 0, { name = "@" .. candidate, link = false })
			if ok and type(h) == "table" and next(h) ~= nil then
				hl = h
				break
			end
			candidate = candidate:match("^(.*)%.[^.]*$")
		end
		if hl == nil then
			return nil
		end
		local fg = hl.fg or -1
		local bold = hl.bold == true
		local italic = hl.italic == true
		local underline = hl.underline == true
		if fg == -1 and not bold and not italic and not underline then
			return nil
		end
		return { fg = fg, bold = bold, italic = italic, underline = underline }
	end

	local function highlight_one(text, lang)
		if lang == nil or lang == "" then
			return false
		end
		local ok, parser = pcall(vim.treesitter.get_string_parser, text, lang)
		if not ok or parser == nil then
			return false
		end
		local query = vim.treesitter.query.get(lang, "highlights")
		if query == nil then
			return false
		end
		local ok2, trees = pcall(function()
			return parser:parse()
		end)
		if not ok2 or trees == nil or trees[1] == nil then
			return false
		end
		local root = trees[1]:root()
		local lines = vim.split(text, "\n", { plain = true })

		-- Cache resolved styles per capture name (false = a resolved-but-empty style,
		-- so we skip re-resolving). Returns the style table or nil to skip.
		local style_cache = {}
		local function resolve_cached(name)
			local cached = style_cache[name]
			if cached == nil then
				cached = resolve(name) or false
				style_cache[name] = cached
			end
			if cached == false then
				return nil
			end
			return cached
		end

		-- winners[r][col] = { style, prio, order }; order breaks priority ties so a later capture wins,
		-- as in nvim.
		local winners = {}
		local order = 0
		for id, node, metadata in query:iter_captures(root, text, 0, -1) do
			local name = query.captures[id]
			local style = resolve_cached(name)
			if style ~= nil then
				order = order + 1
				local prio = tonumber(metadata.priority) or 100
				local srow, scol, erow, ecol = node:range()
				for r = srow, erow do
					local line = lines[r + 1]
					if line ~= nil then
						local seg_start = (r == srow) and scol or 0
						local seg_end = (r == erow) and ecol or #line
						if seg_end > #line then
							seg_end = #line
						end
						local wr = winners[r]
						if wr == nil then
							wr = {}
							winners[r] = wr
						end
						for col = seg_start, seg_end - 1 do
							local incumbent = wr[col]
							if
								incumbent == nil
								or prio > incumbent.prio
								or (prio == incumbent.prio and order > incumbent.order)
							then
								wr[col] = { style = style, prio = prio, order = order }
							end
						end
					end
				end
			end
		end

		-- Coalesce each line's winning bytes into non-overlapping runs of one
		-- identical style; un-won bytes are plain gaps (no run emitted).
		local spans = {}
		for r, wr in pairs(winners) do
			local line = lines[r + 1]
			local len = (line ~= nil) and #line or 0
			local run_start = nil
			local run_style = nil
			local col = 0
			while col <= len do
				local w = (col < len) and wr[col] or nil
				local s = (w ~= nil) and w.style or nil
				if s ~= run_style then
					if run_style ~= nil then
						spans[#spans + 1] = {
							r,
							run_start,
							col,
							run_style.fg,
							run_style.bold,
							run_style.italic,
							run_style.underline,
						}
					end
					run_style = s
					run_start = col
				end
				col = col + 1
			end
		end

		if #spans == 0 then
			return false
		end
		return spans
	end

	local results = {}
	for i, b in ipairs(blocks) do
		local ok, res = pcall(highlight_one, b[1], b[2])
		if ok then
			results[i] = res
		else
			results[i] = false
		end
	end
	return results
end

-- See runtime/nvim/README.md#wolf_setup_layoutchan; split, close and focus are asymmetric.
function _G.wolf_setup_layout(chan)
	-- 'h' = :split (stacked), 'v' = :vsplit (side by side). nvim splits first, then both handles are
	-- reported.
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

	-- A failed write aborts the quit; a plain quit of a modified last pane is refused with E37.
	-- all=true skips the pane count; the modified guard stays current-buffer only, as for :q.
	local function do_quit(opts)
		if opts.write then
			local ok = pcall(vim.cmd, opts.force and "write!" or "write")
			if not ok then
				return
			end
		end
		if not opts.all and #vim.api.nvim_tabpage_list_wins(0) > 1 then
			pcall(vim.api.nvim_win_close, 0, opts.force or false)
		else
			if not opts.force and vim.bo.modified then
				vim.api.nvim_echo(
					{ { "E37: No write since last change (add ! to override)", "ErrorMsg" } },
					true,
					{}
				)
				return
			end
			vim.rpcnotify(chan, "wolf_command", "close_window")
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

	-- No from/orientation: the front-end holds the drawer outside its PaneTree as a root-bottom pane.
	local function emit_drawer(win)
		vim.rpcnotify(chan, "wolf_layout", {
			action = "drawer_open",
			win = win,
		})
	end

	-- Globals so the cnoreabbrev overrides and wolf_toggle_terminal can reach this channel.
	_G.wolf_do_split = do_split
	_G.wolf_do_close = do_close
	_G.wolf_do_quit = do_quit
	_G.wolf_do_focus = do_focus
	_G.wolf_emit_drawer = emit_drawer

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

	-- t-mode chords fire the focus intent without leaving terminal-insert. They shadow the shell's
	-- <C-w> word-erase, an accepted tradeoff; h/l are no-ops for the full-width drawer.
	map("t", "<C-w>h", function() do_focus("left") end, silent)
	map("t", "<C-w>j", function() do_focus("down") end, silent)
	map("t", "<C-w>k", function() do_focus("up") end, silent)
	map("t", "<C-w>l", function() do_focus("right") end, silent)

	-- Re-enter terminal-insert when focus lands in the drawer; keyed on the drawer's identity so a
	-- deliberate <C-\><C-n> (same window, no WinEnter) is never undone.
	local terminsert = vim.api.nvim_create_augroup("WolfTerminalInsert", { clear = true })
	vim.api.nvim_create_autocmd({ "WinEnter", "BufWinEnter" }, {
		group = terminsert,
		callback = function()
			if vim.b.wolf_terminal and vim.bo.buftype == "terminal" then
				vim.cmd("startinsert")
			end
		end,
	})

	-- Whole-word, ':'-only abbreviations: fragile, so the chords above are the primary path.
	-- :new / :vnew are treated as plain splits.
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

	-- Never add q! / wq! / qa! abbrevs: '!' is not a keyword char, so the bare-word abbrev fires and
	-- the '!' rides through to o.bang. See runtime/nvim/README.md#decisions.
	vim.api.nvim_create_user_command("WolfQuit", function(o)
		do_quit({ write = false, force = o.bang })
	end, { bang = true, nargs = 0 })
	vim.api.nvim_create_user_command("WolfWriteQuit", function(o)
		do_quit({ write = true, force = o.bang })
	end, { bang = true, nargs = 0 })
	abbrev("q", "WolfQuit")
	abbrev("quit", "WolfQuit")
	abbrev("wq", "WolfWriteQuit")
	abbrev("x", "WolfWriteQuit")
	abbrev("xit", "WolfWriteQuit")
	vim.api.nvim_create_user_command("WolfQuitAll", function(o)
		do_quit({ write = false, force = o.bang, all = true })
	end, { bang = true, nargs = 0 })
	vim.api.nvim_create_user_command("WolfWriteQuitAll", function(o)
		do_quit({ write = true, force = o.bang, all = true })
	end, { bang = true, nargs = 0 })
	abbrev("qa", "WolfQuitAll")
	abbrev("qall", "WolfQuitAll")
	abbrev("quitall", "WolfQuitAll")
	abbrev("wqa", "WolfWriteQuitAll")
	abbrev("wqall", "WolfWriteQuitAll")
	abbrev("xa", "WolfWriteQuitAll")
	abbrev("xall", "WolfWriteQuitAll")

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

-- Seeding package.loaded is what makes require('wolf') resolve under --clean; a global would not.
-- See runtime/nvim/README.md#wolf_setup_commandschan-commands.
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

-- Tabpage helpers (one nvim tabpage per front-end view), at module level so setup order is moot.

-- See runtime/nvim/README.md#wolf_create_tabpagecwd.
function _G.wolf_create_tabpage(cwd)
	vim.cmd("tabnew")
	if cwd ~= nil and cwd ~= "" then
		vim.cmd("tcd " .. vim.fn.fnameescape(cwd))
	end
	return vim.api.nvim_get_current_tabpage()
end

-- Handles come back in DFS leaf order, which must match the front-end's PaneTree.leaves(). See
-- runtime/nvim/README.md#wolf_rebuild_layoutspec_json.
function _G.wolf_rebuild_layout(spec_json)
	local ok, spec = pcall(vim.json.decode, spec_json)
	if not ok or spec == nil then
		return {}
	end

	local function realize(node)
		if node.leaf then
			local win = vim.api.nvim_get_current_win()
			if node.preview and node.path ~= nil and node.path ~= "" then
				-- Convert this window into a preview in place, keeping its handle for the DFS correspondence;
				-- on failure the inherited buffer stays and the handle is still returned.
				wolf_preview_file(win, node.path)
			elseif node.path ~= nil and node.path ~= "" then
				vim.cmd("edit " .. vim.fn.fnameescape(node.path))
			end
			return { win }
		end

		local children = node.children or {}
		local n = #children
		local vertical = node.orientation == "v"

		-- Regions open rightbelow, so they read left-to-right / top-to-bottom, child 0 first.
		local regions = { vim.api.nvim_get_current_win() }
		for _ = 2, n do
			vim.api.nvim_set_current_win(regions[#regions])
			vim.cmd(vertical and "rightbelow vsplit" or "rightbelow split")
			regions[#regions + 1] = vim.api.nvim_get_current_win()
		end

		-- Recurse into each region, gathering the DFS leaf handles in order.
		local handles = {}
		for i = 1, n do
			vim.api.nvim_set_current_win(regions[i])
			for _, h in ipairs(realize(children[i])) do
				handles[#handles + 1] = h
			end
		end
		return handles
	end

	return realize(spec)
end

-- See runtime/nvim/README.md#wolf_close_tabpagehandle.
function _G.wolf_close_tabpage(handle)
	local ok, n = pcall(vim.api.nvim_tabpage_get_number, handle)
	if not ok or n == nil then
		return
	end
	pcall(vim.cmd, n .. "tabclose")
end

-- See runtime/nvim/README.md#wolf_toggle_terminal.
function _G.wolf_toggle_terminal()
	-- 1. Find the tagged terminal buffer (buffers are global to the nvim).
	local termbuf = nil
	for _, b in ipairs(vim.api.nvim_list_bufs()) do
		if
			vim.api.nvim_buf_is_valid(b)
			and vim.b[b].wolf_terminal
			and vim.bo[b].buftype == "terminal"
		then
			termbuf = b
			break
		end
	end

	-- 2. If a window in the CURRENT tabpage shows it, hide (close) that window.
	--    Closing the window keeps the terminal buffer + job alive.
	if termbuf then
		for _, w in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
			if vim.api.nvim_win_get_buf(w) == termbuf then
				vim.api.nvim_win_close(w, false)
				return
			end
		end
	end

	-- 3. Otherwise open a bottom split ~1/3 tall, under wolf_suppress_buffers: the build's synchronous
	-- WinNew / BufEnter would otherwise push untagged drawer states into navigation history.
	local height = math.max(1, math.floor(vim.o.lines / 3))
	local termwin_buf
	wolf_suppress_buffers(function()
		vim.cmd("botright " .. height .. "split")
		-- Tag the drawer WINDOW immediately, so it is identifiable as the drawer
		-- from this point on regardless of which buffer ends up bound into it.
		vim.w.wolf_terminal = true
		if termbuf and vim.api.nvim_buf_is_valid(termbuf) then
			vim.api.nvim_win_set_buf(0, termbuf)
		else
			vim.cmd("terminal")
			vim.b.wolf_terminal = true
		end
		-- Default the drawer's buffer to full width (still toggleable via :WolfFullWidth).
		termwin_buf = vim.api.nvim_get_current_buf()
		vim.b[termwin_buf].wolf_full_width = true
	end)
	-- This intent is what makes the drawer visible; guarded until wolf_setup_layout has run.
	local w = vim.api.nvim_get_current_win()
	if _G.wolf_emit_drawer then
		_G.wolf_emit_drawer(w)
	end
	-- Explicit re-emit: the autocmds fired on entering the terminal, before wolf_full_width was set.
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end
	if _G.wolf_emit_full_width then
		_G.wolf_emit_full_width()
	end
	vim.cmd("startinsert")
end

-- See runtime/nvim/README.md#wolf_toggle_full_width.
function _G.wolf_toggle_full_width()
	local cur = vim.api.nvim_get_current_buf()
	-- Resolve the effective buffer: the SOURCE buffer when the current pane is a
	-- preview scratch, else the current buffer.
	local buf = cur
	local source_bufnr = vim.b[cur].wolf_preview_source_bufnr
	if source_bufnr ~= nil and vim.api.nvim_buf_is_valid(source_bufnr) then
		buf = source_bufnr
	end
	vim.b[buf].wolf_full_width = not (vim.b[buf].wolf_full_width == true)
	-- Re-emit the three state channels the toggle affects (guarded: the emitters only
	-- exist once their respective setup helpers have run).
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end
	if _G.wolf_emit_full_width then
		_G.wolf_emit_full_width()
	end
	if _G.wolf_emit_previews then
		_G.wolf_emit_previews()
	end
end

-- See runtime/nvim/README.md#wolf_tab_cwdhandle.
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

-- See runtime/nvim/README.md#wolf_setup_tabschan; the tabpage lockout is under Decisions.
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

	-- Wholesale-replaced core-side: TabClosed cannot name the closed tab, so replacement prunes it.
	local function emit_tab_cwds()
		local list = {}
		for _, tp in ipairs(vim.api.nvim_list_tabpages()) do
			local ok, n = pcall(vim.api.nvim_tabpage_get_number, tp)
			if ok and n ~= nil then
				local ok2, dir = pcall(vim.fn.getcwd, -1, n)
				if ok2 and type(dir) == "string" then
					list[#list + 1] = { tp, dir }
				end
			end
		end
		vim.rpcnotify(chan, "wolf_tab_cwds", list)
	end
	vim.api.nvim_create_autocmd({ "DirChanged", "TabNewEntered", "TabClosed" }, {
		group = group,
		callback = emit_tab_cwds,
	})
	-- Initial emit so the startup tab cwds are reflected without a change.
	emit_tab_cwds()

	-- Lock out the nvim-internal tabpage navigation/creation chords. `noremap`
	-- (the default) fully replaces the native behavior.
	local map = vim.keymap.set
	local silent = { silent = true }
	for _, lhs in ipairs({ "gt", "gT", "<C-w>gt", "<C-w>gT", "<C-PageUp>", "<C-PageDown>" }) do
		map("n", lhs, "<Nop>", silent)
	end

	-- Typed :tab* commands expand to an empty line; whole-word matching leaves :tcd/:lcd/:cd alone.
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

-- See runtime/nvim/README.md#wolf_setup_open_fileschan.
function _G.wolf_setup_open_files(chan)
	local group = vim.api.nvim_create_augroup("WolfOpenFiles", { clear = true })

	local function buf_is_preview(buf)
		local ok, val = pcall(vim.api.nvim_buf_get_var, buf, "wolf_preview_source")
		return ok and val ~= nil
	end

	-- Instance-wide buflisted set: survives an :edit that hides a listed buffer, unlike a window scan.
	local function listed_files()
		local seen = {}
		local files = {}
		for _, info in ipairs(vim.fn.getbufinfo({ buflisted = 1 })) do
			local buf = info.bufnr
			local name = info.name
			if name ~= nil and name ~= "" and not buf_is_preview(buf) and not seen[name] then
				seen[name] = true
				files[#files + 1] = name
			end
		end
		return files
	end

	local function emit()
		local files = listed_files()
		local list = {}
		for _, tp in ipairs(vim.api.nvim_list_tabpages()) do
			local active_path = ""
			-- The tabpage's active buffer (its current window's buffer name).
			local ok_win, win = pcall(vim.api.nvim_tabpage_get_win, tp)
			if ok_win then
				local ok_buf, abuf = pcall(vim.api.nvim_win_get_buf, win)
				if ok_buf and not buf_is_preview(abuf) then
					local name = vim.api.nvim_buf_get_name(abuf)
					if name ~= "" then
						active_path = name
					end
				end
			end
			list[#list + 1] = { tp, active_path, files }
		end
		vim.rpcnotify(chan, "wolf_open_files", list)
	end

	vim.api.nvim_create_autocmd({
		"BufEnter",
		"BufWinEnter",
		"BufWritePost",
		"BufFilePost",
		"BufDelete",
		"WinEnter",
		"TabEnter",
		"TabNewEntered",
		"TabClosed",
	}, {
		group = group,
		callback = emit,
	})
	-- Initial emit so the startup open-file set is reflected without an event.
	emit()
end

-- See runtime/nvim/README.md#wolf_setup_fs_eventschan.
function _G.wolf_setup_fs_events(chan)
	local group = vim.api.nvim_create_augroup("WolfFsEvents", { clear = true })
	vim.api.nvim_create_autocmd({ "BufWritePost", "BufFilePost", "BufDelete" }, {
		group = group,
		callback = function(args)
			-- Skip non-file buffers. pcall because args.buf can be invalid on BufDelete; failing open is safe:
			-- an extra rescan is harmless, a missed one is the bug.
			local ok, buftype = pcall(function()
				return vim.bo[args.buf].buftype
			end)
			if ok and buftype ~= "" then
				return
			end
			pcall(vim.rpcnotify, chan, "wolf_fs_changed", { args.event, vim.fn.fnamemodify(args.file, ":p") })
		end,
	})
	-- Separate callback: the buftype guard would swallow TermLeave. "", not fnamemodify("", ":p"),
	-- which fabricates the cwd; the element must be present or core drops the 1-element payload.
	vim.api.nvim_create_autocmd({ "ShellCmdPost", "FocusGained", "TermLeave" }, {
		group = group,
		callback = function(args)
			pcall(vim.rpcnotify, chan, "wolf_fs_changed", { args.event, "" })
		end,
	})
end

-- See runtime/nvim/README.md#wolf_setup_view_tracking.
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

	-- Keyed on (win, buf) so a pane never borrows another pane's scroll. Returns pcall's ok flag; see
	-- runtime/nvim/README.md#wolf_switch_bufferwin-buf.
	function _G.wolf_switch_buffer(win, buf)
		local ok = pcall(function()
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
		return ok
	end
end

-- Navigation-history helpers (back / forward / jump): window-scoped, never :edit.

-- The current-file re-emit is load-bearing after a background bind; the previews one is hardening.
-- See runtime/nvim/README.md#decisions.
function _G.wolf_history_set_buf(win, bufnr)
	if win == nil or bufnr == nil or win == 0 or bufnr == 0 then
		return 0
	end
	if not vim.api.nvim_win_is_valid(win) then
		return 0
	end
	if not vim.api.nvim_buf_is_valid(bufnr) then
		return 0
	end
	if not _G.wolf_switch_buffer(win, bufnr) then
		return 0
	end
	if _G.wolf_emit_previews then
		_G.wolf_emit_previews()
	end
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end
	return bufnr
end

-- Rebind an existing buffer before bufadd, so an unsaved Untitled N.md is restored, not re-read.
-- See runtime/nvim/README.md#wolf_history_openwin-path.
function _G.wolf_history_open(win, path)
	if path == nil or path == "" then
		return 0
	end
	if win == nil or win == 0 or not vim.api.nvim_win_is_valid(win) then
		return 0
	end
	local ok, buf = pcall(function()
		for _, b in ipairs(vim.api.nvim_list_bufs()) do
			-- Ordinary file buffers only: skips preview scratches, terminals and quickfix, and keeps resolve()
			-- off non-file names.
			if vim.api.nvim_buf_is_valid(b) and vim.bo[b].buftype == "" then
				local name = vim.api.nvim_buf_get_name(b)
				if name ~= "" and (name == path or vim.fn.resolve(name) == path) then
					vim.fn.bufload(b)
					return b
				end
			end
		end
		local added = vim.fn.bufadd(path)
		vim.fn.bufload(added)
		return added
	end)
	if not ok or buf == nil or buf == 0 then
		return 0
	end
	return _G.wolf_history_set_buf(win, buf)
end

-- Anonymous-buffer naming: Untitled.md, Untitled 2.md, ...; see runtime/nvim/README.md#decisions.

-- Normalize the stored home_replace'd owner path forward, never the query backward. pcall because
-- ~user can raise. See notes/runtime/nvim/init.lua.md#owner-path-matching.
local function swap_fname_matches(fname, abspath)
	if type(fname) ~= "string" or fname == "" then
		return false
	end
	if fname == abspath then
		return true
	end
	local ok, full = pcall(vim.fn.fnamemodify, fname, ":p")
	return ok and type(full) == "string" and full == abspath
end

-- The swap nvim itself picks: 'directory' ending in "//" selects %-mangled full-path names, and
-- findswapname raises ATTENTION only for the first (.swp) candidate. nil without WOLF_SWAP_DIR.
local function untitled_swap_file(abspath)
	local swap_dir = vim.env.WOLF_SWAP_DIR
	if type(swap_dir) ~= "string" or swap_dir == "" then
		return nil
	end
	local full = abspath
	if type(full) ~= "string" or full == "" then
		return nil
	end
	if full:sub(1, 1) ~= "/" then
		local ok, expanded = pcall(vim.fn.fnamemodify, full, ":p")
		if not ok or type(expanded) ~= "string" or expanded == "" then
			return nil
		end
		full = expanded
	end
	-- Collapse "//" (cwd "/" yields "//Untitled.md") as nvim would. Parenthesized to drop gsub's
	-- count; "%%" in the replacement is a literal "%".
	full = (full:gsub("//+", "/"))
	local mangled = (full:gsub("/", "%%"))
	local dir = (swap_dir:gsub("/+$", ""))
	return dir .. "/" .. mangled .. ".swp"
end

-- Two criteria, either sufficient: the exact mangled .swp, then an owner-path scan of the swap dir.
-- See runtime/nvim/README.md#wolf_untitled_swap_existsabspath.
function _G.wolf_untitled_swap_exists(abspath)
	local uv = vim.uv or vim.loop
	if not uv then
		return false
	end
	-- (a) The exact file nvim would find.
	local exact = untitled_swap_file(abspath)
	if exact ~= nil and uv.fs_stat(exact) ~= nil then
		return true
	end
	-- (b) The owner-path scan.
	local swap_dir = vim.env.WOLF_SWAP_DIR
	if swap_dir == nil or swap_dir == "" then
		return false
	end
	local fs = uv.fs_scandir(swap_dir)
	if not fs then
		return false
	end
	while true do
		local name = uv.fs_scandir_next(fs)
		if not name then
			break
		end
		local ok, info = pcall(vim.fn.swapinfo, swap_dir .. "/" .. name)
		if ok and type(info) == "table" and swap_fname_matches(info.fname, abspath) then
			return true
		end
	end
	return false
end

-- Pure: swap existence arrives only through extra_collision. See
-- runtime/nvim/README.md#wolf_next_untitledcwd-exclude_buf-extra_collision.
function _G.wolf_next_untitled(cwd, exclude_buf, extra_collision)
	local uv = vim.uv or vim.loop
	local i = 1
	-- Defensive upper bound: never expected to be hit in practice.
	while i < 10000 do
		local name = (i == 1) and "Untitled.md" or ("Untitled " .. i .. ".md")
		local abspath = cwd .. "/" .. name
		local collision = uv and uv.fs_stat(abspath) ~= nil
		if not collision and extra_collision then
			collision = extra_collision(abspath)
		end
		if not collision then
			for _, b in ipairs(vim.api.nvim_list_bufs()) do
				if b ~= exclude_buf and vim.api.nvim_buf_is_valid(b) then
					if vim.api.nvim_buf_get_name(b) == abspath then
						collision = true
						break
					end
				end
			end
		end
		if not collision then
			return name, abspath
		end
		i = i + 1
	end
	-- Fallback (never expected): a name unique enough to dodge any clash.
	local name = "Untitled " .. tostring((uv and uv.hrtime()) or os.time()) .. ".md"
	return name, cwd .. "/" .. name
end

-- pcall covers only E95: a stale swap's ATTENTION dialog is no error. Never setlocal noswapfile;
-- see notes/runtime/nvim/init.lua.md#swapfile-stays-on.
function _G.wolf_name_untitled(buf, extra_collision)
	local cwd = vim.fn.getcwd()
	for _ = 1, 8 do
		local _, abspath = wolf_next_untitled(cwd, buf, extra_collision)
		if pcall(vim.api.nvim_buf_set_name, buf, abspath) then
			break
		end
	end
	-- Filetype makes the in-app preview gate pass. pcall so a transient failure
	-- never surfaces as an RPC error.
	pcall(function()
		vim.bo[buf].filetype = "markdown"
	end)
end

-- The previous named buffer stays listed, so repeated calls walk Untitled 2.md, Untitled 3.md.
-- The swap check matters after a crash relaunch, where Untitled.md is free by buffer/disk rules.
function _G.wolf_new_untitled()
	vim.cmd("enew")
	wolf_name_untitled(vim.api.nvim_get_current_buf(), _G.wolf_untitled_swap_exists)
end

-- Never renames a saved file. extra_collision defaults to the swap check; see
-- runtime/nvim/README.md#wolf_name_untitled_currentextra_collision.
function _G.wolf_name_untitled_current(extra_collision)
	local uv = vim.uv or vim.loop
	if vim.bo.buftype ~= "" or vim.bo.modified then
		return
	end
	-- Never re-home a preview scratch pane.
	if vim.b.wolf_preview_source ~= nil then
		return
	end
	local buf = vim.api.nvim_get_current_buf()
	local name = vim.api.nvim_buf_get_name(buf)
	local ok = name == ""
	if not ok then
		-- Lua patterns cannot quantify a capture group, so match the un-numbered
		-- and numbered Untitled names separately: "Untitled.md" or "Untitled N.md".
		local base = vim.fn.fnamemodify(name, ":t")
		local is_untitled = base == "Untitled.md" or base:match("^Untitled %d+%.md$") ~= nil
		if is_untitled and uv and uv.fs_stat(name) == nil then
			ok = true
		end
	end
	if ok then
		wolf_name_untitled(buf, extra_collision or _G.wolf_untitled_swap_exists)
	end
end

-- Startup naming with one listed buffer. The swap check avoids the ATTENTION modal and shadowing
-- the Tier-1 reopen of a crashed Untitled.md; see runtime/nvim/README.md#decisions.
vim.api.nvim_create_autocmd("VimEnter", {
	group = vim.api.nvim_create_augroup("WolfUntitled", { clear = true }),
	callback = function()
		if #vim.fn.getbufinfo({ buflisted = 1 }) == 1 then
			wolf_name_untitled_current(_G.wolf_untitled_swap_exists)
		end
	end,
})

-- Per-source preview scratch-buffer map (source bufnr -> preview bufnr), a global
-- so wolf_open_preview reuses one preview buffer per source across invocations.
_G.wolf_preview_bufs = _G.wolf_preview_bufs or {}

-- See runtime/nvim/README.md#wolf_ensure_preview_bufsource_buf-source_path.
function _G.wolf_ensure_preview_buf(source_buf, source_path)
	local preview_buf = _G.wolf_preview_bufs[source_buf]
	if not preview_buf or not vim.api.nvim_buf_is_valid(preview_buf) then
		-- listed = false (the picker lists the SOURCE buffer instead), scratch =
		-- true (nofile / bufhidden=hide / noswapfile defaults).
		preview_buf = vim.api.nvim_create_buf(false, true)
		local rel = vim.fn.fnamemodify(source_path, ":.")
		-- E95 here means a different source with the same cwd-relative name (the map catches the same
		-- source); fall back to a bufnr-suffixed name so the second preview still opens.
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

-- See runtime/nvim/README.md#wolf_open_preview.
function _G.wolf_open_preview()
	if vim.bo.filetype ~= "markdown" then
		return 0
	end
	local source_buf = vim.api.nvim_get_current_buf()
	local source_path = vim.api.nvim_buf_get_name(source_buf)

	-- Emit the source cursor before the bind: afterwards emit_cursor's preview-scratch guard bails,
	-- leaving the new preview with no scroll anchor.
	if _G.wolf_emit_markdown_cursor then
		_G.wolf_emit_markdown_cursor(source_buf)
	end

	local preview_buf = _G.wolf_ensure_preview_buf(source_buf, source_path)

	-- In place: replace the current window's buffer with the preview scratch.
	vim.api.nvim_win_set_buf(0, preview_buf)

	-- Hardening: BufWinEnter already re-sweeps after the bind; the explicit emit makes the update
	-- independent of autocmd ordering. See runtime/nvim/README.md#decisions.
	if _G.wolf_emit_previews then
		_G.wolf_emit_previews()
	end
	-- Load-bearing: the bind's BufWinEnter ran with the scratch current; re-assert the active buffer.
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end
	return preview_buf
end

-- See runtime/nvim/README.md#wolf_edit_source.
function _G.wolf_edit_source()
	local cur_buf = vim.api.nvim_get_current_buf()
	local source_path = vim.b[cur_buf].wolf_preview_source
	if source_path == nil or source_path == "" then
		return 0
	end
	local source_bufnr = vim.b[cur_buf].wolf_preview_source_bufnr

	-- Bind the source into window `win` (0 = current). Prefer the bufnr bind; fall
	-- back to focusing the window and :editing the path when the bufnr is stale.
	local function edit_into(win)
		if source_bufnr ~= nil and vim.api.nvim_buf_is_valid(source_bufnr) then
			vim.api.nvim_win_set_buf(win, source_bufnr)
		else
			if win ~= 0 then
				vim.api.nvim_set_current_win(win)
			end
			vim.cmd("edit " .. vim.fn.fnameescape(source_path))
		end
	end

	-- In place: replace the current window's preview with the source.
	edit_into(0)

	-- The current-file re-emit is load-bearing, previews is hardening; see
	-- runtime/nvim/README.md#decisions.
	if _G.wolf_emit_previews then
		_G.wolf_emit_previews()
	end
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end
	return source_bufnr or 0
end

-- See runtime/nvim/README.md#wolf_preview_bufferwin-bufnr.
function _G.wolf_preview_buffer(win, bufnr)
	if bufnr == nil or bufnr == 0 or not vim.api.nvim_buf_is_valid(bufnr) then
		return 0
	end
	local source_path = vim.api.nvim_buf_get_name(bufnr)
	if source_path == "" then
		return 0
	end

	-- Markdown gate: honor an already-set filetype, else ask vim.filetype.match
	-- (by buffer + filename) and set it; a non-markdown buffer yields 0.
	if vim.bo[bufnr].filetype ~= "markdown" then
		local ft = vim.filetype.match({ buf = bufnr, filename = source_path })
		if ft == "markdown" then
			vim.bo[bufnr].filetype = "markdown"
		else
			return 0
		end
	end

	-- A picked buffer can be buflisted yet UNLOADED (`:badd`); load it so the
	-- render pull sees its lines. No-op for an already-loaded buffer.
	vim.fn.bufload(bufnr)

	local preview_buf = _G.wolf_ensure_preview_buf(bufnr, source_path)

	-- Bind the preview into the requested window, falling back to the current
	-- window when `win` is missing/zero/invalid.
	if win and win ~= 0 and vim.api.nvim_win_is_valid(win) then
		vim.api.nvim_win_set_buf(win, preview_buf)
	else
		vim.api.nvim_win_set_buf(0, preview_buf)
	end

	-- The current-file re-emit is load-bearing, previews is hardening; see
	-- runtime/nvim/README.md#decisions.
	if _G.wolf_emit_previews then
		_G.wolf_emit_previews()
	end
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end

	return bufnr
end

-- path is already absolute; this does no relative-path resolution. See
-- runtime/nvim/README.md#wolf_preview_filewin-path.
function _G.wolf_preview_file(win, path)
	if path == nil or path == "" then
		return 0
	end
	if vim.fn.filereadable(path) == 0 then
		return 0
	end

	local source_buf = vim.fn.bufadd(path)
	vim.fn.bufload(source_buf)

	local result = _G.wolf_preview_buffer(win, source_buf)
	if result ~= 0 then
		-- List the source only on success: it gives a preview-only file its picker row, and a rejected
		-- non-markdown path must not gain one.
		vim.bo[source_buf].buflisted = true
	end
	return result
end

-- Usable other-pane target: valid, not current (that is the in-place command), not the drawer.
-- Anything else sends the caller down the split path.
local function wolf_other_pane_target_ok(target_win)
	if target_win == nil or target_win == 0 then
		return false
	end
	if not vim.api.nvim_win_is_valid(target_win) then
		return false
	end
	if target_win == vim.api.nvim_get_current_win() then
		return false
	end
	local buf = vim.api.nvim_win_get_buf(target_win)
	if vim.b[buf].wolf_terminal == true or vim.w[target_win].wolf_terminal == true then
		return false
	end
	return true
end

-- Gated before any split. See runtime/nvim/README.md#wolf_preview_in_othertarget_win.
function _G.wolf_preview_in_other(target_win)
	if vim.bo.filetype ~= "markdown" then
		return 0
	end
	local source_buf = vim.api.nvim_get_current_buf()
	local source_path = vim.api.nvim_buf_get_name(source_buf)

	-- Emit the source cursor before the bind, as in wolf_open_preview.
	if _G.wolf_emit_markdown_cursor then
		_G.wolf_emit_markdown_cursor(source_buf)
	end

	local preview_buf = _G.wolf_ensure_preview_buf(source_buf, source_path)

	if wolf_other_pane_target_ok(target_win) then
		-- Reuse: bind into the existing target; focus stays where it is.
		vim.api.nvim_win_set_buf(target_win, preview_buf)
	else
		-- wolf_do_split("v") makes the new right window current; bind there, then refocus the editor.
		local from = vim.api.nvim_get_current_win()
		_G.wolf_do_split("v")
		vim.api.nvim_win_set_buf(0, preview_buf)
		vim.api.nvim_set_current_win(from)
	end

	-- Re-emits as in wolf_open_preview: current-file is load-bearing, previews is hardening.
	if _G.wolf_emit_previews then
		_G.wolf_emit_previews()
	end
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end
	return source_buf
end

-- Returns the bound window's buffer, deliberately unlike wolf_edit_source; see
-- runtime/nvim/README.md#wolf_edit_in_othertarget_win.
function _G.wolf_edit_in_other(target_win)
	local cur_buf = vim.api.nvim_get_current_buf()
	local source_path = vim.b[cur_buf].wolf_preview_source
	if source_path == nil or source_path == "" then
		return 0
	end
	local source_bufnr = vim.b[cur_buf].wolf_preview_source_bufnr

	-- Prefer the window-scoped bufnr bind; :edit the path when the bufnr is stale.
	local function bind_source_into(win)
		if source_bufnr ~= nil and vim.api.nvim_buf_is_valid(source_bufnr) then
			vim.api.nvim_win_set_buf(win, source_bufnr)
		else
			vim.api.nvim_set_current_win(win)
			vim.cmd("edit " .. vim.fn.fnameescape(source_path))
		end
	end

	local bound_win
	if wolf_other_pane_target_ok(target_win) then
		-- Reuse: bind the source into the target, then focus it (the pane now
		-- editing the document).
		bind_source_into(target_win)
		vim.api.nvim_set_current_win(target_win)
		bound_win = target_win
	else
		-- Split: the new (right) window keeps the scratch; the ORIGINAL window
		-- becomes the editor. Bind the source into `from` and focus it.
		local from = vim.api.nvim_get_current_win()
		_G.wolf_do_split("v")
		bind_source_into(from)
		vim.api.nvim_set_current_win(from)
		bound_win = from
	end

	-- Same re-emits as wolf_edit_source: current-file is LOAD-BEARING (focus and
	-- the current buffer changed), previews is hardening.
	if _G.wolf_emit_previews then
		_G.wolf_emit_previews()
	end
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end
	if bound_win ~= nil and vim.api.nvim_win_is_valid(bound_win) then
		return vim.api.nvim_win_get_buf(bound_win)
	end
	return 0
end

-- See runtime/nvim/README.md#wolf_setup_previewchan.
function _G.wolf_setup_preview(chan)
	local group = vim.api.nvim_create_augroup("WolfPreview", { clear = true })

	local function emit_previews()
		local list = {}
		for _, win in ipairs(vim.api.nvim_list_wins()) do
			local buf = vim.api.nvim_win_get_buf(win)
			local src = vim.b[buf].wolf_preview_source
			if src ~= nil and src ~= "" then
				local source_bufnr = vim.b[buf].wolf_preview_source_bufnr or -1
				-- 4th element: the SOURCE buffer's full-width flag, so the front-end
				-- frames this preview pane's web view capped (~90-char) or full-width.
				local full_width = false
				if source_bufnr ~= -1 and vim.api.nvim_buf_is_valid(source_bufnr) then
					full_width = vim.b[source_bufnr].wolf_full_width == true
				end
				list[#list + 1] = { win, source_bufnr, src, full_width }
			end
		end
		vim.rpcnotify(chan, "wolf_preview", list)
	end

	-- Exposed for re-emits: hardening after a bind, load-bearing after wolf_toggle_full_width, since
	-- writing a buffer variable fires no autocmd.
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

	-- Flat scalars (bufnr, line, char_col), never a table; core upserts them by source bufnr.
	local function emit_cursor(buf)
		if not buf or not vim.api.nvim_buf_is_valid(buf) then
			return
		end
		-- line('.') / charcol('.') read the current window, so only the current buffer may report.
		if buf ~= vim.api.nvim_get_current_buf() then
			return
		end
		-- Load-bearing: bufload() fires BufEnter in nvim's autocmd window with a fabricated (1, 0) cursor.
		-- Only a normal window has a real one; see runtime/nvim/README.md#decisions.
		if vim.fn.win_gettype() ~= "" then
			return
		end
		if vim.bo[buf].filetype ~= "markdown" then
			return
		end
		-- Explicit, unlike wolf_markdown_dirty's incidental exclusion: a :filetype detect on a scratch
		-- must never leak a cursor that describes the rendered text.
		if vim.b[buf].wolf_preview_source ~= nil then
			return
		end
		local line = vim.fn.line(".")
		-- 0-based character column for the wl-{L}-{C} markers. Past end-of-line in insert mode this is
		-- char_len, which the front-end tolerates.
		local char_col = vim.fn.charcol(".") - 1
		if char_col < 0 then
			char_col = 0
		end
		vim.rpcnotify(chan, "wolf_markdown_cursor", buf, line, char_col)
	end

	-- Expose the emit closure (it closes over `chan`) so wolf_open_preview can
	-- re-emit BEFORE it binds the preview scratch into a window -- see the call site.
	_G.wolf_emit_markdown_cursor = emit_cursor

	-- BufEnter/BufWinEnter are load-bearing: :edit fires CursorMoved before filetype detection, so a
	-- buffer only looked at would report no cursor. See runtime/nvim/README.md#decisions.
	vim.api.nvim_create_autocmd({ "CursorMoved", "CursorMovedI", "BufEnter", "BufWinEnter" }, {
		group = group,
		callback = function(ev)
			emit_cursor(ev.buf)
		end,
	})

	-- Initial emit so an already-open preview is reflected without a window event.
	emit_previews()
	-- ...and so the startup buffer's cursor is known without waiting for a move.
	emit_cursor(vim.api.nvim_get_current_buf())
end

-- Must stay the last statement: loading last lets init_user.lua override every Wolf default.
-- pcall keeps startup alive; the warning must contain the literal name init_user.lua.
local runtime_user_dir = vim.env.WOLF_RUNTIME_USER_DIR
if runtime_user_dir ~= nil and runtime_user_dir ~= "" then
	local user_init = runtime_user_dir .. "/init_user.lua"
	if uv and uv.fs_stat(user_init) then
		local ok, err = pcall(dofile, user_init)
		if not ok then
			vim.notify("Wolf: error in init_user.lua: " .. tostring(err), vim.log.levels.WARN)
		end
	end
end
