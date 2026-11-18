//! The header bar's Files/Search toggle group: clicks on its toggles show, switch and hide the
//! sidebar, and the group follows Ctrl+Shift+B, Ctrl+Shift+F and `:WolfSidebarShowProject`.
//!
//! Both smokes read two things off every capture. The sidebar's state is OCR of the sidebar band
//! ([`projsearch_sidebar_band`]): a tree row says Files, the editor's heading says the sidebar is
//! hidden, since the pane then slides under the band, and neither says Search. The group's state
//! is the header band's raw pixels, compared between captures of one run: two captures in one
//! state must be identical there, and captures in different states must differ.

use super::editor::SAVE_AS_GDK_DEBUG;
use super::sidebar::{
    projsearch_sidebar_band, search_target_fixture_body, PROJSEARCH_FILE_TARGET,
    PROJSEARCH_OCR_RESULT, PROJSEARCH_QUERY,
};
use super::titlebar::{TB_HEADER_BUTTON_CLEARANCE_PX, TB_HEADER_ROW_INSET_PX, TB_HEADER_SHADE_PX};
use super::*;

/// The file whose tree row says the Files page is up.
///
/// It is never opened, so inside the sidebar band only its tree row can draw its name: the
/// pane's statusbar strip and the `:e` echo, which reach the band while the sidebar is hidden,
/// name [`TG_OPEN_FILE`] instead. Its name carries [`TG_OCR_TREE_ROW`], so it is absent from
/// `RELAUNCH_FIXTURE_NAMES` and has an exemption block in
/// `restore_fixture_names_cannot_satisfy_their_own_assertions`.
pub(crate) const TG_TREE_FILE: &str = "tgrowan.md";

/// Lowercased substring of [`TG_TREE_FILE`]'s tree row.
pub(crate) const TG_OCR_TREE_ROW: &str = "tgrowan";

/// The file the editor holds throughout both smokes. Its heading is the hidden sidebar's
/// observable, so its name is unlike [`TG_OCR_KEEP`].
pub(crate) const TG_OPEN_FILE: &str = "tgbase.md";

/// The whole content of [`TG_OPEN_FILE`]; lowercased it contains [`TG_OCR_KEEP`].
///
/// Short on purpose. It is read inside the sidebar band once a hidden sidebar lets the pane
/// start at the output's left edge, so the whole heading must end left of
/// [`OVERLAY_ORIGIN_X_EXPECTED`]. Both smokes assert that from the measured cell before they
/// launch, through [`assert_keep_heading_fits`], rather than trusting this comment.
pub(crate) const TG_HEADING: &str = "# HOLDFIR";

/// Lowercased substring OCR finds in the sidebar band only while the sidebar is hidden: the
/// heading of the file the editor holds.
///
/// Neither page of the sidebar draws it. The tree draws file names, and the search panel draws
/// the query, its toggles' labels `Aa` and `.*`, its status line and the one matching line,
/// which [`TG_HEADING`] is not.
pub(crate) const TG_OCR_KEEP: &str = "holdfir";

/// libadwaita 1.9.3's padding of the header bar's box at its start edge.
const TG_BAR_PADDING_PX: f64 = 7.0;

/// The linked Back/Forward pair: two of the header bar's image buttons, each about 34 px.
const TG_NAV_PAIR_WIDTH_PX: f64 = 68.0;

/// libadwaita 1.9.3's spacing between the header bar's start children.
const TG_BAR_SPACING_PX: f64 = 6.0;

/// libadwaita 1.9.3's padding of a toggle group around its toggles.
const TG_GROUP_PADDING_PX: f64 = 3.0;

/// One toggle's width, taken as an image button's 34 px.
const TG_TOGGLE_WIDTH_PX: f64 = 34.0;

/// What separates the two toggles: about one pixel, an estimate rather than a measurement.
const TG_TOGGLE_GAP_PX: f64 = 1.0;

/// The fewest header-band pixels two captures in different group states must differ by.
///
/// The active toggle's background alone covers several hundred pixels, so a real change of
/// state clears this by far, while a stray antialiased pixel or two does not.
const TG_GROUP_MIN_CHANGED_PIXELS: usize = 40;

/// Captures of the click smoke, under `XDG_RUNTIME_DIR` so no tree row draws them. The scenario
/// appends the capture's index and `.png`.
const TG_CLICK_SHOT_PREFIX: &str = "wolf_gnome_tg_click_";

/// Captures of the chord smoke; see [`TG_CLICK_SHOT_PREFIX`].
const TG_CHORD_SHOT_PREFIX: &str = "wolf_gnome_tg_chord_";

