//! Headless tests of the `wolf-vt` binding over libghostty-vt: bytes go in through
//! `Terminal::feed` and `Terminal::resize`, and cells, the cursor and colours come back
//! out through `Terminal::screen`, `Terminal::cursor` and the colour getters.
//!
//! No Neovim, no pty and no runfiles: the `//crates/wolf-vt:binding` target links the
//! static archive and nothing else, and `cargo test -p wolf-vt` runs the same file on
//! Linux. The point of running it on both platforms is that each builds libghostty-vt
//! from its own pin, so `the_linked_library_is_the_pinned_release` is the check that
//! fails on whichever side a one-sided bump leaves behind.
//!
//! ## Premises about libghostty-vt
//!
//! Each was read from the pinned 1.0.0 headers (`include/ghostty/vt/render.h` and
//! `terminal.h`) and is measured here against the real library, not assumed:
//!
//! - A new terminal shows the cursor (DEC mode 25 is set) at the top-left.
//! - `CSI row;col H` is 1-based, so `ESC[3;7H` puts the next character at column 6,
//!   row 2 in the 0-based coordinates the binding reports.
//! - A cell's resolved colour is `GHOSTTY_NO_VALUE` when the cell has none of its own,
//!   which the binding reports as `None`. A palette colour such as `SGR 41` is resolved
//!   through the terminal's palette, so it equals `palette()[1]`.
//! - The terminal's default colours set through `set_default_colors` are what
//!   `GHOSTTY_TERMINAL_DATA_COLOR_FOREGROUND` and `…_BACKGROUND` read back.

use wolf_vt::{version, Cell, Cursor, Rgb, Screen, Terminal};

/// The libghostty-vt release both platforms pin: `wolfGhosttyVtPin` in `flake.nix` and
/// `bazel/ghostty-vt/extensions.bzl`. Re-typed here rather than read from either.
const PINNED_VERSION: &str = "1.0.0";

/// Looks a cell up, failing with what the screen holds when it is out of range.
fn cell_at(screen: &Screen, col: u16, row: u16) -> &Cell {
    screen.cell(col, row).unwrap_or_else(|| {
        panic!(
            "Screen::cell({col}, {row}) is None on a {}x{} screen, so the cell the test wrote \
             is outside what the binding reports. Either the screen's size is wrong or `cell` \
             swaps its column and row arguments.",
            screen.cols, screen.rows
        )
    })
}

/// Takes the screen, failing with the binding's error code.
fn screen_of(terminal: &mut Terminal) -> Screen {
    terminal.screen().unwrap_or_else(|error| {
        panic!(
            "Terminal::screen() failed with libghostty-vt result code {}, so no cell can be \
             read back at all: the render state, its row iterator or its row cells could not \
             be read.",
            error.code()
        )
    })
}

/// Makes a terminal, failing with the binding's error code.
fn terminal(cols: u16, rows: u16) -> Terminal {
    Terminal::new(cols, rows).unwrap_or_else(|error| {
        panic!(
            "Terminal::new({cols}, {rows}) failed with libghostty-vt result code {}, so the \
             library linked but would not allocate a terminal.",
            error.code()
        )
    })
}

/// The library the binary linked is the pinned release.
#[test]
fn the_linked_library_is_the_pinned_release() {
    let linked = version();
    assert_eq!(
        linked, PINNED_VERSION,
        "wolf_vt::version() reports {linked:?}, not the pinned {PINNED_VERSION:?}. Empty means \
         the binding does not read ghostty_version(); another release means this platform's pin \
         moved without the other's, or without this test; a suffix such as \"-dev\" means a \
         development build was linked instead of the release tarball."
    );
}

