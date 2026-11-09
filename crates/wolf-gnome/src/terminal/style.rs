//! How one terminal cell paints: its background and foreground after inverse, selection and
//! faint, whether it draws a glyph, and the style the glyph painter takes. Also the cursor's
//! colours and span. Pure: no GTK, no Cairo.

use wolf_core::{
    ResolvedStyle, Rgb, TerminalCell, TerminalCellWidth, TerminalFrame, TerminalUnderline,
};

use crate::statusbar_layout::blend;

/// How far a faint cell's foreground keeps its own colour against the background.
pub const FAINT_ALPHA: f64 = 0.5;

/// What one cell paints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellPaint {
    /// The cell's fill.
    pub background: Rgb,
    /// The glyph's colour, faint already applied.
    pub foreground: Rgb,
    /// Whether the cell's text is drawn at all.
    pub draws_glyph: bool,
    /// The style handed to the glyph painter, carrying `foreground` and `background`.
    pub style: ResolvedStyle,
}

/// Resolves `cell` against the frame's defaults. The colours swap when exactly one of
/// `cell.inverse` and `selected` holds, so a selection over inverse text shows it plain.
/// Blink is ignored: this crate runs no periodic timer, so blinking text draws steady.
#[must_use]
pub fn resolve(cell: &TerminalCell, selected: bool, default_fg: Rgb, default_bg: Rgb) -> CellPaint {
    let fg = cell.fg.unwrap_or(default_fg);
    let bg = cell.bg.unwrap_or(default_bg);
    let (fg, background) = if cell.inverse != selected {
        (bg, fg)
    } else {
        (fg, bg)
    };
    let foreground = if cell.faint {
        blend(fg, background, FAINT_ALPHA)
    } else {
        fg
    };
    let draws_glyph = !cell.invisible
        && !matches!(
            cell.width,
            TerminalCellWidth::SpacerTail | TerminalCellWidth::SpacerHead
        );
    let (underline, undercurl) = match cell.underline {
        TerminalUnderline::None => (false, false),
        TerminalUnderline::Single
        | TerminalUnderline::Double
        | TerminalUnderline::Dotted
        | TerminalUnderline::Dashed => (true, false),
        TerminalUnderline::Curly => (false, true),
    };
    CellPaint {
        background,
        foreground,
        draws_glyph,
        style: ResolvedStyle {
            fg: Some(foreground),
            bg: Some(background),
            special: None,
            bold: cell.bold,
            italic: cell.italic,
            underline,
            undercurl,
            strikethrough: cell.strikethrough,
        },
    }
}

/// Whether `frame` selects cell `col` of `row`; the selection's ends are inclusive.
#[must_use]
pub fn is_selected(frame: &TerminalFrame, col: u16, row: u16) -> bool {
    if col >= frame.cols {
        return false;
    }
    frame
        .selection
        .get(usize::from(row))
        .copied()
        .flatten()
        .is_some_and(|(start, end)| (start..=end).contains(&col))
}

/// The cursor's `(fill, glyph)`: the frame's cursor colour or else its default foreground,
/// and the default background for the glyph a block covers.
#[must_use]
pub fn cursor_colors(frame: &TerminalFrame) -> (Rgb, Rgb) {
    (
        frame.cursor_color.unwrap_or(frame.default_fg),
        frame.default_bg,
    )
}

/// How many cells a cursor on a cell of `width` covers: two over a wide character's lead.
#[must_use]
pub fn cursor_span(width: TerminalCellWidth) -> u16 {
    match width {
        TerminalCellWidth::Wide => 2,
        TerminalCellWidth::Narrow
        | TerminalCellWidth::SpacerTail
        | TerminalCellWidth::SpacerHead => 1,
    }
}