/// How many captures the click smoke takes.
const TG_CLICK_SHOTS: usize = 7;

/// How many captures the chord smoke takes.
const TG_CHORD_SHOTS: usize = 6;

/// Centre of the Files toggle, in absolute output pixels: the bar's padding, the Back/Forward
/// pair, the spacing after it, the group's padding and half a toggle, about 101.
///
/// The toggles' widths are not measured, and they need not be: this point and
/// [`tg_search_x`]'s stay inside their own toggles for toggle widths from 28 to 40 px. A wrong
/// value fails loudly rather than passing by accident, because a click that misses the group
/// moves no sidebar.
fn tg_files_x() -> f64 {
    TG_BAR_PADDING_PX
        + TG_NAV_PAIR_WIDTH_PX
        + TG_BAR_SPACING_PX
        + TG_GROUP_PADDING_PX
        + TG_TOGGLE_WIDTH_PX / 2.0
}

/// Centre of the Search toggle: one toggle and the gap right of [`tg_files_x`], about 136.
fn tg_search_x() -> f64 {
    tg_files_x() + TG_TOGGLE_WIDTH_PX + TG_TOGGLE_GAP_PX
}

/// The row both toggles are clicked on: the middle of the header bar above its shade, which is
/// where libadwaita centres its children.
fn tg_click_y() -> f64 {
    (OVERLAY_ORIGIN_Y_EXPECTED - TB_HEADER_SHADE_PX) / 2.0
}

/// Body of [`toggle_clicks_show_switch_and_hide_the_sidebar_under_cage`], appended to
/// [`POINTER_PREAMBLE`], which has launched the app and parked the pointer at the probe point
/// over the pane area.
///
/// Seven captures, `${TG_SHOT}0.png` to `${TG_SHOT}6.png`: at launch, then after each of six
/// clicks. Every click ends with the pointer back over the pane area, so no hover look or
/// tooltip reaches a capture. Nothing navigates after the first capture, so Back and Forward
/// keep one look in every header band.
const TG_CLICK_BODY: &str = r#"
# The tree's FileSearch streams the cwd listing in; capture 0 reads a tree row.
TREE_SETTLE=1.5

tg_click() {
    hsend motion "$1" "$TG_CLICK_Y"
    hsend press "$BTN_LEFT"
    hsend release "$BTN_LEFT"
    hsend motion "$PROBE_X" "$PROBE_Y"
}

sleep "$TREE_SETTLE"

# The editor holds the keep heading throughout; the cursor is parked off its row.
wtype -- ":e $TG_OPEN_FILE"
wtype -k Return
sleep "$SETTLE"
wtype -- o
wtype -k Escape
sleep "$SETTLE"
grim "${TG_SHOT}0.png"

# 1: Search, from Files, switches.
tg_click "$TG_SEARCH_X"
sleep "$SETTLE"
grim "${TG_SHOT}1.png"

# 2: Search again, the active toggle, hides.
tg_click "$TG_SEARCH_X"
sleep "$SETTLE"
grim "${TG_SHOT}2.png"

# 3: Search while hidden shows Search; only show_search_panel focuses the field here.
tg_click "$TG_SEARCH_X"
sleep "$SETTLE"
wtype -- "$TG_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$SETTLE"
grim "${TG_SHOT}3.png"

# 4: Files, from Search, switches; the tree catches up once it is back on screen.
tg_click "$TG_FILES_X"
sleep "$TREE_SETTLE"
grim "${TG_SHOT}4.png"

# 5: Files again, the active toggle, hides.
tg_click "$TG_FILES_X"
sleep "$SETTLE"
grim "${TG_SHOT}5.png"

# 6: Files while hidden shows Files.
tg_click "$TG_FILES_X"
sleep "$TREE_SETTLE"
grim "${TG_SHOT}6.png"

hsend quit
exit 0
"#;

/// The chord scenario of [`sidebar_chords_move_the_toggle_group_under_cage`], run as a single
/// `sh -c` string inside `cage`, with no pointer on the seat, so no hover look can reach a
/// capture.
///
/// Six captures, `${TG_SHOT}0.png` to `${TG_SHOT}5.png`: at launch, then after each of five
/// steps. The Escape before the Ex command leaves the keys with Neovim whichever widget holds
/// focus: in the panel's field it hands focus back, and in Neovim's normal mode it does nothing.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `40` — the app never became ready (the shared convention).
const TG_CHORD_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# The tree's FileSearch budget; TREE_SCENARIO's value.
TREE_SETTLE=1.5

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome toggle-group chord smoke"

