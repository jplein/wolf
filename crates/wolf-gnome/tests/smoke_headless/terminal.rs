//! Terminal smokes: the terminal drawer, terminals across a Neovim restart, terminal panes
//! beside Neovim panes, and typed keys reaching a pane's shell.

use super::config::FULLWIDTH_SMOKE_FONT_PT;
use super::panes::TABS_FIRST_TAB_CENTRE_FRACTION;
use super::recovery::with_nvim_pid_helper;
use super::*;

// The terminal smokes: three drawer regimes and one split pane; each readback follows the focus.
// See notes/crates/wolf-gnome/tests/smoke_headless/terminal.rs.md#terminal-drawer-smokes

/// Terminal-drawer smoke: the file `:e`d into the editor pane before the chord.
///
/// It exists for two separate assertions, and the second one is why its BODY is
/// sixteen lines rather than a bare heading.
///
/// **The persisted-layout leg.** `layout_snapshot` iterates `tree.leaves()`,
/// so the assertion is that the frozen `state.json` holds exactly one leaf and that
/// the leaf is THIS file. The fixture is what makes the second half of that
/// discriminating — and only the second half. Measured 10/10: with the
/// `:e` removed the layout still persists as one leaf naming `Untitled.md`, because
/// `persistable_layout` suppresses only an UNNAMED lone leaf and the app's default
/// buffer is named. So "one leaf, no terminal leaf" is NOT vacuous without a
/// fixture; "the leaf is the fixture's path" is.
///
/// **The band's placement (see [`drawer_band_rect`]).** Its body carries
/// [`DRAWER_OCR_EDITOR_ABSENT`] on every line, which the sampled band asserts is
/// ABSENT. In the shipped build the band lies inside the drawer strip, so no editor
/// row can reach it; with the drawer unpainted the editor fills the whole pane area
/// and its rows 10 and 11 land in exactly those pixels.
///
/// The NAME is deliberately unlike both asserted terms, per this binary's fixture
/// rule: `restore_fixture_names_cannot_satisfy_their_own_assertions` enforces that
/// mechanically, and its drawer block additionally checks this body.
pub(crate) const DRAWER_FIXTURE_FILE: &str = "drwbase.md";

/// The word every line of [`drawer_fixture_body`] carries.
///
/// All-caps and `ZZ`-prefixed like the other in-buffer markers (`NEWBUF_MARKER`,
/// `FOCUS_MARKER`) so it cannot collide with anything Neovim, the chrome or
/// another fixture puts on screen.
const DRAWER_FILL_LINE: &str = "ZZEDITORFILL";

/// Lowercased [`DRAWER_FILL_LINE`], asserted ABSENT from the sampled band.
///
/// **What it does and does not discriminate.** It is a BAND-PLACEMENT guard, not a
/// second discriminator for the drawer: an OCR misread satisfies a
/// must-not-contain assertion just as an empty band does. What it does catch, and
/// nothing else in this binary would, is the band landing in the EDITOR region through
/// [`drawer_band_rect`]'s derivation drifting. No code edit reaches it: every edit
/// that puts editor rows under the band fails the marker assertion first.
pub(crate) const DRAWER_OCR_EDITOR_ABSENT: &str = "zzeditorfill";

/// How many lines [`drawer_fixture_body`] writes.
///
/// DERIVED rather than measured, and the derivation is the point: the band spans
/// output rows `[strip_top + statusbar_h, 720 - (cell_h + 2))`, and with the drawer
/// unpainted the editor grid starts at the pane area's top plus its own statusbar and
/// padding and steps one `cell_h` per buffer line — which puts buffer lines 10 and 11
/// inside those rows at [`SMOKE_FONT_PT`]. Sixteen leaves five lines of headroom, so
/// the must-NOT-contain assertion still has something to find if a later chrome
/// change moves the grid down by a row or two, and no run has to be re-measured to
/// keep it honest.
///
/// It costs nothing in the GREEN build: the editor region an open drawer leaves is
/// two thirds of the pane area, about six rows, so lines 7 onward are never drawn.
pub(crate) const DRAWER_FIXTURE_LINE_COUNT: usize = 16;

/// Builds the contents of [`DRAWER_FIXTURE_FILE`]: [`DRAWER_FIXTURE_LINE_COUNT`]
/// lines, each a three-digit ordinal followed by [`DRAWER_FILL_LINE`].
///
/// The ordinal is a diagnostic, not an assertion: a failing band read prints its OCR
/// output, and the ordinal says WHICH buffer row the band was showing.
///
/// The trailing newline is [`write_relaunch_fixtures`]'s, so the file ends on a fill
/// line rather than on a blank one.
pub(crate) fn drawer_fixture_body() -> String {
    let mut body = String::new();
    for line in 1..=DRAWER_FIXTURE_LINE_COUNT {
        if line > 1 {
            body.push('\n');
        }
        body.push_str(&format!("{line:03}{DRAWER_FILL_LINE}"));
    }
    body
}

/// The marker the drawer's shell prints, and the ONLY thing the sampled band is
/// asserted to contain.
///
/// It is not a fixture name and no file is named after it, and it is never TYPED
/// either: [`DRAWER_MARKER_LINE`] spells it in two halves, so what puts it on
/// screen is the shell running that line, which no sidebar row, no `:e` toast, no
/// header-bar title and no echo of the typed text can do.
pub(crate) const DRAWER_MARKER: &str = "ZZDRAWERMARK";

/// Lowercased [`DRAWER_MARKER`] — what the band OCR must contain.
///
/// The band is one text row at [`SMOKE_FONT_PT`], so after [`DRAWER_MARKER_LINE`]
/// runs it reads the marker with the shell's [`TERMINAL_SMOKE_PS1`] prompt right
/// after it, `ZZDRAWERMARK$ `. `contains` is what the assertion uses, so the prompt
/// and the cursor cell beside it are harmless.
pub(crate) const DRAWER_OCR_MARKER: &str = "zzdrawermark";

/// The line typed into the drawer's shell to print [`DRAWER_MARKER`].
///
/// **Its shape is what keeps the marker on a ONE-ROW drawer.** At [`SMOKE_FONT_PT`]
/// the drawer holds one text row, so an `echo <MARKER>` + Return scrolls the marker
/// off as the next prompt is printed on a row of its own (a bare prompt band was
/// measured 6/6 when the drawer was a Neovim terminal). `printf` writes it with NO
/// trailing newline, so the prompt follows on the SAME row and the marker stays on
/// the last one, while the shell stays free to run the readbacks typed after it.
///
/// The two `%s` halves keep the marker out of the typed text, which the shell echoes
/// on the same row before Return: assertion 0 checks that.
const DRAWER_MARKER_LINE: &str = "printf '%s%s' ZZDRAWER MARK";

/// The prompt every terminal smoke's shell is given through `SMOKE_PS1`, which
/// [`READINESS_PRELUDE`] exports as `PS1`, with `ENV` removed so the shell reads no rc
/// file: the same on every host, and the same as the terminal-pane smoke's [`TP_PS1`].
const TERMINAL_SMOKE_PS1: &str = "$ ";

/// The shell every terminal smoke pins through `SHELL`. The drawer and a typed
/// `:terminal` run the default shell, so without it the host's login shell, its rc
/// files and its prompt would be on the page.
const TERMINAL_SMOKE_SHELL: &str = "/bin/sh";

/// Pins [`TERMINAL_SMOKE_SHELL`] and [`TERMINAL_SMOKE_PS1`] on a terminal smoke's
/// `cage` command, and removes `ENV`.
///
/// The prompt goes in as `SMOKE_PS1`, not `PS1`: the scenario runs under `sh -c`, and
/// where `/bin/sh` is bash, a non-interactive bash unsets an inherited `PS1` and never
/// exports it, so the app's shells would get bash's default prompt.
/// [`READINESS_PRELUDE`] exports it back as `PS1`.
fn pin_terminal_smoke_shell(cmd: &mut Command) -> &mut Command {
    cmd.env("SHELL", TERMINAL_SMOKE_SHELL)
        .env("SMOKE_PS1", TERMINAL_SMOKE_PS1)
        .env_remove("ENV")
}

/// The cmdline strip's ALLOCATED height beyond `cell_h`, in device pixels.
///
/// `pane_view::strip_height` REQUESTS `ceil(cell_h * CMDLINE_STRIP_ROWS)` as the
/// strip's content height; what GTK allocates it is two pixels more. That is a
/// widget-layer fact this binary does not model, so it is MEASURED rather than
/// derived: the whole band derivation, of which this is the only hand-supplied
/// term, was measured exact at 28, 16 and 10 pt. A term proportional to the font
/// would not hold at three sizes, which is what says two pixels is a fixed overhead
/// rather than a rounding of something.
///
/// It is load-bearing twice over: it sets the pane area's BOTTOM edge, and through
/// it the drawer strip's top edge, so an error here moves the whole sampled band.
const DRAWER_BAND_STRIP_EXTRA_PX: f64 = 2.0;

/// The rectangle of output pixels [`terminal_drawer_opens_and_is_not_persisted_under_cage`]
/// OCRs: the drawer strip's text band, BELOW its own statusbar and above the cmdline
/// strip.
///
/// ## Why a band and not the page
///
/// MEASURED, md5 over repeated runs: the band below the drawer's statusbar is
/// BYTE-IDENTICAL, 6/6 and 3/3, so it is the one region of the drawer this smoke
/// samples. The statusbar's text is `wolf_core::pane_status`'s answer for a terminal
/// pane, asserted in `crates/wolf-ffi/tests/pane_status.rs`, not here.
///
/// **The shell PROMPT does not vary per run.** [`pin_terminal_smoke_shell`] gives the
/// shell the constant [`TERMINAL_SMOKE_PS1`] and no rc file.
///
/// ## The derivation
///
/// Every term but [`DRAWER_BAND_STRIP_EXTRA_PX`] comes from a production constant or
/// a production function, so the band follows the app rather than restating it:
///
/// ```text
/// pane_area_h = CAGE_OUTPUT_H - OVERLAY_ORIGIN_Y - (cell_h + STRIP_EXTRA)
/// drawer_h    = drawer::drawer_split(pane_area_h, true, DRAWER_WEIGHT, min_h)
/// strip_top   = CAGE_OUTPUT_H - (cell_h + STRIP_EXTRA) - drawer_h
/// band        = [strip_top + statusbar_h, CAGE_OUTPUT_H - (cell_h + STRIP_EXTRA))
/// ```
///
/// with `statusbar_h` from `statusbar_layout` at `CellMetrics::size_px` and `min_h`
/// from `editor_inset::min_pane_size`, i.e. exactly what `apply_drawer_frame`
/// passes.
///
/// **A DISCLOSED discrepancy.** A measured run put the band at `y 560..674` at
/// [`SMOKE_FONT_PT`]; this derivation gives `y 557..674`, because it reads
/// `statusbar_layout(...).height` (92 px there) where that measurement implies 95. The
/// three pixels are the strip's own closing `STATUSBAR_VERTICAL_PADDING`, which
/// carries no ink — so the extra rows are background either way. The derivation is
/// kept rather than the recorded literal because it moves with the chrome; if the
/// band ever reads statusbar text, THIS is the paragraph to start from.
///
/// ## Why x is restricted to the pane area
///
/// A full-width crop at one text row contains no sidebar text, but only
/// INCIDENTALLY — which depends on [`SIDEBAR_FILLER_COUNT`] and the
/// tree's row height, neither of which this smoke controls. Starting at the pane
/// area's own left edge ([`OVERLAY_ORIGIN_X_EXPECTED`]) makes it STRUCTURAL: no
/// sidebar row can enter the sample however many files a later scenario writes. Every
/// pixel that removes is outside the drawer strip by construction, so it cannot take
/// any of the marker's ink with it.
///
/// The cmdline strip is BELOW the band's last row, so the typed
/// `:call writefile(…)` command lines — which spell the readback paths — are outside
/// the sample as well.
fn drawer_band_rect() -> SampleRect {
    let edges = drawer_strip_edges();
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    SampleRect {
        x0: pixel_index(OVERLAY_ORIGIN_X_EXPECTED, "the drawer band's first column"),
        x1: pixel_index(output_w, "the drawer band's last column"),
        y0: pixel_index(
            edges.strip_top + edges.statusbar_h,
            "the drawer band's first row",
        ),
        y1: pixel_index(edges.band_bottom, "the drawer band's last row"),
    }
}

/// The drawer strip's edges in output pixels, by [`drawer_band_rect`]'s derivation.
struct DrawerStripEdges {
    /// The strip's top edge: where the panes above it end.
    strip_top: f64,
    /// The height of the drawer's own statusbar, at the top of the strip.
    statusbar_h: f64,
    /// The strip's bottom edge, the cmdline strip's top.
    band_bottom: f64,
}

/// Derives [`DrawerStripEdges`]; [`drawer_band_rect`]'s doc gives the derivation.
fn drawer_strip_edges() -> DrawerStripEdges {
    let cell = cell_metrics_for_smoke();
    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");

    // The per-pane statusbar strip, measured the way `pane_view::build` measures it:
    // in DEVICE PIXELS, from `CellMetrics::size_px`.
    let statusbar = wolf_gnome::statusbar_layout::statusbar_layout(
        cell.size_px,
        wolf_gnome::statusbar_layout::secondary_font_size(cell.size_px),
        wolf_gnome::statusbar_layout::STATUSBAR_VERTICAL_PADDING,
        wolf_gnome::statusbar_layout::STATUSBAR_LINE_GAP,
    );

    let cmdline_strip_h = cell.cell_h + DRAWER_BAND_STRIP_EXTRA_PX;
    let pane_area_h = output_h - OVERLAY_ORIGIN_Y_EXPECTED - cmdline_strip_h;

    // The same floor `apply_drawer_frame` passes, from the same production function.
    let (_, min_h) = wolf_gnome::editor_inset::min_pane_size(
        cell.cell_w,
        cell.cell_h,
        statusbar.height,
        wolf_gnome::editor_inset::EDITOR_PADDING_PX,
    );
    let split = wolf_gnome::drawer::drawer_split(
        pane_area_h,
        true,
        wolf_gnome::drawer::DRAWER_WEIGHT,
        min_h,
    );
    assert!(
        split.drawer_height > 0.0,
        "drawer_strip_edges: `drawer::drawer_split` gave the drawer {} px of a {pane_area_h} px \
         pane area, so there is no strip to sample and the band below would be empty or inverted. \
         A zero here means the derived pane-area height is non-positive or non-finite, which \
         makes every number in this function meaningless — re-derive it from \
         CAGE_OUTPUT_H_EXPECTED and OVERLAY_ORIGIN_Y_EXPECTED rather than adjusting the band.",
        split.drawer_height
    );

    DrawerStripEdges {
        strip_top: output_h - cmdline_strip_h - split.drawer_height,
        statusbar_h: statusbar.height,
        band_bottom: output_h - cmdline_strip_h,
    }
}

