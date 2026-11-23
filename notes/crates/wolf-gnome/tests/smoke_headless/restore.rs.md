# `crates/wolf-gnome/tests/smoke_headless/restore.rs`

Last verified at commit `cc9c7daaaa226139ecbabc36df92df9cefe63016`.

## Relaunch smoke exit codes

The relaunch smokes run two sequential app launches inside one `cage`, so each holds one cage slot. Three share the codes below; the divider smoke keeps `POINTER_PREAMBLE`'s codes for the helpers it copies and carries its own table, and the three-tab smoke carries its own.

- `11`: run 1's app died before it was ready for input.
- `12`: run 1 was still alive after the quit poll budget, so the drain and close path failed.
- `13`: run 1 exited non-zero. That implicates a panic in `Coordinator::on_window_closing` (gather, commit and blocking flush) or a `RefCell` double borrow against `window::wire_close`'s `tabs.shutdown_all()`.
- `14`: run 2's app died before it was ready for input.
- `15`: the Neovim readback file never appeared in run 2.
- `16`: run 1 wrote no `wolf.sqlite` (the single-pane and two-window smokes).
- `21` to `31`: the two-window smoke's own, in `TWOWIN_SCENARIO`'s table.

Every observable is polled rather than slept for. Run 1 is reaped with `wait` so its exit status is checked, because "the process is gone" is equally true of a clean shutdown and of a panic in the GTK main loop.

A save is read from a copy the prelude's `freeze_state` makes after the run that wrote it has exited. The store creates `wolf.sqlite` at its first open, so the file existing proves no save; that the save holds windows is asserted on the Rust side, through `AppStateStore` on the copy.

## Two window smoke fixtures

`cage(1)` shows a single maximized application, and presenting a backgrounded `adw::ApplicationWindow` under cage does not raise it, so no capture can show two windows. The observables are the count of live `nvim` children of the app pid (the crash smokes' `/proc` scan), the state database each run wrote, frozen after it exited, and the `getcwd()` and current buffer the focused window's Neovim writes.

The second window opens from inside the one process, through the palette (Ctrl+Shift+P, `TWOWIN_NEW_WINDOW_QUERY`, Enter). A second `wolf-gnome` launched under the same cage hands off to the first through single-instance activation on cage's private `dbus-run-session` bus, which exercises the launch path instead of New Window.

Window A is opened first and window B second, and B is the window focused last, so the registry order and the stacking order differ, and a save in either order is told apart. The restore maps the saved frontmost window last, and under `cage` the last-mapped window is the one keys reach, so run 2's readback says which window came back in front.

Every tab is in the one project, B's after a typed `:cd`, so the project state is the rule for a directory open in two windows. Each tab shows a file no other tab shows, which is what lets the saved panes say which tab is where. Under `shortmess+=A` a swap a live Neovim owns raises no notice, so tabs sharing a project load their files quietly.

The fixture names may be the strings the assertions compare, because no page is read: the strings come from the saved panes' paths and from the captive Neovim's own `getcwd()`, which no sidebar row or toast can write. The names are still in `RELAUNCH_FIXTURE_NAMES`, and the exemption block of `restore_fixture_names_cannot_satisfy_their_own_assertions` states the exemption and the crossing executably.

## State dir isolation checks

Check 1, the positive one: `wolf.sqlite` exists and is non-empty at the smoke's own `WOLF_STATE_DIR`. Drop the variable and the file is absent (the scenario exits 16 first).

Check 2: `cage_command` redirects the app's `HOME` into the temp tree, so with `XDG_STATE_HOME` unset an app ignoring `WOLF_STATE_DIR` resolves `state_dir()` to `$HOME/.local/state/wolf`, inside the temp tree where check 3 cannot see it. Those exact paths must hold no `wolf.sqlite`, which positively proves the variable was honoured. The path comes from the same `smoke_home` helper `cage_command` uses, so the check and the environment cannot drift.

Check 3 compares the developer's real `wolf.sqlite`. It bites on a host that exports `XDG_STATE_HOME` or `XDG_DATA_HOME`, where a leak lands outside the temp tree and check 2 is blind, so checks 2 and 3 together cover both environments.
