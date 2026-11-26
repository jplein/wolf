//! The `wolf` command's smokes (`specs/cli-and-file-open.md`): `wolf DIR` and `wolf FILE` sent
//! to the app under `cage` over its open channel, from the scenario shell, which carries no
//! `WOLF_TAB_ID`, or typed into a terminal pane, whose shell carries its tab's.
//!
//! Each plants a state as `open_folder.rs` does and runs the `wolf` beside the app binary
//! ([`wolf_command_bin`]). `wolf` exits 0 once the app has taken a request, before it is
//! handled, so each request is followed by a polled readback the captive Neovim writes: its
//! `getcwd()`, `expand('%:p')`, `winnr()`, `winnr('$')`, `win_getid()` and `winlayout()`. A
//! focus move lands on a later wake, so the first readbacks can still come from the old pane.
//!
//! Keys reach only the newest window under `cage`, and a background window cannot be raised
//! there (`notes/wolf-gnome/present-limitations.md`), so every request here lands in the window
//! that holds the keyboard and no smoke asserts a raise. What only the save can show is read
//! from the state frozen after `Ctrl+Shift+Q`. Nothing is OCR'd.
//!
//! ## Exit codes
//!
//! | code | meaning |
//! | --- | --- |
//! | `11` | the app the scenario launched died before it was ready for input |
//! | `12` | the app was still alive after `Ctrl+Shift+Q` |
//! | `13` | the app the scenario launched exited non-zero |
//! | `15` | a readback never appeared: no Ex command reached a Neovim; the log names the leg |
//! | `16` | the app left no `wolf.sqlite` |
//! | `21` | the launch did not run the planted tabs' Neovims |
//! | `22` | the Neovim count changed after a request: a tab or window was added |
//! | `31` | a `wolf` run from the scenario shell exited non-zero; the log names it |
//! | `32` | a terminal's shell never wrote its readback; the log names which |
//! | `40` | the app never wrote its ready file |
//! | `41` | the ready file of an app `wolf` launched names a pid that is gone |
//! | `51` | S5's leg 1: the `git commit` typed into the terminal never wrote its status |
//! | `52` | S5's leg 2: the `git commit` from the scenario shell was still running at its bound |
//! | `53` | S5's leg 3: `wolf --wait` was still running at its bound |
//! | `54` | S5's leg 3: `:wq` on tab 1's last pane ended the app, not that tab alone |
//! | `55` | S5's leg 3: `:wq` on tab 1's last pane did not leave exactly one Neovim |

use super::open_folder::{canonical, edit, make_dir, plant};
use super::recovery::with_nvim_pid_helper;
use super::*;
use wolf_core::app_state::{Frame, Tab, TabContent};

/// Every planted window's size.
const FO_WINDOW_SIZE: (f64, f64) = (900.0, 640.0);

/// The settings, teardown and helpers every file-open scenario starts with.
///
/// `fo_wolf <label> <path>` runs `wolf` from this shell; `fo_wolf_as <id> <label> <path>`
/// runs it with `WOLF_TAB_ID` set to `<id>`. `fo_probe <readback> <cwd> <path> <label>` types
/// the readback until it names `<cwd>` and `<path>`, `*` matching any path, or `READ_TRIES`
/// run out, keeping the last. `fo_shell <line>` types a line into a terminal's shell.
const FO_PRELUDE: &str = r#"
set -eu

# FILE_TRIES and WAIT_INTERVAL are also `wait_nvim_count`'s budget, since it reads them.
WAIT_INTERVAL=0.2
SETTLE=1.0
DRAWER_SETTLE=2.0
QUIT_TRIES=75
FILE_TRIES=100
READ_TRIES=20
READ_FILE_TRIES=25
# 1 while app_pid is this shell's child, so `wait` can read its exit status.
fo_child=1

cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

fo_wolf() {
    _fo_rc=0
    "$WOLF_CMD" "$2" || _fo_rc=$?
    if [ "$_fo_rc" -ne 0 ]; then
        echo "wolf-gnome file-open smoke: $1: wolf $2 exited $_fo_rc, not 0." >&2
        exit 31
    fi
}

fo_wolf_as() {
    _fo_rc=0
    WOLF_TAB_ID="$1" "$WOLF_CMD" "$3" || _fo_rc=$?
    if [ "$_fo_rc" -ne 0 ]; then
        echo "wolf-gnome file-open smoke: $2: WOLF_TAB_ID=$1 wolf $3 exited $_fo_rc, not 0." >&2
        exit 31
    fi
}

fo_shell() {
    wtype -- "$1"
    wtype -k Return
}

# fo_wait_pid <pid> <code> <label>: waits for a child of this shell to exit, leaving its status
# in fo_rc, and exits <code> when it still runs after FILE_TRIES.
fo_wait_pid() {
    _fw_n=0
    while kill -0 "$1" 2>/dev/null; do
        _fw_n=$((_fw_n + 1))
        if [ "$_fw_n" -ge "$FILE_TRIES" ]; then
            kill "$1" 2>/dev/null
            echo "wolf-gnome file-open smoke: $3 (pid $1) still runs after $FILE_TRIES tries." >&2
            exit "$2"
        fi
        sleep "$WAIT_INTERVAL"
    done
    fo_rc=0
    wait "$1" || fo_rc=$?
}

fo_read_once() {
    _fo_t=0
    while [ "$_fo_t" -lt "$READ_FILE_TRIES" ]; do
        if [ -s "$1" ]; then
            return 0
        fi
        _fo_t=$((_fo_t + 1))
        sleep "$WAIT_INTERVAL"
    done
    return 1
}

fo_probe() {
    _fo_n=0
    while :; do
        rm -f "$1"
        wtype -- ":call writefile([getcwd(), expand('%:p'), printf('%d', winnr()), printf('%d', winnr('\$')), printf('%d', win_getid()), string(winlayout())], '$1')"
        wtype -k Return
        if fo_read_once "$1"; then
            _fo_cwd=$(sed -n 1p "$1")
            _fo_path=$(sed -n 2p "$1")
            if [ "$_fo_cwd" = "$2" ] && { [ "$3" = "*" ] || [ "$_fo_path" = "$3" ]; }; then
                return 0
            fi
            echo "wolf-gnome file-open smoke: $4: readback $_fo_n: $(tr '\n' '|' < "$1")" >&2
        fi
        _fo_n=$((_fo_n + 1))
        if [ "$_fo_n" -ge "$READ_TRIES" ]; then
            break
        fi
        sleep "$SETTLE"
    done
    if [ ! -s "$1" ]; then
        echo "wolf-gnome file-open smoke: $4: no readback ever appeared at $1." >&2
        exit 15
    fi
}
"#;

/// The launch of every scenario but the cold one, which `wolf` launches itself.
const FO_LAUNCH: &str = r#"
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome file-open smoke"
"#;

