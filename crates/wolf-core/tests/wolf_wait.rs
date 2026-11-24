//! End-to-end tests of `wolf --wait`, `specs/cli-and-file-open.md` §6: the real `wolf` binary
//! over a real `OpenListener`, and a real core and Neovim behind it.
//!
//! Each test acts as the front-end. It binds a listener, starts a core in its case's `work`
//! directory, runs `wolf --wait <file>` (or `git commit` with `wolf --wait` as its editor), drains
//! the wait with `drain_taken`, sends `InputEvent::Open` for the request's `OpenTarget::File` path
//! and hands the wait to `WolfCore::wait_on`. Keys are typed with `send_ex` and `type_str`. A test
//! that closes the tab's last pane shuts the core down as soon as `Command::CloseWindow` drains,
//! as a front-end closing the tab would.
//!
//! An exit is polled for up to [`EXIT_WAIT`]; "still waiting" is a command still running after
//! [`SETTLE`]. A command a test never sees exit is killed when its [`Proc`] drops.
//!
//! ## Relative socket paths
//!
//! No absolute socket path under Bazel's test `TMPDIR` fits 103 bytes inside the macOS gate, so
//! the binary enters one kept, canonical base dir before any socket is named ([`env`]), as
//! `wolf_command.rs` does. The listener binds `c<n>/s`, and every `wolf` runs with
//! `WOLF_STATE_DIR=../s` from `c<n>/work` or `c<n>/repo`. Each `wolf` is a copy at `bin/wolf` in
//! the base: on Linux one beside the real `wolf-gnome` would start it when nothing listens.
//!
//! ## Premises about the components
//!
//! - Neovim fires `WinClosed` for `:q` on a window that is not the tab's last, and `:bd` or `:bw`
//!   on a buffer shown in the tab's only window keeps that window, showing another buffer. So W1
//!   ends by a window close and W3 and W4 by a deletion, each with nothing else to end it.
//! - `:e other` in the only window showing a buffer hides it and closes no window.
//! - A self-`SIGKILL` of Neovim is a crash the supervisor sees, and one RPC that outlasts the
//!   core's RPC budget while Neovim sleeps marks it `NotResponding`, as `nvim_liveness.rs` shows.
//! - `git commit` runs its editor in the repository's top directory, where the test runs it, and
//!   treats an editor's non-zero exit as an abort that makes no commit.

mod common;

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use common::{
    key, probe_values, require_nvim_bin, resolve_test_runfile, send_ex, settle_for, start_core_at,
    type_str, FlushWaiter, FLUSH_TIMEOUT, POLL_INTERVAL,
};
use wolf_core::{
    Command as CoreCommand, InputEvent, Key, Liveness, OpenListener, OpenTarget, OpenWait, WakeFn,
    WolfCore,
};

/// The variable Bazel names the `wolf` runfile in.
const WOLF_BIN_ENV: &str = "WOLF_TEST_WOLF_BIN";

/// The state dir every `wolf` is given, relative to its case's `work` or `repo` directory.
const STATE_FROM_CASE: &str = "../s";

/// How long a command gets to be taken, and to exit once its wait should have ended.
const EXIT_WAIT: Duration = Duration::from_secs(15);

/// How long a command must keep running to count as still waiting.
const SETTLE: Duration = Duration::from_millis(1500);

/// The toast's text up to the file, `specs/cli-and-file-open.md` §6.
const STILL_WAITING: &str = "wolf --wait is still waiting for";

/// How long W10's wedge holds Neovim, far past the RPC budget; the restart kills it first.
const WEDGE_SLEEP_MS: u64 = 60_000;

/// How long after typing the wedge the stalled RPC is sent, so Neovim has started sleeping.
const WEDGE_SETTLE: Duration = Duration::from_millis(300);

struct Env {
    /// The copy at `bin/wolf` in the base, with no `wolf-gnome` beside it.
    bin: PathBuf,
    base: PathBuf,
}