sleep "$TREE_SETTLE"

# The editor holds the keep heading throughout; the cursor is parked off its row.
wtype -- ":e $TG_OPEN_FILE"
wtype -k Return
sleep "$SETTLE"
wtype -- o
wtype -k Escape
sleep "$SETTLE"
grim "${TG_SHOT}0.png"

# 1: Ctrl+Shift+B hides the sidebar from Files.
wtype -M ctrl -M shift -k b -m shift -m ctrl
sleep "$SETTLE"
grim "${TG_SHOT}1.png"

# 2: Ctrl+Shift+F shows Search, then the query runs.
wtype -M ctrl -M shift -k f -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$TG_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$SETTLE"
grim "${TG_SHOT}2.png"

# 3: Ctrl+Shift+B hides the sidebar from Search.
wtype -M ctrl -M shift -k b -m shift -m ctrl
sleep "$SETTLE"
grim "${TG_SHOT}3.png"

# 4: Ctrl+Shift+B shows it again, on Search.
wtype -M ctrl -M shift -k b -m shift -m ctrl
sleep "$SETTLE"
grim "${TG_SHOT}4.png"

# 5: the Ex command switches to Files.
wtype -k Escape
sleep "$SETTLE"
wtype -- ":WolfSidebarShowProject"
wtype -k Return
sleep "$TREE_SETTLE"
grim "${TG_SHOT}5.png"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The sidebar's state as the sidebar band reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sidebar {
    /// The band holds [`TG_OCR_TREE_ROW`] and not [`TG_OCR_KEEP`].
    Files,
    /// The band holds neither term.
    Search,
    /// The band holds [`TG_OCR_KEEP`] and not [`TG_OCR_TREE_ROW`].
    Hidden,
}

/// What one capture's sidebar band must read.
struct Expect {
    sidebar: Sidebar,
    /// Whether the band must hold [`PROJSEARCH_OCR_RESULT`] (`Some(true)`), must lack it
    /// (`Some(false)`), or is not asked (`None`).
    result: Option<bool>,
    /// The step that led to the capture.
    step: &'static str,
    /// What each wrong reading of this capture means.
    causes: &'static str,
}

/// One capture: the full PNG and its sidebar-band crop.
struct Shot {
    png: PathBuf,
    band_png: PathBuf,
}

/// The paths of `count` captures named `<prefix><index>.png` under `runtime`, each cleared
/// first, with their band crops beside them.
fn tg_shots(runtime: &Path, prefix: &str, count: usize, smoke: &str) -> Vec<Shot> {
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

/// Writes both smokes' fixtures into `cwd`: the file the editor holds, the never-opened file
/// whose tree row says Files, the search target and the sidebar fillers.
///
/// The search target is the project-search smokes' own, so [`PROJSEARCH_QUERY`] matches one
/// line in the whole cwd and the panel draws [`PROJSEARCH_OCR_RESULT`] once it has run.
fn write_toggle_fixtures(cwd: &Path, smoke: &str) {
    write_relaunch_fixtures(smoke, cwd, &[(TG_OPEN_FILE, TG_HEADING)]);

    let tree = cwd.join(TG_TREE_FILE);
    std::fs::write(&tree, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
        panic!(
            "{smoke}: failed to write the tree-row fixture {}: {e}",
            tree.display()
        )
    });

    let target = cwd.join(PROJSEARCH_FILE_TARGET);
    std::fs::write(&target, format!("{}\n", search_target_fixture_body())).unwrap_or_else(|e| {
        panic!(
            "{smoke}: failed to write the search target {}: {e}",
            target.display()
        )
    });
}

/// Asserts that [`TG_HEADING`] ends left of the sidebar band's right edge once the pane starts
/// at the output's left edge, measured with the cell the app is launched with.
fn assert_keep_heading_fits(smoke: &str) {
    let metrics = cell_metrics_for_smoke();
    let cells = u32::try_from(TG_HEADING.chars().count())
        .unwrap_or_else(|e| panic!("{smoke}: TG_HEADING's length does not fit in a u32 ({e})"));
    let right = wolf_gnome::editor_inset::EDITOR_PADDING_PX + f64::from(cells) * metrics.cell_w;
    assert!(
        right < OVERLAY_ORIGIN_X_EXPECTED,
        "{smoke}: TG_HEADING = {TG_HEADING:?} would end at x = {right} with a hidden sidebar \
         ({cells} cells of {} px after the {} px editor padding), past the sidebar band's right \
         edge at {OVERLAY_ORIGIN_X_EXPECTED}. The band would then hold only part of the heading, \
         and a hidden sidebar would read as Search. Shorten the heading and its term together; \
         do not widen the band, which would let the pane area's own text into the Files and \
         Search readings.",
        metrics.cell_w,
        wolf_gnome::editor_inset::EDITOR_PADDING_PX
    );
}

