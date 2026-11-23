//! Headless tests of the `wolf-ffi` open-request surface macOS drives, over real Unix-domain
//! sockets and `wolf-core`'s real `OpenClient`: `wolf_ffi_open_listen` and `wolf_ffi_open_stop`,
//! the drained `FfiOpenItem`s with each request resolved as `specs/cli-and-file-open.md` §1 and
//! §3 say, the wake `wolf_ffi_set_open_wake` registers, and `wolf_ffi_open_item`, Finder's route.
//!
//! ## Relative socket paths
//!
//! No absolute socket path under Bazel's test `TMPDIR` fits 103 bytes inside the macOS gate, so
//! the binary enters a kept, canonical temp dir once ([`base`]), as
//! `crates/wolf-core/tests/open_channel.rs` does, and every state dir is named relative to it.
//!
//! ## One listener per process
//!
//! The listener and the wake are process-wide, so every test takes [`SERIAL`] for its whole
//! body. The discriminants are local constants mirroring the bridge's private `open_kind`.
//!
//! ## Premises about the components
//!
//! - The listener queues a request before it replies `Taken`, so once a client has read
//!   `Taken`, the request is in the queue a drain takes.
//! - `wolf_core::TabId` parses either case and displays lowercase, so a request sent with an
//!   uppercase ID drains lowercase only if the bridge formats the parsed ID.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use wolf_core::{open_socket_path, OpenChannelError, OpenClient, OpenReply, OpenRequest, TabId};
use wolf_ffi::{
    wolf_ffi_open_drain, wolf_ffi_open_item, wolf_ffi_open_listen, wolf_ffi_open_stop,
    wolf_ffi_set_open_wake,
};

// `open_kind`
const ACTIVATE: u8 = 0;
const DIRECTORY: u8 = 1;
const FILE: u8 = 2;

/// How long a test waits for a reply, a drain or a wake before failing rather than hanging.
const WAIT: Duration = Duration::from_secs(10);

/// How often a wait polls.
const POLL: Duration = Duration::from_millis(20);

/// Held for each test's whole body: the listener and the wake are process-wide.
static SERIAL: Mutex<()> = Mutex::new(());

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The calls the registered open wake has had.
static WAKES: AtomicUsize = AtomicUsize::new(0);

extern "C" fn count_wake(_token: u64) {
    WAKES.fetch_add(1, Ordering::SeqCst);
}

/// Takes [`SERIAL`], surviving a test that panicked while holding it.
fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The canonical temp dir the binary runs in, made and entered once.
fn base() -> &'static Path {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| {
        let kept = tempfile::Builder::new()
            .prefix("wolf-ffi-open")
            .tempdir()
            .expect("failed to create the base temp dir")
            .keep();
        let dir = std::fs::canonicalize(&kept).expect("failed to canonicalize the base dir");
        std::env::set_current_dir(&dir).expect("failed to enter the base dir");
        dir
    })
}

