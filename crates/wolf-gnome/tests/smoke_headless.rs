//! Opt-in, headless-compositor end-to-end smoke test for the real `wolf-gnome`
//! window.
//!
//! ## What it proves
//!
//! That the built `wolf-gnome` binary launches a real GTK/Adwaita window under a
//! headless Wayland compositor, attaches Neovim, accepts keyboard input, and
//! *actually renders glyphs* — verified by screenshotting the live window and
//! OCR-ing it. Unlike the Tier-1 seam tests (which exercise `gdk_key`/`paint`
//! offscreen), this is the only test that confirms the whole stack draws a window
//! a human could see.
//!
//! ## Why it is `#[ignore]` (opt-in)
//!
//! It needs a compositor + a screenshot/OCR toolchain that a normal `cargo test`
//! run (and most CI lanes) will not have, so it is ignored by default. Run it
//! explicitly:
//!
//! ```text
//! nix develop --command cargo test -p wolf-gnome -- --ignored
//! ```
//!
//! ## Requirements (all provided by the flake devShell's smoke toolchain)
//!
//! - `cage` (headless wlroots compositor), `wtype`, `grim`, `tesseract` on `PATH`.
//! - `WOLF_NVIM_BIN` pointing at the pinned Neovim (same contract as the other
//!   wolf-gnome tests; nvim is never taken from `PATH`).
//! - `XDG_RUNTIME_DIR` set and writable (the Wayland socket lives there).
//!
//! Per project policy we FAIL LOUD if any of the above is missing rather than
//! silently skipping, mirroring `common::require_nvim_bin` — a missing
//! precondition means the smoke toolchain was not provisioned, which is a setup
//! bug we want surfaced, not hidden. (NOTE to coder: the compositor tools come
//! from the flake, so a hard fail is intentional even for the "optional"-looking
//! binaries; do not soften this to a skip without revisiting the plan.)

use std::path::{Path, PathBuf};
use std::process::Command;

/// The lowercase word we type in insert mode (in the window smoke scenario).
/// Kept short and all-lowercase so OCR noise is unlikely to corrupt every
/// character. After typing it we move the cursor OFF its row (see [`SCENARIO`])
/// so the full word — not just a prefix — can be asserted against OCR.
const OCR_WORD: &str = "hello";

/// Serializes the nine `#[ignore]` cage smoke tests. Each launches its own
/// `cage` compositor, and two compositors started concurrently contend for the
/// same Wayland socket under `XDG_RUNTIME_DIR` (lockfile error), failing both.
/// Under the default multi-threaded test harness these tests would otherwise run
/// in parallel, so each acquires this lock for its whole body to force serial
/// execution. Poisoning is recovered from (`into_inner`) so one panicking test
/// does not cascade into failing the others.
static CAGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Font size (points) handed to the app via `WOLF_FONT_PT`. Larger than the
/// interactive default so the rendered glyphs are big enough for `tesseract` to
/// recognize reliably in a screenshot.
const SMOKE_FONT_PT: &str = "28";

/// Picker smoke scenario: the markdown file dropped into the launched app's temp
/// working directory. Its name contains `PICKER_QUERY` so the typed query matches
/// it, and its heading (below) contains `PICKER_OCR` so OCR can confirm nvim
/// actually opened it.
const PICKER_FILE_NAME: &str = "pickertarget.md";

/// Heading written into [`PICKER_FILE_NAME`]. Once the picker opens the file,
/// nvim renders this line; lowercased, the rendered text contains [`PICKER_OCR`].
const PICKER_HEADING: &str = "# PICKERTARGET";

/// Lowercased substring OCR must find in the screenshot to prove the picker
/// opened, matched, selected, and nvim loaded the target file.
const PICKER_OCR: &str = "pickertarget";

/// Text typed into the picker's search entry to filter down to the target file.
const PICKER_QUERY: &str = "picker";

// ---------------------------------------------------------------------------
// Keyboard-navigation picker smoke (Up/Down move the list selection).
//
// This scenario proves arrow-key navigation: with TWO files both matching the
// query, pressing `Down` once moves the list selection to the SECOND row, and
// `Enter` then opens THAT row (not the top one).
//
// Deterministic row order (no MRU yet, so order is purely the rank function):
// both files are created fresh in the temp cwd, so neither is in the MRU cache —
// `mru_group`/`mru_rank` tie. With query `pick`, both `pick1.md` and `pick2.md`
// score identically: the single path component (`pick1.md` / `pick2.md`) starts
// with `pick` (+100), the word before the extension (`pick1` / `pick2`) starts
// with `pick` (+20), and `pick` is a contiguous substring (+4) => 124 each. The
// later keys also tie, so the lexicographic `display` tiebreak decides:
//   - score: 124 => tie
//   - match_start: 0 => tie
//   - span: `pick` is contiguous, last-first => tie
//   - display.len: `pick1.md` and `pick2.md` are equal length => tie
//   - display lexicographic: `pick1.md` < `pick2.md` => `pick1.md` is ROW 0,
//     `pick2.md` is ROW 1.
// So Down (one step) selects row 1 = `pick2.md`, whose heading is `# PICKTWO`;
// OCR must then contain `picktwo`. We also (loosely) assert OCR does NOT contain
// `pickone`, proving the selection actually moved off the top row.
//
// NOTE to coder: this relies on (a) the rank tiebreaks resolving these two equal
// matches by display length then lexicographically (so `pick1.md` is row 0), and
// (b) `Down` moving the LIST/ListBox selection rather than the SearchEntry text
// cursor. If either assumption is wrong the OCR assertion (`picktwo`) will fail —
// verify both against the picker implementation.

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
const PICKER_NAV_OCR_PRESENT: &str = "picktwo";

/// Lowercased substring OCR must NOT find: the heading of row 0 (`pick1.md`),
/// proving navigation moved the selection off the top row before activating.
const PICKER_NAV_OCR_ABSENT: &str = "pickone";

