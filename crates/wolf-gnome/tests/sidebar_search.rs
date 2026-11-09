//! Headless tests for the pure seams behind the sidebar's project-search panel:
//! `wolf_gnome::search_panel` (what the panel shows, what Enter and a row activation
//! do, where an open is routed) and `wolf_gnome::sidebar::panel_visibility` (which
//! sidebar page is live).
//!
//! No display, no Neovim and no ripgrep: every input is a hand-built
//! `wolf_core::SearchMatch` or a literal, and every expected value is written out by
//! hand rather than computed by the function under test. What the widgets do with
//! these answers is the cage smokes' business, in `smoke_headless.rs`.
//!
//! The fixture paths are absolute and Unix-shaped (`/proj/...`), which is fine
//! because this crate builds only on Linux.

use std::collections::HashSet;
use std::path::PathBuf;

use wolf_core::{Liveness, SearchMatch};
use wolf_gnome::search_panel::{
    activation_route, char_budget, enter_action, first_line_row, panel_rows, row_activation,
    search_options_for, search_status, selection_after_rebuild, show_decision, EnterAction,
    OpenRoute, PanelRow, RowActivation, ShowDecision,
};
use wolf_gnome::sidebar::{panel_visibility, PanelVisibility, SidebarPage};

/// A budget no fixture line comes near, so `context_window` returns every line whole.
const WIDE_BUDGET: usize = 1000;

/// A line cap no fixture comes near, so `panel_rows` drops nothing.
const NO_CAP: usize = 1000;

/// The leading/trailing ellipsis `panel_rows` adds around a trimmed line: U+2026,
/// three bytes in UTF-8, which is the shift the bold ranges must take.
const ELLIPSIS: &str = "\u{2026}";

/// Absolute path of the fixture project's file at `rel`.
fn abs(rel: &str) -> PathBuf {
    PathBuf::from(format!("/proj/{rel}"))
}

/// One match, as `wolf_core`'s search session would report it.
fn found(rel: &str, line_number: u64, column: u32, line_text: &str) -> SearchMatch {
    let end = column + 5;
    SearchMatch {
        abs_path: abs(rel),
        rel_path: rel.to_string(),
        line_number,
        column,
        line_text: line_text.to_string(),
        match_ranges: vec![(column, end)],
    }
}

/// Five matches over three files, in an emission order that is neither the path order
/// nor the line order: `c.md` 5, `b/x.md` 7, `A.md` 3, `b/x.md` 2, `c.md` 1.
///
/// Every line is short and carries the five-byte match `zebra` at its `column`, so
/// under [`WIDE_BUDGET`] each row's text is the line and its bold range the match.
fn out_of_order() -> Vec<SearchMatch> {
    vec![
        found("c.md", 5, 0, "zebra c5"),
        found("b/x.md", 7, 3, "b7 zebra"),
        found("A.md", 3, 2, "a3zebra"),
        found("b/x.md", 2, 1, "bzebra2"),
        found("c.md", 1, 4, "c1  zebra"),
    ]
}

fn file_row(rel: &str, label: &str, expanded: bool) -> PanelRow {
    PanelRow::File {
        abs_path: abs(rel),
        label: label.to_string(),
        expanded,
    }
}

fn line_row(rel: &str, line_number: u64, column: u32, text: &str) -> PanelRow {
    PanelRow::Line {
        abs_path: abs(rel),
        line_number,
        column,
        text: text.to_string(),
        bold: vec![(column, column + 5)],
    }
}

/// The rows [`out_of_order`] projects to with nothing collapsed and nothing capped.
fn all_rows() -> Vec<PanelRow> {
    vec![
        file_row("A.md", "A.md", true),
        line_row("A.md", 3, 2, "a3zebra"),
        file_row("b/x.md", "x.md - b", true),
        line_row("b/x.md", 2, 1, "bzebra2"),
        line_row("b/x.md", 7, 3, "b7 zebra"),
        file_row("c.md", "c.md", true),
        line_row("c.md", 1, 4, "c1  zebra"),
        line_row("c.md", 5, 0, "zebra c5"),
    ]
}

fn collapsed(rels: &[&str]) -> HashSet<PathBuf> {
    rels.iter().map(|&rel| abs(rel)).collect()
}

