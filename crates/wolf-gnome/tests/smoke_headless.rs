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
//! **Eighteen cage smokes** — one window-render smoke, two file-picker smokes, one
//! file-tree-sidebar smoke, five split-pane smokes, one empty-state smoke, one
//! new-untitled-buffer smoke, one Ctrl+Shift+W close-pane smoke (the accelerator
//! path, as opposed to the split-pane suite's `<C-w>c`), two quit smokes (`:q`
//! closes the focused pane, `:qa` closes the window) and four POINTER smokes
//! (click-to-position, drag-to-select, wheel scroll, and the click-then-close
//! focus-wedge regression) — each of which launches the real binary under `cage`.
//!
//! The four pointer smokes additionally run **`wolf-pointer-inject`** inside cage —
//! this repo's own long-lived `zwlr_virtual_pointer_v1` client, see
//! `tools/wolf-pointer-inject/README.md` — started BEFORE the app so the seat
//! already carries `WL_SEAT_CAPABILITY_POINTER` when GTK does its first `wl_seat`
//! roundtrip, and driven over a FIFO with a poll-for-ack protocol. Three of them
//! assert on FILES THE CAPTIVE NEOVIM WRITES ABOUT ITS OWN STATE — `line('.')` /
//! `col('.')`, `visualmode()` plus `'<` / `'>`, and `line('w0')` — rather than on
//! OCR. Those are exact integers that no sidebar row and no `:e` toast can satisfy,
//! and OCR is structurally incapable of answering "did the cursor land at exactly
//! row 12, column 25".
//!
//! Fourteen of the eighteen OCR a screenshot of the live window. The `:qa` smoke
//! deliberately captures no image and asserts on the cage exit status alone (the app
//! must be GONE, and gone with a ZERO exit status — a panic in the GTK main loop
//! makes a process vanish just as a clean shutdown does); the click, drag and scroll
//! smokes still capture and still RETAIN a screenshot for post-mortem, but assert on
//! their readback files rather than on pixels.
//!
//! **One pure guard**, [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`],
//! which mechanically enforces the fixture invariants the smokes' OCR assertions
//! depend on. It is compositor-free: it does NOT call `require_smoke_env`, takes no
//! `CAGE_LOCK`, spawns no subprocess and needs none of the toolchain below, so it
//! still guards the fixture on a machine where the smoke toolchain is unavailable.
//! It lives here, next to the consts it reads, rather than in a file of its own.
//!
//! So this target's summary line reads **`19 passed`**, not `18`: eighteen cage
//! smokes plus that guard.
//!
//! ## When it runs
//!
//! The eighteen cage smokes run as part of the STANDARD suite — `cargo test -p
//! wolf-gnome` and `cargo test --workspace` — inside the flake devShell. They are
//! the Linux verification gate's only end-to-end proof that the GTK front-end draws
//! a window a human could see (see `build.md`, "The Linux verification gate").
//!
//! They are deliberately NOT `#[ignore]`d. "They are slow" is not a reason to skip
//! the only real end-to-end test the front-end has; per `CLAUDE.md` a test the
//! suite command does not run is a silently-skipped test.
//!
//! They are, however, the slowest part of the suite: each drives a real
//! compositor, and all eighteen serialize behind `CAGE_LOCK` (see below).
//!
//! ```text
//! nix develop --command cargo test -p wolf-gnome --test smoke_headless
//! ```
//!
//! ## Requirements of the eighteen cage smokes (from the devShell's smoke toolchain)
//!
//! - `cage` (headless wlroots compositor), `wtype`, `grim`, `tesseract` and
//!   `wolf-pointer-inject` on `PATH`. The last one is NOT an upstream package: it is
//!   built from this repo's own `tools/wolf-pointer-inject/` by `flake.nix`, so
//!   `command -v wolf-pointer-inject` succeeding can only mean the flake provided
//!   it. Only the four pointer smokes use it, but it is required unconditionally —
//!   see [`REQUIRED_TOOLS`].
//! - `WOLF_NVIM_BIN` pointing at the pinned Neovim (same contract as the other
//!   wolf-gnome tests; nvim is never taken from `PATH`).
//! - `XDG_RUNTIME_DIR` set and writable (the Wayland socket lives there).
//!
//! Per project policy every one of the eighteen FAILS LOUD if any of the above is
//! missing rather than silently skipping, mirroring `common::require_nvim_bin` — a
//! missing precondition means the smoke toolchain was not provisioned, which is a
//! setup bug we want surfaced, not hidden. Now that these tests run by default, that
//! hard fail matters more, not less: a missing tool means the suite was invoked
//! outside the devShell, and softening `require_smoke_env` to a skip would let the
//! gate pass green on an unprovisioned machine while proving nothing. Do not.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use cairo::{Context, Format, ImageSurface};
use wolf_gnome::CellMetrics;

/// The lowercase word we type in insert mode (in the window smoke scenario).
/// Kept short and all-lowercase so OCR noise is unlikely to corrupt every
/// character. After typing it we move the cursor OFF its row (see [`SCENARIO`])
/// so the full word — not just a prefix — can be asserted against OCR.
const OCR_WORD: &str = "hello";

/// Serializes the eighteen cage smoke tests. Each launches its own
/// `cage` compositor, and two compositors started concurrently contend for the
/// same Wayland socket under `XDG_RUNTIME_DIR` (lockfile error), failing both.
/// Under the default multi-threaded test harness these tests would otherwise run
/// in parallel, so each acquires this lock for its whole body to force serial
/// execution. Poisoning is recovered from (`into_inner`) so one panicking test
/// does not cascade into failing the others.
///
/// The four pointer smokes join this same lock rather than living in a file of
/// their own: each `tests/*.rs` is a separate crate AND a separate PROCESS, so an
/// in-process `static Mutex` cannot serialize across them at all, and
/// `tests/common/mod.rs` is compiled separately into every integration-test binary,
/// so a `static CAGE_LOCK` moved there would be N independent mutexes rather than
/// one shared one — it would look like a fix and serialize nothing.
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
/// files makes the dependency explicit and deliberate, and mirrors what the twelve
/// other smokes that drop fixture files into their own temp cwds already get
/// incidentally. (Five drop no files: the empty-state smoke hides the sidebar
/// entirely, the `:qa` smoke asserts on an exit status rather than OCR, and the
/// click, drag and scroll pointer smokes build their buffer with `:put` and assert
/// on Neovim-written readback files rather than on pixels.)
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
/// `quitkeep`/`quitgone`, `chordkeep`/`chordgone` and — added with the
/// pointer-harness unit — `wedgestay`/`wedgeclick` — so a stray filler can never
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

// ---------------------------------------------------------------------------
// Click-then-close focus-wedge smoke: fixtures.
//
// The two files exist so the smoke can (a) put a CLICKABLE pane on the left, (b)
// close that same pane, and (c) corroborate with OCR that the close really
// happened. The load-bearing assertions are the two `ALIVE*` readback files; the
// OCR pair is corroboration, not the proof.
//
// Both obey the file-naming convention the sibling close smokes follow: a
// fixture's NAME must not contain the word its own assertion needs. After
// `:e zclick.md` Neovim's toast and the sidebar row both show `zclick.md`, whose
// lowercase contains neither `wedgeclick` nor `wedgestay`, so only a rendered
// BUFFER can satisfy either assertion.

/// Wedge smoke: file opened FIRST, so it ends up in the LEFT pane. This is the
/// pane the smoke CLICKS and then CLOSES.
const WEDGE_FILE_CLICK: &str = "zclick.md";

/// Heading written into [`WEDGE_FILE_CLICK`]; lowercased it contains
/// [`WEDGE_OCR_CLICK`].
const WEDGE_HEADING_CLICK: &str = "# WEDGECLICK";

