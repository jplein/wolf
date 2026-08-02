//! Headless-compositor end-to-end smoke tests for the real `wolf-gnome` window.
//!
//! ## What it proves
//!
//! That the built `wolf-gnome` binary launches a real GTK/Adwaita window under a
//! headless Wayland compositor, attaches Neovim, accepts keyboard input, and
//! *actually renders glyphs* — verified by screenshotting the live window and
//! OCR-ing it. Unlike the Tier-1 seam tests (which exercise `gdk_key`/`paint`
//! offscreen), the cage smokes here are the only tests that confirm the whole stack
//! draws a window a human could see.
//!
//! ## What this file contains
//!
//! **Fourteen cage smokes** — one window-render smoke, two file-picker smokes, one
//! file-tree-sidebar smoke, five split-pane smokes, one empty-state smoke, one
//! new-untitled-buffer smoke, one Ctrl+Shift+W close-pane smoke (the accelerator
//! path, as opposed to the split-pane suite's `<C-w>c`) and two quit smokes (`:q`
//! closes the focused pane, `:qa` closes the window) — each of which launches the
//! real binary under `cage`. Thirteen of them OCR a screenshot of the live window;
//! the `:qa` smoke deliberately captures no image and asserts on the cage exit
//! status alone (the app must be GONE, and gone with a ZERO exit status — a panic
//! in the GTK main loop makes a process vanish just as a clean shutdown does).
//!
//! **One pure guard**, [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`],
//! which mechanically enforces the fixture invariants the smokes' OCR assertions
//! depend on. It is compositor-free: it does NOT call `require_smoke_env`, takes no
//! `CAGE_LOCK`, spawns no subprocess and needs none of the toolchain below, so it
//! still guards the fixture on a machine where the smoke toolchain is unavailable.
//! It lives here, next to the consts it reads, rather than in a file of its own.
//!
//! So this target's summary line reads **`15 passed`**, not `14`: fourteen cage
//! smokes plus that guard.
//!
//! ## When it runs
//!
//! The fourteen cage smokes run as part of the STANDARD suite — `cargo test -p
//! wolf-gnome` and `cargo test --workspace` — inside the flake devShell. They are
//! the Linux verification gate's only end-to-end proof that the GTK front-end draws
//! a window a human could see (see `build.md`, "The Linux verification gate").
//!
//! They are deliberately NOT `#[ignore]`d. "They are slow" is not a reason to skip
//! the only real end-to-end test the front-end has; per `CLAUDE.md` a test the
//! suite command does not run is a silently-skipped test.
//!
//! They are, however, the slowest part of the suite: each drives a real
//! compositor, and all fourteen serialize behind `CAGE_LOCK` (see below).
//!
//! ```text
//! nix develop --command cargo test -p wolf-gnome --test smoke_headless
//! ```
//!
//! ## Requirements of the fourteen cage smokes (from the devShell's smoke toolchain)
//!
//! - `cage` (headless wlroots compositor), `wtype`, `grim`, `tesseract` on `PATH`.
//! - `WOLF_NVIM_BIN` pointing at the pinned Neovim (same contract as the other
//!   wolf-gnome tests; nvim is never taken from `PATH`).
//! - `XDG_RUNTIME_DIR` set and writable (the Wayland socket lives there).
//!
//! Per project policy every one of the fourteen FAILS LOUD if any of the above is
//! missing rather than silently skipping, mirroring `common::require_nvim_bin` — a
//! missing precondition means the smoke toolchain was not provisioned, which is a
//! setup bug we want surfaced, not hidden. Now that these tests run by default, that
//! hard fail matters more, not less: a missing tool means the suite was invoked
//! outside the devShell, and softening `require_smoke_env` to a skip would let the
//! gate pass green on an unprovisioned machine while proving nothing. Do not.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The lowercase word we type in insert mode (in the window smoke scenario).
/// Kept short and all-lowercase so OCR noise is unlikely to corrupt every
/// character. After typing it we move the cursor OFF its row (see [`SCENARIO`])
/// so the full word — not just a prefix — can be asserted against OCR.
const OCR_WORD: &str = "hello";

/// Serializes the fourteen cage smoke tests. Each launches its own
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

/// How many tiny marker `.md` files [`window_renders_typed_text_under_cage`] drops
/// into its temp cwd before launching the app, purely so the tree sidebar has rows
/// of text to draw.
///
/// This is an OCR-robustness fixture, not a product requirement, and it weakens
/// nothing: the assertion, [`OCR_WORD`], [`SMOKE_FONT_PT`], the [`SCENARIO`] string
/// and its timing knobs are all untouched.
///
/// ## Why the sidebar has to carry text
///
/// With an EMPTY cwd the app renders correctly and the assertion still fails: the
/// capture shows row 0 reading `hello` with the block cursor exactly ONE row below
/// it, and an ASCII pixel dump of that region shows `h e l l o` with correct
/// ascenders and no clipping — yet `tesseract` reports `ﬁello`, misclassifying the
/// isolated leading `h` as the ligature U+FB01 (`ﬁ`) when the rest of the page is
/// nearly blank. The decisive control: cropping a PASSING capture (one taken with a
/// densely populated sidebar) down to just the editor region reproduces `ﬁello` from
/// the very same pixels. The `hello` glyphs are therefore byte-identical in the
/// passing and failing cases; the only variable is how much other text is on the page
/// for the classifier to calibrate against. `--psm 3`, `4`, `6`, `11` and `12` were
/// all tried and all fail on the sparse page, so this is not a page-segmentation-mode
/// question and must not be "fixed" by changing psm.
///
/// ## Why this number
///
/// Measured on this host: 5 files is the FLOOR at which OCR reads `hello` reliably
/// (1 file and 3 files both fail; 5 files passed three runs in a row, 12 files
/// passed). Nine sits comfortably above that floor without slowing the file-search
/// traversal. If this ever regresses, add files — do not touch the assertion.
///
/// ## Why it belongs here at all
///
/// Before the per-test temp cwd landed, this smoke ran with `crates/wolf-gnome` as
/// the app's cwd and the sidebar happened to list ~35 repo entries, so the assertion
/// has ALWAYS implicitly depended on that page density — accidentally. Writing these
/// files makes the dependency explicit and deliberate, and mirrors what the eleven
/// other smokes that drop fixture files into their own temp cwds already get
/// incidentally. (The empty-state and `:qa` smokes drop no files: the first hides
/// the sidebar entirely, and the second asserts on an exit status rather than OCR.)
const SIDEBAR_FILLER_COUNT: usize = 9;

