//! Session-restore smokes: two launches in one cage, the second reading what the first
//! saved, for the pane layout, a lone file, tabs, two windows and dragged divider weights;
//! one launch over a stale `state.json`, which must be deleted and not restored; and one
//! launch over a planted state of three windows, which must keep their stacking order.

use super::pointer::{
    divider_seed_span, head_width_ratio, BASELINE_TOLERANCE_CELLS, BISECT_STEPS, MIN_DRAG_CELLS,
    MIN_RATIO_OFFSET, RATIO_TOLERANCE,
};
use super::recovery::with_nvim_pid_helper;
use super::*;
use wolf_core::app_state::{Frame, Tab, TabContent};

/// Pixels the single divider is dragged right in
/// [`dragged_divider_weights_survive_relaunch_under_cage`].
///
/// Sized so BOTH panes stay wide enough to render their whole heading without
/// soft-wrapping (`runtime/nvim/init.lua` sets `wrap`/`linebreak`, and a wrapped
/// heading breaks the OCR corroboration): the narrower pane keeps ~15 columns
/// against a 14-character heading.
const DIVIDER_RELAUNCH_DRAG_PX: i64 = 175;

/// The smallest gap between the two PERSISTED weights that no equal-share vector
/// could show. `[0.5, 0.5]` — everything this front-end could write before divider
/// drag — has a gap of exactly zero.
const MIN_WEIGHT_GAP: f64 = 0.15;

/// Tolerance on the persisted weights' sum, which the pure tree renormalizes to 1.
const WEIGHT_SUM_EPS: f64 = 1e-9;

// State-persistence smokes: two launches in one cage; each reaps run 1 to check its exit status.
// See notes/crates/wolf-gnome/tests/smoke_headless/restore.rs.md#relaunch-smoke-exit-codes

/// Extra settle after the two-window smoke's MID-RUN New Window, on top of the usual
/// `SETTLE`.
///
/// A window opened after launch is not covered by the readiness file, which fires
/// once per process, so this sleep is what waits for it. A launch's own restore needs
/// no settle: `wait_ready` holds until no saved layout is pending and no restore owes
/// a page. A timing knob only: no assertion depends on its value, and every
/// observable below is polled for.
const RELAUNCH_RESTORE_SETTLE: &str = "2.0";

// S1 fixtures: three files, three panes, one asymmetric tree. Names are unlike their headings,
// enforced by `restore_fixture_names_cannot_satisfy_their_own_assertions`.

/// The file opened in the TOP-LEFT pane (`rleft.md`).
pub(crate) const RESTORE_FILE_LEFT: &str = "rleft.md";

/// Heading inside [`RESTORE_FILE_LEFT`]; lowercased it contains
/// [`RESTORE_OCR_ALFA`].
const RESTORE_HEADING_LEFT: &str = "# RESTOREALFA";

/// Lowercased substring OCR must find for the restored TOP-LEFT pane.
pub(crate) const RESTORE_OCR_ALFA: &str = "restorealfa";

/// The file opened in the BOTTOM-LEFT pane (`rmid.md`) — DFS position 2.
pub(crate) const RESTORE_FILE_MID: &str = "rmid.md";

/// Heading inside [`RESTORE_FILE_MID`]; lowercased it contains
/// [`RESTORE_OCR_BRAVO`].
const RESTORE_HEADING_MID: &str = "# RESTOREBRAVO";

/// Lowercased substring OCR must find for the restored BOTTOM-LEFT pane.
pub(crate) const RESTORE_OCR_BRAVO: &str = "restorebravo";

/// The file opened in the full-height RIGHT pane (`rright.md`) — DFS position 3.
pub(crate) const RESTORE_FILE_RIGHT: &str = "rright.md";

/// Heading inside [`RESTORE_FILE_RIGHT`]; lowercased it contains
/// [`RESTORE_OCR_CHARLIE`].
const RESTORE_HEADING_RIGHT: &str = "# RESTORECHARLIE";

/// Lowercased substring OCR must find for the restored RIGHT pane.
pub(crate) const RESTORE_OCR_CHARLIE: &str = "restorecharlie";

/// Name of the file the RESTORED Neovim writes its own window list into, in the
/// smoke's temp cwd (so the tree sidebar renders it — hence its inclusion in
/// `RELAUNCH_FIXTURE_NAMES`).
pub(crate) const RESTORE_READBACK_NAME: &str = "restore_windows.txt";

/// The EXACT readback [`pane_layout_survives_relaunch_under_cage`] requires.
///
/// `tabpagebuflist()` returns one buffer per window in WINDOW-NUMBER order, and
/// nvim numbers windows by layout position (a row left-to-right, a column
/// top-to-bottom). For the tree run 1 builds — `row[ col[left-top, left-bottom],
/// right ]` — that is exactly `[rleft, rmid, rright]`, so this single exact
/// comparison proves the pane COUNT, the buffer IDENTITIES and their ORDER at
/// once.
const RESTORE_READBACK_EXPECTED: &str = "rleft.md rmid.md rright.md";

/// The two-launch PANE-LAYOUT RELAUNCH scenario, run as one `sh -c` string inside
/// ONE `cage`.
///
/// **Run 1** launches the app, opens `rleft.md`, and builds a THREE-PANE
/// ASYMMETRIC tree with the same `wtype` chord invocations
/// `VSPLIT_SCENARIO` / `FOCUS_SCENARIO` use:
/// `<C-w>v` (new pane RIGHT, focused) → `:e rright.md` → `<C-w>h` (focus back
/// LEFT) → `<C-w>s` (new pane BELOW the left one, focused) → `:e rmid.md`. It then
/// types `:qa`, POLLS for the process to go, and REAPS it to check the status.
///
/// **Run 2** launches the SAME binary in the SAME working directory with the SAME
/// `WOLF_STATE_DIR` (both inherited from `cage_command`), waits until it is ready for
/// input (`wait_ready`), which covers the restore, and asks the restored Neovim to
/// `writefile` its own `tabpagebuflist()` — the load-bearing observable — before the
/// screenshot.
///
/// See the exit-code table above this const.
const RESTORE_LAYOUT_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for run 1 to EXIT after :qa, and for the
# run-2 readback file to appear.
QUIT_TRIES=75
FILE_TRIES=100

# Teardown, installed BEFORE anything starts so every `exit NN` below runs it.
# Both runs use the same variable; it is cleared once run 1 has been reaped so a
# later failure cannot signal an already-gone pid.
cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

# Poll for a readback file the captive Neovim writes. Nothing downstream may read
# a file we merely hope is there.
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
    echo "wolf-gnome relaunch smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# =========================================================================
# RUN 1 -- build the three-pane asymmetric tree, then quit cleanly.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome relaunch smoke"

# Open file A in the single starting pane.
wtype -- ":e $RESTORE_FILE_LEFT"
wtype -k Return
sleep "$SETTLE"

# VERTICAL split: the new pane opens to the RIGHT and takes focus.
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"

# The new RIGHT pane shows file C.
wtype -- ":e $RESTORE_FILE_RIGHT"
wtype -k Return
sleep "$SETTLE"

# Move focus BACK to the left pane (a pure FocusDir intent -> the shell's
# `PaneTree::neighbor` -> nvim_set_current_win; the FOCUS_SCENARIO precedent).
wtype -M ctrl -k w -m ctrl
wtype -k h
sleep "$SETTLE"

# HORIZONTAL split of the LEFT pane: the new pane stacks BELOW it and takes focus.
# The tree is now row[ col[A, new], C ] -- an asymmetric three-pane shape no fresh
# launch could ever produce.
wtype -M ctrl -k w -m ctrl
wtype -k s
sleep "$SETTLE"

# The new BOTTOM-LEFT pane shows file B.
wtype -- ":e $RESTORE_FILE_MID"
wtype -k Return
sleep "$SETTLE"

# Quit the whole application. `Coordinator::on_window_closing` is the FIRST statement
# in the close-request handler -- while the core and the panes are still alive -- and
# it gathers this window's tabs, commits the window state and blocks on
# `AppStateStore::flush`, so wolf.sqlite holds it before the process goes.
wtype -- ":qa"
wtype -k Return

# POLL (never sleep-and-hope) for run 1 to go away, then REAP it and check its
# EXIT STATUS. `wait` can still report it: the shell retains a background child's
# status until a `wait` asks for it.
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
    echo "wolf-gnome relaunch smoke: run 1 (pid $app_pid) still alive after :qa." >&2
    echo "  The drained close_window command never reached window.close()." >&2
    exit 12
fi
# `set -e` would abort before the status could be reported, so collect it through
# `||` (which is exempt from `set -e`).
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome relaunch smoke: run 1 (pid $app_pid) exited $rc, not 0." >&2
    echo "  It went away, but NOT cleanly. That implicates the close path:" >&2
    echo "  Coordinator::on_window_closing either panicked, or it holds a core.borrow()" >&2
    echo "  across wire_close's core.borrow_mut().take() (a RefCell double-borrow), or" >&2
    echo "  AppStateStore::flush blocked past its timeout." >&2
    exit 13
fi
app_pid=

# =========================================================================
# RUN 2 -- relaunch in the SAME cwd with the SAME state dir, and read back.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 14 "wolf-gnome relaunch smoke"

# THE load-bearing observable: one line holding every window's buffer TAIL name, in
# window order, space separated. `:t` sidesteps any question of how /tmp resolves.
#
# The mapping uses a LAMBDA rather than the usual `'…v:val…'` string expression so
# that no double quote appears anywhere in the typed command: the whole line is
# already inside a double-quoted shell word, and nesting quotes through sh -> wtype
# -> the Neovim cmdline is exactly where these scenarios have historically broken.
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ' ')], '$RESTORE_READBACK')"
wtype -k Return
wait_file "$RESTORE_READBACK" || exit 15

# Capture the live window for the corroborating OCR.
grim "$OUT_PNG"

# The trap tears run 2 down on the way out.
exit 0
"#;

// --- S2 fixtures: ONE file, ONE pane, and a decoy ---------------------------

/// The single file opened in run 1 (`szulu.md`). Its NAME contains neither
/// [`SOLO_OCR_PRESENT`] nor [`SOLO_OCR_ABSENT`], so only a RENDERED BUFFER can
/// satisfy the OCR assertion.
pub(crate) const SOLO_FILE: &str = "szulu.md";

/// Heading inside [`SOLO_FILE`]; lowercased it contains [`SOLO_OCR_PRESENT`].
const SOLO_HEADING: &str = "# SOLOZULU";

/// Lowercased substring OCR MUST find: the restored file's heading.
pub(crate) const SOLO_OCR_PRESENT: &str = "solozulu";

/// The DECOY file, never opened by anything.
///
/// It is alphabetically FIRST in the cwd, so it kills the one plausible
/// alternative hypothesis: if the app were reopening "the first file it found"
/// rather than the saved layout, the readback would be `salfa.md` and the OCR
/// would carry [`SOLO_OCR_ABSENT`]. Both are asserted against.
pub(crate) const SOLO_DECOY_FILE: &str = "salfa.md";

/// Heading inside [`SOLO_DECOY_FILE`]; lowercased it contains
/// [`SOLO_OCR_ABSENT`].
const SOLO_DECOY_HEADING: &str = "# SOLOALFA";

/// Lowercased substring OCR must NOT find: the decoy's heading.
pub(crate) const SOLO_OCR_ABSENT: &str = "soloalfa";

/// Name of the file the RESTORED Neovim writes its current buffer's tail name
/// into, in the smoke's temp cwd.
pub(crate) const SOLO_READBACK_NAME: &str = "solo_buffer.txt";

/// The EXACT readback [`single_pane_file_survives_relaunch_under_cage`] requires.
const SOLO_READBACK_EXPECTED: &str = "szulu.md";

/// The two-launch SINGLE-PANE RELAUNCH scenario, run as one `sh -c` string inside
/// ONE `cage`. Structurally [`RESTORE_LAYOUT_SCENARIO`] with the split chords
/// removed — that absence IS the subject.
///
/// Between the two runs it copies the state database run 1 wrote into
/// `$SOLO_STATE_COPY`, so the Rust side can assert on RUN 1'S SAVE rather than on
/// whatever run 2 later wrote over it. That separation is what keeps the two
/// red-run reverts distinguishable: with a SAVE-side arity guard the copy carries
/// `root: null`, while with a RESTORE-side arity guard the copy is correct and only
/// the readback and the OCR go red.
///
/// See the exit-code table above [`RELAUNCH_RESTORE_SETTLE`]; this scenario adds
/// `16` for "run 1 wrote no wolf.sqlite at all".
const RESTORE_SOLO_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
QUIT_TRIES=75
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
    echo "wolf-gnome single-pane smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# =========================================================================
# RUN 1 -- open ONE file in ONE pane. NO SPLIT. Then quit cleanly.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome single-pane smoke"

# The whole point: a single unsplit pane holding a single file. Under an ARITY
# guard this shape persists its directory and NOTHING else.
wtype -- ":e $SOLO_FILE"
wtype -k Return
sleep "$SETTLE"

wtype -- ":qa"
wtype -k Return

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
    echo "wolf-gnome single-pane smoke: run 1 (pid $app_pid) still alive after :qa." >&2
    exit 12
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome single-pane smoke: run 1 (pid $app_pid) exited $rc, not 0." >&2
    echo "  It went away, but NOT cleanly -- suspect Coordinator::on_window_closing" >&2
    echo "  panicking or holding a core.borrow() across wire_close's" >&2
    echo "  core.borrow_mut().take()." >&2
    exit 13
fi
app_pid=

# --- Freeze RUN 1'S SAVE before run 2 can write over it --------------------
#
# Run 2 re-submits on every render wake, so by the time the Rust side could read
# the live database it would hold RUN 2's window state. Copying here is what lets
# the two red-run reverts produce DISTINGUISHABLE failures.
if ! freeze_state "$SOLO_STATE_SRC" "$SOLO_STATE_COPY"; then
    echo "wolf-gnome single-pane smoke: run 1 wrote no wolf.sqlite in $SOLO_STATE_SRC." >&2
    echo "  Nothing at all was persisted, so this is not the arity guard: suspect" >&2
    echo "  open_state_store() resolving no directory, or WOLF_STATE_DIR not reaching" >&2
    echo "  the app." >&2
    exit 16
fi

# =========================================================================
# RUN 2 -- relaunch in the SAME cwd with the SAME state dir, and read back.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 14 "wolf-gnome single-pane smoke"

# THE load-bearing observable: the tail name of the CURRENT buffer. A broken build
# answers `Untitled.md`; an app that reopened "the first file it found" would
# answer `salfa.md`.
wtype -- ":call writefile([fnamemodify(bufname('%'),':t')], '$SOLO_READBACK')"
wtype -k Return
wait_file "$SOLO_READBACK" || exit 15

grim "$OUT_PNG"
exit 0
"#;

// S5 fixtures: three tabs. No OCR runs, so names may match the compared strings; the fixture
// guard's exemption block says why. They stay in `RELAUNCH_FIXTURE_NAMES` all the same.

/// The file opened in TAB 0 — the tab the relaunch selects.
pub(crate) const TABS_RESTORE_FILE_ONE: &str = "talfa.md";

/// Heading inside [`TABS_RESTORE_FILE_ONE`]. Nothing reads it: this smoke takes no
/// capture, and the fixture bodies exist only so the `:e` opens a real file.
const TABS_RESTORE_HEADING_ONE: &str = "# TABKEEPONE";

/// The file opened in TAB 1 — the tab that is neither the first nor the one selected
/// when run 1 quits, so no shortcut that keeps a single tab can keep it.
pub(crate) const TABS_RESTORE_FILE_TWO: &str = "tbravo.md";

/// Heading inside [`TABS_RESTORE_FILE_TWO`]. See [`TABS_RESTORE_HEADING_ONE`].
const TABS_RESTORE_HEADING_TWO: &str = "# TABKEEPTWO";

/// The file opened in TAB 2 — the last tab created, and the one selected when run 1
/// quits.
pub(crate) const TABS_RESTORE_FILE_THREE: &str = "tcharlie.md";

/// Heading inside [`TABS_RESTORE_FILE_THREE`]. See [`TABS_RESTORE_HEADING_ONE`].
const TABS_RESTORE_HEADING_THREE: &str = "# TABKEEPTHREE";

/// The three fixtures in TAB ORDER — the expectation both the readback and the two
/// frozen window states are compared against.
///
/// Built from the three consts rather than retyped, so the smoke's expectation cannot
/// drift from the files it writes.
pub(crate) const TABS_RESTORE_FILES: [&str; 3] = [
    TABS_RESTORE_FILE_ONE,
    TABS_RESTORE_FILE_TWO,
    TABS_RESTORE_FILE_THREE,
];

/// Name of the file RUN 2's scenario writes its map into: one line `<pid> <fixture>` for
/// every fixture whose swap file a live `nvim` child of the app holds open.
pub(crate) const TABS_RESTORE_READBACK_NAME: &str = "tabs_run2.txt";

/// The two-launch THREE-TAB RELAUNCH scenario, run as one `sh -c` string inside ONE
/// `cage` and composed with [`with_nvim_pid_helper`]. Structurally
/// [`RESTORE_SOLO_SCENARIO`] with two `Ctrl+Shift+T` chords and a second frozen state
/// database.
///
/// **Two databases are frozen, and that is what keeps three reds distinguishable.**
/// `$TABS_STATE_COPY_1` is run 1's save, taken before run 2 can write over it, so the
/// SAVE side is asserted on its own; `$TABS_STATE_COPY_2` is run 2's, taken after a
/// quit in which tabs 1 and 2 were never selected, so what a whole launch-restore-quit
/// round trip leaves behind is asserted on its own. Without the split, a save side
/// that dropped tabs and a second run that lost them would produce the same one
/// observable. The Rust side also reads `$TABS_STATE_COPY_1` before this scenario's
/// exit status, because a save that drops tabs leaves run 2 short of three Neovims as
/// well and fails exit 24's count; the ordering and the frozen copy together keep the
/// save side asserted on its own.
///
/// **Run 2 selects nothing and types nothing but the quit.** Each tab runs a Neovim of
/// its own, so the restore is read from the process table rather than from one
/// Neovim: three live `nvim` children, then which fixture's swap file each holds open,
/// read from `/proc/<pid>/fd`. A Neovim keeps a loaded buffer's swap file open under
/// `WOLF_SWAP_DIR`, its name the file's path with `%` for each `/` and `.swp` after it.
/// Live pids rather than files on disk keep a stale swap from answering for a tab.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | run 1's app died before it was ready for input |
/// | `12` | run 1 was still alive after `:qa` |
/// | `13` | run 1 exited NON-ZERO — it went away, but not cleanly |
/// | `14` | run 2's app died before it was ready for input |
/// | `16` | run 2 was still alive after `:qa` |
/// | `17` | run 2 exited NON-ZERO |
/// | `21` | run 1 did not reach THREE live Neovims, so Ctrl+Shift+T failed and nothing below is about persistence |
/// | `22` | run 1 wrote no `wolf.sqlite` at all |
/// | `23` | run 2 wrote no `wolf.sqlite` at all |
/// | `24` | run 2 did not restore THREE live Neovims |
///
/// **A map short of three rows is deliberately not an exit code.** The map is re-read
/// until it holds three rows or `READBACK_TRIES` is spent, and a map that settled
/// short is left standing for the Rust side's assertion 2 to name, row by row.
const RESTORE_TABS_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO. FILE_TRIES is also
# `wait_nvim_count`'s budget, since that function reads it.
WAIT_INTERVAL=0.2
SETTLE=1.0
QUIT_TRIES=75
FILE_TRIES=100

# How long to wait after each Ctrl+Shift+T. The adw::TabBar ANIMATES its height in
# over roughly a second, and the new tab's Neovim is started before the page is
# appended and selected. Twice the measured animation, the native-tabs smoke's
# constant. A timing knob only: every observable below is polled for or checked with
# its own exit code.
TABS_BAR_SETTLE=2.0

# How many times RUN 2 re-reads the swap map while waiting for each restored Neovim to
# load its file. Each restored tab's Neovim rebuilds its layout on its own first wake,
# so the count of three can come before the last file is open.
READBACK_TRIES=50

cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

