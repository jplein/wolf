//! Headless tests of what `wolf-vt` reads out of libghostty-vt's render state for terminal
//! sessions: cell styles and widths, per-row dirty state, the cursor's style and blink,
//! colours set and read back, the selection and its text, the scrollback viewport and its
//! scrollbar, and the DEC modes core routes input by.
//!
//! No Neovim, no pty and no runfiles: `//crates/wolf-vt:render` links the static archive and
//! nothing else, and `cargo test -p wolf-vt` runs the same file on Linux.
//!
//! ## Premises about libghostty-vt
//!
//! Each was read from the pinned 1.0.0 headers (`render.h`, `screen.h`, `sgr.h`,
//! `terminal.h`) and is measured here against the real library, not assumed:
//!
//! - SGR `4:2`, `4:3`, `4:4` and `4:5` select the double, curly, dotted and dashed underline
//!   styles (`GHOSTTY_SGR_UNDERLINE_*`), and plain SGR 4 the single one.
//! - A wide character takes a `WIDE` cell holding the grapheme and a `SPACER_TAIL` cell after
//!   it; with wraparound on, a wide character that does not fit in the last column leaves a
//!   `SPACER_HEAD` there and wraps to the next row.
//! - The render state keeps a dirty flag for the whole state and one per row, which the
//!   reader clears; a resize leaves it fully dirty.
//! - `CSI Ps SP q` with `Ps` 1 to 6 selects blinking block, steady block, blinking underline,
//!   steady underline, blinking bar and steady bar.
//! - The render state's cursor has no viewport position while the viewport is scrolled away
//!   from the active area (`CURSOR_VIEWPORT_HAS_VALUE` false).
//! - Cells keep palette indices, which the render state resolves through the palette at
//!   update time, so a palette change recolours text already on screen.
//! - The default scrollback limit (10,000 bytes, kept in whole pages) holds the 30 short lines
//!   the viewport test writes.

use wolf_vt::{
    Cell, CellWidth, CursorStyle, Dirty, Rgb, Screen, ScrollViewport, Terminal, Underline,
};

/// Makes a terminal, failing with the binding's error code.
fn terminal(cols: u16, rows: u16) -> Terminal {
    Terminal::new(cols, rows).unwrap_or_else(|error| {
        panic!(
            "Terminal::new({cols}, {rows}) failed with libghostty-vt result code {}.",
            error.code()
        )
    })
}

/// Takes the screen, failing with the binding's error code.
fn screen_of(terminal: &mut Terminal) -> Screen {
    terminal.screen().unwrap_or_else(|error| {
        panic!(
            "Terminal::screen() failed with libghostty-vt result code {}, so nothing can be read \
             back from the render state.",
            error.code()
        )
    })
}

/// Looks a cell up, failing with what the screen holds when it is out of range.
fn cell_at(screen: &Screen, col: u16, row: u16) -> &Cell {
    screen.cell(col, row).unwrap_or_else(|| {
        panic!(
            "Screen::cell({col}, {row}) is None on a {}x{} screen.",
            screen.cols, screen.rows
        )
    })
}

/// A cell's style as one comparable value: bold, faint, italic, strikethrough, inverse,
/// invisible, blink, underline.
type Style = (bool, bool, bool, bool, bool, bool, bool, Underline);

/// The style of `cell` as a [`Style`].
fn style_of(cell: &Cell) -> Style {
    (
        cell.bold,
        cell.faint,
        cell.italic,
        cell.strikethrough,
        cell.inverse,
        cell.invisible,
        cell.blink,
        cell.underline,
    )
}

/// The style of a cell with no attribute set.
const PLAIN: Style = (
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    Underline::None,
);

/// Feeds `lines` numbered lines, `line 0` to `line {lines - 1}`, the last without a newline.
fn feed_numbered_lines(term: &mut Terminal, lines: usize) {
    for n in 0..lines {
        if n > 0 {
            term.feed(b"\r\n");
        }
        term.feed(format!("line {n}").as_bytes());
    }
}