/// Names of the [`SIDEBAR_FILLER_COUNT`] marker files. Only the FILENAME is ever
/// rendered (they are listed in the sidebar and never opened), so the names are
/// word-like and all-lowercase — the shapes `tesseract` reads best — and short
/// enough to fit the sidebar pane without being ellipsized.
///
/// The naming constraint is load-bearing. No name may contain [`OCR_WORD`]: a filler
/// row in the sidebar would then satisfy this smoke's assertion on its own, and the
/// test would pass green while the editor drew nothing — a silent false positive.
/// They are likewise kept clear of every other smoke's OCR constants — the asserted
/// SUBSTRINGS, not the fixture filenames: `pickertarget`, `pickone`/`picktwo`,
/// `treenode`, `paneleft`/`paneright`, `htopfile`/`hbotfile`,
/// `focusone`/`focustwo`/`zzleft`, `closekeep`/`closegone`,
/// `triaone`/`tribtwo`/`tricthree`, `folder`, `zzfresh`/`newbufbase`,
/// `quitkeep`/`quitgone` and `chordkeep`/`chordgone` — so a stray filler can never
/// satisfy one of those either. [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`]
/// enforces this mechanically against [`ASSERTED_OCR_TERMS`], which holds the consts
/// themselves; this list is a reader's summary and the test is the authority.
///
/// The NATO words `echo` and `hotel` are deliberately SKIPPED: they are the two in
/// the alphabet whose letters are closest to `hello`, and the whole point of this
/// fixture is an OCR classifier that already confuses letter shapes here.
const SIDEBAR_FILLER_FILES: [&str; SIDEBAR_FILLER_COUNT] = [
    "fillalfa.md",
    "fillbravo.md",
    "fillcharlie.md",
    "filldelta.md",
    "fillfoxtrot.md",
    "fillgolf.md",
    "fillindia.md",
    "filljuliett.md",
    "fillkilo.md",
];

/// Body written into every [`SIDEBAR_FILLER_FILES`] entry. Their CONTENTS are never
/// rendered — no scenario opens them — so this exists only so the files are not
/// zero-length.
const SIDEBAR_FILLER_BODY: &str = "# FILLER\n";

/// Picker smoke scenario: the markdown file dropped into the launched app's temp
/// working directory. Its name contains [`PICKER_QUERY`] so the typed query matches
/// it, and its HEADING — deliberately a different word — contains [`PICKER_OCR`] so
/// OCR can confirm nvim actually opened it.
///
/// ## Why the filename must NOT contain [`PICKER_OCR`]
///
/// It used to. This file was named `pickertarget.md` while [`PICKER_OCR`] was
/// `pickertarget`, so the tree sidebar's own row for the fixture satisfied the
/// smoke's positive assertion whether or not the picker ever opened — the editor
/// pane contributed nothing. That is not a theory: during the command-drain unit's
/// broken-accelerator run (GTK's keymap-aware accel matching never fired under
/// `wtype`, so Ctrl+Shift+O was forwarded to Neovim instead of opening the picker)
/// this smoke PASSED while the picker demonstrably never opened.
///
/// It is the exact failure mode
/// [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`] exists to prevent,
/// which that guard could not catch because it only inspects
/// [`SIDEBAR_FILLER_FILES`], not each smoke's own fixture. The fix follows the
/// precedent already set by [`CLOSE_FILE_GONE`] (`cgone.md` => `# CLOSEGONE`) and
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
const PICKER_OCR: &str = "pickertarget";

/// Text typed into the picker's search entry to filter down to the target file.
///
/// It must match [`PICKER_FILE_NAME`], which is the only file in that smoke's temp
/// cwd: as a prefix of `ptarget.md` it takes the ranker's path-component-prefix,
/// word-prefix and contiguous-substring bonuses, so the target is row 0 and plain
/// `Enter` opens it.
const PICKER_QUERY: &str = "ptarget";

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
// Note: this relies on (a) the rank tiebreaks resolving these two equal matches by
// display length then lexicographically (so `pick1.md` is row 0), and (b) `Down`
// moving the LIST/ListBox selection rather than the SearchEntry text cursor. Both
// hold in the current picker; if either ever stops holding, the `picktwo` OCR
// assertion is what catches it.

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
// Note: this asserts tree POPULATION only (the filename is visible in the
// sidebar). Single-click-to-open and the current-file highlight/reveal are NOT
// verified here — they are covered by S1 (current_file core tests), S2 +
// `rel_path_for` pure unit tests, and manual/visual checking, because OCR cannot
// reliably confirm a highlight color and pixel-accurate row clicking is
// deliberately avoided (too flaky).
//
// Note: the gtk::Paned + tree ListView sitting to the left of the editor must not
// crowd out the editor's own glyphs — `window_renders_typed_text_under_cage` and
// the `picker_*_under_cage` smokes still assert OCR_WORD / PICKER_OCR render in
// the right-hand pane, so any future change to the sidebar's width budget that
// starves the editor pane fails those three tests, not just this one.

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
// caller (`pleft.md` => `# PANELEFT`, `pright.md` => `# PANERIGHT`). The OCR
// assertion is LOOSE (contains BOTH `paneleft` AND `paneright`).
//
// Note (why the FILENAMES are not `paneleft.md` / `paneright.md`): they were, and
// that made this smoke unable to fail for the reason it exists. The tree sidebar
// renders every cwd file BY NAME, so rows reading `paneleft.md` and `paneright.md`
// satisfied both `contains` assertions on their own — the editor panes contributed
// nothing, and the test would have passed green with `<C-w>v` splitting nothing at
// all. This is the same defect that let `picker_opens_and_loads_selected_file_under_cage`
// pass while the picker demonstrably never opened (see the note on
// [`PICKER_FILE_NAME`] for that incident). The fix is the convention the sibling
// smokes already follow — `htop.md` => `# HTOPFILE`, `ckeep.md` => `# CLOSEKEEP` —
// put the asserted word in the file's CONTENT and give the file a name whose
// lowercase does not contain it, so only a rendered BUFFER can satisfy the
// assertion. Neither assertion nor either OCR term changed: this strengthens the
// test, it does not weaken it.
//
// Note (the `<C-w>v` chord): the scenario sends the window-command chord as
// `wtype -M ctrl -k w -m ctrl` (hold Ctrl, tap w, release Ctrl) followed by
// `wtype -k v`, which is what the pinned wtype delivers and what the init.lua
// keymap expects (S2 wired <C-w>v -> vsplit). The chord delivery is isolated to
// those two lines; nothing else in the scenario depends on how it is sent.
//
// Note (OCR width): both files are opened at WOLF_FONT_PT=28 (matching the other
// smokes). Two panes SIDE BY SIDE halve the per-pane width, so 28pt glyphs in each
// pane are near tesseract's legibility floor. If OCR ever misses on width alone
// (headings clipped/illegible) — NOT on a real rendering bug — a wider window
// and/or a smaller WOLF_FONT_PT for THIS test are the robustness knobs. The
// assertion stays loose (contains) either way.
//
// Note (cmdline visibility): this relies on the ★ S3 cmdline strip — the typed
// `:e <file><CR>` Ex-commands live on grid 1, which is rendered as the bottom
// command strip so the typed text takes effect and is visible. A failure where
// `:e` does not open the files points at the grid-1 strip or at nvim_input not
// reaching the cmdline.

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
const VSPLIT_OCR_LEFT: &str = "paneleft";

