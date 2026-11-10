//! Recovery smokes: a crashed Neovim restarted, a stalled one raising the stall overlay, and
//! the crash overlay's Restart button.

use super::*;

// Crash and restart smokes: shared fixtures, OCR terms and the failing-respawn wrapper.
// Each OCR term is in `ASSERTED_OCR_TERMS`, so the sidebar-filler guard covers it.

/// Crash-smoke fixture file, dropped into both smokes' temp cwd.
///
/// ## Vacuity analysis (the fixture-naming rule, applied)
///
/// The name must NOT contain [`RESTART_OCR_HEADING`], and `bfile` does not: the tree
/// sidebar renders every cwd file BY NAME and nvim's `:e` toast echoes the filename
/// too, so a fixture called `bounceback.md` would satisfy every `bounceback`
/// assertion in both smokes whether or not Neovim was ever restarted — the exact
/// silent false positive that let `picker_opens_and_loads_selected_file_under_cage`
/// pass while the picker provably never opened (see `PICKER_FILE_NAME`).
///
/// It is also what makes smoke B's must-NOT-contain assertion non-vacuous in the
/// other direction: during the crash-overlay dwell the sidebar is NOT covered (the
/// overlay is the `gtk::Paned`'s end child, so it spans the editor only) and it
/// still renders this row — and `bfile` is not `bounceback`, so a `bounceback` hit
/// can only have come from the editor grid showing through.
const RESTART_FILE_NAME: &str = "bfile.md";

/// Heading on line 1 of [`RESTART_FILE_NAME`]. Lowercased, the rendered line
/// contains [`RESTART_OCR_HEADING`]; nothing else on the page can produce it.
const RESTART_HEADING: &str = "# BOUNCEBACK";

/// How many filler lines [`restart_fixture_body`] writes below the heading.
///
/// LOAD-BEARING for smoke B, not decoration. Its probe click at
/// `(PROBE_X, PROBE_Y)` must land on a REAL cell — measured 2026-08-04 as
/// `line 5, col 17` — or [`assert_probe_cell_and_pane_origin`]'s `MIN_PROBE_CELL`
/// precondition fires and the whole pane-origin derivation (and therefore the button
/// point) means nothing. A one-line fixture would clamp every click to `(1, 1)`.
/// Nineteen filler lines plus the heading is twenty buffer lines, comfortably more
/// than the measured `winheight(0)` of 11. The margin scales the safe way: what this
/// constant has to beat is the VISIBLE row count, which the per-pane chrome reduces.
const RESTART_FILLER_LINE_COUNT: usize = 19;

/// How many `x`s each filler line carries after its three-digit ordinal, so every
/// filler line is exactly [`FIXTURE_LINE_WIDTH`] characters — the same width, and
/// for the same two reasons, as the `:put` fixture in [`POINTER_PREAMBLE`]: wide
/// enough that a click at the probe column (measured 2026-08-04 as column 17) is not
/// clamped to end-of-line, and narrow enough that `opt.wrap` / `opt.linebreak` never
/// soft-wrap it.
///
/// It is also the slack in [`RESTART_UNSAVED_MARKER`]'s zero-margin fit: 3 + this +
/// the marker is the composed width that must not exceed the pane's `winwidth(0)`,
/// and this term is the one to shorten if it ever does, since it is inert page
/// density and the marker is an asserted term.
const RESTART_FILLER_LINE_PAD: usize = 27;

/// Lowercased substring OCR must find to prove the FIXTURE BUFFER is on screen and
/// active.
///
/// ## Vacuity analysis
///
/// It lives only in the fixture's CONTENT ([`RESTART_HEADING`]). The filename is
/// [`RESTART_FILE_NAME`] = `bfile.md`, so neither the sidebar row nor nvim's `:e`
/// toast can produce it, and no other smoke's fixture, filler or heading contains
/// it. In smoke A it therefore means "the crashed Neovim was restarted AND the
/// active file was reopened AND it is the focused buffer"; in smoke B its ABSENCE
/// during the dwell means "the overlay really is opaque and full-bleed".
pub(crate) const RESTART_OCR_HEADING: &str = "bounceback";

/// Lowercased substring OCR must find to prove the transient "Neovim was restarted"
/// banner flashed.
///
/// ## Vacuity analysis
///
/// It appears only in `liveness_ui::RESTARTED_BANNER_MESSAGE`. It is NOT a substring
/// of `CRASHED_RETRYING_MESSAGE` ("Neovim crashed — restarting…"),
/// `CRASHED_FAILED_MESSAGE`, `STALLED_MESSAGE` or `RESTART_BUTTON_LABEL` ("Restart
/// Neovim") — and that is pinned EXECUTABLY by
/// `wolf_gnome::liveness_ui::tests::overlay_message_table`, which asserts both
/// directions over every mode rather than leaving it to this comment. Nothing in the
/// editor grid, the sidebar or a Neovim toast produces it either.
pub(crate) const RESTART_OCR_BANNER: &str = "restarted";

/// Lowercased substring OCR must find to prove the crash OVERLAY is up.
///
/// ## Vacuity analysis
///
/// It appears only in the two crash-overlay titles. It is not producible by the
/// editor grid, by the sidebar (mechanically guarded by
/// `sidebar_filler_names_cannot_satisfy_any_smoke_assertion`, and by the
/// fixture-naming rule for each smoke's own files), or by a Neovim toast. That it
/// cannot cross-satisfy [`RESTART_OCR_BANNER`] — in either direction, and including
/// via the button label that is on screen at exactly the moment smoke B captures —
/// is pinned executably by
/// `wolf_gnome::liveness_ui::tests::overlay_message_table`; do not restate the
/// argument here, read that test.
///
/// Smoke B asserts this term against the state that genuinely DWELLS: a crash whose
/// automatic restart FAILED (`CRASHED_FAILED_MESSAGE`). Nothing asserts on
/// `CRASHED_RETRYING_MESSAGE`, which is a sub-second transient on the success path
/// and is not observable by a screenshot taken from a shell script.
pub(crate) const RESTART_OCR_OVERLAY: &str = "crashed";

/// The UNSAVED EDIT smoke A makes before the crash: APPENDED (`A`) to the end of
/// **line 2** of [`RESTART_FILE_NAME`] — the first filler line — and then flushed to
/// that buffer's swap file with `:preserve`. All-uppercase and word-like, mirroring
/// `FOCUS_MARKER` and `NEWBUF_MARKER`; lowercased, the rendered line contains
/// [`RESTART_OCR_UNSAVED`].
///
/// ## Why line 2 and not line 1 — the banner lies across screen ROW 0
///
/// The "Neovim was restarted" banner ([`RESTART_OCR_BANNER`]) is a small `osd` pill
/// at `halign(Center)` / `valign(Start)` of the editor area, so it is painted ACROSS
/// screen row 0, roughly columns 19-27 at the measured cage geometry. Smoke A asserts
/// the banner AND the recovered marker on the SAME capture, so the marker must not
/// share that row.
///
/// MEASURED 2026-08-02, with the marker appended to line 1 (making row 0
/// `# BOUNCEBACK UNSAVEDBURST`, 25 columns): while the banner was up, row 0 OCR'd as
/// `% bounceback unsavergh r tures` — the banner overlaps the marker's tail and
/// destroys it; once the banner auto-dismissed the same row read
/// `% bounceback unsavedburst` cleanly. The two assertions were therefore mutually
/// exclusive in one frame. Moving the edit one line down separates them: row 0 keeps
/// only [`RESTART_HEADING`] (12 columns, ending well left of the banner) and the
/// marker rides row 1, which the banner does not reach.
///
/// ## The LEADING space is part of the const
///
/// The edit is appended directly after the filler line's `x`s, so without it row 1
/// would end `...xxxxUNSAVEDBURST` and the marker would share a token with the
/// filler. `contains` would still match (it is a substring either way), but the space
/// costs nothing and keeps it a separate token, which is the shape tesseract reads
/// best.
///
/// ## Why APPENDED rather than inserted at column 1 — the block cursor
///
/// After the restart the fixture is reopened with a fresh `:edit`, and with no shada
/// mark to restore (the SIGKILLed Neovim never wrote one) the cursor lands on line 1,
/// column 1. The block cursor INVERTS that cell, and a letter under it is not a
/// letter tesseract reads: `window_renders_typed_text_under_cage` exists in its
/// current shape precisely because of that (its scenario parks the cursor off the
/// typed row before capturing). MEASURED in this feature's pass-1 manual sanity run:
/// the post-recovery capture OCR'd as `% BOUNCEBACK` — the `#` in column 1, and only
/// that cell, came back wrong, while every other glyph on the row was read correctly.
///
/// So column 1 must stay the heading's `#`, which is asserted on by nobody. The
/// marker is on another row entirely, and appending keeps it at the far RIGHT of that
/// row (columns 31-43), clear of anything the cursor can reach.
///
/// ## Why the recovered line CANNOT soft-wrap — re-derived, not measured
///
/// `runtime/nvim/init.lua` sets `opt.wrap` + `opt.linebreak`, so a line longer than
/// `winwidth(0)` breaks across screen rows and the asserted word could be split.
/// After the edit, line 2 is a filler line ([`FIXTURE_LINE_WIDTH`] = 30 characters)
/// plus this const (13 characters) = **43 characters**, and the smoke asserts that
/// composed width against the `winwidth(0)` Neovim itself reported rather than
/// against any assumed number — so the derivation cannot rot silently. If the fixture
/// or the marker ever grows past the reported width, shorten one of them; do not
/// weaken the assertion.
///
/// ## ZERO MARGIN, recorded rather than fixed
///
/// That assertion passes at exactly `43 <= 43`. The pane's `winwidth(0)` is **43**
/// (measured 2026-08-04), which is precisely the composed line's width: the fixture
/// fits with nothing to spare. It is green, and it is one character from red.
///
/// **This is a note, not a licence to buy margin.** Shortening this marker would
/// change an `ASSERTED_OCR_TERMS` entry, and widening the pane is not this
/// constant's to do. What it means in practice: any further growth of the per-pane
/// chrome — a taller strip, more padding — takes a column off this pane and turns the
/// zero margin into a soft-wrapped line 2, at which point the smoke fails on the
/// FIXTURE precondition with a message that says so, which is the correct place for
/// it to fail. Read a red there as "the chrome grew", and fix it by shortening
/// [`RESTART_FILLER_LINE_PAD`] — which is inert page density — rather than the
/// marker.
const RESTART_UNSAVED_MARKER: &str = " UNSAVEDBURST";

/// Lowercased substring OCR must find in the POST-RECOVERY capture, and must NOT
/// find in the pre-crash CONTROL capture, to prove the unsaved edit SURVIVED the
/// SIGKILL — i.e. that the reopened buffer is the SWAP-RECOVERED one rather than a
/// clean re-read from disk.
///
/// ## Vacuity analysis
///
/// It lives only in [`RESTART_UNSAVED_MARKER`], which is typed into the buffer and
/// exists nowhere on disk: no fixture file, no filler file, no readback file and no
/// temp-cwd path contains it, so neither the tree sidebar nor a Neovim `:e` toast can
/// produce it. Checked against every `ASSERTED_OCR_TERMS` entry in BOTH directions
/// — `unsavedburst` contains none of them and none of them contains it — and against
/// all nine [`SIDEBAR_FILLER_FILES`] names, which
/// `sidebar_filler_names_cannot_satisfy_any_smoke_assertion` then re-checks
/// mechanically.
///
/// ## Why the CONTROL capture's must-NOT-contain half is not optional
///
/// The control is taken while everything is still healthy, BEFORE the edit is typed,
/// so this term must be absent from it. Without that half, a marker that leaked into
/// the fixture body, into a filler name or onto a sidebar row would satisfy the
/// post-recovery assertion vacuously — the same silent false positive documented on
/// `PICKER_FILE_NAME`, and the reason both halves are asserted rather than one.
pub(crate) const RESTART_OCR_UNSAVED: &str = "unsavedburst";

/// The input burst smoke A fires at Neovim immediately BEFORE the `kill -9`.
///
/// Eight four-letter groups (39 characters) of `z`s, typed into the COMMAND LINE
/// (`:` + this text) and never executed — the dead Neovim takes the half-typed
/// cmdline to its grave.
///
/// ## Why it is a cmdline burst and not a buffer edit any more
///
/// Its only job is to have a stream of redraw wakes still in flight when `mark_dead`
/// fires — the adversarial half of the wake-coalescing argument (see
/// [`CRASH_SCENARIO`]). Every keystroke into the cmdline produces a `cmdline_show`
/// redraw, so 39 characters produce a comparable flood to 39 characters of insert
/// mode, and the kill still follows the last keystroke with NO settle.
///
/// What a cmdline burst does NOT do is touch the buffer, and that is the point: the
/// unsaved edit this smoke asserts on ([`RESTART_UNSAVED_MARKER`]) is typed and
/// `:preserve`d EARLIER, and polled for. A second BUFFER edit here would land after
/// that flush, so whether the swap contained it would be a race against
/// `'updatecount'` — exactly the non-determinism the `:preserve` step exists to
/// remove — and it would make the recovered line the marker rides (and therefore the
/// wrap derivation on [`RESTART_UNSAVED_MARKER`]) non-deterministic too.
///
/// It is still made of `z`s, and must still contain no `ASSERTED_OCR_TERMS` entry,
/// so the burst can never satisfy one of them, whatever a capture catches on screen.
const RESTART_BURST_TEXT: &str = "zzzz zzzz zzzz zzzz zzzz zzzz zzzz zzzz";

/// Smoke B's `/bin/sh` wrapper, written into the temp cwd and handed to the app as
/// `WOLF_NVIM_BIN` **on the `cage` `Command` only**.
///
/// The name must not contain any `ASSERTED_OCR_TERMS` entry, because the tree
/// sidebar lists it: note in particular that it is `nvimwrap.sh` and NOT something
/// like `restart-wrapper.sh`, whose row would be a hair away from
/// [`RESTART_OCR_BANNER`].
const RESTART_WRAPPER_NAME: &str = "nvimwrap.sh";

/// The file whose EXISTENCE makes [`RESTART_WRAPPER_NAME`] refuse to exec Neovim.
///
/// Smoke B `touch`es it before the kill (so the single automatic restart attempt
/// fails and the overlay dwells) and `rm`s it before clicking the button (so the
/// user-driven restart succeeds). Same naming constraint as the wrapper.
const RESTART_FAIL_MARKER_NAME: &str = "failmark";

/// The file [`RESTART_WRAPPER_NAME`] appends one line to every time it REFUSES to
/// exec Neovim, i.e. once per automatic restart attempt the front-end makes while
/// [`RESTART_FAIL_MARKER_NAME`] exists.
///
/// It is what actually measures `liveness_ui::MAX_AUTO_RESTART_ATTEMPTS`. A refused
/// wrapper exits BEFORE its `exec`, so it is never a process named `nvim` and the
/// `/proc` scan is structurally blind to it — MEASURED by running the revert: with the cap set
/// to `u32::MAX` the scan-only check left smoke B fully GREEN. Same naming
/// constraint as the wrapper: the tree sidebar lists it.
const RESTART_ATTEMPTS_NAME: &str = "spawnlog";

/// Builds the body of [`RESTART_FILE_NAME`]: the heading, then
/// [`RESTART_FILLER_LINE_COUNT`] lines of exactly [`FIXTURE_LINE_WIDTH`] characters
/// (a three-digit ordinal plus [`RESTART_FILLER_LINE_PAD`] `x`s), mirroring the
/// `fill_buffer` fixture in [`POINTER_PREAMBLE`].
fn restart_fixture_body() -> String {
    let mut body = String::from(RESTART_HEADING);
    body.push('\n');
    for line in 1..=RESTART_FILLER_LINE_COUNT {
        body.push_str(&format!(
            "{line:03}{}\n",
            "x".repeat(RESTART_FILLER_LINE_PAD)
        ));
    }
    body
}

