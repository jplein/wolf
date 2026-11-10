//! Sidebar and search smokes: the file tree, the project-search panel, the sidebar toggle,
//! the window's working directory as `:cd`, Open Recent and the window title show it.

use super::*;

// Tree-sidebar smoke: asserts population only. OCR cannot confirm a highlight and pixel-exact
// row clicks are too flaky, so click-to-open and the highlight are left to unit tests.

/// Tree-sidebar smoke markdown file dropped into the launched app's temp cwd.
/// Its NAME is OCR-friendly and unlikely to be misread; the tree shows this
/// FILENAME (`treenode.md`) in the sidebar regardless of whether the editor has
/// it open.
const TREE_FILE_NAME: &str = "treenode.md";

/// Heading written into [`TREE_FILE_NAME`]. Kept DISTINCT from the filename so a
/// match on [`TREE_OCR`] is unambiguous: the assertion targets the FILENAME word
/// `treenode` (which the filename itself contains), not this heading.
const TREE_HEADING: &str = "# TREEHEADING";

/// Lowercased substring OCR must find to prove the tree rendered the file row:
/// the distinctive filename word (`treenode.md` contains `treenode`).
pub(crate) const TREE_OCR: &str = "treenode";

// Project-search fixtures: the match sits below line 1 and past column 1 for the cursor readback.
// See notes/crates/wolf-gnome/tests/smoke_headless/sidebar.rs.md#project-search-fixtures

/// Project-search smokes: the file a search result must OPEN — the one holding the
/// sole line `PROJSEARCH_QUERY` matches.
///
/// A `.txt`, and that is load-bearing rather than incidental. `wolf_core`'s default
/// `SearchOptions` restricts the search to `--glob '*.md' --glob '*.markdown'`, so
/// under that default this file would never be searched at all: the panel would
/// stay empty, the second Enter would activate nothing, and the smokes would go red.
/// That makes `search_panel::search_options_for`'s `all_files: true` an end-to-end
/// discriminator instead of a table-tested boolean.
pub(crate) const PROJSEARCH_FILE_TARGET: &str = "psrzulu.txt";

/// Line 1 of [`PROJSEARCH_FILE_TARGET`]; lowercased, the rendered line contains
/// [`PROJSEARCH_OCR_PRESENT`]. Deliberately unlike the filename, so neither the
/// sidebar row nor nvim's `:e` toast can satisfy the assertion on its own.
pub(crate) const PROJSEARCH_HEADING_TARGET: &str = "# PROJSEARCHALFA";

/// Lowercased substring OCR must find: the heading of the file the search opened.
///
/// It lives only in that file's CONTENT, so a hit can only mean the pane really
/// re-rendered onto the target. It corroborates the readback rather than replacing
/// it: OCR says "the right file is on screen", and only `line('.')` / `col('.')`
/// say "at the right place in it".
pub(crate) const PROJSEARCH_OCR_PRESENT: &str = "projsearchalfa";

/// Project-search smokes: the file open in the editor when the chord fires, so the
/// pane starts somewhere the search must move it away from.
///
/// A `.md`, so it WOULD be searched under core's default glob — but it does not
/// contain [`PROJSEARCH_QUERY`], which is what keeps the match set at exactly one.
///
/// Its NAME is also an observable: it contains [`PROJSEARCH_OCR_TREE_ROW`], the row
/// only the Files page of the sidebar draws. That is why it is absent from
/// `RELAUNCH_FIXTURE_NAMES` and has an exemption block of its own in
/// `restore_fixture_names_cannot_satisfy_their_own_assertions`.
pub(crate) const PROJSEARCH_FILE_START: &str = "psrxray.md";

/// Lowercased substring of [`PROJSEARCH_FILE_START`]'s tree row: the term that says
/// which sidebar page is on screen.
///
/// Inside the sidebar band only the file tree can draw it. The panel draws the
/// query, the status line and the ONE matched file, [`PROJSEARCH_FILE_TARGET`], and
/// the start file never matches; the `:e` echo and the statusbar strip that also
/// spell the name sit in the pane band. So in the sidebar band its absence means the
/// Search page replaced the tree, and its presence means the Files page is back.
pub(crate) const PROJSEARCH_OCR_TREE_ROW: &str = "psrxray";

/// Lowercased word that only a search RESULT can put on screen before the match is
/// opened: it is the last word of the match line, after [`PROJSEARCH_QUERY`].
///
/// It is never typed, it names no file, and the starting file does not contain it,
/// so until the second Enter opens the target the panel's line row is the one
/// surface that can draw it. In the sidebar band it proves the results streamed into
/// the panel; in the pane band its absence proves no overlay drew them over the
/// editor.
///
/// Short on purpose. The panel's line row wraps at word boundaries into at most two
/// lines and ellipsizes the rest, so the match line is three words that fit two
/// lines of about eighteen characters: `xxxxxxxxxxxxx`, then `zebracross zzmango`.
/// That width is an estimate from `Adwaita Sans 11` resolving to DejaVu Sans in a
/// 250 px sidebar, NOT a measurement; the retained band crop is where to check it.
pub(crate) const PROJSEARCH_OCR_RESULT: &str = "zzmango";

/// The whole content of [`PROJSEARCH_FILE_START`]; lowercased it contains
/// [`PROJSEARCH_OCR_ABSENT`].
pub(crate) const PROJSEARCH_HEADING_START: &str = "# PROJSEARCHDELTA";

/// Lowercased substring OCR must NOT find: the heading of the file the editor was
/// showing when the chord fired.
///
/// Its contribution is naming WHICH file is on screen when something went wrong,
/// not detecting that the jump failed — with a single pane two headings cannot both
/// be drawn, so every revert that reddens this one reddens
/// [`PROJSEARCH_OCR_PRESENT`]'s assertion first. Recorded as such in the revert
/// table rather than dressed up as an independent discriminator.
pub(crate) const PROJSEARCH_OCR_ABSENT: &str = "projsearchdelta";

/// Typed into the search entry, then run by the FIRST Enter.
///
/// Four constraints, all of them checked by
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` rather than
/// asserted here in prose: it appears in exactly one line of exactly one file in the
/// cwd (so the result list holds one row and the second Enter has a deterministic
/// target); it is not a substring of any fixture or filler FILENAME (so rg cannot
/// match a name); no `ASSERTED_OCR_TERMS` entry is a substring of it (it is drawn
/// in the capture as content of the opened file, so a term hiding inside it would
/// satisfy or break an OCR assertion on its own); and it is unlike both `psr*` and
/// `projsearch*`, so it cannot be confused with either vocabulary.
pub(crate) const PROJSEARCH_QUERY: &str = "zebracross";

/// The 1-based line of [`PROJSEARCH_FILE_TARGET`] carrying the sole match.
///
/// It must exceed 1, and that is the whole reason it is not 1: `:edit` alone leaves
/// the cursor on line 1, so a `line('.')` readback of 1 cannot tell `open_at` apart
/// from a plain file open. The guard asserts the bound instead of trusting this
/// comment.
pub(crate) const PROJSEARCH_MATCH_LINE: usize = 9;

/// The ASCII run before the query on the match line.
///
/// It must be non-empty for the same reason [`PROJSEARCH_MATCH_LINE`] must exceed
/// 1: a plain file open also leaves the cursor in column 1, so a match at column 0
/// would make the `col('.')` readback undiscriminating. Made of `x`s and a single
/// space, so the line stays clear of every asserted term.
pub(crate) const PROJSEARCH_MATCH_PREFIX: &str = "xxxxxxxxxxxxx ";

/// The value the smoke's `col('.')` readback must show — **derived, never typed.**
///
/// rg's `column` and `nvim_win_set_cursor`'s column are both 0-BASED byte offsets,
/// and `wolf_core::SearchMatch::column` carries rg's verbatim. Vim's `col('.')` — the
/// only thing a readback can observe — is 1-BASED. So the expected readback is the
/// match's byte offset plus one, and that offset is exactly the length of the run
/// before it.
///
/// Writing the number as a literal is the one thing that must not happen here: a
/// reader who does not know the two conventions differ reads `15` as an off-by-one
/// bug, and a reader who writes `14` ships one. The other half is pinned headlessly
/// by `sidebar_search.rs`'s `row_activation_toggles_files_and_opens_lines_unadjusted`,
/// which asserts the seam does NOT apply the +1.
const PROJSEARCH_EXPECTED_COL: usize = PROJSEARCH_MATCH_PREFIX.len() + 1;

/// How many body lines [`search_target_fixture_body`] writes below
/// [`PROJSEARCH_HEADING_TARGET`].
///
/// Comfortably more than the measured `winheight(0)` of 14 (see
/// `DIVIDER_BODY_LINE_COUNT` for where that number came from and what it costs to
/// be under it), so no `~` end-of-buffer column is drawn in the pane the OCR
/// assertion reads. It must also leave [`PROJSEARCH_MATCH_LINE`] inside the file,
/// which the guard checks.
pub(crate) const PROJSEARCH_BODY_LINE_COUNT: usize = 19;

/// How many `x`s each body line carries after its three-digit ordinal, making every
/// filler line twelve characters — far inside a full-width pane, so
/// `runtime/nvim/init.lua`'s `wrap` / `linebreak` never soft-wrap one and each
/// buffer line occupies exactly one screen row.
const PROJSEARCH_BODY_LINE_PAD: usize = 9;

/// Name, in the smoke's own temp cwd, of the file the captive Neovim writes its
/// cursor location into: one line of `<filename tail> <line> <column>`.
///
/// The sidebar lists it, which is why it is registered in
/// `RELAUNCH_FIXTURE_NAMES`. It is written BEFORE the cursor is parked, so the
/// three values describe the instant the jump landed rather than the instant after
/// a normal-mode `o` moved the cursor a line further down.
pub(crate) const PROJSEARCH_READBACK_NAME: &str = "psr_cursor.txt";

/// Name of the file the reveal smoke's captive Neovim writes `expand('%:t')` into
/// after Escape, under `XDG_RUNTIME_DIR` rather than in the cwd.
///
/// Outside the cwd so no tree row bears it: capture B's sidebar band is read for a
/// tree row, and a new one would only add text to it. Registered in
/// `RELAUNCH_FIXTURE_NAMES` anyway, for [`CD_READBACK_NAME`]'s reason: its absolute
/// path is echoed in the typed `:call writefile(…)` command line, which the cmdline
/// strip draws, though in the pane band rather than the sidebar's.
pub(crate) const PROJSEARCH_ESCAPE_READBACK_NAME: &str = "wolf_gnome_psr_escape.txt";

/// Builds the contents of [`PROJSEARCH_FILE_TARGET`]: [`PROJSEARCH_HEADING_TARGET`],
/// then [`PROJSEARCH_BODY_LINE_COUNT`] lines of a three-digit ordinal followed by
/// [`PROJSEARCH_BODY_LINE_PAD`] `x`s — except the one at [`PROJSEARCH_MATCH_LINE`],
/// which is [`PROJSEARCH_MATCH_PREFIX`], [`PROJSEARCH_QUERY`], a space and
/// [`PROJSEARCH_OCR_RESULT`].
///
/// The shape `restart_fixture_body` and `divider_right_fixture_body` already
/// use, with one line replaced. The trailing newline is the caller's, so the file
/// ends on a body line rather than on a blank one.
///
/// ## Why the body may only be digits and `x`
///
/// A fixture whose CONTENT carried an asserted term would satisfy an OCR assertion
/// on its own — the hazard `PICKER_FILE_NAME` documents, aimed at content rather
/// than at a file name. The match line is the deliberate exception and is checked
/// separately: it carries the query and the result word, and
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` proves both are
/// clear of every other asserted term rather than leaving it here as prose.
pub(crate) fn search_target_fixture_body() -> String {
    let mut body = String::from(PROJSEARCH_HEADING_TARGET);
    for line in 1..=PROJSEARCH_BODY_LINE_COUNT {
        // `line` is the body ordinal; the buffer line is one greater, because the
        // heading occupies line 1.
        if line + 1 == PROJSEARCH_MATCH_LINE {
            body.push_str(&format!(
                "\n{PROJSEARCH_MATCH_PREFIX}{PROJSEARCH_QUERY} {PROJSEARCH_OCR_RESULT}"
            ));
        } else {
            body.push_str(&format!(
                "\n{line:03}{}",
                "x".repeat(PROJSEARCH_BODY_LINE_PAD)
            ));
        }
    }
    body
}

/// The TREE-SIDEBAR scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as `SCENARIO`): launch the app, wait until it is ready for input
/// (`wait_ready`), then wait a little longer for the long-lived FileSearch to stream
/// results into the tree, and screenshot. No keyboard interaction at all — we only
/// assert the sidebar lists the file by name, which depends purely on tree population
/// from the app's cwd (set via `Command::current_dir` by the caller).
const TREE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# Extra settle for the streamed FileSearch traversal to populate the tree.
TREE_SETTLE=1.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome tree smoke"

# Extra time for the tree's FileSearch to stream the cwd listing into the sidebar.
sleep "$TREE_SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The first PROJECT-SEARCH scenario, run as a single `sh -c` string INSIDE `cage`:
/// the sidebar panel streams the results, and the second Enter opens the match.
///
/// `BUFPICK_SCENARIO`'s preamble, `wait_file` helper, chord line, readback idiom
/// and cursor parking, copied verbatim with the chord letter and the fixtures
/// changed. The middle is what differs: **there are TWO Enters, they do different
/// things, and a capture sits between them.**
///
/// Standalone — it does NOT need `with_nvim_pid_helper`, because nothing here
/// identifies or signals the Neovim child.
///
/// ## The two Enters are the design, not a retry
///
/// No search runs while the user types: the panel's entry has no
/// `connect_search_changed`, so no keystroke can spawn ripgrep and there is no
/// debounce timer to tune. The FIRST Enter starts the run, and the SECOND — with the
/// entry text unchanged, which is what `search_panel::enter_action` compares —
/// activates the selected row, which a new run puts on the first line row.
///
/// Each Enter gets its own settle, and they buy different things: the first covers
/// spawning rg, its exit, the wake and the rebuild that puts the rows in the panel;
/// the second covers `open_at`'s `:edit` plus `nvim_win_set_cursor` and the pane
/// redraw that follows.
///
/// ## Why capture A is taken between the Enters
///
/// It is the one moment at which the result word, [`PROJSEARCH_OCR_RESULT`], can be
/// drawn by nothing but the search's results: the target file is not open yet, and
/// the word is never typed. After the second Enter the editor draws it too.
///
/// ## Why the readback comes BEFORE the cursor is parked
///
/// The parking keystrokes are a normal-mode `o` (open a line BELOW) and Escape, so
/// they move the cursor one line down from where the jump left it. Reading
/// `line('.')` afterwards would report that moved position and the smoke would be
/// asserting on the parking rather than on the jump. The three values are captured
/// in ONE `writefile` for the same reason: three separate commands would describe
/// three different instants.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `14` — the cursor readback the captive Neovim was told to write never
///   appeared, so the location assertions cannot be checked at all. Capture A is
///   already on disk by then, so the smoke still reads it first. The most likely
///   cause is focus left in the panel's entry, which means the app is still
///   running, so this path runs the `exit 0` path's teardown before it exits.
///   Measured without that teardown on this scenario's overlay-era predecessor, the
///   surviving app and its `cage` outlived the scenario and the test never returned
///   — 673.71 s before it was killed by hand. A hang is not a failure, and a hang
///   can wedge the gate.
const PROJSEARCH_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for the readback file.
FILE_TRIES=100

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome project-search panel smoke"

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
    echo "wolf-gnome project-search panel smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 2: open the STARTING file -----------------------------------------
#
# The pane must begin on a file the search will move it AWAY from, or "the target
# is on screen" would be true before the chord ever fired. This file is never a
# match: it does not contain the query.
wtype -- ":e $PROJSEARCH_FILE_START"
wtype -k Return
sleep "$SETTLE"