// ---------------------------------------------------------------------------
// Tree-sidebar population smoke (the left-hand file tree lists cwd files).
//
// This scenario proves the S3 tree sidebar POPULATES from the same FileSearch
// mechanism as the picker: a distinctively-named markdown file dropped into the
// app's cwd must appear (by FILENAME) in the left-hand tree, with no picker
// interaction at all. We assert ONLY that the filename text renders in the
// window (loose OCR contains), which demonstrates the long-lived FileSearch fed
// the tree projection (`children_of`) and the ListView drew the row.
//
// NOTE to coder: this asserts tree POPULATION only (the filename is visible in
// the sidebar). Single-click-to-open and the current-file highlight/reveal are
// NOT verified here — they are covered by S1 (current_file core tests), S2 +
// `rel_path_for` pure unit tests, and manual/visual checking, because OCR cannot
// reliably confirm a highlight color and we deliberately avoid pixel-accurate
// row clicking (too flaky — the architect explicitly avoids it).
//
// NOTE to coder: adding the gtk::Paned + tree ListView to the left of the editor
// MUST NOT break the existing `window_renders_typed_text_under_cage` and
// `picker_*_under_cage` OCR assertions — the editor must still render its glyphs
// (OCR_WORD / PICKER_OCR) in the right-hand pane. Verify those three smokes still
// pass after wiring the Paned (e.g. give the editor pane adequate width).

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
const TREE_OCR: &str = "treenode";

// ---------------------------------------------------------------------------
// Vertical-split smoke (S3): two panes render two different files side by side.
//
// This scenario proves the S3 native-panes feature end to end: open file A in the
// (single) editor pane, trigger a VERTICAL split (`<C-w>v`), open file B in the
// new pane, and confirm BOTH files' headings render at once — i.e. the shell built
// a second pane bound to the new nvim window/grid and painted it beside the first.
//
// Two distinctly-named markdown files are dropped into the app's temp cwd by the
// caller (`paneleft.md` => `# PANELEFT`, `paneright.md` => `# PANERIGHT`). The OCR
// assertion is LOOSE (contains BOTH `paneleft` AND `paneright`).
//
// NOTE to coder (the `<C-w>v` chord): the scenario sends the window-command chord
// as `wtype -M ctrl -k w -m ctrl` (hold Ctrl, tap w, release Ctrl) followed by
// `wtype -k v`. Verify this against the installed wtype build and against how the
// init.lua keymap expects the chord (S2 wired <C-w>v -> vsplit). If your wtype
// needs a single combined invocation (e.g. `wtype -M ctrl -k w -m ctrl -k v`) or a
// different release sequence, adjust the two chord lines only — the rest of the
// scenario does not depend on how the chord is delivered.
//
// NOTE to coder (OCR width): both files are opened at WOLF_FONT_PT=28 (matching the
// other smokes). Two panes SIDE BY SIDE halve the per-pane width, so 28pt glyphs in
// each pane may be too narrow for tesseract to read reliably. If this OCR fails on
// width alone (headings clipped/illegible) — NOT on a real rendering bug — the
// coder may launch THIS test with a wider window and/or a smaller WOLF_FONT_PT so
// both headings fit. Keep the assertion loose (contains).
//
// NOTE to coder (cmdline visibility): this relies on the ★ S3 cmdline strip — the
// typed `:e <file><CR>` Ex-commands live on grid 1, which must be rendered as the
// bottom command strip for the typed text to take effect/be visible. If `:e` does
// not open the files, confirm the grid-1 strip + that nvim_input reaches cmdline.

/// Vertical-split smoke: file opened in the FIRST (left) pane.
const VSPLIT_FILE_LEFT: &str = "paneleft.md";

/// Heading written into [`VSPLIT_FILE_LEFT`]; lowercased it contains
/// [`VSPLIT_OCR_LEFT`].
const VSPLIT_HEADING_LEFT: &str = "# PANELEFT";

/// Lowercased substring OCR must find for the LEFT pane's file.
const VSPLIT_OCR_LEFT: &str = "paneleft";

/// Vertical-split smoke: file opened in the SECOND (right) pane after `<C-w>v`.
const VSPLIT_FILE_RIGHT: &str = "paneright.md";

/// Heading written into [`VSPLIT_FILE_RIGHT`]; lowercased it contains
/// [`VSPLIT_OCR_RIGHT`].
const VSPLIT_HEADING_RIGHT: &str = "# PANERIGHT";

/// Lowercased substring OCR must find for the RIGHT pane's file.
const VSPLIT_OCR_RIGHT: &str = "paneright";

// ---------------------------------------------------------------------------
// Horizontal-split smoke (S3): two panes render two different files STACKED.
//
// Mirror of the vertical-split smoke, but for a HORIZONTAL split (`<C-w>s`): open
// file A in the (single) editor pane, trigger a horizontal split, open file B in
// the new pane, and confirm BOTH headings render at once — i.e. the shell built a
// second pane bound to the new nvim window/grid and painted it ABOVE/BELOW the
// first (stacked, sharing the full editor width).
//
// Two distinctly-named markdown files are dropped into the app's temp cwd by the
// caller (`htop.md` => `# HTOPFILE`, `hbot.md` => `# HBOTFILE`). The OCR assertion
// is LOOSE (contains BOTH `htopfile` AND `hbotfile`).
//
// NOTE to coder (the `<C-w>s` chord): the scenario sends the window-command chord
// as `wtype -M ctrl -k w -m ctrl` (hold Ctrl, tap w, release Ctrl) followed by
// `wtype -k s`, exactly mirroring the vertical-split smoke's `<C-w>v`. Verify this
// against the installed wtype build and how the init.lua keymap expects the chord
// (S2 wired <C-w>s -> horizontal split). Adjust the two chord lines only if needed.
//
// NOTE to coder (OCR height): both files are opened at WOLF_FONT_PT=28 (matching
// the other smokes). Two panes STACKED halve the per-pane HEIGHT, so each pane may
// be too short for tesseract to read both headings reliably. If this OCR fails on
// height alone (a heading clipped/illegible) — NOT on a real rendering bug — the
// coder may launch THIS test with a taller window and/or a smaller WOLF_FONT_PT so
// both headings fit. Try as-is first; keep the assertion loose (contains).

/// Horizontal-split smoke: file opened in the FIRST (top) pane.
const HSPLIT_FILE_TOP: &str = "htop.md";

