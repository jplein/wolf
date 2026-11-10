//! Paint tests for `wolf_gnome::terminal::canvas::TerminalCanvas`, against a real session.
//!
//! Each case starts a real `WolfCore` (so a real Neovim through `WOLF_NVIM_BIN`), opens a
//! `/bin/sh` session in a pty that prints a fixture and then `exec`s `cat`, polls the session's
//! frame until the fixture shows, and paints that frame with `update` (scale 1.0) then `paint`
//! onto an offscreen ARGB32 `ImageSurface` pre-filled with a sentinel colour. No display.
//!
//! Every colour is read off the frame or written into the fixture, never computed by the code
//! under test, except the cursor fill, which is `style::cursor_colors` (pinned by
//! `terminal_seams.rs`). Pixels are sampled at least one pixel inside every cell edge, since cell
//! origins are fractional, and background checks on cells holding a glyph read the band between
//! the cell's top and the glyph box, which a 180 % line height leaves empty.
//!
//! Premises about the emulator, each asserted where it is used rather than assumed: a truecolour
//! SGR reaches the frame as that exact `Rgb`; a left drag with mouse tracking off selects the
//! columns it crossed, inclusive (as `crates/wolf-core/tests/terminal_input.rs` measures); `\033[6
//! q` makes the cursor a bar; and the tty echoes text sent to a cooked `cat` on the cursor row.

mod common;

use cairo::{Context, Format, ImageSurface};

use wolf_core::{
    MouseButton, Rgb, TerminalCellWidth, TerminalCursorStyle, TerminalFrame, TerminalId,
    TerminalMouseEvent, TerminalMouseKind, TerminalTarget, WolfCore,
};
use wolf_gnome::editor_inset::GridRect;
use wolf_gnome::terminal::canvas::TerminalCanvas;
use wolf_gnome::terminal::style::cursor_colors;
use wolf_gnome::CellMetrics;

use common::{
    frame_of, open_session, poll_frame, sh_script, start_core_sized, wait_for_text, FlushWaiter,
    DEFAULT_COLS, DEFAULT_ROWS,
};

/// Pinned family, the one the devShell's `FONTCONFIG_FILE` exposes.
const FONT_FAMILY: &str = "DejaVu Sans Mono";
/// Font size, in points.
const FONT_SIZE_PT: f64 = 16.0;
/// A second size, for the case that changes the cell metrics.
const OTHER_FONT_SIZE_PT: f64 = 14.0;
/// The shipped default line height, which leaves a band above the glyph box to sample.
const LINE_HEIGHT_PERCENT: f64 = 180.0;

/// Where every frame rect starts on its surface: whole pixels, so canvas pixels map 1:1.
const ORIGIN: f64 = 8.0;
/// Surface margin around the rect, in pixels, where only the sentinel may show.
const MARGIN: f64 = 8.0;

/// What the surface holds before `paint`; asserted unlike every colour a frame paints.
const SENTINEL: (u8, u8, u8) = (255, 0, 255);

/// The truecolour background and foreground the first fixture prints.
const FIXTURE_BG: Rgb = Rgb {
    r: 20,
    g: 40,
    b: 160,
};
const FIXTURE_FG: Rgb = Rgb {
    r: 230,
    g: 180,
    b: 20,
};
/// The glyph the truecolour fixture prints: a full block, so its cell interior is solid ink.
const BLOCK: &str = "\u{2588}";
/// How far an antialiased ink pixel may sit from the foreground, per channel.
const INK_TOLERANCE: u8 = 8;

/// The cell size in pixels the selection case gives the session, as `resize_terminal` takes it.
const MOUSE_CELL_W_PX: u32 = 10;
const MOUSE_CELL_H_PX: u32 = 20;

/// A pixel's `(r, g, b)`.
type Rgb8 = (u8, u8, u8);

fn rgb8(colour: Rgb) -> Rgb8 {
    (colour.r, colour.g, colour.b)
}

/// A half-open rectangle of surface pixels: `x0 <= x < x1`, `y0 <= y < y1`.
#[derive(Clone, Copy, Debug)]
struct Span {
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
}