# --- Step 3: fire the project-search accelerator Ctrl+Shift+F ---------------
#
# The capture-phase chord controller resolves it through `actions::action_for_key`,
# activates `win.search-in-project`, and the shell performs
# UiAction::ShowSearchPanel: the sidebar switches to its Search page and the
# panel's entry takes focus. It starts NO search.
#
# Note: `-M <mod>` / `-m <mod>` press / release a modifier and `-k <key>`
# presses+releases a key while the held modifiers apply, exactly as in the picker,
# new-buffer, palette and buffer-picker scenarios. Chord delivery is isolated to
# this single line.
wtype -M ctrl -M shift -k f -m shift -m ctrl

# Let the stack switch pages and the entry take keyboard focus.
sleep "$SETTLE"

# --- Step 4: type the query. NO SEARCH RUNS YET -----------------------------
#
# The entry has no `connect_search_changed`, so typing cannot spawn rg however fast
# or slow it arrives.
wtype -- "$PROJSEARCH_QUERY"

# Let the last character land in the entry before Enter reads its text.
sleep "$SETTLE"

# --- Step 5: the FIRST Enter runs the search --------------------------------
#
# `enter_action` sees a query no search was started with, so it answers RunSearch.
wtype -k Return

# Let rg spawn, run over the temp cwd, exit, fire the wake, and let the panel's
# refresh rebuild its rows.
sleep "$SETTLE"

# --- Step 6: capture A, with the results in the panel -----------------------
grim "$OUT_PNG_A"

# --- Step 7: the SECOND Enter activates the selected row --------------------
#
# The entry is unchanged, so `enter_action` answers Activate. `row_activation`
# resolves the line row to Open, `activation_route` answers OpenAt over an editor
# pane, and `WolfCore::open_at(path, line, col)` performs `:edit` then
# nvim_win_set_cursor in ONE pump step. Focus returns to the editor.
wtype -k Return

# Let nvim open the file, place the cursor, and let both the pane and the cmdline
# strip redraw.
sleep "$SETTLE"

# --- Step 8: the cursor readback, BEFORE the cursor is parked ---------------
#
# One printf, so the filename, the line and the column describe ONE instant. Step 9
# moves the cursor, so this cannot be deferred until after it.
wtype -- ":call writefile([printf('%s %d %d', expand('%:t'), line('.'), col('.'))], '$PROJSEARCH_READBACK')"
wtype -k Return
if ! wait_file "$PROJSEARCH_READBACK"; then
    # The exit 0 path's teardown, run first: with focus left in the entry the app is
    # still alive, and an unreaped app holds its cage forever.
    set +e
    kill "$app_pid" 2>/dev/null
    wait "$app_pid" 2>/dev/null
    exit 14
fi

# --- Step 9: park the cursor off every line carrying fixture text -----------
#
# The jump leaves it on the MATCH line, whose inverted block would sit atop the
# query text; the asserted heading is on line 1 and is clear either way. In normal
# mode `o` opens a new line BELOW and enters insert on it; Escape parks the cursor on
# that empty line.
wtype -- o
wtype -k Escape

# Let the redraw land before capturing.
sleep "$SETTLE"

# --- Step 10: capture B, with the target open -------------------------------
grim "$OUT_PNG_B"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The second PROJECT-SEARCH scenario, run as a single `sh -c` string INSIDE `cage`:
/// Ctrl+Shift+F reveals a HIDDEN sidebar on its Search page, Escape hands the keys
/// back to Neovim and leaves the panel up, and `:WolfSidebarShowProject` switches
/// the sidebar back to its Files page.
///
/// [`PROJSEARCH_SCENARIO`]'s preamble, `wait_file` helper and chord lines. It runs
/// one Enter only, because what it reads is where the panel is, not what it opens.
///
/// ## Why the readback is the Escape assertion
///
/// After Escape the scenario types an Ex command. If focus went back to the editor,
/// the captive Neovim runs it and writes the file; if focus stayed in the panel's
/// entry, the command is typed into the entry and nothing is written. The file
/// holds `expand('%:t')`, which also says that Escape opened nothing.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `15` — the readback typed after Escape never appeared: the keys never reached
///   Neovim. Capture A is on disk; A2 and B are not taken. This path runs the
///   `exit 0` path's teardown first, for [`PROJSEARCH_SCENARIO`]'s exit-14 reason.
const PROJSEARCH_REVEAL_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# The tree catches up on its first live refresh after the switch back; TREE_SCENARIO's budget.
TREE_SETTLE=1.5
# How many WAIT_INTERVAL polls to allow for the readback file.
FILE_TRIES=100

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome project-search reveal smoke"

# Poll for a readback file the captive Neovim writes. (The CRASH_SCENARIO body, verbatim.)
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
    echo "wolf-gnome project-search reveal smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 2: open the STARTING file -----------------------------------------
wtype -- ":e $PROJSEARCH_FILE_START"
wtype -k Return
sleep "$SETTLE"

# --- Step 3: HIDE the sidebar with Ctrl+Shift+B -----------------------------
#
# `win.toggle-sidebar`. The chord below must bring it back by itself.
wtype -M ctrl -M shift -k b -m shift -m ctrl
sleep "$SETTLE"

# --- Step 4: Ctrl+Shift+F, the query, and the one Enter ---------------------
wtype -M ctrl -M shift -k f -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$PROJSEARCH_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$SETTLE"

# --- Step 5: capture A, the revealed sidebar on its Search page -------------
grim "$OUT_PNG_A"

# --- Step 6: Escape, then prove the keys reach Neovim -----------------------
wtype -k Escape
sleep "$SETTLE"
wtype -- ":call writefile([expand('%:t')], '$PROJSEARCH_ESCAPE_READBACK')"
wtype -k Return
if ! wait_file "$PROJSEARCH_ESCAPE_READBACK"; then
    # Focus is still in the entry, so the app is alive: tear it down before failing.
    set +e
    kill "$app_pid" 2>/dev/null
    wait "$app_pid" 2>/dev/null
    exit 15
fi
sleep "$SETTLE"

# --- Step 7: capture A2, the panel still up after Escape --------------------
grim "$OUT_PNG_A2"

# --- Step 8: switch the sidebar back to its Files page ----------------------
#
# The Ex command reaches the shell through the command drain as
# UiAction::ShowSidebarProject.
wtype -- ":WolfSidebarShowProject"
wtype -k Return
sleep "$TREE_SETTLE"

# --- Step 9: capture B, the file tree back in the sidebar -------------------
grim "$OUT_PNG_B"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

// S3 fixtures: `HOME` is the parent and the cwd the child, so the `~` abbreviation is asserted.
// `HOME` is set explicitly, so a change to `cage_command`'s default cannot retarget this smoke.

/// Basename component of the smoke-owned HOME directory. It is deliberately the
/// term [`TITLE_OCR_ABSENT`] asserts must NOT be readable: the title must render
/// `~/<project>`, so this segment appears on screen only if the abbreviation is
/// broken.
pub(crate) const TITLE_HOME_DIR: &str = "titlehome-parent";

/// Basename of the PROJECT directory, which is what the header-bar title must
/// render. It is the term [`TITLE_OCR_PRESENT`] asserts is present.
pub(crate) const TITLE_PROJECT_DIR: &str = "titlezulu";

/// Lowercased substring OCR MUST find: the working directory's basename, drawn by
/// the `adw::HeaderBar` title. No fixture is named this (enforced by
/// `ASSERTED_OCR_TERMS` plus the fixture guards), so it can only come from
/// the title.
pub(crate) const TITLE_OCR_PRESENT: &str = "titlezulu";

/// Lowercased substring OCR must NOT find: the HOME segment of the path. Its
/// absence is what makes this smoke test `title::display_title`'s ABBREVIATION
/// rather than merely "a title was set" — without the abbreviation the title would
/// read `/tmp/…/titlehome-parent/titlezulu` and this assertion fires.
pub(crate) const TITLE_OCR_ABSENT: &str = "titlehome";

/// The WINDOW-TITLE scenario, run as a single `sh -c` string inside `cage` (the
/// [`TREE_SCENARIO`] shape): launch the app, wait until it is ready for input
/// (`wait_ready`), settle long enough for the tree's `FileSearch` to populate the
/// sidebar AND for nvim to report its cwd on the render wake that sets the title, then
/// screenshot. No keyboard interaction at all.
const TITLE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# Extra settle for the streamed FileSearch traversal to populate the sidebar and
# for nvim's first cwd report to reach `AppShell::refresh_title` on a render wake.
TREE_SETTLE=1.5

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome title smoke"

# Extra time for the tree's FileSearch to stream the cwd listing into the sidebar.
sleep "$TREE_SETTLE"

grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

// Open Recent fixtures: the target is the cwd's sibling, so the title is the constant `~/orzulu`.
// See notes/crates/wolf-gnome/tests/smoke_headless/sidebar.rs.md#open-recent-fixtures

/// Open Recent smoke: the file opened in the LAUNCH cwd before the chord fires.
/// Nothing asserts on it — it exists so the editor pane is not blank and so the
/// scenario's early steps are the palette smoke's, step for step.
pub(crate) const OPENRECENT_START_FILE: &str = "orbase.md";

/// Heading written into [`OPENRECENT_START_FILE`]. Not an `ASSERTED_OCR_TERMS`
/// entry: `:tcd` closes no buffer, so this heading is still on screen after a
/// PERFECTLY CORRECT re-root and asserting its absence would assert something the
/// feature does not do.
const OPENRECENT_START_HEADING: &str = "# ZZORBASE";

/// Basename of the directory seeded into `folder-mru.sqlite`, and — as
/// [`OPENRECENT_QUERY`] — the string typed into the palette. It is a sibling of the
/// launch cwd and a child of the app's `HOME`, so after the chdir the header bar
/// renders the constant `~/orzulu`.
///
/// Because it IS drawn, it must not contain [`OPENRECENT_OCR_PRESENT`]: the title
/// would then satisfy assertion 5 with the sidebar never re-rooting. That crossed
/// pair is asserted in `restore_fixture_names_cannot_satisfy_their_own_assertions`.
pub(crate) const OPENRECENT_TARGET_DIR: &str = "orzulu";

/// The file dropped inside [`OPENRECENT_TARGET_DIR`]. Its **name** is the asserted
/// term, because the sidebar row IS the observable here — the same deliberate
/// exception [`TREE_FILE_NAME`] carries, and the reason this const is absent from
/// `RELAUNCH_FIXTURE_NAMES`. It is never opened, so its body is only
/// [`SIDEBAR_FILLER_BODY`].
pub(crate) const OPENRECENT_TARGET_FILE: &str = "ortango.md";

/// Lowercased substring OCR MUST find: the target directory's own file, drawn as a
/// tree-sidebar row. It lives outside the launch cwd, so it can only be on the page
/// once `settle_reroot` re-rooted the sidebar at the new working directory.
pub(crate) const OPENRECENT_OCR_PRESENT: &str = "ortango";

/// Typed into the palette's search entry. Defined AS [`OPENRECENT_TARGET_DIR`] so
/// the query and the folder it must select cannot drift apart; that the pair yields
/// exactly one palette row is pinned in the fast suite by
/// `palette::tests::palette_rows_query_orzulu_selects_exactly_the_recent_folder_row`.
const OPENRECENT_QUERY: &str = OPENRECENT_TARGET_DIR;

/// Name of the file the captive Neovim writes its `getcwd()` into, in the LAUNCH
/// cwd. Pixel-independent: OCR is structurally incapable of answering "is Neovim's
/// working directory now exactly this path".
pub(crate) const OPENRECENT_READBACK_NAME: &str = "orcwd.txt";

/// The physical SQLite file `wolf_core::RecentDirs` creates under a state directory.
///
/// Private in `wolf-core` (`recent_dirs.rs`'s `RECENT_DIRS_DB_NAME`), so it is
/// duplicated here exactly as the palette smoke duplicates `command-mru.sqlite`. It
/// is used only in failure messages and in assertion 0b's fingerprint of the
/// DEVELOPER'S real store — this is the first smoke that touches that file at all.
const RECENT_DIRS_DB_NAME: &str = "folder-mru.sqlite";

/// The OPEN RECENT scenario, run as a single `sh -c` string inside `cage`.
///
/// `PALETTE_SCENARIO`'s structure with `BUFPICK_SCENARIO`'s `wait_file` helper:
/// launch + poll, settle, `:e` the base file, fire Ctrl+Shift+P, type the folder
/// query, Enter, settle TWICE, read `getcwd()` back, park the block cursor, settle,
/// screenshot.
///
/// ## Why TWO settles after the Enter
///
/// One activation has to travel four hops before the assertion's observable exists:
/// Neovim applies the `:tcd` and emits `DirChanged`; core tracks it into `live_cwd`;
/// the shell's next render wake sees `workspace::reroot_ready` and settles the
/// pending re-root; and the sidebar's FRESH `FileSearch` then streams the new root's
/// entries into the list. Only the last of those puts [`OPENRECENT_OCR_PRESENT`] on
/// the page — and it is a full directory traversal, the same one [`TITLE_SCENARIO`]
/// buys a `TREE_SETTLE` for on top of its `SETTLE`. Nothing here has been measured to
/// fit inside one `SETTLE`, and the cost of guessing wrong is an INTERMITTENT smoke
/// rather than a red one, so the budget is doubled deliberately. If assertion 5 ever
/// fails alone while assertion 3 is green, raise these — never weaken the assertion.
///
/// ## The block-cursor park is convention here, not load-bearing
///
/// The asserted term is a SIDEBAR row, which the editor's inverted cursor block can
/// never sit on top of. The `o` + Escape is kept anyway because every scenario in
/// this binary does it and a reader should not have to work out which ones may skip it
/// — but do not read it as protecting this assertion.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `14` — the `getcwd()` readback never appeared, which means the palette never
///   closed and the Ex command was typed into its `SearchEntry` instead. That is a
///   DISTINCT cause from "the wrong row ran", which exits 0 and fails assertion 3.
const OPENRECENT_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for the readback file.
FILE_TRIES=100

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome open-recent smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there. (The BUFPICK_SCENARIO body, verbatim.)
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
    echo "wolf-gnome open-recent smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# Open the base file so the pane is not blank. Nothing asserts on it: `:tcd` closes
# no buffer, so it is still painted after a perfectly correct re-root.
wtype -- ":e $OPENRECENT_START_FILE"
wtype -k Return

# Let nvim load the base file and redraw before firing the accelerator.
sleep "$SETTLE"

# Fire the command-palette accelerator Ctrl+Shift+P. The capture-phase chord
# controller resolves it through `actions::action_for_key`, activates
# `win.command-palette`, and the shell performs UiAction::OpenCommandPalette --
# which snapshots the recent-directories store through `open_recent_candidates`.
#
# Note: `-M <mod>` / `-m <mod>` press / release a modifier and `-k <key>`
# presses+releases a key while the held modifiers apply, exactly as in the picker,
# new-buffer, palette and buffer-picker scenarios. Chord delivery is isolated to
# this single line.
wtype -M ctrl -M shift -k p -m shift -m ctrl

# Let the palette map and grab keyboard focus on its search entry.
sleep "$SETTLE"

# Type the target directory's basename so the ranked list narrows to its folder row.
wtype -- "$OPENRECENT_QUERY"

# Let the SearchEntry's own search delay fire and the list re-rank. That delay lives
# inside this budget, exactly as it does in PICKER_SCENARIO.
sleep "$SETTLE"

# Activate the sole row: records its MRU key, closes the palette, and hands
# PaletteAction::OpenRecentFolder to the shell, which performs it through chdir_to.
wtype -k Return

# TWO settles, and both are load-bearing -- see the doc comment above. The first
# covers nvim applying the :tcd and core tracking the DirChanged into live_cwd; the
# second covers the shell settling the pending re-root on a render wake and the
# sidebar's FRESH FileSearch streaming the new root's entries into the list. Only
# that last step puts the asserted sidebar row on the page.
sleep "$SETTLE"
sleep "$SETTLE"

# --- The readback: Neovim's OWN answer about its working directory -----------
#
# Pixel-independent. OCR cannot answer "is the working directory exactly this
# path", and the header-bar title only shows what refresh_title read.
wtype -- ":call writefile([getcwd()], '$OPENRECENT_READBACK')"
wtype -k Return
wait_file "$OPENRECENT_READBACK" || exit 14

# Park the block cursor off the buffer's first row, as every scenario here does. The
# asserted term is a SIDEBAR row, so this is convention rather than protection -- see
# the doc comment. These keystrokes also corroborate that the palette really closed.
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

