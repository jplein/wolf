//! New Tab smokes (`specs/application-state.md` §6 and §7): New Tab from a folder tab opens the
//! empty tab New Window opens, in the same window, and a folder opened into that tab takes its
//! sidebar from the folder's project state, else from the sidebar its origin showed at New Tab.
//!
//! Every smoke launches on a folder tab at the temp cwd `project`, whose sidebar lists
//! [`TG_TREE_FILE`]'s row, opens the second tab (or, in S5, window), and then opens `D`, a
//! sibling of `project` seeded in the folder MRU, through its Open Recent palette row. Each polls
//! a `getcwd()` readback until it names `D`, quits with `Ctrl+Shift+Q` and reads the frozen save.
//!
//! The pointer helper's files and every readback live under the runtime dir, never in the cwd:
//! the origin's sidebar lists the cwd, and the helper's file names spell [`NT_OCR_TITLE`].
//!
//! Every smoke pins `gtk-titlebar-style: native`, because the tab clicks aim at the tab bar's
//! own row.

use super::editor::EMPTY_STATE_OCR;
use super::extras::{tab_bar_y, two_tab_x, EXTRAS_HELPERS};
use super::open_folder::{
    assert_open_recent_ran, canonical, edit, make_dir, plant, seed_recent, window_dirs,
};
use super::panes::POINTER_BUTTON_MIDDLE;
use super::recovery::with_nvim_pid_helper;
use super::restore::TWOWIN_NEW_WINDOW_QUERY;
use super::sidebar_toggles::{TG_OCR_TREE_ROW, TG_TREE_FILE};
use super::*;
use wolf_core::app_state::{Frame, Sidebar, Tab, TabContent};
use wolf_core::{PaletteRowKind, PaletteWindow};

/// The title an empty tab and its window draw, lowercased: [`WINDOW_TITLE_FALLBACK`]'s.
///
/// Not in `ASSERTED_OCR_TERMS`: `TEMP_CWD_PREFIX` spells it, so the generic guards would refuse
/// it. [`the_new_tab_fixtures_cannot_draw_their_terms`] guards what these smokes put on screen.
pub(crate) const NT_OCR_TITLE: &str = "wolf";

/// S1's `D`. Also its palette query.
pub(crate) const NT_FILL_DIR: &str = "ntjasper";

/// S2's `D`. Also its palette query.
pub(crate) const NT_HIDDEN_DIR: &str = "ntquince";

/// S3's `D`. Also its palette query.
pub(crate) const NT_OUTLIVE_DIR: &str = "ntzinnia";

/// S4's `D`, whose project state hides the sidebar. Also its palette query.
pub(crate) const NT_STORED_DIR: &str = "ntjuniper";

/// S5's `D`, opened into New Window's empty tab. Also its palette query.
pub(crate) const NT_WINDOW_DIR: &str = "ntkumquat";

/// The file inside every `D`, and S4's planted pane.
pub(crate) const NT_D_FILE: &str = "ntleaf.md";

/// [`NT_D_FILE`]'s content.
pub(crate) const NT_D_HEADING: &str = "# NTLEAF";

/// The size of S4's planted windows.
const NT_PLANTED_SIZE: (f64, f64) = (900.0, 640.0);

/// The exit codes [`NEW_TAB_BODY`] adds to [`POINTER_PREAMBLE`]'s.
const NT_EXITS: std::ops::RangeInclusive<i32> = 21..=28;

/// Body of every New Tab smoke, appended to [`POINTER_PREAMBLE`], [`EXTRAS_HELPERS`] and
/// `recovery::with_nvim_pid_helper`'s helper. The steps a smoke does not take are switched off
/// by an empty or `0` variable.
///
/// 1. `$NT_SHOT_P`: capture P, the origin with its sidebar.
/// 2. `$NT_HIDE_ORIGIN`: `Ctrl+Shift+B` in the origin.
/// 3. `Ctrl+Shift+T`, or with `$NT_NEW_WINDOW` the palette's New Window row; two Neovims.
/// 4. `$NT_SHOT_A`: capture A of the new tab, readback R1 from it, a click on the origin's bar
///    entry, capture B, and a click back on the new tab's.
/// 5. `$NT_SHOT_C`: a middle-click on the origin's bar entry, one Neovim, and capture C.
/// 6. `$NT_QUERY`'s Open Recent row, a `getcwd()` readback polled for `$NT_WANT_BASE`, the
///    quit and the freeze.
///
/// No Ex command is typed before a capture in the tab it shows: the typed line, which the
/// cmdline strip draws, holds the runtime dir's path, and that spells [`NT_OCR_TITLE`].
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15 and 40
///
/// | code | meaning |
/// | --- | --- |
/// | `21` | the launch did not run exactly one Neovim |
/// | `22` | New Tab or New Window started no second Neovim |
/// | `23` | readback R1 never appeared: no Ex command reached the new tab's Neovim |
/// | `24` | the middle-click on the origin's bar entry did not leave exactly one Neovim |
/// | `25` | no readback ever appeared after the Open Recent row |
/// | `26` | the app was still alive after `Ctrl+Shift+Q` |
/// | `27` | the app exited non-zero |
/// | `28` | the app left no `wolf.sqlite` |
const NEW_TAB_BODY: &str = r#"
# The tree's FileSearch streams the cwd listing in; capture P reads a tree row.
TREE_SETTLE=1.5
# Readback attempts after the Open Recent row, and the polls each one waits for its file.
READ_TRIES=15
READ_FILE_TRIES=25

