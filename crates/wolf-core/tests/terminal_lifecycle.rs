//! Integration tests of a terminal session's lifetime in `wolf-core`: the shell exiting, its
//! placeholder window closing from Neovim, `close_terminal`, `shutdown`, and a session
//! surviving a Neovim crash and restart. A real `/bin/sh` runs in a real pty beside a real,
//! hermetic Neovim that holds the placeholder windows.
//!
//! Shells are checked for by pid with `kill -0`, which succeeds on a zombie, so a pid that is
//! gone was both killed and reaped.
//!
//! ## Premises about the components
//!
//! Each is measured by these tests against the real component, not assumed:
//!
//! - `lua (vim.uv or vim.loop).sleep(3000)` blocks Neovim's event loop for 3 s, so an RPC core
//!   sends Neovim in that time waits; anything that changes during it came from core alone.
//! - `trap '' HUP` makes a shell ignore `SIGHUP`, and the ignored disposition survives `exec`,
//!   so only `SIGKILL` ends the `sleep` that replaces it.
//! - `lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)` kills the captive Neovim, which the
//!   core reports as `Liveness::Dead`, as `nvim_restart.rs` does.
//! - `:close` is `init.lua`'s abbreviation for `wolf_do_close()`, which closes the current
//!   window unless it is its tabpage's last.

mod common;

use std::time::{Duration, Instant};

use common::{
    active_window, frame_of, interactive_sh, open_session, pid_alive, poll_frame, poll_until,
    run_line, send_ex, sh_script, start_core_at, term_key, terminal_windows, wait_for_pid,
    wait_for_pid_gone, wait_for_text, win_terminal, FlushWaiter,
};
use wolf_core::{Key, Liveness, SplitOrientation, TerminalId, TerminalTarget, WolfCore};

/// How long the exit case keeps Neovim's event loop blocked.
const WEDGE: Duration = Duration::from_secs(3);

/// How long after the wedge command is typed before the shell is told to exit, so Neovim is
/// inside the sleep when it does.
const WEDGE_SETTLE: Duration = Duration::from_millis(300);

/// How long after a restart before the snapshot's bindings are read.
const RESTART_SETTLE: Duration = Duration::from_secs(1);

/// A vertical split of Neovim's current window.
const SPLIT_CURRENT: TerminalTarget = TerminalTarget::Split {
    win: 0,
    orientation: SplitOrientation::Vertical,
};

/// Starts a core in a fresh canonical cwd.
fn start_core() -> (WolfCore, FlushWaiter) {
    let cwd = std::fs::canonicalize(common::fresh_cwd()).expect("failed to canonicalize the cwd");
    start_core_at(cwd)
}

/// Opens `command` in a vertical split of the current window and returns the session with
/// the split's window, once that window is bound and active.
fn open_in_split(
    core: &WolfCore,
    waiter: &FlushWaiter,
    command: wolf_core::TerminalCommand,
) -> (TerminalId, i64) {
    let id = open_session(core, SPLIT_CURRENT, command);
    let win = poll_until(
        waiter,
        "the split placeholder to be bound and active",
        || {
            let active = core.read_snapshot().active_win;
            active.filter(|win| win_terminal(core, *win) == Some(id))
        },
        Option::is_some,
    )
    .expect("poll_until returned only once the placeholder was active");
    (id, win)
}

