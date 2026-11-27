//! The search panel's case, regex and all-files toggles: they sit directly below the field and
//! above the results, and each decides what the next Enter asks rg for.
//!
//! Every capture is taken after Escape has handed focus back to the editor, so neither the
//! field's caret nor its focus ring is in the band. Where the toggles sit is read from the
//! sidebar band's raw pixels, compared between captures of one run; what a search found and what
//! the status line says are read through OCR of the same band ([`projsearch_sidebar_band`]).
//! One smoke clicks nothing: a tab saved on its Search page restores the page, the query and the
//! toggles, and runs the search.
//!
//! ## Click targets
//!
//! The toggles' centres are estimates from libadwaita 1.9.3's stylesheet and measured label
//! widths, not read off the widgets, as the header bar's toggle-group smokes' are. That is safe
//! because a click that misses a toggle changes no option: the capture that should show the
//! change does not, and the smoke fails there.
//!
//! ## Fixtures
//!
//! Each smoke writes only its own files, plus the sidebar fillers for page density. Every word a
//! smoke reads as a result sits on its file's one line, after the query, and in no file name: the
//! panel's file row draws the file's name, so a word there would be read whatever was found.
//! [`assert_search_toggle_fixtures`] checks that, and what each query matches, before a launch.

use wolf_core::app_state::{Sidebar, SidebarMode, Tab, TabContent};

use super::extras::{canonical_project, plant_window_state, planted_window};
use super::sidebar::projsearch_sidebar_band;
use super::*;

/// The case smoke's query, lower case, so a case-sensitive search tells
/// [`ST_CASE_LOWER_LINE`] from [`ST_CASE_UPPER_LINE`].
pub(crate) const ST_CASE_QUERY: &str = "zzquince";

/// The file holding [`ST_CASE_QUERY`] in its own case.
pub(crate) const ST_CASE_LOWER_FILE: &str = "stcaselow.txt";

/// The whole of [`ST_CASE_LOWER_FILE`]: the query as typed, then [`ST_OCR_CASE_LOWER`].
pub(crate) const ST_CASE_LOWER_LINE: &str = "zzquince zzlemur";

/// Lowercased word only [`ST_CASE_LOWER_LINE`]'s result row draws: either search finds it.
pub(crate) const ST_OCR_CASE_LOWER: &str = "zzlemur";

/// The file holding [`ST_CASE_QUERY`] in upper case.
pub(crate) const ST_CASE_UPPER_FILE: &str = "stcasehigh.txt";

/// The whole of [`ST_CASE_UPPER_FILE`]: the query in upper case, then [`ST_OCR_CASE_UPPER`].
pub(crate) const ST_CASE_UPPER_LINE: &str = "ZZQUINCE zzbison";

/// Lowercased word only [`ST_CASE_UPPER_LINE`]'s result row draws: only a case-insensitive
/// search finds it.
pub(crate) const ST_OCR_CASE_UPPER: &str = "zzbison";

/// The regex smoke's query: as a pattern it matches [`ST_REGEX_MATCH`], and as a literal it
/// matches nothing in the smoke's working directory.
pub(crate) const ST_REGEX_QUERY: &str = "fo+bar";

/// The text on [`ST_REGEX_LINE`] that [`ST_REGEX_QUERY`] matches as a pattern.
pub(crate) const ST_REGEX_MATCH: &str = "foooobar";

/// The file holding [`ST_REGEX_LINE`].
pub(crate) const ST_REGEX_FILE: &str = "stpattern.txt";

/// The whole of [`ST_REGEX_FILE`]: [`ST_REGEX_MATCH`], then [`ST_OCR_REGEX`].
pub(crate) const ST_REGEX_LINE: &str = "foooobar zzheron";

/// Lowercased word only [`ST_REGEX_LINE`]'s result row draws: only a regex search finds it.
pub(crate) const ST_OCR_REGEX: &str = "zzheron";

/// The invalid-regex smoke's query: an unclosed group, which rg refuses with exit status 2 as a
/// pattern and searches for as an ordinary literal.
pub(crate) const ST_PAREN_QUERY: &str = "foo(bar";

/// The file holding [`ST_PAREN_LINE`].
pub(crate) const ST_PAREN_FILE: &str = "stparen.txt";

/// The whole of [`ST_PAREN_FILE`]: [`ST_PAREN_QUERY`] verbatim, then [`ST_OCR_PAREN`].
pub(crate) const ST_PAREN_LINE: &str = "foo(bar zzwalrus";

/// Lowercased word only [`ST_PAREN_LINE`]'s result row draws: only a literal search finds it.
pub(crate) const ST_OCR_PAREN: &str = "zzwalrus";

/// The all-files smoke's query, which both of its files hold, so only the search's scope decides
/// which of them is listed.
pub(crate) const ST_ALL_QUERY: &str = "zzkumquat";

/// The all-files smoke's Markdown file, which a search reaches with `All` off or on.
pub(crate) const ST_ALL_MD_FILE: &str = "stscope.md";

/// The whole of [`ST_ALL_MD_FILE`]: [`ST_ALL_QUERY`], then [`ST_OCR_ALL_MD`].
pub(crate) const ST_ALL_MD_LINE: &str = "zzkumquat zzotter";

/// Lowercased word only [`ST_ALL_MD_LINE`]'s result row draws: either scope finds it.
pub(crate) const ST_OCR_ALL_MD: &str = "zzotter";

/// The all-files smoke's other file, which only a search over every file reaches.
pub(crate) const ST_ALL_TXT_FILE: &str = "stscope.txt";

/// The whole of [`ST_ALL_TXT_FILE`]: [`ST_ALL_QUERY`], then [`ST_OCR_ALL_TXT`].
pub(crate) const ST_ALL_TXT_LINE: &str = "zzkumquat zzbadger";

/// Lowercased word only [`ST_ALL_TXT_LINE`]'s result row draws: only a search over every file
/// finds it.
pub(crate) const ST_OCR_ALL_TXT: &str = "zzbadger";

/// Lowercased word of the status line a toggle changed since the live search ran draws,
/// `Options changed: press Enter`.
pub(crate) const ST_OCR_STALE: &str = "changed";

/// Lowercased status line of a finished search that found nothing, `No matches`.
pub(crate) const ST_OCR_NO_MATCHES: &str = "no matches";

/// Lowercased word of the status line for a session carrying an error, here
/// `Search failed: rg exited with code 2`, which the sidebar ellipsizes after `with c`. The word
/// is `exited` rather than `failed` because `--psm 11` reads this crop's `failed:` as `faile` on
/// every run and reads `exited` correctly.
pub(crate) const ST_OCR_RG_EXITED: &str = "exited";

/// The search panel's margin and the spacing between its children, `search_panel`'s
/// `SEARCH_PANEL_SPACING`.
const ST_PANEL_SPACING_PX: f64 = 6.0;

/// libadwaita 1.9.3's `entry` min-height, which is the field's height.
const ST_ENTRY_HEIGHT_PX: f64 = 34.0;

/// libadwaita 1.9.3's `button`: a 24 px min-height and 5 px of padding above and below.
const ST_TOGGLE_HEIGHT_PX: f64 = 34.0;

/// libadwaita 1.9.3's padding on each side of a `button.text-button`, the class GTK gives a
/// button built with a label.
const ST_TOGGLE_PADDING_PX: f64 = 17.0;

/// The `Aa` label's width: bold Adwaita Sans 11, which the pinned fontconfig resolves to DejaVu
/// Sans Bold, measured at 21 px with the devShell's `pango-view`.
const ST_CASE_LABEL_PX: f64 = 21.0;

/// The `.*` label's content width. It measures 14 px, under libadwaita's 16 px `button`
/// min-width, so the min-width wins.
const ST_REGEX_LABEL_PX: f64 = 16.0;

