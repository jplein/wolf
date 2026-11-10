//! Pointer smokes: click, drag, wheel scroll, the click-then-close focus wedge and a divider
//! drag, each driven through `wolf-pointer-inject` and read back from Neovim.

use super::*;

// Click-then-close focus-wedge fixtures. The `ALIVE*` readbacks are the proof; OCR corroborates.
// Neither name contains `wedgeclick` or `wedgestay`, so only a rendered buffer matches.

/// Wedge smoke: file opened FIRST, so it ends up in the LEFT pane. This is the
/// pane the smoke CLICKS and then CLOSES.
const WEDGE_FILE_CLICK: &str = "zclick.md";

/// Heading written into [`WEDGE_FILE_CLICK`]; lowercased it contains
/// [`WEDGE_OCR_CLICK`].
const WEDGE_HEADING_CLICK: &str = "# WEDGECLICK";

/// Lowercased substring OCR must NOT find after the close: the clicked-then-closed
/// pane's file.
pub(crate) const WEDGE_OCR_CLICK: &str = "wedgeclick";

/// Wedge smoke: file opened in the new RIGHT pane after `<C-w>v` — the SURVIVOR.
const WEDGE_FILE_STAY: &str = "zstay.md";

/// Heading written into [`WEDGE_FILE_STAY`]; lowercased it contains
/// [`WEDGE_OCR_STAY`].
const WEDGE_HEADING_STAY: &str = "# WEDGESTAY";

/// Lowercased substring OCR MUST find after the close: the survivor pane's file.
pub(crate) const WEDGE_OCR_STAY: &str = "wedgestay";

/// Exact contents of the CONTROL readback the wedge smoke writes BEFORE the pane
/// close. Its presence separates "keys never worked at all / the click broke input
/// immediately" from "keys died at the close" — without it a red run is ambiguous.
const WEDGE_ALIVE_MARK_1: &str = "ALIVE1";

/// Exact contents of the PROOF readback the wedge smoke writes AFTER the pane
/// close. Its absence IS the click-then-close focus wedge.
const WEDGE_ALIVE_MARK_2: &str = "ALIVE2";

/// Columns the click smoke moves RIGHT from the probe cell.
const CLICK_DX: i64 = 7;

/// Rows the click smoke moves DOWN from the probe cell.
const CLICK_DY: i64 = 4;

/// Columns the drag smoke's Drag A sweeps RIGHT from the press cell.
const DRAG_DX: i64 = 9;

/// Rows the drag smoke's Drag A sweeps DOWN from the press cell.
const DRAG_DY: i64 = 5;

/// Fractions of the way from the press point to the release point at which Drag A
/// injects intermediate `motion` events, so GTK receives several `drag-update`s the
/// way a real drag would rather than a single teleport.
///
/// Drag Z (the round trip, which runs FIRST) walks these outbound and then walks
/// them again in REVERSE on the way home, so its whole gesture is
/// probe -> waypoints -> `(DRAG_X, DRAG_Y)` -> the same waypoints -> probe.
const DRAG_STEP_FRACTIONS: [f64; 3] = [0.25, 0.5, 0.75];

/// x, in output pixels, at which the drag smoke's Drag B RELEASES — deliberately
/// over the file-tree sidebar, far to the LEFT of the editor pane's left edge.
///
/// This is what proves the `GtkGesture`'s implicit grab keeps delivering after the
/// pointer leaves the pane, and that `cell_at` CLAMPS the out-of-widget point to the
/// nearest edge cell rather than the event being dropped.
const DRAG_OUT_X: i64 = 2;

/// How many wheel detents the scroll smoke injects in each direction.
const SCROLL_DETENTS: i64 = 3;

/// The continuous `wl_pointer.axis` value paired with each discrete detent, as a
/// string because it is passed to the helper verbatim.
///
/// A conventional per-detent value: `zwlr_virtual_pointer_v1.axis_discrete` carries
/// BOTH this continuous value and the discrete step count in one request, and it is
/// the DISCRETE count that GDK turns into `ScrollUnit::Wheel` with `|dy| == 1.0`.
/// MEASURED under cage: `axis 15.0 1` arrives at the client as `axis_source(0)` +
/// `axis_relative_direction(0, 0)` + `axis_value120(0, 120)` + `axis(t, 0, 15.0)` +
/// `frame()`.
const SCROLL_AXIS_VALUE: &str = "15.0";

/// What `visualmode()` returns for a CHARWISE visual selection. An EMPTY string
/// instead means Neovim never entered visual mode at all.
///
/// MEASURED, and NOT what an earlier version of this file assumed: a `<LeftDrag>` is
/// **not** the only way in. On the Linux flake's nvim 0.12.6 a bare `<LeftMouse>` at
/// one cell followed by a `<LeftRelease>` at a DIFFERENT cell also enters charwise
/// visual mode, with exactly the same marks:
///
/// ```text
/// press+release only : visualmode="v" marks="3 4 8 13" cursor="8 13"
/// press+drag+release : visualmode="v" marks="3 4 8 13" cursor="8 13"
/// ```
///
/// So `visualmode() == "v"` after a press-here / release-there gesture proves that
/// `on_pane_release` ran — it does NOT prove that `drag-update` ever fired. The one
/// gesture in this file whose mode DOES prove that is Drag Z, the ROUND TRIP, whose
/// press and release are at the SAME cell: without `<LeftDrag>` in between Neovim
/// sees no movement, stays out of visual mode, and returns the empty string.
const VISUAL_CHARWISE: &str = "v";

// Divider smokes bisect for the theme-owned `gtk::Paned` handle by which window a click focuses;
// the modelled position is only a seed. See `bisect_divider` in `POINTER_DIVIDER_BODY`.

/// How far either side of the MODELLED divider position each bisection starts.
///
/// The model assumes equal shares of the editor pane's width, which is what
/// `geometry::divider_position` produces to within a handle — so the seed error is
/// small, and this margin is sized for the things the model cannot see: the
/// handle's own width per divider in a chain, and the anchor the model starts from
/// (`PANE_ORIGIN_X_EXPECTED`, which names the text grid's corner rather than the pane
/// area's). It only has to exceed those while staying INSIDE the neighbouring panes,
/// which are ~340 px wide in the three-pane case.
const BISECT_SEED_MARGIN_PX: i64 = 128;

/// How many halvings each bisection runs.
///
/// The seeds span `2 * BISECT_SEED_MARGIN_PX` = 256 px, so seven halvings leave a
/// two-pixel residual — comfortably inside a `GtkPaned` handle's grab area, which is
/// several pixels wider than the one-pixel separator Adwaita draws. The drag's own
/// first assertion (the panes either side of it really moved) is what proves the
/// press landed on the handle; this constant only has to make that likely.
pub(crate) const BISECT_STEPS: i64 = 7;

/// Pixels the divider between the MIDDLE and RIGHT panes is dragged right in
/// [`divider_drag_survives_a_pane_relayout_under_cage`].
///
/// About 6.7 cells at [`SMOKE_FONT_PT`], against ~15-cell columns: large enough
/// that the resulting ratio is unmistakably off centre, small enough that the pane
/// it shrinks stays far above the one-cell `set_content_width` floor.
const DIVIDER_INNER_DRAG_PX: i64 = 150;

/// Pixels the divider between the LEFT and MIDDLE panes is dragged left in
/// [`divider_drag_survives_a_pane_relayout_under_cage`]. This is the drag that
/// re-allocates the inner paned and so fires the re-apply the smoke exists to test.
const DIVIDER_OUTER_DRAG_PX: i64 = 150;

/// How far apart three nominally-equal columns may be, in cells, before the
/// baseline precondition fails.
///
/// This is SLACK, not a modelled error. `geometry::divider_position` positions a
/// divider at a fraction of the paned's own SPAN, so equal weights give equal widths
/// to within the handle plus the half pixel rounding can move an odd span's midpoint
/// — under one cell at this font size, and pinned as a unit property by
/// `geometry::equal_weights_give_equal_widths`. Two cells therefore leaves room for a
/// chain of paneds to accumulate their handles without ever describing a spread the
/// arithmetic is expected to produce.
///
/// The widths themselves are exact — `winwidth()` reports the grid size the
/// front-end asked for, and `nvim_ui_try_resize_grid` was MEASURED not to reflow
/// Neovim's own window layout, so resizing one pane's grid cannot take columns from
/// another's.
///
/// **Do not widen it to absorb a real spread.** A baseline that comes out cells apart
/// means equal weights stopped giving equal widths, which is a product defect with
/// its own unit test; the last time this constant was calibrated against one, it hid
/// the defect for the whole life of the divider feature.
pub(crate) const BASELINE_TOLERANCE_CELLS: i64 = 2;

/// The smallest width change, in cells, that counts as "this drag moved the
/// divider". Three cells is ~67 px against drags of 150+ px, so it discriminates a
/// drag that did nothing from one that landed.
pub(crate) const MIN_DRAG_CELLS: i64 = 3;

/// How far a PRESERVED width ratio may drift across a re-layout or a relaunch.
///
/// The ratio is computed from cell counts, which quantize: one cell is ~0.03 of a
/// two-pane ratio, so this allows a couple of cells of rounding while staying far
/// below the ~0.2 gap that separates a preserved ratio from a re-centred one.
pub(crate) const RATIO_TOLERANCE: f64 = 0.08;

/// How far from `0.5` a preserved ratio must sit for "it did not change" to mean
/// anything at all.
///
/// Without this clause a divider that snapped back to the centre would satisfy
/// "unchanged" whenever the drag never took effect. It is the same non-vacuity
/// discipline the fixture-naming guards apply, aimed at an arithmetic assertion.
pub(crate) const MIN_RATIO_OFFSET: f64 = 0.08;

/// Body of [`click_positions_the_cursor_under_cage`], appended to
/// [`POINTER_PREAMBLE`].
///
/// Two clicks: one at the PROBE point, one at a point exactly
/// `(CLICK_DX, CLICK_DY)` cells away, with Neovim's own `line('.')`/`col('.')` read
/// back after each. The assertion is on the exact DELTA, which is
/// origin-independent.
const POINTER_CLICK_BODY: &str = r#"
fill_buffer

# Ask the captive Neovim where its own cursor is, into the file named by $1.
#
# This -- not OCR -- is the observable. OCR is fine for "did this word render" and
# is structurally incapable of "did the cursor land at exactly row 12, column 25".
# The path is absolute and passed in as an env var, so nothing depends on Neovim's
# cwd.
read_cursor() {
    wtype -- ":call writefile([printf('%d %d', line('.'), col('.'))], '$1')"
    wtype -k Return
    wait_file "$1" || exit 14
}

# The grid the 30-character fixture has to fit inside. The Rust side asserts
# winwidth(0) > 30 (or the fixture soft-wraps and screen row stops tracking buffer
# line) and that the click target row is within winheight(0).
wtype -- ":call writefile([printf('%d %d', winwidth(0), winheight(0))], '$DIMS')"
wtype -k Return
wait_file "$DIMS" || exit 14

# --- The PROBE click -------------------------------------------------------
hsend motion "$PROBE_X" "$PROBE_Y"
hsend press "$BTN_LEFT"
hsend release "$BTN_LEFT"
sleep "$SETTLE"
read_cursor "$POS0"

# --- The TARGET click, CLICK_DX columns right and CLICK_DY rows down --------
hsend motion "$TARGET_X" "$TARGET_Y"
hsend press "$BTN_LEFT"
hsend release "$BTN_LEFT"
sleep "$SETTLE"
read_cursor "$POS1"

# grim still runs and the PNG is still retained for post-mortem, even though no
# assertion here reads a pixel.
grim "$OUT_PNG"
hsend quit
exit 0
"#;

/// Body of [`drag_selects_a_range_under_cage`], appended to [`POINTER_PREAMBLE`].
///
/// THREE drags, and the ORDER IS LOAD-BEARING. Drag Z runs first: it is a ROUND TRIP
/// (press at the probe, sweep out, sweep back, release at the probe) and it is the
/// only one of the three whose `visualmode()` can only be `"v"` if `drag-update`
/// fired. Drag A then stays inside the pane and sweeps down-and-right through three
/// intermediate waypoints; Drag B releases OUTSIDE the pane, over the file-tree
/// sidebar. All three read `visualmode()` plus the `'<` / `'>` marks back out of
/// Neovim.
const POINTER_DRAG_BODY: &str = r#"
fill_buffer

