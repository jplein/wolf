//! Markdown-preview smokes: opening a preview and returning to the source, a link click,
//! an unsaved buffer's preview, the configured appearance, and a view created after launch.

use super::config::{APPEARANCE_FORCED_DARK, APPEARANCE_FORCED_LIGHT};
use super::frame_stats::{
    parse_frame_stats_blocks, sum_frame_stats_counts, FRAME_STATS_INTERVAL_SECS,
};
use super::preview_render::{
    PICKOVER_CONTROL_MIN_DARK_FRACTION, PICKOVER_CONTROL_MIN_INK_PX, PICKOVER_HEADING_INK,
    PICKOVER_OTHER_FILE, PICKOVER_OTHER_HEADING, PICKOVER_SOURCE_FILE, PICKOVER_SOURCE_HEADING,
    PICKOVER_SOURCE_PARAGRAPH,
};
use super::*;

// Markdown-preview fixtures: the in-place Preview hides the source, so only WebKit draws the body.
// See notes/crates/wolf-gnome/tests/smoke_headless/preview.rs.md#markdown-preview-fixtures

/// Markdown-preview smoke: the SOURCE file, opened in the split pane and then
/// replaced by its own preview.
///
/// Its name contains neither [`PREVIEW_OCR_BODY`] nor [`PREVIEW_OCR_STATUS`], so the
/// sidebar row and the `:e` toast that both spell `pvsrc.md` can satisfy no
/// assertion of this smoke's.
pub(crate) const PREVIEW_SOURCE_FILE: &str = "pvsrc.md";

/// Heading written into [`PREVIEW_SOURCE_FILE`]; lowercased it contains
/// [`PREVIEW_OCR_BODY`].
///
/// An `h1`, deliberately: `wolf_core`'s preview stylesheet sets `body` to 16 px and
/// `h1` to 32 px, and the asserted term has to survive `tesseract` at whatever size
/// WebKit rasterizes it — which is the ONE glyph source in this binary that is not the
/// pinned editor face. If this ever reads unreliably, make the heading LOWERCASE (the
/// shapes tesseract reads best, per [`SIDEBAR_FILLER_FILES`]) before touching the
/// assertion.
pub(crate) const PREVIEW_SOURCE_HEADING: &str = "# ZZBODYOSCAR";

/// The paragraph under [`PREVIEW_SOURCE_HEADING`], for page density INSIDE the web
/// view — the [`SIDEBAR_FILLER_COUNT`] hazard, aimed at the one region of this
/// capture the filler files do not reach.
///
/// Every word is checked against `ASSERTED_OCR_TERMS` by
/// `restore_fixture_names_cannot_satisfy_their_own_assertions`: this body is
/// RENDERED text, so a word carrying an asserted term would satisfy that assertion
/// from the preview with the heading never drawn.
pub(crate) const PREVIEW_SOURCE_PARAGRAPH: &str = "victor whiskey juliett mike, romeo sierra.";

/// Lowercased substring OCR MUST find in BOTH captures: the rendered heading of the
/// previewed document, which only the web view can draw.
pub(crate) const PREVIEW_OCR_BODY: &str = "zzbodyoscar";

/// Markdown-preview smoke: the OTHER file, opened in the starting pane and drawn by
/// the editor throughout the first capture.
pub(crate) const PREVIEW_OTHER_FILE: &str = "pvoth.md";

/// Heading written into [`PREVIEW_OTHER_FILE`]; lowercased it contains
/// [`PREVIEW_OCR_OTHER`].
pub(crate) const PREVIEW_OTHER_HEADING: &str = "# ZZOTHERTANGO";

/// Lowercased substring OCR MUST find in capture A: the editor pane's own file,
/// which is this capture's proof of life and its proof that the preview took over
/// exactly ONE of the two panes.
pub(crate) const PREVIEW_OCR_OTHER: &str = "zzothertango";

/// Lowercased substring OCR MUST find in capture A: the word a preview pane's
/// statusbar leads with.
///
/// It is [`wolf_gnome::preview_ui::PREVIEW_STATUS_PREFIX`]'s first word, re-typed
/// here as a literal per this binary's re-declare-do-not-import discipline. Reading the
/// crate's own constant would make the assertion a comparison with itself and would
/// hold for whatever that constant became — including the `[No Name]` a preview
/// window's `WolfCore::pane_status` answers, which is the value this assertion exists
/// to rule out.
///
/// **It is an ordinary English word**, unlike every other term in
/// `ASSERTED_OCR_TERMS`, so the usual "no fixture may be named after it" rule is
/// not enough on its own: the rendered document, the paragraph above and both
/// filenames are checked against it too.
pub(crate) const PREVIEW_OCR_STATUS: &str = "preview";

/// Typed (in insert mode, after Edit Source has put the source back in a pane) into
/// the SOURCE buffer while the preview pane is live — the keystrokes whose only route
/// to a pixel is the JSON body swap.
///
/// An `h1` for the same OCR reason [`PREVIEW_SOURCE_HEADING`] is.
pub(crate) const PREVIEW_LIVE_MARKER: &str = "# ZZLIVEKILO";

/// Lowercased substring OCR MUST find in capture B: [`PREVIEW_LIVE_MARKER`] as the
/// web view rendered it.
pub(crate) const PREVIEW_OCR_LIVE: &str = "zzlivekilo";

/// Typed into the palette's search entry to reach the `Preview` row.
///
/// ## Why it is a SUBSEQUENCE rather than a prefix
///
/// `wolf_core::rank_matches` matches a term as a case-insensitive SUBSEQUENCE, and
/// `prv` selects exactly TWO of the palette's actionable titles: `Preview` and
/// `Preview in Browser`. Nothing else carries `p`, then `r`, then `v` —
/// `Buffer Picker…` and `Search in project` have `p` then `r` with no later `v`;
/// `Copy`, `Paste`, `Open File…` and `Open Folder…` have a `p` but no usable `r`; the
/// rest have no `p` at all.
///
/// The literal `preview` would work too and is deliberately NOT used: it is
/// [`PREVIEW_OCR_STATUS`], so a palette that failed to CLOSE would leave the typed
/// query on screen and satisfy the statusbar assertion with no preview pane in
/// existence. `prv` cannot.
///
/// ## The three rows are NOT separable, so this query relies on the RANKING
///
/// `Preview in Browser` and `Preview in Another Pane` are dispatched too, so the
/// palette's classification filter removes no row, and no query can separate them:
/// `Preview` is a PREFIX of both, so every subsequence of the first is a subsequence
/// of the others. What makes the preview smokes deterministic is that `Preview` ranks
/// FIRST — the rows tie on every ranking key down to display LENGTH, where 7
/// characters beat 18 and 22 — and that Enter activates the preselected top row.
///
/// That premise is pinned executably in the fast suite by
/// `palette::tests::palette_rows_for_the_preview_smoke_rank_the_named_command_first`,
/// so a registry change reddens THAT rather than making these smokes stochastic. It
/// also pins the one state that inverts the order: a command MRU holding
/// `preview_in_browser` puts that row first, because an MRU hit outranks every
/// match-quality key. These smokes are safe from it only because
/// [`smoke_state_dir`] gives each run a fresh store and neither ever activates that
/// command — which is also why no smoke here may EVER be extended to run it (on a host
/// whose URI launcher works it opens a tab in the developer's real browser, since
/// `XDG_DATA_DIRS` reaches the system applications directory even under
/// [`cage_command`]'s `HOME` redirect).
pub(crate) const PREVIEW_QUERY: &str = "prv";

/// Typed into the palette's search entry to reach the `Preview in Another Pane` row.
///
/// Every smoke that needs the preview BESIDE its source types this rather than
/// [`PREVIEW_QUERY`], and the reason is the whole pane model: `Preview` binds the
/// preview into the ACTIVE pane, so from a one-pane window it leaves the preview
/// alone on screen with no editor pane, no control band and no right half. Only the
/// other-pane command splits, and a split is what every derived rectangle in those
/// scenarios is measured against.
///
/// Unlike [`PREVIEW_QUERY`] this really is UNIQUE — `prvan` is a subsequence of
/// `Preview in Another Pane` and of no other title, because neither `Preview` nor
/// `Preview in Browser` carries an `a` after its `v`. It therefore rests on
/// uniqueness rather than on a ranking, which is one fewer premise; the fast suite
/// pins that in
/// `palette::tests::palette_rows_for_the_preview_smoke_rank_the_named_command_first`
/// alongside the other two.
pub(crate) const PREVIEW_OTHER_PANE_QUERY: &str = "prvan";

/// Typed into the palette's search entry to reach the `Edit in Another Pane` row.
///
/// `ednap` is a subsequence of `Edit in Another Pane` and of no other palette title,
/// actionable or not — `Preview in Another Pane`, the one title that shares its tail,
/// carries no `d` before its `n`. Unlike [`PREVIEW_QUERY`] it really is UNIQUE — one
/// row, not a top row — which is why its unit-test row claims an unfiltered floor and
/// an actionable row count of one rather than the other's two.
///
/// It reaches the OTHER-PANE Edit rather than the in-place one because the live-edit
/// leg needs the source in a window WITHOUT losing the preview: the in-place
/// `edit_source` replaces the preview it is fired from, which would leave steps 6
/// onward with no preview pane to observe.
const EDIT_QUERY: &str = "ednap";

/// The two canonical `CommandSpec::id`s the palette records when the `Preview` and
/// `Edit in Another Pane` rows are activated — the smoke's proof that the two queries
/// reached the rows they name.
///
/// A PRECONDITION rather than a discriminator, and the ordering is why:
/// `PaletteInner::activate_selection` records BEFORE it dispatches, so a hit here says
/// the right row was activated and says nothing about what `shell::perform` then did
/// with it. `command_palette_chord_runs_the_selected_command_under_cage` is where
/// the `record` call itself is discriminated.
///
/// `palette::tests::palette_rows_for_the_preview_smoke_rank_the_named_command_first`
/// pins the same two ids against the same two queries in the fast suite, which is the
/// join that keeps this literal honest.
const PREVIEW_MRU_KEYS: [&str; 2] = ["preview", "edit_in_other_pane"];

/// Name of the file the captive Neovim writes its window/buffer list into AFTER the
/// Preview leg. It lands in the temp cwd, so the sidebar draws its name — hence the
/// `RELAUNCH_FIXTURE_NAMES` registration and the `pv` prefix that keeps it clear of
/// every asserted term.
pub(crate) const PREVIEW_READBACK_A_NAME: &str = "pvwinsa.txt";

/// Name of the same readback taken again AFTER the Edit-in-another-pane leg.
pub(crate) const PREVIEW_READBACK_B_NAME: &str = "pvwinsb.txt";

/// The prefix `runtime/nvim/init.lua`'s `wolf_ensure_preview_buf` gives a preview
/// scratch buffer, before the source's cwd-relative path.
///
/// Re-typed here rather than imported from `wolf_gnome::preview_ui`, and the
/// distinction is not pedantry: the string this smoke reads back is NEOVIM's buffer
/// NAME, produced by the Lua, while the crate's constant is what the front-end's
/// STATUSBAR prints. They are two ends of one contract, and importing one to check
/// the other would make the readback assertion hold by construction.
pub(crate) const PREVIEW_READBACK_PREFIX: &str = "Preview: ";

/// Milliseconds `wtype` waits between the keystrokes of step 6's live-edit burst,
/// handed to the scenario as `TYPE_DELAY_MS`.
///
/// **Coupled to [`PREVIEW_TRAILING_DELAY`], and the coupling is what makes assertion
/// 11 reach the coalescer's trailing edge at all.** At `wtype`'s default rate the
/// whole marker can land inside one flush and be carried by a single leading-edge
/// render, so the smoke would stay green with the coalescer arming nothing. Paced
/// BELOW the trailing delay, the `o` takes the leading edge, the marker's characters
/// coalesce, and only a fire after the LAST of them carries the complete word onto
/// the page.
///
/// Raise it above the trailing delay and every keystroke becomes its own leading
/// edge again: the smoke stays green and stops discriminating, with nothing in the
/// run naming the number that disarmed it. That is what
/// [`preview_burst_delay_stays_below_the_coalescer_trailing_delay`] refuses.
///
/// The number lives HERE and nowhere else — the scenario reads it out of the
/// environment rather than assigning it, so `set -u` aborts the run if it fails to
/// arrive instead of silently typing at the default rate.
const PREVIEW_TYPE_DELAY_MS: u64 = 40;

/// The text of the SUSTAINED burst step 9 types, handed to the scenario as
/// `PREVIEW_CAP_FILLER`.
///
/// Sixty characters at [`PREVIEW_TYPE_DELAY_MS`] is 2.70-2.76 seconds of unbroken
/// typing, measured under `cage` rather than multiplied out: `wtype` paces SLOWER
/// than the delay it is given, by a near-constant ~5.5 ms a key — 45.8 ms measured at
/// `-d 40` and 35.2 ms at `-d 30` — so every burst here runs about a seventh longer
/// than its nominal length. That is what forces the staleness cap: the trailing timer never fires
/// inside a burst that long, because every arm is superseded before it elapses, so
/// every render during it is a cap render. Measured, this burst pays FOUR renders
/// with the cap against ONE without it, which is the whole of assertion 13's
/// discrimination. The marker leg cannot do that job — at thirteen keys it is one
/// trailing delay's worth of typing, and a leading edge plus one trailing fire carry
/// all of it.
///
/// **It reaches no capture and no disk.** Both `grim` calls are already done when it
/// is typed, and the buffer it goes into is never written, so it can neither satisfy
/// nor corrupt an OCR term — which is also why it needs no
/// `RELAUNCH_FIXTURE_NAMES` row. It is ONE repeated letter for the same reason:
/// a run of a single character spells no word at all, so the property holds however
/// the fixture-naming rules are read.
///
/// Written as SIX GROUPS OF TEN rather than one run, so its length is countable by
/// eye: the arithmetic in [`PREVIEW_CAP_RENDER_FLOOR`] rests on how many keystrokes
/// it is, and a run of sixty identical characters is exactly the thing a reader
/// cannot check.
const PREVIEW_CAP_FILLER: &str = concat!(
    "xxxxxxxxxx",
    "xxxxxxxxxx",
    "xxxxxxxxxx",
    "xxxxxxxxxx",
    "xxxxxxxxxx",
    "xxxxxxxxxx"
);

/// This smoke's own frame-time report file, under the resolved `XDG_RUNTIME_DIR`
/// rather than in the temp cwd, for the reason `FRAME_STATS_FILE_NAME` gives.
const PREVIEW_FRAME_STATS_FILE_NAME: &str = "wolf_gnome_preview_frame_stats.txt";

/// The bucket label assertion 12 sums, re-typed as a LITERAL.
///
/// It is `wolf_gnome::frame_stats::Bucket::PrevRender`'s label — one blocking
/// `WolfCore::render_preview` call, alone, and the only row of the report that
/// counts that RPC. Re-typed rather than imported for this binary's usual reason, and
/// deliberately not indexed out of `FRAME_STATS_ABSENT_LABELS`, where the same
/// string carries the opposite requirement.
const PREVIEW_RENDER_LABEL: &str = "prev.render";

/// The two labels assertion 14 compares, re-typed as LITERALS for the reason
/// [`PREVIEW_RENDER_LABEL`] is.
///
/// `pane.prev` is the whole preview refresh pass and `pane.wake` is one tab's
/// pane-view wake. A pass is opened from TWO places — every pane wake that reaches it,
/// and the preview coalescer's one-shot fire — which is what makes the first count
/// able to exceed the second, and what assertion 14 reads.
const PREVIEW_PANE_PREV_LABEL: &str = "pane.prev";

/// The wake side of that comparison. See [`PREVIEW_PANE_PREV_LABEL`].
const PREVIEW_PANE_WAKE_LABEL: &str = "pane.wake";

/// The ceiling assertion 12 holds the summed `prev.render` count to.
///
/// **MEASURED at both ends.** Three things move the source's `changedtick` here, and
/// nothing else does: the cold open, step 6's thirteen-key marker burst, and step 9's
/// sixty-key [`PREVIEW_CAP_FILLER`] burst. Coalesced, the whole run reads SEVEN or
/// EIGHT — 7, 7 and 8 over three runs, broken down per leg in
/// [`PREVIEW_CAP_RENDER_FLOOR`]. With `request_preview_render` calling the render pass
/// unconditionally — the uncoalesced per-wake render, and the 15.6 % of wall
/// clock `notes/wolf-gnome-decisions.md` records — the same scenario reads SIXTY-ONE.
///
/// Twenty sits between the two and excludes each end for its own reason. It is twelve
/// above the coalesced reading, which no plausible pacing closes: `wtype` running
/// slower than it asks for stretches step 9's burst and so ADDS cap renders, but at
/// one per `preview_coalescer::PREVIEW_MAX_STALENESS` that burst would have to last
/// some six seconds longer than the ~2.4 it does. And it is forty-one below the
/// uncoalesced reading, so no per-keystroke build can hide under it.
///
/// Deliberately NOT a ratio against the report's `keys=` counter: the scenario also
/// types two ~150-character `:call writefile(…)` lines, which bump no source tick and
/// would swamp any ratio.
///
/// Raise it only if a real COALESCED run exceeds it, and never to admit an
/// uncoalesced one — that single edit is what would turn this assertion off.
const PREVIEW_RENDER_BUDGET: u64 = 20;

/// The floor assertion 13 holds the same sum to: the count below which the
/// STALENESS CAP cannot have fired.
///
/// **MEASURED at both ends.** A capped run reads SEVEN or EIGHT — 7, 7 and 8 over
/// three runs: the cold open pays one, step 6's marker leg two, and step 9's sustained
/// burst four or five — its leading edge, two or three cap renders and the trailing
/// fire after its last key, read block by block out of the report. With the cap gone —
/// `delay` the trailing delay alone, the `min` against the cap's remainder dropped —
/// the same run reads FOUR, and step 9's burst pays one.
///
/// Six is what separates them, and the exclusion rests on an argument rather than on
/// this host's timing: with no cap, no burst in this scenario can pay more than its
/// leading edge and one trailing fire, so the whole run cannot exceed one for the open
/// plus two for the marker plus two for the sustained burst — five, whatever the
/// pacing does. Six refuses every reading a cap-less build can produce and still
/// leaves the capped reading a render of margin.
///
/// It carries [`PREVIEW_RENDER_BUDGET`]'s non-vacuity as well: a build that renders
/// nothing at all reads ZERO, and no ceiling refuses that. There is no second, weaker
/// floor beside this one — one number cannot honestly carry two derivations, and a
/// floor of six implies every weaker one.
///
/// **The margin here is one render, the thinnest in this smoke: green runs read 7, 7
/// and 8. If it ever flakes, lengthen [`PREVIEW_CAP_FILLER`] — never lower the
/// floor.** A longer burst buys margin honestly, at one more cap render per
/// `preview_coalescer::PREVIEW_MAX_STALENESS` of typing, and it moves the capped
/// reading alone. Lowering the floor moves it toward the five a cap-less run can
/// reach, and at five it stops separating the two readings at all — which is the
/// whole of the cap's end-to-end evidence, turned off by one character.
const PREVIEW_CAP_RENDER_FLOOR: u64 = 6;

/// The MARKDOWN-PREVIEW scenario, run as a single `sh -c` string inside `cage` (the
/// `PALETTE_SCENARIO` shape, with `STATUS_SCENARIO`'s `<C-w>v` split and
/// `BUFPICK_SCENARIO`'s `wait_file`): launch + poll, settle, open the OTHER file,
/// park the block cursor, `<C-w>v`, open the SOURCE in the new pane, drive Preview
/// through the palette, capture A, read the window list back, freeze `state.json`,
/// drive Edit Source through the palette, read the window list back again, type the
/// live marker into the source, hide the source behind the other file, capture B, and
/// then — with every pixel and every readback already on disk — type one SUSTAINED
/// burst into the source, which is what forces the coalescer's staleness cap.
///
/// ## The two budgets that are not settles
///
/// `PREVIEW_SETTLE` is measured FROM THE CALL, not from `LoadEvent::Started`. A cold
/// preview open costs 254.57 / 256.51 / 265.22 ms call→`Finished` on the measurement
/// host, of which about 110 ms elapses before `Started` fires at all (spawning
/// WebKit's WebProcess), so a `Started`-anchored figure hides nearly half of it. The
/// value here is roughly eight times the measured cold open. Steady-state reloads
/// have no such gap — the body swap itself is 5.3-12.4 ms — but the same budget is
/// spent after the live edit, because what has to land there is a keystroke, a
/// notification, a wake, a blocking `render_preview` and a JavaScript evaluation.
///
/// ## The live edit is PACED, and the pacing is load-bearing
///
/// Step 6 types the marker with `wtype -d "$TYPE_DELAY_MS"` rather than at `wtype`'s
/// default rate, because at the default rate the whole burst can land inside one
/// flush and be carried by a single LEADING-EDGE render — under which assertion 11
/// passes with the preview coalescer arming nothing at all. Paced below
/// [`PREVIEW_TRAILING_DELAY`], the `o` takes the leading edge, the marker's
/// characters coalesce, and only a fire after the last of them carries the complete
/// word onto the page. See [`PREVIEW_TYPE_DELAY_MS`] for the coupling and for the
/// guard that keeps it.
///
/// The premise underneath is `wtype`'s own: that `-d` spaces the key events it
/// synthesizes rather than merely delaying the first. It is not taken on trust —
/// assertion 12's uncoalesced red run measures it, at 61 renders against a green
/// run's 7 or 8. A burst that still arrived inside one flush could not produce that
/// number under any revert.
///
/// Step 9's burst runs at the same pace for a different property. There the pacing
/// has to hold for long enough that the trailing timer never elapses at all, which is
/// what leaves the staleness cap as the only thing that can render during it. See
/// [`PREVIEW_CAP_FILLER`].
///
/// ## Why the block cursor is parked in only ONE pane
///
/// The convention is to park it off every ASSERTED row. In the starting pane it is
/// parked the usual way (`o` then Escape), because that pane's heading is asserted in
/// capture A. The split pane needs no park at all: by capture time it is a PREVIEW
/// pane, and `draw_pane` skips the grid, the scrollbar and the cursor for one — so
/// there is no block to sit anywhere, and everything asserted in it is either web-view
/// pixels or the statusbar strip.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention). A
///   LAUNCH failure; nothing else in this smoke is about the preview.
/// * `14` — a window-list readback the captive Neovim was told to write never
///   appeared, so the leg it belongs to cannot be checked at all.
/// * `16` — no `state.json` existed to freeze while the preview pane was live.
///   DISTINCT from "the persisted leaf is wrong", which exits 0 and fails an
///   assertion: this one means nothing was persisted at all.
const PREVIEW_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for a readback file.
FILE_TRIES=100
#
# PREVIEW_SETTLE -- budgeted FROM THE CALL, not from the web view's Started event.
# A cold open is ~255 ms call->Finished (about 110 ms of it spent spawning the
# WebProcess before Started fires), and the live-edit swap has to carry a keystroke
# through a notification, a wake, a blocking render_preview and a JS evaluation.
# See the doc comment above before trimming it.
PREVIEW_SETTLE=2.0
#
# TYPE_DELAY_MS -- how long wtype waits between the keystrokes of step 6's burst.
# It is NOT assigned here: it arrives from the Rust side (PREVIEW_TYPE_DELAY_MS),
# so the number lives in exactly one place and `set -eu` aborts the run if it fails
# to arrive rather than quietly typing at wtype's default rate.
#
# It is COUPLED to wolf_gnome::preview_coalescer::PREVIEW_TRAILING_DELAY and must
# stay BELOW it. Above it, every keystroke of the burst is its own leading edge,
# the trailing edge stops being what puts the marker on screen, and assertion 11
# goes back to passing over a coalescer that arms nothing. The guard
# preview_burst_delay_stays_below_the_coalescer_trailing_delay is what keeps the
# two in step; PREVIEW_TYPE_DELAY_MS carries the rest of the reasoning.

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome markdown-preview smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there. (The STATUS_SCENARIO body, verbatim.)
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
    echo "wolf-gnome markdown-preview smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 1: the OTHER file in the starting pane -----------------------------
#
# It stays in this pane for the whole first half, so capture A has an EDITOR pane
# beside the preview pane -- which is what makes "the preview replaced exactly one
# of the two panes" observable at all.
wtype -- ":e $PREVIEW_OTHER_FILE"
wtype -k Return
sleep "$SETTLE"

# Park the block cursor off row 0 of THIS pane: its heading is asserted in capture
# A, and the inverted block corrupts the cell it covers.
wtype -- o
wtype -k Escape
sleep "$SETTLE"

# --- Step 2: split, and open the SOURCE in the new pane ----------------------
#
# The `<C-w>v` CHORD, never a typed `:vsplit`: the init.lua keymap is what emits the
# wolf_layout Split intent the front-end binds a new pane from. A typed `:vsplit`
# reaches the same abbreviation, but the chord is the path a user takes and the one
# every sibling split smoke drives.
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"

wtype -- ":e $PREVIEW_SOURCE_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 3: drive PREVIEW through the command palette -----------------------
#
# Ctrl+Shift+P, the query, Enter. Preview has no accelerator row of its own -- the
# palette and the :WolfPreview Ex command are its only routes -- so this is also the
# only end-to-end exercise `shell::perform`'s UiAction::OpenPreview arm gets.
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$PREVIEW_QUERY"
sleep "$SETTLE"
wtype -k Return

# The COLD open budget: the blocking open_preview RPC, the wake that carries the
# wolf_preview notification, the lazily-created WebView's WebProcess spawn, the
# blocking render_preview and the full load_html.
sleep "$PREVIEW_SETTLE"

