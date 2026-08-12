//! Structural golden tests for `wolf_gnome::paint`.
//!
//! Strategy (per plan §5b): the PRIMARY, always-on gate is STRUCTURAL — we paint a
//! hand-built deterministic `Snapshot` onto an offscreen Cairo `ImageSurface`
//! (no window, no display) and then sample raw ARGB bytes at the CENTER of known
//! cell rectangles. Background fills are rasterizer-independent, so we can assert
//! exact equality for them; for glyph cells we only assert that *some* non-bg
//! pixel exists (text was drawn). We deliberately do NOT hash the whole image —
//! that is too brittle across freetype/fontconfig versions.
//!
//! An optional exact-PNG golden can be regenerated via the `#[ignore]`d
//! `regenerate_png_golden` test; it is a dev aid only and never part of the gate.
//!
//! Font determinism relies on the pinned `DejaVu Sans Mono` exposed via
//! `FONTCONFIG_FILE` (set by the flake devShell). The structural asserts here do
//! not depend on exact glyph shapes, only on bg fills + presence of ink.

use cairo::{Context, Format, ImageSurface};

use wolf_core::{Cell, DefaultColors, HlAttr, Rgb, Snapshot};
use wolf_gnome::{paint, CellMetrics};

// NOTE to coder: these tests read `metrics.cell_w` and `metrics.cell_h` directly,
// so the `cell_w`/`cell_h` fields of `CellMetrics` must be `pub`. If you choose to
// keep the fields private, expose `pub fn cell_w(&self) -> f64` / `cell_h(&self)`
// accessors instead and adjust the two `cell_*` reads below — do not change the
// painting contract to fit the test.

// --- Deterministic test fixture dimensions / font ---------------------------

/// Grid columns in the fixture snapshot.
const COLS: usize = 4;
/// Grid rows in the fixture snapshot.
const ROWS: usize = 2;

/// Pinned font family for deterministic measurement/rasterization.
const FONT_FAMILY: &str = "DejaVu Sans Mono";
/// Pinned font size, in points.
const FONT_SIZE_PT: f64 = 16.0;
/// Line-height handed to `CellMetrics::for_font`, as a percentage of the font
/// size. This is the value `wolf_core::default_config()` ships, so the fixture
/// exercises the path a user gets rather than a special case; at 16 pt it makes
/// the cell taller than the glyph box, which is what gives
/// [`the_baseline_centres_the_glyph_box_in_the_cell`] something to measure.
const LINE_HEIGHT_PERCENT: f64 = 130.0;

/// Highlight id carrying a distinct fg/bg (anything non-zero; 0 is the default).
const DEFINED_HL_ID: u64 = 7;

/// Opaque alpha byte for an ARGB32 pixel.
const OPAQUE: u8 = 255;

// --- Fixture colors (kept maximally distinct for unambiguous sampling) ------

/// Default foreground (white).
const DEFAULT_FG: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};
/// Default background (black).
const DEFAULT_BG: Rgb = Rgb { r: 0, g: 0, b: 0 };
/// Defined-highlight foreground (red).
const DEFINED_FG: Rgb = Rgb { r: 255, g: 0, b: 0 };
/// Defined-highlight background (green) — the "known bg" we sample for exactness.
const DEFINED_BG: Rgb = Rgb { r: 0, g: 255, b: 0 };

// --- Cell coordinates in the fixture ----------------------------------------

/// Row holding the painted content.
const CONTENT_ROW: usize = 0;
/// A normal glyph cell using the defined highlight (distinct fg/bg).
const GLYPH_CELL_COL: usize = 2;
/// A wide-trailing / empty-text cell (`text == ""`) using the defined highlight;
/// its rect must be background-only (the "skip the glyph" path).
const EMPTY_CELL_COL: usize = 3;
/// A fully-default empty cell on the second row, sampled to confirm the default
/// background fill path.
const DEFAULT_BG_ROW: usize = 1;
const DEFAULT_BG_COL: usize = 0;

/// Builds the deterministic fixture snapshot:
///   row 0: "A" "B" (hl 0), "C" (defined hl, distinct fg/bg), "" (defined hl, wide-trailing)
///   row 1: all default/empty cells
fn fixture_snapshot() -> Snapshot {
    let mut snap = Snapshot::new(COLS, ROWS);

    snap.highlights.set_defaults(DefaultColors {
        fg: Some(DEFAULT_FG),
        bg: Some(DEFAULT_BG),
        special: None,
    });
    snap.highlights.define(
        DEFINED_HL_ID,
        HlAttr {
            fg: Some(DEFINED_FG),
            bg: Some(DEFINED_BG),
            ..HlAttr::default()
        },
    );

    snap.grid_mut(1).set_cell(
        CONTENT_ROW,
        0,
        Cell {
            text: "A".to_string(),
            hl_id: 0,
        },
    );
    snap.grid_mut(1).set_cell(
        CONTENT_ROW,
        1,
        Cell {
            text: "B".to_string(),
            hl_id: 0,
        },
    );
    snap.grid_mut(1).set_cell(
        CONTENT_ROW,
        GLYPH_CELL_COL,
        Cell {
            text: "C".to_string(),
            hl_id: DEFINED_HL_ID,
        },
    );
    // Wide-trailing placeholder: empty text, but still carries a highlight so its
    // background is filled. paint() must skip glyph rendering for empty text.
    snap.grid_mut(1).set_cell(
        CONTENT_ROW,
        EMPTY_CELL_COL,
        Cell {
            text: String::new(),
            hl_id: DEFINED_HL_ID,
        },
    );

    snap
}

/// Measures `CellMetrics` for one family/size/line-height against a throwaway 1x1
/// surface.
///
/// The probe surface's pixel size is irrelevant — it exists only to obtain a Pango
/// context — which is the same idiom `wolf_gnome::app::measure_metrics` and
/// `tests/smoke_headless.rs::cell_metrics_for_smoke` use, so all three measure the
/// same way.
fn probe_metrics(family: &str, size_pt: f64, line_height_percent: f64) -> CellMetrics {
    let probe = ImageSurface::create(Format::ARgb32, 1, 1).expect("probe surface");
    let probe_cr = Context::new(&probe).expect("probe context");
    CellMetrics::for_font(&probe_cr, family, size_pt, line_height_percent)
}

/// Measures `CellMetrics` against a throwaway 1x1 surface, then sizes the real
/// surface to exactly `COLS x ROWS` cells and paints the fixture onto it.
///
/// Returns the painted surface plus the metrics so the caller can compute cell
/// rectangles. The surface is returned `mut` so the caller can borrow its data.
fn render_fixture() -> (ImageSurface, CellMetrics) {
    let metrics = probe_metrics(FONT_FAMILY, FONT_SIZE_PT, LINE_HEIGHT_PERCENT);

    let width = (metrics.cell_w * COLS as f64).ceil() as i32;
    let height = (metrics.cell_h * ROWS as f64).ceil() as i32;
    assert!(
        width > 0 && height > 0,
        "CellMetrics produced a non-positive cell size: cell_w={}, cell_h={}",
        metrics.cell_w,
        metrics.cell_h
    );

    let surface = ImageSurface::create(Format::ARgb32, width, height).expect("render surface");
    let cr = Context::new(&surface).expect("render context");

    let snap = fixture_snapshot();
    paint(&cr, snap.active_grid(), &snap.highlights, &metrics);

    // Drop the context before reading pixels: cairo wants the surface free of
    // outstanding references when we borrow its data.
    drop(cr);
    surface.flush();

    (surface, metrics)
}

/// A pixel's color as `(r, g, b, a)`.
type Pixel = (u8, u8, u8, u8);

/// Reads the pixel at `(x, y)` from ARGB32 surface bytes.
///
/// ARGB32 is stored in native-endian premultiplied form; on the little-endian
/// targets we run on (x86_64 Linux) the in-memory byte order per pixel is
/// `B, G, R, A`. Opaque fills are unaffected by premultiplication.
///
/// NOTE to coder: if this suite is ever run on a big-endian target, this byte
/// ordering must be flipped. We assert little-endian to keep the comparison exact.
fn pixel_at(data: &[u8], stride: usize, x: usize, y: usize) -> Pixel {
    let i = y * stride + x * 4;
    let b = data[i];
    let g = data[i + 1];
    let r = data[i + 2];
    let a = data[i + 3];
    (r, g, b, a)
}

/// Center pixel coordinate of cell `(row, col)`.
fn cell_center(metrics: &CellMetrics, row: usize, col: usize) -> (usize, usize) {
    let x = ((col as f64 + 0.5) * metrics.cell_w) as usize;
    let y = ((row as f64 + 0.5) * metrics.cell_h) as usize;
    (x, y)
}

#[test]
fn paint_fills_known_background_exactly() {
    let (mut surface, metrics) = render_fixture();

    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");

    // (a) The defined-highlight EMPTY cell has no glyph, so its center is a clean,
    // exact background fill of the defined bg color (green), fully opaque.
    let (x, y) = cell_center(&metrics, CONTENT_ROW, EMPTY_CELL_COL);
    let px = pixel_at(&data, stride, x, y);
    assert_eq!(
        px,
        (DEFINED_BG.r, DEFINED_BG.g, DEFINED_BG.b, OPAQUE),
        "defined-bg empty cell center should equal the exact defined background"
    );

    // The default empty cell (row 1) confirms the default-bg fill path.
    let (dx, dy) = cell_center(&metrics, DEFAULT_BG_ROW, DEFAULT_BG_COL);
    let dpx = pixel_at(&data, stride, dx, dy);
    assert_eq!(
        dpx,
        (DEFAULT_BG.r, DEFAULT_BG.g, DEFAULT_BG.b, OPAQUE),
        "default empty cell center should equal the exact default background"
    );
}

#[test]
fn paint_skips_glyph_for_empty_wide_trailing_cell() {
    let (mut surface, metrics) = render_fixture();

    let width = surface.width() as usize;
    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");

    // (b) Sample several INTERIOR points of the empty cell, inset from its edges
    // so that antialiasing from the neighboring glyph ('C') cannot bleed in. Every
    // sampled interior point must be the pure background (no ink => glyph skipped).
    let fractions = [
        (0.5_f64, 0.5_f64),
        (0.5, 0.3),
        (0.5, 0.7),
        (0.4, 0.5),
        (0.6, 0.5),
    ];
    for (fx, fy) in fractions {
        let x = (((EMPTY_CELL_COL as f64) + fx) * metrics.cell_w) as usize;
        let y = (((CONTENT_ROW as f64) + fy) * metrics.cell_h) as usize;
        let x = x.min(width - 1);
        let y = y.min(height - 1);
        let px = pixel_at(&data, stride, x, y);
        assert_eq!(
            px,
            (DEFINED_BG.r, DEFINED_BG.g, DEFINED_BG.b, OPAQUE),
            "empty wide-trailing cell interior at ({x},{y}) must be background-only"
        );
    }
}

