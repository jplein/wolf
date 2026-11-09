//! A terminal pane's backing surface: the painted cells of its session's frame, repainted row by
//! row as the frame's damage asks, and blitted with the cursor over it on every draw.
//! Display-free Cairo and Pango, like [`crate::render`].

use wolf_core::{
    CursorShape, ResolvedStyle, Rgb, TerminalCellWidth, TerminalCursorStyle, TerminalFrame,
};

use crate::cursor::cursor_rect;
use crate::editor_inset::GridRect;
use crate::render::{draw_glyph, set_source, CellMetrics, GlyphPainter};

use super::damage::{effective_painted_version, rows_to_repaint, PaintKey};
use super::style::{cursor_colors, cursor_span, is_selected, resolve};

/// The width of a hollow cursor's outline, in pixels.
const HOLLOW_LINE_PX: f64 = 1.0;

/// One terminal pane's painted cells. [`update`](Self::update) brings the backing surface up to
/// a frame and [`paint`](Self::paint) blits it; the cursor is never part of the surface.
#[derive(Default)]
pub struct TerminalCanvas {
    /// The painted cells, created at the target's device scale; `None` before the first update.
    surface: Option<cairo::ImageSurface>,
    /// The key the surface was painted under.
    key: Option<PaintKey>,
    /// The frame version the surface holds.
    painted: Option<u64>,
}

impl TerminalCanvas {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Repaints the rows of `frame` the surface lacks, recreating the surface when the paint key
    /// changed, and returns them ascending: every row after a key change, none for a frame
    /// already painted. `target` is the context the surface will be blitted into, whose font
    /// options the glyphs take; `scale` is its device scale.
    pub fn update(
        &mut self,
        target: &cairo::Context,
        frame: &TerminalFrame,
        metrics: &CellMetrics,
        scale: f64,
    ) -> Vec<u16> {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let key = PaintKey {
            cols: frame.cols,
            rows: frame.rows,
            default_bg: frame.default_bg,
            cell_w: metrics.cell_w,
            cell_h: metrics.cell_h,
            scale,
        };
        let mut painted = effective_painted_version(self.key, key, self.painted);
        if painted.is_none() || self.surface.is_none() {
            self.surface = backing_surface(frame, metrics, scale);
            painted = None;
        }
        let Some(surface) = &self.surface else {
            self.key = None;
            self.painted = None;
            return Vec::new();
        };
        let rows = rows_to_repaint(&frame.row_versions, painted);
        if !rows.is_empty() {
            let cr = cairo::Context::new(surface).expect("cairo terminal canvas context failed");
            if let Ok(options) = target.font_options() {
                cr.set_font_options(&options);
            }
            paint_rows(&cr, frame, metrics, &rows);
        }
        self.key = Some(key);
        self.painted = Some(frame.version);
        rows
    }

    /// Paints into `rect`: the frame's default background over the whole rect, the backing
    /// surface at the rect's origin rounded to whole pixels, then the cursor. Nothing reaches
    /// outside `rect`, so a frame larger than it is clipped.
    pub fn paint(
        &self,
        cr: &cairo::Context,
        rect: GridRect,
        frame: &TerminalFrame,
        metrics: &CellMetrics,
    ) {
        cr.save().expect("cairo save failed before the terminal");
        cr.rectangle(rect.x, rect.y, rect.w, rect.h);
        cr.clip();
        set_source(cr, frame.default_bg);
        cr.paint().expect("cairo terminal pad fill failed");
        let (x, y) = (rect.x.round(), rect.y.round());
        if let Some(surface) = &self.surface {
            cr.set_source_surface(surface, x, y)
                .expect("cairo terminal canvas source failed");
            cr.paint().expect("cairo terminal canvas blit failed");
        }
        draw_cursor(cr, x, y, frame, metrics);
        cr.restore()
            .expect("cairo restore failed after the terminal");
    }
}

/// A surface for `frame`'s cells at device scale `scale`, or `None` when Cairo refuses one.
fn backing_surface(
    frame: &TerminalFrame,
    metrics: &CellMetrics,
    scale: f64,
) -> Option<cairo::ImageSurface> {
    let width = device_pixels(f64::from(frame.cols) * metrics.cell_w * scale);
    let height = device_pixels(f64::from(frame.rows) * metrics.cell_h * scale);
    match cairo::ImageSurface::create(cairo::Format::Rgb24, width, height) {
        Ok(surface) => {
            surface.set_device_scale(scale, scale);
            Some(surface)
        }
        Err(err) => {
            eprintln!("wolf-gnome: no {width}x{height} terminal surface: {err}");
            None
        }
    }
}

/// `value` rounded up to whole device pixels, at least 1.
fn device_pixels(value: f64) -> i32 {
    let value = if value.is_finite() { value.ceil() } else { 1.0 };
    #[allow(
        clippy::cast_possible_truncation,
        reason = "value is clamped to [1, i32::MAX] and ceil'd, so the cast is exact and in range"
    )]
    {
        value.clamp(1.0, f64::from(i32::MAX)) as i32
    }
}