# Ask the captive Neovim for the visual mode and the two selection marks.
#
# `line('''<')` is `line("'<")`: doubled single quotes are Vimscript's escape for an
# embedded single quote, which is how this whole file avoids ever typing a literal
# double quote into an Ex command.
#
# Escape is what SETS `'<` / `'>` -- the marks are written when visual mode is left,
# so the readback must come after it.
read_selection() {
    wtype -- ":call writefile([printf('%s %d %d %d %d', visualmode(), line('''<'), col('''<'), line('''>'), col('''>'))], '$1')"
    wtype -k Return
    wait_file "$1" || exit 14
}

wtype -- ":call writefile([printf('%d %d', winwidth(0), winheight(0))], '$DIMS')"
wtype -k Return
wait_file "$DIMS" || exit 14

# --- Drag Z: the ROUND TRIP. THIS DRAG MUST RUN FIRST. -----------------------
#
# WHY FIRST: `visualmode()` reports the LAST visual mode of the SESSION, not the
# mode of the most recent gesture. Run after Drag A or Drag B it would report their
# stale `v` and assert nothing at all. It also has to run before anything else
# leaves marks, since a stale `'<` / `'>` pair would satisfy the collapse assertion
# on its own. Do not reorder these three drags.
#
# WHY IT EXISTS: this is the ONLY gesture in this smoke that can discriminate
# `drag-update`. MEASURED on the Linux flake's nvim 0.12.6, a press at one cell and
# a release at a DIFFERENT cell enters charwise visual mode all by itself --
# press+release only and press+drag+release produce byte-identical `visualmode()`,
# marks and cursor. So Drag A's `v` is real coverage of `on_pane_release` but is NOT
# evidence that `<LeftDrag>` was ever delivered.
#
# Here press and release are at the SAME cell. Without `drag-update` Neovim sees
# `<LeftMouse>` then `<LeftRelease>` with no movement between them, never enters
# visual mode, and returns an empty `visualmode()` with marks `0 0 0 0`. WITH
# `drag-update` the outbound sweep starts visual mode anchored at the probe, the
# return sweep walks the cursor back, and the selection collapses to exactly one
# cell -- the probe cell.
#
# The outbound waypoints are walked again in REVERSE on the way home, so the return
# is a real sweep rather than one teleport back to the anchor.
#
# `hstep` rather than `hsend` for every event of every gesture here -- see `hstep`
# in POINTER_PREAMBLE for why an unpaced press/drag/release collapses onto one cell
# inside Neovim.
hstep motion "$PROBE_X" "$PROBE_Y"
hstep press "$BTN_LEFT"
hstep motion "$DRAG_MID1_X" "$DRAG_MID1_Y"
hstep motion "$DRAG_MID2_X" "$DRAG_MID2_Y"
hstep motion "$DRAG_MID3_X" "$DRAG_MID3_Y"
hstep motion "$DRAG_X" "$DRAG_Y"
hstep motion "$DRAG_MID3_X" "$DRAG_MID3_Y"
hstep motion "$DRAG_MID2_X" "$DRAG_MID2_Y"
hstep motion "$DRAG_MID1_X" "$DRAG_MID1_Y"
hstep motion "$PROBE_X" "$PROBE_Y"
hstep release "$BTN_LEFT"
sleep "$SETTLE"
wtype -k Escape
sleep "$SETTLE"
read_selection "$SEL_Z"

# --- Drag A: press at the probe, sweep DOWN-AND-RIGHT, release inside the pane
#
# The three intermediate motions exist so GTK receives several `drag-update`s, the
# way a real drag would, rather than one teleport from press to release.
#
# Drag A pins the EXACT (DRAG_DY, DRAG_DX) delta, which is real coverage of
# `on_pane_release`'s `drag_point` -> `start_point() + offset` -> `cell_at` path. It
# is NOT a `drag-update` discriminator -- see Drag Z above for why, and note that
# Drag A stays green with `on_pane_drag` neutered to a no-op.
hstep motion "$PROBE_X" "$PROBE_Y"
hstep press "$BTN_LEFT"
hstep motion "$DRAG_MID1_X" "$DRAG_MID1_Y"
hstep motion "$DRAG_MID2_X" "$DRAG_MID2_Y"
hstep motion "$DRAG_MID3_X" "$DRAG_MID3_Y"
hstep motion "$DRAG_X" "$DRAG_Y"
hstep release "$BTN_LEFT"
sleep "$SETTLE"
wtype -k Escape
sleep "$SETTLE"
read_selection "$SEL_A"

# --- Drag B: press at the probe, sweep LEFT out of the pane, release over the
# --- sidebar.
#
# This is the drag that proves the GtkGesture's implicit grab keeps delivering
# after the pointer leaves the widget, and that `cell_at` clamps the out-of-widget
# point to the nearest edge cell rather than the event being dropped. Like Drag A it
# releases at a DIFFERENT cell from its press, so its `visualmode()` is not a
# `drag-update` discriminator either -- see Drag Z.
hstep motion "$PROBE_X" "$PROBE_Y"
hstep press "$BTN_LEFT"
hstep motion "$OUT_X" "$PROBE_Y"
hstep release "$BTN_LEFT"
sleep "$SETTLE"
wtype -k Escape
sleep "$SETTLE"
read_selection "$SEL_B"

grim "$OUT_PNG"
hsend quit
exit 0
"#;

/// Body of [`wheel_scroll_moves_the_viewport_under_cage`], appended to
/// [`POINTER_PREAMBLE`].
///
/// Three wheel detents down, three up, with `line('w0')` read back before, between
/// and after.
const POINTER_SCROLL_BODY: &str = r#"
fill_buffer

# LOAD-BEARING. Neovim's default `mousescroll` is `ver:3`, so one ScrollEvent moves
# the view THREE lines. Pinning it to 1 makes "one detent = one line" an exact,
# injector-side-only measurement instead of a claim entangled with a Neovim default.
wtype -- ":set mousescroll=ver:1"
wtype -k Return
sleep "$SETTLE"

# Ask the captive Neovim for its viewport topline, into the file named by $1.
read_topline() {
    wtype -- ":call writefile([printf('%d', line('w0'))], '$1')"
    wtype -k Return
    wait_file "$1" || exit 14
}

# The pointer must be OVER the pane: GTK's `scroll` signal carries no coordinates,
# so this is the position `record_pointer` stores and `on_pane_scroll` aims at.
hsend motion "$PROBE_X" "$PROBE_Y"
sleep "$SETTLE"

read_topline "$W0_BEFORE"

# SCROLL_DETENTS separate commands, hence SCROLL_DETENTS separate frames -- a real
# wheel sends one event group per detent, and GTK only converts buffered
# wl_pointer events into GdkEvents on wl_pointer.frame.
i=0
while [ "$i" -lt "$SCROLL_DETENTS" ]; do
    hsend axis "$SCROLL_AXIS_VALUE" 1
    i=$((i + 1))
done
sleep "$SETTLE"

read_topline "$W0_DOWN"

# The same number of detents back UP. A negative discrete count is the direction.
i=0
while [ "$i" -lt "$SCROLL_DETENTS" ]; do
    hsend axis "-$SCROLL_AXIS_VALUE" -1
    i=$((i + 1))
done
sleep "$SETTLE"

read_topline "$W0_UP"

grim "$OUT_PNG"
hsend quit
exit 0
"#;

/// Body of [`click_then_close_keeps_keyboard_input_alive_under_cage`], appended to
/// [`POINTER_PREAMBLE`].
///
/// Open a file, vertical-split, open a second file in the new RIGHT pane, CLICK the
/// LEFT pane, prove keys still work (the control), close that same pane, and prove
/// keys STILL work (the regression assertion).
const POINTER_WEDGE_BODY: &str = r#"
# Left pane content.
wtype -- ":e $WEDGE_FILE_CLICK"
wtype -k Return
sleep "$SETTLE"

# Vertical split: the new pane opens to the RIGHT and becomes focused.
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"

# Right pane content. Now: left = the clicked file, right = the survivor, focus
# is on the RIGHT.
wtype -- ":e $WEDGE_FILE_STAY"
wtype -k Return
sleep "$SETTLE"

# --- THE CLICK -------------------------------------------------------------
#
# (PROBE_X, PROBE_Y) is inside the LEFT pane: the probe is only a few cells right
# of the editor pane's left edge, and the left pane spans roughly half the editor
# width. In the FIXED build this runs `on_pane_clicked` -> `focus_win(left)` +
# `set_active_pane` + `cmdline_area.grab_focus()`. In the reverted build it parks
# GTK focus on the left pane's EPHEMERAL DrawingArea.
#
# The nudge to (1, 1) first is deliberate. The preamble already moved the pointer to
# the probe point, but the split has since REBUILT the pane widget tree
# (`rebuild_pane_area` unparents and re-adds every area), and a `motion_absolute` to
# a position the pointer already occupies need not produce a fresh
# `wl_pointer.motion` at all. Leaving the pane and coming back guarantees a real
# leave/enter/motion sequence into the LEFT pane before the button events, so the
# gesture cannot be handed a stale coordinate.
hsend motion 1 1
hsend motion "$PROBE_X" "$PROBE_Y"
hsend press "$BTN_LEFT"
hsend release "$BTN_LEFT"
sleep "$SETTLE"

# --- CONTROL STEP, before the close ----------------------------------------
#
# NOTE the `|| true` on both wait_file calls below, which is deliberate and unlike
# every other pointer body. Here a MISSING readback file IS the observable this
# smoke exists to detect, not a harness failure, and the Rust side's message for it
# is far better than a bare `exit 14`. Let the scenario run to completion so the
# PNG and whichever ALIVE file DID appear survive as evidence.
wtype -- ":call writefile(['$ALIVE_MARK1'], '$ALIVE1')"
wtype -k Return
wait_file "$ALIVE1" || true

# --- Close THAT SAME pane --------------------------------------------------
#
# Step "THE CLICK" moved Neovim's current window to the LEFT one, so <C-w>c closes
# the pane that was clicked. (`:q` is equivalent; <C-w>c is fewer keystrokes and
# does not depend on init.lua's cnoreabbrev machinery.)
wtype -M ctrl -k w -m ctrl
wtype -k c

# There is nothing to poll for "the close landed", so settle -- parity with the
# sibling close smokes.
sleep "$SETTLE"

# --- PROOF STEP, after the close -------------------------------------------
wtype -- ":call writefile(['$ALIVE_MARK2'], '$ALIVE2')"
wtype -k Return
wait_file "$ALIVE2" || true

# Park the pointer in the far top-left, over the sidebar, BEFORE the screenshot.
# grim does not composite the cursor by default (overlay_cursor is only set with
# `grim -c`), so this is belt-and-braces; it costs nothing and removes any
# possibility of a cursor glyph landing on an OCR-asserted heading.
hsend motion 1 1
sleep "$SETTLE"

grim "$OUT_PNG"
hsend quit
exit 0
"#;

/// Body of [`divider_drag_survives_a_pane_relayout_under_cage`], appended to
/// [`POINTER_PREAMBLE`].
///
/// Builds a flat row of THREE panes with two `<C-w>v`, bisects for both divider
/// handles, drags the inner divider right, drags the outer divider left, and reads
/// the three panes' widths back out of Neovim after each step.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// * `21` — the divider-0 seeds did not straddle a divider.
/// * `22` — the divider-1 seeds did not straddle a divider.
/// * `23` — the two bisections disagree about which window is the MIDDLE pane, so
///   the seeds are not landing in the panes they are meant to.
///
/// No fixture file is written and `fill_buffer` is never called: every observable
/// here is a number Neovim reports about its own windows, so there is nothing to
/// render and nothing to OCR.
const POINTER_DIVIDER_BODY: &str = r#"
# ---------------------------------------------------------------------------
# Build the flat row of THREE panes.
#
# Each <C-w>v splits the FOCUSED pane and the new pane takes focus, and the pane
# tree inserts a new pane immediately AFTER its target -- so two of them give the
# on-screen row [first, second, third] with the widget chain
# paned0 = first | paned1(second | third). Divider 0 is paned0's handle, divider 1
# is paned1's.
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"

