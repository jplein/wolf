//! Input-method smokes: composed text reaching Neovim, and a control chord typed while a
//! compose sequence is pending.

use super::*;

// --- The composed-text smoke: dead keys reach Neovim only through `gtk::IMMulticontext`. ---

/// COMPOSE smoke: the file the scenario opens, so the buffer has a name the
/// precondition readback can report.
///
/// Its NAME is deliberately unlike every character [`COMPOSE_EXPECTED`] carries and
/// unlike every `ASSERTED_OCR_TERMS` entry — the `SAVE_FIXTURE_FILE` convention.
pub(crate) const COMPOSE_FIXTURE_FILE: &str = "cmpjuliet.md";

/// The whole content of [`COMPOSE_FIXTURE_FILE`] going in, which is what makes the
/// composed characters' later presence attributable to the input method and to
/// nothing else. Assertion 0 reads it back off disk rather than trusting this const.
const COMPOSE_FIXTURE_HEADING: &str = "# CMPFIXTUREHEAD";

/// The exact line the three compose sequences must produce: `dead_acute` then `e`,
/// `Multi_key` then `a` then `e`, and `Multi_key` then `s` then `s`.
///
/// Every character is non-ASCII and none is reachable from any key on the virtual
/// keyboard `wtype` drives, so nothing but a working compose path can put this
/// string in the buffer.
const COMPOSE_EXPECTED: &str = "éæß";

/// What the readback line reads when the input method is never offered the keys —
/// a DIAGNOSTIC AID quoted by assertion 3's message and asserted against by nothing.
///
/// Deleting the `filter_keypress` call in `window::wire_keys` routes every press
/// straight to `gdk_key`, which answers None for `dead_acute` and `Multi_key`
/// (neither has a Unicode value) and translates the five ordinary letters, so the
/// line reads `eaess`. Naming it here is what lets a red run be classified from the
/// failure message alone: this string means the OFFER was lost, an EMPTY line means
/// the commit never reached core.
const COMPOSE_UNCOMPOSED_READING: &str = "eaess";

/// COMPOSE smoke: the file the captive Neovim writes `bufname('%')` into BEFORE any
/// compose key is pressed.
///
/// A PRECONDITION, and it carries more weight here than in any sibling smoke: the
/// input method filters plain printables, so a broken commit path
/// breaks ALL typing rather than only accents, and every other symptom in a red run
/// would be a red herring. This readback is what says keys were reaching Neovim at
/// all when the compose sequences were driven.
///
/// Under `XDG_RUNTIME_DIR` rather than in the temp cwd, so no new sidebar row and no
/// new picker candidate appears mid-run. Registered in `RELAUNCH_FIXTURE_NAMES`
/// anyway, for `CD_READBACK_NAME`'s reason: its ABSOLUTE path is echoed into the
/// typed `:call writefile(...)` command line.
pub(crate) const COMPOSE_BEFORE_NAME: &str = "wolf_gnome_compose_before.txt";

/// COMPOSE smoke: the file the captive Neovim writes the composed LINE into. The
/// discriminator's only observable. Under `XDG_RUNTIME_DIR` for
/// [`COMPOSE_BEFORE_NAME`]'s reason, and registered for the same one.
pub(crate) const COMPOSE_READBACK_NAME: &str = "wolf_gnome_compose_line.txt";

/// The COMPOSED-TEXT scenario, run as a single `sh -c` string inside `cage` (the
/// `SAVE_SCENARIO` shape): launch, poll until `cage` is up, settle, `:e` the
/// fixture, read `bufname('%')` back, open a fresh line in insert mode, drive the
/// three compose sequences, read the line back, tear down.
///
/// ## What it must NOT contain, and why that is load-bearing
///
/// **No second route from the buffer to a file.** No `:w`, no `:saveas`, no quit
/// path that saves. The discriminator is a `writefile` of one line, and any other
/// producer of that file would satisfy it with the input method doing nothing.
///
/// **No `grim` and no OCR.** Nothing here is a claim about pixels, and the composed
/// glyphs would in any case be a poor OCR subject.
///
/// **No `getline('$')`.** This is a double-quoted shell string, so a `'$'` inside
/// one sits one edit away from being read as a parameter expansion — the reason
/// `CLIPBOARD_SCENARIO` records for avoiding it. The scenario opens its line with
/// `o` at the end of the buffer and never leaves it, so `getline('.')` names the
/// same line with no `$` anywhere.
///
/// ## One `wtype` invocation per key, and that is not stylistic
///
/// Each invocation uploads a fresh single-keycode keymap and releases it, so
/// separate invocations are what keep the compose state machine's input ordered and
/// unbatched. A sequence folded into one `wtype -- ae` would hand GTK two presses
/// whose ordering against the pending `Multi_key` is not the scenario's to state.
/// `COMPOSE_STEP` inside the script is the pause between two keys of one sequence:
/// a timing knob, with no assertion depending on it.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `24` — the `bufname('%')` readback never appeared, so keys were not reaching
///   Neovim at all and nothing below is diagnosable. Check the keyboard-only smokes
///   first.
/// * `25` — the composed-line readback never appeared.
const COMPOSE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for a readback file.
FILE_TRIES=100
# The pause between two keys of ONE compose sequence. A timing knob only: no
# assertion depends on it, and the sequences are client-side in GTK, so this is
# about keeping the keymap uploads ordered rather than about any budget.
COMPOSE_STEP=0.1

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
wait_ready "$app_pid" 11 "wolf-gnome compose smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there. (The SAVE_SCENARIO body, verbatim.)
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
    echo "wolf-gnome compose smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 1: a named buffer with known content --------------------------------
