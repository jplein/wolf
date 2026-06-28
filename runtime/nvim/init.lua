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

-- No soft-wrap so test column assertions are deterministic.
opt.wrap = false

-- Treat opened files as markdown by default (Wolf is a Markdown editor).
vim.filetype.add({ extension = { md = "markdown" } })

-- Enable built-in syntax highlighting (markdown) so the grid carries varied
-- hl_ids. Treesitter is deferred; this is the Phase 1 highlight source.
vim.cmd("syntax on")

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
