//! Headless, standalone coverage for the PUBLIC search-results projection +
//! display surface re-exported from `wolf_core::search_tree`:
//!
//!   * `file_label(rel_path) -> String`
//!   * `project(&[SearchMatch]) -> Vec<SearchFileGroup>`
//!   * `context_window(line, ranges, budget) -> ContextWindow`
//!
//! These functions are PURE (no nvim, no rg, no filesystem), so this test needs
//! no staged runfiles and no `no-sandbox` tag — the pure analog of
//! `pane_status_unit` / `palette_rank`. The PRIVATE rg-JSON parse functions
//! (`parse_rg_line` / `match_from_event`) are covered by the coder's inline
//! `#[cfg(test)]` module inside `src/search.rs`; this file only exercises the
//! public re-exported pure fns.
//!
//! NOTE to coder (field/ctor shape): the tests construct `SearchMatch` values
//! directly via a struct literal, so `SearchMatch`'s fields
//! (`abs_path`/`rel_path`/`line_number`/`column`/`line_text`/`match_ranges`)
//! must be PUBLIC per the plan (§2 `search.rs`). Likewise the tests read
//! `SearchFileGroup { abs_path, rel_path, label, match_indices }` and
//! `ContextWindow { text, ranges, truncated_left, truncated_right }` as public
//! fields. If the coder exposes these behind accessors instead, align the tests
//! with the plan's stated field shape.

use std::path::PathBuf;

use wolf_core::{context_window, file_label, project, ContextWindow, SearchFileGroup, SearchMatch};

/// A generous character budget under which even the longest test lines fit whole,
/// used to exercise the "no truncation" branch of `context_window`.
const WIDE_BUDGET: usize = 40;

/// Builds a `SearchMatch` with the fields `project` actually keys on
/// (`abs_path`, `rel_path`, `line_number`); the display fields are filled with
/// inert placeholders since projection ignores them.
fn mk_match(abs: &str, rel: &str, line: u64) -> SearchMatch {
    SearchMatch {
        abs_path: PathBuf::from(abs),
        rel_path: rel.to_string(),
        line_number: line,
        column: 0,
        line_text: String::new(),
        match_ranges: Vec::new(),
    }
}

// --- file_label ------------------------------------------------------------

#[test]
fn file_label_cases() {
    // (rel_path, expected label)
    let cases: &[(&str, &str)] = &[
        // A root file has no parent dir, so no " - <parent>" suffix.
        ("baz.md", "baz.md"),
        // A nested file: "<basename> - <relative parent dir>".
        ("foo/bar/baz.md", "baz.md - foo/bar"),
        // A single parent segment.
        ("foo/baz.md", "baz.md - foo"),
    ];
    for (rel, expected) in cases {
        assert_eq!(
            file_label(rel),
            *expected,
            "file_label({rel:?}) should be {expected:?}"
        );
    }
}

// --- project ---------------------------------------------------------------