wtype -- ":e $COMPOSE_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: the PRECONDITION readback ----------------------------------------
#
# Proves ordinary keys reach Neovim at all, so a failure of the discriminator is
# attributable to the input method rather than to a lost keystroke. That matters
# more here than in any other smoke: the IM filters plain printables, so a broken
# commit path breaks ALL typing and every other symptom is a red herring.
wtype -- ":call writefile([bufname('%')], '$BEFORE')"
wtype -k Return
wait_file "$BEFORE" || exit 24

# --- Step 3: park at the end and open a fresh EMPTY line in insert mode --------
#
# The composed characters must land on a line of their own: the discriminator is an
# EQUALITY against that line, and sharing it with the heading would make the
# assertion a containment check in disguise.
wtype -- G
wtype -- o
sleep "$COMPOSE_STEP"

# --- Step 4: THE THREE COMPOSE SEQUENCES --------------------------------------
#
# One wtype invocation per key. `dead_acute` and `Multi_key` carry NO Unicode value,
# so `input::gdk_key` answers None for both: the only route they have into Neovim is
# the window's gtk::IMMulticontext, and the composed result arrives as a `commit`
# string rather than as a key press.
wtype -k dead_acute
sleep "$COMPOSE_STEP"
wtype -- e
sleep "$COMPOSE_STEP"

wtype -k Multi_key
sleep "$COMPOSE_STEP"
wtype -- a
sleep "$COMPOSE_STEP"
wtype -- e
sleep "$COMPOSE_STEP"

wtype -k Multi_key
sleep "$COMPOSE_STEP"
wtype -- s
sleep "$COMPOSE_STEP"
wtype -- s
sleep "$COMPOSE_STEP"

wtype -k Escape
sleep "$SETTLE"

# --- Step 5: THE DISCRIMINATOR readback ---------------------------------------
#
# `getline('.')` and not `getline('$')`: see the doc comment. The cursor has not
# left the line opened in step 3, so the two name the same line here.
wtype -- ":call writefile([getline('.')], '$AFTER')"
wtype -k Return
wait_file "$AFTER" || exit 25

exit 0
"#;