/// Heading written into [`HSPLIT_FILE_TOP`]; lowercased it contains
/// [`HSPLIT_OCR_TOP`].
const HSPLIT_HEADING_TOP: &str = "# HTOPFILE";

/// Lowercased substring OCR must find for the TOP pane's file.
const HSPLIT_OCR_TOP: &str = "htopfile";

/// Horizontal-split smoke: file opened in the SECOND (bottom) pane after `<C-w>s`.
const HSPLIT_FILE_BOTTOM: &str = "hbot.md";

/// Heading written into [`HSPLIT_FILE_BOTTOM`]; lowercased it contains
/// [`HSPLIT_OCR_BOTTOM`].
const HSPLIT_HEADING_BOTTOM: &str = "# HBOTFILE";

/// Lowercased substring OCR must find for the BOTTOM pane's file.
const HSPLIT_OCR_BOTTOM: &str = "hbotfile";

// ---------------------------------------------------------------------------
// Focus-move smoke (S3): `<C-w>v` then `<C-w>h` returns focus to the LEFT pane,
// where a typed marker lands.
//
// This scenario exercises the PURE focus-move intent end to end (decision D4):
// open file A in the (single) editor pane, vertical-split (`<C-w>v`) which leaves
// focus in the NEW (right) pane, open file B there, then move focus back LEFT with
// `<C-w>h`. With focus on the left pane we enter insert mode and type a distinctive
// marker (`iZZLEFT<Esc>`); the marker lands in WHICHEVER pane is focused.
//
// Assertions (LOOSE): OCR contains `zzleft` (the insert happened) AND still
// contains BOTH `focusone` and `focustwo` (the split persisted across the focus
// move and marker insert).
//
// NOTE to coder: this verifies the focus-move PLUMBING end to end (chord ->
// FocusDir intent -> shell neighbor -> nvim_set_current_win -> keys route to the
// refocused window). It does NOT pixel-verify WHICH pane received the marker —
// positional OCR (proving `zzleft` sits in the LEFT pane's column next to
// `focusone`) is too fragile to assert here. The pure `layout::neighbor` unit
// tests cover the geometry of which pane is the left neighbor; this smoke proves
// the wiring fires and the layout survives. If the coder can make a cleaner
// deterministic per-pane assertion (e.g. distinct unambiguous text only reachable
// in the left pane), they may strengthen it.

/// Focus-move smoke: file opened in the FIRST (left) pane.
const FOCUS_FILE_LEFT: &str = "fone.md";

/// Heading written into [`FOCUS_FILE_LEFT`]; lowercased it contains
/// [`FOCUS_OCR_LEFT`].
const FOCUS_HEADING_LEFT: &str = "# FOCUSONE";

/// Lowercased substring OCR must find for the LEFT pane's file (split intact).
const FOCUS_OCR_LEFT: &str = "focusone";

/// Focus-move smoke: file opened in the SECOND (right) pane after `<C-w>v`.
const FOCUS_FILE_RIGHT: &str = "ftwo.md";

/// Heading written into [`FOCUS_FILE_RIGHT`]; lowercased it contains
/// [`FOCUS_OCR_RIGHT`].
const FOCUS_HEADING_RIGHT: &str = "# FOCUSTWO";

/// Lowercased substring OCR must find for the RIGHT pane's file (split intact).
const FOCUS_OCR_RIGHT: &str = "focustwo";

/// Distinctive marker typed (in insert mode) after focusing back LEFT. Its
/// lowercased form is [`FOCUS_OCR_MARKER`]. All-uppercase/short to survive OCR.
const FOCUS_MARKER: &str = "ZZLEFT";

/// Lowercased substring OCR must find to prove the post-`<C-w>h` insert landed in
/// the focused buffer.
const FOCUS_OCR_MARKER: &str = "zzleft";

// ---------------------------------------------------------------------------
// Close-pane smoke (S3): `<C-w>c` closes the focused pane; the survivor reclaims
// the full editor width.
//
// This scenario proves close end to end (decision D4 — close is PERFORMED by nvim
// and observed by the shell via the `win_close` redraw event, no intent variant):
// open file A in the (single) editor pane, vertical-split (`<C-w>v`) which focuses
// the NEW (right) pane, open file B there, then close the focused (right) pane with
// `<C-w>c`. The right pane disappears and the left (survivor) reclaims the width.
//
// Assertions: OCR contains `closekeep` (the survivor renders) and does NOT contain
// `closegone` (the closed pane is gone).
//
// NOTE to coder: if nvim leaves the closed buffer listed elsewhere, or OCR noise
// makes `closegone` linger in the capture, relax to ONLY asserting `closekeep` is
// present (survivor renders). Try the stronger NOT-contains assertion first.

/// Close-pane smoke: file opened in the FIRST (left) pane — the SURVIVOR.
const CLOSE_FILE_KEEP: &str = "ckeep.md";

/// Heading written into [`CLOSE_FILE_KEEP`]; lowercased it contains
/// [`CLOSE_OCR_KEEP`].
const CLOSE_HEADING_KEEP: &str = "# CLOSEKEEP";

/// Lowercased substring OCR MUST find: the survivor pane's file.
const CLOSE_OCR_KEEP: &str = "closekeep";

/// Close-pane smoke: file opened in the SECOND (right) pane — the one CLOSED.
const CLOSE_FILE_GONE: &str = "cgone.md";

/// Heading written into [`CLOSE_FILE_GONE`]; lowercased it contains
/// [`CLOSE_OCR_GONE`].
const CLOSE_HEADING_GONE: &str = "# CLOSEGONE";

/// Lowercased substring OCR must NOT find: the closed pane's file.
const CLOSE_OCR_GONE: &str = "closegone";