// Sidebar-toggle smoke: the observable is a file name that vanishes, guarded three ways.
// See notes/crates/wolf-gnome/tests/smoke_headless/sidebar.rs.md#sidebar-toggle-smoke

/// Sidebar-toggle smoke: the file that is NEVER opened. Its NAME is the sidebar
/// observable, so it deliberately CONTAINS [`SIDEBARTOGGLE_OCR_ABSENT`] — the
/// documented exemption this smoke shares with
/// [`tree_sidebar_shows_files_under_cage`].
pub(crate) const SIDEBARTOGGLE_FILE: &str = "sbtangoleaf.md";

/// Lowercased substring OCR must find in capture A and must NOT find in capture B:
/// the sidebar row for [`SIDEBARTOGGLE_FILE`].
pub(crate) const SIDEBARTOGGLE_OCR_ABSENT: &str = "sbtangoleaf";

/// Sidebar-toggle smoke: the file the pane OPENS, so the editor keeps drawing text
/// after the sidebar is hidden.
///
/// Deliberately unlike [`SIDEBARTOGGLE_OCR_KEEP`], the ordinary convention: the
/// keep-term must come from a rendered BUFFER, or a hidden sidebar plus a blank
/// editor would still satisfy it.
pub(crate) const SIDEBARTOGGLE_OPEN_FILE: &str = "sbwhiskey.md";

/// Heading written into [`SIDEBARTOGGLE_OPEN_FILE`]; lowercased it contains
/// [`SIDEBARTOGGLE_OCR_KEEP`].
pub(crate) const SIDEBARTOGGLE_HEADING: &str = "# SIDEBARKEEPALFA";

/// Lowercased substring OCR must find in BOTH captures: the heading of the file the
/// editor holds throughout.
///
/// In capture B it is the ANTI-VACUITY assertion — the window is still drawing, so
/// a crash or a blank frame cannot satisfy the must-NOT-contain assertion that
/// follows it.
pub(crate) const SIDEBARTOGGLE_OCR_KEEP: &str = "sidebarkeepalfa";

/// The SIDEBAR-TOGGLE scenario, run as a single `sh -c` string INSIDE `cage`: launch
/// the app, wait until it is ready for input (`wait_ready`), settle long enough for the
/// streamed `FileSearch` to populate the tree (the [`TREE_SCENARIO`] budget), open the
/// editor fixture, park the block cursor, capture A, fire Ctrl+Shift+B, settle, capture
/// B.
///
/// Standalone — it does NOT need `with_nvim_pid_helper`, and it writes no
/// readback file: both of its observables are pixels, and the three-way guard
/// described above the `SIDEBARTOGGLE_*` consts is what keeps the `!contains` half
/// honest.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
const SIDEBARTOGGLE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# Extra settle for the streamed FileSearch traversal to populate the tree. Capture A
# asserts a sidebar ROW is on screen, so this budget is load-bearing here in a way
# it is not in the keyboard smokes: without it capture A is empty of the term and
# the smoke fails at its own precondition. Matches TREE_SCENARIO.
TREE_SETTLE=1.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome sidebar-toggle smoke"

# Extra time for the tree's FileSearch to stream the cwd listing into the sidebar.
sleep "$TREE_SETTLE"

# --- Step 2: open the editor fixture ----------------------------------------
#
# The editor must keep drawing text after the sidebar goes away, or capture B's
# anti-vacuity assertion has nothing to find and a blank window would satisfy the
# must-NOT-contain assertion that follows it.
wtype -- ":e $SIDEBARTOGGLE_OPEN_FILE"
wtype -k Return
sleep "$SETTLE"

# Park the block cursor off the asserted heading's row (the SCENARIO precedent):
# the inverted cursor block corrupts the cell it covers, and the heading is row 0.
wtype -- o
wtype -k Escape
sleep "$SETTLE"

# --- Step 3: capture A, with the sidebar still SHOWN ------------------------
#
# Both of its assertions are PRECONDITIONS: that the tree was populated at all, and
# that the editor is drawing the keep-term. Without them the whole smoke could pass
# against a window that never rendered a sidebar in the first place.
grim "$OUT_PNG_BEFORE"

# --- Step 4: the chord under test -------------------------------------------
#
# Ctrl+Shift+B is `win.toggle-sidebar`. If the action were not installed the chord
# falls through to Neovim as <C-b> (page backward), which on this one-line buffer
# does nothing visible -- so a broken build leaves the sidebar exactly where it was,
# which is precisely what capture B's must-NOT-contain assertion detects.
wtype -M ctrl -M shift -k b -m shift -m ctrl

# Let the widget hide and the paned re-allocate before capturing.
sleep "$SETTLE"

# --- Step 5: capture B, with the sidebar HIDDEN -----------------------------
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

// Typed-`:cd` fixtures: root node and re-root share one scenario, and the `:cd` is relative.
// See notes/crates/wolf-gnome/tests/smoke_headless/sidebar.rs.md#typed-cd-fixtures

/// Typed-`:cd` smoke: the OUTER directory, one level above the `:cd` target.
///
/// It is drawn ONLY if `title::display_title`'s home abbreviation regresses, which
/// is precisely what assertion 5 asserts it does not. It therefore deliberately
/// CONTAINS [`CD_OCR_UNABBREVIATED`]: an outer directory not named after that term
/// would make assertion 5 hold vacuously.
pub(crate) const CD_OUTER_DIR: &str = "cdouterhome";

/// Typed-`:cd` smoke: the `:cd` target, which is ALSO the app's `HOME` (it is the
/// launch cwd's parent, and [`cage_command`] hands that to the app — see the block
/// comment above).
///
/// Its BASENAME is what the synthetic root row must draw, so it deliberately
/// contains [`CD_OCR_ROOT_NODE`]. Because `cwd == home` after the `:cd`, the header
/// bar renders the bare `~` and this word cannot come from the title.
pub(crate) const CD_ROOT_DIR: &str = "cdrootzulu";