// Stall smoke: SIGSTOP, then Ctrl+Shift+E, since `list_buffers` is a blocking RPC a chord reaches.
// It captures after the block: no frame is committed while the main loop is blocked.

/// Stall smoke: the fixture opened before the wedge, so the editor has something
/// distinctive on screen for the opaque overlay to cover.
///
/// Its name contains neither asserted term, for the reason documented on
/// `PICKER_FILE_NAME`: the sidebar renders it and nvim's `:e` echoes it, so a
/// name carrying [`STALL_OCR_ABSENT`] would break the opacity assertion no matter
/// how opaque the overlay was.
pub(crate) const STALL_FILE: &str = "stfrozen.md";

/// Heading written into [`STALL_FILE`]; lowercased it contains
/// [`STALL_OCR_ABSENT`].
const STALL_HEADING: &str = "# STALLFIXTURE";

/// Lowercased substring OCR must find in the CONTROL capture and must NOT find in
/// the post-stall one.
///
/// The two directions do different jobs. In the control it is the precondition
/// that the fixture rendered at all, without which the post-stall absence would be
/// vacuous — an editor that drew nothing satisfies it trivially. In the post-stall
/// capture its absence is what says the overlay is OPAQUE and full-bleed rather
/// than merely present.
pub(crate) const STALL_OCR_ABSENT: &str = "stallfixture";

/// Lowercased substring OCR must find in the POST-STALL capture and must NOT find
/// in the control: the distinctive word of `liveness_ui::STALLED_MESSAGE`
/// ("Neovim stopped responding").
///
/// ## Vacuity analysis
///
/// It appears in no other overlay string. That it cannot be produced by either
/// crash-overlay message, by the restart banner, or by the "Restart Neovim" button
/// label — which sits INSIDE the overlay and is therefore on screen at exactly the
/// moment this smoke captures — is pinned EXECUTABLY by
/// `wolf_gnome::liveness_ui::tests::overlay_message_table`, in both directions. Do
/// not restate that argument here; read that test. Nothing in the editor grid, the
/// sidebar or a Neovim toast produces it either, and the fixture-name guards
/// re-check that mechanically.
pub(crate) const STALL_OCR_PRESENT: &str = "responding";

/// `WOLF_RPC_TIMEOUT_MS` for the stall smoke, as the string the app parses.
///
/// `WolfCore::start` reads it once from the process environment, so chaining
/// `.env(…)` onto the `cage` command reaches the app through `cage -- sh -c`. A
/// TIMING KNOB ONLY — no assertion depends on its value — but it has two bounds it
/// must sit between: comfortably above a healthy round trip (sub-millisecond,
/// measured) so nothing times out while Neovim is running, and comfortably below
/// the scenario's post-chord sleep so the timeout has certainly elapsed before
/// `grim` fires.
///
/// It is safe to shorten the budget for this run because the buffer picker's
/// snapshot is the ONLY `block_on_rpc` the scenario reaches: with a fresh
/// per-smoke `WOLF_STATE_DIR` there is no saved layout, so
/// `pane_view::apply_layout_rebuild` — the crate's only other one — never runs.
const STALL_RPC_TIMEOUT_MS: &str = "800";

/// The `/proc` scan the crash smokes and the STALL smoke use to find — and to
/// keep watch on — the captive Neovim child, as one `sh` fragment of pure function
/// definitions.
///
/// Its stderr echoes are prefixed `wolf-gnome smoke:` rather than naming one
/// scenario, because three share it and a message claiming to come from the
/// crash smoke while the stall smoke was running would mis-route the first minute
/// of every diagnosis. No assertion reads those strings.
///
/// The stall smoke uses only `wait_nvim_pid` and `assert_app_alive` (it stops its
/// Neovim rather than killing it, and never expects a respawn); the rest of the
/// fragment is inert for it.
///
/// ## Why `/proc` + `sed` and not `pgrep` / `ps`
///
/// Neither is in the flake devShell, and adding `procps` would mean a `flake.nix`
/// change that must keep the Darwin shell evaluable, for no benefit whatsoever.
/// `sed` is already in every scenario's toolchain, and `/proc/<pid>/status` carries
/// both fields the scan needs.
///
/// ## The rule these functions exist to enforce: KILL THE NVIM, NEVER THE APP
///
/// The app's own `Name:` is `wolf-gnome`, so it can never be a candidate, and
/// `wolf-gnome` spawns no other children (it never calls `start_search`, so no `rg`;
/// no portal is ever spawned). A match therefore requires `Name: nvim` AND
/// `PPid: <the app>`, and the `find_nvim_pid` shell function insists on EXACTLY
/// one — anything else is exit 17, because a kill aimed at an unidentified target
/// makes everything downstream meaningless.
///
/// ## Zombies are excluded, and that is load-bearing rather than fastidious
///
/// `kill -0` succeeds on a zombie and `/proc/<pid>` still exists for one, so a
/// SIGKILLed-but-unreaped Neovim would look ALIVE to a naive scan. Two things depend
/// on getting this right:
///
///  * `assert_app_alive` would otherwise pass on a DEAD app that the scenario shell
///    has not yet reaped — and "the app died with its Neovim" is the worst possible
///    failure of this feature, so it must never be able to hide behind a `kill -0`.
///  * `wait_nvim_gone` would otherwise time out (exit 18) on precisely the red run
///    whose predicted symptom is exit 19 — with the liveness read removed there is
///    no restart, hence no `kill_now()`, hence nothing to reap the zombie — and the
///    red evidence would name the wrong half of the harness.
///
/// So every predicate here reads `State:` too and treats `Z (zombie)` as gone.
///
/// ## Composition
///
/// [`with_nvim_pid_helper`] prepends this fragment. It defines functions only, so it
/// is inert until called: shell options are dynamic rather than lexical, so the
/// caller's later `set -eu` still governs these bodies when they run. `$app_pid`,
/// `$FILE_TRIES` and `$WAIT_INTERVAL` are read at CALL time and come from whichever
/// scenario the fragment was prepended to.
///
/// ## Exit codes it owns
///
/// * `16` — the APP died. Its own code, never folded into anything else.
/// * `17` — the number of live `nvim` children of the app was not exactly one.
/// * `18` — the SIGKILLed Neovim never left the process table.
/// * `20` — a new Neovim child appeared during a quiet window that must stay empty
///   (smoke B only: the single automatic attempt must not have become a retry loop).
///
/// `wait_nvim_count` raises none of them: its CALLER names the code, because two
/// scenarios asking "are there two Neovims?" at two different moments are two
/// different failures and must not fold into one. A scenario that calls only that
/// function therefore cannot reach `16`, `17`, `18` or `20` from this fragment at
/// all, which is what lets `TWOWIN_SCENARIO` reuse `16` for the state-persistence
/// table's meaning ("run 1 wrote no `state.json`") without ambiguity.
const FIND_NVIM_PID_HELPER: &str = r#"
# How many WAIT_INTERVAL polls make up the "no respawn" quiet window. 20 * 0.2s =
# 4s, matching the interval the feature's manual sanity run used. A timing knob
# only -- no assertion depends on its value, but it must be comfortably longer than
# one deferred restart attempt (shell.rs's RESTART_DEFER is 50ms).
NVIM_QUIET_TRIES=20

# How many WAIT_INTERVAL polls `wait_nvim_respawn` allows for a RESPAWNED nvim
# child to appear. 25 * 0.2s = 5s; the measured restart (kill -> spawn) is under
# 200ms, so this is generous. A timing knob only -- it is non-fatal, so no
# assertion and no red-run exit code depends on its value.
NVIM_RESPAWN_TRIES=25

# True (exit 0) when $1 is a LIVE process: present in /proc and NOT a zombie.
#
# `kill -0` is deliberately NOT used: it succeeds on a zombie, which is exactly the
# state a SIGKILLed-but-unreaped child sits in.
pid_is_live() {
    [ -r "/proc/$1/status" ] || return 1
    live_state=$(sed -n 's/^State:[[:space:]]*//p' "/proc/$1/status" 2>/dev/null) || return 1
    case "$live_state" in
        Z*) return 1 ;;
    esac
    return 0
}

# Print the pids of the app's DIRECT children, whitespace-separated.
#
# Prefers /proc/<pid>/task/<pid>/children -- ONE read, no forks -- and falls back to
# a full /proc scan reading `PPid:` out of every status file when the kernel does
# not expose it (it needs CONFIG_PROC_CHILDREN, default since Linux 3.5).
#
# The fast path is not an optimisation for its own sake, it is a CORRECTNESS
# requirement of the crash smoke. MEASURED 2026-08-02 on the dev machine: one full
# /proc scan of the shape below costs ~2.6s of wall time (three `sed` forks per
# process, several hundred processes). `wait_nvim_respawn` runs between the restart
# and the post-recovery capture, so a scan there alone consumed most of the
# `liveness_ui::BANNER_VISIBLE_SECS` (4s) window and the banner had auto-dismissed
# before `grim` fired -- an assertion failing on the harness's own cost. It also
# turns smoke B's twenty-poll quiet window from ~55s into ~4s.
app_children() {
    kids="/proc/$app_pid/task/$app_pid/children"
    if [ -r "$kids" ]; then
        cat "$kids" 2>/dev/null
        return 0
    fi
    for d in /proc/[0-9]*; do
        [ -r "$d/status" ] || continue
        ppid=$(sed -n 's/^PPid:[[:space:]]*//p' "$d/status" 2>/dev/null) || continue
        if [ "$ppid" = "$app_pid" ]; then
            echo "${d#/proc/}"
        fi
    done
    return 0
}

# Find the LIVE processes named `nvim` among the app's direct children. Sets
# $nvim_pid (the match) and $count (how many there were). Returns 0 iff EXACTLY one
# matched, so it is safe to use directly as an `if` condition; callers that care
# about "none" test $count after `find_nvim_pid || true`.
find_nvim_pid() {
    nvim_pid=""
    count=0
    for p in $(app_children); do
        [ -r "/proc/$p/status" ] || continue
        pid_is_live "$p" || continue
        name=$(sed -n 's/^Name:[[:space:]]*//p' "/proc/$p/status" 2>/dev/null) || continue
        if [ "$name" = "nvim" ]; then
            nvim_pid=$p
            count=$((count + 1))
        fi
    done
    [ "$count" -eq 1 ]
}

# Poll until EXACTLY ONE live nvim child of the app exists, leaving its pid in
# $nvim_pid. Polled rather than scanned once because smoke B's WOLF_NVIM_BIN
# wrapper is a /bin/sh script: there is a brief window in which the app's child is
# named `sh`, before the `exec` replaces it in place with the real nvim.
wait_nvim_pid() {
    t=0
    while [ "$t" -lt "$FILE_TRIES" ]; do
        if find_nvim_pid; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome smoke: could not identify EXACTLY ONE live Neovim child of the app" >&2
    echo "  (pid $app_pid) within budget; the last scan counted ${count:-0}." >&2
    echo "  0 means the app never spawned nvim, or nvim had already died -- nothing to kill." >&2
    echo "  2+ means the scan cannot tell which process to kill, so nothing below would mean" >&2
    echo "  anything. The kill is NEVER aimed at a guess." >&2
    exit 17
}

# Poll until a NEW live nvim child of the app exists (leaving its pid in
# $nvim_pid), returning non-zero if none ever appears. NON-FATAL by design.
#
# Both crash smokes call this after the kill and BEFORE they start typing at the
# recovered editor, and it is load-bearing rather than a settle:
#
#   * MEASURED: typing an Ex command while the respawn is in flight delivers it
#     SPLIT. wolf-core's input pump reads a fresh RPC handle per batch, so the
#     batch carrying `:call wri` can go to the DEAD handle (and be lost) while the
#     batch carrying the tail reaches the FRESH one -- where, with no leading `:`,
#     it is interpreted as NORMAL-MODE keys. Observed effects included `a`
#     entering insert mode and inserting `me('%')], '/tmp/...')` into the recovered
#     buffer, and the current buffer being left on the recovered Untitled.md, which
#     fails the recovery poll (exit 19) for a reason that has nothing to do with
#     crash recovery.
#   * It is NOT fatal, so the red-run reverts that remove the restart entirely
#     still fail through the recovery poll below with the documented exit 19,
#     rather than acquiring a second, competing symptom here.
#
# It returns as soon as the process EXISTS, which is before its setup chain has
# finished, so callers still settle afterwards.
wait_nvim_respawn() {
    t=0
    while [ "$t" -lt "$NVIM_RESPAWN_TRIES" ]; do
        if find_nvim_pid; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome smoke: no Neovim child reappeared within the respawn budget." >&2
    echo "  Continuing anyway -- the recovery poll below is what decides the outcome." >&2
    return 1
}

# Poll until the pid in $nvim_pid is no longer a live process.
wait_nvim_gone() {
    t=0
    while [ "$t" -lt "$FILE_TRIES" ]; do
        if ! pid_is_live "$nvim_pid"; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome smoke: Neovim (pid $nvim_pid) survived SIGKILL, or was never reaped," >&2
    echo "  and is still a live process after the poll budget. Nothing downstream is about" >&2
    echo "  crash recovery until the process the smoke killed is actually gone." >&2
    exit 18
}

# The APP must still be running. Its own exit code, deliberately distinct from
# every other failure: an app that died alongside its Neovim is the WORST possible
# outcome of this feature and must never be confused with a slow restart, a missing
# readback or a failed OCR.
assert_app_alive() {
    if ! pid_is_live "$app_pid"; then
        echo "wolf-gnome smoke: THE APP (pid $app_pid) DIED WITH ITS NEOVIM." >&2
        echo "  This is the feature's worst failure mode: killing the captive Neovim must leave" >&2
        echo "  the GTK window up, showing the crash overlay. A dead app instead means the" >&2
        echo "  front-end panicked or aborted on the death wake -- look at the drain loop and at" >&2
        echo "  the restart path's RefCell borrows first." >&2
        exit 16
    fi
}

# Assert that NO nvim child of the app reappears for a full quiet window. Used by
# the failing-respawn smoke to prove the crash overlay really DWELLS: it trips if a
# respawn ever succeeds while the wrapper is armed.
#
# NOTE it does NOT, on its own, prove MAX_AUTO_RESTART_ATTEMPTS -- see
# assert_one_restart_attempt, which does.
assert_no_nvim_respawn() {
    t=0
    while [ "$t" -lt "$NVIM_QUIET_TRIES" ]; do
        find_nvim_pid || true
        if [ "$count" -ne 0 ]; then
            echo "wolf-gnome smoke: a NEW Neovim child (pid $nvim_pid) appeared during the" >&2
            echo "  quiet window, so the WOLF_NVIM_BIN wrapper did not refuse." >&2
            echo "  The crash overlay will not dwell, so the capture below would show a recovered" >&2
            echo "  window and nothing in this smoke would mean anything." >&2
            exit 20
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
}

