//! A lone preview pane wider than its capped content: the web view spans the pane below its
//! strip, the page centres its own capped content, the full-width toggle re-caps it live, and a
//! wheel over the side margin scrolls the page.
//!
//! Every observable is raw pixels of three captures, and the scroll the frozen window state
//! saved. Nothing is OCRed, so no fixture name can satisfy an assertion; the fixture is
//! registered in `fixture_guards.rs`'s `RELAUNCH_FIXTURE_NAMES` for the generic loop's sake.

use super::config::FULLWIDTH_SMOKE_FONT_PT;
use super::extras::{require, saved_preview_scroll, EXTRAS_HELPERS};
use super::pointer::SCROLL_AXIS_VALUE;
use super::preview::{colour_extent, preview_theme_pane_area, PREVIEW_DARK_BG};
use super::preview_render::CODEHL_PRE_BG;
use super::*;

use wolf_gnome::editor_inset::{column_cap, grid_rect, GridRect, EDITOR_PADDING_PX};

/// The Markdown source the smoke previews in place. Its name carries no asserted OCR term.
pub(crate) const PM_FILE: &str = "pmgsrc.md";

/// How many one-line paragraphs follow the code block: a page several viewports tall, so
/// [`PM_WHEEL_DETENTS`] scroll it well inside [`PM_SCROLL_RANGE`], as `extras.rs`'s S2 does
/// with as many.
const PM_PARAGRAPHS: usize = 80;

/// The fenced code block (no language) at the top of [`PM_FILE`]: four short lines, so its
/// `pre` box is `4 * 24 + 2 * 24` = 144 rows of `CODEHL_PRE_BG` whatever the width.
const PM_FENCE: &str = concat!(
    "```\n",
    "alpha bravo charlie\n",
    "delta echo foxtrot\n",
    "golf hotel india\n",
    "juliett kilo lima\n",
    "```",
);

/// The body of [`PM_FILE`]: [`PM_FENCE`], then [`PM_PARAGRAPHS`] paragraphs short enough to
/// stay one line capped, so the toggles do not change the page's height.
fn pm_body() -> String {
    let mut body = String::from(PM_FENCE);
    for n in 1..=PM_PARAGRAPHS {
        body.push_str(&format!(
            "\n\nParagraph {n:02} of the margins fixture, on one line."
        ));
    }
    body
}

/// How many wheel detents the probe over the side margin sends, as S2 sends over the page.
const PM_WHEEL_DETENTS: usize = 10;

/// The range the saved scroll must fall in: off both ends, so a page the wheel never reached,
/// which saves 0 or nothing, and one clamped to its end are told apart from one it scrolled.
const PM_SCROLL_RANGE: std::ops::RangeInclusive<f64> = 0.05..=0.95;

/// How many pixels of a colour a row or a column must carry for [`colour_extent`] to count it.
///
/// An acceptance criterion: a side margin below the strip is several hundred rows tall, and
/// the `pre` box is 144 rows and some 700 columns, each far above it, while a stray
/// coincidence of either colour would have to line up this many pixels in one line.
const PM_EXTENT_MIN_RUN_PX: usize = 48;

/// How far the dark extent's left edge may sit from the pane's: antialiasing at the edge.
const PM_LEFT_EDGE_TOLERANCE_PX: usize = 2;

/// How far the dark extent's right edge may sit from the pane's: a WebKit scrollbar, about
/// fifteen pixels, which an overlay scrollbar shown at the capture would put there.
const PM_RIGHT_EDGE_TOLERANCE_PX: usize = 16;

/// How far an edge of the `pre` extent may sit from the grid rect's.
///
/// `editor_inset::grid_rect` floors the centring offset and the page does not, which moves
/// the content right by under one pixel, and WebKit snaps the box's edges to whole pixels,
/// half a pixel either way at most.
const PM_PRE_EDGE_TOLERANCE_PX: f64 = 1.5;