# --- Step 4: capture A, with the preview pane live ---------------------------
grim "$OUT_PNG_BEFORE"

# --- The first readback: NEOVIM's own answer about what its windows hold -----
#
# Taken AFTER the capture on purpose: the typed command line is drawn by the cmdline
# strip, and capture A must carry the `:e` toast rather than this path. Joined with a
# COMMA rather than a space because a preview scratch's own name contains one (it is
# "Preview: <cwd-relative path>"), and split back on the comma by the Rust side, which
# sorts before comparing -- so the assertion is on the SET of buffers and does not
# depend on which side of the split nvim put the new window.
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ',')], '$PREVIEW_READBACK_A')"
wtype -k Return
wait_file "$PREVIEW_READBACK_A" || exit 14

# --- Freeze the persisted layout while the preview pane is still live --------
#
# Persistence runs on every render wake, so the live file is rewritten as soon as the
# Edit leg below lands -- at which point BOTH leaves name the source and the
# editor-leaf half of the assertion would prove nothing. Copying here is what pins the
# one moment where the two leaves differ. The write itself is a tmp-file rename, so a
# copy taken from a running app is never torn.
if [ ! -s "$PREVIEW_STATE_SRC" ]; then
    echo "wolf-gnome markdown-preview smoke: no state.json at $PREVIEW_STATE_SRC." >&2
    echo "  Nothing was persisted at all, so this is not the preview flag: suspect" >&2
    echo "  may_snapshot refusing every wake (snapshot_readiness counts previews too)," >&2
    echo "  open_state_store resolving no directory, or WOLF_STATE_DIR not reaching" >&2
    echo "  the app." >&2
    exit 16
fi
cp "$PREVIEW_STATE_SRC" "$PREVIEW_STATE_COPY"

# --- Step 5: drive EDIT IN ANOTHER PANE through the same palette -------------
#
# The active pane IS the preview. `edit_in_other` with a target of 0 SPLITS and binds
# the source into the new pane, moving focus there and leaving the preview standing.
# That is what puts the source in a window the next step can type into WITHOUT losing
# the preview the rest of this scenario observes — which the in-place `edit_source`
# would replace. After the split the preview is shown by a view created then, on a host
# not yet allocated; capture B is what says that view rendered (assertion 10).
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$EDIT_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$SETTLE"

# The second readback, same shape as the first.
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ',')], '$PREVIEW_READBACK_B')"
wtype -k Return
wait_file "$PREVIEW_READBACK_B" || exit 14

# --- Step 6: the LIVE EDIT ---------------------------------------------------
#
# `G` to the last line, `o` to open one below it, then the marker. This is the ONLY
# step that reaches the body-swap branch: the first load into a view goes through
# load_html, and a payload encoded once instead of twice throws inside JSON.parse
# and leaves the previous document on screen.
#
# The marker is typed at TYPE_DELAY_MS per key rather than at wtype's default rate,
# and that pacing is load-bearing: unpaced, the whole burst can land inside one
# flush and be carried by a single leading-edge render, so this step would prove
# nothing about the coalescer's trailing edge. See the doc comment above.
wtype -- G
wtype -d "$TYPE_DELAY_MS" -- "o$PREVIEW_LIVE_MARKER"
wtype -k Escape
sleep "$PREVIEW_SETTLE"

# --- Step 7: take the source's RAW text back off the screen ------------------
#
# The marker was typed into a pane nvim is drawing, so without this the capture would
# carry it twice and an OCR hit would prove nothing about the web view. Opening the
# other file over it leaves the marker readable in exactly one place. The source
# buffer stays loaded and modified (nvim's 'hidden' default), so the preview keeps
# rendering from it.
wtype -- ":e $PREVIEW_OTHER_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 8: capture B, with the swapped document live -----------------------
grim "$OUT_PNG"

# --- Step 9: a SUSTAINED burst, to force the STALENESS CAP -------------------
#
# It lands here and nowhere else. Everything read from a pixel is already on disk --
# both captures taken, both readbacks written, state.json frozen back at step 4 --
# so a burst at this point can perturb no assertion above it. Beside the marker it
# would land inside an OCR'd capture instead, which is why it is not there.
#
# What it forces: PREVIEW_CAP_FILLER is sixty keys, and at TYPE_DELAY_MS that is
# roughly 2.4 seconds of CONTINUOUS typing. The trailing timer never elapses inside
# a burst that long -- every arm is superseded by the next keystroke -- so every
# render during it is a CAP render, one per the coalescer's maximum staleness.
# Without the cap the same burst can pay at most its leading edge and the trailing
# fire after its last key, and measured it pays ONE; with the cap it pays FOUR. That
# gap is what assertion 13 reads, and the marker leg cannot produce it: thirteen keys
# is one trailing delay's worth of typing, which a leading edge and a trailing fire
# already carry.
#
# Re-opening the source is a WINDOW SWITCH, not an edit. The buffer is still loaded
# and modified, `:e` on an already-loaded buffer does not re-read it from disk, and
# the preview's reload is keyed on the source's changedtick -- so the switch itself
# renders nothing. That is the same premise step 7 rests on, in the other direction.
wtype -- ":e $PREVIEW_SOURCE_FILE"
wtype -k Return
sleep "$SETTLE"

wtype -- G
wtype -- o
wtype -d "$TYPE_DELAY_MS" -- "$PREVIEW_CAP_FILLER"
wtype -k Escape

# At least PREVIEW_SETTLE, so the burst's own trailing fire has certainly landed
# before the report below is flushed: a render that has not happened yet is a render
# assertion 13 cannot count.
sleep "$PREVIEW_SETTLE"

# --- Step 10: force out the frame-stats report covering both bursts ----------
#
# The instrument arms no timer: a report is emitted from the LAST statement of the
# drain-loop body, so it rides a wake. With the app idle after the capture, the
# interval holding the burst's renders would be carried by the next wake -- and the
# teardown below signals the app, so flush_final never runs and that interval would
# never be written at all. Assertion 12 would then read a file missing exactly the
# rows it exists to count.
#
# The sleep exceeds WOLF_FRAME_STATS_INTERVAL (set on the Rust side) so the boundary
# has certainly passed, then a NORMAL-MODE Escape supplies the wake. Escape is the
# cheapest wake there is and the only key that can be pressed here safely: it edits
# no buffer, moves no cursor and draws no message, so it can satisfy nothing else in
# this smoke -- and both captures are already on disk in any case.
sleep 1.2
wtype -k Escape
sleep 0.3

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

// Preview-link-click fixtures live in a subdirectory, so a relative href cannot resolve by luck.
// See notes/crates/wolf-gnome/tests/smoke_headless/preview.rs.md#preview-link-click-fixtures

/// Preview-link smoke: the subdirectory of the temp cwd both fixtures live in.
///
/// **Load-bearing, not tidiness.** It is the one thing that makes the base URI
/// FALSIFIABLE: it puts the directory the link resolves in somewhere other than the
/// captive Neovim's working directory, so a `None` base — which delivers the raw
/// relative href — resolves to a path that does not exist. Moving either fixture back
/// to the project root silently disarms the second falsifier on
/// [`preview_link_click_repoints_the_pane_under_cage`], which is exactly how that
/// falsifier was found to be no falsifier at all.
const LINK_FIXTURE_DIR: &str = "lnkdir";

/// Preview-link smoke: the SOURCE file, whose first line is one wide link, as a path
/// RELATIVE TO THE TEMP CWD.
///
/// It carries the [`LINK_FIXTURE_DIR`] prefix because the readback asserts on this
/// exact string twice over: `:e` is given it verbatim, and
/// `wolf_ensure_preview_buf` names the preview scratch
/// `Preview: <cwd-relative path>`, which for this source is
/// `Preview: lnkdir/lnkfrom.md`.
pub(crate) const LINK_SOURCE_FILE: &str = "lnkdir/lnkfrom.md";

/// Preview-link smoke: the file the link points at, as a path relative to the temp
/// cwd — the answer a CORRECT base URI produces.
pub(crate) const LINK_TARGET_FILE: &str = "lnkdir/lnkinto.md";

/// The href written into [`LINK_SOURCE_LINE`]: the target named RELATIVE TO THE
/// SOURCE, so it carries no directory component at all.
///
/// ## The arithmetic, which is the whole point of the subdirectory
///
/// With the base URI in place, WebKit resolves this against
/// `file://<cwd>/lnkdir/lnkfrom.md` — RFC 3986 replaces the last segment — and the
/// handler is given `<cwd>/lnkdir/lnkinto.md`, which exists.
///
/// With a `None` base the handler is given this string RAW (measured:
/// `uri="lnkinto.md"`), `classify` answers its schemeless row, and Lua's
/// `filereadable` resolves it against the captive Neovim's cwd — `<cwd>/lnkinto.md`,
/// which does NOT exist. `wolf_preview_file` answers 0, the pane is left alone, and
/// the readback assertion reddens.
///
/// Both halves are checked executably in the smoke rather than trusted: this string
/// must appear in [`LINK_SOURCE_LINE`] as a whole markdown destination `](…)`, must
/// carry no path separator, must equal [`LINK_TARGET_FILE`] minus its
/// [`LINK_FIXTURE_DIR`] prefix, and the file the wrong resolution would find must be
/// absent from the temp cwd. Put a directory back into the href and both resolutions
/// succeed again.
const LINK_TARGET_HREF: &str = "lnkinto.md";

/// First line of [`LINK_SOURCE_FILE`], MEASURED as the geometry
/// [`LINK_CLICK_X`] / [`LINK_CLICK_Y`] were derived against.
///
/// The link TEXT is what decides the rendered rectangle; the href is not drawn at
/// all. Twenty-four `L`s at the preview stylesheet's 16 px body size span 215 px, and
/// `L` is chosen because it is a tall, unambiguous glyph whose stroke gives the
/// blue-dominant predicate plenty of pixels to find. **Changing the link text moves
/// the cluster and invalidates the click constants**; the calibration assertion is
/// what turns that into a named failure rather than a silent miss.
///
/// The href half is [`LINK_TARGET_HREF`] spelled out, because a const cannot
/// interpolate another; the smoke asserts the two agree.
const LINK_SOURCE_LINE: &str = "[LLLLLLLLLLLLLLLLLLLLLLLL](lnkinto.md)";

/// Body of [`LINK_TARGET_FILE`]. Never OCR'd — the readback is the observable — but
/// non-empty so the followed preview has a document to render.
const LINK_TARGET_BODY: &str = "# ZZTARGETQUEBEC\n\nkilo lima mike, november oscar.";

/// Name of the file the captive Neovim writes its window/buffer list into BEFORE the
/// click.
pub(crate) const LINK_READBACK_BEFORE_NAME: &str = "lnkwina.txt";

/// Name of the same readback taken AFTER the click.
pub(crate) const LINK_READBACK_AFTER_NAME: &str = "lnkwinb.txt";

/// The BLUE channel a pixel must exceed to be a candidate for the rendered link
/// colour.
///
/// The dark theme's link colour is `#4ea1ff` (`wolf-core`'s
/// `markdown/themes/default.css`) against a `#1b1b1b` background and `#d4d4d4` body
/// text, and [`SMOKE_APPEARANCE`] pins the appearance to `dark`, so a high blue
/// channel alone already separates the link from everything else the document draws.
const LINK_BLUE_MIN_B: u8 = 150;

/// How far the blue channel must exceed the RED channel for a pixel to count as link
/// ink.
///
/// This is the half that rejects the theme's near-grey body text: `#d4d4d4` has a
/// blue channel of 212, comfortably over [`LINK_BLUE_MIN_B`], and only the
/// blue-over-red margin tells it apart from the link. `#4ea1ff` clears it by 177.
const LINK_BLUE_MIN_MARGIN: u16 = 60;

/// How far inside the preview pane's LEFT and RIGHT edges the link search starts.
///
/// It keeps the search off both pane boundaries: the `gtk::Paned` handle and the
/// editor pane beyond it on the left, the window edge on the right. Nothing there is
/// part of the rendered document, and a pane-spanning mark inside the rectangle would
/// swell the located cluster to the whole pane and make the containment assertion below
/// pass for any click point at all — which is what [`LINK_CLUSTER_MAX_W_PX`] fires on.
///
/// **[`BAND_X_INSET_PX`] cannot be reused here**, and the reason is measured: it is 80
/// px, while the link's first glyph sits 22 px right of the pane boundary, so an 80 px
/// inset would start the search 57 px INSIDE the link and cut the cluster in half. Ten
/// pixels clears the boundary and still leaves the whole link inside.
const LINK_SEARCH_INSET_X_PX: f64 = 10.0;

/// How far below the preview pane's own TOP edge the link search starts.
///
/// The same boundary clearance as [`LINK_SEARCH_INSET_X_PX`], with more room because
/// there is nothing to lose: the pane's top edge is at y = 47 and the link's first row
/// is at y = 165, so 40 px of clearance costs nothing and keeps the pane's own top
/// boundary well outside the rectangle. The search's BOTTOM is
/// [`BAND_BOTTOM_CLEARANCE_PX`] above the output, which that constant's own doc comment
/// explains — it clears the window's full-width command-line strip, where a Neovim
/// message could draw in a colour this binary takes no position on.
const LINK_SEARCH_INSET_Y_PX: f64 = 40.0;

/// x of the point injected at the rendered link, in absolute output pixels.
///
/// MEASURED 2026-08-06 under `cage` at a 1280x720 output, with GTK 4.22.4,
/// libadwaita 1.9.0 and WebKitGTK 2.52.4, from a `grim` capture of this scenario: the
/// blue-dominant pixels inside the preview pane form a bounding box of
/// `x [788, 1002]`, `y [165, 176]` — 576 pixels in all, whose dominant colour is
/// `#4ea1ff` at 265 of them. The centroid is `(900, 171)`.
///
/// Clicking it moved the captive Neovim's `tabpagebuflist()` readback from the source's
/// preview scratch to the target's, and the handler logged `nav=LinkClicked` with a
/// `file:` URI naming the target.
///
/// The measurement run used the fixture FILENAMES `alpha.md` / `beta.md` at the
/// project ROOT rather than [`LINK_SOURCE_FILE`] / [`LINK_TARGET_FILE`] under
/// [`LINK_FIXTURE_DIR`]. Neither difference can move the rectangle, and the reasons
/// are structural rather than hopeful:
///
/// * An href is never drawn — only the link TEXT is — and [`LINK_SOURCE_LINE`]'s text
///   is byte-identical to the measured one, so the rendered document is the same
///   bytes. `render_preview` puts no filename in the page.
/// * The source's PATH does grow, and it is drawn: the pane's statusbar strip renders
///   it on the secondary line. That strip cannot move the web view's origin, because
///   `statusbar_layout::statusbar_layout` derives its height from the two FONT SIZES
///   alone and never from its text. Its band's lower rows do fall inside the search
///   rectangle, and what they hold there is grey theme text, which [`is_link_blue`]'s
///   blue-over-red margin rejects.
/// * The sidebar gains a directory row. The pane area is built from
///   [`OVERLAY_ORIGIN_X_EXPECTED`], a fixed widget corner every pane smoke shares, so
///   a row's text cannot move it.
///
/// It is recorded rather than smoothed over, because a reader comparing this note with
/// the run log would otherwise wonder which fixture the numbers came from — and
/// because the constant is RE-VERIFIED on every run by the calibration assertion,
/// which re-derives the cluster from that run's own capture A. That assertion, not
/// this note, is what makes the claim above checked rather than argued.
///
/// **Do not tune this to make a run pass.** The smoke re-asserts the calibration out
/// of its own capture before it uses the number, so a layout change fails there, with
/// a message naming the layout — which is the failure this binary records at the
/// "stayed green while describing a layout that no longer existed" note.
const LINK_CLICK_X: i64 = 900;

/// y of the same point. See [`LINK_CLICK_X`] for the measurement.
const LINK_CLICK_Y: i64 = 171;

/// The widest the located link cluster may be, in pixels.
///
/// An ACCEPTANCE CRITERION rather than a tuned number, and the half that stops the
/// containment assertion below from holding vacuously: the measured cluster is 215 px
/// wide, while any pane-spanning blue would produce something near the search
/// rectangle's own ~484 px. 400 sits far above the one and below the other, so the two
/// readings are never ambiguous.
const LINK_CLUSTER_MAX_W_PX: usize = 400;

/// The tallest the located link cluster may be, in pixels.
///
/// The same criterion on the other axis, and the sharper of the two: the link is ONE
/// line of 16 px body text and measured 12 rows tall, while anything spanning the
/// pane's height would be hundreds. 48 is four times the measurement and an order of
/// magnitude below the alternative.
const LINK_CLUSTER_MAX_H_PX: usize = 48;

/// Body of [`preview_link_click_repoints_the_pane_under_cage`], appended to
/// [`POINTER_PREAMBLE`].
///
/// Open the source in the window's ONE pane, open its preview with `:WolfPreview`,
/// capture, read the window list back, CLICK the rendered link, read the window list
/// back again, capture again.
///
/// ## Why `:WolfPreview` rather than the command palette
///
/// Preview has two routes and this one takes the Ex command, unlike its sibling
/// preview smokes. A palette query is matched as a subsequence and can in principle be
/// reordered by ranking (see [`PREVIEW_QUERY`]); the Ex command names one command and
/// cannot be reordered by anything. That matters more here than elsewhere because this
/// smoke's whole geometry — and therefore its measured click point — depends on the
/// preview landing in the RIGHT half, so a palette that activated the other row would
/// fail on a pixel search rather than on something legible. It is also the exact path
/// the click measurement was taken through.
///
/// ## Why the preview is deterministically the RIGHT half
///
/// One pane is open when Preview in Another Pane fires, and it SPLITS: a split of one
/// pane can only produce two, and the new one is the right half. The rest of the chain
/// is written out on
/// [`preview_document_follows_the_configured_appearance_under_cage`] and is not
/// restated here.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// None. A missing readback is the preamble's own `exit 14`, and a `grim` failure
/// aborts under `set -eu` carrying grim's own status.
const POINTER_LINK_BODY: &str = r#"
# PREVIEW_SETTLE -- budgeted FROM THE CALL, not from the web view's Started event, and
# defined HERE because POINTER_PREAMBLE has no preview leg of its own. A cold open is
# ~255 ms call->Finished on the measurement host (about 110 ms of it spent spawning
# the WebProcess before Started fires); PREVIEW_SCENARIO's doc comment carries the
# measurement. It is spent TWICE below, and both spends are full loads: the follow
# re-establishes the base URI, so it goes through load_html rather than a body swap.
PREVIEW_SETTLE=2.0

# --- Step 1: the source in the window's ONE pane ----------------------------
#
# One pane is what makes the split deterministic, which is what puts the preview
# in the RIGHT half the click constants were measured against.
#
# $LINK_SOURCE_FILE carries its SUBDIRECTORY, and is typed RELATIVE so the buffer's
# name is the cwd-relative path the readback below compares against.
wtype -- ":e $LINK_SOURCE_FILE"
wtype -k Return
sleep "$SETTLE"

# Ask the captive Neovim what its windows hold, into the file named by $1. Joined
# with a COMMA rather than a space because a preview scratch's own name contains one
# (it is "Preview: <cwd-relative path>"); the Rust side splits on the comma and sorts
# before comparing, so window order is not the subject.
#
# `bufname(b)` RAW, deliberately unlike the sibling preview scenarios' `:t`: this
# smoke's fixtures live in a subdirectory, and a tail would drop it -- turning
# "Preview: lnkdir/lnkinto.md" into "lnkinto.md", which is also what the SOURCE
# pane's entry would tail to. That collapse would cost the assertion both its
# "Preview: " prefix (the half no sidebar row or :e toast can forge) and the
# directory (the half that separates the correct resolution from the cwd-relative
# one). PREMISE, and the reason the BEFORE readback is asserted as a precondition:
# a buffer's name is the string it was given -- the relative path typed at `:e` for
# the source, and nvim_buf_set_name's argument for the preview scratch, which
# wolf_ensure_preview_buf builds as "Preview: " .. fnamemodify(abs, ':.').
read_wins() {
    wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> bufname(b)}), ',')], '$1')"
    wtype -k Return
    wait_file "$1" || exit 14
}

# --- Step 2: open the preview BESIDE the source, by its Ex command ------------
#
# An Ex command and not the palette: see this const's doc comment. The OTHER-PANE
# one, because `:WolfPreview` binds in place and would leave this one-pane window
# showing only the preview -- with no editor half for the click geometry below.
wtype -- ":WolfPreviewInOtherPane"
wtype -k Return

# The COLD open budget: the blocking open_preview RPC, the wake carrying the
# wolf_preview notification, the lazily-created WebView's WebProcess spawn, the
# blocking render_preview and the full load_html. PREVIEW_SCENARIO's doc comment
# carries the measurement behind this value.
sleep "$PREVIEW_SETTLE"

# --- Step 3: park the pointer, then capture A --------------------------------
#
# The pointer goes to the far top-left first. grim does not composite the cursor by
# default, so this is belt-and-braces -- but capture A is what the CALIBRATION is read
# out of, and a cursor glyph over the link would be read as link ink.
hsend motion 1 1
sleep "$SETTLE"
grim "$OUT_PNG_BEFORE"

# --- Step 4: the BEFORE readback ---------------------------------------------
#
# Taken AFTER the capture on purpose: the typed command line is drawn by the cmdline
# strip, and capture A must carry no typed text at all.
read_wins "$LINK_WINS_BEFORE"

# --- Step 5: THE CLICK, at the measured centroid of the rendered link --------
#
# The pointer is parked at (1, 1) from step 3, so this motion crosses into the preview
# pane and produces a real leave/enter/motion sequence rather than a no-op
# motion_absolute to a position the pointer already occupies -- the same reason
# POINTER_WEDGE_BODY nudges before its click.
#
# The web view is set_focusable(false)/set_can_focus(false), so this is not a focus
# change: the event reaches WebKit's DOM hit test, the `decide-policy` handler answers
# the navigation with ignore() and hands the resolved URI to preview_link::classify.
hsend motion "$LINK_CLICK_X" "$LINK_CLICK_Y"
hsend press "$BTN_LEFT"
hsend release "$BTN_LEFT"

# The follow costs a blocking preview_file, a wake, a blocking render_preview and a
# full load_html -- the same budget a cold open does.
sleep "$PREVIEW_SETTLE"

# --- Step 6: the AFTER readback ----------------------------------------------
read_wins "$LINK_WINS_AFTER"

# --- Step 7: park the pointer again, then capture B --------------------------
#
# No assertion reads capture B; it is retained for post-mortem, so a failing run can
# be looked at rather than re-derived.
hsend motion 1 1
sleep "$SETTLE"
grim "$OUT_PNG"

hsend quit
exit 0
"#;

// Unsaved-buffer preview smoke: a scenario of its own, so one exit code never covers two features
// and the calibrated captures of `NEWBUF_SCENARIO` and `PREVIEW_SCENARIO` stay put.

/// Unsaved-preview smoke: the file opened OVER the untitled buffer's pane just before
/// the capture, so the marker's raw text is off screen and an OCR hit can only have
/// come from the web view.
///
/// [`PREVIEW_SCENARIO`] step 7's proven move. The untitled buffer stays loaded and
/// modified under nvim's `hidden` default, so the preview keeps rendering from it.
pub(crate) const UNSAVED_COVER_FILE: &str = "uncov.md";

/// Heading written into [`UNSAVED_COVER_FILE`]; lowercased it contains
/// [`UNSAVED_OCR_COVER`]. Deliberately UNLIKE the filename, so neither the sidebar row
/// nor nvim's `:e` toast — both of which spell `uncov.md` — can satisfy the assertion
/// that reads it.
pub(crate) const UNSAVED_COVER_HEADING: &str = "# ZZCOVERSIERRA";

/// Lowercased substring OCR MUST find: the EDITOR pane's own file, drawn by this
/// crate's painter.
///
/// Two jobs, and the second is what licenses the marker assertion: the window was
/// painting when `grim` ran, and the `:e` really landed — so the untitled buffer's raw
/// text is no longer in any pane and only WebKit can be drawing it.
pub(crate) const UNSAVED_OCR_COVER: &str = "zzcoversierra";

/// Typed as an `h1` into the never-saved Ctrl+Shift+N buffer.
///
/// An `h1` for the reason [`PREVIEW_SOURCE_HEADING`] gives: `wolf_core`'s preview
/// stylesheet sets `body` to 16 px and `h1` to 32 px, and this term has to survive
/// `tesseract` at whatever size WebKit rasterizes it.
pub(crate) const UNSAVED_MARKER: &str = "# ZZUNSAVEDROMEO";

/// Lowercased substring OCR MUST find: [`UNSAVED_MARKER`] as the web view rendered it.
///
/// The buffer it was typed into was NEVER WRITTEN TO DISK, so no file on the filesystem
/// carries this word, no sidebar row can spell it, and by capture time no editor pane
/// is showing it either.
pub(crate) const UNSAVED_OCR_MARKER: &str = "zzunsavedromeo";

/// The name `runtime/nvim/init.lua`'s `_G.wolf_new_untitled` gives the Ctrl+Shift+N
/// buffer, as the readback's `fnamemodify(bufname(b), ':t')` reports it.
///
/// **`Untitled 2.md`, not `Untitled.md`**, and the difference is the naming rule rather
/// than an off-by-one: index 1 is un-numbered, so the VimEnter autocmd has already
/// named the STARTUP buffer `Untitled.md`, and the next free name collides up the
/// sequence. MEASURED under `cage` on 2026-08-06 in a fresh temp cwd:
/// `nvim_buf_get_name` answered `…/project/Untitled 2.md` with `filetype=markdown`, and
/// the preview bound as `Preview: Untitled 2.md`.
///
/// The fresh cwd is load-bearing for that measurement: the collision check also
/// consults on-disk files and swap files, so a reused directory holding a stale
/// `Untitled.md` swap would push the name further along the sequence. Every smoke gets
/// one from [`make_temp_cwd`].
pub(crate) const UNSAVED_BUFFER_NAME: &str = "Untitled 2.md";

