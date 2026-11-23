//! Headless tests of the open channel of `specs/cli-and-file-open.md` §2 over real Unix-domain
//! sockets: `open_socket_path` and its 103-byte limit, `OpenListener` taking, refusing and
//! queueing requests and waking the front-end, its socket's lifecycle, and `OpenClient`'s
//! connect and connect-or-launch.
//!
//! ## Relative socket paths
//!
//! Bazel's test `TMPDIR` is about 120 bytes inside the macOS gate, so no absolute socket path
//! under it fits 103 bytes. The binary therefore changes into a kept, canonical temp dir once,
//! before any test touches a socket ([`base`]), and every test names its state dir relative to
//! it. The listener and the client measure a path as given, so a relative one is allowed.
//!
//! ## Premises about the components
//!
//! - Rust's runtime ignores `SIGPIPE`, so a write the listener stops reading fails with `EPIPE`
//!   rather than killing the binary, as `sigpipe.rs` states.
//! - A path of 103 bytes binds and connects on both platforms: macOS's `sun_path` holds 104
//!   bytes with the terminating NUL. [`socket_paths_over_103_bytes_are_refused`] measures it.
//! - A socket file outlives the `std` listener that bound it, which is what a crashed app leaves
//!   behind; [`a_stale_socket_is_replaced`] asserts it.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use wolf_core::{
    open_socket_path, OpenChannelError, OpenClient, OpenListener, OpenReply, OpenRequest, TabId,
    WakeFn, OPEN_SOCKET_NAME,
};

/// How long a test waits for a reply before failing rather than hanging.
const REPLY_WAIT: Duration = Duration::from_secs(10);

/// The timeout `connect_or_launch` gets where it must not run out.
const LAUNCH_WAIT: Duration = Duration::from_secs(5);

/// How long the launched "app" takes to bind, so the client must retry.
const LAUNCH_DELAY: Duration = Duration::from_millis(200);

/// The bytes a 103-byte socket path leaves for its state dir: the separator and
/// [`OPEN_SOCKET_NAME`] take the rest.
const DIR_AT_LIMIT: usize = 103 - 1 - OPEN_SOCKET_NAME.len();

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The canonical temp dir the binary runs in, made and entered once. Every test calls it, through
/// [`fresh_state`] or [`dir_of_len`], before it names a relative path.
fn base() -> &'static Path {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| {
        let kept = tempfile::Builder::new()
            .prefix("wolf-open")
            .tempdir()
            .expect("failed to create the base temp dir")
            .keep();
        let dir = std::fs::canonicalize(&kept).expect("failed to canonicalize the base dir");
        std::env::set_current_dir(&dir).expect("failed to enter the base dir");
        dir
    })
}

/// A fresh state dir, relative to [`base`], that does not exist yet.
fn fresh_state() -> PathBuf {
    base();
    PathBuf::from(format!("s{}", COUNTER.fetch_add(1, Ordering::Relaxed)))
}

/// A fresh relative state dir whose name is `len` bytes, that does not exist yet.
fn dir_of_len(len: usize) -> PathBuf {
    base();
    let mut name = format!("long{}-", COUNTER.fetch_add(1, Ordering::Relaxed));
    while name.len() < len {
        name.push('x');
    }
    assert_eq!(name.len(), len);
    PathBuf::from(name)
}

/// A wake that counts its calls.
fn counting_wake() -> (WakeFn, Arc<AtomicUsize>) {
    let count = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&count);
    let wake: WakeFn = Arc::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    });
    (wake, count)
}

fn noop_wake() -> WakeFn {
    Arc::new(|| {})
}

fn bind(state: &Path) -> OpenListener {
    OpenListener::bind(state, noop_wake())
        .unwrap_or_else(|e| panic!("OpenListener::bind({}) failed: {e:?}", state.display()))
}

fn request(path: &str) -> OpenRequest {
    OpenRequest {
        path: Some(PathBuf::from(path)),
        wait: false,
        tab_id: Some(TabId::random()),
    }
}

/// The error of `result`, failing with `what` when it is `Ok`.
fn err_of<T>(result: Result<T, OpenChannelError>, what: &str) -> OpenChannelError {
    match result {
        Ok(_) => panic!("{what} succeeded where it must fail"),
        Err(e) => e,
    }
}

