# `runtime/nvim/init.lua`

Last verified at commit `3a62e0a429331b259be6cf829d920fdd22556c56`.

## Spellfile is global

`'spellfile'` is local to the buffer, but its global value seeds every new buffer, verified on all three open paths: a startup file argument, `:edit`, and `:enew` followed by `set ft=markdown`. Moving the assignment into a FileType autocmd registered later in the file fails for a markdown startup file argument, for the same `syntax on` ordering reason as the `WolfSpell` autocmd. If a buffer-local value is ever needed, set it inside the existing `WolfSpell` callback rather than registering a new autocmd.

Left unset, nvim derives `'spellfile'` from the first writable runtimepath entry, which sits under the private `XDG_DATA_HOME` wolf-core pins for hermeticity, so a taught word would die with the process. nvim names the dictionary from `'spelllang'` and `'encoding'` (`<lang>.utf-8.add`), so a filename that disagrees with `SPELL_LANG` makes `zg` write a file nvim never reads back.

## Vendored tree resolution

The environment fallback is the only thing that fires the treesitter gate on Linux, where no sibling tree exists. Deleting it removes the vendored queries, the parsers and the `FileType=markdown` autocmd, which costs fenced-block injections but not base markdown highlighting.

Measured on the pinned Neovim, a markdown buffer under `--clean -u NONE`, with no plugin and no gate, already reports `vim.treesitter.highlighter.active[buf] ~= nil`. That expression therefore cannot witness the gate and must not be used as evidence for it; the discriminating readback is an injected child langtree for the fenced language.