/// Asserts that every capture's sidebar band reads as its [`Expect`] says.
fn assert_sidebar_steps(smoke: &str, shots: &[Shot], expects: &[Expect]) {
    for (index, (shot, expect)) in shots.iter().zip(expects).enumerate() {
        assert_png_present(smoke, &shot.png);
        let text =
            ocr_band_to_lowercase(&shot.png, projsearch_sidebar_band(), &shot.band_png, smoke);
        let tree = text.contains(TG_OCR_TREE_ROW);
        let keep = text.contains(TG_OCR_KEEP);
        assert!(
            !(tree && keep),
            "{smoke}: the sidebar band of capture {index}, {}, holds both the tree row \
             {TG_OCR_TREE_ROW:?} and the editor's heading {TG_OCR_KEEP:?}. The tree draws only \
             while the sidebar is shown and the heading reaches the band only while it is \
             hidden, so the band is no longer the sidebar's alone: re-derive \
             `projsearch_sidebar_band` from OVERLAY_ORIGIN_*_EXPECTED, and check that hiding the \
             sidebar both hides the widget and moves the paned. Capture kept at {}, crop at \
             {}.\n\nBand OCR was:\n{text}",
            expect.step,
            shot.png.display(),
            shot.band_png.display()
        );
        let seen = match (tree, keep) {
            (true, _) => Sidebar::Files,
            (false, true) => Sidebar::Hidden,
            (false, false) => Sidebar::Search,
        };
        assert_eq!(
            seen,
            expect.sidebar,
            "{smoke}: the sidebar band of capture {index}, {}, reads as {seen:?}, not {:?}. Files \
             means it holds the tree row {TG_OCR_TREE_ROW:?}; Hidden means it holds the editor's \
             heading {TG_OCR_KEEP:?}, which reaches the band only when the pane slides left; \
             Search means neither. {} Capture kept at {}, crop at {}.\n\nBand OCR was:\n{text}",
            expect.step,
            expect.sidebar,
            expect.causes,
            shot.png.display(),
            shot.band_png.display()
        );
        if let Some(want) = expect.result {
            assert_eq!(
                text.contains(PROJSEARCH_OCR_RESULT),
                want,
                "{smoke}: the sidebar band of capture {index}, {}, reads as {seen:?} as expected, \
                 but it {} {PROJSEARCH_OCR_RESULT:?}, the word only a search result row draws. \
                 {} Capture kept at {}, crop at {}.\n\nBand OCR was:\n{text}",
                expect.step,
                if want { "lacks" } else { "holds" },
                expect.causes,
                shot.png.display(),
                shot.band_png.display()
            );
        }
    }
}

/// The header band the group's state is read from: the rows inside the header bar, clear of
/// its top edge and its bottom shade, and the columns of the start buttons.
fn tg_header_band() -> SampleRect {
    let band = SampleRect {
        x0: 0,
        x1: pixel_index(
            TB_HEADER_BUTTON_CLEARANCE_PX,
            "the header band's last column",
        ),
        y0: pixel_index(TB_HEADER_ROW_INSET_PX, "the header band's first row"),
        y1: pixel_index(
            OVERLAY_ORIGIN_Y_EXPECTED - TB_HEADER_SHADE_PX - TB_HEADER_ROW_INSET_PX,
            "the header band's last row",
        ),
    };
    assert!(
        band.x1 > band.x0 && band.y1 > band.y0,
        "the header band {band:?} is empty, so every comparison read from it would find two \
         captures identical. Re-derive it from OVERLAY_ORIGIN_Y_EXPECTED and titlebar.rs's \
         TB_HEADER_* constants."
    );
    band
}

/// The header-band pixels in which two captures differ.
struct HeaderDiff {
    changed: usize,
    x_sum: usize,
    x_min: usize,
    x_max: usize,
}

impl HeaderDiff {
    /// The mean column of the changed pixels. Called only once [`assert_group_differs`] has
    /// established that some changed.
    fn mean_x(&self, what: &str) -> f64 {
        band_fraction(self.x_sum, self.changed, what)
    }
}