/// Connects a client to `state`, sends `req`, and returns its first reply.
fn round_trip(state: &Path, req: &OpenRequest) -> OpenReply {
    let mut client = OpenClient::connect(state)
        .unwrap_or_else(|e| panic!("OpenClient::connect({}) failed: {e:?}", state.display()));
    client
        .set_read_timeout(Some(REPLY_WAIT))
        .expect("set_read_timeout failed");
    client.send(req).expect("send failed");
    client
        .read_reply()
        .expect("read_reply failed, such as by timing out with no reply")
        .expect("the listener closed the connection without a reply")
}

/// Writes `line` as raw bytes to the socket at `socket` and returns the reply line, parsed.
fn raw_round_trip(socket: &Path, line: &str) -> OpenReply {
    let mut stream = UnixStream::connect(socket)
        .unwrap_or_else(|e| panic!("failed to connect to {}: {e}", socket.display()));
    stream
        .set_read_timeout(Some(REPLY_WAIT))
        .expect("set_read_timeout failed");
    stream
        .write_all(line.as_bytes())
        .expect("failed to write the raw request");
    read_reply_line(stream)
}

fn read_reply_line(stream: UnixStream) -> OpenReply {
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .expect("reading the reply failed, such as by timing out with no reply");
    serde_json::from_str(&reply)
        .unwrap_or_else(|e| panic!("the reply {reply:?} is not one JSON OpenReply line: {e}"))
}

fn assert_taken(reply: &OpenReply, what: &str) {
    assert!(
        matches!(reply, OpenReply::Taken),
        "{what} must be answered Taken; got {reply:?}."
    );
}

fn assert_refused(reply: &OpenReply, what: &str) {
    assert!(
        matches!(reply, OpenReply::Refused { message } if !message.is_empty()),
        "{what} must be answered Refused with a message; got {reply:?}."
    );
}

#[test]
fn socket_paths_over_103_bytes_are_refused() {
    let at_limit = dir_of_len(DIR_AT_LIMIT);
    let path = open_socket_path(&at_limit)
        .unwrap_or_else(|e| panic!("a 103-byte socket path must be allowed: {e:?}"));
    assert_eq!(path, at_limit.join(OPEN_SOCKET_NAME));
    assert_eq!(path.as_os_str().len(), 103);

    // The premise: the kernel binds and connects a 103-byte path.
    let listener = bind(&at_limit);
    assert_taken(
        &round_trip(&at_limit, &request("/abs/limit.md")),
        "a request over a 103-byte socket path",
    );
    drop(listener);

    let over = dir_of_len(DIR_AT_LIMIT + 1);
    let err = err_of(
        open_socket_path(&over),
        "open_socket_path of a 104-byte path",
    );
    assert!(
        matches!(&err, OpenChannelError::PathTooLong { path, len: 104 } if *path == over.join(OPEN_SOCKET_NAME)),
        "a 104-byte socket path must be PathTooLong naming that path and length 104; got {err:?}. \
         Ok would mean the limit is 104, which leaves no byte for sun_path's NUL on macOS."
    );
    assert!(
        err.to_string().contains("103"),
        "the PathTooLong message must name the 103-byte limit; it reads {err}."
    );
}

#[test]
fn the_listener_refuses_an_over_long_path() {
    let over = dir_of_len(DIR_AT_LIMIT + 1);
    let err = err_of(
        OpenListener::bind(&over, noop_wake()),
        "binding a 104-byte path",
    );
    assert!(
        matches!(err, OpenChannelError::PathTooLong { len: 104, .. }),
        "bind must refuse a 104-byte socket path as PathTooLong; got {err:?}, which means bind \
         does not check the length first."
    );
    assert!(
        !over.exists(),
        "bind must check the length before touching the filesystem; {} was created.",
        over.display()
    );
}

#[test]
fn the_client_refuses_an_over_long_path() {
    let over = dir_of_len(DIR_AT_LIMIT + 1);
    let err = err_of(OpenClient::connect(&over), "connecting to a 104-byte path");
    assert!(
        matches!(err, OpenChannelError::PathTooLong { len: 104, .. }),
        "connect must refuse a 104-byte socket path as PathTooLong; got {err:?}. NotListening or \
         Io means it tried the socket without checking the length."
    );

    let launched = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&launched);
    let result = OpenClient::connect_or_launch(
        &over,
        move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        LAUNCH_WAIT,
    );
    let err = err_of(result, "connect_or_launch on a 104-byte path");
    assert!(
        matches!(err, OpenChannelError::PathTooLong { len: 104, .. }),
        "connect_or_launch must refuse a 104-byte socket path as PathTooLong; got {err:?}."
    );
    assert_eq!(
        launched.load(Ordering::SeqCst),
        0,
        "connect_or_launch must not launch the app for a path no app could listen on."
    );
}