/// The `All` label's width in bold Adwaita Sans 11, which resolves to DejaVu Sans Bold, measured
/// at 21 px with the devShell's `pango-view` at 96 dpi, as [`ST_CASE_LABEL_PX`] was.
const ST_ALL_LABEL_PX: f64 = 21.0;

/// How far right of the panel's start margin the case toggle's leftmost changed column may lie
/// and still count as left-aligned. A row centred or aligned to the end starts some 70 px or more
/// further right.
const ST_START_TOLERANCE_PX: f64 = 6.0;

/// The fewest sidebar-band pixels two captures must differ by to count as a change.
///
/// A toggle's checked background alone covers well over a thousand pixels and one typed word
/// several hundred, while a stray antialiased pixel or two does not reach this.
const ST_MIN_CHANGED_PIXELS: usize = 40;

/// Captures of the case smoke, under `XDG_RUNTIME_DIR` so no tree row draws them. The scenario
/// appends the capture's index and `.png`.
const ST_CASE_SHOT_PREFIX: &str = "wolf_gnome_st_case_";

/// Captures of the regex smoke; see [`ST_CASE_SHOT_PREFIX`].
const ST_REGEX_SHOT_PREFIX: &str = "wolf_gnome_st_regex_";

/// Captures of the invalid-regex smoke; see [`ST_CASE_SHOT_PREFIX`].
const ST_PAREN_SHOT_PREFIX: &str = "wolf_gnome_st_paren_";

/// Captures of the all-files smoke; see [`ST_CASE_SHOT_PREFIX`].
const ST_ALL_SHOT_PREFIX: &str = "wolf_gnome_st_all_";

/// The row every toggle is clicked on: the header bar, the panel's top margin, the field, the
/// spacing under it and half a toggle, which is 110.
///
/// Every term is an estimate rather than a measurement. The point sits 17 px inside a 34 px
/// row, so it survives an error of that size in their sum.
pub(crate) fn st_toggle_y() -> f64 {
    OVERLAY_ORIGIN_Y_EXPECTED
        + ST_PANEL_SPACING_PX
        + ST_ENTRY_HEIGHT_PX
        + ST_PANEL_SPACING_PX
        + ST_TOGGLE_HEIGHT_PX / 2.0
}

/// A text button's width over a label of `label_px`.
fn st_toggle_width(label_px: f64) -> f64 {
    label_px + 2.0 * ST_TOGGLE_PADDING_PX
}

/// Centre of the `Aa` toggle: the panel's start margin and half the toggle, about 34, inside a
/// toggle estimated at x 6 to 61.
fn st_case_x() -> f64 {
    ST_PANEL_SPACING_PX + st_toggle_width(ST_CASE_LABEL_PX) / 2.0
}

/// Centre of the `.*` toggle: the margin, the `Aa` toggle, the spacing and half the toggle,
/// about 92, inside a toggle estimated at x 67 to 117.
fn st_regex_x() -> f64 {
    ST_PANEL_SPACING_PX
        + st_toggle_width(ST_CASE_LABEL_PX)
        + ST_PANEL_SPACING_PX
        + st_toggle_width(ST_REGEX_LABEL_PX) / 2.0
}

/// Centre of the `All` toggle: the margin, the `Aa` toggle, the spacing, the `.*` toggle, the
/// spacing and half the toggle, about 150, inside a toggle estimated at x 123 to 178.
pub(crate) fn st_all_x() -> f64 {
    ST_PANEL_SPACING_PX
        + st_toggle_width(ST_CASE_LABEL_PX)
        + ST_PANEL_SPACING_PX
        + st_toggle_width(ST_REGEX_LABEL_PX)
        + ST_PANEL_SPACING_PX
        + st_toggle_width(ST_ALL_LABEL_PX) / 2.0
}

/// Shell helpers every body below starts with, appended to [`POINTER_PREAMBLE`], which has
/// launched the app and parked the pointer at the probe point over the pane area.
///
/// `st_click` ends with the pointer back over the pane, so no hover look or tooltip reaches a
/// capture. `st_field` shows the Search page and focuses its field from wherever focus is. The
/// sidebar opens on its Files page, so a body runs `st_field` before its first `st_click`, which
/// would otherwise land on the file tree. `st_enter` presses Enter in the field and hands focus
/// back to the editor before the capture.
const ST_HELPERS: &str = r#"
st_click() {
    hsend motion "$1" "$ST_TOGGLE_Y"
    hsend press "$BTN_LEFT"
    hsend release "$BTN_LEFT"
    hsend motion "$PROBE_X" "$PROBE_Y"
    sleep "$SETTLE"
}

st_field() {
    wtype -M ctrl -M shift -k f -m shift -m ctrl
    sleep "$SETTLE"
}

st_enter() {
    wtype -k Return
    sleep "$SETTLE"
    wtype -k Escape
    sleep "$SETTLE"
}
"#;

/// Body of [`case_toggle_sits_under_the_field_and_narrows_the_match_under_cage`]: six captures,
/// `${ST_SHOT}0.png` to `${ST_SHOT}5.png`.
const ST_CASE_STEPS: &str = r#"
# 0: the Search page, its field empty.
st_field
wtype -k Escape
sleep "$SETTLE"
grim "${ST_SHOT}0.png"

# 1: the query typed, nothing run.
st_field
wtype -- "$ST_QUERY"
sleep "$SETTLE"
wtype -k Escape
sleep "$SETTLE"
grim "${ST_SHOT}1.png"

# 2: Aa on, with nothing run yet.
st_click "$ST_CASE_X"
grim "${ST_SHOT}2.png"

# 3: Aa off again and All on, since the fixtures are .txt, with nothing run yet.
st_click "$ST_CASE_X"
st_click "$ST_ALL_X"
grim "${ST_SHOT}3.png"

# 4: the query runs.
st_field
st_enter
grim "${ST_SHOT}4.png"

# 5: Aa on, then Enter on the unchanged query.
st_click "$ST_CASE_X"
st_field
st_enter
grim "${ST_SHOT}5.png"

hsend quit
exit 0
"#;

/// Body of [`regex_toggle_switches_between_pattern_and_literal_under_cage`]: three captures,
/// `${ST_SHOT}0.png` to `${ST_SHOT}2.png`.
const ST_REGEX_STEPS: &str = r#"
# 0: All on once the Search page shows, since the fixture is a .txt, then the query runs with
# the other toggles as the panel opens.
st_field
st_click "$ST_ALL_X"
st_field
wtype -- "$ST_QUERY"
sleep "$SETTLE"
st_enter
grim "${ST_SHOT}0.png"

# 1: .* off, with focus in the editor.
st_click "$ST_REGEX_X"
grim "${ST_SHOT}1.png"

# 2: Enter on the unchanged query.
st_field
st_enter
grim "${ST_SHOT}2.png"

hsend quit
exit 0
"#;

/// Body of [`invalid_regex_fails_in_the_status_line_until_regex_is_off_under_cage`]: two
/// captures, `${ST_SHOT}0.png` and `${ST_SHOT}1.png`.
const ST_PAREN_STEPS: &str = r#"
# 0: All on once the Search page shows, since the fixture is a .txt, then the query runs with
# the other toggles as the panel opens.
st_field
st_click "$ST_ALL_X"
st_field
wtype -- "$ST_QUERY"
sleep "$SETTLE"
st_enter
grim "${ST_SHOT}0.png"

# 1: .* off while the field holds focus, then Enter there.
st_field
st_click "$ST_REGEX_X"
st_enter
grim "${ST_SHOT}1.png"

hsend quit
exit 0
"#;

/// Body of [`all_files_toggle_widens_a_markdown_only_search_under_cage`]: three captures,
/// `${ST_SHOT}0.png` to `${ST_SHOT}2.png`.
const ST_ALL_STEPS: &str = r#"
# 0: the query runs with the toggles as the panel opens, All off.
st_field
wtype -- "$ST_QUERY"
sleep "$SETTLE"
st_enter
grim "${ST_SHOT}0.png"