/// Vertical-split smoke: file opened in the SECOND (right) pane after `<C-w>v`.
///
/// Deliberately UNLIKE [`VSPLIT_OCR_RIGHT`], for the reason given on
/// [`VSPLIT_FILE_LEFT`].
const VSPLIT_FILE_RIGHT: &str = "pright.md";

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
// Note (the `<C-w>s` chord): the scenario sends the window-command chord as
// `wtype -M ctrl -k w -m ctrl` (hold Ctrl, tap w, release Ctrl) followed by
// `wtype -k s`, exactly mirroring the vertical-split smoke's `<C-w>v`, and matching
// what the init.lua keymap expects (S2 wired <C-w>s -> horizontal split). Chord
// delivery is isolated to those two lines.
//
// Note (OCR height): both files are opened at WOLF_FONT_PT=28 (matching the other
// smokes). Two panes STACKED halve the per-pane HEIGHT, so each pane has fewer rows
// for tesseract to work with. If OCR ever misses on height alone (a heading
// clipped/illegible) — NOT on a real rendering bug — a taller window and/or a
// smaller WOLF_FONT_PT for THIS test are the robustness knobs. The assertion stays
// loose (contains) either way.

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
// Note: this verifies the focus-move PLUMBING end to end (chord -> FocusDir intent
// -> shell neighbor -> nvim_set_current_win -> keys route to the refocused window).
// It deliberately does NOT pixel-verify WHICH pane received the marker — positional
// OCR (proving `zzleft` sits in the LEFT pane's column next to `focusone`) is too
// fragile to assert here. The pure `layout::neighbor` unit tests cover the geometry
// of which pane is the left neighbor; this smoke proves the wiring fires and the
// layout survives.

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
// `closegone` (the closed pane is gone). The NOT-contains assertion is load-bearing
// — it is the only thing that distinguishes "the pane closed" from "nothing
// happened" — and is well-founded: after `:e cgone.md` nvim's cmdline toast shows
// the FILENAME (`cgone.md`), whose lowercase does not contain the HEADING word
// `closegone`. If it ever fires, that is a finding to diagnose from the retained
// PNG, not an assertion to loosen.

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
// `tribtwo` AND `trictthree`). The load-bearing one is `tribtwo` — the right pane B
// that the pre-n-ary binary model blanked.
//
// This is the regression guard for the n-ary frame model, which HAS landed. Before
// it, the always-nest binary model collapsed the pre-existing right pane B to ~0
// width and blanked file B; `PaneTree::split`'s same-orientation flatten plus
// `pane_view::install_paned_position` (which re-applies the proportional divider on
// EVERY `max-position` notify, not one-shot) fixed it. This test must PASS. The
// chords (`<C-w>v`, `<C-w>h`) and `:e <file><CR>` exactly mirror the vsplit/focus
// smokes.
//
// Note (OCR width): three side-by-side panes split the editor width into thirds, so
// 28pt glyphs in each ~third-width pane can be too narrow for tesseract to read all
// three headings. If OCR misses on WIDTH alone (headings clipped/illegible) — NOT on
// a layout regression — the window size and/or WOLF_FONT_PT for THIS test are the
// robustness knobs to turn. Diagnose from the retained PNG first. The assertion is
// deliberately loose (contains) and must stay; `tribtwo` is the load-bearing one.

/// Three-pane smoke: file opened FIRST (becomes the left pane).
const TRI_FILE_A: &str = "tri_a.md";

/// Heading written into [`TRI_FILE_A`]; lowercased it contains [`TRI_OCR_A`].
const TRI_HEADING_A: &str = "# TRIAONE";

/// Lowercased substring OCR must find for the FIRST/left pane's file.
const TRI_OCR_A: &str = "triaone";

/// Three-pane smoke: file opened in the RIGHT pane after the first `<C-w>v`. This
/// is the PRE-EXISTING right pane the pre-n-ary binary model blanked (load-bearing
/// assertion).
const TRI_FILE_B: &str = "tri_b.md";

/// Heading written into [`TRI_FILE_B`]; lowercased it contains [`TRI_OCR_B`].
const TRI_HEADING_B: &str = "# TRIBTWO";

/// Lowercased substring OCR must find for the RIGHT pane's file (the one the
/// pre-n-ary binary model collapsed to ~0 width).
const TRI_OCR_B: &str = "tribtwo";

/// Three-pane smoke: file opened in the new sub-pane after focusing left and the
/// second `<C-w>v` (the flat row becomes [A, C, B]).
const TRI_FILE_C: &str = "tri_c.md";

/// Heading written into [`TRI_FILE_C`]; lowercased it contains [`TRI_OCR_C`].
const TRI_HEADING_C: &str = "# TRICTHREE";

/// Lowercased substring OCR must find for the third (middle) pane's file.
const TRI_OCR_C: &str = "tricthree";

// ---------------------------------------------------------------------------
// Empty-state smoke: no project open => the "Open Folder…" status page renders.
//
// This scenario proves the launch decision end to end. `workspace::launch_decision`
// returns `EmptyState` when the process cwd IS the home directory (the
// desktop-launcher shape), so the fixture simply points BOTH at the same fresh
// temp dir: `.current_dir(&cwd)` plus `.env("HOME", &cwd)`. That fires the
// production rule with no test-only env var, and keeps every `$HOME`-derived path
// (`state_dir`, the MRU sqlite, the swap dir) inside the temp dir too.
//
// No keyboard interaction at all: the `gtk::FileDialog` the button opens cannot be
// driven under `cage` (there is no portal backend) and is deliberately NOT asserted
// — this smoke covers the empty state's PRESENTATION.
//
// The scenario writes NO fixture files. The sidebar is hidden in the empty state,
// so sidebar filler density is irrelevant here; the asserted text is the
// `adw::StatusPage`, which is theme-font, dark-on-light and large — the opposite of
// the sparse light-on-dark editor grid that made `SIDEBAR_FILLER_COUNT` necessary.

/// Lowercased substring OCR must find to prove the empty state rendered.
///
/// Deliberately the shared word of all three strings the status page draws — its
/// title ("No folder open"), its description ("Open a folder to start editing.")
/// and its button ("Open Folder…") — so the assertion has three independent
/// chances to match rather than one.
const EMPTY_STATE_OCR: &str = "folder";

// ---------------------------------------------------------------------------
// New-untitled-buffer smoke: Ctrl+Shift+N replaces the open file with a fresh
// untitled Markdown buffer.
//
// This proves the accelerator -> gio action -> `InputEvent::NewBuffer` path (which
// does NOT go through the command queue: the accelerator activates the action
// directly). Open a known file, fire the chord, then type a distinctive marker and
// park the cursor off its row; the marker must render while the previous file's
// heading must NOT.
//
// The NOT-contains assertion is load-bearing — it is the only thing that
// distinguishes "a new buffer replaced the old one" from "nothing happened" — and
// it is well-founded for the same reason `close_pane_leaves_one_under_cage`'s is:
// after `:e nbase.md` nvim's cmdline toast shows the FILENAME (`nbase.md`), whose
// lowercase does not contain the HEADING word `newbufbase`, and the sidebar row
// shows that same filename. The filename is deliberately UNLIKE the heading word.

/// New-buffer smoke: the file opened BEFORE the chord (the one that must vanish).
const NEWBUF_FILE: &str = "nbase.md";