#[test]
fn a_request_is_taken_queued_and_wakes() {
    let state = fresh_state();
    let (wake, wakes) = counting_wake();
    let listener = OpenListener::bind(&state, wake).expect("bind failed");
    let req = request("/abs/queued.md");
    assert_taken(&round_trip(&state, &req), "a plain request");
    assert_eq!(
        listener.drain_requests(),
        vec![req],
        "a taken request must be queued as sent, before Taken is replied: empty means it was not \
         queued, a different request means a field was lost on the wire."
    );
    assert!(
        wakes.load(Ordering::SeqCst) >= 1,
        "queueing a request must call wake before Taken is replied, so the front-end drains it; \
         the wake count is 0."
    );
    assert_eq!(
        listener.drain_requests(),
        Vec::new(),
        "a drain must empty the queue."
    );
}

#[test]
fn requests_drain_in_arrival_order() {
    let state = fresh_state();
    let listener = bind(&state);
    let first = request("/abs/first.md");
    let second = request("/abs/second.md");
    assert_taken(&round_trip(&state, &first), "the first request");
    assert_taken(&round_trip(&state, &second), "the second request");
    assert_eq!(
        listener.drain_requests(),
        vec![first, second],
        "requests must drain in the order they were taken."
    );
}

#[test]
fn a_wait_is_refused_and_not_queued() {
    let state = fresh_state();
    let listener = bind(&state);
    let req = OpenRequest {
        wait: true,
        ..request("/abs/waited.md")
    };
    let reply = round_trip(&state, &req);
    assert!(
        matches!(&reply, OpenReply::Refused { message } if message.contains("--wait")),
        "a wait must be refused with a message naming --wait, which this listener does not \
         support; got {reply:?}. Taken means the wait was handled as a plain request."
    );
    assert_eq!(
        listener.drain_requests(),
        Vec::new(),
        "a refused wait must not be queued."
    );
}

#[test]
fn a_malformed_request_is_refused() {
    let state = fresh_state();
    let listener = bind(&state);
    let socket = listener.socket_path().to_owned();

    // The control: the raw route takes a well-formed line, and ignores an unknown key.
    assert_taken(
        &raw_round_trip(
            &socket,
            "{\"path\":\"/abs/raw.md\",\"from_the_future\":1}\n",
        ),
        "a raw line with an unknown key",
    );
    assert_eq!(
        listener.drain_requests(),
        vec![OpenRequest {
            path: Some(PathBuf::from("/abs/raw.md")),
            wait: false,
            tab_id: None,
        }],
        "a raw line with only a path must queue that path with no wait and no tab ID."
    );

    let rows = [
        ("a line that is not JSON", "not json\n"),
        ("a relative path", "{\"path\":\"relative.md\"}\n"),
    ];
    for (what, line) in rows {
        assert_refused(&raw_round_trip(&socket, line), what);
        assert_eq!(
            listener.drain_requests(),
            Vec::new(),
            "{what} must not be queued; a queued request means it was taken anyway."
        );
    }
    assert_taken(
        &round_trip(&state, &request("/abs/after.md")),
        "a request after the refused ones",
    );
}

#[test]
fn an_oversized_request_is_refused() {
    let state = fresh_state();
    let listener = bind(&state);

    // The control: a long request under the cap is taken.
    let long = format!("/{}", "a".repeat(32 * 1024));
    let req = request(&long);
    assert_taken(&round_trip(&state, &req), "a 32 KiB request");
    assert_eq!(listener.drain_requests(), vec![req]);

    let line = format!("{{\"path\":\"/{}\"}}\n", "a".repeat(1024 * 1024));
    let stream = UnixStream::connect(listener.socket_path()).expect("failed to connect");
    stream
        .set_read_timeout(Some(REPLY_WAIT))
        .expect("set_read_timeout failed");
    let mut writer = stream.try_clone().expect("failed to clone the stream");
    // The listener stops reading at its cap, so this write may end in EPIPE.
    thread::spawn(move || {
        let _ = writer.write_all(line.as_bytes());
    });
    let reply = read_reply_line(stream);
    assert_refused(&reply, "a 1 MiB request");
    assert_eq!(
        listener.drain_requests(),
        Vec::new(),
        "a request over the size cap must not be queued."
    );
}

