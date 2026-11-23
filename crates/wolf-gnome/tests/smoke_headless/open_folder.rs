//! Open Folder smokes (`specs/application-state.md` §6): an Open Recent palette row for a
//! directory `D`, driven over a state each smoke plants in its own store before the launch.
//!
//! Each focuses an open tab, fills an empty one, or opens a new window, and each reads what it
//! needs without pixels: the save the `Ctrl+Shift+Q` quit leaves, frozen by the scenario, the
//! command MRU, and a `getcwd()` / `winnr('$')` readback the tab that ends on `D` writes. A
//! restored layout is read LIVE through that readback, never from the save alone: a tab
//! contributes its seed until its first gather succeeds, so a save can echo a restore that never
//! applied. The Open Folder dialog itself is not driven, since no portal backend exists under
//! `cage`; the palette row reaches the same `AppShell::open_folder`.
//!
//! The Rust side plants a project state by submitting a window state holding `D`, then one
//! without it, which leaves `D`'s project state and the second window state in the store.

use super::recovery::with_nvim_pid_helper;
use super::*;
use wolf_core::app_state::{Frame, Tab, TabContent};
use wolf_core::{PaletteRowKind, PaletteWindow};

/// How long a planting's flush may take before the smoke gives up on it.
const OF_PLANT_FLUSH: Duration = Duration::from_secs(5);

/// S1's back window's selected tab's directory, a sibling of the launch cwd.
pub(crate) const OF_FOCUS_OTHER_DIR: &str = "ofcanyon";

/// S1's `D`, open in the back window's background tab. Also its palette query.
pub(crate) const OF_FOCUS_TARGET_DIR: &str = "ofjukebox";

/// S2's `D`, whose project state is a two-pane split. Also its palette query.
pub(crate) const OF_FILL_TARGET_DIR: &str = "ofwaltz";

/// S2's left pane file, inside [`OF_FILL_TARGET_DIR`].
pub(crate) const OF_FILL_LEFT_FILE: &str = "ofwleft.md";

/// S2's right pane file, inside [`OF_FILL_TARGET_DIR`].
pub(crate) const OF_FILL_RIGHT_FILE: &str = "ofwright.md";

/// S2's readback, in the launch cwd.
pub(crate) const OF_FILL_READBACK_NAME: &str = "ofw_read.txt";

/// S3's `D`, whose project state is a two-pane split. Also its palette query.
pub(crate) const OF_NEW_TARGET_DIR: &str = "ofquartz";

/// S3's left pane file, inside [`OF_NEW_TARGET_DIR`].
pub(crate) const OF_NEW_LEFT_FILE: &str = "ofqleft.md";

/// S3's right pane file, inside [`OF_NEW_TARGET_DIR`].
pub(crate) const OF_NEW_RIGHT_FILE: &str = "ofqright.md";

/// S3's launch window's one pane, in the launch cwd.
pub(crate) const OF_NEW_BASE_FILE: &str = "ofpbase.md";

/// S3's readback, in the launch cwd.
pub(crate) const OF_NEW_READBACK_NAME: &str = "ofq_read.txt";

/// The Open Folder scenario, run as one `sh -c` string inside `cage` with
/// [`with_nvim_pid_helper`] prepended for `wait_nvim_count`.
///
/// Launch over the planted state, count its Neovims, `Ctrl+Shift+P`, type `$OF_QUERY`, Enter;
/// then, when `$OF_READBACK` is set, poll a `getcwd()` / `winnr('$')` readback from the focused
/// window until it names `$OF_WANT_BASE` with `$OF_WANT_WINS` windows or `READ_TRIES` run out,
/// keeping the last one; then quit with `Ctrl+Shift+Q` and freeze the save.
///
/// The readback is polled rather than read once: the step under test builds a tab whose
/// layout rebuild runs on its first wake, and a key typed before a new window maps reaches the
/// launch window. Under `cage` the newest toplevel holds the keyboard and a `present()` of any
/// other is declined, so a Focus is invisible to the keys and only the save shows it.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | the app died before it was ready for input |
/// | `12` | the app was still alive after `Ctrl+Shift+Q` |
/// | `13` | the app exited non-zero |
/// | `15` | no readback ever appeared: no Ex command reached a Neovim |
/// | `16` | the app left no `wolf.sqlite` |
/// | `21` | the launch did not run `$OF_NVIMS` Neovims, so the planted state did not restore |
const OPENFOLDER_SCENARIO: &str = r#"
set -eu