/// The quit every scenario ends with: `Ctrl+Shift+Q` unless the app has already exited, the exit
/// status when the app is this shell's child, and the save frozen into `$FO_STATE_COPY`.
const FO_QUIT: &str = r#"
if kill -0 "$app_pid" 2>/dev/null; then
    wtype -M ctrl -M shift -k q -m shift -m ctrl
fi
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
    echo "wolf-gnome file-open smoke: the app (pid $app_pid) is still alive after Ctrl+Shift+Q." >&2
    exit 12
fi
if [ "$fo_child" = 1 ]; then
    rc=0
    wait "$app_pid" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "wolf-gnome file-open smoke: the app (pid $app_pid) exited $rc, not 0." >&2
        exit 13
    fi
fi
app_pid=

if ! freeze_state "$FO_STATE_SRC" "$FO_STATE_COPY"; then
    echo "wolf-gnome file-open smoke: the app left no wolf.sqlite in $FO_STATE_SRC." >&2
    exit 16
fi

exit 0
"#;

/// S1's body: the launch is in the empty state, and `wolf $FO_DIR` must fill that tab.
const FO_DIR_BODY: &str = r#"
wait_nvim_count 1 21 "after the launch in the empty state"
sleep "$SETTLE"
fo_wolf "wolf DIR" "$FO_DIR"
fo_probe "$FO_READ" "$FO_DIR" "*" "after wolf DIR"
wait_nvim_count 1 22 "after wolf DIR filled the empty tab"
"#;

/// S2's body: one `wolf` per planted tab `P1` to `P5`, each followed by a readback that waits
/// for that tab's directory and the file.
const FO_PANES_BODY: &str = r#"
wait_nvim_count 6 21 "after the launch restored six tabs"
sleep "$SETTLE"
for i in 1 2 3 4 5; do
    eval "dir=\$FO_P${i}_DIR file=\$FO_P${i}_FILE read=\$FO_READ$i"
    fo_wolf "wolf P$i" "$file"
    fo_probe "$read" "$dir" "$file" "P$i"
done
"#;

/// S3's body: tab 1's terminal writes its tab ID and runs `wolf` on a file of tab 2's project;
/// then `wolf` with a stale ID from this shell; then tab 2's drawer writes its tab ID.
const FO_TAB_ID_BODY: &str = r#"
wait_nvim_count 2 21 "after the launch restored two tabs"
sleep "$SETTLE"
fo_shell "printf 'id=%s\n' \"\${WOLF_TAB_ID-unset}\" > '$FO_ID1'"
wait_file_contains "$FO_ID1" "id=" "tab 1's terminal" || exit 32
fo_shell "'$WOLF_CMD' '$FO_TAB_FILE'; echo \"rc=\$?\" > '$FO_RC1'"
wait_file_contains "$FO_RC1" "rc=" "wolf typed into tab 1's terminal" || exit 32
fo_probe "$FO_READ1" "$FO_A" "$FO_TAB_FILE" "tab 1 after wolf from its terminal"
fo_wolf_as "$FO_STALE_ID" "wolf with a stale tab ID" "$FO_STALE_FILE"
fo_probe "$FO_READ2" "$FO_B" "$FO_STALE_FILE" "after wolf with a stale tab ID"
wtype -M ctrl -k grave -m ctrl
sleep "$DRAWER_SETTLE"
fo_shell "printf 'id=%s\n' \"\${WOLF_TAB_ID-unset}\" > '$FO_ID2'"
wait_file_contains "$FO_ID2" "id=" "tab 2's drawer" || exit 32
"#;

/// S4's body: with no app running, `wolf` launches the one beside it, which must restore the
/// planted session before it places the file.
const FO_COLD_BODY: &str = r#"
fo_wolf "wolf FILE with no app running" "$FO_FILE"
wait_launched_ready 41 "the wolf-gnome wolf launched"
fo_child=0
wait_nvim_count 2 21 "after the cold launch restored the planted session"
fo_probe "$FO_READ" "$FO_A" "$FO_FILE" "the front window after the cold launch"
"#;

/// S5's body: leg 1 commits from tab 2's terminal, leg 2 commits from this shell and aborts
/// with `:cq`, and leg 3 waits on a file of tab 1 and closes its last pane with `:wq`, which
/// closes tab 1 alone and leaves the app running on tab 2. Each leg leaves its evidence in files
/// for the test.
const FO_WAIT_BODY: &str = r#"
wait_nvim_count 2 21 "after the launch restored two tabs"
sleep "$SETTLE"

fo_shell "GIT_EDITOR='$WOLF_CMD --wait' git commit --allow-empty -q; echo \"rc=\$?\" > '$FO_W_RC1'"
sleep "$SETTLE"
fo_probe "$FO_W_READ1" "$FO_R" "$FO_R/.git/COMMIT_EDITMSG" "leg 1's message file"
wtype -- "ggO$FO_W_MSG1"
wtype -k Escape
wtype -- ":wq"
wtype -k Return
wait_file_contains "$FO_W_RC1" "rc=" "leg 1's git commit" || exit 51
git -C "$FO_R" log -1 --format=%s > "$FO_W_SUBJECT1"
git -C "$FO_R" rev-list --count HEAD > "$FO_W_COUNT1"

GIT_EDITOR="$WOLF_CMD --wait" git -C "$FO_R" commit --allow-empty -q &
fo_git_pid=$!
sleep "$SETTLE"
fo_probe "$FO_W_READ2" "$FO_R" "$FO_R/.git/COMMIT_EDITMSG" "leg 2's message file"
wtype -- "ggO$FO_W_MSG2"
wtype -k Escape
wtype -- ":w"
wtype -k Return
wtype -- ":cq"
wtype -k Return
fo_wait_pid "$fo_git_pid" 52 "leg 2's git commit"
echo "rc=$fo_rc" > "$FO_W_RC2"
git -C "$FO_R" rev-list --count HEAD > "$FO_W_COUNT2"

"$WOLF_CMD" --wait "$FO_W_NOTES" &
fo_wolf_pid=$!
sleep "$SETTLE"
fo_probe "$FO_W_READ3" "$FO_A" "$FO_W_NOTES" "leg 3's file"
wtype -- "ggO$FO_W_TEXT3"
wtype -k Escape
wtype -- ":wq"
wtype -k Return
fo_wait_pid "$fo_wolf_pid" 53 "leg 3's wolf --wait"
echo "rc=$fo_rc" > "$FO_W_RC3"
# Until the close lands: fewer than two Neovims, or no app; then a settle, so a window close
# that shuts both tabs down has finished before the app's aliveness is read.
_fo_n=0
while pid_is_live "$app_pid" && [ "$_fo_n" -lt "$QUIT_TRIES" ]; do
    find_nvim_pid || true
    if [ "$count" -lt 2 ]; then
        break
    fi
    _fo_n=$((_fo_n + 1))
    sleep "$WAIT_INTERVAL"
