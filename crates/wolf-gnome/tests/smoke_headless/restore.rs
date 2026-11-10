//! Session-restore smokes: two launches in one cage, the second reading what the first
//! saved, for the pane layout, a lone file, tabs, two windows and dragged divider weights.

use super::pointer::{
    divider_seed_span, head_width_ratio, BASELINE_TOLERANCE_CELLS, BISECT_STEPS, MIN_DRAG_CELLS,
    MIN_RATIO_OFFSET, RATIO_TOLERANCE,
};
use super::recovery::with_nvim_pid_helper;
use super::*;

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
# it gathers this window's subtree, commits the tree and blocks on
# `StateStore::flush`, so state.json is on disk before the process goes.
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
    echo "  StateStore::flush blocked past its timeout." >&2
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
/// Between the two runs it copies the state file run 1 wrote to
/// `$SOLO_STATE_COPY`, so the Rust side can assert on RUN 1'S SAVE rather than on
/// whatever run 2 later wrote over it. That separation is what keeps the two
/// red-run reverts distinguishable: with the SAVE-side arity guard reintroduced
/// the copy carries `layout: null`, while with the RESTORE-side arity guard
/// reintroduced the copy is correct and only the readback and the OCR go red.
///
/// See the exit-code table above [`RELAUNCH_RESTORE_SETTLE`]; this scenario adds
/// `16` for "run 1 wrote no state.json at all".
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

# The whole point: a single unsplit pane holding a single file. Under the old
# ARITY guard this shape persisted its cwd and NOTHING else.
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
# Run 2 re-snapshots on every render wake, so by the time the Rust side could read
# the live file it would hold RUN 2's tree. Copying here is what lets the two
# red-run reverts produce DISTINGUISHABLE failures.
if [ ! -s "$SOLO_STATE_SRC" ]; then
    echo "wolf-gnome single-pane smoke: run 1 wrote no state.json at $SOLO_STATE_SRC." >&2
    echo "  Nothing at all was persisted, so this is not the arity guard: suspect" >&2
    echo "  open_state_store() resolving no directory, Coordinator::on_window_closing" >&2
    echo "  never running, or WOLF_STATE_DIR not reaching the app." >&2
    exit 16
fi
cp "$SOLO_STATE_SRC" "$SOLO_STATE_COPY"

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

/// The file opened in TAB 0 — the tab the launch selects, and the only one whose
/// gather is certain to run.
pub(crate) const TABS_RESTORE_FILE_ONE: &str = "talfa.md";

/// Heading inside [`TABS_RESTORE_FILE_ONE`]. Nothing reads it: this smoke takes no
/// capture, and the fixture bodies exist only so the `:e` opens a real file.
const TABS_RESTORE_HEADING_ONE: &str = "# TABKEEPONE";

