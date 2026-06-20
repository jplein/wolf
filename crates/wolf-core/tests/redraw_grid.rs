//! Integration tests for the redraw -> snapshot folding: typed text reaching the
//! grid, resize round-tripping through nvim, and scroll updating the viewport.
//! Requires a real nvim via `WOLF_NVIM_BIN`.

mod common;

use common::{ch, key, FLUSH_TIMEOUT};
use wolf_core::Key;

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

#[test]
fn type_text_appears_in_grid() {
    // Open an empty markdown buffer.
    let (core, waiter, _file) = common::start_core_with_temp_md("");

    // Enter insert mode, type "Hello", leave insert mode.
    core.send_input(ch('i'));
    common::type_str(&core, "Hello");
    core.send_input(key(Key::Esc));

    // Wait for the resulting redraw to land in the snapshot.
    waiter.wait_for_flush(FLUSH_TIMEOUT);

    let snap = core.read_snapshot();

    let row0 = snap.grid.row_text(0);
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
        .grid
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
    // First line: a double-width CJK glyph followed by ASCII. A double-width
    // character occupies two grid columns: the leading cell carries the glyph's
    // text, the trailing cell is blank, and any following text keeps its column
    // alignment. This exercises the highest-risk `grid_line` parser path
    // (wide-char trailing empty cell + column alignment).
    let (core, _waiter, _file) = common::start_core_with_temp_md("\u{4e2d}ab\n");

    let snap = core.read_snapshot();

    // Col 0: the wide glyph itself.
    let glyph_cell = snap
        .grid
        .cell(0, WIDE_GLYPH_COL)
        .expect("expected a cell at the wide glyph column");
    assert_eq!(
        glyph_cell.text, "\u{4e2d}",
        "col 0 should carry the double-width glyph's text"
    );

    // Col 1: the trailing cell of the double-width char.
    // NOTE: per the plan/impl (`src/render` Cell: text = "" for trailing wide
    // cell) we assert the trailing cell's text is exactly empty (""). If the
    // implementation instead renders a single space there, the coder must
    // reconcile this assertion. We additionally assert it is neither the glyph
    // nor the following ASCII, which holds under either convention.
    let trailing_cell = snap
        .grid
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
        .grid
        .cell(0, WIDE_ASCII_A_COL)
        .expect("expected a cell for the first ASCII char");
    assert_eq!(a_cell.text, "a", "'a' should render at col 2 after the wide glyph");

    let b_cell = snap
        .grid
        .cell(0, WIDE_ASCII_B_COL)
        .expect("expected a cell for the second ASCII char");
    assert_eq!(b_cell.text, "b", "'b' should render at col 3 after the wide glyph");

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn grid_line_repeat_expansion() {
    // First line: a long run of the SAME character followed by a single distinct
    // trailing char, e.g. "========...===X". nvim's ext_linegrid is likely to
    // emit the run as ONE grid_line cell tuple `[text, hl, repeat]` with a
    // `repeat` count instead of N separate cells. This exercises the parser's
    // repeat-expansion / column-advance path: it must write the char into every
    // column of the run and advance the column cursor by `repeat` (not by 1).
    //
    // NOTE: we do not assume how nvim chunks the line. The run could arrive as a
    // single repeat-tuple OR as several tuples; either way the resulting grid
    // contents are identical, and these assertions only inspect the grid (cell
    // text + column alignment), so they hold under either chunking.
    let mut first_line = REPEAT_CHAR.to_string().repeat(REPEAT_RUN_LEN);
    first_line.push(TRAILING_CHAR);
    first_line.push('\n');

    let (core, _waiter, _file) = common::start_core_with_temp_md(&first_line);

    let snap = core.read_snapshot();

    // The grid is wide enough (DEFAULT_COLS = 80) to hold the run + trailing char.
    assert!(
        snap.grid.cols > TRAILING_CHAR_COL,
        "grid must be wide enough ({} cols) to hold the run and trailing char at col {TRAILING_CHAR_COL}",
        snap.grid.cols
    );

    // Every sampled column across the run must hold the repeated character: this
    // proves the run was faithfully expanded into individual cells.
    for &c in &SAMPLE_COLS {
        let cell = snap
            .grid
            .cell(0, c)
            .unwrap_or_else(|| panic!("expected a cell at run column {c}"));
        assert_eq!(
            cell.text,
            REPEAT_CHAR.to_string(),
            "column {c} of the run should hold the repeated char {REPEAT_CHAR:?}"
        );
    }

    // Column alignment: the distinct trailing char must land at exactly the run's
    // length. If repeat expansion advanced columns by 1 instead of by `repeat`,
    // this char would land in the wrong column (or overwrite part of the run).
    let trailing_cell = snap
        .grid
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
    let row0 = snap.grid.row_text(0);
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

    waiter.drain();
    core.resize(RESIZE_COLS, RESIZE_ROWS);
    waiter.wait_for_flush(FLUSH_TIMEOUT);

    let snap = core.read_snapshot();
    assert_eq!(
        snap.grid.cols, RESIZE_COLS as usize,
        "grid cols should match the resized width (driven by grid_resize)"
    );
    assert_eq!(
        snap.grid.rows, RESIZE_ROWS as usize,
        "grid rows should match the resized height (driven by grid_resize)"
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

#[test]
fn scroll_shifts_grid_content() {
    // Build a buffer far taller than the (short) grid so scrolling moves content.
    let mut content = String::new();
    for i in 0..SCROLL_BUFFER_LINES {
        content.push_str(&format!("line {i}\n"));
    }

    let (core, waiter, _file) =
        common::start_core_with_temp_md_sized(&content, SHORT_COLS, SHORT_ROWS);

    let before_row0 = core.read_snapshot().grid.row_text(0);
    let before_num = trailing_line_number(&before_row0)
        .expect("row 0 should start as a numbered 'line N' before scrolling");

    // Scroll down half a screen; <C-d> moves the view down, shifting grid content.
    waiter.drain();
    core.send_input(common::ctrl(Key::Char('d')));
    waiter.wait_for_flush(FLUSH_TIMEOUT);

    let after_row0 = core.read_snapshot().grid.row_text(0);
    let after_num = trailing_line_number(&after_row0)
        .expect("row 0 should still be a numbered 'line N' after scrolling");

    // The grid content itself must have moved (Grid::scroll), not just the
    // viewport number: row 0 now shows a later line in the buffer.
    assert_ne!(
        before_row0, after_row0,
        "grid row 0 text should change after scrolling down \
         (before={before_row0:?}, after={after_row0:?})"
    );
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

    let initial_topline = core.read_snapshot().viewport.topline;

    // Scroll down half a screen. Per the InputEvent API, <C-d> moves down.
    waiter.drain();
    core.send_input(common::ctrl(Key::Char('d')));
    waiter.wait_for_flush(FLUSH_TIMEOUT);

    let new_topline = core.read_snapshot().viewport.topline;
    assert!(
        new_topline > initial_topline,
        "viewport.topline should increase after scrolling down \
         (initial={initial_topline}, after={new_topline})"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn scroll_up_shifts_grid_content_down() {
    // Companion to `scroll_shifts_grid_content`. That test sends <C-d> (scroll
    // forward / view moves DOWN, which shifts grid content UP on screen), which
    // exercises one branch of `Grid::scroll` (render.rs copy_row_range low->high).
    // This test exercises the OTHER branch: scroll BACKWARD with <C-u>, which
    // moves the view UP and shifts grid content DOWN on screen (high->low copy).
    //
    // To have room to scroll back up, we first jump to the END of a tall buffer
    // so the top visible line is no longer "line 0", then scroll up and assert
    // row 0 now shows an EARLIER buffer line than before.
    let mut content = String::new();
    for i in 0..SCROLL_BUFFER_LINES {
        content.push_str(&format!("line {i}\n"));
    }

    let (core, waiter, _file) =
        common::start_core_with_temp_md_sized(&content, SHORT_COLS, SHORT_ROWS);

    // Jump to the end of the buffer so there is content above the viewport to
    // scroll back into. `G` moves to the last line.
    waiter.drain();
    core.send_input(ch('G'));
    waiter.wait_for_flush(FLUSH_TIMEOUT);

    let before_row0 = core.read_snapshot().grid.row_text(0);
    let before_num = trailing_line_number(&before_row0).expect(
        "after jumping to end, row 0 should be a numbered 'line N' that is not line 0",
    );
    assert!(
        before_num > 0,
        "after `G` on a {SCROLL_BUFFER_LINES}-line buffer, row 0 should show a \
         later line than line 0 (got line {before_num})"
    );

    // Scroll BACKWARD half a screen; <C-u> moves the view up, shifting grid
    // content DOWN on screen (the down-shift branch of Grid::scroll).
    //
    // NOTE: we do not assume how nvim chunks the grid_scroll region or how many
    // lines <C-u> moves; we only assert row 0 changed and now shows an EARLIER
    // buffer line, which holds regardless of the exact scroll amount/chunking.
    waiter.drain();
    core.send_input(common::ctrl(Key::Char('u')));
    waiter.wait_for_flush(FLUSH_TIMEOUT);

    let after_row0 = core.read_snapshot().grid.row_text(0);
    let after_num = trailing_line_number(&after_row0)
        .expect("after scrolling up, row 0 should still be a numbered 'line N'");

    assert_ne!(
        before_row0, after_row0,
        "grid row 0 text should change after scrolling up \
         (before={before_row0:?}, after={after_row0:?})"
    );
    assert!(
        after_num < before_num,
        "row 0 should show an EARLIER buffer line after scrolling up \
         (before line {before_num}, after line {after_num})"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn viewport_reports_line_count() {
    // Guard the positional index the win_viewport handler reads for line_count
    // (currently params index 6, which is version-fragile). Open a buffer with a
    // KNOWN number of lines and assert viewport.linecount matches exactly.
    //
    // NOTE: the expected linecount is exactly LINE_COUNT (50). We build the
    // content by JOINING the lines with '\n' and deliberately add NO trailing
    // newline, so nvim does not append an extra empty final line. With a trailing
    // '\n' nvim would report LINE_COUNT + 1 (51); by omitting it the count is
    // unambiguously LINE_COUNT. The coder must confirm this trailing-newline
    // behavior matches the buffer-open path.
    let content: String = (0..LINE_COUNT)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");

    let (core, _waiter, _file) =
        common::start_core_with_temp_md_sized(&content, SHORT_COLS, SHORT_ROWS);

    let viewport = core.read_snapshot().viewport;

    assert_eq!(
        viewport.linecount, LINE_COUNT as u64,
        "viewport.linecount should equal the {LINE_COUNT} lines in the buffer \
         (guards the win_viewport line_count field index); got {}",
        viewport.linecount
    );

    // Sanity invariants on the reported viewport window: the top of the visible
    // window is at or above the bottom, and the bottom does not exceed the buffer
    // line count (+1 to allow nvim's exclusive/past-the-end botline convention).
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
