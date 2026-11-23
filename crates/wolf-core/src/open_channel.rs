//! The channel the `wolf` command reaches a running Wolf through, as
//! `specs/cli-and-file-open.md#2-the-channel` specifies: a Unix-domain socket in
//! [`crate::state_dir`] carrying one JSON request line and its JSON reply lines.
//!
//! [`OpenClient`] is the command's side. [`OpenListener`] is the app's: one per process, on
//! std threads rather than a core's runtime, since every core belongs to a tab that can close.
//! It queues each request it takes and calls its [`WakeFn`], and the front-end drains the queue
//! on the wake, as it drains [`crate::WolfCore::drain_commands`].

use std::collections::VecDeque;
use std::fs::{self, Permissions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::file_open::OpenTarget;
use crate::redraw::WakeFn;
use crate::tab_id::TabId;

/// The socket's file name inside the state directory.
pub const OPEN_SOCKET_NAME: &str = "wolf.sock";

/// The longest socket path in bytes, on both platforms: macOS's `sun_path` holds 104 bytes,
/// the terminating NUL included.
pub const OPEN_SOCKET_PATH_MAX: usize = 103;

/// How long the listener waits for a connection's request line before refusing it.
pub const OPEN_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// The longest request line the listener reads, newline included.
pub const OPEN_REQUEST_MAX: u64 = 64 * 1024;

/// How often [`OpenClient::connect_or_launch`] retries while a launched app starts.
const LAUNCH_RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// What goes wrong on either side of the channel.
#[derive(Debug, thiserror::Error)]
pub enum OpenChannelError {
    /// The socket path is over [`OPEN_SOCKET_PATH_MAX`] bytes.
    #[error(
        "the socket path {} is {len} bytes, over the {} byte limit for a socket path",
        .path.display(),
        OPEN_SOCKET_PATH_MAX
    )]
    PathTooLong {
        /// The socket path.
        path: PathBuf,
        /// Its length in bytes.
        len: usize,
    },
    /// Nothing listens on the socket: no file there, or a stale one.
    #[error("Wolf is not running: nothing listens on {}", .0.display())]
    NotListening(PathBuf),
    /// A live listener already has the socket.
    #[error("another Wolf already listens on {}", .0.display())]
    InUse(PathBuf),
    /// The launch hook failed.
    #[error("could not launch Wolf: {0}")]
    Launch(io::Error),
    /// A reply line that is not an [`OpenReply`].
    #[error("Wolf sent a malformed reply: {0}")]
    Protocol(String),
    /// Any other I/O failure, a read timeout included.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The socket path inside `state_dir`, measured as given, so a relative `state_dir` is
/// allowed. A path over [`OPEN_SOCKET_PATH_MAX`] bytes is [`OpenChannelError::PathTooLong`].
pub fn open_socket_path(state_dir: &Path) -> Result<PathBuf, OpenChannelError> {
    let path = state_dir.join(OPEN_SOCKET_NAME);
    let len = path.as_os_str().len();
    if len > OPEN_SOCKET_PATH_MAX {
        return Err(OpenChannelError::PathTooLong { path, len });
    }
    Ok(path)
}

/// The one request a connection carries. Each field defaults when absent, and unknown keys
/// are ignored, so either side can gain a field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenRequest {
    /// The absolute path to open, unresolved; `None` to launch or activate Wolf.
    pub path: Option<PathBuf>,
    /// Whether the command waits for the opened buffer to close.
    pub wait: bool,
    /// The command's `WOLF_TAB_ID`, when set and well-formed.
    pub tab_id: Option<TabId>,
}

impl OpenRequest {
    /// What the request's path names, decided on the app's side.
    #[must_use]
    pub fn target(&self) -> OpenTarget {
        OpenTarget::of(self.path.as_deref())
    }
}

/// A reply line, tagged by `"reply"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum OpenReply {
    /// The app has the request.
    Taken,
    /// A wait ended; `status` is the command's exit status.
    WaitEnded {
        /// `0` for a closed buffer, anything else for another end.
        status: i32,
    },
    /// The request is refused, and the command exits `2` with `message`.
    Refused {
        /// Why, for the command's stderr.
        message: String,
    },
}