/// Heading written into [`NEWBUF_FILE`]; lowercased it contains
/// [`NEWBUF_OCR_ABSENT`]. Deliberately unlike the filename so nvim's `:e` toast and
/// the sidebar row cannot satisfy the NOT-contains assertion.
const NEWBUF_HEADING: &str = "# NEWBUFBASE";

/// Lowercased substring OCR must NOT find: the heading of the file that was open
/// before Ctrl+Shift+N.
const NEWBUF_OCR_ABSENT: &str = "newbufbase";

/// Distinctive marker typed (in insert mode) into the NEW untitled buffer. Its
/// lowercased form is [`NEWBUF_OCR_PRESENT`]. Short/all-uppercase to survive OCR,
/// mirroring [`FOCUS_MARKER`].
const NEWBUF_MARKER: &str = "ZZFRESH";

/// Lowercased substring OCR MUST find: proof the new untitled buffer exists and
/// received the typed text.
const NEWBUF_OCR_PRESENT: &str = "zzfresh";

// ---------------------------------------------------------------------------
// `:q` smoke: the Ex command closes the focused PANE (not the window).
//
// `runtime/nvim/init.lua`'s `do_quit` closes the pane with `nvim_win_close` when
// more than one window is open, and the shell observes that through the `win_close`
// redraw event exactly as it does for `<C-w>c`. So this mirrors `CLOSE_SCENARIO`
// step for step and swaps only the closing chord for `:q<CR>`.
//
// Assertions: OCR contains `quitkeep` (the survivor renders) and does NOT contain
// `quitgone` (the closed pane is gone). Same soundness argument as the `<C-w>c`
// smoke: after `:e qgone.md` the cmdline toast shows the FILENAME, whose lowercase
// does not contain the HEADING word `quitgone`.

/// `:q` smoke: file opened in the FIRST (left) pane — the SURVIVOR.
const QUIT_FILE_KEEP: &str = "qkeep.md";

/// Heading written into [`QUIT_FILE_KEEP`]; lowercased it contains
/// [`QUIT_OCR_KEEP`].
const QUIT_HEADING_KEEP: &str = "# QUITKEEP";

/// Lowercased substring OCR MUST find: the survivor pane's file.
const QUIT_OCR_KEEP: &str = "quitkeep";

/// `:q` smoke: file opened in the SECOND (right) pane — the one `:q` closes.
const QUIT_FILE_GONE: &str = "qgone.md";

/// Heading written into [`QUIT_FILE_GONE`]; lowercased it contains
/// [`QUIT_OCR_GONE`].
const QUIT_HEADING_GONE: &str = "# QUITGONE";

/// Lowercased substring OCR must NOT find: the closed pane's file.
const QUIT_OCR_GONE: &str = "quitgone";

// ---------------------------------------------------------------------------
// Ctrl+Shift+W close-pane smoke: the ACCELERATOR closes the focused pane.
//
// This is the only end-to-end coverage of `UiAction::ClosePane`. The two sibling
// close smokes exercise different halves of the stack and neither touches this
// path: `close_pane_leaves_one_under_cage` sends `<C-w>c`, and
// `colon_q_closes_focused_pane_under_cage` sends `:q` — both are performed by
// NEOVIM and merely observed by the shell through the `win_close` redraw event.
// The chord instead travels the FRONT-END path: the capture-phase
// `EventControllerKey` -> `actions::action_for_key` -> `win.close-pane` ->
// `UiAction::ClosePane` -> `InputEvent::ClosePane` -> nvim's `:close`. Only the
// last step is shared, so a break anywhere in the action namespace, the chord
// matcher or the shell's dispatch shows up here and nowhere else.
//
// It otherwise mirrors `CLOSE_SCENARIO` step for step and swaps only the closing
// chord: open file A in the (single) pane, vertical-split (`<C-w>v`, which focuses
// the new RIGHT pane), open file B there, then fire Ctrl+Shift+W.
//
// Assertions: OCR contains `chordkeep` (the survivor renders) and does NOT contain
// `chordgone` (the closed pane is gone). The NOT-contains assertion is
// load-bearing — it is the only thing separating "the pane closed" from "the chord
// did nothing" — and it is sound for the same reason the sibling smokes' are: the
// FILENAMES (`wkeep.md` / `wgone.md`) deliberately share no word with the HEADINGS,
// so neither the sidebar rows nor nvim's `:e` toast can satisfy either assertion.
// Its own fixture names and OCR terms are used rather than the `:q` smoke's, so a
// failure names exactly one smoke.

/// Ctrl+Shift+W smoke: file opened in the FIRST (left) pane — the SURVIVOR.
const CHORD_CLOSE_FILE_KEEP: &str = "wkeep.md";

/// Heading written into [`CHORD_CLOSE_FILE_KEEP`]; lowercased it contains
/// [`CHORD_CLOSE_OCR_KEEP`].
const CHORD_CLOSE_HEADING_KEEP: &str = "# CHORDKEEP";

/// Lowercased substring OCR MUST find: the survivor pane's file.
const CHORD_CLOSE_OCR_KEEP: &str = "chordkeep";

/// Ctrl+Shift+W smoke: file opened in the SECOND (right) pane — the one the chord
/// closes.
const CHORD_CLOSE_FILE_GONE: &str = "wgone.md";

/// Heading written into [`CHORD_CLOSE_FILE_GONE`]; lowercased it contains
/// [`CHORD_CLOSE_OCR_GONE`].
const CHORD_CLOSE_HEADING_GONE: &str = "# CHORDGONE";

/// Lowercased substring OCR must NOT find: the closed pane's file.
const CHORD_CLOSE_OCR_GONE: &str = "chordgone";

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
         tesseract). Run via `nix develop --command cargo test -p wolf-gnome`."
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
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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
/// open the picker with the Ctrl+Shift+O trigger, type a query that matches the
/// target file, activate the selection with Enter, settle, and screenshot.
///
/// The app derives `CoreConfig.cwd` from `std::env::current_dir()`, so the target
/// `.md` is placed in the cage process's working directory (set via
/// `Command::current_dir` by the caller) — NOT created from inside this script.
const PICKER_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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

# Open the picker with the trigger chord Ctrl+Shift+O. That chord is the
# `win.go-to-file` accelerator; Ctrl+Shift+P is reserved for the command palette.
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

/// The interactive picker KEYBOARD-NAVIGATION scenario, run as a single `sh -c`
/// string INSIDE `cage` (same structure as [`PICKER_SCENARIO`]): launch the app,
/// wait for the window to map, open the picker with Ctrl+Shift+O, type a query
/// that matches BOTH navigation files, press `Down` once to move the list
/// selection to the SECOND row, then `Enter` to open it, settle, and screenshot.
///
/// Two files are created in the cage process's working directory by the caller
/// (`pick1.md` => row 0, `pick2.md` => row 1; see the rank-tiebreak comment near
/// the `PICKER_NAV_*` consts). Down+Enter must therefore open `pick2.md`.
const PICKER_NAV_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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

/// The TREE-SIDEBAR scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`SCENARIO`]): launch the app, wait for the window to map, then
/// wait a little longer for the long-lived FileSearch to stream results into the
/// tree, and screenshot. No keyboard interaction at all — we only assert the
/// sidebar lists the file by name, which depends purely on tree population from
/// the app's cwd (set via `Command::current_dir` by the caller).
const TREE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=0.5
# Extra settle for the streamed FileSearch traversal to populate the tree.
TREE_SETTLE=1.5

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
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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