# One line `<pid> <fixture>` per fixture swap file a live nvim child of the app holds
# open, on stdout.
swap_map() {
    for p in $(app_children); do
        pid_is_live "$p" || continue
        name=$(sed -n 's/^Name:[[:space:]]*//p' "/proc/$p/status" 2>/dev/null) || continue
        if [ "$name" != "nvim" ]; then
            continue
        fi
        for fd in /proc/"$p"/fd/*; do
            link=$(readlink "$fd" 2>/dev/null) || continue
            for f in "$TABS_FILE_ONE" "$TABS_FILE_TWO" "$TABS_FILE_THREE"; do
                case "$link" in
                    *"%$f.swp") echo "$p $f" ;;
                esac
            done
        done
    done
    return 0
}

# =========================================================================
# RUN 1 -- three tabs, one file each, then quit cleanly.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome three-tab smoke"

# Tab 0: the tab the next launch will select.
wtype -- ":e $TABS_FILE_ONE"
wtype -k Return
sleep "$SETTLE"

# Tab 1: the MIDDLE tab, which the next launch will restore and never select.
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
wtype -- ":e $TABS_FILE_TWO"
wtype -k Return
sleep "$SETTLE"

# Tab 2: the last tab created, selected at the quit.
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
wtype -- ":e $TABS_FILE_THREE"
wtype -k Return
sleep "$SETTLE"

# PRECONDITION, before anything is saved: the run really did reach three tabs, one
# Neovim each. Without this, a one-tab window state cannot be told apart from a run in
# which the chord never opened a tab.
wait_nvim_count 3 21 "after two Ctrl+Shift+T chords in run 1"

# Quit. `Coordinator::on_window_closing` gathers this window -- once per TAB, each
# from its own Neovim -- commits the window state and blocks on
# `AppStateStore::flush`, so wolf.sqlite holds it before the process goes.
wtype -- ":qa"
wtype -k Return

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
    echo "wolf-gnome three-tab smoke: run 1 (pid $app_pid) still alive after :qa." >&2
    exit 12
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome three-tab smoke: run 1 (pid $app_pid) exited $rc, not 0." >&2
    echo "  It went away, but NOT cleanly. Three tabs close in sequence here, each shutting" >&2
    echo "  its own Neovim down, so a borrow discipline that held at one tab is the first" >&2
    echo "  suspect: the close path gathers every tab while its core is alive." >&2
    exit 13
fi
app_pid=

# --- Freeze RUN 1'S SAVE before run 2 can write over it --------------------
if ! freeze_state "$TABS_STATE_SRC" "$TABS_STATE_COPY_1"; then
    echo "wolf-gnome three-tab smoke: run 1 wrote no wolf.sqlite in $TABS_STATE_SRC." >&2
    echo "  Nothing at all was persisted, so this is not the tab gather: suspect" >&2
    echo "  open_state_store() resolving no directory, or WOLF_STATE_DIR not reaching" >&2
    echo "  the app." >&2
    exit 22
fi

# =========================================================================
# RUN 2 -- relaunch in the SAME cwd with the SAME state dir, read the process
# table, and quit WITHOUT EVER SELECTING another tab.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 14 "wolf-gnome three-tab smoke"

wait_nvim_count 3 24 "after run 2 restored the saved three-tab session"

# THE load-bearing observable: which restored Neovim holds which fixture open.
# Re-read until three rows, or the budget is spent; a short map is assertion 2's red.
nl='
'
rb=0
rows=0
while [ "$rb" -lt "$READBACK_TRIES" ]; do
    map=$(swap_map)
    rb=$((rb + 1))
    rows=$(printf '%s' "$map" | grep -c . || true)
    if [ "$rows" -ge 3 ]; then
        break
    fi
    sleep "$WAIT_INTERVAL"
done
printf '%s%s' "$map" "$nl" > "$TABS_READBACK"
echo "wolf-gnome three-tab smoke: run 2 swap map has $rows row(s) after $rb read(s):" >&2
cat "$TABS_READBACK" >&2

# NOTHING selects another tab: no chord, no click. Tabs 1 and 2 keep whatever the
# restore gave them, which is the whole subject of the third assertion.
wtype -- ":qa"
wtype -k Return

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
    echo "wolf-gnome three-tab smoke: run 2 (pid $app_pid) still alive after :qa." >&2
    exit 16
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome three-tab smoke: run 2 (pid $app_pid) exited $rc, not 0." >&2
    echo "  It went away, but NOT cleanly. Run 2 is the one that RESTORED tabs, so suspect" >&2
    echo "  window::build opening the planned tabs, each starting a core of its own." >&2
    exit 17
fi
app_pid=

if ! freeze_state "$TABS_STATE_SRC" "$TABS_STATE_COPY_2"; then
    echo "wolf-gnome three-tab smoke: run 2 left no wolf.sqlite in $TABS_STATE_SRC." >&2
    echo "  Run 1's copy is already frozen, so the save side is provable without this one --" >&2
    echo "  what is missing is assertion 3's evidence. Suspect the same causes as exit 22." >&2
    exit 23
fi

exit 0
"#;

// S4 fixtures: two files, one dragged divider, two launches. Names contain neither asserted term,
// enforced over `RELAUNCH_FIXTURE_NAMES`.

/// The file opened in the LEFT pane (`dvleft.md`) — the pane the drag widens.
pub(crate) const DIVIDER_FILE_LEFT: &str = "dvleft.md";

/// Heading inside [`DIVIDER_FILE_LEFT`]; lowercased it contains
/// [`DIVIDER_OCR_LEFT`].
const DIVIDER_HEADING_LEFT: &str = "# DIVIDERALFA";

/// Lowercased substring OCR must find for the restored LEFT pane.
pub(crate) const DIVIDER_OCR_LEFT: &str = "divideralfa";

/// The file opened in the RIGHT pane (`dvright.md`) — the pane the drag narrows.
pub(crate) const DIVIDER_FILE_RIGHT: &str = "dvright.md";

/// Heading on line 1 of [`DIVIDER_FILE_RIGHT`]; lowercased it contains
/// [`DIVIDER_OCR_RIGHT`]. [`divider_right_fixture_body`] writes it above
/// [`DIVIDER_BODY_LINE_COUNT`] body lines, for the page-density reason recorded
/// there.
pub(crate) const DIVIDER_HEADING_RIGHT: &str = "# DIVIDERBRAVO";

/// Lowercased substring OCR must find for the restored RIGHT pane. It is 12
/// characters, 14 with the heading marker, which is what
/// [`DIVIDER_RELAUNCH_DRAG_PX`] is sized against: a narrower pane would soft-wrap
/// it and OCR would read two fragments.
///
/// Soft-wrapping is not the only way this term can go unread in a narrow pane, and
/// it was not the first one met: see [`DIVIDER_BODY_LINE_COUNT`].
pub(crate) const DIVIDER_OCR_RIGHT: &str = "dividerbravo";

/// How many body lines [`divider_right_fixture_body`] writes below
/// [`DIVIDER_HEADING_RIGHT`].
///
/// FIXTURE, and load-bearing for the divider relaunch smoke's OCR assertion rather
/// than decoration.
///
/// `tesseract --psm 11` groups a sparse pane into ONE region and estimates a scale
/// from it, so anything long, thin and high-contrast sharing that region corrupts
/// the reading of a 15-cell heading it can plainly see. ONE such neighbour was
/// measured in this pane: the `~` end-of-buffer column, which this constant answers.
///
/// MEASURED for the `~` column, with a heading and nothing else in the file: the
/// narrow pane renders that one short line above a bare vertical column of markers,
/// and the heading read as `4 DIVIDERBR=I-\V0`. The glyphs were not the problem —
/// the `A` bitmaps in the two panes are pixel-identical bar a two-pixel offset, and
/// the SAME pixels moved into the wide pane read `DIVIDERBRAVO` correctly. Three
/// controlled ablations on the retained PNG isolated it: masking out the block
/// cursor and padding the image both left the misread in place, while blanking the
/// `~` column alone — everything else untouched — fixed it. It is the hazard
/// [`SIDEBAR_FILLER_COUNT`] exists for, one pane wide instead of one page wide, and
/// it takes the same fix: give tesseract ordinary text at the ordinary scale to
/// calibrate against.
///
/// Twenty-four lines plus the heading is comfortably more than the measured
/// `winheight(0)` of 14, so no `~` marker is drawn in that pane at all — a later
/// run's retained PNG shows the 24 body lines and no marker anywhere. RAISING IT
/// BUYS NOTHING against a red OCR assertion: the column it targets is already gone.
/// The other lever, shrinking [`DIVIDER_RELAUNCH_DRAG_PX`], is wrong — it would
/// leave the `~` column exactly where it is and narrow the weight gap the smoke's
/// first assertion rests on.
///
/// With that answered, a second cause remained and is not a fixture problem
/// at all: `--psm 11` misclassified one glyph of the heading at 15 columns, on
/// pixels a second segmentation mode read correctly. That one is answered by
/// [`ocr_uniform_block_to_lowercase`], the reader the divider relaunch smoke adds
/// for itself, and it is the first thing to suspect if the OCR assertion goes red —
/// not this constant.
pub(crate) const DIVIDER_BODY_LINE_COUNT: usize = 24;

/// How many `x`s each [`divider_right_fixture_body`] line carries after its
/// three-digit ordinal.
///
/// Nine, so every body line is twelve characters — inside the fifteen columns the
/// drag leaves the right pane, so `runtime/nvim/init.lua`'s `wrap` / `linebreak`
/// never soft-wrap one and each buffer line occupies exactly one screen row. A
/// wrapped body would fill the pane just as well; an UNWRAPPED one is one fewer
/// thing that differs between the two runs when the OCR assertion goes red.
const DIVIDER_BODY_LINE_PAD: usize = 9;

/// Builds the contents of [`DIVIDER_FILE_RIGHT`]: [`DIVIDER_HEADING_RIGHT`], then
/// [`DIVIDER_BODY_LINE_COUNT`] lines of a three-digit ordinal followed by
/// [`DIVIDER_BODY_LINE_PAD`] `x`s — the shape `restart_fixture_body` and the
/// `fill_buffer` fixture in [`POINTER_PREAMBLE`] already use.
///
/// The trailing newline is [`write_relaunch_fixtures`]'s, so the file ends on a
/// body line rather than on a blank one.
///
/// ## Why the body may only be digits and `x`
///
/// A fixture whose CONTENT carried an asserted term would satisfy an OCR assertion
/// on its own — the hazard `PICKER_FILE_NAME` documents, aimed at content rather
/// than at a file name, and the reason this body is deliberately not word-like.
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` enforces it
/// mechanically against `ASSERTED_OCR_TERMS` rather than leaving it here as
/// prose.
pub(crate) fn divider_right_fixture_body() -> String {
    let mut body = String::from(DIVIDER_HEADING_RIGHT);
    for line in 1..=DIVIDER_BODY_LINE_COUNT {
        body.push_str(&format!("\n{line:03}{}", "x".repeat(DIVIDER_BODY_LINE_PAD)));
    }
    body
}

/// Name of the file RUN 1's Neovim writes its pre-drag pane widths into, in the
/// smoke's temp cwd (so the tree sidebar renders it — hence its registration in
/// `RELAUNCH_FIXTURE_NAMES`). Its readback is the precondition that separates "the
/// press missed the divider handle" from "the weight was not written back".
pub(crate) const DIVIDER_W0_NAME: &str = "dv_widths_zero.txt";

/// Name of the file RUN 1's Neovim writes its post-drag pane widths into. See
/// [`DIVIDER_W0_NAME`].
pub(crate) const DIVIDER_W1_NAME: &str = "dv_widths_one.txt";

/// Name of the file RUN 2's Neovim writes its restored pane widths into. See
/// [`DIVIDER_W1_NAME`].
pub(crate) const DIVIDER_W2_NAME: &str = "dv_widths_two.txt";

/// The two-launch DIVIDER-WEIGHT RELAUNCH scenario, run as one `sh -c` string
/// inside ONE `cage`.
///
/// ## Why it duplicates the pointer plumbing instead of reusing it
///
/// [`POINTER_PREAMBLE`] launches the app itself, and a two-run scenario cannot use
/// it: the helper must outlive run 1 while the app must not. So the FIFO, the
/// helper launch, `exec 3>`, `hdead`, `hwait_ready`, `hsend`, `hstep`, `wait_file`
/// and the trapped cleanup are duplicated here, exactly as
/// [`RESTORE_SOLO_SCENARIO`] duplicates [`RESTORE_LAYOUT_SCENARIO`]'s poll-and-reap
/// machinery. The cleanup kills the HELPER first, so the app's shutdown is never
/// racing a pointer event.
///
/// Between the two runs it freezes run 1's state database into `$DIV_STATE_COPY`,
/// following the [`RESTORE_SOLO_SCENARIO`] precedent: run 2 re-submits on every
/// render wake, so the live database would carry RUN 2's window state by the time the
/// Rust side could read it — and the save-side and restore-side red runs would then
/// be indistinguishable.
///
/// ## Exit codes
///
/// The first five are [`POINTER_PREAMBLE`]'s, because the functions that raise them
/// are copied from it verbatim. The relaunch-specific ones are renumbered from the
/// shared state-persistence table to avoid colliding with those.
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | run 1's app died before it was ready for input |
/// | `12` | `wolf-pointer-inject` never printed its `ready …` line |
/// | `13` | a helper command was never acked within budget |
/// | `14` | a Neovim readback file never appeared |
/// | `15` | the helper died mid-scenario |
/// | `16` | run 1 was still alive after the `:qa` poll budget |
/// | `17` | run 1 exited NON-ZERO — it went away, but not cleanly |
/// | `18` | run 1 wrote no `wolf.sqlite` at all |
/// | `19` | run 2's app died before it was ready for input |
/// | `21` | the bisection seeds did not straddle the divider |
const DIVIDER_RELAUNCH_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
ACK_TRIES=75
FILE_TRIES=100
POINTER_STEP=0.2
QUIT_TRIES=75

# Teardown, installed BEFORE anything is started. The HELPER dies first: it is the
# only process that can still inject, so killing it first means neither run's
# shutdown is ever racing a pointer event.
cleanup() {
    set +e
    if [ -n "${helper_pid:-}" ]; then
        kill "$helper_pid" 2>/dev/null
        wait "$helper_pid" 2>/dev/null
    fi
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

mkfifo "$CMD_FIFO"
wolf-pointer-inject < "$CMD_FIFO" > "$ACK_FILE" 2> "$HELPER_ERR" &
helper_pid=$!

# Hold the FIFO's write end open for the WHOLE scenario on fd 3: without it every
# `printf > "$CMD_FIFO"` would close it and hand the helper a spurious EOF, which
# it exits on by design. This `exec` is also what unblocks the helper's own read.
exec 3>"$CMD_FIFO"
seq=0

hdead() {
    if kill -0 "$helper_pid" 2>/dev/null; then
        return 1
    fi
    return 0
}

hwait_ready() {
    t=0
    while [ "$t" -lt "$ACK_TRIES" ]; do
        if grep -q '^ready ' "$ACK_FILE" 2>/dev/null; then
            grep '^ready ' "$ACK_FILE" > "$READY_FILE" || true
            return 0
        fi
        if hdead; then
            echo "wolf-gnome divider relaunch smoke: wolf-pointer-inject exited before printing its readiness line." >&2
            cat "$HELPER_ERR" >&2
            exit 15
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome divider relaunch smoke: wolf-pointer-inject never printed 'ready ...' within budget." >&2
    cat "$HELPER_ERR" >&2
    exit 12
}

# Send one command and BLOCK until it is acked. The ack follows a
# wl_display_roundtrip, so it means the compositor has PROCESSED the event.
hsend() {
    seq=$((seq + 1))
    printf '%s\n' "$*" >&3
    t=0
    while [ "$t" -lt "$ACK_TRIES" ]; do
        if grep -qx "ok $seq" "$ACK_FILE" 2>/dev/null; then
            return 0
        fi
        if hdead; then
            if grep -qx "ok $seq" "$ACK_FILE" 2>/dev/null; then
                return 0
            fi
            echo "wolf-gnome divider relaunch smoke: wolf-pointer-inject died while handling command #$seq ($*)." >&2
            cat "$HELPER_ERR" >&2
            exit 15
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome divider relaunch smoke: command #$seq ($*) was never acked within budget." >&2
    cat "$HELPER_ERR" >&2
    exit 13
}

# `hsend`, then pace before the next event of the SAME gesture. Every event of
# every gesture below goes through this, never through bare `hsend`; see `hstep` in
# POINTER_PREAMBLE for the measured reason.
hstep() {
    hsend "$@"
    sleep "$POINTER_STEP"
}

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
    echo "wolf-gnome divider relaunch smoke: readback file $f never appeared (or stayed empty)." >&2
    cat "$HELPER_ERR" >&2
    return 1
}

# probe_buffer <x>: click at (<x>, $PROBE_Y) and leave the TAIL NAME of the buffer
# in the focused window in $focused.
#
# The buffer name rather than the window number, because each pane here holds a
# distinctly-named file: that makes the bisection's precondition name the actual
# panes ("the low seed must focus dvleft.md"), and it is immune to nvim numbering
# its windows in an order this front-end does not draw them in.
#
# A click that lands ON the divider handle reaches no pane and changes nothing,
# which is what the reset click in `bisect_divider` exists for. See
# POINTER_DIVIDER_BODY's `probe_winnr` for the one recorded hazard of clicking a
# handle: `gtk::Paned` binds Return on a FOCUSED handle, so an exit 14 immediately
# after a probe is the thing to check first.
probe_buffer() {
    rm -f "$PROBE_FILE"
    hstep motion "$1" "$PROBE_Y"
    hstep press "$BTN_LEFT"
    hstep release "$BTN_LEFT"
    sleep "$POINTER_STEP"
    wtype -- ":call writefile([fnamemodify(bufname('%'), ':t')], '$PROBE_FILE')"
    wtype -k Return
    wait_file "$PROBE_FILE" || exit 14
    focused=$(cat "$PROBE_FILE")
}

# bisect_divider <lo> <hi>: locate the single divider's handle. Sets $divider_x.
#
# THE RESET CLICK IS LOAD-BEARING: a probe that lands on the handle focuses no
# window at all, so it reports whatever the PREVIOUS probe focused. Re-focusing the
# low side before every probe is what stops a handle hit being read as "the high
# side" and converging the search on the wrong edge.
bisect_divider() {
    lo=$1
    hi=$2
    probe_buffer "$lo"
    lo_buf=$focused
    probe_buffer "$hi"
    hi_buf=$focused
    if [ "$lo_buf" != "$DIVIDER_FILE_LEFT" ] || [ "$hi_buf" != "$DIVIDER_FILE_RIGHT" ]; then
        echo "wolf-gnome divider relaunch smoke: the bisection seeds do not straddle the divider." >&2
        echo "  Seed $lo focused '$lo_buf' (expected $DIVIDER_FILE_LEFT) and seed $hi focused" >&2
        echo "  '$hi_buf' (expected $DIVIDER_FILE_RIGHT). Candidate causes, in order:" >&2
        echo "    1. the PER-PANE CHROME changed (editor_inset::EDITOR_PADDING_PX or the statusbar" >&2
        echo "       strip's height), which moves the text grid inside every pane;" >&2
        echo "    2. the editor pane's ORIGIN moved (an adw::HeaderBar height or sidebar width" >&2
        echo "       change) -- click_positions_the_cursor_under_cage derives it and fails first;" >&2
        echo "    3. the measured CELL SIZE changed, so the panes are not the size modelled;" >&2
        echo "    4. the divider HANDLE is far wider than the theme's one pixel;" >&2
        echo "    5. a fresh split no longer produces EQUAL shares, which is what the seeds model" >&2
        echo "       -- geometry::equal_weights_give_equal_widths is the unit test for that half." >&2
        echo "  An empty name means the click focused a pane holding no file at all, i.e. the :e" >&2
        echo "  never landed." >&2
        exit 21
    fi
    steps=0
    while [ "$steps" -lt "$BISECT_STEPS" ]; do
        mid=$(( (lo + hi) / 2 ))
        probe_buffer "$lo"
        probe_buffer "$mid"
        if [ "$focused" = "$hi_buf" ]; then
            hi=$mid
        else
            lo=$mid
        fi
        steps=$((steps + 1))
    done
    divider_x=$lo
}

# read_widths <file>: one `<columns>:<buffer tail>` pair per window, space
# separated, in whatever order Neovim enumerates its windows.
#
# `winwidth()`, NEVER `nvim_win_get_width()`, and that difference IS the
# observable. MEASURED against the Linux flake's nvim 0.12.6 with a real
# ext_multigrid UI attached: `nvim_ui_try_resize_grid` -- which is what `WolfCore::resize_grid`
# issues, and the only thing a divider drag ever makes reach Neovim -- changes a
# window's GRID width and leaves its LAYOUT width untouched. Read on both sides of
# this smoke's own drag:
#
#     before the drag  dvright nvim_win_get_width=22 winwidth=21 | dvleft nvim_win_get_width=22 winwidth=21
#     after the drag   dvright nvim_win_get_width=22 winwidth=13 | dvleft nvim_win_get_width=22 winwidth=28
#
# So `nvim_win_get_width` is BLIND to every drag this smoke performs: a readback
# built on it reports identical widths before and after, which is exactly how this
# smoke first failed (22/22 cells either side of a drag the screenshot showed
# plainly). Worse than insensitive, it also drifts on its own across the relaunch
# -- 39/5 for panes drawn 679 and 349 px wide -- so it is not a quantity any
# assertion here may rest on.
#
# Keyed by BUFFER NAME, not by window number: run 1 builds its split with an
# interactive `<C-w>v` while run 2's is rebuilt by `wolf_rebuild_layout`, and nvim
# need not number the same pane the same way in both. `nvim_win_get_number` turns
# each handle into the number `winwidth` wants, so the enumeration stays
# order-independent, and staying inside the nvim_* API keeps `$` out of the typed
# command.
read_widths() {
    rm -f "$1"
    wtype -- ":call writefile([join(map(nvim_tabpage_list_wins(0), {_, w -> winwidth(nvim_win_get_number(w)) . ':' . fnamemodify(bufname(nvim_win_get_buf(w)), ':t')}), ' ')], '$1')"
    wtype -k Return
    wait_file "$1" || exit 14
}

# ---------------------------------------------------------------------------
# Bring the helper up, THEN run 1's app (so the seat already carries pointer
# capability when GTK does its first wl_seat roundtrip).
hwait_ready

# =========================================================================
# RUN 1 -- two panes, one dragged divider, then a clean quit.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome divider relaunch smoke"

# One in-pane motion so wl_pointer.enter and a first motion are delivered before
# anything depends on them.
hsend motion "$PROBE_X" "$PROBE_Y"
sleep "$SETTLE"

wtype -- ":e $DIVIDER_FILE_LEFT"
wtype -k Return
sleep "$SETTLE"

# Vertical split: the new pane is drawn to the RIGHT of its target and takes focus.
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"

wtype -- ":e $DIVIDER_FILE_RIGHT"
wtype -k Return
sleep "$SETTLE"

read_widths "$DIV_W0"

bisect_divider "$D_LO" "$D_HI"

# THE DRAG. `hstep` for every event of the gesture.
hstep motion "$divider_x" "$PROBE_Y"
hstep press "$BTN_LEFT"
hstep motion "$(( divider_x + RELAUNCH_DRAG_PX ))" "$PROBE_Y"
hstep release "$BTN_LEFT"
sleep "$SETTLE"

read_widths "$DIV_W1"

# Quit the whole application. `Coordinator::on_window_closing` is the FIRST statement
# in the close-request handler and blocks on `AppStateStore::flush`, so wolf.sqlite
# holds the window state before the process goes.
wtype -- ":qa"
wtype -k Return

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
    echo "wolf-gnome divider relaunch smoke: run 1 (pid $app_pid) still alive after :qa." >&2
    exit 16
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome divider relaunch smoke: run 1 (pid $app_pid) exited $rc, not 0." >&2
    echo "  It went away, but NOT cleanly -- suspect Coordinator::on_window_closing" >&2
    echo "  panicking or holding a core.borrow() across wire_close's" >&2
    echo "  core.borrow_mut().take()." >&2
    exit 17
fi
app_pid=

# --- Freeze RUN 1'S SAVE before run 2 can write over it --------------------
if ! freeze_state "$DIV_STATE_SRC" "$DIV_STATE_COPY"; then
    echo "wolf-gnome divider relaunch smoke: run 1 wrote no wolf.sqlite in $DIV_STATE_SRC." >&2
    echo "  Nothing at all was persisted, so this is not the weight write-back: suspect" >&2
    echo "  open_state_store() resolving no directory, or WOLF_STATE_DIR not reaching" >&2
    echo "  the app." >&2
    exit 18
fi

# =========================================================================
# RUN 2 -- relaunch in the SAME cwd with the SAME state dir, and read back.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 19 "wolf-gnome divider relaunch smoke"

read_widths "$DIV_W2"

# Move focus to the WIDE left pane, AFTER the widths above are read and before the
# screenshot.
#
# The narrow right pane is what the OCR assertion reads, and `--psm 11` scales a
# sparse region from everything in it, so that pane is given as little non-text ink
# as possible: this moves the active-pane treatment -- the strip's bold,
# full-strength filename, drawn at the top of the pane directly above the heading --
# and Neovim's block cursor into the WIDE pane. `notes/wolf-gnome/window-structure.md`
# records why nothing else is drawn at a pane's edge, and what one would cost this
# reading.
#
# `<C-w>h` is a pure FocusDir intent (the shell's `PaneTree::neighbor` ->
# nvim_set_current_win; the FOCUS_SCENARIO precedent). It resizes nothing, so the
# widths just read stay true whatever it does, and if focus is already on the left
# pane it is a no-op -- the same end state either way.
#
# This MOVES that treatment onto the left pane rather than removing it from the
# capture, so if the OCR assertion fails on DIVIDER_OCR_LEFT instead, this line is
# the first suspect and the next lever is known: give dvleft.md the body dvright.md
# already carries. The left pane is twice as wide and holds a bare heading over a
# `~` column while reading correctly, and several sibling smokes OCR a heading
# inside a focused pane, so that treatment alone is not expected to bite there --
# but that is inference, and only this pane has been ablated.
wtype -M ctrl -k w -m ctrl
wtype -k h
sleep "$SETTLE"

grim "$OUT_PNG"
hsend quit
exit 0
"#;

// Two-window fixtures: under cage no capture can show two windows, so this smoke reads no pixels.
// See notes/crates/wolf-gnome/tests/smoke_headless/restore.rs.md#two-window-smoke-fixtures

/// Window A's first tab's file, in the temp cwd, the one project every tab of the smoke is in.
pub(crate) const TWOWIN_A_FIRST_FILE: &str = "wafirst.md";

/// The left pane of window A's second tab, the tab A has selected at the quit.
pub(crate) const TWOWIN_A_LEFT_FILE: &str = "waleft.md";

/// The right pane of window A's second tab, split off with `<C-w>v` and zoomed. It is
/// depth-first pane 1, so a saved zoom on the wrong pane, or on pane 0 by default, shows.
pub(crate) const TWOWIN_A_ZOOMED_FILE: &str = "waright.md";

/// Window B's first tab's file.
pub(crate) const TWOWIN_B_FIRST_FILE: &str = "wbfirst.md";

/// Window B's second tab's file. That tab is B's active one at the quit, so a restore that
/// selects a window's first tab is told apart from one that selects its saved active tab.
pub(crate) const TWOWIN_B_ACTIVE_FILE: &str = "wbactive.md";

/// Every fixture of the smoke with the heading written into it. Nothing reads a heading:
/// this smoke takes no capture, and the bodies exist only so each `:e` opens a real file.
const TWOWIN_FIXTURES: [(&str, &str); 5] = [
    (TWOWIN_A_FIRST_FILE, "# WINALFA"),
    (TWOWIN_A_LEFT_FILE, "# WINBRAVO"),
    (TWOWIN_A_ZOOMED_FILE, "# WINCHARLIE"),
    (TWOWIN_B_FIRST_FILE, "# WINDELTA"),
    (TWOWIN_B_ACTIVE_FILE, "# WINECHO"),
];

/// The query typed into the palette to open the second window.
///
/// **Carried VERBATIM from `palette::tests::NEW_WINDOW_PALETTE_QUERY`** — two
/// lowercase words separated by one ASCII space — because the `smoke_headless` test
/// binary is a separate crate and cannot import it. The join that keeps the two from
/// drifting is `palette::tests::the_new_window_palette_query_selects_the_new_window_row`,
/// which asserts in the fast suite that this string matches exactly one candidate
/// before the classification filter runs and that the single actionable row it
/// leaves carries `Command::NewWindow`.
///
/// The second term is what does the work. `rank_matches` requires every
/// whitespace-separated term to be a case-insensitive subsequence of the row's
/// label, and `window` is not a subsequence of `New` — so this query DROPS the row
/// that `PALETTE_QUERY` selects rather than merely out-ranking it. A one-word query
/// cannot separate them: `New` is a prefix of `New Window`, so every query matching
/// one matches the other.
pub(crate) const TWOWIN_NEW_WINDOW_QUERY: &str = "new window";

/// Name of the file window B's Neovim writes two lines into, BEFORE the `:cd`:
/// `getcwd()` and then the tail of `bufname('%')`. It lands in the temp cwd, so the
/// tree sidebar draws it — hence its registration in `RELAUNCH_FIXTURE_NAMES`.
///
/// The first line is the smoke's no-inheritance evidence: a window opened by New
/// Window starts at `$HOME` with no project and must not inherit the invoking
/// window's directory. The second is what keeps that evidence attributable — window
/// A has [`TWOWIN_A_ZOOMED_FILE`] current and a new window has an unnamed buffer
/// whatever directory it was given, so a readback naming one of A's files says the
/// keystrokes reached the WRONG window and gets an exit code of its own.
pub(crate) const TWOWIN_READBACK_B_NAME: &str = "wtwo_bcwd.txt";

/// Name of the file the FOCUSED restored window's Neovim writes after the
/// relaunch: two lines, `getcwd()` then the tail of `bufname('%')`.
///
/// Two lines rather than one joined string, so neither value has to be split back
/// out of a separator a path could contain.
pub(crate) const TWOWIN_READBACK_R2_NAME: &str = "wtwo_run2.txt";

/// The TWO-WINDOW scenario, run as one `sh -c` string inside ONE `cage`, with
/// [`with_nvim_pid_helper`] prepended for `find_nvim_pid` / `wait_nvim_count`.
///
/// Structurally [`RESTORE_TABS_SCENARIO`] — two launches, a poll-and-reap of each,
/// a frozen state database after each — with three differences that are the
/// subject: a second window is opened from the palette inside run 1, the quit is
/// the `Ctrl+Shift+Q` chord rather than `:qa`, and the window focused last in run 1
/// is the one opened second, so the saved stacking order differs from the order the
/// windows were opened in.
///
/// Run 1 builds window A with two tabs, the second split and zoomed, then opens
/// window B from the palette, `:cd`s B's empty tab into the same project, and gives
/// B a second tab, which is B's active one when the chord quits. Run 2 launches in
/// the project, has the focused window write its `getcwd()` and current buffer, and
/// quits with the chord again.
///
/// ## Ctrl+Shift+Q is not a stylistic choice
///
/// It is the ONLY exercise `app.quit` gets anywhere in this suite. Every other
/// quit-shaped smoke here (`:q`, `:qa`, Ctrl+Shift+W) reaches Neovim or the
/// `win.close-window` action, so the `app.`/`win.` install split — the coordinator
/// installing the two `app.` rows exactly once per process while every window's
/// shell installs only `win.` rows — and `Coordinator::quit`'s ordering are
/// otherwise executed by nothing. Delete the body of
/// `Coordinator::install_app_actions` and the chord finds no action to activate,
/// falls through to Neovim as a plain `<C-Q>`, the app stays alive, and the exit-12
/// guard below fires.
///
/// ## Exit codes
///
/// `11`–`16` are the shared state-persistence vocabulary documented above
/// `RELAUNCH_RESTORE_SETTLE`; the rest are this scenario's own. This scenario
/// calls none of `FIND_NVIM_PID_HELPER`'s exiting functions — only
/// `find_nvim_pid`, through `wait_nvim_count`, which never exits on its own — so
/// that fragment's `16` / `17` / `18` / `20` are unreachable here and `16` means
/// what the state-persistence table says it means.
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | run 1's app died before it was ready for input |
/// | `12` | run 1 was still alive after `Ctrl+Shift+Q` — the chord reached no `app.quit` |
/// | `13` | run 1 exited NON-ZERO: it went away, but not cleanly |
/// | `14` | run 2's app died before it was ready for input |
/// | `15` | a Neovim readback file never appeared |
/// | `16` | run 1 wrote no `wolf.sqlite` at all |
/// | `21` | run 1 did not have EXACTLY THREE live `nvim` children after the palette activation |
/// | `22` | the new window INHERITED the invoking window's project directory |
/// | `23` | run 2 did not have EXACTLY FOUR live `nvim` children after the restore |
/// | `26` | step 4's readback came from the INVOKING window — a focus failure, not an inheritance |
/// | `27` | run 1 did not have EXACTLY TWO live `nvim` children after window A's Ctrl+Shift+T |
/// | `28` | run 1 did not have EXACTLY FOUR live `nvim` children after window B's Ctrl+Shift+T |
/// | `29` | run 2 was still alive after `Ctrl+Shift+Q` |
/// | `30` | run 2 exited NON-ZERO |
/// | `31` | run 2 left no `wolf.sqlite` |
const TWOWIN_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO. FILE_TRIES is also
# `wait_nvim_count`'s budget, since that function reads it.
WAIT_INTERVAL=0.2
SETTLE=1.0
QUIT_TRIES=75
FILE_TRIES=100

# How long to wait after each Ctrl+Shift+T: the three-tab smoke's TABS_BAR_SETTLE, for
# its reason. A timing knob only: every observable below is polled for.
TABS_BAR_SETTLE=2.0

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
    echo "wolf-gnome two-window smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

nvim_ex() {
    wtype -- ":$1"
    wtype -k Return
    sleep "$SETTLE"
}

# Quits the APPLICATION with Ctrl+Shift+Q, polls (never sleep-and-hope) for it to go,
# then REAPS it and checks its EXIT STATUS: a process that vanished because the GTK
# main loop panicked is just as gone as one that shut down cleanly. $1 is the exit
# code for "still alive", $2 for "exited non-zero", $3 the run's label.
quit_app() {
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
        echo "wolf-gnome two-window smoke: $3 (pid $app_pid) is still alive after Ctrl+Shift+Q." >&2
        echo "  That chord is the ONLY app.quit exercise in this suite. The first thing to check" >&2
        echo "  is whether the action exists at all: Coordinator::install_app_actions installs the" >&2
        echo "  two app.-namespace rows exactly once per process, and with no app.quit installed" >&2
        echo "  the chord controller finds the action disabled, declines the key, and Neovim gets" >&2
        echo "  a plain <C-Q> -- which does nothing and leaves the app running." >&2
        echo "  The other candidates: Coordinator::quit not closing every window the APPLICATION" >&2
        echo "  holds, or a window's close-request handler refusing." >&2
        exit "$1"
    fi
    # `set -e` would abort before the status could be reported, so collect it through
    # `||` (which is exempt from `set -e`).
    rc=0
    wait "$app_pid" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "wolf-gnome two-window smoke: $3 (pid $app_pid) exited $rc, not 0." >&2
        echo "  It went away, but NOT cleanly. Suspect a panic in Coordinator::quit or" >&2
        echo "  Coordinator::on_window_closing (which gather, commit and block on the flush" >&2
        echo "  first), a core.borrow() held across window::wire_close's" >&2
        echo "  core.borrow_mut().take(), or AppStateStore::flush blocking past its timeout." >&2
        echo "  With TWO windows and four tabs closing in sequence, a borrow discipline that" >&2
        echo "  held at one is exactly what would first fail here." >&2
        exit "$2"
    fi
    app_pid=
}

# =========================================================================
# RUN 1 -- window A with two tabs, then window B with two tabs, one quit.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome two-window smoke"

# --- Step 1: window A -- a lone file, then a split tab zoomed on its right pane
nvim_ex "e $TWOWIN_A_FIRST_FILE"
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
nvim_ex "e $TWOWIN_A_LEFT_FILE"
# VERTICAL split: the new pane opens to the RIGHT and takes focus.
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"
nvim_ex "e $TWOWIN_A_ZOOMED_FILE"
nvim_ex "WolfZoomPane"
wait_nvim_count 2 27 "after Ctrl+Shift+T in window A"

# --- Step 2: open the SECOND window, from inside THIS process ---------------
#
# Ctrl+Shift+P, the query, Enter on the preselected top row. New Window carries no
# accelerator (Ctrl+Shift+N is spent on win.new-buffer), so the palette, the primary
# menu and :WolfNewWindow are its only routes -- and of those, the palette is the
# only one this harness can drive. Launching the binary a second time is NOT an
# alternative: on the cage's own bus it hands off through single-instance
# activation, which is the launch path rather than New Window.
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$TWOWIN_NEW_WINDOW_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$SETTLE"

# --- Step 3: a second window means a third Neovim ----------------------------
#
# Window A's two tabs run two; window B's one tab runs the third. No capture could
# take this observable.
wait_nvim_count 3 21 "after the palette activated New Window"

# The count is right as soon as the process exists, which is before its core has
# bound a pane, so settle before typing an Ex command at it.
sleep "$SETTLE"
sleep "$RESTORE_SETTLE"

# --- Step 4: the new window did NOT inherit a project ------------------------
#
# Keys go to the window cage is showing, which is the NEWEST toplevel (measured:
# cage keeps the newest on top and declines a present() on any other), i.e. window
# B. New Window builds its plan from no saved state AND no process cwd, so B starts
# in the empty state at $HOME; inheriting the invoking window's cwd is what New TAB
# does and is not this row's behaviour.
#
# It reads bufname('%') as well as getcwd(), and that second value is not
# decoration: it is what separates "the new window inherited a project" from "these
# keystrokes never reached the new window at all". Window A has its zoomed file
# current; a window opened by New Window has an unnamed buffer whatever cwd it was
# given, whatever its ordinal. So a readback naming one of A's files can only have
# come from window A, and it gets its own exit code rather than being reported as
# an inheritance that did not happen.
rm -f "$TWOWIN_B_CWD"
wtype -- ":call writefile([getcwd(), fnamemodify(bufname('%'), ':t')], '$TWOWIN_B_CWD')"
wtype -k Return
wait_file "$TWOWIN_B_CWD" || exit 15
b_cwd=$(sed -n 1p "$TWOWIN_B_CWD")
b_buf=$(sed -n 2p "$TWOWIN_B_CWD")
echo "wolf-gnome two-window smoke: the focused NEW window reports getcwd() = $b_cwd" >&2
echo "  and bufname('%') = $b_buf" >&2

case "$b_buf" in
    "$TWOWIN_A_FIRST_FILE" | "$TWOWIN_A_LEFT_FILE" | "$TWOWIN_A_ZOOMED_FILE")
        echo "wolf-gnome two-window smoke: the step-4 readback names $b_buf, which only the" >&2
        echo "  INVOKING window has open -- so these keystrokes reached window A, not the new" >&2
        echo "  window, and nothing below would be about the new window at all." >&2
        echo "  This is a FOCUS failure rather than a coordinator one. Three Neovims exist (the" >&2
        echo "  child count above already passed), so check whether the second toplevel mapped" >&2
        echo "  and took keyboard focus: cage shows one toplevel at a time and keeps the NEWEST" >&2
        echo "  on top, so the newest is what wtype drives." >&2
        echo "  If this is intermittent, the settle after the child-count poll is the knob -- the" >&2
        echo "  count is satisfied as soon as the nvim process exists, which is earlier than the" >&2
        echo "  window being ready for input." >&2
        exit 26
        ;;
esac

# The cwd comparison is on the final path component, deliberately. /tmp may resolve
# through a symlink on this host, so nvim's getcwd() need not be string-equal to the
# path this harness built -- but the temp cwd's BASENAME is a fixed constant and
# survives every symlink, and the app's HOME (B's correct answer) is the unique
# `wolf-gnome-smoke-<pid>-<nanos>` parent, which can never share it. The Rust side
# asserts that non-collision before launching anything.
if [ "${b_cwd##*/}" = "$TWOWIN_PROJECT_BASE" ]; then
    echo "wolf-gnome two-window smoke: the NEW window came up in the INVOKING window's" >&2
    echo "  project directory ($b_cwd)." >&2
    echo "  New Window must build its plan from no saved state and NO PROCESS CWD, which by" >&2
    echo "  construction is the empty state at \$HOME with no layout. Passing the process cwd" >&2
    echo "  (or the invoking window's) inherits a project -- the behaviour New TAB has, and" >&2
    echo "  the one that looks right exactly half the time it is tried by hand, because from" >&2
    echo "  a desktop launcher the process cwd IS \$HOME." >&2
    exit 22
