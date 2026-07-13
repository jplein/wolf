//! Pure, headless tests for the per-pane statusbar display-string function
//! (`wolf_core::pane_status` + the `PaneStatus` type + the `NO_NAME` constant).
//! No mocks, no Neovim, no filesystem — `pane_status` is IO-free string/path math
//! over already-canonical inputs (plan §4).
//!
//! These live in a dedicated integration file (with its own `pane_status_unit`
//! BUILD target) rather than an in-module `#[cfg(test)] mod tests` inside the NEW
//! `src/pane_status.rs`, to avoid colliding with the coder who creates that file.
//! This mirrors the crate's precedent (`tests/palette_rank.rs`, `tests/commands.rs`).
//!
//! ## Dependency on coder work (plan §4 / §8)
//!
//! None of this exists yet — the coder adds `pub mod pane_status;` plus
//! `pub use pane_status::{pane_status, PaneStatus, NO_NAME};` in
//! `crates/wolf-core/src/lib.rs`, and adds the `//crates/wolf-core:pane_status_unit`
//! `rust_test` target to `BUILD.bazel`. This file does NOT touch non-test source.
//! It assumes the plan's exact signatures:
//!
//! - `pub const NO_NAME: &str = "[No Name]";`
//! - `pub struct PaneStatus { pub filename: String, pub display_path: String }`
//! - `pub fn pane_status(buf_path: &str, cwd: Option<&Path>, home: Option<&Path>)
//!    -> PaneStatus`
//!
//! Paths here are forward-slash POSIX literals — these tests run on macOS/Linux.
//! The caller (`WolfCore::pane_status`) passes already-canonicalized cwd/home/
//! buf_path so `strip_prefix` lines up; these unit tests supply pre-aligned paths.

use std::path::Path;

use wolf_core::{pane_status, PaneStatus, NO_NAME};

/// Thin wrapper so cases can pass `Option<&str>` and read like the plan's table.
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
