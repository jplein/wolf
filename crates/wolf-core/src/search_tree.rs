//! Pure, toolkit-free projection + display math for the sidebar SEARCH-results
//! tree.
//!
//! The content-search session (`wolf-core`'s [`crate::search::SearchSession`])
//! produces a *flat* list of [`crate::SearchMatch`] values in rg emission order.
//! The search panel, however, shows a two-level tree: each matched FILE with its
//! matching LINES beneath it. This module holds the pure projection from the flat
//! match list into per-file groups (sorted for display), plus the per-line
//! balanced-context "window" computation that trims a long line down to fit the
//! narrow sidebar.
//!
//! Keeping this toolkit-free is what lets it be unit-tested headlessly, exactly
//! mirroring how [`crate::tree`] keeps the file-tree projection pure. The platform
//! search views (`wolf-macos`'s `SearchView`) are the only places that wrap these
//! results in widgets. Width is an integer CHARACTER budget (never pixels), so the
//! math is display-server-free; the front-end converts its pixel width to a char
//! budget and adds the ellipsis glyphs.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::search::SearchMatch;

/// One matched file in the projected search tree: its absolute path, its
/// `/`-separated relative path (the sort key), the display `label`
/// (`"<basename> - <parent dir>"`), and the indices (into the flat match slice
/// passed to [`project`]) of the lines that matched in this file, ordered by line
/// number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchFileGroup {
    /// Absolute path to the matched file (used to issue `open_at`).
    pub abs_path: PathBuf,
    /// The file's `/`-separated path relative to the search root (the
    /// case-insensitive sort key).
    pub rel_path: String,
    /// The display label: `"<basename> - <relative parent dir>"`, or just the
    /// basename for a root file (see [`file_label`]).
    pub label: String,
    /// Indices into the flat match slice passed to [`project`], ordered by line
    /// number, of the lines that matched in this file.
    pub match_indices: Vec<usize>,
}

/// The visible slice of a matched line, trimmed to a character budget and centered
/// on the first match, with the bold match ranges re-based to the slice.
///
/// `text` is a substring of the original line (sliced on CHAR boundaries, never
/// splitting a scalar). `ranges` are byte `[start, end)` offsets INTO `text`
/// (matching AppKit's per-range bold styling). `truncated_left` / `truncated_right`
/// say whether context was hidden on each side (the front-end adds an ellipsis
/// glyph there — kept out of `text` so the byte offsets stay clean).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextWindow {
    /// The visible substring of the original line (char-boundary sliced).
    pub text: String,
    /// Bold match ranges as byte `[start, end)` offsets into [`text`](Self::text).
    pub ranges: Vec<(u32, u32)>,
    /// `true` when context was hidden on the LEFT (window starts after the line
    /// start) — the front-end shows a leading ellipsis.
    pub truncated_left: bool,
    /// `true` when context was hidden on the RIGHT (window ends before the line
    /// end) — the front-end shows a trailing ellipsis.
    pub truncated_right: bool,
}

/// Projects the flat match list into per-file groups, sorted for display.
///
/// Matches are grouped by [`SearchMatch::abs_path`]; groups are sorted by
/// [`SearchMatch::rel_path`] CASE-INSENSITIVELY (via `to_ascii_lowercase`, with
/// the raw `rel_path` as a deterministic tie-break, mirroring [`crate::tree`]'s
/// `node_cmp`); each group's `match_indices` are sorted by line number (a stable
/// sort, so equal line numbers keep emission order). This is the sort-on-read
/// step: `matches` accumulate in rg emission order and are ordered only here, at
/// projection time.
#[must_use]
pub fn project(matches: &[SearchMatch]) -> Vec<SearchFileGroup> {
    // Group by abs_path, preserving first-seen order for a deterministic build
    // (the final order is imposed by the case-insensitive sort below).
    let mut order: Vec<PathBuf> = Vec::new();
    let mut map: HashMap<PathBuf, SearchFileGroup> = HashMap::new();

    for (i, m) in matches.iter().enumerate() {
        let group = map.entry(m.abs_path.clone()).or_insert_with(|| {
            order.push(m.abs_path.clone());
            SearchFileGroup {
                abs_path: m.abs_path.clone(),
                rel_path: m.rel_path.clone(),
                label: file_label(&m.rel_path),
                match_indices: Vec::new(),
            }
        });
        group.match_indices.push(i);
    }

    let mut groups: Vec<SearchFileGroup> = order
        .into_iter()
        .map(|p| map.remove(&p).expect("every ordered key is present"))
        .collect();

    for group in &mut groups {
        // Stable sort by line number keeps equal-line matches in emission order.
        group.match_indices.sort_by_key(|&i| matches[i].line_number);
    }

    groups.sort_by(|a, b| {
        a.rel_path
            .to_ascii_lowercase()
            .cmp(&b.rel_path.to_ascii_lowercase())
            .then_with(|| a.rel_path.cmp(&b.rel_path))
    });

    groups
}

