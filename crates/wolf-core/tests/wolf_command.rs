//! Tests of the real `wolf` binary against the invocation table and exit statuses of
//! `specs/cli-and-file-open.md` §1, over the channel of §2. The app is either a real
//! `OpenListener` or a raw `UnixListener` that records what it is sent and answers with scripted
//! `OpenReply` lines, for replies a real listener never sends, such as a status other than 0 or 1.
//!
//! The binary is Cargo's `CARGO_BIN_EXE_wolf`, or under Bazel the `:wolf` runfile named by
//! `WOLF_TEST_WOLF_BIN`. Each test works in its own case directory under one kept, canonical
//! base the binary changes into once ([`env`]): socket paths are named relative to it, because
//! no absolute one under Bazel's `TMPDIR` fits 103 bytes. Each `wolf` runs in its case's `work`
//! directory with `WOLF_STATE_DIR=../s`, relative too, and with no `WOLF_TAB_ID` unless a test
//! sets one.
//!
//! Every test runs a copy of the binary at `bin/wolf` in the base, never the build's own: on
//! Linux the command starts the `wolf-gnome` beside it when nothing listens, and Cargo builds
//! the real app beside `CARGO_BIN_EXE_wolf`. The launcher's test puts a fake one beside a copy
//! of its own.
//!
//! A refusal must exit 2 before connecting. Exit 2 alone cannot show that, since a `wolf` that
//! connects and gets no answer also exits 2, so the raw listener checks that nobody connected.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use wolf_core::{OpenListener, OpenReply, OpenRequest, OpenTarget, TabId, TakenRequest, WakeFn};
use wolf_test_support::resolve_test_runfile;

/// The variable Bazel names the `wolf` runfile in.
const WOLF_BIN_ENV: &str = "WOLF_TEST_WOLF_BIN";

/// How long a `wolf` run may take before it is killed and the test fails.
const RUN_WAIT: Duration = Duration::from_secs(30);

/// How long the raw listener waits for a connection or a request line.
const SERVE_WAIT: Duration = Duration::from_secs(30);

/// The state dir every `wolf` is given, relative to its case's `work` directory.
const STATE_FROM_WORK: &str = "../s";

/// The bytes a 103-byte socket path leaves for its state dir.
const DIR_AT_LIMIT: usize = 103 - 1 - wolf_core::OPEN_SOCKET_NAME.len();

struct Env {
    /// The copy at `bin/wolf` in the base, with no `wolf-gnome` beside it.
    bin: PathBuf,
    base: PathBuf,
}

/// The binary, copied into the base dir, and the base dir, entered once per test binary.
fn env() -> &'static Env {
    static ENV: OnceLock<Env> = OnceLock::new();
    ENV.get_or_init(|| {
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
            .prefix("wolf-cmd")
            .tempdir()
            .expect("failed to create the base temp dir")
            .keep();
        let base = std::fs::canonicalize(&kept).expect("failed to canonicalize the base dir");
        std::env::set_current_dir(&base).expect("failed to enter the base dir");
        let bin = copy_wolf(&built, &base.join("bin"));
        Env { bin, base }
    })
}

/// Copies `built` to `dir/wolf`, creating `dir`, and returns the copy.
fn copy_wolf(built: &Path, dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).expect("failed to create the binary's dir");
    let copy = dir.join("wolf");
    std::fs::copy(built, &copy).unwrap_or_else(|e| {
        panic!(
            "copying the wolf binary {} to {} failed: {e}",
            built.display(),
            copy.display()
        )
    });
    copy
}

/// One test's directories: `c<n>/work`, where `wolf` runs, and `c<n>/s`, its state dir.
struct Case {
    /// `c<n>/s`, relative to the base, as this process names it.
    state: PathBuf,
    /// The absolute, canonical `work` directory.
    work: PathBuf,
}

fn new_case() -> Case {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let base = &env().base;
    let rel = PathBuf::from(format!("c{}", COUNTER.fetch_add(1, Ordering::Relaxed)));
    let work = base.join(&rel).join("work");
    std::fs::create_dir_all(&work).expect("failed to create the case's work dir");
    Case {
        state: rel.join("s"),
        work,
    }
}