/// Typed-`:cd` smoke: the LAUNCH working directory, a child of [`CD_ROOT_DIR`].
///
/// It is page text twice over — the header bar reads `~/startdir` before the `:cd`,
/// and a sidebar `Dir` row bears it afterwards — so it must stay clear of every
/// asserted term in both directions. The crossed-pair block in
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` asserts that rather
/// than leaving it to this comment.
pub(crate) const CD_START_DIR: &str = "startdir";

/// Typed-`:cd` smoke: the file that lives in the `:cd` TARGET and nowhere else.
///
/// Its **name** is the asserted term, because a sidebar row IS the observable here —
/// the same deliberate exception [`TREE_FILE_NAME`], [`OPENRECENT_TARGET_FILE`] and
/// [`SIDEBARTOGGLE_FILE`] carry, and the reason this const is absent from
/// `RELAUNCH_FIXTURE_NAMES`. It is never opened, so its body is only
/// [`SIDEBAR_FILLER_BODY`].
pub(crate) const CD_TARGET_FILE: &str = "cdtango.md";

/// Lowercased substring OCR MUST find: [`CD_TARGET_FILE`]'s name, drawn as a child
/// row of the synthetic root. It lives OUTSIDE the launch cwd, so it can reach the
/// page only once the window re-rooted on Neovim's live working directory AND the
/// synthetic root auto-expanded.
pub(crate) const CD_OCR_ROOTED: &str = "cdtango";

/// Lowercased substring OCR MUST find: [`CD_ROOT_DIR`]'s basename, drawn by the
/// SYNTHETIC ROOT ROW and by nothing else on the page.
///
/// After the `:cd` the working directory is the app's `HOME`, so the header bar
/// reads `~`; the launch cwd's own name is [`CD_START_DIR`]; and no fixture is named
/// this. The root row is the only remaining source.
pub(crate) const CD_OCR_ROOT_NODE: &str = "cdrootzulu";

/// Lowercased substring OCR must NOT find: [`CD_OUTER_DIR`]'s basename.
///
/// This is a PRECONDITION GUARD for [`CD_OCR_ROOT_NODE`], not a feature assertion. A
/// title that failed to abbreviate would draw the whole path — which contains BOTH
/// this term and [`CD_OCR_ROOT_NODE`] — and would therefore satisfy the root-node
/// assertion with no root row on screen. Asserting this FIRST is what closes that.
pub(crate) const CD_OCR_UNABBREVIATED: &str = "cdouterhome";

/// Typed-`:cd` smoke: the file opened in the LAUNCH cwd before the `:cd` fires.
///
/// Nothing asserts on it — it exists so the editor pane is not blank and so the
/// scenario has a buffer to park the block cursor in. Deliberately unlike every
/// asserted term, the ordinary convention; it is registered in
/// `RELAUNCH_FIXTURE_NAMES` so that is enforced rather than merely intended.
pub(crate) const CD_BASE_FILE: &str = "cdbase.md";

/// Heading written into [`CD_BASE_FILE`]. Not an `ASSERTED_OCR_TERMS` entry: `:cd`
/// closes no buffer, so this line is still painted after a perfectly correct re-root
/// and asserting its absence would assert something the feature does not do.
const CD_BASE_HEADING: &str = "# ZZCDBASE";

/// Typed-`:cd` smoke: the basename of the smoke-owned `WOLF_STATE_DIR`, created
/// under `base` — OUTSIDE both search roots.
///
/// See fact 2 of the block comment above: the helper's default state dir would sit
/// inside the post-`:cd` search root, and the captive Neovim's swap file names there
/// literally contain [`CD_OCR_ROOT_NODE`].
const CD_STATE_DIR: &str = "cdstate";

/// Name of the file the captive Neovim writes its `getcwd()` into. It lands in
/// `base`, outside both search roots, so it is never a sidebar row.
///
/// Pixel-independent: OCR is structurally incapable of answering "is Neovim's
/// working directory now exactly this path".
pub(crate) const CD_READBACK_NAME: &str = "cdcwd.txt";

/// The TYPED-`:cd` scenario, run as a single `sh -c` string inside `cage` (the
/// [`OPENRECENT_SCENARIO`] shape): launch + poll, settle, `:e` the base file, type
/// **`:cd ..`**, settle three times, read `getcwd()` back, park the block cursor,
/// settle, screenshot.
///
/// ## Why the `:cd` is relative
///
/// An absolute `:cd <path>` would put [`CD_OCR_ROOT_NODE`] into the typed command
/// line — page text — and into any residual toast, satisfying assertion 6 with
/// nothing having re-rooted. `..` names the target without spelling it.
///
/// ## Why THREE settles after the `:cd`
///
/// One keystroke has to travel four hops before the assertions' observables exist:
/// Neovim applies the `:cd` and emits `DirChanged`; core tracks it into `live_cwd`;
/// the shell's next render wake settles the re-root; and the sidebar's FRESH
/// `FileSearch` then streams the new root's entries into the list. Only the last of
/// those puts [`CD_OCR_ROOTED`] on the page, and it is a full directory traversal —
/// hence the [`TREE_SCENARIO`] `TREE_SETTLE` on top of the two `SETTLE`s
/// [`OPENRECENT_SCENARIO`] already found necessary for the same four hops. If
/// assertion 4 ever fails alone while assertion 3 is green, raise these; never weaken
/// an assertion.
///
/// ## The block-cursor park
///
/// Both asserted terms are SIDEBAR text, which the editor's inverted cursor block can
/// never sit on top of, so the `o` + Escape is convention here rather than protection
/// — kept because every scenario in this binary does it and a reader should not have to
/// work out which ones may skip it. It also corroborates that keys still reach Neovim
/// after the `:cd`.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention). A LAUNCH
///   failure; nothing below is about the `:cd`.
/// * `14` — the `getcwd()` readback never appeared, i.e. the Ex command never
///   executed. DISTINCT from "the `:cd` went to the wrong place", which exits 0 and
///   fails assertion 3 instead.
const CD_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# Extra settle for the RE-ROOTED FileSearch to stream the new root's listing into
# the sidebar. Matches TREE_SCENARIO, and is load-bearing here for the same reason
# it is there: the asserted terms are sidebar rows.
TREE_SETTLE=1.5
# How many WAIT_INTERVAL polls to allow for the readback file.
FILE_TRIES=100

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome typed-cd smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there. (The OPENRECENT_SCENARIO body, verbatim.)
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
    echo "wolf-gnome typed-cd smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# Open the base file so the pane is not blank and so there is a buffer to park the
# block cursor in. Nothing asserts on it: `:cd` closes no buffer, so it is still
# painted after a perfectly correct re-root.
wtype -- ":e $CD_BASE_FILE"
wtype -k Return

# Let nvim load the base file and redraw before typing the command under test.
sleep "$SETTLE"

# --- The command under test -------------------------------------------------
#
# RELATIVE on purpose. `..` is the parent of the launch cwd, which is the app's HOME
# -- so after this the header bar reads the bare `~` and the target directory's name
# can only reach the page from a sidebar row. An absolute `:cd <path>` would spell
# that name into the command line itself and satisfy the assertion from the cmdline
# strip. Do not "clarify" this into an absolute path.
wtype -- ":cd .."
wtype -k Return

# THREE settles, all load-bearing -- see the doc comment above. The first two cover
# nvim applying the :cd, core tracking the DirChanged into live_cwd, and the shell
# settling the re-root on a render wake; the third covers the sidebar's FRESH
# FileSearch traversing the new root. Only that last step puts the asserted rows on
# the page.
sleep "$SETTLE"
sleep "$SETTLE"
sleep "$TREE_SETTLE"

# --- The readback: Neovim's OWN answer about its working directory -----------
#
# Pixel-independent. OCR cannot answer "is the working directory exactly this path",
# and the header-bar title only shows what refresh_title read. The path is absolute
# and lives OUTSIDE both search roots, so neither this command line nor the written
# file can put an asserted term on the page.
wtype -- ":call writefile([getcwd()], '$CD_READBACK')"
wtype -k Return
wait_file "$CD_READBACK" || exit 14

# Park the block cursor off the buffer's first row, as every scenario here does. Both
# asserted terms are SIDEBAR rows, so this is convention rather than protection -- see
# the doc comment. These keystrokes also corroborate that keys still reach Neovim.
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

/// Reads the first project-search smoke's cursor readback — one
/// `<filename tail> <line> <column>` line — and returns the three values.
///
/// All three come from ONE `printf` inside the captive Neovim, so they describe a
/// single instant; splitting them across three files (or three commands) would let
/// the filename and the cursor position disagree with no way to tell that they had.
/// Parsing them together keeps that property visible here too: a malformed file
/// produces one good message carrying the raw contents rather than three bespoke
/// ones.
///
/// The filename is `expand('%:t')`, i.e. the tail only, so it never carries a space
/// — the three-token shape is therefore a real check rather than an assumption
/// about the fixture's name.
fn read_cursor_location(path: &Path, smoke: &str) -> (String, i64, i64) {
    const LOCATION_TOKENS: usize = 3;

    let raw = read_readback(path, smoke);
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    assert!(
        tokens.len() == LOCATION_TOKENS,
        "{smoke}: cursor readback {} holds {} whitespace-separated token(s), expected \
         {LOCATION_TOKENS} (filename tail, line, column). Raw contents were {raw:?}. The line is \
         one `printf('%s %d %d', expand('%:t'), line('.'), col('.'))`, so a different shape means \
         the typed Ex command did not execute as written — a stray keystroke landing in the \
         buffer rather than the cmdline is the usual cause. An UNNAMED buffer would also land \
         here: `expand('%:t')` is empty for one, which writes a leading space and leaves two \
         tokens.",
        path.display(),
        tokens.len()
    );

    let line = parse_readback_int(tokens[1], path, &raw, smoke);
    let column = parse_readback_int(tokens[2], path, &raw, smoke);
    (tokens[0].to_string(), line, column)
}

/// The tree sidebar lists a file in the working directory, by name.
///
/// It also transitively covers the SYNTHETIC ROOT's auto-expand. The tree's top level
/// is a single root node whose children are `children_of(entries, "")`, so without the
/// re-expand that follows every root-store rebuild [`TREE_FILE_NAME`] sits under a
/// COLLAPSED root and never reaches a pixel. Its one assertion therefore reddens for
/// either cause: a tree that never populated, or a root that never expanded.
#[test]
fn tree_sidebar_shows_files_under_cage() {
    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let target = cwd.join(TREE_FILE_NAME);
    std::fs::write(&target, format!("{TREE_HEADING}\n"))
        .unwrap_or_else(|e| panic!("failed to write tree smoke file {}: {e}", target.display()));

    // Put the screenshot under XDG_RUNTIME_DIR (guaranteed writable) and start
    // from a clean slate so a stale PNG cannot pass.
    let out_png = runtime_dir.join("wolf_gnome_tree_smoke.png");
    if out_png.exists() {
        std::fs::remove_file(&out_png).unwrap_or_else(|e| {
            panic!(
                "failed to remove stale screenshot {}: {e}",
                out_png.display()
            )
        });
    }

    // Run the whole tree scenario inside cage, with the temp dir as the working
    // directory so the app picks it up as its cwd and the tree populates from it.
    let status = cage_command(TREE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .status()
        .expect("failed to spawn `cage` for the headless tree smoke scenario");
    assert!(
        status.success(),
        "cage tree scenario exited unsuccessfully ({status}); the app may have failed to launch, \
         the window may not have mapped, or grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // The screenshot must exist and be non-empty.
    let png_meta = std::fs::metadata(&out_png).unwrap_or_else(|e| {
        panic!(
            "expected screenshot {} to exist after the cage tree scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        png_meta.len() > 0,
        "screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );

    // Tree population only; see the tree-sidebar comment above the TREE_* consts.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(TREE_OCR),
        "OCR of the rendered window did not contain {TREE_OCR:?} (loose match). This usually means \
         the tree sidebar did not populate from the cwd FileSearch (or the Paned layout hid/clipped \
         the tree pane). PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

/// Writes the project-search smokes' fixtures into `cwd`: the target, the starting
/// file and the sidebar fillers, which are page density (see `SIDEBAR_FILLER_COUNT`)
/// and hold no occurrence of the query, so they never join the match set.
fn write_projsearch_fixtures(cwd: &Path, smoke: &str) {
    let target = cwd.join(PROJSEARCH_FILE_TARGET);
    std::fs::write(&target, format!("{}\n", search_target_fixture_body())).unwrap_or_else(|e| {
        panic!(
            "{smoke}: failed to write the search target {}: {e}",
            target.display()
        )
    });

    let start = cwd.join(PROJSEARCH_FILE_START);
    std::fs::write(&start, format!("{PROJSEARCH_HEADING_START}\n")).unwrap_or_else(|e| {
        panic!(
            "{smoke}: failed to write the starting file {}: {e}",
            start.display()
        )
    });

    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
            panic!("{smoke}: failed to write filler {}: {e}", filler.display())
        });
    }
}

/// The SIDEBAR's band of a one-tab capture, in absolute output pixels: x from the
/// output's left edge to the pane area's ([`OVERLAY_ORIGIN_X_EXPECTED`]), y from the
/// header bar's bottom edge ([`OVERLAY_ORIGIN_Y_EXPECTED`]) to the output's.
///
/// What makes the band discriminate is what it leaves out. The header bar spans the
/// full width above it, and the command-line strip, the statusbar strip and any
/// window-level overlay all belong to the pane area to its right, so the `:e` echo,
/// the statusbar's file name and a floating search box can never reach it. Only the
/// sidebar's own page draws here, plus the `gtk::Paned` handle, which draws no text.
fn projsearch_sidebar_band() -> SampleRect {
    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    SampleRect {
        x0: 0,
        x1: pixel_index(OVERLAY_ORIGIN_X_EXPECTED, "the sidebar band's last column"),
        y0: pixel_index(OVERLAY_ORIGIN_Y_EXPECTED, "the sidebar band's first row"),
        y1: pixel_index(output_h, "the sidebar band's last row"),
    }
}

/// The PANE AREA's band of a one-tab capture: the rest of the output below the header
/// bar, from [`OVERLAY_ORIGIN_X_EXPECTED`] to the right edge.
///
/// It holds the editor, its statusbar and command-line strips, and every child of
/// the window-level `gtk::Overlay`, which is where a floating search box would draw.
fn projsearch_pane_band() -> SampleRect {
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    SampleRect {
        x0: pixel_index(OVERLAY_ORIGIN_X_EXPECTED, "the pane band's first column"),
        x1: pixel_index(output_w, "the pane band's last column"),
        y0: pixel_index(OVERLAY_ORIGIN_Y_EXPECTED, "the pane band's first row"),
        y1: pixel_index(output_h, "the pane band's last row"),
    }
}

/// Ctrl+Shift+F, a typed query and Enter stream the results into the SIDEBAR's search
/// panel, and a second Enter opens the matched LINE of the matched file.
///
/// ## What it covers that nothing else does
///
/// Two claims, read off two captures. Capture A, taken between the Enters, is the
/// only frame in which [`PROJSEARCH_OCR_RESULT`] can come from nothing but a search
/// result, so reading it in the sidebar band and not in the pane band says the
/// results are in the sidebar and that no overlay drew them over the editor.
///
/// The second claim is that a result row is a LOCATION rather than a file. An
/// `:edit` with no cursor placement puts the same file on screen with the same
/// heading, and OCR cannot tell the two apart; the captive Neovim's own `line('.')`
/// and `col('.')` can, which is why they are the load-bearing assertions and capture
/// B is corroboration.
///
/// ## The column, which is the one number a reviewer reads as a bug
///
/// `wolf_core::SearchMatch::column` is rg's 0-BASED byte offset, and
/// `nvim_win_set_cursor` takes a column in the same convention, so `open_at` passes
/// it through untouched. Vim's `col('.')` — the only thing a readback can observe —
/// is 1-BASED. [`PROJSEARCH_EXPECTED_COL`] is therefore derived as
/// `PROJSEARCH_MATCH_PREFIX.len() + 1` and never typed; `sidebar_search.rs` pins the
/// other half headlessly, namely that `row_activation` does NOT apply that +1 itself.
///
/// ## Assertions, classified
///
/// A precondition gets NO falsifier: inventing one would manufacture a discriminator
/// this smoke does not have.
///
/// | # | assertion | kind | falsifier |
/// | --- | --- | --- | --- |
/// | 1 | the scenario reached capture A (exit 0, or 14 after it), and capture A exists | precondition | none |
/// | 2 | the SIDEBAR band of capture A contains [`PROJSEARCH_OCR_RESULT`] | **discriminator** — the results streamed into the panel | route `shell::perform`'s `ShowSearchPanel` arm back to the overlay (the task's falsifier); or skip the row rebuild in the panel's refresh |
/// | 3 | the SIDEBAR band of capture A lacks [`PROJSEARCH_OCR_TREE_ROW`] | corroboration — the Search page replaced the tree | none of its own: no edit in `src/` makes this fail first. The two pages share one `gtk::Stack`, which draws one child, so the panel's rows and the tree are never in the band together; leaving the stack on Files was measured to fail 2 first, since the Search page's refresh gate stays shut. Its anti-vacuity is the presence half of [`search_chord_reveals_the_sidebar_and_escape_returns_to_the_editor_under_cage`]'s assertion 4 |
/// | 4 | the PANE band of capture A lacks [`PROJSEARCH_OCR_RESULT`] and contains [`PROJSEARCH_OCR_ABSENT`] | corroboration — no overlay drew the results; the heading is the band's proof that it was readable | none of its own: the task's overlay route was measured to fail 2 first, and an overlay drawn beside the panel would need both to take the typed query, which one keyboard focus cannot do |
/// | 5 | exit 0, and the readback names [`PROJSEARCH_FILE_TARGET`], line [`PROJSEARCH_MATCH_LINE`] and column [`PROJSEARCH_EXPECTED_COL`] | **discriminator** — the right FILE, LINE and COLUMN | send `InputEvent::Open { path }` in the `OpenAt` arm (the line reads 1); make `row_activation` answer column 0 (the column reads 1) |
/// | 6 | whole-page OCR of capture B contains [`PROJSEARCH_OCR_PRESENT`] and lacks [`PROJSEARCH_OCR_ABSENT`] | corroboration — the pane re-rendered onto the target | none of its own: with one pane two headings cannot both be drawn, so every falsifier reaching it reddens 5 first |
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input.
/// * `14` — the cursor readback never appeared, so assertion 5 cannot run.
#[test]
fn search_panel_streams_results_and_opens_the_match_under_cage() {
    const SMOKE: &str = "search_panel_streams_results_and_opens_the_match_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // `resolve_search_root` takes ripgrep's root from the live cwd, so this is the search root too.
    let cwd = make_temp_cwd();
    write_projsearch_fixtures(&cwd, SMOKE);

    let readback = cwd.join(PROJSEARCH_READBACK_NAME);
    let png_a = runtime_dir.join("wolf_gnome_projsearch_panel_a.png");
    let png_b = runtime_dir.join("wolf_gnome_projsearch_panel_b.png");
    let sidebar_png = runtime_dir.join("wolf_gnome_projsearch_panel_a_sidebar.png");
    let pane_png = runtime_dir.join("wolf_gnome_projsearch_panel_a_pane.png");
    // Cleared first: evidence that could be a previous run's leftover is not evidence.
    for path in [&readback, &png_a, &png_b, &sidebar_png, &pane_png] {
        clear_evidence(path, SMOKE);
    }

    // `WOLF_RG_BIN` comes from the devShell like `WOLF_NVIM_BIN`; `require_smoke_env` checked it.
    let status = cage_command(PROJSEARCH_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG_A", &png_a)
        .env("OUT_PNG_B", &png_b)
        .env("PROJSEARCH_FILE_START", PROJSEARCH_FILE_START)
        .env("PROJSEARCH_QUERY", PROJSEARCH_QUERY)
        .env("PROJSEARCH_READBACK", &readback)
        .status()
        .expect("failed to spawn `cage` for the headless project-search panel scenario");

    // --- Assertion 1 (precondition): the scenario reached capture A ------------
    assert!(
        status.success() || status.code() == Some(14),
        "{SMOKE}: the cage project-search scenario exited {status}, which is neither 0 nor the \
         exit 14 that still leaves capture A to read. Exit 11: the app died before it was ready \
         for input, a LAUNCH failure, and nothing below is about the search. Exit 40: the app \
         never became ready; its `wolf-ready:` stderr lines name the condition. Any other status \
         means the window never mapped, the `:e` did not load, or wtype or grim failed. Capture A \
         expected at {}.",
        png_a.display()
    );
    assert_png_present(SMOKE, &png_a);

    let sidebar_text =
        ocr_band_to_lowercase(&png_a, projsearch_sidebar_band(), &sidebar_png, SMOKE);

    // Assertion 2 (discriminator): the word is never typed and its file is not open yet, so only a
    // result row in the sidebar can draw it.
    assert!(
        sidebar_text.contains(PROJSEARCH_OCR_RESULT),
        "{SMOKE}: the sidebar band of capture A did not contain {PROJSEARCH_OCR_RESULT:?}, the \
         word only a search result can draw before the match is opened. Three readings, told \
         apart by the band's text below. (a) It lists the file tree, {PROJSEARCH_OCR_TREE_ROW:?} \
         among it: the chord did not switch the sidebar to its Search page, and if assertion 4 \
         then finds the word in the pane band, the chord opened a floating overlay instead. (b) \
         It shows the query and a status line but no result row: the search ran and the panel \
         never rebuilt its rows, or rg found nothing; a status of `No matches` with the target \
         a `.txt` points at `search_options_for`'s `all_files`. (c) The word is there but \
         misread, e.g. split across a wrapped line: read the crop, and see \
         PROJSEARCH_OCR_RESULT's doc for the width estimate behind it. Capture kept at {}, band \
         crop at {}.\n\nBand OCR was:\n{sidebar_text}",
        png_a.display(),
        sidebar_png.display()
    );

    // Assertion 3 (discriminator): inside this band only the Files page draws the start file's name.
    assert!(
        !sidebar_text.contains(PROJSEARCH_OCR_TREE_ROW),
        "{SMOKE}: the sidebar band of capture A contained {PROJSEARCH_OCR_TREE_ROW:?}, the \
         starting file's tree row. Assertion 2 passed, so results are drawn in the sidebar, yet \
         the tree is drawn there too: the Search page did not replace the Files page. Check that \
         `ShowSearchPanel` calls `sidebar.show_page(SidebarPage::Search)` and that the sidebar's \
         stack really is a `gtk::Stack`, which draws one child, rather than a box holding both. \
         Band crop kept at {}.\n\nBand OCR was:\n{sidebar_text}",
        sidebar_png.display()
    );

    let pane_text = ocr_band_to_lowercase(&png_a, projsearch_pane_band(), &pane_png, SMOKE);

    // Assertion 4 (discriminator): the pane band must not draw a result, and must still read.
    assert!(
        !pane_text.contains(PROJSEARCH_OCR_RESULT),
        "{SMOKE}: the pane band of capture A contained {PROJSEARCH_OCR_RESULT:?}. The target file \
         is not open before the second Enter, so the editor cannot be what drew it: a search \
         result was drawn over the pane area, which is the floating overlay this task replaced \
         with the sidebar panel. Check `shell::perform`'s `ShowSearchPanel` arm and that nothing \
         adds a search box to the window-level `gtk::Overlay`. Band crop kept at {}.\n\nBand OCR \
         was:\n{pane_text}",
        pane_png.display()
    );
    assert!(
        pane_text.contains(PROJSEARCH_OCR_ABSENT),
        "{SMOKE}: the pane band of capture A did not contain {PROJSEARCH_OCR_ABSENT:?}, the \
         heading of {PROJSEARCH_FILE_START}, which the editor shows from the `:e` until the \
         second Enter. Without it the must-not-contain half above holds over a band that read \
         nothing, so this is a precondition failure: the `:e` did not load, something opaque \
         covers the pane, or the band no longer sits on the pane area (re-derive it from \
         OVERLAY_ORIGIN_*_EXPECTED). Band crop kept at {}.\n\nBand OCR was:\n{pane_text}",
        pane_png.display()
    );

    // Assertion 5 (discriminator): the readback, written by one `printf` before the cursor was parked.
    assert!(
        status.success(),
        "{SMOKE}: the scenario exited {status} after capture A. Exit 14 means the cursor readback \
         typed after the second Enter never appeared: the keys did not reach Neovim, so focus \
         stayed in the panel's entry. Assertions 2 to 4 passed, so the panel showed the results; \
         suspect the second Enter, which `enter_action` must answer with Activate for an \
         unchanged query, and the focus return to the editor after an open. Readback expected at \
         {}.",
        readback.display()
    );
    let (file, line, column) = read_cursor_location(&readback, SMOKE);
    assert_eq!(
        file,
        PROJSEARCH_FILE_TARGET,
        "{SMOKE}: after the second Enter the captive Neovim reports its current file as {file:?}, \
         not the search target {PROJSEARCH_FILE_TARGET:?}. If it is {PROJSEARCH_FILE_START:?}, \
         nothing was opened: `enter_action` answered something other than Activate, \
         `row_activation` answered Toggle because the selection sat on the file row rather than \
         the first line row, or `activation_route` refused, in which case the app's stderr \
         carries \"search result not opened\". Readback kept at {}; capture B at {}.",
        readback.display(),
        png_b.display()
    );
    assert_eq!(
        usize::try_from(line).ok(),
        Some(PROJSEARCH_MATCH_LINE),
        "{SMOKE}: the cursor landed on line {line}, not the matched line {PROJSEARCH_MATCH_LINE}. \
         The right FILE is open, so the editor did not jump to the match inside it. A line of 1 \
         is the signature of a plain file open: check that the `OpenAt` arm calls \
         `WolfCore::open_at(path, line, col)` rather than sending `InputEvent::Open`. \
         `sidebar_search.rs`'s `panel_rows_copy_the_match_location_exactly` and \
         `row_activation_toggles_files_and_opens_lines_unadjusted` pin the seams headlessly; if \
         either is red, read it first. Readback kept at {}.",
        readback.display()
    );
    // The column is derived because rg's column is a 0-based byte offset and `col('.')` is 1-based.
    // See PROJSEARCH_EXPECTED_COL.
    assert_eq!(
        usize::try_from(column).ok(),
        Some(PROJSEARCH_EXPECTED_COL),
        "{SMOKE}: the cursor landed in column {column}, not the matched column \
         {PROJSEARCH_EXPECTED_COL}. A column of 1 is the plain-file-open signature, or a \
         `row_activation` that dropped the column. An off-by-one has a definite direction: \
         `SearchMatch::column` and `nvim_win_set_cursor`'s column are BOTH 0-based byte offsets, \
         so the seam must pass the value through untouched, while Vim's `col('.')`, which wrote \
         this number, is 1-based. A reading of {} would mean the seam applied the +1 as well. \
         Readback kept at {}.",
        PROJSEARCH_EXPECTED_COL + 1,
        readback.display()
    );

    // Assertion 6 (corroboration): the pane really re-rendered onto the target.
    assert_png_present(SMOKE, &png_b);
    let ocr_text = ocr_to_lowercase(&png_b);
    assert!(
        ocr_text.contains(PROJSEARCH_OCR_PRESENT),
        "{SMOKE}: OCR of capture B did not contain {PROJSEARCH_OCR_PRESENT:?}, the heading of \
         {PROJSEARCH_FILE_TARGET}, which lives only in that file's CONTENT. Assertion 5 read the \
         state Neovim reports about ITSELF and passed; this is the pixels. The editor holds the \
         right buffer at the right place but the front-end drew something else over it. Capture \
         kept at {}. OCR output was:\n{ocr_text}",
        png_b.display()
    );
    assert!(
        !ocr_text.contains(PROJSEARCH_OCR_ABSENT),
        "{SMOKE}: OCR of capture B contained {PROJSEARCH_OCR_ABSENT:?}, the heading of \
         {PROJSEARCH_FILE_START}, the file the editor showed before the search. The assertion \
         above passed, so both headings are on screen at once, which one pane cannot do: a \
         second pane or a stale frame is showing the starting file. Capture kept at {}. OCR \
         output was:\n{ocr_text}",
        png_b.display()
    );
}

/// Ctrl+Shift+F reveals a HIDDEN sidebar on its Search page, Escape hands the keys
/// back to Neovim and leaves the panel showing, and `:WolfSidebarShowProject` switches
/// the sidebar back to the file tree.
///
/// ## What it covers that nothing else does
///
/// The three ways the panel meets the rest of the window. The chord reaches the panel
/// through the sidebar preference, so a hidden sidebar must come back with it;
/// Escape is the panel's own key, and the GNOME divergence the panel records; and
/// `Command::SidebarShowProject` is the one route back to the Files page this smoke
/// can drive, since no pointer smoke clicks the page switcher.
///
/// Every capture is read through [`projsearch_sidebar_band`], for the reason its doc
/// gives: in that band only the sidebar's current page can draw
/// [`PROJSEARCH_OCR_RESULT`] or [`PROJSEARCH_OCR_TREE_ROW`].
///
/// ## Assertions, classified
///
/// | # | assertion | kind | falsifier |
/// | --- | --- | --- | --- |
/// | 0 | the scenario reached capture A (exit 0, or 15 after it), and capture A exists | precondition | none |
/// | 1 | the sidebar band of capture A contains [`PROJSEARCH_OCR_RESULT`] | **discriminator** — the chord revealed the hidden sidebar | make `ShowSearchPanel` skip `set_sidebar_preference(true)`; the task's overlay route |
/// | 2 | exit 0, and the readback typed after Escape names [`PROJSEARCH_FILE_START`] | **discriminator** — Escape hands the keys to Neovim and opens nothing | delete the panel's Escape arm |
/// | 3 | the sidebar band of capture A2 contains [`PROJSEARCH_OCR_RESULT`] | **discriminator** — Escape leaves the panel showing | make the panel's Escape arm also switch its ancestor `gtk::Stack` to the `files` page; the panel holds no sidebar handle |
/// | 4 | the sidebar band of capture B contains [`PROJSEARCH_OCR_TREE_ROW`] and lacks [`PROJSEARCH_OCR_RESULT`] | **discriminator** — `:WolfSidebarShowProject` switches back to Files | map `Command::SidebarShowProject` back to `UiAction::Deferred` |
///
/// Assertion 4's presence half is also the anti-vacuity of
/// [`search_panel_streams_results_and_opens_the_match_under_cage`]'s assertion 3: it
/// shows the band reads the start file's tree row when the Files page is up.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input.
/// * `15` — the readback typed after Escape never appeared, which is assertion 2.
#[test]
fn search_chord_reveals_the_sidebar_and_escape_returns_to_the_editor_under_cage() {
    const SMOKE: &str =
        "search_chord_reveals_the_sidebar_and_escape_returns_to_the_editor_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    write_projsearch_fixtures(&cwd, SMOKE);

    // Under the runtime dir, so no tree row bears its name; see PROJSEARCH_ESCAPE_READBACK_NAME.
    let readback = runtime_dir.join(PROJSEARCH_ESCAPE_READBACK_NAME);
    let png_a = runtime_dir.join("wolf_gnome_projsearch_reveal_a.png");
    let png_a2 = runtime_dir.join("wolf_gnome_projsearch_reveal_a2.png");
    let png_b = runtime_dir.join("wolf_gnome_projsearch_reveal_b.png");
    let band_a = runtime_dir.join("wolf_gnome_projsearch_reveal_a_sidebar.png");
    let band_a2 = runtime_dir.join("wolf_gnome_projsearch_reveal_a2_sidebar.png");
    let band_b = runtime_dir.join("wolf_gnome_projsearch_reveal_b_sidebar.png");
    for path in [
        &readback, &png_a, &png_a2, &png_b, &band_a, &band_a2, &band_b,
    ] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(PROJSEARCH_REVEAL_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG_A", &png_a)
        .env("OUT_PNG_A2", &png_a2)
        .env("OUT_PNG_B", &png_b)
        .env("PROJSEARCH_FILE_START", PROJSEARCH_FILE_START)
        .env("PROJSEARCH_QUERY", PROJSEARCH_QUERY)
        .env("PROJSEARCH_ESCAPE_READBACK", &readback)
        .status()
        .expect("failed to spawn `cage` for the headless project-search reveal scenario");

    // --- Precondition: the scenario reached capture A --------------------------
    assert!(
        status.success() || status.code() == Some(15),
        "{SMOKE}: the cage reveal scenario exited {status}, which is neither 0 nor the exit 15 \
         that still leaves capture A to read. Exit 11: the app died before it was ready for \
         input, a LAUNCH failure. Exit 40: the app never became ready; its `wolf-ready:` stderr \
         lines name the condition. Any other status means the window never mapped, the `:e` did \
         not load, or wtype or grim failed. Capture A expected at {}.",
        png_a.display()
    );
    assert_png_present(SMOKE, &png_a);

    // Assertion 1 (discriminator): the sidebar was hidden, so a result in its band means the chord
    // brought it back on the Search page.
    let text_a = ocr_band_to_lowercase(&png_a, projsearch_sidebar_band(), &band_a, SMOKE);
    assert!(
        text_a.contains(PROJSEARCH_OCR_RESULT),
        "{SMOKE}: the sidebar band of capture A did not contain {PROJSEARCH_OCR_RESULT:?}. The \
         sidebar was hidden with Ctrl+Shift+B before the chord. If the band shows the editor's \
         text, {PROJSEARCH_OCR_ABSENT:?} among it, the sidebar stayed hidden and the panel ran \
         its search out of sight: `ShowSearchPanel` must call `set_sidebar_preference(true)` \
         before it switches the page. If it shows the file tree, the page never switched. If it \
         shows the panel with no result row, read the first project-search smoke, which checks \
         the same rows with the sidebar never hidden. Capture kept at {}, band crop at {}.\n\n\
         Band OCR was:\n{text_a}",
        png_a.display(),
        band_a.display()
    );

    // Assertion 2 (discriminator): Escape handed the keys to Neovim, which ran the typed readback.
    assert!(
        status.success(),
        "{SMOKE}: the scenario exited {status} after capture A. Exit 15 means the readback typed \
         after Escape never appeared: the keys still went to the panel's entry, so Escape did not \
         give focus back to the editor. Check the panel's capture-phase Escape arm, which must \
         call the focus target's `grab_focus()` and stop the event. Readback expected at {}.",
        readback.display()
    );
    let after_escape = read_trimmed(&readback, SMOKE);
    assert_eq!(
        after_escape,
        PROJSEARCH_FILE_START,
        "{SMOKE}: after Escape the captive Neovim reports its current file as {after_escape:?}, \
         not the starting file {PROJSEARCH_FILE_START:?}. Escape reached the editor, but the \
         buffer changed on the way: if it is {PROJSEARCH_FILE_TARGET:?}, Escape activated the \
         selected result instead of only returning focus. Readback kept at {}.",
        readback.display()
    );

    // Assertion 3 (discriminator): Escape moved focus only; the panel and its rows stay.
    assert_png_present(SMOKE, &png_a2);
    let text_a2 = ocr_band_to_lowercase(&png_a2, projsearch_sidebar_band(), &band_a2, SMOKE);
    assert!(
        text_a2.contains(PROJSEARCH_OCR_RESULT),
        "{SMOKE}: the sidebar band of capture A2, taken after Escape, did not contain \
         {PROJSEARCH_OCR_RESULT:?}, which capture A showed. Escape must only return focus to the \
         editor, leaving the panel, its query and its rows where they are. If the band shows the \
         file tree, Escape switched the page; if it shows the editor, Escape hid the sidebar; if \
         it shows the panel with no rows, Escape cleared the search. Capture kept at {}, band \
         crop at {}.\n\nBand OCR was:\n{text_a2}",
        png_a2.display(),
        band_a2.display()
    );

    // Assertion 4 (discriminator): the Ex command brought the Files page back.
    assert_png_present(SMOKE, &png_b);
    let text_b = ocr_band_to_lowercase(&png_b, projsearch_sidebar_band(), &band_b, SMOKE);
    assert!(
        text_b.contains(PROJSEARCH_OCR_TREE_ROW),
        "{SMOKE}: the sidebar band of capture B, taken after `:WolfSidebarShowProject`, did not \
         contain {PROJSEARCH_OCR_TREE_ROW:?}, the starting file's tree row. If the band still \
         shows the panel, the command did nothing: `command_action` must map \
         `Command::SidebarShowProject` to `UiAction::ShowSidebarProject` rather than `Deferred`, \
         and its arm must call `sidebar.show_page(SidebarPage::Files)`. If it shows a tree \
         without the row, the tree did not catch up after being off-screen: see \
         `tree_gate::TreeRefreshGate`. Capture kept at {}, band crop at {}.\n\nBand OCR \
         was:\n{text_b}",
        png_b.display(),
        band_b.display()
    );
    assert!(
        !text_b.contains(PROJSEARCH_OCR_RESULT),
        "{SMOKE}: the sidebar band of capture B contained {PROJSEARCH_OCR_RESULT:?} beside the \
         tree row the assertion above found. Both pages are drawn at once, so the sidebar is not \
         a stack showing one child. Band crop kept at {}.\n\nBand OCR was:\n{text_b}",
        band_b.display()
    );
}

/// Proves the window TITLE renders the working directory, home-abbreviated.
///
/// A single-launch smoke. The title is drawn by the `adw::HeaderBar`'s title label
/// in the theme font — dark-on-light, like the empty-state status page whose
/// `folder` term already OCRs reliably at [`SMOKE_FONT_PT`].
///
/// ## Setup
///
/// Two NESTED temp directories, so the `~` abbreviation is exercised rather than
/// merely "a title was set": the cage command runs with its working directory set
/// to `<base>/titlehome-parent/titlezulu` and `HOME` set to
/// `<base>/titlehome-parent`, where `<base>` is whatever [`make_temp_cwd`] returned
/// (today `<unique>/project`, used only as a base — this smoke never launches
/// there). `title::display_title` must then render `~/titlezulu`.
///
/// The `HOME` here is an explicit override of [`cage_command`]'s default. The two
/// happen to agree — the default is the cwd's parent, which IS
/// `<base>/titlehome-parent` — and it is stated anyway, because the assertions below
/// are written against this specific value.
/// The nine [`SIDEBAR_FILLER_FILES`] go into the project so the sidebar carries
/// page density — the title alone is thin, and `tesseract` classifies a word by the
/// whole page (see [`SIDEBAR_FILLER_COUNT`]). They are FIXTURE, not assertion.
///
/// ## Assertions
///
/// 1. OCR **contains** `titlezulu` — the cwd basename. It can only come from the
///    header-bar title: the sidebar lists file names, `:e` toasts name files, and no
///    fixture is named that (enforced by `ASSERTED_OCR_TERMS` plus the three
///    fixture guards).
/// 2. OCR does **not** contain `titlehome` — the home prefix was abbreviated to `~`
///    rather than rendered. This is what makes the smoke test `display_title`'s
///    ABBREVIATION and not merely "some title was set".
///
/// ## How it discriminates
///
/// Without the feature the title is the hardcoded `"Wolf"` from `app.rs`'s window
/// builder, so `titlezulu` is simply absent. Without the abbreviation the title
/// would be the full `/tmp/…/titlehome-parent/titlezulu` and assertion 2 fires.
/// Note the two are independent: a title that renders the full path passes 1 and
/// fails 2, and a title that never updates fails 1 and passes 2.
///
/// ## Pre-authorized contingency (no assertion may be weakened)
///
/// If the theme-font title ever proves too small for `tesseract` at cage's
/// 1280x720, set `XDG_CONFIG_HOME` on the cage command to a smoke-owned directory
/// containing `gtk-4.0/settings.ini` with a large `gtk-font-name`. **Do not lower
/// the assertion, do not switch to a substring, and do not drop assertion 2.** (A
/// manual repro at 28 pt OCRs both terms correctly, so this is expected to stay
/// unused.)
///
/// ## Red-run revert (one line)
///
/// | assertion | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | 1 (`titlezulu` present) | in `shell.rs::refresh_title`, delete the `self.inner.window.set_title(Some(&title));` line | OCR misses `titlezulu`; the header bar still reads `wolf` | **AS PREDICTED.** Assertion 1 panicked, and the OCR dump embedded in its own failure message read, in order: `®`, `wolf`, `-`, then the nine `fill*.md` filler names — the hardcoded `"Wolf"` from `app.rs`'s window builder and no `titlezulu` anywhere. Note the failure message's cause (a) is the one that actually applied. The `XDG_CONFIG_HOME` font contingency was NOT needed: the green run OCRs `titlezulu` (and correctly misses `titlehome`) at `SMOKE_FONT_PT`. |
#[test]
fn window_title_shows_the_working_directory_under_cage() {
    const SMOKE: &str = "window_title_shows_the_working_directory_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // `make_temp_cwd` is only a base here; the app's cwd is `<base>/titlehome-parent/titlezulu` and its
    // HOME `<base>/titlehome-parent`, and the state dir is a sibling FileSearch never lists.
    let base = make_temp_cwd();
    let home = base.join(TITLE_HOME_DIR);
    let project = home.join(TITLE_PROJECT_DIR);
    std::fs::create_dir_all(&project).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the nested project dir {}: {e}",
            project.display()
        )
    });

    // Page density for tesseract. Fixture only — nothing here asserts on them.
    for name in SIDEBAR_FILLER_FILES {
        let filler = project.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
            panic!("{SMOKE}: failed to write filler {}: {e}", filler.display())
        });
    }

    let out_png = runtime_dir.join("wolf_gnome_title_smoke.png");
    clear_evidence(&out_png, SMOKE);

    // HOME is the cwd's parent, which makes `display_title` abbreviate. Stated explicitly though it
    // matches the default, so a change to `cage_command` cannot retarget `~/titlezulu`.
    let status = cage_command(TITLE_SCENARIO, &project, &runtime_dir)
        .env("HOME", &home)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .status()
        .expect("failed to spawn `cage` for the headless window-title smoke scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage window-title scenario exited unsuccessfully ({status}); the app may \
         have failed to launch (exit 11 means it died before it was ready for input) or grim failed. \
         Screenshot expected at {}.",
        out_png.display()
    );

    assert_png_present(SMOKE, &out_png);

    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(TITLE_OCR_PRESENT),
        "{SMOKE}: OCR did not contain {TITLE_OCR_PRESENT:?} — the basename of the app's working \
         directory, which the header-bar title must render. The likely causes, in order: (a) \
         `AppShell::refresh_title` never called `window.set_title` (the documented red-run \
         revert), so the title is still the hardcoded \"Wolf\" from `app.rs`'s window builder; (b) \
         `refresh_title` ran but `live_cwd()` was still None, so `title::window_title` returned \
         the {WINDOW_TITLE_FALLBACK:?} fallback — that would be a settle/ordering problem rather \
         than a title one; (c) the title IS set but the header bar did not render it legibly, in \
         which case see this test's pre-authorized XDG_CONFIG_HOME font contingency and do NOT \
         weaken the assertion. PNG kept at {} — open it to tell those apart. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        !ocr_text.contains(TITLE_OCR_ABSENT),
        "{SMOKE}: OCR unexpectedly contained {TITLE_OCR_ABSENT:?} — the HOME segment of the app's \
         working directory. HOME is set to the project's parent for this run, so a correct title \
         reads `~/{TITLE_PROJECT_DIR}` and that segment is never drawn. A hit means \
         `title::display_title` did not abbreviate: check the segment-boundary rule (the match is \
         on `home + \"/\"`, not on `home`), and check that `shell.rs` is injecting the captured \
         `workspace::home_dir()` rather than an empty string — an empty `home` disables \
         abbreviation entirely and is the most likely cause. PNG kept at {}. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
}