# 1: All on, with focus in the editor.
st_click "$ST_ALL_X"
grim "${ST_SHOT}1.png"

# 2: Enter on the unchanged query.
st_field
st_enter
grim "${ST_SHOT}2.png"

hsend quit
exit 0
"#;

/// One smoke's run: its name, its steps, its query and fixtures, and its captures.
struct Leg {
    smoke: &'static str,
    steps: &'static str,
    query: &'static str,
    fixtures: &'static [(&'static str, &'static str)],
    shot_prefix: &'static str,
    shots: usize,
}

/// One capture: the full PNG and its sidebar-band crop.
struct Shot {
    png: PathBuf,
    band_png: PathBuf,
}

/// Asserts what every smoke here assumes of its fixtures, before any of them launches.
///
/// Each result word is on one fixture line, after that line's query, and in no file name or
/// query; no status term is in any fixture or query; and each query matches what its smoke
/// says it matches: [`ST_CASE_QUERY`] the lower line in its own case and the upper line only
/// case-insensitively, [`ST_REGEX_QUERY`] [`ST_REGEX_MATCH`] only as a pattern,
/// [`ST_PAREN_QUERY`] its line only as a literal, being an invalid pattern, and
/// [`ST_ALL_QUERY`] both of its lines.
///
/// It also asserts the premise the all-files smoke's first capture rests on: [`ST_ALL_MD_FILE`]
/// ends in one of core's Markdown-only globs, `*.md` and `*.markdown`, and [`ST_ALL_TXT_FILE`]
/// in neither, so only the scope tells the two apart.
fn assert_search_toggle_fixtures(smoke: &str) {
    let lines = [
        (ST_CASE_LOWER_FILE, ST_CASE_LOWER_LINE, ST_OCR_CASE_LOWER),
        (ST_CASE_UPPER_FILE, ST_CASE_UPPER_LINE, ST_OCR_CASE_UPPER),
        (ST_REGEX_FILE, ST_REGEX_LINE, ST_OCR_REGEX),
        (ST_PAREN_FILE, ST_PAREN_LINE, ST_OCR_PAREN),
        (ST_ALL_MD_FILE, ST_ALL_MD_LINE, ST_OCR_ALL_MD),
        (ST_ALL_TXT_FILE, ST_ALL_TXT_LINE, ST_OCR_ALL_TXT),
    ];
    let queries = [ST_CASE_QUERY, ST_REGEX_QUERY, ST_PAREN_QUERY, ST_ALL_QUERY];
    let statuses = [ST_OCR_STALE, ST_OCR_NO_MATCHES, ST_OCR_RG_EXITED];

    for (file, line, word) in lines {
        assert!(
            line.ends_with(&format!(" {word}")),
            "{smoke}: {file}'s line {line:?} does not end with its result word {word:?}. The \
             word must sit on the matched line, after the query, or no result row draws it and \
             the smoke reads nothing it can attribute to a search."
        );
        for (other_file, other_line, _) in lines {
            if other_file != file {
                assert!(
                    !other_line.to_lowercase().contains(word),
                    "{smoke}: {other_file}'s line {other_line:?} contains {file}'s result word \
                     {word:?}, so finding {other_file} would read as finding {file}."
                );
            }
            assert!(
                !other_file.to_lowercase().contains(word),
                "{smoke}: the file name {other_file:?} contains the result word {word:?}. The \
                 panel draws a result's file name, so the word would be read whatever the \
                 search matched."
            );
        }
        for query in queries {
            assert!(
                !word.contains(&query.to_lowercase()) && !query.to_lowercase().contains(word),
                "{smoke}: the result word {word:?} and the query {query:?} overlap. The query \
                 is drawn in the field, so the word would be read with no search run."
            );
            assert!(
                !file.to_lowercase().contains(&query.to_lowercase()),
                "{smoke}: the file name {file:?} contains the query {query:?}. rg does not \
                 search names, but a failing capture would be ambiguous about which row matched."
            );
        }
        for status in statuses {
            assert!(
                !line.to_lowercase().contains(status) && !file.to_lowercase().contains(status),
                "{smoke}: {file} or its line {line:?} contains the status term {status:?}, so a \
                 result row would read as that status line."
            );
        }
    }
    for status in statuses {
        for query in queries {
            assert!(
                !query.to_lowercase().contains(status),
                "{smoke}: the query {query:?} contains the status term {status:?}, and the field \
                 draws the query in every capture."
            );
        }
    }

    assert!(
        ST_CASE_LOWER_LINE.contains(ST_CASE_QUERY)
            && !ST_CASE_UPPER_LINE.contains(ST_CASE_QUERY)
            && ST_CASE_UPPER_LINE.to_lowercase().contains(ST_CASE_QUERY),
        "{smoke}: ST_CASE_QUERY = {ST_CASE_QUERY:?} must occur as typed in {ST_CASE_LOWER_LINE:?} \
         and only case-insensitively in {ST_CASE_UPPER_LINE:?}. Otherwise both searches find the \
         same lines and the case smoke cannot tell `-i` from `-s`."
    );
    let pattern_matches = ST_REGEX_MATCH
        .strip_prefix('f')
        .and_then(|rest| rest.strip_suffix("bar"))
        .is_some_and(|os| !os.is_empty() && os.chars().all(|c| c == 'o'));
    assert!(
        pattern_matches
            && ST_REGEX_QUERY == "fo+bar"
            && ST_REGEX_LINE.contains(ST_REGEX_MATCH)
            && !ST_REGEX_LINE
                .to_lowercase()
                .contains(&ST_REGEX_QUERY.to_lowercase()),
        "{smoke}: ST_REGEX_LINE = {ST_REGEX_LINE:?} must hold ST_REGEX_MATCH = \
         {ST_REGEX_MATCH:?}, an `f`, one or more `o` and `bar`, which ST_REGEX_QUERY = \
         {ST_REGEX_QUERY:?} matches as a pattern, and must not hold the query literally. \
         Otherwise the regex smoke finds the same line whichever way the toggle is set."
    );
    assert!(
        ST_PAREN_QUERY.matches('(').count() > ST_PAREN_QUERY.matches(')').count()
            && ST_PAREN_LINE.contains(ST_PAREN_QUERY),
        "{smoke}: ST_PAREN_QUERY = {ST_PAREN_QUERY:?} must hold an unclosed `(`, which rg refuses \
         as a pattern with exit status 2, and ST_PAREN_LINE = {ST_PAREN_LINE:?} must hold it \
         verbatim, so the literal search finds it."
    );
    assert!(
        ST_ALL_MD_LINE.contains(ST_ALL_QUERY) && ST_ALL_TXT_LINE.contains(ST_ALL_QUERY),
        "{smoke}: ST_ALL_QUERY = {ST_ALL_QUERY:?} must occur in both ST_ALL_MD_LINE = \
         {ST_ALL_MD_LINE:?} and ST_ALL_TXT_LINE = {ST_ALL_TXT_LINE:?}. A line the query does not \
         match is missing under either scope, so the all-files smoke would read the fixture \
         rather than the toggle."
    );
    let extension = |file: &'static str| Path::new(file).extension().and_then(OsStr::to_str);
    let md_extension = extension(ST_ALL_MD_FILE);
    assert!(
        matches!(md_extension, Some("md" | "markdown")),
        "{smoke}: ST_ALL_MD_FILE = {ST_ALL_MD_FILE:?} has extension {md_extension:?}, which must \
         be `md` or `markdown`, core's Markdown-only globs. Any other file goes unsearched with \
         `All` off, so the all-files smoke's capture 0 would find nothing and could not show the \
         panel opening Markdown-only."
    );
    let txt_extension = extension(ST_ALL_TXT_FILE).map(str::to_ascii_lowercase);
    assert!(
        matches!(txt_extension.as_deref(), Some(ext) if ext != "md" && ext != "markdown"),
        "{smoke}: ST_ALL_TXT_FILE = {ST_ALL_TXT_FILE:?} has extension {txt_extension:?}, which \
         must be present and neither `md` nor `markdown`. A Markdown file is searched with `All` \
         off too, so the all-files smoke's capture 0 would hold its word whichever scope the \
         panel opens with, and capture 2 could not show `All` widening the search."
    );
}