# Assert the app made EXACTLY ONE automatic respawn attempt against the refusing
# WOLF_NVIM_BIN wrapper -- i.e. that MAX_AUTO_RESTART_ATTEMPTS held.
#
# $ATTEMPTS is appended to by the wrapper itself, one line per REFUSED spawn, so
# its line count is the number of automatic attempts the front-end made. Reading it
# is the only sound way to measure the cap:
#
#   MEASURED 2026-08-02. `assert_no_nvim_respawn` alone does NOT
#   discriminate the cap, and this was proven by running the revert: with
#   `MAX_AUTO_RESTART_ATTEMPTS = u32::MAX` the smoke stayed GREEN. The reason is
#   that a refused wrapper never becomes `nvim` at all -- it is a /bin/sh script
#   that exits 1 BEFORE its `exec`, so the app's child is named `sh` for a few
#   milliseconds and disappears. A scan that counts children named `nvim` therefore
#   sees an unbounded retry loop as zero respawns, and exit 20 was unreachable.
#   With the wrapper's own log the same revert measured 75 attempts in the 4s quiet
#   window, against the capped build's 1.
#
# Called AFTER the quiet window, so a retry loop has had the whole window to run.
#
# `wc -l` rather than `grep -c .`: on an EMPTY file `grep -c` prints 0 AND exits 1,
# so the `|| echo 0` fires too and $attempts becomes the two-line string "0\n0",
# which makes the `-ne` below an `integer expression expected` error and aborts the
# scenario under `set -eu` with a code outside the documented 16-20 band. Not
# reachable today (clear_evidence removes the file, and the wrapper only ever
# appends a non-empty line), but the trap costs nothing to remove. The two agree on
# every input this scenario can produce: the wrapper writes whole `echo` lines and
# never a blank one. When the file is MISSING -- the reachable "zero attempts" case
# -- the shell may print its own "cannot open" line before `|| echo 0` fires; that
# is noise on an already-failing path, and $attempts is still 0.
assert_one_restart_attempt() {
    attempts=$(wc -l < "$ATTEMPTS" 2>/dev/null || echo 0)
    if [ "$attempts" -ne 1 ]; then
        echo "wolf-gnome smoke: the app made $attempts automatic respawn attempt(s)" >&2
        echo "  against the refusing WOLF_NVIM_BIN wrapper; MAX_AUTO_RESTART_ATTEMPTS is 1, so" >&2
        echo "  exactly one is the contract." >&2
        echo "  MORE than one means the cap is gone and a permanently failing respawn now" >&2
        echo "  retries forever -- the exact hazard the cap exists to prevent." >&2
        echo "  ZERO means no automatic restart was attempted at all, so the crash overlay" >&2
        echo "  below is showing CRASHED_RETRYING_MESSAGE for a restart that never ran." >&2
        exit 20
    fi
}

# Poll until EXACTLY $1 live nvim children of the app exist. $2 is the exit code to
# raise when the budget elapses; $3 is a phrase naming the MOMENT, so two calls in
# one scenario stay distinguishable in a post-mortem.
#
# APPENDED rather than folded into `find_nvim_pid`, and that is deliberate:
# `find_nvim_pid` keeps its exactly-one contract, which `wait_nvim_pid` and
# `wait_nvim_respawn` -- and through them both crash smokes -- depend on. This
# function calls it as `find_nvim_pid || true` and then compares $count, exactly the
# shape `assert_no_nvim_respawn` uses, because a count of 0 or 2 makes
# `find_nvim_pid` return non-zero and `set -e` would abort the scenario before the
# comparison that is the whole point.
#
# It raises no exit code of its own. "Two Neovims after the palette opened a second
# window" and "two Neovims after a relaunch restored two saved windows" are
# different failures, and folding them into one code would destroy the evidence that
# says which half broke.
#
# It returns as soon as the COUNT is right, which is before the newest core has
# finished its setup chain, so callers still settle afterwards.
wait_nvim_count() {
    want=$1
    code=$2
    moment=$3
    t=0
    while [ "$t" -lt "$FILE_TRIES" ]; do
        find_nvim_pid || true
        if [ "$count" -eq "$want" ]; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome smoke: expected EXACTLY $want live Neovim child(ren) of the app" >&2
    echo "  (pid $app_pid) $moment; the last scan counted ${count:-0}." >&2
    echo "  FEWER means a window that should exist has no core of its own: it was never" >&2
    echo "  built, or WolfCore::start failed and window::build answered None (which is" >&2
    echo "  logged and does NOT abort the others). One core per window is the whole" >&2
    echo "  architecture, so a short count is the claim itself failing, not a symptom." >&2
    echo "  MORE means a window was opened more than once, or a closed window's core was" >&2
    echo "  never shut down." >&2
    if ! pid_is_live "$app_pid"; then
        echo "  READ THIS FIRST: THE APP ITSELF IS GONE (pid $app_pid is not a live process)," >&2
        echo "  so the count is 0 for a reason that has nothing to do with windows -- the" >&2
        echo "  process died, and every window count after that is 0 whatever the feature did." >&2
    fi
    exit "$code"
}
"#;

/// Prepends [`FIND_NVIM_PID_HELPER`] to a keyboard-only scenario ([`CRASH_SCENARIO`],
/// [`STALL_SCENARIO`], `TERMRESTART_SCENARIO`, `TWOWIN_SCENARIO`) or to a
/// pointer body ([`POINTER_RESTART_BUTTON_BODY`]).
/// `Command::arg` and [`run_pointer_scenario`] both accept the result.
///
/// The last of those calls only `find_nvim_pid` (through `wait_nvim_count`) and
/// `pid_is_live`, never a function that exits, which is what keeps the fragment's
/// exit codes out of its table.
pub(crate) fn with_nvim_pid_helper(scenario: &str) -> String {
    format!("{FIND_NVIM_PID_HELPER}\n{scenario}\n")
}

/// The CRASH / auto-restart scenario for
/// [`nvim_crash_recovers_and_reopens_the_file_under_cage`], run as a single `sh -c`
/// string INSIDE `cage`. Keyboard only — no pointer helper.
///
/// Composed with [`with_nvim_pid_helper`], which supplies `wait_nvim_pid`,
/// `wait_nvim_gone`, `assert_app_alive` and their exit codes.
///
/// ## The shape, and the one step that is deliberately hostile
///
/// Step 6 fires a burst of input and kills the Neovim child with NO settle in
/// between. That is not carelessness and must not be "fixed" by inserting a sleep:
/// it is the adversarial half of the wake-coalescing argument. The GTK wake
/// bridge is an `async_channel::bounded(1)`, so a wake fired while the channel is
/// full is DROPPED — and `mark_dead` fires exactly once, after which a dead Neovim
/// produces no further redraw wakes ever. Bursting first maximises the chance the
/// death wake lands in a full channel. If this smoke ever flakes, the diagnosis is a
/// wake-TRANSPORT bug (see `app.rs`'s `a_dropped_wake_implies_a_pending_wake`), NOT
/// a timing knob that needs raising.
///
/// ## Why step 5 exists: making swap recovery DETERMINISTIC rather than a race
///
/// Step 5 types the unsaved edit, then runs `:preserve`, then polls a readback that
/// proves `:preserve` ran, and only then does step 6 burst-and-kill. That ordering is
/// load-bearing in both directions:
///
///  * WITHOUT `:preserve`, "the edit survived" would be a race. `specs/architecture.md:22`
///    records that a SIGKILL skips `preserve_exit`, leaving "a bounded loss window
///    since the last swap sync", and `runtime/nvim/init.lua` deliberately keeps
///    Neovim's default `'updatecount'` — 200 TYPED CHARACTERS between automatic swap
///    syncs. A 13-character edit is nowhere near that, so the swap would usually NOT
///    hold it at kill time and the assertion would be genuinely flaky.
///    `:preserve` writes the buffer to its swap file immediately, which turns "the
///    edit survived" into a property rather than a coin flip.
///  * WITHOUT the burst being MOVED to step 6, the poll in step 5 would insert a
///    settle (`wait_file` sleeps `WAIT_INTERVAL` between tries) between the last
///    keystroke and the kill, the GTK app would have drained its wake channel, and
///    the adversarial property above would be silently traded away. So the burst
///    moved AFTER the observable, and became a CMDLINE burst so it cannot perturb the
///    buffer whose recovery is being asserted. See [`RESTART_BURST_TEXT`].
///
/// The observable itself is an ordering argument, not a guess: Neovim executes the Ex
/// commands it is sent in the order it receives them, so a `$PRESERVED` file written
/// by a LATER `:call writefile(...)` can only exist once the EARLIER `:preserve` has
/// returned. It carries `&modified` and `!empty(swapname())` so the Rust side can also
/// assert the two preconditions that make the flush meaningful at all: the buffer
/// really is dirty, and it really has a swap file to be flushed into.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `14` — a readback file the captive Neovim was told to write never appeared.
/// * `16` — THE APP DIED with its Neovim. See [`FIND_NVIM_PID_HELPER`].
/// * `17` — the Neovim child could not be identified uniquely.
/// * `18` — the SIGKILLed Neovim never left the process table.
/// * `19` — recovery timed out: `bufname('%')` never named the fixture again. This
///   is the headline red-run symptom for four of this smoke's five reverts.
const CRASH_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for a readback file, and for the whole
# crash -> auto-restart -> reopen round trip. The restart runs twelve setup RPCs
# against a freshly spawned Neovim, so it is slower than a redraw.
FILE_TRIES=100

# Teardown, installed BEFORE the app is started so every `exit NN` below runs it.
# Unlike the other keyboard-only scenarios this one has failure exits that happen
# while the app is still UP (16-19), and an orphaned app would otherwise outlive the
# scenario until cage tore its socket down.
cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome crash smoke"

# Poll for a readback file the captive Neovim writes. Nothing downstream may read a
# file we merely hope is there.
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
    echo "wolf-gnome crash smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 2: open the fixture and record the BEFORE state ------------------
#
# The BEFORE state is PROVED rather than assumed: bufname('%') pins that the
# fixture really is the active buffer, and winwidth(0)/winheight(0) are the grid
# size the pane-view reset has to preserve across the restart. All three ride in one
# writefile so they describe one instant.
wtype -- ":e $RESTART_FILE"
wtype -k Return
sleep "$SETTLE"

wtype -- ":call writefile([printf('%s %d %d', bufname('%'), winwidth(0), winheight(0))], '$BEFORE')"
wtype -k Return
wait_file "$BEFORE" || exit 14

# --- Step 3: the POSITIVE CONTROL capture ----------------------------------
#
# Taken while everything is still healthy, so a later OCR miss is diagnosable: if
# this one misses too, the problem is rendering or OCR, not crash recovery.
grim "$OUT_PNG_BEFORE"

# --- Step 4: identify the Neovim child, exactly one ------------------------
wait_nvim_pid

# --- Step 5: the UNSAVED EDIT, flushed to the swap file --------------------
#
# `ggj` puts the cursor on buffer line 2 no matter where it was, then `A` APPENDS
# at the end of that line. BOTH halves are load-bearing -- see
# RESTART_UNSAVED_MARKER:
#
#   * line 2 rather than line 1, because the "Neovim was restarted" banner is
#     drawn at the TOP CENTRE of the editor area and therefore lies across screen
#     ROW 0. MEASURED: with the marker appended to line 1, that row OCR'd as
#     `% bounceback unsavergh r tures` while the banner was up -- the banner and
#     the marker cannot both be read from the same capture, and the smoke asserts
#     on both.
#   * APPEND rather than insert at column 1, because after the restart the block
#     cursor sits on line 1 column 1 and corrupts exactly that cell for OCR, so
#     column 1 has to stay the heading's `#`, which nothing asserts on.
wtype -- "ggj"
wtype -- "A$UNSAVED_MARKER"
wtype -k Escape

# :preserve writes the buffer to its swap file NOW. Neovim's default 'updatecount'
# is 200 TYPED CHARACTERS between automatic syncs and this edit is 13, so without
# this the SIGKILL below would usually beat the swap and "the edit survived" would
# be a coin flip rather than a property. See this const's doc comment.
wtype -- ":preserve"
wtype -k Return

# The observable that proves :preserve RAN, and two preconditions that make it
# meaningful: the buffer is dirty, and it has a swap file to be flushed into.
# Neovim runs Ex commands in the order it receives them, so this file can only
# exist once the :preserve above has returned.
#
# `swapname()` takes a MANDATORY {buf} argument (`:help swapname()`); a bare
# `swapname()` raises E119 -> E116 and writes nothing, which surfaces here as exit
# 14 rather than as a failed assertion. `'%'` is the current buffer, which is the
# fixture -- the same buffer `&modified` and `:preserve` above refer to.
wtype -- ":call writefile([printf('%d %d', &modified, !empty(swapname('%')))], '$PRESERVED')"
wtype -k Return
wait_file "$PRESERVED" || exit 14

# --- Step 6: BURST, then kill IMMEDIATELY ----------------------------------
#
# NO settle between the burst and the kill, and nothing between the step-5
# observable and this step either. See this const's doc comment: this is the
# adversarial half of the wake-coalescing argument, and the absence of a sleep here
# is the point of the step. The burst goes into the COMMAND LINE and is never
# executed -- it exists only to have redraw wakes in flight when Neovim dies, and
# must not touch the buffer whose swap recovery is being asserted.
wtype -- ":$BURST"
# `|| true` only because `set -e` would abort on a kill aimed at a process that has
# ALREADY gone -- which is not a failure of anything. `wait_nvim_gone` below is what
# decides whether the process is really dead, and it exits 18 if it is not.
kill -9 "$nvim_pid" 2>/dev/null || true

# --- Step 7: the Neovim is gone; the APP is NOT ----------------------------
wait_nvim_gone
assert_app_alive

# --- Step 8: poll for recovery ---------------------------------------------
#
# Wait for the RESPAWNED Neovim child before typing at it, then settle. Non-fatal:
# if no respawn ever happens the poll below still runs and still exits 19, which is
# the documented red-run symptom. See `wait_nvim_respawn` for why typing INTO a
# mid-respawn Neovim delivers the command split across the handle swap.
wait_nvim_respawn || true
sleep "$SETTLE"

# Each iteration re-asks Neovim for its own current buffer name. Keys typed while
# nvim is dead are harmlessly dropped, and the leading Escape clears any partial
# cmdline left over from a dropped one -- including the step-6 burst, which the
# input pump replays into the FRESH Neovim's command line. $AFTER is removed each
# time so a stale answer (e.g. the fresh nvim's Untitled.md, written before the
# reopen landed) cannot be mistaken for this iteration's.
recovered=0
t=0
while [ "$t" -lt "$FILE_TRIES" ]; do
    rm -f "$AFTER"
    wtype -k Escape
    wtype -- ":call writefile([bufname('%')], '$AFTER')"
    wtype -k Return
    sleep "$WAIT_INTERVAL"
    if [ -s "$AFTER" ] && grep -qF "$RESTART_FILE" "$AFTER"; then
        recovered=1
        break
    fi
    t=$((t + 1))
done
if [ "$recovered" -ne 1 ]; then
    assert_app_alive
    echo "wolf-gnome crash smoke: Neovim never came back with $RESTART_FILE as its current" >&2
    echo "  buffer within the poll budget. The app is still alive, so this is a RECOVERY" >&2
    echo "  failure, not a crash-handling one: either no restart was attempted (is the" >&2
    echo "  liveness read still first in the drain loop?), or the restart happened and the" >&2
    echo "  Tier-1 reopen did not, or it reopened the files in the wrong ORDER so the active" >&2
    echo "  buffer is the recovered Untitled.md instead of the fixture." >&2
    echo "  Last readback was:" >&2
    cat "$AFTER" >&2 2>/dev/null || echo "  <no $AFTER at all>" >&2
    exit 19
fi

# --- Step 9: the AFTER grid size -------------------------------------------
#
# One settle first: the pane resize is enqueued BEHIND the reopen on the same
# ordered input channel, so reading immediately would race it.
sleep "$SETTLE"
wtype -- ":call writefile([printf('%d %d', winwidth(0), winheight(0))], '$DIMS_AFTER')"
wtype -k Return
wait_file "$DIMS_AFTER" || exit 14