read_once() {
    t=0
    while [ "$t" -lt "$READ_FILE_TRIES" ]; do
        if [ -s "$NT_READ_D" ]; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    return 1
}

wait_nvim_count 1 21 "at launch, with one folder tab"
sleep "$TREE_SETTLE"

if [ -n "$NT_SHOT_P" ]; then
    grim "$NT_SHOT_P"
fi

if [ "$NT_HIDE_ORIGIN" = 1 ]; then
    wtype -M ctrl -M shift -k b -m shift -m ctrl
    sleep "$SETTLE"
fi

if [ "$NT_NEW_WINDOW" = 1 ]; then
    wtype -M ctrl -M shift -k p -m shift -m ctrl
    sleep "$SETTLE"
    wtype -- "$NT_NEW_WINDOW_QUERY"
    sleep "$SETTLE"
    wtype -k Return
else
    wtype -M ctrl -M shift -k t -m shift -m ctrl
fi
wait_nvim_count 2 22 "after New Tab or New Window"
sleep "$TABS_BAR_SETTLE"

if [ -n "$NT_SHOT_A" ]; then
    grim "$NT_SHOT_A"
    nvim_ex "call writefile([getcwd()], '$NT_READ_R1')"
    wait_file "$NT_READ_R1" || exit 23
    click_tab "$TAB0_X" "$TAB_Y"
    sleep "$SETTLE"
    grim "$NT_SHOT_B"
    click_tab "$TAB1_X" "$TAB_Y"
fi

if [ -n "$NT_SHOT_C" ]; then
    hsend motion "$TAB0_X" "$TAB_Y"
    hsend press "$BTN_MIDDLE"
    hsend release "$BTN_MIDDLE"
    hsend motion "$PROBE_X" "$PROBE_Y"
    sleep "$TABS_BAR_SETTLE"
    wait_nvim_count 1 24 "after the middle-click closed the origin tab"
    sleep "$SETTLE"
    grim "$NT_SHOT_C"
fi

wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$NT_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$SETTLE"
sleep "$SETTLE"

n=0
while :; do
    rm -f "$NT_READ_D"
    wtype -- ":call writefile([getcwd()], '$NT_READ_D')"
    wtype -k Return
    if read_once; then
        got=$(sed -n 1p "$NT_READ_D")
        if [ "${got##*/}" = "$NT_WANT_BASE" ]; then
            break
        fi
        echo "wolf-gnome new-tab smoke: readback $n: getcwd() = $got" >&2
    fi
    n=$((n + 1))
    if [ "$n" -ge "$READ_TRIES" ]; then
        break
    fi
    sleep "$SETTLE"
done
if [ ! -s "$NT_READ_D" ]; then
    echo "wolf-gnome new-tab smoke: no readback ever appeared at $NT_READ_D." >&2
    exit 25
fi

wtype -M ctrl -M shift -k q -m shift -m ctrl
reap_app 26 27
freeze_or_exit "$NT_STATE_COPY" 28

hsend quit
exit 0
"#;

/// What one run of [`NEW_TAB_BODY`] is given. Every path is under the runtime dir.
struct NewTabRun<'a> {
    smoke: &'a str,
    /// Hide the origin's sidebar before the second tab or window opens.
    hide_origin: bool,
    /// Open the palette's New Window row rather than press `Ctrl+Shift+T`.
    new_window: bool,
    /// S1's captures P, A and B, and its readback R1.
    switch: Option<SwitchEvidence<'a>>,
    /// S3's capture C, after the middle-click closes the origin.
    shot_c: Option<&'a Path>,
    /// `D`, canonical.
    target: &'a str,
    /// `D`'s basename, which is also its palette query.
    query: &'a str,
}

/// S1's evidence paths.
struct SwitchEvidence<'a> {
    shot_p: &'a Path,
    shot_a: &'a Path,
    shot_b: &'a Path,
    read_r1: &'a Path,
}

/// Writes the sidebar fillers and [`TG_TREE_FILE`] into `cwd`, so the origin's sidebar draws a
/// row only it can draw.
fn write_new_tab_fixtures(smoke: &str, cwd: &Path) {
    write_relaunch_fixtures(smoke, cwd, &[]);
    let tree = cwd.join(TG_TREE_FILE);
    std::fs::write(&tree, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
        panic!(
            "{smoke}: failed to write the tree-row fixture {}: {e}",
            tree.display()
        )
    });
}

