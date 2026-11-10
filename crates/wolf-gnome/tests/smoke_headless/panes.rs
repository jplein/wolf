//! Pane and layout smokes: splits, focus moves, closing panes, `:q` and `:qa`, the per-pane
//! statusbar and native tabs.

use super::*;

// Vertical-split smoke. Fixture names must not contain the heading words (see `PICKER_FILE_NAME`).
// See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#split-pane-smokes

/// Vertical-split smoke: file opened in the FIRST (left) pane.
///
/// Deliberately UNLIKE [`VSPLIT_OCR_LEFT`] (see the note above): the sidebar row
/// and nvim's `:e` toast both show this FILENAME, so a name containing the asserted
/// word would satisfy the assertion whether or not the pane ever rendered.
const VSPLIT_FILE_LEFT: &str = "pleft.md";

/// Heading written into [`VSPLIT_FILE_LEFT`]; lowercased it contains
/// [`VSPLIT_OCR_LEFT`]. This line renders only once nvim has loaded the file INTO A
/// PANE, which is what makes it the honest evidence the split happened.
const VSPLIT_HEADING_LEFT: &str = "# PANELEFT";

/// Lowercased substring OCR must find for the LEFT pane's file.
pub(crate) const VSPLIT_OCR_LEFT: &str = "paneleft";

/// Vertical-split smoke: file opened in the SECOND (right) pane after `<C-w>v`.
///
/// Deliberately UNLIKE [`VSPLIT_OCR_RIGHT`], for the reason given on
/// [`VSPLIT_FILE_LEFT`].
const VSPLIT_FILE_RIGHT: &str = "pright.md";

/// Heading written into [`VSPLIT_FILE_RIGHT`]; lowercased it contains
/// [`VSPLIT_OCR_RIGHT`].
const VSPLIT_HEADING_RIGHT: &str = "# PANERIGHT";

/// Lowercased substring OCR must find for the RIGHT pane's file.
pub(crate) const VSPLIT_OCR_RIGHT: &str = "paneright";

// Horizontal-split smoke: the vertical-split smoke with the two panes stacked.
// See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#split-pane-smokes

/// Horizontal-split smoke: file opened in the FIRST (top) pane.
const HSPLIT_FILE_TOP: &str = "htop.md";

/// Heading written into [`HSPLIT_FILE_TOP`]; lowercased it contains
/// [`HSPLIT_OCR_TOP`].
const HSPLIT_HEADING_TOP: &str = "# HTOPFILE";

/// Lowercased substring OCR must find for the TOP pane's file.
pub(crate) const HSPLIT_OCR_TOP: &str = "htopfile";

/// Horizontal-split smoke: file opened in the SECOND (bottom) pane after `<C-w>s`.
const HSPLIT_FILE_BOTTOM: &str = "hbot.md";

/// Heading written into [`HSPLIT_FILE_BOTTOM`]; lowercased it contains
/// [`HSPLIT_OCR_BOTTOM`].
const HSPLIT_HEADING_BOTTOM: &str = "# HBOTFILE";

/// Lowercased substring OCR must find for the BOTTOM pane's file.
pub(crate) const HSPLIT_OCR_BOTTOM: &str = "hbotfile";

// Focus-move smoke: proves the chord reaches `nvim_set_current_win`, not which pane got the
// marker; positional OCR is too fragile, and `layout::neighbor` unit tests cover the geometry.

/// Focus-move smoke: file opened in the FIRST (left) pane.
const FOCUS_FILE_LEFT: &str = "fone.md";

/// Heading written into [`FOCUS_FILE_LEFT`]; lowercased it contains
/// [`FOCUS_OCR_LEFT`].
const FOCUS_HEADING_LEFT: &str = "# FOCUSONE";

/// Lowercased substring OCR must find for the LEFT pane's file (split intact).
pub(crate) const FOCUS_OCR_LEFT: &str = "focusone";

/// Focus-move smoke: file opened in the SECOND (right) pane after `<C-w>v`.
const FOCUS_FILE_RIGHT: &str = "ftwo.md";

/// Heading written into [`FOCUS_FILE_RIGHT`]; lowercased it contains
/// [`FOCUS_OCR_RIGHT`].
const FOCUS_HEADING_RIGHT: &str = "# FOCUSTWO";

/// Lowercased substring OCR must find for the RIGHT pane's file (split intact).
pub(crate) const FOCUS_OCR_RIGHT: &str = "focustwo";

/// Distinctive marker typed (in insert mode) after focusing back LEFT. Its
/// lowercased form is [`FOCUS_OCR_MARKER`]. All-uppercase/short to survive OCR.
const FOCUS_MARKER: &str = "ZZLEFT";

/// Lowercased substring OCR must find to prove the post-`<C-w>h` insert landed in
/// the focused buffer.
pub(crate) const FOCUS_OCR_MARKER: &str = "zzleft";

// Close-pane smoke. The not-contains on `closegone` is the only proof the pane closed; the `:e`
// toast shows `cgone.md`, which cannot satisfy it. A failure is a finding, not a loosening.

/// Close-pane smoke: file opened in the FIRST (left) pane — the SURVIVOR.
const CLOSE_FILE_KEEP: &str = "ckeep.md";

/// Heading written into [`CLOSE_FILE_KEEP`]; lowercased it contains
/// [`CLOSE_OCR_KEEP`].
const CLOSE_HEADING_KEEP: &str = "# CLOSEKEEP";

/// Lowercased substring OCR MUST find: the survivor pane's file.
pub(crate) const CLOSE_OCR_KEEP: &str = "closekeep";

/// Close-pane smoke: file opened in the SECOND (right) pane — the one CLOSED.
const CLOSE_FILE_GONE: &str = "cgone.md";

/// Heading written into [`CLOSE_FILE_GONE`]; lowercased it contains
/// [`CLOSE_OCR_GONE`].
const CLOSE_HEADING_GONE: &str = "# CLOSEGONE";

/// Lowercased substring OCR must NOT find: the closed pane's file.
pub(crate) const CLOSE_OCR_GONE: &str = "closegone";

// Three-pane regression smoke: `tribtwo` is load-bearing, as binary nesting would blank pane B.
// See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#split-pane-smokes

/// Three-pane smoke: file opened FIRST (becomes the left pane).
const TRI_FILE_A: &str = "tri_a.md";

/// Heading written into [`TRI_FILE_A`]; lowercased it contains [`TRI_OCR_A`].
const TRI_HEADING_A: &str = "# TRIAONE";

/// Lowercased substring OCR must find for the FIRST/left pane's file.
pub(crate) const TRI_OCR_A: &str = "triaone";

/// Three-pane smoke: file opened in the RIGHT pane after the first `<C-w>v`. This
/// is the PRE-EXISTING right pane the pre-n-ary binary model blanked (load-bearing
/// assertion).
const TRI_FILE_B: &str = "tri_b.md";

/// Heading written into [`TRI_FILE_B`]; lowercased it contains [`TRI_OCR_B`].
const TRI_HEADING_B: &str = "# TRIBTWO";

/// Lowercased substring OCR must find for the RIGHT pane's file (the one the
/// pre-n-ary binary model collapsed to ~0 width).
pub(crate) const TRI_OCR_B: &str = "tribtwo";

/// Three-pane smoke: file opened in the new sub-pane after focusing left and the
/// second `<C-w>v` (the flat row becomes [A, C, B]).
const TRI_FILE_C: &str = "tri_c.md";

/// Heading written into [`TRI_FILE_C`]; lowercased it contains [`TRI_OCR_C`].
const TRI_HEADING_C: &str = "# TRICTHREE";

/// Lowercased substring OCR must find for the third (middle) pane's file.
pub(crate) const TRI_OCR_C: &str = "tricthree";

// `:q` smoke: `do_quit` closes the focused pane; mirrors `CLOSE_SCENARIO` with `:q<CR>`.
// `qgone.md` is unlike the heading word `quitgone`, so only a rendered buffer satisfies it.

/// `:q` smoke: file opened in the FIRST (left) pane — the SURVIVOR.
const QUIT_FILE_KEEP: &str = "qkeep.md";

/// Heading written into [`QUIT_FILE_KEEP`]; lowercased it contains
/// [`QUIT_OCR_KEEP`].
const QUIT_HEADING_KEEP: &str = "# QUITKEEP";

/// Lowercased substring OCR MUST find: the survivor pane's file.
pub(crate) const QUIT_OCR_KEEP: &str = "quitkeep";

/// `:q` smoke: file opened in the SECOND (right) pane — the one `:q` closes.
const QUIT_FILE_GONE: &str = "qgone.md";

/// Heading written into [`QUIT_FILE_GONE`]; lowercased it contains
/// [`QUIT_OCR_GONE`].
const QUIT_HEADING_GONE: &str = "# QUITGONE";

/// Lowercased substring OCR must NOT find: the closed pane's file.
pub(crate) const QUIT_OCR_GONE: &str = "quitgone";

// Ctrl+Shift+W smoke: the only end-to-end coverage of `UiAction::ClosePane`, the front-end path
// the `<C-w>c` and `:q` smokes bypass. Its own names and terms keep a failure to one smoke.

/// Ctrl+Shift+W smoke: file opened in the FIRST (left) pane — the SURVIVOR.
const CHORD_CLOSE_FILE_KEEP: &str = "wkeep.md";

/// Heading written into [`CHORD_CLOSE_FILE_KEEP`]; lowercased it contains
/// [`CHORD_CLOSE_OCR_KEEP`].
const CHORD_CLOSE_HEADING_KEEP: &str = "# CHORDKEEP";

/// Lowercased substring OCR MUST find: the survivor pane's file.
pub(crate) const CHORD_CLOSE_OCR_KEEP: &str = "chordkeep";

/// Ctrl+Shift+W smoke: file opened in the SECOND (right) pane — the one the chord
/// closes.
const CHORD_CLOSE_FILE_GONE: &str = "wgone.md";

/// Heading written into [`CHORD_CLOSE_FILE_GONE`]; lowercased it contains
/// [`CHORD_CLOSE_OCR_GONE`].
const CHORD_CLOSE_HEADING_GONE: &str = "# CHORDGONE";

/// Lowercased substring OCR must NOT find: the closed pane's file.
pub(crate) const CHORD_CLOSE_OCR_GONE: &str = "chordgone";

/// The `<linux/input-event-codes.h>` `BTN_MIDDLE` code, in the same grammar. Used by
/// [`native_tabs_open_switch_and_close_under_cage`] alone, to close a tab the way a
/// user does.
///
/// That middle-click closes an `adw::TabBar` tab was a PREDICTION from libadwaita's
/// documented default and is now MEASURED under `cage`: a probe injecting button 274
/// over a two-page bar dropped `n_pages` to 1 and fired `close-page`. Without that
/// measurement the smoke's leg (f) would have been dropped and the close glue would
/// have shipped with no end-to-end coverage at all.
const POINTER_BUTTON_MIDDLE: &str = "274";

/// The VERTICAL-SPLIT scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as `SCENARIO`): launch the app, wait until it is ready for input
/// (`wait_ready`), open file A (`:e $VSPLIT_FILE_LEFT<CR>`) in the editor pane, trigger
/// a vertical split with `<C-w>v`, open file B (`:e $VSPLIT_FILE_RIGHT<CR>`) in the new
/// pane, settle, and screenshot. Both files live in the cage process's working
/// directory (set via `Command::current_dir` by the caller) — NOT created from inside
/// this script.
const VSPLIT_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome vsplit smoke"

# Open file A in the (single) editor pane via an Ex command. The ':' enters the
# cmdline (rendered as the S3 bottom strip on grid 1); Return executes the :e.
wtype -- ":e $VSPLIT_FILE_LEFT"
wtype -k Return

# Let nvim load file A and redraw before splitting.
sleep "$SETTLE"

# Trigger a VERTICAL split with the <C-w>v chord (see the note near the
# VSPLIT_* consts about the exact wtype modifier syntax). The new pane opens to the
# RIGHT of the current one and initially shows the same buffer (file A).
wtype -M ctrl -k w -m ctrl
wtype -k v

# Let the shell build the second pane (bind the new nvim window/grid) and redraw.
sleep "$SETTLE"

# In the now-focused new (right) pane, replace its buffer with file B.
wtype -- ":e $VSPLIT_FILE_RIGHT"
wtype -k Return

# Let nvim load file B and both panes redraw before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The HORIZONTAL-SPLIT scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`VSPLIT_SCENARIO`]): launch the app, wait until it is ready for input
/// (`wait_ready`), open file A (`:e $HSPLIT_FILE_TOP<CR>`) in the editor pane, trigger
/// a HORIZONTAL split with `<C-w>s`, open file B (`:e $HSPLIT_FILE_BOTTOM<CR>`) in the
/// new (stacked) pane, settle, and screenshot. Both files live in the cage process's
/// working directory (set via `Command::current_dir` by the caller).
const HSPLIT_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome hsplit smoke"

# Open file A in the (single) editor pane via an Ex command. The ':' enters the
# cmdline (rendered as the S3 bottom strip on grid 1); Return executes the :e.
wtype -- ":e $HSPLIT_FILE_TOP"
wtype -k Return

# Let nvim load file A and redraw before splitting.
sleep "$SETTLE"

# Trigger a HORIZONTAL split with the <C-w>s chord (see the note near the
# HSPLIT_* consts about the exact wtype modifier syntax). The new pane stacks
# below/above the current one and initially shows the same buffer (file A).
wtype -M ctrl -k w -m ctrl
wtype -k s

# Let the shell build the second pane (bind the new nvim window/grid) and redraw.
sleep "$SETTLE"

# In the now-focused new pane, replace its buffer with file B.
wtype -- ":e $HSPLIT_FILE_BOTTOM"
wtype -k Return

# Let nvim load file B and both panes redraw before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The FOCUS-MOVE scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`VSPLIT_SCENARIO`]): launch the app, wait until it is ready for input
/// (`wait_ready`), open file A in the editor pane, vertical-split (`<C-w>v`, focus
/// moves to the new RIGHT pane), open file B there, move focus back LEFT (`<C-w>h`),
/// then type the marker (`i$FOCUS_MARKER<Esc>`) into the now-focused pane, settle, and
/// screenshot. Both files live in the cage process's working directory (set by the
/// caller).
const FOCUS_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome focus smoke"

# Open file A in the (single) editor pane.
wtype -- ":e $FOCUS_FILE_LEFT"
wtype -k Return

# Let nvim load file A and redraw before splitting.
sleep "$SETTLE"

# Vertical split: the new pane opens to the RIGHT and becomes focused.
wtype -M ctrl -k w -m ctrl
wtype -k v

# Let the shell build the second (right) pane and redraw.
sleep "$SETTLE"

# In the now-focused RIGHT pane, replace its buffer with file B.
wtype -- ":e $FOCUS_FILE_RIGHT"
wtype -k Return

# Let nvim load file B and both panes redraw.
sleep "$SETTLE"

# Move focus BACK to the LEFT pane with the <C-w>h chord (pure FocusDir intent ->
# shell neighbor -> nvim_set_current_win). See the note near the FOCUS_* consts.
wtype -M ctrl -k w -m ctrl
wtype -k h

# Let the focus change land (intent drained on wake, routing and the active pane updated).
sleep "$SETTLE"

# Type the distinctive marker into the now-focused (left) pane's buffer, then leave
# insert mode. The marker lands in whichever pane is focused.
wtype -- "i$FOCUS_MARKER"
wtype -k Escape

# Brief settle between keystroke steps so each is processed in order.
sleep "$SETTLE"

# Move the cursor OFF the marker's row so its inverted block does not sit atop (and
# OCR-distort) the asserted marker. After "i$FOCUS_MARKER<Esc>" the block cursor is
# parked on the marker's LAST char (ZZLEF[T]), which corrupts OCR (reads "zzlef}").
# As in the working SCENARIO, a normal-mode `o` opens a new line BELOW and enters
# insert on the now-empty line; Escape then parks the cursor there, leaving
# "$FOCUS_MARKER" fully unobscured on the previous line so OCR can match the whole
# word. The empty line left in the LEFT pane's buffer is harmless.
wtype -- o
wtype -k Escape

