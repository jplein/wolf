//! Shared helpers for `wolf-core` integration tests.
//!
//! These tests drive a real, hermetic Neovim process (no mocks). The helpers the wolf-ffi and
//! wolf-gnome tests share live in the `wolf_test_support` crate and are re-exported here: the
//! nvim, `init.lua` and runfile resolution, the temp cwd and the hermetic `WOLF_STATE_DIR`,
//! [`config`] and [`config_in`], [`FlushWaiter`] and the wake channels, [`start_core_sized`],
//! and the terminal-session helpers. [`require_nvim_bin`] panics when no pinned Neovim
//! resolves, because these test bodies handle no skip.
//!
//! The helpers defined below are wolf-core's own: [`poll_until`], [`short_tempdir`], the temp
//! `.md` files and the `start_core_with_*` starters, the input constructors, [`send_ex`] and
//! [`probe_values`], and the terminal extras built on the shared session helpers.

#![allow(dead_code, unused_imports)] // Not every helper is used by every test binary.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use std::io::Write as _;

use wolf_core::{
    InputEvent, Key, KeyEvent, TerminalCommand, TerminalFrame, TerminalId, TerminalTarget, WolfCore,
};

pub use wolf_test_support::{
    config, config_in, frame_of, frame_text, fresh_cwd, hermetic_state_dir, init_lua_path,
    open_session, pin_default_shell, poll_frame, recording_wake_channel, require_nvim_bin,
    resolve_test_runfile, rg_bin_or_skip, sh_script, start_core_sized, wait_for_text, wake_channel,
    FlushWaiter, WakeLog, DEFAULT_COLS, DEFAULT_ROWS, FLUSH_TIMEOUT, POLL_INTERVAL,
};

