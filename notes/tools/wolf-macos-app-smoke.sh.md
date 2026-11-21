# `tools/wolf-macos-app-smoke.sh`

Last verified at commit `a20c011d440d16db41fe785bb5adfb8e3d03db59`.

## Why these refusals

The smoke launches the real `Wolf.app` and needs its window to become key, so it runs only in this user's unlocked GUI session, by hand. Each of the script's refusals exits 2 before the build, so a run that cannot pass costs nothing.

- Not Darwin: it launches a macOS bundle.
- `REGINA_AGENT` set: a regina worker runs under Seatbelt, which denies it the GUI session, and a worker must not take over the user's desktop unannounced.
- `TEST_TMPDIR` set: Bazel sets it in every test action, and `bazel test //...` or a gate must never take over the desktop. A test tagged `no-sandbox` runs in this user's session, so the session checks alone would not stop it.
- `/dev/console` not owned by `id -un`: another user, or the login window, holds the console, so this user has no session for the app to draw in.
- The screen locked, which `ioreg -n Root -d1` reports as `"CGSSessionScreenIsLocked" = Yes` in `IOConsoleUsers` or as the Root-level `"IOConsoleLocked" = Yes`. A restored window never becomes key on a locked screen, and a restored view becomes its project's current view only in `WolfWindowController.windowDidBecomeKey`, so nothing renders or drains and the first check times out blaming the restore. The harness repeats this check before each launch, since the screen can lock during the build.
- No `bazel` or `python3` on `PATH`: the message names the dev shell and the `nix develop --command` form.
- Any argument: it takes none, so a mistyped flag fails rather than being ignored.

The session checks are not `launchctl managername`, which prints `Background` inside tmux as well as over `ssh`, although a Wolf launched from tmux runs as a foreground app in the `gui/<uid>` session. It would refuse a run that works.

The macOS base tools are called by absolute path, such as `/usr/bin/stat`, because the user's `PATH` may put GNU coreutils first, and GNU `stat -f` means file-system status, which breaks the console check. The harness does the same for `ioreg`, `getconf`, `ditto` and `pgrep`.

Neither the script nor its harness has a Bazel target, so no wildcard can sweep it in, and `gates.sh` runs only the commands it names. The root `//:comment_srcs` glob does match this script, but only as doc-lint's input: nothing executes it.