/// **A dead key and two compose sequences reach Neovim as composed characters.**
///
/// ## What it covers that nothing else does
///
/// The whole input-method path, end to end, in the only place all of it is joined:
///
/// * a `gtk::IMMulticontext` being ATTACHED to the window at all, with a client
///   widget and focused in — without which `filter_keypress` filters nothing;
/// * `window::wire_keys` offering each press to it before translating, which is what
///   `im_ui::key_route`'s `AskInputMethod` arm licenses;
/// * GTK's own compose table turning `dead_acute e` into `é`, `Multi_key a e` into
///   `æ` and `Multi_key s s` into `ß` (measured under `cage` with no xkb or
///   compose-key configuration in the compositor: the handling is client-side, so
///   the harness needs nothing added to it);
/// * `connect_commit` reaching `im_ui::commit_decision` and its `Send` events riding
///   the ordered input channel into `nvim_input` as text.
///
/// The headless tier, `crates/wolf-gnome/tests/im_commit_nvim.rs`, covers the
/// rulebook against a real Neovim and cannot reach any of the above: it constructs
/// no context, because there is no display.
///
/// ## Why the observable is a readback rather than a pixel
///
/// Two reasons, and the second is the load-bearing one. A composed glyph is a poor
/// OCR subject — `é` against `e` is exactly the discrimination tesseract is worst
/// at — and, more importantly, the claim is an EQUALITY over three characters.
/// `tesseract`'s output is a `contains` on a lowercased page, which could not tell a
/// doubled commit (`ééææßß`) or a reordered one from a correct line.
///
/// ## The regression evidence this smoke does NOT carry
///
/// The input method is measured to filter plain printables with no sequence
/// pending, so EVERY ordinary keystroke — every normal-mode command
/// key, every character of a typed `:e` line — travels the commit path rather than
/// the `gdk_key` path. This is not an accent feature bolted beside typing; it is a
/// rerouting of typing. What proves the rerouting is safe is the rest of this binary
/// staying green, and three groups carry the weight:
/// `window_renders_typed_text_under_cage` and
/// `ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage` for ordinary typing
/// (the second is the sharpest, being an exact-bytes disk assertion rather than an
/// OCR `contains`); the chord smokes for the accelerator namespace, which the
/// capture-phase controller keeps first refusal over; and the `<C-w>` split, focus
/// and drawer smokes for the chords Neovim wants for itself.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator
/// this smoke does not have.
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 0 | the fixture on disk carries none of [`COMPOSE_EXPECTED`]'s characters, and neither does its NAME | precondition — the evidence is not pre-satisfied by the fixture | none | n/a |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | RED under revert (b), and this is where that revert lands: `exit status: 24`, the `bufname('%')` readback never appearing at all |
/// | 2 | `$BEFORE` names [`COMPOSE_FIXTURE_FILE`] | precondition — the `:e` took effect and ordinary keys were reaching Neovim | none | GREEN under revert (a), which is what makes (a)'s reading attributable to the input method rather than to a lost keystroke |
/// | 3 | the readback line contains NO ASCII character | corroboration — any ASCII means at least one member of a sequence reached Neovim UNCOMPOSED, which tells a partly-working compose table from a wholly-broken one. It runs AHEAD of the equality, and that ordering is what makes it a check at all: behind the equality it could only ever inspect a line already proven equal to a wholly non-ASCII constant | (a) short-circuit the offer in `wire_keys`' press handler (`let filtered = false;` in place of the `current_event().is_some_and(…)` call), so every press goes straight to `Translate`: `gdk_key` answers None for `dead_acute` and `Multi_key`, so only the five ordinary letters survive and the line reads [`COMPOSE_UNCOMPOSED_READING`] | (a) RED HERE, in 5.32 s: `the composed line "eaess" contains an ASCII character, so at least one member of a compose sequence reached Neovim UNCOMPOSED — a base letter arriving as itself instead of the sequence's result`, with assertions 1 and 2 green. `"eaess"` is wholly ASCII, so revert (a) never reaches the equality below — this row is where the "was the key ever OFFERED" diagnosis is made |
/// | 4 | the readback line EQUALS [`COMPOSE_EXPECTED`] | **the discriminator** — the readings assertion 3 cannot see: an EMPTY line, a doubled one, a reordered one | (b) replace the `connect_commit` handler's send loop with `{}`. (c) drop every non-ASCII character in `commit_keys` (`c.is_ascii() && !c.is_control()`), which leaves ordinary typing untouched and sends the composed characters nowhere | (c) RED HERE in isolation, in 5.32 s: `the composed line is "", not "éæß"`, `left: ""` against `right: "éæß"`, with assertions 1, 2 and 3 all green — an empty line carries no ASCII either, which is exactly why this row is not redundant with the one above. (b) RED one assertion EARLIER, at assertion 1, in 23.05 s: measurement 3 made visible, since a commit handler that sends nothing breaks ALL typing, the `:e` and the `writefile` command lines included, so no readback is ever written. Sending each committed event TWICE reads the same way — `exit status: 24` in 23.00 s — so the DOUBLED reading this row's message names is caught by assertion 1 rather than here, and no revert produces it in isolation |
///
/// Equality rather than `contains` in assertion 4 is load-bearing: it is what makes
/// a doubled commit and a reordered one visible, which a containment check would
/// not.
#[test]
fn composed_text_reaches_neovim_under_cage() {
    const SMOKE: &str = "composed_text_reaches_neovim_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd holding the fixture plus the sidebar fillers. See
    // `make_temp_cwd` for why the isolation is mandatory rather than tidy.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[(COMPOSE_FIXTURE_FILE, COMPOSE_FIXTURE_HEADING)],
    );

    let fixture = cwd.join(COMPOSE_FIXTURE_FILE);
    let before = runtime_dir.join(COMPOSE_BEFORE_NAME);
    let readback = runtime_dir.join(COMPOSE_READBACK_NAME);
    clear_evidence(&before, SMOKE);
    clear_evidence(&readback, SMOKE);

    // Assertion 0 (precondition): a fixture already holding a composed character would pass assertion 3
    // with the input method idle. Read from the file, since the run reads the file.
    let seeded = std::fs::read_to_string(&fixture).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: could not read the fixture {} that `write_relaunch_fixtures` just wrote: {e}",
            fixture.display()
        )
    });
    for composed in COMPOSE_EXPECTED.chars() {
        assert!(
            !seeded.contains(composed),
            "{SMOKE}: the fixture {} already contains {composed:?} BEFORE the app runs, so \
             assertion 4 would be reading a character the fixture supplied rather than one the \
             input method composed — a permanent silent pass, not a flake. Its contents were \
             {seeded:?}. Change COMPOSE_FIXTURE_HEADING; do NOT change COMPOSE_EXPECTED and do \
             NOT weaken the assertion.",
            fixture.display()
        );
        assert!(
            !COMPOSE_FIXTURE_FILE.contains(composed),
            "{SMOKE}: the fixture NAME {COMPOSE_FIXTURE_FILE:?} contains {composed:?}. The tree \
             sidebar renders every cwd entry by name, so the character would be on screen \
             whatever the input method did — and while this smoke reads no pixel, the next one to \
             assert on this page would inherit the hazard. Keep the two deliberately unlike, as \
             the sibling smokes do."
        );
    }

    let status = cage_command(COMPOSE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("COMPOSE_FILE", COMPOSE_FIXTURE_FILE)
        .env("BEFORE", &before)
        .env("AFTER", &readback)
        .status()
        .expect("failed to spawn `cage` for the headless compose smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage compose scenario exited unsuccessfully ({status}). Read the exit code \
         FIRST.\n\
         Exit 11: the app died before it was ready for input — a LAUNCH failure, and nothing below \
         is about composing.\n\
         Exit 24: the `bufname('%')` readback never appeared, so keys were not reaching Neovim at \
         all. That is the loudest possible symptom of the input method going wrong, because it \
         filters plain printables: a commit handler that sends nothing breaks ALL typing, \
         not only accents. Check `window_renders_typed_text_under_cage` first.\n\
         Exit 25: the composed-line readback never appeared, so the `:call writefile(...)` never \
         ran — ordinary keys reached Neovim (exit 24 would have fired otherwise) but the cmdline \
         did not complete.\n\
         Any other status means wtype failed or the `:e` did not load. Readbacks expected at {} \
         and {}.",
        before.display(),
        readback.display()
    );

    // --- Assertion 2 (PRECONDITION, not a discriminator) ----------------------
    let before_name = read_trimmed(&before, SMOKE);
    assert_eq!(
        before_name,
        COMPOSE_FIXTURE_FILE,
        "{SMOKE}: before the compose sequences Neovim reported its current buffer as \
         {before_name:?}, not {COMPOSE_FIXTURE_FILE:?}. This is a PRECONDITION failure, not a \
         compose one: the `:e` never took effect, so the sequences were typed into some OTHER \
         buffer and assertion 4 is reading a line nothing was going to write. Readback kept at {}.",
        before.display()
    );

    // Assertion 3 (corroboration): ahead of the equality, since behind it the line is already proven
    // free of ASCII; here it names a compose member that arrived as its base letter.
    let line = read_trimmed(&readback, SMOKE);
    assert!(
        !line.chars().any(|c| c.is_ascii()),
        "{SMOKE}: the composed line {line:?} contains an ASCII character, so at least one member \
         of a compose sequence reached Neovim UNCOMPOSED — a base letter arriving as itself \
         instead of the sequence's result. Assertion 2 passed, so ordinary keys WERE reaching this \
         buffer. Classify the reading before changing anything:\n\
         Exactly {COMPOSE_UNCOMPOSED_READING:?} is the WHOLLY-broken form: nothing composed, so \
         the input method was never OFFERED the keys — `dead_acute` and `Multi_key` carry no \
         Unicode value, so `gdk_key` drops them and only the five ordinary letters survive. Look \
         for a missing `filter_keypress` call, a context that was never focused in or never given \
         a client widget, or a `key_route` answering Translate where AskInputMethod belongs.\n\
         A line MIXING ASCII with composed characters is the partly-working form: the offer and \
         the commit both work and GTK's compose table resolved some sequences and not others. \
         Telling those two apart is what this assertion is for. Readback kept at {}.",
        readback.display()
    );

    // --- Assertion 4 (THE DISCRIMINATOR) --------------------------------------
    assert_eq!(
        line,
        COMPOSE_EXPECTED,
        "{SMOKE}: the composed line is {line:?}, not {COMPOSE_EXPECTED:?}. Assertions 2 and 3 \
         passed, so ordinary keys WERE reaching this buffer and nothing in the line arrived \
         uncomposed. Classify the reading before changing anything:\n\
         An EMPTY line means the offer worked and the COMMIT did not: `connect_commit` unwired, \
         its handler refusing, or its send loop finding no core.\n\
         A DOUBLED or REORDERED line ({:?} would be the doubled form) means the commit reached \
         core more than once, or `commit_keys` lost the source order.\n\
         An UNCOMPOSED reading cannot reach this line at all — assertion 3 above fires on it \
         first, and its message is where that diagnosis lives. Readback kept at {}.",
        COMPOSE_EXPECTED.repeat(2),
        readback.display()
    );
}

