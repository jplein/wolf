//! The `wolf` command, `wolf [--wait] [PATH]`, of `specs/cli-and-file-open.md#1-the-command`.
//! It sends one [`OpenRequest`] to a running Wolf over the channel of
//! `specs/cli-and-file-open.md#2-the-channel` and exits with the status the replies give.
//! Stdout stays empty; every message goes to stderr.

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

/// The platform's way to start Wolf when nothing listens, `None` where there is none.
fn launcher() -> Option<fn() -> io::Result<()>> {
    None
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