/// Compares two captures over [`tg_header_band`].
fn header_diff(a: &Capture, b: &Capture) -> HeaderDiff {
    let band = tg_header_band();
    let mut diff = HeaderDiff {
        changed: 0,
        x_sum: 0,
        x_min: usize::MAX,
        x_max: 0,
    };
    for y in band.y0..band.y1 {
        for x in band.x0..band.x1 {
            if a.pixel(x, y) != b.pixel(x, y) {
                diff.changed += 1;
                diff.x_sum += x;
                diff.x_min = diff.x_min.min(x);
                diff.x_max = diff.x_max.max(x);
            }
        }
    }
    diff
}

/// Every capture of one run, decoded, with the states the sidebar assertions established.
struct GroupRun<'a> {
    smoke: &'a str,
    shots: &'a [Shot],
    captures: Vec<Capture>,
}

impl<'a> GroupRun<'a> {
    fn decode(smoke: &'a str, shots: &'a [Shot]) -> Self {
        let captures = shots
            .iter()
            .enumerate()
            .map(|(index, shot)| decode_capture(&shot.png, smoke, &format!("capture {index}")))
            .collect();
        Self {
            smoke,
            shots,
            captures,
        }
    }

    /// Asserts that captures `a` and `b`, both taken with the group in `state`, are identical
    /// in the header band.
    fn assert_same(&self, a: usize, b: usize, state: &str, causes: &str) {
        let diff = header_diff(&self.captures[a], &self.captures[b]);
        assert!(
            diff.changed == 0,
            "{}: captures {a} and {b} were both taken with the sidebar {state}, as the sidebar \
             assertions above read them, yet {} pixels of the header band differ between them, \
             at x {}..={}. The group's look must follow the sidebar's state alone. {causes} Other \
             readings: a hover look or a tooltip reached the band, though the pointer is parked \
             over the pane before each capture; or something else in the band moved, such as Back \
             or Forward changing sensitivity, which means the scenario navigated after capture \
             0. Captures kept at {} and {}.",
            self.smoke,
            diff.changed,
            diff.x_min,
            diff.x_max,
            self.shots[a].png.display(),
            self.shots[b].png.display()
        );
    }

    /// Asserts that captures `a` and `b`, taken with the group in two different states, differ
    /// in the header band by at least [`TG_GROUP_MIN_CHANGED_PIXELS`].
    fn assert_differs(&self, a: usize, b: usize, causes: &str) -> HeaderDiff {
        let diff = header_diff(&self.captures[a], &self.captures[b]);
        assert!(
            diff.changed >= TG_GROUP_MIN_CHANGED_PIXELS,
            "{}: captures {a} and {b} differ in {} pixels of the header band, fewer than \
             TG_GROUP_MIN_CHANGED_PIXELS = {TG_GROUP_MIN_CHANGED_PIXELS}, although the sidebar \
             assertions above read the sidebar in a different state in each. The sidebar moved \
             and the group did not follow. {causes} Captures kept at {} and {}.",
            self.smoke,
            diff.changed,
            self.shots[a].png.display(),
            self.shots[b].png.display()
        );
        diff
    }

    /// Asserts that the pixels that change when Files becomes active lie left of those that
    /// change when Search does, each against the same capture with no toggle active.
    fn assert_orientation(&self, files: usize, search: usize, none: usize) {
        let files_diff = self.assert_differs(
            files,
            none,
            "Files active and no toggle active look the same.",
        );
        let search_diff = self.assert_differs(
            search,
            none,
            "Search active and no toggle active look the same.",
        );
        let files_x = files_diff.mean_x("the Files toggle's changed pixels");
        let search_x = search_diff.mean_x("the Search toggle's changed pixels");
        assert!(
            files_x < search_x,
            "{}: against capture {none}, with no toggle active, the pixels that change in capture \
             {files} (Files) centre at x = {files_x} and those in capture {search} (Search) at x = \
             {search_x}. Files is the group's start toggle, so it must lie left of Search. The \
             group shows the pages the other way round: it adds Search first, or `show_state` \
             gives Files the Search toggle's name. The sidebar assertions can stay green under \
             that, because `toggle_at` reads the click's half rather than the toggle drawn \
             there. Captures kept at {}, {} and {}.",
            self.smoke,
            self.shots[files].png.display(),
            self.shots[search].png.display(),
            self.shots[none].png.display()
        );
    }
}