/// The paths of `count` captures named `<prefix><index>.png` under `runtime`, each cleared
/// first, with their band crops beside them.
fn st_shots(runtime: &Path, prefix: &str, count: usize, smoke: &str) -> Vec<Shot> {
    (0..count)
        .map(|index| {
            let shot = Shot {
                png: runtime.join(format!("{prefix}{index}.png")),
                band_png: runtime.join(format!("{prefix}{index}_sidebar.png")),
            };
            clear_evidence(&shot.png, smoke);
            clear_evidence(&shot.band_png, smoke);
            shot
        })
        .collect()
}

/// Writes `leg`'s fixtures into a fresh working directory, runs its steps under `cage` and
/// asserts the scenario succeeded with every click target inside the output. Returns its
/// captures, which live in `runtime` and so only as long as it does.
fn run_leg(leg: &Leg, runtime: &CageRuntime<'_>) -> Vec<Shot> {
    assert_search_toggle_fixtures(leg.smoke);

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(leg.smoke, &cwd, leg.fixtures);

    let shots = st_shots(runtime, leg.shot_prefix, leg.shots, leg.smoke);
    let prefix = runtime.join(leg.shot_prefix);
    let last = &shots[leg.shots - 1].png;

    let case_x = pixel_target(st_case_x(), "the case toggle's x");
    let regex_x = pixel_target(st_regex_x(), "the regex toggle's x");
    let all_x = pixel_target(st_all_x(), "the all-files toggle's x");
    let toggle_y = pixel_target(st_toggle_y(), "the toggles' y");
    let case_x_env = case_x.to_string();
    let regex_x_env = regex_x.to_string();
    let all_x_env = all_x.to_string();
    let toggle_y_env = toggle_y.to_string();
    let body = format!("{ST_HELPERS}\n{}", leg.steps);

    let status = run_pointer_scenario(
        &body,
        &cwd,
        runtime,
        last,
        &[
            ("ST_SHOT", prefix.as_os_str()),
            ("ST_QUERY", OsStr::new(leg.query)),
            ("ST_CASE_X", OsStr::new(&case_x_env)),
            ("ST_REGEX_X", OsStr::new(&regex_x_env)),
            ("ST_ALL_X", OsStr::new(&all_x_env)),
            ("ST_TOGGLE_Y", OsStr::new(&toggle_y_env)),
        ],
    );
    assert_pointer_scenario_succeeded(leg.smoke, status, &cwd, last);

    let extents = read_output_extents(&cwd, leg.smoke);
    assert_inside_extents(leg.smoke, "the case toggle", (case_x, toggle_y), extents);
    assert_inside_extents(leg.smoke, "the regex toggle", (regex_x, toggle_y), extents);
    assert_inside_extents(
        leg.smoke,
        "the all-files toggle",
        (all_x, toggle_y),
        extents,
    );
    shots
}

/// One term a capture's sidebar band must hold or lack, and what a wrong reading means.
struct Reading {
    term: &'static str,
    held: bool,
    means: &'static str,
}

/// Asserts that capture `index`'s sidebar band holds or lacks each term as `readings` say.
fn assert_band_reads(smoke: &str, shots: &[Shot], index: usize, step: &str, readings: &[Reading]) {
    let shot = &shots[index];
    assert_png_present(smoke, &shot.png);
    let text = ocr_band_to_lowercase(&shot.png, projsearch_sidebar_band(), &shot.band_png, smoke);
    for reading in readings {
        assert_eq!(
            text.contains(reading.term),
            reading.held,
            "{smoke}: the sidebar band of capture {index}, {step}, {} {:?}. {} Capture kept at \
             {}, crop at {}.\n\nBand OCR was:\n{text}",
            if reading.held { "lacks" } else { "holds" },
            reading.term,
            reading.means,
            shot.png.display(),
            shot.band_png.display()
        );
    }
}

/// The sidebar-band pixels in which two captures differ, and the box around them.
struct BandDiff {
    changed: usize,
    x_min: usize,
    x_max: usize,
    y_min: usize,
    y_max: usize,
}

/// Compares two captures over [`projsearch_sidebar_band`].
fn band_diff(a: &Capture, b: &Capture) -> BandDiff {
    let band = projsearch_sidebar_band();
    let mut diff = BandDiff {
        changed: 0,
        x_min: usize::MAX,
        x_max: 0,
        y_min: usize::MAX,
        y_max: 0,
    };
    for y in band.y0..band.y1 {
        for x in band.x0..band.x1 {
            if a.pixel(x, y) != b.pixel(x, y) {
                diff.changed += 1;
                diff.x_min = diff.x_min.min(x);
                diff.x_max = diff.x_max.max(x);
                diff.y_min = diff.y_min.min(y);
                diff.y_max = diff.y_max.max(y);
            }
        }
    }
    diff
}

/// Asserts that captures `a` and `b` differ in at least [`ST_MIN_CHANGED_PIXELS`] pixels of the
/// sidebar band, and returns where.
fn assert_band_changed(
    smoke: &str,
    shots: &[Shot],
    captures: &[Capture],
    (a, b): (usize, usize),
    causes: &str,
) -> BandDiff {
    let diff = band_diff(&captures[a], &captures[b]);
    assert!(
        diff.changed >= ST_MIN_CHANGED_PIXELS,
        "{smoke}: captures {a} and {b} differ in {} pixels of the sidebar band, fewer than \
         ST_MIN_CHANGED_PIXELS = {ST_MIN_CHANGED_PIXELS}. {causes} Captures kept at {} and {}.",
        diff.changed,
        shots[a].png.display(),
        shots[b].png.display()
    );
    diff
}