/// Name of the file the captive Neovim writes its window/buffer list into.
pub(crate) const UNSAVED_READBACK_NAME: &str = "unwins.txt";

/// The UNSAVED-BUFFER PREVIEW scenario, run as a single `sh -c` string inside `cage`
/// (`NEWBUF_SCENARIO`'s shape with [`PREVIEW_SCENARIO`]'s palette leg, its
/// `wait_file` and its step 7).
///
/// Launch and poll; Ctrl+Shift+N; type the marker as an `h1` and Escape; drive Preview
/// through the palette; read the window list back; move to the SOURCE pane and open
/// the cover file over it; capture.
///
/// ## Why the focus move before the `:e` is load-bearing
///
/// `wolf_preview_in_other` splits and then binds the preview into the NEW window, so
/// the current window after the Preview leg is the PREVIEW. A `:e` typed
/// there would replace the preview scratch itself and the smoke would be capturing an
/// ordinary editor pane. `<C-w>h` moves to the original pane — the one still holding
/// the untitled buffer — which is the pane the cover file must land in.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `14` — the window-list readback never appeared, so the Preview leg cannot be
///   checked at all.
const UNSAVED_PREVIEW_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for a readback file.
FILE_TRIES=100
# PREVIEW_SETTLE -- budgeted FROM THE CALL, not from the web view's Started event.
# PREVIEW_SCENARIO's doc comment carries the measurement behind it.
PREVIEW_SETTLE=2.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome unsaved-preview smoke"

# Poll for a readback file the captive Neovim writes. (The PREVIEW_SCENARIO body,
# verbatim.)
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
    echo "wolf-gnome unsaved-preview smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 1: a brand new, NEVER-SAVED buffer ---------------------------------
#
# Ctrl+Shift+N -> win.new-buffer -> InputEvent::NewBuffer -> _G.wolf_new_untitled,
# which does :enew AND names the buffer "Untitled N.md" with filetype=markdown --
# without ever writing it to disk. The name is what the preview gate and the base URI
# both need; a plain :enew leaves the buffer nameless and non-markdown.
wtype -M ctrl -M shift -k n -m shift -m ctrl
sleep "$SETTLE"

# --- Step 2: type the marker as an h1, then leave insert mode ----------------
wtype -- "i$UNSAVED_MARKER"
wtype -k Escape
sleep "$SETTLE"

# --- Step 3: drive PREVIEW IN ANOTHER PANE through the command palette --------
#
# Ctrl+Shift+P, the query, Enter on the only matching row. The OTHER-PANE command and
# not plain Preview: this window has ONE pane, and the in-place Preview would bind the
# scratch into it and leave the untitled buffer displayed nowhere -- so step 4's
# two-window readback could not hold. Neither command has an accelerator row of its
# own; the palette and the :Wolf* Ex commands are their only routes.
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$PREVIEW_OTHER_PANE_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$PREVIEW_SETTLE"

# --- Step 4: NEOVIM's own answer about what its windows hold -----------------
#
# Joined with a COMMA rather than a space, because both names contain one: the preview
# scratch is "Preview: <path>" and the untitled buffer is "Untitled N.md".
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ',')], '$UNSAVED_READBACK')"
wtype -k Return
wait_file "$UNSAVED_READBACK" || exit 14

# --- Step 5: move to the SOURCE pane -----------------------------------------
#
# The Preview leg left the current window on the PREVIEW (the other-pane bind lands in
# the new split), so without this the :e below would replace the preview scratch itself.
wtype -M ctrl -k w -m ctrl
wtype -k h
sleep "$SETTLE"

# --- Step 6: take the marker's RAW text off the screen -----------------------
#
# The marker was typed into a pane nvim is drawing, so without this the capture would
# carry it twice and an OCR hit would prove nothing about the web view. The untitled
# buffer stays loaded and MODIFIED under nvim's 'hidden' default, so the preview keeps
# rendering from it.
wtype -- ":e $UNSAVED_COVER_FILE"
wtype -k Return
sleep "$SETTLE"

# Park the block cursor off row 0 of this pane: its heading is asserted, and the
# inverted block corrupts the cell it covers.
wtype -- o
wtype -k Escape
sleep "$SETTLE"

# --- Step 7: capture, with the preview of a never-saved buffer live ----------
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The prefix of the stderr line the app prints when its `gtk-xft-dpi` pin in
/// `crates/wolf-gnome/src/app.rs` fires, i.e. when GTK came up with the dpi unset.
///
/// Re-typed rather than imported, for this binary's usual reason: an assertion that read
/// the app's own constant would hold for whatever the line became. The two preview
/// smokes that discriminate the pin assert their cage log carries it, which is what
/// makes their green mean "the pin ran" rather than "something else supplied a dpi".
/// [`SMOKE_GTK_SETTINGS_INI`] carries no dpi, but GTK also reads the `gtk-4.0/settings.ini`
/// under its compiled-in sysconfdir, which `XDG_CONFIG_DIRS` does not replace, and a
/// build or host that sets one there leaves the pin idle and both smokes green.
const DPI_PIN_STDERR_PREFIX: &str = "wolf-xft-dpi-pinned:";

/// A Markdown preview pane RENDERS its source, names it in its statusbar, persists
/// as a preview leaf, updates LIVE as the source is typed into, and hands the source
/// back to the other pane when Edit Source is run.
///
/// ## What it covers that no other smoke does
///
/// The whole preview path, and nothing else in this repository touches any of it:
/// `shell::perform`'s `OpenPreview` / `EditSource` arms and the two blocking RPCs
/// beneath them, `preview_ui`'s two placement seams carried out by a real Neovim,
/// `pane_view`'s lazily-created `webkit6::WebView` inside the per-pane overlay host,
/// the `previews` × `markdown_ticks` join that decides a reload, the JSON body swap
/// that performs one, the preview-only statusbar built from `preview_ui::preview_status`,
/// and `layout_snapshot`'s per-pane preview flag. Measured before this smoke existed:
/// with `AppShell::open_preview` neutered by an early `return`, the whole workspace
/// stayed green.
///
/// Preview and Edit Source have no `actions::ACTION_TABLE` row, so the command
/// palette and the `:WolfPreview` / `:WolfEdit` Ex commands are their only routes —
/// which is why both legs go through the palette.
///
/// ## The premises, and where each is measured
///
/// **The placement is `active`.** Two panes, an editor active, no other preview, so
/// `wolf_open_preview` replaces the source pane's buffer and the source's raw text is
/// drawn NOWHERE. That is what makes assertion 6 a statement about WebKit's pixels
/// rather than about nvim's. It is not assumed: assertions 5 and 8 both fail if a
/// third pane appeared instead. Assertion 5 is ordered AHEAD of the two OCR assertions for
/// exactly that reason — the premise is established before it is used.
///
/// **Each palette query ranks the row it names FIRST.** Pinned in the fast suite by
/// `palette::tests::palette_rows_for_the_preview_smoke_rank_the_named_command_first`,
/// which also pins that the two rows still carry the ids assertion 3 reads back, and
/// which states the one MRU state that would invert `prv`. A screenshot cannot rank
/// palette rows — the overlay is closed before `grim` fires.
///
/// **The live edit takes the SWAP branch, not `load_html`.** The first load into a
/// view records `</head>`'s offset; the second finds it unmoved and swaps the body.
/// `crates/wolf-core/src/nvim.rs` documents the head as byte-identical across calls,
/// which is the premise that branch rests on.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. The `measured` column is from an actual red run, per
/// `tests/README.md`'s "A red-run demonstration is a deliverable, not a formality".
/// Rows marked unmeasured have not been run against a captured red.
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the scenario ran to completion | none; do not invent one | n/a |
/// | 2 | both captures present | precondition — `grim` produced two images | none; do not invent one | n/a |
/// | 3 | the command MRU holds both [`PREVIEW_MRU_KEYS`] | precondition — both palette rows were ACTIVATED (`record` runs before the dispatch) | none; the `record` call is `command_palette_chord_runs_the_selected_command_under_cage`'s discriminator | n/a |
/// | 4 | capture A carries [`PREVIEW_OCR_OTHER`] | precondition — the window is drawing, and the preview took over exactly ONE of the two panes | none; do not invent one | n/a |
/// | 5 | readback A is `{Preview: <source>, <other>}` | **discriminator** — Neovim really bound a preview scratch, of the right source, in one of two windows | restore the stub `perform` arm that classified `UiAction::OpenPreview` as unhandled. NOT isolated: assertion 7 fails under it as well, and goes unevaluated only because this one aborts first. Assertion 6 does NOT fail under it — measured twice, independently: a preview that never opens leaves the source's raw Markdown on screen, so [`PREVIEW_OCR_BODY`] is satisfied by the editor grid rather than by WebKit. That is why this assertion is ordered ahead of the two OCR ones: it states the same fact WITHOUT pixels. | RED, and it aborted FIRST, which is what the reordering exists for: `after the Preview leg the captive Neovim's windows hold ["pvoth.md", "pvsrc.md"], not ["Preview: pvsrc.md", "pvoth.md"]`, with `left: ["pvoth.md", "pvsrc.md"]` against `right: ["Preview: pvsrc.md", "pvoth.md"]`. Assertions 3 and 4 stayed green and 6 onward went unevaluated; the app printed `wolf-gnome: ui action not performed: markdown preview is not wired` once per leg. Assertion 6 was then checked directly, by OCRing the retained capture A: it CONTAINS `zzbodyoscar` and lacks `preview`, so 6 would have passed under this revert and 7 is the one that would have reported next. |
/// | 6 | capture A carries [`PREVIEW_OCR_BODY`] | **discriminator** — the web view rendered the document | delete the `self.load_preview_document(id, &html);` call in `PaneInner::refresh_previews`, leaving the `loaded.set(..)` beneath it so nothing re-arms. The pane still becomes a preview and still gets a view; the view is simply never given a document. | RED, isolated: `capture A did not contain "zzbodyoscar" — the rendered heading of pvsrc.md`. Assertions 3, 4 and 5 stayed green, so the preview really was bound and only the DOCUMENT was missing; the OCR dump beside the failure still carries the `preview:` statusbar line, which is assertion 7 passing over an empty view. |
/// | 7 | capture A carries [`PREVIEW_OCR_STATUS`] | **discriminator** — a preview pane's statusbar comes from `preview_ui::preview_status` | in `PaneInner::refresh_pane_state`'s `Some(info)` arm, use `core.pane_status(j.grid)` for a preview pane. Preview windows are omitted from `win_buffers`, so it answers `[No Name]`. NOT isolated: it empties `abs_path` too, so assertion 8 would also fail — but 7 is evaluated first. | RED: `capture A did not contain "preview", so no pane's statusbar named itself a preview`. Assertions 3 through 6 stayed green — the web view still rendered its document — so this reddened the statusbar alone, and assertion 8 went unevaluated exactly as the revert column predicts. |
/// | 8 | the frozen `state.json`'s two leaves | **discriminator** — `layout_snapshot` persists the per-pane preview flag, against the SOURCE path | set `preview: false` unconditionally in `pane_view::layout_snapshot`'s `LeafContent`. Reddens this alone. | RED, and isolated as claimed: `the frozen state persisted leaves [(Some("pvoth.md"), Some(false)), (Some("pvsrc.md"), Some(false))], not [(Some("pvoth.md"), Some(false)), (Some("pvsrc.md"), Some(true))]`. Assertions 3 through 7 stayed green, so both panes were right on screen and only the PERSISTED flag was wrong — which is the whole reason this one is read from a frozen file rather than from pixels. |
/// | 9 | readback B is `{Preview: <source>, <source>, <other>}` | **discriminator** — Edit-in-another-pane split and bound the source WITHOUT replacing the preview | point `shell::perform`'s `EditInOtherPane` arm at `AppShell::edit_source`, the in-place command. The source then replaces the preview it was fired from, and the readback loses the preview scratch. | unmeasured: no red run of this revert is recorded. |
/// | 10 | capture B carries [`PREVIEW_OCR_BODY`] | **discriminator** — the preview RENDERS IN A VIEW CREATED AFTER LAUNCH: after step 5's split the preview is shown by a web view created on a host not yet allocated, which lays its page out at a 0×0 viewport and draws blank wherever GTK's `gtk-xft-dpi` is unset. It is also what separates "the swap failed and froze the stale document" from "the pane went blank" for assertion 11 | remove the `gtk-xft-dpi` pin in `crates/wolf-gnome/src/app.rs`. The view then draws blank after the split, so assertion 11 reddens with it but goes unevaluated. It needs a bus that can activate no portal, which [`smoke_bus_config`] gives every cage, and GTK settings with no dpi, which [`SMOKE_GTK_SETTINGS_INI`] is: with either supplying one the revert stays green. [`preview_created_after_launch_renders_without_a_portal_under_cage`] carries the same claim on pixels rather than OCR | RED with the pin removed: capture B lacked `zzbodyoscar`. Green with it. |
/// | 11 | capture B carries [`PREVIEW_OCR_LIVE`] | **discriminator** — the LIVE EDIT reached the DOM through the JSON body swap | in `pane_view::preview_swap_payload`, encode ONCE (`Ok(serde_json::to_string(body)?)` plus the two replacements). `JSON.parse` is then handed HTML, throws `Unrecognized token '<'` on every keystroke, and the preview keeps showing the document assertion 10 just found. **A SECOND revert reaches this assertion, and the paced burst is what admits it:** neuter the `RenderDecision::Arm` arm of `PaneInner::request_preview_render` so it arms nothing. The marker's characters then coalesce into a fire nobody ever runs, and the page carries at most the leading edge's `o`. The two reverts are distinguishable from the app's stderr — the single-encoding one prints `preview body swap failed` once per keystroke, the trailing-edge one prints nothing at all — and from assertion 12, which stays inside its budget under the first and reads a count with no trailing fire in it under the second. | RED, isolated, and with the predicted stderr: `capture B did not contain "zzlivekilo" — the marker typed into the SOURCE buffer while the preview pane was live`, beside five copies of `wolf-gnome: preview body swap failed: about:blank:1:68: SyntaxError: JSON Parse error: Unrecognized token '<'`. Assertion 10 stayed green, which is the frozen-stale-document half of the signature this row names. The trailing-edge revert: RED, isolated, and with the OTHER half of the signature — the same `capture B did not contain "zzlivekilo"` failure, and `preview body swap failed` printed NOT ONCE anywhere in the run, which is what tells the two reverts apart from stderr alone. Assertion 10 stayed green and the OCR dump beside the failure still carries the rendered heading and the `preview:` statusbar, so the pane was live and only the trailing edge was missing. Measured against the scenario INCLUDING step 9, whose burst lands after both captures and disturbs this leg not at all; the run's summed `prev.render` was 2, the cold open and the marker burst's leading edge with no trailing fire in it. |
/// | 12 | the summed `prev.render` count in this run's own frame-stats report is at most [`PREVIEW_RENDER_BUDGET`] | **discriminator** — the bursts were COALESCED: the whole scenario paid a handful of blocking `render_preview` calls rather than one per keystroke | make `PaneInner::request_preview_render` call `run_preview_render_pass()` unconditionally, which is the uncoalesced per-wake render. Assertion 11 stays GREEN under it — every keystroke renders, so the marker certainly reaches the DOM — and that is exactly the point: only this row and row 13 can see the coalescing at all. | RED, isolated: `counts 61 blocking "prev.render" calls over all 14 report block(s), above the budget of 20`. Assertions 1 through 11 stayed green under it — assertion 11 included, which is this row's own claim, and the reason the row exists. Green runs of the same scenario read 7, 7 and 8, so the budget sits at least 12 above the coalesced reading and 41 below the uncoalesced one and needed no moving. |
/// | 13 | the same sum is at least [`PREVIEW_CAP_RENDER_FLOOR`] | **discriminator** — the STALENESS CAP fired: step 9's ~2.4-second burst rendered several times DURING the typing, not only at its ends. Also row 12's non-vacuity half, since a build that renders nothing reads zero and no ceiling refuses that | drop the `min` against the cap's remainder in `PreviewCoalescer::request`, so the delay is always `PREVIEW_TRAILING_DELAY`. The sustained burst then renders at most twice — its leading edge and the trailing fire after its last key, since every arm inside it is superseded before it elapses — and measured, once. Rows 1 through 12 all stay green under it: the marker leg is shorter than the cap, so nothing else in this scenario can tell the difference. | RED, isolated: the run `counts 4 "prev.render" sample(s) over all 14 report block(s)`, under the floor. Rows 1 through 12 stayed green under it, the budget included, exactly as the revert column claims. Both ends measured: capped reads 7, 7 and 8 across three runs, cap-removed reads 4, and per block step 9's burst pays 4 renders capped against 1 with the cap gone. The floor was moved 7 to 6 on that evidence — two capped runs sat exactly ON seven, with no margin, and the third read 8 — and the derivation in [`PREVIEW_CAP_RENDER_FLOOR`] rewritten to the measured figures. |
/// | 14 | the summed `pane.prev` count EXCEEDS the summed `pane.wake` count | **discriminator** — the coalescer's FIRE opens a preview-pass region of its own, so every `prev.render` sample has a parent and `frame_stats.rs`'s nesting table stays true | delete the `frame_metrics::scope(Bucket::PanePrev)` guard from the fire closure in `PaneInner::request_preview_render` (or bind it with `let _ =`, which drops it instantly). Rows 1 through 13 ALL stay green under it — the RPC is still timed, so every count they read is unchanged — which is why this row exists at all. Deliberately NOT `pane.prev` against `prev.render`: the pass runs on nearly every wake, so that comparison holds with the scope deleted and with the coalescer removed entirely. | RED, isolated: `counts 291 "pane.prev" region(s) against 291 "pane.wake", so no preview pass ran outside a pane wake`. Rows 1 through 13 stayed green under it, exactly as the revert column claims. Green reads +4 on three consecutive runs (298/294, 303/299, 309/305), which is the performing fires. **The comparison was written `>=` and the revert PASSED under it** — measured 299 against 299 — because the wakes-that-skip-the-pass term this scenario was assumed to have is zero: every pane wake reaches the pass. It was made STRICT on that evidence, and the 188-against-187 reading the row first carried predates step 9. |
/// | 15 | the cage log carries [`DPI_PIN_STDERR_PREFIX`] | **non-vacuity** — the app's `gtk-xft-dpi` pin ran, so assertion 10's green is the pin's evidence and not a dpi supplied from elsewhere | add `gtk-xft-dpi=98304` to [`SMOKE_GTK_SETTINGS_INI`]: assertion 10 stays green and this row reddens. Removing the pin reddens assertion 10 first, which is why this row comes last | unmeasured |
///
/// Assertion 11 is the highest-value assertion of the set and the reason the scenario
/// has a second half at all: it is the ONLY one that reaches the body-swap branch, and
/// the single-encoding bug it guards shipped green past every other check here.
///
/// Assertions 12 and 13 are the only ones that read a NUMBER rather than a page, and
/// the only evidence anywhere that the render is coalesced rather than merely
/// correct. They are two ends of ONE band around ONE sum, far enough apart that
/// neither `wtype`'s real pacing nor a stray wake decides either: 12 excludes a
/// per-keystroke render and 13 excludes a coalescer with no staleness cap. A green
/// run sits at 7 or 8 between them, against 61 for the first regression and 4 for the
/// second. Both bounds are measured at both ends, and each constant's doc carries its
/// own readings and what they exclude.
///
/// Assertion 14 reads the report too, but a different pair of counts, and it is the
/// only one that says anything about WHERE a render was timed rather than how many
/// there were. Its own row carries why it cannot be folded into the other two.
///
/// Assertion 15 reads the cage log rather than the report: a dpi reaching GTK from
/// anywhere but the pin leaves assertion 10 green, and only the pin's own line says it
/// was the pin.
#[test]
fn preview_pane_renders_and_edit_source_returns_under_cage() {
    const SMOKE: &str = "preview_pane_renders_and_edit_source_returns_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // The source's paragraph is density inside the web view, which the sidebar fillers cannot reach.
    let cwd = make_temp_cwd();
    let source_body = format!("{PREVIEW_SOURCE_HEADING}\n\n{PREVIEW_SOURCE_PARAGRAPH}");
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (PREVIEW_SOURCE_FILE, source_body.as_str()),
            (PREVIEW_OTHER_FILE, PREVIEW_OTHER_HEADING),
        ],
    );

    // The state dir `cage_command` hands the app as WOLF_STATE_DIR — the same one the
    // scenario freezes a copy of and the same one the command MRU lands in.
    let smoke_state = smoke_state_dir(&cwd);
    let smoke_state_file = smoke_state.join(STATE_FILE_NAME);

    // A directory, because `load_state` takes one; under XDG_RUNTIME_DIR so the sidebar never lists it.
    let frozen_state_dir = runtime_dir.join("wolf_gnome_preview_state");
    std::fs::create_dir_all(&frozen_state_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the frozen-state dir {}: {e}",
            frozen_state_dir.display()
        )
    });
    let frozen_state_file = frozen_state_dir.join(STATE_FILE_NAME);

    let readback_a = cwd.join(PREVIEW_READBACK_A_NAME);
    let readback_b = cwd.join(PREVIEW_READBACK_B_NAME);
    let out_png_before = runtime_dir.join("wolf_gnome_preview_open_smoke.png");
    let out_png = runtime_dir.join("wolf_gnome_preview_live_smoke.png");
    // Assertions 12 and 13's evidence: the report the running app appends to, in this
    // smoke's own fresh runtime directory, so no earlier run's renders are in it.
    let stats_path = runtime_dir.join(PREVIEW_FRAME_STATS_FILE_NAME);
    for path in [
        &readback_a,
        &readback_b,
        &out_png_before,
        &out_png,
        &frozen_state_file,
    ] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(PREVIEW_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG_BEFORE", &out_png_before)
        .env("OUT_PNG", &out_png)
        .env("PREVIEW_SOURCE_FILE", PREVIEW_SOURCE_FILE)
        .env("PREVIEW_OTHER_FILE", PREVIEW_OTHER_FILE)
        .env("PREVIEW_QUERY", PREVIEW_QUERY)
        .env("EDIT_QUERY", EDIT_QUERY)
        .env("PREVIEW_LIVE_MARKER", PREVIEW_LIVE_MARKER)
        // The scenario does not assign it, so `set -u` aborts rather than typing at wtype's default rate,
        // which would disarm assertion 11 and shorten step 9's burst below the staleness cap.
        .env("TYPE_DELAY_MS", PREVIEW_TYPE_DELAY_MS.to_string())
        .env("PREVIEW_CAP_FILLER", PREVIEW_CAP_FILLER)
        .env("PREVIEW_READBACK_A", &readback_a)
        .env("PREVIEW_READBACK_B", &readback_b)
        .env("PREVIEW_STATE_SRC", &smoke_state_file)
        .env("PREVIEW_STATE_COPY", &frozen_state_file)
        // For assertion 12 alone, which sums across reports, so sharing the frame-stats interval is safe.
        .env("WOLF_FRAME_STATS", "1")
        .env("WOLF_FRAME_STATS_INTERVAL", FRAME_STATS_INTERVAL_SECS)
        .env("WOLF_FRAME_STATS_FILE", &stats_path)
        .status()
        .expect("failed to spawn `cage` for the headless markdown-preview smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage markdown-preview scenario exited unsuccessfully ({status}). Exit 11: \
         the app died before it was ready for input — a LAUNCH failure, and nothing below is about \
         the preview. Exit 14: a window-list readback never appeared, so the captive Neovim never \
         ran the typed Ex command. Exit 16: the app persisted no state.json at all while the \
         preview pane was live, which is `may_snapshot` refusing rather than the preview flag \
         being wrong. Otherwise wtype, grim or cp failed. Note this assertion says nothing about \
         whether a preview ever opened: the scenario exits 0 whatever the palette did. Captures \
         expected at {} and {}.",
        out_png_before.display(),
        out_png.display()
    );

    // --- Assertion 2 (precondition): both captures exist ----------------------
    assert_png_present(SMOKE, &out_png_before);
    assert_png_present(SMOKE, &out_png);

    // Assertion 3 (precondition): both palette rows were activated. `activate_selection` records the
    // key before dispatch, and every assertion below rests on it.
    let recent = CommandMru::open(&smoke_state).recent();
    for key in PREVIEW_MRU_KEYS {
        assert!(
            recent.iter().any(|k| k == key),
            "{SMOKE}: the command MRU at {} does not hold {key:?}; it holds {recent:?}. Both \
             palette rows must have been activated — {PREVIEW_QUERY:?} for `preview` and \
             {EDIT_QUERY:?} for `edit_in_other_pane`. An EMPTY list means neither palette ever opened or Enter \
             activated nothing; a list holding ONE of the two names which leg failed; a list \
             holding some OTHER id means a query matched a row it does not name, which \
             `palette::tests::palette_rows_for_the_preview_smoke_rank_the_named_command_first` \
             would also have caught in the fast suite.",
            smoke_state.join("command-mru.sqlite").display()
        );
    }

    let ocr_before = ocr_to_lowercase(&out_png_before);

    // Assertion 4 (precondition): the window was painting and the preview replaced one of two panes, so
    // assertion 6 cannot fail ambiguously between "no preview" and "no window".
    assert!(
        ocr_before.contains(PREVIEW_OCR_OTHER),
        "{SMOKE}: capture A did not contain {PREVIEW_OCR_OTHER:?} — the heading of \
         {PREVIEW_OTHER_FILE}, which the EDITOR pane held throughout the first half. The likely \
         causes, in order: (a) the app was not drawing at all when grim fired; (b) the `<C-w>v` \
         chord never split, so there is one pane and the preview replaced the only file on screen; \
         (c) the preview opened into the wrong pane. Note the fixture's NAME does not contain this \
         term, so a sidebar row cannot satisfy it — only the rendered buffer can. PNG kept at {}. \
         OCR output was:\n{ocr_before}",
        out_png_before.display()
    );

    // Assertion 5 (discriminator): Neovim's own window buffer names. Read before the OCR assertions,
    // because it establishes assertion 6's premise that the source text is off screen.
    let want_open = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{PREVIEW_SOURCE_FILE}"),
            PREVIEW_OTHER_FILE.to_string(),
        ];
        want.sort();
        want
    };
    let got_open = read_window_buffer_names(&readback_a, SMOKE);
    assert_eq!(
        got_open, want_open,
        "{SMOKE}: after the Preview leg the captive Neovim's windows hold {got_open:?}, not \
         {want_open:?} (both sorted, so window order is not the subject). {PREVIEW_SOURCE_FILE:?} \
         in place of the preview scratch means NO preview was bound — the palette dispatched \
         nothing, or `shell::perform` has no `OpenPreview` arm. THREE entries mean the placement \
         split a new pane instead of replacing the active one, which would also leave the source \
         on screen and make assertion 6 below vacuous. A preview scratch naming a DIFFERENT file \
         means the wrong buffer was previewed."
    );

    // Assertion 6 (discriminator): the in-place placement leaves the source in no pane and `pvsrc.md`
    // lacks the word, so only WebKit can draw it, given assertion 5.
    assert!(
        ocr_before.contains(PREVIEW_OCR_BODY),
        "{SMOKE}: capture A did not contain {PREVIEW_OCR_BODY:?} — the rendered heading of \
         {PREVIEW_SOURCE_FILE}, which after an `active` placement only the WEB VIEW can draw. The \
         likely causes, in order: (a) no `render_preview` / `load_preview_document` ran, so the \
         view holds `about:blank`; (b) the view was never created, or was created and then \
         unparented by a pane-tree rebuild; (c) the render returned the empty string, which is \
         what `WolfCore::render_preview` answers on an RPC failure; (d) WebKit drew the document \
         but tesseract misread it at the stylesheet's 32 px h1 — check the retained PNG BEFORE \
         touching the assertion, and if the web view's glyphs are the problem, lowercase the \
         fixture heading rather than weakening this. Assertion 5 above answers the same question \
         without pixels and is the first thing to read beside this one. PNG kept at {}. OCR output \
         was:\n{ocr_before}",
        out_png_before.display()
    );

    // Assertion 7 (discriminator): `pane_status` answers `[No Name]` for a preview, so only
    // `preview_status` draws this word. The query is `prv`, so an open palette cannot satisfy it.
    assert!(
        ocr_before.contains(PREVIEW_OCR_STATUS),
        "{SMOKE}: capture A did not contain {PREVIEW_OCR_STATUS:?}, so no pane's statusbar named \
         itself a preview. The likely causes, in order: (a) `refresh_pane_state` took \
         `core.pane_status` for the preview pane, which answers `[No Name]`; (b) the pane was \
         never recognised as a preview at all, in which case assertion 6 above is red too and THAT \
         is the finding; (c) the strip drew the text but tesseract misread it — the sibling \
         per-pane statusbar smoke reads strip text at this same font size, so suspect this last. \
         PNG kept at {}. OCR output was:\n{ocr_before}",
        out_png_before.display()
    );

    // Assertion 8 (discriminator): read from the frozen copy; the live file is rewritten on every wake
    // and, after the Edit leg, both leaves name the source.
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
    match &tab.layout {
        Some(PaneNode::Split {
            orientation,
            children,
            ..
        }) => {
            assert_eq!(
                *orientation,
                PaneOrientation::Vertical,
                "{SMOKE}: the frozen state persisted a {orientation:?} split for a `<C-w>v`, which \
                 divides WIDTH and must persist as Vertical. File: {}",
                frozen_state_file.display()
            );
            // Compared as a set: which side holds which pane is the divider smoke's subject, not this one's.
            let mut got: Vec<(Option<&str>, Option<bool>)> = children
                .iter()
                .map(|child| (leaf_tail(child), leaf_preview(child)))
                .collect();
            got.sort();
            let mut want = vec![
                (Some(PREVIEW_OTHER_FILE), Some(false)),
                (Some(PREVIEW_SOURCE_FILE), Some(true)),
            ];
            want.sort();
            assert_eq!(
                got,
                want,
                "{SMOKE}: the frozen state persisted leaves {got:?}, not {want:?} (each entry is \
                 `(file name, preview flag)`, sorted). `Some(false)` on BOTH is a stubbed flag \
                 — `layout_snapshot` writing `preview: false` unconditionally, so a \
                 preview pane is restored with `:edit` and the next launch reopens the SOURCE \
                 where the preview was. A `None` NAME on the preview leaf means \
                 `preview_status`'s `abs_path` came back empty or relativized, which is \
                 unopenable on the next launch since state.json records no working directory to \
                 resolve it against. THREE entries mean the placement split a third pane. The \
                 comparison is on the file NAME rather than the whole path because /tmp may \
                 resolve through a symlink. File: {}",
                frozen_state_file.display()
            );
        }
        other => panic!(
            "{SMOKE}: the frozen state persisted `layout: {other:?}` for two side-by-side panes, \
             not a two-child Vertical split. A `None` means the snapshot gate refused every wake — \
             which implicates `snapshot_readiness`, whose reported \
             count must be `win_buffers.len() + previews.len()`, since a preview window appears in \
             neither map otherwise. A `Leaf` means the split never reached the pane tree at all. \
             File: {}",
            frozen_state_file.display()
        ),
    }

    // Assertion 9 (discriminator): `edit_in_other(0)` splits and binds the source into the new pane, so
    // a third window appears and the preview survives.
    let want_edit = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{PREVIEW_SOURCE_FILE}"),
            PREVIEW_SOURCE_FILE.to_string(),
            PREVIEW_OTHER_FILE.to_string(),
        ];
        want.sort();
        want
    };
    let got_edit = read_window_buffer_names(&readback_b, SMOKE);
    assert_eq!(
        got_edit, want_edit,
        "{SMOKE}: after the Edit-in-another-pane leg the captive Neovim's windows hold \
         {got_edit:?}, not {want_edit:?} (both sorted). TWO entries with the preview scratch GONE \
         means the IN-PLACE `edit_source` ran instead of `edit_in_other`: the source replaced the \
         preview it was fired from, and steps 6 onward have no preview left to observe — check \
         `shell::perform`'s `EditInOtherPane` arm and that {EDIT_QUERY:?} still selects the \
         other-pane row. TWO entries with the preview scratch PRESENT means the leg did nothing at \
         all — check `edit_press`'s preview gate. A missing {PREVIEW_OTHER_FILE:?} means core \
         reused that window rather than splitting, i.e. a `target_win` other than \
         `shell::SPLIT_FOR_OTHER_PANE` reached `edit_in_other`."
    );

    let ocr_live = ocr_to_lowercase(&out_png);

    // Assertion 10 (discriminator): the view created by the split rendered. Green here with 11
    // red is a frozen preview, the single-encoding signature; both red is a blank or lost view.
    assert!(
        ocr_live.contains(PREVIEW_OCR_BODY),
        "{SMOKE}: capture B did not contain {PREVIEW_OCR_BODY:?}, so the preview pane is not \
         showing its document at all any more — and assertion 11 below cannot be read, since a \
         blank pane fails it for a reason that has nothing to do with the body swap. The named \
         cause: after step 5's Edit-in-another-pane SPLIT the preview is shown by a web view \
         created on a host not yet allocated, and with GTK's `gtk-xft-dpi` unset WebKitGTK lays \
         that view's page out at a 0×0 viewport — check that the app's `gtk-xft-dpi` pin in \
         crates/wolf-gnome/src/app.rs still runs before any preview view exists. A cage's bus \
         offers no portal and its GTK settings carry no dpi, so nothing else supplies one; \
         `preview_created_after_launch_renders_without_a_portal_under_cage` asserts the same on \
         pixels, and red there too confirms it. The other causes: the pane \
         stopped being a preview (Edit Source bound the source into it — but then assertion 9 \
         above is red and that is the finding), or the swap replaced the body with nothing. PNG \
         kept at {}. OCR output was:\n{ocr_live}",
        out_png.display()
    );

    // Assertion 11 (discriminator): the only one reaching `preview_swap_payload`; step 7 covered the
    // marker's raw text, so it is on screen in one place only.
    assert!(
        ocr_live.contains(PREVIEW_OCR_LIVE),
        "{SMOKE}: capture B did not contain {PREVIEW_OCR_LIVE:?} — the marker typed into the \
         SOURCE buffer while the preview pane was live — although assertion 10 just proved the \
         preview is still rendering its document. That pair is the SINGLE-ENCODING signature: with \
         `preview_swap_payload` encoding once instead of twice, the JS literal's value is the HTML \
         itself, `JSON.parse` is handed `<` and throws `Unrecognized token '<'`, and the view \
         keeps showing the document it already had. Check the app's stderr for `preview body swap \
         failed`. The other candidates, in order: `needs_reload` never fired (the \
         `previews`×`markdown_ticks` join is keyed on the SOURCE bufnr, and nothing on that path \
         may be gated on `Snapshot::seq`); `may_render` refused; or the `head_len` guard sent the \
         reload down `load_html` instead, which would still show the marker — so a MISSING marker \
         is never that. PNG kept at {}. OCR output was:\n{ocr_live}",
        out_png.display()
    );

    // Assertion 12 (discriminator): the blocking `render_preview` count's ceiling. 11 is green
    // either way, so this and 13 are the only measures of what the preview cost.
    let stats_text = std::fs::read_to_string(&stats_path).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: could not read the frame-stats report {}: {e}. `frame_metrics::install()` \
             opens it create/append as the FIRST statement of `app::run`, and the install header \
             is written the moment the flag is read — before any interval can elapse — so an \
             ABSENT file means the flag never reached the app (this smoke's `cage_command` is what \
             sets WOLF_FRAME_STATS and WOLF_FRAME_STATS_FILE) rather than that no render was \
             recorded. Note assertions 1 through 11 above passed, so the app ran and the preview \
             worked; this is about the instrument, not about the feature.",
            stats_path.display()
        )
    });
    let stats_blocks = parse_frame_stats_blocks(&stats_text, &stats_path, SMOKE);
    assert!(
        !stats_blocks.is_empty(),
        "{SMOKE}: the frame-stats report {} holds NO report blocks, so the counts below would be \
         summed over nothing and would read zero whatever the application did. A report is emitted \
         from the last statement of the drain-loop body and so rides a wake; step 10's normal-mode \
         Escape exists to supply one after the last burst. Its absence means either that no \
         interval ever closed (check WOLF_FRAME_STATS_INTERVAL, passed as \
         {FRAME_STATS_INTERVAL_SECS:?}) or that the flush key never reached the app. File \
         contents were:\n{stats_text}",
        stats_path.display()
    );

    let renders = sum_frame_stats_counts(&stats_blocks, PREVIEW_RENDER_LABEL, &stats_path, SMOKE);

    assert!(
        renders <= PREVIEW_RENDER_BUDGET,
        "{SMOKE}: the report at {} counts {renders} blocking {PREVIEW_RENDER_LABEL:?} calls over \
         all {} report block(s), above the budget of {PREVIEW_RENDER_BUDGET}. Three things move \
         the source's `changedtick` in this scenario: the cold open, step 6's paced burst of \
         {PREVIEW_LIVE_MARKER:?}, and step 9's sixty-key sustained burst. Coalesced, this scenario \
         reads 7 or 8; a count around sixty is one render PER KEYSTROKE, which is the uncoalesced \
         per-wake render and the 15.6 % of wall clock \
         `notes/wolf-gnome-decisions.md` records — measured at 61 under exactly that revert. \
         Every assertion above stays green under it, so this and assertion 13 are the only things \
         in either suite that can see it. \
         Do NOT raise PREVIEW_RENDER_BUDGET to admit the reading: read its doc comment first, \
         which says which of the two directions is allowed. File contents were:\n{stats_text}",
        stats_path.display(),
        stats_blocks.len()
    );

    // Assertion 13 (discriminator): the staleness cap fired. Only step 9's long burst exercises it,
    // and the floor is also 12's non-vacuity half, since a build that renders nothing reads zero.
    assert!(
        renders >= PREVIEW_CAP_RENDER_FLOOR,
        "{SMOKE}: the report at {} counts {renders} {PREVIEW_RENDER_LABEL:?} sample(s) over all {} \
         report block(s), below the {PREVIEW_CAP_RENDER_FLOOR} a run whose staleness cap fires \
         cannot fall under. Step 9 types sixty keys at {PREVIEW_TYPE_DELAY_MS} ms — about 2.4 \
         seconds of unbroken typing, during which the trailing timer NEVER elapses, since every arm \
         is superseded by the next keystroke. So every render inside that burst is a cap render, \
         and measured it pays FOUR of them where a green run reads 7 or 8 in total. Classify the \
         reading: a count of four or five is the CAP GONE — `request` computing its delay as the \
         trailing delay alone, with the `min` against the cap's remainder dropped, which measured \
         reads exactly 4 — under which the burst pays only its ends, and a reader typing a long \
         paragraph watches a frozen preview until they pause. A count near ZERO is something far \
         coarser: no `prev.render` row at all, which `summarize`'s zero-count skip makes mean the \
         RPC was never timed. Check that the `Bucket::PrevRender` region still wraps \
         `WolfCore::render_preview`, and that a coalesced fire opens a `pane.prev` scope of its own \
         — a `prev.render` sample with no parent is a nesting-contract break rather than a missing \
         render. Assertions 6 and 11 above passed, so documents DID reach the web view. The unit \
         test `preview_coalescer::tests::the_delay_is_capped_by_the_staleness_deadline` covers the \
         same rule from values; this is its only end-to-end evidence. File contents were:\n\
         {stats_text}",
        stats_path.display(),
        stats_blocks.len()
    );

    // Assertion 14 (discriminator): the fire opened its own `pane.prev` region, compared strictly.
    // See notes/crates/wolf-gnome/tests/smoke_headless/preview.rs.md#fire-scope-arithmetic
    let pane_prev =
        sum_frame_stats_counts(&stats_blocks, PREVIEW_PANE_PREV_LABEL, &stats_path, SMOKE);
    let pane_wake =
        sum_frame_stats_counts(&stats_blocks, PREVIEW_PANE_WAKE_LABEL, &stats_path, SMOKE);
    assert!(
        pane_prev > pane_wake,
        "{SMOKE}: the report at {} counts {pane_prev} {PREVIEW_PANE_PREV_LABEL:?} region(s) \
         against {pane_wake} {PREVIEW_PANE_WAKE_LABEL:?}, so no preview pass ran outside a pane \
         wake. Every performing coalescer fire opens a pass of its own OUTSIDE any wake, and this \
         run performed several of them — assertion 13 above just proved it — so the count must \
         EXCEED the wake count while the fire path still opens its region. What equality means: a \
         `render_preview` performed outside any {PREVIEW_PANE_PREV_LABEL:?} region, which is the \
         fire closure having lost its `frame_metrics::scope(Bucket::PanePrev)` guard (or binding \
         it with `let _ =`, which drops it instantly). Every other assertion here stays green \
         under that, and the cost is silent: `frame_stats.rs` states as FACT that a `prev.render` \
         sample always has a parent, and a measurement run would read the preview RPC as \
         unattributed time. Measured: +4 on three consecutive green runs against +0 with that \
         guard deleted, every pane wake in this scenario reaching the pass. The margin IS the \
         performing fires, so if this fires without the guard having moved, the finding is that \
         the scenario stopped arming any fire at all — re-derive it, never relax the comparison \
         back to `>=`, which the guard-deleted build satisfies. Do NOT weaken it to compare \
         against {PREVIEW_RENDER_LABEL:?} either, which holds by construction. File contents \
         were:\n{stats_text}",
        stats_path.display()
    );

    // Assertion 15 (non-vacuity): the dpi pin ran, so assertion 10 is its evidence. Last, so
    // removing the pin still reaches assertion 10's own red.
    let log_path = runtime_dir.join(CAGE_LOG_NAME);
    let log = std::fs::read_to_string(&log_path).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: could not read the cage log {}: {e}. `cage_command` sends the app's stderr \
             there, so this smoke cannot say whether the dpi pin ran.",
            log_path.display()
        )
    });
    assert!(
        log.contains(DPI_PIN_STDERR_PREFIX),
        "{SMOKE}: the cage log {} carries no {DPI_PIN_STDERR_PREFIX:?} line, so the app's \
         `gtk-xft-dpi` pin did NOT fire: GTK came up with a dpi already set, and assertion 10's \
         green is that dpi's doing, not the pin's. Find where it came from — a portal on the \
         cage's bus, a `gtk-xft-dpi` in SMOKE_GTK_SETTINGS_INI (guard check 15), a user \
         settings.ini under XDG_CONFIG_HOME (guard check 16), or GTK's compiled-in sysconfdir \
         gtk-4.0/settings.ini, which nothing in this harness replaces — or, if the app's line \
         was renamed, re-type DPI_PIN_STDERR_PREFIX from crates/wolf-gnome/src/app.rs.",
        log_path.display()
    );
}