# FILE_TRIES and WAIT_INTERVAL are also `wait_nvim_count`'s budget, since it reads them.
WAIT_INTERVAL=0.2
SETTLE=1.0
QUIT_TRIES=75
FILE_TRIES=100
# Readback attempts, and the polls each one waits for its file.
READ_TRIES=15
READ_FILE_TRIES=25

cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

read_once() {
    t=0
    while [ "$t" -lt "$READ_FILE_TRIES" ]; do
        if [ -s "$OF_READBACK" ]; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    return 1
}

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome open-folder smoke"

# Every planted tab runs one Neovim; another count means the restore is not the planted one.
wait_nvim_count "$OF_NVIMS" 21 "after the launch restored the planted state"
sleep "$SETTLE"

wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$OF_QUERY"
sleep "$SETTLE"
# The sole row is D's Open Recent row; Enter hands it to AppShell::open_folder.
wtype -k Return
sleep "$SETTLE"
sleep "$SETTLE"

if [ -n "$OF_READBACK" ]; then
    n=0
    while :; do
        rm -f "$OF_READBACK"
        wtype -- ":call writefile([getcwd(), printf('%d', winnr('\$'))], '$OF_READBACK')"
        wtype -k Return
        if read_once; then
            got=$(sed -n 1p "$OF_READBACK")
            wins=$(sed -n 2p "$OF_READBACK")
            if [ "${got##*/}" = "$OF_WANT_BASE" ] && [ "$wins" = "$OF_WANT_WINS" ]; then
                break
            fi
            echo "wolf-gnome open-folder smoke: readback $n: getcwd() = $got, $wins window(s)" >&2
        fi
        n=$((n + 1))
        if [ "$n" -ge "$READ_TRIES" ]; then
            break
        fi
        sleep "$SETTLE"
    done
    if [ ! -s "$OF_READBACK" ]; then
        echo "wolf-gnome open-folder smoke: no readback ever appeared at $OF_READBACK." >&2
        exit 15
    fi
fi

wtype -M ctrl -M shift -k q -m shift -m ctrl
tries=0
gone=0
while [ "$tries" -lt "$QUIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        gone=1
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done
if [ "$gone" -ne 1 ]; then
    echo "wolf-gnome open-folder smoke: the app (pid $app_pid) is still alive after Ctrl+Shift+Q." >&2
    exit 12
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome open-folder smoke: the app (pid $app_pid) exited $rc, not 0." >&2
    exit 13
fi
app_pid=

if ! freeze_state "$OF_STATE_SRC" "$OF_STATE_COPY"; then
    echo "wolf-gnome open-folder smoke: the app left no wolf.sqlite in $OF_STATE_SRC." >&2
    exit 16
fi

exit 0
"#;

/// What one run of [`OPENFOLDER_SCENARIO`] is given.
struct OpenFolderRun<'a> {
    smoke: &'a str,
    /// The launch cwd.
    cwd: &'a Path,
    /// The `HOME` the app gets, when not [`smoke_home`]'s.
    home: Option<&'a Path>,
    /// The Neovims the launch runs, one per planted tab.
    nvims: usize,
    /// The palette query, `D`'s basename.
    query: &'a str,
    /// The readback path and the `(basename, window count)` it is polled for, if any.
    readback: Option<(&'a Path, &'a str, usize)>,
    /// The frozen copy's name under the runtime dir.
    frozen_name: &'a str,
}