/// The pane area ABOVE the drawer strip, from the pane area's top-left to the strip's
/// top edge, across the full width of the pane area: where the restart smoke's split
/// terminal pane lies once the drawer is open below it.
fn above_drawer_rect() -> SampleRect {
    let edges = drawer_strip_edges();
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    SampleRect {
        x0: pixel_index(OVERLAY_ORIGIN_X_EXPECTED, "the upper area's first column"),
        x1: pixel_index(output_w, "the upper area's last column"),
        y0: pixel_index(OVERLAY_ORIGIN_Y_EXPECTED, "the upper area's first row"),
        y1: pixel_index(edges.strip_top, "the upper area's last row"),
    }
}

/// S1: the file the captive Neovim writes `drawer=<n> wins=<n>` into after `<C-w>k`
/// out of the drawer — leg d, the half of the focus bridge where the drawer alone
/// would leave focus trapped.
///
/// All of S1's readbacks live under `XDG_RUNTIME_DIR` rather than in the smoke's
/// cwd, so the app's own `FileSearch` never lists them and no tree row bears their
/// names — the `FULLWIDTH_CAPPED_READBACK_NAME` treatment. They are registered in
/// `RELAUNCH_FIXTURE_NAMES` anyway, for `CD_READBACK_NAME`'s reason: their
/// ABSOLUTE paths are typed, into the cmdline strip or into the drawer's shell, in
/// the very window this smoke captures. The capture is taken before the first of
/// them is typed, so the registration is a tripwire against a later whole-page
/// assertion rather than a live hazard.
pub(crate) const DRAWER_UP_NAME: &str = "wolf_gnome_drawer_up.txt";

/// S1: the drawer's SHELL writes [`DRAWER_SHELL_READBACK`] here after `<C-w>j` back
/// INTO the drawer — leg e, the editor->down bridge. See [`DRAWER_UP_NAME`].
pub(crate) const DRAWER_DOWN_NAME: &str = "wolf_gnome_drawer_down.txt";

/// S1: Neovim's readback once the chord has HIDDEN the drawer again — leg h's
/// precondition. See [`DRAWER_UP_NAME`].
pub(crate) const DRAWER_HIDDEN_NAME: &str = "wolf_gnome_drawer_hidden.txt";

/// S1: Neovim's readback after the drawer is RE-OPENED and `<C-w>k` is pressed out of
/// it — leg h's discriminator. With `detect_closes`' drawer arm deleted it still
/// appears, and the scenario stops at leg f-vsplit. See [`DRAWER_UP_NAME`].
pub(crate) const DRAWER_REOPEN_NAME: &str = "wolf_gnome_drawer_reopen.txt";

/// S1: Neovim's readback after a `:vsplit` of an EDITOR pane — leg f's precondition,
/// that the split added a window, and the readback at which the falsifier of
/// `detect_closes`' drawer arm goes red (exit 14). See [`DRAWER_UP_NAME`].
pub(crate) const DRAWER_VSPLIT_WINS_NAME: &str = "wolf_gnome_drawer_vsplit_wins.txt";

/// S1: the drawer's shell writes here after that `:vsplit` and a `<C-w>j` — leg f,
/// the drawer's binding surviving an ordinary `rebuild_pane_area`. See
/// [`DRAWER_UP_NAME`].
pub(crate) const DRAWER_VSPLIT_NAME: &str = "wolf_gnome_drawer_vsplit.txt";

/// S1: the drawer's shell writes here after `<C-w>s` inside the drawer — leg g's
/// precondition, that focus stayed in the drawer. See [`DRAWER_UP_NAME`].
pub(crate) const DRAWER_SPLITCHORD_STAY_NAME: &str = "wolf_gnome_drawer_splitchord_stay.txt";

/// S1: Neovim's readback after `<C-w>s` inside the drawer and a `<C-w>k` out of it —
/// leg g, the split chord splitting nothing. See [`DRAWER_UP_NAME`].
pub(crate) const DRAWER_SPLITCHORD_NAME: &str = "wolf_gnome_drawer_splitchord.txt";

/// What the drawer's shell writes into a shell readback.
///
/// Only a shell can write it: typed into Neovim instead, `echo …` is a run of
/// Normal-mode keys, which writes no file.
const DRAWER_SHELL_READBACK: &str = "in-drawer";

/// The window count `winnr('$')` must report once the chord has HIDDEN the drawer:
/// the editor window alone.
///
/// **A PRECONDITION for leg h**: a count of two says the hiding press never arrived,
/// and leg h's discriminator would then re-prove leg d's bridge rather than the
/// re-open. It cannot discriminate the front-end arm leg h is about:
/// `wolf_toggle_terminal` closes the drawer window inside Neovim, so Neovim reports
/// one window whatever the front-end makes of the vanished grid.
const DRAWER_WINS_AFTER_HIDE: i64 = 1;

/// The window count `winnr('$')` reports with the drawer open beside one editor pane.
///
/// A PRECONDITION wherever it is asserted and never a discriminator: the chord runs
/// `_G.wolf_toggle_terminal(id)` inside Neovim, which builds the drawer window
/// whatever the front-end does with the intent.
const DRAWER_WINS_WITH_DRAWER: i64 = 2;

/// The TERMINAL-DRAWER scenario for
/// [`terminal_drawer_opens_and_is_not_persisted_under_cage`], run as one `sh -c`
/// string inside one `cage`.
///
/// ## Two kinds of readback, chosen by where focus should be
///
/// The front-end sends a terminal pane's keys to its session and an editor pane's to
/// Neovim, so a readback can only be typed where focus is. Where focus should be in
/// an EDITOR, `nvim_probe` types an Ex command writing `drawer=<w:wolf_drawer>
/// wins=<winnr('$')>`; where it should be in the DRAWER, `shell_probe` types an
/// `echo` the drawer's shell runs. A readback typed where focus is NOT never appears,
/// so a red focus leg surfaces as the scenario's exit code, naming the leg, rather
/// than as a wrong value: `14` for a Neovim readback the drawer's shell got, `15` for
/// a shell readback Neovim got.
///
/// ## Ordering that is load-bearing rather than tidy
///
/// **The capture is taken right after the marker line runs**, before any readback is
/// typed into the one-row drawer, which would scroll the marker off.
///
/// **`state.json` is frozen while the drawer is still open.** Persistence runs on
/// every render wake, so the live file is rewritten as the scenario goes on; the
/// copy is what pins the one state the assertion is about. The write is a tmp-file
/// rename, so a copy taken from a running app is never torn (the
/// `PREVIEW_SCENARIO` precedent).
///
/// **The hide/re-open pair (leg h) runs while exactly ONE editor pane exists**, above
/// the `:vsplit` rather than below it, and that placement is the leg rather than
/// housekeeping. The arm it falsifies — `detect_closes`'s drawer arm — matters
/// precisely because the generic loop that would otherwise handle the vanished drawer
/// grid `break`s on `tree.leaves().len() <= 1`, which is TRUE with a single editor
/// leaf and false with two. With the arm deleted, the red shows at leg f-vsplit: the
/// hidden drawer's pane is never unbound, and the `:vsplit`'s readback is typed into
/// a terminal pane.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `14` — a Neovim readback never appeared: either focus was in a terminal pane,
///   whose shell got the Ex command, or keys meant for the drawer's shell had gone
///   to Neovim earlier. Stderr names the leg.
/// * `15` — a shell readback never appeared: focus was in an editor pane, and
///   Neovim got the `echo` as Normal-mode keys. Stderr names the leg.
/// * `16` — no `state.json` existed to freeze while the drawer was open. DISTINCT
///   from "the persisted layout is wrong", which exits 0 and fails an assertion.
const DRAWER_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# The chord's round trip: the GTK accelerator, core starting the drawer's session,
# InputEvent::ToggleTerminal, the Lua helper's botright split, the drawer_open intent,
# apply_drawer_open, rebuild_pane_area and the session's resize. Twice SETTLE, matching
# the other multi-hop settles in this binary. The same budget covers a HIDE, whose
# front-end half is detect_closes plus a rebuild on the next render wake.
DRAWER_SETTLE=2.0
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
wait_ready "$app_pid" 11 "wolf-gnome terminal-drawer smoke"

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
    return 1
}

# $1 is the readback file, $2 the leg. Only an EDITOR pane's keys reach Neovim.
nvim_probe() {
    wtype -- ":call writefile([printf('drawer=%d wins=%d', get(w:, 'wolf_drawer', 0), winnr('\$'))], '$1')"
    wtype -k Return
    if ! wait_file "$1"; then
        echo "wolf-gnome terminal-drawer smoke: leg $2's Neovim readback $1 never appeared." >&2
        echo "  Focus was in a terminal pane, so its shell got the Ex command, or the drawer's keys had gone to Neovim earlier." >&2
        exit 14
    fi
}

# $1 is the readback file, $2 the leg. Only the DRAWER's keys reach its shell.
shell_probe() {
    wtype -- "echo $DRAWER_SHELL_READBACK > '$1'"
    wtype -k Return
    if ! wait_file "$1"; then
        echo "wolf-gnome terminal-drawer smoke: leg $2's shell readback $1 never appeared." >&2
        echo "  Focus was in an editor pane, so Neovim got the echo as Normal-mode keys." >&2
        exit 15
    fi
}

chord_w() {
    wtype -M ctrl -k w -m ctrl
    wtype -k "$1"
    sleep "$SETTLE"
}

# --- Step 1: the fixture, so a layout is persistable AS THIS FILE ------------
wtype -- ":e $DRAWER_FIXTURE"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: the chord under test -------------------------------------------
#
# Ctrl+` is `win.terminal`. The accelerator controller is on the WINDOW and in the
# capture phase, so it wins over the key forwarder whichever pane is focused.
wtype -M ctrl -k grave -m ctrl
sleep "$DRAWER_SETTLE"

# --- Step 3: the marker, printed by the drawer's shell ----------------------
wtype -- "$DRAWER_MARKER_LINE"
wtype -k Return
sleep "$SETTLE"

# --- Step 4: the capture -----------------------------------------------------
grim "$OUT_PNG"

# --- Step 5: freeze the persisted layout while the drawer is open -----------
if [ ! -s "$DRAWER_STATE_SRC" ]; then
    echo "wolf-gnome terminal-drawer smoke: no state.json at $DRAWER_STATE_SRC." >&2
    echo "  Nothing was persisted at all, so this is not about the drawer: suspect" >&2
    echo "  may_snapshot refusing every wake, open_state_store resolving no" >&2
    echo "  directory, or WOLF_STATE_DIR not reaching the app." >&2
    exit 16
fi
cp "$DRAWER_STATE_SRC" "$DRAWER_STATE_COPY"

# --- Step 6: leg d -- <C-w>k OUT of the drawer, through core's router -------
chord_w k
nvim_probe "$PROBE_UP" d

# --- Step 7: leg e -- <C-w>j back INTO the drawer, through the bridge -------
chord_w j
shell_probe "$PROBE_DOWN" e

# --- Step 8: leg h -- hide the drawer, re-open it, and <C-w>k out of it -----
#
# Hiding emits no intent at all (wolf_core::layout records why), so the vanished
# grid is the whole signal the front-end gets. This pair must stay above the
# :vsplit below; see DRAWER_SCENARIO's doc comment.
wtype -M ctrl -k grave -m ctrl
sleep "$DRAWER_SETTLE"
nvim_probe "$PROBE_HIDDEN" h-hidden

wtype -M ctrl -k grave -m ctrl
sleep "$DRAWER_SETTLE"
chord_w k
nvim_probe "$PROBE_REOPEN" h

# --- Step 9: leg f -- a :vsplit of an EDITOR pane, then into the drawer ------
wtype -- ":vsplit"
wtype -k Return
sleep "$SETTLE"
nvim_probe "$PROBE_VSPLIT_WINS" f-vsplit
chord_w j
shell_probe "$PROBE_VSPLIT" f

# --- Step 10: leg g -- <C-w>s INSIDE the drawer -----------------------------
#
# Core's router ignores split chords from the drawer, so focus stays put and no
# window is added; <C-w>k then lets Neovim count the windows.
wtype -M ctrl -k w -m ctrl
wtype -k s
sleep "$DRAWER_SETTLE"
shell_probe "$PROBE_SPLITCHORD_STAY" g-stay
chord_w k
nvim_probe "$PROBE_SPLITCHORD" g

# The trap tears the app down on the way out.
exit 0
"#;