/// `project` groups by `abs_path`, sorts groups by `rel_path`
/// CASE-INSENSITIVELY, and sorts each group's `match_indices` by line number.
///
/// The rel paths are chosen so the case-insensitive order DIFFERS from the raw
/// byte order: raw ASCII sorts `"Foo.md"` (`F` = 0x46) before `"bar.md"`
/// (`b` = 0x62), but case-insensitively `"bar.md"` precedes `"foo.md"`. Asserting
/// `bar.md` comes first proves the sort folds case.
#[test]
fn project_groups_and_sorts_case_insensitively() {
    // Emission order deliberately interleaves files and lines out of order so the
    // sort-on-read is actually exercised (rg emits per-file grouped ascending, but
    // NOT cross-file rel-path ordered).
    let matches = vec![
        mk_match("/r/Foo.md", "Foo.md", 3),         // 0
        mk_match("/r/bar.md", "bar.md", 10),        // 1
        mk_match("/r/Foo.md", "Foo.md", 1),         // 2
        mk_match("/r/bar.md", "bar.md", 2),         // 3
        mk_match("/r/sub/qux.md", "sub/qux.md", 7), // 4
    ];

    let groups: Vec<SearchFileGroup> = project(&matches);

    // Three distinct files -> three groups.
    assert_eq!(groups.len(), 3, "one group per distinct abs_path");

    // Case-insensitive rel-path order: bar.md < Foo.md < sub/qux.md.
    let order: Vec<&str> = groups.iter().map(|g| g.rel_path.as_str()).collect();
    assert_eq!(
        order,
        vec!["bar.md", "Foo.md", "sub/qux.md"],
        "groups sorted case-insensitively by rel_path (raw ASCII would put Foo.md first)"
    );

    // Labels come from `file_label`.
    let labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["bar.md", "Foo.md", "qux.md - sub"],
        "group labels are file_label(rel_path)"
    );

    // Each group's match_indices point back into `matches`, ordered by line number.
    assert_eq!(
        groups[0].match_indices,
        vec![3, 1],
        "bar.md: line 2 (idx 3) before line 10 (idx 1)"
    );
    assert_eq!(
        groups[1].match_indices,
        vec![2, 0],
        "Foo.md: line 1 (idx 2) before line 3 (idx 0)"
    );
    assert_eq!(groups[2].match_indices, vec![4], "sub/qux.md: single match");

    // abs_path is preserved on the group.
    assert_eq!(groups[0].abs_path, PathBuf::from("/r/bar.md"));

    // The line numbers reached via match_indices are strictly ascending per group.
    for g in &groups {
        let lines: Vec<u64> = g
            .match_indices
            .iter()
            .map(|&i| matches[i].line_number)
            .collect();
        let mut sorted = lines.clone();
        sorted.sort_unstable();
        assert_eq!(lines, sorted, "match_indices ordered by line number");
    }
}

#[test]
fn project_empty_input_yields_no_groups() {
    let groups = project(&[]);
    assert!(groups.is_empty(), "no matches -> no groups");
}

// --- context_window --------------------------------------------------------

/// A line whose char count is <= budget is returned WHOLE, ranges untouched, and
/// neither side is truncated.
#[test]
fn context_window_short_line_is_untouched() {
    let line = "hello world"; // 11 chars
    let ranges = [(0u32, 5u32)]; // bold "hello"
    let cw: ContextWindow = context_window(line, &ranges, WIDE_BUDGET);

    assert_eq!(cw.text, line, "short line returned whole");
    assert_eq!(cw.ranges, vec![(0, 5)], "ranges unchanged for a whole line");
    assert!(!cw.truncated_left, "no left truncation for a whole line");
    assert!(!cw.truncated_right, "no right truncation for a whole line");
}

/// A line whose char count EXACTLY equals the budget is still returned whole with
/// no truncation (the boundary case).
#[test]
fn context_window_exact_fit_boundary() {
    let line = "0123456789"; // 10 chars
    let budget = 10;
    let cw = context_window(line, &[(2, 4)], budget);

    assert_eq!(cw.text, line, "exact-fit line returned whole");
    assert_eq!(cw.ranges, vec![(2, 4)], "ranges unchanged at exact fit");
    assert!(!cw.truncated_left);
    assert!(!cw.truncated_right);
}

/// A match at the very START of a long line: the left side clamps to 0 (no left
/// ellipsis) and the leftover budget is spent on the right, which truncates.
#[test]
fn context_window_match_at_start_truncates_right_only() {
    let line = "0123456789ABCDEFGHIJ"; // 20 chars
    let budget = 10;
    let cw = context_window(line, &[(0, 3)], budget); // bold "012"

    assert_eq!(
        cw.text, "0123456789",
        "window fills the budget from the start"
    );
    assert_eq!(cw.ranges, vec![(0, 3)], "start match stays at [0,3)");
    assert!(
        !cw.truncated_left,
        "window starts at line start -> no left ellipsis"
    );
    assert!(
        cw.truncated_right,
        "window ends before line end -> right ellipsis"
    );
}

/// A match at the very END of a long line: the right side clamps to the end (no
/// right ellipsis) and the leftover budget is spent on the left, which truncates.
#[test]
fn context_window_match_at_end_truncates_left_only() {
    let line = "0123456789ABCDEFGHIJ"; // 20 chars
    let budget = 10;
    let cw = context_window(line, &[(17, 20)], budget); // bold "HIJ"

    assert_eq!(
        cw.text, "ABCDEFGHIJ",
        "window fills the budget up to the end"
    );
    assert_eq!(
        cw.ranges,
        vec![(7, 10)],
        "end match re-based to the window: [17,20) - 10 = [7,10)"
    );
    assert!(
        cw.truncated_left,
        "window starts after line start -> left ellipsis"
    );
    assert!(
        !cw.truncated_right,
        "window ends at line end -> no right ellipsis"
    );
}

