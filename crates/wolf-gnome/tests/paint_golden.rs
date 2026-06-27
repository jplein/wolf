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

/// Highlight id carrying a distinct fg/bg (anything non-zero; 0 is the default).
const DEFINED_HL_ID: u64 = 7;

/// Opaque alpha byte for an ARGB32 pixel.
const OPAQUE: u8 = 255;

// --- Fixture colors (kept maximally distinct for unambiguous sampling) ------

/// Default foreground (white).
const DEFAULT_FG: Rgb = Rgb { r: 255, g: 255, b: 255 };
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

    snap.grid.set_cell(
        CONTENT_ROW,
        0,
        Cell {
            text: "A".to_string(),
            hl_id: 0,
        },
    );
    snap.grid.set_cell(
        CONTENT_ROW,
        1,
        Cell {
            text: "B".to_string(),
            hl_id: 0,
        },
    );
    snap.grid.set_cell(
        CONTENT_ROW,
        GLYPH_CELL_COL,
        Cell {
            text: "C".to_string(),
            hl_id: DEFINED_HL_ID,
        },
    );
    // Wide-trailing placeholder: empty text, but still carries a highlight so its
    // background is filled. paint() must skip glyph rendering for empty text.
    snap.grid.set_cell(
        CONTENT_ROW,
        EMPTY_CELL_COL,
        Cell {
            text: String::new(),
            hl_id: DEFINED_HL_ID,
        },
    );

    snap
}

/// Measures `CellMetrics` against a throwaway 1x1 surface, then sizes the real
/// surface to exactly `COLS x ROWS` cells and paints the fixture onto it.
///
/// Returns the painted surface plus the metrics so the caller can compute cell
/// rectangles. The surface is returned `mut` so the caller can borrow its data.
fn render_fixture() -> (ImageSurface, CellMetrics) {
    // Metrics need a context to measure, but the measuring surface size is
    // irrelevant; create a minimal one.
    let probe = ImageSurface::create(Format::ARgb32, 1, 1).expect("probe surface");
    let probe_cr = Context::new(&probe).expect("probe context");
    let metrics = CellMetrics::for_font(&probe_cr, FONT_FAMILY, FONT_SIZE_PT);
    drop(probe_cr);

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

    paint(&cr, &fixture_snapshot(), &metrics);

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