/// Lowercased substring OCR must NOT find after the close: the clicked-then-closed
/// pane's file.
const WEDGE_OCR_CLICK: &str = "wedgeclick";

/// Wedge smoke: file opened in the new RIGHT pane after `<C-w>v` — the SURVIVOR.
const WEDGE_FILE_STAY: &str = "zstay.md";

/// Heading written into [`WEDGE_FILE_STAY`]; lowercased it contains
/// [`WEDGE_OCR_STAY`].
const WEDGE_HEADING_STAY: &str = "# WEDGESTAY";

/// Lowercased substring OCR MUST find after the close: the survivor pane's file.
const WEDGE_OCR_STAY: &str = "wedgestay";

/// Exact contents of the CONTROL readback the wedge smoke writes BEFORE the pane
/// close. Its presence separates "keys never worked at all / the click broke input
/// immediately" from "keys died at the close" — without it a red run is ambiguous.
const WEDGE_ALIVE_MARK_1: &str = "ALIVE1";

/// Exact contents of the PROOF readback the wedge smoke writes AFTER the pane
/// close. Its absence IS the click-then-close focus wedge.
const WEDGE_ALIVE_MARK_2: &str = "ALIVE2";

// ---------------------------------------------------------------------------
// Pointer smokes: injected geometry.
//
// No smoke hardcodes a magic pixel for the cell it means to hit. The Rust side
// measures `cell_w`/`cell_h` EXACTLY (see `cell_metrics_for_smoke`), clicks once at
// the PROBE point, reads the resulting `(line, col)` back out of Neovim, and then
// expresses every later target RELATIVE to the probe:
//
//     TARGET = (round(PROBE_X + DX * cell_w), round(PROBE_Y + DY * cell_h))
//
// with the assertion on the exact DELTA `C1 - C0 == (DY, DX)`. That is
// origin-independent, so no unknown constant can make it pass or fail spuriously.
//
// The derived pane ORIGIN is then checked as a loud, self-checking precondition —
// see `assert_probe_cell_and_pane_origin`. It is the tripwire that makes a future
// GTK / libadwaita / cage geometry change fail these smokes loudly instead of
// silently retargeting a different cell.

/// x of the point every pointer smoke probes first, in absolute OUTPUT pixels
/// (`zwlr_virtual_pointer_v1.motion_absolute` takes output pixels; there is no CSD
/// or surface offset under cage — `motion 640 360` was measured arriving at the GTK
/// client as surface-local `640.00000000, 360.00000000`).
///
/// MEASURED on this host at `WOLF_FONT_PT` = [`SMOKE_FONT_PT`] on a 1280x720 cage
/// output: `(640, 360)` maps to Neovim cell `(line 8, col 18)`, which satisfies
/// every geometry precondition at once — `l0 >= 2 && c0 >= 2`;
/// `c0 + CLICK_DX(7) = 25 <= 30`; `c0 + DRAG_DX(9) = 27 <= 30`; and
/// `l0 + DRAG_DY(5) = 13 <= 14` visible rows. It is also comfortably inside the LEFT
/// pane after a 50/50 vertical split, which the wedge smoke depends on.
///
/// If a phase ever lands within half a pixel of a cell boundary and skews a delta
/// assertion, the symptom is REPRODUCIBLE, not flaky (fixed output size,
/// `FONTCONFIG_FILE`-pinned DejaVu, pinned GTK/libadwaita): shift this by 2 px and
/// record why.
const PROBE_X: i32 = 640;

/// y of the probe point, in absolute output pixels. See [`PROBE_X`].
const PROBE_Y: i32 = 360;

/// x of the editor pane's top-left corner in output pixels, as DERIVED from the
/// probe click: `PROBE_X - (c0 - 1) * cell_w`.
///
/// MEASURED 2026-08-02 on a **1280x720** cage output at `WOLF_FONT_PT` =
/// [`SMOKE_FONT_PT`]: the probe at `(640, 360)` landed on Neovim cell
/// `(line 8, col 18)` with `cell_w = 22.4765625`, `cell_h = 44.0`, giving a derived
/// origin of `(257.8984375, 52.0)`. Both numbers are also recorded in
/// `crates/wolf-gnome/tests/README.md`.
///
/// The value carries the probe click's unknown SUB-CELL PHASE, so it OVER-estimates
/// the true origin by up to one cell — which is exactly why the band adds `cell_w` /
/// `cell_h` (see [`PANE_ORIGIN_TOLERANCE_PX`]). `-1.0` is the deliberate impossible
/// placeholder the smoke is first landed with; re-deriving it is a matter of putting
/// `-1.0` back and reading the band failure's message.
const PANE_ORIGIN_X_EXPECTED: f64 = 257.8984375;

/// y of the editor pane's top-left corner in output pixels, as DERIVED from the
/// probe click: `PROBE_Y - (l0 - 1) * cell_h`. See [`PANE_ORIGIN_X_EXPECTED`] for the
/// measurement, its date and the cage output size it was taken on.
const PANE_ORIGIN_Y_EXPECTED: f64 = 52.0;

/// Half-width of the band the derived pane origin must fall inside.
///
/// ACCEPTANCE CRITERION: it must be narrow enough that a `+20,+20` surface offset
/// FAILS, so it has to be `< 20.0`. `12.0` also comfortably contains the measured
/// 15.2 px (x) / 16 px (y) spread of derived origins across three probe points —
/// that spread is the SUB-CELL PHASE of the probe click, which is why the band adds
/// `cell_w` / `cell_h` on top. Those two terms are an unavoidable phase allowance,
/// NOT slack, and must not be grown to make a failing run pass.
const PANE_ORIGIN_TOLERANCE_PX: f64 = 12.0;

/// The smallest Neovim `line`/`col` the probe click may report. `(1, 1)` is where
/// the cursor already is, so anything below this means the press never landed and
/// every delta downstream would be measuring a no-op.
const MIN_PROBE_CELL: i64 = 2;

/// Width, in characters, of every line the `:put` fixture writes. See the
/// `fill_buffer` comment in [`POINTER_PREAMBLE`] for the three empirical
/// constraints that pin this number.
const FIXTURE_LINE_WIDTH: i64 = 30;

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

/// The `<linux/input-event-codes.h>` `BTN_LEFT` code, as the helper's `press` /
/// `release` grammar takes it. Passed into every pointer scenario as `$BTN_LEFT` so
/// this const is the single source of truth rather than a `272` retyped four times.
const POINTER_BUTTON_LEFT: &str = "272";

/// What `visualmode()` returns for a CHARWISE visual selection. An EMPTY string
/// instead means Neovim never entered visual mode at all.
///
/// MEASURED, and NOT what an earlier version of this file assumed: a `<LeftDrag>` is
/// **not** the only way in. On nvim 0.12.2 a bare `<LeftMouse>` at one cell followed
/// by a `<LeftRelease>` at a DIFFERENT cell also enters charwise visual mode, with
/// exactly the same marks:
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

/// Upper bound on any pixel the harness will inject, used only as a sanity range in
/// [`pixel_target`]. Far above any plausible cage output; its job is to reject a
/// nonsense value (a NaN cell size, a runaway multiplication) before it is silently
/// narrowed into a plausible-looking coordinate.
const MAX_INJECTED_PIXEL: f64 = 65_535.0;

/// Name, under the smoke's temp cwd, of the FIFO the scenario writes helper commands
/// into.
const POINTER_CMD_FIFO_NAME: &str = "wolf-pointer.fifo";

/// Name, under the smoke's temp cwd, of the file the helper's STDOUT (`ready …`,
/// `ok <n>`, `err <n> …`) is redirected to. The scenario polls it for acks.
const POINTER_ACK_NAME: &str = "wolf-pointer.ack";

/// Name, under the smoke's temp cwd, of the file the helper's STDERR is redirected
/// to. Every scenario `exit` echoes it, and the Rust side prints it in the
/// cage-status failure message.
const POINTER_HELPER_ERR_NAME: &str = "wolf-pointer.err";