/// Clicks on the header bar's toggle group show, switch and hide the sidebar, and the group
/// shows which page is up.
///
/// It drives the real window through the pointer harness: from Files, Search switches and a
/// second Search hides; Search while hidden shows the Search page with its field focused; Files
/// switches back, a second Files hides and a third shows Files again.
///
/// ## What each capture reads
///
/// | capture | step | sidebar band | group |
/// | --- | --- | --- | --- |
/// | 0 | launch | Files | Files |
/// | 1 | click Search | Search, no result | Search |
/// | 2 | click Search | Hidden | none |
/// | 3 | click Search, type the query, Enter | Search, with [`PROJSEARCH_OCR_RESULT`] | Search |
/// | 4 | click Files | Files, no result | Files |
/// | 5 | click Files | Hidden | none |
/// | 6 | click Files | Files | Files |
///
/// Then the group: captures 2 and 5 identical, 0, 4 and 6 identical, 1 and 3 identical, the
/// three classes pairwise different, and Files's change left of Search's.
///
/// Capture 1's Search reading is the absence of both terms, which a blank band would also give.
/// Its positive half is capture 3, where the same page draws a result.
///
/// Step 3 is the one place where only `show_search_panel`'s own `focus_entry` can put the typed
/// query in the field: the page was already Search when the sidebar hid, so no page switch
/// focuses anything.
///
/// ## Falsifiers
///
/// | ID | edit | expected red |
/// | --- | --- | --- |
/// | C8 | delete the `Claimed` claim in `connect_click`'s `pressed` handler | a sidebar capture: the toggle's own activation runs and the rule does not |
/// | C9 | `connect_click` calls `f(SidebarPage::Files)` instead of `f(toggle_at(…))` | capture 1 reads Hidden |
/// | C10 | `on_sidebar_toggle_click`'s `Hide` arm does nothing | capture 2 reads Search |
/// | C11 | delete `self.inner.search.focus_entry()` from `show_search_panel` | capture 3 lacks the result |
/// | C12 | pack the toggle group before `nav_box` | capture 1 reads Files: the clicks land on Back and Forward |
///
/// It exports `GDK_DEBUG=no-portals`: a click that misses the group can land on Open, whose
/// folder dialog must not reach a portal.
#[test]
fn toggle_clicks_show_switch_and_hide_the_sidebar_under_cage() {
    const SMOKE: &str = "toggle_clicks_show_switch_and_hide_the_sidebar_under_cage";

    let runtime_dir = require_smoke_env();
    assert_keep_heading_fits(SMOKE);

    let cwd = make_temp_cwd();
    write_toggle_fixtures(&cwd, SMOKE);

    let shots = tg_shots(&runtime_dir, TG_CLICK_SHOT_PREFIX, TG_CLICK_SHOTS, SMOKE);
    let prefix = runtime_dir.join(TG_CLICK_SHOT_PREFIX);
    let last = &shots[TG_CLICK_SHOTS - 1].png;

    let files_x = pixel_target(tg_files_x(), "the Files toggle's x");
    let search_x = pixel_target(tg_search_x(), "the Search toggle's x");
    let click_y = pixel_target(tg_click_y(), "the toggles' y");
    let files_x_env = files_x.to_string();
    let search_x_env = search_x.to_string();
    let click_y_env = click_y.to_string();

    let status = run_pointer_scenario(
        TG_CLICK_BODY,
        &cwd,
        &runtime_dir,
        last,
        &[
            ("TG_SHOT", prefix.as_os_str()),
            ("TG_OPEN_FILE", OsStr::new(TG_OPEN_FILE)),
            ("TG_QUERY", OsStr::new(PROJSEARCH_QUERY)),
            ("TG_FILES_X", OsStr::new(&files_x_env)),
            ("TG_SEARCH_X", OsStr::new(&search_x_env)),
            ("TG_CLICK_Y", OsStr::new(&click_y_env)),
            ("GDK_DEBUG", OsStr::new(SAVE_AS_GDK_DEBUG)),
        ],
    );
    assert_pointer_scenario_succeeded(SMOKE, status, &cwd, last);

    let extents = read_output_extents(&cwd, SMOKE);
    assert_inside_extents(SMOKE, "the Files toggle", (files_x, click_y), extents);
    assert_inside_extents(SMOKE, "the Search toggle", (search_x, click_y), extents);

    assert_sidebar_steps(
        SMOKE,
        &shots,
        &[
            Expect {
                sidebar: Sidebar::Files,
                result: None,
                step: "at launch",
                causes: "A precondition: Search means the tree never populated (raise \
                         TREE_SETTLE in TG_CLICK_BODY) or the app launched into the empty state; \
                         Hidden means the sidebar started hidden.",
            },
            Expect {
                sidebar: Sidebar::Search,
                result: Some(false),
                step: "after a click on Search from Files",
                causes: "Files: the click did not reach the group's rule. If the group drew \
                         Search active, the toggle's own activation ran instead, because the \
                         press was not claimed; if nothing moved, the click landed outside the \
                         group, on Back, Forward or Open, where TG_FILES_X and TG_SEARCH_X land \
                         without the group or with it packed elsewhere. Hidden: the click reached \
                         the rule as Files, the page already up, so the rule hid the sidebar; \
                         `connect_click` is not passing `toggle_at`'s answer. A result here means \
                         a search ran before any query was typed.",
            },
            Expect {
                sidebar: Sidebar::Hidden,
                result: None,
                step: "after a second click on Search",
                causes: "Search: a click on the active toggle did nothing, which is \
                         adw::ToggleGroup's own behaviour; `toggle_click` did not answer Hide, \
                         or `on_sidebar_toggle_click`'s Hide arm does not hide. Files: the click \
                         was read as Files.",
            },
            Expect {
                sidebar: Sidebar::Search,
                result: Some(true),
                step: "after a click on Search while hidden, the query and Enter",
                causes: "Hidden: a click on a hidden sidebar's toggle did not show it. Files: \
                         the click was read as Files. Search without the result: the typed \
                         query never reached the field, so `show_search_panel` did not focus it; \
                         the page was already Search, so nothing else could have.",
            },
            Expect {
                sidebar: Sidebar::Files,
                result: Some(false),
                step: "after a click on Files from Search",
                causes: "Search: the click did not switch the page, or the tree did not catch \
                         up once back on screen (raise TREE_SETTLE before touching anything \
                         else). Hidden: the click was read as Search, the page already up. A \
                         result beside the tree means both pages are drawn at once.",
            },
            Expect {
                sidebar: Sidebar::Hidden,
                result: None,
                step: "after a second click on Files",
                causes: "Files: a click on the active toggle did nothing; the Hide arm does not \
                         hide. Search: the click was read as Search.",
            },
            Expect {
                sidebar: Sidebar::Files,
                result: None,
                step: "after a click on Files while hidden",
                causes: "Hidden: a click on a hidden sidebar's toggle did not show it. Search: \
                         the click was read as Search, or the tree did not catch up.",
            },
        ],
    );

    let run = GroupRun::decode(SMOKE, &shots);
    run.assert_same(0, 4, "on Files", "Capture 4 follows a click from Search.");
    run.assert_same(0, 6, "on Files", "Capture 6 follows a click while hidden.");
    run.assert_same(
        2,
        5,
        "hidden",
        "Capture 2 hid from Search and capture 5 from Files.",
    );
    run.assert_same(
        1,
        3,
        "on Search",
        "Capture 3 follows a click while hidden, capture 1 a switch.",
    );
    run.assert_differs(
        0,
        2,
        "No toggle is active while hidden, so the group shows a stale one.",
    );
    run.assert_differs(
        1,
        2,
        "No toggle is active while hidden, so the group shows a stale one.",
    );
    run.assert_differs(
        0,
        1,
        "Files and Search look the same, so the page never reaches it.",
    );
    run.assert_orientation(0, 1, 2);
}