/// The file opened in TAB 1 — **the deterministically never-selected tab**, and the
/// whole subject of assertion 3.
///
/// After materialization Neovim is current on the LAST tabpage created until the
/// queued `set_current_tabpage` for tab 0 lands, so the last tab may or may not get
/// one refresh; only a MIDDLE tab is guaranteed never to be Neovim-current at launch.
/// This name therefore reaches run 2's `state.json` only if the gather resolved the
/// leaf from the window-keyed snapshot maps, which are global across tabpages, rather
/// than through the current tabpage's `Snapshot::windows`. That is why the smoke opens
/// three tabs rather than two, and
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` asserts the count so a
/// later trim cannot quietly take the guarantee away.
pub(crate) const TABS_RESTORE_FILE_TWO: &str = "tbravo.md";

/// Heading inside [`TABS_RESTORE_FILE_TWO`]. See [`TABS_RESTORE_HEADING_ONE`].
const TABS_RESTORE_HEADING_TWO: &str = "# TABKEEPTWO";

/// The file opened in TAB 2 — the last tab created, and the one Neovim is current on
/// when the launch finishes materializing.
pub(crate) const TABS_RESTORE_FILE_THREE: &str = "tcharlie.md";

/// Heading inside [`TABS_RESTORE_FILE_THREE`]. See [`TABS_RESTORE_HEADING_ONE`].
const TABS_RESTORE_HEADING_THREE: &str = "# TABKEEPTHREE";

/// The three fixtures in TAB ORDER — the expectation both the readback and the two
/// frozen `state.json` files are compared against.
///
/// Built from the three consts rather than retyped, so the smoke's expectation cannot
/// drift from the files it writes.
pub(crate) const TABS_RESTORE_FILES: [&str; 3] = [
    TABS_RESTORE_FILE_ONE,
    TABS_RESTORE_FILE_TWO,
    TABS_RESTORE_FILE_THREE,
];

/// Name of the file RUN 1's Neovim writes its own tabpage COUNT into.
///
/// A precondition rather than an observable: without it, a `state.json` holding one
/// tab cannot be told apart from a run in which Ctrl+Shift+T never opened a tab at
/// all, and the smoke would report a persistence bug for a keyboard failure.
pub(crate) const TABS_RESTORE_RUN1_READBACK_NAME: &str = "tabs_run1.txt";

/// Name of the file RUN 2's restored Neovim writes its per-tabpage buffer names into,
/// one line per tabpage in tabpage order.
pub(crate) const TABS_RESTORE_READBACK_NAME: &str = "tabs_run2.txt";

/// The two-launch THREE-TAB RELAUNCH scenario, run as one `sh -c` string inside ONE
/// `cage`. Structurally [`RESTORE_SOLO_SCENARIO`] with two `Ctrl+Shift+T` chords and
/// a second frozen state file.
///
/// **Two files are frozen, and that is what keeps three reds distinguishable.**
/// `$TABS_STATE_COPY_1` is run 1's save, taken before run 2 can write over it, so the
/// SAVE side is asserted on its own; `$TABS_STATE_COPY_2` is run 2's, taken after a
/// quit in which tabs 1 and 2 were never selected, so what a whole launch-restore-quit
/// round trip leaves behind is asserted on its own. Without the split, a save side
/// that dropped tabs and a second run that lost them would produce the same one
/// observable.
///
/// **Run 2 selects nothing.** No chord, no click, no `:tabnext` — the only thing
/// typed is the readback. That is the whole point of assertion 3: tab 1 is never
/// Neovim's current tabpage for a single wake of that run, so its file reaches run 2's
/// save only through a gather that resolved the leaf by the pane's own Neovim window
/// handle.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | run 1's app died before it was ready for input |
/// | `12` | run 1 was still alive after `:qa` |
/// | `13` | run 1 exited NON-ZERO — it went away, but not cleanly |
/// | `14` | run 2's app died before it was ready for input |
/// | `15` | run 1's tabpage-count readback file never appeared |
/// | `16` | run 2 was still alive after `:qa` |
/// | `17` | run 2 exited NON-ZERO |
/// | `21` | run 1 did not reach THREE tabpages, so Ctrl+Shift+T failed and nothing below is about persistence |
/// | `22` | run 1 wrote no `state.json` at all |
/// | `23` | run 2 wrote no `state.json` at all |
/// | `24` | run 2's readback never got ANY content, across every re-issue — the `:call` keystrokes never reached Neovim |
///
/// **A run-2 readback that is present but SHORT is deliberately not an exit code.**
/// The materialization budget is spent by re-issuing the `writefile` (see
/// `READBACK_TRIES`), and a readback that has settled at one line IS the restore
/// side's red — so the scenario leaves it standing and lets the Rust side's assertion
/// 2 name it, which is a far better diagnosis than any code here could carry. Exit 24
/// covers only the case where nothing was read back at all, which cannot be a
/// persistence defect.
const RESTORE_TABS_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
QUIT_TRIES=75
FILE_TRIES=100

# How long to wait after each Ctrl+Shift+T. The adw::TabBar ANIMATES its height in
# over roughly a second, and the chord itself issues a BLOCKING create_tabpage RPC
# before the page is appended and selected. Twice the measured animation, the
# native-tabs smoke's constant. A timing knob only: every observable below is polled
# for or checked with its own exit code.
TABS_BAR_SETTLE=2.0

# How many times RUN 2 re-issues its readback while waiting for the restored tabs to
# exist. `wait_ready` already holds until no restore owes a page, so the first try
# normally answers; the retries are the backstop for a readiness that released too
# early, spent by ASKING AGAIN rather than by sleeping. At $WAIT_INTERVAL plus two
# wtype round trips per try this is well over ten seconds of retrying, and it is only
# ever paid in full on a run that is already failing.
READBACK_TRIES=60

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
    echo "wolf-gnome three-tab smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
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

# Tab 2: the last tab created.
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
wtype -- ":e $TABS_FILE_THREE"
wtype -k Return
sleep "$SETTLE"

# PRECONDITION, before anything is saved: the run really did reach three tabpages.
# Without this, a one-tab state.json cannot be told apart from a run in which the
# chord never opened a tab, and the Rust side would report a persistence bug for a
# keyboard failure.
wtype -- ":call writefile([printf('%d', len(gettabinfo()))], '$TABS_RUN1_READBACK')"
wtype -k Return
wait_file "$TABS_RUN1_READBACK" || exit 15
run1_tabs=$(cat "$TABS_RUN1_READBACK")
echo "wolf-gnome three-tab smoke: run 1 tabpages=$run1_tabs" >&2
if [ "$run1_tabs" != "3" ]; then
    echo "wolf-gnome three-tab smoke: run 1 has $run1_tabs tabpage(s), not 3." >&2
    echo "  Ctrl+Shift+T did not open a tab, so nothing below would be about PERSISTENCE:" >&2
    echo "  either win.new-tab is not installed (Command::NewTab demoted back to Deferred, in" >&2
    echo "  which case the chord fell through to Neovim as <C-t>), or the liveness gate refused," >&2
    echo "  or create_tabpage answered 0. The native-tabs smoke covers that path directly." >&2
    exit 21
fi

# Quit. `Coordinator::on_window_closing` gathers this window's subtree -- now once per
# TAB -- commits the tree and blocks on `StateStore::flush`, so state.json is on disk
# before the process goes.
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
    echo "  It went away, but NOT cleanly. Three tabs close in sequence here, so a borrow" >&2
    echo "  discipline that held at one tab is the first suspect: the close path gathers every" >&2
    echo "  tab's subtree while the panes and the core are still alive." >&2
    exit 13
fi
app_pid=

# --- Freeze RUN 1'S SAVE before run 2 can write over it --------------------
if [ ! -s "$TABS_STATE_SRC" ]; then
    echo "wolf-gnome three-tab smoke: run 1 wrote no state.json at $TABS_STATE_SRC." >&2
    echo "  Nothing at all was persisted, so this is not the tab gather: suspect" >&2
    echo "  open_state_store() resolving no directory, Coordinator::on_window_closing never" >&2
    echo "  running, or WOLF_STATE_DIR not reaching the app." >&2
    exit 22
fi
cp "$TABS_STATE_SRC" "$TABS_STATE_COPY_1"

# =========================================================================
# RUN 2 -- relaunch in the SAME cwd with the SAME state dir, read back, and
# quit WITHOUT EVER SELECTING another tab.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 14 "wolf-gnome three-tab smoke"

# THE load-bearing observable: one line per TABPAGE, in tabpage order, holding that
# tabpage's buffer tail names. `:t` sidesteps any question of how /tmp resolves.
#
# The mapping uses LAMBDAS rather than the usual `'…v:val…'` string expressions so
# that no double quote appears anywhere in the typed command: the whole line is
# already inside a double-quoted shell word, and nesting quotes through
# sh -> wtype -> the Neovim cmdline is exactly where these scenarios have
# historically broken.
#
# POLLED and RE-ISSUED, rather than written once after a fixed settle. `writefile`
# truncates, so each re-issue replaces the whole file with the answer as of now and
# the loop simply stops once three tabpages exist. Writing once would rest the whole
# observable on `wait_ready`'s restore conjunct: if that ever released before
# materialization ran, the Rust side would read one line and assertion 2 would report
# a deleted queue_restore for what is only a readiness defect.
#
# Exhausting the budget is deliberately NOT fatal. A readback that has settled at one
# line across every re-issue IS assertion 2's red, and assertion 2's message names
# what is in the file and what each shape means; replacing that with an exit code
# here would throw the diagnosis away. Only "nothing was read back at all" aborts,
# with exit 24, because that cannot be a persistence defect.
rb=0
tab_lines=0
while [ "$rb" -lt "$READBACK_TRIES" ]; do
    # Escape first: a half-typed cmdline left behind by a lost keystroke would
    # otherwise swallow the whole :call and the file would never change again.
    wtype -k Escape
    wtype -- ":call writefile(map(gettabinfo(), {_, t -> join(map(tabpagebuflist(t.tabnr), {_, b -> fnamemodify(bufname(b), ':t')}), ' ')}), '$TABS_READBACK')"
    wtype -k Return
    # Counted BEFORE the check, so the number reported below is re-issues ISSUED
    # rather than re-issues that failed -- a green run reports 1, not 0.
    rb=$((rb + 1))
    sleep "$WAIT_INTERVAL"
    if [ -s "$TABS_READBACK" ]; then
        tab_lines=$(wc -l < "$TABS_READBACK" | tr -d ' ')
        if [ "$tab_lines" -ge 3 ]; then
            break
        fi
    fi
done

if [ ! -s "$TABS_READBACK" ]; then
    echo "wolf-gnome three-tab smoke: run 2's readback $TABS_READBACK never got ANY content," >&2
    echo "  across $READBACK_TRIES re-issues. Nothing was read back, so this is not the restore:" >&2
    echo "  the :call keystrokes never reached Neovim (wtype focus, or an nvim wedged from the" >&2
    echo "  first wake), and the Rust side has no readback to assert on at all." >&2
    exit 24
fi
echo "wolf-gnome three-tab smoke: run 2 readback lines=$tab_lines after $rb re-issue(s)" >&2
if [ "$tab_lines" -lt 3 ]; then
    echo "wolf-gnome three-tab smoke: run 2's readback settled at $tab_lines line(s), not 3." >&2
    echo "  NOT aborting, deliberately. The file HAS content, so assertion 2 on the Rust side" >&2
    echo "  can name exactly what is in it; and a readback that stayed short across" >&2
    echo "  $READBACK_TRIES re-issues is a restore that never happened rather than one that" >&2
    echo "  had not happened YET, which is the distinction this loop exists to make." >&2
fi

# NOTHING selects another tab: no chord, no click, no :tabnext. Tabs 1 and 2 keep
# whatever the restore gave them, which is the whole subject of the third assertion.
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
    echo "  It went away, but NOT cleanly. Run 2 is the one that RESTORED tabs, so suspect the" >&2
    echo "  materialization: a panic inside TabsInner::materialize_restore, or an entries" >&2
    echo "  borrow held across open_tab / create_tabpage / set_selected_page (the last fires" >&2
    echo "  on_selected_changed synchronously, which borrows entries and the core again)." >&2
    exit 17
fi
app_pid=

if [ ! -s "$TABS_STATE_SRC" ]; then
    echo "wolf-gnome three-tab smoke: run 2 wrote no state.json at $TABS_STATE_SRC." >&2
    echo "  Run 1's copy is already frozen, so the save side is provable without this file --" >&2
    echo "  what is missing is assertion 3's evidence. Suspect the same causes as exit 22." >&2
    exit 23
fi
cp "$TABS_STATE_SRC" "$TABS_STATE_COPY_2"

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
/// Between the two runs it freezes run 1's `state.json` into `$DIV_STATE_COPY`,
/// following the [`RESTORE_SOLO_SCENARIO`] precedent: run 2 re-snapshots on every
/// render wake, so the live file would carry RUN 2's tree by the time the Rust side
/// could read it — and the save-side and restore-side red runs would then be
/// indistinguishable.
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
/// | `18` | run 1 wrote no `state.json` at all |
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
# in the close-request handler and blocks on `StateStore::flush`, so state.json is on
# disk before the process goes.
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
if [ ! -s "$DIV_STATE_SRC" ]; then
    echo "wolf-gnome divider relaunch smoke: run 1 wrote no state.json at $DIV_STATE_SRC." >&2
    echo "  Nothing at all was persisted, so this is not the weight write-back: suspect" >&2
    echo "  open_state_store() resolving no directory, Coordinator::on_window_closing never" >&2
    echo "  running, or WOLF_STATE_DIR not reaching the app." >&2
    exit 18
fi
cp "$DIV_STATE_SRC" "$DIV_STATE_COPY"

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
# and Neovim's block cursor into the WIDE pane. `notes/wolf-gnome-decisions.md`
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

/// The file window A opens, in the temp cwd — the first project's.
pub(crate) const TWOWIN_FIRST_FILE: &str = "wfirst.md";

/// Heading written into [`TWOWIN_FIRST_FILE`]. Nothing reads it: this smoke takes
/// no capture. It exists so the file is not zero-length and so the pane has
/// something to draw.
const TWOWIN_FIRST_HEADING: &str = "# WINALFA";

/// The subdirectory of the temp cwd that is window B's SEPARATE project.
///
/// It has to be a real directory that is neither the temp cwd (or the two windows
/// share a project, and with it the durable swap directory) nor the app's `$HOME`
/// (which `workspace::is_usable_project` refuses, so the restored plan would fall
/// through to the empty state and carry no layout). A CHILD of the temp cwd
/// satisfies both by construction; a sibling would be a child of `$HOME` and would
/// satisfy them too, so the choice is the tidier of two correct ones rather than a
/// constraint.
///
/// Its basename is what the Rust side compares `windows[1].tabs[0].cwd`'s final
/// component against, and what the scenario's exit-24 pair check keys on — which is
/// why the fixture guard asserts it differs from [`TEMP_CWD_BASENAME`].
pub(crate) const TWOWIN_SUBDIR: &str = "wsub";

/// The file window B opens, as a path RELATIVE TO THE TEMP CWD — which is what
/// [`write_relaunch_fixtures`] joins onto it and what
/// `RELAUNCH_FIXTURE_NAMES` registers.
///
/// It carries the [`TWOWIN_SUBDIR`] prefix for the reason `LINK_SOURCE_FILE`
/// carries `LINK_FIXTURE_DIR`: registering the PREFIXED path puts the directory
/// name inside the generic substring loop too, instead of buying this smoke a
/// directory exemption block of the kind the title and typed-`:cd` smokes carry.
pub(crate) const TWOWIN_SECOND_FILE: &str = "wsub/wsecond.md";

/// The bare filename of [`TWOWIN_SECOND_FILE`] — what the scenario types at `:e`
/// once window B has `:cd`'d into [`TWOWIN_SUBDIR`], and what the Rust side
/// compares the saved leaf's file NAME against.
///
/// A const rather than a `rsplit` of the path above, because both are handed to the
/// scenario as environment variables and each is read at a different step;
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` asserts the two
/// agree, so they cannot drift into writing the fixture in one place and opening it
/// in another.
pub(crate) const TWOWIN_SECOND_NAME: &str = "wsecond.md";

/// Heading written into [`TWOWIN_SECOND_FILE`]. See [`TWOWIN_FIRST_HEADING`].
const TWOWIN_SECOND_HEADING: &str = "# WINBRAVO";

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
/// A has [`TWOWIN_FIRST_FILE`] open and a new window has an unnamed buffer whatever
/// directory it was given, so a readback naming the first file says the keystrokes
/// reached the WRONG window and gets an exit code of its own.
pub(crate) const TWOWIN_READBACK_B_NAME: &str = "wtwo_bcwd.txt";

/// Name of the file the FOCUSED restored window's Neovim writes after the
/// relaunch: two lines, `getcwd()` then the tail of `bufname('%')`.
///
/// Two lines rather than one joined string, so neither value has to be split back
/// out of a separator a path could contain.
pub(crate) const TWOWIN_READBACK_R2_NAME: &str = "wtwo_run2.txt";

/// How many lines carrying `"tabs"` the frozen `state.json` must hold — one per
/// persisted window, because each `WindowState` has exactly one `tabs` key however
/// many tabs it holds, no other type has a key of that name, and `wolf_core`'s
/// writer serializes with `serde_json::to_string_pretty`, which puts every key on
/// its own line.
///
/// **This is a guard on the WRITE side that fires BEFORE run 2 launches**, and that
/// is its whole reason for existing: without it, a build that committed only one
/// window's subtree would sail past this point and fail later at the restore poll,
/// where "the tree was written wrong" and "the tree was read wrong" are
/// indistinguishable. The same masking this binary records for the first
/// project-search smoke's assertion 6, which every falsifier reaching it reddens 5 first.
///
/// The count is over LINES, so it rests on the pretty-printer. If it ever reports
/// `1` for a tree the Rust side then reads back as two windows, the formatter is
/// what changed, not the coordinator — the scenario's message says so, and the
/// authoritative two-window assertion is `load_state` on the frozen copy either
/// way.
const TWOWIN_STATE_TABS_LINES: &str = "2";

