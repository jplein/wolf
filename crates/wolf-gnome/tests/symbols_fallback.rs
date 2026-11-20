//! The symbols-face fallback of `wolf_gnome::render`: the family list `CellMetrics::for_font`
//! paints with, the cell it measures, and the pixels the grid and a terminal pane paint for Nerd
//! Font icons the configured face lacks.
//!
//! ## Method
//!
//! Every pixel case paints the same content several times through the same painter, changing only
//! the family its description names ([`with_family`]), so glyph placement is identical by
//! construction:
//!
//! - **production**: the description `CellMetrics::for_font` built;
//! - **symbols direct**: [`SYMBOLS_FAMILY`] alone;
//! - **configured alone**: the configured family alone. That is the base commit's rendering, and so
//!   fontconfig's own fallback order with no symbols face named;
//! - **hex box**: drawn directly with pangocairo in the configured family alone, with Pango's
//!   `fallback` attribute off. Pango then uses the base font only, so a glyph it lacks draws as the
//!   missing-glyph box (`pango/itemize.c`, `get_font`).
//!
//! A code point's window is its own cell and the next one, inset one pixel, within its row. Every
//! fixture separates code points with two blank cells, so the next cell is blank and holds only
//! what the glyph reaches into it. Windows are compared as ink masks: a match is [`ink_distance`]
//! at most [`MATCH_MAX`], a difference is [`ink_xor`] at least [`DIFFER_MIN`], and every case
//! prints its table of both. The one-pixel tolerance of [`ink_distance`] makes a match robust to
//! anti-aliasing and sub-pixel placement, but it would also call a hex box drawn inside the solid
//! U+F07B at 16 pt nearly the same glyph, so a difference is measured exactly.
//!
//! ## Premises
//!
//! A message beginning `premise:` means the fixture cannot tell the outcomes apart, not that the
//! feature failed. Each is measured against the real faces of the devShell's pinned
//! `FONTCONFIG_FILE` before the assertions that rest on it. A run collects every failed premise of
//! its cases and reports them all at once, with the full table.
//!
//! - P1: for each icon, symbols direct and the hex box differ.
//! - P2: for U+F401, symbols direct and configured alone differ. This is the discriminator: the
//!   template's last-resort rule appends `DejaVu Sans` to every pattern and DejaVu Sans maps U+F401
//!   to a glyph of its own, so without the list U+F401 paints from DejaVu Sans. U+F07B cannot
//!   discriminate, because fontconfig alone already reaches the symbols face for it; it stays as the
//!   "an icon, not a hex box" check.
//! - P3: for the configured face's own glyphs (JetBrains Mono's U+E0A0, DejaVu Sans Mono's U+2665),
//!   configured alone and symbols direct differ.
//!
//! Which face fontconfig alone picks for U+E0A0 under iA Writer Mono S depends on its sort, so that
//! distance is printed and never asserted.

mod common;

use cairo::{Context, Format, ImageSurface};

use wolf_core::{
    Cell, DefaultColors, HlAttr, Rgb, Snapshot, TerminalCellWidth, TerminalFrame, TerminalTarget,
};
use wolf_gnome::editor_inset::GridRect;
use wolf_gnome::render::{family_list, SYMBOLS_FAMILY};
use wolf_gnome::terminal::canvas::TerminalCanvas;
use wolf_gnome::terminal::style::resolve;
use wolf_gnome::{paint, CellMetrics};

use common::{
    open_session, sh_script, start_core_sized, wait_for_text, DEFAULT_COLS, DEFAULT_ROWS,
};

// --- Families and code points ------------------------------------------------

/// The shipped default family, which carries no Nerd Font glyph.
const IA_WRITER: &str = "iA Writer Mono S";
/// The smokes' family, which carries U+2665 and no Nerd Font icon.
const DEJAVU_MONO: &str = "DejaVu Sans Mono";
/// A family carrying the Powerline glyphs itself, U+E0A0 among them.
const JETBRAINS_MONO: &str = "JetBrains Mono";
/// A family fontconfig cannot resolve, as `paint_golden.rs` names it: no pinned tree carries it.
const NO_SUCH_FAMILY: &str = "Wolf No Such Family 8f3a1c";
/// A symbols family no pinned tree carries, standing in for a checkout without the package.
const MISSING_SYMBOLS_FAMILY: &str = "Wolf Missing Symbols 3e9b";

/// `nf-fa-folder`: no configured family carries it, and fontconfig alone reaches the symbols face.
const FOLDER: &str = "\u{F07B}";
/// The Powerline branch glyph, which JetBrains Mono carries itself.
const POWERLINE: &str = "\u{E0A0}";
/// `nf-oct-repo`, the discriminator: fontconfig alone paints it from DejaVu Sans.
const REPO: &str = "\u{F401}";
/// A heart, which DejaVu Sans Mono carries itself.
const HEART: &str = "\u{2665}";
/// The ASCII cell, which every configured family carries and the symbols face does not.
const ASCII: &str = "M";

/// The fixture row: the ASCII cell, then each icon, each followed by two blank cells.
const ROW: [&str; 12] = [
    ASCII, " ", " ", FOLDER, " ", " ", POWERLINE, " ", " ", REPO, " ", " ",
];
/// The column of [`ASCII`] in [`ROW`].
const ASCII_COL: usize = 0;
/// The icons of [`ROW`], with their columns.
const ICONS: [(usize, &str); 3] = [(3, FOLDER), (6, POWERLINE), (9, REPO)];
/// The column of [`FOLDER`] in [`ROW`], whose overhang (d) reads.
const FOLDER_COL: usize = 3;
/// The column of [`REPO`] in [`ROW`].
const REPO_COL: usize = 9;

// --- Sizes and thresholds -----------------------------------------------------