/// Each SGR attribute sets its own flag or underline style on the cell written after it and
/// nothing else, and a cell after SGR 0 has none.
#[test]
fn each_sgr_attribute_reaches_its_cell_flag() {
    // One cell per attribute, each followed by SGR 0, then a plain `Z`.
    let cases: [(&str, char, Style); 12] = [
        (
            "1",
            'A',
            (
                true,
                false,
                false,
                false,
                false,
                false,
                false,
                Underline::None,
            ),
        ),
        (
            "2",
            'B',
            (
                false,
                true,
                false,
                false,
                false,
                false,
                false,
                Underline::None,
            ),
        ),
        (
            "3",
            'C',
            (
                false,
                false,
                true,
                false,
                false,
                false,
                false,
                Underline::None,
            ),
        ),
        (
            "4",
            'D',
            (
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                Underline::Single,
            ),
        ),
        (
            "4:2",
            'E',
            (
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                Underline::Double,
            ),
        ),
        (
            "4:3",
            'F',
            (
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                Underline::Curly,
            ),
        ),
        (
            "4:4",
            'G',
            (
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                Underline::Dotted,
            ),
        ),
        (
            "4:5",
            'H',
            (
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                Underline::Dashed,
            ),
        ),
        (
            "9",
            'I',
            (
                false,
                false,
                false,
                true,
                false,
                false,
                false,
                Underline::None,
            ),
        ),
        (
            "7",
            'J',
            (
                false,
                false,
                false,
                false,
                true,
                false,
                false,
                Underline::None,
            ),
        ),
        (
            "8",
            'K',
            (
                false,
                false,
                false,
                false,
                false,
                true,
                false,
                Underline::None,
            ),
        ),
        (
            "5",
            'L',
            (
                false,
                false,
                false,
                false,
                false,
                false,
                true,
                Underline::None,
            ),
        ),
    ];
    let mut term = terminal(40, 3);
    let mut bytes = Vec::new();
    for (sgr, ch, _) in &cases {
        bytes.extend_from_slice(format!("\x1b[{sgr}m{ch}\x1b[0m").as_bytes());
    }
    bytes.push(b'Z');
    term.feed(&bytes);

    let screen = screen_of(&mut term);
    for (col, (sgr, ch, want)) in cases.iter().enumerate() {
        let cell = cell_at(&screen, col as u16, 0);
        assert_eq!(
            style_of(cell),
            *want,
            "the cell `{ch}` written after SGR {sgr} has the style (bold, faint, italic, \
             strikethrough, inverse, invisible, blink, underline) {:?}, not {want:?}. All false \
             and Underline::None means the binding reads no style at all; another flag set means \
             two style bits are crossed; another underline means the underline styles are \
             mapped in the wrong order.",
            style_of(cell)
        );
    }
    let plain = cell_at(&screen, cases.len() as u16, 0);
    assert_eq!(
        plain.text,
        "Z",
        "the plain cell after the styled ones must hold `Z`, not {:?}; row 0 reads {:?}",
        plain.text,
        screen.row_text(0)
    );
    assert_eq!(
        style_of(plain),
        PLAIN,
        "`Z` after SGR 0 must have no style, not {:?}: a flag set here means it is reported for \
         every cell, or SGR 0 did not reset it.",
        style_of(plain)
    );
    assert_eq!(
        plain.width,
        CellWidth::Narrow,
        "`Z` is a narrow character, so its width must be Narrow, not {:?}.",
        plain.width
    );
}