impl Span {
    fn points(self) -> impl Iterator<Item = (usize, usize)> {
        (self.y0..self.y1).flat_map(move |y| (self.x0..self.x1).map(move |x| (x, y)))
    }
}

/// A painted surface's pixels, copied out.
struct Painted {
    data: Vec<u8>,
    stride: usize,
    width: usize,
    height: usize,
}

impl Painted {
    /// The `(r, g, b)` at `(x, y)`; ARGB32 is `B, G, R, A` in memory on little-endian hosts.
    fn pixel(&self, x: usize, y: usize) -> Rgb8 {
        let i = y * self.stride + x * 4;
        (self.data[i + 2], self.data[i + 1], self.data[i])
    }

    /// Every pixel of `span` that is not `colour`, with its position, at most eight of them.
    fn mismatches(&self, span: Span, colour: Rgb8) -> Vec<((usize, usize), Rgb8)> {
        span.points()
            .map(|(x, y)| ((x, y), self.pixel(x, y)))
            .filter(|&(_, got)| got != colour)
            .take(8)
            .collect()
    }
}

/// Measures the cell for `size_pt` on a throwaway surface, as `paint_golden.rs` does.
fn metrics_at(size_pt: f64) -> CellMetrics {
    let probe = ImageSurface::create(Format::ARgb32, 1, 1).expect("probe surface");
    let cr = Context::new(&probe).expect("probe context");
    CellMetrics::for_font(&cr, FONT_FAMILY, size_pt, LINE_HEIGHT_PERCENT)
}

fn metrics() -> CellMetrics {
    metrics_at(FONT_SIZE_PT)
}

/// The rect exactly covering `frame` at [`ORIGIN`].
fn frame_rect(frame: &TerminalFrame, metrics: &CellMetrics) -> GridRect {
    GridRect {
        x: ORIGIN,
        y: ORIGIN,
        w: (f64::from(frame.cols) * metrics.cell_w).ceil(),
        h: (f64::from(frame.rows) * metrics.cell_h).ceil(),
    }
}

/// Paints `frame` with `canvas` into `rect` on a sentinel-filled surface with [`MARGIN`] on its
/// right and bottom, returning the rows `update` repainted and the pixels.
fn paint(
    canvas: &mut TerminalCanvas,
    frame: &TerminalFrame,
    metrics: &CellMetrics,
    rect: GridRect,
) -> (Vec<u16>, Painted) {
    let width = (rect.x + rect.w + MARGIN).ceil() as i32;
    let height = (rect.y + rect.h + MARGIN).ceil() as i32;
    let mut surface = ImageSurface::create(Format::ARgb32, width, height).expect("render surface");
    let cr = Context::new(&surface).expect("render context");
    cr.set_source_rgb(
        f64::from(SENTINEL.0) / 255.0,
        f64::from(SENTINEL.1) / 255.0,
        f64::from(SENTINEL.2) / 255.0,
    );
    cr.paint().expect("sentinel fill");
    let repainted = canvas.update(&cr, frame, metrics, 1.0);
    canvas.paint(&cr, rect, frame, metrics);
    drop(cr);
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data").to_vec();
    (
        repainted,
        Painted {
            data,
            stride,
            width: width as usize,
            height: height as usize,
        },
    )
}

/// The left and top pixel edges of cell `(col, row)` in a rect at [`ORIGIN`].
fn cell_origin(metrics: &CellMetrics, col: u16, row: u16) -> (f64, f64) {
    (
        ORIGIN + f64::from(col) * metrics.cell_w,
        ORIGIN + f64::from(row) * metrics.cell_h,
    )
}

/// The columns at least one pixel inside cell `col`'s left and right edges.
fn inner_columns(metrics: &CellMetrics, col: u16, row: u16) -> (usize, usize) {
    let (x, _) = cell_origin(metrics, col, row);
    (
        x.ceil() as usize + 1,
        (x + metrics.cell_w).floor() as usize - 1,
    )
}

/// Cell `(col, row)` less a one-pixel border.
fn interior(metrics: &CellMetrics, col: u16, row: u16) -> Span {
    let (x0, x1) = inner_columns(metrics, col, row);
    let (_, y) = cell_origin(metrics, col, row);
    Span {
        x0,
        x1,
        y0: y.ceil() as usize + 1,
        y1: (y + metrics.cell_h).floor() as usize - 1,
    }
}

