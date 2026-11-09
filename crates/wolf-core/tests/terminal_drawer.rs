//! Integration tests of the terminal drawer in `wolf-core`: `toggle_terminal` opening, hiding
//! and reopening a drawer window on one core session, the shell's exit closing it, the router
//! ignoring split chords inside it, and the session surviving a Neovim restart. A real shell runs
//! in a real pty beside a real, hermetic Neovim that holds the drawer window.
//!
//! The drawer runs the user's default shell, so every case pins `SHELL=/bin/sh` first
//! ([`common::pin_default_shell`]).
//!
//! ## Premises about the components
//!
//! Each is measured by these tests against the real component, not assumed:
//!
//! - `/bin/sh` on a tty is interactive and echoes what it is typed, so a test looks for the
//!   output of an expression (`$((2+5))`) that the typed text does not contain. Started as a
//!   login shell (`-l`, macOS) it is still interactive.
//! - `getbufinfo()` lists every buffer, unlisted and hidden ones included, with its `b:`
//!   variables under `variables`, so it finds a hidden drawer placeholder that no window shows.
//! - `lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)` kills the captive Neovim, which the
//!   core reports as `Liveness::Dead`, as `terminal_lifecycle.rs` does. The drawer placeholder
//!   is `nofile` and not in terminal mode, so a typed `:` reaches Neovim's command line.
//! - Paths are compared with `pwd -P`, since macOS temp dirs sit behind the `/var` →
//!   `/private/var` link.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::{
    frame_of, frame_text, poll_frame, poll_until, probe_values, run_line, send_ex, settle_for,
    start_core_at, term_ctrl, term_key, terminal_windows, wait_for_text, FlushWaiter,
    FLUSH_TIMEOUT, POLL_INTERVAL,
};
use wolf_core::{Key, LayoutIntent, Liveness, TerminalId, WolfCore};

/// How long a chord that must do nothing is given to do something anyway.
const NEGATIVE_SETTLE: Duration = Duration::from_millis(1500);

/// How long after a restart before the snapshot's bindings are read.
const RESTART_SETTLE: Duration = Duration::from_secs(1);

/// Starts a core running `/bin/sh` as its default shell, in a fresh canonical cwd.
fn start_core() -> (WolfCore, FlushWaiter, PathBuf) {
    common::pin_default_shell();
    let cwd = std::fs::canonicalize(common::fresh_cwd()).expect("failed to canonicalize the cwd");
    let (core, waiter) = start_core_at(cwd.clone());
    (core, waiter, cwd)
}

/// The window flagged `is_drawer` in `win_buffers`, with the session it shows.
fn drawer_window(core: &WolfCore) -> Option<(i64, Option<TerminalId>)> {
    core.read_snapshot()
        .win_buffers
        .iter()
        .find(|(_, buffer)| buffer.is_drawer)
        .map(|(win, buffer)| (*win, buffer.terminal))
}

/// Toggles the drawer open and waits for a window flagged `is_drawer` that carries a session,
/// returning the window and the session.
fn open_drawer(core: &WolfCore, waiter: &FlushWaiter, desc: &str) -> (i64, TerminalId) {
    core.toggle_terminal();
    let (win, id) = poll_until(
        waiter,
        desc,
        || drawer_window(core),
        |drawer| matches!(drawer, Some((_, Some(_)))),
    )
    .expect("poll_until returned only once a drawer window existed");
    (
        win,
        id.expect("poll_until returned only once the drawer window carried a session"),
    )
}

/// Toggles the drawer closed and waits for no window to be flagged `is_drawer`.
fn hide_drawer(core: &WolfCore, waiter: &FlushWaiter, desc: &str) {
    core.toggle_terminal();
    poll_until(waiter, desc, || drawer_window(core), Option::is_none);
}

