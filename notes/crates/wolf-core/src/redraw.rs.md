# `crates/wolf-core/src/redraw.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Fs change events

A `wolf_fs_changed` notification is not render state, so the handler takes no snapshot lock, bumps no `seq` and fires no wake of its own. The registry's debounced rescan fires a session's wake when, and only when, a swap lands.

The single argument is one msgpack array, the `[event_name, abs_path]` pair, like the other single-payload `wolf_*` events. The path is canonicalized to match the canonical paths in a session's `seen` set, with a raw fallback because canonicalization fails for a just-deleted path; a mismatch only fails open to an extra walk.

`force` marks the unconditional events. Only `BufWritePost` depends on the path being new to a session, so the path-less refocus events (`ShellCmdPost`, `FocusGained`, `TermLeave`, arriving as `[event, ""]`) need no extra decoding: `force` short-circuits before the path matters.

## Orphaned nav history

`win_hide`, sent for every window of a tabpage being left, already removes the `windows` entry, so a later `win_close` for that grid cannot recover the window handle and the pane's navigation history is orphaned. This is a known limit, left alone on purpose. nvim never reuses window handles, so an orphan is only memory, bounded per pane per nvim process; `nav_history` is free of timers, epochs and reapers, and reclaiming a bounded leak is not worth a second authority over it.

A window in a background tabpage is not absent from `win_buffers` or `buffers`: `runtime/nvim/init.lua` fills both by sweeping `nvim_list_wins()`, which spans every tabpage, as `per_window_buffers_are_global_across_tabpages` in `tests/tabpages.rs` measures and [the architecture spec](../../../../specs/architecture.md) states. A reaper keyed on absence from `wolf_buffers` would therefore not mistake a background window for a closed one, but that is not a reason to build one.

## Confirm dialog stash

A `confirm()` dialog's options ride the cmdline while its prompt text rides the toast as a `confirm` message. Neovim never emits `msg_clear` when the dialog resolves, so a toast left alone would linger. An invalid key produces a byte-identical hide and re-prompt, so `cmdline_hide` cannot clear outright: it captures the options-line prompt before resetting the cmdline, stashes the toast's messages in `pending_confirm`, and hides the toast.

The next `cmdline_show` tells the two cases apart by the prompt. A prompt-style cmdline (empty `firstc`, non-empty `prompt`) whose prompt matches the stashed options line is the dialog re-prompting, and restores the toast. Anything else, such as a `:` or `/` cmdline or a fresh `input()`, means the dialog resolved, and the stash is dropped with the toast left cleared.
