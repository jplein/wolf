//! Cairo/Pango paint seam: turn a [`wolf_core::Snapshot`] into pixels on a Cairo
//! context.
//!
//! [`paint`] is pure and widget-free: tests bind it to an offscreen Cairo
//! `ImageSurface`, production will bind it to a `DrawingArea`'s context. That keeps
//! the rasterization path verifiable headlessly (no window, no display server).
//!
//! ## Algorithm
//!
//! For every grid cell `(row, col)`:
//! 1. Resolve its highlight via [`wolf_core::HighlightTable::resolve`] (which has
//!    already applied `reverse` and the editor defaults).
//! 2. Pick fg/bg, falling back to the editor defaults and finally to
//!    [`FALLBACK_FG`]/[`FALLBACK_BG`] if even those are unset.
//! 3. Fill the cell's background rectangle (always — this keeps wide-trailing /
//!    never-painted cells aligned).
//! 4. If the cell text is empty (a wide-char trailing placeholder or a
//!    never-painted cell) skip glyph rendering; otherwise lay the text out with
//!    Pango (toggling bold/italic/underline per style) and draw it in fg.
//!
//! Underline is rendered (single line). Undercurl and strikethrough are deferred
//! for Tier 1.

use wolf_core::{ResolvedStyle, Rgb, Snapshot};

/// Divisor to convert an 8-bit channel (0–255) to Cairo's 0.0–1.0 range.
const RGB_MAX: f64 = 255.0;

/// Fallback foreground when neither the highlight nor the editor defaults set one.
pub(crate) const FALLBACK_FG: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};
/// Fallback background when neither the highlight nor the editor defaults set one.
pub(crate) const FALLBACK_BG: Rgb = Rgb { r: 0, g: 0, b: 0 };

/// Per-cell geometry plus the base font used to paint a [`Snapshot`].
///
/// A monospace cell is `cell_w` wide and `cell_h` tall; `baseline` is the text
/// baseline measured from the cell's top edge. All values are in Cairo device
/// units (pixels for an `ImageSurface`).
pub struct CellMetrics {
    /// Cell advance width, in device units.
    pub cell_w: f64,
    /// Cell height (line height), in device units.
    pub cell_h: f64,
    /// Baseline offset from the cell top, in device units.
    pub baseline: f64,
    /// The base font description (cloned and toggled for bold/italic per cell).
    pub font: pango::FontDescription,
}

impl CellMetrics {
    /// Measures monospace cell metrics for `family` at `size_pt` points, using
    /// `cr` only to obtain a Pango context (its surface size is irrelevant).
    ///
    /// The advance width comes from Pango's approximate char width (exact for a
    /// monospace font); the height/baseline come from the font's ascent/descent.
    #[must_use]
    pub fn for_font(cr: &cairo::Context, family: &str, size_pt: f64) -> Self {
        let mut font = pango::FontDescription::new();
        font.set_family(family);
        font.set_size((size_pt * f64::from(pango::SCALE)) as i32);

        let context = pangocairo::functions::create_context(cr);
        let metrics = context.metrics(Some(&font), None);

        let scale = f64::from(pango::SCALE);
        let cell_w = f64::from(metrics.approximate_char_width()) / scale;
        let ascent = f64::from(metrics.ascent()) / scale;
        let descent = f64::from(metrics.descent()) / scale;

        Self {
            cell_w,
            cell_h: ascent + descent,
            baseline: ascent,
            font,
        }
    }
}

/// Paints `snap` onto the Cairo context `cr` using `metrics`.
///
/// Widget-free: `cr` may be backed by an offscreen `ImageSurface` (tests) or a
/// `DrawingArea` (production). See the module docs for the per-cell algorithm.
pub fn paint(cr: &cairo::Context, snap: &Snapshot, metrics: &CellMetrics) {
    let grid = &snap.grid;
    let highlights = &snap.highlights;

    for row in 0..grid.rows {
        for col in 0..grid.cols {
            let Some(cell) = grid.cell(row, col) else {
                continue;
            };
            let style = highlights.resolve(cell.hl_id);
            let fg = style.fg.unwrap_or(FALLBACK_FG);
            let bg = style.bg.unwrap_or(FALLBACK_BG);

            let x = col as f64 * metrics.cell_w;
            let y = row as f64 * metrics.cell_h;

            // Always fill the background rect; this keeps alignment for empty /
            // wide-trailing cells.
            set_source(cr, bg);
            cr.rectangle(x, y, metrics.cell_w, metrics.cell_h);
            cr.fill().expect("cairo background fill failed");

            // Empty text => wide-char trailing placeholder or never-painted cell:
            // background only, no glyph.
            if cell.text.is_empty() {
                continue;
            }

            draw_glyph(cr, metrics, x, y, &cell.text, fg, &style);
        }
    }
}

/// Sets the Cairo source to an opaque `Rgb` color.
fn set_source(cr: &cairo::Context, color: Rgb) {
    cr.set_source_rgb(
        f64::from(color.r) / RGB_MAX,
        f64::from(color.g) / RGB_MAX,
        f64::from(color.b) / RGB_MAX,
    );
}

/// Lays out and draws one cell's text in `fg`, toggling bold/italic/underline per
/// `style`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_glyph(
    cr: &cairo::Context,
    metrics: &CellMetrics,
    x: f64,
    y: f64,
    text: &str,
    fg: Rgb,
    style: &ResolvedStyle,
) {
    let mut font = metrics.font.clone();
    if style.bold {
        font.set_weight(pango::Weight::Bold);
    }
    if style.italic {
        font.set_style(pango::Style::Italic);
    }

    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&font));
    layout.set_text(text);

    if style.underline {
        let attrs = pango::AttrList::new();
        attrs.insert(pango::AttrInt::new_underline(pango::Underline::Single));
        layout.set_attributes(Some(&attrs));
    }

    set_source(cr, fg);
    // Pango lays out from the top-left of the line; move to the cell origin.
    cr.move_to(x, y);
    pangocairo::functions::show_layout(cr, &layout);
}
