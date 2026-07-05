//! Pure candidate assembly + ranking for the command palette.
//!
//! This module is deliberately IO-free: given the recent directories, the OTHER
//! open windows, the command MRU, and the user's query, it assembles the palette
//! candidate rows and ranks them best-first. That purity is what lets it be
//! unit-tested headlessly (no filesystem, no Neovim, no GUI), exactly like
//! [`crate::rank`], whose [`rank_matches`](crate::rank::rank_matches) it REUSES
//! for the fuzzy match + MRU-first ordering.
//!
//! ## Candidate assembly (stable order, tie-break-relevant)
//!
//! 1. Every [`crate::command::COMMAND_SPECS`] row EXCEPT
//!    [`Command::CommandPalette`](crate::command::Command::CommandPalette) becomes
//!    a [`PaletteRowKind::Static`] row (the palette cannot dispatch itself).
//! 2. Each [`PaletteWindow`] becomes a [`PaletteRowKind::SwitchWindow`] row
//!    (labeled by its cwd, or `"Untitled"` when empty).
//! 3. Each recent directory becomes a [`PaletteRowKind::OpenRecent`] row.
//!
//! Each candidate's MRU rank is the index of its `mru_key` in
//! `command_mru_recent` (or `None`). The candidates are projected into a parallel
//! `Vec<`[`FileEntry`](crate::files::FileEntry)`>` (empty path, the display
//! string, the MRU rank), ranked via
//! [`rank_matches`](crate::rank::rank_matches), truncated to `max`, and each
//! surviving `Match.index` maps back to a cloned [`PaletteRow`].

use std::path::PathBuf;

use crate::command::{Command, COMMAND_SPECS};
use crate::files::FileEntry;
use crate::rank::rank_matches;

/// The kind of a [`PaletteRow`] — which of the three palette actions it performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteRowKind {
    /// A static registry command (dispatched via the command channel).
    Static,
    /// A "switch to window" row (raises the window at `window_index`).
    SwitchWindow,
    /// An "open recent folder" row (opens-or-raises the recent dir at `path`).
    OpenRecent,
}

/// One OTHER open window fed to [`rank_palette`] (the current window is excluded
/// by the caller): its front-end window index and its current working directory.
#[derive(Debug, Clone)]
pub struct PaletteWindow {
    /// The front-end window index (the raise target).
    pub index: u16,
    /// The window's current working directory (`""` for an empty-state window).
    pub cwd: String,
}

/// One assembled + ranked palette candidate row.
///
/// Carries the display label, the opaque MRU `mru_key`, the row `kind`, and the
/// kind-specific payload (`command` for a static row, `window_index` for a
/// switch-window row, `path` for an open-recent row).
#[derive(Debug, Clone)]
pub struct PaletteRow {
    /// The label shown in the palette (and the string ranked against the query).
    pub display: String,
    /// The opaque key recorded in the command MRU on activation.
    pub mru_key: String,
    /// Which of the three palette actions this row performs.
    pub kind: PaletteRowKind,
    /// The static registry command (only for a [`PaletteRowKind::Static`] row).
    pub command: Option<Command>,
    /// The raise-target window index (only for a [`PaletteRowKind::SwitchWindow`]
    /// row).
    pub window_index: Option<u16>,
    /// The recent-folder path (only for a [`PaletteRowKind::OpenRecent`] row).
    pub path: Option<PathBuf>,
}

/// Assembles the palette candidate rows and ranks them best-first for `query`.
///
/// See the module docs for the candidate assembly order and the reuse of
/// [`rank_matches`](crate::rank::rank_matches) (which applies the fuzzy match and
/// the MRU-first ordering). An empty `query` returns all candidates MRU-first;
/// the result is truncated to at most `max` rows.
#[must_use]
pub fn rank_palette(
    recent_dirs: &[PathBuf],
    windows: &[PaletteWindow],
    command_mru_recent: &[String],
    query: &str,
    max: usize,
) -> Vec<PaletteRow> {
    // (1) Static registry commands, EXCEPT the palette's own invoking command.
    let mut rows: Vec<PaletteRow> = Vec::new();
    for spec in COMMAND_SPECS {
        if spec.command == Command::CommandPalette {
            continue;
        }
        rows.push(PaletteRow {
            display: spec.title.to_string(),
            mru_key: spec.id.to_string(),
            kind: PaletteRowKind::Static,
            command: Some(spec.command),
            window_index: None,
            path: None,
        });
    }

    // (2) One "Switch to Window" row per OTHER open window.
    for window in windows {
        let label = if window.cwd.is_empty() {
            "Untitled".to_string()
        } else {
            window.cwd.clone()
        };
        rows.push(PaletteRow {
            display: format!("Switch to Window: {label}"),
            mru_key: format!("switch_window:{}", window.cwd),
            kind: PaletteRowKind::SwitchWindow,
            command: None,
            window_index: Some(window.index),
            path: None,
        });
    }

    // (3) One "Open Recent Folder" row per recent dir.
    for dir in recent_dirs {
        let path_str = dir.to_string_lossy();
        rows.push(PaletteRow {
            display: format!("Open Recent Folder: {path_str}"),
            mru_key: format!("open_recent:{path_str}"),
            kind: PaletteRowKind::OpenRecent,
            command: None,
            window_index: None,
            path: Some(dir.clone()),
        });
    }

    // Project into parallel FileEntry candidates (path is unused by ranking), with
    // each candidate's MRU rank = index of its mru_key in `command_mru_recent`.
    let entries: Vec<FileEntry> = rows
        .iter()
        .map(|row| FileEntry {
            path: PathBuf::new(),
            display: row.display.clone(),
            mru_rank: mru_rank(&row.mru_key, command_mru_recent),
        })
        .collect();

    rank_matches(query, &entries)
        .into_iter()
        .take(max)
        .map(|m| rows[m.index].clone())
        .collect()
}

/// The MRU rank of `key`: the index of the first `command_mru_recent` entry equal
/// to `key` (as a `u32`), or `None` when the key is not in the MRU list.
fn mru_rank(key: &str, command_mru_recent: &[String]) -> Option<u32> {
    command_mru_recent
        .iter()
        .position(|k| k == key)
        .and_then(|i| u32::try_from(i).ok())
}
