//! The `gtk-titlebar-style` smokes: under `tabs`, the key's default, a one-tab window is the
//! window of today and a second tab moves the tab bar into the header bar, where the window
//! title was, with no row below it; closing back to one tab restores the title. Under `native`
//! the second tab opens the tab bar's own row below a titled header.
//!
//! Each smoke launches on a folder tab at the temp cwd `project`, opens a second tab with
//! Ctrl+Shift+T and `:cd`s it into [`HT_DIR`], a sibling of `project` under the smoke's `HOME`,
//! so the two tabs are titled `~/project` and `~/`[`HT_DIR`]. Every OCR reading is cropped to
//! a band, so the sidebar's root row, which draws the same names, is never read.
//!
//! The header band is the header bar's rows across the output, from [`TB_HEADER_ROW_INSET_PX`]
//! down to that inset above the capture's own shade row. The shade row is the first row whose
//! colour differs from the header's, scanning down [`HT_SHADE_COLUMN_X_PX`] from the header's
//! centre row, clear of any window corner: the header bar's `box-shadow` row, `titlebar.rs`'s
//! [`TB_HEADER_SHADE_PX`]. It is measured in each capture rather than assumed, so a header that
//! grew moves it.
//!
//! The pointer helper's files and every readback live under the runtime dir, never in the cwd.
//!
//! Premises, measured by the green run: an in-header tab's title `~/<name>` OCR-reads as
//! `<name>` at the smoke font in the header band, as `new_tab.rs`'s smokes read the row's
//! titles; and a new tab `:cd`ed out of its empty state shows the sidebar its origin showed
//! (`specs/application-state.md` §7), so its pane has the one-tab x origin.

use super::extras::{require, EXTRAS_HELPERS};
use super::open_folder::make_dir;
use super::panes::POINTER_BUTTON_MIDDLE;
use super::pointer::assert_fixture_fits;
use super::recovery::with_nvim_pid_helper;
use super::sidebar_toggles::tg_click_y;
use super::tab_bar::tab_bar_band;
use super::titlebar::{
    hex, tb_read, tb_rect, TB_HEADER_BUTTON_CLEARANCE_PX, TB_HEADER_ROW_INSET_PX,
    TB_HEADER_SHADE_PX, TB_MIN_EXACT_SHARE, TB_TITLE_HALF_WIDTH_PX,
};
use super::*;

/// The second tab's directory, a sibling of `project` under the smoke's `HOME`.
pub(crate) const HT_DIR: &str = "htheron";

/// What OCR must find for the second tab's title, `~/`[`HT_DIR`].
pub(crate) const HT_OCR_DIR: &str = "htheron";

/// What OCR must find for the first tab's title, and the one-tab window's, `~/project`.
///
/// Not in `ASSERTED_OCR_TERMS`: it is [`TEMP_CWD_BASENAME`], which the temp-cwd guard refuses
/// by design. Only the window title and a tab title draw it in the header band, because the
/// band holds nothing else that is text.
const HT_OCR_PROJECT: &str = TEMP_CWD_BASENAME;

/// How much taller the header bar is with two tabs in it under `tabs` than with one.
///
/// `0.0` is the value the in-header stylesheet is designed to hold: the tab bar's tab box and
/// its `box.box` lose their vertical padding, so the bar is a tab's own 34 px and the header
/// pads it to its natural 47. The header's height at one and at two tabs, measured under `cage`
/// with a temporary tick callback as [`OVERLAY_ORIGIN_X_EXPECTED`] records, confirms it.
const TABS_HEADER_GROWTH_PX: f64 = 0.0;

/// The column the shade row is scanned down: inside the header bar's 7 px start padding and
/// clear of its 1 px side border, so no button and, under `tabs`, no tab reaches it.
const HT_SHADE_COLUMN_X_PX: f64 = 3.0;

/// How far left of the output's centre the band inside the first tab's entry stops.
///
/// Under `tabs` the bar is the header's centred, expanding title widget, so two tabs meet at
/// the output's centre. This clears the seam, the tab's 4 px padding and the 24 px close button
/// a tab may draw at its end.
const HT_TAB_END_CLEARANCE_PX: f64 = 40.0;

/// Half the height of the band inside the first tab's entry, around the header's centre row.
/// The tab is 34 px tall and centred there; its title is out of the band's columns.
const HT_TAB_BAND_HALF_HEIGHT_PX: f64 = 6.0;

/// libadwaita 1.9.3's dark-scheme `@define-color headerbar_bg_color #2e2e32`, which its
/// `tabbar .box` also fills with. Read from the flake's `libadwaita-1.so` stylesheet resource,
/// `/org/gnome/Adwaita/styles/gtk.css`; the smokes pin `appearance: dark`.
const ADW_DARK_HEADERBAR_BG: Rgb8 = (0x2e, 0x2e, 0x32);

