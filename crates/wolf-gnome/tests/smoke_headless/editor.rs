//! Editor smokes: the typed-text window, the file and buffer pickers, the empty state, the
//! command palette, navigation history, the command line and its messages, the clipboard
//! and the save chord and dialog.

use super::*;

/// The lowercase word we type in insert mode (in the window smoke scenario).
/// Kept short and all-lowercase so OCR noise is unlikely to corrupt every
/// character. After typing it we move the cursor OFF its row (see [`SCENARIO`])
/// so the full word — not just a prefix — can be asserted against OCR.
pub(crate) const OCR_WORD: &str = "hello";

/// Picker smoke scenario: the markdown file dropped into the launched app's temp
/// working directory. Its name contains [`PICKER_QUERY`] so the typed query matches
/// it, and its HEADING — deliberately a different word — contains [`PICKER_OCR`] so
/// OCR can confirm nvim actually opened it.
///
/// ## Why the filename must NOT contain [`PICKER_OCR`]
///
/// A fixture named `pickertarget.md` under a [`PICKER_OCR`] of `pickertarget` lets
/// the tree sidebar's own row for the fixture satisfy the smoke's positive assertion
/// whether or not the picker ever opens — the editor pane contributes nothing. That
/// is not a theory: in a broken-accelerator run under that naming (GTK's keymap-aware
/// accel matching never fired under `wtype`, so Ctrl+Shift+O was forwarded to Neovim
/// instead of opening the picker) this smoke PASSED while the picker demonstrably
/// never opened.
///
/// It is the exact failure mode
/// `sidebar_filler_names_cannot_satisfy_any_smoke_assertion` exists to prevent,
/// which that guard could not catch because it only inspects
/// [`SIDEBAR_FILLER_FILES`], not each smoke's own fixture. The fix follows the
/// precedent already set by `CLOSE_FILE_GONE` (`cgone.md` => `# CLOSEGONE`) and
/// [`NEWBUF_FILE`] (`nbase.md` => `# NEWBUFBASE`): put the asserted word in the
/// file's CONTENT and give the file a name whose lowercase does not contain it, so
/// only a rendered buffer can satisfy the assertion. Neither the assertion nor
/// [`PICKER_OCR`] changed — this strengthens the test, it does not weaken it.
const PICKER_FILE_NAME: &str = "ptarget.md";

/// Heading written into [`PICKER_FILE_NAME`]. Once the picker opens the file,
/// nvim renders this line; lowercased, the rendered text contains [`PICKER_OCR`].
/// Deliberately UNLIKE the filename (see the note above), so neither the sidebar
/// row nor nvim's `:e` toast — both of which show `ptarget.md` — can satisfy the
/// assertion on their own.
const PICKER_HEADING: &str = "# PICKERTARGET";

/// Lowercased substring OCR must find in the screenshot to prove the picker
/// opened, matched, selected, and nvim loaded the target file.
pub(crate) const PICKER_OCR: &str = "pickertarget";

/// Text typed into the picker's search entry to filter down to the target file.
///
/// It must match [`PICKER_FILE_NAME`], which is the only file in that smoke's temp
/// cwd: as a prefix of `ptarget.md` it takes the ranker's path-component-prefix,
/// word-prefix and contiguous-substring bonuses, so the target is row 0 and plain
/// `Enter` opens it.
const PICKER_QUERY: &str = "ptarget";

// Keyboard-navigation picker smoke. The rank tiebreak alone orders the rows: `pick1.md` is row 0.
// See notes/crates/wolf-gnome/tests/smoke_headless/editor.rs.md#picker-navigation-row-order

/// First navigation-test markdown file; sorts BEFORE [`PICKER_NAV_FILE_B`] under
/// the rank tiebreaks (equal-length display, lexicographically smaller), so it is
/// the picker's row 0 for [`PICKER_NAV_QUERY`].
const PICKER_NAV_FILE_A: &str = "pick1.md";

/// Heading inside [`PICKER_NAV_FILE_A`] (row 0). Lowercased it contains
/// [`PICKER_NAV_OCR_ABSENT`]; it must NOT appear after navigating down to row 1.
const PICKER_NAV_HEADING_A: &str = "# PICKONE";

/// Second navigation-test markdown file; the picker's row 1 for
/// [`PICKER_NAV_QUERY`] (see the rank-tiebreak comment above).
const PICKER_NAV_FILE_B: &str = "pick2.md";

/// Heading inside [`PICKER_NAV_FILE_B`] (row 1 — the one we navigate to).
/// Lowercased it contains [`PICKER_NAV_OCR_PRESENT`].
const PICKER_NAV_HEADING_B: &str = "# PICKTWO";

/// Shared query that matches BOTH navigation files so the picker shows two rows.
const PICKER_NAV_QUERY: &str = "pick";

/// Lowercased substring OCR MUST find: the heading of row 1 (`pick2.md`), proving
/// `Down` selected the second row and `Enter` opened it.
pub(crate) const PICKER_NAV_OCR_PRESENT: &str = "picktwo";

/// Lowercased substring OCR must NOT find: the heading of row 0 (`pick1.md`),
/// proving navigation moved the selection off the top row before activating.
pub(crate) const PICKER_NAV_OCR_ABSENT: &str = "pickone";

// Empty-state smoke: `HOME` is set to the cwd, chained after `cage_command` to beat its default.
// See notes/crates/wolf-gnome/tests/smoke_headless/editor.rs.md#empty-state-smoke

/// Lowercased substring OCR must find to prove the empty state rendered.
///
/// Deliberately the shared word of all three strings the status page draws — its
/// title ("No folder open"), its description ("Open a folder to start editing.")
/// and its button ("Open Folder…") — so the assertion has three independent
/// chances to match rather than one.
pub(crate) const EMPTY_STATE_OCR: &str = "folder";

// New-untitled-buffer smoke. The not-contains on `newbufbase` is the only proof of replacement,
// and `nbase.md` is unlike it so neither the sidebar nor the `:e` toast can satisfy it.

/// New-buffer smoke: the file opened BEFORE the chord (the one that must vanish).
const NEWBUF_FILE: &str = "nbase.md";

/// Heading written into [`NEWBUF_FILE`]; lowercased it contains
/// [`NEWBUF_OCR_ABSENT`]. Deliberately unlike the filename so nvim's `:e` toast and
/// the sidebar row cannot satisfy the NOT-contains assertion.
const NEWBUF_HEADING: &str = "# NEWBUFBASE";

/// Lowercased substring OCR must NOT find: the heading of the file that was open
/// before Ctrl+Shift+N.
pub(crate) const NEWBUF_OCR_ABSENT: &str = "newbufbase";

/// Distinctive marker typed (in insert mode) into the NEW untitled buffer. Its
/// lowercased form is [`NEWBUF_OCR_PRESENT`]. Short/all-uppercase to survive OCR,
/// mirroring `FOCUS_MARKER`.
const NEWBUF_MARKER: &str = "ZZFRESH";

/// Lowercased substring OCR MUST find: proof the new untitled buffer exists and
/// received the typed text.
pub(crate) const NEWBUF_OCR_PRESENT: &str = "zzfresh";

// Command-palette smoke: Enter runs New Buffer only because the tie-break ranks it first.
// See notes/crates/wolf-gnome/tests/smoke_headless/editor.rs.md#palette-smoke-query-order

/// Command-palette smoke: the file opened BEFORE the chord (the one that must
/// vanish when the palette runs New).
pub(crate) const PALETTE_FILE: &str = "pbase.md";

/// Heading written into [`PALETTE_FILE`]; lowercased it contains
/// [`PALETTE_OCR_ABSENT`]. Deliberately unlike the filename so nvim's `:e` toast and
/// the sidebar row cannot satisfy the NOT-contains assertion.
const PALETTE_HEADING: &str = "# PALCMDBASE";

/// Lowercased substring OCR must NOT find: the heading of the file that was open
/// before the palette dispatched `NewBuffer`.
pub(crate) const PALETTE_OCR_ABSENT: &str = "palcmdbase";

/// Distinctive marker typed (in insert mode) into the buffer the palette's command
/// created. Its lowercased form is [`PALETTE_OCR_PRESENT`]. Short/all-uppercase to
/// survive OCR, mirroring [`NEWBUF_MARKER`].
const PALETTE_MARKER: &str = "ZZPALRUN";

/// Lowercased substring OCR MUST find: proof that the text typed after the palette
/// closed reached a buffer at all. It does NOT prove focus was returned to the
/// editor — that revert was measured green; see the revert table on
/// [`command_palette_chord_runs_the_selected_command_under_cage`].
pub(crate) const PALETTE_OCR_PRESENT: &str = "zzpalrun";

/// Typed into the palette's search entry. See the block comment above for why the
/// row Enter activates is decided by a tie-break rather than by uniqueness, and for
/// the unit test that pins the order and the one MRU state that inverts it.
const PALETTE_QUERY: &str = "new";

/// The canonical `CommandSpec::id` the activated row must record in the command MRU
/// (`<state dir>/command-mru.sqlite`). Nothing but a palette activation writes that
/// store — `wolf_core::api`'s `command_mru()` doc states it outright — so a hit for
/// this key can only have come from the palette running the row.
const PALETTE_MRU_KEY: &str = "new_buffer";

// Buffer-picker fixtures: three files, so the queried target is never the preselected row 1.
// See notes/crates/wolf-gnome/tests/smoke_headless/editor.rs.md#buffer-picker-fixtures

/// Buffer-picker smoke: the file opened FIRST, and the one [`BUFPICK_QUERY`] must
/// select. Opened first, so it is the OLDEST of the three by `lastused` and lands
/// at index 2 or later in the picker's MRU-first list.
pub(crate) const BUFPICK_FILE_TARGET: &str = "bpzulu.md";

/// Heading written into [`BUFPICK_FILE_TARGET`]; lowercased, the rendered line
/// contains [`BUFPICK_OCR_PRESENT`]. Deliberately unlike the filename, so neither
/// the sidebar row nor nvim's `:e` toast can satisfy the assertion on its own.
const BUFPICK_HEADING_TARGET: &str = "# BUFPICKALFA";

/// Lowercased substring the QUERY smoke's OCR must find, and the BARE-ENTER smoke's
/// must NOT: the heading of the buffer [`BUFPICK_QUERY`] selects.
///
/// It lives only in that file's CONTENT, so a hit can only mean the active window
/// really was switched to it. In
/// [`buffer_picker_bare_enter_switches_to_the_previous_buffer_under_cage`] no query
/// is typed at all, so this heading appearing would mean the preselection landed on
/// row 2 or deeper instead of on row 1.
pub(crate) const BUFPICK_OCR_PRESENT: &str = "bufpickalfa";

/// Buffer-picker smoke: the file opened SECOND — the row
/// `buffer_picker::initial_selection` preselects, and therefore the row a picker
/// that ignored the typed query would activate.
pub(crate) const BUFPICK_FILE_PRESELECT: &str = "bpyankee.md";

/// Heading written into [`BUFPICK_FILE_PRESELECT`]; lowercased it contains
/// [`BUFPICK_OCR_ABSENT`].
const BUFPICK_HEADING_PRESELECT: &str = "# BUFPICKBRAVO";

/// Lowercased substring the QUERY smoke's OCR must NOT find, and the BARE-ENTER
/// smoke's MUST: the heading of the PRESELECTED row.
///
/// The name records the direction the first smoke to use it asserts. In
/// [`buffer_picker_chord_switches_to_the_queried_buffer_under_cage`] its
/// contribution is naming WHICH wrong buffer was activated, not detecting that one
/// was — with a single pane, two headings cannot both be on screen, so no revert
/// reddens that assertion alone. In
/// [`buffer_picker_bare_enter_switches_to_the_previous_buffer_under_cage`] the same
/// term is the PRIMARY discriminator: with no query typed, this heading is on screen
/// if and only if `buffer_picker::initial_selection`'s row-1 preselection reached the
/// `ListBox` and `Enter` acted on it. See the revert tables on both.
pub(crate) const BUFPICK_OCR_ABSENT: &str = "bufpickbravo";

/// Buffer-picker smoke: the file opened THIRD — the buffer showing when the chord
/// fires, and therefore row 0 of the picker's MRU-first list.
pub(crate) const BUFPICK_FILE_CURRENT: &str = "bpxray.md";

/// Heading written into [`BUFPICK_FILE_CURRENT`]; lowercased it contains
/// [`BUFPICK_OCR_CURRENT`].
///
/// The QUERY smoke does not assert on it and only names it in failure messages, so a
/// capture showing this heading reads as "the picker did nothing at all" rather than
/// as an unexplained miss. The BARE-ENTER smoke asserts its ABSENCE, which is what
/// says a switch happened at all.
const BUFPICK_HEADING_CURRENT: &str = "# BUFPICKCHARLIE";

/// Lowercased substring the BARE-ENTER smoke's OCR must NOT find: the heading of
/// [`BUFPICK_FILE_CURRENT`], the buffer the editor is ALREADY showing when the chord
/// fires.
///
/// It is the term that separates "the preselection switched the buffer" from "the
/// picker closed on nothing and left the pane exactly as it was" — two states that
/// are otherwise indistinguishable in a capture, since both simply lack the
/// preselected row's heading.
///
/// It is asserted only in the post-Enter capture, taken after the overlay has closed
/// (`activate_selection` closes on a successful switch, and the scenario's `o` +
/// Escape only reach Neovim once it has). That matters: while the overlay IS open its
/// opaque box sits over the top of the editor page and can occlude part of this
/// heading, so the term's absence would not be evidence of anything. The failure
/// message says so rather than leaving it to be re-derived.
pub(crate) const BUFPICK_OCR_CURRENT: &str = "bufpickcharlie";

/// Typed into the buffer picker's search entry.
///
/// `wolf_core::rank_matches` keeps every candidate the query is a case-insensitive
/// SUBSEQUENCE of, and of the four listed buffers (`Untitled.md` plus the three
/// fixtures) only [`BUFPICK_FILE_TARGET`] contains a `z` after its `bp`. That the
/// list therefore holds exactly one row is pinned in the fast suite by
/// `buffer_picker::tests::buffer_rows_for_the_smoke_query_select_exactly_the_target`,
/// not left as prose here.
const BUFPICK_QUERY: &str = "bpzulu";

/// Name, in each buffer-picker smoke's own temp cwd, of the file the captive Neovim
/// writes its own `getbufinfo({'buflisted': 1})` into — one `bufnr lastused tail`
/// line per listed buffer.
///
/// One const for both smokes, and one row in `RELAUNCH_FIXTURE_NAMES`: each runs in
/// a FRESH temp cwd, so the two files never meet, and a second row carrying the same
/// name would trip that guard's distinct-names check.
///
/// It is each smoke's PREMISE, read back rather than assumed: the Rust side sorts
/// those lines by the same rule `rank_buffers` uses (lastused DESC, bufnr ASC) and
/// asserts the resulting order really does put the CURRENT file at index 0, the
/// PRESELECT file at index 1 and the target below both. Without it, a picker that
/// ignored the query would activate the preselected row — and if the three `:e`s had
/// landed inside one `lastused` second, that row could BE the target, making the
/// query smoke a permanent silent pass; the bare-Enter smoke would meanwhile be
/// asserting on whichever row the collision happened to promote.
pub(crate) const BUFPICK_READBACK_NAME: &str = "bp_buffers.txt";

/// The whole interactive scenario, run as a single `sh -c` string INSIDE `cage`
/// so that `wtype` and `grim` inherit cage's `WAYLAND_DISPLAY` (cage exports it
/// only for its child process tree).
///
/// Design notes:
/// - Launch the app in the background and capture its pid so we can kill it.
/// - Wait with `wait_ready` (see [`READINESS_PRELUDE`]), which primes the seat's
///   keyboard and then waits until the app reports that its window accepts
///   keystrokes. It exits 11 if the app dies first and 40 if the app never becomes
///   ready, so nothing is typed into a window that cannot take it.
/// - Type `ihello` (enter insert mode + the word), then `Escape`.
/// - MOVE THE CURSOR OFF ROW 0 before screenshotting: send a normal-mode `o`
///   (open a new line BELOW and enter insert on the now-empty row 1), then
///   `Escape` again. Neovim's block cursor renders as an inverted block over
///   whatever cell it occupies; left on row 0 it sits atop a letter of `hello`
///   and distorts OCR of the very text we assert (e.g. `hello` -> `hbehe`).
///   Parking it on the empty row 1 leaves `hello` on row 0 fully unobscured, so
///   OCR can match the whole word. The text itself renders fine either way; this
///   is purely to keep the cursor block off the asserted glyphs.
/// - Then a brief settle before screenshotting so the redraw lands.
/// - `kill` is best-effort (the app may already be gone); we do not let its
///   failure abort the scenario, hence `set +e` around the teardown.
pub(crate) const SCENARIO: &str = r#"
set -eu

# WAIT_INTERVAL paces this file's polls (wait_file, the quit polls).
#
# SETTLE -- this is the CANONICAL copy of this comment; the other scenarios carry
# the same value and point back here. SETTLE is the settle BETWEEN steps, not a
# launch budget: no scenario sends input before `wait_ready` (the prelude
# cage_command prepends) has seen the app write its own pid to $WOLF_READY_FILE,
# which it does only once its launch windows accept keystrokes. A cold shader
# cache or a loaded host therefore costs time in that wait rather than lost keys.
# What SETTLE still covers is the gap between a step and the next one that depends
# on it: a redraw before a capture, an overlay mapping before its query.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome smoke"

# Enter insert mode and type the word, then leave insert mode.
wtype -- "i$OCR_WORD"
wtype -k Escape

# Brief settle between keystroke steps so each is processed in order.
sleep "$SETTLE"

# Move the cursor OFF row 0 so its inverted block does not sit atop (and OCR-
# distort) the asserted word. In normal mode `o` opens a new line BELOW and
# enters insert on the empty row 1; Escape then parks the cursor there, leaving
# "$OCR_WORD" on row 0 cursor-free.
wtype -- o
wtype -k Escape

# Let the redraw of the typed text land before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The interactive PICKER scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`SCENARIO`]): launch the app, wait until it is ready for input
/// (`wait_ready`), open the picker with the Ctrl+Shift+O trigger, type a query that
/// matches the target file, activate the selection with Enter, settle, and screenshot.
///
/// The app derives `CoreConfig.cwd` from `std::env::current_dir()`, so the target
/// `.md` is placed in the cage process's working directory (set via
/// `Command::current_dir` by the caller) — NOT created from inside this script.
const PICKER_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome picker smoke"

# Open the picker with the trigger chord Ctrl+Shift+O. That chord is the
# `win.go-to-file` accelerator; Ctrl+Shift+P opens the command palette.
#
# Note: `-M <mod>` / `-m <mod>` press / release a modifier, and `-k <key>`
# presses+releases a key while the held modifiers apply, so the chord is
# "hold ctrl, hold shift, tap o, release shift, release ctrl". Chord delivery is
# isolated to this single line; the rest of the scenario does not depend on it.
wtype -M ctrl -M shift -k o -m shift -m ctrl

# Let the picker map + grab keyboard focus on its search entry.
sleep "$SETTLE"

# Type the query so the ranked list narrows to the target file.
wtype -- "$PICKER_QUERY"

# Let the streamed results rank/settle on the match.
sleep "$SETTLE"

# Activate the selection: sends InputEvent::Open{path} and closes the picker,
# which makes nvim `:e` the file.
wtype -k Return

# Let nvim load the file and redraw before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The interactive picker KEYBOARD-NAVIGATION scenario, run as a single `sh -c` string
/// INSIDE `cage` (same structure as [`PICKER_SCENARIO`]): launch the app, wait until it
/// is ready for input (`wait_ready`), open the picker with Ctrl+Shift+O, type a query
/// that matches BOTH navigation files, press `Down` once to move the list selection to
/// the SECOND row, then `Enter` to open it, settle, and screenshot.
///
/// Two files are created in the cage process's working directory by the caller
/// (`pick1.md` => row 0, `pick2.md` => row 1; see the rank-tiebreak comment near
/// the `PICKER_NAV_*` consts). Down+Enter must therefore open `pick2.md`.
const PICKER_NAV_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome picker nav smoke"

# Open the picker with the trigger chord Ctrl+Shift+O (same chord as the other
# picker scenario; see its note for the wtype modifier syntax).
wtype -M ctrl -M shift -k o -m shift -m ctrl

# Let the picker map + grab keyboard focus on its search entry.
sleep "$SETTLE"

# Type the shared query so the ranked list shows BOTH matching rows.
wtype -- "$PICKER_NAV_QUERY"

# Let the streamed results rank/settle into the two-row list.
sleep "$SETTLE"

# Move the LIST selection down one row (row 0 -> row 1).
#
# Note: the picker routes Up/Down to the ListBox selection rather than the
# SearchEntry text cursor (focus stays in the textbox), and `Down` is the keyval
# name wtype emits for the arrow key. If the selection ever stops moving, this
# test's `picktwo` assertion is what catches it.
wtype -k Down

# Brief settle so the selection change is processed before activating.
sleep "$SETTLE"

# Activate the SELECTED row: sends InputEvent::Open{path} for row 1 and closes
# the picker, which makes nvim `:e` the second file.
#
# Note: `Return` is wtype's key name for Enter (matches the other picker
# scenario).
wtype -k Return

