# `crates/wolf-gnome/tests/smoke_headless/terminal.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Terminal drawer smokes

S1, `terminal_drawer_opens_and_is_not_persisted_under_cage`, is the only evidence the drawer reaches a pixel, plus the persisted-layout leg and the focus legs, at `SMOKE_FONT_PT`. S2, `terminal_drawer_defaults_to_full_width_under_cage`, reads the drawer session's `stty size` and Neovim's geometry at `FULLWIDTH_SMOKE_FONT_PT`, because the content-width cap cannot bind at `SMOKE_FONT_PT`.

S3, `terminals_survive_a_neovim_restart_under_cage`, kills Neovim with a typed `:terminal` pane and the drawer open and reads both markers back after the restore. S4, `terminal_pane_splits_and_focus_under_cage`, moves focus in and out of a typed `:terminal` pane and splits it. S5, `terminal_close_chord_closes_the_drawer_and_a_split_under_cage`, closes the focused drawer, then a focused split terminal pane, with Ctrl+Shift+W. It reads back Neovim's window count, and whether each shell survived, to tell a kept session from a hung-up one.

The front-end sends a terminal pane's keys to its session and an editor pane's keys to Neovim, so a readback can only be typed where focus is. A Neovim readback is an Ex command writing a file, and a shell readback is an `echo` the shell runs. Each is typed where focus should be, and one typed where focus is not writes nothing, so a red focus leg surfaces as the scenario's exit code naming the leg rather than as a wrong value.

Every shell is pinned to `/bin/sh` and no `ENV`, so no host rc file reaches the page. Its fixed prompt goes in as `SMOKE_PS1`, which the readiness prelude exports as `PS1`, because the non-interactive bash that runs a scenario as `sh -c` drops an inherited `PS1`. A drawer marker is printed by `printf` in two `%s` halves with no trailing newline: the halves keep it out of the echoed typed line, and with no newline the next prompt follows on the same row, so the one-row drawer still shows it. The restart smoke's pane marker ends with a newline instead, because a resize of the pane makes readline redraw its prompt row and clear it.
