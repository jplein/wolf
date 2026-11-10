# `crates/wolf-gnome/tests/smoke_headless/restore.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Relaunch smoke exit codes

The relaunch smokes run two sequential app launches inside one `cage`, so each holds one cage slot. Three share the codes below; the divider smoke keeps `POINTER_PREAMBLE`'s codes for the helpers it copies and carries its own table.

- `11`: run 1's app died before it was ready for input.
- `12`: run 1 was still alive after the quit poll budget, so the drain and close path failed.
- `13`: run 1 exited non-zero. That implicates a panic in `Coordinator::on_window_closing` (gather, commit and blocking flush) or a `RefCell` double borrow against `window::wire_close`'s `core.borrow_mut().take()`.
- `14`: run 2's app died before it was ready for input.
- `15`: the Neovim readback file never appeared in run 2.
- `16`: run 1 wrote no `state.json` (the single-pane and two-window smokes).
- `21` to `26`: the two-window smoke's own, in `TWOWIN_SCENARIO`'s table.

Every observable is polled rather than slept for. Run 1 is reaped with `wait` so its exit status is checked, because "the process is gone" is equally true of a clean shutdown and of a panic in the GTK main loop.

## Two window smoke fixtures

`cage(1)` shows a single maximized application, and presenting a backgrounded `adw::ApplicationWindow` under cage does not raise it, so no capture can show two windows. The two observables are the count of live `nvim` children of the app pid (the crash smokes' `/proc` scan) and the `state.json` run 1 wrote, frozen before run 2 can overwrite it.

The second window opens from inside the one process, through the palette (Ctrl+Shift+P, `TWOWIN_NEW_WINDOW_QUERY`, Enter). A second `wolf-gnome` launched under the same cage hands off to the first through single-instance activation on cage's private `dbus-run-session` bus, which exercises the launch path instead of New Window.

Window B is `:cd`'d into a subdirectory and opens a file there. The per-project swap directory is keyed on the cwd and outlives the process, so two windows on one project meet Neovim's correct `W325: Ignoring swapfile` warning, which is not this smoke's subject. Two projects also make the two saved `cwd` values distinct, so "restored in saved order" is falsifiable.

The fixture names may be the strings the assertions compare, because no page is read: the strings come from `state.json`'s `TabState::layout` leaves and from the captive Neovim's own `getcwd()`, which no sidebar row or toast can write. The names are still in `RELAUNCH_FIXTURE_NAMES`, and the exemption block of `restore_fixture_names_cannot_satisfy_their_own_assertions` states the exemption and the crossing executably.

## State dir isolation checks

Check 1, the positive one: run 1's `state.json` exists and is non-empty at the smoke's own `WOLF_STATE_DIR`. Drop the variable and the file is absent (the scenario exits 16 first).

Check 2: `cage_command` redirects the app's `HOME` into the temp tree, so with `XDG_STATE_HOME` unset an app ignoring `WOLF_STATE_DIR` resolves `state_dir()` to `$HOME/.local/state/wolf`, inside the temp tree where check 3 cannot see it. Those exact paths must hold no `state.json`, which positively proves the variable was honoured. The path comes from the same `smoke_home` helper `cage_command` uses, so the check and the environment cannot drift.

Check 3 compares the developer's real state file. It bites on a host that exports `XDG_STATE_HOME` or `XDG_DATA_HOME`, where a leak lands outside the temp tree and check 2 is blind, so checks 2 and 3 together cover both environments.