/// The binary, copied into the base dir, and the base dir, entered once per test binary.
///
/// The pinned Neovim and the `wolf` runfile resolve before the working directory changes, since
/// a runfiles path can be relative to it.
fn env() -> &'static Env {
    static ENV: OnceLock<Env> = OnceLock::new();
    ENV.get_or_init(|| {
        require_nvim_bin();
        let bin = option_env!("CARGO_BIN_EXE_wolf")
            .map(PathBuf::from)
            .or_else(|| resolve_test_runfile(WOLF_BIN_ENV))
            .unwrap_or_else(|| {
                panic!(
                    "no wolf binary: Cargo sets CARGO_BIN_EXE_wolf for src/bin/wolf.rs, and \
                     Bazel names the :wolf runfile in {WOLF_BIN_ENV}. Neither means the binary \
                     or the target's data and env wiring is missing."
                )
            });
        let built = std::fs::canonicalize(&bin)
            .unwrap_or_else(|e| panic!("the wolf binary {} does not resolve: {e}", bin.display()));
        let kept = tempfile::Builder::new()
            .prefix("wolf-wait")
            .tempdir()
            .expect("failed to create the base temp dir")
            .keep();
        let base = std::fs::canonicalize(&kept).expect("failed to canonicalize the base dir");
        std::env::set_current_dir(&base).expect("failed to enter the base dir");
        let dir = base.join("bin");
        std::fs::create_dir_all(&dir).expect("failed to create the binary's dir");
        let copy = dir.join("wolf");
        std::fs::copy(&built, &copy).unwrap_or_else(|e| {
            panic!("copying the wolf binary to {} failed: {e}", copy.display())
        });
        Env { bin: copy, base }
    })
}

/// One test's directories and listener: `c<n>/work`, where files and the core live, `c<n>/repo`
/// for git, and the listener on `c<n>/s`.
struct Case {
    dir: PathBuf,
    work: PathBuf,
    listener: OpenListener,
}

fn new_case() -> Case {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let base = &env().base;
    let rel = PathBuf::from(format!("c{}", COUNTER.fetch_add(1, Ordering::Relaxed)));
    let dir = base.join(&rel);
    let work = dir.join("work");
    std::fs::create_dir_all(&work).expect("failed to create the case's work dir");
    let wake: WakeFn = Arc::new(|| {});
    let listener = OpenListener::bind(&rel.join("s"), wake)
        .unwrap_or_else(|e| panic!("OpenListener::bind failed: {e:?}"));
    Case {
        dir,
        work,
        listener,
    }
}

impl Case {
    /// Writes `text` to `work/name` and returns its path.
    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.work.join(name);
        std::fs::write(&path, text)
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
        path
    }

    /// A core in `work`.
    fn start_core(&self) -> (WolfCore, FlushWaiter) {
        start_core_at(self.work.clone())
    }

    /// Starts `wolf --wait <file>` in `work`.
    fn spawn_wait(&self, file: &str) -> Proc {
        let mut cmd = Command::new(&env().bin);
        cmd.args(["--wait", file])
            .current_dir(&self.work)
            .env("WOLF_STATE_DIR", STATE_FROM_CASE)
            .env_remove("WOLF_TAB_ID");
        Proc::spawn(cmd, format!("wolf --wait {file}"))
    }

    /// Takes the wait `proc` sends: its `OpenTarget::File` path and its `OpenWait`.
    fn take_wait(&self, proc: &mut Proc) -> (PathBuf, OpenWait) {
        let deadline = Instant::now() + EXIT_WAIT;
        let taken = loop {
            let mut drained = self.listener.drain_taken();
            if !drained.is_empty() {
                assert_eq!(
                    drained.len(),
                    1,
                    "{} must queue one request; the drain holds {drained:?}.",
                    proc.what
                );
                break drained.remove(0);
            }
            if let Some((status, stderr)) = proc.exited() {
                panic!(
                    "{} exited {status} before Wolf took a wait from it. For wolf, 2 means the \
                     listener refused the wait; for git, its editor wolf --wait failed. \
                     stderr:\n{stderr}",
                    proc.what
                );
            }
            assert!(
                Instant::now() < deadline,
                "{} sent no request within {EXIT_WAIT:?}.",
                proc.what
            );
            thread::sleep(POLL_INTERVAL);
        };
        assert!(
            taken.request.wait,
            "{}'s request must carry wait: {:?}",
            proc.what, taken.request
        );
        let OpenTarget::File(path) = taken.request.target() else {
            panic!(
                "{}'s request must name a file; it is {:?}",
                proc.what,
                taken.request.target()
            );
        };
        let wait = taken.wait.unwrap_or_else(|| {
            panic!(
                "{}'s request must drain holding its OpenWait; None means it was queued as a \
                 plain request, so nothing can end the wait.",
                proc.what
            )
        });
        (path, wait)
    }

    /// Acts as the front-end for `proc`'s wait: opens its file in the current window, then hands
    /// the wait to the core. Returns the file's path.
    fn open_waited(&self, core: &WolfCore, proc: &mut Proc) -> PathBuf {
        let (path, wait) = self.take_wait(proc);
        core.send_input(InputEvent::Open { path: path.clone() });
        core.wait_on(&path, wait);
        path
    }

    /// Evaluates `exprs` in Neovim, behind every input sent before.
    fn probe(&self, core: &WolfCore, exprs: &[&str]) -> Vec<String> {
        probe_values(core, exprs, &self.dir.join("probe.txt"))
    }

    /// The current window's file, its buffer number and its window ID.
    fn current(&self, core: &WolfCore) -> (String, String, String) {
        let values = self.probe(core, &["expand('%:p')", "bufnr()", "win_getid()"]);
        (values[0].clone(), values[1].clone(), values[2].clone())
    }

    /// A repository at `c<n>/repo` with one commit, `seed`.
    fn repo(&self) -> PathBuf {
        let repo = self.dir.join("repo");
        std::fs::create_dir_all(&repo).expect("failed to create the repository dir");
        git(&repo, &["init", "-q"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "seed"]);
        repo
    }

    /// Starts `git commit --allow-empty` in `repo` with `wolf --wait` as its editor.
    fn spawn_commit(&self, repo: &Path) -> Proc {
        let editor = format!("'{}' --wait", env().bin.display());
        let mut cmd = git_command(repo);
        cmd.args(["commit", "-q", "--allow-empty"])
            .env("GIT_EDITOR", editor)
            .env("WOLF_STATE_DIR", STATE_FROM_CASE)
            .env_remove("WOLF_TAB_ID");
        Proc::spawn(cmd, "git commit with GIT_EDITOR=\"wolf --wait\"".to_owned())
    }
}