#[test]
fn paint_draws_ink_for_glyph_cell() {
    let (mut surface, metrics) = render_fixture();

    let width = surface.width() as usize;
    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");

    // (c) The glyph cell ('C') must contain at least one pixel that differs from
    // its background — i.e. the glyph was actually rendered. Scan the cell's rect.
    let bg = (DEFINED_BG.r, DEFINED_BG.g, DEFINED_BG.b, OPAQUE);
    let x0 = (GLYPH_CELL_COL as f64 * metrics.cell_w) as usize;
    let x1 = (((GLYPH_CELL_COL + 1) as f64 * metrics.cell_w) as usize).min(width);
    let y0 = (CONTENT_ROW as f64 * metrics.cell_h) as usize;
    let y1 = (((CONTENT_ROW + 1) as f64 * metrics.cell_h) as usize).min(height);

    let mut found_ink = false;
    'scan: for y in y0..y1 {
        for x in x0..x1 {
            if pixel_at(&data, stride, x, y) != bg {
                found_ink = true;
                break 'scan;
            }
        }
    }
    assert!(
        found_ink,
        "glyph cell rect ({x0}..{x1}, {y0}..{y1}) should contain non-background \
         ink (the rendered glyph)"
    );
}

// --- Cell metrics: what the configured line-height and family actually do ----

/// A family fontconfig provably cannot resolve.
///
/// The devShell's generated `FONTCONFIG_FILE` exposes exactly three pinned font
/// trees (DejaVu, JetBrains Mono, iA Writer Mono S) and no host font directory, so
/// no installed font can carry this name. The hex suffix is there so the string
/// cannot collide with a family someone adds later.
const NO_SUCH_FAMILY: &str = "Wolf No Such Family 8f3a1c";

/// The point size the two family-resolution and dpi cases below probe at.
///
/// It is the smokes' `WOLF_FONT_PT`, which is where the measured numbers in
/// [`the_measured_cell_height_matches_the_96_dpi_arithmetic`] come from.
const SMOKE_PROBE_SIZE_PT: f64 = 28.0;

/// The line-height at which `round(28 * 96 / 72 * p / 100)` reproduces the
/// measured 28 pt glyph box exactly.
const SMOKE_PROBE_LINE_HEIGHT: f64 = 118.0;

/// The cell height MEASURED for [`FONT_FAMILY`] at [`SMOKE_PROBE_SIZE_PT`] and
/// [`SMOKE_PROBE_LINE_HEIGHT`] on this repo's pinned font stack, in device pixels.
///
/// Stated as a literal on purpose. Writing it as
/// `(28.0 * 96.0 / 72.0 * 118.0 / 100.0).round()` would restate the
/// implementation's own arithmetic and would then pass against whatever dpi
/// `for_font` happened to use — which is exactly the value under test.
const SMOKE_PROBE_CELL_H: f64 = 44.0;

/// The line-height that selects `cell_metrics_math::cell_height`'s natural-metrics
/// branch, which is any value that is not positive.
///
/// `wolf_core::load_or_create` does not validate the key, so a hand-written
/// `line-height: 0` in a user's `wolf-config.yaml` arrives at `for_font` unchanged.
const NATURAL_LINE_HEIGHT_PERCENT: f64 = 0.0;

/// The cell height MEASURED for [`FONT_FAMILY`] at [`FONT_SIZE_PT`] and
/// [`NATURAL_LINE_HEIGHT_PERCENT`] on this repo's pinned font stack, in device
/// pixels.
///
/// A literal, and never `ceil(ascent + descent + leading)` or any other restatement
/// of the arithmetic under test. A re-derived expectation moves with the code it
/// checks: written that way, this one holds even with the `leading` clamp deleted,
/// which is the single thing
/// [`the_natural_line_height_clamps_a_face_whose_recommended_height_is_short`] is
/// here to catch.
const CLAMPED_NATURAL_CELL_H: f64 = 26.0;

#[test]
fn a_larger_line_height_yields_a_taller_cell() {
    let small = probe_metrics(FONT_FAMILY, FONT_SIZE_PT, 100.0);
    let large = probe_metrics(FONT_FAMILY, FONT_SIZE_PT, 160.0);

    assert!(
        large.cell_h > small.cell_h,
        "cell height at 160% ({}) is not greater than at 100% ({}). Equal heights mean \
         `for_font` ignores its `line_height_percent` argument entirely — the shape the \
         pre-config code had, where `cell_h` was `ascent + descent` — so the configured \
         line-height reaches no pixel at all.",
        large.cell_h,
        small.cell_h
    );

    // 160% must be 1.6x the 100% cell, to within the rounding to a whole device
    // pixel that both calls perform. Without this the assertion above would be
    // satisfied by any monotonic response, including one that ignored the
    // magnitude and merely added a constant.
    let expected = small.cell_h * 1.6;
    let drift = (large.cell_h - expected).abs();
    assert!(
        drift <= 1.0,
        "cell height at 160% is {}, which is {drift} away from 1.6x the 100% height ({expected} = \
         {} * 1.6). Only whole-device-pixel rounding may separate them, so a drift above 1.0 means \
         the percentage is being applied to something other than the font size.",
        large.cell_h,
        small.cell_h
    );
}

#[test]
fn the_advance_width_is_independent_of_the_line_height() {
    let small = probe_metrics(FONT_FAMILY, FONT_SIZE_PT, 100.0);
    let large = probe_metrics(FONT_FAMILY, FONT_SIZE_PT, 160.0);

    // Bit-equal, not approximately equal: both come from the same
    // `approximate_char_width` for the same face at the same size, so any
    // difference at all means the width was derived from the height.
    assert_eq!(
        small.cell_w, large.cell_w,
        "cell width moved with the line-height ({} at 100% vs {} at 160%). Line-height is a \
         VERTICAL rule; a width that tracks it means `cell_w` was made a function of `cell_h`, \
         which silently re-columns every pane and moves every pointer smoke's pixel targets.",
        small.cell_w, large.cell_w
    );
}

#[test]
fn the_baseline_centres_the_glyph_box_in_the_cell() {
    let m = probe_metrics(FONT_FAMILY, FONT_SIZE_PT, LINE_HEIGHT_PERCENT);

    // Fixture precondition. With a cell exactly as tall as the glyph box the
    // leftover is zero, the baseline IS the ascent, and the assertion below holds
    // for the pre-config `baseline: ascent` too — a vacuous pass.
    assert!(
        m.cell_h > m.ascent + m.descent,
        "fixture precondition: at {FONT_SIZE_PT} pt and {LINE_HEIGHT_PERCENT}% the cell ({}) must \
         be TALLER than the glyph box (ascent {} + descent {} = {}), or there is no leftover space \
         to split and this case cannot tell the centring rule from `baseline = ascent`. If the \
         pinned font's metrics moved, raise LINE_HEIGHT_PERCENT for this case rather than dropping \
         the assertion.",
        m.cell_h,
        m.ascent,
        m.descent,
        m.ascent + m.descent
    );

    let leftover_above = (m.cell_h - m.ascent - m.descent) / 2.0;
    assert!(
        (m.baseline - m.ascent - leftover_above).abs() < 1e-9,
        "the baseline is {} with ascent {}, descent {} and cell height {}. It must be \
         `ascent + (cell_h - ascent - descent) / 2` = {}, so the glyph box sits centred with \
         {leftover_above} px above and the same below. A baseline equal to the ascent ({}) is the \
         pre-config value, and with it `draw_glyph` paints every glyph flush against the cell top \
         inside a taller cell.",
        m.baseline,
        m.ascent,
        m.descent,
        m.cell_h,
        m.ascent + leftover_above,
        m.ascent
    );
}

#[test]
fn an_unresolvable_family_still_measures_and_is_not_rewritten() {
    let missing = probe_metrics(NO_SUCH_FAMILY, SMOKE_PROBE_SIZE_PT, LINE_HEIGHT_PERCENT);

    // (a) Pango substitutes rather than failing. There is no error path and no
    // zero-metrics path to detect, which is the premise the crate's
    // no-fallback-of-our-own decision rests on — so it is measured here rather
    // than assumed.
    assert!(
        missing.cell_w > 0.0 && missing.cell_h > 0.0 && missing.baseline.is_finite(),
        "measuring the unresolvable family {NO_SUCH_FAMILY:?} produced cell_w = {}, cell_h = {}, \
         baseline = {}. Pango is expected to substitute a face from the pinned fontconfig and \
         always yield usable metrics; a zero or non-finite value here means it does not, and the \
         crate would then need the explicit fallback it deliberately does not have.",
        missing.cell_w,
        missing.cell_h,
        missing.baseline
    );

    // (b) With a positive line-height the cell height is a function of the font
    // size alone, so it cannot depend on which face was substituted.
    let pinned = probe_metrics(FONT_FAMILY, SMOKE_PROBE_SIZE_PT, LINE_HEIGHT_PERCENT);
    assert_eq!(
        missing.cell_h, pinned.cell_h,
        "the unresolvable family measured cell_h = {} while {FONT_FAMILY:?} measured {}. At \
         {LINE_HEIGHT_PERCENT}% the cell height is `round(size_px * percent / 100)` and reads no \
         font metric, so the two MUST agree bit for bit. A difference means the positive path \
         fell back to `ascent + descent`, and the row height would then change whenever \
         fontconfig's substitution changed.",
        missing.cell_h, pinned.cell_h
    );

    // (c) This crate performs no family substitution of its own: the description
    // still carries exactly what the caller asked for.
    let requested = missing.font.family();
    assert_eq!(
        requested.as_deref(),
        Some(NO_SUCH_FAMILY),
        "the measured font description reports family {requested:?}, not the requested \
         {NO_SUCH_FAMILY:?}. Rewriting the family — a macOS-style `makeFont` fallback to \
         `DejaVu Sans Mono` or to `monospace` — is the change this crate refuses: CoreText returns \
         a DIFFERENT family and so must be checked, while Pango substitutes per lookup and always \
         yields usable metrics, so a hand-rolled fallback here is dead code that can only override \
         fontconfig's better answer."
    );
}