done
sleep "$SETTLE"
if ! pid_is_live "$app_pid"; then
    echo "wolf-gnome file-open smoke: :wq on tab 1's last pane ended the app; it must close" >&2
    echo "  tab 1 alone and leave tab 2 open." >&2
    exit 54
fi
wait_nvim_count 1 55 "after :wq on tab 1's last pane"
"#;

/// What one file-open scenario is given.
struct FileOpenRun<'a> {
    smoke: &'a str,
    cwd: &'a Path,
    /// The `HOME` the app gets, when not [`smoke_home`]'s.
    home: Option<&'a Path>,
    /// Whether the scenario launches the app itself, rather than leaving that to `wolf`.
    launch: bool,
    body: &'a str,
    /// The scenario's own variables.
    env: Vec<(String, String)>,
    /// The frozen copy's name under the runtime dir.
    frozen_name: &'a str,
}

/// Runs the scenario in `runtime_dir` and returns the frozen copy of the save it leaves, after
/// asserting the scenario ran to completion. The copy lives under `runtime_dir`.
fn run_file_open(run: &FileOpenRun<'_>, runtime_dir: &CageRuntime<'_>) -> PathBuf {
    let smoke = run.smoke;
    let state = smoke_state_dir(run.cwd);
    let socket = wolf_core::open_socket_path(&state);
    assert!(
        socket.is_ok(),
        "{smoke}: premise: the app's socket in {} must fit the socket path limit, or neither \
         the app nor wolf can use the channel and nothing below is about opening files: {socket:?}. \
         Point TMPDIR somewhere shorter.",
        state.display()
    );
    let frozen = frozen_state_dir(runtime_dir, run.frozen_name, smoke);
    let launch = if run.launch { FO_LAUNCH } else { "" };
    let scenario =
        with_nvim_pid_helper(&format!("{FO_PRELUDE}\n{launch}\n{}\n{FO_QUIT}", run.body));
    let mut command = cage_command(&scenario, run.cwd, runtime_dir);
    if let Some(home) = run.home {
        command.env("HOME", home);
    }
    for (name, value) in &run.env {
        command.env(name, value);
    }
    let status = command
        .env("WOLF_BIN", env!("CARGO_BIN_EXE_wolf-gnome"))
        .env("WOLF_CMD", wolf_command_bin())
        .env("FO_STATE_SRC", &state)
        .env("FO_STATE_COPY", &frozen)
        .status()
        .expect("failed to spawn `cage` for the file-open scenario");
    assert!(
        status.success(),
        "{smoke}: the cage file-open scenario exited unsuccessfully ({status}). Exit 31: a wolf \
         run from the scenario shell exited non-zero, so the app took no request: no listener \
         (wolf says Wolf is not running), or a refusal. Exit 15: a readback never appeared, so \
         the keys reached no Neovim, as when focus stayed in a terminal pane. Exit 22: the \
         Neovim count changed after a request, so a window or tab was added where a tab should \
         have been reused. Exit 21: the launch did not restore the planted tabs, and nothing \
         below is about opening files. Exit 32: a terminal's shell wrote no readback. Exit 51 \
         to 53: a wolf --wait leg's git or wolf never finished, so nothing ended its wait. Exit \
         54: :wq on tab 1's last pane ended the app, closing the window rather than the tab. \
         Exit 55: that :wq did not leave exactly one Neovim, so the tab's close shut down no \
         core or more than one. Exit 11, 40 or 41: the app never \
         became ready. Exit 12, 13 or 16: the quit failed or left no save. The cage log in the \
         runtime dir carries the scenario's own lines, each readback that did not match among \
         them."
    );
    frozen
}

/// A scenario variable's name.
fn var(name: &str) -> String {
    name.to_owned()
}

/// A planted tab on `directory` holding `root`, its pane `focused` focused and `zoomed`
/// zoomed, each a depth-first index.
fn fo_tab(
    directory: &str,
    root: Option<Node>,
    focused: Option<usize>,
    zoomed: Option<usize>,
) -> Tab {
    Tab {
        directory: directory.to_owned(),
        content: TabContent {
            root,
            focused: focused.map(PaneRef::Pane),
            zoomed: zoomed.map(PaneRef::Pane),
            ..TabContent::default()
        },
    }
}

/// A planted window of `tabs`, `active_tab` selected, at [`FO_WINDOW_SIZE`].
fn fo_window(tabs: Vec<Tab>, active_tab: usize) -> Window {
    Window {
        frame: Frame {
            x: None,
            y: None,
            width: FO_WINDOW_SIZE.0,
            height: FO_WINDOW_SIZE.1,
        },
        active_tab,
        minimized: false,
        tabs,
    }
}

/// A split of `children` along `orientation` at `weights`.
fn split(orientation: PaneOrientation, weights: Vec<f64>, children: Vec<Node>) -> Node {
    Node::Layout(Layout {
        orientation,
        weights,
        children,
    })
}

/// A terminal pane whose shell starts in `directory`.
fn terminal(directory: &str) -> Node {
    Node::Pane(Pane::Terminal {
        path: directory.to_owned(),
    })
}

/// A preview pane of `path`.
fn preview(path: &str) -> Node {
    Node::Pane(Pane::Preview {
        path: path.to_owned(),
        scroll: None,
    })
}

/// Writes `body` to `<directory>/<name>` and returns the path.
fn write_file(smoke: &str, directory: &str, name: &str, body: &str) -> String {
    let path = Path::new(directory).join(name);
    std::fs::write(&path, format!("{body}\n"))
        .unwrap_or_else(|e| panic!("{smoke}: failed to write {}: {e}", path.display()));
    path.to_string_lossy().into_owned()
}

/// `<directory>/<name>`, which is not written.
fn path_in(directory: &str, name: &str) -> String {
    Path::new(directory)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// One readback the scenario's `fo_probe` had the captive Neovim write.
#[derive(Debug)]
struct Readback {
    cwd: String,
    path: String,
    winnr: String,
    wins: String,
    winid: String,
    layout: String,
}

fn read_probe(smoke: &str, path: &Path) -> Readback {
    let raw = read_readback(path, smoke);
    let mut lines = raw.lines().map(|line| line.trim().to_owned());
    let mut next = || lines.next().unwrap_or_default();
    Readback {
        cwd: next(),
        path: next(),
        winnr: next(),
        wins: next(),
        winid: next(),
        layout: next(),
    }
}

/// The saved tab on `directory` in `state`, panicking when no window holds one.
fn saved_tab<'a>(smoke: &str, state: &'a WindowState, directory: &str) -> &'a Tab {
    state
        .windows
        .iter()
        .flat_map(|window| &window.tabs)
        .find(|tab| tab.directory == directory)
        .unwrap_or_else(|| {
            panic!(
                "{smoke}: the save holds no tab on {directory}; its tabs are on {:?}. The tab \
                 was replaced or closed",
                state
                    .windows
                    .iter()
                    .map(|w| w
                        .tabs
                        .iter()
                        .map(|t| t.directory.clone())
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            )
        })
}