/// Re-runs `observe` every [`POLL_INTERVAL`] until `done` accepts what it returned, and returns
/// that value. Panics with `desc` and the last observed value after [`FLUSH_TIMEOUT`].
///
/// A test polls because one `wait_for_flush` can return on a frame that is not its input's:
/// Neovim's markdown ftplugin starts treesitter, and a parse that outlasts its budget finishes
/// through `vim.schedule` and redraws later with no input at all.
///
/// `observe` reads into owned values, so no snapshot read guard outlives it: read the snapshot
/// inside the closure (`|| f(&core.read_snapshot())`). A guard held across a blocking core call
/// such as `list_buffers()`, or across the drain and sleep, stalls the redraw handler's
/// `try_write` and parks the input pump on its write lock.
///
/// It drains `waiter` between tries, so a test that asserts a wake afterward with
/// [`FlushWaiter::wait_for_wake`] cannot use it; `nvim_liveness.rs` keeps a non-draining loop.
pub fn poll_until<T: std::fmt::Debug>(
    waiter: &FlushWaiter,
    desc: &str,
    mut observe: impl FnMut() -> T,
    done: impl Fn(&T) -> bool,
) -> T {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let seen = observe();
        if done(&seen) {
            return seen;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for {desc}; last observed: {seen:?}");
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}
/// Neovim's `BASENAMELEN` (`NAME_MAX - 5`): the byte length a `//`-style swap name is
/// cut to before `.swp` is appended, so a cut name is 254 bytes long.
///
/// The `//`-style name is the file's full path with every `/` turned into `%`, one
/// byte for one byte, so it passes this limit exactly when the full path does. Mirrors
/// `SWAP_NAME_MAX` in `runtime/nvim/init.lua`, which gives such a file no swap at all.
pub const SWAP_NAME_MAX: usize = 250;

/// The prefix of the dir [`short_tempdir`] creates under its root.
const SHORT_TEMP_PREFIX: &str = "wolf-";

/// The random characters `tempfile` appends to [`SHORT_TEMP_PREFIX`], set explicitly so
/// [`SHORT_TEMP_ROOT_MAX`]'s arithmetic does not rest on `tempfile`'s default.
const SHORT_TEMP_RAND_LEN: usize = 6;

/// Bytes reserved below a [`short_tempdir`] for `/<fixture name>`.
///
/// Every swap-bearing fixture its callers place directly in it fits: the longest is the
/// restarted startup buffer's `/Untitled 2.md` (14 bytes). A test that needs a longer name
/// there must raise this, which shortens [`SHORT_TEMP_ROOT_MAX`] by the same amount.
pub const SHORT_FIXTURE_NAME_MAX: usize = 16;

/// Longest canonical temp root, in bytes, that [`short_tempdir`] accepts.
///
/// A fixture there has the full path `<root>/wolf-XXXXXX/<name>`, which must stay within
/// [`SWAP_NAME_MAX`] (250) for the file to keep a real swap file. So the root gets what is
/// left after `/wolf-XXXXXX` (12 bytes) and [`SHORT_FIXTURE_NAME_MAX`] (16): 222 bytes. A
/// fixed small cap would refuse a deep but adequate root, such as Bazel's `TEST_TMPDIR`
/// under a sandboxed worker whose `/tmp` is not writable, leaving no usable candidate.
pub const SHORT_TEMP_ROOT_MAX: usize =
    SWAP_NAME_MAX - (1 + SHORT_TEMP_PREFIX.len() + SHORT_TEMP_RAND_LEN) - SHORT_FIXTURE_NAME_MAX;

/// An owned temp dir, canonicalized, under a root of at most
/// [`SHORT_TEMP_ROOT_MAX`] bytes. Returned with its guard.
///
/// Use it as the cwd of any test that needs a fixture to HAVE a swap file, and keep each
/// such fixture's `/<name>` within [`SHORT_FIXTURE_NAME_MAX`]. The OS temp dir is used
/// when its canonical form is short enough; otherwise `/tmp`. A deep `TMPDIR` (a nested
/// `nix develop`'s, a sandboxed worker's) would otherwise push every fixture past
/// [`SWAP_NAME_MAX`], where Wolf gives the buffer no swap file at all, and a swap-recovery
/// test would fail for a reason that has nothing to do with recovery.
///
/// Canonical because nvim names buffers and swaps by the resolved path (macOS
/// `/tmp` → `/private/tmp`). Panics, naming every candidate and why it was
/// refused, when neither root is usable: a skip would read as a pass.
pub fn short_tempdir() -> (tempfile::TempDir, PathBuf) {
    let candidates = [std::env::temp_dir(), PathBuf::from("/tmp")];
    let mut refusals: Vec<String> = Vec::new();
    for candidate in &candidates {
        let root = match std::fs::canonicalize(candidate) {
            Ok(root) => root,
            Err(e) => {
                refusals.push(format!("{}: cannot canonicalize: {e}", candidate.display()));
                continue;
            }
        };
        let len = root.as_os_str().len();
        if len > SHORT_TEMP_ROOT_MAX {
            refusals.push(format!(
                "{} (canonical {}): {len} bytes, over {SHORT_TEMP_ROOT_MAX}",
                candidate.display(),
                root.display()
            ));
            continue;
        }
        let made = tempfile::Builder::new()
            .prefix(SHORT_TEMP_PREFIX)
            .rand_bytes(SHORT_TEMP_RAND_LEN)
            .tempdir_in(&root);
        let dir = match made {
            Ok(dir) => dir,
            Err(e) => {
                refusals.push(format!("{}: cannot create a temp dir: {e}", root.display()));
                continue;
            }
        };
        let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize short temp dir");
        return (dir, cwd);
    }
    panic!(
        "no short temp root is usable, so a fixture's swap name would pass Neovim's \
         {SWAP_NAME_MAX}-byte limit and Wolf would give it no swap file. A root must be at most \
         {SHORT_TEMP_ROOT_MAX} bytes canonical and writable. Tried, in order: {refusals:?}. \
         Point TMPDIR at a shorter writable directory or make /tmp writable."
    );
}

/// Writes `content` to a fresh temp file with a `.md` extension and returns it.
/// The returned `NamedTempFile` keeps the file alive for the test's duration.
pub fn temp_md(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .prefix("wolf-test-")
        .suffix(".md")
        .tempfile()
        .expect("failed to create temp .md file");
    f.write_all(content.as_bytes())
        .expect("failed to write temp .md content");
    f.flush().expect("failed to flush temp .md content");
    f
}

/// Allocates a path for a not-yet-existing `.md` file (for SaveAs). The file is
/// NOT created on disk; the directory guard keeps the parent alive.
pub fn temp_md_path() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().join("wolf-saveas.md");
    (dir, path)
}