# --- Step 10: capture IMMEDIATELY ------------------------------------------
#
# The banner auto-dismisses after liveness_ui::BANNER_VISIBLE_SECS, so the capture
# follows the step-9 observable with nothing in between. If the banner assertion
# ever flakes, SHORTEN this gap -- do NOT raise BANNER_VISIBLE_SECS, which is a
# product constant a user sees.
grim "$OUT_PNG"

# The trap installed above tears the app down on the way out.
exit 0
"#;

/// The STALL scenario for [`stalled_nvim_raises_the_stall_overlay_under_cage`], run
/// as a single `sh -c` string INSIDE `cage`. Keyboard only — no pointer helper.
///
/// Composed with [`with_nvim_pid_helper`], which supplies `wait_nvim_pid` and
/// `assert_app_alive` and their exit codes. It uses nothing else from that fragment
/// and never expands `$ATTEMPTS`, so `set -u` is satisfied — the helper defines
/// functions only, and an uncalled function body is never evaluated.
///
/// ## The lever
///
/// `kill -STOP` on the captive Neovim, then Ctrl+Shift+E. The buffer picker's
/// candidate snapshot is issued on the GTK main thread through `block_on_rpc`; the
/// stopped peer cannot answer; the budget ([`STALL_RPC_TIMEOUT_MS`]) elapses;
/// `block_on_rpc`'s `Err(Timeout)` arm marks the core `NotResponding` and fires the
/// wake; `liveness_ui(NotResponding, _)` yields `OverlayMode::Stalled`, and
/// `apply_overlay` raises the opaque overlay and closes the empty picker behind it.
///
/// This is only reachable because the buffer picker exists: of the blocking RPCs a
/// RUNNING `wolf-gnome` issues, its candidate snapshot is the one a chord this
/// harness can type reaches with no selection or history to set up first.
///
/// ## Capture AFTER the block, never during it
///
/// No frame is committed while the main loop is blocked, so a `grim` fired during
/// the block would return the PRE-CHORD frame and prove nothing — it would look
/// exactly like a stall overlay that never appeared, and exactly like one that
/// appeared and was correct. `STALL_WAIT` is therefore longer than the timeout
/// budget plus a settle, and the capture follows it. Nothing blocks the main loop
/// after `list_buffers` returns (`apply_overlay` only toggles visibility), so the
/// frame is presented through the same path the crash-overlay smoke already proves.
///
/// ## `SIGCONT` is teardown hygiene, not recovery
///
/// The resumed Neovim completes the detached task, but `block_on_rpc` already returned
/// `Err(Timeout)` and its `Ok` clear never runs, and no typed batch waits on Neovim for the
/// pump to clear it with (the chord is an accelerator; the overlay's key guard sends nothing),
/// so the overlay stays up. The capture precedes the `SIGCONT`, so no assertion rests on it.
///
/// ## Exit codes owned
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `16` — THE APP DIED with its wedged Neovim. The worst failure of this feature,
///   and its own code so it can never be confused with a missing overlay. See
///   [`FIND_NVIM_PID_HELPER`].
/// * `17` — the Neovim child could not be identified uniquely, so nothing was
///   stopped and the chord below would have been fired at a healthy editor.
const STALL_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls `wait_nvim_pid` may spend identifying the child.
FILE_TRIES=100
#
# STALL_WAIT -- how long to wait after the chord before capturing. It must exceed
# the app's WOLF_RPC_TIMEOUT_MS budget (0.8s) plus a redraw settle, because the
# frame the assertions read is the one presented AFTER block_on_rpc gives up:
# nothing is committed while the main loop is blocked. A timing knob only, but a
# value below the timeout captures the pre-chord frame and makes both post-stall
# assertions meaningless rather than red.
STALL_WAIT=2.0

# Teardown, installed BEFORE the app is started so every `exit NN` below runs it.
# This scenario has failure exits that happen while the app is UP (16, 17) and
# while its Neovim is STOPPED, and an orphaned pair would otherwise outlive the
# scenario. The CONT comes first: a stopped process cannot act on the app's
# shutdown, and reaping it matters more than the order looks.
#
# Explicit `if` blocks rather than `[ -n ... ] && ...`: under `set -e` a compound
# whose final command returns non-zero aborts the trap, which would leave the app
# running.
cleanup() {
    set +e
    if [ -n "${nvim_pid:-}" ]; then
        kill -CONT "$nvim_pid" 2>/dev/null
    fi
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome stall smoke"

# --- Step 3: open the fixture, so the editor has something distinctive up ---
wtype -- ":e $STALL_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 4: the HEALTHY CONTROL capture ------------------------------------
#
# Taken while everything still works. It carries both halves of the diagnosis: the
# fixture heading MUST be readable here (otherwise its absence after the stall
# proves nothing about opacity), and the overlay's own term must be ABSENT here
# (otherwise the post-stall hit could have come from something at launch).
grim "$OUT_PNG_BEFORE"

# --- Step 5: identify the Neovim child, exactly one -------------------------
#
# Exit 17 if not exactly one. The STOP below is never aimed at a guess: stopping
# the wrong process would leave a healthy Neovim answering the RPC, and the smoke
# would go red with no indication that it had wedged nothing at all.
wait_nvim_pid

# --- Step 6: WEDGE it -------------------------------------------------------
#
# SIGSTOP, not SIGKILL: a killed Neovim is `Dead` and raises the CRASH overlay,
# which is a different state with a different message and is already smoked
# elsewhere. A stopped one is alive, holds its end of the RPC socket open, and
# simply never answers -- which is the only thing that produces `NotResponding`.
kill -STOP "$nvim_pid" 2>/dev/null || true

# --- Step 7: fire the chord that issues the blocking RPC --------------------
#
# Ctrl+Shift+E -> win.buffer-picker -> UiAction::OpenBufferPicker -> the candidate
# snapshot, which is `WolfCore::list_buffers` through `block_on_rpc` on the GTK
# main thread. Against the stopped peer it cannot return, so the timeout budget
# elapses and the core is marked NotResponding.
wtype -M ctrl -M shift -k e -m shift -m ctrl

# --- Step 8: wait out the block ---------------------------------------------
#
# Longer than the RPC budget plus a settle. The capture below is the frame
# presented AFTER block_on_rpc gives up; there is no frame to capture before that.
sleep "$STALL_WAIT"

# --- Step 9: capture the stalled window -------------------------------------
grim "$OUT_PNG"

# --- Step 10: the APP must still be running ---------------------------------
#
# Exit 16, its own code. An app that blocked forever, or that panicked on the
# liveness wake, is the worst outcome of this feature and must never be confused
# with an overlay that merely failed to render.
assert_app_alive

# --- Step 11: let Neovim run again ------------------------------------------
#
# Teardown hygiene only, and the capture is already taken. The resumed Neovim
# completes the detached task, but block_on_rpc has already returned Err(Timeout)
# and no typed batch waits on Neovim, so the core stays NotResponding -- evidence
# that a stall a blocking call marks does not self-clear while no typing is pending.
kill -CONT "$nvim_pid" 2>/dev/null || true

# The trap installed above tears the app down on the way out.
exit 0
"#;

/// Body of [`crash_overlay_and_restart_button_recover_under_cage`], appended to
/// [`POINTER_PREAMBLE`] and composed with [`with_nvim_pid_helper`].
///
/// ## Why this smoke has to make the respawn FAIL
///
/// On the SUCCESS path the crash overlay is a sub-frame transient — shown, then
/// cleared by the fresh Neovim's `Alive` wake — and this smoke wedges no Neovim, so
/// neither the buffer picker's `block_on_rpc` nor core's input pump times out and
/// marks `NotResponding`. The one state in which the overlay genuinely
/// DWELLS long enough for `grim` to see it is a FAILED respawn, which is also a real
/// production scenario: Neovim removed or upgraded mid-session, an `exec` failure,
/// an OOM kill.
///
/// It is induced with no test-only production branch: `NvimConfig::from_env` reads
/// `WOLF_NVIM_BIN` once, at `WolfCore::start`, and `restart_nvim` reuses that same
/// config verbatim, so a wrapper at that path is re-executed on every respawn. The
/// Rust side writes the wrapper with both absolute paths baked in as literals and
/// sets `WOLF_NVIM_BIN` on the `cage` `Command` ONLY.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// * `16` / `17` / `18` — see [`FIND_NVIM_PID_HELPER`].
/// * `19` — the button click drove no recovery: `bufname('%')` never named the
///   fixture again.
/// * `20` — either a new Neovim child appeared during the quiet window (the wrapper
///   did not refuse, so the overlay would not dwell) or the wrapper's own spawn log
///   did not hold exactly one refused attempt (the attempt cap did not hold, or no
///   automatic restart ran at all).
const POINTER_RESTART_BUTTON_BODY: &str = r#"
# --- Step 1: open the fixture and record the BEFORE buffer -----------------
wtype -- ":e $RESTART_FILE"
wtype -k Return
sleep "$SETTLE"

wtype -- ":call writefile([bufname('%')], '$BEFORE')"
wtype -k Return
wait_file "$BEFORE" || exit 14

# Ask the captive Neovim where its own cursor is, into the file named by $1.
# Reused verbatim from POINTER_CLICK_BODY: its readback is the input to the shared
# `assert_probe_cell_and_pane_origin` band, which is this smoke's CSD / geometry
# tripwire and the measured origin its button point is derived against.
read_cursor() {
    wtype -- ":call writefile([printf('%d %d', line('.'), col('.'))], '$1')"
    wtype -k Return
    wait_file "$1" || exit 14
}

# --- Step 2: the PROBE click -----------------------------------------------
hsend motion "$PROBE_X" "$PROBE_Y"
hsend press "$BTN_LEFT"
hsend release "$BTN_LEFT"
sleep "$SETTLE"
read_cursor "$POS0"

# --- Step 3: arm the refusing wrapper, then kill the Neovim child ----------
#
# The marker is created BEFORE the kill, so the app's single automatic restart
# attempt runs against a wrapper that already refuses.
touch "$FAIL_MARKER"
wait_nvim_pid
# `|| true` only because `set -e` would abort on a kill aimed at a process that has
# ALREADY gone. `wait_nvim_gone` decides whether it is really dead (exit 18).
kill -9 "$nvim_pid" 2>/dev/null || true
wait_nvim_gone
assert_app_alive

# --- Step 4: the QUIET WINDOW ----------------------------------------------
#
# No new nvim child may appear, which is what makes the capture below meaningful --
# a successful respawn here would clear the overlay. Then read the wrapper's OWN
# spawn log: the automatic attempt must have run, failed, and STOPPED at exactly
# one. The scan cannot see a refused spawn (it never becomes `nvim`), so the log is
# what actually proves MAX_AUTO_RESTART_ATTEMPTS -- see assert_one_restart_attempt.
assert_no_nvim_respawn
assert_one_restart_attempt
assert_app_alive

# --- Step 5: the DWELL capture ---------------------------------------------
grim "$OUT_PNG_OVERLAY"

# --- Step 6: disarm the wrapper --------------------------------------------
#
# From here on a respawn is allowed to succeed, so the button has something to do.
rm -f "$FAIL_MARKER"

# --- Step 7: click the RESTART BUTTON --------------------------------------
#
# BTN_X / BTN_Y are derived on the Rust side from `wolf_gnome::restart_button_rect`
# -- the very function the widget builder's size requests realise -- composed with
# the measured pane origin. No magic pixel appears anywhere in this binary.
hsend motion "$BTN_X" "$BTN_Y"
hsend press "$BTN_LEFT"
hsend release "$BTN_LEFT"

# --- Step 8: poll for recovery ---------------------------------------------
#
# Identical in shape to the keyboard crash smoke's step 8, and for the same reasons:
# wait for the respawned child (non-fatal) so the Ex command below is not delivered
# split across wolf-core's handle swap, then poll. Keys typed at a dead Neovim are
# dropped, the leading Escape clears any partial cmdline, and $AFTER is removed each
# iteration so a stale answer cannot pass.
wait_nvim_respawn || true
sleep "$SETTLE"

recovered=0
t=0
while [ "$t" -lt "$FILE_TRIES" ]; do
    rm -f "$AFTER"
    wtype -k Escape
    wtype -- ":call writefile([bufname('%')], '$AFTER')"
    wtype -k Return
    sleep "$WAIT_INTERVAL"
    if [ -s "$AFTER" ] && grep -qF "$RESTART_FILE" "$AFTER"; then
        recovered=1
        break
    fi
    t=$((t + 1))
done
if [ "$recovered" -ne 1 ]; then
    assert_app_alive
    echo "wolf-gnome crash-button smoke: the click at ($BTN_X, $BTN_Y) drove no recovery." >&2
    echo "  The app is alive and the fail marker was removed, so a respawn was possible." >&2
    echo "  Check, in order: (1) the button's clicked handler is still connected; (2) the" >&2
    echo "  derived point still lands ON the button -- the Rust side re-derives the rect from" >&2
    echo "  this run's own measured extents and origin and reports it; (3) the restart path" >&2
    echo "  reopened the Tier-1 files, with the ACTIVE path last." >&2
    echo "  Last readback was:" >&2
    cat "$AFTER" >&2 2>/dev/null || echo "  <no $AFTER at all>" >&2
    exit 19
fi

# --- Step 9: park the pointer off the editor, then capture -----------------
hsend motion 1 1
sleep "$SETTLE"

grim "$OUT_PNG"
hsend quit
exit 0
"#;

/// Asserts the restart button's rect is the NAMED size, before anything derives a
/// click point from it.
///
/// `wolf_gnome::restart_button_rect` collapses its WHOLE result to `(0, 0, 0, 0)`
/// for a degenerate or too-small allocation, mirroring `grid_size_for` / `cell_at`.
/// Without this check that collapse would silently retarget the click to the
/// overlay's TOP-LEFT CORNER — a plausible-looking pixel, inside the output, that
/// hits nothing — and the smoke would fail somewhere far away from the cause.
#[allow(
    clippy::float_cmp,
    reason = "both expected values are small named i32 constants widened with f64::from and \
              returned by `restart_button_rect` unmodified, so they are exactly representable; \
              an epsilon comparison would weaken the collapse guard without making it more \
              correct, and the collapse it exists to catch is a literal 0.0"
)]
fn assert_button_rect_is_the_named_size(smoke: &str, rect: (f64, f64, f64, f64)) {
    let (bx, by, bw, bh) = rect;
    let expected_w = f64::from(wolf_gnome::liveness_ui::RESTART_BUTTON_WIDTH);
    let expected_h = f64::from(wolf_gnome::liveness_ui::RESTART_BUTTON_HEIGHT);
    assert!(
        bw == expected_w && bh == expected_h,
        "{smoke}: `wolf_gnome::restart_button_rect` returned a button of {bw}x{bh}, expected \
         exactly {expected_w}x{expected_h} (RESTART_BUTTON_WIDTH x RESTART_BUTTON_HEIGHT). A \
         `0x0` here is the documented WHOLE-RESULT COLLAPSE: the overlay allocation handed in was \
         non-finite, non-positive, narrower than the button, or shorter than the fixed \
         message+gap+button block. The rect was ({bx}, {by}, {bw}, {bh}). Deriving a click point \
         from a collapsed rect would aim at the overlay's top-left corner and hit nothing, so \
         this fails here instead."
    );
}