# Let the inserted text redraw before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The CLOSE-PANE scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`VSPLIT_SCENARIO`]): launch the app, wait until it is ready for input
/// (`wait_ready`), open file A in the editor pane, vertical-split (`<C-w>v`, focus
/// moves to the new RIGHT pane), open file B there, then close the focused (right) pane
/// with `<C-w>c`, settle, and screenshot. Both files live in the cage process's working
/// directory (set by the caller).
const CLOSE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome close smoke"

# Open file A (the SURVIVOR) in the (single) editor pane.
wtype -- ":e $CLOSE_FILE_KEEP"
wtype -k Return

# Let nvim load file A and redraw before splitting.
sleep "$SETTLE"

# Vertical split: the new pane opens to the RIGHT and becomes focused.
wtype -M ctrl -k w -m ctrl
wtype -k v

# Let the shell build the second (right) pane and redraw.
sleep "$SETTLE"

# In the now-focused RIGHT pane, open file B (the one we will CLOSE).
wtype -- ":e $CLOSE_FILE_GONE"
wtype -k Return

# Let nvim load file B and both panes redraw.
sleep "$SETTLE"

# Close the focused (right) pane with the <C-w>c chord. nvim performs the close
# (guarded against the last window); the shell collapses the pane from win_close.
wtype -M ctrl -k w -m ctrl
wtype -k c

# Let the close land: the right pane vanishes and the survivor reclaims the width.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The THREE-PANE scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`VSPLIT_SCENARIO`]): launch the app, wait until it is ready for input
/// (`wait_ready`), open file A, vertical-split (`<C-w>v`) and open file B in the new
/// RIGHT pane, focus back LEFT (`<C-w>h`), vertical-split AGAIN and open file C in the
/// new sub-pane — flattening into one row of three columns [A, C, B] under the n-ary
/// model. Park the cursor off the text (`o`+Escape) and screenshot. All three files
/// live in the cage process's working directory (set by the caller).
const TRISPLIT_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome three-pane smoke"

# Open file A in the (single) editor pane.
wtype -- ":e $TRI_FILE_A"
wtype -k Return

# Let nvim load file A and redraw before splitting.
sleep "$SETTLE"

# Vertical split: the new pane opens to the RIGHT and becomes focused (A|A).
wtype -M ctrl -k w -m ctrl
wtype -k v

# Let the shell build the second (right) pane and redraw.
sleep "$SETTLE"

# In the now-focused RIGHT pane, open file B (A|B).
wtype -- ":e $TRI_FILE_B"
wtype -k Return

# Let nvim load file B and both panes redraw.
sleep "$SETTLE"

# Move focus BACK to the LEFT pane with the <C-w>h chord.
wtype -M ctrl -k w -m ctrl
wtype -k h

# Let the focus change land before the second split.
sleep "$SETTLE"

# Vertical split the LEFT pane again. Under the n-ary frame model this flattens
# into one row of three columns; the new sub-pane sits between A and B => [A, C, B].
wtype -M ctrl -k w -m ctrl
wtype -k v

# Let the third pane build and all three redraw.
sleep "$SETTLE"

# In the now-focused new sub-pane, open file C (the middle column).
wtype -- ":e $TRI_FILE_C"
wtype -k Return

# Let nvim load file C and all three panes redraw.
sleep "$SETTLE"

# Park the cursor OFF row 0 (normal-mode `o` then Escape) so its inverted block
# does not sit atop and OCR-distort the asserted heading text in the focused pane.
wtype -- o
wtype -k Escape

# Let the redraw of all three panes land before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The `:q` CLOSE-PANE scenario, run as a single `sh -c` string INSIDE `cage` (mirrors
/// [`CLOSE_SCENARIO`] step for step): launch the app, wait until it is ready for input
/// (`wait_ready`), open file A in the editor pane, vertical-split (`<C-w>v`, focus
/// moves to the new RIGHT pane), open file B there, then close the focused pane with
/// the Ex command `:q` instead of `<C-w>c`, settle, and screenshot. Both files live in
/// the cage process's working directory (set by the caller).
const COLON_Q_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome :q smoke"

# Open file A (the SURVIVOR) in the (single) editor pane.
wtype -- ":e $QUIT_FILE_KEEP"
wtype -k Return

# Let nvim load file A and redraw before splitting.
sleep "$SETTLE"

# Vertical split: the new pane opens to the RIGHT and becomes focused.
wtype -M ctrl -k w -m ctrl
wtype -k v

# Let the shell build the second (right) pane and redraw.
sleep "$SETTLE"

# In the now-focused RIGHT pane, open file B (the one we will close).
wtype -- ":e $QUIT_FILE_GONE"
wtype -k Return

# Let nvim load file B and both panes redraw.
sleep "$SETTLE"

# Close the focused (right) pane with `:q`. init.lua's do_quit closes the PANE
# (nvim_win_close) whenever more than one window is open, and only fires the
# `close_window` command when it is the last one -- so this must NOT quit the app.
wtype -- ":q"
wtype -k Return

# Let the close land: the right pane vanishes and the survivor reclaims the width.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The `:qa` CLOSE-WINDOW scenario, run as a single `sh -c` string INSIDE `cage`:
/// launch the app, wait until it is ready for input (`wait_ready`), type `:qa<CR>`,
/// then POLL for the app process to disappear and REAP it to check how it went.
///
/// This is the only scenario that captures NO screenshot: its whole assertion is
/// that the process is gone and exited cleanly, which is a property of the process
/// table, not of any pixel. It therefore calls neither `grim` nor `tesseract`, and
/// contributes no `ASSERTED_OCR_TERMS` rows.
///
/// It asserts the app closed CLEANLY, not merely that it disappeared: after the
/// poll loop sees the process go, it `wait`s for the reaped child and checks the
/// status the shell retained. A panic or abort in the GTK main loop makes the
/// process vanish exactly as a clean shutdown does, so without that check this
/// scenario would pass green on the very regression its failure message blames.
///
/// Exit codes: `0` the app exited cleanly (success); `11` the app died before it
/// was ready for input; `12` the app was still alive after `QUIT_TRIES * WAIT_INTERVAL`
/// seconds — i.e. the drained `close_window` command never reached
/// `window.close()`; `13` the app exited but with a NON-ZERO status — i.e. the
/// shutdown path itself panicked or aborted.
const COLON_QA_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for the app to EXIT after :qa (15 s). Sized
# for the parallel-suite load the shutdown path (close-request -> picker/tree close
# -> WolfCore::shutdown -> nvim exit) runs under.
QUIT_TRIES=75

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome :qa smoke"

# Quit-all. init.lua's do_quit sees `all` and fires
# `vim.rpcnotify(chan, "wolf_command", "close_window")` (behind its E37 guard; the
# startup buffer is untouched here, so the guard does not trip). redraw.rs queues
# the command AND fires the wake, so the GTK drain runs promptly.
wtype -- ":qa"
wtype -k Return

