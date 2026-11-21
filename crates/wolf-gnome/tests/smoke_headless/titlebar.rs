//! The titlebar smokes: under the vendored Lighthouse, the header bar and the tab bar take the
//! editor's `Normal` background, focused and in backdrop, follow a live `:set background`, and
//! take `WolfTitlebar`'s background once it is set. The title and the tabs' text keep
//! libadwaita's foreground for the configured appearance, while the desktop reports the other.

use super::editor::SAVE_AS_GDK_DEBUG;
use super::lighthouse::{rgb8, stage_init_lua, Background, Lighthouse};
use super::*;

/// The staged `init.lua`, the readback and the captures, all under `XDG_RUNTIME_DIR` so no
/// sidebar row draws them. Each smoke has its own runtime directory.
const TB_STAGE_NAME: &str = "wolf_gnome_tb_runtime";
pub(crate) const TB_READ_NAME: &str = "wolf_gnome_tb_read.txt";
const TB_PNG_FOCUSED_NAME: &str = "wolf_gnome_tb_focused.png";
const TB_PNG_LIVE_NAME: &str = "wolf_gnome_tb_live.png";
const TB_PNG_WOLF_NAME: &str = "wolf_gnome_tb_wolf.png";
const TB_PNG_BACKDROP_NAME: &str = "wolf_gnome_tb_backdrop.png";

/// The config directory suffix, whose `appearance` is the smoke's background.
const TB_CONFIG_SUFFIX: &str = "-tb";

/// The `WolfTitlebar` background the last capture sets. A precondition checks that it is
/// neither Lighthouse background, so the bars reaching it cannot be `Normal`'s by accident.
const TB_WOLF_TITLEBAR: u32 = 0x7a_2b_5c;

/// libadwaita's debug override of the desktop's colour scheme, `default`, `prefer-dark` or
/// `prefer-light`. libadwaita 1.9.3's `adw-settings.c` reads it in `init_debug`, ahead of the
/// portal and GSettings, so the cage's desktop reports what the smoke sets here.
const ADW_COLOR_SCHEME_ENV: &str = "ADW_DEBUG_COLOR_SCHEME";

/// How far inside the header bar's top edge, and above its bottom shade, the header rows start
/// and stop, so the rows are clear of the bar's edges.
pub(crate) const TB_HEADER_ROW_INSET_PX: f64 = 3.0;

/// The header bar's bottom row, its shade rather than its background: libadwaita 1.9.3's
/// `headerbar { box-shadow: inset 0 -1px var(--headerbar-shade-color) }`, kept on purpose.
pub(crate) const TB_HEADER_SHADE_PX: f64 = 1.0;

/// How far in from each side of the output the two header spans start, past the start buttons
/// (the Back/Forward pair and the sidebar's toggle group) and the end ones (the primary menu
/// and any window controls).
///
/// libadwaita 1.9.3 pads the bar's box by 7 px and spaces each group by 6 px, an image button
/// is about 34 px wide and the toggle group about 75 px, so the start group ends near 156 px:
/// 7 + 68 + 6 + 75. The rest is room for a window control more at the end.
pub(crate) const TB_HEADER_BUTTON_CLEARANCE_PX: f64 = 240.0;

/// Half the width of the title band, centred on the output. It holds `~/project` in
/// libadwaita's bold title with its 12 px side padding, about 100 px in all, and stops short of
/// [`TB_HEADER_BUTTON_CLEARANCE_PX`], so the band holds the title and no icon.
pub(crate) const TB_TITLE_HALF_WIDTH_PX: f64 = 120.0;

/// libadwaita 1.9.3's `tabbar tabbox { padding-top: 6px; padding-bottom: 6px }`. The bar's
/// `.box` has `padding-top: 0`, so the bar's first 6 rows are its background with no tab in
/// them.
const TB_TABBOX_PADDING_PX: f64 = 6.0;

/// How far inside the tab box's top padding the tab-bar rows start and stop.
const TB_TAB_ROW_INSET_PX: f64 = 1.0;

/// How far in from each side of the output the tab-bar columns start, clear of the bar's 1 px
/// side padding.
///
/// The columns start left of [`OVERLAY_ORIGIN_X_EXPECTED`], over the sidebar. At one page the
/// bar has no height, so these rows then show the sidebar's colour there and fail, which keeps
/// the band from passing on the pane area's background alone.
const TB_TAB_X_INSET_PX: f64 = 8.0;