/// Name, under the smoke's temp cwd, of the file the scenario copies the helper's
/// `ready extents=<W>x<H> …` line into, so the Rust side can assert every injected
/// point really is inside the compositor's reported output rather than assuming it.
const POINTER_READY_NAME: &str = "wolf-pointer.ready";

/// External binaries the scenario shells out to. All come from the flake's
/// smoke toolchain; absence means the devShell was not entered.
///
/// `wolf-pointer-inject` is the odd one out and deliberately so: it is not an
/// upstream nixpkgs package but THIS REPO'S OWN tool, built from
/// `tools/wolf-pointer-inject/` by `flake.nix`'s `wolfPointerInject` derivation and
/// put on the devShell `PATH`. The name is repo-namespaced precisely so that
/// `command -v wolf-pointer-inject` succeeding can only mean the flake provided it —
/// a bare `pointer-inject` could be shadowed by something on the host. It is
/// required unconditionally, not just by the four pointer smokes: a devShell missing
/// it is an unprovisioned devShell, and the project policy is to fail loud rather
/// than let the gate pass green while proving less than it claims.
const REQUIRED_TOOLS: [&str; 5] = ["cage", "wtype", "grim", "tesseract", "wolf-pointer-inject"];

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
/// the five compositor/OCR/pointer tools are on `PATH`, `WOLF_NVIM_BIN` is set, and
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
         tesseract, wolf-pointer-inject). Run via `nix develop --command cargo test -p \
         wolf-gnome`. Note that `wolf-pointer-inject` is built from this repo's own \
         `tools/wolf-pointer-inject/` by flake.nix, NOT from nixpkgs — if it alone is \
         missing, the devShell is stale, so re-enter `nix develop`."
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

/// Everything the FOUR pointer scenarios have in common, as one `sh` fragment:
/// the teardown trap, the command FIFO, the `wolf-pointer-inject` helper, the
/// poll-for-ack protocol, the readback-file poll, the buffer fixture, the app
/// launch + window-map poll, and one initial in-pane `motion`.
///
/// This is a DELIBERATE deviation from this file's "one self-contained scenario
/// const per smoke" style. The preamble is ~90 lines of FIFO / ack / poll plumbing,
/// and four copies would be four places to fix a poll bug — the exact shape that
/// lets a harness rot in three of its four instances unnoticed. Each smoke's own
/// body is still a self-contained const; [`pointer_scenario`] concatenates them.
///
/// ## Ordering: helper FIRST, then the app
///
/// The helper is started and confirmed `ready` BEFORE `"$WOLF_BIN"` launches, so the
/// seat already carries `WL_SEAT_CAPABILITY_POINTER` when GTK does its first
/// `wl_seat` roundtrip and calls `get_pointer` at bind time. MEASURED under cage:
/// the seat trace shows `wl_seat#3.capabilities(3)` -> `get_pointer` at bind time,
/// dropping to `2` only when the helper quits, and the GTK client received
/// `wl_pointer#21.enter(…, 640.0, 360.0)` followed by real
/// `wl_pointer#21.button(…, 272, 1)` / `(…, 272, 0)` events. Mid-session capability
/// appearance also works, so this ordering is belt-and-braces rather than a known
/// requirement — but it removes an entire class of question from every failure
/// diagnosis at zero cost.
///
/// ## Teardown: helper, app, cage
///
/// `cleanup` kills the HELPER first — it is the only process that can still inject —
/// so the app's shutdown never races a pointer event; then the app; then the
/// scenario returns and cage exits with its child. The trap is installed before
/// anything is started, so every `exit NN` below runs it.
///
/// ## Exit codes (following the [`COLON_QA_SCENARIO`] precedent)
///
/// * `11` — the app died before its window mapped.
/// * `12` — the helper never became ready.
/// * `13` — a helper command was not acked within budget.
/// * `14` — a Neovim readback file never appeared.
/// * `15` — the helper died mid-scenario.
///
/// Each one echoes a one-line reason AND the helper's stderr, because when the
/// helper is the failing party its own message ("no `zwlr_virtual_pointer_manager_v1`
/// in the registry", "more than one `wl_output`") is the whole diagnosis.
const POINTER_PREAMBLE: &str = r#"
set -eu

# Poll knobs. WAIT_TRIES/WAIT_INTERVAL/SETTLE are the SAME values the fourteen
# keyboard-only smokes use, sized for the FULL workspace suite where ~90
# integration-test binaries run in parallel. Timing knobs only -- no assertion
# depends on any of them.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=0.5
# How many WAIT_INTERVAL polls to allow for a helper line (`ready ...` or `ok <n>`).
ACK_TRIES=75
# How many WAIT_INTERVAL polls to allow for a readback file the captive Neovim
# writes. Larger than ACK_TRIES: an ack is one compositor roundtrip, whereas a
# readback needs wtype -> GTK -> nvim_input -> Ex execution -> writefile.
FILE_TRIES=100
# Pause between two injected pointer events that belong to the SAME gesture; see
# `hstep`. A timing knob only -- no assertion depends on its value.
POINTER_STEP=0.2