# Let nvim load the file and redraw before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The EMPTY-STATE scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as `TREE_SCENARIO`): launch the app, wait until it is ready for input
/// (`wait_ready`), and screenshot. No keyboard interaction at all — the assertion is
/// purely that the "no folder open" status page rendered.
///
/// The caller launches cage with `HOME` set to the same fresh temp directory it
/// uses as the working directory, which is what makes `workspace::launch_decision`
/// take its `cwd == home` branch (see the notes near [`EMPTY_STATE_OCR`]). That is
/// an explicit OVERRIDE of [`cage_command`]'s default `HOME` (the cwd's parent),
/// chained after it so it wins.
const EMPTY_STATE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome empty-state smoke"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The NEW-UNTITLED-BUFFER scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as `VSPLIT_SCENARIO`): launch the app, wait until it is ready for input
/// (`wait_ready`), open the base file (`:e $NEWBUF_FILE<CR>`), fire the Ctrl+Shift+N
/// accelerator, type the marker into the fresh buffer, park the cursor off the marker's
/// row, settle, and screenshot. The base file lives in the cage process's working
/// directory (set via `Command::current_dir` by the caller).
const NEWBUF_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome new-buffer smoke"

# Open the base file so there is something DISTINCTIVE on screen to displace.
wtype -- ":e $NEWBUF_FILE"
wtype -k Return

# Let nvim load the base file and redraw before firing the accelerator.
sleep "$SETTLE"

# Fire the New (untitled buffer) accelerator Ctrl+Shift+N. GTK captures the chord
# before the window key controller, activates `win.new-buffer`, and the shell sends
# InputEvent::NewBuffer -> `wolf_new_untitled()` (:enew + "Untitled N.md").
#
# Note: `-M <mod>` / `-m <mod>` press / release a modifier and `-k <key>`
# presses+releases a key while the held modifiers apply, exactly as in the picker
# scenarios. Chord delivery is isolated to this single line.
wtype -M ctrl -M shift -k n -m shift -m ctrl

# Let the new buffer be created and painted before typing into it.
sleep "$SETTLE"

# Type the distinctive marker into the fresh buffer, then leave insert mode.
wtype -- "i$NEWBUF_MARKER"
wtype -k Escape

# Brief settle between keystroke steps so each is processed in order.
sleep "$SETTLE"

# Move the cursor OFF the marker's row so its inverted block does not sit atop (and
# OCR-distort) the asserted marker -- the SCENARIO / FOCUS_SCENARIO precedent. In
# normal mode `o` opens a new line BELOW and enters insert on it; Escape parks the
# cursor there.
wtype -- o
wtype -k Escape

# Let the redraw of the typed text land before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The COMMAND-PALETTE scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`NEWBUF_SCENARIO`]): launch the app, wait until it is ready for input
/// (`wait_ready`), open the base file (`:e $PALETTE_FILE<CR>`), fire the Ctrl+Shift+P
/// accelerator, type the query, press Enter to activate the sole ranked row, then type
/// the marker into the buffer that row's command created, park the cursor off the
/// marker's row, settle, and screenshot. The base file lives in the cage process's
/// working directory (set via `Command::current_dir` by the caller).
const PALETTE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome command-palette smoke"

# Open the base file so there is something DISTINCTIVE on screen for the palette's
# command to displace.
wtype -- ":e $PALETTE_FILE"
wtype -k Return

# Let nvim load the base file and redraw before firing the accelerator.
sleep "$SETTLE"

# Fire the command-palette accelerator Ctrl+Shift+P. The capture-phase chord
# controller resolves it through `actions::action_for_key`, activates
# `win.command-palette`, and the shell performs UiAction::OpenCommandPalette.
#
# Note: `-M <mod>` / `-m <mod>` press / release a modifier and `-k <key>`
# presses+releases a key while the held modifiers apply, exactly as in the picker
# and new-buffer scenarios. Chord delivery is isolated to this single line.
wtype -M ctrl -M shift -k p -m shift -m ctrl

# Let the palette map and grab keyboard focus on its search entry.
sleep "$SETTLE"

# Type the query so the ranked list narrows to the single actionable row.
wtype -- "$PALETTE_QUERY"

# Let the SearchEntry's own search delay fire and the list re-rank. That delay lives
# inside this budget, exactly as it does in PICKER_SCENARIO.
sleep "$SETTLE"

# Activate the sole row: records its MRU key, closes the palette, returns focus to
# the editor's cmdline strip, and dispatches the command through the shell.
wtype -k Return

# Let the dispatched command run and the pane repaint before typing into it.
sleep "$SETTLE"

# Type the distinctive marker into the buffer the command created, then leave insert
# mode. These keystrokes only reach nvim once the palette has closed.
wtype -- "i$PALETTE_MARKER"
wtype -k Escape

# Brief settle between keystroke steps so each is processed in order.
sleep "$SETTLE"

# Move the cursor OFF the marker's row so its inverted block does not sit atop (and
# OCR-distort) the asserted marker -- the SCENARIO / FOCUS_SCENARIO precedent. In
# normal mode `o` opens a new line BELOW and enters insert on it; Escape parks the
# cursor there.
wtype -- o
wtype -k Escape

# Let the redraw of the typed text land before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The BUFFER-PICKER scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`PALETTE_SCENARIO`]): launch the app, wait until it is ready for input
/// (`wait_ready`), open the three fixtures in a known order with a gap between them,
/// read the resulting buffer list back out of Neovim, fire Ctrl+Shift+E, type the
/// query, press Enter, park the block cursor off the asserted row, settle, and
/// screenshot.
///
/// Standalone — it does NOT need `with_nvim_pid_helper`, because nothing here
/// identifies or signals the Neovim child.
///
/// ## `MRU_GAP` is not a settle, and it must exceed one second
///
/// `getbufinfo().lastused` is a WHOLE-SECOND Unix timestamp. Three `:e`s inside one
/// second all tie, and `wolf_core::rank_buffers` then falls back to `bufnr` ASC —
/// which INVERTS the MRU order: the startup buffer takes row 0 and the FIRST-opened
/// file, the query's target, takes row 1. Row 1 is exactly the row
/// `buffer_picker::initial_selection` preselects, so the collision does not merely
/// reorder the list — it makes a picker that ignores the typed query activate the
/// target anyway, i.e. a permanent silent pass rather than a flake. The gap is what
/// buys distinct seconds; the readback in step 3 is what proves it worked on this
/// run.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `14` — the buffer-list readback the captive Neovim was told to write never
///   appeared, so the premise below cannot be checked at all.
const BUFPICK_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for the readback file.
FILE_TRIES=100
#
# MRU_GAP -- READ THIS BEFORE TRIMMING IT. It is NOT a settle.
#
# getbufinfo().lastused is a WHOLE-SECOND unix timestamp, so two `:e`s inside one
# second tie and rank_buffers falls back to bufnr ASC -- which INVERTS the MRU
# order and puts the query's target on row 1, the row initial_selection preselects.
# The smoke would then pass with the picker ignoring the query entirely. It must
# therefore exceed 1.0 with margin; the readback in step 3 is what proves it did.
MRU_GAP=1.2

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome buffer-picker smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there. (The CRASH_SCENARIO body, verbatim.)
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
    echo "wolf-gnome buffer-picker smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 2: open the three fixtures, OLDEST FIRST --------------------------
#
# The order is the whole fixture. TARGET first makes it the oldest by `lastused`
# and therefore index 2 or later in the picker's MRU-first list; CURRENT last makes
# it row 0. PRESELECT lands in between, at row 1, which is the row
# buffer_picker::initial_selection preselects -- so it is the row a picker that
# ignored the typed query would activate, and the smoke asserts its heading is
# ABSENT.
#
# The MRU_GAP sleeps are load-bearing, not settles: see the knob comment above.
wtype -- ":e $BUFPICK_FILE_TARGET"
wtype -k Return
sleep "$MRU_GAP"

wtype -- ":e $BUFPICK_FILE_PRESELECT"
wtype -k Return
sleep "$MRU_GAP"

wtype -- ":e $BUFPICK_FILE_CURRENT"
wtype -k Return
sleep "$MRU_GAP"

# --- Step 3: the PREMISE, read back out of Neovim ---------------------------
#
# One `bufnr lastused tail` line per listed buffer -- exactly the fields
# wolf_core::rank_buffers orders by, so the Rust side can re-derive the picker's
# list order from Neovim's own answer instead of assuming the sleeps above worked.
wtype -- ":call writefile(map(getbufinfo({'buflisted':1}), {_, b -> printf('%d %d %s', b.bufnr, b.lastused, fnamemodify(b.name, ':t'))}), '$BUFPICK_READBACK')"
wtype -k Return
wait_file "$BUFPICK_READBACK" || exit 14

# --- Step 4: fire the buffer-picker accelerator Ctrl+Shift+E ----------------
#
# The capture-phase chord controller resolves it through `actions::action_for_key`,
# activates `win.buffer-picker`, and the shell performs UiAction::OpenBufferPicker,
# which snapshots the listed buffers with ONE blocking RPC and shows the overlay.
#
# Note: `-M <mod>` / `-m <mod>` press / release a modifier and `-k <key>`
# presses+releases a key while the held modifiers apply, exactly as in the picker,
# new-buffer and palette scenarios. Chord delivery is isolated to this single line.
wtype -M ctrl -M shift -k e -m shift -m ctrl

# Let the overlay map and grab keyboard focus on its search entry.
sleep "$SETTLE"

# Type the query so the ranked list narrows to the single matching buffer.
wtype -- "$BUFPICK_QUERY"

# Let the SearchEntry's own search delay fire and the list re-rank. That delay lives
# inside this budget, exactly as it does in PICKER_SCENARIO.
sleep "$SETTLE"

# Activate the selection: resolves the active window, sends
# InputEvent::SwitchBuffer for the ranked row's bufnr, and closes the overlay.
wtype -k Return

# Let nvim switch the buffer and both the pane and the cmdline strip redraw.
sleep "$SETTLE"

# Move the cursor OFF the asserted heading's row so its inverted block does not sit
# atop (and OCR-distort) the asserted word -- the SCENARIO / PALETTE_SCENARIO
# precedent. In normal mode `o` opens a new line BELOW and enters insert on it;
# Escape parks the cursor there. These keystrokes only reach nvim once the overlay
# has closed, so they also corroborate that it did.
wtype -- o
wtype -k Escape

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

/// The BARE-ENTER buffer-picker scenario, run as a single `sh -c` string INSIDE
/// `cage`.
///
/// [`BUFPICK_SCENARIO`] up to and including the chord, and then deliberately
/// different: **no query is typed**, and `Enter` is pressed immediately. Everything
/// else — the same three `:e`s in the same order, the same `MRU_GAP`, the same
/// readback, and the same cursor parking before the capture — is reused verbatim, so
/// the two captures differ in exactly one input.
///
/// Standalone: like [`BUFPICK_SCENARIO`] it never identifies or signals the Neovim
/// child, so it does not need `with_nvim_pid_helper`.
///
/// ## What the missing query buys
///
/// With an empty search entry `wolf_core::rank_buffers` returns every listed buffer
/// MRU-first, so the picker's first refresh takes `buffer_picker::initial_selection`
/// rather than `restored_index` — the branch the query scenario never reaches,
/// because typing re-enters `refresh` with `initial_open` already cleared. `Enter`
/// then activates row 1, the most-recently-used OTHER buffer.
///
/// ## `MRU_GAP` is not a settle, and it must exceed one second
///
/// Same reason as in [`BUFPICK_SCENARIO`], with one clause more. `lastused` is a
/// WHOLE-SECOND Unix timestamp; three `:e`s inside one second tie, `rank_buffers`
/// falls back to `bufnr` ASC, and the order inverts. Here that does not merely move
/// the target — it moves the row `initial_selection` PRESELECTS, which is the row
/// this smoke's must-contain term is written against. The readback in step 3 is what
/// proves the gap did its job on this run.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `14` — the buffer-list readback the captive Neovim was told to write never
///   appeared, so the premise below cannot be checked at all.
const BUFPICK_BARE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for the readback file.
FILE_TRIES=100
#
# MRU_GAP -- READ THIS BEFORE TRIMMING IT. It is NOT a settle.
#
# getbufinfo().lastused is a WHOLE-SECOND unix timestamp, so two `:e`s inside one
# second tie and rank_buffers falls back to bufnr ASC -- which INVERTS the MRU
# order. This smoke asserts on the heading of the file it expects at ROW 1, the row
# initial_selection preselects, so an inverted order makes it assert against the
# wrong file entirely. It must therefore exceed 1.0 with margin; the readback in
# step 3 is what proves it did.
MRU_GAP=1.2

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome buffer-picker bare-Enter smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there. (The CRASH_SCENARIO body, verbatim.)
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
    echo "wolf-gnome buffer-picker bare-Enter smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 2: open the three fixtures, OLDEST FIRST --------------------------
#
# Identical to BUFPICK_SCENARIO, and identical on purpose: the two smokes differ in
# exactly one input, the query. TARGET first makes it the oldest by `lastused` and
# therefore index 2 or later; CURRENT last makes it row 0, the buffer on screen when
# the chord fires. PRESELECT lands at row 1 -- the row initial_selection preselects,
# and the one this smoke asserts we end up in.
#
# The MRU_GAP sleeps are load-bearing, not settles: see the knob comment above.
wtype -- ":e $BUFPICK_FILE_TARGET"
wtype -k Return
sleep "$MRU_GAP"

wtype -- ":e $BUFPICK_FILE_PRESELECT"
wtype -k Return
sleep "$MRU_GAP"

wtype -- ":e $BUFPICK_FILE_CURRENT"
wtype -k Return
sleep "$MRU_GAP"

# --- Step 3: the PREMISE, read back out of Neovim ---------------------------
#
# One `bufnr lastused tail` line per listed buffer -- exactly the fields
# wolf_core::rank_buffers orders by, so the Rust side can re-derive the picker's
# list order from Neovim's own answer instead of assuming the sleeps above worked.
wtype -- ":call writefile(map(getbufinfo({'buflisted':1}), {_, b -> printf('%d %d %s', b.bufnr, b.lastused, fnamemodify(b.name, ':t'))}), '$BUFPICK_READBACK')"
wtype -k Return
wait_file "$BUFPICK_READBACK" || exit 14

# --- Step 4: fire the buffer-picker accelerator Ctrl+Shift+E ----------------
#
# The capture-phase chord controller resolves it through `actions::action_for_key`,
# activates `win.buffer-picker`, and the shell performs UiAction::OpenBufferPicker,
# which snapshots the listed buffers with ONE blocking RPC and shows the overlay.
#
# Note: `-M <mod>` / `-m <mod>` press / release a modifier and `-k <key>`
# presses+releases a key while the held modifiers apply, exactly as in the picker,
# new-buffer, palette and query buffer-picker scenarios. Chord delivery is isolated
# to this single line.
wtype -M ctrl -M shift -k e -m shift -m ctrl

# Let the overlay map and grab keyboard focus on its search entry.
sleep "$SETTLE"

# --- Step 5: NO QUERY. Enter, immediately -----------------------------------
#
# This is the whole difference from BUFPICK_SCENARIO. The search entry is still
# empty, so the list is every listed buffer MRU-first and the selection is whatever
# the FIRST refresh after the open chose -- i.e. initial_selection's row 1. Do not
# add a settle-and-type here "for symmetry" with the query scenario: typing anything
# re-enters `refresh` with `initial_open` already cleared, which is precisely the
# branch this smoke exists to avoid.
wtype -k Return

# Let nvim switch the buffer and both the pane and the cmdline strip redraw.
sleep "$SETTLE"

# Move the cursor OFF the asserted heading's row so its inverted block does not sit
# atop (and OCR-distort) the asserted word -- the SCENARIO / PALETTE_SCENARIO
# precedent. In normal mode `o` opens a new line BELOW and enters insert on it;
# Escape parks the cursor there. These keystrokes only reach nvim once the overlay
# has closed, so they also corroborate that it did.
wtype -- o
wtype -k Escape

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

// Navigation-history smoke: the only end-to-end coverage of Alt+Left through `win.history-back`.
// See notes/crates/wolf-gnome/tests/smoke_headless/editor.rs.md#navigation-history-smoke

/// Nav-back smoke: the file opened FIRST, which Back must return to.
///
/// Deliberately unlike [`NAVBACK_OCR_PRESENT`]: this NAME is drawn by the sidebar
/// row and by nvim's `:e` toast, so a name carrying the asserted word would satisfy
/// the assertion with nothing having navigated.
pub(crate) const NAVBACK_FILE_FIRST: &str = "nbxray.md";

/// Heading written into [`NAVBACK_FILE_FIRST`]; lowercased it contains
/// [`NAVBACK_OCR_PRESENT`]. This line renders only once the pane really holds that
/// buffer again, which is what makes it the honest evidence Back moved.
const NAVBACK_HEADING_FIRST: &str = "# NAVBACKALFA";

/// Lowercased substring OCR MUST find after Alt+Left: the heading of the file the
/// pane was on BEFORE the second `:e`.
pub(crate) const NAVBACK_OCR_PRESENT: &str = "navbackalfa";

/// Nav-back smoke: the file opened SECOND — the one Back must leave.
pub(crate) const NAVBACK_FILE_SECOND: &str = "nbyankee.md";

/// Heading written into [`NAVBACK_FILE_SECOND`]; lowercased it contains
/// [`NAVBACK_OCR_ABSENT`].
const NAVBACK_HEADING_SECOND: &str = "# NAVBACKBRAVO";

/// Lowercased substring OCR must NOT find after Alt+Left: the heading of the file
/// the pane navigated AWAY from.
pub(crate) const NAVBACK_OCR_ABSENT: &str = "navbackbravo";

/// Nav-back smoke: the readback the captive Neovim writes with `bufname('%')`
/// BEFORE the chord fires. Its presence proves the second `:e` took effect.
pub(crate) const NAVBACK_READBACK_BEFORE: &str = "nb_before.txt";

/// Nav-back smoke: the readback the captive Neovim writes with `bufname('%')`
/// AFTER the chord fires. This is the smoke's discriminator, and unlike OCR it is
/// an exact string no sidebar row and no `:e` toast can forge.
pub(crate) const NAVBACK_READBACK_AFTER: &str = "nb_after.txt";

/// The NAV-BACK scenario, run as a single `sh -c` string INSIDE `cage` (the
/// [`SCENARIO`] shape): launch the app, wait until it is ready for input
/// (`wait_ready`), `:e` the two fixtures in order, read `bufname('%')` back, fire
/// Alt+Left, read `bufname('%')` back again, park the block cursor off the asserted
/// row, settle, and screenshot.
///
/// Standalone — it does NOT need `with_nvim_pid_helper`, because nothing here
/// identifies or signals the Neovim child.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `24` — the BEFORE `bufname('%')` readback never appeared, so the premise that
///   the second `:e` landed cannot be checked at all.
/// * `25` — the AFTER `bufname('%')` readback never appeared. Distinct from `24` on
///   purpose: `24` means keys were never reaching Neovim, while `25` means they
///   were and then stopped — which is what a chord that wedged the app looks like.
const NAVBACK_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for a readback file.
FILE_TRIES=100

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome nav-back smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there. (The CRASH_SCENARIO body, verbatim.)
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
    echo "wolf-gnome nav-back smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 2: build the pane's history, FIRST then SECOND --------------------
#
# Two `:e`s in one pane give it a two-entry navigation stack on top of the startup
# buffer, so Back has somewhere to go and the file it goes to is known.
wtype -- ":e $NAVBACK_FILE_FIRST"
wtype -k Return
sleep "$SETTLE"

wtype -- ":e $NAVBACK_FILE_SECOND"
wtype -k Return
sleep "$SETTLE"

# --- Step 3: the BEFORE state, read out of Neovim ---------------------------
#
# A PRECONDITION, not a discriminator: it proves the second `:e` took effect, so a
# failure of the AFTER readback below can be attributed to the navigation rather
# than to a keystroke that never landed.
wtype -- ":call writefile([bufname('%')], '$BEFORE')"
wtype -k Return
wait_file "$BEFORE" || exit 24

# --- Step 4: the chord under test -------------------------------------------
#
# `-M <mod>` / `-m <mod>` press / release a modifier and `-k <key>` presses+releases
# a key while the held modifiers apply, exactly as in the Ctrl+Shift chord
# scenarios. Chord delivery is isolated to this single line.
#
# If the action were NOT installed, `AppShell::install_chord_controller` returns
# Propagation::Proceed and this reaches Neovim as <A-Left>, which unmapped splits
# into <Esc> + <Left> -- so a broken build does not merely fail to navigate, it
# types into the buffer. The AFTER readback below is what tells the two apart.
wtype -M alt -k Left -m alt

# Let the blocking history move and the repaint land.
sleep "$SETTLE"

# --- Step 5: the AFTER state, read out of Neovim ----------------------------
#
# THE DISCRIMINATOR. An exact buffer name, which no sidebar row and no `:e` toast
# can forge and which OCR is structurally incapable of answering precisely.
wtype -- ":call writefile([bufname('%')], '$AFTER')"
wtype -k Return
wait_file "$AFTER" || exit 25

# --- Step 6: park the block cursor off the asserted row ---------------------
#
# The inverted cursor block corrupts the cell it covers, and the asserted heading is
# on row 0. In normal mode `o` opens a new line BELOW and enters insert on it;
# Escape parks the cursor there (the SCENARIO precedent).
wtype -- o
wtype -k Escape

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