fi

# --- Step 5: window B joins the project, and gets a second tab ---------------
#
# A typed `:cd` moves the empty tab's Neovim, which takes that tab -- and only that
# tab -- out of the empty state, so it saves a directory and a root. ABSOLUTE,
# because B's cwd is $HOME at this point. The new tab inherits the selected tab's
# directory, so both of B's tabs are in the project too, and B's second tab is the
# one selected when the chord quits: the last keys of run 1 go to window B.
nvim_ex "cd $TWOWIN_PROJECT_ABS"
nvim_ex "e $TWOWIN_B_FIRST_FILE"
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
nvim_ex "e $TWOWIN_B_ACTIVE_FILE"
wait_nvim_count 4 28 "after Ctrl+Shift+T in window B"

# --- Step 6: quit the APPLICATION with Ctrl+Shift+Q --------------------------
#
# The only exercise `app.quit` gets in this binary. Coordinator::quit latches
# shutting_down, refreshes EVERY window's tabs while every core and every pane is
# still alive, commits the window state, blocks on the flush, and only then closes
# the windows -- so wolf.sqlite holds both windows before the process goes.
quit_app 12 13 "run 1"

# --- Step 7: freeze run 1's save before run 2 can write over it --------------
if ! freeze_state "$TWOWIN_STATE_SRC" "$TWOWIN_STATE_COPY"; then
    echo "wolf-gnome two-window smoke: run 1 wrote no wolf.sqlite in $TWOWIN_STATE_SRC." >&2
    echo "  Nothing at all was persisted, so this is upstream of anything about windows:" >&2
    echo "  suspect open_state_store() resolving no directory, or WOLF_STATE_DIR not" >&2
    echo "  reaching the app." >&2
    exit 16
fi

# =========================================================================
# RUN 2 -- relaunch in the SAME cwd with the SAME state dir, read back, quit.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 14 "wolf-gnome two-window smoke"

# --- Step 8: BOTH windows came back, every tab with its own core -------------
wait_nvim_count 4 23 "after the relaunch restored the saved session"

# --- Step 9: what the FOCUSED restored window shows --------------------------
#
# Only one window can be typed at (cage shows one toplevel and nothing can raise
# another), and the restore maps the saved frontmost window last, so this reads the
# window the restore put in front. The Rust side says which window and tab that is.
rm -f "$TWOWIN_R2_READBACK"
wtype -- ":call writefile([getcwd(), fnamemodify(bufname('%'), ':t')], '$TWOWIN_R2_READBACK')"
wtype -k Return
wait_file "$TWOWIN_R2_READBACK" || exit 15

# --- Step 10: quit again, and freeze run 2's save ----------------------------
quit_app 29 30 "run 2"
if ! freeze_state "$TWOWIN_STATE_SRC" "$TWOWIN_STATE_COPY_2"; then
    echo "wolf-gnome two-window smoke: run 2 left no wolf.sqlite in $TWOWIN_STATE_SRC." >&2
    exit 31
fi

exit 0
"#;

/// The two directories, RELATIVE to the app's `$HOME`, that
/// `wolf_core::state::state_dir()` would resolve to if `WOLF_STATE_DIR` were ever
/// missing — the exact paths a leak would land in, and therefore the exact paths
/// [`single_pane_file_survives_relaunch_under_cage`]'s assertion 4 proves are empty.
///
/// `state_dir()` is `dirs::state_dir().or_else(dirs::data_dir).map(|d| d.join("wolf"))`
/// on Linux, and those two `dirs` functions default to `$XDG_STATE_HOME` (else
/// `$HOME/.local/state`) and `$XDG_DATA_HOME` (else `$HOME/.local/share`). Both arms
/// are listed because the chain has two, not because the second is expected: on Linux
/// `dirs::state_dir()` is `Some` whenever `$HOME` resolves, so `data_dir` is
/// effectively unreachable — checking it costs one `Path::exists` and removes the need
/// for anyone to re-derive that.
///
/// These are spelled out here rather than obtained from `dirs`, which `wolf-gnome`
/// does not depend on (and must not gain one for a test: a new dependency changes
/// `Cargo.lock` and needs a `CARGO_BAZEL_REPIN`). That makes them a MIRROR of the XDG
/// defaults, with the honest limitation written down: they are the right paths only
/// while `$XDG_STATE_HOME` / `$XDG_DATA_HOME` are unset in the environment cage
/// inherits, which is the case in this repo's devShell (`flake.nix` exports
/// `XDG_DATA_DIRS`, neither of these). On a host that DOES export one, a leak lands
/// outside the temp tree instead — where the sibling check on the developer's real
/// state file catches it. The two checks are complementary and
/// between them cover both cases.
const HOME_STATE_DIR_FALLBACKS: [&str; 2] = [".local/state/wolf", ".local/share/wolf"];

/// The directory, RELATIVE to the app's `$HOME`, that
/// `wolf_core::preferences_dir()` would resolve to if `WOLF_CONFIG_DIR` were ever
/// missing — the exact path a leak would land in, and therefore the exact path
/// [`single_pane_file_survives_relaunch_under_cage`]'s config-isolation assertion
/// proves is empty.
///
/// `preferences_dir()` is `dirs::config_dir().map(|d| d.join("wolf"))` off macOS,
/// and `dirs::config_dir()` defaults to `$XDG_CONFIG_HOME` (else
/// `$HOME/.config`). There is only one arm here, unlike
/// [`HOME_STATE_DIR_FALLBACKS`]' two, because the chain has only one.
///
/// It is spelled out rather than obtained from `dirs`, which `wolf-gnome` does not
/// depend on and must not gain a dependency on for a test. It is the right path on
/// every host, unlike the state constant: [`cage_command`] sets `XDG_CONFIG_HOME` to
/// the `.config` directory under the same `HOME`, so whatever the developer's
/// environment exports, a leak lands here. The sibling check on the developer's real
/// config file covers a smoke that bypassed that helper.
const HOME_CONFIG_DIR_FALLBACK: &str = ".config/wolf";

/// Parses a name-keyed pane-width readback: whitespace-separated
/// `<columns>:<buffer tail>` pairs, one per Neovim window, in whatever order
/// Neovim enumerated them.
///
/// Keyed by NAME rather than by position because nvim numbers its windows by its
/// own layout, which need not match the order this front-end draws the panes in.
///
/// The columns are `winwidth()`, the GRID width `resize_grid` drives — NOT
/// `nvim_win_get_width()`, which reports the layout width and is measurably blind
/// to a divider drag. [`DIVIDER_RELAUNCH_SCENARIO`]'s `read_widths` carries the
/// measurement; do not "simplify" the readback back to the API call.
fn read_pane_widths(path: &Path, smoke: &str) -> Vec<(String, i64)> {
    let raw = read_readback(path, smoke);
    let mut out = Vec::new();
    for token in raw.split_whitespace() {
        let (width, name) = token.split_once(':').unwrap_or_else(|| {
            panic!(
                "{smoke}: pane-width token {token:?} in {} is not `<columns>:<buffer tail>`. Raw \
                 contents were {raw:?}.",
                path.display()
            )
        });
        out.push((
            name.to_string(),
            parse_readback_int(width, path, &raw, smoke),
        ));
    }
    assert!(
        !out.is_empty(),
        "{smoke}: the pane-width readback {} holds no `<columns>:<buffer tail>` pairs at all. Raw \
         contents were {raw:?}.",
        path.display()
    );
    out
}