/// A match in the MIDDLE of a long line: balanced context on both sides, both
/// sides truncate.
#[test]
fn context_window_match_in_middle_is_balanced_both_truncated() {
    let line = "0123456789ABCDEFGHIJabcdefghij"; // 30 chars
    let budget = 10;
    let cw = context_window(line, &[(14, 16)], budget); // bold "EF"

    // remaining = 10 - 2 = 8, left = 4, right = 4 -> window [10,20).
    assert_eq!(
        cw.text, "ABCDEFGHIJ",
        "balanced window centered on the match"
    );
    assert_eq!(
        cw.ranges,
        vec![(4, 6)],
        "middle match re-based to the window: [14,16) - 10 = [4,6)"
    );
    assert!(cw.truncated_left, "both sides have hidden context");
    assert!(cw.truncated_right, "both sides have hidden context");
}

/// A multibyte line that fits within budget passes through untouched (byte ranges
/// preserved, no scalar split possible since the whole line is returned).
#[test]
fn context_window_multibyte_within_budget_is_untouched() {
    let line = "café résumé"; // 11 chars, 14 bytes (three 2-byte é)
    let ranges = [(0u32, 5u32)]; // bold "café" (c,a,f = 3 bytes + é = 2 bytes)
    let cw = context_window(line, &ranges, WIDE_BUDGET);

    assert_eq!(cw.text, line, "multibyte line within budget returned whole");
    assert_eq!(cw.ranges, vec![(0, 5)], "byte ranges preserved");
    assert!(!cw.truncated_left);
    assert!(!cw.truncated_right);
}

/// A multibyte/emoji line that must truncate: the window is sliced on CHAR
/// boundaries (never splitting a 4-byte scalar) and the bold byte range is
/// re-based to the window's byte offsets.
///
/// Layout: five "😀" (4 bytes / 1 char each) + "MATCH" (5 bytes / 5 chars) + five
/// "😀". 15 chars total, 45 bytes total. The literal "MATCH" occupies byte range
/// [20,25). With budget 9, the balanced window is chars [3,12) — two leading
/// emoji, "MATCH", two trailing emoji — i.e. text "😀😀MATCH😀😀", whose bytes are
/// [8 bytes of emoji] then "MATCH" at byte [8,13).
#[test]
fn context_window_emoji_line_slices_on_scalar_boundaries() {
    let line = "😀😀😀😀😀MATCH😀😀😀😀😀";
    // Sanity-check the fixture's byte geometry so the expectations below are
    // grounded in the actual encoding rather than hand-counted magic numbers.
    let match_start = line.find("MATCH").expect("fixture contains MATCH");
    let match_end = match_start + "MATCH".len();
    assert_eq!(match_start, 20, "five 4-byte emoji precede MATCH");
    assert_eq!(match_end, 25, "MATCH is five ASCII bytes");

    let budget = 9;
    let cw = context_window(line, &[(match_start as u32, match_end as u32)], budget);

    let expected_text = "😀😀MATCH😀😀";
    assert_eq!(
        cw.text, expected_text,
        "window sliced on char boundaries, never splitting an emoji scalar"
    );

    // "MATCH" sits after two leading 4-byte emoji (8 bytes) in the window text.
    let want_start = expected_text.find("MATCH").expect("window contains MATCH") as u32;
    let want_end = want_start + "MATCH".len() as u32;
    assert_eq!(
        cw.ranges,
        vec![(want_start, want_end)],
        "bold byte range re-based to the window ({want_start},{want_end})"
    );
    assert!(cw.truncated_left, "leading emoji were dropped");
    assert!(cw.truncated_right, "trailing emoji were dropped");

    // Whatever the window is, `text` must be valid UTF-8 that round-trips (it is,
    // by virtue of being a &str), and the ranges must land on char boundaries.
    for &(s, e) in &cw.ranges {
        assert!(
            cw.text.is_char_boundary(s as usize),
            "range start must be a char boundary"
        );
        assert!(
            cw.text.is_char_boundary(e as usize),
            "range end must be a char boundary"
        );
    }
}
