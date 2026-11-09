//! Integration test of the file-tree rescan a terminal pane's focus loss forces: a file a
//! session's shell creates is listed by a live `FileSearch` once `terminal_focus(id, false)`
//! reports the pane losing focus, and not before.
//!
//! A real `/bin/sh` in a real pty creates the file, beside a real, hermetic Neovim. Core's
//! filesystem watcher is off (`WOLF_FS_WATCH=0`), so nothing but the focus loss can surface
//! the file; this binary is its own because the variable is process-global, and the session is
//! started under [`WATCH_ENV_LOCK`] as `file_search_rescan.rs` starts its sessions.
//!
//! ## Premises about the components
//!
//! Each is measured by this test against the real component, not assumed:
//!
//! - With the watcher off, only Neovim's triggers (buffer events, `ShellCmdPost`,
//!   `FocusGained`) and core's own force a rescan; a shell writing a file through its pty
//!   fires none of them.
//! - `WOLF_FS_WATCH` is read once per search, on the thread that calls `start_file_search`.

mod common;

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use common::{open_session, settle_for, sh_script, start_core_at, term_key, FlushWaiter};
use wolf_core::{FileSearch, Key, TerminalTarget, WakeFn, WolfCore};

/// Longer than the rescan debounce (200 ms by default) plus a tiny tree's walk, so a rescan
/// something else triggered has landed before the listing is read.
const DEBOUNCE_SETTLE: Duration = Duration::from_millis(1500);

/// Upper bound for a forced rescan to land: the debounce plus a walk of a tiny tree.
const RESCAN_TIMEOUT: Duration = Duration::from_secs(10);

/// The file the session's shell creates.
const CREATED: &str = "made-by-the-shell.md";

/// Serializes setting `WOLF_FS_WATCH` around `start_file_search`, as `file_search_rescan.rs`'s
/// lock does.
static WATCH_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Acquires [`WATCH_ENV_LOCK`], recovering from poisoning.
fn lock_env() -> MutexGuard<'static, ()> {
    WATCH_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Starts a file search on `core` with the filesystem watcher off.
fn start_unwatched_search(core: &WolfCore, wake: WakeFn) -> FileSearch {
    let _env = lock_env();
    std::env::set_var("WOLF_FS_WATCH", "0");
    let search = core.start_file_search(wake);
    std::env::remove_var("WOLF_FS_WATCH");
    search
}

/// Whether the search lists `name`.
fn lists(search: &FileSearch, name: &str) -> bool {
    search
        .read()
        .entries
        .iter()
        .any(|entry| entry.display == name)
}

/// Polls until `pred` holds, failing with `desc` after `timeout`.
fn wait_until(waiter: &FlushWaiter, timeout: Duration, desc: &str, pred: impl Fn() -> bool) {
    let deadline = Instant::now() + timeout;
    while !pred() {
        assert!(
            Instant::now() < deadline,
            "timed out after {timeout:?} waiting for {desc}"
        );
        waiter.drain();
        std::thread::sleep(common::POLL_INTERVAL);
    }
}

/// A file a terminal session's shell creates appears in a live file search once the pane
/// reports losing focus, with the watcher off.
#[test]
fn a_terminal_focus_loss_rescans_the_file_tree() {
    let cwd = std::fs::canonicalize(common::fresh_cwd()).expect("failed to canonicalize the cwd");
    std::fs::write(cwd.join("keeper.md"), "k").expect("failed to write keeper.md");
    let (core, waiter) = start_core_at(cwd.clone());
    let (wake, wakes) = common::wake_channel();
    let search = start_unwatched_search(&core, wake);
    wait_until(
        &wakes,
        RESCAN_TIMEOUT,
        "the file search to complete",
        || search.read().complete,
    );
    assert!(
        lists(&search, "keeper.md"),
        "precondition: the search must list keeper.md."
    );

    let script = format!("stty -echo; printf ready; read x; touch {CREATED}; exec sleep 1000");
    let id = open_session(&core, TerminalTarget::Window(0), sh_script(&script));
    common::wait_for_text(&core, &waiter, id, "ready");
    settle_for(&waiter, DEBOUNCE_SETTLE);

    core.terminal_key(id, term_key(Key::Enter));
    let created = cwd.join(CREATED);
    wait_until(
        &waiter,
        RESCAN_TIMEOUT,
        "the shell to create the file",
        || created.exists(),
    );
    settle_for(&wakes, DEBOUNCE_SETTLE);
    assert!(
        !lists(&search, CREATED),
        "precondition: with the watcher off and no focus change, nothing may rescan, but the \
         search already lists {CREATED}: something else forces rescans, so this test cannot \
         tell the focus loss's rescan apart."
    );

    core.terminal_focus(id, false);
    wait_until(
        &wakes,
        RESCAN_TIMEOUT,
        &format!(
            "{CREATED} to be listed after terminal_focus(false); never means a terminal pane's \
             focus loss forces no rescan"
        ),
        || lists(&search, CREATED),
    );
    assert!(
        lists(&search, "keeper.md"),
        "the rescan must keep listing keeper.md."
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}
