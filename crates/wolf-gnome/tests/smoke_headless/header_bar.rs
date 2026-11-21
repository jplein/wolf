//! The header bar's start ends at the sidebar's Files/Search toggle group, as the macOS toolbar
//! does, and Ctrl+Shift+O opens the file picker with no header-bar button to reach it.

use super::sidebar_toggles::{tg_header_band, tg_search_x, TG_BAR_SPACING_PX};
use super::titlebar::{TB_HEADER_BUTTON_CLEARANCE_PX, TB_MIN_EXACT_SHARE, TB_TITLE_HALF_WIDTH_PX};
use super::*;

/// The one file in the smoke's working directory, which the picker opens. The smoke OCRs
/// nothing; the name is registered in `RELAUNCH_FIXTURE_NAMES` for the generic loop's sake.
pub(crate) const HB_TARGET_FILE: &str = "hbgoal.md";

/// The whole content of [`HB_TARGET_FILE`]. Nothing reads it.
const HB_TARGET_BODY: &str = "# MAPLE";

/// The query typed into the picker. It matches [`HB_TARGET_FILE`], the only file in the
/// smoke's working directory, so the picker ranks it first.
const HB_QUERY: &str = "hbgoal";

/// The readback and the capture, under `XDG_RUNTIME_DIR` so no sidebar row draws them.
pub(crate) const HB_READ_NAME: &str = "wolf_gnome_hb_read.txt";
const HB_PNG_NAME: &str = "wolf_gnome_hb_launch.png";

/// How many of a column's header-band pixels must differ from the bar's background for the
/// column to count as ink.
///
/// A stray antialiased pixel is not a widget, while an icon's column or the toggle group's
/// trough has far more: the trough spans most of the band's rows.
const HB_INK_MIN_PER_COLUMN: usize = 2;

/// libadwaita 1.9.3's header-bar image button, about 34 px wide: the narrowest start child the
/// band must have room to show after the group.
const HB_IMAGE_BUTTON_WIDTH_PX: f64 = 34.0;

/// The HEADER BAR scenario: one launch, one capture before any key, then the picker.
///
/// The capture comes first, so nothing has navigated and no picker or toast is up. Then
/// Ctrl+Shift+O, [`HB_QUERY`] and Enter open [`HB_TARGET_FILE`], and Neovim writes its
/// buffer's name. The Escape before the Ex command matters on the red path only, where the
/// chord and the query reached Neovim as keys and may have left it in insert mode.
///
/// No `GDK_DEBUG`: neither path can reach a `gtk::FileDialog`. Exit codes are on
/// [`header_start_holds_nav_and_toggles_alone_under_cage`].
const HB_SCENARIO: &str = r#"
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
    echo "wolf-gnome header bar smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome header bar smoke"

# The header bar as launched, before any key.
sleep "$SETTLE"
grim "$HB_PNG"

# The picker's chord, the query and Enter.
wtype -M ctrl -M shift -k o -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$HB_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$SETTLE"

# Normal mode, even where the query reached Neovim as keys.
wtype -k Escape
wtype -- ":call writefile([expand('%:t')], '$HB_READ')"
wtype -k Return
wait_file "$HB_READ" || exit 12

exit 0
"#;

/// The most common colour in `rect` and its share of the rect.
fn hb_dominant(capture: &Capture, rect: SampleRect, what: &str) -> (Rgb8, f64) {
    let mut counts: std::collections::BTreeMap<Rgb8, usize> = std::collections::BTreeMap::new();
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            *counts.entry(capture.pixel(x, y)).or_default() += 1;
        }
    }
    let (colour, count) = counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .unwrap_or(((0, 0, 0), 0));
    (colour, band_fraction(count, rect.pixels(), what))
}

/// How many of column `x`'s pixels in `band`'s rows are not exactly `bg`.
fn hb_column_ink(capture: &Capture, x: usize, band: SampleRect, bg: Rgb8) -> usize {
    (band.y0..band.y1)
        .filter(|&y| capture.pixel(x, y) != bg)
        .count()
}