/// Starts a `WolfCore` at default grid size and opens a temp `.md` containing
/// `content`; [`start_core_with_temp_md_sized`] says what it waits for.
///
/// Returns the live core, its flush waiter, and the temp file (kept alive).
pub fn start_core_with_temp_md(content: &str) -> (WolfCore, FlushWaiter, tempfile::NamedTempFile) {
    start_core_with_temp_md_sized(content, DEFAULT_COLS, DEFAULT_ROWS)
}

/// Like [`start_core_with_temp_md`] but with an explicit grid size.
///
/// Returns once the snapshot's `current_file` is the opened file, a redraw frame has landed
/// after a post-startup `seq` baseline, and, when `content`'s first line has text, row 0 of the
/// active grid shows text, so the grid, cursor and viewport show the file. Panics naming the
/// path after [`FLUSH_TIMEOUT`].
///
/// `seq` alone is not enough: a frame from work nvim schedules later, such as treesitter's
/// asynchronous parse of the startup buffer, can land after the baseline and before the `:edit`.
/// `current_file` alone is not enough: it is notified during `:edit`, before the open's frame.
/// A blank first line gets no text check, because an empty file draws exactly like the empty
/// startup buffer, so nothing a test reads differs.
///
/// It fences off no frame that lands after it returns: a late redraw can satisfy any later single
/// `wait_for_flush`, so a test that waits for its own input must poll for the state it expects
/// with [`poll_until`].
pub fn start_core_with_temp_md_sized(
    content: &str,
    cols: u16,
    rows: u16,
) -> (WolfCore, FlushWaiter, tempfile::NamedTempFile) {
    let file = temp_md(content);
    let (wake, waiter) = wake_channel();

    let core = WolfCore::start(config(cols, rows), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");

    // The attach flush lands during WolfCore::start, so this keys on the absolute seq >= 1; an
    // advance-based wait would baseline at 1 and block forever.
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);

    // nvim flushes what it has drawn before answering a request, so the baseline fences off
    // those startup frames, but not a frame from work nvim schedules later.
    assert_ne!(
        core.current_tabpage(),
        0,
        "the startup barrier RPC (nvim_get_current_tabpage) failed, so the baseline taken after \
         it would not fence off late startup frames"
    );
    let baseline = core.read_snapshot().seq;

    core.send_input(InputEvent::Open {
        path: file.path().to_path_buf(),
    });
    let expect_text = content.lines().next().is_some_and(|l| !l.trim().is_empty());
    waiter.wait_for_open(&core, file.path(), baseline, expect_text, FLUSH_TIMEOUT);

    (core, waiter, file)
}

// --- A modified buffer whose file changed on disk ---------------------------

/// What `note.txt` holds when [`start_core_with_disk_conflict`] opens it.
pub const CONFLICT_ORIGINAL: &str = "first draft line\n";

/// The word [`start_core_with_disk_conflict`] appends to line 1 in Neovim and never saves.
pub const CONFLICT_EDIT: &str = "EDITED";

/// What [`start_core_with_disk_conflict`] writes to `note.txt` behind Neovim's back.
///
/// Its byte length differs from [`CONFLICT_ORIGINAL`]'s, so Neovim's size check sees the change
/// whatever the filesystem's mtime granularity, and it shares no word with either that text or
/// [`CONFLICT_EDIT`], so a test can tell the three apart on the grid and on disk.
pub const CONFLICT_DISK: &str = "rewritten outside Wolf by the test\n";

/// How far ahead of now [`start_core_with_disk_conflict`] sets the rewritten file's mtime, so
/// Neovim sees it as changed without the test sleeping out a coarse mtime clock.
const CONFLICT_MTIME_AHEAD: Duration = Duration::from_secs(10);