/// The command's end of one connection.
pub struct OpenClient {
    reader: BufReader<UnixStream>,
}

impl OpenClient {
    /// Connects to the listener in `state_dir`. No socket file, or a stale one, is
    /// [`OpenChannelError::NotListening`].
    pub fn connect(state_dir: &Path) -> Result<OpenClient, OpenChannelError> {
        let path = open_socket_path(state_dir)?;
        match UnixStream::connect(&path) {
            Ok(stream) => Ok(OpenClient {
                reader: BufReader::new(stream),
            }),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                Err(OpenChannelError::NotListening(path))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Connects, and when nothing listens, calls `launch` once and retries until a listener
    /// answers or `timeout` passes. A failing `launch` is returned at once as
    /// [`OpenChannelError::Launch`].
    pub fn connect_or_launch(
        state_dir: &Path,
        launch: impl FnOnce() -> io::Result<()>,
        timeout: Duration,
    ) -> Result<OpenClient, OpenChannelError> {
        match OpenClient::connect(state_dir) {
            Err(OpenChannelError::NotListening(_)) => {}
            other => return other,
        }
        launch().map_err(OpenChannelError::Launch)?;
        let deadline = Instant::now() + timeout;
        loop {
            match OpenClient::connect(state_dir) {
                Err(OpenChannelError::NotListening(path)) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(OpenChannelError::NotListening(path));
                    }
                    thread::sleep(LAUNCH_RETRY_INTERVAL.min(deadline - now));
                }
                other => return other,
            }
        }
    }

    /// Sends `request` as one line.
    pub fn send(&mut self, request: &OpenRequest) -> Result<(), OpenChannelError> {
        let mut line = serde_json::to_vec(request).map_err(io::Error::other)?;
        line.push(b'\n');
        self.reader.get_mut().write_all(&line)?;
        Ok(())
    }

    /// The next reply, or `None` once the app closes the connection.
    pub fn read_reply(&mut self) -> Result<Option<OpenReply>, OpenChannelError> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        serde_json::from_str(&line)
            .map(Some)
            .map_err(|e| OpenChannelError::Protocol(format!("{e}: {line:?}")))
    }

    /// Bounds each [`read_reply`](Self::read_reply); `None` waits indefinitely.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> Result<(), OpenChannelError> {
        self.reader.get_ref().set_read_timeout(timeout)?;
        Ok(())
    }
}

/// What the listener's threads share with it.
struct Shared {
    queue: Mutex<VecDeque<OpenRequest>>,
    wake: WakeFn,
}

/// The app's end: a listening socket whose taken requests a front-end drains on a wake.
///
/// Each connection is served on a thread of its own, so a client that stalls holds up no
/// other. Dropping the listener stops it and removes its socket file, unless another listener
/// has replaced that file since.
pub struct OpenListener {
    shared: Arc<Shared>,
    socket: PathBuf,
    /// The socket file's `(dev, ino)` and absolute path, for the unlink on drop.
    file: ((u64, u64), PathBuf),
    /// Dropping it wakes the accept thread to stop.
    stop: Option<UnixStream>,
    thread: Option<JoinHandle<()>>,
}