/// Reads the buffer-picker smoke's MRU readback — one `bufnr lastused tail` line
/// per `buflisted` buffer — and returns it as `(bufnr, lastused, tail)` SORTED by
/// the rule `wolf_core::rank_buffers` uses: `lastused` DESCending, ties broken by
/// `bufnr` ASCending.
///
/// Sorting here rather than trusting the file's own order is the point of the
/// helper. `getbufinfo()` answers in `bufnr` order, which is the order the buffers
/// were CREATED in; what the picker shows is the MRU order derived from it. Applying
/// the production rule to Neovim's own numbers is what makes the caller's check a
/// premise about the run rather than a restatement of the order the scenario typed
/// its `:e`s in — and it is why a collided whole-second `lastused` shows up as a
/// named, actionable failure instead of as a mysterious OCR miss.
fn read_buffer_mru_order(path: &Path, smoke: &str) -> Vec<(i64, i64, String)> {
    const LINE_TOKENS: usize = 3;

    let raw = read_readback(path, smoke);
    let mut rows: Vec<(i64, i64, String)> = Vec::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        assert!(
            tokens.len() == LINE_TOKENS,
            "{smoke}: buffer readback line {line:?} in {} holds {} whitespace-separated token(s), \
             expected {LINE_TOKENS} (bufnr, lastused, filename tail). Raw contents were {raw:?}. \
             Each line is one `printf('%d %d %s', …)` over a `getbufinfo({{'buflisted':1}})` \
             entry, so a different shape means the typed Ex command did not execute as written — \
             a stray keystroke landing in the buffer rather than the cmdline is the usual cause. \
             A tail containing a SPACE would also land here, which no fixture in this binary has.",
            path.display(),
            tokens.len()
        );
        let bufnr = parse_readback_int(tokens[0], path, &raw, smoke);
        let lastused = parse_readback_int(tokens[1], path, &raw, smoke);
        rows.push((bufnr, lastused, tokens[2].to_string()));
    }

    assert!(
        !rows.is_empty(),
        "{smoke}: the buffer readback {} parsed to NO rows at all (raw contents {raw:?}), so every \
         ordering assertion built on it would run zero times. The scenario polls for a non-empty \
         file before continuing, so an empty parse here means the file held only whitespace.",
        path.display()
    );

    // The production rule, re-applied to Neovim's own numbers.
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    rows
}