/// A core whose current buffer, `note.txt`, holds an unsaved edit while its file changed on disk:
/// what [`start_core_with_disk_conflict`] returns.
pub struct DiskConflict {
    /// The live core, with `note.txt` current and modified.
    pub core: WolfCore,
    /// The core's flush waiter.
    pub waiter: FlushWaiter,
    /// Keeps the core's cwd alive. It holds `note.txt`, and tests may put probe files in it.
    pub dir: tempfile::TempDir,
    /// The canonical path of `note.txt`.
    pub file: PathBuf,
}

/// Starts a core in a fresh canonical temp dir, opens `note.txt` there, appends
/// [`CONFLICT_EDIT`] to its line 1 without saving, then rewrites the file on disk with
/// [`CONFLICT_DISK`] and an mtime [`CONFLICT_MTIME_AHEAD`] in the future.
///
/// Neovim has not looked at the file again when this returns, so the next thing that checks it
/// (`:write`, or the `FocusGained` check of `nvim_ui_set_focus`) finds both sides changed.
///
/// It raises `updatetime` first, so `CursorHold`'s `:checktime` from `WolfAutoRead` cannot find
/// the conflict on Neovim's main loop before the test's own trigger does.
///
/// It calls no `block_on_rpc` method of the core (`current_tabpage`, `list_buffers` and the
/// like), because `nvim_liveness.rs` uses it in a case that asserts the input pump's own stall
/// handling, which one such call would clear behind its back.
pub fn start_core_with_disk_conflict() -> DiskConflict {
    let dir = tempfile::tempdir().expect("failed to create the conflict test's temp dir");
    let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize the temp dir");
    let file = cwd.join("note.txt");
    std::fs::write(&file, CONFLICT_ORIGINAL).expect("failed to write note.txt");

    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(config_in(cwd, DEFAULT_COLS, DEFAULT_ROWS), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);

    send_ex(&core, "set updatetime=600000");

    core.send_input(InputEvent::Open { path: file.clone() });
    let original = CONFLICT_ORIGINAL.trim_end();
    poll_until(
        &waiter,
        "row 0 to show note.txt's original text after the open",
        || core.read_snapshot().active_grid().row_text(0),
        |row0| row0.contains(original),
    );

    core.send_input(ch('A'));
    type_str(&core, &format!(" {CONFLICT_EDIT}"));
    core.send_input(key(Key::Esc));
    let edited = format!("{original} {CONFLICT_EDIT}");
    poll_until(
        &waiter,
        "row 0 to show the unsaved edit appended to line 1",
        || core.read_snapshot().active_grid().row_text(0),
        |row0| row0.contains(&edited),
    );

    std::fs::write(&file, CONFLICT_DISK).expect("failed to rewrite note.txt behind Neovim");
    std::fs::File::options()
        .write(true)
        .open(&file)
        .and_then(|f| f.set_modified(std::time::SystemTime::now() + CONFLICT_MTIME_AHEAD))
        .expect("failed to move note.txt's mtime ahead");

    DiskConflict {
        core,
        waiter,
        dir,
        file,
    }
}

// --- Small input constructors to cut repetition ---------------------------

/// A plain (unmodified) key press.
pub fn key(k: Key) -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: k,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    })
}

/// A single plain character key press.
pub fn ch(c: char) -> InputEvent {
    key(Key::Char(c))
}

/// A Ctrl-modified key press.
pub fn ctrl(k: Key) -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: k,
        ctrl: true,
        shift: false,
        alt: false,
        meta: false,
    })
}

/// Sends each character in `s` as an individual key event, in order.
pub fn type_str(core: &WolfCore, s: &str) {
    for c in s.chars() {
        core.send_input(ch(c));
    }
}

// --- Ex-command + file-probe observation channel ---------------------------