// ---------------------------------------------------------------------------
// Three-pane regression smoke (S3 n-ary frame): three files stay visible at once.
//
// This is the regression for the 3-pane bug (architect's N-ARY frame redesign):
// open file A, vertical-split (`<C-w>v`) and open file B in the new RIGHT pane
// (A|B), focus back LEFT (`<C-w>h`), then vertical-split AGAIN and open file C in
// the new sub-pane. Under the n-ary model this flattens into ONE row of three
// equal columns [A, C, B]; the OLD binary always-nest model collapsed the pre-
// existing RIGHT pane (B) to ~0 width, blanking file B.
//
// Three distinctly-named markdown files are dropped into the app's temp cwd by the
// caller (`tri_a.md` => `# TRIAONE`, `tri_b.md` => `# TRIBTWO`, `tri_c.md` =>
// `# TRICTHREE`). The OCR assertion is LOOSE (contains ALL THREE: `triaone` AND
// `tribtwo` AND `trictthree`). The load-bearing one is `tribtwo` — the right pane
// B that today's bug blanks.
//
// NOTE to coder: this FAILS today (B collapses to ~0 width / gray) and PASSES after
// the n-ary frame redesign + divider (install_paned_position) fix. The chords
// (`<C-w>v`, `<C-w>h`) and `:e <file><CR>` exactly mirror the vsplit/focus smokes.
//
// NOTE to coder (OCR width): three side-by-side panes split the editor width into
// thirds, so 28pt glyphs in each ~third-width pane may be too narrow for tesseract
// to read all three headings. If this OCR fails on WIDTH alone (headings clipped/
// illegible) — NOT on the real B-collapse bug — the coder may launch THIS test with
// a wider window and/or a smaller WOLF_FONT_PT so all three headings fit. Keep the
// assertion loose (contains). Do NOT pre-change app code for this.

/// Three-pane smoke: file opened FIRST (becomes the left pane).
const TRI_FILE_A: &str = "tri_a.md";

/// Heading written into [`TRI_FILE_A`]; lowercased it contains [`TRI_OCR_A`].
const TRI_HEADING_A: &str = "# TRIAONE";

/// Lowercased substring OCR must find for the FIRST/left pane's file.
const TRI_OCR_A: &str = "triaone";

/// Three-pane smoke: file opened in the RIGHT pane after the first `<C-w>v`. This
/// is the PRE-EXISTING right pane the 3-pane bug blanks (load-bearing assertion).
const TRI_FILE_B: &str = "tri_b.md";

/// Heading written into [`TRI_FILE_B`]; lowercased it contains [`TRI_OCR_B`].
const TRI_HEADING_B: &str = "# TRIBTWO";

/// Lowercased substring OCR must find for the RIGHT pane's file (the one the bug
/// collapses to ~0 width today).
const TRI_OCR_B: &str = "tribtwo";

/// Three-pane smoke: file opened in the new sub-pane after focusing left and the
/// second `<C-w>v` (the flat row becomes [A, C, B]).
const TRI_FILE_C: &str = "tri_c.md";

/// Heading written into [`TRI_FILE_C`]; lowercased it contains [`TRI_OCR_C`].
const TRI_HEADING_C: &str = "# TRICTHREE";

/// Lowercased substring OCR must find for the third (middle) pane's file.
const TRI_OCR_C: &str = "tricthree";

/// External binaries the scenario shells out to. All come from the flake's
/// smoke toolchain; absence means the devShell was not entered.
const REQUIRED_TOOLS: [&str; 4] = ["cage", "wtype", "grim", "tesseract"];