# ---------------------------------------------------------------------------
# probe_winnr <x>: click at (<x>, $PROBE_Y) and leave the number of the window
# that click focused in $winnr.
#
# A click inside a pane focuses that pane's window (`on_pane_clicked` ->
# `focus_win`, which the wedge smoke already proves end to end). A click that
# lands on a `gtk::Paned` HANDLE reaches no pane at all and therefore changes
# nothing. Both facts are load-bearing in `bisect_divider`.
#
# ONE HAZARD, recorded because its symptom would point elsewhere: `gtk::Paned`
# binds Return, space, Escape and the arrow keys on a FOCUSED handle
# (accept-position / cancel-position / move-handle). If a click on the handle ever
# started grabbing keyboard focus, the `wtype -k Return` below would be swallowed,
# no readback would appear, and the scenario would exit 14 -- which reads as "keys
# are not reaching Neovim". Measured today it does not: GTK4's paned grabs focus
# only on F6/F8 handle cycling, and the reset click that precedes every probe lands
# in a PANE, whose handler returns focus to the command-line strip.
probe_winnr() {
    rm -f "$PROBE_FILE"
    hstep motion "$1" "$PROBE_Y"
    hstep press "$BTN_LEFT"
    hstep release "$BTN_LEFT"
    sleep "$POINTER_STEP"
    wtype -- ":call writefile([printf('%d', winnr())], '$PROBE_FILE')"
    wtype -k Return
    wait_file "$PROBE_FILE" || exit 14
    winnr=$(cat "$PROBE_FILE")
}

# bisect_divider <lo> <hi> <precondition-exit-code>: locate the divider handle
# between the panes the two seeds land in. Sets $lo_win, $hi_win and $divider_x.
#
# WHY A SEARCH AND NOT A FORMULA: the handle's width is theme-owned and its
# position is a fraction of the paned's TRAVEL, which subtracts the handle and the
# far child's minimum -- no arithmetic on this side can predict it to within a
# handle width. Clicking is exact, and Neovim's own window number is the answer.
#
# THE RESET CLICK IS LOAD-BEARING. A probe that lands ON the handle focuses no
# window, so it reports whatever the PREVIOUS probe focused. Without first putting
# the focus back on the LOW side before every probe, a handle hit would be read as
# "the high side" whenever the previous probe was there, and the search would
# converge on the wrong edge.
bisect_divider() {
    lo=$1
    hi=$2
    probe_winnr "$lo"
    lo_win=$winnr
    probe_winnr "$hi"
    hi_win=$winnr
    if [ "$lo_win" = "$hi_win" ]; then
        echo "wolf-gnome divider smoke: the bisection seeds $lo and $hi BOTH focused window $lo_win," >&2
        echo "  so they do not straddle a divider and no bisection can find one." >&2
        echo "  Candidate causes, in order of likelihood:" >&2
        echo "    1. the PER-PANE CHROME changed (editor_inset::EDITOR_PADDING_PX or the statusbar" >&2
        echo "       strip's height), which moves the text grid inside every pane;" >&2
        echo "    2. the editor pane's ORIGIN moved (an adw::HeaderBar height or a sidebar width" >&2
        echo "       change) -- click_positions_the_cursor_under_cage derives it and fails first;" >&2
        echo "    3. the measured CELL SIZE changed, so the panes are not the size modelled;" >&2
        echo "    4. the divider HANDLE is far wider than the theme's one pixel, and both seeds" >&2
        echo "       landed on it;" >&2
        echo "    5. a fresh split no longer produces EQUAL shares, which is what the seeds model" >&2
        echo "       -- geometry::equal_weights_give_equal_widths is the unit test for that half." >&2
        exit "$3"
    fi
    steps=0
    while [ "$steps" -lt "$BISECT_STEPS" ]; do
        mid=$(( (lo + hi) / 2 ))
        probe_winnr "$lo"
        probe_winnr "$mid"
        if [ "$winnr" = "$hi_win" ]; then
            hi=$mid
        else
            lo=$mid
        fi
        steps=$((steps + 1))
    done
    # $lo is the largest x that did NOT focus the high-side window, so it is inside
    # the handle by construction.
    divider_x=$lo
}

bisect_divider "$D0_LO" "$D0_HI" 21
win_left=$lo_win
win_mid=$hi_win
divider0_x=$divider_x

bisect_divider "$D1_LO" "$D1_HI" 22
if [ "$lo_win" != "$win_mid" ]; then
    echo "wolf-gnome divider smoke: the two bisections disagree about the MIDDLE pane -- divider 0's" >&2
    echo "  high side is window $win_mid but divider 1's low side is window $lo_win. One of the four" >&2
    echo "  seeds is landing in a pane it was not meant to, so the same causes as exit 21 apply." >&2
    exit 23
fi
win_right=$hi_win
divider1_x=$divider_x

# Hand the discovered numbering to the Rust side, which prints it in every failure
# message: nvim numbers windows by ITS OWN layout, which need not agree with the
# order this front-end draws the panes in.
printf '%s %s %s\n' "$win_left" "$win_mid" "$win_right" > "$WINS"

# read_widths <file>: the three panes' widths in SCREEN order, then the window
# count, as four integers on one line.
#
# Keyed by the DISCOVERED window numbers rather than by `range(1, winnr('$'))`,
# because nvim's numbering follows its own window layout: the pane tree inserts a
# new pane after its target, while `:vsplit` (with 'splitright' off, as
# `runtime/nvim/init.lua` leaves it) creates its new window on the other side. Only
# a discovered number makes "the middle pane grew" mean the pane in the middle of
# the SCREEN.
#
# `winwidth`, never `nvim_win_get_width`: the latter reports the LAYOUT width,
# which `nvim_ui_try_resize_grid` does not touch, so it cannot see a divider drag
# at all. DIVIDER_RELAUNCH_SCENARIO's own `read_widths` carries the measurement.
#
# `len(nvim_tabpage_list_wins(0))` rather than `winnr('$')` so that no `$` appears
# inside the double-quoted shell word carrying the Ex command.
read_widths() {
    rm -f "$1"
    wtype -- ":call writefile([printf('%d %d %d %d', winwidth($win_left), winwidth($win_mid), winwidth($win_right), len(nvim_tabpage_list_wins(0)))], '$1')"
    wtype -k Return
    wait_file "$1" || exit 14
}

read_widths "$W_BASE"

# --- The INNER drag: divider 1, to the RIGHT --------------------------------
#
# `hstep` rather than `hsend` for every event of the gesture -- see `hstep` in
# POINTER_PREAMBLE for the measured reason.
hstep motion "$divider1_x" "$PROBE_Y"
hstep press "$BTN_LEFT"
hstep motion "$(( divider1_x + INNER_DRAG_PX ))" "$PROBE_Y"
hstep release "$BTN_LEFT"
sleep "$SETTLE"
read_widths "$W_INNER"

# --- The OUTER drag: divider 0, to the LEFT ---------------------------------
#
# THIS is the step the smoke exists for. It changes the INNER paned's allocation,
# which is the one thing that changes that paned's `max-position` and so fires the
# re-apply. Whatever the re-apply reads -- the live tree, or a ratio frozen when
# the widget was built -- is what the middle and right panes end up sharing.
hstep motion "$divider0_x" "$PROBE_Y"
hstep press "$BTN_LEFT"
hstep motion "$(( divider0_x - OUTER_DRAG_PX ))" "$PROBE_Y"
hstep release "$BTN_LEFT"
sleep "$SETTLE"
read_widths "$W_OUTER"

# The PNG is retained for post-mortem only; no assertion reads a pixel here.
grim "$OUT_PNG"
hsend quit
exit 0
"#;

/// Reads the drag smoke's selection readback: `visualmode()` plus the four
/// `'<` / `'>` line/column numbers.
///
/// The empty-`visualmode()` case is handled explicitly and is the whole point of
/// this helper. `printf('%s %d %d %d %d', '', 0, 0, 0, 0)` writes a LEADING SPACE
/// and therefore only four whitespace-separated tokens, so a naive five-token parse
/// would blow up with a shape error and bury the actual finding. An empty
/// `visualmode()` means Neovim never entered visual mode at all, so it is returned as
/// the empty string and the caller's `visualmode() == "v"` assertion reports it
/// properly.
///
/// What a NON-empty `"v"` does and does not license is measured, and is documented on
/// [`VISUAL_CHARWISE`]: on the Linux flake's nvim 0.12.6 a press at one cell plus a
/// release at a DIFFERENT cell enters charwise visual mode on its own, with the same
/// marks a full press+drag+release produces. So the mode alone discriminates `drag-update` ONLY
/// for the round-trip gesture (Drag Z), whose press and release are at the same cell.
fn read_selection(path: &Path, smoke: &str) -> (String, [i64; 4]) {
    let raw = read_readback(path, smoke);
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    let (mode, numbers): (String, &[&str]) = match tokens.len() {
        4 => (String::new(), &tokens[..]),
        5 => (tokens[0].to_string(), &tokens[1..]),
        n => panic!(
            "{smoke}: selection readback {} holds {n} whitespace-separated token(s); expected 5 \
             (visualmode, '< line, '< col, '> line, '> col) or 4 (the same with an EMPTY \
             visualmode). Raw contents were {raw:?}.",
            path.display()
        ),
    };
    let mut out = [0_i64; 4];
    for (slot, token) in out.iter_mut().zip(numbers.iter()) {
        *slot = parse_readback_int(token, path, &raw, smoke);
    }
    (mode, out)
}

/// Asserts the pane grid Neovim reports can actually hold the `:put` fixture and the
/// cell the smoke is about to target.
///
/// Both halves are preconditions of the fixture, not of the product: if `winwidth(0)`
/// ever drops to the fixture's width the lines soft-wrap (`opt.wrap`/`opt.linebreak`
/// are on in `runtime/nvim/init.lua`) and SCREEN ROW STOPS TRACKING BUFFER LINE,
/// which would break the row correspondence every assertion here rests on — quietly.
/// If the target row is past `winheight(0)` the click lands on a different buffer
/// line than intended. Both fail loudly here instead.
fn assert_fixture_fits(smoke: &str, dims: (i64, i64), probe_line: i64, rows_below: i64) {
    let (win_width, win_height) = dims;
    assert!(
        win_width > FIXTURE_LINE_WIDTH,
        "{smoke}: Neovim reports winwidth(0) = {win_width}, which is not wider than the \
         {FIXTURE_LINE_WIDTH}-character `:put` fixture. `runtime/nvim/init.lua` sets \
         `opt.wrap = true` and `opt.linebreak = true`, so at this width the fixture SOFT-WRAPS and \
         screen row stops corresponding to buffer line — silently invalidating every row \
         assertion in this smoke. Shrink the FIXTURE (see `fill_buffer` in POINTER_PREAMBLE); do \
         NOT relax the assertion, and do NOT turn `wrap` off, which would diverge from the \
         product's real configuration."
    );
    assert!(
        probe_line + rows_below <= win_height,
        "{smoke}: the target row (probe line {probe_line} + {rows_below}) is below the last \
         visible row, since Neovim reports winheight(0) = {win_height}. The click would land on a \
         different buffer line than intended. Move PROBE_Y up, or shrink the row delta; do NOT \
         relax the delta assertion."
    );
}

/// The MODELLED on-screen x of the divider that follows column `index` (1-based)
/// when the editor pane is divided into `columns` equal columns, in output pixels.
///
/// A model, and knowingly an approximate one, though the approximation is small:
/// `geometry::divider_position` places a divider at a fraction of the paned's own
/// SPAN, which is exactly what equal columns of that span means, so the only error
/// left is the handle each divider consumes — one pixel under Adwaita's non-wide
/// separator, and a few more in a chain.
///
/// It stays a SEED rather than a target because that handle's width is theme-owned
/// and its grab area is wider still, so no arithmetic here can name the pixel a press
/// has to land on. The bisection is what finds it. The second, smaller source of
/// error is the anchor: [`PANE_ORIGIN_X_EXPECTED`] names the text GRID's left edge,
/// which sits `editor_inset::EDITOR_PADDING_PX` inside the pane area this model
/// divides.
fn equal_share_divider_x(index: i64, columns: i64) -> f64 {
    let editor_w =
        exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED") - PANE_ORIGIN_X_EXPECTED;
    let share = exact_f64(index, "the divider index") / exact_f64(columns, "the column count");
    PANE_ORIGIN_X_EXPECTED + editor_w * share
}