impl Case {
    /// A real listener on the case's state dir.
    fn listen(&self) -> OpenListener {
        let wake: WakeFn = Arc::new(|| {});
        OpenListener::bind(&self.state, wake)
            .unwrap_or_else(|e| panic!("OpenListener::bind failed: {e:?}"))
    }

    /// A raw listener on the case's socket path.
    fn raw(&self) -> UnixListener {
        std::fs::create_dir_all(&self.state).expect("failed to create the state dir");
        UnixListener::bind(self.state.join(wolf_core::OPEN_SOCKET_NAME))
            .expect("failed to bind the raw listener")
    }

    /// Starts `wolf args` in `work` with the case's state dir and no `WOLF_TAB_ID`.
    fn spawn(&self, args: &[&str]) -> Child {
        let mut cmd = Command::new(&env().bin);
        cmd.args(args)
            .current_dir(&self.work)
            .env("WOLF_STATE_DIR", STATE_FROM_WORK)
            .env_remove("WOLF_TAB_ID");
        spawn_piped(cmd, &format!("wolf {args:?}"))
    }

    /// Runs `wolf args` in `work` with the case's state dir.
    fn run(&self, args: &[&str], tab_id: Option<&str>) -> Output {
        let mut cmd = Command::new(&env().bin);
        cmd.args(args)
            .current_dir(&self.work)
            .env("WOLF_STATE_DIR", STATE_FROM_WORK);
        match tab_id {
            Some(value) => cmd.env("WOLF_TAB_ID", value),
            None => cmd.env_remove("WOLF_TAB_ID"),
        };
        run_to_end(cmd, &format!("wolf {args:?}"))
    }
}

/// Runs `cmd` with piped output, killing it and failing after [`RUN_WAIT`].
fn run_to_end(cmd: Command, what: &str) -> Output {
    wait_to_end(spawn_piped(cmd, what), what)
}

/// Spawns `cmd` with stdin null and piped output.
fn spawn_piped(mut cmd: Command, what: &str) -> Child {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {what}: {e}"))
}

/// Waits for `child` and collects its output, killing it and failing after [`RUN_WAIT`].
fn wait_to_end(mut child: Child, what: &str) -> Output {
    let deadline = Instant::now() + RUN_WAIT;
    while child.try_wait().expect("try_wait failed").is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("{what} was still running after {RUN_WAIT:?}, so it hangs on its reply.");
        }
        thread::sleep(Duration::from_millis(20));
    }
    child
        .wait_with_output()
        .expect("failed to collect the output")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Asserts `out` exited with `code`, naming `what` and showing stderr.
fn assert_exit(out: &Output, code: i32, what: &str) {
    assert_eq!(
        out.status.code(),
        Some(code),
        "{what} must exit {code}; it exited {:?} (None is a signal, 101 a panic). stderr:\n{}",
        out.status,
        stderr(out)
    );
}

/// Asserts `out` exited 0 with nothing on stdout.
fn assert_taken_exit(out: &Output, what: &str) {
    assert_exit(out, 0, what);
    assert!(
        out.stdout.is_empty(),
        "{what} must print nothing on stdout; it printed {:?}.",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// Asserts `out` is a refusal: exit 2 with a message on stderr.
fn assert_refusal(out: &Output, what: &str) {
    assert_exit(out, 2, what);
    assert!(
        !stderr(out).trim().is_empty(),
        "{what} must say why on stderr; stderr is empty."
    );
}

/// Asserts nobody connected to `listener` since it was bound.
fn assert_no_connection(listener: &UnixListener, what: &str) {
    listener
        .set_nonblocking(true)
        .expect("set_nonblocking failed");
    match listener.accept() {
        Err(e) if e.kind() == ErrorKind::WouldBlock => {}
        Err(e) => panic!("accept on the raw listener failed: {e}"),
        Ok(_) => panic!(
            "{what} connected to the app; a refusal must exit 2 before connecting, and this one \
             exited 2 only because no reply came."
        ),
    }
}

/// Serves one connection on `listener`: reads the request, writes `replies` as JSON lines, and
/// closes. Returns the request, or `None` when nobody connected within [`SERVE_WAIT`].
fn serve_once(listener: UnixListener, replies: Vec<OpenReply>) -> JoinHandle<Option<OpenRequest>> {
    thread::spawn(move || {
        listener
            .set_nonblocking(true)
            .expect("set_nonblocking failed");
        let deadline = Instant::now() + SERVE_WAIT;
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == ErrorKind::WouldBlock && Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return None,
            }
        };
        // An accepted socket inherits the listener's O_NONBLOCK on macOS.
        stream
            .set_nonblocking(false)
            .expect("set_nonblocking failed");
        stream
            .set_read_timeout(Some(SERVE_WAIT))
            .expect("set_read_timeout failed");
        let mut line = String::new();
        BufReader::new(&stream)
            .read_line(&mut line)
            .expect("reading the request line failed");
        let request: OpenRequest = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("wolf sent {line:?}, which is not an OpenRequest: {e}"));
        for reply in replies {
            let text = serde_json::to_string(&reply).expect("an OpenReply serializes");
            // wolf may already have hung up after a final reply.
            let _ = stream.write_all(format!("{text}\n").as_bytes());
        }
        Some(request)
    })
}