/// The share of a band that must be exactly the expected colour.
///
/// Every band is clear of glyphs, icons and edges, so a correct fill gives 1.0, and the rest
/// allows a stray antialiased pixel. A bar at another colour gives about 0.
pub(crate) const TB_MIN_EXACT_SHARE: f64 = 0.98;

/// The luma difference from the expected background, on the configured scheme's side, at which
/// a pixel counts as ink: lighter for `dark`, darker for `light`.
///
/// libadwaita's dark-scheme ink is white and its light-scheme ink `rgb(0 0 6 / 80%)`, so the
/// right scheme's full ink is about 228 lighter than Lighthouse's dark background and about 196
/// darker than its light one, and its backdrop ink at half opacity about half that. The wrong
/// scheme's ink lies on the other side, and the selected tab's 10 % `currentColor` wash is
/// about 23 away, so neither can reach this.
const TB_INK_MIN_CONTRAST: f64 = 64.0;

/// How many ink pixels the title band and the tab-ink band must each hold. A bold `~/project`
/// and two tab titles each give hundreds.
const TB_INK_MIN_PIXELS: usize = 40;

/// The most BACKDROP's largest title contrast may be, as a share of WOLF's.
///
/// WOLF is the reference because it has BACKDROP's bar colour, scheme, title and band, and
/// differs only in being focused. libadwaita 1.9.3's
/// `headerbar:backdrop > windowhandle { filter: opacity(0.5) }` halves the contrast, and an
/// active window gives 1.0.
const TB_BACKDROP_MAX_INK_RATIO: f64 = 0.75;

/// The least WOLF's largest title contrast must be, so that [`TB_BACKDROP_MAX_INK_RATIO`]
/// measures a dimmed title rather than a missing one, which would pass as 0 ≤ 0.
///
/// Over [`TB_WOLF_TITLEBAR`], libadwaita's light-scheme ink is about 57 luma darker focused
/// and about 29 in backdrop, and the dark scheme's about 183 and 92 lighter. The floor sits
/// well under the light scheme's focused 57, the smaller of the two references.
const TB_BACKDROP_MIN_REFERENCE_INK: f64 = 30.0;

/// The TITLEBAR scenario: one launch, four captures, the last with GTK's in-process save
/// dialog up.
///
/// The app opens a second tab with Ctrl+Shift+T, reads back `g:colors_name`, `&background`
/// and the tabpage count, and captures FOCUSED. It sets the other background and captures
/// LIVE, then sets `WolfTitlebar` and captures WOLF. `:doautocmd ColorScheme` is what makes
/// the theme tracker re-read the group, since a `:hi` alone fires no autocommand.
///
/// BACKDROP comes last, from a transient dialog. `cage` activates every toplevel it maps and
/// deactivates one only when another takes focus, so the main window is active from its
/// first frame and stays so. A toplevel with a parent is not primary to `cage`: it is centred
/// at its own size rather than maximized, and mapping it moves focus to it, which leaves the
/// main window inactive and visible around it. A typed `:enew` leaves a nameless buffer, on
/// which Ctrl+Shift+S opens the save dialog (see the save-as smoke), measured at 881x326. On
/// the 1280x720 output its top edge sits near y = 197, far below both bars. The Rust side
/// proves the dimming from the title's ink rather than trusting it. The dialog is never
/// closed: the `cleanup` trap kills the app with it still up.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | the app died before it was ready for input |
/// | `12` | the readback never appeared |
/// | `40` | the launch never became ready (the shared convention) |
const TB_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
FILE_TRIES=100

# The adw::TabBar animates its height in over about a second; the native-tabs smoke's value.
TABS_BAR_SETTLE=2.0

# The save dialog maps, realizes and takes keyboard focus; the save-as smoke's DIALOG_SETTLE.
DIALOG_SETTLE=2.0

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
    echo "wolf-gnome titlebar smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome titlebar smoke"

# FOCUSED -- two tabs, the configured background.
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
wtype -- ":call writefile([get(g:, 'colors_name', ''), &background, printf('%d', len(gettabinfo()))], '$READ')"
wtype -k Return
wait_file "$READ" || exit 12
sleep "$SETTLE"
grim "$PNG_FOCUSED"

