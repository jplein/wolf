//! Integration tests for the redraw -> snapshot folding: typed text reaching the
//! grid, resize round-tripping through nvim, and scroll updating the viewport.
//! Requires a real nvim via `WOLF_NVIM_BIN`.

mod common;

use std::time::{Duration, Instant};

use common::{ch, key, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Key, Snapshot, WolfCore};

/// The outer/global grid id. Under ext_multigrid grid 1 is the global grid that
/// a global `core.resize(..)` drives; window content lives on grids >= 2.
const GLOBAL_GRID: u64 = 1;

/// Larger grid used by the resize test so the new dims differ from the default.
const RESIZE_COLS: u16 = 100;
const RESIZE_ROWS: u16 = 40;

/// Short grid used by the scroll test so a many-line buffer overflows it.
const SHORT_COLS: u16 = 80;
const SHORT_ROWS: u16 = 10;

/// Number of lines written into the scroll-test buffer (well over SHORT_ROWS).
const SCROLL_BUFFER_LINES: usize = 200;

/// Grid columns where the wide-char test's ASCII tail is expected to render.
/// A leading double-width glyph occupies cols 0 (glyph) and 1 (trailing blank),
/// so the following `a`/`b` land at cols 2 and 3 (alignment preserved).
const WIDE_GLYPH_COL: usize = 0;
const WIDE_TRAILING_COL: usize = 1;
const WIDE_ASCII_A_COL: usize = 2;
const WIDE_ASCII_B_COL: usize = 3;

/// Length of the repeated-character run for the `grid_line` repeat-expansion
/// test. Long enough that nvim's ext_linegrid is likely to coalesce it into a
/// single cell tuple carrying a `repeat` count rather than N separate cells.
const REPEAT_RUN_LEN: usize = 40;
/// The character repeated to form the run.
const REPEAT_CHAR: char = '=';
/// A distinct trailing character placed immediately after the run, used to
/// verify the run advanced the column cursor by `repeat` (not by 1).
const TRAILING_CHAR: char = 'X';
/// Column of the trailing char: it follows the full run, so it lands at the
/// run's length.
const TRAILING_CHAR_COL: usize = REPEAT_RUN_LEN;
/// A few columns sampled across the run to confirm every cell expanded.
const SAMPLE_COLS: [usize; 4] = [0, 1, REPEAT_RUN_LEN / 2, REPEAT_RUN_LEN - 1];

/// Exact number of lines written into the `viewport_reports_line_count` buffer.
/// Chosen as a round, unambiguous count that exercises the win_viewport
/// line_count field at a known value.
const LINE_COUNT: usize = 50;