/// `git` in `cwd` with no system or global configuration and a fixed identity.
fn git_command(cwd: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.args([
        "-c",
        "user.name=Wolf Test",
        "-c",
        "user.email=wolf-test@example.invalid",
        "-c",
        "init.defaultBranch=main",
        "-c",
        "commit.gpgsign=false",
    ])
    .current_dir(cwd)
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", "/dev/null")
    .env_remove("GIT_DIR")
    .env_remove("GIT_WORK_TREE")
    .env_remove("GIT_INDEX_FILE")
    .env_remove("GIT_EDITOR");
    cmd
}

/// Runs `git args` in `cwd`, asserting success, and returns its trimmed stdout.
fn git(cwd: &Path, args: &[&str]) -> String {
    let out = git_command(cwd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed to spawn git; it must be on PATH, as file_open_project.rs also needs");
    assert!(
        out.status.success(),
        "`git {args:?}` in {} failed with {:?}: {}",
        cwd.display(),
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A command under test. Dropping it kills the command if it still runs.
struct Proc {
    child: Child,
    what: String,
}

impl Proc {
    fn spawn(mut cmd: Command, what: String) -> Proc {
        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn {what}: {e}"));
        Proc { child, what }
    }

    /// Its status and stderr once it has exited.
    fn exited(&mut self) -> Option<(ExitStatus, String)> {
        let status = self.child.try_wait().expect("try_wait failed")?;
        let mut stderr = String::new();
        if let Some(mut pipe) = self.child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        Some((status, stderr))
    }

    /// Its status and stderr, failing with `why` when it is still running after [`EXIT_WAIT`].
    fn exit_within(&mut self, why: &str) -> (ExitStatus, String) {
        let deadline = Instant::now() + EXIT_WAIT;
        loop {
            if let Some(exited) = self.exited() {
                return exited;
            }
            assert!(
                Instant::now() < deadline,
                "{} was still running {EXIT_WAIT:?} after {why}, so nothing ended its wait.",
                self.what
            );
            thread::sleep(POLL_INTERVAL);
        }
    }

    /// Asserts it exits `code` within [`EXIT_WAIT`] after `why`, with `meaning` in the message.
    fn assert_exits(&mut self, code: i32, why: &str, meaning: &str) {
        let (status, stderr) = self.exit_within(why);
        assert_eq!(
            status.code(),
            Some(code),
            "{} must exit {code} after {why}; it exited {status}. {meaning} stderr:\n{stderr}",
            self.what
        );
    }

    /// Asserts it is still running after `why` and [`SETTLE`].
    fn assert_waiting(&mut self, waiter: &FlushWaiter, why: &str, meaning: &str) {
        settle_for(waiter, SETTLE);
        if let Some((status, stderr)) = self.exited() {
            panic!(
                "{} must still be waiting {SETTLE:?} after {why}; it exited {status}. {meaning} \
                 stderr:\n{stderr}",
                self.what
            );
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The toast's lines, each its chunks joined.
fn toast_lines(core: &WolfCore) -> Vec<String> {
    core.read_snapshot()
        .toast
        .lines()
        .iter()
        .map(|line| line.iter().map(|chunk| chunk.text.as_str()).collect())
        .collect()
}

/// The toast line saying `wolf --wait` still waits, if the toast holds one.
fn still_waiting_line(core: &WolfCore) -> Option<String> {
    toast_lines(core)
        .into_iter()
        .find(|line| line.contains(STILL_WAITING))
}

/// Watches the toast for [`SETTLE`], draining `waiter`, and returns every still-waiting line it
/// showed.
fn still_waiting_lines_over_settle(core: &WolfCore, waiter: &FlushWaiter) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let deadline = Instant::now() + SETTLE;
    while Instant::now() < deadline {
        if let Some(line) = still_waiting_line(core) {
            if !seen.contains(&line) {
                seen.push(line);
            }
        }
        waiter.drain();
        thread::sleep(POLL_INTERVAL);
    }
    seen
}

/// Types `text` on a new first line of the current buffer and returns to normal mode.
fn type_first_line(core: &WolfCore, text: &str) {
    type_str(core, "ggO");
    type_str(core, text);
    core.send_input(key(Key::Esc));
}

/// Shuts `core` down once Neovim asks the front-end to close its window, as a front-end closing
/// the tab does; fails with `what` when no `CloseWindow` drains within [`EXIT_WAIT`].
fn shut_down_on_close_window(core: WolfCore, what: &str) {
    let deadline = Instant::now() + EXIT_WAIT;
    loop {
        let commands = core.drain_commands();
        if commands.contains(&CoreCommand::CloseWindow) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "premise: {what} on the tab's only window must ask the front-end to close it, \
             draining Command::CloseWindow; none drained within {EXIT_WAIT:?}. E37 means the \
             buffer was left modified."
        );
        thread::sleep(POLL_INTERVAL);
    }
    core.shutdown().expect("shutdown after CloseWindow failed");
}

/// Polls `liveness()` until it is `want`, failing with `what` after [`FLUSH_TIMEOUT`].
fn wait_for_liveness(core: &WolfCore, want: Liveness, what: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while core.liveness() != want {
        assert!(
            Instant::now() < deadline,
            "{what}: liveness is {:?} after {FLUSH_TIMEOUT:?}; want {want:?}.",
            core.liveness()
        );
        thread::sleep(POLL_INTERVAL);
    }
}

/// W1: `:q` in the split showing the waited buffer, its last window, ends the wait with `0`.
#[test]
fn q_closing_the_waited_split_exits_0() {
    let case = new_case();
    let file = case.write("msgone.md", "# one\n");
    let (core, _waiter) = case.start_core();
    send_ex(&core, "vsplit");
    let mut wolf = case.spawn_wait("msgone.md");
    let path = case.open_waited(&core, &mut wolf);
    assert_eq!(
        path, file,
        "premise: the request must name msgone.md as sent"
    );
    let shown = case.probe(&core, &["expand('%:p')", "winnr('$')"]);
    assert_eq!(
        shown,
        [file.display().to_string(), "2".to_owned()],
        "premise: msgone.md must show in the current window of two"
    );

    send_ex(&core, "q");
    wolf.assert_exits(
        0,
        ":q closed the waited buffer's last window",
        "1 means the close ended the wait as abnormal.",
    );
    core.shutdown().expect("shutdown failed");
}

/// W2: `:wq` on the tab's only window reports `0` before the front-end closes the tab, though
/// the test shuts the core down as soon as `CloseWindow` drains.
#[test]
fn wq_on_the_last_pane_exits_0_before_the_tab_closes() {
    const TEXT: &str = "typed before the last pane closed";
    let case = new_case();
    let file = case.write("msgtwo.md", "# two\n");
    let (core, _waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgtwo.md");
    case.open_waited(&core, &mut wolf);
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        file.display().to_string(),
        "premise: msgtwo.md must be current"
    );

    type_first_line(&core, TEXT);
    send_ex(&core, "wq");
    shut_down_on_close_window(core, ":wq");
    wolf.assert_exits(
        0,
        ":wq on the tab's last pane, then the tab's shutdown",
        "1 means the shutdown ended the wait first: the buffer's close never reached the \
         command before close_window did.",
    );
    let written = std::fs::read_to_string(&file).expect("failed to read msgtwo.md back");
    assert!(
        written.starts_with(TEXT),
        "the :wq must have written the typed line to msgtwo.md; it holds {written:?}"
    );
}

/// W3: `:bd` of the waited buffer in the tab's only window ends the wait with `0`.
#[test]
fn bd_exits_0() {
    let case = new_case();
    case.write("msgthree.md", "# three\n");
    let (core, _waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgthree.md");
    let path = case.open_waited(&core, &mut wolf);
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        path.display().to_string(),
        "premise: msgthree.md must be current"
    );

    send_ex(&core, "bd");
    wolf.assert_exits(
        0,
        ":bd deleted the waited buffer",
        "1 means the deletion ended the wait as abnormal.",
    );
    core.shutdown().expect("shutdown failed");
}

/// W4: `:bw` of the waited buffer in the tab's only window ends the wait with `0`.
#[test]
fn bw_exits_0() {
    let case = new_case();
    case.write("msgfour.md", "# four\n");
    let (core, _waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgfour.md");
    let path = case.open_waited(&core, &mut wolf);
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        path.display().to_string(),
        "premise: msgfour.md must be current"
    );

    send_ex(&core, "bw");
    wolf.assert_exits(
        0,
        ":bw wiped the waited buffer",
        "1 means the wipe ended the wait as abnormal.",
    );
    core.shutdown().expect("shutdown failed");
}

/// W5: `:cq` in the window showing the waited buffer ends the wait with `1`, deletes the buffer
/// and leaves Neovim running.
#[test]
fn cq_in_the_waited_window_exits_1_and_keeps_nvim() {
    let case = new_case();
    case.write("msgfive.md", "# five\n");
    let (core, waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgfive.md");
    let path = case.open_waited(&core, &mut wolf);
    let (shown, buf, _) = case.current(&core);
    assert_eq!(
        shown,
        path.display().to_string(),
        "premise: msgfive.md must be current"
    );

    send_ex(&core, "cq");
    wolf.assert_exits(
        1,
        ":cq in the waited buffer's window",
        "0 means :cq ended the wait as a close.",
    );
    settle_for(&waiter, SETTLE);
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        ":cq in a waited window must not quit Neovim; Dead means it ran Neovim's own :cquit, \
         which a front-end would restart as a crash."
    );
    let listed = case.probe(&core, &[&format!("buflisted({buf})")]);
    assert_eq!(
        listed,
        ["0"],
        ":cq in a waited window must delete the waited buffer {buf}; 1 means it is still \
         listed, so the buffer was left open."
    );
    core.shutdown().expect("shutdown failed");
}

/// W6: `:cq` in a window no longer showing the waited buffer keeps its meaning: Neovim quits,
/// and the crash ends the wait with `1`.
#[test]
fn cq_elsewhere_quits_nvim_and_exits_1() {
    let case = new_case();
    case.write("msgsix.md", "# six\n");
    let other = case.write("plaina.md", "# plain a\n");
    let (core, _waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgsix.md");
    case.open_waited(&core, &mut wolf);
    send_ex(&core, "e plaina.md");
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        other.display().to_string(),
        "premise: :e plaina.md must replace msgsix.md in the window"
    );

    send_ex(&core, "cq");
    wait_for_liveness(
        &core,
        Liveness::Dead,
        ":cq in a window not showing a waited buffer must quit Neovim; Alive means it took the \
         waited branch anyway",
    );
    wolf.assert_exits(
        1,
        "Neovim quit on :cq",
        "0 means a quit Neovim ended the wait as a close.",
    );
    core.shutdown().expect("shutdown of a dead core failed");
}

/// W7: hiding the waited buffer with `:e other` keeps the command waiting and shows a toast
/// naming the file; a later `:bd` of the hidden buffer ends the wait with `0`.
#[test]
fn hiding_the_buffer_keeps_waiting_with_a_toast() {
    let case = new_case();
    case.write("msgseven.md", "# seven\n");
    let other = case.write("plainb.md", "# plain b\n");
    let (core, waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgseven.md");
    let path = case.open_waited(&core, &mut wolf);
    let (shown, buf, _) = case.current(&core);
    assert_eq!(
        shown,
        path.display().to_string(),
        "premise: msgseven.md must be current"
    );

    send_ex(&core, "e plainb.md");
    let seen = still_waiting_lines_over_settle(&core, &waiter);
    if let Some((status, stderr)) = wolf.exited() {
        panic!(
            "{} must still be waiting after :e plainb.md hid its buffer; it exited {status}, \
             so a hide ended the wait. stderr:\n{stderr}",
            wolf.what
        );
    }
    let line = match seen.into_iter().next() {
        Some(line) => line,
        None => poll_still_waiting(&core, &waiter),
    };
    assert!(
        line.contains("msgseven.md") && !line.contains("plainb.md"),
        "the toast must name the hidden file msgseven.md, not the one now shown ({}); it reads \
         {line:?}",
        other.display()
    );

    send_ex(&core, &format!("bd {buf}"));
    wolf.assert_exits(
        0,
        ":bd of the hidden waited buffer",
        "1 means the hide left the wait in a state a deletion cannot end.",
    );
    core.shutdown().expect("shutdown failed");
}

/// Polls the toast for a still-waiting line, failing after [`FLUSH_TIMEOUT`].
fn poll_still_waiting(core: &WolfCore, waiter: &FlushWaiter) -> String {
    common::poll_until(
        waiter,
        "a toast line saying wolf --wait is still waiting, after a hide",
        || still_waiting_line(core),
        Option::is_some,
    )
    .expect("poll_until returned only once a line was found")
}

/// W8: closing the tab, a core shutdown, ends the wait with `1`.
#[test]
fn closing_the_tab_exits_1() {
    let case = new_case();
    case.write("msgeight.md", "# eight\n");
    let (core, _waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgeight.md");
    let path = case.open_waited(&core, &mut wolf);
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        path.display().to_string(),
        "premise: msgeight.md must be current"
    );

    core.shutdown().expect("shutdown failed");
    wolf.assert_exits(
        1,
        "the tab's core shut down",
        "0 means the tab's close was reported as the buffer closing.",
    );
}

/// W9: Neovim crashing ends the wait with `1` while the core lives on.
#[test]
fn a_crash_exits_1() {
    let case = new_case();
    case.write("msgnine.md", "# nine\n");
    let (core, _waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgnine.md");
    let path = case.open_waited(&core, &mut wolf);
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        path.display().to_string(),
        "premise: msgnine.md must be current"
    );

    send_ex(&core, "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)");
    wait_for_liveness(
        &core,
        Liveness::Dead,
        "premise: Neovim's SIGKILL of itself must be seen as a crash",
    );
    wolf.assert_exits(
        1,
        "Neovim crashed, with the core still running",
        "0 means a crash was reported as the buffer closing.",
    );
    core.shutdown().expect("shutdown of a crashed core failed");
}

/// W10: restarting a stalled Neovim ends the wait with `1`, since the dead Neovim's buffers are
/// gone.
#[test]
fn restarting_a_stalled_nvim_exits_1() {
    let case = new_case();
    case.write("msgten.md", "# ten\n");
    let (core, _waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgten.md");
    let path = case.open_waited(&core, &mut wolf);
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        path.display().to_string(),
        "premise: msgten.md must be current"
    );

    send_ex(
        &core,
        &format!("lua (vim.uv or vim.loop).sleep({WEDGE_SLEEP_MS})"),
    );
    thread::sleep(WEDGE_SETTLE);
    let deadline = Instant::now() + Duration::from_secs(30);
    while core.liveness() != Liveness::NotResponding {
        assert!(
            Instant::now() < deadline,
            "premise: a wedged Neovim must turn NotResponding once an RPC outlasts the budget; \
             liveness is {:?}",
            core.liveness()
        );
        let _ = core.list_buffers();
    }
    assert!(
        matches!(core.restart_nvim(), Ok(true)),
        "premise: restart_nvim on a NotResponding core must respawn Neovim"
    );
    wolf.assert_exits(
        1,
        "the stalled Neovim was restarted",
        "0 means the restart was reported as the buffer closing.",
    );
    core.shutdown().expect("shutdown failed");
}

/// W11: a wait for a file already shown is on that buffer, not on the current one: `:q` in the
/// current window, on another file, leaves it waiting, and `:q` on the file's window ends it.
#[test]
fn a_file_already_shown_is_waited_on_its_buffer() {
    let case = new_case();
    let file = case.write("msgeleven.md", "# eleven\n");
    let other = case.write("plainc.md", "# plain c\n");
    let (core, waiter) = case.start_core();
    core.send_input(InputEvent::Open { path: file.clone() });
    send_ex(&core, "vsplit");
    send_ex(&core, "e plainc.md");
    let shown = case.probe(&core, &["expand('%:p')", "winnr('$')"]);
    assert_eq!(
        shown,
        [other.display().to_string(), "2".to_owned()],
        "premise: the current window of two must show plainc.md, the other msgeleven.md"
    );

    let mut wolf = case.spawn_wait("msgeleven.md");
    let (path, wait) = case.take_wait(&mut wolf);
    assert_eq!(path, file, "premise: the request must name msgeleven.md");
    core.wait_on(&path, wait);

    send_ex(&core, "q");
    wolf.assert_waiting(
        &waiter,
        ":q closed the window on plainc.md",
        "The wait was registered on the current buffer, not on msgeleven.md's.",
    );
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        file.display().to_string(),
        "premise: msgeleven.md's window must be current once the other closed"
    );
    send_ex(&core, "q");
    wolf.assert_exits(
        0,
        ":q on msgeleven.md's window",
        "1 means its close ended the wait as abnormal.",
    );
    core.shutdown().expect("shutdown failed");
}

/// W12: a wait for a path no buffer has ends with `1`.
#[test]
fn a_path_with_no_buffer_exits_1() {
    let case = new_case();
    let (core, _waiter) = case.start_core();
    let mut wolf = case.spawn_wait("nevershown.md");
    let (path, wait) = case.take_wait(&mut wolf);
    core.wait_on(&path, wait);

    wolf.assert_exits(
        1,
        "a wait for a path that no buffer has",
        "0 means a wait with nothing to wait on was reported as closed.",
    );
    core.shutdown().expect("shutdown failed");
}

/// W13: a command that hangs up ends its own wait and no other: hiding its buffer then shows no
/// toast for it and leaves the buffer listed, while another file's wait still ends with `0`.
#[test]
fn a_command_that_hangs_up_ends_only_its_wait() {
    let case = new_case();
    case.write("msgfox.md", "# fox\n");
    case.write("msggolf.md", "# golf\n");
    case.write("plaind.md", "# plain d\n");
    case.write("plaine.md", "# plain e\n");
    let (core, waiter) = case.start_core();

    let mut fox = case.spawn_wait("msgfox.md");
    let fox_path = case.open_waited(&core, &mut fox);
    let (shown, fox_buf, fox_win) = case.current(&core);
    assert_eq!(
        shown,
        fox_path.display().to_string(),
        "premise: msgfox.md must be current"
    );
    send_ex(&core, "vsplit");
    let mut golf = case.spawn_wait("msggolf.md");
    let golf_path = case.open_waited(&core, &mut golf);
    let (shown, golf_buf, golf_win) = case.current(&core);
    assert_eq!(
        shown,
        golf_path.display().to_string(),
        "premise: msggolf.md must be current in the split"
    );

    fox.child.kill().expect("failed to kill msgfox.md's wolf");
    fox.child.wait().expect("failed to reap msgfox.md's wolf");
    core.focus_win(fox_win.parse().expect("win_getid() is a number"));
    send_ex(&core, "e plaind.md");
    let seen = still_waiting_lines_over_settle(&core, &waiter);
    assert!(
        seen.iter().all(|line| !line.contains("msgfox.md")),
        "msgfox.md's command hung up, so hiding msgfox.md must show no still-waiting toast for \
         it; the toast showed {seen:?}, so its dead wait was taken as live."
    );
    let listed = case.probe(&core, &[&format!("buflisted({fox_buf})")]);
    assert_eq!(
        listed,
        ["1"],
        "a hang-up must leave msgfox.md's buffer {fox_buf} listed; 0 means it was deleted."
    );

    core.focus_win(golf_win.parse().expect("win_getid() is a number"));
    send_ex(&core, "e plaine.md");
    let line = poll_still_waiting(&core, &waiter);
    assert!(
        line.contains("msggolf.md"),
        "control: hiding msggolf.md, whose command still waits, must toast it; the line reads \
         {line:?}"
    );
    send_ex(&core, &format!("b {golf_buf}"));
    send_ex(&core, "q");
    golf.assert_exits(
        0,
        ":q closed msggolf.md's last window",
        "1 means msgfox.md's hang-up ended msggolf.md's wait too.",
    );
    core.shutdown().expect("shutdown failed");
}

/// W14: `git commit` with `GIT_EDITOR="wolf --wait"` commits the message typed and written with
/// `:wq` on the tab's only window.
#[test]
fn git_commit_lands_the_typed_message() {
    const MESSAGE: &str = "Land the message typed in Wolf";
    let case = new_case();
    let repo = case.repo();
    let (core, _waiter) = case.start_core();
    let mut commit = case.spawn_commit(&repo);
    let path = case.open_waited(&core, &mut commit);
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        path.display().to_string(),
        "premise: git's message file must be current"
    );

    type_first_line(&core, MESSAGE);
    send_ex(&core, "wq");
    shut_down_on_close_window(core, ":wq");
    let (status, stderr) = commit.exit_within(":wq on the message file");
    assert!(
        status.success(),
        "git commit must succeed once its editor exits 0; it exited {status}, so wolf --wait \
         exited non-zero. stderr:\n{stderr}"
    );
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s"]),
        MESSAGE,
        "the new commit's subject must be the typed line; seed means no commit was made."
    );
}

/// W15: a commit whose written message is abandoned with `:cq` makes no commit, and Neovim
/// keeps running.
#[test]
fn git_commit_aborted_with_cq_makes_no_commit() {
    const MESSAGE: &str = "Abandon the message typed in Wolf";
    let case = new_case();
    let repo = case.repo();
    let (core, waiter) = case.start_core();
    let mut commit = case.spawn_commit(&repo);
    let path = case.open_waited(&core, &mut commit);
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        path.display().to_string(),
        "premise: git's message file must be current"
    );

    type_first_line(&core, MESSAGE);
    send_ex(&core, "w");
    common::poll_until(
        &waiter,
        "premise: :w must write the typed message, so only the exit status stops the commit",
        || std::fs::read_to_string(&path).unwrap_or_default(),
        |text| text.starts_with(MESSAGE),
    );
    send_ex(&core, "cq");
    let (status, stderr) = commit.exit_within(":cq on the message file");
    assert!(
        !status.success(),
        "git commit must fail once its editor exits 1 on :cq; it succeeded, so :cq ended the \
         wait with 0. stderr:\n{stderr}"
    );
    assert_eq!(
        (
            git(&repo, &["rev-list", "--count", "HEAD"]),
            git(&repo, &["log", "-1", "--format=%s"]),
        ),
        ("1".to_owned(), "seed".to_owned()),
        "an aborted commit must leave the repository at its seed commit."
    );
    settle_for(&waiter, SETTLE);
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        ":cq on a waited buffer must not quit Neovim."
    );
    core.shutdown().expect("shutdown failed");
}

