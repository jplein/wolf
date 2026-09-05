# runtime/nvim

`init.lua` is the captive Neovim configuration wolf-core launches. It is version-controlled next to the code that depends on it, so editor behavior is pinned to the core.

[`nvim.rs::spawn`](../../crates/wolf-core/src/nvim.rs) starts it as:

```
nvim --embed --clean -u <init.lua> -i NONE
```

with `NVIM_APPNAME=wolf`, the `XDG_*` dirs pinned under a private temp dir, `WOLF_SWAP_DIR` and `WOLF_SPELL_DIR` always exported, and `WOLF_RUNTIME_USER_DIR` / `WOLF_PARSERS_USER_DIR` exported when the platform state dir resolves (see [`state.rs`](../../crates/wolf-core/src/README.md#staters) for the resolvers).

Which copy is loaded depends on the tree. The macOS app resolves a `WOLF_INIT_LUA` override first, then the bundled `Contents/Resources/init.lua` via `Bundle.main.path(forResource:ofType:)`, then the in-repo `runtime/nvim/init.lua` (see [`Project.swift`](../../crates/wolf-macos/Sources/WolfMacOS/Project.swift)'s `initLuaPath`). Bazel tests pass the staged copy through `WOLF_TEST_INIT_LUA`.

`init.lua` self-locates its own directory and conditionally loads sibling trees — `orlando/`, `lighthouse/`, `nvim-treesitter/`, `parsers/`, `nvim-treesitter-textobjects/` — which the `.app` bundle places alongside it under `Contents/Resources/`. The orlando and lighthouse blocks are strict no-ops wherever their sibling is absent.

The two treesitter trees resolve sibling first and environment second. With no sibling, the nvim-treesitter tree comes from `WOLF_NVIM_TREESITTER_DIR` and the compiled grammars from `WOLF_PARSERS_DIR`, both exported by the Linux devShell — see [`build.md`](../../build.md), which owns the environment. Sibling first is what keeps a bundle self-contained: a developer with those variables exported who launches the `.app` must not have its Mach-O parsers silently replaced by the Linux-built tree, and a `dlopen` failure inside a `pcall` would be the only symptom. A resolved path that has since vanished falls through to nothing rather than being prepended.

Everything wolf-core reaches is a global on `_G`. Channel ids and arguments are passed as Lua varargs to `nvim_exec_lua`, never interpolated into the code string.

## The `_G.wolf_*` contract

### Setup helpers

wolf-core invokes these once after `ui_attach`, in the order listed by [`api.rs`](../../crates/wolf-core/src/api.rs): current-file, buffers, full-width, layout, dir, theme, commands, tabs, open-files, fs-events, view, preview. Each takes core's own RPC channel id, creates a `clear = true` augroup so re-invoking is idempotent, and emits once immediately so startup state is reflected without an event.

None of the notifications below is a redraw frame, so none bumps the snapshot `seq`. Most wake the front-end; `wolf_cwd` and `wolf_fs_changed` do not — the first rides the redraw flush its `:cd` already produces, and the second wakes only the file-search sessions whose rescan swaps. [`redraw.rs`](../../crates/wolf-core/src/redraw.rs) holds the decoding rules, including the defensive tolerance for a msgpack-`Ext`-wrapped handle where a plain integer is expected.

#### `wolf_setup_current_file_tracking(chan)`

Tracks the active buffer. A `BufEnter`/`BufWinEnter`/`BufFilePost`/`FileType` autocmd notifies **`wolf_current_file`** with four positional args: `expand('%:p')` (empty for an unnamed buffer, which core maps to no current file), `vim.bo.filetype`, `is_preview`, and `is_full_width`. Core folds them into `Snapshot.current_file`, `current_filetype`, `current_is_preview`, and `current_is_full_width`, which gate the Preview and Edit commands and the full-width menu checkmark.

A preview scratch pane (carrying `b:wolf_preview_source`) reports `("", "", true, <source buffer's b:wolf_full_width>)`, so core never treats the `Preview: <name>` scratch name as a real file, and the checkmark tracks the document rather than the scratch. `FileType` is in the event list so the filetype follows the active buffer even when detection lands after the buffer switch. Exposes `_G.wolf_emit_current_file`.

#### `wolf_setup_buffer_tracking(chan)`

Reports what every window of the current tabpage shows, so the front-end can draw a per-pane statusbar and core can derive a per-pane navigation-history target. Notifies **`wolf_buffers`** with one list of five-element tuples `{ win, nvim_buf_get_name(buf), buf, is_terminal, is_drawer }`, on `BufEnter`/`BufWinEnter`/`BufFilePost`/`WinEnter`/`WinNew`/`WinClosed`/`TermOpen`. Core wholesale-replaces both `Snapshot.buffers` (path only, unnamed windows omitted) and `Snapshot.win_buffers` (every window, `path = nil` when unnamed), so closed windows drop out.

`is_drawer` is the buffer tag `b:wolf_terminal` or'd with the window tag `w:wolf_terminal`. It marks the terminal drawer, which navigation history excludes; a `:terminal` in an ordinary split sets neither tag and is not excluded. Windows showing a preview scratch are omitted from the list entirely, which is what keeps `win_buffers` and `previews` disjoint — the history fold relies on that. Emits are coalesced (see Decisions). Exposes `_G.wolf_emit_buffers`.

#### `wolf_setup_full_width(chan)`

Notifies **`wolf_full_width`** with one list of `{ win, vim.b[buf].wolf_full_width == true }` pairs covering every window, on `BufEnter`/`BufWinEnter`/`WinEnter`/`WinNew`/`WinClosed`. Core wholesale-replaces `Snapshot.full_width`; a window absent from the map defaults to false. The front-end threads each pane's content-width mode — full versus the capped default — into its geometry, keyed by window handle.

Unlike buffer tracking, preview panes are not skipped. A preview pane's own flag is unused, because the front-end frames a preview from the source buffer's flag carried by `wolf_preview`. Exposes `_G.wolf_emit_full_width`.

#### `wolf_setup_layout(chan)`

Rebinds Neovim's window chords and the `:split`/`:close`/`:quit` Ex-command families to report layout intents, because the shell owns pane layout. Notifies **`wolf_layout`** with one map argument carrying an `action` discriminator, parsed by [`layout.rs`](../../crates/wolf-core/src/layout.rs):

- `{ action = "split", orientation = "h"|"v", from, win }` — nvim performs the real split first, so the window, its grid, and `win_pos` exist; the intent tells the shell the orientation and both handles.
- `{ action = "focus_dir", dir = "left"|"right"|"up"|"down", from }` — a pure intent. nvim does not move focus.
- `{ action = "drawer_open", win }` — emitted by `wolf_toggle_terminal` through the exposed `_G.wolf_emit_drawer`.

It also notifies **`wolf_active_win`** with `nvim_get_current_win()` on `WinEnter`, and **`wolf_command`** with `close_window` when a quit reaches the last pane. Closing a pane emits no intent: the shell observes the `win_close` redraw event. Exposes `_G.wolf_do_split`, `wolf_do_close`, `wolf_do_quit`, `wolf_do_focus`, and `wolf_emit_drawer`, which the `cnoreabbrev` overrides invoke from the command line.

Bindings: normal-mode `<C-w>` with `s`/`v`/`c`/`q` and `h`/`j`/`k`/`l` plus arrows; terminal-mode `<C-w>` with `h`/`j`/`k`/`l`; `cnoreabbrev` for `split`, `sp`, `new`, `vsplit`, `vs`, `vnew`, `close`; and the quit families routed through the internal `:WolfQuit`, `:WolfWriteQuit`, `:WolfQuitAll`, and `:WolfWriteQuitAll` user commands.

#### `wolf_setup_dir_tracking(chan)`

Notifies **`wolf_cwd`** with `getcwd()` on `DirChanged` (pattern `*`, so `:cd`, `:lcd`, and `:tcd` all fire it). Core keeps the value in a sibling cell rather than the render snapshot and uses it to scope the file MRU by the live working directory.

The one-shot startup emit carries a distinguishing second argument `true`; `DirChanged` emits carry no second argument. Core tracks the flagged launch directory but does not record it in the recent-directories MRU — a passive launch cwd is not a directory the user opened.

#### `wolf_setup_theme_tracking(chan)`

Notifies **`wolf_theme`** with four flat integers — `cursor_fg`, `cursor_bg`, `titlebar_fg`, `titlebar_bg` — read from the optional `WolfCursor` and `WolfTitlebar` highlight groups, on `ColorScheme`. That event also covers `:set background=…`, which re-sources the active scheme. Each value is a packed `0xRRGGBB` color or `-1` for an unset channel; core accepts only the 24-bit range and maps everything else to unset.

Groups are read with `link = false`, so a linked group resolves to its target's effective attributes. Neither group is ever painted on the grid, so this notification is core's only way to learn them: `WolfCursor` colors the app cursor, `WolfTitlebar` the macOS titlebar.

#### `wolf_setup_commands(chan, commands)`

Registers both user-facing faces of every Wolf command. `commands` is a list of `{ id, ex_command }` pairs core builds from `COMMAND_SPECS` in [`command.rs`](../../crates/wolf-core/src/command.rs), for example `{"close_window", "WolfCloseWindow"}`.

For each pair the helper seeds `package.loaded["wolf"][id]` and creates a `:<ex_command>` user command (`nargs = 0`, `force = true`). Both faces notify **`wolf_command`** with the canonical id, which core resolves through `Command::from_id` onto a drainable queue; an unknown id is dropped. Commands are user-triggered only: no autocmd, no emit on attach.

#### `wolf_setup_tabs(chan)`

Notifies **`wolf_tabpage`** with `nvim_get_current_tabpage()` on `TabEnter`/`TabNewEntered`, and **`wolf_tab_cwds`** with a list of `{ tabpage_handle, getcwd(-1, n) }` pairs covering every tabpage on `DirChanged`/`TabNewEntered`/`TabClosed`. `getcwd(-1, n)` is the tab-local directory, falling back to the global one for a tab never `:tcd`'d.

Core wholesale-replaces `Snapshot.tab_cwds`, which makes `cwd_for_tabpage` a cheap snapshot read authoritative even for a non-current view, and prunes closed tabs — `TabClosed` cannot report the closed handle, so replacement is the only way they leave the map. The helper also installs the tabpage lockout described under Decisions.

#### `wolf_setup_open_files(chan)`

Backs the crash-restart file restore. Notifies **`wolf_open_files`** with a list of `{ tabpage_handle, active_path, files }` triples, one per tabpage, on `BufEnter`/`BufWinEnter`/`BufWritePost`/`BufFilePost`/`BufDelete`/`WinEnter`/`TabEnter`/`TabNewEntered`/`TabClosed`. Core wholesale-replaces `Snapshot.open_files`, which the front-end reads before a restart and replays after.

`files` is the whole instance's file-backed buflisted buffer set, with preview scratch and unnamed buffers excluded. `active_path` is the tabpage's active window's buffer name, empty for an unnamed buffer. See Limitations for what the global file set costs.

#### `wolf_setup_fs_events(chan)`

Feeds the file-search rescan. Notifies **`wolf_fs_changed`** with `{ event_name, abs_path }` from two trigger classes in one augroup: the buffer events `BufWritePost`/`BufFilePost`/`BufDelete` (file buffers only) carry the changed file's absolute path, while the refocus-edge events `ShellCmdPost`/`FocusGained`/`TermLeave` carry an explicit `""` — they name no file, and the empty element must still be present or core's pair decode drops the event. Core routes both into the rescan of every live file-search session (see [`files.rs`](../../crates/wolf-core/src/README.md#filesrs)); only `BufWritePost` is conditional — rescan only for a path not already listed — and every other event forces one.

It is a dedicated event rather than a reuse of `wolf_open_files` because that augroup also fires on `BufEnter`/`WinEnter`/`TabEnter`, which would turn every focus change into a rescan. The refocus-edge autocmd has its own callback because the buffer-event callback's buftype guard would swallow `TermLeave`, whose current buffer is the terminal; that guard is `pcall`-guarded and fails open, since `args.buf` can already be invalid on `BufDelete` and an extra rescan is harmless where a missed one is the bug. `TermLeave` rather than `WinLeave`: the drawer auto-`startinsert`s on `WinEnter`, so every drawer-exit path passes a terminal-to-normal mode change and the one unguarded event covers them all. There is no initial emit: with no event there is nothing to rescan.

`FocusGained` depends on the front-end forwarding OS focus — the same asymmetry as `:checktime` under Limitations — so on GNOME, external changes wait for another trigger.

#### `wolf_setup_view_tracking()`

Takes no channel id and emits nothing, so no scroll or cursor payload ever crosses the FFI boundary. It maintains an in-nvim `view_cache[win][buf] = winsaveview()`, filled on `BufLeave`/`WinLeave` and pruned per window on `WinClosed`, and defines `_G.wolf_switch_buffer` as a closure over that cache.

#### `wolf_setup_preview(chan)`

Registers three things in one augroup. A window scan on `WinEnter`/`BufWinEnter`/`WinNew`/`WinClosed`/`BufWinLeave` notifies **`wolf_preview`** with a list of `{ preview_win, source_bufnr, source_abs_path, source_full_width }` tuples, one per window showing a `b:wolf_preview_source` scratch buffer; core wholesale-replaces `Snapshot.previews`. `source_bufnr` is `-1` when the source is unknown, and `source_full_width` is the source buffer's flag, which decides whether the preview renders capped or full-width.

**A window whose `b:wolf_preview_source` is present but empty is filtered out of that list**, which is what makes a preview of a genuinely nameless buffer unreachable from either front-end: the pane never enters `Snapshot.previews`. An unsaved buffer is not that case — [`wolf_new_untitled`](#wolf_new_untitled) gives it a real `Untitled N.md` name first.

Markdown-guarded `TextChanged`/`TextChangedI`/`BufWritePost` autocmds notify **`wolf_markdown_dirty`** with `{ source_bufnr, changedtick }`. Core upserts it into `Snapshot.markdown_ticks`; the front-end joins a preview's source bufnr to its tick to decide when to re-pull rendered HTML. `init.lua` never renders Markdown — `wolf_core::markdown` does.

`CursorMoved`/`CursorMovedI`/`BufEnter`/`BufWinEnter` autocmds notify **`wolf_markdown_cursor`** with three flat scalars `(source_bufnr, line, char_col)` — a 1-based `line('.')` and a 0-based `charcol('.') - 1`. That column is a character index rather than a byte index, which is what lets it pair with the `wl-{L}-{C}` markers with no conversion anywhere downstream; core upserts it into `Snapshot.markdown_cursors`. Five guards decide whether to emit: the buffer must be valid, must be current, its window must be a normal window, its filetype must be `markdown`, and `b:wolf_preview_source` must be nil.

Exposes `_G.wolf_emit_previews` and `_G.wolf_emit_markdown_cursor`.

#### `wolf_emit_current_file`, `wolf_emit_buffers`, `wolf_emit_full_width`, `wolf_emit_previews`, `wolf_emit_markdown_cursor`, `wolf_emit_drawer`

Each is a closure exposed by its setup helper, over that helper's channel, so it exists only after that setup has run. Every caller guards with `if _G.wolf_emit_… then`.

They exist to re-emit a notification when state changes without firing the autocmd that would otherwise report it: writing a buffer variable fires no event at all, and a bind into a non-current window fires `BufEnter`/`BufWinEnter` with the target window temporarily current, which leaves the current-file emit describing the background pane.

### On-demand helpers

wolf-core drives these over `nvim_exec_lua`. They take no channel id.

#### `wolf_highlight_code_blocks(blocks)`

Highlights the Markdown preview's fenced and indented code blocks. Takes one array of `{ text, lang }` pairs (`lang` is `""` when the fence declares none) and returns an array of the same length, so a whole batch costs one round-trip.

`results[i]` is either the scalar `false` — unknown language, missing parser or `highlights` query, parse failure, or no resolved spans — or an array of 7-element span tuples `{ row, start_col, end_col, fg, bold, italic, underline }`. A span is a single-line, non-overlapping run: `row` is 0-based within the block text, `start_col` and `end_col` are 0-based half-open byte columns, `fg` is a packed `0xRRGGBB` integer or `-1`, and the three attributes are booleans. Capture names resolve through `@`-prefixed groups with dotted-prefix fallback, and later or higher-priority captures win a byte, matching nvim's precedence.

#### `wolf_create_tabpage(cwd)`

Runs `tabnew`, roots the new tabpage at `cwd` via `:tcd` when `cwd` is non-empty, and returns the tabpage's stable handle. An empty or nil `cwd` leaves the tab at the global working directory, which is the empty-state view.

#### `wolf_rebuild_layout(spec_json)`

Realizes a saved pane-split tree in the current tabpage's single starting window and returns the resulting window handles in depth-first leaf order — the same order as the front-end's `PaneTree.leaves()`, so each handle zips to its rebuilt leaf. A malformed spec returns an empty list.

`spec_json` is a JSON string: a leaf is `{ leaf = true, path, preview }`, a split is `{ orientation = "h"|"v", weights, children }`. Two renderers produce it independently — `wolf_core::state::rebuild_layout_spec` and the Swift `PaneLayoutCodec.specJSON` — so a change here has to land on both; see [wolf-core's DECISIONS.md](../../crates/wolf-core/DECISIONS.md).

A preview leaf with a path becomes a preview of that source via `wolf_preview_file`, converting the window rather than replacing it so the handle survives; a plain leaf with a path is `:edit`ed; an empty leaf keeps its inherited buffer. Regions are created child-0-first, left-to-right and top-to-bottom. nvim supplies only the window count, topology, and per-window buffer; the front-end drives pane sizes from its own weights.

#### `wolf_close_tabpage(handle)`

Resolves the stable handle to its current tabpage number and runs `<n>tabclose` under `pcall`. Tabpage numbers renumber as tabs open and close, which is why the handle cannot be used directly. A stale handle, or the last-tabpage error, degrades to a no-op — the front-end never closes the last view, it shuts the whole nvim down instead.

#### `wolf_tab_cwd(handle)`

Returns the tab-local working directory of the tabpage named by its stable handle, or `""` for a stale handle. No core caller: `cwd_for_tabpage` reads the `wolf_tab_cwds` snapshot map instead, because a per-tab blocking RPC turned a wedged nvim into a sequence of timeouts.

#### `wolf_switch_buffer(win, buf)`

Binds `buf` into `win` with a window-scoped `nvim_win_set_buf` — never `:edit`, so there is no disk re-read and no MRU churn — then restores that pane's saved view for that buffer, forcing the top of the buffer when the pane has never shown it. Keying on `(win, buf)` is what stops a pane borrowing another pane's scroll of a shared buffer.

The body is `pcall`-wrapped and the helper returns that `ok` flag, so a bind that throws after the caller's validity checks (an open cmdwin, a `'winfixbuf'` window, an unabandonable modified buffer) is reportable rather than silent.

#### `wolf_history_set_buf(win, bufnr)`

Binds `bufnr` into `win` through `wolf_switch_buffer` and returns `bufnr`, or `0` when either handle is nil, zero, or invalid, or when the bind itself failed. Core turns a `0` into a history-cursor revert, so a target that has been wiped never silently drifts the pane.

On success it calls `_G.wolf_emit_previews` and `_G.wolf_emit_current_file`. The current-file re-emit is load-bearing; the previews re-emit is hardening (see Decisions).

#### `wolf_history_open(win, path)`

Binds the buffer for absolute `path` into `win`, loading it if needed, and delegates the bind, view restore, and re-emits to `wolf_history_set_buf`. Returns the bufnr or `0`.

It first scans `nvim_list_bufs()` for an ordinary file buffer (`buftype == ""`) whose name equals or `resolve()`s to `path`, and only falls back to `bufadd` plus `bufload`. Rebinding the existing buffer is what makes navigating back to an unsaved `Untitled N.md` restore that modified buffer rather than reading a file never written to disk: core canonicalizes `path`, so under a symlinked project root it can differ from the name nvim kept, and a never-written buffer has no file id for nvim's own matching to use.

#### `wolf_toggle_terminal()`

Toggles the terminal drawer. The terminal buffer is a single shared buffer per nvim instance, tagged `b:wolf_terminal` and required to have `buftype == "terminal"`; the drawer window is per-tabpage. If a window in the current tabpage shows that buffer, it closes the window only, so the buffer and its shell job survive.

Otherwise it opens a `botright` split roughly one third of the screen tall, tags the window `w:wolf_terminal`, binds the shared buffer or creates a fresh `:terminal` when the user has exited the shell, seeds `b:wolf_full_width = true` on whichever buffer landed in the drawer, and `startinsert`s. The whole build runs inside `wolf_suppress_buffers`. Afterwards it calls `_G.wolf_emit_drawer` (the intent that makes the drawer visible), then `_G.wolf_emit_current_file` and `_G.wolf_emit_full_width`, all guarded.

#### `wolf_toggle_full_width()`

Flips `b:wolf_full_width` on the effective buffer, then re-emits current-file, per-window full-width, and previews, each guarded. The effective buffer is the current one, unless the current pane is a preview scratch with a valid `b:wolf_preview_source_bufnr`, in which case the source buffer is targeted so a document's editor and preview views stay consistent. Writing a buffer variable fires no autocmd, so the three re-emits are the only way the front-end learns to resize.

#### `wolf_suppress_buffers(fn)`

Runs `fn` with `wolf_buffers` emits suppressed, restores the previous value of `_G.wolf_buffers_suppressed`, re-emits once from the outermost call so core sees the settled state, and only then re-raises any error `fn` raised. Nests safely.

The flag is cleared even when `fn` errors, which is what keeps buffer tracking alive for the rest of the session. Set it through this helper rather than directly: a flag left set kills per-window buffer tracking until the process exits.

#### `wolf_ensure_preview_buf(source_buf, source_path)`

Creates or reuses the preview scratch buffer for `source_buf` and returns its bufnr, memoized in `_G.wolf_preview_bufs` keyed by source buffer number. The buffer is an unlisted scratch, is named `Preview: <cwd-relative path>`, is `nomodifiable`, and carries `b:wolf_preview_source` (the source's absolute path) and `b:wolf_preview_source_bufnr`. Those two variables are what the path-based trackers skip on and what the preview window scan reads.

Unlisted is deliberate: the buffer picker lists one row per file — the source buffers — so the scratch contributes no `Preview: ` row of its own, and its name exists only for window-title display.

Naming falls back to a bufnr-disambiguated name when two sources share a cwd-relative path, so the second preview still opens.

#### `wolf_open_preview()`

Previews the active source buffer in place — the active pane's buffer is replaced with the preview scratch — and returns the preview buffer's number, or `0` when the active buffer's filetype is not `markdown`, the same gate as the Preview command.

It re-emits the source buffer's cursor before the bind, while the source is still current: after the bind the cursor emit's preview-scratch guard would bail, leaving a freshly opened preview with no anchor. Ends with a guarded `_G.wolf_emit_previews` and `_G.wolf_emit_current_file`.

#### `wolf_edit_source()`

The inverse of `wolf_open_preview`: replaces the active pane's preview with its source file in place and returns the source bufnr, or `0` unless the current buffer is a preview scratch.

Binding prefers `nvim_win_set_buf` with the recorded source bufnr — window-scoped, no disk re-read — and falls back to `:edit`ing the recorded path when that bufnr is stale. No MRU is recorded. Ends with the same two guarded re-emits.

#### `wolf_preview_buffer(win, bufnr)`

Previews an already-open buffer: binds `bufnr`'s preview scratch into window `win`, falling back to the current window when `win` is missing, zero, or invalid, and returns the source bufnr, or `0` when `bufnr` is invalid, unnamed, or not Markdown. On `0` the front-end falls back to a plain `switch_buffer`, so a non-markdown pick converts the pane to edit mode.

The gate honors an already-set filetype and otherwise asks `vim.filetype.match`, setting the filetype on a match. It `bufload`s the buffer first, because a `:badd`-ed buffer can be buflisted yet unloaded and the render pull needs its lines. Ends with the same two guarded re-emits.

#### `wolf_preview_file(win, path)`

The disk-path front half of `wolf_preview_buffer`: previews the Markdown file `path` in window `win`, where `path` is already absolute — resolved by the front-end from a clicked `file://` link or a picked file. This helper does no relative-path resolution. Returns the source bufnr, or `0` when `path` is empty, unreadable, or not Markdown — on `0` the front-end leaves the pane alone or applies its own fallback.

It `bufadd`s and `bufload`s the source, then delegates the gate, bind, and re-emits to `wolf_preview_buffer`. Only on success does it mark the source buflisted: a `bufadd`-ed background buffer is unlisted, and listing it is what gives a preview-only file its buffer-picker row, while a rejected non-markdown path must not gain one. It deliberately does not re-emit the cursor: the source is loaded in the background with no window, so it carries no meaningful position.

#### `wolf_preview_in_other(target_win)`

Previews the active pane's Markdown file in another pane and returns the source bufnr — not the scratch, since the front-end joins `previews()` rows on the source — or `0` unless the active filetype is `markdown`. The gate runs before any split, so a mis-timed palette activation is a pure no-op that never changes the layout. `target_win` is the front-end-resolved window, `0` when the front-end wants a split.

A usable target — non-nil, non-zero, a valid window, not the current window, and not the terminal drawer (`b:wolf_terminal` / `w:wolf_terminal`) — receives the scratch via `nvim_win_set_buf` with focus untouched. Anything else takes the split path: `wolf_do_split("v")` (the shell's applySplit inserts the new pane after `from`, so it lands to the right), the scratch bound into the new window, focus handed back to `from`. Focus follows the edit-mode view of the document, so the source editor stays focused either way. The cursor pre-emit, the two guarded re-emits, and the no-MRU rule mirror `wolf_open_preview`.

#### `wolf_edit_in_other(target_win)`

Edits the active preview pane's source in another pane and returns `nvim_win_get_buf` of the bound window, or `0` unless the current buffer carries `b:wolf_preview_source` — gated before any split, like its sibling. Returning the bound window's buffer rather than the recorded bufnr is deliberate: the stale-bufnr `:edit` fallback still reports non-zero, a divergence from `wolf_edit_source`.

Target validation is shared with `wolf_preview_in_other`, and an unusable target falls back to the split path. A usable target has the source bound into it — preferring the window-scoped bufnr bind, falling back to focusing plus `:edit` of the recorded path — and is then focused. On the split path the new right window keeps the scratch and the source is bound into the original window, which is focused: net editor-left, preview-right. Focus follows the edit-mode view of the document. No MRU is recorded; the two guarded re-emits mirror `wolf_edit_source`.

#### `wolf_next_untitled(cwd, exclude_buf, extra_collision)`

Pure name generator. Returns `(name, abspath)` for the first index whose name collides with neither a file or directory on disk in `cwd` nor any existing buffer's name, skipping `exclude_buf`. Index 1 is un-numbered `Untitled.md`, so the literal `Untitled 1.md` is never generated.

Buffer collisions consider every valid buffer, listed or not and loaded or not, because any of them can raise E95. The optional `extra_collision(abspath)` predicate adds a collision source; every naming call site passes [`wolf_untitled_swap_exists`](#wolf_untitled_swap_existsabspath). It stays a parameter rather than a hard-wired call so the generator itself remains pure. A defensive iteration bound falls back to an `hrtime`-suffixed name.

#### `wolf_name_untitled(buf, extra_collision)`

Names `buf` with the first free Untitled name in `getcwd()` and sets its filetype to `markdown`, which is what makes the in-app preview gate pass. It never writes to disk and never touches `buftype`, `bufhidden`, or `swapfile`, so the buffer stays a normal, modifiable, unsaved document until the user saves it.

`nvim_buf_set_name` is retried on an E95 race, since the colliding buffer is picked up by the next collision walk; after a few attempts it leaves the buffer unnamed rather than raising. That retry covers a name race only — the swap dialog described under Decisions is not an error, so no `pcall` around this call can catch it. Naming fires `BufFilePost` and `FileType`, so the existing trackers re-emit the new name with no extra wiring.

#### `wolf_name_untitled_current(extra_collision)`

Guarded re-home of the current buffer into the working directory. It proceeds only when the buffer is normal (`buftype == ""`), unmodified, not a preview scratch, and either unnamed or carrying an existing `Untitled.md` / `Untitled N.md` name that is not yet on disk — so a real saved file is never renamed. Core's `Chdir` arm calls it after a `:tcd`, and the `WolfUntitled` `VimEnter` autocmd calls it at startup when exactly one buffer is listed.

#### `wolf_new_untitled()`

Runs `:enew` and names the resulting buffer. The previous buffer stays listed if it carried a real name, so repeated calls walk `Untitled.md`, `Untitled 2.md`, `Untitled 3.md`.

#### `wolf_untitled_swap_exists(abspath)`

Whether a Wolf-owned swap file already exists for `abspath`, meaning a crashed nvim left unsaved edits for that exact path. Every naming call site passes it as the `extra_collision` predicate, so no naming path can walk into the dialog described under Decisions.

Two criteria, either sufficient. The first stats the `%`-mangled name nvim's own `findswapname` derives from a buffer's full path — the trailing `//` in `'directory'` is what selects that mangling — which is precisely the file the dialog fires on. The second scans `WOLF_SWAP_DIR` and compares each swap's recorded owner path, which is broader: a `.swo` leftover still holds recoverable edits without raising a dialog, and it is the fallback if the mangling ever drifts from nvim's.

**The owner-path comparison normalizes the stored value forward, never the query backward.** nvim writes that path `home_replace`d, so a file under `$HOME` is stored as `~<user>/…` and a plain equality against an absolute path silently never matches — which is every real user project. `fnamemodify(fname, ":p")` expands it and is the identity for a path that is already absolute. Not `expand()`, which also performs wildcard expansion and would mangle a name containing one; not `fnamemodify(abspath, ":~")`, which yields `~/…` rather than the `~<user>/…` form nvim stores.

## Decisions

**Swap files are on, and `-n` is deliberately absent from the launch line.** `-n` forces `updatecount = 0`, which disables swap writing entirely and would make crash recovery of unsaved edits impossible. Swaps live in `WOLF_SWAP_DIR`, a durable Wolf-owned per-project directory that is stable across respawns, so a fresh nvim can recover a crashed one's edits. The trailing `//` on `'directory'` makes nvim encode the full path into each swap name, so identical basenames in different directories never collide, and it drops the default leading `.` entry that would otherwise drop a `.swp` next to the user's file.

**The `SwapExists` autocmd must never surface the blocking E325 prompt**, so it decides from the swap's owner instead. A stale or unreadable swap, or an owner pid that `uv.kill(pid, 0)` cannot find, is dead and sets `v:swapchoice = "r"` to recover silently, then schedules a delete of the consumed swap. A live owner, or a swap written on another host whose pid cannot be probed, sets `"e"` and edits the clean on-disk copy. It fires only on the file-load path, which is where nvim arms the choice machinery; the naming path is defended separately, below.

**`autoread` alone only re-reads on some triggers**, so the `WolfAutoRead` augroup drives `:checktime` on focus regain, buffer entry, and idle. `:checktime` re-reads only an unmodified buffer, so unsaved edits are never clobbered, and it is a no-op on the terminal and preview scratch buffers; the callback is `pcall`ed and `silent!`ed so a special buffer can neither raise nor pollute the grid.

**The grid is deterministic and chrome-free** — `number`, `signcolumn`, `ruler`, `showcmd`, and `laststatus` are all off, the intro message is suppressed, and `termguicolors` is on so `hl_attr_define` carries RGB the front-ends paint directly. The front-end draws its own chrome.

**`showtabline` is 0 because the default of 1 draws a tabline the instant a second tabpage exists.** Neither front-end paints grid 1, so that line is invisible yet still costs the pane grid a row — measured as 80x24 dropping to 80x23 — and leaves every `WinInfo` `start_row` stale until the front-end's own resize wins the row back. Both front-ends draw their own tab chrome.

**`wildoptions` gains `pum`** so command-line completion presents as the native vertical `popupmenu_show` popup rather than the horizontal wildmenu. Wolf attaches with `ext_popupmenu` and `ext_messages`, and the latter implicitly enables `ext_cmdline` and forces `cmdheight = 0`, so the popup must anchor above a command line the grid does not draw.

**Soft wrap is on with `linebreak` and `smoothscroll`.** Wolf edits Markdown, where prose lines routinely exceed the pane width; `linebreak` wraps at spaces and punctuation instead of mid-word, and `smoothscroll` scrolls by screen line so scrolling never skips a wrapped fragment of a long line.

**The colorscheme activates after `syntax on`.** Both clear and redefine highlight groups, so whichever runs last wins, and `vim.o.background` is set before `:colorscheme` so a dual-variant scheme reads it. The hardcoded dark value is only the pre-push fallback for the initial paint: the front-end pushes the resolved appearance via `:set background=…` at launch, and live when the OS toggles in `auto` mode.

**A colorscheme on `runtimepath` does nothing on its own**, unlike a plugin, so the lighthouse block must both prepend the directory and run `:colorscheme`. The orlando block sources its plugin file explicitly so its definitions exist during init, deterministically, rather than waiting for the post-init plugin phase — measured on the pinned nvim, `--clean` keeps `'loadplugins'` on, so an rtp-prepended tree's `plugin/*.lua` files do load after init finishes.

**The vendored nvim-treesitter is a query provider and, when `WOLF_PARSERS_USER_DIR` resolves, an activated plugin.** The gate prepends its tree (queries ahead of Neovim's own) and the vendored grammars; the parsers_user block then sources `plugin/nvim-treesitter.lua` (registering `:TSInstall`), passes the durable dir to `setup({ install_dir = … })`, and prepends it to `runtimepath` ahead of the bundled parsers, so a user-compiled parser shadows the bundled one for the same language. `markdown` and `markdown_inline` ship with nvim and are not re-vendored.

`setup()` is a config merge that itself eagerly prepends `install_dir` by raw string concat; the explicit option-list prepend after it dedupes, landing the entry once and at the front whether or not the `pcall`ed setup succeeded. `:TSInstall` actually compiling a parser depends on the user's own toolchain — Wolf guarantees only a durable install target that survives restarts.

**Spell checking is prose-only, from a `WolfSpell` FileType autocmd registered before `syntax on`.** Measured on the pinned nvim, an autocmd registered after `syntax on` never fires for a markdown startup file argument. The pattern is `*` with an explicit off for non-prose filetypes — determinism hardening: it guarantees `'spell'` is off for a code file regardless of the window-global value, rather than relying on nvim's reset-from-global behavior on a buffer switch, which flips spell-on everywhere the moment anything sets `vim.o.spell`. `'spelllang'` is pinned to `en`, the only language whose `.spl` the pinned runtime ships — a missing `.spl` opens a blocking download prompt with nowhere to draw under `ext_messages`.

The durable dictionary is the global `'spellfile'` at `WOLF_SPELL_DIR/en.utf-8.add`; core creates that dir pre-launch, because nvim never creates an explicitly set spellfile's parent and `zg` would fail E484 silently. Paths assigned to `'directory'` and `'spellfile'` are comma-escaped — both options are comma-separated lists, and a raw comma silently splits the value.

**`init_user.lua` loads last, and its errors warn instead of failing startup.** The end-of-file `dofile` of `WOLF_RUNTIME_USER_DIR/init_user.lua` must stay the final statement in `init.lua`: loading last is the contract that makes user settings override every Wolf default. The `pcall` plus `vim.notify` warning naming `init_user.lua` turns a broken user file into a `:messages` entry rather than a dead editor. Core seeds the file once into an empty directory and never rewrites it — the seed contract lives with [`state.rs`](../../crates/wolf-core/src/README.md#staters).

**What the gate buys the editor is injected fenced-language highlighting, not markdown highlighting.** Measured on the pinned Neovim: a markdown buffer under `--clean -u NONE`, with no plugin and no gate, already reports `vim.treesitter.highlighter.active[buf] ~= nil`, so that expression cannot witness the gate and must not be used as evidence for it. The discriminating readback is an injected child langtree for the fenced language. The `FileType=markdown` autocmd's `vim.treesitter.start` is `pcall`-wrapped, so a query-loader incompatibility is swallowed at startup rather than raised.

**`wolf_highlight_code_blocks` is defined at module level, outside the conditional treesitter block**, so it runs on every platform and gates per language: it answers `false` only for a fence whose language has no parser and `highlights` query on the runtimepath. A fence in a language the pinned Neovim already ships therefore highlights with nothing vendored; what the vendored trees add is every other language.

**Two payloads dodge the same msgpack ambiguity: an empty Lua table cannot be told apart from an empty map.** `wolf_theme` therefore sends four flat scalars with a `-1` sentinel rather than a table, and `wolf_highlight_code_blocks` returns the scalar `false` rather than an empty array for a non-highlightable block.

**Split, close, and focus are deliberately asymmetric.** A split performs the real window operation and also reports an intent, so the shell learns the orientation and handles. A close performs a guarded `nvim_win_close` that refuses the last window and reports nothing, because the shell already observes `win_close`. A focus move performs nothing and is a pure intent, because the shell resolves the neighbour in its own layout.

**The `:q` family routes through internal user commands, not registry commands.** A write variant that fails aborts the quit, matching native `:wq`. A plain `:q` on the last pane with a modified buffer is refused with a native-style E37 echo; `:q!` overrides. The `:qa` family passes `all = true`, which skips the pane-count check so the whole window closes regardless of how many panes are open, with the modified guard still scoped to the current buffer.

**There are deliberately no `q!`, `wq!`, or `qa!` abbreviations.** `!` is not a keyword character, so such an abbreviation would be dead. Typing `:q!` fires the bare-`q` abbreviation and the `!` rides through to `:WolfQuit!`, whose callback reads `o.bang`. `a` is a keyword character, so `:qa` forms a distinct word that never collides with bare `q`.

**Tabpage navigation and creation chords map to `<Nop>`, and the typed `:tab*` commands expand to an empty command line, because Wolf owns all tabpage lifecycle.** A user-typed `:tabnew` would create an orphan tabpage no view is bound to, and `:tabnext` or `:tabclose` would switch or close a tabpage out from under a view, desyncing the front-end-authoritative current view. `:tcd`, `:lcd`, and `:cd` are left open on purpose: they are different words, so the whole-word abbreviations never catch them, and `:tcd` is how each view sets its own working directory.

**`wolf_buffers` emits are coalesced through `vim.schedule`, never synchronous.** Core's history fold pushes on every observed change, so an intermediate wrong emit persists even if a later emit is correct. A single `:terminal` fires `BufEnter` with a brand-new unnamed buffer, then `BufFilePost` with a `term://` name, and only sets `buftype` at `TermOpen` — three history entries for one user action if emitted eagerly.

**Coalescing is not enough for a multi-step window build, which is why `wolf_suppress_buffers` exists.** A `botright split` fires `WinNew` and `WinEnter` synchronously with the new window still showing the previous buffer, and the drawer's buffer is not tagged until after `:terminal` has fired `BufEnter`. Those emits must be suppressed rather than corrected afterwards.

**A background `nvim_win_set_buf` does fire `BufEnter` and `BufWinEnter`**, measured against the pinned nvim 0.12.3, with the target window and buffer temporarily current and focus restored before the call returns. That makes `wolf_emit_current_file` load-bearing after any background bind, because the autocmd-driven emit reads `nvim_get_current_buf()` and would leave core describing the background pane. `wolf_emit_previews` is hardening: its `nvim_list_wins()` sweep does not read the current window, so it is already correct — it is retained because it is idempotent and makes the update independent of autocmd ordering. The one bind that fires nothing is a no-op rebind of the buffer a window already shows, which changes nothing to emit.

**The cursor emit needs both buffer-entry events and the `win_gettype()` guard, and neither is redundant.** `:edit foo.md` fires `CursorMoved` before filetype detection, so the steady-state autocmds alone would leave an opened-but-untouched buffer reporting no cursor at all. Adding `BufEnter` and `BufWinEnter` fixes that but opens a hole: `vim.fn.bufload()` fires both inside nvim's synthetic autocmd window, where the target buffer is genuinely current and every other guard passes, over a fabricated line 1 that belongs to no user pane. Only a normal window carries a real cursor, so the guard is what preserves the `line == 0` unknown sentinel a background-loaded source must report.

**A preview pane is a real nvim scratch buffer — unlisted, with its source buflisted instead.** Carrying `b:wolf_preview_source` is what lets the path-based trackers skip it and what marks its window as a preview-mode pane. The buffer picker lists the union of edited and previewed files, one row per file: the row is always the source buffer, which `wolf_preview_file` marks buflisted on a successful preview, and the unlisted scratch contributes none of its own.

**The full-width flag lives per buffer and, from a preview, targets the source buffer.** Full width is a property of the document, so toggling from either view resizes both and the menu checkmark tracks the document rather than the scratch. The terminal drawer seeds the same flag to `true` as its default, and remains user-toggleable.

**Naming a buffer treats an existing swap for the candidate name as a collision, at every naming call site.** `nvim_buf_set_name` does not skip the swap check: the rename reaches `ml_setname`, which calls `ml_open_file` and creates the swap file right there, on a path where nvim has not armed the `SwapExists` choice machinery. A stale swap therefore goes straight to the blocking `ATTENTION` dialog, which is not an error, so no surrounding `pcall` catches it — it simply sits swallowing keystrokes, and the visible symptom is that the first character typed after a crash restart disappears.

Name avoidance is the only defence. Yielding `Untitled 2.md` also keeps a fresh startup buffer from shadowing the Tier-1 reopen of the crashed `<cwd>/Untitled.md`, which would otherwise lose its unsaved edits. In-session behaviour is unaffected, because a live `Untitled N.md` buffer owns its own swap and so already collides by name; only a dead nvim's leftovers add anything.

**Terminal-mode `<C-w>` chords shadow the shell's word-erase inside the drawer.** This is an accepted tradeoff for a keyboard-navigable drawer; `<C-w>k` is what bridges drawer to editor without first leaving terminal-insert. A `WolfTerminalInsert` autocmd re-enters terminal-insert when focus lands in the drawer, matched on the drawer's identity so it never yanks back from a deliberate `<C-\><C-n>`.

## Limitations

- The tabpage lockout is best-effort. Obscure typed variants, user-defined abbreviations of the `:tab*` commands, and scripted `nvim_*` tabpage calls are not caught, so a determined user can still create or close a tabpage out from under the front-end.
- The open-file mirror reports one global file set. `getbufinfo` is global to the nvim instance, so the same set is attached to every tabpage triple and only `active_path` is genuinely per-tabpage. A multi-view project restart therefore reopens every project file in every view, each with the correct focused file.
- Unnamed buffers are outside the open-file restore; only file-backed buffers are mirrored.
- Crash recovery of unsaved edits is best-effort. A SIGKILL or OOM skips Neovim's `preserve_exit`, so only edits synced to the swap before the kill are recoverable.
- Nothing reaps a stale swap file, because a reaper would be deleting the only copy of someone's unsaved edits and a wrong staleness predicate loses them silently. The one targeted delete is the `SwapExists` recover branch's, which runs after the contents are already in a buffer. Repeated crashes in one project therefore accumulate stale swaps, so a later launch there may name its startup buffer `Untitled 3.md` or `Untitled 4.md`. That is cosmetic and self-correcting once the recovered buffers are opened, which consumes their swaps.
- Fenced-code injection depends on the two vendored trees resolving. A host with neither the bundle's siblings nor the devShell's exports still gets base markdown highlighting from the pinned Neovim, but no injections, and every fence outside the seven parsers Neovim ships renders plain in the preview.
- `nvim-treesitter-textobjects` ships bundled and inert: it is on `runtimepath` with no setup and no keymaps. Unlike the other two trees it deliberately has no environment fallback — nothing off the bundle produces such a tree, so the variable would have no producer, and because the tree is inert no test could tell a fallback for it from its absence.
- A Markdown LSP and a formatter are not wired.
- `:checktime` on focus regain depends on the front-end forwarding OS focus through `nvim_ui_set_focus`. The macOS front-end does; the GNOME one does not, so there the reload is driven by buffer entry and idle.
