# `runtime/nvim/init.lua`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Spellfile is global

`'spellfile'` is local to the buffer, but its global value seeds every new buffer, verified on all three open paths: a startup file argument, `:edit`, and `:enew` followed by `set ft=markdown`. Moving the assignment into a FileType autocmd registered later in the file fails for a markdown startup file argument, for the same `syntax on` ordering reason as the `WolfSpell` autocmd. If a buffer-local value is ever needed, set it inside the existing `WolfSpell` callback rather than registering a new autocmd.

Left unset, nvim derives `'spellfile'` from the first writable runtimepath entry, which sits under the private `XDG_DATA_HOME` wolf-core pins for hermeticity, so a taught word would die with the process. nvim names the dictionary from `'spelllang'` and `'encoding'` (`<lang>.utf-8.add`), so a filename that disagrees with `SPELL_LANG` makes `zg` write a file nvim never reads back.

## Vendored tree resolution

The environment fallback is the only thing that fires the treesitter gate on Linux, where no sibling tree exists. Deleting it removes the vendored queries, the parsers and the `FileType=markdown` autocmd, which costs fenced-block injections but not base markdown highlighting.

The fallback works because `vim.env` is a view over the process environment, which `--clean` does not touch. The captive nvim inherits that environment: `nvim.rs` overrides only its `NVIM_APPNAME`, `XDG_*` and `WOLF_*` variables on the child and calls no `env_clear`, and `cage` clears nothing either. The textobjects tree is a bare sibling probe on purpose; see the Limitations in [the runtime README](../../../runtime/nvim/README.md#limitations).

## User parser activation

The explicit `vim.cmd.runtime("plugin/nvim-treesitter.lua")` is what registers `:TSInstall`, `:TSUpdate`, `:TSUninstall` and `:TSLog`; `setup()` does not. `--clean` keeps `'loadplugins'` on, so the startup plugin phase would also load it after this file, but the explicit call makes registration deterministic, and the plugin's `vim.g.loaded_nvim_treesitter` guard and lazy requires make the later load a cheap no-op.

Only that plugin file is sourced: the sibling `query_predicates.lua` registers predicates that no bundled language uses. The `pcall` around `setup()` degrades a version incompatibility to a missing `:TSInstall` instead of a startup error.

## Buffer tracking emits

Coalescing also collapses the two to four identical emits a plain `:e file` produces into one, which keeps stale in-flight notifications from racing a back or forward move. The one synchronous emit, `wolf_sync_buffers`, is safe for the history fold because it runs only once a rebuild is fully realized: it carries the settled state, and the scheduled emit that follows is identical. `WinClosed` fires while the closing window is still listed by `nvim_list_wins()`, which is harmless: core wholesale-replaces both per-window maps, so the next emit's survivor set drops it.

## Owner path matching

Measured on Linux, the swap for `/home/<user>/project/Untitled.md` records its owner as `~<user>/project/Untitled.md`, because nvim writes `b0_fname` through `home_replace`. The `fname == abspath` fast path is subsumed by the `fnamemodify(fname, ":p")` branch, and it stays because it is the only branch the fixtures outside `$HOME` (`/tmp`, `/private/var/folders`) exercise.

nvim's own `findswapname` uses both identities in turn: it derives the swap name from `b_ffname`, then raises `ATTENTION` for an existing `.swp` only when that swap's expanded `b0_fname` names the same file, inode included. A swap owned by a different file is skipped for the next suffix. The two criteria in `wolf_untitled_swap_exists` check each identity on its own, so either one counts as a collision.

## Swapfile stays on

For a buffer whose swap name fits under the cut (see [Long-path swaps](#long-path-swaps)), `'swapfile'` stays on when it is named, so the swap is created eagerly at naming time. That atomically claims the name against a sibling nvim in the same project, and it is what makes an unsaved Untitled buffer crash-recoverable at all.

`setlocal noswapfile` before naming would avoid the dialog only by giving up recovery: `ml_open_file` returns early while `'swapfile'` is off, so the buffer gets no swap at naming or at its first change. That breaks the recovery pinned by `crates/wolf-core/tests/swap_recovery.rs` (`restart_recovers_unsaved_untitled_buffer`) and `crates/wolf-core/tests/nvim_restart.rs`, which is why name avoidance is the defence.

## Long-path swaps

With `'directory'` ending in `//`, `makeswapname` builds the name from the resolved full path with every `/` turned into `%` (`make_percent_swname`), so the name is as long as the path. `modname` then cuts it to `BASENAMELEN` (`NAME_MAX - 5`, 250 bytes), keeping the root end and dropping the file's own tail; Neovim 0.12.6 hashes nothing.

Past the cut, three things break. `recover_names` searches for the uncut name, so recovery fails with E305, after the `SwapExists` dead-owner branch has already scheduled the delete of the only copy. `:saveas!` onto a sibling fails with E768 in `check_overwrite`, because the target's cut name is the source's live swap. And since `findswapname` compares `b0_fname`, a sibling's swap lands on `.swo`, which no later session offers.

So a buffer whose full or resolved path exceeds `SWAP_NAME_MAX` gets `noswapfile`, with `b:wolf_long_path_noswap` recording that Wolf, not the user, turned it off. `BufNew` is the only per-buffer event early enough, because `check_need_swap` runs before `BufReadPre` and `BufNewFile`. The `BufReadPost`/`BufNewFile` backstop exists because, measured on 0.12.6, `:bdelete` then `:e F` resets `&l:swapfile` to the global value; by then `SwapExists` has fired on any stale cut-name swap, so its first branch answers `e` and never deletes.

`BufFilePost` turns swap back on after a rename onto a short path, unless `wolf_untitled_swap_exists` finds a swap there: setting `'swapfile'` opens the swap at once, on a path where `SwapExists` is not armed. `wolf_name_untitled` runs the same sync directly, because `rename_buffer` applies `BufFilePost` unforced and the startup rename happens inside the non-nested `VimEnter` callback.

Rejected fixes: dropping `//` (basenames collide across directories), a per-buffer or hashed `'directory'` (the option is global), and shortening paths only in tests (users would keep losing edits).