/// Runs [`OPENFOLDER_SCENARIO`] in `runtime_dir` and returns the frozen copy of the save it
/// leaves, after asserting the scenario ran to completion. The copy lives under `runtime_dir`,
/// which its drop removes, so the caller keeps the runtime until it has read the copy.
fn run_open_folder(run: &OpenFolderRun<'_>, runtime_dir: &CageRuntime<'_>) -> PathBuf {
    let smoke = run.smoke;
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let state = smoke_state_dir(run.cwd);
    let frozen = frozen_state_dir(runtime_dir, run.frozen_name, smoke);
    let (readback, want_base, want_wins) = match run.readback {
        Some((path, base, wins)) => {
            clear_evidence(path, smoke);
            (path.to_path_buf(), base, wins.to_string())
        }
        None => (PathBuf::new(), "", String::new()),
    };
    let scenario = with_nvim_pid_helper(OPENFOLDER_SCENARIO);
    let mut command = cage_command(&scenario, run.cwd, runtime_dir);
    if let Some(home) = run.home {
        command.env("HOME", home);
    }
    let status = command
        .env("WOLF_BIN", bin)
        .env("OF_NVIMS", run.nvims.to_string())
        .env("OF_QUERY", run.query)
        .env("OF_READBACK", &readback)
        .env("OF_WANT_BASE", want_base)
        .env("OF_WANT_WINS", want_wins)
        .env("OF_STATE_SRC", &state)
        .env("OF_STATE_COPY", &frozen)
        .status()
        .expect("failed to spawn `cage` for the headless Open Folder scenario");
    assert!(
        status.success(),
        "{smoke}: the cage Open Folder scenario exited unsuccessfully ({status}). Exit 11: the \
         app died before it was ready. Exit 21: the launch did not run {} Neovim(s), so the \
         planted state did not restore as planted and nothing below is about Open Folder. Exit \
         15: no readback ever appeared, so no Ex command reached a Neovim after the activation. \
         Exit 12 or 13: the quit hung or was not clean, which a borrow held across the empty \
         tab's close can give. Exit 16: the app left no wolf.sqlite in {}. The cage log in the \
         runtime dir carries the scenario's own lines.",
        run.nvims,
        state.display()
    );
    frozen
}

/// Creates `<parent>/<name>` and returns its canonical path as a string.
fn make_dir(smoke: &str, parent: &Path, name: &str) -> String {
    let dir = parent.join(name);
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|e| panic!("{smoke}: failed to create {}: {e}", dir.display()));
    canonical(smoke, &dir)
}

/// `path`'s canonical form as a string: tab directories and store keys are canonical.
fn canonical(smoke: &str, path: &Path) -> String {
    let canon = std::fs::canonicalize(path)
        .unwrap_or_else(|e| panic!("{smoke}: failed to canonicalize {}: {e}", path.display()));
    canon
        .to_str()
        .unwrap_or_else(|| panic!("{smoke}: {} is not UTF-8", canon.display()))
        .to_owned()
}

/// A window of one tab per entry of `tabs`, `(directory, root)`, with `active_tab`.
fn planted_window(tabs: Vec<(String, Option<Node>)>, active_tab: usize) -> Window {
    Window {
        frame: Frame {
            x: None,
            y: None,
            width: 900.0,
            height: 640.0,
        },
        active_tab,
        minimized: false,
        tabs: tabs
            .into_iter()
            .map(|(directory, root)| Tab {
                directory,
                content: TabContent {
                    root,
                    ..TabContent::default()
                },
            })
            .collect(),
    }
}

/// A lone edit pane on `path`.
fn edit(path: &str) -> Node {
    Node::Pane(Pane::Edit {
        path: path.to_owned(),
        cursor: None,
        top_line: None,
    })
}

/// A vertical split of edit panes on `left` and `right` under `directory`, writing both files.
fn two_pane_split(smoke: &str, directory: &str, left: &str, right: &str) -> Node {
    let mut children = Vec::new();
    for (name, heading) in [(left, "# OPENFOLDERLEFT"), (right, "# OPENFOLDERRIGHT")] {
        let path = Path::new(directory).join(name);
        std::fs::write(&path, format!("{heading}\n"))
            .unwrap_or_else(|e| panic!("{smoke}: failed to write {}: {e}", path.display()));
        children.push(edit(&path.to_string_lossy()));
    }
    Node::Layout(Layout {
        orientation: PaneOrientation::Vertical,
        weights: vec![0.5, 0.5],
        children,
    })
}

