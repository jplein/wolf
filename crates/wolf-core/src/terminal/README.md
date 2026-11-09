# wolf-core/src/terminal

The submodules of [`terminal.rs`](../README.md#terminalrs), which holds the public types and the registry. [Terminal panes](../../../../specs/terminal-panes.md) is authoritative for what a session does, and the threading and ownership decisions are in [wolf-core-decisions.md](../../../../notes/wolf-core-decisions.md#terminal-sessions).

## Files

#### `pty.rs`

Purpose: every `libc` call a session makes. It opens the pty pair, spawns the program as the leader of a new session whose controlling terminal is the pty, sets the window size, signals and reaps the process group, resolves the default command, and reads the shell's working directory.

**Decisions**

- Both pty ends are close-on-exec. A copy inherited by another child of the app, such as Neovim or a later session's shell, would keep the pty open after a hang-up closes core's copies.
- The `pre_exec` hook calls only `setsid` and `ioctl(TIOCSCTTY)`, because it runs in the forked child, where only async-signal-safe calls are sound.
- The shell leads its own session, so its process group is its pid, and a signal to `-pid` reaches every job it started.
- `decode_pwd` takes a `file://` URI, dropping the host and percent-decoding the path, or a bare absolute path. Anything else names no directory.

**Gotchas**

- `openpty`'s `winp` is `*const` on Linux and `*mut` on macOS, and `TIOCSCTTY`'s type differs too, so a change here needs a clippy run on both platforms.

#### `router.rs`

Purpose: the `<C-w>` prefix router a session's keys pass through first. It is a pure state machine over the held prefix, and returns what a key does — hold, send, split, close, focus or drop — for the session and the registry to carry out.

**Decisions**

- The clock is passed in (`route(key, now)`, `flush_due(now)`), so the router owns no timer: the session's reader thread sleeps until `deadline()` and sends the held `^W` itself.
- A chord is a plain `s v c q h j k l` or an unshifted arrow. Any other key, a second `<C-w>` included, sends `^W` and then that key, so `<C-w><C-w>` reaches the shell as two word-erases.
- Text and a paste release a held prefix first (`take_held`), so the `^W` reaches the program ahead of them.

#### `session.rs`

Purpose: one session: the emulator state under a `parking_lot::Mutex`, the published frame, the reader thread `wolf-term-<id>`, the writer thread `wolf-term-<id>-w`, and the input paths the session API reaches through the registry.

**Decisions**

- The reader thread does everything but the writes. It reads and feeds the pty, handles the effects, publishes, runs the render-hold and prefix deadlines, detects the exit and hangs up. Any other thread changes state under the lock and writes a byte to a self-pipe the reader polls beside the master.
- Every pty write goes through the writer's channel, the emulator's `WRITE_PTY` replies included, so a full pty cannot stall the reader or a caller.
- The reader's sleep is capped at `EXIT_POLL_CAP`, because a background job holding the pty open hides the shell's exit from the read side, and `try_wait` is what notices it.
- The master copy `resize` uses is closed under the lock `resize` takes, so `TIOCSWINSZ` never meets a closed or reused descriptor.
- Frames are built on the reader thread alone, apart from the first at spawn, because `wolf_vt::Terminal::screen` consumes the dirty state the row versions come from.
