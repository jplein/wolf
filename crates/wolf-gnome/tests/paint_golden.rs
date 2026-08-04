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