/// Confirms `tool` resolves on `PATH` (via `command -v`). Returns true if found.
fn tool_on_path(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Panics with an actionable message unless every smoke precondition holds:
/// the four compositor/OCR tools are on `PATH`, `WOLF_NVIM_BIN` is set, and
/// `XDG_RUNTIME_DIR` is set and writable. Returns the resolved `XDG_RUNTIME_DIR`.
fn require_smoke_env() -> PathBuf {
    let missing: Vec<&str> = REQUIRED_TOOLS
        .iter()
        .copied()
        .filter(|t| !tool_on_path(t))
        .collect();
    assert!(
        missing.is_empty(),
        "wolf-gnome headless smoke test: missing required tool(s) {missing:?} on PATH. \
         These are provided by the flake devShell smoke toolchain (cage, wtype, grim, \
         tesseract). Run via `nix develop --command cargo test -p wolf-gnome -- --ignored`."
    );

    match std::env::var("WOLF_NVIM_BIN") {
        Ok(v) if !v.trim().is_empty() => {}
        _ => panic!(
            "wolf-gnome headless smoke test: WOLF_NVIM_BIN is not set (or empty). It must \
             point at the pinned Neovim binary exported by the flake devShell; nvim is NOT \
             taken from PATH."
        ),
    }

    let runtime_dir = match std::env::var("XDG_RUNTIME_DIR") {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => panic!(
            "wolf-gnome headless smoke test: XDG_RUNTIME_DIR is not set (or empty). The \
             headless Wayland socket lives there; the flake devShell smoke env should export \
             it (defaulting to a mktemp dir if unset)."
        ),
    };
    let meta = std::fs::metadata(&runtime_dir).unwrap_or_else(|e| {
        panic!(
            "wolf-gnome headless smoke test: XDG_RUNTIME_DIR {} is not accessible: {e}",
            runtime_dir.display()
        )
    });
    assert!(
        meta.is_dir() && !meta.permissions().readonly(),
        "wolf-gnome headless smoke test: XDG_RUNTIME_DIR {} must be a writable directory.",
        runtime_dir.display()
    );

    runtime_dir
}

/// The whole interactive scenario, run as a single `sh -c` string INSIDE `cage`
/// so that `wtype` and `grim` inherit cage's `WAYLAND_DISPLAY` (cage exports it
/// only for its child process tree).
///
/// Design notes:
/// - Launch the app in the background and capture its pid so we can kill it.
/// - Poll (NOT a fixed sleep) for the window to map + nvim to settle, bounded by
///   `WAIT_TRIES * WAIT_INTERVAL` so a stuck launch fails fast instead of hanging.
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
const SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
WAIT_TRIES=25
WAIT_INTERVAL=0.2
SETTLE=0.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable.
# We poll by attempting cheap no-op wtype invocations until one succeeds, which
# only happens once cage has a mapped surface to deliver input to.
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Give nvim a final moment to finish its initial redraw before typing.
sleep "$SETTLE"

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

/// The interactive PICKER scenario, run as a single `sh -c` string INSIDE `cage`
/// (same structure as [`SCENARIO`]): launch the app, wait for the window to map,
/// open the picker with the Ctrl+Shift+P trigger, type a query that matches the
/// target file, activate the selection with Enter, settle, and screenshot.
///
/// The app derives `CoreConfig.cwd` from `std::env::current_dir()`, so the target
/// `.md` is placed in the cage process's working directory (set via
/// `Command::current_dir` by the caller) — NOT created from inside this script.
const PICKER_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
WAIT_TRIES=25
WAIT_INTERVAL=0.2
SETTLE=0.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable
# (cheap no-op wtype succeeds only once cage has a mapped surface).
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome picker smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle for nvim's initial redraw before driving the picker.
sleep "$SETTLE"

# Open the picker with the trigger chord Ctrl+Shift+P.
#
# NOTE to coder: verify this wtype modifier invocation against the installed
# wtype build. `-M <mod>` / `-m <mod>` press / release a modifier, and `-k <key>`
# presses+releases a key while the held modifiers apply, so the chord is
# "hold ctrl, hold shift, tap p, release shift, release ctrl". If your wtype
# needs different syntax (e.g. explicit `-P p` / `-p p` for press/release of the
# letter, or a `--` separator), adjust this single line — the rest of the
# scenario does not depend on how the chord is delivered.
wtype -M ctrl -M shift -k p -m shift -m ctrl

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

/// The interactive picker KEYBOARD-NAVIGATION scenario, run as a single `sh -c`
/// string INSIDE `cage` (same structure as [`PICKER_SCENARIO`]): launch the app,
/// wait for the window to map, open the picker with Ctrl+Shift+P, type a query
/// that matches BOTH navigation files, press `Down` once to move the list
/// selection to the SECOND row, then `Enter` to open it, settle, and screenshot.
///
/// Two files are created in the cage process's working directory by the caller
/// (`pick1.md` => row 0, `pick2.md` => row 1; see the rank-tiebreak comment near
/// the `PICKER_NAV_*` consts). Down+Enter must therefore open `pick2.md`.
const PICKER_NAV_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
WAIT_TRIES=25
WAIT_INTERVAL=0.2
SETTLE=0.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable
# (cheap no-op wtype succeeds only once cage has a mapped surface).
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome picker nav smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle for nvim's initial redraw before driving the picker.
sleep "$SETTLE"

# Open the picker with the trigger chord Ctrl+Shift+P (same chord as the other
# picker scenario; see its NOTE to coder for the wtype modifier syntax).
wtype -M ctrl -M shift -k p -m shift -m ctrl

# Let the picker map + grab keyboard focus on its search entry.
sleep "$SETTLE"

# Type the shared query so the ranked list shows BOTH matching rows.
wtype -- "$PICKER_NAV_QUERY"

# Let the streamed results rank/settle into the two-row list.
sleep "$SETTLE"

# Move the LIST selection down one row (row 0 -> row 1).
#
# NOTE to coder: this assumes `Down` moves the ListBox selection, NOT the
# SearchEntry text cursor, and that the keyval name `Down` is what wtype emits
# for the arrow key. If the picker leaves focus on the entry and Down does not
# reach the list, the selection will not move and this test's assertion will
# fail. Verify the picker routes Up/Down to the list.
wtype -k Down

# Brief settle so the selection change is processed before activating.
sleep "$SETTLE"

# Activate the SELECTED row: sends InputEvent::Open{path} for row 1 and closes
# the picker, which makes nvim `:e` the second file.
#
# NOTE to coder: `Return` is assumed to be wtype's key name for Enter (matches
# the other picker scenario). Adjust if your wtype build differs.
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

/// The TREE-SIDEBAR scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`SCENARIO`]): launch the app, wait for the window to map, then
/// wait a little longer for the long-lived FileSearch to stream results into the
/// tree, and screenshot. No keyboard interaction at all — we only assert the
/// sidebar lists the file by name, which depends purely on tree population from
/// the app's cwd (set via `Command::current_dir` by the caller).
const TREE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
WAIT_TRIES=25
WAIT_INTERVAL=0.2
SETTLE=0.5
# Extra settle for the streamed FileSearch traversal to populate the tree.
TREE_SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable
# (cheap no-op wtype succeeds only once cage has a mapped surface).
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome tree smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle for nvim's initial redraw, plus extra time for the tree's
# FileSearch to stream the cwd listing into the sidebar.
sleep "$SETTLE"
sleep "$TREE_SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The VERTICAL-SPLIT scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`SCENARIO`]): launch the app, wait for the window to map, open
/// file A (`:e $VSPLIT_FILE_LEFT<CR>`) in the editor pane, trigger a vertical split
/// with `<C-w>v`, open file B (`:e $VSPLIT_FILE_RIGHT<CR>`) in the new pane, settle,
/// and screenshot. Both files live in the cage process's working directory (set via
/// `Command::current_dir` by the caller) — NOT created from inside this script.
const VSPLIT_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
WAIT_TRIES=25
WAIT_INTERVAL=0.2
SETTLE=0.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable
# (cheap no-op wtype succeeds only once cage has a mapped surface).
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome vsplit smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle for nvim's initial redraw before driving the editor.
sleep "$SETTLE"

# Open file A in the (single) editor pane via an Ex command. The ':' enters the
# cmdline (rendered as the S3 bottom strip on grid 1); Return executes the :e.
wtype -- ":e $VSPLIT_FILE_LEFT"
wtype -k Return

# Let nvim load file A and redraw before splitting.
sleep "$SETTLE"

# Trigger a VERTICAL split with the <C-w>v chord (see the NOTE to coder near the
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

/// The HORIZONTAL-SPLIT scenario, run as a single `sh -c` string INSIDE `cage`
/// (same structure as [`VSPLIT_SCENARIO`]): launch the app, wait for the window to
/// map, open file A (`:e $HSPLIT_FILE_TOP<CR>`) in the editor pane, trigger a
/// HORIZONTAL split with `<C-w>s`, open file B (`:e $HSPLIT_FILE_BOTTOM<CR>`) in the
/// new (stacked) pane, settle, and screenshot. Both files live in the cage
/// process's working directory (set via `Command::current_dir` by the caller).
const HSPLIT_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
WAIT_TRIES=25
WAIT_INTERVAL=0.2
SETTLE=0.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable
# (cheap no-op wtype succeeds only once cage has a mapped surface).
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome hsplit smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle for nvim's initial redraw before driving the editor.
sleep "$SETTLE"

# Open file A in the (single) editor pane via an Ex command. The ':' enters the
# cmdline (rendered as the S3 bottom strip on grid 1); Return executes the :e.
wtype -- ":e $HSPLIT_FILE_TOP"
wtype -k Return

# Let nvim load file A and redraw before splitting.
sleep "$SETTLE"

# Trigger a HORIZONTAL split with the <C-w>s chord (see the NOTE to coder near the
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
/// structure as [`VSPLIT_SCENARIO`]): launch the app, wait for the window to map,
/// open file A in the editor pane, vertical-split (`<C-w>v`, focus moves to the new
/// RIGHT pane), open file B there, move focus back LEFT (`<C-w>h`), then type the
/// marker (`i$FOCUS_MARKER<Esc>`) into the now-focused pane, settle, and screenshot.
/// Both files live in the cage process's working directory (set by the caller).
const FOCUS_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
WAIT_TRIES=25
WAIT_INTERVAL=0.2
SETTLE=0.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable
# (cheap no-op wtype succeeds only once cage has a mapped surface).
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome focus smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle for nvim's initial redraw before driving the editor.
sleep "$SETTLE"

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
# shell neighbor -> nvim_set_current_win). See the NOTE to coder near FOCUS_* consts.
wtype -M ctrl -k w -m ctrl
wtype -k h

# Let the focus change land (intent drained on wake, focus ring + routing updated).
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
/// structure as [`VSPLIT_SCENARIO`]): launch the app, wait for the window to map,
/// open file A in the editor pane, vertical-split (`<C-w>v`, focus moves to the new
/// RIGHT pane), open file B there, then close the focused (right) pane with
/// `<C-w>c`, settle, and screenshot. Both files live in the cage process's working
/// directory (set by the caller).
const CLOSE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
WAIT_TRIES=25
WAIT_INTERVAL=0.2
SETTLE=0.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable
# (cheap no-op wtype succeeds only once cage has a mapped surface).
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome close smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle for nvim's initial redraw before driving the editor.
sleep "$SETTLE"

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
/// structure as [`VSPLIT_SCENARIO`]): launch the app, wait for the window to map,
/// open file A, vertical-split (`<C-w>v`) and open file B in the new RIGHT pane,
/// focus back LEFT (`<C-w>h`), vertical-split AGAIN and open file C in the new
/// sub-pane — flattening into one row of three columns [A, C, B] under the n-ary
/// model. Park the cursor off the text (`o`+Escape) and screenshot. All three
/// files live in the cage process's working directory (set by the caller).
const TRISPLIT_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
WAIT_TRIES=25
WAIT_INTERVAL=0.2
SETTLE=0.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable
# (cheap no-op wtype succeeds only once cage has a mapped surface).
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome three-pane smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle for nvim's initial redraw before driving the editor.
sleep "$SETTLE"

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

/// Creates a fresh, unique temp directory to serve as the launched app's working
/// directory. The app derives `CoreConfig.cwd` from `std::env::current_dir()`, so
/// the picker traverses this directory (the recent-files MRU lives in the global
/// `state_dir()/file-mru.sqlite`, not under the cwd). Not
/// auto-deleted: keeping it around aids post-mortem inspection of a failed run,
/// mirroring the wolf-core/common `fresh_cwd` rationale.
fn make_temp_cwd() -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is before the unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("wolf-gnome-picker-smoke-{pid}-{nanos}"));
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|e| panic!("failed to create temp cwd {}: {e}", dir.display()));
    dir
}

