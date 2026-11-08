//! Pure, headless tests for the per-pane statusbar display-string function
//! (`wolf_core::pane_status` + the `PaneStatus` type + the `NO_NAME` constant).
//! No mocks, no Neovim, no filesystem — `pane_status` is IO-free string/path math
//! over already-canonical inputs.
//!
//! These live in a dedicated integration file (with its own `pane_status_unit`
//! BUILD target) rather than an in-module `#[cfg(test)] mod tests` inside
//! `src/pane_status.rs`, mirroring the crate's precedent (`tests/palette_rank.rs`,
//! `tests/commands.rs`).
//!
//! ## Surface under test
//!
//! The crate root re-exports `pane_status`, `PaneStatus` and `NO_NAME`, and the
//! `//crates/wolf-core:pane_status_unit` `rust_test` target runs this file.
//! The signatures:
//!
//! - `pub const NO_NAME: &str = "[No Name]";`
//! - `pub struct PaneStatus { pub filename: String, pub display_path: String,
//!    pub abs_path: String }`
//! - `pub fn pane_status(buf_path: &str, cwd: Option<&Path>, home: Option<&Path>)
//!    -> PaneStatus`
//!
//! A terminal buffer (`term://<cwd>//<pid>:<command>`) has its own arm: `filename`
//! is the basename of the command's first word, `display_path` is empty and
//! `abs_path` is the raw name. A `term://` name with no command word, and a path
//! that merely contains `term://`, fall through to the path rules.
//!
//! Paths here are forward-slash POSIX literals — these tests run on macOS/Linux.
//! The caller (`WolfCore::pane_status`) passes already-canonicalized cwd/home/
//! buf_path so `strip_prefix` lines up; these unit tests supply pre-aligned paths.

use std::path::Path;

use wolf_core::{pane_status, PaneStatus, NO_NAME};

/// Thin wrapper so cases can pass `Option<&str>` and read like a table.
fn status(buf: &str, cwd: Option<&str>, home: Option<&str>) -> PaneStatus {
    pane_status(buf, cwd.map(Path::new), home.map(Path::new))
}

/// An empty `buf_path` (unnamed buffer) yields the `[No Name]` filename and an
/// EMPTY display path — never a relative "" that the caller might mistake for a
/// real path.
#[test]
fn empty_buf_path_is_no_name_with_empty_display() {
    let s = status("", Some("/a/b"), Some("/Users/u"));
    assert_eq!(s.filename, NO_NAME, "empty buf_path -> filename is NO_NAME");
    assert_eq!(
        s.display_path, "",
        "empty buf_path -> display_path is empty"
    );
}

/// A file NESTED under cwd shows its base name and the cwd-RELATIVE remainder.
#[test]
fn nested_under_cwd_is_relative_remainder() {
    let s = status("/a/b/sub/x.md", Some("/a/b"), None);
    assert_eq!(s.filename, "x.md");
    assert_eq!(s.display_path, "sub/x.md");
}

/// A file DIRECTLY under cwd shows just the base name as its relative path.
#[test]
fn direct_under_cwd_is_bare_name() {
    let s = status("/a/b/x.md", Some("/a/b"), None);
    assert_eq!(s.filename, "x.md");
    assert_eq!(s.display_path, "x.md");
}

/// A file NOT under cwd but under home renders with a `~` prefix (absolute-branch).
#[test]
fn under_home_not_cwd_uses_tilde_prefix() {
    let s = status("/Users/u/w/n.md", Some("/tmp/p"), Some("/Users/u"));
    assert_eq!(s.filename, "n.md");
    assert_eq!(s.display_path, "~/w/n.md");
}

/// When the path IS home itself, the filename is home's basename and the display
/// path collapses to a bare `~` (empty home remainder).
#[test]
fn path_equal_home_is_bare_tilde() {
    let s = status("/Users/u", Some("/tmp/p"), Some("/Users/u"));
    assert_eq!(
        s.filename, "u",
        "filename is home's basename when path == home"
    );
    assert_eq!(s.display_path, "~", "path == home collapses to a bare ~");
}

/// A path under NEITHER cwd nor home is shown as its absolute string, unchanged.
#[test]
fn under_neither_is_absolute_passthrough() {
    let s = status("/x/y/z.md", Some("/a/b"), Some("/Users/u"));
    assert_eq!(s.filename, "z.md");
    assert_eq!(s.display_path, "/x/y/z.md");
}

