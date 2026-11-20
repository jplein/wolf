//! The symbols-face smoke: under cage, Nerd Font icons the configured face lacks paint from
//! Symbols Nerd Font Mono, in an editor pane and in a terminal pane, while ASCII still paints from
//! the configured face.

use super::*;

use cairo::{Content, RecordingSurface};
use wolf_core::{Cell, DefaultColors, Rgb, Snapshot};
use wolf_gnome::render::SYMBOLS_FAMILY;

/// The fixture line: `M`, then each icon, each followed by two blanks. U+F07B is `nf-fa-folder`,
/// U+E0A0 the Powerline branch and U+F401 `nf-oct-repo`, none of which DejaVu Sans Mono carries.
const SY_LINE: [&str; 12] = [
    "M", " ", " ", "\u{F07B}", " ", " ", "\u{E0A0}", " ", " ", "\u{F401}", " ", " ",
];
/// The column of the `M`.
const SY_ASCII_COL: usize = 0;
/// The icons' columns in [`SY_LINE`].
const SY_FOLDER_COL: usize = 3;
const SY_POWERLINE_COL: usize = 6;
const SY_REPO_COL: usize = 9;
/// The second blank after the `M`, whose centre gives the background.
const SY_BG_COL: usize = 2;

/// The grid row the editor shows the line on: `setline` puts it on buffer line 2, under an empty
/// line 1 that keeps the cursor off it.
const SY_EDITOR_ROW: usize = 1;
/// The terminal row the script prints the line on.
const SY_TERMINAL_ROW: usize = 0;

/// The line's code points, as the readback reports `str2list(getline(2))`.
const SY_LINE_CODE_POINTS: &str = "77 32 32 61563 32 32 57504 32 32 62465 32 32";

/// The largest [`sy_distance`] that counts as a match in a capture.
const SY_MATCH_MAX: f64 = 0.10;
/// The smallest [`sy_xor`] that counts as a difference in a capture.
const SY_DIFFER_MIN: f64 = 0.25;
/// How many times farther, by [`sy_xor`], a capture must be from every other reference than it is,
/// by [`sy_distance`], from its own.
const SY_MIN_RATIO: f64 = 2.0;

/// The cell [`cell_metrics_for_smoke`] must give, as `paint_golden.rs` measured it. Literals.
const SY_CELL_W: f64 = 22.4765625;
const SY_CELL_H: f64 = 44.0;

/// The script, readback, marker and captures, all under `XDG_RUNTIME_DIR` so no sidebar row
/// draws them.
const SY_SHELL_NAME: &str = "wolf_gnome_sy_shell.sh";
const SY_READBACK_NAME: &str = "wolf_gnome_sy_readback.txt";
const SY_PAINTED_NAME: &str = "wolf_gnome_sy_painted.txt";
const SY_PNG_EDITOR_NAME: &str = "wolf_gnome_sy_editor.png";
const SY_PNG_TERMINAL_NAME: &str = "wolf_gnome_sy_terminal.png";

/// The `SHELL` a typed `:terminal` runs: the fixture line on row 0, then the marker, then a sleep
/// that keeps the session open until the app goes.
const SY_SHELL: &str = "#!/bin/sh\n\
printf 'M  \\357\\201\\273  \\356\\202\\240  \\357\\220\\201  \\r\\n'\n\
echo painted > \"$SY_PAINTED\"\n\
exec sleep 600\n";

/// The SYMBOLS scenario: one launch. It sets buffer line 2 to the fixture with `nr2char`, so no
/// non-ASCII key is typed, reads back the text offset and the line's code points, settles and
/// captures the editor; then it types `:terminal`, waits for the script's marker, settles and
/// captures the terminal. The launch ends with a `kill`: the terminal pane has the keys.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input.
/// * `12` — the readback never appeared.
/// * `13` — the shell never wrote the marker: no session opened on `:terminal`.
/// * `40` — the launch never became ready (the shared convention).
const SY_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
FILE_TRIES=100

cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