/// Creates `D` = `<HOME>/<name>` holding [`NT_D_FILE`], and returns its canonical path.
fn make_target(smoke: &str, cwd: &Path, name: &str) -> String {
    let target = make_dir(smoke, smoke_home(cwd), name);
    let file = Path::new(&target).join(NT_D_FILE);
    std::fs::write(&file, format!("{NT_D_HEADING}\n"))
        .unwrap_or_else(|e| panic!("{smoke}: failed to write {}: {e}", file.display()));
    target
}

/// Runs [`NEW_TAB_BODY`] in `runtime_dir` and returns the frozen copy of the save, after
/// asserting the scenario ran to completion and the readback named `D`.
fn run_new_tab(run: &NewTabRun<'_>, cwd: &Path, runtime_dir: &CageRuntime<'_>) -> PathBuf {
    let smoke = run.smoke;
    let rt: &Path = runtime_dir;
    let read_d = rt.join("wolf_gnome_new_tab_d.txt");
    let out_png = rt.join("wolf_gnome_new_tab_unused.png");
    clear_evidence(&read_d, smoke);
    let frozen = frozen_state_dir(rt, "wolf_gnome_new_tab_state", smoke);
    let state = smoke_state_dir(cwd);
    let path_or_empty = |path: Option<&Path>| path.map_or_else(PathBuf::new, Path::to_path_buf);
    let flag = |on: bool| if on { "1" } else { "0" };
    let (tab0_x, tab1_x, tab_y) = (two_tab_x(0), two_tab_x(1), tab_bar_y());

    let body = with_nvim_pid_helper(&format!("{EXTRAS_HELPERS}\n{NEW_TAB_BODY}"));
    let mut cmd = pointer_command(&body, cwd, runtime_dir, &out_png);
    // In the cwd the origin's sidebar would list the helper's files, whose names spell the title.
    cmd.env("CMD_FIFO", rt.join(POINTER_CMD_FIFO_NAME))
        .env("ACK_FILE", rt.join(POINTER_ACK_NAME))
        .env("HELPER_ERR", pointer_err_path(rt))
        .env("READY_FILE", pointer_ready_path(rt))
        .env(
            "WOLF_CONFIG_DIR",
            titlebar_pinned_config_dir(cwd, GTK_TITLEBAR_NATIVE),
        )
        .env("BTN_MIDDLE", POINTER_BUTTON_MIDDLE)
        .env("TAB0_X", tab0_x.to_string())
        .env("TAB1_X", tab1_x.to_string())
        .env("TAB_Y", tab_y.to_string())
        .env("STATE_SRC", &state)
        .env("NT_STATE_COPY", &frozen)
        .env("NT_HIDE_ORIGIN", flag(run.hide_origin))
        .env("NT_NEW_WINDOW", flag(run.new_window))
        .env("NT_NEW_WINDOW_QUERY", TWOWIN_NEW_WINDOW_QUERY)
        .env(
            "NT_SHOT_P",
            path_or_empty(run.switch.as_ref().map(|s| s.shot_p)),
        )
        .env(
            "NT_SHOT_A",
            path_or_empty(run.switch.as_ref().map(|s| s.shot_a)),
        )
        .env(
            "NT_SHOT_B",
            path_or_empty(run.switch.as_ref().map(|s| s.shot_b)),
        )
        .env(
            "NT_READ_R1",
            path_or_empty(run.switch.as_ref().map(|s| s.read_r1)),
        )
        .env("NT_SHOT_C", path_or_empty(run.shot_c))
        .env("NT_QUERY", run.query)
        .env("NT_WANT_BASE", run.query)
        .env("NT_READ_D", &read_d);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless New Tab scenario");

    assert!(
        !status.code().is_some_and(|code| NT_EXITS.contains(&code)),
        "{smoke}: the cage New Tab scenario exited with one of its own codes ({status}). The \
         cage log in the runtime dir carries the scenario's own lines.\n\
         Exit 21: the launch did not run exactly one Neovim, so nothing below starts from one \
         folder tab.\n\
         Exit 22: no second Neovim after Ctrl+Shift+T or the New Window row: AppShell::new_tab \
         opened no tab, or the new tab's WolfCore::start failed.\n\
         Exit 23: readback R1 never appeared, so no Ex command reached the new tab's Neovim.\n\
         Exit 24: the middle-click on the origin's bar entry did not leave one Neovim: the \
         close-page veto went unanswered, or the click missed the bar.\n\
         Exit 25: no readback appeared after the Open Recent row, so no Ex command reached a \
         Neovim after the activation.\n\
         Exit 26 or 27: the quit hung or was not clean.\n\
         Exit 28: the app left no wolf.sqlite in {}.",
        state.display()
    );
    assert_pointer_scenario_succeeded(smoke, status, rt, &out_png);
    let extents = read_output_extents(rt, smoke);
    for (label, point) in [
        ("the origin's bar entry", (tab0_x, tab_y)),
        ("the new tab's bar entry", (tab1_x, tab_y)),
    ] {
        assert_inside_extents(smoke, label, point, extents);
    }

    let got = read_trimmed(&read_d, smoke);
    assert_eq!(
        got, run.target,
        "{smoke}: precondition: after the Open Recent row the focused tab's Neovim reports \
         getcwd() = {got:?}, not D. The readback is polled for up to READ_TRIES attempts, so \
         this is not an early read: the row ran no Open Folder, or it opened D somewhere the keys \
         do not reach, and nothing below is about a folder opened into the empty tab"
    );
    assert_open_recent_ran(smoke, &state, run.target);
    frozen
}

