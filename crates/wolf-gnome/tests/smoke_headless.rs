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
//! **Twenty-nine cage smokes** — one window-render smoke, two file-picker smokes,
//! one file-tree-sidebar smoke, five split-pane smokes, one empty-state smoke, one
//! new-untitled-buffer smoke, one command-palette smoke, two BUFFER-PICKER smokes
//! (Ctrl+Shift+E then a typed query, switching to the queried buffer; and
//! Ctrl+Shift+E then a BARE Enter, switching to the most-recently-used OTHER buffer
//! through the row-1 preselection), one
//! Ctrl+Shift+W
//! close-pane smoke (the accelerator path, as opposed to the split-pane suite's
//! `<C-w>c`), two quit smokes (`:q`
//! closes the focused pane, `:qa` closes the window), one keyboard-only CRASH /
//! auto-restart smoke, one keyboard-only STALL smoke (`SIGSTOP` the captive Neovim,
//! then make the buffer picker's blocking RPC elapse its budget so the
//! "stopped responding" overlay is raised), six POINTER smokes
//! (click-to-position, drag-to-select,
//! wheel scroll, the click-then-close focus-wedge regression, the crash
//! overlay + "Restart Neovim" button, and a divider drag surviving a pane
//! re-layout), and four STATE-PERSISTENCE smokes (a three-pane layout, a lone open
//! file, and a dragged divider's WEIGHTS each surviving a quit-and-relaunch, plus
//! the window title rendering the working directory) — each of which launches the
//! real binary under `cage`.
//!
//! The three relaunch smokes run **two app launches inside ONE `cage` instance**
//! (one scenario string, two sequential `"$WOLF_BIN" &` blocks), so `CAGE_LOCK`
//! semantics are unchanged and the count of compositors started is still one per
//! smoke.
//!
//! Seven of them additionally run **`wolf-pointer-inject`** inside cage —
//! this repo's own long-lived `zwlr_virtual_pointer_v1` client, see
//! `tools/wolf-pointer-inject/README.md` — started BEFORE the app so the seat
//! already carries `WL_SEAT_CAPABILITY_POINTER` when GTK does its first `wl_seat`
//! roundtrip, and driven over a FIFO with a poll-for-ack protocol. Four of them
//! assert on FILES THE CAPTIVE NEOVIM WRITES ABOUT ITS OWN STATE — `line('.')` /
//! `col('.')`, `visualmode()` plus `'<` / `'>`, `line('w0')`, and the per-window
//! `winwidth()` — rather than on OCR. Those are exact integers that no sidebar row
//! and no `:e` toast can satisfy, and OCR is structurally incapable of answering
//! "did the cursor land at exactly row 12, column 25".
//!
//! Twenty-four of the twenty-nine OCR a screenshot of the live window. The `:qa`
//! smoke deliberately captures no image and asserts on the cage exit status alone
//! (the app must be GONE, and gone with a ZERO exit status — a panic in the GTK main
//! loop makes a process vanish just as a clean shutdown does); the click, drag,
//! scroll and divider-relayout smokes still capture and still RETAIN a screenshot
//! for post-mortem, but assert on their readback files rather than on pixels. The
//! three relaunch smokes OCR **and** assert on a Neovim readback, and two of them
//! additionally assert on the `state.json` the first launch wrote.
//!
//! **Four pure guards**, which mechanically enforce invariants the cage smokes
//! depend on but cannot check themselves:
//!
//! * [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`] — no
//!   [`SIDEBAR_FILLER_FILES`] name may contain a term any smoke asserts on.
//! * [`temp_cwd_name_cannot_satisfy_any_smoke_assertion`] — the same rule aimed at a
//!   second fixture, the fixed components of the temp-cwd path
//!   ([`TEMP_CWD_BASENAME`] and [`TEMP_CWD_PREFIX`]), because the window title
//!   renders the app's working directory and the temp-dir NAME is therefore page
//!   text too. The path is deliberately shaped so the title is the CONSTANT
//!   `~/project` on every run — see [`make_temp_cwd`] and [`cage_command`] for the
//!   measured reason.
//! * [`restore_fixture_names_cannot_satisfy_their_own_assertions`] — the same rule
//!   aimed at a THIRD fixture, [`RELAUNCH_FIXTURE_NAMES`]: the SMOKE-OWNED files a
//!   smoke writes into its own working directory (the state-persistence ones were
//!   the first), which the tree sidebar renders by name. It also pins the two
//!   preconditions behind the window-title smoke's deliberate exemption.
//! * [`cage_is_only_spawned_through_the_isolating_helper`] — `cage` is spawned in
//!   exactly one place, [`cage_command`], which is the only thing that sets
//!   `WOLF_STATE_DIR` and so the only thing keeping the smokes off the developer's
//!   real `~/.local/state/wolf/` — and, since the shader-cache fix, the only thing
//!   that sets `XDG_CACHE_HOME` too (see [`smoke_shader_cache_dir`]).
//!
//! All four are compositor-free: none calls `require_smoke_env`, none takes
//! `CAGE_LOCK`, none spawns a subprocess and none needs any of the toolchain below,
//! so they still guard on a machine where the smoke toolchain is unavailable. They
//! live here, next to the consts and helpers they read, rather than in a file of
//! their own.
//!
//! So this target's summary line reads **`33 passed`**, not `29`: twenty-nine cage
//! smokes plus those four guards.
//!
//! ## A vacuously-passing check recorded here: the `wtype ""` window-map poll
//!
//! Every scenario opens with a loop that retries a no-op `wtype ""` until one
//! succeeds, and its comment used to claim that success "only happens once cage has
//! a mapped surface to deliver input to". **That claim is measured FALSE.**
//! Instrumented, the loop breaks at **`tries = 0` on every single run**: `cage`
//! advertises `zwp_virtual_keyboard_manager_v1` as soon as the compositor itself is
//! up, BEFORE any client surface maps, so the very first `wtype ""` always succeeds
//! and the `WAIT_TRIES = 75` budget (15 s) is never spent.
//!
//! So the poll does **not** gate input readiness, and `WAIT_TRIES` / `WAIT_INTERVAL`
//! are not an input-readiness budget. The ONLY real budget between spawning the app
//! and typing the first key is the fixed `SETTLE` sleep that follows the loop —
//! which is why `SETTLE` carries the measured margin table in [`SCENARIO`] and why
//! it was raised from `0.5` to `1.0`.
//!
//! The poll is deliberately **kept**, not deleted: it still catches the one thing it
//! can catch — a `cage` that never came up at all — and its `kill -0` arm still
//! catches an app that died before the scenario ever typed. It simply no longer
//! claims something it cannot prove.
//!
//! This is the EIGHTH vacuously-passing check this effort has found. It is recorded
//! HERE, beside the scenarios, because that is where the next reader meets it — while
//! editing a scenario's poll knobs, not while reading Markdown;
//! `crates/wolf-gnome/tests/README.md` keeps the running prose record of the others,
//! under "A red-run demonstration is a deliverable, not a formality". It is the same
//! shape as the entries already there (the drag smoke's false premise, the untested
//! auto-restart cap, the centre-derived restart-button click): **a check aimed at the right
//! property, looking somewhere the defect could not appear.**
//!
//! Its concrete cost was DIAGNOSTIC. A residual ~0.8% failure of
//! [`window_renders_typed_text_under_cage`] (1 in 120) was chased as input routing
//! and then as the swap modal, because a poll that appeared to have already proved
//! the client could accept keys ruled out the true cause — too little margin before
//! the first keystroke — by fiat. The measured signature (two empty lines, cursor on
//! line 2) is now the FIRST thing that smoke's failure message tells the reader to
//! check.
//!
//! ## When it runs
//!
//! The twenty-nine cage smokes run as part of the STANDARD suite — `cargo test -p
//! wolf-gnome` and `cargo test --workspace` — inside the flake devShell. They are
//! the Linux verification gate's only end-to-end proof that the GTK front-end draws
//! a window a human could see (see `build.md`, "The Linux verification gate").
//!
//! They are deliberately NOT `#[ignore]`d. "They are slow" is not a reason to skip
//! the only real end-to-end test the front-end has; per `CLAUDE.md` a test the
//! suite command does not run is a silently-skipped test.
//!
//! They are, however, the slowest part of the suite: each drives a real
//! compositor, and all twenty-nine serialize behind `CAGE_LOCK` (see below).
//!
//! ```text
//! nix develop --command cargo test -p wolf-gnome --test smoke_headless
//! ```
//!
//! ## Requirements of the twenty-nine cage smokes (from the devShell's smoke toolchain)
//!
//! - `cage` (headless wlroots compositor), `wtype`, `grim`, `tesseract` and
//!   `wolf-pointer-inject` on `PATH`. The last one is NOT an upstream package: it is
//!   built from this repo's own `tools/wolf-pointer-inject/` by `flake.nix`, so
//!   `command -v wolf-pointer-inject` succeeding can only mean the flake provided
//!   it. Only the seven pointer-driven smokes use it, but it is required
//!   unconditionally — see [`REQUIRED_TOOLS`].
//! - `WOLF_NVIM_BIN` pointing at the pinned Neovim (same contract as the other
//!   wolf-gnome tests; nvim is never taken from `PATH`).
//! - `XDG_RUNTIME_DIR` set and writable (the Wayland socket lives there).
//!
//! Per project policy every one of the twenty-nine FAILS LOUD if any of the above is
//! missing rather than silently skipping, mirroring `common::require_nvim_bin` — a
//! missing precondition means the smoke toolchain was not provisioned, which is a
//! setup bug we want surfaced, not hidden. Now that these tests run by default, that
//! hard fail matters more, not less: a missing tool means the suite was invoked
//! outside the devShell, and softening `require_smoke_env` to a skip would let the
//! gate pass green on an unprovisioned machine while proving nothing. Do not.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::SystemTime;

use cairo::{Context, Format, ImageSurface};
use wolf_core::state::STATE_FILE_NAME;
use wolf_core::{load_state, state_dir, CommandMru, PaneNode, PaneOrientation};
use wolf_gnome::{CellMetrics, WINDOW_TITLE_FALLBACK};

/// The lowercase word we type in insert mode (in the window smoke scenario).
/// Kept short and all-lowercase so OCR noise is unlikely to corrupt every
/// character. After typing it we move the cursor OFF its row (see [`SCENARIO`])
/// so the full word — not just a prefix — can be asserted against OCR.
const OCR_WORD: &str = "hello";

/// Serializes the twenty-nine cage smoke tests. Each launches its own
/// `cage` compositor, and two compositors started concurrently contend for the
/// same Wayland socket under `XDG_RUNTIME_DIR` (lockfile error), failing both.
/// Under the default multi-threaded test harness these tests would otherwise run
/// in parallel, so each acquires this lock for its whole body to force serial
/// execution. Poisoning is recovered from (`into_inner`) so one panicking test
/// does not cascade into failing the others.
///
/// The seven pointer-driven smokes join this same lock rather than living in a file of
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
/// ## The window title is page text too, and is deliberately held CONSTANT
///
/// Since the state-persistence unit the `adw::HeaderBar` renders the app's WORKING
/// DIRECTORY, so the title is part of the page `tesseract` calibrates against — a
/// third input to this fixture's arithmetic, alongside the filler rows and the
/// editor grid. It was measured to matter, and the measurement is recorded here
/// rather than left to be re-derived:
///
/// | arm | five consecutive capture md5s | size |
/// | --- | --- | --- |
/// | title rendering the run-varying temp path | all five DIFFERENT | 34.2–35.0 KB, varying |
/// | `set_title` suppressed (the base page) | `5ea4d070…` ×5, byte-identical | 26074 ×5 |
///
/// With a run-varying title — [`make_temp_cwd`] used to hand the app a directory
/// named from `std::env::temp_dir()` + the pid + a 19-digit nanosecond timestamp, so
/// the header bar drew a different string on every run —
/// [`window_renders_typed_text_under_cage`]'s OCR became STOCHASTIC: the isolated
/// leading `h` of [`OCR_WORD`] misclassified (`ello` / `ﬁello`, the documented
/// signature above) in 2 of ~93 runs. Before that title existed the capture was
/// bit-for-bit reproducible.
///
/// **The fix was not to raise this number.** [`make_temp_cwd`] and [`cage_command`]
/// now hold the rendered title at the fixed string `~/project` for every smoke (a
/// fixed-name cwd under a `HOME` that is its own parent), so the captured page is
/// byte-identical across runs again and the five-file floor measured above still
/// means what it meant when it was measured. Letting a run-varying path back into
/// the title reintroduces that ~2% flake, and raising this count would NOT fix it:
/// more filler files can only make a stochastic page misread less often, never
/// deterministically.
///
/// ## Why it belongs here at all
///
/// Before the per-test temp cwd landed, this smoke ran with `crates/wolf-gnome` as
/// the app's cwd and the sidebar happened to list ~35 repo entries, so the assertion
/// has ALWAYS implicitly depended on that page density — accidentally. Writing these
/// files makes the dependency explicit and deliberate, and mirrors what most other
/// smokes get incidentally from the fixture files they drop into their own temp
/// cwds. (Five drop no fixture files at all: the empty-state smoke hides the sidebar
/// entirely, the `:qa` smoke asserts on an exit status rather than OCR, and the
/// click, drag and scroll pointer smokes build their buffer with `:put` and assert
/// on Neovim-written readback files rather than on pixels.)
///
/// Several smokes now write these fillers DELIBERATELY rather than relying on their
/// own fixtures for density — the keyboard crash-recovery smoke and all three
/// state-persistence smokes, the last of which
/// ([`window_title_shows_the_working_directory_under_cage`]) asserts on a single
/// short header-bar title over an otherwise thin page.
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
/// `quitkeep`/`quitgone`, `chordkeep`/`chordgone`, — added with the
/// pointer-harness unit — `wedgestay`/`wedgeclick`, and — added with the
/// state-persistence unit — `restorealfa`/`restorebravo`/`restorecharlie`,
/// `solozulu`/`soloalfa` and `titlezulu`/`titlehome`, and — added with the
/// buffer-picker unit — `bufpickalfa`/`bufpickbravo`, `responding` and
/// `stallfixture` — so a stray filler can never
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
// temp dir: `cage_command`'s `current_dir` plus this smoke's own `.env("HOME",
// &cwd)`. That fires the production rule with no test-only env var, and keeps every
// remaining `$HOME`-derived path inside the temp dir too. (`state_dir` is no longer
// one of them: `cage_command` pins it with `WOLF_STATE_DIR`, which `state_dir()`
// resolves ahead of anything `$HOME`-derived, so the MRU sqlite and the swap dir land
// in the cwd's `-state` sibling — see `smoke_state_dir`.)
//
// `cage_command` now sets a DEFAULT `HOME` of its own — the cwd's parent, which pins
// every other smoke's header-bar title to the constant `~/project` — and this smoke
// deliberately overrides it. The override is what makes the smoke mean anything: with
// the default, `cwd != home` and the app would open a `Folder`, so the status page
// would never be built. It is chained after `cage_command`, so it wins.
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
// Command-palette smoke: Ctrl+Shift+P opens the palette, a typed query narrows it
// to ONE row, and Enter runs that row's command.
//
// This proves the whole palette path in one shot: the accelerator -> `gio` action ->
// `UiAction::OpenCommandPalette` -> the overlay maps and takes keyboard focus -> the
// search entry ranks `COMMAND_SPECS` -> Enter records the row's MRU key, closes the
// overlay, and hands the command to the shell's `Command -> UiAction` funnel.
//
// It is shaped exactly like the new-untitled-buffer smoke above, and for the same
// reason: the command the palette runs is `NewBuffer`, so "the palette really
// dispatched something" is observable as the base file's heading DISAPPEARING from
// the capture. The chord differs (Ctrl+Shift+P, not Ctrl+Shift+N) and the route
// differs (through the palette's ranked list, not straight to the action), which is
// what makes this a separate smoke rather than a duplicate.
//
// ## Why the fixture NAME and the asserted TERMS are deliberately unlike
//
// The tree sidebar renders every cwd entry by name and nvim's `:e` echoes the
// filename, so a fixture named after an asserted term satisfies that assertion
// whether or not the feature did anything — the permanent silent pass documented on
// `PICKER_FILE_NAME` and `SIDEBAR_FILLER_FILES`. `PALETTE_FILE` is therefore
// `pbase.md` while the term asserted ABSENT is the heading word `palcmdbase`: the
// filename's lowercase does not contain it, so neither the sidebar row nor the `:e`
// toast can keep assertion 3 green once the buffer really has been replaced.
// `restore_fixture_names_cannot_satisfy_their_own_assertions` re-checks that
// mechanically, in both directions, because this comment and the assertion it
// protects sit thousands of lines apart.
//
// ## Why the query is DETERMINISTIC
//
// Among the eight titles the palette lists — `Close`, `Close Window`, `Go to File…`,
// `Minimize`, `New`, `Open File…`, `Open Folder…`, `Quit Wolf` — `new` is a
// case-insensitive SUBSEQUENCE of exactly one, `New`. (`Close Window` has an `n` with
// no later `e`; `Minimize` and both `Open …` titles have `n` then `e` but no later
// `w`; the rest have no `n` at all.) So the list holds one row, `restored_index`
// selects it, and Enter activates it. That holds regardless of MRU state, because
// `rank_matches` drops non-matching entries by its AND filter BEFORE the MRU
// ordering applies. Unfiltered the query would match three (`New Window` and
// `New Tab` are both Deferred), so it is the palette's classification filter that
// makes it unique — which is why the premise is pinned executably by
// `palette::tests::palette_rows_query_new_selects_exactly_the_new_buffer_row` rather
// than left as prose here.

/// Command-palette smoke: the file opened BEFORE the chord (the one that must
/// vanish when the palette runs New).
const PALETTE_FILE: &str = "pbase.md";

/// Heading written into [`PALETTE_FILE`]; lowercased it contains
/// [`PALETTE_OCR_ABSENT`]. Deliberately unlike the filename so nvim's `:e` toast and
/// the sidebar row cannot satisfy the NOT-contains assertion.
const PALETTE_HEADING: &str = "# PALCMDBASE";

/// Lowercased substring OCR must NOT find: the heading of the file that was open
/// before the palette dispatched `NewBuffer`.
const PALETTE_OCR_ABSENT: &str = "palcmdbase";

/// Distinctive marker typed (in insert mode) into the buffer the palette's command
/// created. Its lowercased form is [`PALETTE_OCR_PRESENT`]. Short/all-uppercase to
/// survive OCR, mirroring [`NEWBUF_MARKER`].
const PALETTE_MARKER: &str = "ZZPALRUN";

/// Lowercased substring OCR MUST find: proof that the text typed after the palette
/// closed reached a buffer at all. It does NOT prove focus was returned to the
/// editor — that revert was measured green; see the revert table on
/// [`command_palette_chord_runs_the_selected_command_under_cage`].
const PALETTE_OCR_PRESENT: &str = "zzpalrun";

/// Typed into the palette's search entry. See the block comment above for why it
/// narrows the list to exactly one row, and for the unit test that pins that.
const PALETTE_QUERY: &str = "new";

/// The canonical `CommandSpec::id` the activated row must record in the command MRU
/// (`<state dir>/command-mru.sqlite`). Nothing but a palette activation writes that
/// store — `wolf_core::api`'s `command_mru()` doc states it outright — so a hit for
/// this key can only have come from the palette running the row.
const PALETTE_MRU_KEY: &str = "new_buffer";

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
// Crash / restart smokes: fixtures, OCR terms and the failing-respawn wrapper.
//
// Two smokes share this block:
//
//  * `nvim_crash_recovers_and_reopens_the_file_under_cage` (keyboard only) —
//    SIGKILLs the captive Neovim and asserts the app auto-restarts it, reopens the
//    Tier-1 file set with the ACTIVE path last, recovers the UNSAVED edit from the
//    swap file, flashes the banner, and holds the pane grid at its pre-crash size.
//  * `crash_overlay_and_restart_button_recover_under_cage` (pointer) — makes the
//    respawn FAIL so the overlay dwells, asserts the overlay is up and OPAQUE, then
//    clicks the "Restart Neovim" button and asserts a real recovery.
//
// The FOUR OCR terms below are the only new ones this unit adds, and each is
// listed in `ASSERTED_OCR_TERMS` so
// `sidebar_filler_names_cannot_satisfy_any_smoke_assertion` guards it.

/// Crash-smoke fixture file, dropped into both smokes' temp cwd.
///
/// ## Vacuity analysis (the fixture-naming rule, applied)
///
/// The name must NOT contain [`RESTART_OCR_HEADING`], and `bfile` does not: the tree
/// sidebar renders every cwd file BY NAME and nvim's `:e` toast echoes the filename
/// too, so a fixture called `bounceback.md` would satisfy every `bounceback`
/// assertion in both smokes whether or not Neovim was ever restarted — the exact
/// silent false positive that let `picker_opens_and_loads_selected_file_under_cage`
/// pass while the picker provably never opened (see [`PICKER_FILE_NAME`]).
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
/// `(PROBE_X, PROBE_Y)` must land on a REAL cell — measured `line 8, col 18` — or
/// [`assert_probe_cell_and_pane_origin`]'s `MIN_PROBE_CELL` precondition fires and
/// the whole pane-origin derivation (and therefore the button point) means nothing.
/// A one-line fixture would clamp every click to `(1, 1)`. Nineteen filler lines
/// plus the heading is twenty buffer lines, comfortably more than the measured
/// `winheight(0)` of 14.
const RESTART_FILLER_LINE_COUNT: usize = 19;

/// How many `x`s each filler line carries after its three-digit ordinal, so every
/// filler line is exactly [`FIXTURE_LINE_WIDTH`] characters — the same width, and
/// for the same two reasons, as the `:put` fixture in [`POINTER_PREAMBLE`]: wide
/// enough that a click at column 18 is not clamped to end-of-line, and narrow enough
/// that `opt.wrap` / `opt.linebreak` never soft-wrap it.
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
const RESTART_OCR_HEADING: &str = "bounceback";

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
const RESTART_OCR_BANNER: &str = "restarted";

/// Lowercased substring OCR must find to prove the crash OVERLAY is up.
///
/// ## Vacuity analysis
///
/// It appears only in the two crash-overlay titles. It is not producible by the
/// editor grid, by the sidebar (mechanically guarded by
/// [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`], and by the
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
const RESTART_OCR_OVERLAY: &str = "crashed";

/// The UNSAVED EDIT smoke A makes before the crash: APPENDED (`A`) to the end of
/// **line 2** of [`RESTART_FILE_NAME`] — the first filler line — and then flushed to
/// that buffer's swap file with `:preserve`. All-uppercase and word-like, mirroring
/// [`FOCUS_MARKER`] and [`NEWBUF_MARKER`]; lowercased, the rendered line contains
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
/// composed width against the `winwidth(0)` Neovim itself reported (measured 45)
/// rather than against any assumed number — so the derivation cannot rot silently.
/// If the fixture or the marker ever grows past the reported width, shorten one of
/// them; do not weaken the assertion.
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
/// produce it. Checked against every [`ASSERTED_OCR_TERMS`] entry in BOTH directions
/// — `unsavedburst` contains none of them and none of them contains it — and against
/// all nine [`SIDEBAR_FILLER_FILES`] names, which
/// [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`] then re-checks
/// mechanically.
///
/// ## Why the CONTROL capture's must-NOT-contain half is not optional
///
/// The control is taken while everything is still healthy, BEFORE the edit is typed,
/// so this term must be absent from it. Without that half, a marker that leaked into
/// the fixture body, into a filler name or onto a sidebar row would satisfy the
/// post-recovery assertion vacuously — the same silent false positive documented on
/// [`PICKER_FILE_NAME`], and the reason both halves are asserted rather than one.
const RESTART_OCR_UNSAVED: &str = "unsavedburst";

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
/// It is still made of `z`s, and must still contain no [`ASSERTED_OCR_TERMS`] entry:
/// the GTK cmdline strip may still be showing the dead Neovim's stale text when the
/// post-recovery capture is taken.
const RESTART_BURST_TEXT: &str = "zzzz zzzz zzzz zzzz zzzz zzzz zzzz zzzz";

/// Smoke B's `/bin/sh` wrapper, written into the temp cwd and handed to the app as
/// `WOLF_NVIM_BIN` **on the `cage` `Command` only**.
///
/// The name must not contain any [`ASSERTED_OCR_TERMS`] entry, because the tree
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
/// `/proc` scan is structurally blind to it — MEASURED in pass 5: with the cap set
/// to `u32::MAX` the scan-only check left smoke B fully GREEN. Same naming
/// constraint as the wrapper: the tree sidebar lists it.
const RESTART_ATTEMPTS_NAME: &str = "spawnlog";

/// Cage output WIDTH, in pixels, that smoke B derives its button click against.
///
/// MEASURED 2026-08-02, same run as [`PANE_ORIGIN_X_EXPECTED`]: `1280x720`. It is
/// pinned here — and NOT merely assumed — because the button point has to be
/// computed BEFORE `cage` is spawned, while the authoritative extents only reach the
/// Rust side afterwards, on the helper's `ready` line. Smoke B therefore reads the
/// real extents with [`read_output_extents`] after the run and asserts they equal
/// these constants, so a changed output size fails loudly with a message that names
/// the consequence instead of silently retargeting a click.
const CAGE_OUTPUT_W_EXPECTED: i64 = 1280;

/// Cage output HEIGHT, in pixels. See [`CAGE_OUTPUT_W_EXPECTED`].
const CAGE_OUTPUT_H_EXPECTED: i64 = 720;

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

// ---------------------------------------------------------------------------
// Buffer-picker smokes: fixtures, the query, and the MRU-order readback.
//
// Ctrl+Shift+E snapshots the captive Neovim's listed buffers, ranks them against
// the typed query, and `Enter` switches the active window to the ranked row's
// buffer. Three files are opened in a known order so the picker's list has a known
// shape, and the smoke then types a query that matches exactly ONE of them.
//
// ## TWO smokes share this one fixture set, and pull it in opposite directions
//
// `buffer_picker_chord_switches_to_the_queried_buffer_under_cage` types
// `BUFPICK_QUERY` and asserts the TARGET's heading is on screen and the PRESELECT's
// is not. `buffer_picker_bare_enter_switches_to_the_previous_buffer_under_cage`
// types nothing at all and asserts the exact opposite for those two terms, plus the
// absence of the CURRENT file's heading.
//
// That opposition is what makes the pair discriminating rather than either alone.
// `BUFPICK_OCR_ABSENT` is asserted PRESENT by the bare-Enter smoke, so a green run
// of it is direct evidence that the term is producible on this page at all — and a
// green run of the query smoke, with the same three fixtures in the same cwd, is
// direct evidence that no sidebar row, no `:e` echo and no header-bar segment
// produces it on its own. Neither smoke has to argue that from prose.
//
// ## Why THREE files and not two
//
// `buffer_picker::initial_selection` preselects ROW 1 — the most-recently-used
// OTHER buffer — so with two files the target would BE the preselected row and a
// picker that ignored the typed query entirely would still activate it. The query
// smoke would pass permanently while proving nothing about filtering, and the
// bare-Enter smoke's must-be-absent target term would name the row it expects to
// land on. The third file pushes the target to index 2 or later, which is what
// makes the query the only thing that can select it. That premise is not assumed:
// each scenario reads the real `getbufinfo()` order back out of Neovim and the Rust
// side asserts it (see `BUFPICK_READBACK_NAME`), and the same arithmetic is pinned
// headlessly by
// `buffer_picker::tests::buffer_rows_for_the_smoke_query_select_exactly_the_target`.
//
// ## Why the fixture NAMES and the asserted TERMS are deliberately unlike
//
// The tree sidebar renders every cwd entry by name and nvim's `:e` echoes the
// filename, so a fixture named after an asserted term satisfies that assertion
// whether or not the feature did anything — the permanent silent pass documented
// on `PICKER_FILE_NAME`. The two vocabularies here are therefore disjoint by
// construction: the files are `bp*` and the asserted words are `bufpick*`, so the
// QUERY matches the file's NAME while the ASSERTION reads the file's CONTENT. All
// four filenames are registered in `RELAUNCH_FIXTURE_NAMES` — once each, since the
// two smokes share them and a duplicate row would trip that guard's own
// distinct-names check — and all THREE terms in `ASSERTED_OCR_TERMS`, so the three
// mechanical guards prove that rather than leaving it to this comment.

/// Buffer-picker smoke: the file opened FIRST, and the one [`BUFPICK_QUERY`] must
/// select. Opened first, so it is the OLDEST of the three by `lastused` and lands
/// at index 2 or later in the picker's MRU-first list.
const BUFPICK_FILE_TARGET: &str = "bpzulu.md";

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
const BUFPICK_OCR_PRESENT: &str = "bufpickalfa";

/// Buffer-picker smoke: the file opened SECOND — the row
/// `buffer_picker::initial_selection` preselects, and therefore the row a picker
/// that ignored the typed query would activate.
const BUFPICK_FILE_PRESELECT: &str = "bpyankee.md";

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
const BUFPICK_OCR_ABSENT: &str = "bufpickbravo";

/// Buffer-picker smoke: the file opened THIRD — the buffer showing when the chord
/// fires, and therefore row 0 of the picker's MRU-first list.
const BUFPICK_FILE_CURRENT: &str = "bpxray.md";

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
const BUFPICK_OCR_CURRENT: &str = "bufpickcharlie";

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
/// One const for both smokes, and one row in [`RELAUNCH_FIXTURE_NAMES`]: each runs in
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
const BUFPICK_READBACK_NAME: &str = "bp_buffers.txt";

// ---------------------------------------------------------------------------
// Stall smoke: the fixture, the terms, and the RPC timeout.
//
// This is the first end-to-end coverage the STALL half of the crash/stall overlay
// has, and it is reachable only because the buffer picker exists: `list_buffers`
// is the one blocking RPC a RUNNING wolf-gnome issues, so wedging the captive
// Neovim with SIGSTOP and then firing Ctrl+Shift+E is what makes `block_on_rpc`
// elapse its budget, mark the core NotResponding, and raise the overlay.
//
// The scenario captures AFTER the block, never during it: no frame is committed
// while the main loop is blocked, so a capture taken then would return the
// pre-chord frame and prove nothing.

/// Stall smoke: the fixture opened before the wedge, so the editor has something
/// distinctive on screen for the opaque overlay to cover.
///
/// Its name contains neither asserted term, for the reason documented on
/// [`PICKER_FILE_NAME`]: the sidebar renders it and nvim's `:e` echoes it, so a
/// name carrying [`STALL_OCR_ABSENT`] would break the opacity assertion no matter
/// how opaque the overlay was.
const STALL_FILE: &str = "stfrozen.md";

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
const STALL_OCR_ABSENT: &str = "stallfixture";

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
/// sidebar or a Neovim toast produces it either, and the three fixture-name guards
/// re-check that mechanically.
const STALL_OCR_PRESENT: &str = "responding";

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

// ---------------------------------------------------------------------------
// Divider-drag smokes: finding a `gtk::Paned` handle without hardcoding a pixel.
//
// A divider is the one target in this file that NO arithmetic here can predict.
// Its handle is theme-owned (Adwaita's non-wide `paned > separator` is a single
// pixel, widened by GTK's own invisible grab area), and its position is
// `round(max_position * fraction)` where `max_position` has ALREADY subtracted the
// handle and the far child's minimum — so a position modelled from the pane origin
// and the cell size is out by tens of pixels before it starts, which is more than a
// handle width.
//
// So the two divider smokes BISECT for it, against the only authority available:
// which Neovim window a click focuses. That is exact and theme-independent. The
// modelled position below is a SEED and only a seed; see the `bisect_divider` shell
// function in [`POINTER_DIVIDER_BODY`] for the protocol, including why each probe
// is preceded by a RESET click.

/// How far either side of the MODELLED divider position each bisection starts.
///
/// The model assumes equal shares of the editor pane's width, but a `gtk::Paned`
/// positions its divider at a fraction of its TRAVEL (`max_position`), which
/// excludes the handle and the far child's minimum — worth ~15-20 px per divider at
/// this output size. This margin only has to exceed that error while staying INSIDE
/// the neighbouring panes, which are ~340 px wide in the three-pane case.
const BISECT_SEED_MARGIN_PX: i64 = 128;

/// How many halvings each bisection runs.
///
/// The seeds span `2 * BISECT_SEED_MARGIN_PX` = 256 px, so seven halvings leave a
/// two-pixel residual — comfortably inside a `GtkPaned` handle's grab area, which is
/// several pixels wider than the one-pixel separator Adwaita draws. The drag's own
/// first assertion (the panes either side of it really moved) is what proves the
/// press landed on the handle; this constant only has to make that likely.
const BISECT_STEPS: i64 = 7;

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

/// Pixels the single divider is dragged right in
/// [`dragged_divider_weights_survive_relaunch_under_cage`].
///
/// Sized so BOTH panes stay wide enough to render their whole heading without
/// soft-wrapping (`runtime/nvim/init.lua` sets `wrap`/`linebreak`, and a wrapped
/// heading breaks the OCR corroboration): the narrower pane keeps ~15 columns
/// against a 14-character heading.
const DIVIDER_RELAUNCH_DRAG_PX: i64 = 175;