fn served(handle: JoinHandle<Option<OpenRequest>>, what: &str) -> OpenRequest {
    handle
        .join()
        .expect("the raw listener's thread panicked")
        .unwrap_or_else(|| panic!("{what} never connected to the app."))
}

fn plain(path: Option<PathBuf>, tab_id: Option<TabId>) -> OpenRequest {
    OpenRequest {
        path,
        wait: false,
        tab_id,
    }
}

#[test]
fn wolf_alone_activates() {
    let case = new_case();
    let listener = case.listen();
    let out = case.run(&[], None);
    assert_taken_exit(&out, "wolf with no arguments");
    assert_eq!(
        listener.drain_requests(),
        vec![plain(None, None)],
        "wolf with no arguments must send a request with no path, no wait and no tab ID; a path \
         means it sent its working directory."
    );
}

#[test]
fn a_relative_path_resolves_against_the_working_directory() {
    let case = new_case();
    let listener = case.listen();
    std::fs::create_dir(case.work.join("proj")).expect("failed to create proj");

    let out = case.run(&["proj"], None);
    assert_taken_exit(&out, "wolf proj");
    let out = case.run(&["notes/a.md"], None);
    assert_taken_exit(&out, "wolf notes/a.md");

    let drained = listener.drain_requests();
    assert_eq!(
        drained,
        vec![
            plain(Some(case.work.join("proj")), None),
            plain(Some(case.work.join("notes/a.md")), None),
        ],
        "a relative path must be sent made absolute against wolf's working directory {}; a \
         relative path means it was sent unresolved.",
        case.work.display()
    );
    assert_eq!(
        drained[0].target(),
        OpenTarget::Directory(case.work.join("proj")),
        "an existing directory is a Directory."
    );
    assert_eq!(
        drained[1].target(),
        OpenTarget::File(case.work.join("notes/a.md")),
        "a path that does not exist is a File."
    );
}

#[test]
fn wolf_tab_id_is_sent_when_well_formed() {
    let case = new_case();
    let listener = case.listen();
    let id = TabId::random();
    let path = Some(case.work.join("f.md"));

    let rows: [(&str, Option<String>, Option<TabId>); 4] = [
        ("a well-formed WOLF_TAB_ID", Some(id.to_string()), Some(id)),
        (
            "an uppercase WOLF_TAB_ID",
            Some(id.to_string().to_uppercase()),
            Some(id),
        ),
        (
            "a malformed WOLF_TAB_ID",
            Some("not-a-tab-id".to_owned()),
            None,
        ),
        ("no WOLF_TAB_ID", None, None),
    ];
    for (what, value, expected) in rows {
        let out = case.run(&["f.md"], value.as_deref());
        assert_taken_exit(&out, &format!("wolf f.md with {what}"));
        assert_eq!(
            listener.drain_requests(),
            vec![plain(path.clone(), expected)],
            "wolf f.md with {what} must send tab_id {expected:?}: a well-formed value is sent \
             parsed, a malformed one is dropped as if unset."
        );
    }
}

#[test]
fn refused_invocations_exit_2_before_connecting() {
    let rows: [&[&str]; 7] = [
        &["--wait"],
        &["--wait", "proj"],
        &["a.md", "b.md"],
        &["--bogus"],
        &["-"],
        &[""],
        &["--wait", "--wait", "f.md"],
    ];
    for args in rows {
        let case = new_case();
        std::fs::create_dir(case.work.join("proj")).expect("failed to create proj");
        let raw = case.raw();
        let what = format!("wolf {args:?}");
        let out = case.run(args, None);
        assert_refusal(&out, &what);
        assert_no_connection(&raw, &what);
    }
}