/// A fresh relative name under [`base`] that does not exist yet.
fn fresh(prefix: &str) -> PathBuf {
    base();
    PathBuf::from(format!(
        "{prefix}{}",
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Binds the process-wide listener in a fresh state dir and returns that dir.
fn listen() -> PathBuf {
    let state = fresh("s");
    let answer = wolf_ffi_open_listen(state.display().to_string());
    assert_eq!(
        answer,
        "",
        "wolf_ffi_open_listen({}) must answer \"\" for a bindable state dir; it answered an error",
        state.display()
    );
    state
}

/// A fresh project under [`base`]: `<project>/sub/` and the file `<project>/notes.md`, absolute.
fn project() -> PathBuf {
    let dir = base().join(fresh("p"));
    std::fs::create_dir_all(dir.join("sub")).expect("failed to create the project");
    std::fs::write(dir.join("notes.md"), "# notes\n").expect("failed to write notes.md");
    dir
}

/// Sends `request` through a real `OpenClient` and waits for `Taken`.
fn send(state: &Path, request: &OpenRequest) {
    let mut client = OpenClient::connect(state)
        .unwrap_or_else(|e| panic!("OpenClient::connect({}) failed: {e:?}", state.display()));
    client
        .set_read_timeout(Some(WAIT))
        .expect("set_read_timeout failed");
    client.send(request).expect("send failed");
    let reply = client.read_reply().expect("read_reply failed");
    assert_eq!(
        reply,
        Some(OpenReply::Taken),
        "the listener must take {request:?}; a refusal means the request is malformed"
    );
}

/// Sends `line` raw, as a client with another serializer might, and waits for `Taken`.
fn send_raw(state: &Path, line: &str) {
    let socket = open_socket_path(state).expect("the state dir's socket path must fit");
    let mut stream = UnixStream::connect(&socket)
        .unwrap_or_else(|e| panic!("connect({}) failed: {e}", socket.display()));
    stream
        .set_read_timeout(Some(WAIT))
        .expect("set_read_timeout failed");
    stream
        .write_all(format!("{line}\n").as_bytes())
        .expect("write failed");
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .expect("read_line failed");
    assert!(
        reply.contains("\"taken\""),
        "the listener must take the raw request {line}; it replied {reply:?}"
    );
}

/// One drained item, read through its accessors.
#[derive(Debug, PartialEq, Eq)]
struct Drained {
    kind: u8,
    path: String,
    tab_id: String,
}

/// Drains until `count` items have arrived or [`WAIT`] passes, and returns what arrived.
fn drain(count: usize) -> Vec<Drained> {
    let deadline = Instant::now() + WAIT;
    let mut items = Vec::new();
    loop {
        for item in wolf_ffi_open_drain() {
            items.push(Drained {
                kind: item.kind(),
                path: item.path(),
                tab_id: item.tab_id(),
            });
        }
        if items.len() >= count || Instant::now() >= deadline {
            return items;
        }
        std::thread::sleep(POLL);
    }
}

fn request(path: Option<PathBuf>, tab_id: Option<TabId>) -> OpenRequest {
    OpenRequest {
        path,
        wait: false,
        tab_id,
    }
}

/// R1: a file request whose path has a `..` segment drains as `FILE`, resolved.
#[test]
fn a_file_request_drains_resolved() {
    let _serial = serial();
    let state = listen();
    let dir = project();
    let through_dotdot = dir.join("sub").join("..").join("notes.md");
    let missing = dir.join("sub").join("..").join("sub").join("later.md");

    send(&state, &request(Some(through_dotdot.clone()), None));
    send(&state, &request(Some(missing.clone()), None));
    let items = drain(2);
    wolf_ffi_open_stop();

    assert_eq!(
        items,
        [
            Drained {
                kind: FILE,
                path: dir.join("notes.md").display().to_string(),
                tab_id: String::new(),
            },
            Drained {
                kind: FILE,
                path: dir.join("sub").join("later.md").display().to_string(),
                tab_id: String::new(),
            },
        ],
        "{} and the missing {} must drain in order as FILE items on their resolved paths, with \
         no tab ID. A path with `..` in it means the drain answers the request's raw path, not \
         OpenRequest::target()'s; DIRECTORY means the kind is mis-mapped; one item or none means \
         a request was lost.",
        through_dotdot.display(),
        missing.display()
    );
}

/// R2: a directory request drains as `DIRECTORY` with its canonical path, and a request with no
/// path as `ACTIVATE`.
#[test]
fn a_directory_and_an_activate_request_drain_by_kind() {
    let _serial = serial();
    let state = listen();
    let dir = project();
    let through_dotdot = dir.join("sub").join("..");

    send(&state, &request(Some(through_dotdot.clone()), None));
    send(&state, &request(None, None));
    let items = drain(2);
    wolf_ffi_open_stop();

    assert_eq!(
        items,
        [
            Drained {
                kind: DIRECTORY,
                path: dir.display().to_string(),
                tab_id: String::new(),
            },
            Drained {
                kind: ACTIVATE,
                path: String::new(),
                tab_id: String::new(),
            },
        ],
        "{} must drain as DIRECTORY on its canonical path {}, then the request with no path as \
         ACTIVATE with path \"\". FILE for the first means the kind treats a directory as a \
         file, so Open Folder never runs; a raw path means the drain skips the resolution.",
        through_dotdot.display(),
        dir.display()
    );
}

/// R3: a request's tab ID drains with it, lowercase, and a request with none drains `""`.
#[test]
fn a_tab_id_drains_lowercase() {
    let _serial = serial();
    let state = listen();
    let dir = project();
    let file = dir.join("notes.md");
    let sent = TabId::random();
    let upper = "0A1B2C3D-4E5F-4A6B-8C7D-8E9FA0B1C2D3";

    send(&state, &request(Some(file.clone()), Some(sent)));
    send_raw(
        &state,
        &format!("{{\"path\":\"{}\",\"tab_id\":\"{upper}\"}}", file.display()),
    );
    send(&state, &request(Some(file.clone()), None));
    let items = drain(3);
    wolf_ffi_open_stop();

    let ids: Vec<&str> = items.iter().map(|item| item.tab_id.as_str()).collect();
    assert_eq!(
        ids,
        [sent.to_string().as_str(), upper.to_lowercase().as_str(), ""],
        "the three requests must drain with the tab IDs they carried, in order: {sent}, the \
         uppercase {upper} as lowercase, and \"\" for none. \"\" for the first two means \
         FfiOpenItem::tab_id drops the ID, so WOLF_TAB_ID never reaches the tab; the uppercase \
         form means the bridge passes the raw text rather than the parsed ID."
    );
}

/// R4: the callback `wolf_ffi_set_open_wake` registers runs once per taken request.
#[test]
fn the_open_wake_runs_once_per_taken_request() {
    let _serial = serial();
    wolf_ffi_set_open_wake(Some(count_wake));
    let state = listen();
    let file = project().join("notes.md");
    let before = WAKES.load(Ordering::SeqCst);

    for _ in 0..3 {
        send(&state, &request(Some(file.clone()), None));
    }
    let items = drain(3);
    let deadline = Instant::now() + WAIT;
    while WAKES.load(Ordering::SeqCst) - before < 3 && Instant::now() < deadline {
        std::thread::sleep(POLL);
    }
    let woken = WAKES.load(Ordering::SeqCst) - before;
    wolf_ffi_open_stop();
    wolf_ffi_set_open_wake(None);

    assert_eq!(
        items.len(),
        3,
        "premise: three requests sent must drain as three items; drained {items:?}"
    );
    assert_eq!(
        woken, 3,
        "the registered open wake must run once per taken request, 3 here; 0 means the \
         listener's WakeFn never reaches the registered callback, so the app drains only by \
         chance; more than 3 means a request wakes twice."
    );
}

/// R5: `wolf_ffi_open_stop` removes the socket, so a connect finds nothing listening and a drain
/// is empty.
#[test]
fn stopping_removes_the_socket() {
    let _serial = serial();
    let state = listen();
    let socket = open_socket_path(&state).expect("the state dir's socket path must fit");
    assert!(
        socket.exists(),
        "premise: a bound listener must have its socket file at {}",
        socket.display()
    );

    wolf_ffi_open_stop();
    assert!(
        !socket.exists(),
        "after wolf_ffi_open_stop the socket file {} must be gone; it remains, so the listener \
         was not dropped",
        socket.display()
    );
    match OpenClient::connect(&state) {
        Err(OpenChannelError::NotListening(_)) => {}
        Ok(_) => {
            panic!("after wolf_ffi_open_stop a connect must find nothing listening; it connected")
        }
        Err(e) => panic!("after wolf_ffi_open_stop a connect must fail NotListening, not {e:?}"),
    }
    assert!(
        wolf_ffi_open_drain().is_empty(),
        "with no listener, wolf_ffi_open_drain must answer no items"
    );
}

/// R6: a state dir whose socket path is over 103 bytes makes `wolf_ffi_open_listen` answer an
/// error message.
#[test]
fn an_over_long_socket_path_answers_an_error() {
    let _serial = serial();
    base();
    let mut name = format!("long{}-", COUNTER.fetch_add(1, Ordering::Relaxed));
    while name.len() < 100 {
        name.push('x');
    }
    let answer = wolf_ffi_open_listen(name.clone());
    wolf_ffi_open_stop();

    assert!(
        open_socket_path(Path::new(&name)).is_err(),
        "premise: {name}'s socket path must be over the limit"
    );
    assert_ne!(
        answer, "",
        "wolf_ffi_open_listen({name}) must answer the bind's error, since its socket path is over \
         103 bytes; \"\" means the error was swallowed and the app would run without listening"
    );
}

/// R7: `wolf_ffi_open_item` resolves a path as a request with no tab ID does, and `""` is
/// `ACTIVATE`.
#[test]
fn an_open_item_resolves_as_a_request_without_a_tab_id() {
    let _serial = serial();
    let dir = project();
    let cases = [
        (
            dir.join("sub").join("..").join("notes.md"),
            FILE,
            dir.join("notes.md").display().to_string(),
        ),
        (
            dir.join("sub").join(".."),
            DIRECTORY,
            dir.display().to_string(),
        ),
        (PathBuf::new(), ACTIVATE, String::new()),
    ];
    for (given, kind, path) in cases {
        let item = wolf_ffi_open_item(given.display().to_string());
        assert_eq!(
            Drained {
                kind: item.kind(),
                path: item.path(),
                tab_id: item.tab_id(),
            },
            Drained {
                kind,
                path: path.clone(),
                tab_id: String::new(),
            },
            "wolf_ffi_open_item({:?}) must be kind {kind} on {path:?} with no tab ID, as a \
             request with no WOLF_TAB_ID resolves. The given path back means Finder's route \
             skips the resolution; a wrong kind means it decides directory and file apart from \
             OpenRequest::target().",
            given.display()
        );
    }
}
