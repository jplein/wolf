//! Pure candidate assembly + ranking for the buffer picker.
//!
//! This module is deliberately IO-free: given a snapshot of the open buffers
//! (queried once from Neovim at picker-open), the current working directory, and
//! the user's query, it MRU-orders the buffers, projects them to display strings,
//! and ranks them best-first. That purity is what lets it be unit-tested
//! headlessly (no filesystem, no Neovim, no GUI), exactly like [`crate::palette`],
//! whose reuse of [`rank_matches`](crate::rank::rank_matches) for the fuzzy match
//! + MRU-first ordering it mirrors.
//!
//! ## Ordering + assembly
//!
//! 1. The buffers are sorted by `lastused` DESCending (a stable sort, tie-broken
//!    by `bufnr` ASCending). This puts the current buffer (highest `lastused`) at
//!    index 0 — INCLUDED in the result, not filtered out.
//! 2. Each sorted buffer is projected into a [`FileEntry`](crate::files::FileEntry)
//!    (empty path, its cwd-relative display string, `mru_rank` = its sorted index)
//!    and ranked via [`rank_matches`](crate::rank::rank_matches), truncated to
//!    `max`, and each surviving `Match.index` maps back to a cloned [`BufferRow`].
//!
//! ## Display string (no `~`-home fold)
//!
//! An empty buffer name → [`NO_NAME`](crate::pane_status::NO_NAME); a name under
//! `cwd` → the cwd-relative remainder (lossy); any other name → the absolute name
//! verbatim.

use std::path::{Path, PathBuf};

use crate::files::FileEntry;
use crate::pane_status::NO_NAME;
use crate::rank::rank_matches;

/// One open buffer as queried from Neovim (`getbufinfo({'buflisted': 1})`).
#[derive(Debug, Clone)]
pub struct BufInfo {
    /// The Neovim buffer handle.
    pub bufnr: i64,
    /// The buffer's full name (an absolute path, or `""` for an unnamed buffer).
    pub name: String,
    /// The `lastused` timestamp (higher = more recently used); the MRU key.
    pub lastused: i64,
    /// Whether the buffer has unsaved modifications.
    pub modified: bool,
}

/// One assembled + ranked buffer-picker row.
///
/// Carries the buffer handle to switch to, the display label (also the string
/// ranked against the query), and the modified flag for the row chrome.
#[derive(Debug, Clone)]
pub struct BufferRow {
    /// The Neovim buffer handle to switch the active window to on activation.
    pub bufnr: i64,
    /// The label shown in the picker (and the string ranked against the query).
    pub display: String,
    /// Whether the buffer has unsaved modifications.
    pub modified: bool,
}

/// MRU-orders the buffers, projects them to display strings, and ranks them
/// best-first for `query`.
///
/// See the module docs for the ordering (lastused DESC, tie-broken by bufnr ASC)
/// and the reuse of [`rank_matches`](crate::rank::rank_matches) (which applies the
/// fuzzy match and the MRU-first ordering). An empty `query` returns all buffers
/// MRU-first; the result is truncated to at most `max` rows.
#[must_use]
pub fn rank_buffers(
    buffers: &[BufInfo],
    cwd: Option<&Path>,
    query: &str,
    max: usize,
) -> Vec<BufferRow> {
    // Sort a copy by lastused DESC, stable, tie-broken by bufnr ASC. The current
    // buffer (highest lastused) lands at index 0 and IS included.
    let mut sorted: Vec<&BufInfo> = buffers.iter().collect();
    sorted.sort_by(|a, b| b.lastused.cmp(&a.lastused).then(a.bufnr.cmp(&b.bufnr)));

    let rows: Vec<BufferRow> = sorted
        .iter()
        .map(|buf| BufferRow {
            bufnr: buf.bufnr,
            display: relative_display(&buf.name, cwd),
            modified: buf.modified,
        })
        .collect();

    // Project into parallel FileEntry candidates (path is unused by ranking), with
    // each candidate's MRU rank = its index in the lastused-sorted order.
    let entries: Vec<FileEntry> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| FileEntry {
            path: PathBuf::new(),
            display: row.display.clone(),
            mru_rank: u32::try_from(i).ok(),
        })
        .collect();

    rank_matches(query, &entries)
        .into_iter()
        .take(max)
        .map(|m| rows[m.index].clone())
        .collect()
}

/// The picker display string for a buffer `name` given the current `cwd`.
///
/// An empty name → [`NO_NAME`]; a name under `cwd` → the cwd-relative remainder
/// (lossy); any other name → the absolute name verbatim (no `~`-home fold).
fn relative_display(name: &str, cwd: Option<&Path>) -> String {
    if name.is_empty() {
        return NO_NAME.to_string();
    }
    if let Some(cwd) = cwd {
        if let Ok(rel) = Path::new(name).strip_prefix(cwd) {
            if !rel.as_os_str().is_empty() {
                return rel.to_string_lossy().into_owned();
            }
        }
    }
    name.to_string()
}
