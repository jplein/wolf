//! Integration tests of `WOLF_TAB_ID` in a terminal session's environment, from
//! `specs/cli-and-file-open.md` §3 and `specs/terminal-panes.md`, "Terminal sessions": a core
//! given a tab ID by `WolfCore::set_tab_id` passes it to every session it starts, pane and drawer
//! alike, and across a Neovim restart; a core with none removes the variable. A real `/bin/sh`
//! runs in a real pty beside a real, hermetic Neovim, and the shell itself prints what it sees.
//!
//! ## Premises about the components
//!
//! - A session's shell inherits the app's environment, so the decoy set before any core starts
//!   reaches it unless core sets or removes the variable, as `terminal_terminfo.rs` measures for
//!   `TERM`.
//! - `/bin/sh` on a tty echoes what it is typed, so the drawer's typed line spells the marker
//!   in two quoted halves and only the shell's output holds it whole.
//! - `lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)` kills the captive Neovim, which the
//!   core reports as `Liveness::Dead`, as `terminal_lifecycle.rs` does.

mod common;

use std::sync::Once;
use std::time::Duration;

use common::{
    frame_text, fresh_cwd, open_session, pin_default_shell, poll_until, run_line, send_ex,
    settle_for, sh_script, start_core_at, wait_for_text, FlushWaiter,
};
use wolf_core::{Liveness, TabId, TerminalId, TerminalTarget, WolfCore};

/// A `WOLF_TAB_ID` the app inherited: a well-formed UUID no core is given, so a shell showing it
/// inherited the test's value.
const DECOY_TAB_ID: &str = "0badc0de-0000-4000-8000-00000000dec0";

/// The marker the shell prints after the value, once the whole line is on the row.
const DONE: &str = "|done";

/// The script a pane session runs: the value in brackets, so an empty one shows as `[]`.
const PRINT_TAB_ID: &str = "printf 'tab=[%s]|done\\n' \"$WOLF_TAB_ID\"; exec sleep 1000";

/// The line typed into the drawer's shell, which prints [`DONE`] only in its output.
const TYPED_PRINT_TAB_ID: &str = "printf 'tab=[%s]|%s\\n' \"$WOLF_TAB_ID\" 'do''ne'";

/// How long after a restart before the fresh Neovim's windows are used.
const RESTART_SETTLE: Duration = Duration::from_secs(1);

/// Sets the decoy once per binary, before any core starts, so no `setenv` races a `getenv` on
/// a neighbouring test's thread.
fn set_decoy() {
    static DECOY: Once = Once::new();
    DECOY.call_once(|| std::env::set_var("WOLF_TAB_ID", DECOY_TAB_ID));
}

fn start_core() -> (WolfCore, FlushWaiter) {
    set_decoy();
    pin_default_shell();
    let cwd = std::fs::canonicalize(fresh_cwd()).expect("failed to canonicalize the cwd");
    start_core_at(cwd)
}

/// Waits for the session to print its line, and asserts it shows `expected` as the value.
fn assert_tab_id(
    core: &WolfCore,
    waiter: &FlushWaiter,
    id: TerminalId,
    expected: &str,
    what: &str,
) {
    let frame = wait_for_text(core, waiter, id, DONE);
    let text = frame_text(&frame);
    assert!(
        text.contains(&format!("tab=[{expected}]{DONE}")),
        "{what} must see WOLF_TAB_ID={expected:?}. tab=[{DECOY_TAB_ID}] means the app's inherited \
         value reached it, tab=[] that the variable was removed or never set, another id that a \
         stale one was used. The frame reads:\n{text}"
    );
}

#[test]
fn a_pane_session_gets_its_cores_tab_id() {
    let (core, waiter) = start_core();
    let tab = TabId::random();
    core.set_tab_id(tab);
    let id = open_session(&core, TerminalTarget::Window(0), sh_script(PRINT_TAB_ID));
    assert_tab_id(
        &core,
        &waiter,
        id,
        &tab.to_string(),
        "a pane session of a core given an id",
    );
    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn the_drawer_session_gets_its_cores_tab_id() {
    let (core, waiter) = start_core();
    let tab = TabId::random();
    core.set_tab_id(tab);
    core.toggle_terminal();
    let id = poll_until(
        &waiter,
        "the drawer's session to start",
        || core.drawer_terminal(),
        Option::is_some,
    )
    .expect("poll_until returned only once the drawer had a session");
    run_line(&core, id, TYPED_PRINT_TAB_ID);
    assert_tab_id(
        &core,
        &waiter,
        id,
        &tab.to_string(),
        "the drawer of a core given an id",
    );
    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn a_core_without_a_tab_id_removes_an_inherited_one() {
    let (core, waiter) = start_core();
    let id = open_session(&core, TerminalTarget::Window(0), sh_script(PRINT_TAB_ID));
    assert_tab_id(&core, &waiter, id, "", "a session of a core given no id");
    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn the_tab_id_survives_a_neovim_restart() {
    let (core, waiter) = start_core();
    let tab = TabId::random();
    core.set_tab_id(tab);

    send_ex(&core, "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)");
    poll_until(
        &waiter,
        "the core to report Neovim dead after its SIGKILL",
        || core.liveness(),
        |liveness| *liveness == Liveness::Dead,
    );
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
    settle_for(&waiter, RESTART_SETTLE);

    let id = open_session(&core, TerminalTarget::Window(0), sh_script(PRINT_TAB_ID));
    assert_tab_id(
        &core,
        &waiter,
        id,
        &tab.to_string(),
        "a session started after a Neovim restart",
    );
    core.shutdown().expect("shutdown should be Ok");
}