/// The band of cell `(col, row)` between its top and the glyph box, a pixel inside each edge.
fn band_above_glyph(metrics: &CellMetrics, col: u16, row: u16) -> Span {
    let (x0, x1) = inner_columns(metrics, col, row);
    let (_, y) = cell_origin(metrics, col, row);
    let glyph_top = y + metrics.baseline - metrics.ascent;
    let band = Span {
        x0,
        x1,
        y0: y.ceil() as usize + 1,
        y1: glyph_top.floor() as usize - 1,
    };
    assert!(
        band.y1 > band.y0 && band.x1 > band.x0,
        "premise: at {FONT_SIZE_PT} pt and {LINE_HEIGHT_PERCENT} % the cell must leave a band \
         above the glyph box to sample a glyph cell's background in, but cell ({col}, {row}) \
         gives {band:?} (cell_h {}, baseline {}, ascent {}). Raise the line height; do not sample \
         the glyph box",
        metrics.cell_h,
        metrics.baseline,
        metrics.ascent
    );
    band
}

/// Asserts that every pixel of `span` is `colour`, naming `what` and the likely causes.
fn assert_all(painted: &Painted, span: Span, colour: Rgb8, what: &str, causes: &str) {
    let wrong = painted.mismatches(span, colour);
    assert!(
        wrong.is_empty(),
        "{what}: every pixel of {span:?} must be {colour:?}, but these are not (position, \
         colour): {wrong:?}. {causes}"
    );
}

fn within(a: Rgb8, b: Rgb8, tolerance: u8) -> bool {
    a.0.abs_diff(b.0) <= tolerance
        && a.1.abs_diff(b.1) <= tolerance
        && a.2.abs_diff(b.2) <= tolerance
}

/// Opens `script` as a session in Neovim's current window and waits for `needle`.
fn open_with(core: &WolfCore, waiter: &FlushWaiter, script: &str, needle: &str) -> TerminalId {
    let id = open_session(core, TerminalTarget::Window(0), sh_script(script));
    wait_for_text(core, waiter, id, needle);
    id
}

/// Asserts the frame's defaults are unlike `colours` and the sentinel, so no default paint can
/// stand in for a fixture colour.
fn assert_defaults_unlike(frame: &TerminalFrame, colours: &[(Rgb, &str)]) {
    let defaults = [
        (frame.default_fg, "default_fg"),
        (frame.default_bg, "default_bg"),
    ];
    assert_ne!(
        frame.default_fg, frame.default_bg,
        "premise: the frame's default foreground and background are both {:?}, so a swapped \
         colour could not be told from an unswapped one",
        frame.default_fg
    );
    for (default, name) in defaults {
        assert_ne!(
            rgb8(default),
            SENTINEL,
            "premise: the frame's {name} is the sentinel colour, so a pixel paint never reached \
             would read as painted; change SENTINEL"
        );
        for &(colour, colour_name) in colours {
            assert_ne!(
                default, colour,
                "premise: the frame's {name} equals the fixture's {colour_name} {colour:?}, so a \
                 paint that ignored the fixture would still pass; change the fixture colour"
            );
        }
    }
}

