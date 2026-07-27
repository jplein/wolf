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

-- Swap files are ON (crash-recovery of unsaved edits). They live in a DURABLE,
-- Wolf-owned, per-project swap directory (WOLF_SWAP_DIR, resolved core-side like
-- state_dir() and STABLE across respawns of the same project) rather than next to
-- the user's files, so a fresh nvim can recover a crashed nvim's unsaved edits.
-- The trailing "//" makes nvim encode the full path into the swap file name (so
-- files with the same basename in different dirs never collide) and DROPS the
-- default leading "." entry (which would otherwise put a `.swp` next to the file).
-- Keep nvim's default updatecount/updatetime.
opt.swapfile = true
local swap_dir = vim.env.WOLF_SWAP_DIR
if swap_dir ~= nil and swap_dir ~= "" then
	opt.directory = swap_dir .. "//"
end

-- Recovery-aware swap policy: a SwapExists must NEVER surface the blocking E325
-- prompt. Decide from the swap's owner:
--   * DEAD owner (stale/unreadable swap, or the owning pid is gone) -> "r": silently
--     RECOVER the unsaved edit, then schedule a delete of the consumed swap.
--   * LIVE / UNCERTAIN owner (owner pid still alive, or the swap was written on a
--     DIFFERENT host so we cannot probe) -> "e": edit the clean on-disk copy anyway.
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

-- External-file reload: when the file shown in a buffer is changed on disk from
-- OUTSIDE the editor (e.g. `prettier --write`, `git checkout`), reload it into the
-- buffer. `autoread` is Neovim's built-in mechanism, but on its own it only
-- re-reads on certain triggers, so a `WolfAutoRead` augroup runs `:checktime` on
-- focus regain, buffer entry, and after `updatetime` idle (`CursorHold`/`CursorHoldI`).
-- `:checktime` only re-reads a buffer whose in-editor copy is UNMODIFIED (autoread's
-- own guard — an unsaved edit is never clobbered) and is a NO-OP on buffers with no
-- associated file (the terminal `buftype=terminal` drawer and the `buftype=nofile`
-- preview scratch), so it is harmless there. Wrapped in pcall + `silent!` so a
-- special buffer can neither raise nor pollute the grid.
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

-- Forward mouse to Neovim; the front-end translates OS mouse events to input.
opt.mouse = "a"