/// Asserts the derived button rect sits comfortably inside the overlay allocation,
/// with at least `OVERLAY_SPACING` px of margin on all four sides.
///
/// This is the crash-button smoke's own analogue of `PANE_ORIGIN_TOLERANCE_PX`: a
/// self-checking precondition whose failure names the candidate causes in order,
/// rather than letting a geometry change quietly move the click somewhere harmless.
fn assert_button_rect_fits(smoke: &str, rect: (f64, f64, f64, f64), overlay: (f64, f64)) {
    let (bx, by, bw, bh) = rect;
    let (overlay_w, overlay_h) = overlay;
    let margin = f64::from(wolf_gnome::liveness_ui::OVERLAY_SPACING);
    assert!(
        bx >= margin
            && by >= margin
            && bx + bw + margin <= overlay_w
            && by + bh + margin <= overlay_h,
        "{smoke}: the derived restart-button rect ({bx}, {by}, {bw}, {bh}) does not sit at least \
         {margin} px clear of every edge of the {overlay_w}x{overlay_h} overlay allocation. The \
         five candidate causes, in order of likelihood: (1) THE CALLER SUBTRACTED THE WRONG \
         ORIGIN — this allocation is `output - OVERLAY_ORIGIN_*`, the pane AREA's corner, and \
         `PANE_ORIGIN_*` is the TEXT GRID's; the two look interchangeable and differ by the \
         per-pane chrome, so an allocation roughly a statusbar strip and a padding too small is \
         that mistake; (2) an `adw::HeaderBar` height change (a \
         libadwaita/GTK bump), which moves the overlay's top edge and therefore its height; (3) \
         a `window::TREE_PANE_WIDTH` or `gtk::Paned` divider-width change, which moves its left \
         edge; (4) a cage output-size change, which moves the bottom-right corner; (5) \
         `crash_overlay.rs` having been rewritten to use an `adw::StatusPage`, or its root \
         `gtk::Box` having been given a NON-ZERO spacing — either of which breaks the derivation \
         outright, because `restart_button_rect` models exactly two `vexpand` spacers around a \
         fixed message+gap+button block with the gap living in the label's `margin_bottom`. Fix \
         the geometry or re-measure the constants; do NOT shrink the margin to make this pass."
    );
}

/// Killing the captive Neovim leaves the app alive, restarts Neovim in place,
/// reopens the Tier-1 file set with the ACTIVE path last, RECOVERS the unsaved edit
/// from the swap file, flashes the "Neovim was restarted" banner, and hands the fresh
/// Neovim a grid of the SAME size.
///
/// ## What it proves
///
/// The whole crash-recovery chain, end to end and against a real SIGKILL:
/// `mark_dead` fires a wake; `shell::on_liveness_wake` reads `liveness()` at the TOP
/// of the drain loop and classifies it; the deferred automatic restart runs;
/// `pane_view::reset_after_restart` destroys the stale `DrawingArea` so a fresh
/// `::resize` re-establishes the grid size; the captured Tier-1 files are reopened
/// with the active one LAST so it ends up focused, into the SAME durable
/// `WOLF_SWAP_DIR` so the dead owner's swap is recovered rather than bypassed; and
/// the transient banner is flashed on the success path.
///
/// ## The burst before the kill is DELIBERATE and load-bearing
///
/// The scenario fires a burst of input and SIGKILLs Neovim with no settle in
/// between. That is the adversarial half of the wake-coalescing argument crash recovery
/// rests on: `wolf-gnome`'s wake bridge is an `async_channel::bounded(1)` whose
/// `try_send` DROPS a wake fired into a full channel, `mark_dead` fires exactly once,
/// and a dead Neovim produces no further redraw wakes — so if coalescing were unsafe,
/// the overlay and the restart would simply never happen and this smoke would flake.
/// **If it ever does flake, the diagnosis is a wake-TRANSPORT bug, not a settle that
/// needs raising.** The headless counterparts are `app.rs`'s
/// `a_dropped_wake_implies_a_pending_wake` and
/// `consuming_a_wake_frees_the_slot_before_the_handler_runs`.
///
/// The burst goes into the COMMAND LINE rather than the buffer, and the unsaved edit
/// the recovery assertion rests on is typed and `:preserve`d EARLIER, behind its own
/// poll. Both halves are deliberate and neither may be collapsed into the other: see
/// [`CRASH_SCENARIO`]'s "Why step 5 exists" for why a buffer burst immediately before
/// the kill would make swap recovery a race against `'updatecount'`, and why moving
/// the `:preserve` poll to just before the kill would silently trade the
/// wake-coalescing property away.
///
/// ## The unsaved edit is what proves the buffer was RECOVERED, not re-read
///
/// `bounceback` is on disk in the fixture, so it would render whether the reopened
/// `bfile.md` came from the dead Neovim's swap file or from a clean re-read.
/// `unsavedburst` was never written to disk at all: it exists only in the crashed
/// Neovim's swap. Asserting it PRESENT after the restart and ABSENT in the pre-crash
/// control is what discriminates those two worlds — the front-end reopening into the
/// same durable `WOLF_SWAP_DIR` (so `runtime/nvim/init.lua`'s `SwapExists` autocmd
/// sees a DEAD owner and answers `v:swapchoice = "r"`) versus reopening into a fresh
/// one, which would lose the user's unsaved work silently.
///
/// ## Why the reopen set exercises two rules at once
///
/// At the moment of the crash the tabpage's Tier-1 files are `[Untitled.md,
/// bfile.md]` with `active = bfile.md`, so one pass covers the `Untitled.md`
/// swap-recovery reopen path AND the "active path last" ordering rule.
///
/// What discriminates the ORDER is the SCENARIO'S OWN RECOVERY POLL, not an OCR
/// term. MEASURED by running the revert: with `reopen_order` pushing
/// `active` FIRST, the focused buffer stays the recovered `Untitled.md`, the poll —
/// which waits for `bufname('%')` to name the fixture — never satisfies, and the run
/// ends at scenario exit **19** BEFORE any OCR assertion is reached. `bounceback`
/// would indeed be absent from that frame, but no measured run has ever got far
/// enough to look: it is corroboration, not the discriminator. See row 3 of the
/// revert table below, which records the same finding.
///
/// ## What it must NOT duplicate
///
/// `crates/wolf-core/tests/nvim_liveness.rs::crash_marks_dead`,
/// `nvim_restart.rs` and
/// `swap_recovery.rs::restart_recovers_unsaved_untitled_buffer` already cover the
/// CORE side headlessly and deterministically, including the exact reopen order and
/// the silent `SwapExists` recovery. What none of them can say is whether the GTK
/// front-end ever reads `liveness()`, whether it restarts, whether the pane view
/// survives it, and whether the recovered text ever reaches a PIXEL — which is the
/// only thing this smoke is for. The `unsavedburst` assertion below deliberately
/// overlaps `swap_recovery.rs` on the mechanism while adding the one thing a headless
/// core test structurally cannot have: the GTK front-end in the loop, driving the
/// reopen itself.
///
/// ## Red-run reverts (FIVE reverts, six rows — run EVERY revert)
///
/// The sixth row is listed honestly: it has no isolated revert of its own, and saying
/// so is the point — an assertion that cannot be shown red on its own must not be
/// presented as though it discriminates one of the five.
///
/// | assertion | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | recovery happened at all | delete `let paint = shell.on_liveness_wake();` from `app.rs`'s drain loop and call `panes`/`tree` unconditionally | scenario exit **19** | reproduced (`the cage crash scenario exited unsuccessfully (exit status: 19)`; scenario stderr `Neovim never came back with bfile.md as its current buffer within the poll budget`), restored, green |
/// | the auto-restart | in `liveness_ui`, change the `(Dead, true)` row to `auto_restart: false` | scenario exit **19**, identical | reproduced, identical (`exit status: 19`, same stderr), restored, green |
/// | the reopen ORDER | in `reopen_order`, push `active` FIRST instead of last | recovery and banner both happen, but `bounceback` is ABSENT while `restarted` is present | red, but NOT by the predicted route: measured `exit status: 19`. The scenario's own recovery poll is order-sensitive (it waits for `bufname('%')` to name the fixture, and with the order reversed the focused buffer stays the recovered `Untitled.md`), so the run never reaches the OCR assertions. The `bounceback` term is therefore NOT what discriminates the order — the poll is. Restored, green |
/// | the banner | make `CrashOverlay::flash_banner` a no-op | `restarted` absent, `bounceback` present | reproduced exactly (`after the restart, OCR did not contain "restarted"`, while the same OCR dump carries `% bounceback` and `... unsavedburst`), restored, green |
/// | the pane-view reset | delete `self.inner.panes.reset_after_restart();` from `shell.rs`'s restart path | dims go `45 14` -> `80 24` (the config's initial cols/rows), because no `::resize` fires on an unchanged allocation. **This is the only assertion that discriminates the reset** — all three OCR terms survive it | reproduced exactly (`the pane grid changed across the restart — it was 45x14 before the crash and is 80x24 after`; all three OCR assertions passed first, as predicted), restored, green |
/// | the unsaved edit survived (`unsavedburst` present after, ABSENT in the control) | **no isolated revert of its own.** Reverting the restart (rows 1-2) makes it red along with everything else via scenario exit **19**; reverting the reopen ORDER (row 3) makes it absent alongside `bounceback`. It discriminates none of the five on its own | what it independently discriminates is that the reopened buffer is the SWAP-RECOVERED one rather than a clean re-read from disk: `bounceback` is on disk and renders under either, `unsavedburst` exists only in the dead Neovim's swap file. A future change that reopened into a fresh/per-process swap dir, or that answered `SwapExists` with `e` instead of `r`, would leave every other assertion here green | no isolated revert run, by design. Observed red alongside rows 1-3 via scenario exit 19, and green in every run in which `bounceback` was green |
#[test]
fn nvim_crash_recovers_and_reopens_the_file_under_cage() {
    const SMOKE: &str = "nvim_crash_recovers_and_reopens_the_file_under_cage";
    // `bufname('%') winwidth(0) winheight(0)`, written by one `printf`.
    const BEFORE_TOKENS: usize = 3;
    // `winwidth(0) winheight(0)`.
    const DIMS_TOKENS: usize = 2;
    // `&modified !empty(swapname())`, written right after `:preserve`.
    const PRESERVE_TOKENS: usize = 2;
    // Both preserve-readback flags are Vim booleans, so the only acceptable value is
    // 1. Named rather than inlined so the two assertions below cannot drift apart.
    const VIM_TRUE: i64 = 1;

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd, exactly like every other smoke; see `make_temp_cwd` for why
    // isolation is mandatory. The readback files live here too.
    let cwd = make_temp_cwd();
    let fixture = cwd.join(RESTART_FILE_NAME);
    std::fs::write(&fixture, restart_fixture_body())
        .unwrap_or_else(|e| panic!("failed to write the fixture {}: {e}", fixture.display()));

    // Page density for sparse light-on-dark text (see `SIDEBAR_FILLER_COUNT`). Nothing asserts on them.
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY)
            .unwrap_or_else(|e| panic!("failed to write filler {}: {e}", filler.display()));
    }

    // The sidebar draws the readback names, so they contain none of the asserted OCR terms.
    let before = cwd.join("bufname_before.txt");
    let after = cwd.join("bufname_after.txt");
    let dims_after = cwd.join("dims_after.txt");
    let preserved = cwd.join("preserve_state.txt");
    for path in [&before, &after, &dims_after, &preserved] {
        clear_evidence(path, SMOKE);
    }

    // TWO captures: the healthy control and the post-recovery one. Both are cleared
    // first — evidence that could be a previous run's leftover is not evidence.
    let out_png_before = runtime_dir.join("wolf_gnome_crash_before_smoke.png");
    let out_png = runtime_dir.join("wolf_gnome_crash_after_smoke.png");
    for path in [&out_png_before, &out_png] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(&with_nvim_pid_helper(CRASH_SCENARIO), &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("OUT_PNG_BEFORE", &out_png_before)
        .env("RESTART_FILE", RESTART_FILE_NAME)
        .env("UNSAVED_MARKER", RESTART_UNSAVED_MARKER)
        .env("BURST", RESTART_BURST_TEXT)
        .env("BEFORE", &before)
        .env("AFTER", &after)
        .env("DIMS_AFTER", &dims_after)
        .env("PRESERVED", &preserved)
        .status()
        .expect("failed to spawn `cage` for the headless crash/restart smoke scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage crash scenario exited unsuccessfully ({status}). Read the exit code \
         FIRST; each names a different failure.\n\
         Exit 11: the app died before it was ready for input — a LAUNCH failure, nothing to do \
         with crash recovery.\n\
         Exit 14: a readback file the captive Neovim was told to write never appeared. Keys are \
         not reaching Neovim at all; check the keyboard-only smokes first.\n\
         Exit 16: THE APP DIED WITH ITS NEOVIM. This is the worst possible failure of this \
         feature and has its own code so it can never be confused with a slow restart: the GTK \
         window must survive the death of its captive Neovim and show the crash overlay. Look at \
         the drain loop and at the restart path's RefCell borrows.\n\
         Exit 17: the scenario could not identify EXACTLY ONE live `nvim` child of the app, so it \
         refused to kill anything. 0 means nvim never started or had already gone; 2+ means the \
         scan cannot tell which to kill.\n\
         Exit 18: the SIGKILLed Neovim was still a live process after the poll budget.\n\
         Exit 19: recovery TIMED OUT — `bufname('%')` never named {RESTART_FILE_NAME:?} again. \
         This is the headline red-run symptom: either the liveness read is no longer first in the \
         drain loop, or `(Dead, true)` no longer requests an automatic restart, or the restart \
         happened and the Tier-1 reopen did not.\n\
         Captures expected at {} (control) and {}.",
        out_png_before.display(),
        out_png.display()
    );
    assert_png_present(SMOKE, &out_png_before);
    assert_png_present(SMOKE, &out_png);

    // --- The BEFORE state, proved rather than assumed ----------------------
    let before_raw = read_readback(&before, SMOKE);
    let before_tokens: Vec<&str> = before_raw.split_whitespace().collect();
    assert!(
        before_tokens.len() == BEFORE_TOKENS,
        "{SMOKE}: the BEFORE readback {} holds {} whitespace-separated token(s), expected \
         {BEFORE_TOKENS} (bufname, winwidth, winheight). Raw contents were {before_raw:?}. A \
         SHORT file usually means the `printf` format and its argument list disagree; a bufname \
         containing a SPACE would also land here, which no fixture in this binary has.",
        before.display(),
        before_tokens.len()
    );
    assert!(
        before_tokens[0].contains(RESTART_FILE_NAME),
        "{SMOKE}: before the crash Neovim reported its current buffer as {:?}, which does not \
         name the fixture {RESTART_FILE_NAME:?}. The `:e` never took effect, so the crash below \
         would be crashing a session that was not in the state this smoke assumes and nothing \
         downstream would mean anything. Raw contents were {before_raw:?}. Control capture kept \
         at {}.",
        before_tokens[0],
        out_png_before.display()
    );
    let w_before = parse_readback_int(before_tokens[1], &before, &before_raw, SMOKE);
    let h_before = parse_readback_int(before_tokens[2], &before, &before_raw, SMOKE);
    assert!(
        w_before > FIXTURE_LINE_WIDTH,
        "{SMOKE}: Neovim reports winwidth(0) = {w_before} before the crash, which is not wider \
         than the {FIXTURE_LINE_WIDTH}-character fixture line. `runtime/nvim/init.lua` sets \
         `opt.wrap` and `opt.linebreak`, so at this width the fixture soft-wraps and the page \
         layout every OCR assertion below rests on stops being the one that was measured. This is \
         a precondition of the FIXTURE, not of the product: shrink the fixture (see \
         `restart_fixture_body`), never the assertion. Control capture kept at {}.",
        out_png_before.display()
    );

    // Recovered line 2 is a filler line plus the marker; this checks it fits the reported window width
    // with no wrap, passing at `43 <= 43`. See [`RESTART_UNSAVED_MARKER`] before changing it.
    let composed_line = format!(
        "{:03}{}{RESTART_UNSAVED_MARKER}",
        1,
        "x".repeat(RESTART_FILLER_LINE_PAD)
    );
    let composed_width = i64::try_from(composed_line.chars().count())
        .expect("the composed line-2 width is a small positive number and fits in an i64");
    assert!(
        composed_width <= w_before,
        "{SMOKE}: line 2 after the unsaved edit is {composed_width} characters \
         ({composed_line:?}) but Neovim reports winwidth(0) = {w_before}, so \
         `runtime/nvim/init.lua`'s `opt.wrap` + `opt.linebreak` will soft-wrap it. A wrap can \
         fall inside {RESTART_OCR_UNSAVED:?}, and half a word is not a word OCR can find — the \
         assertion below would then fail for a layout reason that has nothing to do with crash \
         recovery. This is a precondition of the FIXTURE: shorten `RESTART_UNSAVED_MARKER` or \
         `RESTART_FILLER_LINE_PAD`, never the assertions."
    );

    // `:preserve` ran on a dirty buffer with a swap, so the survival below is a property, not a race
    // against `'updatecount'`. See `CRASH_SCENARIO`'s "Why step 5 exists".
    let preserve = read_ints(&preserved, PRESERVE_TOKENS, SMOKE);
    let (modified_flag, has_swap_flag) = (preserve[0], preserve[1]);
    assert!(
        modified_flag == VIM_TRUE,
        "{SMOKE}: just before the crash Neovim reported `&modified` = {modified_flag}, not \
         {VIM_TRUE} — the buffer was NOT dirty, so the unsaved edit never landed in it and \
         there was nothing for `:preserve` to flush. The `ggA{RESTART_UNSAVED_MARKER}<Esc>` \
         keystrokes did not reach the buffer: check that the preceding `gg` did not leave \
         Neovim in a pending-operator state, and that the `:e` toast was not still holding the \
         cmdline. Without this the {RESTART_OCR_UNSAVED:?} assertion below would be asserting \
         on an edit that was never made. Control capture kept at {}.",
        out_png_before.display()
    );
    assert!(
        has_swap_flag == VIM_TRUE,
        "{SMOKE}: just before the crash Neovim reported an EMPTY `swapname()` for the fixture \
         buffer, so it has no swap file and `:preserve` had nowhere to write. Swap recovery \
         across a restart is the whole mechanism this smoke's {RESTART_OCR_UNSAVED:?} assertion \
         rests on, and it is configured in `runtime/nvim/init.lua` (`opt.swapfile = true` plus \
         `opt.directory = WOLF_SWAP_DIR .. \"//\"`). An empty swapname means swapfiles were \
         turned off, or `WOLF_SWAP_DIR` was not handed to the captive Neovim at all — in which \
         case a crash silently loses the user's unsaved work, which is a PRODUCT bug and not a \
         test one. Control capture kept at {}.",
        out_png_before.display()
    );

    let dims = read_ints(&dims_after, DIMS_TOKENS, SMOKE);
    let (w_after, h_after) = (dims[0], dims[1]);

    // --- The positive control ----------------------------------------------
    let ocr_before = ocr_to_lowercase(&out_png_before);
    assert!(
        ocr_before.contains(RESTART_OCR_HEADING),
        "{SMOKE}: the CONTROL capture — taken while everything was still healthy, before anything \
         was killed — did not contain {RESTART_OCR_HEADING:?}. Nothing about crash recovery has \
         happened yet, so this is a RENDERING or OCR failure: the fixture's heading was not drawn, \
         or the page was too sparse for tesseract (see `SIDEBAR_FILLER_COUNT`; this smoke writes \
         all {SIDEBAR_FILLER_COUNT} filler files for exactly that reason). Fix this before reading \
         any assertion below — without a passing control an OCR miss after the restart is not \
         diagnosable. PNG kept at {}. OCR output was:\n{ocr_before}",
        out_png_before.display()
    );

    // The negative half of the control: captured before the edit was typed, so a marker here leaked in
    // through a fixture, filler, readback or path, the `PICKER_FILE_NAME` false positive.
    assert!(
        !ocr_before.contains(RESTART_OCR_UNSAVED),
        "{SMOKE}: the CONTROL capture already contained {RESTART_OCR_UNSAVED:?} — but it was taken \
         BEFORE the unsaved edit was typed, so nothing on that page may carry it. Something is \
         rendering the marker word for a reason that has nothing to do with swap recovery, which \
         would make the post-recovery assertion below pass VACUOUSLY. Look for it in this smoke's \
         own fixture (`restart_fixture_body`), in a `SIDEBAR_FILLER_FILES` name, in a readback \
         filename, or in the temp cwd path — and change the FIXTURE, never the OCR const and \
         never the assertion. PNG kept at {}. OCR output was:\n{ocr_before}",
        out_png_before.display()
    );

    // --- The recovery itself ------------------------------------------------
    let ocr_after = ocr_to_lowercase(&out_png);
    assert!(
        ocr_after.contains(RESTART_OCR_HEADING),
        "{SMOKE}: after the restart, OCR did not contain {RESTART_OCR_HEADING:?} — the heading of \
         {RESTART_FILE_NAME}. The scenario exited 0, so `bufname('%')` DID name the fixture, which \
         means Neovim reopened it; so this says the reopened file is not what is on screen. The \
         control capture passed, so it is not a rendering or OCR problem. Read the \
         {RESTART_OCR_BANNER:?} assertion below next: if the banner IS present while this is \
         absent, the app recovered and flashed the banner but painted something other than the \
         reopened fixture. Do NOT go looking at `reopen_order` first: a reversed reopen order \
         was MEASURED to fail EARLIER, at the scenario's own recovery poll (exit 19), \
         so it cannot be what brought you here. PNG kept at {}. OCR output was:\n{ocr_after}",
        out_png.display()
    );
    assert!(
        ocr_after.contains(RESTART_OCR_BANNER),
        "{SMOKE}: after the restart, OCR did not contain {RESTART_OCR_BANNER:?} — the transient \
         \"Neovim was restarted\" banner. It is the ONLY user-visible confirmation on the success \
         path (the fresh Neovim's Alive wake clears the crash overlay before a human could see \
         it), so its absence is a real user-facing regression, not a cosmetic one. Check that the \
         restart path still calls `CrashOverlay::flash_banner()` on the SUCCESS branch only, and \
         that the generation-token timeout has not been made to fire early. If this flakes rather \
         than failing outright, SHORTEN the gap between the scenario's step-9 readback and its \
         `grim` — do NOT raise `liveness_ui::BANNER_VISIBLE_SECS`, which is a product constant. \
         PNG kept at {}. OCR output was:\n{ocr_after}",
        out_png.display()
    );

    // The unsaved edit survived: this word was never on disk, so only the swap file can bring it back.
    // It is deterministic because of the `:preserve` proof above.
    assert!(
        ocr_after.contains(RESTART_OCR_UNSAVED),
        "{SMOKE}: after the restart, OCR did not contain {RESTART_OCR_UNSAVED:?} — the unsaved \
         edit that was typed into {RESTART_FILE_NAME} and flushed to its swap file with \
         `:preserve` before the SIGKILL. The scenario proved the buffer was dirty and had a swap \
         file at that moment, and {RESTART_OCR_HEADING:?} above proves the file was reopened and \
         is on screen — so the file came back from DISK and the user's unsaved work was silently \
         dropped. What to check, in order: (1) `WolfCore::restart_nvim` must reuse the SAME \
         `NvimConfig`, hence the same durable `WOLF_SWAP_DIR`, or the fresh Neovim never sees the \
         dead one's swap; (2) `runtime/nvim/init.lua`'s `SwapExists` autocmd must answer \
         `v:swapchoice = \"r\"` for a DEAD owner (an `\"e\"` there edits the clean on-disk copy \
         and looks exactly like this); (3) the reopen must go through `InputEvent::Open`, which \
         is an `:edit` — the only path on which Neovim arms the SwapExists machinery at all. \
         Note this assertion has no isolated red-run revert of its own (see this test's revert \
         table): reverting the restart makes it red along with everything else. PNG kept at {}. \
         OCR output was:\n{ocr_after}",
        out_png.display()
    );

    // --- The pane-view reset, and ONLY this assertion discriminates it ------
    assert!(
        (w_after, h_after) == (w_before, h_before),
        "{SMOKE}: the pane grid changed across the restart — it was {w_before}x{h_before} before \
         the crash and is {w_after}x{h_after} after. ALL THREE OCR assertions above can pass while \
         this fails, because the buffer text is still drawn at the top-left of an over-large grid, so \
         this is the ONLY assertion in the suite that discriminates \
         `pane_view::reset_after_restart`. `WolfCore::restart_nvim` re-attaches with the config's \
         initial cols/rows (80x24 for wolf-gnome) and NO `::resize` fires on a widget whose \
         allocation did not change — so without the reset the fresh Neovim keeps an 80x24 grid \
         inside a pane that is {w_before}x{h_before} cells and the editor renders at the wrong \
         size for the rest of the session. An `80 24` reading here IS that defect. Check that the \
         restart path still calls `self.inner.panes.reset_after_restart();` before the reopen. \
         PNG kept at {}.",
        out_png.display()
    );
}

