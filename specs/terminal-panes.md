# Terminal panes

Wolf's terminals are emulated in `wolf-core` by libghostty-vt, and `wolf-core`, not Neovim, owns each terminal's pty and shell process. Neovim keeps owning layout: a terminal pane is a Neovim window whose content the front-end paints from the terminal session rather than from its grid.

This spec is authoritative for the deliveries g01 to g04 listed under [Delivery](#delivery). Every terminal pane, the drawer included, is a core session. A Neovim terminal a script makes is not, as [What goes away](#what-goes-away) says.

## libghostty-vt

libghostty-vt is a C library, with a Zig module, that turns program output into a screen of styled cells and scrollback. It has no pty and no renderer. Its C API is `<ghostty/vt.h>`, which includes one header per area under `ghostty/vt/`. Wolf uses these areas:

- the terminal: `ghostty_terminal_new`, `_free`, `_vt_write`, `_resize`, `_get` and `_set`, the scrollback viewport and the modes;
- effect callbacks: `WRITE_PTY` for the replies to `DA` and `DSR` queries, the title, the pwd an `OSC 7` reports, the clipboard, the bell, and `RENDER_HOLD` for synchronized output (mode 2026);
- the render state, `ghostty_render_state_*`: a copy of one frame with dirty tracking (false, partial or full, plus a dirty flag per row), row and cell iterators, resolved colours, the cursor and the palette;
- the key, mouse, paste (`ghostty_paste_encode`, `ghostty_paste_is_safe`) and focus encoders, the selection, and the formatter.

Sources: the [terminal API](https://lib.ghostty.org/vt/group__terminal.html), [`render.h`](https://lib.ghostty.org/vt/render_8h.html), [`vt.h`](https://lib.ghostty.org/vt/vt_8h.html), and the headers in the release tarball.

Wolf pins release 1.0.0, released 2026-11-06 with the tag `vt-1.0.0`. It is the library's first stable release: it follows semantic versioning, its symbols carry `GHOSTTY_VT_1.0` versions, and its CI compares every change against recorded ABI goldens. The shared library's `SONAME` carries the major version only. `ghostty_version()` reports the linked version at run time and `GHOSTTY_VT_VERSION` the headers' at compile time. Before 1.0 the API broke several times. Sources: [the 1.0 announcement](https://mitchellh.com/writing/libghostty-1-0), `PACKAGING-LIBGHOSTTY.md` in the tarball, and the [Ghostty 1.4.0 release notes](https://ghostty.org/docs/install/release-notes/1-4-0).

libghostty-vt is under the `MIT` licence ("Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors", `LICENSE` at the tarball root). Its `deps/libghostty-vt-third-party.json` lists the third-party code it compiles in:

- Highway 1.2.0, Apache-2.0 or `BSD`-3-Clause;
- simdutf 9.0.0, Apache-2.0 or `MIT`, vendored under `pkg/simdutf/vendor`;
- translate_c 2.0.0, `MIT`;
- uucode 0.2.0, `MIT` and Unicode-3.0;
- Wuffs 0.4.0-alpha.10, Apache-2.0 or `MIT`.

aro (`MIT` and Unicode-3.0) runs only during the build and ships nothing. Every linked library's notice is in `wolf_core::LIBRARY_LICENSES`, which both About surfaces show.

Two alternatives are rejected:

- The crates.io `libghostty-vt` 0.2.2 ([docs.rs](https://docs.rs/libghostty-vt), [repository](https://github.com/uzaaft/libghostty-rs)). Its `-sys` build script fetches the Ghostty source and runs Zig from `build.rs`, which fits neither `crate_universe` nor one checksummed pin per platform, and the crate calls its own API unstable.
- The prebuilt `ghostty-vt-1.0.0.xcframework.zip`. macOS would ship a prebuilt binary while Linux compiles, so the two platforms would not build the same pinned source.

## Building libghostty-vt

Both platforms build the same release from its source tarball and its companion tarball of Zig packages, each pinned by sha256: `wolfGhosttyVtPin` in `flake.nix` on Linux and `bazel/ghostty-vt/extensions.bzl` on macOS. A bump changes both, and the Zig pins too when `build.zig.zon`'s `minimum_zig_version` moves to another minor. The release tarball is required, not a tag archive: only the tarball carries the `LIB_VERSION` file that makes `zig build` build the library rather than the Ghostty app.

The build needs Zig at the minor `build.zig.zon` names, which for 1.0.0 is 0.17. On Linux, Zig is nixpkgs' `zig_0_17`, a build input of the library's derivation only and in neither dev shell. On macOS, it is the official 0.17.0 release tarball for the host architecture, pinned in `//bazel/zig`.

Both builds run `zig build` with `--system <deps>`, `-Doptimize=ReleaseFast` and `-Dcpu=baseline`, with no network and an empty Zig global cache. The cache must be new, because Zig 0.17 can crash with "reached unreachable code" on one that already holds a package's archive.

Both platforms link the static archive, `libghostty-vt.a`, so no dylib ships and no rpath exists. The archive bundles Zig's compiler runtime and its C++ is built without the C++ standard library, so it needs no further link flags.

- Cargo links it through `crates/wolf-vt/build.rs`, which reads `WOLF_GHOSTTY_VT_DIR`, the install prefix the Linux dev shell exports. The package build gets the same variable in g01.
- Bazel builds it in a `ghostty_vt_archive` action and exposes it through `cc_import` as `//bazel/ghostty-vt:ghostty_vt`. Bazel never runs `build.rs`. The action takes the C toolchain's environment, which carries the selected Xcode to the build's `xcrun` calls.

`wolf-vt` is the one crate that declares the C API, by hand and only the subset it uses, and it holds every `unsafe` line and all the link wiring. Neither `wolf-vt` nor libghostty-vt matches a pattern in `tools/lint/wolf_core_forbidden_deps.txt`, and libghostty-vt draws nothing. So the rule that keeps `wolf-core` free of GUI toolkits does not require a separate crate, and `wolf-core` may depend on `wolf-vt`, which g01 does. It is a crate of its own for containment.

`wolf_vt::version()` reports the linked release, and the binding test pins `1.0.0`, so a bump made on one platform only fails the test on the other.

## Terminal sessions

A terminal session is a pty master, the child shell, a `wolf_vt::Terminal`, a reader thread named `wolf-term-<id>`, a writer, and a published frame. Each is named by a `TerminalId`, a `u64` from 1, allocated per `WolfCore`, never reused, and stable across Neovim restarts.

Core spawns the shell with `libc::openpty` and `std::process::Command`, whose `pre_exec` calls `setsid` and sets the controlling terminal with `TIOCSCTTY`. There is no pty crate, following the wolf-core decisions' preference for a small hand-rolled module over a dependency.

- The program is `$SHELL`, else the shell in the user's passwd entry, else `/bin/sh`. The API takes an explicit command as well, so tests run `/bin/sh`.
- macOS starts a login shell (`-l`), as its terminals do; Linux starts an interactive non-login shell, as GNOME's do.
- The environment is the app's own plus `COLORTERM=truecolor`, `TERM_PROGRAM=Wolf` and a terminal type. When the terminfo directory the front-end configures holds the `xterm-ghostty` entry, the type is `xterm-ghostty` and `TERMINFO` names that directory, replacing any inherited one; the front-ends read it from `WOLF_TERMINFO_DIR`, which the bundle and the package wrapper default. Otherwise the type is `xterm-256color` and Wolf sets no `TERMINFO`, because test binaries and runs outside a bundle or package have no entry, and every host database knows `xterm-256color`. The entry is compiled at build time from the pinned libghostty-vt release's terminfo source and ships at `Wolf.app/Contents/Resources/terminfo` and the package's `share/wolf/terminfo`, so no host database is relied on.
- The working directory is the requesting view's tabpage cwd.

The reader thread feeds the pty's bytes into the terminal under a `parking_lot::Mutex`. Every write to the pty goes through the writer: none comes from a front-end thread, and none happens while the terminal lock is held. libghostty-vt's `WRITE_PTY` replies are queued to the same writer, so a full pty buffer cannot deadlock the reader.

A split-pane session lives as long as its placeholder buffer, and the drawer's lives until its shell exits or the core shuts down. Shutdown sends `SIGHUP` to each session's process group, closes the master, and sends `SIGKILL` after a named grace constant.

When a shell exits, core closes every window showing its session; a tabpage's last window has its buffer replaced by an empty one instead. The drawer closes, and its next toggle starts a new shell.

## Terminal panes in the pane tree

A terminal pane is an ordinary Neovim window showing a placeholder buffer bound to one session. The front-end paints it from the session and never from its grid. Because Neovim keeps owning layout, splits through `Split` intents, close through `win_close`, `wolf_rebuild_layout`, per-window full width, tabpage views, and the 1:1 pane-tree parity of both front-ends all apply to terminal panes unchanged. A pane kind Wolf owns is rejected: it needs a second layout authority, in core and in both front-ends.

The placeholder buffer is created by an `init.lua` helper and is:

- `buftype=nofile`, `noswapfile`, `nomodifiable`, unlisted and unnamed;
- tagged `b:wolf_terminal_id = <id>` and `b:wolf_full_width = true`;
- `bufhidden=wipe` for a split session and `bufhidden=hide` for the drawer's.

Being unnamed and `nofile` keeps the path-based trackers (`current_file`, `buffers`, `open_files`) blind to it. `wolf_buffers` reports the id per window as `Snapshot.win_buffers[win].terminal: Option<TerminalId>`. A pane is a terminal pane exactly when its window has one; like preview mode, this is derived and never stored.

Splitting a terminal pane, with `<C-w>s` or `<C-w>v`, splits the placeholder window and gives the new window a new session, one pty per pane. Its cwd is the origin shell's directory: its `OSC 7` report, else the operating system's view of the shell process's cwd, else the tabpage cwd. The split reaches the front-end as an ordinary `Split` intent.

The front-end focuses a terminal pane like any other and sends `FocusWin` for its placeholder, which keeps Neovim's current window, `active_win` and every current-window command consistent.

`<C-w>c` or `<C-w>q` in the pane, Close Pane, or `:close` from elsewhere closes the placeholder window, and the last window is refused as `wolf_do_close` refuses it. Core observes the close through `win_close`, and the buffer's wipe hangs up the session.

Terminal panes default to full width, and the existing full-width command toggles it. The front-end sizes the session from the pane's framed content area. A session shown in more than one pane, which only the shared drawer can be (macOS keeps one drawer per project core), is sized by the most recently focused of its panes, and the others paint it clipped or padded.

## The drawer

There is one drawer session per core: per window on GNOME and per project on macOS. `WolfCore::toggle_terminal()` starts a drawer session if none is live, in the current tabpage's cwd, before it queues `ToggleTerminal { id }`; starting one is a pty and a fork, never an RPC, and the event carries the id because the input pump has no registry. `drawer_terminal()` names the live drawer session. `wolf_toggle_terminal(id)` then opens or closes the drawer window in the current tabpage: a `botright` split a third of the height, tagged `w:wolf_drawer`, showing the drawer's placeholder, focused, and emitting the `drawer_open` intent. Closing the drawer window never ends the session.

Both front-ends keep the drawer outside their pane trees, as their existing decisions say. Core's router ignores split chords from the drawer's session, and `<C-w>c` or `<C-w>q` in the drawer closes it.

## The render contract

Each session publishes a lock-guarded `TerminalFrame` under [the snapshot-and-wake session pattern](../notes/wolf-core-decisions.md#the-snapshot-and-wake-session-pattern). `WolfCore::terminal_frame(id)` returns an owned read. The frame carries:

- `version`, monotonic per session;
- `cols` and `rows`;
- the viewport cells, row-major: the grapheme text, the resolved foreground and background or the default, bold, faint, italic, the underline style, strikethrough, inverse, invisible and blink, and the wide-lead and spacer flags;
- `row_versions`, the frame version at which each row last changed, which a resize or a full redraw sets for every row;
- the cursor's column, row, visibility, style (block, bar, underline or hollow) and blinking, absent while it is scrolled out of view;
- the default foreground, background and cursor colours, and the selection per row;
- the scrollbar's `total`, `offset` and `len`;
- `mouse_tracking`, `title`, `pwd`, the shell name, and `exited: Option<i32>`.

Damage comes from libghostty-vt's render-state dirty tracking, which feeds `row_versions`. A front-end repaints the rows newer than the version it last painted, so independent readers share no dirty bits.

A publish fires the core's existing wake, routed by project token on macOS, and never bumps `Snapshot.seq`, which keeps the rule that only a redraw flush does. A front-end gates a terminal repaint on the frame's `version`. Publishing is coalesced to at most one per named interval per session, about 8 ms. Synchronized output (mode 2026, through `RENDER_HOLD`) defers it until released or until a named maximum passes.

The default foreground and background come from the editor's highlight defaults. The 16 `ANSI` colours come from the colorscheme's `g:terminal_color_0` to `g:terminal_color_15`, which an extended `wolf_theme` carries. The cursor colour comes from `WolfCursor` when set. Core re-applies them on `ColorScheme`; palette entries 16 to 255 keep libghostty-vt's defaults.

On the bridge, from g02, a frame is an opaque handle with flat per-row accessors mirroring the grid read path, and no callback crosses it mid-stream. Kitty graphics are not painted.

## Input

The front-end decides where input goes, because it knows its focused pane synchronously. A terminal pane's input goes to the session API and never to Neovim's input pump. Routing by `active_win` in core would race the asynchronous `wolf_active_win` that follows a focus move.

A prefix router in core, shared by both front-ends, replaces the terminal-mode `tnoremap` chords. `<C-w>` is held, and the next key decides:

- `s` or `v` splits;
- `c` or `q` closes;
- `h`, `j`, `k`, `l` or an arrow emits a `FocusDir` intent whose `from` is the placeholder window;
- any other key sends `^W` and then that key, in order.

A named timeout of 1 s, Neovim's `timeoutlen`, flushes a held `^W`. Word-erase therefore keeps working except before those eight keys and the arrows.

Keys go through libghostty-vt's key encoder, configured from the terminal's modes: cursor-key and keypad application modes, kitty keyboard flags, and modifyOtherKeys. Core maps `KeyEvent` to the encoder, with alt as the encoder's alt. `meta` chords are front-end accelerators and never reach a terminal. If the encoder needs a field `KeyEvent` lacks, such as the physical key, g01 adds it and g02 and g03 fill it. Text from an input method or a dead key arrives as UTF-8 through `terminal_text`.

Paste goes through libghostty-vt's paste encoder, bracketed when mode 2004 is set, with no unsafe-paste confirmation. The Paste command in a terminal pane goes there, not to `nvim_paste`. Copy copies the session's selection, which libghostty-vt holds and a drag makes, not a Neovim yank.

With mouse tracking on, mouse events go through libghostty-vt's mouse encoder, and Shift bypasses tracking so that a drag still selects. With tracking off, a click focuses and a drag selects.

The scroll wheel is encoded when tracking is on. Otherwise, on the alternate screen with mode 1007, it becomes arrow keys. Otherwise it scrolls the viewport through scrollback, and any key or paste snaps the viewport back to the bottom.

`resize_terminal(id, cols, rows, cell_w_px, cell_h_px)` comes from the pane's framed content rectangle. Core resizes the libghostty-vt state first, then the pty with `TIOCSWINSZ`, which sends the shell `SIGWINCH`. `ResizeGrid` still resizes the placeholder's grid, which keeps Neovim's layout consistent but does not size the session.

`terminal_focus(id, gained)` reports pane focus changes and window activation. Core writes focus reports to the pty when mode 1004 is set.

## When Neovim dies

Sessions live in the core beside the Neovim process state, never in the snapshot. `kill_in_place` and `restart_nvim` leave them running, and the snapshot reset drops the placeholder bindings along with the windows. The crash overlay covers terminal panes while their sessions keep running and publishing. A dead Neovim sends no wipe, so a crash hangs up no session.

After a respawn, the front-end puts every live session back through `live_terminals()` and `queue_bind_terminal(id, target)`, which queues the bind on the input pump and returns `false` for an id that is not live. Each live split session goes into a new vertical split of the restored view, in id order, and then the drawer reopens through `toggle_terminal()` if the view had it open. Pane topology does not survive a restart, a limitation every pane shares.

The bind is queued rather than blocking because a blocking bind reaches Neovim ahead of the reopened files queued before it, so the next `:edit` lands in the new split and wipes its placeholder. The front-end runs the steps only once the view's first pane is bound, because both front-ends bind their first pane to the active window, and a split or drawer opened before then would become the first pane in place of the editor window. GNOME puts `live_terminals()` minus `drawer_terminal()` into its one restored tab, because a restart collapses the window to its selected tab. macOS captures each view's terminal leaves and drawer state before the restart and restores each view's own.

## The pane status bar

For a terminal pane, `pane_status` reports `filename` as the basename of the session's program, such as `zsh`, and `display_path` and `abs_path` as `""`. It reads these from the session, never from a buffer name.

## The file-tree rescan

Core replaces the `TermLeave` trigger. When a terminal pane loses focus, through `terminal_focus(id, false)`, core forces a path-less rescan through `SearchRegistry::notify_fs_changed`, with the existing debounce and equality skip. Every exit from a terminal pane passes a focus loss, as every exit from the drawer passes `TermLeave`. The `ShellCmdPost` and `FocusGained` triggers stay.

## Session restore

Shells never survive a quit, and the drawer is not persisted.

A split terminal pane persists as `PaneNode::Leaf { path: <cwd>, preview: false, terminal: true }`. The `terminal` field is serde-defaulted to `false`, and `path` holds the directory to start in, chosen by the cwd rule under [Terminal panes in the pane tree](#terminal-panes-in-the-pane-tree). Its `abs_path` is `""`, so no file path is persisted for it.

The rebuild-layout document gains `{ leaf = true, terminal = true }`, for which `init.lua` opens nothing in the window. Once `rebuild_layout` returns the window handles, core starts a session per terminal leaf and binds it into that window. The session starts in `path` when it is an existing directory, else in the window's tabpage cwd. A front-end reads the cwd it persists through `terminal_cwd(id)`.

## What goes away

g04 removes the Neovim terminal path. From `init.lua`:

- `wolf_toggle_terminal`'s `:terminal` creation and the shared `b:wolf_terminal` Neovim terminal buffer; the function becomes the drawer's placeholder helper and takes the id;
- the `tnoremap` `<C-w>` chords and the `WolfTerminalInsert` autocmd;
- `TermOpen` in the buffer tracking and `TermLeave` in the fs events;
- the `w:wolf_terminal` and `b:wolf_terminal` tags, replaced by `w:wolf_drawer` and `b:wolf_terminal_id`, which `wolf_preview_in_other`'s drawer check reads.

`init.lua` abbreviates `:terminal` and every shorter form Neovim accepts for it, `:te` through `:termina`, to open a session in the current window. Each sends the `wolf_terminal_open` notification with the window, and core starts the session in its tabpage cwd and binds it there.

From core:

- `WinBuffer.is_terminal`, replaced by `terminal`; `is_drawer` stays;
- `redraw::TERMINAL_BUFFER_PREFIX`, `is_terminal_buffer_name` and the `open_files` `term://` filter; the `init.lua` open-files emitter filters on `buftype ~= ""` instead, for the files and for the tabpage's active path;
- `pane_status`'s `term://` rule and `terminal_command_name`;
- `NavTarget::Terminal`. Every terminal pane is excluded from navigation history, as the drawer is, because a terminal is not a document and its session may have exited. A window that becomes a terminal pane loses its stack, so Back never replaces, and so wipes, a session's placeholder.

Scripted paths can still make a Neovim terminal (`termopen`, `jobstart({term=true})`, `:edit term://…`, and a scripted `:terminal` such as `vim.cmd("terminal")`, which no abbreviation reaches). It paints as an ordinary grid and gets no special treatment, a best-effort lockout like the tabpage one.

## Present limitations

Wolf's terminals have no unsafe-paste prompt, no `OSC 52` clipboard, no bell, no title display, no kitty graphics painting, no `OSC 8` link clicks, no scrollback search, and no use of `OSC 133` prompt marks.

The `xterm-ghostty` type does not travel. `ssh` to a host without the entry hands the remote programs a terminal type that host cannot look up, and `sudo` clears `TERMINFO` by default, so a program under it does not find Wolf's entry. Wolf works around neither.

## Delivery

Each delivery is self-contained and leaves the gates green on both platforms.

### g01: core sessions and the snapshot contract

- `wolf-vt` grows to the API the sessions use, with a test for each addition: styles, wide cells, per-row dirty state, cursor style and blink, colours set and read, the selection and its text, the key, mouse, paste and focus encoders, viewport scroll and the scrollbar, and the `WRITE_PTY`, pwd, title and render-hold effects.
- `wolf-core` gains a `terminal` module: the pty spawn, the registry, the reader and writer, the coalesced frame publish and wake, the prefix router, the colour sync, and survival across a Neovim restart.
- The `WolfCore` API: `open_terminal(target, command)`, `terminal_frame`, `terminal_key`, `terminal_text`, `terminal_paste`, `terminal_mouse`, `terminal_scroll`, `terminal_focus`, `resize_terminal`, `close_terminal`, `live_terminals` and `bind_terminal`.
- The `init.lua` placeholder helpers and the wipe notification; `wolf_buffers` carries the id, and `WinBuffer.terminal` exists.
- `pane_status` for terminal panes, and the rescan on focus loss.
- `wolf-core` depends on `wolf-vt`, with a repin and a row in the decisions table. `wolfGnomePackage` gets `WOLF_GHOSTTY_VT_DIR` and a run of `./tools/wolf-package-smoke.sh`. This delivery also links the crate README from `specs/architecture.md`.
- Tests with a real Neovim and a real `/bin/sh` in a pty: an echo round trip, `stty size` after a resize, the shell's exit closing its pane, each router chord, bracketed paste, row versions, survival across `restart_nvim`, the rescan on focus loss, and the `win_buffers` binding.
- The Neovim terminal path stays untouched.

### g02: macOS rendering and input

- The `wolf-ffi` bridge for the session API and the frame handle, with regenerated bindings.
- Terminal-pane painting with CoreText: cells, cursor and selection, with damage by row version.
- Input routing for keys, input methods, paste, copy, mouse, scroll and focus; session sizing; the status bar.
- Tests: `wolf-ffi` tests, a `WolfMacOSTests` offscreen render, and `WolfKeyMap` seam tests.

### g03: Linux rendering and input

- The same for `wolf-gnome`, painting with Cairo and Pango, including `im_commit`.
- Tests: the pure seams, and a cage smoke that types into a terminal pane and reads its output back by `OCR`.

### g04: the switch and the removal

- `ToggleTerminal` and the `:terminal` abbreviations open sessions.
- Everything under [What goes away](#what-goes-away) is removed.
- Terminal leaves restore on both front-ends, and the `crates/wolf-ffi/tests/terminal.rs` family follows the switch.
- The docs: the architecture contract, the Terminal chain in `specs/command-lifecycle.md`, `runtime/nvim/README.md`, the crate READMEs, the Terminal section of `notes/feat-list.md`, and the decisions notes.
