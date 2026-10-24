//! Tests for `wolf-core`'s current-file tracking: the snapshot exposes the
//! absolute, canonicalized path of the buffer currently being edited in nvim via
//! `Snapshot.current_file: Option<PathBuf>`, or `None` for an unnamed buffer.
//!
//! The value is delivered by a `BufEnter`/`BufWinEnter`/`BufFilePost` autocmd that
//! `rpcnotify`s `"wolf_current_file"` with `expand('%:p')`; core canonicalizes it.
//! Because that notification is SEPARATE from redraw flushes, these tests do not
//! assume a single flush carries it — they poll `read_snapshot().current_file`
//! with a generous timeout, draining wakes as they go.
//!
//! All tests drive a REAL, hermetic nvim (no mocks) via `WOLF_NVIM_BIN`; the
//! common helpers panic loudly if it is unset. `/tmp` entries are frequently
//! symlinks, so we canonicalize BOTH the expected temp path and (already done
//! core-side) the snapshot value before comparing.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key, WolfCore};

/// Placeholder content for the temp markdown files opened by these tests.
const DOC: &str = "# Current File\nbody\n";

/// How long to poll for `current_file` to reach an expected value before failing.
/// The autocmd notification arrives independently of redraw flushes and nvim
/// startup is not instantaneous, so we wait well beyond the expected latency.
const CURRENT_FILE_TIMEOUT: Duration = FLUSH_TIMEOUT;

/// Gap between `read_snapshot` polls while waiting for `current_file` to settle.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Polls `core.read_snapshot().current_file` until it canonical-equals `expected`,
/// or panics after [`CURRENT_FILE_TIMEOUT`]. Drains pending wakes each iteration
/// so the wake channel never backs up while we poll.
///
/// `expected` must exist on disk (so it can be canonicalized); the snapshot value
/// is already canonicalized core-side, so both sides are compared canonically.
fn wait_for_current_file(core: &WolfCore, waiter: &FlushWaiter, expected: &Path) {
    let want = std::fs::canonicalize(expected)
        .expect("expected current-file path should exist and canonicalize");
    let deadline = Instant::now() + CURRENT_FILE_TIMEOUT;

    loop {
        let current = core.read_snapshot().current_file.clone();
        if current.as_deref() == Some(want.as_path()) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {CURRENT_FILE_TIMEOUT:?} waiting for current_file to become \
                 {want:?}; last observed: {current:?}"
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Opening a file via [`InputEvent::Open`] sets `current_file` to the
/// canonicalized path of that file.
#[test]
fn open_via_input_sets_current_file() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);

    wait_for_current_file(&core, &waiter, file.path());

    core.shutdown().expect("shutdown should be Ok");
}

/// Opening a file by TYPING `:e <path><CR>` as keystrokes (not via
/// [`InputEvent::Open`]) also sets `current_file`. This exercises the autocmd
/// reveal path — the whole point of tracking buffer changes driven from inside
/// nvim, not just our own `Open` command.
#[test]
fn open_via_ex_command_keystrokes_sets_current_file() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // A freshly created temp .md with a simple (space-free, alphanumeric) name, so
    // it can be typed literally on the nvim command line without escaping.
    let file = common::temp_md(DOC);
    let path = file
        .path()
        .to_str()
        .expect("temp path should be valid UTF-8");

    // Type `:e <path><CR>` one key at a time: ':' enters command-line mode, the
    // path is typed literally, Enter executes the edit.
    core.send_input(ch(':'));
    type_str(&core, "e ");
    type_str(&core, path);
    core.send_input(key(Key::Enter));

    wait_for_current_file(&core, &waiter, file.path());

    core.shutdown().expect("shutdown should be Ok");
}

/// A genuinely UNNAMED buffer must report `current_file == None`.
///
/// The startup buffer is now a NAMED `Untitled.md` (so anonymous-file preview
/// works), so it reports `Some(.../Untitled.md)`, not `None`. To exercise the
/// unnamed case we create a genuinely no-name buffer AFTER startup by typing a
/// raw `:enew<CR>` on the nvim command line. A raw `:enew` does NOT route through
/// `wolf_new_untitled` (that runs only for the `NewBuffer` `InputEvent` and the
/// `VimEnter` autocmd), so the new buffer stays `[No Name]`/unnamed and its empty
/// `expand('%:p')` maps to `None`. We poll until `current_file` transitions from
/// the startup `Some(.../Untitled.md)` to `None`.
#[test]
fn unnamed_buffer_has_no_current_file() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // A genuinely unnamed buffer needs a raw typed `:enew`; `InputEvent::NewBuffer` would name it
    // `Untitled.md`.
    core.send_input(ch(':'));
    type_str(&core, "enew");
    core.send_input(key(Key::Enter));

    // current_file starts at Some(.../Untitled.md) and must become None once the unnamed buffer is
    // current; poll while draining wakes, and report the last value seen on timeout.
    let deadline = Instant::now() + CURRENT_FILE_TIMEOUT;
    loop {
        let current = core.read_snapshot().current_file.clone();
        if current.is_none() {
            break;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {CURRENT_FILE_TIMEOUT:?} waiting for current_file to become \
                 None for an unnamed `:enew` buffer; last observed: {current:?}"
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// `current_file` follows buffer switches: open file A, confirm it tracks A, then
/// open file B via [`InputEvent::Open`] and confirm it moves to B.
#[test]
fn current_file_follows_second_open() {
    let (core, waiter, file_a) = common::start_core_with_temp_md(DOC);
    wait_for_current_file(&core, &waiter, file_a.path());

    let file_b = common::temp_md(DOC);
    core.send_input(InputEvent::Open {
        path: file_b.path().to_path_buf(),
    });

    wait_for_current_file(&core, &waiter, file_b.path());

    core.shutdown().expect("shutdown should be Ok");
}