-- Present command-line completion as a vertical popup (`popupmenu_show`) rather
-- than the horizontal wildmenu, so the native `ext_popupmenu` completion popup
-- anchors above the command line (Wolf attaches with `ext_popupmenu` +
-- `ext_messages`; the latter implicitly enables `ext_cmdline` + `cmdheight=0`).
opt.wildoptions:append("pum")

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
-- scheme reads it. The front-end now pushes the resolved appearance at launch —
-- and live, in `auto` mode, when the OS toggles — via `:set background=…`; this
-- hardcoded value is only the pre-push fallback for the initial paint.
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
-- for the (non-markdown) preview pane. The THIRD arg is a boolean, is_preview:
-- true when the active buffer is a preview scratch (drives core's
-- Snapshot.is_preview_active, the Edit command's enable gate), false otherwise.
-- The FOURTH arg is a boolean, is_full_width: the effective buffer's
-- b:wolf_full_width flag (drives core's Snapshot.is_full_width_active, the View-menu
-- "Use full width for content" checkmark). For a preview scratch this reports the
-- SOURCE buffer's flag (OQ-1), so the checkmark tracks the document, not the scratch.
function _G.wolf_setup_current_file_tracking(chan)
	local group = vim.api.nvim_create_augroup("WolfCurrentFile", { clear = true })
	local function emit()
		local buf = vim.api.nvim_get_current_buf()
		if vim.b[buf].wolf_preview_source ~= nil then
			-- A preview scratch reports the SOURCE buffer's full-width flag (4th arg)
			-- so the View-menu checkmark matches the document (OQ-1).
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

-- Per-window full-width tracking: wolf-core invokes this over RPC after ui_attach
-- (right after buffer tracking), passing its own RPC channel id. It registers
-- autocmds that notify core, via wolf_full_width, of the full-width flag of the
-- buffer shown in EVERY window of the current tabpage -- a list of
-- { win, vim.b[buf].wolf_full_width == true } pairs -- so the front-end can thread a
-- pane's content-width mode (full vs the ~90-char default) into its geometry keyed by
-- window handle. It emits once immediately so the startup layout is reflected.
--
-- Mirrors wolf_setup_buffer_tracking: the whole current window set is re-emitted on
-- each autocmd and core WHOLESALE-REPLACES its per-window map, so stale/closed windows
-- drop out. Unlike buffer tracking, preview scratch panes are NOT skipped -- a window
-- absent from the map defaults to false core-side, and a preview pane's own flag is
-- irrelevant (the front-end frames the preview off the SOURCE flag via wolf_preview).
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

-- Batch treesitter highlighter for the Markdown preview's fenced/indented code
-- blocks. wolf-core (crate nvim.rs::highlight_code_blocks) invokes this over
-- `nvim_exec_lua` with ONE argument: an array of { text, lang } pairs (lang is ""
-- when the fence declares none). It returns an array `results` of the SAME length,
-- highlighting the whole batch in one RPC round-trip.
--
-- results[i] is either:
--   * the scalar `false` -- the i-th block is NOT highlightable (no/unknown
--     language, missing parser, missing `highlights` query, a parse failure, or no
--     resolved spans at all), OR
--   * an array of fixed 7-element span tuples
--     { row, start_col, end_col, fg, bold, italic, underline }.
-- A span is a single-line, non-overlapping byte-column run: `row` is 0-based within
-- the block text (text split on "\n"), `start_col`/`end_col` are 0-based half-open
-- BYTE columns, `fg` is a packed 0xRRGGBB integer or -1 (no foreground), and the
-- three attributes are Lua booleans.
--
-- CONTRACT: NEVER return an empty Lua table for a block -- msgpack cannot tell an
-- empty array from an empty map, so a non-highlightable OR empty-result block is the
-- scalar `false`. Each span is always a full 7-element array.
--
-- Defined at MODULE LEVEL (not inside the conditional treesitter block) so it exists
-- on EVERY platform. On Linux/cargo (no vendored parsers/queries) it self-degrades
-- to `false` for every block because get_string_parser / query.get find nothing.
function _G.wolf_highlight_code_blocks(blocks)
	-- Resolve a treesitter capture NAME (e.g. "function.call") to its effective
	-- highlight style via an @-prefixed `nvim_get_hl` lookup (link = false so a
	-- linked group resolves to its target's attributes -- the same pattern
	-- wolf_setup_theme_tracking uses). On an EMPTY result and a dotted name, retry
	-- against successively trimmed prefixes ("@function.call" -> "@function"), matching
	-- treesitter's group fallback. Returns { fg, bold, italic, underline } or nil when
	-- the group has no foreground AND no emphasis (skip it -- no empty <span style="">).
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

		-- Per-line, per-byte winner map: winners[r][col] = { style, prio, order }.
		-- `order` is a monotonic counter so a later capture wins a same-priority tie,
		-- matching nvim's "later capture wins" precedence.
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

	-- QUIT: the :q family (q/quit/wq/x/xit and their banged variants) routed into
	-- Wolf's pane/window model. `opts = { write = bool, force = bool, all = bool }`.
	--   * write variants (wq/x/xit) write first; a FAILED write ABORTS the quit
	--     (native parity -- a failed :wq must not close).
	--   * multi-pane (NON-all): guarded nvim_win_close(0, force) like do_close;
	--     force=false refuses to discard unsaved changes, force=true (from :q!)
	--     discards. No intent -- the shell observes the removal via the win_close
	--     redraw event.
	--   * last pane: fire the existing close_window registry command so the
	--     front-end closes the window -- BUT a plain :q on a modified buffer is
	--     REFUSED with a native-style E37 (:q! overrides), preventing data loss.
	--   * all=true (the :qa family: qa/qall/quitall/wqa/wqall/xa/xall + banged):
	--     SKIP the pane-count check so control ALWAYS reaches the close-window
	--     branch -- the whole front-end window closes regardless of how many panes
	--     are open. Same write/force/current-buffer-modified (E37) semantics as
	--     :q's last-pane branch; the modified guard stays scoped to the current
	--     buffer (vim.bo.modified), matching :q -- it is not expanded to scan all
	--     panes.
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

	-- DRAWER OPEN: report the newly created terminal-drawer window. Unlike a split
	-- this carries no `from`/`orientation` -- the front-end renders the drawer as a
	-- full-width, root-bottom pane held OUTSIDE its pure PaneTree. Reached from
	-- `wolf_toggle_terminal` (a module-level global) once the layout channel is set.
	local function emit_drawer(win)
		vim.rpcnotify(chan, "wolf_layout", {
			action = "drawer_open",
			win = win,
		})
	end

	-- Expose the helpers as globals so the cnoreabbrev Ex-command overrides below
	-- can invoke them from the command line via `lua ...` (and so `wolf_toggle_terminal`
	-- can emit the drawer intent over this setup's layout channel).
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

	-- Terminal-mode window chords so the drawer is keyboard-navigable WITHOUT first
	-- leaving terminal-insert. A `t`-mode function RHS fires the pure focus intent
	-- (`from` = the drawer window) without forwarding `<C-w>` to the shell. TRADEOFF:
	-- this shadows the shell's `<C-w>` word-erase inside the terminal -- accepted per
	-- the user's request. `<C-w>k` bridges drawer -> editor; the others are handled
	-- front-end-side (drawer is full-width, so h/l are no-ops there).
	map("t", "<C-w>h", function() do_focus("left") end, silent)
	map("t", "<C-w>j", function() do_focus("down") end, silent)
	map("t", "<C-w>k", function() do_focus("up") end, silent)
	map("t", "<C-w>l", function() do_focus("right") end, silent)

	-- Re-entry: auto-enter terminal-insert when focus lands IN the drawer (via
	-- `<C-w>j` from the editor, which drives `nvim_set_current_win` and fires
	-- WinEnter). Matched on the drawer's identity (`b:wolf_terminal` +
	-- `buftype == 'terminal'`) so it never yanks back from a deliberate
	-- `<C-\><C-n>` (same window, no WinEnter). VS Code parity.
	local terminsert = vim.api.nvim_create_augroup("WolfTerminalInsert", { clear = true })
	vim.api.nvim_create_autocmd({ "WinEnter", "BufWinEnter" }, {
		group = terminsert,
		callback = function()
			if vim.b.wolf_terminal and vim.bo.buftype == "terminal" then
				vim.cmd("startinsert")
			end
		end,
	})

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

	-- QUIT family: the bare word expands to an INTERNAL banged user command (NOT a
	-- registry command). Typing :q! fires the bare-`q` abbrev (getcmdline()=='q'
	-- still true), so `q` -> WolfQuit and the trailing `!` rides through to
	-- :WolfQuit! -- a valid banged invocation whose callback sees o.bang == true.
	-- The :qa family (qa/qall/quitall -> WolfQuitAll; wqa/wqall/xa/xall ->
	-- WolfWriteQuitAll) works the same way and passes all=true so do_quit always
	-- closes the window. `a` is a keyword char, so :qa forms a distinct word and
	-- never triggers the bare-`q` abbrev. These are internal plumbing user commands,
	-- NOT registry commands (no command.rs/FFI/Swift changes).
	-- CRITICAL: do NOT add q!/quit!/wq!/qa!/wqa! etc. abbrevs; `!` is not a keyword
	-- char and such an abbrev would be dead/broken. For the banged forms the `!`
	-- rides through to :WolfQuit! / :WolfQuitAll! / :WolfWriteQuitAll! (o.bang ->
	-- force).
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

-- Toggle the per-project terminal drawer. The terminal BUFFER is a single shared
-- buffer for this nvim instance (tagged b:wolf_terminal); the drawer WINDOW is
-- per-tabpage. Hiding closes the window only -- the terminal buffer/job persist
-- (VS Code behavior). Recreated on demand if the user exited the shell.
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

	-- 3. Otherwise open a bottom split ~1/3 tall showing the terminal
	--    (creating + tagging a fresh one if it is gone).
	local height = math.max(1, math.floor(vim.o.lines / 3))
	vim.cmd("botright " .. height .. "split")
	if termbuf and vim.api.nvim_buf_is_valid(termbuf) then
		vim.api.nvim_win_set_buf(0, termbuf)
	else
		vim.cmd("terminal")
		vim.b.wolf_terminal = true
	end
	-- Default the terminal drawer to FULL WIDTH. Capture the buffer actually bound
	-- into the drawer window (the reused termbuf on the persisted path, or the
	-- freshly-created terminal buffer entered by `:terminal`) and tag it. Still
	-- user-toggleable via :WolfFullWidth (this only sets the default).
	local termwin_buf = vim.api.nvim_get_current_buf()
	vim.b[termwin_buf].wolf_full_width = true
	-- Report the new drawer window so the front-end renders it as a full-width
	-- root-bottom pane (it lives OUTSIDE the pure PaneTree). `botright split`
	-- performs the real window op; this intent is what makes it VISIBLE. Guarded
	-- because the emitter only exists once `wolf_setup_layout` has run.
	local w = vim.api.nvim_get_current_win()
	if _G.wolf_emit_drawer then
		_G.wolf_emit_drawer(w)
	end
	-- Re-emit the full-width state on this same wake so the front-end learns the
	-- drawer's full-width default immediately -- the per-window `wolf_full_width`
	-- and current-file autocmds fire when the terminal buffer is ENTERED, BEFORE
	-- the var is set above, so an explicit re-emit is needed for determinism.
	-- current_file drives is_full_width_active (the View-menu checkmark for the
	-- focused terminal); full_width drives the drawer window's pane geometry.
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end
	if _G.wolf_emit_full_width then
		_G.wolf_emit_full_width()
	end
	vim.cmd("startinsert")
end

-- Toggle the active buffer's content width between the ~90-char default and full
-- pane width. Flips b:wolf_full_width on the EFFECTIVE buffer: when the current
-- buffer is a preview scratch (it carries b:wolf_preview_source_bufnr), the SOURCE
-- buffer is targeted (OQ-1) so the visible preview resizes and the document's editor
-- + preview views stay consistent; otherwise the current buffer is targeted. After
-- flipping, re-emits the current-file (menu checkmark), per-window full-width (editor
-- pane geometry), and previews (preview pane framing) notifications so the front-end
-- resizes the content. Defined alongside wolf_toggle_terminal.
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
-- It ALSO notifies core of EVERY tabpage's tab-local cwd via wolf_tab_cwds on
-- every DirChanged/TabNewEntered/TabClosed (emitting once immediately). The
-- payload is a list of { tabpage_handle, getcwd(-1, n) } pairs for all tabpages
-- (getcwd(-1, n) is the tab-local cwd, falling back to the global cwd for a tab
-- never `:tcd`'d). Core WHOLESALE-REPLACES its per-tabpage cwd map, so a closed
-- tab (TabClosed cannot report the closed handle) drops out. This lets core's
-- cwd_for_tabpage be a cheap snapshot read (no blocking RPC), authoritative even
-- for a NON-current view's cwd.
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

	-- Emit every tabpage's tab-local cwd as { handle, getcwd(-1, n) } pairs. Core
	-- wholesale-replaces its per-tabpage cwd map from this, so a closed tab drops
	-- out (TabClosed cannot report the closed handle).
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

-- Tier-1 open-file mirror: wolf-core invokes this over RPC after the command
-- setup, passing its own RPC channel id. It notifies core of the file-backed
-- buffers open PER TABPAGE (plus each tabpage's active buffer) via wolf_open_files
-- on every relevant buffer/window/tab event (emitting once immediately). Core
-- mirrors this Rust-side so the open-file set survives nvim death, and the
-- front-end reopens those files after a crash restart.
--
-- The payload is a list of { tp_handle, active_path, files } triples, one per
-- tabpage. `files` are the non-empty buffer names of the tabpage's windows,
-- EXCLUDING preview scratch buffers (b:wolf_preview_source) and unnamed buffers.
-- `active_path` is the tabpage's active window's buffer name (empty for unnamed).
function _G.wolf_setup_open_files(chan)
	local group = vim.api.nvim_create_augroup("WolfOpenFiles", { clear = true })

	local function buf_is_preview(buf)
		local ok, val = pcall(vim.api.nvim_buf_get_var, buf, "wolf_preview_source")
		return ok and val ~= nil
	end

	-- The whole nvim instance's file-backed (non-empty, non-preview) buflisted
	-- buffer set. Buffers are GLOBAL in nvim (shared across tabpages), so this is
	-- the authoritative "what files are open" set — it survives an `:edit` that
	-- hides (but keeps listed) an earlier buffer, which a per-window scan would
	-- miss. Preview scratch buffers and unnamed buffers are excluded.
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

-- ============================================================================
-- Anonymous-buffer naming
--
-- Give every new/empty EDITABLE buffer a real, unsaved markdown name --
-- "Untitled.md", then "Untitled 2.md", "Untitled 3.md", ... (index 1 is
-- un-numbered, so "Untitled 1.md" is NEVER generated) -- choosing the first name
-- that collides with neither an existing Neovim buffer NOR a file on disk in cwd.
-- Naming sets filetype=markdown (so the in-app preview gate at wolf_open_preview
-- passes) but NEVER writes the file to disk and NEVER changes
-- buftype/bufhidden/swapfile: an Untitled N.md buffer stays a normal, modifiable,
-- UNSAVED document until the user explicitly :w-saves it. Naming fires
-- BufFilePost + FileType, so the existing current-file / per-window trackers
-- re-emit the new name with no extra wiring.
-- ============================================================================

-- Pure name generator. Returns (name, abspath) for the first index i = 1, 2, 3,
-- ... whose name collides with neither a file/dir on disk in `cwd` nor any
-- existing buffer's name (skipping `exclude_buf`, the buffer being (re)named).
--   * i == 1 -> "Untitled.md" (un-numbered); the literal "Untitled 1.md" is
--     never generated.
--   * Disk collision: uv.fs_stat(abspath) ~= nil (file OR dir).
--   * Buffer collision: any valid buffer b ~= exclude_buf (listed OR unlisted,
--     loaded OR unloaded -- all can trigger E95) whose name == abspath.
-- Deliberately does NOT inspect .swp swap files -- only the .md file itself.
-- Whether a Wolf-owned swap file already exists for `abspath` (i.e. a crashed
-- nvim left unsaved edits for that exact path in WOLF_SWAP_DIR). Scans the swap
-- dir and matches on `swapinfo().fname` — the SAME identity nvim's own SwapExists
-- machinery uses — so it is robust to the `dir//` path-mangled swap filenames.
-- Only the VimEnter STARTUP naming consults this (as an extra collision), so a
-- fresh restart names its own startup buffer `Untitled 2.md` and the Tier-1 reopen
-- of `<cwd>/Untitled.md` recovers the crashed edit into its own buffer instead of
-- being shadowed. Never used by Cmd-N (`wolf_new_untitled`) or `Chdir`.
function _G.wolf_untitled_swap_exists(abspath)
	local uv = vim.uv or vim.loop
	local swap_dir = vim.env.WOLF_SWAP_DIR
	if not uv or swap_dir == nil or swap_dir == "" then
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
		if ok and type(info) == "table" and info.fname == abspath then
			return true
		end
	end
	return false
end

-- `extra_collision` (optional): a predicate `abspath -> bool` treated as an
-- ADDITIONAL collision source on top of the on-disk and in-buffer checks. Used
-- ONLY by the VimEnter startup naming (swap-existence); nil for Cmd-N / Chdir, so
-- their collision rules are unchanged.
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

-- Names `buf` with the first free Untitled name in cwd and sets its filetype to
-- markdown. Does NOT write to disk and does NOT touch buftype/bufhidden/swapfile.
-- On an E95 race (the name got taken between the collision walk and the set-name),
-- retries the next index (the now-colliding buffer is picked up by the next
-- collision walk, advancing the index); after a few attempts it bails gracefully
-- (leaves the buffer unnamed rather than raising).
-- `extra_collision` (optional) is forwarded to `wolf_next_untitled` (see there);
-- nil for every caller except the VimEnter startup naming.
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

-- Opens a new empty buffer (:enew) and names it the next free Untitled name.
-- Replaces the raw :enew for the NewBuffer input event. The PREVIOUS buffer, if
-- it carried a real name (e.g. an earlier Untitled.md), stays LISTED (a named
-- buffer is buflisted by default and :enew does not wipe it), so the collision
-- walk yields Untitled 2.md, then Untitled 3.md across repeated calls.
function _G.wolf_new_untitled()
	vim.cmd("enew")
	wolf_name_untitled(vim.api.nvim_get_current_buf())
end

-- GUARDED re-home of the CURRENT buffer into cwd as an Untitled name. Proceeds
-- ONLY when the current buffer is a normal (buftype == ""), UNMODIFIED, non-
-- preview buffer whose name is either empty OR an existing Untitled name that is
-- NOT yet on disk (so a real, saved file is never renamed). Used by the Chdir arm
-- (opening a folder from the empty state names the buffer Untitled.md IN the
-- opened folder) and by the VimEnter autocmd (first-launch / launch-on-folder).
-- `extra_collision` (optional) is forwarded to the naming path; only the VimEnter
-- startup caller passes it (swap-existence), so opening a folder via Chdir keeps
-- the plain collision rules.
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
		wolf_name_untitled(buf, extra_collision)
	end
end

-- On startup (first-launch and launch-on-folder), if there is exactly ONE listed
-- buffer and the current buffer passes the re-home guard, name it Untitled.md.
-- getcwd() at VimEnter is the launch cwd. The guard keeps this off crash-recovery-
-- restored buffers (modified / a non-empty non-Untitled name) and off buffers
-- opened for a real file passed on the command line.
--
-- Restart-shadow guard: after a crash-restart the fresh nvim's startup buffer is
-- named WITHOUT loading it (nvim_buf_set_name skips the swap check). If a crashed
-- nvim left a swap for `<cwd>/Untitled.md`, naming THIS buffer that same name would
-- shadow the Tier-1 reopen `:e <cwd>/Untitled.md` (it would merely switch to the
-- already-named empty buffer, SwapExists never fires, unsaved edits lost). So the
-- STARTUP naming ONLY treats an existing swap for a candidate name as a collision,
-- yielding `Untitled 2.md` here and letting the reopen recover cleanly into its own
-- buffer. Cmd-N / Chdir naming is unaffected (they pass no swap check).
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

-- Navigates from the ACTIVE preview pane back to its SOURCE file and returns the
-- source buffer's number. The inverse of wolf_open_preview. wolf-core calls this
-- over nvim_exec_lua from WolfCore::edit_source, passing `mode`:
--   * "split_left": single-pane placement -- capture the ORIGINAL window, split
--     (via wolf_do_split('v'), which emits a wolf_layout Vertical Split intent so
--     the shell creates+binds the new pane), then bind the source into the ORIGINAL
--     window and focus it. The front-end inserts the new leaf AFTER the original in
--     the PaneTree, so the original ends up the LEFT/editor pane and the new window
--     keeps the preview on the RIGHT -- editor left, preview right.
--   * "reuse_editor_pane": two-pane placement whose OTHER pane is an EDITOR (no
--     b:wolf_preview_source) -- bind the source into that OTHER window and focus it,
--     falling back to the current window when no other editor window is found.
--   * "active": replace the current preview pane's buffer with the source.
-- Gate: reads b:wolf_preview_source / b:wolf_preview_source_bufnr off the CURRENT
-- buffer; returns 0 (no-op) unless the active buffer is a preview scratch. Binding
-- prefers nvim_win_set_buf(win, source_bufnr) when the bufnr is a valid buffer
-- (window-scoped, no disk re-read, no MRU churn -- like wolf_switch_buffer); else
-- it focuses the target window and :edits the source path. No MRU recording. After
-- binding it re-emits the previews + current-file snapshots (both idempotent) so
-- the front-end + core's current_is_preview re-settle. Returns the source bufnr.
function _G.wolf_edit_source(mode)
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

	if mode == "split_left" then
		local orig = vim.api.nvim_get_current_win()
		wolf_do_split("v")
		edit_into(orig)
		vim.api.nvim_set_current_win(orig)
	elseif mode == "reuse_editor_pane" then
		local cur = vim.api.nvim_get_current_win()
		local target = nil
		for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
			if win ~= cur then
				local src = vim.b[vim.api.nvim_win_get_buf(win)].wolf_preview_source
				if src == nil or src == "" then
					target = win
					break
				end
			end
		end
		target = target or cur
		edit_into(target)
		vim.api.nvim_set_current_win(target)
	else
		-- "active" (and any unknown mode): replace the current window's preview.
		edit_into(0)
	end

	-- Re-emit so the front-end learns the pane now shows the source (a
	-- background-window bind fires no window event) and core re-settles
	-- current_is_preview. Both are idempotent.
	if _G.wolf_emit_previews then
		_G.wolf_emit_previews()
	end
	if _G.wolf_emit_current_file then
		_G.wolf_emit_current_file()
	end
	return source_bufnr or 0
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
--     source_abs_path, source_full_width } tuples -- one per window showing a
--     b:wolf_preview_source scratch buffer -- so the front-end learns which pane
--     previews what source (and, via the source bufnr, joins the live changedtick),
--     and frames the pane capped or full-width by the SOURCE buffer's
--     b:wolf_full_width flag. Emits once immediately so an already-open preview is
--     reflected.
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