/// **The header bar's start holds the Back/Forward pair and the sidebar's toggle group and
/// nothing after them, and Ctrl+Shift+O still opens the file picker.**
///
/// One keyboard-only launch: no `wolf-pointer-inject`, so no pointer is on the seat and no
/// hover look or tooltip reaches the capture.
///
/// ## What it reads
///
/// The launch capture's header band, [`tg_header_band`], column by column against the bar's
/// own background. That background is the dominant colour of the span the titlebar smokes read
/// as one flat fill: from [`TB_HEADER_BUTTON_CLEARANCE_PX`] to the title band, over the same
/// rows. A column is ink when at least [`HB_INK_MIN_PER_COLUMN`] of its rows differ from it.
///
/// The run of ink holding the Search toggle's centre, [`tg_search_x`], is the group's trough,
/// and nothing right of it may be ink. Then the readback: Neovim's buffer name after
/// Ctrl+Shift+O, [`HB_QUERY`] and Enter.
///
/// Its premises are libadwaita 1.9.3's, and the diagnostic line every run prints records what
/// they came to. The Search centre lies inside the group, which the toggle-group click smoke
/// relies on. The trough differs from the bar in every column it spans, so the group is one
/// run, and [`TG_BAR_SPACING_PX`] of bare bar separates start children, so a child after the
/// group is a run of its own.
///
/// ## Assertions
///
/// | # | assertion | kind |
/// | --- | --- | --- |
/// | A0 | the scenario exits 0 | precondition |
/// | A1 | the span past the start group is at least [`TB_MIN_EXACT_SHARE`] one colour | precondition |
/// | A2 | the Search toggle's centre column is ink | discriminator |
/// | A3 | no column right of the group's run is ink, with room in the band for a button there | discriminator |
/// | A4 | the readback is [`HB_TARGET_FILE`] | discriminator |
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | the app died before it was ready for input |
/// | `12` | the readback never appeared |
/// | `40` | the launch never became ready (the shared convention) |
///
/// ## Falsifiers
///
/// | ID | edit | expected red |
/// | --- | --- | --- |
/// | H1 | in `window::build`, pack a `document-open-symbolic` button right after the toggle group | A3: ink past the group's run, near x 171..187 |
/// | H2 | delete `header.pack_start(&sidebar.toggles().widget());` | A2: the Search column is bare bar |
/// | H3 | delete `header.pack_start(&nav_box);` | A2: the group sits about 74 px left, off the Search column |
/// | H4 | `set_empty_state`'s `go_to_file` block calls `action.set_enabled(false)` | A4: the readback is empty |
#[test]
fn header_start_holds_nav_and_toggles_alone_under_cage() {
    const SMOKE: &str = "header_start_holds_nav_and_toggles_alone_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target = cwd.join(HB_TARGET_FILE);
    std::fs::write(&target, format!("{HB_TARGET_BODY}\n")).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to write the picker target {}: {e}",
            target.display()
        )
    });

    let read = runtime_dir.join(HB_READ_NAME);
    let png = runtime_dir.join(HB_PNG_NAME);
    for path in [&read, &png] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(HB_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("HB_QUERY", HB_QUERY)
        .env("HB_READ", &read)
        .env("HB_PNG", &png)
        .status()
        .expect("failed to spawn `cage` for the headless header bar scenario");

    // --- A0: the scenario ran to completion --------------------------------------
    assert!(
        status.success(),
        "{SMOKE}: the cage scenario exited unsuccessfully ({status}). Exit 11: the app died \
         before it was ready for input. Exit 12: the readback never appeared, so the typed \
         `:call writefile` never reached Neovim, whatever the picker did. Exit 40: the launch \
         never became ready; its `wolf-ready:` stderr lines name the condition. Any other status \
         means wtype or grim failed. Capture expected at {}.",
        png.display()
    );

    assert_png_present(SMOKE, &png);
    let capture = decode_capture(&png, SMOKE, "the launch capture");
    let band = tg_header_band();

    let width = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let span = SampleRect {
        x0: pixel_index(
            TB_HEADER_BUTTON_CLEARANCE_PX,
            "the header bar's span's first column",
        ),
        x1: pixel_index(
            width / 2.0 - TB_TITLE_HALF_WIDTH_PX,
            "the header bar's span's last column",
        ),
        y0: band.y0,
        y1: band.y1,
    };
    let (bg, share) = hb_dominant(&capture, span, "the header bar's span before the title");

    let search = pixel_index(tg_search_x(), "the Search toggle's centre column");
    let room = pixel_index(
        tg_search_x() + TG_BAR_SPACING_PX + HB_IMAGE_BUTTON_WIDTH_PX,
        "the end of one image button after the Search column",
    );
    let ink = |x: usize| hb_column_ink(&capture, x, band, bg);
    let is_ink = |x: usize| ink(x) >= HB_INK_MIN_PER_COLUMN;
    let search_ink = if search < band.x1 { ink(search) } else { 0 };

    // The group's run: the maximal run of ink columns holding the Search column.
    let run = (search < band.x1 && is_ink(search)).then(|| {
        let mut start = search;
        while start > band.x0 && is_ink(start - 1) {
            start -= 1;
        }
        let mut end = search;
        while end + 1 < band.x1 && is_ink(end + 1) {
            end += 1;
        }
        (start, end)
    });
    let past: Vec<usize> = run
        .map(|(_, end)| (end + 1..band.x1).filter(|&x| is_ink(x)).collect())
        .unwrap_or_default();
    let rightmost = (band.x0..band.x1).rev().find(|&x| is_ink(x));

    eprintln!(
        "{SMOKE}: bar background {bg:?} at {share:.3} of x {}..{}; Search column {search} holds \
         {search_ink} ink px; group run {run:?}; rightmost ink column {rightmost:?} of band x \
         {}..{}, rows {}..{}",
        span.x0, span.x1, band.x0, band.x1, band.y0, band.y1
    );

    // --- A1: the bar's background -------------------------------------------------
    assert!(
        share >= TB_MIN_EXACT_SHARE,
        "{SMOKE}: the header bar's span x {}..{}, past the start group and before the title, is \
         only {share:.3} its most common colour {bg:?}, under TB_MIN_EXACT_SHARE = \
         {TB_MIN_EXACT_SHARE}. The bar is not one flat fill past the start group, so A2 and A3 \
         would read against the wrong colour and prove nothing. A start child reaching past \
         TB_HEADER_BUTTON_CLEARANCE_PX, a title wider than TB_TITLE_HALF_WIDTH_PX or an overlay \
         over the bar would each do this. Capture kept at {}.",
        span.x0,
        span.x1,
        png.display()
    );

    // --- A2: the group sits after Back/Forward ------------------------------------
    assert!(
        search_ink >= HB_INK_MIN_PER_COLUMN,
        "{SMOKE}: the Search toggle's centre column x = {search} holds {search_ink} pixels other \
         than the bar's {bg:?} over the header band's rows {}..{}, under HB_INK_MIN_PER_COLUMN = \
         {HB_INK_MIN_PER_COLUMN}. No group is drawn there, so the header bar's start does not \
         hold Back/Forward followed by the toggle group: either the group is not packed, or \
         `nav_box` is not packed before it and the group sits about 74 px further left. The \
         rightmost ink column is {rightmost:?}. Capture kept at {}.",
        band.y0,
        band.y1,
        png.display()
    );

    // --- A3: nothing after the group -----------------------------------------------
    assert!(
        room <= band.x1,
        "{SMOKE}: the header band ends at x = {}, short of x = {room}, the Search column plus \
         TG_BAR_SPACING_PX and one HB_IMAGE_BUTTON_WIDTH_PX image button. A child packed after \
         the group would then lie outside the band, and A3 could not see it. Widen the band \
         through TB_HEADER_BUTTON_CLEARANCE_PX; do not drop this check.",
        band.x1
    );
    let (run_start, run_end) =
        run.expect("A2 established that the Search column is ink, so the run exists");
    if let (Some(&first), Some(&last)) = (past.first(), past.last()) {
        panic!(
            "{SMOKE}: past the toggle group's run of ink, x {run_start}..={run_end}, the header \
             band holds ink from x = {first} ({} px) to x = {last} ({} px), {} columns in all. \
             Another child is packed at the header bar's start after the toggle group, which the \
             macOS toolbar does not have: the bar between the group and the title must be bare \
             {bg:?}. If the run ends well short of the group's estimated end near x = 156, the \
             trough is not one run and this ink is the group's own right half: re-derive the run \
             before touching the band. Capture kept at {}.",
            ink(first),
            ink(last),
            past.len(),
            png.display()
        );
    }

    // --- A4: the chord still opens the picker --------------------------------------
    let opened = read_readback(&read, SMOKE).trim().to_string();
    assert_eq!(
        opened, HB_TARGET_FILE,
        "{SMOKE}: after Ctrl+Shift+O, the query {HB_QUERY:?} and Enter, Neovim's buffer is \
         {opened:?}, not {HB_TARGET_FILE:?}. Empty: no file was opened, so the chord reached \
         Neovim as `<C-O>` and the query as normal-mode keys; `win.go-to-file` is disabled or \
         unbound from Ctrl+Shift+O. Another name: the picker opened, but the query did not rank \
         the fixture first or Enter did not open the selection."
    );
}