/// The width, in columns, of the pane showing the buffer whose tail name is `name`.
///
/// Panics naming every pane that WAS present, because "the pane holding this file
/// is gone" and "the pane holding this file is the wrong width" are different
/// findings and must not share a message.
fn pane_width_for(panes: &[(String, i64)], name: &str, path: &Path, smoke: &str) -> i64 {
    panes
        .iter()
        .find(|(buffer, _)| buffer == name)
        .map(|(_, width)| *width)
        .unwrap_or_else(|| {
            panic!(
                "{smoke}: no pane in {} is showing {name:?}. The panes present were {panes:?}. An \
                 `Untitled.md` here means the file was never opened (run 1) or never restored (run \
                 2); an EMPTY name means a pane is bound to an unnamed buffer.",
                path.display()
            )
        })
}

/// A fingerprint of a file that must NOT change across a smoke run: its length and
/// modification time, or `None` when it does not exist.
///
/// `None` is a first-class answer, not an error: on a machine that has never run
/// Wolf interactively — which includes CI and this host —
/// `~/.local/state/wolf/wolf.sqlite` simply does not exist, and "absent before,
/// absent after" is exactly as strong an isolation proof as "unchanged". Taking
/// `metadata().unwrap()` for the BEFORE snapshot would panic there instead.
fn state_file_fingerprint(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

/// Proves the tab's PANE-SPLIT TREE survives a quit and relaunch: three panes in an
/// asymmetric layout, each holding its own file, rebuilt from the saved window state.
///
/// Two app launches run inside ONE `cage` instance (see [`RESTORE_LAYOUT_SCENARIO`]
/// and the shared exit-code table above [`RELAUNCH_RESTORE_SETTLE`]), so the smoke
/// holds one cage slot and no second compositor is started.
///
/// ## What it asserts
///
/// 1. **Load-bearing, and EXACT:** the restored Neovim's own
///    `tabpagebuflist()` reads exactly `rleft.md rmid.md rright.md`. That is one
///    comparison proving three things at once — the pane COUNT, the buffer
///    IDENTITIES, and their ORDER — because nvim numbers windows by layout
///    position.
/// 2. **Corroboration (the GTK half):** OCR carries `restorealfa` AND
///    `restorebravo` AND `restorecharlie`. Three distinct panes actually bound to
///    grids and PAINTED, which no `winnr('$')` readback can show.
///
/// ## How it discriminates RESTORATION from COINCIDENCE
///
/// A fresh launch of `wolf-gnome` shows exactly ONE pane holding `Untitled.md`: the
/// binary takes no CLI arguments, opens no file at startup, and has no MRU-reopen
/// path — the readback would be `Untitled.md` and the OCR would carry none of the
/// three terms. Neovim's own session behaviour cannot produce it either: the
/// captive nvim is spawned fresh with `-u init.lua`, there is no `:mksession`, and
/// nothing in `init.lua` restores windows. And the SHAPE is unreachable by
/// accident: a three-pane asymmetric split — a left column split horizontally
/// beside a full-height right pane — holding three DISTINCT files cannot arise from
/// any single-file reopen, any MRU, or any swap recovery, all of which would give
/// one pane and one name. Finally the three fixture names (`rleft.md`, `rmid.md`,
/// `rright.md`) contain none of the three asserted terms, so neither a sidebar row
/// nor an `:e` toast can satisfy one — enforced by
/// `restore_fixture_names_cannot_satisfy_their_own_assertions`.
///
/// ## What it does NOT prove
///
/// **Nothing here exercises non-equal pane WEIGHTS.** This scenario never drags a
/// divider, so every weight it persists is an equal share, and "the saved weights
/// were applied to the dividers" and "the equal-share default was recomputed" agree
/// numerically throughout. It proves the TOPOLOGY round trip and nothing about the
/// weights; [`dragged_divider_weights_survive_relaunch_under_cage`] is the smoke
/// that drags one and asserts on the value that comes back.
///
/// ## Red-run revert (one, and one line)
///
/// | assertion | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | both | in `tab_view.rs`'s `open_tab`, delete the `panes.request_layout_rebuild(restored)` call — or, equivalently, make `pane_view::consume_pending_layout` `return false;` as its first statement | the readback reads `Untitled.md` instead of the three names, and the OCR carries none of `restorealfa` / `restorebravo` / `restorecharlie` | Not re-measured since the app-state switch. |
#[test]
fn pane_layout_survives_relaunch_under_cage() {
    const SMOKE: &str = "pane_layout_survives_relaunch_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Both launches use this one cwd: relaunching in the project you saved from is the scenario.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (RESTORE_FILE_LEFT, RESTORE_HEADING_LEFT),
            (RESTORE_FILE_MID, RESTORE_HEADING_MID),
            (RESTORE_FILE_RIGHT, RESTORE_HEADING_RIGHT),
        ],
    );

    // Start from a clean slate: evidence that could be a previous run's leftover is
    // not evidence. `clear_evidence` removes AND asserts absent.
    let readback = cwd.join(RESTORE_READBACK_NAME);
    clear_evidence(&readback, SMOKE);
    let out_png = runtime_dir.join("wolf_gnome_restore_layout_smoke.png");
    clear_evidence(&out_png, SMOKE);

    // The state dir makes both runs share one `wolf.sqlite` and keeps them out of the developer's.
    let status = cage_command(RESTORE_LAYOUT_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("RESTORE_FILE_LEFT", RESTORE_FILE_LEFT)
        .env("RESTORE_FILE_MID", RESTORE_FILE_MID)
        .env("RESTORE_FILE_RIGHT", RESTORE_FILE_RIGHT)
        .env("RESTORE_READBACK", &readback)
        .status()
        .expect("failed to spawn `cage` for the headless pane-layout relaunch scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage relaunch scenario exited unsuccessfully ({status}). Exit 11: run 1's \
         app died before it was ready for input. Exit 12: run 1 was still alive after `:qa` — the \
         drained close_window never reached window.close(). Exit 13: run 1 exited NON-ZERO, i.e. \
         it went away but not cleanly, which implicates a panic in \
         `Coordinator::on_window_closing`, or a core.borrow() held across `wire_close`'s \
         core.borrow_mut().take(). \
         Exit 14: run 2's app died before it was ready for input. Exit 15: the Neovim readback never \
         appeared in run 2. Screenshot expected at {}; readback at {}.",
        out_png.display(),
        readback.display()
    );

    assert_png_present(SMOKE, &out_png);

    // --- Assertion 1: the EXACT window list, from Neovim itself ---------------
    let got = read_trimmed(&readback, SMOKE);
    assert_eq!(
        got,
        RESTORE_READBACK_EXPECTED,
        "{SMOKE}: after the relaunch, the restored Neovim's tabpagebuflist() names \
         {got:?}, not {RESTORE_READBACK_EXPECTED:?}. `tabpagebuflist()` is in WINDOW-NUMBER order \
         (a row left-to-right, a column top-to-bottom), so this one comparison covers the pane \
         count, the buffer identities and their order. Read it this way: `Untitled.md` alone means \
         NOTHING was restored (the saved layout was never queued, or `consume_pending_layout` \
         never fired); the three right names in the WRONG ORDER means the DFS zip is inverted \
         somewhere between `tab_rebuild_spec`, `wolf_rebuild_layout` and `pane_layout::decode`; \
         and fewer than three names means the rebuild ran but produced the wrong topology. PNG \
         kept at {}.",
        out_png.display()
    );

    // --- Assertion 2: three panes really PAINTED ------------------------------
    let ocr_text = ocr_to_lowercase(&out_png);
    for (term, const_name, position) in [
        (RESTORE_OCR_ALFA, "RESTORE_OCR_ALFA", "top-left"),
        (RESTORE_OCR_BRAVO, "RESTORE_OCR_BRAVO", "bottom-left"),
        (
            RESTORE_OCR_CHARLIE,
            "RESTORE_OCR_CHARLIE",
            "full-height right",
        ),
    ] {
        assert!(
            ocr_text.contains(term),
            "{SMOKE}: OCR did not contain {const_name} = {term:?} (the {position} pane's file). \
             The readback assertion above passed, so NEOVIM has the three windows — this says the \
             GTK side did not bind or paint all three. Check that `apply_layout_rebuild` zipped \
             every returned handle onto a pane (`handles.len() != leaf_ids.len()` logs and \
             returns, leaving the single pre-existing pane), and that `rebuild_pane_area` + \
             `resolve_pending_grids` ran afterwards. PNG kept at {} for inspection. OCR output \
             was:\n{ocr_text}",
            out_png.display()
        );
    }
}

/// Proves the SINGLE-PANE case: one window, one tab, one pane, one file — the most
/// common shape there is — survives a quit and relaunch.
///
/// This is the GTK-side regression guard for the macOS bug fixed by
/// `fix/state-persistence-single-pane-bug`. A tab's open buffer paths have NO
/// persistence channel of their own: they ride ONLY inside the saved tab's `root`.
/// A save side that suppresses the tree by ARITY makes this exact shape persist its
/// directory and sidebar and nothing else, and the file is not reopened. The rule
/// on both sides is that a folder tab always writes its root, a lone pane included,
/// and restore rebuilds ANY saved root but a lone empty edit pane, with no arity
/// guard at all.
///
/// ## What it asserts
///
/// 1. **The regression assertion, and it is on the FILE not on pixels.** The
///    window state RUN 1 wrote (frozen by the scenario into `$SOLO_STATE_COPY`
///    before run 2 can write over it) carries
///    `windows[0].tabs[0].root == Some(Pane::Edit { path, .. })` where `path` is
///    non-empty and its file NAME is exactly `szulu.md`. The name rather than the
///    whole path, deliberately: `/tmp` (or `TMPDIR`) may resolve through a symlink
///    on this host, so nvim's reported `abs_path` need not be string-equal to any
///    `PathBuf` this test could build — but an arity guard yields `root: null`,
///    which this catches perfectly, and an empty-path pane cannot satisfy it either.
///    No pixel can distinguish "the layout was saved and restored" from "the layout
///    was never saved but the file came back another way", which is why this
///    assertion exists at all.
/// 2. **Load-bearing, and exact:** the restored Neovim's `bufname('%')` tail is
///    exactly `szulu.md`.
/// 3. **Corroboration:** OCR contains `solozulu` and does NOT contain `soloalfa`.
/// 4. **State-dir isolation**, in four parts: this TEST process has no
///    `WOLF_STATE_DIR` of its own (without which the whole check would be
///    self-referential); the smoke's own state dir holds a non-empty `wolf.sqlite`,
///    proving the app honoured the variable; NEITHER `$HOME`-derived fallback
///    directory ([`HOME_STATE_DIR_FALLBACKS`], under the temp `$HOME`
///    [`cage_command`] gives the app) holds a `wolf.sqlite`, which is the positive
///    proof that `state_dir()` resolved the variable rather than `$HOME`; and the
///    developer's REAL `wolf.sqlite` is unchanged across the run — including
///    the normal case where it does not exist at all, which must still not exist
///    afterwards. The last two are complementary, not redundant: a leak lands
///    inside the temp tree when `$XDG_STATE_HOME` is unset and outside it when it
///    is set, and one arm catches each.
/// 5. **Config-dir isolation**, in the same four parts and for a sharper reason.
///    `wolf_core::load_or_create` CREATES `wolf-config.yaml` when it finds none, so
///    an app that ignored `WOLF_CONFIG_DIR` would not merely read the developer's
///    preferences — on a machine that has never run Wolf interactively it would
///    WRITE them, and every later smoke would then be handed the default family
///    `iA Writer Mono S` instead of the seeded `DejaVu Sans Mono`. The four parts
///    are: this TEST process has no `WOLF_CONFIG_DIR` of its own (without which
///    `preferences_dir()` below would resolve the override and the check would be
///    self-referential); the smoke's own config dir holds a non-empty
///    `wolf-config.yaml`; the `$HOME`-derived fallback directory
///    ([`HOME_CONFIG_DIR_FALLBACK`], under the temp `$HOME` [`cage_command`] gives
///    the app) holds none; and the developer's REAL `wolf-config.yaml` is
///    byte-identical across the run, including the normal case where it does not
///    exist and must still not exist afterwards.
///
/// ## How it discriminates RESTORATION from COINCIDENCE
///
/// A fresh launch in this cwd shows an untitled buffer (`Untitled.md`), which is
/// the readback a broken build produces. The DECOY (`salfa.md`, alphabetically
/// first, never opened) kills the other plausible hypothesis: if the app were
/// reopening "the first file it found", the readback would be `salfa.md` and the
/// OCR would carry `soloalfa` — both of which the assertions catch. And assertion 1
/// cannot be satisfied by anything except the save path writing a lone pane as the
/// tab's root.
///
/// ## Red-run reverts (TWO, both one-liners, and BOTH must be run)
///
/// They isolate the two halves of the original bug and must produce
/// DISTINGUISHABLE failures — which is exactly what makes assertion 1 worth having,
/// and why the scenario freezes run 1's database before run 2 can overwrite it.
///
/// | half | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | SAVE side | in `state_seam::tab_entry`, write `root: None` when `pane_layout::leaf_count(&root) < 2` | assertion 1 fails FIRST and names the arity guard (run 1's copy carries `root: null`); assertions 2 and 3 also fail | Not re-measured since the app-state switch. |
/// | RESTORE side | in `tab_view.rs`'s `open_tab`, wrap the `request_layout_rebuild` call in `if pane_layout::leaf_count(root) > 1 { … }` | assertion 1 **passes** (the save side is correct and the frozen copy proves it) while assertions 2 and 3 fail | Not re-measured since the app-state switch. |
#[test]
fn single_pane_file_survives_relaunch_under_cage() {
    const SMOKE: &str = "single_pane_file_survives_relaunch_under_cage";

    // Assertion 4, first part: checked first, because `state_dir()` resolves `WOLF_STATE_DIR` ahead of
    // everything, and an override here would make the isolation check self-referential.
    assert!(
        std::env::var_os("WOLF_STATE_DIR").is_none(),
        "{SMOKE}: WOLF_STATE_DIR is set in the TEST process. `wolf_core::state_dir()` resolves \
         that variable FIRST, so the \"developer's real state file\" this smoke proves is \
         untouched would be resolved against the override instead of against \
         $XDG_STATE_HOME/wolf — the isolation assertion would then compare the smoke's own \
         directory with itself and pass no matter what the app did. `cage_command` sets \
         WOLF_STATE_DIR on the CHILD only, which is the whole design; nothing may set it on the \
         test process."
    );

    let real_state_dir = state_dir().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::state_dir() resolved to None in the test process, so this smoke \
             cannot say WHICH file it is proving untouched and its isolation assertion would hold \
             vacuously. On Linux it is $XDG_STATE_HOME/wolf (falling back to the data dir), so a \
             None here means neither is resolvable — check $HOME."
        )
    });
    let real_state_file = real_state_dir.join(STATE_DB_NAME);
    let real_before = state_file_fingerprint(&real_state_file);

    // Assertion 5, first part: the same for `WOLF_CONFIG_DIR`, which `preferences_dir()` reads first.
    assert!(
        std::env::var_os("WOLF_CONFIG_DIR").is_none(),
        "{SMOKE}: WOLF_CONFIG_DIR is set in the TEST process. `wolf_core::preferences_dir()` \
         resolves that variable FIRST, so the \"developer's real wolf-config.yaml\" this smoke \
         proves is untouched would be resolved against the override instead of against \
         $XDG_CONFIG_HOME/wolf — the isolation assertion would then compare the smoke's own \
         directory with itself and pass no matter what the app did. `cage_command` sets \
         WOLF_CONFIG_DIR on the CHILD only, which is the whole design; nothing may set it on the \
         test process."
    );

    let real_config_dir = preferences_dir().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::preferences_dir() resolved to None in the test process, so this \
             smoke cannot say WHICH config file it is proving untouched and its isolation \
             assertion would hold vacuously. On Linux it is $XDG_CONFIG_HOME/wolf (else \
             $HOME/.config/wolf), so a None here means neither is resolvable — check $HOME."
        )
    });
    let real_config_file = real_config_dir.join(CONFIG_FILE_NAME);
    let real_config_before = state_file_fingerprint(&real_config_file);

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd shared by BOTH launches, holding the file to open and the
    // never-opened decoy.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (SOLO_FILE, SOLO_HEADING),
            (SOLO_DECOY_FILE, SOLO_DECOY_HEADING),
        ],
    );

    // The smoke's own state dir — the SAME sibling `cage_command` computes and hands
    // the app as WOLF_STATE_DIR, so the two runs share one `wolf.sqlite`.
    let smoke_state = smoke_state_dir(&cwd);
    let smoke_state_file = smoke_state.join(STATE_DB_NAME);

    let smoke_config = smoke_config_dir(&cwd);
    let smoke_config_file = smoke_config.join(CONFIG_FILE_NAME);

    // Freezes run 1's save before run 2 overwrites it.
    let run1_state_dir = frozen_state_dir(&runtime_dir, "wolf_gnome_solo_run1_state", SMOKE);

    let readback = cwd.join(SOLO_READBACK_NAME);
    let out_png = runtime_dir.join("wolf_gnome_restore_solo_smoke.png");
    for path in [&readback, &out_png] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(RESTORE_SOLO_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("SOLO_FILE", SOLO_FILE)
        .env("SOLO_READBACK", &readback)
        .env("SOLO_STATE_SRC", &smoke_state)
        .env("SOLO_STATE_COPY", &run1_state_dir)
        .status()
        .expect("failed to spawn `cage` for the headless single-pane relaunch scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage single-pane relaunch scenario exited unsuccessfully ({status}). Exit \
         11: run 1's app died before it was ready for input. Exit 12: run 1 was still alive after \
         `:qa`. Exit 13: run 1 exited NON-ZERO — it went away but not cleanly, which implicates a \
         panic in `Coordinator::on_window_closing` or a core.borrow() held across `wire_close`'s \
         core.borrow_mut().take(). Exit 14: run 2's app died before it was ready for input. Exit 15: \
         the Neovim readback never appeared in run 2. Exit 16: run 1 wrote NO wolf.sqlite at all \
         in {} — not the arity guard (which still writes the window state, with a null root) but \
         something upstream of it. Screenshot expected at {}.",
        smoke_state.display(),
        out_png.display()
    );

    assert_png_present(SMOKE, &out_png);

    // --- Assertion 1: RUN 1's saved root is a single NAMED pane ----------------
    let run1_state_file = run1_state_dir.join(STATE_DB_NAME);
    let saved = frozen_window_state(&run1_state_dir, SMOKE, "run 1's save");
    let window = saved.windows.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's window state carries NO windows at all, so nothing was persisted. \
             `state_seam::window_state` commits nothing rather than an empty state, so an empty \
             `windows` means a writer other than the coordinator's. File: {}",
            run1_state_file.display()
        )
    });
    let tab = window.tabs.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's persisted window carries NO tabs. `state_seam::window_entry` \
             contributes no window without a tab. File: {}",
            run1_state_file.display()
        )
    });
    match tab.content.root.as_ref().and_then(leaf_fields) {
        Some(LeafFields { path, preview, .. }) => {
            assert!(
                !path.is_empty(),
                "{SMOKE}: run 1 persisted a lone leaf with an EMPTY path. The pane was showing \
                 {SOLO_FILE}, so this means `layout_snapshot` could not resolve the pane's buffer \
                 path: either the pane had no resolved grid, or `pane_status(grid).abs_path` came \
                 back empty because nvim had not reported its per-window buffer map yet. That is \
                 precisely what `may_snapshot`'s `reported_windows >= 1` gate exists to prevent. \
                 File: {}",
                run1_state_file.display()
            );
            assert!(
                !preview,
                "{SMOKE}: run 1 persisted the lone pane as a preview. The pane was an editor on \
                 {SOLO_FILE}, so `pane_layout::encode` mapped its `LeafContent` to the wrong kind; \
                 a preview here would make the restore call `wolf_preview_file` instead of \
                 `:edit`. File: {}",
                run1_state_file.display()
            );
            let file_name = Path::new(path).file_name().and_then(OsStr::to_str);
            assert_eq!(
                file_name,
                Some(SOLO_FILE),
                "{SMOKE}: run 1 persisted a lone leaf naming {path:?}, whose file name is not \
                 {SOLO_FILE:?}. The comparison is on the file NAME rather than the whole path \
                 deliberately — /tmp may resolve through a symlink, so nvim's reported abs_path \
                 need not be string-equal to anything this test could construct — but the name \
                 itself is exact. `Untitled.md` here means the `:e` never landed before the quit. \
                 File: {}",
                run1_state_file.display()
            );
        }
        None => panic!(
            "{SMOKE}: run 1 persisted `root: {:?}` for a single unsplit pane holding \
             {SOLO_FILE}, not a single named pane. **`None` IS THE SINGLE-PANE REGRESSION**: it \
             means the save side gated on ARITY (`leaf_count(..) > 1`), where a folder tab always \
             writes its root. A tab's open buffer paths have no persistence channel of their own \
             — they ride only inside the tab's `root` — so an arity guard makes the single most \
             common shape (one window, one tab, one pane, one file) persist its directory and \
             nothing else, and the file is not reopened on relaunch. That is the exact macOS bug \
             this smoke exists to keep out of the GTK front-end; see `state_seam::tab_entry`. A \
             LAYOUT here would instead mean the run-1 scenario somehow split the pane. File: {}",
            tab.content.root,
            run1_state_file.display()
        ),
    }

    // --- Assertion 2: the restored Neovim's current buffer, exactly ------------
    let got = read_trimmed(&readback, SMOKE);
    assert_eq!(
        got,
        SOLO_READBACK_EXPECTED,
        "{SMOKE}: after the relaunch the restored Neovim's current buffer is {got:?}, not \
         {SOLO_READBACK_EXPECTED:?}. `Untitled.md` means the saved layout was never rebuilt — \
         assertion 1 above already proved it WAS saved correctly, so with that green this points \
         squarely at the restore side: an arity guard around `request_layout_rebuild`, a \
         `consume_pending_layout` that never fires, or a `pane_layout::decode` that rejected a \
         lone pane. {SOLO_DECOY_FILE:?} would instead mean the app reopened the first file it \
         found rather than the saved one. PNG kept at {}.",
        out_png.display()
    );

    // --- Assertion 3: it actually PAINTED, and the decoy did not --------------
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(SOLO_OCR_PRESENT),
        "{SMOKE}: OCR did not contain {SOLO_OCR_PRESENT:?} (the restored file's heading). The \
         readback above named {SOLO_FILE}, so NEOVIM has the right buffer — this says the GTK pane \
         did not paint it. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        !ocr_text.contains(SOLO_OCR_ABSENT),
        "{SMOKE}: OCR unexpectedly contained {SOLO_OCR_ABSENT:?} — the heading of the DECOY \
         {SOLO_DECOY_FILE}, which no scenario step ever opens. A hit means the app reopened a file \
         it chose itself (the alphabetically first one in the cwd) rather than the one the saved \
         layout names, which would make every other assertion here a coincidence. Note the decoy's \
         FILENAME does not contain the term, so a sidebar row cannot cause this. PNG kept at {}. \
         OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 4: state-dir isolation, three checks that each see a leak the others cannot.
    // See notes/crates/wolf-gnome/tests/smoke_headless/restore.rs.md#state-dir-isolation-checks
    let smoke_meta = std::fs::metadata(&smoke_state_file).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: expected the smoke's own state database {} to exist after the run, but: {e}. Its \
             absence means the app did not honour WOLF_STATE_DIR — which also means it wrote \
             somewhere else, most likely the developer's real ~/.local/state/wolf/.",
            smoke_state_file.display()
        )
    });
    assert!(
        smoke_meta.len() > 0,
        "{SMOKE}: the smoke's own state database {} exists but is empty",
        smoke_state_file.display()
    );

    // Check 2: `smoke_home(&cwd)` is the `HOME` `cage_command` gives the child, so this is the app's.
    let app_home = smoke_home(&cwd);
    assert!(
        app_home.is_dir(),
        "{SMOKE}: the app's HOME {} is not a directory, so the fallback checks below would pass \
         vacuously — a non-existent path never holds a wolf.sqlite. `make_temp_cwd` creates \
         `<unique>/project` and `cage_command` hands `<unique>` to the app as HOME, so this must \
         exist.",
        app_home.display()
    );
    for rel in HOME_STATE_DIR_FALLBACKS {
        let leaked = app_home.join(rel).join(STATE_DB_NAME);
        assert!(
            !leaked.exists(),
            "{SMOKE}: found a state database at {} — inside the app's own $HOME. That path is what \
             `wolf_core::state::state_dir()` resolves to when WOLF_STATE_DIR is ABSENT (the \
             `dirs::state_dir()` / `dirs::data_dir()` chain, XDG default `$HOME/{rel}`), so its \
             existence means the app resolved its state directory from $HOME instead of from \
             WOLF_STATE_DIR. Every smoke is then writing application state somewhere this harness \
             does not control: the relaunch smokes stop sharing one wolf.sqlite between their two \
             launches, and — because the $HOME redirect is a TEST fixture that does not exist on a \
             developer's machine — the same defect there writes to their real \
             ~/.local/state/wolf/wolf.sqlite, reading their saved session at launch and clobbering \
             it at quit. `cage_command` is the single place WOLF_STATE_DIR is set and \
             `cage_is_only_spawned_through_the_isolating_helper` is what keeps it single; start \
             there, then check that `state_dir()` still resolves the variable FIRST.",
            leaked.display()
        );
    }

    let real_after = state_file_fingerprint(&real_state_file);
    assert_eq!(
        real_before,
        real_after,
        "{SMOKE}: the developer's REAL state database {} changed across this smoke run (before: \
         {real_before:?}, after: {real_after:?}; `None` means the file did not exist, which is \
         the normal state on a machine that has never run Wolf interactively and must stay that \
         way). This is a STATE-DIR ISOLATION FAILURE: the app under cage resolved \
         `wolf_core::state_dir()` to the real directory instead of the smoke's own, which means it \
         READ the developer's saved windows, working directories and pane layouts at launch (so \
         this smoke did not start from a clean slate) and OVERWROTE them at quit with a throwaway \
         one-pane temp-cwd layout. `cage_command` is the single place WOLF_STATE_DIR is set, and \
         `cage_is_only_spawned_through_the_isolating_helper` is what keeps it single — start \
         there. The ONE benign cause: a real Wolf running concurrently in another terminal, which \
         writes that database on its own render wakes; close it and re-run before concluding \
         anything else.",
        real_state_file.display()
    );

    // Assertion 5: config-dir isolation, the same checks. Worse failure: `load_or_create` writes the
    // file it cannot find, so a missing WOLF_CONFIG_DIR writes the developer's preferences.
    let config_meta = std::fs::metadata(&smoke_config_file).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: expected the smoke's own config file {} to exist, but: {e}. \
             `smoke_config_dir` SEEDS it before either launch, so its absence means the helper did \
             not run — and the app under cage then had `load_or_create` write one for it carrying \
             `default_config()`, whose family `iA Writer Mono S` the pinned FONTCONFIG_FILE \
             resolves. Every glyph in this run was drawn in the wrong typeface.",
            smoke_config_file.display()
        )
    });
    assert!(
        config_meta.len() > 0,
        "{SMOKE}: the smoke's own config file {} exists but is empty. An empty file is not a \
         partial config — `load_or_create` fails to deserialize it and falls back to \
         `default_config()`, family `iA Writer Mono S`, exactly as an absent one would.",
        smoke_config_file.display()
    );

    let leaked_config = app_home
        .join(HOME_CONFIG_DIR_FALLBACK)
        .join(CONFIG_FILE_NAME);
    assert!(
        !leaked_config.exists(),
        "{SMOKE}: found a config file at {} — inside the app's own $HOME. That path is what \
         `wolf_core::preferences_dir()` resolves to when WOLF_CONFIG_DIR is ABSENT (the \
         `dirs::config_dir()` chain, XDG default `$HOME/{HOME_CONFIG_DIR_FALLBACK}`), so its \
         existence means the app resolved its preferences directory from $HOME instead. Because \
         `load_or_create` CREATES an absent file, the app did not just read the wrong place — it \
         wrote there, with the built-in defaults. The $HOME redirect is a TEST fixture that does \
         not exist on a developer's machine, so the same defect there lands in their real \
         ~/.config/wolf/wolf-config.yaml. `cage_command` is the single place WOLF_CONFIG_DIR is \
         set and `cage_is_only_spawned_through_the_isolating_helper` is what keeps it single.",
        leaked_config.display()
    );

    let real_config_after = state_file_fingerprint(&real_config_file);
    assert_eq!(
        real_config_before,
        real_config_after,
        "{SMOKE}: the developer's REAL config file {} changed across this smoke run (before: \
         {real_config_before:?}, after: {real_config_after:?}; `None` means the file did not \
         exist, which is the normal state on a machine that has never run Wolf interactively and \
         must stay that way). This is a CONFIG-DIR ISOLATION FAILURE. Read the `None` -> `Some` \
         direction as the serious one: `load_or_create` CREATED a wolf-config.yaml in the \
         developer's preferences directory, so every later interactive Wolf launch reads a file \
         the test suite wrote. `cage_command` is the single place WOLF_CONFIG_DIR is set, and \
         `cage_is_only_spawned_through_the_isolating_helper` is what keeps it single — start \
         there. The ONE benign cause: a real Wolf launched concurrently in another terminal, which \
         creates that file on its first run; close it and re-run before concluding anything else.",
        real_config_file.display()
    );
}