# ---------------------------------------------------------------------------
# Teardown, installed BEFORE anything is started so every `exit NN` below runs it.
#
# The helper dies FIRST: it is the only process that can still inject, so killing
# it first means the app's shutdown is never racing a pointer event. Belt and
# braces, the helper also exits on stdin EOF and on a failed wl_display_roundtrip,
# so an orphan reaps itself when cage tears the socket down.
cleanup() {
    set +e
    if [ -n "${helper_pid:-}" ]; then
        kill "$helper_pid" 2>/dev/null
        wait "$helper_pid" 2>/dev/null
    fi
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

# ---------------------------------------------------------------------------
# The command channel.
#
# CMD_FIFO / ACK_FILE / HELPER_ERR / READY_FILE are absolute paths handed in by the
# Rust side, all under this run's fresh temp cwd.
mkfifo "$CMD_FIFO"

# Start the helper BEFORE the app (see this const's doc comment). Its stdout
# carries the readiness line and the acks; its stderr carries diagnostics that
# every failure path below echoes.
wolf-pointer-inject < "$CMD_FIFO" > "$ACK_FILE" 2> "$HELPER_ERR" &
helper_pid=$!

# Open the WRITE end of the FIFO and keep fd 3 open for the WHOLE scenario. This
# is load-bearing, not tidiness: without it every `printf ... > "$CMD_FIFO"` would
# open and then CLOSE the FIFO, handing the helper a spurious EOF after the first
# command, and the helper exits on EOF by design.
#
# This `exec` is also what unblocks the helper: its own `< "$CMD_FIFO"` blocks
# until a writer appears. Readiness must therefore be polled AFTER this line.
# (If `wolf-pointer-inject` were missing from PATH entirely this would block
# forever -- which is why `require_smoke_env` checks for it before we get here.)
exec 3>"$CMD_FIFO"

# 1-based command counter; the helper acks `ok <seq>` counting non-blank lines.
seq=0

# True (exit 0) when the helper is no longer running.
hdead() {
    if kill -0 "$helper_pid" 2>/dev/null; then
        return 1
    fi
    return 0
}

# Poll the helper's stdout for its readiness line, then copy that line into
# READY_FILE so the Rust side can read the AUTHORITATIVE output extents rather
# than assuming 1280x720 (which is wlroots' headless default, not a contract).
hwait_ready() {
    t=0
    while [ "$t" -lt "$ACK_TRIES" ]; do
        if grep -q '^ready ' "$ACK_FILE" 2>/dev/null; then
            grep '^ready ' "$ACK_FILE" > "$READY_FILE" || true
            return 0
        fi
        if hdead; then
            echo "wolf-gnome pointer smoke: wolf-pointer-inject exited before printing its readiness line." >&2
            echo "  Its exit code names the cause (3 = cage does not advertise zwlr_virtual_pointer_manager_v1," >&2
            echo "  4 = no wl_seat, 5 = no wl_output, 6 = more than one output, 7 = zero extents)." >&2
            echo "  wolf-pointer-inject stderr follows:" >&2
            cat "$HELPER_ERR" >&2
            exit 15
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome pointer smoke: wolf-pointer-inject never printed 'ready ...' within budget." >&2
    echo "  Check stdout buffering FIRST: without setbuf(stdout, NULL) the readiness line never" >&2
    echo "  reaches a redirected stdout and this poll can only time out." >&2
    echo "  wolf-pointer-inject stderr follows:" >&2
    cat "$HELPER_ERR" >&2
    exit 12
}

# Send one command to the helper and BLOCK until it is acked.
#
# The ack is printed only after a wl_display_roundtrip, so it means "the compositor
# has processed this event", not "the bytes left our buffer" -- that is what makes
# this poll meaningful rather than cosmetic. (`quit` is the one exception: its ack
# follows a flush + disconnect, because a roundtrip during teardown would race the
# compositor and produce a spurious exit 8.)
hsend() {
    seq=$((seq + 1))
    printf '%s\n' "$*" >&3
    t=0
    while [ "$t" -lt "$ACK_TRIES" ]; do
        if grep -qx "ok $seq" "$ACK_FILE" 2>/dev/null; then
            return 0
        fi
        if hdead; then
            # Re-check the ack before declaring death: `quit` prints its ack and
            # then exits, so a liveness check alone would race a perfectly normal
            # teardown into a spurious exit 15.
            if grep -qx "ok $seq" "$ACK_FILE" 2>/dev/null; then
                return 0
            fi
            echo "wolf-gnome pointer smoke: wolf-pointer-inject died while handling command #$seq ($*)." >&2
            echo "  wolf-pointer-inject stderr follows:" >&2
            cat "$HELPER_ERR" >&2
            exit 15
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome pointer smoke: command #$seq ($*) was never acked within budget." >&2
    echo "  The ack follows a wl_display_roundtrip, so a missing ack means the compositor" >&2
    echo "  never processed the event." >&2
    echo "  wolf-pointer-inject stderr follows:" >&2
    cat "$HELPER_ERR" >&2
    exit 13
}

# `hsend`, then pace before the next pointer event of the SAME gesture.
#
# LOAD-BEARING, and the reason is NEOVIM'S API rather than anything in GTK or in
# this repo. `nvim_input_mouse(button, action, modifier, grid, row, col)` transports
# only the BUTTON CODE through the typeahead; the POSITION rides on the globals
# `mouse_grid` / `mouse_row` / `mouse_col`, which the API handler overwrites the
# moment the call is serviced. Two calls serviced before nvim's main loop has
# CONSUMED the first key therefore both resolve at the SECOND call's position.
#
# MEASURED against nvim 0.12.2, three back-to-back calls (press at one cell, drag and
# release at another):
#
#     rapid : visualmode="" marks="0 0 0 0" cursor=<the RELEASE cell>
#     paced : visualmode="v" marks=<press cell>..<release cell>
#
# `hsend` acks after one compositor roundtrip and GTK hands a whole button/motion
# group to the app in a single main-loop iteration, so an unpaced press -> drag ->
# release reaches nvim ~0.4 ms apart, collapses onto ONE cell, and Neovim never
# enters visual mode at all -- intermittently, depending on whether its loop happened
# to drain in between. A real drag is spread over tens of milliseconds; this restores
# that, and it also stops GDK's motion compression from folding the intermediate
# waypoints into a single `drag-update`.
#
# This is a PACING knob, not a weakening: every assertion downstream is unchanged and
# still exact.
hstep() {
    hsend "$@"
    sleep "$POINTER_STEP"
}

# Poll for a readback file the captive Neovim writes. This replaces every
# assertion-bearing sleep: nothing downstream may read a file we merely hope is
# there. Returns non-zero on timeout so the caller decides what that means.
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
    echo "wolf-gnome pointer smoke: readback file $f never appeared (or stayed empty)." >&2
    echo "  wolf-pointer-inject stderr follows:" >&2
    cat "$HELPER_ERR" >&2
    return 1
}

# Fill the startup buffer with 200 lines of exactly 30 characters and park the
# viewport at the top. Used by the click, drag and scroll bodies; the wedge body
# opens real files instead.
#
# THREE EMPIRICAL CONSTRAINTS, all load-bearing:
#
#  * The lines must be 30 CHARACTERS wide -- not 1, and not 5. This is the trap
#    `crates/wolf-gnome/tests/mouse_nvim.rs` documents: `:put =range(1,200)` yields
#    ONE-character lines, so a click at column N clamps to end-of-line and the
#    exact-column assertion silently degenerates into "the cursor is at the end of
#    a short line", which is true no matter where the click landed.
#  * The lines must be NARROWER THAN THE PANE IN CELLS. This trap is NEW --
#    mouse_nvim.rs never renders, so it never met it. `runtime/nvim/init.lua` sets
#    `opt.wrap = true` and `opt.linebreak = true`, so any line wider than the pane
#    soft-wraps and SCREEN ROW STOPS CORRESPONDING TO BUFFER LINE -- the very
#    correspondence every assertion here rests on. MEASURED at WOLF_FONT_PT=28:
#    winwidth(0) = 45, so 30 characters leaves 15 columns of margin. The click and
#    drag bodies read winwidth(0)/winheight(0) back and the Rust side asserts on
#    them, so a future narrowing fails loudly. If the pane ever narrows below ~35
#    columns this FIXTURE must shrink -- never the assertion.
#  * A literal double quote must NOT appear in any Vimscript typed here. Under
#    `:put =<expr>` Neovim treats a `"` as the start of an Ex comment and strips it
#    BEFORE evaluating the expression (`E115: Missing quote`), so the buffer is
#    never filled. Every string below is SINGLE-quoted, with '' for an embedded
#    single quote. That also removes all shell-quoting and Rust-escaping noise,
#    since these scenarios are r#-raw strings interpolated into double-quoted sh
#    words.
#
# `:1delete` drops the empty line 1 that `:put` leaves above the inserted text, so
# every visible row carries 30 characters; `:1` parks topline at 1. Neither word
# collides with init.lua's `cnoreabbrev` set, which fires only on a whole-line
# exact match.
fill_buffer() {
    wtype -- ":put =map(range(1,200), 'printf(''%03d'', v:val) . repeat(''x'', 27)')"
    wtype -k Return
    sleep "$SETTLE"
    wtype -- ":1delete"
    wtype -k Return
    sleep "$SETTLE"
    wtype -- ":1"
    wtype -k Return
    sleep "$SETTLE"
}

# ---------------------------------------------------------------------------
# Bring the helper up, THEN the app.
hwait_ready

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the Wayland display to be usable
# (cheap no-op wtype succeeds only once cage has a mapped surface).
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome pointer smoke: app process exited early (pid $app_pid)" >&2
        echo "  wolf-pointer-inject stderr follows:" >&2
        cat "$HELPER_ERR" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

# Final settle for nvim's initial redraw. There is nothing observable to poll for
# the FIRST redraw, so this matches every existing smoke.
sleep "$SETTLE"

# One initial in-pane motion, so wl_pointer.enter and a first motion are delivered
# before anything depends on them. This is also the position `record_pointer`
# stores for the scroll handler, which GTK's coordinate-less `scroll` signal has no
# other way to learn.
hsend motion "$PROBE_X" "$PROBE_Y"
sleep "$SETTLE"
"#;