/// The live-edit burst is typed FASTER than the coalescer's trailing delay, so its
/// keystrokes really do coalesce.
///
/// ## What it closes
///
/// [`PREVIEW_TYPE_DELAY_MS`] and [`PREVIEW_TRAILING_DELAY`] live in different files —
/// one is this smoke's typing pace, the other is the crate's tuning — and
/// [`preview_pane_renders_and_edit_source_returns_under_cage`]'s assertion 11 only
/// reaches the trailing edge while the first sits below the second. Raise the pace
/// past the delay and every keystroke of the burst becomes its own LEADING edge: the
/// marker still reaches the page, the smoke still passes, and the assertion quietly
/// stops saying anything about the coalescing. That is the same shape as
/// `codehl_chroma_thresholds_are_separated` — a constant a thousand lines from the
/// assertion it can disarm, with nothing in a green run naming it.
///
/// ## Why it IMPORTS the crate's constant
///
/// This binary's usual discipline is to re-type a production constant rather than read
/// it, so an assertion cannot follow a renamed value silently. The discipline is
/// aimed at ASSERTED TERMS, and this is its opposite: the whole content of the check
/// is that the two numbers stay in a relation, so reading the real one is the point.
/// A re-typed copy would go stale exactly when the tuning moved, which is the moment
/// the guard exists for.
///
/// ## What it needs
///
/// NO environment: no compositor, no [`require_smoke_env`], no cage slot, no
/// subprocess. It runs in microseconds and still guards on a machine with no smoke
/// toolchain at all.
///
/// Falsifier: raise [`PREVIEW_TYPE_DELAY_MS`] to 200.
#[test]
fn preview_burst_delay_stays_below_the_coalescer_trailing_delay() {
    const GUARD: &str = "preview_burst_delay_stays_below_the_coalescer_trailing_delay";

    // Bound to locals because `clippy::assertions_on_constants` rejects a constant condition, and no
    // `#[allow]` may be added (see `codehl_chroma_thresholds_are_separated`).
    let burst_delay = Duration::from_millis(PREVIEW_TYPE_DELAY_MS);
    let trailing = PREVIEW_TRAILING_DELAY;
    assert!(
        burst_delay < trailing,
        "{GUARD}: the preview smoke types its live-edit burst {burst_delay:?} per key, which is \
         not below `preview_coalescer::PREVIEW_TRAILING_DELAY` ({trailing:?}). At that pace each \
         keystroke arrives after the previous one's trailing fire has already run, so each is its \
         own LEADING edge and nothing coalesces. \
         `preview_pane_renders_and_edit_source_returns_under_cage` then stays green while its \
         assertion 11 proves only that a single render reached the DOM — the trailing edge stops \
         being what puts the marker on screen, and its second revert (arming nothing) stops \
         reddening anything. Restore the pace below the delay; do not widen this guard to admit \
         it, and if the coalescer's tuning is what moved, re-derive the pace from the new value."
    );
}