/// A wide character takes a `Wide` cell and a `SpacerTail` cell, a narrow one a `Narrow`
/// cell, and a wide character that does not fit in the last column leaves a `SpacerHead`.
#[test]
fn wide_characters_take_a_wide_cell_and_a_spacer() {
    let mut term = terminal(5, 3);
    term.feed("日a\r\nabcd日".as_bytes());
    let screen = screen_of(&mut term);

    let wide = cell_at(&screen, 0, 0);
    assert_eq!(
        (wide.text.as_str(), wide.width),
        ("日", CellWidth::Wide),
        "cell (0, 0) after `日` must hold `日` as a Wide cell, not {:?} {:?}. Narrow means the \
         width is not read; `日` in cell (1, 0) too means the spacer is reported as a copy of \
         the lead.",
        wide.text,
        wide.width
    );
    let tail = cell_at(&screen, 1, 0);
    assert_eq!(
        (tail.text.as_str(), tail.width),
        ("", CellWidth::SpacerTail),
        "cell (1, 0), the second half of `日`, must be an empty SpacerTail, not {:?} {:?}. `a` \
         here means the wide character took one cell.",
        tail.text,
        tail.width
    );
    let narrow = cell_at(&screen, 2, 0);
    assert_eq!(
        (narrow.text.as_str(), narrow.width),
        ("a", CellWidth::Narrow),
        "cell (2, 0) must hold `a` as a Narrow cell, not {:?} {:?}. Wide here means every cell \
         is reported wide.",
        narrow.text,
        narrow.width
    );

    let head = cell_at(&screen, 4, 1);
    assert_eq!(
        head.width,
        CellWidth::SpacerHead,
        "`日` written at the last column of a 5-column row wraps and leaves a SpacerHead in \
         column 4 of row 1, not {:?} holding {:?}; row 2 reads {:?}.",
        head.width,
        head.text,
        screen.row_text(2)
    );
    assert_eq!(
        cell_at(&screen, 0, 2).width,
        CellWidth::Wide,
        "the wrapped `日` must start row 2 as a Wide cell; row 2 reads {:?}.",
        screen.row_text(2)
    );
}

/// `screen()` reports what changed since the previous `screen()`: one written row is
/// `Partial` with only that row dirty, no change is `Clean`, and a resize is `Full`.
#[test]
fn screen_reports_and_clears_per_row_dirty_state() {
    let mut term = terminal(10, 5);
    // Park the cursor on row 4, so the write below moves it only between rows 4 and 2.
    term.feed(b"top\x1b[5;1H");
    let _ = screen_of(&mut term);

    term.feed(b"\x1b[3;1Hxyz\x1b[5;1H");
    let screen = screen_of(&mut term);
    assert_eq!(
        screen.dirty,
        Dirty::Partial,
        "after writing only row 2, the screen must be Partial, not {:?}. Full means the dirty \
         state is never cleared by the previous `screen()` or Partial is mapped to Full; Clean \
         means it is cleared before it is read.",
        screen.dirty
    );
    assert!(
        screen.row_dirty(2),
        "row 2 was written, so row_dirty(2) must be true; false means per-row dirty flags are \
         not read."
    );
    assert!(
        !screen.row_dirty(0),
        "row 0 was neither written nor crossed by the cursor since the previous `screen()`, so \
         row_dirty(0) must be false; true means every row is reported dirty or the previous read \
         did not clear the row flags."
    );

    let screen = screen_of(&mut term);
    assert_eq!(
        screen.dirty,
        Dirty::Clean,
        "a second `screen()` with nothing fed in between must be Clean, not {:?}: the first read \
         must clear the dirty state it reported.",
        screen.dirty
    );
    assert!(
        !screen.row_dirty(2),
        "row 2 was reported dirty by the previous read and not written since, so row_dirty(2) \
         must be false now."
    );

    term.resize(12, 6, 8, 16).unwrap_or_else(|error| {
        panic!(
            "resize(12, 6, 8, 16) failed with result code {}.",
            error.code()
        )
    });
    let screen = screen_of(&mut term);
    assert_eq!(
        screen.dirty,
        Dirty::Full,
        "after a resize the screen must be Full, not {:?}; Partial means Full is mapped to \
         Partial, which leaves a front-end repainting only some rows of a reflowed grid.",
        screen.dirty
    );
}