#[test]
fn the_measured_cell_height_matches_the_96_dpi_arithmetic() {
    // `pangocairo::functions::context_get_resolution` returns -1.0 on a context
    // created over an `ImageSurface` — measured on this host, not inferred — so
    // `for_font`'s 96.0 dpi arm is the LIVE path on every headless run, not a
    // defensive guard. Nothing else in the suite pins it: every other cell-metric
    // assertion here is relative (taller than, equal to, centred within) and would
    // hold at any dpi applied consistently.
    //
    // 44.0 is MEASURED for the pinned DejaVu Sans Mono at 28 pt and 118%, and is
    // also that face's `ascent + descent` at that size. A change to it means one of
    // two things and they need different fixes: the dpi fallback stopped firing (a
    // context that reports a real resolution, or a changed fallback condition), or
    // the pinned font moved (a different `FONTCONFIG_FILE`, or a nixpkgs bump of
    // `dejavu_fonts`). Re-measure before touching the literal.
    let m = probe_metrics(FONT_FAMILY, SMOKE_PROBE_SIZE_PT, SMOKE_PROBE_LINE_HEIGHT);

    assert_eq!(
        m.cell_h, SMOKE_PROBE_CELL_H,
        "{FONT_FAMILY:?} at {SMOKE_PROBE_SIZE_PT} pt and {SMOKE_PROBE_LINE_HEIGHT}% measured \
         cell_h = {}, not the measured {SMOKE_PROBE_CELL_H}. At 96 dpi that is \
         round(28 * 96/72 * 1.18) = round(44.05); a value near 33 means 72 dpi (the fallback \
         stopped firing and the context reported its own resolution), and any other value means \
         the substituted face changed. Every compositor smoke's cell geometry rides on this \
         number.",
        m.cell_h
    );

    // The property that makes the smoke suite's pixel constants hold: with the cell
    // exactly as tall as the glyph box, `baseline - ascent` is 0.0 and
    // `draw_glyph`'s offset moves no glyph at all.
    assert_eq!(
        m.baseline, m.ascent,
        "at the smokes' font size and line-height the cell is exactly the glyph box, so the \
         baseline ({}) must equal the ascent ({}) and `draw_glyph`'s `baseline - ascent` offset \
         must be exactly 0.0. A non-zero offset shifts every glyph vertically, and every OCR and \
         pointer smoke is calibrated against the unshifted position.",
        m.baseline, m.ascent
    );
}

/// `for_font`'s clamp on `leading` is what keeps a face whose RECOMMENDED line
/// height is shorter than its own glyph box from shrinking the natural cell.
///
/// Measured on this host, twice and independently, under the devShell's pinned
/// `FONTCONFIG_FILE`: [`FONT_FAMILY`] at [`FONT_SIZE_PT`] pt reports an ascent of
/// 20, a descent of 6 and a `metrics.height() / SCALE` of 25, so the raw
/// `height - ascent - descent` is `-1`. The `.max(0.0)` in `render.rs` turns that
/// into a `leading` of `0.0`, and the natural branch then gives
/// `ceil(20 + 6 + 0) == 26` — the glyph box — where the unclamped `-1` gives 25.
/// This repo's pinned font is a unit short of its own glyph box at this size, so
/// the clamp is a live case rather than a hypothetical one.
///
/// This is the only case that reaches `cell_metrics_math::cell_height`'s
/// non-positive branch THROUGH `for_font`. That module's own unit tests call
/// `cell_height` directly with a hand-written `leading`, and the clamp sits between
/// the real Pango metrics and that argument — so a synthetic fixture passes over it
/// and no value one could hand in makes it observable. Only a case that measures a
/// real face crosses it, which is why this one lives here and not in that module's
/// `#[cfg(test)] mod tests`.
#[test]
fn the_natural_line_height_clamps_a_face_whose_recommended_height_is_short() {
    let m = probe_metrics(FONT_FAMILY, FONT_SIZE_PT, NATURAL_LINE_HEIGHT_PERCENT);

    assert_eq!(
        m.cell_h, CLAMPED_NATURAL_CELL_H,
        "{FONT_FAMILY:?} at {FONT_SIZE_PT} pt with `line-height: {NATURAL_LINE_HEIGHT_PERCENT}` \
         measures cell_h = {}, not the measured {CLAMPED_NATURAL_CELL_H}. This face reports a \
         recommended height of 25 against an ascent + descent of 26 at this size, so the \
         `.max(0.0)` clamp on `leading` in `render.rs`'s `for_font` is the only thing standing \
         between those two numbers. A 25 means that clamp is gone and the negative leading reached \
         `cell_height`; a 1 means the non-positive branch is gone, so `round(21.33 * 0 / 100)` ran \
         instead and the degenerate floor caught its 0; any other value means the substituted face \
         or the 96 dpi conversion moved, and the literal is to be re-measured rather than adjusted.",
        m.cell_h
    );
}

// --- Text decorations: undercurl, strikethrough and the `special` colour -----
//
// A SECOND fixture and a second render helper, deliberately independent of
// `fixture_snapshot` / `render_fixture`: the five cases above are calibrated
// against a 4x2 grid at 16 pt, and widening that grid to carry decorations would
// put every one of them at risk for nothing. Nothing below reads or modifies the
// first fixture.

/// Columns in the decoration fixture — control, underline, undercurl,
/// strikethrough, undercurl-without-special, the leak source, and the trailing
/// control.
const DECORATION_COLS: usize = 7;
/// Rows in the decoration fixture.
const DECORATION_ROWS: usize = 1;
/// The single row the decoration fixture paints into.
///
/// The fixture is exactly [`DECORATION_ROWS`] rows tall, so this row's top edge is
/// `y == 0` and its bottom edge is `cell_h`. The row-range helpers below rely on
/// that and fold in no row origin; a second row would have to.
const DECORATION_ROW: usize = 0;

/// The glyph every decoration cell carries.
///
/// `x` has no descender, so every pixel the GLYPH contributes sits above the
/// baseline and anything found below it is decoration. Its name is not the value
/// any assertion here reads — the assertions read colours and ink counts — so it
/// cannot satisfy one by itself.
const DECORATION_GLYPH: &str = "x";

/// The undecorated control cell.
const CONTROL_COL: usize = 0;
/// A plain underline, with `special` set.
const UNDERLINE_COL: usize = 1;
/// An undercurl, with `special` set.
const UNDERCURL_COL: usize = 2;
/// A strikethrough, with `special` set.
const STRIKETHROUGH_COL: usize = 3;
/// An undercurl with `special` UNSET, so its decoration must fall back to the
/// layout foreground.
const UNDERCURL_NO_SPECIAL_COL: usize = 4;
/// A second plain underline with `special` set, carrying [`UNDERLINE_HL_ID`]
/// again.
///
/// Its only job is to be the LAST decorated cell of the row, so that whatever
/// [`TRAILING_CONTROL_COL`] could inherit is drawn in `special` and can be told
/// from glyph ink by colour alone. Without it the last decorated cell would be
/// [`UNDERCURL_NO_SPECIAL_COL`], whose decoration takes the layout foreground —
/// the same white as the glyph — leaving [`reddish_count`] with nothing to say and
/// the case resting on an ink count alone.
const LEAK_SOURCE_COL: usize = 5;
/// An undecorated cell painted AFTER every decorated one.
///
/// [`CONTROL_COL`] is at column 0, before every decoration, and that is why it
/// cannot stand in for this cell: `paint` walks a row left to right through ONE
/// `GlyphPainter`, so a decoration that leaks forward lands only on cells that are
/// decorated anyway. Measured by the coder: with the layout's attribute clear
/// deleted, every case in this file stayed green.
const TRAILING_CONTROL_COL: usize = 6;

/// Highlight id for [`UNDERLINE_COL`].
const UNDERLINE_HL_ID: u64 = 11;
/// Highlight id for [`UNDERCURL_COL`].
const UNDERCURL_HL_ID: u64 = 12;
/// Highlight id for [`STRIKETHROUGH_COL`].
const STRIKETHROUGH_HL_ID: u64 = 13;
/// Highlight id for [`UNDERCURL_NO_SPECIAL_COL`].
const UNDERCURL_NO_SPECIAL_HL_ID: u64 = 14;

/// The `special` colour the three decorated ids carry: pure red.
///
/// Red because the fixture's foreground and background are white and black, which
/// are both grey — no blend of them can ever satisfy [`is_reddish`], so a reddish
/// pixel is evidence of `special` reaching a decoration and of nothing else. The
/// editor defaults deliberately carry `special: None`
/// ([`decoration_fixture_snapshot`]), since `HighlightTable::resolve` folds the
/// default `special` into every style and a default red would put this colour in
/// cells that never asked for it.
const DECORATION_SPECIAL: Rgb = Rgb { r: 255, g: 0, b: 0 };

/// How far a pixel's red channel must exceed BOTH its green and its blue for
/// [`is_reddish`] to count it as carrying [`DECORATION_SPECIAL`].
///
/// Wide enough that no antialiased blend of the fixture's white ink over its
/// black background — where all three channels move together — can pass, and low
/// enough that a heavily blended red edge still does.
const REDDISH_MARGIN: u8 = 40;

/// Horizontal inset, in device pixels, applied to every sampled column.
///
/// A cell origin is `col * cell_w` with a FRACTIONAL `cell_w`, so each column
/// starts at a different sub-pixel phase and the pixel at a cell's edge is
/// partially covered by whatever the neighbouring cell drew. Two pixels of inset
/// puts every sample fully inside one cell, which is what lets
/// [`undercurl_and_underline_do_not_draw_the_same_thing`] compare two columns
/// pixel for pixel at all: without it, two IDENTICAL decorations in two different
/// columns would still differ at their edges and that comparison would pass
/// against the very revert it exists to catch.
const BAND_SIDE_INSET_PX: usize = 2;

/// Vertical inset applied to a whole-cell sample, so a neighbouring row's
/// antialiasing cannot bleed into an ink count.
const INTERIOR_INSET_PX: usize = 1;

/// Guard, in device pixels, between the baseline and the top of the sub-baseline
/// band.
///
/// The glyph's bottom edge sits ON the baseline, which falls inside a pixel row
/// rather than between two, so that row can carry antialiased GLYPH ink. One row
/// of guard keeps it out of a band whose whole purpose is to contain nothing but
/// decoration.
const SUB_BASELINE_GUARD_PX: f64 = 1.0;

/// The fewest rows the sub-baseline band may have for the decoration cases to
/// mean anything.
///
/// Derived, not guessed: at [`SMOKE_PROBE_SIZE_PT`] the font size is
/// `28 * 96/72 == 37.33` device pixels, so [`LINE_HEIGHT_PERCENT`] gives
/// `round(37.33 * 1.3) == 49`; the pinned DejaVu Sans Mono measures ascent 35 and
/// descent 9 at that size (the table in `src/cell_metrics_math.rs`'s own docs), so
/// the baseline centres at `35 + (49 - 44)/2 == 37.5` and roughly ten rows remain
/// below it once [`SUB_BASELINE_GUARD_PX`] is spent. That derivation rests on
/// measurements taken elsewhere, which is why [`render_decoration_fixture`]
/// asserts it rather than assuming it.
const MIN_SUB_BASELINE_ROWS: usize = 4;

/// The fewest device pixels wide a sampled band may be.
const MIN_BAND_WIDTH_PX: usize = 8;