/// Clicking a LINK inside a rendered preview repoints that pane at the linked file.
///
/// ## What it proves that nothing else can
///
/// That a synthetic `wl_pointer` click reaches WebKit's DOM hit test at all, and that
/// everything downstream of it runs: the `decide-policy` handler filtering on
/// `NavigationAction` + `LinkClicked`, answering the decision with `ignore()` and
/// acting out of band; `preview_link::classify` reading a `file:` URI WebKit resolved
/// against the base [`wolf_gnome::preview_link::file_uri_for`] built; the liveness gate
/// in `preview_ui::preview_file_press`; and the blocking `WolfCore::preview_file`
/// rebinding the pane's PREVIEW WINDOW rather than opening a new pane. Every one of
/// those is unreachable from a keyboard: the web view is
/// `set_focusable(false)`/`set_can_focus(false)` and must stay so, or
/// `click_then_close_keeps_keyboard_input_alive_under_cage` breaks.
///
/// ## The two assertions name DIFFERENT causes, which is why there are two
///
/// **Assertion 3 is the CALIBRATION**, re-derived from this run's own capture A: the
/// blue-dominant pixels inside the preview pane must form ONE small cluster, and
/// [`LINK_CLICK_X`] / [`LINK_CLICK_Y`] must fall inside it. Without it the click point
/// is a bare number, and this binary already records what that costs — a smoke that
/// "stayed green while describing a layout that no longer existed". A failure HERE
/// means the rendered link is not where it was measured (a font, a stylesheet, a pane
/// geometry or an output size moved); a failure in assertion 4 means the link is
/// exactly where it was measured and the CLICK did not do its job. Those are different
/// investigations and the messages say so.
///
/// The tightness half of assertion 3 is not decoration. A bounding box is satisfied by
/// any point inside it, so a cluster swollen to the whole pane — which any
/// pane-spanning blue-dominant mark would produce, chrome drawn at the pane's own edge
/// being the obvious way to get one — would contain the click constant whatever it was.
/// The width and height ceilings are what keep "the constant is inside the link" from
/// degenerating into "the constant is inside the pane".
///
/// **Assertion 4 is the EVIDENCE**, and it is a Neovim readback rather than OCR: after
/// a successful follow, `tabpagebuflist()` names a preview scratch
/// `Preview: <target>` where it named `Preview: <source>`. That is an exact string
/// produced by Neovim itself, and no sidebar row, `:e` toast or web-view pixel can
/// satisfy it.
///
/// ## The fixtures live in a SUBDIRECTORY, and that is what makes the BASE URI visible
///
/// Read [`LINK_FIXTURE_DIR`] and [`LINK_TARGET_HREF`] before touching either fixture.
/// The short version: with both files at the project root — where they started — the
/// second falsifier below was **no falsifier at all**. A red run restored
/// `view.load_html(html, None)` and this smoke stayed green, because WebKitGTK then
/// hands the handler the RAW relative href (measured: `uri="lnkinto.md"`),
/// `preview_link::classify` answers its schemeless row, and `WolfCore::preview_file`
/// passes that relative path to Lua's `filereadable`, which resolves it against the
/// captive Neovim's WORKING DIRECTORY — which was the source's directory too. The
/// follow succeeded by coincidence of the cwd and the assertion could not see the base
/// URI at all: a check aimed at the right property, looking somewhere the defect could
/// not appear.
///
/// The subdirectory separates the two answers. `lnkinto.md` against the source's own
/// directory is `<cwd>/lnkdir/lnkinto.md`, which exists; the same string against the
/// cwd is `<cwd>/lnkinto.md`, which does not. **Moving either fixture back to the
/// project root, or putting a directory into the href, silently disarms the falsifier
/// again** — which is why the smoke checks both properties, and the absence of
/// `<cwd>/lnkinto.md`, before it launches anything.
///
/// ## The MEASUREMENT behind the click constants
///
/// Taken 2026-08-06 under `cage` at a 1280x720 output (GTK 4.22.4, libadwaita 1.9.0,
/// WebKitGTK 2.52.4, `cage` 0.3.0), against this scenario and this link text: the
/// blue-dominant pixels (`b > 150 && b > r + 60`) inside the preview pane form a
/// bounding box of `x [788, 1002]`, `y [165, 176]` — 576 pixels, dominant colour
/// `#4ea1ff` at 265 of them, which is the dark theme's link colour. Its centroid is
/// `(900, 171)`. Clicking there moved the readback from the source's preview scratch to
/// the target's and logged `nav=LinkClicked` with a `file:` URI naming the target. See
/// [`LINK_CLICK_X`] for the two differences between the measurement run and this smoke
/// — the fixture FILENAMES and the subdirectory they were moved into — and for why
/// neither can move the rectangle. Neither is taken on trust: assertion 3 re-derives
/// the cluster from every run's own capture.
///
/// **Do not tune a constant to make a run pass.** Assertion 3 exists precisely so a
/// moved layout is a named failure rather than a silent miss; if it fires, re-measure
/// and record, do not adjust.
///
/// ## The STATED NON-FALSIFIER — read this before reverting anything
///
/// Forcing `preview_ui::document_update` to answer `InPlaceEligible` unconditionally
/// leaves this smoke **green**. Neovim rebinds the preview window either way, so the
/// readback changes either way; what the guard prevents is the linked document being
/// swapped into a body whose BASE URI still points at the previous file's directory,
/// which is invisible to a readback and to a single click. The guard's only falsifiable
/// evidence is `preview_ui::tests::document_update_table`. This is the same shape as
/// the vacuously-passing checks `tests/README.md` collects under "Smoke conventions"
/// ("A red-run demonstration is a deliverable, not a formality"): **a check aimed at
/// the right property, looking somewhere the defect cannot appear.** Saying so
/// here is what stops a later reader trying that revert, seeing green, and concluding
/// the guard is unnecessary.
///
/// The fragment scroll reaches no assertion here either — a scroll offset is invisible
/// to both OCR and a readback — and neither does the external-link arm, since nothing
/// under `cage` can capture a browser opening. Both are pure-table-only, by
/// construction rather than by omission.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. The `measured` column is from an actual red run, per
/// `tests/README.md`'s "A red-run demonstration is a deliverable, not a formality".
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the pointer scenario ran to completion | none; do not invent one | n/a |
/// | 2 | both captures present, and the click point is inside the reported output | precondition — `grim` produced images and the injected point was not clamped | none; do not invent one | n/a |
/// | 3 | capture A's blue cluster is ONE small rectangle and contains the click constant | **calibration** — the constant still names the rendered link | change `PREVIEW_SOURCE_HEADING`-style typography, the fixture's link text, or the output size. It is a TRIPWIRE rather than a feature revert, and a failure here says nothing about the click path. | No red run, and none was invented: a tripwire has no feature to revert. It is not idle, though — it runs BEFORE assertion 4 and passed in both of assertion 4's red runs, so the cluster still contained ([`LINK_CLICK_X`], [`LINK_CLICK_Y`]) in each and only the readback moved. Its constants come from the phase-A measurement recorded above. |
/// | 4 | the readback goes from `Preview: <source>` to `Preview: <target>` | **discriminator** — the click reached WebKit AND the whole follow path ran | best: in `pane_view`'s `decide-policy` handler, return `false` for a `LinkClicked` navigation WITHOUT calling `ignore()`, letting WebKit navigate itself. The view then loads the target as a document of its own and the readback still names `Preview: <source>` — exactly the defect the answer-first rule prevents. Second: restore `view.load_html(html, None)`; WebKit then delivers the raw relative href, `classify` answers its schemeless row, and `filereadable` resolves it against the captive Neovim's cwd — a directory the target is NOT in, thanks to [`LINK_FIXTURE_DIR`] — so `preview_file` answers 0 and the readback is unchanged. | RED for BOTH reverts, each run alone. Identical comparison in each: left `["Preview: lnkdir/lnkfrom.md", "lnkdir/lnkfrom.md"]`, right `["Preview: lnkdir/lnkinto.md", "lnkdir/lnkfrom.md"]` — the pane never left the source. The app's stderr separated them exactly as the message predicts: the `ignore()` revert printed no refusal line at all, while the `None`-base revert printed `wolf-gnome: preview link not followed: the captive Neovim declined to preview "lnkinto.md" — it is unreadable, it is not Markdown, or the RPC failed`, naming the BARE filename, which is the base-URI signature. Each source restored and the smoke re-run green. |
#[test]
fn preview_link_click_repoints_the_pane_under_cage() {
    const SMOKE: &str = "preview_link_click_repoints_the_pane_under_cage";

    let runtime_dir = require_smoke_env();

    // The fixture's link cannot interpolate a const, so this checks it: a renamed target makes
    // `preview_file` answer 0 and assertion 4 fail with no hint at the fixture.
    let markdown_destination = format!("]({LINK_TARGET_HREF})");
    assert!(
        LINK_SOURCE_LINE.contains(&markdown_destination),
        "{SMOKE}: LINK_SOURCE_LINE = {LINK_SOURCE_LINE:?} does not carry the markdown destination \
         {markdown_destination:?}. The link's href is written into the fixture as a literal \
         because a const cannot interpolate another; if the two drift, the click follows a link \
         to a file that was never written, core answers 0, and the readback assertion fails while \
         the calibration passes — which would read as \"the click did not reach WebKit\" when the \
         truth is that the fixture is wrong. The whole destination is matched, parentheses \
         included, rather than the bare name: a bare `contains` would also accept \
         `]({LINK_FIXTURE_DIR}/{LINK_TARGET_HREF})`, which is the one variant that must NOT \
         pass — see below."
    );
    let target_inside_dir = format!("{LINK_FIXTURE_DIR}/{LINK_TARGET_HREF}");
    assert_eq!(
        LINK_TARGET_FILE, target_inside_dir,
        "{SMOKE}: LINK_TARGET_FILE = {LINK_TARGET_FILE:?} is not LINK_TARGET_HREF = \
         {LINK_TARGET_HREF:?} inside LINK_FIXTURE_DIR = {LINK_FIXTURE_DIR:?}. The href is written \
         RELATIVE to the source, so the two must name one file; if they drift, the fixture is \
         written at one path and the link points at another and the click can never follow."
    );
    let dir_prefix = format!("{LINK_FIXTURE_DIR}/");
    assert!(
        LINK_SOURCE_FILE.starts_with(&dir_prefix),
        "{SMOKE}: LINK_SOURCE_FILE = {LINK_SOURCE_FILE:?} is not inside LINK_FIXTURE_DIR = \
         {LINK_FIXTURE_DIR:?}. The source has to sit somewhere OTHER than the captive Neovim's \
         working directory or the base URI stops being falsifiable — see LINK_FIXTURE_DIR."
    );
    // Keeps the base URI falsifiable: `lnkdir/lnkinto.md` resolves both ways, so restoring
    // `load_html(html, None)` would leave this smoke green. Do not tidy this away.
    assert!(
        !LINK_TARGET_HREF.contains('/'),
        "{SMOKE}: LINK_TARGET_HREF = {LINK_TARGET_HREF:?} carries a path separator, so the href \
         is no longer relative to the SOURCE. A directory in the href resolves to the same file \
         whether WebKit applies the base URI or the raw string is handed to Lua's `filereadable` \
         against the cwd — which disarms this smoke's second falsifier (restore \
         `view.load_html(html, None)`) and makes it pass while the base URI does nothing. Keep \
         the href a bare filename and keep the fixtures in {LINK_FIXTURE_DIR:?}."
    );

    let cwd = make_temp_cwd();
    // `write_relaunch_fixtures` creates no parents. Made here, where this layout is load-bearing.
    let fixture_dir = cwd.join(LINK_FIXTURE_DIR);
    std::fs::create_dir_all(&fixture_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the fixture subdirectory {}: {e}",
            fixture_dir.display()
        )
    });
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (LINK_SOURCE_FILE, LINK_SOURCE_LINE),
            (LINK_TARGET_FILE, LINK_TARGET_BODY),
        ],
    );

    // The right resolution (against the source's directory) must exist and the wrong one (against
    // Neovim's cwd) must not, or the base URI is unfalsifiable.
    let resolved = cwd.join(LINK_TARGET_FILE);
    assert!(
        resolved.is_file(),
        "{SMOKE}: the link target {} was not written. Assertion 4 expects the click to repoint \
         the pane at it, so without the file on disk `wolf_preview_file`'s `filereadable` gate \
         answers 0 and the smoke could only ever fail.",
        resolved.display()
    );
    let cwd_relative = cwd.join(LINK_TARGET_HREF);
    assert!(
        !cwd_relative.exists(),
        "{SMOKE}: {} exists, so the href {LINK_TARGET_HREF:?} resolves to a real file against the \
         captive Neovim's WORKING DIRECTORY as well as against the source's own. That makes the \
         base URI invisible to this smoke: with `view.load_html(html, None)` restored WebKit \
         hands the handler the raw relative href, `classify` answers its schemeless row, and \
         `filereadable` finds this file — so the follow succeeds and the named falsifier stays \
         green. A SIDEBAR_FILLER_FILES entry or another smoke's fixture renamed to \
         {LINK_TARGET_HREF:?} is the likely cause; rename that, do not move this smoke's \
         fixtures out of {LINK_FIXTURE_DIR:?}.",
        cwd_relative.display()
    );

    let wins_before = cwd.join(LINK_READBACK_BEFORE_NAME);
    let wins_after = cwd.join(LINK_READBACK_AFTER_NAME);
    let out_png_before = runtime_dir.join("wolf_gnome_preview_link_before.png");
    let out_png = runtime_dir.join("wolf_gnome_preview_link_after.png");
    for path in [&wins_before, &wins_after, &out_png_before, &out_png] {
        clear_evidence(path, SMOKE);
    }

    let click_x = LINK_CLICK_X.to_string();
    let click_y = LINK_CLICK_Y.to_string();

    let status = run_pointer_scenario(
        POINTER_LINK_BODY,
        &cwd,
        &runtime_dir,
        &out_png,
        &[
            ("LINK_SOURCE_FILE", OsStr::new(LINK_SOURCE_FILE)),
            ("LINK_WINS_BEFORE", wins_before.as_os_str()),
            ("LINK_WINS_AFTER", wins_after.as_os_str()),
            ("LINK_CLICK_X", OsStr::new(&click_x)),
            ("LINK_CLICK_Y", OsStr::new(&click_y)),
            ("OUT_PNG_BEFORE", out_png_before.as_os_str()),
        ],
    );

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);

    // --- Assertion 2 (precondition): the captures exist and the point is inside
    assert_png_present(SMOKE, &out_png_before);
    assert_png_present(SMOKE, &out_png);

    let extents = read_output_extents(&cwd, SMOKE);
    assert_inside_extents(
        SMOKE,
        "the measured link point",
        (LINK_CLICK_X, LINK_CLICK_Y),
        extents,
    );

    // Assertion 3 (calibration): read from this run's capture A. A failure is a layout finding, not a
    // click-path one, which is why it comes before assertion 4.
    let before = decode_capture(&out_png_before, SMOKE, "capture A");
    let rect = link_search_rect();
    let cluster = find_link_cluster(&before, rect).unwrap_or_else(|| {
        panic!(
            "{SMOKE}: capture A carries NO blue-dominant pixel at all inside the preview pane \
             (searched x {}..{}, y {}..{}, for `b > {LINK_BLUE_MIN_B} && b > r + \
             {LINK_BLUE_MIN_MARGIN}`). The rendered LINK is the only thing on that page with \
             that colour, so this means one of: (a) the preview never opened, in which case the \
             pane is an ordinary editor and assertion 4 below is red too — check that \
             `:WolfPreviewInOtherPane` still resolves and still splits; (b) the document \
             rendered but the link was not styled \
             as one, i.e. `markdown.rs` stopped emitting an `<a>` or the theme's link colour \
             changed; (c) the appearance is not `dark`, so the light palette's link colour is on \
             screen instead — {SMOKE_APPEARANCE:?} is what `write_smoke_config` seeds. PNG kept \
             at {}.",
            rect.x0,
            rect.x1,
            rect.y0,
            rect.y1,
            out_png_before.display()
        )
    });

    assert!(
        cluster.width() <= LINK_CLUSTER_MAX_W_PX && cluster.height() <= LINK_CLUSTER_MAX_H_PX,
        "{SMOKE}: the blue-dominant cluster in capture A spans {}x{} pixels (x [{}, {}], y [{}, \
         {}], {} pixels), which exceeds the {LINK_CLUSTER_MAX_W_PX}x{LINK_CLUSTER_MAX_H_PX} a \
         SINGLE LINE of link text can occupy. The measured cluster is 215x12. A box this large \
         means the search picked up something other than the link: the suspect is a \
         pane-spanning mark satisfying the same blue-dominant predicate — chrome drawn at the \
         pane's own edge, or a second styled element in the document. The containment \
         assertion below would then hold for ANY click point, so this one has to fire first. Fix \
         the search INSET (LINK_SEARCH_INSET_X_PX / LINK_SEARCH_INSET_Y_PX); do NOT raise these \
         ceilings. PNG kept at {}.",
        cluster.width(),
        cluster.height(),
        cluster.x0,
        cluster.x1,
        cluster.y0,
        cluster.y1,
        cluster.count,
        out_png_before.display()
    );

    assert!(
        cluster.contains(LINK_CLICK_X, LINK_CLICK_Y),
        "{SMOKE}: the click constant ({LINK_CLICK_X}, {LINK_CLICK_Y}) is OUTSIDE the rendered \
         link, whose ink in capture A spans x [{}, {}] and y [{}, {}] ({} pixels). The constant \
         was measured at that cluster's centroid on 2026-08-06 at a 1280x720 output; if the two \
         no longer agree, the LAYOUT moved — a font size, the preview stylesheet, the pane \
         geometry or the output — and the click is landing on whatever is now at that pixel. \
         RE-MEASURE and record the new rectangle in `LINK_CLICK_X`'s doc comment; do NOT nudge \
         the constant until a run passes, and do not widen this assertion. Note this failure says \
         NOTHING about whether a click reaches WebKit: that is assertion 4's subject, and it is \
         evaluated after this one precisely so the two causes stay separable. PNG kept at {}.",
        cluster.x0,
        cluster.x1,
        cluster.y0,
        cluster.y1,
        cluster.count,
        out_png_before.display()
    );

    // Assertion 4 (discriminator): Neovim's buffer names before and after the click, cwd-relative. The
    // before readback fails first if `bufname` ever answers an absolute path.
    let want_before = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{LINK_SOURCE_FILE}"),
            LINK_SOURCE_FILE.to_string(),
        ];
        want.sort();
        want
    };
    let got_before = read_window_buffer_names(&wins_before, SMOKE);
    assert_eq!(
        got_before, want_before,
        "{SMOKE}: BEFORE the click the captive Neovim's windows hold {got_before:?}, not \
         {want_before:?} (both sorted, so window order is not the subject). This is the \
         PRECONDITION for the assertion below: without a preview of {LINK_SOURCE_FILE:?} bound \
         at this point, a changed readback afterwards would not mean the link was followed. ONE \
         entry means `:WolfPreview` opened nothing — check `wolf_open_preview`'s markdown \
         filetype gate and `shell::perform`'s `OpenPreview` arm. A preview scratch already naming \
         {LINK_TARGET_FILE:?} means the click landed before this readback was taken, which would \
         make the pair below vacuous. An ABSOLUTE path in either entry means `bufname()` stopped \
         answering the SHORT name this scenario's premise rests on — the fix is to write \
         `fnamemodify(bufname(b), ':.')` in `read_wins`, and it says nothing about the click \
         path. A TAILED entry ({LINK_TARGET_HREF:?} with no directory) means `read_wins` was \
         \"tidied\" back to `:t`, which collapses the preview scratch's name onto the source \
         pane's and drops the directory this smoke's falsifier depends on."
    );

    let want_after = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{LINK_TARGET_FILE}"),
            LINK_SOURCE_FILE.to_string(),
        ];
        want.sort();
        want
    };
    let got_after = read_window_buffer_names(&wins_after, SMOKE);
    assert_eq!(
        got_after,
        want_after,
        "{SMOKE}: AFTER the click the captive Neovim's windows hold {got_after:?}, not \
         {want_after:?} (both sorted). An UNCHANGED readback — still \
         `{PREVIEW_READBACK_PREFIX}{LINK_SOURCE_FILE}` — is the headline failure and has three \
         candidate causes, in order: (a) the `decide-policy` handler returned `false` without \
         calling `ignore()`, so WebKit navigated itself and the view is now showing the linked \
         file as a document of its own while Neovim knows nothing about it (the app's stderr \
         carries no refusal line in that case, which is how it is told from the others); (b) the \
         view was loaded with a `None` base URI, so WebKit delivered the RAW relative href \
         {LINK_TARGET_HREF:?}, `classify` answered its schemeless row, and Lua's `filereadable` \
         looked for that name in the captive Neovim's working directory rather than in \
         {LINK_FIXTURE_DIR:?} — stderr then carries \"the captive Neovim declined to preview \
         {LINK_TARGET_HREF:?}\", naming the bare filename, which is the signature; (c) \
         `preview_file_press` refused, which prints \"wolf-gnome: preview link not followed:\" \
         with a liveness reason. THREE entries mean a new pane was opened rather than \
         the existing preview window rebound, which `WolfCore::preview_file` must never do — it \
         rebinds a window and never creates one. Captures kept at {} and {}.",
        out_png_before.display(),
        out_png.display()
    );
}