/// `CSI Ps SP q` sets the cursor's style and blink, which the screen's cursor reports.
#[test]
fn cursor_style_and_blink_follow_decscusr() {
    let cases = [
        (1, CursorStyle::Block, true),
        (2, CursorStyle::Block, false),
        (3, CursorStyle::Underline, true),
        (4, CursorStyle::Underline, false),
        (5, CursorStyle::Bar, true),
        (6, CursorStyle::Bar, false),
    ];
    let mut term = terminal(10, 3);
    for (ps, style, blinking) in cases {
        term.feed(format!("\x1b[{ps} q").as_bytes());
        let screen = screen_of(&mut term);
        let cursor = screen.cursor.unwrap_or_else(|| {
            panic!(
                "the cursor of a terminal that was never scrolled must be in the viewport, but \
                 screen.cursor is None after CSI {ps} SP q."
            )
        });
        assert_eq!(
            (cursor.style, cursor.blinking),
            (style, blinking),
            "after CSI {ps} SP q the cursor must be {style:?} with blinking {blinking}, not \
             {:?} with blinking {}. The same style for every Ps means the style is not read; \
             the right style with the wrong blink means the blink flag is not read.",
            cursor.style,
            cursor.blinking
        );
    }
}

/// Scrolling the viewport up past the cursor leaves the screen with no cursor, and
/// scrolling back to the bottom gives it back at its place.
#[test]
fn the_cursor_is_absent_while_scrolled_out_of_view() {
    let mut term = terminal(10, 3);
    feed_numbered_lines(&mut term, 10);
    let before = screen_of(&mut term).cursor.unwrap_or_else(|| {
        panic!("the cursor must be in the viewport before any scroll, but screen.cursor is None.")
    });

    term.scroll_viewport(ScrollViewport::Delta(-5));
    let scrolled = screen_of(&mut term);
    assert!(
        scrolled.cursor.is_none(),
        "with the viewport scrolled 5 rows up on a 3-row screen the cursor is out of view, so \
         screen.cursor must be None, not {:?}. Some means the viewport-has-value flag is ignored \
         and the cursor would be painted over scrollback.",
        scrolled.cursor
    );

    term.scroll_viewport(ScrollViewport::Bottom);
    let back = screen_of(&mut term).cursor.unwrap_or_else(|| {
        panic!(
            "after scrolling back to the bottom the cursor is in view again, but screen.cursor is \
             None: the cursor is never reported, or the viewport did not return."
        )
    });
    assert_eq!(
        (back.col, back.row),
        (before.col, before.row),
        "the cursor must be back at ({}, {}) after scrolling to the bottom, not ({}, {}).",
        before.col,
        before.row,
        back.col,
        back.row
    );
}