/// The GTK terminal drawer opens on `<Control>grave` on a core terminal session,
/// paints its shell, keeps the focus bridge working in both directions, hides and
/// re-opens on the same chord, keeps its binding through a pane-area rebuild, is
/// never split by a split chord — and never reaches `state.json`.
///
/// ## What it is the only evidence for
///
/// No other automated test falsifies the intent arm, the pane binding, the structural
/// append, both halves of the focus bridge in a running window, the drawer's only
/// UNBIND path or the `state.json` exclusion.
///
/// ## The legs, and what each one's falsifier is
///
/// | leg | assertion | falsifying edit | measured red |
/// | --- | --- | --- | --- |
/// | b | the sampled band contains [`DRAWER_OCR_MARKER`] | (1) append the drawer host BEFORE the tree widget in `rebuild_pane_area`; (2) restore `LayoutIntent::DrawerOpen { .. } => {}` in `pane_view::apply_intent` | (1) no marker, and the band reads an editor row (`006zzeditorfill`); (2) exit 14 at leg d, before the band is read: no drawer pane is bound, so the drawer's keys go to Neovim |
/// | b' | the band does NOT contain [`DRAWER_OCR_EDITOR_ABSENT`] | none in code: a band-placement guard on [`drawer_band_rect`]'s derivation; every edit that puts editor rows in the band fails leg b first | n/a |
/// | c | the frozen `state.json` holds exactly one leaf, not a terminal leaf, and it is the fixture | (1) add the drawer to the persisted layout: chain `self.drawer.get().id()` onto `leaf_contents`' `tree.leaves()`, and make `layout_snapshot`'s layout a split of the tree's layout and a leaf for the drawer; (2) insert the drawer into the pane tree with `tree.split(..)` in `apply_drawer_open` | (1) the terminal-leaf check ("persists a TERMINAL leaf"); (2) exit 14 at leg h, before `state.json` is read |
/// | d | after `<C-w>k`, Neovim answers from a window that is not the drawer | in `pane_view::apply_focus`, call `tree.neighbor(origin, dir, ..)` in place of `drawer::focus_target` | exit 14 at leg d: focus never left the drawer, so its shell got the readback |
/// | e | after `<C-w>j`, the drawer's shell answers | delete the `dir == Down` bridge arm from `drawer::focus_target` | exit 15 at leg e: focus never left the editor |
/// | f | after a `:vsplit` added a window, `<C-w>j` still reaches the drawer's shell, so the drawer's binding survived the split's rebuild | `self.clear_drawer()` in `apply_split`, before its `rebuild_pane_area()` | exit 15 at leg f |
/// | g | after `<C-w>s` in the drawer, its shell still answers and Neovim counts no new window | delete the drawer guard in `wolf_core::terminal::TerminalRegistry::router_split` | one window more than after the `:vsplit`, or exit 14 at leg g |
/// | h | after the chord HIDES the drawer and another press re-opens it, `<C-w>k` out of the NEW drawer reaches an editor window | delete BOTH the `if let Some(drawer_id) = closed_drawer { … }` block and the loop's `.filter(…)` from `detect_closes` | exit 14 at leg f-vsplit: leg h's readbacks still appear, and the scenario stops at the `:vsplit`'s readback |
///
/// Leg f does not cover where the drawer strip sits on screen after a rebuild. Two
/// edits were measured green there and are not its falsifiers: dropping
/// `rebuild_pane_area`'s host-unparent loop, because the drawer host is a direct
/// child of `pane_area` and the `remove` loop detaches it anyway; and appending the
/// drawer host only for a one-leaf layout, because focus reaches the drawer through
/// `DrawerState` and `focus_win` wherever the strip is, and no capture follows the
/// `:vsplit`.
///
/// Leg g's front-end second line, `apply_split`'s drawer guard, is not reachable
/// from a key: core's router is what refuses the split, so the falsifier is core's.
///
/// **Leg h runs between legs e and f**, not after leg g — the scenario's step 8. It
/// is lettered last; the scenario's step comments are the authority on order.
///
/// ## Ordering
///
/// The legs are ordered and later ones presuppose earlier ones: leg e is typed from
/// wherever leg d left focus, leg h from wherever leg e did, and legs f and g from
/// wherever leg h did. The scenario stops at the first focus leg whose readback
/// never appears, and its exit code and stderr name it.
///
/// The exit status is asserted first, before the band, `state.json` or any readback
/// value, so an edit can go red at an earlier or later leg than its row's; the red
/// column records where each one was measured.
#[test]
fn terminal_drawer_opens_and_is_not_persisted_under_cage() {
    const SMOKE: &str = "terminal_drawer_opens_and_is_not_persisted_under_cage";

    // Assertion 0 (precondition): the marker must come from the shell, not from the typed line.
    assert!(
        !DRAWER_MARKER_LINE
            .to_lowercase()
            .contains(DRAWER_OCR_MARKER),
        "{SMOKE}: DRAWER_MARKER_LINE = {DRAWER_MARKER_LINE:?} contains the OCR term \
         {DRAWER_OCR_MARKER:?}, so the shell's echo of the typed line satisfies the band \
         assertion whether or not the shell ran it. Change the line; do NOT change the term."
    );

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd, mandatory: the captive Neovim's swap directory is keyed on it
    // and outlives the process.
    let cwd = make_temp_cwd();
    let fixture_body = drawer_fixture_body();
    write_relaunch_fixtures(SMOKE, &cwd, &[(DRAWER_FIXTURE_FILE, fixture_body.as_str())]);

    // The state dir `cage_command` hands the app, and the directory the scenario
    // freezes a copy into — a DIRECTORY, because `wolf_core::load_state` takes one.
    let smoke_state = smoke_state_dir(&cwd);
    let smoke_state_file = smoke_state.join(STATE_FILE_NAME);
    let frozen_state_dir = runtime_dir.join("wolf_gnome_drawer_state");
    std::fs::create_dir_all(&frozen_state_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the frozen-state dir {}: {e}",
            frozen_state_dir.display()
        )
    });
    let frozen_state_file = frozen_state_dir.join(STATE_FILE_NAME);

    let out_png = runtime_dir.join("wolf_gnome_drawer.png");
    let band_png = runtime_dir.join("wolf_gnome_drawer_band.png");
    let probe_up = runtime_dir.join(DRAWER_UP_NAME);
    let probe_down = runtime_dir.join(DRAWER_DOWN_NAME);
    let probe_hidden = runtime_dir.join(DRAWER_HIDDEN_NAME);
    let probe_reopen = runtime_dir.join(DRAWER_REOPEN_NAME);
    let probe_vsplit_wins = runtime_dir.join(DRAWER_VSPLIT_WINS_NAME);
    let probe_vsplit = runtime_dir.join(DRAWER_VSPLIT_NAME);
    let probe_splitchord_stay = runtime_dir.join(DRAWER_SPLITCHORD_STAY_NAME);
    let probe_splitchord = runtime_dir.join(DRAWER_SPLITCHORD_NAME);
    for path in [
        &out_png,
        &band_png,
        &probe_up,
        &probe_down,
        &probe_hidden,
        &probe_reopen,
        &probe_vsplit_wins,
        &probe_vsplit,
        &probe_splitchord_stay,
        &probe_splitchord,
        &frozen_state_file,
    ] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(DRAWER_SCENARIO, &cwd, &runtime_dir);
    pin_terminal_smoke_shell(&mut cmd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("DRAWER_FIXTURE", DRAWER_FIXTURE_FILE)
        .env("DRAWER_MARKER_LINE", DRAWER_MARKER_LINE)
        .env("DRAWER_SHELL_READBACK", DRAWER_SHELL_READBACK)
        .env("DRAWER_STATE_SRC", &smoke_state_file)
        .env("DRAWER_STATE_COPY", &frozen_state_file)
        .env("PROBE_UP", &probe_up)
        .env("PROBE_DOWN", &probe_down)
        .env("PROBE_HIDDEN", &probe_hidden)
        .env("PROBE_REOPEN", &probe_reopen)
        .env("PROBE_VSPLIT_WINS", &probe_vsplit_wins)
        .env("PROBE_VSPLIT", &probe_vsplit)
        .env("PROBE_SPLITCHORD_STAY", &probe_splitchord_stay)
        .env("PROBE_SPLITCHORD", &probe_splitchord);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless terminal-drawer scenario");

    // --- Assertion 1: the scenario ran to completion --------------------------
    assert!(
        status.success(),
        "{SMOKE}: the cage terminal-drawer scenario exited unsuccessfully ({status}); cage.log's \
         stderr names the leg. Exit 11: the app died before it was ready for input — a LAUNCH \
         failure, and nothing below is about the drawer. Exit 14: a Neovim readback never \
         appeared because focus was in a terminal pane — at leg d the drawer kept focus after \
         `<C-w>k` (`apply_focus` not resolving through `drawer::focus_target`), or no drawer \
         pane was bound at all (`apply_intent` dropping `LayoutIntent::DrawerOpen`), so the \
         drawer's keys had gone to Neovim; at leg h the drawer was made a pane-tree leaf \
         (`tree.split` in `apply_drawer_open`); at leg f-vsplit the hidden drawer's pane was \
         never unbound (`detect_closes`' drawer arm); at leg g `<C-w>s` split the drawer. Exit \
         15: a shell readback never appeared because focus was in an editor pane — at leg e the \
         bridge arm did not answer `<C-w>j`, at leg f the `:vsplit`'s split unbound the drawer, \
         at leg g-stay something moved focus out \
         of the drawer. Exit 16: the app persisted no state.json at all while the drawer was \
         open, which is `may_snapshot` refusing rather than anything about the drawer. Otherwise \
         wtype, grim or cp failed. Capture expected at {}.",
        out_png.display()
    );
    assert_png_present(SMOKE, &out_png);

    // Assertion 2 (leg b): the band is inside the drawer (see `drawer_band_rect`) and the shell
    // writes the marker, so no page text or fixture can draw it.
    let band = drawer_band_rect();
    let band_text = ocr_band_to_lowercase(&out_png, band, &band_png, SMOKE);
    assert!(
        band_text.contains(DRAWER_OCR_MARKER),
        "{SMOKE}: the drawer's text band did not contain {DRAWER_OCR_MARKER:?}. Three outcomes are \
         distinguishable from the OCR text below, and they have different first suspects. (a) It \
         carries {DRAWER_OCR_EDITOR_ABSENT:?} — the EDITOR is filling those rows, so the drawer \
         strip is not at the bottom of the pane area, and the first suspect is \
         `rebuild_pane_area` appending the drawer host BEFORE the tree widget (a dropped \
         `DrawerOpen` arm fails assertion 1 at leg d instead). (b) It carries a bare {TERMINAL_SMOKE_PS1:?} prompt and no \
         marker — the marker scrolled off, which means the typed line printed a newline (see \
         DRAWER_MARKER_LINE). (c) It is EMPTY — the band was sampled somewhere blank, in which \
         case re-read `drawer_band_rect`'s derivation, or the drawer strip is up but its canvas \
         painted no session. Full capture kept at {}, band crop at {}.\n\nBand OCR was:\n\
         {band_text}",
        out_png.display(),
        band_png.display()
    );

    // Assertion 3 (leg b'): a band-placement guard, not a discriminator, since a misread satisfies a
    // must-not-contain; it catches the band landing in the editor.
    assert!(
        !band_text.contains(DRAWER_OCR_EDITOR_ABSENT),
        "{SMOKE}: the drawer's text band contained {DRAWER_OCR_EDITOR_ABSENT:?} — the word every \
         line of {DRAWER_FIXTURE_FILE} carries. Assertion 2 has passed whenever this one runs, \
         so the band holds the drawer's marker too, and editor fill beside it means \
         `drawer_band_rect`'s derivation has drifted into the editor region. \
         Band crop kept at {}.\n\nBand OCR was:\n{band_text}",
        band_png.display()
    );

    // Assertion 4 (leg c): from the frozen copy. `layout_snapshot` iterates `tree.leaves()` and the
    // drawer is outside the tree; the falsifier changes that construction.
    let saved = load_state(&frozen_state_dir).unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::load_state could not parse the frozen state at {}. `load_state` \
             is silently-ignore, so a None means the file is absent, unreadable, malformed or \
             carries an unsupported version — the scenario already proved it was non-empty (it \
             exits 16 otherwise), so suspect the shape.",
            frozen_state_file.display()
        )
    });
    let window = saved.windows.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: the frozen state.json carries NO windows at all, so nothing was persisted. \
             File: {}",
            frozen_state_file.display()
        )
    });
    let tab = window.tabs.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: the persisted window carries NO tabs. `snapshot_state` writes exactly one. \
             File: {}",
            frozen_state_file.display()
        )
    });
    let layout = tab.layout.as_ref().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: the persisted tab carries NO layout at all. `persistable_layout` returns \
             `None` only for an UNNAMED lone leaf, and this smoke `:e`d {DRAWER_FIXTURE_FILE} \
             before the chord — so a `None` here means the fixture never opened, or the snapshot \
             gate refused every wake. File: {}",
            frozen_state_file.display()
        )
    });
    let mut leaves: Vec<(&str, bool)> = Vec::new();
    collect_leaves(layout, &mut leaves);

    // The terminal-leaf check first: it names the drawer-as-pane hazard alone; a count would mask it.
    assert!(
        leaves.iter().all(|(_, terminal)| !terminal),
        "{SMOKE}: the frozen state.json persists a TERMINAL leaf: {leaves:?} (path, terminal). The \
         drawer is never persisted (specs/terminal-panes.md, Session restore), so a terminal leaf \
         here means the drawer became a `PaneTree` leaf and `layout_snapshot` wrote it, and the \
         next launch would restore it as a split terminal pane. File: {}",
        frozen_state_file.display()
    );
    assert_eq!(
        leaves.len(),
        1,
        "{SMOKE}: the frozen state.json persists {} leaves, want exactly 1. The window held one \
         editor pane and the drawer at freeze time, so TWO leaves mean the drawer reached the \
         persisted layout — check that `leaf_contents` and `layout_snapshot` walk \
         `tree.leaves()` only. The leaves were {leaves:?}. File: {}",
        leaves.len(),
        frozen_state_file.display()
    );
    let tail = Path::new(leaves[0].0)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_else(|| {
            panic!(
                "{SMOKE}: the persisted leaf path {:?} has no final component, so it names no \
                 file and could not be reopened on the next launch. File: {}",
                leaves[0].0,
                frozen_state_file.display()
            )
        });
    assert_eq!(
        tail,
        DRAWER_FIXTURE_FILE,
        "{SMOKE}: the frozen state.json's single leaf names {tail:?}, want \
         {DRAWER_FIXTURE_FILE:?}. An `Untitled.md` here means the `:e` never landed, at which \
         point the leaf-count assertion above proves rather less than it appears to — measured \
         10/10, a drawer-open session with NO `:e` still persists one leaf, because \
         `persistable_layout` suppresses only an UNNAMED lone leaf and the default buffer is \
         named. The comparison is on the file NAME rather than the whole path because /tmp may \
         resolve through a symlink. File: {}",
        frozen_state_file.display()
    );

    // --- Assertion 5 (leg d): <C-w>k LEAVES the drawer ------------------------
    let (drawer, wins) = read_drawer_probe(&probe_up, SMOKE, "after <C-w>k out of the drawer");
    assert_eq!(
        (drawer, wins),
        (0, DRAWER_WINS_WITH_DRAWER),
        "{SMOKE}: after `<C-w>k` out of the drawer, Neovim's (w:wolf_drawer, window count) is \
         ({drawer}, {wins}), want (0, {DRAWER_WINS_WITH_DRAWER}). The readback appeared, so the \
         front-end moved its focus to the editor pane; `drawer=1` means it never sent `focus_win` \
         for it, so Neovim's current window is still the drawer and every current-window command \
         acts on the wrong window. A count other than {DRAWER_WINS_WITH_DRAWER} means the chord \
         never opened a drawer, or opened two."
    );

    // --- Assertion 6 (leg e): <C-w>j BRIDGES back into the drawer -------------
    let down = read_trimmed(&probe_down, SMOKE);
    assert_eq!(
        down, DRAWER_SHELL_READBACK,
        "{SMOKE}: after `<C-w>j` from the editor the readback reads {down:?}, want \
         {DRAWER_SHELL_READBACK:?} as written by the drawer's shell. The file appeared, so \
         something ran the `echo`; another content means the line was mangled on the way."
    );

    // Assertion 7 (leg h, precondition): the chord hid the drawer; a lost chord leaves two windows.
    let (drawer, wins) = read_drawer_probe(&probe_hidden, SMOKE, "after the chord hid the drawer");
    assert_eq!(
        (drawer, wins),
        (0, DRAWER_WINS_AFTER_HIDE),
        "{SMOKE}: after a second `<Control>grave` Neovim's (w:wolf_drawer, window count) is \
         ({drawer}, {wins}), want (0, {DRAWER_WINS_AFTER_HIDE}) — the editor window on its own. \
         TWO windows mean the drawer window is still open: the chord never arrived, or \
         `wolf_toggle_terminal` stopped finding the tabpage's `w:wolf_drawer` window. Both are \
         Neovim-side, and both make leg h below VACUOUS rather than red."
    );

    // --- Assertion 8 (leg h, THE DISCRIMINATOR): the re-opened drawer is BOUND again ---
    let (drawer, wins) = read_drawer_probe(
        &probe_reopen,
        SMOKE,
        "after the drawer was re-opened and <C-w>k pressed out of it",
    );
    assert_eq!(
        (drawer, wins),
        (0, DRAWER_WINS_WITH_DRAWER),
        "{SMOKE}: after the drawer was hidden and re-opened and `<C-w>k` pressed, Neovim's \
         (w:wolf_drawer, window count) is ({drawer}, {wins}), want (0, \
         {DRAWER_WINS_WITH_DRAWER}). ONE window means the third chord never arrived. `drawer=1` \
         with two windows is the FRONT-END half broken: Neovim re-opened the drawer, but the \
         front-end never sent `focus_win` for the editor window on `<C-w>k`, so Neovim's current \
         window is still the re-opened drawer. Suspect the re-open's binding in \
         `apply_drawer_open` and the `DrawerState` it records, which `<C-w>k` resolves through. \
         (Deleting `detect_closes`' drawer arm does not reach this assertion: that edit stops \
         the scenario at leg f-vsplit, exit 14, first.)"
    );

    // Assertion 9 (leg f): two live readbacks prove the `:vsplit` added a window; otherwise a dead
    // `cnoreabbrev` leaves the drawer check passing on an unchanged layout.
    let (drawer, wins_after_vsplit) =
        read_drawer_probe(&probe_vsplit_wins, SMOKE, "after a :vsplit of the editor");
    assert!(
        drawer == 0 && wins_after_vsplit > DRAWER_WINS_WITH_DRAWER,
        "{SMOKE}: the `:vsplit` added no window — Neovim reported {DRAWER_WINS_WITH_DRAWER} \
         before it and (w:wolf_drawer, count) ({drawer}, {wins_after_vsplit}) after. Nothing \
         rebuilt the pane area, so the assertion below would say nothing about the drawer \
         surviving one. The `:vsplit` reaches `wolf_do_split('v')` through a `cnoreabbrev` that \
         fires only on the WHOLE word, so a stray keystroke ahead of it is the first suspect."
    );
    let vsplit = read_trimmed(&probe_vsplit, SMOKE);
    assert_eq!(
        vsplit, DRAWER_SHELL_READBACK,
        "{SMOKE}: after a `:vsplit` of an editor pane and `<C-w>j`, the readback reads \
         {vsplit:?}, want the drawer shell's {DRAWER_SHELL_READBACK:?}. Red means the `:vsplit` \
         unbound the drawer, so `<C-w>j` from the editor found no drawer to bridge to."
    );

    // --- Assertion 10 (leg g): <C-w>s inside the drawer splits nothing -----------
    let stay = read_trimmed(&probe_splitchord_stay, SMOKE);
    assert_eq!(
        stay, DRAWER_SHELL_READBACK,
        "{SMOKE}: after `<C-w>s` inside the drawer the readback reads {stay:?}, want the drawer \
         shell's {DRAWER_SHELL_READBACK:?}: focus must stay in the drawer."
    );
    let (drawer, wins) = read_drawer_probe(
        &probe_splitchord,
        SMOKE,
        "after <C-w>s inside the drawer and <C-w>k out of it",
    );
    assert_eq!(
        (drawer, wins),
        (0, wins_after_vsplit),
        "{SMOKE}: after `<C-w>s` inside the drawer and `<C-w>k` out of it, Neovim's \
         (w:wolf_drawer, window count) is ({drawer}, {wins}), want (0, {wins_after_vsplit}), the \
         count before the chord. One window more means core's router SPLIT the drawer: the guard \
         in `TerminalRegistry::router_split` that ignores split chords from the drawer session is \
         gone, a second session was started, and its window is one the front-end cannot show, \
         since `apply_split` refuses a split of the drawer pane."
    );
}