#[test]
fn double_dash_ends_the_options() {
    let case = new_case();
    let listener = case.listen();
    let out = case.run(&["--", "-x.md"], None);
    assert_taken_exit(&out, "wolf -- -x.md");
    let out = case.run(&["--", "--wait"], None);
    assert_taken_exit(&out, "wolf -- --wait");
    assert_eq!(
        listener.drain_requests(),
        vec![
            plain(Some(case.work.join("-x.md")), None),
            plain(Some(case.work.join("--wait")), None),
        ],
        "after --, an argument starting with - is a path."
    );
}

#[test]
fn nothing_listening_exits_2() {
    let sibling = env().bin.with_file_name("wolf-gnome");
    assert!(
        !sibling.exists(),
        "the premise fails: {} exists beside the copied wolf, so on Linux wolf would start it \
         instead of saying Wolf is not running.",
        sibling.display()
    );
    let case = new_case();
    std::fs::create_dir_all(&case.state).expect("failed to create the state dir");
    let out = case.run(&["f.md"], None);
    assert_refusal(&out, "wolf f.md with no socket");
    assert!(
        stderr(&out).to_lowercase().contains("not running"),
        "with nothing listening, wolf must say Wolf is not running; stderr reads:\n{}",
        stderr(&out)
    );

    let stale = new_case();
    drop(stale.raw());
    let out = stale.run(&["f.md"], None);
    assert_refusal(&out, "wolf f.md with a stale socket");
    assert!(
        stderr(&out).to_lowercase().contains("not running"),
        "with a stale socket, wolf must say Wolf is not running; stderr reads:\n{}",
        stderr(&out)
    );
}

/// The variable naming the file the fake `wolf-gnome` appends its launch record to.
#[cfg(target_os = "linux")]
const LAUNCH_LOG_ENV: &str = "WOLF_TEST_LAUNCH_LOG";

/// A stand-in for the app: one record of four lines per launch, written by one `printf`.
#[cfg(target_os = "linux")]
const FAKE_GNOME: &str = r#"#!/bin/sh
printf '%s\n%s\n%s\n%s\n' "$(pwd -P)" "${WOLF_STATE_DIR-}" "${WOLF_TAB_ID-unset}" "$#" \
  >> "$WOLF_TEST_LAUNCH_LOG"
"#;

