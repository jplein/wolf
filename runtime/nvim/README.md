# runtime/nvim

`init.lua` is the captive Neovim configuration wolf-core launches.

[`nvim.rs::spawn`](../../crates/wolf-core/src/nvim.rs) starts it as:

```
nvim --embed --clean -u <init.lua> -i NONE
```

with `NVIM_APPNAME=wolf`, the `XDG_*` dirs pinned under a private temp dir, `WOLF_SWAP_DIR` and `WOLF_SPELL_DIR` always exported, and `WOLF_RUNTIME_USER_DIR` / `WOLF_PARSERS_USER_DIR` exported when the platform state dir resolves (see [`state.rs`](../../crates/wolf-core/src/README.md#staters) for the resolvers).

Which copy is loaded depends on the tree. The macOS app resolves a `WOLF_INIT_LUA` override first, then the bundled `Contents/Resources/init.lua`, then the in-repo `runtime/nvim/init.lua` (see [`TabCore.swift`](../../crates/wolf-macos/Sources/WolfMacOS/TabCore.swift)'s `initLuaPath`).

`init.lua` self-locates its own directory and conditionally loads sibling trees — `orlando/`, `lighthouse/`, `nvim-treesitter/`, `parsers/`, `nvim-treesitter-textobjects/` — which the `.app` bundle places alongside it under `Contents/Resources/`. The wolf-gnome package installs the `orlando/` and `lighthouse/` siblings beside it under `share/wolf/nvim/`. Those two blocks are strict no-ops wherever their sibling is absent, as in a checkout.

The two treesitter trees resolve sibling first and environment second. With no sibling, the nvim-treesitter tree comes from `WOLF_NVIM_TREESITTER_DIR` and the compiled grammars from `WOLF_PARSERS_DIR`, both exported by the Linux devShell — see [`build.md`](../../build.md), which owns the environment. Sibling first is what keeps a bundle self-contained: a developer with those variables exported who launches the `.app` must not have its Mach-O parsers silently replaced by the Linux-built tree, and a `dlopen` failure inside a `pcall` would be the only symptom. A resolved path that has since vanished falls through to nothing rather than being prepended.

Everything wolf-core reaches is a global on `_G`. Channel ids and arguments travel as Lua varargs to `nvim_exec_lua`, never interpolated into the code string.

## The `_G.wolf_*` contract

### Setup helpers

wolf-core invokes these once after `ui_attach`, in the order [`api.rs`](../../crates/wolf-core/src/api.rs) gives. Each takes core's own RPC channel id, creates a `clear = true` augroup so re-invoking is idempotent, and emits once immediately so startup state is reflected without an event.

None of the notifications below is a redraw frame, so none bumps the snapshot `seq`. Which of them wake the front-end is the condition in [the core↔front-end contract](../../specs/architecture.md#the-corefront-end-contract). [`redraw.rs`](../../crates/wolf-core/src/redraw.rs) holds the decoding rules.

#### `wolf_setup_current_file_tracking(chan)`

Tracks the active buffer. A `BufEnter`/`BufWinEnter`/`BufFilePost`/`FileType` autocmd notifies **`wolf_current_file`** with four positional args: `expand('%:p')` (empty for an unnamed buffer), `vim.bo.filetype`, `is_preview`, and `is_full_width`, which core folds into `Snapshot.current_file` and its `current_*` siblings.

A preview scratch pane (carrying `b:wolf_preview_source`) reports `("", "", true, <source buffer's b:wolf_full_width>)`, so core never treats the `Preview: <name>` scratch name as a real file, and the full-width checkmark tracks the document. `FileType` is in the event list so the filetype follows the active buffer even when detection lands after the buffer switch. Exposes `_G.wolf_emit_current_file`.

#### `wolf_setup_buffer_tracking(chan)`

Reports what every window shows, on every tabpage, so the front-end can draw a per-pane statusbar and core can derive a per-pane navigation-history target; the [architecture spec](../../specs/architecture.md#output-render-state-core--front-end) lists which snapshot maps are global as a result. Notifies **`wolf_buffers`** with one list of five-element tuples `{ win, nvim_buf_get_name(buf), buf, is_drawer, terminal_id }`, on `BufEnter`/`BufWinEnter`/`BufFilePost`/`WinEnter`/`WinNew`/`WinClosed`. Core wholesale-replaces both `Snapshot.buffers` (path only, unnamed windows omitted) and `Snapshot.win_buffers` (every window, `path = nil` when unnamed), so closed windows drop out.

`terminal_id` is the buffer's `b:wolf_terminal_id`, the core terminal session a placeholder shows, or `0`. `is_drawer` is the drawer window's tag `w:wolf_drawer`, set by `wolf_toggle_terminal`. Windows showing a preview scratch are omitted, which keeps `win_buffers` and `previews` disjoint for the history fold. Emits are coalesced, as [Decisions](#decisions) says. Exposes `_G.wolf_emit_buffers`.

It also exposes `_G.wolf_sync_buffers`, which emits at once unless suppressed and then blocks on a `wolf_barrier` request that core answers only once that emit is in the snapshot.

#### `wolf_setup_full_width(chan)`

Notifies **`wolf_full_width`** with one list of `{ win, vim.b[buf].wolf_full_width == true }` pairs covering every window, on `BufEnter`/`BufWinEnter`/`WinEnter`/`WinNew`/`WinClosed`. Core wholesale-replaces `Snapshot.full_width`; a window absent from the map defaults to false. Preview panes are not skipped, but their own flag is unused: the front-end caps a preview by the source buffer's flag that `wolf_preview` carries. Exposes `_G.wolf_emit_full_width`.

#### `wolf_setup_layout(chan)`

Rebinds the window chords and the `:split`/`:close`/`:quit` families to report layout intents, because the shell owns pane layout. Notifies **`wolf_layout`** with one map carrying an `action` discriminator, parsed by [`layout.rs`](../../crates/wolf-core/src/layout.rs):

- `{ action = "split", orientation = "h"|"v", from, win }` — nvim splits first, so the window, its grid and `win_pos` exist, and the shell learns the orientation and handles.
- `{ action = "focus_dir", dir = "left"|"right"|"up"|"down", from }` — a pure intent that moves nothing: the shell resolves the neighbour in its own layout.
- `{ action = "drawer_open", win }` — from `wolf_toggle_terminal`, through `_G.wolf_emit_drawer`.

`<C-w>c`, `<C-w>q` and `:close` refuse the tab's last pane and report nothing ([why](#decisions)). The tab's last pane is its tabpage's last window but the drawer, which is in no pane tree.

It also notifies **`wolf_active_win`** with `nvim_get_current_win()` on `WinEnter`, and **`wolf_command`** with `close_tab` when a quit or Close Pane closes the tab or Close Tab's check passes. Exposes `_G.wolf_do_split`, `wolf_do_close`, `wolf_do_quit`, `wolf_do_focus`, `wolf_emit_drawer`, [`wolf_close_tab`](#wolf_close_tab) and [`wolf_close_pane`](#wolf_close_pane).

`_G.wolf_split_pane(orient)`, the split commands' entry, is `wolf_do_split` except in a terminal placeholder, where it notifies **`wolf_terminal_split`** with the session id and `orient`, because core starts the new session.

Bindings: normal-mode `<C-w>` with `s`/`v`/`c`/`q` and `h`/`j`/`k`/`l` plus arrows; `ZZ` and `ZQ` onto `do_quit`; `cnoreabbrev` for `split`, `sp`, `new`, `vsplit`, `vs`, `vnew`, `close`; the quit families through `:WolfQuit`, `:WolfWriteQuit`, `:WolfQuitAll` and `:WolfWriteQuitAll`; and `bd`, `bw` and their longer forms onto `:WolfBdelete` and `:WolfBwipeout` ([why](#decisions)). It also installs the `wolf --wait` tracking and `:cq`, which [`wolf_wait_register`](#wolf_wait_registerpath-id) describes.

#### `wolf_setup_dir_tracking(chan)`

Notifies **`wolf_cwd`** with `getcwd()` on `DirChanged` (pattern `*`, so `:cd`, `:lcd`, and `:tcd` all fire it). Core uses it to scope the file MRU by the live working directory.

The one-shot startup emit carries a distinguishing second argument `true`; `DirChanged` emits carry no second argument. Core tracks the flagged launch directory but does not record it in the recent-directories MRU — a passive launch cwd is not a directory the user opened.

#### `wolf_setup_theme_tracking(chan)`

Notifies **`wolf_theme`** with twenty flat integers, never a table, on `ColorScheme`: `cursor_fg`, `cursor_bg`, `titlebar_fg` and `titlebar_bg`, read from the optional `WolfCursor` and `WolfTitlebar` highlight groups, then `g:terminal_color_0` to `g:terminal_color_15`, each through `nvim_get_color_by_name`. That event also covers `:set background=…`, which re-sources the active scheme. Each value is a packed `0xRRGGBB` color or `-1` for an unset channel.

Groups are read with `link = false`, so a linked group resolves to its target. Neither group is painted on the grid, so this notification is core's only way to learn them: `WolfCursor` colors the app cursor, `WolfTitlebar` the titlebar background on both front-ends, ahead of `Normal`'s.

#### `wolf_setup_commands(chan, commands)`

Registers both user-facing faces of every Wolf command. `commands` is a list of `{ id, ex_command }` pairs core builds from `COMMAND_SPECS` in [`command.rs`](../../crates/wolf-core/src/command.rs).

For each pair the helper seeds `package.loaded["wolf"][id]` and creates a `:<ex_command>` user command (`nargs = 0`, `force = true`). Both faces notify **`wolf_command`** with the canonical id. Commands are user-triggered only: no autocmd, no emit on attach.

`close_tab`'s two faces run [`wolf_close_tab`](#wolf_close_tab) instead, so they notify only once the tab's check passes.

#### `wolf_setup_tabs(chan)`

Notifies **`wolf_tabpage`** with `nvim_get_current_tabpage()` on `TabEnter`/`TabNewEntered`, and **`wolf_tab_cwds`** with a list of `{ tabpage_handle, getcwd(-1, n) }` pairs covering every tabpage on `DirChanged`/`TabNewEntered`/`TabClosed`. `getcwd(-1, n)` is the tab-local directory, falling back to the global one for a tab never `:tcd`'d.

Core wholesale-replaces `Snapshot.tab_cwds`, since `TabClosed` cannot name the closed tab, which makes `cwd_for_tabpage` a snapshot read even for a tabpage that is not current. The helper also installs the tabpage lockout described under [Decisions](#decisions).

#### `wolf_setup_open_files(chan)`

Backs the crash-restart file restore. Notifies **`wolf_open_files`** with a list of `{ tabpage_handle, active_path, files }` triples, one per tabpage, on `BufEnter`/`BufWinEnter`/`BufWritePost`/`BufFilePost`/`BufDelete`/`WinEnter`/`TabEnter`/`TabNewEntered`/`TabClosed`. Core wholesale-replaces `Snapshot.open_files`, which the front-end reads before a restart and replays after.

`files` is the whole instance's file-backed buflisted buffer set, with preview scratch, unnamed buffers and every buffer with a `buftype` excluded. `active_path` is the tabpage's active window's buffer name, empty for an unnamed buffer, a preview scratch or a buffer with a `buftype`. The `buftype` filter keeps a scripted Neovim terminal's `term://` name out of the mirror, since replaying it after a crash would spawn a shell. See Limitations for what the global file set costs.

#### `wolf_setup_fs_events(chan)`

Feeds the file-search rescan. Notifies **`wolf_fs_changed`** with `{ event_name, abs_path }`: the buffer events `BufWritePost`/`BufFilePost`/`BufDelete` (file buffers only) carry the changed file's absolute path, and the refocus-edge events `ShellCmdPost`/`FocusGained` carry `""`. Core routes both into the rescan of every live file-search session (see [`files.rs`](../../crates/wolf-core/src/README.md#filesrs)); only `BufWritePost` is conditional, rescanning only for a path not already listed. There is no initial emit.

It is a dedicated event rather than a reuse of `wolf_open_files`, whose augroup also fires on `BufEnter`/`WinEnter`/`TabEnter` and would turn every focus change into a rescan. A terminal pane's focus loss forces its rescan in core, through `terminal_focus`, as [the file-tree rescan](../../specs/terminal-panes.md#the-file-tree-rescan) says.

`FocusGained` never fires on GNOME, which reports no focus, as Limitations says of `:checktime`. There, as everywhere, core's filesystem watcher is how external changes reach a session, and these events are its fallback.

#### `wolf_setup_view_tracking()`

Takes no channel id and emits nothing, so no scroll or cursor payload ever crosses the FFI boundary. It keeps an in-nvim `winsaveview()` cache per `(win, buf)` and defines `_G.wolf_switch_buffer` as a closure over it.

#### `wolf_setup_preview(chan)`

Registers three things in one augroup. A window scan on `WinEnter`/`BufWinEnter`/`WinNew`/`WinClosed`/`BufWinLeave` notifies **`wolf_preview`** with a list of `{ preview_win, source_bufnr, source_abs_path, source_full_width }` tuples, one per window showing a `b:wolf_preview_source` scratch buffer; core wholesale-replaces `Snapshot.previews`. `source_bufnr` is `-1` when the source is unknown, and `source_full_width` is the source buffer's flag, which decides whether the preview renders capped or full-width.

**A window whose `b:wolf_preview_source` is present but empty is filtered out of that list**, so a preview of a genuinely nameless buffer never enters `Snapshot.previews`. An unsaved buffer is not that case: [`wolf_new_untitled`](#wolf_new_untitled) names it `Untitled N.md` first.

Markdown-guarded `TextChanged`/`TextChangedI`/`BufWritePost` autocmds notify **`wolf_markdown_dirty`** with `{ source_bufnr, changedtick }`, which core upserts into `Snapshot.markdown_ticks` and the front-end joins to a preview's source to decide when to re-pull rendered HTML. `init.lua` never renders Markdown; `wolf_core::markdown` does.

`CursorMoved`/`CursorMovedI`/`BufEnter`/`BufWinEnter` autocmds notify **`wolf_markdown_cursor`** with three flat scalars `(source_bufnr, line, char_col)`: a 1-based `line('.')` and a 0-based `charcol('.') - 1`, a character index so it pairs with the `wl-{L}-{C}` markers with no conversion. Core upserts it into `Snapshot.markdown_cursors`. Only the valid, current markdown buffer of a normal window that is no preview scratch emits ([why](#decisions)).

Exposes `_G.wolf_emit_previews` and `_G.wolf_emit_markdown_cursor`.

#### `wolf_setup_terminals(chan)`

A `BufWipeout` autocmd notifies **`wolf_terminal_wiped`** with the id of a wiped buffer carrying `b:wolf_terminal_id`, a core terminal session's placeholder, and core hangs that session up. It is silent while Neovim quits (`v:dying` or `v:exiting` set), so quitting hangs up no session, and it has no initial emit.

It also defines `_G.wolf_open_terminal()`, which notifies **`wolf_terminal_open`** with the current window, and the whole-word `:terminal` abbreviations [the spec](../../specs/terminal-panes.md#what-goes-away) gives, which call it. The word expands even when arguments follow, so a typed `:terminal <cmd>` fails as Lua and opens nothing, and only a scripted `:terminal` makes a Neovim terminal.

#### `wolf_emit_current_file`, `wolf_emit_buffers`, `wolf_emit_full_width`, `wolf_emit_previews`, `wolf_emit_markdown_cursor`, `wolf_emit_drawer`

Each is a closure exposed by its setup helper, over that helper's channel, so it exists only after that setup has run. Every caller guards with `if _G.wolf_emit_… then`.

They re-emit when state changes without the autocmd that would report it: writing a buffer variable fires no event, and a background bind's autocmds describe the wrong pane, as [Decisions](#decisions) says.

### On-demand helpers

wolf-core drives these over `nvim_exec_lua`. They take no channel id.

#### `wolf_highlight_code_blocks(blocks)`

Highlights the Markdown preview's fenced and indented code blocks. Takes one array of `{ text, lang }` pairs (`lang` is `""` when the fence declares none) and returns an array of the same length, so a whole batch costs one round-trip.

`results[i]` is either the scalar `false`, never an empty array, which msgpack cannot tell from an empty map — unknown language, missing parser or `highlights` query, parse failure, or no resolved spans — or an array of 7-element span tuples `{ row, start_col, end_col, fg, bold, italic, underline }`. A span is a single-line, non-overlapping run: `row` is 0-based within the block text, `start_col` and `end_col` are 0-based half-open byte columns, `fg` is a packed `0xRRGGBB` integer or `-1`, and the three attributes are booleans. Capture names resolve through `@`-prefixed groups with dotted-prefix fallback, and later or higher-priority captures win a byte, matching nvim's precedence.

It is defined at module level, outside the conditional treesitter block, so it runs on every platform and gates per language: a fence in a language the pinned Neovim ships highlights with nothing vendored.

#### `wolf_create_tabpage(cwd)`

Runs `tabnew`, roots the new tabpage at `cwd` via `:tcd` when `cwd` is non-empty, and returns the tabpage's stable handle. An empty or nil `cwd` leaves the tab at the global working directory, which is the empty-state view.

#### `wolf_rebuild_layout(spec_json)`

Realizes a saved pane-split tree in the current tabpage's lone window and returns the window handles in depth-first leaf order, so each handle zips to its rebuilt leaf. A malformed spec returns an empty list.

`spec_json` is a JSON string: a leaf is `{ leaf = true, path, preview, terminal, zoomed }`, where an edit leaf may add `cursor = { line, col }` and `top_line`, the focused leaf adds `focused = true`, and a split is `{ orientation = "h"|"v", weights, children }`. Core's `app_state` builds it ([why](../../notes/wolf-core-decisions.md#the-rebuild-layout-wire-format)).

A preview leaf with a path becomes a preview of that source in place, keeping its window's handle; a plain leaf with a path is `:edit`ed and takes its cursor and `top_line` under [`wolf_set_window_cursor`](#wolf_set_window_cursorwin-line-col-top_line)'s clamps; an empty leaf keeps its inherited buffer. A terminal leaf's window is returned untouched, since its `path` is a directory: core binds the leaf's session there. nvim supplies no sizes: the front-end drives them from its own weights, then applies each view again ([why](../../notes/wolf-core-decisions.md#a-saved-view-is-applied-twice)).

It then makes the first zoomed leaf's window current, else the first focused leaf's ([why](../../notes/wolf-core-decisions.md#the-persisted-zoom-mark)).

It ends with `_G.wolf_sync_buffers`, so core reports every returned window before the RPC answers ([why a request](../../notes/wolf-core-decisions.md#threading-and-locking)). Without it, a commit straight after the RPC could persist the startup `Untitled.md` over each restored file.

#### `wolf_set_window_cursor(win, line, col, top_line)`

Applies a saved view to `win` inside `nvim_win_call`, so `win` need not be current and the current window is left alone; an invalid `win` does nothing. Core's `WolfCore::set_window_cursor` calls it.

It shares one local function with `wolf_rebuild_layout`'s edit leaves, so their clamps cannot drift apart. The line is clamped to the buffer, the column goes as given, and `top_line` is clamped to `[1, line_count]`.

#### `wolf_close_tabpage(handle)`

Resolves the stable handle to its current tabpage number and runs `<n>tabclose` under `pcall`. Tabpage numbers renumber as tabs open and close, which is why the handle cannot be used directly. A stale handle, or the last-tabpage error, degrades to a no-op — the front-end never closes the last view, it shuts the whole nvim down instead.

#### `wolf_close_tab()`

Close Tab's check, [`wolf_refuse_unsaved`](#wolf_refuse_unsaved), which `:qa` runs too. It returns `false` when the check refused with E37 or E162, and otherwise notifies `close_tab` and returns `true`. It ends no wait, so it must not reuse `do_quit`: Close Tab closes no window, so the shutdown's `1` answers each wait, as [§6](../../specs/cli-and-file-open.md#6-waiting) gives, and `EDITOR="wolf --wait"` sees a failed editor. Core's caller is `WolfCore::request_close_tab`, which a front-end runs before a user's close of the tab. Defined by `wolf_setup_layout`, as a closure over its channel.

#### `wolf_close_pane()`

Close Pane's action, which core's `InputEvent::ClosePane` runs. A pane that can close, the drawer or any pane but the tab's last, closes as `wolf_do_close` closes it. The tab's last pane takes `:q`'s path, `do_quit`, not `wolf_close_tab`'s: it refuses with E37 or E162 and notifies nothing, or else ends every wait of the tab with `0` and notifies `close_tab`. Vim's own `:close` refuses the last window, so a typed close never closes a tab, but Close Pane there closes the buffer's last window, which [§6](../../specs/cli-and-file-open.md#6-waiting) counts as a close that also closes its tab. Defined by `wolf_setup_layout`.

#### `wolf_tab_cwd(handle)`

Returns the tab-local working directory of the tabpage named by its stable handle, or `""` for a stale handle. No core caller: `cwd_for_tabpage` reads the `wolf_tab_cwds` snapshot map instead, because a per-tab blocking RPC turned a wedged nvim into a sequence of timeouts.

#### `wolf_refuse_unsaved()`

Whether closing the tab must be refused for unsaved changes. At the first modified buffer of this Neovim, which is the tab's ([§8](../../specs/application-state.md#8-one-neovim-per-tab)), it echoes Vim's refusal as an `ErrorMsg` and returns `true`; with none it echoes nothing and returns `false`. `do_quit` calls it unless forced, and core reaches it through [`wolf_close_tab`](#wolf_close_tab) and through `WolfCore::show_unsaved`, which echoes for the tab a refused Close Window or Quit shows.

The search takes the order of Neovim's `check_changed_any`. The current buffer gives `E37`, and any other `E162` with the name Vim gives. Unlike native `:qa`, it does not show the refused buffer, because a refusal changes nothing.

**Every modified buffer counts, whatever its `'buftype'`.** Neovim never marks a preview scratch or a terminal buffer modified, and a buftype filter would pass over a modified help, quickfix or prompt buffer, which Neovim's own `:qa` refuses on.

#### `wolf_has_unsaved()`

Close Window's and Quit's silent ask: whether [`wolf_refuse_unsaved`](#wolf_refuse_unsaved) would refuse, by the same search, echoing nothing. Core's caller is `WolfCore::request_unsaved`, which a front-end runs for every tab it would close, so a refusal echoes only in the tab it then shows. Like [`wolf_close_tab`](#wolf_close_tab), it ends no wait.

#### `wolf_switch_buffer(win, buf)`

Binds `buf` into `win` with a window-scoped `nvim_win_set_buf` — never `:edit`, so there is no disk re-read and no MRU churn — then restores that pane's saved view for that buffer, forcing the top of the buffer when the pane has never shown it.

The body is `pcall`-wrapped and the helper returns that `ok` flag, so a bind that throws after the caller's validity checks (an open cmdwin, a `'winfixbuf'` window, an unabandonable modified buffer) is reportable rather than silent.

#### `wolf_history_set_buf(win, bufnr)`

Binds `bufnr` into `win` through `wolf_switch_buffer` and returns `bufnr`, or `0` when either handle is nil, zero, or invalid, or when the bind itself failed. Core turns a `0` into a history-cursor revert, so a target that has been wiped never silently drifts the pane.

On success it calls `_G.wolf_emit_previews` and `_G.wolf_emit_current_file`, the second load-bearing and the first hardening, as [Decisions](#decisions) says.

#### `wolf_history_open(win, path)`

Binds the buffer for absolute `path` into `win`, loading it if needed, and delegates the bind, view restore, and re-emits to `wolf_history_set_buf`. Returns the bufnr or `0`.

It rebinds an ordinary file buffer whose name equals or `resolve()`s to `path` before it falls back to `bufadd` and `bufload`, because core canonicalizes `path`, which under a symlinked project root can differ from the name nvim kept, and a never-written `Untitled N.md` has no file id for nvim's own matching.

#### `wolf_toggle_terminal(id)`

Opens, focuses or hides the drawer on core session `id`, which `WolfCore::toggle_terminal` starts before it queues the call, by the rules of [The drawer](../../specs/terminal-panes.md#the-drawer); the drawer window is per-tabpage and tagged `w:wolf_drawer`. A focus emits nothing, since the drawer's own `WinEnter` reports it, and an open emits `drawer_open`, the intent that makes the drawer visible.

The helper, not a front-end, decides whether the drawer is the active pane, by Neovim's current window. Every focus change a view makes is a `focus_win` on core's ordered input channel ahead of the toggle, so the current window is the active pane when the call runs, where a front-end check would race the asynchronous `wolf_active_win`.

#### `wolf_terminal_bind(id, win, split)`

Shows core terminal session `id` in window `win` (`0` for the current one), splitting it first through `wolf_do_split` when `split` is `"h"` or `"v"`, and returns the window or `0`. It reuses a buffer already tagged with `id`, or creates the placeholder [Terminal panes](../../specs/terminal-panes.md#terminal-panes-in-the-pane-tree) specifies with `bufhidden=wipe`. It ends with `wolf_sync_buffers`, so core's snapshot holds the binding when the call returns.

#### `wolf_terminal_exited(id)`

Closes every window showing session `id`, giving a tabpage's last window a new empty buffer instead, and returns how many it closed or emptied. It then deletes every buffer still tagged with `id`, so a hidden drawer placeholder goes too: its wipe tells core the session is gone, and the next toggle starts a new shell.

#### `wolf_terminal_close(win)`

Closes window `win` unless it is the tab's last pane, the drawer not counting, as `wolf_do_close` does, for core's `<C-w>c` and `<C-w>q` in a terminal pane. The drawer itself closes whenever its tabpage holds another window.

#### `wolf_toggle_full_width()`

Flips `b:wolf_full_width` on the effective buffer, then re-emits current-file, per-window full-width, and previews, since writing a buffer variable fires no autocmd. The flag lives per buffer because full width is a property of the document. From a preview scratch with a valid `b:wolf_preview_source_bufnr` it targets the source, so toggling from either view resizes both. A terminal placeholder's flag starts `true`, as [the spec](../../specs/terminal-panes.md#terminal-panes-in-the-pane-tree) gives, and stays toggleable.

#### `wolf_suppress_buffers(fn)`

Runs `fn` with `wolf_buffers` emits suppressed through `_G.wolf_buffers_suppressed`, then re-emits once from the outermost call so core sees the settled state. Nests safely.

It exists because coalescing cannot fix a multi-step window build: a `botright split` fires `WinNew` and `WinEnter` synchronously while the new window still shows the previous buffer, untagged, so those emits must be suppressed, not corrected later. Set the flag only through this helper, which clears it even when `fn` errors: a flag left set kills buffer tracking until the process exits.

#### `wolf_ensure_preview_buf(source_buf, source_path)`

Creates or reuses the preview scratch buffer for `source_buf` and returns its bufnr, memoized in `_G.wolf_preview_bufs` keyed by source buffer number. The buffer is an unlisted scratch, is named `Preview: <cwd-relative path>`, is `nomodifiable`, and carries `b:wolf_preview_source` (the source's absolute path) and `b:wolf_preview_source_bufnr`. Those two variables are what the path-based trackers skip on and what the preview window scan reads.

Unlisted is deliberate: the buffer picker lists one row per file, the source buffer, which [`wolf_preview_file`](#wolf_preview_filewin-path) lists on success, so the scratch contributes no `Preview: ` row and its name serves only the window title. Two sources sharing a cwd-relative path get a bufnr-disambiguated name, so the second preview still opens.

#### `wolf_open_preview()`

Replaces the active pane's buffer with its preview scratch and returns the preview buffer's number, or `0` when the active buffer's filetype is not `markdown`, the same gate as the Preview command.

It re-emits the source buffer's cursor before the bind, because afterwards the cursor emit's preview-scratch guard bails and the new preview would have no anchor. Ends with the previews and current-file re-emits.

#### `wolf_edit_source()`

The inverse of `wolf_open_preview`: replaces the active pane's preview with its source file in place and returns the source bufnr, or `0` unless the current buffer is a preview scratch.

Binding prefers `nvim_win_set_buf` with the recorded source bufnr and falls back to `:edit`ing the recorded path when that bufnr is stale. No MRU is recorded. Ends with the same two re-emits.

#### `wolf_preview_buffer(win, bufnr)`

Previews an already-open buffer: binds `bufnr`'s preview scratch into window `win`, else the current window, and returns the source bufnr, or `0` when `bufnr` is invalid, unnamed, or not Markdown, on which the front-end falls back to a plain `switch_buffer`.

The gate honors an already-set filetype and otherwise asks `vim.filetype.match`, setting the filetype on a match. It `bufload`s the buffer, because a `:badd`-ed buffer can be buflisted yet unloaded and the render pull needs its lines. Ends with the same two re-emits.

#### `wolf_preview_file(win, path)`

The disk-path front half of `wolf_preview_buffer`: previews the Markdown file `path` in window `win`, where `path` is already absolute, resolved by the front-end from a clicked `file://` link or a picked file. Returns the source bufnr, or `0` when `path` is empty, unreadable, or not Markdown.

It `bufadd`s and `bufload`s the source, then delegates the gate, bind, and re-emits to `wolf_preview_buffer`. Only on success does it mark the source buflisted, which gives a preview-only file its buffer-picker row. It deliberately does not re-emit the cursor: the source is loaded in the background with no window, so it carries no meaningful position.

#### `wolf_preview_in_other(target_win)`

Previews the active pane's Markdown file in another pane and returns the source bufnr — not the scratch, since the front-end joins `previews()` rows on the source — or `0` unless the active filetype is `markdown`. The gate runs before any split, so a mis-timed palette activation never changes the layout. `target_win` is the front-end-resolved window, `0` when the front-end wants a split.

A usable target, a valid window other than the current one and not a terminal pane, receives the scratch with focus untouched. Anything else takes the split path: the new pane `wolf_do_split("v")` puts right of `from` gets the scratch. Focus stays on the source editor either way, following the edit-mode view of the document. The cursor pre-emit, the re-emits and the no-MRU rule are `wolf_open_preview`'s.

#### `wolf_edit_in_other(target_win)`

Edits the active preview pane's source in another pane and returns `nvim_win_get_buf` of the bound window, or `0` unless the current buffer carries `b:wolf_preview_source` — gated before any split, like its sibling. Returning the bound window's buffer rather than the recorded bufnr is deliberate: the stale-bufnr `:edit` fallback still reports non-zero, a divergence from `wolf_edit_source`.

A target `wolf_preview_in_other` would use has the source bound into it, as `wolf_edit_source` binds, and is focused. On the split path the new right window keeps the scratch and the original window takes the source and focus: editor left, preview right. No MRU is recorded, and the re-emits are `wolf_edit_source`'s.

#### `wolf_open_split_left(path)`

Opens `path` with `topleft vsplit`, a new window left of the whole tab, and returns that window, which is current. It emits no `wolf_layout` split, because a front-end applies a split intent to the pane it came from, never to the whole tab; the caller binds the window it is handed. A missing `path` opens as a new buffer for it.

#### `wolf_wait_register(path, id)`

Registers `wolf --wait` `id`, from core's `RegisterWait`, on the listed buffer named `path`, comparing full paths, then resolved ones. It never creates or loads a buffer. With no such buffer it ends the wait with `1` at once.

The `WolfWait` augroup, which `wolf_setup_layout` installs, notifies **`wolf_wait_ended`** with `(ids, status)`. A buffer ends its waits with `0` when [§6](../../specs/cli-and-file-open.md#6-waiting) counts it closed. A hide ends nothing: once the buffer shows in no window, a **`wolf_wait_live`** request asks core which of its waits are still connected, since a killed command leaves its wait registered ([mechanism](../../notes/runtime/nvim/init.lua.md#a-hide-asks-core)). While any is, it echoes `wolf --wait is still waiting for <file>` into the message history, which shows as a toast.

`:cq` and `:cquit` run `:WolfCquit`. In a window showing a waited buffer it ends that buffer's waits with `1` and `bdelete!`s it rather than quitting, because a captive Neovim that quits is a crash, which would end every wait in the tab ([§6](../../specs/cli-and-file-open.md#6-waiting)). Anywhere else it runs `cquit` with the same bang and argument.

#### `wolf_next_untitled(cwd, exclude_buf, extra_collision)`

Pure name generator. Returns `(name, abspath)` for the first index whose name collides with neither a file or directory on disk in `cwd` nor any existing buffer's name, skipping `exclude_buf`. Index 1 is un-numbered `Untitled.md`, so the literal `Untitled 1.md` is never generated.

Buffer collisions consider every valid buffer, listed or not and loaded or not, because any of them can raise E95. The optional `extra_collision(abspath)` predicate adds a collision source, so the generator stays pure; every naming call site passes [`wolf_untitled_swap_exists`](#wolf_untitled_swap_existsabspath).

#### `wolf_name_untitled(buf, extra_collision)`

Names `buf` with the first free Untitled name in `getcwd()` and sets its filetype to `markdown`, which is what makes the in-app preview gate pass. It never writes to disk and never touches `buftype` or `bufhidden`, so the buffer stays a normal, modifiable, unsaved document until the user saves it. `swapfile` changes only through the long-path guard under Decisions.

`nvim_buf_set_name` is retried on an E95 race, and after a few attempts the buffer is left unnamed rather than raising; a stale swap is avoided by name, never caught ([Decisions](#decisions)). Naming fires `BufFilePost` and `FileType`, so the trackers re-emit the new name; inside the startup `VimEnter` callback `BufFilePost` does not fire, so it runs the long-path swap sync itself ([why](../../notes/runtime/nvim/init.lua.md#long-path-swaps)).

#### `wolf_name_untitled_current(extra_collision)`

Guarded re-home of the current buffer into the working directory. It proceeds only when the buffer is normal (`buftype == ""`), unmodified, not a preview scratch, and either unnamed or carrying an existing `Untitled.md` / `Untitled N.md` name that is not yet on disk — so a real saved file is never renamed. Core's `Chdir` arm calls it after a `:tcd`, and the `WolfUntitled` `VimEnter` autocmd calls it at startup when exactly one buffer is listed.

#### `wolf_new_untitled()`

Runs `:enew` and names the resulting buffer. The previous buffer stays listed if it carried a real name, so repeated calls walk `Untitled.md`, `Untitled 2.md`, `Untitled 3.md`.

#### `wolf_untitled_swap_exists(abspath)`

Whether a Wolf-owned swap file already exists for `abspath`, meaning a crashed nvim left unsaved edits for that exact path. Every naming call site passes it as the `extra_collision` predicate, so no naming path opens a swap beside a stale one ([Decisions](#decisions)).

Two criteria, either sufficient: the `%`-mangled `.swp` nvim's own `findswapname` would pick, and any swap in `WOLF_SWAP_DIR` whose recorded owner path, expanded forward from nvim's `~<user>/…` form, is `abspath` ([why](../../notes/runtime/nvim/init.lua.md#owner-path-matching)).

## Decisions

**Swap files are on, and `-n` is deliberately absent from the launch line.** `-n` forces `updatecount = 0`, which disables swap writing and makes crash recovery of unsaved edits impossible ([architecture.md](../../specs/architecture.md#8-the-captive-neovim-is-hermetic-pinned-and-markdown-only) §8). Swaps live in `WOLF_SWAP_DIR`, a durable Wolf-owned per-project directory stable across respawns, so a fresh nvim can recover a crashed one's edits. The trailing `//` on `'directory'` makes nvim encode the full path into each swap name, so identical basenames in different directories do not collide, and it drops the default leading `.` entry that would put a `.swp` beside the user's file.

That encoding holds only while both the full path and its resolved path fit in `SWAP_NAME_MAX`: nvim cuts a longer name, and recovery searches for the uncut one and never finds it. Such a buffer gets `noswapfile` from the `WolfSwap` autocmds, and gets it back on a rename to a short path unless a swap already exists there ([mechanism](../../notes/runtime/nvim/init.lua.md#long-path-swaps)).

**`'shortmess'` carries `A`, so an existing swap raises no prompt and no notice**, as [§8](../../specs/application-state.md#8-one-neovim-per-tab) gives: a live owner's swap would otherwise stop a second Neovim at the E325 prompt, on the rename path too, or toast the `W325` notice of Neovim's default `SwapExists` handler. Under `A` Neovim takes the next swap name. `A` also skips `SwapExists`, so the `WolfSwap` `BufNew` autocmd drops it for one load whose `.swp` belongs to a dead owner ([mechanism](../../notes/runtime/nvim/init.lua.md#dead-owner-re-arm)).

**The `SwapExists` autocmd never lets the blocking E325 prompt show**: a dead owner's swap is recovered with `v:swapchoice = "r"` and then deleted, and any other is edited past with `"e"`. It fires only on the file-load path, where nvim arms the choice machinery, so the naming path is defended by name avoidance, below.

**`autoread` alone re-reads only on some triggers**, so the `WolfAutoRead` augroup drives `:checktime` on focus regain, buffer entry and idle. Focus regain is the `FocusGained` that `WolfCore::set_ui_focus(true)` fires, so a file one tab saves reloads in another when that tab gains focus.

**A colorscheme on `runtimepath` does nothing on its own**, unlike a plugin, so the lighthouse block both prepends its directory and runs `:colorscheme`.

**The vendored nvim-treesitter is a query provider and, when `WOLF_PARSERS_USER_DIR` resolves, an activated plugin.** The gate prepends its tree, so its queries rank ahead of Neovim's own, and the vendored grammars. With that durable directory, the block also sources `plugin/nvim-treesitter.lua`, which registers `:TSInstall`, passes the directory to `setup({ install_dir = … })`, and prepends it to `runtimepath` ahead of the bundled parsers, so a user-compiled parser shadows the bundled one for its language ([details](../../notes/runtime/nvim/init.lua.md#user-parser-activation)). `markdown` and `markdown_inline` ship with nvim and are not re-vendored.

**What the gate buys is injected fenced-language highlighting, not markdown highlighting.** Neovim starts treesitter on a markdown buffer with no gate at all, so `vim.treesitter.highlighter.active` cannot witness the gate, and only an injected child langtree for the fenced language can ([measurement](../../notes/runtime/nvim/init.lua.md#vendored-tree-resolution)).

**Spell checking is prose-only, from a `WolfSpell` FileType autocmd registered before `syntax on`.** Measured on the pinned nvim, one registered after `syntax on` never fires for a markdown startup file argument. Its pattern is `*`, setting `'spell'` off for every other filetype, because nvim's reset from the global value on a buffer switch turns spell on everywhere once anything sets `vim.o.spell`. `'spelllang'` is `en`, the only `.spl` the pinned runtime ships, since a missing one opens a blocking download prompt with nowhere to draw under `ext_messages`.

The durable dictionary is the global `'spellfile'` at `WOLF_SPELL_DIR/en.utf-8.add` ([why global](../../notes/runtime/nvim/init.lua.md#spellfile-is-global)). Core creates that directory before launch, because nvim never creates an explicitly set spellfile's parent and `zg` would fail with E484 silently.

**`init_user.lua` loads last, and its errors warn instead of failing startup**, so user settings override every Wolf default and a broken user file costs a `:messages` warning rather than the editor. Core seeds it once into an empty directory and never rewrites it, as [`state.rs`](../../crates/wolf-core/src/README.md#staters) describes.

**A typed close reports no intent.** `<C-w>c`, `<C-w>q` and `:close` run a guarded `nvim_win_close` that refuses the tab's last pane, because the shell already observes `win_close`.

**The `:q` family routes through internal user commands, not registry commands**, and closes the tab as [§5](../../specs/command-lifecycle.md#5-invocation--drain) gives. A failing write variant aborts the quit, as native `:wq` does: `:wqa` and `:xa` first run `:wall`, whose failure aborts. Unless forced, the tab closes only once [`wolf_refuse_unsaved`](#wolf_refuse_unsaved) finds nothing unsaved, hidden buffers included, because the tab's Neovim and all its buffers end when the tab closes. Normal-mode `ZZ` and `ZQ` map onto it too, because the native keys quit Neovim on a tab's last pane, a crash that ends every wait in the tab with `1` ([§6](../../specs/cli-and-file-open.md#6-waiting)).

**A quit that closes the tab ends the waits of every buffer of the tab before it notifies `close_tab`.** The front-end shuts the tab's core down on that command, which ends every wait still held with `1`, and the `0` must reach the command first ([mechanism](../../notes/runtime/nvim/init.lua.md#waits-end-before-the-tab-closes)).

**`:bd` and `:bw` are intercepted, because beside the drawer they would close the tab's last pane.** Neovim keeps a window showing the deleted buffer only when it is the tabpage's last non-floating window, which the drawer stops a pane from being. When the deletion would empty every window but the drawer, and the drawer is not current, `do_bdelete` first shows in the current window the buffer Neovim would pick there (`bdelete_replacement`, a port of Neovim 0.12's `do_buffer_ext`) or a new empty one. Then Neovim's own command runs, so other windows close as Neovim closes them and a `wolf --wait` on the buffer still ends with `0`.

A target counts as deleted only with `!` or when unmodified, so a refused `:bd` of a modified buffer switches nothing. An error from Neovim's command echoes as its `E…` message, as E37 and E162 do, since a Lua user command would print a traceback. `:WolfCquit` deletes through the same helper.

**There are deliberately no `q!`, `wq!` or `qa!` abbreviations.** `!` is not a keyword character, so such an abbreviation would be dead: `:q!` fires the bare-`q` abbreviation and the `!` rides through to `:WolfQuit!`, whose callback reads `o.bang`. `a` is a keyword character, so `:qa` forms a distinct word that never collides with bare `q`.

**Tabpage navigation and creation chords map to `<Nop>`, and the typed `:tab*` commands expand to an empty command line, because Wolf owns all tabpage lifecycle**: a typed `:tabnew` or `:tabclose` would create or close a tabpage under the front-end. `:tcd`, `:lcd` and `:cd` stay open on purpose: the whole-word abbreviations never catch them, and core's `Chdir` applies `:tcd`.

**`wolf_buffers` emits are coalesced through `vim.schedule`, and only `wolf_sync_buffers` emits synchronously.** Core's history fold pushes on every observed change, so an intermediate wrong emit persists even if a later emit is correct ([examples](../../notes/runtime/nvim/init.lua.md#buffer-tracking-emits)).

**A background `nvim_win_set_buf` does fire `BufEnter` and `BufWinEnter`**, with the target window temporarily current ([measurement](../../notes/runtime/nvim/init.lua.md#buffer-tracking-emits)), so the autocmd-driven current-file emit describes the background pane and `wolf_emit_current_file` is load-bearing after any background bind. `wolf_emit_previews` is hardening: its `nvim_list_wins()` sweep reads no current window, and the re-emit only makes the update independent of autocmd ordering.

**The cursor emit needs both buffer-entry events and the `win_gettype()` guard, and neither is redundant.** Without `BufEnter` and `BufWinEnter` a buffer opened by `:edit` and never moved in reports no cursor, since `CursorMoved` fires before filetype detection. Without the guard, a `bufload()` firing both inside nvim's synthetic autocmd window reports a fabricated line 1, where a background-loaded source must keep the `line == 0` unknown sentinel.

**Naming a buffer treats an existing swap for the candidate name as a collision, at every naming call site.** Naming creates the swap at once, on a path where nvim has not armed `SwapExists`, so beside a stale swap the new one lands on a `.swo`, which no later session offers ([mechanism](../../notes/runtime/nvim/init.lua.md#swapfile-stays-on)). Yielding `Untitled 2.md` also keeps a fresh startup buffer from shadowing the Tier-1 reopen of the crashed `<cwd>/Untitled.md`, which would lose its unsaved edits. In session a live buffer's own swap already collides by name, so only a dead nvim's leftovers add anything.

## Limitations

- `:bunload`, the ranged or counted forms of `:bd` and `:bw` (`:3bd`, `:%bd`) and a scripted `bdelete` reach Neovim's own commands, which close the tab's last pane beside the drawer.
- The tabpage lockout is best-effort: obscure typed variants, user abbreviations of the `:tab*` commands and scripted `nvim_*` tabpage calls are not caught.
- The open-file mirror reports one global file set, attached to every tabpage triple, and only `active_path` is per-tabpage. A restart of a Neovim with several tabpages therefore reopens every file in every tabpage, each with the correct focused file.
- Crash recovery of unsaved edits is best-effort. A SIGKILL or OOM skips Neovim's `preserve_exit`, so only edits synced to the swap before the kill are recoverable.
- A file whose full or resolved path is longer than `SWAP_NAME_MAX` has no swap, so its unsaved edits are not crash-recoverable and a startup `Untitled.md` there is not claimed against a sibling Neovim. A stale swap under a shared cut name is kept, so a forcing Save As onto a sibling with that cut name fails with E768 until the swap is removed by hand.
- Nothing reaps a stale swap file, because a wrong staleness predicate would silently delete the only copy of someone's unsaved edits; the one delete is the `SwapExists` recover branch's, after the contents are in a buffer. Repeated crashes in one project therefore accumulate stale swaps, so a later launch may name its startup buffer `Untitled 3.md`, until the recovered buffers are opened and consume their swaps.
- Fenced-code injection depends on the two vendored trees resolving. A host with neither the bundle's siblings nor the devShell's exports still gets base markdown highlighting from the pinned Neovim, but no injections, and every fence outside the seven parsers Neovim ships renders plain in the preview.
- `nvim-treesitter-textobjects` ships bundled and inert: it is on `runtimepath` with no setup and no keymaps. It deliberately has no environment fallback: nothing off the bundle produces such a tree, and, the tree being inert, no test could tell a fallback from its absence.
- A Markdown LSP and a formatter are not wired.
- `:checktime` on focus regain depends on the front-end reporting focus through `WolfCore::set_ui_focus`. The macOS front-end reports each tab's; the GNOME one does not, so there the reload is driven by buffer entry and idle.
- `:saveas` of a buffer that has a swap, onto a name where a crashed session left a swap, shows that swap's `ATTENTION` text as a toast. It is never a prompt ([why](../../notes/runtime/nvim/init.lua.md#dead-owner-re-arm)).
- When two tabs hold unsaved edits to one file and both Neovims die, the relaunch never offers the second tab's `.swo`, since the dead-owner re-arm checks only the `.swp` name ([why](../../notes/runtime/nvim/init.lua.md#dead-owner-re-arm)).