// --- The composed-chord smoke: one chord, two answers; the control arm gives "no split" meaning.

/// COMPOSED-CHORD smoke: the file the scenario opens, so the buffer has a name the
/// precondition readback can report.
///
/// Deliberately unlike every `ASSERTED_OCR_TERMS` entry, the
/// [`COMPOSE_FIXTURE_FILE`] convention. Nothing here is OCRed, but the name is still
/// a sidebar row in this smoke's own capture-free run and would be page text for
/// anybody who later asserts on this page.
pub(crate) const IMCHORD_FIXTURE_FILE: &str = "imchordkilo.md";

/// The whole content of [`IMCHORD_FIXTURE_FILE`] going in. Nothing asserts on it: the
/// file exists so the buffer has a NAME, which is what assertion 2 reads back.
const IMCHORD_FIXTURE_HEADING: &str = "# IMCHORDFIXTUREHEAD";

/// COMPOSED-CHORD smoke: the file the captive Neovim writes `bufname('%')` into
/// before either arm runs.
///
/// A PRECONDITION shared with [`COMPOSE_BEFORE_NAME`] in purpose and in weight: the
/// input method filters plain printables, so a broken commit path breaks ALL typing,
/// and a window count that never changed would otherwise be indistinguishable from a
/// keystroke that never arrived.
///
/// Under `XDG_RUNTIME_DIR` rather than in the temp cwd, so no new sidebar row and no
/// new picker candidate appears mid-run. Registered in `RELAUNCH_FIXTURE_NAMES`
/// anyway, for `CD_READBACK_NAME`'s reason: its ABSOLUTE path is echoed into the
/// typed `:call writefile(...)` command line.
pub(crate) const IMCHORD_BEFORE_NAME: &str = "wolf_gnome_imchord_bufname.txt";

/// COMPOSED-CHORD smoke: `winnr('$')` BEFORE either arm — the launch's own window
/// count. Under `XDG_RUNTIME_DIR` for [`IMCHORD_BEFORE_NAME`]'s reason, and
/// registered for the same one.
///
/// It is what makes the control arm's reading a CHANGE rather than an absolute: two
/// windows after the chord says nothing on its own if the launch already had two.
pub(crate) const IMCHORD_START_NAME: &str = "wolf_gnome_imchord_wins_start.txt";

/// COMPOSED-CHORD smoke: `winnr('$')` after the CONTROL arm's `<C-w>v`.
pub(crate) const IMCHORD_SPLIT_NAME: &str = "wolf_gnome_imchord_wins_split.txt";

/// COMPOSED-CHORD smoke: `winnr('$')` after the `<C-w>c` that returns the window to
/// one pane, so the subject arm starts from the layout the control arm started from.
pub(crate) const IMCHORD_ONE_NAME: &str = "wolf_gnome_imchord_wins_one.txt";

/// COMPOSED-CHORD smoke: `winnr('$')` after the SUBJECT arm. The discriminator's only
/// observable.
pub(crate) const IMCHORD_AFTER_NAME: &str = "wolf_gnome_imchord_wins_after.txt";