/// The case toggle sits directly under the field, left-aligned, above the status line and the
/// results, and turning it on narrows the next search to the query's own case.
///
/// The query is typed and nothing runs; `Aa` is clicked on and off, and `All` on, because the
/// fixtures are `.txt` and the panel opens Markdown-only; the query runs; `Aa` is clicked on and
/// Enter pressed on the unchanged query. Placement comes from four diffs of the sidebar band: the
/// field's (captures 0 and 1), `Aa`'s (1 and 2), `All`'s (1 and 3, `Aa` drawn alike in both) and
/// the status line and rows' (3 and 4, both toggles drawn alike in both).
///
/// ## What each capture reads
///
/// | capture | step | band |
/// | --- | --- | --- |
/// | 0 | Ctrl+Shift+F, Escape | the empty field |
/// | 1 | Ctrl+Shift+F, the query, Escape | the field holds the query |
/// | 2 | click `Aa` | `Aa` on, nothing run |
/// | 3 | click `Aa`, click `All` | `Aa` off, `All` on, nothing run |
/// | 4 | Ctrl+Shift+F, Enter, Escape | both result words |
/// | 5 | click `Aa`, Ctrl+Shift+F, Enter, Escape | [`ST_OCR_CASE_LOWER`] only, no stale status |
///
/// ## Assertions
///
/// 1. Captures 0 and 1 differ: the field's rows.
/// 2. Captures 1 and 2 differ: the click changed `Aa`.
/// 3. `Aa`'s top changed row lies below the field's bottom changed row.
/// 4. Captures 1 and 3 differ, and every changed column lies right of `Aa`'s rightmost changed
///    column: a click at `All`'s estimated centre changed a toggle after `Aa`. The column clause
///    is a precondition against `Aa` drawing differently after on and off, with no falsifier of
///    its own.
/// 5. Captures 3 and 4 differ, and their top changed row, the status line's, lies below both
///    toggles' bottom changed rows.
/// 6. `Aa`'s leftmost changed column lies within [`ST_START_TOLERANCE_PX`] of the panel's start
///    margin.
/// 7. Capture 4 holds both result words: `All` is on, and the panel starts case-insensitive.
/// 8. Capture 5 holds [`ST_OCR_CASE_LOWER`], lacks [`ST_OCR_CASE_UPPER`] and lacks
///    [`ST_OCR_STALE`]: the Enter re-ran the unchanged query with the case toggle on.
///
/// ## Falsifiers
///
/// Every edit is in `crates/wolf-gnome/src/search_panel.rs`.
///
/// | ID | edit | expected red |
/// | --- | --- | --- |
/// | C6a | append the toggle row after the status label | 2: the click lands on the status line |
/// | C6b | prepend the toggle row above the entry | 2: the click lands in the field, which changes nothing in the band |
/// | C6c | toggle row `halign(gtk::Align::End)` | 2: the click lands left of both toggles |
/// | C6d | `search_options_for` sets `case_sensitive: false` | 8 |
/// | C6e | build the case toggle with `.active(true)` | 7 |
/// | C20a | delete `toggle_row.append(&all_toggle)` | 4: the click lands right of the row |
/// | C20b | append `all_toggle` before `regex_toggle` | 7: the click at `st_all_x` turns `.*` off, `All` stays off, and no `.txt` is found |
#[test]
fn case_toggle_sits_under_the_field_and_narrows_the_match_under_cage() {
    const SMOKE: &str = "case_toggle_sits_under_the_field_and_narrows_the_match_under_cage";

    let runtime_dir = require_smoke_env();
    let shots = run_leg(
        &Leg {
            smoke: SMOKE,
            steps: ST_CASE_STEPS,
            query: ST_CASE_QUERY,
            fixtures: &[
                (ST_CASE_LOWER_FILE, ST_CASE_LOWER_LINE),
                (ST_CASE_UPPER_FILE, ST_CASE_UPPER_LINE),
            ],
            shot_prefix: ST_CASE_SHOT_PREFIX,
            shots: 6,
        },
        &runtime_dir,
    );
    let captures: Vec<Capture> = shots
        .iter()
        .enumerate()
        .map(|(index, shot)| decode_capture(&shot.png, SMOKE, &format!("capture {index}")))
        .collect();

    let field = assert_band_changed(
        SMOKE,
        &shots,
        &captures,
        (0, 1),
        "Assertion 1: typing the query changed nothing in the sidebar band, so the keys never \
         reached the field. Ctrl+Shift+F did not show the Search page or did not focus its \
         field; typing runs nothing, so nothing else in the band may move between these two.",
    );
    let toggle = assert_band_changed(
        SMOKE,
        &shots,
        &captures,
        (1, 2),
        "Assertion 2: a click at the case toggle's estimated centre changed nothing in the band, \
         so no toggle is drawn there. The toggle row sits after the status line or aligned to \
         the end, or the toggle is missing. If the retained capture shows `Aa` directly under \
         the field at the start margin, the estimate in `st_case_x` and `st_toggle_y` is off for \
         this libadwaita: re-measure it rather than moving the widget.",
    );
    assert!(
        toggle.y_min > field.y_max,
        "{SMOKE}: assertion 3: the pixels the click changed start at y {} and the field's end at \
         y {}, so what the click changed is not below the field. Rows overlapping the field's \
         mean the click landed in the field itself: the toggle row is packed above the entry. \
         Captures kept at {}, {} and {}.",
        toggle.y_min,
        field.y_max,
        shots[0].png.display(),
        shots[1].png.display(),
        shots[2].png.display()
    );
    let all = assert_band_changed(
        SMOKE,
        &shots,
        &captures,
        (1, 3),
        "Assertion 4: `Aa` is off in both captures and nothing ran, so only the all-files toggle \
         may differ, and a click at its estimated centre changed nothing in the band: no toggle \
         is drawn there. In order: `All` is missing, it is not in the toggle row, or the \
         estimate in `st_all_x` is off for this libadwaita. If the retained capture shows `All` \
         after `.*` in the row, re-measure the estimate rather than moving the widget. An `All` \
         packed before `.*` passes here, because the click turns `.*` instead, and fails \
         assertion 7.",
    );
    assert!(
        all.x_min > toggle.x_max,
        "{SMOKE}: assertion 4: between captures 1 and 3 the band changes from x {}, not right of \
         `Aa`'s last changed column at x {}. `Aa` is off in both, so it must be drawn alike in \
         them: either the second click on `Aa` did not turn it back off, the click at `st_all_x` \
         reached `Aa` or a toggle left of it, or the status line changed though nothing ran. \
         Captures kept at {}, {} and {}.",
        all.x_min,
        toggle.x_max,
        shots[1].png.display(),
        shots[2].png.display(),
        shots[3].png.display()
    );
    let results = assert_band_changed(
        SMOKE,
        &shots,
        &captures,
        (3, 4),
        "Assertion 5: running the query changed nothing in the band, so the status line and the \
         rows never updated: the Enter did not reach the field, or no search ran.",
    );
    let toggles_bottom = toggle.y_max.max(all.y_max);
    assert!(
        results.y_min > toggles_bottom,
        "{SMOKE}: assertion 5: between captures 3 and 4 the band changes from y {}, not below the \
         toggles' last changed row at y {toggles_bottom}. Either the status line or the results \
         sit above the toggles, or a toggle is drawn differently after the query ran, which \
         happens when the Enter reached a toggle rather than the field. Captures kept at {}, {} \
         and {}.",
        results.y_min,
        shots[2].png.display(),
        shots[3].png.display(),
        shots[4].png.display()
    );
    let start_limit = pixel_index(
        ST_PANEL_SPACING_PX + ST_START_TOLERANCE_PX,
        "the toggle row's start limit",
    );
    assert!(
        toggle.x_min <= start_limit,
        "{SMOKE}: assertion 6: the case toggle's leftmost changed column is x {}, right of x \
         {start_limit}, the panel's start margin plus ST_START_TOLERANCE_PX. The toggle row is \
         not left-aligned: it is centred, aligned to the end, or the case toggle is not its \
         first child. Captures kept at {} and {}.",
        toggle.x_min,
        shots[1].png.display(),
        shots[2].png.display()
    );

    assert_band_reads(
        SMOKE,
        &shots,
        4,
        "after `Aa` was clicked on and off, `All` on, and the query ran",
        &[
            Reading {
                term: ST_OCR_CASE_LOWER,
                held: true,
                means: "Assertion 7: the search did not find the line holding the query in its \
                        own case. No search ran, or its rows never reached the panel, or it ran \
                        with `All` off, which leaves both `.txt` fixtures unsearched: the click \
                        at `st_all_x` turned another toggle, as it turns `.*` when `All` is \
                        packed before it, or the toggle does not reach `search_options_for`.",
            },
            Reading {
                term: ST_OCR_CASE_UPPER,
                held: true,
                means: "Assertion 7: the search missed the upper-case line. If it missed the \
                        lower one too, `All` was not on: read that reading first. Otherwise it \
                        ran case-sensitively with the toggle back off: the toggle starts on, so \
                        two clicks left it on, or `search_options_for` ignores it.",
            },
        ],
    );
    assert_band_reads(
        SMOKE,
        &shots,
        5,
        "after `Aa` was clicked on and Enter pressed on the unchanged query",
        &[
            Reading {
                term: ST_OCR_CASE_LOWER,
                held: true,
                means: "Assertion 8: a case-sensitive search must still find the line holding \
                        the query in its own case. Its absence means the Enter opened a row or \
                        ran something else.",
            },
            Reading {
                term: ST_OCR_CASE_UPPER,
                held: false,
                means: "Assertion 8: the upper-case line is still listed. Either the search ran \
                        with `-i` though the case toggle is on, so `search_options_for` ignores \
                        it, or the Enter did not re-run: if the band also holds the stale \
                        status, `enter_action` compares the query alone and the Enter opened a \
                        row.",
            },
            Reading {
                term: ST_OCR_STALE,
                held: false,
                means: "Assertion 8: the status still says the options changed, so the Enter \
                        did not re-run the query with the toggles now shown, or the run did not \
                        record them.",
            },
        ],
    );
}