/// The sizes the grid is measured and painted at: two, because the port's font-size change is
/// another `for_font` call.
const SIZES_PT: [f64; 2] = [16.0, 28.0];
/// The shipped default line height.
const LINE_HEIGHT_PERCENT: f64 = 180.0;
/// The size the smokes run at, and the line height at which their cell is the glyph box.
const SMOKE_SIZE_PT: f64 = 28.0;
const SMOKE_LINE_HEIGHT_PERCENT: f64 = 118.0;
/// The cell the smokes' `PANE_ORIGIN_*_EXPECTED` and `SMOKE_LINE_HEIGHT` were measured with, for
/// [`DEJAVU_MONO`] at [`SMOKE_SIZE_PT`] and [`SMOKE_LINE_HEIGHT_PERCENT`]. Literals on purpose.
const SMOKE_CELL_W: f64 = 22.4765625;
const SMOKE_CELL_H: f64 = 44.0;

/// The largest [`ink_distance`] that counts as the same glyph in the same place.
const MATCH_MAX: f64 = 0.02;
/// The smallest [`ink_xor`] that counts as a different glyph.
const DIFFER_MIN: f64 = 0.20;
/// How far past the cell's advance the symbols face's U+F07B ink must reach for (d) to have a pixel
/// to read: past the next cell's one-pixel inset, with a pixel to spare for its fractional edge.
const OVERHANG_MIN_PX: f64 = 3.0;

// --- Fixture colours ------------------------------------------------------------

/// The grid fixture's foreground and background, white on black.
const GRID_FG: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};
const GRID_BG: Rgb = Rgb { r: 0, g: 0, b: 0 };
/// The highlight ids of the bold and italic faces; 0 is the plain one.
const BOLD_HL_ID: u64 = 3;
const ITALIC_HL_ID: u64 = 4;

/// Where the terminal frame's rect starts on its surface: whole pixels, as `terminal_paint.rs`.
const ORIGIN: f64 = 8.0;
/// Surface margin to the right of and below the terminal rect.
const MARGIN: f64 = 8.0;

/// A pixel's `(r, g, b)`.
type Rgb8 = (u8, u8, u8);

fn rgb8(colour: Rgb) -> Rgb8 {
    (colour.r, colour.g, colour.b)
}

/// The foreground and background a window was painted in, which decide what is ink.
#[derive(Clone, Copy, Debug)]
struct Colours {
    fg: Rgb8,
    bg: Rgb8,
}

/// The face a cell is painted in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Face {
    Regular,
    Bold,
    Italic,
}

const FACES: [Face; 3] = [Face::Regular, Face::Bold, Face::Italic];

impl Face {
    fn bold(self) -> bool {
        self == Face::Bold
    }

    fn italic(self) -> bool {
        self == Face::Italic
    }

    fn hl_id(self) -> u64 {
        match self {
            Face::Regular => 0,
            Face::Bold => BOLD_HL_ID,
            Face::Italic => ITALIC_HL_ID,
        }
    }

    /// Toggles `font` as `render::GlyphPainter` toggles its clone of the base description.
    fn apply(self, font: &mut pango::FontDescription) {
        if self.bold() {
            font.set_weight(pango::Weight::Bold);
        }
        if self.italic() {
            font.set_style(pango::Style::Italic);
        }
    }
}

// --- Images, windows and ink ------------------------------------------------------

/// A painted surface's pixels, copied out.
struct Image {
    data: Vec<u8>,
    stride: usize,
    width: usize,
    height: usize,
}

impl Image {
    /// Copies `surface`'s pixels; no context may still hold it.
    fn read(mut surface: ImageSurface) -> Self {
        surface.flush();
        let stride = surface.stride() as usize;
        let width = surface.width() as usize;
        let height = surface.height() as usize;
        let data = surface.data().expect("surface data").to_vec();
        Self {
            data,
            stride,
            width,
            height,
        }
    }

    /// The `(r, g, b)` at `(x, y)`; ARGB32 is `B, G, R, A` in memory on little-endian hosts.
    fn pixel(&self, x: usize, y: usize) -> Rgb8 {
        let i = y * self.stride + x * 4;
        (self.data[i + 2], self.data[i + 1], self.data[i])
    }
}

/// A half-open rectangle of surface pixels: `x0 <= x < x1`, `y0 <= y < y1`.
#[derive(Clone, Copy, Debug)]
struct Window {
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
}

/// `cells` cells from `(col, row)` on a grid whose origin is `origin`, inset one pixel.
fn window(
    metrics: &CellMetrics,
    origin: (f64, f64),
    col: usize,
    row: usize,
    cells: usize,
) -> Window {
    let left = origin.0 + col as f64 * metrics.cell_w;
    let top = origin.1 + row as f64 * metrics.cell_h;
    let window = Window {
        x0: left.ceil() as usize + 1,
        x1: (left + cells as f64 * metrics.cell_w).floor() as usize - 1,
        y0: top.ceil() as usize + 1,
        y1: (top + metrics.cell_h).floor() as usize - 1,
    };
    assert!(
        window.x1 > window.x0 && window.y1 > window.y0,
        "premise: the window of {cells} cell(s) at ({col}, {row}) is empty: {window:?}"
    );
    window
}

/// Which pixels of a window are ink.
struct Mask {
    w: usize,
    h: usize,
    ink: Vec<bool>,
}