/// Activating an **Open Recent** palette row re-roots the whole window: Neovim's
/// working directory changes, and the front-end settles the pending re-root and
/// restarts the sidebar traversal at the new root.
///
/// ## What it covers that no other smoke does
///
/// The only end-to-end path from the recent-directories store to a re-rooted window.
/// `PaletteInner::open` reads `WolfCore::recent_dirs()` and filters it through
/// `palette::open_recent_candidates`; `rank_palette` builds an `OpenRecent` row from
/// what survives; `palette_action_for` classifies it as
/// `PaletteAction::OpenRecentFolder`; and `shell::on_palette_activation` performs it
/// through the very same `AppShell::chdir_to` the native Open Folder dialog uses —
/// `InputEvent::Chdir` (`:tcd`), then `settle_reroot` on a later render wake,
/// then `tree_view::reroot`.
///
/// That last leg is why this is not a duplicate of
/// `command_palette_chord_runs_the_selected_command_under_cage`: that smoke proves
/// a palette row dispatches a `Command`, and an Open Recent row is not a `Command` at
/// all. It is also the only cage coverage of the Open Folder settle path, which
/// `tests/README.md` lists under "Deliberate test gaps" for the DIALOG because no
/// portal backend exists under `cage` — the dialog itself is unreachable there; its
/// downstream half is reachable through this smoke.
///
/// ## The premise, and where it is measured
///
/// The store is seeded with exactly ONE directory and the query is that directory's
/// basename, so the palette holds exactly one row. That premise is not asserted here
/// — a screenshot cannot count palette rows, since the overlay is closed before
/// `grim` fires — and is instead pinned in the fast suite by
/// `palette::tests::palette_rows_query_orzulu_selects_exactly_the_recent_folder_row`,
/// which also pins the `open_recent:<path>` key shape assertion 4 reads back.
///
/// Two facts make "exactly one candidate" true of the RUN rather than merely of the
/// seed, and neither is asserted here either: `cage_command` hands the app a fresh
/// per-run `WOLF_STATE_DIR`, so `folder-mru.sqlite` starts empty; and
/// `wolf_core::redraw::apply_cwd` never records the LAUNCH working directory (the
/// initial `wolf_cwd` emit is flagged), so nothing is added before the chord fires.
/// If either changed, the palette would hold a second folder row and this smoke would
/// become stochastic rather than red — which is what assertion 0 is watching for from
/// the other side.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. The `measured` column is from an actual red run, per
/// `tests/README.md`'s "A red-run demonstration is a deliverable, not a formality".
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 0 | the seeded store reads the canonical target back | **precondition** — `RecentDirs::record` is fire-and-forget and swallows every error, so without this a failed seed surfaces as a mysterious empty palette hundreds of lines later | none; do not invent one | n/a |
/// | 0b | the TEST process has no `WOLF_STATE_DIR`, and the developer's real `folder-mru.sqlite` is byte-identical across the run | **precondition** — the isolation proof, the same two checks `single_pane_file_survives_relaunch_under_cage` opens with, aimed at the first store this binary has ever touched | none; do not invent one | n/a |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | n/a |
/// | 2 | [`assert_png_present`] | precondition — grim produced an image | none; do not invent one | n/a |
/// | 3 | the `getcwd()` readback canonicalizes to the seeded target | **discriminator** — Neovim's working directory really changed | in `shell::on_palette_activation`, replace `self.chdir_to(path)` with an `eprintln!` and no send. The chord still opens the palette, the row still ranks, Enter still closes it and records the MRU key — but nothing is sent to Neovim, so the readback is the launch cwd. | RED. `assertion `left == right` failed: open_recent_palette_row_reroots_the_window_under_cage: after the Open Recent activation the captive Neovim's working directory is /tmp/nix-shell.kFZSo6/wolf-gnome-smoke-2578175-1785817088670263436/project, not the seeded target /tmp/nix-shell.kFZSo6/wolf-gnome-smoke-2578175-1785817088670263436/orzulu.`, `left: ".../project"`, `right: ".../orzulu"`, then `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 34 filtered out; finished in 8.00s`. The stubbed arm's own `eprintln!` printed the correct target path, so the row WAS activated and classified to `OpenRecentFolder` — what the readback reports is the launch cwd purely because nothing was sent. Restored, green. |
/// | 4 | the command MRU holds `open_recent:<canonical target>` | **discriminator** — the row was ACTIVATED, it was an Open Recent row, and it carried core's key shape. Independent of the pixels entirely: nothing but a palette activation writes that store. | in `palette::activate_selection`, delete the `mru.record(&mru_key)` call → the key is absent and the store reads back empty. | RED. `open_recent_palette_row_reroots_the_window_under_cage: the command MRU at /tmp/nix-shell.HA9znD/wolf-gnome-smoke-2579118-1785817109092969527/project-state/command-mru.sqlite does not hold "open_recent:/tmp/nix-shell.HA9znD/wolf-gnome-smoke-2579118-1785817109092969527/orzulu"; it holds [].`, then `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 34 filtered out; finished in 7.98s`. Isolated: the panic is at assertion 4, so assertion 3 was green — Neovim did chdir and only the MRU write is gone. Restored, green. |
/// | 5 | OCR contains [`OPENRECENT_OCR_PRESENT`] | **discriminator** — the FRONT-END settled the re-root and restarted the sidebar traversal at the new root | In `shell::settle_reroot`, delete the `self.inner.tree.reroot(..)` call from the `Reroot` arm, keeping `set_empty_state` and the `rooted_cwd` write — see the note below for why it is this call and not `chdir_to`. Neither producer then re-roots the sidebar: `ortango` is absent, and assertions 3 and 4 stay GREEN. | RED. `open_recent_palette_row_reroots_the_window_under_cage: OCR did not contain "ortango" — the name of ortango.md, which lives inside the target directory and can therefore only be drawn once the tree sidebar re-rooted there.`, then `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 37 filtered out; finished in 8.15s`. Isolated: the panic is at assertion 5, so assertions 3 and 4 were green, and the captured page carries `~/orzulu` in the header bar — the title followed the live cwd — while the sidebar still draws the launch root `project` with `orbase.md` under it. Restored, green. |
///
/// ## Why assertion 5's revert deletes the `tree.reroot` call
///
/// It deletes `tree.reroot(..)` from `settle_reroot`'s `Reroot` arm rather than
/// replacing `chdir_to` with a bare `InputEvent::Chdir` send, because the bare send
/// leaves this smoke GREEN: `workspace::reroot_decision`'s `pending == None,
/// live != rooted` arm re-roots on Neovim's live working directory whichever producer
/// set the pending flag, so removing the pending record removes nothing observable.
/// A revert row that does not redden is worse than no row at all — it is a documented
/// claim that is false. Deleting the one call BOTH producers share is the smallest
/// edit that removes the behaviour this assertion names.
///
/// Assertion 5 reads the SIDEBAR rather than the header-bar title on purpose. The
/// title would only prove `AppShell::refresh_title` read the new `live_cwd`, which
/// assertion 3 already implies; the sidebar row is the only observable that proves
/// the settle point ran through to `tree_view::reroot`. [`OPENRECENT_TARGET_FILE`] lives
/// OUTSIDE the launch cwd, so it can reach the page only after the re-root — and the
/// crossed-pair guard in
/// `restore_fixture_names_cannot_satisfy_their_own_assertions` is what stops the
/// header bar (which draws `~/orzulu` on every capture) satisfying it instead.
///
/// ## No must-NOT-contain assertion, deliberately
///
/// `InputEvent::Chdir` is `:tcd`: it closes no buffer and reloads nothing, so
/// [`OPENRECENT_START_HEADING`] is still painted after a perfectly correct re-root and
/// the cmdline strip may still carry the earlier `:e` toast. An absence assertion
/// here would assert something the feature does not guarantee — which is also why
/// that heading is not an `ASSERTED_OCR_TERMS` entry.
#[test]
fn open_recent_palette_row_reroots_the_window_under_cage() {
    const SMOKE: &str = "open_recent_palette_row_reroots_the_window_under_cage";

    // Assertion 0b, first part: checked first, since `state_dir()` resolves `WOLF_STATE_DIR` ahead of
    // everything and an override here would make the isolation check self-referential.
    assert!(
        std::env::var_os("WOLF_STATE_DIR").is_none(),
        "{SMOKE}: WOLF_STATE_DIR is set in the TEST process. `wolf_core::state_dir()` resolves \
         that variable FIRST, so the \"developer's real folder-mru.sqlite\" this smoke proves is \
         untouched would be resolved against the override instead of against $XDG_STATE_HOME/wolf \
         — the isolation assertion would then compare the smoke's own store with itself and pass \
         no matter what the app did. `cage_command` sets WOLF_STATE_DIR on the CHILD only, which \
         is the whole design; nothing may set it on the test process."
    );

    let real_state_dir = state_dir().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::state_dir() resolved to None in the test process, so this smoke \
             cannot say WHICH store it is proving untouched and its isolation assertion would \
             hold vacuously. On Linux it is $XDG_STATE_HOME/wolf (falling back to the data dir), \
             so a None here means neither is resolvable — check $HOME."
        )
    });
    let real_recent_file = real_state_dir.join(RECENT_DIRS_DB_NAME);
    // Bytes, not a fingerprint: `record` can rewrite one row without changing the size. `None` is a
    // real answer, since "absent before and after" is as strong as "unchanged".
    let real_before = std::fs::read(&real_recent_file).ok();

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // The LAUNCH working directory, `<unique>/project`, holding the base file the
    // scenario opens plus the nine density fillers.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[(OPENRECENT_START_FILE, OPENRECENT_START_HEADING)],
    );

    // The target is the cwd's sibling, drawn as `~/orzulu`, with its own fillers since the sidebar
    // re-roots. See notes/crates/wolf-gnome/tests/smoke_headless/sidebar.rs.md#open-recent-fixtures
    let target_dir = smoke_home(&cwd).join(OPENRECENT_TARGET_DIR);
    std::fs::create_dir_all(&target_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the Open Recent target dir {}: {e}",
            target_dir.display()
        )
    });
    write_relaunch_fixtures(
        SMOKE,
        &target_dir,
        &[(OPENRECENT_TARGET_FILE, SIDEBAR_FILLER_BODY)],
    );

    // Canonical: `chdir_to` canonicalizes, `RecentDirs` stores as given, and /tmp may be a symlink.
    let target_canon = std::fs::canonicalize(&target_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to canonicalize the Open Recent target dir {}: {e}. Every comparison \
             below is against the canonical path, because /tmp may be a symlink and the app \
             canonicalizes before it sends InputEvent::Chdir.",
            target_dir.display()
        )
    });

    // The state dir `cage_command` hands the app as WOLF_STATE_DIR: the store this
    // test seeds, and the one assertion 4 reads the command MRU back out of.
    let smoke_state = smoke_state_dir(&cwd);

    // The scoped block drops the connection and its WAL handle before the app opens the same file.
    {
        RecentDirs::open(&smoke_state).record(&target_canon);
    }

    // Assertion 0 (precondition): `RecentDirs::record` swallows every error, so a failed seed would
    // otherwise surface as assertion 3 blaming the shell.
    let seeded = RecentDirs::open(&smoke_state).recent();
    assert!(
        seeded.iter().any(|dir| dir == &target_canon),
        "{SMOKE}: the seeded recent-directories store at {} does not hold {}; it holds {seeded:?}. \
         `RecentDirs::record` swallows every error, so an empty list means the seed silently \
         failed (an unwritable state dir, or a corrupt store the self-heal could not recreate) and \
         the app under cage would open a palette with NO folder rows at all. Nothing below is \
         about the feature until this is green.",
        smoke_state.join(RECENT_DIRS_DB_NAME).display(),
        target_canon.display()
    );

    let readback = cwd.join(OPENRECENT_READBACK_NAME);
    let out_png = runtime_dir.join("wolf_gnome_open_recent_smoke.png");
    for path in [&readback, &out_png] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(OPENRECENT_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("OPENRECENT_START_FILE", OPENRECENT_START_FILE)
        .env("OPENRECENT_QUERY", OPENRECENT_QUERY)
        .env("OPENRECENT_READBACK", &readback)
        .status()
        .expect("failed to spawn `cage` for the headless Open Recent smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage Open Recent scenario exited unsuccessfully ({status}). Exit 11: the app \
         died before it was ready for input — a LAUNCH failure, and nothing below is about Open \
         Recent. Exit 14: the `getcwd()` readback never appeared, which means the palette never \
         CLOSED and the Ex command was typed into its SearchEntry — a distinct cause from \"the \
         wrong row ran\", which exits 0 and fails assertion 3 instead. Otherwise wtype or grim \
         failed. Screenshot expected at {}.",
        out_png.display()
    );

    // --- Assertion 2 (precondition): grim produced an image -------------------
    assert_png_present(SMOKE, &out_png);

    // Assertion 3 (discriminator): Neovim's own cwd, which no page text or OCR can produce.
    let got_cwd = read_trimmed(&readback, SMOKE);
    let got_canon = std::fs::canonicalize(&got_cwd).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: the captive Neovim reported its working directory as {got_cwd:?}, which \
             could not be canonicalized: {e}. The scenario writes `getcwd()` verbatim, so a \
             non-existent path here means Neovim is sitting in a directory that has since gone — \
             not a comparison problem."
        )
    });
    assert_eq!(
        got_canon,
        target_canon,
        "{SMOKE}: after the Open Recent activation the captive Neovim's working directory is {}, \
         not the seeded target {}. The LAUNCH cwd here means nothing reached Neovim at all: the \
         palette row was never activated, or `shell::on_palette_activation` has no \
         PaletteAction::OpenRecentFolder arm, or `chdir_to` sent nothing. Assertion 4 below is \
         pixel-independent and says whether the ROW was activated, so read the two together — an \
         activated row with an unchanged cwd points squarely at the shell's arm. A THIRD \
         directory would mean the palette activated a different folder row than the one seeded, \
         which the fast suite's \
         `palette_rows_query_orzulu_selects_exactly_the_recent_folder_row` would also have caught. \
         PNG kept at {}.",
        got_canon.display(),
        target_canon.display(),
        out_png.display()
    );

    // Assertion 4 (discriminator): only a palette activation writes this store, so a hit means an Open
    // Recent row ran with core's key shape.
    let want_key = format!("open_recent:{}", target_canon.display());
    let recent = CommandMru::open(&smoke_state).recent();
    assert!(
        recent.iter().any(|key| key == &want_key),
        "{SMOKE}: the command MRU at {} does not hold {want_key:?}; it holds {recent:?}. Nothing \
         but a palette activation records into that store, so an EMPTY list means activation never \
         called `CommandMru::record` (or the app resolved a different state dir — `cage_command` \
         is the only thing that sets WOLF_STATE_DIR). A NON-empty list holding a `new_buffer`-style \
         registry id means the palette activated a COMMAND row rather than the folder row, so the \
         query stopped being unique — the fast suite's \
         `palette_rows_query_orzulu_selects_exactly_the_recent_folder_row` pins that. A key with \
         the right prefix and a different path means a different recent directory was offered and \
         chosen.",
        smoke_state.join("command-mru.sqlite").display()
    );

    // Assertion 5 (discriminator): the front-end re-rooted. `ortango.md` is outside the launch cwd, and
    // the guard asserts the `~/orzulu` title cannot satisfy it.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(OPENRECENT_OCR_PRESENT),
        "{SMOKE}: OCR did not contain {OPENRECENT_OCR_PRESENT:?} — the name of \
         {OPENRECENT_TARGET_FILE}, which lives inside the target directory and can therefore only \
         be drawn once the tree sidebar re-rooted there. With assertion 3 GREEN (Neovim did \
         chdir), the likely causes in order: (a) the settle point did not run through to \
         `tree_view::reroot`, so the sidebar kept its old root — this is the documented revert for \
         this row, and it leaves assertion 3 green exactly as you see it. Note that a MISSING \
         pending re-root is NOT among them: `workspace::reroot_decision` re-roots on Neovim's \
         live working directory even with no pending request, which is why this row's revert \
         deletes the `tree.reroot` call both producers share; (b) the re-root fired but the \
         sidebar's fresh `FileSearch` had not streamed its \
         entries by the time `grim` ran, which is a TIMING problem and is fixed by raising the \
         scenario's two post-activation SETTLEs, never by weakening this assertion; (c) the row \
         rendered and OCR misread it — check the retained PNG before touching anything. Note the \
         launch cwd's own rows are expected to be GONE from the capture, since the sidebar \
         re-roots rather than merging. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 0b, second part: last, so it covers the whole run, and in bytes, since `record` can
    // rewrite a row without changing the size.
    let real_after = std::fs::read(&real_recent_file).ok();
    assert!(
        real_before == real_after,
        "{SMOKE}: the developer's REAL recent-directories store {} changed across this smoke run \
         (before: {:?} byte(s), after: {:?} byte(s); `None` means the file did not exist, which is \
         the normal state on a machine that has never run Wolf interactively and must stay that \
         way). This is a STATE-DIR ISOLATION FAILURE, and this smoke is the first one that can see \
         it for THIS store: the app under cage resolved `wolf_core::state_dir()` to the real \
         directory instead of the smoke's own, so it offered the developer's real Open Recent \
         history in its palette and appended this run's throwaway temp directory to it. \
         `cage_command` is the single place WOLF_STATE_DIR is set, and \
         `cage_is_only_spawned_through_the_isolating_helper` is what keeps it single — start \
         there. The ONE benign cause: a real Wolf running concurrently in another terminal, which \
         records into that store on every directory change; close it and re-run before concluding \
         anything else. Note the `-wal` and `-shm` sidecars are NOT compared, so a leak that is \
         still uncommitted at this instant would escape — that is a gap in this check's \
         sensitivity, never a licence to relax it.",
        real_recent_file.display(),
        real_before.as_ref().map(Vec::len),
        real_after.as_ref().map(Vec::len)
    );
}