/// The cursor colour and the palette set through the binding read back, reach the screen
/// and the cells, and `None` restores the defaults; the screen carries the default colours.
#[test]
fn colours_set_through_the_binding_read_back() {
    let foreground = Rgb { r: 1, g: 2, b: 3 };
    let background = Rgb { r: 4, g: 5, b: 6 };
    let cursor = Rgb {
        r: 0x0a,
        g: 0x0b,
        b: 0x0c,
    };
    let red = Rgb {
        r: 0x12,
        g: 0x34,
        b: 0x56,
    };
    let mut term = terminal(10, 3);
    term.set_default_colors(foreground, background)
        .unwrap_or_else(|error| panic!("set_default_colors failed with code {}.", error.code()));
    let screen = screen_of(&mut term);
    assert_eq!(
        (screen.foreground, screen.background),
        (foreground, background),
        "the screen's default colours must be the ones set, {foreground:?} and {background:?}, \
         not {:?} and {:?}.",
        screen.foreground,
        screen.background
    );

    term.set_cursor_color(Some(cursor));
    assert_eq!(
        term.cursor_color(),
        Some(cursor),
        "cursor_color() must read back the colour set, {cursor:?}, not {:?}; None means \
         set_cursor_color never reached the terminal or cursor_color() always reports None.",
        term.cursor_color()
    );
    assert_eq!(
        screen_of(&mut term).cursor_color,
        Some(cursor),
        "the screen's cursor_color must be the colour set, {cursor:?}."
    );
    term.set_cursor_color(None);
    assert_eq!(
        (term.cursor_color(), screen_of(&mut term).cursor_color),
        (None, None),
        "set_cursor_color(None) must clear the cursor colour from both cursor_color() and the \
         screen."
    );

    let builtin = term.palette();
    assert_ne!(
        builtin[1], red,
        "precondition: the built-in palette entry 1 must differ from the test colour {red:?}."
    );
    term.feed(b"\x1b[31mR\x1b[0m");
    let mut palette = builtin;
    palette[1] = red;
    term.set_palette(Some(&palette));
    let screen = screen_of(&mut term);
    assert_eq!(
        cell_at(&screen, 0, 0).fg,
        Some(red),
        "an SGR 31 cell must take palette entry 1, set to {red:?}, as its foreground, not {:?}. \
         The built-in {:?} means set_palette did not reach the terminal or the cell's colour was \
         resolved when it was written.",
        cell_at(&screen, 0, 0).fg,
        builtin[1]
    );

    term.set_palette(None);
    assert_eq!(
        term.palette()[1],
        builtin[1],
        "set_palette(None) must restore the built-in entry 1, {:?}, not {:?}.",
        builtin[1],
        term.palette()[1]
    );
    let screen = screen_of(&mut term);
    assert_eq!(
        cell_at(&screen, 0, 0).fg,
        Some(builtin[1]),
        "after set_palette(None) the SGR 31 cell must be the built-in entry 1 again, {:?}, not \
         {:?}.",
        builtin[1],
        cell_at(&screen, 0, 0).fg
    );
}

/// A selection made through the binding gives its text and its columns on its row only, and
/// clearing it empties both.
#[test]
fn a_selection_gives_its_text_and_its_columns() {
    let mut term = terminal(20, 3);
    term.feed(b"hello world");
    term.select((6, 0), (10, 0))
        .unwrap_or_else(|error| panic!("select failed with code {}.", error.code()));

    assert_eq!(
        term.selection_text().as_deref(),
        Some("world"),
        "selecting columns 6 to 10 of `hello world` must give the text \"world\". None means \
         select did not reach the terminal or selection_text reads nothing; \"hello\" or an \
         off-by-one text means the coordinates are swapped or not inclusive."
    );
    let screen = screen_of(&mut term);
    assert_eq!(
        screen.row_selection(0),
        Some((6, 10)),
        "row 0's selection must be columns 6 to 10 inclusive, not {:?}.",
        screen.row_selection(0)
    );
    assert_eq!(
        screen.row_selection(1),
        None,
        "row 1 holds no part of the selection, so its selection must be None, not {:?}.",
        screen.row_selection(1)
    );

    term.clear_selection()
        .unwrap_or_else(|error| panic!("clear_selection failed with code {}.", error.code()));
    let text = term.selection_text();
    assert!(
        text.as_deref().unwrap_or("").is_empty(),
        "after clear_selection the selection text must be empty, not {text:?}: \"world\" means \
         clear_selection did nothing."
    );
    assert_eq!(
        screen_of(&mut term).row_selection(0),
        None,
        "after clear_selection row 0 must have no selection."
    );
}

