//! Headless tests of `wolf_gnome::render::draw_cells`, the text paint of the message
//! toast and the completion popup.
//!
//! Both surfaces are sized in grid cells, `cols * cell_w` plus
//! `cmdline_ui::SURFACE_PADDING_PX` on each side. A string laid out as one Pango run
//! advances by Pango's whole-pixel-rounded glyph widths instead of `cell_w`, so where
//! the face's advance rounds up the run outgrows its box and the last character is
//! cut off. These tests paint onto an offscreen ARGB32 `ImageSurface` exactly as big
//! as the toast's box for one line, at a size where that overrun is measured by a
//! precondition, and read where the ink landed.
//!
//! No window, no Neovim, no compositor. Font determinism rests on the pinned
//! `DejaVu Sans Mono` the devShell exposes through `FONTCONFIG_FILE`, as in
//! `paint_golden.rs`.

use cairo::{Context, Format, ImageSurface};

use wolf_core::{ResolvedStyle, Rgb};
use wolf_gnome::cmdline_ui::SURFACE_PADDING_PX;
use wolf_gnome::render::draw_cells;
use wolf_gnome::CellMetrics;

/// Pinned font family, resolved through the devShell's fontconfig.
const FONT_FAMILY: &str = "DejaVu Sans Mono";

/// Font size, in points, chosen so that a Pango run outgrows the cells it is
/// measured in.
///
/// At 96 dpi this is 16 px, where the face's design advance is about 9.63 px and a
/// run is expected to advance by a rounded 10 px a glyph: about 0.37 px a cell,
/// which over [`LINE_CELLS`] cells is well past the right padding.
/// [`a_pango_run_of_the_line_outgrows_the_toast_box`] measures that expectation
/// rather than trusting it. If it fails, choose another size by that precondition;
/// never weaken the assertions it guards.
const FONT_SIZE_PT: f64 = 12.0;

/// Line height handed to `CellMetrics::for_font`, in percent. Only the cell's
/// height depends on it, and nothing here reads a row other than the one line.
const LINE_HEIGHT_PERCENT: f64 = 100.0;

/// The glyph the line repeats.
///
/// Its ink sits inside its own advance on both sides, so ink past a cell's right
/// edge can only come from a glyph drawn somewhere other than its cell.
const LINE_GLYPH: char = 'o';

/// How many cells the line covers, and so how many cells its box is sized for.
const LINE_CELLS: usize = 40;

/// The slack, in pixels, allowed for antialiasing and glyph-origin rounding
/// at a cell's right edge.
const INK_TOLERANCE_PX: usize = 1;

/// White, so any painted pixel has non-zero alpha on the transparent surface.
const INK_FG: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};

/// The alpha byte of an ARGB32 pixel on a little-endian target, whose in-memory
/// order is `B, G, R, A`.
const ALPHA_BYTE: usize = 3;

/// Bytes per ARGB32 pixel.
const BYTES_PER_PIXEL: usize = 4;

/// Measures `CellMetrics` against a throwaway 1x1 surface, the idiom of
/// `paint_golden.rs`'s `probe_metrics` and of `wolf_gnome::app::measure_metrics`.
fn probe_metrics() -> CellMetrics {
    let probe = ImageSurface::create(Format::ARgb32, 1, 1).expect("probe surface");
    let probe_cr = Context::new(&probe).expect("probe context");
    let metrics = CellMetrics::for_font(&probe_cr, FONT_FAMILY, FONT_SIZE_PT, LINE_HEIGHT_PERCENT);
    assert!(
        metrics.cell_w > 0.0 && metrics.cell_h > 0.0,
        "CellMetrics::for_font({FONT_FAMILY:?}, {FONT_SIZE_PT}) gave a non-positive cell \
         ({} x {}). The usual cause is a missing FONTCONFIG_FILE: run inside the devShell.",
        metrics.cell_w,
        metrics.cell_h
    );
    metrics
}

/// The line under test: [`LINE_GLYPH`] repeated [`LINE_CELLS`] times.
fn line_text() -> String {
    LINE_GLYPH.to_string().repeat(LINE_CELLS)
}

/// `value` as an `f64`, without an `as` cast.
fn to_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).expect("a cell or pixel count fits a u32"))
}