/// The window count a fresh launch has, and the one the subject arm must still read.
const IMCHORD_WINDOWS_ONE: i64 = 1;

/// The window count `<C-w>v` must produce with nothing pending.
const IMCHORD_WINDOWS_SPLIT: i64 = 2;

/// The COMPOSED-CHORD scenario, run as a single `sh -c` string inside `cage` (the
/// [`COMPOSE_SCENARIO`] shape): launch, poll until `cage` is up, settle, `:e` the
/// fixture, read `bufname('%')` and `winnr('$')` back, drive `<C-w>v` with nothing
/// pending, read the count back, close the new pane, read the count back, press
/// `dead_acute` and drive the SAME `<C-w>v`, read the count back, tear down.
///
/// ## The two arms differ by exactly one keystroke
///
/// Both arms send `wtype -M ctrl -k w -m ctrl`, a `COMPOSE_STEP` pause, and
/// `wtype -k v` — the pinned form `VSPLIT_SCENARIO` uses, through the shared
/// `wv_chord` helper so the two cannot drift apart. The subject arm adds a leading
/// `wtype -k dead_acute` and nothing else. That is what makes a difference in the two
/// readings attributable to the pending compose sequence rather than to timing, to a
/// different chord form, or to which pane happened to be focused.
///
/// It also closes the obvious vacuity: a `dead_acute` that started no sequence at all
/// would make the subject arm IDENTICAL to the control arm, and this smoke would go
/// RED rather than silently green. That the key does start one is not assumed here
/// either — [`composed_text_reaches_neovim_under_cage`] composes `é` out of the same
/// keysym under the same compositor.
///
/// ## Why the readback is a window COUNT and never buffer text
///
/// A pending sequence leaves compose state behind, and what the `v` does with it is
/// GTK's business rather than this scenario's: an unmatched sequence may commit a
/// base character, a combining one, or nothing. Any of those can land in the buffer,
/// and one of them (`v` reaching normal mode as itself) enters visual mode. So the
/// buffer's TEXT is not a stable observable here, while `winnr('$')` is untouched by
/// every one of those outcomes.
///
/// Two `Escape`s follow the subject arm for the same reason, and both are needed:
/// `Escape` while a sequence is pending is claimed by the input method and aborts it,
/// so the FIRST may never reach Neovim. The second lands in a Neovim whose compose
/// state is clean and cancels whatever mode the arm left behind — a visual selection,
/// or a `<C-w>` still waiting for its argument — so the readback's `:` opens a
/// command line rather than becoming somebody's operand.
///
/// ## What it must NOT contain
///
/// **No second route from Neovim's layout to a file.** The four `read_windows` calls
/// are the only producers of the four count files.
///
/// **No `getline`, no `:w`, no `grim`, no OCR.** Nothing here is a claim about text
/// or pixels. `winnr('\$')` carries the one `$` in the file, escaped exactly as
/// `DRAWER_SCENARIO`'s `nvim_probe` escapes it, because this is a double-quoted shell
/// word and a bare `'$'` would be a parameter expansion.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `24` — the `bufname('%')` readback never appeared, so keys were not reaching
///   Neovim at all and nothing below is diagnosable. Check the keyboard-only smokes
///   first.
/// * `25` — the launch-time window count never appeared.
/// * `26` — the control arm's window count never appeared.
/// * `27` — the count after `<C-w>c` never appeared.
/// * `28` — the subject arm's window count never appeared. Since 24 through 27 all
///   passed, keys were reaching Neovim before the dead key: suspect a compose state
///   that outlived both `Escape`s and swallowed the readback's `:`.
const IMCHORD_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for a readback file.
FILE_TRIES=100
# The pause between two keys of ONE chord or ONE compose sequence. A timing knob
# only, and deliberately far below Neovim's 1000 ms 'timeoutlen': `<C-w>v` is a
# MAPPING in runtime/nvim/init.lua, so a gap past that would resolve the prefix to
# the built-in CTRL-W instead and change what the control arm is measuring.
COMPOSE_STEP=0.1

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
wait_ready "$app_pid" 11 "wolf-gnome composed-chord smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there. (The SAVE_SCENARIO body, verbatim.)
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
    echo "wolf-gnome composed-chord smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# read_windows <file> <exit-code>: write winnr('$') -- the current tabpage's window