/// The TWO-WINDOW scenario, run as one `sh -c` string inside ONE `cage`, with
/// [`with_nvim_pid_helper`] prepended for `find_nvim_pid` / `wait_nvim_count`.
///
/// Structurally [`RESTORE_SOLO_SCENARIO`] — two launches, a poll-and-reap of run 1,
/// a frozen `state.json` between them — with three differences that are the
/// subject: a second window is opened from the palette inside run 1, the quit is
/// the `Ctrl+Shift+Q` chord rather than `:qa`, and the evidence is `/proc` plus the
/// file rather than a capture.
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
/// `RELAUNCH_RESTORE_SETTLE`; `21`–`25` are this scenario's own. This scenario
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
/// | `16` | run 1 wrote no `state.json` at all |
/// | `21` | run 1 did not have EXACTLY TWO live `nvim` children after the palette activation |
/// | `22` | the new window INHERITED the invoking window's project directory |
/// | `23` | run 2 did not have EXACTLY TWO live `nvim` children after the restore |
/// | `24` | the focused restored window's `getcwd()` and `bufname('%')` are not one of the two saved pairs |
/// | `25` | run 1's `state.json` does not carry two windows' worth of tabs |
/// | `26` | step 4's readback came from the INVOKING window — a focus failure, not an inheritance |
const TWOWIN_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO. FILE_TRIES is also
# `wait_nvim_count`'s budget, since that function reads it.
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
    echo "wolf-gnome two-window smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# =========================================================================
# RUN 1 -- window A on the project, window B on the subproject, one quit.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome two-window smoke"

# Window A opens the first project's file, so it has a named leaf to persist.
wtype -- ":e $TWOWIN_FIRST_FILE"
wtype -k Return
sleep "$SETTLE"

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

# --- Step 3: TWO windows means TWO Neovims -----------------------------------
#
# The load-bearing observable of the whole smoke, and the one no capture can take.
wait_nvim_count 2 21 "after the palette activated New Window"

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
# keystrokes never reached the new window at all". Window A has TWOWIN_FIRST_FILE
# open; a window opened by New Window has an unnamed buffer whatever cwd it was
# given -- under the inherit defect its plan is a Folder with NO layout, so it
# still answers an Untitled buffer, whatever the ordinal. MEASURED under that
# defect: Untitled 2.md, not Untitled.md. The ordinal is not fixed -- the name
# numbers up from the first free one and the collision check consults the cwd's
# own files and swaps (UNSAVED_BUFFER_NAME documents the rule) -- and nothing
# here keys off it, because the comparison below is against TWOWIN_FIRST_FILE
# alone.
#
# So a readback naming the first file can only have come from window A, and it
# gets its own exit code rather than being reported as an inheritance that did
# not happen.
rm -f "$TWOWIN_B_CWD"
wtype -- ":call writefile([getcwd(), fnamemodify(bufname('%'), ':t')], '$TWOWIN_B_CWD')"
wtype -k Return
wait_file "$TWOWIN_B_CWD" || exit 15
b_cwd=$(sed -n 1p "$TWOWIN_B_CWD")
b_buf=$(sed -n 2p "$TWOWIN_B_CWD")
echo "wolf-gnome two-window smoke: the focused NEW window reports getcwd() = $b_cwd" >&2
echo "  and bufname('%') = $b_buf" >&2

if [ "$b_buf" = "$TWOWIN_FIRST_FILE" ]; then
    echo "wolf-gnome two-window smoke: the step-4 readback names $TWOWIN_FIRST_FILE, which only" >&2
    echo "  the INVOKING window has open -- so these keystrokes reached window A, not the new" >&2
    echo "  window, and nothing below would be about the new window at all." >&2
    echo "  This is a FOCUS failure rather than a coordinator one. Two windows exist (the child" >&2
    echo "  count above already passed), so check whether the second toplevel mapped and took" >&2
    echo "  keyboard focus: cage shows one toplevel at a time and keeps the NEWEST on top, so" >&2
    echo "  the newest is what wtype drives. A window whose core started but whose widget never" >&2
    echo "  presented would leave focus where it was." >&2
    echo "  If this is intermittent, the settle after the child-count poll is the knob -- the" >&2
    echo "  count is satisfied as soon as the nvim process exists, which is earlier than the" >&2
    echo "  window being ready for input." >&2
    exit 26
fi

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
    echo "  A second consequence, not visible here: two windows on one project share the" >&2
    echo "  durable swap directory, so the next step would meet W325." >&2
    exit 22
fi

# --- Step 5: give window B a project of its OWN ------------------------------
#
# A typed `:cd` re-roots the whole window on Neovim's own working directory -- the
# path typed_cd_reroots_the_sidebar_under_cage drives end to end -- and the re-root
# is also what flips this window out of the empty state, which is what makes it
# persist a cwd at all (snapshot_cwd returns "" for an empty-state window however
# far its nvim has wandered). ABSOLUTE, because B's cwd is $HOME at this point and a
# relative path would resolve against the wrong root.
wtype -- ":cd $TWOWIN_SUBDIR_ABS"
wtype -k Return
sleep "$SETTLE"
wtype -- ":e $TWOWIN_SECOND_NAME"
wtype -k Return
sleep "$SETTLE"

# --- Step 6: quit the APPLICATION with Ctrl+Shift+Q --------------------------
#
# The only exercise `app.quit` gets in this binary. Coordinator::quit latches
# shutting_down, refreshes EVERY window's subtree while every core and every pane is
# still alive, commits the tree, blocks on the flush, and only then closes the
# windows -- so state.json holds both sessions before the process goes.
wtype -M ctrl -M shift -k q -m shift -m ctrl

# POLL (never sleep-and-hope) for run 1 to go away, then REAP it and check its EXIT
# STATUS. A process that vanished because the GTK main loop panicked is just as gone
# as one that shut down cleanly, so the status is a separate code.
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
    echo "wolf-gnome two-window smoke: run 1 (pid $app_pid) is still alive after Ctrl+Shift+Q." >&2
    echo "  That chord is the ONLY app.quit exercise in this suite. The first thing to check" >&2
    echo "  is whether the action exists at all: Coordinator::install_app_actions installs the" >&2
    echo "  two app.-namespace rows exactly once per process, and with no app.quit installed" >&2
    echo "  the chord controller finds the action disabled, declines the key, and Neovim gets" >&2
    echo "  a plain <C-Q> -- which does nothing and leaves the app running." >&2
    echo "  The other candidates: Coordinator::quit not closing every window the APPLICATION" >&2
    echo "  holds, or a window's close-request handler refusing." >&2
    exit 12
fi
# `set -e` would abort before the status could be reported, so collect it through
# `||` (which is exempt from `set -e`).
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome two-window smoke: run 1 (pid $app_pid) exited $rc, not 0." >&2
    echo "  It went away, but NOT cleanly. Suspect a panic in Coordinator::on_window_closing" >&2
    echo "  (which gathers, commits and blocks on the flush as the FIRST statement of the" >&2
    echo "  close handler), a core.borrow() held across window::wire_close's" >&2
    echo "  core.borrow_mut().take(), or a StateStore::flush blocking past its timeout." >&2
    echo "  With TWO windows closing in sequence, a borrow discipline that held at N=1 is" >&2
    echo "  exactly what would first fail here." >&2
    exit 13
fi
app_pid=

# --- Step 7: the WRITE side, checked BEFORE run 2 can mask it ----------------
if [ ! -s "$TWOWIN_STATE_SRC" ]; then
    echo "wolf-gnome two-window smoke: run 1 wrote no state.json at $TWOWIN_STATE_SRC." >&2
    echo "  Nothing at all was persisted, so this is upstream of anything about windows:" >&2
    echo "  suspect open_state_store() resolving no directory, Coordinator::quit's commit or" >&2
    echo "  flush never running, or WOLF_STATE_DIR not reaching the app." >&2
    exit 16
fi
# `|| true` because grep exits 1 when it counts zero, which under `set -e` would
# abort the scenario with a status outside this table -- the same trap
# assert_one_restart_attempt documents for `grep -c` on an empty file.
tabs_lines=$(grep -c '"tabs"' "$TWOWIN_STATE_SRC" || true)
if [ "$tabs_lines" -ne "$TWOWIN_STATE_TABS_LINES" ]; then
    echo "wolf-gnome two-window smoke: run 1's state.json carries $tabs_lines line(s)" >&2
    echo "  matching '\"tabs\"', not $TWOWIN_STATE_TABS_LINES -- one per persisted window." >&2
    echo "  ONE means the WRITE side lost a window: Coordinator::assemble walks every live" >&2
    echo "  window's cached subtree, so a truncation there (or a window that contributed" >&2
    echo "  nothing) commits a one-window tree over a two-window session." >&2
    echo "  ZERO means the file holds no window at all -- state_seam::snapshot_state answers" >&2
    echo "  None for an empty contribution set and the coordinator then commits nothing, so" >&2
    echo "  a zero here is a file left over from before that, not a fresh empty commit." >&2
    echo "  This guard exists to fire BEFORE run 2 launches: without it a write-side defect" >&2
    echo "  surfaces as a restore failure and the two become indistinguishable." >&2
    echo "  One more cause worth ruling out, because it is not about windows at all: the" >&2
    echo "  count is over LINES and wolf-core serializes with serde_json::to_string_pretty," >&2
    echo "  so a switch to compact JSON would report 1 for a perfectly good two-window tree." >&2
    echo "  The Rust side's load_state assertion is the authority either way." >&2
    exit 25
fi

# --- Step 8: freeze run 1's save before run 2 can write over it --------------
#
# Run 2 re-snapshots on every render wake, so by the time the Rust side could read
# the live file it would hold RUN 2's tree.
cp "$TWOWIN_STATE_SRC" "$TWOWIN_STATE_COPY"

# =========================================================================
# RUN 2 -- relaunch in the SAME cwd with the SAME state dir, and read back.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 14 "wolf-gnome two-window smoke"

# --- Step 9: BOTH windows came back, each with its own core ------------------
wait_nvim_count 2 23 "after the relaunch restored the saved session"

# --- Step 10: the focused restored window holds a COHERENT session -----------
#
# Only one window can be typed at (cage shows one toplevel and nothing can raise
# another), so this reads whichever window is focused and asserts the PAIR is one of
# the two that were saved: the project directory with the first file, or the
# subdirectory with the second. That is deliberately not a claim about WHICH window
# has focus -- it needs no such measurement -- while still discriminating the two
# failures that matter: `Untitled.md` (the layout was never rebuilt) matches neither
# pair, and a window sitting in one project while showing the OTHER project's file
# is the crossed restore that a plan-per-window mix-up produces.
rm -f "$TWOWIN_R2_READBACK"
wtype -- ":call writefile([getcwd(), fnamemodify(bufname('%'), ':t')], '$TWOWIN_R2_READBACK')"
wtype -k Return
wait_file "$TWOWIN_R2_READBACK" || exit 15
r2_cwd=$(sed -n 1p "$TWOWIN_R2_READBACK")
r2_buf=$(sed -n 2p "$TWOWIN_R2_READBACK")
echo "wolf-gnome two-window smoke: the focused RESTORED window reports getcwd() = $r2_cwd" >&2
echo "  and bufname('%') = $r2_buf" >&2

