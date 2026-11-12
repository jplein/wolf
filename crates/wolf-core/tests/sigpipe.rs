//! Integration test for a host that keeps `SIGPIPE` at its default action, as the Swift app and
//! its XCTest runner do: input after the captive Neovim died must not kill the host, which must
//! see the death as [`Liveness::Dead`] and restart Neovim.
//!
//! Drives a real, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by `common::require_nvim_bin`).
//!
//! ## Why this lives in its own binary
//!
//! The test sets the process-wide `SIGPIPE` disposition back to `SIG_DFL` before any core
//! starts. libtest runs a binary's tests on parallel threads of one process, so it must be the
//! binary's only test: no other test may write a pipe while the disposition is the default.
//!
//! ## Premises about the components
//!
//! - Rust's runtime sets `SIGPIPE` to `SIG_IGN` before `main`, libtest's included, so a Rust
//!   test binary gets `EPIPE` from a write to a closed pipe by itself. Resetting it to `SIG_DFL`
//!   recreates a Swift host, which keeps the default.
//! - nvim-rs writes each call straight to Neovim's stdin, so a call made after Neovim died
//!   writes to a pipe whose read end is closed: the input pump's batch for `send_input`, and
//!   the request of a blocking query such as `current_tabpage`.
//! - A SIGKILL of Neovim's own pid is a crash, not a clean exit: the supervisor sees stdout EOF
//!   without a shutdown in progress and marks `Dead`, as `nvim_liveness.rs` measures.
//!
//! ## The red
//!
//! No assertion can report it. Without `WolfCore::start` ignoring `SIGPIPE`, the first write to
//! the dead Neovim's stdin kills the test binary with signal 13 (`SIGPIPE`) at the input after
//! the crash, and the log stops there with no assertion message.

mod common;

use common::{ch, poll_until, send_ex};
use wolf_core::Liveness;

/// A host with `SIGPIPE` at its default survives input sent after Neovim crashed, still reports
/// the crash as `Dead`, and restarts Neovim.
#[test]
fn input_after_a_crash_does_not_kill_a_host_with_default_sigpipe() {
    // SAFETY: setting a disposition to `SIG_DFL` installs no handler, and no other thread runs.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };

    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "a freshly started core must report Alive; anything else means the start itself failed, \
         and the crash below proves nothing"
    );

    send_ex(&core, "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)");
    poll_until(
        &waiter,
        "liveness() to become Dead after Neovim's SIGKILL",
        || core.liveness(),
        |l| *l == Liveness::Dead,
    );

    // Both writes reach the dead Neovim's stdin. A host killed by SIGPIPE ends here.
    core.send_input(ch('x'));
    assert_eq!(
        core.current_tabpage(),
        0,
        "current_tabpage() on a dead Neovim must fail and return 0; a nonzero handle means a \
         Neovim answered, so the crash did not kill the Neovim this core writes to and the \
         writes above never met a closed pipe"
    );
    assert_eq!(
        core.liveness(),
        Liveness::Dead,
        "after input to a dead Neovim the core must still report Dead; Alive or NotResponding \
         means a failed write was read as a live or wedged Neovim, so the front-end would not \
         offer a restart"
    );

    assert!(
        matches!(core.restart_nvim(), Ok(true)),
        "restart_nvim() must respawn the dead Neovim and return Ok(true); Ok(false) means a \
         concurrent restart was already running, and Err means the respawn failed"
    );
    assert_ne!(
        core.current_tabpage(),
        0,
        "after the restart current_tabpage() must answer with a tabpage; 0 means the RPC still \
         failed, so the restart left the core writing to the dead pipe"
    );
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "after the restart the core must report Alive; Dead means the restart did not reset \
         liveness, NotResponding that the fresh Neovim did not answer in time"
    );

    core.shutdown().expect("shutdown should be Ok");
}