/// Concatenates [`POINTER_PREAMBLE`] with a per-smoke `body`. `Command::arg`
/// accepts a `String`, so the callers need no signature change.
fn pointer_scenario(body: &str) -> String {
    format!("{POINTER_PREAMBLE}\n{body}\n")
}

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
# `drag-update`. MEASURED on nvim 0.12.2, a press at one cell and a release at a
# DIFFERENT cell enters charwise visual mode all by itself -- press+release only and
# press+drag+release produce byte-identical `visualmode()`, marks and cursor. So
# Drag A's `v` is real coverage of `on_pane_release` but is NOT evidence that
# `<LeftDrag>` was ever delivered.
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
# three sibling close smokes.
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

/// Creates a fresh, unique temp directory to serve as the launched app's working
/// directory. The app derives `CoreConfig.cwd` from `std::env::current_dir()`, so
/// this is the directory the file-search traversal walks and the directory the
/// captive nvim runs in (the recent-files MRU lives in the global
/// `state_dir()/file-mru.sqlite`, not under the cwd).
///
/// ALL EIGHTEEN smokes use this. A fresh cwd per run is MANDATORY, not tidiness:
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

// ---------------------------------------------------------------------------
// Pointer smokes: shared Rust helpers.

/// The monospace family the app measures its cell metrics with.
///
/// This MUST match `wolf_gnome::app::FONT_FAMILY`, which is private — hence the
/// duplication. `crates/wolf-gnome/tests/paint_golden.rs` carries the same local
/// const for the same reason. If the app's family ever changes and this does not,
/// [`cell_metrics_for_smoke`] silently measures a DIFFERENT cell size, every derived
/// pixel target lands in the wrong cell, and the exact-delta assertions are what
/// catch it — which is why their failure messages name this const.
const SMOKE_FONT_FAMILY: &str = "DejaVu Sans Mono";

/// [`SMOKE_FONT_PT`] as an `f64` — PARSED, never re-typed as a literal.
///
/// The app is launched with that same STRING via `WOLF_FONT_PT`, so the two sides
/// cannot be allowed to drift: a re-typed `28.0` here would keep compiling after
/// someone changed `SMOKE_FONT_PT`, and every pixel target would then be computed
/// against a cell size the app does not use.
fn smoke_font_pt() -> f64 {
    SMOKE_FONT_PT.parse::<f64>().unwrap_or_else(|e| {
        panic!(
            "SMOKE_FONT_PT = {SMOKE_FONT_PT:?} does not parse as an f64 ({e}). It is handed to the \
             app verbatim as WOLF_FONT_PT and parsed here to size this test's own cell \
             measurement; the two MUST be the same number, so it has to parse."
        )
    })
}

/// Measures the app's monospace cell size EXACTLY, by making the same call the app
/// makes.
///
/// This is what makes the cell size exact rather than estimated:
/// `wolf_gnome::app::measure_metrics` and this function are the same
/// `CellMetrics::for_font` call, on the same throwaway 1x1 `ImageSurface` probe
/// (whose pixel size is irrelevant — it exists only to obtain a Pango context), with
/// the same family, the same points, and the same `FONTCONFIG_FILE`-pinned DejaVu.
/// `tests/paint_golden.rs::render_fixture` uses the identical idiom.
fn cell_metrics_for_smoke() -> CellMetrics {
    let probe = ImageSurface::create(Format::ARgb32, 1, 1)
        .expect("failed to create the 1x1 probe surface for cell measurement");
    let cr = Context::new(&probe).expect("failed to create the probe Cairo context");
    let metrics = CellMetrics::for_font(&cr, SMOKE_FONT_FAMILY, smoke_font_pt());
    drop(cr);
    assert!(
        metrics.cell_w > 0.0 && metrics.cell_h > 0.0,
        "CellMetrics::for_font({SMOKE_FONT_FAMILY:?}, {}) produced a non-positive cell size \
         (cell_w = {}, cell_h = {}). Every pixel target in every pointer smoke is derived from \
         these two numbers, so nothing below can mean anything. The usual cause is a missing \
         FONTCONFIG_FILE (the flake devShell pins DejaVu Sans Mono through it), which leaves Pango \
         resolving the family to nothing.",
        smoke_font_pt(),
        metrics.cell_w,
        metrics.cell_h
    );
    metrics
}

/// Absolute path of the file the scenario copies the helper's `ready …` line into.
fn pointer_ready_path(cwd: &Path) -> PathBuf {
    cwd.join(POINTER_READY_NAME)
}

/// Absolute path of the file the helper's stderr is redirected to.
fn pointer_err_path(cwd: &Path) -> PathBuf {
    cwd.join(POINTER_HELPER_ERR_NAME)
}

/// Runs one pointer scenario — [`POINTER_PREAMBLE`] plus `body` — inside `cage`, and
/// returns cage's exit status.
///
/// The per-smoke environment arrives as a SLICE rather than as positional
/// parameters. That is a deliberate lint-shape decision, following the `GridTarget`
/// precedent in `pane_view.rs`: eight more positional parameters would trip
/// `clippy::too_many_arguments`, which is gating here (`cargo clippy --workspace
/// --all-targets -- -D warnings`), and the only honest fix would be an `#[allow]`.
fn run_pointer_scenario(
    body: &str,
    cwd: &Path,
    out_png: &Path,
    extra_env: &[(&str, &OsStr)],
) -> ExitStatus {
    // Cargo sets this for integration tests of a crate that declares a `[[bin]]`.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let mut cmd = Command::new("cage");
    cmd.arg("--")
        .arg("sh")
        .arg("-c")
        .arg(pointer_scenario(body))
        .current_dir(cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", out_png)
        // Large font so the rendered glyphs are legible to tesseract, AND the exact
        // number `cell_metrics_for_smoke` measures against.
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
        .env("CMD_FIFO", cwd.join(POINTER_CMD_FIFO_NAME))
        .env("ACK_FILE", cwd.join(POINTER_ACK_NAME))
        .env("HELPER_ERR", pointer_err_path(cwd))
        .env("READY_FILE", pointer_ready_path(cwd))
        .env("PROBE_X", PROBE_X.to_string())
        .env("PROBE_Y", PROBE_Y.to_string())
        .env("BTN_LEFT", POINTER_BUTTON_LEFT);
    for &(key, value) in extra_env {
        cmd.env(key, value);
    }
    cmd.status()
        .expect("failed to spawn `cage` for a headless pointer smoke scenario")
}

/// Asserts cage exited 0, spelling out the pointer scenarios' exit-code table and
/// echoing the helper's own stderr — which, when the helper is the failing party, is
/// the whole diagnosis.
fn assert_pointer_scenario_succeeded(smoke: &str, status: ExitStatus, cwd: &Path, out: &Path) {
    let err_path = pointer_err_path(cwd);
    let helper_err = std::fs::read_to_string(&err_path)
        .unwrap_or_else(|e| format!("<could not read {}: {e}>", err_path.display()));
    assert!(
        status.success(),
        "{smoke}: the cage pointer scenario exited unsuccessfully ({status}). Read the exit code \
         FIRST; each one names a different half of the harness.\n\
         Exit 11: the app died before its window ever mapped. A LAUNCH failure — no pointer event \
         was ever injected, and nothing below is about pointer input.\n\
         Exit 12: `wolf-pointer-inject` never printed its `ready …` line. Check its own exit code \
         in the stderr below: 3 = cage does not advertise `zwlr_virtual_pointer_manager_v1` at \
         all, 4 = no `wl_seat`, 5 = no `wl_output` and no `-e`, 6 = more than one output, 7 = zero \
         extents. If the stderr is EMPTY, suspect stdout buffering in the helper \
         (`setbuf(stdout, NULL)` must be the first statement in `main`), which makes every poll \
         time out with nothing to show.\n\
         Exit 13: a helper command was never acked within budget. The ack is printed only after a \
         `wl_display_roundtrip`, so a missing ack means the compositor never processed the event. \
         The other classic cause is the FIFO write end being closed between commands — the \
         scenario holds fd 3 open for its whole life precisely to prevent that.\n\
         Exit 14: a readback file the captive Neovim was told to write never appeared. Keys are \
         not reaching Neovim, or the typed Ex command did not execute — this is a KEYBOARD \
         failure, not a pointer one, so check the keyboard-only smokes first.\n\
         Exit 15: the helper died mid-scenario.\n\
         A signal/abort status on the CAGE process itself means the compositor died, which is an \
         environment failure.\n\
         Screenshot expected at {}.\n\
         wolf-pointer-inject stderr was:\n{helper_err}",
        out.display()
    );
}

/// Asserts the retained screenshot exists and is non-empty.
fn assert_png_present(smoke: &str, out_png: &Path) {
    let meta = std::fs::metadata(out_png).unwrap_or_else(|e| {
        panic!(
            "{smoke}: expected screenshot {} to exist after the cage pointer scenario, but: {e}",
            out_png.display()
        )
    });
    assert!(
        meta.len() > 0,
        "{smoke}: screenshot {} exists but is empty (grim produced no image data)",
        out_png.display()
    );
}

/// Removes a stale evidence file and asserts it is gone BEFORE the scenario runs.
///
/// The temp cwd is fresh every run, so in practice nothing is ever removed here —
/// but a test whose evidence file could be a previous run's leftover is not
/// evidence, and this is the same clean-slate discipline every existing smoke
/// applies to its PNG. The assertion is what makes it non-vacuous: if the file
/// survives, every later assertion would be reading the wrong run.
fn clear_evidence(path: &Path, smoke: &str) {
    if path.exists() {
        std::fs::remove_file(path).unwrap_or_else(|e| {
            panic!(
                "{smoke}: failed to remove stale evidence file {}: {e}",
                path.display()
            )
        });
    }
    assert!(
        !path.exists(),
        "{smoke}: evidence file {} still exists after removal, so this run would read a PREVIOUS \
         run's evidence and could pass without the app doing anything at all.",
        path.display()
    );
}

/// Reads a file the captive Neovim (or the scenario) wrote, panicking with an
/// actionable message when it cannot be read.
fn read_readback(path: &Path, smoke: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "{smoke}: could not read the readback file {}: {e}. The scenario polls for this file \
             with `wait_file`, so a cage status of 0 together with an unreadable file here means \
             the file was written and then removed, or was written somewhere else (the path is \
             handed to Neovim as an ABSOLUTE env var, so it does not depend on Neovim's cwd).",
            path.display()
        )
    })
}