/// The fewest pixels of ink a strikethrough must add over the undecorated
/// control.
///
/// A strike spans the run's whole advance — some twenty device pixels at this
/// font size — so a margin an order of magnitude above antialiasing noise still
/// leaves the case unambiguous. A bare `>` would be satisfied by a single stray
/// blend pixel and would pass against a build that draws no strike at all.
const MIN_STRIKE_INK_PX: usize = 10;

/// The fewest device pixels of vertical spread the undercurl's top edge must show
/// across the cell for it to be a wave rather than a straight line.
///
/// Two, chosen between two measurements rather than picked: the wave's measured
/// top-edge spread is exactly 3 device pixels, rows 37 to 40 across all 18 sampled
/// columns (see [`the_undercurl_is_wavy_rather_than_straight`]), and a horizontal
/// rectangle — which is what `Underline::Single` draws — measures exactly 0, since
/// every column of a rectangle has the same top edge. That leaves one pixel of
/// headroom under the measurement and two above the value the revert produces.
const MIN_WAVE_SPREAD_PX: usize = 2;

/// The fewest of the wave band's columns that must carry the wave's own colour
/// before a spread measured over them describes a shape.
///
/// A spread over two or three columns would be a statement about antialiasing.
const MIN_WAVE_SAMPLED_COLUMNS: usize = 8;

/// Builds the decoration fixture: one row of seven cells, all carrying
/// [`DECORATION_GLYPH`] on white-on-black, differing only in their decoration.
///
/// The row ends decorated-then-undecorated ([`LEAK_SOURCE_COL`] then
/// [`TRAILING_CONTROL_COL`]) because the painter carries state between cells: that
/// pair is the only place in this fixture where an undecorated cell is drawn
/// through a layout a decorated one has already configured.
///
/// The editor defaults set `special: None` on purpose. `HighlightTable::resolve`
/// folds the default `special` into EVERY resolved style, so a default red would
/// give [`UNDERCURL_NO_SPECIAL_COL`] a `special` after all and quietly destroy the
/// one negative case that proves the decoration colour comes from `special`.
fn decoration_fixture_snapshot() -> Snapshot {
    let mut snap = Snapshot::new(DECORATION_COLS, DECORATION_ROWS);

    snap.highlights.set_defaults(DefaultColors {
        fg: Some(DEFAULT_FG),
        bg: Some(DEFAULT_BG),
        special: None,
    });
    snap.highlights.define(
        UNDERLINE_HL_ID,
        HlAttr {
            special: Some(DECORATION_SPECIAL),
            underline: true,
            ..HlAttr::default()
        },
    );
    snap.highlights.define(
        UNDERCURL_HL_ID,
        HlAttr {
            special: Some(DECORATION_SPECIAL),
            undercurl: true,
            ..HlAttr::default()
        },
    );
    snap.highlights.define(
        STRIKETHROUGH_HL_ID,
        HlAttr {
            special: Some(DECORATION_SPECIAL),
            strikethrough: true,
            ..HlAttr::default()
        },
    );
    snap.highlights.define(
        UNDERCURL_NO_SPECIAL_HL_ID,
        HlAttr {
            undercurl: true,
            ..HlAttr::default()
        },
    );

    let ids = [
        (CONTROL_COL, 0),
        (UNDERLINE_COL, UNDERLINE_HL_ID),
        (UNDERCURL_COL, UNDERCURL_HL_ID),
        (STRIKETHROUGH_COL, STRIKETHROUGH_HL_ID),
        (UNDERCURL_NO_SPECIAL_COL, UNDERCURL_NO_SPECIAL_HL_ID),
        (LEAK_SOURCE_COL, UNDERLINE_HL_ID),
        (TRAILING_CONTROL_COL, 0),
    ];
    for (col, hl_id) in ids {
        snap.grid_mut(1).set_cell(
            DECORATION_ROW,
            col,
            Cell {
                text: DECORATION_GLYPH.to_string(),
                hl_id,
            },
        );
    }

    snap
}

/// Measures metrics at [`SMOKE_PROBE_SIZE_PT`] and [`LINE_HEIGHT_PERCENT`], sizes
/// a surface to exactly `DECORATION_COLS x DECORATION_ROWS` cells, and paints the
/// decoration fixture onto it.
///
/// The smokes' 28 pt rather than this file's 16 pt, and the shipped 130 % rather
/// than the smokes' 118 %: the decorations need a cell TALLER than the glyph box,
/// so there is room below the baseline for a wave to be a wave. The two
/// preconditions at the end are what keep that a measured property of the pinned
/// font stack rather than an assumption; both name what to fix if they fire.
fn render_decoration_fixture() -> (ImageSurface, CellMetrics) {
    let metrics = probe_metrics(FONT_FAMILY, SMOKE_PROBE_SIZE_PT, LINE_HEIGHT_PERCENT);

    let width = (metrics.cell_w * DECORATION_COLS as f64).ceil() as i32;
    let height = (metrics.cell_h * DECORATION_ROWS as f64).ceil() as i32;
    assert!(
        width > 0 && height > 0,
        "CellMetrics produced a non-positive cell size for the decoration fixture: cell_w={}, \
         cell_h={}",
        metrics.cell_w,
        metrics.cell_h
    );

    let surface =
        ImageSurface::create(Format::ARgb32, width, height).expect("decoration render surface");
    let cr = Context::new(&surface).expect("decoration render context");

    let snap = decoration_fixture_snapshot();
    paint(&cr, snap.active_grid(), &snap.highlights, &metrics);

    // Drop the context before any caller reads pixels, exactly as `render_fixture`
    // does.
    drop(cr);
    surface.flush();

    let height = height as usize;
    let (y0, y1) = sub_baseline_rows(&metrics, height);
    assert!(
        y1.saturating_sub(y0) >= MIN_SUB_BASELINE_ROWS,
        "fixture precondition: the band below the baseline is rows {y0}..{y1} of a {height}px \
         surface, fewer than the {MIN_SUB_BASELINE_ROWS} rows every decoration case reads. At \
         {SMOKE_PROBE_SIZE_PT} pt and {LINE_HEIGHT_PERCENT}% the cell ({}) must stand clear of the \
         baseline ({}) by more than that. If the pinned font's metrics moved, RAISE \
         LINE_HEIGHT_PERCENT for this fixture rather than shrinking the guard — a band that \
         reaches up into the glyph box makes every case below read glyph ink as decoration.",
        metrics.cell_h,
        metrics.baseline
    );
    assert!(
        band_width(&metrics) >= MIN_BAND_WIDTH_PX,
        "fixture precondition: a sampled band is {} px wide, under the {MIN_BAND_WIDTH_PX} px \
         these cases need. The cell advance is {} px and {BAND_SIDE_INSET_PX} px are inset from \
         each side; a band this narrow means the measured advance collapsed.",
        band_width(&metrics),
        metrics.cell_w
    );

    (surface, metrics)
}

/// The exact background pixel every decoration cell is filled with.
fn decoration_background() -> Pixel {
    (DEFAULT_BG.r, DEFAULT_BG.g, DEFAULT_BG.b, OPAQUE)
}

/// Whether `px` carries the fixture's `special` colour rather than its greyscale
/// foreground or background.
fn is_reddish(px: Pixel) -> bool {
    let (r, g, b, _a) = px;
    r > g.saturating_add(REDDISH_MARGIN) && r > b.saturating_add(REDDISH_MARGIN)
}

/// How many of `pixels` differ from the fixture's exact background — the ink a
/// cell drew.
fn ink_count(pixels: &[Pixel]) -> usize {
    let bg = decoration_background();
    pixels.iter().filter(|px| **px != bg).count()
}

/// How many of `pixels` [`is_reddish`] accepts.
fn reddish_count(pixels: &[Pixel]) -> usize {
    pixels.iter().filter(|px| is_reddish(**px)).count()
}

/// The x of the first sampled pixel in `col`.
fn band_x0(metrics: &CellMetrics, col: usize) -> usize {
    (col as f64 * metrics.cell_w) as usize + BAND_SIDE_INSET_PX
}

/// The width every sampled band shares, so two columns' samples are the same
/// shape and can be compared elementwise.
fn band_width(metrics: &CellMetrics) -> usize {
    (metrics.cell_w as usize).saturating_sub(2 * BAND_SIDE_INSET_PX)
}

/// The `[y0, y1)` device-pixel rows of the band BELOW the glyph box, clamped to a
/// `height`-pixel surface.
///
/// [`DECORATION_ROW`]'s top edge is `y == 0`, so the baseline offset from the cell
/// top IS the baseline's absolute y here.
fn sub_baseline_rows(metrics: &CellMetrics, height: usize) -> (usize, usize) {
    let top = (metrics.baseline + SUB_BASELINE_GUARD_PX).ceil() as usize;
    let bottom = metrics.cell_h as usize;
    (top, bottom.min(height))
}

/// The `[y0, y1)` device-pixel rows the undercurl's WAVE occupies — a taller band
/// than [`sub_baseline_rows`], starting at the baseline's own row.
///
/// MEASURED at [`SMOKE_PROBE_SIZE_PT`] / [`LINE_HEIGHT_PERCENT`] on the pinned
/// font stack: `cell_w = 22.4765625`, `cell_h = 49`, `baseline = 37.5`,
/// `ascent = 35`, `descent = 9`, and Pango's wave occupies device rows 37 to 43.
/// [`sub_baseline_rows`] starts at `ceil(baseline + SUB_BASELINE_GUARD_PX) == 39`
/// and so cuts the wave's whole upper half away; rows 39 and 40 are near-solid,
/// which is why a top-edge measurement taken over that band can only ever report a
/// spread of about 1 and reads as a shallow wave when the wave is not shallow.
///
/// This band starts at `floor(baseline) == 37` instead, and it keeps no guard row
/// against the glyph. The guard is what stops GLYPH ink entering an INK-based
/// band, and the one case that reads this band separates the two by colour rather
/// than by position — see [`topmost_reddish_rows`].
fn wave_band_rows(metrics: &CellMetrics, height: usize) -> (usize, usize) {
    let top = metrics.baseline.floor() as usize;
    let bottom = metrics.cell_h as usize;
    (top, bottom.min(height))
}

/// The `[y0, y1)` device-pixel rows of a whole cell's interior, inset top and
/// bottom.
fn cell_interior_rows(metrics: &CellMetrics, height: usize) -> (usize, usize) {
    let bottom = (metrics.cell_h as usize).min(height);
    (INTERIOR_INSET_PX, bottom.saturating_sub(INTERIOR_INSET_PX))
}

/// Reads `col`'s pixels over the row range `rows`, in row-major order.
fn column_pixels(
    data: &[u8],
    stride: usize,
    metrics: &CellMetrics,
    col: usize,
    rows: (usize, usize),
) -> Vec<Pixel> {
    let (y0, y1) = rows;
    let x0 = band_x0(metrics, col);
    let width = band_width(metrics);

    let mut out = Vec::with_capacity(width * y1.saturating_sub(y0));
    for y in y0..y1 {
        for x in x0..x0 + width {
            out.push(pixel_at(data, stride, x, y));
        }
    }
    out
}