/// The HORIZONTAL-SPLIT scenario, run as a single `sh -c` string INSIDE `cage`
/// (same structure as [`VSPLIT_SCENARIO`]): launch the app, wait for the window to
/// map, open file A (`:e $HSPLIT_FILE_TOP<CR>`) in the editor pane, trigger a
/// HORIZONTAL split with `<C-w>s`, open file B (`:e $HSPLIT_FILE_BOTTOM<CR>`) in the
/// new (stacked) pane, settle, and screenshot. Both files live in the cage
/// process's working directory (set via `Command::current_dir` by the caller).
const HSPLIT_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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
/// structure as [`VSPLIT_SCENARIO`]): launch the app, wait for the window to map,
/// open file A in the editor pane, vertical-split (`<C-w>v`, focus moves to the new
/// RIGHT pane), open file B there, move focus back LEFT (`<C-w>h`), then type the
/// marker (`i$FOCUS_MARKER<Esc>`) into the now-focused pane, settle, and screenshot.
/// Both files live in the cage process's working directory (set by the caller).
const FOCUS_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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
# shell neighbor -> nvim_set_current_win). See the note near the FOCUS_* consts.
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
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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

/// The EMPTY-STATE scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`TREE_SCENARIO`]): launch the app, wait for the window to map,
/// settle, and screenshot. No keyboard interaction at all — the assertion is purely
/// that the "no folder open" status page rendered.
///
/// The caller launches cage with `HOME` set to the same fresh temp directory it
/// uses as the working directory, which is what makes `workspace::launch_decision`
/// take its `cwd == home` branch (see the notes near [`EMPTY_STATE_OCR`]).
const EMPTY_STATE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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
        echo "wolf-gnome empty-state smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle so the status page (and the captive nvim's initial redraw behind it)
# have landed before capturing.
sleep "$SETTLE"

# Capture the live window to the output PNG.
grim "$OUT_PNG"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The NEW-UNTITLED-BUFFER scenario, run as a single `sh -c` string INSIDE `cage`
/// (same structure as [`VSPLIT_SCENARIO`]): launch the app, wait for the window to
/// map, open the base file (`:e $NEWBUF_FILE<CR>`), fire the Ctrl+Shift+N
/// accelerator, type the marker into the fresh buffer, park the cursor off the
/// marker's row, settle, and screenshot. The base file lives in the cage process's
/// working directory (set via `Command::current_dir` by the caller).
const NEWBUF_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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
        echo "wolf-gnome new-buffer smoke: app process exited early (pid $app_pid)" >&2
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

/// The `:q` CLOSE-PANE scenario, run as a single `sh -c` string INSIDE `cage`
/// (mirrors [`CLOSE_SCENARIO`] step for step): launch the app, wait for the window
/// to map, open file A in the editor pane, vertical-split (`<C-w>v`, focus moves to
/// the new RIGHT pane), open file B there, then close the focused pane with the Ex
/// command `:q` instead of `<C-w>c`, settle, and screenshot. Both files live in the
/// cage process's working directory (set by the caller).
const COLON_Q_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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
        echo "wolf-gnome :q smoke: app process exited early (pid $app_pid)" >&2
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
/// launch the app, wait for the window to map, settle, type `:qa<CR>`, then POLL
/// for the app process to disappear and REAP it to check how it went.
///
/// This is the only scenario that captures NO screenshot: its whole assertion is
/// that the process is gone and exited cleanly, which is a property of the process
/// table, not of any pixel. It therefore calls neither `grim` nor `tesseract`, and
/// contributes no [`ASSERTED_OCR_TERMS`] rows.
///
/// It asserts the app closed CLEANLY, not merely that it disappeared: after the
/// poll loop sees the process go, it `wait`s for the reaped child and checks the
/// status the shell retained. A panic or abort in the GTK main loop makes the
/// process vanish exactly as a clean shutdown does, so without that check this
/// scenario would pass green on the very regression its failure message blames.
///
/// Exit codes: `0` the app exited cleanly (success); `11` the app died before the
/// window mapped; `12` the app was still alive after `QUIT_TRIES * WAIT_INTERVAL`
/// seconds — i.e. the drained `close_window` command never reached
/// `window.close()`; `13` the app exited but with a NON-ZERO status — i.e. the
/// shutdown path itself panicked or aborted.
const COLON_QA_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=0.5
# How many WAIT_INTERVAL polls to allow for the app to EXIT after :qa. Sized like
# WAIT_TRIES because the shutdown path (close-request -> picker/tree close ->
# WolfCore::shutdown -> nvim exit) runs under the same parallel-suite load.
QUIT_TRIES=75

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable
# (cheap no-op wtype succeeds only once cage has a mapped surface).
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome :qa smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle for nvim's initial redraw before driving the cmdline.
sleep "$SETTLE"

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

/// The Ctrl+Shift+W CLOSE-PANE scenario, run as a single `sh -c` string INSIDE
/// `cage` (mirrors [`CLOSE_SCENARIO`] step for step): launch the app, wait for the
/// window to map, open file A in the editor pane, vertical-split (`<C-w>v`, focus
/// moves to the new RIGHT pane), open file B there, then close the focused pane
/// with the Ctrl+Shift+W ACCELERATOR instead of `<C-w>c`, settle, and screenshot.
/// Both files live in the cage process's working directory (set by the caller).
const CHORD_CLOSE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for the window to appear.
# WAIT_TRIES is sized for the FULL workspace suite, where ~90 integration-test
# binaries (most spawning a real nvim) run in parallel: under that load the window
# can take well over the old 5s budget to map. This is a timing knob only -- no
# assertion depends on it.
WAIT_TRIES=75
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
        echo "wolf-gnome chord-close smoke: app process exited early (pid $app_pid)" >&2
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

/// Creates a fresh, unique temp directory to serve as the launched app's working
/// directory. The app derives `CoreConfig.cwd` from `std::env::current_dir()`, so
/// this is the directory the file-search traversal walks and the directory the
/// captive nvim runs in (the recent-files MRU lives in the global
/// `state_dir()/file-mru.sqlite`, not under the cwd).
///
/// ALL FOURTEEN smokes use this. A fresh cwd per run is MANDATORY, not tidiness:
///
/// * The captive nvim keeps a DURABLE, per-project swap dir keyed on the cwd
///   (`wolf_core::state::swap_dir_for` → `state_dir()/swap/<hash of cwd>`), which
///   deliberately survives the process so a fresh nvim can recover a crashed
///   nvim's unsaved edits.
/// * Every scenario ends by `kill`ing the app, so its nvim never cleans up its
///   swap files.
/// * At a FIXED cwd those two facts compose into a self-poisoning test: run N
///   leaves a stale `%…%Untitled.md.swp` behind, and run N+1 finds it at startup.
///   Naming the startup buffer `<cwd>/Untitled.md` then collides with that swap
///   inside `nvim_buf_set_name`, which raises a BLOCKING ATTENTION dialog that
///   eats the first keystroke — so the smoke goes red for whatever change happens
///   to be in flight, and its own failure message points at the wrong cause. A
///   test whose fixture is its own previous run's leftovers is order-dependent and
///   non-idempotent.
///
/// The product-level guard for that collision is deliberate, headless and lives
/// elsewhere:
/// `crates/wolf-core/tests/swap_recovery.rs::stale_untitled_swap_does_not_block_a_fresh_start`.
/// Isolating the cwd here does not remove coverage; it moves it from accidental to
/// deliberate.
///
/// Not auto-deleted: keeping it around aids post-mortem inspection of a failed
/// run, mirroring the wolf-core/common `fresh_cwd` rationale. Each run therefore
/// also leaves one new `state_dir()/swap/<hash>/` holding at most one `.swp`;
/// harmless, because no future run resolves to that hash.
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