/// With regex on, as the panel opens, the query is a pattern; turning `.*` off marks the results
/// stale, and the next Enter re-runs the query as a literal, which finds nothing.
///
/// Step 0 turns `All` on, because the fixture is a `.txt` and the panel opens Markdown-only.
///
/// ## What each capture reads
///
/// | capture | step | band |
/// | --- | --- | --- |
/// | 0 | Ctrl+Shift+F, click `All`, Ctrl+Shift+F, the query, Enter, Escape | [`ST_OCR_REGEX`] |
/// | 1 | click `.*` | [`ST_OCR_STALE`] and still [`ST_OCR_REGEX`] |
/// | 2 | Ctrl+Shift+F, Enter, Escape | [`ST_OCR_NO_MATCHES`], not [`ST_OCR_REGEX`] |
///
/// Capture 1 still holds the result word because a toggle leaves the rows alone; only the status
/// line says they no longer describe the toggles. Capture 2's no-matches line is what tells a
/// literal re-run from the Enter opening the selected row, which would leave the row drawn.
///
/// ## Falsifiers
///
/// Every edit is in `crates/wolf-gnome/src/search_panel.rs`.
///
/// | ID | edit | expected red |
/// | --- | --- | --- |
/// | C7a | build the regex toggle with `.active(false)` | capture 0 |
/// | C7b | `search_options_for` sets `use_regex: true` | capture 2 |
/// | C7c | `enter_action`'s rule 2 compares the query alone | capture 2: Enter opens a row instead |
/// | C7d | delete `search_status`'s stale arm | capture 1 |
/// | C7e | delete both `connect_toggled` handlers | capture 1 |
/// | C22a | `PanelInner::toggles` sets `all_files: SearchToggles::INITIAL.all_files` | capture 0 |
#[test]
fn regex_toggle_switches_between_pattern_and_literal_under_cage() {
    const SMOKE: &str = "regex_toggle_switches_between_pattern_and_literal_under_cage";

    let runtime_dir = require_smoke_env();
    let shots = run_leg(
        &Leg {
            smoke: SMOKE,
            steps: ST_REGEX_STEPS,
            query: ST_REGEX_QUERY,
            fixtures: &[(ST_REGEX_FILE, ST_REGEX_LINE)],
            shot_prefix: ST_REGEX_SHOT_PREFIX,
            shots: 3,
        },
        &runtime_dir,
    );

    assert_band_reads(
        SMOKE,
        &shots,
        0,
        "after the query ran with the toggles as the panel opens",
        &[Reading {
            term: ST_OCR_REGEX,
            held: true,
            means: "The pattern did not match `foooobar`: the regex toggle starts off, so the \
                    query ran as a literal, or `search_options_for` ignores it. Or `All` was \
                    not on, so the `.txt` fixture went unsearched: the first click missed it \
                    (check `st_all_x` against the retained capture), or the toggle does not \
                    reach `search_options_for`. If the band holds no status line either, no \
                    search ran.",
        }],
    );
    assert_band_reads(
        SMOKE,
        &shots,
        1,
        "after a click on `.*`",
        &[
            Reading {
                term: ST_OCR_STALE,
                held: true,
                means: "The status line does not say the options changed. The click missed the \
                        regex toggle (check `st_regex_x` against the retained capture), the \
                        toggle's `connect_toggled` handler does not refresh the status, or \
                        `search_status` has no stale arm. If the status line is plainly in the \
                        crop and OCR missed it, the dim label needs a reader that scales the \
                        crop up, not a restyled label.",
            },
            Reading {
                term: ST_OCR_REGEX,
                held: true,
                means: "The result row went away when the toggle changed. A toggle runs \
                        nothing and must leave the rows drawn and clickable; only the status \
                        line marks them stale.",
            },
        ],
    );
    assert_band_reads(
        SMOKE,
        &shots,
        2,
        "after Enter on the unchanged query with `.*` off",
        &[
            Reading {
                term: ST_OCR_NO_MATCHES,
                held: true,
                means: "The literal search did not report finding nothing. If the band still \
                        holds the result word, either the search ran as a pattern again, so \
                        `search_options_for` ignores the toggle, or the Enter opened the row: \
                        `enter_action` compares the query alone.",
            },
            Reading {
                term: ST_OCR_REGEX,
                held: false,
                means: "The result row is still drawn, so the Enter did not re-run the query \
                        as a literal: it ran the pattern again, or opened the selected row.",
            },
        ],
    );
}

/// With regex on, an invalid pattern makes rg exit with status 2, and the status line says the
/// search failed rather than that it found nothing; turning `.*` off while the field holds focus
/// and pressing Enter searches the same text literally.
///
/// Step 0 turns `All` on, because the fixture is a `.txt` and the panel opens Markdown-only, and
/// presses Ctrl+Shift+F again before the query. A toggle that takes focus on click then fails
/// capture 1, where C8b expects it, not capture 0.
///
/// ## What each capture reads
///
/// | capture | step | band |
/// | --- | --- | --- |
/// | 0 | Ctrl+Shift+F, click `All`, Ctrl+Shift+F, the query, Enter, Escape | [`ST_OCR_RG_EXITED`], not [`ST_OCR_NO_MATCHES`] or [`ST_OCR_PAREN`] |
/// | 1 | Ctrl+Shift+F, click `.*`, Enter, Escape | [`ST_OCR_PAREN`], not [`ST_OCR_RG_EXITED`] |
///
/// The click in step 1 comes with the field focused, so the Enter after it reaches the field only
/// if the toggle did not take focus on click.
///
/// ## Falsifiers
///
/// Every edit is in `crates/wolf-gnome/src/search_panel.rs`.
///
/// | ID | edit | expected red |
/// | --- | --- | --- |
/// | C8a | move `search_status`'s error arm below its `total_matches == 0` arm | capture 0 |
/// | C8b | delete `.focus_on_click(false)` from `search_toggle` | capture 1: Enter flips the focused toggle back |
/// | C8c | `enter_action`'s rule 2 compares the query alone | capture 1: Enter does nothing over no rows |
/// | C23a | `PanelInner::toggles` sets `all_files: SearchToggles::INITIAL.all_files` | capture 1 |
#[test]
fn invalid_regex_fails_in_the_status_line_until_regex_is_off_under_cage() {
    const SMOKE: &str = "invalid_regex_fails_in_the_status_line_until_regex_is_off_under_cage";

    let runtime_dir = require_smoke_env();
    let shots = run_leg(
        &Leg {
            smoke: SMOKE,
            steps: ST_PAREN_STEPS,
            query: ST_PAREN_QUERY,
            fixtures: &[(ST_PAREN_FILE, ST_PAREN_LINE)],
            shot_prefix: ST_PAREN_SHOT_PREFIX,
            shots: 2,
        },
        &runtime_dir,
    );

    assert_band_reads(
        SMOKE,
        &shots,
        0,
        "after the invalid pattern ran with regex on",
        &[
            Reading {
                term: ST_OCR_RG_EXITED,
                held: true,
                means: "The status line does not say the search failed. If it reads no \
                        matches, `search_status` puts the no-matches arm before the error arm; \
                        if it reads a count, the query ran as a literal, so regex did not start \
                        on. If the status line is plainly in the crop and OCR missed the word, \
                        assert another word of it that this reader reads in the retained crop, \
                        and never restyle the label.",
            },
            Reading {
                term: ST_OCR_NO_MATCHES,
                held: false,
                means: "rg's exit status 2 reads as a search that found nothing, which sends the \
                        user after another query when the pattern is what is wrong: the \
                        no-matches arm comes before the error arm.",
            },
            Reading {
                term: ST_OCR_PAREN,
                held: false,
                means: "The line holding the query literally was found, so the query ran as a \
                        literal: the regex toggle starts off.",
            },
        ],
    );
    assert_band_reads(
        SMOKE,
        &shots,
        1,
        "after `.*` was clicked off with the field focused and Enter pressed",
        &[
            Reading {
                term: ST_OCR_PAREN,
                held: true,
                means: "The literal search did not run. If the band still says the search \
                        failed, either the click gave the toggle focus and the Enter flipped it \
                        back on, so the toggle takes focus on click, or the click missed the \
                        toggle (check `st_regex_x` against the retained capture). If it says the \
                        options changed, the Enter did nothing: `enter_action` compares the \
                        query alone. If it says no matches, the literal search ran with `All` \
                        off, so the `.txt` fixture went unsearched: the first click missed `All` \
                        (check `st_all_x`), or the toggle does not reach `search_options_for`.",
            },
            Reading {
                term: ST_OCR_RG_EXITED,
                held: false,
                means: "The status line still reports rg's error, so the search with regex off \
                        never replaced the failed one.",
            },
        ],
    );
}