/// The viewport scrolls through scrollback and the scrollbar follows it: at the bottom the
/// offset is `total - len`, a scroll up lowers it and shows earlier lines, and `Top` and
/// `Bottom` reach both ends.
#[test]
fn the_viewport_scrolls_through_scrollback() {
    let mut term = terminal(20, 10);
    feed_numbered_lines(&mut term, 30);

    let bar = term.scrollbar();
    assert_eq!(
        bar.len, 10,
        "the scrollbar's len must be the 10 visible rows, not {}.",
        bar.len
    );
    assert!(
        bar.total >= 30,
        "30 lines were written, so the scrollbar's total must be at least 30, not {}; zero means \
         scrollbar() reads nothing.",
        bar.total
    );
    assert_eq!(
        bar.offset,
        bar.total - bar.len,
        "at the bottom the offset must be total - len = {}, not {}.",
        bar.total - bar.len,
        bar.offset
    );
    assert!(
        term.viewport_at_bottom(),
        "a terminal that was never scrolled must report its viewport at the bottom."
    );
    let bottom_row0 = screen_of(&mut term).row_text(0);
    assert_eq!(
        bottom_row0, "line 20",
        "at the bottom of 30 lines on 10 rows, row 0 must read \"line 20\", not {bottom_row0:?}."
    );

    term.scroll_viewport(ScrollViewport::Delta(-5));
    let scrolled = term.scrollbar();
    assert_eq!(
        scrolled.offset,
        bar.offset - 5,
        "a scroll of -5 must lower the offset by 5, from {} to {}, not to {}. An unchanged \
         offset means scroll_viewport does nothing or scrollbar() is stale.",
        bar.offset,
        bar.offset - 5,
        scrolled.offset
    );
    assert!(
        !term.viewport_at_bottom(),
        "after scrolling up the viewport is not at the bottom, so viewport_at_bottom() must be \
         false; true means it does not read the viewport."
    );
    let row0 = screen_of(&mut term).row_text(0);
    assert_eq!(
        row0, "line 15",
        "after scrolling 5 rows up, row 0 must show the earlier \"line 15\", not {row0:?}: the \
         screen must read the viewport, not the active area."
    );

    term.scroll_viewport(ScrollViewport::Top);
    assert_eq!(
        term.scrollbar().offset,
        0,
        "after scrolling to the top the offset must be 0."
    );
    let top_row0 = screen_of(&mut term).row_text(0);
    assert_eq!(
        top_row0, "line 0",
        "at the top, row 0 must show the first line written, not {top_row0:?}."
    );

    term.scroll_viewport(ScrollViewport::Bottom);
    assert_eq!(
        term.scrollbar(),
        bar,
        "scrolling back to the bottom must restore the scrollbar to {bar:?}, not {:?}.",
        term.scrollbar()
    );
    assert!(
        term.viewport_at_bottom(),
        "after scrolling to the bottom viewport_at_bottom() must be true."
    );
}

/// `dec_mode`, `mouse_tracking` and `alternate_screen` follow the modes the program sets.
#[test]
fn modes_follow_what_the_program_sets() {
    let mut term = terminal(10, 3);
    assert!(
        !term.dec_mode(2004),
        "a fresh terminal has bracketed paste off, so dec_mode(2004) must be false; true means \
         dec_mode reports every mode set."
    );
    assert!(
        term.dec_mode(25),
        "a fresh terminal shows its cursor, so dec_mode(25) must be true; false means dec_mode \
         reports no mode set."
    );
    term.feed(b"\x1b[?2004h");
    assert!(
        term.dec_mode(2004),
        "after CSI ? 2004 h, dec_mode(2004) must be true."
    );

    assert!(
        !term.mouse_tracking(),
        "a fresh terminal has no mouse tracking, so mouse_tracking() must be false."
    );
    term.feed(b"\x1b[?1000h");
    assert!(
        term.mouse_tracking(),
        "after CSI ? 1000 h, mouse_tracking() must be true."
    );

    assert!(
        !term.alternate_screen(),
        "a fresh terminal is on the primary screen, so alternate_screen() must be false."
    );
    term.feed(b"\x1b[?1049h");
    assert!(
        term.alternate_screen(),
        "after CSI ? 1049 h, alternate_screen() must be true."
    );
}