/// The inclusive band of exit codes [`PM_BODY`] adds to [`POINTER_PREAMBLE`]'s 11-15 and 40.
const PM_EXITS: std::ops::RangeInclusive<i32> = 150..=153;

/// What each of [`PM_BODY`]'s exit codes means, for failure messages.
const PM_CODES: &str = "Exit 150: Neovim did not answer after the wheel, so keys went to the \
     web view. 151: the app outlived `:WolfCloseWindow`. 152: the app exited non-zero. 153: no \
     wolf.sqlite after the close.";

/// Body of [`preview_margins_frame_and_scroll_the_page_under_cage`], appended to
/// [`POINTER_PREAMBLE`] and [`EXTRAS_HELPERS`].
///
/// Preview [`PM_FILE`] in place, park the pointer and capture A; `:WolfFullWidth`, capture C;
/// `:WolfFullWidth` again, capture D. Then move to the side-margin probe and wheel
/// [`PM_WHEEL_DETENTS`] detents, read Neovim's buffer name back, close the window and freeze
/// the state, as `extras.rs`'s S2 does.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// | code | meaning |
/// | --- | --- |
/// | `150` | Neovim did not answer after the wheel |
/// | `151` | the app was still alive after `:WolfCloseWindow` |
/// | `152` | the app exited non-zero |
/// | `153` | no `wolf.sqlite` to freeze after the close |
const PM_BODY: &str = r#"
# A cold preview open, and the page's own layout; see PREVIEW_SETTLE in preview.rs.
PREVIEW_SETTLE=3.0
# The toggle's wake, the root style's script and the page's relayout.
TOGGLE_SETTLE=2.0
# The wheel's scroll, smooth or not, and the page's report of it; see PS_BODY.
SCROLL_SETTLE=2.0

# --- A: the capped preview, in place -----------------------------------------
nvim_ex "e $PM_SOURCE"
nvim_ex "WolfPreview"
sleep "$PREVIEW_SETTLE"
hsend motion 1 1
sleep "$SETTLE"
grim "$OUT_PNG"

# --- C: full width ------------------------------------------------------------
nvim_ex "WolfFullWidth"
sleep "$TOGGLE_SETTLE"
grim "$OUT_PNG_C"

# --- D: capped again ----------------------------------------------------------
nvim_ex "WolfFullWidth"
sleep "$TOGGLE_SETTLE"
grim "$OUT_PNG_D"

# --- The wheel over the side margin, then the close ---------------------------
hsend motion "$PM_PROBE_X" "$PM_PROBE_Y"
sleep "$SETTLE"
detents=0
while [ "$detents" -lt "$PM_DETENTS" ]; do
    hstep axis "$PM_AXIS_VALUE" 1
    detents=$((detents + 1))
done
sleep "$SCROLL_SETTLE"
wtype -- ":call writefile([bufname('%')], '$PM_READ')"
wtype -k Return
wait_file "$PM_READ" || exit 150
wtype -- ":WolfCloseWindow"
wtype -k Return
reap_app 151 152
freeze_or_exit "$PM_STATE" 153

hsend quit
exit 0
"#;

/// The cell [`FULLWIDTH_SMOKE_FONT_PT`] gives, measured through the app's own
/// `CellMetrics::for_font` call, as [`cell_metrics_for_smoke`] measures [`SMOKE_FONT_PT`]'s.
fn pm_cell() -> CellMetrics {
    let points = FULLWIDTH_SMOKE_FONT_PT
        .parse::<f64>()
        .unwrap_or_else(|e| panic!("FULLWIDTH_SMOKE_FONT_PT does not parse as an f64 ({e})"));
    let probe = ImageSurface::create(Format::ARgb32, 1, 1)
        .expect("failed to create the 1x1 probe surface for cell measurement");
    let cr = Context::new(&probe).expect("failed to create the probe Cairo context");
    let metrics = CellMetrics::for_font(&cr, SMOKE_FONT_FAMILY, points, smoke_line_height());
    drop(cr);
    assert!(
        metrics.cell_w > 0.0 && metrics.size_px > 0.0,
        "CellMetrics::for_font({SMOKE_FONT_FAMILY:?}, {points}) measured a cell {} px wide at \
         {} px: the pinned FONTCONFIG_FILE is missing, so no expected rect below means anything",
        metrics.cell_w,
        metrics.size_px
    );
    metrics
}