#[test]
fn a_stalled_client_does_not_hold_up_another() {
    let state = fresh_state();
    let listener = bind(&state);
    let mut stalled = UnixStream::connect(listener.socket_path()).expect("failed to connect");
    let started = Instant::now();
    assert_taken(
        &round_trip(&state, &request("/abs/second.md")),
        "a request behind a client that sends nothing",
    );
    stalled
        .set_nonblocking(true)
        .expect("set_nonblocking failed");
    let mut buf = [0u8; 256];
    match stalled.read(&mut buf) {
        Err(e) if e.kind() == ErrorKind::WouldBlock => {}
        other => panic!(
            "the stalled client must still be open and unanswered after the other request, {:?} \
             after it connected; got {other:?}. Ok(0) means it was closed, Ok(n) that it was \
             answered, either of which a listener serving connections one at a time does after \
             its read timeout.",
            started.elapsed()
        ),
    }
    drop(listener);
}

#[test]
fn a_silent_client_is_let_go() {
    let state = fresh_state();
    let listener = bind(&state);
    let stream = UnixStream::connect(listener.socket_path()).expect("failed to connect");
    stream
        .set_read_timeout(Some(REPLY_WAIT))
        .expect("set_read_timeout failed");
    let mut line = String::new();
    let read = BufReader::new(stream).read_line(&mut line);
    match read {
        Ok(0) => {}
        Ok(_) => {
            let reply: Result<OpenReply, _> = serde_json::from_str(&line);
            assert!(
                matches!(reply, Ok(OpenReply::Refused { .. })),
                "a client that sends nothing may only be refused or closed; got {line:?}."
            );
        }
        Err(e) => panic!(
            "a client that sends nothing must be closed or refused once the listener's request \
             timeout passes; after {REPLY_WAIT:?} it is still waiting ({e}), so a silent client \
             holds a thread forever."
        ),
    }
    assert_eq!(listener.drain_requests(), Vec::new());
}

#[test]
fn a_stale_socket_is_replaced() {
    let state = fresh_state();
    std::fs::create_dir_all(&state).expect("failed to create the state dir");
    let socket = state.join(OPEN_SOCKET_NAME);
    drop(UnixListener::bind(&socket).expect("failed to bind the stale listener"));
    assert!(
        socket.symlink_metadata().is_ok(),
        "premise: a dropped std UnixListener must leave its socket file behind."
    );
    let listener = OpenListener::bind(&state, noop_wake()).unwrap_or_else(|e| {
        panic!("bind over a stale socket must remove it and succeed; got {e:?}")
    });
    assert_taken(
        &round_trip(&state, &request("/abs/stale.md")),
        "a request to a listener bound over a stale socket",
    );
    drop(listener);
}

#[test]
fn a_live_listener_is_not_stolen() {
    let state = fresh_state();
    let first = bind(&state);
    let inode = std::fs::metadata(first.socket_path())
        .expect("no socket file")
        .ino();
    let err = err_of(
        OpenListener::bind(&state, noop_wake()),
        "a second bind on a live socket",
    );
    assert!(
        matches!(err, OpenChannelError::InUse(_)),
        "a second bind on a live listener's socket must be InUse; got {err:?}."
    );
    assert_eq!(
        std::fs::metadata(first.socket_path())
            .expect("the socket file is gone")
            .ino(),
        inode,
        "a refused bind must leave the live listener's socket file in place."
    );
    let req = request("/abs/first.md");
    assert_taken(
        &round_trip(&state, &req),
        "a request after a refused second bind",
    );
    assert_eq!(first.drain_requests(), vec![req]);
}

#[test]
fn dropping_the_listener_removes_its_socket() {
    let state = fresh_state();
    let listener = bind(&state);
    let socket = listener.socket_path().to_owned();
    drop(listener);
    assert!(
        socket.symlink_metadata().is_err(),
        "dropping the listener must remove its socket file {}.",
        socket.display()
    );
    let err = err_of(
        OpenClient::connect(&state),
        "connecting after the listener was dropped",
    );
    assert!(
        matches!(err, OpenChannelError::NotListening(_)),
        "connecting where no listener is must be NotListening; got {err:?}."
    );
}

#[test]
fn dropping_a_replaced_listener_keeps_its_successor() {
    let state = fresh_state();
    let first = bind(&state);
    let socket = first.socket_path().to_owned();
    std::fs::remove_file(&socket).expect("failed to remove the first socket");
    let second = bind(&state);
    drop(first);
    assert!(
        socket.symlink_metadata().is_ok(),
        "dropping a listener whose socket file was replaced must leave the successor's file; it \
         is gone, so Drop unlinks whatever is at the path."
    );
    let req = request("/abs/successor.md");
    assert_taken(&round_trip(&state, &req), "a request to the successor");
    assert_eq!(second.drain_requests(), vec![req]);
}