/// Drains layout intents until `pick` accepts one, returning what it picked. Panics with
/// `desc` and every intent seen after [`FLUSH_TIMEOUT`].
fn wait_for_intent<T>(core: &WolfCore, desc: &str, pick: impl Fn(&LayoutIntent) -> Option<T>) -> T {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut seen = Vec::new();
    loop {
        for intent in core.drain_intents() {
            if let Some(picked) = pick(&intent) {
                return picked;
            }
            seen.push(intent);
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for {desc}; intents seen: {seen:?}");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Sends `<C-w>` then `key` to the session.
fn chord(core: &WolfCore, id: TerminalId, key: Key) {
    core.terminal_key(id, term_ctrl(Key::Char('w')));
    core.terminal_key(id, term_key(key));
}

/// Asserts the session's shell runs in `expected`, a canonical directory. The shell prints
/// `in-6` or `out-8`, which the typed line does not contain; on a mismatch its `pwd -P` follows.
fn assert_session_cwd(core: &WolfCore, waiter: &FlushWaiter, id: TerminalId, expected: &Path) {
    run_line(
        core,
        id,
        &format!(
            "if [ \"$(pwd -P)\" = '{}' ]; then echo in-$((2*3)); else echo out-$((2*4)); pwd -P; fi",
            expected.display()
        ),
    );
    let frame = poll_frame(
        core,
        waiter,
        id,
        "the drawer shell to print in-6 or out-8",
        |frame| {
            let text = frame_text(frame);
            text.contains("in-6") || text.contains("out-8")
        },
    );
    assert!(
        frame_text(&frame).contains("in-6"),
        "the drawer's shell must start in the current tabpage's cwd {}; out-8 means it started \
         elsewhere (its pwd -P follows). The frame reads:\n{}",
        expected.display(),
        frame_text(&frame)
    );
}

/// The number of buffers, hidden ones included, whose `b:wolf_terminal_id` is `id`.
fn placeholder_count(core: &WolfCore, cwd: &Path, id: TerminalId) -> String {
    let expr = format!(
        "len(filter(getbufinfo(), {{_, b -> get(b.variables, 'wolf_terminal_id', 0) == {}}}))",
        id.0
    );
    probe_values(core, &[expr.as_str()], &cwd.join("placeholder-count.txt"))
        .pop()
        .expect("probe_values returns one value per expression")
}

/// `toggle_terminal` opens the drawer on a core session: a window flagged `is_drawer` carries
/// the session `drawer_terminal` names, a `DrawerOpen` intent names that window, the
/// placeholder is a hidden-on-close, full-width `nofile` buffer, and the shell runs in the
/// tabpage's cwd and answers a typed line.
#[test]
fn toggle_opens_the_drawer_on_a_core_session() {
    let (core, waiter, cwd) = start_core();
    let _ = core.drain_intents();

    let (win, id) = open_drawer(
        &core,
        &waiter,
        "a drawer window carrying a session after toggle_terminal; a drawer window with no \
         session means toggle_terminal queued the toggle without starting one, and no drawer \
         window at all means w:wolf_drawer is never set",
    );
    assert_eq!(
        core.drawer_terminal(),
        Some(id),
        "drawer_terminal() must name the session the drawer window {win} shows."
    );
    assert_eq!(
        core.live_terminals(),
        vec![id],
        "opening the drawer must start exactly one session."
    );
    let opened = wait_for_intent(&core, "a DrawerOpen intent", |intent| match intent {
        LayoutIntent::DrawerOpen { win } => Some(*win),
        _ => None,
    });
    assert_eq!(
        opened, win,
        "the DrawerOpen intent must name the drawer window {win}, not {opened}."
    );

    let buf = format!("winbufnr({win})");
    let exprs = [
        format!("getbufvar({buf}, '&buftype')"),
        format!("getbufvar({buf}, '&bufhidden')"),
        format!("getbufvar({buf}, 'wolf_full_width') ? 1 : 0"),
    ];
    let exprs: Vec<&str> = exprs.iter().map(String::as_str).collect();
    let values = probe_values(&core, &exprs, &cwd.join("drawer-probe.txt"));
    assert_eq!(
        values,
        vec!["nofile".to_owned(), "hide".to_owned(), "1".to_owned()],
        "the drawer placeholder must read (buftype, bufhidden, b:wolf_full_width) as (nofile, \
         hide, 1). bufhidden wipe hangs the drawer's session up whenever it is hidden; buftype \
         terminal means Neovim's own :terminal made the drawer; 0 means it does not default to \
         full width."
    );

    assert_session_cwd(&core, &waiter, id, &cwd);
    run_line(&core, id, "echo drawer-$((2+5))");
    wait_for_text(&core, &waiter, id, "drawer-7");

    core.shutdown().expect("shutdown should be Ok");
}

/// Hiding the drawer keeps its session running, and the next toggle shows the same session,
/// with its output, again.
#[test]
fn hiding_keeps_the_session_and_reopening_reuses_it() {
    let (core, waiter, _cwd) = start_core();
    let (win, id) = open_drawer(&core, &waiter, "the drawer to open on a session");
    run_line(&core, id, "echo kept-$((3+4))");
    wait_for_text(&core, &waiter, id, "kept-7");

    hide_drawer(
        &core,
        &waiter,
        "the drawer window to close on the second toggle",
    );
    assert!(
        !core.read_snapshot().win_buffers.contains_key(&win),
        "the drawer window {win} must be gone once hidden."
    );
    settle_for(&waiter, RESTART_SETTLE);
    assert_eq!(
        core.live_terminals(),
        vec![id],
        "hiding the drawer must keep its session; gone means the placeholder was wiped with its \
         window (bufhidden=wipe), which hangs the session up."
    );

    let (_reopened, again) = open_drawer(&core, &waiter, "the drawer to reopen");
    assert_eq!(
        again, id,
        "the reopened drawer must show the same session {id:?}, not {again:?}: a new id means \
         toggle_terminal starts a shell even when the drawer's session is live."
    );
    assert_eq!(
        core.live_terminals(),
        vec![id],
        "reopening the drawer must start no second session."
    );
    let frame = frame_of(&core, id);
    assert!(
        frame_text(&frame).contains("kept-7"),
        "the reopened drawer's session must still show its earlier output; the frame reads:\n{}",
        frame_text(&frame)
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The drawer shell exiting closes the drawer and leaves no hidden placeholder behind, and the
/// next toggle starts a new shell.
#[test]
fn the_drawer_shell_exiting_closes_it_and_the_next_toggle_starts_anew() {
    let (core, waiter, cwd) = start_core();
    let (_win, id) = open_drawer(&core, &waiter, "the drawer to open on a session");

    run_line(&core, id, "exit");
    poll_until(
        &waiter,
        "the drawer window to close and its session to leave the registry after `exit`",
        || (drawer_window(&core), core.live_terminals().contains(&id)),
        |(drawer, live)| drawer.is_none() && !*live,
    );
    settle_for(&waiter, RESTART_SETTLE);
    assert_eq!(
        placeholder_count(&core, &cwd, id),
        "0",
        "no buffer may still carry the exited drawer session's id {}: a hidden placeholder left \
         behind (bufhidden=hide) is never wiped, so the session it names is never removed.",
        id.0
    );
    assert_eq!(
        core.drawer_terminal(),
        None,
        "drawer_terminal() must be None once the drawer's shell has exited."
    );

    let (_win, next) = open_drawer(&core, &waiter, "the drawer to reopen after the exit");
    assert_ne!(
        next, id,
        "the toggle after the drawer shell exited must start a new session, not show the exited \
         {id:?} again: the drawer slot is reused without checking the session is live."
    );
    run_line(&core, next, "echo anew-$((4+4))");
    wait_for_text(&core, &waiter, next, "anew-8");

    core.shutdown().expect("shutdown should be Ok");
}

/// `<C-w>s` and `<C-w>v` inside the drawer do nothing, while `<C-w>c` closes the drawer and
/// keeps its session.
#[test]
fn split_chords_are_ignored_and_close_hides_the_drawer() {
    let (core, waiter, _cwd) = start_core();
    let (win, id) = open_drawer(&core, &waiter, "the drawer to open on a session");
    settle_for(&waiter, RESTART_SETTLE);
    let windows_before = core.read_snapshot().win_buffers.len();
    let _ = core.drain_intents();

    chord(&core, id, Key::Char('v'));
    chord(&core, id, Key::Char('s'));
    settle_for(&waiter, NEGATIVE_SETTLE);
    let splits: Vec<LayoutIntent> = core
        .drain_intents()
        .into_iter()
        .filter(|intent| matches!(intent, LayoutIntent::Split { .. }))
        .collect();
    assert_eq!(
        (
            core.read_snapshot().win_buffers.len(),
            core.live_terminals(),
            splits.len()
        ),
        (windows_before, vec![id], 0),
        "<C-w>v and <C-w>s in the drawer must change nothing: (window count, live sessions, \
         Split intents) must stay ({windows_before}, [{id:?}], 0). More sessions and windows \
         mean the router splits the drawer. Split intents: {splits:?}"
    );

    chord(&core, id, Key::Char('c'));
    poll_until(
        &waiter,
        "<C-w>c in the drawer to close the drawer window; it staying means the router does not \
         close the drawer",
        || core.read_snapshot().win_buffers.contains_key(&win),
        |present| !*present,
    );
    settle_for(&waiter, RESTART_SETTLE);
    assert_eq!(
        (core.live_terminals(), core.drawer_terminal()),
        (vec![id], Some(id)),
        "closing the drawer with <C-w>c must keep its session live and named by \
         drawer_terminal()."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The drawer's session survives Neovim being killed and restarted, and the next toggle shows
/// it again with its output.
#[test]
fn the_drawer_survives_a_restart_and_reopens() {
    let (core, waiter, _cwd) = start_core();
    let (_win, id) = open_drawer(&core, &waiter, "the drawer to open on a session");
    run_line(&core, id, "echo before-$((1+8))");
    wait_for_text(&core, &waiter, id, "before-9");

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
    assert_eq!(
        terminal_windows(&core),
        Vec::new(),
        "the fresh Neovim's windows must carry no session before the drawer is toggled."
    );
    assert_eq!(
        core.drawer_terminal(),
        Some(id),
        "drawer_terminal() must still name the live drawer session after the restart."
    );

    let (_win, again) = open_drawer(&core, &waiter, "the drawer to reopen after the restart");
    assert_eq!(
        again, id,
        "the drawer reopened after a restart must show the surviving session {id:?}, not \
         {again:?}: the drawer slot was lost across the restart."
    );
    let frame = frame_of(&core, id);
    assert!(
        frame_text(&frame).contains("before-9"),
        "the surviving drawer session must still show its output; the frame reads:\n{}",
        frame_text(&frame)
    );

    core.shutdown().expect("shutdown should be Ok");
}
