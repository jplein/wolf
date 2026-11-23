//! The `wolf` command, `wolf [--wait] [PATH]`, of `specs/cli-and-file-open.md#1-the-command`.
//! It sends one [`OpenRequest`] to a running Wolf over the channel of
//! `specs/cli-and-file-open.md#2-the-channel` and exits with the status the replies give.
//! With nothing listening it starts Wolf through the platform's [`launcher`], when there is
//! one, and retries. Stdout stays empty; every message goes to stderr.

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use wolf_core::{
    state_dir, OpenChannelError, OpenClient, OpenReply, OpenRequest, OpenTarget, TabId,
    WOLF_TAB_ID_ENV,
};

/// Wolf has the request, or a wait ended with the buffer closed.
const EXIT_OK: u8 = 0;

/// A wait ended any other way, the connection closing included.
const EXIT_WAIT_FAILED: u8 = 1;

/// The invocation was refused, or Wolf could not be reached or refused the request.
const EXIT_REFUSED: u8 = 2;

/// How long the command waits for Wolf to take the request.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a launched Wolf has to start listening.
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);

const USAGE: &str = "usage: wolf [--wait] [PATH]";

/// The variable that moves the state directory, and with it the socket.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const STATE_DIR_ENV: &str = "WOLF_STATE_DIR";

/// The app the Linux launcher starts, a sibling of this executable in the package's `bin/`.
#[cfg(target_os = "linux")]
const GNOME_BIN: &str = "wolf-gnome";

/// The bundle identifier Launch Services starts Wolf by.
// crates/wolf-macos/Info.plist's CFBundleIdentifier is the same string.
#[cfg(target_os = "macos")]
const BUNDLE_ID: &str = "org.wolf.WolfMacOS";

/// The platform's way to start Wolf when nothing listens, `None` where there is none.
///
/// On macOS it launches `Wolf.app` through Launch Services. A launched app inherits no
/// environment, so it listens in the default state directory only: under a non-empty
/// `WOLF_STATE_DIR` there is no launcher.
///
/// On Linux it starts the `wolf-gnome` beside this executable, and with none there is no
/// launcher. The app inherits the environment, so it listens where the command connects,
/// whatever `WOLF_STATE_DIR` says. Other platforms have none.
#[cfg(target_os = "macos")]
fn launcher() -> Option<fn() -> io::Result<()>> {
    let overridden = std::env::var_os(STATE_DIR_ENV).is_some_and(|dir| !dir.is_empty());
    if overridden {
        return None;
    }
    Some(launch_app)
}

/// See the macOS [`launcher`].
#[cfg(target_os = "linux")]
fn launcher() -> Option<fn() -> io::Result<()>> {
    gnome_beside().map(|_| launch_gnome as fn() -> io::Result<()>)
}

/// See the macOS [`launcher`].
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn launcher() -> Option<fn() -> io::Result<()>> {
    None
}

/// The `wolf-gnome` beside this executable, when that file exists.
#[cfg(target_os = "linux")]
fn gnome_beside() -> Option<std::path::PathBuf> {
    let gnome = std::env::current_exe().ok()?.with_file_name(GNOME_BIN);
    gnome.is_file().then_some(gnome)
}

/// Starts the `wolf-gnome` beside this executable and leaves it running; an `Err` when it
/// cannot be started.
///
/// It starts in `/`, so its restore adds no window for the command's directory and a cold
/// launch restores the session alone; the request carries its own absolute path. Hence a
/// non-empty `WOLF_STATE_DIR` is made absolute against the command's working directory. It
/// belongs to no tab, so it gets no `WOLF_TAB_ID`. A stream that is a terminal is nulled,
/// since writing to a closed terminal would fail every later `eprintln!`, and any other is
/// inherited for the journal or a log. Its own process group keeps a hangup from reaching it.
#[cfg(target_os = "linux")]
fn launch_gnome() -> io::Result<()> {
    use std::io::IsTerminal;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let gnome = gnome_beside()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no wolf-gnome beside wolf"))?;
    let mut command = Command::new(&gnome);
    command
        .current_dir("/")
        .env_remove(WOLF_TAB_ID_ENV)
        .stdin(Stdio::null())
        .process_group(0);
    if let Some(dir) = std::env::var_os(STATE_DIR_ENV).filter(|dir| !dir.is_empty()) {
        command.env(STATE_DIR_ENV, std::path::absolute(dir)?);
    }
    if io::stdout().is_terminal() {
        command.stdout(Stdio::null());
    }
    if io::stderr().is_terminal() {
        command.stderr(Stdio::null());
    }
    command.spawn().map(drop)
}

/// Runs `open -b` on Wolf's bundle identifier; an `Err` when `open` fails or exits non-zero.
#[cfg(target_os = "macos")]
fn launch_app() -> io::Result<()> {
    let status = std::process::Command::new("/usr/bin/open")
        .args(["-b", BUNDLE_ID])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("open -b {BUNDLE_ID} {status}")))
    }
}