/// Ctrl+Shift+B hides the file-tree sidebar, and the editor keeps drawing.
///
/// ## What it covers that nothing else does
///
/// The only end-to-end coverage of `win.toggle-sidebar`, whose `BindingStatus` is
/// `Bound` and which is therefore visible to the chord matcher at all: the chord
/// resolving through `actions::action_for_key`, `command_action::ui_action_for`
/// classifying `Command::ToggleSidebar` as a DISPATCHED
/// `UiAction::ToggleSidebar` rather than a `Deferred` one,
/// `shell::perform`'s arm flipping the stored preference, and
/// `AppShell::apply_sidebar_visibility` — over `state_seam::sidebar_is_shown` —
/// being the single site that touches the widget.
///
/// It does NOT cover the RESTORE half: no smoke hides the sidebar, quits and
/// relaunches. That gap is recorded in `tests/README.md`, "What is still unproven",
/// together with the reason it matters — the persistence write side changed from
/// `!empty` to the stored preference, and a stale write plus a live read would hide
/// the sidebar after any empty-state session.
///
/// ## The three-way guard on a `!contains` assertion
///
/// A must-NOT-contain assertion is satisfied by a crashed window, a blank one, and
/// one whose sidebar never populated in the first place. All three are closed, and
/// none of the three may be weakened:
///
/// 1. capture A must CONTAIN [`SIDEBARTOGGLE_OCR_ABSENT`] — the tree was populated
///    at all, so there was something to hide;
/// 2. capture B must contain [`SIDEBARTOGGLE_OCR_KEEP`] — the window is still
///    DRAWING, so a crash or a blank frame cannot satisfy assertion 6;
/// 3. the fixture/term crossed pair is asserted in
///    `restore_fixture_names_cannot_satisfy_their_own_assertions`, so the
///    subject cannot silently stop being drawable.
///
/// ## Assertions, classified
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | n/a — no revert of its own. GREEN under row 6's revert: the scenario exits 0 whether or not the sidebar hid, so the run reached assertion 6. |
/// | 2 | both PNGs present | precondition — grim produced two images | none; do not invent one | n/a — no revert of its own. GREEN under row 6's revert; both captures were retained (`/run/user/1000/wolf_gnome_sidebartoggle_before_smoke.png` and `…_after_smoke.png`) and that run's failure message quotes the OCR of the second. |
/// | 3 | capture A contains [`SIDEBARTOGGLE_OCR_ABSENT`] | **precondition** — the sidebar was populated before the chord. Without it assertion 6 holds for a tree that never rendered. | none | n/a — no revert of its own. GREEN under row 6's revert, which is exactly what makes that red run readable: the tree row was there before the chord, so its survival afterwards is the toggle's doing. |
/// | 4 | capture A contains [`SIDEBARTOGGLE_OCR_KEEP`] | **precondition** — the editor was drawing before the chord, so assertion 5 is measuring a change rather than a starting state | none | n/a — no revert of its own. GREEN under row 6's revert. |
/// | 5 | capture B contains [`SIDEBARTOGGLE_OCR_KEEP`] | **anti-vacuity, not a discriminator** — the window is still drawing after the chord, which is what stops a crashed or blank window satisfying assertion 6 | none; it is deliberately insensitive to the toggle | n/a — no revert of its own, and GREEN under row 6's revert by design: the same OCR dump that still held `sbtangoleaf` also held `sidebarkeepalfa`, so the window was demonstrably still drawing and assertion 6's failure is attributable to the toggle alone. |
/// | 6 | capture B lacks [`SIDEBARTOGGLE_OCR_ABSENT`] | **the discriminator** | replace `shell::perform`'s `UiAction::ToggleSidebar` arm body with `{}`. The chord still resolves and the action still activates, but the widget never hides, so the row is still on screen in capture B. | RED, in 6.11 s, with that arm body replaced by `{}`: `the SECOND capture still contains "sbtangoleaf" — the tree row for sbtangoleaf.md — so Ctrl+Shift+B did not hide the sidebar.` The dumped OCR carried the whole tree (`fillalfa.md` … `sbtangoleaf.md`, `sbwhiskey.md`) alongside `sidebarkeepalfa`, so assertions 3, 4 and 5 all passed under it and this row failed alone; `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 36 filtered out; finished in 6.11s`. |
#[test]
fn sidebar_toggle_chord_hides_the_file_tree_under_cage() {
    const SMOKE: &str = "sidebar_toggle_chord_hides_the_file_tree_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // The fillers are page density for capture A and more rows for the toggle to take away.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[(SIDEBARTOGGLE_OPEN_FILE, SIDEBARTOGGLE_HEADING)],
    );

    // Written by hand: its name is the assertion's subject, so it is absent from
    // `RELAUNCH_FIXTURE_NAMES`. Never opened; the body only keeps it non-empty.
    let observable = cwd.join(SIDEBARTOGGLE_FILE);
    std::fs::write(&observable, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to write the sidebar observable {}: {e}",
            observable.display()
        )
    });

    // TWO captures: with the sidebar shown, and with it hidden. Both cleared first —
    // evidence that could be a previous run's leftover is not evidence.
    let out_png_before = runtime_dir.join("wolf_gnome_sidebartoggle_before_smoke.png");
    let out_png = runtime_dir.join("wolf_gnome_sidebartoggle_after_smoke.png");
    for path in [&out_png_before, &out_png] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(SIDEBARTOGGLE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("OUT_PNG_BEFORE", &out_png_before)
        .env("SIDEBARTOGGLE_OPEN_FILE", SIDEBARTOGGLE_OPEN_FILE)
        .status()
        .expect("failed to spawn `cage` for the headless sidebar-toggle smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage sidebar-toggle scenario exited unsuccessfully ({status}).\n\
         Exit 11: the app died before it was ready for input — a LAUNCH failure, and nothing below \
         is about the sidebar.\n\
         Any other status means wtype or grim failed, or the `:e` did not load. This scenario \
         writes no readback file, so it has no other exit codes of its own; note that it exits 0 \
         whatever the chord did.\n\
         Captures expected at {} (sidebar shown) and {} (sidebar hidden).",
        out_png_before.display(),
        out_png.display()
    );

    // --- Assertion 2 (precondition): grim produced both images ----------------
    assert_png_present(SMOKE, &out_png_before);
    assert_png_present(SMOKE, &out_png);

    let before_text = ocr_to_lowercase(&out_png_before);
    let after_text = ocr_to_lowercase(&out_png);

    // Assertion 3 (precondition): the sidebar populated, or assertion 6 holds for an empty tree.
    assert!(
        before_text.contains(SIDEBARTOGGLE_OCR_ABSENT),
        "{SMOKE}: the FIRST capture, taken with the sidebar still shown, does not contain \
         {SIDEBARTOGGLE_OCR_ABSENT:?} — the tree row for {SIDEBARTOGGLE_FILE}. This is a \
         PRECONDITION failure: the sidebar never listed the file, so assertion 6 below would hold \
         whatever the toggle chord did. The likely causes, in order: (a) the streamed FileSearch \
         had not reached the file when grim fired — raise TREE_SETTLE in \
         SIDEBARTOGGLE_SCENARIO, do not touch the assertion; (b) the app launched into the EMPTY \
         state, in which the sidebar is hidden from the start (check that `make_temp_cwd`'s \
         directory is not also HOME); (c) the row rendered but OCR misread it — look at the \
         retained capture first. PNG kept at {}. OCR output was:\n{before_text}",
        out_png_before.display()
    );

    // Assertion 4 (precondition): the editor was drawing before the chord.
    assert!(
        before_text.contains(SIDEBARTOGGLE_OCR_KEEP),
        "{SMOKE}: the FIRST capture does not contain {SIDEBARTOGGLE_OCR_KEEP:?} — the heading of \
         {SIDEBARTOGGLE_OPEN_FILE}, which the scenario opens before capturing. This is a \
         PRECONDITION failure: the editor was not drawing the keep-term even before the chord, so \
         assertion 5 could not distinguish \"the window is still alive\" from \"it never drew this \
         at all\". Check that the `:e` landed. PNG kept at {}. OCR output was:\n{before_text}",
        out_png_before.display()
    );

    // Assertion 5 (anti-vacuity): insensitive to the toggle on purpose; it stops a crashed, blank or
    // frozen window from satisfying assertion 6.
    assert!(
        after_text.contains(SIDEBARTOGGLE_OCR_KEEP),
        "{SMOKE}: the SECOND capture does not contain {SIDEBARTOGGLE_OCR_KEEP:?} — the heading of \
         {SIDEBARTOGGLE_OPEN_FILE}, which the toggle must not affect. The window has stopped \
         drawing, which means assertion 6 below cannot be trusted: a blank or crashed window \
         satisfies a must-NOT-contain assertion just as a correctly hidden sidebar does, and this \
         assertion exists precisely to tell the two apart. Look for a crash on the toggle path \
         (`apply_sidebar_visibility` is the single site that calls `set_visible` and \
         `set_position`) before concluding anything about the sidebar. Assertion 4 passed, so the \
         editor WAS drawing this heading a moment earlier.\n\
         One non-product cause to rule out FIRST, from the retained capture: hiding the sidebar \
         also removes every sidebar row from the page, and `tesseract` classifies a word by the \
         whole page (see SIDEBAR_FILLER_COUNT). If the heading is plainly legible in the PNG and \
         only OCR disagrees, the fix is page DENSITY inside the editor — give \
         SIDEBARTOGGLE_OPEN_FILE a digits-and-`x` body, the `divider_right_fixture_body` / \
         `search_target_fixture_body` precedent, and register that body with the content loop in \
         `restore_fixture_names_cannot_satisfy_their_own_assertions`. Never weaken this assertion: \
         it is the only thing standing between assertion 6 and a blank window. PNG kept at {}. OCR \
         output was:\n{after_text}",
        out_png.display()
    );

    // --- Assertion 6 (THE DISCRIMINATOR): the sidebar is gone -----------------
    assert!(
        !after_text.contains(SIDEBARTOGGLE_OCR_ABSENT),
        "{SMOKE}: the SECOND capture still contains {SIDEBARTOGGLE_OCR_ABSENT:?} — the tree row \
         for {SIDEBARTOGGLE_FILE} — so Ctrl+Shift+B did not hide the sidebar. Assertion 3 proved \
         the row was there to begin with and assertion 5 proved the window is still drawing, so \
         this is the toggle itself. The likely causes, in order: (a) the chord never fired — check \
         that `win.toggle-sidebar`'s BindingStatus is Bound rather than Reserved; if it fell \
         through to Neovim it arrived as <C-b> (page backward), which on this buffer does nothing \
         visible; (b) `command_action::ui_action_for` still classifies `Command::ToggleSidebar` as \
         `Deferred`, in which case the app's stderr carries \"command ToggleSidebar not \
         dispatched\"; (c) the action IS installed but is disabled — it is disabled in the empty \
         state, and `AppShell::install_chord_controller` returns Propagation::Proceed for a \
         disabled action; (d) `apply_sidebar_visibility` ran but `state_seam::sidebar_is_shown` \
         answered true — its table is the fast-suite test to read. PNG kept at {} (and the \
         before-capture at {}). OCR output was:\n{after_text}",
        out_png.display(),
        out_png_before.display()
    );
}