#[test]
fn the_socket_is_private_to_the_user() {
    let state = fresh_state();
    let listener = bind(&state);
    let mode = std::fs::metadata(listener.socket_path())
        .expect("no socket file")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "the socket must be mode 0600, so no other user can send a request; it is {:o}.",
        mode & 0o777
    );
}

#[test]
fn connect_or_launch_waits_for_a_launched_app() {
    let state = fresh_state();
    let launched = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&launched);
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (drained_tx, drained_rx) = mpsc::channel::<Vec<OpenRequest>>();
    let app_state = state.clone();
    let launch = move || {
        counted.fetch_add(1, Ordering::SeqCst);
        thread::spawn(move || {
            thread::sleep(LAUNCH_DELAY);
            let listener = bind(&app_state);
            let _ = stop_rx.recv();
            let _ = drained_tx.send(listener.drain_requests());
        });
        Ok(())
    };
    let started = Instant::now();
    let mut client = match OpenClient::connect_or_launch(&state, launch, LAUNCH_WAIT) {
        Ok(client) => client,
        Err(e) => panic!(
            "connect_or_launch must connect once the launched app binds, {LAUNCH_DELAY:?} after \
             the launch; got {e:?} after {:?}. NotListening means it did not retry until the \
             timeout.",
            started.elapsed()
        ),
    };
    assert_eq!(
        launched.load(Ordering::SeqCst),
        1,
        "launch must run exactly once."
    );
    let req = request("/abs/launched.md");
    client
        .set_read_timeout(Some(REPLY_WAIT))
        .expect("set_read_timeout failed");
    client.send(&req).expect("send failed");
    let reply = client
        .read_reply()
        .expect("read_reply failed")
        .expect("no reply");
    assert_taken(&reply, "a request to the launched app");
    stop_tx.send(()).expect("the app thread is gone");
    let drained = drained_rx
        .recv_timeout(REPLY_WAIT)
        .expect("the app thread sent nothing");
    assert_eq!(
        drained,
        vec![req],
        "the launched app must have queued the request."
    );
}

#[test]
fn connect_or_launch_gives_up_after_its_timeout() {
    let state = fresh_state();
    let launched = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&launched);
    let timeout = Duration::from_millis(300);
    let started = Instant::now();
    let result = OpenClient::connect_or_launch(
        &state,
        move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        timeout,
    );
    let elapsed = started.elapsed();
    let err = err_of(result, "connect_or_launch with an app that never listens");
    assert!(
        matches!(err, OpenChannelError::NotListening(_)),
        "an app that never listens must end in NotListening; got {err:?}."
    );
    assert!(
        elapsed >= timeout,
        "connect_or_launch must keep retrying until its {timeout:?} timeout; it gave up after \
         {elapsed:?}."
    );
    assert_eq!(
        launched.load(Ordering::SeqCst),
        1,
        "launch must run exactly once."
    );
}

#[test]
fn connect_or_launch_does_not_launch_a_running_app() {
    let state = fresh_state();
    let listener = bind(&state);
    let launched = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&launched);
    let mut client = OpenClient::connect_or_launch(
        &state,
        move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        LAUNCH_WAIT,
    )
    .unwrap_or_else(|e| panic!("connect_or_launch to a live listener failed: {e:?}"));
    assert_eq!(
        launched.load(Ordering::SeqCst),
        0,
        "connect_or_launch must not launch the app when the first connect succeeds."
    );
    let req = request("/abs/running.md");
    client
        .set_read_timeout(Some(REPLY_WAIT))
        .expect("set_read_timeout failed");
    client.send(&req).expect("send failed");
    let reply = client
        .read_reply()
        .expect("read_reply failed")
        .expect("no reply");
    assert_taken(&reply, "a request to the running app");
    assert_eq!(listener.drain_requests(), vec![req]);
}

#[test]
fn a_launch_failure_is_returned_at_once() {
    let state = fresh_state();
    let marker = "launch-failed-marker";
    let started = Instant::now();
    let result = OpenClient::connect_or_launch(
        &state,
        move || Err(std::io::Error::other(marker)),
        LAUNCH_WAIT,
    );
    let elapsed = started.elapsed();
    let err = err_of(result, "connect_or_launch with a failing launch");
    assert!(
        matches!(&err, OpenChannelError::Launch(e) if e.to_string().contains(marker)),
        "a failing launch must be returned as Launch carrying its error; got {err:?}."
    );
    assert!(
        elapsed < LAUNCH_WAIT / 2,
        "a failing launch must be returned at once, not after polling toward the {LAUNCH_WAIT:?} \
         timeout; it took {elapsed:?}."
    );
}