#[test]
fn show_decision_needs_an_open_project() {
    assert_eq!(
        show_decision(true),
        ShowDecision::Show,
        "with a project open the panel must be shown; a refusal here means Ctrl+Shift+F, the \
         palette row and :WolfSearchInProject all do nothing in a normal window"
    );
    match show_decision(false) {
        ShowDecision::Refuse(reason) => assert!(
            !reason.trim().is_empty(),
            "the no-project refusal must name its reason, because the shell prints it as the only \
             feedback the user gets"
        ),
        ShowDecision::Show => panic!(
            "show_decision(false) answered Show: with no project open the search root falls back \
             to the home directory, so the panel would offer to grep the whole home tree"
        ),
    }
}

#[test]
fn search_options_are_literal_case_insensitive_over_all_files() {
    let query = " Zebra(cross ";
    let options = search_options_for(query);
    assert_eq!(
        options.query, query,
        "the query must reach rg verbatim, surrounding spaces and case included, because \
         enter_action compares the raw entry text against the query a search was started with"
    );
    assert!(
        !options.use_regex,
        "use_regex must be false: with no toggle, a regex default turns `(` into an rg error \
         with no way back"
    );
    assert!(
        !options.case_sensitive,
        "case_sensitive must be false: with no toggle and no ignorecase() read, the forgiving \
         default is the only one a user can reach"
    );
    assert!(
        options.all_files,
        "all_files must be true: core's default searches only Markdown, while the tree and the \
         file picker list every file, so a .txt match would silently never appear"
    );
}

#[test]
fn panel_rows_follow_the_projection_order() {
    let rows = panel_rows(&out_of_order(), WIDE_BUDGET, NO_CAP, &HashSet::new());
    assert_eq!(
        rows,
        all_rows(),
        "rows must come grouped by file in case-insensitive path order (A.md, b/x.md, c.md), \
         each file row labelled `<basename> - <parent>` and followed by its lines in ascending \
         line order. Rows in the input order mean panel_rows walked the matches instead of \
         wolf_core::project's groups; a missing file row means the grouping was lost"
    );
}

#[test]
fn panel_rows_copy_the_match_location_exactly() {
    let rows = panel_rows(
        &[found("deep/t.txt", 9, 14, "xxxxxxxxxxxxx zebracross")],
        WIDE_BUDGET,
        NO_CAP,
        &HashSet::new(),
    );
    let lines: Vec<(PathBuf, u64, u32)> = rows
        .iter()
        .filter_map(|row| match row {
            PanelRow::Line {
                abs_path,
                line_number,
                column,
                ..
            } => Some((abs_path.clone(), *line_number, *column)),
            PanelRow::File { .. } => None,
        })
        .collect();
    assert_eq!(
        lines,
        vec![(abs("deep/t.txt"), 9, 14)],
        "a line row must carry the match's own abs_path, 1-based line and 0-based byte column \
         unchanged; a column of 15 means a `+ 1` crept in, which would land every opened cursor \
         one byte past its match"
    );
}

#[test]
fn panel_rows_hide_the_lines_of_a_collapsed_file() {
    let rows = panel_rows(
        &out_of_order(),
        WIDE_BUDGET,
        NO_CAP,
        &collapsed(&["b/x.md"]),
    );
    assert_eq!(
        rows,
        vec![
            file_row("A.md", "A.md", true),
            line_row("A.md", 3, 2, "a3zebra"),
            file_row("b/x.md", "x.md - b", false),
            file_row("c.md", "c.md", true),
            line_row("c.md", 1, 4, "c1  zebra"),
            line_row("c.md", 5, 0, "zebra c5"),
        ],
        "a collapsed file must keep its file row, marked expanded: false, and lose only its own \
         line rows. b/x.md's lines present means `collapsed` was ignored; A.md's or c.md's lines \
         missing means the collapse leaked into other groups"
    );
}