/// `value` rounded up to a whole pixel, as `pane_view`'s `px` rounds a surface's
/// content size.
fn ceil_px(value: f64) -> i32 {
    let rounded = value.ceil();
    assert!(
        rounded.is_finite() && rounded > 0.0 && rounded < f64::from(i32::MAX),
        "{value} px is not a usable surface dimension or column"
    );
    rounded as i32
}

/// `value` rounded up to a pixel column index.
fn ceil_col(value: f64) -> usize {
    usize::try_from(ceil_px(value)).expect("ceil_px answers a positive i32")
}

/// `value` rounded down to a pixel column index.
fn floor_col(value: f64) -> usize {
    ceil_col(value) - usize::from(value.fract() != 0.0)
}

/// A transparent ARGB32 surface exactly as big as the box the toast sizes for one
/// line of [`LINE_CELLS`] cells: `cells * cell_w` and one `cell_h`, plus
/// [`SURFACE_PADDING_PX`] on every side.
fn toast_box_surface(metrics: &CellMetrics) -> ImageSurface {
    let width = ceil_px(to_f64(LINE_CELLS).mul_add(metrics.cell_w, 2.0 * SURFACE_PADDING_PX));
    let height = ceil_px(metrics.cell_h + 2.0 * SURFACE_PADDING_PX);
    ImageSurface::create(Format::ARgb32, width, height).expect("toast-box surface")
}

/// The x of the right edge of cell `col` of a line drawn at [`SURFACE_PADDING_PX`].
fn cell_right_edge(metrics: &CellMetrics, col: usize) -> f64 {
    to_f64(col + 1).mul_add(metrics.cell_w, SURFACE_PADDING_PX)
}

/// Whether any pixel of the columns `x0..x1` holds ink, on any row of `surface`.
///
/// The caller must have dropped every `Context` on `surface` and flushed it.
fn columns_hold_ink(surface: &mut ImageSurface, x0: usize, x1: usize) -> bool {
    let height = usize::try_from(surface.height()).expect("a surface height is non-negative");
    let stride = usize::try_from(surface.stride()).expect("a surface stride is non-negative");
    let data = surface
        .data()
        .expect("the surface's pixels, with no context left on it");
    (0..height).any(|y| (x0..x1).any(|x| data[y * stride + x * BYTES_PER_PIXEL + ALPHA_BYTE] != 0))
}

/// The run a single Pango layout gives the line must overrun the toast's box by more
/// than [`INK_TOLERANCE_PX`]: otherwise this file's fixture cannot show a clip and
/// [`draw_cells_keeps_a_line_inside_the_cells_its_toast_is_sized_for`] passes
/// whether `draw_cells` draws on the cells or as one run.
#[test]
fn a_pango_run_of_the_line_outgrows_the_toast_box() {
    let metrics = probe_metrics();
    let surface = toast_box_surface(&metrics);
    let cr = Context::new(&surface).expect("toast-box context");
    let layout = pangocairo::functions::create_layout(&cr);
    layout.set_font_description(Some(&metrics.font));
    layout.set_text(&line_text());
    let run_w = f64::from(layout.size().0) / f64::from(pango::SCALE);

    let cells_w = to_f64(LINE_CELLS) * metrics.cell_w;
    let limit = cells_w + SURFACE_PADDING_PX + to_f64(INK_TOLERANCE_PX);
    assert!(
        run_w > limit,
        "A Pango run of {LINE_CELLS} {LINE_GLYPH:?} at {FONT_SIZE_PT} pt is {run_w} px wide, \
         and does not overrun its {cells_w} px of cells by more than the right padding \
         ({SURFACE_PADDING_PX} px) plus {INK_TOLERANCE_PX} px; cell_w is {}. At this size the \
         face's rounded advance does not outgrow the cell, so the clip test cannot tell a \
         per-cell paint from a run. Choose another FONT_SIZE_PT by this precondition; never \
         weaken the assertions it guards.",
        metrics.cell_w
    );
}