/// The lines of the launch log at `log` once they make whole records, waiting a little for
/// the last `printf` to land.
#[cfg(target_os = "linux")]
fn launch_records(log: &Path) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let whole = text.ends_with('\n') && !lines.is_empty() && lines.len().is_multiple_of(4);
        if whole || Instant::now() >= deadline {
            return lines;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_missing_listener_starts_the_wolf_gnome_beside_it() {
    use std::os::unix::fs::PermissionsExt;

    let case = new_case();
    let case_dir = case.work.parent().expect("a case's work dir has a parent");
    let wolf = copy_wolf(&env().bin, &case_dir.join("bin"));
    let gnome = wolf.with_file_name("wolf-gnome");
    std::fs::write(&gnome, FAKE_GNOME).expect("failed to write the fake wolf-gnome");
    std::fs::set_permissions(&gnome, std::fs::Permissions::from_mode(0o755))
        .expect("failed to make the fake wolf-gnome executable");
    let log = case_dir.join("launch.log");
    let id = TabId::random();

    let what = "wolf f.md with nothing listening and a wolf-gnome beside it";
    let mut cmd = Command::new(&wolf);
    cmd.arg("f.md")
        .current_dir(&case.work)
        .env("WOLF_STATE_DIR", STATE_FROM_WORK)
        .env("WOLF_TAB_ID", id.to_string())
        .env(LAUNCH_LOG_ENV, &log);
    let mut child = spawn_piped(cmd, what);
    let deadline = Instant::now() + RUN_WAIT;
    while !log.exists() {
        if child.try_wait().expect("try_wait failed").is_some() {
            let out = wait_to_end(child, what);
            panic!(
                "{what} exited {:?} without starting the wolf-gnome beside it, so Linux has no \
                 launcher, or it applies none under a non-empty WOLF_STATE_DIR. stderr:\n{}",
                out.status,
                stderr(&out)
            );
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("{what} started no wolf-gnome within {RUN_WAIT:?}.");
        }
        thread::sleep(Duration::from_millis(20));
    }

    // The launched app listens where the command connects, as the real one would.
    let listener = case.listen();
    let out = wait_to_end(child, what);
    assert_taken_exit(&out, what);
    assert_eq!(
        listener.drain_requests(),
        vec![plain(Some(case.work.join("f.md")), Some(id))],
        "{what} must send its request, with its own tab ID, to the listener bound after the \
         launch; an empty queue means it gave up before retrying."
    );

    let lines = launch_records(&log);
    assert_eq!(
        lines.len(),
        4,
        "the fake wolf-gnome must have been started once, writing one four-line record; it \
         wrote {lines:?}. More means wolf started it again on a retry."
    );
    assert_eq!(
        lines[0],
        "/",
        "the app must start in /, so its restore adds no window for the command's directory; \
         it started in {:?}, the command's working directory if that is {}.",
        lines[0],
        case.work.display()
    );
    let state = Path::new(&lines[1]);
    assert!(
        state.is_absolute(),
        "the app must get WOLF_STATE_DIR made absolute, since it starts in another directory; \
         it got {:?}, unchanged if that is {STATE_FROM_WORK}.",
        lines[1]
    );
    let expected = std::fs::canonicalize(env().base.join(&case.state))
        .expect("the case's state dir exists once bound");
    assert_eq!(
        std::fs::canonicalize(state).ok().as_deref(),
        Some(expected.as_path()),
        "the app's WOLF_STATE_DIR {:?} must name the command's state dir, resolved against the \
         command's working directory and not the app's.",
        lines[1]
    );
    assert_eq!(
        lines[2], "unset",
        "the app belongs to no tab, so WOLF_TAB_ID must be removed from its environment; it got \
         {:?}, the command's {id} if they match.",
        lines[2]
    );
    assert_eq!(
        lines[3], "0",
        "the app must be started with no arguments, since requests come over the socket; it got \
         {} arguments.",
        lines[3]
    );
}

#[test]
fn an_over_long_socket_path_exits_2_naming_the_limit() {
    let base = &env().base;
    let name = |len: usize, fill: char| {
        let mut name = String::from("wolfstate");
        while name.len() < len {
            name.push(fill);
        }
        name
    };
    let run_in_base = |state: &str, args: &[&str]| {
        let mut cmd = Command::new(&env().bin);
        cmd.args(args)
            .current_dir(base)
            .env("WOLF_STATE_DIR", state)
            .env_remove("WOLF_TAB_ID");
        run_to_end(cmd, &format!("wolf {args:?} with WOLF_STATE_DIR={state}"))
    };

    // The control: a 103-byte socket path works end to end.
    let at_limit = name(DIR_AT_LIMIT, 'y');
    let listener = OpenListener::bind(Path::new(&at_limit), Arc::new(|| {}))
        .unwrap_or_else(|e| panic!("binding a 103-byte socket path failed: {e:?}"));
    let out = run_in_base(&at_limit, &["/abs/limit.md"]);
    assert_taken_exit(&out, "wolf with a 103-byte socket path");
    assert_eq!(
        listener.drain_requests(),
        vec![plain(Some(PathBuf::from("/abs/limit.md")), None)]
    );

    let over = name(DIR_AT_LIMIT + 1, 'z');
    let out = run_in_base(&over, &["/abs/over.md"]);
    assert_refusal(&out, "wolf with a 104-byte socket path");
    let message = stderr(&out)
        .replace(&base.display().to_string(), "")
        .replace(&over, "");
    assert!(
        message.contains("103"),
        "wolf must name the 103-byte limit when the socket path is over it; stderr, without the \
         paths, reads:\n{message}"
    );
}

#[test]
fn wait_outcomes_set_the_exit_status() {
    let rows: [(&str, Vec<OpenReply>, i32); 5] = [
        (
            "Taken, WaitEnded 0",
            vec![OpenReply::Taken, OpenReply::WaitEnded { status: 0 }],
            0,
        ),
        (
            "Taken, WaitEnded 1",
            vec![OpenReply::Taken, OpenReply::WaitEnded { status: 1 }],
            1,
        ),
        (
            "Taken, WaitEnded 3",
            vec![OpenReply::Taken, OpenReply::WaitEnded { status: 3 }],
            1,
        ),
        ("Taken, then EOF", vec![OpenReply::Taken], 1),
        (
            "Refused",
            vec![OpenReply::Refused {
                message: "no waiting here".to_owned(),
            }],
            2,
        ),
    ];
    for (what, replies, code) in rows {
        let case = new_case();
        let server = serve_once(case.raw(), replies);
        let out = case.run(&["--wait", "f.md"], None);
        let request = served(server, &format!("wolf --wait f.md answered {what}"));
        assert_eq!(
            request,
            OpenRequest {
                path: Some(case.work.join("f.md")),
                wait: true,
                tab_id: None,
            },
            "wolf --wait f.md must send its absolute path with wait set."
        );
        assert_exit(
            &out,
            code,
            &format!(
                "wolf --wait f.md answered {what} (0 for a wait that ended 0, 1 for any other \
                 end or a close after Taken, 2 for a refusal)"
            ),
        );
        if code == 2 {
            assert!(
                stderr(&out).contains("no waiting here"),
                "a refusal's message must reach stderr; it reads:\n{}",
                stderr(&out)
            );
        }
    }
}

#[test]
fn no_taken_reply_exits_2() {
    let marker = "refusal marker";
    let rows: [(&str, Vec<OpenReply>); 2] = [
        ("a close before any reply", vec![]),
        (
            "Refused",
            vec![OpenReply::Refused {
                message: marker.to_owned(),
            }],
        ),
    ];
    for (what, replies) in rows {
        let refused = !replies.is_empty();
        let case = new_case();
        let server = serve_once(case.raw(), replies);
        let out = case.run(&["f.md"], None);
        let what = format!("wolf f.md answered {what}");
        served(server, &what);
        assert_refusal(&out, &what);
        if refused {
            assert!(
                stderr(&out).contains(marker),
                "a refusal's message must reach stderr; it reads:\n{}",
                stderr(&out)
            );
        }
    }
}

/// The one request `listener` takes from the running `child`, polled for up to [`RUN_WAIT`]. Kills
/// `child` and fails when it exits first or nothing is queued in time.
fn drain_wait(listener: &OpenListener, child: &mut Child, what: &str) -> TakenRequest {
    let deadline = Instant::now() + RUN_WAIT;
    loop {
        let mut taken = listener.drain_taken();
        if !taken.is_empty() {
            assert_eq!(
                taken.len(),
                1,
                "{what} must queue one request; the drain holds {taken:?}."
            );
            return taken.remove(0);
        }
        if let Some(status) = child.try_wait().expect("try_wait failed") {
            let mut err = String::new();
            if let Some(mut pipe) = child.stderr.take() {
                let _ = std::io::Read::read_to_string(&mut pipe, &mut err);
            }
            panic!(
                "{what} exited {status} before its request was queued; 2 means the listener \
                 refused the wait. stderr:\n{err}"
            );
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("{what} was taken by nothing within {RUN_WAIT:?}.");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_wait_against_the_listener_exits_with_its_end() {
    let case = new_case();
    let listener = case.listen();
    let rows: [(&str, Option<i32>, i32); 2] = [
        ("a wait the app ends with end(0)", Some(0), 0),
        ("a wait the app drops", None, 1),
    ];
    for (what, end, code) in rows {
        let what = format!("wolf --wait f.md, {what},");
        let mut child = case.spawn(&["--wait", "f.md"]);
        let taken = drain_wait(&listener, &mut child, &what);
        assert_eq!(
            taken.request,
            OpenRequest {
                path: Some(case.work.join("f.md")),
                wait: true,
                tab_id: None,
            },
            "{what} must send its absolute path with wait set."
        );
        let Some(wait) = taken.wait else {
            let _ = child.kill();
            panic!(
                "{what} must drain holding its OpenWait; None means the listener queued it as a \
                 plain request, so nothing can end the wait."
            );
        };
        match end {
            Some(status) => wait.end(status),
            None => drop(wait),
        }
        let out = wait_to_end(child, &what);
        assert_exit(
            &out,
            code,
            &format!(
                "{what} (0 only for a wait ended with status 0; 1 for a dropped wait, whose Drop \
                 answers 1 or closes the connection)"
            ),
        );
    }
}
