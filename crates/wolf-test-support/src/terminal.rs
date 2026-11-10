//! Terminal-session helpers: the shell pin, opening a session, and reading its frame.

use std::path::PathBuf;
use std::sync::{Arc, Once};
use std::time::Instant;

use wolf_core::{TerminalCommand, TerminalFrame, TerminalId, TerminalTarget, WolfCore};

use crate::{FlushWaiter, FLUSH_TIMEOUT, POLL_INTERVAL};

/// Sets `SHELL=/bin/sh` for the process, once, so a session started with no explicit command
/// (the drawer, typed `:terminal`, a restored terminal leaf) runs POSIX `sh`, whose output the
/// tests type for. Call it before starting a core, so no session can read `SHELL` first.
pub fn pin_default_shell() {
    static PIN: Once = Once::new();
    PIN.call_once(|| std::env::set_var("SHELL", "/bin/sh"));
}

/// `/bin/sh -c <script>`, run as a session's program.
pub fn sh_script(script: &str) -> TerminalCommand {
    TerminalCommand {
        program: PathBuf::from("/bin/sh"),
        args: vec!["-c".to_owned(), script.to_owned()],
    }
}

/// Opens a session through `open_terminal`, failing when it returns `None`.
pub fn open_session(
    core: &WolfCore,
    target: TerminalTarget,
    command: TerminalCommand,
) -> TerminalId {
    let shown = format!(
        "{target:?}, {} {:?}",
        command.program.display(),
        command.args
    );
    core.open_terminal(target, Some(command))
        .unwrap_or_else(|| {
            panic!(
            "open_terminal({shown}) returned None: the pty or the shell could not be started, or \
             the placeholder could not be bound."
        )
        })
}

/// The session's current frame, failing when `terminal_frame` returns `None`.
pub fn frame_of(core: &WolfCore, id: TerminalId) -> Arc<TerminalFrame> {
    core.terminal_frame(id).unwrap_or_else(|| {
        panic!("terminal_frame({id:?}) is None, so the session is not in the registry.")
    })
}

/// Every row's text, joined with newlines.
pub fn frame_text(frame: &TerminalFrame) -> String {
    (0..frame.rows)
        .map(|row| frame.row_text(row))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Polls the session's frame until `done` accepts it, draining `waiter`, and returns it. Panics
/// with `desc` and the frame's text after [`FLUSH_TIMEOUT`], or when the session is gone.
pub fn poll_frame(
    core: &WolfCore,
    waiter: &FlushWaiter,
    id: TerminalId,
    desc: &str,
    done: impl Fn(&TerminalFrame) -> bool,
) -> Arc<TerminalFrame> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let frame = core
            .terminal_frame(id)
            .unwrap_or_else(|| panic!("session {id:?} left the registry while waiting for {desc}"));
        if done(&frame) {
            return frame;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for {desc}; session {id:?}'s frame \
                 (version {}, {}x{}, exited {:?}) reads:\n{}",
                frame.version,
                frame.cols,
                frame.rows,
                frame.exited,
                frame_text(&frame)
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Polls until the session's frame shows `needle` anywhere, and returns that frame.
pub fn wait_for_text(
    core: &WolfCore,
    waiter: &FlushWaiter,
    id: TerminalId,
    needle: &str,
) -> Arc<TerminalFrame> {
    poll_frame(
        core,
        waiter,
        id,
        &format!("the session's frame to show {needle:?}"),
        |frame| frame_text(frame).contains(needle),
    )
}