# LIVE -- the other background.
wtype -- ":set background=$OTHER_BACKGROUND"
wtype -k Return
sleep "$SETTLE"
grim "$PNG_LIVE"

# WOLF -- WolfTitlebar set.
wtype -- ":hi WolfTitlebar guibg=$WOLF_TITLEBAR"
wtype -k Return
wtype -- ":doautocmd ColorScheme"
wtype -k Return
sleep "$SETTLE"
grim "$PNG_WOLF"

# BACKDROP -- a typed :enew leaves a nameless buffer, so Ctrl+Shift+S opens the save dialog.
wtype -- ":enew"
wtype -k Return
sleep "$SETTLE"
wtype -M ctrl -M shift -k s -m shift -m ctrl
sleep "$DIALOG_SETTLE"
grim "$PNG_BACKDROP"

exit 0
"#;

/// The rectangles each capture is read in, all derived from named constants.
struct TbBands {
    /// The header bar's background: two spans, one each side of the title.
    header: [SampleRect; 2],
    /// The tab bar's background: its tab box's top padding, across the output.
    tab_bar: SampleRect,
    /// The grid's background, inside the selected tab's pane.
    grid: SampleRect,
    /// The title, centred over the header rows.
    title: SampleRect,
    /// The tabs, between the tab box's paddings, across the output.
    tab_ink: SampleRect,
}

/// A rectangle from four derived edges, asserting it is not empty.
fn tb_rect(x0: f64, x1: f64, y0: f64, y1: f64, what: &str) -> SampleRect {
    let rect = SampleRect {
        x0: pixel_index(x0, what),
        x1: pixel_index(x1, what),
        y0: pixel_index(y0, what),
        y1: pixel_index(y1, what),
    };
    assert!(
        rect.x1 > rect.x0 && rect.y1 > rect.y0,
        "{what}: the derived band {rect:?} is empty, so every share read from it would be \
         vacuous. A constant it is derived from has moved: re-check CAGE_OUTPUT_*_EXPECTED, \
         OVERLAY_ORIGIN_*_EXPECTED, TAB_BAR_HEIGHT_EXPECTED and this module's TB_* constants."
    );
    rect
}

/// The bands, with no pixel literal.
///
/// The header bar is the rows above [`OVERLAY_ORIGIN_Y_EXPECTED`], the one-tab pane area's top.
/// With two tabs the tab bar sits below it and is [`TAB_BAR_HEIGHT_EXPECTED`] tall, and the
/// pane area starts below the bar. The grid's rows lie below the pane's statusbar strip, its
/// top padding and two cell rows, as `editor_inset::grid_rect` places them, so the fixture's
/// line and the cursor are above the band; its columns are [`BAND_X_INSET_PX`] inside the pane
/// area and its rows stop [`BAND_BOTTOM_CLEARANCE_PX`] above the output's bottom, clear of the
/// `~` column, the thumb, the command-line strip and the message toast.
fn tb_bands() -> TbBands {
    let width = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let height = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    let centre = width / 2.0;

    let header_top = TB_HEADER_ROW_INSET_PX;
    let header_bottom = OVERLAY_ORIGIN_Y_EXPECTED - TB_HEADER_SHADE_PX - TB_HEADER_ROW_INSET_PX;
    let header = [
        tb_rect(
            TB_HEADER_BUTTON_CLEARANCE_PX,
            centre - TB_TITLE_HALF_WIDTH_PX,
            header_top,
            header_bottom,
            "the header bar's span before the title",
        ),
        tb_rect(
            centre + TB_TITLE_HALF_WIDTH_PX,
            width - TB_HEADER_BUTTON_CLEARANCE_PX,
            header_top,
            header_bottom,
            "the header bar's span after the title",
        ),
    ];
    let title = tb_rect(
        centre - TB_TITLE_HALF_WIDTH_PX,
        centre + TB_TITLE_HALF_WIDTH_PX,
        header_top,
        header_bottom,
        "the title band",
    );

    let bar_top = OVERLAY_ORIGIN_Y_EXPECTED;
    let tab_bar = tb_rect(
        TB_TAB_X_INSET_PX,
        width - TB_TAB_X_INSET_PX,
        bar_top + TB_TAB_ROW_INSET_PX,
        bar_top + TB_TABBOX_PADDING_PX - TB_TAB_ROW_INSET_PX,
        "the tab bar's band",
    );
    let tab_ink = tb_rect(
        TB_TAB_X_INSET_PX,
        width - TB_TAB_X_INSET_PX,
        bar_top + TB_TABBOX_PADDING_PX,
        bar_top + TAB_BAR_HEIGHT_EXPECTED - TB_TABBOX_PADDING_PX,
        "the tab-ink band",
    );

    let cell = cell_metrics_for_smoke();
    let statusbar = wolf_gnome::statusbar_layout::statusbar_layout(
        cell.size_px,
        wolf_gnome::statusbar_layout::secondary_font_size(cell.size_px),
        wolf_gnome::statusbar_layout::STATUSBAR_VERTICAL_PADDING,
        wolf_gnome::statusbar_layout::STATUSBAR_LINE_GAP,
    );
    let pane_top = bar_top + TAB_BAR_HEIGHT_EXPECTED;
    let rect = wolf_gnome::editor_inset::grid_rect(
        width - OVERLAY_ORIGIN_X_EXPECTED,
        height - pane_top,
        statusbar.height,
        wolf_gnome::editor_inset::EDITOR_PADDING_PX,
        wolf_gnome::editor_inset::column_cap(true, cell.cell_w),
    );
    let grid = tb_rect(
        OVERLAY_ORIGIN_X_EXPECTED + BAND_X_INSET_PX,
        width - BAND_X_INSET_PX,
        pane_top + rect.y.round() + 2.0 * cell.cell_h,
        height - BAND_BOTTOM_CLEARANCE_PX,
        "the grid band",
    );

    TbBands {
        header,
        tab_bar,
        grid,
        title,
        tab_ink,
    }
}