/// Every leaf in a persisted layout, as `(path, terminal)`, in tree order.
///
/// A walker rather than a match on the root, because the assertion it feeds is about
/// the WHOLE tree: a drawer inside the pane tree would be a second child of a split,
/// and a root-only read would report the editor leaf and miss it entirely.
fn collect_leaves<'a>(node: &'a PaneNode, out: &mut Vec<(&'a str, bool)>) {
    match node {
        PaneNode::Split { children, .. } => {
            for child in children {
                collect_leaves(child, out);
            }
        }
        leaf => out.extend(leaf_fields(leaf).map(|leaf| (leaf.path, leaf.terminal))),
    }
}

/// Reads one `drawer=<n> wins=<n>` Neovim readback: `w:wolf_drawer` of Neovim's current
/// window, `0` when unset, and the tabpage's window count.
fn read_drawer_probe(path: &Path, smoke: &str, leg: &str) -> (i64, i64) {
    let raw = read_trimmed(path, smoke);
    let rest = raw.strip_prefix("drawer=").unwrap_or_else(|| {
        panic!(
            "{smoke}: the readback {leg} ({}) is {raw:?}, which does not start with `drawer=`. \
             Every Neovim probe in these smokes is one `printf('drawer=%d wins=%d', …)`, so a \
             different shape means the typed Ex command did not execute as written — a keystroke \
             landing in a buffer, or in a SHELL, instead of the cmdline.",
            path.display()
        )
    });
    let (drawer, wins) = rest.split_once(" wins=").unwrap_or_else(|| {
        panic!(
            "{smoke}: the readback {leg} ({}) is {raw:?}, which carries no ` wins=` separator; \
             the file was written by something other than this smoke's own probe.",
            path.display()
        )
    });
    (
        parse_readback_int(drawer, path, &raw, smoke),
        parse_readback_int(wins, path, &raw, smoke),
    )
}

/// Terminal-drawer FULL-WIDTH smoke: the file the captive Neovim writes its four
/// geometry integers into, from the editor pane.
///
/// Under `XDG_RUNTIME_DIR` rather than in the smoke's cwd, so no tree row bears its
/// name — belt-and-braces here, since this smoke captures no image at all.
pub(crate) const DRAWER_GEOMETRY_READBACK_NAME: &str = "wolf_gnome_drawer_geometry.txt";

/// Terminal-drawer FULL-WIDTH smoke: the file the drawer's shell writes its own
/// `stty size` into. See [`DRAWER_GEOMETRY_READBACK_NAME`] for where it lives.
pub(crate) const DRAWER_SESSION_SIZE_NAME: &str = "wolf_gnome_drawer_session_size.txt";

/// The FULL-WIDTH DRAWER scenario for
/// [`terminal_drawer_defaults_to_full_width_under_cage`], run as one `sh -c` string
/// inside one `cage`.
///
/// One launch, one chord, two readbacks, no `grim` — the trade
/// `column_cap_and_full_width_toggle_change_the_grid_under_cage` already makes, and
/// for the same reason: what this measures is a COLUMN COUNT, and tesseract's
/// `contains` is position- and count-agnostic by construction.
///
/// The drawer opens focused, so its shell reads its own size first; `<C-w>k` then
/// moves focus to the editor, where Neovim reads the rest in ONE `writefile`, so the
/// four describe one instant. `winwidth()` and `winheight()` are used rather than
/// `nvim_win_get_*`: under `ext_multigrid` the former report the GRID size the
/// front-end drives, the latter the layout size, which is blind to it. The drawer is
/// the `botright` split, so it is `winnr('$')`, which the third number confirms.
///
/// Exit codes: `11` the app died before it was ready for input; `12` the geometry
/// readback never appeared; `13` the session's size readback never appeared.
const DRAWER_FULLWIDTH_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
DRAWER_SETTLE=2.0
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
wait_ready "$app_pid" 11 "wolf-gnome drawer full-width smoke"

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
    echo "wolf-gnome drawer full-width smoke: readback file $f never appeared." >&2
    return 1
}

# --- Open the drawer; its shell writes the session's size --------------------
wtype -M ctrl -k grave -m ctrl
sleep "$DRAWER_SETTLE"
wtype -- "stty size > '$SESSION_SIZE'"
wtype -k Return
wait_file "$SESSION_SIZE" || exit 13

# --- Into the editor; Neovim writes the four numbers -------------------------
wtype -M ctrl -k w -m ctrl
wtype -k k
sleep "$SETTLE"
wtype -- ":call writefile([printf('%d %d %d %d', winwidth(0), winheight(winnr('\$')), getwinvar(winnr('\$'), 'wolf_drawer', 0), &lines)], '$GEOMETRY')"
wtype -k Return
wait_file "$GEOMETRY" || exit 12

exit 0
"#;

/// The terminal drawer defaults to FULL WIDTH — its session is wider than the
/// 90-column content cap every editor pane is held to — and its height is driven by
/// the front-end's own allocation rather than by Neovim's `floor(lines / 3)`.
///
/// ## Why it needs its own font size
///
/// `the_column_cap_cannot_bind_at_the_ocr_smokes_geometry` asserts, deliberately,
/// that at [`SMOKE_FONT_PT`] one pane is far too narrow for 90 columns — so at the
/// shared size the editor's `winwidth` is whatever the pane happens to be and leg b
/// below would FAIL (44, measured, against the cap's 90). At
/// [`FULLWIDTH_SMOKE_FONT_PT`] the cap binds with room to spare and the drawer's
/// uncapped width is a third again as wide (123 columns were measured for the drawer
/// window's grid), which is what makes both comparisons two-sided rather than a
/// rounding judgement.
///
/// ## The three legs
///
/// | leg | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | b | the editor's `winwidth(0) == editor_inset::MAX_GRID_COLS` | — (a PREMISE: it says the cap is in force, without which leg c compares nothing) | the editor is not capped here and this smoke is measuring the wrong regime |
/// | c | the drawer session's `stty size` columns `> MAX_GRID_COLS` | drop `b:wolf_full_width` from the drawer placeholder in `init.lua` | the session is exactly `MAX_GRID_COLS` wide: `sync_terminal_sizes` sizes it from the capped content area |
/// | d | the drawer window's `winheight < &lines / 3` | skip the drawer in `sync_pane_grids` / `pending_resize` | exactly `&lines / 3` — nvim is measured to re-divide only the grids the front-end has not sized |
///
/// Leg c observes, in the running application, that the full-width default ships in
/// `init.lua` alone (`b:wolf_full_width = true` on the drawer's placeholder) and rides
/// the ordinary `full_width_for_win` path this front-end already consumes. **If it
/// cannot pass without new front-end code, that is a finding to report rather than a
/// licence to widen the default.**
///
/// Leg d's strict inequality holds by construction: the front-end's row count
/// subtracts the statusbar strip and both padding gutters from a third of the pane
/// area before dividing by the cell height, so it is always at least one row short of
/// Neovim's own division.
#[test]
fn terminal_drawer_defaults_to_full_width_under_cage() {
    const SMOKE: &str = "terminal_drawer_defaults_to_full_width_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    let geometry = runtime_dir.join(DRAWER_GEOMETRY_READBACK_NAME);
    let session_size = runtime_dir.join(DRAWER_SESSION_SIZE_NAME);
    for path in [&geometry, &session_size] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(DRAWER_FULLWIDTH_SCENARIO, &cwd, &runtime_dir);
    pin_terminal_smoke_shell(&mut cmd)
        .env("WOLF_BIN", bin)
        .env("GEOMETRY", &geometry)
        .env("SESSION_SIZE", &session_size);

    // Chained after `cage_command` so it wins; the rest of the helper's environment stays.
    cmd.env("WOLF_FONT_PT", FULLWIDTH_SMOKE_FONT_PT);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless drawer full-width scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage drawer full-width scenario exited unsuccessfully ({status}). Exit 11: \
         the app died before it was ready for input — at {FULLWIDTH_SMOKE_FONT_PT} pt a panic in \
         the cell measurement or in `min_pane_size` is the first thing to look at. Exit 13: the \
         drawer's shell never wrote {}, so the chord opened no focused drawer session (the \
         `stty size` line went to Neovim). Exit 12: the geometry readback never appeared at {}, \
         so `<C-w>k` left focus in the drawer and its shell got the Ex command.",
        session_size.display(),
        geometry.display()
    );

    let size = read_ints(&session_size, 2, SMOKE);
    let (session_rows, session_cols) = (size[0], size[1]);
    let numbers = read_ints(&geometry, 4, SMOKE);
    let (editor_w, drawer_h, last_is_drawer, lines) =
        (numbers[0], numbers[1], numbers[2], numbers[3]);

    // --- Assertion 1 (precondition): real measurements of the right windows ---
    assert_eq!(
        last_is_drawer, 1,
        "{SMOKE}: Neovim's last window carries no `w:wolf_drawer`, so `winheight(winnr('$'))` \
         below measures some other window. The drawer is the `botright` split; without one the \
         chord never opened it."
    );
    for (label, value) in [
        ("the editor's winwidth(0)", editor_w),
        ("the drawer window's winheight", drawer_h),
        ("the drawer session's rows", session_rows),
        ("Neovim's &lines", lines),
    ] {
        assert!(
            value > 0,
            "{SMOKE}: {label} came back {value}. A non-positive number means the window or \
             session it describes was never sized, and every comparison below would be between \
             a real measurement and a placeholder."
        );
    }
    assert!(
        lines >= 3,
        "{SMOKE}: Neovim reports &lines = {lines}, so `&lines / 3` is not a meaningful third and \
         leg d below would compare against 0 or 1. The window is far too short, which means the \
         cage output size moved."
    );

    // Assertion 2 (leg b): the editor is capped (the production constant), so leg c means something.
    let expected_cap = i64::from(wolf_gnome::editor_inset::MAX_GRID_COLS);
    assert_eq!(
        editor_w, expected_cap,
        "{SMOKE}: the EDITOR window reports winwidth(0) = {editor_w}, want exactly {expected_cap} \
         (`editor_inset::MAX_GRID_COLS`). This is the premise leg c rests on: without the cap in \
         force, \"the drawer is wider than the cap\" says nothing about full width. A count well \
         ABOVE the cap means it is not being applied at all (check \
         `editor_inset::grid_size_for_pane`); a count BELOW it means the pane is narrower than the \
         cap at {FULLWIDTH_SMOKE_FONT_PT} pt, at which point the font size or the window size has \
         moved and this smoke is measuring nothing."
    );

    // --- Assertion 3 (leg c): the drawer's session is not capped ---------------
    assert!(
        session_cols > expected_cap,
        "{SMOKE}: the drawer's shell reports `stty size` columns = {session_cols}, which is not \
         more than the {expected_cap}-column cap the editor beside it is held to. EQUAL counts \
         are the signature of the drawer being sized as a capped pane: `sync_terminal_sizes` \
         takes the cap from the drawer pane's `full_width_for_win`, which `runtime/nvim/init.lua` \
         seeds with `b:wolf_full_width = true` on the drawer's placeholder. Nothing in this \
         front-end special-cases the drawer for width — if making this pass needs front-end \
         code, that is a FINDING about the full-width default, not a reason to widen it."
    );

    // --- Assertion 4 (leg d): the drawer's HEIGHT is front-end driven ---------
    let nvim_third = lines / 3;
    assert!(
        drawer_h < nvim_third,
        "{SMOKE}: the drawer window reports winheight = {drawer_h}, which is not less than \
         Neovim's own `&lines / 3` = {nvim_third} (&lines = {lines}; measured 8 against 14). \
         EQUAL heights mean the front-end never issued a `resize_grid` for the drawer, so the \
         size Neovim divided for itself at `botright <h>split` still stands — nvim is measured to \
         re-divide only the grids the front-end has NOT sized. The front-end's own count is \
         strictly smaller by construction. A GREATER height would mean the drawer was given more \
         than its third."
    );
}