/// The `(LO, HI)` seeds a bisection for the divider after column `index` of
/// `columns` starts from: [`equal_share_divider_x`] widened by
/// [`BISECT_SEED_MARGIN_PX`] on each side.
///
/// Both seeds must land INSIDE the panes either side of that divider, which the
/// scenario checks for itself before it searches — see `bisect_divider`.
pub(crate) fn divider_seed_span(index: i64, columns: i64) -> (i64, i64) {
    let nominal = equal_share_divider_x(index, columns);
    let margin = exact_f64(BISECT_SEED_MARGIN_PX, "BISECT_SEED_MARGIN_PX");
    (
        pixel_target(nominal - margin, "the bisection LO seed"),
        pixel_target(nominal + margin, "the bisection HI seed"),
    )
}

/// The head pane's share of a head/tail pair of widths, as a fraction.
///
/// The pair rather than the whole window: the two panes either side of one
/// `gtk::Paned` are what that paned's position governs, and every other pane's
/// width is irrelevant to it.
pub(crate) fn head_width_ratio(head: i64, tail: i64, smoke: &str, what: &str) -> f64 {
    let total = head + tail;
    assert!(
        total > 0,
        "{smoke}: {what} report {head} and {tail} cells, a total of {total}. A ratio cannot be \
         taken of that, and a non-positive pane width means Neovim was never resized to the \
         widget's allocation at all."
    );
    exact_f64(head, "a pane width") / exact_f64(total, "a pane-pair total width")
}

/// A real `wl_pointer.button` press positions the Neovim cursor at exactly the cell
/// the geometry predicts.
///
/// ## What it proves
///
/// That `gtk::GestureDrag::drag-begin` fires from a real `wl_pointer.button` at all;
/// that `pane_view.rs::on_pane_press` runs; and that `send_pane_mouse`'s `cell_at`
/// call maps a real widget-local coordinate to the cell the measured `CellMetrics`
/// predicts. The assertion is on the EXACT delta between two clicks, so it is
/// independent of the pane origin and of any surface offset.
///
/// ## What it must NOT duplicate
///
/// `crates/wolf-gnome/tests/mouse_nvim.rs::mouse_press_drag_release_selects_via_seams`
/// calls `WolfCore::send_mouse` BY HAND and never runs a GTK controller, and
/// `geometry.rs`'s `cell_at_table` / `cell_at_never_exceeds_grid_size_for` are pure
/// arithmetic. Neither can tell you whether a GTK controller ever fires. That —
/// and only that — is this smoke's subject.
///
/// ## Measured geometry (2026-08-04)
///
/// This is the smoke that derives [`PANE_ORIGIN_X_EXPECTED`] /
/// [`PANE_ORIGIN_Y_EXPECTED`], so this table and those two constants are one
/// measurement written down twice and must never disagree. On a **1280x720** cage
/// output (the extents `wolf-pointer-inject` reported on its `ready` line, read back
/// and asserted rather than assumed) at `WOLF_FONT_PT` = [`SMOKE_FONT_PT`]:
///
/// | quantity | value |
/// | --- | --- |
/// | cage output size | `1280x720` |
/// | `cell_w` / `cell_h` | `22.4765625` / `44.0` |
/// | probe `(640, 360)` -> Neovim cell | `line 5, col 17` |
/// | derived GRID origin | `(280.375, 184.0)` |
/// | the pane's visible rows | 11 |
///
/// The origin this smoke derives is the TEXT GRID's corner, not the pane widget's:
/// it comes from a click-to-cell readback, and the per-pane chrome sits between the
/// two. The pane area's own corner is a separate measurement,
/// [`OVERLAY_ORIGIN_X_EXPECTED`] / [`OVERLAY_ORIGIN_Y_EXPECTED`].
///
/// ## Red-run revert
///
/// In `pane_view.rs::on_pane_press`, delete the
/// `self.send_pane_mouse(target, MouseAction::Press, button, x, y, g);` line, leaving
/// `on_pane_clicked` and the grid/button pinning in place. Expected symptom: both
/// readbacks come back `1 1`, and the `l0 >= 2 && c0 >= 2` precondition fires first
/// with the message naming "the press never reached Neovim". Restore, re-run, green.
#[test]
fn click_positions_the_cursor_under_cage() {
    const SMOKE: &str = "click_positions_the_cursor_under_cage";

    let runtime_dir = require_smoke_env();

    // A fresh temp cwd, exactly like every other smoke; see `make_temp_cwd`
    // for why isolation is mandatory. The readback files live here too.
    let cwd = make_temp_cwd();

    // Measure the cell EXACTLY, the same way the app does, BEFORE the run: the
    // target pixel is derived from it.
    let metrics = cell_metrics_for_smoke();

    let pos0 = cwd.join("pointer_click_pos0.txt");
    let pos1 = cwd.join("pointer_click_pos1.txt");
    let dims = cwd.join("pointer_click_dims.txt");
    for path in [&pos0, &pos1, &dims] {
        clear_evidence(path, SMOKE);
    }

    let out_png = runtime_dir.join("wolf_gnome_pointer_click_smoke.png");
    clear_evidence(&out_png, SMOKE);

    // Relative to the probe, never a magic pixel, so no geometry constant can fake the delta.
    let target_x = pixel_target(
        f64::from(PROBE_X) + exact_f64(CLICK_DX, "CLICK_DX") * metrics.cell_w,
        "the click target x",
    );
    let target_y = pixel_target(
        f64::from(PROBE_Y) + exact_f64(CLICK_DY, "CLICK_DY") * metrics.cell_h,
        "the click target y",
    );
    let target_x_env = target_x.to_string();
    let target_y_env = target_y.to_string();

    let status = run_pointer_scenario(
        POINTER_CLICK_BODY,
        &cwd,
        &runtime_dir,
        &out_png,
        &[
            ("POS0", pos0.as_os_str()),
            ("POS1", pos1.as_os_str()),
            ("DIMS", dims.as_os_str()),
            ("TARGET_X", OsStr::new(&target_x_env)),
            ("TARGET_Y", OsStr::new(&target_y_env)),
        ],
    );
    assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    assert_png_present(SMOKE, &out_png);

    // Extents FIRST: a compositor-clamped injection would make every later message
    // point at the wrong thing.
    let extents = read_output_extents(&cwd, SMOKE);
    assert_inside_extents(
        SMOKE,
        "the PROBE point",
        (i64::from(PROBE_X), i64::from(PROBE_Y)),
        extents,
    );
    assert_inside_extents(
        SMOKE,
        "the CLICK TARGET point",
        (target_x, target_y),
        extents,
    );

    let raw0 = read_readback(&pos0, SMOKE);
    let raw1 = read_readback(&pos1, SMOKE);
    let probe_cell = read_ints(&pos0, 2, SMOKE);
    let target_cell = read_ints(&pos1, 2, SMOKE);
    let (l0, c0) = (probe_cell[0], probe_cell[1]);
    let (l1, c1) = (target_cell[0], target_cell[1]);

    let grid = read_ints(&dims, 2, SMOKE);
    assert_fixture_fits(SMOKE, (grid[0], grid[1]), l0, CLICK_DY);

    let (origin_x, origin_y) =
        assert_probe_cell_and_pane_origin(SMOKE, (l0, c0), &metrics, extents, &out_png, &raw0);

    // Asserted BEFORE the delta is used, so the target column cannot be silently
    // clamped to end-of-line and turn the exact-column assertion into a tautology.
    assert!(
        c0 + CLICK_DX <= FIXTURE_LINE_WIDTH,
        "{SMOKE}: the probe landed at column {c0}, so the target column {} would be past the \
         {FIXTURE_LINE_WIDTH}-character fixture line and Neovim would CLAMP the click to \
         end-of-line — making the exact-column assertion below true no matter where the pointer \
         went. Move PROBE_X left or widen the `fill_buffer` fixture; do NOT relax the assertion.",
        c0 + CLICK_DX
    );

    assert!(
        l1 - l0 == CLICK_DY,
        "{SMOKE}: the click moved the cursor {} row(s), expected exactly {CLICK_DY}. Probe click \
         ({PROBE_X}, {PROBE_Y}) -> line {l0}, column {c0} (raw {raw0:?}); target click \
         ({target_x}, {target_y}) -> line {l1}, column {c1} (raw {raw1:?}); cell_w {}, cell_h {}; \
         derived pane origin ({origin_x}, {origin_y}). A delta of ZERO means the press never \
         reached Neovim at all — check `pane_view.rs::on_pane_press` still calls \
         `send_pane_mouse`. A WRONG NON-ZERO delta means the pixel->cell scale disagrees between \
         this test and the app — check that WOLF_FONT_PT = {SMOKE_FONT_PT} really reached the app \
         (`app::font_pt_override` feeding `config_seam::resolve_font_pt`) and that the \
         wolf-config.yaml `smoke_config_dir` seeds still carries SMOKE_FONT_FAMILY = \
         {SMOKE_FONT_FAMILY:?} and SMOKE_LINE_HEIGHT = {SMOKE_LINE_HEIGHT}. PNG kept at {}.",
        l1 - l0,
        metrics.cell_w,
        metrics.cell_h,
        out_png.display()
    );
    assert!(
        c1 - c0 == CLICK_DX,
        "{SMOKE}: the click moved the cursor {} column(s), expected exactly {CLICK_DX}. Probe \
         click ({PROBE_X}, {PROBE_Y}) -> line {l0}, column {c0} (raw {raw0:?}); target click \
         ({target_x}, {target_y}) -> line {l1}, column {c1} (raw {raw1:?}); cell_w {}, cell_h {}; \
         derived pane origin ({origin_x}, {origin_y}). Read the ROW assertion above first: if the \
         row delta was right and only the column is wrong, `cell_w` is the disagreement, which \
         points at the font family or size rather than at the mouse path. PNG kept at {}.",
        c1 - c0,
        metrics.cell_w,
        metrics.cell_h,
        out_png.display()
    );
}