/// Reads a file whose whole content is one marker word, trimmed.
fn read_trimmed(path: &Path, smoke: &str) -> String {
    read_readback(path, smoke).trim().to_string()
}

/// Parses one whitespace-separated readback token as an `i64`.
fn parse_readback_int(token: &str, path: &Path, raw: &str, smoke: &str) -> i64 {
    token.parse::<i64>().unwrap_or_else(|e| {
        panic!(
            "{smoke}: readback token {token:?} in {} is not an integer ({e}). Raw file contents \
             were {raw:?}. Every readback integer is produced by a `printf('%d', …)` inside the \
             captive Neovim, so a non-integer means the typed Ex command did not execute as \
             written — check for a stray keystroke landing in the buffer instead of the cmdline.",
            path.display()
        )
    })
}

/// Reads a readback file and parses it into exactly `expected` integers.
///
/// Every integer readback goes through this, so a truncated or malformed file
/// produces ONE good message carrying the raw contents rather than four bespoke
/// ones.
fn read_ints(path: &Path, expected: usize, smoke: &str) -> Vec<i64> {
    let raw = read_readback(path, smoke);
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    assert!(
        tokens.len() == expected,
        "{smoke}: readback file {} holds {} whitespace-separated token(s), expected {expected}. \
         Raw contents were {raw:?}. A SHORT file usually means the `printf` format and its \
         argument list disagree; an EMPTY one means `writefile` ran with an empty list.",
        path.display(),
        tokens.len()
    );
    tokens
        .iter()
        .map(|t| parse_readback_int(t, path, &raw, smoke))
        .collect()
}

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
/// [`VISUAL_CHARWISE`]: on nvim 0.12.2 a press at one cell plus a release at a
/// DIFFERENT cell enters charwise visual mode on its own, with the same marks a full
/// press+drag+release produces. So the mode alone discriminates `drag-update` ONLY
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

/// Parses `extents=<W>x<H>` out of the helper's readiness line, which the scenario
/// copies verbatim into `$READY_FILE`.
///
/// The extents are the AUTHORITATIVE output size for the scenario, read rather than
/// assumed: 1280x720 is wlroots' headless default, not a contract.
fn read_output_extents(cwd: &Path, smoke: &str) -> (i64, i64) {
    let path = pointer_ready_path(cwd);
    let raw = read_readback(&path, smoke);
    let field = raw
        .split_whitespace()
        .find_map(|token| token.strip_prefix("extents="))
        .unwrap_or_else(|| {
            panic!(
                "{smoke}: the helper readiness line copied into {} has no `extents=<W>x<H>` \
                 field. Raw contents were {raw:?}. The expected shape is \
                 `ready extents=1280x720 manager_version=2 outputs=1`; see \
                 `tools/wolf-pointer-inject/README.md`.",
                path.display()
            )
        });
    let (w, h) = field.split_once('x').unwrap_or_else(|| {
        panic!(
            "{smoke}: the readiness line's extents field {field:?} in {} is not `<W>x<H>`. Raw \
             contents were {raw:?}.",
            path.display()
        )
    });
    let width = parse_readback_int(w, &path, &raw, smoke);
    let height = parse_readback_int(h, &path, &raw, smoke);
    assert!(
        width > 0 && height > 0,
        "{smoke}: the helper reported a degenerate output extent of {width}x{height} in {}. It \
         exits 7 rather than report zero extents, so this should be unreachable. Raw contents \
         were {raw:?}.",
        path.display()
    );
    (width, height)
}

/// Widens a small readback/geometry integer to `f64` WITHOUT an `as` cast: `i64` ->
/// `i32` through `try_from` (so an absurd value fails loudly instead of silently
/// truncating), then the lossless `f64::from`.
fn exact_f64(value: i64, what: &str) -> f64 {
    let narrowed = i32::try_from(value).unwrap_or_else(|e| {
        panic!(
            "{what} = {value} does not fit in an i32 ({e}), so it cannot be widened losslessly to \
             f64. Every value passed here is a cell count or a Neovim line/column, so this means \
             the readback is nonsense rather than that the bound is too small."
        )
    });
    f64::from(narrowed)
}

/// Converts a derived pixel coordinate into the integer the helper's `motion`
/// grammar takes: `f64::round()` followed by a CHECKED range.
///
/// Never a bare `as` cast on an unchecked value — that would turn a NaN or a runaway
/// multiplication into a plausible-looking coordinate and retarget the injection
/// silently, which is precisely the class of failure these smokes exist to make
/// impossible.
fn pixel_target(value: f64, what: &str) -> i64 {
    assert!(
        value.is_finite(),
        "{what} computed to a non-finite pixel ({value}); `cell_metrics_for_smoke` must have \
         returned a NaN or infinite cell size."
    );
    let rounded = value.round();
    assert!(
        (0.0..=MAX_INJECTED_PIXEL).contains(&rounded),
        "{what} rounded to {rounded}, outside the sane injected-pixel range \
         0.0..={MAX_INJECTED_PIXEL}. Either the measured cell size is wrong or the per-smoke cell \
         delta is far larger than intended."
    );
    // Exact: `rounded` is an integral f64 already proven to lie in
    // 0.0..=MAX_INJECTED_PIXEL, so this conversion cannot truncate. The range check
    // above is what makes the value CORRECT; Rust's float->int `as` merely saturates
    // rather than being undefined, which is not the same guarantee.
    rounded as i64
}