/// Plain text, including a multi-byte character, lands in the cells in order and moves
/// the cursor one column per character; a CR LF moves it to the next row.
#[test]
fn text_lands_in_cells_and_moves_the_cursor() {
    let mut term = terminal(20, 3);
    assert_eq!(
        term.size(),
        (20, 3),
        "a terminal made as 20 columns by 3 rows reports {:?} as its (cols, rows).",
        term.size()
    );

    term.feed("héllo".as_bytes());
    let screen = screen_of(&mut term);
    assert_eq!(
        (screen.cols, screen.rows),
        (20, 3),
        "the screen of a 20x3 terminal is {}x{}, so `screen()` does not read the viewport's size.",
        screen.cols,
        screen.rows
    );
    assert_eq!(
        screen.row_text(0),
        "héllo",
        "row 0 after feeding \"héllo\" reads {:?}. Empty means `feed` never reached \
         ghostty_terminal_vt_write or `screen()` reads a stale render state; mangled text \
         around the \"é\" means a cell's grapheme is decoded from the wrong codepoints.",
        screen.row_text(0)
    );
    assert_eq!(
        cell_at(&screen, 1, 0).text,
        "é",
        "cell (1, 0) holds {:?}, not \"é\": the two UTF-8 bytes of one character must make one \
         cell, and a split into two cells shifts every later character right.",
        cell_at(&screen, 1, 0).text
    );
    assert_eq!(
        term.cursor(),
        Cursor {
            col: 5,
            row: 0,
            visible: true
        },
        "after five characters on a fresh terminal the cursor must be visible at column 5, row 0."
    );

    term.feed(b"\r\nworld");
    let screen = screen_of(&mut term);
    assert_eq!(
        screen.row_text(1),
        "world",
        "row 1 after feeding \"\\r\\nworld\" reads {:?}, so CR LF did not start a new row or the \
         second feed was dropped.",
        screen.row_text(1)
    );
    assert_eq!(
        screen.row_text(0),
        "héllo",
        "row 0 changed to {:?} when only row 1 was written, so `screen()` reads the wrong rows.",
        screen.row_text(0)
    );
    assert_eq!(
        screen.row_text(3),
        "",
        "row 3 of a 3-row screen is past the last row and must read as \"\", not {:?}.",
        screen.row_text(3)
    );
    assert_eq!(
        term.cursor(),
        Cursor {
            col: 5,
            row: 1,
            visible: true
        },
        "after \"\\r\\nworld\" the cursor must be at column 5 of row 1."
    );
}

/// A cursor-position escape moves where the next character lands and what `cursor()`
/// reports, and DEC mode 25 hides and shows the cursor.
#[test]
fn a_cursor_move_and_mode_25_reach_the_cursor() {
    let mut term = terminal(20, 5);
    term.feed(b"\x1b[3;7HX");

    let screen = screen_of(&mut term);
    assert_eq!(
        cell_at(&screen, 6, 2).text,
        "X",
        "ESC[3;7H then \"X\" must put \"X\" at column 6, row 2 (0-based). Row 2 reads {:?}.",
        screen.row_text(2)
    );
    assert_eq!(
        screen.row_text(0),
        "",
        "row 0 reads {:?}, so the \"X\" was written at the top-left: the cursor-position \
         escape was not interpreted.",
        screen.row_text(0)
    );
    // Column 7 and row 2 differ, so a swapped pair reads as (2, 7) here.
    assert_eq!(
        term.cursor(),
        Cursor {
            col: 7,
            row: 2,
            visible: true
        },
        "after \"X\" at column 6, row 2 the cursor must be visible at column 7, row 2. \
         Column 2, row 7 means `cursor()` swaps its column and row."
    );

    term.feed(b"\x1b[?25l");
    assert_eq!(
        term.cursor(),
        Cursor {
            col: 7,
            row: 2,
            visible: false
        },
        "ESC[?25l resets DEC mode 25, so the cursor must report invisible at the same place. \
         `visible: true` means `cursor()` does not read the mode."
    );

    term.feed(b"\x1b[?25h");
    assert!(
        term.cursor().visible,
        "ESC[?25h sets DEC mode 25 again, so the cursor must report visible. Invisible means \
         `cursor()` reports a value that does not follow the mode."
    );
}