/// W16: normal-mode `ZZ` on the tab's only window writes the typed line and ends the wait with
/// `0`, as `:wq` does in W2, rather than quitting Neovim.
#[test]
fn zz_on_the_last_pane_exits_0() {
    const TEXT: &str = "typed before ZZ on the last pane";
    let case = new_case();
    let file = case.write("msgsixteen.md", "# sixteen\n");
    let (core, _waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgsixteen.md");
    case.open_waited(&core, &mut wolf);
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        file.display().to_string(),
        "premise: msgsixteen.md must be current"
    );

    type_first_line(&core, TEXT);
    type_str(&core, "ZZ");
    shut_down_on_close_window(core, "ZZ");
    wolf.assert_exits(
        0,
        "ZZ on the tab's last pane, then the tab's shutdown",
        "1 means the wait ended as abnormal: Neovim quit on ZZ, or the shutdown came first.",
    );
    let written = std::fs::read_to_string(&file).expect("failed to read msgsixteen.md back");
    assert!(
        written.starts_with(TEXT),
        "ZZ on a modified buffer must have written the typed line to msgsixteen.md; it holds \
         {written:?}"
    );
}

/// W17: normal-mode `ZQ` on the tab's only window discards the typed line and ends the wait
/// with `0`, rather than quitting Neovim.
#[test]
fn zq_on_the_last_pane_exits_0() {
    const TEXT: &str = "typed before ZQ on the last pane";
    let case = new_case();
    let file = case.write("msgseventeen.md", "# seventeen\n");
    let (core, _waiter) = case.start_core();
    let mut wolf = case.spawn_wait("msgseventeen.md");
    case.open_waited(&core, &mut wolf);
    let (shown, _, _) = case.current(&core);
    assert_eq!(
        shown,
        file.display().to_string(),
        "premise: msgseventeen.md must be current"
    );

    type_first_line(&core, TEXT);
    type_str(&core, "ZQ");
    shut_down_on_close_window(core, "ZQ");
    wolf.assert_exits(
        0,
        "ZQ on the tab's last pane, then the tab's shutdown",
        "1 means the wait ended as abnormal: Neovim quit on ZQ, or the shutdown came first.",
    );
    let written = std::fs::read_to_string(&file).expect("failed to read msgseventeen.md back");
    assert_eq!(
        written, "# seventeen\n",
        "ZQ must discard the typed line and write nothing to msgseventeen.md"
    );
}