/// The lone pane's geometry at [`FULLWIDTH_SMOKE_FONT_PT`], in absolute output pixels.
struct PmGeometry {
    /// The pane area, as [`preview_theme_pane_area`] gives it: `(x, y, w, h)`.
    area: (f64, f64, f64, f64),
    /// The statusbar strip's height, as `strip_bands` in `preview.rs` derives it.
    strip_h: f64,
    /// The capped grid rect, pane-local.
    capped: GridRect,
    /// The uncapped grid rect, pane-local.
    full: GridRect,
}

impl PmGeometry {
    fn derive() -> Self {
        let cell = pm_cell();
        let layout = wolf_gnome::statusbar_layout::statusbar_layout(
            cell.size_px,
            wolf_gnome::statusbar_layout::secondary_font_size(cell.size_px),
            wolf_gnome::statusbar_layout::STATUSBAR_VERTICAL_PADDING,
            wolf_gnome::statusbar_layout::STATUSBAR_LINE_GAP,
        );
        let area = preview_theme_pane_area();
        let (_, _, w, h) = area;
        let rect = |full_width| {
            grid_rect(
                w,
                h,
                layout.height,
                EDITOR_PADDING_PX,
                column_cap(full_width, cell.cell_w),
            )
        };
        Self {
            area,
            strip_h: layout.height,
            capped: rect(false),
            full: rect(true),
        }
    }

    /// The pane below its strip, down to the rows the theme smoke's bands stop at, in absolute
    /// output pixels: where each extent is searched.
    fn below_strip(&self) -> SampleRect {
        let (x, y, w, _) = self.area;
        let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
        SampleRect {
            x0: pixel_index(x, "the pane's first column"),
            x1: pixel_index(x + w, "the pane's last column"),
            y0: pixel_index(
                (y + self.strip_h).ceil() + 1.0,
                "the first row below the strip",
            ),
            y1: pixel_index(
                output_h - BAND_BOTTOM_CLEARANCE_PX,
                "the last row above the cmdline strip",
            ),
        }
    }

    /// The side-margin probe: mid-way between the left padding gutter's inner edge and the
    /// capped content's left edge, at the pane area's mid-height.
    fn margin_probe(&self) -> (i64, i64) {
        let (x, y, _, h) = self.area;
        (
            pixel_target(
                x + (EDITOR_PADDING_PX + self.capped.x) / 2.0,
                "the probe's x",
            ),
            pixel_target(y + h / 2.0, "the probe's y"),
        )
    }
}

/// What one extent assertion found, for the collected failures.
fn extent_text(extent: Option<SampleRect>) -> String {
    extent.map_or_else(|| "no extent at all".to_owned(), |e| format!("{e:?}"))
}

/// Records `failure` under `id` unless `holds`.
fn check(failures: &mut Vec<String>, id: &str, holds: bool, failure: impl FnOnce() -> String) {
    if !holds {
        failures.push(format!("{id}: {}", failure()));
    }
}

/// Whether `extent` spans the pane area `area` from edge to edge, within the edge tolerances.
fn spans_pane(extent: Option<SampleRect>, below: SampleRect) -> bool {
    extent.is_some_and(|e| {
        e.x0 <= below.x0 + PM_LEFT_EDGE_TOLERANCE_PX
            && e.x1 + PM_RIGHT_EDGE_TOLERANCE_PX >= below.x1
    })
}

/// A pixel column as an `f64`, exactly: a capture is far narrower than `u32::MAX`.
fn column(x: usize) -> f64 {
    f64::from(u32::try_from(x).expect("a capture column fits in a u32"))
}