/// How far apart three nominally-equal columns may be, in cells, before the
/// baseline precondition fails.
///
/// They are not exactly equal and cannot be: each `gtk::Paned` positions its
/// divider at a fraction of its own TRAVEL, which subtracts the handle and the far
/// child's minimum, so a chain of them lands the columns within about a cell of
/// each other rather than identical. The widths themselves are exact —
/// `winwidth()` reports the grid size the front-end asked for, and
/// `nvim_ui_try_resize_grid` was MEASURED not to reflow Neovim's own window layout,
/// so resizing one pane's grid cannot take columns from another's.
const BASELINE_TOLERANCE_CELLS: i64 = 2;

/// The smallest width change, in cells, that counts as "this drag moved the
/// divider". Three cells is ~67 px against drags of 150+ px, so it discriminates a
/// drag that did nothing from one that landed.
const MIN_DRAG_CELLS: i64 = 3;

/// How far a PRESERVED width ratio may drift across a re-layout or a relaunch.
///
/// The ratio is computed from cell counts, which quantize: one cell is ~0.03 of a
/// two-pane ratio, so this allows a couple of cells of rounding while staying far
/// below the ~0.2 gap that separates a preserved ratio from a re-centred one.
const RATIO_TOLERANCE: f64 = 0.08;

/// How far from `0.5` a preserved ratio must sit for "it did not change" to mean
/// anything at all.
///
/// Without this clause a divider that snapped back to the centre would satisfy
/// "unchanged" whenever the drag never took effect. It is the same non-vacuity
/// discipline the fixture-naming guards apply, aimed at an arithmetic assertion.
const MIN_RATIO_OFFSET: f64 = 0.08;

/// The smallest gap between the two PERSISTED weights that no equal-share vector
/// could show. `[0.5, 0.5]` — everything this front-end could write before divider
/// drag — has a gap of exactly zero.
const MIN_WEIGHT_GAP: f64 = 0.15;

/// Tolerance on the persisted weights' sum, which the pure tree renormalizes to 1.
const WEIGHT_SUM_EPS: f64 = 1e-9;

/// External binaries the scenario shells out to. All come from the flake's
/// smoke toolchain; absence means the devShell was not entered.
///
/// `wolf-pointer-inject` is the odd one out and deliberately so: it is not an
/// upstream nixpkgs package but THIS REPO'S OWN tool, built from
/// `tools/wolf-pointer-inject/` by `flake.nix`'s `wolfPointerInject` derivation and
/// put on the devShell `PATH`. The name is repo-namespaced precisely so that
/// `command -v wolf-pointer-inject` succeeding can only mean the flake provided it —
/// a bare `pointer-inject` could be shadowed by something on the host. It is
/// required unconditionally, not just by the seven pointer-driven smokes: a devShell missing
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
/// - Poll, bounded by `WAIT_TRIES * WAIT_INTERVAL`, so a compositor that never
///   came up (or an app that died on launch) fails fast instead of hanging. This
///   poll used to be described as waiting "for the window to map"; **it does not**
///   — `cage` advertises `zwp_virtual_keyboard_manager_v1` before any surface
///   maps, so the probe succeeds at `tries = 0` on every run. See the module doc
///   comment's record of that vacuity, and the `SETTLE` block below, which is the
///   only real budget before the first keystroke.
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

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
#
# SETTLE -- READ THIS BEFORE TRIMMING IT. This is the CANONICAL copy of this
# comment; the other eighteen scenarios carry the same value and point back here.
#
# SETTLE is the ONLY real budget between spawning the app and typing the first
# key. The wtype poll below LOOKS like an input-readiness gate and is not one:
# cage advertises zwp_virtual_keyboard_manager_v1 as soon as the compositor is
# up, before any surface maps, so instrumented that loop breaks at tries=0 on
# every single run and WAIT_TRIES is never spent. See this file's module doc
# comment, which records that as the eighth vacuously-passing check found here.
#
# Measured on this host: 20 iterations per cell, identical code, the ONLY
# variable being whether the mesa shader cache under XDG_CACHE_HOME was warm.
#
#   SETTLE | cold cache | warm cache
#   -------|------------|-----------
#   0.15   |  0/20      |  0/20
#   0.20   |  --        |  0/20
#   0.25   |  0/20      | 20/20
#   0.30   |  0/20      |  --
#   0.35   | 20/20      | 20/20
#   0.40   | 20/20      |  --
#
# Thresholds: warm in (0.20, 0.25], cold in (0.30, 0.35]. A cold cache costs
# ~4.9 MB of shader rebuild (mesa_shader_cache + radv_builtin_shaders) on every
# launch. At the old SETTLE=0.5 that left only ~1.5x headroom cold, and a
# 120-iteration loop found 1 failure (~0.8%) whose capture md5
# c9a797d31d606335297b40823e64371a is BYTE-IDENTICAL to a deliberately induced
# cold/0.25 failure: two empty lines with the cursor on line 2, i.e. the whole
# first `wtype -- "i$OCR_WORD"` and its Escape delivered before the client could
# accept keys, and lost. `cage_command` now pins XDG_CACHE_HOME to a SHARED warm
# directory, which deletes the cold column at its source; 1.0 is ~3x the measured
# cold threshold on top of that. Do not trim it back.
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up.
#
# This is NOT an input-readiness gate, despite how it reads. A no-op `wtype ""`
# succeeds as soon as cage advertises zwp_virtual_keyboard_manager_v1, which it
# does before any surface maps -- measured, this loop breaks at tries=0 on every
# single run, so WAIT_TRIES / WAIT_INTERVAL never gate anything here. It is kept
# rather than deleted because it still catches a cage that never came up at all,
# and the `kill -0` arm still catches an app that died on launch. The real budget
# before the first keystroke is the SETTLE above.
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
/// (same structure as [`SCENARIO`]): launch the app, poll until `cage` is up (NOT
/// until the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle, open the
/// picker with the Ctrl+Shift+O trigger, type a query that matches the target file,
/// activate the selection with Enter, settle, and screenshot.
///
/// The app derives `CoreConfig.cwd` from `std::env::current_dir()`, so the target
/// `.md` is placed in the cage process's working directory (set via
/// `Command::current_dir` by the caller) — NOT created from inside this script.
const PICKER_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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

/// The interactive picker KEYBOARD-NAVIGATION scenario, run as a single `sh -c`
/// string INSIDE `cage` (same structure as [`PICKER_SCENARIO`]): launch the app,
/// poll until `cage` is up (NOT until the window maps — see [`SCENARIO`]'s `SETTLE`
/// comment), settle, open the picker with Ctrl+Shift+O, type a query that matches
/// BOTH navigation files, press `Down` once to move the list selection to the
/// SECOND row, then `Enter` to open it, settle, and screenshot.
///
/// Two files are created in the cage process's working directory by the caller
/// (`pick1.md` => row 0, `pick2.md` => row 1; see the rank-tiebreak comment near
/// the `PICKER_NAV_*` consts). Down+Enter must therefore open `pick2.md`.
const PICKER_NAV_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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
/// structure as [`SCENARIO`]): launch the app, poll until `cage` is up (NOT until
/// the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle, then wait a
/// little longer for the long-lived FileSearch to stream results into the tree, and
/// screenshot. No keyboard interaction at all — we only assert the
/// sidebar lists the file by name, which depends purely on tree population from
/// the app's cwd (set via `Command::current_dir` by the caller).
const TREE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0
# Extra settle for the streamed FileSearch traversal to populate the tree.
TREE_SETTLE=1.5

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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
/// structure as [`SCENARIO`]): launch the app, poll until `cage` is up (NOT until
/// the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle, open
/// file A (`:e $VSPLIT_FILE_LEFT<CR>`) in the editor pane, trigger a vertical split
/// with `<C-w>v`, open file B (`:e $VSPLIT_FILE_RIGHT<CR>`) in the new pane, settle,
/// and screenshot. Both files live in the cage process's working directory (set via
/// `Command::current_dir` by the caller) — NOT created from inside this script.
const VSPLIT_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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
/// (same structure as [`VSPLIT_SCENARIO`]): launch the app, poll until `cage` is up
/// (NOT until the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle,
/// open file A (`:e $HSPLIT_FILE_TOP<CR>`) in the editor pane, trigger a
/// HORIZONTAL split with `<C-w>s`, open file B (`:e $HSPLIT_FILE_BOTTOM<CR>`) in the
/// new (stacked) pane, settle, and screenshot. Both files live in the cage
/// process's working directory (set via `Command::current_dir` by the caller).
const HSPLIT_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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
/// structure as [`VSPLIT_SCENARIO`]): launch the app, poll until `cage` is up (NOT
/// until the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle,
/// open file A in the editor pane, vertical-split (`<C-w>v`, focus moves to the new
/// RIGHT pane), open file B there, move focus back LEFT (`<C-w>h`), then type the
/// marker (`i$FOCUS_MARKER<Esc>`) into the now-focused pane, settle, and screenshot.
/// Both files live in the cage process's working directory (set by the caller).
const FOCUS_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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
/// structure as [`VSPLIT_SCENARIO`]): launch the app, poll until `cage` is up (NOT
/// until the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle,
/// open file A in the editor pane, vertical-split (`<C-w>v`, focus moves to the new
/// RIGHT pane), open file B there, then close the focused (right) pane with
/// `<C-w>c`, settle, and screenshot. Both files live in the cage process's working
/// directory (set by the caller).
const CLOSE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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
/// structure as [`VSPLIT_SCENARIO`]): launch the app, poll until `cage` is up (NOT
/// until the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle,
/// open file A, vertical-split (`<C-w>v`) and open file B in the new RIGHT pane,
/// focus back LEFT (`<C-w>h`), vertical-split AGAIN and open file C in the new
/// sub-pane — flattening into one row of three columns [A, C, B] under the n-ary
/// model. Park the cursor off the text (`o`+Escape) and screenshot. All three
/// files live in the cage process's working directory (set by the caller).
const TRISPLIT_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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
/// structure as [`TREE_SCENARIO`]): launch the app, poll until `cage` is up (NOT
/// until the window maps — see [`SCENARIO`]'s `SETTLE` comment),
/// settle, and screenshot. No keyboard interaction at all — the assertion is purely
/// that the "no folder open" status page rendered.
///
/// The caller launches cage with `HOME` set to the same fresh temp directory it
/// uses as the working directory, which is what makes `workspace::launch_decision`
/// take its `cwd == home` branch (see the notes near [`EMPTY_STATE_OCR`]). That is
/// an explicit OVERRIDE of [`cage_command`]'s default `HOME` (the cwd's parent),
/// chained after it so it wins.
const EMPTY_STATE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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
/// (same structure as [`VSPLIT_SCENARIO`]): launch the app, poll until `cage` is up
/// (NOT until the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle,
/// open the base file (`:e $NEWBUF_FILE<CR>`), fire the Ctrl+Shift+N
/// accelerator, type the marker into the fresh buffer, park the cursor off the
/// marker's row, settle, and screenshot. The base file lives in the cage process's
/// working directory (set via `Command::current_dir` by the caller).
const NEWBUF_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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

/// The COMMAND-PALETTE scenario, run as a single `sh -c` string INSIDE `cage` (same
/// structure as [`NEWBUF_SCENARIO`]): launch the app, poll until `cage` is up (NOT
/// until the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle, open the
/// base file (`:e $PALETTE_FILE<CR>`), fire the Ctrl+Shift+P accelerator, type the
/// query, press Enter to activate the sole ranked row, then type the marker into the
/// buffer that row's command created, park the cursor off the marker's row, settle,
/// and screenshot. The base file lives in the cage process's working directory (set
/// via `Command::current_dir` by the caller).
const PALETTE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome command-palette smoke: app process exited early (pid $app_pid)" >&2
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
/// structure as [`PALETTE_SCENARIO`]): launch the app, poll until `cage` is up (NOT
/// until the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle, open the
/// three fixtures in a known order with a gap between them, read the resulting
/// buffer list back out of Neovim, fire Ctrl+Shift+E, type the query, press Enter,
/// park the block cursor off the asserted row, settle, and screenshot.
///
/// Standalone — it does NOT need [`with_nvim_pid_helper`], because nothing here
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
/// * `11` — the app died before its window mapped (the shared convention).
/// * `14` — the buffer-list readback the captive Neovim was told to write never
///   appeared, so the premise below cannot be checked at all.
const BUFPICK_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
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

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome buffer-picker smoke: app process exited early (pid $app_pid)" >&2
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
/// child, so it does not need [`with_nvim_pid_helper`].
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
/// * `11` — the app died before its window mapped (the shared convention).
/// * `14` — the buffer-list readback the captive Neovim was told to write never
///   appeared, so the premise below cannot be checked at all.
const BUFPICK_BARE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
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

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome buffer-picker bare-Enter smoke: app process exited early (pid $app_pid)" >&2
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

/// The `:q` CLOSE-PANE scenario, run as a single `sh -c` string INSIDE `cage`
/// (mirrors [`CLOSE_SCENARIO`] step for step): launch the app, poll until `cage` is
/// up (NOT until the window maps — see [`SCENARIO`]'s `SETTLE` comment), settle,
/// open file A in the editor pane, vertical-split (`<C-w>v`, focus moves to
/// the new RIGHT pane), open file B there, then close the focused pane with the Ex
/// command `:q` instead of `<C-w>c`, settle, and screenshot. Both files live in the
/// cage process's working directory (set by the caller).
const COLON_Q_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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
/// launch the app, poll until `cage` is up (NOT until the window maps — see
/// [`SCENARIO`]'s `SETTLE` comment), settle, type `:qa<CR>`, then POLL
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

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for the app to EXIT after :qa. Sized like
# WAIT_TRIES because the shutdown path (close-request -> picker/tree close ->
# WolfCore::shutdown -> nvim exit) runs under the same parallel-suite load.
QUIT_TRIES=75

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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
/// `cage` (mirrors [`CLOSE_SCENARIO`] step for step): launch the app, poll until
/// `cage` is up (NOT until the window maps — see [`SCENARIO`]'s `SETTLE` comment),
/// settle, open file A in the editor pane, vertical-split (`<C-w>v`, focus
/// moves to the new RIGHT pane), open file B there, then close the focused pane
/// with the Ctrl+Shift+W ACCELERATOR instead of `<C-w>c`, settle, and screenshot.
/// Both files live in the cage process's working directory (set by the caller).
const CHORD_CLOSE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. A timing
# knob only -- no assertion depends on it.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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

/// The `/proc` scan the two crash smokes and the STALL smoke use to find — and to
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
/// change, which carries a `macos-followups.md` obligation (the Darwin path must
/// stay evaluable) for no benefit whatsoever. `sed` is already in every scenario's
/// toolchain, and `/proc/<pid>/status` carries both fields the scan needs.
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
#   MEASURED 2026-08-02 (pass 5). `assert_no_nvim_respawn` alone does NOT
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
        echo "  retries forever -- the exact hazard the cap exists to prevent, and the defect" >&2
        echo "  macos-followups.md records the Swift side still has." >&2
        echo "  ZERO means no automatic restart was attempted at all, so the crash overlay" >&2
        echo "  below is showing CRASHED_RETRYING_MESSAGE for a restart that never ran." >&2
        exit 20
    fi
}
"#;

/// Prepends [`FIND_NVIM_PID_HELPER`] to a keyboard-only scenario ([`CRASH_SCENARIO`],
/// [`STALL_SCENARIO`]) or to a pointer body ([`POINTER_RESTART_BUTTON_BODY`]).
/// `Command::arg` and [`run_pointer_scenario`] both accept the result.
fn with_nvim_pid_helper(scenario: &str) -> String {
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
/// it is the adversarial half of the plan's wake-coalescing argument. The GTK wake
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
///  * WITHOUT `:preserve`, "the edit survived" would be a race. `architecture.md:22`
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
/// * `11` — the app died before its window mapped (the shared convention).
/// * `14` — a readback file the captive Neovim was told to write never appeared.
/// * `16` — THE APP DIED with its Neovim. See [`FIND_NVIM_PID_HELPER`].
/// * `17` — the Neovim child could not be identified uniquely.
/// * `18` — the SIGKILLed Neovim never left the process table.
/// * `19` — recovery timed out: `bufname('%')` never named the fixture again. This
///   is the headline red-run symptom for four of this smoke's five reverts.
const CRASH_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below). Same
# values the other keyboard-only smokes use, sized for the FULL workspace suite.
# Timing knobs only -- no assertion depends on any of them.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
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

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome crash smoke: app process exited early (pid $app_pid)" >&2
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
/// This is only reachable because the buffer picker exists: it is the one blocking
/// RPC a RUNNING `wolf-gnome` issues.
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
/// The resumed Neovim completes the detached task, but `block_on_rpc` already
/// returned `Err(Timeout)` and its `Ok` auto-clear arm never runs, so liveness stays
/// `NotResponding` and the overlay stays up. The capture is taken BEFORE the
/// `SIGCONT` regardless, so the ordering is not something an assertion rests on.
///
/// ## Exit codes owned
///
/// * `11` — the app died before its window mapped (the shared convention).
/// * `16` — THE APP DIED with its wedged Neovim. The worst failure of this feature,
///   and its own code so it can never be confused with a missing overlay. See
///   [`FIND_NVIM_PID_HELPER`].
/// * `17` — the Neovim child could not be identified uniquely, so nothing was
///   stopped and the chord below would have been fired at a healthy editor.
const STALL_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below). Same
# values the other keyboard-only smokes use, sized for the FULL workspace suite.
#
# SETTLE is the ONLY real budget between spawning the app and the first
# keystroke: the wtype poll below does NOT gate input readiness (it breaks at
# tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
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

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome stall smoke: app process exited early (pid $app_pid)" >&2
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
# and never observes it, so the core stays NotResponding -- which is the concrete
# evidence for the documented claim that a stall on Linux does not self-clear.
kill -CONT "$nvim_pid" 2>/dev/null || true

# The trap installed above tears the app down on the way out.
exit 0
"#;

/// Everything the FIVE pointer scenarios have in common, as one `sh` fragment:
/// the teardown trap, the command FIFO, the `wolf-pointer-inject` helper, the
/// poll-for-ack protocol, the readback-file poll, the buffer fixture, the app
/// launch + window-map poll, and one initial in-pane `motion`.
///
/// This is a DELIBERATE deviation from this file's "one self-contained scenario
/// const per smoke" style. The preamble is ~90 lines of FIFO / ack / poll plumbing,
/// and five copies would be five places to fix a poll bug — the exact shape that
/// lets a harness rot in four of its five instances unnoticed. Each smoke's own
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
#
# SETTLE is the ONLY real budget between spawning the app and the first injected
# event: the wtype poll below does NOT gate readiness (it breaks at tries=0 on
# every run -- see the module doc comment). SCENARIO carries the measured margin
# table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0
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
# viewport at the top. Used by the click, drag and scroll bodies; the wedge and
# crash-button bodies open real files instead.
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

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
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

/// Body of [`crash_overlay_and_restart_button_recover_under_cage`], appended to
/// [`POINTER_PREAMBLE`] and composed with [`with_nvim_pid_helper`].
///
/// ## Why this smoke has to make the respawn FAIL
///
/// On the SUCCESS path the crash overlay is a sub-frame transient — shown, then
/// cleared by the fresh Neovim's `Alive` wake — and `NotResponding` is unreachable
/// from `wolf-gnome` at all (nothing in the front-end issues a synchronous RPC, so
/// no timeout can ever mark it). The one state in which the overlay genuinely
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
# the measured pane origin. No magic pixel appears anywhere in this file.
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
        echo "    1. the editor pane's ORIGIN moved (an adw::HeaderBar height or a sidebar width" >&2
        echo "       change) -- click_positions_the_cursor_under_cage derives it and fails first;" >&2
        echo "    2. the measured CELL SIZE changed, so the panes are not the size modelled;" >&2
        echo "    3. the divider HANDLE is far wider than the theme's one pixel, and both seeds" >&2
        echo "       landed on it;" >&2
        echo "    4. a fresh split no longer produces EQUAL shares, which is what the seeds model." >&2
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

// ---------------------------------------------------------------------------
// STATE-PERSISTENCE smokes (four): the pane layout, the lone open file and a
// dragged divider's WEIGHTS survive a QUIT AND RELAUNCH, and the window title
// renders the working directory.
//
// The three relaunch smokes run TWO app launches inside ONE `cage` instance — one
// scenario string with two sequential `"$WOLF_BIN" &` blocks — so `CAGE_LOCK`
// semantics are unchanged and no second compositor is ever started. The first two
// share this exit-code vocabulary, which is documented once here and referenced
// from each scenario; the divider one keeps `POINTER_PREAMBLE`'s codes for the
// helper functions it copies and renumbers these, so it carries its own table:
//
// | code | meaning |
// | --- | --- |
// | `11` | run 1's app died before its window mapped |
// | `12` | run 1 was still alive after the `:qa` poll budget — the drain/close path failed |
// | `13` | run 1 exited NON-ZERO: it went away, but not cleanly. After the state-persistence unit that specifically implicates a panic in `flush_on_close`, or a `RefCell` double-borrow against `wire_close`'s `core.borrow_mut().take()` |
// | `14` | run 2's app died before its window mapped |
// | `15` | the Neovim readback file never appeared in run 2 |
// | `16` | (the single-pane smoke only) run 1 wrote no `state.json` at all |
//
// Both also poll rather than sleep-and-hope for every observable, and both reap
// run 1 with `wait` so its EXIT STATUS is checked — "the process is gone" is
// equally true of a clean shutdown and of a panic in the GTK main loop, and this
// is the first unit in which the close path does real work.

/// Extra settle after the SECOND launch of a relaunch smoke, on top of the usual
/// `SETTLE`.
///
/// The layout rebuild does not happen at `build_ui` time: it is QUEUED there
/// (`PaneHandle::request_layout_rebuild`) and consumed on the first render wake at
/// which a window grid exists, and it then issues `WolfCore::rebuild_layout` — a
/// BLOCKING RPC that drives `:vsplit` / `:split` / `:edit` inside the captive
/// Neovim once per leaf. A timing knob only: no assertion depends on its value,
/// and every observable below is polled for.
const RELAUNCH_RESTORE_SETTLE: &str = "2.0";

// --- S1 fixtures: three files, three panes, one asymmetric tree -------------
//
// The names are deliberately UNLIKE their headings, following the convention the
// sibling smokes already follow (`ckeep.md` => `# CLOSEKEEP`, `nbase.md` =>
// `# NEWBUFBASE`): the tree sidebar renders every cwd file BY NAME, so a fixture
// named after the word its own smoke asserts on can satisfy that assertion with
// the editor drawing nothing. `restore_fixture_names_cannot_satisfy_their_own_assertions`
// enforces it mechanically.

/// The file opened in the TOP-LEFT pane (`rleft.md`).
const RESTORE_FILE_LEFT: &str = "rleft.md";

/// Heading inside [`RESTORE_FILE_LEFT`]; lowercased it contains
/// [`RESTORE_OCR_ALFA`].
const RESTORE_HEADING_LEFT: &str = "# RESTOREALFA";

/// Lowercased substring OCR must find for the restored TOP-LEFT pane.
const RESTORE_OCR_ALFA: &str = "restorealfa";

/// The file opened in the BOTTOM-LEFT pane (`rmid.md`) — DFS position 2.
const RESTORE_FILE_MID: &str = "rmid.md";

/// Heading inside [`RESTORE_FILE_MID`]; lowercased it contains
/// [`RESTORE_OCR_BRAVO`].
const RESTORE_HEADING_MID: &str = "# RESTOREBRAVO";

/// Lowercased substring OCR must find for the restored BOTTOM-LEFT pane.
const RESTORE_OCR_BRAVO: &str = "restorebravo";

/// The file opened in the full-height RIGHT pane (`rright.md`) — DFS position 3.
const RESTORE_FILE_RIGHT: &str = "rright.md";

/// Heading inside [`RESTORE_FILE_RIGHT`]; lowercased it contains
/// [`RESTORE_OCR_CHARLIE`].
const RESTORE_HEADING_RIGHT: &str = "# RESTORECHARLIE";

/// Lowercased substring OCR must find for the restored RIGHT pane.
const RESTORE_OCR_CHARLIE: &str = "restorecharlie";

/// Name of the file the RESTORED Neovim writes its own window list into, in the
/// smoke's temp cwd (so the tree sidebar renders it — hence its inclusion in
/// [`RELAUNCH_FIXTURE_NAMES`]).
const RESTORE_READBACK_NAME: &str = "restore_windows.txt";

/// The EXACT readback [`pane_layout_survives_relaunch_under_cage`] requires.
///
/// `tabpagebuflist()` returns one buffer per window in WINDOW-NUMBER order, and
/// nvim numbers windows by layout position (a row left-to-right, a column
/// top-to-bottom). For the tree run 1 builds — `row[ col[left-top, left-bottom],
/// right ]` — that is exactly `[rleft, rmid, rright]`, so this single exact
/// comparison proves the pane COUNT, the buffer IDENTITIES and their ORDER at
/// once.
const RESTORE_READBACK_EXPECTED: &str = "rleft.md rmid.md rright.md";

/// The two-launch PANE-LAYOUT RELAUNCH scenario, run as one `sh -c` string inside
/// ONE `cage`.
///
/// **Run 1** launches the app, opens `rleft.md`, and builds a THREE-PANE
/// ASYMMETRIC tree with the same `wtype` chord invocations
/// [`VSPLIT_SCENARIO`] / [`FOCUS_SCENARIO`] use:
/// `<C-w>v` (new pane RIGHT, focused) → `:e rright.md` → `<C-w>h` (focus back
/// LEFT) → `<C-w>s` (new pane BELOW the left one, focused) → `:e rmid.md`. It then
/// types `:qa`, POLLS for the process to go, and REAPS it to check the status.
///
/// **Run 2** launches the SAME binary in the SAME working directory with the SAME
/// `WOLF_STATE_DIR` (both inherited from `cage_command`), polls that `cage` is up
/// (NOT that the window mapped — see [`SCENARIO`]'s `SETTLE` comment), settles for
/// the restore, and asks the restored Neovim to `writefile` its own
/// `tabpagebuflist()` — the load-bearing observable — before the screenshot.
///
/// See the exit-code table above this const.
const RESTORE_LAYOUT_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for either run's window to map, which `wait_window` cannot observe (see it
# below). WAIT_TRIES is sized generously for the FULL workspace suite, where ~90
# integration-test binaries (most spawning a real nvim) run in parallel. Timing
# knobs only -- no assertion depends on any of them.
#
# SETTLE is the ONLY real budget between spawning EITHER run's app and its first
# keystroke: `wait_window`'s wtype probe does NOT gate input readiness (it breaks
# at tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0
# How many WAIT_INTERVAL polls to allow for run 1 to EXIT after :qa, and for the
# run-2 readback file to appear.
QUIT_TRIES=75
FILE_TRIES=100

# Teardown, installed BEFORE anything starts so every `exit NN` below runs it.
# Both runs use the same variable; it is cleared once run 1 has been reaped so a
# later failure cannot signal an already-gone pid.
cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

# Wait for the app to still be alive AND for the compositor to be up. The no-op
# `wtype ""` does NOT prove a surface has mapped (measured: it breaks at tries=0
# on every run) -- SETTLE is the real budget, see SCENARIO. $1 is the exit code
# to use if the app dies first, so run 1 and run 2 stay distinguishable.
wait_window() {
    tries=0
    while [ "$tries" -lt "$WAIT_TRIES" ]; do
        if ! kill -0 "$app_pid" 2>/dev/null; then
            echo "wolf-gnome relaunch smoke: app process exited early (pid $app_pid)" >&2
            exit "$1"
        fi
        if wtype "" >/dev/null 2>&1; then
            return 0
        fi
        tries=$((tries + 1))
        sleep "$WAIT_INTERVAL"
    done
    return 0
}

# Poll for a readback file the captive Neovim writes. Nothing downstream may read
# a file we merely hope is there.
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
    echo "wolf-gnome relaunch smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# =========================================================================
# RUN 1 -- build the three-pane asymmetric tree, then quit cleanly.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_window 11
sleep "$SETTLE"

# Open file A in the single starting pane.
wtype -- ":e $RESTORE_FILE_LEFT"
wtype -k Return
sleep "$SETTLE"

# VERTICAL split: the new pane opens to the RIGHT and takes focus.
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"

# The new RIGHT pane shows file C.
wtype -- ":e $RESTORE_FILE_RIGHT"
wtype -k Return
sleep "$SETTLE"

# Move focus BACK to the left pane (a pure FocusDir intent -> the shell's
# `PaneTree::neighbor` -> nvim_set_current_win; the FOCUS_SCENARIO precedent).
wtype -M ctrl -k w -m ctrl
wtype -k h
sleep "$SETTLE"

# HORIZONTAL split of the LEFT pane: the new pane stacks BELOW it and takes focus.
# The tree is now row[ col[A, new], C ] -- an asymmetric three-pane shape no fresh
# launch could ever produce.
wtype -M ctrl -k w -m ctrl
wtype -k s
sleep "$SETTLE"

# The new BOTTOM-LEFT pane shows file B.
wtype -- ":e $RESTORE_FILE_MID"
wtype -k Return
sleep "$SETTLE"

# Quit the whole application. `StateHandle::flush_on_close` is the FIRST statement
# in the close-request handler -- while the core and the panes are still alive --
# and it blocks on `StateStore::flush`, so state.json is on disk before the process
# goes.
wtype -- ":qa"
wtype -k Return

# POLL (never sleep-and-hope) for run 1 to go away, then REAP it and check its
# EXIT STATUS. `wait` can still report it: the shell retains a background child's
# status until a `wait` asks for it.
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
    echo "wolf-gnome relaunch smoke: run 1 (pid $app_pid) still alive after :qa." >&2
    echo "  The drained close_window command never reached window.close()." >&2
    exit 12
fi
# `set -e` would abort before the status could be reported, so collect it through
# `||` (which is exempt from `set -e`).
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome relaunch smoke: run 1 (pid $app_pid) exited $rc, not 0." >&2
    echo "  It went away, but NOT cleanly. After the state-persistence unit that" >&2
    echo "  specifically implicates flush_on_close: either it panicked, or it holds a" >&2
    echo "  core.borrow() across wire_close's core.borrow_mut().take() (a RefCell" >&2
    echo "  double-borrow), or StateStore::flush blocked past its timeout." >&2
    exit 13
fi
app_pid=

# =========================================================================
# RUN 2 -- relaunch in the SAME cwd with the SAME state dir, and read back.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_window 14
sleep "$SETTLE"
# The rebuild happens on the wake AFTER the first pane binds and issues a blocking
# RPC per leaf, so this waits longer than a plain redraw would need.
sleep "$RESTORE_SETTLE"

# THE load-bearing observable: one line holding every window's buffer TAIL name, in
# window order, space separated. `:t` sidesteps any question of how /tmp resolves.
#
# The mapping uses a LAMBDA rather than the usual `'…v:val…'` string expression so
# that no double quote appears anywhere in the typed command: the whole line is
# already inside a double-quoted shell word, and nesting quotes through sh -> wtype
# -> the Neovim cmdline is exactly where these scenarios have historically broken.
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ' ')], '$RESTORE_READBACK')"
wtype -k Return
wait_file "$RESTORE_READBACK" || exit 15

# Capture the live window for the corroborating OCR.
grim "$OUT_PNG"

# The trap tears run 2 down on the way out.
exit 0
"#;

// --- S2 fixtures: ONE file, ONE pane, and a decoy ---------------------------

/// The single file opened in run 1 (`szulu.md`). Its NAME contains neither
/// [`SOLO_OCR_PRESENT`] nor [`SOLO_OCR_ABSENT`], so only a RENDERED BUFFER can
/// satisfy the OCR assertion.
const SOLO_FILE: &str = "szulu.md";

/// Heading inside [`SOLO_FILE`]; lowercased it contains [`SOLO_OCR_PRESENT`].
const SOLO_HEADING: &str = "# SOLOZULU";

/// Lowercased substring OCR MUST find: the restored file's heading.
const SOLO_OCR_PRESENT: &str = "solozulu";

/// The DECOY file, never opened by anything.
///
/// It is alphabetically FIRST in the cwd, so it kills the one plausible
/// alternative hypothesis: if the app were reopening "the first file it found"
/// rather than the saved layout, the readback would be `salfa.md` and the OCR
/// would carry [`SOLO_OCR_ABSENT`]. Both are asserted against.
const SOLO_DECOY_FILE: &str = "salfa.md";

/// Heading inside [`SOLO_DECOY_FILE`]; lowercased it contains
/// [`SOLO_OCR_ABSENT`].
const SOLO_DECOY_HEADING: &str = "# SOLOALFA";

/// Lowercased substring OCR must NOT find: the decoy's heading.
const SOLO_OCR_ABSENT: &str = "soloalfa";

/// Name of the file the RESTORED Neovim writes its current buffer's tail name
/// into, in the smoke's temp cwd.
const SOLO_READBACK_NAME: &str = "solo_buffer.txt";

/// The EXACT readback [`single_pane_file_survives_relaunch_under_cage`] requires.
const SOLO_READBACK_EXPECTED: &str = "szulu.md";