/// Asserts an injected point lies inside the output extents the helper reported.
///
/// A point outside the output would be CLAMPED by the compositor, so the pointer
/// would silently land somewhere other than the cell the smoke computed, and the
/// exact-delta assertion would then fail for a reason that has nothing to do with
/// the code under test. This turns that into a named precondition.
fn assert_inside_extents(smoke: &str, label: &str, point: (i64, i64), extents: (i64, i64)) {
    let (x, y) = point;
    let (width, height) = extents;
    assert!(
        x >= 0 && y >= 0 && x < width && y < height,
        "{smoke}: the injected point {label} = ({x}, {y}) is OUTSIDE the {width}x{height} \
         compositor output `wolf-pointer-inject` reported on its `ready` line. \
         `zwlr_virtual_pointer_v1.motion_absolute` clamps to the output, so this point would land \
         somewhere other than the cell this smoke computed. Either the cage output shrank, or the \
         measured cell size grew (check that WOLF_FONT_PT = {SMOKE_FONT_PT} still reaches the app \
         and that SMOKE_FONT_FAMILY = {SMOKE_FONT_FAMILY:?} still matches `app::FONT_FAMILY`). \
         Move PROBE_X/PROBE_Y or shrink the per-smoke cell deltas; do NOT loosen the delta \
         assertion."
    );
}