wait_file() {
    f=$1
    t=0
    while [ "$t" -lt "$FILE_TRIES" ]; do
        if [ -s "$f" ]; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome symbols smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome symbols smoke"

wtype -- ":call setline(1, ['', 'M  ' . nr2char(0xF07B) . '  ' . nr2char(0xE0A0) . '  ' . nr2char(0xF401) . '  '])"
wtype -k Return
wtype -- ":set nomodified"
wtype -k Return
wtype -- ":call writefile([string(getwininfo(win_getid())[0].textoff), join(str2list(getline(2)), ' ')], '$SY_READBACK')"
wtype -k Return
wait_file "$SY_READBACK" || exit 12
sleep "$SETTLE"
grim "$SY_PNG_EDITOR"

wtype -- ":terminal"
wtype -k Return
wait_file "$SY_PAINTED" || exit 13
sleep "$SETTLE"
grim "$SY_PNG_TERMINAL"

kill "$app_pid" 2>/dev/null || true
wait "$app_pid" 2>/dev/null || true
app_pid=
exit 0
"#;

/// The editor pane's text-grid origin in output pixels, unrounded, as `pane_view` translates the
/// grid paint by it: `editor_inset::grid_rect` over the pane area at
/// [`OVERLAY_ORIGIN_X_EXPECTED`] and [`OVERLAY_ORIGIN_Y_EXPECTED`], uncapped, which
/// `the_column_cap_cannot_bind_at_the_ocr_smokes_geometry` makes equivalent to capped here.
fn sy_grid_origin() -> (f64, f64) {
    let cell = cell_metrics_for_smoke();
    let statusbar = wolf_gnome::statusbar_layout::statusbar_layout(
        cell.size_px,
        wolf_gnome::statusbar_layout::secondary_font_size(cell.size_px),
        wolf_gnome::statusbar_layout::STATUSBAR_VERTICAL_PADDING,
        wolf_gnome::statusbar_layout::STATUSBAR_LINE_GAP,
    );
    let pane_w =
        exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED") - OVERLAY_ORIGIN_X_EXPECTED;
    let pane_h =
        exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED") - OVERLAY_ORIGIN_Y_EXPECTED;
    let rect = wolf_gnome::editor_inset::grid_rect(
        pane_w,
        pane_h,
        statusbar.height,
        wolf_gnome::editor_inset::EDITOR_PADDING_PX,
        wolf_gnome::editor_inset::column_cap(true, cell.cell_w),
    );
    (
        OVERLAY_ORIGIN_X_EXPECTED + rect.x,
        OVERLAY_ORIGIN_Y_EXPECTED + rect.y,
    )
}

/// A half-open rectangle of pixels in one image's own coordinates.
#[derive(Clone, Copy, Debug)]
struct SyWindow {
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
}

/// `cells` cells from `(col, row)` of a grid whose origin is at `origin`, inset one pixel.
fn sy_window(origin: (f64, f64), col: usize, row: usize, cells: usize) -> SyWindow {
    let cell = cell_metrics_for_smoke();
    let left = origin.0 + col as f64 * cell.cell_w;
    let top = origin.1 + row as f64 * cell.cell_h;
    SyWindow {
        x0: pixel_index(left.ceil() + 1.0, "a window's left edge"),
        x1: pixel_index(
            (left + cells as f64 * cell.cell_w).floor() - 1.0,
            "a window's right edge",
        ),
        y0: pixel_index(top.ceil() + 1.0, "a window's top edge"),
        y1: pixel_index((top + cell.cell_h).floor() - 1.0, "a window's bottom edge"),
    }
}

/// Anything whose pixels can be read: a capture, or a reference painted here.
trait SyPixels {
    fn rgb(&self, x: usize, y: usize) -> Rgb8;
}

impl SyPixels for Capture {
    fn rgb(&self, x: usize, y: usize) -> Rgb8 {
        self.pixel(x, y)
    }
}

/// A reference painted in this process: ARGB32 pixels, `B, G, R, A` in memory.
struct SyImage {
    data: Vec<u8>,
    stride: usize,
}

impl SyPixels for SyImage {
    fn rgb(&self, x: usize, y: usize) -> Rgb8 {
        let i = y * self.stride + x * 4;
        (self.data[i + 2], self.data[i + 1], self.data[i])
    }
}

/// Which pixels of a window are ink.
struct SyMask {
    w: usize,
    h: usize,
    ink: Vec<bool>,
}

fn sy_channel_distance(a: Rgb8, b: Rgb8) -> u8 {
    a.0.abs_diff(b.0)
        .max(a.1.abs_diff(b.1))
        .max(a.2.abs_diff(b.2))
}

impl SyMask {
    /// A pixel is ink when its largest channel distance from `bg` is more than half `fg`'s.
    fn of(image: &dyn SyPixels, window: SyWindow, fg: Rgb8, bg: Rgb8) -> Self {
        let threshold = f64::from(sy_channel_distance(fg, bg)) / 2.0;
        let mut ink = Vec::new();
        for y in window.y0..window.y1 {
            for x in window.x0..window.x1 {
                ink.push(f64::from(sy_channel_distance(image.rgb(x, y), bg)) > threshold);
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

    fn near(&self, x: usize, y: usize) -> bool {
        (x.saturating_sub(1)..=(x + 1).min(self.w - 1))
            .any(|nx| (y.saturating_sub(1)..=(y + 1).min(self.h - 1)).any(|ny| self.at(nx, ny)))
    }
}

/// The ink of `a` with no ink of `b` within one pixel (Chebyshev), plus the same the other way,
/// over the ink of both: the measure for matches, which `tests/symbols_fallback.rs` uses too.
fn sy_distance(a: &SyMask, b: &SyMask) -> f64 {
    let stray = |p: &SyMask, q: &SyMask| {
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
/// the measure for differences, as `ink_xor` in `tests/symbols_fallback.rs`. It has no tolerance,
/// so a hex box drawn inside a solid glyph still counts as different.
fn sy_xor(a: &SyMask, b: &SyMask) -> f64 {
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

/// How a reference is rasterized, matching the surface the capture's pane painted on.
#[derive(Clone, Copy, Debug)]
enum SySurface {
    /// The editor grid: GTK hands a `DrawingArea` a context on a recording surface, which turns
    /// off hinting and metric hinting, translated by the unrounded grid origin.
    Recording,
    /// The terminal canvas: an image surface, blitted at the rounded origin.
    Image,
}

/// Rasterizes `draw` the way `kind` says, with the grid origin at `offset` inside a
/// `width` x `height` image.
fn sy_rasterize(
    kind: SySurface,
    offset: (f64, f64),
    width: i32,
    height: i32,
    draw: impl FnOnce(&Context),
) -> SyImage {
    let mut image =
        ImageSurface::create(Format::ARgb32, width, height).expect("reference image surface");
    match kind {
        SySurface::Recording => {
            let recording =
                RecordingSurface::create(Content::ColorAlpha, None).expect("recording surface");
            let cr = Context::new(&recording).expect("recording context");
            cr.translate(offset.0, offset.1);
            draw(&cr);
            drop(cr);
            let replay = Context::new(&image).expect("replay context");
            replay
                .set_source_surface(&recording, 0.0, 0.0)
                .expect("replay source");
            replay.paint().expect("replay");
        }
        SySurface::Image => {
            let cr = Context::new(&image).expect("reference context");
            cr.translate(offset.0, offset.1);
            draw(&cr);
        }
    }
    image.flush();
    let stride = usize::try_from(image.stride()).expect("a positive stride");
    let data = image.data().expect("reference pixels").to_vec();
    SyImage { data, stride }
}

/// The fixture line on grid row `row` of a snapshot `row + 1` rows tall, in `fg` on `bg`.
fn sy_snapshot(row: usize, fg: Rgb8, bg: Rgb8) -> Snapshot {
    let rgb = |(r, g, b): Rgb8| Rgb { r, g, b };
    let mut snap = Snapshot::new(SY_LINE.len(), row + 1);
    snap.highlights.set_defaults(DefaultColors {
        fg: Some(rgb(fg)),
        bg: Some(rgb(bg)),
        special: None,
    });
    for (col, text) in SY_LINE.iter().enumerate() {
        snap.grid_mut(1).set_cell(
            row,
            col,
            Cell {
                text: (*text).to_owned(),
                hl_id: 0,
            },
        );
    }
    snap
}

/// The four references of one capture, and where its grid sits in them.
struct SyReferences {
    configured: SyImage,
    symbols: SyImage,
    hex: SyImage,
    /// The reference images' origin, which [`sy_window`] takes.
    origin: (f64, f64),
}

/// Paints the references for a capture whose grid origin is `origin` on `kind`'s surface.
fn sy_references(
    kind: SySurface,
    origin: (f64, f64),
    row: usize,
    fg: Rgb8,
    bg: Rgb8,
) -> SyReferences {
    let metrics = cell_metrics_for_smoke();
    let offset = match kind {
        SySurface::Recording => (origin.0 - origin.0.floor(), origin.1 - origin.1.floor()),
        SySurface::Image => (0.0, 0.0),
    };
    let width = (offset.0 + SY_LINE.len() as f64 * metrics.cell_w).ceil() as i32 + 2;
    let height = (offset.1 + (row + 1) as f64 * metrics.cell_h).ceil() as i32 + 2;
    let snap = sy_snapshot(row, fg, bg);
    let grid = |family: &str| {
        let mut font = metrics.font.clone();
        font.set_family(family);
        let painted = CellMetrics { font, ..metrics };
        sy_rasterize(kind, offset, width, height, |cr| {
            wolf_gnome::paint(cr, snap.active_grid(), &snap.highlights, &painted);
        })
    };
    let hex = sy_rasterize(kind, offset, width, height, |cr| {
        let set = |(r, g, b): Rgb8| {
            cr.set_source_rgb(
                f64::from(r) / 255.0,
                f64::from(g) / 255.0,
                f64::from(b) / 255.0,
            );
        };
        set(bg);
        cr.paint().expect("hex-box background");
        let layout = pangocairo::functions::create_layout(cr);
        let mut font = metrics.font.clone();
        font.set_family(SMOKE_FONT_FAMILY);
        layout.set_font_description(Some(&font));
        let attrs = pango::AttrList::new();
        attrs.insert(pango::AttrInt::new_fallback(false));
        layout.set_attributes(Some(&attrs));
        set(fg);
        for col in [SY_FOLDER_COL, SY_POWERLINE_COL, SY_REPO_COL] {
            layout.set_text(SY_LINE[col]);
            cr.move_to(
                col as f64 * metrics.cell_w,
                row as f64 * metrics.cell_h + metrics.baseline - metrics.ascent,
            );
            pangocairo::functions::show_layout(cr, &layout);
        }
    });
    SyReferences {
        configured: grid(SMOKE_FONT_FAMILY),
        symbols: grid(SYMBOLS_FAMILY),
        hex,
        origin: offset,
    }
}

/// One window of a capture: the reference it must match and the references it must not.
struct SyCase<'a> {
    col: usize,
    name: &'a str,
    own: &'a SyImage,
    others: Vec<(&'a str, &'a SyImage)>,
}

/// Checks one capture against its references, printing the table of distances first.
fn sy_assert_capture(
    smoke: &str,
    label: &str,
    png: &Path,
    kind: SySurface,
    origin: (f64, f64),
    row: usize,
) {
    assert_png_present(smoke, png);
    let capture = decode_capture(png, smoke, label);

    // The background from the second blank's centre; the foreground is the M cell's farthest pixel.
    let blank = sy_window(origin, SY_BG_COL, row, 1);
    let bg = capture.pixel((blank.x0 + blank.x1) / 2, (blank.y0 + blank.y1) / 2);
    let m_cell = sy_window(origin, SY_ASCII_COL, row, 1);
    let fg = (m_cell.y0..m_cell.y1)
        .flat_map(|y| (m_cell.x0..m_cell.x1).map(move |x| (x, y)))
        .map(|(x, y)| capture.pixel(x, y))
        .max_by_key(|&p| sy_channel_distance(p, bg))
        .expect("a non-empty M cell");
    assert!(
        sy_channel_distance(fg, bg) >= 64,
        "{smoke}: precondition: in the {label} capture the M cell's farthest pixel {fg:?} is \
         within 64 of the background {bg:?}, so the line is not on grid row {row} where the \
         derived geometry puts it, or nothing painted it. PNG kept at {}.",
        png.display()
    );

    let refs = sy_references(kind, origin, row, fg, bg);
    let mask_capture = |col: usize| SyMask::of(&capture, sy_window(origin, col, row, 2), fg, bg);
    let mask_ref = |image: &SyImage, col: usize| {
        SyMask::of(image, sy_window(refs.origin, col, row, 2), fg, bg)
    };

    let cases = [
        SyCase {
            col: SY_ASCII_COL,
            name: "M",
            own: &refs.configured,
            others: vec![("symbols direct", &refs.symbols)],
        },
        SyCase {
            col: SY_FOLDER_COL,
            name: "U+F07B",
            own: &refs.symbols,
            others: vec![("hex box", &refs.hex)],
        },
        SyCase {
            col: SY_POWERLINE_COL,
            name: "U+E0A0",
            own: &refs.symbols,
            others: vec![("hex box", &refs.hex)],
        },
        SyCase {
            col: SY_REPO_COL,
            name: "U+F401",
            own: &refs.symbols,
            others: vec![
                ("hex box", &refs.hex),
                ("DejaVu Sans Mono alone", &refs.configured),
            ],
        },
    ];

    let mut table = format!(
        "{smoke}: {label} capture, fg {fg:?} on bg {bg:?}, distances; `~` is the \
         one-pixel-tolerant ink distance, `^` the exact XOR:\n"
    );
    let mut failures = Vec::new();
    for SyCase {
        col,
        name,
        own,
        others,
    } in &cases
    {
        let got = mask_capture(*col);
        let own_mask = mask_ref(own, *col);
        let d_own = sy_distance(&got, &own_mask);
        table.push_str(&format!("{name:<7} capture~own {d_own:.3}"));
        assert!(
            own_mask.count() > 0,
            "{smoke}: premise: the {label} reference for {name} holds no ink, so nothing can match \
             it.\n{table}"
        );
        if *col == SY_POWERLINE_COL {
            let d_fc = sy_distance(&got, &mask_ref(&refs.configured, *col));
            table.push_str(&format!(
                "  capture~DejaVu Sans Mono alone {d_fc:.3} (printed only)"
            ));
        }
        for (other_name, other) in others {
            let other_mask = mask_ref(other, *col);
            let d_other = sy_xor(&got, &other_mask);
            let premise = sy_xor(&own_mask, &other_mask);
            table.push_str(&format!(
                "  capture^{other_name} {d_other:.3} (own^{other_name} {premise:.3})"
            ));
            assert!(
                premise >= SY_DIFFER_MIN,
                "{smoke}: premise: for {name} the {label} references are {premise:.3} apart \
                 against {other_name}, under {SY_DIFFER_MIN}, so the capture cannot be told to \
                 match one rather than the other.\n{table}"
            );
            if d_other < SY_DIFFER_MIN || d_other < SY_MIN_RATIO * d_own {
                failures.push(format!(
                    "{name} is {d_other:.3} from {other_name} and {d_own:.3} from its own \
                     reference: it must be at least {SY_DIFFER_MIN} from {other_name} and \
                     {SY_MIN_RATIO} times farther from it than from its own"
                ));
            }
        }
        table.push('\n');
        if d_own > SY_MATCH_MAX {
            failures.push(format!(
                "{name} is {d_own:.3} from its own reference, over {SY_MATCH_MAX}"
            ));
        }
    }
    eprintln!("{table}");
    assert!(
        failures.is_empty(),
        "{smoke}: in the {label} capture:\n{}\nAn icon matching the hex box means it painted as a \
         missing glyph; U+F401 matching DejaVu Sans Mono alone means it painted from fontconfig's \
         own choice (DejaVu Sans), so the family list does not name the symbols face; the M \
         matching symbols direct means the symbols face is ahead of the configured face and the M \
         fell to fontconfig; an icon far from every reference means the line is not where the \
         derived geometry puts it. PNG kept at {}.\n{table}",
        failures.join("\n"),
        png.display()
    );
}

/// **Nerd Font icons paint from the symbols face under cage, in an editor pane and in a terminal
/// pane.**
///
/// The checkout binary runs with the smoke config: DejaVu Sans Mono at `WOLF_FONT_PT` 28 and
/// `SMOKE_LINE_HEIGHT`, dark. The references are painted in this process with `wolf_gnome::paint`
/// on a snapshot holding the same line, in the colours read off the capture, with the description
/// naming one family: DejaVu Sans Mono alone, or the symbols face alone. The hex box is drawn with
/// Pango's fallback off. The editor references are rasterized through a recording surface
/// translated by the grid origin's fraction, as GTK draws a `DrawingArea`; the terminal's on an
/// image surface at the rounded origin, as `TerminalCanvas` paints its backing surface.
///
/// ## Assertions, per capture
///
/// | # | assertion | kind | falsifier |
/// | --- | --- | --- | --- |
/// | 1 | the scenario exits 0 | precondition | none |
/// | 2 | the grid origin is in the `PANE_ORIGIN_*_EXPECTED` band and the cell is 22.4765625 x 44 | precondition | none |
/// | 3 | Neovim reports text offset 0 and the line as typed | precondition | none |
/// | 4 | each pair of references is at least [`SY_DIFFER_MIN`] apart | premise | none |
/// | 5 | `M` matches DejaVu Sans Mono alone | discriminator | C6b: `family_list` returns `SYMBOLS_FAMILY.to_owned()` |
/// | 6 | each icon matches symbols direct and differs from the hex box | discriminator | C6a, for U+F401 |
/// | 7 | U+F401 differs from DejaVu Sans Mono alone | discriminator | C6a: `family_list` returns `configured.to_owned()` |
///
/// A match is [`sy_distance`] at most [`SY_MATCH_MAX`]; a difference is [`sy_xor`] at least
/// [`SY_DIFFER_MIN`] and at least [`SY_MIN_RATIO`] times the capture's [`sy_distance`] to the
/// matching reference. The one-pixel tolerance keeps a match robust to anti-aliasing, but it would
/// read a hex box drawn inside a solid glyph as nearly the same, so a difference is exact. U+E0A0's
/// distance to DejaVu Sans Mono alone is printed only.
#[test]
fn nerd_glyphs_paint_from_the_symbols_face_under_cage() {
    const SMOKE: &str = "nerd_glyphs_paint_from_the_symbols_face_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let shell = runtime_dir.join(SY_SHELL_NAME);
    std::fs::write(&shell, SY_SHELL)
        .unwrap_or_else(|e| panic!("{SMOKE}: failed to write {}: {e}", shell.display()));
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap_or_else(
            |e| {
                panic!(
                    "{SMOKE}: failed to make {} executable: {e}",
                    shell.display()
                )
            },
        );
    }

    // --- Assertion 2 (precondition): the geometry the windows derive from ------
    let cell = cell_metrics_for_smoke();
    assert_eq!(
        (cell.cell_w, cell.cell_h),
        (SY_CELL_W, SY_CELL_H),
        "{SMOKE}: cell_metrics_for_smoke gives a {} x {} cell, not {SY_CELL_W} x {SY_CELL_H}. The \
         family list reached the measurement, or the pinned face changed; every window below and \
         every pointer smoke's target would move with it.",
        cell.cell_w,
        cell.cell_h
    );
    let (origin_x, origin_y) = sy_grid_origin();
    for (axis, derived, measured, cell_size) in [
        ("x", origin_x, PANE_ORIGIN_X_EXPECTED, cell.cell_w),
        ("y", origin_y, PANE_ORIGIN_Y_EXPECTED, cell.cell_h),
    ] {
        assert!(
            (derived - measured).abs() <= PANE_ORIGIN_TOLERANCE_PX + cell_size,
            "{SMOKE}: the grid origin derived from the pane area, {axis} = {derived}, is outside \
             the band around PANE_ORIGIN_{}_EXPECTED = {measured} (tolerance \
             {PANE_ORIGIN_TOLERANCE_PX} plus one {cell_size} px cell). The pane chrome moved: \
             re-check OVERLAY_ORIGIN_*_EXPECTED and the statusbar before the windows.",
            axis.to_uppercase()
        );
    }

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);
    let readback = runtime_dir.join(SY_READBACK_NAME);
    let painted = runtime_dir.join(SY_PAINTED_NAME);
    let png_editor = runtime_dir.join(SY_PNG_EDITOR_NAME);
    let png_terminal = runtime_dir.join(SY_PNG_TERMINAL_NAME);
    for path in [&readback, &painted, &png_editor, &png_terminal] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(SY_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("SHELL", &shell)
        .env("SY_READBACK", &readback)
        .env("SY_PAINTED", &painted)
        .env("SY_PNG_EDITOR", &png_editor)
        .env("SY_PNG_TERMINAL", &png_terminal)
        .status()
        .expect("failed to spawn `cage` for the headless symbols scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage scenario exited unsuccessfully ({status}). Exit 11: the app died before \
         it was ready for input. Exit 12: the readback never appeared, so the typed `:call` lines \
         did not run. Exit 13: the shell never wrote the marker, so no session opened on \
         `:terminal` or SHELL {} never ran. Exit 40: the launch never became ready.",
        shell.display()
    );

    // --- Assertion 3 (precondition): the line is where the windows look -------
    let raw = read_readback(&readback, SMOKE);
    let lines: Vec<&str> = raw.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        ["0", SY_LINE_CODE_POINTS],
        "{SMOKE}: Neovim reports the window's text offset and buffer line 2's code points as \
         {lines:?}. A non-zero offset means a number or sign column shifts the line off the \
         derived columns; other code points mean the typed `setline` did not produce the fixture, \
         so the capture shows something else."
    );

    // --- Assertions 4 to 7, per capture --------------------------------------
    sy_assert_capture(
        SMOKE,
        "editor",
        &png_editor,
        SySurface::Recording,
        (origin_x, origin_y),
        SY_EDITOR_ROW,
    );
    sy_assert_capture(
        SMOKE,
        "terminal",
        &png_terminal,
        SySurface::Image,
        (origin_x.round(), origin_y.round()),
        SY_TERMINAL_ROW,
    );
}