/// What one background band reads: the share exactly `expected`, and the most common colour
/// with its share, for the table and the failure messages.
struct TbReading {
    expected: Rgb8,
    exact: f64,
    dominant: Rgb8,
    dominant_share: f64,
}

fn tb_read(capture: &Capture, rects: &[SampleRect], expected: Rgb8, what: &str) -> TbReading {
    let mut counts: std::collections::BTreeMap<Rgb8, usize> = std::collections::BTreeMap::new();
    for rect in rects {
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                *counts.entry(capture.pixel(x, y)).or_default() += 1;
            }
        }
    }
    let total = rects.iter().map(|rect| rect.pixels()).sum();
    let exact = counts.get(&expected).copied().unwrap_or(0);
    let (dominant, count) = counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .unwrap_or(((0, 0, 0), 0));
    TbReading {
        expected,
        exact: band_fraction(exact, total, what),
        dominant,
        dominant_share: band_fraction(count, total, what),
    }
}

/// The ink in a band: how many pixels are at least [`TB_INK_MIN_CONTRAST`] from the expected
/// background on the configured scheme's side, and the largest difference on each side.
struct TbInk {
    pixels: usize,
    max: f64,
    other_max: f64,
}

/// Rec. 601 luma, which orders libadwaita's foregrounds and Lighthouse's backgrounds the way
/// they look.
fn luma((r, g, b): Rgb8) -> f64 {
    0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b)
}

fn tb_ink(capture: &Capture, rect: SampleRect, bg: Rgb8, scheme: Background) -> TbInk {
    let base = luma(bg);
    let mut ink = TbInk {
        pixels: 0,
        max: 0.0,
        other_max: 0.0,
    };
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let lum = luma(capture.pixel(x, y));
            let side = match scheme {
                Background::Dark => lum - base,
                Background::Light => base - lum,
            };
            if side >= TB_INK_MIN_CONTRAST {
                ink.pixels += 1;
            }
            ink.max = ink.max.max(side);
            ink.other_max = ink.other_max.max(-side);
        }
    }
    ink
}