/// The file name of each pane of `tab`, depth-first; `None` for an empty tab.
fn tab_tails(tab: &Tab) -> Option<Vec<Option<String>>> {
    tab.content.root.as_ref().map(pane_tails)
}

/// The file name of `path`.
fn tail(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(OsStr::to_str)
        .map(str::to_owned)
}

/// S1's directory, which no tab is open on.
const FO_DIR_NAME: &str = "fodelta";

/// **`wolf DIR` from the empty state fills the empty current tab with `DIR`.** §4 through
/// Open Folder's step 2.
///
/// The launch is in the empty state (`HOME` is the launch cwd, which `launch_decision`
/// refuses), restoring a planted window of one empty tab, so it runs one Neovim. The scenario shell
/// runs `wolf D` for a directory no tab is on.
///
/// ## Observables
///
/// The readback from the window is `getcwd() == D`; the Neovim count is still one, so no
/// window was added; and the save holds one window with one tab, on `D`.
///
/// ## Falsifiers
///
/// | # | edit | predicted red |
/// | --- | --- | --- |
/// | S1a | `handle_open_requests` treats `OpenFolder` as `Activate` | the readback never names `D` |
/// | S1b | the coordinator hands `open_folder` no current tab, so a second window opens | exit 22 |
#[test]
fn wolf_dir_fills_the_empty_current_tab_under_cage() {
    const SMOKE: &str = "wolf_dir_fills_the_empty_current_tab_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let target = make_dir(SMOKE, smoke_home(&cwd), FO_DIR_NAME);
    plant(
        SMOKE,
        &smoke_state_dir(&cwd),
        &[WindowState {
            windows: vec![fo_window(vec![fo_tab("", None, None, None)], 0)],
        }],
    );
    let readback = runtime_dir.join("fo_dir_read.txt");
    clear_evidence(&readback, SMOKE);

    let run = FileOpenRun {
        smoke: SMOKE,
        cwd: &cwd,
        home: Some(&cwd),
        launch: true,
        body: FO_DIR_BODY,
        env: vec![
            (var("FO_DIR"), target.clone()),
            (var("FO_READ"), readback.to_string_lossy().into_owned()),
        ],
        frozen_name: "wolf_gnome_file_open_dir_state",
    };
    let frozen = run_file_open(&run, &runtime_dir);

    let read = read_probe(SMOKE, &readback);
    assert_eq!(
        read.cwd, target,
        "{SMOKE}: after wolf D the focused Neovim is in {:?}, not D ({target}). The launch cwd \
         or HOME means the request was taken and not acted on as Open Folder; the readback is \
         polled for READ_TRIES attempts, so this is not an early read",
        read.cwd
    );

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    let dirs: Vec<Vec<&str>> = saved
        .windows
        .iter()
        .map(|w| w.tabs.iter().map(|t| t.directory.as_str()).collect())
        .collect();
    assert_eq!(
        dirs,
        vec![vec![target.as_str()]],
        "{SMOKE}: the save must hold one window with one tab, on D. A second window means Open \
         Folder had no current tab, or took the tab as not empty; an empty directory beside D \
         means a tab was added rather than the empty one filled"
    );
}

/// S2's tab directories, siblings of the launch cwd.
const FO_PANE_DIRS: [&str; 5] = ["fopone", "foptwo", "fopthree", "fopfour", "fopfive"];