/// Submits each of `states` in turn to the store in `state_dir`, flushing each, and asserts
/// the last reads back as the window state.
fn plant(smoke: &str, state_dir: &Path, states: &[WindowState]) {
    {
        let store = AppStateStore::open(state_dir);
        for planted in states {
            let recency = vec![Vec::new(); planted.windows.len()];
            store.submit(planted.clone(), recency);
            assert!(
                store.flush(OF_PLANT_FLUSH),
                "{smoke}: planting a window state in {} did not flush within \
                 {OF_PLANT_FLUSH:?}, so the launch would restore whatever the file holds",
                state_dir.display()
            );
        }
    }
    assert_eq!(
        AppStateStore::open(state_dir).window_state().as_ref(),
        states.last(),
        "{smoke}: premise: the window state must read back as the last one planted; otherwise \
         the launch does not start from the state this smoke describes"
    );
}

/// Asserts `directory`'s project state in `state_dir` holds `root`, the planting's premise.
fn assert_planted_project(smoke: &str, state_dir: &Path, directory: &str, root: &Node) {
    let project = AppStateStore::open(state_dir).project_state(directory);
    assert_eq!(
        project.as_ref().and_then(|p| p.tab.root.as_ref()),
        Some(root),
        "{smoke}: premise: {directory}'s project state must hold the planted split after a \
         window state without it was submitted; got {project:?}. None means the write \
         dropped the row or never wrote it, and Open Folder would read nothing"
    );
}

/// Seeds the folder MRU in `state_dir` with `directory` and asserts it reads back.
fn seed_recent(smoke: &str, state_dir: &Path, directory: &str) {
    {
        RecentDirs::open(state_dir).record(Path::new(directory));
    }
    let seeded = RecentDirs::open(state_dir).recent();
    assert!(
        seeded.iter().any(|dir| dir == Path::new(directory)),
        "{smoke}: premise: the seeded folder MRU in {} does not hold {directory}; it holds \
         {seeded:?}. `RecentDirs::record` swallows every error, so the palette would offer no \
         row for it",
        state_dir.display()
    );
}

/// Asserts the command MRU in `state_dir` holds `open_recent:<directory>`: the palette ran
/// `directory`'s Open Recent row and no other.
fn assert_open_recent_ran(smoke: &str, state_dir: &Path, directory: &str) {
    let want = format!("open_recent:{directory}");
    let recent = CommandMru::open(state_dir).recent();
    assert!(
        recent.iter().any(|key| key == &want),
        "{smoke}: precondition: the command MRU does not hold {want:?}; it holds {recent:?}. \
         Only a palette activation writes it, so an empty list means no row was activated and \
         another key means another row ran, so nothing below is about Open Folder"
    );
}

/// Each saved window's tab directories, in tab order.
fn window_dirs(state: &WindowState) -> Vec<Vec<String>> {
    state
        .windows
        .iter()
        .map(|window| {
            window
                .tabs
                .iter()
                .map(|tab| tab.directory.clone())
                .collect()
        })
        .collect()
}

/// The readback's `(getcwd(), window count)`.
fn read_cwd_and_wins(smoke: &str, path: &Path) -> (String, String) {
    let raw = read_readback(path, smoke);
    let mut lines = raw.lines();
    let cwd = lines.next().unwrap_or_default().trim().to_owned();
    let wins = lines.next().unwrap_or_default().trim().to_owned();
    (cwd, wins)
}