/// How often [`poll_until`] re-reads the snapshot while waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Polls `observe` over the snapshot until `done` accepts the observed value, and returns it,
/// panicking with `desc` and the last value after [`FLUSH_TIMEOUT`]. One `wait_for_flush` can
/// return on a flush that predates the input under test, so a single read can measure that flush
/// rather than the input. Mirrors `redraw_multigrid.rs`'s `poll_until`, adding the returned value.
fn poll_until<T: std::fmt::Debug>(
    core: &WolfCore,
    waiter: &FlushWaiter,
    desc: &str,
    observe: impl Fn(&Snapshot) -> T,
    done: impl Fn(&T) -> bool,
) -> T {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        // One statement: the read guard drops before drain/sleep (see autoread.rs).
        let seen = observe(&core.read_snapshot());
        if done(&seen) {
            return seen;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for {desc}; last observed: {seen:?}");
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[test]
fn type_text_appears_in_grid() {
    // Open an empty markdown buffer.
    let (core, waiter, _file) = common::start_core_with_temp_md("");

    // Enter insert mode, type "Hello", leave insert mode.
    core.send_input(ch('i'));
    common::type_str(&core, "Hello");
    core.send_input(key(Key::Esc));

    // Poll until the typed text lands in the snapshot.
    let row0 = poll_until(
        &core,
        &waiter,
        "row 0 to contain \"Hello\"",
        |snap| snap.active_grid().row_text(0),
        |row| row.contains("Hello"),
    );
    let snap = core.read_snapshot();

    assert!(
        row0.trim_start().starts_with("Hello"),
        "row 0 should start with the typed text; got {row0:?}"
    );

    // The cell at the 'H' column should carry an hl_id that resolves to a real fg.
    // Find the first non-space column on row 0 (where 'H' rendered).
    let h_col = row0
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| i)
        .expect("row 0 should contain a non-space character");

    let cell = snap
        .active_grid()
        .cell(0, h_col)
        .expect("expected a cell at the 'H' position");
    let style = snap.highlights.resolve(cell.hl_id);
    assert!(
        style.fg.is_some(),
        "the 'H' cell's hl_id should resolve to a non-None fg (hl_id={}, style={:?})",
        cell.hl_id,
        style
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn wide_char_grid_line() {
    // A double-width CJK glyph then ASCII: the leading cell carries the glyph, the trailing cell is
    // blank, and following text keeps its column. This is the riskiest `grid_line` parser path.
    let (core, waiter, _file) = common::start_core_with_temp_md("\u{4e2d}ab\n");

    poll_until(
        &core,
        &waiter,
        "row 0 to contain the fixture's \"ab\"",
        |snap| snap.active_grid().row_text(0),
        |row| row.contains("ab"),
    );
    let snap = core.read_snapshot();

    // Col 0: the wide glyph itself.
    let glyph_cell = snap
        .active_grid()
        .cell(0, WIDE_GLYPH_COL)
        .expect("expected a cell at the wide glyph column");
    assert_eq!(
        glyph_cell.text, "\u{4e2d}",
        "col 0 should carry the double-width glyph's text"
    );

    // Col 1, the wide char's trailing cell, has text exactly "" by the `Cell` convention, and is
    // neither the glyph nor the following ASCII.
    let trailing_cell = snap
        .active_grid()
        .cell(0, WIDE_TRAILING_COL)
        .expect("expected a (blank) trailing cell after the wide glyph");
    assert_eq!(
        trailing_cell.text, "",
        "the trailing cell of a double-width char should have empty text"
    );
    assert_ne!(
        trailing_cell.text, "\u{4e2d}",
        "the trailing cell must not duplicate the wide glyph"
    );
    assert_ne!(
        trailing_cell.text, "a",
        "the trailing cell must not hold the following ASCII char"
    );

    // Cols 2 and 3: the ASCII tail, alignment preserved past the wide glyph.
    let a_cell = snap
        .active_grid()
        .cell(0, WIDE_ASCII_A_COL)
        .expect("expected a cell for the first ASCII char");
    assert_eq!(
        a_cell.text, "a",
        "'a' should render at col 2 after the wide glyph"
    );

    let b_cell = snap
        .active_grid()
        .cell(0, WIDE_ASCII_B_COL)
        .expect("expected a cell for the second ASCII char");
    assert_eq!(
        b_cell.text, "b",
        "'b' should render at col 3 after the wide glyph"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn grid_line_repeat_expansion() {
    // A long run of one char then a distinct one, which nvim may send as a `repeat` tuple: every
    // column of the run is written and the cursor advances by `repeat`, whatever the chunking.
    let mut first_line = REPEAT_CHAR.to_string().repeat(REPEAT_RUN_LEN);
    first_line.push(TRAILING_CHAR);
    first_line.push('\n');

    let (core, waiter, _file) = common::start_core_with_temp_md(&first_line);

    poll_until(
        &core,
        &waiter,
        &format!("row 0 to contain the fixture's trailing {TRAILING_CHAR:?}"),
        |snap| snap.active_grid().row_text(0),
        |row| row.contains(TRAILING_CHAR),
    );
    let snap = core.read_snapshot();

    // The grid is wide enough (DEFAULT_COLS = 80) to hold the run + trailing char.
    assert!(
        snap.active_grid().cols > TRAILING_CHAR_COL,
        "grid must be wide enough ({} cols) to hold the run and trailing char at col {TRAILING_CHAR_COL}",
        snap.active_grid().cols
    );

    // Every sampled column across the run must hold the repeated character: this
    // proves the run was faithfully expanded into individual cells.
    for &c in &SAMPLE_COLS {
        let cell = snap
            .active_grid()
            .cell(0, c)
            .unwrap_or_else(|| panic!("expected a cell at run column {c}"));
        assert_eq!(
            cell.text,
            REPEAT_CHAR.to_string(),
            "column {c} of the run should hold the repeated char {REPEAT_CHAR:?}"
        );
    }

    // The trailing char must land exactly at the run's length; advancing by 1 instead of `repeat`
    // would misplace it or overwrite the run.
    let trailing_cell = snap
        .active_grid()
        .cell(0, TRAILING_CHAR_COL)
        .unwrap_or_else(|| panic!("expected the trailing char's cell at col {TRAILING_CHAR_COL}"));
    assert_eq!(
        trailing_cell.text,
        TRAILING_CHAR.to_string(),
        "the distinct trailing char {TRAILING_CHAR:?} must land at col {TRAILING_CHAR_COL} \
         (proves the run advanced the column cursor by repeat, not by 1)"
    );

    // The row text prefix must be the full run of identical chars then the
    // trailing char, with no chars before or interleaved.
    let row0 = snap.active_grid().row_text(0);
    let expected_prefix: String = REPEAT_CHAR.to_string().repeat(REPEAT_RUN_LEN);
    assert!(
        row0.starts_with(&expected_prefix),
        "row 0 should start with the full {REPEAT_RUN_LEN}-char run; got {row0:?}"
    );
    assert_eq!(
        row0.chars().nth(TRAILING_CHAR_COL),
        Some(TRAILING_CHAR),
        "the trailing char should be the ({TRAILING_CHAR_COL}+1)th char of row 0; got {row0:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn resize_updates_grid() {
    // Start at the default size, then resize to clearly different dims.
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let global_dims = |snap: &Snapshot| snap.grid(GLOBAL_GRID).map(|g| (g.cols, g.rows));
    let before = global_dims(&core.read_snapshot());
    core.resize(RESIZE_COLS, RESIZE_ROWS);

    // `core.resize` is global and drives grid 1; the active window grid takes the content area, so
    // inspect grid 1. Per-grid resize is in redraw_multigrid.rs.
    let (cols, rows) = poll_until(
        &core,
        &waiter,
        &format!("grid {GLOBAL_GRID}'s (cols, rows) to change from {before:?} after resize"),
        global_dims,
        |dims| *dims != before,
    )
    .expect("global grid must exist after a global resize");
    assert_eq!(
        cols, RESIZE_COLS as usize,
        "global grid cols should match the resized width (driven by grid_resize)"
    );
    assert_eq!(
        rows, RESIZE_ROWS as usize,
        "global grid rows should match the resized height (driven by grid_resize)"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Parses the trailing integer from a "line N" grid row, ignoring trailing
/// blanks nvim pads the cell row with. Returns None if no integer is present.
fn trailing_line_number(row: &str) -> Option<usize> {
    row.trim()
        .rsplit(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse::<usize>().ok())
}

/// Polls until the active grid's row 0 shows a numbered "line N" of the scroll fixture, so the
/// opened file is drawn, and returns that row.
fn fixture_row0(core: &WolfCore, waiter: &FlushWaiter) -> String {
    poll_until(
        core,
        waiter,
        "row 0 to show the fixture's numbered 'line N'",
        |snap| snap.active_grid().row_text(0),
        |row| trailing_line_number(row).is_some(),
    )
}

#[test]
fn scroll_shifts_grid_content() {
    // Build a buffer far taller than the (short) grid so scrolling moves content.
    let mut content = String::new();
    for i in 0..SCROLL_BUFFER_LINES {
        content.push_str(&format!("line {i}\n"));
    }

    let (core, waiter, _file) =
        common::start_core_with_temp_md_sized(&content, SHORT_COLS, SHORT_ROWS);

    let before_row0 = fixture_row0(&core, &waiter);
    let before_num = trailing_line_number(&before_row0)
        .expect("row 0 should start as a numbered 'line N' before scrolling");

    // Scroll down half a screen; <C-d> moves the view down, shifting grid content.
    core.send_input(common::ctrl(Key::Char('d')));

    // The grid content itself must have moved (Grid::scroll), not just the
    // viewport number: row 0 now shows a later line in the buffer.
    let after_row0 = poll_until(
        &core,
        &waiter,
        &format!("grid row 0 to change from {before_row0:?} to another 'line N' after <C-d>"),
        |snap| snap.active_grid().row_text(0),
        |row| *row != before_row0 && trailing_line_number(row).is_some(),
    );
    let after_num = trailing_line_number(&after_row0)
        .expect("row 0 should still be a numbered 'line N' after scrolling");
    assert!(
        after_num > before_num,
        "row 0 should show a later buffer line after scrolling down \
         (before line {before_num}, after line {after_num})"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn scroll_emits_viewport() {
    // Build a buffer far taller than the (short) grid so scrolling is possible.
    let mut content = String::new();
    for i in 0..SCROLL_BUFFER_LINES {
        // Distinct content per line keeps things deterministic/debuggable.
        content.push_str(&format!("line {i}\n"));
    }

    let (core, waiter, _file) =
        common::start_core_with_temp_md_sized(&content, SHORT_COLS, SHORT_ROWS);

    fixture_row0(&core, &waiter);
    let initial_topline = core.read_snapshot().active_viewport().topline;

    // Scroll down half a screen. Per the InputEvent API, <C-d> moves down.
    core.send_input(common::ctrl(Key::Char('d')));

    let new_topline = poll_until(
        &core,
        &waiter,
        &format!("viewport.topline to change from {initial_topline} after <C-d>"),
        |snap| snap.active_viewport().topline,
        |topline| *topline != initial_topline,
    );
    assert!(
        new_topline > initial_topline,
        "viewport.topline should increase after scrolling down \
         (initial={initial_topline}, after={new_topline})"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn scroll_up_shifts_grid_content_down() {
    // The other `Grid::scroll` branch from `scroll_shifts_grid_content`: <C-u> shifts content down.
    // Jump to the end of a tall buffer first, so there is room to scroll back.
    let mut content = String::new();
    for i in 0..SCROLL_BUFFER_LINES {
        content.push_str(&format!("line {i}\n"));
    }

    let (core, waiter, _file) =
        common::start_core_with_temp_md_sized(&content, SHORT_COLS, SHORT_ROWS);

    let start_row0 = fixture_row0(&core, &waiter);

    // Jump to the end of the buffer so there is content above the viewport to
    // scroll back into. `G` moves to the last line.
    core.send_input(ch('G'));

    let before_row0 = poll_until(
        &core,
        &waiter,
        &format!("grid row 0 to change from {start_row0:?} to another 'line N' after G"),
        |snap| snap.active_grid().row_text(0),
        |row| *row != start_row0 && trailing_line_number(row).is_some(),
    );
    let before_num = trailing_line_number(&before_row0)
        .expect("after jumping to end, row 0 should be a numbered 'line N' that is not line 0");
    assert!(
        before_num > 0,
        "after `G` on a {SCROLL_BUFFER_LINES}-line buffer, row 0 should show a \
         later line than line 0 (got line {before_num})"
    );

    // <C-u> shifts grid content down. Assert only that row 0 now shows an earlier buffer line,
    // whatever the scroll amount or grid_scroll chunking.
    core.send_input(common::ctrl(Key::Char('u')));

    let after_row0 = poll_until(
        &core,
        &waiter,
        &format!("grid row 0 to change from {before_row0:?} to another 'line N' after <C-u>"),
        |snap| snap.active_grid().row_text(0),
        |row| *row != before_row0 && trailing_line_number(row).is_some(),
    );
    let after_num = trailing_line_number(&after_row0)
        .expect("after scrolling up, row 0 should still be a numbered 'line N'");

    assert!(
        after_num < before_num,
        "row 0 should show an EARLIER buffer line after scrolling up \
         (before line {before_num}, after line {after_num})"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn viewport_reports_line_count() {
    // Guards the version-fragile positional index (params 6) win_viewport reads for line_count. No
    // trailing newline: one would make nvim report LINE_COUNT + 1.
    let content: String = (0..LINE_COUNT)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");

    let (core, waiter, _file) =
        common::start_core_with_temp_md_sized(&content, SHORT_COLS, SHORT_ROWS);

    let viewport = poll_until(
        &core,
        &waiter,
        &format!(
            "viewport.linecount to equal the {LINE_COUNT} lines in the buffer \
             (guards the win_viewport line_count field index)"
        ),
        |snap| snap.active_viewport(),
        |viewport| viewport.linecount == LINE_COUNT as u64,
    );

    // Sanity: topline <= botline <= linecount + 1, allowing nvim's past-the-end botline.
    assert!(
        viewport.topline <= viewport.botline,
        "viewport.topline ({}) should be <= botline ({})",
        viewport.topline,
        viewport.botline
    );
    assert!(
        viewport.botline <= viewport.linecount + 1,
        "viewport.botline ({}) should not exceed linecount + 1 ({})",
        viewport.botline,
        viewport.linecount + 1
    );

    core.shutdown().expect("shutdown should be Ok");
}