#[test]
fn panel_rows_cap_lines_in_projection_order() {
    assert_eq!(
        panel_rows(&out_of_order(), WIDE_BUDGET, 2, &HashSet::new()),
        vec![
            file_row("A.md", "A.md", true),
            line_row("A.md", 3, 2, "a3zebra"),
            file_row("b/x.md", "x.md - b", true),
            line_row("b/x.md", 2, 1, "bzebra2"),
        ],
        "with max_lines 2 exactly the first two lines in projection order may remain, and c.md, \
         which has no line under the cap, must have no file row either. More rows mean max_lines \
         was ignored; a c.md row means an empty group was still drawn"
    );
    assert_eq!(
        panel_rows(&out_of_order(), WIDE_BUDGET, 2, &collapsed(&["A.md"])),
        vec![
            file_row("A.md", "A.md", false),
            file_row("b/x.md", "x.md - b", true),
            line_row("b/x.md", 2, 1, "bzebra2"),
        ],
        "a collapsed file's lines still count toward the cap, so collapsing A.md must not pull \
         b/x.md's line 7 in. Seeing it means the cap counted only visible lines, so collapsing a \
         file would change which matches the panel shows"
    );
    assert_eq!(
        panel_rows(&out_of_order(), WIDE_BUDGET, 2, &collapsed(&["c.md"])),
        vec![
            file_row("A.md", "A.md", true),
            line_row("A.md", 3, 2, "a3zebra"),
            file_row("b/x.md", "x.md - b", true),
            line_row("b/x.md", 2, 1, "bzebra2"),
        ],
        "a collapsed file with no line under the cap must have no file row either; a c.md row \
         here means collapse was checked before the cap"
    );
    assert_eq!(
        panel_rows(&out_of_order(), WIDE_BUDGET, 0, &HashSet::new()),
        Vec::<PanelRow>::new(),
        "max_lines 0 leaves no line under the cap, so no group may draw a file row"
    );
}

#[test]
fn panel_rows_window_long_lines_and_shift_the_bold_past_the_ellipsis() {
    let short = panel_rows(
        &[found("s.md", 1, 4, "foo zebra bar")],
        WIDE_BUDGET,
        NO_CAP,
        &HashSet::new(),
    );
    assert_eq!(
        short.get(1),
        Some(&line_row("s.md", 1, 4, "foo zebra bar")),
        "a line within the budget must be drawn whole, with no ellipsis and its ranges unshifted"
    );

    // 40 `a`s, the match, 40 `b`s: a 25-char budget keeps 10 chars of context on each side.
    let long_line = format!("{}zebra{}", "a".repeat(40), "b".repeat(40));
    let long = panel_rows(
        &[found("l.md", 2, 40, &long_line)],
        25,
        NO_CAP,
        &HashSet::new(),
    );
    let want_text = format!(
        "{ELLIPSIS}{}zebra{}{ELLIPSIS}",
        "a".repeat(10),
        "b".repeat(10)
    );
    let Some(PanelRow::Line { text, bold, .. }) = long.get(1) else {
        panic!("a single match must give a file row then a line row, got {long:?}");
    };
    assert_eq!(
        text, &want_text,
        "a long line with its match in the middle must be windowed to the budget with an \
         ellipsis at both ends; no ellipsis means the truncation flags were ignored"
    );
    assert_eq!(
        bold,
        &[(13, 18)],
        "the bold range must be the window's own (10, 15) shifted by the leading ellipsis's 3 \
         bytes; (10, 15) means the shift was skipped and the bold would land 3 bytes early"
    );
    assert_eq!(
        text.get(13..18),
        Some("zebra"),
        "the bold byte range must slice exactly the matched text out of the drawn label"
    );

    let at_start = panel_rows(
        &[found("z.md", 3, 0, &format!("zebra{}", "c".repeat(80)))],
        25,
        NO_CAP,
        &HashSet::new(),
    );
    assert_eq!(
        at_start.get(1),
        Some(&PanelRow::Line {
            abs_path: abs("z.md"),
            line_number: 3,
            column: 0,
            text: format!("zebra{}{ELLIPSIS}", "c".repeat(20)),
            bold: vec![(0, 5)],
        }),
        "a match at the line start must give no leading ellipsis and no shift, only a trailing \
         ellipsis; a shifted range here means the shift was applied unconditionally"
    );
}