/// D's saved tab in `saved`, panicking when no tab or more than one is on `target`.
fn saved_target_tab<'s>(smoke: &str, saved: &'s WindowState, target: &str) -> &'s Tab {
    let tabs: Vec<&Tab> = saved
        .windows
        .iter()
        .flat_map(|window| window.tabs.iter())
        .filter(|tab| tab.directory == target)
        .collect();
    assert_eq!(
        tabs.len(),
        1,
        "{smoke}: precondition: the save holds {} tab(s) on D ({target}); want exactly one. The \
         windows were {:?}",
        tabs.len(),
        window_dirs(saved)
    );
    tabs[0]
}

/// The origin's saved tab in `saved`: the one on `project`.
fn saved_origin_tab<'s>(smoke: &str, saved: &'s WindowState, project: &str) -> &'s Tab {
    saved
        .windows
        .iter()
        .flat_map(|window| window.tabs.iter())
        .find(|tab| tab.directory == project)
        .unwrap_or_else(|| {
            panic!(
                "{smoke}: precondition: the save holds no tab on the origin's directory {project}; \
                 the windows were {:?}. The origin was closed or replaced",
                window_dirs(saved)
            )
        })
}

/// **New Tab from a folder tab opens an empty tab beside it, and a folder with no project state
/// opened into that tab shows the sidebar the origin showed.** §7 and §6.
///
/// The origin is the launch's folder tab on `project`, its sidebar shown. Capture P shows its
/// sidebar's [`TG_TREE_FILE`] row. `Ctrl+Shift+T` opens the new tab, selected: capture A, then
/// readback R1 from its Neovim. A click on the origin's bar entry selects it for capture B, whose
/// header and origin's bar entry draw `~/project`, so only the new tab's title can draw
/// [`NT_OCR_TITLE`]. A click back on the new tab, then [`NT_FILL_DIR`]'s Open
/// Recent row.
///
/// Premise, measured by the green run: the tab bar's title text OCR-reads as [`NT_OCR_TITLE`] at
/// the smoke font, as the header bar's does in `sidebar.rs`'s title smoke.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: capture P holds [`TG_OCR_TREE_ROW`] | none | n/a |
/// | NE1a | R1 is `HOME`, and its final component is not [`TEMP_CWD_BASENAME`] | in `AppShell::new_tab`, set the planned tab's `cwd` to `selected.launch_cwd()` | R1 ends in `project` |
/// | NE4a | capture A holds [`EMPTY_STATE_OCR`] | in `AppShell::new_tab`, open the tab with `empty: false` | `folder` absent |
/// | NE5a | capture A lacks [`TG_OCR_TREE_ROW`] | in `state_seam::sidebar_is_shown`, return `user_wants` | the row is drawn |
/// | NE3a | capture B holds [`NT_OCR_TITLE`] | in `TabHandle::refresh_label`, pass `false` as `tab_label`'s `empty` | `wolf` absent (`~`) |
/// | — | precondition: the save holds one window, the origin's tab and `D`'s | none | n/a |
/// | NE6a | `D`'s saved tab has `sidebar.visible == true` | in `AppShell::new_tab`, capture `Some(false)` | `false` |
#[test]
fn new_tab_opens_an_empty_tab_beside_its_folder_tab_under_cage() {
    const SMOKE: &str = "new_tab_opens_an_empty_tab_beside_its_folder_tab_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    write_new_tab_fixtures(SMOKE, &cwd);
    let project = canonical(SMOKE, &cwd);
    let home = canonical(SMOKE, smoke_home(&cwd));
    let target = make_target(SMOKE, &cwd, NT_FILL_DIR);
    seed_recent(SMOKE, &smoke_state_dir(&cwd), &target);

    let shot = |name: &str| runtime_dir.join(format!("wolf_gnome_new_tab_{name}.png"));
    let (shot_p, shot_a, shot_b) = (shot("p"), shot("a"), shot("b"));
    let read_r1 = runtime_dir.join("wolf_gnome_new_tab_r1.txt");
    for path in [&shot_p, &shot_a, &shot_b, &read_r1] {
        clear_evidence(path, SMOKE);
    }

    let run = NewTabRun {
        smoke: SMOKE,
        hide_origin: false,
        new_window: false,
        switch: Some(SwitchEvidence {
            shot_p: &shot_p,
            shot_a: &shot_a,
            shot_b: &shot_b,
            read_r1: &read_r1,
        }),
        shot_c: None,
        target: &target,
        query: NT_FILL_DIR,
    };
    let frozen = run_new_tab(&run, &cwd, &runtime_dir);
    for path in [&shot_p, &shot_a, &shot_b] {
        assert_png_present(SMOKE, path);
    }

    let page_p = ocr_to_lowercase(&shot_p);
    assert!(
        page_p.contains(TG_OCR_TREE_ROW),
        "{SMOKE}: precondition: capture P, the origin before New Tab, lacks its sidebar's \
         {TG_OCR_TREE_ROW:?} row, so capture A's lack of it would prove nothing. The sidebar was \
         hidden at launch, or the tree had not listed the cwd within TREE_SETTLE. PNG kept at \
         {}. OCR output was:\n{page_p}",
        shot_p.display()
    );

    let r1 = read_trimmed(&read_r1, SMOKE);
    let r1_tail = r1.rsplit('/').next().unwrap_or_default();
    assert!(
        r1_tail != TEMP_CWD_BASENAME && r1 == home,
        "{SMOKE}: the new tab's Neovim reports getcwd() = {r1:?}; want HOME, {home:?}, as an \
         empty tab's Neovim starts. A final component of {TEMP_CWD_BASENAME:?} means New Tab \
         still roots the tab at the origin's directory; another path means it started elsewhere"
    );

    let page_a = ocr_to_lowercase(&shot_a);
    assert!(
        page_a.contains(EMPTY_STATE_OCR),
        "{SMOKE}: capture A, the new tab, lacks {EMPTY_STATE_OCR:?}, the word all three of the \
         empty state's strings share, so the new tab is not in the empty state: New Tab opened a \
         folder tab. PNG kept at {}. OCR output was:\n{page_a}",
        shot_a.display()
    );
    assert!(
        !page_a.contains(TG_OCR_TREE_ROW),
        "{SMOKE}: capture A, the new empty tab, draws the origin's sidebar row \
         {TG_OCR_TREE_ROW:?}, which capture P showed. An empty tab shows no sidebar, so the \
         empty state no longer overrides the tab's sidebar preference. PNG kept at {}. OCR \
         output was:\n{page_a}",
        shot_a.display()
    );

    let page_b = ocr_to_lowercase(&shot_b);
    assert!(
        page_b.contains(NT_OCR_TITLE),
        "{SMOKE}: capture B, the origin selected beside the empty tab, lacks {NT_OCR_TITLE:?}. \
         Its header and its own bar entry draw ~/project, so only the empty tab's bar entry can \
         draw it: that entry is titled by its Neovim's cwd, `~`, rather than \
         {WINDOW_TITLE_FALLBACK:?}, so tab_label was not told the tab is empty. If the dump \
         shows a misread of the title instead, that is the premise in this smoke's doc. \
         PNG kept at {}. OCR output was:\n{page_b}",
        shot_b.display()
    );

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    let dirs = window_dirs(&saved);
    assert!(
        dirs.len() == 1 && dirs[0].len() == 2 && dirs[0].contains(&project),
        "{SMOKE}: precondition: the save holds the windows {dirs:?}; want one window holding the \
         origin's tab on {project} and D's. Two windows means New Tab or the fill opened a \
         window; an empty directory means the fill left the empty tab in place"
    );
    assert!(
        saved_origin_tab(SMOKE, &saved, &project)
            .content
            .sidebar
            .visible,
        "{SMOKE}: precondition: the origin's tab saved its sidebar hidden, though nothing hid it, \
         so the value New Tab captured is not the shown one this smoke needs"
    );
    let tab = saved_target_tab(SMOKE, &saved, &target);
    assert!(
        tab.content.sidebar.visible,
        "{SMOKE}: D has no project state and the origin's sidebar showed when New Tab ran, so \
         D's tab must show its sidebar; it saved it hidden. New Tab captured hidden, or the fill \
         read something other than the captured value"
    );
}