The fallback works because `vim.env` is a view over the process environment, which `--clean` does not touch. The captive nvim inherits that environment: `nvim.rs` overrides only its `NVIM_APPNAME`, `XDG_*` and `WOLF_*` variables on the child and calls no `env_clear`, and `cage` clears nothing either. The textobjects tree is a bare sibling probe on purpose; see the Limitations in [the runtime README](../../../runtime/nvim/README.md#limitations).

## User parser activation

The explicit `vim.cmd.runtime("plugin/nvim-treesitter.lua")` is what registers `:TSInstall`, `:TSUpdate`, `:TSUninstall` and `:TSLog`; `setup()` does not. `--clean` keeps `'loadplugins'` on, so the startup plugin phase would also load it after this file, but the explicit call makes registration deterministic, and the plugin's `vim.g.loaded_nvim_treesitter` guard and lazy requires make the later load a cheap no-op.

Only that plugin file is sourced: the sibling `query_predicates.lua` registers predicates that no bundled language uses. The `pcall` around `setup()` degrades a version incompatibility to a missing `:TSInstall` instead of a startup error.

`setup()` is a config merge that itself eagerly prepends `install_dir` by raw string concat. The explicit option-list prepend after it dedupes, so the entry lands once and at the front whether or not the `pcall`ed setup succeeded. `:TSInstall` compiling a parser depends on the user's own toolchain: Wolf guarantees only a durable install target that survives restarts.

## Buffer tracking emits

A scripted `:terminal <cmd>` fires `BufEnter` with a brand-new unnamed buffer, then `BufFilePost` with a `term://` name, which emitted eagerly would be two history entries for one user action. Coalescing also collapses the two to four identical emits a plain `:e file` produces into one, which keeps stale in-flight notifications from racing a back or forward move. The one synchronous emit, `wolf_sync_buffers`, is safe for the history fold because it runs only once a rebuild is fully realized: it carries the settled state, and the scheduled emit that follows is identical. `WinClosed` fires while the closing window is still listed by `nvim_list_wins()`, which is harmless: core wholesale-replaces both per-window maps, so the next emit's survivor set drops it.

Measured on Neovim 0.12.6, a background `nvim_win_set_buf` fires `BufEnter` and `BufWinEnter` with the target window and buffer temporarily current, and restores focus before the call returns. The one bind that fires nothing is a no-op rebind of the buffer a window already shows, which changes nothing to emit.

## Owner path matching

`wolf_untitled_swap_exists` has two criteria. The first stats the `%`-mangled name `findswapname` derives from the full path, which the trailing `//` in `'directory'` selects: precisely the swap nvim checks first. The second scans `WOLF_SWAP_DIR` and compares each swap's recorded owner path, which is broader: a `.swo` leftover still holds recoverable edits without raising a dialog, and the scan is the fallback if the mangling ever drifts from nvim's.

The owner path is normalized forward, never the query backward. nvim writes it `home_replace`d, so a plain equality against an absolute path silently never matches under `$HOME`, which is every real user project. `fnamemodify(fname, ":p")` expands it and is the identity for a path already absolute. Not `expand()`, which also performs wildcard expansion and would mangle a name containing one, and not `fnamemodify(abspath, ":~")`, which yields `~/…` rather than the `~<user>/…` form nvim stores.

Measured on Linux, the swap for `/home/<user>/project/Untitled.md` records its owner as `~<user>/project/Untitled.md`, because nvim writes `b0_fname` through `home_replace`. The `fname == abspath` fast path is subsumed by the `fnamemodify(fname, ":p")` branch, and it stays because it is the only branch the fixtures outside `$HOME` (`/tmp`, `/private/var/folders`) exercise.

nvim's own `findswapname` uses both identities in turn: it derives the swap name from `b_ffname`, then counts an existing `.swp` as this file's, the one `ATTENTION` and `SwapExists` concern, only when that swap's expanded `b0_fname` names the same file, inode included. A swap owned by a different file is skipped for the next suffix. The two criteria in `wolf_untitled_swap_exists` check each identity on its own, so either one counts as a collision.

## Swapfile stays on

`nvim_buf_set_name` does not skip the swap check: the rename reaches `ml_setname`, which calls `ml_open_file` and creates the swap file right there, on a path where nvim has not armed the `SwapExists` choice machinery. Under `A` a stale swap there raises nothing, but the new swap lands on a `.swo`, which no later session offers. So every naming call site treats an existing swap for the candidate name as a collision.

For a buffer whose swap name fits under the cut (see [Long-path swaps](#long-path-swaps)), `'swapfile'` stays on when it is named, so the swap is created eagerly at naming time. That atomically claims the name against a sibling nvim in the same project, and it is what makes an unsaved Untitled buffer crash-recoverable at all.

`setlocal noswapfile` before naming would dodge a stale swap only by giving up recovery: `ml_open_file` returns early while `'swapfile'` is off, so the buffer gets no swap at naming or at its first change. That breaks the recovery pinned by `crates/wolf-core/tests/swap_recovery.rs` (`restart_recovers_unsaved_untitled_buffer`) and `crates/wolf-core/tests/nvim_restart.rs`, which is why name avoidance is the defence.

## Long-path swaps

With `'directory'` ending in `//`, `makeswapname` builds the name from the resolved full path with every `/` turned into `%` (`make_percent_swname`), so the name is as long as the path. `modname` then cuts it to `BASENAMELEN` (`NAME_MAX - 5`, 250 bytes), keeping the root end and dropping the file's own tail; Neovim 0.12.6 hashes nothing.

Past the cut, three things break. `recover_names` searches for the uncut name, so recovery fails with E305, after the `SwapExists` dead-owner branch has already scheduled the delete of the only copy. `:saveas!` onto a sibling fails with E768 in `check_overwrite`, because the target's cut name is the source's live swap. And since `findswapname` compares `b0_fname`, a sibling's swap lands on `.swo`, which no later session offers.

So a buffer whose full or resolved path exceeds `SWAP_NAME_MAX` gets `noswapfile`, with `b:wolf_long_path_noswap` recording that Wolf, not the user, turned it off. `BufNew` is the only per-buffer event early enough, because `check_need_swap` runs before `BufReadPre` and `BufNewFile`. The `BufReadPost`/`BufNewFile` backstop exists because, measured on 0.12.6, `:bdelete` then `:e F` resets `&l:swapfile` to the global value; by then a stale cut-name swap has been passed over under `A`, or, with `A` dropped by the user, answered `e` by the `SwapExists` callback's first branch, so it is never deleted.

So the `SwapExists` callback answers `e` for a long-path swap and never deletes it: recovering a cut name fails, and the delete would destroy the only copy.

`BufFilePost` turns swap back on after a rename onto a short path, unless `wolf_untitled_swap_exists` finds a swap there: setting `'swapfile'` opens the swap at once, on a path where `SwapExists` is not armed, so it would land on a `.swo`. `wolf_name_untitled` runs the same sync directly, because `rename_buffer` applies `BufFilePost` unforced and the startup rename happens inside the non-nested `VimEnter` callback.

Rejected fixes: dropping `//` (basenames collide across directories), a per-buffer or hashed `'directory'` (the option is global), and shortening paths only in tests (users would keep losing edits).

## Dead-owner re-arm

In Neovim 0.12.6 `findswapname` runs its `ATTENTION` block only when `'shortmess'` lacks `A`, and that block is also where `SwapExists` fires. With `A` set for good, a dead owner's swap would be neither offered nor recovered, which ends the `v:swapchoice = "r"` recovery that `crates/wolf-core/tests/swap_recovery.rs` and `nvim_restart.rs` pin.

So the `BufNew` callback removes `A` when the `.swp` nvim would pick for the buffer (`untitled_swap_file`, tried for the full and the resolved path) exists and its owner is dead, by the same `swap_owner_dead` test the `SwapExists` callback uses, and sets `swap_exists_armed`. `disarm_swap_exists` puts `A` back only while that flag is set, so a user's own `:set shortmess-=A` holds.

`SwapExists` calls it first, and `BufNew` schedules it as the backstop for a buffer that never loads. `sync_long_path_swap`, which runs after each rename, calls it because `:saveas` fires `BufNew` for the new name and a swap opened after that is opened outside any load, where a dead owner's swap would raise the `ATTENTION` text as a toast. That covers the rename whose swap opens at `BufFilePost`: a long-path buffer turning swap back on.

A buffer that already has a swap is renamed earlier, by `buf_name_changed` between `BufNew` and `BufFilePost`, with `A` still out. So `:saveas` of such a buffer onto a name where a dead owner's `.swp` sits still shows that swap's `ATTENTION` text as a toast. It is never a prompt, because `SwapExists` fires only on the file-load path.

A live owner's swap, which is another tab's Neovim, stays under `A` and raises nothing. A long-path buffer is skipped, since it gets `noswapfile`.

## Waits end before the tab closes

`do_quit` ends the waits of every buffer of the tab's Neovim, hidden ones included, with `0` before it notifies `close_tab`. The `0` is row 1 of the table in [§6](../../../specs/cli-and-file-open.md#6-waiting): the buffer was closed. It holds only for the quit family, `do_quit`'s, which Close Pane on the tab's last pane joins through `wolf_close_pane`. Every other route that closes a tab, its window or the app, Close Tab included, leaves the waits to the shutdown's `1`.

The front-end shuts the tab's core down on `close_tab`, and the shutdown ends every wait it still holds with `1`. Core applies notifications in the order Neovim sent them, so the `0` reaches the command first. `WinClosed` cannot do it here, because the front-end closes the tab without closing its last window in Neovim.

## A hide asks core

`BufHidden` fires before the window shows its new buffer, so the check is scheduled past it, and runs only while the buffer still shows in no window. A command that was killed or interrupted leaves its wait registered, so the check asks core with `wolf_wait_live` which of the buffer's waits are still connected, and core drops the others. No toast names a file whose every command has gone, and no watcher thread is needed to notice a hang-up.