#[test]
fn char_budget_is_whole_columns_times_lines_and_never_zero() {
    let cases: [(f64, f64, usize, usize, &str); 10] = [
        (250.0, 8.0, 2, 62, "31 whole columns over two lines"),
        (7.9, 8.0, 1, 1, "less than one column still counts as one"),
        (
            0.0,
            8.0,
            2,
            2,
            "a zero width still counts one column per line",
        ),
        (
            250.0,
            0.0,
            2,
            2,
            "a zero advance counts one column per line",
        ),
        (
            250.0,
            -1.0,
            2,
            2,
            "a negative advance counts one column per line",
        ),
        (
            250.0,
            f64::NAN,
            2,
            2,
            "a NaN advance counts one column per line",
        ),
        (
            250.0,
            f64::INFINITY,
            2,
            2,
            "an infinite advance counts one column per line",
        ),
        (
            f64::NAN,
            8.0,
            2,
            2,
            "a NaN width counts one column per line",
        ),
        (
            -80.0,
            8.0,
            2,
            2,
            "a negative width counts one column per line",
        ),
        (
            -80.0,
            -8.0,
            2,
            2,
            "a negative advance over a negative width still counts one column per line",
        ),
    ];
    for (width, advance, lines, want, why) in cases {
        assert_eq!(
            char_budget(width, advance, lines),
            want,
            "char_budget({width}, {advance}, {lines}): {why}. A value of a single row's count \
             means `* lines` was dropped; 0 or a huge number means the advance or quotient guard \
             was dropped and context_window would get an unusable budget; 20 on the row with \
             two negatives means the advance guard alone was dropped, since that quotient is +10"
        );
    }
}

#[test]
fn first_line_row_skips_file_rows() {
    assert_eq!(
        first_line_row(&all_rows()),
        Some(1),
        "the first line row follows A.md's file row at index 0; Some(0) would select a file row"
    );
    let only_files = panel_rows(
        &out_of_order(),
        WIDE_BUDGET,
        NO_CAP,
        &collapsed(&["A.md", "b/x.md", "c.md"]),
    );
    assert_eq!(
        first_line_row(&only_files),
        None,
        "with every file collapsed there is no line row to select"
    );
    assert_eq!(first_line_row(&[]), None, "no rows have no line row");
}

#[test]
fn selection_after_rebuild_follows_the_row_not_the_index() {
    let rows = all_rows();
    let b2 = line_row("b/x.md", 2, 1, "bzebra2");
    assert_eq!(
        selection_after_rebuild(true, Some(&b2), &rows),
        Some(1),
        "a reset (the first rebuild of a new run) must select the first line row even when a \
         previous row is given; 3 means the previous selection survived into a new search"
    );
    assert_eq!(
        selection_after_rebuild(false, None, &rows),
        Some(1),
        "with nothing selected before, the first line row is selected"
    );

    // A newly streamed file sorts above everything already shown.
    let mut grown = vec![
        file_row("0.md", "0.md", true),
        line_row("0.md", 4, 0, "zebra 04"),
    ];
    grown.extend(all_rows());
    assert_eq!(
        selection_after_rebuild(false, Some(&b2), &grown),
        Some(5),
        "the previously selected b/x.md line 2 moved from index 3 to 5 when 0.md streamed in \
         above it, and must stay selected there. 3 means the selection was kept by index; 4 \
         means it matched b/x.md's file row by path alone; 1 means the previous row was ignored"
    );

    let restyled = line_row("b/x.md", 2, 1, "…bzebra2");
    let mut rebudgeted = all_rows();
    rebudgeted[3] = restyled;
    assert_eq!(
        selection_after_rebuild(false, Some(&b2), &rebudgeted),
        Some(3),
        "a line row is the same row whatever its text and bold, which a budget change rewrites"
    );

    let b_file = file_row("b/x.md", "x.md - b", true);
    let folded = panel_rows(
        &out_of_order(),
        WIDE_BUDGET,
        NO_CAP,
        &collapsed(&["b/x.md"]),
    );
    assert_eq!(
        selection_after_rebuild(false, Some(&b_file), &folded),
        Some(2),
        "a selected file row stays selected after it is collapsed: a file row is matched on its \
         path, not on `expanded`"
    );

    let other_column = line_row("b/x.md", 7, 4, "b7 zebra");
    assert_eq!(
        selection_after_rebuild(false, Some(&other_column), &rows),
        Some(1),
        "a line row is identified by path, line AND column; b/x.md line 7 at column 4 is not in \
         the rows, so the answer falls back to the first line row"
    );

    let gone = line_row("gone.md", 1, 0, "zebra");
    assert_eq!(
        selection_after_rebuild(false, Some(&gone), &rows),
        Some(1),
        "a previous row that is no longer listed falls back to the first line row"
    );
    assert_eq!(
        selection_after_rebuild(false, Some(&gone), &[]),
        None,
        "rows with no line row give no selection, not Some(0)"
    );
}