/// The panel opens Markdown-only, as its counterpart does; turning `All` on marks the results
/// stale, and the next Enter re-runs the query over every file.
///
/// ## What each capture reads
///
/// | capture | step | band holds | band lacks |
/// | --- | --- | --- | --- |
/// | 0 | Ctrl+Shift+F, the query, Enter, Escape | [`ST_OCR_ALL_MD`] | [`ST_OCR_ALL_TXT`] |
/// | 1 | click `All` | [`ST_OCR_STALE`], [`ST_OCR_ALL_MD`] | [`ST_OCR_ALL_TXT`] |
/// | 2 | Ctrl+Shift+F, Enter, Escape | [`ST_OCR_ALL_MD`], [`ST_OCR_ALL_TXT`] | [`ST_OCR_STALE`] |
///
/// The query matches both files, so capture 0 shows the panel opening Markdown-only, and the
/// Markdown word in it is what tells a search that ran from one that did not. Capture 1 shows a
/// flip running nothing and only turning the status line stale; capture 2 shows Enter re-running
/// over every file.
///
/// ## Falsifiers
///
/// Every edit is in `crates/wolf-gnome/src/search_panel.rs`.
///
/// | ID | edit | expected red |
/// | --- | --- | --- |
/// | C21a | `search_options_for` sets `all_files: true` | capture 0 holds [`ST_OCR_ALL_TXT`] |
/// | C21b | `search_options_for` sets `all_files: false` | capture 2 lacks [`ST_OCR_ALL_TXT`] |
/// | C21c | the `connect_toggled` loop runs over `[&case_toggle, &regex_toggle]` only | capture 1 lacks [`ST_OCR_STALE`] |
/// | C21d | build the `All` toggle with `.active(true)` | capture 0 holds [`ST_OCR_ALL_TXT`] |
#[test]
fn all_files_toggle_widens_a_markdown_only_search_under_cage() {
    const SMOKE: &str = "all_files_toggle_widens_a_markdown_only_search_under_cage";

    let runtime_dir = require_smoke_env();
    let shots = run_leg(
        &Leg {
            smoke: SMOKE,
            steps: ST_ALL_STEPS,
            query: ST_ALL_QUERY,
            fixtures: &[
                (ST_ALL_MD_FILE, ST_ALL_MD_LINE),
                (ST_ALL_TXT_FILE, ST_ALL_TXT_LINE),
            ],
            shot_prefix: ST_ALL_SHOT_PREFIX,
            shots: 3,
        },
        &runtime_dir,
    );

    assert_band_reads(
        SMOKE,
        &shots,
        0,
        "after the query ran with the toggles as the panel opens",
        &[
            Reading {
                term: ST_OCR_ALL_MD,
                held: true,
                means: "The Markdown line was not found, though either scope finds it: no \
                        search ran, or its rows never reached the panel. If the band holds no \
                        status line either, the query never reached the field.",
            },
            Reading {
                term: ST_OCR_ALL_TXT,
                held: false,
                means: "The `.txt` line was found with the toggles as the panel opens, so the \
                        panel opens searching every file where its counterpart opens \
                        Markdown-only: `SearchToggles::INITIAL` starts `all_files` on, the `All` \
                        toggle is built active, or `search_options_for` ignores the toggle and \
                        sets it on.",
            },
        ],
    );
    assert_band_reads(
        SMOKE,
        &shots,
        1,
        "after a click on `All`",
        &[
            Reading {
                term: ST_OCR_STALE,
                held: true,
                means: "The status line does not say the options changed. The click missed the \
                        all-files toggle (check `st_all_x` against the retained capture), the \
                        toggle's `connect_toggled` handler does not refresh the status, or \
                        `search_status` ignores `all_files`. If the status line is plainly in \
                        the crop and OCR missed it, the dim label needs a reader that scales the \
                        crop up, not a restyled label.",
            },
            Reading {
                term: ST_OCR_ALL_MD,
                held: true,
                means: "The result row went away when the toggle changed. A toggle runs \
                        nothing and must leave the rows drawn and clickable; only the status \
                        line marks them stale.",
            },
            Reading {
                term: ST_OCR_ALL_TXT,
                held: false,
                means: "The `.txt` line appeared on a click alone, so turning `All` on ran a \
                        search. A toggle must run nothing until the next Enter.",
            },
        ],
    );
    assert_band_reads(
        SMOKE,
        &shots,
        2,
        "after Enter on the unchanged query with `All` on",
        &[
            Reading {
                term: ST_OCR_ALL_MD,
                held: true,
                means: "The Markdown line is gone, though a search over every file still finds \
                        it: the Enter opened a row or ran something other than the query.",
            },
            Reading {
                term: ST_OCR_ALL_TXT,
                held: true,
                means: "The `.txt` line was not found after Enter with `All` on. If the band \
                        still says the options changed, the Enter did not re-run: \
                        `enter_action` ignores `all_files` and opened a row instead. Otherwise \
                        the search ran Markdown-only again: `search_options_for` ignores the \
                        toggle, or `PanelInner::toggles` does not read it.",
            },
            Reading {
                term: ST_OCR_STALE,
                held: false,
                means: "The status still says the options changed, so the Enter did not re-run \
                        the query with the toggles now shown, or the run did not record them.",
            },
        ],
    );
}

/// What the restore smoke types over the restored query, so the save at the quit shows the
/// field's text is recorded. It matches no fixture and is never run.
const ST_RESTORE_REPLACEMENT: &str = "zzrestored";

/// How long the restored search gets to stream its rows into the panel after the launch.
const ST_RESTORE_SEARCH_SETTLE: &str = "2.0";

/// The scenario of [`a_restored_search_sidebar_runs_its_query_under_cage`]: one launch with no
/// pointer on the seat over a planted Search sidebar, one capture, two frozen saves.
///
/// The Escape before the capture hands focus back to the editor whichever widget holds it, so
/// neither the field's caret nor its focus ring is in the band. The field's text is replaced
/// with End and Backspaces rather than a select-all, which leaves the result the same whether
/// or not the entry selects its text on focus.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | the app died before it was ready for input |
/// | `16` | no `wolf.sqlite` to freeze at M or at the quit |
/// | `40` | the app never reported ready |
///
/// The prelude's `quit_app` ends the run, with its own 241 and 242.
const ST_RESTORE_SCENARIO: &str = r#"
set -eu