/// A **typed `:cd`** re-roots the whole window, and the sidebar's top level is a
/// SYNTHETIC ROOT NODE named after the working directory.
///
/// ## What it covers that no other smoke does
///
/// Two user-visible behaviours at once, deliberately folded into one scenario:
///
/// 1. **The synthetic cwd root node.** The tree's top level is a single
///    `TreeNode::Dir` whose name is the cwd's basename and whose children are
///    `children_of(entries, "")`, auto-expanded once its first non-empty batch
///    arrives. Assertion 6 reads that row.
/// 2. **The re-root on Neovim's OWN working directory.** A directory change typed
///    into Neovim re-roots the sidebar with no front-end `chdir_to` and no
///    `pending_reroot` record behind it: `workspace::reroot_decision`'s
///    `pending == None, live != rooted` arm acts on Neovim's live working
///    directory alone. Assertion 4 reads the re-rooted listing.
///
/// It is the only cage coverage of either.
/// [`open_recent_palette_row_reroots_the_window_under_cage`] drives the OTHER
/// producer — a front-end `chdir_to` — through the same settle point, so the two are
/// complementary rather than overlapping: that smoke proves a PENDING re-root still
/// settles, this one proves a re-root with NO pending request happens at all.
///
/// ## Why the two rows are folded into one scenario (it is forced, not convenient)
///
/// A standalone synthetic-root smoke has no discriminating assertion available. The
/// root row draws the cwd's BASENAME and the `adw::HeaderBar` draws `~/<basename>` on
/// every capture, so `contains(basename)` is satisfied by the TITLE with no root row
/// on screen — the exact silent-pass shape `PICKER_FILE_NAME`'s note documents. The
/// only way to stop the title drawing the basename is `cwd == home`, which
/// `title::display_title` rule 4 renders as the bare `~`; and
/// `workspace::launch_decision` refuses `home` as a launch project, so the only route
/// into that state is a live `:cd`. Hence one scenario with two ordered
/// discriminators, each with its own isolated revert. (The alternative — a `grim -g`
/// region capture of the sidebar strip — introduces a hardcoded geometry this binary's
/// conventions reject.)
///
/// ## The three load-bearing fixture facts
///
/// They are stated on the `CD_*` consts and asserted where they can be; the short
/// form is: [`cage_command`]'s default `HOME` is the cwd's PARENT, which here is the
/// `:cd` target, so after `:cd ..` the title is `~`; `WOLF_STATE_DIR` must be
/// OVERRIDDEN because the default sits inside the post-`:cd` search root and Neovim's
/// swap file names there literally contain [`CD_OCR_ROOT_NODE`]; and the readback
/// lands outside both search roots.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. The `measured` column is from an actual red run, per
/// `tests/README.md`'s "A red-run demonstration is a deliverable, not a formality".
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 0 | the DEFAULT state dir is inside the post-`:cd` search root and the OVERRIDE is not | **precondition** — makes the reason for the `WOLF_STATE_DIR` override executable instead of prose, so a later change to [`smoke_state_dir`] cannot silently re-arm the swap-file silent pass assertion 6 would take | none; do not invent one | n/a |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | n/a |
/// | 2 | [`assert_png_present`] | precondition — grim produced an image | none; do not invent one | n/a |
/// | 3 | the `getcwd()` readback canonicalizes to [`CD_ROOT_DIR`] | **precondition** — the typed `:cd` reached Neovim at all. It is NOT a discriminator: no front-end revert can break it, because nothing in `wolf-gnome` sits between `wtype` and Neovim's own `:cd`. | none; **do not invent one** | n/a |
/// | 4 | OCR contains [`CD_OCR_ROOTED`] | **discriminator, the re-root row** | in `shell::settle_reroot`, add `if self.inner.pending_reroot.borrow().is_none() { return; }` at the top — restoring "only a front-end chdir re-roots". Assertion 3 stays green; the sidebar goes on listing `startdir`'s files and `cdtango` never reaches the page. | RED. `typed_cd_reroots_the_sidebar_under_cage: OCR did not contain "cdtango" — the name of cdtango.md, which exists ONLY inside the directory the `:cd` moved to and is therefore drawable only as a child row under a re-rooted, EXPANDED synthetic root.`, then `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 37 filtered out; finished in 7.63s`. Assertion 3 was green, so the attribution held. NOT ISOLATED, as the section below predicts: the OCR dump read `v startdir` over the launch cwd's own files with `cdrootzulu` ALSO absent, i.e. cause (a) by the message's own discriminator — so assertion 6 was never evaluated under this revert. What makes the two rows independently falsifiable is the other direction, measured in row 6. |
/// | 5 | OCR does **not** contain [`CD_OCR_UNABBREVIATED`] | **precondition guard for assertion 6**, asserted BEFORE it deliberately: a title that failed to abbreviate draws the whole path, which contains both this term and [`CD_OCR_ROOT_NODE`], and would satisfy assertion 6 spuriously | none of ours; it fires only if `title::display_title` or `workspace::home_dir` regressed | n/a |
/// | 6 | OCR contains [`CD_OCR_ROOT_NODE`] | **discriminator, the synthetic-root row** | in `tree_view`'s root-store rebuild, append `tree::children_of(&entries, "")` instead of the single `root_node(..)`. Assertion 4 stays GREEN — the children still render, just at top level — but no row bears the cwd basename. Isolated. | RED, and ISOLATED as claimed. `typed_cd_reroots_the_sidebar_under_cage: OCR did not contain "cdrootzulu" — the basename of the working directory, which the sidebar's SYNTHETIC ROOT ROW must draw.`, then `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 37 filtered out; finished in 7.66s`. The panic is at assertion 6, so assertions 3, 4 and 5 were all green, and the captured OCR carries `cdtango.md` — the re-rooted listing rendered, at top level, with no row bearing the cwd basename. Restored, green. |
///
/// ## What the two reverts isolate, stated honestly
///
/// Assertion 6's revert is genuinely isolated: assertions 3, 4 and 5 all stay green
/// under it and only assertion 6 reddens.
///
/// Assertion 4's revert is NOT symmetric, and the asymmetry is inherent rather than a
/// weakness in the fixture: with no re-root at all the sidebar keeps the LAUNCH root
/// label, so [`CD_OCR_ROOT_NODE`] is absent too. Assertion 4 fails first and
/// assertion 6 is never evaluated under it — the same shape as the first project-search
/// smoke's assertion 6, which every falsifier reaching it reddens assertion 5 before.
/// What makes the two rows independently falsifiable is the OTHER direction:
/// assertion 6's revert reddens 6 alone.
///
/// ## No must-NOT-contain assertion on the launch cwd's contents, deliberately
///
/// `:cd` closes no buffer and reloads nothing, so [`CD_BASE_HEADING`] is still
/// painted after a perfectly correct re-root and the cmdline strip may still carry
/// the earlier `:e` toast. An absence assertion there would assert something the
/// feature does not guarantee — which is also why that heading is not an
/// `ASSERTED_OCR_TERMS` entry. [`CD_START_DIR`] is likewise expected to remain on
/// the page, as a collapsed `Dir` row under the new root.
#[test]
fn typed_cd_reroots_the_sidebar_under_cage() {
    const SMOKE: &str = "typed_cd_reroots_the_sidebar_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // The launch cwd is `base/cdouterhome/cdrootzulu/startdir`, so the default HOME is `cdrootzulu`.
    // See notes/crates/wolf-gnome/tests/smoke_headless/sidebar.rs.md#typed-cd-fixtures
    let base = make_temp_cwd();
    let outer = base.join(CD_OUTER_DIR);
    let root = outer.join(CD_ROOT_DIR);
    let cwd = root.join(CD_START_DIR);
    std::fs::create_dir_all(&cwd).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the nested launch cwd {}: {e}",
            cwd.display()
        )
    });

    // The `:cd` target gets fillers of its own: the sidebar re-roots, so density is paid twice.
    write_relaunch_fixtures(SMOKE, &root, &[]);

    // Written by hand: its name is assertion 4's subject, so it is absent from
    // `RELAUNCH_FIXTURE_NAMES`. Never opened; the body only keeps it non-empty.
    let observable = root.join(CD_TARGET_FILE);
    std::fs::write(&observable, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to write the re-root observable {}: {e}",
            observable.display()
        )
    });

    // The LAUNCH cwd's contents: the file the scenario opens, plus its own fillers.
    write_relaunch_fixtures(SMOKE, &cwd, &[(CD_BASE_FILE, CD_BASE_HEADING)]);

    // Canonical, since /tmp may resolve through a symlink and `getcwd()` reports Neovim's resolution.
    let root_canon = std::fs::canonicalize(&root).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to canonicalize the `:cd` target {}: {e}. Assertion 3 compares against \
             the canonical path because /tmp may be a symlink.",
            root.display()
        )
    });

    // Assertion 0 (precondition): the default state dir is in the `:cd` root, so the override matters.
    // See notes/crates/wolf-gnome/tests/smoke_headless/sidebar.rs.md#typed-cd-fixtures
    let default_state = smoke_state_dir(&cwd);
    assert!(
        default_state.starts_with(&root),
        "{SMOKE}: `smoke_state_dir` returned {}, which is NOT inside the `:cd` target {}. This \
         assertion exists to keep the WOLF_STATE_DIR override below honest — it is written for a \
         default that WOULD pollute the post-`:cd` search root. A default that no longer does is \
         not a failure of the app, but it does mean this smoke carries an override and a paragraph \
         of reasoning for nothing; re-read `smoke_state_dir` and simplify, do not silence this.",
        default_state.display(),
        root.display()
    );
    let state_override = base.join(CD_STATE_DIR);
    std::fs::create_dir_all(&state_override).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the smoke-owned state dir {}: {e}",
            state_override.display()
        )
    });
    // This smoke does not go through `smoke_state_dir_at`, so it seeds the spell
    // override itself; see `seed_smoke_spell_override`.
    seed_smoke_spell_override(&state_override);
    assert!(
        !state_override.starts_with(&root),
        "{SMOKE}: the WOLF_STATE_DIR override {} is INSIDE the `:cd` target {}, so the app's own \
         FileSearch will walk it after the re-root. The captive Neovim's swap file names there \
         encode the full buffer path with `%` separators and therefore contain \
         {CD_OCR_ROOT_NODE:?} verbatim — a swap-file row would satisfy assertion 6 with no \
         synthetic root row on screen, and this smoke would pass green against exactly the build \
         assertion 6's revert produces. Move the override back out of the search root; do NOT \
         weaken assertion 6.",
        state_override.display(),
        root.display()
    );

    // In `base`, outside both search roots; its echoed absolute path carries no asserted term.
    let readback = base.join(CD_READBACK_NAME);
    let out_png = runtime_dir.join("wolf_gnome_typed_cd_smoke.png");
    for path in [&readback, &out_png] {
        clear_evidence(path, SMOKE);
    }

    // `WOLF_STATE_DIR` is chained after `cage_command` so it wins; `HOME` keeps the helper's default,
    // which every assertion below is written against.
    let status = cage_command(CD_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_STATE_DIR", &state_override)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("CD_BASE_FILE", CD_BASE_FILE)
        .env("CD_READBACK", &readback)
        .status()
        .expect("failed to spawn `cage` for the headless typed-cd smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage typed-`:cd` scenario exited unsuccessfully ({status}). Read the exit \
         code FIRST; each one names a different half of the run.\n\
         Exit 11: the app died before it was ready for input. A LAUNCH failure — the `:cd` was \
         never typed and nothing below is about the sidebar.\n\
         Exit 14: the `getcwd()` readback never appeared, which means the Ex command never \
         executed — keys stopped reaching Neovim. That is DISTINCT from \"the `:cd` went somewhere \
         unexpected\", which exits 0 and fails assertion 3 instead.\n\
         Any other status means wtype or grim failed. Screenshot expected at {}.",
        out_png.display()
    );

    // --- Assertion 2 (precondition): grim produced an image -------------------
    assert_png_present(SMOKE, &out_png);

    // Assertion 3 (precondition): Neovim's own `:cd` landed. No front-end change breaks it, so it has
    // no revert; it attributes a red 4 or 6 to the front-end.
    let got_cwd = read_trimmed(&readback, SMOKE);
    let got_canon = std::fs::canonicalize(&got_cwd).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: the captive Neovim reported its working directory as {got_cwd:?}, which \
             could not be canonicalized: {e}. The scenario writes `getcwd()` verbatim, so a \
             non-existent path here means Neovim is sitting in a directory that has since gone — \
             not a comparison problem."
        )
    });
    assert_eq!(
        got_canon,
        root_canon,
        "{SMOKE}: after the typed `:cd ..` the captive Neovim's working directory is {}, not the \
         expected parent {}. This is a PRECONDITION failure and it is not about the front-end at \
         all: nothing in `wolf-gnome` sits between the keystrokes and Neovim's own `:cd`. The \
         LAUNCH cwd here means the `:cd ..` never executed — the Ex line went somewhere else, or \
         an overlay held keyboard focus. A THIRD directory means the fixture nesting changed \
         underneath this smoke. Nothing below is about the re-root until this is green. PNG kept \
         at {}.",
        got_canon.display(),
        root_canon.display(),
        out_png.display()
    );

    let ocr_text = ocr_to_lowercase(&out_png);

    // Assertion 4 (discriminator): the window re-rooted on Neovim's cwd and the root auto-expanded.
    // See notes/crates/wolf-gnome/tests/smoke_headless/sidebar.rs.md#typed-cd-reverts
    assert!(
        ocr_text.contains(CD_OCR_ROOTED),
        "{SMOKE}: OCR did not contain {CD_OCR_ROOTED:?} — the name of {CD_TARGET_FILE}, which \
         exists ONLY inside the directory the `:cd` moved to and is therefore drawable only as a \
         child row under a re-rooted, EXPANDED synthetic root. Assertion 3 is GREEN, so Neovim \
         really is in that directory and this is the front-end's half. The candidate causes, in \
         order:\n\
         (a) THE WINDOW NEVER RE-ROOTED. `shell::settle_reroot` did not act on the live working \
         directory — this is the documented revert for this row, and it leaves assertion 3 green \
         exactly as you see it. Distinguish it from (b) using the OCR dump below: if \
         {CD_OCR_ROOT_NODE:?} is ALSO absent, the sidebar is still rooted at the launch cwd and \
         this is (a).\n\
         (b) THE RE-ROOT HAPPENED BUT THE ROOT DID NOT AUTO-EXPAND. `remove_all()` destroys the \
         root's `TreeListRow` and its expanded state, so the root must be re-expanded after EVERY \
         root-store rebuild; without that every child sits under a collapsed row and only the root \
         itself is drawn. If the dump below DOES contain {CD_OCR_ROOT_NODE:?} while this term is \
         missing, it is this.\n\
         (c) the re-root fired but the sidebar's FRESH `FileSearch` had not streamed its entries \
         when grim ran — a TIMING problem, fixed by raising CD_SCENARIO's two SETTLEs and its \
         TREE_SETTLE, never by weakening this assertion.\n\
         (d) the row rendered and OCR misread it — check the retained PNG before touching \
         anything.\n\
         Note the launch cwd's own files are expected to be GONE from the capture (the sidebar \
         re-roots rather than merging), while {CD_START_DIR:?} is expected to remain as a collapsed \
         Dir row. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 5 must precede 6: an unabbreviated title contains both terms and would pass 6 from the
    // header bar. Do not reorder or merge them; only a title regression fires this.
    assert!(
        !ocr_text.contains(CD_OCR_UNABBREVIATED),
        "{SMOKE}: OCR unexpectedly contained {CD_OCR_UNABBREVIATED:?} — the segment of the path \
         ABOVE the app's home directory. After the `:cd ..` the working directory IS `$HOME` (the \
         cage helper sets HOME to the launch cwd's parent), so `title::display_title` rule 4 must \
         render the bare `~` and no part of the absolute path may be on the page at all.\n\
         This is a PRECONDITION GUARD, not a feature assertion, and it is checked BEFORE assertion \
         6 on purpose: an unabbreviated title draws {CD_OUTER_DIR}/{CD_ROOT_DIR}, which CONTAINS \
         assertion 6's term — so a title regression would otherwise satisfy assertion 6 with no \
         synthetic root row on screen. Read this red as \"assertion 6 below cannot be trusted this \
         run\", not as a sidebar failure.\n\
         The likely causes: `workspace::home_dir()` returned None or a path that did not \
         canonicalize, so the shell injected an empty `home` and abbreviation is disabled \
         entirely; or `display_title`'s rule-4 equality (`cwd == normalized_home`) stopped matching \
         because one side is no longer canonical. PNG kept at {}. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // Assertion 6 (discriminator): the synthetic root node. With the title proved `~`, the root row is
    // the only source. See notes/crates/wolf-gnome/tests/smoke_headless/sidebar.rs.md#typed-cd-reverts
    assert!(
        ocr_text.contains(CD_OCR_ROOT_NODE),
        "{SMOKE}: OCR did not contain {CD_OCR_ROOT_NODE:?} — the basename of the working \
         directory, which the sidebar's SYNTHETIC ROOT ROW must draw. Assertion 4 is GREEN, so the \
         tree did re-root and is listing that directory's files; and assertion 5 is GREEN, so the \
         header bar is showing `~` and this word could not have come from the title. The candidate \
         causes, in order: (a) the root store still holds `children_of(entries, \"\")` directly \
         instead of a single `root_node(<label>)` — the documented revert for this row, and it \
         leaves assertion 4 green exactly as you see it; (b) the root node is installed but its \
         LABEL is wrong, which means `workspace::root_label` is not being passed the re-root \
         target (check the `Reroot` arm of `shell::settle_reroot`, not `tree_view`); (c) the root \
         store was left EMPTY, in which case assertion 4 would have failed first, so this cause is \
         already ruled out by the assertion above; (d) the row rendered and OCR misread it — check \
         the retained PNG first. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}