/// The parsed command line.
struct Invocation {
    wait: bool,
    path: Option<OsString>,
}

/// Scans `args` left to right; `--` ends the options.
fn parse_args(args: impl Iterator<Item = OsString>) -> Result<Invocation, String> {
    let mut wait = false;
    let mut path = None;
    let mut options = true;
    for arg in args {
        if options && arg == "--" {
            options = false;
        } else if options && arg == "--wait" {
            if wait {
                return Err("--wait is given twice".to_owned());
            }
            wait = true;
        } else if options && arg.as_encoded_bytes().starts_with(b"-") {
            return Err(format!("unknown option {}", arg.to_string_lossy()));
        } else if arg.is_empty() {
            return Err("the path is empty".to_owned());
        } else if path.is_some() {
            return Err("only one path can be opened".to_owned());
        } else {
            path = Some(arg);
        }
    }
    if wait && path.is_none() {
        return Err("--wait needs a file".to_owned());
    }
    Ok(Invocation { wait, path })
}

/// The request `invocation` makes, its path made absolute against the working directory.
fn build_request(invocation: Invocation) -> Result<OpenRequest, String> {
    let path = match invocation.path {
        Some(path) => {
            let path = std::path::absolute(Path::new(&path))
                .map_err(|e| format!("cannot resolve {}: {e}", path.to_string_lossy()))?;
            if path.to_str().is_none() {
                return Err(format!("{} is not a UTF-8 path", path.display()));
            }
            Some(path)
        }
        None => None,
    };
    if invocation.wait {
        if let OpenTarget::Directory(dir) = OpenTarget::of(path.as_deref()) {
            return Err(format!(
                "--wait needs a file, and {} is a directory",
                dir.display()
            ));
        }
    }
    let tab_id = std::env::var(WOLF_TAB_ID_ENV)
        .ok()
        .and_then(|value| TabId::parse(&value));
    Ok(OpenRequest {
        path,
        wait: invocation.wait,
        tab_id,
    })
}

/// Connects to Wolf, launching it through [`launcher`] when there is one.
fn connect(state: &Path) -> Result<OpenClient, OpenChannelError> {
    match launcher() {
        Some(launch) => OpenClient::connect_or_launch(state, launch, LAUNCH_TIMEOUT),
        None => OpenClient::connect(state),
    }
}

/// Sends `request` and maps the replies to the exit status.
fn converse(client: &mut OpenClient, request: &OpenRequest) -> u8 {
    let taken = client
        .set_read_timeout(Some(REPLY_TIMEOUT))
        .and_then(|()| client.send(request))
        .and_then(|()| client.read_reply());
    match taken {
        Ok(Some(OpenReply::Taken)) => {}
        Ok(Some(OpenReply::Refused { message })) => return fail(EXIT_REFUSED, &message),
        Ok(Some(reply)) => {
            return fail(
                EXIT_REFUSED,
                &format!("Wolf replied {reply:?} before taking it"),
            )
        }
        Ok(None) => {
            return fail(
                EXIT_REFUSED,
                "Wolf closed the connection without taking the request",
            )
        }
        Err(e) => return fail(EXIT_REFUSED, &format!("no answer from Wolf: {e}")),
    }
    if !request.wait {
        return EXIT_OK;
    }
    // macOS fails this with EINVAL once Wolf has closed, and the buffered replies still read.
    let _ = client.set_read_timeout(None);
    match client.read_reply() {
        Ok(Some(OpenReply::WaitEnded { status: 0 })) => EXIT_OK,
        Ok(Some(OpenReply::WaitEnded { .. })) => EXIT_WAIT_FAILED,
        Ok(Some(OpenReply::Refused { message })) => fail(EXIT_REFUSED, &message),
        Ok(Some(OpenReply::Taken)) => fail(EXIT_WAIT_FAILED, "Wolf took the request twice"),
        Ok(None) => fail(
            EXIT_WAIT_FAILED,
            "Wolf closed the connection during the wait",
        ),
        Err(e) => fail(EXIT_WAIT_FAILED, &format!("the wait failed: {e}")),
    }
}

/// Prints `message` to stderr and returns `status`.
fn fail(status: u8, message: &str) -> u8 {
    eprintln!("wolf: {message}");
    status
}

fn run() -> u8 {
    let request = match parse_args(std::env::args_os().skip(1)).and_then(build_request) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("wolf: {message}\n{USAGE}");
            return EXIT_REFUSED;
        }
    };
    let Some(state) = state_dir() else {
        return fail(EXIT_REFUSED, "cannot find Wolf's state directory");
    };
    let mut client = match connect(&state) {
        Ok(client) => client,
        Err(e) => return fail(EXIT_REFUSED, &e.to_string()),
    };
    converse(&mut client, &request)
}

fn main() -> ExitCode {
    ExitCode::from(run())
}