SETTLE=1.0

cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome restored search smoke"

# --- The band and M: the restored page, field, toggles and rows ---------------------------
sleep "$ST_SEARCH_SETTLE"
wtype -k Escape
sleep "$SETTLE"
grim "$ST_SHOT"
if ! freeze_state "$STATE_SRC" "$STATE_M"; then
    echo "wolf-gnome restored search smoke: no wolf.sqlite in $STATE_SRC at M." >&2
    exit 16
fi

# --- Q: the field's text replaced, never run, then the quit --------------------------------
wtype -M ctrl -M shift -k f -m shift -m ctrl
sleep "$SETTLE"
wtype -k End
n=0
while [ "$n" -lt 32 ]; do
    wtype -k BackSpace
    n=$((n + 1))
done
wtype -- "$ST_REPLACEMENT"
sleep "$SETTLE"
wtype -k Escape
sleep "$SETTLE"
quit_app "wolf-gnome restored search smoke"

if ! freeze_state "$STATE_SRC" "$STATE_Q"; then
    echo "wolf-gnome restored search smoke: no wolf.sqlite in $STATE_SRC after the quit." >&2
    exit 16
fi
exit 0
"#;

/// **A tab saved on its Search page restores the page, the query and the three toggles, and
/// runs the search; the save then follows the field.**
///
/// ## The scenario
///
/// One launch through [`cage_command`] over a planted tab on the project whose sidebar is
/// visible, on Search, with the query [`ST_CASE_QUERY`], case on, regex off and all files on.
/// The fixtures are the case smoke's two `.txt` files, so only the case and all-files toggles
/// decide what is found: with all files off neither is searched, and with case off both are.
/// After the launch the sidebar band is captured and M frozen. Ctrl+Shift+F, the field's text
/// replaced with [`ST_RESTORE_REPLACEMENT`] and no Enter, Escape and the prelude's `quit_app`
/// follow, and Q is frozen.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | E19a | the band holds [`ST_OCR_CASE_LOWER`] | `window::build` passes `run: false` | the lower word is missing |
/// | E19c | the same | `restore` leaves the All toggle unset | the lower word is missing |
/// | E19d | the same | `apply_selected_tab` shows no page | the lower word is missing |
/// | E19b | the band lacks [`ST_OCR_CASE_UPPER`] | `restore` leaves the case toggle unset | the upper word is present |
/// | E19e | M's sidebar is the planted one | `restore` leaves the regex toggle unset | M: regex `Some(true)` |
/// | E19f | the same | `restore` sets no field text | M: query `None` |
/// | E19g | Q's sidebar is the planted one with the query [`ST_RESTORE_REPLACEMENT`] | `refresh_subtree` records no sidebar | Q: query [`ST_CASE_QUERY`] |
///
/// The regex toggle reaches no OCR, since the query is a literal either way, so M is what says
/// it was restored.
#[test]
fn a_restored_search_sidebar_runs_its_query_under_cage() {
    const SMOKE: &str = "a_restored_search_sidebar_runs_its_query_under_cage";

    assert_search_toggle_fixtures(SMOKE);
    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (ST_CASE_LOWER_FILE, ST_CASE_LOWER_LINE),
            (ST_CASE_UPPER_FILE, ST_CASE_UPPER_LINE),
        ],
    );
    let (_, project) = canonical_project(SMOKE, &cwd);
    let planted_sidebar = Sidebar {
        visible: true,
        mode: Some(SidebarMode::Search),
        query: Some(ST_CASE_QUERY.to_owned()),
        case_sensitive: Some(true),
        use_regex: Some(false),
        all_files: Some(true),
    };
    let planted = WindowState {
        windows: vec![planted_window(
            0,
            vec![Tab {
                directory: project,
                content: TabContent {
                    root: Some(Node::Pane(Pane::Edit {
                        path: String::new(),
                        cursor: None,
                        top_line: None,
                    })),
                    sidebar: planted_sidebar.clone(),
                    ..TabContent::default()
                },
            }],
        )],
    };
    plant_window_state(SMOKE, &cwd, &planted);

    let shot = st_shots(&runtime_dir, "wolf_gnome_st_restore_", 1, SMOKE)
        .pop()
        .expect("one capture was asked for");
    let state_m = frozen_state_dir(&runtime_dir, "wolf_gnome_st_restore_m_state", SMOKE);
    let state_q = frozen_state_dir(&runtime_dir, "wolf_gnome_st_restore_q_state", SMOKE);

    let status = cage_command(ST_RESTORE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("ST_SEARCH_SETTLE", ST_RESTORE_SEARCH_SETTLE)
        .env("ST_SHOT", &shot.png)
        .env("ST_REPLACEMENT", ST_RESTORE_REPLACEMENT)
        .env("STATE_SRC", smoke_state_dir(&cwd))
        .env("STATE_M", &state_m)
        .env("STATE_Q", &state_q)
        .status()
        .expect("failed to spawn `cage` for the headless restored search scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage scenario exited unsuccessfully ({status}). Exit 11: the app died \
         before it was ready, as a panic in the sidebar's restore would. Exit 40: it never \
         reported ready. Exit 16: no wolf.sqlite to freeze. {QUIT_APP_EXITS} A 241 here most \
         likely means the keys after the field's Escape reached no Neovim."
    );

    assert_band_reads(
        SMOKE,
        std::slice::from_ref(&shot),
        0,
        "after the launch restored the Search page",
        &[
            Reading {
                term: ST_OCR_CASE_LOWER,
                held: true,
                means: "The restored search did not find the line holding the query in its own \
                        case. In order: the band shows the Files page, so the tab switch at \
                        launch did not show the saved page; no search ran at the restore; or it \
                        ran with All off, which leaves both `.txt` fixtures unsearched, so the \
                        saved all-files toggle was not restored. If the band shows the field \
                        empty, the saved query was not restored either.",
            },
            Reading {
                term: ST_OCR_CASE_UPPER,
                held: false,
                means: "The upper-case line was found, so the restored search ignored case: the \
                        saved case toggle was not restored before the search ran.",
            },
        ],
    );

    let saved = frozen_window_state(&state_m, SMOKE, "M, frozen after the launch");
    let sidebar = saved
        .windows
        .first()
        .and_then(|window| window.tabs.first())
        .map(|tab| tab.content.sidebar.clone());
    assert_eq!(
        sidebar.as_ref(),
        Some(&planted_sidebar),
        "{SMOKE}: M saved the tab's sidebar as {sidebar:?}; want the planted one. regex Some(true) \
         means the saved regex toggle was not restored, which the capture cannot show; query \
         None means the field was restored empty; mode Tree means the page was not restored. \
         File: {}",
        state_m.display()
    );

    let saved = frozen_window_state(&state_q, SMOKE, "Q, frozen after the quit");
    let sidebar = saved
        .windows
        .first()
        .and_then(|window| window.tabs.first())
        .map(|tab| tab.content.sidebar.clone());
    let want = Sidebar {
        query: Some(ST_RESTORE_REPLACEMENT.to_owned()),
        ..planted_sidebar
    };
    assert_eq!(
        sidebar.as_ref(),
        Some(&want),
        "{SMOKE}: Q saved the tab's sidebar as {sidebar:?}; want the planted one with the query \
         {ST_RESTORE_REPLACEMENT:?} the field held at the quit. The query {ST_CASE_QUERY:?} \
         means the selected tab's sidebar is not recorded from the widgets when its window is \
         gathered, so the restore's record is saved forever. File: {}",
        state_q.display()
    );
}