/// Every pixel the line inks lies inside the cells its toast box is sized for, and
/// the line's last cell is inked.
///
/// A line drawn as one Pango run advances by the face's rounded glyph width, and its
/// overrun lands in the right padding past the last cell.
#[test]
fn draw_cells_keeps_a_line_inside_the_cells_its_toast_is_sized_for() {
    let metrics = probe_metrics();
    let mut surface = toast_box_surface(&metrics);
    let cr = Context::new(&surface).expect("toast-box context");
    let covered = draw_cells(
        &cr,
        &metrics,
        SURFACE_PADDING_PX,
        SURFACE_PADDING_PX,
        &line_text(),
        INK_FG,
        &ResolvedStyle::default(),
    );
    drop(cr);
    surface.flush();
    assert_eq!(
        covered, LINE_CELLS,
        "draw_cells reported {covered} cells for {LINE_CELLS} narrow characters"
    );

    let width = usize::try_from(surface.width()).expect("a surface width is non-negative");
    let last_right = cell_right_edge(&metrics, LINE_CELLS - 1);
    let padding_from = ceil_col(last_right) + INK_TOLERANCE_PX;
    assert!(
        padding_from < width,
        "the right padding to check, from column {padding_from}, lies outside the {width} px \
         surface, so the next assertion would check nothing. SURFACE_PADDING_PX is \
         {SURFACE_PADDING_PX}; it must exceed {INK_TOLERANCE_PX} px plus a pixel of rounding."
    );
    assert!(
        !columns_hold_ink(&mut surface, padding_from, width),
        "Ink lies in the right padding, columns {padding_from}..{width}, past the right edge \
         of the last cell at x = {last_right} (cell_w {}). The line was not drawn on its \
         cells: drawn as one Pango run it advances by the face's rounded glyph width, and in a \
         toast its last character is cut off at the box's edge. Check that render::draw_cells \
         draws each cluster at x + col * cell_w.",
        metrics.cell_w
    );

    let last_left = cell_right_edge(&metrics, LINE_CELLS - 2);
    let (cell_from, cell_to) = (ceil_col(last_left), floor_col(last_right));
    assert!(
        columns_hold_ink(&mut surface, cell_from, cell_to),
        "No ink in the line's last cell, columns {cell_from}..{cell_to}, so the padding check \
         above saw a line that was not drawn, or not drawn up to its last cell. draw_cells \
         drew nothing, drew it somewhere else, or left the last character out."
    );
}

/// `draw_cells` returns the grid cells its text covered, which is what the toast
/// advances each chunk by: a wide character is two, a combining mark joins the
/// character before it, and empty text is none and draws nothing.
#[test]
fn draw_cells_returns_the_cells_it_covered() {
    let metrics = probe_metrics();
    let style = ResolvedStyle::default();
    let surface = toast_box_surface(&metrics);
    let cr = Context::new(&surface).expect("toast-box context");
    let at = SURFACE_PADDING_PX;

    let wide = draw_cells(&cr, &metrics, at, at, "ab\u{4e2d}", INK_FG, &style);
    assert_eq!(
        wide, 4,
        "draw_cells(\"ab\\u{{4e2d}}\") reported {wide} cells, not 4. U+4E2D is East Asian Wide \
         and draws over two cells: 3 means it was counted as a character, and a toast chunk \
         after it would be drawn over it."
    );
    let combining = draw_cells(&cr, &metrics, at, at, "e\u{301}", INK_FG, &style);
    assert_eq!(
        combining, 1,
        "draw_cells(\"e\\u{{301}}\") reported {combining} cells, not 1. U+0301 is a combining \
         mark that takes no cell of its own: 2 means it was counted as a character, and a \
         toast chunk after it would start a cell late."
    );
    drop(cr);
    drop(surface);

    let mut blank = toast_box_surface(&metrics);
    let cr = Context::new(&blank).expect("blank toast-box context");
    let empty = draw_cells(&cr, &metrics, at, at, "", INK_FG, &style);
    drop(cr);
    blank.flush();
    assert_eq!(empty, 0, "draw_cells(\"\") reported {empty} cells, not 0");
    let width = usize::try_from(blank.width()).expect("a surface width is non-negative");
    assert!(
        !columns_hold_ink(&mut blank, 0, width),
        "draw_cells(\"\") left ink on the surface; empty text must draw nothing"
    );
}