/// Asserts that a frozen window state holds ONE window whose tabs are, in order,
/// single edit panes named [`TABS_RESTORE_FILES`].
///
/// Shared by the three-tab smoke's two frozen-file assertions because the SHAPE they
/// demand is identical; `diagnosis` is what keeps their two reds apart, since the
/// same wrong count means a different thing at each stage. Both callers pass the
/// prediction for their own falsifier, so a failure names the defect that produces it
/// rather than restating the shape.
fn assert_frozen_tabs(smoke: &str, stage: &str, state_dir: &Path, diagnosis: &str) {
    let file = state_dir.join(STATE_DB_NAME);
    let saved = frozen_window_state(state_dir, smoke, stage);

    assert_eq!(
        saved.windows.len(),
        1,
        "{smoke}: {stage}'s window state carries {} window(s), not 1. This smoke opens ONE \
         window and never a second, so anything else means the state's shape is wrong before its \
         tabs are worth reading: ZERO means a writer other than the coordinator's, which commits \
         nothing rather than no window, and MORE than one means a window was registered twice or \
         a closed one was never deregistered. File: {}",
        saved.windows.len(),
        file.display()
    );
    let window = &saved.windows[0];

    // Read before the count, so a short tree still names its survivors: the name is the diagnosis.
    let leaf_names: Vec<Option<&str>> = window
        .tabs
        .iter()
        .map(|tab| {
            tab.content
                .root
                .as_ref()
                .and_then(leaf_fields)
                .and_then(|leaf| Path::new(leaf.path).file_name())
                .and_then(OsStr::to_str)
        })
        .collect();
    assert_eq!(
        window.tabs.len(),
        TABS_RESTORE_FILES.len(),
        "{smoke}: {stage}'s persisted window carries {} tab(s), not {}; their panes' file names, \
         in tab order, are {leaf_names:?} (`None` for a tab with no single named pane). \
         {diagnosis} A tab's open buffer paths have NO persistence channel of their own — they \
         ride only inside the tab's `root` — so a dropped tab is a lost session rather than a \
         cosmetic miscount, and because the next launch reads exactly this state the loss is \
         permanent from here. File: {}",
        window.tabs.len(),
        TABS_RESTORE_FILES.len(),
        file.display()
    );

    for (index, expected) in TABS_RESTORE_FILES.into_iter().enumerate() {
        let tab = &window.tabs[index];
        match tab.content.root.as_ref().and_then(leaf_fields) {
            Some(LeafFields { path, preview, .. }) => {
                assert!(
                    !preview,
                    "{smoke}: {stage}'s tab {index} persisted its lone pane as a preview, but no \
                     step of this scenario opens a preview. A preview here makes the restore \
                     call `wolf_preview_file` instead of `:edit`, so the tab comes back showing a \
                     rendered document where the user had an editor. File: {}",
                    file.display()
                );
                let leaf_name = Path::new(path).file_name().and_then(OsStr::to_str);
                assert_eq!(
                    leaf_name,
                    Some(expected),
                    "{smoke}: {stage}'s tab {index} persisted a pane naming {path:?}, whose file \
                     name is not {expected:?}. The comparison is on the file NAME rather than the \
                     whole path deliberately — /tmp may resolve through a symlink, so nvim's \
                     reported abs_path need not be string-equal to anything this test could \
                     construct — but the name itself is exact, and the three fixtures are \
                     deliberately different names. ANOTHER tab's file here means the tabs were \
                     assembled or restored in the wrong ORDER, which the next gather writes back, \
                     so the swap compounds on every run. `Untitled.md` means that tab's `:e` never \
                     landed. An EMPTY path means the pane was resolved from a window that is in \
                     neither `Snapshot::previews` nor `Snapshot::win_buffers` and mapped to `\"\"` \
                     instead of refusing — the silent-loss shape `pane_layout::resolve_leaves` \
                     exists to make unreachable. File: {}",
                    file.display()
                );
            }
            None => panic!(
                "{smoke}: {stage}'s tab {index} persisted `root: {:?}` where a single named pane \
                 was due. **`None` is the failure that matters**: the tab is present in the \
                 window and carries its directory and its sidebar preference, so every count \
                 above is satisfied — and the tab still comes back EMPTY, because its files ride \
                 only inside its root. A LAYOUT here would instead mean the scenario somehow \
                 split a pane. File: {}",
                tab.content.root,
                file.display()
            ),
        }
    }
}