/// Types `:{cmd}<CR>` as keystrokes: `:` enters command-line mode, `cmd` is
/// typed literally, Enter executes it. `cmd` must NOT include the leading `:`.
///
/// Promoted from the identical local helper in `messages.rs` (`swap_recovery.rs`
/// carries the same shape as `ex_command`), so probe-driven tests share one
/// definition.
pub fn send_ex(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// Gap between polls of the probe file [`probe_values`] waits on.
const PROBE_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Evaluates `exprs` INSIDE the live nvim and returns their values as strings,
/// one per expression, in order.
///
/// The observation channel is a file probe: core exposes no eval RPC, so this
/// types `:call writefile([<exprs>], '<probe>')` through the input pump and
/// polls `probe` until it holds exactly one line per expression (50 ms cadence,
/// [`FLUSH_TIMEOUT`] budget). Because the writefile command is queued BEHIND any
/// keys the caller sent earlier, the values observe nvim's state AFTER those
/// keys were processed — no separate settle is needed.
///
/// Expressions are spliced into the command line verbatim, so they must be
/// cmdline-safe (each must evaluate to a string or number; keep quoting simple —
/// a double-quoted `luaeval("…")` may carry single quotes inside). `probe` is
/// single-quoted, so its path must not contain a single quote (temp paths do
/// not). A timeout means the command never wrote the file — usually because one
/// expression raised — and the panic names the exact command.
pub fn probe_values(core: &WolfCore, exprs: &[&str], probe: &Path) -> Vec<String> {
    // A stale probe from an earlier call at the same path would satisfy the poll
    // with old values; remove it first so only THIS command's write can.
    let _ = std::fs::remove_file(probe);
    let cmd = format!(
        "call writefile([{}], '{}')",
        exprs.join(", "),
        probe.display()
    );
    send_ex(core, &cmd);

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if let Ok(text) = std::fs::read_to_string(probe) {
            let lines: Vec<String> = text.lines().map(String::from).collect();
            // writefile writes one line per list item; fewer means mid-write.
            if lines.len() == exprs.len() {
                return lines;
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for nvim to write the probe file {} \
                 (command `:{cmd}`) — a timeout usually means one of the probed expressions \
                 raised, so writefile never ran",
                probe.display()
            );
        }
        std::thread::sleep(PROBE_POLL_INTERVAL);
    }
}

// --- Terminal sessions --------------------------------------------------------

/// The marker a [`cat_command`] session prints once its tty is raw, before `cat` reads.
pub const CAT_READY: &str = "cat-ready";

/// Starts a `WolfCore` in `cwd` at the default grid size and waits for the first flush.
pub fn start_core_at(cwd: PathBuf) -> (WolfCore, FlushWaiter) {
    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(config_in(cwd, DEFAULT_COLS, DEFAULT_ROWS), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter)
}

/// [`start_core_at`] with a [`recording_wake_channel`].
pub fn start_core_recording(cwd: PathBuf) -> (WolfCore, FlushWaiter, WakeLog) {
    let (wake, waiter, log) = recording_wake_channel();
    let core = WolfCore::start(config_in(cwd, DEFAULT_COLS, DEFAULT_ROWS), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter, log)
}

/// An interactive `/bin/sh` with no arguments.
pub fn interactive_sh() -> TerminalCommand {
    TerminalCommand {
        program: PathBuf::from("/bin/sh"),
        args: Vec::new(),
    }
}

/// A byte-inspection session: `prelude` is printed first (a `printf` format, so `\033` is
/// ESC), then the tty goes raw without echo, [`CAT_READY`] is printed, and `cat -v` shows
/// every byte the session is sent, ESC as `^[`.
pub fn cat_command(prelude: &str) -> TerminalCommand {
    sh_script(&format!(
        "printf '{prelude}'; stty raw -echo; printf '{CAT_READY}\\r\\n'; exec cat -v"
    ))
}

/// What a [`cat_command`] session's `cat -v` printed: the rows after the [`CAT_READY`] row,
/// joined with nothing between them, trimmed. Empty until the marker shows.
pub fn cat_output(frame: &TerminalFrame) -> String {
    let rows: Vec<String> = (0..frame.rows).map(|row| frame.row_text(row)).collect();
    match rows.iter().position(|row| row.contains(CAT_READY)) {
        Some(marker) => rows[marker + 1..].concat().trim().to_owned(),
        None => String::new(),
    }
}

