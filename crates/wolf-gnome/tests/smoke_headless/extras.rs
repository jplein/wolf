//! The per-tab and per-pane extras of `specs/application-state.md` §2, saved at §4's capture
//! moments and restored at the next launch (§5): an edit pane's cursor and top line, a preview
//! pane's scroll, a tab's focused pane and sidebar page, and a zoomed and focused drawer. A
//! restored top line must also survive the grid resize that lays its pane out.
//!
//! Every observable is a readback the captive Neovim or a drawer's shell writes, the frozen
//! window state, or the sidebar band's raw pixels as a precondition. Nothing is OCRed, so no
//! fixture name can satisfy an assertion; the fixtures are registered in
//! `fixture_guards.rs`'s `RELAUNCH_FIXTURE_NAMES` for the generic loop's sake.
//!
//! The two-launch smokes reuse [`POINTER_PREAMBLE`] for their first launch: its helper outlives
//! that app, and the body quits the app, reaps it and launches the second run itself.
//!
//! The smokes that click a tab, through [`tab_bar_y`] and [`two_tab_x`], pin
//! `gtk-titlebar-style: native`, because those points are on the tab bar's own row.

use wolf_core::app_state::{Drawer, Frame, Sidebar, SidebarMode, Tab, TabContent};

use super::panes::TABS_FIRST_TAB_CENTRE_FRACTION;
use super::pointer::SCROLL_AXIS_VALUE;
use super::sidebar::projsearch_sidebar_band;
use super::sidebar_toggles::{tg_click_y, tg_files_x};
use super::terminal::{drawer_paned_at, pin_terminal_smoke_shell};
use super::zoom_pane::{assert_zoom_query_is_unique, ZOOM_QUERY};
use super::*;

/// The fraction of the output width at which the second of two tabs has its centre: two pages
/// split the bar evenly, as [`TABS_FIRST_TAB_CENTRE_FRACTION`] records for the first.
pub(crate) const SECOND_TAB_CENTRE_FRACTION: f64 = 0.75;

/// How long planting a window state may take to flush before the smoke gives up on it.
const PLANT_FLUSH: Duration = Duration::from_secs(5);

/// The tab bar's centre row, with two or more tabs open under `gtk-titlebar-style: native`.
pub(crate) fn tab_bar_y() -> i64 {
    pixel_target(
        OVERLAY_ORIGIN_Y_EXPECTED + TAB_BAR_HEIGHT_EXPECTED / 2.0,
        "the tab bar's centre y",
    )
}

/// The centre x of the first (`0`) or second (`1`) of two tabs.
pub(crate) fn two_tab_x(index: usize) -> i64 {
    let fraction = [TABS_FIRST_TAB_CENTRE_FRACTION, SECOND_TAB_CENTRE_FRACTION][index];
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "the expected cage output width");
    pixel_target(output_w * fraction, "a tab's centre x")
}

/// The canonical form of the smoke's working directory, as a saved tab's `directory` is.
pub(crate) fn canonical_project(smoke: &str, cwd: &Path) -> (PathBuf, String) {
    let project = std::fs::canonicalize(cwd)
        .unwrap_or_else(|e| panic!("{smoke}: failed to canonicalize {}: {e}", cwd.display()));
    let text = project
        .to_str()
        .unwrap_or_else(|| panic!("{smoke}: the temp cwd {} is not UTF-8", cwd.display()))
        .to_owned();
    (project, text)
}

/// One window of `tabs` at 800 by 600 with no position, selecting `active_tab`.
pub(crate) fn planted_window(active_tab: usize, tabs: Vec<Tab>) -> Window {
    Window {
        frame: Frame {
            x: None,
            y: None,
            width: 800.0,
            height: 600.0,
        },
        active_tab,
        tabs,
        minimized: false,
    }
}

/// Writes `planted` into the smoke's state directory, as
/// `three_windows_restore_in_their_stacking_order_under_cage` does, and checks it reads back.
pub(crate) fn plant_window_state(smoke: &str, cwd: &Path, planted: &WindowState) {
    let state = smoke_state_dir(cwd);
    {
        let store = AppStateStore::open(&state);
        let recency = planted
            .windows
            .iter()
            .map(|window| (0..window.tabs.len()).collect())
            .collect();
        store.submit(planted.clone(), recency);
        assert!(
            store.flush(PLANT_FLUSH),
            "{smoke}: planting the window state in {} did not flush within {PLANT_FLUSH:?}, so \
             the launch would restore whatever the file holds",
            state.display()
        );
    }
    assert_eq!(
        AppStateStore::open(&state).window_state().as_ref(),
        Some(planted),
        "{smoke}: premise: the planted window state must read back as planted, or the launch \
         restores something other than what the assertions compare with"
    );
}

/// Ends the run when any of `paths` is missing, so the leg that needs them is never asserted
/// on evidence the scenario did not reach. `codes` names what each exit code means.
pub(crate) fn require(smoke: &str, status: ExitStatus, paths: &[&PathBuf], leg: &str, codes: &str) {
    let missing: Vec<String> = paths
        .iter()
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "{smoke}: leg {leg}'s evidence {missing:?} does not exist, and the scenario exited \
         {status}. {codes} An exit 0 with a file missing means it was written somewhere else."
    );
}