/// Columns the delta click moves RIGHT from the second tab's probe cell.
const HT_CLICK_DX: i64 = 3;

/// Rows the delta click moves DOWN from the second tab's probe cell.
const HT_CLICK_DY: i64 = 2;

/// The exit codes [`HT_BODY`] and [`HN_BODY`] add to [`POINTER_PREAMBLE`]'s.
const HT_EXITS: std::ops::RangeInclusive<i32> = 170..=175;

/// What each of the exit codes in [`HT_EXITS`] means, for failure messages.
const HT_CODES: &str = "Exit 170: the launch did not run exactly one Neovim. 171: a readback \
     in the first tab never appeared. 172: Ctrl+Shift+T started no second Neovim. 173: a \
     readback in the second tab never appeared. 174: the middle-click on the second tab's \
     header entry did not leave exactly one Neovim. 175: no readback appeared after that close, \
     so keys no longer reach Neovim.";

/// The shell helpers the smokes' bodies use, after [`EXTRAS_HELPERS`]: a readback of the
/// focused window's size or cursor, exiting with the second argument when it never appears,
/// and a left click that parks the pointer back at the probe.
const HT_HELPERS: &str = r#"
read_dims() {
    wtype -- ":call writefile([printf('%d %d', winwidth(0), winheight(0))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

read_cursor() {
    wtype -- ":call writefile([printf('%d %d', line('.'), col('.'))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

click_at() {
    hsend motion "$1" "$2"
    hsend press "$BTN_LEFT"
    hsend release "$BTN_LEFT"
    sleep "$SETTLE"
}
"#;

/// Body of [`tabs_join_the_header_bar_from_the_second_tab_under_cage`], appended to
/// [`POINTER_PREAMBLE`], `recovery::with_nvim_pid_helper`'s helper, [`EXTRAS_HELPERS`] and
/// [`HT_HELPERS`].
///
/// 1. One tab: `fill_buffer`, readback D0, probe click P1, capture S1.
/// 2. Ctrl+Shift+T, two Neovims, `:cd ~/`[`HT_DIR`], `fill_buffer` and the bar's settle;
///    readback D1, capture S2, probe click P2 and a click [`HT_CLICK_DX`] by [`HT_CLICK_DY`]
///    cells from it, P3.
/// 3. `:set nomodified` in the second tab, then a middle-click on its header entry, one Neovim,
///    the bar's settle, capture S3 and readback A from the first tab.
const HT_BODY: &str = r#"
wait_nvim_count 1 170 "at launch, with one folder tab"

fill_buffer
read_dims "$HT_D0" 171
click_at "$PROBE_X" "$PROBE_Y"
read_cursor "$HT_P1" 171
grim "$HT_S1"

wtype -M ctrl -M shift -k t -m shift -m ctrl
wait_nvim_count 2 172 "after Ctrl+Shift+T opened a second tab"
sleep "$TABS_BAR_SETTLE"
# New Tab's tab is empty, its Neovim at HOME; the :cd makes it a folder tab titled ~/$HT_DIR.
nvim_ex "cd ~/$HT_DIR"
fill_buffer
sleep "$TABS_BAR_SETTLE"
read_dims "$HT_D1" 173
grim "$HT_S2"
click_at "$PROBE_X" "$PROBE_Y"
read_cursor "$HT_P2" 173
click_at "$HT_TARGET_X" "$HT_TARGET_Y"
read_cursor "$HT_P3" 173

# A tab's close asks its Neovim first, which refuses while fill_buffer leaves the second tab
# modified; so the edit is dropped first, in the second tab, where read_cursor proved keys go.
nvim_ex "set nomodified"
hsend motion "$HT_CLOSE_X" "$HT_CLOSE_Y"
hsend press "$BTN_MIDDLE"
hsend release "$BTN_MIDDLE"
hsend motion "$PROBE_X" "$PROBE_Y"
wait_nvim_count 1 174 "after the middle-click on the second tab's header entry"
sleep "$TABS_BAR_SETTLE"
grim "$HT_S3"
read_cursor "$HT_A" 175

hsend quit
exit 0
"#;

/// Body of [`native_keeps_the_tab_bar_row_under_cage`], appended as [`HT_BODY`] is: readback
/// D0 at one tab, then Ctrl+Shift+T, `:cd ~/`[`HT_DIR`], the bar's settle, capture N and
/// readback D1 from the second tab.
const HN_BODY: &str = r#"
wait_nvim_count 1 170 "at launch, with one folder tab"

fill_buffer
read_dims "$HT_D0" 171

wtype -M ctrl -M shift -k t -m shift -m ctrl
wait_nvim_count 2 172 "after Ctrl+Shift+T opened a second tab"
sleep "$TABS_BAR_SETTLE"
nvim_ex "cd ~/$HT_DIR"
sleep "$TABS_BAR_SETTLE"
grim "$HT_N"
read_dims "$HT_D1" 173

hsend quit
exit 0
"#;

/// The output's width as an `f64`.
fn ht_width() -> f64 {
    exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED")
}

/// `row`, a capture's pixel row, as an `f64` output coordinate.
fn ht_row_f64(row: usize, what: &str) -> f64 {
    let row = i64::try_from(row).unwrap_or_else(|e| panic!("{what}: row {row} overflows ({e})"));
    exact_f64(row, what)
}

/// The header bar's shade row in `capture`; see the module doc.
fn ht_shade_row(capture: &Capture, smoke: &str, shot: &str) -> f64 {
    let x = pixel_index(HT_SHADE_COLUMN_X_PX, "the shade column");
    let top = pixel_index(tg_click_y(), "the shade scan's first row");
    let stop = pixel_index(PANE_ORIGIN_Y_EXPECTED, "the shade scan's last row");
    let header = capture.pixel(x, top);
    let row = (top..stop)
        .find(|&y| capture.pixel(x, y) != header)
        .unwrap_or_else(|| {
            panic!(
                "{smoke}: in capture {shot}, column {x} is {} from row {top} down to the one-tab \
                 grid origin {PANE_ORIGIN_Y_EXPECTED}, so no header shade row was found: the \
                 window drew no header bar there, or the bar and everything below it share one \
                 colour.",
                hex(header)
            )
        });
    ht_row_f64(row, "the shade row")
}

/// The header band of a capture whose shade row is `shade`, across the whole output.
fn ht_header_band(shade: f64) -> SampleRect {
    tb_rect(
        0.0,
        ht_width(),
        TB_HEADER_ROW_INSET_PX,
        shade - TB_HEADER_ROW_INSET_PX,
        "the header band",
    )
}

/// The grid's background, right of the `fill_buffer` text and below its first two rows, in a
/// pane area whose top edge is `pane_top`. Laid out as `titlebar.rs`'s `tb_bands` lays its grid
/// band, starting [`FIXTURE_LINE_WIDTH`] plus two cells right of the grid's origin: one for the
/// probe's sub-cell phase in [`PANE_ORIGIN_X_EXPECTED`] and one of margin.
fn ht_grid_band(pane_top: f64) -> SampleRect {
    let width = ht_width();
    let height = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    let cell = cell_metrics_for_smoke();
    let statusbar = wolf_gnome::statusbar_layout::statusbar_layout(
        cell.size_px,
        wolf_gnome::statusbar_layout::secondary_font_size(cell.size_px),
        wolf_gnome::statusbar_layout::STATUSBAR_VERTICAL_PADDING,
        wolf_gnome::statusbar_layout::STATUSBAR_LINE_GAP,
    );
    let rect = wolf_gnome::editor_inset::grid_rect(
        width - OVERLAY_ORIGIN_X_EXPECTED,
        height - pane_top,
        statusbar.height,
        wolf_gnome::editor_inset::EDITOR_PADDING_PX,
        wolf_gnome::editor_inset::column_cap(true, cell.cell_w),
    );
    let text_cells = exact_f64(FIXTURE_LINE_WIDTH + 2, "the fixture's width in cells");
    tb_rect(
        PANE_ORIGIN_X_EXPECTED + text_cells * cell.cell_w,
        width - BAND_X_INSET_PX,
        pane_top + rect.y.round() + 2.0 * cell.cell_h,
        height - BAND_BOTTOM_CLEARANCE_PX,
        "the grid band",
    )
}

/// The header bar's background either side of the title, as `titlebar.rs`'s `tb_bands` places
/// its header spans, for a capture whose shade row is `shade`.
fn ht_header_spans(shade: f64) -> [SampleRect; 2] {
    let width = ht_width();
    let centre = width / 2.0;
    let bottom = shade - TB_HEADER_ROW_INSET_PX;
    [
        tb_rect(
            TB_HEADER_BUTTON_CLEARANCE_PX,
            centre - TB_TITLE_HALF_WIDTH_PX,
            TB_HEADER_ROW_INSET_PX,
            bottom,
            "the header bar's span before the title",
        ),
        tb_rect(
            centre + TB_TITLE_HALF_WIDTH_PX,
            width - TB_HEADER_BUTTON_CLEARANCE_PX,
            TB_HEADER_ROW_INSET_PX,
            bottom,
            "the header bar's span after the title",
        ),
    ]
}

/// The band inside the first tab's header entry, clear of its title, its end and the seam.
fn ht_first_tab_band() -> SampleRect {
    let centre = ht_width() / 2.0;
    tb_rect(
        centre - TB_TITLE_HALF_WIDTH_PX,
        centre - HT_TAB_END_CLEARANCE_PX,
        tg_click_y() - HT_TAB_BAND_HALF_HEIGHT_PX,
        tg_click_y() + HT_TAB_BAND_HALF_HEIGHT_PX,
        "the band inside the first tab's entry",
    )
}

/// The `(width, height)` readback at `path`.
fn ht_dims(path: &Path, smoke: &str) -> (i64, i64) {
    let dims = read_ints(path, 2, smoke);
    (dims[0], dims[1])
}

/// The `(line, col)` readback at `path`.
fn ht_cell(path: &Path, smoke: &str) -> (i64, i64) {
    let cell = read_ints(path, 2, smoke);
    (cell[0], cell[1])
}

/// Asserts the scenario left none of [`HT_EXITS`]'s codes, then that it exited 0.
fn ht_assert_completed(smoke: &str, status: ExitStatus, rt: &Path, png: &Path) {
    assert!(
        !status.code().is_some_and(|code| HT_EXITS.contains(&code)),
        "{smoke}: the cage scenario exited with one of its own codes ({status}). {HT_CODES} The \
         cage log in the runtime dir carries the scenario's own lines."
    );
    assert_pointer_scenario_succeeded(smoke, status, rt, png);
}

/// Asserts the shared seeded config, which this smoke's app reads, carries no
/// `gtk-titlebar-style`, so the app runs under the key's default.
fn ht_assert_default_style(smoke: &str, cwd: &Path) {
    let path = smoke_config_dir(cwd).join(CONFIG_FILE_NAME);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{smoke}: cannot read {}: {e}", path.display()));
    assert!(
        !text.contains("gtk-titlebar-style"),
        "{smoke}: precondition: the shared seeded config {} carries a `gtk-titlebar-style`, so \
         this smoke no longer shows the key's default reaching the window:\n{text}",
        path.display()
    );
}

/// **Under `gtk-titlebar-style: tabs`, the default, a second tab moves the tab bar into the
/// header bar, with no row below it, and closing back to one tab restores the window title.**
///
/// The app reads the shared seeded config, which carries no key, so the style is the key's
/// default. The second tab's pane origin is the one-tab origin moved by the header's measured
/// growth, S2's shade row minus S1's, which B3 holds at [`TABS_HEADER_GROWTH_PX`]. The close is
/// a middle-click at the output's centre plus [`TB_TITLE_HALF_WIDTH_PX`], on the header's
/// centre row: right of the seam where the centred bar's two tabs meet, so on the second tab.
///
/// The evidence of steps 1 and 2 is checked before the scenario's status, so a close that
/// failed leaves B1 to B4 readable. Assertions run in the order of the table.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15 and 40
///
/// | code | meaning |
/// | --- | --- |
/// | `170` | the launch did not run exactly one Neovim |
/// | `171` | readback D0 or P1 never appeared in the first tab |
/// | `172` | Ctrl+Shift+T started no second Neovim |
/// | `173` | readback D1, P2 or P3 never appeared in the second tab |
/// | `174` | the middle-click on the second tab's header entry did not leave one Neovim |
/// | `175` | readback A never appeared after the close: keys no longer reach Neovim |
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: the shared config has no key; S1's header spans and the grid share one colour, which is not [`ADW_DARK_HEADERBAR_BG`] | none | n/a |
/// | B1 | S1's header band holds `project`; P1 passes `assert_probe_cell_and_pane_origin`; S1's shade row is `OVERLAY_ORIGIN_Y_EXPECTED − TB_HEADER_SHADE_PX` | C4a: `TabBarSlot`'s apply always takes the `InHeader` arm | S1's header lacks `project` |
/// | B2 | S2's header band holds `project` and [`HT_OCR_DIR`]; D1's rows equal D0's; P2 passes the probe assertion against the one-tab origin moved by S2's shade row minus S1's; P3 is P2 moved by the click's cells | C4b: do not connect the `n-pages` notify | S2's header lacks `project`, or D1 is a row short |
/// | B3 | S2's shade row is S1's plus [`TABS_HEADER_GROWTH_PX`] | C4c: skip installing the in-header stylesheet | S2's shade row about 13 px below S1's |
/// | B4 | in S2, the band inside the first tab's entry is at least [`TB_MIN_EXACT_SHARE`] the grid's colour | C4d: in `TitlebarStyle::install`, delete `tab_bar.add_css_class(&class)` | the band reads [`ADW_DARK_HEADERBAR_BG`] |
/// | B5 | S3's header band holds `project`; S3's shade row is S1's; readback A is P1's cell | C4e: the `BelowHeader` arm returns without moving the bar once it is in the header | S3's header lacks `project` |
#[test]
fn tabs_join_the_header_bar_from_the_second_tab_under_cage() {
    const SMOKE: &str = "tabs_join_the_header_bar_from_the_second_tab_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    make_dir(SMOKE, smoke_home(&cwd), HT_DIR);
    let metrics = cell_metrics_for_smoke();

    let rt: &Path = &runtime_dir;
    let d0 = rt.join("wolf_gnome_ht_d0.txt");
    let d1 = rt.join("wolf_gnome_ht_d1.txt");
    let p1 = rt.join("wolf_gnome_ht_p1.txt");
    let p2 = rt.join("wolf_gnome_ht_p2.txt");
    let p3 = rt.join("wolf_gnome_ht_p3.txt");
    let after = rt.join("wolf_gnome_ht_a.txt");
    let s1 = rt.join("wolf_gnome_ht_s1.png");
    let s2 = rt.join("wolf_gnome_ht_s2.png");
    let s3 = rt.join("wolf_gnome_ht_s3.png");
    for path in [&d0, &d1, &p1, &p2, &p3, &after, &s1, &s2, &s3] {
        clear_evidence(path, SMOKE);
    }

    let target_x = pixel_target(
        f64::from(PROBE_X) + exact_f64(HT_CLICK_DX, "HT_CLICK_DX") * metrics.cell_w,
        "the delta click's x",
    );
    let target_y = pixel_target(
        f64::from(PROBE_Y) + exact_f64(HT_CLICK_DY, "HT_CLICK_DY") * metrics.cell_h,
        "the delta click's y",
    );
    let close_x = pixel_target(
        ht_width() / 2.0 + TB_TITLE_HALF_WIDTH_PX,
        "the second tab's header entry's x",
    );
    let close_y = pixel_target(tg_click_y(), "the header's centre row");

    let body = with_nvim_pid_helper(&format!("{EXTRAS_HELPERS}\n{HT_HELPERS}\n{HT_BODY}"));
    let status = pointer_command(&body, &cwd, &runtime_dir, &s3)
        .env("CMD_FIFO", rt.join(POINTER_CMD_FIFO_NAME))
        .env("ACK_FILE", rt.join(POINTER_ACK_NAME))
        .env("HELPER_ERR", pointer_err_path(rt))
        .env("READY_FILE", pointer_ready_path(rt))
        .env("BTN_MIDDLE", POINTER_BUTTON_MIDDLE)
        .env("HT_DIR", HT_DIR)
        .env("HT_TARGET_X", target_x.to_string())
        .env("HT_TARGET_Y", target_y.to_string())
        .env("HT_CLOSE_X", close_x.to_string())
        .env("HT_CLOSE_Y", close_y.to_string())
        .env("HT_D0", &d0)
        .env("HT_D1", &d1)
        .env("HT_P1", &p1)
        .env("HT_P2", &p2)
        .env("HT_P3", &p3)
        .env("HT_A", &after)
        .env("HT_S1", &s1)
        .env("HT_S2", &s2)
        .env("HT_S3", &s3)
        .status()
        .expect("failed to spawn `cage` for the headless titlebar-tabs scenario");

    require(
        SMOKE,
        status,
        &[&d0, &p1, &s1, &d1, &s2, &p2, &p3],
        "1 and 2",
        HT_CODES,
    );
    ht_assert_default_style(SMOKE, &cwd);
    let extents = read_output_extents(rt, SMOKE);
    for (label, point) in [
        ("the probe", (i64::from(PROBE_X), i64::from(PROBE_Y))),
        ("the delta click", (target_x, target_y)),
        ("the second tab's header entry", (close_x, close_y)),
    ] {
        assert_inside_extents(SMOKE, label, point, extents);
    }

    let cap1 = decode_capture(&s1, SMOKE, "capture S1");
    let cap2 = decode_capture(&s2, SMOKE, "capture S2");
    let shade1 = ht_shade_row(&cap1, SMOKE, "S1");
    let shade2 = ht_shade_row(&cap2, SMOKE, "S2");
    let growth = shade2 - shade1;

    // --- Precondition: the header takes the grid's colour, which is not libadwaita's ---------
    let grid1 = tb_read(
        &cap1,
        &[ht_grid_band(OVERLAY_ORIGIN_Y_EXPECTED)],
        (0, 0, 0),
        "S1's grid band",
    );
    let grid_bg = grid1.dominant;
    assert!(
        grid1.dominant_share >= TB_MIN_EXACT_SHARE,
        "{SMOKE}: precondition: S1's grid band is only {:.3} its most common colour {}, so it is \
         not the grid's bare background: it reaches the fixture's text, the cursor or the \
         statusbar. Re-derive `ht_grid_band` before reading anything below. PNG kept at {}.",
        grid1.dominant_share,
        hex(grid_bg),
        s1.display()
    );
    assert_ne!(
        grid_bg, ADW_DARK_HEADERBAR_BG,
        "{SMOKE}: precondition: the grid's background is libadwaita's own dark header colour, so \
         B4 could not tell a themed tab bar from an unthemed one. Seed another colour scheme."
    );
    let header1 = tb_read(
        &cap1,
        &ht_header_spans(shade1),
        grid_bg,
        "S1's header spans",
    );
    assert!(
        header1.exact >= TB_MIN_EXACT_SHARE,
        "{SMOKE}: precondition: S1's header spans are {:.3} the grid's {}, most commonly {} at \
         {:.3}. The header bar does not take the editor background in this setup at all, which \
         `titlebar.rs`'s smokes cover; B4 is unreadable until it does. PNG kept at {}.",
        header1.exact,
        hex(grid_bg),
        hex(header1.dominant),
        header1.dominant_share,
        s1.display()
    );

    // --- B1: one tab under `tabs` is the window of today -------------------------------------
    let band1 = rt.join("wolf_gnome_ht_s1_header.png");
    let header_ocr1 = ocr_band_to_lowercase(&s1, ht_header_band(shade1), &band1, SMOKE);
    assert!(
        header_ocr1.contains(HT_OCR_PROJECT),
        "{SMOKE}: B1: S1, one tab under `tabs`, has no {HT_OCR_PROJECT:?} in its header band, so \
         the window title `~/{TEMP_CWD_BASENAME}` is not drawn: the one-tab window put the bar \
         in the header, where an autohidden bar of one tab draws nothing and replaces the title. \
         Crop kept at {}.\nHeader OCR:\n{header_ocr1}",
        band1.display()
    );
    let (l1, c1) = ht_cell(&p1, SMOKE);
    assert_probe_cell_and_pane_origin(
        SMOKE,
        (l1, c1),
        &metrics,
        extents,
        &s1,
        &read_readback(&p1, SMOKE),
    );
    assert_eq!(
        shade1,
        OVERLAY_ORIGIN_Y_EXPECTED - TB_HEADER_SHADE_PX,
        "{SMOKE}: B1: S1's header shade row is {shade1}, not the one-tab header's last row: a \
         one-tab window under `tabs` has a header of another height. PNG kept at {}.",
        s1.display()
    );

    // --- B2: two tabs put the bar in the header, with no row below it ------------------------
    let band2 = rt.join("wolf_gnome_ht_s2_header.png");
    let header_ocr2 = ocr_band_to_lowercase(&s2, ht_header_band(shade2), &band2, SMOKE);
    assert!(
        header_ocr2.contains(HT_OCR_PROJECT) && header_ocr2.contains(HT_OCR_DIR),
        "{SMOKE}: B2: S2, two tabs under `tabs`, must show both tab titles, ~/{TEMP_CWD_BASENAME} \
         and ~/{HT_DIR}, in its header band. Only {HT_OCR_DIR:?} is the window title of the \
         selected second tab: the bar stayed in its row below the header, so the placement was \
         not re-applied when the second tab opened. Neither means the second tab is not titled \
         by its directory. Crop kept at {}.\nHeader OCR:\n{header_ocr2}",
        band2.display()
    );
    let (_, rows0) = ht_dims(&d0, SMOKE);
    let dims1 = ht_dims(&d1, SMOKE);
    assert_eq!(
        dims1.1, rows0,
        "{SMOKE}: B2: the second tab's pane has {} rows with two tabs open, the first tab's had \
         {rows0} alone. A tab bar row of {TAB_BAR_HEIGHT_EXPECTED} px is taller than a {} px \
         cell, so fewer rows means the row below the header is shown; more means the second \
         tab's pane is taller than a one-tab window's.",
        dims1.1, metrics.cell_h
    );
    let (l2, c2) = ht_cell(&p2, SMOKE);
    assert_probe_cell_and_pane_origin_at(
        SMOKE,
        (l2, c2),
        &metrics,
        extents,
        &s2,
        &read_readback(&p2, SMOKE),
        PANE_ORIGIN_Y_EXPECTED + growth,
    );
    assert_fixture_fits(SMOKE, dims1, l2, HT_CLICK_DY);
    assert!(
        c2 + HT_CLICK_DX <= FIXTURE_LINE_WIDTH,
        "{SMOKE}: precondition: the probe landed at column {c2}, so the delta click's column \
         would be past the {FIXTURE_LINE_WIDTH}-character fixture line and clamp to its end."
    );
    let (l3, c3) = ht_cell(&p3, SMOKE);
    assert_eq!(
        (l3, c3),
        (l2 + HT_CLICK_DY, c2 + HT_CLICK_DX),
        "{SMOKE}: B2: the click {HT_CLICK_DX} cells right and {HT_CLICK_DY} down of the second \
         tab's probe cell ({l2}, {c2}) put the cursor at ({l3}, {c3}). The pointer does not map \
         onto the second tab's grid at the cell this geometry predicts. PNG kept at {}.",
        s2.display()
    );

    // --- B3: the header keeps its height ------------------------------------------------------
    assert_eq!(
        growth,
        TABS_HEADER_GROWTH_PX,
        "{SMOKE}: B3: S2's header shade row is {shade2}, S1's {shade1}: the header bar grew by \
         {growth} px with the tabs in it. About 13 px means the in-header stylesheet did not \
         strip the tab bar's vertical padding, or is not installed. PNGs kept at {} and {}.",
        s1.display(),
        s2.display()
    );

    // --- B4: the in-header bar takes the editor background -----------------------------------
    let grid2 = tb_read(
        &cap2,
        &[ht_grid_band(OVERLAY_ORIGIN_Y_EXPECTED + growth)],
        grid_bg,
        "S2's grid band",
    );
    assert!(
        grid2.exact >= TB_MIN_EXACT_SHARE,
        "{SMOKE}: precondition: S2's grid band is {:.3} the grid colour S1 read, {}, most \
         commonly {} at {:.3}, so the second tab's grid is not the reference B4 compares with. \
         PNG kept at {}.",
        grid2.exact,
        hex(grid_bg),
        hex(grid2.dominant),
        grid2.dominant_share,
        s2.display()
    );
    let tab0 = tb_read(
        &cap2,
        &[ht_first_tab_band()],
        grid_bg,
        "the first tab's band",
    );
    assert!(
        tab0.exact >= TB_MIN_EXACT_SHARE,
        "{SMOKE}: B4: in S2 the band inside the first tab's header entry is {:.3} the grid's {}, \
         most commonly {} at {:.3}. {} is libadwaita's own tab bar: the in-header bar lacks the \
         window's titlebar class, or the stylesheet's tab bar selector does not match it there. \
         A share between the two means the band reaches the tab's title or its close button. \
         PNG kept at {}.",
        tab0.exact,
        hex(grid_bg),
        hex(tab0.dominant),
        tab0.dominant_share,
        hex(ADW_DARK_HEADERBAR_BG),
        s2.display()
    );

    // --- B5: closing back to one tab restores the title --------------------------------------
    ht_assert_completed(SMOKE, status, rt, &s3);
    assert_png_present(SMOKE, &s3);
    let cap3 = decode_capture(&s3, SMOKE, "capture S3");
    let shade3 = ht_shade_row(&cap3, SMOKE, "S3");
    let band3 = rt.join("wolf_gnome_ht_s3_header.png");
    let header_ocr3 = ocr_band_to_lowercase(&s3, ht_header_band(shade3), &band3, SMOKE);
    assert!(
        header_ocr3.contains(HT_OCR_PROJECT),
        "{SMOKE}: B5: S3, back to one tab, has no {HT_OCR_PROJECT:?} in its header band, so the \
         window title is not drawn: the bar stayed in the header, autohidden, when the second \
         tab closed. Crop kept at {}.\nHeader OCR:\n{header_ocr3}",
        band3.display()
    );
    assert_eq!(
        shade3,
        shade1,
        "{SMOKE}: B5: S3's header shade row is {shade3}, S1's {shade1}: the one-tab window after \
         the close does not have the header it launched with. PNG kept at {}.",
        s3.display()
    );
    let cell_after = ht_cell(&after, SMOKE);
    assert_eq!(
        cell_after,
        (l1, c1),
        "{SMOKE}: B5: readback A, typed after the close, reports the cursor at {cell_after:?}, \
         not the first tab's P1 cell ({l1}, {c1}): the keys reached a Neovim other than the \
         surviving first tab's."
    );
}

/// **Under `gtk-titlebar-style: native`, a second tab keeps today's layout: the header bar
/// draws the window title and the tab bar its own row below it.**
///
/// The smoke pins `native` through [`titlebar_pinned_config_dir`]. Capture N is read three
/// ways: the header band, the row band `tab_bar.rs`'s `tab_bar_band` reads, and D1's rows
/// against D0's.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15 and 40
///
/// | code | meaning |
/// | --- | --- |
/// | `170` | the launch did not run exactly one Neovim |
/// | `171` | readback D0 never appeared in the first tab |
/// | `172` | Ctrl+Shift+T started no second Neovim |
/// | `173` | readback D1 never appeared in the second tab |
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | B6 | N's header band holds [`HT_OCR_DIR`] and lacks `project`; the row band holds both; D1 has fewer rows than D0 | C5a: in `tab_bar_placement`, the `Native` arm answers as `Tabs` does | N's header holds `project` |
#[test]
fn native_keeps_the_tab_bar_row_under_cage() {
    const SMOKE: &str = "native_keeps_the_tab_bar_row_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    make_dir(SMOKE, smoke_home(&cwd), HT_DIR);

    let rt: &Path = &runtime_dir;
    let d0 = rt.join("wolf_gnome_hn_d0.txt");
    let d1 = rt.join("wolf_gnome_hn_d1.txt");
    let shot = rt.join("wolf_gnome_hn_n.png");
    for path in [&d0, &d1, &shot] {
        clear_evidence(path, SMOKE);
    }

    let body = with_nvim_pid_helper(&format!("{EXTRAS_HELPERS}\n{HT_HELPERS}\n{HN_BODY}"));
    let status = pointer_command(&body, &cwd, &runtime_dir, &shot)
        .env(
            "WOLF_CONFIG_DIR",
            titlebar_pinned_config_dir(&cwd, GTK_TITLEBAR_NATIVE),
        )
        .env("CMD_FIFO", rt.join(POINTER_CMD_FIFO_NAME))
        .env("ACK_FILE", rt.join(POINTER_ACK_NAME))
        .env("HELPER_ERR", pointer_err_path(rt))
        .env("READY_FILE", pointer_ready_path(rt))
        .env("HT_DIR", HT_DIR)
        .env("HT_D0", &d0)
        .env("HT_D1", &d1)
        .env("HT_N", &shot)
        .status()
        .expect("failed to spawn `cage` for the headless native-titlebar scenario");
    ht_assert_completed(SMOKE, status, rt, &shot);
    assert_png_present(SMOKE, &shot);

    let capture = decode_capture(&shot, SMOKE, "capture N");
    let shade = ht_shade_row(&capture, SMOKE, "N");
    let header_png = rt.join("wolf_gnome_hn_header.png");
    let header = ocr_band_to_lowercase(&shot, ht_header_band(shade), &header_png, SMOKE);
    assert!(
        header.contains(HT_OCR_DIR) && !header.contains(HT_OCR_PROJECT),
        "{SMOKE}: B6: N's header band, two tabs under `native` with the second selected, must \
         hold the window title ~/{HT_DIR} and no {HT_OCR_PROJECT:?}. {HT_OCR_PROJECT:?} there is \
         the first tab's title: the tab bar moved into the header under `native`. No \
         {HT_OCR_DIR:?} means the title is not drawn. Crop kept at {}.\nHeader OCR:\n{header}",
        header_png.display()
    );
    let row_png = rt.join("wolf_gnome_hn_row.png");
    let row = ocr_band_to_lowercase(&shot, tab_bar_band(), &row_png, SMOKE);
    assert!(
        row.contains(HT_OCR_PROJECT) && row.contains(HT_OCR_DIR),
        "{SMOKE}: B6: N's tab bar row band, from {OVERLAY_ORIGIN_Y_EXPECTED} down \
         {TAB_BAR_HEIGHT_EXPECTED} px, must hold both tab titles: the row below the header is \
         not shown under `native`, or does not carry the tabs. Crop kept at {}.\nRow OCR:\n{row}",
        row_png.display()
    );
    let (_, rows0) = ht_dims(&d0, SMOKE);
    let (_, rows1) = ht_dims(&d1, SMOKE);
    assert!(
        rows1 < rows0,
        "{SMOKE}: B6: the second tab's pane has {rows1} rows with two tabs open, the first \
         tab's had {rows0} alone. Under `native` the tab bar's row takes height from the pane, \
         so it must lose a row: the row is not shown."
    );
}

/// Each pinned config file and the shared one parse to their styles with the smoke font kept,
/// so a pin can neither be misspelled nor fail the section back to the defaults' font.
///
/// Pure: no compositor and no cage slot. It removes the temp cwd's parent it made.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | B7 | each file keeps [`SMOKE_FONT_FAMILY`] | C6a: `#[serde(rename = "classic")]` on `GtkTitlebarStyle::Native` | the `native` pin's font family is `iA Writer Mono S` |
/// | B8 | each file parses to its style, the shared one to the default | C6b: `DEFAULT_GTK_TITLEBAR_STYLE` → `GtkTitlebarStyle::Native` | the shared seeded config parses to `Native` |
#[test]
fn titlebar_pins_parse_to_their_styles() {
    const GUARD: &str = "titlebar_pins_parse_to_their_styles";
    let cwd = make_temp_cwd();
    for (dir, want, what) in [
        (
            smoke_config_dir(&cwd),
            wolf_core::GtkTitlebarStyle::Tabs,
            "the shared seeded config",
        ),
        (
            titlebar_pinned_config_dir(&cwd, GTK_TITLEBAR_TABS),
            wolf_core::GtkTitlebarStyle::Tabs,
            "the `tabs` pin",
        ),
        (
            titlebar_pinned_config_dir(&cwd, GTK_TITLEBAR_NATIVE),
            wolf_core::GtkTitlebarStyle::Native,
            "the `native` pin",
        ),
    ] {
        let config = wolf_core::load_or_create(&dir);
        assert_eq!(
            config.font_family,
            SMOKE_FONT_FAMILY,
            "{GUARD}: {what} in {} fell back to the built-in defaults: its YAML failed to parse, \
             so every smoke reading it would draw in `iA Writer Mono S`",
            dir.display()
        );
        assert_eq!(
            config.gtk_titlebar_style,
            want,
            "{GUARD}: {what} in {} parses to the wrong style",
            dir.display()
        );
    }
    let parent = smoke_home(&cwd).to_path_buf();
    std::fs::remove_dir_all(&parent)
        .unwrap_or_else(|e| panic!("{GUARD}: cannot remove {}: {e}", parent.display()));
}
