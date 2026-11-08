//! Pure display-string math for the per-pane statusbar.
//!
//! This module is IO-free string/path arithmetic over ALREADY-CANONICAL inputs:
//! the caller ([`crate::api::WolfCore::pane_status`]) canonicalizes the buffer
//! path, cwd, and home before calling [`pane_status`] so that `strip_prefix`
//! lines up. It draws no pixels and touches no filesystem.
//!
//! The result is a [`PaneStatus`] with two display strings: `filename` (the base
//! name of the buffer file, [`NO_NAME`] for an unnamed buffer, or the command's
//! name for a terminal buffer) and `display_path` (the path shown beneath it —
//! cwd-relative when the file lives under cwd, else the absolute path with a `~`
//! prefix when under home, else the raw absolute path; empty for an unnamed or
//! terminal buffer, so no path line is drawn).

use std::path::Path;

/// The placeholder filename shown for an unnamed buffer (empty buffer path).
pub const NO_NAME: &str = "[No Name]";

/// The two display strings a pane's statusbar shows: the base `filename` and the
/// `display_path` beneath it. Both fields are public; see [`pane_status`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneStatus {
    /// The base name of the buffer file, [`NO_NAME`] for an unnamed buffer, or the
    /// basename of a terminal buffer's command (`bash` for `term://…//42:/bin/bash`).
    pub filename: String,
    /// The path shown beneath the filename: cwd-relative when under cwd, else the
    /// absolute path (with a `~` prefix when under home); empty for an unnamed or
    /// terminal buffer.
    pub display_path: String,
    /// The buffer's raw absolute path (`""` for an unnamed buffer). Unlike
    /// [`display_path`](Self::display_path) this is never relativized — the macOS
    /// front-end persists it as a pane-layout leaf path.
    pub abs_path: String,
}

/// Computes the [`PaneStatus`] for a buffer path, given the (already-canonical)
/// cwd and home directories.
///
/// Precedence for `display_path`:
/// 1. Empty `buf_path` (unnamed buffer) → `{ filename: NO_NAME, display_path: "" }`.
/// 2. A terminal buffer (`term://<cwd>//<pid>:<command>`) with a command word →
///    `{ filename: <basename of the command's first word>, display_path: "" }`. A
///    `term://` name with no command word falls through to rule 3.
/// 3. Otherwise `filename` is the path's `file_name()` (lossy), falling back to
///    the full `buf_path` when it has none. `display_path`:
///    - cwd-relative WINS: when `cwd` is `Some` and `buf_path` strips its prefix
///      to a NON-EMPTY remainder, that remainder (lossy). (`buf_path == cwd`
///      yields an empty remainder and falls through.)
///    - else absolute + `~`: when `home` is `Some` and `buf_path` strips its
///      prefix, `"~"` for an empty remainder (`buf_path == home`), else
///      `"~/" + remainder`.
///    - else the full `buf_path` verbatim.
#[must_use]
pub fn pane_status(buf_path: &str, cwd: Option<&Path>, home: Option<&Path>) -> PaneStatus {
    if buf_path.is_empty() {
        return PaneStatus {
            filename: NO_NAME.to_string(),
            display_path: String::new(),
            abs_path: String::new(),
        };
    }

    if let Some(name) = terminal_command_name(buf_path) {
        return PaneStatus {
            filename: name,
            display_path: String::new(),
            abs_path: buf_path.to_string(),
        };
    }

    let path = Path::new(buf_path);

    let filename = path.file_name().map_or_else(
        || buf_path.to_string(),
        |n| n.to_string_lossy().into_owned(),
    );

    let display_path = cwd_relative(path, cwd)
        .or_else(|| home_relative(path, home))
        .unwrap_or_else(|| buf_path.to_string());

    PaneStatus {
        filename,
        display_path,
        abs_path: buf_path.to_string(),
    }
}

/// The display name of a terminal buffer `term://<cwd>//<pid>:<command>`: the
/// basename of the command's first word (`/bin/bash --login` → `bash`).
///
/// The split is at the FIRST `//` followed by one or more ASCII digits and a `:`,
/// so a later `//<digits>:` inside the command does not move it. The command is
/// split naively on whitespace, so a quoted program path containing spaces is cut
/// at its first space. `None` when `buf_path` lacks the prefix, the separator or a
/// command word.
fn terminal_command_name(buf_path: &str) -> Option<String> {
    let rest = buf_path.strip_prefix(crate::redraw::TERMINAL_BUFFER_PREFIX)?;
    let command = (0..rest.len()).find_map(|i| {
        let after = rest.get(i..)?.strip_prefix("//")?;
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        after.get(digits..)?.strip_prefix(':')
    })?;
    let word = command.split_whitespace().next()?;
    Some(
        Path::new(word)
            .file_name()
            .map_or_else(|| word.to_string(), |n| n.to_string_lossy().into_owned()),
    )
}

/// The cwd-relative remainder, when `cwd` is set and the path strips its prefix
/// to a NON-EMPTY remainder. `path == cwd` (empty remainder) returns `None` so
/// the caller falls through to the absolute forms.
fn cwd_relative(path: &Path, cwd: Option<&Path>) -> Option<String> {
    let rel = path.strip_prefix(cwd?).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some(rel.to_string_lossy().into_owned())
}

/// The `~`-prefixed home form, when `home` is set and the path strips its prefix:
/// a bare `~` for an empty remainder (`path == home`), else `"~/" + remainder`.
fn home_relative(path: &Path, home: Option<&Path>) -> Option<String> {
    let rel = path.strip_prefix(home?).ok()?;
    if rel.as_os_str().is_empty() {
        Some("~".to_string())
    } else {
        Some(format!("~/{}", rel.to_string_lossy()))
    }
}
