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
//!    Pango — the style's bold and italic toggling the cloned
//!    [`pango::FontDescription`], its underline, undercurl and strikethrough
//!    arriving as the Pango attributes [`crate::glyph_style`] resolves — and draw
//!    it in fg, offset down the cell by `baseline - ascent` so the glyph box sits
//!    centred in a cell the configured line-height made taller than it.

use wolf_core::{Grid, HighlightTable, ResolvedStyle, Rgb};

use crate::cell_metrics_math;
use crate::glyph_style;

/// Divisor to convert an 8-bit channel (0–255) to Cairo's 0.0–1.0 range.
const RGB_MAX: f64 = 255.0;

/// The resolution assumed when the Pango context reports none, in dots per inch.
///
/// **This is the live path on every headless run, not a defensive guard.**
/// `pangocairo::functions::context_get_resolution` returns `-1.0` on a context
/// created over an `ImageSurface` — measured on this host — and the probe surface
/// `app::measure_metrics`, `tests/paint_golden.rs` and
/// `tests/smoke_headless.rs::cell_metrics_for_smoke` all measure against is
/// exactly such a surface. `wolf-gnome/DECISIONS.md` records that this crate has
/// no HiDPI handling and assumes 96 dpi; this constant is where that assumption
/// stops being implicit.
const PANGO_DEFAULT_DPI: f64 = 96.0;

/// Points per inch, for converting a point size to device pixels.
const POINTS_PER_INCH: f64 = 72.0;

/// Fallback foreground when neither the highlight nor the editor defaults set one.
pub(crate) const FALLBACK_FG: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};
/// Fallback background when neither the highlight nor the editor defaults set one.
pub(crate) const FALLBACK_BG: Rgb = Rgb { r: 0, g: 0, b: 0 };

/// Per-cell geometry plus the base font used to paint a [`Grid`].
///
/// A monospace cell is `cell_w` wide and `cell_h` tall; `baseline` is the text
/// baseline measured from the cell's top edge, and `ascent`/`descent` are the
/// measured glyph box the baseline centres inside that cell. All values are in
/// Cairo device units (pixels for an `ImageSurface`).
pub struct CellMetrics {
    /// Cell advance width, in device units.
    pub cell_w: f64,
    /// Cell height (line height), in device units.
    pub cell_h: f64,
    /// Baseline offset from the cell top, in device units.
    pub baseline: f64,
    /// The measured font ascent, in device units — the glyph box's top half.
    pub ascent: f64,
    /// The measured font descent, in device units — the glyph box's bottom half.
    pub descent: f64,
    /// The configured font size converted to DEVICE PIXELS — the same number the
    /// cell height is derived from, exposed because the per-pane statusbar sizes
    /// its two lines from it (`statusbar_layout` is unit-agnostic and this crate
    /// calls it in device pixels).
    pub size_px: f64,
    /// The base font description (cloned and toggled for bold/italic per cell).
    pub font: pango::FontDescription,
}

impl CellMetrics {
    /// Measures monospace cell metrics for `family` at `size_pt` points and
    /// `line_height_percent` percent, using `cr` only to obtain a Pango context
    /// (its surface size is irrelevant).
    ///
    /// The advance width comes from Pango's approximate char width, exact for a
    /// monospace font and deliberately UNROUNDED — macOS uses `ceil(advance)`
    /// instead, a pre-existing divergence this function does not close, because
    /// rounding it would move every pointer smoke's pixel targets.
    ///
    /// The cell height and baseline come from [`cell_metrics_math`], which rounds
    /// in the unit its caller uses — so `size_pt` is converted to DEVICE PIXELS
    /// here first, since device pixels are what Cairo paints in.
    ///
    /// **No family fallback of this crate's own.** `set_family` accepts any
    /// string and fontconfig substitutes per lookup, always yielding usable
    /// metrics, so an unresolvable family measures fine and the description keeps
    /// the name the caller asked for. macOS needs a fallback because CoreText
    /// returns a DIFFERENT family rather than substituting; here one could only
    /// override fontconfig's better answer.
    #[must_use]
    pub fn for_font(
        cr: &cairo::Context,
        family: &str,
        size_pt: f64,
        line_height_percent: f64,
    ) -> Self {
        let mut font = pango::FontDescription::new();
        font.set_family(family);
        // Deliberately `set_size` and not `set_absolute_size`: the latter changes
        // how the face is rasterized, and every OCR assertion rides on the
        // current rasterization.
        font.set_size((size_pt * f64::from(pango::SCALE)) as i32);

        let context = pangocairo::functions::create_context(cr);
        let metrics = context.metrics(Some(&font), None);

        let scale = f64::from(pango::SCALE);
        let cell_w = f64::from(metrics.approximate_char_width()) / scale;
        let ascent = f64::from(metrics.ascent()) / scale;
        let descent = f64::from(metrics.descent()) / scale;
        // `FontMetrics::height` is Pango's recommended line height and is the
        // counterpart of what `CTFontGetLeading` contributes on macOS. Clamped
        // because a face whose recommended height is under its own glyph box
        // would otherwise make the natural line height shrink.
        let leading = (f64::from(metrics.height()) / scale - ascent - descent).max(0.0);

        let dpi = pangocairo::functions::context_get_resolution(&context);
        let dpi = if dpi.is_finite() && dpi > 0.0 {
            dpi
        } else {
            PANGO_DEFAULT_DPI
        };
        let size_px = size_pt * dpi / POINTS_PER_INCH;

        let cell_h =
            cell_metrics_math::cell_height(size_px, line_height_percent, ascent, descent, leading);
        let baseline = cell_metrics_math::baseline(cell_h, ascent, descent);

        Self {
            cell_w,
            cell_h,
            baseline,
            ascent,
            descent,
            size_px,
            font,
        }
    }
}