/// **An Open Recent row for a directory open only in a background tab of another window
/// selects that tab, and opens nothing.** §6 step 1.
///
/// Planted: the front window holds one tab on the launch cwd `E`; the back window holds
/// [`OF_FOCUS_OTHER_DIR`], selected, then `D`. The launch maps the back window first, so the
/// front window holds the keyboard and invokes the row. The raise itself is invisible under
/// `cage`, which declines a `present()` of any but the newest toplevel; that is a person check.
///
/// ## Observables
///
/// The save at the quit holds the same two windows with the same tab directories, so nothing
/// opened and nothing was replaced, and the back window's `active_tab` is `D`'s index, 1.
///
/// ## Falsifiers
///
/// | # | edit | predicted red |
/// | --- | --- | --- |
/// | S1a | in `Coordinator::open_folder`'s Focus arm, drop the tab selection and keep the present | the back window's `active_tab` is 0 |
/// | S1b | `open_folder::step` skips the `project_tab` lookup (G1a) | the save holds a third window, on `D` |
#[test]
fn open_recent_focuses_a_background_tab_of_another_window_under_cage() {
    const SMOKE: &str = "open_recent_focuses_a_background_tab_of_another_window_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let east = canonical(SMOKE, &cwd);
    let other = make_dir(SMOKE, smoke_home(&cwd), OF_FOCUS_OTHER_DIR);
    let target = make_dir(SMOKE, smoke_home(&cwd), OF_FOCUS_TARGET_DIR);

    let state = smoke_state_dir(&cwd);
    let planted = WindowState {
        windows: vec![
            planted_window(vec![(east.clone(), None)], 0),
            planted_window(vec![(other.clone(), None), (target.clone(), None)], 0),
        ],
    };
    plant(SMOKE, &state, std::slice::from_ref(&planted));
    seed_recent(SMOKE, &state, &target);

    let run = OpenFolderRun {
        smoke: SMOKE,
        cwd: &cwd,
        home: None,
        nvims: 3,
        query: OF_FOCUS_TARGET_DIR,
        readback: None,
        frozen_name: "wolf_gnome_open_folder_focus_state",
    };
    let frozen = run_open_folder(&run, &runtime_dir);
    assert_open_recent_ran(SMOKE, &state, &target);

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    let dirs = window_dirs(&saved);
    let mut sorted = dirs.clone();
    sorted.sort();
    let mut want = window_dirs(&planted);
    want.sort();
    assert_eq!(
        sorted, want,
        "{SMOKE}: the save holds the windows {dirs:?}; want the two planted ones, {want:?}, in \
         any order. A third window on {target} means Open Folder found no tab on D and opened a \
         new window: the lookup was skipped or missed the back window's background tab. A \
         window that lost a tab or holds another directory means a tab was replaced"
    );
    let back = saved
        .windows
        .iter()
        .find(|window| window.tabs.len() == 2)
        .expect("the sorted comparison above proved a two-tab window is saved");
    assert_eq!(
        back.active_tab, 1,
        "{SMOKE}: the back window's active tab is {}; want 1, the tab on {target}. 0 means Open \
         Folder found the tab but did not select it, as the Focus arm with its selection dropped \
         does; the planted active tab is 0",
        back.active_tab
    );
}

/// **An Open Recent row from an empty tab fills that tab from `D`'s project state.** §6 step 2.
///
/// The launch is in the empty state (`HOME` is the launch cwd, which `launch_decision`
/// refuses), restoring a planted window of one empty tab. `D`'s project state is a vertical
/// split of [`OF_FILL_LEFT_FILE`] and [`OF_FILL_RIGHT_FILE`].
///
/// ## Observables
///
/// The readback from the window, after the fill, is `getcwd() == D` with two windows: the
/// restored split, live. The save holds one window with one tab, on `D`, whose root is the
/// planted split, so the empty tab was replaced in its window rather than a window added.
///
/// ## Falsifiers
///
/// | # | edit | predicted red |
/// | --- | --- | --- |
/// | S2a | `open_folder::plan` ignores the project (G6a) | the readback reports 1 window |
/// | S2b | `open_folder::step` answers `NewWindow` whenever no tab is on `D` (G4a) | the save holds two windows |
/// | S2c | in `TabsInner::open_tab`, drop the `request_layout_rebuild` call | the readback reports 1 window |
#[test]
fn open_recent_from_an_empty_tab_fills_it_from_the_project_state_under_cage() {
    const SMOKE: &str = "open_recent_from_an_empty_tab_fills_it_from_the_project_state_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let target = make_dir(SMOKE, smoke_home(&cwd), OF_FILL_TARGET_DIR);
    let split = two_pane_split(SMOKE, &target, OF_FILL_LEFT_FILE, OF_FILL_RIGHT_FILE);

    let state = smoke_state_dir(&cwd);
    let empty = WindowState {
        windows: vec![planted_window(vec![(String::new(), None)], 0)],
    };
    let holding = WindowState {
        windows: vec![planted_window(
            vec![(target.clone(), Some(split.clone()))],
            0,
        )],
    };
    plant(SMOKE, &state, &[holding, empty]);
    assert_planted_project(SMOKE, &state, &target, &split);
    seed_recent(SMOKE, &state, &target);

    let readback = cwd.join(OF_FILL_READBACK_NAME);
    let run = OpenFolderRun {
        smoke: SMOKE,
        cwd: &cwd,
        home: Some(&cwd),
        nvims: 1,
        query: OF_FILL_TARGET_DIR,
        readback: Some((&readback, OF_FILL_TARGET_DIR, 2)),
        frozen_name: "wolf_gnome_open_folder_fill_state",
    };
    let frozen = run_open_folder(&run, &runtime_dir);
    assert_open_recent_ran(SMOKE, &state, &target);

    let (got_cwd, wins) = read_cwd_and_wins(SMOKE, &readback);
    assert_eq!(
        got_cwd, target,
        "{SMOKE}: after the activation the focused tab's Neovim is in {got_cwd:?}, not D. The \
         launch HOME means the empty tab was never replaced; the readback polls for up to \
         READ_TRIES attempts, so this is not an early read"
    );
    assert_eq!(
        wins, "2",
        "{SMOKE}: the filled tab shows {wins} window(s) live; want 2, D's planted split. 1 means \
         the tab opened on D without its project state, or its layout rebuild never ran"
    );

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    assert_eq!(
        window_dirs(&saved),
        vec![vec![target.clone()]],
        "{SMOKE}: the save must hold one window with one tab, on D. A second window means the \
         step opened a new window instead of filling the empty tab; an empty directory beside D \
         means the empty tab was not closed"
    );
    let tails = saved.windows[0].tabs[0]
        .content
        .root
        .as_ref()
        .map(pane_tails);
    assert_eq!(
        tails,
        Some(vec![
            Some(OF_FILL_LEFT_FILE.to_owned()),
            Some(OF_FILL_RIGHT_FILE.to_owned())
        ]),
        "{SMOKE}: D's saved tab must hold the planted split's two panes, in order"
    );
}