case "${r2_cwd##*/}" in
    "$TWOWIN_PROJECT_BASE")
        want_buf=$TWOWIN_FIRST_FILE
        ;;
    "$TWOWIN_SUBDIR_BASE")
        want_buf=$TWOWIN_SECOND_NAME
        ;;
    *)
        echo "wolf-gnome two-window smoke: the focused restored window is rooted at $r2_cwd," >&2
        echo "  whose final component is neither $TWOWIN_PROJECT_BASE nor $TWOWIN_SUBDIR_BASE." >&2
        echo "  Both saved windows named one of those, so this window was restored at neither" >&2
        echo "  of the saved working directories: state_seam::restore_plans offered a plan the" >&2
        echo "  saved tree does not contain. A home-directory answer means the plan fell" >&2
        echo "  through to the empty state, i.e. the saved cwd was rejected as unusable." >&2
        exit 24
        ;;
esac
if [ "$r2_buf" != "$want_buf" ]; then
    echo "wolf-gnome two-window smoke: the focused restored window is rooted at $r2_cwd but" >&2
    echo "  shows $r2_buf, where the session saved for that directory holds $want_buf." >&2
    echo "  Untitled.md means the saved layout was never rebuilt for this window: a tab's" >&2
    echo "  open buffer paths ride ONLY inside TabState::layout's leaves, so a plan whose" >&2
    echo "  layout was dropped restores the directory and nothing else. The Rust side's" >&2
    echo "  assertions on the FROZEN copy say whether the layout was saved, which is what" >&2
    echo "  separates a write-side loss from a restore-side one." >&2
    echo "  The OTHER project's file here means the plans were CROSSED: each restored window" >&2
    echo "  was handed the other's layout, which is what offering the process cwd to every" >&2
    echo "  index produces." >&2
    exit 24
fi