/// Asserts the probe click moved the cursor off `(1, 1)`, derives the editor pane's
/// on-screen origin from it, and asserts that origin sits inside the measured band.
/// Returns the derived origin so the caller can print it.
///
/// `probe_cell` is whatever Neovim reported for a gesture whose PRESS was at
/// `(PROBE_X, PROBE_Y)`: the click smoke passes its probe click's `line('.')` /
/// `col('.')`, and the drag smoke passes Drag Z's `'<` — Drag Z presses AND releases
/// at the probe, so its `'<` is the probe cell.
///
/// The band is the CSD / geometry TRIPWIRE. Because the probe lands at an unknown
/// sub-cell phase, the derived origin over-estimates the true origin by that phase,
/// i.e. it lies in `[true_origin, true_origin + cell_w)`. The `+ cell_w` / `+ cell_h`
/// terms below are therefore an unavoidable PHASE ALLOWANCE, not slack, and must not
/// be widened to make a failing run pass; [`PANE_ORIGIN_TOLERANCE_PX`] is what
/// carries the real tolerance, and it is deliberately `< 20.0` so a `+20,+20` surface
/// offset FAILS.
fn assert_probe_cell_and_pane_origin(
    smoke: &str,
    probe_cell: (i64, i64),
    metrics: &CellMetrics,
    extents: (i64, i64),
    out_png: &Path,
    raw: &str,
) -> (f64, f64) {
    let (line, col) = probe_cell;
    assert!(
        line >= MIN_PROBE_CELL && col >= MIN_PROBE_CELL,
        "{smoke}: the probe click at ({PROBE_X}, {PROBE_Y}) left the Neovim cursor at line {line}, \
         column {col}, but both must be >= {MIN_PROBE_CELL}. `(1, 1)` is exactly where the cursor \
         already was, so a `(1, 1)` readback means THE PRESS NEVER REACHED NEOVIM and every delta \
         below would be measuring a no-op. Check, in order: (1) `pane_view.rs::on_pane_press` \
         still calls `self.send_pane_mouse(target, MouseAction::Press, button, x, y, g)`; (2) the \
         `gtk::GestureDrag` built in `pane_view.rs::make_pane_area` still has \
         `set_button(gdk::BUTTON_PRIMARY)` and the injected button is still \
         {POINTER_BUTTON_LEFT}; (3) `pane_view.rs::grid_and_dims` resolved a grid at all (an \
         unbound pane sends nothing); (4) the helper really delivered — its stderr is echoed by \
         the scenario, and every command is acked only after a `wl_display_roundtrip`. Raw \
         readback was {raw:?}. PNG kept at {}.",
        out_png.display()
    );

    let origin_x = f64::from(PROBE_X) - exact_f64(col - 1, "the probe column") * metrics.cell_w;
    let origin_y = f64::from(PROBE_Y) - exact_f64(line - 1, "the probe line") * metrics.cell_h;
    let (width, height) = extents;

    assert!(
        (origin_x - PANE_ORIGIN_X_EXPECTED).abs() <= PANE_ORIGIN_TOLERANCE_PX + metrics.cell_w,
        "{smoke}: the editor pane's derived X ORIGIN is {origin_x}, outside the band \
         {PANE_ORIGIN_X_EXPECTED} +/- ({PANE_ORIGIN_TOLERANCE_PX} + cell_w). The four candidate \
         causes, in order of likelihood: (1) a GTK4 client-side-decoration / shadow-margin offset \
         has appeared under cage (there was none: `motion 640 360` was measured arriving as \
         surface-local `640.00000000, 360.00000000`); (2) the `adw::HeaderBar` height changed — a \
         libadwaita/GTK bump; (3) `app::TREE_PANE_WIDTH` or the `gtk::Paned` divider width \
         changed; (4) the cage output size changed. Derived origin ({origin_x}, {origin_y}); \
         expected ({PANE_ORIGIN_X_EXPECTED}, {PANE_ORIGIN_Y_EXPECTED}); cell_w {}, cell_h {}; \
         probe ({PROBE_X}, {PROBE_Y}) -> (line {line}, col {col}); reported output extents \
         {width}x{height}. Raw readback was {raw:?}. PNG kept at {}. If \
         PANE_ORIGIN_X_EXPECTED is still the -1.0 placeholder this failure is EXPECTED and is how \
         the constant is obtained: paste the derived origin above into the const, record it with \
         the date and the output size, and re-run.",
        metrics.cell_w,
        metrics.cell_h,
        out_png.display()
    );
    assert!(
        (origin_y - PANE_ORIGIN_Y_EXPECTED).abs() <= PANE_ORIGIN_TOLERANCE_PX + metrics.cell_h,
        "{smoke}: the editor pane's derived Y ORIGIN is {origin_y}, outside the band \
         {PANE_ORIGIN_Y_EXPECTED} +/- ({PANE_ORIGIN_TOLERANCE_PX} + cell_h). Same four candidate \
         causes as the X band, in the same order, but a Y-only shift points hardest at (2) — the \
         `adw::HeaderBar` height — since the header bar is the only thing above the pane. Derived \
         origin ({origin_x}, {origin_y}); expected ({PANE_ORIGIN_X_EXPECTED}, \
         {PANE_ORIGIN_Y_EXPECTED}); cell_w {}, cell_h {}; probe ({PROBE_X}, {PROBE_Y}) -> \
         (line {line}, col {col}); reported output extents {width}x{height}. Raw readback was \
         {raw:?}. PNG kept at {}. If PANE_ORIGIN_Y_EXPECTED is still the -1.0 placeholder this \
         failure is EXPECTED and is how the constant is obtained.",
        metrics.cell_w,
        metrics.cell_h,
        out_png.display()
    );

    (origin_x, origin_y)
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
/// Fourteen of the eighteen smokes appear here. FOUR contribute no rows at all, on
/// purpose, and it is worth being explicit about which and why:
///
/// * [`colon_qa_closes_the_window_under_cage`] captures no PNG and asserts only that
///   the app process is gone, so it has no OCR term a fixture filename could satisfy.
/// * [`click_positions_the_cursor_under_cage`],
///   [`drag_selects_a_range_under_cage`] and
///   [`wheel_scroll_moves_the_viewport_under_cage`] assert on FILES the captive
///   Neovim writes about its own state — exact `line('.')`/`col('.')`,
///   `visualmode()` plus `'<`/`'>`, and `line('w0')` — not on pixels. They still
///   capture and retain a screenshot for post-mortem, but no assertion reads it, so
///   no fixture filename can help or hurt them.
///
/// The fourth pointer smoke,
/// [`click_then_close_keeps_keyboard_input_alive_under_cage`], DOES OCR, and its two
/// terms are listed below. For it the OCR pair is corroboration (the close really
/// happened, the survivor kept rendering, the app was still drawing at all); the
/// load-bearing assertions are its two `ALIVE*` readback files.
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
    (
        WEDGE_OCR_STAY,
        "WEDGE_OCR_STAY",
        "click_then_close_keeps_keyboard_input_alive_under_cage",
    ),
    (
        WEDGE_OCR_CLICK,
        "WEDGE_OCR_CLICK",
        "click_then_close_keeps_keyboard_input_alive_under_cage",
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
/// ([`CHORD_CLOSE_OCR_KEEP`]) and `chordgone` ([`CHORD_CLOSE_OCR_GONE`]); and — added
/// with the pointer-harness unit — `wedgestay` ([`WEDGE_OCR_STAY`]) and `wedgeclick`
/// ([`WEDGE_OCR_CLICK`]). None is a substring of any [`SIDEBAR_FILLER_FILES`] entry,
/// and no filler may be renamed to make one so. Four smokes contribute no term:
/// [`colon_qa_closes_the_window_under_cage`] asserts on the cage exit status and
/// captures no image, and the click, drag and scroll pointer smokes assert on files
/// the captive Neovim writes about its own state rather than on pixels.
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
/// nothing to the smoke suite's runtime and — unlike the eighteen smokes — still
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

    // A fresh temp cwd, exactly like the other seventeen smokes. The app reads
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
/// ## Measured geometry (2026-08-02)
///
/// This is the smoke that derives [`PANE_ORIGIN_X_EXPECTED`] /
/// [`PANE_ORIGIN_Y_EXPECTED`]. On a **1280x720** cage output (the extents
/// `wolf-pointer-inject` reported on its `ready` line, read back and asserted rather
/// than assumed) at `WOLF_FONT_PT` = [`SMOKE_FONT_PT`]:
///
/// | quantity | value |
/// | --- | --- |
/// | cage output size | `1280x720` |
/// | `cell_w` / `cell_h` | `22.4765625` / `44.0` |
/// | probe `(640, 360)` -> Neovim cell | `line 8, col 18` |
/// | derived pane origin | `(257.8984375, 52.0)` |
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

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // A fresh temp cwd, exactly like the other seventeen smokes; see `make_temp_cwd`
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

    // The target is expressed RELATIVE to the probe, never as a magic pixel, so no
    // unknown geometry constant can make the delta assertion pass or fail
    // spuriously.
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
         (`app::font_pt_from_env`) and that SMOKE_FONT_FAMILY = {SMOKE_FONT_FAMILY:?} still \
         matches the private `app::FONT_FAMILY`. PNG kept at {}.",
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
/// false against nvim 0.12.2: it is NOT the case that "Vim enters visual mode only on
/// a drag, never on a press+release at two different cells". Both gestures produce
/// byte-identical state:
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

    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

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
    // Drag B releases over the SIDEBAR — outside the pane, but still inside the
    // output. That distinction is the whole point of the out-of-widget drag, so the
    // point still has to be a real output pixel.
    assert_inside_extents(
        SMOKE,
        "the DRAG-B release point",
        (DRAG_OUT_X, i64::from(PROBE_Y)),
        extents,
    );

    // --- Drag Z: the ROUND TRIP, asserted FIRST because it RAN first ---------
    //
    // This block, and only this block, discriminates `drag-update`. Its press and
    // release are at the SAME cell, so `on_pane_release` on its own cannot produce
    // the state asserted here: with no movement between `<LeftMouse>` and
    // `<LeftRelease>` Neovim never enters visual mode. MEASURED: with `on_pane_drag`
    // neutered to an immediate no-op, this smoke as it stood BEFORE Drag Z existed
    // — i.e. Drag A and Drag B alone — passed 3 runs out of 3. They cover
    // `on_pane_release`'s reconstruction and clamp paths, not the drag signal.
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
         on nvim 0.12.2, a press at one cell plus a release at a DIFFERENT cell enters charwise \
         visual mode all by itself, with identical marks. A DIFFERENT non-empty mode (`V` \
         linewise, CTRL-V blockwise) would instead mean the encoded modifiers are wrong. Raw \
         readback was {raw_z:?}; PNG kept at {}.",
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

    // Drag Z pressed AND released at the probe point, so its `'<` IS the probe cell —
    // which makes it the right readback to derive the pane origin from, exactly as
    // the click smoke derives it from its probe click. This also asserts
    // `'<`.line >= MIN_PROBE_CELL and `'<`.col >= MIN_PROBE_CELL.
    let (origin_x, origin_y) = assert_probe_cell_and_pane_origin(
        SMOKE,
        (z_start_line, z_start_col),
        &metrics,
        extents,
        &out_png,
        &raw_z,
    );

    // --- Drag A: inside the pane, exact delta -------------------------------
    //
    // Drag A pins the EXACT (DRAG_DY, DRAG_DX) delta. That is real coverage of
    // `on_pane_release`'s `drag_point` -> `start_point() + offset` -> `cell_at` path,
    // and it is NOT a `drag-update` discriminator — see Drag Z above.
    let raw_a = read_readback(&sel_a, SMOKE);
    let (mode_a, marks_a) = read_selection(&sel_a, SMOKE);
    let [a_start_line, a_start_col, a_end_line, a_end_col] = marks_a;

    assert!(
        mode_a == VISUAL_CHARWISE,
        "{SMOKE}: Drag A left `visualmode()` as {mode_a:?}, expected {VISUAL_CHARWISE:?} \
         (charwise). Drag Z passed, so `drag-update` IS firing; this instead says the press and \
         release of a plain down-and-right sweep did not put Neovim into visual mode at all. \
         Note that an empty string here is NOT the `drag-update` discriminator it was once \
         described as: measured on nvim 0.12.2, `<LeftMouse>` at one cell plus `<LeftRelease>` at \
         a DIFFERENT cell enters charwise visual mode WITHOUT any `<LeftDrag>`, with identical \
         marks and cursor. So an empty mode here means Neovim received neither the press nor the \
         release as a mouse event — check `pane_view.rs::on_pane_press` and `on_pane_release` \
         still call `send_pane_mouse`. A DIFFERENT non-empty mode (`V` linewise, CTRL-V blockwise) \
         would instead mean the encoded modifiers are wrong. Raw readback was {raw_a:?}; PNG kept \
         at {}.",
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
    // Where `:1` parks the viewport, and therefore what `line('w0')` must read before
    // any scrolling. A PRECONDITION: without it the increase below would not be
    // falsifiable.
    const TOPLINE_AT_REST: i64 = 1;

    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

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

    // Direction is asserted SEPARATELY from movement: down must strictly INCREASE
    // `line('w0')` and up must strictly DECREASE it, matching the sign convention
    // `mouse.rs::scroll_direction_is_inverted_from_macos` pins headlessly and
    // `crates/wolf-ffi/tests/mouse_scroll.rs` pins against real Neovim.
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

    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Create a fresh temp cwd and drop BOTH markdown targets into it. The app reads
    // `std::env::current_dir()` for `CoreConfig.cwd`, so cage is launched with this
    // directory as its working directory.
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

    // The scenario deliberately does NOT `exit 14` when either readback is missing:
    // here their ABSENCE is the observable, so the discrimination happens on this
    // side, where the message can be a real diagnosis.
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

    // OCR corroboration: the close actually happened, the survivor kept rendering,
    // and the app was still drawing at all. These two are corroboration; the
    // `ALIVE*` files above are the load-bearing assertions.
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