#[test]
fn enter_action_clears_runs_or_activates() {
    let rows = all_rows();
    // (query, ran_query, selected, want, why), every case over `all_rows()`.
    let cases = [
        ("", None, None, EnterAction::Clear, "an empty query clears"),
        (
            "   ",
            Some("zebra"),
            Some(1),
            EnterAction::Clear,
            "a whitespace-only query clears, even over a finished search, and never runs \
             `rg -F -e ''`",
        ),
        (
            "zebra",
            None,
            None,
            EnterAction::RunSearch,
            "nothing run yet runs the query",
        ),
        (
            "zebra ",
            Some("zebra"),
            Some(1),
            EnterAction::RunSearch,
            "a trailing space is a different literal search, so it runs",
        ),
        (
            "Zebra",
            Some("zebra"),
            Some(1),
            EnterAction::RunSearch,
            "the comparison is raw, so a case change runs",
        ),
        (
            "zebra",
            Some("zebra"),
            Some(3),
            EnterAction::Activate(3),
            "an unchanged query activates the selected row",
        ),
        (
            "zebra",
            Some("zebra"),
            Some(2),
            EnterAction::Activate(2),
            "a selected file row is activated too, which toggles it",
        ),
        (
            "zebra",
            Some("zebra"),
            Some(99),
            EnterAction::Activate(1),
            "an out-of-range selection activates the first line row, not row 0",
        ),
        (
            "zebra",
            Some("zebra"),
            None,
            EnterAction::Activate(1),
            "no selection activates the first line row, not row 0",
        ),
    ];
    for (query, ran, selected, want, why) in cases {
        assert_eq!(
            enter_action(query, ran, selected, &rows),
            want,
            "enter_action({query:?}, {ran:?}, {selected:?}) over the fixture rows: {why}"
        );
    }

    for selected in [None, Some(0)] {
        assert_eq!(
            enter_action("zebra", Some("zebra"), selected, &[]),
            EnterAction::Nothing,
            "a search that found nothing must do nothing on Enter, selection {selected:?}; \
             Activate(0) would index an empty row list"
        );
    }

    let only_files = panel_rows(
        &out_of_order(),
        WIDE_BUDGET,
        NO_CAP,
        &collapsed(&["A.md", "b/x.md", "c.md"]),
    );
    assert_eq!(
        enter_action("zebra", Some("zebra"), None, &only_files),
        EnterAction::Nothing,
        "with every file collapsed and nothing selected there is no line to open, so Enter must \
         do nothing rather than toggle row 0"
    );
}

#[test]
fn row_activation_toggles_files_and_opens_lines_unadjusted() {
    assert_eq!(
        row_activation(&file_row("b/x.md", "x.md - b", true)),
        RowActivation::Toggle(abs("b/x.md")),
        "activating a file row must toggle that file's collapse, not open it"
    );
    assert_eq!(
        row_activation(&line_row("deep/t.txt", 9, 14, "xxxxxxxxxxxxx zebracross")),
        RowActivation::Open {
            path: abs("deep/t.txt"),
            line: 9,
            col: 14,
        },
        "activating a line row must open its file at the match's 1-based line and its 0-based \
         byte column unchanged, which is the convention nvim_win_set_cursor takes; col 15 means \
         a `+ 1` and the cursor would land one byte past the match"
    );
}