/// A real button-held pointer move drives Neovim into charwise visual mode and
/// selects exactly the range the geometry predicts — including when the release
/// happens OUTSIDE the pane.
///
/// ## What it proves
///
/// That `gtk::GestureDrag` emits `drag-update` during a button-held move at all;
/// that `pane_view.rs::drag_point` reconstructs the absolute widget-local point from
/// `start_point() + offset` (its callbacks carry OFFSETS, not absolute coordinates);
/// and that the gesture's implicit grab keeps delivering AFTER the pointer leaves the
/// pane, with `cell_at` clamping the out-of-widget point to the nearest edge cell
/// rather than the event being dropped.
///
/// ## Three drags, and which one proves what — read this before editing any of them
///
/// A premise this smoke was originally written on is FALSE, and was measured to be
/// false against the Linux flake's nvim 0.12.6: it is NOT the case that "Vim enters
/// visual mode only on a drag, never on a press+release at two different cells". Both
/// gestures produce byte-identical state:
///
/// ```text
/// press+release only : visualmode="v" marks="3 4 8 13" cursor="8 13"
/// press+drag+release : visualmode="v" marks="3 4 8 13" cursor="8 13"
/// ```
///
/// `on_pane_release` alone therefore reproduces everything Drag A asserts, which is
/// why Drag A cannot discriminate `drag-update` and why the round-trip Drag Z exists.
///
/// * **Drag Z (runs FIRST, and must)** — press at the probe, sweep out to
///   `(DRAG_X, DRAG_Y)` through the waypoints, sweep BACK through them, release at
///   the probe. Press and release are at the SAME cell, so without `drag-update`
///   Neovim sees no movement, never enters visual mode, and returns an empty
///   `visualmode()` with marks `0 0 0 0`. Its `visualmode() == "v"` IS the
///   `drag-update` discriminator, and its collapsed `'<` == `'>` is the proof the
///   cursor was walked out and back rather than teleported. It runs first because
///   `visualmode()` carries SESSION memory: after Drag A or Drag B it would report
///   their stale `"v"`. The derived pane-origin band is computed from ITS `'<`,
///   which is the probe cell.
/// * **Drag A** — pins the exact `(DRAG_DY, DRAG_DX)` delta. Real coverage of
///   `on_pane_release`'s `drag_point` -> `start_point() + offset` -> `cell_at` path;
///   not a `drag-update` discriminator.
/// * **Drag B** — pins the out-of-widget clamp to column 1, proving the gesture's
///   implicit grab keeps delivering after the pointer leaves the pane. Also an
///   `on_pane_release` path.
///
/// ## What it must NOT duplicate
///
/// `mouse_nvim.rs::mouse_press_drag_release_selects_via_seams` drives the same seams
/// BY HAND against a real Neovim and never runs a GTK controller. It cannot observe
/// whether `drag-update` is emitted, whether the offsets reconstruct, or whether the
/// implicit grab holds — those three are exactly what this smoke adds.
///
/// ## Red-run revert
///
/// Make `pane_view.rs::on_pane_drag` an immediate no-op (`fn on_pane_drag(&self, …)
/// { }`). The assertion that goes RED is **Drag Z's `visualmode() == "v"`**, not Drag
/// A's: with `on_pane_drag` neutered Drag A and Drag B both still pass (measured —
/// 3/3), because their press and release are at different cells and `on_pane_release`
/// alone puts Neovim into visual mode. Expected symptom in `$SEL_Z`: an EMPTY
/// `visualmode()` and marks `0 0 0 0`, with Drag Z's mode assertion firing first.
/// Restore, re-run, green.
#[test]
fn drag_selects_a_range_under_cage() {
    const SMOKE: &str = "drag_selects_a_range_under_cage";
    // The column `cell_at` must clamp Drag B's out-of-pane release to.
    const CLAMPED_LEFT_COLUMN: i64 = 1;

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let metrics = cell_metrics_for_smoke();

    let sel_z = cwd.join("pointer_drag_sel_z.txt");
    let sel_a = cwd.join("pointer_drag_sel_a.txt");
    let sel_b = cwd.join("pointer_drag_sel_b.txt");
    let dims = cwd.join("pointer_drag_dims.txt");
    for path in [&sel_z, &sel_a, &sel_b, &dims] {
        clear_evidence(path, SMOKE);
    }

    let out_png = runtime_dir.join("wolf_gnome_pointer_drag_smoke.png");
    clear_evidence(&out_png, SMOKE);

    // Drag Z and Drag A share these: Z sweeps out to the same far corner, walks the
    // same waypoints in reverse, and comes home to the probe.
    let dx_px = exact_f64(DRAG_DX, "DRAG_DX") * metrics.cell_w;
    let dy_px = exact_f64(DRAG_DY, "DRAG_DY") * metrics.cell_h;
    let drag_x = pixel_target(f64::from(PROBE_X) + dx_px, "the far-corner drag x");
    let drag_y = pixel_target(f64::from(PROBE_Y) + dy_px, "the far-corner drag y");
    let waypoints = DRAG_STEP_FRACTIONS.map(|fraction| {
        (
            pixel_target(f64::from(PROBE_X) + fraction * dx_px, "a drag waypoint x"),
            pixel_target(f64::from(PROBE_Y) + fraction * dy_px, "a drag waypoint y"),
        )
    });
    let drag_x_env = drag_x.to_string();
    let drag_y_env = drag_y.to_string();
    let waypoint_env = waypoints.map(|(x, y)| (x.to_string(), y.to_string()));
    let out_x_env = DRAG_OUT_X.to_string();

    let status = run_pointer_scenario(
        POINTER_DRAG_BODY,
        &cwd,
        &runtime_dir,
        &out_png,
        &[
            ("SEL_Z", sel_z.as_os_str()),
            ("SEL_A", sel_a.as_os_str()),
            ("SEL_B", sel_b.as_os_str()),
            ("DIMS", dims.as_os_str()),
            ("DRAG_X", OsStr::new(&drag_x_env)),
            ("DRAG_Y", OsStr::new(&drag_y_env)),
            ("DRAG_MID1_X", OsStr::new(&waypoint_env[0].0)),
            ("DRAG_MID1_Y", OsStr::new(&waypoint_env[0].1)),
            ("DRAG_MID2_X", OsStr::new(&waypoint_env[1].0)),
            ("DRAG_MID2_Y", OsStr::new(&waypoint_env[1].1)),
            ("DRAG_MID3_X", OsStr::new(&waypoint_env[2].0)),
            ("DRAG_MID3_Y", OsStr::new(&waypoint_env[2].1)),
            ("OUT_X", OsStr::new(&out_x_env)),
        ],
    );
    assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    assert_png_present(SMOKE, &out_png);

    let extents = read_output_extents(&cwd, SMOKE);
    assert_inside_extents(
        SMOKE,
        "the PROBE point",
        (i64::from(PROBE_X), i64::from(PROBE_Y)),
        extents,
    );
    assert_inside_extents(
        SMOKE,
        "the far-corner point Drag Z sweeps out to and Drag A releases at",
        (drag_x, drag_y),
        extents,
    );
    for (index, &(x, y)) in waypoints.iter().enumerate() {
        assert_inside_extents(
            SMOKE,
            &format!("intermediate waypoint {index} (walked by Drag Z both ways, and by Drag A)"),
            (x, y),
            extents,
        );
    }
    // Drag B releases over the sidebar: outside the pane but inside the output, which is its point.
    assert_inside_extents(
        SMOKE,
        "the DRAG-B release point",
        (DRAG_OUT_X, i64::from(PROBE_Y)),
        extents,
    );

    // Only drag Z discriminates `drag-update`: press and release share a cell, so release alone has no
    // visual mode. With `on_pane_drag` a no-op, drags A and B alone still pass.
    let raw_z = read_readback(&sel_z, SMOKE);
    let (mode_z, marks_z) = read_selection(&sel_z, SMOKE);
    let [z_start_line, z_start_col, z_end_line, z_end_col] = marks_z;

    assert!(
        mode_z == VISUAL_CHARWISE,
        "{SMOKE}: Drag Z — the ROUND TRIP — left `visualmode()` as {mode_z:?}, expected \
         {VISUAL_CHARWISE:?} (charwise). THIS IS THE `drag-update` DISCRIMINATOR OF THIS SMOKE. \
         Drag Z presses and releases at the SAME cell ({PROBE_X}, {PROBE_Y}), sweeping out to \
         ({drag_x}, {drag_y}) and back through the waypoints in between, so an EMPTY string here \
         means Neovim saw `<LeftMouse>` then `<LeftRelease>` with NO movement between them, never \
         entered visual mode, and left the marks unset (`0 0 0 0`) — i.e. `drag-update` never \
         fired. Check that the `gtk::GestureDrag` in `pane_view.rs::make_pane_area` still connects \
         `drag-update` to `on_pane_drag`, and that `on_pane_drag` still calls `send_pane_mouse` \
         with `MouseAction::Drag`. Note that Drag A's mode CANNOT stand in for this one: measured \
         on the Linux flake's nvim 0.12.6, a press at one cell plus a release at a DIFFERENT \
         cell enters charwise visual mode all by itself, with identical marks. A DIFFERENT \
         non-empty mode (`V` linewise, CTRL-V blockwise) would instead mean the encoded \
         modifiers are wrong. Raw readback was {raw_z:?}; PNG kept at {}.",
        out_png.display()
    );

    assert!(
        z_start_line == z_end_line,
        "{SMOKE}: Drag Z returned to its anchor before releasing, so its `'<` and `'>` must be on \
         the SAME line — instead they are lines {z_start_line} and {z_end_line}. The gesture ends \
         at the exact pixel it started at, so a residual row span means the return sweep's \
         `drag-update`s were dropped (GDK motion compression folding the way home into nothing, or \
         the last waypoint never being delivered) and the selection is still anchored out at the \
         far corner. Raw readback was {raw_z:?}; PNG kept at {}.",
        out_png.display()
    );
    assert!(
        z_start_col == z_end_col,
        "{SMOKE}: Drag Z returned to its anchor before releasing, so its `'<` and `'>` must be in \
         the SAME column — instead they are columns {z_start_col} and {z_end_col}. Read the LINE \
         assertion above first: if the rows collapsed and only the columns did not, the return \
         sweep was delivered but stopped short horizontally. Raw readback was {raw_z:?}; PNG kept \
         at {}.",
        out_png.display()
    );

    let grid = read_ints(&dims, 2, SMOKE);

    // Drag Z pressed and released at the probe, so its `'<` is the probe cell and the origin is derived
    // from it, as the click smoke does.
    let (origin_x, origin_y) = assert_probe_cell_and_pane_origin(
        SMOKE,
        (z_start_line, z_start_col),
        &metrics,
        extents,
        &out_png,
        &raw_z,
    );

    // Drag A pins the exact delta through `on_pane_release`'s `cell_at` path; it is not a `drag-update`
    // discriminator (see Drag Z).
    let raw_a = read_readback(&sel_a, SMOKE);
    let (mode_a, marks_a) = read_selection(&sel_a, SMOKE);
    let [a_start_line, a_start_col, a_end_line, a_end_col] = marks_a;

    assert!(
        mode_a == VISUAL_CHARWISE,
        "{SMOKE}: Drag A left `visualmode()` as {mode_a:?}, expected {VISUAL_CHARWISE:?} \
         (charwise). Drag Z passed, so `drag-update` IS firing; this instead says the press and \
         release of a plain down-and-right sweep did not put Neovim into visual mode at all. \
         Note that an empty string here is NOT the `drag-update` discriminator it was once \
         described as: measured on the Linux flake's nvim 0.12.6, `<LeftMouse>` at one cell \
         plus `<LeftRelease>` at a DIFFERENT cell enters charwise visual mode WITHOUT any \
         `<LeftDrag>`, with identical marks and cursor. So an empty mode here means Neovim \
         received neither the press nor the release as a mouse event — check \
         `pane_view.rs::on_pane_press` and `on_pane_release` still call `send_pane_mouse`. A \
         DIFFERENT non-empty mode (`V` linewise, CTRL-V blockwise) would instead mean the \
         encoded modifiers are wrong. Raw readback was {raw_a:?}; PNG kept at {}.",
        out_png.display()
    );

    assert_fixture_fits(SMOKE, (grid[0], grid[1]), a_start_line, DRAG_DY);

    assert!(
        a_start_col + DRAG_DX <= FIXTURE_LINE_WIDTH,
        "{SMOKE}: Drag A pressed at column {a_start_col}, so its release column {} would be past \
         the {FIXTURE_LINE_WIDTH}-character fixture line and Neovim would CLAMP the selection to \
         end-of-line — making the exact-column assertion below true regardless of where the drag \
         went. Move PROBE_X left or widen the `fill_buffer` fixture; do NOT relax the assertion.",
        a_start_col + DRAG_DX
    );

    // Guaranteed by DRAG_DX, DRAG_DY >= 1, but asserted explicitly so a failure NAMES
    // "the selection was empty" instead of leaving it to be inferred from two deltas.
    assert!(
        (a_start_line, a_start_col) != (a_end_line, a_end_col),
        "{SMOKE}: Drag A's `'<` and `'>` marks are the SAME cell (line {a_start_line}, column \
         {a_start_col}), so the selection is empty and the deltas below would be comparing a \
         no-op against itself. Raw readback was {raw_a:?}; PNG kept at {}.",
        out_png.display()
    );

    assert!(
        a_end_line - a_start_line == DRAG_DY,
        "{SMOKE}: Drag A selected {} row(s), expected exactly {DRAG_DY}. Press ({PROBE_X}, \
         {PROBE_Y}) -> line {a_start_line}, column {a_start_col}; release ({drag_x}, {drag_y}) -> \
         line {a_end_line}, column {a_end_col}; cell_w {}, cell_h {}; derived pane origin \
         ({origin_x}, {origin_y}). A wrong delta with a correct `visualmode()` means the events \
         DID reach Neovim but the point reconstruction is off — check \
         `pane_view.rs::drag_point`, which must return `start_point() + offset` (both the \
         `drag-update` and `drag-end` callbacks carry OFFSETS, not absolute coordinates). Raw \
         readback was {raw_a:?}; PNG kept at {}.",
        a_end_line - a_start_line,
        metrics.cell_w,
        metrics.cell_h,
        out_png.display()
    );
    assert!(
        a_end_col - a_start_col == DRAG_DX,
        "{SMOKE}: Drag A selected {} column(s), expected exactly {DRAG_DX}. Press ({PROBE_X}, \
         {PROBE_Y}) -> line {a_start_line}, column {a_start_col}; release ({drag_x}, {drag_y}) -> \
         line {a_end_line}, column {a_end_col}; cell_w {}, cell_h {}; derived pane origin \
         ({origin_x}, {origin_y}). Read the ROW assertion above first: a correct row delta with a \
         wrong column delta points at `cell_w` (font family/size), not at the drag path. Raw \
         readback was {raw_a:?}; PNG kept at {}.",
        a_end_col - a_start_col,
        metrics.cell_w,
        metrics.cell_h,
        out_png.display()
    );

    // --- Drag B: released OUTSIDE the pane, over the sidebar ----------------
    let raw_b = read_readback(&sel_b, SMOKE);
    let (mode_b, marks_b) = read_selection(&sel_b, SMOKE);
    let [b_start_line, b_start_col, b_end_line, b_end_col] = marks_b;

    assert!(
        mode_b == VISUAL_CHARWISE,
        "{SMOKE}: Drag B left `visualmode()` as {mode_b:?}, expected {VISUAL_CHARWISE:?}. Drag A \
         passed, so the gesture works INSIDE the pane; an empty mode here means the drag stopped \
         being delivered the moment the pointer left the widget, i.e. the `GtkGesture`'s implicit \
         grab is not holding. That is exactly why `make_pane_area` uses ONE `gtk::GestureDrag` \
         rather than an `EventControllerMotion`, which would silently stop at the pane edge. Raw \
         readback was {raw_b:?}; PNG kept at {}.",
        out_png.display()
    );
    assert!(
        b_start_line == b_end_line,
        "{SMOKE}: Drag B was injected along a single row (both motions used y = {PROBE_Y}), but \
         its marks span lines {b_start_line}..{b_end_line}. Raw readback was {raw_b:?}; PNG kept \
         at {}.",
        out_png.display()
    );
    assert!(
        b_start_col == CLAMPED_LEFT_COLUMN,
        "{SMOKE}: Drag B released at x = {DRAG_OUT_X}, far LEFT of the editor pane and over the \
         file-tree sidebar, so `cell_at` must clamp it to column {CLAMPED_LEFT_COLUMN} — instead \
         `'<` is column {b_start_col}. Two candidate causes: either the events STOPPED at the \
         pane edge (the `GtkGesture` implicit grab is not holding, so the last delivered point was \
         still inside the widget), or `cell_at`'s clamp-to-nearest-edge-cell is not being reached \
         (`pane_view.rs::drag_point` rejecting the out-of-widget point instead of passing it \
         through — nothing there may reject it; the clamp IS the mechanism that makes \
         drag-past-the-edge selection work). Raw readback was {raw_b:?}; PNG kept at {}.",
        out_png.display()
    );
    assert!(
        b_end_col == a_start_col,
        "{SMOKE}: Drag B pressed at the same probe point as Drag A, so its `'>` (the press cell, \
         since the drag went LEFT and the marks order the other way) must still be column \
         {a_start_col} — instead it is {b_end_col}. Raw readback was {raw_b:?}; PNG kept at {}.",
        out_png.display()
    );
}