/// The two-launch SINGLE-PANE RELAUNCH scenario, run as one `sh -c` string inside
/// ONE `cage`. Structurally [`RESTORE_LAYOUT_SCENARIO`] with the split chords
/// removed — that absence IS the subject.
///
/// Between the two runs it copies the state file run 1 wrote to
/// `$SOLO_STATE_COPY`, so the Rust side can assert on RUN 1'S SAVE rather than on
/// whatever run 2 later wrote over it. That separation is what keeps the two
/// red-run reverts distinguishable: with the SAVE-side arity guard reintroduced
/// the copy carries `layout: null`, while with the RESTORE-side arity guard
/// reintroduced the copy is correct and only the readback and the OCR go red.
///
/// See the exit-code table above [`RELAUNCH_RESTORE_SETTLE`]; this scenario adds
/// `16` for "run 1 wrote no state.json at all".
const RESTORE_SOLO_SCENARIO: &str = r#"
set -eu

# SETTLE is the ONLY real budget between spawning EITHER run's app and its first
# keystroke: `wait_window`'s wtype probe does NOT gate input readiness (it breaks
# at tries=0 on every run -- see the module doc comment). SCENARIO carries the
# measured margin table behind this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0
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

# The no-op `wtype ""` does NOT prove a surface has mapped (measured: it breaks at
# tries=0 on every run) -- SETTLE is the real budget, see SCENARIO. $1 is the exit
# code to use if the app dies first, so run 1 and run 2 stay distinguishable.
wait_window() {
    tries=0
    while [ "$tries" -lt "$WAIT_TRIES" ]; do
        if ! kill -0 "$app_pid" 2>/dev/null; then
            echo "wolf-gnome single-pane smoke: app process exited early (pid $app_pid)" >&2
            exit "$1"
        fi
        if wtype "" >/dev/null 2>&1; then
            return 0
        fi
        tries=$((tries + 1))
        sleep "$WAIT_INTERVAL"
    done
    return 0
}

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
    echo "wolf-gnome single-pane smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# =========================================================================
# RUN 1 -- open ONE file in ONE pane. NO SPLIT. Then quit cleanly.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_window 11
sleep "$SETTLE"

# The whole point: a single unsplit pane holding a single file. Under the old
# ARITY guard this shape persisted its cwd and NOTHING else.
wtype -- ":e $SOLO_FILE"
wtype -k Return
sleep "$SETTLE"

wtype -- ":qa"
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
    echo "wolf-gnome single-pane smoke: run 1 (pid $app_pid) still alive after :qa." >&2
    exit 12
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome single-pane smoke: run 1 (pid $app_pid) exited $rc, not 0." >&2
    echo "  It went away, but NOT cleanly -- suspect flush_on_close panicking or" >&2
    echo "  holding a core.borrow() across wire_close's core.borrow_mut().take()." >&2
    exit 13
fi
app_pid=

# --- Freeze RUN 1'S SAVE before run 2 can write over it --------------------
#
# Run 2 re-snapshots on every render wake, so by the time the Rust side could read
# the live file it would hold RUN 2's tree. Copying here is what lets the two
# red-run reverts produce DISTINGUISHABLE failures.
if [ ! -s "$SOLO_STATE_SRC" ]; then
    echo "wolf-gnome single-pane smoke: run 1 wrote no state.json at $SOLO_STATE_SRC." >&2
    echo "  Nothing at all was persisted, so this is not the arity guard: suspect" >&2
    echo "  open_state_store() resolving no directory, flush_on_close never running," >&2
    echo "  or WOLF_STATE_DIR not reaching the app." >&2
    exit 16
fi
cp "$SOLO_STATE_SRC" "$SOLO_STATE_COPY"

# =========================================================================
# RUN 2 -- relaunch in the SAME cwd with the SAME state dir, and read back.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_window 14
sleep "$SETTLE"
sleep "$RESTORE_SETTLE"

# THE load-bearing observable: the tail name of the CURRENT buffer. A broken build
# answers `Untitled.md`; an app that reopened "the first file it found" would
# answer `salfa.md`.
wtype -- ":call writefile([fnamemodify(bufname('%'),':t')], '$SOLO_READBACK')"
wtype -k Return
wait_file "$SOLO_READBACK" || exit 15

grim "$OUT_PNG"
exit 0
"#;

// --- S4 fixtures: two files, one dragged divider, two launches --------------
//
// Same naming convention as every sibling: the file NAMES contain neither asserted
// term, so only a rendered buffer can satisfy the OCR corroboration.
// `restore_fixture_names_cannot_satisfy_their_own_assertions` enforces it over
// `RELAUNCH_FIXTURE_NAMES`, which these four names are registered in.

/// The file opened in the LEFT pane (`dvleft.md`) — the pane the drag widens.
const DIVIDER_FILE_LEFT: &str = "dvleft.md";

/// Heading inside [`DIVIDER_FILE_LEFT`]; lowercased it contains
/// [`DIVIDER_OCR_LEFT`].
const DIVIDER_HEADING_LEFT: &str = "# DIVIDERALFA";

/// Lowercased substring OCR must find for the restored LEFT pane.
const DIVIDER_OCR_LEFT: &str = "divideralfa";

/// The file opened in the RIGHT pane (`dvright.md`) — the pane the drag narrows.
const DIVIDER_FILE_RIGHT: &str = "dvright.md";

/// Heading on line 1 of [`DIVIDER_FILE_RIGHT`]; lowercased it contains
/// [`DIVIDER_OCR_RIGHT`]. [`divider_right_fixture_body`] writes it above
/// [`DIVIDER_BODY_LINE_COUNT`] body lines, for the page-density reason recorded
/// there.
const DIVIDER_HEADING_RIGHT: &str = "# DIVIDERBRAVO";

/// Lowercased substring OCR must find for the restored RIGHT pane. It is 12
/// characters, 14 with the heading marker, which is what
/// [`DIVIDER_RELAUNCH_DRAG_PX`] is sized against: a narrower pane would soft-wrap
/// it and OCR would read two fragments.
///
/// Soft-wrapping is not the only way this term can go unread in a narrow pane, and
/// it was not the first one met: see [`DIVIDER_BODY_LINE_COUNT`].
const DIVIDER_OCR_RIGHT: &str = "dividerbravo";

/// How many body lines [`divider_right_fixture_body`] writes below
/// [`DIVIDER_HEADING_RIGHT`].
///
/// FIXTURE, and load-bearing for the divider relaunch smoke's OCR assertion rather
/// than decoration.
///
/// `tesseract --psm 11` groups a sparse pane into ONE region and estimates a scale
/// from it, so anything long, thin and high-contrast sharing that region corrupts
/// the reading of a 15-cell heading it can plainly see. TWO such neighbours were
/// measured in this pane, and they take different fixes: the `~` end-of-buffer
/// column, which this constant answers, and the active-pane FOCUS RING, which
/// [`DIVIDER_RELAUNCH_SCENARIO`]'s focus move answers — its ablation table lives
/// beside that move. A reader who has just seen the OCR assertion go red wants the
/// ring first; it is the one whose corruptor is production rendering.
///
/// MEASURED for the `~` column, with a heading and nothing else in the file: the
/// narrow pane renders that one short line above a bare vertical column of markers,
/// and the heading read as `4 DIVIDERBR=I-\V0`. The glyphs were not the problem —
/// the `A` bitmaps in the two panes are pixel-identical bar a two-pixel offset, and
/// the SAME pixels moved into the wide pane read `DIVIDERBRAVO` correctly. Four
/// controlled ablations on the retained PNG isolated it: masking out the block
/// cursor, masking out the focus ring and padding the image all left the misread in
/// place, while blanking the `~` column alone — everything else untouched — fixed
/// it. It is the hazard [`SIDEBAR_FILLER_COUNT`] exists for, one pane wide instead
/// of one page wide, and it takes the same fix: give tesseract ordinary text at the
/// ordinary scale to calibrate against.
///
/// Twenty-four lines plus the heading is comfortably more than the measured
/// `winheight(0)` of 14, so no `~` marker is drawn in that pane at all — a later
/// run's retained PNG shows the 24 body lines and no marker anywhere. RAISING IT
/// BUYS NOTHING against a red OCR assertion now: the column it targets is already
/// gone. The other lever, shrinking [`DIVIDER_RELAUNCH_DRAG_PX`], is wrong for both
/// hazards — it would leave the `~` column exactly where it is, leave the ring
/// exactly where it is, and narrow the weight gap the smoke's first assertion rests
/// on.
///
/// With BOTH of those answered, a third cause remained and is not a fixture problem
/// at all: `--psm 11` misclassified one glyph of the heading at 15 columns, on
/// pixels a second segmentation mode read correctly. That one is answered by
/// [`ocr_uniform_block_to_lowercase`], the reader the divider relaunch smoke adds
/// for itself, and it is the first thing to suspect if the OCR assertion goes red —
/// not this constant.
const DIVIDER_BODY_LINE_COUNT: usize = 24;

/// How many `x`s each [`divider_right_fixture_body`] line carries after its
/// three-digit ordinal.
///
/// Nine, so every body line is twelve characters — inside the fifteen columns the
/// drag leaves the right pane, so `runtime/nvim/init.lua`'s `wrap` / `linebreak`
/// never soft-wrap one and each buffer line occupies exactly one screen row. A
/// wrapped body would fill the pane just as well; an UNWRAPPED one is one fewer
/// thing that differs between the two runs when the OCR assertion goes red.
const DIVIDER_BODY_LINE_PAD: usize = 9;

/// Builds the contents of [`DIVIDER_FILE_RIGHT`]: [`DIVIDER_HEADING_RIGHT`], then
/// [`DIVIDER_BODY_LINE_COUNT`] lines of a three-digit ordinal followed by
/// [`DIVIDER_BODY_LINE_PAD`] `x`s — the shape [`restart_fixture_body`] and the
/// `fill_buffer` fixture in [`POINTER_PREAMBLE`] already use.
///
/// The trailing newline is [`write_relaunch_fixtures`]'s, so the file ends on a
/// body line rather than on a blank one.
///
/// ## Why the body may only be digits and `x`
///
/// A fixture whose CONTENT carried an asserted term would satisfy an OCR assertion
/// on its own — the hazard [`PICKER_FILE_NAME`] documents, aimed at content rather
/// than at a file name, and the reason this body is deliberately not word-like.
/// [`restore_fixture_names_cannot_satisfy_their_own_assertions`] enforces it
/// mechanically against [`ASSERTED_OCR_TERMS`] rather than leaving it here as
/// prose.
fn divider_right_fixture_body() -> String {
    let mut body = String::from(DIVIDER_HEADING_RIGHT);
    for line in 1..=DIVIDER_BODY_LINE_COUNT {
        body.push_str(&format!("\n{line:03}{}", "x".repeat(DIVIDER_BODY_LINE_PAD)));
    }
    body
}

/// Name of the file RUN 1's Neovim writes its pre-drag pane widths into, in the
/// smoke's temp cwd (so the tree sidebar renders it — hence its registration in
/// [`RELAUNCH_FIXTURE_NAMES`]). Its readback is the precondition that separates "the
/// press missed the divider handle" from "the weight was not written back".
const DIVIDER_W0_NAME: &str = "dv_widths_zero.txt";

/// Name of the file RUN 1's Neovim writes its post-drag pane widths into. See
/// [`DIVIDER_W0_NAME`].
const DIVIDER_W1_NAME: &str = "dv_widths_one.txt";

/// Name of the file RUN 2's Neovim writes its restored pane widths into. See
/// [`DIVIDER_W1_NAME`].
const DIVIDER_W2_NAME: &str = "dv_widths_two.txt";

/// The two-launch DIVIDER-WEIGHT RELAUNCH scenario, run as one `sh -c` string
/// inside ONE `cage`.
///
/// ## Why it duplicates the pointer plumbing instead of reusing it
///
/// [`POINTER_PREAMBLE`] launches the app itself, and a two-run scenario cannot use
/// it: the helper must outlive run 1 while the app must not. So the FIFO, the
/// helper launch, `exec 3>`, `hdead`, `hwait_ready`, `hsend`, `hstep`, `wait_file`
/// and the trapped cleanup are duplicated here, exactly as
/// [`RESTORE_SOLO_SCENARIO`] duplicates [`RESTORE_LAYOUT_SCENARIO`]'s poll-and-reap
/// machinery. The cleanup kills the HELPER first, so the app's shutdown is never
/// racing a pointer event.
///
/// Between the two runs it freezes run 1's `state.json` into `$DIV_STATE_COPY`,
/// following the [`RESTORE_SOLO_SCENARIO`] precedent: run 2 re-snapshots on every
/// render wake, so the live file would carry RUN 2's tree by the time the Rust side
/// could read it — and the save-side and restore-side red runs would then be
/// indistinguishable.
///
/// ## Exit codes
///
/// The first five are [`POINTER_PREAMBLE`]'s, because the functions that raise them
/// are copied from it verbatim. The relaunch-specific ones are renumbered from the
/// shared state-persistence table to avoid colliding with those.
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | run 1's app died before its window mapped |
/// | `12` | `wolf-pointer-inject` never printed its `ready …` line |
/// | `13` | a helper command was never acked within budget |
/// | `14` | a Neovim readback file never appeared |
/// | `15` | the helper died mid-scenario |
/// | `16` | run 1 was still alive after the `:qa` poll budget |
/// | `17` | run 1 exited NON-ZERO — it went away, but not cleanly |
/// | `18` | run 1 wrote no `state.json` at all |
/// | `19` | run 2's app died before its window mapped |
/// | `21` | the bisection seeds did not straddle the divider |
const DIVIDER_RELAUNCH_SCENARIO: &str = r#"
set -eu

# Poll knobs, all copied from POINTER_PREAMBLE and RESTORE_SOLO_SCENARIO. Timing
# knobs only -- no assertion depends on any of them. SETTLE is the ONLY real budget
# between spawning either run's app and its first keystroke; see SCENARIO.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0
ACK_TRIES=75
FILE_TRIES=100
POINTER_STEP=0.2
QUIT_TRIES=75

# Teardown, installed BEFORE anything is started. The HELPER dies first: it is the
# only process that can still inject, so killing it first means neither run's
# shutdown is ever racing a pointer event.
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

mkfifo "$CMD_FIFO"
wolf-pointer-inject < "$CMD_FIFO" > "$ACK_FILE" 2> "$HELPER_ERR" &
helper_pid=$!

# Hold the FIFO's write end open for the WHOLE scenario on fd 3: without it every
# `printf > "$CMD_FIFO"` would close it and hand the helper a spurious EOF, which
# it exits on by design. This `exec` is also what unblocks the helper's own read.
exec 3>"$CMD_FIFO"
seq=0

hdead() {
    if kill -0 "$helper_pid" 2>/dev/null; then
        return 1
    fi
    return 0
}

hwait_ready() {
    t=0
    while [ "$t" -lt "$ACK_TRIES" ]; do
        if grep -q '^ready ' "$ACK_FILE" 2>/dev/null; then
            grep '^ready ' "$ACK_FILE" > "$READY_FILE" || true
            return 0
        fi
        if hdead; then
            echo "wolf-gnome divider relaunch smoke: wolf-pointer-inject exited before printing its readiness line." >&2
            cat "$HELPER_ERR" >&2
            exit 15
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome divider relaunch smoke: wolf-pointer-inject never printed 'ready ...' within budget." >&2
    cat "$HELPER_ERR" >&2
    exit 12
}

# Send one command and BLOCK until it is acked. The ack follows a
# wl_display_roundtrip, so it means the compositor has PROCESSED the event.
hsend() {
    seq=$((seq + 1))
    printf '%s\n' "$*" >&3
    t=0
    while [ "$t" -lt "$ACK_TRIES" ]; do
        if grep -qx "ok $seq" "$ACK_FILE" 2>/dev/null; then
            return 0
        fi
        if hdead; then
            if grep -qx "ok $seq" "$ACK_FILE" 2>/dev/null; then
                return 0
            fi
            echo "wolf-gnome divider relaunch smoke: wolf-pointer-inject died while handling command #$seq ($*)." >&2
            cat "$HELPER_ERR" >&2
            exit 15
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome divider relaunch smoke: command #$seq ($*) was never acked within budget." >&2
    cat "$HELPER_ERR" >&2
    exit 13
}

# `hsend`, then pace before the next event of the SAME gesture. Every event of
# every gesture below goes through this, never through bare `hsend`; see `hstep` in
# POINTER_PREAMBLE for the measured reason.
hstep() {
    hsend "$@"
    sleep "$POINTER_STEP"
}

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
    echo "wolf-gnome divider relaunch smoke: readback file $f never appeared (or stayed empty)." >&2
    cat "$HELPER_ERR" >&2
    return 1
}

# The no-op `wtype ""` does NOT prove a surface has mapped (measured: it breaks at
# tries=0 on every run) -- SETTLE is the real budget, see SCENARIO. $1 is the exit
# code to use if the app dies first, so run 1 and run 2 stay distinguishable.
wait_window() {
    tries=0
    while [ "$tries" -lt "$WAIT_TRIES" ]; do
        if ! kill -0 "$app_pid" 2>/dev/null; then
            echo "wolf-gnome divider relaunch smoke: app process exited early (pid $app_pid)" >&2
            cat "$HELPER_ERR" >&2
            exit "$1"
        fi
        if wtype "" >/dev/null 2>&1; then
            return 0
        fi
        tries=$((tries + 1))
        sleep "$WAIT_INTERVAL"
    done
    return 0
}

# probe_buffer <x>: click at (<x>, $PROBE_Y) and leave the TAIL NAME of the buffer
# in the focused window in $focused.
#
# The buffer name rather than the window number, because each pane here holds a
# distinctly-named file: that makes the bisection's precondition name the actual
# panes ("the low seed must focus dvleft.md"), and it is immune to nvim numbering
# its windows in an order this front-end does not draw them in.
#
# A click that lands ON the divider handle reaches no pane and changes nothing,
# which is what the reset click in `bisect_divider` exists for. See
# POINTER_DIVIDER_BODY's `probe_winnr` for the one recorded hazard of clicking a
# handle: `gtk::Paned` binds Return on a FOCUSED handle, so an exit 14 immediately
# after a probe is the thing to check first.
probe_buffer() {
    rm -f "$PROBE_FILE"
    hstep motion "$1" "$PROBE_Y"
    hstep press "$BTN_LEFT"
    hstep release "$BTN_LEFT"
    sleep "$POINTER_STEP"
    wtype -- ":call writefile([fnamemodify(bufname('%'), ':t')], '$PROBE_FILE')"
    wtype -k Return
    wait_file "$PROBE_FILE" || exit 14
    focused=$(cat "$PROBE_FILE")
}

# bisect_divider <lo> <hi>: locate the single divider's handle. Sets $divider_x.
#
# THE RESET CLICK IS LOAD-BEARING: a probe that lands on the handle focuses no
# window at all, so it reports whatever the PREVIOUS probe focused. Re-focusing the
# low side before every probe is what stops a handle hit being read as "the high
# side" and converging the search on the wrong edge.
bisect_divider() {
    lo=$1
    hi=$2
    probe_buffer "$lo"
    lo_buf=$focused
    probe_buffer "$hi"
    hi_buf=$focused
    if [ "$lo_buf" != "$DIVIDER_FILE_LEFT" ] || [ "$hi_buf" != "$DIVIDER_FILE_RIGHT" ]; then
        echo "wolf-gnome divider relaunch smoke: the bisection seeds do not straddle the divider." >&2
        echo "  Seed $lo focused '$lo_buf' (expected $DIVIDER_FILE_LEFT) and seed $hi focused" >&2
        echo "  '$hi_buf' (expected $DIVIDER_FILE_RIGHT). Candidate causes, in order:" >&2
        echo "    1. the editor pane's ORIGIN moved (an adw::HeaderBar height or sidebar width" >&2
        echo "       change) -- click_positions_the_cursor_under_cage derives it and fails first;" >&2
        echo "    2. the measured CELL SIZE changed, so the panes are not the size modelled;" >&2
        echo "    3. the divider HANDLE is far wider than the theme's one pixel;" >&2
        echo "    4. a fresh split no longer produces EQUAL shares, which is what the seeds model." >&2
        echo "  An empty name means the click focused a pane holding no file at all, i.e. the :e" >&2
        echo "  never landed." >&2
        exit 21
    fi
    steps=0
    while [ "$steps" -lt "$BISECT_STEPS" ]; do
        mid=$(( (lo + hi) / 2 ))
        probe_buffer "$lo"
        probe_buffer "$mid"
        if [ "$focused" = "$hi_buf" ]; then
            hi=$mid
        else
            lo=$mid
        fi
        steps=$((steps + 1))
    done
    divider_x=$lo
}

# read_widths <file>: one `<columns>:<buffer tail>` pair per window, space
# separated, in whatever order Neovim enumerates its windows.
#
# `winwidth()`, NEVER `nvim_win_get_width()`, and that difference IS the
# observable. MEASURED against the pinned nvim 0.12.2 with a real ext_multigrid UI
# attached: `nvim_ui_try_resize_grid` -- which is what `WolfCore::resize_grid`
# issues, and the only thing a divider drag ever makes reach Neovim -- changes a
# window's GRID width and leaves its LAYOUT width untouched:
#
#     before                         win1 nvim_win_get_width=40 winwidth=40 | win2 nvim_win_get_width=39 winwidth=39
#     after resizing grids to 30x20  win1 nvim_win_get_width=40 winwidth=30 | win2 nvim_win_get_width=39 winwidth=30
#
# So `nvim_win_get_width` is BLIND to every drag this smoke performs: a readback
# built on it reports identical widths before and after, which is exactly how this
# smoke first failed (22/22 cells either side of a drag the screenshot showed
# plainly). Worse than insensitive, it also drifts on its own across the relaunch
# -- 39/5 for panes drawn 679 and 349 px wide -- so it is not a quantity any
# assertion here may rest on.
#
# Keyed by BUFFER NAME, not by window number: run 1 builds its split with an
# interactive `<C-w>v` while run 2's is rebuilt by `wolf_rebuild_layout`, and nvim
# need not number the same pane the same way in both. `nvim_win_get_number` turns
# each handle into the number `winwidth` wants, so the enumeration stays
# order-independent, and staying inside the nvim_* API keeps `$` out of the typed
# command.
read_widths() {
    rm -f "$1"
    wtype -- ":call writefile([join(map(nvim_tabpage_list_wins(0), {_, w -> winwidth(nvim_win_get_number(w)) . ':' . fnamemodify(bufname(nvim_win_get_buf(w)), ':t')}), ' ')], '$1')"
    wtype -k Return
    wait_file "$1" || exit 14
}

# ---------------------------------------------------------------------------
# Bring the helper up, THEN run 1's app (so the seat already carries pointer
# capability when GTK does its first wl_seat roundtrip).
hwait_ready

# =========================================================================
# RUN 1 -- two panes, one dragged divider, then a clean quit.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_window 11
sleep "$SETTLE"

# One in-pane motion so wl_pointer.enter and a first motion are delivered before
# anything depends on them.
hsend motion "$PROBE_X" "$PROBE_Y"
sleep "$SETTLE"

wtype -- ":e $DIVIDER_FILE_LEFT"
wtype -k Return
sleep "$SETTLE"

# Vertical split: the new pane is drawn to the RIGHT of its target and takes focus.
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"

wtype -- ":e $DIVIDER_FILE_RIGHT"
wtype -k Return
sleep "$SETTLE"

read_widths "$DIV_W0"

bisect_divider "$D_LO" "$D_HI"

# THE DRAG. `hstep` for every event of the gesture.
hstep motion "$divider_x" "$PROBE_Y"
hstep press "$BTN_LEFT"
hstep motion "$(( divider_x + RELAUNCH_DRAG_PX ))" "$PROBE_Y"
hstep release "$BTN_LEFT"
sleep "$SETTLE"

read_widths "$DIV_W1"

# Quit the whole application. `StateHandle::flush_on_close` is the FIRST statement
# in the close-request handler and blocks on `StateStore::flush`, so state.json is
# on disk before the process goes.
wtype -- ":qa"
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
    echo "wolf-gnome divider relaunch smoke: run 1 (pid $app_pid) still alive after :qa." >&2
    exit 16
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome divider relaunch smoke: run 1 (pid $app_pid) exited $rc, not 0." >&2
    echo "  It went away, but NOT cleanly -- suspect flush_on_close panicking or holding a" >&2
    echo "  core.borrow() across wire_close's core.borrow_mut().take()." >&2
    exit 17
fi
app_pid=

# --- Freeze RUN 1'S SAVE before run 2 can write over it --------------------
if [ ! -s "$DIV_STATE_SRC" ]; then
    echo "wolf-gnome divider relaunch smoke: run 1 wrote no state.json at $DIV_STATE_SRC." >&2
    echo "  Nothing at all was persisted, so this is not the weight write-back: suspect" >&2
    echo "  open_state_store() resolving no directory, flush_on_close never running, or" >&2
    echo "  WOLF_STATE_DIR not reaching the app." >&2
    exit 18
fi
cp "$DIV_STATE_SRC" "$DIV_STATE_COPY"

# =========================================================================
# RUN 2 -- relaunch in the SAME cwd with the SAME state dir, and read back.
# =========================================================================
"$WOLF_BIN" &
app_pid=$!
wait_window 19
sleep "$SETTLE"
sleep "$RESTORE_SETTLE"

read_widths "$DIV_W2"

# Move focus to the WIDE left pane, AFTER the widths above are read and before the
# screenshot.
#
# The blue active-pane FOCUS RING around the narrow right pane corrupts the OCR of
# the heading inside it. Isolated by ablation on a retained PNG, one mask at a
# time and everything else untouched:
#
#     as captured                     4 DIVIDERBR=AVO
#     right pane cropped out alone    77 DIVIDERBRAVO   (correct)
#     whole ring masked               B DIVIDERBRAVO    (correct)
#     ring TOP edge only masked       # DIVIDERBRAVO    (correct)
#     ring LEFT edge only masked      a DIVIDERBRAVO    (correct)
#     ring RIGHT edge only masked     4 DIVIDERBR=AVO   (still wrong)
#     ring BOTTOM edge only masked    4 DIVIDERBR=AVO   (still wrong)
#
# So it is the ring's TOP edge: a long thin high-contrast horizontal line
# immediately above a 15-cell heading, which `--psm 11` folds into the same sparse
# region and scales from. That is the DIVIDER_BODY_LINE_COUNT hazard from a second
# source, and the ring is production rendering -- the active-pane treatment -- so
# the fixture moves rather than the ring.
#
# `<C-w>h` is a pure FocusDir intent (the shell's `PaneTree::neighbor` ->
# nvim_set_current_win; the FOCUS_SCENARIO precedent). It resizes nothing, so the
# widths just read stay true whatever it does, and if focus is already on the left
# pane it is a no-op -- the same end state either way.
#
# This MOVES the ring onto the left pane rather than removing it from the capture,
# so if the OCR assertion now fails on DIVIDER_OCR_LEFT instead, this line is the
# first suspect and the next lever is known: give dvleft.md the body dvright.md
# already carries. The left pane is twice as wide and holds a bare heading over a
# `~` column TODAY while reading correctly, and several sibling smokes OCR a
# heading inside a focused pane, so the ring alone is not expected to bite there --
# but that is inference, and only this pane has been ablated.
wtype -M ctrl -k w -m ctrl
wtype -k h
sleep "$SETTLE"

grim "$OUT_PNG"
hsend quit
exit 0
"#;

// --- S3 fixtures: the window title renders the working directory ------------
//
// Two NESTED temp directories, so the `~` abbreviation is exercised rather than
// merely "some title was set": the cage command runs with `HOME` set to the
// PARENT and its working directory set to the CHILD.
//
// That is the same SHAPE `cage_command` now applies to every other smoke by default
// (HOME == the cwd's parent, so the title is a constant `~/<basename>`); the
// difference here is that the two basenames are chosen to make the abbreviation
// itself observable, one asserted PRESENT and one asserted ABSENT. This smoke sets
// its own HOME explicitly rather than relying on the default, so its subject cannot
// be retargeted by a later change to the helper.

/// Basename component of the smoke-owned HOME directory. It is deliberately the
/// term [`TITLE_OCR_ABSENT`] asserts must NOT be readable: the title must render
/// `~/<project>`, so this segment appears on screen only if the abbreviation is
/// broken.
const TITLE_HOME_DIR: &str = "titlehome-parent";

/// Basename of the PROJECT directory, which is what the header-bar title must
/// render. It is the term [`TITLE_OCR_PRESENT`] asserts is present.
const TITLE_PROJECT_DIR: &str = "titlezulu";

/// Lowercased substring OCR MUST find: the working directory's basename, drawn by
/// the `adw::HeaderBar` title. No fixture is named this (enforced by
/// [`ASSERTED_OCR_TERMS`] plus the three fixture guards), so it can only come from
/// the title.
const TITLE_OCR_PRESENT: &str = "titlezulu";

/// Lowercased substring OCR must NOT find: the HOME segment of the path. Its
/// absence is what makes this smoke test `title::display_title`'s ABBREVIATION
/// rather than merely "a title was set" — without the abbreviation the title would
/// read `/tmp/…/titlehome-parent/titlezulu` and this assertion fires.
const TITLE_OCR_ABSENT: &str = "titlehome";

/// The WINDOW-TITLE scenario, run as a single `sh -c` string inside `cage` (the
/// [`TREE_SCENARIO`] shape): launch the app, poll until `cage` is up (NOT until the
/// window maps — see [`SCENARIO`]'s `SETTLE` comment), settle
/// long enough for the tree's `FileSearch` to populate the sidebar AND for nvim to
/// report its cwd on the render wake that sets the title, then screenshot. No
/// keyboard interaction at all.
const TITLE_SCENARIO: &str = r#"
set -eu

# Poll knobs: up to WAIT_TRIES * WAIT_INTERVAL seconds for CAGE to come up -- NOT
# for the window to map, which this poll cannot observe (see the poll below).
# Timing knobs only -- no assertion depends on any of them.
#
# SETTLE is the ONLY real budget between spawning the app and the capture: the
# wtype poll below does NOT gate readiness (it breaks at tries=0 on every run --
# see the module doc comment). SCENARIO carries the measured margin table behind
# this value; do not trim it back.
WAIT_TRIES=75
WAIT_INTERVAL=0.2
SETTLE=1.0
# Extra settle for the streamed FileSearch traversal to populate the sidebar and
# for nvim's first cwd report to reach `AppShell::refresh_title` on a render wake.
TREE_SETTLE=1.5

"$WOLF_BIN" &
app_pid=$!

# The no-op `wtype ""` does NOT prove a surface has mapped: cage advertises
# zwp_virtual_keyboard_manager_v1 before any surface maps, and measured, this
# breaks at tries=0 on every run. SETTLE above is the real budget; see SCENARIO.
tries=0
while [ "$tries" -lt "$WAIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome title smoke: app process exited early (pid $app_pid)" >&2
        exit 11
    fi
    if wtype "" >/dev/null 2>&1; then
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done

sleep "$SETTLE"
sleep "$TREE_SETTLE"

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
/// ALL TWENTY-FIVE smokes derive their working directory from this — the window
/// title smoke uses the result as the base for the two NESTED directories its `~`
/// abbreviation needs, and the three relaunch smokes share one of these directories
/// across BOTH of their launches, which is the whole scenario. A fresh cwd per run
/// is MANDATORY, not tidiness:
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
/// also leaves one new `<state dir>/swap/<hash>/` holding at most one `.swp`;
/// harmless, because no future run resolves to that hash — and since the
/// `WOLF_STATE_DIR` isolation landed, that state dir is this cwd's own sibling
/// ([`smoke_state_dir`]) rather than the developer's real `~/.local/state/wolf/`.
///
/// ## Why the returned directory has a FIXED name under a unique parent
///
/// The returned path is `<unique>/project`, not `<unique>`: the uniqueness lives
/// entirely in the PARENT, and the component the app ever renders is the constant
/// [`TEMP_CWD_BASENAME`]. That split exists for one reason, and it is an OCR
/// determinism reason rather than a filesystem one.
///
/// Since the state-persistence unit the `adw::HeaderBar` renders the app's working
/// directory, so the temp directory's NAME became page text in every capture. While
/// that name was `<tmpdir>/wolf-gnome-smoke-<pid>-<19-digit nanos>` the captured page
/// therefore differed on every run, and it was measured to differ:
///
/// | arm | five consecutive capture md5s | size |
/// | --- | --- | --- |
/// | title rendering the run-varying temp path | all five DIFFERENT | 34.2–35.0 KB, varying |
/// | `set_title` suppressed (the base page) | `5ea4d070…` ×5, byte-identical | 26074 ×5 |
///
/// `tesseract` classifies a word by the whole page (see [`SIDEBAR_FILLER_COUNT`]), so
/// a page that changes every run makes every OCR assertion stochastic:
/// [`window_renders_typed_text_under_cage`] failed 2 of ~93 runs with the documented
/// signature, the isolated leading `h` of [`OCR_WORD`] misclassifying as `ello` /
/// `ﬁello`. Before the title existed that capture was bit-for-bit reproducible.
///
/// Pairing this fixed basename with [`cage_command`]'s `HOME` — which is exactly
/// `<unique>`, this directory's parent — makes `title::display_title` abbreviate the
/// whole varying part to `~`, so the header bar renders the constant string
/// **`~/project`** in every smoke. Short, so the added glyph mass is small; constant,
/// so the page is byte-identical across runs again. **Do not put a run-varying
/// component back into this path, and do not stop `cage_command` setting `HOME`**:
/// either one reintroduces the ~2% flake above, which no amount of extra filler files
/// can fix.
///
/// Everything a smoke writes into "the temp cwd" still lands in the returned
/// directory unchanged, and every caller builds its paths with `cwd.join(...)`, so
/// nothing downstream sees the extra level.
fn make_temp_cwd() -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is before the unix epoch")
        .as_nanos();
    // The UNIQUE part is the parent, and it is never rendered: `cage_command` makes
    // it the app's `HOME`, so the title abbreviates it to `~`.
    let parent = std::env::temp_dir().join(format!("{TEMP_CWD_PREFIX}{pid}-{nanos}"));
    let dir = parent.join(TEMP_CWD_BASENAME);
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|e| panic!("failed to create temp cwd {}: {e}", dir.display()));
    dir
}