# POLL (do not sleep-and-hope) for the process to actually go away -- then REAP it
# and check its EXIT STATUS.
#
# The status check is load-bearing, not belt-and-braces. "The process is gone" is
# equally true of a clean shutdown and of a panic or abort in the GTK main loop, so
# without it this scenario would exit 0 on exactly the regression its failure
# message blames (an outstanding `core.borrow()` held across the dispatch while
# `wire_close`'s handler does `core.borrow_mut().take()` is a RefCell panic). The
# smoke would then prove only that the app died, not that it closed cleanly.
#
# `wait` can still report the status here: the shell reaps a background child and
# RETAINS its status until a `wait` asks for it -- that retention is also why
# `kill -0` starts failing in this loop at all (a still-unreaped child would linger
# as a zombie and keep answering `kill -0`). A clean run exits 0: `main` forwards
# `adw::Application::run_with_args`, which returns SUCCESS when the last window
# closes and the main loop ends normally.
tries=0
while [ "$tries" -lt "$QUIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        # `set -e` would abort the script on a non-zero status before we could
        # report it, so collect the status through `||` (exempt from `set -e`).
        rc=0
        wait "$app_pid" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "wolf-gnome :qa smoke: app (pid $app_pid) exited $rc, not 0." >&2
            echo "  It went away, but NOT cleanly: the shutdown path panicked" >&2
            echo "  or aborted rather than returning from the GTK main loop." >&2
            exit 13
        fi
        exit 0
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

echo "wolf-gnome :qa smoke: app (pid $app_pid) still alive after :qa" >&2
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 12
"#;

/// The Ctrl+Shift+W CLOSE-PANE scenario, run as a single `sh -c` string INSIDE `cage`
/// (mirrors [`CLOSE_SCENARIO`] step for step): launch the app, wait until it is ready
/// for input (`wait_ready`), open file A in the editor pane, vertical-split (`<C-w>v`,
/// focus moves to the new RIGHT pane), open file B there, then close the focused pane
/// with the Ctrl+Shift+W ACCELERATOR instead of `<C-w>c`, settle, and screenshot. Both
/// files live in the cage process's working directory (set by the caller).
const CHORD_CLOSE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome chord-close smoke"

# Open file A (the SURVIVOR) in the (single) editor pane.
wtype -- ":e $CHORD_CLOSE_FILE_KEEP"
wtype -k Return

# Let nvim load file A and redraw before splitting.
sleep "$SETTLE"

# Vertical split: the new pane opens to the RIGHT and becomes focused.
wtype -M ctrl -k w -m ctrl
wtype -k v

# Let the shell build the second (right) pane and redraw.
sleep "$SETTLE"

# In the now-focused RIGHT pane, open file B (the one we will CLOSE).
wtype -- ":e $CHORD_CLOSE_FILE_GONE"
wtype -k Return

# Let nvim load file B and both panes redraw.
sleep "$SETTLE"

# Close the focused (right) pane with the Ctrl+Shift+W accelerator. Wolf matches
# the chord in a CAPTURE-phase EventControllerKey (actions::action_for_key), so it
# never reaches Neovim as a key; it activates `win.close-pane`, which the shell
# turns into InputEvent::ClosePane -> nvim's `:close`.
#
# Note: `-M <mod>` / `-m <mod>` press / release a modifier and `-k <key>`
# presses+releases a key while the held modifiers apply, exactly as in the picker
# and new-buffer scenarios. Chord delivery is isolated to this single line.
wtype -M ctrl -M shift -k w -m shift -m ctrl

# Let the close land: the right pane vanishes and the survivor reclaims the width.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

// Per-pane statusbar fixtures: the asserted terms are file names, so the files sit outside the cwd.
// See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#per-pane-statusbar-fixtures

/// Per-pane statusbar smoke: the directory holding the two opened fixtures, a
/// SIBLING of the temp cwd and a child of the app's `HOME`.
///
/// It must carry none of the three asserted terms. It is drawn twice over — the
/// strip's second line is the display path, and the typed `:e` command line spells
/// the absolute path — so a directory named after an asserted term would satisfy that
/// term's assertion from a component the strip's FILENAME line never drew. The
/// seventh block of `restore_fixture_names_cannot_satisfy_their_own_assertions`
/// asserts that rather than leaving it to this comment.
pub(crate) const STATUS_FIXTURE_DIR: &str = "sbfixtures";

/// Per-pane statusbar smoke: the file opened in the FIRST pane.
///
/// Its **name** is the asserted term, the same deliberate exception
/// `TREE_FILE_NAME`, `OPENRECENT_TARGET_FILE`, `SIDEBARTOGGLE_FILE` and
/// `CD_TARGET_FILE` carry, and the reason this const is absent from
/// `RELAUNCH_FIXTURE_NAMES`.
pub(crate) const STATUS_FILE_A: &str = "statusalfa.md";

/// Per-pane statusbar smoke: the file opened in the SPLIT pane. See
/// [`STATUS_FILE_A`]; it is absent from `RELAUNCH_FIXTURE_NAMES` for the same
/// reason.
pub(crate) const STATUS_FILE_B: &str = "statusbravo.md";

/// Body written into BOTH statusbar fixtures.
///
/// Deliberately wordless and identical in the two files. These are the only exempt
/// fixtures in this binary whose CONTENT is rendered — the scenario opens both — so a
/// body carrying either filename would satisfy that file's assertion from the EDITOR
/// GRID with no strip drawn at all, which is `PICKER_FILE_NAME`'s failure reached
/// through content instead of through a name. The seventh block of
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` checks it against
/// every asserted term.
pub(crate) const STATUS_FIXTURE_BODY: &str = "# ZZCHROMEBODY";

/// Lowercased substring OCR MUST find: [`STATUS_FILE_A`]'s name, drawn by the FIRST
/// pane's status bar and by nothing else on the page.
///
/// The file lives outside the search root, so no sidebar row bears it; the cmdline
/// carries [`STATUS_OCR_MARKER`] by capture time, not an `:e` path; and Neovim draws
/// no statusline of its own (assertion 3).
pub(crate) const STATUS_OCR_A: &str = "statusalfa";

/// Lowercased substring OCR MUST find: [`STATUS_FILE_B`]'s name, drawn by the SPLIT
/// pane's status bar. See [`STATUS_OCR_A`] for why nothing else can draw it.
pub(crate) const STATUS_OCR_B: &str = "statusbravo";

/// Lowercased substring OCR MUST find: the text the scenario `:echo`s just before
/// `grim` fires.
///
/// It does TWO jobs, and only the first is a feature claim about anything:
///
/// * it replaces the `:e $STATUS_FILE_B` toast in the cmdline strip, which would
///   otherwise put [`STATUS_OCR_B`] on the page with no status bar drawn at all;
/// * it is this capture's PROOF OF LIFE — the window was still painting when `grim`
///   ran — which is why assertion 4 is a precondition with no revert.
pub(crate) const STATUS_OCR_MARKER: &str = "statusready";

/// Name of the file the captive Neovim writes its `winheight(0)` and `&laststatus`
/// into. It lands beside the temp cwd rather than inside it, outside the search root,
/// so no sidebar row bears it.
///
/// Registered in `RELAUNCH_FIXTURE_NAMES` anyway, exactly as
/// `CD_READBACK_NAME` is: its ABSOLUTE path is echoed in the typed
/// `:call writefile(...)` command line, so a readback renamed to carry an asserted
/// term would put that term on the page by a route the sidebar rule does not cover.
pub(crate) const STATUS_READBACK_NAME: &str = "status_winheight.txt";

/// The exact `winheight(0)` the focused pane must report once the chrome is
/// subtracted — the ONLY end-to-end proof that the editor inset and the statusbar
/// strip reach a pixel.
///
/// MEASURED 2026-08-04 on a **1280x720** cage output at `WOLF_FONT_PT` =
/// [`SMOKE_FONT_PT`]: **11** rows, read off the smoke's own failure against a `-1`
/// placeholder. The un-inset grid was 14 rows at the same font on the same output,
/// so the chrome costs three whole rows.
///
/// It is derived from a RUN, never computed: the placeholder is the procedure, exactly
/// as it is for [`PANE_ORIGIN_X_EXPECTED`] and [`PANE_ORIGIN_Y_EXPECTED`]. Put `-1`
/// back, read the number the assertion message reports Neovim answered, and paste it
/// here with the date and the cage output size. Do NOT compute it from the statusbar
/// constants — a value derived from the constants the production code reads holds for
/// whatever those constants become, which is the one thing this assertion exists to
/// rule out.
///
/// It is not slack and it has no band: it is an exact equality, because the pointer
/// smokes' x tolerance (`PANE_ORIGIN_TOLERANCE_PX + cell_w`) is wider than the 20 px
/// the padding moves the pane origin by, so without an exact row count nothing in
/// this binary could tell the padding apart from no padding.
const STATUS_EXPECTED_WINHEIGHT: i64 = 11;

/// The PER-PANE STATUSBAR scenario, run as a single `sh -c` string inside `cage` (the
/// [`VSPLIT_SCENARIO`] shape): launch + poll, settle, `:e` file A by ABSOLUTE path,
/// settle, `<C-w>v`, settle, `:e` file B in the new pane, settle, park the block
/// cursor, read `winheight(0)` and `&laststatus` back, `:echo` the marker, settle,
/// screenshot.
///
/// ## Why the paths are absolute
///
/// Both fixtures live OUTSIDE the app's search root (see the block comment above), so
/// the tree sidebar cannot draw either name and the strips are the only source. A
/// relative `:e` would have to put them back in the cwd, at which point both
/// assertions are satisfiable by a sidebar row with no strip on screen — the exact
/// silent pass `PICKER_FILE_NAME`'s note records.
///
/// ## Why the `:echo` is LAST
///
/// The cmdline strip paints the last message, and `:e <path>` echoes the path it
/// opened. Between the second `:e` and `grim` there must therefore be one more
/// message, carrying neither filename. Anything typed after it that produces a
/// message of its own — including the mode message from an insert-mode park — puts
/// the ordering back at risk, which is why the park and the readback both run BEFORE
/// it and nothing but a `sleep` runs after.
///
/// ## The block-cursor park
///
/// The normal-mode `o` plus Escape is this binary's standing convention (the inverted
/// cursor block corrupts the cell it covers). Here it is convention rather than
/// protection — both asserted terms are STRIP text, which the editor's cursor never
/// sits on — and it doubles as corroboration that keys still reach Neovim after the
/// split.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention). A LAUNCH
///   failure; nothing below is about the status bar.
/// * `14` — the readback never appeared, i.e. the Ex command never executed. DISTINCT
///   from "the panes are the wrong size", which exits 0 and fails assertion 7.
const STATUS_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for the readback file.
FILE_TRIES=100

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome statusbar smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there. (The CD_SCENARIO body, verbatim.)
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
    echo "wolf-gnome statusbar smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# Open file A in the (single) editor pane, by ABSOLUTE path: it lives outside the
# app's search root on purpose, so no sidebar row can draw its name.
wtype -- ":e $STATUS_PATH_A"
wtype -k Return

# Let nvim load file A and redraw before splitting.
sleep "$SETTLE"

# Trigger a VERTICAL split with the <C-w>v chord (the VSPLIT_SCENARIO form). The new
# pane opens to the RIGHT of the current one, takes focus, and initially shows the
# same buffer (file A) -- which is exactly why file B has to be opened into it below
# before the two strips can differ.
wtype -M ctrl -k w -m ctrl
wtype -k v

# Let the shell build the second pane (bind the new nvim window/grid) and redraw.
sleep "$SETTLE"

# In the now-focused new (right) pane, replace its buffer with file B.
wtype -- ":e $STATUS_PATH_B"
wtype -k Return

# Let nvim load file B and both panes redraw.
sleep "$SETTLE"

# Park the block cursor off the buffer's first row, as every scenario here does.
wtype -- o
wtype -k Escape
sleep "$SETTLE"

# --- The readback: NEOVIM's own answer about the focused pane ----------------
#
# winheight(0) is the row count of the pane the front-end sized, which is the only
# place the editor inset and the statusbar strip reach a number a test can compare.
# &laststatus rides along because it is the premise the two OCR assertions rest on:
# a Neovim drawing its own statusline would put the filename inside the GRID, and
# both assertions would then hold with no strip painted at all.
#
# One printf, so both values describe one instant. The path is absolute and arrives
# as an env var, so it does not depend on Neovim's cwd -- and it lands outside the
# search root, so no sidebar row bears its name.
wtype -- ":call writefile([printf('%d %d', winheight(0), &laststatus)], '$STATUS_READBACK')"
wtype -k Return
wait_file "$STATUS_READBACK" || exit 14

# --- The marker, and it MUST be the last thing typed -------------------------
#
# The cmdline strip paints the last message. Without this, that message is the
# `:e $STATUS_PATH_B` toast, which spells file B's name -- so the assertion that the
# SPLIT pane's strip drew it would be satisfied by the cmdline with no status bar on
# screen at all. This also doubles as the capture's proof of life.
wtype -- ":echo '$STATUS_MARKER'"
wtype -k Return

# Let the redraw land before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

#[test]
fn vertical_split_shows_two_panes_under_cage() {
    const SMOKE: &str = "vertical_split_shows_two_panes_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target_left = cwd.join(VSPLIT_FILE_LEFT);
    let target_right = cwd.join(VSPLIT_FILE_RIGHT);
    std::fs::write(&target_left, format!("{VSPLIT_HEADING_LEFT}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write vsplit left file {}: {e}",
            target_left.display()
        )
    });
    std::fs::write(&target_right, format!("{VSPLIT_HEADING_RIGHT}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write vsplit right file {}: {e}",
            target_right.display()
        )
    });

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start from
    // a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_vsplit_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Two side-by-side panes near OCR's floor: adjust the window or font for this test only, never the
    // helper. See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#split-pane-smokes
    let status = cage_command(VSPLIT_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("VSPLIT_FILE_LEFT", VSPLIT_FILE_LEFT)
        .env("VSPLIT_FILE_RIGHT", VSPLIT_FILE_RIGHT)
        .status()
        .expect("failed to spawn `cage` for the headless vsplit smoke scenario");
    assert!(
        status.success(),
        "cage vsplit scenario exited unsuccessfully ({status}); the app may have failed to launch, \
         the window may not have mapped, the <C-w>v split chord may not have fired, the :e commands \
         may not have loaded, or wtype/grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage vsplit scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(VSPLIT_OCR_LEFT),
        "OCR did not contain {VSPLIT_OCR_LEFT:?} — the HEADING of {VSPLIT_FILE_LEFT}, which only \
         renders once that file is loaded into a PANE. A miss usually means the first pane stopped \
         rendering after the split; if NEITHER heading appears, suspect instead that the `:e` \
         commands never reached the cmdline (see the \"Cmdline visibility\" paragraph of \
         `notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#split-pane-smokes`). PNG kept at {} \
         for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        ocr_text.contains(VSPLIT_OCR_RIGHT),
        "OCR did not contain {VSPLIT_OCR_RIGHT:?} — the HEADING of {VSPLIT_FILE_RIGHT}, which only \
         renders once that file is loaded into a PANE. A miss usually means the <C-w>v chord did \
         not create a second pane, or :e did not open file B in it. PNG kept at {} for inspection. \
         OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // A pane's own edges carry no themed chrome, read from pixels: tesseract cannot tell colours apart
    // (see `crates/wolf-gnome/tests/README.md`). This capture already has the right page.
    let capture = decode_capture(&out_png, SMOKE, "the vertical-split capture");
    let (left_probe, right_probe) = pane_edge_probes();

    // The reference is the editor's own background, read from the right gutter below the statusbar
    // strip. See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#pane-edge-reference-colour
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let ref_x = pixel_index(
        output_w - PANE_EDGE_GUTTER_REF_INSET_PX,
        "the pane-edge reference column",
    );
    let ref_y = (right_probe.y0 + right_probe.y1) / 2;
    let editor_bg = capture.pixel(ref_x, ref_y);

    // Precondition, first half: the right probe is aimed at a pane's padding at all.
    assert!(
        capture.count_exactly(right_probe, editor_bg) > 0,
        "{SMOKE}: the RIGHT-edge probe (x {}..{}, y {}..{}) holds no pixel of the editor's own \
         background {editor_bg:?}, read at ({ref_x}, {ref_y}) inside what should be the same \
         pane's padding gutter. The probe is then not looking at the right pane's padding at all, \
         and the accent assertion below would hold for a window that never reached those columns \
         — vacuously, which is what this precondition exists to stop. Three causes, in the order \
         to check them on the retained PNG: the reference pixel landed on INK rather than \
         background; the window reaches past the reference column but not the output's right \
         edge, so something that is not a pane occupies the last {PANE_EDGE_PROBE_W_PX} px; or \
         the pane area's origin moved, so these rows are outside a pane. A window that falls \
         short of the reference column as well leaves this check green — the LEFT cross-check \
         below is what catches that one. RE-DERIVE the probe from CAGE_OUTPUT_W_EXPECTED, \
         CAGE_OUTPUT_H_EXPECTED and OVERLAY_ORIGIN_Y_EXPECTED; do not nudge it until it passes. \
         PNG kept at {}.",
        right_probe.x0,
        right_probe.x1,
        right_probe.y0,
        right_probe.y1,
        out_png.display()
    );

    // The reference must be the editor's background, not the compositor's; the left probe sits inside
    // the pane area, so a shrunken window's exterior colour holds none of its pixels.
    assert!(
        capture.count_exactly(left_probe, editor_bg) > 0,
        "{SMOKE}: the LEFT-edge probe (x {}..{}, y {}..{}) holds no pixel of {editor_bg:?}, \
         the colour read at ({ref_x}, {ref_y}) and taken here for the editor's own \
         background, while the RIGHT probe, checked above, holds it. Both panes fill \
         their whole allocation from the same `highlight 0`, and this probe's columns \
         start at OVERLAY_ORIGIN_X_EXPECTED, inside the pane area by construction — so \
         the reference is NOT the editor's background. The cause this half exists to \
         catch: the window does not extend to the output's right edge, the reference \
         column is outside it too, and the RIGHT probe matched only because both read \
         the COMPOSITOR's background — passing the check above while leaving the accent \
         assertion below vacuous. Failing that, the pane area's origin moved and these \
         columns are outside a pane. RE-DERIVE both probes from CAGE_OUTPUT_W_EXPECTED, \
         CAGE_OUTPUT_H_EXPECTED, OVERLAY_ORIGIN_X_EXPECTED and OVERLAY_ORIGIN_Y_EXPECTED; \
         do not nudge either until it passes. PNG kept at {}.",
        left_probe.x0,
        left_probe.x1,
        left_probe.y0,
        left_probe.y1,
        out_png.display()
    );

    // Subject: no pixel at either pane's own outer edge is the theme's accent.
    let accent_left = capture.count_exactly(left_probe, ADWAITA_ACCENT_RGB);
    let accent_right = capture.count_exactly(right_probe, ADWAITA_ACCENT_RGB);
    assert_eq!(
        accent_left + accent_right,
        0,
        "{SMOKE}: {accent_left} pixels at the LEFT pane's own left edge (x {}..{}) and \
         {accent_right} at the RIGHT pane's own right edge (x {}..{}), rows {}..{}, are exactly \
         GNOME's accent #3584e4 ({ADWAITA_ACCENT_RGB:?} decoded). A pane is painted identically \
         whether it is active or not: `draw_pane` fills the whole allocation from the snapshot's \
         highlight 0 and this front-end takes no colour from the GTK theme into a pane, so the \
         per-pane statusbar strip's TEXT is the only active-pane signal — bold at full strength \
         in the active pane, both lines scaled by STATUSBAR_INACTIVE_ALPHA in the others. \
         `notes/wolf-gnome/window-structure.md` records why, and what an edge treatment costs \
         beyond the look of it: it lands in the padding gutter `scrollbar.rs`'s thumb hides in, \
         and a long thin high-contrast mark beside a narrow pane's text is what \
         DIVIDER_BODY_LINE_COUNT records corrupting `tesseract --psm 11`'s scale estimate for a whole pane. A non-zero \
         count means themed chrome is being drawn inside the pane area — an outline, a border or \
         a tint on a pane's `DrawingArea` — rather than a colour that drifted, so do NOT turn \
         this into a threshold. PNG kept at {}.",
        left_probe.x0,
        left_probe.x1,
        right_probe.x0,
        right_probe.x1,
        right_probe.y0,
        right_probe.y1,
        out_png.display()
    );
}