/// Paints `grid` (resolved against `highlights`) onto the Cairo context `cr`
/// using `metrics`.
///
/// Under `ext_multigrid` the front-end paints a single grid at a time (the
/// active window grid in S1, a per-pane grid in S3), so this takes the grid and
/// its highlight table directly rather than a whole `Snapshot`.
///
/// Widget-free: `cr` may be backed by an offscreen `ImageSurface` (tests) or a
/// `DrawingArea` (production). See the module docs for the per-cell algorithm.
pub fn paint(cr: &cairo::Context, grid: &Grid, highlights: &HighlightTable, metrics: &CellMetrics) {
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

/// Lays out and draws one cell's text in `fg`, toggling bold and italic on the
/// cloned face and asking Pango for the decorations [`glyph_style`] resolves.
///
/// The undecorated cell — nearly every cell of nearly every frame — builds no
/// `AttrList` at all, which is what
/// [`glyph_style::GlyphDecoration::is_empty`]'s early skip is for.
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

    let decoration = glyph_style::glyph_decoration(style);
    if !decoration.is_empty() {
        let attrs = pango::AttrList::new();
        if decoration.underline != pango::Underline::None {
            attrs.insert(pango::AttrInt::new_underline(decoration.underline));
        }
        if decoration.strikethrough {
            // Deliberately no `AttrColor::new_strikethrough_color`: with no such
            // attribute Pango strikes in the layout's foreground, which is the
            // macOS behaviour. See `glyph_style`'s module docs.
            attrs.insert(pango::AttrInt::new_strikethrough(true));
        }
        if let Some(color) = decoration.underline_color {
            let (r, g, b) = glyph_style::pango_channels(color);
            attrs.insert(pango::AttrColor::new_underline_color(r, g, b));
        }
        layout.set_attributes(Some(&attrs));
    }

    set_source(cr, fg);
    // Pango places the LAYOUT'S TOP at the move-to point, so moving to the cell
    // origin would hang the glyphs from the cell's top edge. `baseline - ascent`
    // is the leftover space above the glyph box, so this is what puts the glyph's
    // baseline at `metrics.baseline` inside a cell the configured line-height made
    // taller than the glyphs. When the cell is exactly the glyph box the offset is
    // exactly 0.0 and nothing moves.
    cr.move_to(x, y + metrics.baseline - metrics.ascent);
    pangocairo::functions::show_layout(cr, &layout);
}

/// Lays out and draws ONE line of free text in `fg`, with the layout's TOP at `y`.
///
/// A sibling of [`draw_glyph`] rather than a widening of it. `draw_glyph` centres a
/// glyph inside a CELL (`baseline - ascent`) and takes its face from
/// [`CellMetrics::font`]; the per-pane statusbar has no cells, needs two different
/// font sizes, and gets its line tops from `crate::statusbar_layout`, which already
/// reserves a whole line box — so no baseline centring applies here. Widening
/// `draw_glyph` instead would touch [`paint`], the cursor and the cmdline strip for
/// nothing.
///
/// Weight and size ride on the caller's `font`, which is why there is no `style`
/// argument: `crate::pane_view` builds its three descriptions once and picks
/// between them per frame.
///
/// `pub(crate)` for the same reason [`draw_glyph`] is: a caller that reimplemented
/// the Pango layout would let the strip's text drift away from the grid paint (see
/// this module's Gotchas in `crates/wolf-gnome/src/README.md`).
pub(crate) fn draw_text_line(
    cr: &cairo::Context,
    font: &pango::FontDescription,
    x: f64,
    y: f64,
    text: &str,
    fg: Rgb,
) {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(font));
    layout.set_text(text);

    set_source(cr, fg);
    // Pango places the LAYOUT'S TOP at the move-to point, which is exactly what the
    // caller's line top means.
    cr.move_to(x, y);
    pangocairo::functions::show_layout(cr, &layout);
}