/// The fixed leading part of every [`make_temp_cwd`] directory's UNIQUE PARENT; the
/// rest is the process id and a nanosecond timestamp, i.e. ASCII digits and `-` only.
///
/// It is a named const rather than an inline literal because it is a FIXTURE with the
/// same hazard as [`SIDEBAR_FILLER_FILES`]: the window title renders the app's working
/// directory, so a path component can become page text.
///
/// This one is NOT normally drawn. [`cage_command`] sets `HOME` to the directory this
/// prefix names, so `title::display_title` abbreviates it — and the pid/nanos tail
/// with it — to `~`, and the header bar shows [`TEMP_CWD_BASENAME`] alone. The guard
/// covers it anyway, in both directions and for two reasons: a smoke may override
/// `HOME` (two do), and if the abbreviation ever regresses the FULL path is what gets
/// drawn, at which point a prefix carrying an asserted term would turn a real bug into
/// a mysterious pass or a mysterious spurious failure instead of a clean red.
/// [`temp_cwd_name_cannot_satisfy_any_smoke_assertion`] keeps it clear of every
/// [`ASSERTED_OCR_TERMS`] entry, and reads it from here so a rename cannot drift past
/// the guard.
const TEMP_CWD_PREFIX: &str = "wolf-gnome-smoke-";

/// The FIXED basename of every [`make_temp_cwd`] directory — the app's actual working
/// directory, and the only part of it the header-bar title ever renders (as
/// `~/project`, because [`cage_command`] makes the unique parent the app's `HOME`).
///
/// Being fixed is the whole point: it is what makes the captured page byte-identical
/// across runs, which every OCR assertion in this file depends on. See
/// [`make_temp_cwd`] for the measurement that forced it and
/// [`SIDEBAR_FILLER_COUNT`] for why page stability matters at all.
///
/// Because it IS drawn, it carries the [`SIDEBAR_FILLER_FILES`] hazard for real:
/// were it named `folder`, [`empty_state_shows_open_folder_button_under_cage`] would
/// be satisfied by the header bar with no status page drawn at all.
/// [`temp_cwd_name_cannot_satisfy_any_smoke_assertion`] checks it against every
/// [`ASSERTED_OCR_TERMS`] entry, in both directions.
///
/// It must also stay SHORT and word-like: it is added glyph mass on every page, and
/// a long or noisy title would move the OCR calibration this file's fixtures are
/// measured against.
const TEMP_CWD_BASENAME: &str = "project";

/// The state directory one smoke's app is given via `WOLF_STATE_DIR`: a **sibling**
/// of `cwd`, named after it with a `-state` suffix, created eagerly.
///
/// For a plain smoke that is `<unique>/project-state`, next to the
/// `<unique>/project` [`make_temp_cwd`] returns — i.e. it sits INSIDE the directory
/// [`cage_command`] hands the app as `HOME`, which is harmless (nothing walks `HOME`;
/// the app's `FileSearch` is rooted at the cwd) and keeps the whole run in one
/// deletable tree. Nothing derives the state dir from `HOME`: `state_dir()` resolves
/// `WOLF_STATE_DIR` first.
///
/// ## Why this exists at all
///
/// `wolf_core::state::state_dir()` resolves `WOLF_STATE_DIR` first and otherwise
/// falls back to `$XDG_STATE_HOME/wolf` — on Linux `~/.local/state/wolf`. Without
/// this override every smoke writes into the DEVELOPER'S REAL state directory:
/// today the durable per-project swap dir (`swap/<hash of cwd>/`) and the three MRU
/// sqlite files, and — from the state-persistence unit onward — `state.json` itself.
/// That last one is not untidiness. `state.json` holds the developer's real saved
/// window size, working directory and pane layout, so without this helper running
/// the test suite would read it at launch and then CLOBBER it on quit with a
/// throwaway one-pane temp-cwd layout. [`cage_command`] is the single place it is
/// set, and [`cage_is_only_spawned_through_the_isolating_helper`] is what keeps it
/// single.
///
/// ## Why a SIBLING and not a child
///
/// A child of `cwd` would be walked by the app's own `FileSearch`, which excludes
/// only `.git/` and `.wolf/`. `state.json` and every `swap/<hash>/*.swp` would then
/// appear as rows in the file-tree sidebar and as picker candidates. That changes
/// the page density every OCR assertion is calibrated against (see
/// [`SIDEBAR_FILLER_COUNT`], whose measured floor exists precisely because tesseract
/// classifies a word by the whole page) and adds filenames that could satisfy — or
/// spuriously fail — an assertion. Naming it after `cwd` keeps the pairing obvious
/// when reading a post-mortem, and like the temp cwd it is deliberately not deleted.
fn smoke_state_dir(cwd: &Path) -> PathBuf {
    let name = cwd.file_name().unwrap_or_else(|| {
        panic!(
            "smoke cwd {} has no final path component, so no `<name>-state` sibling can be \
             derived for it. Every smoke's cwd comes from `make_temp_cwd`, which always produces \
             one.",
            cwd.display()
        )
    });
    let parent = cwd.parent().unwrap_or_else(|| {
        panic!(
            "smoke cwd {} has no parent directory, so no SIBLING state dir can be derived for it. \
             A child would be walked by the app's own `FileSearch` and is not an acceptable \
             fallback; see `smoke_state_dir`.",
            cwd.display()
        )
    });
    let dir = parent.join(format!("{}-state", name.to_string_lossy()));
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
        panic!(
            "failed to create the smoke state dir {}: {e}. Without it the app under cage falls \
             back to the developer's real state dir and would read and overwrite their real \
             state.json.",
            dir.display()
        )
    });
    dir
}

/// The `HOME` one smoke's app is given: the PARENT of `cwd`.
///
/// For a plain smoke that is the `<unique>` directory [`make_temp_cwd`] built, whose
/// fixed-name child `<unique>/project` is the cwd — so `title::display_title`
/// abbreviates the varying part away and the header bar renders exactly
/// `~/project`. See [`cage_command`] for why that constancy is load-bearing.
///
/// Borrows from `cwd` rather than allocating; `&Path` is an `AsRef<OsStr>`, which is
/// all `Command::env` needs.
fn smoke_home(cwd: &Path) -> &Path {
    cwd.parent().unwrap_or_else(|| {
        panic!(
            "smoke cwd {} has no parent directory, so no HOME can be derived for it. Every \
             smoke's cwd comes from `make_temp_cwd`, which always produces a fixed-name child of \
             a unique parent precisely so this is total; see `cage_command` for why HOME must be \
             that parent.",
            cwd.display()
        )
    })
}

/// The FIXED name of the one directory every smoke of a `cargo test` invocation
/// shares as its `XDG_CACHE_HOME` — see [`smoke_shader_cache_dir`], which is where the
/// rationale lives, and where the measured limit on how far that sharing reaches is
/// written down.
///
/// Deliberately does NOT begin with [`TEMP_CWD_PREFIX`], but that is a NAMING rule and
/// nothing more. It used to be justified as "this directory must survive a
/// `wolf-gnome-smoke-*` glob sweep", and that premise is false: `std::env::temp_dir()`
/// follows `TMPDIR`, which `nix develop --command` rewrites per invocation, so this
/// directory already lives inside the same per-invocation temp tree as the
/// `wolf-gnome-smoke-*` cwds and any sweep of that tree takes it too. The reason to
/// keep the name distinct is that it is SHARED by every smoke, so it must not be read
/// as one more per-smoke artifact when someone is pairing directories to runs in a
/// post-mortem. Losing it costs one cold launch, which [`smoke_shader_cache_dir`]
/// shows is covered by `SETTLE` on its own.
const SHARED_SHADER_CACHE_DIR: &str = "wolf-smoke-shader-cache";

/// The `XDG_CACHE_HOME` every smoke's app is given: ONE directory —
/// `<std::env::temp_dir()>/wolf-smoke-shader-cache` — deliberately **not** unique per
/// smoke and deliberately **not** deleted.
///
/// ## How far the sharing actually reaches (measured, and NARROWER than it looks)
///
/// Read this before relying on the word "shared". `std::env::temp_dir()` reads
/// `TMPDIR`, and `build.md` mandates that every invocation of this suite runs under
/// `nix develop --command`, which sets a FRESH per-invocation
/// `TMPDIR=/tmp/nix-shell.XXXXXX`. So this path is stable WITHIN one `cargo test`
/// invocation and moves to a brand-new, empty directory on the next one.
///
/// Measured directly, not inferred: a 100-iteration loop with one
/// `nix develop --command` per iteration created exactly 100 DISTINCT freshly-populated
/// 4.9 MB caches, none of them reused; three consecutive
/// `nix develop --command sh -c 'echo $TMPDIR'` calls returned `/tmp/nix-shell.NESmku`,
/// `/tmp/nix-shell.0Gs0Ux`, `/tmp/nix-shell.RcvfBt`; and 724 such leftover directories
/// were counted on this host.
///
/// So state honestly what this buys and what it does not. It buys: the first cage
/// smoke of an invocation pays a cold cache and the other 28 run warm, which is where
/// the pre-state-persistence settle margin came from. It does NOT buy: warmth across
/// invocations — the first smoke of EVERY invocation is cold. The section after the
/// table below is why that is acceptable rather than a defect to fix.
///
/// ## Why this exists (measured, not inferred)
///
/// Before [`cage_command`] set `HOME`, the app under `cage` inherited the developer's
/// real `$HOME`, and therefore their real, long-warm `~/.cache`. Redirecting `HOME` to
/// the per-run [`smoke_home`] — which is load-bearing for OCR determinism and must not
/// be undone — silently made `$HOME/.cache` a FRESH directory on every smoke, so mesa
/// rebuilt its shader cache (`mesa_shader_cache` + `radv_builtin_shaders`, ~4.9 MB)
/// from scratch on every single launch. That is pure added latency between spawning
/// the app and the window being able to accept keys, and `SETTLE` is the only budget
/// covering it (see [`SCENARIO`]).
///
/// Measured, 20 iterations per cell, identical code, cache warmth the only variable:
///
/// | `SETTLE` | cold cache | warm cache |
/// | --- | --- | --- |
/// | 0.15 | 0/20 | 0/20 |
/// | 0.20 | — | 0/20 |
/// | 0.25 | 0/20 | **20/20** |
/// | 0.30 | 0/20 | — |
/// | 0.35 | **20/20** | 20/20 |
/// | 0.40 | 20/20 | — |
///
/// Warm threshold ∈ (0.20, 0.25]; cold threshold ∈ (0.30, 0.35]. Pointing
/// `XDG_CACHE_HOME` at one shared directory warms the cache once per invocation and
/// keeps it warm across the smokes of THAT invocation: 28 of the 29 run warm instead
/// of all 29 running cold. That removes most of the regression at its source rather
/// than paying for it in settle time. It does not remove the cold column — the first
/// smoke of every invocation still lands in it, because the directory moves with
/// `TMPDIR` (see the section above).
///
/// ## Why one cold launch per invocation is ACCEPTABLE, not a defect to fix
///
/// Because `SETTLE = 1.0` covers the cold case on its own: it is ~2.9× the measured
/// cold threshold in the table above, and the cold arm was run deliberately — a fresh
/// `nix develop --command`, and therefore a guaranteed-empty shader cache, on every
/// one of 100 iterations — for a result of 100/100 passes. Cross-invocation warmth is
/// therefore a BONUS, not a requirement, and chasing a `TMPDIR`-proof location would
/// be a design change bought with no measured benefit.
///
/// The tripwire, so the next reader does not have to re-derive it: that argument holds
/// only while `SETTLE` carries that margin. Trim `SETTLE` and the cold case stops being
/// covered by margin alone, at which point this directory's per-invocation instability
/// starts to matter for real. The fix then is to put `SETTLE` back, not to go hunting
/// for a stabler cache path.
///
/// ## Why sharing is SAFE here, and why it is shared ON PURPOSE
///
/// [`make_temp_cwd`] and [`smoke_state_dir`] are per-run precisely to stop one run's
/// state reaching the next. **A GPU shader cache is not application state.** It is
/// content-addressed — keyed by a hash of the shader source plus the driver build —
/// and purely DERIVED: a hit hands back the compiled form of the exact same input a
/// miss would have recompiled, so the only difference a warm cache can make is
/// elapsed time. It cannot carry a fixture file, a Neovim buffer, an MRU row, a swap
/// file or a `state.json` from one smoke into the next, so none of the coupling those
/// two helpers exist to prevent is reachable through it.
///
/// So this is shared **deliberately**. Do not "fix" it into a per-smoke directory to
/// match its neighbours: that is precisely the change that reintroduced the flake, and
/// it reintroduces it invisibly — the symptom is a ~0.8% OCR failure in
/// [`window_renders_typed_text_under_cage`] weeks later, not a red run here.
///
/// ## What else `XDG_CACHE_HOME` reaches, and what it does not
///
/// Not the captive Neovim: `wolf_core::nvim::spawn` pins the child's `XDG_CACHE_HOME`
/// (with the other three `XDG_*` dirs) under the per-run `WOLF_STATE_DIR` state root,
/// so nvim keeps its own isolated, per-smoke cache regardless of what is set here. Not
/// fontconfig: `FONTCONFIG_FILE` pins both the font directory and its cache dir. Not
/// `wolf-gnome` itself, which reads no cache directory at all — `state_dir()` resolves
/// `WOLF_STATE_DIR` first and everything durable the app writes lives there. What is
/// left is the GPU/toolkit derived-artifact caches, which is exactly the target.
///
/// The one shape worth naming: on a shared machine another user may already own this
/// path. `create_dir_all` still returns `Ok` for an existing directory, and an
/// unwritable cache directory is a cache MISS rather than an error, so the expected
/// degradation is back to today's cold behaviour — not a failed run. `create_dir_all`
/// is therefore the only thing that can panic here, and only if the path exists as a
/// non-directory.
///
/// **That bounded failure mode is why a FIXED name is acceptable rather than reckless.**
/// The worst case of a name collision is the cold column of the table above, and at
/// `SETTLE = 1.0` cold still carries ~2.9× the measured cold threshold — so the
/// degradation is covered by the settle margin by construction. A per-user or per-run
/// name would buy nothing that margin does not already cover, while giving up the one
/// property this directory exists for.
fn smoke_shader_cache_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(SHARED_SHADER_CACHE_DIR);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
        panic!(
            "failed to create the shared smoke shader cache dir {}: {e}. It is handed to every \
             smoke as XDG_CACHE_HOME so mesa's shader cache stays warm across runs; without it \
             every launch rebuilds ~4.9 MB of shaders and the fixed SETTLE budget loses most of \
             its headroom. See `smoke_shader_cache_dir`.",
            dir.display()
        )
    });
    dir
}

/// The two directories, RELATIVE to the app's `$HOME`, that
/// `wolf_core::state::state_dir()` would resolve to if `WOLF_STATE_DIR` were ever
/// missing — the exact paths a leak would land in, and therefore the exact paths
/// [`single_pane_file_survives_relaunch_under_cage`]'s assertion 4 proves are empty.
///
/// `state_dir()` is `dirs::state_dir().or_else(dirs::data_dir).map(|d| d.join("wolf"))`
/// on Linux, and those two `dirs` functions default to `$XDG_STATE_HOME` (else
/// `$HOME/.local/state`) and `$XDG_DATA_HOME` (else `$HOME/.local/share`). Both arms
/// are listed because the chain has two, not because the second is expected: on Linux
/// `dirs::state_dir()` is `Some` whenever `$HOME` resolves, so `data_dir` is
/// effectively unreachable — checking it costs one `Path::exists` and removes the need
/// for anyone to re-derive that.
///
/// These are spelled out here rather than obtained from `dirs`, which `wolf-gnome`
/// does not depend on (and must not gain one for a test — see the plan's D2 /
/// amendment A4 on `Cargo.lock` and `CARGO_BAZEL_REPIN`). That makes them a MIRROR of
/// the XDG defaults, with the honest limitation written down: they are the right
/// paths only while `$XDG_STATE_HOME` / `$XDG_DATA_HOME` are unset in the environment
/// cage inherits, which is the case in this repo's devShell (`flake.nix` exports
/// `XDG_DATA_DIRS` and `XDG_RUNTIME_DIR`, neither of these). On a host that DOES
/// export one, a leak lands outside the temp tree instead — where the sibling check on
/// the developer's real state file catches it. The two checks are complementary and
/// between them cover both cases.
const HOME_STATE_DIR_FALLBACKS: [&str; 2] = [".local/state/wolf", ".local/share/wolf"];

/// Builds the `cage` invocation every smoke runs its scenario through — and is the
/// SINGLE place in this file where the compositor is ever spawned.
///
/// It fixes the invocation (`cage -- sh -c <scenario>`, with `cwd` as the working
/// directory, which is how the app comes to derive `CoreConfig.cwd` from it) plus
/// the four environment variables EVERY smoke must have:
///
/// * `WOLF_FONT_PT` = [`SMOKE_FONT_PT`] — a large font, so the rendered glyphs are
///   legible to `tesseract`, AND the exact number [`cell_metrics_for_smoke`]
///   measures against: the pointer smokes derive every pixel target from it, so the
///   app and this test must be handed the same string. The `:qa` smoke captures no
///   image at all and keeps it only for parity, so that a manual repro of that
///   scenario still renders legibly.
/// * `WOLF_STATE_DIR` = [`smoke_state_dir`] — the isolation that keeps a smoke run
///   out of the developer's real `~/.local/state/wolf/`. Read that function's doc
///   comment before changing anything here. It is also what lets the two RELAUNCH
///   smokes work at all: both launches inside one scenario are handed the same
///   directory, so the second reads exactly the `state.json` the first wrote.
/// * `HOME` = [`smoke_home`] — the cwd's PARENT, which is what pins the rendered
///   window title to the constant `~/project`. See below; this one is an OCR
///   DETERMINISM control, not tidiness.
/// * `XDG_CACHE_HOME` = [`smoke_shader_cache_dir`] — ONE directory shared by every
///   smoke of a `cargo test` invocation, so mesa's shader cache is warm for all of
///   them but the first. It does NOT carry into the next invocation: the path is
///   under `TMPDIR`, which `nix develop --command` rewrites every time (measured; see
///   that function). This is the only one of the four that is deliberately NOT
///   per-smoke, and it is the direct counterweight to the `HOME` line above:
///   redirecting `HOME` also redirected `$HOME/.cache`, which made every launch
///   rebuild ~4.9 MB of shaders and cost `SETTLE` most of its measured headroom. Read
///   that function's doc comment — including why sharing carries no state between
///   smokes, and why the one cold launch per invocation is covered by `SETTLE`
///   alone — before changing it, and do not make it per-smoke.
///
/// ## Why `HOME` is set, and why it is the cwd's parent
///
/// Since the state-persistence unit the `adw::HeaderBar` renders the app's working
/// directory, so the temp directory's name is page text in every capture — and
/// `tesseract` classifies a word by the WHOLE page (see [`SIDEBAR_FILLER_COUNT`]).
/// While the title drew the run-varying `<tmpdir>/wolf-gnome-smoke-<pid>-<nanos>`,
/// the captured page differed on every run, measured:
///
/// | arm | five consecutive capture md5s | size |
/// | --- | --- | --- |
/// | title rendering the run-varying temp path | all five DIFFERENT | 34.2–35.0 KB, varying |
/// | `set_title` suppressed (the base page) | `5ea4d070…` ×5, byte-identical | 26074 ×5 |
///
/// and [`window_renders_typed_text_under_cage`] failed ~2% of runs (2 in ~93) with
/// the documented ligature signature. Setting `HOME` to the cwd's parent makes
/// `title::display_title`'s home abbreviation collapse the entire varying prefix to
/// `~`, leaving the constant **`~/project`** ([`TEMP_CWD_BASENAME`]) — so the page is
/// bit-for-bit reproducible again, as it was before the title existed. **Removing
/// this line, or reintroducing a run-varying cwd name, reintroduces that flake**, and
/// raising [`SIDEBAR_FILLER_COUNT`] cannot fix it.
///
/// `HOME` is safe to move here. Everything else the app resolves from it is either
/// overridden (`state_dir()` reads `WOLF_STATE_DIR` first; the captive Neovim runs
/// `--clean -u <init.lua> -i NONE` with all four `XDG_*` dirs pinned under the state
/// root) or absent-and-defaulted (GTK finds no `$HOME/.config/gtk-4.0`, so it uses
/// its defaults — which is exactly how
/// [`empty_state_shows_open_folder_button_under_cage`] has always run). Fontconfig is
/// unaffected: `FONTCONFIG_FILE` pins both the font directory and a `/tmp` cache dir.
///
/// One consequence was NOT harmless and had to be paid for separately: `$HOME/.cache`
/// moved too, so mesa's shader cache went cold on every launch. That is what the
/// explicit `XDG_CACHE_HOME` above exists to undo; see [`smoke_shader_cache_dir`] for
/// the measured cost and for why one shared directory is the right shape.
///
/// The cwd's parent is also never equal to the cwd, so `workspace::launch_decision`'s
/// `cwd == home` branch does not fire and every smoke stays in the `Folder` branch —
/// except the one that deliberately points both at the same directory.
///
/// Two smokes chain their OWN `HOME` after this call and deliberately win:
/// [`empty_state_shows_open_folder_button_under_cage`] (`HOME` == the cwd, to fire
/// the `EmptyState` branch) and
/// [`window_title_shows_the_working_directory_under_cage`] (`HOME` == its nested
/// project's parent, which is the value its abbreviation assertion is written
/// against). Both still render a CONSTANT title (`~` and `~/titlezulu`
/// respectively), so both keep the determinism property this default exists for.
///
/// Callers chain only their own scenario-specific `.env(...)` / `.arg(...)`
/// additions on top. Two per-smoke notes that used to live on the `WOLF_FONT_PT`
/// line stay at their call sites: the vsplit / hsplit / three-pane smokes point at
/// the "Note (OCR width)" / "Note (OCR height)" paragraphs above their own consts,
/// because if two or three panes are ever too narrow or too short for 28 pt OCR the
/// fix is per-test (widen the window, or lower the font for THAT test) and must not
/// be applied here to all twenty-nine.
///
/// [`cage_is_only_spawned_through_the_isolating_helper`] enforces the "single place"
/// property mechanically, because a smoke that spawned the compositor directly would
/// silently lose ALL FOUR variables.
fn cage_command(scenario: &str, cwd: &Path) -> Command {
    let mut cmd = Command::new("cage");
    cmd.arg("--")
        .arg("sh")
        .arg("-c")
        .arg(scenario)
        .current_dir(cwd)
        .env("HOME", smoke_home(cwd))
        .env("XDG_CACHE_HOME", smoke_shader_cache_dir())
        .env("WOLF_FONT_PT", SMOKE_FONT_PT)
        .env("WOLF_STATE_DIR", smoke_state_dir(cwd));
    cmd
}

/// The source text that spawns the compositor, assembled from THREE FRAGMENTS so
/// that this file never contains it contiguously anywhere except inside
/// [`cage_command`].
///
/// The fragment construction is load-bearing, not stylistic — see
/// [`cage_is_only_spawned_through_the_isolating_helper`], which scans this file's own
/// source for it. Written as one contiguous literal it would match ITSELF, the count
/// would be two, and the guard would fail permanently no matter how correct the code
/// was. Do not "simplify" it.
const CAGE_SPAWN_NEEDLE: &str = concat!("Command::new(\"", "cage", "\")");