# count -- into <file>. `winnr('\$')` is escaped for the double-quoted shell word,
# the DRAWER_SCENARIO `nvim_probe` form.
read_windows() {
    wtype -- ":call writefile([printf('%d', winnr('\$'))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

# wv_chord: <C-w>v, the VSPLIT_SCENARIO form (hold Ctrl, tap w, release Ctrl; then
# tap v). ONE definition for BOTH arms deliberately: the arms must differ by the
# leading dead key and by nothing else, or a difference in their readings would not
# be attributable to the compose state.
wv_chord() {
    wtype -M ctrl -k w -m ctrl
    sleep "$COMPOSE_STEP"
    wtype -k v
    sleep "$COMPOSE_STEP"
}

# --- Step 1: a named buffer ---------------------------------------------------
wtype -- ":e $IMCHORD_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: the PRECONDITION readbacks ---------------------------------------
#
# `bufname('%')` proves ordinary keys reach Neovim at all -- which matters more here
# than in most smokes, because the input method filters plain printables and a
# commit path that sends nothing breaks ALL typing. The window count that follows is
# what makes the control arm's answer a CHANGE rather than an absolute value.
wtype -- ":call writefile([bufname('%')], '$BUFNAME_OUT')"
wtype -k Return
wait_file "$BUFNAME_OUT" || exit 24

read_windows "$WINS_START" 25

# --- Step 3: THE CONTROL ARM (nothing pending) --------------------------------
wv_chord
sleep "$SETTLE"
read_windows "$WINS_SPLIT" 26

# --- Step 4: back to one window -----------------------------------------------
#
# <C-w>c closes the FOCUSED pane, which after a vsplit is the new one. init.lua's
# do_close is guarded against closing the last window, so on a run where step 3
# never split this is a no-op rather than a killed window -- a broken control arm
# stays a loud assertion instead of becoming a launch failure.
wtype -M ctrl -k w -m ctrl
sleep "$COMPOSE_STEP"
wtype -k c
sleep "$SETTLE"
read_windows "$WINS_ONE" 27

# --- Step 5: THE SUBJECT ARM (a compose sequence pending) ---------------------
#
# `dead_acute` carries no Unicode value, so `input::gdk_key` answers None for it and
# the window's gtk::IMMulticontext is its only route in. It commits nothing on its
# own: it leaves a sequence pending, and the SAME chord is driven into that state.
wtype -k dead_acute
sleep "$COMPOSE_STEP"
wv_chord
sleep "$SETTLE"

# Two Escapes, and both are load-bearing: the first may be claimed by the input
# method to abort a still-pending sequence, and the second cancels whatever mode the
# arm left in Neovim (a visual selection from a committed `v`, or a <C-w> awaiting
# its argument) so the readback's `:` opens a command line.
wtype -k Escape
sleep "$COMPOSE_STEP"
wtype -k Escape
sleep "$SETTLE"

read_windows "$WINS_AFTER" 28

exit 0
"#;

/// **A plain `Ctrl+<letter>` press while a compose sequence is pending does not
/// reach Neovim.**
///
/// This is the executable form of one bullet under "Present limitations" in
/// `notes/wolf-gnome-decisions.md`, the one beginning *"While a compose sequence
/// is pending, a plain `Ctrl+<letter>` press is claimed by the input method and does
/// not reach Neovim, and the sequence survives it"*. That sentence qualifies two
/// sentences of "The accelerator namespace" — the ones saying a disabled action's
/// accelerator falls through to Neovim and that plain `Ctrl+C` / `Ctrl+V` still reach
/// it — so it is the sharpest claim in that file, and until this smoke existed it
/// rested on a probe that has been deleted.
///
/// ## The two arms, and why one alone would prove nothing
///
/// One launch, one window, the same chord twice:
///
/// * **Control.** With nothing pending, `<C-w>v` must take the window from one pane
///   to two. Assertion 4.
/// * **Subject.** Back at one pane, `dead_acute` then the SAME `<C-w>v` must leave
///   the window at one pane. Assertion 6.
///
/// A smoke carrying only the subject arm would be green on a build where `<C-w>v`
/// splits nothing at all, where the chord never leaves `wtype`, or where the captive
/// Neovim is not accepting keys — three failures that have nothing to do with an
/// input method. The control arm is what excludes them, in the same process and the
/// same second, which is why its failure message says the run proves nothing rather
/// than describing a defect.
///
/// The reverse vacuity is closed by the arms' shape rather than by an assertion: they
/// differ by one keystroke, so a `dead_acute` that started no sequence would make the
/// subject arm a second control arm and turn assertion 6 red. See
/// [`IMCHORD_SCENARIO`], which also records why the observable is a window count and
/// never buffer text.
///
/// ## What a green run does NOT separate
///
/// The subject arm sends two presses into the pending state — `Ctrl+w` and then `v` —
/// and a reading of one window says only that at least one of them did not reach
/// Neovim. It cannot say which. `dead_acute` + `v` is not a sequence GTK's compose
/// table completes, so the `v` may be swallowed by the cancel whatever became of the
/// chord.
///
/// What that costs is precision about the MECHANISM, not about the effect: the effect
/// — a `<C-w>v` typed between a dead key and the key resolving it does not split — is
/// exactly what the `notes/wolf-gnome-decisions.md` bullet says a user pays, and it is what this smoke
/// pins. The recipe for sharpening it is cheap and is recorded rather than taken: send
/// a SECOND `v` after the first, since by then the unmatched sequence has been
/// cancelled, and a chord that reached Neovim would still be waiting for its argument
/// and would split on it. That leg owes a measurement first — that GTK cancels the
/// unmatched sequence rather than holding it, and passes the next key through — and
/// without it the extra `v` could be swallowed too and make the arm vacuous.
///
/// ## No assertion 0
///
/// The sibling non-OCR smokes open with a precondition that the fixture on disk does
/// not already carry the evidence. There is no such assertion here and no gap either:
/// the observable is an integer the captive Neovim computes about its own window
/// layout, which no file in the working directory can pre-satisfy. What the fixture
/// still owes is the ordinary name rule, and `RELAUNCH_FIXTURE_NAMES` is what
/// enforces it.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator
/// this smoke does not have.
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | GREEN in three runs: two on the shipped glue (10.07 s and 9.42 s wall) and one under revert (a), which also reached `exit 0` and reddened row 6 alone. No code from the 11 / 24-28 vocabulary was taken in any of them. |
/// | 2 | `$BUFNAME_OUT` names [`IMCHORD_FIXTURE_FILE`] | precondition — the `:e` took effect and ordinary keys were reaching Neovim | none | GREEN in all three: `imchordkilo.md`, on the shipped glue and under revert (a) alike. |
/// | 3 | `$WINS_START` is [`IMCHORD_WINDOWS_ONE`] | precondition — the launch has ONE pane, so assertion 4 reads a change | none | GREEN in all three: `1`. |
/// | 4 | `$WINS_SPLIT` is [`IMCHORD_WINDOWS_SPLIT`] | **the control arm** — not a discriminator of anything, and the thing that makes assertion 6 evidence | none | GREEN in all three: `2`, revert (a) included. The chord DOES split this window, in this launch, seconds before the subject arm drives the identical keys. |
/// | 5 | `$WINS_ONE` is [`IMCHORD_WINDOWS_ONE`] | precondition — the subject arm starts from the layout the control arm started from | none | GREEN in all three: `1`. |
/// | 6 | `$WINS_AFTER` is [`IMCHORD_WINDOWS_ONE`] | **the discriminator** | (a) short-circuit the offer in `window::wire_keys`' press handler (`let filtered = false;` in place of the `current_event().is_some_and(…)` call), so every press goes straight to `Translate`. PREDICTED: red HERE reading `2`, with assertions 1–5 green — `gdk_key` translates `Ctrl+w` and `v` perfectly well, and drops only the `dead_acute` it has no Unicode value for, so every readback still appears. (b) `im_ui::key_route`'s `Some(true) => Consumed` arm answering `Translate`, so a press the input method claimed is ALSO translated. PREDICTED: red EARLIER, at assertion 2 or as `exit status: 24` — that edit doubles every ordinary keystroke, so the typed `:e` line arrives as `::ee …` and no readback is written | GREEN on the shipped glue, reading `1`: **the claim reproduces.** With a `dead_acute` pending, the same `<C-w>v` that had just taken the window to two panes left it at one. Revert (a) was taken and went RED exactly where it was predicted to, in 10.07 s, with the message "after `dead_acute` and then the SAME `<C-w>v` the control arm split with, `winnr('$')` was 2, not 1." — `$WINS_AFTER` holding `2` while rows 1-5 stayed green (`imchordkilo.md`, `1`, `2`, `1`). So the offer to the input method, and nothing else in the press path, is what withholds the chord. Restored by sha256 and green again at `1` in 9.42 s. Revert (b) NOT run: (a) is isolated to this row, which is what (b) was there to fall back to. |
///
/// ## What will change this smoke, and it is not a regression
///
/// `notes/wolf-gnome-decisions.md` names one follow-up for the limitation above: refusing to offer a
/// press carrying `CONTROL_MASK` to the input method, which owes a measurement that
/// the pending sequence survives the refusal. The day that lands, the subject arm
/// splits and assertion 6 goes red **correctly**. The smoke, the `notes/wolf-gnome-decisions.md`
/// bullet and the two "The accelerator namespace" sentences move together in that
/// commit; assertion 6's expected value is not to be flipped on its own.
#[test]
fn a_control_chord_while_composing_is_claimed_by_the_input_method_under_cage() {
    const SMOKE: &str = "a_control_chord_while_composing_is_claimed_by_the_input_method_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd holding the fixture plus the sidebar fillers. See
    // `make_temp_cwd` for why the isolation is mandatory rather than tidy.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[(IMCHORD_FIXTURE_FILE, IMCHORD_FIXTURE_HEADING)],
    );

    let bufname_out = runtime_dir.join(IMCHORD_BEFORE_NAME);
    let wins_start = runtime_dir.join(IMCHORD_START_NAME);
    let wins_split = runtime_dir.join(IMCHORD_SPLIT_NAME);
    let wins_one = runtime_dir.join(IMCHORD_ONE_NAME);
    let wins_after = runtime_dir.join(IMCHORD_AFTER_NAME);

    // XDG_RUNTIME_DIR is not fresh per run, so a previous run's file would read as this run's evidence.
    for evidence in [
        &bufname_out,
        &wins_start,
        &wins_split,
        &wins_one,
        &wins_after,
    ] {
        clear_evidence(evidence, SMOKE);
    }

    let status = cage_command(IMCHORD_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("IMCHORD_FILE", IMCHORD_FIXTURE_FILE)
        .env("BUFNAME_OUT", &bufname_out)
        .env("WINS_START", &wins_start)
        .env("WINS_SPLIT", &wins_split)
        .env("WINS_ONE", &wins_one)
        .env("WINS_AFTER", &wins_after)
        .status()
        .expect("failed to spawn `cage` for the headless composed-chord smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage composed-chord scenario exited unsuccessfully ({status}). Read the exit \
         code FIRST.\n\
         Exit 11: the app died before it was ready for input — a LAUNCH failure, and nothing below \
         is about the input method.\n\
         Exit 24: the `bufname('%')` readback never appeared, so keys were not reaching Neovim at \
         all. That is the loudest possible symptom of the input method going wrong, because \
         the input method filters plain printables: a commit handler that sends nothing breaks ALL \
         typing, not only accents. Check `window_renders_typed_text_under_cage` first.\n\
         Exit 25, 26 or 27: one of the three `winnr('$')` readbacks before the dead key never \
         appeared. Ordinary keys were reaching Neovim (exit 24 would have fired otherwise), so \
         suspect the typed command line rather than the chord.\n\
         Exit 28: the SUBJECT arm's readback never appeared, and every readback before the dead \
         key did. The likely cause is a compose state that outlived both `Escape`s and swallowed \
         the readback's `:` — see the scenario's note on why there are two of them.\n\
         Any other status means wtype failed or the `:e` did not load. Readbacks expected at {}, \
         {}, {}, {} and {}.",
        bufname_out.display(),
        wins_start.display(),
        wins_split.display(),
        wins_one.display(),
        wins_after.display()
    );

    // --- Assertion 2 (PRECONDITION, not a discriminator) ----------------------
    let before_name = read_trimmed(&bufname_out, SMOKE);
    assert_eq!(
        before_name,
        IMCHORD_FIXTURE_FILE,
        "{SMOKE}: before either arm ran, Neovim reported its current buffer as {before_name:?}, \
         not {IMCHORD_FIXTURE_FILE:?}. This is a PRECONDITION failure, not a chord one: the `:e` \
         never took effect, so ordinary typing was already not arriving as typed and every window \
         count below is about some other run of events. Readback kept at {}.",
        bufname_out.display()
    );

    // Assertion 3 (precondition): one pane at launch, so assertion 4 measures a change.
    let start = read_ints(&wins_start, 1, SMOKE)[0];
    assert!(
        start == IMCHORD_WINDOWS_ONE,
        "{SMOKE}: at launch `winnr('$')` was {start}, not {IMCHORD_WINDOWS_ONE}. A fresh \
         `wolf-gnome` opens exactly one pane — it takes no CLI arguments, restores nothing (each \
         smoke gets its own WOLF_STATE_DIR) and `runtime/nvim/init.lua` splits nothing at \
         VimEnter — so this says the launch was not the one this smoke assumes. Every count below \
         is measured against this number, so nothing after it is diagnosable until it is \
         explained. Readback kept at {}.",
        wins_start.display()
    );

    // Assertion 4 (control arm): makes assertion 6 evidence rather than coincidence.
    let split = read_ints(&wins_split, 1, SMOKE)[0];
    assert!(
        split == IMCHORD_WINDOWS_SPLIT,
        "{SMOKE}: with NOTHING pending, `<C-w>v` left `winnr('$')` at {split}, not \
         {IMCHORD_WINDOWS_SPLIT}. **This is the control arm, and while it is red this smoke proves \
         nothing at all** — assertion 6 below expects ONE window, which a chord that never splits \
         anything satisfies for free, so a green run under a red control arm would be a permanent \
         silent pass rather than evidence about any input method. Classify the reading:\n\
         {IMCHORD_WINDOWS_ONE} means the chord did not reach Neovim, or reached it and did \
         nothing. Check `vertical_split_shows_two_panes_under_cage` and \
         `close_pane_leaves_one_under_cage` FIRST: if those are red too, this is not about the \
         input method — suspect the `<C-w>v` mapping in `runtime/nvim/init.lua`, the bubble-phase \
         key forwarder, or `wtype`'s modifier syntax.\n\
         More than {IMCHORD_WINDOWS_SPLIT} means the chord was delivered more than once, which is \
         the doubled-key signature: every OCR smoke would be reading doubled text as well.\n\
         Assertions 2 and 3 passed, so keys WERE reaching this buffer and the window started with \
         one pane. Readback kept at {}.",
        wins_split.display()
    );

    // --- Assertion 5 (PRECONDITION): back to one pane -------------------------
    let one = read_ints(&wins_one, 1, SMOKE)[0];
    assert!(
        one == IMCHORD_WINDOWS_ONE,
        "{SMOKE}: after `<C-w>c` closed the pane the control arm created, `winnr('$')` was {one}, \
         not {IMCHORD_WINDOWS_ONE}. The subject arm must start from the SAME layout the control \
         arm started from, or its reading is not comparable with the control's: at two windows a \
         subject reading of {IMCHORD_WINDOWS_SPLIT} would be ambiguous between \"the close failed\" \
         and \"the composed chord split\". Readback kept at {}.",
        wins_one.display()
    );

    // --- Assertion 6 (THE DISCRIMINATOR) --------------------------------------
    let after = read_ints(&wins_after, 1, SMOKE)[0];
    assert!(
        after == IMCHORD_WINDOWS_ONE,
        "{SMOKE}: after `dead_acute` and then the SAME `<C-w>v` the control arm split with, \
         `winnr('$')` was {after}, not {IMCHORD_WINDOWS_ONE}. Assertions 4 and 5 passed, so that \
         chord DOES split this window and the window was back at one pane when the dead key was \
         pressed. Classify the reading:\n\
         {IMCHORD_WINDOWS_SPLIT} means the chord and its argument BOTH reached Neovim while a \
         compose sequence was pending. **That is a genuine finding, not a broken test.** It makes \
         the bullet under \"Present limitations\" in `notes/wolf-gnome-decisions.md` — the one \
         beginning \"While a compose sequence is pending, a plain `Ctrl+<letter>` press is claimed \
         by the input method and does not reach Neovim, and the sequence survives it\" — FALSE \
         against the shipped glue, together with the qualifying clauses it puts on \"The \
         accelerator namespace\". The measurement behind that bullet came from a probe that has \
         been deleted, so this run is the better evidence: correct the document, and do NOT weaken \
         this assertion to agree with it.\n\
         More than {IMCHORD_WINDOWS_SPLIT} means something split twice, which neither arm can \
         produce — suspect a stray keystroke or a doubled key path.\n\
         One further reading to expect on purpose: the repair `notes/wolf-gnome-decisions.md` names as a follow-up \
         — refusing to offer a press carrying CONTROL_MASK to the input method — makes this \
         assertion read {IMCHORD_WINDOWS_SPLIT} CORRECTLY. If that is the change in flight, the \
         limitation bullet, the accelerator-namespace clauses and this expectation move together \
         in one commit. Readback kept at {}.",
        wins_after.display()
    );
}