/// EDGE: when the path EQUALS cwd, the cwd-relative remainder would be empty, so
/// the function must NOT emit an empty relative string. With no home it falls
/// through to the absolute form — asserted non-empty AND equal to the absolute.
#[test]
fn path_equal_cwd_is_not_empty_relative() {
    let s = status("/a/b", Some("/a/b"), None);
    assert_eq!(s.filename, "b");
    assert!(
        !s.display_path.is_empty(),
        "path == cwd must not yield an empty relative display_path"
    );
    assert_eq!(
        s.display_path, "/a/b",
        "with cwd remainder empty and no home, display_path is the absolute path"
    );
}

/// PRECEDENCE: a file that is under BOTH cwd and home resolves cwd-relative (no
/// `~`) because the cwd branch wins.
#[test]
fn cwd_wins_over_home_when_both_apply() {
    let s = status(
        "/Users/u/proj/a.md",
        Some("/Users/u/proj"),
        Some("/Users/u"),
    );
    assert_eq!(s.filename, "a.md");
    assert_eq!(
        s.display_path, "a.md",
        "cwd-relative wins over the home ~ form when both would match"
    );
}

/// With NO cwd and NO home, the display path is the absolute `buf_path` verbatim.
#[test]
fn no_cwd_no_home_is_absolute_passthrough() {
    let buf = "/x/y/z.md";
    let s = status(buf, None, None);
    assert_eq!(s.filename, "z.md");
    assert_eq!(
        s.display_path, buf,
        "cwd=None,home=None -> absolute passthrough"
    );
}

/// The cwd and home every terminal row is evaluated against.
const TERM_CWD: &str = "/home/u/proj";
const TERM_HOME: &str = "/home/u";

/// Asserts the terminal arm for one `term://` buffer name: `filename` is the basename
/// of the command's first word, `display_path` is empty, and `abs_path` is the raw name.
fn assert_terminal(buf: &str, expected_name: &str) {
    let s = status(buf, Some(TERM_CWD), Some(TERM_HOME));
    assert_eq!(
        s.display_path, "",
        "{buf:?} is a terminal, so display_path must be empty (no path line); a non-empty \
         value means the terminal arm did not fire and the path rules ran instead"
    );
    assert_eq!(
        s.filename, expected_name,
        "{buf:?} must display as the basename of its command's first word; a different \
         value means the arm split on the wrong `//<pid>:` or kept the arguments or path"
    );
    assert_eq!(
        s.abs_path, buf,
        "{buf:?}: abs_path must stay the raw buffer name, which macOS persists"
    );
}

/// A terminal whose shell is an absolute path shows the shell's basename.
#[test]
fn terminal_absolute_shell_is_its_basename() {
    assert_terminal("term:///tmp/proj//4242:/bin/bash", "bash");
}

/// A terminal whose command carries arguments shows only the first word's basename.
#[test]
fn terminal_command_arguments_are_dropped() {
    assert_terminal(
        "term://~/proj//4242:/nix/store/abc-bash-5.2/bin/bash --login",
        "bash",
    );
}

/// A bare command word (no directory) is shown as is; the path rules would read the
/// last component of `term://~//7:zsh` as `7:zsh`.
#[test]
fn terminal_bare_command_word() {
    assert_terminal("term://~//7:zsh", "zsh");
}

/// The split is at the FIRST `//<digits>:` after the prefix: a later one inside the
/// command (here a URL argument) does not move it.
#[test]
fn terminal_split_is_at_the_first_pid_separator() {
    assert_terminal("term://~//42:bash -c 'curl http://x//1:y'", "bash");
}

/// Neovim's form for a terminal opened with cwd `/`.
#[test]
fn terminal_root_cwd() {
    assert_terminal("term:///.//9:sh", "sh");
}

/// A real file whose path merely CONTAINS `term://` is not a terminal: the arm keys on
/// the prefix, so the path rules apply.
#[test]
fn term_scheme_inside_a_path_is_not_a_terminal() {
    let buf = "/home/u/proj/term://weird.md";
    let s = status(buf, Some(TERM_CWD), Some(TERM_HOME));
    assert_eq!(
        s.filename, "weird.md",
        "{buf:?} is a file, so filename follows the path rules; another value means the \
         terminal arm matched `term://` somewhere other than the start"
    );
    assert!(
        !s.display_path.is_empty(),
        "{buf:?} is a file, so it must keep a path line; an empty display_path means the \
         terminal arm matched `term://` somewhere other than the start"
    );
}

/// A `term://` name with no command word falls through to the path rules, whose
/// display path for a name under neither cwd nor home is the raw input.
#[test]
fn terminal_without_command_word_falls_through() {
    for buf in ["term://", "term://~//7:"] {
        let s = status(buf, Some(TERM_CWD), Some(TERM_HOME));
        assert_eq!(
            s.display_path, buf,
            "{buf:?} has no command word, so the path rules must apply and display it raw; \
             an empty display_path means the arm fired without a command to name"
        );
    }
}