/// S3: the file the split terminal pane's shell writes its pid into before the kill.
pub(crate) const TERMRESTART_PID_SPLIT_NAME: &str = "wolf_gnome_termrestart_pid_split.txt";

/// S3: the file the drawer's shell writes its pid into before the kill.
pub(crate) const TERMRESTART_PID_DRAWER_NAME: &str = "wolf_gnome_termrestart_pid_drawer.txt";

/// S3: the file the focused shell writes its pid into once the restored view is up.
pub(crate) const TERMRESTART_PID_AFTER_NAME: &str = "wolf_gnome_termrestart_pid_after.txt";

/// The marker the terminal pane's shell prints before the kill: after the restart it
/// can only be on screen if the same session was bound into a pane again.
const TERMRESTART_MARKER_PANE: &str = "ZZPANEKILO";

/// Lowercased [`TERMRESTART_MARKER_PANE`], which OCR must find ABOVE the drawer strip.
pub(crate) const TERMRESTART_OCR_PANE: &str = "zzpanekilo";

/// The line that prints [`TERMRESTART_MARKER_PANE`] on a row of its own.
///
/// The two `%s` halves keep the marker out of the typed line, which the shell echoes:
/// assertion 0 checks that. The trailing `echo` ends the row, because any resize of
/// the pane (the drawer opening below it, the restore's half-width split) makes
/// readline redraw its prompt row from column 0 and clear it, erasing a marker that
/// shares the row. It is `; echo` rather than a typed `\n`, so no backslash goes
/// through `wtype`.
const TERMRESTART_PANE_LINE: &str = "printf '%s%s' ZZPANE KILO; echo";

/// The marker the drawer's shell prints before the kill.
const TERMRESTART_MARKER_DRAWER: &str = "ZZDRAWERLIMA";

/// Lowercased [`TERMRESTART_MARKER_DRAWER`], which OCR must find in the drawer band.
pub(crate) const TERMRESTART_OCR_DRAWER: &str = "zzdrawerlima";

/// The line that prints [`TERMRESTART_MARKER_DRAWER`] in two halves, with no trailing
/// newline, as [`DRAWER_MARKER_LINE`] does and for its reasons. The drawer is the same
/// size before and after the restart, so no resize redraws its row.
const TERMRESTART_DRAWER_LINE: &str = "printf '%s%s' ZZDRAWER LIMA";

/// The TERMINAL RESTART scenario for [`terminals_survive_a_neovim_restart_under_cage`],
/// composed with [`with_nvim_pid_helper`], which supplies `wait_nvim_pid`,
/// `wait_nvim_gone`, `wait_nvim_respawn`, `assert_app_alive` and their exit codes.
///
/// A typed `:terminal` turns the editor window into a terminal pane and the chord
/// opens the drawer below it. Each shell writes its pid, then prints its marker: the
/// pane's sits on a row of its own above the prompt ([`TERMRESTART_PANE_LINE`]), and
/// the drawer's shares the drawer's one row with the prompt. Then Neovim is SIGKILLed and
/// nothing more is typed until the restore has had [`TERMRESTART_SCENARIO`]'s
/// `RESTORE_SETTLE`: the restart, the reopened files, the queued rebind of the split
/// session and the drawer's toggle all ride render wakes, with no key involved.
///
/// The readback after the capture is NON-fatal, so that a restore which put focus in
/// the wrong place still lets the Rust side read both captures first.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `13` — a shell's pid readback before the kill never appeared: typed `:terminal`
///   or the chord opened no focused session.
/// * `16` — THE APP DIED with its Neovim. See `FIND_NVIM_PID_HELPER`.
/// * `17` — the Neovim child could not be identified uniquely, so nothing was killed.
/// * `18` — the SIGKILLed Neovim never left the process table.
const TERMRESTART_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
DRAWER_SETTLE=2.0
FILE_TRIES=100
# How long the restore gets after the respawn before anything is read: about four times
# the measured kill-to-answer recovery (1.0 s), since no key may be typed to poll it.
RESTORE_SETTLE=4.0

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
wait_ready "$app_pid" 11 "wolf-gnome terminal restart smoke"

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
    echo "wolf-gnome terminal restart smoke: readback file $f never appeared." >&2
    return 1
}

# $1 is the readback file. Typed into the focused pane; only a shell writes it.
shell_pid() {
    wtype -- "echo \$\$ > '$1'"
    wtype -k Return
}

# --- Step 1: identify the Neovim child, exactly one -------------------------
wait_nvim_pid

# --- Step 2: a terminal pane, through the typed abbreviation ----------------
wtype -- ":terminal"
wtype -k Return
sleep "$DRAWER_SETTLE"
shell_pid "$PID_SPLIT"
wait_file "$PID_SPLIT" || exit 13
wtype -- "$PANE_LINE"
wtype -k Return
sleep "$SETTLE"

# --- Step 3: the drawer below it --------------------------------------------
wtype -M ctrl -k grave -m ctrl
sleep "$DRAWER_SETTLE"
shell_pid "$PID_DRAWER"
wait_file "$PID_DRAWER" || exit 13
wtype -- "$DRAWER_LINE"
wtype -k Return
sleep "$SETTLE"

# --- Step 4: kill the captive Neovim ----------------------------------------
kill -9 "$nvim_pid" 2>/dev/null || true
wait_nvim_gone
assert_app_alive

# --- Step 5: let the automatic restart and the restore run, untouched -------
wait_nvim_respawn || true
sleep "$RESTORE_SETTLE"
assert_app_alive

# --- Step 6: the capture, then a line typed into whatever has focus ---------
grim "$OUT_PNG"
shell_pid "$PID_AFTER"
wait_file "$PID_AFTER" || echo "  (the post-restore line reached no shell)" >&2

exit 0
"#;

/// Both kinds of terminal survive the captive Neovim being killed: after the
/// automatic restart, with no key pressed, the split terminal pane is back above the
/// drawer strip with its output, the drawer is back with its output, and a line typed
/// afterwards reaches the drawer's own shell.
///
/// ## What it is the only evidence for
///
/// The restart path's terminal restore on GTK: `shell.rs` capturing the selected
/// tab's drawer state, handing `terminal_restore(live − drawer_terminal(), …)` to the
/// tab after `reopen_captured`, and `pane_view` running each step from a render wake
/// once the first pane is bound — plus `reset_after_restart` clearing the drawer,
/// whose stale `is_open()` would otherwise refuse the drawer's return.
///
/// ## Assertions
///
/// | # | assertion | kind | falsifying edit | predicted red |
/// | --- | --- | --- | --- | --- |
/// | 0 | no typed line carries either OCR term, and each term is its marker lowercased | precondition | — | — |
/// | 1 | the scenario exits 0 and the capture exists | precondition | — | — |
/// | 2 | two distinct shells wrote their pids before the kill | precondition | — | — |
/// | 3 | OCR above the drawer strip finds [`TERMRESTART_OCR_PANE`] | discriminator | skip the `Split` steps where `pane_view` runs the restore | no pane marker: the split session is live but shown nowhere |
/// | 4 | OCR of the drawer band finds [`TERMRESTART_OCR_DRAWER`] | discriminator | skip the `Drawer` step; or drop `reset_after_restart`'s `DrawerState` clear | no drawer marker: no strip, or a strip the front-end never binds |
/// | 5 | the line typed after the restore was run by the drawer's shell, the same pid | discriminator | skip the `Drawer` step | the split pane's pid, or no readback when focus is in an editor |
#[test]
fn terminals_survive_a_neovim_restart_under_cage() {
    const SMOKE: &str = "terminals_survive_a_neovim_restart_under_cage";

    // Assertion 0 (precondition): each marker must come from its shell, not from the typed line.
    for (line, marker, term) in [
        (
            TERMRESTART_PANE_LINE,
            TERMRESTART_MARKER_PANE,
            TERMRESTART_OCR_PANE,
        ),
        (
            TERMRESTART_DRAWER_LINE,
            TERMRESTART_MARKER_DRAWER,
            TERMRESTART_OCR_DRAWER,
        ),
    ] {
        assert_eq!(
            marker.to_lowercase(),
            term,
            "{SMOKE}: the OCR term {term:?} is not the marker {marker:?} lowercased, so the OCR \
             assertion would look for text the shell never prints and fail even when the \
             terminal is restored. The OCR term must be the marker the shell prints."
        );
        assert!(
            !line.to_lowercase().contains(term),
            "{SMOKE}: the typed line {line:?} contains the OCR term {term:?}, so the shell's echo \
             of it satisfies the assertion whether or not the shell ran it. Change the line; do \
             NOT change the term."
        );
    }

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    let out_png = runtime_dir.join("wolf_gnome_termrestart.png");
    let pane_png = runtime_dir.join("wolf_gnome_termrestart_pane.png");
    let band_png = runtime_dir.join("wolf_gnome_termrestart_band.png");
    let pid_split = runtime_dir.join(TERMRESTART_PID_SPLIT_NAME);
    let pid_drawer = runtime_dir.join(TERMRESTART_PID_DRAWER_NAME);
    let pid_after = runtime_dir.join(TERMRESTART_PID_AFTER_NAME);
    for path in [
        &out_png,
        &pane_png,
        &band_png,
        &pid_split,
        &pid_drawer,
        &pid_after,
    ] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(
        &with_nvim_pid_helper(TERMRESTART_SCENARIO),
        &cwd,
        &runtime_dir,
    );
    pin_terminal_smoke_shell(&mut cmd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("PANE_LINE", TERMRESTART_PANE_LINE)
        .env("DRAWER_LINE", TERMRESTART_DRAWER_LINE)
        .env("PID_SPLIT", &pid_split)
        .env("PID_DRAWER", &pid_drawer)
        .env("PID_AFTER", &pid_after);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless terminal restart scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage terminal restart scenario exited unsuccessfully ({status}). Exit 11: \
         the app died before it was ready for input. Exit 13: a shell never wrote its pid before \
         the kill — typed `:terminal` opened no focused session (no abbreviation, or Neovim's own \
         terminal), or the chord opened no focused drawer; nothing below is about the restart. \
         Exit 16: THE APP DIED WITH ITS NEOVIM, which is this feature's worst outcome and must \
         never be read as a slow restart. Exit 17: the Neovim child could not be identified \
         uniquely, so nothing was killed. Exit 18: the SIGKILLed Neovim never left the process \
         table."
    );
    assert_png_present(SMOKE, &out_png);

    // --- Assertion 2 (precondition): two sessions existed before the kill -----
    let split = read_ints(&pid_split, 1, SMOKE)[0];
    let drawer = read_ints(&pid_drawer, 1, SMOKE)[0];
    assert_ne!(
        split, drawer,
        "{SMOKE}: the terminal pane's and the drawer's shells wrote the same pid {split}, so the \
         chord did not open a drawer of its own and the restore below has one session, not two."
    );

    // --- Assertion 3: the split session is shown above the drawer again -------
    let pane_text = ocr_band_to_lowercase(&out_png, above_drawer_rect(), &pane_png, SMOKE);
    assert!(
        pane_text.contains(TERMRESTART_OCR_PANE),
        "{SMOKE}: OCR of the pane area above the drawer strip does not contain \
         {TERMRESTART_OCR_PANE:?}, the marker the terminal pane's shell printed before the kill. \
         After the restart that session is live but has no window until the restore binds it \
         into a new split with `queue_bind_terminal`, so its absence means the `Split` steps \
         never ran (not handed to the tab, never consumed by a render wake, or consumed before \
         the first pane was bound and lost to it). Crop kept at {}, capture at {}.\n\nOCR was:\n\
         {pane_text}",
        pane_png.display(),
        out_png.display()
    );

    // --- Assertion 4: the drawer is back, on the same session -----------------
    let band_text = ocr_band_to_lowercase(&out_png, drawer_band_rect(), &band_png, SMOKE);
    assert!(
        band_text.contains(TERMRESTART_OCR_DRAWER),
        "{SMOKE}: the drawer band does not contain {TERMRESTART_OCR_DRAWER:?}, the marker the \
         drawer's shell printed before the kill. Either the restore never ran its `Drawer` step \
         (the drawer state was not captured before `perform_restart`, or the step was dropped), \
         or it ran and the front-end refused the drawer: `reset_after_restart` must clear \
         `DrawerState`, or `apply_drawer_open` returns early on a stale `is_open()` against the \
         dead Neovim's pane. Band crop kept at {}.\n\nBand OCR was:\n{band_text}",
        band_png.display()
    );

    // --- Assertion 5: a typed line reaches the drawer's own shell -------------
    assert!(
        pid_after.exists(),
        "{SMOKE}: the line typed after the restore reached no shell, so focus was in an editor \
         pane: the drawer the restore re-opens last should hold it. Readback expected at {}.",
        pid_after.display()
    );
    let after = read_ints(&pid_after, 1, SMOKE)[0];
    assert_eq!(
        after, drawer,
        "{SMOKE}: the line typed after the restore was run by pid {after}, not the drawer's \
         shell {drawer} (the terminal pane's is {split}). The split pane's pid means the restore \
         ran its `Split` step but not its `Drawer` step, which comes last and takes focus; \
         another pid means a NEW shell, so the drawer was re-opened on a fresh session instead of \
         the surviving one."
    );
}

/// S4: the file the terminal pane's shell writes its pid into right after `:terminal`.
pub(crate) const TERMFLOW_PID_OPENED_NAME: &str = "wolf_gnome_termflow_pid_opened.txt";

/// S4: the file Neovim writes into after `<C-w>h` out of the terminal pane.
pub(crate) const TERMFLOW_NVIM_NAME: &str = "wolf_gnome_termflow_nvim.txt";

/// S4: the file the focused shell writes its pid into after `<C-w>l` back in.
pub(crate) const TERMFLOW_PID_BACK_NAME: &str = "wolf_gnome_termflow_pid_back.txt";

/// S4: the file the focused shell writes its pid into after `<C-w>v` in the terminal
/// pane.
pub(crate) const TERMFLOW_PID_SPLIT_NAME: &str = "wolf_gnome_termflow_pid_split.txt";

/// The TERMINAL-PANE FLOW scenario for [`terminal_pane_splits_and_focus_under_cage`],
/// run as one `sh -c` string inside one `cage`.
///
/// Each step's readback is typed where focus should be: a shell readback writes the
/// shell's pid, a Neovim one writes `nvim <b:wolf_terminal_id or 0>` from Neovim's
/// current buffer. A readback typed where focus is not never appears, and the step's
/// own exit code names it.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `31` — no shell answered after `:terminal`: the keys went to Neovim.
/// * `32` — Neovim did not answer after `<C-w>h`: focus stayed in the terminal pane.
/// * `33` — no shell answered after `<C-w>l`: focus stayed in the editor.
/// * `34` — no shell answered after `<C-w>v`: focus is in no terminal pane.
const TERMFLOW_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
DRAWER_SETTLE=2.0
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
wait_ready "$app_pid" 11 "wolf-gnome terminal-pane flow smoke"

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
    echo "wolf-gnome terminal-pane flow smoke: readback file $f never appeared." >&2
    return 1
}