/// Proves the whole per-tab persistence claim in one scenario: **three tabs, saved
/// in tab order with their layouts, restored at relaunch as three Neovims each holding
/// its own tab's file, and still all three after a second quit in which two of them were
/// never selected.**
///
/// Two app launches run inside ONE `cage` instance (see [`RESTORE_TABS_SCENARIO`]),
/// so the smoke holds one cage slot and no second compositor is started.
///
/// ## Why it asserts on no pixel
///
/// The claim is about what reaches the saved window state and what comes back out of
/// it, and neither is visible. A background tab's pane view is not drawn at all, so no
/// capture could show more than one tab's content whatever the file holds; and a tab
/// TITLE is the current file's name, which the per-pane statusbar strip and nvim's
/// `:e` echo also draw, so no OCR term could discriminate one. The observables that
/// DO discriminate are the process table, read through `/proc`, and a state database
/// frozen before anything can write over it.
///
/// ## Why THREE tabs
///
/// Tab 0 is the first saved tab and tab 2 the one selected when run 1 quits, so each
/// is what some shortcut keeps when it keeps one tab. Tab 1 is neither, and is kept
/// only by a save and a restore that walk every tab.
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` asserts the count so a
/// later trim to two tabs cannot quietly take that tab away.
///
/// ## What it asserts
///
/// The scenario's own checks run inside `cage` and carry their own exit codes (see
/// [`RESTORE_TABS_SCENARIO`]'s table): run 1 reaches three live Neovims before it
/// quits (exit 21), and run 2 restores three (exit 24). Three more are the Rust side's:
///
/// 1. **The SAVE side.** `$TABS_STATE_COPY_1` — run 1's database, frozen before run 2
///    could touch it — holds one window with THREE tabs whose roots are single
///    edit panes named `talfa.md` / `tbravo.md` / `tcharlie.md`, in that
///    order. It runs before the scenario's exit code is read whenever run 1's copy
///    exists, since a save that drops tabs also fails run 2's count at exit 24, and
///    reading the code first would report every save break as a restore break.
/// 2. **The RESTORE side.** The swap map has exactly three rows, each fixture held by
///    exactly one live Neovim, and the three pids distinct: three Neovims, each with
///    its own tab's file loaded.
/// 3. **The BACKGROUND tabs' gather.** `$TABS_STATE_COPY_2` — run 2's database, after
///    a quit in which tabs 1 and 2 were never selected — still holds the same three
///    tabs with the same three roots, in order, each gathered from its own Neovim.
///
/// ## Premise
///
/// That a Neovim holds a loaded buffer's swap file open, under `WOLF_SWAP_DIR` with a
/// `%`-mangled name ending in the file's name and `.swp`, is unmeasured here. A green
/// run measures it; a red assertion 2 with every count right is the first place to
/// read `/proc/<pid>/fd` by hand.
///
/// ## Falsifiers (each run alone)
///
/// | # | edit | predicted red |
/// | --- | --- | --- |
/// | TR1a | in `WindowHandle::cached_window`, collect `tabs.selected()` alone | assertion 1, read before the exit status: run 1's window carries 1 tab, not 3, and the pane list names `tcharlie.md` |
/// | TR1b | in `state_seam::window_entry`, truncate `tabs` to one | assertion 1: 1 tab, and the pane list names `talfa.md` |
/// | TR2a | in `window::build`, open only `plan.tabs[0]` | assertion 1 passes, then the status assertion fails with exit status 24 |
/// | TR2b | in `window::build`, pass `saved: None` for every planned tab after the first | assertion 2: no `tbravo.md` row |
///
/// ## What it does NOT prove
///
/// **Nothing here observes a REFUSED gather.** `pane_layout::resolve_leaves`'
/// all-or-nothing rule needs a leaf whose Neovim window has left both snapshot maps
/// while its pane survives, which is timing-dependent under a compositor. Its table
/// is what covers it.
///
/// **Nothing here rests on a SEED.** `tests/app_state_seam.rs`'s
/// `a_restored_tab_contributes_its_saved_tab` covers a tab whose own gather never
/// succeeds, which under a compositor needs its layout to bind no pane at all.
#[test]
fn three_tabs_survive_a_relaunch_under_cage() {
    const SMOKE: &str = "three_tabs_survive_a_relaunch_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // One cwd for both launches, so run 2's terminal launch restores the session as saved.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (TABS_RESTORE_FILE_ONE, TABS_RESTORE_HEADING_ONE),
            (TABS_RESTORE_FILE_TWO, TABS_RESTORE_HEADING_TWO),
            (TABS_RESTORE_FILE_THREE, TABS_RESTORE_HEADING_THREE),
        ],
    );

    // The smoke's own state dir — the SAME sibling `cage_command` computes and hands
    // the app as WOLF_STATE_DIR, so the two runs share one `wolf.sqlite`.
    let smoke_state = smoke_state_dir(&cwd);

    // Two frozen copies, so assertion 1's red stays apart from 3's: run 2 re-submits every wake, so
    // the live database would hide which side dropped the tabs.
    let run1_state_dir = frozen_state_dir(&runtime_dir, "wolf_gnome_tabs_run1_state", SMOKE);
    let run2_state_dir = frozen_state_dir(&runtime_dir, "wolf_gnome_tabs_run2_state", SMOKE);

    let readback = cwd.join(TABS_RESTORE_READBACK_NAME);
    let run1_state_file = run1_state_dir.join(STATE_DB_NAME);
    // Start from a clean slate: evidence that could be a previous run's leftover is
    // not evidence. `clear_evidence` removes AND asserts absent.
    clear_evidence(&readback, SMOKE);

    let status = cage_command(
        &with_nvim_pid_helper(RESTORE_TABS_SCENARIO),
        &cwd,
        &runtime_dir,
    )
    .env("WOLF_BIN", bin)
    .env("TABS_FILE_ONE", TABS_RESTORE_FILE_ONE)
    .env("TABS_FILE_TWO", TABS_RESTORE_FILE_TWO)
    .env("TABS_FILE_THREE", TABS_RESTORE_FILE_THREE)
    .env("TABS_READBACK", &readback)
    .env("TABS_STATE_SRC", &smoke_state)
    .env("TABS_STATE_COPY_1", &run1_state_dir)
    .env("TABS_STATE_COPY_2", &run2_state_dir)
    .status()
    .expect("failed to spawn `cage` for the headless three-tab relaunch scenario");
    // Assertion 1, the SAVE side, before the status: a save that drops tabs also exits 24.
    // The copy exists only past exit 21 and run 1's clean quit; a green run without it fails below.
    if status.success() || run1_state_file.exists() {
        assert_frozen_tabs(
            SMOKE,
            "run 1",
            &run1_state_dir,
            "The scenario's exit-21 check already proved the window really held THREE tabs, one \
             Neovim each, when it quit, so this is the SAVE side and nothing else. ONE tab is the \
             red for two different defects, and the NAME the survivor carries tells them apart: \
             talfa.md means `state_seam::window_entry` truncated its tabs vec, since \
             `truncate(1)` keeps the FIRST entry; tcharlie.md means `window::cached_window` \
             gathered `tabs.selected()` alone instead of every entry, since that is the tab the \
             user was on at the quit.",
        );
    }
    assert!(
        status.success(),
        "{SMOKE}: the cage three-tab relaunch scenario exited unsuccessfully ({status}). Read the \
         exit code FIRST; each names a different half of the claim.\n\
         Exit 11: run 1's app died before it was ready for input — a LAUNCH failure.\n\
         Exit 12: run 1 was still alive after `:qa`.\n\
         Exit 13: run 1 exited NON-ZERO — it went away, but not cleanly. Three tabs close in \
         sequence here, so a borrow discipline that held at one tab is the first suspect.\n\
         Exit 14: run 2's app died before it was ready for input.\n\
         Exit 16: run 2 was still alive after `:qa`.\n\
         Exit 17: run 2 exited NON-ZERO — it RESTORED tabs, so suspect `window::build` opening \
         the planned tabs.\n\
         Exit 21: run 1 never reached three live Neovims, so Ctrl+Shift+T failed and nothing \
         here is about persistence. That is deliberately separated from the Rust side's \
         assertions, because a one-tab window state and a chord that never fired look identical \
         in the database.\n\
         Exit 22: run 1 wrote NO wolf.sqlite at all in {} — upstream of anything about tabs.\n\
         Exit 23: run 2 left NO wolf.sqlite at all, so assertion 3's evidence is missing \
         (assertion 1 has already passed on run 1's frozen copy).\n\
         Exit 24: run 2 did not restore three live Neovims. Assertion 1 has already passed on run \
         1's frozen copy, so run 1 saved all three tabs and this is the RESTORE alone: \
         `window::build` did not open every planned tab, or a tab's `WolfCore::start` failed (its \
         stderr line says so).\n\
         The scenario's own stderr names the candidate causes for each in order.",
        smoke_state.display()
    );

    // Assertion 2: the restore side, read from the process table rather than any one Neovim.
    let raw = read_readback(&readback, SMOKE);
    let rows: Vec<(&str, &str)> = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.split_once(' ').unwrap_or((line, "")))
        .collect();
    let mut pids: Vec<&str> = Vec::new();
    for expected in TABS_RESTORE_FILES {
        let holders: Vec<&str> = rows
            .iter()
            .filter(|(_, name)| *name == expected)
            .map(|(pid, _)| *pid)
            .collect();
        assert!(
            holders.len() == 1,
            "{SMOKE}: after the relaunch, {} live Neovim(s) hold {expected:?}'s swap file open \
             ({holders:?}), not exactly one. The whole map was {rows:?}. NONE for a tab past the \
             first means its restored Neovim never loaded its root: `window::build` handed that \
             tab no saved tab, or its pane view never rebuilt it. NONE for every fixture with three \
             Neovims live means the premise failed instead: read `/proc/<pid>/fd` by hand. MORE \
             than one means two tabs' Neovims loaded the same file, i.e. layouts were crossed \
             between tabs. The scenario re-read the map until three rows or its budget ran out, \
             so a short map is a missing restore, not a slow one. Map kept at {}.",
            holders.len(),
            readback.display()
        );
        pids.push(holders[0]);
    }
    assert!(
        rows.len() == TABS_RESTORE_FILES.len(),
        "{SMOKE}: the swap map has {} rows, not {}, though each fixture has one holder: some row \
         names something else. The whole map was {rows:?}.",
        rows.len(),
        TABS_RESTORE_FILES.len()
    );
    let distinct: std::collections::HashSet<&str> = pids.iter().copied().collect();
    assert!(
        distinct.len() == TABS_RESTORE_FILES.len(),
        "{SMOKE}: the three fixtures are held by the Neovims {pids:?}, which are not three \
         distinct processes. One Neovim holding several tabs' files is the restore putting \
         several tabs into one core rather than starting a Neovim per tab. Map kept at {}.",
        readback.display()
    );

    // Assertion 3: tabs 1 and 2 were never selected in run 2, so each is gathered from its own
    // background Neovim.
    assert_frozen_tabs(
        SMOKE,
        "run 2",
        &run2_state_dir,
        "Assertion 2 above already proved all three tabs were RESTORED, each into a Neovim of its \
         own, so a tab missing here was lost between the restore and the quit — by the very act \
         of running. Tabs 1 and 2 are never selected in run 2, so their panes can only come from \
         a gather that reads every tab's own Neovim, or from their seeds; `root: None` on them, \
         with tab 0 intact, is a gather that reads the selected tab alone and no seed.",
    );
}

/// One saved tab as the two-window smoke compares it: its directory, the file name of
/// each of its panes in depth-first order, and its zoom.
type TabSummary = (String, Vec<Option<String>>, Option<PaneRef>);

/// One saved window as the two-window smoke compares it: its tabs in page order and its
/// active tab. The frame is left out, since a relaunch need not restore the same size.
type WindowSummary = (Vec<TabSummary>, usize);

fn window_summary(window: &Window) -> WindowSummary {
    let tabs = window
        .tabs
        .iter()
        .map(|tab| {
            (
                tab.directory.clone(),
                tab.content
                    .root
                    .as_ref()
                    .map(pane_tails)
                    .unwrap_or_default(),
                tab.content.zoomed,
            )
        })
        .collect();
    (tabs, window.active_tab)
}

/// **Two windows of two tabs each survive a quit and relaunch in their stacking order,
/// each on its saved active tab and with its zoom, and the project both windows share
/// stores the frontmost window's active tab.**
///
/// Two app launches run inside ONE `cage` instance (see [`TWOWIN_SCENARIO`]), so the
/// smoke holds one cage slot and no second compositor is started.
///
/// ## Why it asserts on no pixel, and why that is the honest shape
///
/// `cage(1)`: "Cage runs a single, maximized application … only a single one is
/// visible at any point in time." MEASURED under `cage`: two
/// `adw::ApplicationWindow`s in one process both exist and `app.windows()` reports
/// 2, but the newest toplevel stays on top and presenting the backgrounded first
/// window does **not** raise it. So no capture can show two windows. The observables
/// that discriminate are the count of live `nvim` children of the app pid, the state
/// database each run wrote, frozen after it quit, and the `getcwd()` and current
/// buffer the focused window's Neovim writes about itself.
///
/// ## Why the order is observable
///
/// Window A is opened first and window B second, so the coordinator's registry lists
/// A first, while B is the window that gains focus last, so the stacking order puts B
/// first. A save in registry order and a save in stacking order therefore differ. On
/// the restore side, the window mapped last is the one `cage` shows and keys reach,
/// so the readback says which window the restore put in front.
///
/// ## The route to the second window is forced
///
/// A second `wolf-gnome` process under the same compositor is not New Window. Every
/// cage has its own private `dbus-run-session` bus, so a second launch while the
/// first is alive hands off to it through single-instance activation, which
/// exercises the launch path rather than a second window. The palette is the only
/// route this harness can drive — New Window carries no accelerator, and the primary
/// menu popover cannot be clicked (see the deliberate gaps in `tests/README.md`).
///
/// ## Step 6 is `app.quit`'s ONLY coverage in this suite
///
/// Every other quit-shaped smoke here goes through Neovim (`:q`, `:qa`) or through
/// `win.close-window`, so the `app.`/`win.` install split — the coordinator
/// installing the two `app.` rows exactly once per process while every window's
/// shell installs only `win.` rows — and `Coordinator::quit`'s ordering are executed
/// by nothing else. Its falsifier is named in the table below.
///
/// ## What it asserts
///
/// The scenario's own checks run inside `cage` and each carries its own exit code
/// (see [`TWOWIN_SCENARIO`]'s table): the Neovim count after each tab and window is
/// opened and after the relaunch, and the new window's not inheriting a project.
/// Five more are the Rust side's, in this order:
///
/// 1. Run 1's save holds two windows, B's first. It is read before the exit status
///    whenever run 1's copy exists, so a write-side failure is never reported as a
///    restore one.
/// 2. Each window's tabs, in page order, with their files and kinds: B holds
///    `wbfirst.md` and `wbactive.md` with the second active; A holds `wafirst.md`
///    and `vertical[waleft.md, waright.md]` with the second active and zoomed on its
///    pane 1. Every tab is in the project, and no frame carries a position.
/// 3. The project's stored state is B's active tab with B's frame: §4's first rule,
///    for a directory open in two windows whose active tabs are both in it.
/// 4. Run 2's focused window is B on its saved active tab: the readback names the
///    project and `wbactive.md`.
/// 5. Run 2's save repeats run 1's: the windows in the same order, each tab's
///    directory, panes and zoom, and each window's active tab.
///
/// ## Premise
///
/// The S1f row rests on window A's active tab gathering in run 2 although A is
/// behind B. A restored tab whose gather never opens contributes the tab run 1 saved,
/// zoom included, which would hide a restore that dropped the zoom. Unmeasured.
///
/// ## What it does NOT prove
///
/// **Nothing here minimizes a window.** `cage` ignores a minimize request, so the
/// minimized half of the stacking order is the seam tables' alone.
///
/// **Nothing here observes the `SwitchWindow` raise.** `Coordinator::raise` calls
/// `present()`, which `cage` declines, so the palette's Switch to Window rows are
/// proven as VALUES by `palette.rs`'s tables and by nothing here.
///
/// **Nothing here closes one of two windows**, so `close_commit`'s `DropSubtree` arm
/// is untouched — and on the LAST window that arm is not merely unobserved but
/// indistinguishable from a no-op, because an empty contribution set means "commit
/// nothing" rather than "commit an empty state". Its table in `state_seam.rs` is what
/// covers the rule.
///
/// **It does not discriminate the close path from the per-wake commit.** The quit
/// path's commit has already been preceded by per-wake commits of the same windows,
/// so this smoke's save would be right by either route; that redundancy is recorded
/// in `tests/README.md`.
///
/// **It cannot see the restore's seed of the stacking order.** `cage` focuses each window
/// as it maps, so those focuses order the save with or without the seed.
///
/// ## Falsifiers (each run alone)
///
/// | # | edit | predicted red | measured |
/// | --- | --- | --- | --- |
/// | S1a | in `state_seam::window_state`, keep the input order | assertion 1: window 0 is A | Red at assertion 1. |
/// | S1b | delete the `on_window_focused` call in `window.rs`'s `is-active` handler | assertion 1: window 0 is A | Red at assertion 1. |
/// | S1c | in `restore_session`, present every window at once, ignoring `present_after` | assertion 4: the readback names `waright.md` or `waleft.md` | Red at assertion 4, naming `waright.md`. |
/// | S1d | in `window::build`, select the first opened tab | assertion 4: the readback names `wbfirst.md` | Red at assertion 4. |
/// | S1e | in `layout_snapshot`, answer `zoomed: None` | assertion 2: A's second tab saved unzoomed | Red at assertion 2. |
/// | S1f | in `apply_layout_rebuild`, skip the `ZoomEvent::Restore` | assertion 5: run 2 saved A's second tab unzoomed | Red at assertion 5. |
/// | F1 | in `command_action.rs`, restore `Command::NewWindow => UiAction::Deferred(..)` | exit 21, with a count of 2 | Not re-measured since the app-state switch. |
/// | F3 | in `coordinator.rs`'s `open_new_window`, pass the process cwd to `restore_plans` | exit 22 | Not re-measured since the app-state switch. |
/// | step 6 | in `coordinator.rs`, delete the body of `install_app_actions` | exit 12 | Not re-measured since the app-state switch. |
///
/// Each row is run alone, per `tests/README.md`'s "A red-run demonstration is a
/// deliverable, not a formality".
#[test]
fn two_windows_two_neovims_and_two_saved_sessions_under_cage() {
    const SMOKE: &str = "two_windows_two_neovims_and_two_saved_sessions_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd shared by BOTH launches and every tab: the one project.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &TWOWIN_FIXTURES);
    let project = std::fs::canonicalize(&cwd)
        .unwrap_or_else(|e| panic!("{SMOKE}: failed to canonicalize {}: {e}", cwd.display()))
        .to_str()
        .unwrap_or_else(|| panic!("{SMOKE}: the temp cwd {} is not UTF-8", cwd.display()))
        .to_owned();

    // Exit 22 compares the last component of window B's `getcwd()`, symlink-proof only while HOME (B's
    // correct answer) cannot share it; asserted so a rename reddens here with its cause named.
    let app_home = smoke_home(&cwd);
    assert_ne!(
        app_home.file_name(),
        Some(OsStr::new(TEMP_CWD_BASENAME)),
        "{SMOKE}: the app's HOME {} has the same final path component as the temp cwd \
         ({TEMP_CWD_BASENAME:?}). Window B opened by New Window starts at HOME, and the scenario's \
         exit-22 check reads \"the new window inherited the project\" off exactly that component — \
         so with the two sharing it, a CORRECT window B would be reported as an inherited one and \
         the smoke would fail for a fixture reason while naming a coordinator bug.",
        app_home.display()
    );

    // The smoke's own state dir — the SAME sibling `cage_command` computes and hands
    // the app as WOLF_STATE_DIR, so the two runs share one `wolf.sqlite`.
    let smoke_state = smoke_state_dir(&cwd);
    let run1_state_dir = frozen_state_dir(&runtime_dir, "wolf_gnome_twowin_run1_state", SMOKE);
    let run2_state_dir = frozen_state_dir(&runtime_dir, "wolf_gnome_twowin_run2_state", SMOKE);

    let b_cwd_readback = cwd.join(TWOWIN_READBACK_B_NAME);
    let run2_readback = cwd.join(TWOWIN_READBACK_R2_NAME);
    for path in [&b_cwd_readback, &run2_readback] {
        clear_evidence(path, SMOKE);
    }

    // This scenario calls none of the fragment's exiting functions, so its 16/17/18/20 are unreachable.
    let scenario = with_nvim_pid_helper(TWOWIN_SCENARIO);
    let status = cage_command(&scenario, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("TWOWIN_A_FIRST_FILE", TWOWIN_A_FIRST_FILE)
        .env("TWOWIN_A_LEFT_FILE", TWOWIN_A_LEFT_FILE)
        .env("TWOWIN_A_ZOOMED_FILE", TWOWIN_A_ZOOMED_FILE)
        .env("TWOWIN_B_FIRST_FILE", TWOWIN_B_FIRST_FILE)
        .env("TWOWIN_B_ACTIVE_FILE", TWOWIN_B_ACTIVE_FILE)
        .env("TWOWIN_PROJECT_ABS", &cwd)
        .env("TWOWIN_PROJECT_BASE", TEMP_CWD_BASENAME)
        .env("TWOWIN_NEW_WINDOW_QUERY", TWOWIN_NEW_WINDOW_QUERY)
        .env("TWOWIN_B_CWD", &b_cwd_readback)
        .env("TWOWIN_R2_READBACK", &run2_readback)
        .env("TWOWIN_STATE_SRC", &smoke_state)
        .env("TWOWIN_STATE_COPY", &run1_state_dir)
        .env("TWOWIN_STATE_COPY_2", &run2_state_dir)
        .env("RESTORE_SETTLE", RELAUNCH_RESTORE_SETTLE)
        .status()
        .expect("failed to spawn `cage` for the headless two-window scenario");

    // Assertions 1-3 read run 1's copy before the status, so a write-side failure that also
    // breaks the restore is reported as what it is. The copy exists only past run 1's quit.
    let run1 = run1_state_dir
        .join(STATE_DB_NAME)
        .exists()
        .then(|| frozen_window_state(&run1_state_dir, SMOKE, "run 1's save"));
    if let Some(saved) = &run1 {
        let firsts: Vec<Option<String>> = saved
            .windows
            .iter()
            .map(|window| {
                window
                    .tabs
                    .first()
                    .and_then(|tab| tab.content.root.as_ref())
                    .and_then(|root| pane_tails(root).into_iter().next().flatten())
            })
            .collect();
        assert_eq!(
            firsts,
            [
                Some(TWOWIN_B_FIRST_FILE.to_owned()),
                Some(TWOWIN_A_FIRST_FILE.to_owned())
            ],
            "{SMOKE}: run 1 saved windows whose first tabs show {firsts:?}; want window B's \
             ({TWOWIN_B_FIRST_FILE}) first, then window A's ({TWOWIN_A_FIRST_FILE}). B gained \
             focus last, so it is the frontmost window and the window state lists it first. A \
             first means the windows were written in the order they were opened: \
             `state_seam::window_state` did not order them by the stacking order, or no focus \
             change ever reached it (`Coordinator::on_window_focused` from the window's \
             `is-active` handler). One entry means the save lost a window; `None` for B means \
             its tab never left the empty state, so the typed `:cd` did not un-empty it. Frozen \
             copy: {}",
            run1_state_dir.display()
        );

        let tails = |names: &[&str]| -> Vec<Option<String>> {
            names.iter().map(|name| Some((*name).to_owned())).collect()
        };
        let expected: Vec<WindowSummary> = vec![
            (
                vec![
                    (project.clone(), tails(&[TWOWIN_B_FIRST_FILE]), None),
                    (project.clone(), tails(&[TWOWIN_B_ACTIVE_FILE]), None),
                ],
                1,
            ),
            (
                vec![
                    (project.clone(), tails(&[TWOWIN_A_FIRST_FILE]), None),
                    (
                        project.clone(),
                        tails(&[TWOWIN_A_LEFT_FILE, TWOWIN_A_ZOOMED_FILE]),
                        Some(PaneRef::Pane(1)),
                    ),
                ],
                1,
            ),
        ];
        let got: Vec<WindowSummary> = saved.windows.iter().map(window_summary).collect();
        assert_eq!(
            got,
            expected,
            "{SMOKE}: run 1 saved the windows ((directory, panes, zoomed) per tab, active tab) \
             {got:?}; want {expected:?}. Read it this way: a tab missing means the save dropped \
             it; a directory other than {project:?} means a tab's Neovim is not where the \
             scenario put it, and \"\" means that tab was saved as an empty tab; A's second tab \
             with zoomed None means the zoom was not saved (`layout_snapshot` answers no index), \
             and Some(Pane(0)) that it names the wrong pane; an active tab of 0 means the \
             selected tab is not what is written. Frozen copy: {}",
            run1_state_dir.display()
        );
        for (w, window) in saved.windows.iter().enumerate() {
            assert!(
                window.frame.x.is_none() && window.frame.y.is_none(),
                "{SMOKE}: run 1 saved window {w} with the position ({:?}, {:?}); want both null, \
                 since a Wayland client is never told its position and a number would be read \
                 back as one. Frozen copy: {}",
                window.frame.x,
                window.frame.y,
                run1_state_dir.display()
            );
            for (t, tab) in window.tabs.iter().enumerate() {
                let kinds_ok = tab.content.root.as_ref().is_some_and(|root| {
                    root.panes()
                        .iter()
                        .all(|pane| matches!(pane, Pane::Edit { .. }))
                });
                assert!(
                    kinds_ok,
                    "{SMOKE}: run 1 saved window {w}'s tab {t} with the root {:?}; every pane the \
                     scenario opens is an editor, so each must be saved as kind `edit`. Frozen \
                     copy: {}",
                    tab.content.root,
                    run1_state_dir.display()
                );
            }
        }

        // Assertion 3: §4's first rule picks the frontmost window whose active tab is in the
        // directory, and both windows' active tabs are.
        let front = &saved.windows[0];
        let stored = frozen_project_state(&run1_state_dir, &project);
        let want = ProjectState {
            frame: front.frame,
            tab: front.tabs[front.active_tab].content.clone(),
        };
        assert_eq!(
            stored.as_ref(),
            Some(&want),
            "{SMOKE}: run 1 stored the project state {stored:?} for {project:?}; want window B's \
             active tab ({TWOWIN_B_ACTIVE_FILE}) with B's frame. Window A's active tab \
             ({TWOWIN_A_LEFT_FILE} beside {TWOWIN_A_ZOOMED_FILE}) means the project state was \
             chosen from a window order other than the one saved, or from the last window \
             written; None means no project state was written for a directory with tabs open. \
             Frozen copy: {}",
            run1_state_dir.display()
        );
    }

    assert!(
        status.success(),
        "{SMOKE}: the cage two-window scenario exited unsuccessfully ({status}). Read the exit \
         code FIRST; each names a different half of the claim.\n\
         Exit 11: run 1's app died before it was ready for input — a LAUNCH failure.\n\
         Exit 12: run 1 was still alive after Ctrl+Shift+Q. That chord is this suite's ONLY \
         exercise of `app.quit`, so suspect `Coordinator::install_app_actions` before anything \
         else: with no `app.quit` installed the chord controller declines the key and Neovim gets \
         a plain <C-Q>.\n\
         Exit 13: run 1 exited NON-ZERO — it went away, but not cleanly. Two windows close in \
         sequence here, so a borrow discipline that held at one window is the first suspect.\n\
         Exit 14: run 2's app died before it was ready for input.\n\
         Exit 15: a Neovim readback file never appeared.\n\
         Exit 16: run 1 wrote NO wolf.sqlite at all in {} — upstream of anything about windows.\n\
         Exit 21: run 1 did not have EXACTLY THREE live `nvim` children after the palette \
         activation. Every tab runs a core of its own, so a count of 2 means no second window \
         was built (check `ui_action_for(Command::NewWindow)`, `AppShell::perform`'s arm, and \
         `Coordinator::open_new_window`).\n\
         Exit 22: the new window INHERITED the invoking window's project directory. New Window \
         must build its plan from no saved state and no process cwd.\n\
         Exit 23: run 2 did not have EXACTLY FOUR live `nvim` children after the restore, so \
         the restore did not plan every saved window and tab. Assertions 1 and 2 have already \
         passed on run 1's copy, so this is the restore alone.\n\
         Exit 26: step 4's readback came from the INVOKING window (it names one of window A's \
         files), so the keystrokes never reached the new one. That is a focus failure and NOT \
         the inheritance exit 22 reports.\n\
         Exit 27: window A's Ctrl+Shift+T did not reach two live Neovims, so nothing below is \
         about windows.\n\
         Exit 28: window B's Ctrl+Shift+T did not reach four live Neovims: a New Tab in a window \
         opened after launch opened no tab.\n\
         Exit 29 or 30: run 2 was still alive after Ctrl+Shift+Q, or exited non-zero, as 12 and \
         13 are for run 1.\n\
         Exit 31: run 2 left no wolf.sqlite, so assertion 5's evidence is missing.\n\
         The scenario's own stderr names the candidate causes for each in order.",
        smoke_state.display()
    );
    let run1 = run1.unwrap_or_else(|| {
        panic!(
            "{SMOKE}: the scenario exited 0 but froze no copy of run 1's save in {}",
            run1_state_dir.display()
        )
    });

    // Assertion 4: the restore put window B in front, on its saved active tab.
    let raw = read_readback(&run2_readback, SMOKE);
    let mut lines = raw.lines().map(str::trim);
    let (r2_cwd, r2_buf) = (
        lines.next().unwrap_or_default(),
        lines.next().unwrap_or_default(),
    );
    assert!(
        r2_cwd == project && r2_buf == TWOWIN_B_ACTIVE_FILE,
        "{SMOKE}: after the relaunch the focused window's Neovim is at {r2_cwd:?} showing \
         {r2_buf:?}; want {project:?} showing {TWOWIN_B_ACTIVE_FILE}, window B's saved active \
         tab. {TWOWIN_A_ZOOMED_FILE} or {TWOWIN_A_LEFT_FILE} means window A was mapped last, so \
         the restore did not map the saved frontmost window last; {TWOWIN_B_FIRST_FILE} means \
         window B came back on its first tab rather than its saved active one; an `Untitled` \
         buffer means the focused tab's saved root was never rebuilt. Readback kept at {}.",
        run2_readback.display()
    );

    // Assertion 5: run 2's save repeats run 1's, which also re-proves the zoom was restored.
    let run2 = frozen_window_state(&run2_state_dir, SMOKE, "run 2's save");
    let first: Vec<WindowSummary> = run1.windows.iter().map(window_summary).collect();
    let second: Vec<WindowSummary> = run2.windows.iter().map(window_summary).collect();
    assert_eq!(
        second,
        first,
        "{SMOKE}: run 2 saved the windows {second:?}, where run 1 saved {first:?} (each \
         ((directory, panes, zoomed) per tab, active tab)). A's second tab unzoomed here alone \
         means run 2 restored the layout but not its zoom, so its own gather saved it \
         unzoomed: `apply_layout_rebuild` applied no `ZoomEvent::Restore`. The windows in the \
         other order means the window mapped last was not the saved frontmost. A different \
         active tab means the restore selected another tab. Frozen copies: {} and {}",
        run1_state_dir.display(),
        run2_state_dir.display()
    );
}

// Stale-state fixtures: no capture is taken, so the names serve the generic fixture loop only.

/// The file the planted `state.json` names as its one tab's pane. It exists in the temp cwd,
/// so an import of the planted file would have a real file to restore.
pub(crate) const STALE_STATE_FILE: &str = "sjplanted.md";

/// Name of the file the launched window's Neovim writes two lines into: its window count
/// and the tail of `bufname('%')`.
pub(crate) const STALE_STATE_READBACK_NAME: &str = "sjread.txt";

/// The legacy state files of `specs/application-state.md` §1 "Files Wolf removes" that
/// [`a_stale_state_json_is_deleted_at_launch_under_cage`] plants, each a well-formed file of
/// the old format.
const STALE_STATE_NAMES: [&str; 2] = ["state.json", "state.json.tmp"];

/// The scenario of [`a_stale_state_json_is_deleted_at_launch_under_cage`]: one launch into a
/// state directory holding a planted `state.json` and `state.json.tmp`, a check that both
/// are gone once the app is ready, a readback of what the window shows, and a quit.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | the app died before it was ready for input |
/// | `12` | the app was still alive after `:qa` |
/// | `13` | the app exited non-zero |
/// | `15` | the Neovim readback never appeared |
/// | `16` | the app left no `wolf.sqlite` |
/// | `21` | `state.json` still exists once the app is ready |
/// | `22` | `state.json.tmp` still exists once the app is ready |
const STALE_STATE_SCENARIO: &str = r#"
set -eu

WAIT_INTERVAL=0.2
QUIT_TRIES=75
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
    echo "wolf-gnome stale-state smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome stale-state smoke"

# The store opens before the first window is built, so both files are gone by now.
if [ -e "$STALE_STATE_DIR/state.json" ]; then
    echo "wolf-gnome stale-state smoke: $STALE_STATE_DIR/state.json survived the launch." >&2
    echo "  The store's open deletes the legacy files: suspect state.json missing from its" >&2
    echo "  removal list, or the app opening no store in this directory." >&2
    exit 21
fi
if [ -e "$STALE_STATE_DIR/state.json.tmp" ]; then
    echo "wolf-gnome stale-state smoke: $STALE_STATE_DIR/state.json.tmp survived the launch," >&2
    echo "  while state.json did not: the removal list names the one and not the other." >&2
    exit 22
fi

wtype -- ":call writefile([printf('%d', winnr('\$')), fnamemodify(bufname('%'), ':t')], '$STALE_READBACK')"
wtype -k Return
wait_file "$STALE_READBACK" || exit 15

wtype -- ":qa"
wtype -k Return
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
    echo "wolf-gnome stale-state smoke: the app (pid $app_pid) is still alive after :qa." >&2
    exit 12
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome stale-state smoke: the app (pid $app_pid) exited $rc, not 0." >&2
    exit 13
fi
app_pid=

if ! freeze_state "$STALE_STATE_DIR" "$STALE_STATE_COPY"; then
    echo "wolf-gnome stale-state smoke: the app left no wolf.sqlite in $STALE_STATE_DIR." >&2
    exit 16
fi

exit 0
"#;

/// **A stale `state.json` and `state.json.tmp` in the state directory are deleted at
/// launch, and nothing of them is restored.**
///
/// One launch in one cage. The smoke's state directory holds a well-formed file of the old
/// format under each of [`STALE_STATE_NAMES`]: one window whose one tab is at the temp cwd
/// and shows [`STALE_STATE_FILE`], which exists there, so an import would have a whole
/// window to restore. The scenario exits with its own code when either file is still there
/// once the app is ready; the app opens its store, and so deletes them, before it builds a
/// window.
///
/// ## Premises
///
/// Two observations confirm the launch imported nothing, and no edit to the deletion can
/// make either fail, since nothing reads the planted files:
///
/// - The launched window holds one pane, on a buffer other than [`STALE_STATE_FILE`].
/// - The save the app leaves at its quit names [`STALE_STATE_FILE`] in no pane.
///
/// ## Falsifiers (each run alone)
///
/// | # | edit | predicted red |
/// | --- | --- | --- |
/// | S2a | remove `"state.json"` from `db.rs`'s `LEGACY_FILE_NAMES` | exit 21 |
/// | S2b | remove `"state.json.tmp"` from `LEGACY_FILE_NAMES` | exit 22 |
#[test]
fn a_stale_state_json_is_deleted_at_launch_under_cage() {
    const SMOKE: &str = "a_stale_state_json_is_deleted_at_launch_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[(STALE_STATE_FILE, "# SJPLANTED")]);
    let project = std::fs::canonicalize(&cwd)
        .unwrap_or_else(|e| panic!("{SMOKE}: failed to canonicalize {}: {e}", cwd.display()));

    // The old format: a version header and a tree of windows, each tab with its cwd and layout.
    let old = serde_json::json!({
        "version": 1,
        "windows": [{
            "x": 0.0,
            "y": 0.0,
            "width": 1280.0,
            "height": 720.0,
            "tabs": [{
                "cwd": project.to_string_lossy(),
                "sidebar_visible": true,
                "layout": {
                    "type": "leaf",
                    "path": project.join(STALE_STATE_FILE).to_string_lossy(),
                    "preview": false,
                },
            }],
        }],
    });
    let text = serde_json::to_string_pretty(&old)
        .unwrap_or_else(|e| panic!("{SMOKE}: failed to render the old state file: {e}"));
    let smoke_state = smoke_state_dir(&cwd);
    for name in STALE_STATE_NAMES {
        let path = smoke_state.join(name);
        std::fs::write(&path, &text)
            .unwrap_or_else(|e| panic!("{SMOKE}: failed to plant {}: {e}", path.display()));
    }

    let frozen = frozen_state_dir(&runtime_dir, "wolf_gnome_stale_state_copy", SMOKE);
    let readback = cwd.join(STALE_STATE_READBACK_NAME);
    clear_evidence(&readback, SMOKE);

    let status = cage_command(STALE_STATE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("STALE_STATE_DIR", &smoke_state)
        .env("STALE_STATE_COPY", &frozen)
        .env("STALE_READBACK", &readback)
        .status()
        .expect("failed to spawn `cage` for the headless stale-state scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage stale-state scenario exited unsuccessfully ({status}). Exit 21: \
         state.json was still in {} once the app was ready, so the store's open did not delete \
         it. Exit 22: state.json.tmp was, while state.json was not, so the open deletes one and \
         not the other. Exit 11: the app died before it was ready, which a launch that tried to \
         read the planted file and failed would also do. Exit 12 or 13: the quit hung or was \
         not clean. Exit 15: the Neovim readback never appeared. Exit 16: the app left no \
         wolf.sqlite. Exit 40: the app never reported ready.",
        smoke_state.display()
    );

    // Premise: the window shows one pane, on none of the planted file.
    let raw = read_readback(&readback, SMOKE);
    let mut lines = raw.lines().map(str::trim);
    let (windows, buffer) = (
        lines.next().unwrap_or_default(),
        lines.next().unwrap_or_default(),
    );
    assert!(
        windows == "1" && buffer != STALE_STATE_FILE,
        "{SMOKE}: the launched window's Neovim reports {windows} window(s) on {buffer:?}; want one \
         window on a buffer other than {STALE_STATE_FILE}. {STALE_STATE_FILE} means the launch \
         restored the planted state.json's tab, which nothing may import. Readback kept at {}.",
        readback.display()
    );

    // Premise: nothing of the planted file reached the save.
    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    let planted: Vec<(usize, usize)> = saved
        .windows
        .iter()
        .enumerate()
        .flat_map(|(w, window)| {
            window
                .tabs
                .iter()
                .enumerate()
                .map(move |(t, tab)| (w, t, tab))
        })
        .filter(|(_, _, tab)| {
            tab.content.root.as_ref().is_some_and(|root| {
                pane_tails(root)
                    .iter()
                    .any(|tail| tail.as_deref() == Some(STALE_STATE_FILE))
            })
        })
        .map(|(w, t, _)| (w, t))
        .collect();
    assert!(
        planted.is_empty(),
        "{SMOKE}: the save at the quit shows {STALE_STATE_FILE} in the (window, tab)s \
         {planted:?}. No step of the scenario opens it, so the planted state.json was imported. \
         Frozen copy: {}",
        frozen.display()
    );
}

// Three-window fixtures: no capture is taken, so the names serve the generic fixture loop only.

/// The file of the planted backmost window A's one pane.
pub(crate) const THREEWIN_A_FILE: &str = "wsa.md";

/// The file of the planted middle window B's one pane.
pub(crate) const THREEWIN_B_FILE: &str = "wsb.md";

/// The file of the planted frontmost window C's one pane.
pub(crate) const THREEWIN_C_FILE: &str = "wsc.md";

/// The three fixtures with the heading written into each. Nothing reads a heading: the
/// bodies exist only so each planted pane restores on a real file.
const THREEWIN_FIXTURES: [(&str, &str); 3] = [
    (THREEWIN_A_FILE, "# STACKALFA"),
    (THREEWIN_B_FILE, "# STACKBRAVO"),
    (THREEWIN_C_FILE, "# STACKCHARLIE"),
];

/// How long the planting's flush may take before the smoke gives up on it.
const THREEWIN_PLANT_FLUSH: Duration = Duration::from_secs(5);

/// The scenario of [`three_windows_restore_in_their_stacking_order_under_cage`]: one launch
/// over a planted three-window state, a count of its Neovims, the `Ctrl+Shift+Q` quit and a
/// frozen copy of the save it leaves.
///
/// ## Exit codes
///
/// The scenario calls only `wait_nvim_count` of [`with_nvim_pid_helper`]'s fragment, which
/// never exits on its own, so that fragment's `16` is unreachable and `16` means what this
/// table says.
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | the app died before it was ready for input |
/// | `12` | the app was still alive after `Ctrl+Shift+Q` |
/// | `13` | the app exited non-zero |
/// | `16` | the app left no `wolf.sqlite` |
/// | `21` | the app did not have exactly three live `nvim` children once ready |
/// | `40` | the app never reported ready, which a chain of presents that stalls also gives |
const THREEWIN_SCENARIO: &str = r#"
set -eu

# FILE_TRIES and WAIT_INTERVAL are also `wait_nvim_count`'s budget, since it reads them.
WAIT_INTERVAL=0.2
QUIT_TRIES=75
FILE_TRIES=100

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
# Ready only once every window has painted, so a chain whose next present never runs exits 40.
wait_ready "$app_pid" 11 "wolf-gnome three-window smoke"

# One tab per planted window; a fourth means a launch window was added in front of them.
wait_nvim_count 3 21 "after the restore of the three planted windows"

# C, mapped last, holds the keyboard focus, so the quit reaches it.
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
    echo "wolf-gnome three-window smoke: the app (pid $app_pid) is still alive after Ctrl+Shift+Q." >&2
    echo "  Suspect Coordinator::quit not closing a window, such as one whose present was" >&2
    echo "  still deferred when the close loop reached it." >&2
    exit 12
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome three-window smoke: the app (pid $app_pid) exited $rc, not 0." >&2
    exit 13
fi
app_pid=

if ! freeze_state "$THREEWIN_STATE_SRC" "$THREEWIN_STATE_COPY"; then
    echo "wolf-gnome three-window smoke: the app left no wolf.sqlite in $THREEWIN_STATE_SRC." >&2
    exit 16
fi

exit 0
"#;

/// **Three planted windows come back in their stacking order: the save the relaunch leaves
/// lists them frontmost first, as they were planted.**
///
/// One launch in one cage, over a state the Rust side plants through `AppStateStore`: three
/// windows, frontmost first C, B and A, each with one tab in the project on its own fixture.
/// The scenario waits until the app is ready, counts three Neovims, quits with
/// `Ctrl+Shift+Q` and freezes the save.
///
/// ## Why the order is observable
///
/// The restore builds the windows backmost first, A, B, C, and presents each only after
/// the one behind it has painted. `cage` focuses every toplevel as it maps (`view_map` in
/// cage's `view.c`), so each map is a focus, and the save lists the windows in reverse map
/// order. The planted order, C, B, A, therefore comes back only when they mapped A, B, C.
/// The quit key reaches C, the window mapped last.
///
/// ## Premises
///
/// - Reading the planted state back gives the three windows in the planted order. Asserted
///   before the launch.
/// - `wait_ready` passing means all three windows painted, since readiness needs a painted
///   frame of every launch window. A chain of presents that stalls therefore exits 40.
///
/// ## What it does NOT prove
///
/// **It cannot see the restore's seed of the stacking order.** Each map's focus moves that
/// window to the front, so the save is the same with or without the seed. `stacking.rs`
/// covers the seed rule, and nothing covers its install.
///
/// **It does not show the order on mutter.** Whether mutter focuses each restored window as
/// it maps is a person check.
///
/// **Nothing here minimizes a window**, for S1's reason.
///
/// ## Falsifiers (each run alone)
///
/// Both reds rest on the map order of a burst of presents, which GTK does not promise.
///
/// | # | edit | predicted red | measured |
/// | --- | --- | --- | --- |
/// | S3b | in `restore_session`, present every window at once (`show_after(&[])` for each), ignoring `present_after` | the save lists an order other than the planted one, such as `[wsa, wsb, wsc]` | Red: the save lists `[wsa, wsb, wsc]`. |
/// | S3c | in `restore_session`, ignore `present_after`: present every built window but the last with `show_after(&[])`, and the last, the frontmost, with `show_after` of every other built window that is not minimized | the save lists `[wsc, wsa, wsb]` | Red: the save lists `[wsc, wsa, wsb]`. |
#[test]
fn three_windows_restore_in_their_stacking_order_under_cage() {
    const SMOKE: &str = "three_windows_restore_in_their_stacking_order_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Canonical, so the planted directory holds the launch cwd and no launch window is added.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &THREEWIN_FIXTURES);
    let project = std::fs::canonicalize(&cwd)
        .unwrap_or_else(|e| panic!("{SMOKE}: failed to canonicalize {}: {e}", cwd.display()));
    let project_str = project
        .to_str()
        .unwrap_or_else(|| panic!("{SMOKE}: the temp cwd {} is not UTF-8", cwd.display()))
        .to_owned();

    let planted_window = |file: &str| Window {
        frame: Frame {
            x: None,
            y: None,
            width: 800.0,
            height: 600.0,
        },
        active_tab: 0,
        minimized: false,
        tabs: vec![Tab {
            directory: project_str.clone(),
            content: TabContent {
                root: Some(Node::Pane(Pane::Edit {
                    path: project.join(file).to_string_lossy().into_owned(),
                    cursor: None,
                    top_line: None,
                })),
                ..TabContent::default()
            },
        }],
    };
    let planted = WindowState {
        windows: vec![
            planted_window(THREEWIN_C_FILE),
            planted_window(THREEWIN_B_FILE),
            planted_window(THREEWIN_A_FILE),
        ],
    };

    let smoke_state = smoke_state_dir(&cwd);
    {
        let store = AppStateStore::open(&smoke_state);
        store.submit(planted.clone(), vec![vec![0]; 3]);
        assert!(
            store.flush(THREEWIN_PLANT_FLUSH),
            "{SMOKE}: planting the three-window state in {} did not flush within \
             {THREEWIN_PLANT_FLUSH:?}, so the launch would restore whatever the file holds",
            smoke_state.display()
        );
    }
    assert_eq!(
        AppStateStore::open(&smoke_state).window_state().as_ref(),
        Some(&planted),
        "{SMOKE}: premise: the planted window state must read back as planted, C, B, A; \
         otherwise the order the save is compared with is not the one the launch restores"
    );

    let frozen = frozen_state_dir(&runtime_dir, "wolf_gnome_threewin_state", SMOKE);
    let scenario = with_nvim_pid_helper(THREEWIN_SCENARIO);
    let status = cage_command(&scenario, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("THREEWIN_STATE_SRC", &smoke_state)
        .env("THREEWIN_STATE_COPY", &frozen)
        .status()
        .expect("failed to spawn `cage` for the headless three-window scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage three-window scenario exited unsuccessfully ({status}). Exit 11: the \
         app died before it was ready. Exit 40: it never reported ready, which a window whose \
         deferred present never ran gives, since readiness waits for every window to paint. \
         Exit 21: the launch did not run exactly three Neovims, so the restore lost a planted \
         window, or a launch window was added because the cwd was not taken as held. Exit 12 or \
         13: the quit hung or was not clean. Exit 16: the app left no wolf.sqlite in {}.",
        smoke_state.display()
    );

    let saved = frozen_window_state(&frozen, SMOKE, "the save at the quit");
    let order: Vec<Vec<Option<String>>> = saved
        .windows
        .iter()
        .map(|window| {
            window
                .tabs
                .iter()
                .filter_map(|tab| tab.content.root.as_ref())
                .flat_map(pane_tails)
                .collect()
        })
        .collect();
    let want: Vec<Vec<Option<String>>> = [THREEWIN_C_FILE, THREEWIN_B_FILE, THREEWIN_A_FILE]
        .iter()
        .map(|file| vec![Some((*file).to_owned())])
        .collect();
    assert_eq!(
        order,
        want,
        "{SMOKE}: the save at the quit lists the windows by their panes' files as {order:?}; want \
         {want:?}, the planted order. Under `cage` every map is a focus, so any other order means \
         the windows did not map backmost first, each after the one behind it: the restore's \
         chain of presents is broken. [wsa, wsb, wsc] is what presenting every window at once \
         gives, the windows mapping front to back, and [wsc, wsa, wsb] is what presenting the \
         back windows in one burst ahead of C gives. Fewer than three windows means the restore \
         lost one. Frozen copy: {}",
        frozen.display()
    );
}

/// A dragged divider's weights reach the saved window state and come back on the next
/// launch.
///
/// The round trip, driven by a weight that is NOT an equal share — which is what
/// makes "the saved weights were applied" distinguishable from "the equal-share
/// default was recomputed" at all.
///
/// Two app launches run inside ONE `cage` instance, with `wolf-pointer-inject` alive
/// across both; see [`DIVIDER_RELAUNCH_SCENARIO`] for the exit-code table and for
/// why it duplicates the pointer plumbing rather than reusing
/// [`POINTER_PREAMBLE`].
///
/// ## What it asserts
///
/// 1. **The save side, on the DATABASE.** The window state RUN 1 wrote — frozen by
///    the scenario before run 2 can write over it — carries a `Vertical` layout of two
///    named panes whose weights sum to 1 and differ by at least
///    [`MIN_WEIGHT_GAP`]. Only a drag writes such a vector, through
///    `PaneTree::set_divider_fraction`; `layout.rs`'s other writers produce `1/n` and
///    `[0.5, 0.5]`.
/// 2. **The restore side, on run 2's PIXELS.** The restored panes' width ratio
///    matches run 1's post-drag ratio and sits at least [`MIN_RATIO_OFFSET`] off
///    centre. The second clause is what stops the first being satisfied by a
///    re-centred pair.
/// 3. **Corroboration.** OCR carries both headings, so two panes really did bind
///    grids and paint. The capture is read twice — `--psm 11` through
///    [`ocr_to_lowercase`] and `--psm 6` through [`ocr_uniform_block_to_lowercase`]
///    — and each term is accepted from either reading, for the reason under
///    "Reading a narrow column" below. Both terms are still demanded literally.
///
/// The two preconditions before them — the baseline pair is equal, and the drag
/// moved it — are what separate "the press missed the divider handle" from "the
/// weight was not written back". Without them a `[0.5, 0.5]` on disk would be
/// ambiguous between the two.
///
/// ## Measured geometry, and what the assertions have margin against
///
/// Every width here is `winwidth()` — the GRID width the front-end drives through
/// `resize_grid`. `nvim_win_get_width()` is the LAYOUT width and cannot see a
/// divider drag at all; see [`DIVIDER_RELAUNCH_SCENARIO`]'s `read_widths` for the
/// measurement that pins the difference.
///
/// MEASURED 2026-08-04 on a **1280x720** cage output, from a green run:
///
/// | quantity | value |
/// | --- | --- |
/// | the paned, read live | span 1029 px, `min_position` 63, `max_position` 965 |
/// | run 1 baseline, a fresh split with equal weights | 21 and 21 cells |
/// | run 1 post-drag | 28 and 13 cells |
/// | run 2's restored panes | 28 and 13 cells, identical to run 1's |
/// | the narrow pane against the asserted TERM | 13 columns against 12 characters |
///
/// The paned's three numbers are one measurement rather than three: `965` is
/// `(1029 - 1) - 63`, the span less the handle less the end child's floor, and `63`
/// is `editor_inset::min_pane_size`'s own answer at this cell size. `geometry`'s
/// [`equal_weights_give_equal_widths`] carries that exact paned as a table row, so
/// the same arithmetic is asserted from a unit test with no compositor.
///
/// The baseline PAIR is the thing to read first. Equal weights give equal widths to
/// within the handle, which is what makes `21 and 21` the expected shape rather than
/// a lucky one; a spread of cells there is a product defect with its own unit test,
/// not a tolerance to widen (see [`BASELINE_TOLERANCE_CELLS`]).
///
/// The derived margins, from those cell counts: [`MIN_DRAG_CELLS`] (3) sits against a
/// move of 7 and 8 cells, [`RATIO_TOLERANCE`] (0.08) against a run-1-to-run-2
/// difference of 0 (the counts are identical), and [`MIN_RATIO_OFFSET`] (0.08)
/// against a ratio of `28/41` = 0.683, i.e. 0.183 off centre.
///
/// ## What actually bounds the drag — and it is NOT the heading
///
/// The tightest margin belongs to assertion 3, and its reason is easy to get wrong.
/// The narrow pane is 13 columns and the heading `# DIVIDERBRAVO` is 14 characters,
/// so the heading does **not** fit — and does not need to. `runtime/nvim/init.lua`
/// sets `wrap` and `linebreak`, so the line breaks at the SPACE: the `#` lands alone
/// on the first screen row and `DIVIDERBRAVO` — the 12-character asserted term — lands
/// whole on the second, inside 13 columns with one to spare.
///
/// So the binding constraint is **12 characters in 13 columns**, and it holds only
/// because `linebreak` never splits a word. Widening [`DIVIDER_RELAUNCH_DRAG_PX`]
/// narrows that pane until the term itself no longer fits, at which point `linebreak`
/// has nowhere left to break and assertion 3 reads two fragments.
///
/// **Do not lower the drag "so the 14-character heading fits":** that constraint does
/// not exist, and tuning against it would trade a real margin for an imaginary one.
/// The two things that would break the assertion are a narrow pane under 12 columns
/// and a `linebreak` that stopped breaking at the space. The constant stays at its
/// current value; both preconditions were measured to pass at it.
///
/// ## Reading a narrow column
///
/// The ablations below were run on a 15-column pane, before the per-pane chrome
/// narrowed it to 13. Both findings are properties of the RENDER rather than of that
/// width — an end-of-buffer marker column and a misclassified glyph — and the
/// narrower the column, the more each one matters, so the answers hold a fortiori.
/// The counts are left as they were measured.
///
/// Fitting the term is necessary and was not sufficient, and two separate
/// things had to be answered before assertion 3 could read the narrow pane. One is
/// answered by FIXTURE and one by the READER; each was isolated by measurement on a
/// retained PNG rather than argued.
///
/// | what corrupted the reading | how it was isolated | what answers it |
/// | --- | --- | --- |
/// | the `~` end-of-buffer column above a lone heading — a sparse region `--psm 11` scales the whole pane from | three ablations on one PNG, one at a time; blanking the `~` column alone fixed it | [`DIVIDER_BODY_LINE_COUNT`] body lines, which fill the pane past `winheight` so no marker is drawn |
/// | a single misclassified GLYPH at 15 columns | the same pixels read through both segmentation modes, twice over | the second reader, [`ocr_uniform_block_to_lowercase`] |
///
/// The second is why this smoke, alone among the smokes, reads its capture twice.
/// With the first answered, on one retained PNG:
///
/// | command | verbatim output |
/// | --- | --- |
/// | `tesseract PNG stdout --psm 11` | `# DIVIDERBkAVO` — the `R` of `BRAVO` read as a `k` |
/// | `tesseract PNG stdout --psm 6` | `A 7 DIVIDERALFA # DIVIDERBRAVO` — both asserted terms correct |
///
/// It is deterministic rather than flaky: two independently
/// rendered screenshots gave byte-identical text in both modes, and `DIVIDERALFA` in
/// the wide pane reads correctly under both. The premise is stated where the reader
/// lives: a two-pane page whose narrow column is one heading over a block of body
/// text is a UNIFORM text block, which is what `--psm 6` assumes, and not the sparse
/// layout `--psm 11` assumes. `--psm 11` is untouched for the other smokes.
///
/// ## Red-run reverts (TWO, and BOTH must be run)
///
/// They isolate the two halves of the round trip and must produce DISTINGUISHABLE
/// failures — which is exactly why the scenario freezes run 1's save.
///
/// | half | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | SAVE side | delete the `notify::position` handler from `pane_view::install_paned_position` | assertion 1 fires FIRST, with `weights == [0.5, 0.5]` in the frozen copy; the two preconditions above it stay green, which is what says the drag DID move the divider and only the write-back is missing | AS PREDICTED, on assertion 1: "run 1 persisted weights [0.5, 0.5], whose gap is 0 — below the 0.15 that only a DRAGGED divider can produce." Both preconditions stayed green. `0 passed; 1 failed` in 38.32s |
/// | RESTORE side | in `install_paned_position`'s `max-position` handler, replace `tree.borrow().divider_fraction(&path, boundary)` with `Some(0.5)` | assertion 1 PASSES (the frozen copy carries the dragged weights) and assertion 2 fires, with run 2's panes back at equal widths | AS PREDICTED, on assertion 2 with assertion 1 GREEN: "run 1 quit with the left pane holding 0.6666666666666666 of the pair (30 and 15 cells) and run 2 restored 0.4888888888888889 (22 and 23 cells) — a change of 0.17777777777777776." `0 passed; 1 failed` in 38.36s |
#[test]
fn dragged_divider_weights_survive_relaunch_under_cage() {
    const SMOKE: &str = "dragged_divider_weights_survive_relaunch_under_cage";
    // Columns run 1 builds, and therefore the denominator the bisection seeds are
    // modelled from.
    const COLUMNS: i64 = 2;
    // How many panes each width readback must describe.
    const EXPECTED_PANES: usize = 2;

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd shared by BOTH launches — relaunching in the project you
    // saved from is the whole scenario.
    let cwd = make_temp_cwd();
    // The right pane is left 15 cells wide, and a lone heading over `~` is too sparse for tesseract;
    // see `DIVIDER_BODY_LINE_COUNT`. The wide left pane needs no body.
    let right_fixture = divider_right_fixture_body();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (DIVIDER_FILE_LEFT, DIVIDER_HEADING_LEFT),
            (DIVIDER_FILE_RIGHT, right_fixture.as_str()),
        ],
    );

    // The smoke's own state dir — the SAME one `cage_command` hands the app as
    // WOLF_STATE_DIR, so the two runs share one `wolf.sqlite`.
    let smoke_state = smoke_state_dir(&cwd);

    // Freezes run 1's save.
    let run1_state_dir = frozen_state_dir(&runtime_dir, "wolf_gnome_divider_run1_state", SMOKE);
    let run1_state_file = run1_state_dir.join(STATE_DB_NAME);

    let w_base = cwd.join(DIVIDER_W0_NAME);
    let w_drag = cwd.join(DIVIDER_W1_NAME);
    let w_restored = cwd.join(DIVIDER_W2_NAME);
    // Outside the cwd: the bisection rewrites it once per probe and the sidebar
    // watches the cwd.
    let probe = runtime_dir.join("wolf_gnome_divider_relaunch_probe.txt");
    let out_png = runtime_dir.join("wolf_gnome_divider_relaunch_smoke.png");
    for path in [&w_base, &w_drag, &w_restored, &probe, &out_png] {
        clear_evidence(path, SMOKE);
    }

    let (seed_lo, seed_hi) = divider_seed_span(1, COLUMNS);

    let status = cage_command(DIVIDER_RELAUNCH_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("CMD_FIFO", cwd.join(POINTER_CMD_FIFO_NAME))
        .env("ACK_FILE", cwd.join(POINTER_ACK_NAME))
        .env("HELPER_ERR", pointer_err_path(&cwd))
        .env("READY_FILE", pointer_ready_path(&cwd))
        .env("PROBE_X", PROBE_X.to_string())
        .env("PROBE_Y", PROBE_Y.to_string())
        .env("BTN_LEFT", POINTER_BUTTON_LEFT)
        .env("PROBE_FILE", &probe)
        .env("DIVIDER_FILE_LEFT", DIVIDER_FILE_LEFT)
        .env("DIVIDER_FILE_RIGHT", DIVIDER_FILE_RIGHT)
        .env("DIV_W0", &w_base)
        .env("DIV_W1", &w_drag)
        .env("DIV_W2", &w_restored)
        .env("D_LO", seed_lo.to_string())
        .env("D_HI", seed_hi.to_string())
        .env("BISECT_STEPS", BISECT_STEPS.to_string())
        .env("RELAUNCH_DRAG_PX", DIVIDER_RELAUNCH_DRAG_PX.to_string())
        .env("DIV_STATE_SRC", &smoke_state)
        .env("DIV_STATE_COPY", &run1_state_dir)
        .status()
        .expect("failed to spawn `cage` for the headless divider relaunch scenario");

    let helper_err = std::fs::read_to_string(pointer_err_path(&cwd))
        .unwrap_or_else(|e| format!("<could not read the helper stderr: {e}>"));
    assert!(
        status.success(),
        "{SMOKE}: the cage divider relaunch scenario exited unsuccessfully ({status}). Read the \
         exit code FIRST; each names a different half of the harness.\n\
         Exit 11: run 1's app died before it was ready for input — a LAUNCH failure.\n\
         Exit 12: `wolf-pointer-inject` never printed its `ready …` line (its own exit code is in \
         the stderr below: 3 = no `zwlr_virtual_pointer_manager_v1`, 4 = no `wl_seat`, 5 = no \
         `wl_output`, 6 = more than one output, 7 = zero extents).\n\
         Exit 13: a helper command was never acked within budget.\n\
         Exit 14: a readback file the captive Neovim was told to write never appeared — a KEYBOARD \
         failure, not a pointer one.\n\
         Exit 15: the helper died mid-scenario.\n\
         Exit 16: run 1 was still alive after `:qa`.\n\
         Exit 17: run 1 exited NON-ZERO — it went away, but not cleanly, which implicates \
         `Coordinator::on_window_closing`.\n\
         Exit 18: run 1 wrote NO wolf.sqlite at all in {}, which is upstream of anything about \
         weights.\n\
         Exit 19: run 2's app died before it was ready for input.\n\
         Exit 21: the bisection seeds ({seed_lo}, {seed_hi}) did not straddle the divider; the \
         scenario's stderr names the four candidate causes in order, and \
         `click_positions_the_cursor_under_cage` fails on its own if the pane origin moved.\n\
         Screenshot expected at {}.\n\
         wolf-pointer-inject stderr was:\n{helper_err}",
        smoke_state.display(),
        out_png.display()
    );

    assert_png_present(SMOKE, &out_png);

    // Extents FIRST, as in every pointer smoke. The drag's end point is bounded by
    // the HI seed plus the drag distance, so these three cover every injected x.
    let extents = read_output_extents(&cwd, SMOKE);
    for (label, x) in [
        ("the bisection LO seed", seed_lo),
        ("the bisection HI seed", seed_hi),
        (
            "the furthest RIGHT the drag can reach",
            seed_hi + DIVIDER_RELAUNCH_DRAG_PX,
        ),
    ] {
        assert_inside_extents(SMOKE, label, (x, i64::from(PROBE_Y)), extents);
    }

    let base = read_pane_widths(&w_base, SMOKE);
    let dragged = read_pane_widths(&w_drag, SMOKE);
    let restored = read_pane_widths(&w_restored, SMOKE);
    for (label, panes, path) in [
        ("run 1's baseline", &base, &w_base),
        ("run 1's post-drag", &dragged, &w_drag),
        ("run 2's restored", &restored, &w_restored),
    ] {
        assert_eq!(
            panes.len(),
            EXPECTED_PANES,
            "{SMOKE}: {label} readback describes {} pane(s), expected {EXPECTED_PANES}: {panes:?} \
             (from {}). One pane in RUN 2 means the split was not restored at all, which is the \
             topology failure `pane_layout_survives_relaunch_under_cage` owns — fix that first, \
             since weights cannot be asserted against a layout that did not come back.",
            panes.len(),
            path.display()
        );
    }

    let base_left = pane_width_for(&base, DIVIDER_FILE_LEFT, &w_base, SMOKE);
    let base_right = pane_width_for(&base, DIVIDER_FILE_RIGHT, &w_base, SMOKE);
    let drag_left = pane_width_for(&dragged, DIVIDER_FILE_LEFT, &w_drag, SMOKE);
    let drag_right = pane_width_for(&dragged, DIVIDER_FILE_RIGHT, &w_drag, SMOKE);
    let run2_left = pane_width_for(&restored, DIVIDER_FILE_LEFT, &w_restored, SMOKE);
    let run2_right = pane_width_for(&restored, DIVIDER_FILE_RIGHT, &w_restored, SMOKE);

    // --- Precondition A: run 1 started from two roughly equal panes ------------
    assert!(
        (base_left - base_right).abs() <= BASELINE_TOLERANCE_CELLS,
        "{SMOKE}: the fresh split gave panes of {base_left} and {base_right} cells, a spread of {} \
         — more than the {BASELINE_TOLERANCE_CELLS} cells of slack a paned needs for its handle \
         and rounding. A fresh `<C-w>v` carries EQUAL weights and no drag, so the two panes are \
         supposed to come out equal to within that handle: `geometry::divider_position` scales by \
         the paned's SPAN, and `geometry::equal_weights_give_equal_widths` is the unit test for \
         it. Read a real spread as a PRODUCT defect, not as a tolerance to widen — scaling by \
         `max_position` instead leaves exactly the end child's minimum between the two panes. It \
         also says the bisection seed, modelled on equal shares, is aimed at the wrong pixels. PNG \
         kept at {}.",
        (base_left - base_right).abs(),
        out_png.display()
    );

    // Precondition B: the drag moved the divider; without it, equal shares could mean a missed press.
    assert!(
        drag_left > base_left + MIN_DRAG_CELLS && drag_right < base_right - MIN_DRAG_CELLS,
        "{SMOKE}: after dragging the divider {DIVIDER_RELAUNCH_DRAG_PX} px to the RIGHT the panes \
         are {drag_left} and {drag_right} cells, from {base_left} and {base_right} — less than the \
         {MIN_DRAG_CELLS} cells that counts as movement. Read this FIRST as: THE PRESS MISSED THE \
         DIVIDER HANDLE, a harness failure rather than a product one (the bisection converged on \
         the wrong pixel, or the handle is narrower than the residual {BISECT_STEPS} halvings \
         leave). Only after ruling that out does it mean `gtk::Paned` stopped moving on a drag. \
         Nothing below can be interpreted while this is red: an equal-share weight on disk would \
         then be perfectly correct. PNG kept at {}.",
        out_png.display()
    );

    // --- Assertion 1: RUN 1's saved weights are NOT equal shares ---------------
    let saved = frozen_window_state(&run1_state_dir, SMOKE, "run 1's save");
    let window = saved.windows.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's window state carries NO windows at all, so nothing was persisted. \
             File: {}",
            run1_state_file.display()
        )
    });
    let tab = window.tabs.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's persisted window carries NO tabs. `state_seam::window_entry` \
             contributes no window without a tab. File: {}",
            run1_state_file.display()
        )
    });
    match &tab.content.root {
        Some(Node::Layout(Layout {
            orientation,
            weights,
            children,
        })) => {
            assert_eq!(
                *orientation,
                PaneOrientation::Vertical,
                "{SMOKE}: run 1 persisted a {orientation:?} split for a `<C-w>v`, which divides \
                 WIDTH and must persist as Vertical. File: {}",
                run1_state_file.display()
            );
            let tails: Vec<Option<&str>> = children.iter().map(leaf_tail).collect();
            assert_eq!(
                tails,
                vec![Some(DIVIDER_FILE_LEFT), Some(DIVIDER_FILE_RIGHT)],
                "{SMOKE}: run 1 persisted panes naming {tails:?}, not the two fixture files in \
                 left-to-right order. The weights below are parallel to THIS child order, so they \
                 cannot be read at all until it is right. A `None` entry is a nested layout where a \
                 pane belongs; the comparison is on the file NAME rather than the whole path \
                 because /tmp may resolve through a symlink. File: {}",
                run1_state_file.display()
            );
            assert_eq!(
                weights.len(),
                children.len(),
                "{SMOKE}: run 1 persisted {} weights for {} children. File: {}",
                weights.len(),
                children.len(),
                run1_state_file.display()
            );

            let sum = weights[0] + weights[1];
            assert!(
                (sum - 1.0).abs() < WEIGHT_SUM_EPS,
                "{SMOKE}: run 1's persisted weights {weights:?} sum to {sum}, not 1. \
                 `set_divider_fraction` renormalizes the whole split after every drag precisely so \
                 an unbounded number of them cannot drift the sum. File: {}",
                run1_state_file.display()
            );
            assert!(
                weights[0] - weights[1] >= MIN_WEIGHT_GAP,
                "{SMOKE}: run 1 persisted weights {weights:?}, whose gap is {} — below the \
                 {MIN_WEIGHT_GAP} that only a DRAGGED divider can produce. **`[0.5, 0.5]` HERE IS \
                 THE WHOLE POINT OF THIS SMOKE**: it means the drag never reached the layout's \
                 weights, so `layout_snapshot` serialized the equal shares `split` left behind. \
                 The precondition above already proved the divider DID move on screen \
                 ({base_left}/{base_right} cells before, {drag_left}/{drag_right} after), so this \
                 is the write-back — `install_paned_position`'s `notify::position` handler and \
                 `PaneTree::set_divider_fraction` — and not the gesture. The pure twin is \
                 `pane_layout::tests::encode_carries_a_dragged_weight_vector`. File: {}",
                weights[0] - weights[1],
                run1_state_file.display()
            );
        }
        other => panic!(
            "{SMOKE}: run 1 persisted `root: {other:?}` for two side-by-side panes, not a \
             two-child Vertical layout. A `None` means the tab was saved as an empty tab, and a \
             lone pane means the split never reached the pane tree — either way this is the \
             topology failure `pane_layout_survives_relaunch_under_cage` owns, not a weight \
             failure. File: {}",
            run1_state_file.display()
        ),
    }

    // --- Assertion 2: run 2 came back at the SAME ratio ------------------------
    let ratio1 = head_width_ratio(drag_left, drag_right, SMOKE, "run 1's post-drag panes");
    let ratio2 = head_width_ratio(run2_left, run2_right, SMOKE, "run 2's restored panes");
    assert!(
        (ratio2 - ratio1).abs() <= RATIO_TOLERANCE,
        "{SMOKE}: run 1 quit with the left pane holding {ratio1} of the pair ({drag_left} and \
         {drag_right} cells) and run 2 restored {ratio2} ({run2_left} and {run2_right} cells) — a \
         change of {}, beyond the {RATIO_TOLERANCE} cell quantization can explain. Assertion 1 \
         above already proved the right weights are ON DISK, so with that green this is the RESTORE \
         side: `pane_layout::decode` dropping the weights, or \
         `install_paned_position` re-deriving its position from something other than the live \
         tree's `divider_fraction`. PNG kept at {}.",
        (ratio2 - ratio1).abs(),
        out_png.display()
    );
    assert!(
        ratio2 >= 0.5 + MIN_RATIO_OFFSET,
        "{SMOKE}: the restored ratio is {ratio2}, less than {MIN_RATIO_OFFSET} clear of dead \
         centre — so the clause above (`the ratio is unchanged`) is VACUOUS: an equal split \
         satisfies it whenever the drag never took effect. This clause is what keeps the assertion \
         honest. It fires either because the drag was too small to be distinguishable from centre \
         (a FIXTURE problem — raise DIVIDER_RELAUNCH_DRAG_PX, never lower MIN_RATIO_OFFSET) or \
         because the restored panes really did come back equal. Run 2's panes were {run2_left} and \
         {run2_right} cells. PNG kept at {}.",
        out_png.display()
    );

    // Assertion 3: each term is accepted from either OCR reading, checked separately, since a joined
    // string could pair one reading's tail with the other's head. See `ocr_uniform_block_to_lowercase`.
    let ocr_sparse = ocr_to_lowercase(&out_png);
    let ocr_block = ocr_uniform_block_to_lowercase(&out_png);
    for (term, const_name, side) in [
        (DIVIDER_OCR_LEFT, "DIVIDER_OCR_LEFT", "left"),
        (DIVIDER_OCR_RIGHT, "DIVIDER_OCR_RIGHT", "right"),
    ] {
        assert!(
            ocr_sparse.contains(term) || ocr_block.contains(term),
            "{SMOKE}: NEITHER OCR reading contained {const_name} = {term:?} (the {side} pane's \
             heading). The capture is read twice — `--psm 11` (sparse text, the mode every other \
             smoke uses) and `--psm 6` (one uniform block, this smoke's second reader) — and the \
             term is accepted from either, so this is a miss in BOTH. READ THE TWO DUMPS BELOW FOR \
             A MANGLED NEAR-MISS of the term rather than for its absence: every cause measured on \
             this page corrupts the heading instead of removing it, and the two readings \
             disagreeing about ONE GLYPH is the shape to expect. The narrow RIGHT pane is the one \
             to suspect either way.\n\
             FIRST, GLYPH CLASSIFICATION AT THE NARROW PANE'S COLUMN WIDTH. This is the live \
             hazard and it is what the second reader answers: at 15 cells `--psm 11` read \
             `# DIVIDERBkAVO` — the `R` of `BRAVO` as a `k` — on pixels where `--psm 6` read \
             `# DIVIDERBRAVO` correctly, deterministically, across two independently rendered \
             screenshots. If BOTH modes mangle it now, the levers are a THIRD segmentation mode \
             (`--psm 4`, columns of variable size) or a larger rendered size; NEVER a weaker \
             assertion. Do not relax this to a prefix, a fuzzy match, an edit distance or one of \
             the two terms.\n\
             SECOND, SOFT-WRAPPING — the one hazard a fixture still owns. \
             `runtime/nvim/init.lua` sets `wrap`/`linebreak`, so a pane narrower than the \
             14-character heading splits it into two fragments and no reading of the page can \
             contain the term. That IS what lowering DIVIDER_RELAUNCH_DRAG_PX fixes. Run 2's panes \
             were {run2_left} and {run2_right} cells, against a heading that needs 14.\n\
             THIRD, and only once those are ruled out: a genuinely unbound or unpainted pane. The \
             width readback above already showed Neovim has both windows, so this is the LAST \
             reading of a red assertion 3, not the first.\n\
             ONE further cause is ANSWERED BY FIXTURE, and the retained PNG is where to confirm \
             it is still answered rather than re-derive it: the `~` end-of-buffer column \
             (DIVIDER_BODY_LINE_COUNT's body lines fill the pane past winheight, so no marker is \
             drawn). Do NOT lower DIVIDER_RELAUNCH_DRAG_PX for it: it moves the column nowhere, \
             and it narrows the weight gap assertion 1 rests on. DIVIDER_RELAUNCH_SCENARIO's \
             closing `<C-w>h` keeps the active-pane statusbar emphasis and Neovim's block cursor \
             in the WIDE pane, so neither is ink in the sparse region this pane is scaled from.\n\
             PNG kept at {} for inspection.\n\
             `--psm 11` output was:\n\
             {ocr_sparse}\n\
             `--psm 6` output was:\n\
             {ocr_block}",
            out_png.display()
        );
    }
}