/// Every lowercased substring the OCR-asserting cage smokes must — or must not —
/// find, as `(term, const name, the smoke that asserts it)`.
///
/// The terms are the CONSTS themselves, never re-typed literals: a table of copies
/// could drift away from the values the smokes actually assert without anything
/// noticing, which would leave the check below quietly guarding nothing.
///
/// [`FOCUS_OCR_MARKER`] and [`NEWBUF_OCR_PRESENT`] are the lowercased forms of
/// [`FOCUS_MARKER`] and [`NEWBUF_MARKER`], so the typed markers are covered too.
/// The must-NOT-contain terms ([`PICKER_NAV_OCR_ABSENT`], [`CLOSE_OCR_GONE`],
/// [`NEWBUF_OCR_ABSENT`], [`QUIT_OCR_GONE`], [`CHORD_CLOSE_OCR_GONE`]) are listed
/// deliberately: a filler row carrying one of those would make its smoke fail
/// SPURIOUSLY rather than pass vacuously — the opposite symptom, but the same
/// fixture bug.
///
/// Thirteen of the fourteen smokes appear here.
/// [`colon_qa_closes_the_window_under_cage`] contributes NO rows on purpose: it
/// captures no PNG and asserts only that the app process is gone, so it has no OCR
/// term a fixture filename could satisfy.
const ASSERTED_OCR_TERMS: &[(&str, &str, &str)] = &[
    (OCR_WORD, "OCR_WORD", "window_renders_typed_text_under_cage"),
    (
        PICKER_OCR,
        "PICKER_OCR",
        "picker_opens_and_loads_selected_file_under_cage",
    ),
    (
        PICKER_NAV_OCR_PRESENT,
        "PICKER_NAV_OCR_PRESENT",
        "picker_arrow_navigation_selects_second_match_under_cage",
    ),
    (
        PICKER_NAV_OCR_ABSENT,
        "PICKER_NAV_OCR_ABSENT",
        "picker_arrow_navigation_selects_second_match_under_cage",
    ),
    (TREE_OCR, "TREE_OCR", "tree_sidebar_shows_files_under_cage"),
    (
        VSPLIT_OCR_LEFT,
        "VSPLIT_OCR_LEFT",
        "vertical_split_shows_two_panes_under_cage",
    ),
    (
        VSPLIT_OCR_RIGHT,
        "VSPLIT_OCR_RIGHT",
        "vertical_split_shows_two_panes_under_cage",
    ),
    (
        HSPLIT_OCR_TOP,
        "HSPLIT_OCR_TOP",
        "horizontal_split_shows_two_panes_under_cage",
    ),
    (
        HSPLIT_OCR_BOTTOM,
        "HSPLIT_OCR_BOTTOM",
        "horizontal_split_shows_two_panes_under_cage",
    ),
    (
        FOCUS_OCR_LEFT,
        "FOCUS_OCR_LEFT",
        "focus_move_returns_to_first_pane_under_cage",
    ),
    (
        FOCUS_OCR_RIGHT,
        "FOCUS_OCR_RIGHT",
        "focus_move_returns_to_first_pane_under_cage",
    ),
    (
        FOCUS_OCR_MARKER,
        "FOCUS_OCR_MARKER",
        "focus_move_returns_to_first_pane_under_cage",
    ),
    (
        CLOSE_OCR_KEEP,
        "CLOSE_OCR_KEEP",
        "close_pane_leaves_one_under_cage",
    ),
    (
        CLOSE_OCR_GONE,
        "CLOSE_OCR_GONE",
        "close_pane_leaves_one_under_cage",
    ),
    (
        TRI_OCR_A,
        "TRI_OCR_A",
        "three_panes_keep_all_files_visible_under_cage",
    ),
    (
        TRI_OCR_B,
        "TRI_OCR_B",
        "three_panes_keep_all_files_visible_under_cage",
    ),
    (
        TRI_OCR_C,
        "TRI_OCR_C",
        "three_panes_keep_all_files_visible_under_cage",
    ),
    (
        EMPTY_STATE_OCR,
        "EMPTY_STATE_OCR",
        "empty_state_shows_open_folder_button_under_cage",
    ),
    (
        NEWBUF_OCR_PRESENT,
        "NEWBUF_OCR_PRESENT",
        "new_untitled_buffer_replaces_open_file_under_cage",
    ),
    (
        NEWBUF_OCR_ABSENT,
        "NEWBUF_OCR_ABSENT",
        "new_untitled_buffer_replaces_open_file_under_cage",
    ),
    (
        QUIT_OCR_KEEP,
        "QUIT_OCR_KEEP",
        "colon_q_closes_focused_pane_under_cage",
    ),
    (
        QUIT_OCR_GONE,
        "QUIT_OCR_GONE",
        "colon_q_closes_focused_pane_under_cage",
    ),
    (
        CHORD_CLOSE_OCR_KEEP,
        "CHORD_CLOSE_OCR_KEEP",
        "close_pane_chord_closes_focused_pane_under_cage",
    ),
    (
        CHORD_CLOSE_OCR_GONE,
        "CHORD_CLOSE_OCR_GONE",
        "close_pane_chord_closes_focused_pane_under_cage",
    ),
];