/// Ctrl+Shift+B, Ctrl+Shift+F and `:WolfSidebarShowProject` move the toggle group with the
/// sidebar, though none of them goes through the group.
///
/// A plain `wtype` scenario with no pointer. The group's state comes from one place, the
/// sidebar's relay, which every visibility change and every page change reaches; this smoke is
/// what fails when one of those paths stops reaching it.
///
/// ## What each capture reads
///
/// | capture | step | sidebar band | group |
/// | --- | --- | --- | --- |
/// | 0 | launch | Files | Files |
/// | 1 | Ctrl+Shift+B | Hidden | none |
/// | 2 | Ctrl+Shift+F, the query, Enter | Search, with [`PROJSEARCH_OCR_RESULT`] | Search |
/// | 3 | Ctrl+Shift+B | Hidden | none |
/// | 4 | Ctrl+Shift+B | Search, with the result | Search |
/// | 5 | Escape, `:WolfSidebarShowProject` | Files, no result | Files |
///
/// Then the group. The first group assertion is that capture 1 differs from capture 0, so a
/// group left stale by the accelerator fails there by name. Then captures 1 and 3 identical, 2
/// and 4 identical, 0 and 5 identical, the three classes pairwise different, and Files's change
/// left of Search's.
///
/// ## Falsifiers
///
/// | ID | edit | expected red |
/// | --- | --- | --- |
/// | C13 | in `SidebarInner::relay`, call `toggles.show_state` only while `self.shown` is true | capture 1 does not differ from capture 0 |
/// | C14 | the stack's `visible-child-name` notify closure skips the group's sync | captures 2 and 4 differ: capture 2 still shows Files |
/// | C15 | remove the `show_state` call from `relay` | capture 1 does not differ from capture 0 |
#[test]
fn sidebar_chords_move_the_toggle_group_under_cage() {
    const SMOKE: &str = "sidebar_chords_move_the_toggle_group_under_cage";

    let runtime_dir = require_smoke_env();
    assert_keep_heading_fits(SMOKE);

    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    write_toggle_fixtures(&cwd, SMOKE);

    let shots = tg_shots(&runtime_dir, TG_CHORD_SHOT_PREFIX, TG_CHORD_SHOTS, SMOKE);
    let prefix = runtime_dir.join(TG_CHORD_SHOT_PREFIX);

    let status = cage_command(TG_CHORD_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("TG_SHOT", &prefix)
        .env("TG_OPEN_FILE", TG_OPEN_FILE)
        .env("TG_QUERY", PROJSEARCH_QUERY)
        .status()
        .expect("failed to spawn `cage` for the headless toggle-group chord scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage chord scenario exited {status}. Exit 11: the app died before it was \
         ready for input, a launch failure, and nothing below is about the group. Exit 40: the \
         app never became ready; its `wolf-ready:` stderr lines name the condition. Any other \
         status means wtype or grim failed or the `:e` did not load; the scenario writes no \
         readback, so it exits 0 whatever the chords did. Captures expected at {}0.png to {}5.png.",
        prefix.display(),
        prefix.display()
    );

    assert_sidebar_steps(
        SMOKE,
        &shots,
        &[
            Expect {
                sidebar: Sidebar::Files,
                result: None,
                step: "at launch",
                causes: "A precondition: Search means the tree never populated (raise \
                         TREE_SETTLE in TG_CHORD_SCENARIO) or the app launched into the empty \
                         state; Hidden means the sidebar started hidden.",
            },
            Expect {
                sidebar: Sidebar::Hidden,
                result: None,
                step: "after Ctrl+Shift+B from Files",
                causes: "Files: the chord did not hide the sidebar; \
                         `sidebar_toggle_chord_hides_the_file_tree_under_cage` reads the same \
                         chord without the group, so read it first.",
            },
            Expect {
                sidebar: Sidebar::Search,
                result: Some(true),
                step: "after Ctrl+Shift+F, the query and Enter",
                causes: "Hidden: the chord did not reveal the sidebar. Files: it did not switch \
                         the page. Search without the result: the query never ran; the \
                         project-search smokes in sidebar.rs read the same chord.",
            },
            Expect {
                sidebar: Sidebar::Hidden,
                result: None,
                step: "after Ctrl+Shift+B from Search",
                causes: "Search: the chord did not hide the sidebar while focus was in the \
                         panel's field.",
            },
            Expect {
                sidebar: Sidebar::Search,
                result: Some(true),
                step: "after Ctrl+Shift+B while hidden on Search",
                causes: "Hidden: the chord did not show the sidebar. Files: showing it changed \
                         the page. Search without the result: showing it cleared the panel.",
            },
            Expect {
                sidebar: Sidebar::Files,
                result: Some(false),
                step: "after `:WolfSidebarShowProject`",
                causes: "Search: the Ex command did not switch to Files, or its keys went to \
                         the panel's field rather than Neovim, or the tree did not catch up \
                         (raise TREE_SETTLE first). A result beside the tree means both pages \
                         are drawn at once.",
            },
        ],
    );

    let run = GroupRun::decode(SMOKE, &shots);
    run.assert_differs(
        0,
        1,
        "The group is left stale by the accelerator: Ctrl+Shift+B hid the sidebar and the group \
         still shows Files. `SidebarInner::relay` must set the group on every visibility change, \
         hidden included. If every capture's header band is identical, the relay never sets the \
         group at all.",
    );
    run.assert_same(
        1,
        3,
        "hidden",
        "Capture 3 hid from Search and capture 1 from Files.",
    );
    run.assert_same(
        2,
        4,
        "on Search",
        "Capture 2 follows Ctrl+Shift+F, which shows the sidebar and then changes the page: if \
         capture 2 looks like capture 0, the page change never reached the group, so the \
         stack's notify relays the refresh gates but not the group.",
    );
    run.assert_same(
        0,
        5,
        "on Files",
        "Capture 5 follows `:WolfSidebarShowProject`, a page change with the sidebar shown.",
    );
    run.assert_differs(
        2,
        1,
        "No toggle is active while hidden, so the group shows a stale one.",
    );
    run.assert_differs(
        0,
        2,
        "Files and Search look the same, so the page never reaches it.",
    );
    run.assert_orientation(0, 2, 1);
}