impl OpenListener {
    /// Listens on the socket in `state_dir`, creating the directory, and calls `wake` after
    /// queueing each request.
    ///
    /// A socket whose connect is refused is stale, and is replaced. A live listener's socket is
    /// [`OpenChannelError::InUse`]. Any other connect failure, or a file at the path that is not
    /// a socket, is [`OpenChannelError::Io`]; neither removes the file.
    pub fn bind(state_dir: &Path, wake: WakeFn) -> Result<OpenListener, OpenChannelError> {
        let socket = open_socket_path(state_dir)?;
        fs::create_dir_all(state_dir)?;
        if let Ok(meta) = fs::symlink_metadata(&socket) {
            match UnixStream::connect(&socket) {
                Ok(_) => return Err(OpenChannelError::InUse(socket)),
                Err(e)
                    if e.kind() == io::ErrorKind::ConnectionRefused
                        && meta.file_type().is_socket() =>
                {
                    fs::remove_file(&socket)?;
                }
                Err(e) => return Err(e.into()),
            }
        }
        let listener = UnixListener::bind(&socket)?;
        fs::set_permissions(&socket, Permissions::from_mode(0o600))?;
        let meta = fs::metadata(&socket)?;
        let file = ((meta.dev(), meta.ino()), std::path::absolute(&socket)?);
        listener.set_nonblocking(true)?;
        let (stop, stopped) = UnixStream::pair()?;
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            wake,
        });
        let accept_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("wolf-open-listen".to_owned())
            .spawn(move || accept_loop(&listener, &stopped, &accept_shared))?;
        Ok(OpenListener {
            shared,
            socket,
            file,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// The requests taken since the last drain, in arrival order.
    #[must_use]
    pub fn drain_requests(&self) -> Vec<OpenRequest> {
        self.shared.queue.lock().drain(..).collect()
    }

    /// The socket path, as [`open_socket_path`] made it.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket
    }
}

impl Drop for OpenListener {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let ((dev, ino), path) = &self.file;
        let ours = fs::symlink_metadata(path).is_ok_and(|m| m.dev() == *dev && m.ino() == *ino);
        if ours {
            let _ = fs::remove_file(path);
        }
    }
}

/// Accepts connections until `stopped` reads end of file, serving each on its own thread.
fn accept_loop(listener: &UnixListener, stopped: &UnixStream, shared: &Arc<Shared>) {
    let mut fds = [
        libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: stopped.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        // SAFETY: `fds` is valid for its two entries, whose descriptors outlive the call.
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if rc < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            tracing::warn!("open channel: poll failed, the listener stops");
            return;
        }
        if fds[1].revents != 0 {
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let shared = Arc::clone(shared);
                let spawned = thread::Builder::new()
                    .name("wolf-open-conn".to_owned())
                    .spawn(move || serve(stream, &shared));
                if let Err(e) = spawned {
                    tracing::warn!("open channel: cannot serve a connection: {e}");
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => tracing::warn!("open channel: accept failed: {e}"),
        }
    }
}

/// Reads one request from `stream`, queues and wakes for one it takes, and replies.
fn serve(mut stream: UnixStream, shared: &Shared) {
    // An accepted socket inherits the listener's O_NONBLOCK on macOS.
    let ready = stream
        .set_nonblocking(false)
        .and_then(|()| stream.set_read_timeout(Some(OPEN_REQUEST_TIMEOUT)));
    if ready.is_err() {
        return;
    }
    let reply = match read_request(&stream) {
        Ok(request) => {
            shared.queue.lock().push_back(request);
            (shared.wake)();
            OpenReply::Taken
        }
        Err(message) => OpenReply::Refused { message },
    };
    if let Ok(mut line) = serde_json::to_vec(&reply) {
        line.push(b'\n');
        let _ = stream.write_all(&line);
    }
}

/// The request `stream` sends, or why it is refused.
fn read_request(stream: &UnixStream) -> Result<OpenRequest, String> {
    let mut line = Vec::new();
    BufReader::new(stream.take(OPEN_REQUEST_MAX))
        .read_until(b'\n', &mut line)
        .map_err(|e| format!("no request arrived within {OPEN_REQUEST_TIMEOUT:?}: {e}"))?;
    if line.last() != Some(&b'\n') {
        return Err(if line.len() as u64 >= OPEN_REQUEST_MAX {
            format!("the request is over {OPEN_REQUEST_MAX} bytes")
        } else {
            "the request ended without a newline".to_owned()
        });
    }
    let request: OpenRequest =
        serde_json::from_slice(&line).map_err(|e| format!("malformed request: {e}"))?;
    if request
        .path
        .as_deref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err("the request's path is not absolute".to_owned());
    }
    if request.wait {
        return Err("this Wolf does not support --wait".to_owned());
    }
    Ok(request)
}