#[test]
fn activation_route_checks_liveness_before_the_preview() {
    assert_eq!(
        activation_route(Some(Liveness::Alive), None),
        OpenRoute::OpenAt,
        "a live core with an editor pane active opens the location"
    );
    assert_eq!(
        activation_route(Some(Liveness::Alive), Some(7)),
        OpenRoute::PreviewFile { win: 7 },
        "a live core with a preview pane active repoints that preview, as the architecture spec \
         routes a search result by pane mode; OpenAt here means preview_win was ignored"
    );

    let mut reasons = HashSet::new();
    for (liveness, label) in [
        (Some(Liveness::NotResponding), "NotResponding"),
        (Some(Liveness::Dead), "Dead"),
        (None, "no core"),
    ] {
        for preview_win in [None, Some(7)] {
            match activation_route(liveness, preview_win) {
                OpenRoute::Refuse(reason) => {
                    assert!(
                        !reason.trim().is_empty(),
                        "the {label} refusal must say why; the shell prints it"
                    );
                    if preview_win.is_none() {
                        reasons.insert(reason);
                    }
                }
                other => panic!(
                    "activation_route({liveness:?}, {preview_win:?}) answered {other:?}: a core \
                     that is not Alive must refuse, because the rows sit outside the crash \
                     overlay and preview_file is a blocking RPC"
                ),
            }
        }
    }
    assert_eq!(
        reasons.len(),
        3,
        "NotResponding, Dead and no core must each give a reason of its own, so the printed \
         line says which case it was; got {reasons:?}"
    );
}

#[test]
fn search_status_arms_and_precedence() {
    let idle = search_status(false, 0, 0, false, None);
    assert!(
        !idle.trim().is_empty(),
        "the idle status must say something"
    );
    for (total, shown, complete, error) in [
        (5, 3, true, Some("boom")),
        (1, 1, true, None),
        (2, 2, false, None),
    ] {
        assert_eq!(
            search_status(false, total, shown, complete, error),
            idle,
            "before anything has run the status is the idle line whatever a stale session \
             reports ({total}, {shown}, {complete}, {error:?})"
        );
    }

    let error = "WOLF_RG_BIN unset";
    let failed_complete = search_status(true, 0, 0, true, Some(error));
    assert!(
        failed_complete.contains(error),
        "a complete session carrying an error must show the error, got {failed_complete:?}; a \
         no-matches line here sends the user after a better query instead of a missing ripgrep"
    );
    let failed_running = search_status(true, 3, 3, false, Some(error));
    assert!(
        failed_running.contains(error),
        "the error arm precedes the running arm, so an error shows even before completion, got \
         {failed_running:?}"
    );

    let running = search_status(true, 7, 7, false, None);
    assert!(
        running.contains("7 matches"),
        "a running search shows its count so far, got {running:?}"
    );
    assert_ne!(
        running,
        search_status(true, 7, 7, true, None),
        "a running search and a finished one with the same count must read differently"
    );

    let none_found = search_status(true, 0, 0, true, None);
    assert!(
        !none_found.trim().is_empty(),
        "a finished empty search must say so"
    );
    assert_ne!(
        none_found, idle,
        "a finished search with no matches must not read as a search never run"
    );

    let capped = search_status(true, 250, 200, true, None);
    assert!(
        capped.contains("200") && capped.contains("250 matches"),
        "a capped result must name both the shown count and the total, got {capped:?}"
    );

    let one = search_status(true, 1, 1, true, None);
    assert!(
        one.contains("1 match") && !one.contains("1 matches"),
        "one result is spelled `1 match`, got {one:?}"
    );
    let three = search_status(true, 3, 3, true, None);
    assert!(
        three.contains("3 matches"),
        "several results are spelled `N matches`, got {three:?}"
    );
}

#[test]
fn panel_visibility_is_live_only_on_the_shown_current_page() {
    let cases = [
        (true, SidebarPage::Files, true, false),
        (true, SidebarPage::Search, false, true),
        (false, SidebarPage::Files, false, false),
        (false, SidebarPage::Search, false, false),
    ];
    for (shown, page, files, search) in cases {
        assert_eq!(
            panel_visibility(shown, page),
            PanelVisibility { files, search },
            "panel_visibility({shown}, {page:?}): a page is live only when the sidebar is shown \
             AND it is the current page. A live page under a hidden sidebar means sidebar_shown \
             was ignored; both pages live means the page was ignored, and the hidden one keeps \
             rebuilding its rows"
        );
    }
}