# The trap tears run 2 down on the way out.
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
/// `~/.local/state/wolf/state.json` simply does not exist, and "absent before,
/// absent after" is exactly as strong an isolation proof as "unchanged". Taking
/// `metadata().unwrap()` for the BEFORE snapshot would panic there instead.
fn state_file_fingerprint(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

/// Proves the tab's PANE-SPLIT TREE survives a quit and relaunch: three panes in an
/// asymmetric layout, each holding its own file, rebuilt from `state.json`.
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
/// | both | in `window.rs`'s `build`, delete `if let Some(layout) = plan.layout.clone() { panes.request_layout_rebuild(layout); }` — or, equivalently, make `pane_view::consume_pending_layout` `return false;` as its first statement | the readback reads `Untitled.md` instead of the three names, and the OCR carries none of `restorealfa` / `restorebravo` / `restorecharlie` | **AS PREDICTED.** Assertion 1 panicked first: `assertion 'left == right' failed … left: "Untitled.md" right: "rleft.md rmid.md rright.md"`. Because it fires first the OCR assertion is never reached, so the OCR half was confirmed out of band by running `tesseract --psm 11` over the retained PNG: zero hits for each of `restorealfa`, `restorebravo` and `restorecharlie`. Restoring the line returned the smoke to `1 passed`. |
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

    // The state dir makes both runs share one `state.json` and keeps them out of the developer's.
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
         somewhere between `rebuild_layout_spec`, `wolf_rebuild_layout` and `pane_layout::decode`; \
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
/// persistence channel of their own: they ride ONLY inside `TabState::layout`'s
/// leaves. The macOS save side suppressed the layout by ARITY
/// (`guard tree.leaves().count > 1 else { return nil }`), so this exact shape
/// persisted its cwd and `sidebar_visible` and nothing else, and the file was not
/// reopened. The fix — on both sides — is a CONTENT predicate: suppress ONLY an
/// unnamed, non-preview lone leaf, and rebuild ANY saved layout with no arity guard
/// at all.
///
/// ## What it asserts
///
/// 1. **The regression assertion, and it is on the FILE not on pixels.** The
///    `state.json` RUN 1 wrote (frozen by the scenario into `$SOLO_STATE_COPY`
///    before run 2 can write over it) carries
///    `windows[0].tabs[0].layout == Some(Leaf { path, preview: false })` where
///    `path` is non-empty and its file NAME is exactly `szulu.md`. The name rather
///    than the whole path, deliberately: `/tmp` (or `TMPDIR`) may resolve through a
///    symlink on this host, so nvim's reported `abs_path` need not be string-equal
///    to any `PathBuf` this test could build — but an arity guard yields
///    `layout: null`, which this catches perfectly, and an empty-path leaf cannot
///    satisfy it either. No pixel can distinguish "the layout was saved and
///    restored" from "the layout was never saved but the file came back another
///    way", which is why this assertion exists at all.
/// 2. **Load-bearing, and exact:** the restored Neovim's `bufname('%')` tail is
///    exactly `szulu.md`.
/// 3. **Corroboration:** OCR contains `solozulu` and does NOT contain `soloalfa`.
/// 4. **State-dir isolation**, in four parts: this TEST process has no
///    `WOLF_STATE_DIR` of its own (without which the whole check would be
///    self-referential); the smoke's own state dir holds a non-empty `state.json`,
///    proving the app honoured the variable; NEITHER `$HOME`-derived fallback
///    directory ([`HOME_STATE_DIR_FALLBACKS`], under the temp `$HOME`
///    [`cage_command`] gives the app) holds a `state.json`, which is the positive
///    proof that `state_dir()` resolved the variable rather than `$HOME`; and the
///    developer's REAL `state.json` is byte-identical across the run — including
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
/// cannot be satisfied by anything except the save path writing a single-leaf
/// layout.
///
/// ## Red-run reverts (TWO, both one-liners, and BOTH must be run)
///
/// They isolate the two halves of the original bug and must produce
/// DISTINGUISHABLE failures — which is exactly what makes assertion 1 worth having,
/// and why the scenario freezes run 1's `state.json` before run 2 can overwrite it.
///
/// | half | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | SAVE side | in `state_seam::persistable_layout`, replace `if pane_layout::is_empty_leaf(&node) { None } else { Some(node) }` with `if pane_layout::leaf_count(&node) < 2 { None } else { Some(node) }` | assertion 1 fails FIRST and names the arity guard (run 1's copy carries `layout: null`); assertions 2 and 3 also fail | **AS PREDICTED.** Assertion 1 panicked first, at the `layout: None` branch of the match, with the message that begins `run 1 persisted 'layout: None' for a single unsplit pane holding szulu.md`. The frozen copy carried the tab object with `cwd` and `sidebar_visible` and **no `layout` key at all** (serde skips `None`), which is precisely the on-disk shape of the original macOS bug. OCR of the retained run-2 PNG had zero hits for both `solozulu` and `soloalfa`, so assertions 2 and 3 would have failed as well. |
/// | RESTORE side | in `window.rs`'s `build`, wrap the `request_layout_rebuild` call in `if pane_layout::leaf_count(&layout) > 1 { … }` | assertion 1 **passes** (the save side is correct and the frozen copy proves it) while assertions 2 and 3 fail | **AS PREDICTED, and DISTINGUISHABLY.** Assertion 1 PASSED — the frozen copy carried `"layout": { "type": "leaf", "path": "…/szulu.md", "preview": false }` — and the panic moved to assertion 2: `assertion 'left == right' failed … left: "Untitled.md" right: "szulu.md"`. Two different assertions, two different line numbers, and the assertion-2 message's own self-diagnosis ("assertion 1 above already proved it WAS saved correctly … this points squarely at the restore side") named the injected defect correctly. OCR again had zero hits for `solozulu`. |
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
    let real_state_file = real_state_dir.join(STATE_FILE_NAME);
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
    // the app as WOLF_STATE_DIR, so the two runs share one `state.json`.
    let smoke_state = smoke_state_dir(&cwd);
    let smoke_state_file = smoke_state.join(STATE_FILE_NAME);

    let smoke_config = smoke_config_dir(&cwd);
    let smoke_config_file = smoke_config.join(CONFIG_FILE_NAME);

    // Freezes run 1's save before run 2 overwrites it. A directory, because `load_state` takes one;
    // under XDG_RUNTIME_DIR so the sidebar never lists it.
    let run1_state_dir = runtime_dir.join("wolf_gnome_solo_run1_state");
    std::fs::create_dir_all(&run1_state_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the run-1 state snapshot dir {}: {e}",
            run1_state_dir.display()
        )
    });
    let run1_state_file = run1_state_dir.join(STATE_FILE_NAME);

    let readback = cwd.join(SOLO_READBACK_NAME);
    let out_png = runtime_dir.join("wolf_gnome_restore_solo_smoke.png");
    for path in [&readback, &out_png, &run1_state_file] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(RESTORE_SOLO_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("SOLO_FILE", SOLO_FILE)
        .env("SOLO_READBACK", &readback)
        .env("SOLO_STATE_SRC", &smoke_state_file)
        .env("SOLO_STATE_COPY", &run1_state_file)
        .status()
        .expect("failed to spawn `cage` for the headless single-pane relaunch scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage single-pane relaunch scenario exited unsuccessfully ({status}). Exit \
         11: run 1's app died before it was ready for input. Exit 12: run 1 was still alive after \
         `:qa`. Exit 13: run 1 exited NON-ZERO — it went away but not cleanly, which implicates a \
         panic in `Coordinator::on_window_closing` or a core.borrow() held across `wire_close`'s \
         core.borrow_mut().take(). Exit 14: run 2's app died before it was ready for input. Exit 15: \
         the Neovim readback never appeared in run 2. Exit 16: run 1 wrote NO state.json at all at \
         {} — not the arity guard (which still writes the file, with a null layout) but something \
         upstream of it. Screenshot expected at {}.",
        smoke_state_file.display(),
        out_png.display()
    );

    assert_png_present(SMOKE, &out_png);

    // --- Assertion 1: RUN 1's saved layout is a single NAMED leaf --------------
    let saved = load_state(&run1_state_dir).unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::load_state could not parse the frozen copy of run 1's state at \
             {}. `load_state` is silently-ignore, so a None here means the file is absent, \
             unreadable, malformed, or carries an unsupported version — the scenario already \
             proved it was non-empty (it exits 16 otherwise), so suspect the shape.",
            run1_state_file.display()
        )
    });
    let window = saved.windows.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's state.json carries NO windows at all, so nothing was persisted. \
             `snapshot_state` writes exactly one window and one tab, so an empty `windows` means \
             `may_snapshot` refused every snapshot (nvim not Alive, no bound pane, or no reported \
             window) or `set_snapshot` was never called. File: {}",
            run1_state_file.display()
        )
    });
    let tab = window.tabs.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's persisted window carries NO tabs. `snapshot_state` writes exactly \
             one. File: {}",
            run1_state_file.display()
        )
    });
    match tab.layout.as_ref().and_then(leaf_fields) {
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
                "{SMOKE}: run 1 persisted the lone leaf with `preview: true`. wolf-gnome has no \
                 Markdown preview, so `pane_layout::encode` must set the flag to false \
                 unconditionally; a true here would make the restore call `wolf_preview_file` \
                 instead of `:edit`. File: {}",
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
            "{SMOKE}: run 1 persisted `layout: {:?}` for a single unsplit pane holding \
             {SOLO_FILE}, not a single named leaf. **`None` IS THE SINGLE-PANE REGRESSION**: it \
             means the save side gated on ARITY (`leaf_count(..) > 1`) instead of on the CONTENT \
             predicate `pane_layout::is_empty_leaf`. A tab's open buffer paths have no persistence \
             channel of their own — they ride only inside `TabState::layout`'s leaves — so an \
             arity guard makes the single most common shape (one window, one tab, one pane, one \
             file) persist its cwd and nothing else, and the file is not reopened on relaunch. \
             That is the exact macOS bug this smoke exists to keep out of the GTK front-end; see \
             `state_seam::persistable_layout`. A SPLIT here would instead mean the run-1 scenario \
             somehow split the pane. File: {}",
            tab.layout,
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
         lone leaf. {SOLO_DECOY_FILE:?} would instead mean the app reopened the first file it \
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
            "{SMOKE}: expected the smoke's own state file {} to exist after the run, but: {e}. Its \
             absence means the app did not honour WOLF_STATE_DIR — which also means it wrote \
             somewhere else, most likely the developer's real ~/.local/state/wolf/.",
            smoke_state_file.display()
        )
    });
    assert!(
        smoke_meta.len() > 0,
        "{SMOKE}: the smoke's own state file {} exists but is empty",
        smoke_state_file.display()
    );

    // Check 2: `smoke_home(&cwd)` is the `HOME` `cage_command` gives the child, so this is the app's.
    let app_home = smoke_home(&cwd);
    assert!(
        app_home.is_dir(),
        "{SMOKE}: the app's HOME {} is not a directory, so the fallback checks below would pass \
         vacuously — a non-existent path never holds a state.json. `make_temp_cwd` creates \
         `<unique>/project` and `cage_command` hands `<unique>` to the app as HOME, so this must \
         exist.",
        app_home.display()
    );
    for rel in HOME_STATE_DIR_FALLBACKS {
        let leaked = app_home.join(rel).join(STATE_FILE_NAME);
        assert!(
            !leaked.exists(),
            "{SMOKE}: found a state file at {} — inside the app's own $HOME. That path is what \
             `wolf_core::state::state_dir()` resolves to when WOLF_STATE_DIR is ABSENT (the \
             `dirs::state_dir()` / `dirs::data_dir()` chain, XDG default `$HOME/{rel}`), so its \
             existence means the app resolved its state directory from $HOME instead of from \
             WOLF_STATE_DIR. Every smoke is then writing application state somewhere this harness \
             does not control: the relaunch smokes stop sharing one state.json between their two \
             launches, and — because the $HOME redirect is a TEST fixture that does not exist on a \
             developer's machine — the same defect there writes to their real \
             ~/.local/state/wolf/state.json, reading their saved session at launch and clobbering \
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
        "{SMOKE}: the developer's REAL state file {} changed across this smoke run (before: \
         {real_before:?}, after: {real_after:?}; `None` means the file did not exist, which is \
         the normal state on a machine that has never run Wolf interactively and must stay that \
         way). This is a STATE-DIR ISOLATION FAILURE: the app under cage resolved \
         `wolf_core::state_dir()` to the real directory instead of the smoke's own, which means it \
         READ the developer's saved window size, working directory and pane layout at launch (so \
         this smoke did not start from a clean slate) and OVERWROTE them at quit with a throwaway \
         one-pane temp-cwd layout. `cage_command` is the single place WOLF_STATE_DIR is set, and \
         `cage_is_only_spawned_through_the_isolating_helper` is what keeps it single — start \
         there. The ONE benign cause: a real Wolf running concurrently in another terminal, which \
         writes that file on its own render wakes; close it and re-run before concluding anything \
         else.",
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

/// Asserts that a frozen `state.json` holds ONE window whose tabs are, in order,
/// single non-preview leaves named [`TABS_RESTORE_FILES`].
///
/// Shared by the three-tab smoke's two frozen-file assertions because the SHAPE they
/// demand is identical; `diagnosis` is what keeps their two reds apart, since the
/// same wrong count means a different thing at each stage. Both callers pass the
/// prediction for their own falsifier, so a failure names the defect that produces it
/// rather than restating the shape.
fn assert_frozen_tabs(smoke: &str, stage: &str, state_dir: &Path, diagnosis: &str) {
    let file = state_dir.join(STATE_FILE_NAME);
    let saved = load_state(state_dir).unwrap_or_else(|| {
        panic!(
            "{smoke}: wolf_core::load_state could not parse the frozen copy of {stage}'s state at \
             {}. `load_state` is silently-ignore, so a None here means the file is absent, \
             unreadable, malformed, or carries an unsupported version — the scenario already \
             proved it was non-empty (it exits 22 or 23 otherwise), so suspect the SHAPE.",
            file.display()
        )
    });

    assert_eq!(
        saved.windows.len(),
        1,
        "{smoke}: {stage}'s state.json carries {} window(s), not 1. This smoke opens ONE window \
         and never a second, so anything else means the tree's shape is wrong before its tabs are \
         worth reading: ZERO means `snapshot_state` answered None (a file older than the run, or \
         every window's contribution refused), and MORE than one means a window was registered \
         twice or a closed one was never deregistered. File: {}",
        saved.windows.len(),
        file.display()
    );
    let window = &saved.windows[0];

    assert_eq!(
        window.tabs.len(),
        TABS_RESTORE_FILES.len(),
        "{smoke}: {stage}'s persisted window carries {} tab(s), not {}. {diagnosis} A tab's open \
         buffer paths have NO persistence channel of their own — they ride only inside \
         `TabState::layout`'s leaves — so a dropped tab is a lost session rather than a cosmetic \
         miscount, and because the next launch reads exactly this file the loss is permanent from \
         here. File: {}",
        window.tabs.len(),
        TABS_RESTORE_FILES.len(),
        file.display()
    );

    for (index, expected) in TABS_RESTORE_FILES.into_iter().enumerate() {
        let tab = &window.tabs[index];
        match tab.layout.as_ref().and_then(leaf_fields) {
            Some(LeafFields { path, preview, .. }) => {
                assert!(
                    !preview,
                    "{smoke}: {stage}'s tab {index} persisted its lone leaf with `preview: true`, \
                     but no step of this scenario opens a preview. A true here makes the restore \
                     call `wolf_preview_file` instead of `:edit`, so the tab comes back showing a \
                     rendered document where the user had an editor. File: {}",
                    file.display()
                );
                let leaf_name = Path::new(path).file_name().and_then(OsStr::to_str);
                assert_eq!(
                    leaf_name,
                    Some(expected),
                    "{smoke}: {stage}'s tab {index} persisted a leaf naming {path:?}, whose file \
                     name is not {expected:?}. The comparison is on the file NAME rather than the \
                     whole path deliberately — /tmp may resolve through a symlink, so nvim's \
                     reported abs_path need not be string-equal to anything this test could \
                     construct — but the name itself is exact, and the three fixtures are \
                     deliberately different names. ANOTHER tab's file here means the tabs were \
                     assembled or restored in the wrong ORDER, which the next gather writes back, \
                     so the swap compounds on every run. `Untitled.md` means that tab's `:e` never \
                     landed. An EMPTY path means the leaf was resolved from a window that is in \
                     neither `Snapshot::previews` nor `Snapshot::win_buffers` and mapped to `\"\"` \
                     instead of refusing — the silent-loss shape `pane_layout::resolve_leaves` \
                     exists to make unreachable. File: {}",
                    file.display()
                );
            }
            None => panic!(
                "{smoke}: {stage}'s tab {index} persisted `layout: {:?}` where a single named \
                 leaf was due. **`None` is the failure that matters**: the tab is present in the \
                 tree and carries its cwd and its sidebar preference, so every count above is \
                 satisfied — and the tab still comes back EMPTY, because its files ride only \
                 inside the layout. A SPLIT here would instead mean the scenario somehow split a \
                 pane. File: {}",
                tab.layout,
                file.display()
            ),
        }
    }
}

/// Proves the whole per-tab persistence claim in one scenario: **three tabs, saved
/// in tab order with their layouts, restored in tab order at relaunch, and still all
/// three after a second quit in which two of them were never selected.**
///
/// Two app launches run inside ONE `cage` instance (see [`RESTORE_TABS_SCENARIO`]),
/// so the smoke holds one cage slot and no second compositor is started.
///
/// ## Why it asserts on no pixel
///
/// The claim is about what reaches `state.json` and what comes back out of it, and
/// neither is visible. A background tab's pane view is not drawn at all, so no
/// capture could show more than one tab's content whatever the file holds; and a tab
/// TITLE is the current file's name, which the per-pane statusbar strip and nvim's
/// `:e` echo also draw, so no OCR term could discriminate one. The two observables
/// that DO discriminate are both techniques this binary already proves it can read: a
/// `writefile` readback from the restored Neovim, and a `state.json` frozen before
/// anything can write over it.
///
/// ## Why THREE tabs, and why that is load-bearing
///
/// Materialization creates the queued tabpages in order and each `create_tabpage`
/// leaves Neovim current on the tabpage it made, so at the end of the loop Neovim is
/// on the LAST one until the queued `set_current_tabpage` for tab 0 lands. The last
/// tab may therefore get one refresh and the first certainly does — only a MIDDLE tab
/// is guaranteed never to be Neovim-current at launch. Tab 1 is that tab, and it is
/// the only one whose persisted leaf in assertion 3 can ONLY have been resolved from
/// the window-keyed snapshot maps: run 2 types nothing but the readback, so the
/// current tabpage's `Snapshot::windows` never held tab 1's windows for a single wake
/// of that run.
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` asserts the count so
/// a later trim to two tabs cannot quietly take the guarantee away.
///
/// ## What it asserts
///
/// The scenario's own checks run inside `cage` and carry their own exit codes (see
/// [`RESTORE_TABS_SCENARIO`]'s table); in particular exit 21 proves run 1 really
/// reached three tabpages, so a one-tab save cannot be confused with a chord that
/// never fired. Three more are the Rust side's:
///
/// 1. **The SAVE side.** `$TABS_STATE_COPY_1` — run 1's file, frozen before run 2
///    could touch it — holds one window with THREE tabs whose layouts are single
///    non-preview leaves named `talfa.md` / `tbravo.md` / `tcharlie.md`, in that
///    order. Two different reverts leave ONE tab here, and the survivor's NAME is what
///    tells them apart: F1 and F1' in the table below.
/// 2. **The RESTORE side.** The readback is exactly three lines, `talfa.md` /
///    `tbravo.md` / `tcharlie.md` (F2). The scenario re-issues that readback until
///    three tabpages exist or its budget is spent, so a short answer reaching this
///    assertion is a restore that never happened rather than one that had not happened
///    YET — the false diagnosis a single write after a fixed settle would produce on a
///    slow host. F2's red is distinguishable from assertion 1's, which is why run 1's
///    file is frozen — the two-revert structure
///    [`single_pane_file_survives_relaunch_under_cage`] documents.
/// 3. **The BACKGROUND tabs' gather.** `$TABS_STATE_COPY_2` — run 2's file, after a
///    quit in which tabs 1 and 2 were never selected — still holds the same three
///    tabs with the same three layouts, in order. F3 is the revert this stage owns,
///    and it reddens at assertion 1 rather than here, for the reason its row gives.
///    **The seed is NOT what this measures**, and F4 is the measurement that says so.
///
/// ## Red-run falsifiers (each run alone)
///
/// | # | edit | predicted red | isolated | measured |
/// | --- | --- | --- | --- | --- |
/// | F1 | in `state_seam.rs`, make `window_subtree` truncate its `tabs` vec to one | assertion 1: run 1's frozen file holds ONE tab, and since `truncate(1)` keeps the FIRST entry it names `talfa.md` | within the smoke, yes — the scenario's exit-21 check proves run 1 really reached three tabpages first; across the suite no (`window_subtree_writes_every_tab_in_order` reddens too) | **AS PREDICTED.** Run 1's frozen `state.json` carried **1 tab, naming `talfa.md`**. |
/// | F1' | in `window.rs`, make `cached_subtree` gather `tabs.selected()` alone rather than every entry | assertion 1: ONE tab naming **`tcharlie.md`**, the tab the user was on when run 1 quit | yes | **DERIVED, not measured.** The name follows from `selected()` being tab 2 at the quit. It is in the table because assertion 1's message promises the two one-tab reds are tellable apart, and that promise is only worth anything if both names are written down. |
/// | F2 | in `window.rs`, delete `build`'s `queue_restore` call | assertion 2: the readback is ONE line | yes — assertion 1 reads run 1's FROZEN file and passes first, which is exactly what the freeze is for | **AS PREDICTED.** The readback was **exactly one line, `talfa.md`**, and assertion 1 passed before it. |
/// | F3 | in `pane_view.rs`, replace `leaf_contents`' `win_buffers[win]` lookup with the current-tabpage-only join `WolfCore::pane_status` performs | the background tabs lose their layouts — reported at **assertion 1**, because run 1's own background tabs are gathered the same way and its file is read first | no, and deliberately so: this is the one revert whose red arrives above the stage that owns it | **AS PREDICTED, at assertion 1.** The frozen run-1 file read `0 None / 1 None / 2 tcharlie.md` — the two background tabs lost their files while the current one kept its. Run 2's copy is expected to lose its own the same way, unmeasured because the test aborts above it. |
/// | F4 | in `state_seam.rs`, make `seeded_tab_state` return `None` unconditionally | none — this smoke does not rest on a seed | n/a | **MEASURED GREEN**, and recorded so a later reader does not mistake this smoke for the seed's coverage. All three assertions passed: `realize_layout` creates a restored tab's panes synchronously, so every restored tab is gathered LIVE from then on and never falls back to its seed. |
///
/// ## How it discriminates RESTORATION from COINCIDENCE
///
/// A fresh launch of `wolf-gnome` shows exactly ONE tab holding `Untitled.md`: the
/// binary takes no CLI arguments, opens no file at startup, and has no MRU-reopen
/// path. Neovim's own behaviour cannot produce three tabpages either — the captive
/// nvim is spawned fresh with `-u init.lua`, there is no `:mksession`, and
/// `:tabnew` / `:tabnext` / `:tabclose` are locked out in the shared runtime, so the
/// current tabpage moves only because this front-end asked. Three tabpages holding
/// three DISTINCT files cannot arise from any single-file reopen, any MRU, or any
/// swap recovery, all of which give one tab and one name.
///
/// ## What it does NOT prove
///
/// **Nothing here reaches `tabs_ui::restore_press`'s `Wait` arm.** Arranging a
/// wedged or absent core at the one wake where tabs are still queued is not something
/// a compositor scenario can schedule; that arm is table-tested only.
///
/// **Nothing here observes a REFUSED gather.** `pane_layout::resolve_leaves`'
/// all-or-nothing rule needs a leaf whose Neovim window has left both snapshot maps
/// while its pane survives, which is timing-dependent under a compositor. Its table
/// is what covers it.
///
/// **Nothing here rests on a SEED.** Both seeded paths are table-tested only, in
/// `state_seam.rs`: `seeded_tab_state_table` for a tab whose own gather never
/// succeeds, which under a compositor needs `realize_layout` itself to fail, and
/// `pending_tab_states_table` for a tab still sitting in the restore queue, whose
/// window is sub-second on a healthy launch and unbounded only behind a Neovim wedged
/// from the first wake.
#[test]
fn three_tabs_survive_a_relaunch_under_cage() {
    const SMOKE: &str = "three_tabs_survive_a_relaunch_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // One cwd for both launches: `plan_for_window` restores tabs only for the saved project.
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
    // the app as WOLF_STATE_DIR, so the two runs share one `state.json`.
    let smoke_state_file = smoke_state_dir(&cwd).join(STATE_FILE_NAME);

    // Two frozen copies, so assertion 1's red stays apart from 3's: run 2 re-snapshots every wake, so
    // the live file would hide which side dropped the tabs.
    let run1_state_dir = runtime_dir.join("wolf_gnome_tabs_run1_state");
    let run2_state_dir = runtime_dir.join("wolf_gnome_tabs_run2_state");
    for dir in [&run1_state_dir, &run2_state_dir] {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| {
            panic!(
                "{SMOKE}: failed to create the state snapshot dir {}: {e}",
                dir.display()
            )
        });
    }

    let run1_readback = cwd.join(TABS_RESTORE_RUN1_READBACK_NAME);
    let readback = cwd.join(TABS_RESTORE_READBACK_NAME);
    let run1_state_file = run1_state_dir.join(STATE_FILE_NAME);
    let run2_state_file = run2_state_dir.join(STATE_FILE_NAME);
    // Start from a clean slate: evidence that could be a previous run's leftover is
    // not evidence. `clear_evidence` removes AND asserts absent.
    for path in [
        &run1_readback,
        &readback,
        &run1_state_file,
        &run2_state_file,
    ] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(RESTORE_TABS_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("TABS_FILE_ONE", TABS_RESTORE_FILE_ONE)
        .env("TABS_FILE_TWO", TABS_RESTORE_FILE_TWO)
        .env("TABS_FILE_THREE", TABS_RESTORE_FILE_THREE)
        .env("TABS_RUN1_READBACK", &run1_readback)
        .env("TABS_READBACK", &readback)
        .env("TABS_STATE_SRC", &smoke_state_file)
        .env("TABS_STATE_COPY_1", &run1_state_file)
        .env("TABS_STATE_COPY_2", &run2_state_file)
        .status()
        .expect("failed to spawn `cage` for the headless three-tab relaunch scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage three-tab relaunch scenario exited unsuccessfully ({status}). Read the \
         exit code FIRST; each names a different half of the claim.\n\
         Exit 11: run 1's app died before it was ready for input — a LAUNCH failure.\n\
         Exit 12: run 1 was still alive after `:qa`.\n\
         Exit 13: run 1 exited NON-ZERO — it went away, but not cleanly. Three tabs close in \
         sequence here, so a borrow discipline that held at one tab is the first suspect.\n\
         Exit 14: run 2's app died before it was ready for input.\n\
         Exit 15: run 1's TABPAGE-COUNT readback file never appeared.\n\
         Exit 16: run 2 was still alive after `:qa`.\n\
         Exit 17: run 2 exited NON-ZERO — it RESTORED tabs, so suspect `materialize_restore`.\n\
         Exit 21: run 1 never reached three tabpages, so Ctrl+Shift+T failed and nothing here is \
         about persistence. That is deliberately separated from the assertions below, because a \
         one-tab save.json and a chord that never fired look identical in the file.\n\
         Exit 22: run 1 wrote NO state.json at all at {} — upstream of anything about tabs.\n\
         Exit 23: run 2 wrote NO state.json at all, so assertion 3's evidence is missing (run 1's \
         copy is already frozen, so assertion 1 is unaffected).\n\
         Exit 24: run 2's readback got NO content at all across every re-issue, so the `:call` \
         keystrokes never reached Neovim. Note what this code does NOT mean: a readback that \
         appeared but stayed SHORT is not an exit at all — the scenario leaves it standing so \
         assertion 2 below can name what is in it, because a short readback is the restore side's \
         own red rather than a harness failure.\n\
         The scenario's own stderr names the candidate causes for each in order.",
        smoke_state_file.display()
    );

    // --- Assertion 1: the SAVE side, on RUN 1's frozen file --------------------
    assert_frozen_tabs(
        SMOKE,
        "run 1",
        &run1_state_dir,
        "The scenario's exit-21 check already proved the window really held THREE tabpages when it \
         quit, so this is the SAVE side and nothing else. ONE tab is the red for two different \
         reverts, and the NAME the survivor carries is what tells them apart: talfa.md means \
         `state_seam::window_subtree` truncated its tabs vec, since `truncate(1)` keeps the FIRST \
         entry — that pairing is MEASURED, not predicted; tcharlie.md means \
         `window::cached_subtree` gathered `tabs.selected()` alone instead of every entry, since \
         that is the tab the user was on at the quit. A tab whose order or whose layout is wrong \
         rather than missing is a different defect again, and the per-tab check names it.",
    );

    // Assertion 2: the restore side. The scenario polled until three tabpages existed, so a short
    // readback is a missing tab, not a slow host.
    let raw = read_readback(&readback, SMOKE);
    // No filtering: an unnamed buffer's tab writes an empty line, which must stay visible.
    let got: Vec<&str> = raw.lines().map(str::trim).collect();
    assert_eq!(
        got.as_slice(),
        TABS_RESTORE_FILES.as_slice(),
        "{SMOKE}: after the relaunch the restored Neovim reports {got:?}, not \
         {TABS_RESTORE_FILES:?}. One line per tabpage in tabpage order, each holding that \
         tabpage's buffer tail names — so this one comparison covers the tab COUNT, the buffer \
         IDENTITIES and their ORDER at once. Read it this way: a SINGLE line means no tab was \
         restored beyond the first, which is what deleting `window::build`'s `queue_restore` call \
         produces (assertion 1 above already proved all three were SAVED, so with that green this \
         points squarely at the restore side); the three right names in the WRONG ORDER means the \
         queued layouts were materialized out of order, and since the next gather writes that back \
         the swap compounds every run; `Untitled.md` on a line means that tab was created but its \
         layout never realized, i.e. `realize_layout` did not run while its tabpage was current; \
         and MORE than three lines means materialization ran twice, which the taken queue is \
         supposed to make impossible. A SHORT answer here is NOT a slow launch: the scenario \
         re-issued this readback until three tabpages existed or its budget ran out, and its \
         stderr reports the line count it settled at and how many re-issues that took. Readback \
         file kept at {}.",
        readback.display()
    );

    // Assertion 3: tab 1 was never current in run 2, so its file gets in only by `leaf_contents`
    // resolving it from `Snapshot::win_buffers` by the pane's window handle.
    assert_frozen_tabs(
        SMOKE,
        "run 2",
        &run2_state_dir,
        "Assertion 2 above already proved all three tabs were RESTORED, so a tab missing here was \
         lost between the restore and the quit — by the very act of running. What this stage \
         measures is the BACKGROUND tabs' gather: tab 1 is never Neovim-current in run 2, so its \
         leaf can only have been resolved from `Snapshot::win_buffers` by the pane's own window \
         handle. Reverting that to the current-tabpage-only join `pane_status` performs shows up \
         as `layout: None` on the background tabs rather than as a wrong count, so the per-tab \
         check below is what reports it; a wrong COUNT here means a tab dropped out of the \
         contribution set altogether. It is NOT the seed: `state_seam::seeded_tab_state` returning \
         None leaves this green, because a restored tab's panes are created synchronously and it \
         is gathered live from then on.",
    );
}

/// Proves the whole multi-window claim in one scenario: **two windows, two
/// Neovims, both sessions in one `state.json`, and both restored at relaunch.**
///
/// Two app launches run inside ONE `cage` instance (see [`TWOWIN_SCENARIO`]), so
/// the smoke holds one cage slot and no second compositor is started.
///
/// ## Why it asserts on no pixel, and why that is the honest shape
///
/// `cage(1)`: "Cage runs a single, maximized application … only a single one is
/// visible at any point in time." MEASURED under `cage`: two
/// `adw::ApplicationWindow`s in one process both exist and `app.windows()` reports
/// 2, but the newest toplevel stays on top and presenting the backgrounded first
/// window does **not** raise it. So no capture can show two windows and none can
/// bring the first one back — a screenshot here would be a picture of one window,
/// which is exactly what a build with no second window at all produces.
///
/// The two observables that DO discriminate are both techniques this binary already
/// proves it can read: the count of live `nvim` children of the app pid (the crash
/// smokes' `/proc` scan, extended by `wait_nvim_count`) and the `state.json` run 1
/// wrote, frozen before run 2 can overwrite it (the solo relaunch smoke's
/// `load_state` on a frozen copy).
///
/// ## Why it earns its seconds
///
/// Every headless test in this crate can only see values the front-end computed —
/// never that GTK really built a second window over a second core, and never that
/// the store received a two-window tree. The failure this smoke exists to catch is
/// SILENT: two `StateStore`s over one directory both report a successful flush, the
/// file holds whichever tree was written last, and each store still claims its own
/// in memory, so one window's entire session is lost with no panic and no error and
/// a green headless suite. The `nvim`-child count and the frozen file are the only
/// two observables that tell that apart.
///
/// MEASURED cost: 16.73 s run ALONE, and +16.55 s / +17.38 s on two full-suite
/// runs — the cheapest of this binary's two-launch smokes despite starting four
/// Neovims across its two runs.
///
/// ## The route to the second window is forced
///
/// A second `wolf-gnome` process under the same compositor is not New Window. Every
/// cage has its own private `dbus-run-session` bus, so a second launch while the
/// first is alive hands off to it through single-instance activation, which
/// exercises the launch path rather than a second window. The palette is the only
/// route this harness can drive — New Window
/// carries no accelerator, and the primary menu popover cannot be clicked (see the
/// deliberate gaps in `tests/README.md`).
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
/// (see [`TWOWIN_SCENARIO`]'s table): two windows means two live `nvim` children
/// after the palette activation and again after the relaunch, the new window did not
/// inherit a project, the frozen `state.json` carries two windows' worth of tabs,
/// and the focused restored window holds a coherent session. Four more are the Rust
/// side's, on the frozen copy of run 1's save:
///
/// 1. `load_state` parses the frozen file and it carries **exactly two** windows.
/// 2. Each window carries exactly one tab.
/// 3. The two tabs' `cwd` values name the two projects **in saved order**: window 0
///    at the temp cwd, window 1 at its subdirectory. Compared on the final path
///    component, because `/tmp` may resolve through a symlink and nvim's reported
///    path need not be string-equal to anything this test could build — the
///    component itself is exact.
/// 4. Each tab's `layout` is a non-preview leaf whose file NAME is that window's own
///    fixture. This is the assertion no readback can make: a tab's open buffer paths
///    ride ONLY inside `TabState::layout`'s leaves, so a tree that carried two
///    windows with no layouts would satisfy 1–3 and restore two empty windows.
///
/// ## What it does NOT prove
///
/// **Nothing here observes the `SwitchWindow` raise.** `Coordinator::raise` calls
/// `present()`, which `cage` declines, so the palette's Switch to Window rows are
/// proven as VALUES by `palette.rs`'s tables and by nothing here.
///
/// **Nothing here closes one of two windows**, so `close_commit`'s `DropSubtree` arm
/// is untouched — and on the LAST window that arm is not merely unobserved but
/// indistinguishable from a no-op, because an empty contribution set means "commit
/// nothing" rather than "commit an empty tree". Its table in `state_seam.rs` is what
/// covers the rule.
///
/// **It does not discriminate the close path from the per-wake commit.** The quit
/// path's commit has already been preceded by per-wake commits of the same tree, so
/// this smoke's `state.json` would be right by either route; that redundancy is
/// pre-existing and is recorded in `tests/README.md`.
///
/// **Which window has focus after a restore is not asserted**, deliberately. Only
/// one window can be typed at under `cage` and nothing can raise another, so step 10
/// reads whichever window is focused and asserts the PAIR it reports is one of the
/// two that were saved. That discriminates `Untitled.md` (no layout was rebuilt) and
/// a crossed restore (a window sitting in one project while showing the other's
/// file) without resting on an unmeasured claim about focus order.
///
/// ## Red-run falsifiers (all three, plus step 6's, each run alone)
///
/// | # | edit | predicted red | isolated | measured |
/// | --- | --- | --- | --- | --- |
/// | F1 | in `command_action.rs`, restore `Command::NewWindow => UiAction::Deferred(..)` | the palette's classification filter drops the New Window row, so the typed query lists nothing, Enter dispatches nothing, and step 3 aborts with **exit 21** and `count=1` — steps 4–10 unreached | within the smoke, yes; across the suite no (`command_action.rs`'s and `actions.rs`'s tables redden too) | **AS PREDICTED.** Exit 21 at step 3: `expected EXACTLY 2 live Neovim child(ren) … the last scan counted 1`. The count poll exhausts its budget rather than answering early, so the run is LONGER than the green one — 26.39 s. Steps 4–10 are unreached by construction, which is what makes the exit code readable on its own. |
/// | F2 | in `coordinator.rs`, make `assemble` walk `self.handles().iter().take(1)` | step 7 aborts with **exit 25** (`grep -c` reports 1); run 2 never launches, so the restore poll cannot mask it | **yes** — this is exactly why step 7 exists | **AS PREDICTED.** Exit 25 at step 7: `run 1's state.json carries 1 line(s) matching '"tabs"', not 2`. The isolation claim is MEASURED rather than argued: 11.77 s against 16.73 s green — SHORTER than the passing run, because run 2 never launched. That timing is the evidence that the write-side guard fires before the restore poll can turn a write-side defect into a read-side one. |
/// | F3 | in `coordinator.rs`, make `open_new_window` pass the process cwd into `restore_plans` | step 4's readback is the project directory and the smoke aborts with **exit 22**; steps 5–10 unreached | **yes** (step 3 passes first) | **AS PREDICTED, and exit 22 rather than exit 26** — the discrimination those two codes exist for. The readback was `getcwd() = …/project` with `bufname('%') = Untitled 2.md`, so the focus guard passed first (the keystrokes DID reach the new window) and the inheritance check is what fired. 8.91 s. The `Untitled 2.md` is also why step 4's comment names no fixed ordinal. |
/// | step 6 | in `coordinator.rs`, delete the body of `install_app_actions` | Ctrl+Shift+Q resolves no action, falls through to Neovim as a plain `<C-Q>`, the app stays alive, and step 6 aborts with **exit 12** | **yes** — nothing else in this suite activates an `app.` action | **AS PREDICTED.** Exit 12 at step 6: `run 1 (pid …) is still alive after Ctrl+Shift+Q`. Steps 3, 4 and 5 all passed first — two live `nvim` children, no inherited project, window B re-rooted — so the failure is isolated to `app.quit` and says nothing about building the second window. 26.88 s. |
///
/// Each `measured` cell is from a run of that edit ALONE, per `tests/README.md`'s
/// "A red-run demonstration is a deliverable, not a formality".
#[test]
fn two_windows_two_neovims_and_two_saved_sessions_under_cage() {
    const SMOKE: &str = "two_windows_two_neovims_and_two_saved_sessions_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd shared by BOTH launches, holding window A's file and — in a
    // subdirectory of its own — window B's SECOND project.
    let cwd = make_temp_cwd();
    // `write_relaunch_fixtures` creates no parents. Two projects, since one would share a swap dir and
    // meet Neovim's `W325` warning.
    let subdir = cwd.join(TWOWIN_SUBDIR);
    std::fs::create_dir_all(&subdir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the second project's directory {}: {e}",
            subdir.display()
        )
    });
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (TWOWIN_FIRST_FILE, TWOWIN_FIRST_HEADING),
            (TWOWIN_SECOND_FILE, TWOWIN_SECOND_HEADING),
        ],
    );

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
    // the app as WOLF_STATE_DIR, so the two runs share one `state.json`.
    let smoke_state_file = smoke_state_dir(&cwd).join(STATE_FILE_NAME);

    // Freezes run 1's save before run 2 overwrites it. A directory, because `load_state` takes one;
    // under XDG_RUNTIME_DIR so the sidebar never lists it.
    let run1_state_dir = runtime_dir.join("wolf_gnome_twowin_run1_state");
    std::fs::create_dir_all(&run1_state_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the run-1 state snapshot dir {}: {e}",
            run1_state_dir.display()
        )
    });
    let run1_state_file = run1_state_dir.join(STATE_FILE_NAME);

    let b_cwd_readback = cwd.join(TWOWIN_READBACK_B_NAME);
    let run2_readback = cwd.join(TWOWIN_READBACK_R2_NAME);
    for path in [&b_cwd_readback, &run2_readback, &run1_state_file] {
        clear_evidence(path, SMOKE);
    }

    // This scenario calls none of the fragment's exiting functions, so its 16/17/18/20 are unreachable.
    let scenario = with_nvim_pid_helper(TWOWIN_SCENARIO);
    let status = cage_command(&scenario, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("TWOWIN_FIRST_FILE", TWOWIN_FIRST_FILE)
        .env("TWOWIN_SECOND_NAME", TWOWIN_SECOND_NAME)
        .env("TWOWIN_SUBDIR_ABS", &subdir)
        .env("TWOWIN_SUBDIR_BASE", TWOWIN_SUBDIR)
        .env("TWOWIN_PROJECT_BASE", TEMP_CWD_BASENAME)
        .env("TWOWIN_NEW_WINDOW_QUERY", TWOWIN_NEW_WINDOW_QUERY)
        .env("TWOWIN_B_CWD", &b_cwd_readback)
        .env("TWOWIN_R2_READBACK", &run2_readback)
        .env("TWOWIN_STATE_SRC", &smoke_state_file)
        .env("TWOWIN_STATE_COPY", &run1_state_file)
        .env("TWOWIN_STATE_TABS_LINES", TWOWIN_STATE_TABS_LINES)
        .env("RESTORE_SETTLE", RELAUNCH_RESTORE_SETTLE)
        .status()
        .expect("failed to spawn `cage` for the headless two-window scenario");
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
         Exit 16: run 1 wrote NO state.json at all at {} — upstream of anything about windows.\n\
         Exit 21: run 1 did not have EXACTLY TWO live `nvim` children after the palette \
         activation. This is the smoke's central claim: one core per window, so a count of 1 means \
         no second window was built (check `ui_action_for(Command::NewWindow)`, \
         `AppShell::perform`'s arm, and `Coordinator::open_new_window`).\n\
         Exit 22: the new window INHERITED the invoking window's project directory. New Window \
         must build its plan from no saved state and no process cwd.\n\
         Exit 23: run 2 did not have EXACTLY TWO live `nvim` children after the restore, so \
         `restore_plans` did not walk every saved window.\n\
         Exit 24: the focused restored window's `getcwd()` / `bufname('%')` pair is not one of the \
         two that were saved.\n\
         Exit 25: run 1's state.json does not carry two windows' worth of tabs — a WRITE-side \
         failure, caught deliberately before run 2 could mask it as a restore failure.\n\
         Exit 26: step 4's readback came from the INVOKING window (it names the first project's \
         file, which only that window has open), so the keystrokes never reached the new one. That \
         is a focus failure and NOT the inheritance exit 22 reports; the two are separated so \
         neither can be read as the other.\n\
         The scenario's own stderr names the candidate causes for each in order.",
        smoke_state_file.display()
    );

    // --- Assertions 1-4: RUN 1's frozen save holds BOTH sessions, in order -----
    let saved = load_state(&run1_state_dir).unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::load_state could not parse the frozen copy of run 1's state at \
             {}. `load_state` is silently-ignore, so a None here means the file is absent, \
             unreadable, malformed, or carries an unsupported version — the scenario already \
             proved it was non-empty and that it held two windows' worth of tabs (it exits 16 and \
             25 otherwise), so suspect the shape rather than the contents.",
            run1_state_file.display()
        )
    });
    assert_eq!(
        saved.windows.len(),
        2,
        "{SMOKE}: run 1's state.json carries {} window(s), not 2. This is the smoke's whole claim \
         on the write side. ONE means the tree lost a window between the two windows' subtrees and \
         the file: `Coordinator::assemble` collects every live window's cached subtree in registry \
         order, and a window that contributed nothing — its `may_snapshot` gate shut and no cached \
         subtree ever gathered — drops out of the tree silently. ZERO means \
         `state_seam::snapshot_state` answered None, which the coordinator turns into \"commit \
         nothing\", so a zero here is a file older than the run. MORE than two means a window was \
         registered twice or a closed one was never deregistered. The scenario's exit-25 guard \
         already counted two `\"tabs\"` lines in this same file, so a disagreement between the two \
         means the tree's SHAPE is wrong rather than its size. File: {}",
        saved.windows.len(),
        run1_state_file.display()
    );

    for (index, dir_name, file_name) in [
        (0_usize, TEMP_CWD_BASENAME, TWOWIN_FIRST_FILE),
        (1_usize, TWOWIN_SUBDIR, TWOWIN_SECOND_NAME),
    ] {
        let window = &saved.windows[index];
        assert_eq!(
            window.tabs.len(),
            1,
            "{SMOKE}: run 1's window {index} carries {} tab(s), not 1. `window_subtree` writes \
             every tab the window contributes and this scenario opens one per window, so anything \
             else means the subtree gathered a tab the window does not have. File: {}",
            window.tabs.len(),
            run1_state_file.display()
        );
        let tab = &window.tabs[0];

        // The last component only, since /tmp may resolve through a symlink; the two projects differ there.
        let cwd_name = Path::new(&tab.cwd).file_name().and_then(OsStr::to_str);
        assert_eq!(
            cwd_name,
            Some(dir_name),
            "{SMOKE}: run 1's window {index} persisted cwd {:?}, whose final component is not \
             {dir_name:?}. `None` means the cwd is the empty string, which is what \
             `state_seam::snapshot_cwd` writes for a window still in the EMPTY STATE however far \
             its Neovim has wandered — for window 1 that means the typed `:cd` never re-rooted the \
             window, so `settle_reroot` never called `set_empty_state(false)`. The OTHER \
             project's name here means the two windows' subtrees were assembled in the wrong \
             ORDER, and since restore is positional that hands each window the other's session on \
             the next launch. File: {}",
            tab.cwd,
            run1_state_file.display()
        );

        match tab.layout.as_ref().and_then(leaf_fields) {
            Some(LeafFields { path, preview, .. }) => {
                assert!(
                    !preview,
                    "{SMOKE}: run 1's window {index} persisted its lone leaf with `preview: true`, \
                     but no scenario step opens a preview. A true here makes the restore call \
                     `wolf_preview_file` instead of `:edit`. File: {}",
                    run1_state_file.display()
                );
                let leaf_name = Path::new(path).file_name().and_then(OsStr::to_str);
                assert_eq!(
                    leaf_name,
                    Some(file_name),
                    "{SMOKE}: run 1's window {index} persisted a leaf naming {path:?}, whose file \
                     name is not {file_name:?}. The comparison is on the file NAME rather than the \
                     whole path deliberately (/tmp may resolve through a symlink), but the name is \
                     exact — and the two fixtures are deliberately different names, so the OTHER \
                     window's file here means the subtrees were assembled in the wrong order. \
                     `Untitled.md` means that window's `:e` never landed before the quit. File: {}",
                    run1_state_file.display()
                );
            }
            None => panic!(
                "{SMOKE}: run 1's window {index} persisted `layout: {:?}` where a single \
                 named leaf was due. **`None` is the failure that matters**: a tab's open buffer \
                 paths have NO persistence channel of their own — they ride only inside \
                 `TabState::layout`'s leaves — so a tree that carries both windows and neither \
                 layout satisfies every count in this test and still restores two EMPTY windows. \
                 For window 1 specifically, a `None` is also what a suppressed-by-arity save \
                 produces; `state_seam::persistable_layout` must gate on \
                 `pane_layout::is_empty_leaf` and never on a leaf count. A SPLIT here would \
                 instead mean the scenario somehow split a pane. File: {}",
                tab.layout,
                run1_state_file.display()
            ),
        }
    }
}