/// A real `wl_pointer.axis` group scrolls the Neovim viewport by exactly one line
/// per detent, in both directions.
///
/// ## What it proves
///
/// That `gtk::EventControllerScroll` fires from a real `wl_pointer.axis` frame at
/// all, WHICH `gdk::ScrollUnit` it reports, and how large its `dy` is. The exact
/// delta assertion IS the `ScrollUnit` measurement — see its failure message.
///
/// MEASURED under cage: `axis 15.0 1` arrives at the client as `axis_source(0)`
/// (WHEEL) + `axis_relative_direction(0, 0)` + `axis_value120(0, 120)` +
/// `axis(t, 0, 15.0)` + `frame()`; GDK reports `ScrollUnit::Wheel` with
/// `|dy| == 1.0` per detent; `scroll_threshold(Wheel, _) == 1.0`; and with
/// `:set mousescroll=ver:1` one detent is exactly one line. The assertion is
/// therefore EXACT and has NO fallback branch.
///
/// ## What it must NOT duplicate
///
/// `mouse_nvim.rs::scroll_seam_moves_viewport_via_wolfcore` drives
/// `ScrollAccumulator` and `WolfCore::send_scroll` by hand; `mouse.rs`'s
/// `scroll_threshold_table` / `scroll_accumulator_table` / `pane_scroll_table` /
/// `scroll_steps_table` / `scroll_direction_is_inverted_from_macos` are pure tables;
/// and `crates/wolf-ffi/tests/mouse_scroll.rs` pins the nvim side. None of them can
/// say which `ScrollUnit` a real cage/wlroots wheel event reports. That is this
/// smoke's subject.
///
/// It also does NOT prove `record_pointer`: `on_pane_scroll` names the target grid
/// explicitly, so a stuck `(0.0, 0.0)` pointer would still scroll the right window.
///
/// ## Red-run revert
///
/// In `pane_view.rs::make_pane_area`, replace the scroll controller's closure body
/// with `glib::Propagation::Stop` alone, dropping the `inner.on_pane_scroll(...)`
/// call. Expected symptom: all three readbacks are `1`, and
/// `w0_down - w0_before == SCROLL_DETENTS` fails with an observed delta of 0.
/// Restore, re-run, green.
#[test]
fn wheel_scroll_moves_the_viewport_under_cage() {
    const SMOKE: &str = "wheel_scroll_moves_the_viewport_under_cage";
    // Where `:1` parks the viewport; a precondition, without which the increase below is unfalsifiable.
    const TOPLINE_AT_REST: i64 = 1;

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();

    let w0_before_file = cwd.join("pointer_scroll_w0_before.txt");
    let w0_down_file = cwd.join("pointer_scroll_w0_down.txt");
    let w0_up_file = cwd.join("pointer_scroll_w0_up.txt");
    for path in [&w0_before_file, &w0_down_file, &w0_up_file] {
        clear_evidence(path, SMOKE);
    }

    let out_png = runtime_dir.join("wolf_gnome_pointer_scroll_smoke.png");
    clear_evidence(&out_png, SMOKE);

    let detents_env = SCROLL_DETENTS.to_string();

    let status = run_pointer_scenario(
        POINTER_SCROLL_BODY,
        &cwd,
        &runtime_dir,
        &out_png,
        &[
            ("W0_BEFORE", w0_before_file.as_os_str()),
            ("W0_DOWN", w0_down_file.as_os_str()),
            ("W0_UP", w0_up_file.as_os_str()),
            ("SCROLL_DETENTS", OsStr::new(&detents_env)),
            ("SCROLL_AXIS_VALUE", OsStr::new(SCROLL_AXIS_VALUE)),
        ],
    );
    assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    assert_png_present(SMOKE, &out_png);

    let extents = read_output_extents(&cwd, SMOKE);
    assert_inside_extents(
        SMOKE,
        "the PROBE point (which is where the wheel events are aimed)",
        (i64::from(PROBE_X), i64::from(PROBE_Y)),
        extents,
    );

    let raw_before = read_readback(&w0_before_file, SMOKE);
    let raw_down = read_readback(&w0_down_file, SMOKE);
    let raw_up = read_readback(&w0_up_file, SMOKE);
    let w0_before = read_ints(&w0_before_file, 1, SMOKE)[0];
    let w0_down = read_ints(&w0_down_file, 1, SMOKE)[0];
    let w0_up = read_ints(&w0_up_file, 1, SMOKE)[0];

    assert!(
        w0_before == TOPLINE_AT_REST,
        "{SMOKE}: `line('w0')` was {w0_before} before any wheel event, expected \
         {TOPLINE_AT_REST}. The `fill_buffer` fixture ends with `:1`, which parks topline at 1; \
         this is the precondition that makes the increase below FALSIFIABLE, so it must hold \
         before anything else is believed. Raw readback was {raw_before:?}; PNG kept at {}.",
        out_png.display()
    );

    assert!(
        w0_down - w0_before == SCROLL_DETENTS,
        "{SMOKE}: {SCROLL_DETENTS} DOWN detents moved `line('w0')` by {} line(s) ({w0_before} -> \
         {w0_down}), expected exactly {SCROLL_DETENTS}. THIS ASSERTION IS THE `ScrollUnit` \
         MEASUREMENT, so read the observed number as a diagnosis:\n\
         * exactly {SCROLL_DETENTS} — GDK reported `ScrollUnit::Wheel`, `|dy| == 1.0` per detent, \
         `scroll_threshold(Wheel, _) == 1.0`, one detent = one step. This is the measured, \
         expected outcome.\n\
         * exactly 1 (or 0) — GDK reported `ScrollUnit::Surface`; the threshold is then `cell_h` \
         (~44 px at {SMOKE_FONT_PT} pt) and a {SCROLL_AXIS_VALUE} delta is about a third of a \
         cell, so three detents accumulate to one step. That is the \"over-sensitive beats dead\" \
         fallback working. RECORD the finding in `crates/wolf-gnome/tests/README.md`; do NOT \
         loosen this assertion.\n\
         * {SCROLL_DETENTS} * 120, or 0 with no movement at all — wlroots is interpreting \
         `axis_discrete`'s `discrete` argument in value120 units differently than measured. Probe \
         with `axis {SCROLL_AXIS_VALUE} 120` (the helper's grammar already allows it, no C \
         change) and record which form works.\n\
         * anything else with `w0_before` correct — check that \
         `:set mousescroll=ver:1` actually took effect; Neovim's default is `ver:3`, which would \
         give three lines per step.\n\
         Raw readbacks: before {raw_before:?}, down {raw_down:?}, up {raw_up:?}. PNG kept at {}.",
        w0_down - w0_before,
        out_png.display()
    );

    // Direction is asserted separately from movement, matching the sign convention
    // `scroll_direction_is_inverted_from_macos` and `crates/wolf-ffi/tests/mouse_scroll.rs` pin.
    assert!(
        w0_up < w0_down,
        "{SMOKE}: {SCROLL_DETENTS} UP detents did not move the viewport back UP: `line('w0')` \
         went {w0_down} -> {w0_up}. Scrolling up must strictly DECREASE the topline. If the DOWN \
         assertion passed and this one fails, the sign is being dropped somewhere between \
         `axis_discrete`'s negative `discrete` and `mouse::scroll_steps` — check \
         `mouse::scroll_direction_is_inverted_from_macos`, which pins the convention. Raw \
         readbacks: before {raw_before:?}, down {raw_down:?}, up {raw_up:?}. PNG kept at {}.",
        out_png.display()
    );
    assert!(
        w0_up == w0_before,
        "{SMOKE}: {SCROLL_DETENTS} down detents followed by {SCROLL_DETENTS} up detents left \
         `line('w0')` at {w0_up}, not back at the starting {w0_before}. The two directions are \
         not symmetric, which usually means a sub-cell remainder is being carried across the \
         direction change — check `mouse::PaneScroll::steps_for`. Raw readbacks: before \
         {raw_before:?}, down {raw_down:?}, up {raw_up:?}. PNG kept at {}.",
        out_png.display()
    );
}