/// The file name, cursor and top line of every pane under the tab's root, depth-first.
fn edit_views(tab: &Tab) -> Vec<(Option<String>, Option<Cursor>, Option<u32>)> {
    tab.content
        .root
        .as_ref()
        .map(|root| {
            root.panes()
                .into_iter()
                .map(|pane| {
                    let fields = pane_fields(pane);
                    let tail = Path::new(fields.path)
                        .file_name()
                        .and_then(OsStr::to_str)
                        .map(str::to_owned);
                    (tail, fields.cursor, fields.top_line)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The one window of a frozen state, panicking with `what` when there is not exactly one.
fn only_window<'a>(state: &'a WindowState, smoke: &str, what: &str, file: &Path) -> &'a Window {
    assert_eq!(
        state.windows.len(),
        1,
        "{smoke}: {what} holds {} windows, want the one this smoke opens. File: {}",
        state.windows.len(),
        file.display()
    );
    &state.windows[0]
}

/// The shell helpers every extras body uses, placed after [`POINTER_PREAMBLE`] or a body's own
/// prelude: Ex commands, the window chord, a tab click, the quit's reap and a freeze.
///
/// `reap_app` polls for the app to go after its quit and reaps it, exiting with its first
/// argument when the app outlives `QUIT_TRIES` polls and with its second when it exits
/// non-zero. `freeze_or_exit` copies `$STATE_SRC` into its first argument through the prelude's
/// `freeze_state`, exiting with its second when the app left no database there.
pub(crate) const EXTRAS_HELPERS: &str = r#"
# The adw::TabBar animates its height in over about a second; see POINTER_TABS_BODY.
TABS_BAR_SETTLE=2.0
# How many WAIT_INTERVAL polls an app gets to exit after its quit.
QUIT_TRIES=75
# Each restored tab's Neovim rebuilds its layout on its own first wake; see restore.rs.
RESTORE_SETTLE=2.0

nvim_ex() {
    wtype -- ":$1"
    wtype -k Return
    sleep "$SETTLE"
}

chord_w() {
    wtype -M ctrl -k w -m ctrl
    wtype -k "$1"
    sleep "$SETTLE"
}

# $1 and $2 are a point on the tab bar.
click_tab() {
    hsend motion "$1" "$2"
    hsend press "$BTN_LEFT"
    hsend release "$BTN_LEFT"
    hsend motion "$PROBE_X" "$PROBE_Y"
    sleep "$SETTLE"
}

reap_app() {
    _reap_n=0
    while kill -0 "$app_pid" 2>/dev/null; do
        _reap_n=$((_reap_n + 1))
        if [ "$_reap_n" -ge "$QUIT_TRIES" ]; then
            echo "wolf-gnome extras smoke: the app (pid $app_pid) is still alive after its quit." >&2
            exit "$1"
        fi
        sleep "$WAIT_INTERVAL"
    done
    _reap_rc=0
    wait "$app_pid" || _reap_rc=$?
    if [ "$_reap_rc" -ne 0 ]; then
        echo "wolf-gnome extras smoke: the app (pid $app_pid) exited $_reap_rc, not 0." >&2
        exit "$2"
    fi
    app_pid=
}

freeze_or_exit() {
    if ! freeze_state "$STATE_SRC" "$1"; then
        echo "wolf-gnome extras smoke: no wolf.sqlite in $STATE_SRC to freeze into $1." >&2
        exit "$2"
    fi
}
"#;

// --- S1: cursors captured at a focus change, a tab switch and the quit ---------------------

/// The file S1 opens first, in the left pane: 200 numbered lines, as are the other two.
pub(crate) const CV_A_FILE: &str = "cva.md";

/// The file S1 opens in the pane `<C-w>v` splits off to the right.
pub(crate) const CV_B_FILE: &str = "cvb.md";

/// The file S1 opens in the second tab.
pub(crate) const CV_C_FILE: &str = "cvc.md";

/// How many lines each S1 fixture holds: more than any line the smoke moves to, so no cursor is
/// clamped.
const CV_LINES: usize = 200;

/// The body of each S1 fixture: [`CV_LINES`] numbered lines.
fn cv_body() -> String {
    (1..=CV_LINES)
        .map(|n| format!("{n:03} cursor fixture line"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The inclusive band of exit codes [`CV_BODY`] adds to [`POINTER_PREAMBLE`]'s 11-15 and 40.
const CV_EXITS: std::ops::RangeInclusive<i32> = 110..=119;

/// What each of [`CV_BODY`]'s exit codes means, for failure messages.
const CV_CODES: &str = "Exit 110: Neovim did not answer in cva.md before M1. 111: no \
     wolf.sqlite to freeze at M1. 112: no new tab's Neovim answered after Ctrl+Shift+T. 113: no \
     wolf.sqlite at M2. 114: run 1 outlived `:WolfQuitApplication`. 115: run 1 exited non-zero. \
     116: no wolf.sqlite after the quit. 117: run 2 died before it was ready. 118: run 2's \
     selected tab did not answer. 119: no Neovim answered after the click on tab 0.";

/// Body of [`pane_cursors_are_captured_and_restored_under_cage`], appended to
/// [`POINTER_PREAMBLE`] and [`EXTRAS_HELPERS`].
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// | code | meaning |
/// | --- | --- |
/// | `110` | Neovim did not answer in `cva.md` before M1 |
/// | `111` | no `wolf.sqlite` to freeze at M1 |
/// | `112` | no new tab's Neovim answered after Ctrl+Shift+T |
/// | `113` | no `wolf.sqlite` to freeze at M2 |
/// | `114` | run 1 was still alive after `:WolfQuitApplication` |
/// | `115` | run 1 exited non-zero |
/// | `116` | no `wolf.sqlite` to freeze after the quit |
/// | `117` | run 2 died before it was ready |
/// | `118` | run 2's selected tab did not answer |
/// | `119` | no Neovim answered after the click on tab 0 |
const CV_BODY: &str = r#"
# --- M1: a split, each focus move capturing the pane it leaves -------------------------
nvim_ex "e $CV_A"
chord_w v
nvim_ex "e $CV_B"
nvim_ex "30"
chord_w h
nvim_ex "call cursor(120, 4)"
nvim_ex "normal! zt"
wtype -- ":call writefile([printf('%s %d %d %d %d', expand('%:t'), line('.'), col('.'), line('w0'), getpid())], '$READ_M1')"
wtype -k Return
wait_file "$READ_M1" || exit 110
sleep "$SETTLE"
freeze_or_exit "$STATE_M1" 111

# --- M2: a new tab, which switches away from tab 0 -----------------------------------------
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
# New Tab's tab is empty, its Neovim at HOME; the relative :cd makes it a folder tab.
nvim_ex "cd $CV_PROJECT_BASE"
wtype -- ":call writefile([printf('%d %d', getpid(), winnr('\$'))], '$READ_M2')"
wtype -k Return
wait_file "$READ_M2" || exit 112
sleep "$SETTLE"
freeze_or_exit "$STATE_M2" 113

# --- Q: tab 1 on line 40, then the quit, through Coordinator::quit ----------------------
nvim_ex "e $CV_C"
nvim_ex "40"
wtype -- ":WolfQuitApplication"
wtype -k Return
reap_app 114 115
freeze_or_exit "$STATE_Q" 116

# --- RUN 2: the saved active tab 1, then a click on tab 0 ----------------------------------
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 117 "wolf-gnome cursor extras smoke, run 2"
sleep "$RESTORE_SETTLE"
wtype -- ":call writefile([printf('%s %d %d', expand('%:t'), line('.'), getpid())], '$READ_R1')"
wtype -k Return
wait_file "$READ_R1" || exit 118
click_tab "$TAB0_X" "$TAB_Y"
wtype -- ":call writefile([printf('%s %d %d %d %d', expand('%:t'), line('.'), col('.'), line('w0'), getpid())] + map(range(1, winnr('\$')), {_, n -> fnamemodify(bufname(winbufnr(n)), ':t') . ' ' . line('.', win_getid(n))}), '$READ_R0')"
wtype -k Return
wait_file "$READ_R0" || exit 119

hsend quit
exit 0
"#;

/// Reads a readback's first line as `<file> <ints...>`, with `ints` integers after the name.
fn read_named(path: &Path, ints: usize, smoke: &str) -> (String, Vec<i64>) {
    let raw = read_readback(path, smoke);
    let line = raw.lines().next().unwrap_or_default();
    let mut tokens = line.split_whitespace();
    let name = tokens.next().unwrap_or_default().to_owned();
    let values: Vec<i64> = tokens
        .map(|token| parse_readback_int(token, path, &raw, smoke))
        .collect();
    assert_eq!(
        values.len(),
        ints,
        "{smoke}: the readback {} reads {raw:?}; want a file name and {ints} integers",
        path.display()
    );
    (name, values)
}

/// **An edit pane's cursor and top line are captured when focus leaves it, when its tab is
/// switched away from and at quit, and the next launch restores them with each tab's focused
/// pane.**
///
/// ## The scenario
///
/// Two launches in one cage, the first through [`POINTER_PREAMBLE`]. Run 1 opens
/// [`CV_A_FILE`], splits it with `<C-w>v` (focus moves to the new right pane, capturing
/// `cva` at 1:0), opens [`CV_B_FILE`] there and goes to line 30, then `<C-w>h` back (capturing
/// `cvb` at 30:0). In `cva` it moves the cursor to line 120, byte column 3, with `zt`, reads
/// that back, and freezes M1. Ctrl+Shift+T switches away from tab 0 to an empty tab, a `:cd`
/// makes it a folder tab on the project, and M2 is frozen. In the
/// new tab it opens [`CV_C_FILE`] on line 40, quits with `:WolfQuitApplication`, which is
/// `Coordinator::quit` (`:qa` is the close path's), and freezes Q.
///
/// Run 2 starts on tab 1, the tab selected at the quit, and reads its line back, then clicks
/// tab 0 and reads its current buffer, line, column and first visible line, with every
/// window's buffer and line.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: M1's readback is `cva.md` at 120, column 4, `w0` 120 | none | n/a |
/// | E16a | M1: `cvb` saved at `{30, 0}`, and `cva` at `{1, 0}`, not where its cursor is | in `set_active_pane`, delete the capture of the previous pane | M1: `cvb`'s cursor `None` |
/// | — | precondition: M2's readback is another Neovim, with one window | none | n/a |
/// | E16b | M2: `cva` saved at `{120, 3}` with top line 120 | in `on_tab_selected`, delete the previous tab's `capture_viewports` | M2: `cva` `{1, 0}`, top 1 |
/// | E16f | the same | the edit capture stores `top_line: 1` | M2: top 1 |
/// | E16c | Q: `cvc` saved at `{40, 0}`, and tab 1 the active tab | in `Coordinator::quit`, delete the capture loop | Q: `cvc`'s cursor `None` |
/// | E16e | Q: tab 0 is `vertical[cva, cvb]`, focused on pane 0 | the gather writes `focused: None` | Q: focused `None` |
/// | E16g | run 2: tab 1 on line 40; tab 0 on `cva.md` at 120, column 4, `w0` 120, and `cvb`'s window on line 30 | `PaneHandle::request_layout_rebuild` stores the restored tab with every edit pane's `cursor` and `top_line` `None` | run 2: `cva` on line 1 |
///
/// M1's `cva` at `{1, 0}` is what says the capture is moment-based: the cursor moved to 120
/// with no focus change, so a continuous capture would have saved `{120, 3}` there. Clamping
/// a cursor beyond the end of a file is wolf-core's `app_state_rebuild.rs`'s question.
///
/// ## Premises
///
/// - `<C-w>v` opens the new pane on the right and focuses it, as `restore.rs`'s layout smoke
///   records.
/// - Neovim leaves the cursor of a freshly opened file at line 1, byte column 0: no Neovim
///   default restores a last position from shada.
/// - `cursor(120, 4)` puts the cursor on byte column 3, which `nvim_win_get_cursor` reports
///   0-based, and `zt` scrolls line 120 to the top of a window over a 200-line file.
#[test]
fn pane_cursors_are_captured_and_restored_under_cage() {
    const SMOKE: &str = "pane_cursors_are_captured_and_restored_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let body = cv_body();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (CV_A_FILE, body.as_str()),
            (CV_B_FILE, body.as_str()),
            (CV_C_FILE, body.as_str()),
        ],
    );

    let read = |name: &str| runtime_dir.join(format!("wolf_gnome_cv_{name}.txt"));
    let read_m1 = read("m1");
    let read_m2 = read("m2");
    let read_r1 = read("r1");
    let read_r0 = read("r0");
    let out_png = runtime_dir.join("wolf_gnome_cv_smoke.png");
    for path in [&read_m1, &read_m2, &read_r1, &read_r0, &out_png] {
        clear_evidence(path, SMOKE);
    }
    let state_m1 = frozen_state_dir(&runtime_dir, "wolf_gnome_cv_m1_state", SMOKE);
    let state_m2 = frozen_state_dir(&runtime_dir, "wolf_gnome_cv_m2_state", SMOKE);
    let state_q = frozen_state_dir(&runtime_dir, "wolf_gnome_cv_q_state", SMOKE);

    let tab0_x = two_tab_x(0);
    let tab_y = tab_bar_y();
    let body = format!("{EXTRAS_HELPERS}\n{CV_BODY}");
    let status = pointer_command(&body, &cwd, &runtime_dir, &out_png)
        .env(
            "WOLF_CONFIG_DIR",
            titlebar_pinned_config_dir(&cwd, GTK_TITLEBAR_NATIVE),
        )
        .env("CV_A", CV_A_FILE)
        .env("CV_B", CV_B_FILE)
        .env("CV_C", CV_C_FILE)
        .env("CV_PROJECT_BASE", TEMP_CWD_BASENAME)
        .env("TAB0_X", tab0_x.to_string())
        .env("TAB_Y", tab_y.to_string())
        .env("STATE_SRC", smoke_state_dir(&cwd))
        .env("STATE_M1", &state_m1)
        .env("STATE_M2", &state_m2)
        .env("STATE_Q", &state_q)
        .env("READ_M1", &read_m1)
        .env("READ_M2", &read_m2)
        .env("READ_R1", &read_r1)
        .env("READ_R0", &read_r0)
        .status()
        .expect("failed to spawn `cage` for the headless cursor extras scenario");
    if !status.code().is_some_and(|code| CV_EXITS.contains(&code)) {
        assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    }
    let extents = read_output_extents(&cwd, SMOKE);
    assert_inside_extents(SMOKE, "tab 0's centre", (tab0_x, tab_y), extents);

    // --- Precondition: Neovim's own cursor in cva before M1 ---------------------------------
    require(SMOKE, status, &[&read_m1], "M1's readback", CV_CODES);
    let (m1_file, m1) = read_named(&read_m1, 4, SMOKE);
    assert_eq!(
        (m1_file.as_str(), m1[0], m1[1], m1[2]),
        (CV_A_FILE, 120, 4, 120),
        "{SMOKE}: before M1 Neovim reports (buffer, line, col, w0) = ({m1_file}, {}, {}, {}); \
         want {CV_A_FILE} at line 120, column 4, scrolled to 120. Another buffer means `<C-w>h` \
         did not return to the left pane; another line or w0 means `cursor()` or `zt` did not \
         run, and nothing below could tell a lost capture from a cursor that never moved.",
        m1[0],
        m1[1],
        m1[2]
    );

    // --- E16a: the focus moves captured the panes they left -----------------------------
    require(SMOKE, status, &[&state_m1], "M1", CV_CODES);
    let saved = frozen_window_state(&state_m1, SMOKE, "M1, frozen after the focus moves");
    let window = only_window(&saved, SMOKE, "M1", &state_m1);
    let views = edit_views(&window.tabs[0]);
    assert_eq!(
        views.get(1).map(|view| (view.0.as_deref(), view.1)),
        Some((Some(CV_B_FILE), Some(Cursor { line: 30, col: 0 }))),
        "{SMOKE}: M1 saved tab 0's panes as {views:?}; want {CV_B_FILE} second, at {{30, 0}}, \
         captured when `<C-w>h` moved focus off it. A cursor of None means a focus move captures \
         nothing; a pane count other than two means the split was not saved. File: {}",
        state_m1.display()
    );
    assert_eq!(
        views.first().map(|view| view.1),
        Some(Some(Cursor { line: 1, col: 0 })),
        "{SMOKE}: M1 saved {CV_A_FILE} as {:?}; want {{1, 0}}, captured when `<C-w>v` moved \
         focus off it. {{120, 3}} means the cursor is captured continuously rather than at \
         §4's moments; None means the split's focus move captured nothing. File: {}",
        views.first(),
        state_m1.display()
    );

    // --- E16b and E16f: the switch away from tab 0 captured its panes -------------------
    require(SMOKE, status, &[&read_m2, &state_m2], "M2", CV_CODES);
    let m2 = read_ints(&read_m2, 2, SMOKE);
    assert!(
        m2[0] != m1[3] && m2[1] == 1,
        "{SMOKE}: after Ctrl+Shift+T the answering Neovim is pid {} with {} windows, against tab \
         0's pid {}; want another Neovim with one window. Tab 0's pid means the chord opened no \
         tab, so M2 shows no switch away from tab 0.",
        m2[0],
        m2[1],
        m1[3]
    );
    let saved = frozen_window_state(&state_m2, SMOKE, "M2, frozen after the new tab");
    let window = only_window(&saved, SMOKE, "M2", &state_m2);
    let views = edit_views(&window.tabs[0]);
    assert_eq!(
        views.first(),
        Some(&(
            Some(CV_A_FILE.to_owned()),
            Some(Cursor { line: 120, col: 3 }),
            Some(120)
        )),
        "{SMOKE}: M2 saved tab 0's first pane as {:?}; want {CV_A_FILE} at {{120, 3}} with top \
         line 120, captured when its tab was switched away from. {{1, 0}} with top line 1 means \
         the tab switch captures nothing; the right cursor with top line 1 means the capture \
         does not read the window's first visible line. File: {}",
        views.first(),
        state_m2.display()
    );

    // --- E16c and E16e: the quit captured tab 1, and tab 0 kept its focus -----------------
    require(SMOKE, status, &[&state_q], "Q", CV_CODES);
    let saved = frozen_window_state(&state_q, SMOKE, "Q, frozen after the quit");
    let window = only_window(&saved, SMOKE, "Q", &state_q);
    assert!(
        window.tabs.len() == 2 && window.active_tab == 1,
        "{SMOKE}: Q saved {} tabs with active tab {}; want 2, tab 1 active, the tab the quit \
         came from. File: {}",
        window.tabs.len(),
        window.active_tab,
        state_q.display()
    );
    let views = edit_views(&window.tabs[1]);
    assert_eq!(
        views.first().map(|view| (view.0.as_deref(), view.1)),
        Some((Some(CV_C_FILE), Some(Cursor { line: 40, col: 0 }))),
        "{SMOKE}: Q saved tab 1 as {views:?}; want {CV_C_FILE} at {{40, 0}}. Nothing moved focus \
         off it, so only the quit can have captured it: a cursor of None means \
         `Coordinator::quit` captures nothing before its final write. File: {}",
        state_q.display()
    );
    let tab0 = &window.tabs[0];
    let tails: Vec<Option<String>> = edit_views(tab0).into_iter().map(|view| view.0).collect();
    assert_eq!(
        (tails, tab0.content.focused),
        (
            vec![Some(CV_A_FILE.to_owned()), Some(CV_B_FILE.to_owned())],
            Some(PaneRef::Pane(0))
        ),
        "{SMOKE}: Q saved tab 0 as (panes, focused); want [{CV_A_FILE}, {CV_B_FILE}] focused on \
         pane 0, the pane `<C-w>h` returned to. focused None means the gather writes no focus; \
         Some(Pane(1)) means it writes the last leaf rather than the active pane. File: {}",
        state_q.display()
    );

    // --- E16g: run 2 restores each tab's cursor, top line and focus ---------------------
    require(SMOKE, status, &[&read_r1], "R1", CV_CODES);
    let (r1_file, r1) = read_named(&read_r1, 2, SMOKE);
    assert_eq!(
        (r1_file.as_str(), r1[0]),
        (CV_C_FILE, 40),
        "{SMOKE}: run 2's selected tab reports {r1_file} on line {}; want {CV_C_FILE} on line \
         40. Line 1 means the saved cursor is not applied at the rebuild; another buffer means \
         the saved active tab was not selected.",
        r1[0]
    );
    require(SMOKE, status, &[&read_r0], "R0", CV_CODES);
    let (r0_file, r0) = read_named(&read_r0, 4, SMOKE);
    assert!(
        r0[3] != r1[1],
        "{SMOKE}: after the click on tab 0 the answering Neovim is pid {}, tab 1's; the click \
         selected no other tab, so R0 says nothing about tab 0.",
        r0[3]
    );
    assert_eq!(
        (r0_file.as_str(), r0[0], r0[1], r0[2]),
        (CV_A_FILE, 120, 4, 120),
        "{SMOKE}: run 2's tab 0 reports (buffer, line, col, w0) = ({r0_file}, {}, {}, {}); want \
         {CV_A_FILE} at line 120, column 4, scrolled to 120. {CV_B_FILE} current means the saved \
         focus was not restored and Neovim ended on the last leaf; line 1 means the saved cursor \
         is not applied; the right line with w0 elsewhere means the top line is not.",
        r0[0],
        r0[1],
        r0[2]
    );
    let raw = read_readback(&read_r0, SMOKE);
    let b_line = raw
        .lines()
        .skip(1)
        .find_map(|line| line.strip_prefix(&format!("{CV_B_FILE} ")));
    assert_eq!(
        b_line.map(str::trim),
        Some("30"),
        "{SMOKE}: run 2's tab 0 windows read {raw:?}; want {CV_B_FILE}'s window on line 30, its \
         cursor saved when focus left it. Line 1 means a pane other than the focused one gets no \
         saved cursor; no {CV_B_FILE} window means the layout was not rebuilt."
    );
}

// --- S2: a preview's scroll survives a window close and a relaunch ------------------------

/// The Markdown source S2 previews and scrolls.
pub(crate) const PS_FILE: &str = "psv.md";

/// How many paragraphs [`PS_FILE`] holds: a page several viewports tall, so
/// [`PS_WHEEL_DETENTS`] scroll it well inside its range.
const PS_PARAGRAPHS: usize = 80;

/// How many wheel detents run 1 scrolls the preview by.
const PS_WHEEL_DETENTS: usize = 10;

/// The range run 1's saved scroll must fall in: off both ends of the page, so a scroll that was
/// never applied, or one clamped to an end, is told apart from the one saved.
const PS_SCROLL_RANGE: std::ops::RangeInclusive<f64> = 0.05..=0.95;

/// How many more detents run 2 wheels the restored page by: half of [`PS_WHEEL_DETENTS`], so a
/// seed left in place, a page left at the top and a restored page each give a different reading.
const PS_RESTORE_DETENTS: usize = 5;

/// The body of [`PS_FILE`]: [`PS_PARAGRAPHS`] numbered paragraphs.
fn ps_body() -> String {
    (1..=PS_PARAGRAPHS)
        .map(|n| format!("Paragraph {n:03} of the scroll fixture, a line of ordinary prose."))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The inclusive band of exit codes [`PS_BODY`] adds to [`POINTER_PREAMBLE`]'s 11-15 and 40.
const PS_EXITS: std::ops::RangeInclusive<i32> = 120..=129;

/// What each of [`PS_BODY`]'s exit codes means, for failure messages.
const PS_CODES: &str = "Exit 120: Neovim did not answer after the wheel, so keys went to the \
     web view. 122: run 1 outlived `:WolfCloseWindow`. 123: run 1 exited non-zero. 124: no \
     wolf.sqlite after the close. 125: run 2 died before it was ready. 126: run 2's Neovim did \
     not answer after the wheel. 127: run 2 outlived `:qa`. 128: run 2 exited non-zero. 129: \
     no wolf.sqlite after run 2's quit.";

/// Body of [`preview_scroll_survives_a_close_and_relaunch_under_cage`], appended to
/// [`POINTER_PREAMBLE`] and [`EXTRAS_HELPERS`].
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// | code | meaning |
/// | --- | --- |
/// | `120` | Neovim did not answer after the wheel |
/// | `122` | run 1 was still alive after `:WolfCloseWindow` |
/// | `123` | run 1 exited non-zero |
/// | `124` | no `wolf.sqlite` to freeze after the close |
/// | `125` | run 2 died before it was ready |
/// | `126` | run 2's Neovim did not answer after the wheel |
/// | `127` | run 2 was still alive after `:qa` |
/// | `128` | run 2 exited non-zero |
/// | `129` | no `wolf.sqlite` to freeze after run 2's quit |
const PS_BODY: &str = r#"
# A cold preview open, and the page's own layout; see PREVIEW_SETTLE in preview.rs.
PREVIEW_SETTLE=3.0
# The wheel's scroll, smooth or not, and the page's report of it.
SCROLL_SETTLE=2.0
# Run 2's lazily built view, its load, and the saved scroll's application on load. It must
# precede the wheel: a load that finishes after it puts the page back at the saved scroll.
PREVIEW_RESTORE_SETTLE=4.0

# --- C: preview the source in place, wheel it down, then close the window ---------------
nvim_ex "e $PS_SOURCE"
nvim_ex "WolfPreview"
sleep "$PREVIEW_SETTLE"
hsend motion "$PROBE_X" "$PROBE_Y"
detents=0
while [ "$detents" -lt "$PS_DETENTS" ]; do
    hstep axis "$PS_AXIS_VALUE" 1
    detents=$((detents + 1))
done
sleep "$SCROLL_SETTLE"
wtype -- ":call writefile([bufname('%')], '$READ_C')"
wtype -k Return
wait_file "$READ_C" || exit 120
wtype -- ":WolfCloseWindow"
wtype -k Return
reap_app 122 123
freeze_or_exit "$STATE_C" 124

# --- Q: relaunch, let the page load and scroll, wheel it further, then :qa --------------
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 125 "wolf-gnome preview scroll smoke, run 2"
sleep "$PREVIEW_RESTORE_SETTLE"
hsend motion "$PROBE_X" "$PROBE_Y"
detents=0
while [ "$detents" -lt "$PS_RESTORE_DETENTS" ]; do
    hstep axis "$PS_AXIS_VALUE" 1
    detents=$((detents + 1))
done
sleep "$SCROLL_SETTLE"
wtype -- ":call writefile([bufname('%')], '$READ_Q')"
wtype -k Return
wait_file "$READ_Q" || exit 126
wtype -- ":qa"
wtype -k Return
reap_app 127 128
freeze_or_exit "$STATE_Q" 129

hsend quit
exit 0
"#;

/// The scroll of the lone preview pane of `file` a frozen state saved, after checking its shape.
pub(crate) fn saved_preview_scroll(
    smoke: &str,
    frozen: &Path,
    what: &str,
    file: &str,
) -> Option<f64> {
    let saved = frozen_window_state(frozen, smoke, what);
    let window = only_window(&saved, smoke, what, frozen);
    let root = window
        .tabs
        .first()
        .and_then(|tab| tab.content.root.as_ref());
    let fields = root.and_then(leaf_fields);
    assert!(
        fields.as_ref().is_some_and(
            |leaf| leaf.preview && Path::new(leaf.path).file_name() == Some(OsStr::new(file))
        ),
        "{smoke}: {what} saved the root {root:?}; want a lone preview pane of {file}. An edit \
         pane means `:WolfPreview` bound no preview, so no scroll was there to save. File: {}",
        frozen.display()
    );
    fields.and_then(|leaf| leaf.scroll)
}

/// **A preview pane's scroll is reported by its page, captured when its window closes, and
/// applied once the page has rendered at the next launch.**
///
/// ## The scenario
///
/// Two launches in one cage, the first through [`POINTER_PREAMBLE`]. Run 1 opens [`PS_FILE`],
/// previews it in place with `:WolfPreview`, wheels the page down [`PS_WHEEL_DETENTS`]
/// detents at the probe point, reads Neovim's buffer name back so the keys still reach
/// Neovim, and closes the window with `:WolfCloseWindow`. That is the close path's
/// `KeepSubtree` arm, the last window, and C is frozen. Run 2 waits while the restored preview
/// loads and scrolls, wheels it [`PS_RESTORE_DETENTS`] more detents at the probe point, reads
/// the buffer name back so the keys still reach Neovim after the wheel, quits with `:qa`, the
/// same arm, and Q is frozen.
///
/// Q is checked against run 1's per-detent fraction `δ = s1 / PS_WHEEL_DETENTS`: it must lie
/// within `PS_RESTORE_DETENTS / 2 · δ` of `s1 + PS_RESTORE_DETENTS · δ`. A seed that was never
/// applied keeps Q at s1, and a page left at the top gives `PS_RESTORE_DETENTS · δ`.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | E17a | C: a lone preview of [`PS_FILE`] with a scroll in [`PS_SCROLL_RANGE`] | in `sync_preview_view`, register no script message handler | C: scroll `None` |
/// | E17b | the same | in `on_window_closing`'s `KeepSubtree` arm, delete the capture | C: scroll `None` |
/// | E17c | Q: the scroll within half of run 2's wheel of C's plus that wheel | in `on_preview_load_finished`, take `pending_scroll` and evaluate no script | Q: about `PS_RESTORE_DETENTS · δ`, below the band |
/// | E17d | the same | in `on_preview_load_finished`, return straight after the fragment branch | Q: s1, below the band |
/// | E17e | the same | in `on_preview_load_finished`, evaluate `scroll_to_fraction_js(fraction * 2.0)` | Q: about `2·s1 + PS_RESTORE_DETENTS · δ`, above the band |
///
/// ## Premises, measured by this smoke and by nothing else
///
/// - A cage wheel event over the pane scrolls the `WebView`, and [`PS_WHEEL_DETENTS`] detents
///   of [`SCROLL_AXIS_VALUE`] leave the page inside [`PS_SCROLL_RANGE`]. The range is an
///   estimate from the fixture's length, not a measurement: a red at C with a scroll near 0
///   or 1 is this premise first.
/// - A `load_html` document with a `file://` base reaches a registered script message
///   handler, and the page reports its scroll at `load` and on every `scroll` event.
/// - Run 2's window has run 1's size, so one fraction is one position on both pages.
/// - Run 2's wheel scrolls by run 1's per-detent fraction, since the window size, the page and
///   the probe point are the same.
/// - `PREVIEW_RESTORE_SETTLE` covers the restored page's load and its scroll's application, so
///   run 2's wheel starts from the saved scroll.
#[test]
fn preview_scroll_survives_a_close_and_relaunch_under_cage() {
    const SMOKE: &str = "preview_scroll_survives_a_close_and_relaunch_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let body = ps_body();
    write_relaunch_fixtures(SMOKE, &cwd, &[(PS_FILE, body.as_str())]);

    let read_c = runtime_dir.join("wolf_gnome_ps_c.txt");
    let read_q = runtime_dir.join("wolf_gnome_ps_q.txt");
    let out_png = runtime_dir.join("wolf_gnome_ps_smoke.png");
    for path in [&read_c, &read_q, &out_png] {
        clear_evidence(path, SMOKE);
    }
    let state_c = frozen_state_dir(&runtime_dir, "wolf_gnome_ps_c_state", SMOKE);
    let state_q = frozen_state_dir(&runtime_dir, "wolf_gnome_ps_q_state", SMOKE);

    let body = format!("{EXTRAS_HELPERS}\n{PS_BODY}");
    let status = pointer_command(&body, &cwd, &runtime_dir, &out_png)
        .env("PS_SOURCE", PS_FILE)
        .env("PS_DETENTS", PS_WHEEL_DETENTS.to_string())
        .env("PS_RESTORE_DETENTS", PS_RESTORE_DETENTS.to_string())
        .env("PS_AXIS_VALUE", SCROLL_AXIS_VALUE)
        .env("STATE_SRC", smoke_state_dir(&cwd))
        .env("STATE_C", &state_c)
        .env("STATE_Q", &state_q)
        .env("READ_C", &read_c)
        .env("READ_Q", &read_q)
        .status()
        .expect("failed to spawn `cage` for the headless preview scroll scenario");
    if !status.code().is_some_and(|code| PS_EXITS.contains(&code)) {
        assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    }

    // --- E17a and E17b: the close saved the reported scroll ----------------------------
    require(SMOKE, status, &[&read_c, &state_c], "C", PS_CODES);
    let s1 = saved_preview_scroll(
        SMOKE,
        &state_c,
        "C, frozen after the window closed",
        PS_FILE,
    );
    assert!(
        s1.is_some_and(|s| PS_SCROLL_RANGE.contains(&s)),
        "{SMOKE}: C saved the preview's scroll as {s1:?}; want a fraction in \
         {PS_SCROLL_RANGE:?}. None means no scroll was captured at the close: the page's report \
         never reached the pane, as with no script message handler registered, or the close \
         path captures nothing. A value near 0 or 1 means the wheel did not scroll the page as \
         this smoke assumes: read its premises first. File: {}",
        state_c.display()
    );
    let s1 = s1.unwrap_or_default();

    // --- E17c, E17d and E17e: the relaunch applied the saved scroll, and run 2 wheeled on ---
    let per_detent = s1 / PS_WHEEL_DETENTS as f64;
    let restore_wheel = PS_RESTORE_DETENTS as f64 * per_detent;
    let want = s1 + restore_wheel;
    let slack = restore_wheel / 2.0;
    assert!(
        PS_SCROLL_RANGE.contains(&want),
        "{SMOKE}: precondition: run 1's {s1} plus run 2's wheel of {PS_RESTORE_DETENTS} detents \
         at {per_detent} each is {want}, outside {PS_SCROLL_RANGE:?}, so the wheel may reach the \
         page's end and Q could not tell a restored scroll from a clamped one. Lengthen \
         {PS_FILE} or wheel fewer detents."
    );
    require(SMOKE, status, &[&read_q, &state_q], "Q", PS_CODES);
    let s2 = saved_preview_scroll(SMOKE, &state_q, "Q, frozen after run 2's quit", PS_FILE);
    assert!(
        s2.is_some_and(|s| (s - want).abs() <= slack),
        "{SMOKE}: run 2 saved the preview's scroll as {s2:?}; want run 1's {s1} plus run 2's \
         wheel, {want}, within {slack}. About {s1} means the saved scroll was never applied and \
         every capture kept the seed (`pending_scroll` never taken), or run 2's wheel did not \
         reach the page. About {restore_wheel} means the page loaded at the top and was wheeled \
         from there: the saved scroll was not applied. Well above {want} means the restore \
         overshot the saved scroll. None means run 2's page reported nothing and the seed was \
         lost. File: {}",
        state_q.display()
    );
}

// --- S3: each tab keeps its own sidebar page ----------------------------------------------

/// The query planted in tab 0's sidebar.
const SB_QUERY_ZERO: &str = "sbpalfa";

/// The query planted in tab 1's sidebar.
const SB_QUERY_ONE: &str = "sbpbravo";

/// The fewest sidebar-band pixels the Files toggle's click must change: the Search page's field
/// and toggles against the tree's rows cover far more, and a stray antialiased pixel far less.
const SB_MIN_CHANGED_PIXELS: usize = 40;

/// The inclusive band of exit codes [`SB_BODY`] adds to [`POINTER_PREAMBLE`]'s 11-15 and 40.
const SB_EXITS: std::ops::RangeInclusive<i32> = 131..=133;

/// Body of [`each_tab_keeps_its_sidebar_page_under_cage`], appended to [`POINTER_PREAMBLE`] and
/// [`EXTRAS_HELPERS`].
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// | code | meaning |
/// | --- | --- |
/// | `131` | the app was still alive after `:qa` |
/// | `132` | the app exited non-zero |
/// | `133` | no `wolf.sqlite` to freeze after the quit |
const SB_BODY: &str = r#"
# The tree's FileSearch streams the cwd listing in once its page shows.
TREE_SETTLE=1.5

sleep "$TREE_SETTLE"
grim "$SB_SHOT_0"

# The Files toggle: a page change in GTK alone, which wakes no Neovim.
hsend motion "$SB_FILES_X" "$SB_TOGGLE_Y"
hsend press "$BTN_LEFT"
hsend release "$BTN_LEFT"
hsend motion "$PROBE_X" "$PROBE_Y"
sleep "$TREE_SETTLE"
grim "$SB_SHOT_1"

click_tab "$TAB1_X" "$TAB_Y"
wtype -- ":qa"
wtype -k Return
reap_app 131 132
freeze_or_exit "$STATE_Q" 133

hsend quit
exit 0
"#;

/// A saved tab on `project` with a lone empty edit pane and a visible Search sidebar holding
/// `query`.
fn search_tab(project: &str, query: &str) -> Tab {
    Tab {
        directory: project.to_owned(),
        content: TabContent {
            root: Some(Node::Pane(Pane::Edit {
                path: String::new(),
                cursor: None,
                top_line: None,
            })),
            sidebar: Sidebar {
                visible: true,
                mode: Some(SidebarMode::Search),
                query: Some(query.to_owned()),
                case_sensitive: Some(false),
                use_regex: Some(true),
                all_files: Some(false),
            },
            ..TabContent::default()
        },
    }
}

/// **Each tab of a window records its own sidebar page: a tab switched away from keeps the
/// page it showed, and the tab switched to shows its own.**
///
/// ## The scenario
///
/// One launch through [`POINTER_PREAMBLE`], over a planted window of two tabs on the project,
/// both on their Search page, tab 0 selected. After the launch, a click on the header bar's
/// Files toggle shows tab 0's Files page, with a capture of the sidebar band either side. A
/// click on tab 1 selects it, `:qa` quits through the close path, and Q is frozen.
///
/// The page changes by the toggle rather than by `:WolfSidebarShowProject`: a typed command
/// drains on a render wake, and the record of the selected tab's page on later wakes could then
/// stand in for the record the tab switch makes. A toggle click wakes no Neovim, so only the
/// switch can record tab 0's Files page.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: the click changed at least [`SB_MIN_CHANGED_PIXELS`] of the sidebar band | none | n/a |
/// | — | precondition: Q's active tab is 1 | none | n/a |
/// | E18a | Q: tab 1 saved with mode `Search` | `apply_selected_tab` shows no page | tab 1 saved `Tree` |
/// | E18b | Q: tab 0 saved with mode `Tree` | `on_tab_selected` records nothing into the previous tab | tab 0 saved `Search` |
#[test]
fn each_tab_keeps_its_sidebar_page_under_cage() {
    const SMOKE: &str = "each_tab_keeps_its_sidebar_page_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);
    let (_, project) = canonical_project(SMOKE, &cwd);
    let planted = WindowState {
        windows: vec![planted_window(
            0,
            vec![
                search_tab(&project, SB_QUERY_ZERO),
                search_tab(&project, SB_QUERY_ONE),
            ],
        )],
    };
    plant_window_state(SMOKE, &cwd, &planted);

    let shot_0 = runtime_dir.join("wolf_gnome_sb_0.png");
    let shot_1 = runtime_dir.join("wolf_gnome_sb_1.png");
    for path in [&shot_0, &shot_1] {
        clear_evidence(path, SMOKE);
    }
    let state_q = frozen_state_dir(&runtime_dir, "wolf_gnome_sb_q_state", SMOKE);

    let files_x = pixel_target(tg_files_x(), "the Files toggle's x");
    let toggle_y = pixel_target(tg_click_y(), "the toggle group's y");
    let tab1_x = two_tab_x(1);
    let tab_y = tab_bar_y();
    let body = format!("{EXTRAS_HELPERS}\n{SB_BODY}");
    let status = pointer_command(&body, &cwd, &runtime_dir, &shot_1)
        .env(
            "WOLF_CONFIG_DIR",
            titlebar_pinned_config_dir(&cwd, GTK_TITLEBAR_NATIVE),
        )
        .env("SB_FILES_X", files_x.to_string())
        .env("SB_TOGGLE_Y", toggle_y.to_string())
        .env("TAB1_X", tab1_x.to_string())
        .env("TAB_Y", tab_y.to_string())
        .env("SB_SHOT_0", &shot_0)
        .env("SB_SHOT_1", &shot_1)
        .env("STATE_SRC", smoke_state_dir(&cwd))
        .env("STATE_Q", &state_q)
        .status()
        .expect("failed to spawn `cage` for the headless sidebar page scenario");
    if !status.code().is_some_and(|code| SB_EXITS.contains(&code)) {
        assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &shot_1);
    }
    let extents = read_output_extents(&cwd, SMOKE);
    assert_inside_extents(SMOKE, "the Files toggle", (files_x, toggle_y), extents);
    assert_inside_extents(SMOKE, "tab 1's centre", (tab1_x, tab_y), extents);

    // --- Precondition: the toggle click changed the sidebar -----------------------------
    let before = decode_capture(&shot_0, SMOKE, "the capture before the Files toggle");
    let after = decode_capture(&shot_1, SMOKE, "the capture after the Files toggle");
    let band = projsearch_sidebar_band();
    let changed = (band.y0..band.y1)
        .flat_map(|y| (band.x0..band.x1).map(move |x| (x, y)))
        .filter(|&(x, y)| before.pixel(x, y) != after.pixel(x, y))
        .count();
    assert!(
        changed >= SB_MIN_CHANGED_PIXELS,
        "{SMOKE}: the click on the Files toggle changed {changed} pixels of the sidebar band, \
         fewer than {SB_MIN_CHANGED_PIXELS}, so no page changed and Q cannot say whether a tab \
         switch records it. The click missed the toggle group: check `tg_files_x` against the \
         captures {} and {}.",
        shot_0.display(),
        shot_1.display()
    );

    // --- E18a and E18b: each tab saved its own page ---------------------------------------
    let q_codes = "Exit 131: the app outlived `:qa`, so the keys after the tab click reached no \
                   Neovim. 132: it exited non-zero. 133: it left no wolf.sqlite.";
    require(SMOKE, status, &[&state_q], "Q", q_codes);
    let saved = frozen_window_state(&state_q, SMOKE, "Q, frozen after the quit");
    let window = only_window(&saved, SMOKE, "Q", &state_q);
    assert!(
        window.tabs.len() == 2 && window.active_tab == 1,
        "{SMOKE}: Q saved {} tabs with active tab {}; want 2 with tab 1 active. Tab 0 active \
         means the click on tab 1 selected nothing, so neither page below is evidence. File: {}",
        window.tabs.len(),
        window.active_tab,
        state_q.display()
    );
    let modes: Vec<Option<SidebarMode>> = window
        .tabs
        .iter()
        .map(|tab| tab.content.sidebar.mode)
        .collect();
    assert_eq!(
        modes[1],
        Some(SidebarMode::Search),
        "{SMOKE}: Q saved tab 1's sidebar mode as {:?}; want Search, its planted page. Tree means \
         the switch to tab 1 left tab 0's Files page on screen, which was then recorded as tab \
         1's: the tab switch does not show the selected tab's page. File: {}",
        modes[1],
        state_q.display()
    );
    assert_eq!(
        modes[0],
        Some(SidebarMode::Tree),
        "{SMOKE}: Q saved tab 0's sidebar mode as {:?}; want Tree, the page the toggle showed. \
         Search means the page tab 0 showed when it was switched away from was not recorded \
         into it, so it kept the page recorded at its last render wake. File: {}",
        modes[0],
        state_q.display()
    );
}

// --- S5: a zoomed and focused drawer restores open, zoomed and focused ---------------------

/// The file S5's lone edit pane holds, under the drawer.
pub(crate) const DZ_FILE: &str = "dzk.md";

/// The share S5 plants: half the pane area, apart from a new window's third.
const DZ_SHARE: f64 = 0.5;

/// How far M's saved share may be from [`DZ_SHARE`].
const DZ_SHARE_EPS: f64 = 1e-9;

/// The scenario of [`a_zoomed_focused_drawer_restores_under_cage`], with [`EXTRAS_HELPERS`]
/// after its own prelude.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | the app died before it was ready for input |
/// | `40` | the app never reported ready |
/// | `134` | no shell answered `pwd` after the launch: no focused drawer |
/// | `135` | no shell answered `stty size` while zoomed |
/// | `136` | no `wolf.sqlite` to freeze at M |
/// | `137` | no shell answered `stty size` after the unzoom |
/// | `138` | the app was still alive after Ctrl+Shift+Q |
/// | `139` | the app exited non-zero |
const DZ_SCENARIO: &str = r#"
set -eu

WAIT_INTERVAL=0.2
SETTLE=1.0
FILE_TRIES=100
# The drawer's resize reaching its shell; see DRAWER_SCENARIO in terminal.rs.
DRAWER_SETTLE=2.0

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
    echo "wolf-gnome zoomed drawer smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}
"#;

/// The steps of [`DZ_SCENARIO`], after [`EXTRAS_HELPERS`].
const DZ_STEPS: &str = r#"
# $1 is the shell command, $2 the file it writes, $3 the exit code. Only a shell writes it.
shell_out() {
    wtype -- "$1 > '$2'"
    wtype -k Return
    wait_file "$2" || exit "$3"
}

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome zoomed drawer smoke"

# --- M: the restored drawer is focused and zoomed ----------------------------------------
shell_out "pwd -P" "$READ_PWD" 134
shell_out "stty size" "$READ_Z" 135
sleep "$SETTLE"
freeze_or_exit "$STATE_M" 136

# --- U: the palette's Zoom Pane unzooms it --------------------------------------------------
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$ZOOM_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$DRAWER_SETTLE"
shell_out "stty size" "$READ_U" 137

wtype -M ctrl -M shift -k q -m shift -m ctrl
reap_app 138 139
exit 0
"#;

/// **A drawer saved open, zoomed and focused restores open, with a fresh shell in the tab's
/// directory, zoomed and focused, and at its saved share once unzoomed.**
///
/// ## The scenario
///
/// One launch through [`cage_command`] over a planted tab: a lone edit pane on [`DZ_FILE`], a
/// drawer open at [`DZ_SHARE`], and `zoomed` and `focused` both `"drawer"`. After the launch,
/// `pwd -P` and `stty size` are typed into the focused pane, which only a drawer's shell
/// answers, and M is frozen. The palette's Zoom Pane unzooms, `stty size` is read again as U,
/// and Ctrl+Shift+Q quits.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | premise: the shell's directory is the canonical project, core's `toggle_terminal` | none | n/a |
/// | E20a | the scenario reaches its readbacks | the restore calls no `toggle_terminal` | exit 134 or 40 |
/// | E20c | M: `zoomed` is `"drawer"` | `layout_snapshot` writes `zoomed` as `leaf_index`, with the drawer read as `None` | M: zoomed `None` |
/// | E20d | M: `focused` is `"drawer"` | the gather's `focused` reads a drawer as `None` | M: focused `None` |
/// | E20e | M: the drawer open at [`DZ_SHARE`] | `plan_for_window` sets `drawer_share: DrawerShare::new()` | M: share 1/3 |
/// | E20b | Z's rows exceed U's | at the drawer's bind, skip the Drawer zoom | Z equals U |
/// | — | U's rows are a [`DZ_SHARE`] drawer's at this geometry | none | n/a |
///
/// U's rows derive through `terminal.rs`'s [`drawer_paned_at`] and `geometry`'s
/// `divider_position` at the editor's fraction `1 - DZ_SHARE`, as `drawer_divider.rs` derives
/// its kept share's rows.
#[test]
fn a_zoomed_focused_drawer_restores_under_cage() {
    const SMOKE: &str = "a_zoomed_focused_drawer_restores_under_cage";

    assert_zoom_query_is_unique(SMOKE);
    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[(DZ_FILE, "# Under the drawer")]);
    let (project, project_str) = canonical_project(SMOKE, &cwd);
    let planted = WindowState {
        windows: vec![planted_window(
            0,
            vec![Tab {
                directory: project_str.clone(),
                content: TabContent {
                    root: Some(Node::Pane(Pane::Edit {
                        path: project.join(DZ_FILE).to_string_lossy().into_owned(),
                        cursor: None,
                        top_line: None,
                    })),
                    focused: Some(PaneRef::Drawer),
                    zoomed: Some(PaneRef::Drawer),
                    drawer: Some(Drawer {
                        open: true,
                        share: DZ_SHARE,
                    }),
                    ..TabContent::default()
                },
            }],
        )],
    };
    plant_window_state(SMOKE, &cwd, &planted);

    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    let paned = drawer_paned_at(output_h);
    let position =
        wolf_gnome::geometry::divider_position(paned.floor, paned.max, paned.span, 1.0 - DZ_SHARE);
    let expected_u_rows = paned.rows(paned.span - paned.handle - position);

    let read = |name: &str| runtime_dir.join(format!("wolf_gnome_dz_{name}.txt"));
    let read_pwd = read("pwd");
    let read_z = read("zoomed");
    let read_u = read("unzoomed");
    for path in [&read_pwd, &read_z, &read_u] {
        clear_evidence(path, SMOKE);
    }
    let state_m = frozen_state_dir(&runtime_dir, "wolf_gnome_dz_m_state", SMOKE);

    let scenario = format!("{DZ_SCENARIO}\n{EXTRAS_HELPERS}\n{DZ_STEPS}");
    let mut cmd = cage_command(&scenario, &cwd, &runtime_dir);
    pin_terminal_smoke_shell(&mut cmd)
        .env("WOLF_BIN", bin)
        .env("ZOOM_QUERY", ZOOM_QUERY)
        .env("STATE_SRC", smoke_state_dir(&cwd))
        .env("STATE_M", &state_m)
        .env("READ_PWD", &read_pwd)
        .env("READ_Z", &read_z)
        .env("READ_U", &read_u);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless zoomed drawer scenario");
    let codes = "Exit 11: the app died before it was ready. 40: it never reported ready, which a \
                 restored drawer that never binds gives. 134: no shell answered after the launch, \
                 so no drawer was open and focused. 135: no shell answered while zoomed. 136: no \
                 wolf.sqlite at M. 137: no shell answered after the palette's Zoom Pane, so \
                 focus left the drawer. 138 or 139: the quit hung or was not clean.";

    // --- Premise: the fresh shell starts in the tab's directory -----------------------
    require(SMOKE, status, &[&read_pwd], "PWD", codes);
    let pwd = read_trimmed(&read_pwd, SMOKE);
    assert_eq!(
        std::fs::canonicalize(&pwd).ok(),
        Some(project.clone()),
        "{SMOKE}: the restored drawer's shell started in {pwd:?}; want the project {}. This is \
         core's `toggle_terminal`, which starts the drawer in the tab's Neovim's directory.",
        project.display()
    );

    // --- E20c, E20d and E20e: the save of the restored drawer --------------------------
    require(SMOKE, status, &[&read_z, &state_m], "M", codes);
    let saved = frozen_window_state(&state_m, SMOKE, "M, frozen with the drawer zoomed");
    let window = only_window(&saved, SMOKE, "M", &state_m);
    let content = &window.tabs[0].content;
    assert_eq!(
        content.zoomed,
        Some(PaneRef::Drawer),
        "{SMOKE}: M saved the tab's zoom as {:?}; want Some(Drawer). None means a zoomed drawer \
         is saved as nothing zoomed. File: {}",
        content.zoomed,
        state_m.display()
    );
    assert_eq!(
        content.focused,
        Some(PaneRef::Drawer),
        "{SMOKE}: M saved the tab's focus as {:?}; want Some(Drawer). None means a focused \
         drawer is saved as no focus; Some(Pane(0)) means focus did not reach the drawer. File: \
         {}",
        content.focused,
        state_m.display()
    );
    assert!(
        content
            .drawer
            .is_some_and(|d| d.open && (d.share - DZ_SHARE).abs() <= DZ_SHARE_EPS),
        "{SMOKE}: M saved the drawer as {:?}; want open with share {DZ_SHARE}. A share of a \
         third means the saved share was not restored into the tab; closed or None means the \
         drawer did not reopen or is not saved. File: {}",
        content.drawer,
        state_m.display()
    );

    // --- E20b: the drawer restored zoomed --------------------------------------------
    require(SMOKE, status, &[&read_u], "U", codes);
    let z = read_ints(&read_z, 2, SMOKE);
    let u = read_ints(&read_u, 2, SMOKE);
    assert!(
        z[0] > u[0],
        "{SMOKE}: the drawer's shell reports {} rows while restored and {} after Zoom Pane; want \
         more while restored, since a zoomed drawer fills the pane area. Equal rows mean the \
         saved drawer zoom was not applied at the drawer's bind, and Zoom Pane then zoomed it \
         rather than unzooming it, or did nothing.",
        z[0],
        u[0]
    );
    assert_eq!(
        u[0], expected_u_rows,
        "{SMOKE}: unzoomed, the drawer's shell reports {} rows; a drawer at a {DZ_SHARE} share \
         of this pane area has {expected_u_rows}. The rows of a third mean the planted share did \
         not reach the restored tab's pane view.",
        u[0]
    );
}

// --- S7: a restored top line survives its pane's grid resize ------------------------------

/// The file of S7's upper, focused pane, planted with a cursor below its top row.
pub(crate) const RV_A_FILE: &str = "rva.md";

/// The file of S7's lower pane, planted with no view.
pub(crate) const RV_B_FILE: &str = "rvb.md";

/// The cursor S7 plants on [`RV_A_FILE`]: line 130, byte column 2, ten rows below
/// [`RV_TOP_LINE`], so the view's top line is not the cursor's own line.
const RV_CURSOR: Cursor = Cursor { line: 130, col: 2 };

/// The top line S7 plants on [`RV_A_FILE`].
const RV_TOP_LINE: u32 = 120;

/// The fewest rows S7's upper pane must have once laid out: well above the roughly 11 rows
/// Neovim's own split of its initial 24 gives it, and enough to show [`RV_CURSOR`] below
/// [`RV_TOP_LINE`].
const RV_MIN_HEIGHT: i64 = 16;

/// The font size S7 launches at, overriding [`SMOKE_FONT_PT`], as `config.rs`'s
/// `FULLWIDTH_SMOKE_FONT_PT` does.
///
/// At [`SMOKE_FONT_PT`] a lone pane is 11 rows, so the upper pane of a 0.75 split could not
/// reach [`RV_MIN_HEIGHT`], nor show the planted view at all. At 10 pt a cell is about 16 px
/// high against 44, so the same pane has more than twice the rows. Nothing here is OCRed, so
/// the small size costs nothing.
const RV_FONT_PT: &str = "10";

/// The scenario of [`a_restored_top_line_survives_its_grid_resize_under_cage`], with
/// [`EXTRAS_HELPERS`] after its own prelude.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `40` | the app never reported ready |
/// | `150` | the app died before it was ready for input |
/// | `151` | Neovim did not answer after the launch |
/// | `152` | the app was still alive after `:qa` |
/// | `153` | the app exited non-zero |
const RV_SCENARIO: &str = r#"
set -eu

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
    echo "wolf-gnome restored view smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}
"#;

/// The steps of [`RV_SCENARIO`], after [`EXTRAS_HELPERS`].
const RV_STEPS: &str = r#"
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 150 "wolf-gnome restored view smoke"
# Lets every grid resize of the launch land, so one that re-scrolls the pane late shows.
sleep "$RESTORE_SETTLE"

wtype -- ":call writefile([printf('%s %d %d %d %d', expand('%:t'), line('w0'), line('.'), col('.'), winheight(0))], '$READ_V')"
wtype -k Return
wait_file "$READ_V" || exit 151

wtype -- ":qa"
wtype -k Return
reap_app 152 153
exit 0
"#;

/// **A restored edit pane's top line survives the grid resize that lays the pane out, when
/// its cursor is not on the top row.**
///
/// ## The scenario
///
/// One launch through [`cage_command`], with no pointer, over a planted tab: a `horizontal`
/// root weighted `[0.75, 0.25]`, the upper pane on [`RV_A_FILE`] at [`RV_CURSOR`] with
/// [`RV_TOP_LINE`], the lower on [`RV_B_FILE`] with no view, and `focused` on pane 0. Both
/// files hold S1's 200 numbered lines. After the launch, and a settle, the current window
/// writes back its buffer, `line('w0')`, `line('.')`, `col('.')` and `winheight(0)`, and `:qa`
/// quits.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: the current buffer is [`RV_A_FILE`], so the saved focus restored | none | n/a |
/// | — | precondition: the pane is at least [`RV_MIN_HEIGHT`] rows | none | n/a |
/// | E23a | `w0` 120, line 130, column 3 | `apply_pending_views` returns at once | `w0` is not 120 |
/// | E23b | the same | `restore_extras` calls `set_window_cursor` for each seeded edit view at once, and sets no `pending_view` | `w0` is not 120 |
///
/// ## Premises
///
/// - The rebuild applies the saved view at Neovim's initial 24 rows, where its own split gives
///   the upper pane about 11. The pane's final height differs from that, and Neovim's default
///   `splitkeep=cursor` re-scrolls a window whose height changes, keeping the cursor's share of
///   the rows rather than the top line. E23a measures this: with no view applied again, `w0`
///   is wherever that re-scroll left it.
/// - The settle after readiness lets every grid resize of the launch land before the readback,
///   so a resize that re-scrolls the pane after its view was applied again shows as a red.
#[test]
fn a_restored_top_line_survives_its_grid_resize_under_cage() {
    const SMOKE: &str = "a_restored_top_line_survives_its_grid_resize_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    let body = cv_body();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[(RV_A_FILE, body.as_str()), (RV_B_FILE, body.as_str())],
    );
    let (project, project_str) = canonical_project(SMOKE, &cwd);
    let edit = |file: &str, cursor: Option<Cursor>, top_line: Option<u32>| {
        Node::Pane(Pane::Edit {
            path: project.join(file).to_string_lossy().into_owned(),
            cursor,
            top_line,
        })
    };
    let planted = WindowState {
        windows: vec![planted_window(
            0,
            vec![Tab {
                directory: project_str,
                content: TabContent {
                    root: Some(Node::Layout(Layout {
                        orientation: PaneOrientation::Horizontal,
                        weights: vec![0.75, 0.25],
                        children: vec![
                            edit(RV_A_FILE, Some(RV_CURSOR), Some(RV_TOP_LINE)),
                            edit(RV_B_FILE, None, None),
                        ],
                    })),
                    focused: Some(PaneRef::Pane(0)),
                    ..TabContent::default()
                },
            }],
        )],
    };
    plant_window_state(SMOKE, &cwd, &planted);

    let read_v = runtime_dir.join("wolf_gnome_rv_view.txt");
    clear_evidence(&read_v, SMOKE);

    let scenario = format!("{RV_SCENARIO}\n{EXTRAS_HELPERS}\n{RV_STEPS}");
    let status = cage_command(&scenario, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        // Later than cage_command's own WOLF_FONT_PT, so this one wins.
        .env("WOLF_FONT_PT", RV_FONT_PT)
        .env("READ_V", &read_v)
        .status()
        .expect("failed to spawn `cage` for the headless restored view scenario");
    let codes = "Exit 40: the app never reported ready. 150: it died before it was ready. 151: \
                 no Neovim answered after the launch. 152 or 153: the quit hung or was not clean.";

    require(SMOKE, status, &[&read_v], "V", codes);
    let (file, v) = read_named(&read_v, 4, SMOKE);
    let (w0, line, col, height) = (v[0], v[1], v[2], v[3]);

    // --- Preconditions: the saved focus, and a pane tall enough for the view ------------
    assert_eq!(
        file, RV_A_FILE,
        "{SMOKE}: the current window shows {file:?} after the launch; want {RV_A_FILE}, the \
         focused pane 0. {RV_B_FILE} means the saved focus was not restored, so the readback \
         below reads the other pane."
    );
    assert!(
        height >= RV_MIN_HEIGHT,
        "{SMOKE}: the upper pane is {height} rows; want at least {RV_MIN_HEIGHT}, so that the \
         saved view fits and the pane's height differs from Neovim's own split of its initial \
         24 rows. Fewer means the font size or the planted weights no longer give the pane \
         that height: lower RV_FONT_PT, never the bar."
    );

    // --- E23a and E23b: the saved view applies again once the grid has settled ---------
    assert_eq!(
        (w0, line, col),
        (
            i64::from(RV_TOP_LINE),
            i64::from(RV_CURSOR.line),
            i64::from(RV_CURSOR.col) + 1
        ),
        "{SMOKE}: the restored pane reports (w0, line, col) = ({w0}, {line}, {col}) at \
         {height} rows; want ({RV_TOP_LINE}, {}, {}). The right line with another w0 means the \
         saved view was applied only at Neovim's initial size, or before the pane's last grid \
         resize, and that resize re-scrolled it. Line 1 means no saved cursor was applied at \
         all.",
        RV_CURSOR.line,
        RV_CURSOR.col + 1
    );
}