impl Mask {
    /// `window` of `image`: a pixel is ink when its largest channel distance from the background is
    /// more than half the background's largest channel distance from the foreground.
    fn of(image: &Image, window: Window, colours: Colours) -> Self {
        assert!(
            window.x1 <= image.width && window.y1 <= image.height,
            "premise: window {window:?} reaches outside the {}x{} image",
            image.width,
            image.height
        );
        let distance = |a: Rgb8, b: Rgb8| {
            a.0.abs_diff(b.0)
                .max(a.1.abs_diff(b.1))
                .max(a.2.abs_diff(b.2))
        };
        let threshold = f64::from(distance(colours.fg, colours.bg)) / 2.0;
        assert!(
            threshold > 0.0,
            "premise: the foreground and background are both {:?}, so no pixel can be ink",
            colours.bg
        );
        let mut ink = Vec::new();
        for y in window.y0..window.y1 {
            for x in window.x0..window.x1 {
                ink.push(f64::from(distance(image.pixel(x, y), colours.bg)) > threshold);
            }
        }
        Self {
            w: window.x1 - window.x0,
            h: window.y1 - window.y0,
            ink,
        }
    }

    fn count(&self) -> usize {
        self.ink.iter().filter(|&&ink| ink).count()
    }

    fn at(&self, x: usize, y: usize) -> bool {
        self.ink[y * self.w + x]
    }

    /// Whether any pixel within one pixel of `(x, y)`, Chebyshev distance, is ink.
    fn near(&self, x: usize, y: usize) -> bool {
        (x.saturating_sub(1)..=(x + 1).min(self.w - 1))
            .any(|nx| (y.saturating_sub(1)..=(y + 1).min(self.h - 1)).any(|ny| self.at(nx, ny)))
    }
}

/// The ink of `a` with no ink of `b` within one pixel, plus the same the other way, over the ink
/// of both: 0.0 for the same glyph in the same place, near 1.0 for glyphs that share no stroke.
/// Two empty windows are 0.0, so every case also checks that its reference holds ink.
fn ink_distance(a: &Mask, b: &Mask) -> f64 {
    assert!(
        a.w == b.w && a.h == b.h,
        "two windows of different sizes compared"
    );
    let stray = |p: &Mask, q: &Mask| {
        (0..p.h)
            .flat_map(|y| (0..p.w).map(move |x| (x, y)))
            .filter(|&(x, y)| p.at(x, y) && !q.near(x, y))
            .count()
    };
    let total = a.count() + b.count();
    if total == 0 {
        return 0.0;
    }
    (stray(a, b) + stray(b, a)) as f64 / total as f64
}

/// The pixels that are ink in exactly one of `a` and `b`, over the pixels that are ink in either:
/// the measure for "differs". It has no tolerance, so a glyph drawn inside a solid one, as the hex
/// box sits inside U+F07B, still counts as different where [`ink_distance`] reads them as near. It
/// is never below [`ink_distance`] for the same pair. Two empty windows are 0.0.
fn ink_xor(a: &Mask, b: &Mask) -> f64 {
    assert!(
        a.w == b.w && a.h == b.h,
        "two windows of different sizes compared"
    );
    let (one, either) = a
        .ink
        .iter()
        .zip(&b.ink)
        .fold((0usize, 0usize), |(one, either), (&p, &q)| {
            (one + usize::from(p != q), either + usize::from(p || q))
        });
    if either == 0 {
        return 0.0;
    }
    one as f64 / either as f64
}

// --- Painting -------------------------------------------------------------------

/// Measures the cell for `family` on a throwaway surface, as `paint_golden.rs` does.
fn probe_metrics(family: &str, size_pt: f64, line_height_percent: f64) -> CellMetrics {
    let probe = ImageSurface::create(Format::ARgb32, 1, 1).expect("probe surface");
    let cr = Context::new(&probe).expect("probe context");
    CellMetrics::for_font(&cr, family, size_pt, line_height_percent)
}

/// `metrics` with its description naming `family` alone, everything else unchanged.
fn with_family(metrics: &CellMetrics, family: &str) -> CellMetrics {
    let mut font = metrics.font.clone();
    font.set_family(family);
    CellMetrics { font, ..*metrics }
}