/// A dragged divider's weights reach `state.json` and come back on the next launch.
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
/// 1. **The save side, on the FILE.** The `state.json` RUN 1 wrote — frozen by the
///    scenario before run 2 can write over it — carries a `Vertical` split of two
///    named leaves whose weights sum to 1 and differ by at least
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
    // WOLF_STATE_DIR, so the two runs share one `state.json`.
    let smoke_state_file = smoke_state_dir(&cwd).join(STATE_FILE_NAME);

    // Freezes run 1's save. A directory, because `load_state` takes one; under XDG_RUNTIME_DIR so the
    // sidebar never lists it.
    let run1_state_dir = runtime_dir.join("wolf_gnome_divider_run1_state");
    std::fs::create_dir_all(&run1_state_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the run-1 state snapshot dir {}: {e}",
            run1_state_dir.display()
        )
    });
    let run1_state_file = run1_state_dir.join(STATE_FILE_NAME);

    let w_base = cwd.join(DIVIDER_W0_NAME);
    let w_drag = cwd.join(DIVIDER_W1_NAME);
    let w_restored = cwd.join(DIVIDER_W2_NAME);
    // Outside the cwd: the bisection rewrites it once per probe and the sidebar
    // watches the cwd.
    let probe = runtime_dir.join("wolf_gnome_divider_relaunch_probe.txt");
    let out_png = runtime_dir.join("wolf_gnome_divider_relaunch_smoke.png");
    for path in [
        &w_base,
        &w_drag,
        &w_restored,
        &probe,
        &out_png,
        &run1_state_file,
    ] {
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
        .env("DIV_STATE_SRC", &smoke_state_file)
        .env("DIV_STATE_COPY", &run1_state_file)
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
         Exit 18: run 1 wrote NO state.json at all at {}, which is upstream of anything about \
         weights.\n\
         Exit 19: run 2's app died before it was ready for input.\n\
         Exit 21: the bisection seeds ({seed_lo}, {seed_hi}) did not straddle the divider; the \
         scenario's stderr names the four candidate causes in order, and \
         `click_positions_the_cursor_under_cage` fails on its own if the pane origin moved.\n\
         Screenshot expected at {}.\n\
         wolf-pointer-inject stderr was:\n{helper_err}",
        smoke_state_file.display(),
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
    let saved = load_state(&run1_state_dir).unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::load_state could not parse the frozen copy of run 1's state at {}. \
             `load_state` is silently-ignore, so a None means the file is absent, unreadable, \
             malformed or of an unsupported version — the scenario already proved it was non-empty \
             (it exits 18 otherwise), so suspect the shape.",
            run1_state_file.display()
        )
    });
    let window = saved.windows.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's state.json carries NO windows at all, so nothing was persisted. File: \
             {}",
            run1_state_file.display()
        )
    });
    let tab = window.tabs.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's persisted window carries NO tabs. `snapshot_state` writes exactly \
             one. File: {}",
            run1_state_file.display()
        )
    });
    match &tab.layout {
        Some(PaneNode::Split {
            orientation,
            weights,
            children,
        }) => {
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
                "{SMOKE}: run 1 persisted leaves naming {tails:?}, not the two fixture files in \
                 left-to-right order. The weights below are parallel to THIS child order, so they \
                 cannot be read at all until it is right. A `None` entry is a nested split where a \
                 leaf belongs; the comparison is on the file NAME rather than the whole path \
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
            "{SMOKE}: run 1 persisted `layout: {other:?}` for two side-by-side panes, not a \
             two-child Vertical split. A `None` means nothing was persisted at all (the snapshot \
             gate refused every wake) and a `Leaf` means the split never reached the pane tree — \
             either way this is the topology failure `pane_layout_survives_relaunch_under_cage` \
             owns, not a weight failure. File: {}",
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
