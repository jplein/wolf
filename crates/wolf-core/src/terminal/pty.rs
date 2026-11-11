//! The operating-system half of a terminal session: the pty pair, the shell process in its
//! own session and its terminal type, the window size, signals and reaping, and the shell's
//! working directory.
//!
//! There is no pty crate; this module holds every `libc` call the sessions make. See
//! `specs/terminal-panes.md#terminal-sessions`.

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::ptr;

use super::TerminalCommand;

/// The terminal type of Wolf's own terminfo entry, compiled from the pinned libghostty-vt.
const GHOSTTY_TERM: &str = "xterm-ghostty";

/// The terminal type a session gets without that entry, which every host database knows.
const FALLBACK_TERM: &str = "xterm-256color";

/// The entry inside a terminfo directory: ncurses' letter layout, then the hex layout that
/// macOS's ncurses writes and reads.
const GHOSTTY_ENTRY_PATHS: [&str; 2] = ["x/xterm-ghostty", "78/xterm-ghostty"];

/// A spawned shell: its process and the pty master it talks through.
pub(crate) struct Spawned {
    pub(crate) child: Child,
    /// Non-blocking and close-on-exec.
    pub(crate) master: OwnedFd,
}

/// Starts `command` in `cwd` on a fresh pty of `cols` by `rows` cells, each `cell_px` pixels,
/// as the leader of a new session whose controlling terminal is the pty. The shell is
/// `xterm-ghostty` with `TERMINFO` set to `terminfo` when that directory holds the entry, else
/// `xterm-256color` with the inherited `TERMINFO`.
pub(crate) fn spawn(
    command: &TerminalCommand,
    cwd: &Path,
    terminfo: Option<&Path>,
    (cols, rows): (u16, u16),
    cell_px: (u32, u32),
) -> io::Result<Spawned> {
    let mut size = winsize(cols, rows, cell_px);
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    // SAFETY: the out-pointers are valid, `name` and `termp` may be NULL, and `size` outlives
    // the call (it is `*const` on Linux and `*mut` on macOS; `*mut` coerces to both).
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::addr_of_mut!(size),
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openpty succeeded, so both are fresh descriptors this function now owns.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    set_cloexec(&master)?;
    set_cloexec(&slave)?;

    let mut cmd = Command::new(&command.program);
    cmd.args(&command.args).current_dir(cwd);
    // Setting TERMINFO replaces one the app inherited, such as a Ghostty shell's.
    match ghostty_terminfo(terminfo) {
        Some(dir) => cmd.env("TERM", GHOSTTY_TERM).env("TERMINFO", dir),
        None => cmd.env("TERM", FALLBACK_TERM),
    };
    cmd.env("COLORTERM", "truecolor")
        .env("TERM_PROGRAM", "Wolf")
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    // SAFETY: the hook runs in the forked child before exec and calls only setsid and ioctl,
    // which are async-signal-safe. Descriptor 0 is the slave by then.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    // Dropping the command closes the parent's copies of the slave.
    drop(cmd);
    set_nonblocking(&master)?;
    Ok(Spawned { child, master })
}

/// `dir` when it holds the compiled `xterm-ghostty` entry in either layout.
fn ghostty_terminfo(dir: Option<&Path>) -> Option<&Path> {
    dir.filter(|dir| {
        GHOSTTY_ENTRY_PATHS
            .iter()
            .any(|entry| dir.join(entry).is_file())
    })
}

/// The command a session without an explicit one runs: `$SHELL` when set and non-empty, else
/// the user's passwd shell, else `/bin/sh`; a login shell on macOS, as its terminals start.
pub(crate) fn default_command() -> TerminalCommand {
    default_command_from(
        std::env::var_os("SHELL").as_deref(),
        passwd_shell(),
        cfg!(target_os = "macos"),
    )
}

/// [`default_command`] over its inputs.
pub(crate) fn default_command_from(
    env_shell: Option<&OsStr>,
    passwd_shell: Option<PathBuf>,
    login: bool,
) -> TerminalCommand {
    let program = env_shell
        .filter(|shell| !shell.is_empty())
        .map(PathBuf::from)
        .or(passwd_shell.filter(|shell| !shell.as_os_str().is_empty()))
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    TerminalCommand {
        program,
        args: if login {
            vec!["-l".to_owned()]
        } else {
            Vec::new()
        },
    }
}

/// The login shell in the current user's passwd entry.
fn passwd_shell() -> Option<PathBuf> {
    let mut buf = vec![0 as libc::c_char; 16 * 1024];
    // SAFETY: an all-zero passwd is a valid out-parameter.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = ptr::null_mut();
    // SAFETY: every pointer is valid for the call, and `buf` is valid for its length.
    let rc = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            &mut entry,
            buf.as_mut_ptr(),
            buf.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() || entry.pw_shell.is_null() {
        return None;
    }
    // SAFETY: on success `pw_shell` is a NUL-terminated string inside `buf`, still alive.
    let shell = unsafe { std::ffi::CStr::from_ptr(entry.pw_shell) };
    Some(PathBuf::from(OsStr::from_bytes(shell.to_bytes())))
}

/// The directory a pwd report names: a `file://host/path` URI, percent-decoded, or a bare
/// absolute path. `None` for an empty report or anything else.
pub(crate) fn decode_pwd(raw: &str) -> Option<PathBuf> {
    let path = if let Some(rest) = raw.strip_prefix("file://") {
        let slash = rest.find('/')?;
        percent_decode(&rest[slash..])
    } else if raw.starts_with('/') {
        raw.as_bytes().to_vec()
    } else {
        return None;
    };
    Some(PathBuf::from(OsString::from_vec(path)))
}