/// Whether `extent`'s columns are `grid`'s, offset by the pane's left edge `area_x`, within
/// [`PM_PRE_EDGE_TOLERANCE_PX`].
fn on_grid(extent: Option<SampleRect>, grid: GridRect, area_x: f64) -> bool {
    extent.is_some_and(|e| {
        (column(e.x0) - (area_x + grid.x)).abs() <= PM_PRE_EDGE_TOLERANCE_PX
            && (column(e.x1) - (area_x + grid.x + grid.w)).abs() <= PM_PRE_EDGE_TOLERANCE_PX
    })
}

/// **A lone preview's web view spans its pane below the strip, its page carries the column
/// cap, the full-width toggle re-caps it live, and the wheel scrolls it from the side margin.**
///
/// ## The scenario
///
/// One launch at [`FULLWIDTH_SMOKE_FONT_PT`], set after [`pointer_command`] as `config.rs`'s
/// column-cap smoke sets it, so the cap binds in the lone pane's 989-px grid with a side
/// margin of over a hundred pixels. [`PM_BODY`] previews [`PM_FILE`] in place and takes
/// capture A, toggles full width for capture C and back for capture D, then wheels
/// [`PM_WHEEL_DETENTS`] detents of [`SCROLL_AXIS_VALUE`] at [`PmGeometry::margin_probe`],
/// mid-way across the left side margin and outside the capped content. It closes the window
/// with `:WolfCloseWindow` and freezes the state.
///
/// Expected rects come from `editor_inset::grid_rect` at [`pm_cell`]'s cell, with the strip
/// height derived as `strip_bands` in `preview.rs` derives it and the pane at
/// `OVERLAY_ORIGIN_*`. The preview is located by [`colour_extent`] of [`PREVIEW_DARK_BG`] and
/// the `pre` box by that of [`CODEHL_PRE_BG`], each over [`PmGeometry::below_strip`].
///
/// ## Assertions
///
/// Preconditions panic first. The six assertions are each evaluated and collected, and the
/// test fails at the end listing every failed one by its ID.
///
/// | ID | assertion | kind |
/// | --- | --- | --- |
/// | — | the scenario ran, every capture and the frozen state exist, the probe is on screen | precondition |
/// | — | the cap binds: the capped grid is narrower than the pane less its padding | precondition |
/// | — | the frozen state holds a lone preview pane of [`PM_FILE`] | precondition |
/// | G4a | in A, the dark extent spans the pane: its left edge within [`PM_LEFT_EDGE_TOLERANCE_PX`] of the pane's, its right within [`PM_RIGHT_EDGE_TOLERANCE_PX`] | discriminator |
/// | G5a | in A, the `pre` extent's columns are the capped grid rect's | guard |
/// | G6a | in C, the dark extent spans the pane | discriminator |
/// | G7a | in C, the `pre` extent's columns are the uncapped grid rect's, the pane less its padding | guard |
/// | G8a | in D, the `pre` extent's columns are back on the capped grid rect | guard |
/// | G9a | the frozen state's preview scroll lies in [`PM_SCROLL_RANGE`] | discriminator |
///
/// ## Premises, measured by this smoke and by nothing else
///
/// - The page's root has no background of its own, so CSS paints the body's
///   [`PREVIEW_DARK_BG`] over the whole canvas, root padding and side margins included. A red
///   G4a with a green G5a, the content capped and the margins some other colour, is this
///   premise first.
/// - WebKitGTK's scrollbars overlay the page and take no width from its layout viewport. A
///   classic scrollbar would centre the capped content some seven pixels left and end the
///   full-width content that far short of the right padding, reddening G5a, G7a and G8a
///   together.
/// - A wheel event over the web view scrolls its page wherever the pointer is on it, and
///   [`PM_WHEEL_DETENTS`] detents leave the page inside [`PM_SCROLL_RANGE`], as
///   `preview_scroll_survives_a_close_and_relaunch_under_cage` measures over the content.
#[test]
fn preview_margins_frame_and_scroll_the_page_under_cage() {
    const SMOKE: &str = "preview_margins_frame_and_scroll_the_page_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let body = pm_body();
    write_relaunch_fixtures(SMOKE, &cwd, &[(PM_FILE, body.as_str())]);

    let png_a = runtime_dir.join("wolf_gnome_pm_a.png");
    let png_c = runtime_dir.join("wolf_gnome_pm_c.png");
    let png_d = runtime_dir.join("wolf_gnome_pm_d.png");
    let read = runtime_dir.join("wolf_gnome_pm_read.txt");
    for path in [&png_a, &png_c, &png_d, &read] {
        clear_evidence(path, SMOKE);
    }
    let state = frozen_state_dir(&runtime_dir, "wolf_gnome_pm_state", SMOKE);

    let geometry = PmGeometry::derive();
    let (probe_x, probe_y) = geometry.margin_probe();
    let script = format!("{EXTRAS_HELPERS}\n{PM_BODY}");
    let mut cmd = pointer_command(&script, &cwd, &runtime_dir, &png_a);
    cmd.env("PM_SOURCE", PM_FILE)
        .env("PM_DETENTS", PM_WHEEL_DETENTS.to_string())
        .env("PM_AXIS_VALUE", SCROLL_AXIS_VALUE)
        .env("PM_PROBE_X", probe_x.to_string())
        .env("PM_PROBE_Y", probe_y.to_string())
        .env("PM_READ", &read)
        .env("PM_STATE", &state)
        .env("STATE_SRC", smoke_state_dir(&cwd))
        .env("OUT_PNG_C", &png_c)
        .env("OUT_PNG_D", &png_d);
    // Chained after `pointer_command` so it wins, as config.rs's column-cap smoke does.
    cmd.env("WOLF_FONT_PT", FULLWIDTH_SMOKE_FONT_PT);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless preview margins scenario");
    if !status.code().is_some_and(|code| PM_EXITS.contains(&code)) {
        assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &png_a);
    }

    // --- Preconditions ----------------------------------------------------------------
    require(
        SMOKE,
        status,
        &[&png_a, &png_c, &png_d, &read, &state],
        "A to the close",
        PM_CODES,
    );
    let extents = read_output_extents(&cwd, SMOKE);
    assert_inside_extents(SMOKE, "the side-margin probe", (probe_x, probe_y), extents);
    let (area_x, _, area_w, _) = geometry.area;
    let inset_w = area_w - 2.0 * EDITOR_PADDING_PX;
    assert!(
        geometry.capped.w < inset_w && geometry.capped.x > EDITOR_PADDING_PX + 1.0,
        "{SMOKE}: precondition: at {FULLWIDTH_SMOKE_FONT_PT} pt the capped grid {:?} must be \
         narrower than the pane's {inset_w}-px inset width, leaving a side margin, or the smoke \
         measures nothing. The font size or the output size has moved.",
        geometry.capped
    );
    assert!(
        exact_f64(probe_x, "the probe's x") < area_x + geometry.capped.x - 1.0,
        "{SMOKE}: precondition: the probe x {probe_x} must lie left of the capped content at {}",
        area_x + geometry.capped.x
    );
    let scroll = saved_preview_scroll(SMOKE, &state, "the state frozen after the close", PM_FILE);

    let below = geometry.below_strip();
    let capture_a = decode_capture(&png_a, SMOKE, "capture A");
    let capture_c = decode_capture(&png_c, SMOKE, "capture C");
    let capture_d = decode_capture(&png_d, SMOKE, "capture D");
    let dark =
        |capture: &Capture| colour_extent(capture, below, PREVIEW_DARK_BG, PM_EXTENT_MIN_RUN_PX);
    let pre =
        |capture: &Capture| colour_extent(capture, below, CODEHL_PRE_BG, PM_EXTENT_MIN_RUN_PX);
    let capped_cols = (
        area_x + geometry.capped.x,
        area_x + geometry.capped.x + geometry.capped.w,
    );
    let full_cols = (
        area_x + geometry.full.x,
        area_x + geometry.full.x + geometry.full.w,
    );

    // --- The six assertions, collected -------------------------------------------------
    let mut failures = Vec::new();
    let dark_a = dark(&capture_a);
    check(&mut failures, "G4a", spans_pane(dark_a, below), || {
        format!(
            "in A the PREVIEW_DARK_BG extent is {}, not the pane {}..{}. A left edge at the \
             capped content, about {:.0}, means the web view is framed to the capped grid rect \
             again; one at the padding, {:.0}, means it is framed to the padded pane. Either way \
             the margins are not the page's, and a wheel there misses it.",
            extent_text(dark_a),
            below.x0,
            below.x1,
            capped_cols.0,
            area_x + EDITOR_PADDING_PX
        )
    });
    let pre_a = pre(&capture_a);
    check(
        &mut failures,
        "G5a",
        on_grid(pre_a, geometry.capped, area_x),
        || {
            format!(
                "in A the `pre` extent is {}, not on the capped grid's columns \
                 {:.1}..{:.1}. Columns {:.1}..{:.1}, the pane less its padding, mean the page \
                 carries no cap; a shift of some seven pixels means a classic scrollbar narrows \
                 its viewport.",
                extent_text(pre_a),
                capped_cols.0,
                capped_cols.1,
                full_cols.0,
                full_cols.1
            )
        },
    );
    let dark_c = dark(&capture_c);
    check(&mut failures, "G6a", spans_pane(dark_c, below), || {
        format!(
            "in C, full width, the PREVIEW_DARK_BG extent is {}, not the pane {}..{}: a \
             full-width preview's web view is framed short of its pane.",
            extent_text(dark_c),
            below.x0,
            below.x1
        )
    });
    let pre_c = pre(&capture_c);
    check(
        &mut failures,
        "G7a",
        on_grid(pre_c, geometry.full, area_x),
        || {
            format!(
                "in C, full width, the `pre` extent is {}, not the pane less its padding, \
                 {:.1}..{:.1}. The capped columns {:.1}..{:.1} mean the toggle never reached \
                 the page: the root style was not sent live.",
                extent_text(pre_c),
                full_cols.0,
                full_cols.1,
                capped_cols.0,
                capped_cols.1
            )
        },
    );
    let pre_d = pre(&capture_d);
    check(
        &mut failures,
        "G8a",
        on_grid(pre_d, geometry.capped, area_x),
        || {
            format!(
                "in D, toggled back, the `pre` extent is {}, not the capped columns \
                 {:.1}..{:.1}. Full-width columns mean the second toggle never reached the \
                 page, as when the style sent is not recorded and the comparison skips it.",
                extent_text(pre_d),
                capped_cols.0,
                capped_cols.1
            )
        },
    );
    check(
        &mut failures,
        "G9a",
        scroll.is_some_and(|s| PM_SCROLL_RANGE.contains(&s)),
        || {
            format!(
                "the close saved the preview's scroll as {scroll:?}, not a fraction in \
                 {PM_SCROLL_RANGE:?}, after {PM_WHEEL_DETENTS} detents at ({probe_x}, \
                 {probe_y}) in the left side margin. 0 or None means the wheel there never \
                 reached the page: the margin is the pane's, not the web view's. A value near 1 \
                 means it scrolled to the end, which is this smoke's premise first."
            )
        },
    );

    assert!(
        failures.is_empty(),
        "{SMOKE}: {} of the six assertions failed. Captures kept at {}, {} and {}; state at {}.\n{}",
        failures.len(),
        png_a.display(),
        png_c.display(),
        png_d.display(),
        state.display(),
        failures.join("\n")
    );
}