/// **A folder with no project state opened into New Tab's tab hides the sidebar when the
/// origin's was hidden.** §6.
///
/// `Ctrl+Shift+B` hides the origin's sidebar, `Ctrl+Shift+T` opens the empty tab, and
/// [`NT_HIDDEN_DIR`]'s Open Recent row fills it.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: the origin's saved tab has its sidebar hidden | none | n/a |
/// | NE7a | `D`'s saved tab has `sidebar.visible == false` | in `Coordinator::open_folder`'s `FillCurrent` arm, pass `None` to `open_folder::plan` | `true` |
#[test]
fn a_folder_opened_into_the_new_tab_hides_the_sidebar_the_origin_hid_under_cage() {
    const SMOKE: &str =
        "a_folder_opened_into_the_new_tab_hides_the_sidebar_the_origin_hid_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    write_new_tab_fixtures(SMOKE, &cwd);
    let project = canonical(SMOKE, &cwd);
    let target = make_target(SMOKE, &cwd, NT_HIDDEN_DIR);
    seed_recent(SMOKE, &smoke_state_dir(&cwd), &target);

    let run = NewTabRun {
        smoke: SMOKE,
        hide_origin: true,
        new_window: false,
        switch: None,
        shot_c: None,
        target: &target,
        query: NT_HIDDEN_DIR,
    };
    let frozen = run_new_tab(&run, &cwd, &runtime_dir);

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    assert!(
        !saved_origin_tab(SMOKE, &saved, &project)
            .content
            .sidebar
            .visible,
        "{SMOKE}: precondition: the origin's tab saved its sidebar shown, so Ctrl+Shift+B did \
         not hide it and New Tab had a shown sidebar to capture"
    );
    let tab = saved_target_tab(SMOKE, &saved, &target);
    assert!(
        !tab.content.sidebar.visible,
        "{SMOKE}: the origin's sidebar was hidden when New Tab ran and D has no project state, \
         so D's tab must hide its sidebar; it saved it shown. The fill planned the fresh tab's \
         default rather than the value New Tab captured"
    );
}