/// Enforces mechanically that the compositor is spawned in exactly ONE place in this
/// file — [`cage_command`] — which is the only thing that sets `WOLF_STATE_DIR`,
/// `HOME` and `XDG_CACHE_HOME`.
///
/// ## What an extra occurrence costs
///
/// A smoke that builds its own `cage` command inherits the test process's
/// environment and nothing else, so its app resolves `wolf_core::state_dir()` to the
/// developer's REAL `~/.local/state/wolf/`. It then reads their real `state.json` at
/// launch — restoring their window size, working directory and pane layout into a
/// smoke that is supposed to start from a clean slate — and overwrites it at quit
/// with a throwaway one-pane temp-cwd layout. The developer's saved session is
/// destroyed by running the test suite, and the smoke stops being reproducible
/// because its starting state now depends on what the developer last had open. That
/// is the failure this guard exists to make impossible, and it is invisible in review:
/// a new smoke is a hundred lines of scenario in which one `.env` line is missing.
///
/// It costs a second thing now. Inheriting the developer's real `HOME` means the
/// header-bar title no longer abbreviates the temp cwd, so that smoke draws the full
/// `<tmpdir>/wolf-gnome-smoke-<pid>-<nanos>/project` — a DIFFERENT page on every run,
/// which is measured to make OCR stochastic (see [`cage_command`] and
/// [`SIDEBAR_FILLER_COUNT`] for the md5 evidence). That failure is worse than the
/// state-dir one in one respect: it is intermittent, so it looks like a flake in
/// whatever change happens to be in flight rather than like a missing `.env` line.
///
/// And a third, of the same intermittent kind: without [`cage_command`]'s
/// `XDG_CACHE_HOME` the app's mesa shader cache is whatever the inherited environment
/// gives it, which under a redirected `HOME` is a cold per-run directory. That costs
/// ~4.9 MB of shader rebuild inside the fixed `SETTLE` budget and was measured to turn
/// a 20/20 cell into 0/20 at settle values that pass warm — again surfacing as an OCR
/// flake somewhere else entirely. See [`smoke_shader_cache_dir`].
///
/// ## Why the needle is built from fragments, and why that is load-bearing
///
/// This test reads its own source with `include_str!`. Were [`CAGE_SPAWN_NEEDLE`]
/// spelled as one contiguous literal, the const's own line would contain it, the
/// count would be two, and the test would be red forever. The tempting "fix" for a
/// permanently-red count is to relax it to `<= 1` — at which point a needle that
/// matches NOTHING (someone renames the binary, or `cage` moves behind a const)
/// satisfies it vacuously and the guard silently stops guarding. That is exactly the
/// vacuity shape recorded in `crates/wolf-gnome/tests/README.md` ("Vacuity is not
/// only a fixture-naming problem"), where a check aimed at the right property was
/// looking somewhere the defect could not appear. So the count is asserted from BOTH
/// sides: at least one (the guard is not vacuous) and at most one (the invariant
/// holds).
///
/// This test is PURE: no compositor, no `require_smoke_env`, no `CAGE_LOCK`, no
/// subprocess — the same shape as
/// [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`], and it runs in
/// microseconds on a machine with no smoke toolchain at all.
#[test]
fn cage_is_only_spawned_through_the_isolating_helper() {
    let source = include_str!("smoke_headless.rs");
    let occurrences = source.matches(CAGE_SPAWN_NEEDLE).count();

    assert!(
        occurrences > 0,
        "the compositor-spawn needle {CAGE_SPAWN_NEEDLE:?} does not appear in this file's own \
         source AT ALL, so this guard is VACUOUS: it would accept any number of unguarded spawns \
         because it can no longer recognize one. Either the spawn in `cage_command` was reworded \
         (a binary name moved behind a const, a builder replaced the constructor, rustfmt split \
         the call across lines), or `include_str!` is no longer reading this file. Fix the needle \
         or the helper so the two agree again — do NOT relax the exactly-once check below to an \
         upper bound, which is what would let this zero-match state pass."
    );
    assert_eq!(
        occurrences, 1,
        "expected exactly ONE occurrence of {CAGE_SPAWN_NEEDLE:?} in this file (the one inside \
         `cage_command`), found {occurrences}. Every extra one is a compositor spawned OUTSIDE \
         `cage_command` and therefore WITHOUT `WOLF_STATE_DIR`: that smoke's app resolves \
         `state_dir()` to the developer's real `~/.local/state/wolf/`, READS their real \
         state.json at launch (so it starts from the developer's last session instead of a clean \
         slate) and OVERWRITES it on quit with its own throwaway temp-cwd layout. Route the new \
         smoke through `cage_command(scenario, &cwd)` and chain only its own `.env(...)` \
         additions on top; do not copy the helper's body."
    );
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
    let mut cmd = cage_command(&pointer_scenario(body), cwd);
    cmd.env("WOLF_BIN", bin)
        .env("OUT_PNG", out_png)
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
            "{smoke}: expected screenshot {} to exist after its cage scenario, but: {e}",
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
             A tail containing a SPACE would also land here, which no fixture in this file has.",
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

/// The MODELLED on-screen x of the divider that follows column `index` (1-based)
/// when the editor pane is divided into `columns` equal columns, in output pixels.
///
/// A model, and knowingly a rough one: a `gtk::Paned` positions its divider at a
/// fraction of its TRAVEL rather than of its width, so this over-estimates by the
/// handle and the far child's minimum — worth ~15-20 px per divider here. It exists
/// only to seed a bisection, which is what actually finds the handle.
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
fn divider_seed_span(index: i64, columns: i64) -> (i64, i64) {
    let nominal = equal_share_divider_x(index, columns);
    let margin = exact_f64(BISECT_SEED_MARGIN_PX, "BISECT_SEED_MARGIN_PX");
    (
        pixel_target(nominal - margin, "the bisection LO seed"),
        pixel_target(nominal + margin, "the bisection HI seed"),
    )
}

/// Parses a name-keyed pane-width readback: whitespace-separated
/// `<columns>:<buffer tail>` pairs, one per Neovim window, in whatever order
/// Neovim enumerated them.
///
/// Keyed by NAME rather than by position because nvim numbers its windows by its
/// own layout, which need not match the order this front-end draws the panes in.
///
/// The columns are `winwidth()`, the GRID width `resize_grid` drives — NOT
/// `nvim_win_get_width()`, which reports the layout width and is measurably blind
/// to a divider drag. [`DIVIDER_RELAUNCH_SCENARIO`]'s `read_widths` carries the
/// measurement; do not "simplify" the readback back to the API call.
fn read_pane_widths(path: &Path, smoke: &str) -> Vec<(String, i64)> {
    let raw = read_readback(path, smoke);
    let mut out = Vec::new();
    for token in raw.split_whitespace() {
        let (width, name) = token.split_once(':').unwrap_or_else(|| {
            panic!(
                "{smoke}: pane-width token {token:?} in {} is not `<columns>:<buffer tail>`. Raw \
                 contents were {raw:?}.",
                path.display()
            )
        });
        out.push((
            name.to_string(),
            parse_readback_int(width, path, &raw, smoke),
        ));
    }
    assert!(
        !out.is_empty(),
        "{smoke}: the pane-width readback {} holds no `<columns>:<buffer tail>` pairs at all. Raw \
         contents were {raw:?}.",
        path.display()
    );
    out
}

/// The head pane's share of a head/tail pair of widths, as a fraction.
///
/// The pair rather than the whole window: the two panes either side of one
/// `gtk::Paned` are what that paned's position governs, and every other pane's
/// width is irrelevant to it.
fn head_width_ratio(head: i64, tail: i64, smoke: &str, what: &str) -> f64 {
    let total = head + tail;
    assert!(
        total > 0,
        "{smoke}: {what} report {head} and {tail} cells, a total of {total}. A ratio cannot be \
         taken of that, and a non-positive pane width means Neovim was never resized to the \
         widget's allocation at all."
    );
    exact_f64(head, "a pane width") / exact_f64(total, "a pane-pair total width")
}

/// The file NAME of a persisted leaf's path — `None` for a split, and `None` for a
/// leaf whose path is empty or has no final component.
fn leaf_tail(node: &PaneNode) -> Option<&str> {
    match node {
        PaneNode::Leaf { path, .. } => Path::new(path).file_name().and_then(OsStr::to_str),
        PaneNode::Split { .. } => None,
    }
}

/// The width, in columns, of the pane showing the buffer whose tail name is `name`.
///
/// Panics naming every pane that WAS present, because "the pane holding this file
/// is gone" and "the pane holding this file is the wrong width" are different
/// findings and must not share a message.
fn pane_width_for(panes: &[(String, i64)], name: &str, path: &Path, smoke: &str) -> i64 {
    panes
        .iter()
        .find(|(buffer, _)| buffer == name)
        .map(|(_, width)| *width)
        .unwrap_or_else(|| {
            panic!(
                "{smoke}: no pane in {} is showing {name:?}. The panes present were {panes:?}. An \
                 `Untitled.md` here means the file was never opened (run 1) or never restored (run \
                 2); an EMPTY name means a pane is bound to an unnamed buffer.",
                path.display()
            )
        })
}

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
         four candidate causes, in order of likelihood: (1) an `adw::HeaderBar` height change (a \
         libadwaita/GTK bump), which moves the overlay's top edge and therefore its height; (2) \
         an `app::TREE_PANE_WIDTH` or `gtk::Paned` divider-width change, which moves its left \
         edge; (3) a cage output-size change, which moves the bottom-right corner; (4) \
         `crash_overlay.rs` having been rewritten to use an `adw::StatusPage`, or its root \
         `gtk::Box` having been given a NON-ZERO spacing — either of which breaks the derivation \
         outright, because `restart_button_rect` models exactly two `vexpand` spacers around a \
         fixed message+gap+button block with the gap living in the label's `margin_bottom`. Fix \
         the geometry or re-measure the constants; do NOT shrink the margin to make this pass."
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
/// [`RESTART_OCR_HEADING`] is listed once, against the smoke that asserts it is
/// PRESENT, but BOTH crash smokes assert on it — the crash-overlay smoke needs it
/// present after the button click and ABSENT during the overlay dwell, which is how
/// it proves the overlay is opaque. A filler row carrying it would break both.
///
/// The two BUFPICK terms are the same shape reached differently: each is listed once,
/// against the smoke that asserts it PRESENT, and the OTHER buffer-picker smoke
/// asserts the same term ABSENT. [`BUFPICK_OCR_PRESENT`] is present after the query
/// smoke's typed query and absent after the bare-Enter smoke's preselection;
/// [`BUFPICK_OCR_ABSENT`] is the reverse. A filler row, a temp-cwd component or a
/// fixture name carrying either would break one smoke and vacuously satisfy the
/// other, which is why one row per term is enough to guard both.
///
/// [`RESTART_OCR_UNSAVED`] is likewise asserted in BOTH directions by the SAME smoke:
/// present in the post-recovery capture (the unsaved edit came back out of the dead
/// Neovim's swap file) and absent from the pre-crash control (which is taken before
/// the edit is typed). A filler row carrying it would satisfy the first half and fail
/// the second, i.e. it would turn a real proof into a spurious red — the same fixture
/// bug either way, which is why it belongs here.
///
/// The three STATE-PERSISTENCE terms that are asserted ABSENT
/// ([`SOLO_OCR_ABSENT`], [`TITLE_OCR_ABSENT`]) are listed for the same reason as
/// the other must-NOT-contain rows. [`TITLE_OCR_ABSENT`] is the one whose fixture
/// deliberately CONTAINS it — the smoke's HOME directory is named after it, and
/// the assertion is precisely that the home segment is abbreviated to `~` and
/// therefore never drawn. That is why
/// [`restore_fixture_names_cannot_satisfy_their_own_assertions`] carries an
/// explicit, documented exemption for the two title directory names rather than
/// widening this table's guards to cover them.
///
/// Twenty-four of the twenty-nine smokes appear here. FIVE contribute no rows at
/// all, on purpose, and it is worth being explicit about which and why:
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
/// * [`divider_drag_survives_a_pane_relayout_under_cage`] for the same reason: it
///   asserts on the pane WIDTHS Neovim reports for its own windows, writes no
///   fixture file at all, and its retained screenshot is post-mortem evidence
///   rather than an observable.
///
/// The other two pointer smokes DO OCR and their terms are listed below. For
/// [`click_then_close_keeps_keyboard_input_alive_under_cage`] the OCR pair is
/// corroboration (the close really happened, the survivor kept rendering, the app
/// was still drawing at all) and the load-bearing assertions are its two `ALIVE*`
/// readback files; for [`crash_overlay_and_restart_button_recover_under_cage`] the
/// OCR IS the assertion, because "the overlay is up" and "the overlay is opaque"
/// are properties of pixels and of nothing else.
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
        PALETTE_OCR_PRESENT,
        "PALETTE_OCR_PRESENT",
        "command_palette_chord_runs_the_selected_command_under_cage",
    ),
    (
        PALETTE_OCR_ABSENT,
        "PALETTE_OCR_ABSENT",
        "command_palette_chord_runs_the_selected_command_under_cage",
    ),
    (
        BUFPICK_OCR_PRESENT,
        "BUFPICK_OCR_PRESENT",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        BUFPICK_OCR_ABSENT,
        "BUFPICK_OCR_ABSENT",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        BUFPICK_OCR_CURRENT,
        "BUFPICK_OCR_CURRENT",
        "buffer_picker_bare_enter_switches_to_the_previous_buffer_under_cage",
    ),
    (
        STALL_OCR_PRESENT,
        "STALL_OCR_PRESENT",
        "stalled_nvim_raises_the_stall_overlay_under_cage",
    ),
    (
        STALL_OCR_ABSENT,
        "STALL_OCR_ABSENT",
        "stalled_nvim_raises_the_stall_overlay_under_cage",
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
    (
        RESTART_OCR_HEADING,
        "RESTART_OCR_HEADING",
        "nvim_crash_recovers_and_reopens_the_file_under_cage",
    ),
    (
        RESTART_OCR_UNSAVED,
        "RESTART_OCR_UNSAVED",
        "nvim_crash_recovers_and_reopens_the_file_under_cage",
    ),
    (
        RESTART_OCR_BANNER,
        "RESTART_OCR_BANNER",
        "nvim_crash_recovers_and_reopens_the_file_under_cage",
    ),
    (
        RESTART_OCR_OVERLAY,
        "RESTART_OCR_OVERLAY",
        "crash_overlay_and_restart_button_recover_under_cage",
    ),
    (
        RESTORE_OCR_ALFA,
        "RESTORE_OCR_ALFA",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        RESTORE_OCR_BRAVO,
        "RESTORE_OCR_BRAVO",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        RESTORE_OCR_CHARLIE,
        "RESTORE_OCR_CHARLIE",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        SOLO_OCR_PRESENT,
        "SOLO_OCR_PRESENT",
        "single_pane_file_survives_relaunch_under_cage",
    ),
    (
        SOLO_OCR_ABSENT,
        "SOLO_OCR_ABSENT",
        "single_pane_file_survives_relaunch_under_cage",
    ),
    (
        TITLE_OCR_PRESENT,
        "TITLE_OCR_PRESENT",
        "window_title_shows_the_working_directory_under_cage",
    ),
    (
        TITLE_OCR_ABSENT,
        "TITLE_OCR_ABSENT",
        "window_title_shows_the_working_directory_under_cage",
    ),
    (
        DIVIDER_OCR_LEFT,
        "DIVIDER_OCR_LEFT",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        DIVIDER_OCR_RIGHT,
        "DIVIDER_OCR_RIGHT",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
];

/// Every fixture NAME a smoke creates inside its OWN working directory, as
/// `(name, const name, the smoke that owns it)`.
///
/// The tree sidebar renders every cwd entry BY NAME, so each of these is page text
/// in its own smoke's capture — the same hazard [`SIDEBAR_FILLER_FILES`] carries,
/// aimed at a THIRD fixture. The readback files are listed too: they land in the
/// temp cwd alongside the markdown, so the sidebar draws them as well.
///
/// The state-persistence smokes were the first entries here and the name records
/// that, but the table is not limited to them: any smoke whose own fixture could
/// satisfy — or spuriously break — an [`ASSERTED_OCR_TERMS`] entry belongs in it,
/// and the command-palette smoke's base file is registered here for exactly that
/// reason. A smoke whose fixture is absent from this table is a smoke whose fixture
/// nothing checks, which is precisely how
/// `picker_opens_and_loads_selected_file_under_cage` once passed green while the
/// picker never opened.
///
/// The values are the CONSTS themselves, never re-typed literals, for the same
/// reason [`ASSERTED_OCR_TERMS`] is.
const RELAUNCH_FIXTURE_NAMES: &[(&str, &str, &str)] = &[
    (
        RESTORE_FILE_LEFT,
        "RESTORE_FILE_LEFT",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        RESTORE_FILE_MID,
        "RESTORE_FILE_MID",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        RESTORE_FILE_RIGHT,
        "RESTORE_FILE_RIGHT",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        RESTORE_READBACK_NAME,
        "RESTORE_READBACK_NAME",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        SOLO_FILE,
        "SOLO_FILE",
        "single_pane_file_survives_relaunch_under_cage",
    ),
    (
        SOLO_DECOY_FILE,
        "SOLO_DECOY_FILE",
        "single_pane_file_survives_relaunch_under_cage",
    ),
    (
        SOLO_READBACK_NAME,
        "SOLO_READBACK_NAME",
        "single_pane_file_survives_relaunch_under_cage",
    ),
    (
        DIVIDER_FILE_LEFT,
        "DIVIDER_FILE_LEFT",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        DIVIDER_FILE_RIGHT,
        "DIVIDER_FILE_RIGHT",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        DIVIDER_W0_NAME,
        "DIVIDER_W0_NAME",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        DIVIDER_W1_NAME,
        "DIVIDER_W1_NAME",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        DIVIDER_W2_NAME,
        "DIVIDER_W2_NAME",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        PALETTE_FILE,
        "PALETTE_FILE",
        "command_palette_chord_runs_the_selected_command_under_cage",
    ),
    (
        BUFPICK_FILE_TARGET,
        "BUFPICK_FILE_TARGET",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        BUFPICK_FILE_PRESELECT,
        "BUFPICK_FILE_PRESELECT",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        BUFPICK_FILE_CURRENT,
        "BUFPICK_FILE_CURRENT",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        BUFPICK_READBACK_NAME,
        "BUFPICK_READBACK_NAME",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        STALL_FILE,
        "STALL_FILE",
        "stalled_nvim_raises_the_stall_overlay_under_cage",
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
/// ([`CHORD_CLOSE_OCR_KEEP`]) and `chordgone` ([`CHORD_CLOSE_OCR_GONE`]); — added
/// with the pointer-harness unit — `wedgestay` ([`WEDGE_OCR_STAY`]) and `wedgeclick`
/// ([`WEDGE_OCR_CLICK`]); and — added with the crash-recovery unit — `bounceback`
/// ([`RESTART_OCR_HEADING`]), `unsavedburst` ([`RESTART_OCR_UNSAVED`]), `restarted`
/// ([`RESTART_OCR_BANNER`]) and `crashed` ([`RESTART_OCR_OVERLAY`]); and — added
/// with the state-persistence unit — `restorealfa` / `restorebravo` /
/// `restorecharlie` ([`RESTORE_OCR_ALFA`] / [`RESTORE_OCR_BRAVO`] /
/// [`RESTORE_OCR_CHARLIE`]), `solozulu` ([`SOLO_OCR_PRESENT`]), `soloalfa`
/// ([`SOLO_OCR_ABSENT`]), `titlezulu` ([`TITLE_OCR_PRESENT`]) and `titlehome`
/// ([`TITLE_OCR_ABSENT`]); and — added with the divider-drag unit — `divideralfa`
/// and `dividerbravo` ([`DIVIDER_OCR_LEFT`] / [`DIVIDER_OCR_RIGHT`]); and — added
/// with the command-palette unit — `zzpalrun` ([`PALETTE_OCR_PRESENT`]) and
/// `palcmdbase` ([`PALETTE_OCR_ABSENT`]); and — added with the buffer-picker unit —
/// `bufpickalfa` ([`BUFPICK_OCR_PRESENT`]), `bufpickbravo`
/// ([`BUFPICK_OCR_ABSENT`]), `bufpickcharlie` ([`BUFPICK_OCR_CURRENT`]),
/// `responding` ([`STALL_OCR_PRESENT`]) and
/// `stallfixture` ([`STALL_OCR_ABSENT`]). None is a
/// substring of any [`SIDEBAR_FILLER_FILES`] entry, and no filler may be renamed to
/// make one so. Five smokes contribute no term:
/// [`colon_qa_closes_the_window_under_cage`] asserts on the cage exit status and
/// captures no image, and the click, drag, scroll and divider-relayout smokes assert
/// on files the captive Neovim writes about its own state rather than on pixels.
///
/// ## What it does NOT guard (and why that matters)
///
/// It inspects [`SIDEBAR_FILLER_FILES`] only — never a smoke's OWN fixture
/// filenames — so it cannot catch a smoke whose fixture file is named after the
/// word that smoke asserts. (The state-persistence smokes' own fixtures ARE covered,
/// but by a separate guard:
/// [`restore_fixture_names_cannot_satisfy_their_own_assertions`], so the two
/// failures stay separately diagnosable.) That gap was live, and an audit with the
/// command-drain unit found exactly two instances:
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
/// contain the asserted word.** There are two deliberate exceptions, and both are
/// smokes whose whole subject IS the rendered NAME:
/// [`tree_sidebar_shows_files_under_cage`] (so [`TREE_OCR`] is a filename word on
/// purpose) and [`window_title_shows_the_working_directory_under_cage`] (whose two
/// DIRECTORY names are its asserted terms). That is why this guard cannot simply be
/// widened to cover fixture names without an opt-out list, and why the
/// state-persistence fixtures got their own guard with the exemption written down.
///
/// This test is PURE: no compositor, no `require_smoke_env`, no `CAGE_LOCK`, no
/// subprocess. It reads two const tables and finishes in microseconds, so it adds
/// nothing to the smoke suite's runtime and — unlike the twenty-nine smokes — still
/// guards the fixture on a machine where the smoke toolchain is unavailable.
#[test]
fn sidebar_filler_names_cannot_satisfy_any_smoke_assertion() {
    // Guard the guard: both OCR readers — `ocr_to_lowercase` and
    // `ocr_uniform_block_to_lowercase` — lowercase tesseract's output before every
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

/// The same invariant as [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`],
/// aimed at a SECOND fixture: the two fixed parts of every [`make_temp_cwd`] path —
/// [`TEMP_CWD_BASENAME`], which the header bar actually draws, and
/// [`TEMP_CWD_PREFIX`], which it draws only if the home abbreviation regresses.
///
/// ## Why the temp directory's NAME became page text
///
/// It did not used to be. The window title was the constant `"Wolf"`, so the only
/// on-screen text a smoke could not account for came from file names. Since the
/// window title renders the app's WORKING DIRECTORY, every smoke now draws part of
/// its own temp-cwd path in the header bar, in the theme font, dark-on-light — i.e.
/// in the most legible text on the page — and `tesseract` reads it along with
/// everything else. A basename of `folder` would satisfy
/// [`empty_state_shows_open_folder_button_under_cage`]'s assertion from the header
/// bar alone, and that smoke would pass green with no status page drawn at all.
///
/// The two directions both matter, exactly as they do for the filler names: a
/// must-CONTAIN term in the path makes a smoke pass vacuously, and a must-NOT-CONTAIN
/// term makes it fail spuriously. [`ASSERTED_OCR_TERMS`] carries both kinds and this
/// guard checks against all of them.
///
/// ## What is drawn, and what this guard therefore covers
///
/// [`make_temp_cwd`] returns `<unique>/project` and [`cage_command`] sets `HOME` to
/// `<unique>`, so `title::display_title` abbreviates and the rendered title is the
/// constant `~/project`. So [`TEMP_CWD_BASENAME`] is the part that is REALLY on the
/// page, and it is checked first. [`TEMP_CWD_PREFIX`] and its `{pid}-{nanos}` tail
/// are normally abbreviated away — but they are still checked, because a regression
/// in the abbreviation draws the whole path, and a prefix carrying an asserted term
/// would turn that clean red into a mysterious pass (or a mysterious spurious
/// failure) in a different smoke entirely. Guarding the invisible half costs one loop
/// and keeps the diagnosis honest.
///
/// ## Why it is a separate test from the filler guard
///
/// So the two failures stay separately diagnosable: one says "rename a filler file",
/// the other says "rename the temp-directory basename or prefix", and they have
/// different fixes. The filler guard is deliberately left untouched.
///
/// ## What it does not cover
///
/// Only the two fixed components. The variable part of the path is the process id and
/// a nanosecond timestamp — ASCII digits and `-` — which the third loop below proves
/// cannot contain any asserted term rather than merely asserting it in prose. The
/// PARENT of the unique directory (`std::env::temp_dir()`, i.e. `TMPDIR` or `/tmp`)
/// is NOT checked, because it is a property of the host rather than of this fixture;
/// it is abbreviated away today, but a `TMPDIR` containing an asserted term would
/// show up as a mysterious pass or failure if the abbreviation ever broke, and this
/// note is the place to start looking.
///
/// PURE, like its two siblings: no compositor, no `require_smoke_env`, no
/// `CAGE_LOCK`, no subprocess.
#[test]
fn temp_cwd_name_cannot_satisfy_any_smoke_assertion() {
    // Guard the guard: an empty term table would make every loop below run zero times
    // and pass for the wrong reason.
    assert!(
        !ASSERTED_OCR_TERMS.is_empty(),
        "`ASSERTED_OCR_TERMS` is empty, so this guard checks nothing at all. It is the single \
         registry of every substring the cage smokes assert OCR must or must not contain; if a \
         smoke's term is missing from it, neither this guard nor \
         `sidebar_filler_names_cannot_satisfy_any_smoke_assertion` can protect that smoke."
    );

    // Well-formedness of the basename, because the whole determinism fix rests on it
    // being exactly ONE ordinary path segment: an empty value (or `.`) would make
    // `make_temp_cwd` return the unique parent itself, so HOME would become the temp
    // ROOT and the title would draw the varying name again; an embedded separator
    // would add a level `smoke_home` does not account for; and `..` would climb out.
    let mut components = Path::new(TEMP_CWD_BASENAME).components();
    let first = components.next();
    let extra = components.next();
    assert!(
        matches!(first, Some(std::path::Component::Normal(_))) && extra.is_none(),
        "TEMP_CWD_BASENAME = {TEMP_CWD_BASENAME:?} is not a single ordinary path segment (it \
         parses as {first:?} followed by {extra:?}). `make_temp_cwd` joins it onto a unique parent \
         and `cage_command` hands that parent to the app as HOME, so the header-bar title reads \
         `~/{TEMP_CWD_BASENAME}`. An empty value collapses the two into one directory — HOME == \
         cwd, which also flips `workspace::launch_decision` into its EmptyState branch for EVERY \
         smoke — and a separator or a `..` moves the level HOME is taken from; both put \
         run-varying text back on the page. See `make_temp_cwd` for the measured cost of that."
    );

    // The component that IS rendered: `HOME` is the cwd's parent, so the header bar
    // reads `~/<TEMP_CWD_BASENAME>` in every smoke that does not override HOME.
    let lowercased = TEMP_CWD_BASENAME.to_lowercase();
    for &(term, const_name, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            !lowercased.contains(term),
            "the temp-cwd basename {TEMP_CWD_BASENAME:?} contains {const_name} = {term:?}. Every \
             smoke's window title renders its working directory home-abbreviated, so this word is \
             drawn in the header bar of EVERY capture as `~/{TEMP_CWD_BASENAME}`: it would satisfy \
             (or, for a must-NOT-contain term, spuriously break) `{smoke}`'s OCR assertion with \
             nothing else on screen having to work at all. Rename `TEMP_CWD_BASENAME` — keep it \
             short and word-like, since it is glyph mass on every page — and do NOT change the OCR \
             const or touch the smoke's assertion."
        );
    }

    // The component that is normally abbreviated away, checked anyway: if
    // `display_title` ever stops abbreviating, the FULL path is what gets drawn, and
    // this is what keeps that bug from masquerading as a different smoke's failure.
    let lowercased = TEMP_CWD_PREFIX.to_lowercase();
    for &(term, const_name, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            !lowercased.contains(term),
            "the temp-cwd prefix {TEMP_CWD_PREFIX:?} contains {const_name} = {term:?}. It names \
             the UNIQUE parent directory, which `cage_command` hands the app as HOME and which \
             `title::display_title` therefore abbreviates to `~` — so it is not drawn today. But a \
             regression in that abbreviation draws the whole path in the header bar, at which \
             point this word would satisfy (or, for a must-NOT-contain term, spuriously break) \
             `{smoke}`'s OCR assertion and the abbreviation bug would surface as an unrelated \
             smoke behaving strangely instead of as a clean red. Rename `TEMP_CWD_PREFIX`; do NOT \
             change the OCR const and do NOT touch the smoke's assertion."
        );
    }

    // The variable part of the path is `{pid}-{nanos}`, so it is ASCII digits and `-`
    // only. That is safe if and only if no asserted term is spellable in those
    // characters — which is a property of the terms, so it is checked here rather than
    // asserted in a comment.
    for &(term, const_name, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            term.chars().any(|c| c.is_ascii_alphabetic()),
            "asserted OCR term {const_name} = {term:?} contains no ASCII letter, so it could be a \
             substring of the digits-and-hyphens tail `make_temp_cwd` appends to \
             {TEMP_CWD_PREFIX:?} (the process id and a nanosecond timestamp). `{smoke}` would \
             then be satisfied — or spuriously broken — by whatever pid the test harness happened \
             to get, on any run where the title's home abbreviation is not doing its job. Give the \
             term a letter, or stop deriving the temp-dir name from numbers."
        );
    }
}

/// The same invariant again, aimed at a THIRD fixture: the SMOKE-OWNED files listed
/// in [`RELAUNCH_FIXTURE_NAMES`] — the ones a smoke writes into its own working
/// directory, of which the state-persistence smokes' were the first — plus the two
/// directory names the title smoke builds.
///
/// ## Why these fixtures need their own guard
///
/// [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`] inspects
/// [`SIDEBAR_FILLER_FILES`] and nothing else, and its own doc comment records the
/// gap: it "cannot catch a smoke whose fixture file is named after the word that
/// smoke asserts", which is exactly how
/// `picker_opens_and_loads_selected_file_under_cage` once passed green while the
/// picker provably never opened, and how
/// `vertical_split_shows_two_panes_under_cage` shipped `paneleft.md` /
/// `paneright.md` against `paneleft` / `paneright`. Every smoke in this table
/// asserts on FILE CONTENT (a rendered heading) while dropping its fixture into a
/// cwd the tree sidebar lists BY NAME, so each is in precisely that hazard class —
/// the state-persistence smokes, which brought the table into being, and the
/// command-palette smoke, whose `pbase.md` must never come to contain `palcmdbase`.
/// Both directions matter: a must-CONTAIN term in a fixture name makes its smoke
/// pass vacuously, and a must-NOT-CONTAIN term makes it fail spuriously.
///
/// ## The one deliberate, documented exemption
///
/// The title smoke's two DIRECTORY names are not files and are not in
/// [`RELAUNCH_FIXTURE_NAMES`], because they are the assertion's subject rather
/// than a hazard to it: [`TITLE_PROJECT_DIR`] must contain
/// [`TITLE_OCR_PRESENT`] (the header bar rendering it is the whole point) and
/// [`TITLE_HOME_DIR`] must contain [`TITLE_OCR_ABSENT`] (the assertion is that
/// the home segment is abbreviated to `~` and therefore NOT drawn). This is the
/// same shape as [`tree_sidebar_shows_files_under_cage`]'s exemption from the
/// filler guard. Rather than leave that as prose, the third block below asserts
/// the exemption's PRECONDITIONS — including the crossed pair, where a mistake
/// really would be fatal.
///
/// ## The one fixture whose CONTENT is checked too
///
/// Every other relaunch fixture is a bare heading, so its name is the only thing
/// that can collide with an assertion. [`DIVIDER_FILE_RIGHT`] also carries the body
/// [`divider_right_fixture_body`] builds — page density for a 15-cell pane, see
/// [`DIVIDER_BODY_LINE_COUNT`] — and every one of those lines renders in the same
/// pane as the heading the divider relaunch smoke asserts on. A body line carrying
/// an asserted term would satisfy that assertion with the heading never drawn,
/// which is the [`PICKER_FILE_NAME`] failure reached through CONTENT instead of
/// through a name. The last block below checks it here rather than from a guard of
/// its own so the smoke count this file's docs pin does not move.
///
/// PURE, like its two siblings: no compositor, no `require_smoke_env`, no
/// `CAGE_LOCK`, no subprocess.
#[test]
fn restore_fixture_names_cannot_satisfy_their_own_assertions() {
    // Guard the guard: either empty table would make the loops below run zero times
    // and pass for the wrong reason.
    assert!(
        !ASSERTED_OCR_TERMS.is_empty(),
        "`ASSERTED_OCR_TERMS` is empty, so this guard checks nothing at all."
    );
    assert!(
        !RELAUNCH_FIXTURE_NAMES.is_empty(),
        "`RELAUNCH_FIXTURE_NAMES` is empty, so the smoke-owned fixture files are unguarded — \
         which is the exact gap that let `picker_opens_and_loads_selected_file_under_cage` pass \
         green while the picker never opened."
    );

    for &(name, name_const, owner) in RELAUNCH_FIXTURE_NAMES {
        let lowercased = name.to_lowercase();
        for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
            assert!(
                !lowercased.contains(term),
                "fixture {name_const} = {name:?} (created by `{owner}`) contains {term_const} = \
                 {term:?}. The tree sidebar renders every cwd entry BY NAME, so that row would \
                 satisfy — or, for a must-NOT-contain term, spuriously break — `{smoke}`'s OCR \
                 assertion with the editor pane drawing nothing at all. Follow the convention the \
                 sibling smokes already use: put the asserted word in the file's CONTENT and give \
                 the file a name whose lowercase does not contain it. Do NOT change the OCR const \
                 and do NOT touch the smoke's assertion."
            );
        }
    }

    // Distinct names, and distinct from the filler files: these all land in ONE temp
    // cwd per smoke, so a collision would silently overwrite a fixture with another
    // one's contents.
    let mut seen = std::collections::HashSet::new();
    for &(name, name_const, owner) in RELAUNCH_FIXTURE_NAMES {
        assert!(
            seen.insert(name),
            "fixture {name_const} = {name:?} (owned by `{owner}`) is listed twice in \
             `RELAUNCH_FIXTURE_NAMES`; two fixtures sharing a name overwrite each other on disk."
        );
        assert!(
            !SIDEBAR_FILLER_FILES.contains(&name),
            "fixture {name_const} = {name:?} (owned by `{owner}`) collides with a \
             `SIDEBAR_FILLER_FILES` entry. Both are written into the same temp cwd, so one would \
             clobber the other and the smoke would assert against the wrong contents."
        );
    }

    // The title smoke's exemption, with its preconditions made executable rather
    // than merely documented above.
    assert!(
        TITLE_PROJECT_DIR.to_lowercase().contains(TITLE_OCR_PRESENT),
        "TITLE_PROJECT_DIR = {TITLE_PROJECT_DIR:?} no longer contains TITLE_OCR_PRESENT = \
         {TITLE_OCR_PRESENT:?}. The header-bar title renders that directory's BASENAME, so if the \
         two ever stop agreeing the smoke asserts on a word nothing draws and fails with no \
         visible cause."
    );
    assert!(
        TITLE_HOME_DIR.to_lowercase().contains(TITLE_OCR_ABSENT),
        "TITLE_HOME_DIR = {TITLE_HOME_DIR:?} no longer contains TITLE_OCR_ABSENT = \
         {TITLE_OCR_ABSENT:?}. The must-NOT-contain assertion is what proves the home prefix was \
         ABBREVIATED to `~`; if the home directory is not named after that term, the assertion \
         holds trivially and proves nothing — a vacuous test, not a passing one."
    );
    assert!(
        !TITLE_PROJECT_DIR.to_lowercase().contains(TITLE_OCR_ABSENT),
        "TITLE_PROJECT_DIR = {TITLE_PROJECT_DIR:?} contains TITLE_OCR_ABSENT = \
         {TITLE_OCR_ABSENT:?}. The project basename IS rendered in the title, so the \
         must-NOT-contain assertion would fire on a perfectly correct abbreviation."
    );
    assert!(
        !TITLE_HOME_DIR.to_lowercase().contains(TITLE_OCR_PRESENT),
        "TITLE_HOME_DIR = {TITLE_HOME_DIR:?} contains TITLE_OCR_PRESENT = {TITLE_OCR_PRESENT:?}. \
         If the home segment ever did get rendered it would then satisfy the must-CONTAIN \
         assertion on its own, and a broken abbreviation would look like a pass."
    );

    // The one relaunch fixture whose CONTENT is more than a heading, checked here
    // rather than in a guard of its own so the smoke count this file's docs pin
    // stays put. Its lines are editor text in the very pane whose heading
    // `dragged_divider_weights_survive_relaunch_under_cage` asserts on, so a body
    // line carrying an asserted term would satisfy that assertion with the heading
    // never drawn — the `PICKER_FILE_NAME` failure reached through content instead
    // of through a name. Both directions matter here exactly as they do above.
    let fixture = divider_right_fixture_body();
    let lines: Vec<&str> = fixture.lines().collect();
    assert_eq!(
        lines.len(),
        DIVIDER_BODY_LINE_COUNT + 1,
        "`divider_right_fixture_body` produced {} line(s), expected the heading plus \
         DIVIDER_BODY_LINE_COUNT = {DIVIDER_BODY_LINE_COUNT} body lines. The body's whole job is \
         to fill the narrow pane past the measured winheight(0) of 14 so no `~` end-of-buffer \
         column is drawn; a short body puts that column back and tesseract misreads the heading \
         above it (see DIVIDER_BODY_LINE_COUNT for the ablations that isolated it).",
        lines.len()
    );
    assert_eq!(
        lines.first().copied(),
        Some(DIVIDER_HEADING_RIGHT),
        "`divider_right_fixture_body`'s FIRST line must be DIVIDER_HEADING_RIGHT = \
         {DIVIDER_HEADING_RIGHT:?} — that is the line the smoke's OCR assertion reads, and a body \
         line ahead of it would push it down the pane for no stated reason."
    );
    for (index, line) in lines.iter().enumerate().skip(1) {
        let lowercased = line.to_lowercase();
        for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
            assert!(
                !lowercased.contains(term),
                "body line {index} of `divider_right_fixture_body` ({line:?}) contains \
                 {term_const} = {term:?}. That line renders in the SAME pane as \
                 DIVIDER_HEADING_RIGHT, so it would satisfy — or, for a must-NOT-contain term, \
                 spuriously break — `{smoke}`'s OCR assertion with the heading itself unrendered: \
                 the silent false positive that let \
                 `picker_opens_and_loads_selected_file_under_cage` pass while the picker never \
                 opened. The body is PAGE DENSITY and nothing else, so keep it to digits and `x`; \
                 do NOT change the OCR const and do NOT touch the smoke's assertion."
            );
        }
        assert!(
            line.chars().all(|c| c.is_ascii_digit() || c == 'x'),
            "body line {index} of `divider_right_fixture_body` is {line:?}, which is not digits \
             and `x` alone. The loop above only rejects the terms asserted on TODAY; keeping the \
             body wordless is what keeps it safe against a term added tomorrow by a smoke that \
             never looks at this file."
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

    // A fresh temp cwd, exactly like every other smoke. The app reads
    // `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with this
    // directory as its working directory (see `cage_command` below). See
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
    let status = cage_command(SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("OCR_WORD", OCR_WORD)
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
         concluding anything, and check these FOUR signatures in this order.\n\
         \n\
         (a) THE MOST LIKELY ONE, and the one to rule out FIRST: the whole page drew correctly \
         and looks exactly like a PASSING capture except that row 0 is BLANK — the buffer holds \
         exactly TWO empty lines with the block cursor on line 2, i.e. ONE row below the empty \
         row 0. That means the ENTIRE first `wtype -- \"i{OCR_WORD}\"` AND its `Escape` were \
         delivered before the client could accept keys, and were LOST; the trailing `o` + \
         `Escape` then produced the second line on their own. This is a TIMING / MARGIN failure, \
         NOT a rendering failure and NOT the swap modal. It was measured directly: capture md5 \
         `c9a797d31d606335297b40823e64371a`, byte-identical to a deliberately induced \
         cold-shader-cache run at `SETTLE=0.25`. Look at `SETTLE` (the ONLY real budget before \
         the first keystroke — the `wtype` poll ahead of it gates nothing, see the module doc \
         comment) and at `XDG_CACHE_HOME` in `cage_command` (a cold mesa shader cache costs ~4.9 \
         MB of rebuild inside that budget). Here, unlike (c), more settle IS the fix.\n\
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
         distinguishes them. The KNOWN cause of (c) is the swap modal, and it is where to look \
         first: `nvim_buf_set_name` creates the startup buffer's swap file inside the rename, and \
         a pre-existing swap for `<cwd>/Untitled.md` raises a blocking `do_dialog` ATTENTION/E325 \
         prompt (`mode() == \"r?\"`) that answers itself with your first key. Check \
         `state_dir()/swap/<hash-of-cwd>/` for a `%…%Untitled.md.swp` — since the \
         `WOLF_STATE_DIR` isolation landed that is this run's `-state` sibling of the temp cwd \
         (see `smoke_state_dir`), NOT `~/.local/state/wolf/swap/` — and see \
         `crates/wolf-core/tests/swap_recovery.rs`'s \
         `stale_untitled_swap_does_not_block_a_fresh_start`, which pins this headlessly \
         (and deterministically). Only if there is no such swap should you consider focus / \
         input routing (a Phase 2 pass chased that hypothesis and DISPROVED it — deferring \
         `grab_focus()` into `window.connect_map()` changes nothing). For (c) specifically it is \
         NOT a rendering failure and no amount of extra settle time will fix it — that is exactly \
         what separates it from (a).\n\
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
    // Serialize against the other cage test (shared Wayland socket); recover from
    // poisoning so a failure in the sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop the known markdown target into it. The app
    // reads `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with
    // this directory as its working directory (see `cage_command` below).
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
    let status = cage_command(PICKER_SCENARIO, &cwd)
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
    // with this directory as its working directory (see `cage_command` below).
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
    let status = cage_command(PICKER_NAV_SCENARIO, &cwd)
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
    // launch cage with this directory as its working directory (see `cage_command`
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
    let status = cage_command(TREE_SCENARIO, &cwd)
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
    // directory as its working directory (see `cage_command` below); the scenario
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
    // `cage_command` supplies WOLF_FONT_PT (and WOLF_STATE_DIR). See the "Note (OCR
    // width)" paragraph above the VSPLIT_* consts: if two side-by-side panes are ever
    // too narrow for 28pt OCR, widen the window or lower the font for THIS test —
    // never for all twenty by editing the helper.
    let status = cage_command(VSPLIT_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("VSPLIT_FILE_LEFT", VSPLIT_FILE_LEFT)
        .env("VSPLIT_FILE_RIGHT", VSPLIT_FILE_RIGHT)
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
    // directory as its working directory (see `cage_command` below); the scenario
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
    // `cage_command` supplies WOLF_FONT_PT (and WOLF_STATE_DIR). See the "Note (OCR
    // height)" paragraph above the HSPLIT_* consts: if two stacked panes are ever too
    // short for 28pt OCR, make the window taller or lower the font for THIS test —
    // never for all twenty by editing the helper.
    let status = cage_command(HSPLIT_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("HSPLIT_FILE_TOP", HSPLIT_FILE_TOP)
        .env("HSPLIT_FILE_BOTTOM", HSPLIT_FILE_BOTTOM)
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
    // directory as its working directory (see `cage_command` below).
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
    let status = cage_command(FOCUS_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("FOCUS_FILE_LEFT", FOCUS_FILE_LEFT)
        .env("FOCUS_FILE_RIGHT", FOCUS_FILE_RIGHT)
        .env("FOCUS_MARKER", FOCUS_MARKER)
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
    // directory as its working directory (see `cage_command` below).
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
    let status = cage_command(CLOSE_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("CLOSE_FILE_KEEP", CLOSE_FILE_KEEP)
        .env("CLOSE_FILE_GONE", CLOSE_FILE_GONE)
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
    // this directory as its working directory (see `cage_command` below).
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
    // `cage_command` supplies WOLF_FONT_PT (and WOLF_STATE_DIR). See the "Note (OCR
    // width)" paragraph above the TRI_* consts: if three side-by-side panes are ever
    // too narrow for 28pt OCR, widen the window or lower the font for THIS test —
    // never for all twenty by editing the helper.
    let status = cage_command(TRISPLIT_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("TRI_FILE_A", TRI_FILE_A)
        .env("TRI_FILE_B", TRI_FILE_B)
        .env("TRI_FILE_C", TRI_FILE_C)
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
    // keeps every remaining $HOME-derived path inside the temp dir too. (The state
    // dir is no longer one of them: `cage_command` pins it with `WOLF_STATE_DIR`,
    // which `state_dir()` resolves ahead of anything $HOME-derived, so the MRU sqlite
    // and the swap dir land in this cwd's `-state` sibling.)
    //
    // This `.env("HOME", ...)` DELIBERATELY OVERRIDES the default `cage_command` now
    // sets (the cwd's PARENT, which would leave this smoke in the `Folder` branch and
    // defeat it entirely). It is chained AFTER the helper, so it wins — `Command::env`
    // is last-write-wins per key. The determinism property the default exists for is
    // preserved: with HOME == cwd, `title::display_title` renders the constant `~`,
    // so this smoke's page is still byte-identical across runs.
    let status = cage_command(EMPTY_STATE_SCENARIO, &cwd)
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
    // directory as its working directory (see `cage_command` below).
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
    let status = cage_command(NEWBUF_SCENARIO, &cwd)
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
/// The query is chosen so the list holds EXACTLY ONE row — see the `PALETTE_*` const
/// block for the subsequence walk over all eight actionable titles. That premise is
/// not asserted here (a screenshot cannot count palette rows: the overlay is closed
/// before `grim` fires). It is measured in the fast suite by
/// `palette::tests::palette_rows_query_new_selects_exactly_the_new_buffer_row`, which
/// also pins that `PALETTE_MRU_KEY` still names that row. A registry change that adds
/// a second matching row therefore turns that unit test red rather than making this
/// smoke stochastic.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator
/// this smoke does not have. The `measured` column is the coder's to fill from an
/// actual red run, per `tests/README.md`'s "A red-run demonstration is a deliverable,
/// not a formality".
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

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // Create a fresh temp cwd and drop the base markdown file into it. The app reads
    // `std::env::current_dir()` for `CoreConfig.cwd`, so we launch cage with this
    // directory as its working directory (see `cage_command` below).
    let cwd = make_temp_cwd();
    let target = cwd.join(PALETTE_FILE);
    std::fs::write(&target, format!("{PALETTE_HEADING}\n")).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to write base file {}: {e}",
            target.display()
        )
    });

    // Page density. The asserted marker is a short isolated word on an otherwise thin
    // editor page, which is exactly the `SIDEBAR_FILLER_COUNT` hazard: below the
    // measured five-file floor tesseract misclassifies isolated glyphs. The fillers
    // cannot perturb the palette's candidate list, which comes from `COMMAND_SPECS`
    // and not from a file search.
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

    // The state dir `cage_command` hands the app as WOLF_STATE_DIR — the same one
    // assertion 5 reads the command MRU back out of. No HOME override: this smoke
    // wants the ordinary `Folder` launch decision and the constant `~/project` title.
    let smoke_state = smoke_state_dir(&cwd);

    let status = cage_command(PALETTE_SCENARIO, &cwd)
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
         the app died before its window ever mapped — a LAUNCH failure, and nothing below is \
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

    // --- Assertion 4 (precondition): the marker reached a buffer at all -------
    //
    // Assertion 3 above reads as "the editor is showing a buffer, and it is NOT the
    // base file". Only this assertion carries the first half: an editor drawing no
    // text at all — a dead session, a blank page, an unreadable capture — satisfies
    // assertion 3 vacuously. It does NOT discriminate the palette's focus return:
    // deleting `PaletteInner::close`'s `area.grab_focus()` was measured GREEN here,
    // so read the revert table above before blaming focus for a failure.
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

    // --- Assertion 5 (discriminator): activation recorded the row's MRU key ---
    //
    // Independent of the pixels: nothing but a palette activation writes this store
    // (`WolfCore::command_mru`'s doc states it), so a hit can only mean the row was
    // activated AND that its opaque key is the canonical registry id the palette's
    // ranking is keyed on.
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
/// [`stalled_nvim_raises_the_stall_overlay_under_cage`] possible at all.
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
/// A precondition gets NO revert: inventing one would manufacture a discriminator
/// this smoke does not have. The `measured` column is the coder's to fill from an
/// actual red run, per `tests/README.md`'s "A red-run demonstration is a
/// deliverable, not a formality".
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
    // The captive Neovim always holds its startup buffer plus the three fixtures,
    // so four is the expected count and three is the floor below which the
    // ordering assertions stop being able to say anything.
    const MIN_LISTED_BUFFERS: usize = 3;
    // The first index at which the target may sit. Row 0 is the buffer the user is
    // in and row 1 is the one `initial_selection` preselects, so anything below
    // this makes the query redundant and assertion 4 vacuous.
    const MIN_TARGET_INDEX: usize = 2;

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd holding the three fixtures. The app reads
    // `std::env::current_dir()` for `CoreConfig.cwd`, so cage is launched with this
    // directory as its working directory (see `cage_command`).
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

    // Page density. The asserted heading is a short line on an otherwise thin
    // editor page, which is exactly the `SIDEBAR_FILLER_COUNT` hazard: below the
    // measured five-file floor tesseract misclassifies isolated glyphs. The fillers
    // are never opened, so they never join the picker's candidate list — that list
    // comes from `getbufinfo`, not from a file search.
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
            panic!("{SMOKE}: failed to write filler {}: {e}", filler.display())
        });
    }

    // The premise readback lands in the cwd (the sidebar lists it, which is why its
    // name is registered in `RELAUNCH_FIXTURE_NAMES`). Cleared first: evidence that
    // could be a previous run's leftover is not evidence.
    let readback = cwd.join(BUFPICK_READBACK_NAME);
    clear_evidence(&readback, SMOKE);

    let out_png = runtime_dir.join("wolf_gnome_bufpick_smoke.png");
    clear_evidence(&out_png, SMOKE);

    let status = cage_command(BUFPICK_SCENARIO, &cwd)
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
         Exit 11: the app died before its window ever mapped — a LAUNCH failure, and nothing \
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

    // --- Assertion 3 (precondition): the MRU ORDER the whole smoke rests on ---
    //
    // Re-derived from Neovim's own `getbufinfo()` with the production rule
    // (`lastused` DESC, ties by `bufnr` ASC), never assumed from the order the
    // scenario typed its `:e`s in.
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

    // --- Assertion 5 (discriminator, no isolated red run): WHICH buffer -------
    //
    // With one pane two headings cannot both be on screen, so nothing reddens this
    // alone: its job is to name the wrong buffer when the query is ignored, not to
    // detect that the switch failed. See the revert table on this test.
    assert!(
        !ocr_text.contains(BUFPICK_OCR_ABSENT),
        "{SMOKE}: OCR unexpectedly contained {BUFPICK_OCR_ABSENT:?} — the heading of \
         {BUFPICK_FILE_PRESELECT}, the row `buffer_picker::initial_selection` preselects. The \
         picker therefore activated its PRESELECTED row rather than the row the query selected, \
         which means the typed text never reached the ranking: check that `refresh` ranks against \
         the SearchEntry's text rather than an empty query, and that `app::wire_keys` suppresses \
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
/// class `crates/wolf-gnome/DECISIONS.md` ("The pure seams") records as having
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
/// A precondition gets NO revert: inventing one would manufacture a discriminator
/// this smoke does not have. The `measured` column is the coder's to fill from an
/// actual red run, per `tests/README.md`'s "A red-run demonstration is a deliverable,
/// not a formality".
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
    // The captive Neovim always holds its startup buffer plus the three fixtures, so
    // four is the expected count and three is the floor below which the ordering
    // assertions stop being able to say anything.
    const MIN_LISTED_BUFFERS: usize = 3;
    // The first index at which the target may sit. Row 0 is the buffer the user is
    // in and row 1 is the one `initial_selection` preselects, so a target above this
    // would be the row Enter is EXPECTED to land on and assertion 5 would contradict
    // assertion 3.
    const MIN_TARGET_INDEX: usize = 2;

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd holding the same three fixtures the query smoke uses. The app
    // reads `std::env::current_dir()` for `CoreConfig.cwd`, so cage is launched with
    // this directory as its working directory (see `cage_command`).
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

    // Page density. The asserted heading is a short line on an otherwise thin editor
    // page, which is exactly the `SIDEBAR_FILLER_COUNT` hazard: below the measured
    // five-file floor tesseract misclassifies isolated glyphs. The fillers are never
    // opened, so they never join the picker's candidate list — that list comes from
    // `getbufinfo`, not from a file search — and therefore cannot shift which row
    // `initial_selection` picks.
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
            panic!("{SMOKE}: failed to write filler {}: {e}", filler.display())
        });
    }

    // The premise readback lands in the cwd (the sidebar lists it, which is why its
    // name is registered in `RELAUNCH_FIXTURE_NAMES`). Cleared first: evidence that
    // could be a previous run's leftover is not evidence.
    let readback = cwd.join(BUFPICK_READBACK_NAME);
    clear_evidence(&readback, SMOKE);

    let out_png = runtime_dir.join("wolf_gnome_bufpick_bare_smoke.png");
    clear_evidence(&out_png, SMOKE);

    let status = cage_command(BUFPICK_BARE_SCENARIO, &cwd)
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
         Exit 11: the app died before its window ever mapped — a LAUNCH failure, and nothing \
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

    // --- Assertion 2 (precondition): the MRU ORDER the preselection lands in ---
    //
    // Re-derived from Neovim's own `getbufinfo()` with the production rule
    // (`lastused` DESC, ties by `bufnr` ASC), never assumed from the order the
    // scenario typed its `:e`s in. Index 1 is the load-bearing clause here: it is
    // the row `initial_selection` chooses, so it decides which file assertion 3 is
    // entitled to name.
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
    // Implied by the two clauses above as they stand today — index 0 is CURRENT and
    // index 1 is PRESELECT, and the three names are distinct — so it is stated as the
    // DIRECT premise of assertion 5 rather than as an independent check. If either
    // clause above is ever relaxed (a fourth fixture, a startup buffer that joins the
    // list differently), this is what still keeps assertion 5's must-be-absent
    // heading off the row a bare Enter lands on.
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

    // --- Assertion 5 (discriminator): the preselection was not TOO DEEP -------
    //
    // Not reddened by the revert named for 3 and 4; what it discriminates is the
    // opposite error, a preselection past row 1. See the revert table on this test.
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
    // directory as its working directory (see `cage_command` below).
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
    let status = cage_command(COLON_Q_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("QUIT_FILE_KEEP", QUIT_FILE_KEEP)
        .env("QUIT_FILE_GONE", QUIT_FILE_GONE)
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
    let status = cage_command(COLON_QA_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
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
    // directory as its working directory (see `cage_command` below).
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
    let status = cage_command(CHORD_CLOSE_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("CHORD_CLOSE_FILE_KEEP", CHORD_CLOSE_FILE_KEEP)
        .env("CHORD_CLOSE_FILE_GONE", CHORD_CLOSE_FILE_GONE)
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
/// between. That is the adversarial half of the wake-coalescing argument this unit
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
/// term. MEASURED in pass 5 by running the revert: with `reopen_order` pushing
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
/// ## Red-run reverts (FIVE reverts, six rows — run EVERY revert; pass 5 records the
/// measured outcome)
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

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd, exactly like every other smoke; see `make_temp_cwd` for why
    // isolation is mandatory. The readback files live here too.
    let cwd = make_temp_cwd();
    let fixture = cwd.join(RESTART_FILE_NAME);
    std::fs::write(&fixture, restart_fixture_body())
        .unwrap_or_else(|e| panic!("failed to write the fixture {}: {e}", fixture.display()));

    // Give the tree sidebar rows of text to draw. The asserted heading is sparse,
    // light-on-dark editor text, which is exactly the case `SIDEBAR_FILLER_COUNT`
    // documents: below the measured five-file floor tesseract misclassifies isolated
    // glyphs on a nearly-blank page. Fixture only — nothing here asserts on them.
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY)
            .unwrap_or_else(|e| panic!("failed to write filler {}: {e}", filler.display()));
    }

    // Readback files. `preserve_state.txt` is the observable that proves `:preserve`
    // ran before the kill; like every other name in this cwd it is checked against the
    // asserted OCR terms, because the tree sidebar renders it (it contains neither
    // `unsavedburst` nor `bounceback`, `restarted` or `crashed`).
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

    let status = cage_command(&with_nvim_pid_helper(CRASH_SCENARIO), &cwd)
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
         Exit 11: the app died before its window ever mapped — a LAUNCH failure, nothing to do \
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
         containing a SPACE would also land here, which no fixture in this file has.",
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

    // --- The wrap derivation on `RESTART_UNSAVED_MARKER`, made EXECUTABLE ---
    //
    // The unsaved edit is APPENDED to line 2 — the first filler line — so the
    // recovered line 2 is that filler line followed by the marker. `opt.wrap` +
    // `opt.linebreak` would split it across screen rows if it were wider than the
    // window, and a split could fall inside the asserted word. Rather than trusting
    // the measured `winwidth(0)` of 45, assert the composed width against the width
    // Neovim actually reported. The line is composed from the SAME consts
    // `restart_fixture_body` writes it from, so the two cannot drift.
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

    // --- `:preserve` really ran, on a dirty buffer that really has a swap ----
    //
    // Without this the "the unsaved edit survived" assertion below would be a race
    // against Neovim's default 'updatecount' (200 typed characters between swap
    // syncs) rather than a property — see `CRASH_SCENARIO`'s "Why step 5 exists".
    // The scenario writes this file with a `:call writefile(...)` issued AFTER the
    // `:preserve`, and Neovim runs Ex commands in the order it receives them, so the
    // file's mere existence is the ordering proof; these two flags are the
    // preconditions that make the flush meaningful.
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

    // --- The NEGATIVE half of the control, and it is not optional -----------
    //
    // The control is captured at scenario step 3, BEFORE the unsaved edit is typed at
    // step 5, so the marker cannot legitimately be on this page. Without this half the
    // post-recovery assertion would be satisfiable by a marker that had leaked into the
    // fixture body, a filler filename, a readback filename or the temp cwd path — the
    // same silent false positive documented on `PICKER_FILE_NAME`, where a sidebar row
    // satisfied a smoke whose feature provably never ran.
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
         was MEASURED in pass 5 to fail EARLIER, at the scenario's own recovery poll (exit 19), \
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

    // --- The UNSAVED EDIT survived the SIGKILL ------------------------------
    //
    // The one assertion here that discriminates a SWAP-RECOVERED reopen from a clean
    // re-read of the file on disk. `bounceback` is in the fixture on disk and renders
    // under both; this word was never written to disk at all, so it can only have come
    // back through the dead Neovim's swap file. It is deterministic rather than a race
    // because the scenario `:preserve`d the buffer and POLLED for proof before killing
    // — the two preconditions above are that proof.
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
/// discriminator this smoke does not have. The `measured` column is the coder's to
/// fill from an actual red run.
///
/// | # | assertion | kind | revert | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the scenario ran to completion | none; do not invent one. Exits 11 / 16 / 17 are documented in the failure message. | n/a |
/// | 2 | both PNGs present | precondition — grim produced two images | none; do not invent one | n/a |
/// | 3 | control OCR contains [`STALL_OCR_ABSENT`] | **precondition** — the fixture rendered at all | none. Without it assertion 6 is vacuous: an editor that drew nothing satisfies "the heading is absent" trivially, so the opacity claim would rest on nothing. | n/a |
/// | 4 | control OCR lacks [`STALL_OCR_PRESENT`] | **control**, not falsifiable by a source revert | none, and that is deliberate. It proves the overlay was raised BY THE CHORD rather than being up all along (a launch-time stall, a mis-classified `Alive`), which is a property of the run and not of a line anyone could revert. Do not invent one. | n/a |
/// | 5 | post OCR contains [`STALL_OCR_PRESENT`] | **discriminator** — a stall was detected and shown | in `buffer_picker.rs`'s `open`, replace the `core.list_buffers()` call with `Vec::new()`. No RPC is issued, nothing elapses, liveness stays `Alive`, and the capture shows the editor. This is the feature-disable, and it also reddens [`buffer_picker_chord_switches_to_the_queried_buffer_under_cage`] — expected, and stated. The tempting "isolated" alternative (`liveness_ui`'s `NotResponding` arm yielding `OverlayMode::Hidden`) is NOT isolated: it reddens `liveness_ui_table` and `not_responding_never_auto_restarts` in the fast suite. | RED. `after the wedge and the chord, OCR did not contain "responding" — the distinctive word of liveness_ui::STALLED_MESSAGE`. The capture was the healthy editor: `stallfixture` present, no overlay text. As stated, the same revert reddened [`buffer_picker_chord_switches_to_the_queried_buffer_under_cage`] too (`OCR did not contain "bufpickalfa"`). Restored, green. |
/// | 6 | post OCR lacks [`STALL_OCR_ABSENT`] | **discriminator** — the overlay is OPAQUE and full-bleed | in `crash_overlay::build`, delete `overlay.add_css_class("background")`. The overlay goes transparent and the frozen fixture shows through. Shared with [`crash_overlay_and_restart_button_recover_under_cage`]'s own opacity assertion — the two smokes share the widget, which is why this revert reddens both. | RED. `the post-stall capture still contained "stallfixture" — the heading of stfrozen.md, which was on screen when Neovim was wedged`, while the same OCR dump also carried `neovim stopped responding`: the overlay was up and see-through, which is exactly what this row claims to catch. The sibling smoke went red too, but on its must-CONTAIN assertion (`the dwell capture did not contain "crashed"`) rather than on its opacity assertion; its dwell capture held the pre-crash editor body and none of the overlay's words. Restored, green. |
/// | 7 | (in-scenario) `assert_app_alive` | discriminator, exit 16 | none needed as a Rust assertion; the exit code IS the observable, and it is the worst failure this feature has: the app dying alongside its wedged Neovim. | n/a |
#[test]
fn stalled_nvim_raises_the_stall_overlay_under_cage() {
    const SMOKE: &str = "stalled_nvim_raises_the_stall_overlay_under_cage";

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

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

    // `WOLF_RPC_TIMEOUT_MS` is read once at `WolfCore::start` from the process
    // environment, so setting it here reaches the app through `cage -- sh -c`. It
    // is a timing knob only; see `STALL_RPC_TIMEOUT_MS` for the two bounds it sits
    // between and for why shortening it is safe in this scenario.
    let status = cage_command(&with_nvim_pid_helper(STALL_SCENARIO), &cwd)
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
         Exit 11: the app died before its window ever mapped — a LAUNCH failure, nothing to do \
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

    // --- Assertion 3 (precondition): the fixture RENDERED while healthy -------
    //
    // Without this, assertion 6 is vacuous: "the heading is absent" is satisfied by
    // an editor that drew nothing at all, by a window that never mapped, and by an
    // unreadable capture just as well as by an opaque overlay.
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

    // --- Assertion 4 (control): the overlay was NOT already up ----------------
    //
    // Not falsifiable by a source revert, and deliberately so: it says the overlay
    // seen after the chord was raised BY THE CHORD rather than having been up since
    // launch. Do not invent a revert for it.
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

    // --- Assertion 6 (discriminator): the overlay is OPAQUE -------------------
    //
    // The frozen last frame is still behind it — `liveness_ui` sets `paint: false`,
    // so nothing repaints, and the fixture's heading is exactly where it was. Only
    // an opaque, full-bleed overlay hides it.
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
/// revert rows below, which is why pass 5 must run EVERY one of them rather than
/// sampling.
///
/// * `crashed` PRESENT during the dwell — the overlay is actually shown. Stated
///   honestly, because pass 5 measured it: NO revert exercises this assertion in
///   isolation. B1 (never `set_visible(true)`) exits 19 at the recovery poll before
///   the dwell OCR is read, and B2 fails it for the OPACITY defect. It is a real
///   assertion with no isolated red run of its own.
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
/// extents; minus the pane origin (which IS the overlay origin — the overlay's main
/// child is the pane container whose single `DrawingArea` sits at offset `(0, 0)`,
/// and under cage the window is the output with zero CSD offset); into
/// `restart_button_rect`; then a tenth of the way in from that rect's LEFT edge
/// horizontally, and the rect's CENTRE vertically. That asymmetry is not stylistic —
/// a centre-derived x was measured in pass 5 to be blind to the button-width revert.
/// The derivation comment in the body carries the measurement, and states the width
/// bound the inset does and does not discriminate.
///
/// Three self-checking preconditions guard it:
/// [`assert_button_rect_is_the_named_size`] (a collapsed `(0,0,0,0)` must
/// not silently retarget the click to the overlay's top-left),
/// [`assert_button_rect_fits`] (a `PANE_ORIGIN_TOLERANCE_PX` analogue), and
/// [`assert_inside_extents`]. After the run the whole derivation is REDONE from the
/// run's own measured extents and its own derived origin, and the injected point is
/// asserted to lie inside that rect.
///
/// ## Measured geometry (2026-08-02)
///
/// | quantity | value |
/// | --- | --- |
/// | cage output size | `1280x720` |
/// | overlay origin | `(251, 46)` |
/// | `restart_button_rect(1029.0, 674.0)` | `(354.5, 345.0, 320.0, 64.0)` |
/// | button, absolute | `x 605.5..925.5`, `y 391..455` |
/// | rendered button, inspected | `x 605..925`, `y 392..456` |
///
/// Those are MEASURED values, and the pre-run derivation does NOT use them: it is
/// anchored on the pinned `PANE_ORIGIN_X_EXPECTED` / `PANE_ORIGIN_Y_EXPECTED`
/// (257.9, 52.0), which sit a few px inside the origin these runs rendered at. The
/// point it actually injects is therefore `(641, 426)` — the value the pass-5 red
/// runs report verbatim — and not what this table on its own would imply. The gap is
/// expected and small against the click's margins; the post-run assertion redoes the
/// whole derivation from the run's OWN extents and origin and proves the injected
/// point landed inside the real rect.
///
/// ## Red-run reverts (five; run EVERY one — pass 5 records the measured outcome)
///
/// | assertion | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | `crashed` present | make `CrashOverlay::set_mode` never call `set_visible(true)` | `crashed` absent and `bounceback` PRESENT in the dwell capture (the frozen frame shows through) | red, but NOT by the predicted route: measured `exit status: 19`, scenario stderr `the click at (641, 426) drove no recovery`. The restart button lives INSIDE the overlay, so a never-shown overlay is also a never-shown button and the run exits at the recovery poll before the dwell OCR is read. Restored, green |
/// | `bounceback` absent (opacity) | remove `add_css_class("background")` from the overlay box | BOTH terms present in the dwell capture | red on the FIRST dwell assertion instead: `the dwell capture did not contain "crashed"`, while the same OCR dump carries `# bounceback` — the frozen frame does show through exactly as predicted, and the fully transparent box additionally leaves its own label unreadable against the editor text. Restored, green |
/// | the attempt cap | set `MAX_AUTO_RESTART_ATTEMPTS = u32::MAX` | scenario exit **20**: the app spins respawning against the refusing wrapper. Worth capturing as evidence in its own right — it demonstrates the infinite-retry hazard the cap exists to prevent | **DID NOT GO RED as first written — the smoke stayed fully GREEN with the cap removed.** Cause: `assert_no_nvim_respawn` counted children named `nvim`, and a refused wrapper exits BEFORE its `exec`, so an unbounded retry loop is invisible to it and exit 20 was unreachable. FIXED in pass 5 by having the wrapper log each refusal (see `RESTART_ATTEMPTS_NAME` / `assert_one_restart_attempt`); the same revert then reproduced `exit status: 20` with `the app made 75 automatic respawn attempt(s)` against the capped build's 1. Restored, green |
/// | the button | make `CrashOverlay::connect_restart` drop its callback | scenario exit **19**: the click does nothing | reproduced exactly (`exit status: 19`, scenario stderr `the click at (641, 426) drove no recovery`), restored, green |
/// | the button geometry | change `RESTART_BUTTON_WIDTH` to `40` **in the widget builder only** | the derived point misses the button; scenario exit **19** | **DID NOT GO RED as first written — the smoke stayed fully GREEN.** Cause: the click was derived at the rect's CENTRE, and a `halign(Center)` button is centred on that same point whatever width it requests. Measured by scanning the dwell capture: correct builder renders x 605..924 (320px), the 40px builder renders x 669..861 (193px, the label's natural width) — both centred on x 765, and even the 3/4 point (x 845) is still inside the narrow one. FIXED in pass 5 by aiming a tenth in from the rect's LEFT edge (x 641); the same revert then reproduced `exit status: 19` with `the click at (641, 426) drove no recovery`. Restored, green |
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

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // The REAL Neovim, read from THIS process's environment (which `require_smoke_env`
    // has already proved is set) and baked into the wrapper as a literal. Nothing
    // below relies on the wrapper inheriting anything.
    let real_nvim = std::env::var("WOLF_NVIM_BIN")
        .expect("require_smoke_env proved WOLF_NVIM_BIN is set and non-empty");

    let cwd = make_temp_cwd();
    let fixture = cwd.join(RESTART_FILE_NAME);
    std::fs::write(&fixture, restart_fixture_body())
        .unwrap_or_else(|e| panic!("failed to write the fixture {}: {e}", fixture.display()));

    // The failing-respawn wrapper. All three absolute paths are LITERALS: there is no
    // env inheritance to reason about, and `exec` replaces the shell in place so the
    // app's direct child is still named `nvim` with the same pid, leaving
    // `find_nvim_pid` unaffected.
    //
    // A REFUSED spawn appends one line to `$attempts` before exiting. That log is the
    // ONLY sound measure of how many automatic restarts the front-end attempted: a
    // refused wrapper exits before its `exec`, so it is never named `nvim` and a /proc
    // scan cannot see it at all. See `assert_one_restart_attempt` for the measured
    // proof that a scan-only check left `MAX_AUTO_RESTART_ATTEMPTS` untested.
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

    // --- Derive the button point (no magic pixel) --------------------------
    //
    // The pane origin IS the overlay origin, and under cage the window is the output,
    // so the overlay runs from that origin to the bottom-right corner. Both inputs
    // are the MEASURED constants, because the click point has to exist before `cage`
    // is spawned while the authoritative extents only arrive afterwards on the
    // helper's `ready` line — which is why the run asserts, below, that the reported
    // extents really are the ones this derivation used, and then redoes the whole
    // derivation from its own measurements.
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "the expected cage output width");
    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "the expected cage output height");
    let overlay_w = output_w - PANE_ORIGIN_X_EXPECTED;
    let overlay_h = output_h - PANE_ORIGIN_Y_EXPECTED;
    let rect = wolf_gnome::restart_button_rect(overlay_w, overlay_h);
    assert_button_rect_is_the_named_size(SMOKE, rect);
    assert_button_rect_fits(SMOKE, rect, (overlay_w, overlay_h));
    let (bx, by, bw, bh) = rect;
    // X is a tenth of the way IN FROM THE RECT'S LEFT EDGE — NOT the rect's centre —
    // and that is load-bearing.
    //
    // `gtk::Widget::set_size_request` sets a MINIMUM, and the button is
    // `halign(Center)`, so a builder that requests too little still renders a
    // natural-width pill centred on exactly the same point the rect's centre
    // describes. A centre-derived click is therefore blind to the very drift this
    // smoke exists to catch. MEASURED (pass 5), by scanning the dwell capture:
    //
    //   * builder requests `RESTART_BUTTON_WIDTH` (320) -> rendered x 605..924, 320px
    //   * builder requests 40                           -> rendered x 669..861, 193px
    //
    // Both are centred on x 765, so a centre click hits both — and with a centre
    // click this smoke stayed GREEN with the builder reverted to 40. Even the 3/4
    // point (x 845) still lands inside the 193px button. A tenth in from the left
    // edge puts the injected point at x 641 — the value pass 5's red runs record
    // verbatim as `the click at (641, 426)` — which against those measured spans is
    // 36px inside the CORRECT button and 28px clear of the natural-width one.
    //
    // Both margins absorb both sources of error. The rect derivation itself agreed
    // with the render to 1px once anchored on the run's own origin (pass 1: rendered
    // x 605..925 y 392..456 against a derived x 605.5..925.5, y 391..455); on top of
    // that, the PRE-RUN anchor `PANE_ORIGIN_X_EXPECTED` sits a few px right of the
    // origin those runs actually rendered at (see this smoke's measured-geometry
    // table), which is what shifts the injected point rightwards inside the rect.
    // Neither error is close to eating a 28px margin.
    //
    // What the left-edge inset discriminates, as a BOUND rather than as an anecdote:
    // the button is `halign(Center)`, so it renders across [765 - W/2, 765 + W/2]
    // where W = max(requested, natural ~193), and a click at x 641 misses it iff
    // W < 248. This point therefore catches any builder width that has shrunk below
    // ~248px: the documented 40px revert renders W 193, i.e. 55px inside that bound
    // (the 28px of pixel clearance quoted above). It would NOT catch a shrink from
    // 320 to, say, 260 — widths in (248, 320) are undiscriminated, and "it catches
    // the 40px revert" must not be read as more than that.
    //
    // Y stays at the rect's vertical CENTRE: the vertical derivation is the
    // non-trivial half (two `vexpand` spacers splitting the leftover), the centre is
    // where it is most robust, and no documented revert turns on the button's height —
    // which is the other axis this click leaves undiscriminated.
    const BUTTON_HIT_INSET_FRACTION: f64 = 0.1;
    let btn_x = pixel_target(
        PANE_ORIGIN_X_EXPECTED + bx + bw * BUTTON_HIT_INSET_FRACTION,
        "the restart button x",
    );
    let btn_y = pixel_target(
        PANE_ORIGIN_Y_EXPECTED + by + bh / 2.0,
        "the restart button y",
    );
    let btn_x_env = btn_x.to_string();
    let btn_y_env = btn_y.to_string();

    let body = with_nvim_pid_helper(POINTER_RESTART_BUTTON_BODY);
    let status = run_pointer_scenario(
        &body,
        &cwd,
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
    // The crash-specific exit codes get their own message FIRST; everything else —
    // the shared pointer-harness codes 11-15, a signal on cage itself, and success —
    // falls through to `assert_pointer_scenario_succeeded`, which also echoes
    // `wolf-pointer-inject`'s stderr.
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

    let measured_overlay_w = exact_f64(extents.0, "the reported cage output width") - origin_x;
    let measured_overlay_h = exact_f64(extents.1, "the reported cage output height") - origin_y;
    let measured_rect = wolf_gnome::restart_button_rect(measured_overlay_w, measured_overlay_h);
    assert_button_rect_is_the_named_size(SMOKE, measured_rect);
    let measured_overlay = (measured_overlay_w, measured_overlay_h);
    assert_button_rect_fits(SMOKE, measured_rect, measured_overlay);
    let (mbx, mby, mbw, mbh) = measured_rect;
    let hit_x = exact_f64(btn_x, "the injected button x");
    let hit_y = exact_f64(btn_y, "the injected button y");
    assert!(
        hit_x >= origin_x + mbx
            && hit_x <= origin_x + mbx + mbw
            && hit_y >= origin_y + mby
            && hit_y <= origin_y + mby + mbh,
        "{SMOKE}: the point actually injected, ({btn_x}, {btn_y}), lies OUTSIDE the button rect \
         this run's own measurements describe: origin ({origin_x}, {origin_y}) plus \
         `restart_button_rect({measured_overlay_w}, {measured_overlay_h})` = ({mbx}, {mby}, \
         {mbw}, {mbh}), i.e. absolute x {}..{}, y {}..{}. The pre-run derivation used the pinned \
         PANE_ORIGIN_X_EXPECTED / PANE_ORIGIN_Y_EXPECTED, and the derived origin carries the probe \
         click's sub-cell phase, so a small disagreement is expected and harmless — a disagreement \
         large enough to leave a {mbw}x{mbh} button is not. Re-measure the origin constants (the \
         band assertion above prints the derived value) rather than moving the click. PNG kept at \
         {}.",
        origin_x + mbx,
        origin_x + mbx + mbw,
        origin_y + mby,
        origin_y + mby + mbh,
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

/// Writes the [`RELAUNCH_FIXTURE_NAMES`] markdown fixtures a smoke owns into `cwd`,
/// plus the [`SIDEBAR_FILLER_FILES`] page-density fixture.
///
/// Each `contents` is a whole file, newline-terminated here rather than by the
/// caller. Most callers pass a bare heading; the divider smoke's right-hand fixture
/// passes a heading plus a body ([`divider_right_fixture_body`]).
///
/// The fillers are FIXTURE, not assertion: the asserted terms in both relaunch
/// smokes are sparse, light-on-dark EDITOR text, which is exactly the case
/// [`SIDEBAR_FILLER_COUNT`] documents — below the measured five-file floor
/// `tesseract` misclassifies isolated glyphs on a nearly-blank page. Nothing
/// asserts on a filler, and
/// [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`] is what keeps one
/// from satisfying an assertion by accident.
fn write_relaunch_fixtures(smoke: &str, cwd: &Path, files: &[(&str, &str)]) {
    for (name, contents) in files {
        let path = cwd.join(name);
        std::fs::write(&path, format!("{contents}\n"))
            .unwrap_or_else(|e| panic!("{smoke}: failed to write fixture {}: {e}", path.display()));
    }
    for name in SIDEBAR_FILLER_FILES {
        let filler = cwd.join(name);
        std::fs::write(&filler, SIDEBAR_FILLER_BODY).unwrap_or_else(|e| {
            panic!("{smoke}: failed to write filler {}: {e}", filler.display())
        });
    }
}

/// A fingerprint of a file that must NOT change across a smoke run: its length and
/// modification time, or `None` when it does not exist.
///
/// `None` is a first-class answer, not an error: on a machine that has never run
/// Wolf interactively — which includes CI and this host —
/// `~/.local/state/wolf/state.json` simply does not exist, and "absent before,
/// absent after" is exactly as strong an isolation proof as "unchanged". Taking
/// `metadata().unwrap()` for the BEFORE snapshot would panic there instead.
fn state_file_fingerprint(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

/// Proves the tab's PANE-SPLIT TREE survives a quit and relaunch: three panes in an
/// asymmetric layout, each holding its own file, rebuilt from `state.json`.
///
/// Two app launches run inside ONE `cage` instance (see [`RESTORE_LAYOUT_SCENARIO`]
/// and the shared exit-code table above [`RELAUNCH_RESTORE_SETTLE`]), so `CAGE_LOCK`
/// semantics are unchanged and no second compositor is started.
///
/// ## What it asserts
///
/// 1. **Load-bearing, and EXACT:** the restored Neovim's own
///    `tabpagebuflist()` reads exactly `rleft.md rmid.md rright.md`. That is one
///    comparison proving three things at once — the pane COUNT, the buffer
///    IDENTITIES, and their ORDER — because nvim numbers windows by layout
///    position.
/// 2. **Corroboration (the GTK half):** OCR carries `restorealfa` AND
///    `restorebravo` AND `restorecharlie`. Three distinct panes actually bound to
///    grids and PAINTED, which no `winnr('$')` readback can show.
///
/// ## How it discriminates RESTORATION from COINCIDENCE
///
/// A fresh launch of `wolf-gnome` shows exactly ONE pane holding `Untitled.md`: the
/// binary takes no CLI arguments, opens no file at startup, and has no MRU-reopen
/// path — the readback would be `Untitled.md` and the OCR would carry none of the
/// three terms. Neovim's own session behaviour cannot produce it either: the
/// captive nvim is spawned fresh with `-u init.lua`, there is no `:mksession`, and
/// nothing in `init.lua` restores windows. And the SHAPE is unreachable by
/// accident: a three-pane asymmetric split — a left column split horizontally
/// beside a full-height right pane — holding three DISTINCT files cannot arise from
/// any single-file reopen, any MRU, or any swap recovery, all of which would give
/// one pane and one name. Finally the three fixture names (`rleft.md`, `rmid.md`,
/// `rright.md`) contain none of the three asserted terms, so neither a sidebar row
/// nor an `:e` toast can satisfy one — enforced by
/// [`restore_fixture_names_cannot_satisfy_their_own_assertions`].
///
/// ## What it does NOT prove
///
/// **Nothing here exercises non-equal pane WEIGHTS.** This scenario never drags a
/// divider, so every weight it persists is an equal share, and "the saved weights
/// were applied to the dividers" and "the equal-share default was recomputed" agree
/// numerically throughout. It proves the TOPOLOGY round trip and nothing about the
/// weights; [`dragged_divider_weights_survive_relaunch_under_cage`] is the smoke
/// that drags one and asserts on the value that comes back.
///
/// ## Red-run revert (one, and one line)
///
/// | assertion | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | both | in `app.rs::build_ui`, delete `if let Some(layout) = plan.layout.clone() { panes.request_layout_rebuild(layout); }` — or, equivalently, make `pane_view::consume_pending_layout` `return false;` as its first statement | the readback reads `Untitled.md` instead of the three names, and the OCR carries none of `restorealfa` / `restorebravo` / `restorecharlie` | **AS PREDICTED.** Assertion 1 panicked first: `assertion 'left == right' failed … left: "Untitled.md" right: "rleft.md rmid.md rright.md"`. Because it fires first the OCR assertion is never reached, so the OCR half was confirmed out of band by running `tesseract --psm 11` over the retained PNG: zero hits for each of `restorealfa`, `restorebravo` and `restorecharlie`. Restoring the line returned the smoke to `1 passed`. |
#[test]
fn pane_layout_survives_relaunch_under_cage() {
    const SMOKE: &str = "pane_layout_survives_relaunch_under_cage";

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd, exactly like every other smoke; see `make_temp_cwd` for why
    // isolation is mandatory. BOTH launches use this same directory — relaunching
    // in the project you saved from is the whole scenario.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (RESTORE_FILE_LEFT, RESTORE_HEADING_LEFT),
            (RESTORE_FILE_MID, RESTORE_HEADING_MID),
            (RESTORE_FILE_RIGHT, RESTORE_HEADING_RIGHT),
        ],
    );

    // Start from a clean slate: evidence that could be a previous run's leftover is
    // not evidence. `clear_evidence` removes AND asserts absent.
    let readback = cwd.join(RESTORE_READBACK_NAME);
    clear_evidence(&readback, SMOKE);
    let out_png = runtime_dir.join("wolf_gnome_restore_layout_smoke.png");
    clear_evidence(&out_png, SMOKE);

    // `cage_command` supplies WOLF_FONT_PT and WOLF_STATE_DIR. The state dir is what
    // makes the two runs share a `state.json` at all — and what keeps them out of the
    // developer's real one.
    let status = cage_command(RESTORE_LAYOUT_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("RESTORE_FILE_LEFT", RESTORE_FILE_LEFT)
        .env("RESTORE_FILE_MID", RESTORE_FILE_MID)
        .env("RESTORE_FILE_RIGHT", RESTORE_FILE_RIGHT)
        .env("RESTORE_READBACK", &readback)
        .env("RESTORE_SETTLE", RELAUNCH_RESTORE_SETTLE)
        .status()
        .expect("failed to spawn `cage` for the headless pane-layout relaunch scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage relaunch scenario exited unsuccessfully ({status}). Exit 11: run 1's \
         app died before its window mapped. Exit 12: run 1 was still alive after `:qa` — the \
         drained close_window never reached window.close(). Exit 13: run 1 exited NON-ZERO, i.e. \
         it went away but not cleanly — after this unit that implicates a panic in \
         `flush_on_close`, or a core.borrow() held across `wire_close`'s core.borrow_mut().take(). \
         Exit 14: run 2's app died before its window mapped. Exit 15: the Neovim readback never \
         appeared in run 2. Screenshot expected at {}; readback at {}.",
        out_png.display(),
        readback.display()
    );

    assert_png_present(SMOKE, &out_png);

    // --- Assertion 1: the EXACT window list, from Neovim itself ---------------
    let got = read_trimmed(&readback, SMOKE);
    assert_eq!(
        got,
        RESTORE_READBACK_EXPECTED,
        "{SMOKE}: after the relaunch, the restored Neovim's tabpagebuflist() names \
         {got:?}, not {RESTORE_READBACK_EXPECTED:?}. `tabpagebuflist()` is in WINDOW-NUMBER order \
         (a row left-to-right, a column top-to-bottom), so this one comparison covers the pane \
         count, the buffer identities and their order. Read it this way: `Untitled.md` alone means \
         NOTHING was restored (the saved layout was never queued, or `consume_pending_layout` \
         never fired); the three right names in the WRONG ORDER means the DFS zip is inverted \
         somewhere between `rebuild_layout_spec`, `wolf_rebuild_layout` and `pane_layout::decode`; \
         and fewer than three names means the rebuild ran but produced the wrong topology. PNG \
         kept at {}.",
        out_png.display()
    );

    // --- Assertion 2: three panes really PAINTED ------------------------------
    let ocr_text = ocr_to_lowercase(&out_png);
    for (term, const_name, position) in [
        (RESTORE_OCR_ALFA, "RESTORE_OCR_ALFA", "top-left"),
        (RESTORE_OCR_BRAVO, "RESTORE_OCR_BRAVO", "bottom-left"),
        (
            RESTORE_OCR_CHARLIE,
            "RESTORE_OCR_CHARLIE",
            "full-height right",
        ),
    ] {
        assert!(
            ocr_text.contains(term),
            "{SMOKE}: OCR did not contain {const_name} = {term:?} (the {position} pane's file). \
             The readback assertion above passed, so NEOVIM has the three windows — this says the \
             GTK side did not bind or paint all three. Check that `apply_layout_rebuild` zipped \
             every returned handle onto a pane (`handles.len() != leaf_ids.len()` logs and \
             returns, leaving the single pre-existing pane), and that `rebuild_pane_area` + \
             `resolve_pending_grids` ran afterwards. PNG kept at {} for inspection. OCR output \
             was:\n{ocr_text}",
            out_png.display()
        );
    }
}

/// Proves the SINGLE-PANE case: one window, one tab, one pane, one file — the most
/// common shape there is — survives a quit and relaunch.
///
/// This is the GTK-side regression guard for the macOS bug fixed by
/// `fix/state-persistence-single-pane-bug`. A tab's open buffer paths have NO
/// persistence channel of their own: they ride ONLY inside `TabState::layout`'s
/// leaves. The macOS save side suppressed the layout by ARITY
/// (`guard tree.leaves().count > 1 else { return nil }`), so this exact shape
/// persisted its cwd and `sidebar_visible` and nothing else, and the file was not
/// reopened. The fix — on both sides — is a CONTENT predicate: suppress ONLY an
/// unnamed, non-preview lone leaf, and rebuild ANY saved layout with no arity guard
/// at all.
///
/// ## What it asserts
///
/// 1. **The regression assertion, and it is on the FILE not on pixels.** The
///    `state.json` RUN 1 wrote (frozen by the scenario into `$SOLO_STATE_COPY`
///    before run 2 can write over it) carries
///    `windows[0].tabs[0].layout == Some(Leaf { path, preview: false })` where
///    `path` is non-empty and its file NAME is exactly `szulu.md`. The name rather
///    than the whole path, deliberately: `/tmp` (or `TMPDIR`) may resolve through a
///    symlink on this host, so nvim's reported `abs_path` need not be string-equal
///    to any `PathBuf` this test could build — but an arity guard yields
///    `layout: null`, which this catches perfectly, and an empty-path leaf cannot
///    satisfy it either. No pixel can distinguish "the layout was saved and
///    restored" from "the layout was never saved but the file came back another
///    way", which is why this assertion exists at all.
/// 2. **Load-bearing, and exact:** the restored Neovim's `bufname('%')` tail is
///    exactly `szulu.md`.
/// 3. **Corroboration:** OCR contains `solozulu` and does NOT contain `soloalfa`.
/// 4. **State-dir isolation**, in four parts: this TEST process has no
///    `WOLF_STATE_DIR` of its own (without which the whole check would be
///    self-referential); the smoke's own state dir holds a non-empty `state.json`,
///    proving the app honoured the variable; NEITHER `$HOME`-derived fallback
///    directory ([`HOME_STATE_DIR_FALLBACKS`], under the temp `$HOME`
///    [`cage_command`] gives the app) holds a `state.json`, which is the positive
///    proof that `state_dir()` resolved the variable rather than `$HOME`; and the
///    developer's REAL `state.json` is byte-identical across the run — including
///    the normal case where it does not exist at all, which must still not exist
///    afterwards. The last two are complementary, not redundant: a leak lands
///    inside the temp tree when `$XDG_STATE_HOME` is unset and outside it when it
///    is set, and one arm catches each.
///
/// ## How it discriminates RESTORATION from COINCIDENCE
///
/// A fresh launch in this cwd shows an untitled buffer (`Untitled.md`), which is
/// the readback a broken build produces. The DECOY (`salfa.md`, alphabetically
/// first, never opened) kills the other plausible hypothesis: if the app were
/// reopening "the first file it found", the readback would be `salfa.md` and the
/// OCR would carry `soloalfa` — both of which the assertions catch. And assertion 1
/// cannot be satisfied by anything except the save path writing a single-leaf
/// layout.
///
/// ## Red-run reverts (TWO, both one-liners, and BOTH must be run)
///
/// They isolate the two halves of the original bug and must produce
/// DISTINGUISHABLE failures — which is exactly what makes assertion 1 worth having,
/// and why the scenario freezes run 1's `state.json` before run 2 can overwrite it.
///
/// | half | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | SAVE side | in `state_seam::persistable_layout`, replace `if pane_layout::is_empty_leaf(&node) { None } else { Some(node) }` with `if pane_layout::leaf_count(&node) < 2 { None } else { Some(node) }` | assertion 1 fails FIRST and names the arity guard (run 1's copy carries `layout: null`); assertions 2 and 3 also fail | **AS PREDICTED.** Assertion 1 panicked first, at the `layout: None` branch of the match, with the message that begins `run 1 persisted 'layout: None' for a single unsplit pane holding szulu.md`. The frozen copy carried the tab object with `cwd` and `sidebar_visible` and **no `layout` key at all** (serde skips `None`), which is precisely the on-disk shape of the original macOS bug. OCR of the retained run-2 PNG had zero hits for both `solozulu` and `soloalfa`, so assertions 2 and 3 would have failed as well. |
/// | RESTORE side | in `app.rs::build_ui`, wrap the `request_layout_rebuild` call in `if pane_layout::leaf_count(&layout) > 1 { … }` | assertion 1 **passes** (the save side is correct and the frozen copy proves it) while assertions 2 and 3 fail | **AS PREDICTED, and DISTINGUISHABLY.** Assertion 1 PASSED — the frozen copy carried `"layout": { "type": "leaf", "path": "…/szulu.md", "preview": false }` — and the panic moved to assertion 2: `assertion 'left == right' failed … left: "Untitled.md" right: "szulu.md"`. Two different assertions, two different line numbers, and the assertion-2 message's own self-diagnosis ("assertion 1 above already proved it WAS saved correctly … this points squarely at the restore side") named the injected defect correctly. OCR again had zero hits for `solozulu`. |
#[test]
fn single_pane_file_survives_relaunch_under_cage() {
    const SMOKE: &str = "single_pane_file_survives_relaunch_under_cage";

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // --- Assertion 4, first part: this TEST process must be un-overridden -----
    //
    // Asserted BEFORE anything runs, because `wolf_core::state_dir()` below resolves
    // `WOLF_STATE_DIR` ahead of everything else. Were it set here, the "developer's
    // real state file" this test guards would be whatever that variable points at —
    // very possibly the smoke's own temp dir — and the whole isolation check would
    // become self-referential and prove nothing.
    assert!(
        std::env::var_os("WOLF_STATE_DIR").is_none(),
        "{SMOKE}: WOLF_STATE_DIR is set in the TEST process. `wolf_core::state_dir()` resolves \
         that variable FIRST, so the \"developer's real state file\" this smoke proves is \
         untouched would be resolved against the override instead of against \
         $XDG_STATE_HOME/wolf — the isolation assertion would then compare the smoke's own \
         directory with itself and pass no matter what the app did. `cage_command` sets \
         WOLF_STATE_DIR on the CHILD only, which is the whole design; nothing may set it on the \
         test process."
    );

    let real_state_dir = state_dir().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::state_dir() resolved to None in the test process, so this smoke \
             cannot say WHICH file it is proving untouched and its isolation assertion would hold \
             vacuously. On Linux it is $XDG_STATE_HOME/wolf (falling back to the data dir), so a \
             None here means neither is resolvable — check $HOME."
        )
    });
    let real_state_file = real_state_dir.join(STATE_FILE_NAME);
    let real_before = state_file_fingerprint(&real_state_file);

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd shared by BOTH launches, holding the file to open and the
    // never-opened decoy.
    let cwd = make_temp_cwd();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (SOLO_FILE, SOLO_HEADING),
            (SOLO_DECOY_FILE, SOLO_DECOY_HEADING),
        ],
    );

    // The smoke's own state dir — the SAME sibling `cage_command` computes and hands
    // the app as WOLF_STATE_DIR, so the two runs share one `state.json`.
    let smoke_state = smoke_state_dir(&cwd);
    let smoke_state_file = smoke_state.join(STATE_FILE_NAME);

    // Where the scenario freezes RUN 1's save before run 2 can write over it. It is
    // a DIRECTORY holding a `state.json`, because `wolf_core::load_state` takes the
    // directory rather than the file. It lives under XDG_RUNTIME_DIR rather than in
    // the temp cwd so the app's own FileSearch never lists it in the sidebar.
    let run1_state_dir = runtime_dir.join("wolf_gnome_solo_run1_state");
    std::fs::create_dir_all(&run1_state_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the run-1 state snapshot dir {}: {e}",
            run1_state_dir.display()
        )
    });
    let run1_state_file = run1_state_dir.join(STATE_FILE_NAME);

    let readback = cwd.join(SOLO_READBACK_NAME);
    let out_png = runtime_dir.join("wolf_gnome_restore_solo_smoke.png");
    for path in [&readback, &out_png, &run1_state_file] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(RESTORE_SOLO_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("SOLO_FILE", SOLO_FILE)
        .env("SOLO_READBACK", &readback)
        .env("SOLO_STATE_SRC", &smoke_state_file)
        .env("SOLO_STATE_COPY", &run1_state_file)
        .env("RESTORE_SETTLE", RELAUNCH_RESTORE_SETTLE)
        .status()
        .expect("failed to spawn `cage` for the headless single-pane relaunch scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage single-pane relaunch scenario exited unsuccessfully ({status}). Exit \
         11: run 1's app died before its window mapped. Exit 12: run 1 was still alive after \
         `:qa`. Exit 13: run 1 exited NON-ZERO — it went away but not cleanly, which after this \
         unit implicates a panic in `flush_on_close` or a core.borrow() held across `wire_close`'s \
         core.borrow_mut().take(). Exit 14: run 2's app died before its window mapped. Exit 15: \
         the Neovim readback never appeared in run 2. Exit 16: run 1 wrote NO state.json at all at \
         {} — not the arity guard (which still writes the file, with a null layout) but something \
         upstream of it. Screenshot expected at {}.",
        smoke_state_file.display(),
        out_png.display()
    );

    assert_png_present(SMOKE, &out_png);

    // --- Assertion 1: RUN 1's saved layout is a single NAMED leaf --------------
    let saved = load_state(&run1_state_dir).unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::load_state could not parse the frozen copy of run 1's state at \
             {}. `load_state` is silently-ignore, so a None here means the file is absent, \
             unreadable, malformed, or carries an unsupported version — the scenario already \
             proved it was non-empty (it exits 16 otherwise), so suspect the shape.",
            run1_state_file.display()
        )
    });
    let window = saved.windows.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's state.json carries NO windows at all, so nothing was persisted. \
             `snapshot_state` writes exactly one window and one tab, so an empty `windows` means \
             `may_snapshot` refused every snapshot (nvim not Alive, no bound pane, or no reported \
             window) or `set_snapshot` was never called. File: {}",
            run1_state_file.display()
        )
    });
    let tab = window.tabs.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's persisted window carries NO tabs. `snapshot_state` writes exactly \
             one. File: {}",
            run1_state_file.display()
        )
    });
    match &tab.layout {
        Some(PaneNode::Leaf { path, preview }) => {
            assert!(
                !path.is_empty(),
                "{SMOKE}: run 1 persisted a lone leaf with an EMPTY path. The pane was showing \
                 {SOLO_FILE}, so this means `layout_snapshot` could not resolve the pane's buffer \
                 path: either the pane had no resolved grid, or `pane_status(grid).abs_path` came \
                 back empty because nvim had not reported its per-window buffer map yet. That is \
                 precisely what `may_snapshot`'s `reported_windows >= 1` gate exists to prevent. \
                 File: {}",
                run1_state_file.display()
            );
            assert!(
                !*preview,
                "{SMOKE}: run 1 persisted the lone leaf with `preview: true`. wolf-gnome has no \
                 Markdown preview, so `pane_layout::encode` must set the flag to false \
                 unconditionally; a true here would make the restore call `wolf_preview_file` \
                 instead of `:edit`. File: {}",
                run1_state_file.display()
            );
            let file_name = Path::new(path).file_name().and_then(OsStr::to_str);
            assert_eq!(
                file_name,
                Some(SOLO_FILE),
                "{SMOKE}: run 1 persisted a lone leaf naming {path:?}, whose file name is not \
                 {SOLO_FILE:?}. The comparison is on the file NAME rather than the whole path \
                 deliberately — /tmp may resolve through a symlink, so nvim's reported abs_path \
                 need not be string-equal to anything this test could construct — but the name \
                 itself is exact. `Untitled.md` here means the `:e` never landed before the quit. \
                 File: {}",
                run1_state_file.display()
            );
        }
        other => panic!(
            "{SMOKE}: run 1 persisted `layout: {other:?}` for a single unsplit pane holding \
             {SOLO_FILE}, not a single named leaf. **`None` IS THE SINGLE-PANE REGRESSION**: it \
             means the save side gated on ARITY (`leaf_count(..) > 1`) instead of on the CONTENT \
             predicate `pane_layout::is_empty_leaf`. A tab's open buffer paths have no persistence \
             channel of their own — they ride only inside `TabState::layout`'s leaves — so an \
             arity guard makes the single most common shape (one window, one tab, one pane, one \
             file) persist its cwd and nothing else, and the file is not reopened on relaunch. \
             That is the exact macOS bug this smoke exists to keep out of the GTK front-end; see \
             `state_seam::persistable_layout`. A SPLIT here would instead mean the run-1 scenario \
             somehow split the pane. File: {}",
            run1_state_file.display()
        ),
    }

    // --- Assertion 2: the restored Neovim's current buffer, exactly ------------
    let got = read_trimmed(&readback, SMOKE);
    assert_eq!(
        got,
        SOLO_READBACK_EXPECTED,
        "{SMOKE}: after the relaunch the restored Neovim's current buffer is {got:?}, not \
         {SOLO_READBACK_EXPECTED:?}. `Untitled.md` means the saved layout was never rebuilt — \
         assertion 1 above already proved it WAS saved correctly, so with that green this points \
         squarely at the restore side: an arity guard around `request_layout_rebuild`, a \
         `consume_pending_layout` that never fires, or a `pane_layout::decode` that rejected a \
         lone leaf. {SOLO_DECOY_FILE:?} would instead mean the app reopened the first file it \
         found rather than the saved one. PNG kept at {}.",
        out_png.display()
    );

    // --- Assertion 3: it actually PAINTED, and the decoy did not --------------
    let ocr_text = ocr_to_lowercase(&out_png);
    assert!(
        ocr_text.contains(SOLO_OCR_PRESENT),
        "{SMOKE}: OCR did not contain {SOLO_OCR_PRESENT:?} (the restored file's heading). The \
         readback above named {SOLO_FILE}, so NEOVIM has the right buffer — this says the GTK pane \
         did not paint it. PNG kept at {} for inspection. OCR output was:\n{ocr_text}",
        out_png.display()
    );
    assert!(
        !ocr_text.contains(SOLO_OCR_ABSENT),
        "{SMOKE}: OCR unexpectedly contained {SOLO_OCR_ABSENT:?} — the heading of the DECOY \
         {SOLO_DECOY_FILE}, which no scenario step ever opens. A hit means the app reopened a file \
         it chose itself (the alphabetically first one in the cwd) rather than the one the saved \
         layout names, which would make every other assertion here a coincidence. Note the decoy's \
         FILENAME does not contain the term, so a sidebar row cannot cause this. PNG kept at {}. \
         OCR output was:\n{ocr_text}",
        out_png.display()
    );

    // --- Assertion 4: state-dir isolation --------------------------------------
    //
    // THREE checks, and each one covers a leak the others cannot see.
    //
    // 1. The POSITIVE check immediately below — run 1's `state.json` exists and is
    //    non-empty at the smoke's own `WOLF_STATE_DIR` — is what pins the variable
    //    directly: drop it and the file is not there (the scenario exits 16 first).
    // 2. The `$HOME`-FALLBACK check is new, and it exists because `cage_command` now
    //    redirects the app's `$HOME` into the temp tree. That redirect closed a hole
    //    and opened a smaller one: with `$XDG_STATE_HOME` unset (this devShell), an
    //    app that ignored `WOLF_STATE_DIR` would resolve `state_dir()` to
    //    `$HOME/.local/state/wolf` — i.e. INSIDE the temp tree, where check 3 cannot
    //    see it. So we assert those exact paths carry no `state.json`. It is a
    //    positive proof that the variable was honoured, not merely that the real
    //    directory happened not to change, and the path is derived from the same
    //    `smoke_home` helper `cage_command` feeds to the child so the check and the
    //    environment cannot drift apart. On a developer machine WITHOUT that redirect
    //    this is exactly the leak that would land in their real
    //    `~/.local/state/wolf/state.json`.
    // 3. The real-state-file comparison stays exactly as it was. It is the arm that
    //    bites on a host which DOES export `$XDG_STATE_HOME` / `$XDG_DATA_HOME` — a
    //    leak there lands outside the temp tree, invisible to check 2 — so 2 and 3
    //    are complementary and between them cover both environments.
    let smoke_meta = std::fs::metadata(&smoke_state_file).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: expected the smoke's own state file {} to exist after the run, but: {e}. Its \
             absence means the app did not honour WOLF_STATE_DIR — which also means it wrote \
             somewhere else, most likely the developer's real ~/.local/state/wolf/.",
            smoke_state_file.display()
        )
    });
    assert!(
        smoke_meta.len() > 0,
        "{SMOKE}: the smoke's own state file {} exists but is empty",
        smoke_state_file.display()
    );

    // Check 2: nothing was written to the app's `$HOME`-derived state directories.
    // `smoke_home(&cwd)` is the SAME value `cage_command` hands the child as HOME, so
    // this cannot be looking at a directory the app never had.
    let app_home = smoke_home(&cwd);
    assert!(
        app_home.is_dir(),
        "{SMOKE}: the app's HOME {} is not a directory, so the fallback checks below would pass \
         vacuously — a non-existent path never holds a state.json. `make_temp_cwd` creates \
         `<unique>/project` and `cage_command` hands `<unique>` to the app as HOME, so this must \
         exist.",
        app_home.display()
    );
    for rel in HOME_STATE_DIR_FALLBACKS {
        let leaked = app_home.join(rel).join(STATE_FILE_NAME);
        assert!(
            !leaked.exists(),
            "{SMOKE}: found a state file at {} — inside the app's own $HOME. That path is what \
             `wolf_core::state::state_dir()` resolves to when WOLF_STATE_DIR is ABSENT (the \
             `dirs::state_dir()` / `dirs::data_dir()` chain, XDG default `$HOME/{rel}`), so its \
             existence means the app resolved its state directory from $HOME instead of from \
             WOLF_STATE_DIR. Every smoke is then writing application state somewhere this harness \
             does not control: the relaunch smokes stop sharing one state.json between their two \
             launches, and — because the $HOME redirect is a TEST fixture that does not exist on a \
             developer's machine — the same defect there writes to their real \
             ~/.local/state/wolf/state.json, reading their saved session at launch and clobbering \
             it at quit. `cage_command` is the single place WOLF_STATE_DIR is set and \
             `cage_is_only_spawned_through_the_isolating_helper` is what keeps it single; start \
             there, then check that `state_dir()` still resolves the variable FIRST.",
            leaked.display()
        );
    }

    let real_after = state_file_fingerprint(&real_state_file);
    assert_eq!(
        real_before,
        real_after,
        "{SMOKE}: the developer's REAL state file {} changed across this smoke run (before: \
         {real_before:?}, after: {real_after:?}; `None` means the file did not exist, which is \
         the normal state on a machine that has never run Wolf interactively and must stay that \
         way). This is a STATE-DIR ISOLATION FAILURE: the app under cage resolved \
         `wolf_core::state_dir()` to the real directory instead of the smoke's own, which means it \
         READ the developer's saved window size, working directory and pane layout at launch (so \
         this smoke did not start from a clean slate) and OVERWROTE them at quit with a throwaway \
         one-pane temp-cwd layout. `cage_command` is the single place WOLF_STATE_DIR is set, and \
         `cage_is_only_spawned_through_the_isolating_helper` is what keeps it single — start \
         there. The ONE benign cause: a real Wolf running concurrently in another terminal, which \
         writes that file on its own render wakes; close it and re-run before concluding anything \
         else.",
        real_state_file.display()
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
/// all — which is also why this smoke contributes no row to [`ASSERTED_OCR_TERMS`].
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
    // How far the LEFT pane's reported width may move while the INNER divider is
    // dragged. The inner drag leaves the left pane's ALLOCATION untouched, so
    // nothing resizes its grid, and `nvim_ui_try_resize_grid` was MEASURED not to
    // reflow Neovim's own layout — so the honest expectation is that it does not
    // move at all. The spare cell is slack against Neovim's bookkeeping rather than
    // against any known effect, and it costs nothing: a drag that hit the OUTER
    // handle instead would move this pane by the whole drag distance, six cells or
    // more.
    const UNDISTURBED_CELLS: i64 = 2;

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

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

    // Extents FIRST: a compositor-clamped injection would land somewhere other than
    // the point this smoke computed and make every message below point at the wrong
    // thing. The two drag END points are bounded by the seeds, so checking the
    // extremes covers every injected x.
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

    // The window numbers the bisection discovered, purely so every message below can
    // print them: nvim numbers windows by ITS layout, not by the order this
    // front-end draws the panes in.
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

    // --- Assertion 1: the baseline really is three roughly equal columns -------
    //
    // A PRECONDITION as much as an assertion: it is the equal-share assumption the
    // bisection seeds are modelled on.
    let widest = w0[0].max(w0[1]).max(w0[2]);
    let narrowest = w0[0].min(w0[1]).min(w0[2]);
    assert!(
        widest - narrowest <= BASELINE_TOLERANCE_CELLS,
        "{SMOKE}: the three fresh panes are {:?} cells wide, a spread of {} — more than the \
         {BASELINE_TOLERANCE_CELLS} cells a chain of paneds positioned by TRAVEL rather than by \
         width can account for. The bisection seeds are modelled on equal shares, so this also \
         says those seeds are aimed at the wrong pixels. PNG kept at {}.",
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

/// A dragged divider's weights reach `state.json` and come back on the next launch.
///
/// The round trip unit 4 built the channel for, driven for the first time by a
/// weight that is NOT an equal share — which is what makes "the saved weights were
/// applied" distinguishable from "the equal-share default was recomputed" at all.
///
/// Two app launches run inside ONE `cage` instance, with `wolf-pointer-inject` alive
/// across both; see [`DIVIDER_RELAUNCH_SCENARIO`] for the exit-code table and for
/// why it duplicates the pointer plumbing rather than reusing
/// [`POINTER_PREAMBLE`].
///
/// ## What it asserts
///
/// 1. **The save side, on the FILE.** The `state.json` RUN 1 wrote — frozen by the
///    scenario before run 2 can write over it — carries a `Vertical` split of two
///    named leaves whose weights sum to 1 and differ by at least
///    [`MIN_WEIGHT_GAP`]. Nothing before this unit could write such a vector:
///    `layout.rs`'s only writers produce `1/n` and `[0.5, 0.5]`.
/// 2. **The restore side, on run 2's PIXELS.** The restored panes' width ratio
///    matches run 1's post-drag ratio and sits at least [`MIN_RATIO_OFFSET`] off
///    centre. The second clause is what stops the first being satisfied by a
///    re-centred pair.
/// 3. **Corroboration.** OCR carries both headings, so two panes really did bind
///    grids and paint. The capture is read twice — `--psm 11` through
///    [`ocr_to_lowercase`] and `--psm 6` through [`ocr_uniform_block_to_lowercase`]
///    — and each term is accepted from either reading, for the reason under
///    "Reading a 15-cell column" below. Both terms are still demanded literally.
///
/// The two preconditions before them — the baseline pair is equal, and the drag
/// moved it — are what separate "the press missed the divider handle" from "the
/// weight was not written back". Without them a `[0.5, 0.5]` on disk would be
/// ambiguous between the two.
///
/// ## Measured geometry, and what the assertions have margin against
///
/// Every width here is `winwidth()` — the GRID width the front-end drives through
/// `resize_grid`. `nvim_win_get_width()` is the LAYOUT width and cannot see a
/// divider drag at all; see [`DIVIDER_RELAUNCH_SCENARIO`]'s `read_widths` for the
/// measurement that pins the difference.
///
/// | quantity | value |
/// | --- | --- |
/// | paned travel at the drag | `max_position` 1005 px, position 679 px after it, fraction 0.6756 |
/// | run 1 post-drag, MEASURED at the `resize_grid` calls | 30 and 15 cells, ratio 0.667 |
/// | run 2's restored panes, MEASURED in pixels | 679 and 349 px — the same 30 and 15 cells |
/// | run 1 baseline, DERIVED from half the same travel | about 22 and 23 cells |
/// | the narrow pane against the 14-character heading | 15 columns, one to spare |
///
/// So [`MIN_DRAG_CELLS`] (3) sits against a move of about 8 cells per pane,
/// [`RATIO_TOLERANCE`] (0.08) against a run-1-to-run-2 difference of 0, and
/// [`MIN_RATIO_OFFSET`] (0.08) against a ratio 0.167 off centre. The tightest of
/// the five is the heading's one spare column, which is what
/// [`DIVIDER_RELAUNCH_DRAG_PX`] is sized by: widen the drag and the narrow pane
/// soft-wraps `# DIVIDERBRAVO`, and assertion 3 reads two fragments.
///
/// ## Reading a 15-cell column
///
/// Fitting the heading is necessary and was not sufficient, and three separate
/// things had to be answered before assertion 3 could read the narrow pane. Two are
/// answered by FIXTURE and one by the READER; each was isolated by measurement on a
/// retained PNG rather than argued.
///
/// | what corrupted the reading | how it was isolated | what answers it |
/// | --- | --- | --- |
/// | the `~` end-of-buffer column above a lone heading — a sparse region `--psm 11` scales the whole pane from | four ablations on one PNG, one at a time; blanking the `~` column alone fixed it | [`DIVIDER_BODY_LINE_COUNT`] body lines, which fill the pane past `winheight` so no marker is drawn |
/// | the active-pane FOCUS RING's top edge — a long thin high-contrast line immediately above the heading | six ablations on one PNG; the top edge alone accounted for it | [`DIVIDER_RELAUNCH_SCENARIO`]'s closing `<C-w>h`, which moves the ring to the WIDE pane |
/// | a single misclassified GLYPH at 15 columns | the same pixels read through both segmentation modes, twice over | the second reader, [`ocr_uniform_block_to_lowercase`] |
///
/// The third is why this smoke, alone of the twenty-nine, reads its capture twice.
/// With the first two answered, on one retained PNG:
///
/// | command | verbatim output |
/// | --- | --- |
/// | `tesseract PNG stdout --psm 11` | `# DIVIDERBkAVO` — the `R` of `BRAVO` read as a `k` |
/// | `tesseract PNG stdout --psm 6` | `A 7 DIVIDERALFA # DIVIDERBRAVO` — both asserted terms correct |
///
/// It is deterministic rather than flaky: two independently
/// rendered screenshots gave byte-identical text in both modes, and `DIVIDERALFA` in
/// the wide pane reads correctly under both. The premise is stated where the reader
/// lives: a two-pane page whose narrow column is one heading over a block of body
/// text is a UNIFORM text block, which is what `--psm 6` assumes, and not the sparse
/// layout `--psm 11` assumes. `--psm 11` is untouched for the other smokes.
///
/// ## Red-run reverts (TWO, and BOTH must be run)
///
/// They isolate the two halves of the round trip and must produce DISTINGUISHABLE
/// failures — which is exactly why the scenario freezes run 1's save.
///
/// | half | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | SAVE side | delete the `notify::position` handler from `pane_view::install_paned_position` | assertion 1 fires FIRST, with `weights == [0.5, 0.5]` in the frozen copy; the two preconditions above it stay green, which is what says the drag DID move the divider and only the write-back is missing | AS PREDICTED, on assertion 1: "run 1 persisted weights [0.5, 0.5], whose gap is 0 — below the 0.15 that only a DRAGGED divider can produce." Both preconditions stayed green. `0 passed; 1 failed` in 38.32s |
/// | RESTORE side | in `install_paned_position`'s `max-position` handler, replace `tree.borrow().divider_fraction(&path, boundary)` with `Some(0.5)` | assertion 1 PASSES (the frozen copy carries the dragged weights) and assertion 2 fires, with run 2's panes back at equal widths | AS PREDICTED, on assertion 2 with assertion 1 GREEN: "run 1 quit with the left pane holding 0.6666666666666666 of the pair (30 and 15 cells) and run 2 restored 0.4888888888888889 (22 and 23 cells) — a change of 0.17777777777777776." `0 passed; 1 failed` in 38.36s |
#[test]
fn dragged_divider_weights_survive_relaunch_under_cage() {
    const SMOKE: &str = "dragged_divider_weights_survive_relaunch_under_cage";
    // Columns run 1 builds, and therefore the denominator the bisection seeds are
    // modelled from.
    const COLUMNS: i64 = 2;
    // How many panes each width readback must describe.
    const EXPECTED_PANES: usize = 2;

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd shared by BOTH launches — relaunching in the project you
    // saved from is the whole scenario.
    let cwd = make_temp_cwd();
    // The RIGHT fixture carries a body as well as its heading: the drag leaves that
    // pane 15 cells wide, and a heading alone above a bare `~` column is a sparse
    // region tesseract misclassifies. See `DIVIDER_BODY_LINE_COUNT` for the
    // ablations. The LEFT pane is the wide one and needs none.
    let right_fixture = divider_right_fixture_body();
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (DIVIDER_FILE_LEFT, DIVIDER_HEADING_LEFT),
            (DIVIDER_FILE_RIGHT, right_fixture.as_str()),
        ],
    );

    // The smoke's own state dir — the SAME one `cage_command` hands the app as
    // WOLF_STATE_DIR, so the two runs share one `state.json`.
    let smoke_state_file = smoke_state_dir(&cwd).join(STATE_FILE_NAME);

    // Where the scenario freezes RUN 1's save. A DIRECTORY holding a `state.json`,
    // because `wolf_core::load_state` takes the directory; under XDG_RUNTIME_DIR so
    // the app's own `FileSearch` never lists it in the sidebar.
    let run1_state_dir = runtime_dir.join("wolf_gnome_divider_run1_state");
    std::fs::create_dir_all(&run1_state_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the run-1 state snapshot dir {}: {e}",
            run1_state_dir.display()
        )
    });
    let run1_state_file = run1_state_dir.join(STATE_FILE_NAME);

    let w_base = cwd.join(DIVIDER_W0_NAME);
    let w_drag = cwd.join(DIVIDER_W1_NAME);
    let w_restored = cwd.join(DIVIDER_W2_NAME);
    // Outside the cwd: the bisection rewrites it once per probe and the sidebar
    // watches the cwd.
    let probe = runtime_dir.join("wolf_gnome_divider_relaunch_probe.txt");
    let out_png = runtime_dir.join("wolf_gnome_divider_relaunch_smoke.png");
    for path in [
        &w_base,
        &w_drag,
        &w_restored,
        &probe,
        &out_png,
        &run1_state_file,
    ] {
        clear_evidence(path, SMOKE);
    }

    let (seed_lo, seed_hi) = divider_seed_span(1, COLUMNS);

    let status = cage_command(DIVIDER_RELAUNCH_SCENARIO, &cwd)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .env("CMD_FIFO", cwd.join(POINTER_CMD_FIFO_NAME))
        .env("ACK_FILE", cwd.join(POINTER_ACK_NAME))
        .env("HELPER_ERR", pointer_err_path(&cwd))
        .env("READY_FILE", pointer_ready_path(&cwd))
        .env("PROBE_X", PROBE_X.to_string())
        .env("PROBE_Y", PROBE_Y.to_string())
        .env("BTN_LEFT", POINTER_BUTTON_LEFT)
        .env("PROBE_FILE", &probe)
        .env("DIVIDER_FILE_LEFT", DIVIDER_FILE_LEFT)
        .env("DIVIDER_FILE_RIGHT", DIVIDER_FILE_RIGHT)
        .env("DIV_W0", &w_base)
        .env("DIV_W1", &w_drag)
        .env("DIV_W2", &w_restored)
        .env("D_LO", seed_lo.to_string())
        .env("D_HI", seed_hi.to_string())
        .env("BISECT_STEPS", BISECT_STEPS.to_string())
        .env("RELAUNCH_DRAG_PX", DIVIDER_RELAUNCH_DRAG_PX.to_string())
        .env("DIV_STATE_SRC", &smoke_state_file)
        .env("DIV_STATE_COPY", &run1_state_file)
        .env("RESTORE_SETTLE", RELAUNCH_RESTORE_SETTLE)
        .status()
        .expect("failed to spawn `cage` for the headless divider relaunch scenario");

    let helper_err = std::fs::read_to_string(pointer_err_path(&cwd))
        .unwrap_or_else(|e| format!("<could not read the helper stderr: {e}>"));
    assert!(
        status.success(),
        "{SMOKE}: the cage divider relaunch scenario exited unsuccessfully ({status}). Read the \
         exit code FIRST; each names a different half of the harness.\n\
         Exit 11: run 1's app died before its window mapped — a LAUNCH failure.\n\
         Exit 12: `wolf-pointer-inject` never printed its `ready …` line (its own exit code is in \
         the stderr below: 3 = no `zwlr_virtual_pointer_manager_v1`, 4 = no `wl_seat`, 5 = no \
         `wl_output`, 6 = more than one output, 7 = zero extents).\n\
         Exit 13: a helper command was never acked within budget.\n\
         Exit 14: a readback file the captive Neovim was told to write never appeared — a KEYBOARD \
         failure, not a pointer one.\n\
         Exit 15: the helper died mid-scenario.\n\
         Exit 16: run 1 was still alive after `:qa`.\n\
         Exit 17: run 1 exited NON-ZERO — it went away, but not cleanly, which implicates \
         `flush_on_close`.\n\
         Exit 18: run 1 wrote NO state.json at all at {}, which is upstream of anything about \
         weights.\n\
         Exit 19: run 2's app died before its window mapped.\n\
         Exit 21: the bisection seeds ({seed_lo}, {seed_hi}) did not straddle the divider; the \
         scenario's stderr names the four candidate causes in order, and \
         `click_positions_the_cursor_under_cage` fails on its own if the pane origin moved.\n\
         Screenshot expected at {}.\n\
         wolf-pointer-inject stderr was:\n{helper_err}",
        smoke_state_file.display(),
        out_png.display()
    );

    assert_png_present(SMOKE, &out_png);

    // Extents FIRST, as in every pointer smoke. The drag's end point is bounded by
    // the HI seed plus the drag distance, so these three cover every injected x.
    let extents = read_output_extents(&cwd, SMOKE);
    for (label, x) in [
        ("the bisection LO seed", seed_lo),
        ("the bisection HI seed", seed_hi),
        (
            "the furthest RIGHT the drag can reach",
            seed_hi + DIVIDER_RELAUNCH_DRAG_PX,
        ),
    ] {
        assert_inside_extents(SMOKE, label, (x, i64::from(PROBE_Y)), extents);
    }

    let base = read_pane_widths(&w_base, SMOKE);
    let dragged = read_pane_widths(&w_drag, SMOKE);
    let restored = read_pane_widths(&w_restored, SMOKE);
    for (label, panes, path) in [
        ("run 1's baseline", &base, &w_base),
        ("run 1's post-drag", &dragged, &w_drag),
        ("run 2's restored", &restored, &w_restored),
    ] {
        assert_eq!(
            panes.len(),
            EXPECTED_PANES,
            "{SMOKE}: {label} readback describes {} pane(s), expected {EXPECTED_PANES}: {panes:?} \
             (from {}). One pane in RUN 2 means the split was not restored at all, which is the \
             topology failure `pane_layout_survives_relaunch_under_cage` owns — fix that first, \
             since weights cannot be asserted against a layout that did not come back.",
            panes.len(),
            path.display()
        );
    }

    let base_left = pane_width_for(&base, DIVIDER_FILE_LEFT, &w_base, SMOKE);
    let base_right = pane_width_for(&base, DIVIDER_FILE_RIGHT, &w_base, SMOKE);
    let drag_left = pane_width_for(&dragged, DIVIDER_FILE_LEFT, &w_drag, SMOKE);
    let drag_right = pane_width_for(&dragged, DIVIDER_FILE_RIGHT, &w_drag, SMOKE);
    let run2_left = pane_width_for(&restored, DIVIDER_FILE_LEFT, &w_restored, SMOKE);
    let run2_right = pane_width_for(&restored, DIVIDER_FILE_RIGHT, &w_restored, SMOKE);

    // --- Precondition A: run 1 started from two roughly equal panes ------------
    assert!(
        (base_left - base_right).abs() <= BASELINE_TOLERANCE_CELLS,
        "{SMOKE}: the fresh split gave panes of {base_left} and {base_right} cells, a spread of {} \
         — more than the {BASELINE_TOLERANCE_CELLS} cells a paned positioned by TRAVEL rather than \
         by width can account for. The bisection seed is modelled on equal shares, so this also \
         says the seed is aimed at the wrong pixels. PNG kept at {}.",
        (base_left - base_right).abs(),
        out_png.display()
    );

    // --- Precondition B: the drag moved the divider ----------------------------
    //
    // This is what makes assertion 1 unambiguous. Without it an equal-share pair on
    // disk could equally mean "the press missed the handle".
    assert!(
        drag_left > base_left + MIN_DRAG_CELLS && drag_right < base_right - MIN_DRAG_CELLS,
        "{SMOKE}: after dragging the divider {DIVIDER_RELAUNCH_DRAG_PX} px to the RIGHT the panes \
         are {drag_left} and {drag_right} cells, from {base_left} and {base_right} — less than the \
         {MIN_DRAG_CELLS} cells that counts as movement. Read this FIRST as: THE PRESS MISSED THE \
         DIVIDER HANDLE, a harness failure rather than a product one (the bisection converged on \
         the wrong pixel, or the handle is narrower than the residual {BISECT_STEPS} halvings \
         leave). Only after ruling that out does it mean `gtk::Paned` stopped moving on a drag. \
         Nothing below can be interpreted while this is red: an equal-share weight on disk would \
         then be perfectly correct. PNG kept at {}.",
        out_png.display()
    );

    // --- Assertion 1: RUN 1's saved weights are NOT equal shares ---------------
    let saved = load_state(&run1_state_dir).unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::load_state could not parse the frozen copy of run 1's state at {}. \
             `load_state` is silently-ignore, so a None means the file is absent, unreadable, \
             malformed or of an unsupported version — the scenario already proved it was non-empty \
             (it exits 18 otherwise), so suspect the shape.",
            run1_state_file.display()
        )
    });
    let window = saved.windows.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's state.json carries NO windows at all, so nothing was persisted. File: \
             {}",
            run1_state_file.display()
        )
    });
    let tab = window.tabs.first().unwrap_or_else(|| {
        panic!(
            "{SMOKE}: run 1's persisted window carries NO tabs. `snapshot_state` writes exactly \
             one. File: {}",
            run1_state_file.display()
        )
    });
    match &tab.layout {
        Some(PaneNode::Split {
            orientation,
            weights,
            children,
        }) => {
            assert_eq!(
                *orientation,
                PaneOrientation::Vertical,
                "{SMOKE}: run 1 persisted a {orientation:?} split for a `<C-w>v`, which divides \
                 WIDTH and must persist as Vertical. File: {}",
                run1_state_file.display()
            );
            let tails: Vec<Option<&str>> = children.iter().map(leaf_tail).collect();
            assert_eq!(
                tails,
                vec![Some(DIVIDER_FILE_LEFT), Some(DIVIDER_FILE_RIGHT)],
                "{SMOKE}: run 1 persisted leaves naming {tails:?}, not the two fixture files in \
                 left-to-right order. The weights below are parallel to THIS child order, so they \
                 cannot be read at all until it is right. A `None` entry is a nested split where a \
                 leaf belongs; the comparison is on the file NAME rather than the whole path \
                 because /tmp may resolve through a symlink. File: {}",
                run1_state_file.display()
            );
            assert_eq!(
                weights.len(),
                children.len(),
                "{SMOKE}: run 1 persisted {} weights for {} children. File: {}",
                weights.len(),
                children.len(),
                run1_state_file.display()
            );

            let sum = weights[0] + weights[1];
            assert!(
                (sum - 1.0).abs() < WEIGHT_SUM_EPS,
                "{SMOKE}: run 1's persisted weights {weights:?} sum to {sum}, not 1. \
                 `set_divider_fraction` renormalizes the whole split after every drag precisely so \
                 an unbounded number of them cannot drift the sum. File: {}",
                run1_state_file.display()
            );
            assert!(
                weights[0] - weights[1] >= MIN_WEIGHT_GAP,
                "{SMOKE}: run 1 persisted weights {weights:?}, whose gap is {} — below the \
                 {MIN_WEIGHT_GAP} that only a DRAGGED divider can produce. **`[0.5, 0.5]` HERE IS \
                 THE WHOLE POINT OF THIS SMOKE**: it means the drag never reached the layout's \
                 weights, so `layout_snapshot` serialized the equal shares `split` left behind. \
                 The precondition above already proved the divider DID move on screen \
                 ({base_left}/{base_right} cells before, {drag_left}/{drag_right} after), so this \
                 is the write-back — `install_paned_position`'s `notify::position` handler and \
                 `PaneTree::set_divider_fraction` — and not the gesture. The pure twin is \
                 `pane_layout::tests::encode_carries_a_dragged_weight_vector`. File: {}",
                weights[0] - weights[1],
                run1_state_file.display()
            );
        }
        other => panic!(
            "{SMOKE}: run 1 persisted `layout: {other:?}` for two side-by-side panes, not a \
             two-child Vertical split. A `None` means nothing was persisted at all (the snapshot \
             gate refused every wake) and a `Leaf` means the split never reached the pane tree — \
             either way this is the topology failure `pane_layout_survives_relaunch_under_cage` \
             owns, not a weight failure. File: {}",
            run1_state_file.display()
        ),
    }

    // --- Assertion 2: run 2 came back at the SAME ratio ------------------------
    let ratio1 = head_width_ratio(drag_left, drag_right, SMOKE, "run 1's post-drag panes");
    let ratio2 = head_width_ratio(run2_left, run2_right, SMOKE, "run 2's restored panes");
    assert!(
        (ratio2 - ratio1).abs() <= RATIO_TOLERANCE,
        "{SMOKE}: run 1 quit with the left pane holding {ratio1} of the pair ({drag_left} and \
         {drag_right} cells) and run 2 restored {ratio2} ({run2_left} and {run2_right} cells) — a \
         change of {}, beyond the {RATIO_TOLERANCE} cell quantization can explain. Assertion 1 \
         above already proved the right weights are ON DISK, so with that green this is the RESTORE \
         side: `pane_layout::decode` dropping the weights, or \
         `install_paned_position` re-deriving its position from something other than the live \
         tree's `divider_fraction`. PNG kept at {}.",
        (ratio2 - ratio1).abs(),
        out_png.display()
    );
    assert!(
        ratio2 >= 0.5 + MIN_RATIO_OFFSET,
        "{SMOKE}: the restored ratio is {ratio2}, less than {MIN_RATIO_OFFSET} clear of dead \
         centre — so the clause above (`the ratio is unchanged`) is VACUOUS: an equal split \
         satisfies it whenever the drag never took effect. This clause is what keeps the assertion \
         honest. It fires either because the drag was too small to be distinguishable from centre \
         (a FIXTURE problem — raise DIVIDER_RELAUNCH_DRAG_PX, never lower MIN_RATIO_OFFSET) or \
         because the restored panes really did come back equal. Run 2's panes were {run2_left} and \
         {run2_right} cells. PNG kept at {}.",
        out_png.display()
    );

    // --- Assertion 3: two panes really bound grids and PAINTED -----------------
    //
    // The page is read TWICE, in two page-segmentation modes, and each term is
    // accepted from either reading. Both readers see the same pixels of the same
    // capture, so accepting either does not weaken what is demanded — it is still
    // the literal term, both terms, and no page element other than the two headings
    // can produce one (see `ocr_uniform_block_to_lowercase` for the mechanical
    // guards, which cover the extra page text `--psm 6` reads).
    //
    // The two `contains` calls are kept SEPARATE rather than run against the two
    // readings joined: a concatenation would let a `divider` at the tail of one and a
    // `bravo` at the head of the other satisfy the assertion between them, and no
    // rendered pane would have produced that.
    let ocr_sparse = ocr_to_lowercase(&out_png);
    let ocr_block = ocr_uniform_block_to_lowercase(&out_png);
    for (term, const_name, side) in [
        (DIVIDER_OCR_LEFT, "DIVIDER_OCR_LEFT", "left"),
        (DIVIDER_OCR_RIGHT, "DIVIDER_OCR_RIGHT", "right"),
    ] {
        assert!(
            ocr_sparse.contains(term) || ocr_block.contains(term),
            "{SMOKE}: NEITHER OCR reading contained {const_name} = {term:?} (the {side} pane's \
             heading). The capture is read twice — `--psm 11` (sparse text, the mode every other \
             smoke uses) and `--psm 6` (one uniform block, this smoke's second reader) — and the \
             term is accepted from either, so this is a miss in BOTH. READ THE TWO DUMPS BELOW FOR \
             A MANGLED NEAR-MISS of the term rather than for its absence: every cause measured on \
             this page corrupts the heading instead of removing it, and the two readings \
             disagreeing about ONE GLYPH is the shape to expect. The narrow RIGHT pane is the one \
             to suspect either way.\n\
             FIRST, GLYPH CLASSIFICATION AT THE NARROW PANE'S COLUMN WIDTH. This is the live \
             hazard and it is what the second reader answers: at 15 cells `--psm 11` read \
             `# DIVIDERBkAVO` — the `R` of `BRAVO` as a `k` — on pixels where `--psm 6` read \
             `# DIVIDERBRAVO` correctly, deterministically, across two independently rendered \
             screenshots. If BOTH modes mangle it now, the levers are a THIRD segmentation mode \
             (`--psm 4`, columns of variable size) or a larger rendered size; NEVER a weaker \
             assertion. Do not relax this to a prefix, a fuzzy match, an edit distance or one of \
             the two terms.\n\
             SECOND, SOFT-WRAPPING — the one hazard a fixture still owns. \
             `runtime/nvim/init.lua` sets `wrap`/`linebreak`, so a pane narrower than the \
             14-character heading splits it into two fragments and no reading of the page can \
             contain the term. That IS what lowering DIVIDER_RELAUNCH_DRAG_PX fixes. Run 2's panes \
             were {run2_left} and {run2_right} cells, against a heading that needs 14.\n\
             THIRD, and only once those are ruled out: a genuinely unbound or unpainted pane. The \
             width readback above already showed Neovim has both windows, so this is the LAST \
             reading of a red assertion 3, not the first.\n\
             Two further causes are ANSWERED BY FIXTURE, and the retained PNG is where to confirm \
             they are still answered rather than re-derive them: the `~` end-of-buffer column \
             (DIVIDER_BODY_LINE_COUNT's body lines fill the pane past winheight, so no marker is \
             drawn) and the active-pane FOCUS RING (DIVIDER_RELAUNCH_SCENARIO's closing `<C-w>h` \
             puts it around the WIDE pane; the ablation table is beside that move). Do NOT lower \
             DIVIDER_RELAUNCH_DRAG_PX for either: it moves neither, and it narrows the weight gap \
             assertion 1 rests on.\n\
             PNG kept at {} for inspection.\n\
             `--psm 11` output was:\n\
             {ocr_sparse}\n\
             `--psm 6` output was:\n\
             {ocr_block}",
            out_png.display()
        );
    }
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
///    fixture is named that (enforced by [`ASSERTED_OCR_TERMS`] plus the three
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

    // Serialize against the other cage tests (shared Wayland socket); recover from
    // poisoning so a failure in a sibling test does not cascade here.
    let _cage_guard = CAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp base, then the two NESTED directories the abbreviation needs.
    // `smoke_state_dir` will make `<home>/titlezulu-state` a SIBLING of the project,
    // so the app's own FileSearch (rooted at the project) never lists it.
    //
    // `make_temp_cwd` returns `<unique>/project`, and this smoke uses that only as a
    // BASE — it never launches there — so the extra fixed component is inert here:
    // the app's cwd is `<base>/titlehome-parent/titlezulu` and its HOME is
    // `<base>/titlehome-parent`, exactly as before.
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

    // HOME is the PARENT of the working directory, which is what makes
    // `display_title` abbreviate. It also keeps every remaining $HOME-derived path
    // inside the temp tree; the state dir is not one of them, since `cage_command`
    // pins it with WOLF_STATE_DIR (resolved ahead of anything $HOME-derived).
    //
    // This `.env("HOME", ...)` DELIBERATELY OVERRIDES `cage_command`'s default, and
    // is kept explicit even though the two now agree: the helper's default is
    // `smoke_home(cwd)` == the cwd's parent, and this smoke's cwd IS `home/titlezulu`,
    // so both resolve to `home`. The assertions below are written against THIS value
    // (`~/titlezulu`, with `titlehome-parent` abbreviated away), so it is stated here
    // rather than inherited — a later change to the helper's rule must not silently
    // retarget this smoke's subject. Chained after the helper, so it wins regardless.
    let status = cage_command(TITLE_SCENARIO, &project)
        .env("HOME", &home)
        .env("WOLF_BIN", bin)
        .env("OUT_PNG", &out_png)
        .status()
        .expect("failed to spawn `cage` for the headless window-title smoke scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage window-title scenario exited unsuccessfully ({status}); the app may \
         have failed to launch (exit 11 means it died before its window mapped) or grim failed. \
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

/// Runs `tesseract <png> stdout --psm 11` and returns its lowercased text output.
/// (`--psm 11` — "sparse text" — is needed because the default page-segmentation
/// mode returns empty on the sparse, light-on-dark editor grid.) Panics with the
/// captured stderr if tesseract fails to run or exits non-zero.
///
/// This is the reader every smoke but one uses, and its mode is deliberately fixed
/// rather than a parameter. The one exception reads the page a SECOND time through
/// [`ocr_uniform_block_to_lowercase`]; that function's doc says why, and nothing
/// about this one changes for it.
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

/// Runs `tesseract <png> stdout --psm 6` and returns its lowercased text output.
///
/// A SECOND reader, used by
/// [`dragged_divider_weights_survive_relaunch_under_cage`] and by nothing else.
/// Every other smoke reads through [`ocr_to_lowercase`] at `--psm 11`, which this
/// leaves entirely untouched: those assertions are calibrated against that mode's
/// output on their own pages, and re-segmenting them is not what this reader is for.
///
/// ## Why this smoke's page needs a different segmentation mode
///
/// `--psm 11` is "sparse text": it locates text in no particular order and estimates
/// a scale per region it decides is sparse. That is the right assumption for the
/// pages the other smokes capture, and the WRONG one for this one. After the drag,
/// this smoke's page is two columns of editor text, the narrow one being a single
/// heading over a solid block of body lines — a uniform block of text at one scale,
/// which is precisely what `--psm 6` ("a single uniform block") assumes. The 15-cell
/// column is too narrow for `--psm 11`'s per-region scale estimate to resolve, and
/// the failure it produces is not a missing word but a misclassified glyph.
///
/// MEASURED, both modes over the same retained PNG:
///
/// | mode | verbatim output |
/// | --- | --- |
/// | `tesseract PNG stdout --psm 11` | `# DIVIDERBkAVO` — the `R` of `BRAVO` read as a `k`; `DIVIDERALFA` in the wide pane correct |
/// | `tesseract PNG stdout --psm 6` | `A 7 DIVIDERALFA # DIVIDERBRAVO` — BOTH asserted terms correct |
///
/// Deterministic, not flaky: two independently rendered screenshots produced
/// byte-identical OCR text in both modes. `DIVIDERALFA` reads correctly under both,
/// so what is being worked around is a property of the narrow COLUMN, not of the
/// font, the theme or the term.
///
/// ## Red-run revert (one line)
///
/// This reader is LOAD-BEARING, not belt-and-braces, and the revert below is the
/// measurement that settles it — meet it here rather than re-deriving it, because
/// the deletion it argues against is the one that turns the smoke red.
///
/// | assertion | revert | expected red symptom | measured |
/// | --- | --- | --- | --- |
/// | assertion 3 of [`dragged_divider_weights_survive_relaunch_under_cage`] | drop this reader from it: `ocr_sparse.contains(term)` in place of `ocr_sparse.contains(term) || ocr_block.contains(term)` | the loop's [`DIVIDER_OCR_RIGHT`] iteration fires on the narrow pane's heading, while its [`DIVIDER_OCR_LEFT`] iteration stays green | AS PREDICTED, and against a FRESH render under `cage` rather than the retained PNG: `thread 'dragged_divider_weights_survive_relaunch_under_cage' (2140204) panicked at crates/wolf-gnome/tests/smoke_headless.rs:11700:9:` / `dragged_divider_weights_survive_relaunch_under_cage: NEITHER OCR reading contained DIVIDER_OCR_RIGHT = "dividerbravo" (the right pane's heading).`, then `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 28 filtered out; finished in 39.92s`. It fires on `dividerbravo` ONLY: the same run's `divideralfa` iteration passes, so `--psm 11` resolves the wide pane's heading and what the revert exposes is the narrow COLUMN rather than the sparse reader as such. Reproducing on a render of its own is also what says the misread in the table above is a property of the settled arrangement, not an artifact of one retained capture. |
///
/// ## What the wider mode does not open up
///
/// `--psm 6` reads MORE of the page than `--psm 11` does — the header-bar title and
/// every tree-sidebar row included — so the premise that only the right pane's
/// heading can supply [`DIVIDER_OCR_RIGHT`] has to hold against the whole page, not
/// just against the editor grid. It does, and mechanically rather than by
/// convention: [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`] covers the
/// filler names, [`temp_cwd_name_cannot_satisfy_any_smoke_assertion`] the two path
/// components the title bar draws, and
/// [`restore_fixture_names_cannot_satisfy_their_own_assertions`] both
/// [`RELAUNCH_FIXTURE_NAMES`] — which is where `dvright.md`, deliberately unlike
/// `dividerbravo`, is registered — and every body line
/// [`divider_right_fixture_body`] writes. So no sidebar row, no `:e` echo of a
/// filename and no title segment can satisfy the assertion however much of the page
/// is read.
///
/// ## Why it duplicates [`ocr_to_lowercase`] instead of parameterizing it
///
/// So that the reader every OTHER OCR-asserting smoke depends on stays a fixed,
/// unparameterized command. A mode argument is a value a later change can pass
/// wrongly from a call site that looks unrelated to OCR, and the symptom would be a
/// distant smoke failing on a term it renders perfectly. This file already
/// duplicates for that class of reason — see [`DIVIDER_RELAUNCH_SCENARIO`] on the
/// pointer plumbing.
///
/// Panics with the captured stderr if tesseract fails to run or exits non-zero.
fn ocr_uniform_block_to_lowercase(png: &Path) -> String {
    let output = Command::new("tesseract")
        .arg(png)
        .arg("stdout")
        .arg("--psm")
        .arg("6")
        .output()
        .expect("failed to spawn `tesseract` for OCR in --psm 6");
    assert!(
        output.status.success(),
        "tesseract failed on {} in --psm 6 ({}): {}. `--psm 11` is the mode every other smoke \
         uses and it is unaffected by this; a failure HERE is tesseract rejecting the mode \
         argument or the image, not a rendering problem.",
        png.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_lowercase()
}