/// Opens a [`cat_command`] session in `target` and waits for its [`CAT_READY`] marker.
pub fn open_cat(
    core: &WolfCore,
    waiter: &FlushWaiter,
    target: TerminalTarget,
    prelude: &str,
) -> TerminalId {
    let id = open_session(core, target, cat_command(prelude));
    wait_for_text(core, waiter, id, CAT_READY);
    id
}

/// Waits for a session's shell to print `PID=<n>` and returns `n`.
pub fn wait_for_pid(core: &WolfCore, waiter: &FlushWaiter, id: TerminalId) -> u32 {
    let frame = poll_frame(core, waiter, id, "the shell to print PID=<n>", |frame| {
        parse_pid(&frame_text(frame)).is_some()
    });
    parse_pid(&frame_text(&frame)).expect("poll_frame returned only once a PID was printed")
}

/// The number after the first `PID=` in `text` that is followed by at least one digit.
fn parse_pid(text: &str) -> Option<u32> {
    let rest = &text[text.find("PID=")? + "PID=".len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Whether `pid` names a process, a zombie included: `kill -0` succeeds on a zombie and fails
/// only once its parent has reaped it.
pub fn pid_alive(pid: u32) -> bool {
    std::process::Command::new("/bin/sh")
        .args(["-c", &format!("kill -0 {pid} 2>/dev/null")])
        .status()
        .expect("failed to run /bin/sh for kill -0")
        .success()
}

/// Polls until `pid` is gone (reaped), failing with `desc` after [`FLUSH_TIMEOUT`].
pub fn wait_for_pid_gone(pid: u32, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while pid_alive(pid) {
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for pid {pid} to be gone: {desc}. \
                 Still there means the shell was never signalled, or it was killed and never \
                 reaped (a zombie still answers kill -0)."
            );
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Drains `waiter` for `dur`, so late notifications land before a negative assertion.
pub fn settle_for(waiter: &FlushWaiter, dur: Duration) {
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The session bound to window `win` in the snapshot's `win_buffers`, if any.
pub fn win_terminal(core: &WolfCore, win: i64) -> Option<TerminalId> {
    core.read_snapshot()
        .win_buffers
        .get(&win)
        .and_then(|buffer| buffer.terminal)
}

/// Every window whose `win_buffers` entry carries a session, with its session.
pub fn terminal_windows(core: &WolfCore) -> Vec<(i64, TerminalId)> {
    let snap = core.read_snapshot();
    let mut found: Vec<(i64, TerminalId)> = snap
        .win_buffers
        .iter()
        .filter_map(|(win, buffer)| buffer.terminal.map(|id| (*win, id)))
        .collect();
    found.sort();
    found
}

/// Polls until the snapshot names an active window and returns it.
pub fn active_window(core: &WolfCore, waiter: &FlushWaiter) -> i64 {
    poll_until(
        waiter,
        "the snapshot to name an active window",
        || core.read_snapshot().active_win,
        Option::is_some,
    )
    .expect("poll_until returned only once active_win was set")
}

/// The grid that shows window `win`, if the snapshot has one.
pub fn grid_of_win(core: &WolfCore, win: i64) -> Option<u64> {
    core.read_snapshot()
        .windows
        .values()
        .find(|info| info.win == win && !info.hidden)
        .map(|info| info.grid)
}

/// A key press for the session API with no modifier.
pub fn term_key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// A Ctrl-modified key press for the session API.
pub fn term_ctrl(key: Key) -> KeyEvent {
    KeyEvent {
        ctrl: true,
        ..term_key(key)
    }
}

/// Types `line` into a session as text, then presses Enter through `terminal_key`.
pub fn run_line(core: &WolfCore, id: TerminalId, line: &str) {
    core.terminal_text(id, line);
    core.terminal_key(id, term_key(Key::Enter));
}