/// Builds the display label for a matched file's `/`-separated relative path:
/// `"<basename> - <relative parent dir>"`, e.g. `"foo/bar/baz.md"` ->
/// `"baz.md - foo/bar"`. A root file (no parent) is just its basename, e.g.
/// `"baz.md"` -> `"baz.md"` (no `" - "` suffix).
#[must_use]
pub fn file_label(rel_path: &str) -> String {
    match rel_path.rsplit_once('/') {
        Some((parent, base)) => format!("{base} - {parent}"),
        None => rel_path.to_string(),
    }
}

/// Trims `line` to a `budget` of CHARACTERS, centered on the FIRST match range,
/// with roughly equal context on either side, and re-bases the bold `ranges` to
/// the returned window.
///
/// If `line`'s char count is `<= budget` the whole line is returned unchanged
/// (both sides un-truncated, `ranges` untouched). Otherwise the window is a
/// `budget`-char slice: the match plus `remaining = budget - match_len` chars of
/// context split evenly left/right, "balanced but greedy at edges" — when the
/// match sits near one end, the leftover budget is spent on the other side. The
/// slice is taken on CHAR boundaries (never splitting a scalar); each input range
/// is clamped to the window and re-based to byte offsets into the returned `text`
/// (ranges fully outside the window are dropped). `truncated_left` /
/// `truncated_right` reflect whether the window starts/ends inside the line.
#[must_use]
pub fn context_window(line: &str, ranges: &[(u32, u32)], budget: usize) -> ContextWindow {
    // Byte offset of each char boundary, plus the end sentinel: `boundaries[k]` is
    // the byte offset of char `k`, and `boundaries[char_count]` is `line.len()`.
    let mut boundaries: Vec<usize> = line.char_indices().map(|(i, _)| i).collect();
    boundaries.push(line.len());
    let char_count = boundaries.len() - 1;

    // Short (or exact-fit) line: return whole, ranges untouched, no truncation.
    if char_count <= budget {
        return ContextWindow {
            text: line.to_string(),
            ranges: ranges.to_vec(),
            truncated_left: false,
            truncated_right: false,
        };
    }

    // Center on the FIRST match range (byte offsets -> char indices). With no
    // ranges, center on the line start (show the first `budget` chars).
    let (match_start_char, match_end_char) = match ranges.first() {
        Some(&(s, e)) => (
            byte_to_char(&boundaries, s as usize),
            byte_to_char(&boundaries, e as usize),
        ),
        None => (0, 0),
    };
    let match_len = match_end_char.saturating_sub(match_start_char);

    // Split the leftover budget evenly, then rebalance greedily at the edges.
    let context = budget.saturating_sub(match_len);
    let mut left_ctx = context / 2;
    let mut right_ctx = context - left_ctx;

    // Not enough room on the left: clamp and spend the overflow on the right.
    if match_start_char < left_ctx {
        let overflow = left_ctx - match_start_char;
        left_ctx = match_start_char;
        right_ctx += overflow;
    }
    // Not enough room on the right: clamp and spend the overflow on the left.
    let right_avail = char_count - match_end_char;
    if right_ctx > right_avail {
        let overflow = right_ctx - right_avail;
        right_ctx = right_avail;
        left_ctx = (left_ctx + overflow).min(match_start_char);
    }

    let win_start_char = match_start_char - left_ctx;
    let win_end_char = match_end_char + right_ctx;

    let win_byte_start = boundaries[win_start_char];
    let win_byte_end = boundaries[win_end_char];
    let text = line[win_byte_start..win_byte_end].to_string();

    // Re-base every intersecting range to window-relative byte offsets, clamping
    // partials and dropping ranges fully outside the window.
    let mut out_ranges: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for &(s, e) in ranges {
        let (s, e) = (s as usize, e as usize);
        let ns = s.max(win_byte_start);
        let ne = e.min(win_byte_end);
        if ns < ne {
            out_ranges.push(((ns - win_byte_start) as u32, (ne - win_byte_start) as u32));
        }
    }

    ContextWindow {
        text,
        ranges: out_ranges,
        truncated_left: win_start_char > 0,
        truncated_right: win_end_char < char_count,
    }
}

/// Maps a byte offset (which MUST be a char boundary of the line the `boundaries`
/// were built from) to its char index, via a binary search over the sorted
/// boundary offsets.
fn byte_to_char(boundaries: &[usize], byte: usize) -> usize {
    boundaries.partition_point(|&b| b < byte)
}