/// For each sampled x of `col`, the topmost row in `rows` carrying the DECORATION's
/// colour; columns with none contribute nothing.
///
/// Reddish rather than merely inked, and that is the whole defence of the case
/// that reads it. [`wave_band_rows`] deliberately includes the baseline's own row,
/// where the glyph `x`'s two leg terminals still put ink at the left and right
/// extremes of the sample — nearer the cell top than any part of the wave. A
/// topmost-INK measurement would take those columns' glyph ink as the wave's crest
/// and report a spread the decoration never had: it would pass over a straight
/// line as readily as over a wave, which is the one failure mode that matters
/// here.
///
/// Colour separates them exactly, where a wider horizontal inset would only
/// separate them approximately and would rest on this glyph's own outline. The
/// fixture's glyph is white and its `special` is pure red: composite a red of
/// coverage `c` over ANY greyscale value and the red channel exceeds the other two
/// by exactly `255 * c`, so glyph ink beneath the wave cannot invent a reddish
/// pixel. The other order is not symmetric — an opaque glyph pixel painted over
/// the wave hides it — but that direction only drops a column from the sample, and
/// a dropped column can shrink the measured spread, never fabricate one.
///
/// It inherits one premise from
/// [`undercurl_draws_ink_below_the_glyph_box_in_the_special_colour`]: that the wave
/// is drawn in `special` at all. If that stops holding, this sampler returns
/// nothing and its caller's first assertion names the cause.
fn topmost_reddish_rows(
    data: &[u8],
    stride: usize,
    metrics: &CellMetrics,
    col: usize,
    rows: (usize, usize),
) -> Vec<usize> {
    let (y0, y1) = rows;
    let x0 = band_x0(metrics, col);

    let mut tops = Vec::new();
    for x in x0..x0 + band_width(metrics) {
        for y in y0..y1 {
            if is_reddish(pixel_at(data, stride, x, y)) {
                tops.push(y);
                break;
            }
        }
    }
    tops
}

/// An undercurl paints below the glyph box, and it paints there in `special`.
///
/// Falsifiers: (a) delete the `AttrColor::new_underline_color` insert in
/// `draw_glyph` — the undercurl column keeps its ink and loses its red; (b) return
/// `Underline::None` for `undercurl` in `glyph_style::glyph_decoration` — that
/// column loses all sub-baseline ink.
///
/// This case is also the MEASUREMENT the plan owes for whether Pango honours an
/// underline colour on `PANGO_UNDERLINE_ERROR` as it does on `Single`. A red here
/// that reports ink but no reddish pixel is that answer, not a broken test: the
/// recorded response is to keep the wave, drop the colour from the undercurl arm,
/// and record the limitation — never to fall back to a straight `Single`.
#[test]
fn undercurl_draws_ink_below_the_glyph_box_in_the_special_colour() {
    let (mut surface, metrics) = render_decoration_fixture();

    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");

    let rows = sub_baseline_rows(&metrics, height);
    let (y0, y1) = rows;

    // PREMISE, not coverage: a plain underline is what this renderer already drew
    // before decorations existed, so ink in ITS band is what establishes that the
    // band is aimed at the rows a decoration lands in. Without it, "no ink in the
    // undercurl's band" would be equally explained by a mis-placed band.
    let underline = column_pixels(&data, stride, &metrics, UNDERLINE_COL, rows);
    assert!(
        ink_count(&underline) > 0,
        "fixture premise: the plain-underline cell's band (rows {y0}..{y1}) holds no ink at all. \
         A single underline is drawn by this renderer independently of anything undercurl adds, so \
         an empty band here means the band is aimed at the wrong rows — SUB_BASELINE_GUARD_PX, the \
         measured baseline, or the cell height moved — and every other assertion in this case is \
         reading an empty rectangle."
    );

    let curl = column_pixels(&data, stride, &metrics, UNDERCURL_COL, rows);
    assert!(
        reddish_count(&curl) > 0,
        "the undercurl cell's band (rows {y0}..{y1}) holds {} inked pixels but none of them is \
         reddish. Two outcomes are distinguishable here and they need different fixes. Ink but no \
         red means Pango draws PANGO_UNDERLINE_ERROR in the layout foreground and ignores \
         AttrColor::new_underline_color — that is the open measurement, and the answer is to keep \
         the wave, drop the colour from the undercurl arm and record the limitation, NOT to fall \
         back to Underline::Single. No ink at all (0 above) means glyph_decoration answered \
         Underline::None for undercurl, or the AttrList never reached the layout.",
        ink_count(&curl)
    );

    let control = column_pixels(&data, stride, &metrics, CONTROL_COL, rows);
    assert_eq!(
        reddish_count(&control),
        0,
        "the undecorated control cell's band holds {} reddish pixels. Its job is the DETECTOR, not \
         the feature: nothing in this fixture is red except `special` on the three decorated ids, \
         and hl 0 carries no decoration, so a reddish pixel here means is_reddish is accepting the \
         white glyph ink or the black background and every red-based assertion in this file is \
         meaningless.",
        reddish_count(&control)
    );

    let no_special = column_pixels(&data, stride, &metrics, UNDERCURL_NO_SPECIAL_COL, rows);
    assert!(
        ink_count(&no_special) > 0,
        "the undercurl-WITHOUT-special cell's band holds no ink. Its `special` is unset, which is \
         a request to leave the colour to Pango — not a request to draw nothing. An empty band \
         means the undercurl arm is gated on a `special` being present, which silently drops the \
         decoration for every group set with a bare `cterm=undercurl`."
    );
    assert_eq!(
        reddish_count(&no_special),
        0,
        "the undercurl-without-special cell's band holds {} reddish pixels. Its `special` is None, \
         so its wave must take the layout foreground (white). Red here means the decoration colour \
         came from something other than this cell's own `special`: a hard-coded colour in \
         draw_glyph, a fallback reading another id, or the fixture's editor defaults having \
         acquired a `special`, which HighlightTable::resolve folds into every style.",
        reddish_count(&no_special)
    );
}

/// The undercurl is not the mark a plain underline draws.
///
/// This is what proves the wave is a wave WITHOUT measuring a wave: it asserts no
/// amplitude, no period and no shape — only that the two decorations differ over
/// the same rectangle. The two columns are sampled at the same inset and the same
/// width, and the comparison is elementwise.
///
/// Falsifier: map `undercurl` to `pango::Underline::Single` in
/// `glyph_style::glyph_decoration` — which is exactly macOS's behaviour — and the
/// two bands become identical.
#[test]
fn undercurl_and_underline_do_not_draw_the_same_thing() {
    let (mut surface, metrics) = render_decoration_fixture();

    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");

    let rows = sub_baseline_rows(&metrics, height);
    let straight = column_pixels(&data, stride, &metrics, UNDERLINE_COL, rows);
    let wavy = column_pixels(&data, stride, &metrics, UNDERCURL_COL, rows);

    // Two EMPTY bands are also identical, and would redden this case for a reason
    // that has nothing to do with the rule. Name that cause before asserting on
    // the difference.
    assert!(
        ink_count(&straight) > 0 && ink_count(&wavy) > 0,
        "fixture premise: the two bands hold {} and {} inked pixels. A band with no ink means no \
         decoration was drawn there at all, and the difference asserted below would then be \
         reporting on two empty rectangles.",
        ink_count(&straight),
        ink_count(&wavy)
    );

    // `assert!` rather than `assert_ne!`: the two operands are hundreds of pixels
    // and printing both twice would bury the message that says what the failure
    // means.
    assert!(
        straight != wavy,
        "the plain-underline band and the undercurl band are byte-identical over {} sampled \
         pixels. That is what mapping `undercurl` to Underline::Single produces — macOS's own \
         `hasUnderline = underline || undercurl` — a straight line where Neovim asked for a wave. \
         Both columns are sampled at the same inset and the same width, so identical bytes cannot \
         be a sampling artefact: they mean one Pango attribute was used for both.",
        straight.len()
    );
}

/// A strikethrough adds ink across the glyph, and that ink is NOT the `special`
/// colour.
///
/// The second half is the macOS-parity half: `AttrColor::new_strikethrough_color`
/// is deliberately never called, so Pango strikes in the layout foreground exactly
/// as `GlyphStyleResolver.resolve`'s unconditional `strikethroughColor: fg` does.
///
/// Falsifiers: (a) drop the `AttrInt::new_strikethrough(true)` insert in
/// `draw_glyph` — the two ink counts converge to within antialiasing noise, well
/// under [`MIN_STRIKE_INK_PX`]; (b) add
/// `AttrColor::new_strikethrough_color(special)` — a reddish pixel appears and the
/// parity half reddens.
#[test]
fn strikethrough_adds_foreground_ink_across_the_glyph() {
    let (mut surface, metrics) = render_decoration_fixture();

    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");

    let rows = cell_interior_rows(&metrics, height);
    let struck = column_pixels(&data, stride, &metrics, STRIKETHROUGH_COL, rows);
    let control = column_pixels(&data, stride, &metrics, CONTROL_COL, rows);

    let struck_ink = ink_count(&struck);
    let control_ink = ink_count(&control);
    assert!(
        struck_ink >= control_ink + MIN_STRIKE_INK_PX,
        "the struck cell's interior holds {struck_ink} inked pixels against the undecorated \
         control's {control_ink} — a difference of {}, under the {MIN_STRIKE_INK_PX} px margin. \
         Both cells draw the same glyph, so the whole difference is the strike, and a strike spans \
         the run's entire advance ({} px here): a difference this small means the \
         new_strikethrough(true) attribute never reached the layout and the two cells differ only \
         by antialiasing. The margin is here because a bare `greater than` would be satisfied by \
         one stray blend pixel.",
        struck_ink as i64 - control_ink as i64,
        metrics.cell_w
    );

    assert_eq!(
        reddish_count(&struck),
        0,
        "the struck cell holds {} reddish pixels. Its `special` IS set, and it must not reach the \
         strike: with no strikethrough-colour attribute Pango strikes in the layout foreground, \
         which is what gives parity with macOS's unconditional `strikethroughColor: fg`. A reddish \
         pixel means AttrColor::new_strikethrough_color was added — the constructor exists, and \
         not calling it is the decision.",
        reddish_count(&struck)
    );
}