/// A one-row snapshot of `cells` in `face`, white on black.
fn row_snapshot(cells: &[&str], face: Face) -> Snapshot {
    let mut snap = Snapshot::new(cells.len(), 1);
    snap.highlights.set_defaults(DefaultColors {
        fg: Some(GRID_FG),
        bg: Some(GRID_BG),
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
    for (col, text) in cells.iter().enumerate() {
        snap.grid_mut(1).set_cell(
            0,
            col,
            Cell {
                text: (*text).to_owned(),
                hl_id: face.hl_id(),
            },
        );
    }
    snap
}

const GRID_COLOURS: Colours = Colours {
    fg: (GRID_FG.r, GRID_FG.g, GRID_FG.b),
    bg: (GRID_BG.r, GRID_BG.g, GRID_BG.b),
};

/// Paints `snap` with `wolf_gnome::paint` onto a surface exactly its size.
fn paint_grid(snap: &Snapshot, metrics: &CellMetrics) -> Image {
    let grid = snap.active_grid();
    let width = (metrics.cell_w * grid.cols as f64).ceil() as i32;
    let height = (metrics.cell_h * grid.rows as f64).ceil() as i32;
    let surface = ImageSurface::create(Format::ARgb32, width, height).expect("grid surface");
    let cr = Context::new(&surface).expect("grid context");
    paint(&cr, grid, &snap.highlights, metrics);
    drop(cr);
    Image::read(surface)
}

/// Draws each of `glyphs`, `(col, row, text)`, in `family` alone with Pango's fallback off, at the
/// cell position `render::GlyphPainter` uses, on a `width` x `height` surface of `colours.bg`.
#[allow(clippy::too_many_arguments)]
fn paint_hex_boxes(
    width: usize,
    height: usize,
    origin: (f64, f64),
    metrics: &CellMetrics,
    family: &str,
    face: Face,
    glyphs: &[(usize, usize, &str)],
    colours: Colours,
) -> Image {
    let surface =
        ImageSurface::create(Format::ARgb32, width as i32, height as i32).expect("hex-box surface");
    let cr = Context::new(&surface).expect("hex-box context");
    let set = |(r, g, b): Rgb8| {
        cr.set_source_rgb(
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
        );
    };
    set(colours.bg);
    cr.paint().expect("hex-box background");
    let layout = pangocairo::functions::create_layout(&cr);
    let mut font = metrics.font.clone();
    font.set_family(family);
    face.apply(&mut font);
    layout.set_font_description(Some(&font));
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrInt::new_fallback(false));
    layout.set_attributes(Some(&attrs));
    set(colours.fg);
    for &(col, row, text) in glyphs {
        layout.set_text(text);
        cr.move_to(
            origin.0 + col as f64 * metrics.cell_w,
            origin.1 + row as f64 * metrics.cell_h + metrics.baseline - metrics.ascent,
        );
        pangocairo::functions::show_layout(&cr, &layout);
    }
    drop(cr);
    Image::read(surface)
}

/// The right edge of the symbols face's ink for `text` in `face`, from the layout's origin.
fn symbols_ink_right(metrics: &CellMetrics, face: Face, text: &str) -> f64 {
    let probe = ImageSurface::create(Format::ARgb32, 1, 1).expect("probe surface");
    let cr = Context::new(&probe).expect("probe context");
    let layout = pangocairo::functions::create_layout(&cr);
    let mut font = metrics.font.clone();
    font.set_family(SYMBOLS_FAMILY);
    face.apply(&mut font);
    layout.set_font_description(Some(&font));
    layout.set_text(text);
    let (ink, _) = layout.pixel_extents();
    f64::from(ink.x() + ink.width())
}

/// One window's distances, for the printed table.
fn row_line(case: &str, what: &str, distances: &[(&str, f64)]) -> String {
    let mut line = format!("{case:<44} {what:<8}");
    for (name, d) in distances {
        line.push_str(&format!("  {name} {d:.3}"));
    }
    line.push('\n');
    line
}

/// The code point of a one-character string, for messages.
fn code(text: &str) -> String {
    text.chars()
        .map(|c| format!("U+{:04X}", u32::from(c)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Panics with every failed premise, one per line, then `table`, when any premise failed.
fn assert_premises(premises: &[String], table: &str) {
    assert!(
        premises.is_empty(),
        "premise: {} premise(s) failed, so the fixture cannot tell the outcomes apart; this is \
         not a verdict on the feature:\n{}\n{table}",
        premises.len(),
        premises.join("\n")
    );
}

// --- T1 -----------------------------------------------------------------------------

/// **The family list is the configured family, then the symbols face, with no duplicate.**
///
/// ## Falsifiers
///
/// - C1a: `family_list` returns `configured.to_owned()`; the first two rows go red.
/// - C1b: `family_list` always appends, with no duplicate check; the last four rows go red.
/// - C1c: `family_list` returns `format!("{SYMBOLS_FAMILY},{configured}")`; rows 1 and 2 go red.
#[test]
fn the_family_list_is_the_configured_family_then_the_symbols_face() {
    assert_eq!(
        SYMBOLS_FAMILY, "Symbols Nerd Font Mono",
        "SYMBOLS_FAMILY must be the family the vendored face's TTF names itself, which \
         `paint_golden.rs::the_pinned_stack_resolves_symbols_nerd_font_mono` resolves; any other \
         string is a family fontconfig cannot match, and the fallback silently never fires"
    );
    let table = [
        (IA_WRITER, "iA Writer Mono S,Symbols Nerd Font Mono"),
        (DEJAVU_MONO, "DejaVu Sans Mono,Symbols Nerd Font Mono"),
        ("Symbols Nerd Font Mono", "Symbols Nerd Font Mono"),
        ("symbols nerd font mono", "symbols nerd font mono"),
        ("SymbolsNerdFontMono", "SymbolsNerdFontMono"),
        (
            "Fira Code, Symbols Nerd Font Mono",
            "Fira Code, Symbols Nerd Font Mono",
        ),
    ];
    let mut wrong = Vec::new();
    for (input, expected) in table {
        let got = family_list(input);
        if got != expected {
            wrong.push(format!("{input:?} gave {got:?}, expected {expected:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "family_list broke its rule for {} of {} rows:\n{}\nA configured family alone means the \
         symbols face is never named and fontconfig's own sort decides every icon; the symbols \
         face FIRST means it shadows the configured face's own glyphs (a patched Nerd Font, \
         JetBrains Mono's Powerline) and its missing ASCII falls to fontconfig; a second copy of \
         the symbols face means the duplicate check does not compare members ignoring ASCII case \
         and whitespace, as fontconfig compares family names.",
        wrong.len(),
        table.len(),
        wrong.join("\n")
    );
}

// --- T2 -----------------------------------------------------------------------------

/// **The cell is measured from the configured family alone.**
///
/// For each family, size and line height, `for_font`'s advance, ascent and descent must be
/// exactly Pango's metrics for a description naming that family alone, from a context made the
/// way `for_font` makes it. And [`DEJAVU_MONO`] at the smokes' size and line height must still
/// give the literal cell every pointer smoke's targets were measured with.
///
/// ## Falsifier
///
/// - C2a: in `for_font`, measure (`context.metrics`) from a description naming only
///   `SYMBOLS_FAMILY`. The sample text then falls back to DejaVu Sans and `cell_w` moves.
#[test]
fn the_cell_is_measured_from_the_configured_family_alone() {
    let mut wrong = Vec::new();
    for family in [IA_WRITER, DEJAVU_MONO, JETBRAINS_MONO, NO_SUCH_FAMILY] {
        for size in SIZES_PT {
            let probe = ImageSurface::create(Format::ARgb32, 1, 1).expect("probe surface");
            let cr = Context::new(&probe).expect("probe context");
            let context = pangocairo::functions::create_context(&cr);
            let mut alone = pango::FontDescription::new();
            alone.set_family(family);
            alone.set_size((size * f64::from(pango::SCALE)) as i32);
            let pango_metrics = context.metrics(Some(&alone), None);
            let scale = f64::from(pango::SCALE);
            let expected = (
                f64::from(pango_metrics.approximate_char_width()) / scale,
                f64::from(pango_metrics.ascent()) / scale,
                f64::from(pango_metrics.descent()) / scale,
            );
            let m = probe_metrics(family, size, LINE_HEIGHT_PERCENT);
            let got = (m.cell_w, m.ascent, m.descent);
            if got != expected {
                wrong.push(format!(
                    "{family:?} at {size} pt: for_font gave (cell_w, ascent, descent) = {got:?}, \
                     the family alone measures {expected:?}"
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "the cell is not the configured family's own:\n{}\nfor_font must measure from a \
         description naming the configured family alone and only paint with the family list. \
         Pango's context metrics start from the fontset's first font and take the larger values \
         of every font the sample text falls back to, so measuring the list (or the symbols face) \
         lets the symbols face or its fallback move the cell, and with it every pointer smoke's \
         targets.",
        wrong.join("\n")
    );

    let smoke = probe_metrics(DEJAVU_MONO, SMOKE_SIZE_PT, SMOKE_LINE_HEIGHT_PERCENT);
    assert_eq!(
        (smoke.cell_w, smoke.cell_h),
        (SMOKE_CELL_W, SMOKE_CELL_H),
        "{DEJAVU_MONO:?} at {SMOKE_SIZE_PT} pt and {SMOKE_LINE_HEIGHT_PERCENT} % measures a {} x {} \
         cell, not the {SMOKE_CELL_W} x {SMOKE_CELL_H} that PANE_ORIGIN_*_EXPECTED and \
         SMOKE_LINE_HEIGHT were measured with. The family list reached the measurement, or the \
         pinned face changed; do not re-measure the smoke constants to absorb it.",
        smoke.cell_w,
        smoke.cell_h
    );
}

// --- T3 -----------------------------------------------------------------------------

/// **The grid paints Nerd Font icons from the symbols face.**
///
/// For iA Writer Mono S, DejaVu Sans Mono and an unresolvable family, in each face, at 16 and
/// 28 pt, one row of [`ROW`] painted with `wolf_gnome::paint`:
///
/// - (a) each icon's window matches symbols direct and differs from the hex box;
/// - (b) U+F401's window differs from configured alone;
/// - (c) the ASCII cell's window matches configured alone;
/// - (d) the cell after U+F07B holds ink, so the row's backgrounds are filled before its glyphs;
/// - (e) a list naming a symbols family no tree carries matches configured alone in every window,
///   which is the missing-package case degrading to the base rendering. A premise of D2 with no
///   falsifier of its own.
///
/// U+E0A0's distance to configured alone is printed only.
///
/// ## Falsifiers
///
/// - C3a: `family_list` returns `configured.to_owned()`; (a) and (b) go red at U+F401.
/// - C3b: `paint` fills each cell and draws its glyph in one pass, the base commit's loop; (d) goes
///   red.
/// - C3c: `family_list` returns `SYMBOLS_FAMILY.to_owned()`; (c) goes red, because `M` then comes
///   from DejaVu Sans.
/// - C3d: in `GlyphPainter::draw`, just before `set_source(cr, fg)`, set on the layout an attribute
///   list holding only `pango::AttrInt::new_fallback(false)`. Production then draws the configured
///   face's hex box for every icon it lacks, and (a)'s "from the hex box" goes red for every icon
///   of iA Writer Mono S and DejaVu Sans Mono.
#[test]
fn the_grid_paints_nerd_glyphs_from_the_symbols_face() {
    let mut table = String::from(
        "grid: distances from production (prod) and between references, per window; `~` is the \
         one-pixel-tolerant ink distance, `^` the exact XOR\n",
    );
    let mut premises = Vec::new();
    let mut failures = Vec::new();
    for family in [IA_WRITER, DEJAVU_MONO, NO_SUCH_FAMILY] {
        for face in FACES {
            for size in SIZES_PT {
                let case = format!("{family} / {face:?} / {size} pt");
                let metrics = probe_metrics(family, size, LINE_HEIGHT_PERCENT);
                let snap = row_snapshot(&ROW, face);
                let production = paint_grid(&snap, &metrics);
                let symbols = paint_grid(&snap, &with_family(&metrics, SYMBOLS_FAMILY));
                let alone = paint_grid(&snap, &with_family(&metrics, family));
                let missing_list = format!("{family},{MISSING_SYMBOLS_FAMILY}");
                let missing = paint_grid(&snap, &with_family(&metrics, &missing_list));
                let glyphs: Vec<(usize, usize, &str)> =
                    ICONS.iter().map(|&(col, text)| (col, 0, text)).collect();
                let hex = paint_hex_boxes(
                    production.width,
                    production.height,
                    (0.0, 0.0),
                    &metrics,
                    family,
                    face,
                    &glyphs,
                    GRID_COLOURS,
                );
                let mask = |image: &Image, col: usize, cells: usize| {
                    Mask::of(
                        image,
                        window(&metrics, (0.0, 0.0), col, 0, cells),
                        GRID_COLOURS,
                    )
                };

                for &(col, text) in &ICONS {
                    let own = mask(&symbols, col, 2);
                    let prod = mask(&production, col, 2);
                    let hex_box = mask(&hex, col, 2);
                    let configured = mask(&alone, col, 2);
                    let d_symbols = ink_distance(&prod, &own);
                    let d_hex = ink_xor(&prod, &hex_box);
                    let d_alone = ink_xor(&prod, &configured);
                    let p1 = ink_xor(&own, &hex_box);
                    let d_missing = ink_distance(&mask(&missing, col, 2), &configured);
                    let mut distances = vec![
                        ("prod~symbols", d_symbols),
                        ("prod^hex", d_hex),
                        ("prod^alone", d_alone),
                        ("symbols^hex", p1),
                        ("missing~alone", d_missing),
                    ];
                    let p2 = (col == REPO_COL).then(|| ink_xor(&own, &configured));
                    if let Some(p2) = p2 {
                        distances.push(("symbols^alone", p2));
                    }
                    table.push_str(&row_line(&case, &code(text), &distances));

                    if own.count() == 0 {
                        premises.push(format!(
                            "{case}: symbols direct paints no ink for {}, so {SYMBOLS_FAMILY:?} \
                             does not draw it at this size and nothing can tell its glyph from an \
                             empty cell",
                            code(text)
                        ));
                    }
                    if p1 < DIFFER_MIN {
                        premises.push(format!(
                            "{case}: P1, symbols direct and the hex box for {} are {p1:.3} apart \
                             (^), under {DIFFER_MIN}, so an icon cannot be told from a missing \
                             glyph",
                            code(text)
                        ));
                    }
                    if let Some(p2) = p2 {
                        if p2 < DIFFER_MIN {
                            premises.push(format!(
                                "{case}: P2, symbols direct and configured alone for U+F401 are \
                                 {p2:.3} apart (^), under {DIFFER_MIN}, so fontconfig alone \
                                 already paints it from the symbols face and the list cannot be \
                                 told from fontconfig's own fallback here"
                            ));
                        }
                        if d_alone < DIFFER_MIN {
                            failures.push(format!(
                                "(b) {case}: U+F401 is {d_alone:.3} from configured alone, under \
                                 {DIFFER_MIN}: it paints from fontconfig's own choice (DejaVu \
                                 Sans), so the family list does not name the symbols face after \
                                 the configured family"
                            ));
                        }
                    }
                    if d_symbols > MATCH_MAX {
                        failures.push(format!(
                            "(a) {case}: {} is {d_symbols:.3} from symbols direct, over \
                             {MATCH_MAX}: it is not painted from {SYMBOLS_FAMILY:?}",
                            code(text)
                        ));
                    }
                    if d_hex < DIFFER_MIN {
                        failures.push(format!(
                            "(a) {case}: {} is {d_hex:.3} from the hex box, under {DIFFER_MIN}: \
                             it paints as a missing glyph",
                            code(text)
                        ));
                    }
                    if d_missing > MATCH_MAX {
                        failures.push(format!(
                            "(e) {case}: with {missing_list:?}, {} is {d_missing:.3} from \
                             configured alone, over {MATCH_MAX}: a missing symbols family changes \
                             the rendering instead of being skipped",
                            code(text)
                        ));
                    }
                }

                let prod = mask(&production, ASCII_COL, 2);
                let configured = mask(&alone, ASCII_COL, 2);
                let d_alone = ink_distance(&prod, &configured);
                let d_missing = ink_distance(&mask(&missing, ASCII_COL, 2), &configured);
                table.push_str(&row_line(
                    &case,
                    ASCII,
                    &[("prod~alone", d_alone), ("missing~alone", d_missing)],
                ));
                if configured.count() == 0 {
                    premises.push(format!(
                        "{case}: configured alone paints no ink for {ASCII:?}"
                    ));
                }
                if d_alone > MATCH_MAX {
                    failures.push(format!(
                        "(c) {case}: {ASCII:?} is {d_alone:.3} from configured alone, over \
                         {MATCH_MAX}: the configured face no longer paints ASCII, so the symbols \
                         face comes first in the list and its missing ASCII falls to fontconfig"
                    ));
                }
                if d_missing > MATCH_MAX {
                    failures.push(format!(
                        "(e) {case}: with {missing_list:?}, {ASCII:?} is {d_missing:.3} from \
                         configured alone, over {MATCH_MAX}"
                    ));
                }

                let reach = symbols_ink_right(&metrics, face, FOLDER);
                if reach < metrics.cell_w + OVERHANG_MIN_PX {
                    premises.push(format!(
                        "{case}: the symbols face's U+F07B ink ends {reach} px from the cell's \
                         left edge, not {OVERHANG_MIN_PX} px past the {} px cell, so it does not \
                         reach the next cell and (d) cannot tell a one-pass paint from two passes",
                        metrics.cell_w
                    ));
                }
                let overhang = mask(&production, FOLDER_COL + 1, 1).count();
                if overhang == 0 {
                    failures.push(format!(
                        "(d) {case}: the cell after U+F07B holds no ink although the glyph's ink \
                         reaches {reach} px into a {} px cell: the next cell's background was \
                         filled over it, so `paint` does not fill a row's backgrounds before \
                         drawing its glyphs",
                        metrics.cell_w
                    ));
                }
            }
        }
    }
    eprintln!("{table}");
    assert_premises(&premises, &table);
    assert!(
        failures.is_empty(),
        "{} grid check(s) failed:\n{}\n{table}",
        failures.len(),
        failures.join("\n")
    );
}

// --- T4 -----------------------------------------------------------------------------

/// **A configured face keeps its own glyphs.** JetBrains Mono's U+E0A0 and DejaVu Sans Mono's
/// U+2665 paint from the configured face, not from the symbols face, which carries both.
///
/// ## Falsifier
///
/// - C4a: `family_list` returns `format!("{SYMBOLS_FAMILY},{configured}")`; both cells go red.
#[test]
fn a_configured_face_keeps_its_own_glyphs() {
    let mut table = String::from(
        "configured glyphs: distances; `~` is the one-pixel-tolerant ink distance, `^` the exact \
         XOR\n",
    );
    let mut premises = Vec::new();
    let mut failures = Vec::new();
    for (family, text) in [(JETBRAINS_MONO, POWERLINE), (DEJAVU_MONO, HEART)] {
        let case = format!("{family} / {SMOKE_SIZE_PT} pt");
        let metrics = probe_metrics(family, SMOKE_SIZE_PT, LINE_HEIGHT_PERCENT);
        let snap = row_snapshot(&[text, " ", " "], Face::Regular);
        let mask = |m: &CellMetrics| {
            Mask::of(
                &paint_grid(&snap, m),
                window(&metrics, (0.0, 0.0), 0, 0, 2),
                GRID_COLOURS,
            )
        };
        let prod = mask(&metrics);
        let configured = mask(&with_family(&metrics, family));
        let symbols = mask(&with_family(&metrics, SYMBOLS_FAMILY));
        let d_alone = ink_distance(&prod, &configured);
        let d_symbols = ink_xor(&prod, &symbols);
        let p3 = ink_xor(&configured, &symbols);
        table.push_str(&row_line(
            &case,
            &code(text),
            &[
                ("prod~alone", d_alone),
                ("prod^symbols", d_symbols),
                ("alone^symbols", p3),
            ],
        ));
        if configured.count() == 0 || p3 < DIFFER_MIN {
            premises.push(format!(
                "{case}: P3, configured alone ({} ink pixels) and symbols direct for {} are \
                 {p3:.3} apart (^), under {DIFFER_MIN} or empty, so the two faces draw it alike \
                 and this case cannot tell which one painted it",
                configured.count(),
                code(text)
            ));
        }
        if d_alone > MATCH_MAX || d_symbols < DIFFER_MIN {
            failures.push(format!(
                "{case}: {} is {d_alone:.3} from configured alone (match is at most {MATCH_MAX}) \
                 and {d_symbols:.3} from symbols direct (a difference is at least {DIFFER_MIN}): \
                 the symbols face shadows the configured face's own glyph, so it is ahead of the \
                 configured family in the list",
                code(text)
            ));
        }
    }
    eprintln!("{table}");
    assert_premises(&premises, &table);
    assert!(failures.is_empty(), "{}\n{table}", failures.join("\n"));
}

// --- T5 -----------------------------------------------------------------------------

/// The UTF-8 octal escapes `printf` writes for each icon.
const FOLDER_OCTAL: &str = "\\357\\201\\273";
const POWERLINE_OCTAL: &str = "\\356\\202\\240";
const REPO_OCTAL: &str = "\\357\\220\\201";

/// The script printing [`ROW`] plain, bold and italic, then the marker `zz`.
fn terminal_script() -> String {
    let line = format!("M  {FOLDER_OCTAL}  {POWERLINE_OCTAL}  {REPO_OCTAL}  ");
    format!("printf '{line}\\n\\033[1m{line}\\033[0m\\n\\033[3m{line}\\033[0m\\nzz'; exec cat")
}

/// One fixture row of the terminal frame.
struct TerminalRow {
    row: u16,
    /// The column of the row's `M`.
    col: u16,
    face: Face,
    colours: Colours,
}

/// Paints `frame` through a fresh `TerminalCanvas` at [`ORIGIN`], unfocused.
fn paint_terminal(frame: &TerminalFrame, metrics: &CellMetrics) -> Image {
    let w = (f64::from(frame.cols) * metrics.cell_w).ceil();
    let h = (f64::from(frame.rows) * metrics.cell_h).ceil();
    let rect = GridRect {
        x: ORIGIN,
        y: ORIGIN,
        w,
        h,
    };
    let width = (ORIGIN + w + MARGIN).ceil() as i32;
    let height = (ORIGIN + h + MARGIN).ceil() as i32;
    let surface = ImageSurface::create(Format::ARgb32, width, height).expect("terminal surface");
    let cr = Context::new(&surface).expect("terminal context");
    let mut canvas = TerminalCanvas::new();
    canvas.update(&cr, frame, metrics, 1.0);
    canvas.paint(&cr, rect, frame, metrics, false);
    drop(cr);
    Image::read(surface)
}

/// Finds the three fixture rows and checks each icon's cell, as premises.
fn terminal_rows(frame: &TerminalFrame) -> Vec<TerminalRow> {
    let mut rows = Vec::new();
    for row in 0..frame.rows {
        let Some(col) =
            (0..frame.cols).find(|&col| frame.cell(col, row).is_some_and(|c| c.text == ASCII))
        else {
            continue;
        };
        let cell = frame.cell(col, row).expect("the M cell");
        let face = match (cell.bold, cell.italic) {
            (false, false) => Face::Regular,
            (true, false) => Face::Bold,
            (false, true) => Face::Italic,
            other => panic!("premise: row {row}'s M is (bold, italic) = {other:?}"),
        };
        let paint = resolve(cell, false, frame.default_fg, frame.default_bg);
        rows.push(TerminalRow {
            row,
            col,
            face,
            colours: Colours {
                fg: rgb8(paint.foreground),
                bg: rgb8(paint.background),
            },
        });
    }
    let faces: Vec<Face> = rows.iter().map(|r| r.face).collect();
    assert_eq!(
        faces,
        FACES,
        "premise: the session's frame holds M rows in faces {faces:?}, not one plain, one bold and \
         one italic row in that order, so the script's SGR did not reach the frame as written"
    );
    for r in &rows {
        for &(offset, text) in &ICONS {
            let col = r.col + offset as u16;
            let cell = frame.cell(col, r.row);
            let next = frame.cell(col + 1, r.row);
            assert!(
                cell.is_some_and(|c| c.text == text && c.width == TerminalCellWidth::Narrow)
                    && next.is_some_and(|c| c.text.trim().is_empty()),
                "premise: row {} column {col} must hold {} as a narrow cell followed by a blank \
                 one, but holds {:?} followed by {:?}. A wide cell would paint across both \
                 cells of the window whatever the face.",
                r.row,
                code(text),
                cell.map(|c| (&c.text, c.width)),
                next.map(|c| &c.text)
            );
        }
    }
    rows
}

/// **A terminal pane paints Nerd Font icons from the symbols face.**
///
/// A real core and a `/bin/sh` session print [`ROW`] plain, bold and italic. The frame is painted
/// through `TerminalCanvas` for DejaVu Sans Mono and iA Writer Mono S at 16 pt, and (a), (b) and
/// (c) of [`the_grid_paints_nerd_glyphs_from_the_symbols_face`] hold for every row, against
/// references painted by fresh canvases over the same frame.
///
/// ## Falsifiers
///
/// - C5a: `family_list` returns `configured.to_owned()`; U+F401 goes red.
/// - C5b: the edit of C3d, the layout's fallback turned off in `GlyphPainter::draw`; (a)'s "from
///   the hex box" goes red for both families.
#[test]
fn a_terminal_pane_paints_nerd_glyphs_from_the_symbols_face() {
    let (core, waiter) = start_core_sized(DEFAULT_COLS, DEFAULT_ROWS);
    let id = open_session(
        &core,
        TerminalTarget::Window(0),
        sh_script(&terminal_script()),
    );
    let frame = wait_for_text(&core, &waiter, id, "zz");
    let rows = terminal_rows(&frame);

    let mut table = String::from(
        "terminal: distances from production (prod) and between references; `~` is the \
         one-pixel-tolerant ink distance, `^` the exact XOR\n",
    );
    let mut premises = Vec::new();
    let mut failures = Vec::new();
    for family in [DEJAVU_MONO, IA_WRITER] {
        let size = SIZES_PT[0];
        let metrics = probe_metrics(family, size, LINE_HEIGHT_PERCENT);
        let production = paint_terminal(&frame, &metrics);
        let symbols = paint_terminal(&frame, &with_family(&metrics, SYMBOLS_FAMILY));
        let alone = paint_terminal(&frame, &with_family(&metrics, family));
        for r in &rows {
            let case = format!("{family} / {:?} / {size} pt / row {}", r.face, r.row);
            let glyphs: Vec<(usize, usize, &str)> = ICONS
                .iter()
                .map(|&(offset, text)| (usize::from(r.col) + offset, usize::from(r.row), text))
                .collect();
            let hex = paint_hex_boxes(
                production.width,
                production.height,
                (ORIGIN, ORIGIN),
                &metrics,
                family,
                r.face,
                &glyphs,
                r.colours,
            );
            let mask = |image: &Image, col: usize| {
                Mask::of(
                    image,
                    window(&metrics, (ORIGIN, ORIGIN), col, usize::from(r.row), 2),
                    r.colours,
                )
            };
            for &(offset, text) in &ICONS {
                let col = usize::from(r.col) + offset;
                let own = mask(&symbols, col);
                let prod = mask(&production, col);
                let hex_box = mask(&hex, col);
                let configured = mask(&alone, col);
                let d_symbols = ink_distance(&prod, &own);
                let d_hex = ink_xor(&prod, &hex_box);
                let d_alone = ink_xor(&prod, &configured);
                let p1 = ink_xor(&own, &hex_box);
                let mut distances = vec![
                    ("prod~symbols", d_symbols),
                    ("prod^hex", d_hex),
                    ("prod^alone", d_alone),
                    ("symbols^hex", p1),
                ];
                let p2 = (text == REPO).then(|| ink_xor(&own, &configured));
                if let Some(p2) = p2 {
                    distances.push(("symbols^alone", p2));
                }
                table.push_str(&row_line(&case, &code(text), &distances));
                if own.count() == 0 || p1 < DIFFER_MIN {
                    premises.push(format!(
                        "{case}: P1, symbols direct ({} ink pixels) and the hex box for {} are \
                         {p1:.3} apart (^), under {DIFFER_MIN} or empty, so an icon cannot be \
                         told from a missing glyph",
                        own.count(),
                        code(text)
                    ));
                }
                if let Some(p2) = p2 {
                    if p2 < DIFFER_MIN {
                        premises.push(format!(
                            "{case}: P2, symbols direct and configured alone for U+F401 are \
                             {p2:.3} apart (^), under {DIFFER_MIN}, so the list cannot be told \
                             from fontconfig's own fallback here"
                        ));
                    }
                    if d_alone < DIFFER_MIN {
                        failures.push(format!(
                            "(b) {case}: U+F401 is {d_alone:.3} from configured alone: the pane \
                             paints it from fontconfig's own choice, so the family list does not \
                             reach `CellMetrics::font`"
                        ));
                    }
                }
                if d_symbols > MATCH_MAX {
                    failures.push(format!(
                        "(a) {case}: {} is {d_symbols:.3} from symbols direct, over {MATCH_MAX}",
                        code(text)
                    ));
                }
                if d_hex < DIFFER_MIN {
                    failures.push(format!(
                        "(a) {case}: {} is {d_hex:.3} from the hex box: it paints as a missing \
                         glyph",
                        code(text)
                    ));
                }
            }
            let prod = mask(&production, usize::from(r.col));
            let configured = mask(&alone, usize::from(r.col));
            let d_alone = ink_distance(&prod, &configured);
            table.push_str(&row_line(&case, ASCII, &[("prod~alone", d_alone)]));
            if configured.count() == 0 {
                premises.push(format!(
                    "{case}: configured alone paints no ink for {ASCII:?}"
                ));
            }
            if d_alone > MATCH_MAX {
                failures.push(format!(
                    "(c) {case}: {ASCII:?} is {d_alone:.3} from configured alone, over \
                     {MATCH_MAX}: the configured face no longer paints ASCII"
                ));
            }
        }
    }
    eprintln!("{table}");
    assert_premises(&premises, &table);
    assert!(
        failures.is_empty(),
        "{} terminal check(s) failed:\n{}\n{table}",
        failures.len(),
        failures.join("\n")
    );
}