/// **The sidebar New Tab captured outlives the origin tab.** §6.
///
/// `Ctrl+Shift+B` hides the origin's sidebar and `Ctrl+Shift+T` opens the empty tab. A
/// middle-click on the origin's bar entry closes it, leaving one page, so the tab bar takes no
/// height: capture C's header is the only thing that can draw [`NT_OCR_TITLE`]. Then
/// [`NT_OUTLIVE_DIR`]'s Open Recent row fills the empty tab, now its window's only one.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: capture C holds [`EMPTY_STATE_OCR`] | none | n/a |
/// | NE2a | capture C holds [`NT_OCR_TITLE`] | in `AppShell::on_render_wake`, hand `refresh_title` the live cwd without the empty-tab filter | header `~`, `wolf` absent |
/// | NE8a | the save holds one window whose one tab is `D`, with `sidebar.visible == false` | in the `FillCurrent` arm, pass the captured value only while the invoking window holds more than one tab | `true` |
#[test]
fn the_captured_sidebar_outlives_the_origin_tab_under_cage() {
    const SMOKE: &str = "the_captured_sidebar_outlives_the_origin_tab_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    write_new_tab_fixtures(SMOKE, &cwd);
    let target = make_target(SMOKE, &cwd, NT_OUTLIVE_DIR);
    seed_recent(SMOKE, &smoke_state_dir(&cwd), &target);

    let shot_c = runtime_dir.join("wolf_gnome_new_tab_c.png");
    clear_evidence(&shot_c, SMOKE);
    let run = NewTabRun {
        smoke: SMOKE,
        hide_origin: true,
        new_window: false,
        switch: None,
        shot_c: Some(&shot_c),
        target: &target,
        query: NT_OUTLIVE_DIR,
    };
    let frozen = run_new_tab(&run, &cwd, &runtime_dir);
    assert_png_present(SMOKE, &shot_c);

    let page_c = ocr_to_lowercase(&shot_c);
    assert!(
        page_c.contains(EMPTY_STATE_OCR),
        "{SMOKE}: precondition: capture C lacks {EMPTY_STATE_OCR:?}, so the tab left after the \
         origin closed is not the empty one: the middle-click closed the new tab instead. PNG \
         kept at {}. OCR output was:\n{page_c}",
        shot_c.display()
    );
    assert!(
        page_c.contains(NT_OCR_TITLE),
        "{SMOKE}: capture C, the empty tab alone in its window, lacks {NT_OCR_TITLE:?}. With one \
         page the tab bar is hidden, so only the header can draw it: the window of an empty tab \
         is titled by its Neovim's cwd, `~`, rather than {WINDOW_TITLE_FALLBACK:?}. PNG kept at \
         {}. OCR output was:\n{page_c}",
        shot_c.display()
    );

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    assert_eq!(
        window_dirs(&saved),
        vec![vec![target.clone()]],
        "{SMOKE}: precondition: the save must hold one window with one tab, on D. The origin's \
         directory means the middle-click closed no tab; two windows means the fill opened one"
    );
    let tab = saved_target_tab(SMOKE, &saved, &target);
    assert!(
        !tab.content.sidebar.visible,
        "{SMOKE}: the origin's sidebar was hidden when New Tab ran, and the origin closed before \
         D opened, so D's tab must still hide its sidebar; it saved it shown. The captured value \
         was lost with the origin, or read from the window rather than from the empty tab"
    );
}