/// Runs `tesseract <png> stdout --psm 6` and returns its lowercased text output.
///
/// A SECOND reader, used by
/// [`dragged_divider_weights_survive_relaunch_under_cage`] and by nothing else.
/// Every other smoke reads through [`ocr_to_lowercase`] at `--psm 11`, which this
/// leaves entirely untouched: those assertions are calibrated against that mode's
/// output on their own pages, and re-segmenting them is not what this reader is for.
///
/// ## Why this smoke's page needs a different segmentation mode
///
/// `--psm 11` is "sparse text": it locates text in no particular order and estimates
/// a scale per region it decides is sparse. That is the right assumption for the
/// pages the other smokes capture, and the WRONG one for this one. After the drag,
/// this smoke's page is two columns of editor text, the narrow one being a single
/// heading over a solid block of body lines — a uniform block of text at one scale,
/// which is precisely what `--psm 6` ("a single uniform block") assumes. The 15-cell
/// column is too narrow for `--psm 11`'s per-region scale estimate to resolve, and
/// the failure it produces is not a missing word but a misclassified glyph.
///
/// MEASURED, both modes over the same retained PNG:
///
/// | mode | verbatim output |
/// | --- | --- |
/// | `tesseract PNG stdout --psm 11` | `# DIVIDERBkAVO` — the `R` of `BRAVO` read as a `k`; `DIVIDERALFA` in the wide pane correct |
/// | `tesseract PNG stdout --psm 6` | `A 7 DIVIDERALFA # DIVIDERBRAVO` — BOTH asserted terms correct |
///
/// Deterministic, not flaky: two independently rendered screenshots produced
/// byte-identical OCR text in both modes. `DIVIDERALFA` reads correctly under both,
/// so what is being worked around is a property of the narrow COLUMN, not of the
/// font, the theme or the term.
///
/// ## Red-run revert (one line)
///
/// This reader is LOAD-BEARING, not belt-and-braces, and the revert below is the
/// measurement that settles it — meet it here rather than re-deriving it, because
/// the deletion it argues against is the one that turns the smoke red.
///
/// | assertion | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | assertion 3 of [`dragged_divider_weights_survive_relaunch_under_cage`] | drop this reader from it: `ocr_sparse.contains(term)` in place of `ocr_sparse.contains(term) || ocr_block.contains(term)` | the loop's [`DIVIDER_OCR_RIGHT`] iteration fires on the narrow pane's heading, while its [`DIVIDER_OCR_LEFT`] iteration stays green | AS PREDICTED, and against a FRESH render under `cage` rather than the retained PNG: `thread 'dragged_divider_weights_survive_relaunch_under_cage' (2140204) panicked at …:` / `dragged_divider_weights_survive_relaunch_under_cage: NEITHER OCR reading contained DIVIDER_OCR_RIGHT = "dividerbravo" (the right pane's heading).`, then `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 28 filtered out; finished in 39.92s`. It fires on `dividerbravo` ONLY: the same run's `divideralfa` iteration passes, so `--psm 11` resolves the wide pane's heading and what the revert exposes is the narrow COLUMN rather than the sparse reader as such. Reproducing on a render of its own is also what says the misread in the table above is a property of the settled arrangement, not an artifact of one retained capture. |
///
/// ## What the wider mode does not open up
///
/// `--psm 6` reads MORE of the page than `--psm 11` does — the header-bar title and
/// every tree-sidebar row included — so the premise that only the right pane's
/// heading can supply [`DIVIDER_OCR_RIGHT`] has to hold against the whole page, not
/// just against the editor grid. It does, and mechanically rather than by
/// convention: `sidebar_filler_names_cannot_satisfy_any_smoke_assertion` covers the
/// filler names, `temp_cwd_name_cannot_satisfy_any_smoke_assertion` the two path
/// components the title bar draws, and
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` both
/// `RELAUNCH_FIXTURE_NAMES` — which is where `dvright.md`, deliberately unlike
/// `dividerbravo`, is registered — and every body line
/// [`divider_right_fixture_body`] writes. So no sidebar row, no `:e` echo of a
/// filename and no title segment can satisfy the assertion however much of the page
/// is read.
///
/// ## Why it duplicates [`ocr_to_lowercase`] instead of parameterizing it
///
/// So that the reader every OTHER OCR-asserting smoke depends on stays a fixed,
/// unparameterized command. A mode argument is a value a later change can pass
/// wrongly from a call site that looks unrelated to OCR, and the symptom would be a
/// distant smoke failing on a term it renders perfectly. This binary already
/// duplicates for that class of reason — see [`DIVIDER_RELAUNCH_SCENARIO`] on the
/// pointer plumbing.
///
/// Panics with the captured stderr if tesseract fails to run or exits non-zero.
fn ocr_uniform_block_to_lowercase(png: &Path) -> String {
    let output = Command::new("tesseract")
        .arg(png)
        .arg("stdout")
        .arg("--psm")
        .arg("6")
        .output()
        .expect("failed to spawn `tesseract` for OCR in --psm 6");
    assert!(
        output.status.success(),
        "tesseract failed on {} in --psm 6 ({}): {}. `--psm 11` is the mode every other smoke \
         uses and it is unaffected by this; a failure HERE is tesseract rejecting the mode \
         argument or the image, not a rendering problem.",
        png.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_lowercase()
}