/// **An Open Recent row from a tab with a directory opens a new window restored from `D`'s
/// project state, and leaves the launch tab alone.** §6 step 3.
///
/// The launch is at folder `P`, restoring a planted window of one tab on `P` showing
/// [`OF_NEW_BASE_FILE`]. `D`'s project state is a vertical split of [`OF_NEW_LEFT_FILE`] and
/// [`OF_NEW_RIGHT_FILE`].
///
/// ## Observables
///
/// The readback from the new window, the newest toplevel and so the one holding the keyboard,
/// is `getcwd() == D` with two windows. The save holds two windows: `P`'s, one tab on `P`
/// still showing its file, and `D`'s, one tab whose root is the planted split.
///
/// ## Falsifiers
///
/// | # | edit | predicted red |
/// | --- | --- | --- |
/// | S3a | `open_folder::plan` ignores the project (G6a) | the readback reports 1 window |
/// | S3b | `open_folder::step` answers `FillCurrent` whenever no tab is on `D` (G5a) | the save holds one window |
#[test]
fn open_recent_from_a_folder_tab_opens_a_restored_window_under_cage() {
    const SMOKE: &str = "open_recent_from_a_folder_tab_opens_a_restored_window_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let project = canonical(SMOKE, &cwd);
    write_relaunch_fixtures(SMOKE, &cwd, &[(OF_NEW_BASE_FILE, "# OPENFOLDERBASE")]);
    let base = edit(&Path::new(&project).join(OF_NEW_BASE_FILE).to_string_lossy());
    let target = make_dir(SMOKE, smoke_home(&cwd), OF_NEW_TARGET_DIR);
    let split = two_pane_split(SMOKE, &target, OF_NEW_LEFT_FILE, OF_NEW_RIGHT_FILE);

    let state = smoke_state_dir(&cwd);
    let launch = WindowState {
        windows: vec![planted_window(vec![(project.clone(), Some(base))], 0)],
    };
    let holding = WindowState {
        windows: vec![planted_window(
            vec![(target.clone(), Some(split.clone()))],
            0,
        )],
    };
    plant(SMOKE, &state, &[holding, launch]);
    assert_planted_project(SMOKE, &state, &target, &split);
    seed_recent(SMOKE, &state, &target);

    let readback = cwd.join(OF_NEW_READBACK_NAME);
    let run = OpenFolderRun {
        smoke: SMOKE,
        cwd: &cwd,
        home: None,
        nvims: 1,
        query: OF_NEW_TARGET_DIR,
        readback: Some((&readback, OF_NEW_TARGET_DIR, 2)),
        frozen_name: "wolf_gnome_open_folder_new_state",
    };
    let frozen = run_open_folder(&run, &runtime_dir);
    assert_open_recent_ran(SMOKE, &state, &target);

    let (got_cwd, wins) = read_cwd_and_wins(SMOKE, &readback);
    assert_eq!(
        got_cwd, target,
        "{SMOKE}: after the activation the focused window's Neovim is in {got_cwd:?}, not D. P \
         means no window opened on D, or the keys never reached it"
    );
    assert_eq!(
        wins, "2",
        "{SMOKE}: the new window shows {wins} window(s) live; want 2, D's planted split. 1 means \
         the window opened on D without its project state, or its layout rebuild never ran"
    );

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    let dirs = window_dirs(&saved);
    let mut sorted = dirs.clone();
    sorted.sort();
    let mut want = vec![vec![project.clone()], vec![target.clone()]];
    want.sort();
    assert_eq!(
        sorted, want,
        "{SMOKE}: the save holds the windows {dirs:?}; want P's and D's, one tab each. One \
         window on D means P's tab was replaced, which §6 forbids for a tab with a directory"
    );
    for window in &saved.windows {
        let tab = &window.tabs[0];
        let (want_tails, what) = if tab.directory == target {
            (
                vec![
                    Some(OF_NEW_LEFT_FILE.to_owned()),
                    Some(OF_NEW_RIGHT_FILE.to_owned()),
                ],
                "D's tab must hold the planted split's two panes, in order",
            )
        } else {
            (
                vec![Some(OF_NEW_BASE_FILE.to_owned())],
                "P's tab must still show its one file, untouched by the Open Folder",
            )
        };
        assert_eq!(
            tab.content.root.as_ref().map(pane_tails),
            Some(want_tails),
            "{SMOKE}: {what}; the tab on {} saved another root",
            tab.directory
        );
    }
}