/// The preview of a NEVER-SAVED buffer renders, and its pane names the unsaved file.
///
/// ## What it proves that nothing else can
///
/// That the anonymous-buffer naming path and the preview path compose end to end. A
/// Ctrl+Shift+N buffer is not a nameless buffer: `runtime/nvim/init.lua`'s
/// `_G.wolf_new_untitled` gives it a real, absolute `Untitled N.md` name and a
/// `markdown` filetype WITHOUT ever writing it to disk. Both halves are load-bearing
/// and both are invisible to every other test here — `wolf_open_preview` returns 0
/// unless `vim.bo.filetype == "markdown"`, and `preview_ui::preview_press` gates on
/// `markdown_active`, so a plain `:enew` buffer never reaches a preview at all.
///
/// `new_untitled_buffer_replaces_open_file_under_cage` covers the naming and
/// [`preview_pane_renders_and_edit_source_returns_under_cage`] the preview, and
/// NEITHER covers the join: the first never opens a preview and the second only ever
/// previews a file that exists on disk.
///
/// ## Why this is its own scenario rather than a leg of one of those two
///
/// `NEWBUF_SCENARIO`'s assertions are about the new buffer replacing the old, and a
/// preview leg would move its calibrated capture. `PREVIEW_SCENARIO` is already the
/// longest scenario in this binary, with eleven assertions and a calibrated A/B pair.
/// Two independent features behind one exit code contradicts this binary's rule that
/// exit codes carry meaning.
///
/// ## The premises, and where each is measured
///
/// **The buffer is named `Untitled 2.md`, not `Untitled.md`.** Index 1 is un-numbered
/// and the `VimEnter` autocmd has already given it to the STARTUP buffer, so
/// Ctrl+Shift+N collides up the sequence. MEASURED under `cage` on 2026-08-06 in a
/// fresh temp cwd; see [`UNSAVED_BUFFER_NAME`] for why the freshness matters.
///
/// **The palette's `prv` row is the one that runs.** `prv` matches `Preview` AND
/// `Preview in Another Pane` is reached by [`PREVIEW_OTHER_PANE_QUERY`], which is
/// UNIQUE and needs no ranking — pinned in the fast suite by
/// `palette::tests::palette_rows_for_the_preview_smoke_rank_the_named_command_first`.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert. The `measured` column is from an actual red run.
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the scenario ran to completion | none; do not invent one | n/a |
/// | 2 | the capture is present | precondition — `grim` produced an image | none; do not invent one | n/a |
/// | 3 | the readback is `{Preview: Untitled 2.md, Untitled 2.md}` | **discriminator** — the naming autocmd ran, the markdown filetype gate passed, and the preview BOUND, all three in one exact comparison | revert `_G.wolf_new_untitled` to a plain `:enew`. The buffer is then `[No Name]` with no markdown filetype, `wolf_open_preview` returns 0 and no preview window exists — so `fnamemodify(bufname(b), ':t')` is the empty string and `read_window_buffer_names` reports it one step earlier, as "a window is bound to an unnamed buffer". Same finding, said by the harness rather than by the comparison. | RED, exactly where the revert column predicts — in [`read_window_buffer_names`] rather than in the comparison: `the window-buffer readback <tmp>/project/unwins.txt holds an EMPTY name among [""]. Raw contents were "\n". An empty entry means a window is bound to an unnamed buffer, or that the typed writefile produced no list at all.` ONE entry, not two, so no preview window existed at all, and the app's stderr carried `wolf-gnome: preview not opened: the buffer being edited is not Markdown, and wolf_open_preview no-ops on anything else, so the blocking RPC would be spent to be told nothing happened`. `runtime/nvim/init.lua` restored and the smoke re-run green. |
/// | 4 | OCR carries [`UNSAVED_OCR_COVER`] | precondition — the window is drawing AND the `:e` landed, so the marker's raw text is off screen and only the web view can be drawing it | none; do not invent one | n/a. Never reached under assertion 3's revert, which panics first. |
/// | 5 | OCR carries [`UNSAVED_OCR_MARKER`] | **discriminator** — the web view rendered a document that exists ONLY in an unsaved buffer | the SAME revert as assertion 3. It has no independent falsifier and that is stated rather than papered over: everything that stops the preview binding also stops it rendering, and there is no edit that binds a preview of an unsaved buffer and then fails to render it. | No isolated red run, by construction. The one revert that reaches this behaviour reddens assertion 3 first and this line never runs, so its evidence is the shared revert plus the argument in the column beside it — not a run of its own. |
#[test]
fn preview_of_an_unsaved_buffer_renders_under_cage() {
    const SMOKE: &str = "preview_of_an_unsaved_buffer_renders_under_cage";

    let runtime_dir = require_smoke_env();

    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[(UNSAVED_COVER_FILE, UNSAVED_COVER_HEADING)]);

    let readback = cwd.join(UNSAVED_READBACK_NAME);
    let out_png = runtime_dir.join("wolf_gnome_unsaved_preview_smoke.png");
    for path in [&readback, &out_png] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(UNSAVED_PREVIEW_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("UNSAVED_MARKER", UNSAVED_MARKER)
        .env("UNSAVED_COVER_FILE", UNSAVED_COVER_FILE)
        .env("UNSAVED_READBACK", &readback)
        .env("PREVIEW_OTHER_PANE_QUERY", PREVIEW_OTHER_PANE_QUERY)
        .status()
        .expect("failed to spawn `cage` for the headless unsaved-buffer preview scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage unsaved-buffer preview scenario exited unsuccessfully ({status}). Exit \
         11: the app died before it was ready for input — a LAUNCH failure, and nothing below is \
         about the preview. Exit 14: the window-list readback never appeared, so the captive \
         Neovim never ran the typed Ex command, which is a KEYBOARD failure rather than a preview \
         one. Otherwise wtype or grim failed. Note this assertion says nothing about whether a \
         preview ever opened: the scenario exits 0 whatever the palette did. Capture expected at \
         {}.",
        out_png.display()
    );

    // --- Assertion 2 (precondition): the capture exists -----------------------
    assert_png_present(SMOKE, &out_png);

    // Assertion 3 (discriminator): `Preview: Untitled 2.md` proves the naming autocmd ran, the filetype
    // gate passed and the scratch is in a window. Read before the OCR assertions it licenses.
    let want = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{UNSAVED_BUFFER_NAME}"),
            UNSAVED_BUFFER_NAME.to_string(),
        ];
        want.sort();
        want
    };
    let got = read_window_buffer_names(&readback, SMOKE);
    assert_eq!(
        got, want,
        "{SMOKE}: after the Preview leg the captive Neovim's windows hold {got:?}, not {want:?} \
         (both sorted, so window order is not the subject). A single entry with no \
         `{PREVIEW_READBACK_PREFIX}` row means the palette dispatched nothing: check that \
         {PREVIEW_OTHER_PANE_QUERY:?} still selects `Preview in Another Pane` (the fast suite \
         pins it) and that `shell::perform` still has its `PreviewInOtherPane` arm. A DIFFERENT `Untitled N.md` index \
         means the collision check found something in the temp cwd — a stale swap file or an \
         on-disk `Untitled.md` — which would make this a fixture problem rather than a preview \
         one; `make_temp_cwd` gives every run a fresh directory precisely so that cannot happen. \
         A SINGLE entry naming only the preview scratch means the IN-PLACE Preview ran instead \
         of the other-pane one, so the untitled buffer is displayed nowhere. Note the NAMING revert does not reach this assertion at all: an \
         unnamed buffer makes `fnamemodify(bufname(b), ':t')` the empty string, and \
         `read_window_buffer_names` panics on it first with \"a window is bound to an unnamed \
         buffer\" — which is the same finding said one step earlier, not a different one."
    );

    let ocr_text = ocr_to_lowercase(&out_png);

    // Assertion 4 (precondition): the window was painting and the cover file's `:e` landed, so only the
    // web view can draw the marker, which licenses assertion 5.
    assert!(
        ocr_text.contains(UNSAVED_OCR_COVER),
        "{SMOKE}: the capture did not contain {UNSAVED_OCR_COVER:?} — the heading of \
         {UNSAVED_COVER_FILE}, which the EDITOR pane must hold by capture time. The likely \
         causes, in order: (a) the app was not drawing at all when grim fired; (b) the `<C-w>h` \
         never moved the current window, so the `:e` replaced the PREVIEW scratch instead and \
         there is no preview left — in which case assertion 5 below is red too and THAT is the \
         finding; (c) the `:e` did not run. Note the fixture's NAME does not contain this term, \
         so a sidebar row cannot satisfy it — only the rendered buffer can. Without this \
         assertion, a marker found by assertion 5 could have come from the editor grid rather \
         than from WebKit. PNG kept at {}. OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // --- Assertion 5 (discriminator): the web view rendered the UNSAVED buffer
    assert!(
        ocr_text.contains(UNSAVED_OCR_MARKER),
        "{SMOKE}: the capture did not contain {UNSAVED_OCR_MARKER:?} — the marker typed into a \
         buffer that was NEVER WRITTEN TO DISK. Nothing on the filesystem carries that word, no \
         sidebar row can spell it, and assertion 4 above just proved the editor pane is showing \
         {UNSAVED_COVER_FILE} rather than the untitled buffer — so the web view is the only thing \
         on this page that could have drawn it. The likely causes, in order: (a) the preview \
         bound but never rendered, i.e. `render_preview` answered the empty string or \
         `load_preview_document` was not called; (b) the web view was never created; (c) WebKit \
         drew the document but tesseract misread it at the stylesheet's 32 px h1 — check the \
         retained PNG BEFORE touching the assertion, and if the glyphs are the problem, lowercase \
         the marker rather than weakening this. This assertion shares its revert with assertion 3 \
         and has no independent one: everything that stops the preview binding also stops it \
         rendering. PNG kept at {}. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

// The preview-appearance smoke: everything down to the column-cap section is its alone, and its
// evidence is raw pixels; nothing is OCR'd or shared.

/// The markdown file both launches open and then preview.
///
/// Nothing in this smoke is OCR'd, so this name carries none of the
/// [`SIDEBAR_FILLER_FILES`] hazard and this smoke contributes no row to
/// `RELAUNCH_FIXTURE_NAMES`: a sidebar row is TEXT, and text cannot satisfy an
/// assertion whose subject is the colour of a background.
pub(crate) const PREVIEW_THEME_SOURCE_FILE: &str = "pvtheme.md";

/// The whole body of [`PREVIEW_THEME_SOURCE_FILE`]: ONE paragraph, long enough to
/// soft-wrap across the preview pane's content width, and nothing else.
///
/// ## Why a paragraph and NOT a heading, which is the load-bearing choice
///
/// Assertion 6 asks whether the document's first line box sits flush against the top
/// of the web view. That question is only about `body.wolf-markdown`'s own padding if
/// nothing ELSE can push the first block down, and `base.css` gives `p` a flat
/// `margin: 0 0 24px` — a zero top margin on EVERY paragraph, not just the first. A
/// heading would instead carry `margin-top: 48px` and depend on the edge-rule
/// selector (`body.wolf-markdown > *:not(.wolf-src):not(...)`) to reset it, so a
/// heading-led fixture would fail assertion 6 for either of TWO causes and the
/// assertion could not name which. With a paragraph, the body's padding is the only
/// term left.
///
/// ## Why it must WRAP
///
/// The sampled columns are a band across the middle of the preview pane, not the
/// whole of it, so a short first line would put its ink to the LEFT of everything
/// assertion 6 looks at and the band would read empty on a perfectly good render.
/// A paragraph that soft-wraps fills the content width edge to edge.
///
/// ## Why it must stay SHORT overall
///
/// Assertion 5's band is the lower half of the pane, and its `>= 90%` threshold
/// assumes that region is document BACKGROUND rather than text. Roughly four wrapped
/// lines of 16 px body text is about 96 px of content against a web view some 500 px
/// tall, so the band sits far below the last line. Growing this fixture eats that
/// margin; if more ink is ever needed at the TOP, make the LINE longer, not the
/// document taller.
pub(crate) const PREVIEW_THEME_SOURCE_BODY: &str =
    "alfa bravo charlie delta echo foxtrot golf hotel india \
                                         juliett kilo lima mike november oscar papa quebec romeo \
                                         sierra tango uniform victor whiskey xray yankee zulu, \
                                         and around again once more.";

/// Name suffix of run 1's config AND state directory siblings.
///
/// One suffix for both so a post-mortem pairs `…-config-preview-dark` with
/// `…-state-preview-dark` by eye. See [`smoke_state_dir_at`] for why the state
/// directories must not be shared between the two launches.
const PREVIEW_THEME_DARK_SUFFIX: &str = "-preview-dark";

/// Name suffix of run 2's config and state directory siblings. See
/// [`PREVIEW_THEME_DARK_SUFFIX`].
const PREVIEW_THEME_LIGHT_SUFFIX: &str = "-preview-light";

/// Run 1's capture, taken with its preview pane live.
///
/// Both captures live under `XDG_RUNTIME_DIR` rather than in the smoke's cwd, so the
/// app's own `FileSearch` never lists them and neither becomes a tree-sidebar row.
/// Nothing here is OCR'd, so that is the sibling smokes' convention rather than a
/// load-bearing property — but a PNG appearing in the sidebar would still change the
/// page the EDITOR pane draws, which assertion 4 samples.
const PREVIEW_THEME_DARK_PNG_NAME: &str = "wolf_gnome_preview_theme_dark.png";

/// Run 2's capture. See [`PREVIEW_THEME_DARK_PNG_NAME`].
const PREVIEW_THEME_LIGHT_PNG_NAME: &str = "wolf_gnome_preview_theme_light.png";

/// File run 1's Neovim writes its window/buffer list into.
const PREVIEW_THEME_DARK_WINS_NAME: &str = "wolf_gnome_preview_theme_wins_1.txt";

/// File run 2's Neovim writes its window/buffer list into.
const PREVIEW_THEME_LIGHT_WINS_NAME: &str = "wolf_gnome_preview_theme_wins_2.txt";

/// The `background-color` `wolf_core`'s `markdown/themes/default.css` gives
/// `body.wolf-markdown` in DARK, as `(r, g, b)`.
///
/// RE-TYPED here as a literal, never imported from `wolf-core`, and that is the
/// whole point rather than a stylistic preference: this smoke's claim is that the
/// pixels on screen are THIS colour, and reading the value out of the crate that
/// produced them would make the assertion a comparison with itself — it would hold
/// for whatever the stylesheet became, including a stylesheet that stopped
/// distinguishing the two appearances at all. [`PREVIEW_OCR_STATUS`] sets the same
/// precedent for a string.
///
/// MEASURED against the real WebKitGTK, not read off the CSS:
/// `getComputedStyle(body).backgroundColor` is `rgb(27, 27, 27)` with
/// `StyleManager::set_color_scheme(ForceDark)` applied before any widget exists.
pub(crate) const PREVIEW_DARK_BG: (u8, u8, u8) = (0x1b, 0x1b, 0x1b);

/// The same in LIGHT, from the `@media (prefers-color-scheme: light)` block. See
/// [`PREVIEW_DARK_BG`] for why it is re-typed rather than imported.
///
/// MEASURED the same way: `rgb(244, 244, 244)` under `ForceLight`, on a desktop that
/// was reading DARK before the call — so the forced value OVERRIDES the ambient one
/// rather than coinciding with it.
const PREVIEW_LIGHT_BG: (u8, u8, u8) = (0xf4, 0xf4, 0xf4);

/// How many rows below the web view's own top edge assertion 6 counts ink in.
///
/// ONE baseline unit, re-typed rather than imported from `base.css`'s
/// `--wolf-baseline`, because the document's first line box is exactly that tall:
/// `body.wolf-markdown` sets `line-height: 24px` as a LENGTH, so every line box in
/// the body is 24 px whatever font size sits inside it.
///
/// ## Why not more, and why not less
///
/// The named falsifier is `padding-top: 48px` on `body.wolf-markdown` — TWO baseline
/// units — so any band up to 48 rows tall is emptied by it. Stopping at 24 leaves a
/// factor of two in hand on the falsifying side, which is where the discrimination
/// lives. Going below 24 would start to depend on where inside its line box the
/// first line's ink falls, which is a font-metric question this binary must not take a
/// position on.
///
/// ## The premise it rests on, stated rather than assumed
///
/// That the leading `.wolf-src` marker span `markdown.rs` flushes at the top of the
/// first block costs NO vertical space. It is an empty, zero-size inline anchor, so
/// per CSS 2.1 §10.8 the anonymous block box around it holds a line box that must be
/// treated as not existing — but specs/markdown-stylesheet.md#edge-rules calls that
/// corner engine-dependent, so it is MEASURED rather than reasoned: against the
/// real `render_html` output in WebKitGTK 2.52.4, `body` padding read `0px` on all
/// four sides and the first block's top offset read `0`. If a leading anchor ever
/// did cost a line box, the first ink would land at row 24 and this band would read
/// empty on a good render.
///
/// MEASURED (first-ink row inside the band, from a retained green-run capture, at
/// GTK 4.22.4 / libadwaita 1.9.0 / WebKitGTK 2.52.4): the located `web_top` is
/// absolute row 159 on BOTH launches, and the first row carrying any non-theme pixel
/// is absolute row 165 — OFFSET 6 into the band, with rows 0..5 pure background. So
/// the leading `.wolf-src` anchor costs no line box here: had it cost one, the first
/// ink would have landed at offset 24 and this band would have read empty. Offset 6
/// against a 24-row band leaves the whole rest of the first line box in hand, so 24
/// stands and is NOT raised towards 48 — raising it would only weaken the
/// discrimination the falsifier depends on.
const FLUSH_BAND_PX: f64 = 24.0;

/// The fewest non-theme pixels assertion 6 accepts inside the flush band.
///
/// ACCEPTANCE CRITERION, not a tuned number: it must sit far above what stray
/// antialiasing at a band edge could produce, and far below what a wrapped line of
/// 16 px body text across the sampled columns really draws. The estimate behind the
/// value: the first line box's glyph rows are roughly two thirds of its 24, the
/// sampled columns are 354 wide, and text covers something like a fifth of that
/// area — order 1,000 pixels, so 200 is a fivefold margin.
///
/// MEASURED (actual ink count in a green run): 1,987 non-theme pixels on run 1
/// (dark) and 1,987 on run 2 (light) — just under TEN times this floor, and twice
/// the order-1,000 estimate above. The two launches read the SAME count because the
/// two documents differ only in colour, so the set of pixels that are not the
/// background is the same set of glyph pixels in both. Under the named falsifier
/// (`padding-top: 48px`) both counts read 0. No change to the fixture is needed.
const MIN_FLUSH_INK_PX: usize = 200;

/// The most of assertion 4's CONTROL band that may be the launch's theme colour.
///
/// A ceiling rather than an exact zero: the editor pane draws glyphs, and an
/// antialiased edge between the editor's foreground and background sweeps a range of
/// colours. 2% is far above any plausible coincidence and far below the ~100% a real
/// collision would produce, so the two readings are never ambiguous.
const CONTROL_MAX_THEME_FRACTION: f64 = 0.02;

/// The least of assertion 5's SAMPLE band that must be the launch's own theme
/// colour.
///
/// Not 100%: the band is inside the web view, and a document that grew — or a WebKit
/// that drew a scrollbar — would put a little non-background ink in it. The
/// dominant colour of a whole-screen preview frame measured 79.8% with the app
/// chrome included; inside the pane, with the fixture deliberately short, the band
/// is background almost everywhere.
const SAMPLE_MIN_THEME_FRACTION: f64 = 0.90;

/// The most of assertion 5's sample band that may be the OTHER appearance's theme
/// colour.
///
/// The half that makes assertion 5 a DISCRIMINATOR rather than a presence check: a
/// launch that rendered the wrong appearance would put the other colour at ~100%
/// here, and the two thresholds cannot both be satisfied by one frame.
const SAMPLE_MAX_OTHER_FRACTION: f64 = 0.01;

/// Whether `pixel` is the rendered LINK's ink rather than body text or background.
///
/// Both clauses are load-bearing and each rejects a different thing. `b >
/// [`LINK_BLUE_MIN_B`]` rejects the dark theme's `#1b1b1b` background; the
/// blue-over-red margin rejects its `#d4d4d4` body text, whose blue channel is 212 and
/// would sail past the first clause on its own. The theme's link colour `#4ea1ff`
/// clears the first by 105 and the second by 177.
///
/// The margin is computed in `u16` rather than `u8` so a red channel near 255 cannot
/// wrap the comparison around — an overflow there would silently classify a bright
/// warm pixel as link ink.
fn is_link_blue(pixel: Rgb8) -> bool {
    let (r, _g, b) = pixel;
    b > LINK_BLUE_MIN_B && u16::from(b) > u16::from(r) + LINK_BLUE_MIN_MARGIN
}

/// The bounding box of the link ink [`find_link_cluster`] located, plus how many
/// pixels went into it.
///
/// The bounds are INCLUSIVE on both axes, matching how the measurement recorded them
/// (`x [788, 1002]`), so [`Self::width`] and [`Self::height`] add one.
struct LinkCluster {
    /// Leftmost column carrying link ink.
    x0: usize,
    /// Rightmost column carrying link ink.
    x1: usize,
    /// Topmost row carrying link ink.
    y0: usize,
    /// Bottommost row carrying link ink.
    y1: usize,
    /// How many pixels inside the search rectangle satisfied [`is_link_blue`].
    count: usize,
}

impl LinkCluster {
    /// The bounding box's width in pixels.
    fn width(&self) -> usize {
        self.x1 - self.x0 + 1
    }

    /// The bounding box's height in pixels.
    fn height(&self) -> usize {
        self.y1 - self.y0 + 1
    }

    /// Whether `(x, y)` — an injected pointer coordinate — falls inside the box.
    ///
    /// Takes `i64` because that is what the injector's grammar and
    /// [`assert_inside_extents`] use; a negative coordinate is outside by definition
    /// and is answered without a cast.
    fn contains(&self, x: i64, y: i64) -> bool {
        let Ok(x) = usize::try_from(x) else {
            return false;
        };
        let Ok(y) = usize::try_from(y) else {
            return false;
        };
        (self.x0..=self.x1).contains(&x) && (self.y0..=self.y1).contains(&y)
    }
}

/// The bounding box of every [`is_link_blue`] pixel inside `rect`, or `None` when
/// there is none.
///
/// A bounding box rather than a connected-component search, deliberately: the link is
/// one run of glyphs on one line and its strokes are NOT connected to each other, so a
/// component search would find twenty-four separate `L`s and force this file to decide
/// which one the click belongs on. The box over all of them is exactly the region the
/// click point has to fall inside, and the caller's width and height ceilings are what
/// make "one line of link text" a checked property rather than an assumption.
fn find_link_cluster(capture: &Capture, rect: SampleRect) -> Option<LinkCluster> {
    let mut found: Option<LinkCluster> = None;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            if !is_link_blue(capture.pixel(x, y)) {
                continue;
            }
            found = Some(match found {
                None => LinkCluster {
                    x0: x,
                    x1: x,
                    y0: y,
                    y1: y,
                    count: 1,
                },
                Some(c) => LinkCluster {
                    x0: c.x0.min(x),
                    x1: c.x1.max(x),
                    y0: c.y0.min(y),
                    y1: c.y1.max(y),
                    count: c.count + 1,
                },
            });
        }
    }
    found
}

/// The rectangle inside the PREVIEW (right) pane that
/// [`preview_link_click_repoints_the_pane_under_cage`] searches for link ink, in
/// absolute output pixels.
///
/// Built from the same pane area [`preview_theme_pane_area`] gives, because the two
/// smokes put their preview in the same half by the same other-pane split and
/// duplicating that arithmetic would be two things to keep in step. It is inset from
/// ALL FOUR of the preview pane's edges — see [`LINK_SEARCH_INSET_X_PX`] for what the
/// horizontal inset clears, and [`BAND_BOTTOM_CLEARANCE_PX`] for what the bottom one
/// does.
///
/// Derived at the pinned output size: `x 776..1270`, `y 87..600`. The MEASURED link
/// cluster is `x [788, 1002]`, `y [165, 176]`, which sits inside it with room on every
/// side.
fn link_search_rect() -> SampleRect {
    let (area_x, area_y, area_w, _) = preview_theme_pane_area();
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    SampleRect {
        x0: pixel_index(
            area_x + area_w / 2.0 + LINK_SEARCH_INSET_X_PX,
            "the link search's first column",
        ),
        x1: pixel_index(
            output_w - LINK_SEARCH_INSET_X_PX,
            "the link search's last column",
        ),
        y0: pixel_index(
            area_y + LINK_SEARCH_INSET_Y_PX,
            "the link search's first row",
        ),
        y1: pixel_index(
            output_h - BAND_BOTTOM_CLEARANCE_PX,
            "the link search's last row",
        ),
    }
}

/// The pane AREA in absolute output pixels, as `(x, y, w, h)`.
///
/// The same pair [`the_column_cap_cannot_bind_at_the_ocr_smokes_geometry`] reads, and
/// for the same reason: [`OVERLAY_ORIGIN_X_EXPECTED`] / [`OVERLAY_ORIGIN_Y_EXPECTED`]
/// name the WIDGET's corner, which is what a band inside a pane has to be built from.
/// `PANE_ORIGIN_*` names the TEXT GRID's corner and must NOT be substituted: the two
/// differ by a statusbar strip and a padding.
///
/// Derived at the pinned output size: `(251, 47, 1029, 673)`.
pub(crate) fn preview_theme_pane_area() -> (f64, f64, f64, f64) {
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    (
        OVERLAY_ORIGIN_X_EXPECTED,
        OVERLAY_ORIGIN_Y_EXPECTED,
        output_w - OVERLAY_ORIGIN_X_EXPECTED,
        output_h - OVERLAY_ORIGIN_Y_EXPECTED,
    )
}

/// The rows BOTH bands sample, as absolute output pixel indices `y0..y1`.
///
/// It starts at the pane area's vertical MIDPOINT, which puts it below everything
/// [`PREVIEW_THEME_SOURCE_BODY`] renders (about four wrapped lines, some 96 px, under
/// a web view whose top edge sits about 160 px down the output), and ends
/// [`BAND_BOTTOM_CLEARANCE_PX`] above the output's bottom edge.
///
/// Derived at the pinned output size: rows 384..600.
pub(crate) fn preview_theme_band_rows() -> (usize, usize) {
    let (_, area_y, _, area_h) = preview_theme_pane_area();
    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    let y0 = pixel_index(area_y + area_h / 2.0, "the sampled band's first row");
    let y1 = pixel_index(
        output_h - BAND_BOTTOM_CLEARANCE_PX,
        "the sampled band's last row",
    );
    (y0, y1)
}

/// The columns sampled inside the PREVIEW (right) pane, as `x0..x1`.
///
/// Derived at the pinned output size: columns 846..1200.
pub(crate) fn preview_theme_preview_columns() -> (usize, usize) {
    let (area_x, _, area_w, _) = preview_theme_pane_area();
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let x0 = pixel_index(
        area_x + area_w / 2.0 + BAND_X_INSET_PX,
        "the preview band's first column",
    );
    let x1 = pixel_index(output_w - BAND_X_INSET_PX, "the preview band's last column");
    (x0, x1)
}

/// The columns sampled inside the EDITOR (left) pane — assertion 4's control — as
/// `x0..x1`.
///
/// Derived at the pinned output size: columns 331..686.
fn preview_theme_editor_columns() -> (usize, usize) {
    let (area_x, _, area_w, _) = preview_theme_pane_area();
    let x0 = pixel_index(area_x + BAND_X_INSET_PX, "the control band's first column");
    let x1 = pixel_index(
        area_x + area_w / 2.0 - BAND_X_INSET_PX,
        "the control band's last column",
    );
    (x0, x1)
}

/// The rows assertion 6 scans for the web view's own top edge, as `y0..y1`: from the
/// pane area's top edge down to the sampled band's first row.
///
/// It deliberately starts ABOVE the web view, in the preview pane's statusbar strip,
/// so that "the first row that is entirely the theme colour" is a real transition
/// rather than the first row it happened to look at.
///
/// Derived at the pinned output size: rows 47..384.
pub(crate) fn preview_theme_scan_rows() -> (usize, usize) {
    let (_, area_y, _, _) = preview_theme_pane_area();
    let y0 = pixel_index(area_y, "the web-view scan's first row");
    let (band_y0, _) = preview_theme_band_rows();
    (y0, band_y0)
}

/// One launch's evidence, so the per-launch assertions read as ONE loop rather than
/// as two copies that can drift.
///
/// Named fields rather than a tuple: four of the six are `&str` and two are `Rgb8`,
/// which is exactly the shape a positional literal transposes silently — and a
/// transposed `own` / `other` would turn assertion 5 into its own negation while
/// still compiling.
struct ThemeLaunch<'a> {
    /// How a failure names this launch, matching the scenario's own stderr labels.
    label: &'a str,
    /// The launch's decoded capture.
    capture: &'a Capture,
    /// The theme colour this launch's `appearance` should have produced.
    own: Rgb8,
    /// The const name of `own`, for the failure message.
    own_name: &'a str,
    /// The OTHER appearance's theme colour.
    other: Rgb8,
    /// The const name of `other`, for the failure message.
    other_name: &'a str,
}

/// The two-launch PREVIEW-APPEARANCE scenario, run as one `sh -c` string inside ONE
/// `cage`: `APPEARANCE_SCENARIO`'s two-launch shape with [`PREVIEW_SCENARIO`]'s
/// preview leg inside each launch.
///
/// Each launch exports its own `WOLF_CONFIG_DIR` **and** its own `WOLF_STATE_DIR`,
/// deliberately overriding both of the ones [`cage_command`] sets. The config one is
/// what makes the appearance the variable; the state one is what keeps launch 1's
/// persisted two-pane layout out of launch 2 — see [`smoke_state_dir_at`], where the
/// consequence of sharing is written out.
///
/// The launch body is a shell FUNCTION rather than two copies, because the two runs
/// must differ in nothing but their four paths and their four exit codes: a preview
/// leg duplicated by hand is a preview leg that can be edited on one side only, and
/// this smoke's whole design is that the two launches are the same run at two
/// settings. Its nine positional parameters are saved into named variables
/// immediately, since `wait_ready` and `quit_app` rebind `$1`.
///
/// Exit codes are PER LAUNCH, `APPEARANCE_SCENARIO`'s scheme exactly: `11` run 1's
/// app died before it was ready for input; `12` run 1 was still alive after `:qa!`; `13`
/// run 1 exited non-zero; `14` run 1's window-list readback never appeared; `15`–`18`
/// the same four for run 2. Eight and not four, for the reason that smoke gives: "an
/// app died before it was ready for input" is a different investigation depending on which
/// appearance it was reading.
///
/// A `grim` failure has no code of its own: it aborts under `set -eu` and the cage
/// status carries grim's own. That is deliberate — a capture that did not happen is
/// not a preview finding, and assertion 2 names it anyway.
const PREVIEW_THEME_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
QUIT_TRIES=75
FILE_TRIES=100
# PREVIEW_SETTLE -- budgeted FROM THE CALL, not from the web view's Started event.
# PREVIEW_SCENARIO's doc comment carries the measurement behind it (~255 ms for a
# cold open, about 110 ms of that spent spawning the WebProcess before Started
# fires). It is spent TWICE here, and BOTH spends are cold opens: run 2 is a brand
# new process with a brand new WebProcess. Do not trim it.
PREVIEW_SETTLE=2.0

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
    echo "wolf-gnome preview-appearance smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# $1 is the exit code for "still alive after :qa!", $2 for "exited non-zero", and
# $3 is the run label.
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
        echo "wolf-gnome preview-appearance smoke: $3 app (pid $app_pid) still alive after :qa!." >&2
        exit "$1"
    fi
    # `set -e` would abort before the status could be reported, so collect it
    # through `||`, which is exempt from it.
    rc=0
    wait "$app_pid" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "wolf-gnome preview-appearance smoke: $3 app (pid $app_pid) exited $rc, not 0." >&2
        exit "$2"
    fi
    app_pid=
}

# ONE launch, end to end. The two runs call it with different paths and different
# exit codes and are otherwise the same run -- which is the whole design of this
# smoke, so the body must never be duplicated.
#
# The nine parameters are saved into named variables FIRST: wait_ready and
# quit_app below rebind $1..$3 for their own calls.
run_launch() {
    cfg=$1
    state=$2
    png=$3
    wins=$4
    code_died=$5
    code_alive=$6
    code_status=$7
    code_file=$8
    label=$9

    # Both exports deliberately override what `cage_command` set: the helper's
    # directories are shared, and this smoke needs the appearance to be the one
    # variable AND each launch's persistence to be its own.
    WOLF_CONFIG_DIR="$cfg"
    WOLF_STATE_DIR="$state"
    export WOLF_CONFIG_DIR WOLF_STATE_DIR

    "$WOLF_BIN" &
    app_pid=$!
    wait_ready "$app_pid" "$code_died" "wolf-gnome preview-appearance smoke $label"

    # --- Step 1: open the SOURCE in the window's ONE pane --------------------
    #
    # One pane is what makes the geometry deterministic: the other-pane Preview in
    # step 2 splits it, and a split of ONE pane can only produce two. See the
    # smoke's doc comment for the rest of that chain.
    wtype -- ":e $SRC_FILE"
    wtype -k Return
    sleep "$SETTLE"

    # --- Step 2: drive PREVIEW IN ANOTHER PANE through the command palette ----
    #
    # Ctrl+Shift+P, the query, Enter. The OTHER-PANE command and not plain Preview:
    # Preview binds in place, so from one pane it would leave no editor pane for
    # assertion 4's control band and no right half for assertion 5's sample band.
    # Neither has an accelerator row of its own, so the palette and the :Wolf* Ex
    # commands are their only two routes.
    wtype -M ctrl -M shift -k p -m shift -m ctrl
    sleep "$SETTLE"
    wtype -- "$PREVIEW_OTHER_PANE_QUERY"
    sleep "$SETTLE"
    wtype -k Return
    sleep "$PREVIEW_SETTLE"

    # --- Step 3: the capture, with the preview pane live ---------------------
    grim "$png"

    # --- Step 4: NEOVIM's own answer about what its windows hold -------------
    #
    # Taken AFTER the capture on purpose: this typed command line is drawn by the
    # cmdline strip, and the capture must not carry it. Joined with a COMMA rather
    # than a space because a preview scratch's own name contains one.
    wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ',')], '$wins')"
    wtype -k Return
    wait_file "$wins" || exit "$code_file"

    quit_app "$code_alive" "$code_status" "$label"
}

run_launch "$CONFIG_DIR_DARK" "$STATE_DIR_DARK" "$OUT_PNG_DARK" "$WINS_DARK" \
    11 12 13 14 "run 1 (appearance: dark)"

run_launch "$CONFIG_DIR_LIGHT" "$STATE_DIR_LIGHT" "$OUT_PNG_LIGHT" "$WINS_LIGHT" \
    15 16 17 18 "run 2 (appearance: light)"

exit 0
"#;