#[test]
fn window_renders_typed_text_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test (Cargo sets this env var for
    // integration tests of a crate that declares a `[[bin]]`).
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Isolation is mandatory: at a fixed cwd this scenario's `kill` leaves an `Untitled.md` swap that
    // becomes the next run's fixture. See `make_temp_cwd`.
    let cwd = make_temp_cwd();

    // Page density for OCR; see `SIDEBAR_FILLER_COUNT`. Nothing asserts on these files.
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY)
            .unwrap_or_else(|e| panic!("failed to write filler {}: {e}", filler.display()));
    }

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable above) so we
    // do not depend on CWD being writable.
    let out_png = runtime_dir.join("wolf_gnome_smoke.png");
    // Start from a clean slate so a stale PNG from a previous run cannot pass.
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    let status = cage_command(SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("OCR_WORD", OCR_WORD)
        .status()
        .expect("failed to spawn `cage` for the headless smoke scenario");
    assert!(
        status.success(),
        "cage scenario exited unsuccessfully ({status}); the app may have failed to launch, \
         the window may not have mapped, or wtype/grim failed. Exit 11: the app died before it \
         was ready for input. Exit 40: it never became ready — the app's `wolf-ready: waiting \
         on …` stderr line names the condition it was stuck on, and \"wtype could not prime \
         the seat\" means no keyboard was ever created. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    // The scenario parks the cursor on row 1, so it does not sit on a letter of OCR_WORD.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(OCR_WORD),
        "OCR of the rendered window did not contain the typed word {OCR_WORD:?}. The cursor was \
         moved off row 0 so it should not obscure any letter. PNG kept at {} — OPEN IT before \
         concluding anything, and check these FOUR signatures in this order.\n\
         \n\
         (a) THE MOST LIKELY ONE, and the one to rule out FIRST: the whole page drew correctly \
         and looks exactly like a PASSING capture except that row 0 is BLANK — the buffer holds \
         exactly TWO empty lines with the block cursor on line 2, i.e. ONE row below the empty \
         row 0. That means the ENTIRE first `wtype -- \"i{OCR_WORD}\"` AND its `Escape` were \
         delivered before the client could accept keys, and were LOST; the trailing `o` + \
         `Escape` then produced the second line on their own. This is a READINESS failure, NOT a \
         rendering failure and NOT the swap modal: the scenario typed only after `wait_ready` saw \
         the app's readiness file, so the app declared itself ready while the compositor was \
         still dropping keys, and the readiness predicate is missing a conjunct. Read the app's \
         `wolf-ready:` stderr lines and `notes/wolf-gnome-decisions.md`, \"The readiness \
         file\", and find the condition that held too early. A sleep after `wait_ready` is NOT \
         the fix; it would hide which conjunct is missing.\n\
         \n\
         (b) if nothing drew at all (no window, no tildes, no cursor), it is a rendering/launch \
         failure.\n\
         \n\
         (c) if the window, sidebar, editor pane, tildes and cursor all drew correctly but row 0 \
         is EMPTY and the cursor sits TWO rows below it — THREE lines in the buffer, one row \
         lower than signature (a) — then exactly ONE keystroke, the leading `i`, was consumed \
         before Neovim saw it, so {OCR_WORD:?} executed in NORMAL mode instead of being inserted \
         (its trailing `o` is what opened the extra line). Count the lines and the cursor row \
         before choosing between (a) and (c); they are one row apart and nothing else \
         distinguishes them. (c) has two KNOWN causes. The first is a seat that had no \
         keyboard: the first key the first virtual keyboard sends after the window maps is \
         lost, which is why `wait_ready` primes the seat with `wtype \"\"` and the app waits on \
         `no-keyboard` — if that priming or that condition was removed, look there first. The \
         second is the swap modal: `nvim_buf_set_name` creates the startup buffer's swap file inside the rename, and \
         a pre-existing swap for `<cwd>/Untitled.md` raises a blocking `do_dialog` ATTENTION/E325 \
         prompt (`mode() == \"r?\"`) that answers itself with your first key. Check \
         `state_dir()/swap/<hash-of-cwd>/` for a `%…%Untitled.md.swp` — that is this run's \
         `-state` sibling of the temp cwd \
         (see `smoke_state_dir`), NOT `~/.local/state/wolf/swap/` — and see \
         `crates/wolf-core/tests/swap_recovery.rs`'s \
         `stale_untitled_swap_does_not_block_a_fresh_start`, which pins this headlessly \
         (and deterministically). Only if neither applies should you consider focus / \
         input routing (that hypothesis is DISPROVED: deferring `grab_focus()` into \
         `window.connect_map()` changes nothing). For (c) specifically it is \
         NOT a rendering failure: all but the first key reached Neovim, which is exactly what \
         separates it from (a).\n\
         \n\
         (d) if row 0 visibly DREW {OCR_WORD:?} but the OCR text below garbles it (measured: the \
         isolated leading `h` classified as the ligature U+FB01), the page went sparse or \
         run-varying and tesseract lost its calibration: see `SIDEBAR_FILLER_COUNT`, \
         `make_temp_cwd` and `cage_command`'s `HOME`. Add page text; do not touch the \
         assertion.\n\
         \n\
         OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
fn picker_opens_and_loads_selected_file_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target = cwd.join(PICKER_FILE_NAME);
    std::fs::write(&target, format!("{PICKER_HEADING}\n"))
        .unwrap_or_else(|e| panic!("failed to write picker target {}: {e}", target.display()));

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start
    // from a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_picker_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Run the whole picker scenario inside cage, with the temp dir as the working
    // directory so the app picks it up as its cwd.
    let status = cage_command(PICKER_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("PICKER_QUERY", PICKER_QUERY)
        .status()
        .expect("failed to spawn `cage` for the headless picker smoke scenario");
    assert!(
        status.success(),
        "cage picker scenario exited unsuccessfully ({status}); the app may have failed to \
         launch, the window may not have mapped, the picker trigger may not have opened it, or \
         wtype/grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage picker scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    // The heading word appears only inside the buffer, so only a rendered buffer satisfies this; see
    // `PICKER_FILE_NAME`.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(PICKER_OCR),
        "OCR of the rendered window did not contain {PICKER_OCR:?} (loose match) — the HEADING of \
         {PICKER_FILE_NAME}, which only renders once nvim has actually loaded that file. This \
         usually means the picker did not open the target: either the Ctrl+Shift+O chord never \
         reached `win.go-to-file` (Wolf matches its chords in a capture-phase \
         EventControllerKey via `actions::action_for_key`), or the query \
         {PICKER_QUERY:?} did not rank {PICKER_FILE_NAME} first. PNG kept at {} for inspection. \
         OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
fn picker_arrow_navigation_selects_second_match_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target_a = cwd.join(PICKER_NAV_FILE_A);
    let target_b = cwd.join(PICKER_NAV_FILE_B);
    std::fs::write(&target_a, format!("{PICKER_NAV_HEADING_A}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write picker nav file {}: {e}",
            target_a.display()
        )
    });
    std::fs::write(&target_b, format!("{PICKER_NAV_HEADING_B}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write picker nav file {}: {e}",
            target_b.display()
        )
    });

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start
    // from a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_picker_nav_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Run the whole picker-navigation scenario inside cage, with the temp dir as
    // the working directory so the app picks it up as its cwd.
    let status = cage_command(PICKER_NAV_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("PICKER_NAV_QUERY", PICKER_NAV_QUERY)
        .status()
        .expect("failed to spawn `cage` for the headless picker nav smoke scenario");
    assert!(
        status.success(),
        "cage picker nav scenario exited unsuccessfully ({status}); the app may have failed to \
         launch, the window may not have mapped, the picker trigger may not have opened it, the \
         Down key may not have moved the selection, or wtype/grim failed. Screenshot expected at \
         {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage picker nav scenario, but: {e}",
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
        ocr_text.contains(PICKER_NAV_OCR_PRESENT),
        "OCR did not contain {PICKER_NAV_OCR_PRESENT:?} (loose match). Expected `Down` to select \
         row 1 ({PICKER_NAV_FILE_B}) and `Enter` to open it. A miss usually means Down did not \
         move the list selection (or the rank order differed). PNG kept at {} for inspection. \
         OCR output was:\n{ocr_text}",
        out_png.display()
    );
    // Row 0's heading must be absent, so Enter did not just open row 0.
    assert!(
        !ocr_text.contains(PICKER_NAV_OCR_ABSENT),
        "OCR unexpectedly contained {PICKER_NAV_OCR_ABSENT:?}: it looks like row 0 ({PICKER_NAV_FILE_A}) \
         was opened, i.e. `Down` did NOT move the selection off the top row. PNG kept at {} for \
         inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

/// The empty state renders its "Open Folder" status page.
///
/// It is ALSO the regression guard for **"the empty state must not spuriously
/// re-root"**. `ShellInner::rooted_cwd` is seeded from the canonicalized launch cwd,
/// which in this smoke is `HOME` (the whole point of the `HOME == cwd` override
/// below); the captive Neovim reports that same directory as its live working
/// directory, and `workspace::reroot_decision`'s `live == rooted` arm must answer
/// `Stay`. A broken seed — an uncanonicalized one, or none at all — makes
/// `live != rooted` on the very first wake, which re-roots the window at `HOME` and
/// sets a `FileSearch` walking the whole home tree behind a status page that is
/// supposed to mean "no project is open".
#[test]
fn empty_state_shows_open_folder_button_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd with NO fixture files: the empty state hides the sidebar, so
    // there is nothing for filler rows to populate and nothing that asserts on them.
    let cwd = make_temp_cwd();

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start from
    // a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_empty_state_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // `HOME` equal to the cwd fires `launch_decision`'s `cwd == home` branch. Chained after the helper
    // so it overrides the default (last write wins); the title is the constant `~`.
    let status = cage_command(EMPTY_STATE_SCENARIO, &cwd, &runtime_dir)
        .env("HOME", &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .status()
        .expect("failed to spawn `cage` for the headless empty-state smoke scenario");
    assert!(
        status.success(),
        "cage empty-state scenario exited unsuccessfully ({status}); the app may have failed to \
         launch, the window may not have mapped, or grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage empty-state scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    // The word appears three times (title, description, button), which is the OCR margin.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(EMPTY_STATE_OCR),
        "OCR of the rendered window did not contain {EMPTY_STATE_OCR:?} (loose match). The likely \
         causes, in order: (a) `launch_decision` did not return EmptyState — HOME and the cwd are \
         the same temp dir here, so a Folder decision means the rule or the canonicalization is \
         wrong; (b) the empty-state overlay was built but never shown \
         (`set_empty_state(true)`); (c) it was shown but is transparent/behind the editor \
         DrawingArea, so the grid painted over it. PNG kept at {} — open it to tell those apart. \
         OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
fn new_untitled_buffer_replaces_open_file_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target = cwd.join(NEWBUF_FILE);
    std::fs::write(&target, format!("{NEWBUF_HEADING}\n"))
        .unwrap_or_else(|e| panic!("failed to write new-buffer base {}: {e}", target.display()));

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start from
    // a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_newbuf_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Run the whole new-untitled-buffer scenario inside cage, with the temp dir as
    // the working directory so the app picks it up as its cwd.
    let status = cage_command(NEWBUF_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("NEWBUF_FILE", NEWBUF_FILE)
        .env("NEWBUF_MARKER", NEWBUF_MARKER)
        .status()
        .expect("failed to spawn `cage` for the headless new-untitled-buffer smoke scenario");
    assert!(
        status.success(),
        "cage new-untitled-buffer scenario exited unsuccessfully ({status}); the app may have \
         failed to launch, the window may not have mapped, the :e may not have loaded, or \
         wtype/grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage new-buffer scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    // Both assertions are load-bearing; the NEWBUF_* comment says why the not-contains is sound.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(NEWBUF_OCR_PRESENT),
        "OCR did not contain the marker {NEWBUF_OCR_PRESENT:?} typed into the new untitled \
         buffer. The likely causes: the Ctrl+Shift+N accelerator never fired (GTK's application \
         accels are matched in the CAPTURE phase — if this fails together with both picker \
         smokes, suspect accel matching for a shifted letter), or \
         `win.new-buffer` fired but never reached `InputEvent::NewBuffer`. PNG kept at {} for \
         inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        !ocr_text.contains(NEWBUF_OCR_ABSENT),
        "OCR unexpectedly contained {NEWBUF_OCR_ABSENT:?} (the base file's heading). A hit means \
         the pane still shows {NEWBUF_FILE} — i.e. no new buffer replaced it, and the marker \
         above was typed into the ORIGINAL buffer. PNG kept at {} for inspection. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
}

/// Ctrl+Shift+P opens the command palette, a typed query narrows it to one row, and
/// Enter RUNS that row's command.
///
/// ## What it covers that no other smoke does
///
/// The whole palette path end to end: the accelerator resolving through
/// `actions::action_for_key` to `win.command-palette`, `shell::perform` mapping
/// `UiAction::OpenCommandPalette` onto `PaletteHandle::open`, the overlay mapping and
/// taking keyboard focus, `palette_rows` ranking `COMMAND_SPECS` against the typed
/// query, and activation recording the row's MRU key, closing the overlay and
/// handing the command to the shell's dispatch funnel. The overlay's focus return
/// runs on that path too, but nothing here discriminates it — see row 4 of the
/// table below.
///
/// The command the palette runs is `New`, so "a command really ran" is observable the
/// same way [`new_untitled_buffer_replaces_open_file_under_cage`] observes it: the
/// base file's heading DISAPPEARS from the capture. That smoke drives the chord
/// straight to the action; this one drives it through the palette's ranked list, so
/// neither subsumes the other.
///
/// ## The premise, and where it is measured
///
/// The query matches TWO actionable rows and the preselected one wins on a
/// tie-break, not on uniqueness — see the `PALETTE_*` const block for the mechanism
/// and for the one MRU state that inverts it. That premise is not asserted here (a
/// screenshot can neither count palette rows nor read their order: the overlay is
/// closed before `grim` fires). It is measured in the fast suite by
/// `palette::tests::palette_rows_query_new_ranks_the_new_buffer_row_first`, which
/// also pins that `PALETTE_MRU_KEY` still names that row. A registry change that
/// reorders the two therefore turns that unit test red rather than making this smoke
/// stochastic — and the inversion matters here beyond bookkeeping, because a second
/// WINDOW hides the base file's heading exactly as a new buffer replaces it, so
/// assertion 3 below cannot tell them apart.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. The `measured` column is from an actual red run, per
/// `tests/README.md`'s "A red-run demonstration is a deliverable, not a formality".
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | n/a |
/// | 2 | [`assert_png_present`] | precondition — grim produced an image | none; do not invent one | n/a |
/// | 3 | OCR lacks [`PALETTE_OCR_ABSENT`] | **discriminator** — the palette DISPATCHED something | in `palette.rs`'s `activate_selection`, delete the call that invokes the `on_activate` handler, keeping the `record` and the `close`. The chord still opens the palette and Enter still closes it, but no command is dispatched, so `pbase.md`'s heading is still drawn. Nothing else reads that closure, so this isolates the smoke. | RED. `OCR unexpectedly contained "palcmdbase" (the base file's heading), so the pane still shows pbase.md and NO command was dispatched`. The capture held both `palcmdbase` and `zzpalrun#`, i.e. the marker was typed into the still-open base file — so assertion 4 stayed green and this row failed alone, exactly as designed. |
/// | 4 | OCR contains [`PALETTE_OCR_PRESENT`] | **precondition** — the marker reached the editor after the palette closed | none. Deleting `self.area.grab_focus()` from `PaletteInner::close` was run as the candidate revert and did NOT go red. | GREEN across three consecutive runs with the `grab_focus` deleted. That is the measurement; the mechanism behind it was not established, and no explanation should be recorded here without one. This row is therefore reclassified from discriminator to precondition — it says the keystrokes after the activation landed somewhere, not that focus was returned. Do not substitute a different revert to manufacture a discriminator, and do not read this as licence to delete a focus return elsewhere: the ones in `picker::close`, `rebuild_pane_area` and `on_pane_clicked` redden a smoke when removed. |
/// | 5 | the command MRU holds [`PALETTE_MRU_KEY`] | **discriminator** — activation recorded the row's opaque key, and it is the registry id | delete the `CommandMru::record` call in `activate_selection`. | RED, three times out of three: `the command MRU at …/project-state/command-mru.sqlite does not hold "new_buffer"; it holds []`. No flakiness observed — the store read back non-empty on every run with the `record` in place, so the WAL-recovery worry in `tests/README.md` did not materialize here. |
///
/// Assertion 3 is what separates "the palette ran a command" from "nothing happened",
/// and its soundness rests on the fixture-naming rule: nvim's `:e` toast and the
/// sidebar row both show `pbase.md`, whose lowercase does not contain `palcmdbase`.
/// Assertion 5 is independent of the pixels entirely — nothing but a palette
/// activation writes that SQLite store — so it survives an OCR misread.
#[test]
fn command_palette_chord_runs_the_selected_command_under_cage() {
    const SMOKE: &str = "command_palette_chord_runs_the_selected_command_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target = cwd.join(PALETTE_FILE);
    std::fs::write(&target, format!("{PALETTE_HEADING}\n")).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to write base file {}: {e}",
            target.display()
        )
    });

    // Page density for a short isolated marker (see `SIDEBAR_FILLER_COUNT`). The palette's candidates
    // come from `COMMAND_SPECS`, so the fillers cannot perturb them.
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
            panic!("{SMOKE}: failed to write filler {}: {e}", filler.display())
        });
    }

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start from a
    // clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_palette_smoke.png");
    clear_evidence(&out_png, SMOKE);

    // No `HOME` override: this smoke wants the `Folder` launch and the constant `~/project` title.
    let smoke_state = smoke_state_dir(&cwd);

    let status = cage_command(PALETTE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("PALETTE_FILE", PALETTE_FILE)
        .env("PALETTE_QUERY", PALETTE_QUERY)
        .env("PALETTE_MARKER", PALETTE_MARKER)
        .status()
        .expect("failed to spawn `cage` for the headless command-palette smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage command-palette scenario exited unsuccessfully ({status}). Exit 11: \
         the app died before it was ready for input — a LAUNCH failure, and nothing below is \
         about the palette. Otherwise the app may have failed to map its window, the `:e` may \
         not have loaded, or wtype/grim failed. Note this assertion says nothing about whether \
         the palette opened: the scenario has no readback and exits 0 whatever the chord did. \
         Screenshot expected at {}.",
        out_png.display()
    );

    // --- Assertion 2 (precondition): grim produced an image -------------------
    assert_png_present(SMOKE, &out_png);

    let ocr_text = ocr_to_lowercase(&out_png);

    // --- Assertion 3 (discriminator): the palette DISPATCHED a command --------
    assert!(
        !ocr_text.contains(PALETTE_OCR_ABSENT),
        "{SMOKE}: OCR unexpectedly contained {PALETTE_OCR_ABSENT:?} (the base file's heading), so \
         the pane still shows {PALETTE_FILE} and NO command was dispatched. The likely causes, in \
         order: (a) the Ctrl+Shift+P chord never reached `win.command-palette` — check \
         `actions::action_for_key` and that the row's BindingStatus is Bound, not Reserved; (b) \
         the palette opened but Enter activated nothing, so `activation_index` returned None or \
         the connected handler was never invoked; (c) the palette opened and dispatched, but \
         `shell::perform` has no arm for the command. Note the marker assertion below can still \
         pass in every one of those cases — the marker would simply have been typed into \
         {PALETTE_FILE} itself. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 4 (precondition): the marker reached a buffer. It does not test focus return; deleting
    // `PaletteInner::close`'s `area.grab_focus()` stays green, so read the revert table first.
    assert!(
        ocr_text.contains(PALETTE_OCR_PRESENT),
        "{SMOKE}: OCR did not contain the marker {PALETTE_OCR_PRESENT:?} typed after the palette \
         closed, so the keystrokes that followed the activation never reached a buffer — which \
         also means assertion 3 above passed VACUOUSLY and proves nothing about the dispatch. The \
         likely causes, in order: (a) the palette never closed, so its SearchEntry swallowed the \
         marker; (b) the activation ran a row other than New — a Close or Quit row would leave no \
         editor to draw the marker, and assertion 5 below names the row that actually ran; (c) \
         the marker rendered but OCR misread it — check the retained PNG before touching \
         anything. NOT a cause: `PaletteInner::close` failing to return focus, which was measured \
         green across three runs; the mechanism behind that is unestablished, so do not \
         generalize it. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 5 (discriminator): only a palette activation writes this store (see
    // `WolfCore::command_mru`), so a hit means the row ran with its canonical registry id.
    let recent = CommandMru::open(&smoke_state).recent();
    assert!(
        recent.iter().any(|k| k == PALETTE_MRU_KEY),
        "{SMOKE}: the command MRU at {} does not hold {PALETTE_MRU_KEY:?}; it holds {recent:?}. \
         Nothing but a palette activation records into that store, so an EMPTY list means \
         activation never called `CommandMru::record` (or the app resolved a different state dir \
         — `cage_command` is the only thing that sets WOLF_STATE_DIR). A NON-empty list holding a \
         different key means the activated row was not the New row, which would also have failed \
         the unit test that pins the query to exactly one row. If this is the ONLY assertion that \
         fails, and it fails intermittently, suspect SQLite WAL recovery from the scenario's \
         `kill` rather than the palette — see tests/README.md, \"What is still unproven\".",
        smoke_state.join("command-mru.sqlite").display()
    );
}

/// Ctrl+Shift+E opens the buffer picker, a typed query narrows its list to one row,
/// and Enter switches the active window to THAT buffer.
///
/// ## What it covers that no other smoke does
///
/// The whole buffer-picker path end to end: the accelerator resolving through
/// `actions::action_for_key` to `win.buffer-picker`, `shell::perform` mapping
/// `UiAction::OpenBufferPicker` onto the overlay's `open`, the liveness guard
/// letting a healthy core through, the ONE blocking `WolfCore::list_buffers`
/// snapshot returning a candidate set, `wolf_core::rank_buffers` filtering it per
/// keystroke, and activation resolving `active_win` plus the ranked row's `bufnr`
/// into an `InputEvent::SwitchBuffer` Neovim acts on.
///
/// It is also the only smoke in which the app issues a synchronous RPC while the
/// window is interactive, which is what makes
/// `stalled_nvim_raises_the_stall_overlay_under_cage` possible at all.
///
/// ## The premise, and why it is READ BACK rather than assumed
///
/// `buffer_picker::initial_selection` preselects ROW 1, so a picker that ignored
/// the typed query entirely would still activate a buffer — the most-recently-used
/// OTHER one. The smoke is only a test of FILTERING while the target is neither row
/// 0 nor row 1, and that ordering rests on `getbufinfo().lastused`, a WHOLE-SECOND
/// timestamp: three `:e`s inside one second tie, `rank_buffers` falls back to
/// `bufnr` ASC, the order inverts, and the target lands on exactly the row the
/// preselection would have picked. The scenario therefore writes Neovim's own
/// buffer list to [`BUFPICK_READBACK_NAME`] and assertion 3 re-derives the picker's
/// order from it with the production rule. The same arithmetic is pinned headlessly
/// by
/// `buffer_picker::tests::buffer_rows_for_the_smoke_query_select_exactly_the_target`,
/// so a ranking change reddens the fast suite rather than making this smoke
/// stochastic.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. The `measured` column is from an actual red run, per
/// `tests/README.md`'s "A red-run demonstration is a deliverable, not a formality".
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | n/a |
/// | 2 | [`assert_png_present`] | precondition — grim produced an image | none; do not invent one | n/a |
/// | 3 | the readback's MRU order puts the CURRENT file at index 0, the PRESELECT file at index 1, and the target at index 2 or later | **precondition**, loud and self-checking | none. It is a property of the SCENARIO's timing, not of any source line: its failure means the three `:e`s landed inside one `lastused` second, and the fix is to raise `MRU_GAP`. Without it, assertions 4 and 5 are vacuous — a picker that ignored the query would activate the preselected row, and under a collided order that row could be the target itself, so the smoke would pass forever with no filtering at all. | n/a |
/// | 4 | OCR contains [`BUFPICK_OCR_PRESENT`] | **discriminator** — the activation really switched the buffer | in `buffer_picker.rs`'s `activate_selection`, delete the `WolfCore::switch_buffer` call and keep the `close()`. The chord still fires, the overlay still opens, the query still filters and Enter still closes it — but the pane still shows `bpxray.md`, so the target's heading is absent. Isolated: assertion 5 stays green. | RED. `OCR did not contain "bufpickalfa" — the HEADING of bpzulu.md, which only renders once the active window really has been switched to that buffer`. The only asserted term in the capture was `bufpickcharlie`, i.e. the pane never left `bpxray.md`; `bufpickbravo` was absent, so assertion 5's condition held. Restored, green. |
/// | 5 | OCR lacks [`BUFPICK_OCR_ABSENT`] | **discriminator, with NO isolated red run** | in `buffer_picker.rs`'s `refresh`, rank against `""` instead of `self.entry.text()`. The list then holds every buffer MRU-first and Enter activates the PRESELECTED row, so `bpyankee.md`'s heading is what the capture shows. That reddens assertion 4 as well, and no revert reddens 5 alone: with one pane, two headings cannot both be on screen. Recorded honestly — assertion 5's contribution is naming WHICH wrong buffer was activated, not detecting that one was. | No isolated red run, by design. Under assertion 4's revert this row's own condition HELD (the capture carried no `bufpickbravo`) and assertion 4 panicked before it was reached. Green in every run in which assertion 4 was green. |
///
/// A third, coarser revert, for use only if the two above are inconclusive: delete
/// `self.inner.buffers.open()` from `shell::perform`'s new arm. Nothing opens, the
/// query is typed into `bpxray.md` as normal-mode keys, and assertions 4 and 5 both
/// go red.
///
/// ## Its sibling
///
/// [`buffer_picker_bare_enter_switches_to_the_previous_buffer_under_cage`] runs the
/// same three fixtures in the same order and types NO query, so it asserts
/// [`BUFPICK_OCR_ABSENT`] PRESENT and [`BUFPICK_OCR_PRESENT`] absent — the exact
/// opposite of this smoke on both terms. It is also the only test that reaches
/// `refresh`'s `initial_open` branch, which this one cannot: typing re-enters
/// `refresh` with the flag already cleared, so the selection here comes from
/// `restored_index`.
#[test]
fn buffer_picker_chord_switches_to_the_queried_buffer_under_cage() {
    const SMOKE: &str = "buffer_picker_chord_switches_to_the_queried_buffer_under_cage";
    // Startup buffer plus three fixtures makes four; below three the ordering assertions say nothing.
    const MIN_LISTED_BUFFERS: usize = 3;
    // Row 0 is the current buffer and row 1 the preselected one, so a target above this makes the query
    // redundant and assertion 4 vacuous.
    const MIN_TARGET_INDEX: usize = 2;

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    for (name, heading) in [
        (BUFPICK_FILE_TARGET, BUFPICK_HEADING_TARGET),
        (BUFPICK_FILE_PRESELECT, BUFPICK_HEADING_PRESELECT),
        (BUFPICK_FILE_CURRENT, BUFPICK_HEADING_CURRENT),
    ] {
        let path = cwd.join(name);
        std::fs::write(&path, format!("{heading}\n"))
            .unwrap_or_else(|e| panic!("{SMOKE}: failed to write fixture {}: {e}", path.display()));
    }

    // Page density (see `SIDEBAR_FILLER_COUNT`). Never opened, so they never join the picker's
    // candidates, which come from `getbufinfo`.
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
            panic!("{SMOKE}: failed to write filler {}: {e}", filler.display())
        });
    }

    // Cleared first: evidence that could be a previous run's leftover is not evidence.
    let readback = cwd.join(BUFPICK_READBACK_NAME);
    clear_evidence(&readback, SMOKE);

    let out_png = runtime_dir.join("wolf_gnome_bufpick_smoke.png");
    clear_evidence(&out_png, SMOKE);

    let status = cage_command(BUFPICK_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("BUFPICK_FILE_TARGET", BUFPICK_FILE_TARGET)
        .env("BUFPICK_FILE_PRESELECT", BUFPICK_FILE_PRESELECT)
        .env("BUFPICK_FILE_CURRENT", BUFPICK_FILE_CURRENT)
        .env("BUFPICK_QUERY", BUFPICK_QUERY)
        .env("BUFPICK_READBACK", &readback)
        .status()
        .expect("failed to spawn `cage` for the headless buffer-picker smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage buffer-picker scenario exited unsuccessfully ({status}). Read the exit \
         code FIRST.\n\
         Exit 11: the app died before it was ready for input — a LAUNCH failure, and nothing \
         below is about the buffer picker.\n\
         Exit 14: the buffer-list readback the captive Neovim was told to write never appeared, \
         so the premise assertion below cannot run at all. Keys are not reaching Neovim, or the \
         typed `:call writefile(...)` did not execute — check the keyboard-only smokes first.\n\
         Any other status means the app failed to map its window, a `:e` did not load, or \
         wtype/grim failed. Note this assertion says nothing about whether the picker opened: the \
         scenario exits 0 whatever the chord did.\n\
         Screenshot expected at {}.",
        out_png.display()
    );

    // --- Assertion 2 (precondition): grim produced an image -------------------
    assert_png_present(SMOKE, &out_png);

    // Assertion 3 (precondition): the MRU order, from `getbufinfo()` with the production rule
    // (`lastused` desc, ties by `bufnr` asc), never assumed from the typed `:e` order.
    let order = read_buffer_mru_order(&readback, SMOKE);
    let tails: Vec<&str> = order.iter().map(|(_, _, tail)| tail.as_str()).collect();

    assert!(
        order.len() >= MIN_LISTED_BUFFERS,
        "{SMOKE}: Neovim listed only {} buflisted buffer(s) ({tails:?}), fewer than the \
         {MIN_LISTED_BUFFERS} this smoke's premise needs. All three `:e` commands must have \
         landed — with fewer buffers the target cannot be pushed below the preselected row, so \
         assertions 4 and 5 would be measuring a picker whose query is redundant. Readback kept \
         at {}.",
        order.len(),
        readback.display()
    );
    assert_eq!(
        tails.first().copied(),
        Some(BUFPICK_FILE_CURRENT),
        "{SMOKE}: the MRU-ordered buffer list opens on {:?}, not the last-opened file \
         {BUFPICK_FILE_CURRENT:?}. The order was re-derived from Neovim's own \
         `bufnr lastused tail` readback with the same rule `rank_buffers` uses, so this is a \
         PREMISE failure rather than a product one: the three `:e`s landed inside one \
         `lastused` second (it is a WHOLE-SECOND timestamp), the tie fell back to `bufnr` ASC, \
         and the whole order inverted. RAISE `MRU_GAP` in `BUFPICK_SCENARIO` — it must exceed \
         1.0s with margin. Full order was {tails:?}; readback kept at {}.",
        tails.first(),
        readback.display()
    );
    assert_eq!(
        tails.get(1).copied(),
        Some(BUFPICK_FILE_PRESELECT),
        "{SMOKE}: row 1 of the MRU-ordered buffer list is {:?}, not {BUFPICK_FILE_PRESELECT:?}. \
         Row 1 is the row `buffer_picker::initial_selection` preselects, so it is the row a \
         picker that IGNORED the typed query would activate — and assertion 5 asserts that \
         file's heading is absent. With a different file there, assertion 5 names the wrong \
         buffer and stops discriminating anything. The usual cause is the same one as above: \
         three `:e`s inside one whole-second `lastused` tick, fixed by raising `MRU_GAP`. Full \
         order was {tails:?}; readback kept at {}.",
        tails.get(1),
        readback.display()
    );

    let target_index = tails
        .iter()
        .position(|tail| *tail == BUFPICK_FILE_TARGET)
        .unwrap_or_else(|| {
            panic!(
                "{SMOKE}: the buffer list does not contain {BUFPICK_FILE_TARGET:?} at all \
                 ({tails:?}), so the query could not have selected it however well the ranking \
                 worked — its `:e` never took effect. Readback kept at {}.",
                readback.display()
            )
        });
    assert!(
        target_index >= MIN_TARGET_INDEX,
        "{SMOKE}: the query's target {BUFPICK_FILE_TARGET:?} sits at index {target_index} of the \
         MRU-ordered buffer list, but it must be at index {MIN_TARGET_INDEX} or later. At index 0 \
         or 1 it is the row `initial_selection` preselects, so a picker that ignored the typed \
         query ENTIRELY would still activate it: assertion 4 would pass permanently while proving \
         nothing about filtering, and assertion 5 would pass because the preselected row and the \
         target are the same file. This is a PREMISE failure, and its cause is timing rather than \
         source: `lastused` is a whole-second timestamp, so three `:e`s inside one second tie and \
         the order inverts. Raise `MRU_GAP` in `BUFPICK_SCENARIO`; do NOT relax this bound. Full \
         order was {tails:?}; readback kept at {}.",
        readback.display()
    );

    let ocr_text = ocr_to_lowercase(&out_png);

    // --- Assertion 4 (discriminator): the activation SWITCHED the buffer ------
    assert!(
        ocr_text.contains(BUFPICK_OCR_PRESENT),
        "{SMOKE}: OCR did not contain {BUFPICK_OCR_PRESENT:?} — the HEADING of \
         {BUFPICK_FILE_TARGET}, which only renders once the active window really has been \
         switched to that buffer. Assertion 3 passed, so the target was in the list and below \
         the preselected row; this says the switch did not happen. The likely causes, in order: \
         (a) the Ctrl+Shift+E chord never reached `win.buffer-picker` — check that the \
         ACTION_TABLE row's BindingStatus is Bound rather than Reserved, and that \
         `command_action::ui_action_for` maps `Command::BufferPicker` to a real UiAction (BOTH \
         are required; neither alone installs the action); (b) the overlay opened but its \
         `open_decision` refused, in which case the app's stderr carries \
         \"buffer picker not opened\"; (c) Enter activated nothing, so `switch_target` returned \
         None — `active_win` being None or 0 is the arm to check; (d) the switch was sent but \
         `bpxray.md` is still on screen, which assertion 5 would also catch. If the capture shows \
         {BUFPICK_HEADING_CURRENT} instead, the picker did nothing at all rather than picking \
         wrongly. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 5 (discriminator, no isolated red run): names the wrong buffer if the query is ignored.
    // See the revert table on this test.
    assert!(
        !ocr_text.contains(BUFPICK_OCR_ABSENT),
        "{SMOKE}: OCR unexpectedly contained {BUFPICK_OCR_ABSENT:?} — the heading of \
         {BUFPICK_FILE_PRESELECT}, the row `buffer_picker::initial_selection` preselects. The \
         picker therefore activated its PRESELECTED row rather than the row the query selected, \
         which means the typed text never reached the ranking: check that `refresh` ranks against \
         the SearchEntry's text rather than an empty query, and that `window::wire_keys` suppresses \
         forwarding while the overlay is open (without that suppression the query is delivered to \
         Neovim as normal-mode keys instead). Assertion 4 above is red too in that case, and it \
         is the one to read first — this assertion's contribution is naming WHICH wrong buffer \
         was activated. The assertion is sound: after `:e {BUFPICK_FILE_PRESELECT}` Neovim's \
         toast and the sidebar row both show the FILENAME, whose lowercase does not contain \
         {BUFPICK_OCR_ABSENT:?}. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

/// Ctrl+Shift+E and then a BARE Enter — no query typed — switches to the
/// most-recently-used OTHER buffer.
///
/// ## What it covers that nothing else does
///
/// `buffer_picker::initial_selection`'s arithmetic is table-tested headlessly
/// (`initial_selection_table`), but the branch that APPLIES it — `refresh`'s
/// `initial_open` arm selecting that row in the real `ListBox` — is a rule living
/// inside a widget closure the fast suite cannot execute at all. That is exactly the
/// class `notes/wolf-gnome-decisions.md` ("The pure seams") records as having
/// "repeatedly meant a rule nothing verified" on this platform.
///
/// [`buffer_picker_chord_switches_to_the_queried_buffer_under_cage`] does not reach
/// it either, and cannot: it types a query, and typing re-enters `refresh` with
/// `initial_open` already cleared, so its selection comes from `restored_index`. The
/// preselection is only ever applied on the FIRST refresh after an open, which is the
/// only refresh this smoke performs.
///
/// It is also the behaviour that makes the feature a buffer SWITCHER rather than a
/// list: open it, press Enter, land back in the file you came from. macOS does the
/// same thing — `crates/wolf-macos/Sources/WolfMacOS/BufferPickerView.swift`'s
/// `min(1, rows.count - 1)` behind an `isInitialOpen` flag.
///
/// ## It shares the query smoke's fixtures, and pulls them the other way
///
/// Same three files, same `:e` order, same `MRU_GAP`, same readback — so the two
/// captures differ in exactly one input. [`BUFPICK_OCR_ABSENT`] is asserted PRESENT
/// here and ABSENT there, and [`BUFPICK_OCR_PRESENT`] the reverse. That opposition is
/// what makes the pair discriminating rather than either alone: the other smoke's
/// green run, over the same fixture set in the same shape of cwd, is measured
/// evidence that no sidebar row, no `:e` echo and no header-bar segment can produce
/// [`BUFPICK_OCR_ABSENT`] by itself.
///
/// ## The premises, and how each is measured rather than assumed
///
/// * **The MRU order the preselection lands in.** `getbufinfo().lastused` is a
///   WHOLE-SECOND timestamp: three `:e`s inside one second tie, `rank_buffers` falls
///   back to `bufnr` ASC and the order inverts — which moves the row
///   `initial_selection` picks, i.e. the row this smoke's must-contain term names.
///   The scenario therefore writes Neovim's own buffer list to
///   [`BUFPICK_READBACK_NAME`] and assertion 2 re-derives the picker's order from it
///   with the production rule. `MRU_GAP` is what buys the distinct seconds; the
///   readback is what proves it worked on THIS run.
/// * **`Enter` in an EMPTY `gtk::SearchEntry` still emits `activate`.** The query
///   smoke cannot say so — its entry holds text — and no headless test can reach a
///   real `SearchEntry` at all. This smoke is what measures it against the real
///   widget. If the premise is false the overlay never closes, so the capture shows
///   the picker's row list (which draws FILENAMES, carrying none of the three
///   asserted terms) over an unswitched pane: assertion 3 goes red and assertion 4
///   may or may not, since the open overlay's opaque box can occlude part of the
///   heading beneath it. Assertion 3's message enumerates that outcome next to the
///   others so the two are told apart from the failure text rather than from the PNG.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. The `measured` column is from an actual red run, per
/// `tests/README.md`'s "A red-run demonstration is a deliverable, not a formality".
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` and [`assert_png_present`] | precondition — the cage scenario ran to completion and grim produced an image | none; do not invent one | n/a |
/// | 2 | the readback's MRU order puts the CURRENT file at index 0, the PRESELECT file at index 1, and the target at index 2 or later | **precondition**, loud and self-checking | none. It is a property of the SCENARIO's timing, not of any source line: its failure means the three `:e`s landed inside one `lastused` second, and the fix is to raise `MRU_GAP`. Without its index-1 clause, assertion 3 asserts on the wrong row's heading and the whole smoke measures nothing. | n/a |
/// | 3 | OCR contains [`BUFPICK_OCR_ABSENT`] — the PRESELECT file's heading | **discriminator** — the row-1 preselection reached the `ListBox` and Enter acted on it | in `buffer_picker.rs`'s `refresh`, replace the `initial_open` branch's `initial_selection(new_len)` with `restored_index(reset_to_top, prev_index, new_len)`: delete the preselection and fall through to the rule the other two overlays share, which selects row 0. Enter then re-selects the buffer the user is already in, so this heading never appears. | RED. `OCR did not contain "bufpickbravo" — the HEADING of bpyankee.md, which renders only once the active window really has been switched to that buffer`. The capture carried `bufpickcharlie`, i.e. row 0 — the buffer already on screen — was what Enter activated. Restored, green. |
/// | 4 | OCR lacks [`BUFPICK_OCR_CURRENT`] — the CURRENT file's heading | **discriminator** — a switch happened at all, rather than the picker closing on nothing | the same revert. With row 0 selected the "switch" is to the buffer already on screen, so this heading is still there afterwards. The two assertions go red together under it and that is the point: one says we did not arrive, the other says we never left. | Not reached: assertion 3 panics first under that revert. Its condition WAS violated in the same capture, which carried `bufpickcharlie`, so the pair does fail together — but only assertion 3's message is emitted. |
/// | 5 | OCR lacks [`BUFPICK_OCR_PRESENT`] — the TARGET file's heading | **discriminator, NOT reddened by the revert above** | it discriminates the opposite error: a preselection that is too DEEP, e.g. `Some(2.min(n_rows - 1))`, which lands on the target and reddens 3 and 5 while leaving 4 green. Recorded honestly rather than dressed up — that revert also reddens the fast suite's `initial_selection_table`, so it is not an isolated front-end change. | Not reached: under `Some(2.min(n_rows - 1))` assertion 3 panicked first, again on `OCR did not contain "bufpickbravo"`. That capture carried `bufpickalfa` — this row's forbidden term — and no `bufpickcharlie`, so assertion 4 would have passed and this one would have failed. Recorded as observed, not as an isolated red run. |
#[test]
fn buffer_picker_bare_enter_switches_to_the_previous_buffer_under_cage() {
    const SMOKE: &str = "buffer_picker_bare_enter_switches_to_the_previous_buffer_under_cage";
    // Startup buffer plus three fixtures makes four; below three the ordering assertions say nothing.
    const MIN_LISTED_BUFFERS: usize = 3;
    // Row 0 is the current buffer and row 1 the preselected one, so a target above this is where Enter
    // is expected to land and assertion 5 would contradict assertion 3.
    const MIN_TARGET_INDEX: usize = 2;

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    for (name, heading) in [
        (BUFPICK_FILE_TARGET, BUFPICK_HEADING_TARGET),
        (BUFPICK_FILE_PRESELECT, BUFPICK_HEADING_PRESELECT),
        (BUFPICK_FILE_CURRENT, BUFPICK_HEADING_CURRENT),
    ] {
        let path = cwd.join(name);
        std::fs::write(&path, format!("{heading}\n"))
            .unwrap_or_else(|e| panic!("{SMOKE}: failed to write fixture {}: {e}", path.display()));
    }

    // Page density (see `SIDEBAR_FILLER_COUNT`). Never opened, so they never join the picker's
    // candidates and cannot shift `initial_selection`.
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
            panic!("{SMOKE}: failed to write filler {}: {e}", filler.display())
        });
    }

    // Cleared first: evidence that could be a previous run's leftover is not evidence.
    let readback = cwd.join(BUFPICK_READBACK_NAME);
    clear_evidence(&readback, SMOKE);

    let out_png = runtime_dir.join("wolf_gnome_bufpick_bare_smoke.png");
    clear_evidence(&out_png, SMOKE);

    let status = cage_command(BUFPICK_BARE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("BUFPICK_FILE_TARGET", BUFPICK_FILE_TARGET)
        .env("BUFPICK_FILE_PRESELECT", BUFPICK_FILE_PRESELECT)
        .env("BUFPICK_FILE_CURRENT", BUFPICK_FILE_CURRENT)
        .env("BUFPICK_READBACK", &readback)
        .status()
        .expect("failed to spawn `cage` for the headless bare-Enter buffer-picker smoke scenario");

    // --- Assertion 1 (preconditions): the scenario ran, and grim produced a PNG ---
    assert!(
        status.success(),
        "{SMOKE}: the cage bare-Enter buffer-picker scenario exited unsuccessfully ({status}). \
         Read the exit code FIRST.\n\
         Exit 11: the app died before it was ready for input — a LAUNCH failure, and nothing \
         below is about the buffer picker.\n\
         Exit 14: the buffer-list readback the captive Neovim was told to write never appeared, \
         so the premise assertion below cannot run at all. Keys are not reaching Neovim, or the \
         typed `:call writefile(...)` did not execute — check the keyboard-only smokes first.\n\
         Any other status means the app failed to map its window, a `:e` did not load, or \
         wtype/grim failed. Note this assertion says nothing about whether the picker opened or \
         what Enter did: the scenario exits 0 whatever the chord produced.\n\
         Screenshot expected at {}.",
        out_png.display()
    );
    assert_png_present(SMOKE, &out_png);

    // Assertion 2 (precondition): the MRU order from `getbufinfo()` with the production rule. Index 1
    // is load-bearing: it is the row `initial_selection` picks, so it decides assertion 3's file.
    let order = read_buffer_mru_order(&readback, SMOKE);
    let tails: Vec<&str> = order.iter().map(|(_, _, tail)| tail.as_str()).collect();

    assert!(
        order.len() >= MIN_LISTED_BUFFERS,
        "{SMOKE}: Neovim listed only {} buflisted buffer(s) ({tails:?}), fewer than the \
         {MIN_LISTED_BUFFERS} this smoke's premise needs. All three `:e` commands must have \
         landed — with fewer buffers there is no distinct row 1 for `initial_selection` to \
         preselect, so assertions 3 to 5 would be measuring a list whose shape cannot \
         discriminate the preselection from a clamp. Readback kept at {}.",
        order.len(),
        readback.display()
    );
    assert_eq!(
        tails.first().copied(),
        Some(BUFPICK_FILE_CURRENT),
        "{SMOKE}: the MRU-ordered buffer list opens on {:?}, not the last-opened file \
         {BUFPICK_FILE_CURRENT:?}. The order was re-derived from Neovim's own \
         `bufnr lastused tail` readback with the same rule `rank_buffers` uses, so this is a \
         PREMISE failure rather than a product one: the three `:e`s landed inside one `lastused` \
         second (it is a WHOLE-SECOND timestamp), the tie fell back to `bufnr` ASC, and the whole \
         order inverted. Row 0 is the buffer the user is IN, so with a different file there \
         assertion 4's must-be-absent term names the wrong buffer as well. RAISE `MRU_GAP` in \
         `BUFPICK_BARE_SCENARIO` — it must exceed 1.0s with margin. Full order was {tails:?}; \
         readback kept at {}.",
        tails.first(),
        readback.display()
    );
    assert_eq!(
        tails.get(1).copied(),
        Some(BUFPICK_FILE_PRESELECT),
        "{SMOKE}: row 1 of the MRU-ordered buffer list is {:?}, not {BUFPICK_FILE_PRESELECT:?}. \
         Row 1 is the row `buffer_picker::initial_selection` preselects, and this smoke types no \
         query at all — so it is the row Enter activates and the ONLY row whose heading assertion \
         3 is entitled to demand. With a different file there, assertion 3 asserts the wrong \
         row's heading: it would fail on a perfectly working preselection, or (if the file that \
         moved into row 1 happened to be the one it names) pass without the preselection working \
         at all. The usual cause is timing, not source: three `:e`s inside one whole-second \
         `lastused` tick, fixed by raising `MRU_GAP`. Full order was {tails:?}; readback kept at \
         {}.",
        tails.get(1),
        readback.display()
    );

    let target_index = tails
        .iter()
        .position(|tail| *tail == BUFPICK_FILE_TARGET)
        .unwrap_or_else(|| {
            panic!(
                "{SMOKE}: the buffer list does not contain {BUFPICK_FILE_TARGET:?} at all \
                 ({tails:?}), so assertion 5's must-be-absent heading could never have been \
                 rendered whatever the picker did — its `:e` never took effect. Readback kept at \
                 {}.",
                readback.display()
            )
        });
    // Implied by the clauses above, so stated as assertion 5's direct premise; it still holds if
    // either is relaxed (a fourth fixture, a startup buffer listed differently).
    assert!(
        target_index >= MIN_TARGET_INDEX,
        "{SMOKE}: {BUFPICK_FILE_TARGET:?} sits at index {target_index} of the MRU-ordered buffer \
         list, but it must be at index {MIN_TARGET_INDEX} or later. At index 1 it IS the row \
         `initial_selection` preselects, so assertions 3 and 5 would demand that one capture both \
         show and not show a heading — an unsatisfiable pair that would read as a product bug. At \
         index 0 it is the buffer already on screen and assertion 5 merely duplicates assertion 4. \
         This is a PREMISE failure and its cause is timing rather than source: `lastused` is a \
         whole-second timestamp, so three `:e`s inside one second tie and the order inverts. Raise \
         `MRU_GAP` in `BUFPICK_BARE_SCENARIO`; do NOT relax this bound. Full order was {tails:?}; \
         readback kept at {}.",
        readback.display()
    );

    let ocr_text = ocr_to_lowercase(&out_png);

    // --- Assertion 3 (discriminator): the ROW-1 PRESELECTION was applied -------
    assert!(
        ocr_text.contains(BUFPICK_OCR_ABSENT),
        "{SMOKE}: OCR did not contain {BUFPICK_OCR_ABSENT:?} — the HEADING of \
         {BUFPICK_FILE_PRESELECT}, which renders only once the active window really has been \
         switched to that buffer. Assertion 2 passed, so that file WAS row 1 of the picker's \
         MRU-first list: the row `buffer_picker::initial_selection` preselects on the first \
         refresh after an open, and with no query typed the only row a bare Enter can activate. \
         The distinguishable causes, in order:\n\
         (a) the preselection never reached the ListBox — `refresh`'s `initial_open` branch fell \
         through to `restored_index`, which selects row 0, so the pane never left \
         {BUFPICK_FILE_CURRENT} and ASSERTION 4 IS RED TOO. Read that one next; the pair says \
         \"we never left\" and \"we did not arrive\".\n\
         (b) the preselection landed too DEEP (row 2 or later), in which case assertion 5 is red \
         and names the file it landed on instead.\n\
         (c) Enter did nothing at all: `gtk::SearchEntry` did not emit `activate` for an EMPTY \
         entry, or `switch_target` returned None (`active_win` None or 0). The overlay then never \
         closed, and the capture shows its row list — which draws FILENAMES (`bp*`), none of \
         which contains any asserted term — over the unswitched pane. Assertion 4 may or may not \
         catch this, since the open overlay's opaque box can occlude the heading beneath it, so \
         this assertion is the one that speaks for that case. The scenario's `o` + Escape also \
         only reach Neovim once the overlay has closed.\n\
         (d) the chord never reached `win.buffer-picker`, or `open_decision` refused — the app's \
         stderr then carries \"buffer picker not opened\". \
         PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // --- Assertion 4 (discriminator): a switch happened AT ALL ----------------
    assert!(
        !ocr_text.contains(BUFPICK_OCR_CURRENT),
        "{SMOKE}: OCR unexpectedly contained {BUFPICK_OCR_CURRENT:?} — the heading of \
         {BUFPICK_FILE_CURRENT}, the buffer the editor was ALREADY showing when the chord fired. \
         A bare Enter must move the pane off it, so this is the assertion that separates \"the \
         preselection switched the buffer\" from \"the picker closed on nothing and left the page \
         exactly as it was\" — two states no other assertion here can tell apart, because both \
         simply lack the preselected row's heading. The likeliest cause is the preselection \
         falling through to row 0, which is the buffer the user is already in (`rank_buffers` \
         sorts by `lastused` DESC): check that `refresh`'s `initial_open` branch still calls \
         `initial_selection` rather than `restored_index`. Assertion 3 is red alongside this one \
         in that case. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 5 (discriminator): the preselection is not past row 1, the opposite error to 3 and 4.
    // See the revert table on this test.
    assert!(
        !ocr_text.contains(BUFPICK_OCR_PRESENT),
        "{SMOKE}: OCR unexpectedly contained {BUFPICK_OCR_PRESENT:?} — the heading of \
         {BUFPICK_FILE_TARGET}, which assertion 2 proved sits at index {target_index} of the \
         MRU-ordered list, i.e. below the row a bare Enter must activate. NO QUERY IS TYPED in \
         this smoke, so nothing should be able to select that row: either \
         `buffer_picker::initial_selection` returned an index past 1 (the rule is \
         `min(1, n_rows - 1)`, and `initial_selection_table` pins it), or the list the picker \
         built is not in the MRU order the readback describes — check that `refresh` ranks the \
         snapshot taken at open rather than some re-fetched or re-sorted list. Assertion 3 is red \
         with it in the first case. Full readback order was {tails:?}; PNG kept at {} for \
         inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

// --- Copy and paste through the system clipboard -----------------------------

/// The word the captive Neovim puts on line 1, copies to the system clipboard, and
/// pastes back.
///
/// It is deliberately NOT a fixture filename and NOT an OCR term: nothing in this
/// smoke is captured or OCR'd, so it contributes no row to `ASSERTED_OCR_TERMS`
/// and no fixture may be named after it either — no file called `zzclipmark.md` is
/// written anywhere. What the assertion counts is occurrences inside a file the
/// captive Neovim wrote OUT OF ITS OWN BUFFER, so a sidebar row, a header-bar
/// title or an `:e` toast cannot contribute to the count even in principle.
///
/// All-caps and `ZZ`-prefixed for the same reason the other in-buffer markers are
/// ([`NEWBUF_MARKER`], `FOCUS_MARKER`): it cannot collide with a word Neovim, the
/// chrome or a fixture would produce on its own, so a count of two can only mean
/// the text made the round trip.
const CLIP_MARK: &str = "ZZCLIPMARK";

/// File the captive Neovim writes EVERY LINE of its buffer into, after the paste.
///
/// It lives under `XDG_RUNTIME_DIR` rather than in the smoke's cwd, so the app's own
/// `FileSearch` never lists it and it never becomes a tree-sidebar row — the same
/// treatment `FULLWIDTH_CAPPED_READBACK_NAME` gets, and belt-and-braces here for
/// the same reason: nothing on this page is read by OCR.
const CLIP_READBACK_NAME: &str = "wolf_gnome_clipboard_lines.txt";

/// How many times [`CLIP_MARK`] must appear in the readback: ONCE for the line the
/// scenario seeded, ONCE for the copy that came back through the clipboard.
///
/// Written as a named constant rather than a literal `2` so the assertion's message
/// and the number it checks cannot drift apart.
///
/// The count is a SUBSTRING count, which is only meaningful because the scenario
/// lands the paste on an empty line; see [`CLIPBOARD_SCENARIO`]'s seeding comment
/// for why a paste onto the marker's own line would splice the two into a single
/// occurrence and make a working build indistinguishable from a broken one.
const CLIP_EXPECTED_OCCURRENCES: usize = 2;

/// The single-launch CLIPBOARD ROUND-TRIP scenario, run as one `sh -c` string inside
/// one `cage`, structurally `FULLWIDTH_SCENARIO`'s shape.
///
/// Both buffer accesses go through `nvim_buf_set_lines` / `nvim_buf_get_lines`
/// rather than `setline` / `getline(1, '$')`: the scenario is a double-quoted shell
/// string, and a `'$'` inside one sits one edit away from being read as a parameter
/// expansion. Each API line carries exactly one `$`, which is the readback path, so
/// there is nothing to get wrong.
///
/// Exit codes, each naming ONE cause and no two folded together: `11` the app died
/// before it was ready for input; `12` the buffer readback never appeared (or was empty);
/// `13` the app was still alive after `:qa!`; `14` the app exited non-zero. 13 and 14
/// are separate because a process that panicked in the GTK main loop and one that
/// shut down cleanly are equally GONE — only the status tells them apart.
const CLIPBOARD_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# TWICE the settle, and the doubling is deliberate. Every other pause in this
# scenario follows a key Neovim answers by itself. The paste follows a round trip
# with six hops, each a separate main-loop turn and NONE of them pollable from out
# here: the chord -> gdk::Clipboard::read_text_async -> the async callback landing on
# a later turn of the GTK main context -> WolfCore::paste's send on the ordered input
# channel -> the input pump's nvim_paste(text, true, -1) -> Neovim's flush. The read
# is asynchronous and the insert is fire-and-forget, so nothing on that path
# reports completion and there is no file to wait on until the buffer already holds
# the text.
PASTE_SETTLE=2.0
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
    echo "wolf-gnome clipboard smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# $1 is the exit code for "still alive after :qa!", $2 for "exited non-zero".
quit_app() {
    wtype -- ":qa!"
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
        echo "wolf-gnome clipboard smoke: app (pid $app_pid) still alive after :qa!." >&2
        exit "$1"
    fi
    # `set -e` would abort before the status could be reported, so collect it
    # through `||`, which is exempt from it.
    rc=0
    wait "$app_pid" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "wolf-gnome clipboard smoke: app (pid $app_pid) exited $rc, not 0." >&2
        exit "$2"
    fi
    app_pid=
}

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome clipboard smoke"

# Seed the whole buffer: the marker on line 1, then TWO EMPTY LINES. Set through the
# API rather than typed in insert mode, so the marker cannot be corrupted by a dropped
# or doubled keystroke -- either the whole Ex command executed or none of it did, and
# the "none of it did" case is the ZERO-occurrence arm of the Rust assertion.
#
# THE EMPTY LINES ARE LOAD-BEARING, and they are the one thing here that is easy to
# "simplify" away. `nvim_paste` splits the linewise register "ZZCLIPMARK\n" into
# ["ZZCLIPMARK", ""] and, in normal mode, `vim.paste` performs a CHARWISE
# `nvim_put(..., 'c', true, false)` -- an insert AFTER the cursor character, not a
# linewise put. Pasted onto the marker's own line that splices the two into
# "ZZZCLIPMARK" plus "ZCLIPMARK", in which the marker occurs exactly ONCE: a fully
# working build would then fail with the same count a broken one produces. Landing the
# paste on an EMPTY line is what makes the two occurrences separate strings, whatever
# `nvim_put` does with the boundary.
wtype -- ":call nvim_buf_set_lines(0, 0, -1, v:false, ['$CLIP_MARK', '', ''])"
wtype -k Return
sleep "$SETTLE"

# Park on line 1 and select it LINEWISE. `V` and not `v`: core's yank accepts v, V
# and <C-v> alike, and a linewise selection needs no motion to cover the whole word.
wtype -- "gg"
wtype -- "V"
sleep "$SETTLE"

# The COPY chord. Wolf matches it in a CAPTURE-phase EventControllerKey
# (actions::action_for_key), so Neovim is sent no key at all and the visual selection
# is STILL LIVE when copy_visual_selection's nvim_feedkeys('y','nx',false) consumes
# it. That is what makes the feature work rather than a happy accident.
wtype -M ctrl -M shift -k c -m shift -m ctrl
sleep "$SETTLE"

# Park at the last line -- one of the two EMPTY ones seeded above -- before pasting.
# See the seeding comment: this is what stops the pasted text splicing into the
# marker's own line and being counted as a single occurrence.
wtype -- "G"
sleep "$SETTLE"

# The PASTE chord. Same capture-phase route; see PASTE_SETTLE above for why the
# settle after it is doubled.
wtype -M ctrl -M shift -k v -m shift -m ctrl
sleep "$PASTE_SETTLE"

# READBACK -- every line of the buffer, as Neovim itself sees it.
wtype -- ":call writefile(nvim_buf_get_lines(0, 0, -1, v:false), '$CLIP_READBACK')"
wtype -k Return
wait_file "$CLIP_READBACK" || exit 12

quit_app 13 14

exit 0
"#;

/// Copy and Paste carry text through the SYSTEM CLIPBOARD and back into the buffer:
/// Ctrl+Shift+C on a linewise visual selection, then Ctrl+Shift+V, and the marker is
/// in the buffer TWICE.
///
/// ## What it proves, and why the observable is a file rather than a pixel
///
/// One launch, no capture, one readback. The scenario seeds the buffer with
/// [`CLIP_MARK`] on line 1 and two empty lines under it, selects line 1 with `V`,
/// presses the copy chord, moves to the last (empty) line, presses the paste chord,
/// and asks Neovim to write EVERY LINE of its own buffer out. The assertion is that
/// [`CLIP_MARK`] appears exactly [`CLIP_EXPECTED_OCCURRENCES`] times.
///
/// The empty landing line is not padding — [`CLIPBOARD_SCENARIO`] carries the
/// derivation. `nvim_paste` performs a CHARWISE put in normal mode, so a paste onto
/// the marker's own line splices the two into one occurrence and a working build
/// fails with a broken build's count.
///
/// That is an integer-shaped assertion on a file the captive Neovim wrote ABOUT ITS
/// OWN BUFFER, which is why this smoke needs no row in any fixture-name guard: no
/// sidebar row, no header-bar title and no `:e` toast can contribute to a count taken
/// inside the buffer, so the vacuity that
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` exists to prevent is
/// structurally unreachable here. It also captures no PNG, so it contributes no row
/// to `ASSERTED_OCR_TERMS` either — the same treatment the divider-relayout and
/// column-cap smokes get, and for the same reason.
///
/// ## The four falsifiers, and how their DIAGNOSTICS differ
///
/// All four produce ONE occurrence, so the panic text alone separates none of them.
/// What separates them is everything around it. MEASURED — all four reverts were
/// run, and the columns below are what each one actually produced:
///
/// | revert | smoke | fast suite | app stderr |
/// | --- | --- | --- | --- |
/// | **A** delete `clipboard.set_text(text)` in `shell::copy_selection` | red | green | `wolf-gnome: clipboard read failed: No compatible transfer format found` |
/// | **B** delete `core.paste(..)` in the async read callback | red | green | nothing extra |
/// | **C** restore `Command::Copy => UiAction::Deferred(..)` | red | **red** at `command_action::ui_action_table` and `dispatched_commands_are_the_expected_set` | the same `clipboard read failed:` line |
/// | **D** set `win.copy`'s `status` back to `Reserved` | red | **red** at `actions::action_for_key_table`, `bound_set_is_exactly_this_unit` and `menu_never_lists_a_reserved_action` | the same `clipboard read failed:` line |
///
/// Read them in two steps. **The stderr line says which HALF broke.** It appears
/// whenever the copy half wrote nothing at all — A, C and D — because an empty
/// clipboard makes `read_text_async` resolve `Err`, not `Ok(None)`, and
/// `shell::paste_clipboard` prints it rather than swallowing it. Its ABSENCE is
/// therefore B's signature, and B alone: the text reached the clipboard and the
/// paste side dropped it. That the diagnostic exists at all is the "never swallow
/// the `Err`" rule paying for itself.
///
/// **Then the fast suite says which of A, C and D.** A leaves it green; C and D
/// redden it, at different tests, because each is a revert of a rule the fast suite
/// pins directly. C and D produce identical stderr, so the reddened test names are
/// the whole of what tells them apart.
///
/// One natural guess is wrong and is worth stating, because it would send a reader
/// looking for a line that never comes: **C prints no `command Copy not dispatched:`
/// line.** That line is emitted by `dispatch_command` alone, which is the drain and
/// palette path. A chord takes `install_chord_controller` → `action_is_enabled`,
/// which answers false for an action `install_actions` skipped, and returns
/// `Propagation::Proceed` — forwarding the key and printing nothing.
///
/// C and D do both end with the chord reaching Neovim as `<C-c>`, which is the
/// accelerator invariant holding rather than failing: a broken build here is one
/// where Ctrl+Shift+C interrupts rather than copies, never one where a Neovim key
/// became unreachable.
///
/// ## The conventions it obeys, including the one that does not bite
///
/// A fresh temp working directory ([`make_temp_cwd`], mandatory — the captive
/// Neovim's swap directory is keyed on it); [`require_smoke_env`]'s cage slot and runtime directory for the whole body; one
/// `sh -c` string through [`cage_command`], which is the single environment helper
/// and the only place the compositor is spawned; and four exit codes that name four
/// causes with no two folded together.
///
/// The "press no key after one that produced a message" rule does not bite here,
/// and the reason is worth stating rather than leaving as an omission: that rule
/// protects an OCR assertion on a toast, and this smoke reads no pixels at all. The
/// keys after the paste chord are one Ex command whose only product is the readback
/// file.
///
/// ## What it does NOT prove
///
/// **That the clipboard reaches another Wayland client.** `gdk::Clipboard` can serve
/// a `read_text_async` out of its own local content provider when the same client
/// owns the selection, so this round trip may never leave the process. `wl-copy` /
/// `wl-paste` are deliberately not in [`REQUIRED_TOOLS`] and would not settle it
/// either: wlroots delivers a selection offer to the KEYBOARD-FOCUSED client, and a
/// second client started under `cage` alongside the app has no focus. Proving the
/// compositor half needs a focus dance no smoke here performs.
///
/// **That the refusal arms are ever taken.** No smoke can arrange a dead, wedged or
/// absent core behind a chord press; `clipboard_ui`'s own tables cover the whole
/// domain headlessly, the same shape as `nav_ui`'s and the buffer picker's refusals.
#[test]
fn copy_and_paste_round_trip_through_the_clipboard_under_cage() {
    const SMOKE: &str = "copy_and_paste_round_trip_through_the_clipboard_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Mandatory: the swap directory is keyed on the cwd and outlives the process, so a leftover swap
    // raises a prompt that eats the next run's first keystroke.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    let readback = runtime_dir.join(CLIP_READBACK_NAME);
    clear_evidence(&readback, SMOKE);

    let mut cmd = cage_command(CLIPBOARD_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        .env("CLIP_MARK", CLIP_MARK)
        .env("CLIP_READBACK", &readback);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless clipboard scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage clipboard scenario exited unsuccessfully ({status}). Exit 11: the app \
         died before it was ready for input, so no chord was ever pressed and nothing below is about \
         the clipboard. Exit 12: the buffer readback never appeared at {}, so the app never got as \
         far as executing the final `:call writefile(...)` — a panic on the paste path (the async \
         clipboard callback runs on the GTK main thread and takes the core borrow) is the first \
         thing to look at. Exit 13: still alive after `:qa!`. Exit 14: it went away, but not \
         cleanly — a panic in the GTK main loop makes a process vanish exactly as a clean shutdown \
         does, which is why these last two are separate codes.",
        readback.display()
    );

    // Counted in a file Neovim wrote from its own buffer, so nothing else on screen affects it.
    let lines = read_readback(&readback, SMOKE);
    let occurrences = lines.matches(CLIP_MARK).count();
    assert_eq!(
        occurrences, CLIP_EXPECTED_OCCURRENCES,
        "{SMOKE}: {CLIP_MARK:?} appears {occurrences} time(s) in the buffer readback, want exactly \
         {CLIP_EXPECTED_OCCURRENCES} — once for the line the scenario seeded and once for the copy \
         that came back through the system clipboard.\n\
         ZERO occurrences is a HARNESS failure, not a feature one: the seeding \
         `:call nvim_buf_set_lines(...)` never executed, so nothing was ever selected and nothing \
         could have been copied. Check that keys are reaching Neovim at all (the keyboard-only \
         smokes first), not the clipboard code.\n\
         ONE occurrence is the FEATURE failure, and it has four candidate causes this message \
         cannot name on its own. The doc comment's MEASURED table separates them in two steps. \
         FIRST, look on the app's stderr for `wolf-gnome: clipboard read failed:`. It appears \
         whenever the COPY half wrote nothing — an empty clipboard makes `read_text_async` resolve \
         Err, not Ok(None) — so its ABSENCE means the copy worked and the paste side dropped it: \
         `core.paste(..)` is missing from the async read callback in `shell.rs`. SECOND, if the \
         line IS there, read the fast suite: GREEN means `clipboard.set_text(text)` is missing \
         from `shell::copy_selection`; RED at `command_action::ui_action_table` means Copy was \
         demoted to Deferred; RED at `actions::action_for_key_table` means `win.copy` was demoted \
         to Reserved. Do NOT go looking for a `command Copy not dispatched:` line — a chord never \
         takes the `dispatch_command` path that prints it, so it does not appear under any of the \
         four.\n\
         MORE than {CLIP_EXPECTED_OCCURRENCES} means the paste was delivered more than once — a \
         chord handled in both the capture and the bubble phase, or an `InputEvent::Paste` queued \
         per callback invocation.\n\
         Readback file kept at {}. Its contents were:\n{lines}",
        readback.display()
    );
}

/// Alt+Left navigates the focused pane BACK to the file it was showing before.
///
/// ## What it covers that nothing else does
///
/// The whole per-pane navigation path, end to end and in the only place all of it
/// is joined together:
///
/// * the capture-phase chord controller resolving `<Alt>Left` through
///   `actions::action_for_key` — GTK's own keymap-aware matching does not fire
///   under `wtype`, so this is the real matcher;
/// * `win.history-back` being INSTALLED at all. It is the first action in this
///   crate with no `wolf_core::Command`, so `shell::install_actions` reaches it
///   only through `command_action::ui_action_for_front_end_action`; the ordinary
///   command branch never sees it, and a row that resolves in neither place is
///   skipped with a printed reason and an accelerator pointing at nothing;
/// * `shell::perform`'s `UiAction::HistoryBack` arm and `AppShell::navigate`;
/// * `nav_ui::nav_press` ACCEPTING against a live core with a focused window —
///   the seam's tables cover the refusals, and nothing but this smoke covers the
///   acceptance reaching a real `WolfCore`;
/// * `WolfCore::history_back`'s blocking move through `_G.wolf_history_set_buf`,
///   and the repaint that follows it.
///
/// Forward is NOT smoked. `nav_press` / `nav_buttons` treat the two symmetrically
/// and `history_forward` mirrors `history_back` in core, but the widget wiring for
/// Forward is undiscriminated — recorded in `tests/README.md`, "What is still
/// unproven".
///
/// ## Why the readbacks exist
///
/// OCR alone cannot tell "Back worked" from "the second `:e` never happened":
/// both leave [`NAVBACK_HEADING_FIRST`] on screen. The scenario therefore has the
/// captive Neovim write `bufname('%')` before and after the chord, with their own
/// exit codes, so the failure is ATTRIBUTED rather than merely detected.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. The `measured` column is from an actual red run, per
/// `tests/README.md`'s "A red-run demonstration is a deliverable, not a formality".
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | n/a — no revert of its own. GREEN under row 4's revert, which is the designed behaviour: the scenario exits 0 whether or not the chord did anything, so the run got as far as assertion 4. |
/// | 2 | [`assert_png_present`] | precondition — grim produced an image | none; do not invent one | n/a — no revert of its own. GREEN under row 4's revert; that run's panic message names the retained capture at `/run/user/1000/wolf_gnome_navback_smoke.png`. |
/// | 3 | `$BEFORE` names [`NAVBACK_FILE_SECOND`] | **precondition, not a discriminator** — it proves the second `:e` took effect, so a failure of assertion 4 can be attributed to the navigation rather than to a keystroke that never landed | none | n/a — no revert of its own. GREEN under row 4's revert, and load-bearing there: assertion 4's message could say "the pane really was on `nbyankee.md` going in" only because this row had already passed. |
/// | 4 | `$AFTER` names [`NAVBACK_FILE_FIRST`] | **the discriminator** | replace `shell::perform`'s `UiAction::HistoryBack` arm body with `{}`. The chord still resolves and the action still activates, but nothing navigates, so `bufname('%')` still reads `nbyankee.md`. Isolated: every headless test stays green. | RED, in 6.57 s, with that arm body replaced by `{}`: `after Alt+Left Neovim reported its current buffer as "nbyankee.md", not "nbxray.md". Assertion 3 passed, so the pane really was on "nbyankee.md" going in — the navigation is what did not happen.` — `left: "nbyankee.md"`, `right: "nbxray.md"`; `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 36 filtered out; finished in 6.57s`. The isolation is measured rather than reasoned: `cargo test -p wolf-gnome --lib` under the SAME revert reported `test result: ok. 172 passed; 0 failed`, so this smoke is the only test in the tree that discriminates that arm. |
/// | 5 | OCR contains [`NAVBACK_OCR_PRESENT`] | **aimed at a real property, but UNDISCRIMINATED by any executed revert** — it says the pane really REPAINTED the file it moved to, which the readback alone cannot show. The shape it would uniquely catch is a Neovim that moved while the GTK side never redrew, and that shape leaves assertion 4 GREEN. No revert producing it has been run, and none is invented here to manufacture one. | the same revert | NOT EVALUATED in the recorded red run: under row 4's revert the harness aborted at assertion 4, which is evaluated first and reddens under the same cause. Recorded as measured rather than dressed up as a red run of its own. What it independently discriminates — a Neovim that moved while the GTK side never redrew — leaves assertion 4 green, and no revert producing that shape has been run. |
/// | 6 | OCR lacks [`NAVBACK_OCR_ABSENT`] | **discriminator with NO isolated red run** | the same `UiAction::HistoryBack` revert, which never reaches this assertion: the run aborts at assertion 4, ahead of BOTH OCR assertions, so neither 5 nor 6 has a red run of its own. Nor could anything redden this one alone — with a single pane two headings cannot both be on screen, so it is only ever reached once an earlier assertion has already failed. Its contribution is naming WHICH file is showing, not detecting that the wrong one is: the same shape as the bare-Enter buffer-picker smoke's must-not-contain assertion. | n/a, by design — recorded rather than invented. It was not evaluated in the recorded red run either: that run ended at assertion 4, ahead of both OCR assertions. |
#[test]
fn alt_left_navigates_back_to_the_previous_file_under_cage() {
    const SMOKE: &str = "alt_left_navigates_back_to_the_previous_file_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (NAVBACK_FILE_FIRST, NAVBACK_HEADING_FIRST),
            (NAVBACK_FILE_SECOND, NAVBACK_HEADING_SECOND),
        ],
    );

    // Cleared first: evidence that could be a previous run's leftover is not evidence.
    let before = cwd.join(NAVBACK_READBACK_BEFORE);
    let after = cwd.join(NAVBACK_READBACK_AFTER);
    let out_png = runtime_dir.join("wolf_gnome_navback_smoke.png");
    for path in [&before, &after, &out_png] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(NAVBACK_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("NAVBACK_FILE_FIRST", NAVBACK_FILE_FIRST)
        .env("NAVBACK_FILE_SECOND", NAVBACK_FILE_SECOND)
        .env("BEFORE", &before)
        .env("AFTER", &after)
        .status()
        .expect("failed to spawn `cage` for the headless nav-back smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage nav-back scenario exited unsuccessfully ({status}). Read the exit code \
         FIRST.\n\
         Exit 11: the app died before it was ready for input — a LAUNCH failure, and nothing below \
         is about navigation.\n\
         Exit 24: the BEFORE `bufname('%')` readback never appeared, so keys were not reaching \
         Neovim at all. Check the keyboard-only smokes first; nothing here is diagnosable.\n\
         Exit 25: the AFTER readback never appeared even though the BEFORE one did. Keys were \
         reaching Neovim and then stopped, which is what a chord that WEDGED the app looks like — \
         `AppShell::navigate` holding a `RefCell` borrow across the blocking \
         `WolfCore::history_back` is the first thing to check, and the snapshot read guard being \
         bound to a local in its own statement is the second (the `capture_open_files` \
         precedent).\n\
         Any other status means wtype or grim failed, or a `:e` did not load. Note this assertion \
         says nothing about whether the chord did anything: the scenario exits 0 either way.\n\
         Screenshot expected at {}.",
        out_png.display()
    );

    // --- Assertion 2 (precondition): grim produced an image -------------------
    assert_png_present(SMOKE, &out_png);

    // Assertion 3 (precondition): the second `:e` took effect, so a red 4 means Back failed, not that
    // the pane never left the first file; OCR cannot tell those apart.
    let before_name = read_trimmed(&before, SMOKE);
    assert_eq!(
        before_name,
        NAVBACK_FILE_SECOND,
        "{SMOKE}: before the chord Neovim reported its current buffer as {before_name:?}, not \
         {NAVBACK_FILE_SECOND:?}. This is a PRECONDITION failure, not a navigation one: the second \
         `:e` never took effect, so the pane had nothing to go Back FROM and everything below \
         would be measuring the wrong starting state. Keys reaching the buffer instead of the \
         cmdline is the usual cause. Readback kept at {}.",
        before.display()
    );

    // Assertion 4 (discriminator): an exact buffer name, which no page text can forge.
    let after_name = read_trimmed(&after, SMOKE);
    assert_eq!(
        after_name, NAVBACK_FILE_FIRST,
        "{SMOKE}: after Alt+Left Neovim reported its current buffer as {after_name:?}, not \
         {NAVBACK_FILE_FIRST:?}. Assertion 3 passed, so the pane really was on \
         {NAVBACK_FILE_SECOND:?} going in — the navigation is what did not happen. The likely \
         causes, in order:\n\
         (a) THE CHORD NEVER FIRED. `actions::action_for_key` only scans BOUND rows, so check that \
         `win.history-back`'s BindingStatus is Bound rather than Reserved. If the chord fell \
         through to Neovim instead, it arrived as <A-Left>, which unmapped splits into <Esc> + \
         <Left> — so look for a stray cursor move rather than for nothing at all.\n\
         (b) THE ACTION WAS NOT INSTALLED. `win.history-back` carries no `wolf_core::Command`, so \
         `shell::install_actions` can only reach it through \
         `command_action::ui_action_for_front_end_action`. A row that resolves in NEITHER place is \
         skipped with a printed reason while its accelerator is still registered: the chord is \
         then swallowed in the capture phase and does nothing at all. The app's stderr names it.\n\
         (c) `nav_ui::nav_press` REFUSED. It prints \"navigation not performed: <reason>\" to \
         stderr, and the reason names which arm: a core that is not Alive, or a `Snapshot::active_win` \
         that is None or 0.\n\
         (d) THE MOVE FAILED AND REVERTED. `WolfCore::history_back` returns false when the \
         recorded target is gone or the bind throws, and `apply_nav` then reverts the cursor — the \
         pane legitimately does not move. That is covered headlessly by \
         `crates/wolf-core/tests/nav_history_live.rs`; if it is happening HERE, the pane's history \
         is not what this scenario built.\n\
         Readbacks kept at {} and {}; screenshot at {}.",
        before.display(),
        after.display(),
        out_png.display()
    );

    let ocr_text = ocr_to_lowercase(&out_png);

    // Assertion 5 (undiscriminated): only this proves GTK drew the result. No revert of that shape has
    // been run, so the discrimination is unmeasured; see this test's row 5.
    assert!(
        ocr_text.contains(NAVBACK_OCR_PRESENT),
        "{SMOKE}: OCR did not contain {NAVBACK_OCR_PRESENT:?} — the HEADING of \
         {NAVBACK_FILE_FIRST}, which renders only once the pane really holds that buffer again. \
         Assertion 4 passed, so NEOVIM moved; this says the WINDOW did not draw the result. Check \
         that `AppShell::navigate` ends in a refresh rather than waiting for a wake that may not \
         come, and that the pane view rebinds the grid. If the capture looks right to the eye, \
         suspect OCR density before touching anything — the fillers are written for that reason \
         (see SIDEBAR_FILLER_COUNT). PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 6 (discriminator, no isolated red run): it names which file shows and is reached only
    // after an earlier failure; under the recorded revert, assertion 4 aborts first.
    assert!(
        !ocr_text.contains(NAVBACK_OCR_ABSENT),
        "{SMOKE}: OCR unexpectedly contained {NAVBACK_OCR_ABSENT:?} — the heading of \
         {NAVBACK_FILE_SECOND}, the file the pane was asked to navigate AWAY from. The pane is \
         still showing it, so either the chord did nothing or the move was reverted. Assertion 5 \
         above is red too in that case and it is the one to read first. The assertion is sound: \
         after `:e {NAVBACK_FILE_SECOND}` Neovim's toast and the sidebar row both show the \
         FILENAME, whose lowercase does not contain {NAVBACK_OCR_ABSENT:?}. PNG kept at {} for \
         inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

// Command line, messages and completion: the popup, the multi-line toast and a `confirm()` dialog.
// Each smoke's terms are absent from every other surface, so only the feature satisfies them.

/// The COMPLETION-POPUP scenario: launch, settle, type `:se` and press Tab, then
/// capture without pressing anything else.
///
/// The trailing `Tab` is what opens the popup — `wildoptions+=pum` in the captive
/// `init.lua` is what makes it a vertical popup rather than the horizontal wildmenu —
/// and NOTHING may be typed afterwards, because the next key closes it.
const PUM_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome pum smoke"

# Park the block cursor on its own row so it cannot corrupt a glyph the OCR reads.
wtype -k o
wtype -k Escape
sleep "$SETTLE"

# A partial Ex command, then the completion key. `:se` has four matches, so
# wildmenu shows the menu rather than completing outright.
wtype -- ":se"
sleep "$SETTLE"
wtype -k Tab

# Let the popupmenu_show land and the overlay draw. NO further keys: the next one
# closes the popup.
sleep "$SETTLE"

grim "$OUT_PNG"

set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// Lowercased substring OCR must find in the completion-popup capture: a `:se`
/// completion item that exists ONLY inside the popup.
///
/// Not `set`, which the popup's own selected row also completes INTO the command
/// line — asserting on it would be satisfied by the strip alone, with no popup drawn.
/// `setglobal` is long, unambiguous to OCR, and cannot appear anywhere else on the
/// page: no fixture, no path component and no menu label contains it.
pub(crate) const PUM_OCR_ITEM: &str = "setglobal";

/// A SECOND completion item, asserted alongside [`PUM_OCR_ITEM`].
///
/// One item could in principle be the longest-common-prefix completion Neovim writes
/// into the command line; two items on screen at once can only be a list.
pub(crate) const PUM_OCR_ITEM2: &str = "setfiletype";

/// The MULTI-LINE MESSAGE scenario: launch, settle, `:echo` a two-line string,
/// then capture without pressing anything else.
///
/// The trailing keystroke rule is load-bearing here for a second reason: any key is
/// dismissing input for a non-prompt toast (`wolf_core::InputEvent::is_toast_dismissing`),
/// so one stray key after the Return clears the message this smoke exists to read.
const MSG_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome message smoke"

wtype -k o
wtype -k Escape
sleep "$SETTLE"

# One `msg_show` carrying TWO lines: core splits the content on "\n", so the toast
# holds two lines rather than two messages.
wtype -- ":echo \"$MSG_FIRST\n$MSG_SECOND\""
wtype -k Return

sleep "$SETTLE"

grim "$OUT_PNG"

set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The FIRST line of the two-line `:echo`, and the one that discriminates.
///
/// It is the line a single-row strip cannot show: with only the last line drawn this
/// term is absent and [`MSG_OCR_SECOND`] is present.
pub(crate) const MSG_OCR_FIRST: &str = "victorlima";

/// The SECOND (last) line of the two-line `:echo`.
///
/// Asserted as a control rather than as the feature: a single-row strip draws it
/// too, so on its own it proves only that the message arrived at all.
pub(crate) const MSG_OCR_SECOND: &str = "oscarhotel";

/// The CONFIRM-PROMPT scenario: make an unsaved edit, run `:confirm bdelete`,
/// capture the dialog, answer it with Cancel, and capture again.
///
/// `bdelete` rather than `quit`: an unanswered or mishandled `:confirm q` kills the
/// captive Neovim and the app with it, which would turn a rendering regression into a
/// crash-overlay capture. `bdelete` cannot end the process however the dialog goes,
/// so a failure here stays a failure about pixels.
///
/// Exit codes: `11` the app died before it was ready for input; `14` the app was gone
/// before the second capture, i.e. answering the dialog took the process down.
const CONFIRM_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome confirm smoke"

# An unsaved edit, which is what gives `:confirm bdelete` something to ask about.
wtype -- "i$CONFIRM_EDIT"
wtype -k Escape
sleep "$SETTLE"

wtype -- ":confirm bdelete"
wtype -k Return

# Let the dialog's `msg_show` (kind `confirm`) and its `cmdline_show` prompt land.
sleep "$SETTLE"

grim "$OUT_PNG"

# Answer it: Cancel. Core keeps a `confirm` toast alive across this keypress and
# clears it only when the dialog actually resolves.
wtype -k c
sleep "$SETTLE"

if ! kill -0 "$app_pid" 2>/dev/null; then
    echo "wolf-gnome confirm smoke: app died while answering the dialog" >&2
    exit 14
fi

grim "$OUT_PNG2"

set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The text typed into the startup buffer to make it modified.
///
/// Deliberately NOT an asserted term: it is drawn in the buffer and would still be
/// on screen after the dialog closes, so asserting on it would say nothing about the
/// dialog.
const CONFIRM_EDIT: &str = "dirtybuffer";

/// Lowercased substring OCR must find in the DIALOG capture and must NOT find after
/// the dialog is answered: the distinctive word of Neovim's own
/// `Save changes to "…"?` question, which arrives as a `confirm`-kind message.
pub(crate) const CONFIRM_OCR_QUESTION: &str = "changes";

/// Lowercased substring OCR must find in the DIALOG capture: a fragment of the
/// options line `[Y]es, (N)o, (C)ancel:`, which arrives as the command line's
/// `prompt` rather than as part of the message.
///
/// A FRAGMENT on purpose. The word is wrapped in parentheses on screen, and `--psm
/// 11` reads a parenthesis inconsistently as `(`, `[` or nothing, so the leading `c`
/// cannot be relied on. Everything after it can.
pub(crate) const CONFIRM_OCR_OPTIONS: &str = "ancel";

#[test]
fn completion_popup_lists_matches_under_cage() {
    const SMOKE: &str = "completion_popup_lists_matches_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    let out_png = runtime_dir.join("wolf_gnome_pum.png");
    clear_evidence(&out_png, SMOKE);

    let status = cage_command(PUM_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .status()
        .expect("failed to spawn `cage` for the completion-popup scenario");
    assert!(
        status.success(),
        "{SMOKE}: cage scenario exited unsuccessfully ({status}). Screenshot expected at {}.",
        out_png.display()
    );
    assert_png_present(SMOKE, &out_png);

    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(PUM_OCR_ITEM),
        "{SMOKE}: OCR did not contain the completion item {PUM_OCR_ITEM:?}. That item exists only \
         inside the `ext_popupmenu` popup, so its absence means no popup was drawn — check that \
         `pane_view::place_pum` made the overlay surface visible and that `wildoptions+=pum` is \
         still set in the captive init.lua. If instead the command line reads `:set` and nothing \
         else is on the strip, Neovim completed outright rather than showing a menu, which means \
         the match set changed. PNG kept at {}.\n\nOCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        ocr_text.contains(PUM_OCR_ITEM2),
        "{SMOKE}: OCR contained {PUM_OCR_ITEM:?} but not {PUM_OCR_ITEM2:?}. One item can be a \
         completion written into the command line; two at once can only be a list, so this is the \
         row that separates a popup from a completed word. PNG kept at {}.\n\nOCR output was:\n\
         {ocr_text}",
        out_png.display()
    );
}

#[test]
fn multi_line_message_shows_more_than_its_last_line_under_cage() {
    const SMOKE: &str = "multi_line_message_shows_more_than_its_last_line_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    let out_png = runtime_dir.join("wolf_gnome_message.png");
    clear_evidence(&out_png, SMOKE);

    let status = cage_command(MSG_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("MSG_FIRST", MSG_OCR_FIRST)
        .env("MSG_SECOND", MSG_OCR_SECOND)
        .status()
        .expect("failed to spawn `cage` for the multi-line message scenario");
    assert!(
        status.success(),
        "{SMOKE}: cage scenario exited unsuccessfully ({status}). Screenshot expected at {}.",
        out_png.display()
    );
    assert_png_present(SMOKE, &out_png);

    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(MSG_OCR_SECOND),
        "{SMOKE}: OCR did not contain the message's LAST line {MSG_OCR_SECOND:?}. This is the \
         control, not the feature: a single-row strip draws it too, so its absence means \
         the `:echo` never ran or the toast never drew at all, and the next assertion would be \
         meaningless. PNG kept at {}.\n\nOCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        ocr_text.contains(MSG_OCR_FIRST),
        "{SMOKE}: OCR contained the message's last line {MSG_OCR_SECOND:?} but not its FIRST line \
         {MSG_OCR_FIRST:?}. That is exactly the single-row failure: a \
         two-line `msg_show` reduced to its final line. Check `pane_view::place_toast`'s height \
         and `cmdline_ui::toast_view`'s budget. PNG kept at {}.\n\nOCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
fn confirm_prompt_shows_then_clears_once_answered_under_cage() {
    const SMOKE: &str = "confirm_prompt_shows_then_clears_once_answered_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    let out_png = runtime_dir.join("wolf_gnome_confirm.png");
    let out_png2 = runtime_dir.join("wolf_gnome_confirm_answered.png");
    clear_evidence(&out_png, SMOKE);
    clear_evidence(&out_png2, SMOKE);

    let status = cage_command(CONFIRM_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("OUT_PNG2", &out_png2)
        .env("CONFIRM_EDIT", CONFIRM_EDIT)
        .status()
        .expect("failed to spawn `cage` for the confirm-prompt scenario");
    assert!(
        status.success(),
        "{SMOKE}: cage scenario exited unsuccessfully ({status}). Exit 14 means the app died \
         while the dialog was being answered. Screenshots expected at {} and {}.",
        out_png.display(),
        out_png2.display()
    );
    assert_png_present(SMOKE, &out_png);
    assert_png_present(SMOKE, &out_png2);

    let dialog = ocr_to_lowercase(&out_png);
    assert!(
        dialog.contains(CONFIRM_OCR_QUESTION),
        "{SMOKE}: OCR of the DIALOG capture did not contain {CONFIRM_OCR_QUESTION:?}. The \
         question is a `confirm`-kind message and reaches the screen only through the toast \
         surface, so its absence means the toast was not drawn while the command line was \
         active — the two used to take turns in one row. PNG kept at {}.\n\nOCR output was:\n\
         {dialog}",
        out_png.display()
    );
    assert!(
        dialog.contains(CONFIRM_OCR_OPTIONS),
        "{SMOKE}: OCR of the DIALOG capture contained the question but not the options fragment \
         {CONFIRM_OCR_OPTIONS:?}. The options line arrives as the command line's `prompt`, which \
         is a field `draw_cmdline` has to draw explicitly: dropping it leaves the user answering \
         a dialog whose answers are invisible. PNG kept at {}.\n\nOCR output was:\n{dialog}",
        out_png.display()
    );

    let answered = ocr_to_lowercase(&out_png2);
    assert!(
        !answered.contains(CONFIRM_OCR_QUESTION),
        "{SMOKE}: the dialog was answered with Cancel, but OCR of the SECOND capture still \
         contained {CONFIRM_OCR_QUESTION:?}. A prompt toast must disappear once its dialog \
         resolves; core clears it on that flush, so a surviving one means the front-end kept \
         drawing a hidden toast. PNG kept at {}.\n\nOCR output was:\n{answered}",
        out_png2.display()
    );
}

// --- The save smokes: readback, no OCR. Each checks its target lacks the sentinel first. ---

/// SAVE smoke: the file the chord must write. Its NAME is deliberately unlike
/// [`SAVE_SENTINEL`], which lives only in the buffer the user types into — the
/// `NAVBACK_FILE_FIRST` convention, applied to a file rather than to a page.
pub(crate) const SAVE_FIXTURE_FILE: &str = "svromeo.md";

/// Heading written into [`SAVE_FIXTURE_FILE`] before the launch. It is the file's
/// WHOLE content going in, which is what makes the sentinel's later presence
/// attributable to the save and to nothing else.
const SAVE_FIXTURE_HEADING: &str = "# SVFIXTUREHEAD";

/// The string typed into the buffer and asserted ON DISK afterwards. Nothing else in
/// the scenario writes it, and the scenario contains no `:w` anywhere, so its presence
/// in the file is the chord's doing.
const SAVE_SENTINEL: &str = "zzsavedelta";

/// SAVE smoke: the file the captive Neovim writes `bufname('%')` into BEFORE the
/// chord fires. A PRECONDITION — it proves the `:e` took effect and the typed text
/// landed in the intended buffer, so a failure of the discriminator is attributable to
/// the chord rather than to a lost keystroke.
///
/// It lands under `XDG_RUNTIME_DIR` rather than in the temp cwd, so no new sidebar row
/// and no new picker candidate appears mid-run. Registered in
/// `RELAUNCH_FIXTURE_NAMES` anyway, for `CD_READBACK_NAME`'s reason: its ABSOLUTE
/// path is echoed in the typed `:call writefile(...)` command line.
pub(crate) const SAVE_READBACK_BEFORE_NAME: &str = "wolf_gnome_save_before.txt";

/// The SAVE scenario, run as a single `sh -c` string inside `cage` (the
/// [`NAVBACK_SCENARIO`] shape): launch, poll until `cage` is up, settle, `:e` the
/// fixture, type the sentinel into it, read `bufname('%')` back, fire Ctrl+Shift+S,
/// settle, tear down.
///
/// ## What it must NOT contain, and why that is load-bearing
///
/// **No `:w`, no `:x`, no `:wq`, and no quit path that saves.** The whole
/// discriminator is that [`SAVE_SENTINEL`] reaches the disk, so any other route from
/// the buffer to the file would satisfy it with the chord doing nothing. The teardown
/// signals the app rather than typing at it, and `WolfCore::shutdown` writes no buffer.
///
/// **No `grim` and no OCR.** Nothing here is a claim about pixels.
///
/// ## The premise this rests on, stated rather than assumed
///
/// If the chord does NOT fire, the key falls through to Neovim as `<C-s>` in NORMAL
/// mode, which neither Neovim nor `runtime/nvim/init.lua` maps to anything (the built-in
/// `<C-s>` signature-help mapping is insert/visual/select mode only, and init.lua's
/// only `<C-s>`-bearing map is the two-key `<C-w><C-s>`). So a fall-through writes
/// nothing, which is exactly what makes the disk readback discriminating. The Escape
/// before the chord is what puts the editor in normal mode and is therefore part of
/// that premise, not tidiness.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `24` — the `bufname('%')` readback never appeared, so keys were not reaching
///   Neovim at all and nothing below is diagnosable.
const SAVE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for a readback file.
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
wait_ready "$app_pid" 11 "wolf-gnome save smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there. (The NAVBACK_SCENARIO body, verbatim.)
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
    echo "wolf-gnome save smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 1: open the fixture, so the buffer has a NAME to be written to ------
#
# A named buffer is the whole point: `save_ui::save_press` answers Write for one and
# opens a dialog only for a buffer with no file name at all.
wtype -- ":e $SAVE_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: make the buffer differ from the file on disk ---------------------
#
# `i` enters insert mode at the top of the buffer, the sentinel is typed, Escape
# returns to NORMAL mode -- which is also what makes an un-fired chord a no-op, since
# Neovim maps <C-s> in insert/visual/select mode only. Nothing writes yet.
wtype -- i
wtype -- "$SAVE_SENTINEL"
wtype -k Escape
sleep "$SETTLE"

# --- Step 3: the PRECONDITION readback ----------------------------------------
#
# Proves the `:e` took effect, so a failure of the disk assertion can be attributed to
# the chord rather than to a keystroke that never landed.
wtype -- ":call writefile([bufname('%')], '$BEFORE')"
wtype -k Return
wait_file "$BEFORE" || exit 24

# --- Step 4: THE CHORD UNDER TEST ---------------------------------------------
#
# `-M <mod>` / `-m <mod>` press / release a modifier and `-k <key>` presses+releases a
# key while the held modifiers apply, exactly as in the other Ctrl+Shift scenarios.
# Chord delivery is isolated to this single line.
#
# If the action were NOT installed, `AppShell::install_chord_controller` returns
# Propagation::Proceed and this reaches Neovim as <C-s>, which is unmapped in normal
# mode -- so a broken build writes nothing at all, which is what the disk readback
# distinguishes.
wtype -M ctrl -M shift -k s -m shift -m ctrl

# Wait for nvim's `write` to land: the sentinel exists only in the buffer, so it
# reaching the file is the write itself. A timeout falls through on purpose, so the
# Rust assertion on the file's contents names the red.
wait_file_contains "$SAVE_FILE" "$SAVE_SENTINEL" "wolf-gnome save smoke" || true

exit 0
"#;

/// **Ctrl+Shift+S writes the focused buffer to its own file, with no dialog.**
///
/// ## What it covers that nothing else does
///
/// The whole Save path, end to end and in the only place all of it is joined:
///
/// * the capture-phase chord controller resolving `<Control><Shift>s` through
///   `actions::action_for_key` — GTK's own keymap-aware matching does not fire under
///   `wtype`, so this is the real matcher;
/// * `win.save` being INSTALLED at all. It carries no `wolf_core::Command`, so
///   `shell::install_actions` reaches it only through
///   `command_action::ui_action_for_front_end_action`; a row that resolves in neither
///   place is skipped with a printed reason while its accelerator is still registered,
///   which swallows the chord and does nothing;
/// * `shell::perform`'s `UiAction::SaveBuffer` arm and `AppShell::save_buffer`;
/// * `save_ui::save_press` answering `Write` against a live core over a NAMED buffer —
///   the seam's tables cover every other input, and nothing but this smoke covers the
///   acceptance reaching a real `WolfCore`;
/// * `InputEvent::Save` riding the ordered input channel into nvim's `write`.
///
/// ## Why the observable is a file rather than a pixel
///
/// "The buffer was saved" has no rendering. A screenshot after a correct save and one
/// after a save that never happened are the same image, since the buffer's text is on
/// screen either way. The disk is the only place the two differ.
///
/// ## A warning about the WRONG failure mode
///
/// This smoke sets [`SAVE_AS_GDK_DEBUG`] even though its green path opens NO dialog,
/// and the reason is the red path rather than the green one. Should an edit ever make
/// `save_press` answer `Choose` for a NAMED buffer, `AppShell::open_save_dialog` runs;
/// without the variable GTK takes the portal route, and wherever that route reaches
/// the host's portal a modal file dialog opens on the developer's DESKTOP while this
/// smoke hangs holding its cage slot. Each cage's only bus is its own private one, so
/// the variable is defence in depth; see [`SAVE_AS_GDK_DEBUG`]. With it, that same wrong
/// answer merely opens an in-process dialog inside `cage` that nothing answers, and the
/// smoke fails on its own assertion instead. If that happens, the finding is in
/// `save_ui::save_press`. See [`the_save_dialog_writes_a_named_file_under_cage`].
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. The `measured` column carries an actual red run, per
/// `tests/README.md`'s "A red-run demonstration is a deliverable".
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 0 | the fixture on disk lacks [`SAVE_SENTINEL`] before the launch | precondition — the evidence is not pre-satisfied by the fixture | none | n/a |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | green under row 3's revert, as predicted: the scenario exits 0 whether or not anything saved, so the red arrives at row 3 |
/// | 2 | `$BEFORE` names [`SAVE_FIXTURE_FILE`] | precondition — the `:e` took effect, so a failure of row 3 is the chord's | none | green under row 3's revert |
/// | 3 | the fixture on disk CONTAINS [`SAVE_SENTINEL`] | **the discriminator** | replace `shell::perform`'s `UiAction::SaveBuffer` arm body with `{}`. The chord still resolves and the action still activates; nothing writes; the file still holds only its heading. | red: `after Ctrl+Shift+S the file …/svromeo.md does not contain "zzsavedelta", so the buffer was never written`, with `File contents were "# SVFIXTUREHEAD\n"` — the heading alone. NOT isolated: [`the_save_dialog_writes_a_named_file_under_cage`] reddens with it, since that smoke reaches the dialog through this same arm, and the run reads `test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 68 filtered out; finished in 13.69s`. The pair reddening together says the ARM was lost; this one alone would say the `Write` branch was |
/// | 4 | the saved file still contains [`SAVE_FIXTURE_HEADING`] | corroboration — the file was written FROM THE BUFFER rather than replaced by something carrying only the sentinel | none | not reached under row 3's revert, which fires first. Green on every passing run, and its `File contents were` dump in row 3's red is what shows the heading surviving |
#[test]
fn ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage() {
    const SMOKE: &str = "ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd holding the fixture plus the sidebar fillers. See
    // `make_temp_cwd` for why the isolation is mandatory rather than tidy.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[(SAVE_FIXTURE_FILE, SAVE_FIXTURE_HEADING)]);

    let fixture = cwd.join(SAVE_FIXTURE_FILE);
    let before = runtime_dir.join(SAVE_READBACK_BEFORE_NAME);
    clear_evidence(&before, SMOKE);

    // Assertion 0 (precondition): the fixture's own bytes must not carry the sentinel, or assertion 3
    // passes with nothing saved. Read from the file, since assertion 3 reads the file.
    let seeded = std::fs::read_to_string(&fixture).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: could not read the fixture {} that `write_relaunch_fixtures` just wrote: {e}",
            fixture.display()
        )
    });
    assert!(
        !seeded.contains(SAVE_SENTINEL),
        "{SMOKE}: the fixture {} already contains {SAVE_SENTINEL:?} BEFORE the app runs, so \
         assertion 3 would be satisfied by the fixture rather than by the save — a permanent \
         silent pass, not a flake. Its contents were {seeded:?}. Change SAVE_FIXTURE_HEADING; do \
         NOT change SAVE_SENTINEL and do NOT weaken the assertion.",
        fixture.display()
    );
    assert!(
        !SAVE_FIXTURE_FILE.to_lowercase().contains(SAVE_SENTINEL),
        "{SMOKE}: the fixture NAME {SAVE_FIXTURE_FILE:?} contains {SAVE_SENTINEL:?}. It is not a \
         hazard to this smoke's own disk assertion, but it is one to every OCR smoke that runs \
         after a later change makes this sentinel an asserted term — keep the two deliberately \
         unlike, as the sibling smokes do."
    );

    let status = cage_command(SAVE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("SAVE_FILE", SAVE_FIXTURE_FILE)
        .env("SAVE_SENTINEL", SAVE_SENTINEL)
        .env("BEFORE", &before)
        // Set though the green path opens no dialog: it keeps a falsifying edit's dialog inside cage. See
        // [`SAVE_AS_GDK_DEBUG`].
        .env("GDK_DEBUG", SAVE_AS_GDK_DEBUG)
        .status()
        .expect("failed to spawn `cage` for the headless save smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage save scenario exited unsuccessfully ({status}). Read the exit code \
         FIRST.\n\
         Exit 11: the app died before it was ready for input — a LAUNCH failure, and nothing below \
         is about saving.\n\
         Exit 24: the `bufname('%')` readback never appeared, so keys were not reaching Neovim at \
         all. Check the keyboard-only smokes first; nothing here is diagnosable.\n\
         Any other status means wtype failed or the `:e` did not load. Note this assertion says \
         nothing about whether the chord did anything: the scenario exits 0 either way. Readback \
         expected at {}.",
        before.display()
    );

    // --- Assertion 2 (PRECONDITION, not a discriminator) ----------------------
    let before_name = read_trimmed(&before, SMOKE);
    assert_eq!(
        before_name,
        SAVE_FIXTURE_FILE,
        "{SMOKE}: before the chord Neovim reported its current buffer as {before_name:?}, not \
         {SAVE_FIXTURE_FILE:?}. This is a PRECONDITION failure, not a save one: the `:e` never took \
         effect, so the sentinel was typed into some OTHER buffer and assertion 3 would be reading \
         a file nothing was ever going to write. Keys reaching the buffer instead of the cmdline is \
         the usual cause. Readback kept at {}.",
        before.display()
    );

    // --- Assertion 3 (THE DISCRIMINATOR): the buffer reached the disk ---------
    let saved = std::fs::read_to_string(&fixture).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: could not read {} after the run: {e}. The file existed before the launch \
             (assertion 0 read it), so this means the run DELETED it, which no path in this \
             application does.",
            fixture.display()
        )
    });
    assert!(
        saved.contains(SAVE_SENTINEL),
        "{SMOKE}: after Ctrl+Shift+S the file {} does not contain {SAVE_SENTINEL:?}, so the buffer \
         was never written. Assertion 2 passed, so the sentinel really was typed into THAT buffer. \
         The likely causes, in order:\n\
         (a) THE CHORD NEVER FIRED. `actions::action_for_key` scans BOUND rows only, so check that \
         `win.save`'s BindingStatus is Bound rather than Reserved. If the chord fell through to \
         Neovim it arrived as <C-s>, which is unmapped in normal mode — so look for nothing at all \
         having happened rather than for a stray edit.\n\
         (b) THE ACTION WAS NOT INSTALLED. `win.save` carries no `wolf_core::Command`, so \
         `shell::install_actions` can only reach it through \
         `command_action::ui_action_for_front_end_action`. A row that resolves in NEITHER place is \
         skipped with a printed reason while its accelerator IS registered: the chord is then \
         swallowed in the capture phase and activates nothing. The app's stderr names it.\n\
         (c) `save_ui::save_press` REFUSED or answered Choose. It prints \"save not performed: \
         <reason>\" to stderr on a refusal; a `Choose` prints nothing and opens a native dialog \
         instead — and because this scenario exports SAVE_AS_GDK_DEBUG, that dialog is GTK's \
         in-process fallback inside `cage` rather than a portal request on the developer's \
         desktop. Nothing answers it, so a stray `Choose` surfaces HERE, as this assertion \
         failing, rather than as a hang.\n\
         (d) `AppShell::save_buffer` sent nothing, or `InputEvent::Save` never reached nvim's \
         `write`. `crates/wolf-core/tests/save_open.rs::save_writes_file` covers that half \
         headlessly; if it is green, the finding is in this crate.\n\
         File contents were {saved:?}; it is kept at {} for inspection.",
        fixture.display(),
        fixture.display()
    );

    // Assertion 4 (corroboration): the sentinel and the fixture's heading together prove the buffer
    // was written, not just the sentinel.
    assert!(
        saved.contains(SAVE_FIXTURE_HEADING),
        "{SMOKE}: the saved file no longer contains its original heading \
         {SAVE_FIXTURE_HEADING:?}, so what landed on disk is not the buffer the user was editing — \
         nvim's `write` writes the WHOLE buffer, and that buffer opened on this file. Contents were \
         {saved:?}; kept at {}.",
        fixture.display()
    );
}

/// SAVE AS smoke: the name TYPED into the dialog's entry, and the file the save must
/// therefore create in the temp cwd.
///
/// Deliberately unlike [`SAVEAS_SENTINEL`], which lives only in the buffer, and
/// deliberately NOT the name the dialog proposes — see [`SAVEAS_PROPOSED_NAME`].
///
/// The name is asserted EXACTLY, and that exactness is what carries the whole
/// select-all measurement: a run in which the typed characters were appended to the
/// pre-filled `Untitled.md` — or prepended to it — produces a third name and no file
/// under this one, so assertion 3 reddens. Measured on this host, assertion 3 green:
/// `wtype -M ctrl -k a -m ctrl` in the dialog's pre-filled name entry REPLACES the
/// proposal, and the path handed back to the callback is the typed name alone.
pub(crate) const SAVEAS_TYPED_NAME: &str = "svmike.md";

/// The string typed into the NAMELESS buffer and asserted on disk afterwards.
const SAVEAS_SENTINEL: &str = "zzsaveasfoxtrot";

/// The name the dialog PRE-FILLS, read from the production constant rather than
/// retyped so it cannot drift.
///
/// It is the look-alike assertion 4 asserts against: it says the buffer did not ALSO
/// reach disk under the name nobody typed.
///
/// **What it cannot see, and why the note matters here rather than in a report.** A run
/// in which the entry was never edited writes NO file at all, so assertion 4 passes
/// vacuously on it. The proposal is `<cwd>/Untitled.md`, which is the name
/// `runtime/nvim/init.lua` has already given the startup buffer, and `saveas!` against a
/// name another loaded buffer holds raises `E139: File is loaded in another buffer` —
/// the bang overrides "file exists", not a buffer-name collision. Measured directly
/// against the pinned Neovim: a named buffer, a typed `:enew`, then `saveas!` back at
/// the first name errors E139 and leaves the directory empty, with and without
/// `hidden`. That case, and a concatenated name, both redden assertion 3 instead.
const SAVEAS_PROPOSED_NAME: &str = wolf_gnome::save_ui::UNTITLED_SAVE_NAME;

/// SAVE AS smoke: the file the captive Neovim writes `bufname('%')` into after the
/// typed `:enew`. A PRECONDITION, and the sharpest one this smoke has — the entire
/// route depends on the buffer being genuinely NAMELESS, which is what makes
/// `save_ui::save_press` answer `Choose` and open the dialog.
///
/// Under `XDG_RUNTIME_DIR` for [`SAVE_READBACK_BEFORE_NAME`]'s reason.
pub(crate) const SAVEAS_READBACK_NAMELESS_NAME: &str = "wolf_gnome_saveas_nameless.txt";

/// The value of `GDK_DEBUG` **both** save smokes export: defence in depth against a
/// dialog opening on the developer's desktop.
///
/// Measured on a bus that could activate the host's portal:
/// `gdk_display_should_use_portal` answered true, GTK exported a window handle through
/// `zxdg_exporter_v2`, which `cage` does not implement, and proceeded to
/// `show_portal_file_chooser(self, NULL)` anyway — so the request went to the HOST's
/// `org.freedesktop.portal.Desktop`, **a modal file dialog opened on the developer's
/// desktop and this smoke hung** until it was answered or the app was killed, holding
/// its cage slot the whole time. Each cage's only bus is its own from
/// [`cage_command`]'s `dbus-run-session`, and [`smoke_bus_config`] gives it no service
/// directory, so no portal is reachable from it; the export keeps GTK's in-process
/// chooser, the one this smoke was measured against, without resting on that, and
/// guards against the isolation regressing.
///
/// `no-portals` forces GTK's in-process fallback, which was measured under `cage` to
/// present (`visible=true mapped=true realized=true`, 881x326) and to return
/// `Ok(<cwd>/typedname.md)` to the callback after a typed name plus Return.
///
/// ## Why [`ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage`] exports it too,
/// though it opens no dialog
///
/// Because the hazard belongs to that smoke's RED path, not its green one. On a green
/// run Ctrl+Shift+S over a NAMED buffer answers `Write` and no dialog is ever
/// constructed, so the variable does nothing. But any falsifying edit that makes
/// `save_press` answer `Choose` for a named buffer — including the obvious one, dropping
/// the `save_name` check — reaches `AppShell::open_save_dialog`. Wherever a portal is
/// reachable, that opens a modal on the developer's DESKTOP and hangs the smoke.
/// With it, the same wrong answer opens an unanswered in-process dialog inside `cage`
/// and the smoke fails on its own assertion. It costs nothing green.
///
/// This is PER-SMOKE SCENARIO ENVIRONMENT and never production behaviour.
const SAVE_AS_GDK_DEBUG: &str = "no-portals";

/// The SAVE AS scenario, run as a single `sh -c` string inside `cage`.
///
/// ## The route, stated plainly
///
/// Save As has NO chord and its menu item is unreachable (the primary-menu popover is
/// a documented deliberate test gap), so this smoke reaches the dialog through the
/// OTHER door: `Ctrl+Shift+S` on a NAMELESS buffer, which `save_ui::save_press`
/// answers `Choose`. That is a genuine end-to-end path through `save_press`'s `Choose`
/// arm, `save_dialog_proposal`'s unnamed branch, `AppShell::open_save_dialog`, the
/// callback, `save_as_target` and `InputEvent::SaveAs`.
///
/// ## Why a TYPED `:enew`
///
/// It is the only way to reach a genuinely nameless buffer. `runtime/nvim/init.lua`
/// names an anonymous buffer only at `VimEnter` (the startup buffer) and inside
/// `wolf_new_untitled` (the front-end's own New Buffer), so a typed `:enew` leaves the
/// new buffer with an EMPTY `bufname('%')` — which the readback below measures rather
/// than assumes, because the whole route depends on it.
///
/// The other half of that premise is on the core side and is read out of the source
/// rather than assumed: `:enew` fires `BufEnter`, init.lua's current-file autocmd emits
/// `expand('%:p')` — the empty string for an unnamed buffer — and
/// `redraw::apply_current_file` maps an EMPTY path to `None` rather than leaving the
/// previous value in place. So `Snapshot::current_file` really does become `None` here,
/// which is what makes `save_ui::save_press` answer `Choose`. Were it to keep the
/// startup buffer's `<cwd>/Untitled.md`, the chord would answer `Write` and this
/// scenario would silently save the wrong buffer instead of opening a dialog.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `26` — the nameless-buffer readback never appeared, so keys were not reaching
///   Neovim at all and nothing below is diagnosable.
const SAVEAS_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# The native dialog is a separate toplevel: it has to map, realize and take keyboard
# focus before a keystroke can reach its name entry, and it has to close and run its
# callback before the save is issued. Both budgets are this one.
DIALOG_SETTLE=2.0
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
wait_ready "$app_pid" 11 "wolf-gnome save-as smoke"

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
    echo "wolf-gnome save-as smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 1: a genuinely NAMELESS buffer --------------------------------------
#
# A TYPED :enew, deliberately. The front-end's own New Buffer runs
# `wolf_new_untitled`, which NAMES the buffer <cwd>/Untitled.md -- and a named buffer
# is answered Write, so it would never reach the dialog at all.
wtype -- ":enew"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: the PRECONDITION readback ----------------------------------------
#
# `bufname('%')` must be EMPTY here. writefile of an empty string still writes one
# newline, so `wait_file`'s `-s` test is satisfied either way and the Rust side reads
# the answer rather than inferring it from a timeout.
wtype -- ":call writefile([bufname('%')], '$NAMELESS')"
wtype -k Return
wait_file "$NAMELESS" || exit 26

# --- Step 3: make the buffer worth saving -------------------------------------
wtype -- i
wtype -- "$SAVEAS_SENTINEL"
wtype -k Escape
sleep "$SETTLE"

# --- Step 4: THE CHORD, which on a nameless buffer opens the DIALOG -----------
wtype -M ctrl -M shift -k s -m shift -m ctrl

# Let the native dialog map, realize and take keyboard focus.
sleep "$DIALOG_SETTLE"

# --- Step 5: replace the pre-filled name, then accept -------------------------
#
# Ctrl+A selects the whole of the GtkEntry's text so the typed name REPLACES the
# proposed `Untitled.md` rather than being appended to it. MEASURED, not assumed: with
# these three lines the callback receives <cwd>/$SAVEAS_TYPED_NAME and the file appears
# under exactly that name. The Rust side asserts the name exactly, so a concatenation in
# either order lands under a third name and reddens assertion 3.
wtype -M ctrl -k a -m ctrl
wtype -- "$SAVEAS_TYPED_NAME"
sleep "$SETTLE"
wtype -k Return

# Let the dialog close, its callback run, and `InputEvent::SaveAs` reach nvim.
#
# A fixed settle on purpose, not a poll for the typed file: assertion 4 checks that
# the buffer did not ALSO reach disk under the proposed name, and a poll would end
# the run the moment the first write landed, before any second one. Its falsifier
# (a second SaveAs to the proposed name) stays green with either wait, because
# Neovim refuses that write with E139, so no measurement licenses shortening this.
sleep "$DIALOG_SETTLE"

exit 0
"#;

/// **The native save dialog names a file, and the buffer is written to it.**
///
/// ## What it covers that nothing else does
///
/// The only end-to-end drive of a native `gtk::FileDialog` in this repository:
/// `save_ui::save_press`'s `Choose` arm reaching a real dialog,
/// `save_dialog_proposal`'s unnamed branch supplying its initial name and folder, the
/// dialog's `Ok(GFile)` reaching the callback, `save_as_target` accepting the chosen
/// path, and the resulting `InputEvent::SaveAs` writing the file.
///
/// ## What it proves and does NOT prove
///
/// * Proves: the action is installed, the seam's `Choose` answer reaches a real
///   `gtk::FileDialog`, the dialog's `Ok(GFile)` reaches the callback, and the
///   resulting `InputEvent::SaveAs` writes the file — against **GTK's in-process
///   fallback**.
/// * Does NOT prove anything about the PORTAL route, which is the route a real desktop
///   session takes (`gdk_display_should_use_portal` answers true wherever GDBus reaches
///   a session bus). That is a different widget, a different overwrite confirmation and
///   a different `GFile` producer.
/// * Does NOT prove the overwrite confirmation, since the target does not exist, nor
///   the front end's `force: true` end to end. Driving GTK's confirmation alert from
///   the keyboard is fragile; `crates/wolf-core/tests/save_open.rs`'s pair covers the
///   EFFECT of `force` against a real Neovim instead.
///
/// ## The select-all, MEASURED rather than assumed
///
/// The measurement behind [`SAVE_AS_GDK_DEBUG`] covered a typed name plus Return
/// returning `Ok(<cwd>/typedname.md)`, but not against a PRE-FILLED entry, where the
/// typed characters could as easily have been appended to the proposal. Measured by
/// running this smoke: `wtype -M ctrl -k a -m ctrl` selects the whole of the entry, the
/// typed name REPLACES the proposed `Untitled.md`, and the path handed back to the
/// callback is `<cwd>/`[`SAVEAS_TYPED_NAME`] alone. Assertion 3 is what carries that
/// measurement — it names the file exactly, so a concatenation in either order is a
/// third name and a red. **Do not weaken it to a glob** if the keystrokes ever need
/// adjusting.
///
/// ## Assertions, classified
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 0 | [`SAVE_AS_GDK_DEBUG`] is `"no-portals"` | tripwire on a PIN — no falsifier in `src/` by construction, the `the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes` shape | edit the const | n/a |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | green under row 3's revert: the scenario exits 0 whether or not a dialog ever opened |
/// | 2 | the `:enew` buffer's `bufname('%')` is EMPTY | precondition, and the sharpest one — a NAMED buffer is answered `Write`, so the dialog would never open and rows 3 and 4 would be about a route that was never taken | none | green under row 3's revert, which is what makes that red attributable to the dialog rather than to the buffer having a name |
/// | 3 | `<cwd>/`[`SAVEAS_TYPED_NAME`] exists and contains [`SAVEAS_SENTINEL`] | **the discriminator** | replace the `SavePress::Choose` branch of `shell::perform`'s `UiAction::SaveBuffer` arm with `{}`. No dialog opens, the typed name lands in the Neovim buffer instead, and no file appears. | red: `the save dialog did not produce …/svmike.md: No such file or directory (os error 2)`. ISOLATED: [`ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage`] stays green under it, because that smoke takes the `Write` branch — the two branches of one arm, told apart — and the run reads `test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 68 filtered out; finished in 13.68s` |
/// | 4 | no file under [`SAVEAS_PROPOSED_NAME`] carries the sentinel | corroborating negative — the buffer did not ALSO reach disk under the name nobody typed. It is NOT what tells the select-all from an unedited entry; see [`SAVEAS_PROPOSED_NAME`] for why that case is vacuous here and row 3 for what does measure it | none of its own; it is reached only when row 3 passes | not reached under row 3's revert. Green on every passing run |
#[test]
fn the_save_dialog_writes_a_named_file_under_cage() {
    const SMOKE: &str = "the_save_dialog_writes_a_named_file_under_cage";

    // Assertion 0: the pin, first, because getting it wrong opens a modal dialog on the developer's
    // desktop and hangs. The literal is retyped, since a const compared with itself pins nothing.
    assert_eq!(
        SAVE_AS_GDK_DEBUG, "no-portals",
        "SAVE_AS_GDK_DEBUG is not \"no-portals\". DO NOT RUN THIS SMOKE until it is. \
         Measured: whenever the app reaches a session bus, `gdk_display_should_use_portal` \
         answers true; GTK then tries to export a window handle through `zxdg_exporter_v2`, \
         which `cage` does not implement, and calls `show_portal_file_chooser(self, NULL)` \
         anyway. The request therefore goes to the HOST's org.freedesktop.portal.Desktop, so a \
         MODAL FILE DIALOG OPENS ON THE DEVELOPER'S DESKTOP and this smoke hangs until it is \
         answered, holding its cage slot. Each cage's only bus is its own private one from \
         `dbus-run-session`; this pin keeps GTK's in-process chooser regardless, and is the \
         defence in depth for when that isolation regresses. `no-portals` forces GTK's in-process \
         fallback, which was measured under `cage` to present at 881x326 and to hand \
         `Ok(<cwd>/typedname.md)` back to the callback. This is per-smoke scenario environment and \
         never production behaviour."
    );

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    let nameless = runtime_dir.join(SAVEAS_READBACK_NAMELESS_NAME);
    let typed = cwd.join(SAVEAS_TYPED_NAME);
    let proposed = cwd.join(SAVEAS_PROPOSED_NAME);
    for path in [&nameless, &typed, &proposed] {
        clear_evidence(path, SMOKE);
    }

    // The typed name and the proposed name are assertion 4's whole discrimination, so they must differ.
    assert_ne!(
        SAVEAS_TYPED_NAME, SAVEAS_PROPOSED_NAME,
        "the name typed into the dialog and the name the dialog proposes are identical, so \
         assertions 3 and 4 are about the SAME file and contradict each other outright — 3 \
         requires it to carry the sentinel and 4 requires it not to, so no run can pass. Keeping \
         the two distinct is also the whole of what lets assertion 3 measure the select-all: only \
         a name unlike the proposal can tell a REPLACED entry from an appended one."
    );

    let mut cmd = cage_command(SAVEAS_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        .env("NAMELESS", &nameless)
        .env("SAVEAS_SENTINEL", SAVEAS_SENTINEL)
        .env("SAVEAS_TYPED_NAME", SAVEAS_TYPED_NAME);

    // Keeps the dialog inside cage rather than on the desktop; chained after `cage_command` to join its
    // environment. See SAVE_AS_GDK_DEBUG and assertion 0.
    cmd.env("GDK_DEBUG", SAVE_AS_GDK_DEBUG);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless save-as scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage save-as scenario exited unsuccessfully ({status}). Read the exit code \
         FIRST.\n\
         Exit 11: the app died before it was ready for input — a LAUNCH failure, and nothing below \
         is about the dialog.\n\
         Exit 26: the `bufname('%')` readback after the typed `:enew` never appeared, so keys were \
         not reaching Neovim at all. Check the keyboard-only smokes first.\n\
         Any other status means wtype failed. If instead this run HUNG, read \
         `SAVE_AS_GDK_DEBUG`: a portal-routed dialog opens on the developer's desktop and waits \
         there. Readback expected at {}.",
        nameless.display()
    );

    // Assertion 2 (precondition): only a nameless buffer gets the dialog, so this separates "the dialog
    // wrote it" from "a plain Save wrote a file with the right name".
    let nameless_answer = read_trimmed(&nameless, SMOKE);
    assert!(
        nameless_answer.is_empty(),
        "{SMOKE}: after the typed `:enew` Neovim reported its current buffer as \
         {nameless_answer:?}, not the empty string. The buffer is NAMED, so \
         `save_ui::save_press` answers Write for it and Ctrl+Shift+S saves it silently instead of \
         opening a dialog — every assertion below would then be about a route that was never \
         taken. The usual cause is that something NAMED the buffer: `runtime/nvim/init.lua` names \
         an anonymous buffer at VimEnter and inside `wolf_new_untitled`, so check that this \
         scenario really typed `:enew` rather than firing the front end's own New Buffer. Readback \
         kept at {}.",
        nameless.display()
    );

    // --- Assertion 3 (THE DISCRIMINATOR) --------------------------------------
    let saved = std::fs::read_to_string(&typed).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: the save dialog did not produce {}: {e}. Assertion 2 passed, so the buffer \
             really was nameless and `save_ui::save_press` should have answered Choose. The likely \
             causes, in order:\n\
             (a) THE DIALOG NEVER PRESENTED. Under GDK_DEBUG={SAVE_AS_GDK_DEBUG:?} it should use \
             GTK's in-process fallback; if the variable is not reaching the app, the dialog went to \
             the desktop portal and this run would normally HANG rather than reach here.\n\
             (b) `shell::perform`'s `UiAction::SaveBuffer` arm never took its `Choose` branch, or \
             `AppShell::open_save_dialog` was not called.\n\
             (c) THE CALLBACK DROPPED THE RESULT. An `Err` is treated as \"the user dismissed the \
             dialog\", so a keystroke that closed it the wrong way looks identical to a cancel.\n\
             (d) `save_ui::save_as_target` REFUSED the chosen path — it answers None for a \
             relative path, an empty one, or one with no file name.\n\
             (e) THE TYPED NAME WAS APPENDED TO THE PROPOSED ONE rather than replacing it, in \
             which case a file exists under some THIRD name in {} — list that directory before \
             changing anything, and see this test's \"select-all, MEASURED\" note.\n\
             (f) THE ENTRY WAS NEVER EDITED AT ALL, in which case the directory is EMPTY rather \
             than holding a stray file: the dialog then returns the proposed \
             {SAVEAS_PROPOSED_NAME:?}, which is the name init.lua has already given the startup \
             buffer, and `saveas!` against a name another loaded buffer holds raises `E139: File \
             is loaded in another buffer` — the bang overrides \"file exists\", not a buffer-name \
             collision. So an empty directory points HERE and not at (a)-(d), and assertion 4 \
             stays green through it.",
            typed.display(),
            cwd.display()
        )
    });
    assert!(
        saved.contains(SAVEAS_SENTINEL),
        "{SMOKE}: {} exists but does not contain {SAVEAS_SENTINEL:?}, so the file was created \
         from something other than the buffer the sentinel was typed into. Contents were \
         {saved:?}.",
        typed.display()
    );

    // Assertion 4 (corroborating negative): the buffer did not also reach disk under the proposed name.
    // An absent file passes on purpose; see SAVEAS_PROPOSED_NAME for the E139 measurement.
    let proposed_contents = std::fs::read_to_string(&proposed).unwrap_or_default();
    assert!(
        !proposed_contents.contains(SAVEAS_SENTINEL),
        "{SMOKE}: the file {} — the name the dialog PROPOSED, not the one that was typed — carries \
         {SAVEAS_SENTINEL:?}. Assertion 3 passed, so the typed name was written too: the buffer \
         reached disk TWICE, under two names, and only one of them was asked for. Something \
         issued a second save — check for a second dialog answer or a stray Ctrl+Shift+S in the \
         scenario. Fix the keystrokes; do NOT relax assertion 3 to a glob. Contents were \
         {proposed_contents:?}.",
        proposed.display()
    );
}