#[test]
fn a_sessions_truecolor_text_paints_its_colours() {
    let (core, waiter) = start_core_sized(DEFAULT_COLS, DEFAULT_ROWS);
    let script = format!(
        "printf '\\033[48;2;{};{};{}m\\033[38;2;{};{};{}m  {BLOCK}{BLOCK}  \\033[0m  zz'; exec cat",
        FIXTURE_BG.r, FIXTURE_BG.g, FIXTURE_BG.b, FIXTURE_FG.r, FIXTURE_FG.g, FIXTURE_FG.b
    );
    let id = open_with(&core, &waiter, &script, "zz");
    let frame = frame_of(&core, id);
    assert_defaults_unlike(
        &frame,
        &[(FIXTURE_BG, "background"), (FIXTURE_FG, "foreground")],
    );
    assert!(
        !within(rgb8(FIXTURE_FG), rgb8(frame.default_fg), INK_TOLERANCE),
        "premise: the fixture foreground is within the ink tolerance of default_fg {:?}, so ink \
         in the wrong colour would pass",
        frame.default_fg
    );

    // Locate the fixture by its background, so no column is assumed.
    let coloured: Vec<(u16, u16)> = (0..frame.rows)
        .flat_map(|row| (0..frame.cols).map(move |col| (col, row)))
        .filter(|&(col, row)| {
            frame
                .cell(col, row)
                .is_some_and(|c| c.bg == Some(FIXTURE_BG))
        })
        .collect();
    let glyphs: Vec<(u16, u16)> = coloured
        .iter()
        .copied()
        .filter(|&(col, row)| frame.cell(col, row).is_some_and(|c| c.text == BLOCK))
        .collect();
    let blanks: Vec<(u16, u16)> = coloured
        .iter()
        .copied()
        .filter(|&(col, row)| {
            frame
                .cell(col, row)
                .is_some_and(|c| c.text.trim().is_empty() && c.width == TerminalCellWidth::Narrow)
        })
        .collect();
    assert!(
        glyphs.len() == 2 && blanks.len() >= 4,
        "premise: the fixture must reach the frame as two {BLOCK:?} cells and four blank cells \
         with background {FIXTURE_BG:?}; found glyph cells {glyphs:?} and blank cells {blanks:?} \
         among {coloured:?}. Fewer means the emulator did not keep the truecolour SGR"
    );
    for &(col, row) in &glyphs {
        let cell = frame.cell(col, row).expect("located above");
        assert_eq!(
            cell.fg,
            Some(FIXTURE_FG),
            "premise: the block cell ({col}, {row}) must carry the fixture foreground"
        );
    }
    let &(last_col, row) = coloured.last().expect("non-empty, asserted above");
    let after = last_col + 1;
    assert!(
        frame
            .cell(after, row)
            .is_some_and(|c| c.bg.is_none() && c.text.trim().is_empty()),
        "premise: the cell after the SGR reset, ({after}, {row}), must be blank with no \
         background of its own; it is {:?}",
        frame.cell(after, row)
    );

    let metrics = metrics();
    let mut canvas = TerminalCanvas::new();
    let (_, painted) = paint(&mut canvas, &frame, &metrics, frame_rect(&frame, &metrics));

    let bg = rgb8(FIXTURE_BG);
    let bg_causes = "The default background here means the canvas fills every cell with \
                     `default_bg` instead of the resolved background; the sentinel means the row \
                     was never painted or the canvas was not blitted";
    for &(col, row) in &coloured {
        assert_all(
            &painted,
            band_above_glyph(&metrics, col, row),
            bg,
            &format!("the band above the glyph box of coloured cell ({col}, {row})"),
            bg_causes,
        );
    }
    for &(col, row) in &blanks {
        assert_all(
            &painted,
            interior(&metrics, col, row),
            bg,
            &format!("the interior of blank coloured cell ({col}, {row})"),
            bg_causes,
        );
    }

    let fg = rgb8(FIXTURE_FG);
    for &(col, row) in &glyphs {
        let span = interior(&metrics, col, row);
        let ink: Vec<Rgb8> = span.points().map(|(x, y)| painted.pixel(x, y)).collect();
        assert!(
            ink.iter().any(|&px| within(px, fg, INK_TOLERANCE)),
            "the full block in cell ({col}, {row}) must ink some pixel within {INK_TOLERANCE} of \
             the fixture foreground {fg:?}; none of {} pixels does (first: {:?}). Ink in \
             default_fg {:?} means glyphs are drawn in the default foreground; only background \
             means the glyph was skipped",
            ink.len(),
            ink.first(),
            rgb8(frame.default_fg)
        );
    }

    assert_all(
        &painted,
        interior(&metrics, after, row),
        rgb8(frame.default_bg),
        &format!("the interior of cell ({after}, {row}), after the SGR reset"),
        "The fixture background here means the resolved background leaks past the reset; the \
         sentinel means the cell was never painted",
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn inverse_and_a_drag_selection_paint_swapped_colours() {
    let (core, waiter) = start_core_sized(DEFAULT_COLS, DEFAULT_ROWS);
    let id = open_with(
        &core,
        &waiter,
        "printf 'aa\\033[7m   \\033[0m\\nselectable\\n'; exec cat",
        "selectable",
    );
    let frame = frame_of(&core, id);
    assert_defaults_unlike(&frame, &[]);

    let inverse: Vec<u16> = (0..frame.cols)
        .filter(|&col| frame.cell(col, 0).is_some_and(|c| c.inverse))
        .collect();
    assert_eq!(
        inverse.len(),
        3,
        "premise: `\\033[7m` must mark the three spaces after `aa` inverse on row 0; found \
         inverse columns {inverse:?}"
    );

    // Selection: cell px set explicitly, then a left drag from column 2 to 5 of row 1.
    let select_row = 1;
    assert!(
        frame.row_text(select_row).starts_with("selectable"),
        "premise: row {select_row} must hold the selectable word; it reads {:?}",
        frame.row_text(select_row)
    );
    core.resize_terminal(id, frame.cols, frame.rows, MOUSE_CELL_W_PX, MOUSE_CELL_H_PX);
    let at = |kind, col: u16| TerminalMouseEvent {
        kind,
        button: Some(MouseButton::Left),
        x: (f32::from(col) + 0.5) * MOUSE_CELL_W_PX as f32,
        y: (f32::from(select_row) + 0.5) * MOUSE_CELL_H_PX as f32,
        shift: false,
        ctrl: false,
        alt: false,
    };
    core.terminal_mouse(id, at(TerminalMouseKind::Press, 2));
    core.terminal_mouse(id, at(TerminalMouseKind::Motion, 5));
    core.terminal_mouse(id, at(TerminalMouseKind::Release, 5));
    let frame = poll_frame(
        &core,
        &waiter,
        id,
        "the drag's selection on row 1; none means the drag did not select",
        |frame| {
            frame
                .selection
                .get(usize::from(select_row))
                .copied()
                .flatten()
                .is_some()
        },
    );
    let (first, last) = frame.selection[usize::from(select_row)].expect("polled for above");
    assert!(
        last > first && first > 0 && usize::from(last) + 1 < "selectable".len(),
        "premise: the drag must select at least two cells inside the word, leaving one \
         unselected on each side; it selected columns {first} to {last}"
    );

    let metrics = metrics();
    let mut canvas = TerminalCanvas::new();
    let (_, painted) = paint(&mut canvas, &frame, &metrics, frame_rect(&frame, &metrics));

    for &col in &inverse {
        assert_all(
            &painted,
            interior(&metrics, col, 0),
            rgb8(frame.default_fg),
            &format!("the interior of inverse cell ({col}, 0)"),
            "An inverse blank with no colours must fill in default_fg; default_bg means inverse \
             is not applied",
        );
    }
    for col in first..=last {
        let cell = frame.cell(col, select_row).expect("inside the frame");
        let swapped = rgb8(cell.fg.unwrap_or(frame.default_fg));
        assert_all(
            &painted,
            band_above_glyph(&metrics, col, select_row),
            swapped,
            &format!("the background band of selected cell ({col}, {select_row})"),
            "A selected cell paints its foreground as its background; default_bg here means the \
             canvas resolves cells as unselected",
        );
    }
    for col in [first - 1, last + 1] {
        assert_all(
            &painted,
            band_above_glyph(&metrics, col, select_row),
            rgb8(frame.default_bg),
            &format!("the background band of unselected cell ({col}, {select_row})"),
            "A cell next to the selection keeps the default background; default_fg here means the \
             selection is painted wider than the frame's inclusive range",
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn the_cursor_paints_in_its_colour_and_shape() {
    let (core, waiter) = start_core_sized(DEFAULT_COLS, DEFAULT_ROWS);

    // A block cursor, the default style, after `ab` on row 0.
    let block = open_with(&core, &waiter, "printf 'ab'; exec cat", "ab");
    let frame = poll_frame(
        &core,
        &waiter,
        block,
        "a visible cursor at (2, 0)",
        |frame| {
            frame
                .cursor
                .is_some_and(|c| c.visible && (c.col, c.row) == (2, 0))
        },
    );
    let cursor = frame.cursor.expect("polled for above");
    assert_eq!(
        cursor.style,
        TerminalCursorStyle::Block,
        "premise: a session that set no cursor style must show a block"
    );
    let (fill, _) = cursor_colors(&frame);
    assert_ne!(
        fill, frame.default_bg,
        "premise: the cursor fill equals default_bg, so a cursor that was never drawn would pass"
    );
    let metrics = metrics();
    let mut canvas = TerminalCanvas::new();
    let (_, painted) = paint(&mut canvas, &frame, &metrics, frame_rect(&frame, &metrics));
    assert_all(
        &painted,
        interior(&metrics, cursor.col, cursor.row),
        rgb8(fill),
        "the block cursor's empty cell",
        "default_bg here means the cursor was not drawn over the blit",
    );

    // A steady bar, set with DECSCUSR 6.
    let bar = open_with(&core, &waiter, "printf 'ab\\033[6 q'; exec cat", "ab");
    let frame = poll_frame(&core, &waiter, bar, "a bar cursor at (2, 0)", |frame| {
        frame.cursor.is_some_and(|c| {
            c.visible && (c.col, c.row) == (2, 0) && c.style == TerminalCursorStyle::Bar
        })
    });
    let cursor = frame.cursor.expect("polled for above");
    let (fill, _) = cursor_colors(&frame);
    let mut canvas = TerminalCanvas::new();
    let (_, painted) = paint(&mut canvas, &frame, &metrics, frame_rect(&frame, &metrics));
    let cell = interior(&metrics, cursor.col, cursor.row);
    let (left, _) = cell_origin(&metrics, cursor.col, cursor.row);
    // The bar is at least MIN_BAR_PX (2) wide from the cell's left edge, so this column is covered.
    let bar_x = left.ceil() as usize;
    assert_all(
        &painted,
        Span {
            x0: bar_x,
            x1: bar_x + 1,
            ..cell
        },
        rgb8(fill),
        "the bar cursor's leftmost whole column",
        "default_bg here means the bar was not drawn",
    );
    let middle = (left + metrics.cell_w / 2.0).floor() as usize;
    assert_all(
        &painted,
        Span {
            x0: middle,
            x1: middle + 1,
            ..cell
        },
        rgb8(frame.default_bg),
        "the middle column of the bar cursor's cell",
        "The cursor fill here means a bar is drawn as a block",
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn a_new_frame_repaints_only_its_damaged_rows() {
    let (core, waiter) = start_core_sized(DEFAULT_COLS, DEFAULT_ROWS);
    let id = open_with(
        &core,
        &waiter,
        "printf 'zero\\nfirst\\n'; exec cat",
        "first",
    );
    let first = poll_frame(&core, &waiter, id, "the cursor on row 2", |frame| {
        frame.cursor.is_some_and(|c| c.row == 2)
    });
    let all: Vec<u16> = (0..first.rows).collect();
    let metrics = metrics();
    let rect = frame_rect(&first, &metrics);
    let mut canvas = TerminalCanvas::new();

    let (repainted, _) = paint(&mut canvas, &first, &metrics, rect);
    assert_eq!(
        repainted, all,
        "a canvas's first update must repaint every row, ascending; fewer means rows of the \
         backing surface are left unpainted"
    );

    core.terminal_text(id, "x");
    let second = poll_frame(&core, &waiter, id, "the echoed x on row 2", |frame| {
        frame.row_text(2).contains('x')
    });
    let (repainted, _) = paint(&mut canvas, &second, &metrics, rect);
    assert!(
        repainted.contains(&2) && !repainted.contains(&0) && repainted.len() < all.len(),
        "after the tty echoed one character on row 2, update must repaint row 2 and not row 0 \
         (unchanged since the last paint), and fewer than all {} rows; it repainted {repainted:?}. \
         Every row means the painted version is ignored; no row 2 means damage is lost",
        all.len()
    );

    let (repainted, _) = paint(&mut canvas, &second, &metrics, rect);
    assert!(
        repainted.is_empty(),
        "an update with the frame already painted must repaint nothing; it repainted \
         {repainted:?}, so the painted version is not recorded"
    );

    let other = metrics_at(OTHER_FONT_SIZE_PT);
    assert!(
        other.cell_w != metrics.cell_w && other.cell_h != metrics.cell_h,
        "premise: {OTHER_FONT_SIZE_PT} pt must give a different cell than {FONT_SIZE_PT} pt"
    );
    let (repainted, _) = paint(&mut canvas, &second, &other, frame_rect(&second, &other));
    assert_eq!(
        repainted, all,
        "a change of cell metrics changes the paint key, so update must repaint every row; \
         {repainted:?} means rows painted at the old cell size are kept"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn a_frame_is_padded_and_clipped_to_its_rect() {
    let (core, waiter) = start_core_sized(DEFAULT_COLS, DEFAULT_ROWS);
    let id = open_with(&core, &waiter, "printf 'pad\\n'; exec cat", "pad");
    core.resize_terminal(id, 20, 6, 0, 0);
    let frame = poll_frame(&core, &waiter, id, "the 20x6 resize", |frame| {
        (frame.cols, frame.rows) == (20, 6)
    });
    assert_defaults_unlike(&frame, &[]);
    let metrics = metrics();
    let covered = frame_rect(&frame, &metrics);

    // Larger than the frame: the uncovered part of the rect is the default background.
    let wide = GridRect {
        w: covered.w + 40.0,
        h: covered.h + 30.0,
        ..covered
    };
    let mut canvas = TerminalCanvas::new();
    let (_, painted) = paint(&mut canvas, &frame, &metrics, wide);
    let frame_right = (ORIGIN + covered.w) as usize + 1;
    let frame_bottom = (ORIGIN + covered.h) as usize + 1;
    let rect_right = (wide.x + wide.w) as usize - 1;
    let rect_bottom = (wide.y + wide.h) as usize - 1;
    let pad_causes = "The sentinel here means the rect is not filled with default_bg before the \
                      blit, so the area a frame does not cover shows whatever was under it";
    assert_all(
        &painted,
        Span {
            x0: frame_right,
            x1: rect_right,
            y0: ORIGIN as usize + 1,
            y1: rect_bottom,
        },
        rgb8(frame.default_bg),
        "the rect right of the frame",
        pad_causes,
    );
    assert_all(
        &painted,
        Span {
            x0: ORIGIN as usize + 1,
            x1: rect_right,
            y0: frame_bottom,
            y1: rect_bottom,
        },
        rgb8(frame.default_bg),
        "the rect below the frame",
        pad_causes,
    );

    // Smaller than the frame: nothing outside the rect may change.
    let small = GridRect {
        w: (covered.w / 2.0).floor(),
        h: (covered.h / 2.0).floor(),
        ..covered
    };
    let mut canvas = TerminalCanvas::new();
    let (_, painted) = paint(&mut canvas, &frame, &metrics, small);
    let (x_end, y_end) = ((small.x + small.w) as usize, (small.y + small.h) as usize);
    let outside: Vec<((usize, usize), Rgb8)> = (0..painted.height)
        .flat_map(|y| (0..painted.width).map(move |x| (x, y)))
        .filter(|&(x, y)| x < ORIGIN as usize || y < ORIGIN as usize || x >= x_end || y >= y_end)
        .map(|(x, y)| ((x, y), painted.pixel(x, y)))
        .filter(|&(_, px)| px != SENTINEL)
        .take(8)
        .collect();
    assert!(
        outside.is_empty(),
        "a frame larger than its rect must be clipped to it, but pixels outside the rect \
         {small:?} changed (position, colour): {outside:?}. The paint does not clip to the rect"
    );
    assert_all(
        &painted,
        Span {
            x0: ORIGIN as usize + 1,
            x1: ORIGIN as usize + 2,
            y0: ORIGIN as usize + 1,
            y1: ORIGIN as usize + 2,
        },
        rgb8(frame.default_bg),
        "the clipped rect's top-left pixel",
        "The sentinel here means nothing was painted at all, so the clip check above is vacuous",
    );

    core.shutdown().expect("shutdown should be Ok");
}