/// The rendered preview document's background follows the `appearance` in
/// `wolf-config.yaml`, and the document sits FLUSH against the top of the web view.
///
/// ## What it proves that nothing else can
///
/// It is the first test in this repository that observes
/// `adw::StyleManager::set_color_scheme` at all, and the only one whose evidence is
/// the COLOUR of a pixel. `tesseract` reads no colour — it cannot tell light-on-dark
/// from dark-on-light — so every other smoke here is structurally incapable of the
/// claim. Two launches inside ONE `cage`, at the two configured appearances, each
/// opening the same document into a preview pane and each capturing its own PNG,
/// which this test decodes with `cairo::ImageSurface::create_from_png` and samples at
/// a derived rectangle.
///
/// ## THE PAIR IS WHAT DISCRIMINATES, AND A FUTURE EDIT MUST NOT DROP ONE LAUNCH
///
/// Under `cage` there is no settings portal and no xsettings bridge, so
/// `adw::StyleManager::is_dark()` is HOST-DETERMINED and constant for the whole run.
/// This binary may not assert a value for it and does not try to. The consequence is
/// the design: with `set_color_scheme` removed, both launches render whatever the
/// ambient scheme happens to be, so one of the two launches still reads the colour it
/// expected — the WEAK half — and the other reads the opposite one and fires. WHICH
/// of the two is the weak half is not knowable from inside this binary, so both are
/// required and neither may be dropped as redundant.
///
/// This is a DIFFERENT argument from the one
/// `configured_appearance_reaches_neovims_background_under_cage` makes, and the two
/// must not be conflated. There, the weak half is fixed and knowable — `dark` is
/// Neovim's own default under `--embed`, so run 1 is weak on every host. Here the
/// weak half is whichever one the host's ambient scheme happens to agree with, and it
/// can differ between machines and between GTK versions.
///
/// MEASURED on the development host (GTK 4.22.4, libadwaita 1.9.0, WebKitGTK 2.52.4,
/// `cage` 0.3.0), by deleting `set_color_scheme` and re-running: run 1 (`appearance:
/// dark`) stayed GREEN and run 2 (`appearance: light`) fired, reading 1.0000 of
/// [`PREVIEW_DARK_BG`] where its own [`PREVIEW_LIGHT_BG`] was required. So the
/// ambient scheme reaching WebKit under `cage` here is DARK, run 1 is the weak half
/// on this host, and run 2 is the one carrying the discrimination. This is a reading
/// of one host at one moment, NOT a property of the harness — libadwaita resolves it
/// off the session's own settings, so a machine whose desktop reads light swaps the
/// two halves and the smoke is equally sound. It is recorded so a later reader knows
/// which half went red when the falsifier was applied, and does not conclude from a
/// single green launch that the other one is redundant.
///
/// ## Why the preview is deterministically the RIGHT half
///
/// The whole geometry rests on this and it is not obvious, so it is written out
/// rather than assumed. Each launch opens the source in the window's ONE pane, so:
///
/// 1. Preview in Another Pane passes `shell::SPLIT_FOR_OTHER_PANE` (`0`), so core
///    SPLITS rather than reusing a window. Nothing about what the pane holds can
///    change that: this front end never nominates a target window.
/// 2. The Lua's `wolf_do_split('v')` emits a Split intent naming the ORIGINAL window
///    as `from`.
/// 3. `pane_view::apply_split` builds `Frame::Split { children: [Leaf(target),
///    Leaf(new_pane)] }` — the target first, the new pane second.
/// 4. `PaneTree::pane_rects` walks a split's children LEFT TO RIGHT.
/// 5. `PaneTree::split` gives that new two-child frame EQUAL weights
///    (`layout::DEFAULT_RATIO` twice), so the boundary between the two panes lands
///    at the pane area's horizontal MIDPOINT — which is exactly where
///    [`preview_theme_preview_columns`] and [`preview_theme_editor_columns`] cut.
///    A change to those weights would move the boundary and silently eat part of
///    the sample band without reddening anything.
///
/// So the new pane is the right half whatever Neovim's `'splitright'` says, and the
/// source stays in the left one. Assertion 4 is what makes that self-checking rather
/// than merely argued: if the two panes were the other way round, the "control" band
/// would be inside the preview and would read ~100% of the theme colour.
///
/// ## Why assertion 4 is read FIRST, and what it licenses
///
/// Assertion 6 locates the web view's top edge by scanning DOWN the sampled columns
/// for the first row that is entirely the theme colour. Everything above the web view
/// in those columns is the preview pane's statusbar strip and its padding, painted
/// with the EDITOR's background — so the scan only means what it says if the editor's
/// background is not the theme colour. Assertion 4 is exactly that measurement, taken
/// in the other pane, which is why it is a named control rather than a formality.
///
/// ## Assertion 6 is deliberately INSENSITIVE to `pane_view::apply_preview_frame`
///
/// `web_top` is self-located, so a change to where the pane puts its web view moves
/// the band WITH the view and cannot redden anything. State that plainly, or the next
/// reader tries `apply_preview_frame` as a revert, sees green and concludes the
/// assertion is vacuous. What assertion 6 measures is the distance from the web
/// view's own top edge to the document's first ink, which is a property of the CSS
/// and of nothing in this crate.
///
/// ## What assertion 6 does and does NOT pin of the typography row
///
/// It pins ONE of that commit's four observable changes: the removed `body` padding,
/// which is the only one that is a positive pixel statement. The other three — the
/// `h1`/`h2` bottom borders and their padding, and the `blockquote` left border — are
/// ABSENCES, and proving an absence from a screenshot is the vacuity trap this
/// harness's conventions exist to prevent (an absent border and an unrendered
/// document look identical). Those three stay pinned by `markdown_render.rs`'s
/// CSS-text assertions in the fast suite, and this smoke deliberately does not
/// restate them.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator
/// this smoke does not have. The `measured` column carries the readings from actual
/// runs, per `tests/README.md`'s "A red-run demonstration is a deliverable, not a
/// formality", taken at GTK 4.22.4, libadwaita 1.9.0, WebKitGTK 2.52.4 and `cage`
/// 0.3.0 on a 1280x720 output. A cell reading `n/a` is a precondition with no red run
/// by design; the three discriminators each carry both their green and their red
/// reading.
///
/// | # | assertion | kind | falsifying edit | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the two-launch cage scenario ran to completion | none; do not invent one | n/a |
/// | 2 | both PNGs present, and each decodes at exactly `CAGE_OUTPUT_W_EXPECTED` x `CAGE_OUTPUT_H_EXPECTED` | precondition — and the one that makes every derived rectangle below mean what it says | none; do not invent one | n/a |
/// | 3 | each launch's `tabpagebuflist()` readback, sorted, is the preview scratch plus the source | precondition — a preview pane exists at all, and exactly two panes do | none; do not invent one. Without it, assertions 5 and 6 would be describing a pane that never became a preview. | n/a |
/// | 4 | fewer than [`CONTROL_MAX_THEME_FRACTION`] of the CONTROL band is this launch's theme colour | **control, self-checking** | none. Its failure means the EDITOR's background collides with the preview theme's, and the response is to RE-DERIVE the two colours against the real render — never to raise the fraction, which would disarm the premise assertion 6's `web_top` scan rests on. | green: EXACTLY 0.0000 on both launches — 0 of the control band's 76,680 pixels, not merely fewer than the ceiling. The editor pane's own background reads `(20, 22, 27)` in run 1 and `(224, 226, 234)` in run 2, so it follows the appearance too and collides with NEITHER theme colour at either setting. That the margin is the whole ceiling rather than a sliver is what makes assertion 6's `web_top` scan safe. |
/// | 5 | at least [`SAMPLE_MIN_THEME_FRACTION`] of the SAMPLE band is this launch's own theme colour, and at most [`SAMPLE_MAX_OTHER_FRACTION`] is the other appearance's | **discriminator** — the configured appearance reaches the rendered document | delete `manager.set_color_scheme(decided.color_scheme);` from `appearance::apply_at_launch`. Isolated: the whole fast suite and every other smoke stay green, because nothing else in the repository observes that call. | green: 1.0000 own / 0.0000 other on BOTH launches — the sample band is uniformly one colour, all 76,464 pixels of it. Red: RUN 2 (light) fired, reading `0.0000 its own PREVIEW_LIGHT_BG … and 1.0000 the other appearance's PREVIEW_DARK_BG`; run 1 (dark) stayed green. So the ambient scheme under `cage` on this host is DARK and run 1 is the weak half — see the pair-discriminates section above. Isolation CONFIRMED by a full `cargo test --workspace` under the falsifier: 49 of the target's 50 tests and every other target passed, this smoke alone failed. |
/// | 5b | the same, with the theme's own light palette removed | **discriminator** — the light palette is what `prefers-color-scheme: light` selects | delete the `@media (prefers-color-scheme: light)` block from `wolf-core`'s `markdown/themes/default.css`; run 2 reddens. | red: run 2 (light) fired with the same 0.0000 own / 1.0000 other reading as 5's. `markdown_render.rs` REDDENED alongside — 43 passed, 2 failed: `default_theme_carries_dark_and_light_palettes` ("the light media query must mirror the light-mode link color `#0969da`") and `hr_occupies_whole_baselines_and_is_painted_by_its_border` ("the LIGHT (`prefers-color-scheme: light`) theme must color the hr via `border-top-color: #d0d0d0`"). Red smoke over a RED fast suite, so this pairing reads as the THEME being wrong rather than the applier — which is the diagnostic separating 5b from 5, where the fast suite stayed fully green. |
/// | 6 | with `web_top` self-located from the pixels, the first [`FLUSH_BAND_PX`] rows carry at least [`MIN_FLUSH_INK_PX`] non-theme pixels | **discriminator** — the document is flush against the top of the view | add `padding-top: 48px;` to `body.wolf-markdown` in `wolf-core`'s `markdown/base.css`; both launches redden. | green: 1,987 non-theme pixels in the band on run 1 and 1,987 on run 2, against the 200 floor, with `web_top` located at row 159 on both and the first ink 6 rows into the band. Red: 0 on BOTH launches (the harness reports run 1 and stops, so run 2's 0 is read out of its own retained capture — the scenario runs both launches before any assertion). The fast suite STAYED GREEN under the falsifier: `markdown_render.rs` 45 passed, `wolf-core` unit tests 223 passed, exactly as predicted — 48 is a multiple of 24, so the grid sweep accepts it and the `padding` shorthand assertion is untouched. No fallback to `padding: 48px 40px` was needed. |
///
/// ## The conventions it obeys
///
/// A fresh temp working directory ([`make_temp_cwd`], mandatory — the captive
/// Neovim's swap directory is keyed on it); [`require_smoke_env`]'s cage slot and runtime directory for the whole body; one
/// `sh -c` string through [`cage_command`], the single environment helper and the
/// only place the compositor is spawned; per-launch config AND state directories; and
/// eight exit codes naming eight causes, four per launch.
///
/// It asserts no OCR term, so it contributes no row to `ASSERTED_OCR_TERMS`, and it
/// creates its one fixture only in its own cwd where nothing reads it as text, so it
/// contributes no row to `RELAUNCH_FIXTURE_NAMES` either. That the seeded config
/// really carries an `appearance` key at all — the one field `wolf_core` defaults to
/// `auto`, which under `cage` resolves against a settings portal that does not exist
/// — is guarded once for the whole file by
/// [`the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes`], so it is
/// not restated here.
#[test]
fn preview_document_follows_the_configured_appearance_under_cage() {
    const SMOKE: &str = "preview_document_follows_the_configured_appearance_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Nothing here is OCR'd; the fillers keep this smoke's page the same shape as the others'.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[(PREVIEW_THEME_SOURCE_FILE, PREVIEW_THEME_SOURCE_BODY)],
    );

    // Both launches run at SMOKE_FONT_PT: the appearance is the ONE variable.
    let config_dark = smoke_config_dir_at(
        &cwd,
        PREVIEW_THEME_DARK_SUFFIX,
        SMOKE_FONT_PT,
        APPEARANCE_FORCED_DARK,
    );
    let config_light = smoke_config_dir_at(
        &cwd,
        PREVIEW_THEME_LIGHT_SUFFIX,
        SMOKE_FONT_PT,
        APPEARANCE_FORCED_LIGHT,
    );
    let state_dark = smoke_state_dir_at(&cwd, PREVIEW_THEME_DARK_SUFFIX);
    let state_light = smoke_state_dir_at(&cwd, PREVIEW_THEME_LIGHT_SUFFIX);

    assert_ne!(
        config_dark,
        config_light,
        "{SMOKE}: both runs were pointed at the SAME config directory {}, so they would read the \
         same appearance and one of the two halves of assertion 5 could never hold. The two \
         suffixes ({PREVIEW_THEME_DARK_SUFFIX:?} and {PREVIEW_THEME_LIGHT_SUFFIX:?}) must differ.",
        config_dark.display()
    );
    assert_ne!(
        state_dark,
        state_light,
        "{SMOKE}: both runs were pointed at the SAME state directory {}. Launch 1 quits with a \
         preview pane open, so its `state.json` records a two-leaf layout with a live preview leaf \
         — which launch 2 would then RESTORE, so its own Preview press would split a SECOND \
         time and put its preview in a pane none of the derived \
         rectangles below is aimed at. See `smoke_state_dir_at`.",
        state_dark.display()
    );

    let png_dark = runtime_dir.join(PREVIEW_THEME_DARK_PNG_NAME);
    let png_light = runtime_dir.join(PREVIEW_THEME_LIGHT_PNG_NAME);
    let wins_dark = runtime_dir.join(PREVIEW_THEME_DARK_WINS_NAME);
    let wins_light = runtime_dir.join(PREVIEW_THEME_LIGHT_WINS_NAME);
    for path in [&png_dark, &png_light, &wins_dark, &wins_light] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(PREVIEW_THEME_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        .env("SRC_FILE", PREVIEW_THEME_SOURCE_FILE)
        .env("PREVIEW_OTHER_PANE_QUERY", PREVIEW_OTHER_PANE_QUERY)
        .env("CONFIG_DIR_DARK", &config_dark)
        .env("CONFIG_DIR_LIGHT", &config_light)
        .env("STATE_DIR_DARK", &state_dark)
        .env("STATE_DIR_LIGHT", &state_light)
        .env("OUT_PNG_DARK", &png_dark)
        .env("OUT_PNG_LIGHT", &png_light)
        .env("WINS_DARK", &wins_dark)
        .env("WINS_LIGHT", &wins_light);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless preview-appearance scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage preview-appearance scenario exited unsuccessfully ({status}). Every \
         code names ONE cause in ONE launch. Exit 11: run 1's app (`appearance: \
         {APPEARANCE_FORCED_DARK}`) died before it was ready for input — `appearance::apply_at_launch` \
         runs before any widget is built and takes the whole launch with it, so a panic there is \
         the first thing to look at. Exit 12: run 1 was still alive after `:qa!`. Exit 13: run 1 \
         exited NON-ZERO — it went away, but not cleanly, which is a separate code from 12 because \
         a panic in the GTK main loop makes a process vanish exactly as a clean shutdown does. \
         Exit 14: run 1's window-list readback never appeared at {}. Exit 15/16/17/18: the same \
         four for run 2 (`appearance: {APPEARANCE_FORCED_LIGHT}`), whose readback is at {}. Any \
         OTHER status is `grim` or `wtype` failing under `set -eu`, and a grim failure has no code \
         of its own on purpose: a capture that did not happen is not a preview finding. Note this \
         assertion says nothing about whether a preview ever opened — the scenario exits 0 whatever \
         the palette did.",
        wins_dark.display(),
        wins_light.display()
    );

    // Assertion 2 (precondition): presence first, then `decode_capture`'s checks. Every rectangle here
    // is derived from the size it asserts, so read its doc comment first.
    assert_png_present(SMOKE, &png_dark);
    assert_png_present(SMOKE, &png_light);
    let dark = decode_capture(&png_dark, SMOKE, "run 1 (appearance: dark)");
    let light = decode_capture(&png_light, SMOKE, "run 2 (appearance: light)");

    // Assertion 3 (precondition): Neovim's own window buffer names, read before any pixel, since a band
    // inside a pane that never became a preview fails assertion 5 for nothing.
    let want_windows = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{PREVIEW_THEME_SOURCE_FILE}"),
            PREVIEW_THEME_SOURCE_FILE.to_string(),
        ];
        want.sort();
        want
    };
    for (label, path) in [
        ("run 1 (appearance: dark)", &wins_dark),
        ("run 2 (appearance: light)", &wins_light),
    ] {
        let got = read_window_buffer_names(path, SMOKE);
        assert_eq!(
            got, want_windows,
            "{SMOKE}: after {label}'s Preview leg the captive Neovim's windows hold {got:?}, not \
             {want_windows:?} (both sorted, so window order is not the subject). A SINGLE entry \
             naming {PREVIEW_THEME_SOURCE_FILE:?} means no preview was bound at all — the palette \
             dispatched nothing, or `shell::perform` has no `OpenPreview` arm, or the \
             {PREVIEW_OTHER_PANE_QUERY:?} query matched no row. A SINGLE entry naming the \
             preview scratch means the IN-PLACE Preview ran rather than the other-pane one: the \
             preview replaced the \
             window's only pane, so there is no editor pane for assertion 4's control band and no \
             right half for assertion 5's sample band. THREE entries mean it split twice, which \
             also moves the preview out of the right half. Every rectangle below is derived on the \
             assumption of exactly two side-by-side panes, so none of them means anything until \
             this holds."
        );
    }

    // Asserts per launch, so a red run reports only the first failing one; the second capture is on
    // disk regardless, so read it before concluding it was green.
    for launch in [
        ThemeLaunch {
            label: "run 1 (appearance: dark)",
            capture: &dark,
            own: PREVIEW_DARK_BG,
            own_name: "PREVIEW_DARK_BG",
            other: PREVIEW_LIGHT_BG,
            other_name: "PREVIEW_LIGHT_BG",
        },
        ThemeLaunch {
            label: "run 2 (appearance: light)",
            capture: &light,
            own: PREVIEW_LIGHT_BG,
            own_name: "PREVIEW_LIGHT_BG",
            other: PREVIEW_DARK_BG,
            other_name: "PREVIEW_DARK_BG",
        },
    ] {
        assert_preview_theme_launch(SMOKE, &launch);
    }
}

/// Assertions 4, 5 and 6 for ONE launch of
/// [`preview_document_follows_the_configured_appearance_under_cage`].
///
/// Split out so the two launches run the identical checks: a per-launch block written
/// twice is a per-launch block that can be edited on one side only, and this smoke's
/// whole design is that the two launches differ in exactly one setting.
fn assert_preview_theme_launch(smoke: &str, launch: &ThemeLaunch<'_>) {
    let label = launch.label;
    let capture = launch.capture;
    let (band_y0, band_y1) = preview_theme_band_rows();
    let (preview_x0, preview_x1) = preview_theme_preview_columns();
    let (editor_x0, editor_x1) = preview_theme_editor_columns();

    let control = SampleRect {
        x0: editor_x0,
        x1: editor_x1,
        y0: band_y0,
        y1: band_y1,
    };
    let sample = SampleRect {
        x0: preview_x0,
        x1: preview_x1,
        y0: band_y0,
        y1: band_y1,
    };

    // Non-vacuity: a collapsed or off-edge band would panic in the sampler or report clean percentages
    // over nothing.
    for (what, rect) in [("control", control), ("sample", sample)] {
        assert!(
            rect.x0 < rect.x1
                && rect.y0 < rect.y1
                && rect.x1 <= capture.width
                && rect.y1 <= capture.height,
            "{smoke}: {label}'s {what} band {rect:?} is degenerate or falls outside the \
             {}x{} capture. The bands are derived from CAGE_OUTPUT_*_EXPECTED, \
             OVERLAY_ORIGIN_*_EXPECTED, BAND_X_INSET_PX and BAND_BOTTOM_CLEARANCE_PX, so this \
             means two of those have crossed — most likely the overlay origin moved right or down \
             past the inset. Re-measure the overlay origin; do NOT shrink the insets, which are \
             what keep the bands clear of the `gtk::Paned` handle and the command-line strip.",
            capture.width,
            capture.height
        );
    }

    // Assertion 4 (control, self-checking): read first, since 6's `web_top` scan needs the editor's
    // background to differ from the preview theme's colour.
    let control_hits = capture.count_exactly(control, launch.own);
    let control_fraction = band_fraction(
        control_hits,
        control.pixels(),
        "the control band's theme-colour fraction",
    );
    assert!(
        control_fraction < CONTROL_MAX_THEME_FRACTION,
        "{smoke}: {label}: {control_fraction:.4} of the CONTROL band {control:?} — inside the \
         LEFT, editor pane — is exactly {} = {:?}, which is at or above the \
         {CONTROL_MAX_THEME_FRACTION} ceiling. Two readings are possible and they are told apart \
         by the size of the number. A fraction near 1.0 means the two panes are the wrong way \
         round: the preview landed LEFT, so this band is inside the web view and the smoke's whole \
         geometry is inverted — check `pane_view::apply_split`'s \
         child order and `PaneTree::pane_rects`. A fraction just above the ceiling means the \
         captive Neovim's own background has COLLIDED with the preview theme's, at which point \
         assertion 6 below can no longer tell the pane chrome from the document. The response to \
         either is to RE-DERIVE the two theme colours against a real render and to re-measure the \
         editor background beside them. Do NOT raise this ceiling: it is not a tolerance, it is \
         the statement that makes assertion 6 mean anything.",
        launch.own_name,
        launch.own
    );

    // --- Assertion 5 (discriminator): the CONFIGURED appearance reached the DOM
    let own_hits = capture.count_exactly(sample, launch.own);
    let other_hits = capture.count_exactly(sample, launch.other);
    let own_fraction = band_fraction(
        own_hits,
        sample.pixels(),
        "the sample band's own-theme fraction",
    );
    let other_fraction = band_fraction(
        other_hits,
        sample.pixels(),
        "the sample band's other-theme fraction",
    );
    assert!(
        own_fraction >= SAMPLE_MIN_THEME_FRACTION && other_fraction <= SAMPLE_MAX_OTHER_FRACTION,
        "{smoke}: {label}: the SAMPLE band {sample:?} — inside the RIGHT, preview pane — is \
         {own_fraction:.4} its own theme colour {} = {:?} (needs >= {SAMPLE_MIN_THEME_FRACTION}) \
         and {other_fraction:.4} the other appearance's {} = {:?} (needs <= \
         {SAMPLE_MAX_OTHER_FRACTION}). The likely causes, in order. (a) The OTHER colour is \
         dominant: the configured appearance never reached WebKit, and the chain to check is \
         `appearance::apply_at_launch` calling `StyleManager::set_color_scheme`, libadwaita \
         propagating that to GtkSettings' `gtk-application-prefer-dark-theme` — which upstream \
         does only while the running GTK is OLDER than 4.23.1 — and WebKitGTK reading that \
         property for `prefers-color-scheme`. A GTK bump past 4.23.1 removes that propagation \
         SILENTLY, and this assertion is the only thing in the repository that would notice. (b) \
         NEITHER colour is dominant: the web view is showing something that is not the rendered \
         document — `about:blank` (white), or the empty string `WolfCore::render_preview` answers \
         on an RPC failure. (c) Both fractions are small but the band is not uniform: the fixture \
         grew tall enough to reach the lower half of the pane, which is a fixture bug — shorten \
         `PREVIEW_THEME_SOURCE_BODY` rather than lowering the threshold. Captures are retained; \
         read the band's actual colours out of them before changing any number here.",
        launch.own_name,
        launch.own,
        launch.other_name,
        launch.other
    );

    // Assertion 6 (discriminator): `web_top` is self-located, so this measures the view's top edge to
    // the first ink. See the smoke's doc comment.
    let scan_rows = preview_theme_scan_rows();
    let web_top = capture
        .first_uniform_row((preview_x0, preview_x1), scan_rows, launch.own)
        .unwrap_or_else(|| {
            panic!(
                "{smoke}: {label}: no row between {} and {} is entirely {} = {:?} across columns \
                 {preview_x0}..{preview_x1}, so the web view's own top edge could not be located \
                 and assertion 6 has nothing to measure from. Assertion 5 just passed, so the band \
                 lower down IS the theme colour — which leaves a document that starts with \
                 full-width ink at its very first row (an `hr` or a table as the first block would \
                 do it, and neither belongs in this fixture) or a web view whose top edge is below \
                 the scan's end at row {}.",
                scan_rows.0, scan_rows.1, launch.own_name, launch.own, scan_rows.1
            )
        });

    let flush_height = pixel_index(FLUSH_BAND_PX, "FLUSH_BAND_PX");
    let flush = SampleRect {
        x0: preview_x0,
        x1: preview_x1,
        y0: web_top,
        y1: web_top + flush_height,
    };
    assert!(
        flush.y1 <= capture.height,
        "{smoke}: {label}: the flush band {flush:?} runs off the bottom of the {}x{} capture. The \
         web view's located top edge is row {web_top}, which is far lower than the pane chrome can \
         account for.",
        capture.width,
        capture.height
    );

    let flush_ink = flush.pixels() - capture.count_exactly(flush, launch.own);
    assert!(
        flush_ink >= MIN_FLUSH_INK_PX,
        "{smoke}: {label}: the first {FLUSH_BAND_PX} rows below the web view's own top edge (row \
         {web_top}) carry only {flush_ink} non-{} pixels, fewer than the {MIN_FLUSH_INK_PX} floor. \
         The document is NOT flush against the top of its view: something above the first line box \
         is painting pure background where the first line's ink should be. The named cause is \
         `body.wolf-markdown` regaining vertical padding in `wolf-core`'s `markdown/base.css` — \
         the document is flush to the pane edges BY DESIGN, and any inset belongs to the pane \
         chrome. Two things this is NOT. It is not a change to \
         `pane_view::apply_preview_frame`: {web_top} is located from the pixels, so moving the web \
         view moves this band with it and cannot redden it. And it is not the first block's own \
         top margin: the fixture's first block is a PARAGRAPH, and `base.css` gives `p` a flat \
         `margin: 0 0 24px`, so no edge-rule selector is on this path. If a green run ever \
         measures the first ink BELOW row {FLUSH_BAND_PX}, re-measure and raise FLUSH_BAND_PX \
         towards 48 with the reason recorded — do not lower this floor.",
        launch.own_name
    );
}