# $1 is the readback file, $2 the exit code. Only a shell writes it.
shell_pid() {
    wtype -- "echo \$\$ > '$1'"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

# $1 is the readback file, $2 the exit code. Only Neovim writes it.
nvim_mark() {
    wtype -- ":call writefile([printf('nvim %d', getbufvar('%', 'wolf_terminal_id', 0))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

chord_w() {
    wtype -M ctrl -k w -m ctrl
    wtype -k "$1"
}

# --- Step 1: a split, and a terminal pane in its new (right) window ---------
wtype -- ":vsplit"
wtype -k Return
sleep "$SETTLE"
wtype -- ":terminal"
wtype -k Return
sleep "$DRAWER_SETTLE"
shell_pid "$PID_OPENED" 31

# --- Step 2: <C-w>h out of it, through core's router ------------------------
chord_w h
sleep "$SETTLE"
nvim_mark "$NVIM_MARK" 32

# --- Step 3: <C-w>l back into it, through Neovim's pure intent --------------
chord_w l
sleep "$SETTLE"
shell_pid "$PID_BACK" 33

# --- Step 4: <C-w>v inside it: a new pane on a new session, focused ---------
chord_w v
sleep "$DRAWER_SETTLE"
shell_pid "$PID_SPLIT" 34

exit 0
"#;

/// A terminal pane opened by typing `:terminal` takes the keys, `<C-w>h` and `<C-w>l`
/// move focus out of it and back into the same shell, and `<C-w>v` inside it splits it
/// into a new pane on a new session, which takes focus.
///
/// It reads no pixel: every claim is about where keys go, which only the program that
/// receives them can report. The terminal-pane smoke above covers painting.
///
/// ## Assertions
///
/// | # | assertion | kind | falsifying edit | predicted red |
/// | --- | --- | --- | --- | --- |
/// | 1 | the scenario exits 0 | precondition | route keys to Neovim in `window.rs` even when a terminal pane is active | exit 31 |
/// | 2 | after `<C-w>h`, Neovim answers from a buffer that is no placeholder | discriminator | drop the `Route::Focus` intent push for the router's focus chords | exit 32: focus never leaves the terminal pane |
/// | 3 | after `<C-w>l`, the SAME shell answers | discriminator | route keys to Neovim in `window.rs` even when a terminal pane is active | exit 31 or 33 |
/// | 4 | after `<C-w>v`, a DIFFERENT shell answers | discriminator | delete `router_split`'s `terminal_bind` | exit 34, or the first shell's pid again |
#[test]
fn terminal_pane_splits_and_focus_under_cage() {
    const SMOKE: &str = "terminal_pane_splits_and_focus_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);

    let pid_opened = runtime_dir.join(TERMFLOW_PID_OPENED_NAME);
    let nvim_mark = runtime_dir.join(TERMFLOW_NVIM_NAME);
    let pid_back = runtime_dir.join(TERMFLOW_PID_BACK_NAME);
    let pid_split = runtime_dir.join(TERMFLOW_PID_SPLIT_NAME);
    for path in [&pid_opened, &nvim_mark, &pid_back, &pid_split] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(TERMFLOW_SCENARIO, &cwd, &runtime_dir);
    pin_terminal_smoke_shell(&mut cmd)
        .env("WOLF_BIN", bin)
        .env("PID_OPENED", &pid_opened)
        .env("NVIM_MARK", &nvim_mark)
        .env("PID_BACK", &pid_back)
        .env("PID_SPLIT", &pid_split);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless terminal-pane flow scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage terminal-pane flow scenario exited unsuccessfully ({status}). Exit 11: \
         the app died before it was ready for input. Exit 31: no shell answered after typed \
         `:terminal` — no session opened in the window (no abbreviation, or no \
         `wolf_terminal_open` handler), or the pane's keys still go to Neovim. Exit 32: Neovim \
         did not answer after `<C-w>h`, so focus never left the terminal pane: the router's \
         `FocusDir` intent is not applied. Exit 33: no shell answered after `<C-w>l`, so focus \
         never came back. Exit 34: no shell answered after `<C-w>v`, so the router's split \
         produced no focused terminal pane."
    );

    let opened = read_ints(&pid_opened, 1, SMOKE)[0];

    // --- Assertion 2: <C-w>h put Neovim on an editor buffer -------------------
    let mark = read_trimmed(&nvim_mark, SMOKE);
    assert_eq!(
        mark, "nvim 0",
        "{SMOKE}: after `<C-w>h` Neovim answered {mark:?}, want \"nvim 0\": the readback ran in \
         Neovim, so the front-end focused the editor pane, but a nonzero id means Neovim's \
         current buffer is still the terminal's placeholder — the front-end never sent \
         `focus_win` for the editor window."
    );

    // --- Assertion 3: <C-w>l came back to the same shell ----------------------
    let back = read_ints(&pid_back, 1, SMOKE)[0];
    assert_eq!(
        back, opened,
        "{SMOKE}: after `<C-w>l` the shell with pid {back} answered, not the terminal pane's \
         {opened}. Focus came back into a terminal, but a different one."
    );

    // --- Assertion 4: <C-w>v made a new session and focused it ----------------
    let split = read_ints(&pid_split, 1, SMOKE)[0];
    assert_ne!(
        split, opened,
        "{SMOKE}: after `<C-w>v` in the terminal pane the original shell (pid {opened}) answered \
         again, so focus stayed where it was: the router's split started no session in a new \
         pane, or the front-end never focused the pane its `Split` intent made."
    );
}

// Neovim panes beside terminals: the drawer, a split and a new tab, each open and then closed.

/// The file every editor pane of [`neovim_panes_stay_visible_beside_terminals_under_cage`]
/// shows.
///
/// Its name carries no asserted term, per this binary's fixture rule: [`NEAR_OCR_FILL`] is
/// its CONTENT, so the sidebar row, the statusbar strip and the `:e` echo that spell the
/// name can satisfy nothing. `restore_fixture_names_cannot_satisfy_their_own_assertions`
/// enforces that, and its neighbour-pane block checks the body.
pub(crate) const NEAR_FIXTURE_FILE: &str = "nbrbase.md";

/// The word every line of [`near_fixture_body`] carries.
///
/// All-caps and `ZZ`-prefixed like the other in-buffer markers ([`DRAWER_FILL_LINE`]) so
/// nothing Neovim, the chrome, a shell or another fixture puts on screen can spell it.
const NEAR_FILL_WORD: &str = "ZZNEARPANE";

/// Lowercased [`NEAR_FILL_WORD`]: what OCR must find in every capture.
///
/// Only an editor pane showing [`NEAR_FIXTURE_FILE`] draws it. Nothing typed spells it,
/// and the shells print only their prompt, their pid readbacks and the echo of those.
pub(crate) const NEAR_OCR_FILL: &str = "zznearpane";

/// How many lines [`near_fixture_body`] writes: more than the editor's rows at
/// [`SMOKE_FONT_PT`] with no drawer open, so every editor row a leg shows carries the word,
/// and one corrupted by the block cursor leaves others to read.
pub(crate) const NEAR_FIXTURE_LINE_COUNT: usize = 16;

/// Builds the contents of [`NEAR_FIXTURE_FILE`]: [`NEAR_FIXTURE_LINE_COUNT`] lines, each a
/// three-digit ordinal followed by [`NEAR_FILL_WORD`].
///
/// The ordinal is a diagnostic, as in [`drawer_fixture_body`]: a failing leg prints its OCR
/// text, and the ordinals say which buffer rows were read.
pub(crate) fn near_fixture_body() -> String {
    let mut body = String::new();
    for line in 1..=NEAR_FIXTURE_LINE_COUNT {
        if line > 1 {
            body.push('\n');
        }
        body.push_str(&format!("{line:03}{NEAR_FILL_WORD}"));
    }
    body
}

/// Fraction of the output WIDTH at which the SECOND tab's centre sits with two tabs open.
///
/// [`TABS_FIRST_TAB_CENTRE_FRACTION`] records the measurement: two pages split the
/// 1280 px bar evenly, with centres at x = 320 and x = 960. Three quarters is the second,
/// as far from the boundary between them as the first.
const NEAR_SECOND_TAB_CENTRE_FRACTION: f64 = 0.75;

/// Leg g1: the drawer's shell writes its pid here once the drawer is open. Every readback
/// of this smoke lives under `XDG_RUNTIME_DIR`, out of the sidebar, and is registered in
/// `RELAUNCH_FIXTURE_NAMES` anyway because its absolute path is typed on screen.
pub(crate) const NEAR_G1_PID_NAME: &str = "wolf_gnome_near_g1_pid.txt";

/// Leg g2: Neovim's `tabpagenr() winnr('$') b:wolf_terminal_id` once the drawer is hidden.
/// See [`NEAR_G1_PID_NAME`].
pub(crate) const NEAR_G2_NVIM_NAME: &str = "wolf_gnome_near_g2_nvim.txt";

/// Leg g3: the split terminal pane's shell writes its pid here. See [`NEAR_G1_PID_NAME`].
pub(crate) const NEAR_G3_PID_NAME: &str = "wolf_gnome_near_g3_pid.txt";

/// Leg g4: Neovim's readback after `<C-w>c` closed the split terminal pane. See
/// [`NEAR_G1_PID_NAME`].
pub(crate) const NEAR_G4_NVIM_NAME: &str = "wolf_gnome_near_g4_nvim.txt";

/// Leg g5: Neovim's readback right after Ctrl+Shift+T, before anything is opened in the
/// new tab. See [`NEAR_G1_PID_NAME`].
pub(crate) const NEAR_G5_TAB_NAME: &str = "wolf_gnome_near_g5_tab.txt";

/// Leg g5: the new tab's terminal pane's shell writes its pid here. See
/// [`NEAR_G1_PID_NAME`].
pub(crate) const NEAR_G5_PID_NAME: &str = "wolf_gnome_near_g5_pid.txt";

/// Leg g6: Neovim's readback after the click on the first tab. See [`NEAR_G1_PID_NAME`].
pub(crate) const NEAR_G6_NVIM_NAME: &str = "wolf_gnome_near_g6_nvim.txt";

/// Leg g7: the focused shell writes its pid here after the click on the second tab, before
/// `<C-w>c`. See [`NEAR_G1_PID_NAME`].
pub(crate) const NEAR_G7_PID_NAME: &str = "wolf_gnome_near_g7_pid.txt";

/// Leg g7: Neovim's readback after `<C-w>c` closed the second tab's terminal pane. See
/// [`NEAR_G1_PID_NAME`].
pub(crate) const NEAR_G7_NVIM_NAME: &str = "wolf_gnome_near_g7_nvim.txt";

/// The legs of [`neovim_panes_stay_visible_beside_terminals_under_cage`], in scenario order:
/// the name its capture's env var and file carry, and what is on screen when it is taken.
const NEAR_LEGS: [(&str, &str); 8] = [
    ("g0", "before any terminal"),
    ("g1", "the drawer open below the editor"),
    ("g2", "the drawer hidden again by the same chord"),
    ("g3", "a terminal split right of the editor"),
    ("g4", "after <C-w>c closed that terminal split"),
    ("g5", "a terminal split right of the editor in a new tab"),
    ("g6", "back in the first tab, by a click on its bar entry"),
    (
        "g7",
        "back in the second tab, after <C-w>c closed its terminal split",
    ),
];

/// Body of [`neovim_panes_stay_visible_beside_terminals_under_cage`], appended to
/// [`POINTER_PREAMBLE`].
///
/// One `grim` capture per leg of [`NEAR_LEGS`], each taken before that leg's readback is
/// typed. A readback typed where focus is not never appears, and its exit code names the
/// leg, so a step that did not happen cannot leave a later capture vacuously green.
///
/// The tab switches are clicks on the `adw::TabBar`, as in `POINTER_TABS_BODY`: no key
/// switches a tab, since `init.lua` maps Neovim's own tab chords to nothing.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// | code | leg | meaning |
/// | --- | --- | --- |
/// | `61` | g1 | no shell answered with the drawer open: the chord opened no focused drawer |
/// | `62` | g2 | Neovim did not answer once the drawer was hidden |
/// | `63` | g3 | no shell answered after `:vsplit` and `:terminal` |
/// | `64` | g4 | Neovim did not answer after `<C-w>c` in the terminal pane |
/// | `65` | g5 | Neovim did not answer after Ctrl+Shift+T |
/// | `66` | g5 | no shell answered after `:vsplit` and `:terminal` in the new tab |
/// | `67` | g6 | Neovim did not answer after the click on the first tab |
/// | `68` | g7 | no shell answered after the click on the second tab |
/// | `69` | g7 | Neovim did not answer after `<C-w>c` in the second tab's terminal pane |
const NEAR_BODY: &str = r#"
# The drawer chord's and a :terminal's round trip; see DRAWER_SCENARIO.
DRAWER_SETTLE=2.0
# The adw::TabBar animates its height in over about a second; see POINTER_TABS_BODY.
TABS_BAR_SETTLE=2.0

# $1 is the readback file, $2 the exit code. Only a shell writes it.
shell_probe() {
    wtype -- "echo \$\$ > '$1'"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

# $1 is the readback file, $2 the exit code. Only Neovim writes it.
nvim_probe() {
    wtype -- ":call writefile([printf('%d %d %d', tabpagenr(), winnr('\$'), getbufvar('%', 'wolf_terminal_id', 0))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

chord_w() {
    wtype -M ctrl -k w -m ctrl
    wtype -k "$1"
}

# $1 and $2 are a point on the tab bar.
click_tab() {
    hsend motion "$1" "$2"
    hsend press "$BTN_LEFT"
    hsend release "$BTN_LEFT"
    sleep "$SETTLE"
}

# --- g0: the fixture, before any terminal -----------------------------------
wtype -- ":e $NEAR_FIXTURE"
wtype -k Return
sleep "$SETTLE"
# Park the block cursor on a new line of its own: it corrupts the cell it covers.
wtype -- o
wtype -k Escape
sleep "$SETTLE"
grim "$OUT_G0"

# --- g1: the drawer open ------------------------------------------------------
wtype -M ctrl -k grave -m ctrl
sleep "$DRAWER_SETTLE"
grim "$OUT_G1"
shell_probe "$NEAR_G1_PID" 61

# --- g2: the drawer hidden by the same chord; its session stays live ----------
wtype -M ctrl -k grave -m ctrl
sleep "$DRAWER_SETTLE"
grim "$OUT_G2"
nvim_probe "$NEAR_G2_NVIM" 62

# --- g3: a split, and a terminal pane in its new (right) window ---------------
wtype -- ":vsplit"
wtype -k Return
sleep "$SETTLE"
wtype -- ":terminal"
wtype -k Return
sleep "$DRAWER_SETTLE"
grim "$OUT_G3"
shell_probe "$NEAR_G3_PID" 63

# --- g4: <C-w>c in the terminal pane, through core's router -------------------
chord_w c
sleep "$DRAWER_SETTLE"
grim "$OUT_G4"
nvim_probe "$NEAR_G4_NVIM" 64

# --- g5: a new tab, the fixture, a split and a terminal pane ------------------
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
nvim_probe "$NEAR_G5_TAB" 65
wtype -- ":e $NEAR_FIXTURE"
wtype -k Return
sleep "$SETTLE"
wtype -- ":vsplit"
wtype -k Return
sleep "$SETTLE"
wtype -- ":terminal"
wtype -k Return
sleep "$DRAWER_SETTLE"
grim "$OUT_G5"
shell_probe "$NEAR_G5_PID" 66

# --- g6: back to the first tab ------------------------------------------------
click_tab "$NEAR_TAB1_X" "$NEAR_TAB_Y"
grim "$OUT_G6"
nvim_probe "$NEAR_G6_NVIM" 67

# --- g7: back to the second tab, and <C-w>c in its terminal pane --------------
click_tab "$NEAR_TAB2_X" "$NEAR_TAB_Y"
shell_probe "$NEAR_G7_PID" 68
chord_w c
sleep "$DRAWER_SETTLE"
grim "$OUT_G7"
nvim_probe "$NEAR_G7_NVIM" 69

hsend quit
exit 0
"#;

/// Neovim panes stay drawn beside every kind of terminal pane, and where a closed one was:
/// the drawer, a terminal split, and a terminal split in a second tab, each while it is
/// open and again once it is hidden or closed.
///
/// ## What it is evidence for
///
/// A terminal pane that paints over its neighbours, or a front-end that stops painting
/// the editor grid while any session is live, hides the editor's text with every terminal
/// smoke still green: they read the terminal's own output, never the editor beside it.
/// The macOS front-end's `TerminalPaneView` did the first. Each GTK pane is its own
/// `DrawingArea`, whose draw is clipped to its allocation, so this smoke is expected
/// GREEN on the GTK front-end and is the evidence that it does not show that defect.
///
/// Every capture is OCR'd whole and one assertion names every leg missing
/// [`NEAR_OCR_FILL`], so a single run shows each red leg. The scenario's readbacks are
/// preconditions: each says its leg's step happened, so no capture is green because a
/// terminal never opened.
///
/// ## Assertions
///
/// | # | assertion | kind | falsifying edit | predicted red |
/// | --- | --- | --- | --- | --- |
/// | 1 | the scenario exits 0 | precondition | none | n/a |
/// | 2 | eight captures, both click points inside the reported output | precondition | none | n/a |
/// | 3 | the Neovim readbacks name one editor window in the expected tab, and the shell pids say a new session answered in g3 and g5 and the second tab's own in g7 | precondition | none | n/a |
/// | 4 | capture g0 carries [`NEAR_OCR_FILL`] | precondition: OCR reads the fixture with no terminal open | none | n/a |
/// | 5 | captures g1 to g7 each carry [`NEAR_OCR_FILL`] | **discriminator** | in `PaneInner::draw_pane`, skip the editor grid branch while `core.live_terminals()` is not empty | g1 to g7: the hidden drawer's session stays live through every later leg |
#[test]
fn neovim_panes_stay_visible_beside_terminals_under_cage() {
    const SMOKE: &str = "neovim_panes_stay_visible_beside_terminals_under_cage";
    // The inclusive band of exit codes NEAR_BODY adds to POINTER_PREAMBLE's 11-15.
    const NEAR_EXIT_FIRST: i32 = 61;
    const NEAR_EXIT_LAST: i32 = 69;
    // A Neovim readback's `tabpagenr() winnr('$') b:wolf_terminal_id`: one editor window.
    const ONE_EDITOR_IN_TAB_1: [i64; 3] = [1, 1, 0];
    const ONE_EDITOR_IN_TAB_2: [i64; 3] = [2, 1, 0];

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let fixture_body = near_fixture_body();
    write_relaunch_fixtures(SMOKE, &cwd, &[(NEAR_FIXTURE_FILE, fixture_body.as_str())]);

    let captures: Vec<(&str, &str, PathBuf)> = NEAR_LEGS
        .iter()
        .map(|&(leg, what)| {
            (
                leg,
                what,
                runtime_dir.join(format!("wolf_gnome_near_{leg}.png")),
            )
        })
        .collect();
    let pid_g1 = runtime_dir.join(NEAR_G1_PID_NAME);
    let nvim_g2 = runtime_dir.join(NEAR_G2_NVIM_NAME);
    let pid_g3 = runtime_dir.join(NEAR_G3_PID_NAME);
    let nvim_g4 = runtime_dir.join(NEAR_G4_NVIM_NAME);
    let tab_g5 = runtime_dir.join(NEAR_G5_TAB_NAME);
    let pid_g5 = runtime_dir.join(NEAR_G5_PID_NAME);
    let nvim_g6 = runtime_dir.join(NEAR_G6_NVIM_NAME);
    let pid_g7 = runtime_dir.join(NEAR_G7_PID_NAME);
    let nvim_g7 = runtime_dir.join(NEAR_G7_NVIM_NAME);
    let readbacks = [
        &pid_g1, &nvim_g2, &pid_g3, &nvim_g4, &tab_g5, &pid_g5, &nvim_g6, &pid_g7, &nvim_g7,
    ];
    for path in captures.iter().map(|(_, _, png)| png).chain(readbacks) {
        clear_evidence(path, SMOKE);
    }

    // The tab centres, pinned before cage starts as the native-tabs smoke's click is.
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "the expected cage output width");
    let tab1_x = pixel_target(
        output_w * TABS_FIRST_TAB_CENTRE_FRACTION,
        "the first tab's centre x",
    );
    let tab2_x = pixel_target(
        output_w * NEAR_SECOND_TAB_CENTRE_FRACTION,
        "the second tab's centre x",
    );
    let tab_y = pixel_target(
        OVERLAY_ORIGIN_Y_EXPECTED + TAB_BAR_HEIGHT_EXPECTED / 2.0,
        "the tab bar's centre y",
    );

    let (_, _, last_png) = &captures[captures.len() - 1];
    let mut cmd = pointer_command(NEAR_BODY, &cwd, &runtime_dir, last_png);
    pin_terminal_smoke_shell(&mut cmd)
        .env("NEAR_FIXTURE", NEAR_FIXTURE_FILE)
        .env("NEAR_TAB1_X", tab1_x.to_string())
        .env("NEAR_TAB2_X", tab2_x.to_string())
        .env("NEAR_TAB_Y", tab_y.to_string())
        .env("NEAR_G1_PID", &pid_g1)
        .env("NEAR_G2_NVIM", &nvim_g2)
        .env("NEAR_G3_PID", &pid_g3)
        .env("NEAR_G4_NVIM", &nvim_g4)
        .env("NEAR_G5_TAB", &tab_g5)
        .env("NEAR_G5_PID", &pid_g5)
        .env("NEAR_G6_NVIM", &nvim_g6)
        .env("NEAR_G7_PID", &pid_g7)
        .env("NEAR_G7_NVIM", &nvim_g7);
    for (leg, _, png) in &captures {
        cmd.env(format!("OUT_{}", leg.to_uppercase()), png);
    }
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless neighbour-pane scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        !matches!(status.code(), Some(c) if (NEAR_EXIT_FIRST..=NEAR_EXIT_LAST).contains(&c)),
        "{SMOKE}: the cage scenario exited with a leg-specific code ({status}); NEAR_BODY's table \
         names each. Exit 61: no shell answered with the drawer open, so the chord opened no \
         focused drawer. Exit 62: Neovim did not answer once the drawer hid, so focus stayed in a \
         terminal. Exit 63 or 66: no shell answered after `:vsplit` and `:terminal`, so no \
         focused terminal pane opened. Exit 64 or 69: Neovim did not answer after `<C-w>c` in a \
         terminal pane, so focus stayed in a terminal. Exit 65: Neovim did not answer after \
         Ctrl+Shift+T. Exit 67: Neovim did not answer after the click on the first tab, so keys \
         went to a terminal. Exit 68: no shell answered after the click on the second tab, so \
         that tab's terminal pane did not get focus back and the `<C-w>c` would have gone to \
         Neovim. None of these is the painting this smoke is about."
    );
    assert_pointer_scenario_succeeded(SMOKE, status, &cwd, last_png);

    // --- Assertion 2 (precondition): the captures and the click points ---------
    for (_, _, png) in &captures {
        assert_png_present(SMOKE, png);
    }
    let extents = read_output_extents(&cwd, SMOKE);
    assert_inside_extents(SMOKE, "the FIRST TAB's centre", (tab1_x, tab_y), extents);
    assert_inside_extents(SMOKE, "the SECOND TAB's centre", (tab2_x, tab_y), extents);

    // --- Assertion 3 (precondition): every leg's step happened ----------------
    for (path, want, leg, step) in [
        (
            &nvim_g2,
            ONE_EDITOR_IN_TAB_1,
            "g2",
            "the chord hid the drawer",
        ),
        (
            &nvim_g4,
            ONE_EDITOR_IN_TAB_1,
            "g4",
            "`<C-w>c` closed the terminal split",
        ),
        (
            &tab_g5,
            ONE_EDITOR_IN_TAB_2,
            "g5",
            "Ctrl+Shift+T opened and entered a second tab",
        ),
        (
            &nvim_g6,
            ONE_EDITOR_IN_TAB_1,
            "g6",
            "the click made the first tab current",
        ),
        (
            &nvim_g7,
            ONE_EDITOR_IN_TAB_2,
            "g7",
            "`<C-w>c` closed the second tab's split",
        ),
    ] {
        let got = read_ints(path, want.len(), SMOKE);
        assert_eq!(
            got,
            want,
            "{SMOKE}: leg {leg}'s readback {} reads {got:?} as `tabpagenr() winnr('$') \
             b:wolf_terminal_id`, want {want:?}: one editor window in that tab, which is what \
             says {step}. A wrong tab means the switch or the new tab did not happen; a second \
             window means the drawer or the terminal pane is still open; a nonzero id means \
             Neovim's current buffer is a terminal placeholder. The capture of this leg would \
             then show something other than its row in NEAR_LEGS.",
            path.display()
        );
    }
    let drawer_pid = read_ints(&pid_g1, 1, SMOKE)[0];
    let split_pid = read_ints(&pid_g3, 1, SMOKE)[0];
    let tab_pid = read_ints(&pid_g5, 1, SMOKE)[0];
    let tab_back_pid = read_ints(&pid_g7, 1, SMOKE)[0];
    assert!(
        split_pid != drawer_pid && tab_pid != drawer_pid && tab_pid != split_pid,
        "{SMOKE}: the shells answering in g1, g3 and g5 have pids {drawer_pid}, {split_pid} and \
         {tab_pid}; each must be its own session. A repeated pid means a `:terminal` started no \
         session and the keys reached an earlier shell, so that leg's capture shows no new \
         terminal pane."
    );
    assert_eq!(
        tab_back_pid, tab_pid,
        "{SMOKE}: after the click on the second tab, the shell with pid {tab_back_pid} answered, \
         not that tab's terminal pane's {tab_pid}, so the `<C-w>c` of leg g7 did not close the \
         pane this smoke opened there."
    );

    // --- Assertions 4 and 5: the editor's word in every capture ---------------
    let pages: Vec<(&str, &str, &PathBuf, String)> = captures
        .iter()
        .map(|(leg, what, png)| (*leg, *what, png, ocr_to_lowercase(png)))
        .collect();
    let (_, _, before_png, before_text) = &pages[0];
    assert!(
        before_text.contains(NEAR_OCR_FILL),
        "{SMOKE}: capture g0, before any terminal, does not contain {NEAR_OCR_FILL:?}. Every \
         editor row carries it, so either the `:e {NEAR_FIXTURE_FILE}` never opened the fixture \
         or OCR cannot read the word at SMOKE_FONT_PT; nothing after this is about terminals. \
         PNG kept at {}. OCR was:\n{before_text}",
        before_png.display()
    );
    let missing: Vec<String> = pages[1..]
        .iter()
        .filter(|(_, _, _, text)| !text.contains(NEAR_OCR_FILL))
        .map(|(leg, what, png, text)| format!("{leg}, {what} ({}):\n{text}", png.display()))
        .collect();
    assert!(
        missing.is_empty(),
        "{SMOKE}: {} of the seven terminal legs lost the editor's {NEAR_OCR_FILL:?}, which g0 \
         read with no terminal open. Every leg from g1 on means the editor grid is not painted \
         while any session is live, the hidden drawer's included. The open legs alone (g1, g3, \
         g5) mean a terminal pane paints over its neighbours. The closing legs alone (g2, g4, \
         g7) mean a closed or hidden terminal's view or paint stays where it was. g6 alone \
         means a tab switch leaves the first tab's editor unpainted. The legs and their OCR:\n\n\
         {}",
        missing.len(),
        missing.join("\n\n")
    );
}

// --- The terminal-pane smoke: typed keys reach a session's shell, which paints its colours. ---

/// TERMINAL-PANE smoke: the program `WOLF_SMOKE_TERMINAL` names, which the app opens as a
/// session in tab 0's first pane before it reports itself ready.
const TP_PROGRAM: &str = "/bin/sh";

/// The prompt the session's shell is given through `SMOKE_PS1`, which [`READINESS_PRELUDE`]
/// exports as `PS1` (see [`pin_terminal_smoke_shell`] for why not `PS1` itself), fixed so
/// the page is the same on every host. The scenario also removes `ENV`, so the shell reads
/// no rc file.
const TP_PS1: &str = "$ ";

/// The first line typed into the session. Its output, [`TP_OCR_WORD`], is the two values
/// joined in the other order, so no typed text and no echo of one carries it: only the shell
/// running the line can put it on screen.
const TP_ECHO_LINE: &str = "a=lantern; b=foxglove; echo \"$b$a\"";

/// What [`TP_ECHO_LINE`] prints, and the OCR assertion's term.
pub(crate) const TP_OCR_WORD: &str = "foxglovelantern";

/// The second line typed: the shell writes the session's pty size into `$TP_SIZE_FILE`, which
/// it inherits from the app. The path itself is never typed, so it is on no page.
const TP_SIZE_LINE: &str = "stty size > \"$TP_SIZE_FILE\"";

/// The size readback, under `XDG_RUNTIME_DIR` so no sidebar row draws it.
const TP_SIZE_NAME: &str = "wolf_gnome_terminal_size.txt";

/// The two captures: the colour line typed but not run, and the page after it ran.
const TP_TYPED_PNG_NAME: &str = "wolf_gnome_terminal_typed.png";
const TP_FINAL_PNG_NAME: &str = "wolf_gnome_terminal_final.png";

/// The truecolour background [`tp_colour_line`] prints under [`TP_COLOUR_CELLS`] blank cells.
/// Nothing else Wolf draws uses it, which the first capture asserts rather than assumes.
const TP_COLOUR: Rgb8 = (17, 201, 113);

/// How many blank cells the colour line prints on [`TP_COLOUR`].
const TP_COLOUR_CELLS: usize = 9;

/// The share of [`TP_COLOUR_CELLS`] cells' area the final capture must hold of [`TP_COLOUR`]
/// exactly. Under one because the antialiased seams between fractional cells blend.
const TP_MIN_COLOUR_FRACTION: f64 = 0.90;

/// The size a session has before any `resize_terminal`, as `stty size` prints it (rows, cols):
/// core's default. A readback of it means the pane never sized its session.
const TP_UNSIZED: (i64, i64) = (24, 80);

/// The third line typed: [`TP_COLOUR_CELLS`] spaces on [`TP_COLOUR`], then a reset. Built from
/// the constants, so the colour asserted and the colour printed cannot drift apart.
fn tp_colour_line() -> String {
    format!(
        "printf '\\033[48;2;{};{};{}m%{TP_COLOUR_CELLS}s\\033[0m\\n' ''",
        TP_COLOUR.0, TP_COLOUR.1, TP_COLOUR.2
    )
}

/// The TERMINAL-PANE scenario, run as a single `sh -c` string inside `cage` (the
/// `COMPOSE_SCENARIO` shape): launch with `WOLF_SMOKE_TERMINAL` set, so tab 0's pane shows a
/// shell session; type a line whose output is the OCR term; type the size readback; type the
/// colour line without Return and capture; press Return and capture again.
///
/// Every line is typed with `wtype`, so its printables arrive as input-method commits and its
/// Return through `gdk_key`, the two routes a terminal pane takes keys by.
///
/// A size readback that never appears is reported on stderr and does not end the scenario, so
/// the two captures are still taken and the Rust side names which claim failed.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `40` — it never became ready (the shared convention).
const TERMPANE_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for the size readback.
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
wait_ready "$app_pid" 11 "wolf-gnome terminal-pane smoke"

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
    echo "wolf-gnome terminal-pane smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 1: a line whose output appears in no typed text ---------------------
wtype -- "$TP_ECHO_LINE"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: the session's size, written by the shell itself ------------------
wtype -- "$TP_SIZE_LINE"
wtype -k Return
wait_file "$TP_SIZE_FILE" || true

# --- Step 3: the colour line, typed and captured, then run and captured --------
wtype -- "$TP_COLOUR_LINE"
sleep "$SETTLE"
grim "$OUT_TYPED"
wtype -k Return
sleep "$SETTLE"
grim "$OUT_FINAL"

exit 0
"#;

/// **Keys typed into a terminal pane reach its shell, the pane sizes the session, and the
/// session's own colours reach the screen.**
///
/// The only test that joins the window's key routing, the pane's sizing and the canvas in a
/// running app: `terminal_seams.rs` pins the rules and `terminal_paint.rs` the canvas against a
/// real session, neither with a widget. The pane is opened through the
/// `WOLF_SMOKE_TERMINAL` hook before the app reports ready, so the first key typed is
/// the shell's; [`terminal_pane_splits_and_focus_under_cage`] opens one by typing
/// `:terminal`.
///
/// ## Assertions
///
/// | # | assertion | kind | falsifier |
/// | --- | --- | --- | --- |
/// | 0 | no typed line carries [`TP_OCR_WORD`] | precondition | none |
/// | 1 | the scenario exits 0 and both captures exist | precondition | none |
/// | 2 | OCR of the final capture contains [`TP_OCR_WORD`] | discriminator | in `window.rs`, route keys and commits to Neovim even when a terminal is active |
/// | 3 | the readback's cols equal the pane's grid width derived from the pinned geometry and [`cell_metrics_for_smoke`]; its rows are at least 1, at most what the pane area could hold, and not [`TP_UNSIZED`]'s | discriminator | skip `resize_terminal` in `sync_terminal_sizes`, so the readback reads `24 80` |
/// | 4 | the typed capture holds no pixel of exactly [`TP_COLOUR`] | precondition: nothing else draws the colour | none |
/// | 5 | the final capture holds at least [`TP_MIN_COLOUR_FRACTION`] of [`TP_COLOUR_CELLS`] cells' area of exactly [`TP_COLOUR`] | discriminator | in `draw_pane`, paint the placeholder grid instead of the canvas |
///
/// Rows have no exact expectation: the pane's height depends on the statusbar strip and the
/// cmdline strip, which no constant here pins. The bound still tells a sized session (11 rows
/// were measured for this pane by `pane_status_bar_shows_each_panes_filename_under_cage`) from
/// an unsized one.
#[test]
fn typed_keys_reach_a_terminal_pane_shell_under_cage() {
    const SMOKE: &str = "typed_keys_reach_a_terminal_pane_shell_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd, with the sidebar fillers for page density; see `SIDEBAR_FILLER_COUNT`.
    let cwd = make_temp_cwd();
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
            panic!("{SMOKE}: failed to write filler {}: {e}", filler.display())
        });
    }

    let colour_line = tp_colour_line();
    // Assertion 0 (precondition): the word must come from the shell, not from an echo.
    for (what, typed) in [
        ("TP_ECHO_LINE", TP_ECHO_LINE),
        ("TP_SIZE_LINE", TP_SIZE_LINE),
        ("tp_colour_line()", colour_line.as_str()),
        ("TP_PS1", TP_PS1),
    ] {
        assert!(
            !typed.to_lowercase().contains(TP_OCR_WORD),
            "{SMOKE}: {what} = {typed:?} contains the OCR term {TP_OCR_WORD:?}, so the echo of \
             the typed line satisfies assertion 2 whether or not the shell ran it. Change the \
             line; do NOT change the term to match it."
        );
    }

    let size_file = runtime_dir.join(TP_SIZE_NAME);
    let typed_png = runtime_dir.join(TP_TYPED_PNG_NAME);
    let final_png = runtime_dir.join(TP_FINAL_PNG_NAME);
    for path in [&size_file, &typed_png, &final_png] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(TERMPANE_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("WOLF_SMOKE_TERMINAL", TP_PROGRAM)
        .env("SMOKE_PS1", TP_PS1)
        .env_remove("ENV")
        .env("TP_SIZE_FILE", &size_file)
        .env("TP_ECHO_LINE", TP_ECHO_LINE)
        .env("TP_SIZE_LINE", TP_SIZE_LINE)
        .env("TP_COLOUR_LINE", &colour_line)
        .env("OUT_TYPED", &typed_png)
        .env("OUT_FINAL", &final_png)
        .status()
        .expect("failed to spawn `cage` for the headless terminal-pane smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage scenario exited unsuccessfully ({status}). Exit 11: the app died \
         before it was ready for input. Exit 40: it never became ready; its `wolf-ready:` stderr \
         lines in cage.log name the condition. Any other status means wtype or grim failed."
    );
    assert_png_present(SMOKE, &typed_png);
    assert_png_present(SMOKE, &final_png);

    // --- Assertion 2 (discriminator): the shell ran the typed line ------------
    let ocr = ocr_to_lowercase(&final_png);
    assert!(
        ocr.contains(TP_OCR_WORD),
        "{SMOKE}: OCR of the final capture does not contain {TP_OCR_WORD:?}, the output of the \
         typed {TP_ECHO_LINE:?}. Open {} and check, in order:\n\
         (a) no terminal opened: cage.log holds a `wolf-gnome:` line saying open_terminal \
         failed, and the pane shows an editor buffer rather than a prompt;\n\
         (b) the keys reached Neovim instead of the session: the pane shows a prompt with \
         nothing typed after it, and the size readback is missing too;\n\
         (c) the shell was slow: the typed line is there with no output under it;\n\
         (d) the word drew but OCR misread it: add page text, do not weaken the term.\n\
         OCR output was:\n{ocr}",
        final_png.display()
    );

    // --- Assertion 3 (discriminator): the pane sized the session --------------
    assert!(
        size_file.exists(),
        "{SMOKE}: the shell never wrote {}. Assertion 2 passed, so keys reached the shell; the \
         typed `stty size` line failed, or `TP_SIZE_FILE` did not reach the session's \
         environment through the app.",
        size_file.display()
    );
    let size = read_ints(&size_file, 2, SMOKE);
    let (rows, cols) = (size[0], size[1]);
    let cell = cell_metrics_for_smoke();
    let padding = wolf_gnome::editor_inset::EDITOR_PADDING_PX;
    // The pane area's width as `the_column_cap_cannot_bind_at_the_ocr_smokes_geometry` takes it.
    let pane_area_w =
        exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED") - OVERLAY_ORIGIN_X_EXPECTED;
    let expected_cols = ((pane_area_w - 2.0 * padding).floor() / cell.cell_w).floor() as i64;
    let pane_area_h =
        exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED") - OVERLAY_ORIGIN_Y_EXPECTED;
    let max_rows = ((pane_area_h - 2.0 * padding) / cell.cell_h).floor() as i64;
    assert_eq!(
        cols,
        expected_cols,
        "{SMOKE}: the session is {cols} columns wide, not the {expected_cols} the pane's grid \
         holds ({pane_area_w} px of pane area less two {padding} px paddings, over a {} px cell). \
         {} means the session was never resized from core's default; another width means the \
         pane sized it from a different area than its grid, or the pinned geometry moved \
         (re-check `the_column_cap_cannot_bind_at_the_ocr_smokes_geometry` first). The count \
         sits close to a boundary: {expected_cols} cells need {} px, so a pane area one pixel \
         narrower than OVERLAY_ORIGIN_X_EXPECTED says reads one column fewer; re-measure the \
         area before blaming the sizing. Readback kept at {}.",
        cell.cell_w,
        TP_UNSIZED.1,
        expected_cols as f64 * cell.cell_w,
        size_file.display()
    );
    assert!(
        (1..=max_rows).contains(&rows) && rows != TP_UNSIZED.0,
        "{SMOKE}: the session is {rows} rows tall. A sized session has between 1 and {max_rows} \
         rows (the pane area's {pane_area_h} px less its paddings, over a {} px cell; 11 were \
         measured for this pane's grid), and {} is core's default, which means the session was \
         never resized. Readback kept at {}.",
        cell.cell_h,
        TP_UNSIZED.0,
        size_file.display()
    );

    // --- Assertion 4 (precondition): nothing else draws the colour ------------
    let typed = decode_capture(&typed_png, SMOKE, "the typed capture");
    let whole = SampleRect {
        x0: 0,
        x1: typed.width,
        y0: 0,
        y1: typed.height,
    };
    let before = typed.count_exactly(whole, TP_COLOUR);
    assert_eq!(
        before,
        0,
        "{SMOKE}: the capture taken with the colour line typed but not run already holds {before} \
         pixel(s) of exactly {TP_COLOUR:?}, so assertion 5 could count pixels the session never \
         painted. Change TP_COLOUR. PNG kept at {}.",
        typed_png.display()
    );

    // --- Assertion 5 (discriminator): the session's colour reached the screen --
    let ran = decode_capture(&final_png, SMOKE, "the final capture");
    let after = ran.count_exactly(whole, TP_COLOUR);
    let area = TP_COLOUR_CELLS as f64 * cell.cell_w * cell.cell_h;
    assert!(
        after as f64 >= TP_MIN_COLOUR_FRACTION * area,
        "{SMOKE}: the final capture holds {after} pixel(s) of exactly {TP_COLOUR:?}, under \
         {TP_MIN_COLOUR_FRACTION} of the {area} px {TP_COLOUR_CELLS} cells cover. Assertion 2 \
         passed, so the shell ran its lines: none means the pane painted something other than \
         the session's frame (the placeholder grid) or the canvas dropped the cells' \
         backgrounds; some means part of the cells is painted over. PNG kept at {}.",
        final_png.display()
    );
}