#[test]
fn horizontal_split_shows_two_panes_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target_top = cwd.join(HSPLIT_FILE_TOP);
    let target_bottom = cwd.join(HSPLIT_FILE_BOTTOM);
    std::fs::write(&target_top, format!("{HSPLIT_HEADING_TOP}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write hsplit top file {}: {e}",
            target_top.display()
        )
    });
    std::fs::write(&target_bottom, format!("{HSPLIT_HEADING_BOTTOM}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write hsplit bottom file {}: {e}",
            target_bottom.display()
        )
    });

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start from
    // a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_hsplit_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Stacked panes near OCR's floor: adjust the window or font for this test only, never the helper.
    // See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#split-pane-smokes
    let status = cage_command(HSPLIT_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("HSPLIT_FILE_TOP", HSPLIT_FILE_TOP)
        .env("HSPLIT_FILE_BOTTOM", HSPLIT_FILE_BOTTOM)
        .status()
        .expect("failed to spawn `cage` for the headless hsplit smoke scenario");
    assert!(
        status.success(),
        "cage hsplit scenario exited unsuccessfully ({status}); the app may have failed to launch, \
         the window may not have mapped, the <C-w>s split chord may not have fired, the :e commands \
         may not have loaded, or wtype/grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage hsplit scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(HSPLIT_OCR_TOP),
        "OCR did not contain {HSPLIT_OCR_TOP:?} (the TOP pane's file). A miss usually means the \
         first pane stopped rendering after the split. PNG kept at {} for inspection. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        ocr_text.contains(HSPLIT_OCR_BOTTOM),
        "OCR did not contain {HSPLIT_OCR_BOTTOM:?} (the BOTTOM pane's file). A miss usually means \
         the <C-w>s split did not create a second pane, or :e did not open file B in it. PNG kept \
         at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
fn focus_move_returns_to_first_pane_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target_left = cwd.join(FOCUS_FILE_LEFT);
    let target_right = cwd.join(FOCUS_FILE_RIGHT);
    std::fs::write(&target_left, format!("{FOCUS_HEADING_LEFT}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write focus left file {}: {e}",
            target_left.display()
        )
    });
    std::fs::write(&target_right, format!("{FOCUS_HEADING_RIGHT}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write focus right file {}: {e}",
            target_right.display()
        )
    });

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start from
    // a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_focus_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Run the whole focus-move scenario inside cage, with the temp dir as the
    // working directory so the app picks it up as its cwd.
    let status = cage_command(FOCUS_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("FOCUS_FILE_LEFT", FOCUS_FILE_LEFT)
        .env("FOCUS_FILE_RIGHT", FOCUS_FILE_RIGHT)
        .env("FOCUS_MARKER", FOCUS_MARKER)
        .status()
        .expect("failed to spawn `cage` for the headless focus-move smoke scenario");
    assert!(
        status.success(),
        "cage focus-move scenario exited unsuccessfully ({status}); the app may have failed to \
         launch, the window may not have mapped, a chord (<C-w>v / <C-w>h) may not have fired, the \
         :e commands may not have loaded, or wtype/grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage focus-move scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    // Proves the focus-move wiring, not which pane got the marker: positional OCR is too fragile, and
    // `layout::neighbor` unit tests cover the geometry.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(FOCUS_OCR_MARKER),
        "OCR did not contain the inserted marker {FOCUS_OCR_MARKER:?}. A miss usually means the \
         post-<C-w>h insert did not reach a focused pane (focus-move plumbing broken). PNG kept at \
         {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        ocr_text.contains(FOCUS_OCR_LEFT),
        "OCR did not contain {FOCUS_OCR_LEFT:?} (the LEFT pane's file). A miss means the split did \
         not survive the focus move + insert. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        ocr_text.contains(FOCUS_OCR_RIGHT),
        "OCR did not contain {FOCUS_OCR_RIGHT:?} (the RIGHT pane's file). A miss means the split did \
         not survive the focus move + insert. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
fn close_pane_leaves_one_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target_keep = cwd.join(CLOSE_FILE_KEEP);
    let target_gone = cwd.join(CLOSE_FILE_GONE);
    std::fs::write(&target_keep, format!("{CLOSE_HEADING_KEEP}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write close keep file {}: {e}",
            target_keep.display()
        )
    });
    std::fs::write(&target_gone, format!("{CLOSE_HEADING_GONE}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write close gone file {}: {e}",
            target_gone.display()
        )
    });

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start from
    // a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_close_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Run the whole close-pane scenario inside cage, with the temp dir as the
    // working directory so the app picks it up as its cwd.
    let status = cage_command(CLOSE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("CLOSE_FILE_KEEP", CLOSE_FILE_KEEP)
        .env("CLOSE_FILE_GONE", CLOSE_FILE_GONE)
        .status()
        .expect("failed to spawn `cage` for the headless close-pane smoke scenario");
    assert!(
        status.success(),
        "cage close-pane scenario exited unsuccessfully ({status}); the app may have failed to \
         launch, the window may not have mapped, a chord (<C-w>v / <C-w>c) may not have fired, the \
         :e commands may not have loaded, or wtype/grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage close-pane scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    // Both are load-bearing; the close-pane scenario comment says why the not-contains is sound.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(CLOSE_OCR_KEEP),
        "OCR did not contain {CLOSE_OCR_KEEP:?} (the survivor pane's file). A miss usually means the \
         surviving pane stopped rendering after the close. PNG kept at {} for inspection. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        !ocr_text.contains(CLOSE_OCR_GONE),
        "OCR unexpectedly contained {CLOSE_OCR_GONE:?} (the closed pane's file). A hit usually means \
         <C-w>c did not remove the focused pane. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
fn three_panes_keep_all_files_visible_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target_a = cwd.join(TRI_FILE_A);
    let target_b = cwd.join(TRI_FILE_B);
    let target_c = cwd.join(TRI_FILE_C);
    std::fs::write(&target_a, format!("{TRI_HEADING_A}\n"))
        .unwrap_or_else(|e| panic!("failed to write tri file A {}: {e}", target_a.display()));
    std::fs::write(&target_b, format!("{TRI_HEADING_B}\n"))
        .unwrap_or_else(|e| panic!("failed to write tri file B {}: {e}", target_b.display()));
    std::fs::write(&target_c, format!("{TRI_HEADING_C}\n"))
        .unwrap_or_else(|e| panic!("failed to write tri file C {}: {e}", target_c.display()));

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start from
    // a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_trisplit_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Three side-by-side panes near OCR's floor: adjust the window or font for this test only.
    // See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#split-pane-smokes
    let status = cage_command(TRISPLIT_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("TRI_FILE_A", TRI_FILE_A)
        .env("TRI_FILE_B", TRI_FILE_B)
        .env("TRI_FILE_C", TRI_FILE_C)
        .status()
        .expect("failed to spawn `cage` for the headless three-pane smoke scenario");
    assert!(
        status.success(),
        "cage three-pane scenario exited unsuccessfully ({status}); the app may have failed to \
         launch, the window may not have mapped, a chord (<C-w>v / <C-w>h) may not have fired, the \
         :e commands may not have loaded, or wtype/grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage three-pane scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    // `tribtwo` is the load-bearing term: a binary-nest model blanks pane B.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(TRI_OCR_A),
        "OCR did not contain {TRI_OCR_A:?} (the FIRST/left pane's file). A miss usually means the \
         first pane stopped rendering after the splits. PNG kept at {} for inspection. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        ocr_text.contains(TRI_OCR_B),
        "OCR did not contain {TRI_OCR_B:?} (the pre-existing RIGHT pane's file). This is THE 3-pane \
         regression: the third split must NOT collapse/blank the right pane B. PNG kept at {} for \
         inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        ocr_text.contains(TRI_OCR_C),
        "OCR did not contain {TRI_OCR_C:?} (the new middle pane's file). A miss usually means the \
         second <C-w>v did not create a third pane, or :e did not open file C in it. PNG kept at {} \
         for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
fn colon_q_closes_focused_pane_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target_keep = cwd.join(QUIT_FILE_KEEP);
    let target_gone = cwd.join(QUIT_FILE_GONE);
    std::fs::write(&target_keep, format!("{QUIT_HEADING_KEEP}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write the :q survivor file {}: {e}",
            target_keep.display()
        )
    });
    std::fs::write(&target_gone, format!("{QUIT_HEADING_GONE}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write the :q closed-pane file {}: {e}",
            target_gone.display()
        )
    });

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start from
    // a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_colon_q_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Run the whole :q scenario inside cage, with the temp dir as the working
    // directory so the app picks it up as its cwd.
    let status = cage_command(COLON_Q_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("QUIT_FILE_KEEP", QUIT_FILE_KEEP)
        .env("QUIT_FILE_GONE", QUIT_FILE_GONE)
        .status()
        .expect("failed to spawn `cage` for the headless :q smoke scenario");
    assert!(
        status.success(),
        "cage :q scenario exited unsuccessfully ({status}); the app may have failed to launch, \
         the window may not have mapped, the <C-w>v split chord may not have fired, the :e \
         commands may not have loaded, or wtype/grim failed. A crash here can also mean `:q` \
         closed the WINDOW instead of the pane. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage :q scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(QUIT_OCR_KEEP),
        "OCR did not contain {QUIT_OCR_KEEP:?} (the survivor pane's file). A miss usually means \
         the surviving pane stopped rendering after the close — or that `:q` closed the whole \
         WINDOW (init.lua's do_quit must close the PANE while more than one window is open). PNG \
         kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        !ocr_text.contains(QUIT_OCR_GONE),
        "OCR unexpectedly contained {QUIT_OCR_GONE:?} (the closed pane's file). A hit usually \
         means `:q` did not remove the focused pane. PNG kept at {} for inspection. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
fn colon_qa_closes_the_window_under_cage() {
    // No screenshot is taken here, but the smoke env is still required: the scenario
    // launches under `cage` and drives it with `wtype`.
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd with no fixture files: nothing is opened, and the startup
    // buffer stays untouched so init.lua's E37 modified-buffer guard does not trip.
    let cwd = make_temp_cwd();

    // No PNG: the assertion is that the app process exited cleanly, which no pixel can show.
    let status = cage_command(COLON_QA_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .status()
        .expect("failed to spawn `cage` for the headless :qa smoke scenario");

    assert!(
        status.success(),
        "cage :qa scenario exited unsuccessfully ({status}). This is the load-bearing proof of \
         the WHOLE command drain: `:qa` -> init.lua's do_quit -> `wolf_command close_window` -> \
         redraw.rs queues it AND fires the wake -> `WolfCore::drain_commands` on the render wake \
         -> `UiAction::CloseWindow` -> `window.close()` -> clean `WolfCore::shutdown`. The \
         scenario reports WHICH half failed through its exit code; read it before anything else. \
         \n\
         Exit 11: the app died before it was ready for input. That is a LAUNCH failure, not a \
         drain failure — `:qa` was never even typed. \n\
         Exit 12: the app was STILL RUNNING after the poll budget, i.e. the drained \
         `close_window` never reached `window.close()`. Check that `shell.on_render_wake()` is \
         actually called from the wake closure (and called LAST, after panes and tree), and that \
         `UiAction::CloseWindow` calls `window.close()` rather than `window.destroy()`. \n\
         Exit 13: the app DID exit, but with a non-zero status — it went away without closing \
         cleanly, so the shutdown path itself panicked or aborted. The most likely culprit is an \
         outstanding `core.borrow()` held across the dispatch while `wire_close`'s handler does \
         `core.borrow_mut().take()`, which is a RefCell double-borrow panic; the scenario echoes \
         the raw status to stderr. This is the case the smoke was BLIND to before the status \
         check was added — a panic makes the process vanish exactly as a clean close does, so it \
         used to pass green. \n\
         A signal/abort status on the CAGE process itself (rather than one of the three above) \
         means the compositor died, which is an environment failure."
    );
}

#[test]
fn close_pane_chord_closes_focused_pane_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target_keep = cwd.join(CHORD_CLOSE_FILE_KEEP);
    let target_gone = cwd.join(CHORD_CLOSE_FILE_GONE);
    std::fs::write(&target_keep, format!("{CHORD_CLOSE_HEADING_KEEP}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write the Ctrl+Shift+W survivor file {}: {e}",
            target_keep.display()
        )
    });
    std::fs::write(&target_gone, format!("{CHORD_CLOSE_HEADING_GONE}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write the Ctrl+Shift+W closed-pane file {}: {e}",
            target_gone.display()
        )
    });

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start from
    // a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_chord_close_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Run the whole Ctrl+Shift+W close-pane scenario inside cage, with the temp dir
    // as the working directory so the app picks it up as its cwd.
    let status = cage_command(CHORD_CLOSE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("CHORD_CLOSE_FILE_KEEP", CHORD_CLOSE_FILE_KEEP)
        .env("CHORD_CLOSE_FILE_GONE", CHORD_CLOSE_FILE_GONE)
        .status()
        .expect("failed to spawn `cage` for the headless Ctrl+Shift+W close-pane smoke scenario");
    assert!(
        status.success(),
        "cage Ctrl+Shift+W close-pane scenario exited unsuccessfully ({status}); the app may have \
         failed to launch, the window may not have mapped, the <C-w>v split chord may not have \
         fired, the :e commands may not have loaded, or wtype/grim failed. Screenshot expected at \
         {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage Ctrl+Shift+W close-pane scenario, \
             but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    // Both assertions are load-bearing; the CHORD_CLOSE_* comment says why the not-contains is sound.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(CHORD_CLOSE_OCR_KEEP),
        "OCR did not contain {CHORD_CLOSE_OCR_KEEP:?} (the survivor pane's file). A miss usually \
         means the surviving pane stopped rendering after the close — or, if OCR shows NEITHER \
         heading, that Ctrl+Shift+W was forwarded to Neovim instead of being captured, in which \
         case `<C-w>` started a window command and `W` moved the cursor. PNG kept at {} for \
         inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        !ocr_text.contains(CHORD_CLOSE_OCR_GONE),
        "OCR unexpectedly contained {CHORD_CLOSE_OCR_GONE:?} (the closed pane's file). A hit \
         means the Ctrl+Shift+W chord did not remove the focused pane. Check, in order: \
         `actions::action_for_key` matched the delivered keyval (a real keymap sends the SHIFTED \
         glyph `W`, so the letter comparison must be case-insensitive — \
         `actions::tests::action_for_key_table` pins that), the `win.close-pane` action is \
         installed and enabled, and the shell's `UiAction::ClosePane` arm sends \
         `InputEvent::ClosePane`. If `colon_q_closes_focused_pane_under_cage` and \
         `close_pane_leaves_one_under_cage` both pass while this fails, the nvim-side close works \
         and the break is in the front-end chord path. PNG kept at {} for inspection. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
}

// A pane's own edge carries no themed chrome: constants and a helper for the vertical-split smoke,
// which reads `Capture` pixels because a colour claim needs them.

/// GNOME's accent colour, `#3584e4`, which no pixel at a pane's own edge may carry.
///
/// The pane area takes NO colour from the GTK theme. `pane_view::draw_pane` fills the
/// whole allocation — the statusbar strip's band and all four padding gutters included —
/// from highlight 0 of the Neovim snapshot, and every fill after it comes from the same
/// snapshot. So this is not merely a colour that happens to be absent: it is the value
/// themed chrome painted at a pane's edge lands on, and the one value that tells "the
/// editor drew this" apart from "the theme drew this".
///
/// Exact equality rather than a distance, because a solid CSS outline or border is
/// axis-aligned and opaque and hits the literal value. A tolerance would start matching
/// the editor's own blues instead.
const ADWAITA_ACCENT_RGB: Rgb8 = (0x35, 0x84, 0xe4);

/// How deep into a pane, from its own left or right edge, [`pane_edge_probes`] reads.
///
/// Four pixels, and the number carries a reason rather than a margin: a 2 px treatment
/// offset 2 px inward lands on the pane's last four columns whichever way the offset
/// rounds, so four columns catch it without the probe modelling the rounding. Four also
/// stays well outside the scrollbar thumb, whose left edge sits `SCROLLBAR_THICKNESS_PX`
/// plus `SCROLLBAR_MARGIN_PX` — 14 px — in from the pane's right edge: a deeper probe
/// would read a grey mark whose presence depends on whether the buffer overflows its
/// window, which is not this assertion's subject.
const PANE_EDGE_PROBE_W_PX: f64 = 4.0;

/// How far below the pane area's own top edge [`pane_edge_probes`] starts.
///
/// One `editor_inset::EDITOR_PADDING_PX` gutter, so neither probe reads the CORNER where
/// a pane's top edge meets its side. At a corner a treatment along the top and one along
/// the side are the same pixels, and a probe that included one could not say which edge
/// it had found.
const PANE_EDGE_PROBE_TOP_INSET_PX: f64 = wolf_gnome::editor_inset::EDITOR_PADDING_PX;

/// How far in from the OUTPUT's right edge the reference background pixel is read.
///
/// Eighteen puts it inside the right pane's own 20 px `EDITOR_PADDING_PX` gutter, which
/// `draw_pane` fills with the editor's background along with the rest of the allocation,
/// and four pixels left of the scrollbar thumb's left edge at 14 px in.
///
/// It sits OUTSIDE [`pane_edge_probes`]' right probe deliberately. A reference read from
/// inside the probe would be whatever the probe happens to hold, so the precondition it
/// feeds would hold however the pane was painted — including with the probe covered edge
/// to edge by something that is not the editor's background at all.
const PANE_EDGE_GUTTER_REF_INSET_PX: f64 = 18.0;

/// The two rectangles [`vertical_split_shows_two_panes_under_cage`] reads for themed
/// chrome, as `(LEFT, RIGHT)`: the LEFT pane's own left edge first, the RIGHT pane's own
/// right edge second, in the order of the x each is derived from. A same-typed pair is
/// the shape a later edit transposes silently, which is why the order is written down
/// here and why the call site binds both halves by name.
///
/// TWO probes rather than one, because which pane is ACTIVE is not the subject. An edge
/// treatment follows the focus, and this scenario's focus sits in the right pane only by
/// construction (`<C-w>v` focuses the new pane and `:e` file B lands there). Reading both
/// outer edges makes the assertion hold whichever pane is active, and neither probe has
/// to know where the divider is — the `gtk::Paned` handle's width is theme-owned and this
/// file models it nowhere.
///
/// Both span rows from one [`PANE_EDGE_PROBE_TOP_INSET_PX`] below the pane area's top
/// edge down to the output's vertical midpoint. The midpoint rather than the bottom keeps
/// them clear of the window's full-width command-line strip, which belongs to no pane.
///
/// The two are the same width and share their rows by construction, so neither RECTANGLE
/// can go quietly degenerate while the other holds: a non-empty right probe is a non-empty
/// left one. That is a fact about the geometry alone and says nothing about what either
/// probe holds, which is why the precondition below reads both for a colour.
///
/// Derived at the pinned output size: LEFT `x 251..255`, RIGHT `x 1276..1280`, both
/// `y 67..360`.
///
/// MEASURED 2026-08-15 on a **1280x720** cage output, from the PNG this smoke retains: each
/// probe holds 1172 pixels of ONE distinct colour, `(20, 22, 27)` — 4 columns across all 293
/// rows — and the reference pixel at `(1262, 213)`, inside the right pane's padding gutter,
/// reads that same value. The outermost four columns are therefore the RIGHT pane's own
/// padding, and the four at [`OVERLAY_ORIGIN_X_EXPECTED`] the LEFT pane's: that colour is
/// the editor's background out of the Neovim snapshot, and the theme paints nothing there.
///
/// The precondition re-establishes it per run, and it reads BOTH probes for that colour,
/// because each establishes what the other cannot. The RIGHT probe is the side no measured
/// constant pins, so it is the one that says the outermost columns are a pane's padding at
/// all. The LEFT probe's x is [`OVERLAY_ORIGIN_X_EXPECTED`], measured and cross-checked in
/// its own doc comment, so it says instead what the REFERENCE is: a colour read off the
/// exterior of a window that falls short of the output's right edge holds no pixel in those
/// columns. Together they exclude the reading neither excludes alone — reference and right
/// probe both outside the window, both holding the compositor's background, agreeing with
/// each other while the subject assertion below runs vacuously.
///
/// The single colour is a reading of this page rather than a property to assert. `draw_pane`
/// clips the statusbar strip vertically and not horizontally, so a long secondary line can
/// reach these columns with no themed chrome present anywhere; the precondition's job is
/// non-vacuity, and a full-uniformity check would report that text as an edge treatment.
///
/// The subject has ink to find: a 2 px treatment at `outline-offset: -2px` covers two of the
/// four columns on every row, and adding such a rule reads 586 accent pixels in the right
/// probe — half of 1172 — against 0 in the left, the right pane being the active one here.
///
/// If the precondition fails, move the probe inward or narrow [`PANE_EDGE_PROBE_W_PX`] to
/// 2; never delete the precondition, and never soften the subject assertion into a
/// tolerance.
fn pane_edge_probes() -> (SampleRect, SampleRect) {
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    let y0 = pixel_index(
        OVERLAY_ORIGIN_Y_EXPECTED + PANE_EDGE_PROBE_TOP_INSET_PX,
        "the pane-edge probes' first row",
    );
    let y1 = pixel_index(output_h / 2.0, "the pane-edge probes' last row");
    let left = SampleRect {
        x0: pixel_index(OVERLAY_ORIGIN_X_EXPECTED, "the left probe's first column"),
        x1: pixel_index(
            OVERLAY_ORIGIN_X_EXPECTED + PANE_EDGE_PROBE_W_PX,
            "the left probe's last column",
        ),
        y0,
        y1,
    };
    let right = SampleRect {
        x0: pixel_index(
            output_w - PANE_EDGE_PROBE_W_PX,
            "the right probe's first column",
        ),
        x1: pixel_index(output_w, "the right probe's last column"),
        y0,
        y1,
    };
    (left, right)
}

/// Each pane's status bar draws THAT pane's filename, and the chrome it sits in
/// costs the grid real rows.
///
/// ## What it covers that nothing else does
///
/// Three things, and the third is the one no other smoke can reach:
///
/// * that a per-pane status bar is painted at all — `draw_pane`'s two
///   `render::draw_text_line` calls, over the `PaneStatus` cached on the wake;
/// * that the two strips carry DIFFERENT filenames, i.e. that the paint reads the
///   status of the pane it is painting rather than of the active one. That is what
///   the crossed fixture pair buys, and it is the only property here a "the feature
///   is roughly working" capture could not fake;
/// * that the editor inset and the strip's height reach the GRID SIZE, through the
///   captive Neovim's own `winheight(0)`. Nothing else in this binary can see the
///   padding: the pointer smokes' origin band is `PANE_ORIGIN_TOLERANCE_PX + cell_w`
///   wide, which is wider than the 20 px the padding moves the pane origin by, so
///   without an exact row count `EDITOR_PADDING_PX` would be table-tested and
///   nothing more.
///
/// ## The premises, and where each is pinned
///
/// The two OCR assertions are about FILENAMES, which inverts this binary's usual
/// fixture rule. Everything that makes that safe is stated on the `STATUS_*` consts
/// and asserted rather than assumed: the fixtures live outside the search root
/// (assertion 0), the cmdline carries the marker rather than an `:e` path by capture
/// time (assertion 4 reads it), and Neovim draws no statusline of its own
/// (assertion 3 asks the captive Neovim, rather than trusting a `runtime/nvim`
/// setting this test does not own).
/// `restore_fixture_names_cannot_satisfy_their_own_assertions`'s seventh block
/// covers the naming half mechanically.
///
/// ## What this smoke does NOT prove
///
/// The active/inactive treatment. `statusbar_layout::statusbar_text` pins both alphas
/// and the bold flag as values, and `draw_pane` demonstrably reads `active_pane`, but
/// OCR reads neither font weight nor a 50 % dim — both strips are legible either way,
/// which is exactly why both are asserted PRESENT here. Same shape as the sidebar's
/// current-file bold, recorded in `tests/README.md`.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. The `measured` column is from an actual red run, per
/// `tests/README.md`'s "A red-run demonstration is a deliverable, not a formality".
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 0 | the fixture directory and the readback sit OUTSIDE the app's search root | **precondition** — makes the anti-vacuity argument executable instead of prose: inside the root, the tree sidebar would draw both filenames and assertions 5 and 6 would hold with no status bar painted at all | none; do not invent one | n/a |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | n/a |
/// | 2 | [`assert_png_present`] | precondition — grim produced an image | none; do not invent one | n/a |
/// | 3 | the `&laststatus` readback is `0` | **precondition** — Neovim draws no statusline of its own, so a filename on the page came from our strip. Asked of the CAPTIVE NEOVIM rather than read off `runtime/nvim/init.lua`, because that file is a premise this test does not control | none; do not invent one | n/a |
/// | 4 | OCR contains [`STATUS_OCR_MARKER`] | **precondition, not a discriminator** — the capture's proof of life. Its absence means the capture is stale or the cmdline stopped painting, neither of which is the feature under test | none; do not invent one | n/a |
/// | 5 | OCR contains [`STATUS_OCR_A`] | **the discriminator for PER-PANE-ness** | in `draw_pane`, look the cached status up by `self.active_pane.get()` instead of by the `pane_id` being painted — a one-line change. Both strips then read `statusbravo.md`, so this row goes red while row 6 stays GREEN, which is the only revert here that discriminates per-pane-ness rather than presence. | RED: `OCR did not contain "statusalfa" — the name of statusalfa.md, which is open in the FIRST (left) pane and can be drawn only by that pane's status bar. Assertion 4 is GREEN`. **The panic alone does not carry the claim; the TERM COUNTS in that run's dump do: `statusbravo` 3, `statusalfa` 0, `statusready` 1.** Assertion 6's term survives on the page — twice over, as this row's own message tells the reader to check for — while assertion 5's is gone. That asymmetry is what makes the failure discriminate per-pane-ness rather than mere presence. |
/// | 6 | OCR contains [`STATUS_OCR_B`] | **discriminator for the strip rendering at all**, jointly with row 5 | delete the two `render::draw_text_line` calls from `draw_pane`. Rows 5 and 6 both go red; 5 fails first, so this row has no isolated red run under it. | RED, and the panic NAMES ASSERTION 5 — the test is fail-fast and this row has no isolated red run, exactly as the revert column says. **Its evidence is again the term counts: `statusready` 1, and NOTHING else — neither `statusalfa` nor `statusbravo` is anywhere on the page.** That is what separates this revert from row 5's, where `statusbravo` survived: there one strip still drew, here neither does. |
/// | 7 | `winheight(0)` equals [`STATUS_EXPECTED_WINHEIGHT`] | **the discriminator for the editor inset and the strip's height** — the only end-to-end proof either reaches a pixel | delete the `- 2 * padding` term from `editor_inset::grid_rect`'s `h` (one more row survives) or the `statusbar_h` term (two more) | RED under BOTH reverts, with assertions 4, 5 and 6 green under each, so both are observed here as a changed row count. **Three distinct values, which is what proves this assertion separates the two terms rather than merely noticing that the chrome changed: baseline 11; without `statusbar_h`, `the focused pane reports winheight(0) = 13`; without `- 2 * padding`, `= 12`.** Both messages read `expected -1` because the constant was still its placeholder when they were taken — the evidence is the REPORTED `winheight(0)`, which the constant since being pinned to 11 does not affect, so neither revert needs re-running. Against the pinned 11 they would read `left: 13` and `left: 12` against `right: 11`. |
#[test]
fn pane_status_bar_shows_each_panes_filename_under_cage() {
    const SMOKE: &str = "pane_status_bar_shows_each_panes_filename_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // The launch cwd holds only fillers, so neither asserted file name can come from a sidebar row.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    // The fixtures sit in the cwd's sibling, under HOME and outside the search root.
    // See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#per-pane-statusbar-fixtures
    let fixture_dir = smoke_home(&cwd).join(STATUS_FIXTURE_DIR);
    std::fs::create_dir_all(&fixture_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the statusbar fixture dir {}: {e}",
            fixture_dir.display()
        )
    });

    let path_a = fixture_dir.join(STATUS_FILE_A);
    let path_b = fixture_dir.join(STATUS_FILE_B);
    for path in [&path_a, &path_b] {
        std::fs::write(path, format!("{STATUS_FIXTURE_BODY}\n")).unwrap_or_else(|e| {
            panic!(
                "{SMOKE}: failed to write the statusbar fixture {}: {e}",
                path.display()
            )
        });
    }

    // Beside the cwd like the fixtures; its absolute path is echoed in the typed `writefile` line.
    let readback = smoke_home(&cwd).join(STATUS_READBACK_NAME);
    let out_png = runtime_dir.join("wolf_gnome_statusbar_smoke.png");
    for path in [&readback, &out_png] {
        clear_evidence(path, SMOKE);
    }

    // Assertion 0 (precondition): the fixtures are outside the search root. That rests on the shared
    // `make_temp_cwd` and `smoke_home`, so a change to either surfaces here.
    for (path, what) in [
        (&fixture_dir, "the fixture directory"),
        (&readback, "the readback file"),
    ] {
        assert!(
            !path.starts_with(&cwd),
            "{SMOKE}: {what} {} is INSIDE the app's working directory {}, which is its file-search \
             root. The tree sidebar renders every entry under that root BY NAME, so \
             {STATUS_OCR_A:?} and {STATUS_OCR_B:?} would be on the page as sidebar rows and \
             assertions 5 and 6 would hold with no status bar painted at all — a permanent silent \
             pass, not a flake. Put them back beside the cwd (`smoke_home(&cwd).join(...)`); do NOT \
             weaken the assertions.",
            path.display(),
            cwd.display()
        );
    }

    let status = cage_command(STATUS_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("STATUS_PATH_A", &path_a)
        .env("STATUS_PATH_B", &path_b)
        .env("STATUS_READBACK", &readback)
        .env("STATUS_MARKER", STATUS_OCR_MARKER)
        .status()
        .expect("failed to spawn `cage` for the headless per-pane statusbar smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage per-pane statusbar scenario exited unsuccessfully ({status}). Read the \
         exit code FIRST; each names a different half of the run.\n\
         Exit 11: the app died before it was ready for input. A LAUNCH failure — nothing below is \
         about the status bar.\n\
         Exit 14: the `winheight(0)` readback never appeared, which means the Ex command never \
         executed: keys stopped reaching Neovim, or the <C-w>v split left focus somewhere \
         unexpected. That is DISTINCT from \"the panes are the wrong size\", which exits 0 and \
         fails assertion 7 instead.\n\
         Any other status means wtype or grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // --- Assertion 2 (precondition): grim produced an image -------------------
    assert_png_present(SMOKE, &out_png);

    // Assertion 3 (precondition): `laststatus` is 0, or Neovim draws the name in the grid and both OCR
    // assertions pass with no strip. `init.lua` is not this test's to control, so Neovim is asked.
    let readings = read_ints(&readback, 2, SMOKE);
    let (got_winheight, got_laststatus) = (readings[0], readings[1]);
    assert_eq!(
        got_laststatus, 0,
        "{SMOKE}: the captive Neovim reports `&laststatus` = {got_laststatus}, not 0. It then \
         draws its OWN status line inside each window's grid, carrying the buffer name — so \
         assertions 5 and 6 below would be satisfied by Neovim's chrome rather than by this \
         front-end's status bar, and would keep passing with `draw_pane`'s text calls deleted. \
         This is a PRECONDITION about the shared `runtime/nvim/init.lua`, not a front-end failure; \
         read it as \"assertions 5 and 6 cannot be trusted this run\"."
    );

    let ocr_text = ocr_to_lowercase(&out_png);

    // Assertion 4 (precondition, no revert): the marker proves the window painted and displaces the
    // `:e` toast that would draw assertion 6's term. Nothing here can break it; invent no revert.
    assert!(
        ocr_text.contains(STATUS_OCR_MARKER),
        "{SMOKE}: OCR did not contain {STATUS_OCR_MARKER:?} — the text the scenario `:echo`s \
         immediately before the capture, which the cmdline strip paints as the last message. This \
         is a PRECONDITION with no revert of ours: its absence means the capture is stale, the \
         cmdline stopped painting, or `:echo` under `ext_messages` produced no visible toast at \
         all (in which case the fix is a different Ex command with a deterministic message — never \
         a weaker assertion). Read this red as \"assertions 5 and 6 below cannot be trusted this \
         run\": without the marker the cmdline may still be showing the `:e` path, which spells \
         {STATUS_OCR_B:?}. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 5 (discriminator): only the left pane's own strip can draw `statusalfa.md`.
    // See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#per-pane-statusbar-reverts
    assert!(
        ocr_text.contains(STATUS_OCR_A),
        "{SMOKE}: OCR did not contain {STATUS_OCR_A:?} — the name of {STATUS_FILE_A}, which is \
         open in the FIRST (left) pane and can be drawn only by that pane's status bar. Assertion \
         4 is GREEN, so the window was painting and the cmdline is showing the marker rather than \
         an `:e` path. The candidate causes, in order:\n\
         (a) EVERY STRIP IS DRAWING THE ACTIVE PANE'S STATUS. That is the documented revert for \
         this row, and it leaves assertion 6 green exactly as you see it — check the dump below \
         for {STATUS_OCR_B:?} appearing TWICE.\n\
         (b) no strip is drawn at all, in which case assertion 6 is red too and the cause is \
         `draw_pane`'s text calls rather than which status they read.\n\
         (c) the left pane's cached `PaneStatus` was never refreshed, so it still holds the \
         `[No Name]` it was built with — the dump then carries `no name` where this term belongs.\n\
         (d) the strip is drawn but clipped away, which would need the grid rect and the strip \
         rect to disagree; assertion 7 is the one that would also move.\n\
         PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 6: with 5, proves a strip is painted; alone, names the surviving strip's file. No
    // isolated red: deleting `draw_pane`'s text calls reddens 5 first.
    assert!(
        ocr_text.contains(STATUS_OCR_B),
        "{SMOKE}: OCR did not contain {STATUS_OCR_B:?} — the name of {STATUS_FILE_B}, which the \
         scenario opened into the pane the <C-w>v split created. With assertion 5 GREEN the split \
         pane's strip is the only thing missing, so the likely causes are that the split never \
         happened (one pane, one strip), that the second `:e` did not land in the new pane, or \
         that the new pane's cached status was never refreshed after the layout rebuild. With \
         assertion 5 RED as well, no strip is being painted at all and this row adds nothing to \
         that diagnosis. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 7 (discriminator): the only exact reading of `EDITOR_PADDING_PX` and the strip height.
    // See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#per-pane-statusbar-reverts
    assert_eq!(
        got_winheight, STATUS_EXPECTED_WINHEIGHT,
        "{SMOKE}: the focused pane reports winheight(0) = {got_winheight}, expected \
         {STATUS_EXPECTED_WINHEIGHT}. If STATUS_EXPECTED_WINHEIGHT is still the -1 placeholder \
         this failure is EXPECTED and is how the value is derived: record {got_winheight} in that \
         const together with today's date and the cage output size, exactly as PANE_ORIGIN_X_EXPECTED \
         and PANE_ORIGIN_Y_EXPECTED were derived, and do NOT compute it from the statusbar \
         constants. Otherwise the chrome changed size. A HIGHER number means the grid was given \
         back space the chrome is supposed to take — one extra row per missing `padding` term, \
         several per missing `statusbar_h` term — and a LOWER one means something is subtracted \
         twice. Assertions 5 and 6 stay green under all of those, which is why this row is the \
         only end-to-end proof the inset and the strip height exist at all. PNG kept at {}.",
        out_png.display()
    );
}

// Native tabs: two `adw::TabPage`s over one Neovim, switched and closed with real pointer events.
// See notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md#native-tabs-smoke

/// Native-tabs smoke: the file opened in the FIRST tab.
///
/// Its name carries no asserted term — the CONTENT is what every OCR assertion is
/// about — so the tree sidebar's row for it, the `:e` echo that spells it, and the
/// tab bar's own title (which is this name) can satisfy nothing. That is the
/// standing convention here and it matters more than usual, because the tab bar
/// draws BOTH filenames in the very captures whose discriminating half is a
/// must-NOT-contain.
pub(crate) const TABS_FIRST_FILE: &str = "tbxray.md";

/// Heading written into [`TABS_FIRST_FILE`]; lowercased it contains
/// [`TABS_OCR_FIRST`].
pub(crate) const TABS_FIRST_HEADING: &str = "# ZZTABALFA";

/// Lowercased substring OCR MUST find in captures A and C, and must NOT find in
/// capture B.
///
/// The ONLY thing on the page that can draw it is tab 1's own editor grid: no
/// filename contains it (the guard enforces that for every fixture in this binary),
/// and the other tab's heading does not either (the crossed pair in
/// `restore_fixture_names_cannot_satisfy_their_own_assertions`).
pub(crate) const TABS_OCR_FIRST: &str = "zztabalfa";

/// Native-tabs smoke: the subdirectory leg (b) `:tcd`s into, and which leg (c)'s new
/// tab is then observed to be sitting in.
///
/// **It does NOT make assertion 5 falsifiable, and the opposite claim was measured
/// false.** `:tabnew` copies the current window's EFFECTIVE working directory into
/// the tabpage it creates, so leg (b)'s tab-local `:tcd` is INHERITED whatever
/// `create_tabpage` was passed:
///
/// ```text
/// nvim --headless --clean -c "tcd tbsub" -c "tabnew" -c "echo getcwd()"
///   new tab getcwd = <project>/tbsub        global = <project>
/// ```
///
/// `wolf_create_tabpage` runs `:tabnew` and only THEN `:tcd cwd` when `cwd ~= nil`,
/// so the explicit form lands on the directory the tabpage already had — and a new
/// tab inheriting the window's folder makes the front-end's chosen cwd that same
/// directory by construction, so the two calls agree by design rather than by
/// accident. Two escapes were measured and both
/// fail: `haslocaldir(-1, 0)` reads `1` for an INHERITED tab as well as for an
/// explicitly `:tcd`'d one, and a window-local `:lcd` is inherited by `:tabnew` too.
///
/// What the subdirectory still buys is that the new tab's rooting is OBSERVED rather
/// than assumed — by leg (c)'s readback and by leg (d)'s bare `:e`, which resolves
/// only against this directory — so a tab that landed somewhere UNRELATED is caught.
/// That is the whole of assertion 5's remaining value; see its row in
/// [`native_tabs_open_switch_and_close_under_cage`]'s table.
pub(crate) const TABS_SUBDIR: &str = "tbsub";

/// Native-tabs smoke: the file opened in the SECOND tab, as a path RELATIVE TO THE
/// TEMP CWD.
///
/// Registered in `RELAUNCH_FIXTURE_NAMES` carrying its [`TABS_SUBDIR`] prefix —
/// the `LINK_SOURCE_FILE` precedent — so the DIRECTORY name goes through the same
/// substring loop instead of buying this smoke a directory exemption block.
pub(crate) const TABS_SECOND_FILE: &str = "tbsub/tbyankee.md";

/// The bare name leg (d) types at `:e`, resolved against the SECOND tab's own
/// working directory.
///
/// Typed bare rather than as a path on purpose: it only opens the fixture if the
/// tab really inherited [`TABS_SUBDIR`], so a build whose new tab landed in the
/// project root gets an empty scratch buffer and capture B's must-CONTAIN half
/// fails too. Leg (c)'s readback reaches that failure first and names it precisely;
/// this is the belt to its braces.
pub(crate) const TABS_SECOND_NAME: &str = "tbyankee.md";

/// Heading written into [`TABS_SECOND_FILE`]; lowercased it contains
/// [`TABS_OCR_SECOND`].
pub(crate) const TABS_SECOND_HEADING: &str = "# ZZTABBRAVO";

/// Lowercased substring OCR MUST find in capture B and must NOT find in capture C.
/// See [`TABS_OCR_FIRST`] for why only tab 2's editor grid can draw it.
pub(crate) const TABS_OCR_SECOND: &str = "zztabbravo";

/// Name of the readback leg (a) writes: the tabpage count and `winheight(0)` = H1.
pub(crate) const TABS_READ_A_NAME: &str = "tb_reada.txt";

/// Name of the readback leg (c) writes: two lines, `tabpagecount nr winheight
/// showtabline` and then `getcwd()`.
///
/// Two lines rather than one joined string, so the PATH never has to be split back
/// out of a separator it could contain — the `TWOWIN_READBACK_R2_NAME` precedent.
pub(crate) const TABS_READ_C_NAME: &str = "tb_readc.txt";

/// Name of the readback leg (e) writes after the tab-bar click: two lines,
/// `tabpagenr()` and then the TAIL of `bufname('%')`.
///
/// The tail rather than the raw name because `:tcd` re-derives every buffer's
/// displayed name against the new directory, and leg (b) moves the tab into a
/// subdirectory the first file does not live in. The tail is the same string under
/// either behaviour and costs nothing, because the two fixtures' tails are distinct
/// — which the crossed pair in
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` keeps true.
pub(crate) const TABS_READ_E_NAME: &str = "tb_reade.txt";

/// Name of the readback leg (f) writes after the middle-click: the tabpage count
/// and `&hidden`.
///
/// `&hidden` rides along because it is the premise the close rests on.
/// `_G.wolf_close_tabpage` runs `<n>tabclose` inside a `pcall`, which SWALLOWS a
/// refusal — and every leg here parks its block cursor with `o`, which modifies the
/// buffer. With `hidden` off, `tabclose` would refuse to abandon that buffer, the
/// pcall would eat the error, and the count would stay at 2: a red that looks
/// exactly like an unanswered `close-page` veto while being nothing of the kind.
pub(crate) const TABS_READ_F_NAME: &str = "tb_readf.txt";

/// Fraction of the output WIDTH at which the FIRST tab's centre sits with two tabs
/// open.
///
/// MEASURED with [`TAB_BAR_HEIGHT_EXPECTED`], from the same tick callback: the bar
/// spans the whole 1280 px output (it is above the `gtk::Paned`, not inside the
/// pane area) and two pages split it evenly, so the centres are at x = 320 and
/// x = 960. A quarter of the width is therefore the first tab's centre and is 320 px
/// clear of the boundary between them — the widest margin available, which is what
/// makes "this click selects tab ONE" robust rather than lucky.
///
/// Unlike the crash overlay's button, a CENTRE is the right target here: what the
/// click has to discriminate is WHICH tab, not how wide one is, and the tab's width
/// is not a property any documented revert changes.
pub(crate) const TABS_FIRST_TAB_CENTRE_FRACTION: f64 = 0.25;

/// Body of [`native_tabs_open_switch_and_close_under_cage`], appended to
/// [`POINTER_PREAMBLE`].
///
/// Six legs over ONE launch: open a file in tab 1, `:tcd` into a subdirectory, open
/// a second tab with Ctrl+Shift+T, open the second file into it, click tab 1's
/// entry in the bar, then middle-click it to close it. Three captures and four
/// Neovim readbacks.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// | code | leg | meaning |
/// | --- | --- | --- |
/// | `21` | a | the run did not START on exactly one tabpage, so every count below would be measuring the wrong baseline |
/// | `22` | c | Ctrl+Shift+T produced no second tabpage |
/// | `23` | c | the new tabpage is not rooted in [`TABS_SUBDIR`] — a precondition for leg (d), NOT a check on `create_tabpage`'s cwd argument; see assertion 5 |
/// | `24` | c | the editor did not lose a row: either the tab bar took no height, or the new page was never selected and its grid kept a taller default |
/// | `25` | e | the click on tab 1's bar entry did not move Neovim to tabpage 1 |
/// | `26` | f | the middle-click did not close a tabpage |
/// | `27` | f | the APP DIED on the middle-click — the last-page arm ran with two pages live |
///
/// ## Why `len(gettabinfo())` rather than `tabpagenr('$')`
///
/// Identical value. The idiomatic form would put a `$'` inside the double-quoted
/// shell word `wtype` is handed; POSIX leaves that `$` literal, so it would in fact
/// work — but "in fact works because of a quoting rule" is not worth spending a red
/// run on, and `gettabinfo()` needs no argument at all.
const POINTER_TABS_BODY: &str = r#"
# How long to wait after Ctrl+Shift+T before ANYTHING reads a number back.
#
# LOAD-BEARING, and the reason is measured rather than defensive: the adw::TabBar
# ANIMATES its height in over roughly one second (successive frames read 0, 2, 13,
# ... , 47). Two assertions depend on the final value -- leg (c)'s winheight(0),
# which only loses a whole row once the bar has taken its full height, and leg (e)'s
# injected click, which is composed from TAB_BAR_HEIGHT_EXPECTED and lands on the
# header bar instead if the strip is still short. Twice the measured animation.
TABS_BAR_SETTLE=2.0

# Every readback path below is ABSOLUTE and arrives as an env var, so no `writefile`
# here depends on Neovim's working directory -- which this scenario deliberately
# changes twice, once with `:tcd` and once by inheritance into a new tab. A missing
# readback is the preamble's own exit 14 (a KEYBOARD failure), never one of this
# smoke's own codes.
#
# --- Leg (a): one tab, one file, and the baseline row count ------------------
wtype -- ":e $TABS_FIRST_FILE"
wtype -k Return
sleep "$SETTLE"

# Park the block cursor off the heading's row: the inverted cursor block corrupts
# the cell it covers, and the heading is the asserted term. This binary's standing
# convention.
wtype -- o
wtype -k Escape
sleep "$SETTLE"

wtype -- ":call writefile([printf('%d %d', len(gettabinfo()), winheight(0))], '$TABS_READ_A')"
wtype -k Return
wait_file "$TABS_READ_A" || exit 14
set -- $(cat "$TABS_READ_A")
tabs_a=$1
h1=$2
echo "wolf-gnome native-tabs smoke: leg (a) tabs=$tabs_a winheight=$h1" >&2
if [ "$tabs_a" != "1" ]; then
    echo "wolf-gnome native-tabs smoke: the run started with $tabs_a tabpage(s), not 1." >&2
    echo "  Every count below is a comparison against this baseline, so nothing downstream" >&2
    echo "  would be about opening a tab. A leftover tabpage means the captive Neovim was not" >&2
    echo "  freshly launched, or something typed earlier created one." >&2
    exit 21
fi

# Capture A: the FIRST tab's heading, with one tab open. This is capture B's
# non-vacuity partner -- without it, B's must-NOT-contain half would hold for a
# window that never drew the first file at all.
grim "$OUT_PNG_A"

# --- Leg (b): give tab 1 a tab-local working directory -----------------------
#
# What this does NOT do is make the exit-23 check below able to tell
# create_tabpage(Some(cwd)) from create_tabpage(None). MEASURED: `:tabnew` copies the
# current window's effective directory into the tabpage it creates, so the new tab
# inherits this `:tcd` either way (see TABS_SUBDIR for the probe and for the two
# escapes that were tried and failed). What it does buy is a directory that is NOT
# the global cwd, so exit 23 reads "the new tab is rooted where the window was"
# rather than "the new tab is rooted at the only directory in play", and leg (d)'s
# bare `:e` has one specific place to resolve against.
#
# RELATIVE on purpose, the typed-`:cd` smoke's reason: an absolute `:tcd <path>`
# would put the whole temp path into the command line the cmdline strip draws.
#
# `:tcd` is deliberately NOT locked out by runtime/nvim/init.lua's `:tab*`
# cnoreabbrev set -- whole-word matching, and `tcd` is a different word.
wtype -- ":tcd $TABS_SUBDIR"
wtype -k Return
sleep "$SETTLE"

# --- Leg (c): Ctrl+Shift+T ---------------------------------------------------
#
# GTK captures the chord before the window key controller and activates
# `win.new-tab`; the shell gates on liveness, resolves the new tab's directory from
# the window's live cwd, issues the blocking create_tabpage RPC and appends a page.
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"

# Four numbers and a path, in ONE writefile so they describe one instant.
# &showtabline rides along as a PRECONDITION: Neovim's own default draws a tabline
# as soon as a second tabpage exists, which would cost the editor a row for a reason
# that has nothing to do with the adw::TabBar and would leave the h2 < h1 check
# below green with the bar never appended.
wtype -- ":call writefile([printf('%d %d %d %d', len(gettabinfo()), tabpagenr(), winheight(0), &showtabline), getcwd()], '$TABS_READ_C')"
wtype -k Return
wait_file "$TABS_READ_C" || exit 14
set -- $(sed -n 1p "$TABS_READ_C")
tabs_c=$1
nr_c=$2
h2=$3
stl_c=$4
cwd_c=$(sed -n 2p "$TABS_READ_C")
echo "wolf-gnome native-tabs smoke: leg (c) tabs=$tabs_c nr=$nr_c winheight=$h2 showtabline=$stl_c" >&2
echo "  getcwd=$cwd_c" >&2

if [ "$tabs_c" != "2" ]; then
    echo "wolf-gnome native-tabs smoke: after Ctrl+Shift+T the captive Neovim has $tabs_c" >&2
    echo "  tabpage(s), not 2. The chord produced no tabpage at all: either win.new-tab is not" >&2
    echo "  installed (Command::NewTab demoted back to Deferred, in which case the chord fell" >&2
    echo "  through to Neovim as <C-t>), or the liveness gate refused, or create_tabpage" >&2
    echo "  answered 0." >&2
    exit 22
fi

# The comparison is on the final path component, the two-window smoke's reason:
# /tmp may resolve through a symlink on this host, so getcwd() need not be
# string-equal to the path this harness built, while the basename survives every
# symlink. The Rust side asserts the same thing against its own fixture path.
if [ "${cwd_c##*/}" != "$TABS_SUBDIR" ]; then
    echo "wolf-gnome native-tabs smoke: the NEW tab's working directory is $cwd_c, whose final" >&2
    echo "  component is not $TABS_SUBDIR. This is a PRECONDITION for leg (d), not a check on" >&2
    echo "  create_tabpage's cwd argument: :tabnew COPIES the current window's effective" >&2
    echo "  directory into the tabpage it creates, so leg (b)'s :tcd is inherited whether" >&2
    echo "  create_tabpage was passed the cwd or nothing at all (measured; see TABS_SUBDIR)." >&2
    echo "  What a red here names is a tab that landed somewhere UNRELATED -- the launch cwd," >&2
    echo "  the app's home directory, or a stale tracked_cwd handed over in place of the" >&2
    echo "  window's live folder -- and leg (d)'s bare :e would then open an empty scratch" >&2
    echo "  buffer instead of the second fixture." >&2
    exit 23
fi

if [ "$h2" -ge "$h1" ]; then
    echo "wolf-gnome native-tabs smoke: the editor pane reports $h2 row(s) with two tabs" >&2
    echo "  open, against $h1 with one. The adw::TabBar is supposed to take real height out of" >&2
    echo "  the window's vertical box, so the pane must lose at least one row. The two causes" >&2
    echo "  are told apart by the number above. EQUAL to $h1: the bar was never appended between" >&2
    echo "  the header bar and the gtk::Paned, or it is still autohidden because the second page" >&2
    echo "  was never added to the view. GREATER than $h1 (measured 14 against 11): the new page" >&2
    echo "  was appended but never SELECTED, so its DrawingArea stays unmapped at 0x0," >&2
    echo "  pane_view::pending_resize refuses that size, and the new tabpage's grid keeps a" >&2
    echo "  taller default -- check that open_tab still calls set_selected_page." >&2
    echo "  (showtabline=$stl_c: a value other than 0 would make this check unsound in the" >&2
    echo "  other direction -- Neovim's own tabline would take the row instead.)" >&2
    exit 24
fi

# --- Leg (d): the second file, into the second tab ---------------------------
#
# BARE name, resolved against the new tab's own working directory: see
# TABS_SECOND_NAME.
wtype -- ":e $TABS_SECOND_NAME"
wtype -k Return
sleep "$SETTLE"
wtype -- o
wtype -k Escape
sleep "$SETTLE"

# Capture B: the SECOND tab's heading, and NOT the first's. The must-not-contain
# half is what says only ONE page's pane view is on screen.
grim "$OUT_PNG_B"

# --- Leg (e): click the FIRST tab's entry in the bar -------------------------
#
# The pointer is parked at the probe point inside the editor pane (POINTER_PREAMBLE
# ends there), so this motion crosses into the bar and produces a real
# leave/enter/motion sequence rather than a no-op motion_absolute.
hsend motion "$TABS_CLICK_X" "$TABS_CLICK_Y"
hsend press "$BTN_LEFT"
hsend release "$BTN_LEFT"
sleep "$SETTLE"

# Capture C BEFORE the readback, the preview-link smoke's ordering: the readback's
# typed command line is drawn by the cmdline strip, and this capture is meant to
# carry no typed text at all.
grim "$OUT_PNG"

# The buffer's TAIL rather than its raw name, and not for brevity: `:tcd` re-derives
# every buffer's displayed name against the new directory, so after leg (b) the first
# file -- which lives in the PARENT of that directory -- is no longer expressible
# relative to it. The tail is the same string under either behaviour, and it loses
# nothing here because the two fixtures' tails are distinct (the guard asserts the
# crossing).
wtype -- ":call writefile([printf('%d', tabpagenr()), fnamemodify(bufname('%'), ':t')], '$TABS_READ_E')"
wtype -k Return
wait_file "$TABS_READ_E" || exit 14
nr_e=$(sed -n 1p "$TABS_READ_E")
buf_e=$(sed -n 2p "$TABS_READ_E")
echo "wolf-gnome native-tabs smoke: leg (e) nr=$nr_e bufname=$buf_e" >&2

if [ "$nr_e" != "1" ]; then
    echo "wolf-gnome native-tabs smoke: after clicking the FIRST tab the captive Neovim is on" >&2
    echo "  tabpage $nr_e, not 1. GTK may well have selected the page -- what this says is that" >&2
    echo "  the selection never reached Neovim, i.e. the selected-page notify no longer calls" >&2
    echo "  set_current_tabpage. Note the capture taken a moment ago stays CORRECT under that" >&2
    echo "  defect, because a background grid is frozen and byte-identical, so page 0 repaints" >&2
    echo "  its own content whether or not Neovim switched. This readback is the discriminator." >&2
    exit 25
fi
if [ "$buf_e" != "$TABS_FIRST_FILE" ]; then
    echo "wolf-gnome native-tabs smoke: Neovim is on tabpage 1 but its current buffer's tail is" >&2
    echo "  $buf_e rather than $TABS_FIRST_FILE, so the two tabs are not bound to the tabpages" >&2
    echo "  this smoke thinks they are. A page bound to the wrong handle would switch to the" >&2
    echo "  wrong tabpage and still report a plausible number." >&2
    exit 25
fi

# --- Leg (f): middle-click the same entry to CLOSE that tab ------------------
#
# Middle-click on an adw::TabBar tab raises `close-page`, which is a VETO the
# handler must answer with close_page_finish and Stop. MEASURED under cage before
# this leg was written; see POINTER_BUTTON_MIDDLE.
hsend motion "$TABS_CLICK_X" "$TABS_CLICK_Y"
hsend press "$BTN_MIDDLE"
hsend release "$BTN_MIDDLE"
sleep "$TABS_BAR_SETTLE"

# Aliveness FIRST: a window that closed takes its readback with it, and "the file
# never appeared" (exit 14) would bury the finding under a keyboard failure.
if ! kill -0 "$app_pid" 2>/dev/null; then
    echo "wolf-gnome native-tabs smoke: the APP EXITED when one of TWO tabs was closed." >&2
    echo "  The last-page arm ran with two pages live: close_outcome answered CloseWindow at a" >&2
    echo "  page count where it must answer CloseTabpage, so the window routed a tab close into" >&2
    echo "  its own close and shut the captive Neovim down." >&2
    exit 27
fi

wtype -- ":call writefile([printf('%d %d', len(gettabinfo()), &hidden)], '$TABS_READ_F')"
wtype -k Return
wait_file "$TABS_READ_F" || exit 14
set -- $(cat "$TABS_READ_F")
tabs_f=$1
hidden_f=$2
echo "wolf-gnome native-tabs smoke: leg (f) tabs=$tabs_f hidden=$hidden_f" >&2

if [ "$tabs_f" != "1" ]; then
    echo "wolf-gnome native-tabs smoke: after the middle-click the captive Neovim still has" >&2
    echo "  $tabs_f tabpage(s), so no tabpage was closed. Two candidate causes, in order:" >&2
    echo "  (1) the close-page VETO was left unanswered -- a handler returning Proceed without" >&2
    echo "  close_page_finish leaves the page on screen forever with no error, and" >&2
    echo "  close_tabpage is never reached; (2) hidden=$hidden_f -- with 'hidden' off, Neovim" >&2
    echo "  refuses <n>tabclose over the modified buffer this scenario's cursor park creates," >&2
    echo "  and wolf_close_tabpage's pcall swallows that refusal silently." >&2
    exit 26
fi

hsend quit
exit 0
"#;

/// Two `adw::TabPage`s over ONE captive Neovim: opened with Ctrl+Shift+T, switched
/// by a real click on the tab bar, and closed by a real middle-click.
///
/// ## What it proves that nothing else can
///
/// Every headless tab test can only see values the front-end itself
/// computed. This is the only observation in the repository that a second
/// `adw::TabPage` exists over one core at all, that the GTK selection reaches
/// Neovim, that the bar costs the editor grid real rows, and that `close-page`'s
/// veto is answered.
///
/// ## Which capture proves what, and which one does NOT
///
/// Capture A carries the first heading with one tab open; capture B the second and
/// NOT the first; capture C the first again and NOT the second. A and B together
/// are the pixel claim that exactly one page's pane view is on screen at a time.
///
/// **Capture C is not the switch's discriminator, and reading it as one is the
/// mistake this paragraph exists to prevent.** A background grid was measured to be
/// FROZEN and byte-identical rather than pruned, and a background page's
/// `DrawingArea`s are unmapped and never asked to draw — so with
/// `set_current_tabpage` deleted from the selection notify, GTK still shows page 0,
/// page 0 still repaints its own last content, and capture C is still exactly the
/// first heading with the second absent. It stays GREEN under that revert. The
/// `tabpagenr()` READBACK is what discriminates, and it exits 25.
///
/// ## Assertions, classified
///
/// A precondition gets no revert of its own. Where a revert WAS run and this smoke
/// stayed green, or reddened somewhere other than the row it was aimed at, that
/// measurement stands in the revert column in place of the claim it falsified. The
/// `measured` column is from an actual run.
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 0 | the second fixture is inside [`TABS_SUBDIR`] and NOT at the project root | precondition on the fixture layout, checked against the filesystem this run uses | none | n/a |
/// | 1 | the scenario exited 0 | precondition — it ran to completion | none; do not invent one | n/a |
/// | 2 | three captures present, the injected point inside the reported extents | precondition | none | n/a |
/// | 3 | `&showtabline == 0` | PRECONDITION for assertion 6 — Neovim draws no tabline of its own | none of this crate's; `crates/wolf-core/tests/tabpages.rs::a_second_tabpage_draws_no_neovim_tabline` owns it | n/a |
/// | 4 | leg (c) reports two tabpages, the second current | **discriminator** | `command_action.rs`: restore `Command::NewTab => Deferred(..)` | scenario exit 22 |
/// | 5 | leg (c)'s `getcwd()` ends in [`TABS_SUBDIR`] | PRECONDITION for leg (d) — where the new tab is rooted | **no isolated red.** The revert in `shell.rs`, `create_tabpage(Some(cwd))` -> `create_tabpage(None)`, was RUN and this smoke stayed GREEN: `:tabnew` copies the current window's effective directory, so the tab inherits leg (b)'s `:tcd` under both calls, and a new tab inheriting the window's folder makes the two arguments name the same directory anyway. `haslocaldir(-1, 0)` and a window-local `:lcd` were both measured and neither separates them either. Kept because a tab rooted somewhere UNRELATED has no other observer here; see [`TABS_SUBDIR`] | ran: GREEN |
/// | 6 | `H2 < H1` | **discriminator** | `window.rs`: drop the `content.append(&tabs.bar_widget())`, which leaves `H2 == H1`. A SECOND revert lands on this same assertion: dropping `set_selected_page` from `open_tab` leaves the new page unmapped and `0x0`, `pane_view::pending_resize` refuses that size, and the new tabpage's grid keeps a taller default — `H2` reads 14 against `H1` = 11, a GROWN count | scenario exit 24, from either |
/// | 7 | leg (e) reports tabpage 1 and the first file | **discriminator** | `tab_view.rs`: delete the `set_current_tabpage` call in the selection notify | scenario exit 25 |
/// | 8 | `&hidden` is on | PRECONDITION for assertion 9 — Neovim would accept the close | none | n/a |
/// | 9 | leg (f) reports one tabpage | **discriminator** | `tab_view.rs`: return `Propagation::Proceed` with no `close_page_finish` | scenario exit 26 |
/// | 10 | capture A carries [`TABS_OCR_FIRST`] | precondition — capture B's non-vacuity partner | none | n/a |
/// | 11 | capture B carries [`TABS_OCR_SECOND`] | corroboration for the SELECTION of a newly appended page | `tab_view.rs`: drop `set_selected_page` from `open_tab`. MEASURED, and the run never reaches capture B: the unselected page's `DrawingArea` stays unmapped at `0x0`, `pending_resize` refuses that size, the new tabpage's grid keeps a taller default and leg (c) exits 24 with `H2` = 14 against `H1` = 11. The revert DOES redden this smoke — at assertion 6, not here — so it is not an unproven rule; what has no red of its own is this assertion | scenario exit 24, in leg (c) |
/// | 12 | capture B does NOT carry [`TABS_OCR_FIRST`] | **discriminator** | **no isolated red.** What it catches is two panes drawn at once, which is reachable only by a structural edit that appends both tabs' widgets to one visible container. Recorded rather than papered over, the same shape as the drawer smoke's leg (f) | n/a |
/// | 13 | capture C carries [`TABS_OCR_FIRST`] and not [`TABS_OCR_SECOND`] | corroboration — the only pixel evidence that selecting a page changes what is on screen | none that is isolated; see the capture-C paragraph above | n/a |
///
/// ## Seconds
///
/// 16.9 s, measured over three filtered single-test runs (16.98 / 16.92 / 16.90)
/// against a ~500 s suite — alongside
/// `two_windows_two_neovims_and_two_saved_sessions_under_cage` at 16.73 s.
#[test]
fn native_tabs_open_switch_and_close_under_cage() {
    const SMOKE: &str = "native_tabs_open_switch_and_close_under_cage";
    // `tabpagecount winheight`.
    const LEG_A_TOKENS: usize = 2;
    // `tabpagecount tabpagenr winheight showtabline`.
    const LEG_C_TOKENS: usize = 4;
    // `tabpagecount hidden`.
    const LEG_F_TOKENS: usize = 2;
    // The inclusive band of scenario exit codes this smoke adds on top of
    // `POINTER_PREAMBLE`'s shared 11-15.
    const TABS_EXIT_FIRST: i32 = 21;
    const TABS_EXIT_LAST: i32 = 27;
    // What `&showtabline` must read: re-typed here rather than imported, so a
    // change to the captive config fails this assertion instead of moving with it.
    const SHOWTABLINE_OFF: i64 = 0;
    // What `&hidden` must read.
    const HIDDEN_ON: i64 = 1;
    // Tabpage counts and numbers the legs expect.
    const TABS_AFTER_OPEN: i64 = 2;
    const TABS_AFTER_CLOSE: i64 = 1;
    const FIRST_TABPAGE_NR: i64 = 1;

    let runtime_dir = require_smoke_env();

    let cwd = make_temp_cwd();
    // `write_relaunch_fixtures` writes with `std::fs::write` and creates no parents,
    // so the subdirectory has to exist first — the `LINK_FIXTURE_DIR` precedent.
    let subdir = cwd.join(TABS_SUBDIR);
    std::fs::create_dir_all(&subdir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the fixture subdirectory {}: {e}",
            subdir.display()
        )
    });
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (TABS_FIRST_FILE, TABS_FIRST_HEADING),
            (TABS_SECOND_FILE, TABS_SECOND_HEADING),
        ],
    );

    // Assertion 0 (precondition): if the second fixture also existed at the root, leg (d)'s bare `:e`
    // would find it whether or not the new tab inherited the subdirectory.
    let second_in_subdir = cwd.join(TABS_SECOND_FILE);
    assert!(
        second_in_subdir.is_file(),
        "{SMOKE}: the second fixture {} was not written. Leg (d) opens it by BARE NAME from \
         inside {TABS_SUBDIR:?}, so without it on disk capture B could only ever fail.",
        second_in_subdir.display()
    );
    let second_at_root = cwd.join(TABS_SECOND_NAME);
    assert!(
        !second_at_root.exists(),
        "{SMOKE}: {} exists, so leg (d)'s bare `:e {TABS_SECOND_NAME}` resolves to a real file \
         against the PROJECT ROOT as well as against {TABS_SUBDIR:?}. That disarms the belt half \
         of leg (c)'s inheritance check: a new tab that never inherited the window's folder would \
         still open a document and still draw a heading. A SIDEBAR_FILLER_FILES entry or another \
         smoke's fixture renamed to {TABS_SECOND_NAME:?} is the likely cause; rename that rather \
         than moving this smoke's fixture out of {TABS_SUBDIR:?}.",
        second_at_root.display()
    );

    let read_a = cwd.join(TABS_READ_A_NAME);
    let read_c = cwd.join(TABS_READ_C_NAME);
    let read_e = cwd.join(TABS_READ_E_NAME);
    let read_f = cwd.join(TABS_READ_F_NAME);
    let out_png_a = runtime_dir.join("wolf_gnome_native_tabs_a.png");
    let out_png_b = runtime_dir.join("wolf_gnome_native_tabs_b.png");
    let out_png = runtime_dir.join("wolf_gnome_native_tabs_c.png");
    for path in [
        &read_a, &read_c, &read_e, &read_f, &out_png_a, &out_png_b, &out_png,
    ] {
        clear_evidence(path, SMOKE);
    }

    // x is a quarter of the output, y half the bar below the header bar; pinned because the point must
    // exist before cage starts, so the run re-checks the reported extents.
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "the expected cage output width");
    let click_x = pixel_target(
        output_w * TABS_FIRST_TAB_CENTRE_FRACTION,
        "the first tab's centre x",
    );
    let click_y = pixel_target(
        OVERLAY_ORIGIN_Y_EXPECTED + TAB_BAR_HEIGHT_EXPECTED / 2.0,
        "the tab bar's centre y",
    );
    let click_x_env = click_x.to_string();
    let click_y_env = click_y.to_string();

    let status = run_pointer_scenario(
        POINTER_TABS_BODY,
        &cwd,
        &runtime_dir,
        &out_png,
        &[
            ("TABS_FIRST_FILE", OsStr::new(TABS_FIRST_FILE)),
            ("TABS_SECOND_NAME", OsStr::new(TABS_SECOND_NAME)),
            ("TABS_SUBDIR", OsStr::new(TABS_SUBDIR)),
            ("TABS_READ_A", read_a.as_os_str()),
            ("TABS_READ_C", read_c.as_os_str()),
            ("TABS_READ_E", read_e.as_os_str()),
            ("TABS_READ_F", read_f.as_os_str()),
            ("TABS_CLICK_X", OsStr::new(&click_x_env)),
            ("TABS_CLICK_Y", OsStr::new(&click_y_env)),
            ("BTN_MIDDLE", OsStr::new(POINTER_BUTTON_MIDDLE)),
            ("OUT_PNG_A", out_png_a.as_os_str()),
            ("OUT_PNG_B", out_png_b.as_os_str()),
        ],
    );

    // Assertion 1 (precondition): tab-specific codes first; the rest fall through to
    // `assert_pointer_scenario_succeeded`.
    assert!(
        !matches!(status.code(), Some(c) if (TABS_EXIT_FIRST..=TABS_EXIT_LAST).contains(&c)),
        "{SMOKE}: the cage scenario exited with a TAB-SPECIFIC code ({status}). The scenario \
         echoes every number it read to stderr, so read that first; each code names one leg.\n\
         Exit 21: the run did not START on exactly one tabpage, so every count below would be \
         measured against the wrong baseline.\n\
         Exit 22: Ctrl+Shift+T produced no second tabpage. Either `win.new-tab` is not installed \
         (a `Command::NewTab` demoted back to `Deferred` leaves the chord falling through to \
         Neovim as `<C-t>`), or `tabs_ui::new_tab_press` refused, or `create_tabpage` answered 0.\n\
         Exit 23: the new tabpage is not rooted in the subdirectory leg (b) moved the window into. \
         A PRECONDITION for leg (d) rather than a check on `create_tabpage`'s cwd argument — \
         `:tabnew` copies the current window's effective directory, so the tab inherits leg (b)'s \
         `:tcd` either way — and what it names is a tab that landed somewhere UNRELATED, e.g. the \
         launch cwd or a stale `tracked_cwd` handed over in place of the window's live folder.\n\
         Exit 24: the editor did not lose a row, and the number the scenario printed says which \
         cause. EQUAL: the `adw::TabBar` took no height — check that `window::build` still appends \
         it between the header bar and the `gtk::Paned`. GREATER: `open_tab` no longer calls \
         `set_selected_page`, so the new page is never mapped, reports 0x0, and its grid keeps a \
         taller default that `pane_view::pending_resize` refuses to correct.\n\
         Exit 25: the click on the first tab's bar entry did not move Neovim to tabpage 1. Either \
         the selection notify no longer calls `set_current_tabpage`, or the derived point missed \
         the bar (both constants behind it are pinned; see `TAB_BAR_HEIGHT_EXPECTED`).\n\
         Exit 26: the middle-click closed no tabpage — the `close-page` veto was left unanswered, \
         or Neovim refused the `tabclose` (the scenario prints `&hidden`, which decides that).\n\
         Exit 27: THE APP DIED closing one of TWO tabs. `close_outcome` answered `CloseWindow` at \
         a page count where it must answer `CloseTabpage`, so a tab close shut the whole window \
         and its captive Neovim down.\n\
         Captures expected at {}, {} and {}.",
        out_png_a.display(),
        out_png_b.display(),
        out_png.display()
    );
    assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);

    // --- Assertion 2 (precondition): the captures and the geometry -----------
    for path in [&out_png_a, &out_png_b, &out_png] {
        assert_png_present(SMOKE, path);
    }

    let extents = read_output_extents(&cwd, SMOKE);
    assert!(
        extents == (CAGE_OUTPUT_W_EXPECTED, CAGE_OUTPUT_H_EXPECTED),
        "{SMOKE}: `wolf-pointer-inject` reported a {}x{} output, but the tab-bar click point was \
         derived BEFORE the run against CAGE_OUTPUT_W_EXPECTED x CAGE_OUTPUT_H_EXPECTED = \
         {CAGE_OUTPUT_W_EXPECTED}x{CAGE_OUTPUT_H_EXPECTED}. The click was therefore aimed with the \
         wrong bar width, and the first tab's centre is not a quarter of THIS output. Re-measure \
         and update both constants with the date; do not widen anything.",
        extents.0,
        extents.1
    );
    assert_inside_extents(SMOKE, "the FIRST TAB's centre", (click_x, click_y), extents);

    // Assertion 3 (precondition): `showtabline` is 0, or a second tabpage costs a row with no bar and
    // dropping the bar has no red. `init.lua` is not this test's to control, so Neovim is asked.
    let leg_c = read_readback(&read_c, SMOKE);
    let mut leg_c_lines = leg_c.lines();
    let leg_c_numbers = leg_c_lines.next().unwrap_or_default();
    let leg_c_cwd = leg_c_lines.next().unwrap_or_default().trim().to_string();
    let leg_c_tokens: Vec<&str> = leg_c_numbers.split_whitespace().collect();
    assert!(
        leg_c_tokens.len() == LEG_C_TOKENS,
        "{SMOKE}: leg (c)'s readback {} holds {} number(s) on its first line, expected \
         {LEG_C_TOKENS} (tabpage count, tabpage number, winheight, showtabline). Raw contents were \
         {leg_c:?}. A SHORT line means the `printf` format and its argument list disagree.",
        read_c.display(),
        leg_c_tokens.len()
    );
    let tabs_after_open = parse_readback_int(leg_c_tokens[0], &read_c, &leg_c, SMOKE);
    let nr_after_open = parse_readback_int(leg_c_tokens[1], &read_c, &leg_c, SMOKE);
    let winheight_two = parse_readback_int(leg_c_tokens[2], &read_c, &leg_c, SMOKE);
    let showtabline = parse_readback_int(leg_c_tokens[3], &read_c, &leg_c, SMOKE);
    assert_eq!(
        showtabline, SHOWTABLINE_OFF,
        "{SMOKE}: with two tabpages live the captive Neovim reports `&showtabline` = \
         {showtabline}, not {SHOWTABLINE_OFF}. Neovim then draws its OWN tabline into grid 1, \
         which neither front-end paints and which still costs the editor a whole row — so \
         assertion 6 below would be satisfied by Neovim's chrome rather than by the \
         `adw::TabBar`, and would keep passing with the bar never appended. This is a PRECONDITION \
         about the shared `runtime/nvim/init.lua`, not a front-end failure; read it as \"assertion \
         6 cannot be trusted this run\", and fix it in \
         `crates/wolf-core/tests/tabpages.rs::a_second_tabpage_draws_no_neovim_tabline`'s subject."
    );

    // --- Assertion 4 (DISCRIMINATOR): the chord opened a second tabpage ------
    let leg_a = read_ints(&read_a, LEG_A_TOKENS, SMOKE);
    let (tabs_at_start, winheight_one) = (leg_a[0], leg_a[1]);
    assert_eq!(
        tabs_after_open, TABS_AFTER_OPEN,
        "{SMOKE}: after Ctrl+Shift+T the captive Neovim reports {tabs_after_open} tabpage(s), not \
         {TABS_AFTER_OPEN} (it started with {tabs_at_start}). The scenario exits 22 on this, so \
         reaching the assertion means the readback SHAPE changed rather than the behaviour."
    );
    assert_eq!(
        nr_after_open, TABS_AFTER_OPEN,
        "{SMOKE}: the second tabpage exists but Neovim is on tabpage {nr_after_open}, not \
         {TABS_AFTER_OPEN}. `wolf_create_tabpage` runs `:tabnew`, which ENTERS the new tabpage, so \
         this says something switched back — and leg (d) would then open the second file into the \
         FIRST tab, making capture B's crossed pair describe the wrong page."
    );

    // Assertion 5 (precondition for leg (d)): `:tabnew` inherits the effective cwd, so it has no red;
    // kept because leg (d)'s bare `:e` resolves here. Compared on the last component (symlinks).
    let got_tail = leg_c_cwd.rsplit('/').next().unwrap_or_default();
    assert_eq!(
        got_tail, TABS_SUBDIR,
        "{SMOKE}: the new tab's working directory is {leg_c_cwd:?}, whose final component is \
         {got_tail:?} rather than {TABS_SUBDIR:?}. This is a PRECONDITION for leg (d), whose bare \
         `:e {TABS_SECOND_NAME}` resolves against exactly this directory; it does NOT tell \
         `create_tabpage(Some(cwd))` from `create_tabpage(None)`, because `:tabnew` copies the \
         current window's effective directory and the new tab inherits leg (b)'s `:tcd` either way \
         (measured — see `TABS_SUBDIR`). A component of {TEMP_CWD_BASENAME:?} means the tab landed \
         at the PROJECT ROOT, i.e. something other than the window's live folder reached \
         `create_tabpage` — a stale `tracked_cwd`, or the launch cwd. The scenario exits 23 on \
         this, so reaching the assertion means the readback shape changed."
    );

    // Assertion 6 (discriminator): a strict inequality, no constant. Measured provenance only: H1 = 11
    // rows at one tab, H2 = 10 at two, 1280x720 at SMOKE_FONT_PT.
    assert!(
        winheight_two < winheight_one,
        "{SMOKE}: the editor pane reports {winheight_two} row(s) with two tabs open against \
         {winheight_one} with one, so the editor lost no row when the second tab appeared. \
         Assertion 3 is GREEN, so Neovim is drawing no tabline of its own and this is entirely \
         about the front-end. Two reverts land here and the COUNT tells them apart. EQUAL: \
         `window::build` no longer appends `tabs.bar_widget()` to the content box between the \
         header bar and the `gtk::Paned`, so the bar exists and is allocated nothing. GREATER than \
         the one-tab count (measured 14 against 11): `open_tab` no longer calls \
         `set_selected_page`, so the appended page is never mapped, its `DrawingArea` reports \
         0x0, `pane_view::pending_resize` refuses that size, and the new tabpage's grid keeps a \
         taller default nothing corrects — capture B would be red for the same cause if the run \
         reached it. The scenario exits 24 on this, so reaching the assertion means the readback \
         shape changed."
    );

    // Assertion 7 (discriminator): capture C stays green under this row's revert, so only this readback
    // makes the claim. See the doc comment.
    let leg_e = read_readback(&read_e, SMOKE);
    let mut leg_e_lines = leg_e.lines();
    let nr_after_click = parse_readback_int(
        leg_e_lines.next().unwrap_or_default().trim(),
        &read_e,
        &leg_e,
        SMOKE,
    );
    let buf_after_click = leg_e_lines.next().unwrap_or_default().trim().to_string();
    assert_eq!(
        nr_after_click, FIRST_TABPAGE_NR,
        "{SMOKE}: after clicking the FIRST tab's entry the captive Neovim is on tabpage \
         {nr_after_click}, not {FIRST_TABPAGE_NR}. GTK may well have selected the page — what this \
         says is that the selection never reached Neovim, i.e. the selected-page notify no longer \
         calls `set_current_tabpage`. The scenario exits 25 on this, so reaching the assertion \
         means the readback shape changed."
    );
    assert_eq!(
        buf_after_click, TABS_FIRST_FILE,
        "{SMOKE}: Neovim is on tabpage {FIRST_TABPAGE_NR} but the tail of its current buffer's \
         name is {buf_after_click:?} rather than {TABS_FIRST_FILE:?}. The two pages are not bound \
         to the tabpages this smoke believes they are — a page holding the wrong handle switches \
         to the wrong tabpage and still reports a plausible NUMBER, which is why the buffer name \
         is asserted beside it. An empty value means the window is on an unnamed buffer, i.e. leg \
         (a)'s `:e` never took effect and capture A's heading came from somewhere else."
    );

    let leg_f = read_ints(&read_f, LEG_F_TOKENS, SMOKE);
    let (tabs_after_close, hidden) = (leg_f[0], leg_f[1]);

    // --- Assertion 8 (PRECONDITION for assertion 9) --------------------------
    assert_eq!(
        hidden, HIDDEN_ON,
        "{SMOKE}: the captive Neovim reports `&hidden` = {hidden}, not {HIDDEN_ON}. Every leg here \
         parks its block cursor with `o`, which MODIFIES the buffer, and with `hidden` off Neovim \
         refuses to abandon a modified buffer — so `_G.wolf_close_tabpage`'s `<n>tabclose` would \
         fail, its `pcall` would swallow the error, and assertion 9 would report a tabpage count \
         that never moved. That failure is indistinguishable from an unanswered `close-page` veto \
         without this row. `hidden` is on by Neovim's own default; something turned it off."
    );

    // --- Assertion 9 (DISCRIMINATOR): the close-page veto was answered -------
    assert_eq!(
        tabs_after_close, TABS_AFTER_CLOSE,
        "{SMOKE}: after middle-clicking the first tab the captive Neovim still reports \
         {tabs_after_close} tabpage(s), not {TABS_AFTER_CLOSE}. Assertion 8 is GREEN, so Neovim \
         would have accepted the close: what this names is the front-end never asking. \
         `close-page` is a VETO — a handler that returns `Propagation::Proceed` without calling \
         `close_page_finish` leaves the page on screen forever with no error and no message, and \
         `close_tabpage` is never reached. The scenario exits 26 on this, so reaching the \
         assertion means the readback shape changed."
    );

    // --- Assertion 10 (precondition): capture A is capture B's partner -------
    let ocr_a = ocr_to_lowercase(&out_png_a);
    assert!(
        ocr_a.contains(TABS_OCR_FIRST),
        "{SMOKE}: capture A did not contain {TABS_OCR_FIRST:?} — the heading of \
         {TABS_FIRST_FILE:?}, with ONE tab open and nothing tab-related having happened yet. This \
         is a PRECONDITION with no revert of ours: without it, capture B's must-NOT-contain half \
         below holds for a window that never drew the first file at all, and the sharpest claim \
         this smoke makes would be vacuous. Read this red as \"assertion 12 cannot be trusted this \
         run\". PNG kept at {}. OCR output was:\n{ocr_a}",
        out_png_a.display()
    );

    // --- Assertions 11 and 12: exactly ONE page's pane view is on screen -----
    let ocr_b = ocr_to_lowercase(&out_png_b);
    assert!(
        ocr_b.contains(TABS_OCR_SECOND),
        "{SMOKE}: capture B did not contain {TABS_OCR_SECOND:?} — the heading of \
         {TABS_SECOND_FILE:?}, opened into the tab Ctrl+Shift+T created. Assertions 4 to 6 are \
         GREEN, so the tabpage exists, it is rooted in {TABS_SUBDIR:?} and the editor lost a row; \
         what is missing is the new page being SHOWN. The obvious suspect — `open_tab` no longer \
         calling `set_selected_page`, since `adw::TabView::append` selects only the FIRST page — \
         is MEASURED not to arrive here: an unselected page is never mapped, so its grid keeps a \
         taller default and the scenario exits 24 in leg (c) long before this capture is taken. \
         Reaching THIS red therefore means the page was selected and still not drawn, or that leg \
         (d)'s `:e` opened nothing. PNG kept at {}. OCR output was:\n{ocr_b}",
        out_png_b.display()
    );
    assert!(
        !ocr_b.contains(TABS_OCR_FIRST),
        "{SMOKE}: capture B contained {TABS_OCR_FIRST:?} — the FIRST tab's heading — while the \
         second tab was selected. Only one page's pane view may be on screen at a time, and \
         assertion 10 proves the term really is drawable, so this says both tabs' widgets are \
         visible at once: the first suspect is the page's `gtk::Overlay` having been added to a \
         container that shows every child rather than to the `adw::TabView`. Nothing in this binary \
         has an isolated red for this row — what it catches is reachable only by a structural \
         edit — and that is recorded rather than papered over. It is not vacuous in the other \
         direction: no fixture NAME carries this term (the tab bar draws both filenames in this \
         very capture), which the fixture guards enforce. PNG kept at {}. OCR output was:\n{ocr_b}",
        out_png_b.display()
    );

    // Assertion 13: corroboration only; a frozen background grid repaints identically, so this stays
    // green under assertion 7's revert.
    let ocr_c = ocr_to_lowercase(&out_png);
    assert!(
        ocr_c.contains(TABS_OCR_FIRST),
        "{SMOKE}: capture C did not contain {TABS_OCR_FIRST:?} after the click selected the first \
         tab. Assertion 7 is GREEN, so Neovim really is on tabpage {FIRST_TABPAGE_NR}; what this \
         says is that the first page is not what GTK is drawing. Either the click selected some \
         other page, or the page's pane view stopped painting when it was re-shown. PNG kept at \
         {}. OCR output was:\n{ocr_c}",
        out_png.display()
    );
    assert!(
        !ocr_c.contains(TABS_OCR_SECOND),
        "{SMOKE}: capture C contained {TABS_OCR_SECOND:?} — the SECOND tab's heading — after the \
         click selected the FIRST tab. Together with assertion 12 this is the pair that says \
         exactly one page is drawn; a term from the deselected page means both are. PNG kept at \
         {}. OCR output was:\n{ocr_c}",
        out_png.display()
    );
}