/// Mechanically enforces the naming invariant documented on [`SIDEBAR_FILLER_FILES`]:
/// no filler filename may contain any substring a cage smoke asserts on.
///
/// ## Why a comment was promoted to a test
///
/// The invariant is load-bearing, and until now a comment was the only thing holding
/// it. Were a filler ever named `fillhello.md`, the tree sidebar would render the word
/// `hello` on a row of its own, [`window_renders_typed_text_under_cage`]'s OCR
/// assertion would be satisfied by that sidebar row, and the smoke would pass green
/// forever — with an editor pane that drew nothing at all. That is precisely the
/// silently-passing test `CLAUDE.md` warns about, and it is invisible in review:
/// [`SIDEBAR_FILLER_FILES`] and the assertion it can defeat sit a thousand lines apart,
/// and adding a filler file does not look like touching an assertion. A fixture that
/// can disarm the assertion it exists to support needs an executable guard, not prose.
///
/// ## The terms it guards
///
/// [`ASSERTED_OCR_TERMS`] is the authority; as a reader's summary the terms are
/// `hello`, `pickertarget`, `picktwo`/`pickone`, `treenode`,
/// `paneleft`/`paneright`, `htopfile`/`hbotfile`, `focusone`/`focustwo`/`zzleft`,
/// `closekeep`/`closegone`, `triaone`/`tribtwo`/`tricthree`, and — added with the
/// command-drain unit — `folder` ([`EMPTY_STATE_OCR`]), `zzfresh`
/// ([`NEWBUF_OCR_PRESENT`]), `newbufbase` ([`NEWBUF_OCR_ABSENT`]), `quitkeep`
/// ([`QUIT_OCR_KEEP`]), `quitgone` ([`QUIT_OCR_GONE`]), `chordkeep`
/// ([`CHORD_CLOSE_OCR_KEEP`]) and `chordgone` ([`CHORD_CLOSE_OCR_GONE`]). None is a
/// substring of any [`SIDEBAR_FILLER_FILES`] entry, and no filler may be renamed to
/// make one so. [`colon_qa_closes_the_window_under_cage`] contributes no term: it
/// asserts on the cage exit status and captures no image.
///
/// ## What it does NOT guard (and why that matters)
///
/// It inspects [`SIDEBAR_FILLER_FILES`] only — never a smoke's OWN fixture
/// filenames — so it cannot catch a smoke whose fixture file is named after the
/// word that smoke asserts. That gap was live, and an audit with the command-drain
/// unit found exactly two instances:
///
/// * [`picker_opens_and_loads_selected_file_under_cage`] shipped a
///   `pickertarget.md` fixture against a `pickertarget` assertion, and PASSED green
///   during a run in which the picker provably never opened — the sidebar row alone
///   satisfied it. See the note on [`PICKER_FILE_NAME`].
/// * [`vertical_split_shows_two_panes_under_cage`] shipped `paneleft.md` /
///   `paneright.md` against `paneleft` / `paneright`, so BOTH of its assertions
///   were satisfiable without `<C-w>v` splitting anything. See the note above the
///   `VSPLIT_*` consts.
///
/// Both are fixed, and the convention they now follow is the one to apply to any
/// new smoke: **when a smoke asserts on a file's CONTENT, the file's NAME must not
/// contain the asserted word.** The single deliberate exception is
/// [`tree_sidebar_shows_files_under_cage`], whose whole subject IS the sidebar row,
/// so [`TREE_OCR`] is a filename word on purpose — which is also why this guard
/// cannot simply be widened to cover fixture names without an opt-out list.
///
/// This test is PURE: no compositor, no `require_smoke_env`, no `CAGE_LOCK`, no
/// subprocess. It reads two const tables and finishes in microseconds, so it adds
/// nothing to the smoke suite's runtime and — unlike the fourteen smokes — still
/// guards the fixture on a machine where the smoke toolchain is unavailable.
#[test]
fn sidebar_filler_names_cannot_satisfy_any_smoke_assertion() {
    // Guard the guard: `ocr_to_lowercase` lowercases tesseract's output before every
    // `contains`, so an OCR const that is not itself lowercase could never match a real
    // capture — and would make the filename check below vacuous too.
    for &(term, const_name, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            !term.chars().any(char::is_uppercase),
            "asserted OCR term {const_name} = {term:?} is not all-lowercase. `ocr_to_lowercase` \
             lowercases tesseract's output before every `contains`, so `{smoke}` could never \
             match this term against a real capture. Lowercase the const."
        );
    }

    for name in SIDEBAR_FILLER_FILES {
        let lowercased = name.to_lowercase();
        for &(term, const_name, smoke) in ASSERTED_OCR_TERMS {
            assert!(
                !lowercased.contains(term),
                "sidebar filler file {name:?} contains {const_name} = {term:?}. The tree sidebar \
                 renders filler FILENAMES, so that row would satisfy `{smoke}`'s OCR assertion \
                 all on its own: the smoke would then pass green forever even if the editor pane \
                 drew nothing — a permanent silent false positive, not a flake. Rename the filler \
                 (see the doc comment on `SIDEBAR_FILLER_FILES`); do NOT change the OCR const and \
                 do NOT touch the smoke's assertion."
            );
        }
    }

    // Names must also be distinct: duplicates collapse on disk, so the fixture would
    // write fewer files than it claims to and thin the page density silently.
    let mut seen = std::collections::HashSet::new();
    for name in SIDEBAR_FILLER_FILES {
        assert!(
            seen.insert(name),
            "sidebar filler file {name:?} is listed twice in `SIDEBAR_FILLER_FILES`. Duplicate \
             names collapse on disk, so {SIDEBAR_FILLER_COUNT} entries would write fewer than \
             {SIDEBAR_FILLER_COUNT} files and silently thin the page density \
             `window_renders_typed_text_under_cage` depends on (measured floor: five files). \
             Below that floor tesseract misreads the isolated leading `h` of the typed word as a \
             ligature, and the smoke fails with no visible cause. Give every entry a distinct \
             name."
        );
    }
}

#[test]
fn window_renders_typed_text_under_cage() {
    // Serialize against the other cage test (shared Wayland socket); recover from
    // poisoning so a failure in the sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test (Cargo sets this env var for
    // integration tests of a crate that declares a `[[bin]]`).
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd, exactly like the other thirteen smokes. The app reads
    // `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with this
    // directory as its working directory (see `.current_dir` below). See
    // `make_temp_cwd` for why isolation is mandatory here: at a fixed cwd this
    // scenario's own `kill` leaves a stale `Untitled.md` swap that becomes the NEXT
    // run's fixture.
    let cwd = make_temp_cwd();

    // Give the tree sidebar rows of text to draw. See `SIDEBAR_FILLER_COUNT`: with an
    // empty cwd the editor draws `hello` correctly (proven pixel by pixel) but
    // tesseract misreads its isolated leading `h` as the ligature `ﬁ` on the sparse
    // page. These files restore the page density this assertion has always implicitly
    // relied on. They are fixture only — nothing here asserts on them.
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

    // Run the WHOLE scenario inside cage. cage sets WAYLAND_DISPLAY for the child
    // tree, so wtype/grim inside the scenario see the headless display. We inherit
    // the parent environment (WOLF_NVIM_BIN, WLR_BACKENDS, XDG_RUNTIME_DIR,
    // FONTCONFIG_FILE, ...) and add the scenario-specific vars.
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(SCENARIO)
        .current_dir(&cwd)
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
         moved off row 0 so it should not obscure any letter. PNG kept at {} — OPEN IT before \
         concluding anything: (a) if nothing drew at all (no window, no tildes, no cursor), it is \
         a rendering/launch failure; (b) if the window, sidebar, editor pane, tildes and cursor \
         all drew correctly but row 0 is EMPTY and the cursor sits TWO rows below it, then \
         exactly ONE keystroke — the leading `i` — was consumed before Neovim saw it, so \
         {OCR_WORD:?} executed in NORMAL mode instead of being inserted. The KNOWN cause of that \
         signature is the swap modal, and it is where to look FIRST: `nvim_buf_set_name` creates \
         the startup buffer's swap file inside the rename, and a pre-existing swap for \
         `<cwd>/Untitled.md` raises a blocking `do_dialog` ATTENTION/E325 prompt \
         (`mode() == \"r?\"`) that answers itself with your first key. Check \
         `state_dir()/swap/<hash-of-cwd>/` for a `%…%Untitled.md.swp` (on Linux that is \
         `~/.local/state/wolf/swap/`), and see `crates/wolf-core/tests/swap_recovery.rs`'s \
         `stale_untitled_swap_does_not_block_a_fresh_start`, which pins this headlessly \
         (and deterministically). Only if there is no such swap should you consider focus / \
         input routing (a Phase 2 pass chased that hypothesis and DISPROVED it — deferring \
         `grab_focus()` into `window.connect_map()` changes nothing). Either way it is NOT a \
         rendering failure, and no amount of extra settle time will fix it. OCR output \
         was:\n{ocr_text}",
        out_png.display()
    );
}