/// Clicking a pane and then CLOSING that same pane must not wedge keyboard input.
///
/// This is the first ever execution of the fix in `df3bb3d`, which until now has
/// been a structural argument only.
///
/// ## HONEST SCOPE — read this before extending or "simplifying" anything
///
/// This smoke proves the **composite** invariant — *a click followed by closing the
/// clicked pane never wedges keyboard input* — and NOT the `focusable(false)` half in
/// isolation. Isolating that half would need a Tab-traversal probe, which is not
/// worth the fragility. Do not read a green run as more than the composite claim.
///
/// ## What it proves
///
/// That the click really moves Neovim's current window (`<C-w>c` afterwards closes
/// the pane that was CLICKED, which the OCR pair corroborates), and that keys still
/// reach Neovim after that pane's `DrawingArea` is destroyed. The `$ALIVE1` control
/// step BEFORE the close is what makes a red run diagnosable at all: without it,
/// "keys never worked" and "keys died at the close" are indistinguishable.
///
/// ## What it must NOT duplicate
///
/// `close_pane_leaves_one_under_cage` (`<C-w>c`),
/// `colon_q_closes_focused_pane_under_cage` (`:q`) and
/// `close_pane_chord_closes_focused_pane_under_cage` (Ctrl+Shift+W) all close a
/// pane, and NONE of them ever CLICKS one first — so in all three the wedge cannot
/// arise, and all three stay green with the fix reverted. The one thing this smoke
/// adds is the click before the close, and its assertion is on KEYBOARD LIVENESS
/// AFTERWARDS, not on which pane survived.
///
/// ## Red-run revert — THREE parts, not one
///
/// All three, or `$ALIVE2` still appears and the wedge assertion never fires (see
/// the measured matrix below for what each smaller revert actually produces):
///
/// 1. `pane_view.rs::make_pane_area` — add `.focusable(true)` to the
///    `gtk::DrawingArea::builder()` chain.
/// 2. `pane_view.rs::on_pane_clicked` — replace `self.cmdline_area.grab_focus();`
///    with a `grab_focus()` on the clicked pane's own `area` (the historical code).
/// 3. `pane_view.rs::rebuild_pane_area` — neuter the return-focus guard, e.g.
///    `let had_focus = false;` in place of
///    `let had_focus = self.focus_inside_pane_area();`.
///
/// **Why (3) is required.** `detect_closes` removes the closed pane from
/// `self.panes` and THEN calls `rebuild_pane_area`, which samples
/// `focus_inside_pane_area()` BEFORE any unparenting — at which moment the closed
/// pane's `DrawingArea` is still in the widget tree and still holds GTK focus. So
/// `had_focus` is `true` and the guard re-grabs focus onto `cmdline_area` after the
/// rebuild. Reverting only (1)+(2) therefore leaves BOTH `ALIVE` assertions PASSING:
/// keyboard input survives on the guard alone, which is the finding — the guard is
/// load-bearing, measured rather than argued.
///
/// ## The MEASURED revert matrix (2026-08-02) — Run B is NOT "still green"
///
/// Run B's two `ALIVE` assertions pass, but the RUN IS NOT GREEN OVERALL: that
/// deliberately-broken configuration renders the surviving pane BLANK (screenshot
/// inspected: sidebar and header draw, editor pane empty), so the OCR corroboration
/// below fails. That is a SECOND, separate defect of a broken build, not the wedge,
/// and it was reproduced twice. Do not record Run B as green; record it as "both
/// `ALIVE` assertions pass, OCR fails".
///
/// ```text
/// configuration                          | result
/// ---------------------------------------|--------------------------------------
/// revert (1) `.focusable(true)` only      | green
/// revert (2) `area.grab_focus()` only     | green
/// revert (1)+(2), guard intact  (Run B)   | ALIVE1 ok, ALIVE2 ok, OCR FAILS — pane blank
/// revert (1)+(2)+(3)            (Run A)   | ALIVE1 ok, ALIVE2 ABSENT — the wedge
/// ```
///
/// Run A is the headline red evidence and holds: it is the first execution of
/// `df3bb3d`. The guard's load-bearingness is evidenced by the two `ALIVE`
/// assertions passing in Run B, NOT by a green overall run.
#[test]
fn click_then_close_keeps_keyboard_input_alive_under_cage() {
    const SMOKE: &str = "click_then_close_keeps_keyboard_input_alive_under_cage";

    let runtime_dir = require_smoke_env();

    let cwd = make_temp_cwd();
    let target_click = cwd.join(WEDGE_FILE_CLICK);
    let target_stay = cwd.join(WEDGE_FILE_STAY);
    std::fs::write(&target_click, format!("{WEDGE_HEADING_CLICK}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write the wedge clicked-pane file {}: {e}",
            target_click.display()
        )
    });
    std::fs::write(&target_stay, format!("{WEDGE_HEADING_STAY}\n")).unwrap_or_else(|e| {
        panic!(
            "failed to write the wedge survivor file {}: {e}",
            target_stay.display()
        )
    });

    let alive1 = cwd.join("pointer_wedge_alive1.txt");
    let alive2 = cwd.join("pointer_wedge_alive2.txt");
    for path in [&alive1, &alive2] {
        clear_evidence(path, SMOKE);
    }

    let out_png = runtime_dir.join("wolf_gnome_pointer_wedge_smoke.png");
    clear_evidence(&out_png, SMOKE);

    let status = run_pointer_scenario(
        POINTER_WEDGE_BODY,
        &cwd,
        &runtime_dir,
        &out_png,
        &[
            ("WEDGE_FILE_CLICK", OsStr::new(WEDGE_FILE_CLICK)),
            ("WEDGE_FILE_STAY", OsStr::new(WEDGE_FILE_STAY)),
            ("ALIVE1", alive1.as_os_str()),
            ("ALIVE2", alive2.as_os_str()),
            ("ALIVE_MARK1", OsStr::new(WEDGE_ALIVE_MARK_1)),
            ("ALIVE_MARK2", OsStr::new(WEDGE_ALIVE_MARK_2)),
        ],
    );
    assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    assert_png_present(SMOKE, &out_png);

    let extents = read_output_extents(&cwd, SMOKE);
    assert_inside_extents(
        SMOKE,
        "the PROBE point (which must land in the LEFT pane)",
        (i64::from(PROBE_X), i64::from(PROBE_Y)),
        extents,
    );

    // The scenario does not `exit 14` on a missing readback: absence is the observable here, diagnosed
    // on this side.
    let alive2_present = alive2.exists();
    assert!(
        alive1.exists(),
        "{SMOKE}: the CONTROL readback {} does not exist, so keys were NOT reaching Neovim even \
         BEFORE the pane close.{} This is a different defect from the click-then-close focus \
         wedge, and it makes the wedge assertion below meaningless. Check \
         `click_positions_the_cursor_under_cage` FIRST: if it is also red, the click itself is \
         broken; if it is green, suspect that `on_pane_clicked`'s `cmdline_area.grab_focus()` \
         moved GTK focus somewhere that never gets it back. PNG kept at {}.",
        alive1.display(),
        if alive2_present {
            " (Curiously the POST-close readback DID appear, which would mean input recovered — \
             investigate the ordering in the scenario.)"
        } else {
            " Neither readback appeared."
        },
        out_png.display()
    );
    let alive1_text = read_trimmed(&alive1, SMOKE);
    assert!(
        alive1_text == WEDGE_ALIVE_MARK_1,
        "{SMOKE}: the CONTROL readback {} holds {alive1_text:?}, expected \
         {WEDGE_ALIVE_MARK_1:?}. The file exists, so `writefile` ran, but with the wrong payload — \
         the typed Ex command was garbled in transit. PNG kept at {}.",
        alive1.display(),
        out_png.display()
    );

    assert!(
        alive2_present,
        "{SMOKE}: `{WEDGE_ALIVE_MARK_1}` exists but `{WEDGE_ALIVE_MARK_2}` does not ({}). Keys \
         reached Neovim BEFORE the pane close and stopped reaching it AFTER — THIS IS THE \
         CLICK-THEN-CLOSE FOCUS WEDGE (`crates/wolf-gnome/src/README.md`, \"GTK keyboard focus \
         never lands on a pane area\"). Closing the pane that held GTK focus destroyed the focus \
         widget, the toplevel was left with none, and window-level key dispatch died for BOTH the \
         Neovim key controller and the chord controller. Check, in order: (1) pane `DrawingArea`s \
         are still built WITHOUT `focusable(true)` in `pane_view.rs::make_pane_area`; (2) \
         `pane_view.rs::on_pane_clicked` still calls `self.cmdline_area.grab_focus()` and never \
         `area.grab_focus()`; (3) `pane_view.rs::rebuild_pane_area`'s \
         `focus_inside_pane_area()` -> `cmdline_area.grab_focus()` guard is still present and \
         still sampled BEFORE any unparenting. MEASURED: reverting only (1) and (2) leaves BOTH \
         `ALIVE` readbacks passing — the guard in (3) catches it on its own, so keyboard input \
         survives — although that build then renders the surviving pane BLANK and fails the OCR \
         corroboration below for an unrelated reason. Only reverting all three produces THIS \
         failure. PNG kept at {}.",
        alive2.display(),
        out_png.display()
    );
    let alive2_text = read_trimmed(&alive2, SMOKE);
    assert!(
        alive2_text == WEDGE_ALIVE_MARK_2,
        "{SMOKE}: the POST-close readback {} holds {alive2_text:?}, expected \
         {WEDGE_ALIVE_MARK_2:?}. The file exists, so keys DO still reach Neovim after the close — \
         the wedge is not present — but the payload is wrong, so the typed Ex command was garbled \
         in transit. PNG kept at {}.",
        alive2.display(),
        out_png.display()
    );

    // OCR corroboration only; the `ALIVE*` files above are the load-bearing assertions.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(WEDGE_OCR_STAY),
        "{SMOKE}: OCR did not contain {WEDGE_OCR_STAY:?} — the HEADING of {WEDGE_FILE_STAY}, the \
         pane that must SURVIVE the close. Both `ALIVE` readbacks passed, so keyboard input is \
         alive; this instead says the surviving pane stopped rendering, or that the wrong pane was \
         closed. MEASURED PRECEDENT: this is exactly the shape of Run B in this smoke's revert \
         matrix — with `.focusable(true)` and `area.grab_focus()` both reverted but the \
         `rebuild_pane_area` guard intact, the surviving pane renders BLANK while keyboard input \
         keeps working. If a revert is in flight, check for a half-reverted focus setup before \
         suspecting the renderer. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        !ocr_text.contains(WEDGE_OCR_CLICK),
        "{SMOKE}: OCR unexpectedly contained {WEDGE_OCR_CLICK:?} — the HEADING of \
         {WEDGE_FILE_CLICK}, the pane that was CLICKED and then closed. A hit means `<C-w>c` did \
         not close the clicked pane, which usually means the click never moved Neovim's current \
         window: `pane_view.rs::on_pane_clicked` must call `core.focus_win(win)` for the clicked \
         pane. Without that, the whole premise of this smoke collapses — it would be closing the \
         pane that was never clicked, in which case no wedge could arise and the green `ALIVE2` \
         above proves nothing. The assertion is sound: after `:e {WEDGE_FILE_CLICK}` Neovim's \
         toast and the sidebar row both show the FILENAME, whose lowercase contains neither \
         {WEDGE_OCR_CLICK:?} nor {WEDGE_OCR_STAY:?}. PNG kept at {} for inspection. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
}