/// The undercurl's top edge rises and falls across the cell, which no straight
/// line can do.
///
/// This is the one case that tells a wave from a thicker or a vertically offset
/// straight line. [`undercurl_and_underline_do_not_draw_the_same_thing`] proves
/// only that the two marks DIFFER, and every one of those alternatives differs
/// from a plain underline too.
///
/// MEASURED at [`SMOKE_PROBE_SIZE_PT`] / [`LINE_HEIGHT_PERCENT`] on the pinned
/// font stack: `cell_w = 22.4765625`, `cell_h = 49`, `baseline = 37.5`,
/// `ascent = 35`, `descent = 9`; Pango's wave occupies device rows 37 to 43, a
/// seven-row band whose top edge spans exactly 3 device pixels. All 18 sampled
/// columns carry the decoration's colour, and their topmost coloured rows run 37,
/// 38, 39, 40 and back — so the spread is measured over the full band, not over a
/// subset the colour threshold happened to keep. The same sampler over the
/// plain-underline column reports row 37 in every one of those 18 columns, a
/// spread of 0. Anyone moving this fixture's font size or line-height is moving
/// all of those numbers at once and owes the band and [`MIN_WAVE_SPREAD_PX`] a
/// fresh measurement — the wave's amplitude scales with the face's underline
/// thickness, not with the cell.
///
/// The sample is the FULL band width, and glyph ink is excluded by colour rather
/// than by insetting the columns; [`topmost_reddish_rows`] carries the reason and
/// the hazard it defends against. It reads one row per column, so it is blind to
/// the wave's thickness and to its period: what it discriminates is a decoration
/// whose top edge is CONSTANT across the cell.
///
/// Falsifier: map `undercurl` to `pango::Underline::Single` in
/// `glyph_style::glyph_decoration` — a rectangle's top edge is the same row in
/// every column, so the spread collapses to 0.
#[test]
fn the_undercurl_is_wavy_rather_than_straight() {
    let (mut surface, metrics) = render_decoration_fixture();

    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");

    let rows = wave_band_rows(&metrics, height);
    let (y0, y1) = rows;
    let tops = topmost_reddish_rows(&data, stride, &metrics, UNDERCURL_COL, rows);

    assert!(
        tops.len() >= MIN_WAVE_SAMPLED_COLUMNS,
        "only {} of the {} sampled columns of the wave band (rows {y0}..{y1}) carry the \
         decoration's colour, under the {MIN_WAVE_SAMPLED_COLUMNS} this case needs. A wave is \
         drawn across the run's whole advance, so this few means one of three things: no undercurl \
         was drawn; the wave is drawn in the layout foreground rather than in `special`, which \
         `undercurl_draws_ink_below_the_glyph_box_in_the_special_colour` would also report; or the \
         band is aimed past the wave, which is what `wave_band_rows`' measured geometry exists to \
         prevent. The spread below would otherwise be describing antialiasing rather than a shape.",
        tops.len(),
        band_width(&metrics)
    );

    let min = *tops.iter().min().expect("a non-empty top-edge sample");
    let max = *tops.iter().max().expect("a non-empty top-edge sample");
    let spread = max - min;
    assert!(
        spread >= MIN_WAVE_SPREAD_PX,
        "the undercurl's topmost coloured row runs between {min} and {max} across the cell — a \
         spread of {spread} device pixels, under {MIN_WAVE_SPREAD_PX}. A spread of 0 means every \
         column's decoration starts on the same row, which is a horizontal RECTANGLE: exactly what \
         Underline::Single draws and what macOS folds undercurl into. A spread of 1 is not \
         evidence of a wave either — an antialiased or one-pixel-offset straight edge produces it \
         — which is why the threshold is 2 and not 1. The measured wave spans about 3, so a 1 here \
         is far likelier to mean the band is clipping the wave's upper half than that the wave \
         went shallow: RE-MEASURE the band before touching this threshold, and never lower it to \
         match what was observed."
    );
}

/// A decorated cell's Pango attributes reach that cell and stop there.
///
/// `paint` builds ONE `GlyphPainter` per call and re-texts a single layout for
/// every cell of the grid, so the layout REMEMBERS the attribute list the last
/// decorated cell installed. The `set_attributes(None)` clear in
/// `render::GlyphPainter::draw` is the only thing between that memory and an
/// underline drawn beneath every ordinary cell painted after a decorated one —
/// which, on a real buffer, is most of the frame.
///
/// [`CONTROL_COL`] is structurally unable to see this: it is painted BEFORE every
/// decorated cell, so a decoration leaking FORWARD only ever reaches cells that
/// are decorated anyway. It is used here as the reference for what an undecorated
/// cell's band holds, not as the detector.
///
/// The two assertions fail for different reasons and both are reachable. Reddish
/// pixels name a leak of a decoration drawn in `special`, and that readout cannot
/// be confused with glyph ink whatever the band contains, since the fixture's ink
/// is white and no blend of white over black is reddish. Inked pixels name a leak
/// of any decoration at all, including one drawn in the layout foreground — the
/// shape [`UNDERCURL_NO_SPECIAL_COL`] would produce — and that readout rests on
/// the control band being empty, which is asserted rather than assumed.
///
/// Falsifier: delete the `self.layout.set_attributes(None);` line in
/// `render::GlyphPainter::draw`. [`LEAK_SOURCE_COL`]'s red underline then survives
/// onto [`TRAILING_CONTROL_COL`] and both assertions redden.
#[test]
fn a_decoration_does_not_leak_onto_the_cell_after_it() {
    let (mut surface, metrics) = render_decoration_fixture();

    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");

    let rows = sub_baseline_rows(&metrics, height);
    let (y0, y1) = rows;

    // PREMISE: there is something to leak. A source that drew nothing would make
    // a clean trailing cell prove nothing at all, and both assertions below would
    // then be green against a build with no clear in it.
    let source = column_pixels(&data, stride, &metrics, LEAK_SOURCE_COL, rows);
    let source_red = reddish_count(&source);
    assert!(
        source_red > 0,
        "fixture premise: the last decorated cell of the row (column {LEAK_SOURCE_COL}) holds {} \
         inked pixels and {source_red} reddish ones in its own band (rows {y0}..{y1}). It carries \
         the same red underline as column {UNDERLINE_COL}, so an empty or colourless band here \
         means the decoration is not being drawn at all — `undercurl_draws_ink_below_the_glyph_box\
         _in_the_special_colour` says which half — and nothing after it could inherit anything.",
        ink_count(&source)
    );

    // The reference: what an undecorated cell's band holds when nothing has
    // leaked into it. Asserted, because the ink assertion below is stated as an
    // absolute zero and would be reading a band full of glyph antialiasing if the
    // guard, the baseline or the cell height moved.
    let control = column_pixels(&data, stride, &metrics, CONTROL_COL, rows);
    let control_ink = ink_count(&control);
    assert_eq!(
        control_ink, 0,
        "fixture premise: the undecorated control at column {CONTROL_COL} — painted before every \
         decoration, so nothing can have leaked into it — holds {control_ink} inked pixels in the \
         band below the baseline (rows {y0}..{y1}). That band is supposed to contain decoration \
         and nothing else: `{DECORATION_GLYPH}` has no descender, and SUB_BASELINE_GUARD_PX keeps \
         the baseline's own antialiased row out of it. Ink here means the band has moved up into \
         the glyph box, and the assertion below would then redden for a build with a perfectly \
         good attribute clear."
    );

    let trailing = column_pixels(&data, stride, &metrics, TRAILING_CONTROL_COL, rows);
    let trailing_red = reddish_count(&trailing);
    let trailing_ink = ink_count(&trailing);

    assert_eq!(
        trailing_red, 0,
        "the undecorated cell painted AFTER every decorated one (column {TRAILING_CONTROL_COL}) \
         holds {trailing_red} reddish pixels in its sub-baseline band, where the control at column \
         {CONTROL_COL} holds none. This cell carries hl 0, and nothing in this fixture is red \
         except `special` on the decorated ids, so red beneath it is the AttrList column \
         {LEAK_SOURCE_COL} installed on the SHARED layout still being attached: \
         `GlyphPainter::draw`'s `set_attributes(None)` did not run, and every ordinary cell drawn \
         after a decorated one is underlined in the previous cell's colour. The control cannot \
         report this — it is painted first — which is the whole reason this cell exists."
    );

    assert_eq!(
        trailing_ink, 0,
        "the undecorated cell painted AFTER every decorated one (column {TRAILING_CONTROL_COL}) \
         holds {trailing_ink} inked pixels in its sub-baseline band, where the control at column \
         {CONTROL_COL} holds {control_ink}. Ink with no red among it ({trailing_red} reddish) is a \
         leaked decoration drawn in the LAYOUT foreground rather than in `special` — an undercurl \
         whose highlight set no `special`, the shape column {UNDERCURL_NO_SPECIAL_COL} carries — \
         so the attribute clear is missing for that case even if the coloured one above passed. \
         Ink accompanied by red is the same failure the assertion above already named."
    );
}

// --- The face: bold and italic, switched on and back off inside one paint ----
//
// A THIRD fixture and a third render helper, for the reason the decoration
// fixture gives for being a second one, plus one of its own. The cases above are
// calibrated against a band below the baseline of a row carrying `x`, and the
// face question is asked with whole-cell ink counts and a left edge — a different
// glyph and a different band. Keeping the two fixtures apart also keeps the two
// falsifiers' evidence apart: a decoration leaking forward through the decoration
// fixture would reach face cells appended to it, so one red run would indict two
// independent pieces of painter state at once.
//
// Everything else is shared: the pixel reader, the band helpers, `ink_count` and
// the colours, which is why this fixture must keep DEFAULT_FG on DEFAULT_BG —
// `ink_count` measures against exactly that background.

/// Columns in the face fixture — plain, bold, plain, italic, plain.
const FACE_COLS: usize = 5;
/// Rows in the face fixture.
const FACE_ROWS: usize = 1;
/// The single row the face fixture paints into.
const FACE_ROW: usize = 0;

/// The glyph every face cell carries.
///
/// Chosen for two measurable properties and not for its name, which no assertion
/// here reads. Its left stem is a plain vertical rectangle, so the leftmost inked
/// pixel of every row an upright cell draws sits at the same x and any drift is
/// the slant; and it is two stems plus a crossbar, so bold thickens three strokes
/// at once and the ink count moves by far more than a rounding.
const FACE_GLYPH: &str = "H";

/// The plain reference cell, painted FIRST — before the layout has been told
/// anything about a face.
const PLAIN_BEFORE_COL: usize = 0;
/// The bold cell, painted immediately after a plain one.
const BOLD_COL: usize = 1;
/// The plain cell painted immediately AFTER the bold one: the restore.
const PLAIN_AFTER_BOLD_COL: usize = 2;
/// The italic cell, painted immediately after a plain one.
const ITALIC_COL: usize = 3;
/// The plain cell painted immediately AFTER the italic one: the restore.
const PLAIN_AFTER_ITALIC_COL: usize = 4;

/// Highlight id for [`BOLD_COL`].
const BOLD_HL_ID: u64 = 21;
/// Highlight id for [`ITALIC_COL`].
const ITALIC_HL_ID: u64 = 22;

