//! Pure display-string math for the per-pane statusbar (plan §4).
//!
//! This module is IO-free string/path arithmetic over ALREADY-CANONICAL inputs:
//! the caller ([`crate::api::WolfCore::pane_status`]) canonicalizes the buffer
//! path, cwd, and home before calling [`pane_status`] so that `strip_prefix`
//! lines up. It draws no pixels and touches no filesystem.
//!
//! The result is a [`PaneStatus`] with two strings: `filename` (the base name of
//! the buffer file, or [`NO_NAME`] for an unnamed buffer) and `display_path` (the
//! path shown beneath it — cwd-relative when the file lives under cwd, else the
//! absolute path with a `~` prefix when under home, else the raw absolute path).

use std::path::Path;

/// The placeholder filename shown for an unnamed buffer (empty buffer path).
pub const NO_NAME: &str = "[No Name]";

/// The two display strings a pane's statusbar shows: the base `filename` and the
/// `display_path` beneath it. Both fields are public; see [`pane_status`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneStatus {
    /// The base name of the buffer file, or [`NO_NAME`] for an unnamed buffer.
    pub filename: String,
    /// The path shown beneath the filename: cwd-relative when under cwd, else the
    /// absolute path (with a `~` prefix when under home), else empty for an
    /// unnamed buffer.
    pub display_path: String,
}

/// Computes the [`PaneStatus`] for a buffer path, given the (already-canonical)
/// cwd and home directories (plan §4).
///
/// Precedence for `display_path`:
/// 1. Empty `buf_path` (unnamed buffer) → `{ filename: NO_NAME, display_path: "" }`.
/// 2. Otherwise `filename` is the path's `file_name()` (lossy), falling back to
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
    }
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