/// A user's divider drag becomes the LAYOUT'S WEIGHTS, so it survives the next
/// re-allocation of the paned that carries it.
///
/// ## What it proves
///
/// That a real `wl_pointer` drag of a `gtk::Paned` handle is written back into the
/// pure tree, and that the `max-position` re-apply then re-derives the divider from
/// THAT rather than from a ratio frozen when the widget was built. Those two are
/// one property from the outside: drag one divider, then re-allocate its paned by
/// dragging a different one, and see whether the first drag is still there.
///
/// Nothing here is a pixel assertion. Every observable is `winwidth()` for a window
/// Neovim reports about itself, so no OCR term and no fixture file is involved at
/// all — which is also why this smoke contributes no row to `ASSERTED_OCR_TERMS`.
///
/// ## How it finds the divider, and why not by arithmetic
///
/// The handle's width is theme-owned and its position is a fraction of the paned's
/// TRAVEL, which subtracts the handle and the far child's minimum — so nothing this
/// repo controls can predict it to within a handle width. The scenario therefore
/// BISECTS for it against Neovim's own `winnr()`, with a reset click before every
/// probe because a click that lands on the handle focuses no window at all. See
/// `bisect_divider` in [`POINTER_DIVIDER_BODY`]; the modelled seed from
/// [`divider_seed_span`] is a seed and nothing more.
///
/// ## Why assertion 5 cannot pass without the feature
///
/// The outer drag changes the INNER paned's allocation, which is the one thing that
/// changes its `max-position` and fires the re-apply. Without the write-back the
/// tree still holds equal thirds, the re-apply restores `0.5`, and the middle and
/// right panes come back equal — MEASURED in the red run below at 18 vs 19 cells,
/// against the 0.7 share the inner drag had just left them at (widths
/// `[14, 21, 9]`, which the write-back is what preserves).
///
/// Its second clause is what stops the first being vacuous, and the hazard is real:
/// `set_size_request` is a MINIMUM and a centred widget stays centred, so an
/// assertion sited at a midpoint can hold for any size. Here "the ratio did not
/// change" would be satisfied by a ratio that snapped back to `0.5` if the drag had
/// never taken effect — so the ratio is also required to sit at least
/// [`MIN_RATIO_OFFSET`] away from the centre, and assertion 2 has already excluded
/// a drag that did nothing.
///
/// ## Red-run revert (one line)
///
/// | assertion | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | 5 | delete the `notify::position` handler from `pane_view::install_paned_position` | assertions 1-4 stay GREEN — the drag itself is stock `gtk::Paned` behaviour and is measured before any re-layout — and assertion 5 fires on its ratio-preservation clause, with the middle and right panes back at equal widths | AS PREDICTED. Assertions 1-4 all passed; assertion 5 fired on the ratio-preservation clause: "the middle pane's share of the middle/right pair was 0.7 after the inner drag and 0.4864864864864865 after the outer one — a change of 0.21351351351351344, beyond the 0.08 that cell quantization can explain. … Widths after the inner drag [14, 21, 9], after the outer drag [7, 18, 19]." `0 passed; 1 failed` in 57.41s |
#[test]
fn divider_drag_survives_a_pane_relayout_under_cage() {
    const SMOKE: &str = "divider_drag_survives_a_pane_relayout_under_cage";
    // Columns the scenario builds, and therefore the denominator the bisection
    // seeds are modelled from.
    const COLUMNS: i64 = 3;
    // How many windows every width readback must report.
    const EXPECTED_WINDOWS: i64 = 3;
    // The inner drag leaves the left pane's allocation alone and `nvim_ui_try_resize_grid` does not
    // reflow, so it should not move; an outer-handle hit moves it six cells or more.
    const UNDISTURBED_CELLS: i64 = 2;

    let runtime_dir = require_smoke_env();

    // A fresh temp cwd like every other smoke. It stays EMPTY of fixtures: this
    // smoke opens no file and renders nothing it asserts on.
    let cwd = make_temp_cwd();

    let w_base = cwd.join("divider_widths_base.txt");
    let w_inner = cwd.join("divider_widths_inner.txt");
    let w_outer = cwd.join("divider_widths_outer.txt");
    let wins = cwd.join("divider_windows.txt");
    // The bisection's scratch file lives OUTSIDE the cwd: it is written and removed
    // once per probe, and the sidebar watches the cwd.
    let probe = runtime_dir.join("wolf_gnome_divider_relayout_probe.txt");
    let out_png = runtime_dir.join("wolf_gnome_divider_relayout_smoke.png");
    for path in [&w_base, &w_inner, &w_outer, &wins, &probe, &out_png] {
        clear_evidence(path, SMOKE);
    }

    let (d0_lo, d0_hi) = divider_seed_span(1, COLUMNS);
    let (d1_lo, d1_hi) = divider_seed_span(2, COLUMNS);
    let d0_lo_env = d0_lo.to_string();
    let d0_hi_env = d0_hi.to_string();
    let d1_lo_env = d1_lo.to_string();
    let d1_hi_env = d1_hi.to_string();
    let steps_env = BISECT_STEPS.to_string();
    let inner_env = DIVIDER_INNER_DRAG_PX.to_string();
    let outer_env = DIVIDER_OUTER_DRAG_PX.to_string();

    let status = run_pointer_scenario(
        POINTER_DIVIDER_BODY,
        &cwd,
        &runtime_dir,
        &out_png,
        &[
            ("PROBE_FILE", probe.as_os_str()),
            ("WINS", wins.as_os_str()),
            ("W_BASE", w_base.as_os_str()),
            ("W_INNER", w_inner.as_os_str()),
            ("W_OUTER", w_outer.as_os_str()),
            ("D0_LO", OsStr::new(&d0_lo_env)),
            ("D0_HI", OsStr::new(&d0_hi_env)),
            ("D1_LO", OsStr::new(&d1_lo_env)),
            ("D1_HI", OsStr::new(&d1_hi_env)),
            ("BISECT_STEPS", OsStr::new(&steps_env)),
            ("INNER_DRAG_PX", OsStr::new(&inner_env)),
            ("OUTER_DRAG_PX", OsStr::new(&outer_env)),
        ],
    );

    // This smoke's OWN exit codes first: the shared helper below names 11-15 and
    // would report a bisection failure as an unexplained non-zero status.
    assert!(
        !matches!(status.code(), Some(21..=23)),
        "{SMOKE}: the cage scenario exited {status}, which is a BISECTION precondition failure, not \
         an assertion failure. Exit 21: the seeds for divider 0 ({d0_lo}, {d0_hi}) both focused the \
         same window. Exit 22: the same for divider 1 ({d1_lo}, {d1_hi}). Exit 23: the two \
         bisections disagree about which window is the middle pane. The scenario's own stderr \
         names the four candidate causes in order — pane origin, cell size, handle width, the \
         equal-share assumption. Read `click_positions_the_cursor_under_cage` first: it derives \
         PANE_ORIGIN_X_EXPECTED = {PANE_ORIGIN_X_EXPECTED} and fails on its own if that moved. PNG \
         kept at {}.",
        out_png.display()
    );
    assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    assert_png_present(SMOKE, &out_png);

    // Extents first: a clamped injection lands elsewhere and misdirects every message below. The drag
    // end points are bounded by the seeds, so the extremes cover every x.
    let extents = read_output_extents(&cwd, SMOKE);
    for (label, x) in [
        ("the divider-0 LO seed", d0_lo),
        ("the divider-0 HI seed", d0_hi),
        ("the divider-1 LO seed", d1_lo),
        ("the divider-1 HI seed", d1_hi),
        (
            "the furthest RIGHT the inner drag can reach",
            d1_hi + DIVIDER_INNER_DRAG_PX,
        ),
        (
            "the furthest LEFT the outer drag can reach",
            d0_lo - DIVIDER_OUTER_DRAG_PX,
        ),
    ] {
        assert_inside_extents(SMOKE, label, (x, i64::from(PROBE_Y)), extents);
    }

    // Only for messages: nvim numbers windows by its layout, not by this front-end's pane order.
    let numbering = read_ints(&wins, 3, SMOKE);
    assert!(
        numbering[0] != numbering[1]
            && numbering[1] != numbering[2]
            && numbering[0] != numbering[2],
        "{SMOKE}: the bisections discovered window numbers {numbering:?} for the left, middle and \
         right panes, but two of them are the same — so at least one seed landed in a pane it was \
         not meant to and the widths below are not three different panes."
    );

    let w0 = read_ints(&w_base, 4, SMOKE);
    let w1 = read_ints(&w_inner, 4, SMOKE);
    let w2 = read_ints(&w_outer, 4, SMOKE);
    for (label, widths) in [
        ("the baseline", &w0),
        ("after the inner drag", &w1),
        ("after the outer drag", &w2),
    ] {
        assert_eq!(
            widths[3], EXPECTED_WINDOWS,
            "{SMOKE}: Neovim reports {} windows {label}, expected {EXPECTED_WINDOWS}. Two `<C-w>v` \
             must leave a flat row of three panes; a different count means a split did not land (or \
             one closed), and the three widths are not the three columns this smoke thinks they \
             are. Widths were {widths:?}, window numbering {numbering:?}. PNG kept at {}.",
            widths[3],
            out_png.display()
        );
    }

    // Assertion 1: three roughly equal columns, also the equal-share premise the bisection seeds use.
    let widest = w0[0].max(w0[1]).max(w0[2]);
    let narrowest = w0[0].min(w0[1]).min(w0[2]);
    assert!(
        widest - narrowest <= BASELINE_TOLERANCE_CELLS,
        "{SMOKE}: the three fresh panes are {:?} cells wide, a spread of {} — more than the \
         {BASELINE_TOLERANCE_CELLS} cells of slack a chain of paneds needs for its handles and \
         rounding. Equal weights are supposed to give equal widths: `geometry::divider_position` \
         scales by the paned's SPAN, so the only asymmetry left is the handle. Read a real spread \
         as a PRODUCT defect — scaling by `max_position` instead puts the end child's minimum \
         between the two panes at every fraction, which is what \
         `geometry::equal_weights_give_equal_widths` pins — and NOT as a tolerance to widen. It \
         also says the bisection seeds, which are modelled on equal shares, are aimed at the wrong \
         pixels. PNG kept at {}.",
        &w0[..3],
        widest - narrowest,
        out_png.display()
    );

    // --- Assertion 2: the inner drag actually moved the inner divider ----------
    assert!(
        w1[1] > w0[1] + MIN_DRAG_CELLS,
        "{SMOKE}: after dragging the inner divider {DIVIDER_INNER_DRAG_PX} px to the RIGHT the \
         middle pane is {} cells wide, up from {} — less than the {MIN_DRAG_CELLS} cells that \
         counts as movement. Read this FIRST as: THE PRESS MISSED THE DIVIDER HANDLE, which is a \
         harness failure and not a product one — the bisection converged on the wrong pixel, or \
         the handle is narrower than the two-pixel residual {BISECT_STEPS} halvings leave. Only \
         after ruling that out does it mean `gtk::Paned` stopped moving on a drag at all. Baseline \
         {:?}, after the drag {:?}, window numbering {numbering:?}. PNG kept at {}.",
        w1[1],
        w0[1],
        &w0[..3],
        &w1[..3],
        out_png.display()
    );
    assert!(
        w1[2] < w0[2] - MIN_DRAG_CELLS,
        "{SMOKE}: the middle pane grew but the RIGHT pane did not shrink ({} cells, from {}). The \
         two share one paned, so this means the drag resized something else — check that the \
         bisection found the INNER divider and not the outer one. Baseline {:?}, after the drag \
         {:?}. PNG kept at {}.",
        w1[2],
        w0[2],
        &w0[..3],
        &w1[..3],
        out_png.display()
    );

    // --- Assertion 3: the inner drag did not disturb the outer divider ---------
    assert!(
        (w1[0] - w0[0]).abs() <= UNDISTURBED_CELLS,
        "{SMOKE}: dragging the INNER divider moved the LEFT pane from {} to {} cells. The left pane \
         is the OUTER paned's start child, and resizing inside that paned's end child cannot move \
         it — so read this as: THE FIRST BISECTION FOUND THE WRONG HANDLE and this drag was \
         actually on divider 0. The other candidate is a write-back that named the wrong (path, \
         boundary) and a later re-apply propagating it. Either way assertion 5 below would be \
         measuring the wrong pair. Baseline {:?}, after the drag {:?}. PNG kept at {}.",
        w0[0],
        w1[0],
        &w0[..3],
        &w1[..3],
        out_png.display()
    );

    // --- Assertion 4: the outer drag actually moved the outer divider ----------
    assert!(
        w2[0] < w1[0] - MIN_DRAG_CELLS,
        "{SMOKE}: after dragging the outer divider {DIVIDER_OUTER_DRAG_PX} px to the LEFT the left \
         pane is {} cells wide, down from {} — less than the {MIN_DRAG_CELLS} cells that counts as \
         movement. As with assertion 2, read this first as THE PRESS MISSED THE DIVIDER HANDLE. \
         Without this drag the re-apply below is never provoked at all, so assertion 5 would prove \
         nothing. After the inner drag {:?}, after the outer drag {:?}. PNG kept at {}.",
        w2[0],
        w1[0],
        &w1[..3],
        &w2[..3],
        out_png.display()
    );

    // --- Assertion 5: THE assertion --------------------------------------------
    let ratio1 = head_width_ratio(
        w1[1],
        w1[2],
        SMOKE,
        "the middle/right pair after the inner drag",
    );
    let ratio2 = head_width_ratio(
        w2[1],
        w2[2],
        SMOKE,
        "the middle/right pair after the outer drag",
    );
    assert!(
        (ratio2 - ratio1).abs() <= RATIO_TOLERANCE,
        "{SMOKE}: the middle pane's share of the middle/right pair was {ratio1} after the inner \
         drag and {ratio2} after the outer one — a change of {}, beyond the {RATIO_TOLERANCE} that \
         cell quantization can explain. THE OUTER DRAG RE-ALLOCATED THE INNER PANED, and whatever \
         its `max-position` handler read is what those two panes now share. A ratio back at ~0.5 \
         means it re-derived from a ratio captured when the widget was built instead of from the \
         live tree — i.e. the drag was never written back. Assertions 2 and 4 already proved both \
         drags moved, so this is not a missed handle. Widths after the inner drag {:?}, after the \
         outer drag {:?}. PNG kept at {}.",
        (ratio2 - ratio1).abs(),
        &w1[..3],
        &w2[..3],
        out_png.display()
    );
    assert!(
        (ratio2 - 0.5).abs() >= MIN_RATIO_OFFSET,
        "{SMOKE}: the preserved ratio is {ratio2}, within {MIN_RATIO_OFFSET} of dead centre — so \
         the clause above (`the ratio did not change`) is VACUOUS here: an equal split satisfies it \
         whenever the drag never took effect. This clause is what keeps the assertion honest, and \
         it fires either because the inner drag was too small to be distinguishable from centre (a \
         FIXTURE problem — raise DIVIDER_INNER_DRAG_PX, never lower MIN_RATIO_OFFSET) or because \
         both panes really did come back equal. Widths after the outer drag {:?}. PNG kept at {}.",
        &w2[..3],
        out_png.display()
    );
}