/// A WEDGED (not dead) Neovim raises the opaque "Neovim stopped responding" overlay.
///
/// ## What it proves that nothing else can
///
/// The stall half of the crash/stall overlay, end to end, against a genuinely
/// unresponsive Neovim: `SIGSTOP` the captive process, fire Ctrl+Shift+E so the
/// buffer picker's candidate snapshot goes out through `block_on_rpc`, let the
/// budget elapse, and read the raised overlay off a pixel. The chain it exercises —
/// `block_on_rpc`'s `Err(Timeout)` arm marking the core `NotResponding` and firing
/// the wake, `shell::on_liveness_wake` classifying it, `liveness_ui` yielding
/// `OverlayMode::Stalled` with `paint: false`, and `apply_overlay` raising the
/// opaque child and closing the empty picker behind it — has no other end-to-end
/// coverage at all. `crates/wolf-core/tests/nvim_liveness.rs` covers the CORE side
/// headlessly; what no core test can say is whether the GTK front-end ever raises
/// anything.
///
/// It is reachable only because the buffer picker exists. Before it, `wolf-gnome`'s
/// only `block_on_rpc` ran at launch, before the window was interactive, so a
/// running app could not raise this overlay at all.
///
/// ## Why `SIGSTOP` and not `SIGKILL`
///
/// A killed Neovim is `Dead` and raises the CRASH overlay, which is a different
/// state, carries a different message and is already smoked by
/// [`nvim_crash_recovers_and_reopens_the_file_under_cage`] and
/// [`crash_overlay_and_restart_button_recover_under_cage`]. A stopped one is alive,
/// holds its end of the RPC socket open and simply never answers — which is the
/// only thing that produces `NotResponding`. That the two states' messages cannot
/// cross-satisfy each other's OCR terms is pinned in the fast suite by
/// `wolf_gnome::liveness_ui::tests::overlay_message_table`.
///
/// ## Why the capture is taken AFTER the block, not during it
///
/// No frame is committed while the GTK main loop is blocked, so a `grim` fired
/// during the block returns the PRE-CHORD frame — indistinguishable from an overlay
/// that never appeared. The scenario therefore sleeps past the timeout budget and
/// captures the frame presented once `list_buffers` has given up.
///
/// ## Assertions, classified
///
/// A precondition and a control get NO revert: inventing one would manufacture a
/// discriminator this smoke does not have. The `measured` column is from an actual red
/// run.
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the scenario ran to completion | none; do not invent one. Exits 11 / 16 / 17 are documented in the failure message. | n/a |
/// | 2 | both PNGs present | precondition — grim produced two images | none; do not invent one | n/a |
/// | 3 | control OCR contains [`STALL_OCR_ABSENT`] | **precondition** — the fixture rendered at all | none. Without it assertion 6 is vacuous: an editor that drew nothing satisfies "the heading is absent" trivially, so the opacity claim would rest on nothing. | n/a |
/// | 4 | control OCR lacks [`STALL_OCR_PRESENT`] | **control**, not falsifiable by a source revert | none, and that is deliberate. It proves the overlay was raised BY THE CHORD rather than being up all along (a launch-time stall, a mis-classified `Alive`), which is a property of the run and not of a line anyone could revert. Do not invent one. | n/a |
/// | 5 | post OCR contains [`STALL_OCR_PRESENT`] | **discriminator** — a stall was detected and shown | in `buffer_picker.rs`'s `open`, replace the `core.list_buffers()` call with `Vec::new()`. No RPC is issued, nothing elapses, liveness stays `Alive`, and the capture shows the editor. This is the feature-disable, and it also reddens `buffer_picker_chord_switches_to_the_queried_buffer_under_cage` — expected, and stated. The tempting "isolated" alternative (`liveness_ui`'s `NotResponding` arm yielding `OverlayMode::Hidden`) is NOT isolated: it reddens `liveness_ui_table` and `not_responding_never_auto_restarts` in the fast suite. | RED. `after the wedge and the chord, OCR did not contain "responding" — the distinctive word of liveness_ui::STALLED_MESSAGE`. The capture was the healthy editor: `stallfixture` present, no overlay text. As stated, the same revert reddened `buffer_picker_chord_switches_to_the_queried_buffer_under_cage` too (`OCR did not contain "bufpickalfa"`). Restored, green. |
/// | 6 | post OCR lacks [`STALL_OCR_ABSENT`] | **discriminator** — the overlay is OPAQUE and full-bleed | in `crash_overlay::build`, delete `overlay.add_css_class("background")`. The overlay goes transparent and the frozen fixture shows through. Shared with [`crash_overlay_and_restart_button_recover_under_cage`]'s own opacity assertion — the two smokes share the widget, which is why this revert reddens both. | RED. `the post-stall capture still contained "stallfixture" — the heading of stfrozen.md, which was on screen when Neovim was wedged`, while the same OCR dump also carried `neovim stopped responding`: the overlay was up and see-through, which is exactly what this row claims to catch. The sibling smoke went red too, but on its must-CONTAIN assertion (`the dwell capture did not contain "crashed"`) rather than on its opacity assertion; its dwell capture held the pre-crash editor body and none of the overlay's words. Restored, green. |
/// | 7 | (in-scenario) `assert_app_alive` | discriminator, exit 16 | none needed as a Rust assertion; the exit code IS the observable, and it is the worst failure this feature has: the app dying alongside its wedged Neovim. | n/a |
#[test]
fn stalled_nvim_raises_the_stall_overlay_under_cage() {
    const SMOKE: &str = "stalled_nvim_raises_the_stall_overlay_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd holding the one fixture; see `make_temp_cwd` for why
    // isolation is mandatory.
    let cwd = make_temp_cwd();
    let fixture = cwd.join(STALL_FILE);
    std::fs::write(&fixture, format!("{STALL_HEADING}\n")).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to write the fixture {}: {e}",
            fixture.display()
        )
    });

    // Page density. The control capture asserts on a short heading over an
    // otherwise thin editor page — the `SIDEBAR_FILLER_COUNT` hazard exactly.
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
            panic!("{SMOKE}: failed to write filler {}: {e}", filler.display())
        });
    }

    // TWO captures: the healthy control and the post-stall one. Both cleared first
    // — evidence that could be a previous run's leftover is not evidence.
    let out_png_before = runtime_dir.join("wolf_gnome_stall_before_smoke.png");
    let out_png = runtime_dir.join("wolf_gnome_stall_after_smoke.png");
    for path in [&out_png_before, &out_png] {
        clear_evidence(path, SMOKE);
    }

    // Read once at `WolfCore::start`; a timing knob only. See `STALL_RPC_TIMEOUT_MS` for its bounds.
    let status = cage_command(&with_nvim_pid_helper(STALL_SCENARIO), &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("OUT_PNG_BEFORE", &out_png_before)
        .env("STALL_FILE", STALL_FILE)
        .env("WOLF_RPC_TIMEOUT_MS", STALL_RPC_TIMEOUT_MS)
        .status()
        .expect("failed to spawn `cage` for the headless stall smoke scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage stall scenario exited unsuccessfully ({status}). Read the exit code \
         FIRST; each names a different failure.\n\
         Exit 11: the app died before it was ready for input — a LAUNCH failure, nothing to do \
         with stall detection.\n\
         Exit 16: THE APP DIED WITH ITS WEDGED NEOVIM. This is the worst possible failure of this \
         feature and has its own code so it can never be confused with a missing overlay: the GTK \
         window must survive a Neovim that stops answering, show the stall overlay, and stay \
         interactive. A dead app means the main loop blocked forever (is `block_on_rpc` still \
         bounded by WOLF_RPC_TIMEOUT_MS?) or panicked on the liveness wake — look at the drain \
         loop and at the restart path's RefCell borrows.\n\
         Exit 17: the scenario could not identify EXACTLY ONE live `nvim` child of the app, so it \
         refused to SIGSTOP anything and the chord below was fired at a healthy editor. 0 means \
         nvim never started or had already gone; 2+ means the scan cannot tell which to stop.\n\
         Captures expected at {} (control) and {}.",
        out_png_before.display(),
        out_png.display()
    );

    // --- Assertion 2 (precondition): grim produced both images ----------------
    assert_png_present(SMOKE, &out_png_before);
    assert_png_present(SMOKE, &out_png);

    let ocr_before = ocr_to_lowercase(&out_png_before);

    // Assertion 3 (precondition): the fixture rendered while healthy; without it assertion 6 is
    // satisfied by a blank editor, an unmapped window or an unreadable capture.
    assert!(
        ocr_before.contains(STALL_OCR_ABSENT),
        "{SMOKE}: the CONTROL capture — taken while everything was still healthy, before anything \
         was stopped — did not contain {STALL_OCR_ABSENT:?}, the heading of {STALL_FILE}. Nothing \
         about stall detection has happened yet, so this is a RENDERING or OCR failure: the `:e` \
         did not load, the heading was not drawn, or the page was too sparse for tesseract (see \
         `SIDEBAR_FILLER_COUNT`; this smoke writes all {SIDEBAR_FILLER_COUNT} filler files for \
         exactly that reason). Fix this before reading anything below — without a passing control, \
         the ABSENCE of this term after the stall proves nothing about the overlay's opacity. PNG \
         kept at {}. OCR output was:\n{ocr_before}",
        out_png_before.display()
    );

    // Assertion 4 (control): the overlay was raised by the chord, not up since launch. No source revert
    // can falsify it, deliberately.
    assert!(
        !ocr_before.contains(STALL_OCR_PRESENT),
        "{SMOKE}: the CONTROL capture already contained {STALL_OCR_PRESENT:?} — the stall \
         overlay's own word — but it was taken while Neovim was running normally and nothing had \
         been signalled. The post-stall assertion below would then pass with the chord doing \
         nothing at all. Something raised the overlay at LAUNCH: check whether \
         `pane_view::apply_layout_rebuild`'s `block_on_rpc` ran (it should not — a fresh \
         WOLF_STATE_DIR means there is no saved layout to rebuild) and whether \
         {STALL_RPC_TIMEOUT_MS} ms is too short for a healthy round trip on this host, which is \
         the one way a working core marks itself NotResponding by accident. PNG kept at {}. OCR \
         output was:\n{ocr_before}",
        out_png_before.display()
    );

    let ocr_after = ocr_to_lowercase(&out_png);

    // --- Assertion 5 (discriminator): the stall was detected and SHOWN --------
    assert!(
        ocr_after.contains(STALL_OCR_PRESENT),
        "{SMOKE}: after the wedge and the chord, OCR did not contain {STALL_OCR_PRESENT:?} — the \
         distinctive word of `liveness_ui::STALLED_MESSAGE`. The app is still alive (the scenario \
         exited 0, so `assert_app_alive` passed), so the overlay simply was not raised. The likely \
         causes, in order: (a) the chord never reached `win.buffer-picker`, so no RPC was issued \
         at all and the core stayed Alive — the buffer-picker smoke would be red too; (b) `open` \
         issued no blocking call, so nothing could elapse; (c) `block_on_rpc`'s timeout arm no \
         longer calls `mark_not_responding`, or the wake it fires is not reaching \
         `shell::on_liveness_wake`; (d) `liveness_ui(NotResponding, _)` no longer yields \
         `OverlayMode::Stalled` — which would also redden `liveness_ui_table` and \
         `not_responding_never_auto_restarts` in the fast suite, so check those first; (e) the \
         message was reworded to drop the word, which \
         `liveness_ui::tests::overlay_message_table` pins precisely so it cannot happen silently. \
         PNG kept at {}. OCR output was:\n{ocr_after}",
        out_png.display()
    );

    // Assertion 6 (discriminator): `liveness_ui` sets `paint: false`, so the frozen frame stays behind,
    // and only an opaque full-bleed overlay hides the heading.
    assert!(
        !ocr_after.contains(STALL_OCR_ABSENT),
        "{SMOKE}: the post-stall capture still contained {STALL_OCR_ABSENT:?} — the heading of \
         {STALL_FILE}, which was on screen when Neovim was wedged. Assertion 3 proved it rendered \
         while healthy, so a hit HERE means the overlay is up but SEE-THROUGH: the frozen editor \
         frame is showing through it. Check that `crash_overlay::build` still adds the \
         `background` CSS class to the overlay box, and that the overlay is still a full-bleed \
         child rather than a centred one. This is the same widget the crash-overlay smoke asserts \
         opacity on, so that smoke is red too — one fix serves both. PNG kept at {}. OCR output \
         was:\n{ocr_after}",
        out_png.display()
    );
}