#[test]
#[ignore = "headless-compositor smoke test; run explicitly with --ignored (needs flake smoke toolchain)"]
fn window_renders_typed_text_under_cage() {
    // Serialize against the other cage test (shared Wayland socket); recover from
    // poisoning so a failure in the sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test (Cargo sets this env var for
    // integration tests of a crate that declares a `[[bin]]`).
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

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

    // Run the WHOLE scenario inside cage. cage sets WAYLAND_DISPLAY for the child
    // tree, so wtype/grim inside the scenario see the headless display. We inherit
    // the parent environment (WOLF_NVIM_BIN, WLR_BACKENDS, XDG_RUNTIME_DIR,
    // FONTCONFIG_FILE, ...) and add the scenario-specific vars.
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(SCENARIO)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("OCR_WORD", OCR_WORD)
        // Large font so the rendered glyphs are legible to tesseract.
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
        .status()
        .expect("failed to spawn `cage` for the headless smoke scenario");
    assert!(
        status.success(),
        "cage scenario exited unsuccessfully ({status}); the app may have failed to launch, \
         the window may not have mapped, or wtype/grim failed. Screenshot expected at {}.",
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

    // OCR the screenshot and assert the full typed word rendered. The scenario
    // parks the block cursor on the empty row 1 (via `o`+Escape) so it no longer
    // sits atop any letter of OCR_WORD on row 0, making the whole word legible to
    // OCR rather than just a cursor-free prefix.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(OCR_WORD),
        "OCR of the rendered window did not contain the typed word {OCR_WORD:?}. The cursor was \
         moved off row 0 so it should not obscure any letter; a miss here usually means no glyphs \
         rendered. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
#[ignore = "headless-compositor smoke test; run explicitly with --ignored (needs flake smoke toolchain)"]
fn picker_opens_and_loads_selected_file_under_cage() {
    // Serialize against the other cage test (shared Wayland socket); recover from
    // poisoning so a failure in the sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop the known markdown target into it. The app
    // reads `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with
    // this directory as its working directory (see `.current_dir` below).
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
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(PICKER_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("PICKER_QUERY", PICKER_QUERY)
        // Large font so the rendered glyphs are legible to tesseract.
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot and assert (loosely) that the target file's heading is
    // present — proving the picker opened, matched `PICKER_QUERY`, selected the
    // entry, and nvim loaded the file.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(PICKER_OCR),
        "OCR of the rendered window did not contain {PICKER_OCR:?} (loose match). This usually \
         means the picker did not open the target file. PNG kept at {} for inspection. OCR \
         output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
#[ignore = "headless-compositor smoke test; run explicitly with --ignored (needs flake smoke toolchain)"]
fn picker_arrow_navigation_selects_second_match_under_cage() {
    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop BOTH known markdown targets into it. The
    // app reads `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage
    // with this directory as its working directory (see `.current_dir` below).
    // Neither file is in the MRU cache, so the picker's row order is determined
    // purely by the rank tiebreaks: `pick1.md` is row 0, `pick2.md` is row 1.
    let cwd = make_temp_cwd();
    let target_a = cwd.join(PICKER_NAV_FILE_A);
    let target_b = cwd.join(PICKER_NAV_FILE_B);
    std::fs::write(&target_a, format!("{PICKER_NAV_HEADING_A}\n"))
        .unwrap_or_else(|e| panic!("failed to write picker nav file {}: {e}", target_a.display()));
    std::fs::write(&target_b, format!("{PICKER_NAV_HEADING_B}\n"))
        .unwrap_or_else(|e| panic!("failed to write picker nav file {}: {e}", target_b.display()));

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
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(PICKER_NAV_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("PICKER_NAV_QUERY", PICKER_NAV_QUERY)
        // Large font so the rendered glyphs are legible to tesseract.
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot. The selection started on row 0 (`pick1.md`), `Down`
    // moved it to row 1 (`pick2.md`), and `Enter` opened that row — so nvim must
    // have loaded `pick2.md`, whose heading lowercases to contain `picktwo`.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(PICKER_NAV_OCR_PRESENT),
        "OCR did not contain {PICKER_NAV_OCR_PRESENT:?} (loose match). Expected `Down` to select \
         row 1 ({PICKER_NAV_FILE_B}) and `Enter` to open it. A miss usually means Down did not \
         move the list selection (or the rank order differed). PNG kept at {} for inspection. \
         OCR output was:\n{ocr_text}",
        out_png.display()
    );
    // Prove the selection actually moved OFF the top row: row 0's heading
    // (`pickone`) must not be the opened file. Loose, but a strong signal that
    // navigation occurred rather than Enter just opening row 0.
    assert!(
        !ocr_text.contains(PICKER_NAV_OCR_ABSENT),
        "OCR unexpectedly contained {PICKER_NAV_OCR_ABSENT:?}: it looks like row 0 ({PICKER_NAV_FILE_A}) \
         was opened, i.e. `Down` did NOT move the selection off the top row. PNG kept at {} for \
         inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
#[ignore = "headless-compositor smoke test; run explicitly with --ignored (needs flake smoke toolchain)"]
fn tree_sidebar_shows_files_under_cage() {
    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop the distinctively-named markdown file into
    // it. The app reads `std::env::current_dir()` for `CoreConfig.cwd`, so we
    // launch cage with this directory as its working directory (see `.current_dir`
    // below); the long-lived FileSearch then populates the tree from this cwd.
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
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(TREE_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        // Large font so the rendered glyphs are legible to tesseract.
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot and assert (loosely) that the file's NAME rendered in the
    // sidebar — proving the long-lived FileSearch populated the tree projection
    // and the ListView drew the row. This verifies tree POPULATION only (same
    // mechanism as the picker); highlight/reveal and single-click-open are covered
    // by S1/S2 + `rel_path_for` pure tests and manual checking (see the NOTE to
    // coder near the TREE_* consts).
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(TREE_OCR),
        "OCR of the rendered window did not contain {TREE_OCR:?} (loose match). This usually means \
         the tree sidebar did not populate from the cwd FileSearch (or the Paned layout hid/clipped \
         the tree pane). PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
#[ignore = "headless-compositor smoke test; run explicitly with --ignored (needs flake smoke toolchain)"]
fn vertical_split_shows_two_panes_under_cage() {
    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop BOTH markdown targets into it. The app reads
    // `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with this
    // directory as its working directory (see `.current_dir` below); the scenario
    // opens file A in the first pane and file B in the new (split) pane by name.
    let cwd = make_temp_cwd();
    let target_left = cwd.join(VSPLIT_FILE_LEFT);
    let target_right = cwd.join(VSPLIT_FILE_RIGHT);
    std::fs::write(&target_left, format!("{VSPLIT_HEADING_LEFT}\n"))
        .unwrap_or_else(|e| panic!("failed to write vsplit left file {}: {e}", target_left.display()));
    std::fs::write(&target_right, format!("{VSPLIT_HEADING_RIGHT}\n"))
        .unwrap_or_else(|e| {
            panic!("failed to write vsplit right file {}: {e}", target_right.display())
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

    // Run the whole vertical-split scenario inside cage, with the temp dir as the
    // working directory so the app picks it up as its cwd.
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(VSPLIT_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("VSPLIT_FILE_LEFT", VSPLIT_FILE_LEFT)
        .env("VSPLIT_FILE_RIGHT", VSPLIT_FILE_RIGHT)
        // Large font so the rendered glyphs are legible to tesseract. (See the NOTE
        // to coder near the VSPLIT_* consts: if two side-by-side panes are too
        // narrow for 28pt OCR, widen the window or lower the font for THIS test.)
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot and assert (loosely) that BOTH files' headings render at
    // once — proving the vertical split produced a second pane bound to the new
    // nvim window/grid and painted it beside the first.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(VSPLIT_OCR_LEFT),
        "OCR did not contain {VSPLIT_OCR_LEFT:?} (the LEFT pane's file). A miss usually means the \
         first pane stopped rendering after the split. PNG kept at {} for inspection. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        ocr_text.contains(VSPLIT_OCR_RIGHT),
        "OCR did not contain {VSPLIT_OCR_RIGHT:?} (the RIGHT pane's file). A miss usually means the \
         <C-w>v split did not create a second pane, or :e did not open file B in it. PNG kept at {} \
         for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
#[ignore = "headless-compositor smoke test; run explicitly with --ignored (needs flake smoke toolchain)"]
fn horizontal_split_shows_two_panes_under_cage() {
    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop BOTH markdown targets into it. The app reads
    // `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with this
    // directory as its working directory (see `.current_dir` below); the scenario
    // opens file A in the first pane and file B in the new (stacked) pane by name.
    let cwd = make_temp_cwd();
    let target_top = cwd.join(HSPLIT_FILE_TOP);
    let target_bottom = cwd.join(HSPLIT_FILE_BOTTOM);
    std::fs::write(&target_top, format!("{HSPLIT_HEADING_TOP}\n"))
        .unwrap_or_else(|e| panic!("failed to write hsplit top file {}: {e}", target_top.display()));
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

    // Run the whole horizontal-split scenario inside cage, with the temp dir as the
    // working directory so the app picks it up as its cwd.
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(HSPLIT_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("HSPLIT_FILE_TOP", HSPLIT_FILE_TOP)
        .env("HSPLIT_FILE_BOTTOM", HSPLIT_FILE_BOTTOM)
        // Large font so the rendered glyphs are legible to tesseract. (See the NOTE
        // to coder near the HSPLIT_* consts: if two stacked panes are too short for
        // 28pt OCR, make the window taller or lower the font for THIS test.)
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot and assert (loosely) that BOTH files' headings render at
    // once — proving the horizontal split produced a second pane bound to the new
    // nvim window/grid and painted it stacked with the first.
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
#[ignore = "headless-compositor smoke test; run explicitly with --ignored (needs flake smoke toolchain)"]
fn focus_move_returns_to_first_pane_under_cage() {
    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop BOTH markdown targets into it. The app reads
    // `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with this
    // directory as its working directory (see `.current_dir` below).
    let cwd = make_temp_cwd();
    let target_left = cwd.join(FOCUS_FILE_LEFT);
    let target_right = cwd.join(FOCUS_FILE_RIGHT);
    std::fs::write(&target_left, format!("{FOCUS_HEADING_LEFT}\n"))
        .unwrap_or_else(|e| panic!("failed to write focus left file {}: {e}", target_left.display()));
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
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(FOCUS_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("FOCUS_FILE_LEFT", FOCUS_FILE_LEFT)
        .env("FOCUS_FILE_RIGHT", FOCUS_FILE_RIGHT)
        .env("FOCUS_MARKER", FOCUS_MARKER)
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot. PRIMARY assertions: the marker was inserted (so the
    // post-<C-w>h insert reached the focused pane) AND the split persisted (both
    // file headings still render).
    //
    // NOTE to coder: this proves the focus-move PLUMBING end to end but does NOT
    // pixel-verify WHICH pane received the marker (positional OCR is too fragile).
    // The pure `layout::neighbor` unit tests cover which pane is the left neighbor.
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
#[ignore = "headless-compositor smoke test; run explicitly with --ignored (needs flake smoke toolchain)"]
fn close_pane_leaves_one_under_cage() {
    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop BOTH markdown targets into it. The app reads
    // `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with this
    // directory as its working directory (see `.current_dir` below).
    let cwd = make_temp_cwd();
    let target_keep = cwd.join(CLOSE_FILE_KEEP);
    let target_gone = cwd.join(CLOSE_FILE_GONE);
    std::fs::write(&target_keep, format!("{CLOSE_HEADING_KEEP}\n"))
        .unwrap_or_else(|e| panic!("failed to write close keep file {}: {e}", target_keep.display()));
    std::fs::write(&target_gone, format!("{CLOSE_HEADING_GONE}\n"))
        .unwrap_or_else(|e| panic!("failed to write close gone file {}: {e}", target_gone.display()));

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
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(CLOSE_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("CLOSE_FILE_KEEP", CLOSE_FILE_KEEP)
        .env("CLOSE_FILE_GONE", CLOSE_FILE_GONE)
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot. The survivor (left) pane's file must render, and the
    // closed (right) pane's file must NOT — proving <C-w>c removed the focused pane
    // and the survivor reclaimed the width.
    //
    // NOTE to coder: if nvim leaves the closed buffer listed elsewhere or OCR noise
    // makes `closegone` linger, relax to ONLY the `closekeep` present assertion.
    // Try the stronger NOT-contains first.
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
#[ignore = "headless-compositor smoke test; run explicitly with --ignored (needs flake smoke toolchain)"]
fn three_panes_keep_all_files_visible_under_cage() {
    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop ALL THREE markdown targets into it. The app
    // reads `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with
    // this directory as its working directory (see `.current_dir` below).
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

    // Run the whole three-pane scenario inside cage, with the temp dir as the
    // working directory so the app picks it up as its cwd.
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(TRISPLIT_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("TRI_FILE_A", TRI_FILE_A)
        .env("TRI_FILE_B", TRI_FILE_B)
        .env("TRI_FILE_C", TRI_FILE_C)
        // Large font so the rendered glyphs are legible to tesseract. (See the NOTE
        // to coder near the TRI_* consts: if three side-by-side panes are too narrow
        // for 28pt OCR, widen the window or lower the font for THIS test.)
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot and assert (loosely) that ALL THREE files' headings render
    // at once — proving the third (same-orientation) split flattened into a row of
    // three panes WITHOUT collapsing the pre-existing right pane (B). The
    // load-bearing assertion is `tribtwo`: today's bug blanks the right pane B.
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

/// Runs `tesseract <png> stdout --psm 11` and returns its lowercased text output.
/// (`--psm 11` — "sparse text" — is needed because the default page-segmentation
/// mode returns empty on the sparse, light-on-dark editor grid.) Panics with the
/// captured stderr if tesseract fails to run or exits non-zero.
fn ocr_to_lowercase(png: &Path) -> String {
    let output = Command::new("tesseract")
        .arg(png)
        .arg("stdout")
        .arg("--psm")
        .arg("11")
        .output()
        .expect("failed to spawn `tesseract` for OCR");
    assert!(
        output.status.success(),
        "tesseract failed on {} ({}): {}",
        png.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_lowercase()
}