// --- A preview view created after launch renders without a portal ------------

/// New-view smoke: the window-list readback taken BEFORE the split, with the preview
/// live in the right of two panes.
///
/// Under `XDG_RUNTIME_DIR` for `PICKOVER_WINS_NAME`'s reason, and registered in
/// `RELAUNCH_FIXTURE_NAMES` for the same one: the typed `:call writefile(...)` echoes
/// its absolute path in the cmdline strip.
pub(crate) const NEWVIEW_WINS_BEFORE_NAME: &str = "wolf_gnome_newview_wins_a.txt";

/// The same readback taken AFTER the split. See [`NEWVIEW_WINS_BEFORE_NAME`].
pub(crate) const NEWVIEW_WINS_AFTER_NAME: &str = "wolf_gnome_newview_wins_b.txt";

/// Where the scenario writes `dbus-send`'s reply to `ListActivatableNames`, before the
/// app launches. Under `XDG_RUNTIME_DIR`; written by the shell, so nothing draws it.
const NEWVIEW_ACTIVATABLE_NAME: &str = "wolf_gnome_newview_activatable.txt";

/// The CONTROL capture: the preview live in the view it opened with, before the split.
/// Retained after the run, as `PICKOVER_CONTROL_PNG_NAME` is.
const NEWVIEW_CONTROL_PNG_NAME: &str = "wolf_gnome_newview_control.png";

/// The SAMPLE capture: the same preview after the split.
const NEWVIEW_SAMPLE_PNG_NAME: &str = "wolf_gnome_newview_sample.png";

/// Name suffix of this smoke's config directory sibling.
const NEWVIEW_SUFFIX: &str = "-newview";

/// How many pixels of exactly [`PREVIEW_DARK_BG`] a row or a column of the pane area
/// must carry before [`colour_extent`] counts it as part of the preview.
///
/// An acceptance criterion rather than a measurement. Nothing but a rendered preview
/// paints that exact colour — the control capture asserts its extent lies in the
/// preview's half — so a coincidence would have to line up this many pixels in one
/// line; and a preview pane after the split, in a half of the 1029-pixel pane area cut
/// in two either way, is still well over this wide and tall.
const NEWVIEW_EXTENT_MIN_RUN_PX: usize = 96;

/// The bus daemon's own name, which `ListActivatableNames` may report on a bus with no
/// service directory. Re-typed, as every bus name in this binary is.
const BUS_DAEMON_NAME: &str = "org.freedesktop.DBus";

/// The desktop portal's well-known name: the one service whose activation supplies the
/// `gtk-xft-dpi` the app's own pin exists for, and so the one this smoke's bus must not
/// offer.
const PORTAL_BUS_NAME: &str = "org.freedesktop.portal.Desktop";

/// The NEW-VIEW scenario, run as a single `sh -c` string inside ONE `cage`:
/// `PICKOVER_SCENARIO`'s split-and-preview opening, a CONTROL capture, then Edit in
/// Another Pane from the preview — [`PREVIEW_SCENARIO`]'s step 5, which splits the
/// preview's own pane — and a SAMPLE capture.
///
/// ## Why the split is of the preview's pane
///
/// The defect needs a preview web view created after launch on a host GTK has not yet
/// allocated. The view the in-place Preview opens is created in a pane already on
/// screen and renders whatever the dpi; measured, the preview after a split of its own
/// pane is shown by a view created with the new layout, first rendered while its host
/// is 0×0. That is [`PREVIEW_SCENARIO`]'s step 5 exactly, the leg its assertion 10
/// reads.
///
/// ## The bus probe comes first
///
/// Before the app launches, `dbus-send` asks this cage's bus what it can activate and
/// writes the reply under the runtime directory. The defect only shows where no portal
/// can be activated, so the smoke states that premise from the bus itself rather than
/// from the configuration file alone.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `14` — the readback before the split never appeared.
/// * `15` — the readback after the split never appeared: the first leg's keys
///   reached Neovim, so the palette leg is what lost them.
/// * `17` — `dbus-send` failed or is missing, so nothing below can say which bus the
///   app ran on.
const NEWVIEW_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
FILE_TRIES=100
# PREVIEW_SETTLE -- budgeted FROM THE CALL; PREVIEW_SCENARIO's doc comment carries the
# measurement. Spent after the cold open and again after the split.
PREVIEW_SETTLE=2.0

# --- Step 0: what this cage's private bus can activate -----------------------
dbus-send --session --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus.ListActivatableNames >"$ACTIVATABLE" || exit 17

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome new-view smoke"

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
    echo "wolf-gnome new-view smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 1: the OTHER file in the starting pane -----------------------------
wtype -- ":e $OTHER_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: split, and open the SOURCE in the new (right) pane --------------
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"

wtype -- ":e $SRC_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 3: the IN-PLACE Preview, into the right pane -----------------------
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$PREVIEW_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$PREVIEW_SETTLE"

# --- Step 4: NEOVIM's own answer before the split ----------------------------
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ',')], '$WINS_BEFORE')"
wtype -k Return
wait_file "$WINS_BEFORE" || exit 14

# Let the cmdline strip clear before the control capture.
sleep "$SETTLE"

# --- Step 5: the CONTROL capture ---------------------------------------------
grim "$OUT_PNG_CONTROL"

# --- Step 6: EDIT IN ANOTHER PANE, from the preview --------------------------
# The active pane IS the preview, so this splits the preview's own pane and binds the
# source into the new half; the preview after the split is a view created then.
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$EDIT_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$PREVIEW_SETTLE"

# --- Step 7: NEOVIM's own answer after the split -----------------------------
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ',')], '$WINS_AFTER')"
wtype -k Return
wait_file "$WINS_AFTER" || exit 15
sleep "$SETTLE"

# --- Step 8: the SAMPLE capture ----------------------------------------------
grim "$OUT_PNG_SAMPLE"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The extent of the rows and columns of `area` that each carry at least `min_run`
/// pixels of exactly `colour`: from the first such row to the last, and the first such
/// column to the last. `None` when no row or no column qualifies.
///
/// A count per line rather than a run, so the glyphs drawn across a row of a document
/// lower its count without breaking the extent; and a threshold rather than any pixel,
/// so a stray coincidence of the colour elsewhere cannot widen it.
fn colour_extent(
    capture: &Capture,
    area: SampleRect,
    colour: Rgb8,
    min_run: usize,
) -> Option<SampleRect> {
    let rows: Vec<usize> = (area.y0..area.y1)
        .filter(|&y| {
            (area.x0..area.x1)
                .filter(|&x| capture.pixel(x, y) == colour)
                .count()
                >= min_run
        })
        .collect();
    let columns: Vec<usize> = (area.x0..area.x1)
        .filter(|&x| {
            (area.y0..area.y1)
                .filter(|&y| capture.pixel(x, y) == colour)
                .count()
                >= min_run
        })
        .collect();
    Some(SampleRect {
        x0: *columns.first()?,
        x1: *columns.last()? + 1,
        y0: *rows.first()?,
        y1: *rows.last()? + 1,
    })
}

/// A Markdown preview whose web view is created after launch, on a host GTK has not yet
/// allocated, renders its document — on a session bus that can activate no desktop
/// portal.
///
/// ## What it proves that nothing else does
///
/// Measured: with no portal reachable, GTK leaves `gtk-xft-dpi` unset, and a WebView
/// created after launch whose host is still 0×0 when it first renders lays its page
/// out at a 0×0 viewport and stays there — realized, mapped and sized, its DOM present,
/// and blank. The preview after a split of its own pane is such a view. The app pins
/// `gtk-xft-dpi` itself when it is unset, in `crates/wolf-gnome/src/app.rs`, and this
/// smoke is that pin's end-to-end evidence. A NixOS host masked the defect, because a
/// cage's bus could activate the host's portal through its service directories, while
/// every container and every minimal desktop showed it; [`smoke_bus_config`] makes
/// every host the second kind, and [`SMOKE_GTK_SETTINGS_INI`] deliberately sets no dpi.
///
/// [`preview_pane_renders_and_edit_source_returns_under_cage`]'s assertion 10 makes the
/// same claim by OCR after the same split. This smoke makes it on pixels, with a control
/// taken in the same launch, and with the bus premise asserted rather than assumed.
///
/// ## The pair, and the extent
///
/// The control capture is taken with the preview live in the view it opened with; the
/// sample after Edit in Another Pane has split the preview's pane. The split moves the
/// preview, so neither capture is read at a fixed rectangle: in each, the preview is
/// the [`colour_extent`] of [`PREVIEW_DARK_BG`] over the pane area above the cmdline
/// strip, at [`NEWVIEW_EXTENT_MIN_RUN_PX`]. Inside it both captures must clear the
/// same two floors, [`PICKOVER_CONTROL_MIN_DARK_FRACTION`] of that background and
/// [`PICKOVER_CONTROL_MIN_INK_PX`] of [`PICKOVER_HEADING_INK`], the `h1`'s pure white.
///
/// **Ink is the sole discriminator.** The 0×0-viewport view draws no document but does
/// show its `PREVIEW_DARK_BG` across the whole pane, so it has an extent and passes the
/// dark fraction — measured at 0.9994 — and reads ZERO white pixels. The extent only
/// locates the preview and the dark fraction only says the extent is the preview; zero
/// ink is the defect, or an empty document, which the control rules out for the same
/// source. The floors are reused from the picker smoke rather than re-measured, and
/// the readings below sit far from both.
///
/// A green sample does not prove the pin by itself: a dpi from anywhere else renders
/// the view just as well. Assertion 8 is what ties the green to the pin — the cage log
/// must carry its [`DPI_PIN_STDERR_PREFIX`] line. It comes LAST, so removing the pin
/// still reaches assertion 7 and reads the discriminator's own red.
///
/// ## Assertions, classified
///
/// | # | assertion | kind | falsifying edit | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the scenario ran to completion | none; do not invent one | n/a |
/// | 2 | the bus's `ListActivatableNames` reply was read, and names nothing but the bus daemon itself — no [`PORTAL_BUS_NAME`] | precondition — the configuration this smoke's red needs: a portal reachable supplies the dpi and masks the defect | add `<standard_session_servicedirs/>` to [`SMOKE_BUS_CONFIG_TEMPLATE`]; on a host whose data directories carry a portal service this row reddens naming it | unmeasured |
/// | 3 | both PNGs present, each decoding at the pinned output size | precondition | none; do not invent one | n/a |
/// | 4 | readback before is the preview scratch plus the other file | precondition — a preview exists in one of exactly two panes before the split | none; do not invent one | unmeasured |
/// | 5 | readback after is the preview scratch, the source and the other file | precondition — Edit in Another Pane split the preview's pane and kept the preview | none; do not invent one | unmeasured |
/// | 6 | in the CONTROL, the extent lies in the right half and clears both floors | **paired control** — the view the preview opened with rendered the document, so a red sample is about the view created by the split and not the cold open | blank the render (`WolfCore::render_preview` answering the empty string) | unmeasured |
/// | 7 | in the SAMPLE, an extent exists and clears both floors | **discriminator** — the view created by the split renders its document | remove the `gtk-xft-dpi` pin in `crates/wolf-gnome/src/app.rs` | RED with the pin removed: the sample's extent `SampleRect { x0: 959, x1: 1260, y0: 159, y1: 600 }`, dark fraction 0.9994, ink **0**, against the control's extent at `x0: 787` with ink 1,976. GREEN with the pin: the sample's extent the same rectangle, dark 0.9624, ink 1,976, against floors of 0.8 and 500 |
/// | 8 | the cage log carries [`DPI_PIN_STDERR_PREFIX`] | **non-vacuity** — the pin ran, so a green assertion 7 is the pin's evidence and not a dpi from elsewhere | add `gtk-xft-dpi=98304` to [`SMOKE_GTK_SETTINGS_INI`]: assertion 7 stays green and this row reddens (as does the guard's check 15). Removing the pin reddens assertion 7 first | unmeasured |
///
/// Without that pin, row 7 is red on every host, which is this smoke's expected red
/// rather than a flake. On a bus that can activate a portal, or with a dpi reaching GTK
/// any other way, the revert stays green; row 2, the guard's check 15 and row 8 are
/// what refuse those.
///
/// ## The conventions it obeys
///
/// A fresh temp working directory; [`require_smoke_env`]'s cage slot and runtime
/// directory for the whole body; one `sh -c` string through [`cage_command`]; a config
/// directory of its own pinning the dark appearance the two colours are measured at; and
/// distinct exit codes for each readback and for the bus probe. It asserts no OCR term,
/// so it contributes no `ASSERTED_OCR_TERMS` row; it reuses the picker smoke's two
/// fixtures, already in `RELAUNCH_FIXTURE_NAMES`, and registers its two readbacks.
#[test]
fn preview_created_after_launch_renders_without_a_portal_under_cage() {
    const SMOKE: &str = "preview_created_after_launch_renders_without_a_portal_under_cage";
    const CONTROL_LABEL: &str = "the control capture (preview live, before the split)";
    const SAMPLE_LABEL: &str = "the sample capture (after the preview's pane split)";
    // How `dbus-send --print-reply` prints an array and one string element of it.
    const REPLY_ARRAY_OPEN: &str = "array [";
    const REPLY_STRING_HEAD: &str = "string \"";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd, mandatory: the captive Neovim's swap directory is keyed on it.
    let cwd = make_temp_cwd();
    let source_body = format!("{PICKOVER_SOURCE_HEADING}\n\n{PICKOVER_SOURCE_PARAGRAPH}");
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (PICKOVER_SOURCE_FILE, source_body.as_str()),
            (PICKOVER_OTHER_FILE, PICKOVER_OTHER_HEADING),
        ],
    );

    // Forced dark, for the picker smoke's reason: both colours read below are dark-palette values.
    let config = smoke_config_dir_at(&cwd, NEWVIEW_SUFFIX, SMOKE_FONT_PT, APPEARANCE_FORCED_DARK);

    let control_png = runtime_dir.join(NEWVIEW_CONTROL_PNG_NAME);
    let sample_png = runtime_dir.join(NEWVIEW_SAMPLE_PNG_NAME);
    let wins_before = runtime_dir.join(NEWVIEW_WINS_BEFORE_NAME);
    let wins_after = runtime_dir.join(NEWVIEW_WINS_AFTER_NAME);
    let activatable = runtime_dir.join(NEWVIEW_ACTIVATABLE_NAME);
    for path in [
        &control_png,
        &sample_png,
        &wins_before,
        &wins_after,
        &activatable,
    ] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(NEWVIEW_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        // Chained AFTER `cage_command`, deliberately winning over the shared config directory.
        .env("WOLF_CONFIG_DIR", &config)
        .env("SRC_FILE", PICKOVER_SOURCE_FILE)
        .env("OTHER_FILE", PICKOVER_OTHER_FILE)
        .env("PREVIEW_QUERY", PREVIEW_QUERY)
        .env("EDIT_QUERY", EDIT_QUERY)
        .env("OUT_PNG_CONTROL", &control_png)
        .env("OUT_PNG_SAMPLE", &sample_png)
        .env("WINS_BEFORE", &wins_before)
        .env("WINS_AFTER", &wins_after)
        .env("ACTIVATABLE", &activatable);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless new-view scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage new-view scenario exited unsuccessfully ({status}). Exit 17: \
         `dbus-send` failed or is not on PATH — it comes from the devShell's `pkgs.dbus`, beside \
         `dbus-run-session` — so nothing says which bus the app ran on. Exit 11: the app died \
         before it was ready for input, a LAUNCH failure. Exit 14: the readback BEFORE the \
         split never appeared at {}, so keys are not reaching Neovim at all. Exit 15: the \
         readback AFTER the split never appeared at {}, although the first one did, so the \
         Edit in Another Pane leg lost the keyboard. Any OTHER status is `grim` or `wtype` \
         failing under `set -eu`. This assertion says nothing about what the preview drew.",
        wins_before.display(),
        wins_after.display()
    );

    // --- Assertion 2 (precondition): the bus can activate no portal -------------
    let reply = read_readback(&activatable, SMOKE);
    let names: Vec<&str> = reply
        .lines()
        .filter_map(|line| line.trim().strip_prefix(REPLY_STRING_HEAD))
        .map(|rest| rest.trim_end_matches('"'))
        .collect();
    let activatable_services: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| *name != BUS_DAEMON_NAME)
        .collect();
    let verdict = if !reply.contains(REPLY_ARRAY_OPEN) {
        "NOT a `dbus-send --print-reply` array reply at all, so nothing below could say which \
         services the bus offered"
    } else if activatable_services.contains(&PORTAL_BUS_NAME) {
        "a bus that can activate the desktop PORTAL: the configuration carries a service \
         directory that reaches one, the portal supplies the dpi, and assertion 7 would stay \
         green with the app's `gtk-xft-dpi` pin deleted"
    } else if !activatable_services.is_empty() {
        "a bus with service directories: no portal among them today, but the configuration is \
         not the service-less one `smoke_bus_config` writes"
    } else {
        "a bus that activates nothing, as required"
    };
    assert!(
        reply.contains(REPLY_ARRAY_OPEN) && activatable_services.is_empty(),
        "{SMOKE}: the cage's bus answered ListActivatableNames with {names:?}, which is {verdict}. \
         It must name nothing but {BUS_DAEMON_NAME:?}. Check \
         `every_cage_gets_a_private_runtime_dir_and_no_host_session`'s checks 11 and 12, which \
         read the same configuration statically. Raw reply at {}:\n{reply}",
        activatable.display()
    );

    // Assertion 3 (precondition): presence first, so "grim wrote nothing" is not a decode error.
    assert_png_present(SMOKE, &control_png);
    assert_png_present(SMOKE, &sample_png);
    let control = decode_capture(&control_png, SMOKE, CONTROL_LABEL);
    let sample = decode_capture(&sample_png, SMOKE, SAMPLE_LABEL);

    // --- Assertion 4 (precondition): a preview in one of two panes, before -----
    let preview_name = format!("{PREVIEW_READBACK_PREFIX}{PICKOVER_SOURCE_FILE}");
    let want_before = {
        let mut want = vec![preview_name.clone(), PICKOVER_OTHER_FILE.to_string()];
        want.sort();
        want
    };
    let got_before = read_window_buffer_names(&wins_before, SMOKE);
    assert_eq!(
        got_before, want_before,
        "{SMOKE}: before the split the captive Neovim's windows hold {got_before:?}, not \
         {want_before:?} (both sorted). No preview scratch means no preview was bound — the \
         palette dispatched nothing or {PREVIEW_QUERY:?} matched no row. A SINGLE entry means \
         the `<C-w>v` split never happened. The control below assumes exactly two side-by-side \
         panes with the preview in the RIGHT one."
    );

    // --- Assertion 5 (precondition): the preview's pane split, the preview kept --
    let want_after = {
        let mut want = vec![
            preview_name.clone(),
            PICKOVER_SOURCE_FILE.to_string(),
            PICKOVER_OTHER_FILE.to_string(),
        ];
        want.sort();
        want
    };
    let got_after = read_window_buffer_names(&wins_after, SMOKE);
    let has_preview = got_after.contains(&preview_name);
    let after_verdict = if got_after == want_before {
        "the same two windows: Edit in Another Pane did nothing, so no view was created and the \
         sample proves nothing — check `edit_press`'s preview gate and that EDIT_QUERY still \
         selects the other-pane row"
    } else if !has_preview && got_after.len() == 2 {
        "TWO windows and NO preview: the in-place Edit Source replaced the preview instead of \
         splitting, so there is no preview left to observe"
    } else if !has_preview {
        "NO preview window: the split lost it on Neovim's side, which is a different defect from \
         a blank view"
    } else {
        "an unexpected window set: a missing other file means core reused that window rather \
         than splitting"
    };
    assert_eq!(
        got_after, want_after,
        "{SMOKE}: after Edit in Another Pane the captive Neovim's windows hold {got_after:?}, not \
         {want_after:?} (both sorted): {after_verdict}."
    );

    // Assertions 6 and 7 find the preview in each capture the same way: the extent of its own
    // background over the pane area, above the cmdline strip.
    let (area_x, area_y, area_w, _) = preview_theme_pane_area();
    let (_, band_y1) = preview_theme_band_rows();
    let pane_area = SampleRect {
        x0: pixel_index(area_x, "the pane area's left edge"),
        x1: control.width,
        y0: pixel_index(area_y, "the pane area's top edge"),
        y1: band_y1,
    };
    let pane_mid_x = pixel_index(area_x + area_w / 2.0, "the pane area's horizontal midpoint");
    let read = |capture: &Capture, what: &str| -> Option<(SampleRect, f64, usize)> {
        let extent = colour_extent(
            capture,
            pane_area,
            PREVIEW_DARK_BG,
            NEWVIEW_EXTENT_MIN_RUN_PX,
        )?;
        let dark = capture.count_exactly(extent, PREVIEW_DARK_BG);
        let ink = capture.count_exactly(extent, PICKOVER_HEADING_INK);
        Some((extent, band_fraction(dark, extent.pixels(), what), ink))
    };

    // --- Assertion 6 (paired control): the opening view rendered the document --
    let control_reading = read(&control, "the control's dark fraction");
    assert!(
        control_reading.is_some_and(|(extent, dark, ink)| {
            extent.x0 >= pane_mid_x
                && dark >= PICKOVER_CONTROL_MIN_DARK_FRACTION
                && ink >= PICKOVER_CONTROL_MIN_INK_PX
        }),
        "{SMOKE}: in {CONTROL_LABEL}, the extent of PREVIEW_DARK_BG = {PREVIEW_DARK_BG:?} over \
         the pane area {pane_area:?}, its dark fraction and its PICKOVER_HEADING_INK count read \
         {control_reading:?}. It must lie right of the pane midpoint ({pane_mid_x}), be at least \
         {PICKOVER_CONTROL_MIN_DARK_FRACTION} background and carry at least \
         {PICKOVER_CONTROL_MIN_INK_PX} white pixels. This is the CONTROL, taken in the view the \
         preview opened with, before any split, so the sample below would prove nothing whatever \
         it read. None: no preview background anywhere, so the cold open drew nothing. An extent \
         reaching LEFT of the midpoint: something besides the preview paints that exact colour, \
         and the extent cannot be trusted to be the preview. Ink of ZERO: an empty document. \
         Capture kept at {}.",
        control_png.display()
    );

    // --- Assertion 7 (discriminator): the view created by the split renders it --
    let sample_reading = read(&sample, "the sample's dark fraction");
    assert!(
        sample_reading.is_some_and(|(_, dark, ink)| {
            dark >= PICKOVER_CONTROL_MIN_DARK_FRACTION && ink >= PICKOVER_CONTROL_MIN_INK_PX
        }),
        "{SMOKE}: in {SAMPLE_LABEL}, the extent of PREVIEW_DARK_BG, its dark fraction and its \
         PICKOVER_HEADING_INK count read {sample_reading:?}, against the control's \
         {control_reading:?}; the floors are {PICKOVER_CONTROL_MIN_DARK_FRACTION} and \
         {PICKOVER_CONTROL_MIN_INK_PX}. The control passed, so the document rendered before the \
         split and this is the view the split created.\n\
         Read the shape of the failure. An extent with a passing dark fraction and ZERO ink is \
         the named cause — measured exactly so with the pin removed (dark 0.9994, ink 0): GTK's \
         `gtk-xft-dpi` is unset, which it is wherever no portal answers, and WebKitGTK laid the \
         new view's page out at a 0×0 viewport because its host was not yet allocated when it \
         first rendered, so the view's background shows and no document does. Check that the \
         app's `gtk-xft-dpi` pin in crates/wolf-gnome/src/app.rs still runs before any preview \
         view exists; an empty document reads the same, but the control drew this source. None \
         means no preview background anywhere (the view drew white, or the editor's background \
         shows through). A low dark fraction with ink present is an extent spanning more than \
         the preview. Captures kept at {} and {}.",
        control_png.display(),
        sample_png.display()
    );

    // --- Assertion 8 (non-vacuity): the pin is what rendered it ----------------
    // Last, so removing the pin still reaches assertion 7's own red.
    let log_path = runtime_dir.join(CAGE_LOG_NAME);
    let log = std::fs::read_to_string(&log_path).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: could not read the cage log {}: {e}. `cage_command` sends the app's stderr \
             there, so this smoke cannot say whether the dpi pin ran.",
            log_path.display()
        )
    });
    assert!(
        log.contains(DPI_PIN_STDERR_PREFIX),
        "{SMOKE}: the cage log {} carries no {DPI_PIN_STDERR_PREFIX:?} line, so the app's \
         `gtk-xft-dpi` pin did NOT fire: GTK came up with a dpi already set, and assertion 7's \
         green is that dpi's doing, not the pin's — the smoke would pass with the pin deleted. \
         Find where the dpi came from: a portal on the cage's bus (assertion 2 should have \
         caught it), a `gtk-xft-dpi` in SMOKE_GTK_SETTINGS_INI (guard check 15), a user \
         settings.ini under XDG_CONFIG_HOME (guard check 16), or GTK's compiled-in sysconfdir \
         gtk-4.0/settings.ini, which nothing in this harness replaces. If the app's line was \
         renamed instead, re-type DPI_PIN_STDERR_PREFIX from crates/wolf-gnome/src/app.rs.",
        log_path.display()
    );
}