/// A FAILED respawn leaves an opaque crash overlay up, and clicking its "Restart
/// Neovim" button recovers the session.
///
/// ## Why this smoke has to break the respawn to exist at all
///
/// The crash overlay is unobservable on the success path — it is shown and then
/// cleared by the fresh Neovim's `Alive` wake within a frame or two — and the OTHER
/// dwelling state, `NotResponding`, is a different overlay mode with a different
/// message, smoked separately by
/// [`stalled_nvim_raises_the_stall_overlay_under_cage`]. The one
/// state in which the CRASH overlay genuinely DWELLS is a crash whose automatic restart
/// FAILED, which is also a real production scenario (Neovim removed or upgraded
/// mid-session, an `exec` failure, an OOM kill). So this smoke induces one, with a
/// `/bin/sh` wrapper at `WOLF_NVIM_BIN` that refuses while a marker file exists —
/// no test-only branch anywhere in the product.
///
/// The wrapper's path is set on the `cage` `Command` ONLY, never on this test
/// process, whose own `WOLF_NVIM_BIN` `require_smoke_env` checks and every serially
/// following smoke still needs.
///
/// ## What it proves that nothing else can
///
/// `crash_overlay.rs` is all GTK widget construction and has NO unit tests — it
/// cannot have any. This smoke is therefore the sole coverage for four of its five
/// revert rows below, which is why EVERY one of them must be run rather than
/// sampled.
///
/// * `crashed` PRESENT during the dwell — the overlay is actually shown. Stated
///   honestly, because it is measured: NO revert exercises this assertion in
///   isolation. The visibility revert (never `set_visible(true)`) exits 19 at the
///   recovery poll before the dwell OCR is read, and the opacity revert fails it for
///   the OPACITY defect. It is a real assertion with no isolated red run of its own.
/// * `bounceback` ABSENT during the dwell — the overlay is OPAQUE and full-bleed.
///   Nothing else in this repo proves occlusion: the empty-state smoke asserts
///   presence only. The assertion is non-vacuous in the other direction too, because
///   the sidebar is NOT covered (the overlay is the `gtk::Paned`'s end child) and it
///   still renders the `bfile.md` row — and `bfile` is not `bounceback`.
/// * `bounceback` PRESENT after the click — the BUTTON drove a real recovery and
///   the Tier-1 reopen ran.
/// * scenario exit 20 — the single automatic attempt really is capped, measured off
///   the wrapper's OWN spawn log rather than off a `/proc` scan (a refused wrapper
///   exits before its `exec` and is never named `nvim`, so the scan is blind to it —
///   see [`RESTART_ATTEMPTS_NAME`]).
///
/// ## No magic pixel: how the click point is derived
///
/// `wolf_gnome::restart_button_rect` is the SAME function the widget builder's size
/// requests realise, so the two cannot drift. The composition is: the cage output
/// extents; minus the OVERLAY origin ([`OVERLAY_ORIGIN_X_EXPECTED`] /
/// [`OVERLAY_ORIGIN_Y_EXPECTED`]), which is the pane area's own top-left corner and
/// therefore the overlay's, since the overlay's main child is the pane container and
/// under cage the window is the output with zero CSD offset; into
/// `restart_button_rect`; then a tenth of the way in from that rect's LEFT edge
/// horizontally, and the rect's CENTRE vertically. That asymmetry is not stylistic —
/// a centre-derived x was measured to be blind to the button-width revert.
/// The derivation comment in the body carries the measurement, and states the width
/// bound the inset does and does not discriminate.
///
/// **The anchor is the OVERLAY origin, never the pane origin, and the two are
/// separate measurements on purpose.** [`PANE_ORIGIN_X_EXPECTED`] /
/// [`PANE_ORIGIN_Y_EXPECTED`] are derived from a click-to-cell readback, so they name
/// the TEXT GRID's top-left — inside the pane widget by
/// `editor_inset::EDITOR_PADDING_PX` horizontally and by the statusbar strip plus
/// that padding vertically. The overlay is anchored to the pane WIDGET's top-left.
/// The two differ by the per-pane chrome, they look interchangeable, and collapsing
/// them puts the derived click a strip and a padding away from the button — which on
/// a 320x64 target is a miss, reported as scenario exit 19. Neither may be derived
/// from the other: `OVERLAY_ORIGIN_*` carries its own measurement and its own
/// cross-check.
///
/// Three self-checking preconditions guard it:
/// [`assert_button_rect_is_the_named_size`] (a collapsed `(0,0,0,0)` must
/// not silently retarget the click to the overlay's top-left),
/// [`assert_button_rect_fits`] (a `PANE_ORIGIN_TOLERANCE_PX` analogue), and
/// [`assert_inside_extents`]. After the run the derivation is REDONE from the run's
/// own reported extents and the injected point is asserted to lie inside the
/// resulting rect.
///
/// **What that redo cannot do, stated because its absence is what let an anchor
/// error live here undetected:** the only origin this run measures for itself is the
/// GRID's, so the redo pins the overlay origin on both sides and is self-consistent
/// on the anchor — it moves with an anchor error rather than catching one. It still
/// catches a changed output size, a collapsed rect, and a click derived from a stale
/// button geometry. Anchor drift is caught by re-measuring the overlay origin the way
/// its provenance was taken, and, cheaply, by the scenario's own recovery poll: a
/// click that misses the button drives no restart and exits 19.
///
/// ## Measured geometry (overlay origin and allocation re-measured 2026-08-04)
///
/// | quantity | value |
/// | --- | --- |
/// | cage output size | `1280x720` |
/// | overlay origin | `(251, 47)` |
/// | overlay allocation | `1029 x 673` (`1280 - 251`, `720 - 47`) |
/// | `restart_button_rect(1029.0, 673.0)` | `(354.5, 344.5, 320.0, 64.0)` |
/// | button, absolute | `x 605.5..925.5`, `y 391.5..455.5` |
/// | rendered button, inspected | `x 605..925`, `y 392..456` |
///
/// The origin and the allocation are a 2026-08-04 measurement, taken from a
/// temporary tick callback over every distinct allocation across the crash, the
/// restart and the click, and reproduced on a second run; the rendered button is an
/// independent inspection of a retained capture. The two agree to within the half
/// pixel GTK snaps a centred block of odd leftover height by, which is what makes the
/// pair a cross-check rather than one number written down twice.
///
/// Unlike the pane-origin constants, these ARE what the pre-run derivation uses, so
/// the table and the click are one composition rather than two: it injects
/// `(251 + 354.5 + 32, 47 + 344.5 + 32)` = `(637.5, 423.5)`, which
/// [`pixel_target`]'s round-half-away-from-zero takes to **(638, 424)** — 32.5 px
/// inside the button's left edge, and on its vertical centre line to within the half
/// pixel that rounding adds.
///
/// CONFIRMED BY EXECUTION, 2026-08-04: this smoke passed in the final verification
/// gate. That discharges the derivation more directly than a transcript of the
/// injected point would, because the point is not asserted on — it is either inside
/// the button, or the restart never fires and the scenario's recovery poll exits 19.
/// A green run is therefore the observation that the derived point hits the target,
/// and it is the anchor above that it confirms.
///
/// ## Red-run reverts (five; run EVERY one — the last column records the measured outcome)
///
/// | assertion | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | `crashed` present | make `CrashOverlay::set_mode` never call `set_visible(true)` | `crashed` absent and `bounceback` PRESENT in the dwell capture (the frozen frame shows through) | red, but NOT by the predicted route: measured `exit status: 19`, scenario stderr `the click at (641, 426) drove no recovery`. The restart button lives INSIDE the overlay, so a never-shown overlay is also a never-shown button and the run exits at the recovery poll before the dwell OCR is read. Restored, green |
/// | `bounceback` absent (opacity) | remove `add_css_class("background")` from the overlay box | BOTH terms present in the dwell capture | red on the FIRST dwell assertion instead: `the dwell capture did not contain "crashed"`, while the same OCR dump carries `# bounceback` — the frozen frame does show through exactly as predicted, and the fully transparent box additionally leaves its own label unreadable against the editor text. Restored, green |
/// | the attempt cap | set `MAX_AUTO_RESTART_ATTEMPTS = u32::MAX` | scenario exit **20**: the app spins respawning against the refusing wrapper. Worth capturing as evidence in its own right — it demonstrates the infinite-retry hazard the cap exists to prevent | **DID NOT GO RED as first written — the smoke stayed fully GREEN with the cap removed.** Cause: `assert_no_nvim_respawn` counted children named `nvim`, and a refused wrapper exits BEFORE its `exec`, so an unbounded retry loop is invisible to it and exit 20 was unreachable. FIXED by having the wrapper log each refusal (see `RESTART_ATTEMPTS_NAME` / `assert_one_restart_attempt`); the same revert then reproduced `exit status: 20` with `the app made 75 automatic respawn attempt(s)` against the capped build's 1. Restored, green |
/// | the button | make `CrashOverlay::connect_restart` drop its callback | scenario exit **19**: the click does nothing | reproduced exactly (`exit status: 19`, scenario stderr `the click at (641, 426) drove no recovery`), restored, green |
/// | the button geometry | change `RESTART_BUTTON_WIDTH` to `40` **in the widget builder only** | the derived point misses the button; scenario exit **19** | **DID NOT GO RED as first written — the smoke stayed fully GREEN.** Cause: the click was derived at the rect's CENTRE, and a `halign(Center)` button is centred on that same point whatever width it requests. Measured by scanning the dwell capture: correct builder renders x 605..924 (320px), the 40px builder renders x 669..861 (193px, the label's natural width) — both centred on x 765, and even the 3/4 point (x 845) is still inside the narrow one. FIXED by aiming a tenth in from the rect's LEFT edge (x 641); the same revert then reproduced `exit status: 19` with `the click at (641, 426) drove no recovery`. Restored, green |
#[test]
fn crash_overlay_and_restart_button_recover_under_cage() {
    const SMOKE: &str = "crash_overlay_and_restart_button_recover_under_cage";
    // `line('.') col('.')`.
    const PROBE_TOKENS: usize = 2;
    // rwxr-xr-x: the app execs this wrapper, so it must be executable.
    const WRAPPER_MODE: u32 = 0o755;
    // The inclusive band of scenario exit codes this smoke adds on top of
    // `POINTER_PREAMBLE`'s shared 11-15, and which therefore need their own message.
    const CRASH_EXIT_FIRST: i32 = 16;
    const CRASH_EXIT_LAST: i32 = 20;

    use std::os::unix::fs::PermissionsExt;

    let runtime_dir = require_smoke_env();

    // The real Neovim, baked into the wrapper as a literal; nothing relies on the wrapper's env.
    let real_nvim = std::env::var("WOLF_NVIM_BIN")
        .expect("require_smoke_env proved WOLF_NVIM_BIN is set and non-empty");

    let cwd = make_temp_cwd();
    let fixture = cwd.join(RESTART_FILE_NAME);
    std::fs::write(&fixture, restart_fixture_body())
        .unwrap_or_else(|e| panic!("failed to write the fixture {}: {e}", fixture.display()));

    // Literal paths and `exec`, so the app's child is still `nvim` for `find_nvim_pid`. A refused spawn
    // logs to `$attempts`, the only count a `/proc` scan cannot miss; see `assert_one_restart_attempt`.
    let fail_marker = cwd.join(RESTART_FAIL_MARKER_NAME);
    let attempts = cwd.join(RESTART_ATTEMPTS_NAME);
    let wrapper = cwd.join(RESTART_WRAPPER_NAME);
    let wrapper_script = format!(
        "#!/bin/sh\nif [ -e '{}' ]; then echo refused >> '{}'; exit 1; fi\nexec '{real_nvim}' \
         \"$@\"\n",
        fail_marker.display(),
        attempts.display()
    );
    std::fs::write(&wrapper, wrapper_script)
        .unwrap_or_else(|e| panic!("failed to write the wrapper {}: {e}", wrapper.display()));
    let wrapper_mode = std::fs::Permissions::from_mode(WRAPPER_MODE);
    std::fs::set_permissions(&wrapper, wrapper_mode).unwrap_or_else(|e| {
        panic!(
            "failed to make the failing-respawn wrapper {} executable: {e}. The app execs it as \
             WOLF_NVIM_BIN, so a non-executable wrapper would fail EVERY spawn including the \
             FIRST, and the app would never attach Neovim at all.",
            wrapper.display()
        )
    });

    let before = cwd.join("bufname_before.txt");
    let pos0 = cwd.join("probe_pos0.txt");
    let after = cwd.join("bufname_after.txt");
    // The marker must NOT exist at launch, or the very first spawn is refused. The
    // attempt log must not either: a leftover would make the cap look breached.
    for path in [&before, &pos0, &after, &fail_marker, &attempts] {
        clear_evidence(path, SMOKE);
    }

    let out_png_overlay = runtime_dir.join("wolf_gnome_crash_overlay_smoke.png");
    let out_png = runtime_dir.join("wolf_gnome_crash_button_smoke.png");
    for path in [&out_png_overlay, &out_png] {
        clear_evidence(path, SMOKE);
    }

    // Measure the cell EXACTLY, the same way the app does, BEFORE the run.
    let metrics = cell_metrics_for_smoke();

    // The anchor is the overlay origin, not the pane origin. Measured constants are needed before cage
    // starts, so the run re-checks them against the reported extents.
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "the expected cage output width");
    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "the expected cage output height");
    let overlay_w = output_w - OVERLAY_ORIGIN_X_EXPECTED;
    let overlay_h = output_h - OVERLAY_ORIGIN_Y_EXPECTED;
    let rect = wolf_gnome::restart_button_rect(overlay_w, overlay_h);
    assert_button_rect_is_the_named_size(SMOKE, rect);
    assert_button_rect_fits(SMOKE, rect, (overlay_w, overlay_h));
    let (bx, by, bw, bh) = rect;
    // A tenth in from the rect's left edge, not its centre: a centred click hits a shrunken button too.
    // See notes/crates/wolf-gnome/tests/smoke_headless/recovery.rs.md#restart-button-click-point
    const BUTTON_HIT_INSET_FRACTION: f64 = 0.1;
    let btn_x = pixel_target(
        OVERLAY_ORIGIN_X_EXPECTED + bx + bw * BUTTON_HIT_INSET_FRACTION,
        "the restart button x",
    );
    let btn_y = pixel_target(
        OVERLAY_ORIGIN_Y_EXPECTED + by + bh / 2.0,
        "the restart button y",
    );
    let btn_x_env = btn_x.to_string();
    let btn_y_env = btn_y.to_string();

    let body = with_nvim_pid_helper(POINTER_RESTART_BUTTON_BODY);
    let status = run_pointer_scenario(
        &body,
        &cwd,
        &runtime_dir,
        &out_png,
        &[
            // The wrapper, on the CAGE command only.
            ("WOLF_NVIM_BIN", wrapper.as_os_str()),
            ("FAIL_MARKER", fail_marker.as_os_str()),
            ("ATTEMPTS", attempts.as_os_str()),
            ("RESTART_FILE", OsStr::new(RESTART_FILE_NAME)),
            ("BEFORE", before.as_os_str()),
            ("POS0", pos0.as_os_str()),
            ("AFTER", after.as_os_str()),
            ("OUT_PNG_OVERLAY", out_png_overlay.as_os_str()),
            ("BTN_X", OsStr::new(&btn_x_env)),
            ("BTN_Y", OsStr::new(&btn_y_env)),
        ],
    );
    // Crash-specific exit codes first; the rest fall through to `assert_pointer_scenario_succeeded`.
    assert!(
        !matches!(status.code(), Some(c) if (CRASH_EXIT_FIRST..=CRASH_EXIT_LAST).contains(&c)),
        "{SMOKE}: the cage scenario exited with a CRASH-SPECIFIC code ({status}).\n\
         Exit 16: THE APP DIED WITH ITS NEOVIM. This is the worst possible failure of this \
         feature and has its own code so it can never be confused with anything else: the GTK \
         window must survive the death of its captive Neovim and show the crash overlay.\n\
         Exit 17: the scenario could not identify EXACTLY ONE live `nvim` child of the app, so it \
         refused to kill a guess. 0 means Neovim never started or had already gone; 2+ means the \
         scan cannot tell which to kill.\n\
         Exit 18: the SIGKILLed Neovim was still a live process after the poll budget.\n\
         Exit 19: the button click drove NO recovery — `bufname('%')` never named \
         {RESTART_FILE_NAME:?} again. Either the button's handler is gone, or the derived point \
         missed it, or the restart ran and the Tier-1 reopen did not.\n\
         Exit 20: EITHER a new Neovim child appeared during the quiet window — so the \
         WOLF_NVIM_BIN wrapper did not refuse (check that {} is executable and that the marker \
         path baked into it matches {}) and the overlay never dwelled — OR the wrapper's own \
         spawn log did not hold EXACTLY ONE refused attempt, which means \
         `MAX_AUTO_RESTART_ATTEMPTS` no longer caps the automatic retry (more than one) or no \
         automatic restart ran at all (zero). Read the scenario's stderr: it prints which, and \
         the count.\n\
         Captures expected at {} (dwell) and {}.",
        wrapper.display(),
        fail_marker.display(),
        out_png_overlay.display(),
        out_png.display()
    );
    assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    assert_png_present(SMOKE, &out_png_overlay);
    assert_png_present(SMOKE, &out_png);

    // --- The geometry the derivation assumed, checked against the run ------
    let extents = read_output_extents(&cwd, SMOKE);
    assert!(
        extents == (CAGE_OUTPUT_W_EXPECTED, CAGE_OUTPUT_H_EXPECTED),
        "{SMOKE}: `wolf-pointer-inject` reported a {}x{} output, but the restart-button point was \
         derived BEFORE the run against CAGE_OUTPUT_W_EXPECTED x CAGE_OUTPUT_H_EXPECTED = \
         {CAGE_OUTPUT_W_EXPECTED}x{CAGE_OUTPUT_H_EXPECTED}. The click was therefore aimed using \
         the wrong overlay size. This constant is PINNED rather than assumed precisely so the \
         change is loud: re-measure the output, update both constants with the date, and re-run. \
         Do not widen anything.",
        extents.0,
        extents.1
    );
    assert_inside_extents(
        SMOKE,
        "the PROBE point",
        (i64::from(PROBE_X), i64::from(PROBE_Y)),
        extents,
    );
    assert_inside_extents(SMOKE, "the RESTART BUTTON point", (btn_x, btn_y), extents);

    // --- The pane origin, and the derivation redone from THIS run ----------
    let raw0 = read_readback(&pos0, SMOKE);
    let probe_cell = read_ints(&pos0, PROBE_TOKENS, SMOKE);
    let (l0, c0) = (probe_cell[0], probe_cell[1]);
    let (origin_x, origin_y) =
        assert_probe_cell_and_pane_origin(SMOKE, (l0, c0), &metrics, extents, &out_png, &raw0);

    // The redo pins the overlay origin on both sides, so it cannot see anchor drift; it catches a new
    // output size, a collapsed rect and stale button geometry. A missed click exits 19.
    let measured_overlay_w =
        exact_f64(extents.0, "the reported cage output width") - OVERLAY_ORIGIN_X_EXPECTED;
    let measured_overlay_h =
        exact_f64(extents.1, "the reported cage output height") - OVERLAY_ORIGIN_Y_EXPECTED;
    let measured_rect = wolf_gnome::restart_button_rect(measured_overlay_w, measured_overlay_h);
    assert_button_rect_is_the_named_size(SMOKE, measured_rect);
    let measured_overlay = (measured_overlay_w, measured_overlay_h);
    assert_button_rect_fits(SMOKE, measured_rect, measured_overlay);
    let (mbx, mby, mbw, mbh) = measured_rect;
    let hit_x = exact_f64(btn_x, "the injected button x");
    let hit_y = exact_f64(btn_y, "the injected button y");
    assert!(
        hit_x >= OVERLAY_ORIGIN_X_EXPECTED + mbx
            && hit_x <= OVERLAY_ORIGIN_X_EXPECTED + mbx + mbw
            && hit_y >= OVERLAY_ORIGIN_Y_EXPECTED + mby
            && hit_y <= OVERLAY_ORIGIN_Y_EXPECTED + mby + mbh,
        "{SMOKE}: the point actually injected, ({btn_x}, {btn_y}), lies OUTSIDE the button rect \
         this run's own extents describe: overlay origin ({OVERLAY_ORIGIN_X_EXPECTED}, \
         {OVERLAY_ORIGIN_Y_EXPECTED}) plus \
         `restart_button_rect({measured_overlay_w}, {measured_overlay_h})` = ({mbx}, {mby}, \
         {mbw}, {mbh}), i.e. absolute x {}..{}, y {}..{}. Both sides use the pinned overlay \
         origin, so this cannot be an anchor disagreement: what it names is a changed OUTPUT SIZE \
         (the extents assertion above prints the reported one) or a `restart_button_rect` whose \
         shape moved under the click derivation. The pane origin derived from this run's probe \
         click was ({origin_x}, {origin_y}) — a DIFFERENT corner, the text grid's, and not \
         comparable with these numbers. PNG kept at {}.",
        OVERLAY_ORIGIN_X_EXPECTED + mbx,
        OVERLAY_ORIGIN_X_EXPECTED + mbx + mbw,
        OVERLAY_ORIGIN_Y_EXPECTED + mby,
        OVERLAY_ORIGIN_Y_EXPECTED + mby + mbh,
        out_png.display()
    );

    // --- The BEFORE state, proved rather than assumed ----------------------
    let before_text = read_trimmed(&before, SMOKE);
    assert!(
        before_text.contains(RESTART_FILE_NAME),
        "{SMOKE}: before the crash Neovim reported its current buffer as {before_text:?}, which \
         does not name the fixture {RESTART_FILE_NAME:?}. The `:e` never took effect, so the probe \
         click landed in some other buffer and the whole geometry derivation is describing the \
         wrong thing. PNG kept at {}.",
        out_png.display()
    );

    // --- The DWELL capture: the overlay is up, and it is OPAQUE ------------
    let ocr_overlay = ocr_to_lowercase(&out_png_overlay);
    assert!(
        ocr_overlay.contains(RESTART_OCR_OVERLAY),
        "{SMOKE}: the dwell capture did not contain {RESTART_OCR_OVERLAY:?}, so the crash overlay \
         was not on screen. The scenario reached this point, which means the app survived the kill \
         and no Neovim respawned during the quiet window — so the state IS `Dead` with the \
         automatic attempt spent, and the overlay should be showing \
         `liveness_ui::CRASHED_FAILED_MESSAGE`. Check, in order: (1) `CrashOverlay::set_mode` \
         still calls `set_visible(true)`; (2) the crash overlay is still added to the \
         `gtk::Overlay` ABOVE the picker and the empty state; (3) `shell::on_liveness_wake` still \
         applies the mapping on every wake. If {RESTART_OCR_HEADING:?} is present in the same \
         capture, the frozen pre-crash frame is showing through, which is the same finding from \
         the other side. PNG kept at {}. OCR output was:\n{ocr_overlay}",
        out_png_overlay.display()
    );
    assert!(
        !ocr_overlay.contains(RESTART_OCR_HEADING),
        "{SMOKE}: the dwell capture contained {RESTART_OCR_HEADING:?} — the fixture's heading — \
         WHILE the crash overlay was up. The overlay's whole purpose is to hide the frozen last \
         frame of a dead Neovim, so this says it is not opaque or not full-bleed: check that the \
         overlay box still carries `add_css_class(\"background\")` and still has \
         `hexpand`/`vexpand(true)` with `halign`/`valign(Fill)`. Nothing else in this repo proves \
         occlusion — the empty-state smoke asserts presence only — so this assertion is the sole \
         coverage of that guarantee. It is not vacuous in the other direction either: the sidebar \
         is NOT covered by the overlay and still renders the {RESTART_FILE_NAME:?} row, whose \
         lowercase does not contain {RESTART_OCR_HEADING:?}. PNG kept at {}. OCR output \
         was:\n{ocr_overlay}",
        out_png_overlay.display()
    );

    // --- The BUTTON drove a real recovery ----------------------------------
    let ocr_after = ocr_to_lowercase(&out_png);
    assert!(
        ocr_after.contains(RESTART_OCR_HEADING),
        "{SMOKE}: after the button click, OCR did not contain {RESTART_OCR_HEADING:?} — the \
         heading of {RESTART_FILE_NAME}. The scenario exited 0, so `bufname('%')` DID name the \
         fixture, which means the restart and the reopen both ran; so this says the reopened file \
         is not what is on screen. Check that the crash overlay was HIDDEN again on the success \
         path (`apply_overlay(OverlayMode::Hidden)`) — an overlay left up would occlude exactly \
         this heading, which is the same defect the dwell assertion above proves the overlay is \
         capable of. PNG kept at {}. OCR output was:\n{ocr_after}",
        out_png.display()
    );
}
