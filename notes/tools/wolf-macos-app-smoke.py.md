# `tools/wolf-macos-app-smoke.py`

Last verified at commit `a20c011d440d16db41fe785bb5adfb8e3d03db59`.

doc-lint does not lex `.py` files, so nothing checks the `#` comments that link these headings. Keep them matched by hand.

## Launching the bundle

The wrapper resolves `Wolf.zip` through `bazel cquery --output=files`, and `extract_bundle` unpacks a private copy with `ditto -x -k` into the run directory. It never uses the `_archive-root/` directory beside the zip: that directory is undeclared, so after a disk-cache hit it can be stale or missing, and a private copy is also immune to a concurrent rebuild.

`App.launch` execs `Contents/MacOS/Wolf` directly with `start_new_session=True`. A direct exec passes the environment through while `Bundle.main` still resolves to the `.app`. The new session gives the harness a PID and a process group, which the Neovim child joins, so `os.killpg` reaches both, and it keeps a terminal Ctrl-C off the app.

The drawer's shell is the exception. Core's pty spawn calls `setsid` ([`pty.rs`](../../crates/wolf-core/src/terminal/pty.rs)), putting the shell in its own session, so neither the group kill nor cleanup's sweep of argv under the app copy reaches it. It ends on `SIGHUP` when the pty master closes with the app.

It does not use `open -n --env`. That returns no PID, `pgrep` is ambiguous while the user's own Wolf (the same bundle id, `org.wolf.WolfMacOS`) runs, and LaunchServices state restoration would apply.

`APP_FLAGS` carries two flags. `-ApplePersistenceIgnoreState YES` keeps AppKit off `~/Library/Saved Application State/org.wolf.WolfMacOS.savedState`, which the user's own Wolf shares, and `-NSAppSleepDisabled YES` keeps App Nap from throttling the run, as a defence only.

## Isolation

`app_env` removes every `WOLF_*` and `DYLD_*` variable, and `NVIM`, `NVIM_LISTEN_ADDRESS`, `VIMINIT`, `MYVIMRC` and `ENV`, then points `WOLF_STATE_DIR` and `WOLF_CONFIG_DIR` into the run directory. The config directory starts empty, so `load_or_create` writes the default config there and the grid metrics do not depend on the user's config. `SHELL=/bin/sh` is what `LiveCore` pins, and a `TMPDIR` inside the run takes core's private Neovim state root with it.

The `nvim` client gets the same scrub, plus its own `NVIM_APPNAME` and `XDG_*_HOME` under `client/`.

`HOME` is left alone. `state_dir()` and `preferences_dir()` read their overrides first, core pins Neovim's `XDG_*` itself, and the seeded launch starts Neovim in the project. cfprefsd and AppKit's saved state do not follow `$HOME`, so a temporary one would isolate nothing there, and it would make the drawer's login shell disagree with CoreFoundation.

Wolf writes no defaults of its own: `Sources/` has no `UserDefaults`, autosave name, `restorationClass` or `NSDocumentController`, and Open Recent is core's recent-directories list under `WOLF_STATE_DIR`. The smoke does not write `~/Library/Saved Application State/org.wolf.WolfMacOS.savedState`, though the user's own Wolf may create it.

`-ApplePersistenceIgnoreState` writes `$(getconf DARWIN_USER_TEMP_DIR)org.wolf.WolfMacOS.savedState` instead. `record_saved_state` notes before the first launch whether that path exists, and cleanup deletes it only when it did not. Otherwise cleanup leaves it and logs a warning, since it may belong to the user's own Wolf.

## Driving Neovim

The app is launched with `WOLF_INIT_LUA` naming `PRELUDE_LUA`, which opens the socket and then `dofile`s the bundled `init.lua`.

- `serverstart` comes first, so a broken `init.lua` still leaves a socket to diagnose through. Its `pcall` keeps a crash-restart respawn, which reuses the path, from aborting.
- `dofile` sets the chunk name to the bundled path, so `init.lua`'s `debug.getinfo` self-location still finds the plugins and parsers beside it.
- The prelude holds no timers or trigger files, so control flow, deadlines and failure messages stay in the harness, one synchronous request per step.

The client is the bundle's own `nvim`, the server's version, run as `--clean --headless --server <sock> --remote-expr <expr>` under `RPC_TIMEOUT_S`. It prints the value under `--headless`, and a missing socket gives `E247` with empty stdout. That is why `EX_EXPR` and `EDIT_EXPR` return the sentinel `EX_OK` rather than `''`.

`PROBE_EXPR` returns the whole tabpage as one JSON value, so a probe never mixes two layouts, and output that is not that JSON counts as no answer yet. It reads `winwidth()` and `winheight()`, which report the grid size the front-end requested, as the GNOME twin's checks do; do not switch to `nvim_win_get_width` without measuring. `wait_for` needs its predicate on `STABLE_READS` probes in a row, so a layout caught mid-resize passes no check.

Paths and commands reach Lua only through `_A`, as a Vimscript literal with `'` doubled (`vim_string`), and are never spliced into Ex text.

The run directory is made under `/tmp` and resolved with `os.path.realpath`, because `/tmp` is a symlink and the socket path must fit macOS's 104-byte `sun_path` (`SUN_PATH_MAX`). Each launch gets its own `sock/nvim-<n>.sock`, and a path at the limit is a setup error, exit 2.

## The seeded first launch

A fresh first launch is the empty state, Neovim in the home directory with no folder. A scripted `:cd` leaves that state only if a render wake has already recorded `initialCwd` in `WolfWindowController.onRenderWake`, which is a race, and Open Folder needs an `NSOpenPanel`. So `write_fixtures` seeds `state/state.json` instead, in the v1 schema [`state.rs`](../../crates/wolf-core/src/state.rs) owns: one window, one tab with `cwd` the resolved project directory, and a lone `za.md` leaf.

The window's width 0 is the unset sentinel, so the window keeps the size it is built at, beside a visible sidebar, which keeps L1 under `PaneContainerView.maxGridCols`. C1 and R2 tell a zoom from none by zb's width reaching L1.w, which needs a split half to be narrower than L1, and C3's `w < L1.w` on both halves checks that premise.

The layout rebuild `:edit`s `za.md`, so `za.md` showing proves the view was built, its first pane bound and the rebuild run. That, the socket answering and `exists(':WolfZoomPane') == 2` together are the readiness signal, so the app needs no readiness file like the GNOME front-end's `WOLF_READY_FILE`.