/// **A folder's stored sidebar setting wins over the one New Tab captured.** §6.
///
/// `D` is [`NT_STORED_DIR`], whose project state, planted with `open_folder.rs`'s
/// two-submission method, is one pane on [`NT_D_FILE`] with its sidebar hidden. The launch
/// restores the origin's folder tab with its sidebar shown, `Ctrl+Shift+T` opens the empty
/// tab, and `D`'s Open Recent row fills it.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: the origin's saved tab has its sidebar shown | none | n/a |
/// | NE9a | the readback names `D`, and `D`'s saved tab has `sidebar.visible == false` | in `open_folder::plan`, apply `new_tab_sidebar` over the project's saved tab too | `true` |
#[test]
fn a_stored_sidebar_wins_over_the_origins_under_cage() {
    const SMOKE: &str = "a_stored_sidebar_wins_over_the_origins_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    write_new_tab_fixtures(SMOKE, &cwd);
    let project = canonical(SMOKE, &cwd);
    let target = make_target(SMOKE, &cwd, NT_STORED_DIR);
    let pane = edit(&Path::new(&target).join(NT_D_FILE).to_string_lossy());

    let window = |directory: &str, content: TabContent| Window {
        frame: Frame {
            x: None,
            y: None,
            width: NT_PLANTED_SIZE.0,
            height: NT_PLANTED_SIZE.1,
        },
        active_tab: 0,
        minimized: false,
        tabs: vec![Tab {
            directory: directory.to_owned(),
            content,
        }],
    };
    let holding = WindowState {
        windows: vec![window(
            &target,
            TabContent {
                root: Some(pane),
                sidebar: Sidebar {
                    visible: false,
                    ..Sidebar::default()
                },
                ..TabContent::default()
            },
        )],
    };
    let launch = WindowState {
        windows: vec![window(&project, TabContent::default())],
    };
    let state = smoke_state_dir(&cwd);
    plant(SMOKE, &state, &[holding, launch]);
    let stored = AppStateStore::open(&state).project_state(&target);
    assert_eq!(
        stored.as_ref().map(|p| p.tab.sidebar.visible),
        Some(false),
        "{SMOKE}: premise: D's project state must hide the sidebar after a window state without \
         it was submitted; got {stored:?}. None means the write dropped the row, and Open Folder \
         would read no stored setting"
    );
    seed_recent(SMOKE, &state, &target);

    let run = NewTabRun {
        smoke: SMOKE,
        hide_origin: false,
        new_window: false,
        switch: None,
        shot_c: None,
        target: &target,
        query: NT_STORED_DIR,
    };
    let frozen = run_new_tab(&run, &cwd, &runtime_dir);

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    assert!(
        saved_origin_tab(SMOKE, &saved, &project)
            .content
            .sidebar
            .visible,
        "{SMOKE}: precondition: the origin's tab saved its sidebar hidden, so New Tab captured \
         the same value D stores and the assertion below could not tell them apart"
    );
    let tab = saved_target_tab(SMOKE, &saved, &target);
    assert!(
        !tab.content.sidebar.visible,
        "{SMOKE}: D's project state hides the sidebar and the origin showed it when New Tab ran, \
         so the stored setting wins and D's tab must hide it; it saved it shown. The captured \
         value was applied over the project's saved tab"
    );
}

/// **New Window's empty tab keeps the default sidebar, whatever its origin showed.** §6 and §7.
///
/// `Ctrl+Shift+B` hides the origin's sidebar, the palette's New Window row opens a window,
/// which holds the keyboard under `cage`, and [`NT_WINDOW_DIR`]'s Open Recent row fills its
/// empty tab.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: the save holds the origin's window and `D`'s, the origin's sidebar hidden | none | n/a |
/// | NE10a | `D`'s saved tab, in the new window, has `sidebar.visible == true` | in `window::tab_parts`, set `new_tab_sidebar: Some(false)` | `false` |
#[test]
fn new_windows_empty_tab_keeps_the_default_sidebar_under_cage() {
    const SMOKE: &str = "new_windows_empty_tab_keeps_the_default_sidebar_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    write_new_tab_fixtures(SMOKE, &cwd);
    let project = canonical(SMOKE, &cwd);
    let target = make_target(SMOKE, &cwd, NT_WINDOW_DIR);
    seed_recent(SMOKE, &smoke_state_dir(&cwd), &target);

    let run = NewTabRun {
        smoke: SMOKE,
        hide_origin: true,
        new_window: true,
        switch: None,
        shot_c: None,
        target: &target,
        query: NT_WINDOW_DIR,
    };
    let frozen = run_new_tab(&run, &cwd, &runtime_dir);

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    let mut dirs = window_dirs(&saved);
    dirs.sort();
    let mut want = vec![vec![project.clone()], vec![target.clone()]];
    want.sort();
    assert_eq!(
        dirs, want,
        "{SMOKE}: precondition: the save must hold two windows of one tab each, the origin's and \
         D's. One window means New Window opened a tab, or the fill replaced the origin"
    );
    assert!(
        !saved_origin_tab(SMOKE, &saved, &project)
            .content
            .sidebar
            .visible,
        "{SMOKE}: precondition: the origin's tab saved its sidebar shown, so Ctrl+Shift+B did \
         not hide it and the assertion below could not tell a captured value from the default"
    );
    let tab = saved_target_tab(SMOKE, &saved, &target);
    assert!(
        tab.content.sidebar.visible,
        "{SMOKE}: D has no project state and was opened into New Window's empty tab, which New \
         Tab did not make, so D's tab keeps the default and shows its sidebar; it saved it \
         hidden. New Window's tab carries a captured value, or a fill reads one from elsewhere"
    );
}