/// The fewest pixels of ink a bold cell must add over a plain one.
///
/// A margin rather than a bare `greater than`, for the reason
/// [`MIN_STRIKE_INK_PX`] records: a cell origin is `col * cell_w` with a
/// fractional `cell_w` and Cairo rounds a glyph's origin to a whole device pixel,
/// so two columns drawing the SAME face can differ by a pixel or two at the edges
/// of the sampled band. Bold thickens both stems and the crossbar of
/// [`FACE_GLYPH`] over the full cap height, which is a difference in the hundreds
/// at [`SMOKE_PROBE_SIZE_PT`]; twenty sits an order of magnitude above the noise
/// and an order of magnitude below the signal.
const MIN_BOLD_INK_PX: usize = 20;

/// The smallest share of [`PLAIN_BEFORE_COL`]'s ink, in percent, that
/// [`PLAIN_AFTER_BOLD_COL`] may hold and still count as having drawn its glyph.
///
/// The restore assertion is an upper bound, and a cell that drew NOTHING sits
/// comfortably under any upper bound: this is the floor that closes it, and the
/// only thing it is here to exclude is an empty or near-empty cell.
///
/// Half, and deliberately nowhere near tight. The two plain cells draw the same
/// glyph in the same face and measure 288 and 260 inked pixels on this host — a
/// 28 px gap that is sub-pixel phase alone, since a cell origin is `col * cell_w`
/// with a fractional `cell_w` and Cairo rounds a glyph's origin to a whole device
/// pixel (see [`leftmost_ink_columns`]). Fifty percent leaves room for several
/// times that difference; every count between the floor and the upper bound is a
/// question about the rasterizer rather than about the painter's state, and this
/// case is not the place to ask it.
const MIN_RESTORED_INK_PERCENT: usize = 50;

/// The fewest device pixels the leftmost inked pixel must drift down an italic
/// cell before the face counts as slanted.
///
/// MEASURED under the devShell's pinned `FONTCONFIG_FILE`, not derived: the
/// oblique face this stack resolves for [`FACE_GLYPH`] at [`SMOKE_PROBE_SIZE_PT`]
/// drifts 3 px in [`ITALIC_COL`], and 4 px in [`PLAIN_AFTER_ITALIC_COL`] when the
/// restore falsifier named below leaves that column oblique too. The same face at
/// a different sub-pixel phase, so what the pinned stack produces is a 3–4 px
/// RANGE rather than a single number.
///
/// Two, one pixel under the low end of that range. A floor sitting ON the
/// measurement reddens a correct build on any fontconfig or rasterizer nudge that
/// costs a pixel, and it buys nothing: two is still strictly above
/// [`MAX_UPRIGHT_SLANT_PX`], so an upright stem can never satisfy it, and the
/// falsifier that deletes `set_style` drives the drift to 0 rather than to 2. The
/// margin costs this constant no discrimination at all.
///
/// A drift of 0 on the italic cell while the bold case is green is a reading about
/// the FONT STACK rather than about the painter: fontconfig resolved no oblique
/// face for this family and Pango synthesized no slant. The answer to that is to
/// record it and drop the italic direction — never to lower this floor to whatever
/// a stack carrying no oblique face happens to produce.
const MIN_SLANT_SPREAD_PX: usize = 2;

/// The most drift an UPRIGHT face may show down the same stem.
///
/// A vertical edge puts the same partially-covered pixel in every row, so the
/// measured drift is 0; one pixel of headroom is left for a rasterizer that snaps
/// the top or bottom row of the stem differently.
const MAX_UPRIGHT_SLANT_PX: usize = 1;

/// The fewest rows of a cell that must carry ink before a drift measured over
/// them describes a stem rather than an antialiased corner.
///
/// Low on purpose. [`FACE_GLYPH`] at [`SMOKE_PROBE_SIZE_PT`] spans some 27 rows,
/// so this floor is not a fit to the fixture; what it has to survive is the
/// falsifier that deletes the base-description restore, which also leaves the
/// FIRST cell with no description and draws it in Pango's own default face —
/// smaller, and so fewer rows. A floor tight enough to trip on that would make
/// this case report a thin premise where the assertion three lines down has the
/// real answer.
const MIN_SLANT_SAMPLED_ROWS: usize = 6;

/// Builds the face fixture: one row of five cells, all carrying [`FACE_GLYPH`] on
/// white-on-black, differing only in their `(bold, italic)` pair.
///
/// The order is the fixture: plain, bold, plain, italic, plain. `paint` walks a
/// row left to right through one `GlyphPainter`, so each emphasized cell is
/// preceded by a plain one and followed by a plain one, and both directions of
/// every face change happen inside a single call.
///
/// The two emphasized ids set no colour, so `HighlightTable::resolve` gives them
/// the editor defaults and all five cells are white on black — which is what lets
/// [`ink_count`] measure them.
fn face_fixture_snapshot() -> Snapshot {
    let mut snap = Snapshot::new(FACE_COLS, FACE_ROWS);

    snap.highlights.set_defaults(DefaultColors {
        fg: Some(DEFAULT_FG),
        bg: Some(DEFAULT_BG),
        special: None,
    });
    snap.highlights.define(
        BOLD_HL_ID,
        HlAttr {
            bold: true,
            ..HlAttr::default()
        },
    );
    snap.highlights.define(
        ITALIC_HL_ID,
        HlAttr {
            italic: true,
            ..HlAttr::default()
        },
    );

    let ids = [
        (PLAIN_BEFORE_COL, 0),
        (BOLD_COL, BOLD_HL_ID),
        (PLAIN_AFTER_BOLD_COL, 0),
        (ITALIC_COL, ITALIC_HL_ID),
        (PLAIN_AFTER_ITALIC_COL, 0),
    ];
    for (col, hl_id) in ids {
        snap.grid_mut(1).set_cell(
            FACE_ROW,
            col,
            Cell {
                text: FACE_GLYPH.to_string(),
                hl_id,
            },
        );
    }

    snap
}

/// Measures metrics at [`SMOKE_PROBE_SIZE_PT`] and [`LINE_HEIGHT_PERCENT`], sizes
/// a surface to exactly `FACE_COLS x FACE_ROWS` cells, and paints the face fixture
/// onto it in ONE `paint` call.
///
/// The single call is the point rather than an economy: the state these cases read
/// lives between the cells of one `GlyphPainter`, and five separate paints would
/// build five painters and pin nothing.
///
/// The smokes' 28 pt for the same reason the decoration fixture uses it — a stem
/// wide enough that bold and a slant are several device pixels rather than one.
fn render_face_fixture() -> (ImageSurface, CellMetrics) {
    let metrics = probe_metrics(FONT_FAMILY, SMOKE_PROBE_SIZE_PT, LINE_HEIGHT_PERCENT);

    let width = (metrics.cell_w * FACE_COLS as f64).ceil() as i32;
    let height = (metrics.cell_h * FACE_ROWS as f64).ceil() as i32;
    assert!(
        width > 0 && height > 0,
        "CellMetrics produced a non-positive cell size for the face fixture: cell_w={}, cell_h={}",
        metrics.cell_w,
        metrics.cell_h
    );

    let surface = ImageSurface::create(Format::ARgb32, width, height).expect("face render surface");
    let cr = Context::new(&surface).expect("face render context");

    let snap = face_fixture_snapshot();
    paint(&cr, snap.active_grid(), &snap.highlights, &metrics);

    // Drop the context before any caller reads pixels, exactly as the other two
    // render helpers do.
    drop(cr);
    surface.flush();

    assert!(
        band_width(&metrics) >= MIN_BAND_WIDTH_PX,
        "fixture precondition: a sampled band is {} px wide, under the {MIN_BAND_WIDTH_PX} px \
         these cases need. The cell advance is {} px and {BAND_SIDE_INSET_PX} px are inset from \
         each side; a band this narrow means the measured advance collapsed.",
        band_width(&metrics),
        metrics.cell_w
    );

    (surface, metrics)
}

/// For each row of `rows` in `col` that carries any ink, how far right of the
/// band's first sampled pixel that row's LEFTMOST ink sits; rows with none
/// contribute nothing.
///
/// Offsets from the band rather than absolute device columns, so the numbers a
/// failure prints mean the same thing in every column. Only the spread of the
/// returned values is ever read, and every value comes from one column, so this
/// measurement is immune to the sub-pixel phase that stops two columns being
/// compared pixel for pixel: `col * cell_w` is fractional and Cairo rounds a
/// glyph's origin to a whole device pixel, so the SAME glyph in two columns lands
/// at a different offset inside its band.
///
/// Ink rather than colour, unlike [`topmost_reddish_rows`]: this fixture draws no
/// decoration at all, so the only thing in a cell is its glyph.
fn leftmost_ink_columns(
    data: &[u8],
    stride: usize,
    metrics: &CellMetrics,
    col: usize,
    rows: (usize, usize),
) -> Vec<usize> {
    let (y0, y1) = rows;
    let x0 = band_x0(metrics, col);
    let width = band_width(metrics);
    let bg = decoration_background();

    let mut lefts = Vec::new();
    for y in y0..y1 {
        for dx in 0..width {
            if pixel_at(data, stride, x0 + dx, y) != bg {
                lefts.push(dx);
                break;
            }
        }
    }
    lefts
}

/// The distance between the extremes of a [`leftmost_ink_columns`] sample, or
/// `None` when it is empty.
fn left_edge_drift(lefts: &[usize]) -> Option<usize> {
    let min = lefts.iter().min()?;
    let max = lefts.iter().max()?;
    Some(max - min)
}