/// A true-colour foreground, a palette background and an unstyled cell come back as
/// the colour, the palette's colour and `None`, and the default colours read back as set.
#[test]
fn cell_colours_resolve_and_default_colours_read_back() {
    let foreground = Rgb { r: 1, g: 2, b: 3 };
    let background = Rgb { r: 4, g: 5, b: 6 };
    let mut term = terminal(20, 3);
    term.set_default_colors(foreground, background)
        .unwrap_or_else(|error| {
            panic!(
                "set_default_colors failed with libghostty-vt result code {}.",
                error.code()
            )
        });
    assert_eq!(
        term.foreground(),
        Some(foreground),
        "the default foreground reads back as {:?} after being set to {foreground:?}. None or \
         another colour means `set_default_colors` never reached the terminal, or \
         `foreground()` reads a different colour.",
        term.foreground()
    );
    assert_eq!(
        term.background(),
        Some(background),
        "the default background reads back as {:?} after being set to {background:?}. Equal to \
         the foreground means the two getters read the same colour.",
        term.background()
    );

    let palette = term.palette();
    assert_ne!(
        palette[1], palette[0],
        "palette entries 0 and 1 are both {:?}, so the palette `R`'s background is checked \
         against cannot tell index 1 from a zeroed array: `palette()` does not read the \
         terminal's palette.",
        palette[0]
    );

    term.feed(b"\x1b[38;2;10;20;30mT\x1b[0m\x1b[41mR\x1b[0mD");
    let screen = screen_of(&mut term);
    assert_eq!(
        screen.row_text(0),
        "TRD",
        "row 0 must read \"TRD\", the three characters around the escapes, not {:?}; escape \
         bytes printed as text mean the bytes did not go through the VT parser.",
        screen.row_text(0)
    );

    let t = cell_at(&screen, 0, 0);
    assert_eq!(
        t.fg,
        Some(Rgb {
            r: 10,
            g: 20,
            b: 30
        }),
        "`T` after SGR 38;2;10;20;30 must have the foreground (10, 20, 30), not {:?}. None means \
         `screen()` drops every cell's foreground.",
        t.fg
    );
    assert_eq!(
        t.bg, None,
        "`T` sets no background, so its bg must be None, not {:?}.",
        t.bg
    );

    let r = cell_at(&screen, 1, 0);
    assert_eq!(
        r.fg, None,
        "`R` after SGR 0 and SGR 41 sets no foreground, so its fg must be None, not {:?}. \
         (10, 20, 30) means SGR 0 did not reset the previous style.",
        r.fg
    );
    assert_eq!(
        r.bg,
        Some(palette[1]),
        "`R` after SGR 41 must have palette entry 1, {:?}, as its resolved background, not {:?}. \
         None means the palette index was not resolved; the default background {background:?} \
         means the cell's own colour was replaced by the terminal's.",
        palette[1],
        r.bg
    );

    let d = cell_at(&screen, 2, 0);
    assert_eq!(
        (d.fg, d.bg),
        (None, None),
        "`D` after SGR 0 has no colour of its own, so fg and bg must both be None, not {:?}. \
         Some((0, 0, 0)) means GHOSTTY_NO_VALUE is read as a colour instead of as absent.",
        (d.fg, d.bg)
    );
}

/// A resize changes the grid the emulator writes into, the size the binding reports and
/// the screen it reads, and keeps what was written.
#[test]
fn a_resize_reaches_the_grid_and_keeps_its_text() {
    let mut term = terminal(10, 4);
    term.feed(b"abc");
    assert_eq!(
        term.size(),
        (10, 4),
        "a 10x4 terminal reports {:?}.",
        term.size()
    );

    term.resize(30, 8, 8, 16).unwrap_or_else(|error| {
        panic!(
            "resize(30, 8, 8, 16) failed with libghostty-vt result code {}.",
            error.code()
        )
    });
    assert_eq!(
        term.size(),
        (30, 8),
        "after resize(30, 8, …) the terminal reports {:?}. (10, 4) means `resize` returned Ok \
         without resizing the terminal.",
        term.size()
    );

    let screen = screen_of(&mut term);
    assert_eq!(
        (screen.cols, screen.rows),
        (30, 8),
        "after resize(30, 8, …) the screen is {}x{}. 10x4 means the render state was read from \
         the old size, or the terminal itself was never resized.",
        screen.cols,
        screen.rows
    );
    assert_eq!(
        screen.row_text(0),
        "abc",
        "row 0 reads {:?} after a grow; the text written before the resize must survive it.",
        screen.row_text(0)
    );

    // Column 26 exists only in the resized grid; at 10 columns the move clamps to the last column.
    term.feed(b"\x1b[1;26HZ");
    let screen = screen_of(&mut term);
    assert_eq!(
        screen.cell(25, 0).map(|cell| cell.text.as_str()),
        Some("Z"),
        "ESC[1;26H then \"Z\" must write column 25 of the 30-column grid. Row 0 reads {:?}; a \
         \"Z\" at column 9 means the emulator's grid is still 10 wide whatever `size()` says.",
        screen.row_text(0)
    );
}

/// `Terminal` may move to another thread: the reader thread that owns it in core is not
/// the thread that made it.
#[test]
fn a_terminal_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Terminal>();
}