/// The terms the New Tab smokes assert on cannot be drawn by what they put on screen: the
/// header's `~/project`, the origin's sidebar rows and bar title, and `D`'s names, which a
/// sidebar rooted at `HOME` would list. Compositor-free.
#[test]
fn the_new_tab_fixtures_cannot_draw_their_terms() {
    assert_eq!(
        NT_OCR_TITLE,
        WINDOW_TITLE_FALLBACK.to_lowercase(),
        "NT_OCR_TITLE must be the empty tab's title, lowercased as the OCR output is"
    );
    let mut on_page: Vec<(&str, &str)> = vec![
        (TEMP_CWD_BASENAME, "TEMP_CWD_BASENAME"),
        (TG_TREE_FILE, "TG_TREE_FILE"),
        (NT_FILL_DIR, "NT_FILL_DIR"),
        (NT_HIDDEN_DIR, "NT_HIDDEN_DIR"),
        (NT_OUTLIVE_DIR, "NT_OUTLIVE_DIR"),
        (NT_STORED_DIR, "NT_STORED_DIR"),
        (NT_WINDOW_DIR, "NT_WINDOW_DIR"),
        (NT_D_FILE, "NT_D_FILE"),
        (NT_D_HEADING, "NT_D_HEADING"),
    ];
    on_page.extend(SIDEBAR_FILLER_FILES.iter().map(|name| (*name, "a filler")));
    for (text, name) in on_page {
        let lower = text.to_lowercase();
        assert!(
            !lower.contains(NT_OCR_TITLE) && !lower.contains(EMPTY_STATE_OCR),
            "{name} = {text:?} contains {NT_OCR_TITLE:?} or {EMPTY_STATE_OCR:?}. It is page text \
             in a New Tab capture, so it would draw the title or the empty state whether or not \
             New Tab made an empty tab. Rename it"
        );
        assert!(
            text == TG_TREE_FILE || !lower.contains(TG_OCR_TREE_ROW),
            "{name} = {text:?} contains {TG_OCR_TREE_ROW:?}, which capture A must lack. It would \
             draw that row in the empty tab whatever its sidebar does. Rename it"
        );
    }
}

/// Each smoke's palette query selects exactly its Open Recent row, and S5's New Window query
/// selects the New Window command with `D` on the folder MRU. Compositor-free.
#[test]
fn the_new_tab_queries_select_exactly_their_rows() {
    let other = PaletteWindow {
        index: 1,
        cwd: format!("/tmp/nix-shell.abcdef/{TEMP_CWD_PREFIX}1-2-0/{TEMP_CWD_BASENAME}"),
    };
    for query in [
        NT_FILL_DIR,
        NT_HIDDEN_DIR,
        NT_OUTLIVE_DIR,
        NT_STORED_DIR,
        NT_WINDOW_DIR,
    ] {
        let dir = PathBuf::from(format!(
            "/tmp/nix-shell.abcdef/{TEMP_CWD_PREFIX}1-2-0/{query}"
        ));
        let rows = wolf_gnome::palette::palette_rows(
            std::slice::from_ref(&dir),
            std::slice::from_ref(&other),
            &[],
            query,
            usize::MAX,
        );
        let kinds: Vec<(PaletteRowKind, &str)> = rows
            .iter()
            .map(|row| (row.kind, row.display.as_str()))
            .collect();
        assert!(
            rows.len() == 1
                && rows[0].kind == PaletteRowKind::OpenRecent
                && rows[0].path.as_deref() == Some(dir.as_path()),
            "the query {query:?} must yield exactly the Open Recent row for {dir:?}; got \
             {kinds:?}. Another row would make which one Enter runs depend on ranking. Rename \
             that smoke's directory"
        );
    }

    let dir = PathBuf::from(format!(
        "/tmp/nix-shell.abcdef/{TEMP_CWD_PREFIX}1-2-0/{NT_WINDOW_DIR}"
    ));
    let rows = wolf_gnome::palette::palette_rows(
        std::slice::from_ref(&dir),
        &[],
        &[],
        TWOWIN_NEW_WINDOW_QUERY,
        usize::MAX,
    );
    assert!(
        rows.first().and_then(|row| row.command) == Some(wolf_core::Command::NewWindow)
            && rows
                .iter()
                .all(|row| row.kind != PaletteRowKind::OpenRecent),
        "the query {TWOWIN_NEW_WINDOW_QUERY:?} must put New Window first and match no Open \
         Recent row while {dir:?} is on the folder MRU; got {:?}",
        rows.iter().map(|row| &row.display).collect::<Vec<_>>()
    );
}