/// Bold is applied to a bold cell and taken back off for the ordinary cell after
/// it, inside one `paint`.
///
/// `GlyphPainter` sets a font description only when the `(bold, italic)` pair
/// CHANGES, so that pair is state carried from one cell to the next and the arm
/// that re-applies the base description is the only thing that ever ends a bold
/// run. Both directions are read from a single paint: [`BOLD_COL`] follows a plain
/// cell, [`PLAIN_AFTER_BOLD_COL`] follows the bold one.
///
/// An ink COUNT, deliberately, where
/// [`undercurl_and_underline_do_not_draw_the_same_thing`] compares bands
/// elementwise: that comparison is sound only in the direction it is used, since
/// two columns drawing the same face at different sub-pixel phases also compare
/// unequal (see [`leftmost_ink_columns`]). A count is blind to a one-pixel shift
/// and not to three thickened strokes.
///
/// Falsifiers, three, each one line of `render::GlyphPainter::draw`:
/// (a) delete the `else` arm of `if face == (false, false)` — the arm that clones
/// the base description and sets the weight — and no cell is ever bold, which
/// reddens the first assertion; (b) delete the
/// `self.layout.set_font_description(Some(&metrics.font));` inside the
/// `face == (false, false)` arm and nothing restores the base face, which reddens
/// the second. Falsifier (b) also leaves the FIRST cell with no description at
/// all, so its count drops to whatever Pango's own default face draws — visible in
/// the printed numbers, and not a second defect. (c) Hand that same arm `None`
/// instead of the base description when `self.face.is_some()`, so the restored cell
/// draws in Pango's own tiny context default rather than in the base face: the
/// third assertion reddens, where the second — an upper bound — accepts it.
#[test]
fn bold_is_applied_and_then_restored_within_one_paint() {
    let (mut surface, metrics) = render_face_fixture();

    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");

    let rows = cell_interior_rows(&metrics, height);
    let plain_before = ink_count(&column_pixels(
        &data,
        stride,
        &metrics,
        PLAIN_BEFORE_COL,
        rows,
    ));
    let bold = ink_count(&column_pixels(&data, stride, &metrics, BOLD_COL, rows));
    let plain_after = ink_count(&column_pixels(
        &data,
        stride,
        &metrics,
        PLAIN_AFTER_BOLD_COL,
        rows,
    ));

    // PREMISE: the plain reference drew a glyph at all. Three empty cells would
    // satisfy the restore assertion below without anything having been restored.
    assert!(
        plain_before > 0,
        "fixture premise: the plain cell at column {PLAIN_BEFORE_COL} holds no ink at all over the \
         cell interior (rows {}..{}). Every count below is a difference against this one, so an \
         empty cell means the fixture painted nothing — a glyph that resolved to nothing, or a \
         band aimed outside the surface — and the two assertions would be comparing zeroes.",
        rows.0,
        rows.1
    );

    // Direction one: plain, then bold.
    assert!(
        bold >= plain_before + MIN_BOLD_INK_PX,
        "the bold cell at column {BOLD_COL} holds {bold} inked pixels against the plain cell \
         before it at column {PLAIN_BEFORE_COL} with {plain_before} — a difference of {}, under \
         the {MIN_BOLD_INK_PX} px margin. Both cells draw `{FACE_GLYPH}` at the same size, so the \
         whole difference is the weight: bold thickens two stems and a crossbar over the full cap \
         height, which is a difference in the hundreds here. A difference near zero means the \
         painter never set a bold description — the `(bold, italic)` arm was skipped, or the pair \
         was resolved as (false, false) — and every bold highlight Neovim sends is drawn at the \
         regular weight.",
        bold as i64 - plain_before as i64
    );

    // Direction two: bold, then plain. This is the state, and the reason the
    // painter is not stateless.
    assert!(
        plain_after < plain_before + MIN_BOLD_INK_PX,
        "the plain cell painted immediately AFTER the bold one (column {PLAIN_AFTER_BOLD_COL}) \
         holds {plain_after} inked pixels, against {plain_before} for the identical plain cell \
         painted BEFORE it (column {PLAIN_BEFORE_COL}) and {bold} for the bold one. A count up \
         near the bold cell's means the layout is still carrying the bold description: \
         `GlyphPainter` re-applies the base face only when the `(bold, italic)` pair changes back, \
         and with that arm gone one bold cell turns the whole rest of the paint bold. Nothing \
         painted before a bold cell can report this, which is why the fixture puts a plain cell on \
         both sides."
    );

    // The floor under direction two. The bound above is one-sided: a cell that
    // drew NOTHING is under it too, and would report a restore that never
    // happened.
    let restored_floor = plain_before * MIN_RESTORED_INK_PERCENT / 100;
    assert!(
        plain_after >= restored_floor,
        "the plain cell painted immediately AFTER the bold one (column {PLAIN_AFTER_BOLD_COL}) \
         holds {plain_after} inked pixels, under the floor of {restored_floor} — \
         {MIN_RESTORED_INK_PERCENT}% of the {plain_before} the identical plain cell before it \
         holds. This bound excludes exactly one thing, and it is the thing the assertion above \
         cannot see: a cell that drew nothing, or nearly nothing. That is not bold being taken \
         back off — it is the restore arm handing the layout a description that resolves to no \
         glyph, a cleared description falling back to Pango's context default or a collapsed \
         size — and it would otherwise read as a successful restore. The floor is loose on \
         purpose: two identical plain cells already differ by tens of pixels of sub-pixel phase, \
         so nothing between it and the bound above is a statement about the painter's state."
    );
}

/// Italic is applied to an italic cell and taken back off for the ordinary cell
/// after it, inside one `paint`.
///
/// The bold half's argument, for the other bit of the pair
/// ([`bold_is_applied_and_then_restored_within_one_paint`] carries it), measured
/// as a shape rather than a quantity: an upright stem's leftmost inked pixel is
/// the same in every row and a slanted one's walks across the cell.
///
/// The upright cell at [`PLAIN_BEFORE_COL`] is the DETECTOR for the measurement
/// itself. It is painted before any face has been set, so a drift there means
/// [`leftmost_ink_columns`] is reading something other than a stem — clipping at
/// the band edge, or a glyph whose left edge is not vertical — and the italic
/// threshold below would then be describing the sampler rather than the face.
///
/// Falsifiers: (a) delete `font.set_style(pango::Style::Italic);` in
/// `render::GlyphPainter::draw` and the italic cell draws upright, which reddens
/// the second assertion alone; (b) delete the
/// `self.layout.set_font_description(Some(&metrics.font));` inside the
/// `face == (false, false)` arm and the cell after the italic one stays slanted,
/// which reddens the third — the same one-line revert the bold case's direction
/// two names, so the two cases redden together under it and separately under
/// everything else.
#[test]
fn italic_is_applied_and_then_restored_within_one_paint() {
    let (mut surface, metrics) = render_face_fixture();

    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");

    let rows = cell_interior_rows(&metrics, height);
    let upright = leftmost_ink_columns(&data, stride, &metrics, PLAIN_BEFORE_COL, rows);
    let slanted = leftmost_ink_columns(&data, stride, &metrics, ITALIC_COL, rows);
    let restored = leftmost_ink_columns(&data, stride, &metrics, PLAIN_AFTER_ITALIC_COL, rows);

    // PREMISE: each cell drew a stem tall enough for a drift to mean anything. A
    // sample of one row has a drift of 0 whatever face drew it, which would make
    // both of the upright assertions below pass vacuously.
    for (col, sample) in [
        (PLAIN_BEFORE_COL, &upright),
        (ITALIC_COL, &slanted),
        (PLAIN_AFTER_ITALIC_COL, &restored),
    ] {
        assert!(
            sample.len() >= MIN_SLANT_SAMPLED_ROWS,
            "fixture premise: only {} of the {} sampled rows of column {col} carry any ink, under \
             the {MIN_SLANT_SAMPLED_ROWS} rows a left edge needs to be an edge. `{FACE_GLYPH}` \
             spans the cap height, so this few means the cell drew nothing or the band is aimed \
             outside the glyph, and a drift measured over what is left describes an antialiased \
             corner rather than a stem.",
            sample.len(),
            rows.1 - rows.0
        );
    }

    let upright_drift = left_edge_drift(&upright).expect("a non-empty upright left-edge sample");
    let slanted_drift = left_edge_drift(&slanted).expect("a non-empty italic left-edge sample");
    let restored_drift = left_edge_drift(&restored).expect("a non-empty restored left-edge sample");

    // The detector, not the feature: with an upright stem drifting, the two
    // assertions after it would be measuring the sampler.
    assert!(
        upright_drift <= MAX_UPRIGHT_SLANT_PX,
        "the leftmost inked pixel of the plain cell at column {PLAIN_BEFORE_COL} — painted before \
         any face was set — drifts {upright_drift} px down the cell, over the \
         {MAX_UPRIGHT_SLANT_PX} px an upright stem is allowed. `{FACE_GLYPH}`'s left stem is a \
         vertical rectangle, so every row's leftmost ink belongs at the same offset. A drift here \
         means the sample is not reading that stem: the glyph is clipped at the band's left edge, \
         or the substituted face is not the pinned one. Fix that before reading the two \
         assertions below, which are both differences against this number."
    );

    // Direction one: plain, then italic.
    assert!(
        slanted_drift >= MIN_SLANT_SPREAD_PX,
        "the italic cell at column {ITALIC_COL} shows a left-edge drift of {slanted_drift} px, \
         under the {MIN_SLANT_SPREAD_PX} px floor, against {upright_drift} px for the plain cell \
         beside it. Two outcomes are distinguishable and they need different answers. A drift \
         equal to the upright cell's means the italic bit reached no font description at all — \
         `set_style` skipped, or the `(bold, italic)` pair resolved without it — and every italic \
         highlight is drawn upright. A drift equal to the upright cell's WITH the bold case green \
         may instead be a fact about the pinned font stack: no oblique face resolved for this \
         family and no synthetic slant. That is to be recorded, not accommodated by lowering the \
         floor."
    );

    // Direction two: italic, then plain.
    assert!(
        restored_drift <= MAX_UPRIGHT_SLANT_PX,
        "the plain cell painted immediately AFTER the italic one (column \
         {PLAIN_AFTER_ITALIC_COL}) shows a left-edge drift of {restored_drift} px, over the \
         {MAX_UPRIGHT_SLANT_PX} px an upright stem is allowed and against {upright_drift} px for \
         the identical plain cell painted BEFORE the italic one. It is still being drawn in the \
         oblique face: `GlyphPainter` re-applies the base description only when the \
         `(bold, italic)` pair changes back, and with that arm gone one italic cell slants the \
         whole rest of the paint. The plain cell at column {PLAIN_BEFORE_COL} cannot report this — \
         it is painted first — which is why there is a plain cell on both sides."
    );
}

/// Dev aid (NOT part of the CI gate): regenerates an optional PNG golden under
/// `tests/testdata/paint_golden.png`. Run explicitly with:
///
/// ```text
/// nix develop --command cargo test -p wolf-gnome --test paint_golden \
///     -- --ignored regenerate_png_golden
/// ```
///
/// NOTE to coder: `ImageSurface::write_to_png` requires the cairo-rs `png`
/// feature to be enabled in `crates/wolf-gnome/Cargo.toml`'s `cairo` dependency.
#[test]
#[ignore = "dev aid: regenerates the optional PNG golden; never part of the CI gate"]
fn regenerate_png_golden() {
    use std::fs;
    use std::path::PathBuf;

    let (surface, _metrics) = render_fixture();

    let mut out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    out_dir.push("tests");
    out_dir.push("testdata");
    fs::create_dir_all(&out_dir).expect("create testdata dir");

    let mut out_path = out_dir;
    out_path.push("paint_golden.png");

    let mut file = fs::File::create(&out_path).expect("create golden PNG");
    surface
        .write_to_png(&mut file)
        .expect("write golden PNG (is the cairo `png` feature enabled?)");
}
