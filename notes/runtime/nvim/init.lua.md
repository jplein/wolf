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

Coalescing also collapses the two to four identical emits a plain `:e file` produces into one, which keeps stale in-flight notifications from racing a back or forward move. `WinClosed` fires while the closing window is still listed by `nvim_list_wins()`, which is harmless: core wholesale-replaces both per-window maps, so the next emit's survivor set drops it.

## Owner path matching

Measured on Linux, the swap for `/home/<user>/project/Untitled.md` records its owner as `~<user>/project/Untitled.md`, because nvim writes `b0_fname` through `home_replace`. The `fname == abspath` fast path is subsumed by the `fnamemodify(fname, ":p")` branch, and it stays because it is the only branch the fixtures outside `$HOME` (`/tmp`, `/private/var/folders`) exercise. nvim's own `SwapExists` machinery matches on the swap file name derived from `b_ffname`, not on `b0_fname`; the two criteria in `wolf_untitled_swap_exists` are those two identities.

## Swapfile stays on

`'swapfile'` stays on when a buffer is named, so the swap is created eagerly at naming time. That atomically claims the name against a sibling nvim in the same project, and it is what makes an unsaved Untitled buffer crash-recoverable at all.

`setlocal noswapfile` on that buffer would break the recovery pinned by `crates/wolf-core/tests/swap_recovery.rs` (`restart_recovers_unsaved_untitled_buffer`) and `crates/wolf-core/tests/nvim_restart.rs`. It would not even avoid the dialog: it only defers `ml_open_file` to `changed()`, moving the modal onto the user's first keystroke.