/// **`wolf FILE` places the file by each pane rule of §5.2.** One window holds a tab on the
/// launch cwd and one tab on each of `P1` to `P5`; each `wolf` names a file of one `P`, so it
/// is routed by project to that tab.
///
/// - P1 `Vertical[edit, edit]` focused on pane 1, and a file that does not exist: rule 1, the
///   active editor pane, opens a new buffer for the path.
/// - P2 `Vertical[Horizontal[edit 0.3, edit 0.7], terminal]` focused on the terminal: rule 2,
///   the leftmost editor pane, the larger of the two at the left edge.
/// - P3 `Vertical[preview, terminal]` focused on the terminal: rule 3, the preview becomes an
///   editor pane showing the file.
/// - P4 `Vertical[edit F, edit other]` focused on pane 1, and `wolf F`: the pane on `F` is
///   focused and pane 1 keeps `other`.
/// - P5 `Vertical[edit, edit]` zoomed and focused on pane 1: unzoomed, then rule 1.
///
/// ## Observables
///
/// Each leg's readback names its tab's directory and the file, with the window number the
/// rule picks where the layout makes it unambiguous. The save holds each tab's tree with the
/// file in that pane, P4's focus on pane 0, and no zoom on P5. P1's file is not on disk.
///
/// ## Falsifiers
///
/// | # | edit | predicted red |
/// | --- | --- | --- |
/// | S2a | `placement` maps every index to the first leaf | P1 |
/// | S2b | `Open` and `Convert` skip `focus_win` | P2 |
/// | S2c | `Convert` is handled as `SplitLeft` | P3 |
/// | S2d | `Focus` is handled as `Open` | P4 |
/// | S2e | `unzoom` is ignored | P5 |
/// | S2f | `PaneHandle::open_file` drops a path that is not an existing file | P1 |
#[test]
fn wolf_file_lands_by_each_pane_rule_under_cage() {
    const SMOKE: &str = "wolf_file_lands_by_each_pane_rule_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let project = canonical(SMOKE, &cwd);
    let dirs: Vec<String> = FO_PANE_DIRS
        .iter()
        .map(|name| make_dir(SMOKE, smoke_home(&cwd), name))
        .collect();
    let [p1, p2, p3, p4, p5] = [&dirs[0], &dirs[1], &dirs[2], &dirs[3], &dirs[4]];
    let even = || vec![0.5, 0.5];

    let p1_left = write_file(SMOKE, p1, "oneleft.md", "# one left");
    let p1_right = write_file(SMOKE, p1, "oneright.md", "# one right");
    let p1_file = path_in(p1, "onenew.md");

    let p2_top = write_file(SMOKE, p2, "twotop.md", "# two top");
    let p2_bottom = write_file(SMOKE, p2, "twobottom.md", "# two bottom");
    let p2_file = write_file(SMOKE, p2, "twoopened.md", "# two opened");

    let p3_source = write_file(SMOKE, p3, "threesource.md", "# three source");
    let p3_file = write_file(SMOKE, p3, "threeopened.md", "# three opened");

    let p4_file = write_file(SMOKE, p4, "fourshown.md", "# four shown");
    let p4_other = write_file(SMOKE, p4, "fourother.md", "# four other");

    let p5_left = write_file(SMOKE, p5, "fiveleft.md", "# five left");
    let p5_right = write_file(SMOKE, p5, "fiveright.md", "# five right");
    let p5_file = write_file(SMOKE, p5, "fiveopened.md", "# five opened");

    let tabs = vec![
        fo_tab(&project, None, None, None),
        fo_tab(
            p1,
            Some(split(
                PaneOrientation::Vertical,
                even(),
                vec![edit(&p1_left), edit(&p1_right)],
            )),
            Some(1),
            None,
        ),
        fo_tab(
            p2,
            Some(split(
                PaneOrientation::Vertical,
                even(),
                vec![
                    split(
                        PaneOrientation::Horizontal,
                        vec![0.3, 0.7],
                        vec![edit(&p2_top), edit(&p2_bottom)],
                    ),
                    terminal(p2),
                ],
            )),
            Some(2),
            None,
        ),
        fo_tab(
            p3,
            Some(split(
                PaneOrientation::Vertical,
                even(),
                vec![preview(&p3_source), terminal(p3)],
            )),
            Some(1),
            None,
        ),
        fo_tab(
            p4,
            Some(split(
                PaneOrientation::Vertical,
                even(),
                vec![edit(&p4_file), edit(&p4_other)],
            )),
            Some(1),
            None,
        ),
        fo_tab(
            p5,
            Some(split(
                PaneOrientation::Vertical,
                even(),
                vec![edit(&p5_left), edit(&p5_right)],
            )),
            Some(1),
            Some(1),
        ),
    ];
    let state = smoke_state_dir(&cwd);
    plant(
        SMOKE,
        &state,
        &[WindowState {
            windows: vec![fo_window(tabs, 0)],
        }],
    );

    let files = [&p1_file, &p2_file, &p3_file, &p4_file, &p5_file];
    let reads: Vec<PathBuf> = (1..=5)
        .map(|i| runtime_dir.join(format!("fo_pane_read{i}.txt")))
        .collect();
    let mut env = Vec::new();
    for (i, read) in reads.iter().enumerate() {
        clear_evidence(read, SMOKE);
        let n = i + 1;
        env.push((format!("FO_P{n}_DIR"), dirs[i].clone()));
        env.push((format!("FO_P{n}_FILE"), files[i].clone()));
        env.push((format!("FO_READ{n}"), read.to_string_lossy().into_owned()));
    }
    let run = FileOpenRun {
        smoke: SMOKE,
        cwd: &cwd,
        home: None,
        launch: true,
        body: FO_PANES_BODY,
        env,
        frozen_name: "wolf_gnome_file_open_panes_state",
    };
    let frozen = run_file_open(&run, &runtime_dir);

    let live: Vec<Readback> = reads.iter().map(|path| read_probe(SMOKE, path)).collect();
    for (i, read) in live.iter().enumerate() {
        assert_eq!(
            (read.cwd.as_str(), read.path.as_str()),
            (dirs[i].as_str(), files[i].as_str()),
            "{SMOKE}: P{}'s readback names {:?} in {:?}; want {} in its tab's directory {}. \
             Another directory means the request did not select P{}'s tab; the old file in the \
             right directory means the file was never placed. Readback: {read:?}",
            i + 1,
            read.path,
            read.cwd,
            files[i],
            dirs[i],
            i + 1
        );
    }
    assert_eq!(
        (live[0].winnr.as_str(), live[0].wins.as_str()),
        ("2", "2"),
        "{SMOKE}: P1's file must open in window 2 of 2, the active editor pane (rule 1). \
         Window 1 means the placement named the first leaf instead of the active one; a third \
         window means the tab was split"
    );
    assert!(
        !Path::new(&p1_file).exists(),
        "{SMOKE}: P1's file {p1_file} exists on disk; opening a missing file must make a new, \
         unsaved buffer for the path, and nothing here writes it"
    );
    assert_eq!(
        (live[3].winnr.as_str(), live[3].wins.as_str()),
        ("1", "2"),
        "{SMOKE}: wolf on P4's shown file must focus the pane already showing it, window 1 of \
         2. Window 2 means the file was opened in the active pane instead of the existing pane \
         being focused"
    );
    assert_eq!(
        (live[4].winnr.as_str(), live[4].wins.as_str()),
        ("2", "2"),
        "{SMOKE}: P5's file must open in its zoomed, active pane, window 2 of 2"
    );

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    let want: [(&str, Vec<Option<String>>, &str); 5] = [
        (
            p1,
            vec![tail(&p1_left), tail(&p1_file)],
            "the new buffer in the active pane 1, pane 0 untouched",
        ),
        (
            p2,
            vec![tail(&p2_top), tail(&p2_file), tail(p2)],
            "the file in the 0.7 pane, the larger at the left edge, and the terminal kept",
        ),
        (
            p3,
            vec![tail(&p3_file), tail(p3)],
            "the file where the preview was, and the terminal kept",
        ),
        (
            p4,
            vec![tail(&p4_file), tail(&p4_other)],
            "pane 1 still on the other file",
        ),
        (
            p5,
            vec![tail(&p5_left), tail(&p5_file)],
            "the file in the zoomed pane 1",
        ),
    ];
    for (directory, tails, what) in want {
        let tab = saved_tab(SMOKE, &saved, directory);
        assert_eq!(
            tab_tails(tab),
            Some(tails),
            "{SMOKE}: the tab on {directory} must save {what}. Its root: {:?}",
            tab.content.root
        );
    }
    let p3_tab = saved_tab(SMOKE, &saved, p3);
    let p3_kinds: Vec<bool> = p3_tab
        .content
        .root
        .as_ref()
        .map(|root| {
            root.panes()
                .into_iter()
                .map(|pane| matches!(pane, Pane::Preview { .. }))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        p3_kinds,
        vec![false, false],
        "{SMOKE}: P3's preview must become an editor pane on the file, leaving no preview (rule \
         3); a preview left means the file opened beside it. Root: {:?}",
        p3_tab.content.root
    );
    assert_eq!(
        saved_tab(SMOKE, &saved, p4).content.focused,
        Some(PaneRef::Pane(0)),
        "{SMOKE}: P4's focus must move to pane 0, the pane already on the file"
    );
    let p5_tab = saved_tab(SMOKE, &saved, p5);
    assert_eq!(
        p5_tab.content.zoomed, None,
        "{SMOKE}: P5 was zoomed, and opening a file must unzoom it first; Some means the zoom \
         survived, so the pane the file lands in could be hidden"
    );
}

/// S3's tab 2 directory, a sibling of the launch cwd.
const FO_TAB_B_NAME: &str = "fobravo";

/// **`WOLF_TAB_ID` reaches each tab's terminal sessions and names its tab, and a stale one is
/// as if unset.** §3 and §5.1 rule 1.
///
/// Tab 1 is on the launch cwd `A`, `Horizontal[terminal, terminal]` focused on pane 1; tab 2
/// is on `B`, one edit pane; tab 1 is selected. Typed into tab 1's terminal: a line writing
/// `$WOLF_TAB_ID`, then `wolf B/f.md`. From the scenario shell: `wolf B/g.md` with a fresh,
/// well-formed `WOLF_TAB_ID` no tab holds. Then tab 2's drawer writes its `$WOLF_TAB_ID`.
///
/// ## Observables
///
/// Both IDs are version-4 UUIDs and they differ. Tab 1's readback is in `A`, on `B/f.md`,
/// with its window the first leaf of a `row`: the whole tab was split vertically, the new pane
/// on the left. The stale request's readback is in `B` on `B/g.md`: it went to its project.
/// The save holds tab 1 as `Vertical[edit B/f.md, Horizontal[terminal, terminal]]`.
///
/// ## Falsifiers
///
/// | # | edit | predicted red |
/// | --- | --- | --- |
/// | S3a | `open_tab` never calls `set_tab_id` | tab 1's ID reads `unset` |
/// | S3b | one ID is minted per window and given to each of its tabs | the IDs are equal |
/// | S3c | `route` sends a file with an unmapped ID to the current tab as `OpenFile` | the stale readback is in `A` |
/// | S3d | the split-left bind splits the active pane instead of the root | tab 1's layout is a `col` |
#[test]
fn wolf_tab_id_names_its_tab_under_cage() {
    const SMOKE: &str = "wolf_tab_id_names_its_tab_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let a = canonical(SMOKE, &cwd);
    let b = make_dir(SMOKE, smoke_home(&cwd), FO_TAB_B_NAME);
    let b_shown = write_file(SMOKE, &b, "bravoshown.md", "# bravo shown");
    let tab_file = path_in(&b, "bravofromterm.md");
    let stale_file = path_in(&b, "bravostale.md");
    let stale_id = wolf_core::TabId::random().to_string();

    let tabs = vec![
        fo_tab(
            &a,
            Some(split(
                PaneOrientation::Horizontal,
                vec![0.5, 0.5],
                vec![terminal(&a), terminal(&a)],
            )),
            Some(1),
            None,
        ),
        fo_tab(&b, Some(edit(&b_shown)), Some(0), None),
    ];
    let state = smoke_state_dir(&cwd);
    plant(
        SMOKE,
        &state,
        &[WindowState {
            windows: vec![fo_window(tabs, 0)],
        }],
    );

    let id1 = runtime_dir.join("fo_tab_id1.txt");
    let id2 = runtime_dir.join("fo_tab_id2.txt");
    let rc1 = runtime_dir.join("fo_tab_rc1.txt");
    let read1 = runtime_dir.join("fo_tab_read1.txt");
    let read2 = runtime_dir.join("fo_tab_read2.txt");
    for path in [&id1, &id2, &rc1, &read1, &read2] {
        clear_evidence(path, SMOKE);
    }
    let lossy = |path: &Path| path.to_string_lossy().into_owned();
    let run = FileOpenRun {
        smoke: SMOKE,
        cwd: &cwd,
        home: None,
        launch: true,
        body: FO_TAB_ID_BODY,
        env: vec![
            (var("FO_A"), a.clone()),
            (var("FO_B"), b.clone()),
            (var("FO_TAB_FILE"), tab_file.clone()),
            (var("FO_STALE_FILE"), stale_file.clone()),
            (var("FO_STALE_ID"), stale_id.clone()),
            (var("FO_ID1"), lossy(&id1)),
            (var("FO_ID2"), lossy(&id2)),
            (var("FO_RC1"), lossy(&rc1)),
            (var("FO_READ1"), lossy(&read1)),
            (var("FO_READ2"), lossy(&read2)),
        ],
        frozen_name: "wolf_gnome_file_open_tab_id_state",
    };
    let frozen = run_file_open(&run, &runtime_dir);

    let parse_id = |path: &Path, what: &str| -> String {
        let raw = read_trimmed(path, SMOKE);
        let value = raw.strip_prefix("id=").unwrap_or(&raw).to_owned();
        let bytes = value.as_bytes();
        let version4 = wolf_core::TabId::parse(&value).is_some()
            && bytes.get(14) == Some(&b'4')
            && bytes.get(19).is_some_and(|v| b"89ab".contains(v));
        assert!(
            version4,
            "{SMOKE}: {what} read WOLF_TAB_ID as {value:?}; want a lowercase version-4 UUID. \
             `unset` means the tab's core was given no tab ID, so its sessions carry none"
        );
        value
    };
    let first = parse_id(&id1, "tab 1's terminal");
    let rc = read_trimmed(&rc1, SMOKE);
    assert_eq!(
        rc, "rc=0",
        "{SMOKE}: wolf typed into tab 1's terminal must exit 0 once the app takes the request"
    );

    let read = read_probe(SMOKE, &read1);
    assert_eq!(
        (read.cwd.as_str(), read.path.as_str()),
        (a.as_str(), tab_file.as_str()),
        "{SMOKE}: wolf on a file of B's project, typed into tab 1's terminal, must open it in \
         tab 1, whose Neovim is in A (§5.1 rule 1). A readback in B means the tab ID was \
         ignored and the file went to its project. Readback: {read:?}"
    );
    let row_head = format!("['row', [['leaf', {}], ", read.winid);
    assert!(
        read.layout.starts_with(&row_head),
        "{SMOKE}: tab 1 holds only terminals, so the file must get a new pane split off the \
         whole tab on the left (rule 4): its window {} first in a `row` at the root. The layout \
         is {}; a `col` at the root means the active pane was split, not the tab",
        read.winid,
        read.layout
    );

    let read = read_probe(SMOKE, &read2);
    assert_eq!(
        (read.cwd.as_str(), read.path.as_str()),
        (b.as_str(), stale_file.as_str()),
        "{SMOKE}: wolf with a tab ID no tab holds must go to the file's project, B, open in tab \
         2. A readback in A means the stale ID fell back to the current tab. Readback: {read:?}"
    );

    let second = parse_id(&id2, "tab 2's drawer");
    assert_ne!(
        first, second,
        "{SMOKE}: tabs 1 and 2 report the same WOLF_TAB_ID, so the ID is not per tab"
    );
    assert!(
        first != stale_id && second != stale_id,
        "{SMOKE}: premise: the stale ID {stale_id} must be one no tab holds"
    );

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    let tab = saved_tab(SMOKE, &saved, &a);
    let shape = tab.content.root.as_ref().and_then(|root| match root {
        Node::Layout(Layout {
            orientation: PaneOrientation::Vertical,
            children,
            ..
        }) => match children.as_slice() {
            [Node::Pane(Pane::Edit { path, .. }), Node::Layout(Layout {
                orientation: PaneOrientation::Horizontal,
                children: column,
                ..
            })] => Some((
                path.clone(),
                column
                    .iter()
                    .map(|node| matches!(node, Node::Pane(Pane::Terminal { .. })))
                    .collect::<Vec<_>>(),
            )),
            _ => None,
        },
        _ => None,
    });
    assert_eq!(
        shape,
        Some((tab_file.clone(), vec![true, true])),
        "{SMOKE}: tab 1 must save as Vertical[edit {tab_file}, Horizontal[terminal, terminal]]. \
         Its root: {:?}",
        tab.content.root
    );
}

/// S4's directories, siblings of the launch cwd.
const FO_COLD_FRONT_NAME: &str = "focharlie";
const FO_COLD_BACK_NAME: &str = "foecho";

/// **With no app running, `wolf FILE` launches the app beside it, which restores the session
/// before it places the file.** §2, "When Wolf is not running".
///
/// Planted: a front window on `A` holding `Vertical[edit a1, edit a2]` focused on pane 1, and
/// a back window on `C`. The scenario launches nothing: from its cwd, a usable project held by
/// no tab, it runs `wolf A/f.md`, then waits for the ready file of whatever app that started.
///
/// ## Observables
///
/// `wolf` exits 0; exactly two Neovims run, so the launch restored the two windows and added
/// none for the command's directory; the readback from the front window is in `A` on the file
/// with two windows; and the save holds both windows, the front tab as `Vertical[a1, f.md]`.
///
/// ## Falsifiers
///
/// | # | edit | predicted red |
/// | --- | --- | --- |
/// | S4a | Linux `launcher` answers `None` | exit 31 |
/// | S4b | `PaneHandle::open_file` places at once rather than on a ready wake | the readback or the save |
/// | S4c | the launched app keeps the command's working directory | exit 21 |
#[test]
fn a_cold_launch_restores_the_session_before_the_file_under_cage() {
    const SMOKE: &str = "a_cold_launch_restores_the_session_before_the_file_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let front = make_dir(SMOKE, smoke_home(&cwd), FO_COLD_FRONT_NAME);
    let back = make_dir(SMOKE, smoke_home(&cwd), FO_COLD_BACK_NAME);
    let a1 = write_file(SMOKE, &front, "charlieone.md", "# charlie one");
    let a2 = write_file(SMOKE, &front, "charlietwo.md", "# charlie two");
    let file = write_file(SMOKE, &front, "charlieopened.md", "# charlie opened");

    let state = smoke_state_dir(&cwd);
    let planted = WindowState {
        windows: vec![
            fo_window(
                vec![fo_tab(
                    &front,
                    Some(split(
                        PaneOrientation::Vertical,
                        vec![0.5, 0.5],
                        vec![edit(&a1), edit(&a2)],
                    )),
                    Some(1),
                    None,
                )],
                0,
            ),
            fo_window(vec![fo_tab(&back, None, None, None)], 0),
        ],
    };
    plant(SMOKE, &state, std::slice::from_ref(&planted));

    let readback = runtime_dir.join("fo_cold_read.txt");
    clear_evidence(&readback, SMOKE);
    let run = FileOpenRun {
        smoke: SMOKE,
        cwd: &cwd,
        home: None,
        launch: false,
        body: FO_COLD_BODY,
        env: vec![
            (var("FO_A"), front.clone()),
            (var("FO_FILE"), file.clone()),
            (var("FO_READ"), readback.to_string_lossy().into_owned()),
        ],
        frozen_name: "wolf_gnome_file_open_cold_state",
    };
    let frozen = run_file_open(&run, &runtime_dir);

    let read = read_probe(SMOKE, &readback);
    assert_eq!(
        (read.cwd.as_str(), read.path.as_str(), read.wins.as_str()),
        (front.as_str(), file.as_str(), "2"),
        "{SMOKE}: the front window must show the file in A's restored two-pane tab. One window \
         means the file was placed before the restore's layout bound, or the restore did not \
         apply; another directory means the request reached a window the restore did not \
         plant. Readback: {read:?}"
    );

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    let mut dirs: Vec<Vec<String>> = saved
        .windows
        .iter()
        .map(|w| w.tabs.iter().map(|t| t.directory.clone()).collect())
        .collect();
    dirs.sort();
    let mut want = vec![vec![front.clone()], vec![back.clone()]];
    want.sort();
    assert_eq!(
        dirs, want,
        "{SMOKE}: the save must hold the two planted windows and no other. A window on the \
         launch cwd means the launched app restored with the command's directory as its own"
    );
    assert_eq!(
        tab_tails(saved_tab(SMOKE, &saved, &front)),
        Some(vec![tail(&a1), tail(&file)]),
        "{SMOKE}: A's tab must save the file in the active pane 1, pane 0 untouched"
    );
}

/// S5's repository, a sibling of the launch cwd.
const FO_WAIT_REPO_NAME: &str = "fogolf";

/// What S5's legs type: leg 1's commit message, leg 2's abandoned one, and leg 3's text.
const FO_WAIT_MSG1: &str = "landed from a wolf terminal";
const FO_WAIT_MSG2: &str = "abandoned with cq";
const FO_WAIT_TEXT3: &str = "typed before the last pane closed";

/// Runs `git args` in `dir` with no system or global configuration, asserting success.
fn fo_git(smoke: &str, dir: &str, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .envs(fo_git_env())
        .output()
        .unwrap_or_else(|e| panic!("{smoke}: failed to spawn git: {e}"));
    assert!(
        out.status.success(),
        "{smoke}: `git {args:?}` in {dir} failed with {}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The environment every `git` in S5 runs with, the app's terminals included: no system or
/// global configuration, and a fixed identity.
fn fo_git_env() -> Vec<(String, String)> {
    [
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_AUTHOR_NAME", "Wolf Smoke"),
        ("GIT_AUTHOR_EMAIL", "wolf-smoke@example.invalid"),
        ("GIT_COMMITTER_NAME", "Wolf Smoke"),
        ("GIT_COMMITTER_EMAIL", "wolf-smoke@example.invalid"),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value.to_owned()))
    .collect()
}

/// **`wolf --wait` serves `git commit` from a Wolf terminal and from outside Wolf, and reports
/// `0` for `:wq` on a tab's last pane.** §6.
///
/// Planted: one window. Tab 1 is on the launch cwd `A` with one edit pane; tab 2 is on the
/// repository `R`, a sibling of `A` with one commit, as `Vertical[edit X, terminal]` focused on
/// the terminal, and it is selected.
///
/// 1. Typed into tab 2's terminal: `GIT_EDITOR='<wolf> --wait' git commit`. The message file
///    lands in the edit pane by rule 2; the scenario types a message and `:wq`, which closes
///    that pane only.
/// 2. From the scenario shell: `GIT_EDITOR="<wolf> --wait" git -C R commit &`. The file lands
///    in a pane split off the whole tab; the scenario types a message, `:w` and `:cq`.
/// 3. From the scenario shell: `wolf --wait A/notes.md &`. It lands in tab 1, which it selects;
///    the scenario types a line and `:wq` on tab 1's last pane. That closes tab 1 alone
///    (`specs/command-lifecycle.md`), so the app stays alive with tab 2's one Neovim.
///
/// ## Observables
///
/// Leg 1's commit exits 0 and `R`'s newest commit has the typed subject. Leg 2's commit exits
/// non-zero and `R`'s commit count is what leg 1 left. Leg 3's `wolf` exits 0, `notes.md`
/// starts with the typed line, and the app is left running one Neovim. Each leg's readback first
/// shows its file landed.
///
/// ## Falsifiers
///
/// | # | edit | predicted red |
/// | --- | --- | --- |
/// | S5a | `handle_open_requests` drains with `drain_requests` | leg 1, no commit |
/// | S5b | `run_pending_file_opens` drops the wait instead of `wait_on` | leg 1 |
/// | S5c | `WolfCquit`'s waited branch ends with `0` | leg 2 commits |
/// | S5d | delete the `end_waits` loop before `close_tab` | leg 3 exits 1 |
#[test]
fn wolf_wait_serves_git_and_the_last_pane_under_cage() {
    const SMOKE: &str = "wolf_wait_serves_git_and_the_last_pane_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let a = canonical(SMOKE, &cwd);
    let r = make_dir(SMOKE, smoke_home(&cwd), FO_WAIT_REPO_NAME);
    fo_git(SMOKE, &r, &["init", "-q"]);
    fo_git(SMOKE, &r, &["commit", "-q", "--allow-empty", "-m", "seed"]);
    let a_shown = write_file(SMOKE, &a, "alphashown.md", "# alpha shown");
    let notes = write_file(SMOKE, &a, "notes.md", "# alpha notes");
    let r_shown = write_file(SMOKE, &r, "golfshown.md", "# golf shown");
    let message_file = path_in(&r, ".git/COMMIT_EDITMSG");

    let tabs = vec![
        fo_tab(&a, Some(edit(&a_shown)), Some(0), None),
        fo_tab(
            &r,
            Some(split(
                PaneOrientation::Vertical,
                vec![0.5, 0.5],
                vec![edit(&r_shown), terminal(&r)],
            )),
            Some(1),
            None,
        ),
    ];
    plant(
        SMOKE,
        &smoke_state_dir(&cwd),
        &[WindowState {
            windows: vec![fo_window(tabs, 1)],
        }],
    );

    let names = [
        "FO_W_READ1",
        "FO_W_READ2",
        "FO_W_READ3",
        "FO_W_RC1",
        "FO_W_RC2",
        "FO_W_RC3",
        "FO_W_SUBJECT1",
        "FO_W_COUNT1",
        "FO_W_COUNT2",
    ];
    let evidence: Vec<PathBuf> = names
        .iter()
        .map(|name| runtime_dir.join(format!("{}.txt", name.to_lowercase())))
        .collect();
    let mut env = fo_git_env();
    for (name, path) in names.iter().zip(&evidence) {
        clear_evidence(path, SMOKE);
        env.push((var(name), path.to_string_lossy().into_owned()));
    }
    env.extend([
        (var("FO_A"), a.clone()),
        (var("FO_R"), r.clone()),
        (var("FO_W_NOTES"), notes.clone()),
        (var("FO_W_MSG1"), FO_WAIT_MSG1.to_owned()),
        (var("FO_W_MSG2"), FO_WAIT_MSG2.to_owned()),
        (var("FO_W_TEXT3"), FO_WAIT_TEXT3.to_owned()),
    ]);
    let run = FileOpenRun {
        smoke: SMOKE,
        cwd: &cwd,
        home: None,
        launch: true,
        body: FO_WAIT_BODY,
        env,
        frozen_name: "wolf_gnome_file_open_wait_state",
    };
    run_file_open(&run, &runtime_dir);
    let [read1, read2, read3, rc1, rc2, rc3, subject1, count1, count2] =
        [0, 1, 2, 3, 4, 5, 6, 7, 8].map(|i| evidence[i].as_path());

    let read = read_probe(SMOKE, read1);
    assert_eq!(
        (read.cwd.as_str(), read.path.as_str()),
        (r.as_str(), message_file.as_str()),
        "{SMOKE}: premise: leg 1's message file must land in tab 2, on R. Readback: {read:?}"
    );
    assert_eq!(
        (read_trimmed(rc1, SMOKE), read_trimmed(subject1, SMOKE)),
        ("rc=0".to_owned(), FO_WAIT_MSG1.to_owned()),
        "{SMOKE}: leg 1's git commit, typed into tab 2's terminal, must exit 0 and commit the \
         typed subject once :wq closes its pane. rc=1 with the subject seed means wolf --wait \
         exited 1, so the wait was dropped between the listener and the tab's core: drained \
         without its wait, or the pane placed the file and dropped it"
    );

    let read = read_probe(SMOKE, read2);
    assert_eq!(
        (read.cwd.as_str(), read.path.as_str()),
        (r.as_str(), message_file.as_str()),
        "{SMOKE}: premise: leg 2's message file must land in tab 2, on R. Readback: {read:?}"
    );
    let rc = read_trimmed(rc2, SMOKE);
    assert_ne!(
        rc, "rc=0",
        "{SMOKE}: leg 2's git commit must fail once :cq ends its editor with 1; rc=0 means :cq \
         ended the wait as a close"
    );
    assert_eq!(
        read_trimmed(count2, SMOKE),
        read_trimmed(count1, SMOKE),
        "{SMOKE}: leg 2's aborted commit must leave R's commit count where leg 1 left it; one \
         more means the message written with :w was committed"
    );

    let read = read_probe(SMOKE, read3);
    assert_eq!(
        (read.cwd.as_str(), read.path.as_str()),
        (a.as_str(), notes.as_str()),
        "{SMOKE}: premise: leg 3's file must land in tab 1, on A. Readback: {read:?}"
    );
    assert_eq!(
        read_trimmed(rc3, SMOKE),
        "rc=0",
        "{SMOKE}: leg 3's wolf --wait must exit 0 for :wq on tab 1's last pane, though that \
         closes the tab; rc=1 means the tab's close reached the command before the buffer's"
    );
    let written = std::fs::read_to_string(&notes)
        .unwrap_or_else(|e| panic!("{SMOKE}: failed to read {notes}: {e}"));
    assert!(
        written.starts_with(FO_WAIT_TEXT3),
        "{SMOKE}: leg 3's :wq must have written the typed line to {notes}; it holds {written:?}"
    );
}