/// The shell's exit is published at once, Neovim or no Neovim, and then closes its pane: the
/// placeholder window goes, the other window stays, and the session leaves the registry.
#[test]
fn the_shell_exiting_is_reported_and_closes_its_pane() {
    let (core, waiter) = start_core();
    let other = active_window(&core, &waiter);
    let (id, win) = open_in_split(
        &core,
        &waiter,
        sh_script("stty -echo; printf ready; read x; exit 7"),
    );
    wait_for_text(&core, &waiter, id, "ready");

    send_ex(
        &core,
        &format!("lua (vim.uv or vim.loop).sleep({})", WEDGE.as_millis()),
    );
    let wedged = Instant::now();
    std::thread::sleep(WEDGE_SETTLE);
    core.terminal_key(id, term_key(Key::Enter));
    let frame = poll_frame(
        &core,
        &waiter,
        id,
        "the frame to report the exit",
        |frame| frame.exited.is_some(),
    );
    let seen = wedged.elapsed();
    assert_eq!(
        frame.exited,
        Some(7),
        "a shell that ran `exit 7` must be reported as exited with 7."
    );
    assert!(
        seen < WEDGE,
        "the exit must be published while Neovim is still blocked ({WEDGE:?}), but it took \
         {seen:?}: the exit is reported only after a Neovim round trip."
    );

    poll_until(
        &waiter,
        "the exited session's window to close and the session to leave the registry",
        || {
            let snap = core.read_snapshot();
            (
                snap.win_buffers.contains_key(&win),
                snap.win_buffers.contains_key(&other),
                core.live_terminals().contains(&id),
                core.terminal_frame(id).is_some(),
            )
        },
        |state| *state == (false, true, false, false),
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A shell exiting in its tabpage's last window leaves that window open with an empty,
/// unnamed buffer, and the session leaves the registry.
#[test]
fn a_shell_exiting_in_the_last_window_leaves_an_empty_buffer() {
    let (core, waiter) = start_core();
    let win = active_window(&core, &waiter);
    let id = open_session(
        &core,
        TerminalTarget::Window(0),
        sh_script("stty -echo; printf ready; read x; exit 0"),
    );
    let placeholder = core
        .read_snapshot()
        .win_buffers
        .get(&win)
        .map(|buffer| buffer.bufnr)
        .unwrap_or_else(|| panic!("window {win} has no win_buffers entry after open_terminal"));
    wait_for_text(&core, &waiter, id, "ready");

    core.terminal_key(id, term_key(Key::Enter));
    let after = poll_until(
        &waiter,
        "the last window to show another buffer once the shell exits; the placeholder staying \
         means the exit leaves a tabpage's last window alone",
        || core.read_snapshot().win_buffers.get(&win).cloned(),
        |buffer| {
            buffer
                .as_ref()
                .is_some_and(|buffer| buffer.bufnr != placeholder && buffer.terminal.is_none())
        },
    )
    .expect("poll_until returned only once the window had another buffer");
    assert_eq!(
        after.path, None,
        "the buffer that replaces the placeholder must be empty and unnamed."
    );
    poll_until(
        &waiter,
        "the exited session to leave the registry",
        || core.terminal_frame(id).is_none(),
        |gone| *gone,
    );
    assert!(
        !core.live_terminals().contains(&id),
        "an exited session must not be listed as live."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Closing the placeholder window from Neovim wipes its buffer, which hangs the session up:
/// it leaves the registry and its shell is gone.
#[test]
fn closing_the_placeholder_from_neovim_hangs_the_session_up() {
    let (core, waiter) = start_core();
    let (id, win) = open_in_split(&core, &waiter, sh_script("echo PID=$$; exec sleep 1000"));
    let pid = wait_for_pid(&core, &waiter, id);

    send_ex(&core, "close");
    poll_until(
        &waiter,
        "`:close` to close the placeholder window",
        || core.read_snapshot().win_buffers.contains_key(&win),
        |present| !*present,
    );
    poll_until(
        &waiter,
        "the session to leave the registry once its placeholder is wiped; staying means the \
         wipe is never reported or the registry ignores it",
        || core.terminal_frame(id).is_some(),
        |present| !*present,
    );
    wait_for_pid_gone(pid, "the shell of a session whose placeholder was closed");

    core.shutdown().expect("shutdown should be Ok");
}

/// `close_terminal` hangs the session up: its shell is gone and its pane closes.
#[test]
fn close_terminal_hangs_up_and_closes_the_pane() {
    let (core, waiter) = start_core();
    let (id, win) = open_in_split(&core, &waiter, sh_script("echo PID=$$; exec sleep 1000"));
    let pid = wait_for_pid(&core, &waiter, id);

    core.close_terminal(id);
    wait_for_pid_gone(pid, "the shell of a session close_terminal was called on");
    poll_until(
        &waiter,
        "the closed session's window to go and the session to leave the registry",
        || {
            (
                core.read_snapshot().win_buffers.contains_key(&win),
                core.terminal_frame(id).is_some(),
            )
        },
        |state| *state == (false, false),
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `shutdown` returns only once every shell is gone, one that ignores `SIGHUP` included.
#[test]
fn shutdown_reaps_every_shell() {
    let (core, waiter) = start_core();
    let stubborn = open_session(
        &core,
        TerminalTarget::Window(0),
        sh_script("trap '' HUP; echo PID=$$; exec sleep 1000"),
    );
    let plain = open_session(
        &core,
        SPLIT_CURRENT,
        sh_script("echo PID=$$; exec sleep 1000"),
    );
    let stubborn_pid = wait_for_pid(&core, &waiter, stubborn);
    let plain_pid = wait_for_pid(&core, &waiter, plain);

    core.shutdown().expect("shutdown should be Ok");
    assert!(
        !pid_alive(plain_pid),
        "the plain shell (pid {plain_pid}) must be gone once shutdown returns: alive or a zombie \
         means shutdown returns before the shells are hung up and reaped."
    );
    assert!(
        !pid_alive(stubborn_pid),
        "the shell that ignores SIGHUP (pid {stubborn_pid}) must be gone once shutdown returns: \
         still there means no SIGKILL follows the grace, or shutdown does not wait to reap it."
    );
}

/// A session keeps running while Neovim is dead and across `restart_nvim`, loses only its
/// binding, and is shown again by `bind_terminal`; an unknown id binds nothing.
#[test]
fn a_session_survives_a_neovim_restart() {
    let (core, waiter) = start_core();
    let id = open_session(&core, TerminalTarget::Window(0), interactive_sh());
    run_line(&core, id, "echo before-$((1+2))");
    wait_for_text(&core, &waiter, id, "before-3");

    send_ex(&core, "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)");
    poll_until(
        &waiter,
        "the core to report Neovim dead after its SIGKILL",
        || core.liveness(),
        |liveness| *liveness == Liveness::Dead,
    );

    run_line(&core, id, "echo dead-$((2+3))");
    wait_for_text(&core, &waiter, id, "dead-5");

    assert!(
        matches!(core.restart_nvim(), Ok(true)),
        "restart_nvim on a dead Neovim must respawn it and return Ok(true)."
    );
    poll_until(
        &waiter,
        "the fresh Neovim to report its windows",
        || core.read_snapshot().win_buffers.len(),
        |windows| *windows > 0,
    );
    common::settle_for(&waiter, RESTART_SETTLE);
    assert_eq!(
        core.live_terminals(),
        vec![id],
        "the session must survive the restart; gone means restart_nvim hangs sessions up."
    );
    assert_eq!(
        frame_of(&core, id).exited,
        None,
        "the shell must still be running after the restart."
    );
    assert_eq!(
        terminal_windows(&core),
        Vec::new(),
        "the fresh Neovim's windows must carry no session before one is bound."
    );

    assert!(
        core.bind_terminal(id, TerminalTarget::Window(0)),
        "bind_terminal of a live session into the current window must return true."
    );
    let win = active_window(&core, &waiter);
    assert_eq!(
        win_terminal(&core, win),
        Some(id),
        "after bind_terminal returns, the current window {win} must carry the session."
    );

    run_line(&core, id, "echo after-$((3+4))");
    wait_for_text(&core, &waiter, id, "after-7");

    assert!(
        !core.bind_terminal(TerminalId(999), TerminalTarget::Window(0)),
        "bind_terminal of an id no session has must return false."
    );

    core.shutdown().expect("shutdown should be Ok");
}