/// Paints each of `rows` clipped to its own strip: the default background, every cell's
/// background, then the glyphs, so a spacer's background cannot cover half of a wide glyph.
fn paint_rows(cr: &cairo::Context, frame: &TerminalFrame, metrics: &CellMetrics, rows: &[u16]) {
    let width = f64::from(frame.cols) * metrics.cell_w;
    let mut painter = GlyphPainter::new(cr);
    for &row in rows {
        let y = f64::from(row) * metrics.cell_h;
        cr.save().expect("cairo save failed before a terminal row");
        // Whole pixels per cell: adjacent antialiased fills leave a seam of the colour beneath.
        cr.set_antialias(cairo::Antialias::None);
        cr.rectangle(0.0, y, width.ceil(), metrics.cell_h);
        cr.clip();
        set_source(cr, frame.default_bg);
        cr.paint().expect("cairo terminal row fill failed");
        let mut glyphs = Vec::new();
        for col in 0..frame.cols {
            let Some(cell) = frame.cell(col, row) else {
                continue;
            };
            let paint = resolve(
                cell,
                is_selected(frame, col, row),
                frame.default_fg,
                frame.default_bg,
            );
            let x = f64::from(col) * metrics.cell_w;
            set_source(cr, paint.background);
            cr.rectangle(x, y, metrics.cell_w, metrics.cell_h);
            cr.fill().expect("cairo terminal cell fill failed");
            if paint.draws_glyph && !cell.text.is_empty() {
                glyphs.push((x, cell.text.as_str(), paint));
            }
        }
        cr.set_antialias(cairo::Antialias::Default);
        for (x, text, paint) in glyphs {
            painter.draw(cr, metrics, x, y, text, paint.foreground, &paint.style);
        }
        cr.restore()
            .expect("cairo restore failed after a terminal row");
    }
}

/// Draws `frame`'s cursor with the frame's origin at `x`, `y`: a block fills its cells and
/// redraws the covered glyph plainly, a bar or underline fills a strip at least
/// [`crate::cursor::MIN_BAR_PX`] thick, and a hollow cursor strokes the block's outline.
fn draw_cursor(cr: &cairo::Context, x: f64, y: f64, frame: &TerminalFrame, metrics: &CellMetrics) {
    let Some(cursor) = frame.cursor.filter(|c| c.visible) else {
        return;
    };
    let cell = frame.cell(cursor.col, cursor.row);
    let span = cursor_span(cell.map_or(TerminalCellWidth::Narrow, |c| c.width));
    let (fill, glyph) = cursor_colors(frame);
    let cell_x = x + f64::from(cursor.col) * metrics.cell_w;
    let cell_y = y + f64::from(cursor.row) * metrics.cell_h;
    let cell_w = f64::from(span) * metrics.cell_w;
    set_source(cr, fill);
    match cursor.style {
        TerminalCursorStyle::Block => {
            cr.rectangle(cell_x, cell_y, cell_w, metrics.cell_h);
            cr.fill().expect("cairo terminal cursor fill failed");
            if let Some(cell) = cell.filter(|c| !c.text.is_empty() && !c.invisible) {
                let style = plain_style(glyph, fill);
                draw_glyph(cr, metrics, cell_x, cell_y, &cell.text, glyph, &style);
            }
        }
        TerminalCursorStyle::Bar | TerminalCursorStyle::Underline => {
            let shape = if cursor.style == TerminalCursorStyle::Bar {
                CursorShape::Vertical
            } else {
                CursorShape::Horizontal
            };
            let (bx, by, bw, bh) =
                cursor_rect(shape, cell_x, cell_y, cell_w, metrics.cell_h, Some(0));
            cr.rectangle(bx, by, bw, bh);
            cr.fill().expect("cairo terminal cursor fill failed");
        }
        TerminalCursorStyle::BlockHollow => {
            let inset = HOLLOW_LINE_PX / 2.0;
            cr.set_line_width(HOLLOW_LINE_PX);
            cr.rectangle(
                cell_x + inset,
                cell_y + inset,
                (cell_w - HOLLOW_LINE_PX).max(0.0),
                (metrics.cell_h - HOLLOW_LINE_PX).max(0.0),
            );
            cr.stroke().expect("cairo terminal cursor stroke failed");
        }
    }
}

/// The style a block cursor redraws its glyph in: `fg` on `bg`, with no bold or italic, so the
/// glyph cannot overflow the cell.
fn plain_style(fg: Rgb, bg: Rgb) -> ResolvedStyle {
    ResolvedStyle {
        fg: Some(fg),
        bg: Some(bg),
        special: None,
        bold: false,
        italic: false,
        underline: false,
        undercurl: false,
        strikethrough: false,
    }
}