fn hex((r, g, b): Rgb8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Checks the readback: `lighthouse`, `background`'s name and two tabpages.
fn tb_assert_readback(smoke: &str, path: &Path, background: Background) {
    let raw = read_readback(path, smoke);
    let lines: Vec<&str> = raw.lines().map(str::trim).collect();
    let name = background.name();
    assert_eq!(
        lines,
        ["lighthouse", name, "2"],
        "{smoke}: Neovim reports `g:colors_name`, `&background` and the tabpage count as \
         {lines:?}, not [\"lighthouse\", {name:?}, \"2\"]. An empty name means `init.lua` did not \
         find the staged `lighthouse/` beside it (WOLF_INIT_LUA); the other background means the \
         configured appearance did not reach Neovim; a count of 1 means Ctrl+Shift+T opened no \
         tab, so the tab bar had no height in the captures. The pixels would then fail for \
         reasons that are not the titlebar's."
    );
}

/// Asserts that `reading` is at least [`TB_MIN_EXACT_SHARE`] its expected colour.
fn tb_assert_band(smoke: &str, shot: &str, band: &str, reading: &TbReading, causes: &str) {
    assert!(
        reading.exact >= TB_MIN_EXACT_SHARE,
        "{smoke}: in the {shot} capture, the {band} band is {:.3} exactly {}, under \
         {TB_MIN_EXACT_SHARE}; its most common colour is {} at {:.3}. {causes} A share between \
         the two means the band reaches a glyph, an icon or an edge: re-derive it before \
         touching the threshold.",
        reading.exact,
        hex(reading.expected),
        hex(reading.dominant),
        reading.dominant_share,
    );
}

/// The smoke both backgrounds run; see the two tests' doc comments.
fn titlebar_smoke(smoke: &str, background: Background) {
    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let lighthouse = Lighthouse::from_env(smoke);
    let other = background.other();
    let name = background.name();
    let other_name = other.name();

    let bg = lighthouse.rgb8(background, "background");
    let other_bg = lighthouse.rgb8(other, "background");
    let wolf = rgb8(TB_WOLF_TITLEBAR);
    assert!(
        bg != other_bg && wolf != bg && wolf != other_bg,
        "{smoke}: Lighthouse's dark and light backgrounds and the WolfTitlebar fixture are not \
         three distinct colours ({} {}, {} {}, WolfTitlebar {}), so a bar left at one could pass \
         for another. Pick another TB_WOLF_TITLEBAR, or re-check the Lighthouse pin.",
        name,
        hex(bg),
        other_name,
        hex(other_bg),
        hex(wolf),
    );

    let init_lua = stage_init_lua(smoke, &runtime_dir, TB_STAGE_NAME, &lighthouse);
    let cwd = make_temp_cwd();
    let config = smoke_config_dir_at(&cwd, TB_CONFIG_SUFFIX, SMOKE_FONT_PT, name);

    let read = runtime_dir.join(TB_READ_NAME);
    let png_focused = runtime_dir.join(TB_PNG_FOCUSED_NAME);
    let png_live = runtime_dir.join(TB_PNG_LIVE_NAME);
    let png_wolf = runtime_dir.join(TB_PNG_WOLF_NAME);
    let png_backdrop = runtime_dir.join(TB_PNG_BACKDROP_NAME);
    for path in [&read, &png_focused, &png_live, &png_wolf, &png_backdrop] {
        clear_evidence(path, smoke);
    }

    // The desktop reports the scheme opposite to the configured appearance.
    let desktop = format!("prefer-{other_name}");
    let status = cage_command(TB_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("WOLF_INIT_LUA", &init_lua)
        .env("WOLF_CONFIG_DIR", &config)
        .env(ADW_COLOR_SCHEME_ENV, &desktop)
        // BACKDROP opens a native file dialog; see [`SAVE_AS_GDK_DEBUG`].
        .env("GDK_DEBUG", SAVE_AS_GDK_DEBUG)
        .env("READ", &read)
        .env("PNG_FOCUSED", &png_focused)
        .env("PNG_LIVE", &png_live)
        .env("PNG_WOLF", &png_wolf)
        .env("PNG_BACKDROP", &png_backdrop)
        .env("OTHER_BACKGROUND", other_name)
        .env("WOLF_TITLEBAR", hex(wolf))
        .status()
        .expect("failed to spawn `cage` for the headless titlebar scenario");

    // --- The scenario ran to completion ----------------------------------------
    assert!(
        status.success(),
        "{smoke}: the cage scenario exited unsuccessfully ({status}). Exit 11: the app died \
         before it was ready for input. Exit 12: the readback never appeared, so keys were not \
         reaching Neovim. Exit 40: the launch never became ready."
    );

    let bands = tb_bands();
    // BACKDROP's grid band is read for the table but not asserted: the save dialog covers it.
    let shots = [
        ("FOCUSED", &png_focused, bg, bg, true),
        ("LIVE", &png_live, other_bg, other_bg, true),
        ("WOLF", &png_wolf, wolf, other_bg, true),
        ("BACKDROP", &png_backdrop, wolf, other_bg, false),
    ];
    let mut table = format!(
        "{smoke}: appearance {name}, desktop {desktop}, against {}:\n\
         capture   band      expected  exact   dominant  share\n",
        lighthouse.path().display()
    );
    let readings = shots.map(|(shot, png, bar, grid, grid_asserted)| {
        assert_png_present(smoke, png);
        let capture = decode_capture(png, smoke, &format!("the {shot} capture"));
        let header = tb_read(&capture, &bands.header, bar, "the header bar band");
        let tab_bar = tb_read(&capture, &[bands.tab_bar], bar, "the tab bar band");
        let grid = tb_read(&capture, &[bands.grid], grid, "the grid band");
        for (band, reading, asserted) in [
            ("header", &header, true),
            ("tab bar", &tab_bar, true),
            ("grid", &grid, grid_asserted),
        ] {
            let mark = if asserted { "" } else { "   (unasserted)" };
            table.push_str(&format!(
                "{shot:<9} {band:<9} {}   {:.3}   {}   {:.3}{mark}\n",
                hex(reading.expected),
                reading.exact,
                hex(reading.dominant),
                reading.dominant_share
            ));
        }
        // The ink is read against the colour the bars should have, the configured scheme's side.
        let title = tb_ink(&capture, bands.title, bar, background);
        let tabs = tb_ink(&capture, bands.tab_ink, bar, background);
        table.push_str(&format!(
            "{shot:<9} ink       title {} px, max {:.1} (other side {:.1}); \
             tabs {} px, max {:.1} (other side {:.1})\n",
            title.pixels, title.max, title.other_max, tabs.pixels, tabs.max, tabs.other_max
        ));
        (shot, header, tab_bar, grid, title, tabs)
    });
    eprintln!("{table}");
    let [focused, live, wolf_shot, backdrop] = readings;

    // --- FOCUSED: the configured background on all three ------------------------
    tb_assert_readback(smoke, &read, background);
    let (shot, header, tab_bar, grid, title, tabs) = &focused;
    tb_assert_band(
        smoke,
        shot,
        "grid",
        grid,
        &format!(
            "The grid is the reference the bars must match: another colour means Lighthouse's \
             {name} Normal is not what the grid fills with, or the band is off the pane.\n{table}"
        ),
    );
    tb_assert_band(
        smoke,
        shot,
        "header",
        header,
        &format!(
            "A dominant colour from no Lighthouse palette is libadwaita's own header bar: the \
             stylesheet has no rule for the header bar, the header bar lacks the window's class, \
             or the provider is not on the display. The other background's {} means the colour \
             came from the wrong palette; channels in another order mean the hex is written \
             wrongly.\n{table}",
            hex(other_bg)
        ),
    );
    tb_assert_band(
        smoke,
        shot,
        "tab bar",
        tab_bar,
        &format!(
            "A dominant colour from no Lighthouse palette, with the header band green, is \
             libadwaita's own tab bar: the stylesheet's tab bar selector matches no node, or the \
             tab bar lacks the window's class. The sidebar's colour means the bar had no height, \
             so the second tab was not open (see the readback).\n{table}"
        ),
    );
    let side = match background {
        Background::Dark => "lighter",
        Background::Light => "darker",
    };
    assert!(
        title.pixels >= TB_INK_MIN_PIXELS,
        "{smoke}: the FOCUSED title band holds {} pixels at least {TB_INK_MIN_CONTRAST} luma \
         {side} than {}, under {TB_INK_MIN_PIXELS}; the largest difference is {:.1} on that \
         side and {:.1} on the other. The title is not drawn in libadwaita's {name} foreground. \
         Ink only on the other side is the {other_name} scheme's foreground, the desktop's \
         ({desktop}), so the forced appearance did not reach the chrome; no ink on either side \
         means no title was drawn in the band, or the stylesheet set its colour to the \
         background's.\n{table}",
        title.pixels,
        hex(bg),
        title.max,
        title.other_max,
    );
    assert!(
        tabs.pixels >= TB_INK_MIN_PIXELS,
        "{smoke}: the FOCUSED tab-ink band holds {} pixels at least {TB_INK_MIN_CONTRAST} luma \
         {side} than {}, under {TB_INK_MIN_PIXELS}; the largest difference is {:.1} on that \
         side and {:.1} on the other. The tabs' text is not drawn in libadwaita's {name} \
         foreground. Ink only on the other side is the {other_name} scheme's, the desktop's \
         ({desktop}); no ink on either side means no tab was drawn, so the bar had one page or \
         no height.\n{table}",
        tabs.pixels,
        hex(bg),
        tabs.max,
        tabs.other_max,
    );

    // --- LIVE: `:set background` moves all three --------------------------------
    let (shot, header, tab_bar, grid, _, _) = &live;
    tb_assert_band(
        smoke,
        shot,
        "grid",
        grid,
        &format!(
            "The grid did not follow `:set background={other_name}`: Lighthouse was not reloaded \
             for the new background, so the bars cannot be judged against it. {} is the old \
             background.\n{table}",
            hex(bg)
        ),
    );
    let stale = format!(
        "{} is the {name} background the bars had before `:set background={other_name}`: the \
         stylesheet was not rewritten when the editor background changed.\n{table}",
        hex(bg)
    );
    tb_assert_band(smoke, shot, "header", header, &stale);
    tb_assert_band(smoke, shot, "tab bar", tab_bar, &stale);

    // --- WOLF: a set WolfTitlebar beats Normal, which the grid keeps ------------
    let (shot, header, tab_bar, grid, wolf_title, _) = &wolf_shot;
    tb_assert_band(
        smoke,
        shot,
        "grid",
        grid,
        &format!(
            "The grid must keep Lighthouse's {other_name} Normal: WolfTitlebar colours the bars \
             alone.\n{table}"
        ),
    );
    let ignored = format!(
        "{} is Lighthouse's {other_name} Normal: the bars kept Normal's background with \
         WolfTitlebar set, so WolfTitlebar did not reach the rule or Normal is tested first.\n\
         {table}",
        hex(other_bg)
    );
    tb_assert_band(smoke, shot, "header", header, &ignored);
    tb_assert_band(smoke, shot, "tab bar", tab_bar, &ignored);

    // --- BACKDROP: the save dialog is up, and the bars keep WolfTitlebar's --------
    let (shot, header, tab_bar, grid, title, _) = &backdrop;
    assert!(
        wolf_title.max >= TB_BACKDROP_MIN_REFERENCE_INK,
        "{smoke}: WOLF's title band has a largest contrast of {:.1} on the {name} scheme's side \
         of WolfTitlebar's {}, under {TB_BACKDROP_MIN_REFERENCE_INK}, so it is no reference for \
         BACKDROP's dimming: a missing title would pass the ratio as 0 against 0. No contrast \
         on either side means no title was drawn in the band; {:.1} on the other side means the \
         title took the {other_name} scheme's foreground. Nothing below is evidence about \
         `:backdrop`.\n{table}",
        wolf_title.max,
        hex(wolf),
        wolf_title.other_max,
    );
    let lead = if grid.exact >= TB_MIN_EXACT_SHARE {
        format!(
            "BACKDROP's grid band is {:.3} the grid's colour, so no dialog opened: Ctrl+Shift+S \
             answered Write because the buffer after `:enew` was named, or the dialog went to a \
             portal.",
            grid.exact
        )
    } else {
        format!(
            "BACKDROP's grid band is only {:.3} the grid's colour, so the dialog opened and \
             covers it, but the main window was not dimmed: cage did not deactivate it, or \
             libadwaita's backdrop dimming has changed.",
            grid.exact
        )
    };
    assert!(
        title.max <= TB_BACKDROP_MAX_INK_RATIO * wolf_title.max,
        "{smoke}: the BACKDROP title's largest contrast, {:.1}, is over \
         {TB_BACKDROP_MAX_INK_RATIO} of WOLF's, {:.1}. libadwaita dims an inactive header bar's \
         contents to half opacity, so the main window was still active in BACKDROP. {lead} \
         Either way, nothing below is evidence about `:backdrop`.\n{table}",
        title.max,
        wolf_title.max,
    );
    let backdrop_colour = format!(
        "With WOLF green, a dominant colour from no Lighthouse palette is libadwaita's \
         --headerbar-backdrop-color: the stylesheet's rule does not match in `:backdrop`, or a \
         theme rule outranks it there. A dominant colour that is the save dialog's own \
         background means the dialog reached the bars: re-check its measured size and where \
         cage centres it.\n{table}"
    );
    tb_assert_band(smoke, shot, "header", header, &backdrop_colour);
    tb_assert_band(smoke, shot, "tab bar", tab_bar, &backdrop_colour);
}

/// **The header bar and the tab bar take Lighthouse's dark background under cage, while the
/// desktop prefers light.**
///
/// The checkout binary runs over a staged `init.lua` beside a copy of the vendored Lighthouse
/// (`WOLF_INIT_LUA`), with `appearance: dark` and `ADW_DEBUG_COLOR_SCHEME=prefer-light`. The
/// expected colours come from the pinned `lighthouse.lua` through [`Lighthouse::from_env`],
/// read in this process, and every run prints a table of expected and measured colours.
///
/// ## Assertions
///
/// | # | capture | assertion | kind | falsifier |
/// | --- | --- | --- | --- | --- |
/// | 1 | all | the backgrounds and the WolfTitlebar fixture are distinct | precondition | none |
/// | 2 | all | the scenario exits 0 | precondition | none |
/// | 3 | FOCUSED | readback `lighthouse`, `dark`, `2` | precondition | none |
/// | 4 | FOCUSED | grid, header and tab bar are the dark background | discriminator | C8 drops the header selector, C9 the tab bar's |
/// | 5 | FOCUSED | the title and the tabs' text are drawn lighter than the background | discriminator | C13a: `Appearance::Dark` gives `ColorScheme::Default` |
/// | 6 | LIVE | grid, header and tab bar are the light background | discriminator | C11: `apply` stops after the first colour |
/// | 7 | WOLF | the grid is the light background, and header and tab bar `WolfTitlebar`'s | discriminator | C12: `refresh_titlebar` passes `None` for `WolfTitlebar` |
/// | 8 | BACKDROP | WOLF's title contrast reaches [`TB_BACKDROP_MIN_REFERENCE_INK`], and BACKDROP's is at most [`TB_BACKDROP_MAX_INK_RATIO`] of it | precondition | none |
/// | 9 | BACKDROP | header and tab bar are `WolfTitlebar`'s | discriminator | C10: `:not(:backdrop)` on both selectors |
///
/// BACKDROP's grid band is printed in the table, marked unasserted, because the save dialog
/// covers it. Its exact share is what assertion 8's message reads to tell a dialog that
/// never opened from a window that was not dimmed.
#[test]
fn header_bar_and_tab_bar_take_the_editor_background_dark_under_cage() {
    titlebar_smoke(
        "header_bar_and_tab_bar_take_the_editor_background_dark_under_cage",
        Background::Dark,
    );
}

/// **The header bar and the tab bar take Lighthouse's light background under cage, while the
/// desktop prefers dark.**
///
/// The dark smoke's scenario and assertions with the backgrounds swapped: `appearance: light`
/// and `ADW_DEBUG_COLOR_SCHEME=prefer-dark`, the title and the tabs' text drawn darker than
/// the background, and LIVE, WOLF and BACKDROP over the dark background. Its title over
/// `WolfTitlebar` has the least contrast of the two smokes, about 57 focused, which is what
/// [`TB_BACKDROP_MIN_REFERENCE_INK`] is set under. Falsifier C13b, the
/// `Appearance::Light` arm giving `ColorScheme::Default`, reddens assertion 5. Without
/// `ADW_DEBUG_COLOR_SCHEME` the cage's desktop reads light and C13b would stay green, so its
/// red is also what shows that the variable reaches the app.
#[test]
fn header_bar_and_tab_bar_take_the_editor_background_light_under_cage() {
    titlebar_smoke(
        "header_bar_and_tab_bar_take_the_editor_background_light_under_cage",
        Background::Light,
    );
}