/// The smokes' palette queries each select exactly their folder's Open Recent row: none
/// matches a registry command, and S1's matches no Switch to Window row for its back window.
///
/// Each smoke presses Enter on the top row, so a second matching row would make which row
/// runs depend on ranking, and a Close or Quit row would end the run. Compositor-free.
#[test]
fn the_open_folder_queries_select_exactly_their_folder_row() {
    let other = PaletteWindow {
        index: 1,
        cwd: format!("/tmp/nix-shell.abcdef/{TEMP_CWD_PREFIX}1-2-0/{OF_FOCUS_OTHER_DIR}"),
    };
    let cases: [(&str, &[PaletteWindow]); 3] = [
        (OF_FOCUS_TARGET_DIR, std::slice::from_ref(&other)),
        (OF_FILL_TARGET_DIR, &[]),
        (OF_NEW_TARGET_DIR, &[]),
    ];
    for (query, windows) in cases {
        let registry = wolf_gnome::palette::palette_rows(&[], &[], &[], query, usize::MAX);
        assert!(
            registry.is_empty(),
            "the query {query:?} matches registry rows {:?}; rename that smoke's directory",
            registry.iter().map(|row| &row.display).collect::<Vec<_>>()
        );
        let dir = PathBuf::from(format!("/proj/{query}"));
        let rows = wolf_gnome::palette::palette_rows(
            std::slice::from_ref(&dir),
            windows,
            &[],
            query,
            usize::MAX,
        );
        let kinds: Vec<(PaletteRowKind, &str)> = rows
            .iter()
            .map(|row| (row.kind, row.display.as_str()))
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "the query {query:?} must yield exactly its folder's row; got {kinds:?}. None means \
             the folder never became a row; two means a Switch to Window row matches too"
        );
        assert_eq!(
            (rows[0].kind, rows[0].path.as_deref()),
            (PaletteRowKind::OpenRecent, Some(dir.as_path())),
            "the query {query:?}'s one row must be the Open Recent row for {dir:?}"
        );
    }
}