#[test]
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

    // OCR the screenshot and assert (loosely) that the target file's HEADING is
    // present — proving the picker opened, matched `PICKER_QUERY`, selected the
    // entry, and nvim loaded the file. The heading word appears nowhere but inside
    // the buffer (the fixture is named `ptarget.md`), so only a rendered buffer can
    // satisfy this; see the note on `PICKER_FILE_NAME` for the run in which a
    // filename-shaped term made this assertion pass with the picker never opening.
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
        // Large font so the rendered glyphs are legible to tesseract. (See the
        // "Note (OCR width)" paragraph above the VSPLIT_* consts: if two
        // side-by-side panes are too narrow for 28pt OCR, widen the window or lower
        // the font for THIS test.)
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

    // OCR the screenshot and assert (loosely) that BOTH files' HEADINGS render at
    // once — proving the vertical split produced a second pane bound to the new
    // nvim window/grid and painted it beside the first. Both terms are heading
    // words that appear nowhere but inside a loaded buffer (the fixtures are named
    // `pleft.md` / `pright.md`), so neither the sidebar rows nor nvim's `:e` toast
    // can satisfy either assertion; see the note above the VSPLIT_* consts.
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(VSPLIT_OCR_LEFT),
        "OCR did not contain {VSPLIT_OCR_LEFT:?} — the HEADING of {VSPLIT_FILE_LEFT}, which only \
         renders once that file is loaded into a PANE. A miss usually means the first pane stopped \
         rendering after the split; if NEITHER heading appears, suspect instead that the `:e` \
         commands never reached the cmdline (see the cmdline-visibility note above the VSPLIT_* \
         consts). PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
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
}

#[test]
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
        // Large font so the rendered glyphs are legible to tesseract. (See the
        // "Note (OCR height)" paragraph above the HSPLIT_* consts: if two stacked
        // panes are too short for 28pt OCR, make the window taller or lower the font
        // for THIS test.)
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
    // Note: this proves the focus-move PLUMBING end to end but does NOT
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
    // and the survivor reclaimed the width. Both assertions are load-bearing; see
    // the close-pane scenario notes above for why the NOT-contains one is sound.
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
        // Large font so the rendered glyphs are legible to tesseract. (See the
        // "Note (OCR width)" paragraph above the TRI_* consts: if three side-by-side
        // panes are too narrow for 28pt OCR, widen the window or lower the font for
        // THIS test.)
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
    // load-bearing assertion is `tribtwo`: the pre-n-ary binary model blanked the
    // right pane B.
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
fn empty_state_shows_open_folder_button_under_cage() {
    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

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

    // Run the whole empty-state scenario inside cage. Setting HOME to the SAME temp
    // directory we hand cage as its cwd is what fires `launch_decision`'s
    // `cwd == home` branch — the production rule, with no test-only env var — and
    // keeps every $HOME-derived path (state_dir, the MRU sqlite, the swap dir)
    // inside the temp dir too.
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(EMPTY_STATE_SCENARIO)
        .current_dir(&cwd)
        .env("HOME", &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        // Large font so the rendered glyphs are legible to tesseract.
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot and assert (loosely) that the status page rendered. The
    // word appears three times (title, description, button), which is the
    // OCR-robustness margin.
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
    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop the base markdown file into it. The app reads
    // `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with this
    // directory as its working directory (see `.current_dir` below).
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
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(NEWBUF_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("NEWBUF_FILE", NEWBUF_FILE)
        .env("NEWBUF_MARKER", NEWBUF_MARKER)
        // Large font so the rendered glyphs are legible to tesseract.
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot. The marker typed into the FRESH buffer must render, and
    // the base file's heading must NOT — together they prove Ctrl+Shift+N created a
    // new untitled buffer and put it in the focused pane. Both assertions are
    // load-bearing; see the NEWBUF_* const notes for why the NOT-contains one is
    // sound (the filename differs from the heading word, so nvim's `:e` toast and
    // the sidebar row cannot satisfy it).
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(NEWBUF_OCR_PRESENT),
        "OCR did not contain the marker {NEWBUF_OCR_PRESENT:?} typed into the new untitled \
         buffer. The likely causes: the Ctrl+Shift+N accelerator never fired (GTK's application \
         accels are matched in the CAPTURE phase — if this fails together with both picker \
         smokes, suspect accel matching for a shifted letter and see the plan's contingency), or \
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

#[test]
fn colon_q_closes_focused_pane_under_cage() {
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
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(COLON_Q_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("QUIT_FILE_KEEP", QUIT_FILE_KEEP)
        .env("QUIT_FILE_GONE", QUIT_FILE_GONE)
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot. The survivor (left) pane's file must render, and the
    // closed (right) pane's file must NOT — proving `:q` removed the focused pane
    // and the survivor reclaimed the width, exactly as <C-w>c does.
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
    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // No screenshot is taken here, but the smoke env is still required: the scenario
    // launches under `cage` and drives it with `wtype`.
    let _runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd with no fixture files: nothing is opened, and the startup
    // buffer stays untouched so init.lua's E37 modified-buffer guard does not trip.
    let cwd = make_temp_cwd();

    // Run the whole :qa scenario inside cage. This smoke deliberately captures NO
    // PNG and runs neither grim nor tesseract: its assertion is that the app PROCESS
    // exited, and exited CLEANLY, which no pixel can show.
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(COLON_QA_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        // Kept for parity with the other smokes: the window renders before :qa, and
        // a legible window is what makes a retained-video/manual repro readable.
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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
         Exit 11: the app died before its window ever mapped. That is a LAUNCH failure, not a \
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
    let status = Command::new("cage")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(CHORD_CLOSE_SCENARIO)
        .current_dir(&cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("CHORD_CLOSE_FILE_KEEP", CHORD_CLOSE_FILE_KEEP)
        .env("CHORD_CLOSE_FILE_GONE", CHORD_CLOSE_FILE_GONE)
        // Large font so the rendered glyphs are legible to tesseract.
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
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

    // OCR the screenshot. The survivor (left) pane's file must render, and the
    // closed (right) pane's file must NOT — proving the chord travelled the whole
    // front-end path (capture-phase controller -> `actions::action_for_key` ->
    // `win.close-pane` -> `UiAction::ClosePane` -> `InputEvent::ClosePane`) and
    // removed the focused pane. Both assertions are load-bearing; see the
    // CHORD_CLOSE_* const notes for why the NOT-contains one is sound.
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