/// Decodes `%XX` escapes; a `%` not followed by two hex digits stays as it is.
fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    out
}

/// The operating system's view of process `pid`'s working directory.
#[cfg(target_os = "linux")]
pub(crate) fn process_cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

/// The operating system's view of process `pid`'s working directory.
#[cfg(target_os = "macos")]
pub(crate) fn process_cwd(pid: u32) -> Option<PathBuf> {
    // SAFETY: an all-zero proc_vnodepathinfo is a valid out-parameter.
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>();
    // SAFETY: `info` is valid for `size` bytes, which is what PROC_PIDVNODEPATHINFO writes.
    let written = unsafe {
        libc::proc_pidinfo(
            i32::try_from(pid).ok()?,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            ptr::addr_of_mut!(info).cast(),
            i32::try_from(size).ok()?,
        )
    };
    if usize::try_from(written).ok()? < size {
        return None;
    }
    // SAFETY: vip_path is MAXPATHLEN bytes of a NUL-terminated path, laid out contiguously.
    let path =
        unsafe { std::ffi::CStr::from_ptr(info.pvi_cdir.vip_path.as_ptr().cast::<libc::c_char>()) };
    let bytes = path.to_bytes();
    (!bytes.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(bytes)))
}

/// Tells the pty its size, which sends the foreground process group `SIGWINCH`.
pub(crate) fn set_window_size(
    master: &OwnedFd,
    (cols, rows): (u16, u16),
    cell_px: (u32, u32),
) -> io::Result<()> {
    let size = winsize(cols, rows, cell_px);
    // SAFETY: `master` is an open pty master and TIOCSWINSZ reads one winsize.
    if unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ as _, &size) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Sends `signal` to the process group `pid` leads.
pub(crate) fn signal_group(pid: u32, signal: libc::c_int) {
    if let Ok(pid) = i32::try_from(pid) {
        // SAFETY: kill takes plain values; a group that is gone is an ignored ESRCH.
        unsafe {
            libc::kill(-pid, signal);
        }
    }
}

/// The session's `exited` value: the exit code, or 128 plus the signal for a signal death.
pub(crate) fn exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(-1)
}

fn winsize(cols: u16, rows: u16, (cell_w, cell_h): (u32, u32)) -> libc::winsize {
    let px = |cells: u16, cell: u32| u16::try_from(u32::from(cells) * cell).unwrap_or(u16::MAX);
    libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: px(cols, cell_w),
        ws_ypixel: px(rows, cell_h),
    }
}

fn set_cloexec(fd: &OwnedFd) -> io::Result<()> {
    // SAFETY: F_GETFD and F_SETFD on a descriptor this process owns.
    unsafe {
        let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFD);
        if flags == -1 || libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, flags | libc::FD_CLOEXEC) == -1
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub(crate) fn set_nonblocking(fd: &OwnedFd) -> io::Result<()> {
    // SAFETY: F_GETFL and F_SETFL on a descriptor this process owns.
    unsafe {
        let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFL);
        if flags == -1 || libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) == -1
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// A close-on-exec, non-blocking pipe: `(read end, write end)`.
pub(crate) fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds: [RawFd; 2] = [-1; 2];
    // SAFETY: `fds` holds the two descriptors pipe writes.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: pipe succeeded, so both are fresh descriptors this function now owns.
    let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    for fd in [&read, &write] {
        set_cloexec(fd)?;
        set_nonblocking(fd)?;
    }
    Ok((read, write))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::PathBuf;

    use super::{decode_pwd, default_command_from};

    #[test]
    fn default_command_prefers_shell_then_passwd_then_bin_sh() {
        let passwd = || Some(PathBuf::from("/bin/passwd-shell"));
        let shell = default_command_from(Some(OsStr::new("/bin/env-shell")), passwd(), false);
        assert_eq!(
            shell.program,
            PathBuf::from("/bin/env-shell"),
            "a set $SHELL must win over the passwd entry"
        );
        let empty = default_command_from(Some(OsStr::new("")), passwd(), false);
        assert_eq!(
            empty.program,
            PathBuf::from("/bin/passwd-shell"),
            "an empty $SHELL must fall through to the passwd entry"
        );
        let unset = default_command_from(None, passwd(), false);
        assert_eq!(unset.program, PathBuf::from("/bin/passwd-shell"));
        let neither = default_command_from(None, None, false);
        assert_eq!(
            neither.program,
            PathBuf::from("/bin/sh"),
            "with neither, the shell must be /bin/sh"
        );
    }

    #[test]
    fn default_command_is_a_login_shell_only_when_asked() {
        let login = default_command_from(Some(OsStr::new("/bin/zsh")), None, true);
        assert_eq!(login.args, vec!["-l".to_owned()]);
        let plain = default_command_from(Some(OsStr::new("/bin/bash")), None, false);
        assert!(
            plain.args.is_empty(),
            "a non-login shell takes no arguments"
        );
    }

    #[test]
    fn pwd_reports_decode_to_paths() {
        assert_eq!(
            decode_pwd("file://host/a%20b"),
            Some(PathBuf::from("/a b")),
            "a file URI's host is dropped and its path percent-decoded"
        );
        assert_eq!(decode_pwd("file:///x"), Some(PathBuf::from("/x")));
        assert_eq!(decode_pwd("/y"), Some(PathBuf::from("/y")));
        assert_eq!(decode_pwd(""), None, "an empty report names no directory");
    }
}
